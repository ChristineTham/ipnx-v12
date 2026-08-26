# poc — the proof of concept

A working slice of the architecture, small enough to read in a sitting: **a hosted kernel
in Node executing freestanding-C WebAssembly guests in per-process namespaces**, booting
to **`rc`** — with pipes, a writable ramfs, nine commands, and **wire 9P at a mount
boundary**: a guest process serving 9P2000 on a pipe, mounted with `mount(2)`, read
through the namespace by clients that cannot tell it from a kernel device. The lazy
fork's resume mechanism (RESEARCH.md §5.2) and the Worker/SAB syscall transport (§5.3)
carry all of it.

```sh
bash mk.sh       # build guests — needs wasi-sdk and binaryen at ~/.local/opt/
bash run.sh      # boot on Node; init (pid 1) runs the acceptance tests
bash run.sh -i   # boot on Node to an interactive rc (EOF to shut down)
node serve.mjs   # serve the browser port: http://localhost:8095/browser/ (?i = interactive)
```

The kernel is one platform-neutral module (`supervisor/kernel.mjs`, with `guestcore.mjs`
for the guest runner); `supervisor/main.mjs` hosts it on Node and `browser/main.mjs`
hosts it in a page — the console is a window in the DOM, and `serve.mjs` exists only to
set the COOP/COEP headers SharedArrayBuffer requires. The same forty tests pass in both
(measured in Chrome 148).

The test boot prints forty `PASS` lines — nineteen from init (kernel, mount, and the
forktest harness), four from forktest itself, seventeen from `/rc/tests.rc` (the shell
tests) — and exits 0.

## What it proves

- **`exec` is instantiate.** The image is read *through the caller's namespace* and
  compiled at exec time; binaries in `rootfs/bin` have no extension because a fresh
  `.wasm` is indistinguishable from a shipped one.
- **A Worker is a process.** Syscalls block in `Atomics.wait` on a per-process SAB
  mailbox; the kernel thread never blocks — reads that must wait (pipes, the console)
  park in the device and complete later. `await` is a mailbox answered at a child's death.
- **The lazy fork resumes the parent — deep enough to run a shell.** `procrfork(flags,
  fn, arg)` runs the child inside a hand-assembled `try_table`/`catch_all` guard frame;
  the child's `exec` throws; the guard catches; the supervisor restores the `[0, sp)`
  stack region saved at fork. rc forks every pipeline stage and every `` `{...} ``
  capture this way, including rc re-execing *itself* for command substitution. Bare
  dual-return `rfork(RFPROC)` is refused with an error naming why (asyncify's case).
- **Bare dual-return `rfork(RFPROC)` works — on the binaries that pay for it.** The
  asyncify path (`mk.sh`'s `ASYNCIFY` list, transformed by binaryen's `wasm-opt` with
  instrumentation confined to paths reaching the fork import — rc costs +5.5%): the
  worker unwinds the stack into the guest's buffer, snapshots the whole linear memory,
  the supervisor spawns a fresh Worker over the copy, and both sides rewind — pid into
  the parent, 0 into the child. rc's subshells `(...)` are exactly this: a forked copy of
  the interpreter, usable as a pipeline stage. An untransformed binary calling bare
  `rfork` is refused with an error naming the list.
- **Namespaces are per-process, survive `exec`, and are shared by default.** rc runs in a
  namespace copy (`RFNAMEG`) and its binds never reach init; *within* rc, `bind` works as
  an ordinary command — the child shares rc's namespace, so its bind lands there. Both
  directions are tested.
- **The file interface is Plan 9's shape — and the wire is exactly at the boundary.**
  Devices (`ramfs`, `#c` cons, `#|` pipe) implement attach/walk/open/read/write/stat as
  function calls; **only the mount driver marshals 9P** (`supervisor/mnt9p.mjs`):
  `mount(fd)` negotiates Tversion/Tattach on a channel — usually a pipe to `hellofs`, a
  guest 9P2000 server — and every operation below the mount point is one wire message,
  tagged and demultiplexed, so several processes share one connection. A chan is cloned
  (`Twalk` with no names) before open, so the attach fid is never consumed — the bug the
  tests caught. Server errors (`Rerror`) surface as `errstr`. Directory reads return an
  integral number of `stat(5)` records on both sides of the wire; pipes are bidirectional
  with EOF on last clunk; channels are refcounted and a process's descriptors close at
  exit.

## Layout

| | |
|---|---|
| `supervisor/kernel.mjs` | the kernel, platform-neutral: proc table, namespaces, channels and fd tables (refcounted), dispatch, rfork/exec/exits/await |
| `supervisor/guestcore.mjs` | guest runner, platform-neutral: mailbox protocol, the fork guard (the only hand-written wasm), save/restore, the asyncify dance |
| `supervisor/main.mjs`, `supervisor/worker.mjs` | the Node host and its worker shim |
| `browser/`, `serve.mjs` | the browser host: console-window page, worker shim, COOP/COEP static server |
| `supervisor/bytes.mjs` | Uint8Array vocabulary shared by both hosts |
| `supervisor/devs.mjs` | ramfs (writable), the console (host stdin/stdout), the pipe device |
| `supervisor/mnt9p.mjs` | devmnt: the mount driver — the one place the kernel marshals wire 9P |
| `supervisor/stat9.mjs` | 9P2000 `stat(5)` marshalling |
| `libc/` | `lib9.h`, `crt0.c`, `lib9.c` — Plan 9-shaped freestanding libc |
| `cmd/rc.c` | the shell |
| `cmd/hellofs.c` | a 9P2000 file server in a guest process, serving on fd 0 |
| `cmd/forktest.c` | the bare dual-return fork, exercised |
| `cmd/` | `init` (pid 1 + kernel tests), `cat`, `echo`, `ls`, `wc`, `cp`, `mkdir`, `rm`, `bind` |
| `rootfs/` | the boot filesystem; `rc/tests.rc` is the shell test suite; `bin/` is generated |

## The rc subset

Words with `'...'` quoting and `#` comments; `$var`, `$status`, `x=v` assignments (lists
arrive via substitution); `` `{...} `` command substitution; `*`/`?` globbing in the final
path component; pipelines `|`; `;` `&` `&&` `||`; redirections `>` `>>` `<`;
`if(list) pipeline`, `if not pipeline`, `for(x in words) pipeline` — single-pipeline
bodies; **subshells `(...)`**, standalone or as pipeline stages, running in a bare-forked
copy of the interpreter; builtins `cd`, `~`, `exit`. Statements are single-line.

Not there: brace groups and functions (wrap in `( )` instead — the parser says so).
Known v0 warts: quoted `*` still globs, builtins ignore redirections, prefix assignments
persist rather than scope to the command.

## v0 deviations, all deliberate

argv arrives via a boot syscall rather than pre-placed on the stack; `brk` is guest-local
(`memory.grow`); `bind` is `MREPL` only, no union directories, no `..`; `errstr` reads but
does not exchange; pipe writers never block (unbounded queue); one user; nested lazy fork
within one Worker refused. On the wire: no `Tauth` (afid is always NOFID), no `Tflush`,
walks are one name per `Twalk`, msize is fixed at 8216, `unmount` is absent, and a failed
mid-walk leaks its intermediate fid. Each is a lifted restriction away, not a redesign.
