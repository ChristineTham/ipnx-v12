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
  const hasSW = "serviceWorker" in navigator;
  const ctl = hasSW && navigator.serviceWorker.controller ? "controlled" : "uncontrolled";
  const line1 = "This demo needs cross-origin isolation and this load did not get it.";
  let advice = "Reload the page normally — and avoid hard reloads (\u2318\u21e7R), " +
    "which deliberately bypass the isolation worker.";
  if (!isSecureContext)
    advice = "This page loaded over plain http:// — an insecure context, where " +
      "browsers remove the worker API entirely (the address bar says Not " +
      "Secure). Use https://" + location.host + location.pathname +
      " — the site now redirects http there.";
  else if (!hasSW)
    advice = "Service workers are unavailable in this Safari profile, and the " +
      "demo cannot isolate without them. Three settings do this: a Private " +
      "Browsing window; Lockdown Mode (system-wide or per-site); or " +
      "Safari \u2192 Settings \u2192 Privacy \u2192 \u201cBlock all " +
      "cookies\u201d, which also removes the worker API. A normal window " +
      "with cookies allowed runs the demo.";
  d.textContent = line1 + " " + advice +
    " If it persists: Safari \u2192 Settings \u2192 Privacy \u2192 Manage Website " +
    "Data \u2192 remove christham.net, then visit again.";
  const diag = document.createElement("div");
  diag.style.cssText = "margin-top:14px;color:#7f8ea0;font-size:12px;user-select:text";
  diag.textContent = `state: secure=${isSecureContext} proto=${location.protocol} sw-api=${hasSW} page=${ctl} build=BUILDSTAMP`;
  d.appendChild(diag);
  if (hasSW)
    navigator.serviceWorker.getRegistration().then((r) => {
      const st = !r ? "none" : r.active ? "active" : r.waiting ? "waiting" : r.installing ? "installing" : "empty";
      diag.textContent += ` registration=${st}`;
      if (r && r.active) diag.textContent += ` script=${(r.active.scriptURL || "").split("/").pop()}`;
    }).catch((e) => { diag.textContent += ` registration=error:${e && e.name}`; });
  desktop.appendChild(d);
  throw new Error("no SAB");
}

// ---------- windows ----------
let ztop = 10;
const vw = () => Math.max(innerWidth, 640);    // the pane can report 0 while hidden
const vh = () => Math.max(innerHeight, 480);
const clampX = (x, w) => Math.max(4, Math.min(x, vw() - Math.min(w, vw()) + 40));
const clampY = (y) => Math.max(4, Math.min(y, vh() - 80));
const DSCALE = 1;                            // draw windows: crisp 1.5x until real fonts land
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
if (!new URLSearchParams(location.search).has("lite"))
  consT.term.writeln("\x1b[38;5;110mthe toolchains stream in behind — cc, go, python, pip (watch the corner)\x1b[0m");

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
    onClose: () => {
      if (gw.cvMode) { wsys?.canvasEvent(id, "close 0"); return; }  // advisory: the app decides
      closeGuest(gw);
    },
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
               t, termEl, drawMode: false, buttons: 0, line: "", scale: DSCALE };
  gwins.set(id, gw);
  win.setFocusInner(() => (gw.drawMode ? gw.canvas : t.term).focus());
  win.onResize(() => {
    if (gw.cvMode) {
      wsys?.canvasEvent(id, `resize 0 ${win.div.offsetWidth} ${win.div.offsetHeight - 30}`);
      return;
    }
    if (!gw.drawMode) { t.fit.fit(); return; }
    // draw windows: the grip scales the view of the fixed raster
    const availW = win.div.offsetWidth - 2, availH = win.div.offsetHeight - 32;
    const k = Math.max(0.5, Math.min(availW / gw.canvas.width, availH / gw.canvas.height));
    sizeDrawWin(gw, Math.round(k * 4) / 4);
  });

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
  // three buttons on modern devices, plan9port's convention: option-click=2,
  // cmd/ctrl-click=3, right/two-finger=3; a forced 1|2|3 for touch
  const btnbits = (e) => {
    if (gw.forceBtn && (e.buttons & 1)) return gw.forceBtn === 2 ? 2 : gw.forceBtn === 3 ? 4 : 1;
    if (e.buttons & 1) { if (e.altKey) return 2; if (e.metaKey || e.ctrlKey) return 4; return 1; }
    return (e.buttons & 4 ? 2 : 0) | (e.buttons & 2 ? 4 : 0);
  };
  canvas.addEventListener("pointerdown", (e) => {
    e.preventDefault(); canvas.focus();
    // capture: a release outside the canvas still reaches us — otherwise
    // gw.buttons sticks and every later modifier-click silently CHORDS
    // against a phantom held button (measured: b2 dead on any touched
    // window, fine on a fresh one)
    try { canvas.setPointerCapture(e.pointerId); } catch (_) {}
    const m = btnbits(e);
    mouse(e, m);
  });
  canvas.addEventListener("pointerup", (e) => { const m = btnbits(e); mouse(e, m); });
  canvas.addEventListener("pointercancel", (e) => mouse(e, 0));
  canvas.addEventListener("lostpointercapture", () => { if (gw.buttons) { gw.buttons = 0; wsys?.mouse(id, 0, 0, 0); } });
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

