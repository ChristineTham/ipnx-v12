> **ARCHIVED 2026-09-04. NOTHING HERE IS CURRENT.** This is the plan as it
> stood from 2026-08-29 to 2026-09-03 — M0 through M18, grown by accretion:
> each design decision that invalidated an earlier milestone got a *new*
> milestone appended rather than the earlier one redone. Superseded in full by
> [../implementation.md](../implementation.md), which plans the same system in
> three layers with the demo as a milestone. Kept because the measurements and
> the reasoning inside the milestones are still good; the *sequence* is not.

# The implementation plan

**Role: the *how*.** The plan — milestones, dependencies, acceptance
criteria, and the sequence work follows. What is *built* is
[when.md](../when.md).

**Living document** (begun 2026-08-29, the day the PoC was declared complete).
This is the build sequence for the real system. It consumes the
[decision log](../design.md) — decisions are made there, with dates and
evidence, and *sequenced* here; nothing in this file reopens one. What the
system *is* — the invariants and contracts the milestones build against — is
[architecture.md](../architecture.md); the working practice is
[handbook.md](../handbook.md); the deployment forms and the namespace map are
[platforms.md](../platforms.md). The PoC's record is frozen in
[poc.md](../poc.md); findings continue to land in [RESEARCH.md](../../RESEARCH.md)
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
decisions consumed not re-derived — are [handbook.md](../handbook.md#the-working-rules).
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
(the Rust kernel compiled to wasm for the page) moved to the browser
host's own line in platforms.md, unblocked but unscheduled — **and
landed 2026-08-31, on her directive** ("replacing the demo with the
browser surface"): `hosts/browser/` compiles the untouched `kernel/`
crate to wasm32-unknown-unknown behind a C ABI (574KB), and
`demo/supervisor/rustkern.mjs` is the mach layer — the same boot()
interface kernel.mjs served, the same Workers, mailboxes and
guestcore.mjs (the guest world cannot tell the kernels apart), with
entries mirroring the macOS host's Ev and one binary drain mirroring
run_effects. Landing it made the kernel literally OS-free: '#Z' now
delegates every host-file operation as an effect answered by the
embedding (hostop_done — webfs's fetch_done pattern applied to the
filesystem), so hosts/macos gained the op server the kernel lost, the
browser serves OPFS and picked folders through the same protocol, and
the Node harness (main-rust.mjs) serves node:fs — 151 green on all of
them, plus 151 in Chrome, `cc hello.c` to Hello Kitty on the Rust
core in the tab, and a file written to browser storage surviving a
full reload. The demo's JS kernel lineage retired to
reference-in-tree; the frozen oracle stands where it always did — and the
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
accessibility as justice, not a feature ([virtue-ethics.md](../virtue-ethics.md)).

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
journey ([personas.md](../personas.md)) is "click a URL, type into rc". Host it:
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

### M6 — the iPadOS app *(S–M)* — **re-aimed 2026-08-30: a WebKit launcher over local files; re-aimed again 2026-08-31: the app is a SURFACE OF EMCA, not a host that runs a demo — the SwiftUI presenter is M14g** — consumes: the emca decision (design.md 2026-08-31), the iPad-surface decision (design.md 2026-08-30), M4 (files), the browser port
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
([personas.md](../personas.md)).

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

### M14 — emca, the user interface *(L, staged)* — **PARTLY SUPERSEDED 2026-09-01 by M15, the compositor: a–d built emca's surface on three hardcoded named regions, and the layout half of that work is replaced. What survives is the control interface (`/dev/window`), `/type` as a registry, the editor component, content over 9P, and emca as a watching file server. What does not is every part that assumed named panes.** — sequenced immediately after M5; M6 and M13 become its surfaces — consumes: the emca decision (design.md 2026-08-31), M5 (`/dev/canvas`), `/pkg`
The system's face (design: [emca.md](../emca.md); parts list:
[acme.md](../acme.md)). Not an editor — what IPNX *boots into* on every
surface, with an editor as one window type. Split in two throughout:
**IPNX owns state, meaning and policy; the surface owns rendering and
input**, and the test for any case is *differs between a Mac and an
iPad → the surface's; differs between one workspace and another →
IPNX's*. Staged so each stage is separately demonstrable and the suite
grows with it.

**a. The channels *(M — restructured 2026-08-31; the DEVICE HALF LANDED the same day)*.** Not additions to
`/dev/canvas` — that was the wrong shape (decision log; the framing
error was calling canvas "the display protocol" when 9P is the only
protocol). **Four semantics over one protocol**: content is **9P
directly** and the host renders it natively (**IPNX implements no
renderers**); **`/dev/window/<type>/<n>`** is the bidirectional control
interface with the **type in the path** ([window.md](../window.md));
**`/type`** is the registry both sides read; **`/dev/canvas` narrows to
genuine drawing** ([canvas.md](../canvas.md), narrowed in place with its
over-derivation stated — three of its four founding benchmarks were
never drawing consumers). Controls name a side: `ipnx:Put` round-trips,
`host:toggle-wrap` never does. This stage is *narrow canvas, specify and
build `/dev/window`* — and it **deletes** far more than it adds.
**Landed 2026-08-31**: `#w` grew the control interface — root `clone` and parked
`events`, a type-indexed path (`#w/<type>/<n>`, validated, with `#w/<n>` untouched
so nothing broke), and per-window `content`/`toolbar`/`tag`/`ui`/`events`. Proven
headlessly by one suite test in rc — *a UI you can cat, echo and grep* — at **152
PASS / 0 FAIL** on the Rust host, self-skipping on the frozen oracle. **And the host half's chrome
landed the same day**: `WinChrome` crosses as an ordinary effect, the browser
shell renders native furniture (content path, real buttons, tag field), a click
returns through `bh_win_event`, and `host:` actions never leave the surface —
proved headlessly by `node demo/supervisor/winproof.mjs userspace/rootfs`, **with
no emca in the loop**. **And the content half the same day**: the surface opens the file
itself (`readPath` → `Effect::ReadDone`) and **the browser renders it with its
own engines** — SVG as SVG, images as images, HTML sandboxed, else text. *IPNX
implements no renderers* is now a fact about the code. In-process this is a
function call, per *"wire 9P at boundaries, a Dev table inside"*; a remote
surface (M12) marshals. **And the editor component the same day**, which
retires the spike: CodeMirror 6 vendored offline, the buffer mirrored as
insert/delete with a sequence and a hash, `⌘Z` routed to emca's single stack,
`⌘S` streaming the file back — and the verbs riding the *platform's* context
menu, observed putting `look 29 36 surface` where a plain guest read it.
**And the SURFACE ITSELF, the same day**: the demo no longer runs a
floating-window desktop with a menu bar — it **is** emca. Full viewport, a top
toolbar carrying the system's managers (each only a typed window onto a
filesystem), a rail, an editor area with **tabs**, a console pane, a status
line with the global tag, and `⌘B`/`⌘J` pane toggles that never round-trip.
Placement is by TYPE, and with **no emca program running** the surface falls
back to each type's default pane *(RETIRED 2026-09-02 — panes are gone and
the real degradation is better: no window manager, no windows, with the devices
going straight to the screen)* — the recorded "degrades correctly" property,
in code. The default workspace is `/rc/emca`, two lines of rc, which is emca's
policy exactly where policy belongs. **And `/type` landed as a real registry**: four small
files per type (`ns`, optional `cmd`, `window`, `pane`), read by BOTH halves —
`/rc/emcaopen` knows nothing about any type, and `dir` is the only placement
compiled into the shell. Twelve types ship, `/type` is itself a type, and
clicking Processes opens a live `/proc` with no process-manager program in
existence. **Still unbuilt after this stage**: the responsive breakpoints and
canvas's narrowing. (emca as a watching program landed the same day — M14c.) *(The
registry's shape has moved on since this dated entry: `cmd` became `manager`
and `pane` retired when M15 gave the compositor its own allocation. The
present contract is in [window.md](../window.md), the design in
[type.md](../type.md).)*

