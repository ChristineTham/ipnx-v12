# The architecture — invariants and contracts

> **PROPOSED — not reviewed.** Claude wrote this. Nothing in it is endorsed, and
> nothing in it approves a deviation from Plan 9. What is built is
> [when.md](when.md).

**Role: the *what*.** What the system is, present tense — invariants and
contracts. The *why* behind every shape here
is argued in [design.md](archive/design-log-claude-written.md); the *evidence* lives in
[RESEARCH.md](../RESEARCH.md); the *sequence* is [implementation.md](implementation.md);
the *practice* is [handbook.md](handbook.md); the *deployments and namespace map*
are [platforms.md](platforms.md); *who a user is* — [identity.md](identity.md);
This document carries no rationale and no
chronology — if a sentence would start with "because," it belongs elsewhere and
appears here as a link. It changes only when a contract changes, in the same
commit as the change.

## The three layers

**Saranos** is the operating system, **IPNX** the kernel and userspace,
**emca** the windowing and UI system. What each name covers, why Saranos needs
a name of its own, and where the boundaries fall is [saranos.md](saranos.md).
This document is the contract map for the technical system underneath.

## The system, in one paragraph

The IPNX kernel — an original implementation of Plan 9's architecture, sharing
none of its code — runs as an ordinary userspace process on each platform.
Processes are WebAssembly instances; `exec` is instantiation. Every process has
its own namespace — a mount table mapping paths to file servers — and every
non-process syscall resolves through it. 9P is the only IPC: in-process devices
present the file interface as function calls, and exactly one driver marshals
wire 9P at mount boundaries. Userspaces are libc dialects over the one kernel:
Plan 9's own (`lib9`), the V10 exhibit (`libv10`), a WASI shim, and a measured
modern personality to come. (The full statement and its derivation:
[design.md](archive/design-log-claude-written.md).)

## The component map

```
kernel/            the kernel core (Rust): pure state machine — syscalls in,
                   effects out — with its own single-threaded async executor
hosts/macos/       embeds the core over wasmtime/Cranelift; threads as processes
hosts/{oci,ipados,browser}/   the same contract, per implementation.md milestones
userspace/         the userspace (graduated at M0): libcs, vendored sources
                   (verbatim), commands, citizens, the rootfs seed, mk.sh,
                   VERSIONS (the measured toolchain, drift-warned)
```

The kernel core and the JS reference implement the same kernel; a host
implements the host contract below; every userspace binary implements the guest
ABI. The conformance suite binds all three.

## The kernel's shape

- **A process** is: a pid, a parent, a namespace, an fd table, a wait queue, a
  note queue.
- **`Chan`** is the object everything acts on. A walk produces one, an fd holds
  one, a mount point is one, and every device operation takes one. Plan 9's
  `struct Chan` (`portdat.h`), minus what a kernel without hardware has no use
  for.
- **The namespace** is a per-process table of mount points, and **a mount point
  is a FILE, not a path**: Plan 9 keys it by the identity of the channel
  mounted upon — `findmount(Chan**, Mhead**, int type, int dev, Qid qid)`,
  `chan.c:855` — and a walk checks for a mount at **every component**. A bind
  is therefore visible through every path that reaches the file. An entry is a
  **union list** (`MREPL`/`MBEFORE`/`MAFTER`); walks try elements in order,
  creates land in the element carrying `MCREATE`, and a union with none refuses
  creates. Flagless `rfork` **shares** the namespace; `RFNAMEG` copies;
  `RFCNAMEG` starts it empty — the same three-way rule the fd table and the
  environment follow.
- **The Dev table** is `struct Dev` (`portdat.h:241`): a letter, a name and
  seventeen function pointers, **and no state**. That is why it can be reached
  from anywhere: a device's own state lives in its file's globals, outside the
  table — `static Srv *srv` (`devsrv.c:21`), `pipealloc` (`devpipe.c:22`),
  `struct Mntalloc` (`devmnt.c:48`). Omitted from the seventeen are `reset`,
  `init`, `shutdown` and `power` (hardware lifecycle) and `bread`/`bwrite`
  (the block fast path, which takes a `Block` this kernel has none of).
