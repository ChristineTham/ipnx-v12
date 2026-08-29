# The implementation plan

**Living document** (begun 2026-08-29, the day the PoC was declared complete).
This is the build sequence for the real system. It consumes the
[decision log](design.md) — decisions are made there, with dates and
evidence, and *sequenced* here; nothing in this file reopens one. What the
system *is* — the invariants and contracts the milestones build against — is
[architecture.md](architecture.md); the working practice is
[handbook.md](handbook.md); the deployment forms and the namespace map are
[platforms.md](platforms.md). The PoC's record is frozen in
[poc.md](poc.md); findings continue to land in [RESEARCH.md](../RESEARCH.md)
as they are measured.

## What exists on day one

The inheritance is unusually strong for a "start of implementation":

- **The kernel, twice.** The Rust core (`kernel/`) — a pure state machine,
  syscalls in, effects out, its own 160-line async executor — passes the full
  conformance suite under wasmtime. The JS kernel (`poc/supervisor/`) passes
  the same suite on Node and in Chrome, and is now frozen as the **reference
  implementation**: the oracle a divergence is judged against.
- **The conformance suite as the contract.** 131 tests, run by pid 1 out of the
  rootfs. This is the project's executable spec: *a host is real when init
  exits 0*. The suite only grows.
- **The real userspace.** Plan 9's rc, sam, samterm, acme and twenty-four
  commands over `libp9.a` (~150 verbatim sources); the V10 exhibit on
  `libv10`; three WASI citizens (wasi-libc, Go `wasip1`, CPython 3.14). This
  is production code that happens to live under `poc/` — M0 moves it out.
- **The designs, decided.** The uid model running; the identity architecture
  (su, the user decomposition, the profile, the capability doctrine) recorded
  2026-08-29 at zero kernel cost, waiting on `/net` for its wire half.

## How the work is run