**b. `/type` and the manager types *(M)*.** The registry: a directory
per type holding `ns`, optional `cmd`, and `window` — house style, small
files, so each field is separately editable, greppable and bindable, and
a personal override is a union element. One built-in type (`dir`) is the
bootstrap floor, because `/type` is itself a type. The starting set is
emca.md's table — `dir file errors proc usr pkg project net type
shell credential` — each at most three small files. **This stage is
where "no manager programs" becomes true**: `/proc` gets a process
manager without a process manager being written.

**c. emca, the IPNX half *(M — was L; shrunk 2026-08-31)*.** Editing is
the surface's (decision log), so this stage is a **file server with a
workspace, not an editor**: push text into windows, run commands, apply
**sam's structural language**, and serve the file interface. The
interactive editing machinery is not written here — it is inherited
from the host's editor component. The program: buffers and windows (N
windows may view one buffer, so `Zerox` aliases and its recorded
divergence retires), the tag as one string with the auto-block preserved,
context resolution, the verb set, the running-command table as a view of
`/proc`, and the file interface (landed 2026-09-03 as M17a1, at
`/srv/emca.<user>.<pid>`). `acme.c`'s behaviour is
the floor: emca is a new program and must pass acme's suite unchanged.

**Landed 2026-08-31** — `userspace/cmd/emca.c`, ~700 lines, con(1)'s libthread
shape (reader threads feeding one consumer through a channel, so a single thread
owns every window write and all tag arithmetic; no locks, shadow state only).
What it owns is what IPNX owns:

- **The window set and each window's ONE TAG STRING.** Title / toolbar / tag bar
  are three views of `<name> <builtins> <dynamic> | <scratch>`; the dynamic block
  is tracked by OFFSET, so the user co-authors the string and their text never
  shifts under them. `acme.c:383`'s rebuildauto, kept.
- **The core verbs** — `Del Snarf Get Look Edit`, plus `Put` and `Zerox`. The
  toolbar a window shows is that set merged with `/type/<t>/window`, and a type
  **may not redeclare a core verb** (compared by LABEL, not by whole line): a
  type that redeclared `Put` would make the dirty indicator lie, since Put's
  presence *is* the indicator. Six type files shed verbs to this rule.
- **Dirty state**, and therefore Put's appearance.
- **Buffers**, on the heap — N windows may view one, so `Zerox` **aliases** and
  the recorded copy-not-alias divergence retires.
- **Placement**: emca reads `/type/<t>/pane` and writes the window's `wctl`,
  which grew a `pane <name>` verb. A **watcher, not a gatekeeper** *(RETIRED
2026-09-02 — emca is the window manager; the phrase and the `pane` verb both
belonged to the hardcoded-pane design M15 replaced)* — it learns of
  windows from the device exactly as the surface does, has no privilege, and with
  emca not running a window still opens in its type's default pane *(retired
2026-09-02, as above)*.

Two contract changes came with it, both in [window.md](../window.md): **`content` is
an event** (a window is minted before its file is known, so sampling it once
races whoever fills it in), and **`put` notifies rather than commands** (the
surface holds the byte-exact text; emca re-reads what landed — one writer per
file, so a mirror error can no longer overwrite correct bytes).

Proved twice, because the claim has two halves. In rc — *emca driven entirely by
writing a window's `events`, which is the surface's own voice*: the merged
toolbar, Put absent while clean and present while dirty, `Put` writing **emca's**
buffer rather than the bytes it read, Zerox aliasing, Del closing. And headlessly
at the surface, `node demo/supervisor/emcaproof.mjs userspace/rootfs`: the
placement reaches the host on the chrome, and the toolbar that arrives is the
merged set. Its sibling `winproof.mjs` still runs with **no emca at all** — that
pair is the degrades-correctly rule, in code. **155 PASS / 0 FAIL** on the Rust
host and on the frozen oracle, which self-skips.

Also landed: `/lib/namespace` binds `#w` at `/dev/window`, so the contract's path
is the real one. **Still unbuilt in this stage**: sam's structural language
(`Edit`), the plumber (`Look`), and the running-command table as a view of
`/proc`. The file interface landed later, as M17a1.

**d. The web surface *(L)*.** The four chrome surfaces under
*operand determines surface* (system → top toolbar; window → its
toolbar; range → the floating bar; nothing → the status line), the
responsive rules **measured in characters not pixels** (72 columns a
leaf, 10 lines a body — so accessibility text sizing moves the
breakpoints for free and WCAG 1.4.4 holds by construction), native text
input replacing the hand-rolled caret (the prerequisite for any soft
keyboard), the keyboard grammar, and the pin replacing the 2-1 chord.
**Amended 2026-08-31, twice**: the surface no longer receives a tree for
text at all — it is handed a **path**, opens it over 9P, and renders it
natively, so SVG, HTML, Markdown, PostScript and images cost nothing and
`Put` is the host streaming the file back. For text specifically, the
renderer is a real editor component —
**Monaco** on the web, TextKit on Apple — which brings selection,
clipboard, keystroke undo, syntax highlighting, folding, multi-cursor
and find-in-file for free, and carries command history and line editing
for the console. **xterm.js returns as the raw-input door.** The
gating question, to be proven FIRST and not assumed: **can `execute` and
`look` ride Monaco's action and context-menu API?** Property 1 of the
acceptance test depends on it, so a spike answering that precedes the
rest of this stage.

**The gating question is ANSWERED, and the component is CodeMirror, not
Monaco** (2026-08-31). CodeMirror 6 has **no menu of its own** — it uses the
platform's — which turns out to be *stronger* for property 1 than Monaco's
action API would have been: the verbs ride the surface the user already
right-clicks, rather than a component-specific menu that a different surface
would have to reimplement. Observed putting `look 29 36 surface` where a plain
guest read it, byte-correct across a multi-byte em-dash.

**Landed 2026-08-31.** The four chrome surfaces are complete: **the floating
bar** now appears AT THE SELECTION (it was a right-click menu), carrying the
closed range set with the range **snapshotted when the bar appears** — the known
trap, since the tap that presses a button collapses the selection first. Always-
applicable verbs render immediately and Open/Jump arrive when emca answers, so
the bar never waits on a round trip.

**Two channels, and the reason each exists.** `verbs`, per window, is
emca.md's third sanctioned protocol addition (verb applicability) in the form
the narrowed architecture allows — a FILE, the exact parallel of `toolbar` one
operand narrower. `/dev/window/pin`, at the root, is a **gap the build found**:
the pin is IPNX's by the design's own test (it differs between workspaces) and
`execute` is emca's, so emca must hold it — but the design named no
workspace-scope channel, and the status line cannot show what it cannot read.

**The responsive rules are measured in characters**, from the *rendered* font:
`leaves = floor(cols / 72)`, `bodies = floor(height / (10 lines + tag))`, and
the class follows — 3+ leaves xlarge, 2 large, 1 medium if a detached rail still
fits beside it, else small. Grow the text and the breakpoints move for free.
Overrides are remembered **per size class**. And **collapsed is not hidden**:
the first implementation read "panes: none" at small as *hide them*, which
deleted the listing and the console outright — the exact information loss the
invariant forbids. Small INLINES both panes as concertina rows showing their
tags. Which exposed the structural bug behind it: only `/dev/window` windows had
a tag row, so the console had nothing to collapse *to* and vanished. Every
window now carries one, and IPNX's chrome **enriches** that row rather than
building a rival — which is what "a concertina row, a rail entry, a tab and a
minimised window are the same object" actually requires in code.

