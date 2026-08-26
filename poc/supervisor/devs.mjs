// Devices: each presents attach/walk/open/read/write/stat over its tree —
// Plan 9's kernel-device shape (intro(3)); wire 9P would slot in as one more
// dev (the mount driver) speaking the same interface.
//
// A device read may PARK: return undefined and complete later via ctx.done(buf)
// — that is how pipe reads and console reads block their caller without
// blocking the kernel.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { marshalStat, QTDIR, QTFILE, DMDIR } from "./stat9.mjs";

let qgen = 1;

// ---- ramfs: an in-memory tree, seeded from a host directory at boot ----
export function makeRamfs(hostdir) {
  function load(dir, name) {
    const st = statSync(dir);
    if (st.isDirectory()) {
      const node = { name, qpath: qgen++, dir: true, kids: new Map() };
      for (const e of readdirSync(dir)) node.kids.set(e, load(join(dir, e), e));
      return node;
    }
    return { name, qpath: qgen++, dir: false, data: new Uint8Array(readFileSync(dir)) };
  }
  const root = load(hostdir, "/");
  if (!root.kids.has("tmp"))                      // a writable corner, always
    root.kids.set("tmp", { name: "tmp", qpath: qgen++, dir: true, kids: new Map() });
  const statNode = (k) => marshalStat({
    name: k.name, qtype: k.dir ? QTDIR : QTFILE, qpath: k.qpath,
    mode: (k.dir ? DMDIR | 0o755 : 0o644), length: k.dir ? 0 : k.data.length,
  });
  return {
    name: "ramfs",
    attach: () => root,
    walk: (node, name) => (node.dir ? node.kids.get(name) ?? null : null),
    read: (node, n, off) => {
      if (node.dir) {
        // read(5): "an integral number of directory entries" — never split one
        let skip = Number(off), out = [];
        for (const k of node.kids.values()) {
          const rec = statNode(k);
          if (skip >= rec.length) { skip -= rec.length; continue; }
          if (out.reduce((a, r) => a + r.length, 0) + rec.length > n) break;
          out.push(rec);
        }
        return Buffer.concat(out);
      }
      const end = Math.min(Number(off) + n, node.data.length);
      return node.data.subarray(Math.min(Number(off), node.data.length), end);
    },
    write: (node, data, off) => {
      if (node.dir) throw new Error("write on directory");
      const o = Number(off), end = o + data.length;
      if (end > node.data.length) {
        const grown = new Uint8Array(end);
        grown.set(node.data);
        node.data = grown;
      }
      node.data.set(data, o);
      return data.length;
    },
    create: (parent, name, perm, isdir) => {
      if (!parent.dir) throw new Error("create in non-directory");
      const old = parent.kids.get(name);
      if (old) {                                   // create(2): existing file truncates
        if (old.dir || isdir) throw new Error(`'${name}' already exists`);
        old.data = new Uint8Array(0);
        return old;
      }
      const node = isdir
        ? { name, qpath: qgen++, dir: true, kids: new Map() }
        : { name, qpath: qgen++, dir: false, data: new Uint8Array(0) };
      parent.kids.set(name, node);
      return node;
    },
    remove: (parent, name) => {
      const node = parent.kids.get(name);
      if (!node) throw new Error(`'${name}' does not exist`);
      if (node.dir && node.kids.size) throw new Error("directory not empty");
      parent.kids.delete(name);
    },
    truncate: (node) => { if (!node.dir) node.data = new Uint8Array(0); },
    stat: statNode,
    len: (node) => (node.dir ? 0 : node.data.length),
  };
}

// ---- cons: '#c' — console; write to host stdout, read from host stdin ----
export function makeCons(out = process.stdout) {
  const root = { name: "/", qpath: qgen++, dir: true };
  const cons = { name: "cons", qpath: qgen++, dir: false };
  let buf = Buffer.alloc(0), eof = false;
  const parked = [];
  const serve = () => {
    while (parked.length && (buf.length || eof)) {
      const { n, ctx } = parked.shift();
      const give = buf.subarray(0, Math.min(n, buf.length));
      buf = buf.subarray(give.length);
      ctx.done(give);                              // empty at eof: read returns 0
    }
  };
  const dev = {
    name: "cons",
    attach: () => root,
    walk: (node, name) => (node === root && name === "cons" ? cons : null),
    read: (node, n, off, ctx) => {
      if (node !== cons) return Buffer.alloc(0);
      if (buf.length || eof) {
        const give = buf.subarray(0, Math.min(n, buf.length));
        buf = buf.subarray(give.length);
        return give;
      }
      parked.push({ n, ctx });
      return undefined;                            // parked
    },
    write: (node, data) => { out.write(Buffer.from(data)); return data.length; },
    stat: (node) => marshalStat({
      name: node.name, qtype: node.dir ? QTDIR : QTFILE, qpath: node.qpath,
      mode: node.dir ? DMDIR | 0o555 : 0o666, length: 0,
    }),
    len: () => 0,
    feed: (chunk) => { buf = Buffer.concat([buf, chunk]); serve(); },
    end: () => { eof = true; serve(); },
  };
  return dev;
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
      if (p.refs[d] === 0) return Buffer.alloc(0); // EOF
      p.parked[d].push({ n, ctx });
      return undefined;
    },
    write: (node, data) => {
      const { p, end } = node;
      if (p.refs[1 ^ end] === 0) throw new Error("write on closed pipe");
      p.q[end].push(Buffer.from(data));            // copy: tx is reused
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
    return Buffer.concat(out);
  }
  function serve(p, d) {
    while (p.parked[d].length && (p.nbytes[d] > 0 || p.refs[d] === 0)) {
      const { n, ctx } = p.parked[d].shift();
      ctx.done(p.nbytes[d] > 0 ? drain(p, d, n) : Buffer.alloc(0));
    }
  }
  return dev;
}
