// rustkern: the Rust kernel core (browserhost.wasm) behind the SAME boot()
// interface kernel.mjs serves — the browser surface's kernel half, M5's
// last named step landed. Structurally parallel to hosts/macos/main.rs:
// entries mirror Ev, the drain mirrors run_effects; Workers, mailboxes and
// guestcore.mjs are untouched (the guest world cannot tell the kernels
// apart — that is the point). Platform-neutral: Node's harness
// (main-rust.mjs) and the browser pages both drive this file.
const ST = { IDLE: 0, REQ: 1, DONE: 2 };
const TXSIZE = 65536;
const R_FORKRESUME = -1000, R_EXECSELF = -1001, R_RETIRE = -2000, R_DIE = -3000;
const T_STR = new Set([2, 3, 7, 8, 14, 22, 25, 41, 42, 44, 60, 61, 62]);
const KINDNAME = ["stack", "text", "edit", "path"];
const td = new TextDecoder();
const te = new TextEncoder();

// how many tx bytes a trap carried (guestcore's marshalling, mirrored)
function txPrefix(tx, trap, a) {
  const nthNul = (k) => {
    let o = 0;
    for (let i = 0; i < k; i++) {
      const e = tx.indexOf(0, o);
      if (e < 0) return tx.length;
      o = e + 1;
    }
    return o;
  };
  if (trap === 45) return Math.min(a[2], TXSIZE);
  if (trap === 51) return Math.min(a[2], TXSIZE);
  if (trap === 35 || trap === 46) return nthNul(2);
  if (!T_STR.has(trap)) return 0;
  if (trap === 7) return nthNul(1 + a[2]);
  if (trap === 2 || trap === 60 || trap === 61) return nthNul(2);
  if (trap === 44) return nthNul(1) + a[2];
  return nthNul(1);
}

