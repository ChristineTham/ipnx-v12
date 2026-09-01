# ipnx-v12 — the design

*Scope framing 2026-08-26; re-founded 2026-08-27 (decision log). The question this
document exists to answer: **what is Unix if you write it afresh today, keep Plan 9's
answers, and put back the compatibility Plan 9 threw away?***

*Role: the **why** — rationale and the decision log. What the system **is** is
[architecture.md](architecture.md); **how** to work on it, [handbook.md](handbook.md);
**when** it gets built, [implementation.md](implementation.md); **where** it runs and
where things live, [platforms.md](platforms.md); **who** a user is,
[identity.md](identity.md), and who it is **for**, [personas.md](personas.md);
what **was**, [poc.md](poc.md).*

**The statement, once.** **The IPNX kernel — Plan 9's architecture, none of its
code — hosted as an ordinary userspace process** — browser, macOS, iPadOS, OCI,
eventually hypervisor-direct; **9P as the only
IPC**; **per-process namespaces**; **everything exposed as a file**; **WebAssembly as
the executable format**; and **personalities as libc dialects** above the one kernel —
Plan 9's userland entire by the curation principle, a WASI second ABI, and a **modern
Unix personality** derived by measurement against git, CPython and Go. The V10 exhibit
stays as heritage; its completeness is not a goal.

No VAX. No disk image. No emulator. No POSIX, no systemd, no sediment.

## Why the kernel's architecture is Plan 9's and not V10's

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

- **(2026-08-31) emca is the user interface, and IPNX is a whole operating
  system.** The largest reframe since the re-founding, and it reorders much of
  what follows. Christine, after four days of designing what everyone assumed
  was an editor: *"What we have been designing is not acme, or a replacement
  for acme. It is the shell that IPNX boots into, it is the IPNX primary user
  interface, it is the browser surface, the macos app, the ios app. The system
  boots into emca. Emca is the user interface."* And on what that makes the
  project: *"It's not just a barebones UNIX reimagined, it is a full operating
  system with our own semantics, user interface, artifacts."* The full design is
  [emca.txt](emca.txt); the parts list it derives from is [acme.txt](acme.txt).
  **The mechanism, and why it is an IPNX design rather than a portable one**:
  *"Traditional Unix and Linux manages different types differently, using
  separate commands. ps list processes, there are separate commands to manage
  network connections, packages, users, etc. In IPNX everything is managed as a
  file. That's what makes emca work."* There is no process manager program, no
  package-manager GUI, no network panel — there is a filesystem, a **window
  type** declaring which verbs its files accept, and a surface rendering those
  verbs natively. Adding a manager to the system is adding a file. A rich system
  UI normally destroys acme's central property (any text can be a verb's
  operand) because a process table becomes a native widget; **here it cannot,
  because the managers are already text filesystems** — `/proc/1/status` *is*
  text, so a process window is a text window that happens to carry `Kill`. The
  property holds by construction. On a system where the process table is a
  syscall, the widget toolkit returns and acme dies inside it. **The
  architecture, hers**: *"you need to split emca design into two halves — a half
  that lives in IPNX, and a half that is native to the surface."* IPNX owns
  state, meaning and policy; the surface owns rendering and input; the test that
  settles any case is *differs between a Mac and an iPad → the surface's;
  differs between one workspace and another → IPNX's*. It earned its keep
  immediately on an unanticipated question — credential key custody is the
  surface's (Keychain, WebCrypto, a passphrase), the credential listing and its
  verbs are IPNX's. **A window type is a triple** — namespace, *optional*
  command, window configuration — and the command being optional dissolves the
  types-vs-programs fork: no command means emca renders the tree, a command
  means the program drives the window through `/mnt/emca` (acme's client model,
  unchanged), so `/proc` can start as a one-line type and grow a live `ps`
  without the type system changing. `/type` is itself a type, so the interface
  is configured by editing files *in* the interface — no plugin API, no manifest,
  no restart; one built-in type (`dir`) is the bootstrap floor. **`/pkg` and
  `/project` are different kinds** (hers, replacing an earlier single "recipe"
  registry that was trying to be both): a package is a toolchain, command or
  library and is a *leaf*, bound not instantiated; a project *combines* packages
  into a namespace and becomes a process. A two-level graph, not an arbitrary
  DAG — and it is what `pkg.c` already was ("installing BINDS it; the namespace
  is the installation record"). **A project is a proto-process**, which names the
  symmetry: `/pkg/go` is what it is made of, `/project/ipnx` the process that
  could be, `/proc/1741` the process that is — so "save as project" is reading
  `/proc/N/ns`. Templates instantiate, workspaces open; `/project` is a union of
  system and personal, so promotion is moving a file between union elements, and
  **promotion promotes the declaration, not your files**. Clone and instantiate
  are separate acts, and instantiate shows the declaration first — a cloned
  project file is a stranger's declaration, the hole Docker and devcontainers
  both have, and IPNX's answer is not a mitigation: the blast radius is readable
  in one small file, and the namespace bounds it whether you read it or not.
  **Naming, measured** (the house rule beating four rounds of opinion): sixteen
  root names across the vendored Plan 9 source, **not one over four characters**
  — so a root is ≤4 characters, abbreviation is what you do when the word is
  longer, and a directory is named for what one of its *entries is* (`/proc/17`
  is a proc), which is why Unix never wrote `/procs`. Expanding everything was
  refused by two standing decisions, not by taste: "modern software must run"
  (CPython, Go and git carry hardcoded `/tmp`, `/bin`, `/lib`, `/usr`) and
  "vendored sources are verbatim" (571 references to `/lib`). `/project` is the
  **one exception**, recorded with its reason so it constrains rather than
  licenses: `/proj` is one keystroke from `/proc` and adjacent in meaning, so it
  is a tab-completion footgun rather than merely a longer word. Rejected en
  route, with reasons kept so the search is not redone: `recipe` (6), `rec`
  (opaque), `menu` (32 uses in the UI sense in these documents, and the design's
  founding quote is *"Acme doesn't need menus"*), `kit`/`app` (overloaded),
  `spec` (geeky). **The protocol amendment**: acme.txt's constraint 2 expected
  "little or no protocol change" and this exceeds it — four additions to
  `/dev/canvas` and no more (structure roles, window type, verb applicability,
  show request), recorded as deliberate. Canvas v0 anticipated the direction
  ("span attrs arrive with the web presenter's real links, later"); emca is the
  benchmark that demands them, which is how every element of v0 was derived.
  **Consequences, dispositioned**: "one editor" ([userland.md](userland.md))
  becomes **one surface** — the editor is one window type; **rio-today retires**
  as a separate design, absorbed (`#w` still mints windows; a program's own
  canvas window appears as an emca window of a canvas type); the browser page,
  the macOS app and the iPadOS app **stop being hosts that run a demo and become
  surfaces of emca**, so the public page is the system's face rather than a
  demonstration of it; and **the "no phone form factor" refusal is reversed** —
  it was recorded 2026-08-29 on the evidence that no persona's journey contained
  one, and the new evidence is that responsive design in *characters rather than
  pixels* makes the phone one value of a knob built anyway, not a separate
  design. Reversal by evidence, per the standing rule, not by fresh opinion.
  Largest open item, named rather than buried: **the surface half has no test
  story** — emca's behaviour stays testable through the virtual surface, but
  nothing in the tree proves a button was reachable or that Tab reached it, and
  "no half-working" plus the keyboard-complete law make that the design's biggest
  untested area, with no precedent in this repository to borrow from.

- **(2026-08-31) The system is named Saranos; IPNX is the kernel; emca is the
  interface.** The companion to the reframe above: once emca became the user
  interface and the project became "a full operating system with our own
  semantics, user interface, artifacts", the layer that had no name was the
  system itself. Christine's observation that opened it: Apple names XNU, Darwin,
  SwiftUI, macOS and MacBook separately, and IPNX had one name doing several
  jobs. **The layering, decided**:

  | | |
  |---|---|
  | **Saranos** | **the user experience** — emca the shell ([emca.txt](emca.txt)), the window types, the presenters, the surfaces' furniture |
  | **IPNX** | **the kernel AND the userspace** — the file world (Darwin's slot, not XNU's) |
  | wasm, and the surfaces | the machine it runs on |

  Christine's correction, which settled where the line falls: *"IPNX is still
  there, it is still the userspace and kernel. Saranos is what goes on the top —
  a user experience."* So IPNX keeps everything it already named — kernel,
  libcs, commands, `/pkg`, the whole file world — and Saranos names only what
  a person sees and touches. `/dev/canvas` is the interface between them: an
  IPNX device that Saranos consumes.

  A symmetry worth recording, because it says the layering is not arbitrary:
  **XNU stands for "X is Not Unix", and IPNX is "IP is Not UNIX"** (README's own
  etymology) — the two kernels carry the same joke, and Apple's answer to the
  layer above it was a human name, not a second acronym. **Saranos** is from
  Sanskrit *śaraṇa* (शरण) — **refuge, shelter, sanctuary**. Her reading, which
  is the naming rationale and leads: it is *"a refuge from the complexities of
  the modern computing environment"* — a refuge for the PERSON, which is what
  makes it a name for the experience layer rather than for the kernel. Her
  criteria, in her words: *"easy to pronounce, mixed etymology, it sounds like a
  word but isn't, and conveys serenity"* — the phonetic reading (*saran-* /
  *seren-*) and the Sanskrit agree, which is rare in a coinage. A second reading
  runs underneath and is structural rather than felt: a *process* also runs in a
  refuge bounded by what it was given — from the README, *"IPNX v12 processes are
  secure by design. It assumes from day one that the running application is
  malicious"* — which is P4's *"what could it touch?"* answerable by
  construction. The two readings are one word doing both jobs, at the two layers
  the name now separates. **The mixed etymology is a feature, not a compromise**:
  a Sanskrit root with a Greek-shaped ending, for a system that is itself a
  synthesis — Plan 9's architecture, Unix's interface, wasm's world. The name's
  construction mirrors the thing's. **Searched before committing, and clean**: zero
  software, zero operating systems, zero developer tools; the hits are a South
  African restaurant franchise, a surname, and an Elder Scrolls NPC.
  **The rejections, kept so the search is never re-run** — nine candidates over
  seven rounds, every one killed by measurement rather than taste:
  `Ecma` (Ecma International / ECMAScript, in a system built on JS engines — and
  acme backwards is *emca*, not *ecma*) · `Kitty`/`KittyOS` (kitty the terminal
  emulator, plus two existing operating systems, the most prominent headlined
  *"Writing A Toy OS"*) · `Mimmy`/`Mimi` (Sanrio's actively enforced portfolio —
  and *Sanrio v Dong-A Pencil* found against **KITTY** alone; naming the system
  after a defended character would turn IPNX's own IP-litigation joke into the
  thing) · `MimiOS` (free, but roots in *mimic* — which argues against the
  2026-08-30 decision that IPNX is not a modified Plan 9 — and files beside
  Minix, Mimix, Mimiker, all teaching systems) · `Miaos` (two existing OSes, one
  of them a recursive acronym, the same genus as IPNX) · `Ailuros` (a live
  local-first AI-agent studio at ailouros.io, in P4/P5's territory — and it
  begins with `AI`, which in 2026 is read before any Greek) · `namastos` (a weld,
  homage names nothing about the system, and reads as a yoga pun) · `loka`
  (Sanskrit *world*, apt — but Loka Inc. is an AWS Innovation Partner of the Year
  selling to the same audience since 2004) · `viharos` (Sanskrit *dwelling* — but
  already a word in **Hungarian**, meaning *stormy*) · `marjaros` (Sanskrit
  *cat*, clean in search — but two characters from **Manjaro**, an operating
  system) · and `topos` (Greek *place*, free, apt, seriously-neighboured — and
  declined; recorded because it cleared every test and still was not the name,
  which is data about how naming actually resolves). **The transferable rule,
  measured across all of them**: *the company a name keeps in search results is a
  positioning decision made before anyone reads a word you wrote* — and for a
  project whose recorded anxiety is "I feel like we are underselling what has
  been achieved", that is not a tiebreaker but an argument. **And the cat goes
  where Bell Labs always kept it**: glenda is Plan 9's mascot and Plan 9 is the
  name. Six of the nine rejections were cats; the affection was fighting for the
  wrong slot. A cat becomes Saranos's mascot, costs nothing, clears nothing, and
  `/usr/kitty` stays exactly as it reads in every example.
  **Scope of the rename, and its limit**: dated entries in this log and in the
  other records are HISTORY and keep the words they were written with — a log is
  not retroactively renamed. Only present-tense statements of what the system
  *is* take the new layering ([CLAUDE.md](../CLAUDE.md), [emca.txt](emca.txt),
  [platforms.md](platforms.md), and architecture.md when the contracts land).
  README.md is Christine's and is not touched.

