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
| network, once | — | the CPython wasi build (26 MB), cached in `build/` |

## Build and run

```sh
bash poc/mk.sh                             # build every guest binary
bash poc/run.sh                            # boot the frozen reference on Node
bash poc/run.sh -i                         # …to an interactive rc (EOF ends)
node poc/serve.mjs                         # the same kernel in a page (?i = interactive)
cargo run --release -p host -- poc/rootfs  # the Rust core under wasmtime
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

Binaries land in `poc/rootfs/bin` (and `/v10/bin`) with **no `.wasm`
extension**. `poc/build/` and the generated rootfs subtrees are gitignored.

## How to add things

**A command.** Own code goes in `cmd/`; Plan 9 source is vendored verbatim
under `plan9/sys/src/…` with its batch NOTICE, shim headers beside it, and a
`mk.sh` stanza (copy the nearest existing one). If it forks bare, add it to
`ASYNCIFY`. Ship a test in the same commit.

**A test.** Shell-visible behaviour goes in `poc/rootfs/rc/tests.rc`;
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

## Conventions

Prose is British-inflected, em-dashed; numbers are load-bearing and carry
provenance; guest C is Plan 9 style (tabs, `nil`, no const clutter). The
full working conventions — including the document roles and the
same-commit rules — are in `CLAUDE.md`.
