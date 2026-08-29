#!/bin/sh
# Populate cache/ — the demo's same-origin package registry (design.md
# 2026-08-29: GitHub asset downloads send no CORS, measured, so the browser
# host installs from this curated mirror; native hosts may reach anywhere).
# Upstream digests are PINNED here and verified at fetch; pkg(1) verifies
# again at install against the digests written into cache/index. The zlib
# sysroot is exploded (pkg's tree kind fetches plain files — no archive code
# in the guest) and carries a meta binding it into cc's sysroot paths.
set -e
cd "$(dirname "$0")"
mkdir -p cache
R="https://github.com/vmware-labs/webassembly-language-runtimes/releases/download"

RUBY_SHA=8b4abc0da1cfca2bc6d04c9b4e89ac6cd52340e020c3bf974fb6139b8195d8ac
PHP_SHA=4fd2e8c42ae529ba72f88a0f1e46de1fc69a4b4f01e01fedd65ca966b8ffe6fa
ZLIB_SHA=1ac699b8a416247bdbe03f0a99f07066177c1764b08b5df53e9cac87d0efe029

get() { # url sha dst
  [ -f "cache/$3" ] || curl -sL "$1" -o "cache/$3"
  got=$(shasum -a 256 "cache/$3" | awk '{print $1}')
  [ "$got" = "$2" ] || { echo "DIGEST MISMATCH: $3" >&2; exit 1; }
}

get "$R/ruby%2F3.2.2%2B20230714-11be424/ruby-3.2.2.wasm" $RUBY_SHA ruby-3.2.2.wasm
get "$R/php%2F8.2.6%2B20230714-11be424/php-cgi-8.2.6-slim.wasm" $PHP_SHA php-8.2.6.wasm
# zlib is COMPILED BY THE SYSTEM'S OWN cc: WLR's prebuilt libz.a is a
# wasi-sdk-20 (LLVM 16) object and the in-tab wasm-ld is LLVM 8 — "Bad
# section type", measured (RESEARCH: layer-2 sysroots must match the
# toolchain's ERA, not just the target). So this step boots the demo kernel
# headless, compiles zlib 1.2.13 from pinned source with the same clang the
# tab ships, and packages the objects; users link them by glob:
#   cc z.c /lib/wasm32-wasi/zlib/*.o
ZSRC_SHA=1525952a0a567581792613a9723333d7f8cc20b87a81f920fb8bc7e3f2251428
[ -f cache/zlibsrc.tar.gz ] || curl -sL https://github.com/madler/zlib/archive/refs/tags/v1.2.13.tar.gz -o cache/zlibsrc.tar.gz
got=$(shasum -a 256 cache/zlibsrc.tar.gz | awk '{print $1}')
[ "$got" = "$ZSRC_SHA" ] || { echo "DIGEST MISMATCH: zlib source" >&2; exit 1; }
rm -rf cache/zbuild cache/zlib-1.2.13 cache/zlib-1.2.13.manifest
mkdir -p cache/zbuild/src cache/zlib-1.2.13/lib/wasm32-wasi/zlib cache/zlib-1.2.13/include
tar xzf cache/zlibsrc.tar.gz -C cache/zbuild/src --strip-components 1
# a throwaway bootable root: the shared rootfs + the tab's own toolchain
rsync -a ../../userspace/rootfs/ cache/zbuild/root/
cp ../toolchain/cache/clang cache/zbuild/root/bin/clang
cp ../toolchain/cache/lld cache/zbuild/root/bin/wasm-ld
mkdir -p cache/zbuild/root/lib
tar xf ../toolchain/cache/sysroot.tar -C cache/zbuild/root/
mkdir -p cache/zbuild/root/zsrc cache/zbuild/root/zout
cp cache/zbuild/src/*.c cache/zbuild/src/*.h cache/zbuild/root/zsrc/
ZFILES="adler32 compress crc32 deflate inflate inffast inftrees trees uncompr zutil"
# ramfs is memory-only, so the guest base64s each object to the console
# (the root carries full CPython) and the host decodes them back out
{ echo 'cd /zsrc'
  for f in $ZFILES; do echo "cc -c -I/zsrc '-DNO_GZIP' $f.c -o /zout/$f.o"; done
  for f in $ZFILES; do
    echo "echo ZOBJ $f"
    echo "python -c 'import base64,sys; sys.stdout.write(base64.b64encode(open(\"/zout/$f.o\",\"rb\").read()).decode()+chr(10))'"
  done
  echo 'echo ZDONE'
} > cache/zbuild/script.rc
(node ../supervisor/main.mjs cache/zbuild/root -i < cache/zbuild/script.rc > cache/zbuild/log 2>&1) &
ZPID=$!
for i in $(seq 1 180); do sleep 5; grep -q "ZDONE" cache/zbuild/log && break; done
kill $ZPID 2>/dev/null || true; wait $ZPID 2>/dev/null || true
grep -q ZDONE cache/zbuild/log || { echo "zlib guest build failed:" >&2; tail -8 cache/zbuild/log >&2; exit 1; }
python3 - cache/zbuild/log cache/zlib-1.2.13/lib/wasm32-wasi/zlib <<'PYX'
import base64, sys, pathlib
log = pathlib.Path(sys.argv[1]).read_text().splitlines()
out = pathlib.Path(sys.argv[2])
cur = None
for ln in log:
    ln = ln.lstrip("% ").rstrip()
    if ln.startswith("ZOBJ "):
        cur = ln.split()[1]; continue
    if cur and ln and all(c.isalnum() or c in "+/=" for c in ln):
        data = base64.b64decode(ln)
        if not data.startswith(b"\x00asm"):
            raise SystemExit(f"{cur}: not a wasm object")
        (out / (cur + ".o")).write_bytes(data)
        cur = None
count = len(list(out.glob("*.o")))
if count != 10:
    raise SystemExit(f"expected 10 objects, got {count}")
print(f"  zlib: {count} era-matched objects extracted")
PYX
cp cache/zbuild/src/zlib.h cache/zbuild/src/zconf.h cache/zlib-1.2.13/include/
cat > cache/zlib-1.2.13/meta <<'EOF'
# zlib 1.2.13, compiled from pinned source by the system's own cc —
# link with: cc yourfile.c /lib/wasm32-wasi/zlib/*.o
bind -a ./lib/wasm32-wasi /lib/wasm32-wasi
bind -a ./include /include
EOF
( cd cache/zlib-1.2.13 && find . -type f | sed 's|^\./||' | sort | while read -r f; do
    printf '%s zlib-1.2.13/%s %s\n' "$f" "$f" "$(shasum -a 256 "$f" | awk '{print $1}')"
  done ) > cache/zlib-1.2.13.manifest

# the index: name version kind url sha256 (urls relative to the base)
{
  printf 'ruby 3.2.2 bin ruby-3.2.2.wasm %s\n' "$RUBY_SHA"
  printf 'php 8.2.6 bin php-8.2.6.wasm %s\n' "$PHP_SHA"
  printf 'zlib 1.2.13 tree zlib-1.2.13.manifest %s\n' \
    "$(shasum -a 256 cache/zlib-1.2.13.manifest | awk '{print $1}')"
} > cache/index
rm -rf cache/zbuild                      # the throwaway build root
echo "registry cache: $(du -sh cache | awk '{print $1}')"
cat cache/index
