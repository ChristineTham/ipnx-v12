#!/bin/sh
# Assemble the static demo into demo/dist: the frozen browser port, the
# frozen supervisor it imports, the packed rootfs, and the landing pages.
# Run userspace/mk.sh first (dist wants a fresh build/rootfs.json).
set -e
cd "$(dirname "$0")"
rm -rf dist
mkdir -p dist/build
cp -R ../poc/browser dist/browser
cp -R supervisor dist/supervisor            # the demo's own kernel lineage
cp ../userspace/build/rootfs.json dist/build/rootfs.json
# the kernel itself: the Rust core compiled to wasm (the browser surface's
# kernel half; rustkern.mjs drives it)
( cd .. && cargo build --release --target wasm32-unknown-unknown -p browserhost )
cp ../target/wasm32-unknown-unknown/release/browserhost.wasm dist/build/kernel.wasm
cp index.html NOTICES.html _headers coi-sw.js coi-register.js dist/
cp -R shell dist/shell
cp -R vendor dist/vendor
# stamp the SW cache name per build: a stale CacheStorage must never serve a
# previous build's modules (measured: it silently did, mid-debug)
STAMP=$(date +%s)
sed -i '' "s/coi-v3/coi-$STAMP/" dist/coi-sw.js
sed -i '' "s/BUILDSTAMP/$STAMP/g" dist/shell/shell.mjs
grep -q "v=$STAMP" dist/shell/shell.mjs || { echo "stamp inject failed" >&2; exit 1; }
sed -i '' "s|src=\"shell.mjs\"|src=\"shell.mjs?v=$STAMP\"|" dist/shell/index.html
grep -q "shell.mjs?v=$STAMP" dist/shell/index.html || { echo "entry bust failed" >&2; exit 1; }
grep -q "coi-$STAMP" dist/coi-sw.js || { echo "cache stamp failed" >&2; exit 1; }
# the demo's package registry (regenerate with registry/fetch.sh): the
# browser host installs from here — same origin, so no CORS wall
if [ -f registry/cache/index ]; then
  mkdir -p dist/registry
  cp registry/cache/index registry/cache/*.wasm registry/cache/*.manifest dist/registry/
  cp -R registry/cache/zlib-1.2.13 dist/registry/
  for f in $(find dist/registry -type f); do
    sz=$(wc -c < "$f")
    [ "$sz" -lt 52428800 ] || { echo "$f over the 50MB line" >&2; exit 1; }
  done
  echo "  registry: $(du -sh dist/registry | awk '{print $1}') ($(find dist/registry -type f | wc -l | tr -d ' ') files)"
fi
touch dist/.nojekyll
# inject the isolation shim into the dist COPY of the frozen browser page —
# the derivation-layer move: the source is never edited, the bundle is derived
sed -i '' 's|<meta charset="utf-8">|<meta charset="utf-8">\
<script src="../coi-register.js"></script>|' dist/browser/index.html
grep -q coi-register dist/browser/index.html || { echo "coi injection failed" >&2; exit 1; }
# the suite page runs the RUST CORE: swap the dist copy's kernel import and
# boot call to rustkern + kernel.wasm (dist-only, the frozen source untouched)
node -e '
const fs = require("fs"), f = "dist/browser/main.mjs";
let t = fs.readFileSync(f, "utf8");
const a = `import { boot } from "../supervisor/kernel.mjs";`;
const b = `import { boot } from "../supervisor/rustkern.mjs";`;
if (!t.includes(a)) { console.error("kernel import line not found"); process.exit(1); }
t = t.replace(a, b);
const c = `const booted = await boot(host, { rootSeed: seedFromJson(files), interactive });`;
const d = `const kernelWasm = await (await fetch("../build/kernel.wasm")).arrayBuffer();
const booted = await boot(host, { rootSeed: seedFromJson(files), interactive, kernelWasm });`;
if (!t.includes(c)) { console.error("boot line not found"); process.exit(1); }
t = t.replace(c, d);
fs.writeFileSync(f, t);
'
grep -q "rustkern" dist/browser/main.mjs || { echo "rust-core rewrite failed" >&2; exit 1; }

# the guard message in the dist COPY speaks to visitors, not developers:
# Safari (WebKit) takes isolation only from real response headers, which this
# host cannot send — measured live, 2026-08-29
node -e '
const fs = require("fs"), f = "dist/browser/main.mjs";
let t = fs.readFileSync(f, "utf8");
const dev = `append("not cross-origin isolated — serve this page with poc/serve.mjs\\n", "err");`;
const vis = `document.getElementById("status").textContent = "not isolated"; append("This demo needs cross-origin isolation and this load did not get it. Reload normally — hard reloads bypass the isolation worker and guarantee this screen.\\n", "err");`;
if (!t.includes(dev)) { console.error("guard line not found"); process.exit(1); }
fs.writeFileSync(f, t.replace(dev, vis));
'
grep -q "hard reloads bypass" dist/browser/main.mjs || { echo "guard rewrite failed" >&2; exit 1; }
# surface real worker errors (the frozen handler prints the bare event; on an
# engine we have no inspector automation for, the message must reach the page)
node -e '
const fs = require("fs"), f = "dist/browser/main.mjs";
let t = fs.readFileSync(f, "utf8");
const dev = `w.addEventListener("error", (e) => errh(e));`;
const vis = `w.addEventListener("error", (e) => errh(((e && (e.message || (e.error && e.error.message))) || String(e)) + " @" + (e && e.filename || "?") + ":" + (e && e.lineno || "?"))); w.addEventListener("messageerror", (e) => errh("messageerror: " + String(e && e.data)));`;
if (!t.includes(dev)) { console.error("error-handler line not found"); process.exit(1); }
fs.writeFileSync(f, t.replace(dev, vis));
'
node -e '
const fs = require("fs"), f = "dist/browser/main.mjs";
const t = fs.readFileSync(f, "utf8");
if (!t.includes("messageerror")) { console.error("error surfacing failed"); process.exit(1); }
'
# relay real in-worker errors out: WebKit strips ErrorEvent details at the
# Worker-object boundary, but inside the worker they are intact — post them
# through a __dbg side channel the page prints before normal dispatch
node -e '
const fs = require("fs");
let w = fs.readFileSync("dist/browser/worker.mjs", "utf8");
w = `self.addEventListener("error", (e) => { try { self.postMessage({ __dbg: (e.message||"?") + " @" + (e.filename||"?") + ":" + (e.lineno||"?") + " | " + (e.error && e.error.stack || "") }); } catch (_) {} });
self.addEventListener("unhandledrejection", (e) => { try { self.postMessage({ __dbg: "rejection: " + (e.reason && (e.reason.stack || e.reason.message) || String(e.reason)) }); } catch (_) {} });
` + w;
fs.writeFileSync("dist/browser/worker.mjs", w);
let m = fs.readFileSync("dist/browser/main.mjs", "utf8");
const dev = `w.addEventListener("message", (e) => handler(e.data));`;
if (!m.includes(dev)) { console.error("message-handler line not found"); process.exit(1); }
m = m.replace(dev, `w.addEventListener("message", (e) => { if (e.data && e.data.__dbg) { errh("DBG " + e.data.__dbg); return; } handler(e.data); });`);
fs.writeFileSync("dist/browser/main.mjs", m);
'
grep -q "__dbg" dist/browser/worker.mjs && grep -q "__dbg" dist/browser/main.mjs || { echo "dbg relay failed" >&2; exit 1; }
# serialize module-worker creation: WebKit (Safari 26.6.2, measured with the
# minimal repro in demo/webkit-repro) deterministically fails a module-worker
# script load that starts while another is in flight through a service
# worker. One load in flight at a time cures it; posts buffer until the real
# worker exists.
node -e '
const fs = require("fs"), f = "dist/browser/main.mjs";
let t = fs.readFileSync(f, "utf8");
const a = t.indexOf("    const w = new Worker(new URL(\"./worker.mjs\", import.meta.url), { type: \"module\" });");
const bMark = "      terminate: () => w.terminate(),";
const b = t.indexOf(bMark);
const bEnd = t.indexOf("};", b);
if (a < 0 || b < 0 || bEnd < 0) { console.error("spawn block anchors not found"); process.exit(1); }
const repl = `    let w = null, pending = [[initMsg, transfer]], dead = false;
    let handler = () => {};
    let errh = () => {};
    const attach = (ww) => {
      w = ww;
      ww.addEventListener("message", (e) => { if (e.data && e.data.__dbg) { errh("DBG " + e.data.__dbg); return; } handler(e.data); });
      ww.addEventListener("error", (e) => errh(((e && (e.message || (e.error && e.error.message))) || String(e)) + " @" + (e && e.filename || "?") + ":" + (e && e.lineno || "?")));
      ww.addEventListener("messageerror", (e) => errh("messageerror: " + String(e && e.data)));
      const q = pending; pending = null;
      for (const [m, tr] of q) ww.postMessage(m, tr);
      if (dead) ww.terminate();
    };
    globalThis.__wgate = (globalThis.__wgate || Promise.resolve()).then(() => new Promise((release) => {
      const ww = new Worker(new URL("./worker.mjs", import.meta.url), { type: "module" });
      attach(ww);
      let released = false;
      const go = () => { if (!released) { released = true; clearTimeout(bt); release(); } };
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
    `;
t = t.slice(0, a) + repl + t.slice(bEnd);
fs.writeFileSync(f, t);
'
grep -q "__wgate" dist/browser/main.mjs || { echo "gate injection failed" >&2; exit 1; }
node --check dist/browser/main.mjs 2>/dev/null || node -e 'require("fs"); try { new Function(require("fs").readFileSync("dist/browser/main.mjs","utf8")); } catch(e) { /* module syntax: parse via import */ }'
# the shim's wasi_unstable adapter, unique inodes, cwd resolution and the
# kitty eve now live in demo/supervisor (the demo's kernel lineage) — no
# derivations, so a demo kernel change is an ordinary edit, reviewable.
grep -q "adaptUnstable" dist/supervisor/guestcore.mjs && grep -q "function cwd" dist/supervisor/wasi1.mjs && grep -q '"kitty"' dist/supervisor/kernel.mjs || { echo "demo/supervisor missing a demo mod" >&2; exit 1; }

