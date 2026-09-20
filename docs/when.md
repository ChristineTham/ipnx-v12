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
| `devproc.rs` | `#p` — the process table as files: **nine of `procdir[]`'s eighteen** (`devproc.c:79`), and the table says why each of the other nine is absent. `ns` prints the bind lines that rebuild the namespace — paths, `int2flag`'s letters, and `mount <flag> <server> <on> <spec>` with `srvname` |
| `devcap.rs` | `#¤` — eve mints a capability; a process spends it once and becomes another user |
| `devmnt.rs` | `#M` — the 9P client: version, attach, walk, open, read and write in a loop, clunk. Reached through the table's dispatcher, which takes it out while it runs |
| `sha1.rs` | SHA-1 and HMAC-SHA1, because `#¤` needs them and the kernel has no dependencies |
| `devsrv.rs` | `#s` — post a file descriptor's NUMBER, and an open of the name answers with the channel behind it |
| `devvirtio9p.rs` | `#9` — a channel to a 9P server the MACHINE provides. It marshals nothing: `#M` writes a T-message down it and reads the R-message back, as it would down a TCP connection |
| `devdup.rs` | `#d` — a process's fds as files; opening `#d/3` returns the channel fd 3 holds, so a dup IS an open |
| `devenv.rs` | `#e` — the environment as files, one per variable, over the group `rfork` shares; and `#ec`, the kernel configuration group, which nothing fills yet |
| `dev.rs`'s `eve` | `char *eve` (`auth.c:10`) — **kernel-wide, mutable, and empty at boot** (`pc/main.c:285`). The device table hands the one cell to each device as it joins, which is what a Rust kernel writes where Plan 9 reads a global. `boot` names the host owner by writing `#c/hostowner` (`bootauth.c:56`), so `$user` is `glenda` and not the role's own name |
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

**`sleep` is built** (2026-09-20), both branches of `syssleep`
(`sysproc.c`). `n <= 0` is `yield()`, and yielding to nobody is returning:
a child made by `procrfork` runs to its end inside the call that made it, so
exactly one process is ever runnable. `n > 0` goes to `Machine::delay` —
`delay(int)` is declared beside `touser` in the same machine-dependent list
(`pc/fns.h:23`) and the PC spins on the TSC (`i8253.c:320`); with one
runnable process `tsleep` and `delay` are the same thing. `init`'s loop is
now Plan 9's, `sleep(1000)` and all.

**Four still refuse, and a scheduler is the whole of what they want:**
`alarm`, `notify`, `noted`, `rendezvous`. A note is delivered on the way out
of the kernel by rewriting the user stack so the handler runs and `noted`
returns through it (`notify(Ureg*)`, `trap.c`); **this machine has no user
stack the kernel can write**, and a process that is not running is not
suspended but finished. `rendezvous` would have to ready a process that is
BELOW this one on the machine's call stack. With them go `RFNOTEG` (absent
from `rfork`'s flags), a write to `#p/<n>/note`, and `#p/<n>/ctl`'s
`start`/`stop`/`waitstop`/`hang`/`nohang` — each of which now names what
Plan 9 does rather than a phase that has shipped. rc's `Trapinit` is still a
stub, so nothing interrupts. **The design is a proposal, not a gap.**

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

**`pread` with an explicit offset is not reliable**, and `date` is the only
thing that does it: `nsec(2)` is `pread(open("/dev/bintime"), b, 8, 0)`,
where everything else reads with the channel's own offset. Three `date -n`
in one session answer `-1`, `0` and a real clock in any order — an
uninitialised 8-byte stack buffer, so `pread` is returning without the bytes
landing. **The kernel is not at fault**: instrumented, `#c` is reached with
`n=8 off=0` and serves the right bytes every time, and `cat /dev/bintime`
(`n=8192`, the same import) is exact every time. Adding a write to stderr
between the calls makes it go away, which is a timing signature. Root cause
unknown; `date <seconds>` is exact, `date` is not.

## The userspace — `userspace/`

| | |
|---|---|
| `include/u.h` | the **wasm32 architecture header**. Plan 9 keeps one per architecture; this is 386's, with clang's `va_list`, because wasm passes arguments in the engine's value stack and only the compiler knows where a variadic one is |
| `libc/wasm/` | the machine-dependent half. Plan 9 generates it from `9syscall/sys.h`; this machine's trap instruction is an import (`sys.c`), its `brk_` is `memory.grow` (`sbrk.c`), its `main9.s` is called with the argument block's address (`main9.c`), and `procrfork.c` is how a process makes a process |
| `libc/{port,fmt,9sys}/` | vendored verbatim from `plan9/sys/src/libc/` |
| `libbio/`, `libauth/` | vendored: buffered i/o, and `newns` — which is all of libauth that is left once there is no factotum to talk to |
| `rc/` | Plan 9's rc, with `ipnx.c` as its platform file — it ships three of those and the mkfile picks one — and `haventfork.c`, Plan 9's own file for a system that cannot fork. `termrc` and `rcmain` beside it |
| `cmd/` | `boot` and `init`; `bind`, `cat`, `echo`, `ls`, `mkdir`, `rm`, `unmount`, a cut-down `tr` and `args`; and **`cp`, `date`, `mount`, `mv`, `ps`, `sleep`, `wc`** — vendored verbatim from `plan9/sys/src/cmd/` except `mount`, whose `amount0` is `fauth` plus `auth_proxy` there and `mount(fd, -1, …)` here, for the reason `newns.c` already carries. **Seventeen, against the demo's "twenty-four real Plan 9 commands"**, and that list of twenty-four is written down nowhere. `pwd` is not among them: `getwd` is `fd2path`, one of the twelve calls this kernel omits |
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

## Functional equivalence to the demo — 5 of 12

The conformance suite lists twelve capabilities and **runs a check for every
one it claims**: it boots the whole system on a scripted console and types at
it, so a line reads `reached` only when a person really can do the thing, on
this boot. The five: boot to a shell, list a directory, read a file, a
pipeline, and per-process namespaces (`@{rfork n; bind /tmp/alt /etc}` sees
the bind; the shell outside it does not). The other seven are P6 and P7.

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
`boot`, `init`, `/lib/namespace` and `/rc/bin/termrc`, with the embedding
reduced to `initcode.c`'s nine lines.

P6 is the registries, and P7 is emca and the browser.
