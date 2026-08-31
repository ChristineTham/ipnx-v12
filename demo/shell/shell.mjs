// The demo shell: the browser surface's presentation layer. The kernel
// underneath is THE RUST CORE compiled to wasm (rustkern.mjs drives
// build/kernel.wasm — the same kernel/ crate the macOS host runs), with
// Workers, mailboxes and guestcore untouched. Character windows are real
// terminals (xterm.js); draw windows are the real libdraw raster; canvas
// windows are the presenter. Worker startup is serialized (WebKit defect,
// demo/webkit-repro/).
import { boot, makeHandleHostServer } from "../supervisor/rustkern.mjs";
import { createCanvasView } from "./presenter.mjs";
import { createEditor } from "./editor.mjs";

const desktop = document.body;   // the fatal-error banner has no pane to live in
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

const vw = () => Math.max(innerWidth, 640);    // the pane can report 0 while hidden
const vh = () => Math.max(innerHeight, 480);

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

// ---------- emca: panes, tabs, and placement by type ----------
// A window does not float. It lives in a PANE, and which pane is decided by
// its TYPE — panes carry kinds (lists, shells, logs), rows and columns split
// the display, tabs switch between windows of the same type (docs/emca.txt).
const PANES = {
  left:   document.querySelector("#emcaLeft .paneBody"),
  bottom: document.querySelector("#emcaBottom .paneBody"),
  main:   document.getElementById("emcaStack"),
};
const tabsEl = document.getElementById("emcaTabs");

// type -> pane, READ FROM /type — the registry both halves read (window.md).
// Nothing about a type is compiled in here; `dir` is the bootstrap floor, and
// everything else arrives from files. A HINT for placement, never a constraint
// on capability: any window stays fully editable text wherever it lands.
const PANE_OF = { dir: "left", cons: "bottom" };   // the floor, and the console
const paneFor = (type) => PANE_OF[type] ?? "main";

// read the registry once the namespace is up. /type is itself a type, so this
// is the interface learning its own vocabulary from files it could also edit.
async function loadTypes() {
  try {
    const names = dirEntries(await readPath("/type")).filter((e) => e.dir);
    await Promise.all(names.map(async (e) => {
      try {
        const pane = new TextDecoder().decode(await readPath(`/type/${e.name}/pane`)).trim();
        if (pane) PANE_OF[e.name] = pane;
      } catch {}
    }));
    setStatus(`emca — ${Object.keys(PANE_OF).length} types`);
  } catch (err) { /* no registry on this host: the floor still stands */ }
}

const mainTabs = [];            // windows of the same type, switched
let activeTab = null;

function renderTabs() {
  tabsEl.replaceChildren();
  for (const w of mainTabs) {
    const b = document.createElement("button");
    b.textContent = (w.dirty ? "\u25CF " : "") + w.title;
    b.title = w.title;
    if (w === activeTab) b.className = "on";
    b.addEventListener("click", () => selectTab(w));
    tabsEl.appendChild(b);
  }
}
function selectTab(w) {
  activeTab = w;
  for (const x of mainTabs) x.div.classList.toggle("off", x !== w);
  renderTabs();
  setTimeout(() => w.focusInner?.(), 0);
}

