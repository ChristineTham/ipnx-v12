// The draw engine behind #w's per-window /dev/draw — the Rust port of
// supervisor/draw.mjs: RGBA rasters and the draw(3) operations. Values are
// "low-order byte first" per draw(3); colours are RGBA32. An image may be a
// VIEW onto another image's backing store — that is what a window on a
// screen is — and every write clips against r ∩ clipr, per draw(2).
//
// One Rust-side deviation from the JS: every operation snapshots the pixels
// it reads from its source image before writing the destination, because
// source and destination may share a backing (a screen and its windows) and
// RefCell forbids the aliased borrow the JS version never noticed it took.
// Sources here are small — 1×1 replicated colours, font strips — so the
// copy is noise.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct Backing {
    pub data: Vec<u8>,
    pub w: i32,
    pub h: i32,
    pub x0: i32,
    pub y0: i32,
}
pub type BackR = Rc<RefCell<Backing>>;

pub struct FontSlot {
    pub r: [i32; 4],
    pub left: i8,
    pub width: u8,
}

pub struct FontInfo {
    pub nchars: u32,
    pub ascent: u8,
    pub slots: HashMap<u16, FontSlot>,
}

pub struct Image {
    pub r: [i32; 4],
    pub clipr: [i32; 4],
    pub w: i32,
    pub h: i32,
    pub repl: u8,
    pub chan: u32,
    pub back: BackR,
    pub font: Option<FontInfo>,
}
pub type ImageR = Rc<RefCell<Image>>;

pub fn new_image(r: [i32; 4], repl: u8, chan: u32, clipr: Option<[i32; 4]>,
                 color: Option<[u8; 4]>, back: Option<BackR>) -> ImageR {
    let (x0, y0, x1, y1) = (r[0], r[1], r[2], r[3]);
    let w = x1 - x0;
    let h = y1 - y0;
    let back = back.unwrap_or_else(|| {
        Rc::new(RefCell::new(Backing {
            data: vec![0; (w.max(0) * h.max(0) * 4) as usize],
            w, h, x0, y0,
        }))
    });
    let img = Rc::new(RefCell::new(Image {
        r,
        clipr: clipr.unwrap_or(r),
        w, h, repl, chan, back,
        font: None,
    }));
    if let Some(c) = color {
        fill_rect(&img, r, c);
    }
    img
}

