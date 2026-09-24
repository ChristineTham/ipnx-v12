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
# The ROOTFS — what the machine serves over 9P, and what `/` becomes once
# boot has mounted it. `#/boot` is a different thing and holds one file.
root=$here/root

[ -x "$CC" ] || { echo "mk.sh: no wasi-sdk at $WASI_SDK (set WASI_SDK)" >&2; exit 1; }

CFLAGS="--target=wasm32-unknown-unknown -nostdlib -nostdinc -fno-builtin -fms-extensions -std=gnu89 -O2
	-mbulk-memory -mnontrapping-fptoint -msign-ext
	-mexception-handling -mllvm -wasm-enable-sjlj -mllvm -wasm-use-legacy-eh=false
	-I$here/include -I$here/libc/fmt
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
libc() {
	local objs=() src obj
	mkdir -p "$build/libc"
	for src in "$here"/libc/wasm/*.c "$here"/libc/fmt/*.c \
	           "$here"/libc/port/*.c "$here"/libc/9sys/*.c; do
		case "$(basename "$(dirname "$src")")/$(basename "$src")" in
		$LIBCSKIP) continue;;
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

# Portable sources that do not belong to this machine: they reach for calls
# this kernel does not have (segments, notes, semaphores, the network) or for
# tables no command here needs.
LIBCSKIP='@(port|9sys)/@(atnotify|hangup|lock|profile|netcrypt|crypt|truerand|ntruerand|notify|postnote|announce|dial|pushssl|pushtls|getnetconninfo|setnetmtpt|syslog|qlock|privalloc|sbrk|read9pmsg|procsetname|fork|execl|readv|writev|byteserial|encodefmt|netmkaddr|mktemp).c'
shopt -s extglob

# ---- the other libraries --------------------------------------------------
# Plan 9 keeps these apart and so does this: `libbio.a` is buffered i/o,
# `libauth.a` is `newns` — which is all of libauth that is left once there is
# no factotum to talk to — and `libregexp.a` is Plan 9's regular expressions,
# vendored whole, for `sed`, `ed` and the rest.
lib() {	# lib <name> <dir>
	local name=$1 dir=$2 objs=() src obj
	mkdir -p "$build/$name"
	for src in "$here/$dir"/*.c; do
		obj=$build/$name/$(basename "$src" .c).o
		if [ ! -f "$obj" ] || [ "$src" -nt "$obj" ]; then
			cc "$src" "$obj"
		fi
		objs+=("$obj")
	done
	rm -f "$build/$name.a"
	$AR rcs "$build/$name.a" "${objs[@]}"
}

# ---- a command ------------------------------------------------------------
link() {	# link <into> <obj>...
	local into=$1; shift
	$LD $LDFLAGS -o "$into" "$@" \
		"$build/libauth.a" "$build/libregexp.a" "$build/libbio.a" "$build/libc.a"
}

cmd() {	# cmd <name> [<into>]
	local name=$1 obj=$build/$1.o
	cc "$here/cmd/$name.c" "$obj"
	link "${2:-$pkg/$OBJTYPE/bin/$name}" "$obj"
}

# A command that does not build is recorded and the build goes on — `mk -k`
# — so one missing piece does not hide every other. What is recorded is the
# work that is left, with the reason, in build/failed.
failed=$build/failed
: >"$failed"
try() {	# try <name>
	local log=$build/$1.log
	if ! cmd "$1" >"$log" 2>&1; then
		echo "$1: $(grep -m1 -oE "fatal error: '[^']*'|undefined symbol: [A-Za-z0-9_]+" "$log" | sort -u | tr '\n' ' ')" >>"$failed"
	fi
}

libc
lib libbio libbio
lib libauth libauth
lib libregexp libregexp

for c in "$here"/cmd/*.c; do
	[ -e "$c" ] || continue
	name=$(basename "$c" .c)
	case $name in
	# `boot` is the one file `#/boot` carries, as Plan 9's kernel carries
	# `/boot/boot` and nothing else. Everything else is the file server's.
	boot)	cmd boot "$build/boot/boot";;
	# and `init` sits beside the bin directory, not in it, because boot
	# execs it by a name that must resolve before `/bin` exists
	# (`execinit`, `boot.c:205`: `"/%s/init"` with `$cputype`).
	init)	cmd init "$root/$OBJTYPE/init";;
	*)	try "$name";;
	esac
done

# ---- the rest of the rootfs -----------------------------------------------
cp -f "$here"/profile/* "$root/profile/"
cp -f "$here"/usr/kitty/profile/* "$root/usr/kitty/profile/"
cp -f "$here/etc/motd" "$root/etc/motd"
cp -f "$here/pkg/system/pkg.cfg" "$pkg/pkg.cfg"
# what is installed to the system (docs/packages.md): `ndb`, one tuple each
echo "pkg=system version=$VERSION" >"$root/profile/pkg"

# ---- rc -------------------------------------------------------------------
# rc is Plan 9's, taken entire. `ipnx.c` is its platform file — Plan 9 ships
# three of those and the mkfile picks one — and `haventfork.c` is Plan 9's own
# answer for a system that cannot fork, which this machine cannot.
if [ -f "$here/rc/rc.h" ]; then
	mkdir -p "$build/rc"
	(cd "$build/rc" && bison -y -d "$here/rc/syn.y" >/dev/null 2>&1)
	cp -f "$build/rc/y.tab.h" "$build/rc/x.tab.h"
	objs=()
	for src in "$here"/rc/*.c "$build/rc/y.tab.c"; do
		obj=$build/rc/$(basename "$src" .c).o
		$CC $CFLAGS -I"$here/rc" -I"$build/rc" -c "$src" -o "$obj"
		objs+=("$obj")
	done
	cp -f "$here/rc/rcmain" "$pkg/lib/rcmain"
	# rc.h's tentative definitions are common symbols everywhere but here;
	# weaken.py explains the whole of it.
	python3 "$here/weaken.py" "$WASI_SDK/bin/llvm-nm" "${objs[@]}"
	link "$pkg/$OBJTYPE/bin/rc" "${objs[@]}"
fi

echo "mk.sh: #/boot: $(ls "$build/boot" | tr '\n' ' ')"
echo "mk.sh: /pkg/system/$VERSION/$OBJTYPE/bin: $(ls "$pkg/$OBJTYPE/bin" | wc -l) programs"
if [ -s "$failed" ]; then
	echo "mk.sh: $(wc -l <"$failed") did not build (build/failed):"
	sed 's/^/	/' "$failed"
fi
