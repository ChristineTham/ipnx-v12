# The kernel call list

> **PROPOSED — not reviewed.** Claude wrote this. Nothing in it is endorsed, and
> nothing in it approves a deviation from Plan 9. What is built is
> [when.md](when.md).

**Role: a *what* — the kernel's call list.**

**The list is derived from Plan 9's subset outward.** What a personality needs
beyond it is the personality's problem, solved in userspace. Deriving inward
from a personality guarantees the personality ends up in the kernel — that is
how `link` once appeared here as a trap when Plan 9 has no `link` syscall at
all, and neither does 9legacy.

The input is Plan 9's `/sys/src/libc/9syscall/sys.h`, recorded verbatim in
[RESEARCH.md](../RESEARCH.md) §2.

## The principle that sorts them

In a hosted kernel **every** system call is a trap to the supervisor; the question per call
is what the supervisor does with it. Five answers cover everything:

| class | answered from | examples |
|---|---|---|
| **proc** | process state in the supervisor | `rfork`, `exits`, `await`, `alarm` |
| **mem** | the guest's linear memory arrangement | `brk_`, `seg*` |
| **fd** | the descriptor table | `dup`, `seek`, `fd2path`, `chdir` |
| **ns** | the per-process mount table | `bind`, `unmount`; `mount` speaks 9P to attach |
| **9P** | dispatched down a Chan | `open`, `pread`, `wstat`, `remove` |
| **sync** | rendezvous/semaphore tables | `rendezvous`, `semacquire` |

A **9P**-class call becomes exactly one 9P message *when the Chan crosses a server
boundary*, and the same operation on an in-supervisor device otherwise. That is Plan 9's
own architecture, not an invention: kernel devices present the file interface as function
calls (`intro(3)`: each driver implements attach/walk/open/read/write for its tree), and
only the mount driver marshals wire 9P for remote servers. The supervisor reproduces that
shape — a Dev table inside, wire 9P at the boundary.

Two structural notes the table depends on:

- **`Twalk` has no syscall.** Walking is what the kernel does *with* paths inside `open`,
  `create`, `stat`, `bind` — name resolution is kernel business, which is why per-process
  namespaces are a kernel feature and not a server one.
- **`seek` is not a 9P message.** `Tread`/`Twrite` carry an explicit offset; the file
  offset is kernel state in the Chan. `seek` therefore lands in **fd**, and `pread` at
  offset −1 means "use and advance the Chan's offset".

## Plan 9's 52 slots, dispositioned

`sys.h` names 52 slots (0–47, 50–53): 11 superseded `_` variants kept for old binaries, one
reserved slot, **40 live calls**. V12 runs only recompiled binaries, so the superseded
slots are dropped entirely.

