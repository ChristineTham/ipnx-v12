// ipnx-v12 poc supervisor: the hosted kernel, platform-neutral.
// One Worker per process; syscalls arrive on a per-process SAB mailbox
// (RESEARCH.md §5.3); the kernel thread never blocks — reads that must wait
// (pipes, the console) park in the device and complete later.
//
// The host (main.mjs on Node, browser/main.mjs in a page) supplies:
//   spawnWorker(initMsg, transfer) -> { post, onMessage, onError, terminate }
//   consWrite(bytes)   the console's output
//   error(text)        kernel-level failures
//   exit(code)         the machine halts
// and calls boot(host, { rootSeed, interactive }), receiving { cons } to
// feed console input into.
import { makeRamfs, makeCons, makePipeDev } from "./devs.mjs";
import { makeMntDev, mountConn } from "./mnt9p.mjs";
import { sbytes, bstr, concat } from "./bytes.mjs";

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

let host, cons, devs;

// ---- union directories ----
// A mount-table entry is a LIST of elements tried in order (bind -a/-b);
// exactly Plan 9's shape. Walking the union point itself yields a synthetic
// chan on UNION; creates go to the first element bound with MCREATE (-c).
const UNION = {
  name: "union",
  walk: async (node, name) => {
    for (const el of node.list) {
      try {
        const n2 = await el.c.dev.walk(el.c.node, name);
        if (n2) return { union: el, node: n2 };   // unwrapped by kernel walk
      } catch { /* try the next element */ }
    }
    return null;
  },
  read: async (node, n, off) => {
    // concatenated listings, still an integral number of records
    let skip = Number(off);
    const out = [];
    let total = 0;
    for (const el of node.list) {
      const listing = await listDir(el.c);
      for (let o = 0; o < listing.length;) {
        const size = listing[o] | (listing[o + 1] << 8);
        const rec = listing.subarray(o, o + size + 2);
        o += size + 2;
        if (skip >= rec.length) { skip -= rec.length; continue; }
        if (total + rec.length > n) return concatOut();
        out.push(rec);
        total += rec.length;
      }
    }
    return concatOut();
    function concatOut() { return concat(out); }
  },
  stat: (node) => node.list[0].c.dev.stat(node.list[0].c.node),
  len: () => 0,
};
async function listDir(c) {                       // full listing of one element
  let { dev, node } = c;
  if (dev.clone && !node.ephemeral) node = await dev.clone(node);
  if (dev.open) await dev.open(node, 0);
  const parts = [];
  for (let off = 0; ;) {
    const chunk = await dev.read(node, 8192, off);
    if (chunk.length === 0) break;
    parts.push(new Uint8Array(chunk));
    off += chunk.length;
  }
  if (node !== c.node) dev.clunk?.(node);
  return concat(parts);
}
async function nsInsert(self, old, chan, flag) {
  const mode = flag & 3, create = !!(flag & 4);
  const el = { c: chan, create };
  if (mode === 0) { self.ns.set(old, [el]); return; }         // MREPL
  let list = self.ns.get(old);
  if (!list) {
    const under = await walk(self, old);                      // must exist, per bind(2)
    list = under.dev === UNION
      ? under.node.list.slice()                               // flatten an existing union
      : [{ c: under, create: false }];
    self.ns.set(old, list);
  }
  if (mode === 2) list.push(el);                              // MAFTER
  else list.unshift(el);                                      // MBEFORE
}

