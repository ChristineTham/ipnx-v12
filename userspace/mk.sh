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
root=$here/root

[ -x "$CC" ] || { echo "mk.sh: no wasi-sdk at $WASI_SDK (set WASI_SDK)" >&2; exit 1; }

CFLAGS="--target=wasm32-unknown-unknown -nostdlib -nostdinc -fno-builtin -fms-extensions -std=gnu89 -O2
	-mbulk-memory -mnontrapping-fptoint -msign-ext
	-I$here/include -I$here/libc/fmt
	-Wall -Wno-unknown-pragmas -Wno-parentheses -Wno-missing-braces
	-Wno-unused-value -Wno-unused-but-set-variable -Wno-incompatible-pointer-types
	-Wno-dangling-else -Wno-empty-body -Wno-implicit-int-float-conversion -Wno-unused-variable -Wno-unused-parameter -Wno-unused-label -Wno-implicit-int -Wno-implicit-function-declaration -Wno-incompatible-library-redeclaration -Wno-builtin-requires-header"

LDFLAGS="--no-entry --export=_start --export-memory --stack-first -z stack-size=65536 --allow-multiple-definition"

mkdir -p "$build" "$root/bin"

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
LIBCSKIP='@(port|9sys)/@(atexit|atnotify|hangup|lock|perror|profile|netcrypt|crypt|truerand|ntruerand|notify|postnote|announce|dial|pushssl|pushtls|getnetconninfo|setnetmtpt|syslog|qlock|privalloc|sbrk|read9pmsg|procsetname|fork|execl|readv|writev|byteserial|encodefmt|netmkaddr|mktemp).c'
shopt -s extglob

# ---- a command ------------------------------------------------------------
link() {	# link <name> <obj>...
	local name=$1; shift
	$LD $LDFLAGS -o "$root/bin/$name" "$@" "$build/libc.a"
}

cmd() {	# cmd <name>
	local name=$1 obj=$build/$1.o
	cc "$here/cmd/$name.c" "$obj"
	link "$name" "$obj"
}

libc
for c in "$here"/cmd/*.c; do
	[ -e "$c" ] || continue
	cmd "$(basename "$c" .c)"
done

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
	# rc.h's tentative definitions are common symbols everywhere but here;
	# common.py explains the whole of it and puts the objects in the only
	# order that keeps an initialised definition.
	cp -f "$here/rc/rcmain" "$root/bin/rcmain"
	# rc.h's tentative definitions are common symbols everywhere but here;
	# weaken.py explains the whole of it.
	python3 "$here/weaken.py" "$WASI_SDK/bin/llvm-nm" "${objs[@]}"
	link rc "${objs[@]}"
fi

echo "mk.sh: built $(ls "$root/bin" | tr '\n' ' ')"
