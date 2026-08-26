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
const host = {
  spawnWorker: (initMsg, transfer) => {
    const w = new Worker(new URL("./worker.mjs", import.meta.url), { type: "module" });
    w.postMessage(initMsg, transfer);
    return {
      post: (m, t = []) => w.postMessage(m, t),
      onMessage: (cb) => w.addEventListener("message", (e) => cb(e.data)),
      onError: (cb) => w.addEventListener("error", (e) => cb(e)),
      terminate: () => w.terminate(),
    };
  },
  consWrite: (bytes) => append(td.decode(bytes)),
  error: (text) => append(text + "\n", "err"),
  exit: (code) => {
    append(`\n[system halted: exit ${code}]\n`, code === 0 ? "okline" : "err");
    status.textContent = code === 0 ? "halted, clean" : `halted, exit ${code}`;
    input.disabled = true;
  },
};

status.textContent = "fetching rootfs…";
const files = await (await fetch("../build/rootfs.json")).json();
status.textContent = interactive ? "running — interactive" : "running — acceptance tests";
const { cons } = await boot(host, { rootSeed: seedFromJson(files), interactive });

const enc = new TextEncoder();
input.addEventListener("keydown", (e) => {
  if (e.key !== "Enter") return;
  const line = input.value;
  input.value = "";
  append(line + "\n", "echo");                 // the cooked tty's own echo
  cons.feed(enc.encode(line + "\n"));
});