- **(2026-08-31) Editing is the surface's: emca implements sam, not a WYSIWYG
  editor.** The two-halves split taken to its conclusion — and the first draft
  of [emca.txt](emca.txt) had the rule and stopped short of it. Christine:
  *"emca doesn't really need to implement a WYSIWYG editor on the IPNX side. It
  can implement sam, a batch editor. The job of emca is to push a file into a
  window via /dev/canvas. The host side can display and scroll the file, and
  more importantly edit it using Monaco or TextEdit or similar."* Extended over
  three further messages: *"selecting, copying, pasting can all be host side
  operations, with sync to /dev/snarf"* · *"even command history and shell
  command line edit — host implemented"* · *"we can even bring xterm/js back"*.
  **The split test already decided this and was not followed through**: does
  editing behaviour differ between a Mac and an iPad? Profoundly — TextKit is
  not Monaco. So editing is the surface's, and "the surface owns text input"
  was the same rule applied only as far as the caret. **What moves**: the caret,
  selection, insertion and deletion, keystroke undo, syntax highlighting,
  folding, multi-cursor, find-in-file, IME, autocorrect, dictation, the
  clipboard (synced to `/dev/snarf`), **command history and line editing**, and
  **terminal emulation**. **What IPNX keeps**: the authoritative buffer
  (shadowed from events), the file (`Get`/`Put`), commands and `+Errors` and the
  `| < >` filters, **sam's structural language**, context, what the verbs *mean*,
  and the file interface. **So emca is a file server with a workspace, not an
  editor** — it pushes text into windows, runs commands, applies
  transformations. **Why this is "best of both worlds" rather than a
  concession**: Monaco and sam are *complementary, not competing*. Monaco is
  interactive, local and cursor-scoped; sam is batch, structural and file-scoped
  — `x/re/` over every match, `s//repl/` through an address, transformations no
  cursor can express. Every other system has one or the other, or bolts a macro
  language onto an editor that owns the buffer; here the buffer is a **file** and
  the surface is a **view**, so the halves compose by construction. Nobody has
  shipped both well together because nobody else had this boundary. **What it
  retires, none of it to be written**: the hand-rolled caret, guest-side
  readline, escape-sequence history, syntax highlighting, folding, multi-cursor,
  find-in-file — inherited rather than implemented, which is the same move as
  borrowing V8 and wasmtime under the kernel (the founding pattern: borrow the
  era's engines through a narrow waist). **And it reconciles the console's "AND,
  not XOR"** ([userland.md](userland.md), 2026-08-30): the transcript is the
  **line-oriented door**, where history and editing are the host's; **xterm.js is
  the raw-input door**, for programs wanting keystrokes rather than lines. That
  was recorded as a concession to familiarity and is now the consistent answer,
  with an architectural reason. **Four risks, recorded as open rather than
  waved past**: (1) **property 1** — *any text can be a verb's operand* is the
  claim the design rests on, and Monaco owns the body's text, so `execute` and
  `look` must ride *its* action and context-menu API; it looks possible and must
  be PROVEN before adoption. (2) **Undo has two owners now** — the surface's at
  keystroke granularity, emca's at command granularity (`acme.c`'s sequence
  numbers); acme had one, and which answers ⌘Z is undecided and will be felt
  immediately. (3) **Buffer fidelity** — the shadow discipline is *apps never
  re-read* (con(1)'s, load-bearing); Monaco must report every edit, multi-cursor
  emits many ranges per keystroke, and if surface and app ever diverge the only
  repair is a re-read, which breaks the discipline. (4) **The surfaces now differ
  in dependencies**, not only in grammar — Monaco (~5MB) on the web, TextKit on
  Apple. Consequence for the plan: **M14c shrinks substantially** (emca's IPNX
  half is a file-pusher plus sam) and **M14d gains the editor component**;
  implementation.md is amended in the same commit.

- **(2026-08-31) `/dev/canvas` was over-derived: the redesign into 9P, `/dev/window`,
  `/type`, and a canvas narrowed to drawing.** The largest correction to a landed
  contract so far, and it was forced by the founding decisions rather than chosen.
  Christine, stopping a build that was going the wrong way: *"You are trying to
  push everything through a protocol that should have been an exception rather
  than the rule."* And the test she applied to the result: *"This is the only
  solution that fits the principle (everything is a file, per process namespace,
  and 9P is the only protocol)."* **The framing error, named**: canvas.md's own
  title is *"the display protocol"* — but **9P is the only protocol** (founding).
  Calling canvas a protocol invited treating it as the place all host/IPNX
  exchange happens, and it grew until it carried layout, chrome, text and
  drawing. It was always files; the name is what made it universal.
  **The measurement record shows exactly where it went wrong.** v0 was derived
  from four benchmarks — console-today, acme-today, rio-today, one plot. Under
  the split: console-today is a text file plus a line back-channel; acme-today is
  a file plus chrome; rio-today is window management; **only the plot was
  drawing.** Three of the four were never canvas consumers, so `stack`, `text`
  and `edit` are generalisations from things that wanted files and chrome — which
  is also why `frame` and `image` never found a consumer. The measurement
  discipline caught its own over-derivation, a day late, which is the system
  working. **The redesign, four semantics over ONE protocol** (9P throughout —
  these are not four protocols but four conventions):
  **(1) Content is 9P, directly.** emca names a file; the host mounts it and
  renders it natively — text, SVG, HTML, Markdown, PostScript, images.
  **IPNX IMPLEMENTS NO RENDERERS AT ALL**, which is the deletion that makes the
  redesign worth doing, and is the founding pattern again (borrow the era's
  engines through a narrow waist — the host's renderers *are* the era's engines,
  as V8 and wasmtime are under the kernel).
  **(2) `/dev/window/<type>/<n>` is the control interface**, bidirectional, a
  numbered directory of small files in the house shape (`/proc/17`,
  `/net/tcp/0`, `/mnt/acme/27`): IPNX declares the chrome — these buttons, this
  one runs `Put` in IPNX, that one toggles wrap host-side — and the host reports
  back what the user did. **The window's TYPE IS IN THE PATH**, not an attribute:
  `/dev/window/proc/1` tells the host it is drawing a proc window, exactly as
  `/net/tcp/0` differs from `/net/udp/0`. This is `#w` grown up — the device
  keeps minting and owning windows and gains the interface it should have had.
  **(3) `/type` is the registry both sides read**: what window types exist, what
  IPNX command drives each, and what the semantics of that type's host/IPNX
  exchange are. It answers "how does the host know what this is" without
  sniffing, and it is the same registry the emca design already needed.
  **(4) `/dev/canvas` narrows to its name** — genuine drawing, the classic
  turtle vocabulary: open a rectangle, draw a circle, place text. The
  **exception**, for programs that actually draw. `path` survives (it came from
  the one real benchmark); `stack`, `text` and `edit` retire to `/dev/window`
  and to files.
  **Editing and `Put`** *(refined by the next entry: emca holds the AUTHORITATIVE
  buffer and the host mirrors it — the phrasing here predates that question being
  raised)*: the host holds the edited buffer, and on Save *"tells
  IPNX here is the edited file and streams it over 9P"* — so writing back is an
  ordinary 9P write, and dirty state is reported for `Putall` and for `Exit`
  refusing. This completes the 2026-08-31 *editing is the surface's* decision:
  not only does the host edit, it returns the result through the one protocol.
  **What retires, none of it to be written**: canvas's `stack`/`text`/`edit`
  kinds, the shadow-buffer discipline *for display*, every renderer emca would
  have needed, most of acme.c's canvas-writing machinery, and the `role=`/`type=`
  canvas attrs added earlier the same day (they belong on `/dev/window`).
  **Open, and worth deciding early**: whether `/dev/window/<type>/<n>` and
  acme's own `/mnt/acme/<n>` are ONE window vocabulary or two that rhyme — same
  files, opposite directions, and the only difference today is that
  `/dev/window` declares toolbars and actions, which acme's interface arguably
  should have had. Consequence for the plan: canvas.md is narrowed with its
  over-derivation stated in place, `/dev/window` is specified, and M14 is
  restructured — its protocol stage shrinks to *narrow canvas, specify
  `/dev/window`*, and its surface stage gains the host's renderers for free.

- **(2026-08-31) The buffer contract: mirrored with detectable divergence, one
  undo stack, `/mnt/acme` merged, and versioning as policy.** Four consequences
  of *editing is the surface's*, settled together because they interlock.
  **(1) emca holds the authoritative buffer; the host mirrors it.** Christine:
  *"we can't just let host edit and send the completed file to emca. We must
  notify emca of every edit, so essentially emca and the host are maintaining
  mirror buffers."* Four independent reasons force it, only one of which is
  undo: **the suite runs headless** (against a virtual surface there is no host
  buffer, so every acme behaviour test would break), sam's structural commands
  operate on text, `Put` and the `| < >` filters need it, and the `body` file
  serves client programs. **The risk this creates, named**: emca writes too (sam,
  `Get`, `+Errors`, steering `sel`), so mirroring is not replication with one
  writer — it is two writers on one buffer, which is collaborative editing and
  OT/CRDT territory. **The existing canvas design already solves it** and the
  solution is lifted rather than reinvented: *"a user edit event is a mutation
  that happened — the device applies insert and delete to the node's data BEFORE
  queueing the event, so presenter echo and node data are one thing."* Single
  source of truth, optimistic local echo. **And divergence must be detectable**,
  because its failure mode is not a crash but silent disagreement — one dropped
  or misapplied edit and every later `s//` operates on text that is not there.
  Each edit carries a **sequence number**, each sync a **hash of the buffer**;
  mismatch triggers resync. Which amends a load-bearing discipline deliberately:
  con(1)'s *"apps never re-read"* becomes **"apps never re-read ROUTINELY"** — a
  resync on detected divergence is a repair path, not normal operation.
  **(2) One undo stack, and it lives in emca.** The host's undo is DISABLED;
  `⌘Z` round-trips. The realisation that makes this simple rather than a
  compromise: **emca is not remote.** It is a process on the same machine
  reached through a SAB mailbox, so a round trip is microseconds. *Every*
  argument that forces web editors into local optimistic undo stacks is a
  latency argument, and none of them apply — so the two-stack problem is one we
  would be inventing rather than one we are forced into. Acme's infinite undo is
  preserved exactly, with no interleaving semantics to explain and no "sometimes
  it undoes a keystroke, sometimes a command". The one exception, flagged for
  later: a *remote* surface under M12's distributed story would feel the trip,
  and only then is local undo worth its complexity.
  **(3) `/mnt/acme` retires, merged into `/dev/window`.** Christine: *"I think we
  may need to retire /mnt/acme."* It is a merge, not a loss — the two are the
  same files pointing opposite ways (a program driving emca; emca driving the
  host), and the only difference was that `/dev/window` declares toolbars and
  actions, which acme's interface arguably should have had. This answers
  window.md's open question in favour of **one window vocabulary**, and it
  carries an obligation: **`/dev/window` must be usable by ordinary programs**,
  not only by emca and the host, because that client interface is the paper's §7
  and the thing that made acme extensible without plugins. Designed for from the
  start, not retrofitted.
  **(4) Versioning is policy over `#V`, and nothing may depend on it.** The
  distinction that settles it: **undo is edit-granular and session-scoped**
  ("un-type that"); **versioning is tree-granular and durable** ("what did this
  look like on Tuesday?"). Neither substitutes — hourly snapshots cannot unpick a
  keystroke, and editor undo cannot recover last week. `#V` as built is already
  the right shape and already explicitly triggered (`#V/ctl` takes `snap [name]`;
  *"a snapshot is a tree; restore is a bind"*, architecture.md), so **the
  mechanism exists and only policy is missing — and policy is a file.** Plan 9
  made the same separation with fossil and venti: the live filesystem and the
  archival store are different concerns and the editor knows about neither.
  Christine's shape, adopted: **optional, off by default, user-configurable, and
  not realtime.** Two trigger axes rather than one — **on `Put`** for anything a
  human authors (a clock snapshot catches a half-finished edit; a version at
  `Put` catches a state somebody *meant*), and **hourly or daily** for things
  that change with no human intention to align to: logs, state, databases. Off
  for `/tmp`, scratch and build output. **Per-namespace**, which falls out of
  per-process namespaces for free and yields a product feature: **an agent's
  sandbox can be versioned aggressively as audit** — P4's *"what could it
  touch?"* becomes *"what did it change?"*, with `restore is a bind` as the undo
  button. **And the rule that keeps the layers independent: because versioning is
  optional, nothing may be built on it.** Undo cannot be "walk to the previous
  version", since a user with versioning off would then have none. `#V` is a
  safety net underneath, never a mechanism anything requires — which is what
  protects (2).

- **(2026-08-31) `/dev/window` belongs to every program, and both halves watch it.**
  Christine: *"any program can write to /dev/window, and in fact it is how emca
  operates"* — emca holds no privilege, only a job. The shape that gives every
  tool: **one binary, and a flag is the only difference between a command-line
  utility and a system manager** — `pkg` lists to stdout, `pkg --emca` mints a
  window and lists them there. **The question this raised** — *"how does the host
  know there is a new window? … either way, we need a `/dev/emca` channel"* — had
  a false premise, and the answer is **no new channel**. The device already mints
  (`#w/clone` does it today), so nothing needs to announce; the real difficulty is
  narrower and was already named in the plan at M13: **9P has no change
  notification**, *"poll, or a synthetic event file"*. The house answer is the
  second, and canvas already uses it. So `/dev/window` grows a **root `clone` and
  a root `events` whose reads park** — `/net/tcp/clone` and `/dev/draw/new`'s
  shape — with two levels of event file: the root's for window lifecycle, each
  window's for what the user did inside it. **Both the host and emca read the
  root's**, which dissolves the choice that was put as either/or: the **device**
  mints (mechanism), **emca watches and places** the window by writing its `ctl`
  (policy), the **host watches and renders** where emca placed it. emca is a
  **watcher, not a gatekeeper** — programs mint directly so emca has no privilege,
  and emca is not bypassed so it keeps the workspace it owns. Policy in a
  userspace program watching a device is the Plan 9 move, the same shape as
  `/rc/tile` being a window manager in a dozen lines of rc. **Both offered
  alternatives are recorded as rejected**: *programs write, only the host watches*
  loses emca's workspace (a window it never learns of is outside the session it
  owns); *programs ask emca, emca tells the host* makes emca a required
  intermediary and contradicts the premise — it is what acme did through
  `/mnt/acme/new`, and worked only because acme **was** the file server, where here
  the device is. **The property that decided it**: with emca not running the window
  still exists and still renders, in its type's default pane, because the type is
  in the path — so `pkg --emca` works with no shell at all. Neither alternative had
  that. **One race, named**: mint → host renders → emca places, so a window sits
  briefly in its type's default pane rather than its considered one; benign by
  construction, since type already determines default placement, so it never
  appears somewhere *wrong* — only somewhere provisional, moving at most once.

- **(2026-08-31) Building emca amended two things the design had settled, and
  both amendments came from the code refusing to be written the stated way.**
  Recorded because each is a genuine correction, not a detail.
  **(1) `content` is an EVENT, not a sample.** The window contract had emca and
  the surface both *reading* a window's `content` file. But a window is minted
  before its file is known — `clone` first, `content` after — so a watcher that
  reads it once at mint races whoever fills it in, and emca is a watcher by the
  decision immediately above. The device now announces the write on the root
  `events` file. What the fix revealed: the race was already there for the
  surface, hidden only because the surface is push-driven; and the same line
  turns out to be how a surface reopens an existing window on a different file,
  which the design had no mechanism for.
  **(2) `put` notifies; it does not command — ONE WRITER PER FILE.** The buffer
  contract above is right that emca holds the authoritative buffer, and the four
  reasons for it stand. But it does not follow that emca should perform the
  *write*: with a real editor component behind the window, the surface holds
  byte-exact text where emca's copy is RECONSTRUCTED from the change stream. The
  first implementation had the surface write the file and then send `exec Put`,
  so emca wrote again — meaning any reconstruction error would silently overwrite
  correct bytes, which is precisely the failure the sequence-and-hash exists to
  catch, arriving too late to help. Inverted: the surface writes, then notifies,
  and emca re-reads what landed. `exec Put` remains the road for a window with no
  editor behind it, where emca's buffer is the only copy. The divergence hash
  becomes a pure diagnostic — it can no longer corrupt a file — which is what
  makes it cheap enough to always send. **And a measurement fell out**: the hash
  was over UTF-16 code units while the offsets beside it were byte offsets, so it
  would have false-positived on exactly the multi-byte content it exists to
  protect. Both now run over the bytes.
  **A third, smaller**: a window type may not redeclare a core verb, compared by
  LABEL rather than by whole line. Six of the eleven shipped type files declared
  one — mostly `Look` and `Get`, harmlessly, but `text` declared `Put`, which made
  Put appear in a clean window and so made the dirty indicator lie. The rule is
  now enforced in emca and asserted in the suite over `/type/*/window`.

- **(2026-08-31) Building the web surface answered the gating question and
  corrected the responsive rules' own reading.** M14d's stated precondition was
  *can `execute` and `look` ride Monaco's action and context-menu API?* — to be
  proven first, not assumed. Answered, and with a different component:
  **CodeMirror 6, which has no menu of its own and uses the platform's.** That
  is *stronger* than the Monaco route, not a compromise: the verbs ride the
  surface the user already right-clicks, rather than a component-specific menu a
  different surface would have to reimplement. Property 1 survives a rich editor.
  **The floating bar moved from right-click to the selection**, which is what
  *operand determines surface* actually requires — position encodes what a verb
  acts on. And splitting `look` into Open/Jump/Search **re-divided the labour**:
  Open, Execute, Pin and Edit are IPNX's; Jump and Search are the surface's,
  because they move a caret inside a buffer it already holds; Cut/Copy/Paste stay
  the platform's. Two ambiguities had to be settled to make the bar honest, and
  both resolve to acme's own order rather than to a guess: **a path that exists
  beats an address**, and **a regexp address is delimited at both ends** —
  without that second rule every absolute path reads as `/regexp/` and
  `/etc/motd` offers Jump, which is precisely the silent misjudgement the bar
  exists to expose.
  **The correction worth recording** is in the responsive rules. The table's
  `panes: none` at small was implemented as *hide them*, and the result deleted
  the home listing and the console outright — the exact information loss
  "nothing disappears as the viewport grows" was written to forbid, arriving from
  the other direction. **Collapsed is not hidden**: small INLINES both panes as
  concertina rows showing their tags. Which exposed the structural bug beneath
  it — only windows IPNX had declared chrome for owned a tag row, so the console
  had nothing to collapse *to* and vanished at zero height. Every window now
  carries one and IPNX's chrome **enriches** it. That is what *"a concertina row,
  a rail entry, a tab and a minimised window are the same object"* costs in code,
  and the design asserted it without anything enforcing it.
  **Two channels the design had not named.** `verbs` per window is the sanctioned
  third protocol addition (verb applicability) in the only form the narrowed
  architecture allows — a file, the parallel of `toolbar` one operand narrower;
  its stated form, "an attr in response to a select event", belonged to the
  canvas protocol that has since narrowed. `/dev/window/pin` is a genuine **gap
  the build found**: the pin is IPNX's by the design's own test and `execute` is
  emca's, so emca must hold it — but nothing carried workspace-scope state to a
  surface, and a status line cannot show what it cannot read. And naming a
  running command needed **`/proc/<pid>/args`**, proc(3)'s own file, whose data
  sat in the proc record all along with nothing able to read it.

- **(2026-09-01) THE COMPOSITOR: one object, composited recursively — and
  the redesign that should have come first.** Christine, stopping a build that
  was going wrong: *"We need a compositor - something that arranges windows into
  columns and rows, and it is recursive. each window itself is a compositor that
  can further decompose into windows... The entire browser surface (or macos/ios
  screen) starts off as one giant window, of type root."* And the diagnosis:
  *"The first thing we should have designed was the compositor and the root
  window. If we get this right then we have correct behaviour, window controls,
  resizing, tabs, panes, etc."*
  **The evidence, from acme's own source.** An earlier reading here called acme
  "three fixed levels, not recursive" and treated Row and Column as containers of
  a different kind from Window. `dat.h` refutes it — all three are a rectangle, a
  tag, and either children or a body — and `cols.c`'s `colcloseall()` closes a
  column exactly as a window closes, `textclose()` on its tag then `winclose()`
  on everything inside. **A column IS a window**, one whose content is windows.
  Acme stops at depth three, but nothing in the object requires the stop.
  **What this makes wrong, and it is most of a day's work.** The implementation
  had a `PANES` map holding `left`, `main`, `bottom`, a `pane <name>` verb in
  `wctl`, and a `pane` file per type naming one of those strings. Once placement
  is a NAME, composition is frozen at three slots, nothing nests, and rows and
  columns are decoration. Every symptom traced to it: windows that could not
  split, a "rail" that was really region #1, a bottom pane that existed because a
  string was declared. Her own diagnosis of the naming — *"Where you have been
  confused (judging by you naming panes as Rail, Transcript etc) is that panes
  are not special, they are just normal windows"* — is the same fault seen from
  the vocabulary end.
  **ALLOCATION, which arrived last and simplified the rest.** The first pass at
  the controls had maximise MINIMISE every sibling — and Christine caught that it
  does not work: *"a window with minimised rows still take up space (one line per
  row). so maximising a window may not actually give much extra room."* True, and
  the fix she pointed at — *"if we are adopting a stack metaphor, then minimise
  should be minimising into a stack"* — collapses the whole layout model into one
  sentence: **a parent allocates rectangles to some of its children along an
  axis, and those it does not allocate to appear as tabs.** minimise(me) moves me
  out of the allocation; maximise(me) moves everyone else out; each is undone by
  moving back. One mechanism, two arguments, O(1) space either way. What falls
  out of it, none of which had to be written: a tabbed window IS a maximised one;
  "tabs display only when there is more than one" is not a rule but an empty
  strip having nothing to draw; stack stops being a third composition beside row
  and column; a minimised container takes its contents with it because they are
  inside it; and the small breakpoint stops being a "concertina" — it is the
  ordinary allocation rule with room for one rectangle, driven by the same
  mechanism a person drives with the minimise button, which is why the compositor
  can no longer delete a window by accident. It also retired, unbuilt, an
  elaborate design for a rotated minimised strip: a tab is a WHOLE window the
  parent has not given a rectangle to, so the questions that design agonised over
  — a title too cramped to edit, one button standing for three, a close control
  made unreachable — simply do not arise.
  **The principles, and the sentence that governs them**: *"The design is not a
  set of exceptions, it's a few principles applied consistently."* One object.
  Every window composites itself into a row, a column or a stack of tabs.
  Controls INFORM THE PARENT — which is what makes recursion work, and which
  retires the earlier refusal of maximise (that objection reasoned about a child
  in isolation; maximise is an instruction to the parent, and the parent already
  remembers an arrangement). The tag line is an OPERAND, not a command line: its
  text is the argument to whichever toolbar verb is pressed, which is the 2-1
  chord decomposed for keyboard and finger — **and which retires both the
  floating bar and the pin**, two mechanisms built for one job. Every window has
  the same four components and its own status bar. Panes are windows the root
  created by convention, and nothing downstream can tell them from any other.
  **All window verbs are always visible.** The implementation hid Newcol and
  friends behind an overflow on the judgement that they were rare. Hers: *"They
  are not, all the window controls need to be available at all times, they are
  part of the UI."* A narrow toolbar WRAPS — geometry answering geometry, never a
  ranking of importance.
  **The unit is device-independent pixels, plus a reported text cell.** Characters
  remain the leaf measure (72 x cellWidth) so accessibility sizing still moves the
  breakpoints, but they cannot be the unit: *"not all windows display text. emca
  needs to be able to know how to fit an image into a window... it knows what an
  image is (or video, or postscript etc) and what aspect ratio is."* Acme could
  measure in characters because everything was text. Her precedent: *"this is why
  macos uses postscript underlying"* — Display PostScript and Quartz are
  device-independent imaging models for exactly this reason.
  **And emca owns the tree.** *"Host tells emca - we have a 800x1024 window. emca
  says 'Ok, we need to apply this responsive layout' create these windows... emca
  owns the tree, and the host renders the tree."* With the reason: *"emca needs to
  understand the geometry as well, so it does not ask the host impossible
  things."* This amends PART ONE's *"emca never learns the viewport's width"* —
  it does now, and must. A gain falls out: the responsive rules become testable
  headlessly, since rc can write a geometry and read back the tree.

