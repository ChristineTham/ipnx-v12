// Devices: each presents attach/walk/open/read/write/stat over its tree —
// Plan 9's kernel-device shape (intro(3)); wire 9P would slot in as one more
// dev (the mount driver) speaking the same interface.
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
  return {
    name: "ramfs",
    attach: () => root,
    walk: (node, name) => (node.dir ? node.kids.get(name) ?? null : null),
    read: (node, n, off) => {
      if (node.dir) {  // dirread: consecutive stat records (read(5) on a directory)
        const recs = [...node.kids.values()].map((k) => statNode(k));
        const all = Buffer.concat(recs);
        return all.subarray(Math.min(off, all.length), Math.min(off + n, all.length));
      }
      const end = Math.min(Number(off) + n, node.data.length);
      return node.data.subarray(Math.min(Number(off), node.data.length), end);
    },
    write: () => { throw new Error("ramfs is read-only in v0"); },
    stat: (node) => statNode(node),
    len: (node) => (node.dir ? 0 : node.data.length),
  };
  function statNode(k) {
    return marshalStat({
      name: k.name, qtype: k.dir ? QTDIR : QTFILE, qpath: k.qpath,
      mode: (k.dir ? DMDIR | 0o755 : 0o644), length: k.dir ? 0 : k.data.length,
    });
  }
}

// ---- cons: '#c' — the console device; write goes to the host's stdout ----
export function makeCons(out = process.stdout) {
  const root = { name: "/", qpath: qgen++, dir: true };
  const cons = { name: "cons", qpath: qgen++, dir: false };
  return {
    name: "cons",
    attach: () => root,
    walk: (node, name) => (node === root && name === "cons" ? cons : null),
    read: (node) => (node === cons ? Buffer.alloc(0) : null),  // EOF in v0
    write: (node, data) => { out.write(Buffer.from(data)); return data.length; },
    stat: (node) => marshalStat({
      name: node.name, qtype: node.dir ? QTDIR : QTFILE, qpath: node.qpath,
      mode: node.dir ? DMDIR | 0o555 : 0o666, length: 0,
    }),
    len: () => 0,
  };
}
