# The architecture — invariants and contracts

**Role: what the system *is*, present tense.** The *why* behind every shape here
is argued in [design.md](design.md); the *evidence* lives in
[RESEARCH.md](../RESEARCH.md); the *sequence* is [implementation.md](implementation.md);
the *practice* is [handbook.md](handbook.md); the *deployments and namespace map*
are [platforms.md](platforms.md); *who a user is* — [identity.md](identity.md);
the *history* is [poc.md](poc.md). This document carries no rationale and no
chronology — if a sentence would start with "because," it belongs elsewhere and
appears here as a link. It changes only when a contract changes, in the same
commit as the change.

## The system, in one paragraph

A modified Plan 9 kernel runs as an ordinary userspace process on each platform.
Processes are WebAssembly instances; `exec` is instantiation. Every process has
its own namespace — a mount table mapping paths to file servers — and every
non-process syscall resolves through it. 9P is the only IPC: in-process devices
present the file interface as function calls, and exactly one driver marshals
wire 9P at mount boundaries. Userspaces are libc dialects over the one kernel:
Plan 9's own (`lib9`), the V10 exhibit (`libv10`), a WASI shim, and a measured
modern personality to come. (The full statement and its derivation:
[design.md](design.md).)

## The component map

```
kernel/            the kernel core (Rust): pure state machine — syscalls in,
                   effects out — with its own single-threaded async executor
hosts/macos/       embeds the core over wasmtime/Cranelift; threads as processes
hosts/{oci,ipados,browser}/   the same contract, per implementation.md milestones
poc/supervisor/    the FROZEN JS twin of core + host: the reference
                   implementation and conformance oracle
userspace/         the userspace (graduated at M0): libcs, vendored sources
                   (verbatim), commands, citizens, the rootfs seed, mk.sh,
                   VERSIONS (the measured toolchain, drift-warned)
```

The kernel core and the JS reference implement the same kernel; a host
implements the host contract below; every userspace binary implements the guest
ABI. The conformance suite binds all three.

## The kernel's shape

- **A process** is: a pid, a parent, credentials (uname/ruid pair —
  [identity.md](identity.md)), a namespace, an fd table, a wait queue, a note queue.
- **The namespace** is a per-process mount map walked by longest matching
  prefix. An entry is a **union list** (`bind -a/-b/-c` order); walks try
  elements in order, directory reads concatenate integrally, creates land in
  the element carrying MCREATE. Flagless `rfork` **shares** the parent's
  namespace object; `RFNAMEG` copies it; `RFCNAMEG` starts it empty.
- **The Dev table** — in-process devices, selected by `#` letter, presenting
  the file interface as function calls:

  | dev | serves |
  |---|---|
  | `M` | the root ramfs (seeded at boot; V10 permission enforcement) |
  | `c` | the console — `/dev/cons` (there is no `/dev/tty`) |
  | `\|` | pipes, bidirectional |
  | `m` | **devmnt** — the mount driver, the only wire-9P marshal |
  | `p` | `/proc` — status, ctl (identity transitions), notes, wait |
  | `e` | `/env` |
  | `w` | the window server — `#w/clone` mints windows; a window is a
        directory (`cons ctl mouse wctl label rgb draw/…`) a namespace can
        `bind` over `/dev` |
  | `d` | `/fd` — dup by open |
  | `s` | `/srv` — a posted fd's channel, kept alive by name |

- **Blocking without blocking**: the dispatcher is async end to end. A device
  read may *park* (complete later); in the Rust core a parked operation is a
  completion the executor resumes, and **first-completion-wins is the
  interrupt semantics** — a note winning the race is how a blocked call gets
  interrupted.
- **devmnt** speaks 9P2000 per connection with tagged demultiplexing, and
  **clones a fid (`Twalk`, no names) before every open** so an attach fid is
  never consumed. `exportfs` is its mirror in userspace: it relays wire
  requests into real syscalls, so private binds travel and binaries exec
  across the wire. `Tflush` is handled kernel-side; servers never see it.
- **Symlinks resolve in the walking process's own namespace** — never on the
  server ([design.md](design.md), links decision).
