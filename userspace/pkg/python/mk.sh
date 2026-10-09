#!/bin/bash
# Python, as a package (docs/implementation.md, P10; docs/packages.md): a
# WASI program, run natively (docs/architecture.md, *A WASI binary runs
# natively*). CPython's own WASI build — its source at the tag `pkg.cfg`
# names, built by its own script (`Tools/wasm/wasi`) with this machine's
# wasi-sdk — laid out in the store as `/pkg/python/<version>/` and installed
# to the system as `pkg install` installs one (`cmd/pkg`): a line in
# `/profile/pkg`, its binds in `/profile/pkg.ns`.
#
# Run by `mk.sh`, which gives it `$build`, `$root`, `$OBJTYPE` and
# `$WASI_SDK`. The build is kept in `$build`, so it is made once.
#
# **Its prefix is `/sys`**, so its library is `/sys/lib/python3.14`, where
# Plan 9 keeps a system library's files (`/sys/lib/ghostscript`,
# `/sys/lib/lex`) — not `/usr/local`, as `/usr` is the users'.
#
# The package is what CPython's `make install` installs, less what running
# it does not use: the regression tests (`test/`, 161 MB with their
# bytecode), the static library for embedding it (`config-*`, 43 MB), and
# the bytecode, which Python makes again.
set -e
here=$(cd "$(dirname "$0")" && pwd)
version=$(sed -n 's/^pkg=python version=\([^ ]*\).*/\1/p' "$here/pkg.cfg")
short=${version%.*}
src=$build/cpython-$version
stage=$build/python-$version

if [ ! -f "$stage/sys/bin/python$short.wasm" ]; then
	[ -d "$src" ] || git clone -q --depth 1 --branch "v$version" https://github.com/python/cpython "$src"
	cd "$src"
	# a build Python of the same version first, then the WASI one, which
	# runs nothing of itself here: `--host-runner` is only for its own
	# check at the end, which needs wasmtime's command
	python3 Tools/wasm/wasi build-python
	WASI_SDK_PATH=$WASI_SDK python3 Tools/wasm/wasi configure-host --host-runner /bin/true -- --prefix=/sys
	WASI_SDK_PATH=$WASI_SDK python3 Tools/wasm/wasi make-host
	rm -rf "$stage"
	make -C cross-build/wasm32-wasip1 install DESTDIR="$stage"
	# what the package leaves out, gone from what is kept of the build too
	rm -rf "$stage/sys/lib/python$short/test"
fi

dst=$root/pkg/python/$version
rm -rf "$root/pkg/python"
mkdir -p "$dst/$OBJTYPE/bin" "$dst/sys/lib"
cp "$here/pkg.cfg" "$dst/pkg.cfg"
# the interpreter, without its debugging information
"$WASI_SDK/bin/llvm-strip" --strip-debug "$stage/sys/bin/python$short.wasm" -o "$dst/$OBJTYPE/bin/python3"
(cd "$stage/sys/lib" &&
 tar --exclude="python$short/test" --exclude="python$short/config-$short-*" --exclude=__pycache__ -cf - "python$short") |
	(cd "$dst/sys/lib" && tar -xf -)

# installed to the system, as `pkg install` writes it: the binds `pkgbinds`
# makes for this layout — `$objtype/bin` after `/bin`, and `sys` after
# `/sys`, and `sys/lib` after `/sys/lib`, which the root has too
echo "pkg=python version=$version" >>"$root/profile/pkg"
{
	echo "# pkg=python version=$version"
	echo "bind -a /pkg/python/$version/$OBJTYPE/bin /bin"
	echo "bind -a /pkg/python/$version/sys /sys"
	echo "bind -a /pkg/python/$version/sys/lib /sys/lib"
} >>"$root/profile/pkg.ns"
echo "pkg/python/mk.sh: python $version, $(du -sh "$dst" | cut -f1)"
