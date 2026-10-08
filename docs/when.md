# When — what is built, and what is not

**Role: a *when* — the single authoritative statement of build status.** No
other document carries it.

Measured 2026-09-20; the kernel's size and the test counts 2026-10-08.

## The kernel — 19,581 lines of Rust, no dependencies

13,681 of them outside the test modules (2026-10-08, after the uart and
`#/boot` went; `testfs.rs`, the tests' file server, is a test module).

| | |
|---|---|
| `chan.rs` | `Chan` — the object every name resolves to; `ismtpt`, so a mount point is neither removed nor renamed (`sysfile.c:1151`, `:1181`) |
| `dev.rs` | the device table, Plan 9's `struct Dev`; eleven letters (`/ \| s M p d e c ¤ 9 t`); `devdir`, `devdirread`, `cclone`, `devpermcheck`, which `devopen` runs for every file in a device's table (`dev.c:371`), and `devstat`'s answer for a directory (`dev.c:281`). And the kernel's libc and `parse.c`: `atoi`, `strtoul`, `tokenize`, `parsecmd`, `lookupcmd` and `cmderror` |
| `ns.rs` | the namespace, keyed by the identity of the channel mounted upon, in `MNTHASH` chains of mount heads held by reference (`portdat.h:490`) — and a union is a LIST: `cmount` puts the directory itself in first, copies a union when one is bound onto a directory, and refuses an inconsistent mount (`Emount`); `cunmount` and its two errors; `pgrpcpy`'s copy, sharing the channels and numbering the mounts again in order; `closepgrp` |
| `devroot.rs` | `#/` — the ten empty directories `rootreset` adds for a first process to bind onto, and nothing else: no `boot` and no files, because the host attaches the root before the first program runs (2026-10-08); every entry is eve's `0555`, so an open for writing is `devopen`'s `Eperm` |
| `testfs.rs` | for the tests only: their root, a read-only tree held in memory and served in 9P over `#9/0`, mounted by pid 1 with the calls the host makes for the store |
| `qio.rs` | `qio.c`'s queues, for every device that streams: `qread`, `qwrite`, `qproduce`, `qconsume`, the kick, `qhangup`, `qclose`, `qreopen`, `qflush`, `qsetlimit`, `qnoblock`, and `Qcoalesce` |
| `devpipe.rs` | `#\|` — an attach mints a pipe; the two ends are crossed. Each end is a `qio` queue: a read of an empty pipe **sleeps** on `q->rr` until a write wakes it, and the last close of an end hangs up the other (`pipeclose`, `devpipe.c:247`), which is end of file once what was queued is read. Eve may change both ends' mode (`pipewstat`, `devpipe.c:181`) |
| `devproc.rs` | `#p` — the process table as files: **twelve of `procdir[]`'s eighteen** (`devproc.c:79`), and the table says why each of the other six is absent. Each file's mode is `procgen`'s — `ctl`, `note` and `notepg` take the process's `procmode`, `0640` — checked at the open (`devproc.c:471`), and changed by `procwstat`. `ns` prints the bind lines that rebuild the namespace — paths, `int2flag`'s letters, and `mount <flag> <server> <on> <spec>` with `srvname`. `ctl` is `lookupcmd` over `proccmd[]` (`devproc.c:102`); the real-time scheduler's messages are not built |
| `devcap.rs` | `#¤` — eve mints a capability; a process spends it once and becomes another user. Eve removing `caphash` hides it for good (`capremove`, `devcap.c:64`) |
| `devmnt.rs` | `#M` — the 9P client: version, attach, a walk of up to `MAXWELEM` names, open, read and write in a loop, and a clunk that waits for `Rclunk`; an RPC a note interrupts is flushed (`mountio`, `devmnt.c:782`). A walk the server refuses leaves no fid to clunk (`devmnt.c:410`). Reached through the table's dispatcher, which takes it out while it runs. Fids come from one counter for the whole driver, as `chanalloc.fid` is (`chan.c:250`) — per mount, two mounts of one wire collided (2026-09-24) |
| `sha1.rs` | SHA-1 and HMAC-SHA1, because `#¤` needs them and the kernel has no dependencies |
| `devsrv.rs` | `#s` — post a file descriptor's NUMBER (`strtoul`'s), and an open of the name answers the posted channel itself, shared with the poster (`devsrv.c:135`). Its owner or eve renames it (`srvwstat`); it is removed by `srvremove`'s rules, and removing it closes what was posted |
| `devvirtio9p.rs` | `#9` — a channel to a 9P server the MACHINE provides. It marshals nothing: `#M` writes a T-message down it and reads the R-message back, as it would down a TCP connection |
| `devdup.rs` | `#d` — a process's fds as files; opening `#d/3` answers the channel fd 3 holds — the same one, offset and all — in the mode it is open in (`devdup.c:86`), so a dup IS an open. A ctl file reads as the descriptor's `/proc/n/fd` line |
| `devenv.rs` | `#e` — the environment as files, one per variable, over the group `rfork` shares; and `#ec`, the kernel configuration group, which nothing fills yet |
| `dev.rs`'s `eve` | `char *eve` (`auth.c:10`) — **kernel-wide, mutable, and empty at boot** (`pc/main.c:285`). The device table hands the one cell to each device as it joins, which is what a Rust kernel writes where Plan 9 reads a global. The host names the host owner by writing `#c/hostowner`, as Plan 9's `boot` does (`bootauth.c:56`), so `$user` is `kitty` — the host's `plan9.ini` says `user=kitty` (`plan9ini`, `hosts/ipnx/src/lib.rs`); Plan 9's fallback, `glenda`, is for one that names none — and not the role's own name |
| `devcons.rs` | `#c` — all 23 of `consdir[]`, `cons` and `consctl` among them: the line discipline is here, as `port/devcons.c` keeps it, and the machine supplies only `screenputs` and the keyboard's characters. The rest is a **reporting** device — identity, this process's numbers, the clock, the kernel's log and name, the generators. `kprint` is a queue that takes the console's output while it is open (`devcons.c:166`) |
| `namec.rs` | name → channel, with the mount check at every component, closing each channel a walk makes once it steps past it (`chan.c:1109`); all seven of Plan 9's access modes, and which of them steps onto a mount; the union walk, where a device's walk that errs is a miss, as `ewalk`'s is (`chan.c:948`; 2026-09-24); `validname`'s characters (`chan.c:1731`) and `namelenerror`'s form of a name in an error (`:1250`) |
| `proc.rs` | the process table; `rfork`'s share, copy and clear, its flag checks (`sysproc.c:43`), `exits`, `await`, and `up->user` with `renameuser`. **And the scheduler above the switch** (P6, begun 2026-09-21): the twelve states (`portdat.h:610`), `Rendez`, `runq[Nrq]` with `queueproc`/`dequeueproc`, `updatecpu`, `reprioritize`, `ready`, `runproc`, `sleep`, `wakeup`, `tsleep` and `timerintr` — all `port/proc.c`'s. **And the clock** (2026-09-22): `Mach` (`pc/dat.h:206`, the fields `port/` reads), `timersinit`, `hzclock`, `accounttime`, `hzsched`, `rebalance`, `anyhigher`, and `sched`'s tail with `m->schedticks`. `pexit` takes the descriptor table, `dot` and the namespace (`proc.c:1145`) and answers what was their last reference, for the kernel to close; `rfork` without `RFPROC` the same of the tables it replaces; `clunkq` (`chan.c:517`) takes what a kill leaves unclosed and what `#p`'s `ctl` closes. The kernel processes share `kpgrp` (`proc.c:1469`) and have no descriptor table. The descriptor table grows `DELTAFD` at a time to 5000 (`growfd`, `sysfile.c:25`) and holds references: a channel's device is closed at its last |
| `ninep.rs` | the 9P2000 codec, and `Dir` with `convD2M`/`convM2D` — how every directory in the system reads |
| `machine.rs` | `procsetup`, `touser` and **`gotolabel`** — the machine-dependent half, naming no machine. `Left` says how a process left, because a module's exported function can simply return where a Plan 9 process cannot |
| `lib.rs` | the 31 calls, `exec`, and `unionread` |

