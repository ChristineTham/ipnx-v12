// The kernel's worker: the Rust kernel (hosts/web, compiled to wasm32) and
// the half of the machine only JavaScript can be — the processes'
// memories, their mailboxes, the clock, the keyboard, the files.
//
// Every function the kernel imports is here, under "host", and each is
// named in hosts/web/src/js.rs with what it does. None decides anything.
//
// This worker blocks: the kernel waits on a process's mailbox, or for a key,
// with Atomics.wait. So nothing can be sent to it once it has booted — the
// keyboard is a ring in shared memory, and the processes' workers are made
// by the page, which it asks.

import { ev, mailbox } from './mailbox.mjs';

const node = typeof process === 'object' && !!process.versions?.node;
const port = node ? (await import('node:worker_threads')).parentPort : self;
const fs = node ? await import('node:fs') : null;
const post = (m) => port.postMessage(m);
if (node) port.on('message', boot);
else self.onmessage = (e) => boot(e.data);

const enc = new TextEncoder(), dec = new TextDecoder();

let K;            // the kernel's exports
let site;         // where the page is: its files are beside it
let C, R;         // the keyboard: counters, and the ring
const RING = 1 << 16;
const procs = new Map();   // pid → { mem, maximum, module, box }
const modules = [];        // compiled images
let last = null;           // the last file fetched

const K8 = () => new Uint8Array(K.memory.buffer);
const str = (p, n) => dec.decode(K8().subarray(p >>> 0, (p >>> 0) + (n >>> 0)));

// A file beside the page, synchronously — this worker cannot wait for a
// promise once the kernel runs. null if there is none.
function get(rel) {
  const url = new URL(rel.split('/').map(encodeURIComponent).join('/'), site);
  if (node) {
    try {
      return new Uint8Array(fs.readFileSync(url));
    } catch {
      return null;
    }
  }
  const x = new XMLHttpRequest();
  x.open('GET', url, false);
  x.responseType = 'arraybuffer';
  try {
    x.send();
  } catch {
    return null;
  }
  return x.status === 200 ? new Uint8Array(x.response) : null;
}

const mem8 = (pid) => {
  const p = procs.get(pid);
  return p ? new Uint8Array(p.mem.buffer) : null;
};

