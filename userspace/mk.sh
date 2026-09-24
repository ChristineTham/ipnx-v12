#!/bin/bash
# Build the IPNX userspace: libc, then the commands, then rc.
#
# Output is `userspace/build/` (objects) and `userspace/root/` (the files a
# booting system finds by name). Both are generated and gitignored.
#
# The compiler is wasi-sdk's clang, used for its wasm backend ONLY: nothing
# here links against wasi, and a built binary imports exactly the calls in
# `libc/wasm/sys.c` and nothing else. Apple's clang has no wasm backend, which
# is why the SDK is a prerequisite rather than a convenience.
#
# `-fno-builtin` is load-bearing (RESEARCH §9.4): clang's libcall recogniser
# otherwise rewrites strlen's own body into a call to strlen.
#
# `-wasm-enable-sjlj` is how this machine has `setjmp` at all: the compiler
# turns it into wasm exception handling, in the `try_table` encoding (the
# legacy one is rejected by these engines), and `libc/wasm/setjmp.c` is the
# library's half of it.
set -e

WASI_SDK=${WASI_SDK:-$HOME/.local/opt/wasi-sdk}
CC=$WASI_SDK/bin/clang
LD=$WASI_SDK/bin/wasm-ld
AR=$WASI_SDK/bin/ar

here=$(cd "$(dirname "$0")" && pwd)
build=$here/build
# Plan 9's tree, vendored: `sys/include` and `sys/src` as they are there
sys=$here/sys
# The ROOTFS — what the machine serves over 9P, and what `/` becomes once
# boot has mounted it. `#/boot` is a different thing and holds one file.
root=$here/root

[ -x "$CC" ] || { echo "mk.sh: no wasi-sdk at $WASI_SDK (set WASI_SDK)" >&2; exit 1; }

CFLAGS="--target=wasm32-unknown-unknown -nostdlib -nostdinc -fno-builtin -fms-extensions -std=gnu89 -O2
	-mbulk-memory -mnontrapping-fptoint -msign-ext
	-mexception-handling -mllvm -wasm-enable-sjlj -mllvm -wasm-use-legacy-eh=false
	-I$here/wasm/include -I$sys/include -I$sys/src/libc/fmt
	-Wall -Wno-unknown-pragmas -Wno-parentheses -Wno-missing-braces
	-Wno-unused-value -Wno-unused-but-set-variable -Wno-incompatible-pointer-types
	-Wno-dangling-else -Wno-empty-body -Wno-implicit-int-float-conversion -Wno-unused-variable -Wno-unused-parameter -Wno-unused-label -Wno-implicit-int -Wno-implicit-function-declaration -Wno-incompatible-library-redeclaration -Wno-builtin-requires-header"

# `__stack_pointer` is exported because `notify(Ureg*)` writes the note onto
# the process's stack below its stack pointer (pc/trap.c:834), and on this
# machine the stack pointer is that global.
LDFLAGS="--no-entry --export=_start --export-memory --export=__stack_pointer --stack-first -z stack-size=65536 --allow-multiple-definition"

# The rootfs: the programs in ONE package, `system` — Plan 9's userland as
# this machine runs it, its commands and rc (docs/packages.md) — at
# `/pkg/system/<version>/`, laid out as the root it binds onto: `$objtype/bin`
# onto `/bin`, `lib` onto `/lib`, both by `/profile/start.ns`. `init` stays
# at `/$objtype/init`, outside it, because boot runs it before any `/bin`
# exists. The system's configuration is in `/profile`; each user's under
# `/usr/<name>`, bound at `/home`. What older builds left — `/rc`,
# `/lib/namespace`, `/$objtype/bin`, `/lib/rcmain` — is cleared.
OBJTYPE=wasm
VERSION=2026.09.24
pkg=$root/pkg/system/$VERSION
rm -rf "$root/rc" "$root/lib/namespace" "$root/lib/rcmain" "$root/$OBJTYPE/bin" \
	"$root/pkg/system" "$root/profile" "$root/usr/kitty/profile"
mkdir -p "$build" "$build/boot" \
	"$pkg/$OBJTYPE/bin" "$pkg/lib" "$root/$OBJTYPE" "$root/lib" "$root/profile" "$root/home" \
	"$root/usr/kitty/profile" "$root/etc" "$root/tmp"

cc() {	# cc <src> <obj>
	$CC $CFLAGS -c "$1" -o "$2"
}

