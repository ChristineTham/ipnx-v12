# When — what is built, and what is not

**Role: a *when* — the single authoritative statement of build status.** No
other document carries it.

Measured 2026-09-18.

## The kernel — 2,999 lines of Rust, no dependencies

| | |
|---|---|
| `chan.rs` | `Chan` — the object every name resolves to |
| `dev.rs` | the device table, Plan 9's `struct Dev`; nine letters (`/ \| s M p d e c ¤`) |
| `devroot.rs` | `#/` — the read-only boot directory; every write is `Egreg` |
| `devpipe.rs` | `#\|` — an attach mints a pipe; the two ends are crossed |
| `devcons.rs` | `#c` — all 23 of `consdir[]`. A **reporting** device: identity, this process's numbers, the clock, the kernel's log and name, the generators. The host supplies the clock, entropy, memory figures, its own drivers and `reboot`; the kernel names them |
| `ns.rs` | the namespace, keyed by the identity of the channel mounted upon |
| `namec.rs` | name → channel, with the mount check at every component |
| `proc.rs` | the process table; `rfork`'s share, copy and clear, its flag checks (`sysproc.c:43`), `exits`, `await`, and `up->user` with `renameuser` |
| `ninep.rs` | the 9P2000 codec |
| `machine.rs` | `procsetup` and `touser` — the machine-dependent half, naming no machine |
| `lib.rs` | the 28 calls, and `exec` |

66 tests.

## The host — `hosts/ipnx`, 116 lines

Implements `Machine` over wasmtime. `cargo run -p ipnx` resolves a name through
a namespace, reads the image out of `#/`, instantiates it and runs it. It
prints:

```
a process ran, and said so
```

## What is not built

**The 28 calls are a list, not an implementation.** `Call` is declared in
`lib.rs` and referenced nowhere — not by the kernel, not by the host, not by a
test. The kernel's whole public surface is `new`, `exec` and `exec_image`;
there is no `bind`, `mount`, `open`, `read`, `write`, `close`, `dup`, `chdir`,
`rfork`, `exits` or `await` a process can invoke. `rfork`, `exits` and `await`
exist on `Procs` and are unreachable from a process.

**There is no syscall path.** `hosts/ipnx` gives a guest one import, a print
function. A process cannot open a file, fork, or reach any device — so
everything `#c` and `#|` do is reachable only from Rust, not from a process.

**`Dev::open` does not return a channel.** Plan 9's does
(`portdat.h:250`, `Chan* (*open)(Chan*, int)`), and `devdup` depends on it:
opening `#d/3` returns the channel fd 3 holds.

**The 9P codec has no wire.** `ninep.rs` formats `stat` replies and nothing
else; no `Tversion`/`Tattach`/`Twalk` is ever exchanged, because `#M` is not
built.

**Nothing stamps a process's start**, so `/dev/cputime`'s `TReal` is 0 — the
kernel has no clock of its own. Plan 9's is machine-provided and available
kernel-wide (`MACHP(0)->ticks`, `todget`), so **this kernel should have one**;
it was left out by reading the growth rule as a ban on anything that is not
process management, which is not what it says.

No mount, srv, proc, dup, env or cap device. No shell, no userspace, no
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
