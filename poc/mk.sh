#!/bin/sh
# Build the guest binaries into rootfs/bin. Needs wasi-sdk (used freestanding).
set -e
cd "$(dirname "$0")"
SDK="${WASI_SDK:-$HOME/.local/opt/wasi-sdk}"
CC="$SDK/bin/clang --target=wasm32 -nostdlib -O2 -Ilibc -Wno-incompatible-library-redeclaration -Wno-empty-body"
LD="$SDK/bin/wasm-ld --no-entry --import-memory --stack-first -z stack-size=65536 --initial-memory=4194304 --export=_start"
mkdir -p build rootfs/bin
$CC -c libc/crt0.c -o build/crt0.o
$CC -c libc/lib9.c -o build/lib9.o
for c in cmd/*.c; do
  b=$(basename "$c" .c)
  $CC -c "$c" -o "build/$b.o"
  $LD build/crt0.o build/lib9.o "build/$b.o" -o "rootfs/bin/$b"
  echo "  bin/$b  $(wc -c < "rootfs/bin/$b" | tr -d ' ') bytes"
done
