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
# stamp the SW cache name per build: a stale CacheStorage must never serve a
# previous build's modules (measured: it silently did, mid-debug)
STAMP=$(date +%s)
sed -i '' "s/coi-v3/coi-$STAMP/" dist/coi-sw.js
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
const vis = `document.getElementById("status").textContent = "not isolated"; append("This demo needs cross-origin isolation and this page did not get it — a hard reload sometimes helps (the isolation shim needs one reload to take control). Chromium browsers (Chrome, Edge, Brave, Arc) are the measured-reliable path on this host.\\n", "err");`;
if (!t.includes(dev)) { console.error("guard line not found"); process.exit(1); }
fs.writeFileSync(f, t.replace(dev, vis));
'
grep -q "Chromium browser" dist/browser/main.mjs || { echo "guard rewrite failed" >&2; exit 1; }
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
# WebKit advisory — truth instead of a wall: Safari runs here, but this host
# needs the SW in the path and WebKit's SW randomly fails worker-script loads
# (~1% per process spawn, measured 2026-08-29; with real headers Safari is
# 132/132) — say so, in the page, and let it run
node -e '
const fs = require("fs"), f = "dist/browser/main.mjs";
let t = fs.readFileSync(f, "utf8");
const anchor = `status.textContent = "fetching rootfs…";`;
if (!t.includes(anchor)) { console.error("advisory anchor not found"); process.exit(1); }
t = t.replace(anchor, `if (navigator.vendor && navigator.vendor.startsWith("Apple")) append("note: on this host Safari is known-unreliable — WebKit sometimes fails a process spawn through the isolation shim (with real headers Safari runs the full suite; a mirror is planned). If it halts, reload.\\n", "echo");
` + anchor);
fs.writeFileSync(f, t);
'
grep -q "known-unreliable" dist/browser/main.mjs || { echo "advisory failed" >&2; exit 1; }
echo "demo/dist: $(du -sh dist | awk '{print $1}') ($(find dist -type f | wc -l | tr -d ' ') files)"
