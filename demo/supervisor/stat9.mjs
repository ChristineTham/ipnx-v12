// 9P2000 stat(5) marshalling: size[2] type[2] dev[4] qid.type[1] qid.vers[4]
// qid.path[8] mode[4] atime[4] mtime[4] length[8] name[s] uid[s] gid[s] muid[s]
import { sbytes, bstr, concat } from "./bytes.mjs";

export const QTDIR = 0x80, QTFILE = 0x00, QTSYMLINK = 0x02;  // qid bit: 9P2000.u's
export const DMDIR = 0x80000000;
export const DMSETUID = 0x00080000;   // 9P2000.u's bit position, adopted (docs/uid.md)
export const DMSYMLINK = 0x02000000;  // 9P2000.u's bit position

export function marshalStat(d) {
  const s = (x) => {
    const b = sbytes(x);
    const o = new Uint8Array(2 + b.length);
    new DataView(o.buffer).setUint16(0, b.length, true);
    o.set(b, 2);
    return o;
  };
  const name = s(d.name), uid = s(d.uid ?? "kitty"), gid = s(d.gid ?? "kitty"), muid = s(d.muid ?? "kitty");
  const fixed = new Uint8Array(2 + 2 + 4 + 13 + 4 + 4 + 4 + 8);
  const v = new DataView(fixed.buffer);
  let o = 2;                                    // size filled last
  v.setUint16(o, d.type ?? 0, true); o += 2;
  v.setUint32(o, d.dev ?? 0, true); o += 4;
  v.setUint8(o, d.qtype); o += 1;
  v.setUint32(o, d.qvers ?? 0, true); o += 4;
  v.setBigUint64(o, BigInt(d.qpath), true); o += 8;
  v.setUint32(o, d.mode >>> 0, true); o += 4;
  v.setUint32(o, d.atime ?? 0, true); o += 4;
  v.setUint32(o, d.mtime ?? 0, true); o += 4;
  v.setBigUint64(o, BigInt(d.length), true); o += 8;
  const all = concat([fixed, name, uid, gid, muid]);
  new DataView(all.buffer).setUint16(0, all.length - 2, true);  // size excludes itself
  return all;
}

// Read one stat(5) record. For wstat, "don't touch" is ~0 for integers and
// the zero-length string for names, per stat(5) — the caller checks.
export function parseStat(b) {
  const v = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const str = (o) => {
    const n = v.getUint16(o, true);
    return { s: bstr(b.subarray(o + 2, o + 2 + n)), next: o + 2 + n };
  };
  const mode = v.getUint32(21, true);
  const atime = v.getUint32(25, true);
  const mtime = v.getUint32(29, true);
  const length = v.getBigUint64(33, true);
  let o = 41;
  const name = str(o); o = name.next;
  const uid = str(o); o = uid.next;
  const gid = str(o);
  return { mode, atime, mtime, length, name: name.s, uid: uid.s, gid: gid.s };
}
