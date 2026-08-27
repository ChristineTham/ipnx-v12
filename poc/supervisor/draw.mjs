// The draw engine behind #w's per-window /dev/draw: RGBA rasters and the
// draw(3) operations. Values are "low-order byte first", per draw(3);
// colours are RGBA32 (DBlack = 0x000000FF). An image may be a VIEW onto
// another image's backing store — that is what a window on a screen is —
// and every write clips against r ∩ clipr, per draw(2).
//
// Uploads ('y') arrive in the image's own channel format; GREY1 (fonts),
// GREY2/4/8, RGB24 and RGBA32/XRGB32 decode into the RGBA raster, greys
// landing in alpha too so subfont images work directly as masks.

// back = {data, w, h, x0, y0}: an RGBA raster whose (x0,y0) is the
// backing's coordinate origin. A plain image owns its backing; a view
// shares its parent's.
export function newImage(r, opts = {}) {
  const { repl = 0, chan = 0x08182848, color = null, back = null } = opts;
  const [x0, y0, x1, y1] = r;
  const w = x1 - x0, h = y1 - y0;
  const img = {
    r: [...r], clipr: opts.clipr ? [...opts.clipr] : [...r],
    w, h, repl, chan,
    back: back ?? { data: new Uint8Array(w * h * 4), w, h, x0, y0 },
  };
  if (color !== null) fillRect(img, img.r, color);
  return img;
}

export const rgba32 = (v) => [(v >>> 24) & 255, (v >>> 16) & 255, (v >>> 8) & 255, v & 255];

// channel descriptor: 4 packed bytes, high first, each (type<<4)|nbits;
// types per draw.h: CRed 0, CGreen 1, CBlue 2, CGrey 3, CAlpha 4, CMap 5, CIgnore 6
export function chanparts(chan) {
  const out = [];
  for (let s = 24; s >= 0; s -= 8) {
    const b = (chan >>> s) & 255;
    if (b) out.push({ type: b >> 4, n: b & 15 });
  }
  return out;
}
export const chandepth = (chan) => chanparts(chan).reduce((a, c) => a + c.n, 0);

// backing index of (x,y) for reads: repl tiles r over all space
const px = (img, x, y) => {
  if (img.repl) {
    x = img.r[0] + (((x - img.r[0]) % img.w) + img.w) % img.w;
    y = img.r[1] + (((y - img.r[1]) % img.h) + img.h) % img.h;
  } else if (x < img.r[0] || x >= img.r[2] || y < img.r[1] || y >= img.r[3])
    return -1;
  const bx = x - img.back.x0, by = y - img.back.y0;
  if (bx < 0 || bx >= img.back.w || by < 0 || by >= img.back.h) return -1;
  return (by * img.back.w + bx) * 4;
};
export const pxOf = px;

// writable index: inside r ∩ clipr and the backing
const wpx = (img, x, y) => {
  if (x < img.r[0] || x >= img.r[2] || y < img.r[1] || y >= img.r[3]) return -1;
  const c = img.clipr;
  if (x < c[0] || x >= c[2] || y < c[1] || y >= c[3]) return -1;
  const bx = x - img.back.x0, by = y - img.back.y0;
  if (bx < 0 || bx >= img.back.w || by < 0 || by >= img.back.h) return -1;
  return (by * img.back.w + bx) * 4;
};

function blend(dst, di, src, si) {
  const sa = src[si + 3];
  if (sa === 255) {
    dst[di] = src[si]; dst[di + 1] = src[si + 1]; dst[di + 2] = src[si + 2]; dst[di + 3] = 255;
    return;
  }
  const ka = 255 - sa;
  dst[di] = src[si] + ((dst[di] * ka / 255) | 0);
  dst[di + 1] = src[si + 1] + ((dst[di + 1] * ka / 255) | 0);
  dst[di + 2] = src[si + 2] + ((dst[di + 2] * ka / 255) | 0);
  dst[di + 3] = sa + ((dst[di + 3] * ka / 255) | 0);
}

export function fillRect(img, r, rgba) {
  for (let y = r[1]; y < r[3]; y++)
    for (let x = r[0]; x < r[2]; x++) {
      const di = wpx(img, x, y);
      if (di >= 0) img.back.data.set(rgba, di);
    }
}

// d: combine src (aligned at sp with dstr.min) into dst over dstr
export function drawOp(dst, dstr, src, sp) {
  const [dx0, dy0, dx1, dy1] = dstr;
  for (let y = dy0; y < dy1; y++)
    for (let x = dx0; x < dx1; x++) {
      const di = wpx(dst, x, y);
      if (di < 0) continue;
      const si = px(src, sp[0] + (x - dx0), sp[1] + (y - dy0));
      if (si < 0) continue;
      blend(dst.back.data, di, src.back.data, si);
    }
}

export function line(dst, p0, p1, src, sp) {
  let [x0, y0] = p0;
  const [x1, y1] = p1;
  const dx = Math.abs(x1 - x0), dy = -Math.abs(y1 - y0);
  const sx = x0 < x1 ? 1 : -1, sy = y0 < y1 ? 1 : -1;
  let e = dx + dy;
  for (;;) {
    const di = wpx(dst, x0, y0);
    if (di >= 0) {
      const si = px(src, sp[0], sp[1]);
      if (si >= 0) blend(dst.back.data, di, src.back.data, si);
    }
    if (x0 === x1 && y0 === y1) break;
    const e2 = 2 * e;
    if (e2 >= dy) { e += dy; x0 += sx; }
    if (e2 <= dx) { e += dx; y0 += sy; }
  }
}

