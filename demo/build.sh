#!/bin/sh
# Assemble the static demo into demo/dist: the frozen browser port, the
# frozen supervisor it imports, the packed rootfs, and the landing pages.
# Run userspace/mk.sh first (dist wants a fresh build/rootfs.json).
set -e
cd "$(dirname "$0")"
rm -rf dist
mkdir -p dist/build
cp -R ../poc/browser dist/browser
cp -R ../poc/supervisor dist/supervisor
cp ../userspace/build/rootfs.json dist/build/rootfs.json
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
touch dist/.nojekyll
# inject the isolation shim into the dist COPY of the frozen browser page —
# the derivation-layer move: the source is never edited, the bundle is derived
sed -i '' 's|<meta charset="utf-8">|<meta charset="utf-8">\
<script src="../coi-register.js"></script>|' dist/browser/index.html
grep -q coi-register dist/browser/index.html || { echo "coi injection failed" >&2; exit 1; }
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
# ---- the wasi_unstable adapter + unique inodes, derived into the dist
# supervisor copies (the frozen originals never change): lets 2019-era LLVM
# binaries (clang/lld) run under the preview1 shim ----
node -e '
const fs = require("fs");
let g = fs.readFileSync("dist/supervisor/guestcore.mjs", "utf8");
const a1 = `if (WebAssembly.Module.imports(mod).some((i) => i.module === "wasi_snapshot_preview1")) {`;
if (!g.includes(a1)) { console.error("guestcore wasi anchor missing"); process.exit(1); }
g = g.replace(a1, `if (WebAssembly.Module.imports(mod).some((i) => i.module === "wasi_snapshot_preview1" || i.module === "wasi_unstable")) {`);
const a2 = `    const inst = new WebAssembly.Instance(mod, { wasi_snapshot_preview1: wasi.imports });`;
if (!g.includes(a2)) { console.error("guestcore instantiate anchor missing"); process.exit(1); }
g = g.replace(a2, `    const unstable = adaptUnstable(wasi.imports);
    const inst = new WebAssembly.Instance(mod, { wasi_snapshot_preview1: wasi.imports, wasi_unstable: unstable });
    unstable.__getmem = () => inst.exports.memory.buffer;`);
g += `
function adaptUnstable(p1) {
  const wrap = { ...p1 };
  const WMAP = [1, 2, 0];
  wrap.fd_seek = (fd, off, whence, out) => p1.fd_seek(fd, off, WMAP[whence] ?? whence, out);
  const repack = (buf) => {
    const m = new DataView(wrap.__getmem());
    const dev = m.getBigUint64(buf, true), ino = m.getBigUint64(buf + 8, true);
    const ft = m.getUint8(buf + 16);
    const nlink = m.getBigUint64(buf + 24, true), size = m.getBigUint64(buf + 32, true);
    const at = m.getBigUint64(buf + 40, true), mt = m.getBigUint64(buf + 48, true), ct = m.getBigUint64(buf + 56, true);
    m.setBigUint64(buf, dev, true); m.setBigUint64(buf + 8, ino, true);
    m.setUint8(buf + 16, ft);
    m.setUint32(buf + 20, Number(nlink & 0xffffffffn), true);
    m.setBigUint64(buf + 24, size, true);
    m.setBigUint64(buf + 32, at, true); m.setBigUint64(buf + 40, mt, true); m.setBigUint64(buf + 48, ct, true);
  };
  wrap.fd_filestat_get = (fd, buf) => { const r = p1.fd_filestat_get(fd, buf); if (r === 0) repack(buf); return r; };
  wrap.path_filestat_get = (fd, fl, ptr, len, buf) => { const r = p1.path_filestat_get(fd, fl, ptr, len, buf); if (r === 0) repack(buf); return r; };
  return wrap;
}
`;
fs.writeFileSync("dist/supervisor/guestcore.mjs", g);
let w = fs.readFileSync("dist/supervisor/wasi1.mjs", "utf8");
const a3 = `  function putFilestat(ptr, st, path) {
    const d = dv();
    d.setBigUint64(ptr, 0n, true);
    d.setBigUint64(ptr + 8, 0n, true);`;
if (!w.includes(a3)) { console.error("wasi1 filestat anchor missing"); process.exit(1); }
w = w.replace(a3, `  function pathIno(path) {
    let h = 1469598103934665603n;
    for (let i = 0; i < path.length; i++) { h ^= BigInt(path.charCodeAt(i)); h = (h * 1099511628211n) & 0xffffffffffffffffn; }
    return h;
  }
  function putFilestat(ptr, st, path) {
    const d = dv();
    d.setBigUint64(ptr, 0n, true);
    d.setBigUint64(ptr + 8, st.qpath !== undefined ? BigInt(st.qpath) : pathIno(path || ""), true);`);
fs.writeFileSync("dist/supervisor/wasi1.mjs", w);
'
grep -q "adaptUnstable" dist/supervisor/guestcore.mjs && grep -q "pathIno" dist/supervisor/wasi1.mjs || { echo "toolchain shim derivation failed" >&2; exit 1; }
# the demo instance names its person kitty (the frozen oracle keeps glenda;
# the suite is name-agnostic — it reads /dev/user rather than asserting one)
sed -i '' 's/"glenda"/"kitty"/g' dist/supervisor/kernel.mjs dist/supervisor/devs.mjs dist/supervisor/mnt9p.mjs dist/supervisor/stat9.mjs
grep -q '"kitty"' dist/supervisor/kernel.mjs || { echo "kitty derivation failed" >&2; exit 1; }

# ---- the opt-in C-toolchain manifest: base rootfs + clang/lld/sysroot ----
if [ -s toolchain/cache/clang ]; then
  node -e '
const fs = require("fs"), path = require("path"), cp = require("child_process");
const base = {};                                   // overlay only: merged client-side
const add = (p, buf) => { base[p] = buf.toString("base64"); };
add("/bin/cc", fs.readFileSync("toolchain/cache/clang"));
add("/bin/wasm-ld", fs.readFileSync("toolchain/cache/lld"));
add("/rc/cc", fs.readFileSync("toolchain/cc.rc"));
add("/tmp/hello.c", fs.readFileSync("toolchain/hello.c"));
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
fs.writeFileSync("dist/build/cc-overlay.json", JSON.stringify(base));
console.log("  cc-overlay.json", fs.statSync("dist/build/cc-overlay.json").size, "bytes (GitHub caps files at 100MB — the overlay merges onto the base in the shell)");
'
fi
echo "demo/dist: $(du -sh dist | awk '{print $1}') ($(find dist -type f | wc -l | tr -d ' ') files)"