function makeWindow({ title, type, closable, onClose }) {
  const pane = paneFor(type);
  const div = document.createElement("div");
  div.className = "ewin";
  div.innerHTML = `<div class="wbody"></div>`;
  PANES[pane].appendChild(div);

  const rec = { div, title, dirty: false, focusInner: null, pane };
  if (pane === "main") { mainTabs.push(rec); selectTab(rec); }

  const sized = [];
  const focus = () => {
    if (pane === "main" && activeTab !== rec) selectTab(rec);
    else setTimeout(() => rec.focusInner?.(), 0);
  };
  div.addEventListener("pointerdown", focus, true);
  const ro = new ResizeObserver(() => sized.forEach((f) => f()));
  ro.observe(div);

  return {
    div, body: div.querySelector(".wbody"),
    setTitle: (s) => { rec.title = s; if (pane === "main") renderTabs(); },
    setPid: () => {},                       // the tab strip carries identity now
    setDirty: (d) => { rec.dirty = !!d; if (pane === "main") renderTabs(); },
    onResize: (f) => sized.push(f), focus,
    setFocusInner: (f) => { rec.focusInner = f; },
    // emca places by writing ctl; with NO EMCA RUNNING the surface falls back
    // to the type's default pane — the "degrades correctly" property, in code
    setPane: (type) => {
      const want = paneFor(type);
      if (want === rec.pane) return;
      const i = mainTabs.indexOf(rec);
      if (i >= 0) {
        mainTabs.splice(i, 1);
        if (activeTab === rec) { activeTab = null; if (mainTabs.length) selectTab(mainTabs[mainTabs.length - 1]); }
      }
      rec.pane = want;
      div.classList.remove("off");
      PANES[want].appendChild(div);
      if (want === "main") { mainTabs.push(rec); selectTab(rec); }
      renderTabs();
    },
    remove: () => {
      ro.disconnect(); div.remove();
      const i = mainTabs.indexOf(rec);
      if (i >= 0) {
        mainTabs.splice(i, 1);
        if (activeTab === rec) { activeTab = null; if (mainTabs.length) selectTab(mainTabs[mainTabs.length - 1]); else renderTabs(); }
        else renderTabs();
      }
      if (closable && onClose) { /* the owner already decided */ }
    },
  };
}

