#!/bin/sh
# Build the guest binaries into rootfs/bin. Needs wasi-sdk (used freestanding)
# and, for the ASYNCIFY set, binaryen's wasm-opt.
set -e
cd "$(dirname "$0")"
SDK="${WASI_SDK:-$HOME/.local/opt/wasi-sdk}"
BINARYEN="${BINARYEN:-$HOME/.local/opt/binaryen}"
CC="$SDK/bin/clang --target=wasm32 -nostdlib -O2 -fno-builtin -Ilibc -Wno-incompatible-library-redeclaration -Wno-empty-body"
LD="$SDK/bin/wasm-ld --no-entry --import-memory --stack-first -z stack-size=262144 --initial-memory=2097152 --export=_start --export-if-defined=__stack_pointer --table-base=4096"
# The per-binary flag (never system-wide): programs whose fork sites do not
# all exec. Instrumentation is confined to call paths reaching env.forka.
ASYNCIFY="forktest forkind forkvm"   # bare forkers; the real rc rule asyncifies itself
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
P9EXCLUDE="fltfmt strtod charstod pow10 frexp nan64 atof execl"   # execl: &f+1 assumes stack varargs; lib9 has a va_arg one
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

# rc: the REAL shell — mkfile's OFILES (code..var, havefork, plan9, y.tab)
# minus unix.c; the host's bison regenerates the parser, and x.tab.h is Plan
# 9 yacc's header name. rc's subshells bare-fork, so the binary is asyncified.
bison -y -d -o build/rc-ytab.c plan9/sys/src/cmd/rc/syn.y
cp build/rc-ytab.h build/x.tab.h
for c in plan9/sys/src/cmd/rc/*.c; do
  b=$(basename "$c" .c)
  case "$b" in unix) continue;; esac
  $P9CC -Ibuild -Iplan9/sys/src/cmd/rc -c "$c" -o "build/p9-rc-$b.o"
done
$P9CC -Ibuild -Iplan9/sys/src/cmd/rc -Wno-implicit-function-declaration -c build/rc-ytab.c -o build/p9-rc-ytab.o
# rc.h carries pre-ANSI tentative definitions in every TU, and the wasm
# backend refuses -fcommon. Restore real common-symbol semantics by hand:
# weaken every zero-bss global, so an initialized definition (havefork=1,
# Rcmain, doprompt...) beats the tentative copies REGARDLESS of link order —
# ordering cannot work, the initialized TUs tentatively define each other's
# symbols (measured: plan9.o's bss havefork beat havefork.o's =1, so code.c
# emitted the forkless pipe layout while Xpipe executed the fork one).
cp build/lib9.o build/lib9-rc.o
# Each duplicated global keeps ONE strong copy — the TU whose source
# initializes it (the measured set below) or the first TU otherwise —
# and is weakened everywhere else, so the linker resolves initialized-
# over-tentative REGARDLESS of order. (Ordering cannot work: plan9.c and
# havefork.c tentatively define each other's initialized symbols.)
rc_owner() {
  case "$1" in
    havefork) echo build/p9-rc-havefork.o;;
    Rcmain|Fdprefix|Signame|syssigname|Builtin|interrupted) echo build/p9-rc-plan9.o;;
    future|doprompt) echo build/p9-rc-lex.o;;
    argv0) echo build/p9-rc-exec.o;;
    flagset) echo build/p9-rc-getflags.o;;
    nullpath) echo build/p9-rc-simple.o;;
    pfmtnest) echo build/p9-rc-io.o;;
    *) echo "";;
  esac
}
RCOBJS="build/lib9-rc.o $(echo build/p9-rc-*.o)"
for o in $RCOBJS; do
  "$SDK/bin/llvm-nm" --defined-only --extern-only "$o" | awk -v o="$o" '{print o, $3}'
done > build/rc-defs.txt
awk '{n[$2]++} END{for(s in n) if(n[s]>1) print s}' build/rc-defs.txt > build/rc-dups.txt
for o in $RCOBJS; do
  weak=""
  while read -r sym; do
    grep -q "^$o $sym\$" build/rc-defs.txt || continue
    own=$(rc_owner "$sym")
    [ -z "$own" ] && own=$(awk -v s="$sym" '$2==s{print $1; exit}' build/rc-defs.txt)
    [ "$o" = "$own" ] && continue
    weak="$weak $sym"
  done < build/rc-dups.txt
  [ -n "$weak" ] && node weaken.mjs "$o" $weak
done
$LD build/crt9.o build/lib9-rc.o build/lib9p.o build/draw9.o \
  build/p9-rc-*.o build/libp9.a -o rootfs/bin/rc
"$BINARYEN/bin/wasm-opt" rootfs/bin/rc \
  --asyncify --pass-arg=asyncify-imports@env.forka,env.setj,env.longj -O2 \
  --enable-mutable-globals --enable-sign-ext --enable-bulk-memory \
  --enable-nontrapping-float-to-int \
  -o rootfs/bin/rc.tmp && mv rootfs/bin/rc.tmp rootfs/bin/rc
echo "  bin/rc  $(wc -c < rootfs/bin/rc | tr -d ' ') bytes (REAL rc, asyncified)"

# the real libdraw and libframe, as archives; draw clients link them
: > build/p9draw.list
for c in plan9/sys/src/libdraw/*.c plan9/sys/src/libframe/*.c; do
  b=$(basename "$c" .c)
  $P9CC -c "$c" -o "build/p9-draw-$b.o"
  echo "build/p9-draw-$b.o" >> build/p9draw.list
done
"$SDK/bin/llvm-ar" crs build/libdraw.a $(cat build/p9draw.list)
echo "  libdraw.a  $(wc -c < build/libdraw.a | tr -d ' ') bytes (libdraw + libframe)"

# drtest: OUR test, but against the REAL draw.h and libdraw
$P9CC -c cmd/drtest.c -o build/drtest.o
$LD build/crt9.o build/lib9.o build/lib9p.o build/draw9.o build/drtest.o build/libdraw.a build/libp9.a -o rootfs/bin/drtest
echo "  bin/drtest  $(wc -c < rootfs/bin/drtest | tr -d ' ') bytes (REAL libdraw client)"

# sam: the real editor's backend — `sam -d` is ed-with-structural-regexps
# on the console; the samterm protocol (mesg.c) compiles along, quiet until
# a terminal exists. L"..." literals are 16-bit Runes, hence -fshort-wchar;
# `!` shell escapes fork bare, hence asyncify.
for c in plan9/sys/src/cmd/sam/*.c; do
  b=$(basename "$c" .c)
  $P9CC -fshort-wchar -Iplan9/sys/src/cmd/sam -c "$c" -o "build/p9-sam-$b.o"
done
# parse.h tentatively defines cmdtab[] in every TU; cmd.c owns the real one
for o in build/p9-sam-*.o; do
  case "$o" in */p9-sam-cmd.o) continue;; esac
  "$SDK/bin/llvm-nm" --defined-only --extern-only "$o" 2>/dev/null | grep -q " cmdtab\$" && node weaken.mjs "$o" cmdtab
