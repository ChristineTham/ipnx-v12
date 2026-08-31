# The handbook — building, running, extending, debugging

**Role: the *how* — the practice.** What the system is: [architecture.md](architecture.md);
why: [design.md](design.md); the sequence: [implementation.md](implementation.md);
where it runs and where things live: [platforms.md](platforms.md). This document
tells you how to work on it. (CLAUDE.md at the repository root is the
agent-facing operational digest of the same practice; this is the human manual.
When they disagree, one of them has a bug — fix it in the same commit.)

## Prerequisites

| tool | where | note |
|---|---|---|
| wasi-sdk | `~/.local/opt/wasi-sdk` (override `WASI_SDK`) | clang with the wasm backend; Apple's clang has none |
| binaryen | `~/.local/opt/binaryen` (override `BINARYEN`) | `wasm-opt --asyncify` |
| bison | host | regenerates vendored yacc grammars (rc's `syn.y`) |
| go | host | the `GOOS=wasip1` citizen |
| Node ≥ 22 | host | `worker_threads`, SAB, `try_table` |
| Rust (stable) | host | the kernel core and hosts; workspace at the repo root |
| network, once | — | the CPython wasi build (26 MB), cached in `userspace/build/` |

## Build and run

```sh
bash userspace/mk.sh                       # build every guest binary
bash poc/run.sh                            # boot the frozen reference on Node
bash poc/run.sh -i                         # …to an interactive rc (EOF ends)
node poc/serve.mjs                         # the same kernel in a page (?i = interactive)
cargo run --release -p host -- userspace/rootfs  # the Rust core under wasmtime
```

Green is: init (pid 1) prints the suite's `PASS` lines — the floor is 131 —
and exits 0. Any other exit is a failure even if PASS lines appeared.

## The working rules

- **Every milestone ends in an artifact someone can hold** — a container
  image, an app, a bootable profile — not a refactor. The PoC's cadence (each
  commit demonstrably runs more of the world) continues.
- **The suite is the merge bar.** The 131 are the permanent floor on every
  host; the count only grows. New tests probe for the features they need and
  self-skip where absent, so one rootfs serves every host including the
  frozen reference. A feature's tests are written with the feature, never
  after.
- **Vendored sources stay verbatim** — the derivation layer (shim headers,
  `sed` into `build/`) is ours; the vendored trees are never edited.
  Provenance and notices travel with any new import.
- **A contract change lands in [architecture.md](architecture.md) in the same
  commit** — that document is present-tense by rule, so code and contract
  never describe two different systems.
- **Decisions are consumed, not re-derived.** They live in
  [design.md](design.md) with dates; reopening one requires new evidence,
  in the log, first.
- **A commit message claims only what its diff contains** — checked against
  the tree, not the intention ([virtue-ethics.md](virtue-ethics.md)'s worked
  example: a failed patch script, a message that claimed it landed).

## The build system's shape

`mk.sh` compiles the userspace: our C (`libc/`, `cmd/`), the vendored trees
through a **derivation layer** (`sed` into `build/…` — the vendored source is
never edited; the derived copy is disposable), libraries as archives
(`libp9.a`, `libv10.a`), then links. Load-bearing flags, each measured —
follow the links before touching them:

- `-fno-builtin` on all vendored libc (RESEARCH §9.4 — clang's libcall
  recogniser rewrites strlen's own body into a self-call without it).
- `--table-base=4096` (RESEARCH §9.5 — keeps function-table indexes disjoint
  from rc's operand integers).
- `weaken.mjs` (RESEARCH §9.5 — restores common-symbol semantics for
  pre-ANSI tentative definitions; wasm-ld keeps the *first* definition).
- The `ASYNCIFY` list names the binaries that may bare-fork; membership costs
  ~2× size on that binary (rc measured +103%) and is never system-wide.
- V10 compiles are `-std=c89 -fno-builtin`, K&R implicitness left authentic.

Binaries land in `userspace/rootfs/bin` (and `/v10/bin`) with **no `.wasm`
extension**. `userspace/build/` and the generated rootfs subtrees are
gitignored. `userspace/VERSIONS` records the measured toolchain; `mk.sh`
warns (never fails) when what it finds has drifted.

## How to add things

**A command.** Own code goes in `cmd/`; Plan 9 source is vendored verbatim
under `plan9/sys/src/…` with its batch NOTICE, shim headers beside it, and a
`mk.sh` stanza (copy the nearest existing one). If it forks bare, add it to
`ASYNCIFY`. Ship a test in the same commit.

**A test.** Shell-visible behaviour goes in `userspace/rootfs/rc/tests.rc`;
kernel-level assertions in init's C; subsystem harnesses as their own
`cmd/*test.c`. A test for a feature the frozen reference lacks must
**self-skip by probing the namespace** (walk to the file or device it needs)
so one rootfs serves every host including the oracle.

**A vendored batch.** Verbatim, always — fixes happen in shim headers or the
derivation `sed`, never in the tree. The batch carries a NOTICE with
provenance; Research Unix imports update `LICENSE`'s scope note in the same
commit.

**A device or kernel feature.** Lands in the Rust core (`kernel/`) — the JS
kernel is frozen and does not grow it. The feature's tests self-skip on the
reference; the contract lands in [architecture.md](architecture.md) **in the
same commit** (its standing rule); a finding worth keeping lands in RESEARCH
with provenance.

**A host.** Implement the host contract
([architecture.md](architecture.md) — mailbox, exec-as-instantiate, forka
snapshot, the guard, console, optional presentation), point it at the rootfs,
and run the suite; `poc/supervisor/` is the executable statement of the
contract to read alongside. A host is real when init exits 0.

## Running a service (M12's local stage)

A **process file** is a directory: `cmd` (path, then one argument per
line), and optionally `namespace` (namespace(6) subset, applied whole —
start from `/lib/namespace`), `packages` ("name [version]" per line,
pkg-verified), `env` (`NAME=value`), `user`, `replicas`, `health`.
`run /path/to/spec` instantiates it once. For supervision:

```
svc &                              # posts /srv/svc
mount svc /n/svc
echo start web /path/to/spec 3 > /n/svc/ctl
cat /n/svc/web/status              # want 3 have 3 pids …  (reading reconciles)
echo scale web 5 > /n/svc/ctl
echo stop web > /n/svc/ctl
```

Replicas spawn `RFNOWAIT` (no zombies); a dead one is noticed because its
`/proc/<pid>/status` no longer opens, and the reconciler replaces it — on
every ctl write, every status read, and an idle tick. `kill <pid>` posts
the note; `ps` lists everyone. The whole suite is userspace over the
kernel's existing primitives — the frozen oracle runs it unmodified.

## Redeploying the demo

The live demo ([christham.net/ipnx-v12](https://christham.net/ipnx-v12/)) is
the `gh-pages` branch serving `demo/dist`. After a userspace change worth
publishing: `bash userspace/mk.sh && bash demo/build.sh`, then publish
`dist/` as a SINGLE parentless commit — in a worktree: `git add -A`, then
`git push -f origin $(git commit-tree $(git write-tree) -m "deploy"):gh-pages`
— so gh-pages always holds exactly one snapshot (26 accumulated ~250MB
snapshots were collapsed this way, 2026-08-29). Two size rules, both
measured: **Git LFS is not an option** — GitHub Pages serves LFS *pointer
files*, not content, so an LFS'd overlay would break the demo silently —
and every published file stays under 50MB (GitHub's warning line;
`demo/build.sh` packs overlays in size-budgeted parts, splits any single
oversized file into base64 pieces the shell folds back, and fails the build
if a part crosses the line).
Pages rebuilds in under a minute. The COI service worker in the bundle
supplies COOP/COEP; verify in a real browser — embedded panes may refuse
service workers. **Wait out the CDN before testing**: Pages' edge can serve
the previous deploy for a minute or two after the build reports green, and a
page loaded in that window poisons the service worker's fresh per-stamp cache
with the stale modules (measured 2026-08-29: a boot mixed the new shell with
the old kernel — "unknown device #H"). Before driving the live site, curl a
just-changed file and grep for a marker from the new deploy; a plain reload
heals an already-poisoned tab once the edge settles.

## Debugging

- **Start from the suite's own output**: every FAIL names its assertion; init's
  exit status catches silent deaths. Run the interactive boot (`run.sh -i`)
  and drive the failing path by hand in rc.
- **errstr is the error channel** — a wrong `errstr` string is usually the
  whole diagnosis (the kernel's mapping regexes are keyed on those strings;
  truncation and misquoting have both caused real bugs).
- **Raster assertions print ASCII dumps on failure** (the `dtest`/`acmetest`
  pattern: an 80-column downsample of the window). Read the picture before
  the code — it distinguishes "nothing painted" from "painted elsewhere".
  Assert semantics (inked bands), never one host's pixel layout.
- **Divergence between hosts is an oracle problem**: reproduce on the frozen
  reference first; whichever side disagrees with it is wrong (and if the
  *reference* is wrong, that is a finding for RESEARCH, not a patch — it is
  frozen).
- **Wire trouble**: `exportfs` + a second instance reproduces most devmnt
  issues without a network; tag/expect mismatches and un-cloned fids are the
  historical culprits.
- **Before blaming a platform, reduce.** A claim of "engine bug" requires a
  minimal repro that indicts them or you — the Safari case (2026-08-29) took
  three walls: two were ours, and the third became file-ready only when a
  70-line reduction (`demo/webkit-repro/`) showed the deterministic trigger
  a whole-system symptom had hidden.

**Debugging the demo in a driven browser pane**: browsers throttle
`requestAnimationFrame` in hidden panes, and the canvas presenter renders
on rAF — so DOM queries made while the pane is backgrounded read the
UNPAINTED tree and report features as absent that worked fine. Front the
pane (a screenshot does it) before asserting on rendered state; three
separate "bugs" in one evening were this one ghost.

## Conventions

Prose is British-inflected, em-dashed; numbers are load-bearing and carry
provenance; guest C is Plan 9 style (tabs, `nil`, no const clutter). The
full working conventions — including the document roles and the
same-commit rules — are in `CLAUDE.md`.


## Proving the `/dev/window` round trip

The control interface's host half has its own headless check — it mints a typed
window from rc, hands the declared chrome to a host, clicks the `ipnx:` control
from the host side, and watches a plain guest read the click back out of the
window's events. **No emca is involved**, which is the point:

```bash
node demo/supervisor/winproof.mjs userspace/rootfs
```

Exits 0 on the round trip, 1 on a 20-second timeout.

Its sibling proves the other half — that **emca** places windows, and that the
placement reaches a surface, which is the part rc cannot see:

```bash
node demo/supervisor/emcaproof.mjs userspace/rootfs
```

It boots the unmodified default workspace (`/rc/emca`) and asserts each window
landed in the pane its type declares, that the toolbar arriving at the host is
emca's core set merged with the type's extras, and that `Put` is absent because
nothing is dirty. The two proofs are a pair: **with** emca the window lands where
emca says, **without** it the surface falls back to the type's default. That is
the degrades-correctly rule, in code.

**Both need the wasm kernel built first** — and this is a real trap, because
`cargo build --release` builds the NATIVE kernel while these harnesses (and the
whole browser surface) load `target/wasm32-unknown-unknown/release/browserhost.wasm`.
Editing `kernel/src/lib.rs` and re-running a proof without this rebuilds nothing
the proof can see, and the symptom is a change that plainly landed and plainly
is not running:

```bash
cargo build --release --target wasm32-unknown-unknown -p browserhost
```

`bash demo/build.sh` does it as its first step, so a full demo build never has
the problem.