281 kernel tests, and 65 in `hosts/ipnx` (2026-10-08, after the uart's went).

## The host — `hosts/ipnx`, five files

`machine.rs` is the machine: `procsetup`, `todget` and `touser` over wasmtime,
and the import table that is this architecture's `9syscall`. `touser`
compiles an image once and keeps the module by the image's bytes, so an
image run again is not compiled again (2026-09-24; RESEARCH §15.12) — a
booted session running three `echo`s went from 19.5s to 9.2s in a debug
build; what is left is compiling each distinct image once per boot. `store.rs` is the
filesystem the machine serves — qemu's `-fsdev local` half, a host directory
exported over 9P. `lib.rs` is `startboot` (`initcode.c:21`): the device
table — which is what a Plan 9 kernel's configuration file is, `mkdevc`
turning a `dev` list into `devtab[]` — three opens of `#c/cons`, the binds,
then Plan 9's `boot`'s part — the host owner, the root posted as `#s/root`
and mounted, and `exec` of init.

```
% cargo run -p ipnx -- echo hello </dev/null
hello
```

**A command on the host's command line boots the whole system** (2026-09-24;
RESEARCH §15.11). The host puts it in its configuration as `plan9.ini`'s
`init=` line would — `init=/wasm/init -t 'echo hello'`, into `#e` and `#ec`
as `pc/main.c:257` does — the host reads `$init` and tokenizes it into
init's arguments, as Plan 9's `boot` does (`boot.c:202`), and `init` runs
the command with `rc -c`
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

