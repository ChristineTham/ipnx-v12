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
cp -R hosts/web/www/. "$out/"
# The repository's licence beside the page, and the repository itself at
# the commit checked out: Ghostscript's licence asks that a copy served
# from a site serve its source from the same place (userspace/LICENSE.afpl,
# 2(c)(iv) and the paragraph after (vi)), so a site is deployed from a
# clean checkout. The root carries the rest of the notices at its /
# (userspace/mk.sh).
cp LICENSE "$out/LICENSE"
if git rev-parse --git-dir >/dev/null 2>&1; then
	git archive --format=tar.gz --prefix=ipnx-v12/ -o "$out/source.tar.gz" HEAD
fi
cp target/wasm32-unknown-unknown/release/ipnx_web.wasm "$out/kernel.wasm"
ln -s ../../../userspace/root "$out/root"
node hosts/web/index.mjs userspace/root > "$out/root.index"
# a cache of the page's own scripts per build, so a reload never runs a
# previous build's (coi-sw.js)
node -e 'const fs = require("fs"), f = process.argv[1]; fs.writeFileSync(f, fs.readFileSync(f, "utf8").replace("coi-v3", "coi-" + Date.now()))' "$out/coi-sw.js"
echo "hosts/web/dist: kernel $(wc -c < "$out/kernel.wasm") bytes, $(wc -l < "$out/root.index") files in the root"
