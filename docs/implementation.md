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

### The package manager, v1 *(S)* — **landed 2026-08-29** — consumes: the package model (design.md 2026-08-29), the registry survey (RESEARCH)

`pkg(1)` in userspace: install/list/remove/verify; fetch over `#H`, sha256
always, unpack to `/pkg/<name>/<ver>`, bind per the package `meta`
(namespace-fragment dialect + `abi` + `env`). The demo gains a curated
same-origin `/registry/` (browser CORS reality, measured) with two or three
verified runtimes — acceptance: **`pkg install ruby` in the public demo's
tab, then `ruby -e` runs**; on the native host the same command against the
real WLR release URLs. A `libs/` package (zlib) lands as a layer-2 sysroot
and links by glob (`cc z.c /lib/wasm32-wasi/zlib/*.o`) — the porting
inversion made purchasable; the objects are era-matched because the registry
build compiles them with the tab's own cc (RESEARCH: WLR's prebuilt .a is
LLVM-16 objects the LLVM-8 linker rejects; a guest `ar` would restore
`-lz`). Suite: a
self-skipping test where `#H` exists. Feeds M10 (port personalities) and the
profile milestone (installs as namespace fragments).

### M1 — the `FROM scratch` container *(S–M)* — **landed 2026-08-29** — consumes: OCI decision (2026-08-27)
The first OCI weight: cross-compile the macOS host's code for
`x86_64/aarch64-unknown-linux-musl` (wasmtime supports both; Cranelift JITs
fine inside a container), static binary, `FROM scratch`, `COPY` the host and
the rootfs. The whole operating system as a distroless image measured in
megabytes. **Acceptance, all met 2026-08-29:** the suite runs in the container at
135 PASS and exits 0; `-i` boots to rc (both proven in CI — this Mac has no
container runtime, measured); the 62.2MB image size is in RESEARCH; **CI on
push** builds the world and proves it in the container and on the amber
oracle (Node pinned to VERSIONS' v22.23.2 — the amber's FIRST run caught an
arbitrary 22.12.0 pin lacking default try_table, validating the idea on day
one). amd64 is the built image; the aarch64-musl target compiles clean and
its image is a small follow-up. Both six-hats catches closed.

### M2 — the namespace-file boot *(S)* — **landed 2026-08-29** — consumes: profile decision stage 1 (2026-08-29)
The decision log has said from the start that *boot is rc plus a namespace
file*; boot today is compiled C. Implement the namespace(6) little language
(subset: `bind`/`mount`/`cd`/`clear`, flags `-abcC`, `#`-device names,
comments), a `newns()` constructor in lib9, and shrink init to: build the
namespace from `/lib/namespace`, then run the suite or rc. This is also stage
one of the profile — the namespace fragment format *is* this file format.
**Acceptance, all met 2026-08-29:** boot namespaces identical before/after
(the full 136 pass on all three hosts through the file boot — the Rust
kernel included, with zero Rust changes); the suite's newns test rebinds via
an edited file with no recompile; init.c carries no bind list — five lines
of C became five lines of text. `newns()` lives in lib9 (bind/mount/cd/
clear, `-abcC`, quoting, line-start comments; warn-and-continue so a boot
never wedges on one bad line), and `newns(1)` runs any command in a
file-built namespace — the profile's mechanism in miniature.

