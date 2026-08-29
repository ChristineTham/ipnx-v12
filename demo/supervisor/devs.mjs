// Devices: each presents attach/walk/open/read/write/stat over its tree —
// Plan 9's kernel-device shape (intro(3)); only the mount driver (mnt9p.mjs)
// marshals wire 9P.
//
// A device read may PARK: return undefined and complete later via ctx.done(buf)
// — that is how pipe reads and console reads block their caller without
// blocking the kernel. Platform-neutral: no Node APIs, no Buffer.
import { marshalStat, QTDIR, QTFILE, QTSYMLINK, DMDIR, DMSYMLINK } from "./stat9.mjs";
import { concat } from "./bytes.mjs";

let qgen = 1;
const empty = new Uint8Array(0);
const derr = (m) => Object.assign(new Error(m), { guest: true });

// ---- ramfs: an in-memory tree, seeded from a host-supplied structure ----
// seed: { name, dir, kids: [seed...], data: Uint8Array }
// Enforcement here is V10's regime (docs/uid.md): every node carries uid and
// mode, checked against the caller's credential; eve bypasses as root does.
export function makeRamfs(seed, eve = "kitty") {
  const now = () => Math.floor(Date.now() / 1000);
  const boot = now();
  function load(s, name) {
    if (s.dir) {
      const node = { name, qpath: qgen++, dir: true, kids: new Map(), uid: eve, mode: 0o755, atime: boot, mtime: boot };
      for (const k of s.kids) node.kids.set(k.name, load(k, k.name));
      return node;
    }
    return { name, qpath: qgen++, dir: false, data: s.data, uid: eve, mode: 0o644, atime: boot, mtime: boot };
  }
  const root = load(seed, "/");
  if (!root.kids.has("tmp"))                      // a writable corner, always
    root.kids.set("tmp", { name: "tmp", qpath: qgen++, dir: true, kids: new Map(), uid: eve, mode: 0o777 });
  const statNode = (k) => marshalStat({
    name: k.name, uid: k.uid, qpath: k.qpath, atime: k.atime, mtime: k.mtime,
    qtype: k.dir ? QTDIR : k.symlink !== undefined ? QTSYMLINK : QTFILE,
    mode: ((k.dir ? DMDIR : k.symlink !== undefined ? DMSYMLINK : 0) | k.mode) >>> 0,
    length: k.dir ? 0 : k.symlink !== undefined ? k.symlink.length : k.data.length,
  });
  const access = (node, cred, want) => {          // want: 4 read, 2 write
    if (!cred || cred.euid === eve) return;
    const bits = node.uid === cred.euid ? (node.mode >> 6) : node.mode;
    if ((bits & want) !== want)
      throw derr(`permission denied ('${node.name}' is ${node.uid}'s, mode ${(node.mode & 0o777).toString(8)})`);
  };
  return {
    name: "ramfs",
    attach: () => root,
    walk: (node, name) => (node.dir ? node.kids.get(name) ?? null : null),
    open: (node, mode, cred) => {
      const rw = mode & 3;                        // OREAD 0, OWRITE 1, ORDWR 2, OEXEC 3
      let want = rw === 1 ? 2 : rw === 2 ? 6 : 4;
      if (mode & 16) want |= 2;                   // OTRUNC
      access(node, cred, want);
      if ((mode & 16) && !node.dir) node.data = empty;
    },
    read: (node, n, off) => {
      if (node.dir) {
        // read(5): "an integral number of directory entries" — never split one
        let skip = Number(off);
        const out = [];
        let total = 0;
        for (const k of node.kids.values()) {
          const rec = statNode(k);
          if (skip >= rec.length) { skip -= rec.length; continue; }
          if (total + rec.length > n) break;
          out.push(rec);
          total += rec.length;
        }
        return concat(out);
      }
      const end = Math.min(Number(off) + n, node.data.length);
      return node.data.subarray(Math.min(Number(off), node.data.length), end);
    },
    write: (node, data, off) => {
      if (node.dir) throw derr("write on directory");
      const o = Number(off), end = o + data.length;
      if (end > node.data.length) {
        const grown = new Uint8Array(end);
        grown.set(node.data);
        node.data = grown;
      }
      node.data.set(data, o);
      node.mtime = now();
      delete node._mod;
      return data.length;
    },
    create: (parent, name, perm, isdir, omode, cred) => {
      if (!parent.dir) throw derr("create in non-directory");
      access(parent, cred, 2);
      const old = parent.kids.get(name);
      if (old) {                                   // create(2): existing file truncates
        if (old.dir || isdir) throw derr(`'${name}' already exists`);
        access(old, cred, 2);
        old.data = empty;
        return old;
      }
      const node = isdir
        ? { name, qpath: qgen++, dir: true, kids: new Map() }
        : { name, qpath: qgen++, dir: false, data: empty };
      node.uid = cred?.euid ?? eve;
      node.mode = perm & 0o7777;
      node.atime = node.mtime = now();
      parent.mtime = now();
      parent.kids.set(name, node);
      return node;
    },
    remove: (parent, name, cred) => {
      const node = parent.kids.get(name);
      if (!node) throw derr(`'${name}' does not exist`);
      access(parent, cred, 2);
      if (node.dir && node.kids.size) throw derr("directory not empty");
      parent.kids.delete(name);
    },
    link: (parent, name, node, cred) => {        // two names, one node — a hard link
      if (!parent.dir) throw derr("link into a non-directory");
      access(parent, cred, 2);
      if (parent.kids.has(name)) throw derr(`'${name}' already exists`);
      if (node.dir) throw derr("cannot hard-link a directory");
      parent.kids.set(name, node);
    },
    symlink: (parent, name, target, cred) => {
      if (!parent.dir) throw derr("symlink into a non-directory");
      access(parent, cred, 2);
      if (parent.kids.has(name)) throw derr(`'${name}' already exists`);
      parent.kids.set(name, { name, qpath: qgen++, dir: false, symlink: target,
        uid: cred?.euid ?? eve, mode: 0o777 });
    },
    readlink: (node) => {
      if (node.symlink === undefined) throw derr("not a symlink");
      return node.symlink;
    },
    wstat: (node, ch, cred, rawRec, parent, base) => {  // chmod/chown/rename/touch
      if (ch.mode !== undefined) {
        if (cred && cred.euid !== eve && cred.euid !== node.uid)
          throw derr(`not owner of '${node.name}'`);
        node.mode = ch.mode & (0o7777 | 0x000C0000); // permission + DMSETUID/DMSETGID
      }
      if (ch.uid !== undefined) {
        if (cred && cred.euid !== eve)
          throw derr("only the host owner may chown (docs/uid.md D3)");
        node.uid = ch.uid;
      }
      if (ch.name !== undefined && parent && ch.name !== base) {
        if (cred && cred.euid !== eve && cred.euid !== node.uid)
          throw derr(`not owner of '${node.name}'`);
        if (parent.kids.has(ch.name)) throw derr(`'${ch.name}' already exists`);
        parent.kids.delete(base);
        node.name = ch.name;
        parent.kids.set(ch.name, node);
        parent.mtime = now();
      }
      if (ch.mtime !== undefined) node.mtime = ch.mtime;
    },
    truncate: (node) => { if (!node.dir) { node.data = empty; delete node._mod; } },
    stat: statNode,
    len: (node) => (node.dir ? 0 : node.data.length),
    // graft: merge a seed-shaped tree into the LIVE root — how the demo
    // streams toolchain overlays in after boot (files land whole; a driver's
    // probe file ships in an overlay's last part, so probe-passing means the
    // toolchain is entirely aboard)
    graft: (seed) => {
      const t = now();
      const add = (node, sk) => {
        for (const k of sk.kids ?? []) {
          if (k.dir) {
            let d = node.kids.get(k.name);
            if (!d || !d.dir) {
              d = { name: k.name, qpath: qgen++, dir: true, kids: new Map(), uid: node.uid, mode: 0o755, atime: t, mtime: t };
              node.kids.set(k.name, d);
            }
            add(d, k);
          } else {
            node.kids.set(k.name, { name: k.name, qpath: qgen++, dir: false, data: k.data, uid: node.uid, mode: 0o755, atime: t, mtime: t });
          }
        }
      };
      add(root, seed);
    },
  };
}

