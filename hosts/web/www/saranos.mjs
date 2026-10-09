// The page's half of the browser machine: it makes the workers the kernel
// asks for — one for the kernel, one for each process — and is the console
// between them and whoever is typing.
//
// The kernel cannot make a worker itself: it blocks, and a worker it made
// would not start until it stopped. So it says what it wants and the page
// does it. A fork's child is the one join: the parent's worker sends its
// unwound stack here, the kernel sends the child's memory, and the child's
// worker starts when both have come.
//
// Workers start one at a time, each after the last has said it is ready —
// WebKit loses every second worker started concurrently (platforms.md).
//
// The same file runs under Node, with worker_threads for workers, which is
// how the host's tests drive it.

import { ev, mailbox, tell, gone } from './mailbox.mjs';

const node = typeof process === 'object' && !!process.versions?.node;
const NodeWorker = node ? (await import('node:worker_threads')).Worker : null;

const RING = 1 << 16;
const NRING = 1 << 20;

// A worker, the same on both: post, hear, hear it fail, end it.
function worker(name) {
  const url = new URL(name, import.meta.url);
  if (node) {
    // Plan 9's user stack, as the terminal machine gives it (pc/mem.h:51)
    const w = new NodeWorker(url, { resourceLimits: { stackSizeMb: 16 } });
    return {
      post: (m, transfer = []) => w.postMessage(m, transfer),
      hear: (f) => w.on('message', f),
      failed: (f) => w.on('error', (e) => f(e?.message ?? String(e))),
      end: () => w.terminate(),
    };
  }
  const w = new Worker(url, { type: 'module' });
  return {
    post: (m, transfer = []) => w.postMessage(m, transfer),
    hear: (f) => w.addEventListener('message', (e) => f(e.data)),
    failed: (f) =>
      w.addEventListener('error', (e) => {
        e.preventDefault();
        f(e.message ?? 'the worker failed');
      }),
    end: () => w.terminate(),
  };
}

// The keyboard is a ring in shared memory, which the kernel's worker reads
// at its clock: C[0] how many bytes typed, C[1] how many taken, C[2] the
// interrupt key pressed and not yet taken, C[3] no more input, C[4] counts
// wakings, C[5] how many had been typed when the interrupt was pressed.
//
// Boot the system. `site` is where kernel.wasm, root.index and root/ are;
// `cmd` is what init runs, or nothing for the shell alone; `out` is given
// what the system writes to its screen. Answers the keyboard, and `done`:
// pid 1's status when the system is over.
export function boot({ site, cmd = [], out, halt, emca }) {
  const ctl = new SharedArrayBuffer(64 + RING);
  const C = new Int32Array(ctl, 0, 16);
  const R = new Uint8Array(ctl, 64, RING);
  // emca's replies, for the kernel's worker to harvest: a ring of
  // R-messages, each its own length first (kernel.mjs, `harvest`)
  const nine = new SharedArrayBuffer(64 + NRING);
  const N = new Int32Array(nine, 0, 2);
  const NB = new Uint8Array(nine, 64, NRING);
  const waiting = [];
  function answer(r) {
    if (r) waiting.push(r);
    while (waiting.length) {
      const m = waiting[0];
      const w = N[0];
      if (NRING - ((w - Atomics.load(N, 1)) | 0) < m.length) {
        setTimeout(answer, 5);
        return;
      }
      for (let i = 0; i < m.length; i++) NB[(w + i) & (NRING - 1)] = m[i];
      Atomics.store(N, 0, (w + m.length) | 0);
      waiting.shift();
    }
    wake();
  }
  if (emca) emca.reply = answer;
  const procs = new Map();     // pid → its worker
  const queue = [];            // workers to start, in order
  let starting = null;         // the one starting now
  const forks = new Map();     // child → its parent's unwound stack
  const children = new Map();  // child → what the kernel made it
  let finish;
  const done = new Promise((r) => (finish = r));

  const kernel = worker('kernel.mjs');
  const everything = () => {
    kernel.end();
    for (const p of procs.values()) {
      // woken first: a worker ended in Atomics.wait is never collected
      if (p.box) gone(p.box);
      p.end();
    }
    procs.clear();
  };

  function next() {
    if (starting || !queue.length) return;
    const m = queue.shift();
    procs.get(m.pid)?.end();
    const w = worker('proc.mjs');
    procs.set(m.pid, w);
    starting = w;
    const box = mailbox(m.box);
    w.box = box;
    w.hear((x) => {
      if (x.ready) {
        if (starting === w) starting = null;
        next();
      } else if (x.forkstate) {
        forks.set(x.forkstate.child, x.forkstate);
        join(x.forkstate.child);
      }
    });
    // A worker that dies outside its image — its script, its memory — is
    // a fault, said for it.
    w.failed((e) => {
      if (starting === w) starting = null;
      tell(box, ev.FAULT, `sys: trap: ${e}`);
      next();
    });
    // the far end of the channel its memory is coming down (kernel.mjs)
    w.post(m, [m.port]);
  }

  function join(child) {
    const f = forks.get(child), c = children.get(child);
    if (!f || !c) return;
    forks.delete(child);
    children.delete(child);
    queue.push({ ...c, state: f });
    next();
  }

  kernel.hear((m) => {
    if (m.nine) emca?.serve(m.nine);
    else if (m.out) out(m.out);
    else if (m.start) {
      queue.push(m);
      next();
    } else if (m.child) {
      children.set(m.pid, m);
      join(m.pid);
    } else if (m.kill) {
      for (let i = queue.length - 1; i >= 0; i--) if (queue[i].pid === m.pid) queue.splice(i, 1);
      forks.delete(m.pid);
      children.delete(m.pid);
      procs.get(m.pid)?.end();
      procs.delete(m.pid);
    } else if (m.halt) {
      everything();
      halt?.();
      finish({ status: 'halt', ok: true });
    } else if ('status' in m) {
      everything();
      finish({ status: m.status, ok: m.ok });
    }
  });
  kernel.failed((e) => {
    everything();
    finish({ status: `the kernel's worker failed: ${e}`, ok: false });
  });
  kernel.post({ boot: { site: String(site), cmd, ctl, nine, wm: !!emca } });

  function wake() {
    Atomics.add(C, 4, 1);
    Atomics.notify(C, 4);
  }
  return {
    done,
    // What is typed, as bytes: how many the ring took.
    type(bytes) {
      let w = C[0];
      let n = 0;
      for (const b of bytes) {
        if (((w - Atomics.load(C, 1)) | 0) >= RING) break;
        R[w & (RING - 1)] = b;
        w = (w + 1) | 0;
        n++;
      }
      Atomics.store(C, 0, w);
      wake();
      return n;
    },
    // The interrupt key — after what has been typed so far, and before
    // whatever is typed next.
    interrupt() {
      C[5] = C[0];
      Atomics.store(C, 2, 1);
      wake();
    },
    // No more input: the keyboard is gone.
    end() {
      Atomics.store(C, 3, 1);
      wake();
    },
    stop: everything,
  };
}