// read a channel from kernel context, parking-aware, as a promise
const readChan = (chan, n) => new Promise((res) => {
  const r = chan.dev.read(chan.node, n, -1, { done: res });
  if (r !== undefined) res(r);
});
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
async function walk(proc, path) {
  path = canon(path, proc.cwd);
  if (path.startsWith("#")) return attach(path);
  let best = "", list = null;
  for (const [pfx, l] of proc.ns)
    if ((path === pfx || path.startsWith(pfx === "/" ? "/" : pfx + "/")) && pfx.length > best.length)
      { best = pfx; list = l; }
  let dev, node;
  if (list) {
    if (list.length === 1) ({ dev, node } = list[0].c);
    else { dev = UNION; node = { list }; }
  } else {
    dev = devs.M; node = devs.M.attach();
  }
  const rest = path.slice(best.length).split("/").filter(Boolean);
  for (const name of rest) {
    let next = await dev.walk(node, name);
    if (next && next.union) { dev = next.union.c.dev; next = next.node; }  // left a union
    if (!next) throw err(`'${path}' does not exist`);
    node = next;
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
    parts.push(new Uint8Array(chunk));
    off += chunk.length;
  }
  c.dev.discard?.(c.node);
  return concat(parts);
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
const nsCopy = (ns) => new Map([...ns].map(([k, v]) => [k, v.slice()]));
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

const isAsyncified = (mod) =>
  WebAssembly.Module.exports(mod).some((e) => e.name === "asyncify_start_unwind");

function spawn(proc, mod, argv, asy = null) {
  proc.mb = new Int32Array(new SharedArrayBuffer(64));
  proc.tx = new Uint8Array(new SharedArrayBuffer(TXSIZE));
  proc.argv = argv;
  proc.module = mod;
  proc.asyncified = isAsyncified(mod);
  const w = host.spawnWorker(
    { t: "init", mb: proc.mb, tx: proc.tx, mod, snap: asy?.snap, dataPtr: asy?.dataPtr },
    asy ? [asy.snap] : []);
  proc.worker = w;
  w.onMessage((m) => {
    if (m.t === "sc") onSyscall(proc);
    // The parent's Worker finished its unwind: the child (whose proc record
    // rfork already made) gets a fresh Worker over the copied memory.
    if (m.t === "asyfork") {
      const child = procs.get(m.pid);
      if (child) spawn(child, proc.module, [...proc.argv], { snap: m.snap, dataPtr: m.dataPtr });
    }
  });
  w.onError((e) => { host.error(`worker pid ${proc.pid}: ${e.message ?? e}`); shutdown(1); });
  return w;
}

const txstr = (proc, off = 0) => {
  const end = proc.tx.indexOf(0, off);
  return bstr(proc.tx.subarray(off, end < 0 ? undefined : end));
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
    if (!e.guest) { host.error(String(e.stack ?? e)); return shutdown(1); }
    self.errstr = e.message;
    reply(proc, -1);
  }
}

async function dispatch(host, self, trap, a0, a1, a2, a3, a4) {
  switch (trap) {
  case T.ARGS: {
    const block = sbytes(self.argv.map((s) => s + "\0").join(""));
    host.tx.set(block.subarray(0, a1));
    return Math.min(block.length, a1);
  }
  case T.BIND: {
    const name = txstr(host), old = txstr(host, name.length + 1);
    const src = await walk(self, name);                          // resolved now, per bind(2)
    await nsInsert(self, canon(old, self.cwd), src, a2);
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
    if (parent.dev === UNION) {
      const el = parent.node.list.find((e) => e.create);
      if (!el) throw err("create in a union needs an element bound with -c (MCREATE)");
      parent = { dev: el.c.dev, node: el.c.node };
    }
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
    await nsInsert(self, canon(old, self.cwd), { dev: devs.m, node }, a3);
    return 0;
  }
  case T.PIPE: {
    const pdev = devs["|"];
    const p = pdev.newPipe();
    const fd0 = fdAlloc(self, { dev: pdev, node: { p, end: 0 }, mode: 2, offset: 0, refs: 1 });
    const fd1 = fdAlloc(self, { dev: pdev, node: { p, end: 1 }, mode: 2, offset: 0, refs: 1 });
    const out = new Uint8Array(8);
    const odv = new DataView(out.buffer);
    odv.setInt32(0, fd0, true); odv.setInt32(4, fd1, true);
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
    const b = sbytes(self.errstr.slice(0, a1 - 1) + "\0");
    host.tx.set(b);
    return b.length - 1;
  }
  case T.SLEEP: setTimeout(() => reply(host, 0), a0); return undefined;
  case T.RFORK: return rfork(host, self, a0, a2);
  case T.EXEC: return exec(host, self, a2);
  case T.EXITS: return exits(host, self);
  case T.AWAIT: return doAwait(host, self, a1);
  default: throw err(`bad syscall ${trap} (v0)`);
  }
}