**A forked child is a process of its own from the start** (2026-09-23;
RESEARCH §15.9, §16.12): a new instance of the parent's image, with a copy
of the parent's memory, its stack wound back in it, on a fiber of its own.
It can sleep before it `exec`s — `exec` itself reads its image through a
pipe or a server and waits there — and the parent goes on. With `RFMEM`
the child shares the parent's memory and has a stack of its own. An image the
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
`libc/9syscall/mkfile`'s generated assembly, and nothing else: **`rfork(RFPROC)`
returns twice** (2026-09-24, RESEARCH §16.12). Every image is asyncified,
the stack unwinds to the machine, and the child is a new instance with a
copy of memory and the stack wound back in it. `setjmp` and `longjmp` are
the same mechanism, and a `jmp_buf` holding a pc — libthread's new thread —
starts that function on its stack. `procrfork`, the libc addition that
stood in for `fork` until then, is gone, and rc and init are Plan 9's own
fork-based code. **`RFMEM` shares the memory** (RESEARCH §16.13): every
image imports a shared memory the machine makes, and a sharing process's
stack region — the one segment `RFMEM` does not share — is kept by the
machine and put in place when that process runs. **The stack is Plan 9's
16M** (RESEARCH §16.18), in memory and on the machine's call stack, copied
only as far as it is used; the argument block is on it under the `Tos`, as
`sysexec` puts it, and the heap begins at `end`. **A file server that is a process can be
mounted** (RESEARCH §16.14): the mount driver sleeps for its replies, one
process at a time reads a wire and hands each reply to the RPC with its tag
(`mountio`, `mountmux`), and a call that slept runs again with what it had
already done on the wires given back to it. `plumber` serves `/mnt/plumb`
and `plumb` delivers through it. Not yet: `Tflush` for an RPC a note
interrupts (`mntflushalloc`). A failed call answers −1 and leaves its reason for
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

**Plan 9's tree, vendored whole** (2026-09-24): `sys/include`, `sys/src` —
all 36 libraries, all of `cmd`, `ape`, `games` — and every architecture's
`include`, 80 MB, committed as they are in `plan9/`. Built from Plan 9's own
mkfiles, and every linked image run through `wasm-opt --asyncify` (RESEARCH
§16.12).

