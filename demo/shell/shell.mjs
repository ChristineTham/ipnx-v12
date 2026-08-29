// The demo shell: the browser host's presentation layer, arriving ahead of M5.
// The kernel underneath is the frozen reference (../supervisor/*, byte-identical);
// this file replaces only the page — the part the demo owns. Character windows
// are real terminals (xterm.js, per the design's own naming of it); draw
// windows are the real libdraw raster, shown crisp; windows carry macOS-style
// chrome. Worker startup is serialized (WebKit defect, demo/webkit-repro/).
import { boot } from "../supervisor/kernel.mjs";

const desktop = document.getElementById("desktop");
const statusEl = document.getElementById("status");
const td = new TextDecoder();
const enc = new TextEncoder();

const setStatus = (s) => { statusEl.textContent = s; };
const toast = (msg, ms = 2600) => {
  const t = document.createElement("div");
  t.className = "toast"; t.textContent = msg;
  document.body.appendChild(t);
  setTimeout(() => t.remove(), ms);
};

if (!crossOriginIsolated) {
  setStatus("not isolated");
  const d = document.createElement("div");
  d.style.cssText = "padding:60px;max-width:34rem;margin:auto;user-select:text";
  d.textContent = "This demo needs cross-origin isolation and the page did not " +
    "get it — a hard reload usually fixes it (the isolation shim takes control " +
    "on the second load).";
  desktop.appendChild(d);
  throw new Error("no SAB");
}

// ---------- windows ----------
let ztop = 10;
const vw = () => Math.max(innerWidth, 640);    // the pane can report 0 while hidden
const vh = () => Math.max(innerHeight, 480);
const clampX = (x, w) => Math.max(4, Math.min(x, vw() - Math.min(w, vw()) + 40));
const clampY = (y) => Math.max(4, Math.min(y, vh() - 80));
const DSCALE = 1.5;                            // draw windows: crisp 1.5x until real fonts land
const MONO = 'ui-monospace, "SF Mono", Menlo, Consolas, "DejaVu Sans Mono", monospace';

function makeTerm(el) {
  const term = new Terminal({
    fontFamily: MONO, fontSize: 13, lineHeight: 1.25,
    cursorBlink: true, scrollback: 4000,
    theme: { background: "#0d1117", foreground: "#d6dde4", cursor: "#9ecbff",
             selectionBackground: "#264f78" },
  });
  const fit = new FitAddon.FitAddon();
  term.loadAddon(fit);
  term.open(el);
  fit.fit();
  return { term, fit };
}