export function ellipse(dst, c, a, b, src, sp, filled) {
  const [cx, cy] = c;
  const aa = a * a || 1, bb = b * b || 1;
  for (let y = -b; y <= b; y++)
    for (let x = -a; x <= a; x++) {
      const v = (x * x) / aa + (y * y) / bb;
      const inside = v <= 1;
      const edge = !filled && inside &&
        (((x + 1) * (x + 1)) / aa + (y * y) / bb > 1 ||
         ((x - 1) * (x - 1)) / aa + (y * y) / bb > 1 ||
         (x * x) / aa + ((y + 1) * (y + 1)) / bb > 1 ||
         (x * x) / aa + ((y - 1) * (y - 1)) / bb > 1);
      if (filled ? inside : edge) {
        const di = wpx(dst, cx + x, cy + y);
        if (di < 0) continue;
        const si = px(src, sp[0], sp[1]);
        if (si >= 0) blend(dst.back.data, di, src.back.data, si);
      }
    }
}

// s: blit rectangle maskr of mask into dst at p, colouring by src — the
// per-glyph half of the string message; mask alpha scales src alpha.
export function glyph(dst, p, mask, maskr, src, sp) {
  const [mx0, my0, mx1, my1] = maskr;
  for (let y = my0; y < my1; y++)
    for (let x = mx0; x < mx1; x++) {
      const mi = px(mask, x, y);
      if (mi < 0) continue;
      const ma = mask.back.data[mi + 3];
      if (ma === 0) continue;
      const di = wpx(dst, p[0] + (x - mx0), p[1] + (y - my0));
      if (di < 0) continue;
      const si = px(src, sp[0], sp[1]);
      if (si < 0) continue;
      const s = src.back.data;
      blend2(dst.back.data, di,
        (s[si] * ma / 255) | 0, (s[si + 1] * ma / 255) | 0,
        (s[si + 2] * ma / 255) | 0, (s[si + 3] * ma / 255) | 0);
    }
}
function blend2(dst, di, r, g, b, a) {
  const ka = 255 - a;
  dst[di] = r + ((dst[di] * ka / 255) | 0);
  dst[di + 1] = g + ((dst[di + 1] * ka / 255) | 0);
  dst[di + 2] = b + ((dst[di + 2] * ka / 255) | 0);
  dst[di + 3] = a + ((dst[di + 3] * ka / 255) | 0);
}

// y: upload rows in the image's own channel format (draw(2) loadimage).
// Rows span r's x-range at the image's depth, packed at absolute bit
// positions, leftmost pixel in the high bits — draw(6)'s layout.
export function loadRows(img, r, bytes) {
  const [x0, y0, x1, y1] = r;
  const parts = chanparts(img.chan);
  const depth = chandepth(img.chan);
  const bpl = Math.floor((x1 * depth + 7) / 8) - Math.floor((x0 * depth) / 8);
  const put = (x, y, rgba) => {
    const di = wpx(img, x, y);
    if (di >= 0) img.back.data.set(rgba, di);
  };
  for (let y = y0; y < y1; y++) {
    const row = bytes.subarray((y - y0) * bpl, (y - y0 + 1) * bpl);
    if (depth < 8) {
      const off0 = Math.floor((x0 * depth) / 8) * 8;   // bit position of row[0]
      for (let x = x0; x < x1; x++) {
        const bit = x * depth - off0;
        const byte = row[bit >> 3] ?? 0;
        const v = (byte >> (8 - depth - (bit & 7))) & ((1 << depth) - 1);
        const g = (v * 255 / ((1 << depth) - 1)) | 0;
        put(x, y, [g, g, g, g]);
      }
    } else {
      const nb = depth / 8;
      for (let x = x0; x < x1; x++) {
        const o = (x - x0) * nb;
        const rgba = [0, 0, 0, 255];
        let k = 0;
        // wire order: last channel in the descriptor is the low-order byte
        for (let ci = parts.length - 1; ci >= 0; ci--, k++) {
          const v = row[o + k] ?? 0;
          switch (parts[ci].type) {
          case 0: rgba[0] = v; break;                  // CRed
          case 1: rgba[1] = v; break;                  // CGreen
          case 2: rgba[2] = v; break;                  // CBlue
          case 3: rgba[0] = rgba[1] = rgba[2] = rgba[3] = v; break; // CGrey
          case 4: rgba[3] = v; break;                  // CAlpha
          }
        }
        put(x, y, rgba);
      }
    }
  }
  return (y1 - y0) * bpl;
}

export function copyRect(dst, dr, src, sp) {
  const [x0, y0, x1, y1] = dr;
  for (let y = y0; y < y1; y++)
    for (let x = x0; x < x1; x++) {
      const di = wpx(dst, x, y), si = px(src, sp[0] + (x - x0), sp[1] + (y - y0));
      if (di >= 0 && si >= 0) dst.back.data.set(src.back.data.subarray(si, si + 4), di);
    }
}