// ---------- the system console ----------
let cons = null;               // set after boot; cooked line discipline below
const consWin = makeWindow({
  title: "rc — the transcript", type: "cons", closable: false,
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
// The SURFACE opens the file and the BROWSER renders it — IPNX implements no
// renderers at all. What the file IS decides how it is shown, which is why an
// SVG or a PNG costs this design nothing: the platform already knows.
// stat(5): size[2] type[2] dev[4] qid[13] mode[4] atime[4] mtime[4] length[8]
// name[s] … — a directory read returns these, and the host knows the format,
// so the HOST renders it. Same claim as SVG or PNG: IPNX ships no renderer.
function dirEntries(bytes) {
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const td2 = new TextDecoder();
  const out = [];
  let o = 0;
  while (o + 2 <= bytes.length) {
    const sz = dv.getUint16(o, true);
    if (sz < 41 || o + 2 + sz > bytes.length) break;
    const rec = o + 2;
    const mode = dv.getUint32(rec + 19, true);
    const len = dv.getUint16(rec + 39, true);
    out.push({ name: td2.decode(bytes.subarray(rec + 41, rec + 41 + len)),
               dir: (mode & 0x80000000) !== 0 });
    o += 2 + sz;
  }
  // self-validating: a real directory read consumes the WHOLE buffer. The host
  // renders by what the file IS, not by what a type claimed it would be.
  return (out.length && o === bytes.length) ? out : null;
}

async function showContent(gw, path, type) {
  let host = gw.contentEl;
  if (!host) {
    host = document.createElement("div");
    host.style.cssText = "position:absolute;left:0;right:0;top:30px;bottom:0;overflow:auto;" +
      "background:#fff;color:#111;";
    gw.win.body.appendChild(host);
    gw.contentEl = host;
  }
  host.replaceChildren();
  let bytes;
  try { bytes = await readPath(path); }
  catch (e) { host.textContent = `${path}: ${e.message}`; return; }
  const head = new TextDecoder().decode(bytes.slice(0, 512)).trimStart();
  const ext = (path.split(".").pop() || "").toLowerCase();
  const put = (el, css = "") => { el.style.cssText = css; host.appendChild(el); };
  const ents = dirEntries(bytes);
  if (ents) {
    // every LINE is a look target — type drives interactivity, which is what
    // buys back the single tap for structured output (docs/emca.txt)
    const ul = document.createElement("div");
    ul.style.cssText = "padding:4px 0;font:500 12px/1.5 ui-monospace,Menlo,monospace;";
    for (const e of ents) {
      const a = document.createElement("a");
      a.href = "#";
      a.textContent = e.name + (e.dir ? "/" : "");
      a.style.cssText = "display:block;padding:2px 10px;color:#123;text-decoration:none;";
      a.addEventListener("mouseenter", () => { a.style.background = "#eaffff"; });
      a.addEventListener("mouseleave", () => { a.style.background = "none"; });
      a.addEventListener("click", (ev) => {
        ev.preventDefault();
        const full = (path.endsWith("/") ? path : path + "/") + e.name;
        feedCons(`rc /rc/emcaopen ${e.dir ? "dir" : "text"} ${full}`);
      });
      ul.appendChild(a);
    }
    put(ul);
    return;
  }
  if (ext === "svg" || head.startsWith("<svg") || head.startsWith("<?xml")) {
    const d = document.createElement("div");                 // the browser draws it
    d.innerHTML = new TextDecoder().decode(bytes);
    put(d, "padding:8px;");
  } else if (["png","jpg","jpeg","gif","webp"].includes(ext)) {
    const img = document.createElement("img");
    img.src = URL.createObjectURL(new Blob([bytes]));
    put(img, "max-width:100%;display:block;margin:8px;");
  } else if (ext === "html" || ext === "htm") {
    const f = document.createElement("iframe");
    f.setAttribute("sandbox", "");                            // untrusted content
    f.srcdoc = new TextDecoder().decode(bytes);
    put(f, "width:100%;height:100%;border:0;");
  } else {
    // TEXT IS A REAL EDITOR. Everything that is input — caret, selection,
    // clipboard, IME, wrapping, find — belongs to the component, and none of it
    // is written here. What crosses to IPNX is the buffer as insert/delete with
    // a sequence and a hash, the selection, and the verbs.
    gw.editor?.destroy();
    gw.putFn = null;
    const ed = createEditor({
      mount: host,
      text: new TextDecoder().decode(bytes),
      send: (line) => wsys?.winEvent?.(gw.id, line),
      onPut: async (s) => {
        try {
          const wrote = await writePath(path, new TextEncoder().encode(s));
          wsys?.winEvent?.(gw.id, "dirty 0");
          wsys?.winEvent?.(gw.id, `exec Put`);
          console.log(`Put ${path}: ${wrote} bytes`);
        } catch (e) { console.error("Put failed:", e.message); }
      },
    });
    gw.editor = ed;
    gw.putFn = () => ed && ed.text && ed.view && edPut(ed, path, gw);
  }
}

let wsys = null;
let readPath = null;   // the surface's own open (docs/window.md)
let writePath = null;  // and Put, streaming the edited file back

// Put from the toolbar takes the same road as Cmd-S
async function edPut(ed, path, gw) {
  try {
    const wrote = await writePath(path, new TextEncoder().encode(ed.text()));
    wsys?.winEvent?.(gw.id, "dirty 0");
    wsys?.winEvent?.(gw.id, "exec Put");
    console.log(`Put ${path}: ${wrote} bytes`);
  } catch (e) { console.error("Put failed:", e.message); }
}
const gwins = new Map();

function winCreate(id, x, y, w, h, title) {
  // cascade new windows clear of the console (which sits at 60,46 720x460)
  const win = makeWindow({
    title, closable: true,
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
  // the window fills its pane; only the raster is scaled
}
function addMouseSwitch(gw) {
  // the raster exhibit's button switch lived in the old titlebar; under emca a
  // window has no chrome of its own, so it rides above the raster instead
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
  gw.win.div.insertBefore(sw, gw.win.body);
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
  // /dev/snarf <-> the host clipboard, the hypervisor's own habit
  // (Parallels precedent, her words): writes push, reads pull where the
  // platform permits; Safari's gesture-only reads degrade to the buffer
  snarfSet: (text) => { try { navigator.clipboard?.writeText(text); } catch {} },
  snarfGet: () => { try { return navigator.clipboard?.readText?.(); } catch { return null; } },
  // /dev/window: IPNX declared this window's chrome and the host renders it
  // NATIVELY. Nothing here parses content — that is a path the host opens over
  // 9P. Actions name a side: ipnx: round-trips, host: never leaves the surface.
  winChrome: (id, ch) => {
    const gw = gwins.get(id);
    if (!gw) return;
    gw.win.setPane?.(ch.type);
    if (ch.content) gw.win.setTitle(ch.content.split("/").filter(Boolean).pop() || ch.content);
    gw.termEl && (gw.termEl.style.display = "none");
    gw.canvas && (gw.canvas.style.display = "none");
    gw.win.div.querySelector(".mswitch")?.remove();   // not a raster window
    gw.win.div.classList.remove("drawwin");
    let bar = gw.chromeEl;
    if (!bar) {
      bar = document.createElement("div");
      bar.style.cssText = "display:flex;align-items:center;gap:6px;flex-wrap:wrap;" +
        "padding:4px 6px;background:#eaffff;border-bottom:1px solid #000;" +
        "font:500 12px/1 'Lucida Grande',system-ui,sans-serif;color:#123;";
      gw.win.body.prepend(bar);
      gw.chromeEl = bar;
    }
    bar.replaceChildren();
    if (ch.content) {
      const t = document.createElement("span");
      t.textContent = ch.content;
      t.title = `${ch.type} window — content opened by the surface`;
      t.style.cssText = "font-weight:600;margin-right:4px;white-space:nowrap;";
      bar.appendChild(t);
      if (ch.content !== gw.shownContent) { gw.shownContent = ch.content; showContent(gw, ch.content, ch.type); }
    }
    for (const line of ch.toolbar.split("\n")) {
      const s = line.trim();
      if (!s) continue;
      const sp = s.indexOf(" ");
      const label = sp < 0 ? s : s.slice(0, sp);
      const action = sp < 0 ? "" : s.slice(sp + 1).trim();
      const b = document.createElement("button");
      b.textContent = label;
      b.title = action;
      const hostSide = action.startsWith("host:");
      b.style.cssText = "font:500 12px/1 inherit;padding:3px 8px;min-height:22px;" +
        "border:1px solid #9aa;border-radius:4px;cursor:pointer;white-space:nowrap;" +
        (hostSide ? "background:#f4f4ff;color:#334;" : "background:#f6ffff;color:#123;");
      b.addEventListener("click", () => {
        if (hostSide) {
          // never round-trips — layout is the surface's half
          if (action === "host:toggle-wrap") gw.win.body.style.whiteSpace =
            gw.win.body.style.whiteSpace === "pre" ? "pre-wrap" : "pre";
          return;
        }
        if (label === "Put" && gw.putFn) { gw.putFn(); return; }
        wsys?.winEvent?.(id, `exec ${label}`);
      });
      bar.appendChild(b);
    }
    if (ch.tag) {
      const f = document.createElement("input");
      f.value = ch.tag;
      f.style.cssText = "flex:1 1 8ch;min-width:8ch;border:1px solid #9aa;border-radius:4px;" +
        "padding:3px 5px;font:inherit;background:rgba(255,255,255,.6);color:#123;";
      f.addEventListener("keydown", (e) => {
        if (e.key !== "Enter") return;
        e.preventDefault();
        wsys?.winEvent?.(id, `tag ${f.value}`);   // the way back into sam
      });
      bar.appendChild(f);
    }
  },
  winCanvas: (id, snap) => {
    const gw = gwins.get(id);
    if (!gw) return;
    if (!gw.cv) {
      // the window becomes the browser surface's client
      gw.cv = createCanvasView({
        mount: gw.win.body,
        send: (line) => wsys?.canvasEvent(id, line),
        snarf: (t) => wsys?.snarfPut?.(t),
      });
      gw.termEl.style.display = "none";
      gw.canvas.style.display = "none";
      gw.cvMode = true;
      gw.win.setFocusInner(() => gw.cv.focus());
    }
    gw.cv.update(snap);
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
setStatus("fetching the kernel…");
const kernelWasm = await (await fetch("../build/kernel.wasm?v=BUILDSTAMP")).arrayBuffer();
setStatus("booting…");
const zserver = makeHandleHostServer();
const booted = await boot(host, { rootSeed: seedFromJson(files), interactive: true,
  kernelWasm, caps: "interactive input", hostServe: zserver });
// ---- the home chooser: play (ramfs) · browser storage (OPFS) · a real
// local folder (File System Access). A granted directory IS a bind.
{
  const sel = document.getElementById("mHome");
  let current = "play";
  sel?.addEventListener("change", async () => {
    const want = sel.value;
    try {
      if (want === "play") {
        feedCons("unmount /n/host >[2]/dev/null; cd /usr/kitty");
        toast("home: the shipped examples — nothing persists");
      } else if (want === "browser") {
        const h = await navigator.storage.getDirectory();
        zserver.grant(h);
        feedCons("bind -c '#Z' /n/host; cd /n/host");
        toast("home: browser storage — /n/host persists in this browser, private to this site");
      } else if (want === "folder") {
        const h = await window.showDirectoryPicker({ mode: "readwrite" });
        zserver.grant(h);
        feedCons("bind -c '#Z' /n/host; cd /n/host");
        toast("home: " + h.name + " — /n/host is your real folder; Put writes to your disk");
      }
      current = want;
    } catch (e) {
      sel.value = current;               // picker cancelled or refused
      if (want === "folder") toast("no folder granted — home unchanged");
    }
  });
}
cons = booted.cons;
wsys = booted.wsys;
readPath = booted.readPath;
writePath = booted.writePath;
window.__emca = { readPath, writePath, wsys, PANE_OF };   // debug affordance, as __consT
loadTypes();
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
// THE TOP SURFACE IS THE SYSTEM'S. Its operand is the system, so the managers
// live here — and each one is only a typed window onto a filesystem, which is
// why "adding a manager is adding a file" (docs/emca.txt).
for (const b of document.querySelectorAll("#managers button"))
  b.addEventListener("click", () => feedCons(`rc /rc/emcaopen ${b.dataset.open}`));
document.getElementById("mAcme").addEventListener("click", () => feedCons("acme9 &"));
document.getElementById("mTour").addEventListener("click", () => feedCons("rc /rc/tour"));

// the pane toggles: how the responsive layout is driven by hand. host: side —
// these never round-trip, because layout is the surface's half.
const togglePane = (el, btn) => { el.classList.toggle("hidden"); btn.classList.toggle("off"); };
document.getElementById("tLeft").addEventListener("click",
  () => togglePane(document.getElementById("emcaLeft"), document.getElementById("tLeft")));
document.getElementById("tBottom").addEventListener("click",
  () => togglePane(document.getElementById("emcaBottom"), document.getElementById("tBottom")));
addEventListener("keydown", (e) => {
  if (!(e.metaKey || e.ctrlKey)) return;
  if (e.key === "b") { e.preventDefault(); document.getElementById("tLeft").click(); }
  if (e.key === "j") { e.preventDefault(); document.getElementById("tBottom").click(); }
});

// the GLOBAL TAG — acme's root tag, at workspace scope, in the status line
// THE SYSTEM BOOTS INTO EMCA. The default workspace declares itself — motd in
// an editor tab, the home directory in the rail — and rc is already the
// transcript below. No menu, no desktop, no shell in front of it.
setTimeout(() => feedCons("rc /rc/emca"), 900);

const gt = document.getElementById("globalTag");
gt.addEventListener("keydown", (e) => {
  if (e.key !== "Enter") return;
  e.preventDefault();
  const line = gt.value.trim();
  if (line) feedCons(line);
});