| | |
|---|---|
| `mk.sh` | the build: libc, then `mkfile.py libs` and `mkfile.py cmds`, then `args` and rc; then **a second pass**, in which `ipnx` is built and what failed is built again — because some sources are made by Plan 9 programs (libsec's curves by `mpc`), and the recipe runs on this system, as Plan 9 builds itself with itself. What does not build is written to `build/failed` with its reason, and the build goes on, as `mk -k` does |
| `mkfile.py` | reads each mkfile as mk does — continuation, comments per physical line, `<` includes, `${VAR:a%b=c%d}`, backquotes (rc's `reduce` done natively), `DIRS` below first, `cc` first in `cmd` — and builds what it declares: `mksyslib`/`mklib` libraries (members added, `ar vu`), `mkone`, `mkmany` (with the prerequisites a recipe-less rule adds, as mk merges them: `plumb/mkfile`'s `$O.plumber: $PLUMBER`), the one-file programs of `cmd/mkfile`, explicit `$O.x:` links and `%.$O: ../cc/%.c` metarules; `init` to `/$objtype/init` (`cmd/mkfile:116`) |
| `kencc.py` | **Plan 9's C as clang compiles it**, a derivation into `build/kencc/` (RESEARCH §16.10): `-Dconst=`; every unnamed member written `union { T; T T; }` — or only named where kencc's lookup would find another member first; the conversions kencc promotes, written `&(E)->T` from clang's own diagnostics; absolute includes; old designators; block-scope `static`; prototypes that disagree with their definitions; and string literals as writable data, as kencc's are (`8c/swt.c:106`) — IR with the optimiser off, `@.str` made `internal global`, then optimised; a file's `end` is the loader's, which wasm-ld calls `__heap_base` (RESEARCH §16.18) |
| `wasm/include/u.h`, `wasm/mkfile` | the **wasm32 architecture**: 386's `u.h` with clang's `va_list`, a `jmp_buf` whose address the machine keys its saved stack by, and 386's FP constants |
| `sys/src/libc/wasm/` | the machine-dependent half of libc, what `libc/386` is for the 386: the call stubs (`sys.c`, which also makes a `notejmp` jump on the way back), `_start` (`main9.c`, given the `Tos` the machine puts at the top of the stack, as `main9.s` is given it in AX), `sbrk`, `setjmp` (calls to the machine that unwind the stack, and the buffer it unwinds into), `tas` and the atomics, `execl`, `notejmp`, `cycles` (0: no counter), the FP control words, and the profiling pair. **libc is all of `port`, `9sys` and `fmt`**, less what this directory replaces — nothing of Plan 9's left out (the cut-down `lock.c` and `mem.c` are gone) |
| **built** | **all 36 libraries** — libdynld with Plan 9's "unimplemented" machine file, as ten of its architectures have — libsec among them, its curve tables made by the system's own `mpc`, and libthread, its threads coroutines by the machine's `setjmp`/`longjmp`; **APE's 12** (`ap`, `9`, `bsd`, `draw`, `fmt`, `l`, `mp`, `net`, `regexp`, `sec`, `utf`, `v`), `libap` with a wasm machine directory (`ape/lib/ap/wasm`: `_start`, the call stubs, `setjmp`, `brk`, the atomics); and **514 programs** in `/pkg/system/2026.09.24/wasm/bin`, rc and init among them as Plan 9's own, the compilers, loaders and assemblers, awk, APE's `sh`, `sed`, `diff`, `patch` in `bin/ape`; and **Plan 9's rc scripts** (`rc/bin`, vendored whole) in the package's `rc/bin`, bound after the programs as `/lib/namespace:27` binds them, less the startup files `/profile` replaces. Grammars are Plan 9's yacc's and lexers Plan 9's lex's, and a source a recipe makes is made by running the recipe on the system, in the second pass |
| **not built** | 3 programs, **left out by decision** (Christine, 2026-09-29: *"skip them"*; `build/failed` is the list, with each reason). `aux/vmware`'s two are 386 programs on every machine — their mkfile sets `objtype=386` (`aux/vmware/mkfile:1`) and `backdoor.c` reads 386's registers for VMware's I/O port — and a 386 build here needs Plan 9's 386 toolchain and libraries on the system, which are not installed. `syscall` calls every call through one pointer type, `int (*)(...)` (`syscall.c:32`), which the 386 takes and wasm's typed indirect calls refuse. `7a` does not build on Plan 9 either: `IOUNIT` is defined in no header (`7a/a.h:23`). `gs` is built and renders, to `pbmraw` among others, with Plan 9's fonts (`sys/lib/ghostscript/font`, vendored); `units` and `grap` have their data files (`/lib/units`, `/sys/lib/grap.defines`, vendored) |
| `sys/src/libthread/wasm.c` | libthread's machine file, after `386.c`: a new thread's stack, and the launcher its `jmp_buf` names |
| changed from Plan 9 | `libc/9sys/nsec.c` (one cast), `libauth/newns.c` (`/profile/start.ns`), rc's `plan9.c` (`Rcmain` is `/lib/rcmain`) and `rcmain` (the profiles' `shell.rc`), `cmd/init.c` (the profiles: the user's `start.ns`, the startup, and the stop at the end of the session), `gs/arch.h` (a case for this machine, whose header `genarch` writes), each change marked `ipnx:` in place |
| `adm/timezone` | Plan 9's, vendored; init copies `local` into `#e/timezone`, and `local` — a site's choice, US_Eastern in the Labs' tree — is GMT's |
| `profile/`, `usr/kitty/profile/`, `etc/motd`, `pkg/system/pkg.cfg` | the system's configuration and kitty's (P7 step 1; docs/packages.md), and the `system` package's description |
| `cmd/pkg`, `cmd/service`, `cmd/template` | the commands written in rc that are not Plan 9's (P7 steps 3–5), installed in the `system` package's `rc/bin` |

**The system boots itself.** `cargo run -p ipnx` is `initcode.c:21` — three
opens of `#c/cons` and four binds — and then **Plan 9's `boot`, done by the
host** (2026-10-08): it names the host owner, posts `#9/0` as `#s/root`,
mounts it at `/root` and binds it after `/`, and runs `/wasm/init`.
Everything after that is the system's own:

| | |
|---|---|
| the host | names the host owner and attaches `#9/0` as `/`, so **the root is a file server** — Plan 9's `boot` (`boot.c:284`–`:296`), done before the first program runs |
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
`IPNX_STORE`) over 9P — as `u9fs` does, and after it since 2026-09-24: a
stat is the host file's own times, mode and inode, a `Twstat` changes the
mode, the mtime, the name and the length, and reads and writes are at
their offset (RESEARCH §16.9) — `#9/0` is the channel to it, and the host mounts it
exactly as Plan 9's `boot` does (`boot.c:152`). `ls /` shows both halves because `unionread`
reads every element. A file written under `/tmp` is a file on the host, so it
is still there after the next boot.

