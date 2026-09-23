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
	-I$here/include -I$here/libc/fmt
	-Wall -Wno-unknown-pragmas -Wno-parentheses -Wno-missing-braces
	-Wno-unused-value -Wno-unused-but-set-variable -Wno-incompatible-pointer-types
	-Wno-dangling-else -Wno-empty-body -Wno-implicit-int-float-conversion -Wno-unused-variable -Wno-unused-parameter -Wno-unused-label -Wno-implicit-int -Wno-implicit-function-declaration -Wno-incompatible-library-redeclaration -Wno-builtin-requires-header"

# `__stack_pointer` is exported because `notify(Ureg*)` writes the note onto
# the process's stack below its stack pointer (pc/trap.c:834), and on this
# machine the stack pointer is that global.
LDFLAGS="--no-entry --export=_start --export-memory --export=__stack_pointer --stack-first -z stack-size=65536 --allow-multiple-definition"

# The rootfs is laid out as Plan 9 lays one out: the binaries under
# `/$objtype`, the scripts under `/rc`, and `/bin` a UNION of the two made by
# `/lib/namespace` — never a directory of its own.
OBJTYPE=wasm
mkdir -p "$build" "$build/boot" \
	"$root/$OBJTYPE/bin" "$root/rc/bin" "$root/rc/lib" "$root/lib" \
	"$root/etc" "$root/tmp"

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
# no factotum to talk to.
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
		"$build/libauth.a" "$build/libbio.a" "$build/libc.a"
}

cmd() {	# cmd <name> [<into>]
	local name=$1 obj=$build/$1.o
	cc "$here/cmd/$name.c" "$obj"
	link "${2:-$root/$OBJTYPE/bin/$name}" "$obj"
}

libc
lib libbio libbio
lib libauth libauth

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
	*)	cmd "$name";;
	esac
done

# ---- the rest of the rootfs -----------------------------------------------
cp -f "$here/rc/termrc" "$root/rc/bin/termrc"
cp -f "$here/lib/namespace" "$root/lib/namespace"
cp -f "$here/etc/motd" "$root/etc/motd"

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
	cp -f "$here/rc/rcmain" "$root/rc/lib/rcmain"
	# rc.h's tentative definitions are common symbols everywhere but here;
	# weaken.py explains the whole of it.
	python3 "$here/weaken.py" "$WASI_SDK/bin/llvm-nm" "${objs[@]}"
	link "$root/$OBJTYPE/bin/rc" "${objs[@]}"
fi

echo "mk.sh: #/boot: $(ls "$build/boot" | tr '\n' ' ')"
echo "mk.sh: /$OBJTYPE/bin: $(ls "$root/$OBJTYPE/bin" | tr '\n' ' ')"