function makeWindow({ title, x, y, w, h, closable, onClose }) {
  const div = document.createElement("div");
  div.className = "win";
  div.style.left = clampX(x, w) + "px"; div.style.top = clampY(y) + "px";
  div.style.width = w + "px"; div.style.height = h + "px";
  div.style.zIndex = ++ztop;
  // static chrome markup; every dynamic string below goes through textContent
  div.innerHTML = `
    <div class="tbar">
      <span class="lights">
        <button class="light close" title="close"></button>
        <button class="light zoom" title="bigger"></button>
      </span>
      <span class="ttitle"></span><span class="pid"></span>
    </div>
    <div class="wbody"></div>
    <div class="grip" title="resize"></div>`;
  div.querySelector(".ttitle").textContent = title;
  desktop.appendChild(div);

  let focusInner = null;      // set by the owner: focuses the term/canvas
  const focus = () => {
    div.style.zIndex = ++ztop;
    document.querySelectorAll(".win.focused").forEach((e) => e.classList.remove("focused"));
    div.classList.add("focused");
    if (focusInner) setTimeout(focusInner, 0);
  };
  div.addEventListener("pointerdown", focus, true);

  // drag by titlebar
  const tbar = div.querySelector(".tbar");
  tbar.addEventListener("pointerdown", (e) => {
    if (e.target.closest(".light")) return;
    const sx = e.clientX - div.offsetLeft, sy = e.clientY - div.offsetTop;
    const move = (ev) => {
      div.style.left = Math.max(0, ev.clientX - sx) + "px";
      div.style.top = Math.max(0, ev.clientY - sy) + "px";
    };
    const up = () => { removeEventListener("pointermove", move); removeEventListener("pointerup", up); };
    addEventListener("pointermove", move); addEventListener("pointerup", up);
  });

  // resize by grip
  const grip = div.querySelector(".grip");
  const sized = [];           // callbacks after resize
  grip.addEventListener("pointerdown", (e) => {
    e.preventDefault(); e.stopPropagation();
    const sw = div.offsetWidth - e.clientX, sh = div.offsetHeight - e.clientY;
    const move = (ev) => {
      div.style.width = Math.max(240, ev.clientX + sw) + "px";
      div.style.height = Math.max(140, ev.clientY + sh) + "px";
      sized.forEach((f) => f());
    };
    const up = () => { removeEventListener("pointermove", move); removeEventListener("pointerup", up); };
    addEventListener("pointermove", move); addEventListener("pointerup", up);
  });

  const closeBtn = div.querySelector(".light.close");
  if (closable) closeBtn.addEventListener("click", () => onClose && onClose());
  else closeBtn.disabled = true;
  div.querySelector(".light.zoom").addEventListener("click", () => {
    const big = div.dataset.big === "1";
    if (big) { div.style.width = div.dataset.w; div.style.height = div.dataset.h; div.dataset.big = "0"; }
    else {
      div.dataset.w = div.style.width; div.dataset.h = div.style.height;
      div.style.width = Math.min(innerWidth - 40, 980) + "px";
      div.style.height = Math.min(innerHeight - 80, 640) + "px";
      div.dataset.big = "1";
    }
    sized.forEach((f) => f());
  });

  return { div, body: div.querySelector(".wbody"),
           setTitle: (t) => { div.querySelector(".ttitle").textContent = t; },
           setPid: (t) => { div.querySelector(".pid").textContent = t; },
           onResize: (f) => sized.push(f), focus,
           setFocusInner: (f) => { focusInner = f; },
           remove: () => div.remove() };
}

// ---------- the system console ----------
let cons = null;               // set after boot; cooked line discipline below
const consWin = makeWindow({
  title: "console — ipnx-v12", x: 60, y: 46, w: 720, h: 460, closable: false,
});
const consTermEl = document.createElement("div");
consTermEl.className = "term";
consWin.body.appendChild(consTermEl);
const consT = makeTerm(consTermEl);
consWin.setFocusInner(() => consT.term.focus());
consWin.onResize(() => consT.fit.fit());
consT.term.writeln("\x1b[38;5;110mipnx-v12 — a reimagining of Unix, booting in this tab\x1b[0m");

// cooked line discipline for /dev/cons (the kernel's cons expects fed lines;
// the echo is the host's job, as on the Node host's tty)
let consLine = "";
consT.term.onData((d) => {
  if (!cons) return;
  for (const ch of d) {
    if (ch === "\r" || ch === "\n") {
      consT.term.write("\r\n");
      cons.feed(enc.encode(consLine + "\n"));
      consLine = "";
    } else if (ch === "\x7f" || ch === "\b") {
      if (consLine.length) { consLine = consLine.slice(0, -1); consT.term.write("\b \b"); }
    } else if (ch >= " " || ch === "\t") {
      consLine += ch;
      consT.term.write(ch);
    }
  }
});

// ---------- guest windows (#w) ----------
let wsys = null;
const gwins = new Map();