Fifty tests in `hosts/ipnx` (counted 2026-09-24: forty-five in the binary — most typing at a scripted console after a full boot, some booting into a filesystem of their own — and five in the library), and 220 in `kernel/`. They need
`userspace/mk.sh` to have run — `cargo test` cannot build a wasm userspace —
and say so rather than passing quietly.

## Functional equivalence to the demo — 6 of 12

The conformance suite lists twelve capabilities and **runs a check for every
one it claims**: it boots the whole system on a scripted console and types at
it, so a line reads `reached` only when a person really can do the thing, on
this boot. The six: boot to a shell, list a directory, read a file, a
pipeline, per-process namespaces (`@{rfork n; bind /tmp/alt /etc}` sees the
bind; the shell outside it does not), and **installing a package as a bind**
— a repository made in the session, a package installed with `pkg install
-n` inside `@{rfork n; …}`, run there, and not there outside (P7's `pkg`).
Of the other six, three are P8's — several windows, actions by kind, the
browser — and **three are built by no phase of
[implementation.md](implementation.md)**: a Go program and Python, whose
design is proposed and unreviewed ([proposals.md](proposals.md)), and
building a program with a toolchain, a gap.

Two lines were corrected on 2026-10-08. *"A package becomes available
without installing anything into the tree"* and *"a language toolchain
becomes usable during a session"* described how the earlier demo delivered
its packages and toolchains to a page — streaming 260 MB after boot — not
what it offered; Christine: *"Opening a project effectively opens a session
with the right packages preinstalled"*. They are now the demo's own claims:
*"installing is a bind … a subshell that does `rfork n` owns a private
environment"*, and *"`cc hello.c` then `./a.out` … `go run hello.go`"*.

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
`init`, `/profile/start.ns` and `/profile/start.rc`, after the host has done
`initcode.c`'s nine lines and `boot`'s part (2026-10-08).

**P6 is the scheduler** — added 2026-09-21, before the registries, because the browser host is a worker per process and that IS P6's machine boundary. P7 is packages, services, templates, projects and profiles; P8 is emca and the browser.

**P7 is built but for identity** (2026-09-29). Step 1, the profiles: `/profile` and `/home/profile` with `start.ns`, and `start`, `shell` and `stop` each as a `.env` and a `.rc`, run in the order docs/packages.md gives — the user's `start.ns` added at login by `addns` — `/home` bound to `/usr/$user`, and `/rc` gone. Step 2, the `system` package (above). **Step 3, `pkg`**: install, remove, list and prune, to the system, the user or the namespace; `disk/mkfs -a` archives from a repository at `/n/pkg` named in `/profile/repository`, found in its ndb `index`, refused unless `sha1sum -2 256` matches, unpacked by `disk/mkext` into `/pkg/<name>/<version>/`, their `depend=`s first and marked `auto`; bound at once and from `/profile/pkg.ns` at every boot. **P7's acceptance passes**: a package installs as a bind, `pkg remove` unbinds it, and its files survive. **Step 4, `service`**: enable, disable, start and stop, `/service/<name>/`, `/profile/service` with Plan 9's `!`; the system's start at boot and stop at shutdown, the user's at login and logout; `user=none` runs one through `auth/none`. **Step 5, `template`**: instantiate — an included template first, the scaffolding, `project.cfg`, `install.rc` — and remove; a project is promoted by its own `pkg.rc` (the window type is P8's). **Step 6, identity, is not built**: `su` and `sudo` are defined over `auth/login`, which needs an authentication server over `/net` (docs/packages.md, *The forms*). Tested: `a_package_installs_as_a_bind_and_removes_as_an_unbind`, `an_enabled_service_starts_at_boot_and_stops_at_shutdown`, `a_template_makes_a_project_and_the_project_a_package`.

