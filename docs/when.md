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
| `devpipe.rs` | `#\|` — an attach mints a pipe; the two ends are crossed |
| `devproc.rs` | `#p` — the process table as files: **nine of `procdir[]`'s eighteen** (`devproc.c:79`) — `args` `ctl` `fd` `note` `noteid` `ns` `proc` `status` `wait`. `fpregs`/`kregs`/`regs` have no counterpart on a machine with no register set; `notepg` waits on notes; `mem`, `segment`, `text`, `profile` and `syscall` are simply not built. **`ns` does not yet print the bind lines**: it formats `#<letter>/<qid>` where Plan 9 prints `Chan.path`, calls a mount a bind, and infers the flag from list position because `Element` keeps a `create` bool where Plan 9's `Mount` keeps `mflag` and `spec` |
| `devcap.rs` | `#¤` — eve mints a capability; a process spends it once and becomes another user |
| `devmnt.rs` | `#M` — the 9P client: version, attach, walk, open, read and write in a loop, clunk. Reached through the table's dispatcher, which takes it out while it runs |
| `sha1.rs` | SHA-1 and HMAC-SHA1, because `#¤` needs them and the kernel has no dependencies |
| `devsrv.rs` | `#s` — post a file descriptor's NUMBER, and an open of the name answers with the channel behind it |
| `devvirtio9p.rs` | `#9` — a channel to a 9P server the MACHINE provides. It marshals nothing: `#M` writes a T-message down it and reads the R-message back, as it would down a TCP connection |
| `devdup.rs` | `#d` — a process's fds as files; opening `#d/3` returns the channel fd 3 holds, so a dup IS an open |
| `devenv.rs` | `#e` — the environment as files, one per variable, over the group `rfork` shares; and `#ec`, the kernel configuration group, which nothing fills yet |
| `devcons.rs` | `#c` — all 23 of `consdir[]`, `cons` and `consctl` among them: the line discipline is here, as `port/devcons.c` keeps it, and the machine supplies only `screenputs` and the keyboard's characters. The rest is a **reporting** device — identity, this process's numbers, the clock, the kernel's log and name, the generators |
| `namec.rs` | name → channel, with the mount check at every component; all seven of Plan 9's access modes, and which of them steps onto a mount; the union walk |
| `proc.rs` | the process table; `rfork`'s share, copy and clear, its flag checks (`sysproc.c:43`), `exits`, `await`, and `up->user` with `renameuser` |
| `ninep.rs` | the 9P2000 codec, and `Dir` with `convD2M`/`convM2D` — how every directory in the system reads |
| `machine.rs` | `procsetup` and `touser` — the machine-dependent half, naming no machine |
| `lib.rs` | the 28 calls, `exec`, and `unionread` |

151 kernel tests, and 21 in `hosts/ipnx` — five that run a guest module against a real kernel, and sixteen that boot the whole system.

## The host — `hosts/ipnx`, three files

`machine.rs` is the machine: `procsetup`, `todget` and `touser` over wasmtime,
and the import table that is this architecture's `9syscall`. `store.rs` is the
filesystem the machine serves — qemu's `-fsdev local` half, a host directory
exported over 9P. `main.rs` is `startboot` (`initcode.c:21`): the device
table — which is what a Plan 9 kernel's configuration file is, `mkdevc`
turning a `dev` list into `devtab[]` — three opens of `#c/cons`, the binds,
the mount, and `exec`.

```
% cargo run -p ipnx -- echo hello
hello
```

Nothing in `echo` knows where anything is: the kernel resolved `/bin/echo`
through pid 1's namespace, where `#/boot` is bound at `/bin`.

## What is not built

**The calls are dispatched.** `Kernel::syscall(up, Call)` is the one door —
Plan 9's `syscall()` (`pc/trap.c:665`) looking a number up in `systab[]`.
**`up` is an argument**, where Plan 9 keeps it in a per-machine global: the
same information, made explicit because a Rust kernel cannot hand a device an
ambient mutable global.

Answered: `rfork` `exec` `exits` `await` `errstr` `bind` `mount` `unmount`
`chdir` `open` `create` `close` `pread` `pwrite` `seek` `dup` `pipe` `remove`
`stat` `fstat` `wstat` `fwstat` — **22 of 28**. A failed call leaves its reason where
`errstr` finds it, and reading exchanges it as Plan 9's does.

