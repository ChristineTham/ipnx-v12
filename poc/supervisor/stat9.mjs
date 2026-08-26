// 9P2000 stat(5) marshalling: size[2] type[2] dev[4] qid.type[1] qid.vers[4]
// qid.path[8] mode[4] atime[4] mtime[4] length[8] name[s] uid[s] gid[s] muid[s]
export const QTDIR = 0x80, QTFILE = 0x00;
export const DMDIR = 0x80000000;

export function marshalStat(d) {
  const s = (x) => { const b = Buffer.from(x, "utf8"); const o = Buffer.alloc(2 + b.length); o.writeUInt16LE(b.length, 0); b.copy(o, 2); return o; };
  const name = s(d.name), uid = s(d.uid ?? "glenda"), gid = s(d.gid ?? "glenda"), muid = s(d.muid ?? "glenda");
  const fixed = Buffer.alloc(2 + 2 + 4 + 13 + 4 + 4 + 4 + 8);
  let o = 2;                                   // size filled last
  fixed.writeUInt16LE(d.type ?? 0, o); o += 2; // type
  fixed.writeUInt32LE(d.dev ?? 0, o); o += 4;  // dev
  fixed.writeUInt8(d.qtype, o); o += 1;        // qid.type
  fixed.writeUInt32LE(d.qvers ?? 0, o); o += 4;
  fixed.writeBigUInt64LE(BigInt(d.qpath), o); o += 8;
  fixed.writeUInt32LE(d.mode >>> 0, o); o += 4;
  fixed.writeUInt32LE(d.atime ?? 0, o); o += 4;
  fixed.writeUInt32LE(d.mtime ?? 0, o); o += 4;
  fixed.writeBigUInt64LE(BigInt(d.length), o); o += 8;
  const all = Buffer.concat([fixed, name, uid, gid, muid]);
  all.writeUInt16LE(all.length - 2, 0);        // size excludes itself
  return all;
}