**P8 is not built.** Its first step was the serial line — `#t` in the
kernel and `eia0` on a host line, built 2026-09-29 (RESEARCH §16.20) — and
it is **removed** (2026-10-08, RESEARCH §16.33): it emulated hardware, and
*"This is WASM we have no hardware we do not want to emulate hardware"*.
Step 1 is now the host's devices — its screen, keyboard and mouse served
over 9P as the store is — and it, emca's IPNX half, the demo's types,
`hosts/web` and the page as the surface are not built.

**A channel's mode, and who may open what** (2026-10-07; RESEARCH §16.21).
`read` and `write` check a descriptor's open mode, `mount`, `fversion` and
`fauth` want `ORDWR`, and a mount's message channel is `CMSG` — all
`fdtochan`'s (`sysfile.c:120`). Every device's open stores `openmode`;
`namec` keeps `OCEXEC` from devices and servers and sets the channel flags
on a create as on an open; `#d` and `#s` hand back a channel only in the
mode it is open in; `#p`'s files carry `procgen`'s mode with `procmode`,
and `procwstat` is built; `#e`'s `OTRUNC` empties the variable; `dup`
closes what it replaces; `exec` closes the close-on-exec descriptors; and a
copy of an open channel is counted by `#c`, `#9` and `#M` as by `#|` and
`#t`. Not built: a last close that waits — the uart's drain and the mount
driver's `Rclunk` (§16.14) — and a `#d` open sharing the original's offset.

**A forked child is not entered before it is made** (2026-10-07; RESEARCH
§16.22): a clock interrupt in a fork's unwind could switch the parent and
hand the processor to a child with no fiber yet, ending the system with
*"no such process"* under load. The tests boot on a copy of the built root
each, not on `userspace/root`.

