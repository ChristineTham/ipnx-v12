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
import { marshalStat } from "./stat9.mjs";
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
      label: `window ${wid}`, img: D.newImage([0, 0, w, h], { color: [255, 255, 255, 255] }),
      conns: new Map(), nextconn: 1,
      consbuf: new Uint8Array(0), consparked: [],
      mousebuf: [], mouseparked: [],
      cv: null,                              // lazy: the canvas tree (docs/canvas.md)
      dead: false,
    };
    wins.set(wid, win);
    hostRef.host.winCreate?.(wid, win.x, win.y, w, h, win.label);
    return win;
  }
  // ---- /dev/canvas (docs/canvas.md): a flat tree of semantic nodes.
  // v0 kinds: stack, text, edit, path. addr/data is acme's buffer
  // interface simplified (q0,q1 and $); events are %-quoted lines.
  function mkcv(win) {
    if (!win.cv) win.cv = {
      nodes: new Map([[0, { kind: "stack", attrs: new Map([["dir", "col"]]), data: new Uint8Array(0), addr: [0, 0] }]]),
      order: 1, events: [], evparked: [],
    };
    return win.cv;
  }
  const serveCv = (win) => {
    const cv = win.cv;
    if (!cv) return;
    while (cv.evparked.length && (cv.events.length || win.dead)) {
      const { ctx } = cv.evparked.shift();
      ctx.done(sbytes(cv.events.length ? cv.events.shift() : ""));
    }
  };
  const cvunq = (t) => t.replace(/%0[Aa]/g, "\n").replace(/%20/g, " ").replace(/%25/g, "%");
  // a user edit event is a mutation that HAPPENED: the tree is the truth,
  // so the device applies insert/delete to the node before notifying the
  // app — presenter echo and node data stay one thing (acme's discipline)
  const cvapply = (win, line) => {
    const f = line.trim().split(/\s+/);
    const nd = win.cv?.nodes.get(+f[1]);
    if (!nd) return;
    if (f[0] === "insert" && f.length >= 4) {
      const ins = sbytes(cvunq(f[3]));
      const q0 = Math.min(+f[2], nd.data.length);
      const grown = new Uint8Array(nd.data.length + ins.length);
      grown.set(nd.data.subarray(0, q0), 0);
      grown.set(ins, q0);
      grown.set(nd.data.subarray(q0), q0 + ins.length);
      nd.data = grown;
    } else if (f[0] === "delete" && f.length >= 4) {
      const q0 = Math.min(+f[2], nd.data.length);
      const q1 = Math.min(+f[3], nd.data.length);
      if (q1 > q0) {
        const cut = new Uint8Array(nd.data.length - (q1 - q0));
        cut.set(nd.data.subarray(0, q0), 0);
        cut.set(nd.data.subarray(q1), q0);
        nd.data = cut;
      }
    }
  };
  const cvpush = (win, line) => {
    if (!win.cv) return;
    cvapply(win, line);
    win.cv.events.push(line + "\n");
    if (win.cv.events.length > 512) win.cv.events.shift();
    serveCv(win);
  };
  const qt = (t) => t.replace(/%/g, "%25").replace(/ /g, "%20").replace(/\n/g, "%0A");
  function cvsnap(win) {
    const out = [];
    for (const [id, nd] of win.cv.nodes)
      out.push({ id, kind: nd.kind, attrs: Object.fromEntries(nd.attrs), data: bstr(nd.data) });
    return out;
  }
  function cvparseaddr(nd, raw) {
    const t = raw.trim();
    const end = nd.data.length;
    if (t === "$") { nd.addr = [end, end]; return; }
    const m = t.match(/^(\d+)(?:,(\d+|\$))?$/);
    if (!m) throw derr(`bad addr '${t}'`);
    const q0 = Math.min(+m[1], end);
    const q1 = m[2] === undefined ? q0 : m[2] === "$" ? end : Math.min(+m[2], end);
    if (q1 < q0) throw derr("addr reversed");
    nd.addr = [q0, q1];
  }
  function cvctl(win, raw) {
    const cv = mkcv(win);
    for (const line of raw.split("\n")) {
      const t = line.trim();
      if (t === "") continue;
      const f = t.split(/\s+/);
      if (f[0] === "new") {
        const id = +f[1];
        if (!(id > 0) || cv.nodes.has(id)) throw derr(`new: bad or taken id '${f[1]}'`);
        if (!["stack", "text", "edit", "path"].includes(f[2])) throw derr(`new: unknown kind '${f[2]}'`);
        cv.nodes.set(id, { kind: f[2], data: new Uint8Array(0), addr: [0, 0],
          attrs: new Map([["parent", "0"], ["order", String(cv.order++)]]) });
      } else if (f[0] === "del") {
        const id = +f[1];
        if (id === 0 || !cv.nodes.has(id)) throw derr(`del: no node '${f[1]}'`);
        const doomed = [id];                 // the subtree goes with it
        for (let i = 0; i < doomed.length; i++)
          for (const [k, nd] of cv.nodes)
            if (+(nd.attrs.get("parent") ?? -1) === doomed[i]) doomed.push(k);
        for (const k of doomed) cv.nodes.delete(k);
      } else if (f[0] === "sync") {
        hostRef.host.winCanvas?.(win.wid, cvsnap(win));
      } else if (f[0] === "event") {
        cvpush(win, t.slice(6));             // the virtual-surface door (wctl 'type' precedent)
      } else
        throw derr(`canvas ctl: unknown verb '${f[0]}'`);
    }
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
        if (name === "canvas") { mkcv(n.win); return { kind: "cvdir", win: n.win }; }
        if (["cons", "mouse", "wctl", "winid", "label", "rgb", "consctl", "cursor"].includes(name))
          return { kind: name, win };
        if (name === "draw") return { kind: "drawdir", win };
        return null;
      }
      if (n.kind === "cvdir") {
        if (name === "ctl") return { kind: "cvctl", win: n.win };
        if (name === "events") return { kind: "cvevents", win: n.win };
        if (name === "caps") return { kind: "cvcaps", win: n.win };
        if (/^\d+$/.test(name) && n.win.cv.nodes.has(+name))
          return { kind: "cvnode", win: n.win, id: +name };
        return null;
      }
      if (n.kind === "cvnode") {
        if (["kind", "attrs", "addr", "data"].includes(name))
          return { kind: "cv" + name, win: n.win, id: n.id };
        return null;
      }
      if (n.kind === "drawdir") {
        if (name === "new") return { kind: "drawnew", win: n.win, conn: null };
        const conn = n.win.conns.get(Number(name));
        return conn ? { kind: "conndir", win: n.win, conn } : null;
      }
      if (n.kind === "conndir" && (name === "ctl" || name === "data" || name === "refresh"))
        return { kind: "draw" + name, win: n.win, conn: n.conn };
      return null;
    },
    open: (n) => {
      if (n.kind === "drawnew") {          // a fresh connection; image 0 is the window
        const { win } = n;
        n.conn = { id: win.nextconn++, images: new Map([[0, win.img]]), screens: new Map() };
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
      case "cvcaps": return one((hostRef.host.canvasCaps?.() ?? "virtual") + "\n");
      case "cvkind": return one(win.cv.nodes.get(n.id).kind + "\n");
      case "cvaddr": {
        const nd = win.cv.nodes.get(n.id);
        return one(`${nd.addr[0]},${nd.addr[1]}\n`);
      }
      case "cvattrs": {
        const nd = win.cv.nodes.get(n.id);
        let t = "";
        for (const [k, v] of nd.attrs) t += `${k}=${v}\n`;
        return one(t);
      }
      case "cvdata": {
        const nd = win.cv.nodes.get(n.id);
        const [q0, q1] = nd.addr;
        const seg = nd.data.subarray(q0, q1);
        const o = Math.min(Number(off), seg.length);
        return seg.subarray(o, Math.min(o + count, seg.length));
      }
      case "cvevents": {
        const cv = mkcv(win);
        if (cv.events.length || win.dead)
          return sbytes(cv.events.length ? cv.events.shift() : "");
        cv.evparked.push({ ctx });
        return undefined;                      // park: completes via ctx.done
      }
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
        const d = win.img.back.data;
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
      case "drawrefresh":                 // refresh events: none in v0; readers wait
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
        const raw = bstr(data);
        if (raw.startsWith("type ")) {               // keyboard injection: tests drive sam
          win.consbuf = concat([win.consbuf, new Uint8Array(data.slice(5))]);
          if (typeof process !== "undefined" && process.env.KDRAW)
            console.error(`[type fed ${data.length - 5} bytes, parked readers: ${win.consparked.length}]`);
          serveCons(win);
          return data.length;
        }
        const t = raw.trim().split(/\s+/);
        if (t[0] === "move" && t.length === 3) { win.x = +t[1]; win.y = +t[2]; }
        else if (t[0] === "resize" && t.length === 3) resize(win, +t[1], +t[2]);
        else if (t[0] === "mouse" && t.length === 4) { injectMouse(win, +t[1], +t[2], +t[3]); return data.length; }
        else if (t[0] === "delete") del(win);
        else throw derr(`wctl: bad message '${raw.trim()}'`);
        hostRef.host.winGeom?.(win.wid, win.x, win.y, win.w, win.h);
        return data.length;
      }
      case "cvctl": cvctl(win, bstr(data)); return data.length;
      case "cvaddr": cvparseaddr(win.cv.nodes.get(n.id), bstr(data)); return data.length;
      case "cvdata": {
        const nd = win.cv.nodes.get(n.id);
        const [q0, q1] = nd.addr;
        const ins = new Uint8Array(data);
        const grown = new Uint8Array(nd.data.length - (q1 - q0) + ins.length);
        grown.set(nd.data.subarray(0, q0), 0);
        grown.set(ins, q0);
        grown.set(nd.data.subarray(q1), q0 + ins.length);
        nd.data = grown;
        nd.addr = [q0 + ins.length, q0 + ins.length];
        return data.length;
      }
      case "cvattrs": {
        const nd = win.cv.nodes.get(n.id);
        for (const line of bstr(data).split("\n")) {
          const eq = line.indexOf("=");
          if (eq > 0) nd.attrs.set(line.slice(0, eq).trim(), line.slice(eq + 1).trim());
        }
        return data.length;
      }
      case "drawdata": return drawmsgs(win, n.conn, new Uint8Array(data));
      case "consctl": return data.length;             // rawon/rawoff: raw is the default
      case "cursor": return data.length;              // cursor shapes: host policy, v0 ignores
      default: throw derr(`no write on ${n.kind}`);
      }
    },
    clunk: () => {},
    stat: (n) => {
      const dir = ["root", "windir", "drawdir", "conndir", "cvdir", "cvnode"].includes(n.kind);
      const names = { root: "wsys", windir: String(n.win?.wid ?? 0), drawdir: "draw",
        conndir: String(n.conn?.id ?? 0), drawnew: "new", drawctl: "ctl",
        drawdata: "data", drawrefresh: "refresh" };
      return marshalStat({
        name: names[n.kind] ?? n.kind,
        qtype: dir ? 0x80 : 0,
        qpath: (n.win?.wid ?? 0) * 1024 + (n.conn?.id ?? 0) * 16 + (n.kind.length),
        mode: ((dir ? 0x80000000 | 0o555 : 0o666) >>> 0),
        length: n.kind === "rgb" ? n.win.img.back.data.length : 0,
      });
    },
    len: (n) => (n.kind === "rgb" ? n.win.img.back.data.length : 0),
    // the host half calls in through these:
    mouse: (wid, x, y, buttons) => {
      const win = wins.get(wid);
      if (!win) return;
      injectMouse(win, x, y, buttons);
    },
    key: (wid, byte) => {
      const win = wins.get(wid);
      if (!win) return;
      win.consbuf = concat([win.consbuf, Uint8Array.of(byte)]);
      serveCons(win);
    },
    canvasEvent: (wid, line) => {            // the interactive surface speaks
      const win = wins.get(wid);
      if (!win) return;
      cvpush(win, line);
    },
    quote: qt,
  };

  // one injector for both halves (host events and wctl 'mouse x y b'):
  // msec really advances, because double-click detection is msec arithmetic
  function injectMouse(win, x, y, buttons) {
    win.mousebuf.push(sbytes("m" + pad11(x) + pad11(y) + pad11(buttons) + pad11(Date.now() & 0x3fffffff)));
    if (win.mousebuf.length > 256) win.mousebuf.shift();
    serveMouse(win);
  }

  function resize(win, w, h) {
    win.w = w; win.h = h;
    cvpush(win, `resize 0 ${w} ${h}`);
    win.img = D.newImage([0, 0, w, h], { color: [255, 255, 255, 255] });
    for (const c of win.conns.values()) c.images.set(0, win.img);
  }
  function del(win) {
    cvpush(win, "close 0");
    win.dead = true;
    serveCons(win); serveMouse(win); serveCv(win);
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
    const oplog = [];
    while (o < b.length) {
      const op = String.fromCharCode(b[o]);
      if (typeof process !== "undefined" && process.env.KDRAW) oplog.push(op);
      switch (op) {
      case "b": {                          // id[4] screen[4] refresh[1] chan[4] repl[1] r[16] clipr[16] color[4]
        const id = u32(o + 1), screenid = u32(o + 5);
        const chan = u32(o + 10), repl = b[o + 14];
        const r = [s32(o + 15), s32(o + 19), s32(o + 23), s32(o + 27)];
        const clipr = [s32(o + 31), s32(o + 35), s32(o + 39), s32(o + 43)];
        const color = D.rgba32(u32(o + 47));
        if (screenid !== 0) {
          const scr = conn.screens.get(screenid);
          if (!scr) throw derr(`draw: no screen ${screenid}`);
          // a window on a screen: a view sharing the screen image's backing
          conn.images.set(id, D.newImage(r, { repl, chan: scr.image.chan, clipr,
            color, back: scr.image.back }));
        } else
          conn.images.set(id, D.newImage(r, { repl, chan, clipr, color }));
        o += 51;
        break;
      }
      case "A": {                          // allocscreen: id[4] image[4] fill[4] public[1]
        conn.screens.set(u32(o + 1), { image: img(u32(o + 5)) });
        o += 14;
        break;
      }
      case "F": conn.screens.delete(u32(o + 1)); o += 5; break;
      case "c": {                          // replclipr: id[4] repl[1] clipr[16]
        const im = img(u32(o + 1));
        im.repl = b[o + 5];
        im.clipr = [s32(o + 6), s32(o + 10), s32(o + 14), s32(o + 18)];
        o += 22;
        break;
      }
      case "t": o += 4 + 4 * v.getUint16(o + 2, true); break;  // top/bottom: one window per screen
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
      case "y": {                          // id[4] r[16] data in the image's chan (draw(6))
        const im = img(u32(o + 1));
        const r = [s32(o + 5), s32(o + 9), s32(o + 13), s32(o + 17)];
        const nb = D.loadRows(im, r, b.subarray(o + 21));
        o += 21 + nb;
        break;
      }
      case "i": {                          // id[4] nchars[4] ascent[1]: image becomes a font
        const im = img(u32(o + 1));
        im.font = { nchars: u32(o + 5), ascent: b[o + 9], slots: [] };
        o += 10;
        break;
      }
      case "l": {                          // cache[4] src[4] index[2] r[16] p[8] left[1] width[1]
        const cache = img(u32(o + 1)), src = img(u32(o + 5));
        if (!cache.font) throw derr("draw: l on a non-font image (send i first)");
        const index = v.getUint16(o + 9, true);
        const r = [s32(o + 11), s32(o + 15), s32(o + 19), s32(o + 23)];
        D.copyRect(cache, r, src, [s32(o + 27), s32(o + 31)]);
        cache.font.slots[index] = { r, left: v.getInt8(o + 35), width: b[o + 36] };
        o += 37;
        break;
      }
      case "s": case "x": {                // dst[4] src[4] font[4] p[8] clipr[16] sp[8] [x: bg[4] bgp[8]] ni[2] ni*index[2]
        const dst = img(u32(o + 1)), src = img(u32(o + 5)), fontim = img(u32(o + 9));
        if (!fontim.font) throw derr(`draw: ${op} needs a font image`);
        let x = s32(o + 13);
        const y = s32(o + 17);
        const sp = [s32(o + 37), s32(o + 41)];
        const ni = v.getUint16(o + 45, true);
        let base = o + 47, bg = null, bgp = null;
        if (op === "x") {                  // string with background: libframe's path
          bg = img(u32(o + 47));
          bgp = [s32(o + 51), s32(o + 55)];
          base = o + 59;
        }
        for (let k = 0; k < ni; k++) {
          const slot = fontim.font.slots[v.getUint16(base + 2 * k, true)];
          if (slot) {
            if (bg) {
              D.drawOp(dst, [x, y - fontim.font.ascent, x + slot.width,
                y - fontim.font.ascent + fontim.h], bg, bgp);
              bgp = [bgp[0] + slot.width, bgp[1]];
            }
            D.glyph(dst, [x + slot.left, y - fontim.font.ascent], fontim, slot.r, src, sp);
            x += slot.width;
          }
        }
        o = base + 2 * ni;
        break;
      }
      case "O": o += 2; break;             // set compositing op: S-over-D is all v0 does
      case "v":
        hostRef.host.winPresent?.(win.wid, win.w, win.h, win.img.back.data);
        o += 1;
        break;
      default: throw derr(`draw: message '${op}' not implemented (have b d f L e E y i l s v c A F t)`);
      }
    }
    if (typeof process !== "undefined" && process.env.KDRAW && oplog.length)
      console.error(`[draw ${oplog.join("")}]`);
    return b.length;
  }

  return dev;
}