- **(2026-09-01) THE VERBS: the era's names, four surfaces with one operand
  each, and the floating bar restored.** Christine: *"acme names for the
  builtins are idiosyncratic (snarf, zerox, put, get, etc.). They have not stood
  the test of time, and are against Apple HIG… This only applies to emca, not
  acme. Acme of course retains it's naming."* So emca says Copy, Save, Revert,
  Open; acme's port keeps Snarf, Put, Get and Zerox, because renaming acme's
  buttons would be changing acme rather than porting it.
  **The grouping is by operand, and it resolves an overlap the first draft had.**
  Hers: some operations are window operations (newcol, newrow), some operate on
  the body (cut, copy, paste), some on the tag line itself (run, search, add).
  Four surfaces, each with exactly one operand: the window as a thing in a layout
  → the **title bar row**, beside the controls, since these are not about what
  the window holds; the window's **content** → the toolbar; the **tag line's
  text** → its own buttons, kept separate so it is visible that Run acts on what
  you just typed and Save does not; a **selection in the body** → the floating
  bar.
  **THE FLOATING BAR WAS RETIRED IN ERROR AND IS RESTORED.** The reasoning for
  dropping it — that the tag line plus the toolbar already did the job — missed
  that they take different operands: the tag line holds text you COMPOSE, the bar
  acts on text you POINT AT, and acme's chord was the second. Hers, restoring it:
  *"The floating toolbar is still the floating toolbar, so it is context
  sensitive to the window body."* That the same three words appear on two
  surfaces is not duplication but the rule working.
  **Two corrections fell out of the audit.** The toolbar is NOT a closed set: her
  `Add` verb puts the tag line's text on it as a button, which is how acme's
  "type Indent in the tag and it works" becomes durable — so the CORE is closed
  and the toolbar is extensible. And **New column / New row DUPLICATE** this
  window rather than creating an empty one (*"new col (in reality duplicate
  horizontally)"*), which subsumes Zerox entirely: you duplicate, then retitle,
  because the title retargets. One verb where acme had three.
  **All 38 of acme.txt's operations are accounted for in the spec**, and three
  acme builtins disappear as buttons because something else already does the
  work: Zerox (New column duplicates), Edit (sam's language is a command, so you
  type it and press Run), and ID (it is state, so it lives in the status bar).

