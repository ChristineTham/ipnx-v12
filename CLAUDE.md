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
with V10 permission enforcement, the uid model running (docs/uid.md), **the REAL
`rc`** — bison over its own `syn.y`, asyncified, prompting when fd 0 is `/dev/cons` —
wire 9P at the mount boundary, exportfs serving a guest's namespace
back out, and the window server: `#w` mints windows, `bind '#w/N' /dev` makes a
namespace a window, `/dev/draw` is a real per-window file with text (`y i l s` and an
8×8 font of our own authorship), `win rc` is a shell in a browser window, and the
link/symlink family lands as the V12 additions, and **both real userspaces have their
real libraries**: `libp9.a` is ~150 files of genuine 4th-edition libc/libbio/libregexp
over one platform shim (`u.h`), twenty-four real commands and the real `rc` ride it,
V10 `cat`/`echo` sit in `/v10/bin` on `libv10`, and the kernel carries what rc needs:
notes at the syscall boundary, `unmount`, honest rfork flags, `#d`, and real
`setjmp/longjmp` over the asyncify machinery — which is what lets **the real `sam`**
run in terminal mode (`sam -d`): structural regexps, the `x/c/s/i` loop, and the
buffer piped through real commands — and **the real libdraw draws**: `geninitdraw`
speaks `/dev/draw/new` against `#w`, `getwindow` allocates a screen and window view,
and the real default font lands glyphs through `y/i/l/s` — libthread runs (the real
thread.h API as a wasm platform layer: coroutines over saved asyncify contexts,
channels delivering through off-stack slots, blocking reads that park a thread while
the process keeps scheduling) — and **the real sam EDITS IN A WINDOW**: `win sam &`
boots sam over samterm, libframe renders the command window through the device's
`x` (string-with-background), typed text crosses the mesg protocol and sam's answer
renders back — 120 acceptance tests). Everything else is design documents.

## Commands

Build the guest binaries (requires wasi-sdk at `~/.local/opt/wasi-sdk`, binaryen's
wasm-opt at `~/.local/opt/binaryen` — override with `WASI_SDK`/`BINARYEN` — and the
host's `bison` for vendored yacc grammars; Apple's clang has no wasm backend). `mk.sh`'s
`ASYNCIFY` list names the binaries that may bare-fork. Compiling vendored libc source
REQUIRES `-fno-builtin`: clang's libcall recogniser otherwise rewrites strlen's own body
into a self-call (measured; RESEARCH §9.4). Two more findings are load-bearing
(RESEARCH §9.5): `--table-base=4096` keeps wasm's small function-table indexes disjoint
from rc's operand integers (`codefree` tells ops from operands by comparing `.f` slots),
and `weaken.mjs` restores common-symbol semantics for rc.h's pre-ANSI tentative
definitions — the wasm backend refuses `-fcommon` and wasm-ld's
`--allow-multiple-definition` keeps the *first* definition even over a later
initialized one (measured: plan9.o's zero `havefork` beat `havefork.c`'s `= 1`):

```bash
bash poc/mk.sh
```

Boot the kernel — init (pid 1) runs the acceptance tests, prints 120 PASS lines,
exits 0:

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

**The V10 measurement tree lives in `../ipnx`, not here.** Every V10 number quoted
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
  `env.forka` (the real rc costs +103% — its fmt-driven error paths make nearly every
  function transitively reach an indirect call, so the confinement barely confines);
  the worker unwinds, snapshots the whole memory (the fork-time `__stack_pointer`
  travels too — globals are not part of the snapshot), the supervisor spawns a fresh
  Worker over the copy, both sides rewind — pid and 0. Every fork the real rc makes
  runs this way.
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

