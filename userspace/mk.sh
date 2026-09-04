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

# VERSIONS drift check: the measured findings are version-dependent (six-hats
# catch, 2026-08-29). Warn — never fail — when the found toolchain differs.
vcheck() { # vcheck key found — VERSIONS may list several measured versions per key
  exp=$(awk -v k="$1" '$1==k{$1=""; sub(/^ /,""); print}' VERSIONS)
  [ -z "$exp" ] && return
  for v in $exp; do [ "$v" = "$2" ] && return; done
  echo "mk.sh: WARNING: $1 is $2, VERSIONS records $exp (findings are version-dependent; re-verify, then update VERSIONS)" >&2
}
vcheck wasi-sdk-clang "$("$SDK/bin/clang" --version | sed -n '1s/clang version \([^ ]*\).*/\1/p')"
vcheck wasm-opt "$("$BINARYEN/bin/wasm-opt" --version | awk '{print $3}')"
vcheck bison "$(bison --version | sed -n '1s/.* \([0-9.]*\)$/\1/p')"
vcheck node "$(node --version)"
vcheck go "$(go version | awk '{print $3}')"

mkdir -p build rootfs/bin rootfs/srv rootfs/mnt rootfs/tmp
$CC -c libc/crt0.c -o build/crt0.o
$CC -c libc/crt9.c -o build/crt9.o
$CC -c libc/lib9.c -o build/lib9.o
$CC -c libc/lib9p.c -o build/lib9p.o
$CC -c libc/draw9.c -o build/draw9.o
# libp9.a: the REAL Plan 9 libraries — libc (port, fmt, 9sys), libbio,
# libregexp — compiled from verbatim 4th-edition source. Floats excluded
# for now (fltfmt/strtod need libm); -fms-extensions carries kencc's
# anonymous struct members (Biobufhdr in Biobuf).
P9CC="$SDK/bin/clang --target=wasm32 -nostdlib -O1 -fno-builtin -fms-extensions -Xclang -fwchar-type=int -Xclang -fno-signed-wchar -Wno-incompatible-pointer-types -Wno-int-conversion -Iplan9/include -Iplan9/sys/include -Wno-unknown-pragmas -Wno-unused-variable -Wno-unused-parameter -Wno-parentheses -Wno-empty-body -Wno-comment -Wno-deprecated-non-prototype -Wno-implicit-int -Wno-return-type -Wno-main-return-type"
P9EXCLUDE="execl"   # execl: &f+1 assumes stack varargs; lib9 has a va_arg one. The float door is open: wasm has native f64.
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
  --asyncify --pass-arg=asyncify-imports@env.forka,env.setj,env.longj,env.tsave,env.tjump -O2 \
  --enable-mutable-globals --enable-sign-ext --enable-bulk-memory \
  --enable-nontrapping-float-to-int \
  -o rootfs/bin/rc.tmp && mv rootfs/bin/rc.tmp rootfs/bin/rc
echo "  bin/rc  $(wc -c < rootfs/bin/rc | tr -d ' ') bytes (REAL rc, asyncified)"

