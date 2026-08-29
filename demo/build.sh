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
echo "demo/dist: $(du -sh dist | awk '{print $1}') ($(find dist -type f | wc -l | tr -d ' ') files)"