**The keyboard grammar** is the table from emca.md, complete — `⌘↵` execute,
`⌘⇧↵` look, `⌘⌥↵` pin (on the selection, or acme's own fallback, the word under
the caret), `⌘S`/`⌘⌥S`, `⌘W`, `⌘N`, `⌘T`/`⌘⇧T`, `⌘B`/`⌘J`/`⌘⌥B`, `⌘1..9`,
`⌘⌥→ ←`, `⌃⇥`. Cut/Copy/Paste/Undo/Redo/Find are deliberately NOT intercepted
inside a focused editor: they are the platform's, and taking them would replace
working IME, clipboard and find-in-file with worse copies.

**The status line's right zone** carries what the design named and nothing else:
running commands with tap-to-kill, the pin, the dirty count, line:col. Naming a
running command needed **`/proc/<pid>/args`** — proc(3)'s own file, whose data
was always in the proc record with nothing able to read it, so nothing could
name a process rather than number it.

**157 PASS / 0 FAIL** on both hosts (`/proc/<pid>/args` and the applicability +
pin test both self-skip on the frozen oracle), plus both surface proofs.
**Still unbuilt in this stage**: native text input for a soft keyboard beyond
what the component gives, and the type's extra verbs on the floating bar —
decided item 6 says extras appear on both surfaces, but the registry declares
one verb list and `Wrap host:toggle-wrap` is plainly not a range verb, so the
merge wants a second field rather than a guess.

**e. `/project` *(M)*.** Templates instantiate, workspaces open;
`/project` is a union of system and personal so promotion is moving a
file between union elements; promotion promotes the *declaration*, not
your files. **Clone and instantiate are separate acts** and instantiate
shows the declaration first — a cloned project file is a stranger's
declaration. Full `/project/ipnx` waits on git (M10); the mechanism does
not.

**f. Boot into emca *(S)*.** The default workspace as a namespace file —
motd in an editor, a listing left, an rc bottom, the managers in the top
toolbar. The "boot is rc plus a namespace file" refusal paying out.

**g. The SwiftUI surface *(M)* — this is M6's second re-aim.** The same
tree, Apple's grammar, HIG furniture; divergence from the web surface
accepted and named per the input convention.

**Acceptance:** the four protocol additions carry a window tree that a
generic surface renders as native furniture, proven headlessly through
the virtual surface; `/type/proc` is three small files and `/proc` gains
Kill without a manager program existing; emca passes acme's behaviour
suite unchanged; the web surface renders one workspace correctly at all
four breakpoints with nothing disappearing as the viewport grows, and
every floating-bar verb and toolbar button is reachable from the
keyboard with Tab; a project instantiates, is saved, and is promoted,
with its declaration shown before it runs; the system boots to emca with
the default workspace and no shell in front of it.

### M15 — the compositor *(L, staged)* — **supersedes much of M14a–d; the piece that should have been designed first** — consumes: the compositor decision (design.md 2026-09-01), [emca.md](../emca.md) PARTS FOUR and FIVE

M14 built emca's surface as three hardcoded named regions — a `PANES`
map holding `left`, `main` and `bottom`, a `pane <name>` verb in
`wctl`, and a `pane` file per type naming one of those strings. Once
placement is a NAME the composition is frozen at three slots, nothing
nests, and rows and columns are decoration. Every symptom traced to
it. **This milestone replaces that model with the one in emca.md:
one object, composited recursively.**

It is sequenced FIRST among what remains, because the types and the
demo both sit on top of it. Christine's own diagnosis: *"The first
thing we should have designed was the compositor and the root window.
If we get this right then we have correct behaviour, window controls,
resizing, tabs, panes, etc."*

**a. Strip the superseded model *(S)*.** Delete before building: the
`PANES` map and named regions, `pane <name>` in `wctl` and
`Win.pane`, `/type/<t>/pane`, the floating bar and its CSS, the pin
(kernel file, `Effect::Pin`, emca state, the status chip), the
overflow menu, the single global status line, and the tab strip's
separate leaf machinery. The suite must stay green across the
deletion — anything that fails is telling us the thing was load-
bearing after all.
**Acceptance:** the tree builds and the suite is green with none of
the above present.

**b. The tree, in the kernel and in emca *(M)*.** A window gains
children, an axis, an allocation and a tab flag. Nesting is walkable
as files — `/dev/window/<type>/<n>/<child>` — so the structure is
`ls`-able and `grep`-able, which is the claim the whole design rests
on. **Alternation is enforced**: a container's axis is perpendicular
to its parent's, stored not derived (derivation does not survive
restructuring), and a same-axis container is flattened into its
parent. `New row` and `New column` resolve to *give children* or *add
a sibling* from the parent's axis, and never ask.
**Acceptance:** in rc, build a nested tree, read it back, assert its
canonical form; assert that creating a same-axis container flattens.
**LANDED 2026-09-01.** `axis` and a walkable `kids/` per window;
`newrow`/`newcol` on `wctl` resolving from the parent's axis; and
`delete` closing the subtree — acme's `colcloseall()`, one rule rather
than two, because a column is a window. Alternation needed no
flattening pass in the end: a same-axis container is never created,
because that case adds a sibling instead. **157 PASS / 0 FAIL** on all
three hosts.
**And it surfaced a latent bug of my own from M14a**: `rustkern.mjs`
decoded four effects as `host.x?.(read())`, and an optional call
SHORT-CIRCUITS ITS ARGUMENTS — so on any host without that handler the
reads never ran, the cursor never advanced, and every later effect was
parsed as garbage. It had been silently corrupting the headless
stream since WinChrome was added; the tree work made it fatal.

**c. Allocation, and the sizing heuristic *(M)*.** A parent allocates
rectangles to some children; the rest are tabs. `minimise(me)` moves
me out of the allocation, `maximise(me)` moves everyone else out,
both reversible. Sizing is content-aware after acme's `coladd()`:
every window declares a MINIMUM and a NATURAL along its parent's
axis; minimums first, the remainder shared in proportion to
`natural - minimum` capped at natural, the rest shared
proportionally; when minimums do not fit, the excess become tabs. A
user resize is a remembered override. **`Fit`** drops the size
overrides in a subtree and re-derives; **`Reset`** rebuilds the root
from its convention.
**Acceptance:** the strongest test in this milestone, and one that
only exists because emca owns the geometry — resize a tree
arbitrarily from rc, press `Fit`, and assert the exact allocation.
`Fit` is idempotent. A column of one full and one nearly-empty window
splits unevenly, **in the FULL one's favour** (the plan first said the
empty one's, which is backwards: acme shrinks the victim to the space
its content occupies, so the sparse window keeps only what it needs
and the full one takes the rest).
**LANDED 2026-09-01.** Allocation in the kernel — `alloc` per window,
`minimise`/`maximise`/`newtab` on `wctl`, `premax` making maximise
reversible, and `kids/` named BY POSITION because order is the layout
and `ls` sorts. The heuristic in emca, because it needs content sizes:
minimum and natural along the parent's axis, minimums first, the
remainder shared by slack and capped at natural, the rest shared
equally. A user `resize` is a remembered override; `Fit` clears the
subtree's and re-derives. The kernel gained `rect` (distinct from
`resize`, which reallocates a raster a tree window does not have) and
a `parent` file, since a resize must re-lay-out the SIBLINGS.
**Measured**: a 1200x800 column holding a one-line file and a 720-line
file splits **72 / 728**; `Fit` twice changes nothing; a resize to 300
gives 300/500 and survives until `Fit` drops it. **159 PASS / 0 FAIL**
on all three hosts.

