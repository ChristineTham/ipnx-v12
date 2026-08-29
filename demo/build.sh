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
const vis = `document.getElementById("status").textContent = "browser not supported here"; append("This demo needs cross-origin isolation. Safari cannot get it on this host (WebKit takes the COOP/COEP headers only from the real response, which GitHub Pages cannot send) — please use a Chromium browser: Chrome, Edge, Brave, or Arc. Firefox is untested. Safari support needs a header-capable host, and is on the list.\\n", "err");`;
if (!t.includes(dev)) { console.error("guard line not found"); process.exit(1); }
fs.writeFileSync(f, t.replace(dev, vis));
'
grep -q "Chromium browser" dist/browser/main.mjs || { echo "guard rewrite failed" >&2; exit 1; }
echo "demo/dist: $(du -sh dist | awk '{print $1}') ($(find dist -type f | wc -l | tr -d ' ') files)"