| slot | call | class | 9P message | note |
|---|---|---|---|---|---|
| 0 | `sysr1` | drop | — | — | reserved |
| 1 | `_errstr` | drop | — | — | superseded by 41 |
| 2 | `bind` | ns | — | ✓ | resolves source *at bind time*; MREPL/MBEFORE/MAFTER/MCREATE — a mount point is a union list |
| 3 | `chdir` | fd | — | ✓ | cwd is a Chan in the proc |
| 4 | `close` | 9P | `Tclunk` | ✓ | |
| 5 | `dup` | fd | — | ✓ | |
| 6 | `alarm` | proc | — | — | |
| 7 | `exec` | proc | (uses ns) | ✓ | image read via the caller's namespace, then instantiate |
| 8 | `exits` | proc | — | ✓ | |
| 9 | `_fsession` | drop | — | — | |
| 10 | `fauth` | ns | `Tauth` | — | |
| 11 | `_fstat` | drop | — | — | |
| 12 | `segbrk` | mem | — | — | |
| 13 | `_mount` | drop | — | — | |
| 14 | `open` | 9P | `Twalk`+`Topen` | ✓ | |
| 15 | `_read` | drop | — | — | |
| 16 | `oseek` | drop | — | — | |
| 17 | `sleep` | proc | — | ✓ | |
| 18 | `_stat` | drop | — | — | |
| 19 | `rfork` | proc | — | ✓ | v0: the lazy path, `RFPROC` implies `RFMEM` |
| 20 | `_write` | drop | — | — | |
| 21 | `pipe` | 9P | — | ✓ | the pipe device `#\|`; bidirectional |
| 22 | `create` | 9P | `Tcreate` | ✓ | |
| 23 | `fd2path` | fd | — | — | |
| 24 | `brk_` | mem | — | (guest) | v0 deviation: heap is guest-local `memory.grow`, see plan |
| 25 | `remove` | 9P | `Tremove` | ✓ | |
| 26 | `_wstat` | drop | — | — | |
| 27 | `_fwstat` | drop | — | — | |
| 28 | `notify` | proc | — | — | notes; the personality's signal substrate |
| 29 | `noted` | proc | — | — | |
| 30 | `segattach` | mem | — | — | |
| 31 | `segdetach` | mem | — | — | |
| 32 | `segfree` | mem | — | — | |
| 33 | `segflush` | mem | — | — | |
| 34 | `rendezvous` | sync | — | — | |
| 35 | `unmount` | ns | — | — | |
| 36 | `_wait` | drop | — | — | |
| 37 | `semacquire` | sync | — | — | |
| 38 | `semrelease` | sync | — | — | |
| 39 | `seek` | fd | — | ✓ | kernel state; see note above |
| 40 | `fversion` | ns | `Tversion` | — | |
| 41 | `errstr` | proc | — | ✓ | per-proc error string, exchanged |
| 42 | `stat` | 9P | `Twalk`+`Tstat` | ✓ | |
| 43 | `fstat` | 9P | `Tstat` | ✓ | |
| 44 | `wstat` | 9P | `Twstat` | ✓ | the call class B collapses into; carries `chmod`/`chown` and the setuid bit |
| 45 | `fwstat` | 9P | `Twstat` | — | |
| 46 | `mount` | ns | `Tversion`+`Tattach` | ✓ | takes an fd to a server; the wire-9P boundary |
| 47 | `await` | proc | — | ✓ | |
| 50 | `pread` | 9P | `Tread` | ✓ | offset −1 = Chan offset |
| 51 | `pwrite` | 9P | `Twrite` | ✓ | |
| 52 | `tsemacquire` | sync | — | — | |
| 53 | `nsec` | proc | — | — | |

**Live-call census: proc 10 · mem 6 · fd 4 · ns 5 · 9P 11 · sync 4 = 40.** Of the eleven
9P-class calls, ten are single messages and `mount` is the boundary itself. Everything else
never leaves the supervisor — **29 of 40 live calls are pure kernel calls**, which is the
concrete answer to "which survive as kernel calls and which become 9P messages".

## The subset — 28 calls

Of the 40 live calls the kernel implements 28.

| processes | `rfork` `exec` `exits` `await` `sleep` `alarm` `notify` `noted` `rendezvous` |
|---|---|
| **namespace** | `bind` `mount` `unmount` `chdir` |
| **channels** | `open` `create` `close` `pread` `pwrite` `seek` `dup` `pipe` `remove` `stat` `fstat` `wstat` `fwstat` `fversion` `errstr` |

The twelve it omits, and why:

| omitted | why |
|---|---|
| `segbrk` `brk_` `segattach` `segdetach` `segfree` `segflush` | memory is the machine's, not the kernel's. A guest grows its own linear memory; on another machine the arrangement differs and the kernel does not change |
| `fd2path` | a convenience over state the process already holds |
| `fauth` | authentication is a file server's, established at attach |
| `semacquire` `semrelease` `tsemacquire` | `rendezvous` is the primitive; semaphores are a library over it |
| `nsec` | time is a file |

Each omission is a call the kernel does not have, not a call answered
elsewhere in the kernel. Adding one back is a deviation and needs approval.
