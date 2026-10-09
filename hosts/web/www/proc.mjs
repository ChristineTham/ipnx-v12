// A process: one worker, running one image.
//
// The process's side of the browser machine; the kernel's side is
// hosts/web/src/machine.rs. The image's calls are its imports from module
// "sys" (the stubs are userspace/sys/src/libc/wasm/sys.c), and each is a
// message in the mailbox and a wait until the kernel answers — the trap.
// setjmp, longjmp and fork are asyncify's, as on the terminal machine
// (hosts/ipnx/src/machine.rs: `run`, `unwind`, `rewound`; RESEARCH §16.12),
// and this file follows that one function by function. A WASI program's
// calls are WASI's, which wasi.mjs makes into these.

import { CALLS, ev, rep, mailbox, tell } from './mailbox.mjs';
import { iswasi, runwasi } from './wasi.mjs';

const node = typeof process === 'object' && !!process.versions?.node;
const port = node ? (await import('node:worker_threads')).parentPort : self;
const post = (m) => port.postMessage(m);
if (node) port.on('message', begin);
else self.onmessage = (e) => begin(e.data);

// the one call answering a vlong
const RET64 = new Set(['seek']);

// The Tos (sys/include/tos.h), on wasm32; ERRMAX (libc.h); TSTKSIZ, the
// most exec's arguments may take (pc/mem.h:53); a jmp_buf's words
// (wasm/include/u.h).
const TOSSIZE = 72, TOSPID = 48, ERRMAX = 128, TSTKSIZ = 100 * 4096;
const JMPBUFSP = 0, JMPBUFPC = 1;

// A fault: its message is the note. And noted(NCONT), leaving a handler.
class Trap extends Error {}
class Noted extends Error {}
// The process is gone (mailbox.mjs, `gone`): stop, and say nothing.
class Gone extends Error {}

let pid, mem, X, box;
const st = {
  op: null,        // why the stack is unwinding, and the stack pointer then
  rewind: null,    // what the call being wound back into answers
  jmps: new Map(), // each setjmp's stack, by its jmp_buf's address
  entry: null,     // what the process was entered through
  tos: 0,          // where its Tos is
  stacktop: 0,     // the end of its stack region, [0, stacktop)
  regionLow: 0,    // where it left the region for a stack outside it
};

const u8 = () => new Uint8Array(mem.buffer);
const dv = () => new DataView(mem.buffer);

// For a page that starts its workers one at a time (WebKit, platforms.md).
post({ ready: true });

function begin(m) {
  if (!m.start && !m.child) return;
  // its memory comes down a channel of its own (kernel.mjs, `carry`)
  if (node) m.port.once('message', (x) => go(m, x.mem));
  else m.port.onmessage = (e) => go(m, e.data.mem);
}

function go(m, memory) {
  m.port.close();
  pid = m.pid;
  mem = memory;
  box = mailbox(m.box);
  try {
    // A WASI program: its own memory, and its calls made through this one
    if (m.start && iswasi(m.module)) {
      runwasi(m.module, mem, m.args, call);
      tell(box, ev.EXITED);
      return;
    }
    const imports = { sys: sys() };
    for (const i of WebAssembly.Module.imports(m.module)) {
      if (i.kind === 'memory') (imports[i.module] ??= {})[i.name] = mem;
    }
    X = new WebAssembly.Instance(m.module, imports).exports;
    if (m.start) {
      const tos = settos();
      const [argc, argv] = place(tos, m.args);
      run({ kind: 'start', argc, argv, tos });
    } else {
      // A child: its parent's stack, wound back from the frames as they
      // unwound, in the memory the kernel made it; its rfork answers 0.
      const s = m.state;
      st.jmps = s.jmps;
      st.regionLow = s.regionLow;
      st.tos = s.tos;
      st.stacktop = s.top;
      rewindinto(s.frames, s.sp, 0);
      run(s.entry);
    }
    tell(box, ev.EXITED);
  } catch (e) {
    if (e instanceof Gone) return;
    tell(box, ev.FAULT, e instanceof Trap ? e.message : `sys: trap: ${e?.message ?? e}`);
  }
}

