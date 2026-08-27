// Browser host for the hosted kernel: fetch-seeded rootfs, Web Workers, a
// console window in the DOM. The kernel (../supervisor/kernel.mjs) is the
// same code Node runs. Requires cross-origin isolation (serve.mjs's COOP/
// COEP headers) for SharedArrayBuffer.
import { boot } from "../supervisor/kernel.mjs";

const out = document.getElementById("out");
const input = document.getElementById("in");
const status = document.getElementById("status");
const td = new TextDecoder();

function append(text, cls) {
  const span = document.createElement("span");
  if (cls) span.className = cls;
  span.textContent = text;
  out.appendChild(span);
  out.parentElement.scrollTop = out.parentElement.scrollHeight;
}

if (!crossOriginIsolated) {
  append("not cross-origin isolated — serve this page with poc/serve.mjs\n", "err");
  throw new Error("no SAB");
}

function seedFromJson(files) {
  const root = { name: "/", dir: true, kids: [] };
  const dirs = new Map([["", root]]);
  const ensure = (dir) => {
    if (dirs.has(dir)) return dirs.get(dir);
    const i = dir.lastIndexOf("/");
    const parent = ensure(dir.slice(0, Math.max(i, 0)));
    const node = { name: dir.slice(i + 1), dir: true, kids: [] };
    parent.kids.push(node);
    dirs.set(dir, node);
    return node;
  };
  for (const [path, b64] of Object.entries(files)) {
    if (b64 === null) { ensure(path.replace(/^\//, "").replace(/\/$/, "")); continue; }  // empty-dir marker
    const i = path.lastIndexOf("/");
    const raw = atob(b64);
    const data = new Uint8Array(raw.length);
    for (let j = 0; j < raw.length; j++) data[j] = raw.charCodeAt(j);
    ensure(path.slice(0, Math.max(i, 0)).replace(/^\//, "") ? path.slice(1, i) : "")
      .kids.push({ name: path.slice(i + 1), dir: false, data });
  }
  return root;
}

const interactive = new URLSearchParams(location.search).has("i");

// ---- guest windows: elements backed by per-window namespaces (#w) ----
let wsys = null;                                  // set after boot
let ztop = 10;
const gwins = new Map();
function winCreate(id, x, y, w, h, title) {
  const div = document.createElement("div");
  div.className = "gwin";
  div.style.left = x + "px";
  div.style.top = y + "px";
  div.style.zIndex = ++ztop;
  div.innerHTML = `<div class="gtitle"><span class="gname"></span><span class="gid">#${id}</span></div>
    <canvas width="${w}" height="${h}"></canvas><pre class="gtext"></pre>`;
  div.querySelector(".gname").textContent = title;
  document.body.appendChild(div);
  const canvas = div.querySelector("canvas");
  const gw = { id, div, canvas, ctx: canvas.getContext("2d"),
    text: div.querySelector(".gtext"), buttons: 0 };
  gwins.set(id, gw);

  const focus = () => {
    div.style.zIndex = ++ztop;
    document.querySelectorAll(".gwin.focused").forEach((e) => e.classList.remove("focused"));
    div.classList.add("focused");
    focusedWin = gw;
  };
  div.addEventListener("pointerdown", focus);
  const title_ = div.querySelector(".gtitle");
  title_.addEventListener("pointerdown", (e) => {
    const sx = e.clientX - div.offsetLeft, sy = e.clientY - div.offsetTop;
    const move = (ev) => { div.style.left = ev.clientX - sx + "px"; div.style.top = ev.clientY - sy + "px"; };
    const up = () => { removeEventListener("pointermove", move); removeEventListener("pointerup", up); };
    addEventListener("pointermove", move);
    addEventListener("pointerup", up);
  });
  const mouse = (e, downs) => {
    if (downs !== undefined) gw.buttons = downs;
    const r = canvas.getBoundingClientRect();
    wsys?.mouse(id, Math.round(e.clientX - r.left), Math.round(e.clientY - r.top), gw.buttons);
  };
  const btnbits = (e) => (e.buttons & 1 ? 1 : 0) | (e.buttons & 4 ? 2 : 0) | (e.buttons & 2 ? 4 : 0);
  canvas.addEventListener("pointerdown", (e) => { e.preventDefault(); mouse(e, btnbits(e)); });
  canvas.addEventListener("pointerup", (e) => mouse(e, btnbits(e)));
  canvas.addEventListener("pointermove", (e) => { if (gw.buttons) mouse(e); });
  canvas.addEventListener("contextmenu", (e) => e.preventDefault());
  return gw;
}
let focusedWin = null;
const enc2 = new TextEncoder();
document.addEventListener("keydown", (e) => {
  if (!focusedWin || !wsys) return;
  if (document.activeElement === input) return;   // the console keeps its box
  let bytes = null;
  if (e.key === "Enter") bytes = [10];
  else if (e.key === "Backspace") bytes = [8];
  else if (e.key.length === 1 && !e.metaKey && !e.ctrlKey) bytes = [...enc2.encode(e.key)];
  if (!bytes) return;
  e.preventDefault();
  for (const b of bytes) wsys.key(focusedWin.id, b);
  focusedWin.text.textContent += e.key === "Enter" ? "\n" : e.key.length === 1 ? e.key : "";
});

const host = {
  spawnWorker: (initMsg, transfer) => {
    const w = new Worker(new URL("./worker.mjs", import.meta.url), { type: "module" });
    let handler = () => {};
    let errh = () => {};
    w.addEventListener("message", (e) => handler(e.data));
    w.addEventListener("error", (e) => errh(e));
    w.postMessage(initMsg, transfer);
    return {
      post: (m, t = []) => w.postMessage(m, t),
      setHandler: (cb) => { handler = cb; },
      onMessage: (cb) => { handler = cb; },
      onError: (cb) => { errh = cb; },
      terminate: () => w.terminate(),
    };
  },
  consWrite: (bytes) => append(td.decode(bytes)),
  error: (text) => append(text + "\n", "err"),
  winCreate,
  winPresent: (id, w, h, rgba) => {
    const gw = gwins.get(id);
    if (!gw) return;
    if (gw.canvas.width !== w || gw.canvas.height !== h) { gw.canvas.width = w; gw.canvas.height = h; }
    gw.ctx.putImageData(new ImageData(new Uint8ClampedArray(rgba), w, h), 0, 0);
  },
  winText: (id, bytes) => {
    const gw = gwins.get(id);
    if (gw) { gw.text.textContent += td.decode(bytes); gw.text.scrollTop = gw.text.scrollHeight; }
  },
  winLabel: (id, label) => { gwins.get(id)?.div.querySelector(".gname") &&
    (gwins.get(id).div.querySelector(".gname").textContent = label); },
  winGeom: (id, x, y, w, h) => {
    const gw = gwins.get(id);
    if (!gw) return;
    gw.div.style.left = x + "px"; gw.div.style.top = y + "px";
    gw.canvas.width = w; gw.canvas.height = h;
  },
  winClose: (id) => { gwins.get(id)?.div.remove(); gwins.delete(id); },
  exit: (code) => {
    append(`\n[system halted: exit ${code}]\n`, code === 0 ? "okline" : "err");
    status.textContent = code === 0 ? "halted, clean" : `halted, exit ${code}`;
    input.disabled = true;
  },
};

status.textContent = "fetching rootfs…";
const files = await (await fetch("../build/rootfs.json")).json();
status.textContent = interactive ? "running — interactive" : "running — acceptance tests";
const booted = await boot(host, { rootSeed: seedFromJson(files), interactive });
const { cons } = booted;
wsys = booted.wsys;

const enc = new TextEncoder();
input.addEventListener("keydown", (e) => {
  if (e.key !== "Enter") return;
  const line = input.value;
  input.value = "";
  append(line + "\n", "echo");                 // the cooked tty's own echo
  cons.feed(enc.encode(line + "\n"));
});
