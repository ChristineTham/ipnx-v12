// The draw engine behind #w's per-window /dev/draw: RGBA images and the
// draw(3) binary message subset — b (allocate), d (draw), f (free),
// L (line), e/E (ellipse), v (flush). Values are "low-order byte first",
// per draw(3); colours arrive RGBA and are treated as premultiplied for
// S-over-D. Text, arcs and compressed images are later work.
export function newImage(x0, y0, x1, y1, repl = 0, color = null) {
  const w = x1 - x0, h = y1 - y0;
  const img = { r: [x0, y0, x1, y1], w, h, repl, data: new Uint8Array(w * h * 4) };
  if (color !== null) fill(img, color);
  return img;
}

export function fill(img, rgba) {
  const [r, g, b, a] = rgba;
  for (let i = 0; i < img.data.length; i += 4) {
    img.data[i] = r; img.data[i + 1] = g; img.data[i + 2] = b; img.data[i + 3] = a;
  }
}

const px = (img, x, y) => {
  // repl: tile the source over all space, per draw(2)
  if (img.repl) {
    x = ((x - img.r[0]) % img.w + img.w) % img.w;
    y = ((y - img.r[1]) % img.h + img.h) % img.h;
    return ((y * img.w) + x) * 4;
  }
  if (x < img.r[0] || x >= img.r[2] || y < img.r[1] || y >= img.r[3]) return -1;
  return (((y - img.r[1]) * img.w) + (x - img.r[0])) * 4;
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

// d: combine src (aligned at sp for dstr.min) into dst over rectangle dstr
export function drawOp(dst, dstr, src, sp) {
  const [dx0, dy0, dx1, dy1] = dstr;
  for (let y = Math.max(dy0, dst.r[1]); y < Math.min(dy1, dst.r[3]); y++)
    for (let x = Math.max(dx0, dst.r[0]); x < Math.min(dx1, dst.r[2]); x++) {
      const si = px(src, sp[0] + (x - dx0), sp[1] + (y - dy0));
      if (si < 0) continue;
      blend(dst.data, px(dst, x, y), src.data, si);
    }
}

export function line(dst, p0, p1, src, sp) {
  let [x0, y0] = p0;
  const [x1, y1] = p1;
  const dx = Math.abs(x1 - x0), dy = -Math.abs(y1 - y0);
  const sx = x0 < x1 ? 1 : -1, sy = y0 < y1 ? 1 : -1;
  let e = dx + dy;
  for (;;) {
    const di = px(dst, x0, y0);
    if (di >= 0) {
      const si = px(src, sp[0], sp[1]);
      if (si >= 0) blend(dst.data, di, src.data, si);
    }
    if (x0 === x1 && y0 === y1) break;
    const e2 = 2 * e;
    if (e2 >= dy) { e += dy; x0 += sx; }
    if (e2 <= dx) { e += dx; y0 += sy; }
  }
}

export function ellipse(dst, c, a, b, src, sp, filled) {
  const [cx, cy] = c;
  for (let y = -b; y <= b; y++)
    for (let x = -a; x <= a; x++) {
      const v = (x * x) / (a * a || 1) + (y * y) / (b * b || 1);
      const inside = v <= 1;
      const edge = !filled && inside && ((x + 1) * (x + 1)) / (a * a || 1) + (y * y) / (b * b || 1) > 1
        || !filled && inside && (x * x) / (a * a || 1) + ((y + 1) * (y + 1)) / (b * b || 1) > 1
        || !filled && inside && ((x - 1) * (x - 1)) / (a * a || 1) + (y * y) / (b * b || 1) > 1
        || !filled && inside && (x * x) / (a * a || 1) + ((y - 1) * (y - 1)) / (b * b || 1) > 1;
      if (filled ? inside : edge) {
        const di = px(dst, cx + x, cy + y);
        if (di < 0) continue;
        const si = px(src, sp[0], sp[1]);
        if (si >= 0) blend(dst.data, di, src.data, si);
      }
    }
}

// s: blit rectangle maskr of mask into dst at p, colouring by src — the
// per-glyph half of the string message; mask alpha scales src alpha.
export function glyph(dst, p, mask, maskr, src, sp) {
  const [mx0, my0, mx1, my1] = maskr;
  for (let y = my0; y < my1; y++)
    for (let x = mx0; x < mx1; x++) {
      const mi = pxOf(mask, x, y);
      if (mi < 0) continue;
      const ma = mask.data[mi + 3];
      if (ma === 0) continue;
      const di = pxOf(dst, p[0] + (x - mx0), p[1] + (y - my0));
      if (di < 0) continue;
      const si = pxOf(src, sp[0], sp[1]);
      if (si < 0) continue;
      blend2(dst.data, di,
        (src.data[si] * ma / 255) | 0, (src.data[si + 1] * ma / 255) | 0,
        (src.data[si + 2] * ma / 255) | 0, (src.data[si + 3] * ma / 255) | 0);
    }
}
function blend2(dst, di, r, g, b, a) {
  const ka = 255 - a;
  dst[di] = r + ((dst[di] * ka / 255) | 0);
  dst[di + 1] = g + ((dst[di + 1] * ka / 255) | 0);
  dst[di + 2] = b + ((dst[di + 2] * ka / 255) | 0);
  dst[di + 3] = a + ((dst[di + 3] * ka / 255) | 0);
}
// copy: plain rectangle copy (no blend) — 'y' uploads and 'l' loads use it
export function loadPixels(img, r, bytes) {
  const [x0, y0, x1, y1] = r;
  let o = 0;
  for (let y = y0; y < y1; y++)
    for (let x = x0; x < x1; x++) {
      const di = pxOf(img, x, y);
      if (di >= 0) img.data.set(bytes.subarray(o, o + 4), di);
      o += 4;
    }
}
export function copyRect(dst, dr, src, sp) {
  const [x0, y0, x1, y1] = dr;
  for (let y = y0; y < y1; y++)
    for (let x = x0; x < x1; x++) {
      const di = pxOf(dst, x, y), si = pxOf(src, sp[0] + (x - x0), sp[1] + (y - y0));
      if (di >= 0 && si >= 0) dst.data.set(src.data.subarray(si, si + 4), di);
    }
}
export const pxOf = (img, x, y) => {
  if (img.repl) {
    x = ((x - img.r[0]) % img.w + img.w) % img.w;
    y = ((y - img.r[1]) % img.h + img.h) % img.h;
    return ((y * img.w) + x) * 4;
  }
  if (x < img.r[0] || x >= img.r[2] || y < img.r[1] || y >= img.r[3]) return -1;
  return (((y - img.r[1]) * img.w) + (x - img.r[0])) * 4;
};