// ---- cons: '#c' — console; write to the host's output, read what it feeds ----
export function makeCons(out) {
  const root = { name: "/", qpath: qgen++, dir: true };
  const cons = { name: "cons", qpath: qgen++, dir: false };
  const userf = { name: "user", qpath: qgen++, dir: false };
  const pidf = { name: "pid", qpath: qgen++, dir: false };
  const nullf = { name: "null", qpath: qgen++, dir: false };
  let buf = empty, eof = false;
  const parked = [];
  const serve = () => {
    while (parked.length && (buf.length || eof)) {
      const { n, ctx } = parked.shift();
      const give = buf.subarray(0, Math.min(n, buf.length));
      buf = buf.subarray(give.length);
      ctx.done(give);                              // empty at eof: read returns 0
    }
  };
  return {
    name: "cons",
    attach: () => root,
    walk: (node, name) =>
      node === root
        ? ({ cons, user: userf, pid: pidf, null: nullf }[name] ?? null)
        : null,
    read: (node, n, off, ctx) => {
      if (node === userf)                          // /dev/user: the caller's euid
        return Number(off) === 0 ? new TextEncoder().encode(ctx?.cred?.euid ?? "") : empty;
      if (node === pidf)                           // #c/pid: what getpid(2) reads
        return Number(off) === 0 ? new TextEncoder().encode(String(ctx?.pid ?? 0)) : empty;
      if (node === nullf) return empty;
      if (node !== cons) return empty;
      if (buf.length || eof) {
        const give = buf.subarray(0, Math.min(n, buf.length));
        buf = buf.subarray(give.length);
        return give;
      }
      parked.push({ n, ctx });
      return undefined;                            // parked
    },
    write: (node, data) => {
      if (node === nullf) return data.length;
      out(new Uint8Array(data));
      return data.length;
    },
    stat: (node) => marshalStat({
      name: node.name, qtype: node.dir ? QTDIR : QTFILE, qpath: node.qpath,
      mode: node.dir ? DMDIR | 0o555 : 0o666, length: 0,
    }),
    len: () => 0,
    feed: (chunk) => { buf = concat([buf, chunk]); serve(); },
    end: () => { eof = true; serve(); },
  };
}

