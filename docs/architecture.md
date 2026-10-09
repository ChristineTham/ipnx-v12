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
wire 9P at mount boundaries. The userspace is Plan 9's own, vendored whole —
its libraries, APE among them — and a personality is userspace too. WASI
binaries are supported natively: a WASI engine runs them, not a personality.

## The component map

```
kernel/            the kernel core (Rust, no dependencies): Plan 9's kernel,
                   a subset — processes, their tables, the namespace, the
                   devices below — over a machine each host supplies
hosts/ipnx/        a terminal: the machine is wasmtime, each process a fiber;
                   and the boot and the root's 9P server, which hosts share
hosts/web/         a browser: the core compiled to wasm32 in a worker, each
                   process a worker of its own, a call a mailbox in shared
                   memory; and emca, the window manager, in the page
userspace/         the userspace (graduated at M0): libcs, vendored sources
                   (verbatim), commands, citizens, the rootfs seed, mk.sh,
                   VERSIONS (the measured toolchain, drift-warned)
```

The kernel core implements the kernel; a host implements the host contract
below; every userspace binary implements the userspace ABI. The conformance
suite binds all three.

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
  environment follow. It is kept as Plan 9 keeps it: `MNTHASH` chains of
  mount heads (`portdat.h:474`, `:490`), each a reference, and every channel
  in it — the one mounted upon, each one mounted — a reference too, which a
  copy shares (`pgrpcpy`, `pgrp.c:128`) and the last close lets go.
  `cmount` refuses an inconsistent mount, `Emount` (`chan.c:654`, `:662`,
  `:686`); `cunmount` answers `Eunmount` or `Eunion` (`:792`, `:812`); a
  mount point is neither removed nor renamed, `Eismtpt` (`sysfile.c:1151`,
  `:1181`). The kernel processes share one namespace group, `kpgrp`
  (`proc.c:1469`).
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
  **The host names the host owner** before the first program runs, as Plan
  9's `boot` does — writing `#c/hostowner` with `$user` or `"glenda"`
  (`bootauth.c:56`). A Rust kernel cannot have an
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
  directory itself in first when a union is made on it (`chan.c:708`), and
  copies a union when one is bound onto a directory (`:725`); a walk tries
  each element in turn when the first has no such name (`:1027`); a read of
  such a directory reads every element (`unionread`, `sysfile.c:323`) — of
  the list as it stands, because the open channel holds the mount HEAD
  (`Chan.umh`), not a copy of it; and
  `Amount` and `Atodir` do NOT step onto a mount, so a second bind attaches
  to the original directory and `cd` is left before the mount point
  (`:1532`, `:1522`).
- **`cclone` is how a channel becomes the caller's own** — a walk of NO names
  (`chan.c:837`). `namec` takes one before it opens, removes or creates, and
  the reason is in Plan 9's own comment: *"We need our own copy of the Chan
  because we're about to send a create, which will move it."* A channel taken
  out of a mount table is shared by everything that resolves through it.
- **An open answers a reference, and a close lets one go.** `devtab[]`'s
  `open` returns a `Chan*`, and two devices return one that already exists,
  with one more reference: `dupopen` the descriptor's (`devdup.c:86`) and
  `srvopen` the posted one (`devsrv.c:135`). So the device table's open
  answers `Rc<RefCell<Chan>>`, a descriptor holds one, and `cclose` is its
  count: the device's close runs at the last reference (`chan.c:490`). A
  descriptor, `/fd/3` opened from it, and a name it was posted under are one
  channel, offset and all — and the offset is shared as Plan 9 shares it: a
  write takes its range before the device writes (`sysfile.c:744`), a read
  adds what it read when it ends (`:683`).
- **A last close may wait, and the call it is made in waits with it** —
  the mount driver for `Rclunk`, as `cclose` waits in `mntclunk`. A call
  that leaves the processor keeps the rest of itself, and where a close is
  made the closes not yet made are part of that rest. `pexit` closes before
  the parent is told (`proc.c:1160`, `:1219`): the descriptors, then `dot`,
  then the namespace (`:1166`, `:1168`), each at its last reference, and it
  leaves them nil — so `#p/<n>/ns` of a process that has exited is
  `Eprocdied` (`devproc.c:958`). `rfork` without `RFPROC` closes the tables
  it replaces (`sysproc.c:61`, `:70`), and `chdir` the `dot` it leaves.
  **A last close is made only where what is left of it can be kept**: one
  reached inside another close, or in a call that may run again from the
  top, is handed to the closing loop it is inside, or to the call's end,
  and made there next — once.
