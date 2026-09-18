# When — what is built, and what is not

**Role: a *when* — the single authoritative statement of build status.** No
other document carries it.

Measured 2026-09-18.

## The kernel — 2,804 lines of Rust, no dependencies

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

61 tests.

## The host — `hosts/ipnx`, 116 lines

Implements `Machine` over wasmtime. `cargo run -p ipnx` resolves a name through
a namespace, reads the image out of `#/`, instantiates it and runs it. It
prints:

```
a process ran, and said so
```

## What is not built

No mount, srv, proc, dup, env or cap device. No shell, no userspace, no
surface. `#c`'s `cons` and `consctl` wait for a host to serve them (P4), and
`pgrpid`, `cputime` and `sysstat` report placeholders because nothing keeps
process groups, CPU time or per-processor counters yet.

## Functional equivalence to the demo — 0 of 12

The conformance suite lists twelve capabilities and reaches none of them. It
fails, and will until it does.

The phases are in [implementation.md](implementation.md). P0 and P1 are done;
P2 is the devices.
