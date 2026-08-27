# ipnx-v12 — the design

*Scope framing 2026-08-26; re-founded 2026-08-27 (decision log). The question this
document exists to answer: **what is Unix if you write it afresh today, keep Plan 9's
answers, and put back the compatibility Plan 9 threw away?***

**The statement, once.** A **modified Plan 9 kernel hosted as an ordinary userspace
process** — browser, macOS, iPadOS, OCI, eventually hypervisor-direct; **9P as the only
IPC**; **per-process namespaces**; **everything exposed as a file**; **WebAssembly as
the executable format**; and **personalities as libc dialects** above the one kernel —
Plan 9's userland entire by the curation principle, a WASI second ABI, and a **modern
Unix personality** derived by measurement against git, CPython and Go. The V10 exhibit
stays as heritage; its completeness is not a goal.

No VAX. No disk image. No emulator. No POSIX, no systemd, no sediment.

## Why the kernel is Plan 9's and not V10's

The parent project's V10 kernel is 61,072 lines of C and 3,141 of VAX assembly. Measured
against this target:

| Subsystem | Lines | Fate |
|---|---|---|
| `io/` — 60 driver files | 27,035 | **Gone.** No hardware to drive |
| `vm/` — demand paging | 5,882 | **Gone.** The engine is the MMU |
| `md/` — per-machine | 8,116 | **Gone.** There is no machine |
| `ml/` — `swtch.s`, `trap.s`, `setjmp.s`, `copy.s` | 3,141 asm | **Gone.** No registers to save |
| `fs/` + the mount layer | 4,538 | Replaced by 9P |
| `os/` process semantics | ~3,300 | The only part with anything to say |

Three thousand surviving lines is not a port, it is a rewrite. And the kernel worth writing
already exists.

**The decisive asymmetry is which way the surgery runs.** Plan 9 has per-process
namespaces, 9P and `rfork` *by design*. V10 has a mount table that lives on the inode —
`ip->i_mpoint`, `mip->i_mroot`, pointers between two objects in a **global** inode table,
so every process necessarily sees every mount. Making that per-process means lifting the
relationship out of `struct inode` and rewriting `nami()`.

So:

- **Adding V10 semantics to Plan 9 is addition.** `setuid`, hard links, `umask` are things
  Plan 9 chose not to have. Nothing is displaced.
- **Adding Plan 9 semantics to V10 is eviction.** The mount table, `errno`, signals and
  `ioctl` all have incumbents that must be torn out while the system keeps running.

And only one direction has a precedent: **APE proves POSIX-on-Plan 9 works, built by the
people who designed both systems.** Nobody has ever run Plan 9 binaries on a Unix kernel,
and doing so would need a second `a.out` format *and* a second ABI on the same trap —
V10's stubs are literally `.set open,5` / `chmk $open`, and Plan 9's `OPEN` is 14, `CLOSE`
is 4, `DUP` is 5. The numbers collide on the same instruction.

## The V10 personality, mapped

Plan 9's `/sys/src/libc/9syscall/sys.h` names 52 slots holding **40 live calls** (eleven
superseded `_` variants and one reserved slot). V10's `os/sysent.c` fills 69 of 128 slots
with **68 distinct routines**. The mapping, and the full call-by-call derivation is
[docs/syscalls.md](syscalls.md):

| Class | Count | Detail |
|---|---|---|
| **A — direct** | **28** | `exit`→`exits`, `fork`→`rfork`, `creat`→`create`, `unlink`→`remove`, `exece`→`exec`, `wait`→`await`, `fmount`→`mount`, `dirread`→`read` on a directory, `sbreak`→`brk_` |
| **B — library only** | **12** | `chmod` `fchmod` `chown` `fchown` `utime` all collapse into **`wstat`/`fwstat`**; `chroot`→`rfork(RFCNAMEG)`+`bind`; `setpgrp`→`rfork(RFNOTEG)`; `getlogname`→`/dev/user`; `saccess`→ try the open; `nice`/`times`→`/proc/n/ctl`, `/proc/n/status` |
| **C — genuine work** | **17** | `link` · `symlink`/`readlink`/`lstat` · the seven uid/gid calls · `umask` · `mknod` · `ioctl` · `select` · `kill`/`ssig` vs notes |
| **D — machine management, drop** | **11** | `stime` `sysacct` `biasclock` `syslock` `sysboot` `profil` `vadvise` `vlimit` `vswapon` `vtimes` `nap` |

**40 of 68 — 59% — are free or library-only.** The counts are of routines rather than of
rows — `lseek`/`seek` and `gtime`/`ftime` are two `sysent.c` entries each with one Plan 9
answer — and the full census with its derivation is RESEARCH.md §3.

Class B is where Plan 9 is smaller *and better*, and it should be preserved as such:
`chmod`, `fchmod`, `chown`, `fchown` and `utime` are five V10 calls that each write one
field of a file's metadata, and Plan 9 replaced the family with `wstat`, which writes a
`Dir` (V10 has no `rename` syscall — renaming is `link`+`unlink` in userland). Restoring
them as five **syscalls** would un-elegant the kernel. Restoring them as **libc functions**
over `wstat` costs about forty lines and loses nothing.

Of the 17 in class C, most are cases Plan 9 was right about and where V10 programs should be
recompiled rather than accommodated — `ioctl` becomes `ctl` files, `mknod` becomes file
servers, `chroot` becomes `bind`, `select` becomes processes. **The genuine kernel additions
are the uid model and hard links**, and the uid model is the one APE called impossible:

> setting the userid, groupid, effective userid and effective groupid do not do anything
> useful. The concept is impossible to simulate in Plan 9.

**That is the item to design first, because it decides whether the answer is yes.**

### Why this is not APE

APE targets POSIX.1-1990, and every limitation it confesses is a POSIX feature Plan 9
refused:

> the functions dealing with stacking signals, `sigpending`, `sigprocmask` and
> `sigsuspend`, do not work
> · `O_NOCTTY` option has no effect. The concept of a controlling tty is foreign to Plan 9.
> · `setsid` forks the name space and note group, which is only approximately the right
> behavior
> · Advisory locking via `fcntl` is not implemented

**V10 has none of those.** No `sigaction`, no `sigprocmask`, no sessions, no `fcntl`
locking, no job control, no sockets, no `mmap`, no threads. The V10 personality is the
subset of APE that would have existed if its authors had targeted their own previous system
instead of a standards committee's — and APE's source, MIT, is the starting point to cut
down rather than a thing to write fresh.

## Hosting: the kernel as a userspace process

The architecture has been built three times and the differences are instructive.

| | Kernel | Guest execution |
|---|---|---|
| **plan9port** | none — a library port | native host processes |
| **9vx** | Plan 9's, as a user program | **vx32**, a user-level x86 sandbox |
| **Inferno `emu`** | Inferno's, hosted | **Dis** bytecode |