// ---- rfork / exec / exits / await ----
function rfork(host, self, flags, marker) {
  if (!(flags & RF.PROC)) return 0;                     // flag changes on self: v0 no-ops
  if (host.borrower) throw err("nested lazy fork unsupported in v0");
  if (marker === 2) {                                   // asyncify: bare dual return
    if (!self.asyncified)
      throw err("not an asyncify build — add it to ASYNCIFY in poc/mk.sh, or use procrfork");
    if (flags & RF.MEM) throw err("RFMEM is the lazy path's flag; a bare fork copies");
    const child = newProc(self.pid, {
      ns: flags & RF.NAMEG ? nsCopy(self.ns) : self.ns,
      fdt: flags & RF.FDG ? fdtCopy(self.fdt) : (self.fdt.refs++, self.fdt),
      cwd: self.cwd,
    });
    child.module = self.module;
    child.asyncified = true;
    return child.pid;   // one kernel return; the worker synthesizes the two
  }
  if (marker !== 1)
    throw err("bare rfork(RFPROC) needs an asyncify build; procrfork is the exec path");
  if (!(flags & RF.MEM)) throw err("plain fork needs asyncify — the guard path is lazy");
  const child = newProc(self.pid, {
    ns: flags & RF.NAMEG ? nsCopy(self.ns) : self.ns,
    fdt: flags & RF.FDG ? fdtCopy(self.fdt) : (self.fdt.refs++, self.fdt),
    cwd: self.cwd,
  });
  host.borrower = child;   // child borrows the parent's Worker and stack
  return { ret: 0, aux: child.pid };  // worker saves [0,sp) on seeing aux!=0
}
async function exec(host, self, argc) {
  const path = txstr(host);
  const argv = [];
  for (let o = path.length + 1, i = 0; i < argc; i++) {   // counted: "" survives
    const s = txstr(host, o);
    argv.push(s);
    o += s.length + 1;
  }
  const c = await walk(self, path);
  const mod = await WebAssembly.compile(await readAll(c));
  self.argv = argv.length ? argv : [path];
  if (host.borrower === self) {
    // Lazy-fork child leaves the borrowed stack for its own instance; the
    // parent's Worker restores [0,sp) and unwinds to the rfork guard (§5.2).
    host.borrower = null;
    spawn(self, mod, self.argv);
    return { ret: R_FORKRESUME, aux: self.pid };
  }
  self.module = mod;
  self.asyncified = isAsyncified(mod);
  host.worker.post({ t: "load", mod });
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
  const s = sbytes(self.zombies.shift().slice(0, self.awaitmax - 1) + "\0");
  const host = self.awaithost;
  host.tx.set(s);
  if (inline) return s.length - 1;
  reply(host, s.length - 1);
}
function shutdown(code) {
  for (const p of procs.values()) p.worker?.terminate();
  setTimeout(() => host.exit(typeof code === "number" ? code : 1), 20);
  return undefined;
}

// ---- boot: pid 1 ----
export async function boot(theHost, { rootSeed, interactive }) {
  host = theHost;
  cons = makeCons(host.consWrite);
  devs = { M: makeRamfs(rootSeed), c: cons, "|": makePipeDev(), m: makeMntDev() };
  const init = newProc(0, { ns: new Map(), fdt: newFdt(), cwd: "/" });
  const image = await readAll(await walk(init, "/bin/init"));
  spawn(init, await WebAssembly.compile(image), interactive ? ["init", "-i"] : ["init"]);
  return { cons };
}
