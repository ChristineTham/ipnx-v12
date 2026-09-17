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
4. **Host storage** — served by the host over 9P and mounted, NOT a device.
   Plan 9 has no letter for it and this kernel mints none.
5. **The userspace** — a libc over the syscall boundary, `rc`, the commands.
6. **Boot** — `/namespace` read by the host, `/rc/bin/termrc` as the rc half.
7. **`/store`, `/pkg`, `/profile`** — one format, three registries.
8. **emca** — the window contract, then the surfaces.

Each step lands with the tests that say what it promised.