const host = {
  // ---- the console ----
  putstr(p, n) {
    post({ out: K8().slice(p >>> 0, (p >>> 0) + (n >>> 0)) });
  },
  // What was typed — but while an interrupt is waiting to be taken, only
  // what was typed before it: the console clears its queue when it takes
  // the interrupt, as rio discards what was typed before DEL, and what
  // came after must survive that.
  kbd(p, cap) {
    const w = Atomics.load(C, 0), r = C[1];
    const end = Atomics.load(C, 2) ? C[5] : w;
    const n = Math.min((end - r) | 0, cap >>> 0);
    if (n <= 0) return Atomics.load(C, 3) && end === w ? -1 : 0;
    const k = K8();
    for (let i = 0; i < n; i++) k[(p >>> 0) + i] = R[(r + i) & (RING - 1)];
    Atomics.store(C, 1, (r + n) | 0);
    return n;
  },
  interrupt() {
    return Atomics.exchange(C, 2, 0);
  },
  now() {
    return performance.timeOrigin + performance.now();
  },
  random(p, n) {
    const b = K8().subarray(p >>> 0, (p >>> 0) + (n >>> 0));
    for (let o = 0; o < b.length; o += 65536) crypto.getRandomValues(b.subarray(o, o + 65536));
  },
  reboot(p, n) {
    const cmd = str(p, n);
    if (cmd !== 'halt') return -1;
    post({ halt: true });
    return 0;
  },
  status(p, n, ok) {
    post({ status: str(p, n), ok: ok !== 0 });
  },
  // Wait up to ms, less if a key comes.
  delay(ms) {
    const s = Atomics.load(C, 4);
    if (Atomics.load(C, 0) !== C[1] || Atomics.load(C, 2) || Atomics.load(C, 3)) return;
    Atomics.wait(C, 4, s, ms >>> 0);
  },

  // ---- the files ----
  fetch(p, n) {
    last = get(`root/${str(p, n)}`);
    return last ? last.length : -1;
  },
  fetched(p, n) {
    K8().set(last.subarray(0, n >>> 0), p >>> 0);
    last = null;
  },

  // ---- the processes ----
  compile(p, n) {
    try {
      modules.push(new WebAssembly.Module(K8().slice(p >>> 0, (p >>> 0) + (n >>> 0))));
      return modules.length - 1;
    } catch {
      return -1;
    }
  },
  start(pid, module, initial, maximum, a, n) {
    const mem = new WebAssembly.Memory({ initial, maximum, shared: true });
    const args = str(a, n).split('\0');
    args.pop();
    const box = mailbox();
    procs.set(pid, { mem, maximum, module, box });
    post({ start: true, pid, module: modules[module], mem, box: box.sab, args });
  },
  wait(pid, ms) {
    const b = procs.get(pid);
    if (!b) return ev.EXITED;
    const I = b.box.I;
    const end = performance.now() + ms;
    for (;;) {
      const s = Atomics.load(I, 0);
      if (s === 1) return I[1];
      const left = end - performance.now();
      if (left <= 0) return ev.TIME;
      Atomics.wait(I, 0, s, left);
    }
  },
  word(pid, i) {
    return procs.get(pid)?.box.W[i] ?? 0n;
  },
  reply(pid, kind, value, aux) {
    const b = procs.get(pid)?.box;
    if (!b) return;
    b.I[2] = kind;
    b.W[8] = value;
    b.I[3] = aux;
    Atomics.store(b.I, 0, 2);
    Atomics.notify(b.I, 0);
  },
  xfer(pid, p, n) {
    const b = procs.get(pid)?.box;
    if (!b) return;
    b.T.set(K8().subarray(p >>> 0, (p >>> 0) + (n >>> 0)));
    b.I[4] = n;
  },
  xferin(pid, p, cap) {
    const b = procs.get(pid)?.box;
    if (!b) return 0;
    const n = Math.min(b.I[4], cap >>> 0);
    K8().set(b.T.subarray(0, n), p >>> 0);
    return n;
  },
  memread(pid, addr, p, n) {
    const m = mem8(pid);
    addr >>>= 0;
    n >>>= 0;
    if (!m || addr + n > m.length) return -1;
    K8().set(m.subarray(addr, addr + n), p >>> 0);
    return 0;
  },
  memwrite(pid, addr, p, n) {
    const m = mem8(pid);
    addr >>>= 0;
    n >>>= 0;
    if (!m || addr + n > m.length) return -1;
    m.set(K8().subarray(p >>> 0, (p >>> 0) + n), addr);
    return 0;
  },
  strlen(pid, addr) {
    const m = mem8(pid);
    addr >>>= 0;
    if (!m || addr >= m.length) return -1;
    const i = m.indexOf(0, addr);
    return i < 0 ? m.length - addr : i - addr;
  },
  load(pid, addr, ok) {
    const p = procs.get(pid);
    addr >>>= 0;
    const fine = p && addr + 4 <= p.mem.buffer.byteLength;
    new DataView(K.memory.buffer).setInt32(ok >>> 0, fine ? 1 : 0, true);
    if (!fine) return 0;
    if (addr % 4 === 0) return Atomics.load(new Int32Array(p.mem.buffer), addr >> 2);
    return new DataView(p.mem.buffer).getInt32(addr, true);
  },
  cas(pid, addr, old, nw) {
    const p = procs.get(pid);
    addr >>>= 0;
    if (!p || addr + 4 > p.mem.buffer.byteLength) return -1;
    if (addr % 4 === 0) return Atomics.compareExchange(new Int32Array(p.mem.buffer), addr >> 2, old, nw) === old ? 1 : 0;
    const d = new DataView(p.mem.buffer);
    if (d.getInt32(addr, true) !== old) return 0;
    d.setInt32(addr, nw, true);
    return 1;
  },
  fork(parent, child, share, lo) {
    const p = procs.get(parent);
    let mem = p.mem;
    if (!share) {
      const src = new Uint8Array(p.mem.buffer);
      mem = new WebAssembly.Memory({ initial: src.length / 65536, maximum: p.maximum, shared: true });
      new Uint8Array(mem.buffer).set(src.subarray(lo >>> 0), lo >>> 0);
    }
    procs.set(child, { mem, maximum: p.maximum, module: p.module, box: mailbox() });
  },
  child(pid) {
    const c = procs.get(pid);
    post({ child: true, pid, module: modules[c.module], mem: c.mem, box: c.box.sab });
  },
  kill(pid) {
    procs.delete(pid);
    post({ kill: true, pid });
  },
};

function boot(m) {
  if (!m.boot) return;
  site = m.boot.site;
  C = new Int32Array(m.boot.ctl, 0, 16);
  R = new Uint8Array(m.boot.ctl, 64, RING);
  const wasm = get('kernel.wasm'), index = get('root.index');
  if (!wasm || !index) {
    post({ status: `cannot fetch ${wasm ? 'root.index' : 'kernel.wasm'} beside ${site}`, ok: false });
    return;
  }
  K = new WebAssembly.Instance(new WebAssembly.Module(wasm), { host }).exports;
  const put = (b) => {
    const p = K.web_alloc(b.length);
    K8().set(b, p);
    return [p, b.length];
  };
  const [cp, cn] = put(enc.encode(m.boot.cmd.map((w) => `${w}\0`).join('')));
  const [ip, iN] = put(index);
  try {
    K.web_boot(cp, cn, ip, iN);
  } catch (e) {
    post({ status: `the kernel stopped: ${e?.message ?? e}`, ok: false });
  }
}
