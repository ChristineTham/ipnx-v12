// 9P2000 stat(5) marshalling: size[2] type[2] dev[4] qid.type[1] qid.vers[4]
// qid.path[8] mode[4] atime[4] mtime[4] length[8] name[s] uid[s] gid[s] muid[s]
import { sbytes, concat } from "./bytes.mjs";

export const QTDIR = 0x80, QTFILE = 0x00;
export const DMDIR = 0x80000000;

export function marshalStat(d) {
  const s = (x) => {
    const b = sbytes(x);
    const o = new Uint8Array(2 + b.length);
    new DataView(o.buffer).setUint16(0, b.length, true);
    o.set(b, 2);
    return o;
  };
  const name = s(d.name), uid = s(d.uid ?? "glenda"), gid = s(d.gid ?? "glenda"), muid = s(d.muid ?? "glenda");
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
