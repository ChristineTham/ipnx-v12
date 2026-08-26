#!/bin/sh
# Build the guest binaries into rootfs/bin. Needs wasi-sdk (used freestanding)
# and, for the ASYNCIFY set, binaryen's wasm-opt.
set -e
cd "$(dirname "$0")"
SDK="${WASI_SDK:-$HOME/.local/opt/wasi-sdk}"
BINARYEN="${BINARYEN:-$HOME/.local/opt/binaryen}"
CC="$SDK/bin/clang --target=wasm32 -nostdlib -O2 -Ilibc -Wno-incompatible-library-redeclaration -Wno-empty-body"
LD="$SDK/bin/wasm-ld --no-entry --import-memory --stack-first -z stack-size=65536 --initial-memory=4194304 --export=_start"
# The per-binary flag (never system-wide): programs whose fork sites do not
# all exec. Instrumentation is confined to call paths reaching env.forka.
ASYNCIFY="rc forktest"
mkdir -p build rootfs/bin
$CC -c libc/crt0.c -o build/crt0.o
$CC -c libc/lib9.c -o build/lib9.o
for c in cmd/*.c; do
  b=$(basename "$c" .c)
  $CC -c "$c" -o "build/$b.o"
  $LD build/crt0.o build/lib9.o "build/$b.o" -o "rootfs/bin/$b"
  case " $ASYNCIFY " in *" $b "*)
    "$BINARYEN/bin/wasm-opt" "rootfs/bin/$b" \
      --asyncify --pass-arg=asyncify-imports@env.forka -O2 \
      --enable-mutable-globals --enable-sign-ext --enable-bulk-memory \
      --enable-nontrapping-float-to-int \
      -o "rootfs/bin/$b.tmp" && mv "rootfs/bin/$b.tmp" "rootfs/bin/$b"
    echo "  bin/$b  $(wc -c < "rootfs/bin/$b" | tr -d ' ') bytes (asyncified)" ;;
  *)
    echo "  bin/$b  $(wc -c < "rootfs/bin/$b" | tr -d ' ') bytes" ;;
  esac
done
