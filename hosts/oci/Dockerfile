# ipnx-v12 — the whole operating system as a distroless image (M1).
# The build context is staged by hosts/oci/build.sh (or CI): a static
# musl `host` binary and the shared rootfs. Nothing else: no distro,
# no libc, no init system beneath — the kernel is the process.
#
#   docker run --rm ipnx           the conformance suite, exit 0
#   docker run --rm -it ipnx -i    boot to rc on the console
FROM scratch
COPY host /host
COPY rootfs /rootfs
ENTRYPOINT ["/host", "/rootfs"]
