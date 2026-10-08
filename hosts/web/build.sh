#!/bin/sh
# Build the browser host into hosts/web/dist: the kernel for wasm32, the
# page's files beside it, the built root, and the root's index.
#
# Run userspace/mk.sh first: the root is userspace/root, linked rather than
# copied, so a rebuilt userspace is what the page serves.
set -e
cd "$(dirname "$0")/../.."
[ -d userspace/root ] || { echo "hosts/web/build.sh: no userspace/root — run 'bash userspace/mk.sh' first" >&2; exit 1; }
cargo build -p ipnx-web --target wasm32-unknown-unknown --release
out=hosts/web/dist
rm -rf "$out"
mkdir -p "$out"
cp hosts/web/www/* "$out/"
cp target/wasm32-unknown-unknown/release/ipnx_web.wasm "$out/kernel.wasm"
ln -s ../../../userspace/root "$out/root"
node hosts/web/index.mjs userspace/root > "$out/root.index"
# a cache of the page's own scripts per build, so a reload never runs a
# previous build's (coi-sw.js)
node -e 'const fs = require("fs"), f = process.argv[1]; fs.writeFileSync(f, fs.readFileSync(f, "utf8").replace("coi-v3", "coi-" + Date.now()))' "$out/coi-sw.js"
echo "hosts/web/dist: kernel $(wc -c < "$out/kernel.wasm") bytes, $(wc -l < "$out/root.index") files in the root"