**d. The geometry conversation *(S)*.** The host reports
`resize <w> <h> <cellw> <cellh>` to the root window's `events` — the
viewport and the text cell, both device-independent. emca computes,
the host renders. Characters remain the leaf measure as
`72 x cellWidth`, so accessibility text sizing still moves the
breakpoints. Intrinsic size for pictures comes from file headers (PNG
IHDR, JPEG SOF, GIF, SVG viewBox), with a `size <w> <h>` event from
the host correcting anything emca cannot parse.
**Acceptance:** headless — write a geometry from rc, read back the
tree; the same geometry with a doubled cell changes the allocation by
exactly the factor the cell changed (fewer *columns* is M15e's, since
that is the root's convention dividing itself).
**LANDED 2026-09-01.** `resize <w> <h> [<cellw> <cellh>]` is ONE verb:
two fields from a user dragging a divider, four from a surface
reporting its viewport, because only the surface knows the cell — it
depends on the rendered font and the reader's text-size setting, both
of which are the surface's half. The extra fields carry extra
information rather than changing what the verb means. **Measured**: the
same 1200x800 viewport gives a one-line window 72px at an 8x18 cell and
**144px at 16x36** — every measure doubling with the cell, which is
WCAG 1.4.4 holding by construction rather than by testing.
**Pictures are sized from their headers** — PNG IHDR, GIF, JPEG SOF,
SVG viewBox — bounded work, a header and never a decode. A picture's
natural size is its INTRINSIC size, deliberately not "the width it was
given divided by the aspect", which would make the natural depend on
the allocation and stop `Fit` being idempotent. Anything emca cannot
parse is corrected by a `size <w> <h>` event from the surface, which
has actually decoded it — so layout never blocks on a decode.
**160 PASS / 0 FAIL** on all three hosts.

**e. The root window and its convention *(S)*.** Type `root`, the
window with no parent. It picks columns by convention, divides into
three at large with the middle one splitting into two rows, and
**nothing downstream can tell those columns from any others**.
**Acceptance:** boot at four geometries and assert the tree; assert
that closing a convention-created column leaves a valid tree, and
that `Reset` restores it.
**LANDED 2026-09-01.** `leaves = cols / 72` from the reported cell
decides how many columns: one at small (the root IS the column, so no
degenerate single-child container), two at medium, three at large,
four at xlarge. Measured across 400/1200/1800/2400 at an 8x18 cell.
The kernel gained the two PRIMITIVES emca needs to express a tree —
`axis row|col` and `newkid` — distinct from `newcol`/`newrow`/`newtab`,
which are the USER's verbs and resolve alternation for them; and
`reparent <wid> [<index>]`, which is what acme files under layout as a
gesture and what M15g2's drag will land on.
**Two things the build corrected.** Crossing a breakpoint RESTRUCTURES
rather than destroying: a shrink hoists windows out of the columns it
removes before deleting them, so nothing is lost to a window being
resized. And the convention belongs to THE ROOT TYPE, not to every
parentless window — the first cut applied it to any window without a
parent, which silently rebuilt an unrelated test's tree.
**161 PASS / 0 FAIL** on all three hosts.

**f. The verbs, and the four window types *(M)*.** Four surfaces,
one operand each ([emca.md](../emca.md)): the window in its layout ->
the title bar row (Close, Minimise, Maximise, and Duplicate as
column / row / tab); the window's content -> the toolbar (the
manager's verbs); the tag line's text -> its own six buttons (New,
Open, Run, Find, Edit, Add — always these six, in this order); a selection in the body -> the floating bar
(Cut, Copy, Paste, Open, Find). **`Find` is acme's jump and search in
one**, because `:` and `#` already tell an address from literal text —
the button does not have to. **Verb
applicability is one mechanism serving two operands** — the tag line's
text and the body's selection — which is what the stripped `verbs`
file comes back for. **emca uses the era's names** — Copy, Save,
Revert — while acme's port keeps Snarf, Put and Get, because renaming
acme's buttons would be changing acme.

**Potential types: `root`, `ls`, `edit`, `shell`, `output`, `pkg`,
`project`.** *(Written 2026-09-02 when all were undesigned. By the end
of that day: `root` is **`inode/system`**, `ls` and `edit` turned out
to be **roles** rather than types, `output` is **not a type** at all —
`text/plain` under a path convention — and `shell`, `pkg`, `project`,
`text/plain` and `inode/directory` are designed in
[type.md](../type.md). The paragraph stands as the record of the moment
the list was opened.)* They have been *named* in design conversation and
nothing more; naming a type is not specifying one. An earlier draft of
this line read *"and no others"*, which converted an open list into a
closed decision that was never taken — `pkg` and `project` were
mentioned as types **we will need to design** (2026-08-31), and the
constraint erased them.

The reason a type list wants care at all: a type in the table becomes a
button on a toolbar, so an undesigned type ships as a control that does
nothing.

*(Naming, since two of these are managers rather than types: the type
says what content **is**, the manager says what to do with it, so `ls`
and `edit` are the managers of types `directory` and `text` —
[type.md](../type.md), 2026-09-02. An earlier draft of this line renamed
the types themselves to `ls` and `edit`, which that correction
reverses.)*

**`/output` is a
filesystem served by emca, MIRRORING the filesystem** *(SUPERSEDED
2026-09-02: a command line cannot be a filename — it contains `/` — so
encoding one into a path needed escaping or substitution at every access.
`/output/<n>/` takes `/proc`'s shape instead: a number, with `cmd`, `dir`,
`data` and `status` as files inside. The fact the mirror encoded is simply
another file, and `grep -l /template /output/*/dir` answers the question
better than navigating a tree you must already know the path of.)* —
`/output/home/project/ipnx/mk all` is a real path, so command output cats,
greps and retargets like anything else, where acme's `<dir>/+Errors`
lies about the directory and a file in `/tmp` gets the context wrong
and dangles. Mirroring resolves the collision (two projects, two paths)
and removes an exception: context is the title's directory with
`/output` stripped, so no type keeps a private field. `Add` makes the toolbar user-extensible, so the
CORE is closed but the toolbar is not.
**Acceptance:** 37 of acme.md's 38 operations are reachable (Sort is declined, with its reason in the spec) and
does what it says; each type's declared verbs work; each status bar
shows what its type says it shows; and no button exists that does
nothing.

**PART LANDED 2026-09-01 — the types and the toolbar.** `dir` is now
`ls` and `text` is `edit`; `output` and `root` join them, and the
window files carry the era's names. **Nothing is universal on the
toolbar any more** — not Save, not Revert, not even Undo, which
belongs to `edit` alone because Undo is available exactly where a
window's operations stay in a buffer emca holds. emca's `core[]` is
empty; what it emits is the TYPE's list plus `Save` while the buffer is
dirty, which is acme.c:383's rule with acme's `Put` translated.
Verified in the surface: an `ls` window shows no toolbar at all, an
`edit` window shows `Revert Undo Redo`, and `Save` appears only when it
should. **161 PASS / 0 FAIL** on all three hosts.

**Still owed in this stage**: the tag line's six buttons (Open, Find,
Run, Find, Edit, Add) and the window operations on the title bar row.
Three of the six need infrastructure that does not exist yet — `Find`
needs a channel for emca to hand a set of ranges back to the surface,
and `Run`/`Pipe` need `/output` served, which is emca's deferred file
interface. **They must not appear until they work**, which is the
acceptance criterion above.

**g. The surface renders the tree *(M)*.** The browser surface stops
owning layout and becomes a renderer: it draws the tree emca computed
and reports geometry back. Every window gets the four components,
three controls, scrollbars and its own status bar. Controls send
messages to the parent; none acts locally.
**Acceptance:** `emcaproof.mjs` asserts the rendered tree matches the
one emca published, at three geometries.

