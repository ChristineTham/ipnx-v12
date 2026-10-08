// The mailbox a process and the kernel share — one per process, in shared
// memory: the browser machine's trap. A process writes what it says and
// waits; the kernel answers and the process goes on. The numbers are
// `hosts/web/src/js.rs`'s `ev` and `rep`, and must stay them.
//
// I[0]  the state: 0 the process runs, 1 it has said something, 2 answered
// I[1]  what it said (ev)          I[2]  the answer's kind (rep)
// I[3]  the answer's other word    I[4]  how many bytes are in the transfer
// W[0..5]  a call's number and its words, or an event's details
// W[6]  the lowest address of its stack region in use, when it said it
// W[7]  where it is, when the kernel asked (rep.PC)
// W[8]  the answer's value
// T     the transfer: a note's text, a fault's

export const ev = { TIME: 0, CALL: 1, DELIVER: 2, EXITED: 3, FAULT: 4, FORKED: 5, YIELD: 6, PC: 7 };
export const rep = { RET: 0, HANDLER: 1, NOTED: 2, FORK: 3, GO: 4, PC: 5 };

// sys.h's numbers (plan9/sys/src/libc/9syscall/sys.h; kernel/src/lib.rs,
// `sysno`): what the PC's stub puts in AX.
export const CALLS = {
  sysr1: 0, bind: 2, chdir: 3, close: 4, dup: 5, alarm: 6, exec: 7, exits: 8,
  fauth: 10, _fstat: 11, segbrk: 12, open: 14, sleep: 17, _stat: 18, rfork: 19,
  pipe: 21, create: 22, fd2path: 23, remove: 25, notify: 28, noted: 29,
  segattach: 30, segdetach: 31, segfree: 32, segflush: 33, rendezvous: 34,
  unmount: 35, semacquire: 37, semrelease: 38, seek: 39, fversion: 40,
  errstr: 41, stat: 42, fstat: 43, wstat: 44, fwstat: 45, mount: 46,
  await: 47, pread: 50, pwrite: 51, tsemacquire: 52,
};

export const SIZE = 4096;

export function mailbox(sab = new SharedArrayBuffer(SIZE)) {
  return { sab, I: new Int32Array(sab, 0, 8), W: new BigInt64Array(sab, 32, 16), T: new Uint8Array(sab, 256) };
}

// Say something that is not answered — the image has ended, or trapped —
// from the process, or from the page for a worker that died.
export function tell(b, kind, text = '') {
  const t = new TextEncoder().encode(text).subarray(0, b.T.length);
  b.T.set(t);
  b.I[4] = t.length;
  b.I[1] = kind;
  Atomics.store(b.I, 0, 1);
  Atomics.notify(b.I, 0);
}