function winCreate(id, x, y, w, h, title) {
  // cascade new windows clear of the console (which sits at 60,46 720x460)
  const win = makeWindow({
    title, x: 560 + (id * 30) % 200, y: 60 + (id * 26) % 260,
    w: Math.max(w + 16, 460), h: Math.max(h + 46, 340), closable: true,
    onClose: () => closeGuest(gw),
  });
  win.setPid("#" + id);

  const termEl = document.createElement("div");
  termEl.className = "term";
  win.body.appendChild(termEl);
  const t = makeTerm(termEl);

  const canvas = document.createElement("canvas");
  canvas.width = w; canvas.height = h;
  canvas.style.display = "none";
  win.body.appendChild(canvas);

  const gw = { id, win, canvas, ctx: canvas.getContext("2d"),
               t, termEl, drawMode: false, buttons: 0, line: "" };
  gwins.set(id, gw);
  win.setFocusInner(() => (gw.drawMode ? gw.canvas : t.term).focus());
  win.onResize(() => { if (!gw.drawMode) t.fit.fit(); });

  // keyboard: bytes to the window's cons via wsys.key; host-side echo, cooked
  t.term.onData((d) => {
    if (!wsys) return;
    for (const ch of d) {
      if (ch === "\r" || ch === "\n") {
        t.term.write("\r\n");
        for (const b of enc.encode(gw.line + "\n")) wsys.key(id, b);
        gw.line = "";
      } else if (ch === "\x7f" || ch === "\b") {
        if (gw.line.length) { gw.line = gw.line.slice(0, -1); t.term.write("\b \b"); }
      } else if (ch >= " " || ch === "\t") {
        gw.line += ch;
        t.term.write(ch);
      }
    }
  });

  // draw-mode input: raw keys + mouse on the canvas
  canvas.tabIndex = 0;
  canvas.addEventListener("keydown", (e) => {
    if (!wsys) return;
    let bytes = null;
    if (e.key === "Enter") bytes = [10];
    else if (e.key === "Backspace") bytes = [8];
    else if (e.key === "Escape") bytes = [27];
    else if (e.key.length === 1 && !e.metaKey && !e.ctrlKey) bytes = [...enc.encode(e.key)];
    if (!bytes) return;
    e.preventDefault();
    for (const b of bytes) wsys.key(id, b);
  });
  const mouse = (e, downs) => {
    if (downs !== undefined) gw.buttons = downs;
    const r = canvas.getBoundingClientRect();
    const sx = canvas.width / r.width, sy = canvas.height / r.height;
    wsys?.mouse(id, Math.round((e.clientX - r.left) * sx), Math.round((e.clientY - r.top) * sy), gw.buttons);
  };
  const btnbits = (e) => (e.buttons & 1 ? 1 : 0) | (e.buttons & 4 ? 2 : 0) | (e.buttons & 2 ? 4 : 0);
  canvas.addEventListener("pointerdown", (e) => { e.preventDefault(); canvas.focus(); mouse(e, btnbits(e)); });
  canvas.addEventListener("pointerup", (e) => mouse(e, btnbits(e)));
  canvas.addEventListener("pointermove", (e) => { if (gw.buttons) mouse(e); });
  canvas.addEventListener("contextmenu", (e) => e.preventDefault());

  win.focus();
  setTimeout(() => t.term.focus(), 0);
  return gw;
}

function closeGuest(gw) {
  if (gw.drawMode) {
    // the raster clients own their exit; hide the window, keep it running
    gw.win.div.style.display = "none";
    toast("window hidden — its program is still running (draw clients exit on their own terms)");
  } else {
    // a shell window: end it the shell way
    for (const b of enc.encode("exit\n")) wsys.key(gw.id, b);
  }
}

function sizeDrawWin(gw) {
  const w = gw.canvas.width, h = gw.canvas.height;
  gw.canvas.style.width = Math.round(w * DSCALE) + "px";
  gw.canvas.style.height = Math.round(h * DSCALE) + "px";
  gw.win.div.style.width = (Math.round(w * DSCALE) + 2) + "px";
  gw.win.div.style.height = (Math.round(h * DSCALE) + 32) + "px";
  const d = gw.win.div;
  d.style.left = clampX(d.offsetLeft, d.offsetWidth) + "px";
  d.style.top = clampY(d.offsetTop) + "px";
}
function enterDrawMode(gw) {
  if (gw.drawMode) return;
  gw.drawMode = true;
  gw.termEl.style.display = "none";
  gw.canvas.style.display = "block";
  gw.win.div.classList.add("drawwin");
  sizeDrawWin(gw);
  gw.canvas.focus();
}

