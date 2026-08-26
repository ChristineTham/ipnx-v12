// ipnx-v12 poc supervisor: the hosted kernel.
// One Worker per process; syscalls arrive on a per-process SAB mailbox
// (RESEARCH.md §5.3); the kernel thread never blocks.
import { Worker } from "node:worker_threads";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { makeRamfs, makeCons } from "./devs.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const rootdir = process.argv[2] ?? join(here, "..", "rootfs");

// ---- traps (Plan 9 numbering) ----
const T = { BIND: 2, CHDIR: 3, CLOSE: 4, DUP: 5, EXEC: 7, EXITS: 8, OPEN: 14,
  SLEEP: 17, RFORK: 19, SEEK: 39, ERRSTR: 41, STAT: 42, FSTAT: 43, AWAIT: 47,
  PREAD: 50, PWRITE: 51, ARGS: 200 };
const RF = { NAMEG: 1, FDG: 4, PROC: 16, MEM: 32 };
// mailbox: [0]=state(0 idle,1 req,2 done) [1]=trap [2..7]=args [8]=ret [9]=aux
const ST = { IDLE: 0, REQ: 1, DONE: 2 };
// special replies the worker interprets (out of band of ordinary returns)
const R_FORKRESUME = -1000;  // restore stack, throw; guard catches, returns aux
const R_EXECSELF   = -1001;  // throw to worker top level; new image follows
const TXSIZE = 65536;

const devs = { M: makeRamfs(rootdir), c: makeCons() };
const procs = new Map();
let nextpid = 1;

// ---- namespace ----
const canon = (path, cwd) => {
  if (!path.startsWith("/") && !path.startsWith("#")) path = cwd + "/" + path;
  if (path.startsWith("#")) return path;
  const out = [];
  for (const c of path.split("/")) {
    if (c === "" || c === ".") continue;
    if (c === "..") throw err("dotdot unsupported in v0");
    out.push(c);
  }
  return "/" + out.join("/");
};
function attach(spec) {  // '#c' etc.
  const dev = devs[spec[1]];
  if (!dev) throw err(`unknown device #${spec[1]}`);
  return { dev, node: dev.attach() };
}
function walk(proc, path) {
  path = canon(path, proc.cwd);
  if (path.startsWith("#")) return attach(path);
  // longest-prefix match in the per-process mount table
  let best = "", start = { dev: devs.M, node: devs.M.attach() };
  for (const [pfx, chan] of proc.ns)
    if ((path === pfx || path.startsWith(pfx === "/" ? "/" : pfx + "/")) && pfx.length > best.length)
      { best = pfx; start = chan; }
  let { dev, node } = start;
  const rest = path.slice(best.length).split("/").filter(Boolean);
  for (const name of rest) {
    node = dev.walk(node, name);
    if (!node) throw err(`'${path}' does not exist`);
  }
  return { dev, node };
}
const err = (msg) => Object.assign(new Error(msg), { guest: true });

// ---- processes ----
function newProc(ppid, { ns, fds, cwd }) {
  const pid = nextpid++;
  const p = { pid, ppid, ns, fds, cwd, errstr: "", zombies: [], awaitPending: false,
    borrower: null, worker: null, mb: null, tx: null, argv: [] };
  procs.set(pid, p);
  return p;
}
const newChan = (c, mode) => ({ dev: c.dev, node: c.node, mode, offset: 0 });

function spawn(proc, modBytes, argv) {
  proc.mb = new Int32Array(new SharedArrayBuffer(64));
  proc.tx = new Uint8Array(new SharedArrayBuffer(TXSIZE));
  proc.argv = argv;
  const w = new Worker(join(here, "worker.mjs"), {
    workerData: { mb: proc.mb, tx: proc.tx, modBytes },
  });
  proc.worker = w;
  w.on("message", (m) => { if (m.t === "sc") onSyscall(procs.get(proc.pid) ? proc : null); });
  w.on("error", (e) => { console.error(`worker pid ${proc.pid}:`, e); shutdown(1); });
  return w;
}

