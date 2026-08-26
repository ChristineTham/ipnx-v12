#!/bin/sh
# Build the guest binaries into rootfs/bin. Needs wasi-sdk (used freestanding)
# and, for the ASYNCIFY set, binaryen's wasm-opt.
set -e
cd "$(dirname "$0")"
SDK="${WASI_SDK:-$HOME/.local/opt/wasi-sdk}"
BINARYEN="${BINARYEN:-$HOME/.local/opt/binaryen}"
CC="$SDK/bin/clang --target=wasm32 -nostdlib -O2 -fno-builtin -Ilibc -Wno-incompatible-library-redeclaration -Wno-empty-body"
LD="$SDK/bin/wasm-ld --no-entry --import-memory --stack-first -z stack-size=65536 --initial-memory=2097152 --export=_start"
# The per-binary flag (never system-wide): programs whose fork sites do not
# all exec. Instrumentation is confined to call paths reaching env.forka.
ASYNCIFY="rc forktest"
mkdir -p build rootfs/bin
$CC -c libc/crt0.c -o build/crt0.o
$CC -c libc/crt9.c -o build/crt9.o
$CC -c libc/lib9.c -o build/lib9.o
$CC -c libc/lib9p.c -o build/lib9p.o
$CC -c libc/draw9.c -o build/draw9.o
# libp9.a: the REAL Plan 9 libraries — libc (port, fmt, 9sys), libbio,
# libregexp — compiled from verbatim 4th-edition source. Floats excluded
# for now (fltfmt/strtod need libm); -fms-extensions carries kencc's
# anonymous struct members (Biobufhdr in Biobuf).
P9CC="$SDK/bin/clang --target=wasm32 -nostdlib -O1 -fno-builtin -fms-extensions -Wno-incompatible-pointer-types -Wno-int-conversion -Iplan9/include -Iplan9/sys/include -Wno-unknown-pragmas -Wno-unused-variable -Wno-unused-parameter -Wno-parentheses -Wno-empty-body -Wno-comment -Wno-deprecated-non-prototype -Wno-implicit-int -Wno-return-type -Wno-main-return-type"
P9EXCLUDE="fltfmt strtod charstod pow10 frexp nan64 atof"
: > build/p9lib.list
for c in plan9/sys/src/libc/port/*.c plan9/sys/src/libc/fmt/*.c plan9/sys/src/libc/9sys/*.c plan9/sys/src/libbio/*.c plan9/sys/src/libregexp/*.c plan9/sys/src/libString/*.c; do
  b=$(basename "$c" .c)
  case " $P9EXCLUDE " in *" $b "*) continue;; esac
  $P9CC -c "$c" -o "build/p9-lib-$b.o"
  echo "build/p9-lib-$b.o" >> build/p9lib.list
done
"$SDK/bin/llvm-ar" crs build/libp9.a $(cat build/p9lib.list)
echo "  libp9.a  $(wc -c < build/libp9.a | tr -d ' ') bytes ($(wc -l < build/p9lib.list | tr -d ' ') real objects)"

# libv10 + real V10 sources (poc/v10/NOTICE): K&R C, compiled unmodified
# against the personality's own headers, into /v10/bin — side by side
V10CC="$SDK/bin/clang --target=wasm32 -nostdlib -O1 -std=c89 -fno-builtin -Wno-implicit-function-declaration -Wno-implicit-int -Wno-deprecated-non-prototype"
mkdir -p rootfs/v10/bin
$V10CC -Ilibc -c v10/lib/stdio.c -o build/v10-stdio.o
$V10CC -Iv10/include -c v10/lib/stat.c -o build/v10-stat.o
$V10CC -Ilibc -c v10/lib/crt.c -o build/v10-crt.o
for c in v10/usr/src/cmd/*.c; do
  b=$(basename "$c" .c)
  $V10CC -Iv10/include -c "$c" -o "build/v10-$b.o"
  $LD build/v10-crt.o build/v10-stdio.o build/v10-stat.o build/lib9.o build/lib9p.o "build/v10-$b.o" build/libp9.a -o "rootfs/v10/bin/$b"
  echo "  v10/bin/$b  $(wc -c < "rootfs/v10/bin/$b" | tr -d ' ') bytes (V10 source)"
done

# grep: three C files plus a yacc grammar; the host's bison regenerates the
# parser into build/ (the vendored tree stays verbatim)
bison -y -o build/grep-ytab.c plan9/sys/src/cmd/grep/grep.y
printf 'long yylex(void);\nvoid yyerror(char*, ...);\n' > build/grep-proto.h
$P9CC -Iplan9/sys/src/cmd/grep -include build/grep-proto.h -Wno-implicit-function-declaration -c build/grep-ytab.c -o build/p9-grep-ytab.o
for c in plan9/sys/src/cmd/grep/*.c; do
  b=$(basename "$c" .c)
  $P9CC -Iplan9/sys/src/cmd/grep -c "$c" -o "build/p9-grep-$b.o"
done
$LD build/crt9.o build/lib9.o build/lib9p.o build/draw9.o build/p9-grep-*.o build/libp9.a -o rootfs/bin/grep
echo "  bin/grep  $(wc -c < rootfs/bin/grep | tr -d ' ') bytes (Plan 9 source)"

# real Plan 9 sources (poc/plan9/NOTICE): compiled unmodified, void main,
# through the shim headers — these SUPERSEDE any same-named PoC command
for c in plan9/sys/src/cmd/*.c; do
  b=$(basename "$c" .c)
  $P9CC -c "$c" -o "build/p9-$b.o"
  $LD build/crt9.o build/lib9.o build/lib9p.o build/draw9.o "build/p9-$b.o" build/libp9.a -o "rootfs/bin/$b"
  echo "  bin/$b  $(wc -c < "rootfs/bin/$b" | tr -d ' ') bytes (Plan 9 source)"
done
for c in cmd/*.c; do
  b=$(basename "$c" .c)
  $CC -c "$c" -o "build/$b.o"
  $LD build/crt0.o build/lib9.o build/lib9p.o build/draw9.o "build/$b.o" build/libp9.a -o "rootfs/bin/$b"
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
# pack the rootfs for the browser host (fetched by browser/main.mjs)
node -e '
const fs = require("fs"), p = require("path");
const out = {};
(function walk(d, pre){
  for (const e of fs.readdirSync(d)) {
    const f = p.join(d, e), s = fs.statSync(f);
    if (s.isDirectory()) walk(f, pre + e + "/");
    else out[pre + e] = fs.readFileSync(f).toString("base64");
  }
})("rootfs", "/");
fs.writeFileSync("build/rootfs.json", JSON.stringify(out));
'
echo "  build/rootfs.json  $(wc -c < build/rootfs.json | tr -d " ") bytes"