**g2. Move: drag and drop *(S)*.** acme's `move`, one of the eight
gesture-only operations. One `wctl` verb — re-parent to a given window
at a given index — and two drop targets in the surface: onto a window
becomes a tab of its parent, onto an edge becomes a new row or column.
That second one is the other way a USER creates a split, which is what
keeps "only a user creates rows and columns" true.
**Acceptance:** in rc, re-parent a window and assert both trees; in
the surface, a drag lands where the drop target says.

**h. The demo *(S)*.** Wired from what a–g built and nothing else: a
listing in a column, edit windows as tabs, rc in a row. No button
that is not functional, no type that is not built, no name that is
not in the spec.
**Acceptance:** every control on screen does what it says, verified
by driving the real page.

### M17 — the demo, rebuilt on the new design *(L, staged)* — **the whole of the 2026-09-02 design, and nothing beyond it** — consumes: the type system, the roles, the manager interface, `/dev/window`, the layout file ([type.md](../type.md), [window.md](../window.md), [surface.md](../surface.md))

**The objective is replacement, not features.** The demo that exists —
a listing on the left, `motd`/`tour`/`README` as tabs, `rc` below —
must come back **built on the new design**, doing the same things. New
capability arrives only where the demo cannot otherwise be equivalent.

**Explicitly out of scope** (Christine, 2026-09-02): **`project` and
`template` are not implemented.** They are designed and can wait. **`pkg`
may be**, and only far enough to carry **Go, Python and Ruby**, because
the demo has those today and would otherwise lose them.

**The gate, and why nothing can be done incrementally on top of what
exists:** the compositor is green but lives in the kernel, which this
design removes. M15's *behaviour* transfers — allocation, alternation,
sizing, geometry, the root convention are all correct — but its
*location* is wrong, so stage **a** moves it before anything else runs.

**a. The window system leaves the kernel *(L, and larger than "delete")*.**

> **Measured before starting, 2026-09-02:** `acme`, `win` and `con`
> reach the kernel's window device directly, and all of them draw
> through `kernel/src/draw.rs` — **17 raster functions, 363 lines**.
> "Delete it from the kernel" would break the real acme, sam, con and
> win on the first commit, which is the project's headline achievement
> and 95 of the suite's window assertions. **This stage is a MOVE with
> the interface held constant, not a removal.**

Two destinations, and the design already says which is which:

| what moves | to | why |
|---|---|---|
| the **tree** — `parent`, `kids`, `axis`, `allocated`, `premax`, `split`, `maximise`, `reparent`, recursive close, the four file kinds, the nine `wctl` verbs | **emca**, as an ordinary 9P file server | it is window-manager state, and acme's `fsys.c` and rio both do exactly this |
| the **raster engine** — `draw.rs` | **the host** (Saranos), as a delegated file | *"renderers are in Saranos"*; `/dev/draw` becomes host-served, and its clients cannot tell |

**The interface does not change**, which is the whole point: `acme`
keeps opening `/dev/draw` and never learns who answers. That is the
substitutability property the design rests on, tested by the one case
that would expose it.

**Acceptance**: the kernel has no window concept and no raster engine;
`acme`, `sam`, `con` and `win` run unmodified; the 95 window
assertions pass against emca and the host instead of the kernel.

> **Measured again, 2026-09-02, before writing any code: emca is NOT a 9P
> file server.** It has no `Tversion`, `Tattach`, `Twalk` or `Tread` — it
> `post()`s messages, reads `#w` events and writes back through `wctl`. It is a
> **client** of the kernel's window device. So "the tree moves to emca" is not
> relocating code between files: **emca must become a server first.**

**a1 — emca becomes a file server *(M)* — LANDED 2026-09-03.** `emca.c` serves
9P2000 on a pipe posted at `/srv/emca.<user>.<pid>` (`#s`, srv(3)), from a libthread thread
that parks on the read while the workspace keeps running. Two levels, and the
qid is arithmetic rather than a node table — `qpath = (wid+1)*16 + file` — so
enumeration cannot drift out of step with the window table:

```
/            the window set
/<n>/        one window, named by its id
/<n>/{rect,size,verbs,status,dirty,type,role,title,events,ctl}
```

**Every answer comes from emca's own state, not from the kernel.** `setrect`
retains the rectangle emca decided; `setcontent` sets the title emca owns;
`pushtoolbar` stashes the verb list it already computes, so one merge rule
serves both the window's `toolbar` file and the contract's `verbs` — a second
implementation there would be a second answer waiting to disagree.

**What a1 does NOT do, and which stage owns it:**

| not yet | owned by |
|---|---|
| **`/dev/window/` mounted into each manager's namespace** — the door a manager actually uses. Today only the external `/srv` door exists | **b** |
| **a blocking read on `rect`** — reads return immediately, so *resize* is not yet the contract's blocking read | **b** |
| `body` | **b** |
| `ctl`'s minimise and maximise (close and duplicate route today) | **b** |
| the tree — `parent`, `kids`, `axis`, `allocated` are held but still mirrored from the kernel | **a2** |

> **Measured while building it, and it is why a1 had to come first:**
> `wctlf(wid, "newkid")` **returns no window id** — emca discovers the child by
> re-reading `kids/<i>/winid` from the kernel. So the ids are the kernel's, and
> the tree cannot move to emca until emca is the thing that mints them. Reading
> the write path, not the read path, is what surfaced this.

> **Two rectangle paths exist, and only one is the contract's.** `rect` written
> to the kernel's `wctl` is the surface talking **past** emca — the kernel
> records it and emca never learns. `resize` on the window's `events` is the
> surface talking **to** emca, which decides and then tells the kernel. The
> suite drives the second and asserts both answers agree; **the first is
> precisely the coupling a2 removes.**

> **One emca per event queue.** `/dev/window/events` is a queue, so two emcas
> each consume half the events and neither holds the whole set — which is how a
> second instance ended up owning a window's type but not its title. The suite
> runs exactly one, started after `bind '#s' /srv` so its server can post.

**a2 — the tree moves to emca *(M)* — LANDED 2026-09-03. THE KERNEL HAS NO
TREE.** `Win` holds `parent`, `kid[]`, `axis`, `allocated`, `premax`; `split`,
`reparent`, `minimise`, `maximise`, the recursive close and the root convention
all compute in emca; and emca serves the result at `/dev/emca/<n>/` —
`parent`, `axis`, `alloc`, `winid`, `kids/<i>/` — for tools. **A manager's own
`/dev/window/` shows none of it**, so the contract is untouched and no manager
can see the arrangement it sits in.

**285 lines left `kernel/src/lib.rs`**: the five `Win` fields, `split`,
`maximise`, `reparent`, `clone_window`, the eight tree verbs on `wctl`, the
three tree file kinds, the `kids` directory, and `win_close`'s recursion.
`grep` finds zero references to any of them. What the kernel keeps is what a
window *is* to it — an identity, a raster, its connections — and `wctl` is down
to `rect`, `move`, `resize`, `mouse`, `delete`.

> **The identity/structure split is what made this possible, and it corrects
> what a1 concluded.** a1 found `newkid` returns no window id and inferred emca
> must mint ids first. One `nextwid` serves `#w/<type>/clone` and `newkid`
> alike, so minting is the **raster** side and stays until a3 — but `clone`
> *returns* the id synchronously. emca asks for a window, receives an identity,
> and decides the structure itself. **The kernel supplies identity; emca
> decides place.**

**Three things had to move that were not state:**