// Say something, and wait for the answer.
function say(kind) {
  const { I, W } = box;
  I[1] = kind;
  W[6] = BigInt(low());
  Atomics.store(I, 0, 1);
  Atomics.notify(I, 0);
  for (;;) {
    for (let s; (s = Atomics.load(I, 0)) !== 2; ) {
      if (Atomics.load(I, 5)) throw new Gone();
      Atomics.wait(I, 0, s);
    }
    if (I[2] !== rep.PC) break;
    // the kernel asks where the call was made from, and still owes it
    W[7] = BigInt(userpc());
    I[1] = ev.PC;
    Atomics.store(I, 0, 1);
    Atomics.notify(I, 0);
  }
  const r = { kind: I[2], value: W[8], aux: I[3] };
  Atomics.store(I, 0, 0);
  Atomics.notify(I, 0);
  return r;
}

// ureg->pc: the innermost frame of the image on this worker's stack — the
// stub that made the call — as its offset in the module, which is what the
// engines print for a wasm frame (V8 and SpiderMonkey; WebKit prints none,
// and the answer there is 0).
Error.stackTraceLimit = 32;
function userpc() {
  const m = /wasm-function\[\d+\]:0x([0-9a-f]+)/.exec(new Error().stack ?? '');
  return m ? parseInt(m[1], 16) : 0;
}

// One argument word, as the process passed it: a 32-bit value as its
// bits, a vlong whole.
const word = (a) => (typeof a === 'bigint' ? a : BigInt(a >>> 0));

function call(no, args) {
  const { W } = box;
  W[0] = BigInt(no);
  for (let k = 0; k < 5; k++) W[k + 1] = k < args.length ? word(args[k]) : 0n;
  return answer(say(ev.CALL));
}

// The kernel's answer, as the process takes it: a note handler first, if
// it says so — with the call's value kept until the handler is done.
function answer(r) {
  const v = r.value;
  for (;;) {
    switch (r.kind) {
      case rep.RET:
        return v;
      case rep.HANDLER:
        handler(r.aux);
        r = say(ev.DELIVER);
        continue;
      case rep.NOTED:
        throw new Noted();
      default:
        throw new Trap(`sys: trap: the kernel answered ${r.kind}`);
    }
  }
}

const int = (v) => Number(BigInt.asIntN(32, v));

function sys() {
  const s = {};
  for (const [name, no] of Object.entries(CALLS)) {
    s[name] = RET64.has(name) ? (...a) => call(no, a) : (...a) => int(call(no, a));
  }
  // rfork(RFPROC) returns twice — 0 in the child, the child's pid in the
  // parent — and this is the second time through, in either; then the
  // first, the call.
  s.rfork = (flags) => {
    const v = rewound();
    if (v !== null) {
      // the parent: sysrfork's "ready(p); sched();", and the answer
      if (v !== 0) answer(say(ev.YIELD));
      else answer(say(ev.DELIVER));
      return v;
    }
    const { W } = box;
    W[0] = BigInt(CALLS.rfork);
    W[1] = word(flags);
    W[2] = W[3] = W[4] = W[5] = 0n;
    const r = say(ev.CALL);
    if (r.kind !== rep.FORK) return int(answer(r));
    const child = Number(r.value);
    if (!unwind({ kind: 'fork', child, share: r.aux !== 0 })) {
      W[0] = BigInt(child);
      W[1] = W[2] = W[3] = W[4] = 0n;
      W[5] = 1n;
      return int(answer(say(ev.FORKED)));
    }
    return 0;
  };
  // setjmp and longjmp (libc/wasm/setjmp.c): the machine keeps the stack.
  s.setjmp = (env) => {
    const v = rewound();
    if (v !== null) return v;
    if (!unwind({ kind: 'setjmp', env })) throw new Trap('sys: trap: not asyncified');
    return 0;
  };
  s.longjmp = (env, val) => {
    if (jmpbuf(env)[1] === 0 && !st.jmps.has(env)) throw new Trap('sys: trap: longjmp to a jmp_buf no setjmp made');
    if (!unwind({ kind: 'longjmp', env, val })) throw new Trap('sys: trap: not asyncified');
  };
  return s;
}