**The userspace objective**: real Plan 9 and real V10 userspace, compiled to wasm from
their own trees, side by side — each on its own libc.a rewritten over the kernel
(`libc/` is lib9; `v10/lib` + `v10/include` is libv10). Vendored sources under
`poc/plan9/sys/` and `poc/v10/usr/` are **verbatim — never edit them**; each batch
carries a NOTICE with provenance (Foundation MIT for Plan 9; Nokia's covenant for V10,
with LICENSE's scope note kept in step). The shim headers beside them are ours. V10
compiles want `-std=c89 -fno-builtin` (the libcall optimiser rewrites bare fprintf into
fwrite) with K&R implicitness left authentic; growing both userspaces command by command
is the standing work.

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
vocabulary both servers share. **The shell is the real rc** —
`plan9/sys/src/cmd/rc/` compiled verbatim (bison regenerates `syn.y` into `x.tab.h`'s
namesake), linked by mk.sh's own rule: `weaken.mjs` restores common-symbol semantics
for rc.h's tentative definitions, `--table-base=4096` keeps function-table indexes
disjoint from `codefree`'s operand integers (both RESEARCH §9.5), and wasm-opt
asyncifies the result so every bare `fork()` genuinely returns twice — pipelines,
subshells, captures, `fn`, `while`, `switch`, and a prompt whenever `fd2path(0)` ends
in `/dev/cons`. `supervisor/mnt9p.mjs` is devmnt — the only place
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
- **Upstream source may be brought in; provenance and notices travel with it.** Plan 9,
  plan9port and APE are MIT (Foundation transfer, March 2021) and the plan calls APE "a
  source to cut down" — importing such source is expected, keeping the Foundation's
  notice per `LICENSE`. Research Unix source is governed by the covenant reasoning in
  the parent repository; when any lands here, update `LICENSE`'s scope note in the same
  commit. Separate from licensing: **the V10 tree used for measurements stays in
  `../ipnx`** so every quoted number keeps file-and-line provenance (RESEARCH.md's own
  rule).
- Prose is British-inflected (`licence`, `rasterise`), em-dashed; tables carry comparisons.
  Guest C is Plan 9 style (tabs, `nil`, no const clutter); the build silences the
  builtin-redeclaration warnings that style causes.

## Current state (2026-08-27)

120 acceptance tests pass on Node (`bash poc/run.sh`) **and in the browser**
(`node poc/serve.mjs` → `/browser/`, measured in Chrome 148); `?i` boots to **the real
Plan 9 rc** — pipelines, subshells, `` `{...} `` captures, `fn`, `while`, `switch` — and
`win rc &` opens a shell window that prompts because rc's own `Isatty` finds
`/dev/cons` by `fd2path`. Milestones on `main`: initial commit, rc, wire 9P, asyncify,
the browser port, unions + exportfs, the uid model, the window server, text in draw,
the link/symlink family, D1–D4 measured, the real Plan 9 userspace grown to a system,
the real rc running the whole suite over a kernel readied for it (notes with
V7-boundary delivery, `alarm`, `unmount`, honest rfork flags, `#d`, `..` in walks),
**the real sam in terminal mode** — `sam -d` with structural regexps and `|`
filters through real commands, carried by real `setjmp/longjmp` built on the asyncify
machinery — and **the real libdraw** (with libframe compiled beside it) drawing
through `#w`: the device grew screens (`A`), window views (`b` on a screen, one
backing store), clipping (`c`), and channel-correct uploads (`y` decodes GREY1
fonts per draw(6)), verified by drtest's seven pixel checks — and **libthread**:
thread.h's API implemented as this platform's layer (its real core is per-arch
scheduler assembly), coroutines as saved asyncify contexts (frames + stack pointer +
shadow-stack region), `proccreate` collapsing to `threadcreate`, and the kernel's
AREAD/IOWAIT letting a read park one thread while the scheduler runs the rest. The
load-bearing rule, learned the hard way: a suspended coroutine's stack is dead
storage, so channels register, stash send values, and deliver through off-stack
per-thread slots (V10 growth waits for the parent project's ANSI conversion).
Next: samterm — the real editor's screen — then the native host — **macOS first,
then iPad** (the user's platform order).
