# A hosted Plan 9 with a Research Unix personality — feasibility study

**META — this document does not answer one of the six questions. It informs
and guides the ones that do.**

**Role: the evidence base.** Every finding with provenance — measurements,
source citations, measured tables with dates. It is what the *why* and the
*what* documents are built on; a claim here carries file-and-line provenance,
and a claim elsewhere that contradicts it is wrong.

*Begun 2026-08-26 and **living** — the evidence base behind
[docs/design.md](docs/archive/design-log-claude-written.md). Findings land here as they are established, with their
provenance; decisions and scope stay in the plan.*

*It is written to be self-contained. Measurements taken against the Research Unix V10 tree
were made in the parent repository ([ipnx](https://github.com/ChristineTham/ipnx)) and are
recorded here as data, because that tree is deliberately **not** copied into this one.*

## TL;DR — the recommendation

Build **the IPNX kernel — Plan 9's architecture, none of its code — as an ordinary
userspace process** on macOS,
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

### 9.9 What making emca a file server measured (2026-09-03)

M17a1 turned `emca` from a **client** of the kernel's window device into a
**9P server** of its own windows ([implementation.md](docs/implementation.md)).
Four things were measured doing it, and each changed something.

**1. `newkid` returns no window id — which is why a1 had to precede a2.**
`userspace/cmd/emca.c`'s `convention()` grows a column by writing
`wctlf(wid, "newkid")` and then **re-reads `kids/<i>/winid` from the kernel** to
learn what it just created (`nkids()`, emca.c). So the window ids are the
kernel's. The tree therefore cannot move into emca while emca is not the thing
that mints ids — and that dependency is invisible in the read path, which is
where the stage was originally scoped. **Reading the write path is what found
it.**

**2. Two rectangle paths exist, and only one is the contract's.**

| path | who decides | does emca learn? |
|---|---|---|
| `rect x y w h` → the kernel's `wctl` | the kernel records it | **no** |
| `resize w h` → the window's `events` | emca decides, then tells the kernel | yes |

Measured directly: driving the first leaves emca serving `0 0 0 0` while the
kernel holds `0 0 900 600`; driving the second makes both answer `0 0 900 600`.
The suite drives the second and asserts the equality, because **that equality is
what makes M17a2 a deletion rather than a rewrite** — two answers agree today,
so one of them can go. The first path is the coupling a2 removes.

**3. `ls` over a 9P mount had never been exercised, and the gap hid a
marshalling bug.** The suite's wire-9P tests (`init.c`, the `namespace(6)` and
`srv(3)` cases) **open and read a file**; none reads a *directory*, so no test
had ever parsed an `Rstat`. emca's first server answered every read correctly
and yet `ls` printed `/n/` — because `ls` had fallen into its **non-directory**
branch, which prints `dirname(arg) + "/" + name` with an empty name (`ls.c`).

The cause was one line: **`put16(p, 0)` returns the *advanced* pointer.**
Writing the stat record at `p + 2` *after* that call left a two-byte hole and
placed `nstat` at the wrong offset. Reads never touch `Tstat`, so `cat` worked
throughout and only `ls` could see it.

> The lesson generalises past this bug: **`p = put16(p, v)` is the idiom, and
> mixing the returned pointer with the original in adjacent statements produces
> a message that is wrong only in the paths no test walks.** The record itself
> was byte-for-byte identical to the kernel's `marshal_stat` (`kernel/src/stat9.rs`)
> — 62 bytes, plain 9P2000, no `.u` fields — which is what ruled the format out
> and left only the framing.

**4. `#s` is GLOBAL, and that decides how a service is named.** `srv_posts` is
a single `HashMap<String, SrvPost>` on the kernel (`kernel/src/lib.rs`), so a
posted name is system-wide — the one kind of name in this system that is *not*
namespace-local. Since **emca nests by design**, a fixed `/srv/emca` is a
collision by construction: the second emca fails to post and serves a door
nobody can find. Verified after qualifying: two emcas post `emca.kitty.5` and
`emca.kitty.9`.

> The general rule this settles: **anything that may run more than once posts
> `<name>.<user>.<pid>`**, which is what the real acme already does
> (`plan9/sys/src/cmd/acme/acme.c:321`). And the reason it stayed invisible for
> a while is worth keeping: the *posted* name was mistaken for the interface.
> It is not — a manager reaches `/dev/window/` through its own namespace, which
> has no global name and therefore nothing to collide.

**5. One emca per event queue.** `/dev/window/events` is a **queue**, so two
emca processes each consume half the events. Observed precisely: a second emca
received a window's `new` event (so it knew the type) but not its `content`
event (so its title stayed empty). The suite therefore runs exactly one emca,
started after `bind '#s' /srv` so that its server has somewhere to post.

### 9.10 What moving the window tree out of the kernel measured (2026-09-03)

M17a2 moved the window tree out of the kernel entirely — **285 lines deleted**,
zero references left to `parent`, `kids`, `axis`, `allocated`, `premax`,
`split`, `maximise`, `reparent`, `clone_window`, the eight tree verbs, the
three tree file kinds or `win_close`'s recursion. Seven measurements, and the
first two change what "moving state out of a kernel" means in general.

**1. THE INVARIANTS MOVE WITH THE STATE, and they are easy to leave behind.**
The kernel's `reparent` refused two things: a window as its own parent, and a
window becoming a child of its own descendant (`kernel/src/lib.rs`). Neither is
state; both are guarantees *about* the state. emca's first tree accepted a
self-reparent and corrupted the tree — caught because a test deliberately probes
it. **A layout walk over a cycle does not terminate**, so this is not a tidiness
point.

> The general rule: **when state moves, enumerate what the old owner refused,
> not just what it stored.** A move that copies the fields and not the refusals
> is a regression that the type checker cannot see and that most tests will not
> reach.

**2. IDENTITY AND STRUCTURE ARE DIFFERENT THINGS, and only one of them moved.**
`nextwid` is a single counter serving `#w/<type>/clone` — used by `acme`, `win`
and `con` — and `newkid` alike, so the id space is shared. That means emca
**cannot** mint ids while the raster half is still the kernel's, which is what
M17a1 concluded from `newkid` returning nothing.

Measuring further dissolved the problem rather than solving it: **`clone`
returns the id synchronously**, so emca asks the kernel for a window, receives
an identity, and decides the structure itself. The kernel supplies identity;
emca decides place. The a2/a3 line runs exactly there.

**3. A tree verb is now a MESSAGE, so it is asynchronous.** Written to the
kernel's `wctl`, a verb applied before the write returned. Routed to emca, it
crosses a process boundary and lands in a message loop. `tests.rc` gained
`kidwait` and `allocwait` beside `emwait`. This is the visible cost of the
window manager being a program rather than a table, and it is the same cost rio
and acme pay.

**4. A mirror must converge, not assume.** `reparent` carries parent and
position but **not** the allocation bit, and the kernel's `alloc` is read-only —
its only setter is `minimise`, which *toggles*. So the mirror reads the kernel's
value and toggles on disagreement. Assuming a starting state was wrong because
`treelink` runs both on fresh windows and on windows being hoisted, and those do
not start alike.

**5. `rc` does not substitute a backtick inside a REDIRECTION TARGET.**

```
echo newcol > /dev/window/`{cat $w/winid}/events     # goes nowhere, silently
kwid=`{cat $w/winid}; echo newcol > /dev/window/$kwid/events   # correct
```

Measured while retargeting the suite: the first form produced no error and no
effect, and cost an hour of looking for a fault in emca that was not there. The
same substitution works fine in an ordinary word — it is the redirection target
specifically. **Resolve into a variable first.**

**6. `rc` HAS NO `return` AND NO `break`** — Plan 9 rc's `Builtin[]` table is
`cd`, `whatis`, `eval`, `exec`, `exit`, `shift`, `wait`, `.`, `finit`, `flag`,
`rfork`, and nothing else (`plan9/sys/src/cmd/rc/plan9.c:40`). The suite's
`emwait` had called `return` since it was written: rc looked for `/bin/return`,
did not find it, and reported — so **every wait ran its full eight seconds
whether or not the condition had been met**, and printed an error each time.
Forty-nine such lines were in the baseline output, read for months as noise.
The idiom that works is a flag:

```
fn emwait {
	wdone=no
	for(i in 1 2 3 4 5 6 7 8){
		if(~ $wdone no){
			if(~ `{cat $1 >[2]/dev/null} $2) wdone=yes
			if(~ $wdone no) sleep 1
		}
	}
}
```

**7. A posted service is ONE channel, so it can be mounted only once.**
srv(3)'s rule is that opening a posted name *shares the channel itself*. Two
`mount`s of `/srv/emca.<user>.<pid>` therefore put two clients on one
connection — a second `Tversion`, which 9P defines as resetting it, against a
server holding a single fid table. Measured: the suite wedged indefinitely,
with emca answering a well-formed read and then never being asked again. **One
mount, made once where emca starts**, and every reader shares it through the
namespace, which is what a namespace is for.

### 9.11 The kernel audited against "process orchestration only" (2026-09-03)

Christine's rule — *"the kernel only handles process orchestration; everything
else is handled by host or userspace"* — applied to the whole kernel and
measured, not estimated. Sizes are dedicated functions plus each device's arms
in the four dispatchers (`dev_read_async`, `dev_write_sync`, `dev_walk_one`,
`dev_stat`). Kernel total: **5,331 lines** (`lib.rs` 4,967 + `draw.rs` 364).

**Does not belong — 1,825 lines, 34% of the kernel:**

| | lines | why it fails the test |
|---|---|---|
| **`#w` windows, draw, canvas** | **1,460** | already decided (M17a3). Rendering, rasterising and compositing are not orchestration |
| **`#M` ramfs** | 160 | it is doing **two jobs Plan 9 splits** — see §9.12. A tiny read-only bootstrap root belongs in a kernel (Plan 9's `#/` is 261 lines and refuses writes); a read-write general filesystem does not, and in Plan 9 it is a user program |
| **`#V` snapshots** | 83 | copy-on-write snapshots of a filesystem — squarely the read-write half, so it goes with it (§9.12) |
| **`#c` cons** | 69 | console I/O. The host owns the keyboard and the screen; this is a shim to them |
| **`#H` web** | 53 | an **HTTP client in the kernel**. It already delegates the GET through `Effect::Fetch`, so what remains is a shim that should be a userspace file server over the host's fetch |

**Arguable, and I would not move them without a ruling:**

| | lines | the argument both ways |
|---|---|---|
| **`#p` proc** | 152 | the *state* is orchestration and unambiguously the kernel's; *serving it as files* is presentation. But `/proc/<pid>/ctl` is how one process controls another, which is orchestration's own interface |
| **`#e` env** | 48 | per-namespace environment is process state — but it is also just a small filesystem, and Plan 9 kept it in the kernel |
| **`#Z` host files** | 24 | this **is** the embedding boundary, already delegated. Small, and arguably the one filesystem that must be here |

**Belongs — orchestration proper:** the union/namespace machinery (**574**),
`#s` srv (54, connecting processes by name), `Mnt` (66, 9P at the boundary),
`Pipe` (42), `#d` dup (9), and the dispatcher with `rfork`, `exec`, `exits`
and `await`.

> **The consequential one is the ramfs, and it is not a simple lift.** The
> rootfs is loaded into it, so if it leaves, something else answers `/` at
> boot — the host through `#Z`, or a userspace file server started before
> anything else needs a filesystem. That is a real design question and it is
> **undesigned**.

### 9.12 How Plan 9 roots itself — and where §9.11 was sloppy (2026-09-03)

Christine, on the audit's claim that the ramfs must leave: *"It sounds like you
have strayed a lot from original plan 9 design. How does plan9 handle the root
filesystem if ramfs is userspace?"* Answered from the source, now kept locally
at `plan9/` — **the 9legacy tree**, 4th edition with the maintained patches
applied (gitignored; `plan9-stock/` holds the raw Labs release beside it so a
difference can be attributed). **9legacy leaves `devroot.c` byte-identical to
stock**, so every quotation below holds under both.

**Plan 9's kernel DOES have a root filesystem. It is deliberately almost
nothing.** `sys/src/9/port/devroot.c` is **261 lines**, and the shape is in its
first declarations:

> ```c
> Nrootfiles = 32,
> static Dirtab rootdir[Nrootfiles] = {
> 	"#/",		{Qdir, 0, QTDIR},	0,		DMDIR|0555,
> 	"boot",	{Qboot, 0, QTDIR},	0,		DMDIR|0555,
> };
> ```

**A fixed 32-entry table, mode `0555` throughout, and writing is not merely
unimplemented but an error:**

> ```c
> static long
> rootwrite(Chan*, void*, long, vlong)
> {
> 	error(Egreg);
> 	return 0;
> }
> ```

`rootreset()` adds empty directories — `bin`, `dev`, `env`, `fd` — and
`addbootfile()` puts in files compiled into the kernel image. That is the whole
device. **The real filesystem is a user program or another machine**: the
kernel binds root(3) on `/` and execs `/boot/boot`, **the bootloader**, which
attaches the file server and execs `init`.

> **CORRECTED 2026-09-04.** This paragraph originally said boot *"mounts
> `bootfs.paq`, runs `bootrc`, which attaches the file server … then binds
> `/root` after `/`"*, cited to [boot(8)](https://man2.aiju.de/8/boot) — **a
> web man page, not the tree**. There is **no `bootrc` anywhere in `plan9/`**.
> In 9legacy the bootloader is C: `sys/src/9/boot/boot.c`, whose `rootserver()`
> picks a method, `nsinit()` attaches and mounts the root, and `execinit()`
> (line 202) execs `/$cputype/init`. `newns` is not boot's either — it is
> `newns(2)` (`sys/src/libauth/newns.c:60`), called by `cmd/init.c` among
> others. The rule this breaks is CLAUDE.md's own: a claim about Plan 9 traces
> to file and line **in `plan9/`**, which is why the tree is checked out at
> all.

And `ramfs` is `plan9/sys/src/cmd/ramfs.c` — **945 lines of userspace**
(plan9-stock's is 907, and the two differ; the figure first recorded here was
stock's).

**So Plan 9 splits two roles that ipnx-v12 has conflated in one device:**

| role | Plan 9 | ipnx-v12 |
|---|---|---|
| bootstrap root | `#/` — 32 entries, read-only, writes are `Egreg` | `#M`, and it is the real one |
| the actual filesystem | a **user program** (cwfs, kfs, fossil) or a remote server over 9P | `#M` again — read-write, plus `#V` snapshots |

**§9.11's conclusion was right and its reasoning was sloppy.** It compared our
`#M` to `ramfs(4)` and concluded "the ramfs is userspace in Plan 9, so ours
should be". The honest comparison is that Plan 9 has **both** — a kernel root
device *and* a userspace filesystem — and the disposition is therefore a
**split, not an eviction**: a tiny read-only bootstrap root may legitimately
stay in the kernel, because Plan 9 keeps one; what must leave is the read-write
general filesystem and its snapshots.

> **The straying is real but narrow, and it is now measurable.** Not "we drifted
> from Plan 9" in general — one device doing two jobs, where Plan 9 does one of
> them in 261 read-only lines and the other in a process.

### 9.13 The device table audited against Plan 9's — the contract, broken (2026-09-03)

Christine: *"the kernel was supposed to be a reimplementation of a subset of
plan 9 kernel. it sounds like you have broken the contract. that needs to be
rectified completely."* Measured against `plan9/sys/src/9/port/dev*.c` — **the 9legacy tree** — whose
`Dev …devtab` first field is the device letter. **Verified the same under
stock**: 9legacy changes no device letter, and `devroot.c`, `devwd.c` and
`devdraw.c` are byte-identical between the two trees.

| ours | we mean | **Plan 9 means** | |
|---|---|---|---|
| `#c` | cons | cons | ok |
| `#e` | env | env | ok |
| `#d` | dup | dup | ok |
| `#p` | proc | proc | ok |
| `#s` | srv | srv | ok |
| **`#M`** | **our ramfs** | **`mnt` — THE MOUNT DRIVER** | **COLLISION** |
| **`#w`** | **windows, draw, canvas** | **`watchdog`** | **COLLISION** |
| **`#H`** | HTTP fetch | *nothing* — `webfs` is `sys/src/cmd/webfs`, **userspace** | **INVENTED** |
| **`#V`** | snapshots | *nothing* | **INVENTED** |
| **`#Z`** | host files | *nothing* | **INVENTED** (the hosted boundary; Inferno `emu`'s role, never written down as such) |

Verbatim, so the collisions are not arguable:

> ```c
> Dev mntdevtab = {          Dev wddevtab = {
> 	'M',                     	'w',
> 	"mnt",                   	"watchdog",
> ```

**And two Plan 9 devices are missing where it matters.** `#/` **root** — the
261-line read-only bootstrap device (§9.12) — does not exist here; `#M` does its
job and much more. `#|` **pipe** we implement but gave **no letter at all**, as
we did with the mount driver: `devmnt.c` is **1,198 lines** and the single most
load-bearing device in the architecture, and in this kernel it is a nameless
internal `DevId::Mnt` while its letter is on a ramfs.

**The sharpest statement of the break is not about letters.** Plan 9's kernel
holds `devdraw` (**2,218 lines**) and `devmouse` (**779**) — a graphics device,
because it drives a framebuffer. Its **window system, `rio`, is 5,587 lines in
`sys/src/cmd/rio` — USERSPACE.**

> **We put rio in the kernel.** `#w` bundled the raster, the window tree, the
> canvas, the chrome and the type registry into one device. That is the
> contract break, and M17a1/a2 have been undoing it without naming it: the
> tree's departure to emca was **restoring** Plan 9's shape, not innovating.

**One departure is justified and must be recorded as one, not smuggled.**
Moving the *raster* out (M17a3) is **not** Plan 9-faithful — Plan 9 keeps
`devdraw` in the kernel. It is faithful to **Inferno `emu`**, which this project
took its hosting architecture from: `devdraw` exists to drive hardware, and a
hosted kernel has none — the host owns the screen. The role is preserved; the
location moves because the machine did.

### 9.14 The kernel audited against Plan 9's, completely (2026-09-03)

Christine: *"we are essentially implementing a micro kernel based on a subset
of Plan 9, we should not be adding to it (even the Unix v10 personality should
be userspace) … It is important to keep our kernel pure otherwise we will
encounter serious issues extending the kernel"* — to a MicroVM on a hypervisor,
then to real hardware. Measured against `plan9/` (9legacy) and
`plan9-stock/`.

#### The syscall table — 28 right, 7 added

Ours uses **Plan 9's own numbers** for 28 calls (`libc/9syscall/sys.h`), which
is the contract working. Seven have no Plan 9 number, in two very different
categories:

| trap | | verdict |
|---|---|---|
| `LINK` 60, `SYMLINK` 61, `READLINK` 62 | **absent from Plan 9 AND from 9legacy** | **ADDITION.** Plan 9 has no links: `bind` and `mount` are its answer, and refusing them is a design position, not an omission |
| `ARGS` 200 | `crt0.c`/`crt9.c` fetch argv with it; Plan 9 delivers argv on the stack at `exec` | hosting artifact |
| `NOTEGET` 202 | Plan 9 delivers notes by upcall to a handler; wasm cannot | hosting artifact |
| `AREAD` 210, `IOWAIT` 211 | libthread parks a thread and asks the kernel to wait only when nothing can run | hosting artifact |

#### Identity — the Unix personality, in the kernel

**Plan 9's per-process identity is one field.** `portdat.h:664`:

> ```c
> char	*user;
> ```

and `auth.c` gives it exactly one predicate, `iseve()`. **There is no `euid`,
no `ruid`, no `setuid` anywhere in Plan 9's kernel** — the only match in
`9/port` is `devpermcheck(char *fileuid, …)`, which compares `up->user` to the
file's owner and nothing else.

Ours carries `pub struct Cred { euid: String, ruid: String }` — **a Unix
effective/real pair** — plus `DMSETUID`. And Plan 9's mode bits are exactly
`DMDIR DMAPPEND DMEXCL DMMOUNT DMAUTH DMTMP` (`sys/include/libc.h:597`):
**neither `DMSETUID` nor `DMSYMLINK` exists.** Both are 9P2000.u — a *Unix
extension to 9P*, not Plan 9.

> **This is the clearest instance of what the instruction names.** The uid
> model is a **V10 personality feature living in the kernel**, and
> [identity.md](docs/identity.md) argues for it explicitly — *"A hosted kernel
> inverts the trust geometry"*. That reasoning is coherent and it is still an
> addition to Plan 9, decided when "hosted" was the only target. **On a
> hypervisor or on a Raspberry Pi there is no host to invert the geometry
> toward, and the argument evaporates while the mechanism remains.**

#### The Effect enum — the hosted boundary, undeclared

`Effect` has thirteen variants the kernel pushes to its embedding: `WinUpdate`,
`WinGone`, `WinText`, `WinCanvas`, `WinChrome`, `ReadDone`, `WriteDone`,
`Fetch`, `SnarfSet`, `SnarfGet`, `ConsWrite`, `Timer`, `Host`, `Shutdown`.
Plan 9 has no such concept; **Inferno `emu` does**, and that is the honest
precedent — but it is nowhere written down as one, exactly as `#Z` was not
until 2026-09-03.

**Most of these die on real hardware**, which is the point of auditing now:
`WinUpdate`/`WinCanvas`/`WinChrome` are the window system (leaving anyway),
`Fetch` is `webfs` (userspace in Plan 9), `Snarf*` is a clipboard, and
`Timer` is a clock the kernel should own directly.

#### What is correct, and worth saying

The 28 syscall numbers; the five matching devices; per-process namespaces with
union directories; **wire 9P only at the mount boundary with a Dev table
inside**; notes; `rfork` flags. The architecture is Plan 9's where it counts —
the deviations are additions round the edge, not a different design.

### 9.15 Is the kernel substrate-independent? (2026-09-03)

Christine: deviations are authorised *"only when it is to do with adapting it
for WASM and WASI"*, and *"even then it should be done in a machine independent
way as we may want a non WASM kernel in the future … for example, dis, or .NET
CLR"*. **So the question is not only whether a deviation is forced, but whether
Dis or the CLR could satisfy it unchanged.** Measured across `kernel/src/`.

**The kernel is nearly clean — eleven substrate-specific tokens in 5,331
lines**, and they fall into two places.

**1. `AsySnap` — a legitimate need in an illegitimate shape.** It sits in the
**public** `Effect::Spawn`, which every host must implement:

> ```rust
> pub struct AsySnap {
>     pub snap: Vec<u8>,
>     pub data_ptr: u32,
>     pub sp: u32,
> }
> ```

`snap` is a memory image, `data_ptr` a **wasm linear-memory address**, and `sp`
wasm's **`__stack_pointer` global**. Fork-on-a-VM is a real problem every
substrate has — but **Dis has no linear memory, and the CLR has no stack
pointer to snapshot**; both would fork by other means entirely. This is wasm's
mechanism promoted into the kernel's interface, and it is the exact failure the
second test exists to catch: *a deviation that is permanently justified is the
one that quietly acquires the substrate's shape*.

The fix is to make it **opaque** — a continuation token the kernel stores and
returns without inspecting, whose contents are the embedding's business.

**2. `#[cfg(target_arch = "wasm32")]` — eight occurrences, two purposes.** Six
shadow `eprintln!` because wasm32 has no stderr (a build accommodation, and
harmless). Two are load-bearing:

> `// On wasm32 the host feeds the clock (clock_set); native asks the OS.`

**A compile-time branch on the substrate, inside the kernel.** Time should
arrive the way every other platform fact does — as an operation the embedding
answers — not as a `cfg`. On a hypervisor or a Pi there is a third answer
(a timer device), and a two-way `cfg` has nowhere to put it.

**Nothing else.** No wasm types in the syscall path, no linear-memory
assumptions in the namespace or mount code, no WASI anywhere in `kernel/`. The
substrate leaks in exactly two places, and both are fixable without touching
what the kernel *does*.

> **The authorised deviations pass the second test as they stand:** `ARGS`,
> `NOTEGET`, `AREAD` and `IOWAIT` are named and shaped for *a VM*, not for
> wasm — Dis and the CLR would need the same four and could implement them
> unchanged.

### 9.16 The browser suite is bounded by process spawn, not by the kernel (2026-09-04)

Measured while re-checking the landing page's test count: **164 PASS / 0 FAIL in
Chromium on the Rust core** (`rustkern.mjs` + `kernel.wasm`, `dist` rebuilt from
today's tree), exit 0 — the same 164 as wasmtime and Node. **But ~5 minutes of
wall time against ~1 under wasmtime**, and the network log shows why: a
`worker.mjs` fetch per process, because **a Worker is a process** (RESEARCH
§5.3), and the suite's settling helpers — `emwait`, `kidwait`, `allocwait` —
fork `cat`/`ls`/`wc` **once a second while they poll**. Chrome spawns a Worker
an order of magnitude slower than wasmtime spawns a store.

> Two consequences. **For the suite**: polling by forking is the wrong shape on
> this host, and a blocking read on the thing being waited for — which the
> window contract already specifies for `rect` — would remove the loop entirely.
> **For P5's surface**: nothing the demo does should spawn a process per
> event; the surface reads emca's files and the files change under it.

Also measured: the deployed page (gh-pages, 2026-08-31) says **151**, the local
`dist` said **160**, `index.md` said **161**, the suite is **164** — four values
for one number, the drift CLAUDE.md's *status in one place* rule exists to stop.
`mkpage.mjs` renders `index.md` verbatim with no templating, so the count on the
landing page is hand-maintained; **generating it at build time from a measured
run is the durable fix**, and is not done.

### 9.17 A fresh Linux machine re-measured (2026-09-04)

The handbook's fresh-machine prompt was executed by a cloud session (x86_64 Linux)
that started with the repo and nothing else. What it measured:

- **bison 3.8.2 builds the same system as 2.3.** VERSIONS recorded macOS's bison
  2.3; a Linux package manager supplies 3.8.2 and cannot supply 2.3. With 3.8.2
  regenerating `grep.y` and rc's `syn.y`, the suite ran **164 PASS / 0 FAIL, exit
  0, on all three hosts** — wasmtime, the Rust core under Node, and the frozen
  oracle — with identical PASS sets on the two Rust-core hosts. Per VERSIONS'
  own rule the record moved only after that run: the line now lists both
  measured versions, and `mk.sh`'s `vcheck` warns only when the found version
  matches none of them.
- **wasi-sdk-34 is the release carrying clang 23.1.0-wasi-sdk**, confirmed from
  the tarball (`wasi-sdk-34.0-x86_64-linux.tar.gz`); binaryen `version_132`
  matched directly. Both exact.
- **The prompt omitted a step the Node harness needs.** `main-rust.mjs` loads
  `target/wasm32-unknown-unknown/release/browserhost.wasm`, which
  `cargo build --release -p host` does not produce; the run died with ENOENT
  before any test. The handbook already named this trap under the proofs
  (*"a real trap"*), but not in the setup list a session executes. The step is
  now in the prompt, in Build and run, and in CLAUDE.md's commands.
- **A cloud session's egress differs from a laptop's.** go.dev, dl.google.com
  and python.org were denied by policy; github.com (git and release downloads),
  nodejs.org, static.rust-lang.org and proxy.golang.org were open, and GitHub's
  *API* was scoped to this repository. The workarounds that held — tags by
  `git ls-remote`, Go through `GOTOOLCHAIN` and the module proxy — are recorded
  in the prompt so the next session does not rediscover them.

### 9.18 `pipe(2)` IS an attach of `#|` — and IPNX had it inverted (2026-09-04)

Asked what "`#|` takes its letter" should mean (implementation.md P1 step 2),
the reference answers it outright. `pipe(3)`:

> `bind #|` *dir* … An attach(5) of this device allocates two new
> cross-connected I/O streams, *dir*`/data` and *dir*`/data1`. … The
> `pipe(2)` system call performs an *attach* of this device and returns file
> descriptors to the new pipe's `data` and `data1`.

And `syspipe` (`plan9/sys/src/9/port/sysfile.c`) is literally that:
`namec("#|", Atodir, 0, 0)`, `cclone`, `walk` to `data` and `data1`, `open`
both. **There is one pipe mechanism in Plan 9 and it is the device.**

**IPNX had it the other way round.** `pipe(2)` built the `Pipe` and both chans
itself and no device existed — the letter `'|'` was in no attach table. The
tell was already in the tree: those chans carried `path: Some("#|/data")`, a
path naming a device that could not be walked to, and both ends said `data`.
This is **not a substrate-forced deviation** (the 2026-09-03 test): no VM
requires it, so it was unauthorised, and it had survived only because the PoC
wrote pipes as a special case and nothing revisited them.

Corrected: `attach('|')` mints the pipe, a walk to `data`/`data1` takes an end,
and `pipe(2)` is those three steps. Net effect on kernel size is close to
neutral — the syscall shed what the device gained — and the *shape* is now
Plan 9's.

**Two bugs the inversion had hidden**, both the same mistake: a per-end
refcount of 0 was read as "closed" when it also means "never opened". With
both ends always minted together that could never be observed; the moment a
bound `#|` had one end walked and not the other, a read answered EOF and a
write answered *write on closed pipe*. Plan 9 hangs a queue up in `pipeclose`
— on a *close*, never at attach — so a third bit was needed: `opened[2]`,
with EOF and hangup now requiring `refs[e] == 0 && opened[e]`. Measured before
and after from `rc`: `bind '#|' /n/pp` then `echo … >data` errored, and after
the fix `cat data1` prints what was written.

**A frozen-oracle hazard, found by the new test and worth knowing.** The
oracle's device table (`poc/supervisor/kernel.mjs:1014`) contains
`"|": makePipeDev()`, but that object has **no `attach` method**, and
`kernel.mjs:312` calls `dev.attach(spec, proc)` unguarded — so binding `#|`
does not fail cleanly, it throws a `TypeError` and takes the whole kernel
down (suite stopped at 42 PASS, exit 1). `poc/` is frozen and must not be
edited, so **a test may not probe a device by binding it unless the oracle
either has it working or lacks the table entry entirely.** The pipe-device
test therefore gates on `#R` instead, with the reason written at the test.
Any future test that binds a letter should check the oracle's table first.

### 9.19 The plan audited against the reference — six answers already in the tree, and two claims wrong (2026-09-04)

Prompted by Christine after `#|`: *"You already know the answer. Can you check
you are not making the same mistakes elsewhere in the plan?"* The `#|` error
was not a wrong answer but a **wrong category** — a question of fact, asked as
a question of design. This is the sweep for the rest.

**Answers that are already in `plan9/`, not gaps:**

| the plan says | the reference says |
|---|---|
| P1 step 5: pkg's fetch *"needs a userspace `webfs` **or** a local registry"* | **`webfs` is a userspace program Plan 9 ships** — `sys/src/cmd/webfs`, 12 files. Not undesigned |
| P7: *"`/dev/mouse`, which left with `#w`, comes back as the host's"* | The letter is a fact — `devmouse.c`, **`'m'`** — and it is why architecture.md's lowercase `m` row was doubly wrong: it documented devmnt on **the mouse's letter**. Where the mouse *lives* here is not the reference's to answer; see below |
| P1 step 3 exposes *"whether links survive as a **capability** in a userspace file server"* | **There is no link operation to relocate.** No `Tlink`/`Rlink`/`Tsymlink` in `sys/include/fcall.h`, no `syslink` in `sys/src/9/port/`, nothing in `9syscall/sys.h`. The capability does not exist in Plan 9 at any layer; the answer is `bind` and `mount`, as devised |
| P10: *"what the embedding is when it is virtual hardware"* | Partly answered: **Plan 9 already boots over virtio-9p** — `sys/src/9/boot/bootvirtio9p.c` |

**Two of the plan's own claims are wrong, measured:**

- **P2 step 3 calls the thing `/boot/boot`.** The *name* is wrong and the name
  alone: **`/boot` is the loader's.** *"We can't call something /boot and refer
  to something other than a bootloader"* — and *"not that we should implement a
  bootloader"*. What the script is called is now an open gap.

  **"It is rc" was RIGHT, and this sweep's first draft wrongly "corrected" it**
  — the mirror error for the third time in one day. *"No systemd — boot is rc
  plus a namespace file"* is one of CLAUDE.md's three founding refusals. Plan 9's
  equivalent being C (`sys/src/9/boot/boot.c`) is not a counter-argument; it is
  **the departure that refusal exists to make**. The `plan9/` measurement was
  sound and the inference from it was not: *the reference does not overrule a
  decision to differ from the reference.*

  (Still true, and separately useful: **there is no `bootrc` in `plan9/` at
  all** — §9.12 claimed one on the authority of a 9front web man page rather
  than the tree, and is corrected above.)

  Also already right in the code: `/lib/namespace`'s own header says it is
  *"read by init through newns()"*, so this system had `newns` in the correct
  process before the sweep questioned it. The namespace-file half of the
  refusal landed as M2 on 2026-08-29; **step 3 is the rc half**.
- **P2 step 3 puts `newns` in `/boot/boot`.** `/lib/namespace` is read by
  **`newns(2)`** — `libauth/newns.c:60` — and its callers are `cmd/init.c`,
  `auth/login.c`, `cmd/cpu.c`, `auth/none.c`. **`newns` belongs to init**, not
  to the bootloader. Boot attaches the root and execs; init makes the
  namespace.

**A third failure mode, and it is the worst of the three.** `/boot` was
neither undecided nor merely a fact: **it was decided, recorded, and I did not
find it.** design.md 2026-09-02 says it outright — *"`boot` also takes a name
Unix already owns: the bootfile, the kernel image, the loader … a name Unix
already uses is not available. All three slips were the same shape — reaching
for a plausible-sounding name without checking what it already carried."*
P2 step 3 is the fourth instance of that shape, and this sweep first wrote the
correction down **backwards** — as *"`/boot/boot` IS the bootloader"*, i.e. as
a licence to build one — before Christine corrected it twice more.

**Why it was missed, which is the transferable part.** The search was for
*bootloader* and `/boot/boot`. The rule is filed under **naming** — *"do not
put configuration into a root that means something else"* — and contains
neither term prominently. So: **a decision has to be findable from the words
someone would search, not only from the words it was written with.** Grepping
`design.md` for the *thing* is not enough; the rule that governs a thing is
often filed under the *principle*. When a search comes up empty, that is
evidence the wrong word was used, not that the decision does not exist —
especially when Christine says *"we definitely had this conversation before."*

**A provenance slip, and the reason the two trees exist.** P2 step 2 cites
*"`sys/src/cmd/ramfs.c` (907 lines)"*. That is **plan9-stock's** figure;
**9legacy's is 945**, and the files differ. CLAUDE.md's rule is explicit —
*"`plan9/` … **Check claims against this one**"* — so the citation was taken
from the wrong tree.

**Citations that hold, checked:** `devroot.c` is 261 lines exactly, as P2 step
1 says. `auth.c`'s `userwrite` errors unless the write is exactly `"none"`, as
P1 step 4 says (the line moved; the function is what matters).

**On `#R`, honestly.** Plan 9 has **no ramfs device** — ramfs is userspace, and
P2 step 2 is what makes it so here. The placeholder letter really was
arbitrary, so asking was not wrong; presenting it as *three design options*
was, because the reference's actual answer is *"this should not be a device at
all"*, which is a better thing to have said.

**THE OPPOSITE ERROR, MADE IN THIS SAME SWEEP.** The first draft of the table
above answered [proposals.md](docs/proposals.md)'s *"A shared rasteriser, or
native per host?"* with drawterm — `libmemdraw`/`libmemlayer` as a portable
rasteriser, `gui-*` as the only per-platform part. Christine, immediately:
*"similarly for draw. we said we would replace by `/dev/canvas`."* She is
right and the finding was struck.

**It contradicts two decisions already taken** (both 2026-09-03, design.md):
*"`/dev/draw` should be rendered by host. the kernel does not know how to
draw"* — which makes *"IPNX implements no renderers"* literal — and *"we don't
use `/dev/draw` — we use `/dev/canvas` … `/dev/draw` is only needed for
acme."* A shared portable rasteriser is a renderer **inside IPNX**, which is
the thing those decisions removed. drawterm is not evidence here: it is a
Plan 9 *terminal*, and this system deliberately does not do what it does.

So the two errors are mirrors, and the sweep committed both:

| | |
|---|---|
| **`#\|`** | a question of **fact**, taken to Christine as a question of design |
| **the rasteriser** | a question already **decided**, re-derived from the reference as if it were fact |

**Two proposals are stale, and by the rewrite rule should already be empty.**
Christine's rule (2026-09-02): *"The decision itself moves off the proposal —
so I am reviewing genuine open decisions rather than settled decisions."*

- proposals.md's *"A shared rasteriser, or native per host?"* was **decided the
  same day it was reframed** — the host renders, natively. It still sits under
  *"What is still genuinely open"*.
- proposals.md's *"GAP — where the ramfs goes, and what answers `/` at boot"*
  asks for a design that **now exists**: implementation.md's P2 steps 1–2 —
  `#/` as Plan 9's root device, and a userspace root file server. The gap is
  filled; the block was not emptied.

**The rule this produces, in the order it must be applied.** Before an item in
the plan is treated as open: **check [design.md](docs/archive/design-log-claude-written.md) first** — a
decided question is consumed, never re-derived — **then check `plan9/`** — a
question of fact is measured, never asked. Only what survives both is a
decision, and only that is worth Christine's time.

### 9.20 The link family removed — and the plan's count was one short (2026-09-04)

P1 step 3, executing the 2026-09-03 decision that `link`/`symlink`/`readlink`
are unauthorised deviations. Two measurements worth keeping.

**The assertion count was 9 in the plan and is 10 in the suite.** The tenth is
*"a server without the extension answers Rerror, not a wedge"* — driven by
`link9("/n/hello/motd", …)` but phrased about `Rerror`, so a grep for
*link*|*lstat* (which is how P0 counted) does not see it. The floor decision's
requirement that **a deletion name its assertions** is what caught it: naming
them forces reading them, and reading them found the one the grep missed.
165 → **155**, on all three hosts, identical assertion for assertion on the two
that run the Rust core.

**Acceptance, measured rather than asserted.** 32 traps remain. Every one is in
`plan9/sys/src/libc/9syscall/sys.h` **at the same number AND under the same
name** — checked pairwise, not just by number — except the four the substrate
forces: `ARGS` 200, `NOTEGET` 202, `AREAD` 210, `IOWAIT` 211. The walk
simplified with the feature: no `WalkRes` enum, no redirect, no eight-deep
symlink limit, no `symtarget` — **a walk is one pass**, which is what it is in
a system with no links.

**A caller the suite does not cover, found by reading rather than by running.**
The WASI shims implemented `path_rename` as **link + unlink** — V10's rule, and
this system's *definition* of the call (it was stated that way in CLAUDE.md and
architecture.md). Deleting trap 60 left both shims calling a trap that no
longer exists, and **the suite stayed green**, because nothing in it renames.
Deleted features leave callers behind, and green is not evidence they are gone.

The fix is deliberately a **refusal, not a substitution**: `path_rename`,
`path_link`, `path_symlink` and `path_readlink` answer `NOTSUP` in both shims.
copy+remove would compile and would mostly work, but it is **a different
contract** — non-atomic, and atomicity is precisely what callers use `rename`
for — so substituting it silently would trade a loud failure for a quiet one.
Where `rename` lives belongs to the **WASI personality**, which
implementation.md's P2 already carries as an open gap.

### 9.21 Identity narrowed to Plan 9's — and eve stopped being omnipotent (2026-09-04)

P1 step 4. `Cred{euid,ruid}` became one `user` per process, and `devpermcheck`
was ported **by rule**, not by name only (`plan9/sys/src/9/port/dev.c:339`):

> ```c
> if(strcmp(up->user, fileuid) == 0)   perm <<= 0;   /* owner bits  */
> else if(strcmp(up->user, eve) == 0)  perm <<= 3;   /* GROUP bits  */
> else                                 perm <<= 6;   /* other bits  */
> ```

**The load-bearing detail, and it is easy to miss:** eve is tested against the
**group** bits, not waved through. The old `ram_access` returned `Ok(())` for
eve unconditionally; Plan 9 does not. So a 0600 file owned by someone else is
now closed to eve too, because its group bits are zero. **Nothing in the suite
would have caught that**, since every rootfs file is eve's own and eve
therefore takes the *owner* path — the change is invisible until someone else
owns something. An assertion was written with the rule for exactly that reason.

Also ported by citation: `#c/user` writable only as `"none"`
(`auth.c`'s `userwrite`: *"anyone can become none"*), and `#c/hostowner`
eve-only, renaming the machine's owner as well as the writer
(`hostownerwrite`).

**Two dependents, both found by RUNNING and not by reading**, which is the
recurring lesson of this phase:

- **`run(1)`'s `user` directive.** `cmd/run.c` performed the spec's identity
  change through `/proc/<pid>/ctl`. With transitions gone it errored, and
  the failure surfaced as *"rc ran the test script and exited clean"* — a
  suite-level FAIL that named nothing. Rewired to `/dev/user`, which makes the
  directive what it always meant: **a drop**, never an escalation ("the
  credential drops LAST" is the comment already in that file).
- **`cmd/su.c`.** Its whole mechanism left. It is no longer built; P2 step 4
  rewrites it as a userspace personality program.

**And a frozen-oracle hazard of a new shape.** The suite's helpers had to keep
working on the old kernel *and* the new one from one rootfs. Making
`becomenone()` try `/dev/user` first and fall back looked right and was wrong:
on the oracle a write to the **cons device reaches the console whatever the
fds say**, so the word `none` landed in the middle of the suite's own output —
`nonePASS uid: mode 0600 …` — which does not match `^PASS` and silently
*lowered the count* without failing anything. The helper now **asks first**
(does `/proc/<pid>/ctl` still take a transition?) and takes one path or the
other. The same hazard is why `run`'s spec line left the suite rather than
`run.c` growing an oracle special case: **a shipped program does not carry
compatibility with the frozen reference.**

### 9.22 `#H` removed — the need was real and the placement was not (2026-09-04)

P1 step 5. `#H` fetched an http(s) body as a file: `#H/<hex-of-url>`, with the
kernel parking the reader and the host performing the GET. It is a clean design
and it was in the wrong place — **`'H'` is not one of Plan 9's letters**
(§9.13's audit), and the need is answered upstream in **userspace**:
`plan9/sys/src/cmd/webfs`, twelve files (§9.19).

Removed: `DevId::Web`, `Node::WebRoot`/`Node::Web`, the `WebState`/`WebEntry`
machinery, `Effect::Fetch`, `Kernel::fetch_done`, the wasmtime host's `ureq`
client **and its dependency**, the browser host's `bh_fetch_done`, and
`rustkern.mjs`'s tag-9 handler. Measured after: `grep -c DevId::Web` is 0, and
`bind '#H'` answers *unknown device #H*.

**The suite did not move**, which is worth stating plainly: 150 before and
after. The `pkg` test already ran against an **offline local registry**
(`pkg -r /tmp/reg`), so nothing in it ever touched the device. That is the
opposite hazard to §9.20's — there a deleted feature left live callers the
suite could not see; here the suite was blind to the feature from the start.
**Both are the same lesson twice: the suite is evidence about what it asserts,
never about what it does not.**

The three callers that did use it were found by reading, and each now says why
it cannot: `pkg`'s http branch, the Python shim's `pip.fetch()`, and the
deployed demo's own registry, which is served over http from the page and is
therefore the one live capability this step costs until P2 step 5 makes the
demo's registry local.

### 9.23 `#/` added — and two parsing facts the reference forced (2026-09-04)

P2 step 1, the plan's one authorised addition to the kernel. `#/` is
devroot(3): a fixed, read-only table of empty mount points, built exactly as
`plan9/sys/src/9/port/devroot.c`'s `rootreset()` builds them —

> ```c
> addrootdir("bin");  addrootdir("dev");   addrootdir("env");
> addrootdir("fd");   addrootdir("mnt");   addrootdir("net");
> addrootdir("net.alt");  addrootdir("proc");  addrootdir("root");
> addrootdir("srv");
> ```

— minus `net`/`net.alt`, because there is no `/net` here yet, and minus `boot`,
because the boot path's *name* is an open gap and `/boot` is the loader's
(design.md 2026-09-04). Writes are refused: Plan 9's `rootwrite` is
`error(Egreg)`, whose string, for the record, is the famous placeholder
**"jmk added reentrancy for threads"** (`port/error.h:44`) — the behaviour is
what was ported, not the joke.

**The device-path parse had to change, and the reference is why.** `walk_once`
split a `#` path at the FIRST `/`, so the spec was everything before it. For
every letter this system had, that is the same as taking one character — and
for `#/` it is not: the letter *is* a slash, so `#/` parsed as the bare `#`
and attached nothing. Plan 9's own rule is simply that the device letter is
the one character after `#`, so that is what the walk does now. **A device
this system did not have exposed a parse that was accidentally right.**

**And an rc parsing fact, measured the hard way** (three failed probes, worth
the note): in the real rc,

```
echo A=$"a
b=`{...}
```

is a **syntax error at the `b=` line** — `$"var` at end of line followed by an
assignment does not parse. Writing `echo A is $"a` instead is fine. This is
vendored rc behaving as vendored rc does; it cost three iterations to see
because the error names the *following* line, not the offending one.

**One more suite-writing constraint, related to §9.21's.** A failed redirect's
diagnostic (`rc: can't open: create not supported on this device`) goes to
**rc's own stderr**, not to the redirection the command was given, so
`>[2=1]` does not capture it and it lands in the middle of the suite's output.
The assertion uses `mkdir`, which reports its own error to fd 2 and *is*
capturable. **Test a refusal through a command that owns its error message.**

### 9.24 The root file server fits; the rootfs does not (2026-09-04)

P2 step 2 builds `userspace/cmd/ramfs.c` — the real filesystem as a user
program, after `plan9/sys/src/cmd/ramfs.c` (945 lines in 9legacy). It works:
seeded from a directory read through the namespace, posted at `/srv`, mounted,
and then created in, written to, read back and removed from. **The tree is a
process**, which is the property the step exists to establish, and the
assertion passes on the frozen oracle as well, because nothing in it needs a
kernel feature the oracle lacks.

**And it cannot yet be the system's root, for a reason that is arithmetic.**

| | |
|---|---|
| the rootfs on disk | **49 MB** |
| `bin/python` | 29.1 MB |
| `bin/gotest` + `bin/gohello` | 5.0 MB |
| a guest's linear-memory ceiling | **16 MB** (`hosts/macos/src/main.rs`: `max_pages = 256.max(min_pages + 32)`) |

A userspace server that *holds* the tree cannot hold this tree — it is three
times the ceiling, and two thirds of it is one file. Raising the ceiling is the
wrong answer twice over: it would put a 50 MB allocation in every host, and
browsers budget wasm memory per tab (§9.4's third finding — ~100 guests
exhausted Chrome's budget at much smaller sizes).

**The right answer is already in the plan, one step later.** P2 step 5 makes Go
and Python **packages** rather than rootfs residents. After that the base tree
is ~15 MB and the question is live again. So **step 2's deletions depend on
step 5**, and the plan does not say so — its step 2 lists only step 1 as a
dependency. Recorded here and marked in the plan rather than resequenced,
because the ordering is Christine's to change.

> **ANSWERED THE SAME DAY, and not by resequencing** (design.md 2026-09-04).
> `/` is a **project instantiated from the system template**, `/template/system`.
> A template is *declaration plus a skeleton*, and the skeleton is the
> **editable** part — while the bulk binds from the store as packages, by the
> rule already settled on 2026-09-02: *bind what stays shared, copy what
> becomes yours*. So the 34 MB of Go, Python and stdlib were never the root
> server's to hold. **The size problem and the boot path's naming problem had
> one answer**, which is the strongest evidence either was framed correctly.
> The dependency on step 5 stands — the packages must actually move — but it is
> a sequencing fact now, not a design obstacle.
>
> **And the bootstrap ordering went the same way.** Boot reads **`/namespace`**
> — the instantiated `/`'s own configuration, not the template — and **the host
> reads it**, because the host already owns the storage. So the third thing
> this step looked blocked on was not a problem either: no kernel addition, no
> `#/boot` equivalent, nothing to carry inside the kernel. Three obstacles, one
> answer, and the answer was a distinction already in the specs — **a template
> is a proto thing, and the instance is what runs.**

**The general form, which is worth more than the instance:** a step that moves
a responsibility from the kernel into a process inherits *that process's*
limits. "Userspace" is not a location, it is a budget — and the budget wants
measuring before the move is scheduled, not after it is attempted.

### 9.25 P2 step 5 measured before building — the plan cites a spec that says something else (2026-09-04)

Step 5 reads: *"the `pkg` design is spec'd ([type.md](docs/type.md)):
`/pkg/<name>/<version>` subtrees, bind-to-install, `/lib/pkg/registries`."*
**type.md does not say that.** It was replaced on 2026-09-02, and the plan's
sentence describes what `cmd/pkg.c` implements — pkg **v1** — rather than what
the spec it cites accepted the same day.

| | `cmd/pkg.c` today (v1) | `docs/type.md`, accepted 2026-09-02 |
|---|---|---|
| `/pkg/<name>` | a **directory** holding the installed bytes | a **declaration file** — *"a list of bindings plus commands"* |
| where bytes live | copied into `/pkg/<name>/<version>/bin/` | **`/store/<name>/<version>/`**, fetched once, verified, immutable |
| install | fetch + **copy** + bind the copy | fetch into the store if absent, then **bind from it** |
| remove | delete the subtree | **an unbind** — *"the entry survives, so re-installing is free"* |

**Why the difference decides whether step 5 can do its job.** Step 2 needs the
tree small enough for a userspace server to hold (§9.24: 49 MB against a 16 MB
guest ceiling). Under **v1, `pkg install python` copies 29 MB into the tree** —
the seed shrinks and the running tree does not, so the root server is no better
off. Only **bind-from-store** helps, because the bytes stay on the host and are
bound, never held. **The plan's own step 5, as written, cannot unblock the
step 2 it is supposed to unblock.**

**`#Z` is available where it needs to be**, measured from the suite's own
hostfs assertion: it passes under wasmtime *and* under the Rust core on Node;
only the frozen oracle self-skips. So a host-backed `/store` works on both
hosts that run the kernel. (A real browser needs a backing store for `#Z` —
OPFS or the loaded blob — and `#H` is gone as of P1 step 5, so it cannot fetch
either. That is the browser surface's problem, P5/P6, not this step's.)

**One genuine ambiguity in the spec itself, and it is not mine to settle.**
type.md's paragraph *"DECIDED at the stated lean — who serves `/store`"* then
states **no lean**: it weighs *host storage behind `#Z`* (simpler; the host owns
integrity) against *a userspace file server over a host directory* (integrity in
IPNX, one more process) and stops. The neighbouring visibility paragraph does
state its lean (*"I propose visible and read-only"*), so the omission reads as
an editing slip rather than a deliberate blank. **Both options keep the bytes
on the host**, so §9.24's arithmetic is satisfied either way — but only if the
server **streams** rather than holds, which is the same lesson as §9.24:
*userspace is a budget, not a location.*

### 9.26 pkg v2 — the declaration is the record, and a refused install must leave nothing (2026-09-04)

`cmd/pkg.c` moved from v1 (a directory of copied bytes under
`/pkg/<name>/<version>`) to what type.md accepted on 2026-09-02: **`/pkg/<name>`
is a declaration file**, the bytes live in `/store`, install is a **bind** and
remove an **unbind**. The three verbs are the spec's — `fetch`, `bind`, `env` —
over `/lib/namespace`'s little language, so nothing new was designed.

**Two properties are now observable that v1 could only claim:**

- **`ls /pkg` IS `pkg list`.** v1 kept a `/pkg/.installed` database; v2 does not,
  because the declarations *are* the record. One fewer thing to disagree.
- **Remove is an unbind, and the store entry survives** — asserted directly:
  after `pkg remove`, `pkg list` is empty and the store file still measures the
  source's exact byte count. That is what makes *"reinstalling is free and
  rollback costs nothing"* a fact rather than a promise.

**A defect the suite caught, worth recording because the fix is a rule.** The
first v2 wrote the declaration to `/pkg/<name>` and *then* applied it. A refused
install — the conflict case, *"a name that would bind over different bytes"* —
therefore left the declaration behind, and `pkg list` reported a package that
was never installed. The conflict assertion failed on exactly that. The fix is
ordering: **apply from the registry's copy, and record only once it has
worked.** Where the record IS the state, writing the record before the work is
a lie waiting for the work to fail.

**The gap raised here was endorsed and built — §9.27 below.**

**Measured, for the step this unblocks:** without `bin/python` (29.1 MB),
`lib/python3.14` (12 MB) and the two Go binaries (5 MB), the rootfs is
**3.4 MB** — comfortably under the 16 MB guest ceiling, where 15 MB would not
have been. So moving all three matters, and the stdlib is the one that needs
the tree form.

### 9.27 The tree form — one digest over a manifest (2026-09-15)

**Raised as a gap on 2026-09-04, endorsed on 2026-09-15** (*"yes, create a tree
form"*), which is the order the rule requires: undesigned work is proposed, not
invented. [design.md](docs/archive/design-log-claude-written.md) decision 122 carries the decision;
[type.md](docs/type.md) carries the format.

**The gap, stated precisely.** type.md's declaration sketch points a single
`fetch` digest at `/store/python/3.14` — a **directory** — and never said how
one digest becomes many files. Measured: CPython's stdlib is **539 files,
12 MB** (§9.24). 539 `fetch` lines would satisfy the letter of the format and
destroy the property it exists for: *"`cat /pkg/python` tells you what will be
fetched"* is not true of a file nobody reads.

**What was built.** A fourth verb in `cmd/pkg.c`:

```
tree  <manifest>  <sha256>  /store/python/3.14
```

The digest pins the **manifest**; the manifest pins every file. The manifest is
`sha256sum`'s own output — `<hex>  <path>`, one per line — which is why no
format was invented: `sha256sum bin/python lib/python3.14/os.py …` *is* the
authoring command, and `cmd/sha256sum.c` already printed that shape for its
named-file case.

**Three properties are load-bearing, and all three are asserted:**

- **The manifest is verified before it is read.** It is fetched into the store
  through the same streaming sink as any other entry, hashed on the way, and
  the pinned digest checked before a single entry is believed. A mismatch
  removes it. Nothing is trusted on its own word.
- **The manifest is kept in the store**, at `<entry>/manifest`. This is what
  lets `pkg verify` re-check all 539 files **offline, from the store, with the
  registry unreachable** — the plane test applied to verification rather than
  to installation. The suite proves it by `rm -r`-ing the registry between
  install and verify.

**Why not an archive**, which is what every other system does. It loses on the
constraint measured in §9.24 before it loses on anything else:

| | manifest | archive |
|---|---|---|
| memory | streams file by file; nothing held | 12 MB materialised then unpacked, inside a **16 MB** guest |
| the store | holds the tree, once | holds the archive *and* the tree |
| granularity | per-file digests — `verify` **names** the altered file | the digest covers the blob; a change is detectable, not nameable |
| vocabulary | none invented — `sha256sum` emits it | an archive format, plus an unpacker |

**The cost, stated rather than discovered:** `manifest` is the one name a tree
may not contain at its root, and an entry claiming it is refused with that
sentence.

**A hole closed before it shipped, and the asymmetry that creates it.** The
pinned digest proves the manifest is the one the packager published; it does
**not** make the manifest's paths benign. And the manifest is precisely the
half of the audit nobody reads line by line — that is the whole reason it
exists. So an entry reading `../../bin/rc` would have been fetched and written
outside the store entry, digest-verified and all. `pkg` now refuses any entry
that is absolute, empty, or carries a `..` component, on install **and** on
verify, and the suite asserts the refusal records nothing. *A digest
authenticates bytes; it does not authorise what they say.*

**One engineering note.** The declaration reader was byte-at-a-time — a syscall
per byte, which is nothing for a five-line declaration and 40,000 syscalls for
a 539-entry manifest. It is now a buffered reader that **carries its own
buffer** rather than using a static one, because `apply()` reads a declaration
while `dotree()` reads a manifest: the two are nested, and a shared buffer
would have interleaved them.

### 9.28 The host plumbing for `/store` — and a defect it exposed (2026-09-16)

**What the step needed.** `/store` must survive a boot, or `pkg install` is
worth doing once per boot rather than once. `#Z` was rooted at the rootfs dir
(`--live`) or a per-boot temp dir, so there was nowhere for it to live.

**The change is entirely host-side, and the kernel proves it.** `set_hostfs`
takes the directory and **discards it** — *"the HOST keeps the real root"* — so
the kernel only ever sees root-relative paths. `--host <dir>` therefore adds a
durable `#Z` root to `hosts/macos/src/main.rs` and `demo/supervisor/main-rust.mjs`
with **no kernel change at all**. The browser was already durable (OPFS or a
picked directory). No host knows the name `store`: what goes inside the
directory is IPNX's business, which is what keeps the host a storage box.

**Measured, not asserted — two boots:**

```
$ ... --host /tmp/ipnxstore -i   <<< "bind '#Z' /n/z; echo persisted > /n/z/store/demo/proof"
$ ... --host /tmp/ipnxstore -i   <<< "bind '#Z' /n/z; cat /n/z/store/demo/proof"
persisted-across-boots
```

A separate boot of the host read back what the first one wrote. The suite
proves the rest short of a reboot: `pkg` installs through `storefs` and the
bytes read back through the **raw `#Z` path**, so they are in the host's
directory and not in guest ramfs.

**A KERNEL DEFECT THIS EXPOSED, and `pkg`'s strictness is what caught it.**
The first run failed with `pkg: short write on /n/hs/hecho/1.0/bin/hecho`.
Measured: `MSIZE` is **8216** (8192 data + IOHDRSZ), `pkg`'s `sink` writes in
**16384**-byte chunks, and our `devmnt` write did **one** RPC clamped to
`MSIZE-24` and returned the short count. Plan 9 does not:
`plan9/sys/src/9/port/devmnt.c:688` (`mntrdwr`) **loops** — it clamps each
request to `m->msize-IOHDRSZ` and continues until `if(nr != nreq || n == 0)
break`. So a 16 KB write to a mounted file is written whole there, and a caller
never needs to know the connection's msize. Ours now loops the same way, with a
zero-length write still sending one RPC as it does there.

`pkg` was **left strict** — it still treats a short write as fatal. A program
that loops would have papered over the kernel bug and the suite would have
stayed green. The strictness is the detector.

**The frozen oracle keeps the v0 shortcut** (`poc/supervisor/mnt9p.mjs`: one
RPC, `data.subarray(0, MSIZE-24)`), so this property cannot be asserted there —
the assertion that covers it self-skips on the oracle anyway, for the
independent reason that the oracle has no `#Z`.

**A SECOND DEVIATION, MEASURED AND DELIBERATELY LEFT.** `mkdir '#Z/store'` is
refused with *"bad path"* while `cat '#Z/store/demo/proof'` works: `walk_parent`
rejects **every** `#`-rooted path for create. Plan 9 does not — `namec`'s
`Acreate` case (`plan9/sys/src/9/port/chan.c:1540`) acts on the walked parent
whatever the parent came from, so `create("#s/foo")` is ordinary there. It is
**not** fixed here, for a reason that is about governance rather than code: the
frozen oracle carries the identical refusal
(`poc/supervisor/kernel.mjs:379`), so changing the real kernel alone would put
it out of conformance with the reference over something nothing asked for, and
the workaround — `bind '#Z' /n/z` first — is the idiomatic Plan 9 form anyway.
One line each side if Christine wants them aligned. Recorded rather than done.

### 9.29 Step 3 whole, and the move DONE (2026-09-17)

**`/lib/namespace` is now `/namespace`.** `/` is a PROJECT instantiated from
`/template/system`, so the file boot reads is the **instance's own**
configuration and lives in `/` because `/` is the project ([design.md](docs/archive/design-log-claude-written.md)
2026-09-04). `init` reads it through `newns()`; the host put it there, which is
the whole answer to the bootstrap ordering.

**The `bind #w /dev/window` line STAYS, against the plan cell.** `#w` does not
leave until P4, and measured there are **53** uses of `/dev/window` in
`tests.rc` plus `rc/emca` and `cmd/emca.c`, all gating on that path resolving.
Dropping the bind early would not fail them — it would turn about ten live
assertions into silent *skips* with the PASS count unchanged, which is worse
than breaking loudly.

### The rc half was never a gap — Christine found it

I reported the boot script's name and location as a gap "reserved in
design.md". Her question — *"doesn't /rc contain boot sequence and command?"* —
was the correction, and the measurement settles it:
`plan9/sys/src/cmd/init.c:178` execs

```c
rc -c ". /rc/bin/termrc; home=/usr/$user; cd; . lib/profile"
```

on a terminal (`cpurc` on a cpu server). **`/rc/bin/termrc` is Plan 9's own
name at Plan 9's own location**, and `/rc/lib/rcmain` was already ours from the
identical convention. This is the fourth instance of one failure shape: hunting
for a name to invent while the reference the project explicitly follows already
names the thing. *An empty search means the wrong word* — and here it meant the
wrong question, because I searched the decision log for a gap instead of
`plan9/rc/`.

`init` cannot `exec` it as Plan 9 does — it is also the suite's driver — so it
**forks without RFNAMEG**, and `kernel/src/lib.rs:4769` is why that works:
without that flag the namespace is *shared*, "per rfork(2)". termrc's mounts
and binds therefore land in init's own namespace.

### The flip

`mk.sh` materialises the store outside `rootfs/` — a constraint, not a
preference, since `poc/run.sh` walks `userspace/rootfs` to build the frozen
oracle's seed and **may not be modified**, so anything inside it is loaded into
guest memory, the very thing the move exists to stop:

| | |
|---|---|
| `userspace/store/python/3.14` | 541 files, **41 MB** |
| `userspace/store/go/1.25` | 3 files, 5.0 MB |
| `userspace/pkg/{python,go}` | the declarations, five lines each |

and then **deletes them from the seed**. Measured: the rootfs goes from
**49 MB to 3.4 MB**, and the browser's packed `rootfs.json` from 65 MB to
**3.95 MB**. `rc/storeproof` passes five checks — the declarations are the
record, `pkg verify` is clean against the pinned digest, and REAL CPython and
REAL Go run **from the store**, streamed over 9P out of host storage.

**`pkg install` on an already-recorded declaration now REBINDS** rather than
re-materialising (`apply` act 3). Boot needs the bindings, not the bytes: the
store is durable and the declaration records the install, but a namespace is
per-process and empty at boot. Re-hashing 41 MB at every boot to discover what
`/pkg` already says would be absurd, and re-verifying is `pkg verify`'s job.

### Four defects this turn, each found by measurement

- **A leaked mount fid per failed lookup.** A walk holds one server fid per
  resolved path; `open` hands it to a chan and `stat` clunks it, but a path
  that is *dropped* never gave it back. CPython's import machinery misses on
  most candidate names, so `storefs`'s 64 fids were gone before the interpreter
  finished starting, and `/store` died for everything after. `walk()` now
  releases on both failure paths; `chdir` too. **`NFID` stays at 64** — a
  generous limit would have hidden the leak rather than fixed it.
- **`init`'s `await` stranded the suite.** `bootrc` first waited for termrc by
  awaiting, and init is *also* the suite's driver whose tests own their own
  children: the wait record it consumed belonged to one of them, and the suite
  deadlocked. It now forks `RFNOWAIT` and waits on a marker termrc writes —
  `bootrc=done` in `/env`, as Plan 9's own boot sets `$service`.
- **The marker was in `/tmp` first**, and `ls /tmp` is asserted exactly.
- **Mounting one posted channel twice hangs.** `/srv/store` is a single pipe,
  so a second `mount store /store` puts two devmnt clients on one wire with
  independent tag spaces. termrc now mounts only when `/store` is empty.

**What is NOT done, stated plainly.** After init's `newns` restore test the
namespace is cleared and the store goes with it, so the suite's Go and Python
tranches self-skip from that point on — `rc/storeproof` is what proves the
packages run, not the suite. Re-running termrc there would fix it, and a second
run is not reliably idempotent yet, so it is not done: **a hang in pid 1 is
worse than a skip**. And with `--host`, one legacy pkg assertion (`pecho`'s
whole life) fails against a pre-populated `/pkg`; the documented three-host
configuration is 156 PASS / 0 FAIL.

**The frozen oracle loses Python and Go, and this is inherent.** Packages live
in host storage reached through `#Z`; the oracle has no `#Z`, so it has no
store and cannot have packages. Its tranches self-skip. That is the cost of the
move, and it is the design's own consequence rather than an accident.

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

- **A Safari profile can lack the ServiceWorker API entirely (measured in
  the field, 2026-08-29).** Christine's Safari showed the isolation guard on
  a build fresh WebKit (iPadOS simulator) booted clean; two register
  hardenings changed nothing. The diagnostic guard settled it in one paste:
  `sw-api=false` — `navigator.serviceWorker` absent, so no registration
  dance was ever possible. Three Safari switches remove the API: Private
  Browsing windows, Lockdown Mode, and Privacy → "Block all cookies". On
  GitHub Pages the worker is the only source of COOP/COEP, so an SW-less
  profile cannot run the demo there at all — the guard now names the three
  switches; the only complete cure would be a host that sends real headers
  (a standing hosting decision, not a code fix). The method note: the fix
  that worked was making the ERROR SCREEN report its evidence — one paste
  then carried sw-api, controller state, registration state, and the build
  stamp, ending a three-round guessing game.

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

- **A presentation layer must coalesce frames (measured 2026-08-30: 640GB).**
  M3's first build emitted a full-window RGBA frame per draw WRITE; acme's
  boot makes thousands of small writes, the UI queue is unbounded, and macOS
  paused the app at 640.79GB of compressed queued frames. The cure is the
  classic one: a dirty set in the kernel and a host 30Hz tick draining ONE
  frame per dirty window (RSS flat ~245MB under acme thereafter). Two more
  M3/M4 measurements: `win acme` exposed that rc's `&` needs /dev/null —
  win(1) now binds the window BEFORE /dev so the union keeps the console's
  null; and first paint takes 90–150s on the native host because cranelift
  compiles acme's 20MB module at spawn (KDBG multiplies everything ~100x —
  a diagnosis run under it manufactured a wild-goose "hang"). Rust-host
  parity gaps noted while diagnosing — **all closed 2026-08-30**: '#s'
  (srv(3), JS-parity semantics, now suite-pinned on every host — before it,
  acme's post landed on a plain ramfs dir and nothing noticed), '#H' (the
  GET runs on a host thread via ureq/rustls with bundled roots, completing
  parked readers through the Effect/Ev shape timers use — `pkg install
  ruby` verified on the native host against the real registry over HTTPS),
  /proc root listing, and the WASI '#'-passthrough. Two more caveats closed
  the same pass: a content-keyed .cwasm cache (suite 7s cold → 2s warm;
  acme's 60–150s first paint becomes milliseconds on relaunch; deserialize
  is trusted because the cache holds only this binary's own writes), and
  `-lz` — no guest ar yet, so a package of bare objects ships a
  `libX.objects` list beside where libX.a would sit and cc(1) expands it
  (archive → objects-list → linker, in that order); `cc z.c -lz` links the
  registry's zlib.

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

### 9.8 The versioning layer and ar: two measurements (2026-08-30)

**A whole-root snapshot costs structure, never bytes.** The `#V` device
freezes the ram root by structural clone: every node copied shallow, every
data buffer shared (`Rc<Vec<u8>>` in the Rust core, a `dshared` mark in the
demo kernel), the live side copying a buffer only on its next write to it —
`Rc::make_mut` is the whole mechanism. Measured on the 710-node rootfs
(macOS host, release build): baseline max RSS 120,864,768 bytes; after
**twenty whole-root snapshots** 129,826,816 — **8,962,048 bytes for all
twenty, ≈448 KiB per snapshot, ≈630 bytes per node**, wall clock under one
second for the twenty (`date` unchanged across the loop). The rootfs
carries ~50 MB of file data; a copying snapshot would have spent ~1 GB.
Enforcement is one gate: `ram_access` refuses `want & 2` on an `ro` node
*before* the eve bypass — nobody rewrites history, eve included. The
follow-on caught in review: a device that synthesises directory listings
must derive `walk` and `read` from one entry table — the first cut listed
snapshots but not `ctl`, a ghost file (walkable, invisible to `ls`).

**wasm-ld links index-less archives.** `llvm-ar rcS` (S: no symbol table)
against `llvm-ar rc` on the same object: 532 vs 606 bytes; wasm-ld links
both without complaint — lld scans archive members itself, so the ranlib
table is optional. That measurement is the whole licence for `ar(1)` as a
~200-line guest command writing the plain GNU format (`!<arch>\n`, 60-byte
headers, even padding, no name table — members over 15 characters refused
honestly). `ar r libx.a x.o` then `cc main.c -lx` is the complete static
library story; pkg's `libX.objects` list convention stays for trees that
ship loose objects.

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

## 13. Prior art: package formats (researched 2026-09-02)

**Why measured:** the design needed to know whether a package is a *file* or a
*directory*, and the answer turns on what a package format actually carries.
Christine: *"Research existing package implementations before answering… That
will tell you what is needed, rather than me guessing on your behalf."*

| | Debian `.deb` | FreeBSD port | Homebrew |
|---|---|---|---|
| **shape** | control archive + data archive | **directory** | **single file** |
| declaration | `control` | `Makefile` | the formula (Ruby) |
| checksums | `md5sums` | `distinfo` | `sha256`, inline |
| file list | implicit in `data.tar` | `pkg-plist` | implicit |
| lifecycle | `preinst`, `postinst`, `prerm`, `postrm` — **four scripts** | `pkg-install` / `pkg-deinstall` | `install` method, inline |
| config protection | `conffiles` | — | — |
| patches | in the source package | **`files/`** subdirectory | **inline**, `patch :DATA`, or a URL |

**Sources:**
[Debian Policy §5, control files](https://www.debian.org/doc/debian-policy/ch-controlfields.html) ·
[FreeBSD Porter's Handbook](https://docs.freebsd.org/en/books/porters-handbook/porting-why/) ·
[Homebrew Formula Cookbook](https://docs.brew.sh/Formula-Cookbook)

**What the measurement decided** (disposition in the decision log,
[design.md](docs/archive/design-log-claude-written.md), 2026-09-02): **four of the five reasons a package
is a folder elsewhere are compensations for MUTATION**, and this system does not
mutate —

| the compensation | why it is unnecessary here |
|---|---|
| four maintainer scripts | installing is a **bind**; nothing to prepare or clean up — **overruled 2026-09-24**: Christine's package has *"potentially initialisation scripts (write out config files, set out environment etc.)"* (`verbatim.md`) |
| `md5sums` | the store is **immutable after verification**; files cannot drift |
| `pkg-plist` | removal is an **unbind**; the namespace is the installation record |
| `conffiles` | your `/home/<x>` **binds over** the system's — a different file in a union, never overwritten |

Only **patches** survive as a reason for extra files, and Homebrew demonstrates
they can be inline. So a package is a **file**, becoming a folder only when
patches grow large enough to warrant separate ones.

**Templates are the opposite, and the industry is unanimous there**:
cookiecutter, GitHub template repositories, Yeoman, degit and `.devcontainer/`
are all **directories**, because a project skeleton is files. The principle that
separates them: **bind what stays shared, copy what becomes yours** — a
package's content is shared and bound from `/store`; a template's skeleton
becomes the user's files and is copied.

**The finding exceeds the answer**: the format is small because it carries no
compensations. This is the project's *"complexity is compensation"* thesis with
the deleted parts enumerated and cited, rather than asserted.

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

---

## §10 — The deviation audit, 2026-09-18

Every structure and device implemented so far, read against `plan9/` line by
line. Christine asked for it *"no matter how small"*, after a day in which
seven deviations were found one at a time.

**Thirteen found.** Each is a fact about the source, not a judgement.

### `struct Chan` (`portdat.h`) — five fields absent

| | |
|---|---|
| **`flag`** | `COPEN CMSG CCEXEC CFREE CRCLOSE CCACHE` (`portdat.h:154–160`). **Code already written depends on it**: `srvwrite` refuses a posted fd carrying `CCEXEC\|CRCLOSE` — *"posted fd has remove-on-close or close-on-exec"* (`devsrv.c:323`); `srvclose` acts on `CRCLOSE` (`:286`); `pipewrite` suppresses the note when the pipe is a mounted queue, `(c->flag & CMSG) == 0` (`devpipe.c:348`) |
| **`iounit`** | *"chunk size for i/o; 0==default"*. `mntversion` caps the negotiated msize by it: `if(msize > c->iounit && c->iounit != 0) msize = c->iounit` (`devmnt.c:119`) |
| **`mux`** | the `Mnt` for clients using this channel for messages. `mntattach` reads `m = c->mux` and only versions when it is nil (`devmnt.c:317`). **Ours re-versions on every mount of the same channel** |
| **`mchan`, `mqid`** | the channel to the mounted server and the qid of the mount root |
| `devoffset`, `ismtpt`, `aux`, `umh`/`umc`/`uri` | union-read and cache state, for machinery not built |

### `struct Dev` (`portdat.h:241`) — two entries absent

| | |
|---|---|
| **`name`** | Plan 9's `Dev` carries `"cons"`, `"pipe"`. `/dev/drivers` prints `#%C %s` from `devtab[i]->dc` and `->name` (`devcons.c:198`). Ours has the letter and **the names were passed into `#c` separately**, so two places now hold the same list |
| `bread`, `bwrite` | block I/O. `mountio` writes with `bwrite`; ours uses `write` with a buffer |

### `rfork` (`libc.h:610`) — two flags absent

| | |
|---|---|
| **`RFREND` (1<<13)** | the rendezvous group. `sysrfork` does `up->rgrp = newrgrp()` (`sysproc.c:44`). There is no `rgrp` here |
| **`RFNOMNT` (1<<14)** | `up->pgrp->noattach = 1` (`sysproc.c:42`), and a copy carries it: `up->pgrp->noattach = opg->noattach` (`:38`). **This is the containment tool this project's own documents cite** — `rfork(RFNOMNT\|RFCNAMEG)` — and the namespace has no `noattach` |

Also absent: `noteid` (`up->noteid = incref(&noteidalloc)`, `:57`), which `RFNOTEG`
exists to allocate.

### `namec` (`portdat.h:144`) — three access modes absent

Plan 9 has seven: `Aaccess` **`Abind`** `Atodir` `Aopen` `Amount` **`Acreate`**
**`Aremove`**. Ours has four. `Abind` is *"for left-hand-side of bind"* and
ours uses `Atodir` for both sides; `create` is a separate function rather than
`Acreate`; `remove` does not use `Aremove`.

### `devroot` — boot files are in the wrong place

`rootdir[]` is `#/` and **`boot`, both directories** (`devroot.c:27`), and
`addbootfile` adds to `bootlist`, whose base is `Qboot` (`devroot.c:80`). **A
boot file is `#/boot/init`, not `#/init`.** Ours is flat, so `/init` is at the
root where Plan 9 has a `boot` directory.

### Limits not enforced

| | |
|---|---|
| **`ERRMAX` = 128** | `libc.h:553`, *"max length of error string"*. `sysexits` copies a status through `char buf[ERRMAX]` (`sysproc.c:668`). Our `errstr` and exit status are unbounded `String`s |
| **`NFD` = 100** | `portdat.h:476`, *"per process file descriptors"*. Our `Fds` grows without limit |

### A fourteenth, found by fixing the others

**`exec` clobbered the status the process set.** `sysexec` never returns in
Plan 9 — the process runs and `sysexits` sets the status (`sysproc.c:668`).
Here `touser` returns when the process is finished, and recording its return
unconditionally overwrote what the process had said on its way out.

It was invisible because the host test asserted only `is_ok()`. Strengthening
that test to check *what the guest read* — after the `#/boot` move left the
demo opening `/boot/` and printing a directory listing while the test stayed
green — exposed both at once.

### All fourteen are fixed, 2026-09-18

With a test each that fails if the behaviour regresses: a channel carrying
`CCEXEC`/`CRCLOSE` refused by `srvwrite`; a name opened `ORCLOSE` unposting
itself on close; a second mount of one wire joining its session rather than
sending a second `Tversion`; the wire's `iounit` bounding the session;
`RFNOMNT` setting `noattach` and both a copied and a cleared namespace
carrying it; `noattach` permitting exactly `"|decp"`; `ERRMAX` bounding an
error string and an exit status; `NFD` bounding the descriptor table;
`#/boot/init` rather than `#/init`; and a guest exiting with the bytes it read.

### The last two, closed 2026-09-18 — and two behavioural bugs behind them

**`namec` has all seven access modes now**, and adding them was not cosmetic:

| | |
|---|---|
| **`bind` refused a file** | `bindmount` resolves the source with `Abind` and the target with `Amount` (`sysfile.c:51`, `:60`). **Neither is `Atodir`.** Ours used `Atodir` for both, which requires a directory — so `bind /bin/rc /bin/sh`, binding a file over a file, was impossible |
| **a directory could be executed** | `Aopen` with `OEXEC` on a directory is *"cannot exec directory"* (`chan.c`), refused by `namec` because only it knows the mode the caller asked for. Ours went to the device, which read a directory |

`Acreate` also carries two checks made **before** anything is walked: a name
ending in `/` or `/.` must be created with `DMDIR`, and creating the root is
`Eexist`. Both are now in `create`, which is where `Acreate` goes because it
walks the parent (`e.nelems--`).

**`bread`/`bwrite` stay absent, and this is the one difference needing no
approval:** they take a `Block`, the kernel buffer the network stack and the
queues pass around. A device that cares handles one natively and saves a copy;
every device that does not gets `devbread`/`devbwrite`, which forward to `read`
and `write` and nothing else — `devtab[c->type]->write(c, bp->rp, BLEN(bp),
offset)` (`dev.c:418`). This kernel has no `Block` because it has no network
stack and no queues, so the pair would forward to methods callers already use.
Recorded at the trait, per the rule.

### What was checked and matches

The pipe's permissions (`0500` on the directory, `0600` on each end,
`devpipe.c:33`); the qid on the wire — one byte of type, four of version, eight
of path; `KNAMELEN` = 28 (`port/lib.h:171`); `readnum`'s width and trailing
space (`devcons.c:633`); `consdir[]`'s 23 names and permissions; `devpermcheck`'s
shift and mask (`dev.c:339`); `MAXRPC`/`MAXCMNRPC` and which one `mntversion`
asks for (`devmnt.c:19,118`); the three `rfork` flag-pair checks
(`sysproc.c:44–50`); `procdir[]`'s names and permissions; `capdir[]`'s two
write-only files.

---

## §11 — What building the userspace measured, 2026-09-19

P3 built a libc over the call list and ported Plan 9's rc to it. Everything
below was found by running the result, and every item is a fact with a file and
a line behind it.

### The toolchain, re-measured on this machine

| | |
|---|---|
| **`-fcommon` does not exist for wasm** | `clang --target=wasm32-unknown-unknown -fcommon -c a.c` → *"error: common symbols are not yet implemented for Wasm: runq"*. C89's tentative definitions — `int runq;` in every file that includes rc.h — therefore become ordinary strong definitions, and the link fails with a duplicate for each |
| **`llvm-objcopy` cannot weaken one** | for wasm it supports *"only flags for section dumping, removal, and addition"* |
| **`wasm-ld --allow-multiple-definition` keeps the FIRST** | measured both ways round with two objects, one holding `int Rcmain = 7;` and one holding `int Rcmain;`: `a.o b.o` → 7, `b.o a.o` → 0. **Ordering cannot fix it**, because `ipnx.c` initialises `Rcmain` and `Fdprefix` while `lex.c` initialises `doprompt`; whichever object goes first, the other's value is lost silently. That is how `havefork = 1` was once beaten by a zero |
| **the weak bit can be set in place** | `WASM_SYM_BINDING_WEAK` lives in a LEB128 flags field in the `linking` section's symbol table, and setting bit 0 of an even number never lengthens a LEB128 — so no section size moves. `userspace/weaken.py` does exactly this, leaving strong the ONE definition whose bytes are in a `.data` segment rather than a `.bss` one, which is the difference between an initialiser and a tentative definition |
| `-fms-extensions` | `port/pool.c` is written in kencc's anonymous struct members (`struct Free { Bhdr; … }`), and this is clang's name for them |
| `-fno-builtin` | still load-bearing, as §9.4 measured: clang rewrites `strlen`'s own body into a call to `strlen` |

### 9legacy's rc cannot be built without fork — three defects, none of them ours

`rc/haventfork.c` is Plan 9's own file for a system that cannot fork, and this
machine cannot (§5.2). **It has never been compiled against this edition of
rc**, and building it found three things, each provable by putting it beside
`havefork.c`:

| | |
|---|---|
| **`-S` is passed and not accepted** | `rcargv` starts every stage as `rc -S -c '<text>'` (`haventfork.c:22`), and `S` appears NOWHERE else: not in `exec.c`'s `ARGBEGIN`, not as a `flag['S']` read, not in the usage line. Measured over `plan9/sys/src/cmd/rc/`: one hit, the one that passes it. Every pipeline stage died on the usage message, into the pipe, unseen |
| **the backquote compiles the wrong subtree** | `code.c:147` — the fork branch compiles `c1`, the command; the no-fork branch emits `fnstr(c0)`, the SEPARATORS. For a bare `` `{…} `` c0 is nil, so the child was given nothing to run |
| **and the separator list is never popped** | `havefork.c:133` has `poplist(); /* ditch split in "stop" */` and reads `stop` from `runq->argv->words`; `haventfork.c` reads `vlook("ifs")` instead and pops nothing. The pushed list stayed on the argument list and BECAME the substitution's result — every `` `{…} `` answered with the value of `$ifs` |

### Six deviations in the kernel, all found by a program using it

| | |
|---|---|
| **`up` was not set on a system call** | Plan 9 gets it free: `syscall()` runs on the trapping process's own kernel stack. Here `up` is a shared cell, and nothing set it — so `up->egrp` in devenv, `up->fgrp` in devdup and `up->user` in devsrv, devmnt, devproc and devcap all answered for whoever ran last. Invisible until a second process existed |
| **`devcons` declared a second `Up`** | so `#c` read a different `up` than every other device. The same mistake as the namespace keyed by path text: the right word, a different thing |
| **`errstr(2)` is an EXCHANGE** | `generrstr` (`sysproc.c:748`) puts the caller's buffer into `up->syserrstr` and answers the old one, returning **0**, not a length. Ours read-and-cleared, which makes `werrstr` — which is nothing but an exchange (`9sys/werrstr.c`) — impossible |
| **`await` returned a pid and a status** | `sysawait` (`sysproc.c:727`) formats the whole message in the KERNEL: `"%d %lud %lud %lud %q"`, five fields that `wait(2)` splits back out with `tokenize` (`9sys/wait.c`). The `%q` matters: a status holding a space must come back as one field |
| **`rootreset` was missing** | `devroot.c:95` adds ten empty directories — bin, dev, env, fd, mnt, net, net.alt, proc, root, srv — and they exist so a first process can bind onto them. Without `/bin`, the first bind of any boot fails |
| **`create` of a name that exists must TRUNCATE** | `chan.c:1540`: `Acreate` walks the last element first and, if it is there, opens it `OTRUNC` unless `OEXCL`. Plan 9's comment names the very case that found this — *"it happens when two rc subshells simultaneously update the same environment variable"* |

### And the largest: a directory is not a list of names

**Every device here returned its directory as newline-separated text.** A Plan
9 directory reads as a run of `Dir` entries in `stat(5)` form, packed by
`convD2M` and parsed back by `convM2D` (`dev.c:306`, `devdirread`). Nothing in a
Plan 9 userland can read anything else — `dirread(2)` is the only way to list a
directory, and it calls `statcheck` on every entry.

It surfaced as rc's `Vinit` reading `/env` and getting nonsense: every variable
took another's value. Three further facts came with the fix:

| | |
|---|---|
| **the stat entries were short three strings** | `uid`, `gid` and `muid` were never written, and the leading `size[2]` was zero. `statcheck` (`9sys/convM2D.c`) checks both |
| **`mode` is `perm \| qid.type << 24`** | `devdir` (`dev.c:42`) sets the directory bit once, in the qid, and derives `DMDIR` from it |
| **an entry is never split, so `Chan.dri` is the position** | a read stops at the last WHOLE entry and the next resumes at the next ENTRY. A byte offset cannot say where one begins, which is why `sysseek` refuses any offset but 0 on a directory (`sysfile.c:820`, `Eisdir`) and clears `dri` (`:856`) |
| **and `pread` handed the device a COPY of the channel** | so `c->dri` went back with the copy and every directory read restarted at the first entry. Plan 9 passes the `Chan*` an fd holds; this kernel copied it and wrote one field back |

### Still different, and named rather than hidden

* **`eve` is `#c`'s, not the kernel's.** Plan 9 keeps it in `auth.c:10` and
  every device reads it for a file's group. Devices here that cannot reach `#c`
  use the boot value (`dev::EVE`), so renaming eve would not show in their
  `gid`.
* **`consdir[]` carries no lengths.** Plan 9's table gives `bintime` 24,
  `cputime` `6*NUMSIZE` and `hostdomain` `DOMLEN` (`devcons.c:606`); ours
  reports 0 for all twenty-three.
* **A child runs to completion inside `procrfork`.** One memory, two processes,
  one of them running — so a pipeline's stages are sequential and its pipe must
  hold a whole stage's output. rc starts the WRITING stage first
  (`haventfork.c:131`), so this is correct for two stages and bounded by the
  pipe for large ones.

---

## §12 — What building the console and the store measured, 2026-09-19

P4 put `rc` on a real console and a real filesystem. As in §11, everything
below was found by running the result.

### The console belongs in the kernel, and the plan said otherwise

`docs/implementation.md` had P4 build *"the console and storage as **userspace
file servers**"*. The reference says otherwise and that decides it: Plan 9's
`cons` and `consctl` are `#c`'s, in `port/devcons.c`, and what the machine
supplies is two named things —

| | |
|---|---|
| `screenputs` | `devcons.c:12`: `void (*screenputs)(char*, int) = nil`, a function pointer the architecture fills, called by `putstrn0` |
| the keyboard's characters | `kbdputc` (`devcons.c:525`) is called at interrupt time, stages runes, and `qproduce`s them into `kbdq`; `consread` blocks in `qread(kbdq, &ch, 1)` |

Everything between them — `kbd.raw`, backspace, `^U`, `^D`, the `kbdq`/`lineq`
split that makes a read answer one whole line — is PORTABLE code in
`port/devcons.c`. Putting the console in userspace would have been the
deviation, not avoiding one.

**The one difference, named where it is:** Plan 9's `consread` blocks while an
interrupt fills a queue behind it. This machine has no interrupts, so the
blocking read is a call outward, made at the same point with the same meaning.
A terminal that ends has no Plan 9 counterpart — a keyboard does not end — so
it is treated as the `^D` its user would have typed.

### `initcode.c` is the boot, and it is nine lines

`plan9/sys/src/9/port/initcode.c:21` is the whole of what a Plan 9 kernel's
first process does, and `startboot` here is the same nine lines: three opens
of `#c/cons` (**three opens, not dups** — each descriptor gets its own
offset), `bind #c /dev`, `bind #e /env` with `MCREATE`, `bind #s /srv`, then
`exec`. Three binds are added on top, each a boot script's job on Plan 9
rather than an invention.

Which measured a defect: **`bind(2)`'s `MCREATE` (`libc.h:559`) was being
dropped**, so `bind -c` did nothing and a create in a union landed wherever
the walk did.

### A fourth defect in 9legacy's no-fork path

Joining the three in §11, and found by asking the shell what it thought a
command's status was:

| | |
|---|---|
| **`addwaitpid` is never called** | `havefork.c:230` calls it after forking; `haventfork.c`'s `execforkexec` does not. So `havewaitpid` says no, `Waitfor` returns before it waits, `setstatus` is never reached — and `$status` keeps whatever it last held. **Every failing command looked like a succeeding one** to every `if` and `&&` in every script |

### `#9` — how a host filesystem reaches a mount

Plan 9's own answer, and it is in 9legacy: **`bootvirtio9p.c`**, a boot method
whose whole body is `open("#9/0", ORDWR)`, and `boot.c:171` mounting what it
returns. The device behind it is `pc/devvirtio9p.c:1227`, `Dev
virtio9pdevtab = { '9', "virtio9p", … }`, whose comment is this system's
situation word for word:

> *mount a host directory exported by qemu's `-device virtio-9p-pci` /
> `-fsdev local` directly over a virtqueue, with no network in the path.*
>
> *devmnt drives this chan like a tcp connection: it `write()`s a whole
> T-message and reads the reply back as a byte stream reassembled by the
> `size[4]` prefix.*

So the device marshals nothing. It is absent from `plan9-stock` — a 9legacy
addition — and configured into the shipped kernels (`pc/pcf:11`,
`pc/pccpuf:11`). Two details of its contract are load-bearing and are kept:
`v9open` is `Einuse` when the channel is already open (`:1110`), because one
reply stream cannot be shared; and `v9read` answers bytes of ONE R-message and
never spans two (`:1171`), which is what lets `#M` reassemble by the prefix.

Its 9P2000.u shim (`tshim`, `rfixup`) has no counterpart here: that exists
*"because qemu's 9pfs speaks only 9P2000.u"*, and the server this machine
provides speaks 9P2000.

### And the two defects mounting anything would have found

Both are in the same place — a channel is the only thing that can carry what a
server knows — and neither could be seen with a test server that ignores fids.

| | |
|---|---|
| **a walk threw away the fid it minted** | `Dev::walk` answered a QID and let the caller build the channel. `struct Dev`'s walk is `Walkqid* (*walk)(Chan*, Chan*, char**, int)` and **fills in `nc`** (`dev.c:169`), because for `#M` a walk mints a fid and the channel is the only place to keep it. Every channel through a mount therefore carried the mount ROOT's fid |
| **and `create` moves a fid** | so a create through a mount walked the server's root fid onto the new file, and nothing resolved through that mount again. Plan 9 takes `cunique` first (`chan.c:1611`) and says why: *"We need our own copy of the Chan because we're about to send a create, which will move it."* `cclone` is a walk of NO names (`chan.c:842`) |

`cunique` is taken before an open or a remove too (`chan.c:1479`), and at the
end of every walk (`:1118`). Here the equivalent place is `domount`: a channel
taken out of a mount table is shared by definition, and that is where the
clone goes.

### `close(2)` never reached a device

`sysclose` → `fdclose` → `cclose` (`sysfile.c:285`, `chan.c:490`), and the
last of those is `devtab[c->type]->close(c)`. This kernel dropped the
descriptor and told nobody, so **nothing any device does on a close ever
happened**: `consctl` never put the console back out of raw mode, `#9` never
took its server back. The count that decides it is the channel's own —
`if(decref(c)) return;` — because a dup shares the channel and closing one
name for it is not closing the file.

### Still different, and named rather than hidden

* **`Isatty` asks the file, not its name.** plan9.c uses `fd2path`, which this
  kernel does not have (`docs/syscalls.md`: *"a convenience over state the
  process already holds"*). The state it is a convenience over is a `stat`:
  `stat(5)` carries the device letter, and the console is the file called
  `cons` served by `#c` — which is what plan9.c's two string comparisons are
  both trying to establish.
* **`procctl`'s `close n` does not reach the device** (`devproc.c`), because
  `#p` cannot reach the device table. That is exactly what `devtab` being a
  global gives Plan 9 and what this kernel has in `namec` instead.

---

## §13 — What building the boot measured, 2026-09-20

P5 moved the boot out of Rust and into the system: `boot`, `init`,
`/lib/namespace`, `/rc/bin/termrc`. The embedding is now `initcode.c:21` and
nothing more.

### The chain, at file and line

| | |
|---|---|
| `initcode.c:21` | three opens of `#c/cons` (**three opens, not dups**), four binds, `exec`. This is the whole of the embedding now |
| `boot.c:151` (`nsinit`) | `bind("/","/",MREPL)`, `mount(fd, afd, "/root", MREPL\|MCREATE)`, **`bind(rootdir, "/", MAFTER\|MCREATE)`** — that last line is what makes the root a file server |
| `bootvirtio9p.c:20` | the whole of the method: `open("#9/0", ORDWR)` |
| `aux.c:125` (`srvcreate`) | post the root channel at `#s/boot`, so a namespace built later can mount it again |
| `boot.c:201` (`execinit`) | `"/%s/init"` with `$cputype` — **not `/bin/init`**, because `/bin` is a union `/lib/namespace` makes and nothing has read that file yet |
| `pc/main.c:250` | `ksetenv("cputype", "386", 0)` — the MACHINE names itself, and `$objtype` is init's copy of it |
| `init.c:23` | `readenv("#c/user")`, `newns(user, 0)`, then the loop: the first rc runs the startup, `manual` becomes 1, and every one after it is the bare interactive shell |
| `libauth/newns.c:35` | `/lib/namespace` — the file is opened BEFORE `rfork(RFENVG\|RFCNAMEG)` clears the namespace, and read from the open fd afterwards |

### Five deviations, and every one of them was in the union

A union was a word here and not a thing. `bind -a` put an element in a list
that nothing ever reached, and the first line of P5 — `bind -a /root /`, the
root becoming a file server — needs all five of these to work.

| | |
|---|---|
| **a walk never tried the other elements** | *"try a union mount, if any"* (`chan.c:1027`): when the first element has no such name, `walk` tries each one after it — `for(f = (f? f->next: f); f; f = f->next)` |
| **the directory itself was not in its own union** | `cmount` (`chan.c:707`): *"if this is a union mount, add the old node to the mount chain."* Without it `bind -a x /` does not ADD to `/`, it replaces it |
| **a union was not copied when bound onto a directory** | `cmount` (`chan.c:719`), and `Chan.umh` is kept by `namec`'s `Abind` for exactly this. `/root` is itself a union, so `bind -a /root /` bound one element of it and the server became unreachable |
| **`Amount` and `Atodir` must NOT step onto a mount** | `chan.c:1532`: *"When mounting on an already mounted upon directory, one wants subsequent mounts to be attached to the original directory, not the replacement."* A second `bind -a x /n` attached to the first bind's channel, so a union could never have more than one element. `Atodir` has its own reason (`:1522`): *"Directories (e.g. for cd) are left before the mount point, so one may mount on / or . and see the effect"* |
| **a union directory read only one element** | `unionread` (`sysfile.c:323`) opens each element in turn, answers as soon as one gives anything, and moves on when one runs out — `c->uri` and `c->umc`. `ls /` showed whichever element answered the walk, which once `/` is a union is most of the system missing |

### Four more, each found by something failing to boot

| | |
|---|---|
| **`exec` read the image at the device** | `sysexec` reads it through `devtab` (`sysproc.c:302`), which for a mounted file is the mount driver. Reaching the device directly worked while every binary was in `#/boot`, and stopped the moment `/bin` became what it is on Plan 9: a file server's |
| **`rootreset`'s ten directories listed the boot files** | `rootgen` (`devroot.c:116`) switches on `Qdir` and `Qboot` and generates nothing for the rest. Treating everything that is not `#/` as `boot` made `ls /` show the boot files under `#/root` as well |
| **`#c`'s own directory was "no such file"** | Plan 9's `consdir[]` has `"."` as its zeroth entry (`devcons.c:607`) and `devgen` skips it. This table holds only the files, so `Qdir` had to be answered separately — and until it was, `ls /dev` printed nothing and looked like an empty directory |
| **a number in `#c` never ended** | `readnum` takes the offset and answers 0 past the end (`devcons.c:640`). Handing the formatted number back whatever the offset made `cat /dev/pid` print the pid for ever |

### And one the union exposed in the kernel's own shape

`Call::Pread` held a `RefCell` borrow on the descriptor's channel across the
device call. That is Plan 9's `Chan*` handed to the device — and Plan 9 does
not mind that the device may reach the same channel again through the fd
table, which `dupgen` does for every open descriptor (`devdup.c:34`). `ls
/dev` with `#d` in its union is exactly that, and it panicked. The channel is
now copied out and the whole of it put back: copying only the offset back is
what lost `dri` and `uri` and made a directory read start over every time.

### A fifth defect in 9legacy's no-fork path

Joining the four in §11 and §12: `haventfork.c` re-executes `argv0` for every
pipeline stage, and `init` execs the shell as `"rc"` (`init.c:174`:
`execl("/bin/rc", "rc", nil)`) — a bare name, which Plan 9 can pass because
its rc forks and never has to find itself. Here `access("rc", 1)` fails and
every pipeline dies. `ForkExecute` now searches `$path` for a name with no
`/`, exactly as `execforkexec` searches for a command.

### §13.1 — `#ec`, and three things wrong where one was named (2026-09-20)

*"Why is #ec absent?"* — Christine, on a gap the P5 summary had just listed.
The answer was that I never implemented it, and the note in `main.rs` had
dressed that up as a finding: *"Naming it would be claiming something that is
not there."* Reading `devenv.c` turned up **three** defects, not the one that
had been named.

**1. The spec was discarded.** `envattach` (`devenv.c:64`) is six lines:

```c
if(spec && *spec) {
	if(strcmp(spec, "c") == 0)
		egrp = &confegrp;
	if(egrp == nil)
		error(Ebadarg);
}
c = devattach('e', spec);
c->aux = egrp;
```

Ours took `_spec: &str` and threw it away, so `#ec` attached the caller's own
environment and `#ewhatever` did too — **wrong in both directions at once**:
the configuration group was unreachable, and a misspelling Plan 9 rejects was
silently accepted. `envgrp` (`:369`) and `envwriteable` (`:377`) are the other
half, and both read `c->aux`: `confegrp` is *"the global environment group
containing the kernel configuration"* (`:16`), writable by eve alone and
readable by everyone.

**2. `Chan` had no `aux`.** Plan 9's `struct Chan` carries `void *aux`, the
device's own word on a channel, and `devclone` copies it. Ours omitted it, so
there was nowhere to put the group choice. A Plan 9 device keeps its state in
file-scope globals and `aux` points into them; a device here keeps its state
in its own struct, so what a channel has to carry is only *which* of that
state it means — `aux: u64`, and for `#e` that is the two-way choice Plan 9's
nil/`&confegrp` already is.

**3. The attach spec was being walked as a name** — the defect that would have
bitten whatever added the next spec. `dev::split` took the device letter and
called **everything after it** the path below, so `bind #ec /env` attached
`#e` and then looked for a file called `c`. Plan 9 takes the letter and the
spec together (`chan.c:1348`):

> ```c
> while(*name != '\0' && (*name != '/' || n < 2)){
> ```

Everything up to the first `/` is the device's business, and the `n < 2` is
`#/`, the root device, whose letter IS a slash. Two consequences worth
recording: `#/boot` parses as the root device **with the spec `boot`**, not as
a walk to `boot` — which is why Plan 9 only ever writes `#/` bare
(`pc/main.c:242`, and every other `main.c`), and why two of our own tests were
written against a path form Plan 9 does not use; and `devattach` builds the
path as `"#%C%s"`, so a channel to `#ec` says `#ec` and reports itself that
way.

**And `initcode.c:28` binds both**, which we had as three lines and not four:

```c
bind(ec, env, MAFTER);           /* #ec -> /env */
bind(e, env, MCREATE|MAFTER);    /* #e  -> /env */
```

The configuration reads through `/env` underneath the process's own group, and
`MCREATE` is on the second, so what you set is yours and the configuration is
never written by accident. `ksetenv(name, val, conf)` (`:386`) writes to
either — `"#e%s/%s"` with `conf?"c":""`.

**What this does not close.** `#ec` is empty, because what fills it on a Plan 9
machine is `plan9.ini` and this host has no counterpart. That is a gap, not a
decision — see §13.2.

### §13.2 — why there is no plan9.ini, and the two configurations (2026-09-20)

*"why is there no plan9.ini?"* — Christine, on the gap §13.1 left. Reading it
out turned up that **Plan 9 has TWO configurations with different lifetimes,
and this kernel had them crossed.**

**The build-time one: `$CONF`.** `/sys/src/9/pc/pcf` and its like — a `dev`
section naming devices, then `link`, `ip`, `misc`, `port`, `boot`. `mkdevc`
turns it into `devtab[]`, and `portmkfile:53` embeds the file's own bytes in
the kernel image:

```rc
{echo 'uchar configfile[]={'
 xd -1x $CONF | sed ...
 echo '};'} >> $CONF.c
```

That array is what `/dev/config` reads (`devcons.c:871`), and `mkdevc:187`
also emits `char* conffile = "<pwd>/<$CONF>"`, the path, which is the second
half of `$terminal`: `snprint(buf, sizeof(buf), "%s %s", arch->id, conffile)`
(`pc/main.c:250`). **We have this configuration** — it is `LETTERS` in
`hosts/ipnx/src/main.rs`, which is `mkdevc`'s input here.

**The boot-time one: plan9.ini.** `plan9.ini(8)`:

> When booting Plan 9 on a PC, the bootstrap programs described in 9boot(8)
> first read, via TFTP or a FAT filesystem on the boot disk, a file containing
> configuration information.

The bootloader leaves it in physical memory and the kernel picks it out of raw
bytes — `cp = BOOTARGS;	/* where b.com leaves its config */` (`pc/main.c:66`),
`BOOTARGS` being `CONFADDR+BOOTLINELEN` (`:25`). `options()` splits it on
newlines into `confname[]`/`confval[]`, and `main` exports it (`:257`): every
line to `#ec`, and the ones not beginning `*` to `#e` as well.

**The defect this exposed.** `/dev/config` returned the host's command line
and `$terminal` was `wasm <command line>`. That is plan9.ini's material put
where `$CONF`'s belongs — the two configurations crossed, and the one place a
plan9.ini equivalent could have gone was already spent. Fixed: `/dev/config`
is now `LETTERS` in `$CONF`'s own shape and `$terminal` is `wasm
hosts/ipnx/src/main.rs`.

**And why there is no plan9.ini.** A first draft of this entry said its
mechanism answers a bootstrap problem — that a PC kernel must be configured
before it can read a filesystem, so a program that runs earlier leaves the
answer in memory and vanishes. Christine: *"Isn't plan9.ini just the default
configuration so you don't have to specify it on the command line?"* **She is
right, and `boot.c` says so in its own comment.** `rootserver` (`boot.c:328`)
is the shape of the whole thing:

```c
/* look for required reply */
dprint("read #e/nobootprompt...");
readfile("#e/nobootprompt", reply, sizeof(reply));
if(reply[0]){
	mp = findmethod(reply);
	if(mp)
		goto HaveMethod;
...
/* create default reply */
dprint("read #e/bootargs...");
readfile("#e/bootargs", reply, sizeof(reply));
...
do{
	outin(prompt, reply, sizeof(reply));
	mp = findmethod(reply);
}while(mp == nil);
```

`outin` (`boot/aux.c:157`) prints `"%s[%s]: "` and reads a line: the prompt
with the default in brackets, and Enter accepts it. So **`bootargs` is the
default answer and `nobootprompt` skips the question entirely** — which is
what the man page says in as many words:

> **`nobootprompt=`** *root* — Suppress the `root from` prompt and use *root*
> as the answer instead.
>
> **`user=`** *user* — Suppress the `user` prompt and use *user* as the answer
> instead.

The hardware half — `etherX=`, `scsiX=`, `console=`, `*ncpu=` — was the
original purpose and `plan9.ini(8)` itself says it has mostly lapsed:

> The file is used by the bootstrap programs and the kernel to configure the
> hardware available, **although nowadays the kernel can usually detect the
> attached hardware by itself.**

So plan9.ini is **stored answers**, and the bootstrap ordering is how they get
delivered on a PC, not why they exist.

**Which restates our gap, and makes it smaller.** A configuration is the
answer to a question, and **this `boot` asks nothing**: one method
(`#9/0` — `bootvirtio9p.c`, whose whole body is that one `open`), no
authentication, no prompt, `rootdir` a `char*` in the file. There is no
`root is from (local, tcp, virtio9p)` to answer because there is one method,
and no `user` prompt because there is no factotum. `#ec` is empty because
nothing yet asks anything for it to answer — not because the machine has no
way to tell the kernel things.

**When it earns its place** is when `boot` gains a second method, or a user to
choose, or a root that is not the one the host handed over. Until then the
honest statement is that the mechanism is in place (`#ec`, bound, attachable,
eve-writable) and there is nothing to put in it.

**How it would arrive, when there is something.** Not the FAT file — the
branch above it in the same function (`pc/main.c:49`), for a multiboot loader
with no `plan9.ini` to read:

```c
if(mbi->flags & Fcmdline){
	q = (char*)KADDR(mbi->cmdline);
	p = BOOTARGS;
	while(*q && p < BOOTARGS+BOOTARGSLEN-1){
		*p++ = (*q == ' ')? '\n' : *q;
```

**The bootloader's command line, spaces turned into newlines, IS plan9.ini**
on such a machine, and `ipnx`'s argv is that command line. Note the
precedence: the cmdline is read *only* `if(BOOTARGS[0] == 0)` — the two are
alternatives filling one slot, not a default and an override.

**A second finding, from reading `glenda()` (`bootauth.c:56`).** `eve` starts
as the **empty string** (`pc/main.c:285`, `kstrdup(&eve, "")`), and the first
process's user is a copy of it (`:287`); `boot` then writes `#c/hostowner`
with `$user` or, failing that, `"glenda"`. `hostownerwrite` permits it because
`iseve()` compares two empty strings (`auth.c:19`, `:128`). Ours is
`pub const EVE: &str = "eve"`, fixed at compile time, and the comment above it
cites `auth.c:10` as `char *eve = "bootes"` — **which is not in this tree**;
9legacy has a bare `char *eve;`. So the boot-time hostowner is a hardcoded
constant where Plan 9 has an empty string a userspace program fills, and
`$user` is one of the questions plan9.ini answers. **Not fixed; recorded.**

### Still different, and named rather than hidden

* **`#ec` exists, and nothing fills it** (closed 2026-09-20, §13.1). The
  device and the bind are there; what a Plan 9 kernel copies into it is every
  line of `plan9.ini` (`pc/main.c:257`), and this host has no counterpart to
  `plan9.ini`. So `/lib/namespace`'s `$rootspec` and `$rootdir` are still
  absent and `boot` still has one root. **What the host's configuration is,
  is a gap** — the command line is not obviously it, and inventing one is
  not on.
* **`/` has no `MCREATE` element**, exactly as on Plan 9: `/lib/namespace`
  binds the root with `-a` and not `-ac`. A create goes in `/tmp`, which is
  the server's own directory and not a union.
* **init's loop ends.** Plan 9's goes round forever because a terminal does
  not end; input can end here, so the second exit is the end of the session
  rather than a reason to start a third — and `sleep`, which is what Plan 9
  puts in the loop to stop a spin, is one of the calls this kernel refuses.

## §14 — What the machine can actually do about scheduling (measured 2026-09-20)

Christine, on being told four calls want a scheduler this machine cannot
have: *"You can control WASM memory allocation and scheduling from host:
Sets initial/maximum boundaries; handles memory, grow requests, Controls
execution time limits, interrupts, thread pools, and request triggering.
Given we are building effectively a custom host runtime environment, you can
scope this out."*

**She is right, and the mistake was in the wrong half.** I had written that
*"this machine has no user stack the kernel can write"* and that *"a process
that is not running is not suspended but finished"*. Both are true of the
host **as built** — one guest at a time, run to completion inside one
`touser` — and neither is true of the substrate.

### The thing I had not read: where `sched` actually divides

`sched()` (`port/proc.c:119`) is sixty-seven lines and the switch is two of
them:

```c
procsave(up);
if(setlabel(&up->sched)){
	procrestore(up);
	spllo();
	return;
}
gotolabel(&m->sched);
```

`setlabel` and `gotolabel` are declared in **`port/portfns.h`** (`:328`,
`:128`) and implemented in **`pc/l.s`** (`:1000`, `:992`). `Label` is
`{ulong sp; ulong pc;}` and lives in **`pc/dat.h:51`** — a machine-dependent
type. So **the scheduler is Plan 9's portable code and the stack switch is
the architecture's**, exactly as `touser` is. A scheduler here is not a
deviation to be authorised; it is `port/proc.c` arriving, with this machine
supplying what `pc/l.s` supplies there.

### What wasmtime 39.0.2 provides, at file and line

Read in `~/.cargo/registry/.../wasmtime-39.0.2`, not from memory:

| Plan 9 | machine-dependent there | this machine |
|---|---|---|
| `setlabel`/`gotolabel` (`l.s:1000`, `:992`) | save and restore SP+PC | `Config::async_support` (`config.rs:429`) pulls in **`wasmtime-fiber`**: `Func::call_async` (`func.rs:1099`) runs the guest on a fiber, and a host function that suspends returns to the caller with the guest's stack intact |
| `hzclock()` → `sched()` | the clock interrupt | `Config::epoch_interruption` (`config.rs:705`) + `Engine::increment_epoch()` (`engine.rs:797`) from a timer + **`Store::epoch_deadline_async_yield_and_update`** (`store/async_.rs:125`) — the guest **yields and continues**; `epoch_deadline_trap` (`store.rs:1120`) is the other disposition, and is not the one wanted |
| `splhi`/`spllo` | cli/sti | nothing: one kernel thread, and an epoch check is the only preemption point |
| `procsave`/`procrestore` (`pc/fns.h:156`) | FP state | nothing: the fiber holds it |
| `mmuswitch` | page tables | nothing: a process is a `Store`, and `RFMEM` is sharing one |
| `segbrk`/`brk_` — **omitted from the call list**, *"memory is the machine's"* | | `Store::limiter` (`store.rs:932`) + `ResourceLimiter::memory_growing` (`limits.rs:69`): the machine decides whether a grow succeeds. `Config::memory_reservation` (`:1728`), `max_wasm_stack` (`:756`), `async_stack_size` (`:778`) |
| — | — | `consume_fuel`/`set_fuel` (`config.rs:591`, `store.rs:1026`): deterministic metering, an alternative to epochs with different properties |

Cargo features present in this version: `async`, `threads`,
`stack-switching`, `pooling-allocator`. We build with
`default-features = false, features = ["cranelift", "runtime", "std"]`, so
`async` is a feature to add, not a fork.

### What would come in, and all of it is Plan 9's

Line counts from `port/proc.c` (1,705 lines in total), because they are the
size of the claim:

| | |
|---|---|
| `sched` 67 · `ready` 31 · `runproc` 74 · `yield` 8 | the switch and the run queues |
| `sleep` 74 · `tsleep` 24 · `wakeup` 30 | `Rendez` — and `struct Rendez` is `{Lock; Proc *p;}`, two fields (`portdat.h:104`) |
| `postnote` 78 · `procctl` 39 | notes, and `#p/<n>/ctl`'s `start`/`stop`/`waitstop`/`hang` |
| twelve process states (`portdat.h:610`) | `Ready` `Running` `Queueing` `Wakeme` `Stopped` `Rendezvous` … |
| `notify(Ureg*)` 79 · `noted(Ureg*)` 90 (`pc/trap.c`) | machine-dependent, and the shape a wasm machine must find its own answer to |

About 425 lines of `port/proc.c`, plus the machine's half. **No invention:
every item is in `plan9/` at file and line, which is the test.**

### The one thing fibers do not buy

**`rfork(RFPROC)` still cannot return twice.** A fork duplicates an address
space *and* a stack; a wasm instance's memory can be copied, but a fiber's
stack holds host frames with raw pointers into that instance and cannot.
`procrfork` (`libthread/create.c:103`) stays, for the reason RESEARCH §5.2
gives. A scheduler makes processes *concurrent*; it does not make them
*forkable*.

### And what it costs

**The kernel must survive a suspend in the middle of a syscall.** Today
`Machine::touser` is handed a `&mut dyn Syscalls` — *"the kernel, lent for
the duration"* — and a blocking call would hold that borrow while its fiber
sleeps. That is the one structural change, and it is in the host/kernel
boundary rather than in anything Plan 9 designed.

Two shapes, and they are not equivalent:

* the kernel takes `&self` with interior mutability throughout — it already
  holds `procs`, `ns` and `fds` as `Rc<RefCell<…>>`, so this is the smaller
  edit and the one that hides a re-entrancy bug best;
* **the call answers "blocked" and the machine re-enters when it is
  readied** — which is what `sleep()` returning after `wakeup` *is*, and is
  therefore the shape with a counterpart.

Also measured: `async_support` makes every `Func::call` an `await`, so the
host needs an executor. A single-threaded one suffices and must be
preferred — the kernel is dependency-free and the browser host has to do the
same thing with JSPI or the stack-switching proposal, where there is no
wasmtime at all. **A trait method named for a fiber would be a boundary P7
cannot implement.**

Finally, `asyncify` and the `env.setj/longj/sjbuf` and `tsave/tjump/tdrop`
imports named in `when.md` are what a machine reaches for when it has no
stack switch. With one, they are unnecessary.

### §14.1 — How Plan 9 survives a suspend, and where it preempts (2026-09-21)

Two of the four decisions §14 put to Christine came back as *"what does plan
9 do?"* — which is her standing answer to a question that is a lookup. Both
were.

#### The kernel is entered per process, on that process's own stack

`Proc.kstack` (`portdat.h:661`, commented *"known to l.s"*) is `KSTACK` =
**4096 bytes** (`pc/mem.h:26`), allocated in `newproc` (`proc.c:724`:
`p->kstack = smalloc(KSTACK)`). A syscall runs on it. When it sleeps
(`sleep`, `proc.c:815`):

```c
procsave(up);
if(setlabel(&up->sched)) {
	/*  here when the process is awakened  */
	procrestore(up);
	spllo();
} else {
	/*  here to go to sleep (i.e. stop Running)  */
	unlock(&up->rlock);
	unlock(r);
	gotolabel(&m->sched);
}
```

The half-finished syscall's C locals stay where they are. `wakeup` readies
the process, `sched` does `gotolabel(&up->sched)`, `setlabel` returns 1, and
**the syscall continues from the line it stopped on**. There is no "blocked"
return value, no re-entrancy, and no lending: the kernel is not one object
that a process borrows, it is code every process runs on its own stack.

**So neither option §14 offered was Plan 9's.** The fiber is the kernel
stack: a host function that awaits keeps its Rust locals on the fiber
exactly as C locals stay on `kstack`.

**And the discipline that makes it safe is stated, with a diagnostic**
(`proc.c:821`):

```c
if(up->nlocks.ref)
	print("process %lud sleeps with %lud locks held, ...");
```

A process must not sleep holding a lock. `sched()` will not switch while one
is held — `up->delaysched++` and return (`proc.c:213`) — and `unlock`
(`taslock.c:216`) calls `sched()` the moment the last one goes: *"Call sched
if the need arose while locks were held."* **In Rust that rule is: no
`RefCell` borrow held across a suspension point.** Same rule, same reason,
and it is Plan 9's, not ours.

#### Plan 9 preempts, but never inside the kernel

`hzclock` (`portclock.c:136`) ends:

```c
if(up && up->state == Running)
	hzsched();	/* in proc.c */
```

and `hzsched` does **not** call `sched()`:

```c
/* unless preempted, get to run for at least 100ms */
if(anyhigher()
|| (!up->fixedpri && m->ticks > m->schedticks && anyready())){
	m->readied = nil;	/* avoid cooperative scheduling */
	up->delaysched++;
}
```

**The clock only marks.** The switch happens at three points the kernel
chose:

| | |
|---|---|
| `trap()`'s tail (`pc/trap.c:438`) | *"delaysched set because we held a lock or because our quantum ended"* — and only `if(clockintr && m->ilockdepth == 0)` |
| `syscall()`'s tail (`pc/trap.c:778`) | *"if we delayed sched because we held a lock, sched now"* |
| `unlock()` (`taslock.c:216`) | when the last lock is released |

So: **preemption is real and it is of user mode only.** That is an exact fit
for an epoch deadline, and the fit is not a coincidence — wasmtime's epoch
checks are compiled into **guest** code and never into a host function, so a
yield can only land where Plan 9's `trap()` would, never in the middle of a
syscall. The kernel half suspends only where an `await` is written, which is
`sleep`, which is the one place Plan 9 suspends too.

#### What this settles

* the suspend shape: **per-process stacks**, not a blocked-and-resumed call;
* the borrow rule: **`up->nlocks`**, spelled as no borrow across an await;
* preemption: **yes, and only of the guest** — the kernel is not
  preemptible, and the machine's yield lands exactly where Plan 9's does.

## §15 — The scheduler's deviation audit (2026-09-22)

Christine, on the switch landing: *"why are there differences from plan 9?"*

Every difference in P6, found by reading my own code against `port/proc.c`
rather than by remembering what I meant to write. **Two were inventions and
are already undone**; the rest are listed so she can strike any of them.

### Undone, because they were mine and not Plan 9's

| | |
|---|---|
| **`sleep` declined to sleep on an occupied `Rendez`** | `proc.c:826` prints *"double sleep called from …"*, dumps the stack, and **carries on**: `r->p = up` happens either way. I had written a branch that returned without sleeping. It now counts (`Procs.doublesleep`) where Plan 9 prints, because this kernel has nowhere to print from, and a test reads the count |
| **`checkalarms` was the wrong name for what it did** | `checkalarms` (`alarm.c:47`) walks `alarms.head` and wakes `alarmr` so `alarmkproc` can post notes for `procalarm`. **Nothing to do with `tsleep`.** Firing a due timer is `timerintr` (`portclock.c:172`), whose function for a `tsleep` is `twakeup` (`proc.c:897`) — `wakeup(p->trend)`. Renamed, and `Proc.trend` (`portdat.h:733`) exists now, set by `tsleep` and cleared by `twakeup`, where the code had been reaching for `p->r` instead |

### Cannot exist here — the narrow case, stated at each point

No approval needed by the standing rule, but each is a difference and each is
named in the code.

| | |
|---|---|
| no `Lock` on `Rendez`, no `splhi`/`spllo`, no `ilock` | one processor, one thread, no interrupts. `sleep` and `wakeup` cannot interleave |
| no `procsave`/`procrestore` | no FP state the kernel owns — the fiber holds it |
| no `mmuswitch`, `flushmmu` | no page tables. A process is a `Store` |
| `runproc` without `p->mp`, `p->wired`, `MACHP(i)` | affinity and load balancing across processors, of which there is one |
| `Label` is a fiber, not `{ulong sp; ulong pc;}` (`pc/dat.h:51`) | the machine's business either way, which is the point of it being in `pc/` |
| **`Left::Exited`** | a Plan 9 process leaves by a trap or a syscall and nothing else; `_start` there ends in `exits`. A wasm export can simply return, and a machine must say so or leave a process on the queue that will never run |
| **`schedinit` returns** | Plan 9's *"never returns"*, because its clock is an interrupt and there is always another. With no runnable process and no timer, nothing here can ever make one runnable |

### Forced by the substrate, and these are the ones worth striking

| | |
|---|---|
| **`Ret::Sched`** — the call answers *"leave"* where `sleep` does `gotolabel(&m->sched)` | Rust cannot jump out of a call. The effect is Plan 9's: the process's frames stay on its own stack (the fiber), and `sleep` releases everything first — which is literally what `proc.c:860` does, `unlock(&up->rlock); unlock(r);` on the line before it goes |
| **`procrfork`** instead of `rfork(RFPROC)` returning twice | RESEARCH §5.2, and it predates P6. A fork duplicates a stack, and no fiber's can be copied |
| **`rfork`'s `sched()` is in the host, not the kernel** | `sysrfork` ends `ready(p); sched();` (`sysproc.c`). The `ready` is in `Call::Rfork`; the `sched` is a yield in the `procrfork` import, because the child runs its few instructions on the parent's own instance first and the parent cannot leave until it has |
| **`Rid(pid, which)`** instead of `Rendez*` | Plan 9's `Rendez` are fields on their owner — `&up->sleep` (`portdat.h:720`), `&up->waitr` (`:683`) — and Rust cannot pass that address |
| **`exits` takes the process off the run queue** | Plan 9 never needs to: a process that exits is `up`, which `runproc` already dequeued. A `procrfork` child can be readied and end before the scheduler enters it |
| **the host ends a child whose `__childstart` returns** | our own libc already calls `exits("child returned")`; the host repeats it for a module that exports its own `__childstart`, so a raw guest cannot leave a process nothing can enter |
| ~~**`unsafe impl Send for Guest`**~~ — **struck 2026-09-22** | the kernel pointer is out of `Guest` and in a thread-local that `gotolabel` sets for the length of each poll — the counterpart of `up` and `m` being per processor (`pc/dat.h`), a thread being this machine's processor. `Guest` is `Send` by construction, which a test checks at compile time; a fiber resumed on another thread now stops with a message rather than using a kernel it does not own. It also fixed a real fault: the stored pointer was taken from the borrow of the first entry and reused on later ones after that borrow had ended |
| a timer is a field on the `Proc`, walked | **the field is Plan 9's** — `Proc` embeds a `Timer` (`portdat.h:732`, *"For tsleep and real-time"*). What differs is the list: Plan 9 keeps a sorted `timers[machno]` and `timerintr` walks it; one table is not a list worth keeping twice here |
| `anyready()` is `nrdy > 0`, not `runvec` | the same answer without the bitmask. `anyhigher()` needs the bitmask and is not built — it belongs to preemption, which is next |

### Present and inert, which is worth saying plainly

**`updatecpu` and `reprioritize` do nothing yet** — see §15.1 for why, which
is larger than it first looked. *(Resolved the same day: the clock is built,
§15.2.)*

### §15.1 — The missing clock tick (2026-09-22)

Christine: *"updatecpu and reprioritize are present and inert. why?"*

The first answer — *"`ticks` is never incremented … there is no clock
interrupt here until preemption"* — had the cause right and the dependency
backwards. **Preemption does not bring the clock; the clock brings
preemption**, as one of four things it does.

Plan 9's clock interrupt reaches `timerintr` (`portclock.c:172`), which calls
`hzclock` (`:196`) HZ times a second — 100 on the PC (`pc/mem.h:31`).
`hzclock` (`:136`):

```c
m->ticks++;
...
accounttime();
...
checkalarms();

if(up && up->state == Running)
	hzsched();	/* in proc.c */
```

and `accounttime` (`proc.c:1615`) is where two of the four inputs come
from:

```c
p = m->proc;
if(p) {
	nrun++;
	p->time[p->insyscall]++;
}
...
n = (nrdy+n)*1000;
m->load = (m->load*(HZ-1)+n)/HZ;
```

So one absence — **nothing calls `hzclock`** — accounts for four things
reported separately until now:

| `hzclock` does | fed | here |
|---|---|---|
| `m->ticks++` | `updatecpu` | `p->cpu` never decays |
| `accounttime()` | `m->load`; `p->time[TUser/TSys]` | `reprioritize` returns `basepri`; `/dev/cputime` charges nothing; `/dev/sysstat`'s load is zero |
| `checkalarms()` (`alarm.c:47`) | `procalarm` | `alarm` has nothing to fire it |
| `hzsched()` | preemption | nothing is preempted |

**Why it was shipped inert.** The P6 plan put the clock inside "preemption",
after the switch, and I wrote the kernel half from `port/proc.c` as one
piece before the thing that drives half of it existed. That was my
sequencing, not a constraint of the substrate: the machine already has a
clock (`Machine::todget`) and simply never calls into the kernel on a tick.

**What it needs** is the machine's half of `timerintr`: something that
enters the kernel's `hzclock` at HZ. Plan 9's is the i8253 or the local
APIC; here the same timer thread P6 planned for preemption — the one that
calls `Engine::increment_epoch()` — is the natural source, and it gives all
four at once. One thing to settle when it is written, by reading
`timerintr` and `hzclock` at the moment of writing: whether ticks that
elapse while no guest is running are counted as they would have been
(`hzclock` runs on every tick on Plan 9, idle or not — `accounttime`'s
`m->perf.inidle` is exactly the idle case) or lost.

**Settled by reading, when it was written** (§15.2): lost. `timerintr`
counts the HZ timer each time its loop finds it due — `callhzclock++` — but
only tests the count: `if(callhzclock) hzclock(u)` (`portclock.c:195`). An
interrupt that arrives late ticks once. Idle ticks are counted because on
Plan 9 the clock interrupts an idle processor too, and here the idle loop
waits a tick at a time for the same reason.

### §15.2 — The clock, and what building it exposed (2026-09-22)

Christine: *"continue P6"* — the next row was the clock.

**What was read, at file and line, before each piece was written:**
`portclock.c` whole (`tadd` `:26`, `timeradd` `:96`, `hzclock` `:136`,
`timerintr` `:169`, `timersinit` `:221`); `proc.c` `schedinit` `:67`,
`sched` `:119`, `anyhigher` `:194`, `hzsched` `:203`, `preempted` `:222`,
`updatecpu` `:279`, `reprioritize` `:318`, `yield` `:454`, `rebalance`
`:471`, `runproc` `:507`, `newproc` `:672`–`:735`, `accounttime` `:1615`,
`pexit` `:1145`–`:1168`; `alarm.c` whole; `pc/trap.c` `trap` `:315`–`:445`,
`intrtime` `:271`, `syscall` `:660`–`:781`; `pc/i8253.c:228`–`:265`;
`kw/clock.c:46`; `pc/dat.h:206` (`struct Mach`); `portdat.h:992` (`struct
Perf`); `devcons.c:873`, `:1082`; `sysproc.c:196`–`:206`, `:564`–`:569`;
`devpipe.c` whole; `qio.c` `qwait` `:849`, `qread` `:1070`, `qbwrite`
`:1165`, `qwrite` `:1270`, `qclose` `:1386`, `qhangup` `:1418`, `qreopen`
`:1447`; `pgrp.c:207` (`closefgrp`); `chan.c:517`–`:564` (`clunkq`).

**Built:** `Mach` with the fields `port/` reads; `timersinit`, `timerintr`,
`hzclock`, `accounttime`, `hzsched`, `anyhigher`, `rebalance`; `sched`'s
tail with `m->schedticks`; the machine's `clockintr` as wasmtime's epoch
callback, fed by a thread that moves the epoch on at HZ; and the
interrupt's tail yielding when `up->delaysched` is set.

**Found wrong while reading, and made Plan 9's:**

| | Plan 9 | was |
|---|---|---|
| a child's priority | *"p->basepri = up->basepri; p->priority = up->basepri"* (`sysproc.c:203`) | `PriNormal` |
| a child's `lastupdate` | `MACHP(0)->ticks*Scaling` (`proc.c:731`) | 0 |
| `exec` from `#/` | `PriRoot` (`sysproc.c:567`) | nothing |
| `runproc` | marks what it takes `Scheding` (`:569`); `sched` clears `m->readied` after comparing (`:174`) | `runproc` cleared `readied` itself, so `schedticks` could not know |
| `schedinit` on entry | *"if(up) { … updatecpu(up); up = nil; }"* (`:105`) — whoever was `up` stops being it | only a process `gotolabel` returned was dealt with, so a stale `up` would have been charged idle ticks |
| `/dev/sysstat` | ten numbers from `Mach` (`devcons.c:873`) | eight, and the two counted were fields of `#c` nothing set outside a test |
| **`pipe`** | `qread` sleeps on `q->rr` until data or hangup (`qio.c:866`); `pipeclose` counts opens and hangs up the other end on the last (`devpipe.c:247`); a read answers one block (`qio.c:1125`) | a byte buffer: an empty read answered 0 — end of file — and `close` did nothing |
| **`pexit`** | `closefgrp(fgrp)` (`proc.c:1160`) | the table was dropped and no device heard |
| `implementation.md`'s *"user mode only"* | `trap.c:438` has no `user` test — Plan 9 preempts its own kernel at a clock interrupt when no ilock is held | stated as Plan 9's rule; it is this machine's limit |

**The pipe was found by the clock.** A pipeline test failed one run in six:
`tr` read end of file while the `rc` that would run `echo` still held the
write end. Every pipeline had worked only because `rfork`'s `sched` ran the
writer first; preemption let the reader in first, and a pipe that could not
block told it the pipe was over. The fix is Plan 9's — the device sleeps the
caller on the queue's `Rendez` — and the call then answers `Ret::Sched`: the
syscall layer finds the caller `Wakeme`, the machine leaves, and the read is
made again when the process is entered. `Rid` gains `Rr(dev, devno, q)` for
`&q->rr`, the same adaptation `Rid` already was for `&up->sleep`. 30 of 30
runs pass after it.

**Differences, as first built** — listed here the same day, and then
removed rather than put to Christine: *"I don't understand why you are asking
or refusing work when the brief is clear."* The brief is Plan 9; a
difference that is not forced by the machine is a fault to fix. What each
became:

| as first built | now |
|---|---|
| a tick lands only in guest code, so `TSys` is never charged | **Plan 9's.** A call runs to its end with nothing able to interrupt it — Plan 9's kernel at `splhi` — and an interrupt held off by `splhi` is taken at `spllo`: at the call's end, still `insyscall`, so the tick is `TSys`'s. `p->insyscall` is set and cleared where `syscall()` does it (`pc/trap.c:674`, `:767`) |
| `!up->fixedpri` omitted from `hzsched` | **Plan 9's.** `p->fixedpri`, `procpriority` (`proc.c:772`), and `/proc/n/ctl`'s `pri` and `fixedpri` (`devproc.c:1373`, `:1379`) |
| **a call that slept was made again from the top** | **Plan 9's: it carries on.** `Kernel.labels` is `p->sched` — the rest of a call that left the processor, run when the process is entered again, so nothing before the `sleep` happens twice. The machine gains one method, `Syscalls::resume`, which is `setlabel` answering 1 (`proc.c:830`); every import goes through it, because any call may end in `sched()` (`pc/trap.c:778`, now honoured) |
| no pipe flow control | **Plan 9's.** `qbwrite` queues, then sleeps on `q->wr` until `qnotfull` (`qio.c:1250`), at `conf.pipeqsize` = 32K (`devpipe.c:50`), counting `BALLOC` (`allocb.c`); `qread` wakes it below half (`qwakeup_iunlock`, `:996`) |
| no `q->rlock`/`q->wlock` | **Plan 9's.** `QLock` (`qlock.c:17`, `:69`) — a second reader waits `Queueing` and is handed the lock |
| `qbwrite`'s *"let it run"* omitted | **Plan 9's.** A writer that wakes a higher-priority reader `sched()`s, still `Running`, and finishes when entered again (`qio.c:1234`) |
| `Proc.time` in milliseconds | **Plan 9's: ticks.** Converted on read (`devcons.c:815`, `devproc.c:875`); `TReal` from the tick the process was made on (`sysproc.c:193`) — it had started at `exec` |
| `clockintr` does not yield while a `procrfork` child is on its parent's frames | **stays** — forced by `procrfork` (§5.2): the child cannot be entered anywhere else before its `exec`. It is Plan 9's own delay: `sched` does not switch while it cannot, and `delaysched` keeps counting (`proc.c:145`). The switch after an `rfork` is `sysrfork`'s own (*"ready(p); sched();"*), taken once the child has run, and that `sched` zeroes `delaysched` before `syscall()`'s check (`proc.c:154`, `pc/trap.c:778`) — so the check is not made after `rfork(RFPROC)` |
| `clunkq` drained by the kernel, not a `closeproc` kproc | **stays until notes** — it exists for `/proc/n/ctl`'s `kill` and `close`, which run inside `#p` where `devtab` cannot be reached. `kill` becomes the note Plan 9 posts, and the victim's own `pexit` closes directly |
| `checkalarms`, *"sys: write on closed pipe"* | **notes** — the next step |

**Found while removing them, and made Plan 9's too:** the wait message is
*"%s %lud: %s"* — text, pid, exit string — or empty (`proc.c:1195`), so
rc's `$status` after a failed `cat` begins `cat 9:`; `p->text` exists
(`*init*`, the parent's, the `exec`'d file's last element — `pc/main.c:286`,
`sysproc.c:195`, `:483`); `pexit` adds `utime` and `stime` to the parent's
`TCUser` and `TCSys` and **nothing** to `TCReal` (`proc.c:1189`–`:1206`) —
it had added all three slots one for one; `/proc/n/status` is `readstr` and
`readnum` fields with nine numbers (`devproc.c:869`–`:890`) — it was
left-justified with three and a hard-coded `init`; and `/proc/n/ctl`'s
`close` and `closefiles` now reach the device.

**One thing the resumption exposed, and its fix is Plan 9's order.** With
`trap.c:778` honoured, an `rfork`'s own call could end in a switch before the
child had run on the parent's frames — and the scheduler then chose a child
with nothing to enter (one pipeline run in twenty-one). Plan 9 cannot reach
that state because `sysrfork`'s `sched` has already zeroed `delaysched` when
`syscall()` checks it; the kernel now reads the order the same way.

### §15.3 — Notes (2026-09-23)

**Read before writing:** `proc.c` `sleep` `:815`, `tsleep` `:910`, `wakeup`
`:942`, `postnote` `:981`, `pexit` `:1123`–`:1225`, `kproc` `:1436`,
`procctl` `:1484`; `pc/trap.c` `trap` `:315`–`:445`, `syscall` `:767`–`:780`,
`notify` `:788`, `noted` `:872`; `sysproc.c` `sysrfork` `:80`–`:109`,
`:188`, `sysexec` `:575`–`:585`, `sysalarm` `:658`, `sysnotify` `:782`,
`sysnoted` `:791`; `alarm.c` whole; `chan.c` `ccloseq`/`closeproc`
`:510`–`:575`; `pgrp.c` `pgrpnote` `:16`; `devproc.c` `Qnote`/`Qnotepg`/
`Qnoteid` `:418`–`:455`, `:792`, `:990`, `:1054`, `:1115`–`:1145`, `CMkill`
`:1352`; `pc/main.c:264`; `devcons.c` `pprint` `:322`, and its keyboard
paths `:470`–`:560`, `:755`–`:800`; `rc/plan9.c:513`–`:541`;
`portdat.h:331`–`:340`, `:624`, `:638`, `:675`, `:721`–`:754`; `error.h`
(the strings are its comments, turned into `errstr.h`).

**Built, all of it Plan 9's:** the note queue on the `Proc` (`NNOTE`,
`notified`, `lastnote`, `notepending`, `noteid`, `procctl`); `postnote`,
which wakes a sleeper; `sleep`'s refusal to commit with a note pending and
its `Eintr` on the way out, honoured in `sleep`, `await` and the pipe's
`qread` and `qbwrite` (whose `waserror`s let go of their `QLock`s);
`notify`'s decision, taken where `syscall()` takes it (`:773`, never after
`rfork`); `noted`; `alarm`, `procalarm`, `checkalarms` and **the `alarm`
kernel process**; `ccloseq` and **the `closeproc` kernel process**;
`pgrpnote`; `RFNOTEG`; `#p/<n>/note`, `notepg`, `noteid`; `kill` as
*"p->procctl = Proc_exitme; postnote(p, 0, "sys: killed", NExit)"*;
`pprint`'s *"suicide: …"*; and `exec` clearing the notes and the handler.

**A kernel process is a process whose body is kernel code.** Plan 9 makes
one with `kproc`, and `kprocchild` starts it in the function it was given.
Here that body is what `p->sched` already holds for a call that left — the
rest of what it was doing — so entering a kproc is running that, and it
keeps itself there each time it sleeps. `alarm` is pid 2, as `init0` starts
it before the first process reaches user mode.

**The machine's half of `notify(Ureg*)`** is this machine's own and says
so: the note is written onto the process's stack below its stack pointer
(*"sp -= 256; … memmove((char*)sp, up->note[0].msg, ERRMAX)"*), which here
is the exported global `__stack_pointer`, and the handler is entered through
the image's `__notestart` — the way `procrfork`'s child is entered through
`__childstart`. `noted(NCONT)` unwinds the handler's frames back to where
the machine entered it and restores the stack pointer; the frames below are
the interrupted context, still there. A handler that returns instead of
calling `noted` has returned into nothing, which is a fault on Plan 9 and is
one here.

**Cannot exist here, each stated in the code:**

| | why |
|---|---|
| no `Ureg`; `__notestart`'s `ureg` is nil | no register set a guest can see |
| *"sys:"* notes gain no *" pc=0x…"* (`pc/trap.c:809`) | no program counter to report |
| a fault ends the process even with a handler | the trapped instruction cannot be gone back to — a handler could only `noted(NCONT)` into it |
| at a clock interrupt, a note for a handler waits for the end of the process's next call | the guest can be entered from a host call and from nowhere else; an epoch callback is not one. A note that ends the process does not wait: the callback traps out of the guest |
| no note is taken at an interrupt while a `procrfork` child is on its parent's frames | the same frames, the same reason as the switch (§15.2) |

**Found, and not the kernel's: `^C`.** The P6 acceptance test reads *"`^C`
interrupts a command"*, and **Plan 9's kernel console turns no key into a
note** — `devcons.c`'s keyboard paths know `^T^T`'s debug keys and nothing
else (`:470`–`:560`). The interrupt a Plan 9 user types is `rio`'s: it
writes *"interrupt"* to the window's process group's `notepg`. So the
kernel side is done — a note written there interrupts a command, and a test
shows rc catching one — and the key is a job for whatever plays `rio`'s
part: the CLI host's terminal, and emca.

### §15.4 — `rendezvous`, the process controls, `Broken` and `psstate` (2026-09-23)

**Read before writing:** `sysproc.c` `sysrendezvous` `:910`–`:946`,
`sysrfork` `:153`–`:171`, `sysexec` `:587`; `portdat.h` `Rgrp` `:472`–`:497`,
`psstate` `:671`, `pdbg` `:711`; `proc.c` `postnote`'s rendezvous branch
`:1039`–`:1057`, `NBROKEN`/`addbroken`/`unbreak` `:1061`–`:1104`, `pexit`
`:1227`, `procctl` `:1484`–`:1522`; `devproc.c` `procstopwait` `:1223`,
`procctlreq` `:1321`–`:1440`, `procstopped` `:1501`, `status` `:865`;
`port/systab.h:114` (`sysctab[]`).

**Built as read.** `rendezvous` finds the first waiter with the tag in the
caller's `Rgrp`, swaps values and readies it, or waits `Rendezvous` to be
found; the group is shared across `rfork` unless `RFREND`; `postnote` pulls
a waiter out answering `~0`. `stop` is `procstopwait(p, Proc_stopme)` — the
writer sleeps on its own `sleep` until the process, on its way out of the
kernel, reaches `procctl`, becomes `Stopped`, wakes `pdbg` and leaves the
processor; `waitstop` waits without asking; `start` readies a stopped
process; `hang` makes the next `exec` stop; `kill` of a `Stopped` process
readies it to die and of a `Broken` one `unbreak`s it. `pexit` with
`freemem` false — a note that was `NDebug`, a *"Suicide"* — keeps the
process `Broken`, at most `NBROKEN` (4); there is no `*nobroken` to say
otherwise. `psstate` is set to `sysctab[]`'s name on the way into a call
and cleared on the way out, so `/proc/<n>/status` — and `ps` — shows
`Await`, `Pread`, `Sleep`, `Stopwait`, `Stopped` as Plan 9's does; it had
shown only the scheduler state.

**A stop at a clock interrupt** leaves the processor by the same yield as a
preemption, still `Stopped`, so the scheduler does not put it back; `start`
readies it and it goes on from the instruction it was at.

**Not built:** tracing — `startstop`, `startsyscall`, `/proc/<n>/syscall`,
`profile`. (Built later the same day: §15.8.)

### §15.5 — `^C`, and a console that does not block (2026-09-23)

Christine: *"^C should be received by host app and then sent to relevant
process as a signal"* — answering §15.3's finding that Plan 9's kernel
console turns no key into a note.

**Read before writing:** `rio/wind.c` `interruptproc` `:436`, DEL `:651`;
`init.c` `fexec` `:127`–`:170`, `pinhead` `:119`; `devcons.c` `kbdputc`
`:525`, `kbdputcclock` `:556`, `consinit`'s `addclock0link` `:671`,
`consread` `:762`–`:800`.

**What "the relevant process" is, from Plan 9:** a note **group**. `rio`
writes *"interrupt"* to the window's `notepg`, so the shell in the window
and everything it runs get it; rc's handler survives and the command dies.
`init` gives the shell a group of its own — *"rfork(RFNOTEG)"* in `fexec` —
and catches notes itself with `pinhead`, so an interrupt never ends `init`.
Ours did neither (it predated notes); it does both now. The console's group
is the group of the process reading it, which is the shell's.

**Built:** the host catches `SIGINT` (the terminal's cooked mode turns `^C`
into it) instead of dying of it, and reports it through `Console::interrupt`;
`kbdputcclock` — the console's clock routine, every 22ms as there — posts
*"interrupt"* to the group and drops the typed-ahead line, as `rio` does
(*"w->qh = w->nr"*).

**Found while doing it: the console blocked the machine.** `consread` asked
the host for keys with a blocking `read_line`, inside the kernel, so while
the shell waited at its prompt nothing else ran and no note could be taken.
Plan 9's `kbdputc` stages keys at interrupt time, `kbdputcclock` takes them
in at clock time, and `consread` sleeps in `qread(kbdq)` under
`qlock(&kbd)`. That is what it does now: the host reads the terminal on a
thread of its own, `Console::kbdchars` answers without waiting, and the
reader sleeps and is woken by the clock routine. The idle loop keeps the
clock running while a reader waits for a key.

Verified on a real terminal (a pseudo-terminal driven by a script): `sleep
30`, `^C` three seconds later, `echo after` — the sleep ended at once and
the host went on.

### §15.6 — `date` printed 0 and -1: kencc's promotion rule (2026-09-23)

`docs/when.md` recorded *"`pread` with an explicit offset is not
reliable"*, diagnosed as an uninitialised buffer with *"a timing
signature"*. **That diagnosis was wrong, and the kernel and `pread` were
never at fault.** The bytes arrived every time; the arithmetic on them
differed.

`nsec.c:4` (vendored verbatim) assembles each 32-bit word of
`/dev/bintime` from `uchar`s:

> `#define	U32(x)	(((((((x)[0]<<8)|(x)[1])<<8)|(x)[2])<<8)|(x)[3])`
> `return (u64int)U32(b)<<32 | U32(b+4);`

**kencc promotes `uchar` to UNSIGNED int.** `cc/sub.c:688`–`:691`, in
`arith`:

> `/* convert up to at least int */`
> `while(k < TINT)`
> `	k += 2;`

and `cc.h:307` orders the types `TCHAR TUCHAR TSHORT TUSHORT TINT TUINT`,
so `TUCHAR` steps to `TUSHORT` and then to `TUINT`. clang follows ANSI C
and promotes `uchar` to `int`. So here `U32(b+4)` is signed, and the `|`
with a `u64int` sign-extends it: whenever bit 31 of the low word is set,
the high word becomes all ones, `nsec()` is a small negative number, and
`time()` is 0 or -1. That bit flips every 2³¹ ns — 2.1 seconds — which is
the "timing signature": a write to stderr between calls moved the reading
into the other half.

**Fixed at the site:** `(u32int)U32(b+4)`, commented with the rule. The
other vendored places that widen bytes to 64 bits are safe: `fcall.h`'s
`GBIT64` casts its low half to `u32int`, and `byteserial.c`'s `legetvl`
and `begetvl` go through functions returning `uint`. **Any further vendored
code carries the same exposure** — an expression of `uchar`s that is
widened past 32 bits compiles differently here — and is checked when it
arrives. Test: `date_reads_the_clock_every_time`, which fails on the old
file.

### §15.7 — The semaphores, which were never a library (2026-09-23)

`docs/syscalls.md` listed `semacquire`, `semrelease` and `tsemacquire` as
omitted: *"`rendezvous` is the primitive; semaphores are a library over
it"*. **Plan 9 disagrees**: they are calls 37, 38 and 52
(`9syscall/sys.h`), with bodies in the kernel — `syssemacquire`
`sysproc.c:1187`, `systsemacquire` `:1206`, `syssemrelease` `:1225`. A
kernel without them is smaller than Plan 9's, which is a deviation.
Christine: *"do 1-5"*, the first being to build them.

**Read before writing:** `sysproc.c:954`–`:1240` entire (the comment, then
`semqueue`, `semdequeue`, `semwakeup`, `semrelease`, `canacquire`,
`semawoke`, `semacquire`, `tsemacquire` and the three calls);
`portdat.h:438` (`Sema`), `:466` (`Segment.sema`); `segment.c:155`
(`dupseg`, *"if(share) goto sameseg"* at `:192`); `sysproc.c:114` (*"n =
flag & RFMEM"*); `fault.c:291`–`:316` (`okaddr`, `validaddr`);
`pc/trap.c:964` (`validalign`); `pc/fns.h:16` and `pc/devarch.c:544`
(`cmpswap`).

**What is Plan 9's, as it is:** the waiters are a list on the segment,
added at the tail and woken from the head; `semwakeup` clears `waiting`
before it wakes; the loop re-arms `waiting`, tries `canacquire`, and
sleeps on `semawoke`; a waiter that leaves woken but empty-handed — a note,
a timeout — passes the wakeup to the next; `tsemacquire` measures what it
slept in ticks (`TK2MS(m->ticks - t)`) and gives up when that reaches its
time. `RFMEM` shares the segment and so the list; without it a child's
segment is a copy with nobody waiting; `exec` makes new segments.

**The one difference, which cannot be otherwise:** Plan 9's kernel reads
`*addr` and calls `cmpswap(addr, …)` on the user address directly, because
the process's segments are mapped in its address space. This kernel has no
address space holding a process's memory, so the two operations go through
the machine as `Machine::load(pid, addr)` and `Machine::cmpswap(pid, addr,
old, new)`. `cmpswap` was already a machine function in Plan 9; the pid is
added because the memory is the process's and not the kernel's. `okaddr`'s
bounds check is the machine's answer to either: an address outside the
process's memory is an error, which the kernel turns into `validaddr`'s
*"sys: bad address in syscall"* and `Ebadarg`. The wasm machine reaches the
memory of the process whose call is in progress, and only that one, for
exactly the length of the call; Dis or the CLR would do the same with their
own heap.

`Segment` here is only its `sema` list: a segment's pages and bounds are
the machine's. Its sharing is Plan 9's.

**Found while doing it, in the host's import table:** `alarm` answered
`i64` where the stub declares `long` (32 bits), so the first program to
call `alarm()` would have failed to instantiate — nothing did, so nothing
noticed; and `rendezvous` answered 0 whatever the kernel returned, so no
process ever received the value it was swapped. Both fixed; a test now
checks every stub in `sys.c` against its import's type.

### §15.8 — Tracing (2026-09-23)

The last of P6. Christine: *"do 1-5"*, the third being tracing.

**Read before writing:** `pc/trap.c:660`–`:790` (`syscall`, the two
`Proc_tracesyscall` blocks at `:682` and `:755`); `port/syscallfmt.c`
entire; `port/proc.c:1480` (`procctl`, `Proc_traceme` at `:1498`) and
`:696` (a traced process's children are traced); `port/devproc.c:169`
(`profclock`), `:309` (*"addclock0link(profclock, 113)"*), `:267`
(`profile`'s length), `:747` (`Qsyscall`), `:779` (`Qprofile`), `:1388`
(`CMprofile`), `:1404`, `:1411` (`CMstartstop`, `CMstartsyscall`);
`port/segment.c:245` (`attachimage`), `:786` (`segclock`), `:170`
(text shared by `dupseg`); `libc/fmt/dofmt.c:315`–`:447` (`%#p`, `%#ux`);
`port/systab.h:114`; `cmd/tprof.c:100`.

**What is Plan 9's, as it is:** the stop on the way into a call with
`syscallfmt`'s line and on the way out with `sysretfmt`'s, each as a
`Proc_stopme` through `procctl`; the trace freed when the process goes on;
`Proc_traceme` stopping only when a note is pending; children of a traced
process traced; `startstop` and `startsyscall` refusing a process that is
not `Stopped`, readying it and waiting in `procstopwait`; every format as
`syscallfmt.c` writes it (`%#p` and `%#ux` always carry `0x`); the text
segment found in the image cache by qid, `mqid`, `mchan` and type, and
shared by `rfork` whatever the flags; `profile` a zeroed count per
`1<<LRESPROF` bytes of text; `profclock` every 113ms, charging `TK2MS(1)`
to `[0]` and to the pc's slot, in user mode only.

**What `syscall()` needed from the machine, and got:** the raw argument
words and the pc — Plan 9's `up->s`, *"*((Sargs*)(sp+BY2WD))"*, and
`ureg->pc`. The decoded `Call` has lost what the trace prints: every string
is shown as its address and its text (`%#p/"%s"`). So `Syscalls::syscall`
takes a `Ureg` of the two, the host setting the words on the way into each
import and finding the pc — the innermost wasm frame's offset in its module
— by a backtrace, asked for only when a call is traced; `timerintr` takes
the pc the same way for `profclock`. Strings are read through
`Machine::load`, the one way the kernel reaches a process's memory.

**Differences that cannot be otherwise, stated where they are:**

* `profclock` also adds the tick to `tos->clock`. The kernel maps no `Tos`
  here (`main9.c`), so that half is not done.
* `seek`'s first argument on the PC is where the kernel writes the result;
  this machine answers it, so the trace shows it as nil.
* `sysretfmt` reads a `pread`'s data and `await`'s message out of the
  process; here the machine writes them into the process after the kernel
  answers, so the trace takes them from the answer — the same bytes.
* A trace's pc is a module offset, not an address; the text segment is the
  module, based at 0.

**Found while doing it:** `syscall_tail` was told whether the call was
`rfork` by its caller, and `resume` always said no — wrong for any `rfork`
that left the processor, which a traced one does. It now reads
`up->scallnr` as Plan 9 does. And a process `stop`ped at `notify` was, once
started with `startsyscall`, given an exit trace of the call it had
already finished; the stop is past `syscall()`'s exit trace, and the tail
now knows that by `insyscall`, which Plan 9 clears between the two.

### §15.9 — A `procrfork` child of its own, and `exec` that can wait (2026-09-23)

Christine: *"do 1-5"*, the fourth being the hazard recorded since §15.2: a
`procrfork` child ran on its PARENT'S frames until its `exec`, so a child
that slept before then — and one does as soon as its `exec` reads an image
from anything that answers later than at once — left the parent's fiber
suspended with the child half-run on it, and the child on the run queue
with nothing to enter.

**It is not hypothetical past P6.** rc's child does nothing before `exec`
but `dup` and `close` (`ipnx.c`, as `plan9.c:329`), but `exec` reads the
image through `devtab` (`sysproc.c:302`), and on a Plan 9 system `/bin` is
a file server: every command would be read through the mount driver, whose
reads sleep until the server answers. With userspace file servers in P7,
every `exec` would have been exposed.

**What changed, in the machine:** the child is a new instance of the
parent's module with the parent's memory copied into it and the parent's
stack pointer, and it calls `__childstart(f, arg)` on a fiber of its own.
A compiled C module's state is its memory and its stack pointer — function
pointers are table indices the same image fills the same way — so the copy
is the parent as it stood, and `arg`, which may point into the parent's
frames (rc's `Fe` does), is valid in it. The kernel is asked for `RFPROC`
and the caller's flags: a copy of the data segment, which is `rfork`'s own
behaviour without `RFMEM`. `RFMEM` cannot be given — a wasm memory belongs
to one instance and an instance runs on one stack — and is refused rather
than silently not done. libthread's `procrfork` adds `RFMEM` itself; ours
does not, and says so (`procrfork.c`).

This removes the one difference §15.2 recorded as forced: `clockintr` no
longer defers a switch or a note while a child is on its parent's frames,
because no child ever is. The rows in §15.2 and §15.3 that say so are
records of what was true then.

**Found with it, both fixed:**

* **`exec` could not sleep.** `exec_image` read the image with `dread` in a
  loop and nothing else, so a device that slept — a pipe here, the mount
  driver later — handed back a short image. The read is now resumable as
  `pread`'s is: when the device sleeps, the rest of `exec` is kept for when
  the process is entered again.
* **An image that was not a module ended the system.** `touser` kept the
  bytes and the module was compiled on first entry, inside the scheduler,
  whose error ended `schedinit`. `sysexec` refuses a bad header before it
  commits — *"if(indir || line[0]!='#' || line[1]!='!') error(Ebadexec)"*
  (`sysproc.c:343`) — so `touser` now compiles, and fails with *"exec header
  invalid"*; the kernel asks it before committing anything, so the process
  goes on in its old image.

**Seen and not done:** `sysexec` also runs `#!` scripts, reading the
interpreter from the first line (`sysproc.c:340`–`:360`). This kernel does
not; a script run by name is refused as a bad header.

### §15.10 — What the screen shows after `^C` (2026-09-23)

Christine: *"do 1-5"*, the fifth being the observation that rc printed no
fresh prompt after `^C`. **Compared with Plan 9, that was wrong.** On a
pseudo-terminal: `sleep 30`, `^C`, gave `^C% ` — the prompt was there,
after the terminal's echo of the key; `^C` at an idle prompt gave
`% ^C` then a newline and `% `.

rc is Plan 9's here, line for line. Its note handler is `plan9.c`'s
unchanged (`notifyf`, `:510`–`:520`, `interrupted = 1`), and an
interrupted read of the next command is *"if(Eintr()){ pchr(err, '\n');
p->eof = 0; }"* then the prompt again (`exec.c:976`); a command it was
running ends of the note and rc prompts as after any command.

The difference was the host's: a terminal in cooked mode echoes `^C`
(`ECHOCTL`), and `rio` echoes nothing for its interrupt key — it writes
*"interrupt"* to the note group and that is all (`wind.c:651`). So the
host turns `ECHOCTL` off while it runs and restores the terminal's settings
when it exits. With it off the terminal echoes the bare control byte, which
it does not display; the prompt now begins its line after a command is
interrupted, as in a `rio` window. The terminal cannot be told not to echo
the key at all without turning echo off for everything, which the host's
cooked-mode line editing depends on.

### §15.11 — One command from the host's command line (2026-09-24)

`cargo run -p ipnx -- echo hello`, as CLAUDE.md documents it, failed with
*"'echo' does not exist"*: the host exec'd `/bin/echo` as pid 1 straight
after the kernel's own binds, and nothing had mounted the store — `boot`
does that — so once the commands moved from `#/boot` onto the store there
was nothing at that name.

**How Plan 9 runs one command at boot**, read before choosing:

* `pc/main.c:257` — every line of `plan9.ini` goes into `#e` (unless its
  name begins `*`) and into `#ec`, by `ksetenv`.
* `boot.c:202`–`:228`, `execinit` — *"cmd = getenv("init")"*; with none,
  *"/%s/init -%s%s"* with `$cputype`, `t` or `c` from `#e/service`
  (`:255`–`:260`) and `m` from `boot -m`. The line is `tokenize`d (quotes
  honoured, `qtoken`) and `argv[0]` is its first word's last element.
* `init.c:34`–`:44` — `-c`, `-m`, `-t`, then *"cmd = *argv"*; `rcexec`
  (`:171`) runs *"execl("/bin/rc", "rc", "-c", cmd, nil)"* in place of the
  terminal's `termrc` start, and the loop (`:66`) then clears `cmd` and
  starts the interactive shell.

**So nothing was invented**: the host's command line becomes the `init=`
configuration line, which the host applies as `pc/main.c` applies
`plan9.ini`; `boot`'s `execinit` is now Plan 9's instead of a fixed
`exec("/wasm/init")`; `init` takes `-m`, `-t` and a command. `-c` and
`cpustart` are not here: there is no cpu service and no `cpurc`. `init`'s
exit test, which ends the session when input has ended, is now *"the shell
that exited was the bare interactive one"* rather than *"`manual` was
set"*, so a command given with `-m` still gets its shell.

**Found on the way: `seek` had no `whence` 2.** `getenv` sizes a variable
with `seek(f, 0, 2)` (`9sys/getenv.c`, verbatim), and `sysseek` answered
*"bad whence"*, so `getenv` of a variable that existed malloc'd a garbage
size and read into it — `boot`'s `$init` came back as noise. It had never
shown because the only `getenv` in the system, `boot`'s `$user`, names a
variable nobody sets. `sseek` is now Plan 9's (`sysfile.c:793`): from the
end is the length `stat` gives, a pipe is `Eisstream`, any other whence is
`Ebadarg`.

**Differences:** the host's exit status is init's (empty), not the
command's, because pid 1 is `boot` and then `init`, as on Plan 9. And a
bare interactive shell ending at end of input makes `init` print *"rc exit
status: rc N: false"*; that predates this change and is not yet compared
with Plan 9.

### §15.12 — Compiling each image once (2026-09-24)

`touser` compiled the image with Cranelift on every `exec`, so the same few
images — above all `rc`, which is re-executed for every pipeline stage and
subshell — were compiled over and over. **Measured**, debug build, `printf
'echo a; echo b; echo c\n' | target/debug/ipnx`: **19.4–19.8s wall, 18.3–18.6s
user** before; **9.2s wall, 8.1–8.2s user** after. The rest is each distinct
image — `boot`, `init`, `rc`, `bind`, `cat`, `echo` — compiled once per
boot.

**Where the cache is, and why there:** in the machine, keyed by the image's
bytes (hashed to look up, compared whole on a hit, so what runs is exactly
what was read). A compiled module is the machine's concern and nothing
Plan 9 has a counterpart for; the kernel's image cache is `attachimage`'s,
keyed by channel (`segment.c:259`), and serves the text segment. Keying the
machine's by content keeps the `Machine` trait unchanged — it still names
no machine — and needs no kernel change. An image that fails to compile is
refused with `Ebadexec` as before and not kept.

**Not done:** keeping compiled modules across boots (wasmtime can
serialise a module to disk), which would remove most of the remaining 8s.

### §15.13 — `#!` scripts (2026-09-24)

§15.9 recorded, seen and not done, that `sysexec` runs `#!` scripts and
this kernel refused them as a bad header — a kernel smaller than Plan 9's.
Built now.

**Read before writing:** `sysproc.c:259`–`:360` (`sysexec` to the end of
its header loop) and `:601`–`:628` (`shargs`); `chan.c:1652`–`:1656` (the
final element `namec` leaves in `up->genbuf`); `a.out.h:2` (`Exec`, eight
`long`s, 32 bytes).

**What is Plan 9's, as it is:** fewer than two bytes is `Ebadexec`
(`:311`); a `#!` line is split by `shargs` at blanks and tabs up to its
newline, and a line with no newline or no words is `Ebadexec`; the
interpreter is found by the first word and given, as `argv`, the script's
last element (`progelem`, refused at 64 bytes or more), the line's other
words, the script's name as the caller gave it, and the caller's arguments
after its `argv[0]` (*"arg[1] += oBY2WD"*); `indir` makes an interpreter
that is itself a script `Ebadexec`; and `up->text` stays the script's
element.

**Two differences, both stated in the code:**

* Plan 9 tests its binary magic first and a `#!` line second. The kernel
  cannot recognise the machine's binaries, so it tests the other way round:
  an image that begins `#!` is a script and anything else goes to the
  machine, which refuses what it cannot run with the same `Ebadexec`. No
  binary begins `#!` — a wasm module begins `\0asm` — so the outcome is the
  same for every image.
* `shargs` is passed `n`, the bytes read, which can be 40 (`sizeof(exec)`
  with the 64-bit entry), while `line` holds `sizeof(Exec)`, 32; a newline
  past the 32nd byte is looked for past the end of `line`, which is not
  defined. Here the line is what `line` holds, so it must end within 32
  bytes.

Found in passing: four comments cited `sysexec` at `sysproc.c:302`, which in
this tree is inside its header loop; they now cite `:259` (the function),
`:310` (the read) and `:436` (placing the arguments).

### §15.14 — `validaddr` for every call (2026-09-24)

Plan 9 validates every user pointer a call is given. `validaddr`
(`fault.c:310`) asks `okaddr` (`:291`), which prints *"suicide: invalid
address %#lux/%lud in sys call pc=%#lux"*, then posts *"sys: bad address in
syscall"* as `NDebug` and raises `Ebadarg`; the process dies of the note on
its way out of the call. Here the host decoded the pointers itself and an
address it could not read made the import answer -1 — no note, the process
carrying on — for every call but the semaphores and tracing.

**Read before writing:** each call's own checks — `sysfile.c:193`–`:194`
(`pipe`), `:271` (`open`), `:635` (`read`), `:726` (`write`), `:938`,
`:958`–`:959` (`fstat`, `stat`), `:980` (`chdir`), `:1004`–`:1005`,
`:1038`, `:1047` (`bindmount`), `:1093`, `:1109` (`unmount`), `:1127`,
`:1145` (`create`, `remove`), `:1203`–`:1205`, `:1217` (`wstat`, `fwstat`);
`sysproc.c:286`–`:287`, `:401`–`:408` (`exec` and its `argv`), `:671`–`:675`
(`exits`), `:723` (`await`), `:753`–`:755` (`errstr`), `:784`–`:785`
(`notify`), `:1193`, `:1212`, `:1230` (the semaphores); `auth.c:32`–`:35`
(`fversion`); `chan.c:1330` and `:1703`–`:1716` (`namec`'s `validnamedup`,
which scans a user name with `vmemchr`); `fault.c:322` (`vmemchr`, which
`validaddr`s each page it reaches).

**Where the checks are:** in the kernel, at the top of each call that came
from a process, from the argument words the machine hands over (`up->s`) —
each call's checks in its own order. They run after a tracer's entry stop,
as there. The host no longer refuses: an import that cannot read a name or
a buffer passes an empty one, and the kernel refuses the call before it is
used, because every address the host cannot read is one the kernel's check
fails. Nothing in this names the machine: the kernel reads the process's
memory through `Machine::load`, as the semaphores already did.

**What is Plan 9's, as it is:** the message, the note, `Ebadarg`, the death
on the way out; a name checked to its NUL and *"name too long"* at `1<<16`;
a buffer's `n` negative is a bad address (*"(long)len >= 0"*); `exits` with
a bad status exits with *"invalid exit string"* instead of failing;
`errstr` with no buffer is `Ebadarg` with no note; `pipe`'s pair and a
semaphore misaligned are *"sys: odd address"*.

**Differences, stated in the code:**

* `notify`'s argument is not checked: on the PC it is the handler's address
  in the process's memory; here it is an index into the module's table.
* Address 0 is in the process's memory, where Plan 9 leaves page 0
  unmapped — a nil pointer is bad only if what it points at is.
* `okaddr` reports where its walk of the segments stopped; with one segment
  the address is reported as given.
* `exec` checks its `argv` after reading the header; here with the name,
  first — so a missing file with a bad `argv` answers the bad address.
* The kernel's own calls, made at boot with names from the kernel's memory,
  carry no words and are not checked.

A test found one more thing: the old host did not refuse a name starting
exactly at the end of memory — it read it as empty — so that case already
reached the kernel. The host test uses an address far past the end, which
fails with the old host and passes with this one.

## §16 — Packages and services: what apt, Cargo and Plan 9 do (2026-09-24)

Christine: *"Use existing package managers as an inspiration for packages
primitives"*; *"i've already said use plan 9 or existing package managers as
inspiration"*; *"This requires you to do research"*. The manuals could not
be fetched — this environment's proxy refuses `man.freebsd.org`,
`docs.brew.sh` and `www.debian.org` — so what follows is measured on the
**installed systems in this container**: Ubuntu 24.04's apt and dpkg with
real `postgresql` and `redis-server` packages, the Cargo registry this
project builds from, and `plan9/`. Each claim names the file it was read
from.

### §16.1 — apt and dpkg

**The chain of verification.** `/var/lib/apt/lists/*_noble-updates_InRelease`
is a PGP-signed message (*"-----BEGIN PGP SIGNED MESSAGE----- Hash:
SHA512"*, signature at its line 1259) listing a checksum and size for every
index it covers. The `Packages` index lists each package with its file and
hash — `apt-cache show postgresql-16`:

> `Filename: pool/main/p/postgresql-16/postgresql-16_16.13-0ubuntu0.24.04.1_amd64.deb`
> `Size: 15584148`
> `SHA256: 457531fa724387210589e071f471abd0287840a68f3874a68bb211ec00970d46`

So **one signature covers the index, and the index's hashes cover every
package file.** Nothing is signed per package.

**A package's metadata** (`dpkg -s postgresql-16`): `Package`, `Version`,
`Architecture`, `Installed-Size`, `Depends` (with version bounds, and `|`
for alternatives), `Recommends`, `Provides`, `Breaks`, `Description`.

**What dpkg keeps per installed package** (`/var/lib/dpkg/info/`): `.list`
(every file it installed), `.md5sums`, `.conffiles`, and the maintainer
scripts `preinst`, `postinst`, `prerm`, `postrm` — for `redis-server`, all
four.

**Configuration files survive removal.** `redis-server.conffiles` lists
`/etc/default/redis-server`, `/etc/init.d/redis-server`,
`/etc/logrotate.d/redis-server`, `/etc/redis/redis.conf`; `redis-server.postrm`
undoes them only on `purge` (*"if [ "${1}" = "purge" ]"*).

**Pruning is by marking.** `/var/lib/apt/extended_states` records
`Auto-Installed: 1` for packages installed only as dependencies (13 here);
`apt-mark showmanual` lists the rest. What `autoremove` may take is what is
auto-installed and no longer depended on.

**A daemon is a package of its own.** `redis-server` (`Depends: redis-tools
(= 5:7.0.15-…)`) carries the daemon's start script and configuration;
`redis-tools` carries the programs and their libraries. PostgreSQL splits
the same way: `postgresql-16` is the server's programs, and
`postgresql-common.conffiles` holds `/etc/init.d/postgresql`. **The service
and the software are different packages** — the distinction Christine drew
(*"a package should be like installing a toolchain or a library, services
installs daemons"*) is the one Debian already makes.

**What installing a daemon does** (`redis-server.postinst`):

> `update-rc.d redis-server defaults` — enable it at boot
> `invoke-rc.d --skip-systemd-native redis-server $_dh_action` — start it (or restart on upgrade)

`redis-server.prerm` stops it on removal (*"invoke-rc.d … redis-server
stop"*); `postrm` on purge disables it (*"update-rc.d redis-server
remove"*). **Enabling, starting, stopping and disabling are four separate
acts.**

**How a daemon is described** (`/etc/init.d/redis-server`): an `INIT INFO`
header (`Provides`, `Required-Start`, `Default-Start: 2 3 4 5`), the program
(`DAEMON=/usr/bin/redis-server`), its arguments (the config file), a pid
file under `/run/redis`, and **its settings in a separate file sourced at
start** — *"if [ -r /etc/default/$NAME ] then . /etc/default/$NAME"* —
`/etc/default/redis-server` being `ULIMIT=65536`.

**A daemon runs as its own user**: `/etc/passwd` has `redis` (home
`/var/lib/redis`) and `postgres` (*"PostgreSQL administrator"*, home
`/var/lib/postgresql`), and `redis-server.postinst` sets its config file's
owner to `redis`.

### §16.2 — Cargo

**The registry is named by a file**, `~/.cargo/registry/index/*/config.json`:
*`"dl": "https://static.crates.io/crates"`* — where the packages are.

**A lock file records each dependency's exact version and hash.**
`Cargo.lock`:

> `name = "wasmtime"` `version = "39.0.2"` `checksum = "a667153732c6cfba625cf5adc5db60ea2849f9a027b012a48cdd81e691e7b70a"`

and `sha256sum ~/.cargo/registry/cache/*/wasmtime-39.0.2.crate` is
`a667153732c6…70a` — **the lock's checksum is the SHA-256 of the package
file.** The downloaded files are kept in a cache (`registry/cache`) and
unpacked once (`registry/src`, 265 here), shared by every project that
names them.

### §16.3 — Plan 9

**A distribution is a file server you mount.** `rc/bin/9fs:24`: *"srv -nq
tcp!9p.io sources /n/sources"*; `dist/replica/network`'s `servermount` is
*"9fs sources; bind /n/sources/plan9 /n/dist"*; `rc/bin/replica/pull`
mounts it (*"must servermount"*), fetches the server's log and applies it.
There is no download protocol: fetching is reading files over 9P, and
trust is the server's authentication.

**A service is a file in a directory.** `listen(8)`: *"The services available
are executable, non-empty files in … `/bin/service`"*, named by network and
port (`tcp565`); `rc/bin/service` has 30, and **a disabled one is renamed
with a leading `!`** (`!tcp515`, `!il17008`). **A service runs in a namespace
of its own**: *"When changing user to `none`, a new namespace is created,
usually by executing `/lib/namespace`, but `-n` selects an alternate
namespace"* — and Plan 9 ships such files, `lib/namespace.httpd` (used at
`ip/httpd/httpd.c:103`) and `lib/namespace.ftp` (`ip/ftpd.c:120`, with a
per-user `/usr/%s/lib/namespace.ftp` at `:626`).

**Daemons that are not network listeners are started from the machine's
startup scripts**: `rc/bin/cpurc:9` `ndb/cs`, `:60` `aux/listen -q tcp`,
`:66` `aux/timesync`; and **per machine** from `/cfg/$sysname/termrc`
(`termrc:39`), `/cfg/$sysname/cpurc` (`cpurc:26`) and
`/cfg/$sysname/cpustart` (`cpurc:74`).

**Services end with their window.** `rio/wind.c:1111`, on delete: *"write(w->notefd,
"hangup", 6)"*; `rio/rio.c:329`, when rio exits: *"postnote(PNGROUP,
window[i]->pid, "hangup")"*.

**A user's profile** is `$home/lib/profile`, sourced by `init` at login
(`init.c:178`).

### §16.4 — What Plan 9 does for trust and for identity

Christine, of the two questions left open: *"for both questions, what does
plan 9 do?"*

**Trust in what is fetched.**

* **Stock Plan 9 trusts nothing it checks.** `9fs sources` is *"srv -nq
  tcp!9p.io sources /n/sources"* (`rc/bin/9fs:24`), and `srv`'s `-n` is
  *"doauth = 0"* (`cmd/srv.c:109`): the distribution server is mounted
  without authentication. Stock `replica` records no hash of a file
  (`plan9-stock/sys/src/cmd/replica/util.c` has no `hashfd`).
* **9legacy adds a hash per file, not a signature.** Its `replica` writes
  each file's SHA-1 into the log and checks it before installing — *"verify
  the source bytes against the log hash before touching the local file, so a
  corrupt copy is never installed"* (`replica/applylog.c:1055`, `hashfd` at
  `util.c:145`). The log comes over the same unauthenticated connection, so
  this guards against corruption, not against the server.
* **Where Plan 9 does authenticate, trust is the connection**: `srv`
  without `-n` authenticates the 9P session through `factotum`, and what the
  server says is then believed.
* **Keys live in `factotum`**, an agent holding them in memory and answering
  challenges (`factotum(4)`), started for each login (`auth/login.c:117`,
  `startfactotum`), with `secstore` keeping them between sessions. There is
  no keyring file on disk for a program to read.

So Plan 9 has **no signed index**: apt's chain (§16.1) is the only signed
scheme in the research. What Plan 9 offers is 9legacy's hash per file, and an
authenticated connection to the server that supplies them.

**Identity** (`auth(8)`):

* **A terminal has one user, the host owner**, named at boot (`$user`,
  written to `#c/hostowner`, `boot/bootauth.c:56`).
* **`auth/login user`** — *"allows a user to change his authenticated id to
  user. Login sets up a new namespace from /lib/namespace, starts a
  factotum(4) under the new id and execs rc(1) under the new id"* — as a
  login shell, `rc -li` (`auth/login.c:214`). The change of id is the
  capability device: a write to `#¤/capuse` (`login.c:88`). **Logout is that
  shell ending**; there is no logout command.
* **`auth/as user command`** — Plan 9's `su`: *"executes command as user…
  This only works for the hostowner and only if #¤/caphash still exists"*;
  it writes a hash to `#¤/caphash` and uses it at `#¤/capuse` (`as.c:112`,
  `:147`, `:160`).
* **`auth/none`** — *"sets up a new namespace from namespace (default
  /lib/namespace) as the user none and execs its arguments"*: how daemons
  run, and what `listen` does for every service (§16.3).

**The kernel already has what these need**: `#¤`, with `caphash` and
`capuse` (`kernel/src/devcap.rs`). `login`, `as` and `none` are userspace
programs over it.

### 16.5 Configuration, packaging and fids — what building P7's first step measured (2026-09-24)

**Plan 9's one configuration format is `ndb(6)`**, and it is the format every
`.cfg` takes (Christine: *"if plan 9 has ndb let's use that consistently"*):

> *"The files comprise multi-line tuples made up of attribute/value pairs of
> the form attr=value or sometimes just attr. Each line starting without
> white space starts a new tuple. Lines starting with # are comments."*
> — `plan9/sys/man/6/ndb`

A value with spaces is double-quoted (`libndb/ndbaux.c:41`). `libndb` is
1,787 lines over `libbio` (`plan9/sys/src/libndb`, 22 files); from rc,
`ndb/query -f file attr value rattr` answers one value
(`plan9/sys/src/cmd/ndb/query.c`, 114 lines). **Plan 9 has no YAML** — no
file under `plan9/sys/src` mentions it.

**Plan 9's nearest thing to promoting a directory into a package is a proto
file and `disk/mkfs`**: a proto names which files of a tree go into a
distribution (`plan9/sys/lib/sysconfig/proto/`, eight of them), and `mkfs`
*"copies files from the file tree"*, under `-a` writing *"an archive file to
standard output"* (`plan9/sys/man/8/mkfs`).

**A fid is unique across the whole kernel, not per mount.** Plan 9
allocates it with the channel — *"c->fid = ++chanalloc.fid"* (`chan.c:250`)
— and sends the channel's own as the 9P fid (`devmnt.c:344`, `:425`). The
mount driver here counted per `Mnt`, from 1. Two mounts of one wire — `boot`'s
`#s/boot` and `newns`'s `mount -aC #s/boot /root` — share one 9P session and
so one fid space, and each handed out fids 1, 2, 3…: a clunk through one took
the other's file. It surfaced as `/home`, the first bind `newns` makes of a
directory under the server, answering *"unknown fid"*; `/$objtype/bin` had
been lucky. The counter is now the driver's (`kernel/src/devmnt.rs`), and
`two_mounts_of_one_wire_never_share_a_fid` fails without it.

### 16.6 What plan9port does without `/rc` (measured 2026-09-24)

plan9port (`github.com/9fans/plan9port`, commit `b6564bd`, cloned beside
this repository at `../plan9port`) runs Plan 9's userland on Unix and has no
`/rc`. Each of `/rc`'s parts goes somewhere else:

| Plan 9 | plan9port |
|---|---|
| `/rc/lib/rcmain`, compiled in (`rc/plan9.c:27`) | **`$PLAN9/rcmain`**, at the top of the install: `Rcmain()` answers `unsharp("#9/rcmain")` (`src/cmd/rc/plan9ish.c:28`), `#9` being the install root (`src/lib9/unsharp.c:12`) |
| `/rc/bin`, bound onto `/bin` | **mixed into `$PLAN9/bin`** with the binaries and the sh scripts — 24 rc scripts and 27 sh scripts in the source tree's `bin` (`9fs`, `man`, `sig`, `spell`, `src`, `yesterday` …), bare-named, one directory on `$PATH` (`bin/9`, `bin/9.rc` put it first) |
| `termrc`, `cpurc`, `service/` | **none** — Unix boots the machine and starts daemons |
| `/lib/namespace` | **none** — no per-process namespaces; "the namespace" is a directory where servers post, `/tmp/ns.$USER.$DISPLAY` (`src/lib9/getns.c:58`), or `$NAMESPACE` |
| `$home/lib/profile` | **the same**, read by a login rc (`rcmain:20`) |

So rc's startup file travels with rc's installation and is found relative
to it, and rc-written commands sit beside compiled ones under bare names.

### 16.7 How Plan 9 reads a file into the environment (2026-09-24)

An earlier reply here said only that Plan 9 has no `env` command, which is
true and misleading: it has three ways, none of them a command.

1. **`/env` is the environment.** Each variable is a file in `#e`; writing
   one sets it, and a new rc reads every file there at start (`Vinit`,
   `sys/src/cmd/rc/plan9.c:129`).
2. **rc reads and writes its own format.** `whatis` prints variables *"in a
   form suitable for input to rc"* (`sys/man/1/rc:745`) — `plain=1`,
   `list=(a b)`, `rcq='says hello'` — and that is read back with `.` or,
   from a command's output, with `ifs=() eval \`{…}` — the idiom Plan 9's
   own scripts use (`rc/bin/lp:97`, `aux/getflags`). Measured here: `ifs=()
   eval \`{cat /tmp/t.env}` sets all three, list and quoted value intact,
   and `whatis` prints them back in the same form. Per-machine settings are
   done this way: `termrc` runs `. /cfg/$sysname/termrc` (`rc/bin/termrc:40`).
3. **`plan9.ini` is loaded into the environment by the kernel.** One
   `name=value` per line, `#` comments, no quoting — the value is
   everything after `=` (`sys/src/9/pc/main.c:83`–`:93`) — and each pair is
   set in `#e` and in the configuration environment `#ec`
   (`main.c:257`–`:260`, `ksetenv`, `port/devenv.c:386`).

Also found: `/lib/namespace` ends by including a per-machine namespace file,
`. /cfg/$sysname/namespace` (`lib/namespace:47`).

### 16.8 A union walk must survive a device that errs (2026-09-24)

Found by the first user `start.ns`: `bind -a /etc /home`, over `/home`
bound from `/usr/kitty`, made `ls /home` list `motd` and `cat /home/motd`
fail. The union's first element is a directory on the 9P file server, and a
9P server answers a missing first name with `Rerror`, not an empty `Rwalk`.
The kernel's walk propagated that error before trying the union's other
elements. Plan 9 cannot: each element is walked through `ewalk` —

> ```c
> if(waserror())
> 	return nil;
> wq = devtab[c->type]->walk(c, nc, name, nname);
> ```
> — `plan9/sys/src/9/port/chan.c:948`

— so an error is a miss, the loop at `:1027` (*"try a union mount, if
any"*) goes on to the next element, and only when every one misses does
`walk` fail, with the last device's error still set. Measured before the
fix: a union whose first element was a kernel directory worked (`bind -a
/etc /mnt; cat /mnt/motd`), one whose first element was on the server did
not (`bind -a /etc /tmp; cat /tmp/motd`). `kernel/src/namec.rs` now does
what `ewalk` does, and `a_union_is_tried_when_its_first_element_errs` fails
without it. The same session's other kernel finding, one fid space per
wire, is §16.5.

### 16.9 Building Plan 9's commands as they are (2026-09-24)

**A trial build of all 128 single-file commands** (`plan9/sys/src/cmd/*.c`)
against the libc, libbio and libauth then vendored, with `mk.sh`'s flags:
**51 compiled and linked** unchanged. Of the rest, 54 stopped at a header —
`draw.h` 15, `thread.h` 8, `mach.h` 8, `libsec.h` 6, `String.h` 5,
`regexp.h` 4, `mp.h` 3, `ip.h` 2, `ndb.h` 1, `ar.h` 1 (a library not
vendored) — and 23 at the link: `fork` in 10, `execl` 7, `fd2path` 5,
`mktemp` 4, `postnote` 4, `atnotify` 3, `dial` and `netmkaddr` 2 each, and
one each of `qlock`, `truerand`, `read9pmsg`, `amount`, `auth_proxy`,
`fauth`. Two define no `main` of their own. The cut-down `cat`, `echo`, `ls`
and `tr` written earlier had no cause: Plan 9's own compile unchanged.

**`setjmp` on a machine whose stack cannot be saved** (superseded by §16.12). Plan 9's is two
instructions a side on the 386 (`libc/386/setjmp.s`): save SP and the return
pc, restore them. Wasm has no addressable stack. clang lowers `setjmp` and
`longjmp` onto wasm exception handling (`-mllvm -wasm-enable-sjlj`, with
`-wasm-use-legacy-eh=false` for the `try_table` encoding these engines
accept), leaving the library three functions and a tag: `__wasm_setjmp`,
`__wasm_setjmp_test`, `__wasm_longjmp` and `__c_longjmp`
(`userspace/libc/wasm/setjmp.c`). `jmp_buf` grows to four longs. wasmtime
39 exposes exceptions only under its `gc` feature and will not build an
engine with it unless a collector is compiled in, so the host enables `gc`
and `gc-null`. Measured: `sed 's/[/y/'` unwinds from `regcomp`'s `longjmp`
into sed's *"r.e.-using command garbled"* and the shell goes on.

**The store was a shortcut; `u9fs` is the model.** Every time was 0, every
file mode `0644`, the qid a hash of the path with version 0, there was no
`Twstat`, a read read the whole file and a write rewrote it. `u9fs`
(`plan9/sys/src/cmd/unix/u9fs/u9fs.c`) takes the times, mode and inode from
`stat` (`stat2dir`, `:694`; `plan9mode`, `:609`; `stat2qid`, `:624`, with
*"qid.vers = st->st_mtime ^ (st->st_size << 8)"*), applies a wstat's mode,
mtime, name and length in that order (`rwstat`, `:909`), masks a create's
permission with the directory's (`usercreate`, `:1605`), and reads and writes
with `pread`/`pwrite`. The store now does each. One consequence, as in
`u9fs`: a directory's length is the host's (4096), where Plan 9's own file
servers report 0.

### 16.10 kencc's language, and building all of Plan 9 with clang (2026-09-24)

Plan 9's C is kencc's, defined in *How to Use the Plan 9 C Compiler*
(`plan9/sys/doc/comp.ms`): `const` and `volatile` *"are also ignored"*
(:256); an unnamed member's members *"are addressable without prefix in the
outer structure"*, it *"may be accessed by type name if (and only if)"*
declared with a typedef name, and *"the address of a struct Node may be used
without a cast anywhere that the address of a struct Lock is used … The
compiler automatically promotes the type and adjusts the address"* (:1111
on); designators may omit the `=` (:1206). clang has only the flattening
(`-fms-extensions`), and takes the promotion as an incompatible-pointer
diagnostic — **passing the unadjusted address, right only when the member
comes first**. `mk.sh` had silenced that diagnostic.

Measured over the whole tree with `-Dconst=`, before the derivation: 1,547
errors — 145 *"no member named 'T'"* (a member reached by type name:
`Mouse` 18, `Store` 15, `Frame` 12 …), 130 *"duplicate member"* (an outer
member, or an earlier unnamed one, has the same name: `9p.h`'s `File` has
its own `readers` and an unnamed `RWLock` with another), 232 *"redefinition
of parameter"* (prototypes naming two parameters alike, `aquarela`), and 136
promotions in files that did compile (most to `Lock*` or `QLock*` from an
unnamed-struct global).

`userspace/kencc.py` derives each file into `build/kencc/`: an unnamed
member `T;` becomes `union { T; T T; };` — clang then finds `x->f` through
the anonymous half and `x->T` through the named one — unless a name in T is
already found (the outer structure's own members first, then earlier unnamed
members in order), in which case it is only named, `T T;`, and a use of its
members is written through it. The promotions are found from clang's own
diagnostics, which carry both types and the expression's exact range (the
range end is one past), and written `(&(E)->T)`, or through the chain of
unnamed members to it. The rest are clang's reports too: a redefined
parameter renamed, `.fd 0` given its `=`, a block-scope `static` dropped,
and a prototype that disagrees with its definition (an enum parameter
declared `int`, `stringbgop`) written as the definition.

What building everything turned up in the build itself: a comment inside a
continued assignment (`libmach/mkfile`'s `#\t0\`) ends at the physical line
and the list continues; `mksyslib` adds members (`ar vu`), and libsec, libmp
and libc are each built from `port` then the machine's directory into one
library; `reduce` is an rc script, `rc ./reduce $O $objtype $ALLOFILES`, and
without rc the list came back empty; the compilers take `pgen.c` from
`../cc` by metarule and link `../cc/cc.a$O`, which `cmd/mkfile` builds first
(`for(i in cc $DIRS)`). Result: 34 of 36 libraries and 246 programs.

### 16.11 A create must send its own channel; building with the system itself (2026-09-24)

**`cunique` before a create.** 9P's `Tcreate` moves the fid it is sent onto
the new file. Plan 9's `namec` therefore creates on a copy — *"We need our
own copy of the Chan because we're about to send a create, which will move
it"*, `cnew = cunique(cnew)` (`port/chan.c:1606`). This kernel sent the
parent's own channel: a create in `.` moved the current directory's fid, and
after `cd /tmp; echo a >x` every relative name answered *"unknown fid"*; a
create in a union moved the mount's channel. Found when the build ran a
recipe on the system for the first time. `kernel/src/namec.rs` now clones
first; `a_create_in_dot_leaves_dot_where_it_was` fails without it.

**Sources made by Plan 9 programs.** libsec's elliptic-curve tables are
generated at build time — `%.c:D: %.mp` → `mpc $prereq >> $target`
(`libsec/port/mkfile`). `mkfile.py` runs such a recipe in rc on the system
it is building, `ipnx rc recipe.rc` over the store, one at a time: 35 lines
of `.mp` become 164 lines of C, and libsec builds. `mk.sh` makes this its
second pass.

### 16.12 `setjmp`, `longjmp` and `fork` by asyncify (2026-09-24)

**Christine's decision** (`docs/verbatim.md`): this machine gives Plan 9's
programs `fork` and libthread's coroutines by Binaryen's asyncify, and
`RFMEM` by wasm shared memory. It replaces §16.9's wasm-exception `setjmp`,
which could not have given `fork` at all: a wasm call's frames are the
engine's, and only a program transformed to save its own can be copied.

**Asyncify cannot run over wasm exception handling.** Measured with Binaryen
132 on an image built with `-mllvm -wasm-enable-sjlj`: the pass's `Flatten`
stops at `try_table` (*"unexpected expr type"*). So the two could not
coexist, and the exception flags, the `__c_longjmp` tag and wasmtime's `gc`
and `gc-null` features are gone.

**The mechanism.** `mk.sh` and `mkfile.py` run `wasm-opt --asyncify
--pass-arg=asyncify-imports@sys.setjmp,sys.longjmp,sys.rfork -O2` over every
linked image, so only call paths that can reach those three imports are
instrumented. `libc/wasm/setjmp.c` is the library's half: two calls, and a
megabyte of bss the stack unwinds into (`__asyncbuf`). An import that needs
the stack records why, sets the buffer's header and calls
`asyncify_start_unwind`; the process's code returns frame by frame to the
machine's run loop (`hosts/ipnx/src/machine.rs`, `run`), which:

| call | what the machine does | the call answers |
|---|---|---|
| `setjmp(j)` | keeps a copy of the frames and the stack pointer under j's address, and winds the same stack back | 0 |
| `longjmp(j, v)` | drops this stack and winds back the copy kept under j | v, from setjmp's call |
| `rfork(RFPROC)` | the kernel's `sysrfork`; then a new instance with a copy of all of memory, the stack wound back in it — and the parent's wound back here, `ready(p); sched()` as `sysrfork` ends | 0 in the child, its pid in the parent |

Two things measured the hard way. **Rewinding reads the frames from the
end**: the header's first word must be past the frames, as unwinding left
it — pointing it at their start makes the rewind read garbage, and the
first symptom was an unrelated `Binits: unknown mode` trap. And **the stack
pointer** is a global the compiler keeps outside memory, so it is saved with
each copy and restored before each rewind.

Measured: `sed 's/[/y/'` unwinds from `regcomp`'s `setjmp`; `ed` answers two
bad commands by `longjmp` to one `setjmp` and runs the third; `time echo`
forks, the child `exec`s and the parent reports its times (`time.c`
unchanged). sed grows from 69,524 bytes linked to 101,626 asyncified; the
whole `system` package's programs are 26,001,716 bytes.

**A string literal is writable data in kencc.** `outstring`
(`8c/swt.c:106`) emits each literal byte by byte as `ADATA` into `.string`,
an ordinary static (`cc/lex.c:1263`); Plan 9's code relies on it —
`ed.c:159` passes `"/tmp/eXXXXX"` to its own `mktemp`, which writes the Xs.
clang makes a literal `constant` and, having inlined that `mktemp`, deletes
the stores, so every `ed` named its temporary file `/tmp/eXXXXX` and the
second failed. `kencc.py` now compiles each file to IR with the optimiser
off, makes every `@.str` an `internal global`, and optimises that.

**libthread's coroutines by the same unwinding.** A thread switch is
`setjmp(t->sched)` then `longjmp(p->sched, 1)` (`sched.c:111`), and the
kept stacks are exactly that. What is new is a thread's FIRST run: 386's
`_threadinitstack` writes a launcher's pc and the new stack's top into the
`jmp_buf` (`libthread/386.c`), and `longjmp` returns into it. Here
`setjmp` writes SP and a pc of 0 into the `jmp_buf`, as `setjmp.s` writes
SP and pc; `libthread/wasm.c` writes the launcher and the stack top as
`386.c` does; and a `longjmp` that finds a pc calls that function — an
index into the exported function table — on that stack. libthread builds,
and `tprimes` runs its sieve.

**`_tos`.** libthread reads the pid from `_tos` (`sched.c:36`), which here
was a zeroed static. Plan 9's kernel puts the `Tos` at the top of the stack
(`USTKTOP-sizeof(Tos)`), hands its address to `_main` in AX
(`libc/386/main9.s`), and writes the pid into it on every return to user
mode (`kexit`, `pc/trap.c:302`). The machine now does the first two at
`_start` — which takes the address as a fourth argument — and writes the
pid at start and in each forked child.

**libc's `procrfork` is gone, and rc and init are Plan 9's.** Once
libthread built, its `procrfork` (`create.c:103`) shadowed the one this
libc had added in its name — init linked libthread's and exited at once.
That addition only ever stood in for `fork`. rc is built from its mkfile
with `havefork.c` and `plan9.c`, `init.c` is Plan 9's with the profiles
marked in place, and `libc.h` is Plan 9's again. Measured differences:
init prints *"init: starting /bin/rc"* as Plan 9's does; a subshell keeps
its parent's `$pid`, which rc sets once (`exec.c:227`); and init copies
`/adm/timezone/local` into `#e/timezone`, so `local` is set to GMT's.

**Not yet:** `RFMEM` (shared memory, the decision's second half), which
libthread's `proccreate` and `threadexec` need (`main.c:130`, `:143`).
