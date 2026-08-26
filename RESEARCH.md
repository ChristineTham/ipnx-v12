# A hosted Plan 9 with a Research Unix personality — feasibility study

*Begun 2026-08-26 and **living** — the evidence base behind
[docs/v12-plan.md](docs/v12-plan.md). Findings land here as they are established, with their
provenance; decisions and scope stay in the plan.*

*It is written to be self-contained. Measurements taken against the Research Unix V10 tree
were made in the parent repository ([ipnx](https://github.com/ChristineTham/ipnx)) and are
recorded here as data, because that tree is deliberately **not** copied into this one.*

## TL;DR — the recommendation

Build a **modified Plan 9 kernel that runs as an ordinary userspace process** on macOS,
iPadOS and in the browser, executing **WebAssembly** binaries in a per-process namespace,
and add a **Research Unix Tenth Edition personality** on top of it later.

Four findings drive that, in the order they mattered:

1. **A V10 kernel does not survive the crossing.** Of 61,072 lines of C and 3,141 of VAX
   assembly, only ~3,300 lines of process semantics have anything to say on a target with no
   MMU and no hardware. That is a rewrite, not a port (§1).
2. **The surgery runs the right way round.** Adding Unix semantics to Plan 9 is *addition* —
   `setuid`, hard links and `umask` are things Plan 9 chose not to have. Adding Plan 9
   semantics to Unix is *eviction* (§2). And only one direction has a precedent: APE.
3. **The architecture has been built three times** — plan9port, 9vx, Inferno `emu` — and
   each got one thing wrong. This project is **`emu` with WebAssembly in place of Dis** (§4).
4. **iOS forces the choice of execution substrate.** Apps cannot spawn child processes and
   cannot JIT, so the jail cannot be host processes or native code. 9vx's answer (vx32) is
   x86 and dead; emu's answer is a VM. Wasm is the VM with an ecosystem (§4.3).

**Do not use WASI as the system interface** (§6). Its filesystem proposal names this
project's founding premise as a stated non-goal, and no WASI proposal at any phase covers
processes. WASI's role is `wasi:cli/command` — argv, environ, exit, stdio — so a ported
foreign program can find its arguments.

**`fork` was the one genuinely unsolved thing** (§5) and the design turns on
`rfork(RFPROC|RFMEM)` — §5.2 now records the lazy path's resume mechanism and its bound
(the `procrfork(fn)` shape), proven end-to-end in [poc/](poc/), and §5.3 the syscall
transport that needs no asyncify.

---

## 1. Why not retarget the V10 kernel

Measured against `v10/usr/src/sys/` in the parent repository:

| Subsystem | Lines | Files | Fate on this target |
|---|---|---|---|
| `io/` — device drivers | 27,035 | 60 `.c` | Gone. No hardware to drive |
| `md/` — per-machine | 8,116 | 40 `.c` | Gone. There is no machine |
| `vm/` — demand paging | 5,882 | 12 `.c` | Gone. The runtime is the MMU |
| `inet/` | 5,419 | 17 `.c` | Replaced by a `/net` server |
| `fs/` | 4,538 | 9 `.c` | Replaced by 9P |
| `os/` — process semantics | 7,477 | 23 `.c` | **~3,300 survives** |
| `ml/` — `swtch.s` `trap.s` `setjmp.s` `copy.s` | 3,141 asm | 24 `.s` | Gone. No registers to save |
| `misc/` | 2,605 | 10 `.c` | Config-generated |

**Total: 61,072 lines of C, 3,141 of assembly.** Roughly 5% has anything to say.

### What V10 did get right, and it is more than expected

Worth recording because it is the strongest argument that Research Unix was already moving
toward this design:

`usr/src/sys/sys/conf.h:44-56` defines the file system switch as eleven operations:

```c
struct fstypsw {
	int		(*t_put)();	int	(*t_updat)();
	int		(*t_read)();	int	(*t_write)();
	int		(*t_trunc)();	int	(*t_stat)();
	int		(*t_nami)();	int	(*t_mount)();
	int		(*t_ioctl)();	struct inode * (*t_open)();
	int		(*t_dirread)();
};
```

and a real machine's config (`misc/most.c.c:146`) registers **seven types** behind it:

| slot | symbol | source | lines | what it is |
|---|---|---|---|---|
| 0 | `fsfs` | `fs/fs.c` | 823 | the disk filesystem |
| 1 | `nafs` | `fs/neta.c` | 695 | netfs protocol A — remote |
| 2 | `prfs` | `fs/proca.c` | 736 | **`/proc`** — processes as files |
| 3 | `msfs` | `fs/ms.c` | 279 | the mount layer |
| 4 | `nbfs` | `fs/netb.c` | 785 | netfs protocol B — remote |
| 5 | `erfs` | `fs/errfs.c` | 31 | the error filesystem |
| 6 | `pipfs` | `fs/pipe.c` | 128 | **pipes**, as a filesystem |

On this machine a process is a filesystem and so is a pipe. `fs/proca.c` includes
`sys/proc.h`, `sys/text.h`, `sys/reg.h`, `sys/pioctl.h` — it is Killian's `/proc`, which went
on to Plan 9.

And `os/mount.c` (67 lines entire) mounts a **file descriptor**, not a device:

```c
register struct a { int fstype; int fd; char *name; int flag; } *uap = ...;
if ((fp = getf(uap->fd)) == NULL) { u.u_error = EBADF; return; }
(*fstypsw[uap->fstype]->t_mount)(fp->f_inode, ip, uap->flag, 1, uap->fstype);
```

That is Plan 9's `mount(fd, afd, old, flag, aname)` shape, five years early.

### And the one thing it got wrong that decides the direction

`os/mount.c`'s `funmount` reads `if ((mip = ip->i_mpoint) == NULL || mip->i_mroot != ip)`.
**The mount is a pair of pointers between two inodes in the global inode table.** There is no
per-process mount list to make per-process; the binding is a property of a shared object, so
every process necessarily sees every mount. Per-process namespaces mean lifting that out of
`struct inode` and rewriting `nami()` (117 lines) and `fsnami()` (528) and `iget()` (322).

Plan 9 never made that mistake.

---

## 2. Plan 9's interface, complete

`/sys/src/libc/9syscall/sys.h` names 52 slots (0–47, 50–53). Eleven `_`-prefixed entries
are superseded variants retained for old binaries and slot 0 is reserved, leaving **40 live
calls**:

```
SYSR1 0     _ERRSTR 1    BIND 2       CHDIR 3      CLOSE 4      DUP 5
ALARM 6     EXEC 7       EXITS 8      _FSESSION 9  FAUTH 10     _FSTAT 11
SEGBRK 12   _MOUNT 13    OPEN 14      _READ 15     OSEEK 16     SLEEP 17
_STAT 18    RFORK 19     _WRITE 20    PIPE 21      CREATE 22    FD2PATH 23
BRK_ 24     REMOVE 25    _WSTAT 26    _FWSTAT 27   NOTIFY 28    NOTED 29
SEGATTACH 30 SEGDETACH 31 SEGFREE 32  SEGFLUSH 33  RENDEZVOUS 34 UNMOUNT 35
_WAIT 36    SEMACQUIRE 37 SEMRELEASE 38 SEEK 39    FVERSION 40  ERRSTR 41
STAT 42     FSTAT 43     WSTAT 44     FWSTAT 45    MOUNT 46     AWAIT 47
PREAD 50    PWRITE 51    TSEMACQUIRE 52 NSEC 53
```

Grouped: **files** (`open` `create` `close` `read` `write` `pread` `pwrite` `seek` `stat`
`fstat` `wstat` `fwstat` `remove` `dup` `pipe` `fd2path` `chdir`) · **namespace** (`bind`
`mount` `unmount` `fversion` `fauth`) · **process** (`rfork` `exec` `exits` `await` `sleep`
`alarm`) · **notes** (`notify` `noted`) · **memory** (`brk_` `segbrk` `segattach` `segdetach`
`segfree` `segflush`) · **sync** (`rendezvous` `semacquire` `semrelease` `tsemacquire`) ·
**misc** (`errstr` `nsec`).

From `intro(2)`: *"All system calls return integers, with –1 indicating that an error
occurred; errstr(2) recovers a string describing the error."*

### `rfork` flags, verbatim from `rfork(2)`

The manual's first line settles the relationship to Unix: **"*Fork* is just a call of
`rfork(RFFDG|RFREND|RFPROC)`."**

| flag | the manual's words |
|---|---|
| `RFPROC` | "If set a new process is created; otherwise changes affect the current process." |
| `RFMEM` | "If set, the child and the parent will share data and bss segments. Otherwise, the child inherits a copy." |
| `RFFDG` | "the invoker's file descriptor table is copied; otherwise the two processes share a single table" |
| `RFCFDG` | "the new process starts with a clean file descriptor table" |
| `RFNAMEG` | "the new process inherits a copy of the parent's name space; otherwise […] shares" |
| `RFCNAMEG` | "the new process starts with a clean name space" |
| `RFNOMNT` | "subsequent mounts into the new name space and dereferencing of pathnames starting with # are disallowed" |
| `RFNOTEG` | "the process becomes the first in a new group" |
| `RFNOWAIT` | "the child process will be dissociated from the parent" |
| `RFREND` | "the process will be unable to rendezvous with any of its ancestors" |
| `RFENVG` / `RFCENVG` | environment copied / empty |

**`RFNOMNT` and `RFCNAMEG` are the capability primitives** — a sealed namespace expressed as
a fork flag rather than as a separate security subsystem.

### The design paper's own statement of the mistake

From *Plan 9 from Bell Labs* (Pike, Presotto, Thompson, Trickey):

> "First, resources are named and accessed like files in a hierarchical file system. Second,
> there is a standard protocol, called 9P, for accessing these resources. Third, the disjoint
> hierarchies provided by different services are joined together into a single private
> hierarchical file name space."

and

> **"Compatibility was not a requirement for the system. Where the old commands or notation
> seemed good enough, we kept them. When they didn't, we replaced them."**

A choice, not an oversight — which is what makes it undoable.

### `a.out(6)`

```c
typedef struct Exec {
  long magic; long text; long data; long bss;
  long syms;  long entry; long spsz; long pcsz;
} Exec;
```

All fields **"4-byte integers in big-endian order"**, regardless of target — worth
remembering on a little-endian host. Magic is `_MAGIC(b) = ((((4*b)+0)*b)+7)`; the published
table runs 68020(8), 386(11), 960(12), SPARC(13), MIPS(16), DSP3210(17), MIPS4000(18),
29000(19), ARM(20), PowerPC(21), MIPS4000LE(22), Alpha(23). **There is no VAX magic in the
4th-edition list.**

### The wire protocol has one defined version

From `version(5)`: "The version request negotiates the protocol version and message size to
be used on the connection", the string "must always begin with the two characters ″9P″", and
**"Currently, the only defined version is the 6 characters ″9P2000″."** An unrecognised
version is answered with `Rversion` carrying "unknown", so negotiation is built in.
Original 9P — `NAMELEN` 28, `Tsession`, DES tickets — survives only in period servers such
as V10's own `u9fs`, which nothing here needs to interoperate with: the V10 personality is
an API personality, not a wire one. **9P2000** (the decision is recorded in the plan).

---

## 3. The V10 personality (for later)

Not the first work. Recorded now because the mapping is what makes the project's claim
credible and it should not have to be re-derived.

V10's `os/sysent.c` fills 69 of 128 slots with **68 distinct routines**. Against Plan 9's 40:

| Class | Count | Detail |
|---|---|---|
| **A — direct** | **28** | `exit`→`exits` · `fork`→`rfork` · `creat`→`create` · `unlink`→`remove` · `exece`→`exec` · `wait`→`await` · `fmount`→`mount` · `funmount`→`unmount` · `sbreak`→`brk_` · `dirread`→`read` on a directory · `pause`→`sleep` · `gtime`/`ftime`→`nsec` · plus `read` `write` `open` `close` `lseek`/`seek` `chdir` `stat` `fstat` `dup` `pipe` `alarm` `mkdir` `rmdir` `getpid` |
| **B — library only** | **12** | `chmod` `fchmod` `chown` `fchown` `utime` → **`wstat`/`fwstat`** · `chroot`→`rfork(RFCNAMEG)`+`bind` · `setpgrp`→`rfork(RFNOTEG)` · `getlogname`→`/dev/user` · `saccess`→ try the open · `nice`/`times`→`/proc/n/ctl`, `/proc/n/status` · `sync`→ the file server's business |
| **C — genuine work** | **17** | `link` · `symlink`/`readlink`/`lstat` · `setuid` `setgid` `setruid` `getuid` `getgid` `setgroups` `getgroups` · `umask` · `mknod` · `ioctl` · `select` · `kill`/`ssig` |
| **D — machine management, drop** | **11** | `stime` `sysacct` `biasclock` `syslock` `sysboot` `profil` `vadvise` `vlimit` `vswapon` `vtimes` `nap` |

**40 of 68 — 59% — are free or library-only.**

*The counts are of **routines**, not of table rows: `lseek` and `seek` are two `sysent.c`
entries with one Plan 9 answer, as are `gtime` and `ftime`, so class A lists 26 entries and
accounts for 28 calls. `lstat` sits in class C because Plan 9 has no symbolic links, so it
has nothing to mean until `symlink` does. Derived against `v10/usr/src/sys/os/sysent.c`:
69 filled slots, 68 distinct routines, `fork` twice (slot 2 and slot 66, "former vfork");
28 + 12 + 17 + 11 = 68.*

Class B is where Plan 9 is smaller *and better*: `chmod`, `fchmod`, `chown`, `fchown` and
`utime` are five V10 calls that each write one field of a file's metadata, and `wstat`
replaced the family by writing a `Dir`. (V10 has no `rename` syscall — renaming is
`link`+`unlink` in userland — so `rename` is the personality's libc's business, not a
class-B row.) Restore them as libc functions, never as syscalls.

Of the 17 in class C, most are cases Plan 9 was right about — `ioctl` becomes `ctl` files,
`mknod` becomes file servers, `chroot` becomes `bind`, `select` becomes processes. **The
genuine kernel additions are the uid model and hard links.**

### APE, and why this is not it

APE (the ANSI/POSIX Environment) is the proof that the direction works — POSIX over Plan 9,
built by the people who designed both. Its confessed limitations, verbatim, are the map of
where the kernel has to help:

> setting the userid, groupid, effective userid and effective groupid **do not do anything
> useful. The concept is impossible to simulate in Plan 9.**
> · `link` always fails
> · `umask` has no effect, as there is no such concept in Plan 9
> · the functions dealing with stacking signals, `sigpending`, `sigprocmask` and
> `sigsuspend`, do not work
> · `O_NOCTTY` option has no effect. **The concept of a controlling tty is foreign to Plan 9.**
> · `setsid` forks the name space and note group, which is only approximately the right behavior
> · Advisory locking via `fcntl` is not implemented
> · `execlp` and the related functions do not look at the PATH environment variable
> · `isatty` is only sometimes correct

**Every one of those except the first two is a POSIX.1-1990 feature V10 does not have.** No
`sigaction`, no `sigprocmask`, no sessions, no `fcntl` locking, no job control, no sockets,
no `mmap`, no threads — V10's signal API is V7-style `ssig` entire.

So the V10 personality is the subset of APE that would have existed had its authors targeted
their own previous system rather than a standards committee's. **The uid model is the item
that decides whether the answer is yes**, and it should be designed first.

### V10 measurements worth keeping

Taken against the parent repository's tree, and not re-derivable here:

- `usr/src/cmd`: **5,059 `.c` files, 1,446,595 lines**
- **239 `fork(` call sites**; 196 `.c` files call `fork`, of which **161 also call some
  `exec*` and 35 do not** (the 35 are daemons — `fsck`, `update`, `wall`, `dump`, `uucp`,
  `daemon0`, `postio`)
- The shell has **exactly one** fork site, `sh/xec.c:432`, with two exits:
  `if (type != TCOM) execute(forkptr(t)->forktre, ...)` — the subshell branch, which recurses
  into the interpreter and never execs — `else if (com[0] != ENDARGS) execa(com)`.
  **So the fast fork path cannot be chosen by inspecting a binary; it must be chosen at the
  call.**
- V10's syscall ABI is `chmk` with the number as operand — `libc/sys/open.s` is
  `.set open,5` / `chmk $open`. Plan 9's `OPEN` is 14, `CLOSE` 4, `DUP` 5: **the number
  spaces collide on the same instruction**, which is why running Plan 9 binaries on a Unix
  kernel would need a tagged personality as well as a second `a.out` format.
- `sysent.c` slot 66 is `0, fork, /* 64 +2 = former vfork */` — vfork's number, aliased back
  to `fork` once the VAX had virtual memory to make copying cheap.

---

## 4. Hosting: three precedents

| | Kernel | Guest execution | What it got wrong |
|---|---|---|---|
| **plan9port** | none — a library port | native host processes | No kernel, so no namespaces, no `bind`, no `rfork` flags. `devdraw` **abandons the file interface for graphics** because Unix cannot give each client its own `/dev/draw` |
| **9vx** | Plan 9's, as a user program | **vx32**, a user-level x86 sandbox | x86-only; dead on Apple silicon |
| **Inferno `emu`** | Inferno's, hosted | **Dis** bytecode | The right architecture, the wrong VM — Dis never got an ecosystem |

9vx is the closest technical relative: it *"runs as an ordinary user program, but behaves
like a separate VM running Plan 9"*, treating vx32 as *"an architecture with a
software-managed TLB"*, unmapping all pages on context switch and remapping on demand, with
faults returning a virtual trap it handles as Plan 9 would; it preempts by asking the host
for `SIGALRM` at intervals. Everything there transfers except the sandbox.

`emu` is the closest architectural relative: *"The Inferno kernel can run both native and
'hosted' on a range of platforms and which presents the same interface to programs in both
cases."*

**This project is `emu`'s architecture with WebAssembly in place of Dis.**

### 4.3 The constraint that forces wasm

**iOS apps cannot spawn child processes** — `fork()` and `posix_spawn()` are prohibited in
the sandbox — and cannot create writable-executable pages, so there is no JIT. The jail
therefore cannot be host processes and cannot be native ARM64 code, which eliminates 9vx's
answer and every "real processes with a shim libc" variant on the platform that matters most.

Note the corollary: **binary compatibility was the only thing that ever required a trap.**
Once everything is recompiled, the syscall boundary can be a function call into a libc that
marshals to the kernel. What "jail" then means is *namespace* isolation, not memory
isolation — and `RFNOMNT`/`RFCNAMEG` are namespace properties, so the capability design
survives intact. On iOS the wasm sandbox returns the memory isolation for free.

---

## 5. WebAssembly as the execution substrate

### 5.1 `exec` is instantiate

Unix's `exec` maps an `a.out`; this one is `WebAssembly.instantiate` over bytes named by a
path in the caller's namespace. **The engine compiles on every process start**, so a freshly
produced `.wasm` is indistinguishable from a shipped one — which makes the toolchain question
one of convenience, never of capability.

| | Browser | Native (iPadOS / macOS) |
|---|---|---|
| Engine | the host's, **JIT** — WKWebView's JavaScriptCore has JIT because it runs out-of-process | an interpreter you own; **no JIT** |
| Speed | near-native | wasm3's docs: 4–15× slower than native, ~12.5× on CoreMark |
| Candidate | the host engine | **WasmKit** — pure Swift, iOS 12+, interpreter, depends only on swift-system |

An interpreter executes wasm *as data*, so it runs bytes produced a millisecond ago with no
entitlement.

WasmKit's own README (read 2026-08-26) settles what the native interpreter can carry: "a
standalone and embeddable WebAssembly runtime (virtual machine) implementation and related
tooling written in Swift", **exception handling and threads/atomics both supported since
v0.3.0**, tail calls since v0.1.4, platforms "macOS 10.13+, iOS 12.0+", core depending only
on swift-system. GC and multiple memories are unsupported; neither is needed here. The
consequence that matters: the fork-resume mechanism below relies on exception handling,
**and the native engine already has it**.

### 5.2 `fork` — the one genuinely unsolved thing

A wasm process is five pieces of state and four are copyable from the supervisor:

| State | Where it lives | Copyable? |
|---|---|---|
| Code | `WebAssembly.Module` | **Yes** — structured-cloneable, `postMessage` to a Worker |
| Heap, `.data`, **and the C shadow stack** | `Memory.buffer` (`ArrayBuffer`) | **Yes** — a byte copy |
| Mutable globals, incl. `__stack_pointer` | `WebAssembly.Global` | **Yes**, if exported; *not* in linear memory |
| Table (funcrefs) | `WebAssembly.Table` | **Yes** — per slot |
| **Value stack, locals, return-address chain** | the engine's own stack | **No** |

Clang maintains a downward-growing shadow stack in linear memory with `__stack_pointer` as
its ABI stack register (`wasm-ld --stack-first`, `-z stack-size`, typically 64 KB), so a
`memcpy` already carries everything address-taken. **What is stranded is the continuation.**

**The platform will not fix this.** The stack-switching proposal (WasmFX, stage 2 since
August 2024, implemented in Wasmtime) names the exclusion in its own explainer:

> some applications such as backtracking, probabilistic programming, and **process
> duplication** exploit multi-shot continuations, none of the critical use cases require
> multi-shot continuations

and adds that a compiler "should make sure that every continuation is used linearly". JSPI
suspends one stack; it does not duplicate one.

**Two mechanisms remain.**

**Lazy fork — `rfork(RFPROC|RFMEM)`.** The child *declares* it shares memory:

1. Child calls it → traps to the supervisor.
2. Supervisor suspends the parent, creates a child record sharing the parent's instance,
   returns `0`. The call returns **once**, into what is now the child.
3. Child runs, mutating shared memory.
4. Child `exec`s → supervisor builds a new instance, returns the child pid into the original
   one, resuming it as the parent.

**No asyncify in this path** — the child never reconstructs a stack, because it *is* the
stack with a different return value. The parent's stack restores exactly for a 64 KB copy of
the bounded shadow-stack region. The residual hazard is vfork's, with half of it already
gone: vfork was dangerous because Unix kept fds, signal dispositions and cwd *in the process
image*, and here they are in the kernel and the namespace.

**The resume mechanism, named, measured — and bounded.** Step 4 hides the real question:
the parent's continuation is engine frames, and on a JS engine nothing *outside* wasm can
unwind to a chosen frame. Exception handling can, from inside. The EH proposal, verbatim:
`try_table` "catches foreign exceptions generated from calls to function imports as well,
including JavaScript exceptions", excluding only traps and "JavaScript exceptions generated
from stack overflow and out of memory"; the catch_all forms "catch any exception, so that
they can be used to define a *default* handler". So:

1. libc calls through a **guard** — a hand-assembled wasm function whose body is
   `try_table (catch_all) call $rfork_raw` — passing `__builtin_frame_address(0)`, which is
   the shadow-stack pointer (measured, §9.4).
2. The raw import saves the scribble region `[0, sp)` host-side, and the child runs —
   mutating shared memory, vfork's discipline, declared by `RFMEM`.
3. At the child's `exec` (or `exits`), the supervisor sets up the new image elsewhere,
   restores `[0, sp)`, and the import **throws**. The throw unwinds the child's frames —
   which are dead, their new life being in the new instance — and `catch_all` stops it at
   the guard, which returns the child's pid. The parent's frames above the guard were never
   touched.

**The bound, found by building it:** the catch frame exists only while the guard has not
returned — so a guard that returns 0 to let the child run has already destroyed the thing
that resumes the parent. The child's pre-exec code must therefore run **inside the guard's
dynamic extent**, as a function the guard's import calls back into. That is not a new API —
it is Plan 9's own thread library shape, `procrfork(void (*fn)(void*), void *arg, …, int
rforkflag)` — and the PoC's libc exposes exactly that: `procrfork(flags, fn, arg)`. Bare
dual-return `rfork(RFPROC)` on a JS engine remains what this section always said the
general case was — asyncify's job — and the PoC's kernel refuses it with an error saying
so. On the native interpreter the restriction can lift: the engine's frames are owned and
host-side copyable (§4's 9vx lineage), and WasmKit carries EH regardless (§5.1).

The one-shot restriction is never violated: nothing is resumed twice. One continuation is
*returned into* twice, with two different values — which is what `vfork` always was.

Measured on this machine with hand-encoded modules, then end-to-end in the PoC
([poc/](poc/), whose acceptance suite forks every rc pipeline stage and every command
substitution this way):

| Engine | legacy `try`/`catch_all` | `try_table` + `catch_all` | JS throw from an import |
|---|---|---|---|
| Node v22.23.2 (V8 12.4) | rejected at compile | validates | **caught by `catch_all`** |
| Node v24.19.0 (V8 13.6) | rejected at compile | validates | **caught by `catch_all`** |

The standardised encoding is the one that works and the legacy one is already gone — the
opposite of what folklore expects. Emit `try_table`.

JSPI, for the record: shipped in Chrome 137, flagged in Firefox 139, in Safari Technology
Preview, Phase 4 since April 2025 — real, and still not this, because it suspends one stack
rather than unwinding into one. The transport in §5.3 needs none of it.

**Asyncify — for children that do not exec.** Asyncify's saved state is **a data structure in
linear memory** (two `i32`s bounding a stack of spilled frames), which is what defeats the
one-shot restriction: *a wasm continuation cannot be resumed twice, but bytes in an
`ArrayBuffer` can be copied as often as you like.* Unwind the parent to linear memory, copy,
rewind both instances, return pid to one and 0 to the other. This is what WASIX does — its
`proc_fork` doc says the child "starts from the same point as the parent process, including
the call stack, registers, and program counter", and the announcement credits
`setjmp`/`longjmp` to "`asyncify` wizardy".

Realised in the PoC (2026-08-26): `wasm-opt --asyncify` with
`--pass-arg=asyncify-imports@env.forka`, so instrumentation is confined to call paths that
can reach the bare-fork import. Measured on rc, the largest guest: **15,819 → 16,695
bytes, +5.5%** (transformed with `-O2` in the same pass), against the folklore below. The
worker's dance is unwind → snapshot the whole linear memory → post it to the supervisor,
which spawns a fresh Worker over the copy → both sides rewind, pid into one, 0 into the
other. rc's subshells run this way — a bare-forked copy of the interpreter — while its
pipeline stages stay on the guard path, which is the per-binary, per-call-site cost
placement the design asked for.

Cost, from Emscripten's own docs: **"something like 50%"**, and **"no worse than double size
/ halve speed for most code"** — and the whole-program analysis that keeps it low **is
defeated by indirect calls**, which a Unix userland is dense with. So it is a **per-binary
build flag, never system-wide**.

WASIX is also not a standard and will not become one: the Bytecode Alliance's co-founder
states the alliance "doesn't promote non-standard system interfaces that exist for
WebAssembly, such as… WASIX", and only Wasmer implements it. The *mechanism* (asyncify) is
portable; the *implementation* is not.

---

### 5.3 The syscall transport: a Worker is a process

A hosted kernel needs guests that can **block** in a system call without asyncify. The web
platform's own answer is threads: one Worker per process, one small `SharedArrayBuffer`
mailbox per process, and `Atomics`.

- Guest side: the syscall import writes trap and arguments to the mailbox and calls
  `Atomics.wait` — Workers may block.
- Supervisor side: the kernel replies with `Atomics.store` + `Atomics.notify` and itself
  never blocks. (MDN: `Atomics.waitAsync` "is non-blocking and, unlike `Atomics.wait()`,
  can be used on the main thread" — Baseline since November 2025 — though a
  `postMessage`-woken supervisor does not even need it.)
- A blocked call is simply a mailbox the kernel has not answered yet — `await`, a read on
  an empty queue, `sleep` — a kernel's sleep/wakeup with the host scheduler as scheduler.
  Preemption falls out too: a runaway guest is a Worker, and the supervisor can terminate
  a Worker.

Platform constraints, verbatim from MDN: "To use shared memory your document must be in a
secure context and cross-origin isolated" — the COOP/COEP headers, a browser deployment
requirement absent in Node — and shared wasm memory is the same object: "the backing buffer
of the Memory object is a SharedArrayBuffer". Note what is *not* required: the guest's own
linear memory need not be shared for any of this. The mailbox is a plain SAB and the Worker
owns its instance, so guest binaries need neither atomics nor shared-memory flags; sharing
guest memory with the supervisor is an optimisation, not a requirement.

---

## 6. WASI is a shim, not the system interface

`wasi-filesystem`'s README names this project's founding premise as a stated non-goal:

> **"WASI filesystem is not intended to be used as a virtual API for accessing arbitrary
> resources. Unix's 'everything is a file' philosophy is in conflict with the goals of
> supporting modularity and the principle of least authority."**

And the API bears it out. `types.wit` defines eight descriptor types — `unknown`,
`block-device`, `character-device`, `directory`, `fifo`, `symbolic-link`, `regular-file`,
`socket` — and **no `chmod`, no `chown`, no `ioctl`, no `mknod`**: it can *recognise* a
character device and cannot *create* one. Settable metadata is exactly size, atime, mtime.

**No processes, ever, so far.** The full proposal list by phase:

| Phase | Proposals |
|---|---|
| 5, 4 | *(none)* |
| **3** | Clocks · Random · Filesystem · Sockets · CLI · HTTP |
| 2 | Timezone · HTTP variants · I2C · Key-value · ML · Runtime Config · WebGPU · Messaging |
| 1 | Blob Store · Crypto · GPIO · Distributed Lock · Logging · Observe · Parallel · Pattern Match · SPI · SQL · SQL Embed · **Threads** · TLS · URL · USB · OTel |
| 0 | proxy-wasm/spec |

**Nothing at any phase for processes, spawning, fork, exec, signals, job control, tty or
device nodes.** Threads is Phase 1, and future thread work moves to `shared-everything-threads`
— "still in the early stages of development and is not yet available in any WASI host
runtime". The wasi.dev roadmap's 0.3.x plan lists "Threads (cooperative first, then
preemptive)" — threads, not processes.

**WASI 0.3** (released **2026-06-11**, Wasmtime 43+ and jco) removed `wasi:io` entirely and
absorbed it into the Canonical ABI as `async func`, `stream<T>` and `future<T>`; "The
runtime, not each component, drives the scheduling", accommodating both stackful and
stackless coroutines. Useful — the supervisor's blocking calls become an unresolved
`async func` rather than a fake — but it is suspend/resume, not a duplicable stack. WASI 1.0
is targeted late 2026 / early 2027.

**And the component model competes with "everything is a file".** Its thesis is *typed*
interfaces (WIT; `stream<T>` generic in 0.3 where 0.2 had only `stream<u8>`); this project's
is a *uniform untyped* one. They do not compose: a server exporting a WIT interface forfeits
the property that makes 9P worth having — that any client works with any server, and `cat`
works on a network connection. **So file servers are plain wasm binaries speaking 9P over a
byte stream, and WASI's role is `wasi:cli/command` and nothing else.**

---

## 7. The GUI: rio-shaped, so `sam` and `acme` work

The requirement is **the interface those programs open**, not rio. Window policy — placement,
menus, tiling versus floating — is free.

From `rio(4)`: **"A mount of `$wsys` causes rio to create a new window; the attach specifier
in the mount gives the coordinates of the created window."** Rio serves into each client's
namespace: `cons` · `consctl` · `cursor` · `label` · `mouse` · `screen` · `snarf` · `text` ·
`wctl` · `wdir` · `winid` · `window` · `wsys`.

`/dev/draw` is likewise a file protocol. From `draw(3)`: a client opens `/dev/draw/new` and
reads **twelve 11-character strings** — connection number, image id of the display image
(always zero), channel format, and the min.x/min.y/max.x/max.y of both the display image and
the clipping rectangle — then writes single-letter binary messages to `data`, **low-order
byte first**:

| msg | operation | backend |
|---|---|---|
| `b` | allocate image (channel format, refresh method) | texture / offscreen canvas |
| `d` | combine rectangles with the draw operator and alpha mask | blit |
| `L` | line, with thickness and endpoints | geometry, or rasterise in the server |
| `e` / `E` | ellipse / arc | geometry, or rasterise |
| `s` / `x` | cached-font text | glyph atlas |
| `y` / `Y` | replace pixels, uncompressed / compressed | texture upload |

### The thing plan9port could not do

plan9port abandoned this: `devdraw` is a separate binary with X11 and Cocoa backends (now
`CAMetalLayer`) that libdraw talks to directly, **not over 9P** — because Unix has no
per-process namespaces, so no client can have its own `/dev/draw`.

**This project has them by construction**, so `/dev/draw` can be an actual file, per window,
per namespace. It is the one place this system can be *more* faithful to Plan 9 than the
official port, and it costs nothing extra.

**Order of clients: `sam` first** — needs libdraw and libframe and nothing else, and
plan9port's is MIT. **Then `acme`** — the real test, because acme is *itself a file server*
(it serves `/mnt/acme`), so it exercises the namespace in both directions. If acme works,
the design works.

---

## 8. Storage

One interface, per-platform backing:

- **Native** — the host filesystem.
- **Browser** — **OPFS** via `@zenfs/dom`. ZenFS is BrowserFS's successor: the `browserfs`
  npm packages are deprecated and republished under `@zenfs`; `@zenfs/dom` supplies
  WebAccess (File System Access / OPFS), IndexedDB and WebStorage backends, and all ZenFS
  backends support synchronous operation.

Two hard constraints, not preferences:

1. **`createSyncAccessHandle()` is Worker-only** — not exposed on the main thread. The
   storage server must live in a Worker. (Architecturally correct anyway: it *is* a separate
   task.)
2. **iOS Safari evicts aggressively** and `persist()` is harder to obtain there.

---

## 9. The toolchain

### 9.1 Building the kernel

**clang, from Xcode** — and **Harvey OS** demonstrates this exact thing: "an effort to get
the Plan 9 code working with gcc and clang", with ELF64 and standard ABIs, plus **APEX** (APE
reimplemented over musl code). "You can compile in Linux (or Mac, or BSD) and run into
Harvey."

### 9.2 But kencc is not ANSI, and the userspace is written in it

From *How to Use the Plan 9 C Compiler*:

- **`extern register`** — "will dedicate a register to a variable on a global basis… External
  register variables must be identically declared in all modules and libraries." Used for the
  current-process pointer. clang has no equivalent.
- **Anonymous struct/union members** — "**the most important and most heavily used of the
  extensions**… If an anonymous structure or union is declared within another structure or
  union, the members of the internal structure or union are addressable without prefix in the
  outer structure."
- **The preprocessor "does not support `#if`"**, though it handles `#ifdef` and `#include`.

Harvey's answer was to port the source; **`goken9cc`**'s is to keep kencc and add targets —
Plan 9 → Inferno → the Go repo (forked October 2010) → this, carrying eight architectures on
single letters including an experimental **`e` for WebAssembly** (`ea`/`ec`/`el`). Its README
states that back end was AI-written and the release in progress, so treat it as **evidence
the retarget is tractable, not a component to depend on.**

### 9.3 A guest toolchain, if wanted

Three answers, not exclusive, and self-hosting is **not a goal**:

1. **Move the toolchain in** — clang and `wasm-ld` compiled to wasm (~30 MB packaged; Wasmer
   ships this and reports WASIX self-hosted "meaning it can compile itself and any C
   programs"; wasm3 compiles *itself* this way). Costs size; buys no host dependency, which
   is what makes a browser build self-contained.
2. **Move the namespace out** — Plan 9's `cpu(1)`: **"The name space of the terminal side of
   the *cpu* command is mounted, via *exportfs*(4), on the CPU side on directory
   /mnt/term."** Native speed; needs 9P running **both directions**; has no browser.
3. **Make the compiler a file server** — `/cc`, on the `/net` pattern. The only one that
   makes compilation a **capability**: a process rforked with `RFCNAMEG` or `RFNOMNT` cannot
   compile, because the name does not resolve. Makes 1 and 2 implementation details.

### 9.4 Measured on this machine (2026-08-26)

Apple's clang has no wasm backend — `clang --print-targets` under Xcode's 21.0.0 lists zero
wasm entries — so the guest toolchain is **wasi-sdk-34** (released 2026-08-25), installed at
`~/.local/opt/wasi-sdk`, whose clang lists `wasm32`/`wasm64`. It is used freestanding, per
§6's verdict on WASI: `--target=wasm32 -nostdlib`, imports and exports declared with
`__attribute__((import_module, import_name))` and `export_name`, linked with lld's wasm
port — "--no-entry: Don't search for the entry point symbol (by default `_start`)",
"--import-memory: Import memory from the environment", "--stack-first: Place stack at start
of linear memory rather than after data" — plus `-z stack-size=65536`. The probe binary
imports exactly `env.memory` and `env.sys`, and `__builtin_frame_address(0)` returns the
shadow-stack pointer (65472, one frame below a 64 KB stack top), which is what lets the
fork guard receive `sp` as an ordinary argument instead of exporting the `__stack_pointer`
global. The WASI sysroot is present and deliberately unused.

---

## 10. Licensing

- **Plan 9** — Nokia Bell Labs transferred the copyright to the **Plan 9 Foundation** on
  23 March 2021, which relicensed all previous editions under the **MIT licence**. plan9port
  carries the same terms.
- **Research Unix** — Nokia's 2017 covenant, as reasoned about in the parent repository.
- **APE** — ships with Plan 9, so MIT under the same transfer. It is a source to cut down,
  not a thing to write fresh.
- **9front** — "All of 9front is now provided under the MIT License unless otherwise
  indicated", its additions under the MIT licence reproduced in `/lib/legal/mit` — so
  consulting or borrowing from the maintained fork raises no new estate question.
- **Inferno** — irrelevant here. Its VM is replaced by wasm and its protocol half is 9P; the
  Lucent → Vita Nuova GPLv2/MIT estate never has to be resolved.

A V12 image mixing the Plan 9 and Research Unix estates is tractable but must be answered
before code, not after.

---

## 11. Risks and open questions

**Resolved 2026-08-26** (reasoning in the sections cited; decisions recorded in the plan):

- **The kernel call list is derived** — call by call, in
  [docs/syscalls.md](docs/syscalls.md): Plan 9's 52 slots dispositioned for V12, and V10's
  68 routines mapped onto them.
- **9P2000** (§2). One defined version, negotiation built in, and nothing here needs wire
  compatibility with original 9P.
- **The Plan 9 base is the 4th edition as reference, 9front consulted for fixes** — both
  estates are MIT (§10), the 4th edition is what every citation here already reads, and the
  kernel is transcribed structure, not a forked tree.
- **`fork`'s lazy path has its mechanism and its bound** (§5.2) — exception-unwind to a
  live guard frame, which means the `procrfork(fn)` shape rather than bare dual-return
  `rfork(RFPROC)` on a JS engine — proven end-to-end in [poc/](poc/); the syscall transport
  (§5.3) makes blocking calls ordinary.
- **Two guest substrates or one? One.** Wasm on every platform; §5.3's transport is the
  same design in the browser and in Node, and WasmKit carries EH and threads for the native
  interpreter (§5.1).
- **`/dev/tty`: there is none.** The console is `/dev/cons`, per Plan 9; the V10
  personality's libc aliases `/dev/tty` to it, and the fd-3 accident stays in the parent
  repository's notebook as history, not design.

**Still open:**

- **The uid model.** APE called it impossible to simulate. It is the single item that
  decides whether V10 compatibility is real or approximate, and it is design work, not
  research: the hosted kernel *can* own per-process credentials and stamp them on every
  attach, which is exactly what Plan 9's kernel devices do with `up->user` — the question
  is the model, not the mechanism. Hard links and `symlink`/`readlink`/`lstat` sit in the
  same design because all of them need protocol room (9P2000.L's `Tlink`/`Tsymlink` prove
  the extension is expressible; whether to adopt or mint is part of the task).
- **kencc or clang for the ported userspace**, given `extern register` and anonymous struct
  members. Fresh code is clang (§9.4 is the measured recipe); the question is Plan 9's own
  source.
- **Does the `d` message's alpha compositing map cleanly onto canvas and Metal**, or does
  the window server rasterise? Decides whether the backend is thin.
- **Heap size.** Narrowed but open: an unshared guest memory may `memory.grow` freely, and
  the PoC takes that path; a *shared* memory must declare its maximum up front, so the
  moment guest memory is shared with the supervisor for zero-copy I/O, sizing becomes
  policy.

---

## Appendix: primary sources

**Plan 9** — [design paper](https://9p.io/sys/doc/9.html) ·
[`intro(2)`](https://9p.io/magic/man2html/2/intro) ·
[syscall numbers](https://raw.githubusercontent.com/0intro/plan9/master/sys/src/libc/9syscall/sys.h) ·
[`rfork(2)`](https://9p.io/magic/man2html/2/fork) ·
[`rio(4)`](https://9p.io/magic/man2html/4/rio) ·
[`draw(3)`](https://9p.io/magic/man2html/3/draw) ·
[`cpu(1)`](https://9p.io/magic/man2html/1/cpu) ·
[`a.out(6)`](https://9p.io/magic/man2html/6/a.out) ·
[APE](https://9p.io/sys/doc/ape.html) ·
[Plan 9 C Compilers](https://9p.io/sys/doc/compiler.html) ·
[How to Use the Plan 9 C Compiler](https://9p.io/sys/doc/comp.html) ·
[Adding Application Support for a New Architecture](https://9p.io/sys/doc/libmach.html) ·
[Other hardware — the lost VAX compiler](https://9p.io/wiki/plan9/Other_hardware/index.html) ·
[System requirements](https://9p.io/wiki/plan9/system_requirements/index.html)

**Hosted implementations** — [9vx](https://swtch.com/9vx/) ·
[9VX wiki](https://9p.io/wiki/plan9/9vx/index.html) ·
[Vx32 (USENIX '08)](https://pdos.csail.mit.edu/papers/vx32:usenix08.pdf) ·
[Inferno ports: hosted and native](http://doc.cat-v.org/inferno/4th_edition/inferno_ports) ·
[Inferno `intro(1)`](https://inferno-os.org/inferno/man/1/0intro.html) ·
[Harvey OS / APEX](https://github.com/Harvey-OS/apex/wiki) ·
[plan9port](https://9fans.github.io/plan9port/) ·
[plan9port `devdraw`](https://9fans.github.io/plan9port/man/man1/devdraw.html)

**WebAssembly** — [WasmFX explainer](https://wasmfx.dev/specs/explainer/) ·
[WasmFX](https://wasmfx.dev/) ·
[Continuing WebAssembly with Effect Handlers](https://arxiv.org/pdf/2308.08347) ·
[Binaryen's Asyncify](https://kripken.github.io/blog/wasm/2019/07/16/asyncify.html) ·
[Emscripten: Asynchronous Code](https://emscripten.org/docs/porting/asyncify.html) ·
[LLVM D46141 — `--stack-first`](https://reviews.llvm.org/D46141) ·
[LLVM D101140 — wasm local variables](https://reviews.llvm.org/D101140) ·
[WasmKit](https://github.com/swiftwasm/WasmKit) ·
[wasm3 performance](https://github.com/wasm3/wasm3/blob/main/docs/Performance.md) ·
[structured control flow](https://labs.leaningtech.com/blog/control-flow) ·
[Beyond Relooper (Tufts)](https://www.cs.tufts.edu/~nr/pubs/relooper.pdf)

**WASI** — [proposals and phases](https://github.com/WebAssembly/WASI/blob/main/docs/Proposals.md) ·
[roadmap](https://wasi.dev/roadmap) · [0.3 release](https://wasi.dev/releases/wasi-p3) ·
[Bytecode Alliance: WASI 0.3 launched](https://bytecodealliance.org/articles/WASI-0.3) ·
[WASI 0.2 launched](https://bytecodealliance.org/articles/WASI-0.2) ·
[migrating 0.2 → 0.3](https://component-model.bytecodealliance.org/design/migrating-to-p3.html) ·
[wasi-filesystem](https://github.com/WebAssembly/wasi-filesystem) ·
[wasi-filesystem `types.wit`](https://github.com/WebAssembly/wasi-filesystem/blob/main/wit/types.wit) ·
[shared-everything-threads](https://github.com/WebAssembly/shared-everything-threads) ·
[WIT reference](https://component-model.bytecodealliance.org/design/wit.html) ·
[Empowering WebAssembly with Thin Kernel Interfaces (WALI)](https://arxiv.org/html/2312.03858v3)

**WASIX** — [`proc_fork`](https://wasix.org/docs/api-reference/wasix/proc_fork) ·
[Announcing WASIX](https://wasmer.io/posts/announcing-wasix) ·
[Bytecode Alliance's objection](https://www.infoworld.com/article/2338660/wasix-undermines-webassembly-system-interface-spec-bytecode-alliance-says.html) ·
[Clang in the browser](https://wasmer.io/posts/clang-in-browser)

**Toolchains** — [goken9cc](https://github.com/aryx/goken9cc) ·
[Go Wiki: WebAssembly](https://go.dev/wiki/WebAssembly) ·
[Go's Plan 9 lineage](https://go.dev/wiki/Plan9) · [QBE](https://c9x.me/compile/) ·
[cproc](https://sr.ht/~mcf/cproc/)

**Browser platform** — [Wanix](https://github.com/tractordev/wanix) ·
[wanix = webassembly + unix -> plan9 in the browser](https://groups.google.com/g/Golang-Nuts/c/Dvk6g8jcRfE) ·
[ZenFS](https://zenfs.dev/core/) ·
[OPFS](https://web.dev/articles/origin-private-file-system) ·
[`createSyncAccessHandle()`](https://developer.mozilla.org/en-US/docs/Web/API/FileSystemFileHandle/createSyncAccessHandle) ·
[xterm.js](https://github.com/xtermjs/xterm.js) ·
[`@xterm/addon-webgl`](https://github.com/xtermjs/xterm.js/blob/master/addons/addon-webgl/README.md) ·
[iOS sandbox: no child processes](https://developer.apple.com/forums/thread/747499) ·
[WKWebView JIT](https://news.ycombinator.com/item?id=40726948)

**Licensing** — [Plan 9 copyright to the Plan 9 Foundation, MIT (2021)](https://www.phoronix.com/news/Plan-9-2021) ·
[The Register's account](https://www.theregister.com/2021/03/24/bell_labs_transfers_plan9pto_foundation/)

**Engineering findings (2026-08-26)** —
[exception handling proposal](https://github.com/WebAssembly/exception-handling/blob/main/proposals/exception-handling/Exceptions.md) ·
[legacy EH proposal](https://github.com/WebAssembly/exception-handling/blob/main/proposals/exception-handling/legacy/Exceptions.md) ·
[JSPI (V8 blog)](https://v8.dev/blog/jspi) ·
[MDN SharedArrayBuffer](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/SharedArrayBuffer) ·
[MDN Atomics.waitAsync](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Atomics/waitAsync) ·
[lld's WebAssembly port](https://lld.llvm.org/WebAssembly.html) ·
[wasi-sdk releases](https://github.com/WebAssembly/wasi-sdk/releases) ·
[`version(5)`](https://9p.io/magic/man2html/5/version) ·
[9front FQA](https://fqa.9front.org/)
