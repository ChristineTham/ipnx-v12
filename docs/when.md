# When — what is built, and what is not

**Role: a *when* — the single authoritative statement of build status.** No
other document carries it.

Measured 2026-09-20.

## The kernel — 8,456 lines of Rust, no dependencies

| | |
|---|---|
| `chan.rs` | `Chan` — the object every name resolves to |
| `dev.rs` | the device table, Plan 9's `struct Dev`; ten letters (`/ \| s M p d e c ¤ 9`); `devdir`, `devdirread`, `cclone` and the permission check every device shares |
| `ns.rs` | the namespace, keyed by the identity of the channel mounted upon — and a union is a LIST: `cmount` puts the directory itself in first, and copies a union when one is bound onto a directory |
| `devroot.rs` | `#/` — `#/` and `boot` from `rootdir[]`, plus the ten empty directories `rootreset` adds for a first process to bind onto; every write is `Egreg` |
| `devpipe.rs` | `#\|` — an attach mints a pipe; the two ends are crossed. Each end is a `Queue` of blocks (`qio.c`): a read of an empty pipe **sleeps** on `q->rr` until a write wakes it, and the last close of an end hangs up the other (`pipeclose`, `devpipe.c:247`), which is end of file once what was queued is read |
| `devproc.rs` | `#p` — the process table as files: **nine of `procdir[]`'s eighteen** (`devproc.c:79`), and the table says why each of the other nine is absent. `ns` prints the bind lines that rebuild the namespace — paths, `int2flag`'s letters, and `mount <flag> <server> <on> <spec>` with `srvname` |
| `devcap.rs` | `#¤` — eve mints a capability; a process spends it once and becomes another user |
| `devmnt.rs` | `#M` — the 9P client: version, attach, walk, open, read and write in a loop, clunk. Reached through the table's dispatcher, which takes it out while it runs. Fids come from one counter for the whole driver, as `chanalloc.fid` is (`chan.c:250`) — per mount, two mounts of one wire collided (2026-09-24) |
| `sha1.rs` | SHA-1 and HMAC-SHA1, because `#¤` needs them and the kernel has no dependencies |
| `devsrv.rs` | `#s` — post a file descriptor's NUMBER, and an open of the name answers with the channel behind it |
| `devvirtio9p.rs` | `#9` — a channel to a 9P server the MACHINE provides. It marshals nothing: `#M` writes a T-message down it and reads the R-message back, as it would down a TCP connection |
| `devdup.rs` | `#d` — a process's fds as files; opening `#d/3` returns the channel fd 3 holds, so a dup IS an open |
| `devenv.rs` | `#e` — the environment as files, one per variable, over the group `rfork` shares; and `#ec`, the kernel configuration group, which nothing fills yet |
| `dev.rs`'s `eve` | `char *eve` (`auth.c:10`) — **kernel-wide, mutable, and empty at boot** (`pc/main.c:285`). The device table hands the one cell to each device as it joins, which is what a Rust kernel writes where Plan 9 reads a global. `boot` names the host owner by writing `#c/hostowner` (`bootauth.c:56`), so `$user` is `kitty` — the host's `plan9.ini` says `user=kitty` (`plan9ini`, `hosts/ipnx/src/lib.rs`); Plan 9's fallback, `glenda`, is for one that names none — and not the role's own name |
| `devcons.rs` | `#c` — all 23 of `consdir[]`, `cons` and `consctl` among them: the line discipline is here, as `port/devcons.c` keeps it, and the machine supplies only `screenputs` and the keyboard's characters. The rest is a **reporting** device — identity, this process's numbers, the clock, the kernel's log and name, the generators |
| `namec.rs` | name → channel, with the mount check at every component; all seven of Plan 9's access modes, and which of them steps onto a mount; the union walk, where a device's walk that errs is a miss, as `ewalk`'s is (`chan.c:948`; 2026-09-24) |
| `proc.rs` | the process table; `rfork`'s share, copy and clear, its flag checks (`sysproc.c:43`), `exits`, `await`, and `up->user` with `renameuser`. **And the scheduler above the switch** (P6, begun 2026-09-21): the twelve states (`portdat.h:610`), `Rendez`, `runq[Nrq]` with `queueproc`/`dequeueproc`, `updatecpu`, `reprioritize`, `ready`, `runproc`, `sleep`, `wakeup`, `tsleep` and `timerintr` — all `port/proc.c`'s. **And the clock** (2026-09-22): `Mach` (`pc/dat.h:206`, the fields `port/` reads), `timersinit`, `hzclock`, `accounttime`, `hzsched`, `rebalance`, `anyhigher`, and `sched`'s tail with `m->schedticks`. `exits` does `closefgrp` (`proc.c:1160`), queuing what reaches a device on `clunkq` (`chan.c:517`) |
| `ninep.rs` | the 9P2000 codec, and `Dir` with `convD2M`/`convM2D` — how every directory in the system reads |
| `machine.rs` | `procsetup`, `touser` and **`gotolabel`** — the machine-dependent half, naming no machine. `Left` says how a process left, because a module's exported function can simply return where a Plan 9 process cannot |
| `lib.rs` | the 31 calls, `exec`, and `unionread` |