- **A mount's wire is held while the server holds a fid for any channel
  through it**, and let go with the last (`chanfree`'s *"cclose(c->mchan)"*,
  `chan.c:475`); its session ends when the wire's own last reference goes
  (*"muxclose(c->mux)"*, `:471`).
- **What a walk makes, it closes.** Each step through a mount is a fid on
  the server — up to `MAXWELEM` names to a `Twalk` (`chan.c:1006`) — and a
  walk closes the channel it steps from and the one it holds when it fails
  (`chan.c:1109`); `namec` answers, with its channel, whether the caller
  owns it — and so must close it — or it is the process's `dot` or `slash`,
  or a mount's. From the walk on, `namec`'s errors name the name as far as
  the element they concern (`chan.c:1406`).
- **An RPC a note interrupts is flushed**, and waits for the flush's
  answer (`mountio`, `devmnt.c:782`).

**THE DEVICE LETTERS ARE PLAN 9'S, AND THERE IS NO EXCEPTION.** A device exists
here only if Plan 9 has one, means the same by it, and spells it with the same
letter. Eleven:

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
  | `9` | **devvirtio9p** — a CHANNEL to each 9P server the machine provides, and nothing else: `#9/0` the root's store, and `#9/1` a window manager where the host serves one (`hosts/web`'s emca). `pc/devvirtio9p.c:1227`; its own comment is this system's situation, *"mount a host directory … with no network in the path"*. A 9legacy device, absent from `plan9-stock`. **A server may answer later**, as the virtqueue's does: the machine's `Nineserver` takes a request (`submit`, `devvirtio9p.c:597`) and gives what it has answered when asked (`vqharvest`, `:470`), which the kernel does at the clock (`v9interrupt`, `:508`) |

**What is NOT here, and why it is not an omission.** `#i` draw, `#m` mouse
and `#t` uart are hardware this machine has none of, and none is emulated
(*"This is WASM we have no hardware we do not want to emulate hardware"*,
Christine, 2026-10-08); and emca is no device — it is the host's window
manager, serving its files over `#9` as the root's store is served, with a
half in userspace (`/bin/emca`). Any
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
`hosts/ipnx/src/main.rs`; **the second is `plan9ini`** in
`hosts/ipnx/src/lib.rs` — `user=`, and `init=` when there is a command —
which the host reads as Plan 9's boot reads `plan9.ini`. There is one root,
so nothing else to answer.

## Contract: the userspace ABI

A Plan 9-dialect binary is a wasm32 module that (as built by
`userspace/mk.sh`; the stubs are `userspace/sys/src/libc/wasm/sys.c`):

- **imports** from `sys` the calls of Plan 9's `libc/9syscall`, one import
  per call, plus `setjmp` and `longjmp` (`libc/wasm/setjmp.c`) — and
  nothing else;
- **imports** its memory — shared, maximum 4 GiB, the stack first at
  `[0, 16M)`, Plan 9's `USTKSIZE` (`pc/mem.h:51`) — which the machine makes
  for each new image, putting the `Tos` at the top of the stack and the
  argument block under it, as `sysexec` does (`sysproc.c:450`); the heap
  begins at `end`, which wasm-ld calls `__heap_base`;
- **exports** `memory`, `__stack_pointer`, the function table (linked
  from index 4128, where the 386's text starts, `8l/obj.c:183`, so no
  function pointer is a small number — RESEARCH §16.19),
  `_start(argc, argv, tos)` (`libc/wasm/main9.c`), `__notestart`
  (where a note handler is entered), `__asyncbuf`/`__asyncbufsize`, and
  asyncify's `asyncify_*` functions;
