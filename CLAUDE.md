# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repository is

**ipnx-v12**: **a modified Plan 9 kernel hosted as an ordinary userspace process** on
macOS, iPadOS and the browser; **9P as the only IPC**; **per-process namespaces**;
**WebAssembly as the executable format**; and a **Research Unix Tenth Edition personality**
alongside Plan 9's own userland.

The thesis, from the README: *the Plan 9 authors' biggest mistake was not preserving Unix
semantics, and it was a choice rather than an oversight — "Compatibility was not a
requirement for the system" — which is what makes it undoable.* With V10 rather than POSIX,
because every limitation APE confesses is a POSIX.1-1990 feature V10 does not have.

The architecture runs, boots to a shell, forks both ways, and speaks its protocol in
both directions: `poc/` is a working slice (hosted kernel in Node and the browser from
one neutral core, freestanding-C wasm guests, per-process namespaces with union
directories, the lazy-fork resume *and* the asyncify bare fork, pipes, a writable ramfs
with V10 permission enforcement, the uid model running (docs/uid.md), a minimal `rc`
with subshells, wire 9P at the mount boundary, exportfs serving a guest's namespace
back out, and the window server: `#w` mints windows, `bind '#w/N' /dev` makes a
namespace a window, `/dev/draw` is a real per-window file with text (`y i l s` and an
8×8 font of our own authorship), `win rc` is a shell in a browser window, and the
link/symlink family lands as the V12 additions — seventy-eight acceptance tests).
Everything else is design documents.

## Commands

Build the guest binaries (requires wasi-sdk at `~/.local/opt/wasi-sdk` and binaryen's
wasm-opt at `~/.local/opt/binaryen` — override with `WASI_SDK`/`BINARYEN`; Apple's clang
has no wasm backend). `mk.sh`'s `ASYNCIFY` list names the binaries that may bare-fork:

```bash
bash poc/mk.sh
```

Boot the kernel — init (pid 1) runs the acceptance tests, prints seventy-eight PASS
lines, exits 0:

```bash
bash poc/run.sh
```

Boot to an interactive `rc` on the console (EOF shuts down):

```bash
bash poc/run.sh -i
```

Serve the browser port — the same kernel in a page (the server only supplies the
COOP/COEP headers SharedArrayBuffer needs; `?i` boots interactive):

```bash
node poc/serve.mjs
```

Node ≥ 22 (`worker_threads`, SAB, wasm `try_table` exception handling — the legacy EH
encoding is *rejected* by these engines, so any new wasm emission must use `try_table`).
`poc/build/` and `poc/rootfs/bin/` are generated and gitignored. Guest binaries carry no
`.wasm` extension: exec walks the namespace for `/bin/cat`, and a freshly produced module
is indistinguishable from a shipped one.

## The documents

| | |
|---|---|
| `README.md` | public overview — why this repository is separate from the parent, the precedents, status |
| `RESEARCH.md` | **living evidence base** — every finding with provenance: Plan 9's call list, `rfork` flags verbatim, APE's limits, the fork-resume mechanism and its measurements, WASI phases, the toolchain recipe (§9.4), V10 measurements |
| `docs/v12-plan.md` | **the spec** — scope, decisions taken (with dates), open questions, PoC status |
| `docs/syscalls.md` | **the derived call list** — Plan 9's 40 live calls dispositioned for V12; V10's 68 routines mapped onto them |
| `docs/uid.md` | **the uid model** — the item APE called impossible, decided and running: kernel credentials, `/proc` ctl transitions, `DMSETUID`, the two enforcement regimes |
| `poc/README.md` | what the PoC proves, its layout, its deliberate v0 deviations |

Findings go in RESEARCH.md with provenance; decisions go in the plan; both are living.
Keep them consistent — the architecture statement appears in README, RESEARCH TL;DR and
the plan, deliberately, and a change to it changes all three. Prefer a pointer over a copy
for anything else ("a list that appears twice will disagree").

## The parent repository

