# Saranos

The operating system: the host side and the wasm side together. **IPNX** is the
kernel and the userspace; **emca** is the windowing and UI system.

Built from the design in [`../docs`](../docs) and, for anything claimed about
Plan 9, from `../plan9` at file and line. Nothing else is an input.

```
kernel/        the kernel: a pure state machine — syscalls in, effects out
hosts/ipnx/    Saranos on a terminal: performs the effects, owns console,
               storage and clock
```

```bash
cd saranos && cargo test     # the contracts, asserted
```

## Order of work

1. **The kernel's core** — 9P codec, namespace, device table. *Done, tested.*
2. **Processes** — instantiation as `exec`, fd tables, the fork guard.
3. **Devices** — `#/ #c #e #d #p #s #|`, then `#M`, the one wire boundary.
4. **What the host owns** — console, clock, randomness, storage: all served by
   the host over 9P and mounted. None of them is a device letter and none is
   an effect. Plan 9's `#c` alone serves `cons`, `time`, `random` and `reboot`,
   so a variant per file adds four before it reaches a second device.
5. **The userspace** — a libc over the syscall boundary, `rc`, the commands.
6. **Boot** — `/namespace` read by the host, `/rc/bin/termrc` as the rc half.
7. **`/store`, `/pkg`, `/profile`** — one format, three registries.
8. **emca** — the window contract, then the surfaces.

Each step lands with the tests that say what it promised.