const txstr = (proc, off = 0) => {
  const end = proc.tx.indexOf(0, off);
  return Buffer.from(proc.tx.subarray(off, end < 0 ? undefined : end)).toString();
};
function reply(proc, ret, aux = 0) {
  proc.mb[8] = ret | 0; proc.mb[9] = aux | 0;
  Atomics.store(proc.mb, 0, ST.DONE);
  Atomics.notify(proc.mb, 0);
}

// ---- the dispatcher ----
function onSyscall(proc) {
  if (!proc) return;
  // While a lazy-fork child borrows the parent's Worker, syscalls arriving on
  // the parent's mailbox belong to the child — its own fds, its own namespace.
  const self = proc.borrower ?? proc;
  const [, trap, a0, a1, a2, a3, a4] = proc.mb;
  try {
    const r = dispatch(proc, self, trap, a0, a1, a2, a3, a4);
    if (r !== undefined) reply(proc, r.ret ?? r, r.aux ?? 0);
  } catch (e) {
    if (!e.guest) throw e;
    self.errstr = e.message;
    reply(proc, -1);
  }
}

function dispatch(host, self, trap, a0, a1, a2, a3, a4) {
  switch (trap) {
  case T.ARGS: {
    const block = Buffer.from(self.argv.map((s) => s + "\0").join(""), "utf8");
    host.tx.set(block.subarray(0, a1));
    return Math.min(block.length, a1);
  }
  case T.BIND: {
    const name = txstr(host), old = txstr(host, name.length + 1);
    self.ns.set(canon(old, self.cwd), walk(self, name));   // resolved now, per bind(2)
    return 0;
  }
  case T.CHDIR: {
    const path = canon(txstr(host), self.cwd);
    walk(self, path);
    self.cwd = path;
    return 0;
  }
  case T.OPEN: {
    const c = walk(self, txstr(host));
    const fd = self.fds.findIndex((f) => !f);
    const chan = newChan(c, a1);
    if (fd < 0) { self.fds.push(chan); return self.fds.length - 1; }
    self.fds[fd] = chan;
    return fd;
  }
  case T.CLOSE: { fdchk(self, a0); self.fds[a0] = null; return 0; }
  case T.DUP: {
    const c = fdchk(self, a0);
    if (a1 >= 0) { self.fds[a1] = c; return a1; }        // shared chan — dup(2) semantics
    const fd = self.fds.findIndex((f) => !f);
    if (fd < 0) { self.fds.push(c); return self.fds.length - 1; }
    self.fds[fd] = c; return fd;
  }
  case T.PREAD: {
    const c = fdchk(self, a0);                       // a1=buf (guest side), a2=n, a3/a4=offset
    const n = Math.min(a2, TXSIZE);
    const cur = (a3 >>> 0) === 0xffffffff && (a4 >>> 0) === 0xffffffff;
    const off = cur ? c.offset : (a4 * 0x100000000 + (a3 >>> 0));
    const data = c.dev.read(c.node, n, off);
    if (cur) c.offset += data.length;
    host.tx.set(data);
    return data.length;
  }
  case T.PWRITE: {
    const c = fdchk(self, a0);
    const n = Math.min(a2, TXSIZE);
    return c.dev.write(c.node, host.tx.subarray(0, n));
  }
  case T.SEEK: {
    const c = fdchk(self, a0);
    const off = a2 * 0x100000000 + (a1 >>> 0);
    c.offset = a3 === 0 ? off : a3 === 1 ? c.offset + off : c.dev.len(c.node) + off;
    return c.offset;
  }
  case T.STAT: {
    const c = walk(self, txstr(host));
    const rec = c.dev.stat(c.node);
    if (rec.length > a2) throw err("stat buffer too small");
    host.tx.set(rec);
    return rec.length;
  }
  case T.FSTAT: {
    const c = fdchk(self, a0);
    const rec = c.dev.stat(c.node);
    host.tx.set(rec);
    return rec.length;
  }
  case T.ERRSTR: {
    const b = Buffer.from(self.errstr.slice(0, a1 - 1) + "\0");
    host.tx.set(b);
    return b.length - 1;
  }
  case T.SLEEP: setTimeout(() => reply(host, 0), a0); return undefined;
  case T.RFORK: return rfork(host, self, a0, a2);
  case T.EXEC: return exec(host, self);
  case T.EXITS: return exits(host, self);
  case T.AWAIT: return doAwait(host, self, a1);
  default: throw err(`bad syscall ${trap} (v0)`);
  }
}
function fdchk(self, fd) {
  const c = self.fds[fd];
  if (!c) throw err(`fd ${fd} not open`);
  return c;
}

