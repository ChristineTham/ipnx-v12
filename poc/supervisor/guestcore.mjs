// Guest runner core: one Worker per process, platform-neutral. Owns the
// instance and its (unshared) linear memory; talks to the kernel through the
// SAB mailbox. Also implements the host half of the lazy fork: save [0,sp) at
// rfork, restore + throw at the child's exec/exits so the guard's catch_all
// resumes the parent (§5.2). The thin shims (worker.mjs for Node,
// ../browser/worker.mjs for the page) supply the message port.
let port, mb, tx;
const ST = { IDLE: 0, REQ: 1, DONE: 2 };
const R_FORKRESUME = -1000, R_EXECSELF = -1001, R_RETIRE = -2000;
const T_STR = new Set([2, 3, 7, 8, 14, 22, 25, 41, 42, 44, 60, 61, 62]);   // traps whose a0 is a path/string
class ForkResume { constructor(pid) { this.pid = pid; } }
class ExecReplace {}

// The guard: the only hand-written wasm in the system. One function,
//   (func (export "rfork") (param flags sp) (result i32)
//     block $ret(i32) block $ca try_table (catch_all $ca)
//       local.get 0; local.get 1; call $raw; br $ret
//     end unreachable end call $forkpid end)
// try_table/catch_all is the standardised EH; measured working (RESEARCH §5.2).
function guardBytes() {
  const leb = (n) => { const o = []; do { let b = n & 127; n >>= 7; if (n) b |= 128; o.push(b); } while (n); return o; };
  const sec = (id, body) => [id, ...leb(body.length), ...body];
  const str = (x) => [x.length, ...[...x].map((c) => c.charCodeAt(0))];
  const types = sec(1, [2, 0x60, 4, 0x7f, 0x7f, 0x7f, 0x7f, 1, 0x7f, 0x60, 0, 1, 0x7f]);
  const imports = sec(2, [2, ...str("env"), ...str("raw0"), 0, 0,
                             ...str("env"), ...str("forkpd"), 0, 1]);
  const funcs = sec(3, [1, 0]);
  const exports = sec(7, [1, ...str("rfork"), 0, 2]);
  const body = [0,                          // no locals
    0x02, 0x7f,                 // block (result i32)
    0x02, 0x40,                 //   block (void)          <- catch_all target
    0x1f, 0x40, 1, 0x02, 0x00,  //     try_table (catch_all -> label 0)
    0x20, 0x00, 0x20, 0x01,     //       local.get 0..3
    0x20, 0x02, 0x20, 0x03,     //       (flags, sp, fn, arg)
    0x10, 0x00,                 //       call $raw0
    0x0c, 0x02,                 //       br 2 (-> outer block, carrying i32)
    0x0b,                       //     end try_table
    0x00,                       //     unreachable
    0x0b,                       //   end
    0x10, 0x01,                 //   call $forkpd
    0x0b,                       // end
    0x0b];                      // end func
  const code = sec(10, [1, ...leb(body.length), ...body]);
  return new Uint8Array([0, 0x61, 0x73, 0x6d, 1, 0, 0, 0,
    ...types, ...imports, ...funcs, ...exports, ...code]);
}

let memory, memU8, curInst = null, savedStack = null, forkPid = 0;
// memory.grow DETACHES the old buffer; never trust a cached view (found the
// hard way: rc's heap crossing the initial 4MB detached memU8 mid-fork)
function m8() {
  if (memU8 === undefined || memU8.buffer !== memory.buffer)
    memU8 = new Uint8Array(memory.buffer);
  return memU8;
}
let pendingFork = null, rewinding = false, rewindReturn = 0;

// Bare dual-return rfork, via asyncify (RESEARCH §5.2): unwind the whole
// stack into the guest's buffer, snapshot linear memory, rewind twice.
function forka(flags, databuf) {
  if (rewinding) {                                   // the second return
    curInst.exports.asyncify_stop_rewind();
    rewinding = false;
    return rewindReturn;
  }
  const pid = sys(19, flags, 0, 2, 0, 0);            // a2=2: asyncify fork
  if (pid < 0) return -1;
  pendingFork = { pid, databuf };
  curInst.exports.asyncify_start_unwind(databuf);
  return 0;                                          // dummy; we are unwinding
}