Moved where *how* belongs: the working rules — artifact-per-milestone, the
merge bar and the 131 floor, vendored-verbatim, the same-commit contract rule,
decisions consumed not re-derived — are [handbook.md](handbook.md#the-working-rules).
This document is sequence only.

## The tree

The repository converges on this shape (the 2026-08-29 scaffold did the first
move; M0 completes it):

```
kernel/          the Rust kernel core — the one kernel (moved from native/kernel)
hosts/
  macos/         wasmtime host, Cranelift; grows the app shell (from native/host)
  oci/           FROM-scratch container build (M1)
  ipados/        wasmtime host on Pulley + SwiftUI shell (M6)
  browser/       the Rust core compiled to wasm, JS embedding (M5)
userspace/       the guest world (graduated at M0): libc/ (lib9),
                 plan9/ + v10/ (vendored, verbatim), cmd/, wasi/, rootfs seed,
                 the build system (mk.sh and its tools)
docs/            design.md (why), architecture.md (what), handbook.md (how),
                 implementation.md (when — this), platforms.md (where),
                 poc.md (was), identity.md (who a user is), personas.md
                 (who it is for), syscalls.md (the call census)
poc/             FROZEN — the JS reference kernel and its two host shims.
                 Runs the floor suite forever; changes no more.
```

M0 landed 2026-08-29: the hosts consume `userspace/rootfs`, `userspace/mk.sh`
builds the guests, and `poc/` holds exactly what is frozen.

## Milestones

Sizes: **S** a session, **M** a few sessions, **L** a phase. Order is the
default path; dependencies are stated so opportunistic reordering stays honest.

### M0 — the tree completes *(S–M)* — **landed 2026-08-29**
Graduate the guest world out of `poc/`: `git mv` of `libc/ plan9/ v10/ cmd/
wasi/ rootfs/ mk.sh weaken.mjs` to `userspace/`, path fixes in `mk.sh`,
`run.sh`, `serve.mjs`, the hosts' default rootfs path, `.gitignore`, and the
docs' path references. `poc/` keeps only the frozen supervisor and its Node and
browser shims, consuming `userspace/`'s build products.
**Acceptance:** both kernels green from the new paths; `poc/run.sh` unchanged
in behaviour; a tree listing in the README's terms; a `VERSIONS` record of the
measured toolchain (wasi-sdk, binaryen, bison, Node, Rust) that `mk.sh` warns
against on drift (six-hats catch: the §9.4–9.5 findings are version-dependent
and the versions were recorded nowhere).

### M1 — the `FROM scratch` container *(S–M)* — consumes: OCI decision (2026-08-27)
The first OCI weight: cross-compile the macOS host's code for
`x86_64/aarch64-unknown-linux-musl` (wasmtime supports both; Cranelift JITs
fine inside a container), static binary, `FROM scratch`, `COPY` the host and
the rootfs. The whole operating system as a distroless image measured in
megabytes. **Acceptance:** `docker run ipnx` prints the floor suite and exits
0; `docker run -it ipnx -i` boots to rc; image size recorded in RESEARCH;
**CI on push** runs the suite in this container (the floor stops being a
manual discipline), and a second pinned-Node image preserves **the oracle in
amber** — the frozen reference guarded against host drift (both six-hats
catches).

### M2 — the namespace-file boot *(S)* — consumes: profile decision stage 1 (2026-08-29)
The decision log has said from the start that *boot is rc plus a namespace
file*; boot today is compiled C. Implement the namespace(6) little language
(subset: `bind`/`mount`/`cd`/`clear`, flags `-abcC`, `#`-device names,
comments), a `newns()` constructor in lib9, and shrink init to: build the
namespace from `/lib/namespace`, then run the suite or rc. This is also stage
one of the profile — the namespace fragment format *is* this file format.
**Acceptance:** boot namespaces identical before/after; a test rebinds via an
edited file with no recompile; init.c contains no hardcoded bind list.

### M3 — the macOS app *(M)* — consumes: GUI per-platform backing, native-host decisions
The native host is headless; `win acme` paints into memory nobody shows. Add a
presentation layer: one host window per `#w` window (rio's model), blitting
the existing backing store (the same bytes the raster tests read), injecting
mouse/keyboard through the same paths wctl tests use; wire `-i` to a real
terminal. Package as a `.app`. This makes the native host daily-drivable and
settles the presentation shape iPadOS inherits. (P1's journey pairs this
with M4 — a screen without persistence is a demo, not a home.)
**Acceptance:** the floor suite headless, unchanged; `win rc`, `win sam`,
`win acme` interactive on screen; a keystroke-to-glyph demo recorded.

### M4 — host storage *(M)* — consumes: storage decisions (2026-08-27)
Persistence: a file server over a host directory, mounted where the namespace
wants it — the rootfs becomes a real directory rather than a ramfs seed, a
container gets volumes, the profile gets a local tree that survives reboot.
Permission/uid mapping between host metadata and V10 enforcement is the design
question to settle *at this milestone* (sidecar vs mode-bit mapping).
**Acceptance:** boot from a host-directory root; a write survives restart;
V10 permission tests still enforce; the frozen reference still boots its ramfs.

### M5 — the browser host on the Rust core *(M)*
Compile `kernel/` to wasm (the core is single-threaded, no OS dependencies —
built for this), embed it in a JS shim structurally parallel to
`hosts/macos`: Workers as processes, the SAB mailbox, the existing browser
plumbing. The JS kernel then serves as oracle only.
**Acceptance:** the floor suite in Chrome on the Rust core; `?i` boots rc;
the window server drawing through the same `#w`; console windows text-mirror
into the DOM (the xterm.js path) so a screen reader can follow a shell —
accessibility as justice, not a feature ([virtue-ethics.md](virtue-ethics.md)).

### The public demo *(S, standalone)* — **landed 2026-08-29: [christham.net/ipnx-v12](https://christham.net/ipnx-v12/)**
GitHub Pages (her call — no new Netlify site), `gh-pages` branch assembled by
`demo/build.sh`; COOP/COEP supplied by our own COI service worker, injected
into the dist copy of the frozen browser page (the derivation-layer move;
self-skips where real headers exist). Verified live in a real Chrome — 132
PASS, 0 FAIL, `crossOriginIsolated` true. Field note: this workstation's
embedded browser pane refuses service-worker registration (measured over
HTTP/1.0 and 1.1), so live verification needs a real browser. P2/P3's
validation events are now armed. — *consumes: personas (2026-08-29)*
The browser port is finished, frozen, and unreachable — P2 and P3's whole
journey ([personas.md](personas.md)) is "click a URL, type into rc". Host it:
any static host that can set the COOP/COEP headers SharedArrayBuffer needs
(Netlify/Cloudflare Pages via a `_headers` file; GitHub Pages only with the
service-worker shim), the rootfs and binaries as static assets, a landing
line beside it — and **the tour**: an rc script in the rootfs (`tour` at the
demo prompt), its chapters doubling as P3's seed exercises, runnable
non-interactively by a self-skipping test. No kernel work.
**Acceptance:** the floor suite green at the public URL in a fresh browser;
`?i` boots rc; the README (hers) gains the link as a status fact; a NOTICES
page beside the landing states the third-party licence surface (PSF, Go's
BSD, the V10 covenant); the landing names which browsers are measured.

### The bench pass *(S, standalone — before CI trusts timing)*
Six-hats catch: zero runtime performance numbers exist — P4's "boots in
under a second" belief test has no baseline, and Pulley's slowdown is
flagged "measure before trusted" and unmeasured. A self-skipping measurement
boot prints syscall round-trip, pipe throughput, fork latency, boot-to-init
time, and per-process memory; numbers land in RESEARCH per host, and the
raster tests' polling margins are measured on slow hosts before CI treats a
timeout as a failure.
**Acceptance:** the table in RESEARCH, one row per host, dated.

### M6 — the iPadOS app *(M–L)* — consumes: engine matrix + iOS-files decisions (2026-08-27)
wasmtime with Pulley (the interpreter backend) and
`signals_based_traps(false)` — no JIT, no runtime codegen, App Store-lawful.
SwiftUI shell over M3's presentation shape; user-granted folders arrive as
security-scoped bookmarks surfaced as binds (the decision: a granted subtree
*is* a bind). Measure Pulley's slowdown honestly into RESEARCH. **Gate
(six-hats):** the WKWebView-stopgap claim rides on the browser port running
under WebKit — unverified; the suite runs under Safari before the stopgap is
trusted.
**Acceptance:** the floor suite green on an iPad (self-skipping what the
sandbox forbids, each skip named); acme editing a file in a granted folder.

### M7 — `/net` *(L)* — consumes: "sockets won" adoption (founding)
The network as files: `/net/tcp/clone`, per-connection `ctl`/`data`/`local`/
`remote`/`status`, `dial`/`announce`/`listen` in lib9, host sockets beneath
(browser: a WebSocket relay — an engineering question below). Then the step
that makes IPNX a distributed system: **exportfs over TCP** — one instance
mounts another's namespace across machines, per-attach identity already
stamped (identity.md).
**Acceptance:** two instances on one machine mount each other over TCP and run
a cross-instance pipeline; the BSD-API surface deferred to the personality
(M9) — `/net` itself is the kernel's whole contribution.

### M8 — identity on the wire *(M–L)* — consumes: the five 2026-08-29 identity decisions
The credential half, in the capability doctrine's terms: `devcap` minting
Amoeba-shaped sparse tickets (port/object/rights/check with a modern MAC —
boring, reviewed primitives only, the six-hats rule — expiry over
revocation); the auth agent (factotum's shape) holding keys as
use-don't-read files; `su`'s third rule — authenticated transition — landing
beside the two running ones; wire mounts authenticating by ticket.
**Acceptance:** a ticketed mount succeeds, an expired one refuses, a `su none`
shell cannot use another's agent; the audit view is the mount table.

### M9 — the profile, whole *(M)* — consumes: profile decision stage 2 (2026-08-29)
The portable person: a profile as a file tree (namespace fragments from M2,
service dials from M7, keys via M8's agent), a store that syncs devices (a git
repository is the recorded candidate), agent sub-profiles booting constrained
instances — "the kernel instance is the new uid," productised.
**Acceptance:** one profile boots the same working namespace on macOS and in
the container; an agent sub-profile boots visibly narrower; enrolment
documented as a user, not developer, procedure; **the five-line sandbox
quickstart ships as an artifact** — P4's belief test, made deliverable
([personas.md](personas.md)).

### M10 — the modern personality, then git *(L)* — consumes: benchmark discipline (founding), git deferral (2026-08-27)
`libunix`: the measured C surface — derived from what git, CPython and Go
actually call, never adopted from POSIX — as a personality libc over the
kernel: errno at the boundary, numeric uids via `/etc/passwd` (names stay
canonical), `/dev/tty` aliased, the BSD socket API over `/net` files. Then the
deferral resolves: **git is built under the personality** and becomes its
conformance test, the way acme was the GUI's.
**Acceptance:** `git init/add/commit/log/diff` in an IPNX shell on IPNX files;
every libunix entry point traceable to a benchmark's demand (the measurement
table in RESEARCH).

### M11 — the microVM *(L, research-first)* — consumes: OCI second weight (2026-08-27)
The second OCI weight and the hypervisor-direct aspiration: the host as PID 1
on a minimal Linux (Firecracker/cloud-hypervisor), or Virtualization.framework
on macOS — boot-to-shell fast enough to feel like a program, isolation strong
enough to be a tenant. A research spike sizes it before code.
**Acceptance (spike):** a written comparison with measurements; **(build):**
the floor suite inside a microVM, boot time recorded.

### M12 — the system in the world *(open-ended)* — consumes: cloud/AI aspirations (2026-08-27)
The tranche that turns deployments into products: S3-shaped storage as a file
server, lambda-shaped execution (a request is an exec in a fresh namespace),
Kubernetes hosting notes, and the agent sandbox — M9's sub-profiles as the
standing answer to "give an AI a computer without giving it the computer."
Sequenced by demand once M7–M9 exist.

### Continuous — curation sweeps *(S each, standing)*
The Plan 9 userland, command by command, PR-sized tranches as ever: `mk`, the
troff document factory, `cron`, upas (resequenced per decision), the games,
`/cc` as the compilation capability. These interleave with milestones freely —
they are how the system stays honest about running real software.

## Dependency spine

```
M0 ──► M1 ──► M11
 │
 ├──► M2 ──────────────┐
 ├──► M3 ──► M6        ├──► M9 ──► M12
 ├──► M4               │
 ├──► M5               │
 └──► M7 ──► M8 ───────┘
              └──► M10 (personality needs /net for sockets; git wants M4's disk)
```

M1–M5 are independent of each other after M0 and can be reordered by appetite;
M7 is the gate everything distributed waits behind.

## Engineering questions (not design questions — those live in design.md)

- **M1:** static musl wasmtime build flags; whether Cranelift's mmap'd code
  pages need a seccomp note in the image docs.
- **M2:** the exact namespace(6) subset grammar — what of `unmount`, `.`-rooted
  paths, environment substitution lands in v1.
- **M3:** the presentation stack (winit+softbuffer is the current default: no
  GPU dependency, the backing store is already CPU pixels); cursor/keyboard
  mapping tables.
- **M4:** uid/permission mapping onto host filesystems — sidecar metadata vs
  host-mode projection; crash-consistency posture.
- **M5:** whether guest Workers instantiate modules themselves (as today) or
  the core-in-wasm proxies instantiation; SAB ownership between two wasm
  instances.
- **M3/M5:** text-mirrored console windows (the xterm.js path) as the
  screen-reader answer — accessibility is unpromised before it exists.
- **M6:** Pulley's measured slowdown on the suite; whether CPython under
  Pulley is usable or needs the wasmi fallback from the engine matrix.
- **M7:** the browser's `/net` relay protocol (WebSocket framing of the wire);
  whether `announce` exists in a tab or only `dial`.
- **M8:** MAC choice and key rotation for tickets; ticket lifetime defaults.
- **M9:** profile-store merge semantics when two devices diverge (git's answer
  vs last-writer-wins per file).
- **M10:** the measurement harness that derives libunix's surface from the
  three benchmarks' link demands.