Refusing, and saying why rather than pretending: `sleep`, `alarm`, `notify`,
`noted` and `rendezvous` want a scheduler. **They were assigned to P3 and P3
shipped without them**, so the refusals still say "— P3" and should not. With
them go `RFNOTEG` (absent from `rfork`'s flags), a write to `#p/<n>/note`, and
`#p/<n>/ctl`'s `start`/`stop`/`waitstop`/`hang`/`nohang`. What it costs
today: `init`'s loop ends where Plan 9's runs for ever on `sleep(1000)`, rc's
`Trapinit` is a stub so nothing interrupts, and there is no `sleep` command.

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

`/dev/sysstat`'s interrupt, page-fault, tlb and load counters are zero, and
`cputime`'s `TUser`/`TSys` are charged by nothing yet. Both count honestly
rather than reporting a number nothing produced.

`#c/kprint` is not exclusive-use: Plan 9 declares it `{Qkprint, 0, QTEXCL}`
with `DMEXCL|0440` (`devcons.c:614`) and `CONSDIR` here has no qid-type
column, so a second open is not refused.

**The root channel keeps the path `#/`.** Plan 9 renames it in as many words
(`pc/main.c:242`): `up->slash = namec("#/", Atodir, 0, 0); pathclose(
up->slash->path); up->slash->path = newpath("/")`. Without those three lines
`cd` reports `#/` and every line of `#p/<n>/ns` names the device rather than
the path.

## The userspace — `userspace/`

| | |
|---|---|
| `include/u.h` | the **wasm32 architecture header**. Plan 9 keeps one per architecture; this is 386's, with clang's `va_list`, because wasm passes arguments in the engine's value stack and only the compiler knows where a variadic one is |
| `libc/wasm/` | the machine-dependent half. Plan 9 generates it from `9syscall/sys.h`; this machine's trap instruction is an import (`sys.c`), its `brk_` is `memory.grow` (`sbrk.c`), its `main9.s` is called with the argument block's address (`main9.c`), and `procrfork.c` is how a process makes a process |
| `libc/{port,fmt,9sys}/` | vendored verbatim from `plan9/sys/src/libc/` |
| `libbio/`, `libauth/` | vendored: buffered i/o, and `newns` — which is all of libauth that is left once there is no factotum to talk to |
| `rc/` | Plan 9's rc, with `ipnx.c` as its platform file — it ships three of those and the mkfile picks one — and `haventfork.c`, Plan 9's own file for a system that cannot fork. `termrc` and `rcmain` beside it |
| `cmd/` | `boot`, `init`, and `bind`, `cat`, `echo`, `ls`, `mkdir`, `rm`, `unmount`, a cut-down `tr`, `args` — **ten, against the demo's "twenty-four real Plan 9 commands"**, and that list of twenty-four is written down nowhere. `mount` and `wc` are named in the demo and absent; so are `ps`, `pwd`, `cp`, `mv` and `date`, and `sleep` cannot exist until the call does |
| `lib/namespace`, `etc/motd` | the instance's configuration, and something to read |
| `mk.sh` | the build. `weaken.py` beside it restores common-symbol semantics for rc.h's tentative definitions, which the wasm backend has none of |

**The system boots itself.** `cargo run -p ipnx` is `initcode.c:21` and nothing
more — three opens of `#c/cons`, four binds, and `exec("/boot/boot")`.
Everything after that line is the system's own:

| | |
|---|---|
| `boot` | mounts `#9/0` at `/root` and binds it onto `/`, so **the root is a file server** (`boot.c:151`) |
| `init` | reads `#c/user`, calls `newns` — and starts `rc` (`init.c:23`) |
| `/lib/namespace` | what the namespace IS: every mount and bind, as a file (`newns`, `libauth/newns.c:35`) |
| `/rc/bin/termrc` | what a terminal wants on top of it (`rc/bin/termrc`) |

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

Twenty-one tests in `hosts/ipnx` run the real thing — thirteen typing at a
scripted console after a full boot, three booting twice into a filesystem of
their own, and five driving a guest module directly. They need
`userspace/mk.sh` to have run — `cargo test` cannot build a wasm userspace —
and say so rather than passing quietly.

## Functional equivalence to the demo — 0 of 12

The conformance suite lists twelve capabilities and reaches none of them. It
fails, and will until it does.

The phases are in [implementation.md](implementation.md). P0–P3 are done.

**The suite cannot record progress.** Marking a behaviour `Reached` panics —
*"is claimed reached but has no check wired up"* — because `Behaviour` has no
check to wire: there is no field for one. Five of the twelve are demonstrably
reached today (boot to a shell, list a directory, read a file, a pipeline, and
per-process namespaces, which `@{rfork n; bind /tmp/alt /etc}` shows), and the
suite still says 0 of 12. **Until `Behaviour` carries its check, the number
measures nothing.**

**P5 is done — the CLI.** Typing `ipnx` boots to `rc` on the terminal; `ls`,
`cat /etc/motd` and the demo's commands run. The boot is the system's own:
`boot`, `init`, `/lib/namespace` and `/rc/bin/termrc`, with the embedding
reduced to `initcode.c`'s nine lines.

P6 is the registries, and P7 is emca and the browser.