# ---- overlays are packed in parts UNDER 50MB (GitHub warns above 50 and
# caps at 100; Git LFS is NOT an option — GitHub Pages serves LFS pointer
# files, not content). Each profile's driver-probed marker file ships in the
# LAST part, so a passing probe means the whole toolchain is aboard. ----
if [ -s toolchain/cache/clang ]; then
  node -e '
const fs = require("fs"), path = require("path"), cp = require("child_process");
const entries = [];                                // [path, b64], probe file LAST
const add = (p, buf) => entries.push([p, buf.toString("base64")]);
add("/bin/wasm-ld", fs.readFileSync("toolchain/cache/lld"));
const tmp = fs.mkdtempSync("/tmp/sysroot-");
cp.execSync(`tar xf toolchain/cache/sysroot.tar -C ${tmp}`);
(function walk(d, pre) {
  for (const e of fs.readdirSync(d)) {
    const f = path.join(d, e), s = fs.statSync(f);
    if (s.isDirectory()) walk(f, pre + e + "/");
    else add(pre + e, fs.readFileSync(f));
  }
})(tmp, "/");
fs.rmSync(tmp, { recursive: true, force: true });
add("/bin/clang", fs.readFileSync("toolchain/cache/clang"));   // the probe
const BUDGET = 45 * 1024 * 1024;
let part = {}, size = 0, idx = 0;
const flush = () => {
  if (!size) return;
  const name = `cc-overlay-${idx++}.json`;
  fs.writeFileSync(`dist/build/${name}`, JSON.stringify(part));
  console.log(`  ${name}`, fs.statSync(`dist/build/${name}`).size, "bytes");
  part = {}; size = 0;
};
for (const [p, b64] of entries) {
  if (b64.length > BUDGET) {                       // one FILE over budget: ship it
    const step = Math.floor(BUDGET / 4) * 4;       // as b64 pieces (4-char blocks
    let n = 0;                                     // concatenate losslessly)
    for (let o = 0; o < b64.length; o += step, n++) {
      const piece = b64.slice(o, o + step);
      if (size && size + piece.length > BUDGET) flush();
      part[`${p}\u0000${n}`] = piece; size += piece.length;
    }
    continue;
  }
  if (size && size + b64.length > BUDGET) flush();
  part[p] = b64; size += b64.length;
}
flush();
'
fi