- **A name beginning `#` carries an ATTACH SPEC, and it is the device's, not
  a name to walk.** `namec` takes everything up to the first `/` together
  (`chan.c:1348`, *"while(*name != '\\0' && (*name != '/' || n < 2))"* — the
  `n < 2` is `#/`, whose letter IS a slash) and hands what follows the letter
  to `attach`. So `#ec/cputype` is the env device, the spec `c`, and the name
  `cputype`. A device that does not understand a spec must say so:
  `envattach` errors `Ebadarg` (`devenv.c:73`).
- **`eve` is kernel-wide, mutable, and empty at boot.** `char *eve`
  (`auth.c:10`) is a global every device reads — `devdir` makes it every
  file's group (`dev.c:106`), `iseve()` compares it, `hostownerwrite`
  renames it for all of them (`auth.c:135`) — and `userinit` leaves it the
  empty string (`pc/main.c:285`), with the first process's user a copy.
  **It is `boot` that names the host owner**, by writing `#c/hostowner` with
  `$user` or `"glenda"` (`bootauth.c:56`). A Rust kernel cannot have an
  ambient mutable global, so the device table hands the one cell to each
  device as it joins.
- **A walk carries the path beside the channel**, not on it (`chan.c`,
  `walk`): `path = c->path` before the loop, `addelem` per name, `c->path =
  path` at the end — and `domount` does not touch the text, only the mount
  point it records. So a name keeps the name it was walked by. `namec` puts
  the saved path back after its own `domount` for `Aaccess`/`Aremove`/
  `Aopen` (`:1480`); `Abind` does not, and says why.
- **A mount point keeps the flag WORD and the spec.** `struct Mount` is
  `Chan *to; int mflag; char *spec` (`portdat.h:295`) and `Mhead` keeps
  `Chan *from` (`:307`), because `#p/<n>/ns` prints all four back as the
  lines that rebuild the namespace. `int2flag` composes `-a`, `-bc`, `-aC`
  and gives `MREPL` the empty string.
- **`Chan.aux` is the device's own word on a channel** (`portdat.h`), copied
  by `devclone` (`dev.c`) and meaningless to everything else. Plan 9 holds a
  `void*`; a device here keeps its state in its own struct, so what the
  channel carries is only *which* of that state it means. `#e` is why it
  exists: `envattach` puts `&confegrp` there for the spec `c` and nil
  otherwise (`devenv.c:77`), and `envgrp`/`envwriteable` read it back
  (`:371`, `:379`).
- **A walk produces the new CHANNEL**, as `devwalk` fills in `nc`
  (`dev.c:169`). Not a qid: a channel through `#M` carries the FID the server
  knows it by, and a walk is what mints one.
- **A union is a LIST, and every part of it is Plan 9's**: `cmount` puts the
  directory itself in first when a union is made on it (`chan.c:707`), and
  copies a union when one is bound onto a directory (`:719`); a walk tries
  each element in turn when the first has no such name (`:1027`); a read of
  such a directory reads every element (`unionread`, `sysfile.c:323`); and
  `Amount` and `Atodir` do NOT step onto a mount, so a second bind attaches
  to the original directory and `cd` is left before the mount point
  (`:1532`, `:1522`).
- **`cclone` is how a channel becomes the caller's own** — a walk of NO names
  (`chan.c:837`). `namec` takes one before it opens, removes or creates, and
  the reason is in Plan 9's own comment: *"We need our own copy of the Chan
  because we're about to send a create, which will move it."* A channel taken
  out of a mount table is shared by everything that resolves through it.

