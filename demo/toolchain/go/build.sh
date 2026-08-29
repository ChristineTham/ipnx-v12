#!/bin/sh
# Build the Go toolchain personality's artifacts into cache/ (gitignored):
# the real gc compiler, linker and gofmt cross-built to wasip1, plus the
# standard library's export archives for the package set DERIVED FROM
# gobyexample.com's imports — the measured benchmark (net/http is refused:
# +31MB for examples that cannot run without sockets; the /net milestone
# reopens it). Requires the host's go; the .a files must come from the same
# go version as the tools, so everything is built here together.
set -e
cd "$(dirname "$0")"
mkdir -p cache/pkg
export GOOS=wasip1 GOARCH=wasm

go build -o cache/compile.wasm cmd/compile
go build -o cache/link.wasm cmd/link
go build -o cache/gofmt.wasm cmd/gofmt
go version | awk '{print $3}' > cache/VERSION

# gobyexample's union of imports, sans net/http (measured 2026-08-29)
PKGS="fmt os errors strings strconv sort slices maps math math/rand time sync
sync/atomic regexp text/template encoding/json encoding/base64 encoding/xml
crypto/sha256 crypto/md5 net/url path/filepath io bufio unicode unicode/utf8
flag context log log/slog iter cmp bytes os/exec os/signal syscall io/fs embed"

go build std
echo "$PKGS" | tr '\n' ' ' | xargs go list -export -deps \
  -f '{{if .Export}}{{.ImportPath}}={{.Export}}{{end}}' | sort -u > cache/manifest
rm -rf cache/pkg cache/importcfg
mkdir -p cache/pkg
while IFS= read -r line; do
  imp=${line%%=*}; path=${line#*=}
  mkdir -p "cache/pkg/$(dirname "$imp")"
  cp "$path" "cache/pkg/$imp.a"
  echo "packagefile $imp=/go/pkg/$imp.a"
done < cache/manifest > cache/importcfg
echo "go cache: $(du -sh cache | awk '{print $1}') ($(wc -l < cache/importcfg | tr -d ' ') packages, $(cat cache/VERSION))"