**One channel and its references; who may change what** (2026-10-08;
RESEARCH §16.23). An open answers a reference, and `#d` and `#s` answer the
channel that already exists, so `/fd/3` and a posted name share the
descriptor's offset; `Dev::incref` is gone. The descriptor table stops at
5000, as `growfd` does, not at 100. `srvremove`, `srvwstat`, `pipewstat` and
`capremove` are Plan 9's, a posted channel is closed when its name is
removed, listings name eve where Plan 9's do, a directory stats as
`devstat`'s, `/proc/n/ctl` is `lookupcmd`'s, and every error's text is its
comment in `error.h`.

**A last close waits, and a walk closes what it makes** (2026-10-08;
RESEARCH §16.24). The mount driver's clunk waits for `Rclunk`, and every
last close keeps the rest of its call while it waits — `close`, `dup`,
`exec` past its commit, `closeproc`, and `pexit`, which closes before the
parent hears. A walk closes the channels it steps past, and a call the
channel it is done with: the host's server held 182 fids after a session of
one command line and four more for every line, and now holds 10 however many
lines ran.

**The namespace by reference; what `pexit` closes** (2026-10-08; RESEARCH
§16.25). A mount head, each channel in it, and `dot` are references a copy
shares, so `pexit` closes `dot` and the namespace at their last reference,
as `chdir`, an `MREPL` bind, `unmount` and `rfork` without `RFPROC` close
what they replace — and a session ends holding **no** fid on the host's
server. A union directory's channel holds the mount head, not a copy;
closing it closes the element a read had open. `cmount` refuses an
inconsistent mount; `unmount` answers `Eunmount` and `Eunion` and resolves
the name mounted by opening it; `bind` answers the mount's id and checks
its flag; `mount` is refused under `RFNOMNT`; a name with a control
character is `Ebadchar`; a mount point is neither removed nor renamed; and
a walk the server refuses no longer clunks a fid it never had. Not built:
the wire's release when the last channel through its mount goes (`chanfree`,
`chan.c:475`), `Tflush` for an interrupted RPC, several names to a `Twalk`,
and the uart's drain.

**A wire let go with its last channel; the uart's drain** (2026-10-08;
RESEARCH §16.26). The mount driver holds a wire for as long as the server
holds a fid for one of its channels, and lets it go with the last
(`chanfree`'s *"cclose(c->mchan)"*): an unmount of a private server's last
mount hangs its pipe up, and the server reads end of file. `fversion` takes
no hold. A close made where it cannot wait — inside another close, or in a
call that may run again from the top — is made next by the loop or the
call's end, once. The uart's last close waits for the line to drain, as
`uartclose` does, and a note ends the wait (the uart was removed later the
same day). A kill during `exits` lets the close in progress finish and queues
only the rest. Not built: `Tflush` for an interrupted RPC, and several
names to a `Twalk`.

**A name in its errors** (2026-10-08; RESEARCH §16.27). From the walk on,
`namec`'s errors name the name as far as the element they concern —
`ls: /rc: '/rc' file does not exist` — as `namelenerror` writes it. A name
is `parsename`'s: `create("x.")` makes `x.` (it made `x`), and a name
ending in `/` must be a directory. `create` is `Acreate`'s: `OEXCL` on what
exists is `Eexist`, and a failed create walks again and opens what is
there.

**`Tflush`, and several names to a `Twalk`** (2026-10-08; RESEARCH §16.28).
An RPC a note interrupts sends `Tflush` and waits for its answer, then is
`Eintr` — or its own reply, if that came first — as `mountio` does; a
server is never left answering a read nobody waits for. A walk sends up to
`MAXWELEM` names in one `Twalk`, stops at a mount point among the qids,
and says *"does not exist"* for a name lost partway, as `walk` does.

**`read` and `write` on a shared channel** (2026-10-08; RESEARCH §16.29). A
write takes its range of the offset before the device writes and gives
back what it did not write, so two writers on one descriptor never write
the same bytes; a read adds to the offset as it is when it ends. A read at
0 rewinds a directory and its union, a directory is read only where its
channel is (`Edirseek`), and an offset below 0 but `~0` is `Enegoff`.