# the real libdraw and libframe, as archives; draw clients link them.
# stringbg.c writes `int op` against draw.h's `Drawop op` — kencc shrugged,
# clang errors — so the build derives the reconciled file, as with io.c.
sed 's/, int op)/, Drawop op)/' plan9/sys/src/libdraw/stringbg.c > build/libdraw-stringbg.c
: > build/p9draw.list
for c in plan9/sys/src/libdraw/*.c plan9/sys/src/libframe/*.c; do
  b=$(basename "$c" .c)
  src="$c"
  case "$b" in stringbg) src=build/libdraw-stringbg.c;; esac
  $P9CC -c "$src" -o "build/p9-draw-$b.o"
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
# a terminal exists. L"..." literals are 32-bit UNSIGNED Runes: P9CC sets
# -fwchar-type=int -fno-signed-wchar so wide literals match u.h's
# late-4th-edition 21-bit Rune (typedef uint) exactly;
# `!` shell escapes fork bare, hence asyncify.
for c in plan9/sys/src/cmd/sam/*.c; do
  b=$(basename "$c" .c)
  $P9CC -Iplan9/sys/src/cmd/sam -c "$c" -o "build/p9-sam-$b.o"
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
  --asyncify --pass-arg=asyncify-imports@env.forka,env.setj,env.longj,env.tsave,env.tjump -O2 \
  --enable-mutable-globals --enable-sign-ext --enable-bulk-memory \
  --enable-nontrapping-float-to-int \
  -o rootfs/bin/sam.tmp && mv rootfs/bin/sam.tmp rootfs/bin/sam
cp rootfs/bin/sam rootfs/bin/sam9   # the raster original, name stepped back
echo "  bin/sam9  $(wc -c < rootfs/bin/sam9 | tr -d ' ') bytes (heritage raster sam)"

# libthread (ours: the wasm platform layer under the real thread.h) and
# its test — threaded binaries are asyncified, their contexts demand it
$P9CC -c libc/libthread.c -o build/libthread.o
$P9CC -c cmd/threadtest.c -o build/threadtest.o
$LD build/crt9.o build/lib9.o build/lib9p.o build/draw9.o build/threadtest.o build/libthread.o build/libp9.a -o rootfs/bin/threadtest
"$BINARYEN/bin/wasm-opt" rootfs/bin/threadtest \
  --asyncify --pass-arg=asyncify-imports@env.forka,env.setj,env.longj,env.tsave,env.tjump -O2 \
  --enable-mutable-globals --enable-sign-ext --enable-bulk-memory \
  --enable-nontrapping-float-to-int \
  -o rootfs/bin/threadtest.tmp && mv rootfs/bin/threadtest.tmp rootfs/bin/threadtest
echo "  bin/threadtest  $(wc -c < rootfs/bin/threadtest | tr -d ' ') bytes (libthread, asyncified)"
$P9CC -c cmd/con.c -o build/con.o
$LD build/crt9.o build/lib9.o build/lib9p.o build/draw9.o build/con.o build/libthread.o build/libp9.a -o rootfs/bin/con
"$BINARYEN/bin/wasm-opt" rootfs/bin/con \
  --asyncify --pass-arg=asyncify-imports@env.forka,env.setj,env.longj,env.tsave,env.tjump -O2 \
  --enable-mutable-globals --enable-sign-ext --enable-bulk-memory \
  --enable-nontrapping-float-to-int \
  -o rootfs/bin/con.tmp && mv rootfs/bin/con.tmp rootfs/bin/con
echo "  bin/con  $(wc -c < rootfs/bin/con | tr -d ' ') bytes (console-today)"
# emca: the IPNX half of the user interface (implementation.md M14c). con's
# shape exactly — reader threads feeding one consumer — so it wants the same
# libthread and the same asyncify: a thread blocked in read() must park while
# the others keep running.
$P9CC -c cmd/emca.c -o build/emca.o
$LD build/crt9.o build/lib9.o build/lib9p.o build/draw9.o build/emca.o build/libthread.o build/libp9.a -o rootfs/bin/emca
"$BINARYEN/bin/wasm-opt" rootfs/bin/emca \
  --asyncify --pass-arg=asyncify-imports@env.forka,env.setj,env.longj,env.tsave,env.tjump -O2 \
  --enable-mutable-globals --enable-sign-ext --enable-bulk-memory \
  --enable-nontrapping-float-to-int \
  -o rootfs/bin/emca.tmp && mv rootfs/bin/emca.tmp rootfs/bin/emca
echo "  bin/emca  $(wc -c < rootfs/bin/emca | tr -d ' ') bytes (the workspace: windows, tags, verbs, placement)"
$P9CC -c cmd/sam.c -o build/samtoday.o
$LD build/crt9.o build/lib9.o build/lib9p.o build/draw9.o build/samtoday.o build/libthread.o build/libp9.a -o rootfs/bin/sam
"$BINARYEN/bin/wasm-opt" rootfs/bin/sam \
  --asyncify --pass-arg=asyncify-imports@env.forka,env.setj,env.longj,env.tsave,env.tjump -O2 \
  --enable-mutable-globals --enable-sign-ext --enable-bulk-memory \
  --enable-nontrapping-float-to-int \
  -o rootfs/bin/sam.tmp && mv rootfs/bin/sam.tmp rootfs/bin/sam
echo "  bin/sam  $(wc -c < rootfs/bin/sam | tr -d ' ') bytes (sam-today: the language as a filter — the name inherited)"
$P9CC -c cmd/acme.c -o build/acmetoday.o
$LD build/crt9.o build/lib9.o build/lib9p.o build/draw9.o build/acmetoday.o build/libthread.o build/libp9.a -o rootfs/bin/acme
"$BINARYEN/bin/wasm-opt" rootfs/bin/acme \
  --asyncify --pass-arg=asyncify-imports@env.forka,env.setj,env.longj,env.tsave,env.tjump -O2 \
  --enable-mutable-globals --enable-sign-ext --enable-bulk-memory \
  --enable-nontrapping-float-to-int \
  -o rootfs/bin/acme.tmp && mv rootfs/bin/acme.tmp rootfs/bin/acme
echo "  bin/acme  $(wc -c < rootfs/bin/acme | tr -d ' ') bytes (acme-today: the one editor, the name inherited)"

# samterm: the real editor's screen — libframe over libdraw over libthread,
# spoken to by sam over the mesg protocol; sam spawns /bin/aux/samterm
mkdir -p rootfs/bin/aux
# io.c's one `&mousectl->Mouse` names an unnamed member — kencc's idiom,
# GCC's -fplan9-extensions, but not clang's. The build derives an
# equivalent file (the member is at offset 0), as bison derives grammars.
sed 's/&mousectl->Mouse/(Mouse*)mousectl/' plan9/sys/src/cmd/samterm/io.c > build/samterm-io.c
for c in plan9/sys/src/cmd/samterm/*.c; do
  b=$(basename "$c" .c)
  src="$c"
  case "$b" in io) src=build/samterm-io.c;; esac
  $P9CC -Iplan9/sys/src/cmd/samterm -Iplan9/sys/src/cmd/sam -c "$src" -o "build/p9-samterm-$b.o"
done
$P9CC -c libc/mousekbd.c -o build/mousekbd.o
$LD build/crt9.o build/lib9.o build/lib9p.o build/draw9.o build/p9-samterm-*.o build/libthread.o build/mousekbd.o build/p9-plumb-mesg.o build/p9-plumb-sendtext.o build/libdraw.a build/libp9.a -o rootfs/bin/aux/samterm
"$BINARYEN/bin/wasm-opt" rootfs/bin/aux/samterm \
  --asyncify --pass-arg=asyncify-imports@env.forka,env.setj,env.longj,env.tsave,env.tjump -O2 \
  --enable-mutable-globals --enable-sign-ext --enable-bulk-memory \
  --enable-nontrapping-float-to-int \
  -o rootfs/bin/aux/samterm.tmp && mv rootfs/bin/aux/samterm.tmp rootfs/bin/aux/samterm
echo "  bin/aux/samterm  $(wc -c < rootfs/bin/aux/samterm | tr -d ' ') bytes (REAL samterm, asyncified)"

# acme: the real one — every column, tag and 9P file of it; threaded,
# forking, self-mounting. Asyncified like everything that forks bare.
# kencc's named access to unnamed members, reconciled per file into build/
# (the same derivation shape as bison over the grammars)
mkdir -p build/acme-src
# kencc converts a pointer-to-struct into a pointer to its unnamed member at
# call sites; clang does not, so acme's pervasive frinsert(t, ...) — Text* for
# Frame* — would make libframe write its fields over Text's head (measured:
# t->file became the font pointer; Buffer.cnc read Font.height|ascent<<16).
# Frame's first member is font, and with -fms-extensions x->font resolves to
# the embedded Frame's font for any embedder — so (Frame*)&(x)->font is the
# adjusted pointer whether x is a Frame* (+0) or a Text* (+offset).
cat > build/acme-src/frameadjust.h <<'FRADJEOF'
#define FRADJ(x) ((Frame*)&(x)->font)
#define frcharofpt(f, a)          (frcharofpt)(FRADJ(f), a)
#define frptofchar(f, a)          (frptofchar)(FRADJ(f), a)
#define frdelete(f, a, b)         (frdelete)(FRADJ(f), a, b)
#define frinsert(f, a, b, c)      (frinsert)(FRADJ(f), a, b, c)
#define frselect(f, a)            (frselect)(FRADJ(f), a)
#define frselectpaint(f, a, b, c) (frselectpaint)(FRADJ(f), a, b, c)
#define frdrawsel(f, a, b, c, d)  (frdrawsel)(FRADJ(f), a, b, c, d)
#define frdrawsel0(f, a, b, c, d, e) (frdrawsel0)(FRADJ(f), a, b, c, d, e)
#define frinit(f, a, b, c, d)     (frinit)(FRADJ(f), a, b, c, d)
#define frsetrects(f, a, b)       (frsetrects)(FRADJ(f), a, b)
#define frclear(f, a)             (frclear)(FRADJ(f), a)
#define frtick(f, a, b)           (frtick)(FRADJ(f), a, b)
#define frinittick(f)             (frinittick)(FRADJ(f))
#define frredraw(f)               (frredraw)(FRADJ(f))
FRADJEOF
for c in plan9/sys/src/cmd/acme/*.c; do
  sed -e 's/&mousectl->Mouse/(Mouse*)mousectl/' \
      -e 's/mousectl->Mouse/*(Mouse*)mousectl/' \
      -e 's/&x->Fcall/(Fcall*)\&x->type/' \
      -e 's/->Frame\./->/g' \
      -e 's/&\([a-zA-Z_]*\)->Frame/(Frame*)\&\1->font/g' \
      -e 's|#include <frame.h>|#include <frame.h>\n#include "frameadjust.h"|' \
      -e 's/xselect(t, mousectl/xselect((Frame*)\&t->font, mousectl/' \
      "$c" > "build/acme-src/$(basename "$c")"
done
for c in build/acme-src/*.c; do
  b=$(basename "$c" .c)
  $P9CC -Iplan9/sys/src/cmd/acme -c "$c" -o "build/p9-acme-$b.o"
done
# dat.h carries pre-ANSI tentative definitions in every TU (rc's disease,
# rc's cure): weaken every duplicated symbol except its initializing owner
acme_owner() {
  case "$1" in
    boxcursor) echo build/p9-acme-acme.o;;
    display|font|screen) echo libdraw;;   # the library owns them; every acme copy yields
    *) echo "";;
  esac
}
ACMEOBJS="$(echo build/p9-acme-*.o)"
for o in $ACMEOBJS; do
  "$SDK/bin/llvm-nm" --defined-only --extern-only "$o" | awk -v o="$o" '{print o, $3}'
done > build/acme-defs.txt
awk '{n[$2]++} END{for(s in n) if(n[s]>1) print s}' build/acme-defs.txt > build/acme-dups.txt
printf 'display\nfont\nscreen\n' >> build/acme-dups.txt
for o in $ACMEOBJS; do
  weak=""
  while read -r sym; do
    grep -q "^$o $sym\$" build/acme-defs.txt || continue
    own=$(acme_owner "$sym")
    [ -z "$own" ] && own=$(awk -v s="$sym" '$2==s{print $1; exit}' build/acme-defs.txt)
    [ "$o" = "$own" ] && continue
    weak="$weak $sym"
  done < build/acme-dups.txt
  [ -n "$weak" ] && node weaken.mjs "$o" $weak
done
$P9CC -c plan9/sys/src/libcomplete/complete.c -o build/p9-complete.o
$LD build/crt9.o build/lib9.o build/lib9p.o build/draw9.o build/p9-acme-*.o build/libthread.o build/mousekbd.o build/p9-plumb-mesg.o build/p9-plumb-sendtext.o build/p9-complete.o build/libdraw.a build/libp9.a -o rootfs/bin/acme9
"$BINARYEN/bin/wasm-opt" rootfs/bin/acme9 \
  --asyncify --pass-arg=asyncify-imports@env.forka,env.setj,env.longj,env.tsave,env.tjump -O2 \
  --enable-mutable-globals --enable-sign-ext --enable-bulk-memory \
  --enable-nontrapping-float-to-int \
  -o rootfs/bin/acme9.tmp && mv rootfs/bin/acme9.tmp rootfs/bin/acme9
echo "  bin/acme9  $(wc -c < rootfs/bin/acme9 | tr -d ' ') bytes (heritage raster acme)"

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
  # su is NOT built: its mechanism (credential transitions through
  # /proc/<pid>/ctl) left the kernel in P1 step 4, and P2 step 4 rewrites it as
  # a userspace personality program. The source stays for that rewrite.
  case "$b" in drtest|threadtest|con|emca|acme|sam) continue;; esac   # built above, against the real headers
  case "$b" in su) continue;; esac
  $CC -c "$c" -o "build/$b.o"
  $LD build/crt0.o build/lib9.o build/lib9p.o build/draw9.o "build/$b.o" build/libp9.a -o "rootfs/bin/$b"
  case " $ASYNCIFY " in *" $b "*)
    "$BINARYEN/bin/wasm-opt" "rootfs/bin/$b" \
      --asyncify --pass-arg=asyncify-imports@env.forka,env.setj,env.longj,env.tsave,env.tjump -O2 \
      --enable-mutable-globals --enable-sign-ext --enable-bulk-memory \
      --enable-nontrapping-float-to-int \
      -o "rootfs/bin/$b.tmp" && mv "rootfs/bin/$b.tmp" "rootfs/bin/$b"
    echo "  bin/$b  $(wc -c < "rootfs/bin/$b" | tr -d ' ') bytes (asyncified)" ;;
  *)
    echo "  bin/$b  $(wc -c < "rootfs/bin/$b" | tr -d ' ') bytes" ;;
  esac
done
# ---- the WASI second ABI: real foreign citizens against the same kernel ----
# wasitest is wasi-libc through the FULL sysroot (no -nostdlib): it imports
# wasi_snapshot_preview1 and knows nothing of Plan 9; the shim is
# supervisor/wasi1.mjs, and the preopen is the namespace root.
"$SDK/bin/clang" --target=wasm32-wasip1 --sysroot="$SDK/share/wasi-sysroot" -O2 \
  wasi/wasitest.c -o rootfs/bin/wasitest
echo "  bin/wasitest  $(wc -c < rootfs/bin/wasitest | tr -d ' ') bytes (wasi-libc citizen)"
# gotest is a REAL Go binary: the wasip1 target of the first benchmark's toolchain
(cd wasi/gotest && GOOS=wasip1 GOARCH=wasm go build -trimpath -o ../../rootfs/bin/gotest .)
echo "  bin/gotest  $(wc -c < rootfs/bin/gotest | tr -d ' ') bytes (REAL Go, wasip1)"
(cd wasi/gohello && GOOS=wasip1 GOARCH=wasm go build -trimpath -o ../../rootfs/bin/gohello .)
echo "  bin/gohello  $(wc -c < rootfs/bin/gohello | tr -d ' ') bytes (REAL Go, wasip1)"

# The third citizen: REAL CPython, Brett Cannon's wasi_sdk build of 3.14.7,
# cached in build/ (a 26MB download, once). The FULL stdlib ships (2026-08-29,
# the toolchain-personality decision): the build's own 529-file Lib, matched
# to the binary — a real Python personality, not a demo subset. pip installs
# into site-packages; lib-dynload stays empty (no shared objects on wasm).
PYZIP=build/python-wasi-3.14.7.zip
if [ ! -f "$PYZIP" ]; then
  curl -fsSL -o "$PYZIP" https://github.com/brettcannon/cpython-wasi-build/releases/download/v3.14.7/python-3.14.7-wasi_sdk-24.zip
fi
unzip -o -q "$PYZIP" python.wasm -d build/pyx
cp build/pyx/python.wasm rootfs/bin/python
mkdir -p rootfs/lib/python3.14/lib-dynload rootfs/lib/python3.14/site-packages
unzip -o -q "$PYZIP" "lib/*" -d rootfs/
cp wasi/pyshim/*.py rootfs/lib/python3.14/   # personality files (pure-Python zlib &c)
cp wasi/pytest.py rootfs/tmp/pytest.py
echo "  bin/python  $(wc -c < rootfs/bin/python | tr -d ' ') bytes (REAL CPython 3.14.7, wasi)"

# pack the rootfs for the browser host (fetched by browser/main.mjs)
node -e '
const fs = require("fs"), p = require("path");
const out = {};
(function walk(d, pre){
  const es = fs.readdirSync(d);
  if (es.length === 0) { out[pre] = null; return; }   // empty dir: a marker, so /srv survives
  for (const e of es) {
    const f = p.join(d, e), s = fs.statSync(f);
    if (s.isDirectory()) walk(f, pre + e + "/");
    else out[pre + e] = fs.readFileSync(f).toString("base64");
  }
})("rootfs", "/");
fs.writeFileSync("build/rootfs.json", JSON.stringify(out));
'
echo "  build/rootfs.json  $(wc -c < build/rootfs.json | tr -d " ") bytes"