// notify(Ureg*)'s machine half (pc/trap.c:834–857): the note on the
// process's own stack below its stack pointer, and the handler entered
// through the image's __notestart. It returns when the handler calls
// noted(NCONT).
function handler(f) {
  const { I, T } = box;
  const g = X?.__stack_pointer, start = X?.__notestart;
  if (!g || !start) throw new Trap('sys: trap: no note handler entry');
  const old = g.value;
  const at = ((old >>> 0) - (256 + ERRMAX)) & ~15;
  u8().set(T.subarray(0, I[4]), at);
  g.value = at;
  try {
    start(f, 0, at);
  } catch (e) {
    g.value = old;
    if (e instanceof Noted) return;
    throw e;
  }
  g.value = old;
  throw new Trap('sys: trap: note handler returned');
}

// The lowest address of the stack region in use: the stack pointer while
// the stack is in the region, and where it left the region while it is on
// another.
const lowat = (sp) => (sp >= 0 && sp < st.stacktop ? sp : st.regionLow);
const low = () => (X?.__stack_pointer ? lowat(X.__stack_pointer.value) : 0);
function moved(from, to) {
  const inside = (x) => x >= 0 && x < st.stacktop;
  if (inside(from) && !inside(to)) st.regionLow = from;
}

function jmpbuf(env) {
  const d = dv();
  return [d.getInt32(env + 4 * JMPBUFSP, true), d.getInt32(env + 4 * JMPBUFPC, true)];
}

// Begin unwinding the stack, for op. The import returns, and the code
// returns frame by frame to run.
function unwind(op) {
  const g = X.__stack_pointer;
  if (!g || !X.asyncify_start_unwind || !X.__asyncbuf) return false;
  const b = X.__asyncbuf(), n = X.__asyncbufsize();
  const d = dv();
  d.setUint32(b, b + 8, true);
  d.setUint32(b + 4, b + n, true);
  st.op = { ...op, sp: g.value };
  X.asyncify_start_unwind(b);
  return true;
}

// If this import is the one a stack was wound back into, finish the
// winding and answer what it answers.
function rewound() {
  if (st.rewind === null) return null;
  const v = st.rewind;
  st.rewind = null;
  X.asyncify_stop_rewind();
  return v;
}

function rewindinto(frames, sp, val) {
  const buf = X.__asyncbuf(), size = X.__asyncbufsize();
  if (frames.length + 8 > size) throw new Trap('sys: trap: stack too deep to unwind');
  // Rewinding reads the frames back from the end, outermost first, so the
  // next free byte is past them, as unwinding left it.
  u8().set(frames, buf + 8);
  const d = dv();
  d.setUint32(buf, buf + 8 + frames.length, true);
  d.setUint32(buf + 4, buf + size, true);
  X.__stack_pointer.value = sp;
  st.rewind = val;
  X.asyncify_start_rewind(buf);
}