- **(2026-09-01) acme and emca are two documents about two things.** Hers:
  *"acme is Bell Labs program. We are going to update it to fit emca, but not
  change functionality. emca is effectively our new windowing system and UI.
  Don't confuse between the two."* So `docs/acme.txt` stops being "the anatomy
  emca derives from" and becomes **the port spec**: how acme is modified to fit
  into emca, functionality preserved. `docs/emca.txt` is the windowing system —
  what a window is, how the compositor works, window types. The anatomy was
  input, not parentage, and describing emca as "derived from acme" invited
  exactly the confusion that had me editing acme's own record to justify emca's
  design.
  **The port decision, taken**: acme's Row/Column/Window **become emca windows**.
  Acme today is its own compositor — it tiles internally and paints through
  libdraw on `/dev/draw`. Under emca it stops window-managing and delegates
  composition, keeping its verbs, its executable text and its `/mnt/acme` file
  server intact. The alternative — acme as one opaque window that draws itself —
  is less work but makes it a guest rather than a citizen, and none of emca's
  furniture would reach its columns.

- **(2026-08-31) The open questions, resolved in one pass — and what it revealed
  about them.** Eleven items stood open across [emca.txt](emca.txt) and
  [window.md](window.md); worked through together on Christine's instruction,
  they resolved almost entirely by reasoning from decisions already taken, which
  is itself the finding: **most of them were consequences waiting to be noticed
  rather than questions waiting to be answered.** **Two were stale** — *who owns
  undo* and *buffer fidelity* had been settled by the buffer-contract decision
  and never struck out; a list that keeps answered items is a list that stops
  being read. **One dissolved**: *how a type declares a view mode without
  becoming a widget toolkit* has no answer because a type declares **nothing
  about rendering** — the host opens the file and renders it natively, so there
  is no vocabulary to grow into and the dozen-kinds tripwire has nothing to trip.
  The canvas redesign removed the risk rather than bounding it, which is worth
  noting: the right architecture deletes questions instead of answering them.
  **One resolved by adoption rather than design**: the plumber takes Plan 9's
  **plumb(6) rules syntax and message format verbatim** — *adopt the notation,
  own the model*, the same move that took SVG path data, and the standing
  instruction to maximise reuse of existing protocols applied where it was
  cheapest. **One resolved by reframing**: *property 1 under Monaco* is not a
  risk about Monaco but a **selection criterion for any editor component** —
  expose the selection, accept custom context-menu commands, or be disqualified.
  Monaco, CodeMirror and TextKit all qualify. The spike still runs to verify the
  chosen one; it no longer gates the design. **And one produced genuinely new
  mechanism**: *the surface's own suite*, flagged as the largest untested area
  with no precedent to borrow. Resolved by having **the surface publish what it
  rendered** — a `ui` file per window listing each control's label, role,
  keyboard path and enabled state, **derived from the platform accessibility
  tree** on real surfaces and synthesised on the virtual one. The a11y tree is
  precisely the "can this be reached" answer, on the web and on Apple alike, so
  one assertion — *every floating-bar verb and every toolbar button is present,
  named and keyboard-reachable* — runs on every surface including headless. That
  converts a claim into a measurement: the input convention has held since
  2026-08-30 that accessibility is *"enforced by construction"*, and nothing
  checked it until now. **The remainder, resolved plainly**: snapshot-vs-replay
  is snapshot and it is *free*, because a workspace's writable layer already is a
  tree and keeping it is the snapshot (replay stays available as recorded
  provenance); an unnamed instance gets its layer at birth under
  `/usr/$user/.inst/<id>` so saving is a **rename, not a copy**, and there is no
  window in which work sits somewhere it could be lost; cross-window at small is
  a **stated degradation** — operation works via the pin at every size, viewing
  two bodies on one small screen is impossible for any design and is named rather
  than solved; the floating bar's **set** is closed and derived from acme's layer
  3 while its **order and grouping** stay empirical; and surface dependency
  divergence was never open, being the input convention working as intended.
  **Also specified in the same pass** ([window.md](window.md)): the event
  vocabulary, reusing canvas's verbs rather than inventing a second set, with a
  root `events` for lifecycle and a per-window one for user action; and the
  **transcript**, which is not a special mechanism at all — its content is a
  growing file the host tails, typed lines return as ordinary `insert`, and
  con(1)'s mark arithmetic is **deleted** because the input region is just the
  host's editable tail. **What actually remains** is small and honest: the
  floating bar's arrangement, the editor component's verification spike, and the
  fact that **none of it is built** — every decision here is design, M14 is
  unstarted, and the suite's 151 pass because the code has not moved.

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

