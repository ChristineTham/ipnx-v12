// devwsys: '#w' — the window server's kernel half, rio's INTERFACE rather
// than rio (the plan's GUI decision): reading clone mints a window; a
// window is a directory of cons, mouse, wctl, winid, label — and draw/,
// an actual per-window /dev/draw (the thing plan9port had to give up).
// Bind '#w/<id>' over /dev in a namespace copy and that namespace IS the
// window. Window policy (placement, dragging) belongs to the host.
//
// v0 deviations: 'rgb' exposes the window's raw RGBA for headless pixel
// tests; cons text is host-rendered (no fonts in draw yet); dir reads and
// stats are absent.
import { sbytes, bstr, concat } from "./bytes.mjs";
import * as D from "./draw.mjs";

const derr = (m) => Object.assign(new Error(m), { guest: true });
const pad11 = (v) => String(v).padStart(11, " ") + " ";

export function makeWsys(hostRef) {
  const wins = new Map();
  let nextwid = 1;
  let qgen = 1;

  function newWindow() {
    const wid = nextwid++;
    const w = 400, h = 300;
    const win = {
      wid, x: 40 + (wid * 24) % 200, y: 40 + (wid * 24) % 140, w, h,
      label: `window ${wid}`, img: D.newImage(0, 0, w, h, 0, [255, 255, 255, 255]),
      conns: new Map(), nextconn: 1,
      consbuf: new Uint8Array(0), consparked: [],
      mousebuf: [], mouseparked: [],
      dead: false,
    };
    wins.set(wid, win);
    hostRef.host.winCreate?.(wid, win.x, win.y, w, h, win.label);
    return win;
  }
  const serveCons = (win) => {
    while (win.consparked.length && (win.consbuf.length || win.dead)) {
      const { n, ctx } = win.consparked.shift();
      const give = win.consbuf.subarray(0, Math.min(n, win.consbuf.length));
      win.consbuf = win.consbuf.subarray(give.length);
      ctx.done(give);
    }
  };
  const serveMouse = (win) => {
    while (win.mouseparked.length && (win.mousebuf.length || win.dead)) {
      const { ctx } = win.mouseparked.shift();
      ctx.done(win.dead ? new Uint8Array(0) : win.mousebuf.shift());
    }
  };

  const dev = {
    name: "wsys",
    attach: () => ({ kind: "root" }),
    walk: (n, name) => {
      if (n.kind === "root") {
        if (name === "clone") return { kind: "clone", win: null };
        const win = wins.get(Number(name));
        return win ? { kind: "windir", win } : null;
      }
      if (n.kind === "windir") {
        const { win } = n;
        if (["cons", "mouse", "wctl", "winid", "label", "rgb"].includes(name))
          return { kind: name, win };
        if (name === "draw") return { kind: "drawdir", win };
        return null;
      }
      if (n.kind === "drawdir") {
        if (name === "new") return { kind: "drawnew", win: n.win, conn: null };
        const conn = n.win.conns.get(Number(name));
        return conn ? { kind: "conndir", win: n.win, conn } : null;
      }
      if (n.kind === "conndir" && (name === "ctl" || name === "data"))
        return { kind: "draw" + name, win: n.win, conn: n.conn };
      return null;
    },
    open: (n) => {
      if (n.kind === "drawnew") {          // a fresh connection; image 0 is the window
        const { win } = n;
        n.conn = { id: win.nextconn++, images: new Map([[0, win.img]]) };
        win.conns.set(n.conn.id, n.conn);
      }
    },
    read: (n, count, off, ctx) => {
      const { win } = n;
      const one = (s) => (Number(off) === 0 ? sbytes(s) : new Uint8Array(0));
      switch (n.kind) {
      case "clone":
        if (!n.win) n.win = newWindow();
        return one(`${n.win.wid}`);
      case "winid": return one(`${win.wid}`);
      case "label": return one(win.label);
      case "wctl": return one(`${win.x} ${win.y} ${win.w} ${win.h} current visible\n`);
      case "drawnew": case "drawctl": {
        const c = n.conn ?? { id: 0 };
        return one(pad11(c.id) + pad11(0) + "r8g8b8a8   " + " " + pad11(0) +
          pad11(0) + pad11(0) + pad11(win.w) + pad11(win.h) +
          pad11(0) + pad11(0) + pad11(win.w) + pad11(win.h) + "\n");
      }
      case "rgb": {
        const d = win.img.data;
        const end = Math.min(Number(off) + count, d.length);
        return d.subarray(Math.min(Number(off), d.length), end);
      }
      case "cons":
        if (win.consbuf.length || win.dead) {
          const give = win.consbuf.subarray(0, Math.min(count, win.consbuf.length));
          win.consbuf = win.consbuf.subarray(give.length);
          return give;
        }
        win.consparked.push({ n: count, ctx });
        return undefined;
      case "mouse":
        if (win.mousebuf.length) return win.mousebuf.shift();
        if (win.dead) return new Uint8Array(0);
        win.mouseparked.push({ ctx });
        return undefined;
      default: throw derr(`no read on ${n.kind}`);
      }
    },
    write: (n, data, off) => {
      const { win } = n;
      switch (n.kind) {
      case "cons":
        hostRef.host.winText?.(win.wid, new Uint8Array(data));
        return data.length;
      case "label":
        win.label = bstr(data).trim();
        hostRef.host.winLabel?.(win.wid, win.label);
        return data.length;
      case "wctl": {
        const t = bstr(data).trim().split(/\s+/);
        if (t[0] === "move" && t.length === 3) { win.x = +t[1]; win.y = +t[2]; }
        else if (t[0] === "resize" && t.length === 3) resize(win, +t[1], +t[2]);
        else if (t[0] === "delete") del(win);
        else throw derr(`wctl: bad message '${bstr(data).trim()}'`);
        hostRef.host.winGeom?.(win.wid, win.x, win.y, win.w, win.h);
        return data.length;
      }
      case "drawdata": return drawmsgs(win, n.conn, new Uint8Array(data));
      default: throw derr(`no write on ${n.kind}`);
      }
    },
    clunk: () => {},
    stat: () => { throw derr("no stat on wsys files (v0)"); },
    len: (n) => (n.kind === "rgb" ? n.win.img.data.length : 0),
    // the host half calls in through these:
    mouse: (wid, x, y, buttons) => {
      const win = wins.get(wid);
      if (!win) return;
      win.mousebuf.push(sbytes("m" + pad11(x) + pad11(y) + pad11(buttons) + pad11(0)));
      if (win.mousebuf.length > 256) win.mousebuf.shift();
      serveMouse(win);
    },
    key: (wid, byte) => {
      const win = wins.get(wid);
      if (!win) return;
      win.consbuf = concat([win.consbuf, Uint8Array.of(byte)]);
      serveCons(win);
    },
  };

  function resize(win, w, h) {
    win.w = w; win.h = h;
    win.img = D.newImage(0, 0, w, h, 0, [255, 255, 255, 255]);
    for (const c of win.conns.values()) c.images.set(0, win.img);
  }
  function del(win) {
    win.dead = true;
    serveCons(win); serveMouse(win);
    hostRef.host.winClose?.(win.wid);
    wins.delete(win.wid);
  }

  // the draw(3) message subset; values low-order byte first
  function drawmsgs(win, conn, b) {
    const v = new DataView(b.buffer, b.byteOffset, b.byteLength);
    const u32 = (o) => v.getUint32(o, true);
    const s32 = (o) => v.getInt32(o, true);
    const img = (id) => {
      const i = conn.images.get(id);
      if (!i) throw derr(`draw: no image ${id}`);
      return i;
    };
    let o = 0;
    while (o < b.length) {
      const op = String.fromCharCode(b[o]);
      switch (op) {
      case "b": {                          // id[4] screen[4] refresh[1] chan[4] repl[1] r[16] clipr[16] color[4]
        const id = u32(o + 1), repl = b[o + 14];
        const r = [s32(o + 15), s32(o + 19), s32(o + 23), s32(o + 27)];
        const color = [b[o + 47], b[o + 48], b[o + 49], b[o + 50]];
        conn.images.set(id, D.newImage(r[0], r[1], r[2], r[3], repl, color));
        o += 51;
        break;
      }
      case "d": {                          // dst[4] src[4] mask[4] dstr[16] srcpt[8] maskpt[8]
        const dst = img(u32(o + 1)), src = img(u32(o + 5));
        const dstr = [s32(o + 13), s32(o + 17), s32(o + 21), s32(o + 25)];
        D.drawOp(dst, dstr, src, [s32(o + 29), s32(o + 33)]);
        o += 45;
        break;
      }
      case "f": conn.images.delete(u32(o + 1)); o += 5; break;
      case "L": {                          // dst[4] p0[8] p1[8] end0[4] end1[4] thick[4] src[4] sp[8]
        const dst = img(u32(o + 1)), src = img(u32(o + 33));
        D.line(dst, [s32(o + 5), s32(o + 9)], [s32(o + 13), s32(o + 17)],
          src, [s32(o + 37), s32(o + 41)]);
        o += 45;
        break;
      }
      case "e": case "E": {                // dst[4] c[8] a[4] b[4] thick[4] src[4] sp[8]
        const dst = img(u32(o + 1)), src = img(u32(o + 25));
        D.ellipse(dst, [s32(o + 5), s32(o + 9)], s32(o + 13), s32(o + 17),
          src, [s32(o + 29), s32(o + 33)], op === "E");
        o += 37;
        break;
      }
      case "v":
        hostRef.host.winPresent?.(win.wid, win.w, win.h, win.img.data);
        o += 1;
        break;
      default: throw derr(`draw: message '${op}' not in the v0 subset (b d f L e E v)`);
      }
    }
    return b.length;
  }

  return dev;
}