// ---- pipe: '#|' — Plan 9 pipes are bidirectional; two queues, two ends ----
// Writes at end e land in q[e]; reads at end e drain q[1^e]; EOF when the
// other end's refs hit zero and its queue is dry. Writers never block in v0.
export function makePipeDev() {
  const dev = {
    name: "pipe",
    newPipe: () => ({ q: [[], []], nbytes: [0, 0], refs: [1, 1], parked: [[], []] }),
    read: (node, n, off, ctx) => {
      const { p, end } = node, d = 1 ^ end;
      if (p.nbytes[d] > 0) return drain(p, d, n);
      if (p.refs[d] === 0) return empty;           // EOF
      p.parked[d].push({ n, ctx });
      return undefined;
    },
    write: (node, data) => {
      const { p, end } = node;
      if (p.refs[1 ^ end] === 0) throw derr("write on closed pipe");
      p.q[end].push(new Uint8Array(data));         // copy: tx is reused
      p.nbytes[end] += data.length;
      serve(p, end);
      return data.length;
    },
    clunk: (node) => {
      const { p, end } = node;
      if (--p.refs[end] === 0) serve(p, end);      // wake readers: data then EOF
    },
    stat: () => marshalStat({ name: "data", qtype: QTFILE, qpath: qgen++, mode: 0o600, length: 0 }),
    len: () => 0,
  };
  function drain(p, d, want) {
    const out = [];
    let got = 0;
    while (p.q[d].length && got < want) {
      const head = p.q[d][0];
      const take = Math.min(head.length, want - got);
      out.push(head.subarray(0, take));
      got += take;
      if (take === head.length) p.q[d].shift();
      else p.q[d][0] = head.subarray(take);
    }
    p.nbytes[d] -= got;
    return concat(out);
  }
  function serve(p, d) {
    while (p.parked[d].length && (p.nbytes[d] > 0 || p.refs[d] === 0)) {
      const { n, ctx } = p.parked[d].shift();
      ctx.done(p.nbytes[d] > 0 ? drain(p, d, n) : empty);
    }
  }
  return dev;
}

// ---- webfs: '#H' — the network as files, in webfs(4)'s spirit ----
// '#H/<hex-of-utf8-url>' reads as the body of a GET of that URL. The fetch is
// the host's (a browser fetch obeys CORS — PyPI permits it; Node's does not
// care), starts at open, and reads slice the settled body — the read is
// async, so a failed fetch surfaces as an ordinary guest error, not a hang.
// Read-only, 64MB cap. This is the demo lineage's device; the /net milestone
// (implementation.md) replaces it with wire-native networking.
export function makeWebfs() {
  const root = { name: "/", qpath: qgen++, dir: true };
  const dec = (hex) => {
    if (!/^[0-9a-f]+$/.test(hex) || hex.length % 2) throw derr("bad url encoding");
    const b = new Uint8Array(hex.length / 2);
    for (let i = 0; i < b.length; i++) b[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
    return new TextDecoder().decode(b);
  };
  return {
    name: "webfs",
    attach: () => root,
    walk: (node, name) => {
      if (node !== root) return null;
      let url;
      try { url = dec(name); } catch { return null; }
      if (!/^https?:\/\//.test(url)) return null;
      return { name, qpath: qgen++, dir: false, url };
    },
    open: (node) => {
      if (node.dir) return;
      node.body ??= (async () => {
        const res = await fetch(node.url, { redirect: "follow" });
        if (!res.ok) throw derr(`GET ${node.url}: ${res.status} ${res.statusText}`);
        const buf = new Uint8Array(await res.arrayBuffer());
        if (buf.length > 64 * 1024 * 1024) throw derr("response over 64MB");
        return buf;
      })();
    },
    read: async (node, n, off) => {
      if (node.dir) return empty;
      let body;
      try { body = await node.body; }
      catch (e) { throw e.guest ? e : derr(String(e.message ?? e)); }
      const end = Math.min(Number(off) + n, body.length);
      return body.subarray(Math.min(Number(off), body.length), end);
    },
    write: () => { throw derr("webfs is read-only"); },
    stat: (node) => marshalStat({
      name: node.name, qtype: node.dir ? QTDIR : QTFILE, qpath: node.qpath,
      mode: node.dir ? DMDIR | 0o555 : 0o444, length: 0,
    }),
    len: () => 0,
  };
}
