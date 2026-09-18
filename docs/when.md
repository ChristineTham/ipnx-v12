# When — what is built, and what is not

**Role: a *when* — the single authoritative statement of build status.** No
other document carries it.

Measured 2026-09-18.

## The kernel — 1,617 lines of Rust, no dependencies

| | |
|---|---|
| `chan.rs` | `Chan` — the object every name resolves to |
| `dev.rs` | the device table, Plan 9's `struct Dev`; seven letters (`/ \| s M p d e`) |
| `devroot.rs` | `#/` — the read-only boot directory; every write is `Egreg` |
| `ns.rs` | the namespace, keyed by the identity of the channel mounted upon |
| `namec.rs` | name → channel, with the mount check at every component |
| `proc.rs` | the process table; `rfork`'s share, copy and clear |
| `ninep.rs` | the 9P2000 codec |
| `machine.rs` | `procsetup` and `touser` — the machine-dependent half, naming no machine |
| `lib.rs` | the 28 calls, and `exec` |

32 tests.

## The host — `hosts/ipnx`, 116 lines

Implements `Machine` over wasmtime. `cargo run -p ipnx` resolves a name through
a namespace, reads the image out of `#/`, instantiates it and runs it. It
prints:

```
a process ran, and said so
```

## What is not built

No filesystem beyond `#/`. No pipe, mount, srv, proc, dup or env device. No
shell. No userspace. No surface.

## Functional equivalence to the demo — 0 of 12

The conformance suite lists twelve capabilities and reaches none of them. It
fails, and will until it does.

The phases are in [implementation.md](implementation.md). P0 and P1 are done;
P2 is the devices.