9vx is the closest technical relative: it "runs as an ordinary user program, but behaves
like a separate VM running Plan 9", treating vx32 as "an architecture with a
software-managed TLB", unmapping pages on context switch and remapping on demand, and
preempting by asking the host for `SIGALRM` at intervals. Everything in that sentence
transfers except the sandbox, which is x86 and therefore dead.

`emu` is the closest architectural relative: "The Inferno kernel can run both native and
'hosted' on a range of platforms and which presents the same interface to programs in both
cases." **This project is `emu` with wasm in place of Dis.**

plan9port is the cautionary one: without a kernel there are no namespaces, and without
namespaces `devdraw` had to abandon the file interface for graphics (see §The GUI).

### The constraint that forces wasm

**iOS apps cannot spawn child processes** — `fork()` and `posix_spawn()` are prohibited in
the sandbox — and cannot make writable-executable pages, so there is no JIT. The jail
therefore cannot be host processes and cannot be native ARM64 code.

That leaves an in-process, sandboxed, interpretable substrate. 9vx's answer is dead; emu's
answer is a VM. **Wasm is the answer that has an ecosystem.**

### Building it

The kernel is built with **clang, from Xcode** — and Harvey OS demonstrates this exact
thing: Plan 9 working with gcc and clang, ELF64, standard ABIs, and **APEX** (APE
reimplemented over musl). "You can compile in Linux (or Mac, or BSD) and run into Harvey."

But kencc is not ANSI and the *userspace* is written in it. Three divergences matter:

- **`extern register`** — "will dedicate a register to a variable on a global basis…
  must be identically declared in all modules and libraries". Used for the current-process
  pointer. clang has no equivalent.
- **Anonymous struct/union members** — "the members of the internal structure or union are
  addressable without prefix in the outer structure" — "the most important and most heavily
  used of the extensions".
- **The preprocessor "does not support `#if`"**, only `#ifdef`.

Harvey's answer was to port the source; `goken9cc`'s is to keep kencc and add targets — it
carries eight architectures on single letters including an experimental `e` for wasm
(`ea`/`ec`/`el`), though its README states that back end was AI-written and unfinished, so
it is evidence the retarget is tractable, not a component to depend on. **This is a decision
to take deliberately, not to discover.**

## `exec` is instantiate

Unix's `exec` maps an `a.out` into an address space. This one is `WebAssembly.instantiate`
over the bytes named by a path in the caller's namespace. Two consequences:

- **The engine compiles on every process start.** There is no "compile to native" step a
  program can see.
- **A freshly produced `.wasm` is indistinguishable from a shipped one at `exec` time**, so
  the toolchain question is about convenience, never about whether the system can run what
  it just built.

