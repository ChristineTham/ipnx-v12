# Saranos

The operating system: the host side and the wasm side together. **IPNX** is the
kernel and the userspace; **emca** is the windowing and UI system.

Built from the design in [`../docs`](../docs) and, for anything claimed about
Plan 9, from `../plan9` at file and line. Nothing else is an input.

```
kernel/        the kernel: a SUBSET of Plan 9's, containing process
               orchestration. Nothing in it is invented.
hosts/ipnx/    Saranos on a terminal: performs the effects, owns console,
               storage and clock
```

```bash
cd saranos && cargo test     # the contracts, asserted
```

## Order of work

1. **The kernel's core** — 9P codec, namespace, device letters, the process
   table with `rfork`'s share/copy/clear. *Done, tested.*
2. **`exec`** — resolve the path through the namespace, read the image, hand it
   to the engine. The one thing the kernel cannot do for itself.
3. **The devices orchestration needs** — `#| #s #d #p #M #/ #e`, all Plan 9's.
4. **The userspace** — a libc over the call list, `rc`, the commands.
5. **The file servers** — console, clock, storage, the store, emca. Every one
   of them a process, none of them in the kernel.

Each step lands with the tests that say what it promised.
