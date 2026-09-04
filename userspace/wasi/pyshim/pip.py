# pip: package installation for ipnx's Python personality.
#
# Installed real packages from the real PyPI over the kernel's '#H' device.
# THAT DEVICE IS GONE (P1 step 5, 2026-09-04): fetching is not the kernel's
# job, and Plan 9 answers it with a USERSPACE webfs (sys/src/cmd/webfs), which
# nobody has written here yet. So pip has no network and says so. Everything
# below — the sha256 verification, the wheel unpacking — is intact and waits
# on a fetch(). Wheels were verified against PyPI's sha256 before unpacking
# into site-packages, and will be again.
#
# Scope, honestly: pure-Python wheels (py3-none-any). Native-code wheels need
# a compiled extension for this platform, and stock CPython-wasi loads no
# shared objects — a measured personality boundary, not a bug. Upstream pip
# itself cannot run here yet: it insists on ssl+socket at the C level, which
# this CPython build omits; when /net lands with real sockets, upstream pip
# becomes the target and this program retires.

import json
import re
import struct
import sys
import zlib
from binascii import crc32
from pathlib import Path

SITE = Path("/lib/python3.14/site-packages")


def fetch(url):
    # '#H' left the kernel in P1 step 5 — fetching is not the kernel's job, and
    # Plan 9 answers it with a userspace webfs (sys/src/cmd/webfs). Until one
    # exists here, pip has no network. See docs/implementation.md P1 step 5.
    raise OSError(
        "pip: no network — '#H' left the kernel and no userspace webfs exists yet"
    )


def norm(name):
    return re.sub(r"[-_.]+", "-", name).lower()


def pick_wheel(files):
    for f in files:
        n = f["filename"]
        if n.endswith(".whl") and ("py3-none-any" in n or "py2.py3-none-any" in n):
            return f
    return None


def installed():
    out = {}
    if SITE.is_dir():
        for d in SITE.iterdir():
            if d.name.endswith(".dist-info") and "-" in d.name:
                name, _, ver = d.name[:-10].rpartition("-")
                out[norm(name)] = ver
    return out


def unwheel(body):
    """Parse the wheel (a zip) by its central directory; inflate via the
    personality's pure-Python zlib. Returns {name: bytes}."""
    eocd = body.rfind(b"PK\x05\x06", max(0, len(body) - 65558))
    if eocd < 0:
        sys.exit("ERROR: not a zip (no end-of-central-directory)")
    _, _, _, _, count, cdsize, cdofs, _ = struct.unpack("<4sHHHHIIH", body[eocd:eocd + 22])
    files, pos = {}, cdofs
    for _ in range(count):
        (sig, _, _, _, method, _, _, crc, csize, usize,
         nlen, elen, clen, _, _, _, lofs) = struct.unpack("<4s6H3I5H2I", body[pos:pos + 46])
        if sig != b"PK\x01\x02":
            sys.exit("ERROR: bad central directory")
        name = body[pos + 46:pos + 46 + nlen].decode()
        pos += 46 + nlen + elen + clen
        lnlen, lelen = struct.unpack("<HH", body[lofs + 26:lofs + 30])
        data = body[lofs + 30 + lnlen + lelen:][:csize]
        if name.startswith("/") or ".." in name.split("/"):
            continue                              # zip-slip guard
        if name.endswith("/"):
            continue
        raw = data if method == 0 else zlib.decompress(data, -15) if method == 8 else None
        if raw is None:
            sys.exit(f"ERROR: {name}: unsupported compression method {method}")
        if crc32(raw) & 0xFFFFFFFF != crc:
            sys.exit(f"ERROR: {name}: crc mismatch in wheel")
        files[name] = raw
    return files


def requires(files, distinfo):
    deps = []
    meta = files.get(distinfo + "/METADATA")
    if meta is None:
        return deps
    for line in meta.decode(errors="replace").splitlines():
        if not line.startswith("Requires-Dist:"):
            continue
        req = line.split(":", 1)[1].strip()
        if ";" in req:            # environment markers (extras etc.) — skipped
            continue
        m = re.match(r"[A-Za-z0-9._-]+", req)
        if m:
            deps.append(m.group(0))
    return deps


