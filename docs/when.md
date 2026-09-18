# When — what is built, and what is not

**Role: a *when* — the single authoritative statement of build status.** No
other document carries it.

Measured 2026-09-18.

## The kernel — 4,547 lines of Rust, no dependencies

| | |
|---|---|
| `chan.rs` | `Chan` — the object every name resolves to |
| `dev.rs` | the device table, Plan 9's `struct Dev`; nine letters (`/ \| s M p d e c ¤`) |
| `devroot.rs` | `#/` — the read-only boot directory; every write is `Egreg` |
| `devpipe.rs` | `#\|` — an attach mints a pipe; the two ends are crossed |
| `devsrv.rs` | `#s` — post a file descriptor's NUMBER, and an open of the name answers with the channel behind it |
| `devdup.rs` | `#d` — a process's fds as files; opening `#d/3` returns the channel fd 3 holds, so a dup IS an open |
| `devenv.rs` | `#e` — the environment as files, one per variable, over the group `rfork` shares |
| `devcons.rs` | `#c` — all 23 of `consdir[]`. A **reporting** device: identity, this process's numbers, the clock, the kernel's log and name, the generators. The host supplies the clock, entropy, memory figures, its own drivers and `reboot`; the kernel names them |
| `ns.rs` | the namespace, keyed by the identity of the channel mounted upon |
| `namec.rs` | name → channel, with the mount check at every component |
| `proc.rs` | the process table; `rfork`'s share, copy and clear, its flag checks (`sysproc.c:43`), `exits`, `await`, and `up->user` with `renameuser` |
| `ninep.rs` | the 9P2000 codec |
| `machine.rs` | `procsetup` and `touser` — the machine-dependent half, naming no machine |
| `lib.rs` | the 28 calls, and `exec` |

96 kernel tests, and 3 in `hosts/ipnx` that run a guest against a real kernel.

## The host — `hosts/ipnx`, 116 lines

Implements `Machine` over wasmtime. `cargo run -p ipnx` resolves a name through
a namespace, reads the image out of `#/`, instantiates it and runs it. It
prints:

```
a process read a file it opened by name
```

That line is the point of it: `/init` is a wasm module that calls `open` with
the name `/hello`, and the kernel resolves it through that process's namespace,
reads it out of `#/`, and hands the bytes back. Nothing in the module knows
where the file is.

## What is not built

**The calls are dispatched.** `Kernel::syscall(up, Call)` is the one door —
Plan 9's `syscall()` (`pc/trap.c:665`) looking a number up in `systab[]`.
**`up` is an argument**, where Plan 9 keeps it in a per-machine global: the
same information, made explicit because a Rust kernel cannot hand a device an
ambient mutable global.

Answered: `rfork` `exec` `exits` `await` `errstr` `bind` `unmount` `chdir`
`open` `create` `close` `pread` `pwrite` `seek` `dup` `pipe` `remove` `stat`
`fstat` `wstat` `fwstat` — **21 of 28**. A failed call leaves its reason where
`errstr` finds it, and reading exchanges it as Plan 9's does.

Refusing, and saying why rather than pretending: `mount` and `fversion` want
the mount driver (P2); `sleep`, `alarm`, `notify`, `noted` and `rendezvous`
want a scheduler (P3).

**A guest reaches them.** `Machine::touser` is handed a `Syscalls` — the
kernel, lent for the duration — and turns whatever its trap looks like into a
`Call`. Plan 9 needs no such arrangement: a trap lands in `syscall()` and
reaches the kernel through globals. Here the machine goes out of the kernel
for the call and comes back before anything else can ask.

`hosts/ipnx` exposes `open` `pread` `pwrite` `close` `errstr` `exits`, plus a
`write` that is deliberately **not** a call — the kernel has none for writing
to a console, because a console is a file something else serves (P4). A failed
call answers −1 and leaves its reason for `errstr`, which is Plan 9's
convention rather than an error type crossing the boundary.

**`Dev::open` does not return a channel.** Plan 9's does
(`portdat.h:250`, `Chan* (*open)(Chan*, int)`), and `devdup` depends on it:
opening `#d/3` returns the channel fd 3 holds.

**The 9P codec has no wire.** `ninep.rs` formats `stat` replies and nothing
else; no `Tversion`/`Tattach`/`Twalk` is ever exchanged, because `#M` is not
built.

**The kernel has a clock.** `Machine::todget` (`port/tod.c:153`) answers
nanoseconds, the fast-tick counter and its frequency; `exec` stamps a
process's start from it, so `/dev/cputime`'s `TReal` is wall time.

No mount, proc or cap device. No shell, no userspace, no
surface. `#c`'s `cons` and `consctl` wait for a host to serve them (P4).

`/dev/sysstat`'s interrupt, page-fault, tlb and load counters are zero, and
`cputime`'s `TUser`/`TSys` are charged by nothing yet — there is no syscall
path to charge them from until P3. Both count honestly rather than reporting
a number nothing produced.

## Functional equivalence to the demo — 0 of 12

The conformance suite lists twelve capabilities and reaches none of them. It
fails, and will until it does.

The phases are in [implementation.md](implementation.md). P0 and P1 are done;
P2 is the devices.