[ipnx](https://github.com/ChristineTham/ipnx) restores Research Unix under full-system VAX
emulation. It is checked out beside this one at `../ipnx`, but referenced from the docs
only by URL — deliberately.

**The V10 source tree is not copied here and must not be.** Every V10 number quoted
(61,072 kernel lines, 239 `fork(` sites, `sh/xec.c:432`, `sysent.c` slot 66) was measured
in `../ipnx` and is recorded **as data** because it cannot be re-derived here. A new
measurement is taken in `../ipnx` against `v10/usr/src/…` and recorded with file-and-line
provenance in RESEARCH.md.

## The decisions taken — do not re-derive them

Each has its reasoning and citations in RESEARCH.md / the plan. Reopening one requires new
evidence, not a fresh opinion.

- **The kernel is Plan 9's, not V10 retargeted.** ~5% of V10's 61,072-line kernel has
  anything to say on a target with no MMU and no hardware. The surgery runs Unix-onto-
  Plan 9 (addition), never the reverse (eviction) — and V10's `chmk $n` trap numbers
  collide with Plan 9's on the same instruction (`.set open,5` vs `DUP` 5).
- **Hosted, not native, not emulated** — Inferno `emu`'s architecture with wasm in place
  of Dis, forced by iOS (no child processes, no JIT).
- **9P is the system interface; WASI is a shim** (`wasi:cli/command` only). WIT's typed
  interfaces and 9P's uniform untyped one do not compose.
- **9P2000.** `version(5)`: the only defined version; negotiation built in; nothing here
  needs wire compatibility with original 9P.
- **Wire 9P at boundaries, a Dev table inside** — Plan 9's own kernel shape: devices
  present the file interface as function calls, only the mount driver marshals 9P.
- **The kernel call list is derived** (`docs/syscalls.md`): of Plan 9's 40 live calls, 29
  never leave the supervisor; ten are one 9P message each; `mount` is the boundary itself.
  `Twalk` has no syscall, and `seek` is fd-table state, not a message.
- **Class-B calls return as libc functions, never syscalls** — `chmod`/`fchmod`/`chown`/
  `fchown`/`utime` collapse into `wstat` (V10 has no `rename`; renaming is `link`+`unlink`
  in userland). 40 of V10's 68 routines — 59% — are direct or library-only; counts are of
  routines, not `sysent.c` rows (`lseek`/`seek` and `gtime`/`ftime` pair up).
- **The lazy fork's resume mechanism and its bound** (RESEARCH §5.2): the child's `exec`
  throws; a hand-assembled `try_table`/`catch_all` guard catches; the supervisor restores
  the `[0, sp)` shadow-stack region saved at fork; the guard returns the pid. The catch
  frame must be live when the child execs, so the child's pre-exec code runs inside the
  guard's extent — **`procrfork(flags, fn, arg)`**, Plan 9's thread-library shape. Bare
  dual-return `rfork(RFPROC)` is asyncify's case, realised as a per-binary flag (never
  system-wide): `wasm-opt --asyncify` with instrumentation confined to paths reaching
  `env.forka` (rc costs +5.5%); the worker unwinds, snapshots the whole memory, the
  supervisor spawns a fresh Worker over the copy, both sides rewind — pid and 0. rc's
  subshells run this way; its exec'ing forks stay on the guard path.
- **Syscall transport: a Worker is a process** (§5.3) — per-process SAB mailbox,
  `Atomics.wait` in the Worker, kernel never blocks. Browser deployment needs COOP/COEP;
  Node needs nothing. Guest memory stays unshared (so no atomics/shared-memory flags in
  guest builds); sharing it is a later optimisation.
- **One guest substrate** (wasm everywhere); base is **Plan 9 4th edition as reference,
  9front consulted** (both MIT). **`/dev/tty` does not exist** — `/dev/cons`, aliased in
  the personality's libc.
- **`/dev/draw` stays an actual file, per window, per namespace** — the one place this
  system can out-Plan 9 plan9port, **now demonstrated**: `supervisor/devwsys.mjs` is the
  window server's kernel half (rio's *interface*), `draw.mjs` its raster engine
  (draw(3)'s `b d f L e E y i l s v` — text included, via `libc/font8x8.h`, our own
  authorship), `cmd/win.c` rio's spawn. Next: the `sam` port, with `acme` the real
  test. **Self-hosting is not a goal** (`/cc` as
  file server makes compilation a capability).

- **The uid model is decided and running** (`docs/uid.md`): mutable per-process
  credentials in the kernel, names canonical (numbers are the personality's, via
  `/etc/passwd`), transitions through `/proc/<pid>/ctl` under the eve/ruid rule — no new
  syscalls — 9P2000.u's `DMSETUID` bit at exec, V10 enforcement in in-process devices,
  per-attach identity stamped on wire mounts. Open: uid.md's D1–D4 measurements in
  `../ipnx`.
- **Links landed as the V12 additions**: kernel traps 60–62 (`lstat` is a stat flag),
  minted wire types 128/130/132 above every dialect's range (strangers `Rerror`,
  clients degrade), 9P2000.u's `QTSYMLINK`/`DMSYMLINK` bits, and V10's rule — **the
  kernel resolves symlinks in the walking process's namespace**, since no server knows
  the client's namespace.

