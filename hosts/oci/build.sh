#!/bin/sh
# Stage the FROM scratch context and build the image (M1). The binary is the
# macOS host's code cross-compiled for linux-musl; wasmtime's build wants a
# musl C compiler for its signal helper, so on machines without one (this
# repo's Mac) the build runs in CI — .github/workflows/ci.yml is the same
# recipe and the acceptance gate.
set -e
cd "$(dirname "$0")/../.."
TARGET="${TARGET:-x86_64-unknown-linux-musl}"
cargo build --release -p host --target "$TARGET"
rm -rf hosts/oci/ctx
mkdir -p hosts/oci/ctx
cp "target/$TARGET/release/host" hosts/oci/ctx/host
cp -R userspace/rootfs hosts/oci/ctx/rootfs
if command -v docker >/dev/null 2>&1; then
  docker build -f hosts/oci/Dockerfile -t ipnx hosts/oci/ctx
  docker image inspect -f 'image: {{.Size}} bytes' ipnx
else
  echo "context staged at hosts/oci/ctx (no docker here — CI builds and runs it)"
fi