197 kernel tests, and 32 in `hosts/ipnx` — two on the machine itself, five that run a guest module against a real kernel, and twenty-five that boot the whole system.

## The host — `hosts/ipnx`, three files

`machine.rs` is the machine: `procsetup`, `todget` and `touser` over wasmtime,
and the import table that is this architecture's `9syscall`. `touser`
compiles an image once and keeps the module by the image's bytes, so an
image run again is not compiled again (2026-09-24; RESEARCH §15.12) — a
booted session running three `echo`s went from 19.5s to 9.2s in a debug
build; what is left is compiling each distinct image once per boot. `store.rs` is the
filesystem the machine serves — qemu's `-fsdev local` half, a host directory
exported over 9P. `main.rs` is `startboot` (`initcode.c:21`): the device
table — which is what a Plan 9 kernel's configuration file is, `mkdevc`
turning a `dev` list into `devtab[]` — three opens of `#c/cons`, the binds,
the mount, and `exec`.

```
% cargo run -p ipnx -- echo hello </dev/null
hello
```

**A command on the host's command line boots the whole system** (2026-09-24;
RESEARCH §15.11). The host puts it in its configuration as `plan9.ini`'s
`init=` line would — `init=/wasm/init -t 'echo hello'`, into `#e` and `#ec`
as `pc/main.c:257` does — `boot` reads `$init` and tokenizes it into init's
arguments (`boot.c:208`), and `init` runs the command with `rc -c`
(`init.c:171`) in the namespace it built, then the interactive shell as
Plan 9's `init` does. Before, the host exec'd `/bin/<command>` as pid 1
before anything had mounted the store, and since the commands moved onto
it the mode had failed with *"'echo' does not exist"*. The exit status is
init's, not the command's.

`seek` from the end (`whence` 2) is built (`sysfile.c:839`): the offset is
the length the file's server states. Without it `getenv` — which sizes a
variable with `seek(f, 0, 2)` — could not read any variable that existed.
A pipe is *"seek on a stream"* and any other `whence` `Ebadarg`, as there.

## What is not built

**The calls are dispatched.** `Kernel::syscall(up, Call)` is the one door —
Plan 9's `syscall()` (`pc/trap.c:665`) looking a number up in `systab[]`.
**`up` is an argument**, where Plan 9 keeps it in a per-machine global: the
same information, made explicit because a Rust kernel cannot hand a device an
ambient mutable global.