pub fn rgba32(v: u32) -> [u8; 4] {
    [(v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8]
}

// channel descriptor: 4 packed bytes, high first, each (type<<4)|nbits
fn chanparts(chan: u32) -> Vec<(u8, u8)> {
    let mut out = Vec::new();
    for s in [24, 16, 8, 0] {
        let b = ((chan >> s) & 255) as u8;
        if b != 0 {
            out.push((b >> 4, b & 15));
        }
    }
    out
}
fn chandepth(chan: u32) -> u32 {
    chanparts(chan).iter().map(|&(_, n)| n as u32).sum()
}

// backing index of (x,y) for reads: repl tiles r over all space
fn px(img: &Image, back: &Backing, mut x: i32, mut y: i32) -> Option<usize> {
    if img.repl != 0 {
        x = img.r[0] + (((x - img.r[0]) % img.w) + img.w) % img.w;
        y = img.r[1] + (((y - img.r[1]) % img.h) + img.h) % img.h;
    } else if x < img.r[0] || x >= img.r[2] || y < img.r[1] || y >= img.r[3] {
        return None;
    }
    let bx = x - back.x0;
    let by = y - back.y0;
    if bx < 0 || bx >= back.w || by < 0 || by >= back.h {
        return None;
    }
    Some(((by * back.w + bx) * 4) as usize)
}

// writable index: inside r ∩ clipr and the backing
fn wpx(img: &Image, back: &Backing, x: i32, y: i32) -> Option<usize> {
    if x < img.r[0] || x >= img.r[2] || y < img.r[1] || y >= img.r[3] {
        return None;
    }
    let c = img.clipr;
    if x < c[0] || x >= c[2] || y < c[1] || y >= c[3] {
        return None;
    }
    let bx = x - back.x0;
    let by = y - back.y0;
    if bx < 0 || bx >= back.w || by < 0 || by >= back.h {
        return None;
    }
    Some(((by * back.w + bx) * 4) as usize)
}

fn blend(dst: &mut [u8], di: usize, src: [u8; 4]) {
    let sa = src[3] as u32;
    if sa == 255 {
        dst[di..di + 4].copy_from_slice(&src);
        return;
    }
    let ka = 255 - sa;
    for i in 0..4 {
        dst[di + i] = (src[i] as u32 + dst[di + i] as u32 * ka / 255) as u8;
    }
}

// a source snapshot: image geometry + copied pixels, aliasing-proof
struct Src {
    r: [i32; 4],
    w: i32,
    h: i32,
    repl: u8,
    x0: i32,
    y0: i32,
    bw: i32,
    bh: i32,
    data: Vec<u8>,
}
fn snap(img: &ImageR) -> Src {
    let i = img.borrow();
    let b = i.back.borrow();
    Src {
        r: i.r, w: i.w, h: i.h, repl: i.repl,
        x0: b.x0, y0: b.y0, bw: b.w, bh: b.h,
        data: b.data.clone(),
    }
}
fn spx(s: &Src, mut x: i32, mut y: i32) -> Option<usize> {
    if s.repl != 0 {
        x = s.r[0] + (((x - s.r[0]) % s.w) + s.w) % s.w;
        y = s.r[1] + (((y - s.r[1]) % s.h) + s.h) % s.h;
    } else if x < s.r[0] || x >= s.r[2] || y < s.r[1] || y >= s.r[3] {
        return None;
    }
    let bx = x - s.x0;
    let by = y - s.y0;
    if bx < 0 || bx >= s.bw || by < 0 || by >= s.bh {
        return None;
    }
    Some(((by * s.bw + bx) * 4) as usize)
}
fn sget(s: &Src, x: i32, y: i32) -> Option<[u8; 4]> {
    spx(s, x, y).map(|i| [s.data[i], s.data[i + 1], s.data[i + 2], s.data[i + 3]])
}

pub fn fill_rect(img: &ImageR, r: [i32; 4], rgba: [u8; 4]) {
    let i = img.borrow();
    let back = i.back.clone();
    let mut b = back.borrow_mut();
    for y in r[1]..r[3] {
        for x in r[0]..r[2] {
            if let Some(di) = wpx(&i, &b, x, y) {
                b.data[di..di + 4].copy_from_slice(&rgba);
            }
        }
    }
}

// d: combine src (aligned at sp with dstr.min) into dst over dstr
pub fn draw_op(dst: &ImageR, dstr: [i32; 4], src: &ImageR, sp: [i32; 2]) {
    let s = snap(src);
    let d = dst.borrow();
    let back = d.back.clone();
    let mut b = back.borrow_mut();
    for y in dstr[1]..dstr[3] {
        for x in dstr[0]..dstr[2] {
            let Some(di) = wpx(&d, &b, x, y) else { continue };
            let Some(pxl) = sget(&s, sp[0] + (x - dstr[0]), sp[1] + (y - dstr[1])) else { continue };
            blend(&mut b.data, di, pxl);
        }
    }
}

pub fn line(dst: &ImageR, p0: [i32; 2], p1: [i32; 2], src: &ImageR, sp: [i32; 2]) {
    let s = snap(src);
    let d = dst.borrow();
    let back = d.back.clone();
    let mut b = back.borrow_mut();
    let (mut x0, mut y0) = (p0[0], p0[1]);
    let (x1, y1) = (p1[0], p1[1]);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut e = dx + dy;
    loop {
        if let Some(di) = wpx(&d, &b, x0, y0) {
            if let Some(pxl) = sget(&s, sp[0], sp[1]) {
                blend(&mut b.data, di, pxl);
            }
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * e;
        if e2 >= dy {
            e += dy;
            x0 += sx;
        }
        if e2 <= dx {
            e += dx;
            y0 += sy;
        }
    }
}

pub fn ellipse(dst: &ImageR, c: [i32; 2], a: i32, bb_: i32, src: &ImageR, sp: [i32; 2], filled: bool) {
    let s = snap(src);
    let d = dst.borrow();
    let back = d.back.clone();
    let mut b = back.borrow_mut();
    let aa = (a * a).max(1) as f64;
    let bb2 = (bb_ * bb_).max(1) as f64;
    let inside = |x: i32, y: i32| (x * x) as f64 / aa + (y * y) as f64 / bb2 <= 1.0;
    for y in -bb_..=bb_ {
        for x in -a..=a {
            let is_in = inside(x, y);
            let edge = !filled && is_in
                && (!inside(x + 1, y) || !inside(x - 1, y) || !inside(x, y + 1) || !inside(x, y - 1));
            if if filled { is_in } else { edge } {
                if let Some(di) = wpx(&d, &b, c[0] + x, c[1] + y) {
                    if let Some(pxl) = sget(&s, sp[0], sp[1]) {
                        blend(&mut b.data, di, pxl);
                    }
                }
            }
        }
    }
}

// s: blit rectangle maskr of mask into dst at p, colouring by src — the
// per-glyph half of the string message; mask alpha scales src alpha
pub fn glyph(dst: &ImageR, p: [i32; 2], mask: &ImageR, maskr: [i32; 4], src: &ImageR, sp: [i32; 2]) {
    let ms = snap(mask);
    let ss = snap(src);
    let d = dst.borrow();
    let back = d.back.clone();
    let mut b = back.borrow_mut();
    for y in maskr[1]..maskr[3] {
        for x in maskr[0]..maskr[2] {
            let Some(mp) = sget(&ms, x, y) else { continue };
            let ma = mp[3] as u32;
            if ma == 0 {
                continue;
            }
            let Some(di) = wpx(&d, &b, p[0] + (x - maskr[0]), p[1] + (y - maskr[1])) else { continue };
            let Some(sc) = sget(&ss, sp[0], sp[1]) else { continue };
            let scaled = [
                (sc[0] as u32 * ma / 255) as u8,
                (sc[1] as u32 * ma / 255) as u8,
                (sc[2] as u32 * ma / 255) as u8,
                (sc[3] as u32 * ma / 255) as u8,
            ];
            let ka = 255 - scaled[3] as u32;
            for i in 0..4 {
                b.data[di + i] = (scaled[i] as u32 + b.data[di + i] as u32 * ka / 255) as u8;
            }
        }
    }
}

// y: upload rows in the image's own channel format (draw(2) loadimage).
// Rows span r's x-range at the image's depth, packed at absolute bit
// positions, leftmost pixel in the high bits — draw(6)'s layout.
pub fn load_rows(img: &ImageR, r: [i32; 4], bytes: &[u8]) -> usize {
    let (x0, y0, x1, y1) = (r[0], r[1], r[2], r[3]);
    let (chan, parts, depth) = {
        let i = img.borrow();
        (i.chan, chanparts(i.chan), chandepth(i.chan))
    };
    let _ = chan;
    let bpl = ((x1 as i64 * depth as i64 + 7) / 8 - (x0 as i64 * depth as i64) / 8) as usize;
    let i = img.borrow();
    let back = i.back.clone();
    let mut b = back.borrow_mut();
    for y in y0..y1 {
        let row_start = ((y - y0) as usize) * bpl;
        let row = &bytes[row_start.min(bytes.len())..((y - y0 + 1) as usize * bpl).min(bytes.len())];
        if depth < 8 {
            let off0 = ((x0 as i64 * depth as i64) / 8 * 8) as i64;
            for x in x0..x1 {
                let bit = (x as i64 * depth as i64 - off0) as usize;
                let byte = *row.get(bit >> 3).unwrap_or(&0) as u32;
                let v = (byte >> (8 - depth as usize - (bit & 7))) & ((1 << depth) - 1);
                let g = (v * 255 / ((1 << depth) - 1)) as u8;
                if let Some(di) = wpx(&i, &b, x, y) {
                    b.data[di..di + 4].copy_from_slice(&[g, g, g, g]);
                }
            }
        } else {
            let nb = (depth / 8) as usize;
            for x in x0..x1 {
                let o = ((x - x0) as usize) * nb;
                let mut rgba = [0u8, 0, 0, 255];
                // wire order: last channel in the descriptor is the low byte
                for (k, ci) in (0..parts.len()).rev().enumerate() {
                    let v = *row.get(o + k).unwrap_or(&0);
                    match parts[ci].0 {
                        0 => rgba[0] = v,
                        1 => rgba[1] = v,
                        2 => rgba[2] = v,
                        3 => {
                            rgba = [v, v, v, v];
                        }
                        4 => rgba[3] = v,
                        _ => {}
                    }
                }
                if let Some(di) = wpx(&i, &b, x, y) {
                    b.data[di..di + 4].copy_from_slice(&rgba);
                }
            }
        }
    }
    (y1 - y0).max(0) as usize * bpl
}

pub fn copy_rect(dst: &ImageR, dr: [i32; 4], src: &ImageR, sp: [i32; 2]) {
    let s = snap(src);
    let d = dst.borrow();
    let back = d.back.clone();
    let mut b = back.borrow_mut();
    for y in dr[1]..dr[3] {
        for x in dr[0]..dr[2] {
            if let Some(di) = wpx(&d, &b, x, y) {
                if let Some(pxl) = sget(&s, sp[0] + (x - dr[0]), sp[1] + (y - dr[1])) {
                    b.data[di..di + 4].copy_from_slice(&pxl);
                }
            }
        }
    }
}
