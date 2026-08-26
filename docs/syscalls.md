# The kernel call list — derived

*The first task, stated in both the plan and the research: derive, call by call, which
system calls survive as kernel calls and which become 9P messages. Derived 2026-08-26.
The inputs are Plan 9's `/sys/src/libc/9syscall/sys.h` and V10's `os/sysent.c`, both
recorded verbatim in [RESEARCH.md](../RESEARCH.md) §2–3.*

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

| slot | call | class | 9P message | PoC v0 | note |
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

## V10's 68 routines onto the V12 interface

The A–D classing and its evidence are RESEARCH.md §3; this table is the per-call landing.
"libc" means the personality's C library over the calls above; "design" means the item is
part of the uid-model design task; "drop" means no expression in V12.

| V10 call | class | lands on | note |
|---|---|---|---|
| `exit` | A | `exits` | status int → exit string in libc |
| `fork` | A | `rfork(RFFDG\|RFREND\|RFPROC)` | the manual's own equation |
| `read` | A | `pread(fd,buf,n,-1)` | |
| `write` | A | `pwrite(fd,buf,n,-1)` | |
| `open` | A | `open` | mode bits translated |
| `close` | A | `close` | |
| `wait` | A | `await` | status string parsed |
| `creat` | A | `create` | falls back to `open(OTRUNC)` when it exists |
| `link` | C | **design** | needs protocol room; cf. 9P2000.L `Tlink` |
| `unlink` | A | `remove` | |
| `lseek` | A | `seek` | |
| `chdir` | A | `chdir` | |
| `gtime` | A | `nsec` | ns → s in libc |
| `mknod` | C | not restored | devices are file servers; libc errors |
| `chmod` | B | `wstat` | |
| `chown` | B | `wstat` | |
| `sbreak` | A | `brk_` | |
| `stat` | A | `stat` | `Dir` → `struct stat` in libc |
| `seek` | A | `seek` | |
| `getpid` | A | libc | no trap in Plan 9 either |
| `dirread` | A | `pread` on the directory | stat records, converted |
| `setuid` | C | libc: write `/proc/n/ctl` | the item APE called impossible — [uid.md](uid.md) |
| `getuid` | C | libc: read `/dev/user`, map via passwd | |
| `stime` | D | drop | the host owns the clock |
| `fmount` | A | `mount` | |
| `alarm` | A | `alarm` | |
| `fstat` | A | `fstat` | |
| `pause` | A | `sleep` | |
| `utime` | B | `wstat` | |
| `fchmod` | B | `fwstat` | |
| `fchown` | B | `fwstat` | |
| `saccess` | B | attempt the `open`, `close` | |
| `nice` | B | write `/proc/n/ctl` | |
| `ftime` | A | `nsec` | |
| `sync` | B | none | the file server's business |
| `kill` | C | write `/proc/n/note` | |
| `select` | C | personality helper | over multiple procs or a mux server |
| `setpgrp` | B | `rfork(RFNOTEG)` | |
| `lstat` | C | **design** | meaningless until `symlink` exists |
| `dup` | A | `dup` | |
| `pipe` | A | `pipe` | |
| `times` | B | read `/proc/n/status` | |
| `profil` | D | drop | |
| `setgid` | C | as `setuid` (uid.md D2) | |
| `getgid` | C | as `getuid` (uid.md D2) | |
| `ssig` | C | `notify`/`noted` | V7-style signals over notes; libc table |
| `funmount` | A | `unmount` | |
| `sysacct` | D | drop | |
| `biasclock` | D | drop | |
| `syslock` | D | drop | |
| `ioctl` | C | `ctl` files | libc translation per device class |
| `sysboot` | D | drop | |
| `setruid` | C | libc: write `/proc/n/ctl` (uid.md D1) | |
| `symlink` | C | **design** | cf. 9P2000.L `Tsymlink` |
| `readlink` | C | **design** | |
| `exece` | A | `exec` | |
| `umask` | C | per-proc field, applied at `create` | in the PoC kernel |
| `chroot` | B | `rfork(RFCNAMEG)` + `bind` | |
| `rmdir` | A | `remove` | |
| `mkdir` | A | `create(DMDIR)` | |
| `vfork` (slot 66 = `fork`) | A | `rfork(RFPROC\|RFMEM)` | the lazy path — vfork *is* the design, §5.2 |
| `getlogname` | B | read `/dev/user` | |
| `vadvise` | D | drop | |
| `setgroups` | C | as `setuid` (uid.md D2) | |
| `getgroups` | C | as `getuid` (uid.md D2) | |
| `vlimit` | D | drop | |
| `vswapon` | D | drop | |
| `nap` | D | drop | `sleep` exists if ever wanted |
| `vtimes` | D | drop | |

**Landing census over the 68: 28 direct onto live calls (A) · 12 libc/namespace idioms (B)
· 17 split as — the seven uid calls landed by [uid.md](uid.md), `umask` in the kernel,
4 libc translations (`ioctl`, `select`, `kill`, `ssig`), `mknod` deliberately not
restored, and the `link`/`symlink`/`lstat`/`readlink` four awaiting the protocol-room
choice — · 11 dropped (D).**