Answered: `rfork` `exec` `exits` `await` `errstr` `bind` `mount` `unmount`
`chdir` `open` `create` `close` `pread` `pwrite` `seek` `dup` `pipe` `remove`
`stat` `fstat` `wstat` `fwstat` `sleep` `alarm` `notify` `noted`
`rendezvous` `semacquire` `tsemacquire` `semrelease` — **30 of 31**; `fversion` is refused because `mount` does the
version exchange itself (`mntversion`). A failed call leaves its reason where
`errstr` finds it, and reading exchanges it as Plan 9's does.

**`sleep` is a real `tsleep`** (P6, 2026-09-21). `n <= 0` is `yield()`,
and yielding to nobody is returning. `n > 0` is `tsleep(&up->sleep,
return0, 0, n)`: the process commits to its own `Rendez`, becomes
`Wakeme` with a deadline, and `timerintr` is what ends it — then
`sched`'s tail takes it off the run queue and marks it `Running`. The
floor is `TK2MS(1)`, 10ms at the PC's `HZ`.

**The switch is built** (2026-09-22). `Machine::gotolabel(pid)` is
`gotolabel(&up->sched)` (`pc/l.s:992`): enter the process, and return when
it leaves. The machine's is `Config::async_support` — a `wasmtime-fiber`
per process, where one poll is the jump and a future that comes back
`Pending` is a stack suspended inside a host call. `Kernel::schedinit` is
`schedinit` (`proc.c:67`), the loop every `gotolabel(&m->sched)` lands in.

**The clock is built** (2026-09-22; RESEARCH §15.1, §15.2). The machine
raises an interrupt HZ times a second — wasmtime's epoch, moved on by a
thread, with each store's callback as `clockintr` — and the kernel's
`timerintr` (`portclock.c:169`) fires what is due and calls `hzclock`
(`:136`) once however late it is. `hzclock` does what Plan 9's does:

| `hzclock` does | here |
|---|---|
| `m->ticks++` | `updatecpu` decays `p->cpu`, and `rebalance` runs once a second |
| `accounttime()` (`proc.c:1615`) | the running process is charged the tick; `m->load` and `m->perf` are the decaying averages `reprioritize` and `/dev/sysstat` read |
| `checkalarms()` | wakes the `alarm` kproc when the first alarm is due; it posts *"alarm"* |
| `hzsched()` | a process past its 100ms quantum with something else ready, or with something higher ready, is marked `delaysched`, and the interrupt's tail `sched()`s it (`pc/trap.c:438`) |

**So a process in a tight loop does not stop the system** — P6's second
acceptance test, and a test runs it: `{while(~ 1 1) x=1} &`, then the shell
kills it and carries on. The idle loop waits a tick at a time, as
`idlehands()` waits for the clock.

A tick that falls due during a call is taken at the call's end, still
`insyscall`, so it is `TSys`'s — Plan 9's interrupt held off by `splhi` and
taken at `spllo`. Every call ends as `syscall()` does, in *"if(up->delaysched)
sched();"* (`pc/trap.c:778`).

**A `procrfork` child is a process of its own from the start** (2026-09-23;
RESEARCH §15.9): a new instance of the parent's image, with a copy of the
parent's memory and the parent's stack pointer, on a fiber of its own. It
can sleep before it `exec`s — `exec` itself reads its image through a pipe
or a server and waits there — and the parent goes on. It used to run on
the parent's own frames, where a sleep left neither process enterable.
`RFMEM` cannot be given on this machine and is refused. An image the
machine cannot run fails `exec` with *"exec header invalid"*, leaving the
process in its old image; it used to end the whole system from inside the
scheduler.

**A bad address ends the process** (2026-09-24; RESEARCH §15.14). Every
call from a process checks the addresses it was given as Plan 9's calls
do — `validaddr` on each pointer, `validname` on each name — and a bad one
fails the call with `Ebadarg`, prints *"suicide: invalid address …"* and
posts *"sys: bad address in syscall"*, of which the process dies on its way
out. The host used to refuse such a call itself with -1 and no note. Not
checked: `notify`'s argument, a function index here rather than an address.