def install(name, want_ver=None, seen=None):
    seen = seen if seen is not None else set()
    key = norm(name)
    if key in seen:
        return
    seen.add(key)
    have = installed()
    if key in have and want_ver in (None, have[key]):
        print(f"Requirement already satisfied: {name} ({have[key]})")
        return
    print(f"Collecting {name}")
    data = json.loads(fetch(f"https://pypi.org/pypi/{key}/json"))
    if want_ver:
        files = data.get("releases", {}).get(want_ver)
        if not files:
            sys.exit(f"ERROR: no release {want_ver} of {name}")
        ver = want_ver
    else:
        files, ver = data["urls"], data["info"]["version"]
    wheel = pick_wheel(files)
    if wheel is None:
        sys.exit(
            f"ERROR: {name} {ver} has no pure-Python wheel — it needs native code,\n"
            "which this platform's Python cannot load (no shared objects on wasm)."
        )
    print(f"  Downloading {wheel['filename']} ({wheel['size'] // 1024} kB)")
    body = fetch(wheel["url"])
    want = wheel.get("digests", {}).get("sha256")
    if want:
        import hashlib
        got = hashlib.sha256(body).hexdigest()
        if got != want:
            sys.exit(f"ERROR: sha256 mismatch for {wheel['filename']}")
    files = unwheel(body)
    distinfo = next((n.split("/")[0] for n in files if n.endswith(".dist-info/METADATA")), None)
    deps = requires(files, distinfo) if distinfo else []
    SITE.mkdir(parents=True, exist_ok=True)
    for fn, raw in files.items():
        dst = SITE / fn
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_bytes(raw)
    print(f"Installing collected packages: {name}")
    print(f"Successfully installed {name}-{ver}")
    for dep in deps:
        install(dep, None, seen)


def uninstall(name):
    key = norm(name)
    for d in SITE.iterdir() if SITE.is_dir() else []:
        if d.name.endswith(".dist-info") and norm(d.name[:-10].rpartition("-")[0]) == key:
            record = d / "RECORD"
            files = []
            if record.is_file():
                for line in record.read_text().splitlines():
                    p = line.split(",")[0]
                    if p:
                        files.append(SITE / p)
            for f in files:
                try:
                    f.unlink()
                except OSError:
                    pass
            for f in sorted({p.parent for p in files}, key=lambda p: -len(p.parts)):
                try:
                    f.rmdir()
                except OSError:
                    pass
            print(f"Successfully uninstalled {d.name[:-10]}")
            return
    sys.exit(f"WARNING: {name} is not installed")


def main(argv):
    if not argv or argv[0] in ("-h", "--help", "help"):
        print("pip <command> — ipnx's package installer (real PyPI, pure-Python wheels)\n")
        print("  install <name>[==ver]...   download, verify, unpack into site-packages")
        print("  uninstall <name>           remove by RECORD")
        print("  list                       installed distributions")
        print("  show <name>                metadata")
        return
    if argv[0] == "--version":
        print("pip (ipnx) for " + sys.version.split()[0])
        return
    cmd, args = argv[0], argv[1:]
    if cmd == "install":
        if not args:
            sys.exit("ERROR: pip install needs at least one package name")
        for spec in args:
            name, _, ver = spec.partition("==")
            install(name, ver or None)
    elif cmd == "uninstall":
        for name in args:
            uninstall(name)
    elif cmd == "list":
        have = installed()
        if have:
            w = max(len(n) for n in have) + 2
            print("Package".ljust(w) + "Version")
            print("-" * (w - 2) + "  " + "-------")
            for n, v in sorted(have.items()):
                print(n.ljust(w) + v)
    elif cmd == "show":
        for name in args:
            for d in SITE.iterdir() if SITE.is_dir() else []:
                if d.name.endswith(".dist-info") and norm(d.name[:-10].rpartition("-")[0]) == norm(name):
                    meta = (d / "METADATA").read_text(errors="replace")
                    for line in meta.splitlines():
                        if not line.strip():
                            break
                        print(line)
    else:
        sys.exit(f"ERROR: unknown command {cmd!r} (try pip --help)")


if __name__ == "__main__":
    main(sys.argv[1:])
