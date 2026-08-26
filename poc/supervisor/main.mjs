// ipnx-v12 poc supervisor: the hosted kernel.
// One Worker per process; syscalls arrive on a per-process SAB mailbox
// (RESEARCH.md §5.3); the kernel thread never blocks — reads that must wait
// (pipes, the console) park in the device and complete later.
import { Worker } from "node:worker_threads";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { makeRamfs, makeCons, makePipeDev } from "./devs.mjs";
import { makeMntDev, mountConn } from "./mnt9p.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const rootdir = process.argv[2] ?? join(here, "..", "rootfs");
const interactive = process.argv.includes("-i");

// ---- traps (Plan 9 numbering) ----
const T = { BIND: 2, CHDIR: 3, CLOSE: 4, DUP: 5, EXEC: 7, EXITS: 8, OPEN: 14,
  SLEEP: 17, RFORK: 19, PIPE: 21, CREATE: 22, REMOVE: 25, SEEK: 39, ERRSTR: 41,
  STAT: 42, FSTAT: 43, MOUNT: 46, AWAIT: 47, PREAD: 50, PWRITE: 51, ARGS: 200 };
const RF = { NAMEG: 1, FDG: 4, PROC: 16, MEM: 32 };
const OTRUNC = 16, DMDIR = 0x80000000;
// mailbox: [0]=state(0 idle,1 req,2 done) [1]=trap [2..7]=args [8]=ret [9]=aux
const ST = { IDLE: 0, REQ: 1, DONE: 2 };
// special replies the worker interprets (out of band of ordinary returns)
const R_FORKRESUME = -1000;  // restore stack, throw; guard catches, returns aux
const R_EXECSELF   = -1001;  // throw to worker top level; new image follows
const TXSIZE = 65536;

const cons = makeCons();
const devs = { M: makeRamfs(rootdir), c: cons, "|": makePipeDev(), m: makeMntDev() };

// read a channel from kernel context, parking-aware, as a promise
const readChan = (chan, n) => new Promise((res) => {
  const r = chan.dev.read(chan.node, n, -1, { done: res });
  if (r !== undefined) res(r);
});
const procs = new Map();
let nextpid = 1;

if (interactive) {
  process.stdin.on("data", (c) => cons.feed(c));
  process.stdin.on("end", () => cons.end());
}

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
async function walk(proc, path) {
  path = canon(path, proc.cwd);
  if (path.startsWith("#")) return attach(path);
  let best = "", start = { dev: devs.M, node: devs.M.attach() };
  for (const [pfx, chan] of proc.ns)
    if ((path === pfx || path.startsWith(pfx === "/" ? "/" : pfx + "/")) && pfx.length > best.length)
      { best = pfx; start = chan; }
  let { dev, node } = start;
  const rest = path.slice(best.length).split("/").filter(Boolean);
  for (const name of rest) {
    node = await dev.walk(node, name);
    if (!node) throw err(`'${path}' does not exist`);
  }
  return { dev, node };
}
async function walkParent(proc, path) {            // for create/remove
  path = canon(path, proc.cwd);
  const i = path.lastIndexOf("/");
  const base = path.slice(i + 1);
  if (!base || path.startsWith("#")) throw err(`bad path '${path}'`);
  return { parent: await walk(proc, path.slice(0, i) || "/"), base };
}
async function readAll(c) {                        // exec's image read; open if the dev needs it
  if (c.dev.open) await c.dev.open(c.node, 0 /*OREAD*/);
  const parts = [];
  for (let off = 0; ;) {
    const chunk = await c.dev.read(c.node, 8192, off);
    if (chunk.length === 0) break;
    parts.push(Buffer.from(chunk));
    off += chunk.length;
  }
  c.dev.discard?.(c.node);
  return Buffer.concat(parts);
}
const err = (msg) => Object.assign(new Error(msg), { guest: true });

// ---- processes, channels, descriptor tables ----
function newProc(ppid, { ns, fdt, cwd }) {
  const pid = nextpid++;
  const p = { pid, ppid, ns, fdt, cwd, errstr: "", zombies: [], awaitPending: false,
    borrower: null, worker: null, mb: null, tx: null, argv: [] };
  procs.set(pid, p);
  return p;
}
const newChan = (c, mode) => ({ dev: c.dev, node: c.node, mode, offset: 0, refs: 1 });
const incref = (c) => { c.refs++; return c; };
const decref = (c) => { if (--c.refs === 0) c.dev.clunk?.(c.node); };
const newFdt = () => ({ refs: 1, fds: [] });
function fdtCopy(fdt) {
  const t = { refs: 1, fds: [...fdt.fds] };
  for (const c of t.fds) if (c) incref(c);
  return t;
}
function fdtClose(fdt) {
  if (--fdt.refs === 0) { for (const c of fdt.fds) if (c) decref(c); fdt.fds = []; }
}
function fdAlloc(self, chan, at = -1) {
  const fds = self.fdt.fds;
  if (at >= 0) { if (fds[at]) decref(fds[at]); fds[at] = chan; return at; }
  const fd = fds.findIndex((f) => !f);
  if (fd < 0) { fds.push(chan); return fds.length - 1; }
  fds[fd] = chan;
  return fd;
}
function fdchk(self, fd) {
  const c = self.fdt.fds[fd];
  if (!c) throw err(`fd ${fd} not open`);
  return c;
}