- **Notes** are delivered at the syscall boundary; kill rides note
  permissions (V10's euid rule — [identity.md](identity.md)).

## Contract: the guest ABI

A Plan 9-dialect binary is a wasm32 module that:

- **imports** `env.memory` (the host supplies linear memory) and the kernel
  interface: `env.sys` (the trap gate), `env.forka` (bare fork),
  `env.setj/longj/sjbuf` (setjmp over asyncify), `env.tsave/tjump/tdrop`
  (libthread contexts), and `guard.rfork` (the lazy-fork guard);
- **exports** `_start`, called once on the process's own execution context;
- issues syscalls as Plan 9's trap numbers plus this system's additions
  (60–62 for the link family; the derived list is
  [syscalls.md](syscalls.md)); `read`/`write` are `pread`/`pwrite` at
  offset −1; strings and buffers cross through a per-process transfer
  buffer; errors are `errstr` strings, never numbers.

Fork obligations, chosen per call site:

- **`procrfork(flags, fn, arg)`** — the lazy fork: the child borrows the
  parent's context and must reach `exec` (or exit) inside the guard's extent;
  the guard returns the pid to the parent.
- **Bare `rfork(RFPROC)` / `fork()`** — dual return requires the binary to be
  asyncified (a per-binary build flag, never system-wide); the host snapshots
  memory and both sides rewind.

A WASI-dialect binary is selected by its imports: a module importing
`wasi_snapshot_preview1` (or `wasi_unstable`) gets the WASI shim instead — it
exports its own memory, fd 3 is the single preopen and it is the namespace
root, `rename` is link-plus-remove, and the personality is
`wasi:cli/command` and nothing more.

**A personality is a libc dialect over this one ABI, and there are two kinds.**
A *native* personality (lib9, libv10, the measured `libunix`) presents IPNX's
own chosen surface. A *port* personality supplies a foreign environment —
`libc.a`, its headers, its runtime — so that **unmodified** third-party source
compiles against it (a GNU/glibc personality, a musl personality, a BSD
personality). The port personality is where the adaptation lives: the source
is never patched to match IPNX; the environment is built to match the source.
Both kinds are ordinary userspace over the unchanged kernel — the design's
porting inversion ([design.md](design.md), 2026-08-29). The demo's in-tab `cc`
compiles stock C against a wasi-libc/POSIX port environment, the first
instance.

A personality is *provided* at three layers, each cheap:

1. **The ABI shim** (supervisor code) — the imports a running binary needs.
   The WASI personality is `wasi1.mjs`; carrying both `wasi_snapshot_preview1`
   and `wasi_unstable` makes it two dialects of one personality. A binary's
   stat, inode, cwd and environ semantics live here too.
2. **The target sysroot** (namespace files) — what *compiled* code links
   against: `libc.a`, headers, crt objects in a subtree, pointed at by the
   compiler (`-isysroot`, `-L`). A different port personality is a different
   subtree (musl vs glibc vs BSD); nothing else changes.
3. **The runtime support** (namespace files + environment) — what an
   interpreter or runtime needs at run time: CPython's stdlib tree at
   `/lib/python3.14` plus `PYTHONHOME`, for instance.

Because layers 2 and 3 are *files in a namespace*, **a personality is a
subtree you bind in, and a process chooses its personality by its namespace**
— the founding idea, concrete. The provisioning method is measurement: run
the real program, and each thing it fails on is a missing piece of its
personality (the demo's `wasi_unstable` dialect, real inodes, cwd-aware
paths, populated environ and `PYTHONHOME` were all found this way).

Binaries carry no `.wasm` extension: `exec` walks the caller's namespace for
the path and instantiates the bytes it finds — a freshly built module is
indistinguishable from a shipped one.

## Contract: the host

A host embeds the kernel and provides exactly this, in its platform's terms
(Workers + SharedArrayBuffer in JS; threads in Rust — either way, **a process
gets an execution context; the kernel never blocks**):

- **The mailbox**: per-process shared cells the guest sleeps on
  (`Atomics.wait`-shaped); the host wakes it with the result. Plus the
  transfer buffer for byte-carrying calls, with a copy-out discipline for the
  traps that return data.
- **exec** as module instantiation on a fresh context; **forka** as memory
  snapshot, fresh context over the copy, both sides rewound; **the guard** as
  a frame that catches the child's exec/exit and returns the pid (a host
  function natively; hand-assembled wasm in the JS reference).
- **Console** wiring for `#c` (stdio, a DOM window, a terminal view), a clock,
  and the effect seam outward: the core emits effects (console output, window
  presentation), the host performs them.
- **Presentation is optional** — a headless host is legal and complete; the
  suite reads windows back through `rgb`, not through a screen.

`poc/supervisor/` is the frozen executable statement of this contract; a new
host is written against this section and judged by the suite.

## Contract: the wire

- **9P2000, the only version.** Extensions are *minted above every dialect's
  range*, never squatted: `Tlink`/`Rlink` 128, `Tsymlink` 130, `Treadlink`
  132. A server without them answers `Rerror` and the client degrades.
- Reused bit positions from 9P2000.u, not its protocol: `DMSETUID`,
  `QTSYMLINK`/`DMSYMLINK`.
- **Identity crosses at attach**: every wire mount carries the mounting
  process's `uname` in `Tattach`; the server applies its own policy to that
  name ([identity.md](identity.md) — "the namespace unions services; it cannot union
  their trust").

## Contract: the conformance suite

- **The 131 are the permanent floor.** `init` (pid 1) runs the suite from the
  rootfs; *a host is real when init exits 0*. The frozen reference must stay
  green forever: `bash poc/run.sh`.
- **The suite only grows**, and new tests **self-skip by probing the
  namespace** for the features they need, so one rootfs serves every host
  including the frozen oracle. Tests land in the same commit as the feature.
- Assertions are about **semantics, not one host's layout** (raster tests
  assert inked bands, not pixel positions).

## Contract: what is trusted

- **The engine's sandbox is the isolation primitive.** A guest reaches
  exactly its imports and its linear memory; everything else it can touch is
  what its namespace resolves. There is no second barrier: on hosted rungs
  the host OS process stands behind the engine, on the microVM rung nothing
  does — a runtime escape there is a whole-system escape, recorded plainly
  ([design.md](design.md), OCI decision).
- **Trusted**: the kernel (both implementations), the host shims, the build
  toolchain's output.
- **Untrusted by construction**: every guest (its authority is its
  namespace, nothing else), every wire-mounted server (the client applies
  the protocol and its own policy — "the namespace unions services; it
  cannot union their trust", [identity.md](identity.md)), and every wire
  client (per-attach identity; the server decides what the name may do).
- **A demo visitor** executes guests in their own browser only; the hosting
  is static files; nothing a visitor does reaches another visitor or a
  server.
- The future credential mechanism (M8) uses boring, reviewed cryptographic
  primitives only — cleverness is out of contract.

## Deliberately not architecture

Per-host internals (how a shim spawns threads or paints), window *policy*
(placement, menus — the contract is the files a window serves), the modern
personality's surface (measured against its benchmarks when built —
[implementation.md](implementation.md) M10), and on-disk formats (**the system
never learns one** — durability is always another filesystem reached through
9P or a host device; [design.md](design.md), storage invariant).