**`#!` scripts run** (2026-09-24; RESEARCH §15.13), as `sysexec` runs them
(`sysproc.c:340`–`:360`): an image that begins `#!` names its interpreter,
which is given the script's last element as `argv[0]`, the line's
arguments, the script's name, and the caller's arguments — so an rc script
runs by name. One level only; the line must end within 32 bytes,
`sizeof(Exec)`.

**A call that leaves the processor carries on where it stopped.** `sleep`,
`qlock` and `sched` mark the process (`setlabel`), whatever it was doing
keeps the rest of the call (`p->sched`), and when the process is entered
again the machine calls `Syscalls::resume` and the call goes on — nothing
before the sleep happens twice. It had been made again from the top.

**Pipes are Plan 9's queues** (2026-09-22). They did not block: a read of an
empty pipe answered 0, which is end of file, and `pipeclose` did nothing.
Pipelines worked only because `rfork`'s `sched` ran the writer first, and
preemption broke that ordering one run in six. Now `qread` sleeps on
`q->rr`, `qbwrite` waits under the limit on `q->wr` (32K, `devpipe.c:50`),
one reader and one writer at a time hold `q->rlock` and `q->wlock`, and a
writer that wakes a higher-priority reader lets it run first. **Exit closes
the descriptors**, which it also did not.

**So processes are concurrent.** A pipeline is two of them with the shell
asleep in `pwait` between; `sleep` leaves the processor and `timerintr`
brings it back; `exec` gives a process a new image and unwinds the old
one's frames, which is what `exec` means. `/proc/1/status` reads `Wakeme`
while a command runs, because that is what `init` is doing.

**Notes are built** (2026-09-23; RESEARCH §15.3). `postnote`, `notify` and
`noted`, `alarm` through an `alarm` **kernel process** as `init0` starts one
(`pc/main.c:264`), `RFNOTEG`, and `#p/<n>/note`, `notepg` and `noteid`.
`kill` is the note Plan 9 posts — `procctl` then ends the process on its way
out of the kernel — so it wakes a sleeper at once. A note ends a `sleep`
with `Eintr`; one for a handler is written onto the process's own stack and
the handler entered through the image's `__notestart`; `noted(NCONT)`
unwinds it back to where the note found the process. **rc's `Trapinit` is
`plan9.c`'s**, and rc runs its `sigint` function when told `interrupt`. A
fault is *"sys: trap: …"* and ends the process. `closeproc` is a kernel
process too.

What differs: a note for a handler that arrives at a clock interrupt waits
for the process's next call to end, because the guest can only be entered
from a host call; a note that ends the process does not wait. There is no
`Ureg` for a handler to see, and *"sys:"* notes gain no *" pc=…"*.

**`rendezvous` and the process controls are built** (2026-09-23): a
rendezvous group per `Rgrp`, new with `RFREND`, and a note pulls a process
out with `~0`; `#p/<n>/ctl`'s `start`, `stop`, `waitstop`, `hang` and
`nohang`, with `Stopped` and `procstopwait`. A process a fault or a suicide
ends is kept `Broken` — at most four — until `kill` lets it go, as `pexit`
does. `/proc/<n>/status` shows the call a process is in (`Await`, `Pread`)
before its state, so `ps` reads as Plan 9's.

**Tracing is built** (2026-09-23; RESEARCH §15.8). `startsyscall` stops a
process on its way into its next call and, started the same way, on its
way out, with `syscallfmt`'s and `sysretfmt`'s lines in `/proc/<n>/syscall`
— the pc is the wasm frame's offset in its module, found by a backtrace
only when a call is traced. `startstop` stops it at its next note.
`profile` keeps a count per eight bytes of the text segment, which
processes running the same file share (`attachimage`), charged every
113ms in user mode; `/proc/<n>/profile` is the counts. What differs: the
`Tos` clock is not advanced, as the kernel maps no `Tos`; `seek`'s trace
shows a nil return pointer, as this machine answers the offset; and there
is no `tprof` or `ratrace` here to read either file.