### M3 — the macOS app *(M)* — **landed 2026-08-30** — consumes: GUI per-platform backing, native-host decisions
The native host is headless; `win acme` paints into memory nobody shows. Add a
presentation layer: one host window per `#w` window (rio's model), blitting
the existing backing store (the same bytes the raster tests read), injecting
mouse/keyboard through the same paths wctl tests use; wire `-i` to a real
terminal. Package as a `.app`. This makes the native host daily-drivable and
settles the presentation shape iPadOS inherits. (P1's journey pairs this
with M4 — a screen without persistence is a demo, not a home.)
**Acceptance, met 2026-08-30:** the floor suite headless, unchanged (139);
`win rc` interactive on screen (typed into the app window, echoed through
the rootfs's own 9x18 subfont — the glass tty renders window-cons text over
the frame); acme runs in its own titled app window (label written by the
guest), proven twice — acmetest's full three-assert suite PASSES inside the
running app, and the window's raster censuses 6,296 ink pixels,
deterministic across runs (~90–150s to first paint: cranelift compiles the
20MB module at spawn); keystroke-to-glyph recorded (docs/img/m3-app.png).
The presentation layer is winit+softbuffer: one host window per `#w`
window, frames as Effect::WinUpdate at a 30Hz coalescing tick — the first
build flushed per draw-write and was measured at **640GB of queued frames**
during acme's boot (RESEARCH; the dirty-set tick is the cure, RSS flat at
~245MB under acme thereafter). Input goes back as Ev::{WinKey,WinMouse,
WinClose} with the demo's chords (option-left=2, cmd-left=3). `-w` boots
straight to a windowed shell; a terminal launch (isatty, or `--app -i`)
keeps a console rc beside the windows. hosts/macos/mkapp.sh packages
IPNX.app. Found and fixed on the way: win(1) now binds the window BEFORE
/dev, so the union keeps /dev/null (rc's `&` needs it).

### M4 — host storage *(M)* — **landed 2026-08-29 (v1)** — consumes: storage decisions (2026-08-27)
Persistence: a file server over a host directory, mounted where the namespace
wants it — the rootfs becomes a real directory rather than a ramfs seed, a
container gets volumes, the profile gets a local tree that survives reboot.
Permission/uid mapping between host metadata and V10 enforcement is the design
question to settle *at this milestone* (sidecar vs mode-bit mapping) —
settled for hostfs v1 (2026-08-29): mode bits map 1:1, files present as
eve's, sidecar deferred. The versioning layer's v1 landed
2026-08-30 as `#V` (design log; RESEARCH §9.8): epoch snapshots of the ram
root by structural clone (COW via shared buffers — 448 KiB and <50 ms per
whole-root snapshot, measured), read-only through the one `ram_access`
gate eve included, restore by `bind`, suite-tested on the native and demo
kernels (the oracle self-skips). Remaining here, sequenced: persistence —
the content-addressed store under hostfs — and the doctrine's asymptote,
per-write granularity; both slot in behind `#V` unchanged.
**Acceptance:** boot from a host-directory root; a write survives restart;
V10 permission tests still enforce; the frozen reference still boots its ramfs.

### M5 — the browser host on the Rust core *(M)* — **re-aimed 2026-08-30: lands `/dev/canvas`** — **landed 2026-08-30**
**The canvas decision and the userland reimagining (design.md 2026-08-30,
docs/userland.md) are this milestone's target**:
the browser host's presenter is the ONE universal SPA rendering the semantic
node tree (`stack·text·edit·image·path·frame`) to DOM and returning events;
the `events` file's resize/close protocol retires the demo's deferred
resize/close for real; v0 vocabulary is measured against sam-today,
acme-today, rio-today and one plot before code. The demo kernel prototypes
canvas first (demo-leads-discovery doctrine); the Mac app's presenter then
maps the same tree to CoreText/AppKit, and iPadOS inherits the shape.
Build order per userland.md: measure → console-today (the editable
transcript — the first canvas client; **AND, not XOR**, 2026-08-30: the
xterm byte console stays beside it) → acme-today absorbing sam's Edit language (one editor plus
a language; raster sam/acme retire to the exhibit with the suite floor
intact) → rio-today policy files. The plumber's rules and message shape
are designed inside the measurement pass — the look verb needs them.
**Progress 2026-08-30**: the measurement pass is written — docs/canvas.md
derives v0 from the four benchmarks (four kinds live: stack·text·edit·path;
`image` provisionally folds into `frame`; addr/data simplified to
`q0,q1`/`$` with the verbatim language recorded as sam-today's return) —
and **canvas v0 runs on the demo kernel**: the per-window `canvas/` tree
in devwsys, the virtual surface as the suite's user (`event` door, wctl
precedent), resize/close as protocol lines (the old deferral, retired at
last), and the SPA presenter's seed in the shell (stacks→flex, paths→SVG,
action attrs→real links and buttons, presenter-local echo on edit).
Verified end to end in a real browser: a tree echoed from rc rendered as
DOM, and a button click came back as `execute 3 0 7 Refresh` on the
events file. One suite test, pure rc — 146 on the demo kernel, 146 with
self-skips on native and the frozen oracle. `read(1)` landed beside it
(one read per invocation — the line-per-read device's natural client).
**Console-today landed 2026-08-30**: `con(1)` — the transcript is one
edit node, a mark separates history from the input region, Enter sends
the line; three libthread coroutines feed one canvas-writing consumer
(no locks, shadow state only, the transcript never re-read); the device
now applies user insert/delete events to node data before notifying (the
tree is the truth — presenter echo and buffer stay one thing); the
presenter gained Backspace. Suite-proven through the virtual surface
with output the typed text never contained — 147 on the demo kernel,
147 with self-skips on native and the oracle. Found on the way and kept:
the port's procexec now reports exec failure on fd 2 (a silent child
death proved undiagnosable). **acme-today's first slice landed the same
day as `edit(1)`** — tag verbs as real nodes (look re-reads, Put and Del
are honest buttons), the body shadowed from events, the file round-trip
suite-proven and verified by hand in the browser.
**M5 closed 2026-08-30, all four remaining slices in one round.** The
Edit language returned: `edit(1)` carries sam's core on the real
libregexp — `x/re/c|d` and `s/re/sub/[g]` with regsub's `&` and
backrefs, matches collected then applied back-to-front, suite-proven
end-to-file. rio-today's proof is the doctrine made ridiculous-honest:
`/rc/tile` is **a window manager in a dozen lines of rc** (the window
server's root now lists windows; policy is a loop writing wctl files).
The Rust kernel reached full canvas parity — device, virtual surface,
event application, resize/close pushes — so the canvas, console, edit
and rio tests all run natively: **149 / 149 / 149**, self-skips now
belonging to the frozen oracle alone, forever by design. And the Mac
presenter's v0 maps the same tree to native drawing: stacks lay out,
text and edit nodes render through the k1 subfont, action nodes are
bordered hit-regions on the input convention's verbs, keys make
insert/delete events with local echo — verified by window capture:
con(1)'s transcript, typed command and output, in a real macOS window
from the same files the browser renders as DOM. One tree, two
presenters, demonstrated.
**Post-landing alignment (same day, her question "do we need to redo
M3?")**: canvas syncs now travel ONLY through M3's credit system —
sync marks the window dirty, the tick flushes the LATEST tree when
credit allows, the UI acks after paint. Snapshots built at flush time
coalesce by construction (intermediate trees never travel), so the
credit law — coalescing changes the rate, only credit changes the
bound — holds for semantic frames exactly as for raster ones. A
canvas-first window is created by the UI on its first flush, as
Update would. Verified: 149 native, and con's transcript re-captured
identical through the credited path. M4 needed nothing (measured:
edit's Put lands on #Z's real disk; #V coexists untouched — canvas is
display, files are the persistent objects, and the namespace already
serves both).
**And the clipboard is a file (same day, her confirmation and her
framing)**: `/dev/snarf` landed at `#w/snarf` on every running kernel —
on macOS it IS the pasteboard (pbcopy/pbpaste through the effect loop),
in the browser writes push and reads pull through
`navigator.clipboard` where the platform permits (Chrome after one
grant; Safari's gesture-only reads degrade to the kernel buffer), and
the editor's ⌘C/⌘X/⌘V mirror every gesture into it — guest↔host
clipboard sync, "what Parallels and other hypervisors do," her words,
now a file any program can cat. One rc subtlety recorded from the
suite work: a failed redirection aborts the command WITHOUT setting
$status, so probes must probe by read, not by write-status.
**Recorded deferrals leaving M5** *(revised by the acme fidelity pass,
2026-08-30 — the paper's examples as yardstick; design.md logs the
decision)*: `path` nodes not yet drawn on the Mac surface; the MAC
presenter's editing stays append/backspace v0 (the browser presenter
now carries full editing, selection reporting and app-steered `sel`);
the plumber's rules and message shape (narrowed: look now resolves
files, directories and sam addresses app-side and delegates through
acme's own event file — what remains is the plumber as a separate
program with rules); canvas on iPad is M6's bridge and
VSCode panels M13's; the original "browser host on the Rust core" aim
(the Rust kernel compiled to wasm for the page) moves to the browser
host's own line in platforms.md, unblocked but unscheduled — and the
question "why can't the demo be the browser surface" (hers,
2026-08-30) is answered on the record: it IS, structurally — and the first of the
two named steps landed the same day, on her directive ("convert the
demo to the browser surface… swap out ramfs with a real namespace"):
the presenter is factored into `demo/shell/presenter.mjs`, a
standalone artifact (createCanvasView: snapshots in, event lines out,
rAF-coalesced) with the shell as its first client — and the namespace
became a user CHOICE of three homes, her design: play (ramfs +
examples), browser storage (OPFS), or a real local folder (File
System Access) — one `#Z` hostfs device over FileSystemDirectoryHandle
serving both persistent modes, since OPFS and picked folders speak
the identical API. A granted directory IS a bind, demonstrated: a
file written through the namespace survived a full page reload. The
remaining step — the Rust core beneath the page — stays unblocked,
unscheduled. The xterm
console stays beside con per AND-not-XOR.
Compile `kernel/` to wasm (the core is single-threaded, no OS dependencies —
built for this), embed it in a JS shim structurally parallel to
`hosts/macos`: Workers as processes, the SAB mailbox, the existing browser
plumbing. The JS kernel then serves as oracle only.
**Acceptance:** the floor suite in Chrome on the Rust core; `?i` boots rc;
the window server drawing through the same `#w`; console windows text-mirror
into the DOM (the xterm.js path) so a screen reader can follow a shell —
accessibility as justice, not a feature ([virtue-ethics.md](virtue-ethics.md)).

### The public demo *(S, standalone)* — **landed 2026-08-29; the desktop shell same day: [christham.net/ipnx-v12](https://christham.net/ipnx-v12/)**
**v2, after the author's field review** ("not impressive" — the first live P1
evidence): `demo/shell/` is the browser host's presentation layer ahead of
M5 — xterm.js character windows typed in-window (the component design.md
always named), macOS window chrome (close/drag/resize/zoom), a desktop with
a menu bar, draw windows at 1.5× crisp scale. **v3 same night**: the real Plan 9 fixed 9x18
face in the editors (subfonts converted to uncompressed k1 at vendor time —
the frozen engine has no compressed-'Y' op; the suite keeps the small default
font, the demo launches with `-f`/`$font`), and the three-button answer:
plan9port's convention (⌥-click=2, ⌘/ctrl-click=3, right-click=3) plus a
1|2|3 titlebar switch for touch. **The WASI interop proof is on stage (2026-08-29 late)**: the tour runs
wasitest and the real Go binary live; CPython runs by invitation
(`python /tmp/pytest.py`); and the landing's second button boots **the C
toolchain** — real clang + wasm-ld as guests (RESEARCH §9.7: the
wasi_unstable adapter and the inode-dedup finding). **The toolchains are
real (2026-08-29, the shims-are-over decision, design.md): `cc(1)` is a
genuine driver (flags, -o, -c, -E, -S, multiple files) over clang and
wasm-ld; `go` is a genuine driver (build/run/fmt/version/env) over the REAL
gc compiler and linker cross-built to wasip1 — the folklore's "Go cannot
compile on wasm" fell to ipnx's process model (RESEARCH §9.7) — with the
gobyexample-derived stdlib export set at /go/pkg; `pip` installs real
pure-Python wheels from the real PyPI over `#H` (webfs — the network as
files, /net's forerunner), sha256-verified, on the full 529-file stdlib.
Acceptance was external: python.org's example programs and gobyexample's
run as written.** `demo/supervisor` is the demo's own kernel lineage
(forked 2026-08-29; the oracle stays untouched); still queued there:
**audit notes, all closed 2026-08-30:** pkg's remove path now streams (no
package-count cap); the `mount` verb is suite-exercised (a namespace file
mounts a 9P server via /srv — 141); the app places windows by the kernel's
cascade; and the pid-1 oddity was chased to a script(1) pty artifact — both
the typed-exit and stdin-EOF shutdown paths verified clean. The aarch64
image gained its CI job (cross-built musl host, qemu smoke boot; the full
suite stays on amd64). The 2026-08-30 engineering round closed the rest:
the versioning layer's v1 (`#V`, above), `ar(1)` on the index-less-archive
measurement (RESEARCH §9.8), and identity.md's D1–D4 confirmed
dispositioned (D1/D3/D4 closed with `../ipnx` provenance; D2's groups ride
a later milestone by design). Shared guest memory stays a *deliberate*
optimisation deferral (RESEARCH §5.3): unsharing keeps guest builds free
of atomics/shared-memory flags on every host, and no transport cost has
yet shown in a profile — the recorded trigger for revisiting is a measured
profile where syscall copy time dominates, not a hunch. Still queued:
window-refresh events for TRUE guest resize and clean window-close
semantics — judged by the same 132 plus its own tests. The modern-draw question stays queued in design.md.
GitHub Pages (her call — no new Netlify site), `gh-pages` branch assembled by
`demo/build.sh`; COOP/COEP supplied by our own COI service worker, injected
into the dist copy of the frozen browser page (the derivation-layer move;
self-skips where real headers exist). Verified live in a real Chrome — 132
PASS, 0 FAIL, `crossOriginIsolated` true. Field notes: this workstation's
embedded browser pane refuses service-worker registration (measured over
HTTP/1.0 and 1.1), so live verification needs a real browser; and the Safari story,
reduced to the bottom through two on-the-record corrections: the shim DOES
isolate WebKit (our register script's reload race was bug one), the system
is fully WebKit-clean (132/132 with real headers), and the last wall proved
to be a **deterministic WebKit defect** — concurrent module-worker script
loads through a service worker fail (second-in-flight always dies; minimal
repro in `demo/webkit-repro/`; **filed as
[WebKit bug 322883](https://bugs.webkit.org/show_bug.cgi?id=322883)**). The bundle serialises worker startup as the workaround,
and **Safari runs the full 132 on this host** — verified live. The lesson
is in the handbook: reduce before blaming upstream. P2/P3's
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

### M6 — the iPadOS app *(S–M)* — **re-aimed 2026-08-30: a WebKit launcher over local files** — consumes: the iPad-surface decision (design.md 2026-08-30), M4 (files), the browser port
The stopgap becomes the design, with the split stated precisely (design
log): **the webview is the engine room, not the display.** Kernel and
wasm binaries run inside WKWebView — full JavaScriptCore JIT, sanctioned,
in Apple's own content process; a WKURLSchemeHandler serves the bundled
dist with **real COOP/COEP headers** (no service worker, no register
race; retake the module-worker-serialisation measurement in-app — the
recorded defect was specific to loads through a service worker). First
boot is offline. `/dev/canvas` crosses the script-message bridge to a
**native SwiftUI presenter** (M5's tree, Apple's grammar per the input
convention — semantic trees are small on a bridge where raster frames
were the 640GB lesson). The app's file entitlements serve the local
filesystem inward to hostfs over the same bridge — a security-scoped
bookmark is a bind. Pulley (the prior plan) demotes to recorded fallback
research per the engine-matrix decision.
**Acceptance:** the app boots the bundled port offline and the suite runs
green in-app with real headers; a granted folder reads and writes
through the namespace; a canvas tree renders through SwiftUI and its
events land back in the kernel; the WebKit measurement is retaken and
recorded.

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

### M10 — the modern personality, then git; and the first port personality *(L)* — consumes: benchmark discipline (founding), git deferral (2026-08-27), the porting inversion (2026-08-29)
`libunix`: the measured C surface — derived from what git, CPython and Go
actually call, never adopted from POSIX — as a personality libc over the
kernel: errno at the boundary, numeric uids via `/etc/passwd` (names stay
canonical), `/dev/tty` aliased, the BSD socket API over `/net` files. Then the
deferral resolves: **git is built under the personality** and becomes its
conformance test, the way acme was the GUI's.
**Acceptance:** `git init/add/commit/log/diff` in an IPNX shell on IPNX files;
every libunix entry point traceable to a benchmark's demand (the measurement
table in RESEARCH). **And the porting inversion becomes concrete here**: git
builds against libunix if the measured surface suffices, but the general
mechanism is a *port personality* — a foreign `libc.a`+headers environment
(musl-shaped first, since the wasi-libc sysroot already in the demo is the
seed) that compiles unmodified upstream source. The acceptance widens over
time from "git builds" to "an unmodified upstream package builds against a
port personality with no source edits."

### M11 — the microVM *(L, research-first)* — consumes: OCI second weight (2026-08-27)
The second OCI weight and the hypervisor-direct aspiration: the host as PID 1
on a minimal Linux (Firecracker/cloud-hypervisor), or Virtualization.framework
on macOS — boot-to-shell fast enough to feel like a program, isolation strong
enough to be a tenant. A research spike sizes it before code.
**Acceptance (spike):** a written comparison with measurements; **(build):**
the floor suite inside a microVM, boot time recorded.

### M12 — the system in the world *(open-ended)* — **re-aimed 2026-08-30: the orchestration suite** — consumes: cloud/AI aspirations (2026-08-27), the third dissolution + orchestration plan (design.md 2026-08-30)
The spine is now the **process orchestration suite** (design.md 2026-08-30:
"a Dockerfile is a process file, the orchestrator is a file server, kubectl
is `cat` and `echo`"): the spec directory (`namespace`/`packages`/`user`/
`env`/`cmd`/`replicas`/`health`), `run(1)` as docker run (~100 lines,
nothing missing after M4 — the **local stage may interleave early**, it is
pure userspace), `svc(4)` as the control plane file server (desired state
written, observation read, a Service is a `/srv` post, a load balancer is
a 9P multiplexer), and the **cluster stage** after M7+M8 — remote
instantiation on `cpu(1)`'s precedent, a cluster's control plane as
`bind -a` over kernels' `/svc` trees, scheduling as userspace policy.
Lambda-shaped execution collapses into `run` of a spec in a fresh
namespace; S3-shaped storage as a file server and the agent sandbox (M9's
sub-profiles as "give an AI a computer without giving it the computer")
remain in the tranche. The tripwire travels: a spec dialect grown past its
few files means we have rebuilt YAML Kubernetes, and we stop.
**Acceptance (local): landed 2026-08-30** — the spec directory
instantiates namespace, env, credential and cmd (`run(1)`); `svc(4)`
holds replicas at N through kills (reconcile-on-read: reading status IS
the probe), scales down by note, drains on stop; `sleep`/`kill`/`ps`/
`mount`/`unmount` landed beside them; two suite tests, **145 on all
three hosts including the frozen oracle — zero kernel changes**, the
compensation thesis demonstrated. Deferred within the local stage,
honestly: per-service `log` collection, the `health` file's use by the
reconciler (liveness today is /proc walkability), and the LB — a posted
service is one name but does not yet fan attaches across replicas.
**(cluster):** one `svc` tree spans two kernels over TCP and a spec runs
remotely with the caller's namespace imported — judged by the suite like
everything else. **The distributed-OS stretch (design.md 2026-08-30)
aims this stage further**: rehosting as the orchestrator's normal move
(kill-here, run-there — the spec re-applies), live migration as the fork
snapshot shipped across the wire (the fd half is the named engineering:
wire mounts re-dial, pipes proxy or drain), discovery as mounted
neighbourhood directories in cs/ndb's lineage ("what can you do" is
`ls`), and the spanning identity riding M8's tickets and M9's profile.
The consensus debt stands unwaived.
Sequenced by demand once M7–M9 exist.

### M13 — the VSCode surface *(S–M, interleavable)* — consumes: kernel-as-a-library (the JS twin), the orchestration local stage; canvas panels consume M5
The namespace surface (design.md 2026-08-30): an extension boots
`kernel.mjs` inside the extension host (it is Node; no transport) and
serves the namespace through FileSystemProvider — stat/readDirectory/
readFile/writeFile/delete onto walks, rename onto `wstat`; terminals are
Pseudoterminal over `/dev/cons`; tasks run process files; Remote-IPNX
where devcontainer.json needed Docker. Watch is the named question (9P
has no notification: poll, or a synthetic event file). vscode.dev's
web-worker constraints get measured before that form is promised.
**Acceptance:** an IPNX namespace opens as a workspace folder — edit,
save, `run` a spec from a task, rc in the integrated terminal; the suite
drives the same kernel the editor mounts.

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
