// Node harness for the Rust kernel core: the same worker.mjs guests, the
// same host shape as main.mjs, but boot() comes from rustkern.mjs and the
// kernel is browserhost.wasm. '#Z' is served from a temp directory with
// node:fs — the macOS host's op server, in JavaScript.
import { Worker } from "node:worker_threads";
import fs from "node:fs";
import os from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { boot } from "./rustkern.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const rootdir = process.argv[2] ?? join(here, "..", "..", "userspace", "rootfs");
const interactive = process.argv.includes("-i");

function loadSeed(dir, name = "/") {
  const st = fs.statSync(dir);
  if (st.isDirectory())
    return { name, dir: true,
      kids: fs.readdirSync(dir).map((e) => loadSeed(join(dir, e), e)) };
  return { name, dir: false, data: new Uint8Array(fs.readFileSync(dir)) };
}

// '#Z' over node:fs, rooted in a per-run temp dir (the suite's hostfs test)
const zroot = fs.mkdtempSync(join(os.tmpdir(), "ipnx-z-"));
const at = (rel) => {
  const p = rel ? join(zroot, rel) : zroot;
  const probe = fs.existsSync(p) ? p : dirname(p);
  const real = fs.existsSync(probe) ? fs.realpathSync(probe) : null;
  if (!real || !(real === fs.realpathSync(zroot) || real.startsWith(fs.realpathSync(zroot) + "/")))
    throw new Error("path escapes the host root");
  return p;
};
async function hostServe(op) {
  const { kind, path } = op;
  if (kind === 1) {
    const p = at(path);
    try {
      const st = fs.statSync(p);
      return { kind: "meta", dir: st.isDirectory(), len: st.size,
               mtime: Math.floor(st.mtimeMs / 1000), ino: st.ino, mode: st.mode };
    } catch { return { kind: "missing" }; }
  }
  if (kind === 2) {
    const fd = fs.openSync(at(path), "r");
    try {
      const buf = Buffer.alloc(op.n);
      const got = fs.readSync(fd, buf, 0, op.n, op.off);
      return { kind: "bytes", bytes: new Uint8Array(buf.subarray(0, got)) };
    } finally { fs.closeSync(fd); }
  }
  if (kind === 3) {
    const fd = fs.openSync(at(path), "r+");
    try { fs.writeSync(fd, op.data, 0, op.data.length, op.off); }
    finally { fs.closeSync(fd); }
    return { kind: "unit" };
  }
  if (kind === 4) {
    const p = at(path);
    if (op.dir) fs.mkdirSync(p);
    else fs.writeFileSync(p, "");
    fs.chmodSync(p, op.perm);
    return { kind: "unit" };
  }
  if (kind === 5) {
    const p = at(path);
    if (fs.statSync(p).isDirectory()) fs.rmdirSync(p);
    else fs.unlinkSync(p);
    return { kind: "unit" };
  }
  if (kind === 6) {
    const p = at(path);
    if (fs.existsSync(p) && fs.statSync(p).isFile()) fs.truncateSync(p, 0);
    return { kind: "unit" };
  }
  if (kind === 7) {
    const p = at(path);
    const entries = fs.readdirSync(p).sort().map((n) => {
      const st = fs.statSync(join(p, n));
      return { name: n, dir: st.isDirectory(), len: st.size,
               mtime: Math.floor(st.mtimeMs / 1000), ino: st.ino, mode: st.mode };
    });
    return { kind: "entries", entries };
  }
  throw new Error(`hostfs: unknown op ${kind}`);
}

let clicked = false;
let seen = "";
const host = {
  spawnWorker: (initMsg, transfer) => {
    const w = new Worker(join(here, "worker.mjs"));
    let handler = () => {};
    let errh = () => {};
    w.on("message", (m) => handler(m));
    w.on("error", (e) => errh(e));
    w.postMessage(initMsg, transfer);
    return {
      post: (m, t = []) => w.postMessage(m, t),
      setHandler: (cb) => { handler = cb; },
      onMessage: (cb) => { handler = cb; },
      onError: (cb) => { errh = cb; },
      terminate: () => w.terminate(),
    };
  },
  consWrite: (bytes) => {
    process.stdout.write(bytes);
    seen += Buffer.from(bytes).toString();
    if (seen.includes("WINPROOF got exec Install")) {
      console.log("\nWINPROOF PASS: chrome declared in IPNX reached the host, the host's"
        + " click reached the guest — /dev/window round trip, no emca involved");
      process.exit(0);
    }
  },
  winChrome: (wid, ch) => {
    if (ch.type !== "pkg" || clicked) return;   // chrome arrives per declaration
    console.log(`WINPROOF host sees: type=${ch.type} content=${ch.content} toolbar=${JSON.stringify(ch.toolbar)}`);
    for (const line of ch.toolbar.split("\n")) {
      const s = line.trim(); if (!s) continue;
      const sp = s.indexOf(" ");
      const label = sp < 0 ? s : s.slice(0, sp);
      const action = sp < 0 ? "" : s.slice(sp + 1).trim();
      if (!action.startsWith("ipnx:")) continue;          // host: never round-trips
      console.log(`WINPROOF host clicks: ${label} (${action})`);
      clicked = true;
      setTimeout(() => wsys.winEvent(wid, `exec ${label}`), 200);
    }
  },
  error: (text) => console.error(text),
  exit: (code) => process.exit(code),
};

const kernelWasm = fs.readFileSync(join(here, "..", "..", "target",
  "wasm32-unknown-unknown", "release", "browserhost.wasm"));
const { cons, wsys } = await boot(host, {
  rootSeed: loadSeed(rootdir), interactive: true, kernelWasm, hostServe,
});

// THE HOST HALF, PROVED. IPNX declares a window's chrome; it arrives here as
// an ordinary effect (no polling, no renderer); this host reads the toolbar,
// finds the ipnx: control, and reports a click back down the same bridge a
// real surface uses. The guest reading that window's events is plain rc.
const enc = new TextEncoder();
cons.feed(enc.encode("rc /rc/winproof\n"));
setTimeout(() => { console.error("WINPROOF FAIL: no round trip in 20s"); process.exit(1); }, 20000);