function sizeDrawWin(gw, scale) {
  const w = gw.canvas.width, h = gw.canvas.height;
  const k = scale || gw.scale || DSCALE;
  gw.scale = k;
  gw.canvas.style.width = Math.round(w * k) + "px";
  gw.canvas.style.height = Math.round(h * k) + "px";
  // integer scales stay pixel-crisp; fractional ones smooth
  gw.canvas.style.imageRendering = Number.isInteger(k) ? "pixelated" : "auto";
  gw.win.div.style.width = (Math.round(w * k) + 2) + "px";
  gw.win.div.style.height = (Math.round(h * k) + 32) + "px";
  const d = gw.win.div;
  d.style.left = clampX(d.offsetLeft, d.offsetWidth) + "px";
  d.style.top = clampY(d.offsetTop) + "px";
}
function addMouseSwitch(gw) {
  const bar = gw.win.div.querySelector(".tbar");
  const sw = document.createElement("span");
  sw.className = "mswitch";
  sw.title = "which mouse button a plain click sends — 1 select · 2 execute · 3 look (or: ⌥-click = 2, ⌘/ctrl-click = 3, right-click = 3)";
  for (const n of [1, 2, 3]) {
    const b = document.createElement("button");
    b.textContent = n;
    if (n === 1) b.classList.add("on");
    b.addEventListener("click", (ev) => {
      ev.stopPropagation();
      gw.forceBtn = n === 1 ? 0 : n;
      sw.querySelectorAll("button").forEach((x) => x.classList.remove("on"));
      b.classList.add("on");
    });
    sw.appendChild(b);
  }
  bar.insertBefore(sw, bar.querySelector(".pid"));
}
function enterDrawMode(gw) {
  if (gw.drawMode) return;
  gw.drawMode = true;
  addMouseSwitch(gw);
  gw.termEl.style.display = "none";
  gw.canvas.style.display = "block";
  gw.win.div.classList.add("drawwin");
  sizeDrawWin(gw);
  gw.canvas.focus();
}