| | Browser | Native (iPadOS / macOS) |
|---|---|---|
| Engine | the host's, **JIT** — WKWebView's JavaScriptCore has JIT because it is out-of-process | an interpreter you own; **no JIT** |
| Speed | near-native | wasm3's own docs: 4–15× slower than native, ~12.5× on CoreMark |
| Candidate | the host engine | [WasmKit](https://github.com/swiftwasm/WasmKit) — pure Swift, iOS 12+, no Foundation dependency |

## Processes: `rfork`, lazy fork, and what the platform will not give us

A wasm process is five pieces of state and four are copyable from the supervisor:

| State | Where | Copyable? |
|---|---|---|
| Code | `WebAssembly.Module` | **Yes** — structured-cloneable |
| Heap, `.data`, **and the C shadow stack** | `Memory.buffer` | **Yes** — a byte copy |
| Mutable globals, incl. `__stack_pointer` | `WebAssembly.Global` | **Yes**, if exported |
| Table | `WebAssembly.Table` | **Yes** — per slot |
| **Value stack, locals, return-address chain** | the engine's own stack | **No** |

Clang keeps a downward-growing shadow stack in linear memory with `__stack_pointer` as its
ABI stack register (`wasm-ld --stack-first`, `-z stack-size`), so a `memcpy` already carries
everything address-taken. What is stranded is the **continuation**.

**The platform will not fix this.** The stack-switching proposal (WasmFX, stage 2 since
August 2024) names the exclusion in its own explainer:

> some applications such as backtracking, probabilistic programming, and **process
> duplication** exploit multi-shot continuations, none of the critical use cases require
> multi-shot continuations

Process duplication, listed and declined. JSPI suspends one stack; it does not duplicate
one. So there are two mechanisms.

### Lazy fork — and `rfork` already names it

`rfork(RFPROC|RFMEM)` — *"the child and the parent will share data and bss segments"* — is
exactly a fork that does not copy, **declared by the child rather than inferred by the
kernel**:

1. Child calls it → traps to the supervisor.
2. Supervisor suspends the parent, creates a child record sharing the parent's instance,
   returns `0`. The call returns once, into what is now the child.
3. Child runs, mutating shared memory.
4. Child `exec`s → supervisor builds a new instance, returns the child pid into the
   original one, resuming it as the parent.

**No asyncify in this path** — the child never reconstructs a stack, because it *is* the
stack with a different return value. The parent's stack restores exactly for a 64 KB copy
of the bounded shadow-stack region.

The residual hazard is vfork's, and half of it is already gone by construction: vfork was
dangerous because Unix kept fds, signal dispositions and cwd *in the process image*. Here
they are in the kernel and the namespace, so the child's `dup`, `close`, `chdir` and note
work touches nothing the parent can see. What remains is the program's own globals.

### Asyncify — for children that do not exec

Asyncify's saved state is **a data structure in linear memory** — two `i32`s bounding a
stack of spilled frames. That is what defeats the one-shot restriction: *a wasm continuation
cannot be resumed twice, but bytes in an `ArrayBuffer` can be copied as often as you like.*

Cost is whole-module: Emscripten says "something like 50%", and "no worse than double size /
halve speed for most code" — and the analysis that keeps it low **is defeated by indirect
calls**, which a Unix userland is dense with. **So it is a per-binary flag, never
system-wide.**

Measured over V10's `cmd` tree: of 196 `.c` files calling `fork`, **161 also call some
`exec*` and 35 do not**. But that is a per-file proxy and the shell disproves it — `xec.c`
has one fork site with two exits, and `( cmd; cmd )` takes the branch that never execs.
**The fast path is chosen at the call, which is what `rfork` flags are for.**

## 9P is the system interface

Every non-process call is a 9P message to a server named in the caller's namespace.

| Namespace entry | Served by |
|---|---|
| `/` | a storage server |
| `/proc` | a process server |
| `/net` | a network server |
| `/dev/cons`, `/dev/draw`, `/dev/mouse` | the window server |
| `/mnt/*` | anything, including remote |

**Where the bytes live** is a per-platform backing decision behind one interface:

- **Native** — the host filesystem.
- **Browser** — **OPFS** via `@zenfs/dom` (ZenFS is BrowserFS's successor; the `browserfs`
  packages are deprecated and republished under `@zenfs`). Two hard constraints:
  `createSyncAccessHandle()` is **Worker-only**, so the storage server lives in a Worker;
  and iOS Safari evicts aggressively with `persist()` hard to obtain.

### WASI is a shim, not the system interface

`wasi-filesystem`'s own README settles this:

> **"WASI filesystem is not intended to be used as a virtual API for accessing arbitrary
> resources. Unix's 'everything is a file' philosophy is in conflict with the goals of
> supporting modularity and the principle of least authority."**

The founding premise of this project, named as a stated non-goal. And the API bears it out:
eight descriptor types including `block-device` and `character-device`, but **no `chmod`, no
`chown`, no `ioctl`, no `mknod`** — it can recognise a character device and cannot create
one.

Nor do processes arrive later: across the whole WASI proposal list there is **no proposal at
any phase** for processes, spawning, fork, exec, signals, job control, tty or device nodes.
WASI 0.3 (released 2026-06-11) moved `wasi:io` into the Canonical ABI as `async func`,
`stream<T>` and `future<T>` — useful, because the supervisor's blocking calls become an
unresolved `async func` rather than a fake — but it is suspend/resume, not a duplicable
stack.

There is also a tension worth naming rather than discovering: the component model's thesis
is **typed** interfaces (WIT), and this system's is a **uniform untyped** one. They do not
compose. A server exporting a WIT interface forfeits the property that makes 9P worth
having — that any client works with any server, and `cat` works on a network connection.

**WASI's role is `wasi:cli/command`** — argv, environ, exit, stdio — so a ported foreign
program can find its arguments. That is all.

## The GUI: rio-shaped, so `sam` and `acme` work

The requirement is **the interface those programs open**, not rio itself. Window policy —
placement, menus, tiling versus floating — is entirely ours.

From `rio(4)`:

> A mount of `$wsys` causes rio to create a new window; the attach specifier in the mount
> gives the coordinates of the created window.

So the window server is **a 9P server that manufactures a namespace per window**, serving
`cons`, `consctl`, `cursor`, `label`, `mouse`, `screen`, `snarf`, `text`, `wctl`, `wdir`,
`winid`, `window`.

`/dev/draw` is likewise a file protocol. From `draw(3)`: a client opens `/dev/draw/new` and
reads twelve 11-character strings — connection number, image id, channel format, and the
display image's and clipping rectangle's coordinates — then writes single-letter binary
messages to `data`, low-order byte first:

| msg | operation | backend |
|---|---|---|
| `b` | allocate image | texture / offscreen canvas |
| `d` | combine rectangles with alpha mask | blit |
| `L`, `e`, `E` | line, ellipse, arc | geometry, or rasterise in the server |
| `s` / `x` | cached-font text | glyph atlas |
| `y` / `Y` | replace pixels, raw / compressed | texture upload |

### The thing plan9port could not do

plan9port **abandoned the file interface for graphics**: `devdraw` is a separate binary with
X11 and Cocoa (now `CAMetalLayer`) backends that libdraw talks to directly, not over 9P.
The reason is structural — Unix has no per-process namespaces, so no client can have its own
`/dev/draw`.

**This project has them by construction.** So `/dev/draw` can be an actual file, per window,
per namespace, which is what Plan 9 does and what every Plan 9 port has had to give up. It
is the one place this system can be *more* faithful to Plan 9 than the official port, and it
costs nothing extra.

### Per-platform backing

- **Browser** — canvas/WebGL, with **xterm.js** as the `/dev/cons` implementation for
  character windows.
- **macOS / iPadOS** — Metal, and a native terminal view for `/dev/cons`.

Character windows and draw windows are the same kind of object with different files opened
in them, which is what makes a terminal emulator and a bitmap editor peers rather than
special cases.

### What this buys, in order

1. **`sam`** — needs libdraw and libframe and nothing else. plan9port's is MIT. First real
   client.
2. **`acme`** — the real test, because **acme is itself a file server**. It does not merely
   run on the namespace design, it exercises it in both directions. If acme works, the design
   works.

## The toolchain

A wasm shell compiling a wasm program is a binding problem, not a location problem. Three
answers, not exclusive:

1. **Move the toolchain in** — clang and `wasm-ld` compiled to wasm, ~30 MB, as Wasmer ships
   and Wanix does with Go. Costs size; buys no host dependency, which is what makes the
   browser build self-contained.
2. **Move the namespace out** — Plan 9's `cpu(1)`: *"The name space of the terminal side of
   the cpu command is mounted, via exportfs(4), on the CPU side on directory /mnt/term."*
   Native speed; needs 9P running **both directions**; has no browser.
3. **Make the compiler a file server** — `/cc`, on the `/net` pattern. The only one that
   makes compilation a **capability**: a process rforked with `RFCNAMEG` or `RFNOMNT` cannot
   compile, because the name does not resolve. Makes 1 and 2 implementation details.

## Platform asymmetries to design for

| | Browser | Native |
|---|---|---|
| Engine | host, JIT | owned interpreter, no JIT |
| Fork | cannot own the stack → **asyncify** | owns the interpreter → host-side copy possible |
| Storage | OPFS, Worker-only sync handles, evictable | host filesystem |
| Toolchain | answer 1 or 3 | any |

Taking asyncify on both buys uniformity at ~2× on binaries that natively would not need it.
Taking `rfork(RFMEM)` plus a per-binary asyncify flag keeps the cost where it belongs.

## Decisions (2026-08-26; native-host, OCI, storage, toolchain, userland-curation and V10-completeness decisions added 2026-08-27)

- **(2026-08-27) The native host is a Rust kernel core plus per-platform embedding
  shims — after the PoC completes.** The kernel never executes guest code, so the
  core (proc table, namespaces, devices, 9P, the draw engine) compiles once in Rust
  and twice over: native for macOS/iPadOS (guests on wasmtime or wasmi — interpreter
  paths, iOS-legal) and to wasm for the browser (guests stay on the browser's own
  engine). Each platform keeps a thin mach layer — Workers/SharedArrayBuffer glue in
  JS for the browser, threads plus the runtime embedding natively — which is Plan 9's
  own `port/`-vs-`pc/` split with the browser as just another machine. This
  supersedes the WasmKit-in-Swift candidate (kept in the table above as history);
  WasmKit remains an option for the shim's app shell, not the kernel. The 124-test
  suite is guest-side and language-blind: it is the conformance spec any second
  kernel must pass. With the PoC closed (same day), `kernel.mjs` is the reference
  implementation the Rust core is measured against.

- **(2026-08-27) The engine matrix: wasmtime everywhere, mode per shim; WasmKit
  stays superseded.** iOS forbids *two* things, not one: JIT (the
  `dynamic-codesigning` entitlement is Apple-only) **and runtime-loaded AOT** —
  every executable page must come from a signed binary in the bundle, so
  `wasmtime compile` artifacts mapped at runtime are just as illegal as
  Cranelift. Wasmtime's answer is **Pulley**, its portable-bytecode
  interpreter, with `Config::signals_based_traps(false)` so traps are explicit
  rather than guard-page signals: in that mode wasmtime executes no unsigned
  native code at all. The matrix: **macOS — Cranelift JIT** (legal even in the
  Mac App Store under the `allow-jit` entitlement); **Linux/OCI/microVM —
  Cranelift, or AOT `.cwasm` for cold-start**; **iOS/iPadOS — Pulley, signals
  off**. Caveats recorded: `aarch64-apple-ios` is a tier-3 wasmtime target, and
  Pulley is an interpreter — several times slower than Cranelift; fine for rc
  and the editors, *measured* before it is trusted for CPython and Go. The
  engine sits behind one trait in the Rust core, and the like-for-like fallback
  is **wasmi** (which deliberately mirrors the wasmtime API), not WasmKit — the
  Swift candidate would only return if the Rust core itself were abandoned.
  None of the load-bearing machinery cares which engine runs: the fork guard,
  `setjmp/longjmp` and libthread are asyncify — instrumentation inside the
  modules, engine-blind — and fuel/epoch preemption (the OCI entry's need)
  exists in both wasmtime-with-Pulley and wasmi. App Store precedent for the
  interpreter shape is settled practice (iSH ships a usermode x86 Linux
  emulator; UTM SE an interpreter-only VM); wasm binaries shipped in the bundle
  are unambiguous, and user-loaded programs are the standard interpreted-code
  gray zone (guideline 2.5.2 / agreement 3.3.1B) every scriptable app lives in
  — a policy risk, not a technical one.

- **(2026-08-27) iOS local files: always a file server over user-granted
  subtrees — and the browser sandbox is not the barrier it looks like.** iOS
  has no full-disk access for any app, ever: an app sees its own container
  (Documents, surfaceable in the Files app) plus exactly the subtrees the user
  picks through the document picker, persisted as security-scoped bookmarks —
  iCloud Drive, USB drives and file-provider shares included. That consent
  model *is* the ipnx worldview: **a security-scoped bookmark is a capability
  to a subtree, and a granted folder enters the system as a bind** (`bind
  /host/ProjectX /usr/work`) — the platform enforces what the namespace design
  argues for. It composes with the storage invariant: the grant arrives as a
  host-backed file server; ipnx never learns an on-disk format. The three
  deployment forms differ only in plumbing: **plain Safari** — OPFS-only
  private world plus one-shot imports (WebKit never implemented the
  File System Access API), fine for the synthetic rootfs, no real user files;
  **WKWebView shell (the iPad stopgap)** — web content sandboxed as in Safari,
  but the app's native half serves granted files across the bridge
  (`WKURLSchemeHandler`, already needed for COOP/COEP, plus the script-message
  channel), i.e. the native half is literally a file server the kernel mounts —
  the cost is serialization per byte, measured before `git status` meets a big
  repository; **native app (the real milestone)** — the same consent model with
  no web layer. The WKWebView shell also carries a performance irony worth
  keeping: WebKit's content process holds the JIT entitlement, so the
  already-green browser port runs on iPad *with full JIT today* — faster
  execution than the native Pulley build it is a stopgap for.

- **(2026-08-27) OCI is two targets, taken at two different weights.**
  *The scratch container is a stated target of the Rust milestone, for free*: the
  kernel as a static musl binary, PID 1 in a `FROM scratch` image — no distro, no
  userland, a few megabytes. Linux is the thinnest mach layer of any host: `futex`
  is what `Atomics.wait` has been emulating all along, threads are Workers without
  ceremony, and with no JIT ban the guests run on full wasmtime/Cranelift — the
  fastest deployment of the system anywhere. It is also the natural CI machine.
  *The cloud machine — no OS underneath — is a named aspiration, sequenced after
  macOS and iPadOS*: the kernel booting directly on a hypervisor (Firecracker/KVM;
  Kata- or Unikraft-style OCI packaging of the microVM), `pc/` directory number
  four. Its mach layer is the first to add mechanism rather than glue — a timer
  tick driving **epoch/fuel preemption** in the runtime (Workers gave preemption
  free until now), **virtio-9p** serving the root straight into `devmnt`'s native
  tongue, virtio-console for `#c`, and eventually virtio-net under **smoltcp** in a
  `devip`-shaped device, the largest single lift. What it never adds is the part
  that makes bare-metal kernels brutal: wasm's sandbox replaces the MMU (one
  address space, no page tables, no context-switch assembly — recording the risk
  plainly: a runtime escape is then a whole-system escape, with no hardware
  backstop), and 9P replaces the VFS and the driver zoo. The wasm-native OCI shims
  (runwasi, the component model) are watched, not targeted: they cannot yet host a
  kernel that spawns instances with shared-memory mailboxes. The strategic point of
  the cloud machine: a microVM that boots in ~100ms to a per-process-namespace
  world, `exportfs` handing private namespaces between containers over wire 9P —
  namespaces as the cloud primitive the sidecar pattern keeps reinventing badly.

- **(2026-08-27) Storage in containers: the invariant and the design.** First the
  inversion that makes this easy: an OCI image is a union filesystem, a volume is a
  bind mount, and container startup is namespace assembly performed by the runtime —
  the container ecosystem reimplemented Plan 9's namespace operations one layer
  down, so ipnx in a container sits above a machine that already thinks in binds
  and unions. The invariant, held on every rung: **ipnx never learns an on-disk
  format.** Durability is always somebody else's filesystem, reached through 9P or
  a host Dev; virtio-blk plus a filesystem of our own is the door that stays
  closed, and fossil goes unported even in spirit. Per rung:
  *Scratch container* — the image carries the kernel and the rootfs seed; the seed
  becomes the ramfs (the running root is synthesized, never mutated — the container
  ideal, already the PoC's behaviour). Volumes enter through the Linux mach layer
  as a host-fs Dev or a mach-side 9P server, and init's profile binds them into
  place — Plan 9's `/lib/namespace` reborn as the container's mount configuration,
  the YAML of volumeMounts replaced by a namespace script. Recorded honestly:
  Plan 9 unions are not overlayfs — creates land in the MCREATE element but there
  is no copy-up — and the resolution is Unix's own layout discipline, a read-only
  seed (`/bin`, `/lib`, `/rc`) with the writable trees (`/usr`, `/tmp`, `/data`)
  mounted whole from the volume, which is how V10 machines were actually laid out.
  *Cloud machine* — no Linux below, so durability arrives as 9P over a virtio
  transport: virtio-9p under QEMU/Kata, and under Firecracker (which has no 9p
  device) **wire 9P over vsock** — a stream transport plus the mount driver that
  already exists, since `devmnt` mounts any fd speaking 9P. The host agent — a
  hundred lines against the host's real filesystem — owns ext4 or whatever lies
  beneath. Storage therefore adds almost nothing to the cloud machine's mach-layer
  heft; the heft stays in preemption and networking, because 9P absorbs storage.
  *Between containers* — `exportfs` already hands a namespace, private binds
  included, to another process; over a container network it is the same bytes on a
  TCP or vsock stream. Persistent-volume claims, storage sidecars and CSI drivers
  collapse into: one container exports `/data`, another mounts it — per-process
  namespaces as the composition primitive between containers, the CPU-server model
  in OCI clothing, needing nothing beyond a network transport the PoC does not
  already demonstrate.

- **(2026-08-27) The dev toolchain: mk, diff, `/cc` as a capability — and the
  document factory is Plan 9's.** `mk` joins the workbench tier: it is the one
  tool both heritages own — Hume's, born in Research Unix (Ninth Edition) and
  carried into Plan 9 — so building either userspace with mk inside ipnx is
  historically correct twice over (cite mk(1) from the Tenth Edition manual via a
  `../ipnx` measurement when it lands). Its port profile is sam-shaped: libbio, a
  hand-written parser, recipes run through the real rc, out-of-date decisions on
  ramfs mtimes, bare forks so it rides asyncify. `diff` joins beside it — the one
  hole in the daily command set. Compilation stays host-side per the self-hosting
  non-goal, exposed as **`/cc`, a mach-layer file server**: write source, poke
  ctl, read the object or the errors back; a ten-line guest `cc` gives mkfiles
  something to call, and a process without `/cc` in its namespace cannot compile —
  the workflow self-hosts, the compiler does not. acid/db/prof are not ported
  (they read Plan 9 a.out symbols; wasm has none — host tooling stands in, and
  any future in-system debugger is a `/proc` view of guest memory, not an acid
  port); nm/ar/strip/cpp stay llvm, host-side; yacc is optional-later for
  in-system regeneration through `/cc`.
  **The document factory — troff, eqn, tbl, pic, grap, dpost — is taken from
  Plan 9, superseding the earlier V10-side disposition.** The reasoning: Plan 9
  troff is not a rival to Research troff but its own later life — Ossanna to
  Kernighan's device-independent rewrite to Ninth/Tenth Edition to Plan 9, the
  same lineage, rune-aware to match a system that is UTF-8 end to end; V10's is
  the identical machine frozen earlier, pre-Unicode, in K&R. Taking Plan 9's
  moves the document factory OFF the parent project's ANSI-conversion dependency
  entirely (the V10-growth rule itself is unchanged). Staging: troff with the man
  macros and terminal output first — `man(1)` readable in a win — then eqn and
  tbl, pic and grap behind the float door they need, dpost when paper matters;
  font and device tables ship in the rootfs seed.

- **(2026-08-27) The curation principle: the Plan 9 command set is in scope
  entire, because it is the designers' own testimony about Unix.** The earlier
  capped-workbench framing is superseded. Plan 9's `/bin` was assembled by
  Unix's own authors at the one moment compatibility no longer protected the
  accidents — it is what they chose to carry, and this project undoes their
  kernel pivot while preserving their curation. So every Plan 9 utility with
  Unix ancestry replaces the equivalently named Unix tool in `/bin`, including
  the small filters (`join`, `comm`, `look`, `fmt`, `fold`, `freq`, `split`,
  `strings`, `sum`, `cal`, `factor`, `primes`, `units` and kin), `cron` in its
  local-execution slice (the dial-out half waits for `/net`), and **the games**
  — Unix's humanity, kept by the designers, and incidentally the draw stack's
  best free stress tests, being interactive libdraw clients of a kind samterm
  is not. The principle cuts both ways: **the absences are curation too** —
  `/bin` inherits the refusals (no `head`: the Research line's answer was
  `sed 10q`; no `find`: `du -a` piped to `grep` is the curated idiom), and the
  namespace reconciles philosophy without argument, since V10's own `find`
  will live in `/v10/bin` and bind order chooses. The boundary: the principle
  covers the utility set — things a person types — not the pivot's
  infrastructure, which stays dispositioned at the mach layer (fossil/venti,
  factotum, ndb/cs, the compilers on self-hosting grounds); `upas` parks on
  the line with a note that mail could someday arrive as a lib9p mailfs.
  **The detailed edge set is accepted with it**: `ed` (by the troff reasoning —
  Research ed's own later life, rune-aware; V10's ed will sit beside it as cat
  and echo already do), the real **lib9p** (newly portable on this week's
  libthread and bare fork; brings iostats — 9P tracing for this project's own
  development — ramfs(1), srvfs with a ~50-line `#s` device, mntgen; our
  lib9p.[ch] retires; Tflush stays a kernel-side deviation servers simply
  never see), **tar + libflate + gzip** paired with the storage decision's
  host-ingress Dev as the door in and out, the **observability pair** (`ps`
  over a devproc brought up to the real status format — the kernel conforms,
  vendored code does not bend — and `ns` over a new `/proc/n/ns` file whose
  hard half, recorded source paths on union elements, unmount already built),
  the **float door** (un-exclude fltfmt/strtod, vendor port's own Cody-Waite
  era math C; opens awk, dc, bc, hoc, seq, units, pic, grap), the **digest
  slice** of libsec (md5/sha1 only; TLS stays mach-side), real **font files**
  in the rootfs seed, `xd`, `p`, `dd`, `time` (await's times vector gains real
  wall-clock deltas). Flagged for the Rust milestone, not decided now:
  adopting the real **libmemdraw/libmemlayer** as the kernel's compositor in
  place of a draw-engine transliteration — verbatim-beats-reimplementation
  reaching into the kernel, with the suite's pixel tests making engines
  swappable. Port-time rule as ever: verify contents against the 4e tree and
  measure before claiming (the games list, cron's exact shape, tarfs's
  provenance).

- **(2026-08-27) The completeness principle for V10 — and upas resequenced, not
  refused.** Refusal and sequencing are different acts, and only the second is
  ever applied to software with Research ancestry. Stated for the specimen side
  as the curation principle is stated for `/bin`: **the V10 personality aims at
  the whole Tenth Edition userland; nothing is refused; the only exceptions are
  dead substrates, represented by their living descendants** — Datakit (its
  role passes to `/net` over IP, the `dial` abstraction preserved) and the Blit
  hardware (whose interactive layer survives as its own descendants: sam, and
  the mux lineage the window server implements; the Blit was optional
  equipment, so the text userland is the complete common experience). Under
  this principle **upas is in on both sides** — Presotto's, born in Research
  Unix, the later Research editions' mail system, carried by its author into
  Plan 9, and by design small programs over mailbox files rather than a
  monolith. Decomposed by dependency: the **local core** (marshal, mailbox
  reading, local delivery, aliases) needs no network and is meaningful now —
  ipnx is already multi-user; Plan 9's compiles on the open toolchain door,
  V10's follows the ANSI conversion, side by side per the cat-and-echo
  pattern. **Transport** (smtp, qer/runq) parks behind `/net` beside cron's
  dial-out half. **upas/fs** — mail as a 9P filesystem — rides lib9p and
  libthread, and is the most this-system-shaped piece in the garden. The
  garden sorts by the same ancestry test: in — `faces` (vismon's descendant),
  **`proof`** (the troff previewer, Blit-era roots — typeset preview in a
  window without ghostscript, closing the document factory's loop visually),
  `calendar`, `news`, the clock-tier trinkets under the games precedent;
  sequenced behind absent substrate — `page` (ghostscript), `vt`/`con`
  (`/net`), `juke`/audio (no devaudio in any mach layer yet); out on
  principled grounds — `mothra`/`abaco` fail both tests (the web postdates
  V10: no ancestry to curate, no completeness claim) and carry the heaviest
  dependency chain in the tree. The never-list's basis is henceforth: no
  Research ancestry AND not required infrastructure — discretionary rather
  than forbidden should `/net` and appetite ever coincide.

- **(2026-08-27, the re-founding) v12 is a reimagining of Unix; the V10
  completeness principle is dropped.** The project line's own teleology, stated
  plainly: ipnx v10 was the resurrection, v11 the reckoning that resurrection is
  a logical dead end, and **v12 is the counterfactual next edition** — Unix
  written afresh on a modern stack, starting from where its creators finished
  (Plan 9), undoing the compatibility break while honouring Plan 9 semantics,
  and avoiding the nightmare of Linux, BSD, POSIX-the-standard and systemd.
  Consequences, each superseding where it conflicts:
  *The V10 completeness principle (earlier today) is superseded* — the goal was
  never to resurrect a dead operating system, and by the same period-piece
  logic that excluded mothra/abaco, much of V10 would fall anyway. The V10
  personality becomes **the exhibit**: the TUHS-tape binaries stay in
  `/v10/bin` as heritage, the parent repository remains the museum, V10's
  *sensibility* — small, sharp, anti-bureaucratic — survives as taste rather
  than checklist, and V10 growth is no longer a goal (the ANSI-conversion
  dependency dissolves). Upas and the garden dispositions stand on their Plan 9
  curation-side merits.
  *The modern personality is curated by measurement, not adopted from POSIX*:
  port the three benchmarks, record every interface they demand, and that
  derived list — the 20% of POSIX that is actually Unix: fds, fork/exec/pipes,
  dirents, errno, sockets, mmap-enough — is the specification, the
  `docs/syscalls.md` method aimed at the modern surface. APE's lesson kept,
  re-aimed: it failed by chasing the whole standard.
  *WASI is elevated from footnote to second ABI*: `wasi_snapshot_preview1`
  implemented as a syscall dialect over the same chans (preopens map onto
  per-process binds; no second VFS, no second process model), which carries
  **Go** (`GOOS=wasip1` — the gc runtime is never ported natively) and
  **CPython's official wasi builds** essentially on arrival. The §6 finding is
  unchanged: the system interface is 9P; a dialect is not an interface.
  *`libunix` takes libv10's seat* as the native modern personality for source
  ports, **git the flagship** — famously portable C, NO_MMAP fallbacks,
  fork/exec-heavy, and local git needs no sockets: `git status` on a
  namespace-mounted repository is the single most persuasive demo available. A
  native CPython against libunix later exceeds the wasi build, because
  `subprocess` needs the fork/exec this kernel genuinely has.
  *Sockets win the API; files keep the implementation*: when `/net` lands, the
  personality exposes BSD socket calls translating to dial strings against
  `/net` files — the winner's interface over the elegant loser's architecture.
  *Acceptance is operational*: v12 supports modern Unix when, measurably, stock
  git does init/status/commit/log/diff on a real repository through the
  namespace; CPython runs a script that forks a pipeline through `subprocess`;
  and a Go `wasip1` binary passes its own tests under the shim. Three programs,
  three dialects, one kernel.
  *Sequencing*: the PoC still closes with acme; post-PoC the WASI shim jumps
  the curation sweeps in priority (small kernel-adjacent work, two benchmark
  languages as the prize), then libunix-and-git as the personality milestone,
  sockets with `/net`. The README and CLAUDE.md carry the manifesto from this
  date; RESEARCH's TL;DR is annotated in place.

- **(2026-08-27) The citizenship clause: ipnx lives in the wasm world in both
  directions.** WASI-as-second-ABI was accommodation — the inbound direction,
  foreign software running here. This clause adds citizenship: ipnx also
  *provides into* the component ecosystem, a neighbour among Rust, Python, Go
  and Node components rather than a walled garden that imports them. The
  reasoning: the component model answers "how do many modules make one
  program" — typed interfaces, linking — and lacks everything an operating
  system contributes: processes, identity, names, mounts, runtime composition.
  There is no `bind` in the component world; that is this kernel's entire
  inventory. **The component model has linkers; it needs an operating
  system.** Outbound, concretely: a **9P → `wasi:filesystem` bridge** serving
  an ipnx namespace — unions, private binds, exportfs-imported trees — as a
  foreign component's world, per-request namespaces for an ecosystem whose
  preopens only gesture at them; **the Rust kernel core packaged as a wasm
  component**, embeddable in other people's runtimes as a library OS (the
  runwasi *hosting* verdict stands — watched, not targeted — providing is a
  different act from being hosted); and **preview2 with `/net`**, sockets
  arriving in WASI's timeline exactly where the network milestone sits. The
  standing tension stays stated: WIT's typed interfaces and 9P's uniform
  untyped one do not compose as system interfaces (RESEARCH §6, unchanged) —
  the resolution is topological, **typed at the edges, files at the core** —
  dialects and adapters at the boundary, nine-ish operations on names within.
  Node needs no new posture: it cannot be a guest and was the first mach
  layer — living beside the modern components is the architecture's origin
  story, now policy. This is genuinely new and beyond Plan 9, and embraced as
  such.

- **(2026-08-27) The native-to-the-new-world aspirations: cloud-native,
  AI-native, Kubernetes-native — stated now, sequenced later.** All
  aspirational, none blocking, each admitted only because it passes the one
  test: *does it become a file tree in a namespace?* **S3 over 9P** — the
  webfs pattern (Plan 9 served FTP and HTTP as file trees) aimed at a bucket:
  objects as files, an `s3fs` guest behind `/net` or a mach service before
  it, and the same treatment for any cloud API with nouns. **Lambdas both
  ways** — functions as files (write payload, read response), and the deeper
  symmetry: Lambda runs on Firecracker, and the cloud-machine rung IS the
  Firecracker architecture — ipnx *as* the function is the already-specified
  deployment form meeting its market. **Inside k8s** — mach-layer courtesies:
  logs to stdout (cons already is), config as namespace scripts (decided),
  SIGTERM as a note, probes as an exec'd rc script or a served health file,
  PVCs as 9P mounts (the storage decision verbatim). **Hosting workloads** —
  honesty draws the credible line: ipnx executes wasm, not Linux ELF, and
  never pretends to host arbitrary Docker images; the honest form is **a
  Kubernetes node for wasm workloads** — a CRI shim mapping pods onto
  processes plus namespaces, a stronger isolation and composition story than
  the existing wasm shims, because pods are namespace assembly and that is
  this kernel's native verb. **AI-native** — models as files (`/mnt/llm`:
  write prompt, read completion; sessions as directories) is the easy half;
  the strong half is that **a per-agent namespace is the capability model the
  agent world is groping toward**: an agent's whole visible world assembled
  from binds and unions, nothing else reachable by construction, iostats as
  the audit log, MCP-shaped tool access as "mount this tree". That one is not
  ipnx keeping up with AI; it is ipnx holding the answer to agent sandboxing
  that the ecosystem currently fakes with allowlists. Sequencing: post-PoC,
  mostly post-`/net`, none ahead of the benchmarks.

Evidence for each is in RESEARCH.md at the cited section.

- **The kernel call list is derived** — [docs/syscalls.md](syscalls.md), call by call.
  Of Plan 9's 40 live calls, 29 never leave the supervisor; 11 are the file interface, ten
  of them one 9P message each and `mount` the wire boundary itself.
- **9P2000** (§2). `version(5)`: "Currently, the only defined version is the 6 characters
  ″9P2000″." Original 9P survives only in period servers this system never has to speak to.
- **Wire 9P at boundaries, a Dev table inside** (syscalls.md). Plan 9's own kernel shape:
  devices present the file interface as function calls; only the mount driver marshals 9P.
- **Base: Plan 9 4th edition as reference, 9front consulted for fixes** (§10) — both MIT,
  and the kernel is transcribed structure, not a forked tree.
- **The lazy fork has its resume mechanism, and its bound** (RESEARCH §5.2): the child's
  `exec` throws, a hand-assembled `try_table`/`catch_all` guard frame catches, the
  supervisor restores the `[0, sp)` stack region it saved at fork, the guard returns the
  child's pid. The catch frame must be live when the child execs, so the child's pre-exec
  code runs inside the guard's extent — **`procrfork(flags, fn, arg)`**, Plan 9's own
  thread-library shape. Bare dual-return `rfork(RFPROC)` on a JS engine stays asyncify's
  case; the native interpreter owns its frames and can lift the restriction. Proven
  end-to-end in [poc/](../poc/).
- **Syscall transport: a Worker is a process** (§5.3). Blocking calls are an unanswered
  SAB mailbox plus `Atomics.wait` in the Worker; no asyncify anywhere in the base system.
  In the browser this costs cross-origin isolation (COOP/COEP); in Node nothing.
- **One guest substrate.** Wasm on every platform; the JS-engine path is built first.
- **`/dev/tty`: there is none.** The console is `/dev/cons`; the V10 personality's libc
  aliases the name. The fd-3 accident stays history.

## Open questions

- ~~The uid model~~ — **decided and running**: [docs/uid.md](uid.md). Per-process
  credentials in the kernel, `/proc/<pid>/ctl` transitions with no new syscalls,
  9P2000.u's `DMSETUID` at exec, V10 enforcement in-process and per-attach identity on
  the wire. The answer to "is V10 compatibility real?" is yes.
- ~~Hard links and the symlink family~~ — **decided and running**. The choice was
  *mint*: `Tlink`/`Tsymlink`/`Treadlink` live at types 128/130/132, a range no 9P
  dialect uses, so the base version stays plain `9P2000` and a server without the
  extension answers `Rerror` — the client degrades gracefully (tested against one).
  Symlink identity rides 9P2000.u's `QTSYMLINK`/`DMSYMLINK` bit positions, like
  `DMSETUID` before it. Three kernel calls join the interface (traps 60–62; `lstat` is
  `stat` with a nofollow flag, not a call), and resolution follows **V10's rule: the
  kernel resolves symlinks in the walking process's own namespace** — no server knows
  the client's namespace, which is why this cannot be delegated. A symlink created
  through a mount into an exporter's tree, read back through the mount, resolves to the
  *client's* `/etc/motd` — the acceptance test that settles it.
- kencc or clang for the *ported* userspace, given `extern register` and anonymous struct
  members? Fresh code is clang (§9.4 is the measured recipe).
- Does the `d` message's alpha compositing map cleanly onto canvas and Metal, or does the
  window server rasterise? Decides whether the backend is thin.
- Heap sizing where guest memory is *shared* with the supervisor for zero-copy I/O — a
  shared memory must declare its maximum up front. The unshared default grows freely.

## The proof of concept (2026-08-26)

[poc/](../poc/) is the architecture's working slice: the supervisor in Node, guests in
freestanding C compiled with wasi-sdk's clang (RESEARCH §9.4), one Worker per process,
per-process namespaces, Plan 9 trap numbers, 9P2000 stat records — booting to **the
real 4th-edition `rc`** over a writable ramfs and twenty-four real commands, and
**wire 9P at a mount boundary**: `hellofs`, a 9P2000
server in a guest process serving on a pipe, is attached with `mount(2)`
(Tversion/Tattach), and every operation below the mount point is one tagged wire message
through the mount driver — the only place the kernel marshals 9P, exactly the
Dev-table-inside decision — and **the asyncify path**: bare dual-return `rfork(RFPROC)`
on transformed binaries (RESEARCH §5.2), which is what runs every fork the real rc
makes — pipelines, subshells, captures — as forked copies of the interpreter. 102
acceptance tests pass
end-to-end: kernel, mount and fork tests (lazy-fork resume, namespace isolation through
`exec`, Twalk/Topen/Tread/Tstat/Twrite over the wire, integral directory reads, `Rerror`
arriving as `errstr`, two processes sharing one connection, dual return with copied
memory and intact parent locals) and seventeen shell tests (pipelines, `` `{...} ``
substitution, glob, redirections, `$status`, `for`/`if`, `bind` as an ordinary command,
subshells as pipeline stages with copy semantics and status propagation).

```sh
bash poc/mk.sh && bash poc/run.sh     # the tests
bash poc/run.sh -i                    # boot to an interactive rc
```

Union directories complete the namespace algebra — `bind -a`/`-b`/`-c`, ordered walks,
concatenated directory reads, creates landing in the MCREATE element — and **exportfs**
completes the boundary: a guest serving its own namespace (private binds included) over
wire 9P, from which another process can read, list, and **exec binaries**. Both
directions of the mount boundary are now guest-reachable.

The kernel is **one platform-neutral module with two hosts**: `bash poc/run.sh` boots it
on Node, and `node poc/serve.mjs` serves the same kernel into a page (the server exists
only to set the COOP/COEP headers SharedArrayBuffer requires) — the full suite passes in
Chrome 148, and `?i` boots the page to an interactive rc in a console window
(RESEARCH §5.3, §7 for the Wanix/Apptron precedent that shapes where the GUI goes next).

What it deliberately does not do is listed in [poc/README.md](../poc/README.md). The
**The userspace objective, stated once**: real Plan 9 userspace and real Research Unix
V10 userspace, compiled to wasm from their own trees, running side by side on this
kernel — each against its own libc.a rewritten over the kernel interface (`lib9` for
Plan 9, `libv10` for V10; both this project's code, in poc/). The first citizens of each
are in: 4th-edition `cat` and `echo` compiled **unmodified** through shim headers and now
doing all the suite's work, and TUHS-tape V10 `cat` and `echo` — K&R C, `-std=c89
-fno-builtin`, implicit declarations left authentic — living in `/v10/bin`, fingerprinted
by `echo -e`, and piping into Plan 9 `cat` in a single pipeline. Growing both userspaces
command by command is now the PoC's standing work — and the first sweep landed: the
REAL `/sys/include/libc.h` over one platform shim, the real libc (port/fmt/9sys),
libbio, libregexp and libString as `libp9.a`, twenty-four real commands including
`ls`, `sed`, `grep` and `sort` — and **the real `rc`** (bison over `syn.y`,
asyncified), running the whole suite interactively, in batch, and in a window, over a
kernel readied for it: notes delivered at the syscall boundary, `alarm`, `unmount`,
honest rfork flags (`RFNOMNT`/`RFCNAMEG`/`RFCFDG`/`RFNOWAIT`/`RFNOTEG`), the dup
device `#d`, and `..` in walks. V10 growth waits, per direction, for the parent
project's ANSI conversion of its userspace. Platform order ahead: **macOS native first,
then iPadOS**, as a **Rust kernel core plus per-platform embedding shims** (decision
below, 2026-08-27). The engineering lifts the plan named are done, the uid model is
designed ([docs/uid.md](uid.md)) and running, the window server speaks the real draw
device protocol (screens, window views, clipping, channel-correct uploads,
`b d f L e E y i l s x c A F t O v`), and **the whole editor runs**: the real `sam`
over the real `samterm`, libframe over libdraw over the wasm libthread, typed at in a
browser window and headlessly under samtest. And **the PoC is closed** (2026-08-27):
**the real `acme`** — the GUI decision's declared real test — boots in a browser
window and under acmetest, all twenty of its source files verbatim through the
derivation layer (the load-bearing find: kencc adjusts pointers to unnamed
substructures at call sites and clang does not — RESEARCH §9.5's `frameadjust.h`
shape), its own 9P file server armed over a pipe, the mouse crossing wctl with
button-2 execute and button-3 look verified in the raster, the float door opened
(`strtod`/`fltfmt` verbatim over the real `FPdbleword`), and the kernel grown its
last two PoC devices: `#s` (srv — a posted fd's channel kept alive by name, which
is what makes acme's error pipe park instead of EOF-spinning) and `#d` bound at
`/fd`. **124 acceptance tests, green on Node and in Chrome.** The PoC has nothing
left to prove; the native work begins. And the post-PoC queue's first item is
already moving (same day): **the WASI second ABI runs** — `wasi1.mjs`
implements `wasi_snapshot_preview1` over the same mailbox with fd 3, the one
preopen, as the namespace root; a wasi-libc citizen and a **real Go binary**
(`GOOS=wasip1`, go1.25.6) read the motd, list directories, round-trip files
and sleep on `poll_oneoff`, on both hosts — and **REAL CPython 3.14.7** (the
wasi build, 30.5MB) boots by landmark, imports its stdlib from a 21-file
measured subset (`wasi/pylib.txt` — everything else is frozen into the
binary), runs a script out of the namespace and round-trips json — **130
tests**. Two of the three benchmark runtimes speak to the kernel; git's
`libunix` port is the third.

## Sources

- [Plan 9 from Bell Labs (design paper)](https://9p.io/sys/doc/9.html) ·
  [`intro(2)`](https://9p.io/magic/man2html/2/intro) ·
  [syscall numbers](https://raw.githubusercontent.com/0intro/plan9/master/sys/src/libc/9syscall/sys.h) ·
  [`rfork(2)`](https://9p.io/magic/man2html/2/fork) · [`rio(4)`](https://9p.io/magic/man2html/4/rio) ·
  [`draw(3)`](https://9p.io/magic/man2html/3/draw) · [`cpu(1)`](https://9p.io/magic/man2html/1/cpu) ·
  [`a.out(6)`](https://9p.io/magic/man2html/6/a.out)
- [APE — The ANSI/POSIX Environment](https://9p.io/sys/doc/ape.html) ·
  [Plan 9 C Compilers](https://9p.io/sys/doc/compiler.html) ·
  [How to Use the Plan 9 C Compiler](https://9p.io/sys/doc/comp.html) ·
  [Adding Application Support for a New Architecture](https://9p.io/sys/doc/libmach.html)
- [9vx](https://swtch.com/9vx/) · [9VX wiki](https://9p.io/wiki/plan9/9vx/index.html) ·
  [Vx32 (USENIX '08)](https://pdos.csail.mit.edu/papers/vx32:usenix08.pdf) ·
  [Inferno ports: hosted and native](http://doc.cat-v.org/inferno/4th_edition/inferno_ports) ·
  [Harvey OS / APEX](https://github.com/Harvey-OS/apex/wiki) ·
  [plan9port `devdraw`](https://9fans.github.io/plan9port/man/man1/devdraw.html)
- [WasmFX explainer](https://wasmfx.dev/specs/explainer/) ·
  [Binaryen's Asyncify](https://kripken.github.io/blog/wasm/2019/07/16/asyncify.html) ·
  [Emscripten: Asynchronous Code](https://emscripten.org/docs/porting/asyncify.html) ·
  [WASIX `proc_fork`](https://wasix.org/docs/api-reference/wasix/proc_fork)
- [WASI proposals and phases](https://github.com/WebAssembly/WASI/blob/main/docs/Proposals.md) ·
  [WASI roadmap](https://wasi.dev/roadmap) · [WASI 0.3 launched](https://bytecodealliance.org/articles/WASI-0.3) ·
  [wasi-filesystem README](https://github.com/WebAssembly/wasi-filesystem)
- [WasmKit](https://github.com/swiftwasm/WasmKit) ·
  [Pulley — wasmtime's portable interpreter](https://docs.wasmtime.dev/examples-pulley.html) ·
  [wasmtime tiers of support](https://docs.wasmtime.dev/stability-tiers.html) ·
  [wasmi](https://github.com/wasmi-labs/wasmi) ·
  [wasm3 performance](https://github.com/wasm3/wasm3/blob/main/docs/Performance.md) ·
  [LLVM D46141 — `--stack-first`](https://reviews.llvm.org/D46141) ·
  [goken9cc](https://github.com/aryx/goken9cc) · [Wanix](https://github.com/tractordev/wanix)
- [ZenFS](https://zenfs.dev/core/) · [OPFS](https://web.dev/articles/origin-private-file-system) ·
  [`createSyncAccessHandle()`](https://developer.mozilla.org/en-US/docs/Web/API/FileSystemFileHandle/createSyncAccessHandle) ·
  [xterm.js](https://github.com/xtermjs/xterm.js) ·
  [iOS sandbox: no child processes](https://developer.apple.com/forums/thread/747499)
- Measurements against Research Unix V10 come from the parent repository
  ([ipnx](https://github.com/ChristineTham/ipnx)): `usr/src/sys/os/{sysent.c,mount.c}`,
  `usr/src/sys/{io,vm,md,ml,fs}/`, `usr/src/cmd/sh/xec.c`, `usr/src/libc/sys/open.s`
