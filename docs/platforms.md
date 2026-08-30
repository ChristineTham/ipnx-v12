# The platforms — where it runs, and where everything lives

**Role: the *where*.** Two maps — the deployment forms the one kernel runs in,
and the canonical namespace a booted system presents — plus the **deployment
review ledger** that capability doctrine #5 ([design.md](design.md),
2026-08-29) mandates: the deployment story is re-examined periodically with
the same honesty as the code, and this file is where those reviews land, dated.
How to build and run each form: [handbook.md](handbook.md). What a host must
provide: [architecture.md](architecture.md).

## Where it runs

One kernel, one rootfs, one suite — per platform, only the host shim and the
engine change (the engine matrix is a dated decision in
[design.md](design.md)):

| form | host | engine (guests) | state (2026-08-30) |
|---|---|---|---|
| **Node** | `poc/supervisor` (frozen) | V8, Workers + SAB | **green, 149** (floor 131) — the oracle; also the dev loop; `#V` and canvas self-skip by design |
| **Browser tab** | `poc/browser` (frozen) → `hosts/browser` (M5) | the page's own JIT, Workers + SAB | **green, 150** (floor 131) in Chrome 148 on the reference; **the demo shell IS the browser surface's home** — its canvas presenter is the universal SPA of the canvas decision (the same trees render on the Mac presenter), the demo chrome around it is packaging, and the kernel beneath the page is the conformant JS twin with the Rust-core-in-page swap unblocked but unscheduled; iPadOS 26.5 Safari (simulator) boots the desktop to rc, measured 2026-08-29; **live at [christham.net/ipnx-v12](https://christham.net/ipnx-v12/)**; Rust core in wasm is M5 |
| **macOS** | `hosts/macos` | wasmtime 48, Cranelift JIT | **green, 149** (floor 131) headless — full canvas parity (M5); **IPNX.app runs** (M3): windows via winit/softbuffer, acme on screen, `--live` hostfs persistence (M4), `#V` snapshots, and the canvas presenter v0 renders the semantic tree natively (con's transcript verified by capture) |
| **OCI container** | `hosts/oci` | wasmtime, Cranelift (or AOT `.cwasm`) | **green, 149** (floor 131) — `FROM scratch`, musl-static, 62.2MB image, proven on every push by CI (amd64 full suite; the aarch64 image smoke-boots under qemu) |
| **iPadOS** | `hosts/ipados` (M6, **re-aimed 2026-08-30**) | WKWebView (JavaScriptCore, full JIT — sanctioned in the content process); SwiftUI presents | the webview is the engine room, not the display: kernel + binaries inside WebKit (WKURLSchemeHandler serves the bundled dist with real COOP/COEP; offline first boot), `/dev/canvas` crosses the bridge to a native SwiftUI presenter, and the app's file entitlements serve the local filesystem into hostfs (a security-scoped bookmark is a bind); Pulley demoted to fallback research |
| **VSCode** | `hosts/vscode` (M13, designed 2026-08-30) | the extension host's Node (kernel-as-a-library) | unbuilt — FileSystemProvider ≈ 9P mounts the namespace as a workspace; Pseudoterminal is `/dev/cons`; tasks run process files; canvas panels after M5 |
| **microVM** | (M11) | wasmtime over Firecracker/virtio, 9P-over-vsock | aspiration, research-first — the second OCI weight |

Platform constraints that shape everything: the browser needs COOP/COEP for
SharedArrayBuffer (that is `serve.mjs`'s whole job) — and the WebKit story
is measured to the bottom (2026-08-29, twice corrected on the record as the
reduction went deeper): service-worker-supplied COOP/COEP **does** isolate
Safari (the first failure was our register script racing WebKit's slower
controller takeover); the system is fully WebKit-clean (132/132, real
headers, no shim); and the residual wall reduced to a **deterministic WebKit
defect — a module-worker script load that starts while another is in flight
through a service worker always fails** (minimal repro and file-ready report:
`demo/webkit-repro/`; at concurrency 2, exactly every second load dies).
The demo routes around it by serialising worker startup, and **Safari runs
the full suite on the shim host** — M6's WebKit gate: passed;
iOS forbids JIT *and* runtime-loaded AOT (hence Pulley); iPad-Safari's OPFS world is private and
evictable, so real user files arrive only through granted subtrees
(a security-scoped bookmark **is** a bind).

## Where everything lives — the canonical namespace

The boot namespace, **as `/lib/namespace` declares it** (M2, landed
2026-08-29: init carries no bind list — boot is the file's text, read
through `newns()`; the root is implicit). Mount points are prefix-map
entries and need not exist
in any underlying tree (`/proc` has no directory in the rootfs; the bind is
the directory):

| path | served by | contents |
|---|---|---|
| `/` | ramfs (`#M`), seeded from `rootfs/` | V10 permissions enforced |
| `/bin` | seed | the Plan 9 userland — rc, sam, acme, the twenty-four, the harnesses |
| `/rc` | seed | rc's library and `tests.rc`, the shell half of the suite |
| `/lib` | seed | `namespace` (the boot file itself), `pkg/` (registries), `font/` (real subfonts + `*default*`), `python3.14/` (the full stdlib + personality files), `alt/` (union-test fixture) |
| `/etc` | seed | personality-side files as they land (`/etc/passwd` is future, personality-owned) |
| `/tmp` | seed | scratch; wiped per boot |
| `/v10/bin` | seed | the V10 exhibit — TUHS-tape `cat`, `echo` on `libv10` |
| `/mnt` | convention | parking for `mount` targets (`/mnt/profile` is the profile's decided seat) |
| `/dev` | `bind #c` | `cons`; a window replaces it wholesale — `bind '#w/N' /dev` makes a namespace a window (`cons ctl mouse wctl label rgb draw/…`) |
| `/env` | `bind #e` | environment as files |
| `/fd` | `bind #d` | dup by open |
| `/srv` | `bind #s` | posted channels, alive by name |
| `/proc` | `bind #p` | status, ctl (identity transitions — [identity.md](identity.md)), notes |
| `/net` | — | **does not exist yet** (M7); its absence is what the suite's future network tests will probe |

Where the pieces live in the **repository** is the tree in
[implementation.md](implementation.md) — one copy, there.

## The deployment review ledger

Doctrine #5's cadence: a dated, honest entry after each shipped form, and at
least when a milestone changes the story. Amoeba and Plan 9 died of their
deployment wave, not their kernels; the ledger exists so ours is examined on
the record, not assumed. Each review also asks the **external-goods
question** ([virtue-ethics.md](virtue-ethics.md)): has any external good —
attention, adoption, a store's approval — begun steering a decision that
belongs to the practice's internal goods?

### Review 2026-08-29 (at declaration — the baseline)

**The wave we are betting on**: the browser tab, the laptop app, the
container, and the agent sandbox — the claim is that these are this era's
workstation, as the diskless terminal was 1990's.
**What has actually shipped**: developer artifacts only — three green hosts
behind `bash`/`cargo` commands. Zero end-user forms. The bet is entirely
ahead of the evidence, which is the honest reading of a baseline.
**Nearest tests of the bet**: M1 (does the container form find use beyond
CI?) and M3 (does the macOS app survive daily driving by its own author?).
**Standing risk, named**: the agent-sandbox form has the strongest
market-timing claim and the least built substance (it waits on M7–M9); if
the wave passes before `/net` lands, that is this project's Amoeba scenario.
**Next review**: after M1 and M3 land, or 2026-11 — whichever is first.

### Review 2026-08-29 (evening) — the first shipped form

Same day as the baseline: **the public demo shipped** —
[christham.net/ipnx-v12](https://christham.net/ipnx-v12/), the frozen browser
port on GitHub Pages behind a COI service worker, verified live at 132/0.
"Zero end-user forms" is no longer true; one form is in the field, and it is
the cheapest one, aimed at the megaphone personas. What it changes about the
bet: nothing yet — it *arms the instruments* (P2's unsolicited-posts signal,
P3's course-adoption signal). External-goods question, asked: the demo was
built to the personas' needs, not to an audience metric; nothing steered.
**Next review** unchanged.

### Review 2026-08-29 (late) — the demo becomes the developer world

The shipped form deepened past its own plan, driven by Christine's
shims-are-over directive and confirmed by her own hands ("I've been trying
it. seems to work"). **What the field form now is**: one door, the desktop
in seconds, and three real toolchains streaming into the live namespace
behind the visitor — a real cc(1) over clang, THE REAL gc compiler and
linker as guests (the "Go cannot compile on wasm" folklore fell to the
process model), pip installing sha256-verified wheels from the real PyPI
over the `#H` webfs device, the full CPython stdlib, and runnable examples
in the home (python.org's programs verbatim; gobyexample's features).
**What it changes about the bet**: the browser-tab form now demonstrates
the *thesis*, not just the kernel — personalities absorbing unmodified
foreign software — so P2/P3 signals, when they come, will be about the
idea and not the novelty. **Cost honestly named**: ~260MB streamed per
boot; nothing persists (ramfs only — a virtue for a demo, the M4 storage
question for a home). External-goods question, asked: the depth was built
to the user's stated acceptance (python.org, gobyexample), not to
impress-metrics; the one telemetry artifact found (a localStorage debug
ring) was removed the day it was noticed. **Next review** unchanged.

### Review 2026-08-30 — the scheduled review: M1 and M3 have landed

The baseline scheduled this review "after M1 and M3 land, or 2026-11".
M0–M4 landed within two days of the declaration, so it arrives early and
answers the baseline's two named tests honestly.

**What has actually shipped since the last entry**: the container form
(M1 — `FROM scratch`, musl-static, 62.2MB, the floor gated on every push,
an aarch64 image smoke-booting under qemu); boot as a namespace file (M2);
the macOS app (M3); host storage (M4 — `#Z`, `--live`); pkg v1 with the
demo doubling as a live registry and five language toolchains installable
in the field form; the Safari failure root-caused to `http://` (Pages'
`https_enforced` was off — forced on, both repositories); and the
engineering round that closed every recorded deferral, the versioning
layer `#V` and `ar(1)` among them. 143 on all three hosts; CI proves
world, container, arm64 and the amber oracle on every push.

**The baseline's two tests, answered.** *Does the container form find use
beyond CI?* Not yet — CI is its only daily user, and that is the honest
reading: the form exists, the evidence does not. *Does the macOS app
survive daily driving by its own author?* Its first real session **killed
the machine** — a 128GB Mac out of application memory, from unbounded
frame flushes and an unbounded tty buffer — and the fix became
architecture: the credit system, one frame in flight per window
("coalescing changes the rate, only credit changes the bound"). That is
what daily driving is *for*; the app survived its second session.
Sustained daily use is still ahead of the evidence.

**What this round changes about the bet**: three of the wave's four forms
are now built and one is in the field. The canvas decision and the
compatibility goodbye (design.md 2026-08-30) re-aim every form's product
surface at `/dev/canvas` — the demo's tty and raster windows are now
declared interim, which raises a named near-term risk: the shipped demo
shows yesterday's surface while M5 builds tomorrow's. Accepted, because
the exhibit keeps the floor green while the product userland is rebuilt.

**Standing risk, unchanged**: the agent-sandbox form still has the
strongest timing claim and the least substance (it waits on M7–M9). The
versioning layer strengthens that story — snapshot per run, rollback per
run, history unwritable even by eve — but `/net` remains the gate.

**External-goods question, asked**: the round was steered by the author's
directives and by recorded doctrine — the versioning layer exists because
the doctrine said it should, not because a feature chart wanted a row.
Nothing external steered.

**Next review**: after M5 ships the canvas surface, or 2026-11 —
whichever is first.