**The semaphores are built** (2026-09-23; RESEARCH §15.7): `semacquire`,
`tsemacquire` and `semrelease`, as `sysproc.c` has them — a waiter on its
segment's list, woken oldest first, passing a wakeup on if it leaves without
the semaphore, and `validaddr`'s and `validalign`'s notes for a bad address.
The word is read and swapped in the process's memory through the machine's
`load` and `cmpswap`. No command calls them yet, and no two processes here
share a memory while both can run, so a waiter is woken by a note or its
time running out, never yet by another process.

**`^C` interrupts a command** (2026-09-23; RESEARCH §15.5) — P6's last
acceptance test. Plan 9's kernel console turns no key into a note; `rio`
does, writing *"interrupt"* to the window's note group. Christine's
decision: *"^C should be received by host app and then sent to relevant
process as a signal"*. So the host catches `SIGINT`, and the console's
clock routine posts *"interrupt"* to the note group of the process reading
it — the shell, which `init` now gives a group of its own (`RFNOTEG`, as
Plan 9's does) — and drops what was typed and not yet sent, as `rio` does.
rc's handler survives it; the command it is running does not.

**The screen after `^C` is `rio`'s** (2026-09-23; RESEARCH §15.10). rc
behaves as Plan 9's: a command it was running ends and it prompts again;
interrupted at its prompt it prints the newline itself and prompts again
(`rc/exec.c:976`). The host terminal used to echo `^C` in front of that
prompt, where `rio` shows nothing; its echo of control characters is now
off while the host runs, and restored when it exits.

**The console no longer holds up the system.** A read of `cons` blocked the
whole machine in a host `read_line`. Now the host reads the terminal on a
thread of its own, `kbdputcclock` takes the keys in every 22ms
(`devcons.c:556`, `:671`), and a reader sleeps in `qread(kbdq)` under
`qlock(&kbd)` while everything else runs.

### `#M`, and the refactor it needed

`mount(2)` is wired: a channel posted at `/srv` is opened by name, mounted, and
a file resolves through it by an ordinary `open`. That is P2's acceptance, and
a test runs the whole path — `#s`, `#M`, the namespace and `namec` — against a
device that speaks real 9P.

Getting there needed a correction, because **the device table was a deviation
from Plan 9 that had gone unnoticed.**

`struct Dev` (`portdat.h`) is a letter, a name and seventeen function
pointers. **It holds no state.** `devtab[]` is `Dev* devtab[]` — pointers to
static vtables, generated by `mkdevc` — and each device's state lives in its
own file's globals, outside the table entirely: `static Srv *srv`
(`devsrv.c:21`), `pipealloc` (`devpipe.c:22`), `struct Mntalloc`
(`devmnt.c:48`). So `mountio`'s `devtab[m->c->type]->bwrite` reaches a
**vtable**, never another device's state, and devmnt's own `Mnt` list is not
in `devtab` at all. There is no cycle because there is nothing to cycle
through.

This kernel's table was `HashMap<DevId, Box<dyn Dev>>` where the box **is** the
state, so the table owned `#M` and `#M` could not reach the table.

**What changed.** `Mnt` holds the wire **channel**, as Plan 9 holds `m->c`,
and every operation takes a transport looked up at call time rather than one
stored for the life of the mount. `Devtab` grew the dispatcher —
`devtab[c->type]->op(...)` — and for `#M` it takes the driver **out of the
table** for the operation, so while the mount driver runs it is unreachable and
the rest of the table is free to carry its wire. That is the same property Plan
9 has by construction.

`Kernel.procs` is now shared, which is what makes `up` reachable from the
devices that need it — again what Plan 9 gets from a global.

**A guest reaches them.** `Machine::touser` is handed a `Syscalls` — the
kernel, lent for the duration — and turns whatever its trap looks like into a
`Call`. Plan 9 needs no such arrangement: a trap lands in `syscall()` and
reaches the kernel through globals. Here the machine goes out of the kernel
for the call and comes back before anything else can ask.

`hosts/ipnx` serves the whole call list as imports — the counterpart of
`libc/9syscall/mkfile`'s generated assembly — plus **`procrfork`**, which is
how a process makes a process on a machine that cannot return twice from one
call (RESEARCH §5.2, §11). A failed call answers −1 and leaves its reason for
`errstr`, which is Plan 9's convention rather than an error type crossing the
boundary.

**The kernel has a clock.** `Machine::todget` (`port/tod.c:153`) answers
nanoseconds, the fast-tick counter and its frequency; `exec` stamps a
process's start from it, so `/dev/cputime`'s `TReal` is wall time.

No surface.

`/dev/sysstat` is Plan 9's ten numbers (`devcons.c:873`) read from `Mach`:
context switches, interrupts, syscalls, load, and the idle and interrupt
percentages. It was eight, and the two it counted were fields of `#c` that
only a test ever set, so the running system read zeros. Page faults and the
TLB have no counterpart here and stay zero. `cputime` and `/proc/n/status`
report ticks converted with `TK2MS`, as Plan 9's do.

`date` reads the clock every time. It printed 0 or -1 about half the time
until 2026-09-23 because clang promotes `uchar` to `int` and kencc to
`unsigned int` (RESEARCH §15.6), not because of `pread`.

## The userspace — `userspace/`

| | |
|---|---|
| `include/u.h` | the **wasm32 architecture header**. Plan 9 keeps one per architecture; this is 386's, with clang's `va_list`, because wasm passes arguments in the engine's value stack and only the compiler knows where a variadic one is |
| `libc/wasm/` | the machine-dependent half. Plan 9 generates it from `9syscall/sys.h`; this machine's trap instruction is an import (`sys.c`), its `brk_` is `memory.grow` (`sbrk.c`), its `main9.s` is called with the argument block's address (`main9.c`), and `procrfork.c` is how a process makes a process |
| `libc/{port,fmt,9sys}/` | vendored verbatim from `plan9/sys/src/libc/`, but for one cast in `nsec.c` — kencc's `uchar` promotes to unsigned int, clang's to int (RESEARCH §15.6) |
| `libbio/`, `libauth/` | vendored: buffered i/o, and `newns` — which is all of libauth that is left once there is no factotum to talk to |
| `rc/` | Plan 9's rc, with `ipnx.c` as its platform file — it ships three of those and the mkfile picks one — and `haventfork.c`, Plan 9's own file for a system that cannot fork. `rcmain` beside it, installed at `/lib/rcmain` (Plan 9's is `/rc/lib/rcmain`), and running `/profile/shell.rc` then `$home/profile/shell.rc` in every shell |
| `cmd/` | `boot` and `init`; `bind`, `cat`, `echo`, `ls`, `mkdir`, `rm`, `unmount`, a cut-down `tr` and `args`; and **`cp`, `date`, `mount`, `mv`, `ps`, `sleep`, `test`, `wc`** — vendored verbatim from `plan9/sys/src/cmd/` except `mount`, whose `amount0` is `fauth` plus `auth_proxy` there and `mount(fd, -1, …)` here, for the reason `newns.c` already carries. **Eighteen, against the demo's "twenty-four real Plan 9 commands"**, and that list of twenty-four is written down nowhere. `pwd` is not among them: `getwd` is `fd2path`, one of the nine calls this kernel omits |
| `profile/`, `usr/kitty/profile/`, `etc/motd` | the system's configuration — `start.ns`, and `start`, `shell` and `stop` each as a `.env` and a `.rc` — and kitty's, the same seven, bound at `/home` (P7 step 1, 2026-09-24; docs/packages.md); and something to read. `/rc` is retired |
| `mk.sh` | the build. `weaken.py` beside it restores common-symbol semantics for rc.h's tentative definitions, which the wasm backend has none of |

**The system boots itself.** `cargo run -p ipnx` is `initcode.c:21` and nothing
more — three opens of `#c/cons`, four binds, and `exec("/boot/boot")`.
Everything after that line is the system's own:

| | |
|---|---|
| `boot` | mounts `#9/0` at `/root` and binds it onto `/`, so **the root is a file server** (`boot.c:151`) |
| `init` | reads `#c/user`, calls `newns` — and starts `rc` (`init.c:23`), whose first job is `/profile/start.rc` then `/home/profile/start.rc`; when the last shell ends, `/home/profile/stop.rc` then `/profile/stop.rc` |
| `/profile/start.ns` | what the namespace IS: every mount and bind, as a file (`newns`, `libauth/newns.c:35`; Plan 9's `/lib/namespace`) |
| `/profile/start.rc` | what a terminal wants on top of it (Plan 9's `rc/bin/termrc`) |

rc works out that it is interactive by asking what fd 0 is:

```
% ipnx
% echo hello
hello
% echo shouting | tr a-z A-Z
SHOUTING
% cat /etc/motd
Saranos.
% echo kept > /tmp/note
```

`/` is a union of the kernel's root and the machine's filesystem:
`hosts/ipnx/src/store.rs` exports a host directory (`userspace/root`, or
`IPNX_STORE`) over 9P, `#9/0` is the channel to it, and `boot` mounts it
exactly as `boot.c:171` does. `ls /` shows both halves because `unionread`
reads every element. A file written under `/tmp` is a file on the host, so it
is still there after the next boot.

Forty-seven tests in `hosts/ipnx` (counted 2026-09-24: forty-two in the binary — most typing at a scripted console after a full boot, some booting into a filesystem of their own — and five in the library), and 220 in `kernel/`. They need
`userspace/mk.sh` to have run — `cargo test` cannot build a wasm userspace —
and say so rather than passing quietly.

## Functional equivalence to the demo — 5 of 12

The conformance suite lists twelve capabilities and **runs a check for every
one it claims**: it boots the whole system on a scripted console and types at
it, so a line reads `reached` only when a person really can do the thing, on
this boot. The five: boot to a shell, list a directory, read a file, a
pipeline, and per-process namespaces (`@{rfork n; bind /tmp/alt /etc}` sees
the bind; the shell outside it does not). The other seven are P7 and P8.

It still fails, and will until all twelve are reached.

The phases are in [implementation.md](implementation.md). P0–P3 are done.

**The suite records progress** (since 2026-09-20). `Behaviour` carries a
`check`, and the rule the file always stated — *"is claimed reached but has
no check wired up"* — is now satisfiable rather than unsatisfiable. A check
boots the system and types at it, using `hosts/ipnx` as a **library**:
`startboot` on a console the caller supplies. No kernel internals, no test
fixtures, because a behaviour is reached when a person can do it.

**P5 is done — the CLI.** Typing `ipnx` boots to `rc` on the terminal; `ls`,
`cat /etc/motd` and the demo's commands run. The boot is the system's own:
`boot`, `init`, `/profile/start.ns` and `/profile/start.rc`, with the embedding
reduced to `initcode.c`'s nine lines.

**P6 is the scheduler** — added 2026-09-21, before the registries, because the browser host is a worker per process and that IS P6's machine boundary. P7 is packages, services, templates, projects and profiles; P8 is emca and the browser.

**P7 is begun** (2026-09-24). Step 1, the profiles, is built: `/profile` and `/home/profile` with `start.ns`, and `start`, `shell` and `stop` each as a `.env` and a `.rc`, run in the order docs/packages.md gives — the user's `start.ns` added at login by `addns` — `/home` bound to `/usr/$user`, and `/rc` gone. Not built: every `.cfg` and the `libndb` that reads them, `/pkg`, `pkg`, services, templates, projects and identity — steps 2 to 6.