**THE DEVICE LETTERS ARE PLAN 9'S, AND THERE IS NO EXCEPTION.** A device exists
here only if Plan 9 has one, means the same by it, and spells it with the same
letter. Ten:

  | dev | why the kernel has it |
  |---|---|
  | `/` | **devroot** — a namespace starts somewhere. `rootdir[]`'s two entries plus the ten empty directories `rootreset` adds for a first process to bind over; writes refused, as `rootwrite` refuses them |
  | `\|` | **devpipe** — two processes talk when neither serves the other. `pipe(2)` IS an attach of this device, not a second mechanism |
  | `s` | **devsrv** — a posted channel kept alive by name, so a process that did not inherit it can find a server |
  | `M` | **devmnt** — the mount driver, and the only place wire 9P is marshalled. Not attachable by name: `mntattach` takes an internal struct, so it is reached only through `mount()` |
  | `p` | **devproc** — processes as files. The kernel holds that state, so the kernel serves it |
  | `d` | **devdup** — a process's own descriptors as files |
  | `e` | **devenv** — the environment group, which `rfork`'s `ENVG` and `CENVG` exist to share, copy or clear. **Two groups, and the attach spec picks one**: no spec is the calling process's own, `c` is `confegrp`, *"the global environment group containing the kernel configuration"* (`devenv.c:16`) — so `#ec` — and any other spec is `Ebadarg` (`:73`). Only eve writes the configuration (`envwriteable`, `:377`) |
  | `c` | **devcons** — all 23 of `consdir[]`. The console's line discipline is here because `port/devcons.c` keeps it there; the machine supplies only `screenputs` and the keyboard's characters. The rest is the kernel's own state as files |
  | `¤` | **devcap** — the only way a process becomes another user: eve mints a capability, a process spends it once |
  | `9` | **devvirtio9p** — a CHANNEL to a 9P server the machine provides, and nothing else. `pc/devvirtio9p.c:1227`; its own comment is this system's situation, *"mount a host directory … with no network in the path"*. A 9legacy device, absent from `plan9-stock` |

**What is NOT here, and why it is not an omission.** `#i` draw and `#m` mouse
are hardware this machine has none of, and emca is userspace entirely. Any
letter Plan 9 lacks — a fetcher, a versioning layer, host files — is not a
device at all; it is a file server, which is what Plan 9 would have made it.

> **Every deviation from this needs Christine's approval, and the default
> answer is no.** A structure can be Plan 9's in vocabulary and something else
> in mechanism, so the test is a counterpart in `plan9/` at file and line — not
> whether the deviation can be argued for.

