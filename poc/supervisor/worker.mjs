// Guest runner: one Worker per process. Owns the instance and its (unshared)
// linear memory; talks to the kernel through the SAB mailbox. Also implements
// the host half of the lazy fork: save [0,sp) at rfork, restore + throw at
// the child's exec/exits so the guard's catch_all resumes the parent (§5.2).
import { workerData, parentPort } from "node:worker_threads";

const { mb, tx } = workerData;
const ST = { IDLE: 0, REQ: 1, DONE: 2 };
const R_FORKRESUME = -1000, R_EXECSELF = -1001;
const T_STR = new Set([2, 3, 7, 8, 14, 42]);   // traps whose a0 is a path/string
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

function sys(trap, a0, a1, a2, a3, a4) {
  // string/buffer arguments are copied into the transfer SAB
  if (T_STR.has(trap)) {
    let o = cstr(a0, 0);
    if (trap === 7) {                       // exec: path then argv strings
      for (let pp = a1; ; pp += 4) {
        const sp = new DataView(memory.buffer).getUint32(pp, true);
        if (!sp) break;
        o = cstr(sp, o);
      }
    }
    if (trap === 2) o = cstr(a1, o);        // bind: name then old
    tx[o] = 0;
  }
  if (trap === 51) tx.set(memU8.subarray(a1, a1 + Math.min(a2, tx.length)));  // pwrite buf
  mb[1] = trap; mb[2] = a0; mb[3] = a1; mb[4] = a2; mb[5] = a3; mb[6] = a4;
  Atomics.store(mb, 0, ST.REQ);
  parentPort.postMessage({ t: "sc" });
  Atomics.wait(mb, 0, ST.REQ);                       // Workers may block
  Atomics.store(mb, 0, ST.IDLE);
  const ret = mb[8], aux = mb[9];
  if (ret === R_FORKRESUME) {                        // child left; resume parent
    memU8.set(savedStack); savedStack = null;
    forkPid = aux;
    throw new ForkResume(aux);
  }
  if (ret === R_EXECSELF) throw new ExecReplace();
  if (ret > 0) {                                     // copy-outs, per trap
    if (trap === 50) memU8.set(tx.subarray(0, ret), a1);                     // pread -> buf
    else if (trap === 42 || trap === 43) memU8.set(tx.subarray(0, ret), a1); // stat/fstat -> edir
    else if (trap === 41 || trap === 47 || trap === 200)
      memU8.set(tx.subarray(0, ret + 1), a0);        // errstr/await/args -> buf (NUL incl.)
  }
  return ret;
}
function cstr(ptr, o) {
  let end = memU8.indexOf(0, ptr);
  tx.set(memU8.subarray(ptr, end), o);
  o += end - ptr; tx[o++] = 0;
  return o;
}

function raw0(flags, sp, fn, arg) {                   // the guard's raw rfork
  const ret = sys(19, flags, sp, 1, 0, 0);            // a2=1: guarded call
  if (ret === 0 && mb[9] > 0) {
    savedStack = memU8.slice(0, sp);                  // the child's scribble region
    curInst.exports.__forkshim(fn, arg);              // run the child INSIDE the guard's
    throw new Error("forkshim returned");             // extent; exec/exits unwind out
  }
  return ret;
}

async function run(modBytes, guardMod) {
  memory = new WebAssembly.Memory({ initial: 64, maximum: 512 });
  memU8 = new Uint8Array(memory.buffer);
  const guard = new WebAssembly.Instance(guardMod, { env: { raw0, forkpd: () => forkPid } });
  const inst = new WebAssembly.Instance(new WebAssembly.Module(modBytes), {
    env: { memory, sys },
    guard: { rfork: guard.exports.rfork },
  });
  curInst = inst;
  memU8 = new Uint8Array(memory.buffer);              // in case of growth on instantiation
  inst.exports._start();
}

const guardMod = new WebAssembly.Module(guardBytes());
let pending = Promise.resolve();
function start(modBytes) {
  pending = run(modBytes, guardMod).catch((e) => {
    if (e instanceof ForkResume) return;              // parent resumed and _start returned? no: guard caught inside — reaching here means unwound past _start: only if guard missing
    if (e instanceof ExecReplace) return;             // replaced; 'load' message follows
    throw e;
  });
}
parentPort.on("message", (m) => { if (m.t === "load") start(m.modBytes); });
start(workerData.modBytes);
