// 9P2000, as the page speaks it: the messages emca's file server reads and
// writes (intro(5) and the pages after it; `fcall.h`'s numbers). The kernel
// is the client — its mount driver — and this is the other end of `#9/1`.

export const T = {
  version: 100, auth: 102, attach: 104, error: 106, flush: 108, walk: 110,
  open: 112, create: 114, read: 116, write: 118, clunk: 120, remove: 122,
  stat: 124, wstat: 126,
};
export const NOTAG = 0xffff;
export const NOFID = 0xffffffff;
export const QTDIR = 0x80;
export const DMDIR = 0x80000000;
export const OTRUNC = 0x10;

const enc = new TextEncoder();
const dec = new TextDecoder();

// A message read: its type, tag and fields in order.
export class Reader {
  constructor(b) {
    this.b = b;
    this.d = new DataView(b.buffer, b.byteOffset, b.byteLength);
    this.at = 0;
  }
  u8() { return this.d.getUint8(this.at++); }
  u16() { const v = this.d.getUint16(this.at, true); this.at += 2; return v; }
  u32() { const v = this.d.getUint32(this.at, true); this.at += 4; return v; }
  u64() { const v = this.d.getBigUint64(this.at, true); this.at += 8; return v; }
  s() { const n = this.u16(); const v = dec.decode(this.b.subarray(this.at, this.at + n)); this.at += n; return v; }
  bytes(n) { const v = this.b.subarray(this.at, this.at + n); this.at += n; return v; }
}

// A message written: fields appended, then framed with size[4] type[1] tag[2].
export class Writer {
  constructor() { this.parts = []; this.n = 0; }
  push(b) { this.parts.push(b); this.n += b.length; return this; }
  u8(v) { return this.push(Uint8Array.of(v)); }
  u16(v) { const b = new Uint8Array(2); new DataView(b.buffer).setUint16(0, v, true); return this.push(b); }
  u32(v) { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, v >>> 0, true); return this.push(b); }
  u64(v) { const b = new Uint8Array(8); new DataView(b.buffer).setBigUint64(0, BigInt(v), true); return this.push(b); }
  s(v) { const b = enc.encode(v); return this.u16(b.length).push(b); }
  data(b) { return this.u32(b.length).push(b); }
  qid(q) { return this.u8(q.type).u32(q.vers ?? 0).u64(q.path); }
  bytes() {
    const out = new Uint8Array(this.n);
    let at = 0;
    for (const p of this.parts) { out.set(p, at); at += p.length; }
    return out;
  }
  frame(type, tag) {
    const body = this.bytes();
    const out = new Uint8Array(7 + body.length);
    const d = new DataView(out.buffer);
    d.setUint32(0, out.length, true);
    d.setUint8(4, type);
    d.setUint16(5, tag, true);
    out.set(body, 7);
    return out;
  }
}

// One directory entry, stat(5)'s machine-independent form.
export function dir({ qid, mode, length = 0, name, user = 'emca', mtime = 0 }) {
  const w = new Writer().u16(0).u32(0).qid(qid).u32(mode).u32(mtime).u32(mtime).u64(length).s(name).s(user).s(user).s(user);
  const b = w.bytes();
  new DataView(b.buffer).setUint16(0, b.length - 2, true);
  return b;
}

export const error = (tag, msg) => new Writer().s(msg).frame(T.error + 1, tag);
