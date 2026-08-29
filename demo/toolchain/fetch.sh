#!/bin/sh
# Fetch the in-system C toolchain (network once, cached): clang and wasm-ld
# compiled to WASI plus a wasi sysroot, from binji/wasm-clang (LLVM,
# Apache-2.0 WITH LLVM-exception — LICENSE.llvm alongside).
set -e
cd "$(dirname "$0")"
mkdir -p cache
for f in clang lld sysroot.tar LICENSE.llvm; do
  [ -s "cache/$f" ] || curl -sL -o "cache/$f" "https://raw.githubusercontent.com/binji/wasm-clang/master/$f"
done
ls -la cache