// ---------- the host ----------
// ---- the canvas presenter (docs/canvas.md): the universal SPA's seed.
// stacks are flex, text is text, paths are inline SVG, action=look is a
// real link and action=execute a real button; edit nodes echo locally
// (presenter-local echo, acme's discipline) and report v0 append-only
// insert events. Full diff-reporting edit arrives with console-today.
const cvenc = new TextEncoder();
function renderCaret(gw) {
  gw.cvEl?.querySelectorAll(".cvcaret").forEach((c) => c.remove());
  const ed = gw.cvEdit;
  if (!ed || !ed.el.isConnected) return;
  const c = document.createElement("span");
  c.className = "cvcaret";
  c.textContent = "\u258f";                        // ▏ a thin block
  c.style.cssText = "animation:cvblink 1s steps(1) infinite;color:#334;";
  ed.el.appendChild(c);
}
if (!document.getElementById("cvcaretstyle")) {
  const st = document.createElement("style");
  st.id = "cvcaretstyle";
  st.textContent = "@keyframes cvblink { 50% { opacity: 0; } }";
  document.head.appendChild(st);
}
const cvq = (t) => t.replace(/%/g, "%25").replace(/ /g, "%20").replace(/\n/g, "%0A");
const cvblen = (t) => cvenc.encode(t).length;
function renderCanvas(gw, snap) {
  if (!gw.cvEl) {
    gw.cvEl = document.createElement("div");
    gw.cvEl.tabIndex = 0;
    gw.cvEl.style.cssText = "position:absolute;left:0;right:0;bottom:0;top:30px;overflow:auto;" +
      "background:#fff;color:#111;font:13px/1.45 system-ui,sans-serif;padding:8px;outline:none;";
    gw.win.body.appendChild(gw.cvEl);
    gw.termEl.style.display = "none";
    gw.canvas.style.display = "none";
    gw.cvMode = true;
    gw.win.setFocusInner(() => gw.cvEl.focus());
    setTimeout(() => gw.cvEl.focus(), 0);   // the new window takes the keyboard
    // one delegated keyboard for the window's edit node (v0: the console's
    // single transcript) — the container holds focus, the edit receives
    gw.cvEl.addEventListener("keydown", (e) => {
      const ed = gw.cvEdit;
      if (!ed || e.metaKey || e.ctrlKey) return;
      const plain = () => {                          // text without the caret
        let t = "";
        for (const nd of ed.el.childNodes)
          if (nd.nodeType === 3) t += nd.nodeValue;
        return t;
      };
      const setText = (t) => {
        ed.el.textContent = t;
        renderCaret(gw);
      };
      if (e.key === "Backspace") {
        e.preventDefault();
        const t = plain();
        if (!t.length) return;
        const last = t.slice(-1);
        setText(t.slice(0, -1));                     // presenter-local echo
        const end = cvblen(plain());
        wsys?.canvasEvent(gw.id, `delete ${ed.id} ${end} ${end + cvblen(last)}`);
        return;
      }
      let ch = null;
      if (e.key === "Enter") ch = "\n";
      else if (e.key.length === 1) ch = e.key;
      if (ch === null) return;
      e.preventDefault();
      const at = cvblen(plain());
      setText(plain() + ch);                         // presenter-local echo
      ed.el.scrollIntoView?.(false);
      wsys?.canvasEvent(gw.id, `insert ${ed.id} ${at} ${cvq(ch)}`);
    });
  }
  gw.cvEdit = null;
  const kids = new Map();
  const byid = new Map();
  for (const n of snap) byid.set(n.id, n);
  for (const n of snap) {
    if (n.id === 0) continue;
    const par = +(n.attrs.parent ?? 0);
    if (!kids.has(par)) kids.set(par, []);
    kids.get(par).push(n);
  }
  for (const l of kids.values())
    l.sort((a, b) => (+(a.attrs.order ?? 0) - +(b.attrs.order ?? 0)) || (a.id - b.id));
  const build = (n) => {
    let el;
    if (n.kind === "stack") {
      el = document.createElement("div");
      el.style.display = "flex";
      el.style.flexDirection = n.attrs.dir === "row" ? "row" : "column";
      el.style.gap = "6px";
      if (n.attrs.bg) el.style.background = n.attrs.bg;
      for (const k of kids.get(n.id) ?? []) el.appendChild(build(k));
    } else if (n.kind === "path") {
      el = document.createElementNS("http://www.w3.org/2000/svg", "svg");
      el.setAttribute("viewBox", (n.attrs.viewbox ?? "0 0 100 100").replace(/"/g, ""));
      el.style.cssText = "width:100%;max-height:300px;";
      const pa = document.createElementNS("http://www.w3.org/2000/svg", "path");
      pa.setAttribute("d", n.data);
      pa.setAttribute("stroke", n.attrs.stroke ?? "#111");
      pa.setAttribute("fill", n.attrs.fill ?? "none");
      pa.setAttribute("stroke-width", n.attrs.width ?? "1.5");
      el.appendChild(pa);
    } else {
      const action = n.attrs.action;
      if (action === "look") { el = document.createElement("a"); el.href = "#"; }
      else if (action === "execute") el = document.createElement("button");
      else el = document.createElement(n.kind === "edit" ? "pre" : "div");
      el.textContent = n.data;
      if (n.attrs.bg) el.style.background = n.attrs.bg;
      if (n.kind === "edit") {
        el.style.cssText = "white-space:pre-wrap;margin:0;outline:none;min-height:1.4em;" +
          (n.attrs.bg ? "background:" + n.attrs.bg + ";" : "");
        gw.cvEdit = { el, id: n.id };                // the delegated keyboard's target
        el.addEventListener("mousedown", (e) => {
          e.stopPropagation();
          gw.cvEdit = { el, id: n.id };
          renderCaret(gw);
        });
      }
      if (action)
        el.addEventListener("click", (e) => {
          e.preventDefault();
          wsys?.canvasEvent(gw.id,
            `${action} ${n.id} 0 ${cvblen(n.data)} ${cvq(n.data)}`);
        });
    }
    return el;
  };
  const root = byid.get(0);
  gw.cvEl.replaceChildren(root ? build(root) : document.createTextNode(""));
  renderCaret(gw);
}

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
  winLabel: (id, label) => {
    const gw = gwins.get(id);
    if (!gw) return;
    const isDefault = /^window \d+$/.test(label);
    if (gw.labeled && isDefault && gw.drawMode) {
      // the app restored the default label on its way out (acme does);
      // the v0 kernel sends no close for wrapper-held windows — close here
      gw.win.setTitle(label + " — exited");
      setTimeout(() => { gw.win.remove(); gwins.delete(id); }, 600);
      return;
    }
    if (!isDefault) gw.labeled = true;
    gw.win.setTitle(label);
  },
  winGeom: (id, x, y, w, h) => {
    const gw = gwins.get(id);
    if (!gw) return;
    gw.canvas.width = w; gw.canvas.height = h;
    if (gw.drawMode) sizeDrawWin(gw);
  },
  winClose: (id) => { const gw = gwins.get(id); if (gw) { gw.win.remove(); gwins.delete(id); } },
  winCanvas: (id, snap) => {
    // the browser's own credit system: keep only the LATEST tree and
    // render once per animation frame — coalescing changes the rate,
    // only credit changes the bound, here too
    const gw = gwins.get(id);
    if (!gw) return;
    gw.cvSnap = snap;
    if (!gw.cvRaf) {
      gw.cvRaf = requestAnimationFrame(() => {
        gw.cvRaf = null;
        if (gwins.has(id)) renderCanvas(gw, gw.cvSnap);
      });
    }
  },
  canvasCaps: () => "interactive input",
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
    let data;
    if (Uint8Array.fromBase64) data = Uint8Array.fromBase64(b64);
    else {
      const raw = atob(b64);
      data = new Uint8Array(raw.length);
      for (let j = 0; j < raw.length; j++) data[j] = raw.charCodeAt(j);
    }
    ensure(path.slice(0, Math.max(i, 0)).replace(/^\//, "") ? path.slice(1, i) : "")
      .kids.push({ name: path.slice(i + 1), dir: false, data });
  }
  return root;
}

setStatus("fetching rootfs…");
const params = new URLSearchParams(location.search);
const explicit = ["cc", "go"].filter((k) => params.has(k));
const PROFILES = params.has("lite") ? [] : explicit;   // empty + no lite = all
const WANTALL = !params.has("lite") && explicit.length === 0;
const resp = await fetch("../build/rootfs.json?v=BUILDSTAMP");
const total = +resp.headers.get("content-length") || 0;
let text;
if (resp.body && total) {
  const reader = resp.body.getReader();
  const chunks = []; let got = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value); got += value.length;
    setStatus(`fetching rootfs… ${(got / 1048576).toFixed(0)} / ${(total / 1048576).toFixed(0)} MB`);
  }
  const all = new Uint8Array(got); let o = 0;
  for (const c of chunks) { all.set(c, o); o += c.length; }
  text = new TextDecoder().decode(all);
} else text = await resp.text();
setStatus("unpacking…");
await new Promise((r) => setTimeout(r, 0));      // let the status paint
const files = JSON.parse(text);
setStatus("booting…");
const booted = await boot(host, { rootSeed: seedFromJson(files), interactive: true });
cons = booted.cons;
wsys = booted.wsys;
setStatus("running");
(async () => {                                    // the toolchains stream in
  if (params.has("lite")) return;
  try {
    const man = await (await fetch("../build/overlays.json?v=BUILDSTAMP")).json();
    const profs = WANTALL ? Object.keys(man) : PROFILES.filter((k) => man[k]);
    for (const prof of profs) {
      const label = prof === "cc" ? "C" : "Go";
      const parts = man[prof] ?? [];
      const pieces = {};                           // oversized files, split as
      for (let i = 0; i < parts.length; i++) {     // b64 text across parts
        setStatus(`running · fetching the ${label} toolchain (${i + 1}/${parts.length})…`);
        window.__stream = `fetch ${prof} ${i}`;
        const ov = await (await fetch(`../build/${parts[i]}?v=BUILDSTAMP`)).json();
        for (const k of Object.keys(ov)) {
          const z = k.indexOf("\u0000");
          if (z < 0) continue;
          const path = k.slice(0, z), n = +k.slice(z + 1);
          (pieces[path] ??= [])[n] = ov[k];
          delete ov[k];
        }
        window.__stream = `graft ${prof} ${i}`;
        booted.graft(seedFromJson(ov));
        window.__stream = `grafted ${prof} ${i}`;
        await new Promise((r) => setTimeout(r, 0));
      }
      const folded = {};
      for (const [path, segs] of Object.entries(pieces)) folded[path] = segs.join("");
      if (Object.keys(folded).length) booted.graft(seedFromJson(folded));
      window.__stream = `grafted ${prof} folded`;
      toast(prof === "cc" ? "C toolchain aboard — try: cc hello.c" : "Go toolchain aboard — try: go run hello.go");
    }
  } catch (e) {
    window.__stream = "ERR " + (e && (e.stack || e.message) || e);
    toast("a toolchain failed to load — reload to retry (" + (e && e.message || e) + ")", 8000);
  }
  window.__stream = window.__stream || "done";
  setStatus("running");
})();
statusEl.title = "build BUILDSTAMP";
document.getElementById("brand").title = "build BUILDSTAMP";
const bs = document.createElement("button");
bs.style.cssText = "all:unset;cursor:pointer;color:#4a5866;font-size:10.5px;margin-left:8px;user-select:text";
bs.textContent = "bBUILDSTAMP";
bs.title = "the build stamp — click to copy";
bs.addEventListener("click", () => {
  navigator.clipboard?.writeText("bBUILDSTAMP").then(() => toast("build stamp copied: bBUILDSTAMP"));
});
statusEl.after(bs);
consT.term.focus();
window.__consT = consT;   // debug affordance

// ---------- menu ----------
const feedCons = (line) => {
  consT.term.write(line + "\r\n");
  cons.feed(enc.encode(line + "\n"));
  consT.term.focus();
};
const BIGFONT = "/lib/font/bit/go/regular.13.font";
document.getElementById("mCon").addEventListener("click", () => feedCons("con &"));
document.getElementById("mEdit").addEventListener("click", () => feedCons("edit /usr/kitty/README &"));
document.getElementById("mNew").addEventListener("click", () => feedCons("win rc &"));
document.getElementById("mAcme").addEventListener("click", () => feedCons("win acme -f " + BIGFONT + " &"));
document.getElementById("mSam").addEventListener("click", () => feedCons("font=" + BIGFONT + " win sam &"));
document.getElementById("mTour").addEventListener("click", () => feedCons("rc /rc/tour"));