export async function boot(host, opts) {
  const { rootSeed, interactive = false, kernelWasm, caps = null,
          verbose = false, hostServe = null } = opts;

  // ---- the core ----
  let inst;
  const imports = { env: { bh_log: (p, n) => host.error?.(td.decode(mem().subarray(p, p + n))) } };
  const mod = kernelWasm instanceof WebAssembly.Module
    ? kernelWasm : await WebAssembly.compile(kernelWasm);
  inst = await WebAssembly.instantiate(mod, imports);
  const X = inst.exports;
  const mem = () => new Uint8Array(X.memory.buffer);

  const push = (bytes) => {
    const p = X.bh_alloc(bytes.length || 1);
    if (bytes.length) mem().set(bytes, p);
    return p;
  };
  const withBytes = (bytes, f) => {
    const p = push(bytes);
    try { return f(p, bytes.length); } finally { X.bh_free(p, bytes.length || 1); }
  };

  // seed serializer (parse_seed's mirror): node = [dir u8][dir? u32 n +
  // per-kid u16 name + node : u32 len + data]
  function seedSize(n) {
    if (n.dir) {
      let s = 1 + 4;
      for (const k of n.kids ?? []) s += 2 + te.encode(k.name).length + seedSize(k);
      return s;
    }
    return 1 + 4 + (n.data?.length ?? 0);
  }
  function seedWrite(v, dv, o, n) {
    v[o] = n.dir ? 1 : 0; o += 1;
    if (n.dir) {
      const kids = n.kids ?? [];
      dv.setUint32(o, kids.length, true); o += 4;
      for (const k of kids) {
        const nm = te.encode(k.name);
        dv.setUint16(o, nm.length, true); o += 2;
        v.set(nm, o); o += nm.length;
        o = seedWrite(v, dv, o, k);
      }
    } else {
      const d = n.data ?? new Uint8Array(0);
      dv.setUint32(o, d.length, true); o += 4;
      v.set(d, o); o += d.length;
    }
    return o;
  }
  function seedBlob(seed) {
    const v = new Uint8Array(seedSize(seed));
    seedWrite(v, new DataView(v.buffer), 0, seed);
    return v;
  }

  // every entry stamps the clock, runs, then pumps the drain
  let pumping = false, repump = false;
  function call(f, ...args) {
    X.bh_clock(Date.now());
    f(...args);
    schedulePump();
  }
  function schedulePump() {
    if (pumping) { repump = true; return; }
    pumping = true;
    (async () => {
      try {
        do { repump = false; await pump(); } while (repump);
      } finally { pumping = false; }
    })().catch((e) => host.error?.(String(e?.stack ?? e)));
  }

  // ---- workers (kernel.mjs's pool, verbatim shape) ----
  const procs = new Map(); // pid -> { mb, tx, worker }
  const workerPool = [];
  const modules = new Map(); // image id -> Promise<Module>
  const isAsyncified = (m) =>
    WebAssembly.Module.exports(m).some((e) => e.name === "asyncify_start_unwind");

  function spawnGuest(pid, mod2, asy) {
    const proc = { pid, mb: new Int32Array(new SharedArrayBuffer(64)),
                   tx: new Uint8Array(new SharedArrayBuffer(TXSIZE)), worker: null };
    procs.set(pid, proc);
    const asyf = isAsyncified(mod2);
    call(X.bh_started, pid, asyf ? 1 : 0);
    const initMsg = { t: "init", mb: proc.mb, tx: proc.tx, mod: mod2,
      snap: asy?.snap, dataPtr: asy?.dataPtr, sp: asy?.sp,
      maxPages: asyf ? 256 : 80 };
    let w = workerPool.pop();
    if (w) w.post(initMsg, asy ? [asy.snap] : []);
    else w = host.spawnWorker(initMsg, asy ? [asy.snap] : []);
    proc.worker = w;
    w.setHandler((m) => {
      if (m.t === "sc") onSyscall(proc);
      if (m.t === "asyfork") {
        const b = new Uint8Array(m.snap);
        withBytes(b, (p2, n2) => call(X.bh_asyfork, pid, m.pid, p2, n2, m.dataPtr, m.sp));
      }
    });
    w.onError((e) => {
      const msg = te.encode(String(e?.message ?? e));
      host.error?.(`worker pid ${pid}: ${String(e?.message ?? e)}`);
      withBytes(msg, (p, n) => call(X.bh_died, pid, p, n));
    });
    return proc;
  }

  function onSyscall(proc) {
    const [, trap, a0, a1, a2, a3, a4] = proc.mb;
    const a = [a0, a1, a2, a3, a4];
    const n = txPrefix(proc.tx, trap, a);
    const bytes = n ? proc.tx.slice(0, n) : new Uint8Array(0);
    withBytes(bytes, (p, len) => call(X.bh_syscall, proc.pid, trap, a0, a1, a2, a3, a4, p, len));
  }

  function reply(proc, ret, aux, note, data) {
    if (data && data.length) proc.tx.set(data.subarray(0, TXSIZE));
    proc.mb[10] = note ? 1 : 0;
    proc.mb[8] = ret | 0;
    proc.mb[9] = aux | 0;
    Atomics.store(proc.mb, 0, ST.DONE);
    Atomics.notify(proc.mb, 0);
  }

  function retireWorker(proc) {
    if (proc.worker) {
      proc.worker.setHandler(() => {});
      workerPool.push(proc.worker);
      proc.worker = null;
    }
    procs.delete(proc.pid);
  }

  async function moduleById(id, bytes) {
    if (!modules.has(id)) modules.set(id, WebAssembly.compile(bytes));
    return modules.get(id);
  }

  // ---- window bookkeeping over the effect stream ----
  const wins = new Map(); // wid -> { x, y, w, h, label, made }
  function winSync(wid, label, x, y, w2, h) {
    let gw = wins.get(wid);
    if (!gw) {
      gw = { x, y, w: w2, h, label, made: true };
      wins.set(wid, gw);
      host.winCreate?.(wid, x, y, w2, h, label);
      return gw;
    }
    if (gw.x !== x || gw.y !== y || gw.w !== w2 || gw.h !== h) {
      Object.assign(gw, { x, y, w: w2, h });
      host.winGeom?.(wid, x, y, w2, h);
    }
    if (gw.label !== label) {
      gw.label = label;
      host.winLabel?.(wid, label);
    }
    return gw;
  }

  // ---- the drain ----
  async function pump() {
    for (;;) {
      const ptr = X.bh_drain();
      const dv = new DataView(X.memory.buffer);
      const len = dv.getUint32(ptr, true);
      const blob = mem().slice(ptr + 4, ptr + 4 + len);
      const b = new DataView(blob.buffer);
      let o = 0;
      const u8 = () => blob[o++];
      const u16 = () => { const v = b.getUint16(o, true); o += 2; return v; };
      const u32 = () => { const v = b.getUint32(o, true); o += 4; return v; };
      const i32 = () => { const v = b.getInt32(o, true); o += 4; return v; };
      const u64 = () => { const lo = u32(), hi = u32(); return lo + hi * 4294967296; };
      const take = (n) => { const v = blob.subarray(o, o + n); o += n; return v; };
      const str16 = () => td.decode(take(u16()));
      const bytes32 = () => take(u32());

      const ncompl = u32();
      let acted = ncompl > 0;
      for (let i = 0; i < ncompl; i++) {
        const pid = u32();
        const ret = i32();
        const aux = i32();
        const action = u8();
        const note = u8();
        const data = bytes32();
        const hasLoad = u8();
        let load = null;
        if (hasLoad) {
          const id = u32();
          const lb = bytes32();
          load = { id, bytes: lb.length ? lb.slice() : null };
        }
        const proc = procs.get(pid);
        if (!proc) continue;
        if (action === 0) reply(proc, ret, aux, note, data);
        else if (action === 1) reply(proc, R_FORKRESUME, aux, 0, data);
        else if (action === 2) {
          const mod2 = await moduleById(load.id, load.bytes);
          call(X.bh_started, pid, isAsyncified(mod2) ? 1 : 0);
          reply(proc, R_EXECSELF, 0, 0, null);
          proc.worker.post({ t: "load", mod: mod2 });
        } else if (action === 3) {
          reply(proc, R_RETIRE, 0, 0, null);
          retireWorker(proc);
        } else if (action === 4) {
          reply(proc, R_DIE, 0, 0, null);
          retireWorker(proc);
        }
      }

      const ne = u32();
      acted = acted || ne > 0;
      for (let i = 0; i < ne; i++) {
        const tag = u8();
        if (tag === 1) { // Spawn
          const pid = u32();
          const id = u32();
          const ib = bytes32();
          let asy = null;
          if (u8()) {
            const snap = bytes32().slice().buffer;
            const dataPtr = u32();
            const sp = u32();
            asy = { snap, dataPtr, sp };
          }
          const mod2 = await moduleById(id, ib.length ? ib.slice() : null);
          spawnGuest(pid, mod2, asy);
        } else if (tag === 2) {
          host.consWrite?.(bytes32().slice());
        } else if (tag === 3) {
          const ms = u64();
          const token = u64();
          setTimeout(() => call(X.bh_timer, token), ms);
        } else if (tag === 4) {
          const code = i32();
          host.exit?.(code);
        } else if (tag === 5) { // WinUpdate
          const wid = u32();
          const label = str16();
          const x = i32(), y = i32(), w2 = i32(), h = i32();
          const rgba = bytes32();
          winSync(wid, label, x, y, w2, h);
          host.winPresent?.(wid, w2, h, rgba.slice());
        } else if (tag === 6) {
          const wid = u32();
          wins.delete(wid);
          host.winClose?.(wid);
        } else if (tag === 7) {
          const wid = u32();
          host.winText?.(wid, bytes32().slice());
        } else if (tag === 13) { // WinChrome — /dev/window's declared furniture
          const wid = u32();
          host.winChrome?.(wid, { type: str16(), content: str16(), toolbar: str16(), tag: str16() });
        } else if (tag === 8) { // WinCanvas
          const wid = u32();
          const label = str16();
          const x = i32(), y = i32(), w2 = i32(), h = i32();
          const nn = u32();
          const snap = [];
          for (let j = 0; j < nn; j++) {
            const nid = u32();
            const kind = KINDNAME[u8()] ?? "text";
            const na = u16();
            const attrs = {};
            for (let a2 = 0; a2 < na; a2++) {
              const k2 = str16();
              attrs[k2] = str16();
            }
            snap.push({ id: nid, kind, attrs, data: td.decode(bytes32()) });
          }
          winSync(wid, label, x, y, w2, h);
          host.winCanvas?.(wid, snap);
          call(X.bh_win_ack, wid);   // the browser's rAF is the presenter's own credit
        } else if (tag === 9) { // Fetch
          const url = str16();
          (async () => {
            try {
              const r = await fetch(url);
              if (!r.ok) throw new Error(`GET ${url}: ${r.status}`);
              const body = new Uint8Array(await r.arrayBuffer());
              withBytes(te.encode(url), (up, un) =>
                withBytes(body, (bp, bn) => call(X.bh_fetch_done, up, un, 1, bp, bn)));
            } catch (e) {
              const msg = te.encode(String(e?.message ?? e));
              withBytes(te.encode(url), (up, un) =>
                withBytes(msg, (bp, bn) => call(X.bh_fetch_done, up, un, 0, bp, bn)));
            }
          })();
        } else if (tag === 10) {
          host.snarfSet?.(td.decode(bytes32()));
        } else if (tag === 11) { // SnarfGet
          (async () => {
            let got = null;
            try { got = await host.snarfGet?.(); } catch { got = null; }
            if (typeof got === "string") {
              withBytes(te.encode(got), (p, n) => call(X.bh_snarf_done, 1, p, n));
            } else {
              call(X.bh_snarf_done, 0, 0, 0);
            }
          })();
        } else if (tag === 12) { // Host op -> the platform's file server
          const t2 = u64();
          const kind = u8();
          const op = { tag: t2, kind };
          op.path = str16();
          if (kind === 2) { op.off = u64(); op.n = u32(); }
          if (kind === 3) { op.off = u64(); op.data = bytes32().slice(); }
          if (kind === 4) { op.dir = u8() !== 0; op.perm = u32(); }
          (async () => {
            let ok = 1, payload;
            try {
              if (!hostRef.serve) throw new Error("no host directory granted (#Z) — use the home menu");
              payload = hostReplyBlob(await hostRef.serve(op));
            } catch (e) {
              ok = 0;
              payload = te.encode(String(e?.message ?? e));
            }
            withBytes(payload, (p, n) => call(X.bh_hostop_done, t2, ok, p, n));
          })();
        }
      }
      if (!acted) break;
    }
  }

  // HostReply serializer: mirrors bh_hostop_done's reader
  function hostReplyBlob(r) {
    const parts = [];
    const w = { v: new Uint8Array(64), o: 0 };
    const need = (n) => {
      if (w.o + n > w.v.length) {
        const nv = new Uint8Array(Math.max(w.v.length * 2, w.o + n));
        nv.set(w.v.subarray(0, w.o));
        w.v = nv;
      }
    };
    const p8 = (x) => { need(1); w.v[w.o++] = x; };
    const p16 = (x) => { need(2); new DataView(w.v.buffer).setUint16(w.o, x, true); w.o += 2; };
    const p32 = (x) => { need(4); new DataView(w.v.buffer).setUint32(w.o, x >>> 0, true); w.o += 4; };
    const p64 = (x) => { p32(x % 4294967296); p32(Math.floor(x / 4294967296)); };
    const pb32 = (b) => { p32(b.length); need(b.length); w.v.set(b, w.o); w.o += b.length; };
    const ps16 = (s) => { const b = te.encode(s); p16(b.length); need(b.length); w.v.set(b, w.o); w.o += b.length; };
    if (r.kind === "missing") p8(0);
    else if (r.kind === "meta") { p8(1); p8(r.dir ? 1 : 0); p64(r.len); p32(r.mtime); p64(r.ino); p32(r.mode); }
    else if (r.kind === "bytes") { p8(2); pb32(r.bytes); }
    else if (r.kind === "entries") {
      p8(3); p32(r.entries.length);
      for (const e of r.entries) { ps16(e.name); p8(e.dir ? 1 : 0); p64(e.len); p32(e.mtime); p64(e.ino); p32(e.mode); }
    } else p8(4);
    void parts;
    return w.v.subarray(0, w.o);
  }

  const hostRef = { serve: hostServe };

  // ---- boot ----
  withBytes(seedBlob(rootSeed), (p, n) => { X.bh_clock(Date.now()); X.bh_init(p, n, interactive ? 1 : 0, verbose ? 1 : 0, hostServe ? 1 : 0); });
  if (caps) withBytes(te.encode(caps), (p, n) => X.bh_canvas_caps(p, n));
  if (caps && caps.includes("interactive")) setInterval(() => call(X.bh_win_tick), 33);
  withBytes(te.encode(interactive ? "init\0-i\0" : "init\0"), (p, n) => call(X.bh_boot, p, n));

  return {
    cons: {
      feed: (bytes) => withBytes(bytes, (p, n) => call(X.bh_cons_feed, p, n)),
      end: () => call(X.bh_cons_end),
    },
    wsys: {
      mouse: (wid, x, y, b) => call(X.bh_win_mouse, wid, x, y, b),
      key: (wid, byte) => withBytes(new Uint8Array([byte]), (p, n) => call(X.bh_win_key, wid, p, n)),
      close: (wid) => call(X.bh_win_close, wid),
      canvasEvent: (wid, line) => withBytes(te.encode(line), (p, n) => call(X.bh_canvas_event, wid, p, n)),
      winEvent: (wid, line) => withBytes(te.encode(line), (p, n) => call(X.bh_win_event, wid, p, n)),
      snarfPut: (text) => withBytes(te.encode(text), (p, n) => call(X.bh_snarf_put, p, n)),
    },
    graft: (seed) => withBytes(seedBlob(seed), (p, n) => call(X.bh_graft, p, n)),
    grantHostfs: (serve) => { hostRef.serve = serve; },
  };
}

