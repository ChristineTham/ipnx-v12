// devmnt: the mount driver — the ONE place the kernel marshals wire 9P.
// Everything below the Dev interface here is 9P2000 messages on a channel
// (usually a pipe end whose other side is a guest file server).
//
// v0 deviations: no Tauth (afid is always NOFID), no Tflush, walks are one
// name per Twalk (the kernel resolves component-wise), msize fixed at 8216.

const MSIZE = 8216;                // 8192 data + IOHDRSZ(24)
const NOFID = 0xffffffff;
const Tv = { version: 100, attach: 104, walk: 110, open: 112, create: 114,
  read: 116, write: 118, clunk: 120, remove: 122, stat: 124, wstat: 126 };
const Rerror = 107;

// ---- marshal helpers ----
class W {
  constructor() { this.parts = []; }
  u8(v) { this.parts.push(Buffer.from([v & 0xff])); return this; }
  u16(v) { const b = Buffer.alloc(2); b.writeUInt16LE(v); this.parts.push(b); return this; }
  u32(v) { const b = Buffer.alloc(4); b.writeUInt32LE(v >>> 0); this.parts.push(b); return this; }
  u64(v) { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(v)); this.parts.push(b); return this; }
  s(str) { const d = Buffer.from(str, "utf8"); this.u16(d.length); this.parts.push(d); return this; }
  raw(b) { this.parts.push(b); return this; }
  frame(type, tag) {
    const body = Buffer.concat(this.parts);
    const hdr = Buffer.alloc(7);
    hdr.writeUInt32LE(7 + body.length, 0);
    hdr.writeUInt8(type, 4);
    hdr.writeUInt16LE(tag, 5);
    return Buffer.concat([hdr, body]);
  }
}
class R {
  constructor(buf) { this.b = buf; this.o = 0; }
  u8() { return this.b.readUInt8(this.o++); }
  u16() { const v = this.b.readUInt16LE(this.o); this.o += 2; return v; }
  u32() { const v = this.b.readUInt32LE(this.o); this.o += 4; return v; }
  u64() { const v = this.b.readBigUInt64LE(this.o); this.o += 8; return v; }
  s() { const n = this.u16(); const v = this.b.toString("utf8", this.o, this.o + n); this.o += n; return v; }
  qid() { return { type: this.u8(), vers: this.u32(), path: this.u64() }; }
  rest() { return this.b.subarray(this.o); }
}

const err = (msg) => Object.assign(new Error(msg), { guest: true });

// ---- the connection: framing, tags, demux ----
class Conn {
  constructor(chan, readChan) {
    this.chan = chan;              // kernel chan to the transport (incref'd by mount)
    this.readChan = readChan;      // (chan, n) -> Promise<Buffer>
    this.tags = new Map();
    this.nexttag = 1;
    this.nextfid = 1;
    this.rbuf = Buffer.alloc(0);
    this.dead = null;
    this.reader();
  }
  async reader() {
    try {
      for (;;) {
        while (this.rbuf.length < 4) await this.fill();
        const size = this.rbuf.readUInt32LE(0);
        while (this.rbuf.length < size) await this.fill();
        const msg = this.rbuf.subarray(0, size);
        this.rbuf = this.rbuf.subarray(size);
        const type = msg.readUInt8(4), tag = msg.readUInt16LE(5);
        const pend = this.tags.get(tag);
        this.tags.delete(tag);
        if (!pend) continue;                       // stray tag: drop
        if (type === Rerror) pend.reject(err(new R(msg.subarray(7)).s()));
        else if (type !== pend.rtype) pend.reject(err(`9P: expected R${pend.rtype}, got ${type}`));
        else pend.resolve(new R(msg.subarray(7)));
      }
    } catch (e) {
      this.dead = err(`mount server closed: ${e.message}`);
      for (const p of this.tags.values()) p.reject(this.dead);
      this.tags.clear();
    }
  }
  async fill() {
    const chunk = await this.readChan(this.chan, MSIZE);
    if (chunk.length === 0) throw new Error("eof");
    this.rbuf = Buffer.concat([this.rbuf, chunk]);
  }
  rpc(type, w) {
    if (this.dead) return Promise.reject(this.dead);
    const tag = this.nexttag++ & 0xffff;
    return new Promise((resolve, reject) => {
      this.tags.set(tag, { resolve, reject, rtype: type + 1 });
      this.chan.dev.write(this.chan.node, w.frame(type, tag), -1);
    });
  }
  async version() {
    const r = await this.rpc(Tv.version, new W().u32(MSIZE).s("9P2000"));
    const msize = r.u32(), ver = r.s();
    if (ver !== "9P2000") throw err(`server speaks '${ver}', not 9P2000`);
    return msize;
  }
  async attach(aname, uname = "glenda") {
    const fid = this.nextfid++;
    const r = await this.rpc(Tv.attach, new W().u32(fid).u32(NOFID).s(uname).s(aname));
    return { conn: this, fid, qid: r.qid(), ephemeral: false, opened: false };
  }
  clunk(fid) {                     // fire-and-forget: nothing to do with the answer
    this.rpc(Tv.clunk, new W().u32(fid)).catch(() => {});
  }
}

