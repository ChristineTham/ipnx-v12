// devhostfs: '#Z' in the browser — the SAME hostfs letter and contract as
// the native host, over the File System Access API instead of std::fs.
// A granted directory IS a bind (the doctrine line platforms.md wrote for
// iPad's bookmarks): the user picks a folder, the handle becomes the
// device root, and real files serve the namespace. OPFS handles speak the
// identical API, so the device also serves the origin-private tree when a
// page hands one in. Everything here is async; the kernel awaits devices
// throughout. Writes are read-splice-rewrite (createWritable truncates by
// default); adequate for editing, honest about cost.
import { marshalStat, QTDIR, QTFILE, DMDIR } from "./stat9.mjs";
import { concat, sbytes } from "./bytes.mjs";

export function makeBrowserHostfs() {
  const err9 = (m) => Object.assign(new Error(m), { guest: true });
  let root = null;                 // FileSystemDirectoryHandle, once granted
  let qgen = 1;
  const qids = new Map();          // path -> stable qid
  const qid = (path) => {
    if (!qids.has(path)) qids.set(path, 0x5a0000 + qgen++);
    return qids.get(path);
  };

  const node = (handle, path) => ({ handle, path, dir: handle.kind === "directory" });

  async function statOf(n) {
    let length = 0;
    if (!n.dir) {
      const f = await n.handle.getFile();
      length = f.size;
    }
    return marshalStat({
      name: n.path === "/" ? "/" : n.path.slice(n.path.lastIndexOf("/") + 1),
      uid: "kitty", qpath: qid(n.path),
      qtype: n.dir ? QTDIR : QTFILE,
      mode: ((n.dir ? DMDIR | 0o755 : 0o644) >>> 0),
      length,
    });
  }

  return {
    name: "hostfs",
    granted: () => root != null,
    grant: (handle) => { root = handle; },
    attach: () => {
      if (root == null) throw err9("no host directory granted (#Z) — use Folder…");
      return node(root, "/");
    },
    walk: async (n, name) => {
      if (!n.dir) return null;
      try {
        return node(await n.handle.getDirectoryHandle(name), n.path + (n.path === "/" ? "" : "/") + name);
      } catch { /* not a directory by that name */ }
      try {
        return node(await n.handle.getFileHandle(name), n.path + (n.path === "/" ? "" : "/") + name);
      } catch {
        return null;
      }
    },
    open: async (n, mode) => {
      if ((mode & 16) && !n.dir) {           // OTRUNC
        const w = await n.handle.createWritable();
        await w.close();                     // truncated by default
      }
    },
    read: async (n, count, off) => {
      if (n.dir) {
        let skip = Number(off);
        const out = [];
        let total = 0;
        for await (const h of n.handle.values()) {
          const rec = await statOf(node(h, n.path + (n.path === "/" ? "" : "/") + h.name));
          if (skip >= rec.length) { skip -= rec.length; continue; }
          if (total + rec.length > count) break;
          out.push(rec);
          total += rec.length;
        }
        return concat(out);
      }
      const f = await n.handle.getFile();
      const o = Math.min(Number(off), f.size);
      const buf = await f.slice(o, Math.min(o + count, f.size)).arrayBuffer();
      return new Uint8Array(buf);
    },
    write: async (n, data, off) => {
      if (n.dir) throw err9("write on directory");
      const f = await n.handle.getFile();
      const old = new Uint8Array(await f.arrayBuffer());
      const o = Number(off);
      const grown = new Uint8Array(Math.max(old.length, o + data.length));
      grown.set(old);
      grown.set(new Uint8Array(data), o);
      const w = await n.handle.createWritable();
      await w.write(grown);
      await w.close();
      return data.length;
    },
    create: async (parent, name, perm, isdir) => {
      if (!parent.dir) throw err9("create in non-directory");
      const h = isdir
        ? await parent.handle.getDirectoryHandle(name, { create: true })
        : await parent.handle.getFileHandle(name, { create: true });
      if (!isdir) {                          // create(2) truncates an existing file
        const w = await h.createWritable();
        await w.close();
      }
      return node(h, parent.path + (parent.path === "/" ? "" : "/") + name);
    },
    remove: async (parent, name) => {
      await parent.handle.removeEntry(name).catch(() => {
        throw err9(`'${name}' does not exist`);
      });
    },
    truncate: async (n) => {
      if (n.dir) return;
      const w = await n.handle.createWritable();
      await w.close();
    },
    stat: (n) => statOf(n),
    len: async (n) => (n.dir ? 0 : (await n.handle.getFile()).size),
  };
}