## The PoC's shape (poc/)

One platform-neutral kernel, two hosts: `supervisor/kernel.mjs` runs unmodified on Node
(`supervisor/main.mjs` + `worker.mjs` shims) and in the browser (`browser/main.mjs` +
`browser/worker.mjs`, served by `serve.mjs`, whose only job is COOP/COEP). Everything in
the supervisor speaks `Uint8Array` (`bytes.mjs`), never Buffer — a browser `TextDecoder`
refuses SAB-backed views, so `bstr` copies them (measured, RESEARCH §5.3).

`supervisor/kernel.mjs` is the kernel (proc table, namespaces as per-proc mount maps with
longest-prefix walk, refcounted channels and fd tables closed at exit, dispatch,
rfork/exec/exits/await). `supervisor/guestcore.mjs` is the guest runner: the mailbox
protocol, and the fork guard — **the only hand-written wasm in the system**, emitted as
bytes with computed section sizes. `supervisor/devs.mjs` holds the devices (writable
ramfs, cons wired to host stdio, bidirectional pipes); a device read may **park** (return
undefined, complete later via `ctx.done`) — that is how pipe and console reads block their
caller without blocking the kernel. `libc/` is Plan 9-shaped freestanding C (Plan 9's own
trap numbers; `read`/`write` are `pread`/`pwrite` at offset −1). Syscalls carrying
strings/buffers copy through a per-proc transfer SAB. A lazy-fork child *borrows the
parent's Worker*: the supervisor routes syscalls arriving on the parent's mailbox to the
child's proc record (`borrower`) — that is how a child's `bind` lands in the child's
namespace while sharing the parent's stack. A mount-table entry is a **union list**
(`bind -a/-b/-c`): walks try elements in order, directory reads concatenate integrally,
creates land in the MCREATE element. `cmd/exportfs.c` is devmnt's mirror — it serves its
own namespace over wire 9P by relaying every request into real syscalls, so private
binds travel and binaries exec across the wire; `libc/lib9p.[ch]` is the guest marshal
vocabulary both servers share. `cmd/rc.c` is the shell: every fork is
`procrfork(RFFDG, fn, arg)` (pipeline stages, `` `{...} `` captures — the latter exec
`/bin/rc -c` so nothing ever forks without exec'ing), and subshells/functions are refused
with an error naming the asyncify path. `supervisor/mnt9p.mjs` is devmnt — the only place
the kernel marshals wire 9P: `mount(fd)` negotiates Tversion/Tattach, operations become
tagged messages demultiplexed per connection, and a chan is **cloned (`Twalk`, no names)
before open** so the attach fid is never consumed. `cmd/hellofs.c` is the proof server:
9P2000 on fd 0, mounted over a pipe. The kernel dispatcher is async throughout; devices
may instead *park* a read (return undefined, complete via `ctx.done`).

## Conventions

- **Cite primary sources, quote verbatim in blockquotes; numbers are load-bearing** —
  line counts, call counts and file:line references carry the argument, and each traces to
  a measurement or source. State the decision, then the constraint that forced it.
- **Measure rather than assume** — engine capabilities here contradicted folklore (legacy
  EH gone, `try_table` on); RESEARCH.md records measured tables with dates.
- **No Plan 9, plan9port, APE or Research Unix source is copied in.** `LICENSE` is MIT
  *inherited* from Plan 9 (Foundation transfer, March 2021); Plan 9-derived material keeps
  the Foundation's notice; Research Unix is a separate estate (Nokia's 2017 covenant),
  explicitly not covered. A V12 image mixes the estates — answered before code; revisit
  LICENSE then.
- Prose is British-inflected (`licence`, `rasterise`), em-dashed; tables carry comparisons.
  Guest C is Plan 9 style (tabs, `nil`, no const clutter); the build silences the
  builtin-redeclaration warnings that style causes.

## Current state (2026-08-26)

Documents and PoC written today; sixty-five acceptance tests pass on Node
(`bash poc/run.sh`) **and in the browser** (`node poc/serve.mjs` → `/browser/`, measured
in Chrome 148); `?i` boots to an interactive `rc` where `win bounce &`, `win scribble &`
and `win rc &` open live windows (drag by title bar; click to focus; typing lands in the
focused window's cons). Milestones on `main`: initial commit, rc, wire 9P, asyncify, the
browser port, unions + exportfs, the uid model, the window server, text in draw, the
link/symlink family. Next: the `sam` port (libframe over the existing draw messages —
note this means bringing plan9port source in, a licensing/convention decision for the
user), the native WasmKit host, and uid.md's D1–D4 measurements in `../ipnx`.