function spawn(proc, modBytes, argv) {
  proc.mb = new Int32Array(new SharedArrayBuffer(64));
  proc.tx = new Uint8Array(new SharedArrayBuffer(TXSIZE));
  proc.argv = argv;
  const w = new Worker(join(here, "worker.mjs"), {
    workerData: { mb: proc.mb, tx: proc.tx, modBytes },
  });
  proc.worker = w;
  w.on("message", (m) => { if (m.t === "sc") onSyscall(proc); });
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
async function onSyscall(proc) {
  if (!procs.has(proc.pid) && !proc.borrower) return;
  // While a lazy-fork child borrows the parent's Worker, syscalls arriving on
  // the parent's mailbox belong to the child — its own fds, its own namespace.
  const self = proc.borrower ?? proc;
  const [, trap, a0, a1, a2, a3, a4] = proc.mb;
  try {
    const r = await dispatch(proc, self, trap, a0, a1, a2, a3, a4);
    if (r !== undefined) reply(proc, r.ret ?? r, r.aux ?? 0);
  } catch (e) {
    if (!e.guest) { console.error(e); return shutdown(1); }
    self.errstr = e.message;
    reply(proc, -1);
  }
}

async function dispatch(host, self, trap, a0, a1, a2, a3, a4) {
  switch (trap) {
  case T.ARGS: {
    const block = Buffer.from(self.argv.map((s) => s + "\0").join(""), "utf8");
    host.tx.set(block.subarray(0, a1));
    return Math.min(block.length, a1);
  }
  case T.BIND: {
    const name = txstr(host), old = txstr(host, name.length + 1);
    self.ns.set(canon(old, self.cwd), await walk(self, name));   // resolved now, per bind(2)
    return 0;
  }
  case T.CHDIR: {
    const path = canon(txstr(host), self.cwd);
    const c = await walk(self, path);
    c.dev.discard?.(c.node);
    self.cwd = path;
    return 0;
  }
  case T.OPEN: {
    let c = await walk(self, txstr(host));
    if (c.dev.clone && !c.node.ephemeral)          // never open a mount-table fid:
      c = { dev: c.dev, node: await c.dev.clone(c.node) };   // clone first, per Plan 9
    if (c.dev.open) await c.dev.open(c.node, a1);
    else if ((a1 & OTRUNC) && c.dev.truncate) c.dev.truncate(c.node);
    return fdAlloc(self, newChan(c, a1));
  }
  case T.CREATE: {
    let { parent, base } = await walkParent(self, txstr(host));
    if (!parent.dev.create) throw err("create not supported on this device");
    if (parent.dev.clone && !parent.node.ephemeral)          // Tcreate consumes the fid
      parent = { dev: parent.dev, node: await parent.dev.clone(parent.node) };
    const node = await parent.dev.create(parent.node, base, a2 >>> 0,
      !!((a2 >>> 0) & DMDIR), a1);
    return fdAlloc(self, newChan({ dev: parent.dev, node }, a1));
  }
  case T.REMOVE: {
    const { parent, base } = await walkParent(self, txstr(host));
    if (!parent.dev.remove) throw err("remove not supported on this device");
    await parent.dev.remove(parent.node, base);
    parent.dev.discard?.(parent.node);
    return 0;
  }
  case T.MOUNT: {
    const old = txstr(host), aname = txstr(host, old.length + 1);
    const c = incref(fdchk(self, a0));             // the kernel holds its own reference
    const node = await mountConn(c, readChan, aname);
    self.ns.set(canon(old, self.cwd), { dev: devs.m, node });
    return 0;
  }
  case T.PIPE: {
    const pdev = devs["|"];
    const p = pdev.newPipe();
    const fd0 = fdAlloc(self, { dev: pdev, node: { p, end: 0 }, mode: 2, offset: 0, refs: 1 });
    const fd1 = fdAlloc(self, { dev: pdev, node: { p, end: 1 }, mode: 2, offset: 0, refs: 1 });
    const out = Buffer.alloc(8);
    out.writeInt32LE(fd0, 0); out.writeInt32LE(fd1, 4);
    host.tx.set(out);
    return 0;
  }
  case T.CLOSE: { decref(fdchk(self, a0)); self.fdt.fds[a0] = null; return 0; }
  case T.DUP: {
    const c = incref(fdchk(self, a0));
    return fdAlloc(self, c, a1 >= 0 ? a1 : -1);
  }
  case T.PREAD: {
    const c = fdchk(self, a0);                     // a1=buf (guest side), a2=n, a3/a4=offset
    const n = Math.min(a2, TXSIZE);
    const cur = (a3 >>> 0) === 0xffffffff && (a4 >>> 0) === 0xffffffff;
    const off = cur ? c.offset : (a4 * 0x100000000 + (a3 >>> 0));
    const ctx = { done: (data) => { if (cur) c.offset += data.length; host.tx.set(data); reply(host, data.length); } };
    const data = await c.dev.read(c.node, n, off, ctx);
    if (data === undefined) return undefined;      // parked in the device
    if (cur) c.offset += data.length;
    host.tx.set(data);
    return data.length;
  }
  case T.PWRITE: {
    const c = fdchk(self, a0);
    const n = Math.min(a2, TXSIZE);
    const cur = (a3 >>> 0) === 0xffffffff && (a4 >>> 0) === 0xffffffff;
    const off = cur ? c.offset : (a4 * 0x100000000 + (a3 >>> 0));
    const wrote = await c.dev.write(c.node, host.tx.subarray(0, n), off);
    if (cur) c.offset += wrote;
    return wrote;
  }
  case T.SEEK: {
    const c = fdchk(self, a0);
    const off = a2 * 0x100000000 + (a1 >>> 0);
    c.offset = a3 === 0 ? off : a3 === 1 ? c.offset + off : c.dev.len(c.node) + off;
    return c.offset;
  }
  case T.STAT: {
    const c = await walk(self, txstr(host));
    const rec = await c.dev.stat(c.node);
    c.dev.discard?.(c.node);
    if (rec.length > a2) throw err("stat buffer too small");
    host.tx.set(rec);
    return rec.length;
  }
  case T.FSTAT: {
    const c = fdchk(self, a0);
    const rec = await c.dev.stat(c.node);
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

// ---- rfork / exec / exits / await ----
function rfork(host, self, flags, guarded) {
  if (!(flags & RF.PROC)) return 0;                     // flag changes on self: v0 no-ops
  if (!guarded)
    throw err("bare rfork(RFPROC) needs asyncify on this engine; use procrfork (v0)");
  if (!(flags & RF.MEM)) throw err("plain fork needs asyncify — v0 is the lazy path");
  if (host.borrower) throw err("nested lazy fork unsupported in v0");
  const child = newProc(self.pid, {
    ns: flags & RF.NAMEG ? new Map(self.ns) : self.ns,
    fdt: flags & RF.FDG ? fdtCopy(self.fdt) : (self.fdt.refs++, self.fdt),
    cwd: self.cwd,
  });
  host.borrower = child;   // child borrows the parent's Worker and stack
  return { ret: 0, aux: child.pid };  // worker saves [0,sp) on seeing aux!=0
}
async function exec(host, self) {
  const path = txstr(host);
  const argv = [];
  for (let o = path.length + 1; host.tx[o];) { const s = txstr(host, o); argv.push(s); o += s.length + 1; }
  const c = await walk(self, path);
  const bytes = await readAll(c);
  new WebAssembly.Module(bytes);                       // validate before committing
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
  fdtClose(self.fdt);                                  // pipes learn their EOF here
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
  self.awaithost = host;
  if (self.zombies.length) return sendWait(self, true);
  self.awaitPending = true;
  return undefined;                                    // blocked until a child dies
}
function sendWait(self, inline = false) {
  const s = Buffer.from(self.zombies.shift().slice(0, self.awaitmax - 1) + "\0");
  const host = self.awaithost;
  host.tx.set(s);
  if (inline) return s.length - 1;
  reply(host, s.length - 1);
}
function shutdown(code) {
  for (const p of procs.values()) p.worker?.terminate();
  setTimeout(() => process.exit(typeof code === "number" ? code : 1), 20);
  return undefined;
}

// ---- boot: pid 1 ----
const init = newProc(0, { ns: new Map(), fdt: newFdt(), cwd: "/" });
spawn(init, readFileSync(join(rootdir, "bin", "init")),
  interactive ? ["init", "-i"] : ["init"]);