let postSc = null;
function sys(trap, a0, a1, a2, a3, a4) {
  // string/buffer arguments are copied into the transfer SAB
  if (T_STR.has(trap)) {
    let o = cstr(a0, 0);
    if (trap === 7) {                       // exec: path then argc counted argv strings
      let argc = 0;
      for (let pp = a1; ; pp += 4) {
        const sp = new DataView(memory.buffer).getUint32(pp, true);
        if (!sp) break;
        o = cstr(sp, o);
        argc++;
      }
      a2 = argc;                            // empty argv strings must survive
    }
    if (trap === 2 || trap === 60 || trap === 61)
      o = cstr(a1, o);                      // bind/link/symlink: two strings
    if (trap === 44) tx.set(m8().subarray(a1, a1 + a2), o);  // wstat: the raw record
    else tx[o] = 0;
  }
  if (trap === 45) tx.set(m8().subarray(a1, a1 + a2));    // fwstat: raw record
  if (trap === 46) {                        // mount: old at a2, aname at a4
    let o = cstr(a2, 0);
    o = cstr(a4, o);
    tx[o] = 0;
  }
  if (trap === 51) tx.set(m8().subarray(a1, a1 + Math.min(a2, tx.length)));  // pwrite buf
  mb[1] = trap; mb[2] = a0; mb[3] = a1; mb[4] = a2; mb[5] = a3; mb[6] = a4;
  Atomics.store(mb, 0, ST.REQ);
  port.post({ t: "sc" });
  Atomics.wait(mb, 0, ST.REQ);                       // Workers may block
  Atomics.store(mb, 0, ST.IDLE);
  const ret = mb[8], aux = mb[9];
  if (trap === 19 && typeof process !== 'undefined' && process.env.KDBG)
    console.error(`[sys19 ret=${ret} aux=${aux}]`);
  if (ret === R_FORKRESUME) {                        // child left; resume parent
    m8().set(savedStack); savedStack = null;
    forkPid = aux;
    throw new ForkResume(aux);
  }
  if (ret === R_EXECSELF) throw new ExecReplace();
  if (ret === R_RETIRE) throw new ExecReplace();       // park; the next init reuses us
  if (ret > 0) {                                     // copy-outs, per trap
    if (trap === 50) m8().set(tx.subarray(0, ret), a1);                     // pread -> buf
    else if (trap === 42 || trap === 43) m8().set(tx.subarray(0, ret), a1); // stat/fstat -> edir
    else if (trap === 41 || trap === 47 || trap === 200)
      m8().set(tx.subarray(0, ret + 1), a0);        // errstr/await/args -> buf (NUL incl.)
    else if (trap === 62 || trap === 23) m8().set(tx.subarray(0, ret + 1), a1);  // readlink/fd2path
    else if (trap === 53) m8().set(tx.subarray(0, 8), a0);                  // nsec -> vlong*
  }
  if (trap === 21 && ret === 0) m8().set(tx.subarray(0, 8), a0);            // pipe -> fd[2]
  return ret;
}
function cstr(ptr, o) {
  let end = m8().indexOf(0, ptr);
  tx.set(m8().subarray(ptr, end), o);
  o += end - ptr; tx[o++] = 0;
  return o;
}

function raw0(flags, sp, fn, arg) {                   // the guard's raw rfork
  const ret = sys(19, flags, sp, 1, 0, 0);            // a2=1: guarded call
  if (ret === 0 && mb[9] > 0) {
    savedStack = m8().slice(0, sp);                  // the child's scribble region
    curInst.exports.__forkshim(fn, arg);              // run the child INSIDE the guard's
    throw new Error("forkshim returned");             // extent; exec/exits unwind out
  }
  return ret;
}

function callStart() {
  for (;;) {
    curInst.exports._start();
    if (!pendingFork) break;                         // only an unwind returns here
    curInst.exports.asyncify_stop_unwind();
    const { pid, databuf } = pendingFork;
    pendingFork = null;
    const snap = m8().slice().buffer;               // the child's whole memory
    port.post({ t: "asyfork", pid, snap, dataPtr: databuf }, [snap]);
    rewindReturn = pid;                              // this side is the parent
    rewinding = true;
    curInst.exports.asyncify_start_rewind(databuf);
  }
}

async function run(mod, guardMod, asy) {
  const initial = asy ? Math.max(32, asy.snap.byteLength >>> 16) : 32;
  const wantMax = Math.max(maxPages, initial + 32);
  // Reuse the previous guest's Memory when it fits (workers are pooled and a
  // fresh Memory per guest exhausted Chrome's wasm budget — measured); the
  // old contents are zeroed so BSS assumptions hold.
  const cur = memory ? memory.buffer.byteLength >>> 16 : 0;
  if (memory && cur >= initial && (memory.maxPages ?? 0) >= wantMax) {
    new Uint8Array(memory.buffer).fill(0);
  } else {
    memory = new WebAssembly.Memory({ initial, maximum: wantMax });
    memory.maxPages = wantMax;
  }
  memU8 = undefined;
  const guard = new WebAssembly.Instance(guardMod, { env: { raw0, forkpd: () => forkPid } });
  const inst = new WebAssembly.Instance(mod, {
    env: { memory, sys, forka },
    guard: { rfork: guard.exports.rfork },
  });
  curInst = inst;
  if (asy) {                                          // a freshly forked child: rewind
    m8().set(new Uint8Array(asy.snap));
    rewindReturn = 0;
    rewinding = true;
    inst.exports.asyncify_start_rewind(asy.dataPtr);
  }
  callStart();
}

const guardMod = new WebAssembly.Module(guardBytes());
let pending = Promise.resolve();
function start(mod, asy) {
  pendingFork = null; rewinding = false;
  pending = run(mod, guardMod, asy).catch((e) => {
    if (e instanceof ForkResume) return;              // parent resumed and _start returned? no: guard caught inside — reaching here means unwound past _start: only if guard missing
    if (e instanceof ExecReplace) return;             // replaced; 'load' message follows
    throw e;
  });
}

let maxPages = 80;
export function startGuest(thePort) {
  port = thePort;
  port.onMessage((m) => {
    if (m.t === "init") {
      mb = m.mb; tx = m.tx;
      maxPages = m.maxPages ?? 80;
      start(m.mod, m.snap ? { snap: m.snap, dataPtr: m.dataPtr } : null);
    }
    if (m.t === "load") start(m.mod);
  });
}
export const _memoryReuse = { get: () => memory, set: (v) => { memory = v; } };