// ---- rfork / exec / exits / await ----
function rfork(host, self, flags, guarded) {
  if (!(flags & RF.PROC)) return 0;                     // flag changes on self: v0 no-ops
  if (!guarded)
    throw err("bare rfork(RFPROC) needs asyncify on this engine; use procrfork (v0)");
  if (!(flags & RF.MEM)) throw err("plain fork needs asyncify — v0 is the lazy path");
  if (host.borrower) throw err("nested lazy fork unsupported in v0");
  const child = newProc(self.pid, {
    ns: flags & RF.NAMEG ? new Map(self.ns) : self.ns,
    fds: flags & RF.FDG ? [...self.fds] : self.fds,
    cwd: self.cwd,
  });
  host.borrower = child;   // child borrows the parent's Worker and stack
  return { ret: 0, aux: child.pid };  // worker saves [0,sp) on seeing aux!=0
}
function exec(host, self) {
  const path = txstr(host);
  const argv = [];
  for (let o = path.length + 1; host.tx[o];) { const s = txstr(host, o); argv.push(s); o += s.length + 1; }
  const c = walk(self, path);
  const bytes = c.dev.read(c.node, c.dev.len(c.node), 0);
  new WebAssembly.Module(Buffer.from(bytes));          // validate before committing
  self.argv = argv.length ? argv : [path];
  if (host.borrower === self) {
    // Lazy-fork child leaves the borrowed stack for its own instance; the
    // parent's Worker restores [0,sp) and unwinds to the rfork guard (§5.2).
    host.borrower = null;
    spawn(self, bytes, self.argv);
    return { ret: R_FORKRESUME, aux: self.pid };
  }
  host.worker.postMessage({ t: "load", modBytes: bytes });
  return R_EXECSELF;
}
function exits(host, self) {
  const msg = txstr(host);
  const parent = procs.get(self.ppid);
  procs.delete(self.pid);
  if (host.borrower === self) {                        // exited without exec: vfork's other exit
    host.borrower = null;
    zombie(parent, self.pid, msg);
    return { ret: R_FORKRESUME, aux: self.pid };
  }
  host.worker?.terminate();
  if (self.pid === 1) return shutdown(msg === "" ? 0 : 1);
  zombie(parent, self.pid, msg);
  return undefined;                                    // no one to reply to
}
function zombie(parent, pid, msg) {
  if (!parent) return;
  parent.zombies.push(`${pid} 0 0 0 '${msg}'`);
  if (parent.awaitPending) { parent.awaitPending = false; sendWait(parent); }
}
function doAwait(host, self, max) {
  self.awaitmax = max;
  if (self.zombies.length) return sendWait(self, true);
  self.awaitPending = true;
  return undefined;                                    // blocked until a child dies
}
function sendWait(self, inline = false) {
  const s = Buffer.from(self.zombies.shift().slice(0, self.awaitmax - 1) + "\0");
  self.tx.set(s);
  if (inline) return s.length - 1;
  reply(self, s.length - 1);
}
function shutdown(code) {
  for (const p of procs.values()) p.worker?.terminate();
  // let stdout drain
  setTimeout(() => process.exit(typeof code === "number" ? code : 1), 20);
  return undefined;
}

// ---- boot: pid 1 ----
const init = newProc(0, { ns: new Map(), fds: [], cwd: "/" });
spawn(init, readFileSync(join(rootdir, "bin", "init")), ["init"]);