// ---- the Dev ----
export function makeMntDev() {
  return {
    name: "mnt",
    clone: async (node) => {                       // Twalk, zero names: a fresh fid
      const { conn } = node;
      const newfid = conn.nextfid++;
      await conn.rpc(Tv.walk, new W().u32(node.fid).u32(newfid).u16(0));
      return { conn, fid: newfid, qid: node.qid, ephemeral: true, opened: false };
    },
    walk: async (node, name) => {
      const { conn } = node;
      const newfid = conn.nextfid++;
      const r = await conn.rpc(Tv.walk, new W().u32(node.fid).u32(newfid).u16(1).s(name));
      if (r.u16() !== 1) throw err(`'${name}' does not exist`);
      const qid = r.qid();
      if (node.ephemeral) conn.clunk(node.fid);
      return { conn, fid: newfid, qid, ephemeral: true, opened: false };
    },
    open: async (node, mode) => {
      await node.conn.rpc(Tv.open, new W().u32(node.fid).u8(mode));
      node.ephemeral = false;
      node.opened = true;
    },
    create: async (parent, name, perm, isdir, mode = 1 /*OWRITE*/) => {
      const { conn } = parent;
      await conn.rpc(Tv.create, new W().u32(parent.fid).s(name).u32(perm >>> 0).u8(mode));
      // Tcreate: the fid now represents (and has opened) the new file
      return { conn, fid: parent.fid, qid: parent.qid, ephemeral: false, opened: true };
    },
    remove: async (parent, name) => {
      const { conn } = parent;
      const fid = conn.nextfid++;
      const r = await conn.rpc(Tv.walk, new W().u32(parent.fid).u32(fid).u16(1).s(name));
      if (r.u16() !== 1) throw err(`'${name}' does not exist`);
      await conn.rpc(Tv.remove, new W().u32(fid));   // remove clunks the fid, per remove(5)
    },
    read: async (node, n, off) => {
      const count = Math.min(n, MSIZE - 24);
      const r = await node.conn.rpc(Tv.read, new W().u32(node.fid).u64(off).u32(count));
      const got = r.u32();
      return r.rest().subarray(0, got);
    },
    write: async (node, data, off) => {
      const d = data.subarray(0, Math.min(data.length, MSIZE - 24));
      const r = await node.conn.rpc(Tv.write,
        new W().u32(node.fid).u64(off < 0 ? 0 : off).u32(d.length).raw(Buffer.from(d)));
      return r.u32();
    },
    stat: async (node) => {
      const r = await node.conn.rpc(Tv.stat, new W().u32(node.fid));
      r.u16();                                       // outer size, per stat(5)'s double count
      return r.rest();                               // the record, self-sized
    },
    clunk: (node) => node.conn.clunk(node.fid),
    discard: (node) => { if (node.ephemeral) node.conn.clunk(node.fid); },
    len: () => 0,
  };
}

export async function mountConn(chan, readChan, aname) {
  const conn = new Conn(chan, readChan);
  await conn.version();
  return await conn.attach(aname);
}
