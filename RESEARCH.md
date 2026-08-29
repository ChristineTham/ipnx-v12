# A hosted Plan 9 with a Research Unix personality — feasibility study

*Begun 2026-08-26 and **living** — the evidence base behind
[docs/design.md](docs/design.md). Findings land here as they are established, with their
provenance; decisions and scope stay in the plan.*

*It is written to be self-contained. Measurements taken against the Research Unix V10 tree
were made in the parent repository ([ipnx](https://github.com/ChristineTham/ipnx)) and are
recorded here as data, because that tree is deliberately **not** copied into this one.*

## TL;DR — the recommendation

Build a **modified Plan 9 kernel that runs as an ordinary userspace process** on macOS,
iPadOS and in the browser, executing **WebAssembly** binaries in a per-process namespace,
and add a **Unix personality** on top. *(Re-founded 2026-08-27, decision log: the
personality is a **modern** Unix surface — derived by measurement against git, CPython
and Go, not adopted from POSIX — plus a WASI second ABI; the Tenth Edition personality
became the V10 exhibit, kept but no longer grown. The kernel findings below are
untouched by the re-founding.)*

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
processes. *(2026-08-27: WASI's role widened from `wasi:cli/command` to a full second
guest ABI — `wasi_snapshot_preview1` as a syscall dialect over the same chans, preopens
mapping onto per-process binds — which is what carries Go `wasip1` binaries and
CPython's official wasi builds. The finding stands unchanged: a dialect is not an
interface; the system interface is 9P.)*

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
- **The uid family, measured 2026-08-26** (closing docs/identity.md's D1–D4):
  - `os/sys4.c:74` `setuid`: permitted when `u.u_ruid == uid || u.u_uid == uid || suser()`,
    sets `u_uid`, `p_uid` **and** `u_ruid` — and on denial it **silently does nothing**
    (no `u_error`), V7's manner. `sys4.c:97` `setruid`: `suser()` only.
  - `os/sys4.c:90` `getuid`: `r_val1 = u.u_ruid; r_val2 = u.u_uid` — **one trap returns
    both ids in two registers**, and `libc/sys/getuid.s` holds the pair: `_getuid` takes
    `r0`, `_geteuid` is the *same* `chmk $getuid` followed by `movl r1,r0`. The
    personality's `geteuid` is a register pick, not a second call.
  - `sys/sys/param.h:12`: `NGROUPS 32`, groups are `short`s ending at a `NOGROUP`
    sentinel; `sys4.c:132` `setgroups` is `suser()`-only with `EINVAL` past the array.
  - `os/sys4.c` `chown1` + `os/fio.c:203` `accowner`: the gate is owner-or-root, but a
    **non-root owner cannot give a file away** — `chown1` requires `ip->i_uid == uid` and
    a member gid for non-root, so only root changes a file's uid (and a gid change clears
    `ISGID` unless `ICONC`). The PoC's eve-only chown was already V10's rule.
  - `os/iget.c:314` `maknode`: `i_mode = IFREG|(0666 & ~u.u_cmask)`, `i_uid = u.u_uid`,
    `i_gid = u.u_gid` — a created file takes the **creator's** effective ids, not the
    directory's.

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

**Measured in the browser (2026-08-26, Chrome 148).** The PoC's kernel runs unmodified in
a page — one platform-neutral `kernel.mjs`, thin Node and browser hosts — and the full
acceptance suite passes there identically: lazy fork, asyncify fork, wire-9P mounts and
exportfs, union directories, rc with subshells. Two divergences Node hides, found by running: a browser
`TextDecoder` **refuses views over a `SharedArrayBuffer`** ("The provided ArrayBufferView
value must not be shared") where Node's decodes them — copy shared views before decoding —
and `new WebAssembly.Module` is size-restricted on the browser main thread, so the kernel
compiles with `await WebAssembly.compile` everywhere, which costs Node nothing.

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

### The browser precedent: Wanix, and Apptron on top of it

The nearest living relative arrived while this was being designed. Wanix is "an
embeddable runtime that brings a Unix-like environment to the browser", and its own
description is this project's §2 restated: "Everything is a file. Processes, terminals,
VMs, browser APIs, and storage are exposed through a unified namespace you compose with
binds. The same idea as Plan 9, with improvements, in the browser." Its devices are
`#`-named (`#task`, `#term`, `#ramfs`, `#web` — the last exposing OPFS, DOM, workers and
caches *through the filesystem*), "each task gets its own namespace", and namespaces
federate across origins — "import a remote Wanix namespace via WebSocket (ws:// / wss://)
or iframe + 9P". Apptron, built on it, is the desktop: terminals are custom elements
("render an xterm.js terminal connected to a Wanix terminal device"), whole environments
embed as elements, and Alpine Linux runs in v86 when x86 compatibility is wanted.

What it corroborates: per-task namespaces, `#device` naming, binds, and 9P at the
boundary all *work in a page*, and windows-as-elements-backed-by-namespaces is a
practical shape for the rio-interface window server — each window an element whose
`cons`, `mouse` and `draw` are files in that window's namespace. What it does not carry:
Wanix's wasm tasks are WASI/Go-shaped with no fork story, and Apptron's Unix is an
emulated x86 Linux — the two places this project's substrate decisions (Plan 9 calls,
the fork mechanisms of §5.2) do the work instead.

**Realised in the PoC (2026-08-26), as a subset.** `#w` mints windows from `clone`;
`bind '#w/N' /dev` in a namespace copy gives a process its own `cons`, `mouse`, `wctl`,
`label` — and a real `draw/` tree: `new` answers the twelve 11-character fields of
draw(3), `data` accepts `b d f L e E v` (low-order byte first), and the engine rasterises
into a per-window RGBA image the host presents (a canvas element in the browser; a
headless buffer on Node, where the acceptance tests assert pixels through the namespace).
`win cmd` reproduces rio's spawn in forty lines, and `win rc` is a working shell in a
window — typed at through focus, its output through its own `/dev/cons`. Verified in
Chrome by synthesising pointer events against `scribble` and reading the inked pixels
back off its canvas. Text landed next (2026-08-26, same day): `y` uploads pixels, `i`
declares a font cache with its ascent, `l` loads glyph slots, and `s` draws strings as
alpha-masked blits — carrying an 8×8 font of this project's own authorship
(`poc/libc/font8x8.h`; no font data is copied in). Glyph strokes, gaps and advances are
asserted by pixel in the headless suite. What `sam` now waits on is not the device but
the port: libframe's line-editing over exactly these messages.

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

Container and microVM backing — the no-on-disk-format invariant, volumes as
namespace scripts, 9P over virtio-9p or vsock — is decided and specified in the
plan's decision log (2026-08-27); this section stays the platform-backing survey.

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

Three findings from growing the real userspace (2026-08-26), each measured the hard way:

- **`-fno-builtin` is load-bearing when the source IS the libc.** clang's libcall
  recogniser rewrote `strchr(s, 0) - s` *inside* `port/strlen.c` back into a call to
  `strlen` — self-recursion the optimiser then collapsed into "return an uninitialised
  local", so every `strlen` returned 0 and argv assembly degraded into per-byte
  suffixes. The same recogniser had earlier turned a bare `fprintf` into `fwrite`.
- **`memory.grow` detaches the old buffer.** A cached `Uint8Array` view over guest
  memory went stale the moment rc's heap crossed the linked initial size; the throw
  surfaced *inside the fork guard's extent*, was swallowed by `catch_all` as a foreign
  exception, and the guard returned a stale pid — a hang two causes removed from its
  symptom. Views are now refreshed against `memory.buffer` identity on every use.
- **Browsers budget wasm memory per tab, and terminated Workers return it lazily.**
  ~100 allocate-and-abandon guest Workers exhausted Chrome's budget
  (`WebAssembly.Memory(): could not allocate memory`) even at small maxima. The kernel
  now pools retired Workers — a retired guest zeroes its memory and waits for the next
  image — and caches compiled Modules per file node, which also made the in-page suite
  several times faster.

The kencc-vs-clang question now has measured data at both ends of the timeline: real
**4th-edition** sources (`cat.c`, `echo.c`) compile **unmodified** against two shim
headers, and real **V10** sources — K&R definitions, implicit ints, implicit function
declarations — compile with `-std=c89 -fno-builtin` plus three warning suppressions
(`-fno-builtin` matters: the libcall optimiser rewrote a bare `fprintf` into `fwrite`).
`-fms-extensions` now carries kencc's anonymous members in earnest — `Biobuf` embeds
`Biobufhdr` unnamed, and `-Wno-incompatible-pointer-types` reproduces kencc's implicit
`Biobuf*`→`Biobufhdr*` conversion (sound: the header is the first member). grep's yacc
grammar regenerates with the host's bison 2.3 at build time, which is the same door rc's
`syn.y` will use. Neither tree needed a source change.

### 9.5 What porting the real rc measured (2026-08-27)

The real rc — seventeen C files plus `syn.y` through bison, compiled verbatim, linked
against `libp9.a`, asyncified — runs the whole suite. Getting there surfaced five
findings, each measured on this machine, none requiring a source change:

- **wasm function "pointers" are small table indexes, and 1992 code assumes they are
  addresses.** rc's `codefree` walks compiled code arrays telling ops from operands by
  comparing `.f` slots against op function pointers, terminating at `.f == 0`
  (`code.c`). On every real machine op addresses and small operand integers (jump
  targets, fd numbers) occupy disjoint ranges; on wasm32 the linker numbers the
  function table from 1, so `Xpipe` was table slot 53 and a jump target of 53 derailed
  the walk into `efree` of a non-pointer — a heap corruption whose detonation depended
  on which integers a compiled line happened to contain. **`--table-base=4096`**
  restores the disjointness (operand ints stay far below 4096; string operands are heap
  addresses far above) at the cost of ~4K unused table slots per instance. The same
  hazard exists for any vendored code that compares function pointers against data.
- **Pre-ANSI common blocks need hand-restored semantics.** `rc.h` tentatively defines
  its globals in every TU while `plan9.c`, `havefork.c`, `lex.c`, `exec.c` and
  `getflags.c` carry the initialized definitions — and the initializing TUs tentatively
  define *each other's* symbols, so no link order satisfies first-wins. Measured:
  clang's wasm backend refuses `-fcommon` ("common symbols are not yet implemented for
  Wasm"); wasm-ld's `--allow-multiple-definition` keeps the **first** definition even
  when a later one is initialized (plan9.o's zero `havefork` beat `havefork.c`'s
  `= 1`, so `code.c` emitted forkless pipe layouts that the forking `Xpipe` then
  misread — the layout carries a *string* where the fork layout carries a pc);
  wasi-sdk's llvm-objcopy cannot rewrite wasm symbol tables; and tentative definitions
  appear as `D`, not `B`, in wasm objects' llvm-nm output. `poc/weaken.mjs` therefore
  patches the objects directly — OR-ing `WASM_SYM_BINDING_WEAK` (0x1) into the linking
  section's symbol flags never changes a LEB's length, so the patch is in-place — with
  an owner map keeping one strong copy per symbol. Weak-yields-to-strong then resolves
  correctly in any order.
- **Globals are not part of the fork snapshot.** The asyncify fork copies linear
  memory, but `__stack_pointer` is a wasm global: a child instantiated fresh starts at
  the stack top, and its post-rewind epilogues then walk the pointer into the data
  segment. The fork now carries the fork-time `__stack_pointer` value (exported via
  `--export-if-defined`) and both sides restore it before rewinding. Corollary,
  measured the hard way: re-instantiating a module re-runs its **active data
  segments** — a fresh instance over live memory wipes initialized globals, so the
  child's order (instantiate, then overwrite memory with the snapshot) is the only
  correct one.
- **Notes must not dispatch on the fork return.** The mailbox flags pending notes on
  every reply; guestcore then calls the guest's `__notedispatch`. On trap 19 the
  guard's `catch_all` frame is live in the parent, and a handler dying there would feed
  `ExecReplace` to `catch_all` as a foreign exception — the detach-bug class — so
  delivery is deferred to the next syscall.
- **`fd2path` returns 0 on success** (fd2path(2)), and rc's `Isatty` tests exactly
  that before checking the path ends in `/dev/cons`. The lib9 wrapper now masks the
  kernel's byte-count return; `win rc &` prompts because rc itself concludes it is on
  a console — the same inference it makes on Plan 9.

Porting the real `sam` (2026-08-27, same day) added three more:

- **Real `setjmp`/`longjmp` fall out of the fork machinery.** sam's error recovery
  longjmps to its main loop; a stub that exits is not an editor. setjmp is the fork
  parent's exact dance — unwind through asyncify, save the frame buffer and
  `__stack_pointer` host-side keyed by the `jmp_buf` address, rewind in place
  returning 0; longjmp unwinds (discarding its own frames), restores the saved
  buffer, and rewinds — the re-entered `setj` import returns the value at the
  original setjmp site. Two more listed imports (`env.setj`, `env.longj`), ~40 lines
  of guestcore. Uninstrumented binaries keep the old contract: their setjmp arms
  nothing (libregexp's bad-pattern bailout in `grep` never fires it), and a longjmp
  there is a named error.
- **`port/execl.c` cannot work on wasm**: `exec(f, &f+1)` assumes variadic arguments
  sit on the stack after the named one; clang's wasm ABI passes them through a
  separate buffer. The one excluded port file with a semantic (not float) reason —
  lib9 carries a `va_arg` execl.
- **`create(2)` on an existing file truncates and opens it** — the words in the
  manual carry rc's `> /dev/null`: the kernel's create now walks first and opens in
  place (devices without a create method included), reserving true creation for the
  missing-file and DMDIR cases.

And samterm (same day, the stack's top) measured four more:

- **libframe's entire text path is the `x` message** — string-with-background, `s`
  plus a background image and point after the index count (`ni` stays at +45; the
  bg fields follow, then the cache indices). A draw device without `x` renders sam
  with a moving cursor and invisible words.
- **kencc names unnamed members and mixes enum with int in prototypes**; GCC's
  `-fplan9-extensions` covers the former, clang has no such flag, and clang makes
  the latter a hard error. Both surface exactly twice in the whole tree
  (`samterm/io.c`'s `&mousectl->Mouse`, libdraw's `stringbg.c`), and both are
  handled as build-time derivations into `build/` — the same shape as bison over
  the yacc grammars, the vendored files untouched. `initmouse`/`initkeyboard`
  themselves are ours (`libc/mousekbd.c`), platform-IO like libthread.
- **Async reads must carry the channel's real offset** — devices like `draw/ctl`
  answer only at offset 0, so AREAD passing a stream's -1 read empty and
  `initdisplay` died parsing nothing. The completion still advances the offset.
- **`access(2)` is `stat` in disguise** (dirstat under AEXIST), so a device without
  stats fails `initdraw`'s probe and libdraw falls back to binding `#i`. The window
  server now answers stat for every node it serves.

And acme (same day, the proof of concept's close) measured five that generalise:

- **kencc converts a pointer-to-struct into a pointer to its unnamed member at
  call sites; clang passes it unadjusted** — a warning, not an error, and the
  worst kind of wrong. Acme's pervasive `frinsert(t, …)` hands a `Text*` to
  libframe's `Frame*`: libframe then wrote its fields over Text's head, and
  `t->file` *became the font pointer*. The corruption announced itself as one
  integer: `ntosize n=851983` = `(13<<16)|15` = `Font.ascent`/`height` read as
  a rune count through `Buffer.cnc` at offset 8. The fix is 15 lines of
  derivation header (`frameadjust.h`): `(Frame*)&(x)->font` is the adjusted
  pointer whether `x` is a `Frame*` (font is Frame's first member, +0) or any
  embedder (`-fms-extensions` resolves `x->font` to the embedded Frame's), so
  each libframe entry point becomes a function-like macro over it. Acme's own
  `xselect(t, mousectl, …)` — the same idiom, one internal function — is one
  sed. The finding generalises: every kencc program embedding structs will
  need its call sites audited, and the macro shape handles them without
  touching vendored text.
- **The float door is open** (the standing exclusion resolved): `atof`,
  `strtod`, `charstod`, `fltfmt`, `nan`, `pow10`, `frexp`, `ctype`, `toupper`
  compile verbatim once `u.h` defines the real little-endian `FPdbleword`
  union — wasm has native f64; the exclusion had only ever been about that
  union. `umuldiv` alone is ours (upstream it is per-arch assembler).
- **`/srv` must be a device, not a directory** — acme posts its error pipe's
  fd and reopens it via `/fd/N`; with `/srv` as ramfs the pipe's peer died at
  `close()` and `acmeerrorproc`'s `while(read() >= 0)` spun on EOF: 2,867
  empty warnings before the measurement. `#s` now captures the posted fd's
  **channel** under the name — the name holds a reference, the reference is
  the "potential writer" that makes a pipe reader park instead of seeing EOF,
  which is precisely srv(3)'s semantic content. The kernel's OPEN shares a
  posted channel the way `#d/N` does.
- **The default font can back a font(6) file**: acme dies without its named
  font (`openfont` failure is fatal in `geninitdraw`), so the rootfs carries
  `/lib/font/bit/lucidasans/euro.8.font` = `15 13␤0x0000 0x00FF *default*` —
  a legitimate font file whose one range resolves to the compiled-in default
  subfont. The metrics are measured from `defont.c`'s own data (256 glyphs,
  height 15, ascent 13), not assumed.
- **A browser rootfs must carry its empty directories** — the packer emitted
  files only, so `/srv` and `/mnt` existed on Node (directory seeding) and
  vanished in Chrome. Empty dirs travel as explicit `null` markers in
  `rootfs.json`.

The wasm libthread finished growing to acme's needs the same day: `procexec`
as fork-plus-exec with the runproc's self-`rfork` intercepted (`_threadrfork`
stashes fds 0–19 at 100–119 and **defers** `RFNAMEG|RFNOTEG|RFENVG` onto the
fork itself — the child gets the isolation the runproc meant for it, the
shared instance stays untouched; acme's *startup* `rfork(RFENVG|RFNAMEG)`
defers harmlessly because `win` already isolates the window namespace — the
recorded deviation), `threadwaitchan` as a nohang-await poller,
`threadnotify` as a handler chain over the note machinery, thread-aware
`sleep` via a `wakeat` field and the scheduler passing its nearest deadline
as the kernel `IOWAIT` timeout, and `Ref` as plain arithmetic (cooperative
scheduling is the lock). Mouse events cross the same boundary keyboard ones
do: wctl accepts `mouse x y buttons` with a really-advancing msec, because
double-click detection is msec arithmetic.

And the WASI second ABI, landing the same day the PoC closed, measured six:

- **The preopen is the namespace root, and no kernel change was needed.**
  `supervisor/wasi1.mjs` (484 lines) implements `wasi_snapshot_preview1`
  entirely over the existing mailbox traps: fd 3 is `/`, paths resolve
  shim-side (WASI is dirfd-relative and has no cwd) and walk kernel-side, so
  a foreign binary crosses symlinks, unions and mounts identically to a
  native one. Both citizens ran green on the FIRST full-suite run on Node —
  the syscall surface the PoC had already grown was sufficient without
  addition.
- **The kernel reads string traps from the transfer SAB, not from guest
  pointers** — so the shim's `sysTx` writes JS strings straight into tx and
  the a0 register is dead weight for those traps. No scratch region in guest
  memory was ever needed.
- **`NSEC`'s reply is `clock_time_get`'s exact shape** — one u64 of
  little-endian nanoseconds, written to a guest pointer. The wasi clock is
  one trap with zero translation.
- **The SAB TextDecoder rule found its second victim** (§5.3's finding
  recurring): Node decodes SAB-backed views, Chrome refuses — the shim
  stalled at 110/128 in the browser until every `decode(tx.subarray(…))`
  became `decode(tx.slice(…))`. The rule is now: **no decode without a
  copy, anywhere tx is read.**
- **`path_rename` is V10's rule serving WASI**: rename = `link` + `remove`
  in the shim, exactly the userland decomposition the syscall census
  recorded (V10 has no rename syscall).
- **The citizens**: wasitest, wasi-libc through the full `wasm32-wasip1`
  sysroot, 278,314 bytes — stdio, argv, clock, `fopen`, `readdir`. gotest,
  **real Go** (`GOOS=wasip1 GOARCH=wasm`, go1.25.6), 2,725,560 bytes —
  `os.ReadFile`, `os.ReadDir`, `os.WriteFile`, and `time.Sleep` parking on
  `poll_oneoff` → the kernel's `SLEEP`. Go's runtime wanted nothing the
  shim didn't have: args, environ (empty), monotonic clock, `random_get`,
  `poll_oneoff`, and the fd/path families.

And CPython — the second benchmark's interpreter, Brett Cannon's wasi build of
3.14.7, 30,522,756 bytes — landed the same day and measured five more:

- **preview1's `fd_readdir` signals end-of-directory by `bufused < buflen`** —
  so a shim that stops at the last whole dirent reads as exhaustion. Measured:
  `os.listdir` of a 185-entry directory returned 118, and importlib's CACHED
  FileFinder scan then swore the stdlib had no `re` — zero opens, module not
  found, while smaller directories worked perfectly. The contract wants the
  final dirent written TRUNCATED so the buffer fills exactly; the caller
  resumes from the last whole entry's cookie.
- **Byte-offset cookies across separate directory enumerations are fragile**
  (the directory can change between reads — CPython writes `__pycache__`
  entries mid-scan, measured). The shim now snapshots the whole directory on
  first read — one continuous enumeration at the offsets the kernel itself
  returned — and serves dirents by index cookie.
- **The stdlib closure is 21 files** (`wasi/pylib.txt`): os + json + re and
  re's cascade (enum, functools, operator, keyword, types, reprlib, copyreg,
  collections, encodings×3); everything else CPython boots with is frozen
  into python.wasm. Two measurement lessons carried in the manifest: getpath
  STATS `os.py` as the prefix landmark without ever opening it, and importlib
  prefers shipped `__pycache__` pycs over sources — an open log must
  normalise pyc loads back to their source names or undercount.
- **POSIX callers probe with `readlink` and expect EINVAL** for a
  non-symlink; the kernel's "not a symlink" errstr maps there, not to EIO —
  getpath treats EIO as fatal and EINVAL as "not a venv".
- **The shim's own bug class: errstr truncation breaks errno regexes.** An
  argument in the wrong mailbox slot capped errstr at 26 bytes;
  `"'/pyvenv.cfg' does not exi"` failed the `/does not exist/` match, mapped
  to EIO, and CPython died at line 355 of frozen getpath — three layers of
  misdirection from one transposed parameter. errno mapping wants the whole
  message.

The run itself: `python /tmp/pytest.py` boots, finds its stdlib through the
namespace by landmark, imports json (through `re`, compiled by the real
`_compiler` chain), round-trips a file, and writes `__pycache__` pycs whose
finalisation exercises `path_rename` — V10's link+remove — in passing.

### 9.6 What the Rust kernel core measured (2026-08-27)

> The `native/` tree these findings name became `kernel/` + `hosts/macos/` at the
> PoC declaration (2026-08-29, decision log); the findings stand as measured.

The native milestone opened the same day the WASI ABI closed: `native/` is a
cargo workspace — `kernel` (the core, 1,900 lines, a structural port of
kernel.mjs) and `host` (the macOS shim: wasmtime 37, one OS thread per guest,
mpsc mailboxes). First findings, all measured against the 130-test suite:

- **The native guard needs no hand-written wasm.** The JS host must throw a
  JS exception through wasm so `try_table`/`catch_all` can catch the lazy
  fork child's unwind. On wasmtime the host-function frame boundary of the
  nested `__forkshim` call plays catch_all's role: a typed host error unwinds
  the child's wasm frames and stops exactly at the Rust frame that made the
  call, which restores `[0,sp)` and returns the pid. `guard.rfork` is an
  ordinary host function; the byte-emitted guard module stays a JS-host
  artefact.
- **The asyncify machinery ports intact**: bare fork (memory snapshot into a
  fresh store on a new thread, both sides rewound), real setjmp/longjmp, and
  the thread contexts all run — the real rc's pipelines, subshells and
  captures pass, and `sam -d` runs its structural-regexp suite over native
  wasmtime on the first attempt.
- **The bind command is WHY fork shares the namespace.** The port initially
  copied the namespace on flagless forks ("harmless: every such fork execs
  immediately") — and three rc tests failed within the hour: `/bin/bind`
  mutates the PARENT's view, which only works because rfork(RFPROC) without
  RFNAMEG shares the mount table by reference. The deviation was measured
  wrong and removed; the namespace is now a shared handle, copied only under
  RFNAMEG. (One of the three "failures" had been passing falsely on a glob:
  `*hello*` matched `hellofs`.)
- **The kernel stayed a pure state machine.** Syscalls arrive with their
  strings pre-marshalled (the tx SAB's exact shape), replies leave through
  per-call senders, parking is a stored sender, and everything platform-bound
  — spawn a guest, write the console, arm a timer, shut down — leaves as an
  `Effect` the embedding shim drains. That contract is the per-platform
  seam the decision log promised.

Tranche two (same day) brought notes (V7 timing, postnote interrupting
blocked calls through per-call reply channels — a late device completion
dies in a dropped receiver instead of poisoning the next syscall), devproc
with the uid ctl rules, and AREAD/IOWAIT (the wasm libthread runs natively:
threadtest passes on wasmtime). **80 of 130.** And the wire-9P port measured
the next structural fact before writing itself: **devmnt is irreducibly
async** — every mount operation awaits an R-message that arrives through a
parked transport read, and the reference kernel's `async/await` is
load-bearing there ("the kernel dispatcher is async throughout" was a
design sentence, not a convenience). The Rust core's synchronous dispatch
cannot express a suspension in the middle of a walk, so the mount driver
forces the core async — a single-threaded executor of the kernel's own
(no tokio; the effect seam unchanged) is the recorded next step, ahead of
devmnt, the window server, and the wasi shim.

Tranche three (same day): **the core went async and devmnt landed** —
96 of 130. The executor is 160 lines (`exec.rs`): boxed futures, wakers
pushing task ids onto a ready queue, and a oneshot whose first-completion-
wins rule IS the interrupt semantics (postnote completes Intr; the device's
late completion finds the slot taken — no double-reply plumbing). Dispatch
became tasks; parking became awaiting; AREAD became a spawned subtask that
made async reads device-blind (mounts included) for free. Three measured
lessons: (1) a missing trap-46 marshalling in the runner sent MOUNT an
empty `old`, which canonicalised to `/` and SHADOWED THE ROOT with the
9P server — twenty tests failed at a distance from one absent case-arm,
and the errstr text (`mnt walk 'n'`) was the map back; (2) **the copy-out
table is load-bearing and both hosts have now lost the same entry
independently** — IOWAIT's tag+data never reached guest memory, libthread
matched a garbage tag, and the reader thread slept forever (the JS host
lost the identical entry to a broken batch script months apart); (3) the
per-connection reader task plus tag/expect maps port mnt9p.mjs directly,
clone-before-open included — exportfs and wire symlinks passed unchanged
once MOUNT marshalled.

Tranche four (same day): **the WASI shim on wasmtime — all six citizen
tests green on the first run.** `host/src/wasi.rs` is wasi1.mjs ported
mechanically: the JS shim was already kernel-trap-shaped (sysTx strings in,
reply bytes out), so the Rust version is the same table of small functions
over the same traps, with the fd table and dirent-snapshot semantics
carried whole — including the truncated-final-dirent rule and the
readlink-probe-is-EINVAL mapping, both of which were measured lessons the
first time and simply facts the second. wasi-libc, real Go (wasip1), and
real CPython 3.14 run unchanged against the Rust kernel.

Tranche five (same day): **the window server and the draw engine — and the
suite closes at 130 of 130.** `draw.rs` (363 lines) is draw.mjs with one
recorded deviation: every operation snapshots its source pixels before
writing, because a screen and its windows share a backing and RefCell
refuses the aliased borrow the JS never noticed it took (sources are 1×1
colours and font strips; the copy is noise). devwsys lives in the kernel
with presentation as a no-op — the native host is headless, which the suite
was designed for: rasters read back through `rgb`, input injected through
`wctl`. One test needed widening, and the lesson is worth its line: acme's
column-split point differs a few lines between hosts (the height heuristic),
so the button-3 assertion band now covers the semantic claim — new ink below
the listing — rather than one host's layout. **All three hosts pass the
identical 130: Node, Chrome, and the Rust kernel under wasmtime.** The
native core totals 5,907 lines (kernel 4,015 across lib/exec/draw/stat9;
host 1,529 across the runner and the wasi shim).

Conformance at first light: **75 of 130** — init's lifecycle tranche, the
whole rc script (35 lines), forktest, sam -d, links/symlinks, wstat, unmount,
unions, RFNOMNT/RFNOWAIT. The 55 still red sit in four unported subsystems:
wire 9P (devmnt/exportfs), devproc + the uid ctl, the window server with the
draw engine, and notes/AREAD/IOWAIT (threadtest, samterm, acme) — plus the
WASI shim. `kernel.mjs` remains the reference; the suite is the spec.

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

- **The uid model is designed and running** — [docs/identity.md](docs/identity.md), the item APE
  called impossible: mutable per-process credentials in the kernel (names canonical,
  numbers the personality's), transitions through `/proc/<pid>/ctl` with no new system
  calls, 9P2000.u's `DMSETUID` bit position at exec, V10 enforcement in the in-process
  devices and per-attach identity on the wire. The PoC exercises all of it: setuid down,
  0600 denial, no privilege climb, chown/chmod as pure-libc `wstat`, and a setuid image
  elevating euid while ruid stays.

- **Hard links and the symlink family are decided and running** — minted wire types
  128/130/132 (above every dialect's range; strangers answer `Rerror` and the client
  degrades, tested), 9P2000.u's `QTSYMLINK`/`DMSYMLINK` bits, kernel traps 60–62, and
  V10's resolution rule: symlinks resolve in the walking process's namespace, in the
  kernel, because no server knows the client's namespace. The plan records the decision.

**Still open:**

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

### 9.7 The toolchain moves in: clang as a guest (measured 2026-08-29)

The demo's C-toolchain profile runs LLVM-8 clang and lld (binji/wasm-clang's
wasm/WASI builds) as ordinary guests under the WASI shim, compiling and
linking against a wasi sysroot in the namespace and exec'ing the result —
the design's "move the toolchain in" option, working. Two findings with
teeth:

- **`wasi_unstable` is preview1 with two traps.** The 2019 binaries import
  `wasi_unstable`: identical function names, but `fd_seek`'s whence enum is
  reordered (unstable CUR=0,END=1,SET=2; preview1 SET=0,CUR=1,END=2 — every
  object-file seek corrupts without a remap) and `filestat` packs `nlink`
  as u32 at offset 20 (56-byte struct) where preview1 has u64 at 24
  (64 bytes). A ~30-line adapter (module aliasing + whence map + struct
  repack) over the preview1 shim runs them; the demo applies it as a dist
  derivation, the frozen reference untouched.

- **Layer-2 sysroots must match the toolchain's ERA, not just the target
  (measured 2026-08-29).** WLR's prebuilt `libz.a` (wasi-sdk-20, LLVM 16
  objects) is rejected by the in-tab LLVM-8 `wasm-ld`: "Bad section type."
  The registry's answer became the system's own dogfood: the demo registry's
  zlib package is **compiled from pinned source by the tab's own cc** at
  registry-build time (ten per-file compiles in a headless boot — zlib's
  K&R parameter names collide with its own `code` typedef under
  amalgamation, so one TU is not an option — the objects base64'd out
  through the console since ramfs is memory-only), and linked by glob:
  `cc z.c /lib/wasm32-wasi/zlib/*.o`. An `ar` for the guest would restore
  `-lz`; queued.

- **Host toolchain re-measurement (2026-08-29, late): node v24.20.0, go
  1.27.0.** Christine updated the host toolchains; per VERSIONS' rule the
  full suite re-ran on all three hosts first — 135/135/135 — and the
  records moved together: VERSIONS, the amber pin (the amber tracks the
  measured engine, moving only deliberately), CI's go line, and the demo's
  entire Go toolchain overlay regenerated at 1.27.0 (tools and stdlib
  archives must share one go version; the gobyexample-derived set grew 115
  → 124 packages under 1.27's dep tree). The 1.27 gc compiler, cross-built
  and run as a guest, compiles and its output runs — re-proven headless
  before deploy.

- **M1 measured (2026-08-29): the whole operating system is a 62.2MB
  distroless image.** `FROM scratch` + two COPYs: the statically-linked musl
  host (16,789,232 bytes — wasmtime 48 slimmed to
  cranelift/runtime/gc, the default features' zstd/gdbjit C deps refused as
  dead weight) and the shared rootfs (48,467,666 bytes, CPython the bulk).
  Total image 65,256,898 bytes; 135 PASS inside it, `-i` boots to rc —
  proven in CI on every push. The oracle-in-amber job earned its name on
  its first run: an arbitrary Node 22.12.0 pin failed with "Invalid opcode
  0x1f (enable with --experimental-wasm-exnref)" — try_table is default-on
  by 22.23.2 (VERSIONS' measured engine, now the pin) but flagged at
  22.12.0, a sharper bound on the §2 engine table than folklore had.

- **The registry survey (measured 2026-08-29): external wasm binaries run
  today, and the acquisition constraints are known.** The probe: WLR's
  `ruby-3.2.2.wasm` (23.3MB, single file) fetched from GitHub releases,
  sha256-verified against the registry's own `.sha256sum` sibling, dropped
  into `/bin` — **ran on the existing WASI personality with zero shim
  changes** (`ruby 3.2.2 … [wasm32-wasi]`; expressions, blocks, sort/map all
  correct). The generic import machinery already exists: exec selects the
  dialect by the module's imports. Registry facts, each probed:
  - **WLR** (vmware-labs/webassembly-language-runtimes; assets still on the
    original org, continued by the webassemblylabs fork): GitHub releases;
    single-file runtimes (ruby 23.3MB, ruby-slim 7.9MB, php-cgi-slim 6.0MB,
    python as tar.gz) **plus a `libs/` catalogue — zlib, sqlite, libpng,
    libxml2, oniguruma as wasi-sdk sysroot tarballs: layer-2 personality
    material our in-tab `cc` could link against**. Every asset has a
    `.sha256sum` sibling. CORS, measured: the GitHub API sends
    `access-control-allow-origin: *`; **asset downloads (302 →
    objects.githubusercontent.com, GET/206) send none** — so browser-hosted
    installs from GitHub need an intermediary (a same-origin mirror, or
    /net); the Node and native hosts are unconstrained.
  - **Wasmer** (wasmer.io): GraphQL API; packages ship as `.webc` (their
    own container format bundling module+fs+metadata); a large share of the
    catalogue is **WASIX** — fork/exec/threads/sockets extensions this
    system does not shim. Noted, not planned: ipnx has real fork/exec, so a
    wasix personality is plausible later; webc parsing plus GraphQL makes
    wasmer a phase-2 registry either way.
  - **PyPI**: the CORS-friendly one, already live (pip over `#H`).
  - **Component registries (warg) and WIT worlds**: out of scope by the
    founding decision — typed component interfaces do not compose with 9P's
    uniform untyped one; ipnx imports *commands*. Where the wasm world links
    components, ipnx mounts servers. OCI-hosted wasm artifacts align with
    M1's infrastructure and stay native-host-first (no CORS).

- **Rune width must match the vendored snapshot's era — and the shim's own
  helpers must use Rune, never a hardcoded width (measured 2026-08-29).**
  The u.h shim declared `Rune` as `unsigned short` ("the 4th edition's"),
  but the vendored tree is the LATE 4th edition — `libc.h` says `Runemax =
  0x10FFFF`, `UTFmax = 4`, and `rune.c` checks surrogates. Almost nothing
  notices 16-bit truncation of ASCII-era data; the one construct that does
  is sam's class-range sentinel (`regexp.c` `bldcclass`: `classp[n] =
  Runemax`, matched by `*p == Runemax`) — a 16-bit Rune truncates the store
  to 0xFFFF, the compare never fires, and every `[a-z]` range silently
  becomes the literal set {0xFFFF, lo, hi}. Measured: `,x/[a-z]+es/` no-ops
  while `[abco]`, `[^x]`, closures and alternation all work. Fixes, all
  shim/derivation: `Rune` is now `unsigned int`; `L"…"` literals match via
  P9CC's `-Xclang -fwchar-type=int -Xclang -fno-signed-wchar` (wasm32's
  default wchar_t is a SIGNED int, and clang rejects initialising an
  unsigned array from a signed wide literal; there is no driver-level
  `-funsigned-wchar`); and lib9.c's `_runebsearch` — our platform helper
  behind the vendored `runetype.c` — had `unsigned short` hardcoded, which
  half-strided every 32-bit classification table. That last one links
  silently: wasm promotes u16 and u32 alike to i32, so mismatched C
  signatures produce identical wasm signatures — the failure surfaced as
  `wc` counting zero words (`isspacerune` over garbage). The suite's new
  class-range test (`,x/[a-z]+es/ g// c//`) pins all three.

- **The gc compiler hosts on ipnx (measured 2026-08-29).** `cmd/compile`,
  `cmd/link` and `cmd/gofmt` are pure Go, so `GOOS=wasip1 GOARCH=wasm go
  build cmd/compile` simply works (41.9MB, 11.0MB, 4.8MB wasm). Run as ipnx
  guests they compile and link real programs: `compile -p main -importcfg
  /go/importcfg -o x.o x.go` then `link -importcfg /go/importcfg -o x x.o`,
  the export archives shipped at `/go/pkg/<import>.a` (the build cache's own
  `.a`, via `go list -export -deps`; tools and archives must share one go
  version). A binary the guest linker produced runs as a guest itself —
  goroutine worker pools, channels, select all correct. The full std export
  set is 117.5MB; the gobyexample-derived set is 115 packages / 35.3MB
  (net/http alone +31MB, refused until /net). The folklore "Go cannot
  compile on wasm" conflates the orchestrator with the tools: only `go
  build`'s os/exec is missing, and ipnx's fork+exec supplies it.

- **This CPython wasi build has no zlib (and pip needs none).** Measured:
  `zlib` absent from `sys.builtin_module_names` (as are `_sqlite3`, `_ssl`,
  `_lzma`, `_bz2`; present: `binascii` with crc32, `_struct`, `_json`,
  `pyexpat`, the hash builtins). Wheels are DEFLATE, so the personality
  ships a pure-Python `zlib.py` — a puff.c-shaped inflate, crc32 delegated
  to binascii — and pip walks the zip central directory itself, one-shot
  inflating each member (streaming decompressobj semantics are the hard
  part of zlib's API; the personality sidesteps them). PyPI's JSON API and
  files.pythonhosted.org send permissive CORS (the fact micropip relies
  on), so the browser's own fetch serves `#H` unproxied.

- **What each demo citizen's personality wanted (measured 2026-08-29).**
  The three benchmarks all *run* under one ABI personality — the WASI shim —
  but each exposed different missing pieces, and completing them IS the
  personality:
  - **clang (cc):** the older `wasi_unstable` ABI dialect (whence-remap +
    filestat repack); real inodes (its FileManager dedups by inode, so `ino=0`
    made every file the same file); and, to compile *for* something, the
    wasi-libc/POSIX target sysroot (`/include`, `/lib/wasm32-wasi`, crt1.o).
  - **CPython (python):** `wasi_snapshot_preview1`; a cwd-honouring path
    resolver (relative opens must find the process's directory); a populated
    `environ` with `PYTHONHOME=/`; and its stdlib tree at `/lib/python3.14`.
    Its personality is ABI + runtime-support files + environment.
  - **Go (the binary):** `wasip1` and nothing more — a static binary. Its
    *toolchain*, though, wants a personality ipnx does not yet offer: exec
    exposed to the Go runtime. That is the wall, and the honest long-term
    answer is an ipnx-native-Go port personality whose runtime targets ipnx's
    fork/exec directly.
  The lesson: a demo that runs real software is a personality-measurement
  harness. Every failure names a missing piece; the fix completes the
  personality; nothing patches the source.

- **Why C compiles in the tab and Go does not — and what ipnx adds.**
  clang's wasm build (binji) cannot run in *driver* mode: the driver spawns
  `clang -cc1` and the linker as subprocesses, and WASI has no process
  spawning, so binji's harness orchestrates the pieces from JavaScript. ipnx
  removes that limitation from a *different* direction: it has real fork+exec,
  so an ordinary ipnx process — `cc(1)`, `userspace/cmd/cc.c` — drives
  `clang -cc1` and `wasm-ld` itself. The demo's `cc` is therefore a genuine
  compiler driver (flags, `-o`, `-c`, multiple files) rather than a wrapper.
  The **Go toolchain hits the same wall with no such exit**: `go build`
  orchestrates compile/assemble/link through `os/exec`, and Go's `wasip1`
  runtime returns `ENOSYS` for exec (WASI omits processes); the individual
  tools are not shipped as wasm and are large. So C compiles in-tab (clang is
  one self-contained wasm binary an external driver can invoke per step), Go
  binaries *run* in-tab but are *built* on the host, and Python interprets
  in-tab (compiled once, interprets anything). The three benchmarks turn out
  to have three different relationships to "runs in the tab," and the
  difference is exactly the process model — the thing ipnx supplies and WASI
  omits.

- **A WASI shim must give every file a distinct inode.** The shim reported
  `ino = 0` universally; clang's FileManager deduplicates headers by
  (dev, ino) and therefore treated *every file as the same file* — it
  cached hello.c as the content of `stdio.h` and diagnosed "redefinition
  of 'main'" *inside the header*, seven include levels deep. Plumbing the
  9P qid.path (falling back to a path hash) as the inode fixes it. The
  Rust host's shim (`hosts/macos/src/wasi.rs`) has the same `ino = 0` and
  needs the same fix before it meets a compiler. **The Rust host
  (`hosts/macos/src/wasi.rs`) carried the same `ino=0` and is fixed the same
  way (2026-08-29): `parse9` now yields qid.path, `put_filestat` and
  `fd_readdir`'s dirents write it (FNV-1a of the path when a server reports
  no qid), and the suite pins it — wasitest stats two files and prints
  `inodes: distinct`/`unreported`/`BROKEN`; init asserts not-BROKEN, so the
  frozen shim's honest 0 self-skips while a regression fails on any host.
  Verified: `distinct` on wasmtime, 134 PASS on all three hosts.**

## 12. Prior art: capability operating systems (researched 2026-08-29)

The identity decisions (su, the user decomposition, the profile) rest on
capability thinking, and capability operating systems have a fifty-year
graveyard worth learning from. The census, with the honest post-mortems:

**Hardware era.** Dennis & Van Horn coined capabilities (1966). The
[Plessey System 250](https://en.wikipedia.org/wiki/Plessey_System_250)
(1969–72) shipped and ran telephone switches. Cambridge CAP and CMU HYDRA
proved the model in research. IBM's System/38 (1978) put
[capability-based addressing in a commercial machine](https://www.semanticscholar.org/paper/IBM-System/38-support-for-capability-based-Houdek-Soltis/34e41ebc64b786e20efc490363aaeb5fa508866b).
Intel's [iAPX 432](https://en.wikipedia.org/wiki/Intel_iAPX_432) (1981) ran
capabilities in silicon at roughly a quarter of an 8086's speed and poisoned
the well for a generation —
[Colwell's autopsy](https://archive.org/details/432_complexity_paper) found
most of the loss was implementation (an Ada compiler emitting poor code,
25–35% alone; missing instruction-stream literals), not the capability
model.

**OS era.** Berkeley's CAL-TSS (1968–71) was among the first capability
OSes; it ran for about a year and was abandoned —
[Lampson & Sturgis's retrospective](https://dl.acm.org/doi/10.1145/360051.360074)
(CACM 1976) is the honest post-mortem of paying for indirection everywhere
on a machine that could not afford it. Tymshare's KeyKOS ran capabilities
plus a checkpointed single-level store in commercial production.
**[Amoeba](https://www.cs.vu.nl/pub/amoeba/Intro.pdf)** (Vrije Universiteit,
Tanenbaum, 1981–96) was the distributed capability OS: a 128-bit SPARSE
ticket — 48-bit server port, 24-bit object, 8-bit rights, 48-bit check
field — protected by a one-way function rather than kernel tables, so a
client could itself derive a reduced-rights capability, and capabilities
were plain bits storable in files. The direct ancestor of the signed URL.
[Shapiro's EROS](https://www.semanticscholar.org/paper/Eros:-a-capability-system-Shapiro-Farber/f7aa91b60a056594db8bc111d914746754b939e3)
(1990s) inherited KeyKOS and demolished the performance myth (capability
IPC comparable to conventional kernels). All of these are dead.

**The survivors, all in disguise.** Mach ports — genuine capabilities —
live inside every iPhone. seL4 is a verified capability microkernel
succeeding in defence/automotive niches. FreeBSD's Capsicum bolted
capability mode onto Unix by observing that file descriptors already are
capabilities. Fuchsia ships on smart displays and never displaced Android.
CHERI/Morello revives capability hardware for memory safety. The largest
capability system ever deployed has no name: signed URLs, bearer tokens,
JWTs.

**The five recurring causes of death**, in descending lethality:

1. **The compatibility cliff.** Amoeba (its Ajax POSIX emulation partial),
   EROS (none), Fuchsia (Starnix late): users choose their software over
   your security, every time. Killed more capability systems than
   everything else combined.
2. **Ambient authority is the incumbent's moat.** Unix programs open by
   pathname from anywhere; capability discipline says pass handles; the
   retrofit friction (Capsicum's cap_enter disabling global namespaces) is
   semantic, not mechanical. Hardy's Confused Deputy (1988) is the standing
   argument that ambient authority is the bug factory.
3. **Performance folklore.** The 432's implementation failures were
   attributed to the model and the myth outlived EROS's refutation by a
   generation. Expect to fight folklore with measurement.
4. **Revocation and legibility.** The System/38→AS/400 retreat: IBM
   [found no way to revoke](https://en.wikipedia.org/wiki/IBM_System/38)
   capabilities users could save to tape and restore, and moved authority
   into user profiles, keeping the capability machinery invisible beneath.
   KeyKOS's checkpointed single-level store was elegant and illegible to
   operators. Administrators must be able to audit; held bits resist audit.
5. **All-or-nothing adoption.** The 432 needed a new language, compilers,
   OS. Every survivor hid inside something that already existed.

**What this vindicates and what it directs** (the doctrine is in the
decision log, 2026-08-29): the WASI-ABI-plus-benchmarks posture is the
anti-Amoeba move and stays sacred; IPNX's capabilities stay invisible
(namespace, fd, bind — no "capability" noun ever reaches a user, the
System/38-in-AS/400 and Capsicum lesson); devcap's earmarked mechanism
adopts Amoeba's sparse crypto-checked self-attenuating ticket with modern
MACs; revocation is answered by expiry and re-attach and unmount, never a
revocation registry; the storage invariant is the legible alternative to
the single-level store; and the deployment story (tab, laptop, container,
agent sandbox) gets re-examined periodically with the same honesty as the
code — Amoeba and Plan 9 both died of their deployment wave, not their
kernels.

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
[9front FQA](https://fqa.9front.org/) ·
[Wanix](https://github.com/tractordev/wanix) ·
[Apptron](https://github.com/tractordev/apptron) ·
[Apptron announcement](https://progrium.medium.com/announcing-apptron-cross-platform-native-apis-made-accessible-da661f492541)
