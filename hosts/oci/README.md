# hosts/oci — the `FROM scratch` container (implementation M1)

The first OCI weight (decision 2026-08-27): the macOS host's code cross-compiled
for `*-unknown-linux-musl`, statically linked, copied with the rootfs into an
empty image — the whole operating system as a distroless container. Cranelift
JITs normally inside a container; no distro, no libc, no init system beneath.
Acceptance: `docker run ipnx` prints the conformance floor and exits 0.