// The process runs from its entry until its code is done — or until its
// stack unwinds for one of the machine's own operations. Then the machine
// does what the stack was unwound for and winds back the stack that should
// run next.
function run(entry) {
  for (;;) {
    st.entry = entry;
    let err = null;
    try {
      if (entry.kind === 'start') {
        X._start(entry.argc, entry.argv, entry.tos);
      } else {
        // A function returning here has returned into nothing.
        const f = X.__indirect_function_table.get(entry.pc);
        if (!f) throw new Trap('sys: trap: jump to a pc that is no function');
        f(entry.sp);
        if (!st.op) throw new Trap('sys: trap: a started function returned');
      }
    } catch (e) {
      err = e;
    }
    const o = st.op;
    st.op = null;
    if (err) throw err;
    if (!o) return;
    X.asyncify_stop_unwind();
    const buf = X.__asyncbuf();
    const cur = dv().getUint32(buf, true);
    let frames = u8().slice(buf + 8, cur);
    let sp = o.sp, val = 0, next = entry;
    switch (o.kind) {
      case 'setjmp': {
        // what 386's setjmp.s stores — SP, and a pc, which here is 0: the
        // frames are the machine's, kept under env
        const d = dv();
        d.setUint32(o.env + 4 * JMPBUFSP, o.sp, true);
        d.setUint32(o.env + 4 * JMPBUFPC, 0, true);
        st.jmps.set(o.env, { frames, sp: o.sp, entry });
        break;
      }
      case 'longjmp': {
        const [nsp, pc] = jmpbuf(o.env);
        if (pc !== 0) {
          // A stack no setjmp made: libthread's _threadinitstack writes a
          // function and a new stack's top into the jmp_buf. Nothing is
          // wound back: the function is called, on that stack.
          st.jmps.delete(o.env);
          moved(o.sp, nsp & ~15);
          X.__stack_pointer.value = nsp & ~15;
          entry = { kind: 'jump', pc, sp: nsp };
          continue;
        }
        const s = st.jmps.get(o.env);
        if (!s) throw new Trap('sys: trap: longjmp to a jmp_buf no setjmp made');
        moved(o.sp, s.sp);
        ({ frames, sp } = s);
        val = o.val;
        next = s.entry;
        break;
      }
      case 'fork': {
        // Below the lowest address the stack is using, nothing is live.
        const top = st.stacktop;
        const lo = Math.min(Math.max(lowat(o.sp), 0), Math.max(top, 0));
        post({ forkstate: { child: o.child, frames, sp: o.sp, entry, tos: st.tos, top, jmps: st.jmps, regionLow: st.regionLow } });
        const { W } = box;
        W[0] = BigInt(o.child);
        W[1] = o.share ? 1n : 0n;
        W[2] = BigInt(lo);
        W[3] = BigInt(st.tos);
        W[4] = BigInt(top);
        W[5] = 0n;
        // answered when the child's memory is made
        const r = say(ev.FORKED);
        if (r.kind !== rep.GO) throw new Trap(`sys: trap: the kernel answered ${r.kind} to a fork`);
        val = o.child;
        break;
      }
    }
    rewindinto(frames, sp, val);
    entry = next;
  }
}

// The Tos at the top of the stack, where Plan 9's kernel puts it
// (USTKTOP-sizeof(Tos)), the stack below it, and the pid in it — what
// kexit writes on every return to user mode (pc/trap.c:302).
function settos() {
  const g = X.__stack_pointer;
  if (!g) return 0;
  const top = g.value;
  st.stacktop = top;
  const tos = (top - TOSSIZE) & ~15;
  u8().fill(0, tos, tos + TOSSIZE);
  dv().setUint32(tos + TOSPID, pid, true);
  g.value = tos;
  st.tos = tos;
  return tos;
}

// The argument block on the stack, as sysexec places it (sysproc.c:389,
// :450): the strings end at the Tos, the char* array below them, and the
// stack pointer below that, 16-aligned.
function place(tos, args) {
  const enc = new TextEncoder();
  const strs = args.map((a) => enc.encode(a));
  const nbytes = strs.reduce((n, s) => n + s.length + 1, 0);
  const nptr = (args.length + 1) * 4;
  if (nptr + nbytes > TSTKSIZ || nptr + nbytes + 16 > tos) throw new Trap('sys: trap: virtual memory allocation failed');
  const charp = tos - nbytes;
  const argv = (charp - nptr) & ~15;
  X.__stack_pointer.value = argv;
  const d = dv(), u = u8();
  let at = charp;
  strs.forEach((s, k) => {
    d.setUint32(argv + 4 * k, at, true);
    u.set(s, at);
    u[at + s.length] = 0;
    at += s.length + 1;
  });
  d.setUint32(argv + 4 * args.length, 0, true);
  return [args.length, argv];
}