done
$P9CC -c plan9/sys/src/libplumb/mesg.c -o build/p9-plumb-mesg.o
$P9CC -c plan9/sys/src/libplumb/plumbsendtext.c -o build/p9-plumb-sendtext.o
$LD build/crt9.o build/lib9.o build/lib9p.o build/draw9.o build/p9-sam-*.o build/p9-plumb-*.o build/libp9.a -o rootfs/bin/sam
"$BINARYEN/bin/wasm-opt" rootfs/bin/sam \
  --asyncify --pass-arg=asyncify-imports@env.forka,env.setj,env.longj -O2 \
  --enable-mutable-globals --enable-sign-ext --enable-bulk-memory \
  --enable-nontrapping-float-to-int \
  -o rootfs/bin/sam.tmp && mv rootfs/bin/sam.tmp rootfs/bin/sam
echo "  bin/sam  $(wc -c < rootfs/bin/sam | tr -d ' ') bytes (REAL sam, asyncified)"

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
  case "$b" in drtest) continue;; esac   # built above, against the real draw.h
  $CC -c "$c" -o "build/$b.o"
  $LD build/crt0.o build/lib9.o build/lib9p.o build/draw9.o "build/$b.o" build/libp9.a -o "rootfs/bin/$b"
  case " $ASYNCIFY " in *" $b "*)
    "$BINARYEN/bin/wasm-opt" "rootfs/bin/$b" \
      --asyncify --pass-arg=asyncify-imports@env.forka,env.setj,env.longj -O2 \
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