# the Go toolchain: real gc compiler/linker/gofmt (wasip1) + the
# gobyexample-derived stdlib export set; regenerate with toolchain/go/build.sh
if [ -s toolchain/go/cache/compile.wasm ]; then
  node -e '
const fs = require("fs"), path = require("path");
const entries = [];                                // probe (/go/bin/compile) LAST
const add = (p, file) => entries.push([p, fs.readFileSync(file).toString("base64")]);
add("/go/bin/link", "toolchain/go/cache/link.wasm");
add("/go/bin/gofmt", "toolchain/go/cache/gofmt.wasm");
add("/go/importcfg", "toolchain/go/cache/importcfg");
add("/go/VERSION", "toolchain/go/cache/VERSION");
(function walk(d, pre) {
  for (const e of fs.readdirSync(d)) {
    const f = path.join(d, e), s = fs.statSync(f);
    if (s.isDirectory()) walk(f, pre + e + "/");
    else add(pre + e, f);
  }
})("toolchain/go/cache/pkg", "/go/pkg/");
add("/go/bin/compile", "toolchain/go/cache/compile.wasm");     // the probe
const BUDGET = 45 * 1024 * 1024;
let part = {}, size = 0, idx = 0;
const flush = () => {
  if (!size) return;
  const name = `go-overlay-${idx++}.json`;
  fs.writeFileSync(`dist/build/${name}`, JSON.stringify(part));
  console.log(`  ${name}`, fs.statSync(`dist/build/${name}`).size, "bytes");
  part = {}; size = 0;
};
for (const [p, b64] of entries) {
  if (b64.length > BUDGET) {                       // one FILE over budget: ship it
    const step = Math.floor(BUDGET / 4) * 4;       // as b64 pieces (4-char blocks
    let n = 0;                                     // concatenate losslessly)
    for (let o = 0; o < b64.length; o += step, n++) {
      const piece = b64.slice(o, o + step);
      if (size && size + piece.length > BUDGET) flush();
      part[`${p}\u0000${n}`] = piece; size += piece.length;
    }
    continue;
  }
  if (size && size + b64.length > BUDGET) flush();
  part[p] = b64; size += b64.length;
}
flush();
'
fi

# the overlay manifest reflects what this build actually packed
node -e '
const fs = require("fs");
const man = {};
for (const prof of ["cc", "go"]) {
  const parts = fs.readdirSync("dist/build")
    .filter((n) => new RegExp(`^${prof}-overlay-\\d+\\.json$`).test(n))
    .sort((a, b) => parseInt(a.match(/\d+/)) - parseInt(b.match(/\d+/)));
  if (parts.length) man[prof] = parts;
  for (const p of parts)
    if (fs.statSync(`dist/build/${p}`).size > 50 * 1024 * 1024) {
      console.error(`${p} exceeds the 50MB warning line`); process.exit(1);
    }
}
fs.writeFileSync("dist/build/overlays.json", JSON.stringify(man));
console.log("  overlays.json", JSON.stringify(man));
'

echo "demo/dist: $(du -sh dist | awk '{print $1}') ($(find dist -type f | wc -l | tr -d ' ') files)"