**Two configurations, with different lifetimes.** `$CONF` is the kernel
configuration file — the `dev` list `mkdevc` turns into `devtab[]`; its bytes
are embedded in the kernel image (`port/portmkfile:53`) and read at
`/dev/config`, and its path is `conffile`, the second half of `$terminal`
(`pc/main.c:250`). **`plan9.ini` is the other one**: what the bootloader read
before the kernel existed, exported to `#ec` entire and to `#e` for the names
not beginning `*` (`pc/main.c:257`). Here the first is `LETTERS` in
`hosts/ipnx/src/main.rs`; **the second is empty**, because plan9.ini holds the
stored answers to what `boot` would otherwise ask (`boot.c:354`, *"create
default reply"*) and this `boot` asks nothing — one method, no prompt. See
`docs/proposals.md`.

## Contract: the guest ABI

A Plan 9-dialect binary is a wasm32 module that:

- **imports** `env.memory` (the host supplies linear memory) and the kernel
  interface: `env.sys` (the trap gate), `env.forka` (bare fork),
  `env.setj/longj/sjbuf` (setjmp over asyncify), `env.tsave/tjump/tdrop`
  (libthread contexts), and `guard.rfork` (the lazy-fork guard);
- **exports** `_start`, called once on the process's own execution context;
- issues syscalls as **Plan 9's trap numbers and no others**, but for the four
  the substrate forces — `ARGS` 200, `NOTEGET` 202, `AREAD` 210, `IOWAIT` 211
  (the derived list is [syscalls.md](syscalls.md)); `read`/`write` are
  `pread`/`pwrite` at
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
root, `rename` is copy-plus-remove (there is no link), and the personality is
`wasi:cli/command` and nothing more.

**A personality is a libc dialect over this one ABI, and there are two kinds.**
A *native* personality (lib9, libv10, the measured `libunix`) presents IPNX's
own chosen surface. A *port* personality supplies a foreign environment —
`libc.a`, its headers, its runtime — so that **unmodified** third-party source
compiles against it (a GNU/glibc personality, a musl personality, a BSD
personality). The port personality is where the adaptation lives: the source
is never patched to match IPNX; the environment is built to match the source.
The same construction dissolves the package layer's two classic problems:
versions coexist under `/pkg/<name>/<version>` — which installing
**binds**, never copies (P7's proposals, 2026-09-24) — and namespaces
choose, so there is no dependency solver at the OS layer; and a conflict — a name about
to be bound over DIFFERENT bytes — is checkable at install, and pkg refuses
it (identical bytes are idempotent; deliberate shadowing remains expressible
through union order). Per-process installs follow: an `rfork n` child that
pkg-installs has its own package set — coexisting development environments
are processes, with no activation machinery.
Both kinds are ordinary userspace over the unchanged kernel — the design's
porting inversion ([design.md](archive/design-log-claude-written.md), 2026-08-29). The demo's in-tab `cc`
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

## Contract: the embedding

> **UNAPPROVED — this boundary is a deviation from Plan 9 and needs Christine's
> decision.** Plan 9's kernel has no host: it drives hardware. Everything in
> this section exists because this kernel does not, so none of it can be
> settled by argument here. What the section may state is what is FORCED and
> what is merely convenient, so the decision has something to act on.

**Forced.** A process is a WebAssembly instance, and `exec` is instantiation —
so something must hold an engine, and the kernel cannot. That is the whole of
what is unarguable.

**Not forced, and not decided.** How the kernel reaches that engine; whether a
fork's resumable state crosses the kernel and in what form; how bytes move
between a process and a server the embedding holds. Earlier drafts of this
section specified all three — an effect list, an opaque continuation, console
wiring for `#c`, a clock stamped in before every entry. All of it was invented
here and has been removed from the code.

**What is settled, because it follows from rules that are:** the embedding owns
no device. A console, a clock, storage and randomness are **file servers**,
reached by processes over 9P, because 9P is the only IPC and the kernel holds
no driver.

## Contract: the wire

- **9P2000, the only version — and there are no extensions.** The `Tlink` 128
  / `Tsymlink` 130 / `Treadlink` 132 messages were minted here and **removed
  again** (P1 step 3, 2026-09-04): Plan 9 has no link operation at any layer,
  so there was nothing for them to carry. A message type above 127 is now
  unused, as it is upstream.
- **Identity crosses at attach**: every wire mount carries the mounting
  process's `uname` in `Tattach`; the server applies its own policy to that
  name ([identity.md](identity.md) — "the namespace unions services; it cannot union
  their trust").

## Contract: the conformance suite

- **It measures one thing: have we reached functional equivalence with the
  demo?** It is a checklist of what the system can DO, and it starts almost
  entirely unreached. That is its purpose — it is a distance, and it shrinks as
  phases land.
- **Equivalence is in features, not mechanism and not presentation.** The
  surface will look very different, so no check may be wired to tabs, panes,
  placement or chrome. The rebuild is free to reach any line by another route.
- **It is not there to lock the design.** A test asserting "the call list is a
  subset" freezes a decision rather than measuring a system; guards of that
  kind are unit tests of the code they guard.
- **Moving a line to reached is a claim that a person can do that thing** — not
  that code exists, not that a unit test passes. The harness fails if a line is
  marked reached with no check behind it.

## Contract: what is trusted

- **The engine's sandbox is the isolation primitive.** A guest reaches
  exactly its imports and its linear memory; everything else it can touch is
  what its namespace resolves. There is no second barrier: on hosted rungs
  the host OS process stands behind the engine, on the microVM rung nothing
  does — a runtime escape there is a whole-system escape, recorded plainly
  ([design.md](archive/design-log-claude-written.md), OCI decision).
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
9P or a host device; [design.md](archive/design-log-claude-written.md), storage invariant).
