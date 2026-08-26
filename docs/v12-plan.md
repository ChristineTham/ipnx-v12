# ipnx-v12 — the design

*Scope framing, 2026-08-26. Nothing here is built. This document exists to make the
question answerable: **what is Unix if you design it today, keep Plan 9's answers, and
put back the compatibility Plan 9 threw away?***

**The statement, once.** A **modified Plan 9 kernel hosted as an ordinary userspace
process** on macOS, iPadOS and the browser; **9P as the only IPC**; **per-process
namespaces**; **everything exposed as a file**; **WebAssembly as the executable format**;
and a **Research Unix Tenth Edition personality** running alongside Plan 9's own userland.

No VAX. No disk image. No emulator.

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

## Decisions (2026-08-26)

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
per-process namespaces, Plan 9 trap numbers, 9P2000 stat records — booting to **a minimal
`rc`** with pipes, a writable ramfs, nine commands (`cat` `echo` `ls` `wc` `cp` `mkdir`
`rm` `bind`, plus `init`), and **wire 9P at a mount boundary**: `hellofs`, a 9P2000
server in a guest process serving on a pipe, is attached with `mount(2)`
(Tversion/Tattach), and every operation below the mount point is one tagged wire message
through the mount driver — the only place the kernel marshals 9P, exactly the
Dev-table-inside decision — and **the asyncify path**: bare dual-return `rfork(RFPROC)`
on transformed binaries (rc costs +5.5%, RESEARCH §5.2), which is what runs rc's
subshells `(...)` as forked copies of the interpreter. Forty acceptance tests pass
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
engineering lifts the plan named are done, the uid model is designed
([docs/uid.md](uid.md)) and running, and **the window server exists in v0 subset**
(RESEARCH §7): `#w` mints windows, `bind '#w/N' /dev` makes a namespace a window,
`/dev/draw` is an actual per-window file speaking draw(3)'s `b d f L e E v`, and
`win rc` is a shell in a browser window, and **text lands in draw**: `y`/`i`/`l` carry
an 8×8 font of our own authorship into a cache image, `s` draws strings through it,
glyphs asserted by pixel on both hosts. Next: **the `sam` port** (libframe over exactly
these messages), the native host over WasmKit, and the link/symlink protocol decision.

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