// The browser's '#Z' op server over a FileSystemDirectoryHandle (OPFS or a
// picked folder — the two persistent homes speak one API; devhostfs.mjs's
// logic re-homed onto the delegated-op protocol).
export function makeHandleHostServer() {
  let root = null;
  const walk = async (path, forWrite = false) => {
    if (!root) throw new Error("no host directory granted (#Z)");
    let h = root;
    const parts = path.split("/").filter(Boolean);
    for (let i = 0; i < parts.length; i++) {
      const last = i === parts.length - 1;
      try {
        h = await h.getDirectoryHandle(parts[i]);
        continue;
      } catch {}
      if (!last) throw new Error(`${path}: does not exist`);
      h = await h.getFileHandle(parts[i], { create: forWrite });
    }
    return h;
  };
  const serve = async (op) => {
    const { kind, path } = op;
    if (kind === 1) { // Meta
      let h;
      try { h = await walk(path); } catch { return { kind: "missing" }; }
      if (h.kind === "directory")
        return { kind: "meta", dir: true, len: 0, mtime: 0, ino: 1, mode: 0o755 };
      const f = await h.getFile();
      return { kind: "meta", dir: false, len: f.size,
               mtime: Math.floor(f.lastModified / 1000), ino: 1, mode: 0o644 };
    }
    if (kind === 2) { // Read
      const h = await walk(path);
      const f = await h.getFile();
      const o = Math.min(op.off, f.size);
      return { kind: "bytes",
               bytes: new Uint8Array(await f.slice(o, Math.min(o + op.n, f.size)).arrayBuffer()) };
    }
    if (kind === 3) { // Write: read-splice-rewrite (createWritable truncates)
      const h = await walk(path, true);
      const f = await h.getFile();
      const old = new Uint8Array(await f.arrayBuffer());
      const grown = new Uint8Array(Math.max(old.length, op.off + op.data.length));
      grown.set(old);
      grown.set(op.data, op.off);
      const w = await h.createWritable();
      await w.write(grown);
      await w.close();
      return { kind: "unit" };
    }
    if (kind === 4) { // Create
      const i = path.lastIndexOf("/");
      const dirPath = i < 0 ? "" : path.slice(0, i);
      const name = path.slice(i + 1);
      let ph = root;
      for (const part of dirPath.split("/").filter(Boolean))
        ph = await ph.getDirectoryHandle(part);
      if (op.dir) await ph.getDirectoryHandle(name, { create: true });
      else {
        const fh = await ph.getFileHandle(name, { create: true });
        const w = await fh.createWritable();
        await w.close();
      }
      return { kind: "unit" };
    }
    if (kind === 5) { // Remove
      const i = path.lastIndexOf("/");
      const dirPath = i < 0 ? "" : path.slice(0, i);
      const name = path.slice(i + 1);
      let ph = root;
      for (const part of dirPath.split("/").filter(Boolean))
        ph = await ph.getDirectoryHandle(part);
      await ph.removeEntry(name);
      return { kind: "unit" };
    }
    if (kind === 6) { // Trunc
      try {
        const h = await walk(path);
        if (h.kind === "file") {
          const w = await h.createWritable();
          await w.close();
        }
      } catch {}
      return { kind: "unit" };
    }
    if (kind === 7) { // ReadDir
      const h = path === "" ? root : await walk(path);
      const entries = [];
      for await (const e of h.values()) {
        if (e.kind === "directory")
          entries.push({ name: e.name, dir: true, len: 0, mtime: 0, ino: 1, mode: 0o755 });
        else {
          const f = await e.getFile();
          entries.push({ name: e.name, dir: false, len: f.size,
                         mtime: Math.floor(f.lastModified / 1000), ino: 1, mode: 0o644 });
        }
      }
      entries.sort((a, b) => a.name < b.name ? -1 : 1);
      return { kind: "entries", entries };
    }
    throw new Error(`hostfs: unknown op ${kind}`);
  };
  serve.grant = (handle) => { root = handle; };
  serve.granted = () => root != null;
  return serve;
}