- **(2026-08-29) su without a superuser — the security landing.** The
  README's "there is no superuser" is structural, at three boundaries: the
  sandbox above (nothing to escalate to), device policy within (the eve
  bypass belongs to each device, not the core), and the mount table outward
  (eve-ness does not serialize; each server applies its own policy to the
  attach-time `uname` — the superuser's reach ends where root's always did
  in practice, at the NFS boundary). `su` is identity transition under
  docs/identity.md's two rules, never escalation: a personality-layer command
  over `/proc/self/ctl`, no password, no setuid machinery; `su none` — the
  privilege-drop shell — is the celebrated direction and the per-agent
  primitive; kill comes free through note permissions (V10's euid rule).
  Recorded impossibility, a feature: namespace-wide write is the union of
  per-server grants fixed at attach, so it is not a grantable thing — su
  re-evaluates local authority instantly and can only request more by
  re-attaching (auth/as's shape). **The namespace unions services; it
  cannot union their trust.** Earmarked: Plan 9's devcap (`#¤`,
  caphash/capuse) as the authenticated third rule ("user X, bearing
  proof"), sequenced with /net and factotum-shaped work; uids stay
  per-server names with the personality owning name↔number. Landed with
  the entry: `cmd/su.c`, `#p` in the boot namespace, and the suite's 131st
  test (`su none id` → `none none`) green on all three hosts.

- **(2026-08-29) What a "user" is — the conflation decomposed.** Unix's uid
  merged four things: a person at a terminal (originally a *billing*
  construct), a protection domain, a service principal (`lp`, `uucp` — the
  daemon users, never logged into, the part that aged best), and root, the
  anti-user. IPNX keeps the kernel's mechanism minimal (a name pair per
  process, ownership, the transition rules, `DMSETUID`, the per-attach
  `uname` — all already running) and resolves the CONCEPT four ways:
  **(1) the person is eve, exactly one per kernel instance, many instances
  per person** — timesharing is inverted, not restored: kernels cost a
  browser tab, so multi-tenancy happens by instance and *the kernel
  instance is the new uid*; "fast user switching" is a different instance;
  in-kernel multi-user survives only where Plan 9 put it, at servers, per
  attach. **(2) The role is the daemon user, kept deliberately and
  ennobled**: a name that owns resources and is conferred, never logged
  into — assumed by DMSETUID exec, eve's grant, or a future devcap ticket
  — and where Unix daemons got only a uid, IPNX daemons get a reduced
  namespace (systemd's forty sandboxing directives are a namespace system
  described one flag at a time; here confinement is the binds a daemon
  started with). **(3) The agent is a role plus a namespace** — the
  genuinely new population, already the largest identity population in
  the industry: the name is for the audit trail, the namespace is the
  authority; `none` is the anonymous agent and `su none` its front door.
  **(4) The network person is an authenticated claim, per connection** —
  no global registry (NIS/LDAP sediment, refused); servers believe
  proofs, today the attach-time uname, later factotum-shaped tickets
  with /net; a person across their instances is their keyring. The
  organising sentence: **names are for accounting; namespaces are for
  authority.** Consequences at zero mechanism cost: `login` never exists
  (no getty — a person does not log into their own instance),
  `/etc/passwd` stays personality-side (V10 numbers, plus the curated
  V10 role names as heritage), authentication stays sequenced with /net,
  and groups (deferral D2) get less urgent because roles absorb most of
  what groups did.

- **(2026-08-29) The profile: identity's configuration, unified.** Plan 9
  built the user profile in four scattered pieces nobody named as one
  thing: factotum (the key agent — secrets in memory only, protocol steps
  performed on the owner's behalf, mounted as a file server), secstore
  (the durable *networked* keyring, one strong unlock at boot — agent in
  RAM, store on the network: a distributed password manager in 1999),
  `$home/lib/profile` (the per-user namespace script), and
  `/lib/namespace` (the declarative bind/mount language `newns`
  interprets). The industry rebuilt each piece separately — password
  managers and platform keychains are secstore, passkeys are factotum's
  sign-the-challenge model winning late, dotfiles and kubeconfig contexts
  are lib/profile — and unified nothing. IPNX unifies them as **the
  profile: a file tree served by a userspace file server** (the kernel's
  fifth consecutive identity decision costing it zero lines), mounted at
  `/mnt/profile`:
  `namespace/` holds fragment files in the /lib/namespace little language
  — `base` + `device/<name>` + `service/<svc>`, unioned by context
  (ssh_config's Host blocks, kubeconfig's contexts, done as files);
  `services/` is the fstab-of-the-person (dial address, protocol, aname,
  mountpoint, credential *reference*); `keys/` is the agent interface and
  **secrets are never ordinary readable files** — programs write a
  challenge and read a response, use-don't-read, which also lets each
  platform shim back the store with its native secure enclave while the
  profile stays portable text. "Rights" is deliberately NOT a subtree:
  by the user decision, rights are namespaces plus roles, so the profile
  records how to *exercise* them (which fragments, which role tickets)
  and grants nothing — servers still decide. Three portability rules:
  **the profile speaks IPNX names, never host paths** (each shim maps
  "the home directory" to its device — the profile never learns a host
  path, as the kernel never learns an on-disk format); **construction is
  best-effort** (an unreachable NAS mounts nothing; a laptop on a plane
  is not a broken profile); **the store is remote and durable, the agent
  local and volatile** — the durable profile is text plus encrypted
  blobs and lives in anything mountable, including a git repo (versioned,
  diffable, rollback-able identity for free), unlocked once per device
  (PAK then, passkey-shaped now). Delegation falls out: an AI agent's
  identity (role + namespace) is exactly *a sub-profile* — fewer
  fragments, scoped tickets, its own audit name. Refused: kernel
  mechanism, a global identity provider (the profile presents proofs;
  servers verify per connection), plaintext secrets, and package
  management (this is not Nix). Factotum is adopted as a SHAPE, not a
  program — its protocols are period pieces, its design (agent as file
  server, use-don't-read, delegation) is the ancestor. **Sequencing in
  two stages, neither throwaway**: the namespace half needs nothing and
  can land now (boot already wants "rc plus a namespace file"; per-agent
  sub-profiles included); the credential half needs /net to reach stores
  and services, and pairs with devcap.

- **(2026-08-29) The PoC is declared complete; the implementation begins.**
  Nothing was left on the PoC's own list: 131 acceptance tests pass identically
  on Node, in Chrome, and on the Rust kernel core under wasmtime; both editors
  run; both foreign runtimes run; the identity architecture is recorded. Three
  consequences, structural: **(1) `poc/` freezes** — the JS supervisor is the
  reference implementation and conformance oracle, valuable precisely because
  it still runs; it changes no more (the guest world inside it is not frozen
  and graduates out as implementation milestone M0). **(2) The documents
  split** — [poc.md](poc.md) is the PoC's frozen record;
  [implementation.md](implementation.md) is the living build plan (milestones
  M0–M12: the tree, the scratch container, the namespace-file boot, the macOS
  app, host storage, the browser host on the Rust core, iPadOS on Pulley,
  `/net`, identity on the wire, the profile, the modern personality and git,
  the microVM, the world); this document remains design and decisions only.
  **(3) The tree rearranges around the real implementation** — `native/kernel`
  becomes `kernel/` (the one kernel), `native/host` becomes `hosts/macos/`
  (first of four hosts), and the Cargo workspace moves to the repository root.
  The conformance suite is the contract across all of it: the 131 are the
  permanent floor, and new features add self-skipping tests so one rootfs
  serves every host including the frozen oracle.

- **(2026-08-29) Capability doctrine, learned from the graveyard.** The
  fifty-year history of capability operating systems (RESEARCH §12:
  Plessey 250, CAL-TSS, HYDRA, System/38, the iAPX 432, KeyKOS, Amoeba,
  EROS, and the disguised survivors — Mach ports, seL4, Capsicum, signed
  URLs) yields five causes of death and this project's five answers,
  recorded as doctrine: **(1) the compatibility cliff killed more
  capability systems than everything else combined** — the WASI ABI and
  the benchmark discipline are the anti-Amoeba posture and are never
  compromised; **(2) capabilities stay invisible** — the capability IS the
  namespace and LOOKS like a filesystem, the handle is an fd, the grant is
  a bind, and no "capability" noun ever reaches a user (the System/38
  lesson: caps succeeded while invisible, and the AS/400 kept them only
  beneath the surface); **(3) devcap adopts Amoeba's mechanism** — the
  sparse, cryptographically checked, self-attenuating ticket (Amoeba's
  128-bit port/object/rights/check design, one-way-function protected, the
  ancestor of the signed URL) with modern MACs, over kernel capability
  tables; **(4) revocation is answered by expiry, re-attach, and unmount,
  never a revocation registry** — the AS/400 retreat proved held bits
  cannot be recalled, and modern practice (short-lived tokens) beat
  revocation lists; audits stay possible because authority is legible in
  the mount table, not scattered in held bits; **(5) the deployment story
  is reviewed periodically with the same honesty as the code** — Amoeba
  and Plan 9 died of their deployment wave (processor pools, CPU servers),
  not their kernels; ours (tab, laptop, container, agent sandbox) is
  today's wave, and the lesson is not "we are safe" but "re-examine."

- **(2026-08-29) The first formal design-thinking iteration — scope
  re-derived from personas, and the standing decisions survived it.** The
  method (d.school five modes through the Double Diamond, adapted and cited)
  and the full record are [design-thinking.md](design-thinking.md); the
  empathy artifacts are [personas.md](personas.md). Its epistemology, made
  standing: **P1 (the author) is live user research; P2–P5 are assumption
  personas whose insights stay hypotheses until their named validation
  events fire** — and design thinking is *evidence, not authority*: a
  persona-derived conflict with a dated decision arrives as a proposal with
  evidence attached, never a silent change. This iteration produced zero
  such conflicts — every standing refusal was independently re-derived from
  the personas — and five deltas, adopted: **the tour** (an rc script in the
  demo rootfs; chapters double as the educator's seed exercises), **the M9
  sandbox quickstart as a shipped artifact** (P4's belief test made
  deliverable), **M3+M4 as a pair** (P1's journey: a screen without
  persistence is a demo, not a home), and two explicit won'ts joining the
  refusals: **no phone form factor** (no persona's journey contains one) and
  **no Windows host** (until a persona demands it with evidence); courseware
  beyond the tour's chapters is likewise declined. Cadence: an iteration
  reruns with each deployment-ledger review and after any validation event.

- **(2026-08-31) The demo runs the Rust core — and the kernel loses its
  last OS dependency.** Her directive ("now continue with replacing the
  demo with the browser surface") executed the 2026-08-27 decision's
  second half: the one kernel crate now compiles native for macOS and
  to wasm for the page, and the page is the product. The load-bearing
  architecture note is what the landing forced: '#Z' hostfs was the
  kernel's only std::fs, and the browser cannot block on a filesystem —
  so every host-file operation became a delegated effect
  (Effect::Host{tag,op} out, hostop_done back), the exact webfs/fetch
  pattern already in the kernel. Consequences, all measured: the
  kernel is now literally free of OS calls ("no OS dependencies" was
  the design line; now it is a fact of the build); the macOS host
  gained the op server the kernel lost and stayed 151; the browser
  serves OPFS and picked directories over the same op protocol; a
  Node harness drives the identical wasm kernel headless (151, the
  fast iteration loop the browser lacks); and the guest world runs
  UNCHANGED — same worker.mjs, same guestcore, same mailboxes — the
  proof that the mach-layer seam sits exactly where the 2026-08-27
  decision drew it. Chrome runs the suite at 151 on the Rust core;
  cc-to-Hello-Kitty and reload-surviving browser storage verified by
  hand. The demo's JS kernel lineage retires to reference-in-tree;
  the frozen oracle remains the oracle.

- **(2026-08-31) Parity is measured against the running reference.**
  Christine: "I am comparing the acme from the demo vs acme from
  plan9port, and there seem to be differences" — and she separated the
  problems: parity first; the touchscreen/HIG redesign is PARKED for a
  full design-thinking session (mouse chords and discoverability
  belong there; nothing of it ships in this pass — the brief is
  docs/acme.txt). Method, per the
  house rule that measurement beats memory: her plan9port acme was
  launched beside the demo, driven through its own /mnt/acme file
  interface (dirty, selection, tag states staged with 9p), and
  captured by CGWindowID — the screenshots are the spec. Adopted from
  the reference: the tag seeds (root `Newcol Kill Putall Dump Exit`,
  column `New Cut Paste Snarf Sort Zerox Delcol`, window
  `Del Snarf Get Look Edit |` — column Cut/Paste/Snarf/Zerox act on
  the last selection); Undo/Put appearing between the fixed words and
  the bar; the Medblue dirty box and white clean box on a Purpleblue
  column button (draw.h's own constants); black hairline separators
  and no gaps; the proportional Lucida face; acme's selection colours
  (#eeee9e bodies, #9eeeee tags); a static chunky caret; and the
  screen shape — columns fill the window, windows divide the column,
  bodies clip and scroll (the canvas spec grew column-prop for it).
  The pass also caught two real presenter bugs the comparison exposed:
  the window chrome's user-select:none had made native selection
  impossible inside every canvas view (sweeps, double-click,
  select-to-replace all dead — very likely the felt "difference"),
  and the caret repaint on mousedown destroyed the browser's selection
  anchor mid-gesture; both fixed (select on mouseup, selection owned
  by the view). Still divergent, stamped: the scrollbar sits right
  (its colours are acme's), no drag-layout between columns yet, look
  warps focus not the pointer, Look sits before the bar (the
  tag-scratch owns Edit's arguments), and tag wrap is the surface's.

- **(2026-08-30) The paper is the yardstick: acme answers its own
  literature.** Christine, after the succession: "I still feel like
  acme does not behave like real acme. Have you read the acme paper? Do
  you know if all the examples in the paper will work?" The honest
  answer was no — acme-today had the paper's shape and not its engine —
  so the paper (Pike, *Acme: A User Interface for Programmers*) was
  inventoried example by example and the gaps closed in one pass:
  **external commands** run in the window's directory (`mk` in a tag,
  the §19 workflow) with output to a `dir/+Errors` window created
  towards the right; the **selection filters `| < >`** (4th-edition
  exec.c:118 provenance) pipe dot through real commands; **Undo/Redo**
  unwind by sequence number, typing coalesced, exactly the paper's
  two-list algorithm; **Put appears in the tag only while dirty** (and
  Undo/Redo only when they have work), maintained as a tracked auto
  block that user tag edits shift; **B3 takes sam addresses** —
  `dat.h:27`, `:/re/`, `#c`, compounds — opening at the line, reusing
  an existing window, `<header>` resolving through /sys/include; **Cut,
  Snarf, Paste, Look, Sort, ID, Kill, Delete are words** (Cut/Paste
  compose with the host clipboard through /dev/snarf itself, so the
  stamped divergence and the paper's builtins reconcile in one buffer);
  and **the editor serves its file interface** — `index`, `new`,
  `N/{tag,body,addr,data,ctl,event}` — as 9P posted at `/srv/acme`,
  the paper's §7: `grep -n var *.c > /mnt/acme/new/body` works, `addr`
  speaks the same address language, `data` replaces the addressed
  range, and the `event` file delegates execute/look to a client in
  the paper's own format (`MI15 19 0 4 time`), writeback applying the
  default interpretation. The canvas protocol grew exactly two
  elements for all this — the `select` event and the `sel` attr, both
  device-transparent (neither kernel changed). What remains divergent
  is stamped in the matrix delivered with the pass: chords are the
  host clipboard's gestures (decided earlier), scrollbars and fonts
  are the surface's, Zerox copies rather than aliasing one buffer, the
  guide-file tools of §6 are superseded by the built-in Edit (as later
  acme did itself), and the Mac presenter still renders v0
  (deferral already on record). The suite grew one composite test
  driving every behaviour headlessly through the virtual surface and a
  real mount of /srv/acme — green on the Rust host and the demo
  kernel, self-skipped on the frozen oracle.

- **(2026-08-30) The name on the door: IPNX, not "modified Plan 9".**
  Christine, in her words: "We shouldn't call it a modified Plan 9 kernel.
  You yourself stated it does not inherit any code from Plan 9. We should
  be promoting IPNX, not Plan 9. IPNX isn't Plan 9 and will never be" —
  and, minutes later: "I feel like we are underselling what has been
  achieved." Both corrections are of fact, not of taste. The kernel was
  never a modification: it is an original implementation of Plan 9's
  *architecture*, written twice over — the Rust core and the JS
  supervisor, neither sharing a line with Plan 9's C — and proven one
  system by one conformance suite. Her README said it first ("The kernel
  is fresh… IPNX is not Plan 9, it is not UNIX"); this decision brings
  the working documents and the public page into line. The statement now
  opens with the IPNX kernel; Plan 9 is credited for the architecture
  and for the vendored heritage userland (which really is its code, and
  says so, notices intact); and the demo page leads with the achievement
  — a complete operating system, compilers and editors included, running
  client-side in a browser tab — rather than with the ancestor.

- **(2026-08-30) The input convention: roles in the tree, native grammar
  per platform, verbs never in the hardware.** The three-button replacement,
  articulated (the canvas decision's input addendum). The canvas declares
  SEMANTICS — spans with `action=look target=…`, `action=execute`, nodes
  with `menu=…` — and each presenter activates them with its platform's
  own grammar; apps receive verb events only (`look`, `execute`, `menu`,
  and edit verbs), never gestures (one scoped exception: `frame`/`path`
  leaves may opt into a raw pointer stream, Pointer-Events-shaped, for
  games). **Web bindings — web best practice literally**: look-spans are
  real `role=link`, execute-spans real `role=button`, so click/Tab/Enter/
  focus-rings/screen-readers arrive free because the things ARE native;
  context menus from `contextmenu`; selection verbs on the selection;
  ⌘-click a look target = look in a new window (the browsers' own grammar,
  inherited). **Apple bindings — HIG literally**: SwiftUI `Button`/`Link`/
  `.contextMenu` (touch-and-hold with preview on iPad; right-click/⌃-click
  on Mac); selection verbs in the NATIVE edit menu beside Cut/Copy/Paste;
  `.keyboardShortcut` ⌘-conventions; and HIG's own rule honoured — no
  action lives only in a context menu, so the Mac presenter surfaces menus
  in the real menu bar. **Hardware mouse, modern not nostalgic**: left
  activates, right menus, middle looks-in-new-window; the Plan 9 b2/b3
  semantics survive only in the raster exhibit's input synthesis —
  "buttons as accelerators" is REFUSED as convention pollution.
  **Keyboard**: Tab traverses, Enter/Space activate, ⌘Enter executes the
  selection or word at caret (acme-today's tempo), hold-⌘ shows the iPad
  shortcuts HUD. **The laws, lifted not composed**: keyboard-complete
  (WCAG 2.1.1); hover decorates, never gates; activation on release with
  slide-off cancel (2.5.2); no gesture-only functions (2.5.1); target
  sizes max(44pt HIG, 24px WCAG); destructive actions confirm. And the
  structural claim: accessibility is enforced BY CONSTRUCTION — apps never
  render controls, presenters always render them natively, so an
  inaccessible button cannot ship. Refused: custom gestures in v0; the
  nostalgic button mapping. The verbs in one line: b1 was always
  universal; look = tapping the thing; execute = tapping the tag or
  ⌘Enter; the chords were always the clipboard.

- **(2026-08-30) The succession rule: a name is inherited by passing
  the ancestor's tests.** Christine's directive — "get sam-today
  working, then we can retire the heritage items" — executed as a
  rule worth keeping: sam-today took `/bin/sam` only by passing the
  three existing sam tests UNCHANGED (structural regexps, the `g//`
  guard on class ranges, and `,| tr` — the buffer piped through a
  real command, fork machinery and all); the raster original stepped
  back to `sam9`, still built, still driven by samtest (the whole
  sam/samterm/libframe stack stays proven), still holding its share
  of the floor. The demo menu's heritage wing retired the same hour —
  the exhibit remains runnable by name (`sam9`, `acme`), present in
  the tree and the suite, absent from the product face. Retirement
  means the successor answers to the ancestor's name and the
  ancestor's proof; it never means deletion. **The second clause,
  from acme's succession the same hour**: an ancestor's tests divide
  into BEHAVIOURAL (file in, commands, file out — sam's, which the
  heir passed verbatim) and SUBSTRATE (raster pixels, the draw
  protocol — acmetest's, which certify the stack, not the editor).
  A name passes on the behavioural tests; the substrate tests step
  back with the ancestor (`acme9`, still driven by acmetest, the
  sam/samterm/libframe and acme raster stacks staying proven). So
  `acme` now means acme-today, held to the workspace behaviour suite
  — columns, editable tags everywhere, word execution, Put-by-name,
  the look browser, the Edit language — and both originals answer to
  their stepped-back names.

- **(2026-08-30) No half-working.** Christine, on acme-today's deferral
  list, in her words: "I don't understand why we have deferrals. Either
  acme works like acme, or it doesn't. We can't have half working." The
  standard, recorded: slices may BUILD in sequence, but what ships to a
  user carries whole behaviour — a deferral list is a build-planning
  tool, never a licence for a shipped half-experience. Applied the same
  hour: acme-today gained caret editing everywhere (click positions,
  arrows navigate, selection replaces, paste inserts), sweep-execution
  with arguments, Look (in-window literal search presenter-side; paths
  open windows app-side), the dirty box that Put clears, Zerox, Dump
  and Load — leaving exactly one deliberate divergence, already
  stamped: the native clipboard IS snarf (the input convention's
  "chords die into the host clipboard"), so Cut/Snarf/Paste live in
  the platform's own gestures rather than as tag words.

- **(2026-08-30) The design stretch: a distributed operating system.**
  Christine's articulation, recorded in her words: "the design stretch
  for IPNX is a distributed operating system. There is no reason why the
  process orchestrator in IPNX can't orchestrate processes on other
  systems, so we can truly replace kubernetes if we wanted to. We need
  to be able to support process migration and rehosting, discovery of
  other systems and capabilities, and a user identity that can span
  systems." The framing that makes it honest: this is a **return, not a
  departure** — Plan 9 was a distributed operating system first (cpu
  servers, file servers, terminals, import and export), M0–M6 built the
  single-kernel half, and the stretch names what completes the circle.
  The three capabilities, mapped to mechanisms already in the tree:
  **Rehosting** is the orchestrator's normal move once specs exist — a
  process file is declarative, so "run it there instead" is kill-here,
  run-there with the namespace re-applied on arrival; wasm makes the
  binary host-neutral by construction (the same image runs under V8,
  JavaScriptCore and wasmtime today — rehosting across ARCHITECTURES is
  already routine in this repository). **Migration** — the live form —
  is uniquely plausible here because *every bare fork already serialises
  a whole process*: the asyncify unwind snapshots memory and stack, and
  a fresh Worker rewinds it; migration is that snapshot shipped to
  another kernel instead of a sibling Worker. The honest edge, named
  where classic migration died: open fds do not travel — wire mounts
  must re-dial and pipes must proxy or drain; the declarative namespace
  makes the FILE half re-bindable, and the fd half is the engineering.
  **Discovery** stays files, per the founding principle: Plan 9's cs(8)
  and ndb are the precedent — a neighbourhood is a mounted directory,
  a system's capabilities are read from its files (/svc, caps, /proc),
  and "what can you do" is `ls`. **Spanning identity** is M8+M9 seen
  whole: tickets authenticate the wire, the factotum-shaped agent holds
  the keys, and the profile is the portable person — one identity
  booting constrained instances on any kernel, "the kernel instance is
  the new uid" extended to a person who owns many. The kubernetes
  clause keeps its recorded honesty: the orchestration entry's
  consensus debt (a union of /svc trees is not a quorum) stands — the
  stretch does not waive it. Sequenced, not new milestones: M7 carries
  the wire, M8 the identity, M9 the person, M12's cluster stage the
  orchestration; the stretch names their sum and aims them.

- **(2026-08-30) The console amendment: AND, not XOR.** Christine, mid-
  build, in her words: "we should allow xterm.js as well - it is what
  people are used to... and not xor." The retirement clause softens on
  the record: console-today (the editable transcript, `con(1)`) is the
  NATIVE design and the doctrine's direction, and the xterm byte console
  stays beside it as the familiar door — the same reasoning that made
  VSCode a surface (meet people where they live) applied to our own
  terminal, and the exhibit philosophy applied to the present: nothing
  is deleted, surfaces multiply. Programs that write bytes get either
  console; programs that want structure get canvas.

- **(2026-08-30) The ultimate dev environment, and VSCode as a surface.**
  Christine's realisation, recorded in her words: "we have created the
  ultimate dev environment. A system can build and run a cluster - in a
  browser, without relying on VMs or containers... VSCode needs to be a
  surface." The first half is a shipped fact, not an aspiration: the
  live demo tab holds five toolchains, pkg, and — since the local stage
  landed — a supervised replica set; the cluster-in-a-tab runs today.
  The consideration, worked through: **VSCode is two surfaces, arriving
  at two times.** *The namespace surface, buildable now*: VSCode's
  FileSystemProvider API is nearly 9P — stat/readDirectory/readFile/
  writeFile/delete map to Tstat/dirread/Topen+Tread/Twrite/Tremove, and
  rename is `wstat` carrying a name, exactly the V10 shape we already
  landed. And the transport is *nothing*: the extension host is Node,
  and `demo/supervisor/kernel.mjs` is platform-neutral — the kernel
  boots inside the extension process, FileSystemProvider methods call
  straight into kernel walks (kernel-as-a-library, the JS twin's third
  host). Terminals are the Pseudoterminal API wired to `/dev/cons`;
  tasks run process files; the debugger's substrate is `/proc`. The
  doctrinal kicker: **devcontainer.json is a process file, badly** —
  Remote-Containers is a large extension plus Docker because the OS
  beneath had no per-process namespace; Remote-IPNX needs neither.
  *The canvas surface, after M5*: webview panels presenting canvas
  trees — acme-today can live in a VSCode tab. No conflict with the
  one-editor doctrine: VSCode is a surface a developer already
  inhabits, not our editor; IPNX mounts into their world, and surfaces
  multiply while the protocol stays one. Honest engineering questions,
  named: 9P has no change-notification (FileSystemProvider.watch needs
  polling or a synthetic event file — decide when building);
  vscode.dev runs extensions in a web worker (nested-worker and SAB
  constraints to measure, WebKit's especially). Sequenced as its own
  small milestone (M13), interleavable like the local stage was.

- **(2026-08-30) The iPad surface, re-aimed: an app that launches
  WebKit over local files.** Christine's call, in her words: "the ipad
  surface has changed. it is simply an app that launches webkit,
  connecting to local files. It circumvents JIT restrictions." The
  stopgap becomes the design. WKWebView's content process carries
  Apple's own JIT entitlement — third-party apps cannot JIT in-process
  but may host WKWebView, so the browser port runs at full JavaScriptCore
  speed inside the app, sanctioned. The shell shrinks to a few hundred
  lines of Swift: a WKURLSchemeHandler serving the bundled dist with
  **real COOP/COEP headers on every response** — no service worker, no
  register race, and plausibly no WebKit module-worker serialisation
  defect, since that measured failure was specific to loads through a
  service worker (a measurement to retake in-app); local files mean
  first boot is offline, the 260MB stream gone; and the app bridges
  real files inward — a security-scoped bookmark IS a bind
  (platforms.md), served to the kernel's hostfs over the script-message
  channel. Pulley demotes from the M6 plan to recorded fallback
  research (the engine-matrix decision stands if store policy or
  WKWebView limits ever bite; App Store honesty: this is a full local
  system that works offline, not a remote-site wrapper). The
  precision, hers in the same breath: "the ipad app runs the kernel
  and binaries inside webkit, but **/dev/canvas connects to swiftui**"
  — and "we can give webkit entitlement to access the local ipad
  filesystem." So the webview is an **engine room, not a display**:
  compute (kernel + wasm guests) runs in WKWebView for the JIT; the
  canvas tree crosses the script-message bridge and the presenter is
  native SwiftUI (stack→layout containers, text→Text, edit→the native
  editor, path→Path, events flowing back per the verb convention) —
  exactly the canvas doctrine's split, and cheap on the wire because
  semantic trees are small where raster frames were the 640GB lesson;
  and the app's entitlements serve the local filesystem inward to
  hostfs over the same bridge. The unification, stated once: **the
  browser port is the universal embedding, and every surface is a
  shell that lends it three things — a place to run, a screen to draw
  on, files to touch.** VSCode lends Node, its own panes, and a file
  API; the iPad app lends WebKit's JIT, SwiftUI, and its file
  entitlements. The ecosystem statement, cashed out.

- **(2026-08-30) The compensation thesis: complexity grows where a
  primitive is missing.** Christine's capstone over the dissolution
  series, recorded in her words: "the real benefit of this is that we
  are avoiding all the mistakes of linux, systemd, docker and
  kubernetes. These are overly complex systems because they did not
  have a per process namespace. We are not only living in the modern
  ecosystem, we are simplifying and replacing it." The causal argument,
  made precise so the claim can defend itself: **when a kernel lacks a
  primitive, userspace grows an industry** — and every such industry
  ships its own config dialect, its own daemon, and its own privilege
  model. Linux kept the global filesystem view; namespaces arrived
  piecemeal as `CLONE_` flags (2002–2013), root-only for a decade,
  disjoint from the file model — so Docker exists to *assemble* them
  (a privileged daemon, image formats to cache mutation, overlay
  filesystems to fake composition); systemd's unit files grew dozens of
  sandboxing directives (`PrivateTmp`, `ProtectHome`,
  `RootDirectory`…), each a hand-cut slice of what one `bind` verb
  gives uniformly, and imperative boot ordering that a declarative
  namespace file does not need (M2); Kubernetes then re-glues what the
  layer below fractured — pods to group processes namespaces would have
  grouped, CNI to give pods what per-process `/net` gives, service
  meshes to interpose what a 9P proxy does at the file layer,
  ConfigMaps to inject what a bind injects. The venv/flatpak entry
  (2026-08-29) was this same theorem's first instance; Docker and
  Kubernetes are its industrial form. The scope of "replacing,"
  reconciled with the ecosystem statement ("the web platform is our
  VAX"): we adopt the *surfaces* — browser, wasm, serverless — and
  simplify away the *middle* of the stack, the distro-systemd-docker-
  kubernetes plumbing between hardware and surface. Two honesty
  clauses, kept beside the claim: not all of that complexity is
  compensation — metering (cgroups' quota half, our standing open
  question), consensus (etcd exists because a cluster must *agree*;
  `bind -a` over `/svc` trees is a union, not a quorum, and the
  cluster stage owes this its real engineering answer), and hardware's
  own mess are essential complexity we inherit like everyone else. And
  the practitioners' temptation, named per virtue-ethics.md: those
  systems' bulk also encodes operational scar tissue earned under load
  we have not yet borne — we avoid their *structural* mistake, the
  missing primitive; our own scars are still ahead.

- **(2026-08-30) Containerisation and orchestration, planned: a Dockerfile
  is a process file, the orchestrator is a file server, kubectl is `cat`
  and `echo`.** Christine's directive, recorded in her words: "a
  Dockerfile simply sets up what packages need to be installed for a
  process and what commands needs to be executed - it is effectively a
  process instantiation specification. we can implement an entire process
  orchestration suite (in userspace). Effectively, we can do everything
  kubernetes can do, within the constraints of our architecture." The
  design, worked out from that observation:

  **The spec is a directory — the process file.** M2 already made boot a
  namespace file, and boot is just the instantiation of pid 1; the
  general case is a spec directory: `namespace` (M2's dialect verbatim),
  `packages` (name/version pairs — pkg verifies digests and refuses
  conflicts), `user` (an identity.md transition), `env`, `cmd`, and
  optionally `replicas` and `health` (a path to read — a liveness probe
  is a file read). The structural claim under it: **a Dockerfile is a
  script because installing is mutation; a process file is a declaration
  because installing is a bind.** `RUN` steps and image baking exist to
  cache filesystem mutation — with namespace assembly instant and
  package trees immutable under `/pkg`, there is no build step to cache.
  And the symmetry that makes it honest: a spec directory stands to a
  live process as `/proc/<pid>` stands to it at runtime — instantiation
  is introspection's mirror.

  **`run(1)` is docker run** — ~a hundred lines of userspace: rfork,
  install the declared packages, `newns()` the namespace file, set env,
  transition the credential through `/proc/<pid>/ctl` (the eve/ruid rule
  unchanged — no new mechanism), exec. Runs on today's kernel; nothing
  is missing after M4.

  **`svc(4)` is the control plane, as a file server.** Desired state is
  files you write, observation is files you read: `/svc/ctl` takes
  `start name spec` / `stop` / `scale name n`; `/svc/<name>/` holds
  `spec`, `replicas`, `pids`, `status`, `log`. A reconciler keeps
  desired and live equal with backoff — a Deployment is a directory
  entry, and kubectl is `cat` and `echo`. **A Service is a `/srv`
  post**: svc posts one name serving a 9P proxy that fans attaches
  across replicas — a load balancer is a 9P multiplexer, and consumers
  just `mount` it, oblivious. Rollout composes with what already
  exists: blue-green is repointing the post; rollback is binding the
  previous `/pkg` versions — or a `#V` snapshot.

  **The cluster stage** consumes M7 (`/net`) and M8 (identity on the
  wire), with `cpu(1)` as the precedent: a spec instantiated on a
  remote kernel that imports the caller's namespace pieces back over
  9P; a cluster's control plane is a union — `bind -a` each kernel's
  `/svc` into one tree, and the scheduler is any userspace program
  choosing which `ctl` to write. Policy lives in userspace; the
  mechanism is file writes.

  **What does not dissolve, kept on the record**: metering and quotas
  (the standing open question from the third-dissolution entry);
  bin-packing and affinity — genuinely policy, admissible as userspace
  programs, never core; inter-kernel networking, which belongs to the
  hosts. And the tripwire travels: if the spec dialect grows past its
  few files, we have rebuilt YAML Kubernetes and must stop. One
  pleasing inversion closes it: M1 ships the kernel *inside* a
  `FROM scratch` container; this suite makes processes the contained
  unit — and the two compose, a fleet manager running IPNX kernels,
  each orchestrating thousands of processes, pods of pods with none of
  the machinery. Sequenced at M12, re-aimed accordingly: the local
  stage (spec, `run`, `svc` on one kernel) is pure userspace and may
  interleave early; the cluster stage waits on M7+M8.

- **(2026-08-30) The namespace's third dissolution: every process is a
  jail, a container and a microVM — "our computer is a network."**
  Christine's articulation, recorded in her words: "per process namespace
  solves not only package dependency management and backups, it also
  significantly lessens the need for jails, containers, even kubernetes.
  every process is a jail, and a container, and a microvm. processes are
  isolated from each other, and it is possible to run a whole cluster of
  processes with different roles, network interfaces, sockets etc. side
  by side… Our computer is a network." (Sun's exact motto, inverted:
  "The network is the computer" — John Gage.) The argument, built out:
  isolation here is not a product but what a process *is*, and it is
  **double-walled by construction** — the namespace bounds what a process
  can *name* (its visible universe is its mount table, nothing else
  reachable), and the wasm instance bounds what it can *do* (no ambient
  syscalls, no ambient memory; only the mailbox). And because **9P is the
  only IPC**, every process boundary is already a wire boundary: two
  processes on one kernel differ from two machines on a network only in
  latency — `exportfs` demonstrates the equivalence, serving one
  process's world to another across any wire. That is what makes the
  inversion literal rather than rhetorical: one computer decomposes into
  a network of small machines, each with its own filesystem view, its
  own posted services (`/srv`), its own credentials (identity.md's
  roles), and — when M7 lands `/net` — its own network interfaces and
  socket space, Plan 9's own trick (bind a different `/net` and you are
  on a different network). Against each tool, honestly: chroot and the
  jail are one-shot, root-only namespaces without composition — ours is
  the general case of what they special-case. The container's isolation
  half dissolves (the image is a pkg subtree; isolating is just process
  creation), but its *metering* half — cgroups' cpu/memory quotas — does
  not, and stays a named open question (whether per-process quotas
  belong in this kernel, or whether the host OS metering the one kernel
  process suffices per deployment form). Kubernetes' service discovery
  is `/srv` plus binds; its scheduling and replication remain genuinely
  orchestration — "lessens," her verb, kept deliberately over
  "replaces." Status and standing test: the file half runs today
  (`rfork n`, private binds, pkg, `#V`); the network half is M7's
  mechanism, and this entry is measured the day two processes hold
  different `/net` binds side by side.

- **(2026-08-30) The versioning layer, v1: a snapshot is a tree, restore
  is a bind — landed as `#V`.** The immutable-systems doctrine
  (2026-08-29) gets its first mechanism. `echo snap t1 > '#V/ctl'`
  freezes the ram root by **structural clone**: nodes copied shallow,
  every byte buffer shared, the live side copying a buffer only on its
  next write to it (`Rc::make_mut` in the Rust core; a `dshared` mark in
  the demo kernel — the frozen oracle self-skips). Measured: twenty
  whole-root snapshots of the 710-node rootfs cost 8.9 MB and under a
  second, against ~50 MB of file data a copying design would have
  duplicated per snapshot (RESEARCH §9.8). The interface is three files
  and a verb pair — `ctl` takes `snap [name]` and `del name`, `#V/<name>`
  walks the past read-only, and **rollback is `bind '#V/t1/dir' /dir`**;
  a whole-system rollback is the same line first in `/lib/namespace`,
  which makes booting from a snapshot pure M2 machinery. Enforcement is
  one gate: `ram_access` refuses writes on snapshot nodes *before* the
  eve bypass — nobody rewrites history, eve included. Honest scope:
  these are epoch snapshots (taken when asked), kernel-resident, gone at
  shutdown. The doctrine's asymptote — *every write* an incremental
  version — and persistence across boots are M4's remaining storage
  question (a content-addressed store under hostfs); the interface is
  designed so both slot in behind `#V` unchanged. Alongside it, `ar(1)`
  landed on a measurement (wasm-ld links index-less archives, RESEARCH
  §9.8), closing the static-library gap: `ar r libx.a x.o` then
  `cc main.c -lx` now works end to end.

- **(2026-08-30) Compatibility, kissed goodbye — the userland is
  reimagined, and the verbatim world becomes the exhibit.** Christine's
  call, in her words: "Let's redesign sam, acme and the rest of the Plan 9
  utilities to use our new paradigms. It's time to kiss compatibility
  goodbye." This SUPERSEDES half of the re-founded userspace objective
  (2026-08-27): "the real Plan 9 userspace entire — the designers'
  curation of Unix" ends as a product goal. What replaces it: the
  CURATION survives, the VERBATIM does not — the essences are carried
  forward into native designs (docs/userland.md) and the vendored raster
  world reclassifies wholesale as heritage exhibit beside /v10 (still
  building, still running, still holding the suite's floor — the exhibit
  is load-bearing for conformance, never for design). The three classes:
  filters (cat, grep, sed, sort…) were ALWAYS native — their paradigm is
  files and pipes, which is our paradigm; nothing to redesign. Screen
  programs redesign onto canvas: ONE editor — acme-today absorbs sam
  wholesale (acme always contained sam's Edit language; two editors
  become one editor plus a language, with sam surviving as the language
  spec and a batch CLI); the console becomes an editable transcript (an
  edit node with a prompt discipline — acme's win(1) was the prototype;
  tty emulation retires); rio-today is the window-policy file server over
  host presenters; the plumber returns to the centre as the look verb's
  engine. Full designs and sequencing: docs/userland.md; M5 carries the
  build order.

- **(2026-08-30) `/dev/canvas` — the display is a semantic file tree; the
  modern-draw question, decided.** Settled in a five-round brainstorm
  (Christine's adversarial pushes each carved a clause; the full iteration
  record is in design-thinking.md). The decision, whole:
  **The model.** A window is a filesystem of semantic nodes — acme's file
  interface generalised, not libdraw modernised. Six node kinds in v0:
  `stack · text · edit · image · path · frame`. Content is greppable plain
  UTF-8 (styling in sidecar `attrs`; a live UI can be grepped, tested, and
  read by an agent without any engine). `edit` lifts acme/sam's addr/data
  buffer interface — the crown jewel; typing echo, selection, scrolling and
  IME are presenter-local, apps observe via events (acme's own discipline).
  `path` carries SVG path data verbatim; `frame` is pixels-in-a-grid,
  honestly named and honestly opaque (video is a frame updated at cadence).
  `draw` the name retires with its era. One `events` file per window
  (resize · close · tap · execute · look · key) — resize/close as protocol
  is the target that retires the demo's deferral. `sync` commits atomically.
  **Surfaces.** A surface is anything that renders the tree, and it
  NEGOTIATES capabilities (interactive/reflow/input/snapshot): browser, Mac
  and iPad presenters; SVG, PDF, PostScript as write-only document surfaces
  (layout resolution is a surface property — "print" is attaching a
  document surface); virtual surfaces — a test asserts on the tree (pixel
  censuses retire), the accessibility reader IS a render; remote surfaces
  over 9P (exportfs a window = a semantic remote UI); multiple surfaces on
  one canvas = mirroring for free. To the browser, the surface is ONE
  universal SPA, ours and cached, consuming tree-files and emitting DOM.
  The scope of the refusal is the PROTOCOL, not the apps' knowledge
  (corrected 2026-08-30): an ipnx app may of course understand the web —
  fetch HTML over '#H', parse it, generate it, even be a browser — but
  markup never crosses /dev/canvas; the display protocol speaks the six
  kinds regardless of what the app knows.
  **The four clauses from the pushback rounds.** (1) *SVG*: for the marks
  corner we ARE describing SVG and adopt it outright (path data, transform
  and colour notation); the whole is not SVG — no flowed text, no editing,
  no protocol (SVG 1.2's flowed text died; the web itself needed HTML+SVG).
  Rule: adopt notation, own the model. (2) *HTML*: the semantic retained
  tree is the web's discovery and we adopt it — re-housed: the tree as
  files not a JS-API, events as files, no behaviour inside the surface,
  and a vocabulary small enough that a phone, a PDF and a test are peer
  surfaces (an HTML-subset model would make every surface a browser).
  (3) *Borrowed engines*: the hard four-fifths of a visual surface is the
  era's text engine, and we borrow the local one — WebKit on the web,
  CoreText/TextKit on Apple — as RENDERERS, never runtimes; the canvas
  protocol is the narrow waist above them, exactly as 9P is the waist above
  V8/wasmtime (NeXT licensed Adobe's interpreter: "display is PostScript"
  was always own-the-protocol, borrow-the-engine). Layout divergence
  between engines is accepted and named; document surfaces choose their
  authoritative resolver. (4) *The ecosystem statement*: **the web platform
  is our VAX** — the modern stack (browser, WebKit, wasm, serverless) is
  the hardware of the era; we port to hardware and do not adopt its
  operating system, because we are the operating system. The invariant that
  is IPNX: adopt substrates, engines and notations; refuse object models
  (wasm yes, WIT no; sockets yes, POSIX no; V8 yes, JS-as-model no).
  **Input: the verbs leave the buttons.** The three-button mouse encoded
  verbs — b1 point/select, b2 execute, b3 look — and the verbs become
  system-level: tap plumbs (look is the free gesture), tags are genuinely
  tappable, selections raise the native popover (Execute · Look · Cut ·
  Copy · Paste), ⌘Enter executes at the keyboard; chords die into the host
  clipboard via /dev/snarf. Real button hardware maps straight onto the
  verbs — the paradigm is discarded as a requirement, kept as an
  accelerator. A compat layer may synthesize /dev/mouse for raster-era
  clients.
  **The reclassification.** Raster sam/acme stay runnable as they run
  today, reclassified as heritage exhibit beside /v10 — not a constraint on
  canvas. sam-today is its command language over the server's edit buffers
  (libframe evaporates); acme-today is a policy client of stacks, edit
  nodes and actionable tags, still serving its own 9P interface. Their
  essences — structural regexps; everything-is-text-and-text-is-actionable
  — intensify rather than survive.
  **Commitments.** Canvas surfaces embeddable in web pages; a first-class
  JS/TS client library; the six kinds learnable in an afternoon; a
  quarantined `web` leaf stays a personality-shaped future door (not v0).
  **The tripwire.** v0's vocabulary is measured against four benchmarks —
  sam-today, acme-today, rio-today, one plot. If it ever grows past
  roughly a dozen kinds, the HTML refusal is declared wrong and adoption
  of a real subset is re-litigated. Open: whether `image` folds into
  `frame`; edit's addr/data taken verbatim vs simplified.

- **(2026-08-29) Immutable systems and time travel are namespace operations.**
  Christine, in her words: "If files are tagged with version numbers, and
  every write results in an incremental version, then backup is simply a
  snapshot of a namespace at a given time, and can be restored simply by
  binding the correct versions. We can truly roll back to any point of time
  in the past." This is Plan 9's dump filesystem and Venti given the
  general form: there, the snapshot was the file server's nightly gift and
  yesterday(1) bound you into it; here it falls out of the same two
  primitives everything else uses — versioned files are just files, a
  snapshot is a recorded set of binds (a namespace fragment — M2's format
  again), and restore/rollback is applying it. Immutability is the same
  fact seen forward: a process bound to fixed versions cannot be changed
  underneath. Sequenced, not built: this shapes M4's storage design
  question (a content-addressed or versioning layer under hostfs) and the
  profile milestone (a profile snapshot IS a backup of an identity); the
  fragment format is already in the tree.

- **(2026-08-29) Dependency hell and package conflicts dissolve in the
  namespace.** Christine's articulation, recorded in her words: "Since every
  process has its own namespace, and the package manager simply binds files,
  we don't have to do dependency management — every package installs exactly
  the files it needs. We can also resolve package conflicts: if a package
  binds a different file to the same name, we can reject the install." The
  two classic package-manager problems are artifacts of a GLOBAL filesystem,
  and this system does not have one: (1) versions coexist by construction —
  /pkg/<name>/<version> keeps every version's subtree, and a namespace binds
  the one it wants, so "A needs libfoo 1, B needs libfoo 2" is two binds in
  two worlds, not a solver problem. The OS layer therefore carries NO
  dependency machinery (language ecosystems keep their own graphs — pip
  still walks Requires-Dist — but that is the personality's business, not
  the package layer's). (2) A conflict is well-defined and CHECKABLE: a name
  about to be bound that already resolves to DIFFERENT bytes is a genuine
  collision, and pkg refuses the install (same bytes = idempotent, allowed).
  Union order already makes deliberate shadowing expressible (bind -b);
  refusal guards only the accidental case. And the third consequence, hers
  in the same breath: "it also means we can have multiple dev environments
  coexisting as different processes — each process can manage its own
  package versions." What venv, nvm, rbenv and every toolchain manager
  simulate with PATH shims and activation scripts, the namespace gives
  natively: `rfork n` (or newns with a fragment) and install — the
  environment IS the process's namespace, it nests, it composes with
  everything else namespaces do, and it vanishes with the process. All
  three consequences implemented and pinned in pkg(1) and the suite the
  same day.

- **(2026-08-29) The package model — a package is a subtree, installing is
  binding.** Investigated at Christine's direction before M1 ("genericise
  this personality to allow us to import from external registries"), with
  the survey and the proof in RESEARCH (WLR's ruby-3.2.2.wasm, fetched,
  digest-verified, ran on the unmodified WASI personality — the import
  machinery is already generic for well-behaved preview1 commands). The
  design, in six commitments:
  1. **No package database.** A package is a file tree under
     `/pkg/<name>/<version>/`; installing binds it (`bind -a …/bin /bin` —
     the union directory is the merge mechanism); uninstalling unbinds. The
     namespace is the installation record, which makes installs per-process
     by construction and per-identity via the profile: "installed software"
     persists as a profile namespace fragment (identity.md), so a person, a
     role and an agent each carry their own package set. An agent can have
     packages its person does not.
  2. **The personality declaration rides the package**: a `meta` file in the
     namespace-fragment dialect (bind lines, plus small extensions: `abi
     wasip1|wasi_unstable|plan9`, `env K=V`). The three provisioning layers
     map onto package content — the ABI names the shim; `libs/` packages
     land as layer-2 sysroot trees (`/lib/wasm32-wasi`, `/include`) that
     `cc -l` links; runtime-support trees and env are layer 3.
  3. **`pkg(1)` is the v1**: `pkg install <registry>/<name>[@ver]`, `list`,
     `remove`, `verify`. Fetch over `#H` (later `/net`); **sha256 verified
     always** (every surveyed registry publishes digests; refusal on
     mismatch, as pip already does); unpack; bind per `meta`. pip remains
     the Python ecosystem's arm; pkg handles wasm commands and sysroots.
  4. **Registries as filesystems is the end-state**: a registryfs (a
     userspace 9P server per registry, the hellofs/exportfs lineage)
     translating walks into API calls — then installation decays into
     `cp -r /n/wlr/ruby/latest /pkg/… ` plus a bind, and the package
     manager nearly disappears into the file model. Where the wasm world
     links components, ipnx mounts servers; WIT/warg stay refused per the
     founding decision.
  5. **The demo ships a curated same-origin mirror** — GitHub asset
     downloads send no CORS (measured), so the browser host cannot fetch
     them directly; a small `/registry/` on the demo site (verified
     runtimes plus digests, each under the 50MB line) makes `pkg install
     ruby` work in the tab, makes the demo the first ipnx registry, and is
     the curation principle applied to acquisition. Native hosts reach the
     real registries unconstrained.
  6. **Trust is digests now, identity later**: v1 pins sha256; signature
     schemes (warg's logs, sigstore) wait until the identity milestone
     gives them an anchor. The capability doctrine already does the heavy
     lifting — an installed binary gets nothing but the namespace it is
     started with, so trying untrusted software is safer here by
     construction than on any system where install grants ambient
     authority.
  Open, deliberately: the command's name (`pkg` is the placeholder); the
  wasix personality (plausible — ipnx has real fork/exec — but unplanned);
  OCI artifacts as a registry (aligns with M1, native-first).

- **(2026-08-29) The toolchains are real — the time for shims is over.**
  Christine's directive: "We need the toolchains to work, not demo shims…
  this is for real. We need the ability to install packages, etc. so pip
  needs to work as well. All this is what a user would expect from a demo."
  Acceptance is external and measured: Python runs the example programs from
  python.org; Go runs the examples from gobyexample.com (verified 2026-08-29:
  the python.org front-page programs verbatim, and seventeen gobyexample
  features — closures, generics, interfaces, goroutines, select, atomics,
  regexp, text/template, base64, net/url — compiled in-guest and run).
  What landed: the FULL 529-file CPython stdlib (the wasi build's own Lib) in
  place of the PoC's 21-file subset; a pure-Python `zlib` (a puff-shaped
  inflate; this CPython build has no zlib builtin — measured) as a
  personality file; `pip` installing real pure-Python wheels from the real
  PyPI — sha256-verified, RECORD-listed, `python -m pip` the same program —
  over `#H`, a webfs-shaped kernel device (the network as files; the /net
  milestone's forerunner); **the real gc compiler and linker cross-built to
  wasip1 and run as guest processes**, orchestrated by a real `go` driver
  (build/run/fmt/version/env) exactly as cc(1) drives clang, with the
  stdlib's export archives for the gobyexample-derived package set (115
  packages, 35.3MB; net/http refused at +31MB until sockets exist); and
  cc(1) grown to -E/-S/-lm. Upstream pip itself still cannot run (it demands
  C-level ssl+socket, absent from this CPython build) — pip here is ipnx's
  own installer speaking the real protocol to the real index; when /net
  lands, upstream pip becomes the target.

- **(2026-08-29) The porting inversion — personalities carry the
  environment, not patches on the source.** Articulated by Christine after
  the demo's C toolchain landed. The universal way to port software is to
  change the *source* until it matches the target's environment (its libc,
  its headers, its syscalls). **IPNX inverts this: build a port *personality*
  — a libc.a, its headers, and the runtime environment — so that the
  *unmodified* source compiles and runs.** Personalities are cheap here (they
  are libc dialects over the one kernel — the founding decision), so IPNX can
  afford a whole shelf of them: a GNU/glibc personality, a musl personality, a
  Linux personality, a BSD personality — each an environment against which
  real-world packages build with no edits. This is strictly more useful than
  any single Linux or BSD distribution, which binds you to one libc and one
  ABI: IPNX presents whichever environment the source expects, per process,
  by namespace. The demo already contains the first instance — a wasi-libc /
  POSIX personality (the wasi sysroot the in-tab `cc` compiles against): stock
  C compiles unmodified because the *environment* was supplied, not because
  the source was bent. Relationship to the measured modern personality
  (2026-08-27): the two compose and do not conflict. **`libunix` is the
  *native* modern-Unix personality, derived by measurement** (the 20% of
  POSIX that git/CPython/Go actually call) — the surface IPNX offers as *its
  own* Unix. **The port personalities are fuller environments whose telos is
  compiling foreign source unmodified** — measured by "does the package
  build," not by taste. A program picks its personality the way it picks its
  libc; the kernel underneath is unchanged. This is the deep reason
  compilation-as-a-capability (`/cc`) and the toolchain work matter beyond the
  demo: each port personality plus a compiler is a machine for absorbing the
  existing software world without forking it.

- **(2026-08-29) The demo is the product.** Christine's formulation, verbatim
  doctrine: *"It's like a game demo. A lousy game demo means no one will buy
  the game."* The public demo is every visitor's one and only encounter with
  the system — for them it IS the implementation, and it has to work BETTER
  than what ships later, because a visitor who sees faults leaves and never
  returns. Consequences: **(1)** demo quality items (genuine guest window
  resize, clean close, first-five-minutes polish) are product work, never
  deferred as "waiting for the real implementation"; **(2)** the demo stops
  being confined to page-level derivations of the frozen kernel — `poc/`
  remains the untouchable conformance oracle, but the demo ships its own
  supervisor lineage (forked from the reference) where visitor-facing kernel
  needs land first: window-refresh events for true guest resize, close
  semantics, and whatever the experience demands; the fork is judged by the
  same suite plus its own additions. **(3)** The deployment ledger's review
  treats demo-quality regressions as shipped-form failures, not cosmetics.

- **(2026-08-29) The six-hats pass — the blind spots, caught and adopted.**
  De Bono's parallel-thinking sweep run over the whole project at the
  declaration; the record is [six-hats.md](six-hats.md). The catches, each
  now in the plan: **CI on push via the M1 container** (the floor stops
  being a manual discipline) and **the oracle in amber** (a pinned-Node
  image, because frozen means unfixable and host drift would kill it);
  **toolchain pinning at M0** (`VERSIONS` — the §9.4–9.5 findings are
  version-dependent and the versions were recorded nowhere); **the bench
  pass** (zero runtime performance numbers existed — P4's belief test had
  no baseline); **the WebKit gate on M6's stopgap claim** (a dated decision
  rode on an unverified assumption); **the trust-boundary contract in
  architecture.md**; **NOTICES on the demo**; accessibility and M8's
  boring-primitives rule noted where they will be paid. Meta-verdict,
  recorded: the thinking phase is complete — declare, roles, design
  thinking, hats — and the review rituals consolidate into one cadence
  (ledger + design-thinking + hats; after each shipped form, any validation
  event, or quarterly). The next commit builds.

- **(2026-08-29) The virtue-ethics pass — character made explicit.** The
  third lens ([virtue-ethics.md](virtue-ethics.md)): the telos named (the
  counterfactual *inhabited*), the project read as a MacIntyrean practice
  whose goods are internal, and eight virtues located as means between
  vices with receipts from this log — the founding thesis restated as
  ethics: **Plan 9's temperance tipped into excess at the compatibility
  break, and this project is the recovered mean**; the log itself
  recognised as a record of means being found (necrolatry↔vandalism at the
  re-founding, cliff↔bloat in the measured personality, garden↔doormat in
  typed-at-the-edges). Three dispositions adopted: the **external-goods
  question** joins the ledger review (MacIntyre's corruption warning,
  asked on the record before the demo brings an audience); **M5's
  acceptance gains the text-mirrored console** (accessibility reframed
  from feature to justice); and the working rules gain **"a commit message
  claims only what its diff contains"** — habituated from the same day's
  worked example, an overclaimed commit caught against the tree. The
  practitioners section binds both practitioners, human and AI, and names
  the AI's standing temptations: overclaim, flattery, performative
  industry. The lens joins the consolidated review cadence.

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

- ~~The uid model~~ — **decided and running**: [docs/identity.md](identity.md). Per-process
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
- **The modern-draw question** (raised by the author's demo review,
  2026-08-29): character windows are DOM-first (xterm.js — settled and now
  shipped in the demo shell); should draw(3) itself gain a modern backend —
  the `d`/`L`/`e` ops mapped to canvas/SVG/SwiftUI vectors rather than a
  raster — or a modern *dialect* for future personality apps, while the
  verbatim editors keep the raster path their code emits? Bitmapped displays
  are not how UIs are built now; the file-server *interface* is the
  invariant, the backend is per-platform by decision. Needs a dated decision
  before M3's presentation layer hardens.
- kencc or clang for the *ported* userspace, given `extern register` and anonymous struct
  members? Fresh code is clang (§9.4 is the measured recipe).
- Does the `d` message's alpha compositing map cleanly onto canvas and Metal, or does the
  window server rasterise? Decides whether the backend is thin.
- Heap sizing where guest memory is *shared* with the supervisor for zero-copy I/O — a
  shared memory must declare its maximum up front. The unshared default grows freely.

## The proof of concept — complete, and recorded elsewhere

The PoC ran 2026-08-26 → 2026-08-29 and was **declared complete** at 131
acceptance tests green on three hosts (Node, Chrome, the Rust core under
wasmtime), with the real Plan 9 userspace — rc, sam, acme — the V10 exhibit,
and the WASI citizens (Go, CPython) running. Its full chronology and final
state are frozen in [poc.md](poc.md); `poc/` itself is frozen as the reference
implementation and conformance oracle. **The build sequence from here is
[implementation.md](implementation.md)** — this document remains the design
and the decision log.

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