| what | why it mattered |
|---|---|
| **the cycle refusals** | the kernel refused a window as its own parent, and as a child of its own descendant. emca had to acquire both — *the invariants the kernel enforced are part of what moves*, and a layout walk over a cycle does not terminate |
| **the recursive close** | closing a container closes what it holds (acme's `colcloseall`). The kernel did it while it held a tree; `dodel` does it now, and `delete` therefore had to become one of emca's verbs — written to the kernel it closes one window and strands its children |
| **the door** | every tree verb — `newcol`, `newrow`, `newtab`, `minimise`, `maximise`, `reparent`, `delete` — arrives on a window's `events`. Written to the kernel's `wctl` it was the surface talking **past** the window manager, the same correction the rectangle needed in a1 |

**And the tree verbs became asynchronous**, which is not incidental: a verb is
now a **message to another process** rather than a write the kernel applies
before returning. `tests.rc` gained `kidwait`/`allocwait` beside `emwait`.

**The scaffolding is gone with the thing it supported.** While both answers
were live, every emca mutation mirrored to the kernel and the suite pinned
their equality — which is what made the deletion safe rather than hopeful. With
the kernel's copy gone, `mirroralloc` and every mirroring `wctlf` went too.

**a3 — THE WINDOW DEVICE LEAVES THE KERNEL *(L)* — DECIDED, DEFERRED, AND NOT
THE GATE (2026-09-03).** Scoped as *"the rasteriser moves"* until the rule that
**the kernel does not grow** was applied to it: `#w` is **~1,100 lines, 22% of
the kernel** across 29 `WKind` variants, and **none of it is process
orchestration**. a1 and a2 already removed the tree this way. So a3 is not a
relocation of `draw.rs` — it is the rest of the device following the tree
out.** Christine: *"/dev/draw should be rendered by host. the
kernel does not know how to draw."* So **the host serves it**; the kernel stops
producing pixels and `Effect::WinUpdate`'s rgba feed goes with them.

> **The plan was wrong about who needs this, and the correction changes the
> sequence.** It claimed *"`acme`, `win` and `con` reach the kernel's window
> device and all draw through `draw.rs`"*. Measured 2026-09-03: **`con.c` has
> zero draw references; `win.c` has one, and it is a comment; `emca.c` has
> zero**; `/rc/emca` and `/rc/emcaopen` touch draw nowhere. The real clients
> are **acme** and **samterm** — the heritage exhibit — plus `acmetest`,
> `drtest`, `samtest` and `init.c`.

**THE DEMO DOES NOT NEED THE RASTER.** Christine: *"we don't need /dev/draw for
the demo, text is sent to host, which is responsible for rendering."* The
demo's path is text — `Effect::WinChrome` carries content, toolbar and tag,
`Effect::WinText` carries bytes, the host renders natively. **So M17b and
M17d–h do not wait on this**, and a3 is sequenced by its own value rather than
by anything blocked behind it.

**What it still costs when it is done** (measured 2026-09-03): `draw.rs` is
363 lines and 17 functions, self-contained (`std` only); `drawmsgs()` is 174
lines, the draw(3) interpreter, with 18 call sites in `lib.rs`; roughly 19 of
the suite's 164 assertions are raster, and they are the acme/sam/libdraw
proofs. `poc/supervisor/draw.mjs` is a readable 220-line JS reference in the
frozen oracle.

**Still open, and registered in [proposals.md](../proposals.md):**

| | |
|---|---|
| **`/dev/window/<n>/rgb`** | the suite's only window onto pixels. If the kernel holds none, either the host serves `rgb` too or the raster assertions move host-side |
| **shared rasteriser, or native per host?** | the two Rust hosts could share a crate and Node needs JS — but implementing natively (Core Graphics, Canvas2D) is the *point* of moving it |
| **chattiness** | libdraw issues many small ops; today they are direct calls. Whether a per-op crossing needs batching is unmeasured, and worth measuring rather than designing around |

**Sequencing consequence, corrected 2026-09-03**: the gate was **only ever the
tree**, and the tree is out (a1, a2). **Every remaining stage — b, and d
through h — can proceed now.** a3 is heritage work for acme and samterm and
blocks nothing: the demo's windows carry text, and the host renders it.

The earlier reading — that stage **c** alone could land first because
everything else waited on the raster — rested on the claim that `con` and `win`
draw. They do not.

**b. `/dev/window` and `/dev/emca` *(M)*.** emca serves one directory
per window — `rect`, `size`, `verbs`, `status`, `dirty`, `type`,
`role`, `title`, `events`, `ctl`, `body` — with each manager's own
bound at `/dev/window` and the full set at `/dev/emca/<n>/`. **A
blocking read on `rect` IS resize.** **Acceptance**: a manager opens
`/dev/window/rect`, blocks, and returns when the window is resized,
with no window id anywhere in its namespace.

**c. The type registry, rewritten *(M)*.** `/type/<mime>/` with
`recognise`, `managers`, `verbs`. Recognition in pipeline order —
serving device, qid, path, exact name, extension, magic, fallback —
**stopping after the qid for streams**, which must never be sniffed.
`emcaopen <path> [role]`. **Acceptance**: `emcaopen /etc/motd` opens a
`text/plain` window with no type named; `emcaopen /home` opens a
listing; a stream is classified without a byte being read.

**d. `text/plain` and `inode/directory` *(M)*.** The two managers the
demo needs: `look` and `edit` over both. Text carries Save/Revert/
Undo/Redo, addresses in runes, Find-as-multi-cursor with the match
count in the status line. The listing is one name per line, with
identity tracked through the edit stream. **Acceptance**: the three
files open, edit, save; a rename by editing the listing applies on
Save and shows `pending` before it.

**e. `inode/system`, the layout and the sidebar *(M)*.** `/` recognised
by exact path, specialising `inode/directory`; `manage` adding `Halt`,
`Reboot`, `New Shell`; `/type/inode/system/layout` declaring the
windows; the manager's own writable child rendered as the left pane.
`/rc/emca` shrinks to one line. **Acceptance**: the demo's shape comes
from `layout` and from nothing else — editing that file changes it.

**f. The `shell` role *(M)*.** `rc` in a window: the transcript as a
real file, line discipline **host-side**, `rawon`/`rawoff` on `ctl`.
**Acceptance**: `vi` works (Escape reaches the process); output
arriving mid-line does not corrupt what is being typed; a path selected
in the scrollback opens with `Open`.

**g. The surface, rebuilt *(M)*.** The browser surface renders the tree
rather than owning a layout; the outermost window's chrome is the
host's; the six tag line buttons; the status line. **Acceptance**: the
surface holds no layout state, and `demo/shell/shell.mjs` reports the
**content** rectangle without a hardcoded chrome constant.

**h. `pkg`, only as far as the demo needs *(M)* — OPTIONAL.**
Declarations for Go, Python and Ruby over `/store`. Not `project`, not
`template`, not promotion. **Acceptance**: `python hello.py`, `go run
hello.go` and `pkg install ruby` work as they do today.

**Still unbuilt after this stage**: `project`, `template`, promotion,
`properties`, the SwiftUI surface, and every parked question in
[type.md](../type.md)'s open list.

### M16 — Rust, as a package *(M, research-first)* — consumes: `pkg` (installing is a bind), the toolchain pattern (cc, go)

**Rust arrives the way anything else does: as a package.** `pkg
install rust` binds a verified subtree into `/bin`, exactly as it does
for any other toolchain, so this milestone needs NO new mechanism —
which is the point of "installing is a bind". Hers, correcting an
earlier draft of this entry that treated it as a bespoke build: *"We've
already said we will support packages. Rust will obviously be one of
them."*

What remains is a MEASUREMENT, and it belongs to whoever builds the
package rather than to the system: can `rustc` run as a wasip1 guest?
`cc` runs `clang -cc1` and `wasm-ld` as guests, so LLVM already does;
`go` runs the real gc compiler because it is pure Go and cross-builds.
`rustc` is a Rust program with a C++ backend that loads codegen
backends dynamically, and has no shipping wasip1 build — so the open
questions are whether the obstacle is the backend or the driver
(threads, dynamic loading, `std::process`), and whether the tractable
form is a driver like `cc(1)` orchestrating pieces that DO cross-build.

**Acceptance:** the finding recorded in RESEARCH.md with provenance,
either way. A negative — "rustc cannot be a guest today, and here is
what would have to change" — closes it honestly and is worth as much
as a positive. The registry entry lands when the answer is yes.

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
 ├──► M3 ──► M6 ◄──┐   ├──► M9 ──► M12
 ├──► M4           │   │
 ├──► M5 ──► M14 ──┴──►│   (M14 also feeds M13)
 └──► M7 ──► M8 ───────┘
              └──► M10 (personality needs /net for sockets; git wants M4's disk)
```

M1–M5 are independent of each other after M0 and can be reordered by appetite;
M7 is the gate everything distributed waits behind. **M14 (emca) sits
immediately after M5 and before M6**: the iPadOS app and the VSCode surface
are *surfaces of emca*, not independent hosts (decision log 2026-08-31), so
M6 consumes M14g and M13's panels consume the same tree. M14e's `/project`
is mechanism-complete without git; `/project/ipnx` waits on M10.

## Engineering questions (not design questions — those live in design.md)

- **M14:** ~~the editing questions, the surface's testing, snapshot-vs-replay,
  the plumber, instance storage, the type ceiling~~ — **all resolved 2026-08-31**
  (decision log). Undo is one stack in emca; the resync path is a sequence number
  per edit and a hash per sync; **property 1 is a selection criterion** for any
  editor component (expose the selection, accept custom context-menu commands)
  rather than a risk about Monaco. The surface half is tested through a per-window
  **`ui` file derived from the platform accessibility tree**, so one assertion runs
  headless and native alike; snapshot is free, the writable layer already being a
  tree; the plumber **adopts plumb(6)** verbatim; an unnamed instance gets its
  layer at birth, so saving is a rename; and the type ceiling **dissolved** — a
  type declares nothing about rendering, so there is no vocabulary to grow into.
  **What remains for this milestone**: the floating bar's order and grouping,
  which is empirical, and a verification spike on whichever editor component is
  chosen — an afternoon, not a gate.
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

## Who each milestone serves

*Moved from personas.md 2026-09-02. A persona is extrinsic — it exists whether
or not this project does — so a mapping from milestones to personas is a fact
about the PLAN, not about the people. It belongs here and changes when the plan
changes.*

| milestone | serves | note |
|---|---|---|
| M0, M1 | all (substrate); P4-adjacent ops | the container is also CI |
| M2 | P5 (first fragment), P4 (sandbox syntax) | |
| M3 + M4 | **P1** | the journey says: land as a pair |
| **demo + tour** | **P2, P3** | finished software, missing URL — gap 1 |
| M5 | P3 (teach the one kernel twice), P2 | |
| M6 | P1, P5 | |
| M7–M9 | **P4, P5** | the timing bet; M9 ships the quickstart |
| M10 | P1, P3, P5 — and credibility for all | git is every persona's proof |
| M11, M12 | P4, ops | |
| **M14 (emca)** | **all five** | the system's interface; P1's daily-driver gate |

Every milestone serves someone concrete — the spine survives the exercise.

### M18 — restore the kernel to a subset of Plan 9's *(L, and mostly deletion)* — consumes: [RESEARCH §9.11–9.14](../../RESEARCH.md)

**The contract, restated:** the kernel is a reimplementation of a **subset** of
Plan 9's. A device exists here only if Plan 9 has it, means the same thing by
it, and **uses the same letter**. The one permitted exception is the hosted
boundary, which must be justified in writing against Inferno `emu`.

**This is not a design exercise. The target is readable in `plan9/`** — which
is why the reference tree is now kept locally.

| # | what is wrong | rectification | status |
|---|---|---|---|
| **`#w`** | Plan 9's `'w'` is **watchdog**. Ours bundles raster + window tree + canvas + chrome + type registry — i.e. **rio, which Plan 9 runs in userspace** (5,587 lines, `sys/src/cmd/rio`) | the window system leaves entirely. The tree already has (M17a1/a2); the raster follows as **M17a3** | in progress |
| **`#M`** | Plan 9's `'M'` is **`mnt`, the mount driver** — 1,198 lines and the most load-bearing device in the architecture. We took its letter for a ramfs, and left our own mount driver **nameless** | `'M'` returns to the mount driver. The ramfs splits: a read-only bootstrap root as **`#/`** (Plan 9's, 261 lines, writes are `Egreg`), the read-write filesystem to **userspace** | **gap** — the bootstrap ordering is undesigned |
| **`#V`** | invented; no Plan 9 equivalent | goes with the read-write filesystem it snapshots | with `#M` |
| **`#H`** | invented. **`webfs` is `sys/src/cmd/webfs` — userspace in Plan 9** | a userspace file server over the host's fetch. The kernel keeps nothing | not started |
| **`#\|`** | we implement pipes with **no letter** | `'\|'`, as Plan 9 has it | trivial |
| **`#Z`** | invented — but it **is** the hosted boundary, Inferno `emu`'s role | **kept, and justified in writing** rather than assumed. The one documented exception | write it down |

**Sequencing:** `#|`'s letter and `#Z`'s justification are immediate. `#H` is
self-contained and needs no design. `#w` is M17a3, already scoped. **`#M` is
the only one that is a genuine gap** — splitting bootstrap from filesystem is a
boot-ordering problem, registered in [proposals.md](../proposals.md).

**Beyond devices: the syscalls, the identity model, and the host boundary**
([RESEARCH §9.14](../../RESEARCH.md)). The same rule reaches three more places,
and all of them matter for the MicroVM and the Pi, where there is no host to
lean on.

| | what is wrong | rectification | status |
|---|---|---|---|
| **`link`/`symlink`/`readlink`** (traps 60–62, wire types 128/130/132) | **absent from Plan 9 and from 9legacy.** Plan 9 refuses links as a design position — `bind` and `mount` are its answer | the traps leave the kernel. Whether the *capability* survives in a userspace file server is a **separate question** and needs its own answer | **gap** |
| **`Cred { euid, ruid }`, `DMSETUID`, `DMSYMLINK`** | Plan 9's identity is **one field**, `char *user`. No euid/ruid/setuid in `9/port`; `DMSETUID`/`DMSYMLINK` are 9P2000.u, a **Unix extension** | the kernel carries `user` and `iseve()`. Everything else — the eve/ruid rule, transitions through `/proc/<pid>/ctl`, setuid-at-exec — belongs to the **personality, in userspace** | **gap** — and it contradicts [identity.md](../identity.md), which must be revisited rather than quietly overridden |
| **`Effect`'s 13 variants** | the hosted boundary, undeclared. Inferno `emu` is the precedent and was never written down as one | **justified in writing** against `emu`, like `#Z` — and shrunk: the `Win*` variants leave with the window system, `Fetch` with `#H`, `Snarf*` with the clipboard. What remains should be what a *machine* needs | write it down, then shrink |

| **`AsySnap { snap, data_ptr, sp }`** in the public `Effect::Spawn` | `data_ptr` is a **wasm linear-memory address**, `sp` is wasm's **`__stack_pointer`**. Dis has no linear memory; the CLR has no stack pointer to snapshot | make it an **opaque continuation token** the kernel stores and returns without inspecting. Authorised in purpose, wrong in shape | not started |
| **`#[cfg(target_arch = "wasm32")]` on the clock** | a compile-time branch on the substrate, inside the kernel. A hypervisor or a Pi is a **third** answer a two-way `cfg` cannot hold | time arrives as an operation the embedding answers, like every other platform fact | not started |

> **The root cause is the derivation's direction, and it must be corrected in
> the document that caused it.** [syscalls.md](../syscalls.md) derives the kernel
> call list from **V10's 68 routines**, dispositioning each as library,
> collapse, or kernel — which is exactly how `link` became *"C: kernel, trap
> 60"*. **Deriving from the personality inward guarantees the personality ends
> up in the kernel.** The list must be derived from Plan 9's subset outward,
> and what a personality needs beyond it is the personality's problem.

**Why this is urgent even though nothing is broken today:** on a hypervisor and
on a Raspberry Pi there is no host. `Effect`'s callbacks have nothing to call,
and identity.md's argument for euid/ruid — *"a hosted kernel inverts the trust
geometry"* — has nothing to invert toward, while the mechanism it justified
remains. **Every addition is a thing that must be carried onto hardware or
removed there**, and removing it there is harder than removing it here.

#### The kernel M18 produces — the design, revised

**Everything below is quoted from or measured against `plan9/`.** Where a line
has no Plan 9 counterpart it is marked, and it carries the two tests: *forced by
running on a VM at all?* and *could Dis or the CLR satisfy it unchanged?*

##### The devices — Plan 9's letters, Plan 9's meanings

| | | |
|---|---|---|
| `#/` | **root** | the bootstrap root: read-only, a fixed small table, `rootwrite` is `error(Egreg)`. Holds `/boot` and empty mount points and nothing else. **Restored** — we never had it |
| `#c` | cons | console, plus `user` and `hostowner` |
| `#d` | dup | |
| `#e` | env | |
| `#p` | proc | |
| `#s` | srv | |
| `#M` | **mnt** | **the mount driver takes its letter back.** Today it is nameless while a ramfs wears `'M'` |
| `#\|` | **pipe** | takes its letter |
| `#Z` | *(none)* | **the sole exception**, justified in writing against Inferno `emu`: the embedding's storage. A Plan 9 kernel reaches a disk through `#S`; a hosted kernel has no disk |

**Gone:** `#w` (the window system — emca and the host), `#V` (snapshots — with
the filesystem), `#H` (`webfs` is userspace in Plan 9 and will be here).

##### The calls — Plan 9's numbers, and four for the substrate

The 28 that carry Plan 9's own numbers stay. **`link` 60, `symlink` 61 and
`readlink` 62 go**: absent from Plan 9 and 9legacy, and refusing them is a
position — `bind` and `mount` are the answer. A personality wanting links
implements them in **its file server**, which is where 9P2000.u puts them
anyway.

Four traps have no Plan 9 number and are **authorised**, because no VM — wasm,
Dis or CLR — can do without them, and each is shaped for *a VM* rather than for
wasm: `ARGS` (argv at start-up, since no VM hands it on a Plan 9 stack),
`NOTEGET` (collect a note, since no VM takes an asynchronous upcall), `AREAD`
and `IOWAIT` (park cooperatively, since no VM offers blocking syscalls).

##### Identity — exactly Plan 9's, which is very little

One field per process, `user`, and one for the machine, `eve`, with `iseve()`
its only predicate. `#c/user` reads it. **Writing it can only ever say
`"none"`** — `auth.c:109`:

> ```c
> userwrite(char *a, int n)
> {
> 	if(n!=4 || strncmp(a, "none", 4)!=0)
> 		error(Eperm);
> 	kstrdup(&up->user, "none");
> ```

**A one-way drop, and no way to become another user at all.** `#c/hostowner` is
writable only by eve, and writing it sets `user` too. Permission is
`devpermcheck`: compare `up->user` to the file's owner.

**Leaving the kernel:** `Cred { euid, ruid }`, `DMSETUID`, setuid-at-exec, and
credential transitions through `/proc/<pid>/ctl`. **Where they go:** identity in
Plan 9 is established **per file server at attach**, so `su` is *"attach with a
different identity"* — a userspace act by the personality's libc and its file
servers, not a kernel call. `DMSYMLINK` goes with the symlinks.

##### The embedding boundary — substrate-independent

`Effect::Spawn` carries an **opaque continuation the kernel never inspects**:

```rust
Spawn { pid, image, argv, cont: Option<Cont> }   //  Cont(Vec<u8>)
```

Not `AsySnap { snap, data_ptr, sp }` — `data_ptr` is a wasm linear-memory
address and `sp` is wasm's `__stack_pointer`. **Dis has no linear memory; the
CLR has no stack pointer to snapshot.** Fork-on-a-VM is universal; wasm's
mechanism for it is not, and the kernel must not know which one it has.

**Time becomes an operation the embedding answers**, not
`#[cfg(target_arch = "wasm32")]`. A hypervisor and a Pi are a third answer a
two-way `cfg` cannot hold.

**Every remaining variant must be a *machine* fact** — console, timer, storage,
shutdown, and the delegated `#Z` — not a host convenience. `WinUpdate`,
`WinCanvas`, `WinChrome`, `WinText`, `WinGone` leave with the window system;
`Fetch` with `#H`; `SnarfSet`/`SnarfGet` with the clipboard.

> **This is the test the MicroVM and the Pi apply for us.** On bare metal the
> embedding is a hardware layer: every remaining `Effect` must map to a driver,
> and none may map to *"ask the host"*. Anything that cannot be written as a
> driver was never a machine fact.

##### What it costs

Measured in the suite: about **five `uid:` assertions** in `init.c` lose their
subject (the euid/ruid and climb-back cases; *"`/dev/user` names the host
owner"* and *"mode 0600 denies another user"* are Plan 9's own and survive),
**twelve link/symlink assertions** go with the traps, and **eighteen `snarf`
mentions** in `tests.rc` leave with the window system. Nothing else in the 164
depends on any of it — which is itself the measurement that says the additions
were never load-bearing.

##### The order to do it in

Each step is a **deletion or a rename**, and neither changes what a program can
do — so the suite stays green throughout, and any step that needs a test
rewritten is a step that has found a real dependency.

| | | gated on |
|---|---|---|
| 1 | **`AsySnap` → opaque `Cont`**, and time as an operation | nothing. Pure shape, no behaviour, and it is the one thing every later host inherits |
| 2 | **`#|` takes its letter**; `#Z`'s justification recorded | nothing (the justification is written; the letter is naming what exists) |
| 3 | **`link`/`symlink`/`readlink` leave** | a decision on whether the *capability* survives in a userspace file server — the traps go either way |
| 4 | **identity narrows to `user` + `eve`** | rewriting [identity.md](../identity.md), which currently argues the opposite. **Its model may be right as a personality** — the question is only what the kernel holds |
| 5 | **`#H` → a userspace file server** over the host's fetch | nothing; self-contained |
| 6 | **`#w` leaves** (M17a3) | the two open questions in [proposals.md](../proposals.md) |
| 7 | **`#M` splits**: `#/` bootstrap root, filesystem to userspace; `#V` with it; `'M'` returns to the mount driver | **the boot-ordering gap** — the only genuinely undesigned step |

**Step 1 first, deliberately.** It is the smallest and the least urgent by
today's symptoms, and it is the one that compounds: every host written before
it is fixed inherits wasm's fork mechanism in its interface.

**Acceptance**: every device letter appears in `plan9/sys/src/9/port/dev*.c`
with the same meaning; every trap number appears in
`plan9/sys/src/libc/9syscall/sys.h`; the kernel's per-process identity is one
name, as Plan 9's is; and every remaining host callback is justified in writing
against Inferno `emu`. Anything that fails those four is either removed or is
a **written, dated exception**. The suite stays green throughout — each step is a
deletion or a rename, and neither changes what a program can do.