- **is asyncified** (`wasm-opt --asyncify`, instrumenting only paths to
  `sys.setjmp`, `sys.longjmp` and `sys.rfork`), because the machine owns
  the stack (RESEARCH §16.12). Errors are `errstr` strings, never numbers.

The machine's obligations, which are Plan 9's semantics:

- **`rfork(RFPROC)` returns twice**: the stack unwinds, the child is a new
  instance with a copy of all of memory and the stack wound back in it,
  where `rfork` answers 0; the parent's is wound back and answers the pid.
  With `RFMEM` the child's instance imports the parent's memory, and its
  stack region is its own: a copy the machine puts in place whenever it
  runs (`segment.c:175`).
- **`setjmp(j)`** keeps the stack under j's address and writes SP and a pc
  of 0 into j (`JMPBUFSP`, `JMPBUFPC`, 386's); **`longjmp(j, v)`** winds
  back the kept stack with setjmp answering v — or, if j holds a pc, calls
  that function on j's SP with SP as its argument (libthread's new thread,
  `libthread/wasm.c`).
- **The `Tos`** (`sys/include/tos.h`) is at the top of the stack, the stack
  below it, its `pid` written for each process, as `kexit` writes it.

**A WASI binary runs natively** — a module importing
`wasi_snapshot_preview1`, run by an existing WASI engine and understanding
none of IPNX's conventions (Christine, 2026-10-08: *"as WASI native as
possible"*; *"WASI is not a personality, we support WASI binaries
natively"*). Which files it sees is proposed, not decided
([proposals.md](proposals.md), *Go and Python*), and nothing runs one yet.

**A personality is userspace** (Christine: *"even the Unix v10 personality
should be userspace"*) — a library over this ABI, and nothing in the kernel.
Plan 9's own APE is one, vendored with the rest of the userland. WASI is not
one.

**Installing a package binds it** ([packages.md](packages.md)). Versions
coexist under `/pkg/<name>/<version>` and namespaces choose, so there is no
dependency solver at the OS layer; a name about to be bound over different
bytes is refused at install (identical bytes are idempotent, and deliberate
shadowing remains expressible through union order); and an `rfork n` child
that installs a package has its own package set.

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

**Settled by Christine, 2026-10-08 — the host does `boot`'s part** (*"The
host does it"*). Before the first program runs the host makes, as pid 1,
the calls Plan 9's `boot` makes (`boot/boot.c:284`–`:296`): it names the host
owner (`bootauth.c:56`), opens `#9/0`, negotiates the version and posts the
channel as **`#s/root`** — Plan 9's `#s/boot`, renamed — then binds `/` onto
itself, mounts the server at `/root` and binds `/root` after `/`, and runs
`init` from `$init` or `/$cputype/init -t`. There is no `/boot/boot` and no
`#/boot`: `boot` is the Unix bootloader's name (2026-09-04).

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

- **The host is part of the system, not something to be kept out.**
  *"We are in symbiosis with the host. Our wasm binaries can access host
  resources and invoke host binaries"* (Christine, 2026-10-08;
  [saranos.md](saranos.md), *The host's resources*). The engine runs each
  program in its linear memory, reaching out only through its imports —
  that is how wasm runs code, not a wall between the system and its host.
- **A program's authority is its namespace** — what it resolves, the
  host's own files among them where the host serves them — and the host
  commands it starts, which run as the host's user (decided, not built).
- **Trusted**: the kernel (both implementations), the host, the build
  toolchain's output.
- **Untrusted by construction**: every wire-mounted server (the client
  applies the protocol and its own policy — "the namespace unions services;
  it cannot union their trust", [identity.md](identity.md)), and every wire
  client (per-attach identity; the server decides what the name may do).
- **A demo visitor** runs the system in their own browser only; the hosting
  is static files; nothing a visitor does reaches another visitor or a
  server.
- The future credential mechanism (M8) uses boring, reviewed cryptographic
  primitives only — cleverness is out of contract.

## Deliberately not architecture

Per-host internals (how a shim spawns threads or paints), window *policy*
(placement, menus — the contract is the files a window serves), and on-disk formats (**the system
never learns one** — durability is always another filesystem reached through
9P or a host device; [design.md](archive/design-log-claude-written.md), storage invariant).