# ---- libc -----------------------------------------------------------------
# Plan 9's own arrangement (`libc/mkfile`): the portable directories `port`,
# `9sys` and `fmt`, and the machine's own — here `wasm` — which supplies what
# the architecture must (`setjmp`, `tas`, the atomics, the call stubs) and
# replaces a portable file of the same name, as `reduce` does for 386's.
# Nothing of Plan 9's is left out.
libc() {
	local objs=() src obj name
	mkdir -p "$build/libc"
	for src in "$sys"/src/libc/wasm/*.c "$sys"/src/libc/fmt/*.c \
	           "$sys"/src/libc/port/*.c "$sys"/src/libc/9sys/*.c; do
		name=$(basename "$src")
		case "$src" in
		*/wasm/*) ;;
		*) [ -e "$sys/src/libc/wasm/$name" ] && continue;;
		esac
		obj=$build/libc/$(basename "$(dirname "$src")")-$(basename "$src" .c).o
		if [ ! -f "$obj" ] || [ "$src" -nt "$obj" ]; then
			cc "$src" "$obj"
		fi
		objs+=("$obj")
	done
	rm -f "$build/libc.a"
	$AR rcs "$build/libc.a" "${objs[@]}"
}

# ---- the other libraries, and the commands --------------------------------
# From Plan 9's own mkfiles, every one: `mkfile.py` reads what each declares
# and builds it with this machine's compiler and loader. The objects and
# libraries are build/'s; a program lands where its mkfile's BIN says, under
# the system package, and `init` at /$objtype/init, as `cmd/mkfile:116` puts
# it. What does not build goes into build/failed with its reason.
link() {	# link <into> <obj>...
	local into=$1; shift
	$LD $LDFLAGS -o "$into" "$@" "$build"/lib/*.a "$build/libc.a"
}

libc
failed=$build/failed
: >"$failed"
export CC LD AR CFLAGS LDFLAGS build pkg root OBJTYPE
python3 "$here/mkfile.py" libs libc
python3 "$here/mkfile.py" cmds

# `boot` is the one file `#/boot` carries, as Plan 9's kernel carries
# `/boot/boot` and nothing else; this machine's, not Plan 9's (`boot/`).
$CC $CFLAGS -c "$here/cmd/boot.c" -o "$build/boot.o"
link "$build/boot/boot" "$build/boot.o"
# `args`, a test's: what a program is given
$CC $CFLAGS -c "$here/cmd/args.c" -o "$build/args.o"
link "$pkg/$OBJTYPE/bin/args" "$build/args.o"

# ---- the rest of the rootfs -----------------------------------------------
cp -f "$here"/profile/* "$root/profile/"
cp -f "$here"/usr/kitty/profile/* "$root/usr/kitty/profile/"
cp -f "$here/etc/motd" "$root/etc/motd"
cp -f "$here/pkg/system/pkg.cfg" "$pkg/pkg.cfg"
# what is installed to the system (docs/packages.md): `ndb`, one tuple each
echo "pkg=system version=$VERSION" >"$root/profile/pkg"

# ---- rc -------------------------------------------------------------------
# rc is Plan 9's, built from its own mkfile's COMMONOFILES and y.tab, with two
# substitutions both of which Plan 9 provides for: `haventfork.c` for
# `havefork.c`, its own file for a system that cannot fork, which this
# machine cannot; and `ipnx.c` for `plan9.c`, the platform file — Plan 9
# ships three (`plan9.c`, `unix.c`, `win32.c`) and the mkfile picks one.
rcdir=$sys/src/cmd/rc
rcfiles=$(sed -n '/^COMMONOFILES=/,/^$/p' "$rcdir/mkfile" | grep -o '[a-z]*\.\$O' | sed 's/\.\$O$//; s/^havefork$/haventfork/')
mkdir -p "$build/rc"
(cd "$build/rc" && bison -y -d "$rcdir/syn.y" >/dev/null 2>&1)
cp -f "$build/rc/y.tab.h" "$build/rc/x.tab.h"
objs=()
for f in $rcfiles ipnx; do
	obj=$build/rc/$f.o
	$CC $CFLAGS -I"$rcdir" -I"$build/rc" -c "$rcdir/$f.c" -o "$obj"
	objs+=("$obj")
done
$CC $CFLAGS -I"$rcdir" -I"$build/rc" -c "$build/rc/y.tab.c" -o "$build/rc/y.tab.o"
objs+=("$build/rc/y.tab.o")
cp -f "$rcdir/rcmain" "$pkg/lib/rcmain"
# rc.h's tentative definitions are common symbols everywhere but here;
# weaken.py explains the whole of it.
python3 "$here/weaken.py" "$WASI_SDK/bin/llvm-nm" "${objs[@]}"
link "$pkg/$OBJTYPE/bin/rc" "${objs[@]}"

# **The second pass.** Some sources are made by a Plan 9 program — libsec's
# curves by `mpc` (`libsec/port/mkfile`: `%.c:D: %.mp`) — and Plan 9 builds
# itself with itself, so the recipe runs on this system: `ipnx` is built,
# and what failed is built again with it.
if [ -s "$failed" ] && command -v cargo >/dev/null; then
	(cd "$here/.." && env -u CC -u CFLAGS -u LD -u LDFLAGS -u AR cargo build -q -p ipnx) && export IPNX="$here/../target/debug/ipnx"
	if [ -n "$IPNX" ]; then
		: >"$failed"
		python3 "$here/mkfile.py" libs libc
		python3 "$here/mkfile.py" cmds
	fi
fi

echo "mk.sh: #/boot: $(ls "$build/boot" | tr '\n' ' ')"
echo "mk.sh: /pkg/system/$VERSION/$OBJTYPE/bin: $(ls "$pkg/$OBJTYPE/bin" | wc -l) programs"
if [ -s "$failed" ]; then
	echo "mk.sh: $(wc -l <"$failed") did not build (build/failed):"
	sed 's/^/	/' "$failed"
fi
