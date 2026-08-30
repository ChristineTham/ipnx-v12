// Byte helpers over Uint8Array — the supervisor's only string/binary
// vocabulary, so the same kernel runs on Node and in the browser.
const te = new TextEncoder(), td = new TextDecoder();

export const sbytes = (s) => te.encode(s);
// Browsers refuse to decode a view over a SharedArrayBuffer (Node does not),
// so copy shared views out first — measured, not folklore (RESEARCH §5.3).
export const bstr = (b) =>
  td.decode(typeof SharedArrayBuffer !== "undefined" && b.buffer instanceof SharedArrayBuffer ? b.slice() : b);

export function concat(list) {
  let n = 0;
  for (const b of list) n += b.length;
  const out = new Uint8Array(n);
  let o = 0;
  for (const b of list) { out.set(b, o); o += b.length; }
  return out;
}

export const dv = (b) => new DataView(b.buffer, b.byteOffset, b.byteLength);
export const g16 = (b, o) => dv(b).getUint16(o, true);
export const g32 = (b, o) => dv(b).getUint32(o, true);
export const g64 = (b, o) => dv(b).getBigUint64(o, true);