// ---------- the host ----------
const host = {
  spawnWorker: (initMsg, transfer) => {
    let w = null, pending = [[initMsg, transfer]], dead = false;
    let handler = () => {};
    let errh = () => {};
    const attach = (ww) => {
      w = ww;
      ww.addEventListener("message", (e) => handler(e.data));
      ww.addEventListener("error", (e) => errh((e && (e.message || "worker error")) + ""));
      const q = pending; pending = null;
      for (const [m, tr] of q) ww.postMessage(m, tr);
      if (dead) ww.terminate();
    };
    // serialize module-worker creation: WebKit fails a load that starts while
    // another is in flight through a service worker (demo/webkit-repro/)
    globalThis.__wgate = (globalThis.__wgate || Promise.resolve()).then(() => new Promise((release) => {
      const ww = new Worker(new URL("../browser/worker.mjs", import.meta.url), { type: "module" });
      attach(ww);
      let done = false;
      const go = () => { if (!done) { done = true; clearTimeout(bt); release(); } };
      const bt = setTimeout(go, 3000);
      ww.addEventListener("message", go, { once: true });
      ww.addEventListener("error", go, { once: true });
    }));
    return {
      post: (m, tr = []) => { if (pending) pending.push([m, tr]); else w.postMessage(m, tr); },
      setHandler: (cb) => { handler = cb; },
      onMessage: (cb) => { handler = cb; },
      onError: (cb) => { errh = cb; },
      terminate: () => { dead = true; if (w) w.terminate(); },
    };
  },
  consWrite: (bytes) => consT.term.write(td.decode(bytes).replace(/\n/g, "\r\n")),
  error: (text) => consT.term.write("\x1b[31m" + text + "\x1b[0m\r\n"),
  winCreate,
  winPresent: (id, w, h, rgba) => {
    const gw = gwins.get(id);
    if (!gw) return;
    if (!gw.drawMode) { gw.canvas.width = w; gw.canvas.height = h; enterDrawMode(gw); }
    else if (gw.canvas.width !== w || gw.canvas.height !== h) { gw.canvas.width = w; gw.canvas.height = h; sizeDrawWin(gw); }
    gw.ctx.putImageData(new ImageData(new Uint8ClampedArray(rgba), w, h), 0, 0);
  },
  winText: (id, bytes) => {
    const gw = gwins.get(id);
    if (gw) gw.t.term.write(td.decode(bytes).replace(/\n/g, "\r\n"));
  },
  winLabel: (id, label) => { const gw = gwins.get(id); if (gw) gw.win.setTitle(label); },
  winGeom: (id, x, y, w, h) => {
    const gw = gwins.get(id);
    if (!gw) return;
    gw.canvas.width = w; gw.canvas.height = h;
    if (gw.drawMode) sizeDrawWin(gw);
  },
  winClose: (id) => { const gw = gwins.get(id); if (gw) { gw.win.remove(); gwins.delete(id); } },
  exit: (code) => {
    setStatus(code === 0 ? "halted, clean" : "halted, exit " + code);
    consT.term.write("\r\n\x1b[33m[system halted: exit " + code + "]\x1b[0m\r\n");
  },
};

// ---------- boot ----------
function seedFromJson(files) {
  const root = { name: "/", dir: true, kids: [] };
  const dirs = new Map([["", root]]);
  const ensure = (dir) => {
    if (dirs.has(dir)) return dirs.get(dir);
    const i = dir.lastIndexOf("/");
    const parent = ensure(dir.slice(0, Math.max(i, 0)));
    const node = { name: dir.slice(i + 1), dir: true, kids: [] };
    parent.kids.push(node); dirs.set(dir, node);
    return node;
  };
  for (const [path, b64] of Object.entries(files)) {
    if (b64 === null) { ensure(path.replace(/^\//, "").replace(/\/$/, "")); continue; }
    const i = path.lastIndexOf("/");
    const raw = atob(b64);
    const data = new Uint8Array(raw.length);
    for (let j = 0; j < raw.length; j++) data[j] = raw.charCodeAt(j);
    ensure(path.slice(0, Math.max(i, 0)).replace(/^\//, "") ? path.slice(1, i) : "")
      .kids.push({ name: path.slice(i + 1), dir: false, data });
  }
  return root;
}

setStatus("fetching rootfs…");
const files = await (await fetch("../build/rootfs.json")).json();
setStatus("booting…");
const booted = await boot(host, { rootSeed: seedFromJson(files), interactive: true });
cons = booted.cons;
wsys = booted.wsys;
setStatus("running");
consT.term.focus();
window.__consT = consT;   // debug affordance

// ---------- menu ----------
const feedCons = (line) => {
  consT.term.write(line + "\r\n");
  cons.feed(enc.encode(line + "\n"));
  consT.term.focus();
};
document.getElementById("mNew").addEventListener("click", () => feedCons("win rc &"));
document.getElementById("mTour").addEventListener("click", () => feedCons("rc /rc/tour"));
