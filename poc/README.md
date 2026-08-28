# poc — the proof of concept

> **Complete and frozen (declared 2026-08-29).** The PoC finished at 131
> acceptance tests green on three hosts; its record is
> [docs/poc.md](../docs/poc.md), and the build sequence from here is
> [docs/implementation.md](../docs/implementation.md). The JS supervisor in
> this directory is now the **reference implementation and conformance
> oracle** — it changes no more, and it must keep running: `bash poc/run.sh`
> printing the floor suite is the standing check any new host is judged
> against. The guest world here (`libc/`, `plan9/`, `v10/`, `cmd/`, `wasi/`,
> `rootfs/`, `mk.sh`) is **not** frozen — it is the real userspace, shared by
> every host, and graduates to `userspace/` as implementation milestone M0.

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
set the COOP/COEP headers SharedArrayBuffer requires. The same 131 tests pass in both
(measured in Chrome 148).

The test boot prints 131 `PASS` lines — sixty-one from init (kernel, mount, exportfs,
the WASI second ABI with its three foreign citizens,
uid, links, notes, unmount, rfork flags, and the harnesses), twelve from dtest (the
window server and text), seven from drtest (the REAL libdraw: geninitdraw, getwindow,
allocimage, the default font), five from threadtest (libthread: channels, alt, reads
that park a thread, not the process), two from samtest (the whole editor: sam over
samterm in a window, typed at through wctl, the reply's glyphs read from the raster),
three from acmetest (the whole of acme: boot paint, button-2 execute of New, button-3
look on rc/ — mouse chords injected through wctl, windows verified in the raster),
four from forktest, thirty-seven from `/rc/tests.rc` run by the REAL rc, the real sam
included — and exits 0, identically on Node and in the browser.

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
  instrumentation confined to paths reaching the fork import — the real rc costs
  +103%, its fmt-driven error paths defeating the confinement): the
  worker unwinds the stack into the guest's buffer, snapshots the whole linear memory,
  the supervisor spawns a fresh Worker over the copy, and both sides rewind — pid into
  the parent, 0 into the child. rc's subshells `(...)` are exactly this: a forked copy of
  the interpreter, usable as a pipeline stage. An untransformed binary calling bare
  `rfork` is refused with an error naming the list.
- **Namespaces are per-process, survive `exec`, and are shared by default.** rc runs in a
  namespace copy (`RFNAMEG`) and its binds never reach init; *within* rc, `bind` works as
  an ordinary command — the child shares rc's namespace, so its bind lands there. Both
  directions are tested.
- **`/dev/draw` is an actual file, per window, per namespace — the plan9port claim,
  demonstrated.** `#w` is the window server's kernel half (rio's *interface*, not rio):
  reading `clone` mints a window; `bind '#w/N' /dev` in a namespace copy makes that
  namespace *be* the window — its own `cons`, `mouse`, `wctl`, `winid`, `label`, and a
  real `draw/` tree speaking draw(3)'s message subset (`b d f L e E v`, low-order byte
  first). `win cmd` is rio's spawn as a forty-line command; `win rc` is a shell in a
  window. **Text lands in draw too**: the `y`/`i`/`l` messages carry an 8×8 font of this
  project's own authorship (`libc/font8x8.h`) into a font-cache image, and `s` draws
  strings through it as alpha masks — glyph strokes, gaps and advances asserted by pixel
  headless, `win dtext` showing it live. dtest asserts pixels through the v0 `rgb` file;
  in the browser, windows are draggable DOM elements (canvas + text layer), `bounce`
  animates, `scribble` inks where the pointer drags, and typing lands in the focused
  window's cons.
- **Real userspaces, both of them — and the Plan 9 one is now a system.**
  `plan9/sys/` holds the verbatim 4th-edition tree slice (NOTICE): the REAL
  `/sys/include/libc.h` over our one platform shim `u.h`; the REAL libc (port, fmt with
  `fmtinstall`/`%q`/`dirmodefmt`, 9sys with dirstat/dirread/convM2D/wait/getwd/ctime),
  libbio, libregexp, libString — about 150 source files in `libp9.a` — and twenty-four
  real commands compiled unmodified: `cat echo ls wc cp mv rm mkdir chmod tee cmp tr
  uniq tail sort sed grep du date test touch sleep basename cleanname`, grep's yacc
  grammar regenerated by the host's bison at build time — **and the real `rc` itself**,
  seventeen source files and `syn.y` through the same bison door, asyncified so its
  every bare fork genuinely returns twice. The suite's pipelines run real `sed`
  rewrites, real `sort | uniq`, real anchored `grep`, real `date`, rc conditionals
  driven by real `test`, and rc's own `fn`, `while` and `switch`. Porting it surfaced
  two wasm-vs-1992 mismatches now handled in the build (RESEARCH §9.5): function
  "pointers" are small table indexes that collide with rc's operand integers unless the
  table starts high (`--table-base=4096`), and rc.h's pre-ANSI tentative definitions
  need hand-restored common-symbol semantics (`weaken.mjs`).
  `v10/usr/src/cmd/` holds verbatim TUHS-tape V10 sources (NOTICE: Nokia's covenant,
  via the parent repository) — K&R C compiled with `-std=c89 -fno-builtin`, implicit
  declarations left authentic — in `/v10/bin`, fingerprinted by `echo -e`, piping into
  Plan 9 `cat` side by side. Each userspace links its own libc.a rewritten over the
  kernel: `lib9` (`libc/`) and `libv10` (`v10/lib/`: V7-lineage stdio, `struct stat`
  decoded from stat(5) records, a flushing crt).
- **Hard links and symlinks, V10's way.** `link`/`symlink`/`readlink` are the V12 kernel
  additions (traps 60–62; `lstat` is a stat flag); on the wire they are minted types
  128/130/132, and a server without them answers `Rerror` — degradation, not a wedge.
  The kernel resolves symlinks **in the walking process's namespace** (V10's rule): a
  symlink created through a mount into an exporter's tree reads back through the mount as
  the *client's* `/etc/motd`, not the exporter's. Hard links are two directory entries,
  one node; writes through either name land in the one file.
- **The uid model runs — the thing APE called impossible** ([docs/identity.md](../docs/identity.md)).
  Credentials are mutable per-process kernel state: `/dev/user` names the caller,
  `/proc/self/ctl` accepts `user <name>` under the transition rule (the host owner may
  become anyone; anyone may fall back to their real uid), ramfs enforces V10 rwx against
  per-node owners, `chown`/`chmod` are forty lines of libc over `wstat`, and an image
  carrying 9P2000.u's `DMSETUID` bit elevates euid at `exec` while ruid stays. The tests
  drop to `none`, get denied a 0600 file, fail to climb back, and elevate through a
  setuid `id`. No new system calls anywhere.
- **Union directories are the namespace's algebra.** A mount point is a list: `bind -a`
  and `-b` append and prepend, walks try elements in order, directory reads concatenate
  (still an integral number of stat records), and creates land in the first element bound
  with `-c` (MCREATE). The tests exec a binary *through* a union and create through one.
- **exportfs closes the loop: a namespace is a value.** `exportfs` serves its own
  process's namespace over wire 9P by answering every request with real system calls
  against its own view — so an exporter that privately rebound `/etc` exports *that*
  view, the mounter reads it at `/n/exp/etc/motd`, and `exec /n/exp/bin/echo` runs a
  binary fetched across the wire. Serving and mounting are now both guest-reachable,
  each side of the same boundary.
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
| `supervisor/devwsys.mjs`, `supervisor/draw.mjs` | the window server's kernel half and its raster engine |
| `libc/draw9.[ch]` | guest libdraw-lite over /dev/draw |
| `supervisor/devs.mjs` | ramfs (writable), the console (host stdin/stdout), the pipe device |
| `supervisor/mnt9p.mjs` | devmnt: the mount driver — the one place the kernel marshals wire 9P |
| `supervisor/stat9.mjs` | 9P2000 `stat(5)` marshalling |
| `libc/` | `lib9.h`, `crt0.c`, `lib9.c` — Plan 9-shaped freestanding libc |
| `plan9/sys/src/cmd/rc/` | THE shell — real 4th-edition rc, compiled verbatim, asyncified |
| `plan9/sys/src/cmd/sam/` | the real sam — `sam -d` on the console, `win sam` in a window |
| `plan9/sys/src/cmd/samterm/` | the real samterm — libframe over libdraw over libthread |
| `libc/mousekbd.c` | initmouse/initkeyboard as platform-IO (kencc idioms kept them out of clang) |
| `cmd/samtest.c` | the editor round trip, headless: boot, type, read the reply's glyphs |
| `plan9/sys/src/cmd/acme/` | the real acme — its own 9P server, columns and windows by mouse |
| `cmd/acmetest.c` | acme headless: boot paint, button-2 New, button-3 look, via wctl chords |
| `supervisor/wasi1.mjs` | the WASI second ABI: wasi_snapshot_preview1 over the mailbox; the preopen is the namespace root |
| `wasi/` | the foreign citizens: wasitest (wasi-libc), gotest (REAL Go, wasip1), pytest.py + pylib.txt (REAL CPython, measured stdlib subset) |
| `plan9/sys/src/libdraw/`, `libframe/` | the real draw libraries — `libdraw.a`, exercised by drtest |
| `cmd/drtest.c` | REAL-libdraw client: geninitdraw over `#w`, pixels verified |
| `libc/libthread.c` | thread.h's API as the wasm platform layer (coroutines, channels, alt) |
| `cmd/threadtest.c` | libthread exercised: rendezvous, alt, thread-parking reads |
| `weaken.mjs` | restores common-symbol semantics in rc's objects (RESEARCH §9.5) |
| `libc/lib9p.h`, `libc/lib9p.c` | the guest side of wire 9P: marshal vocabulary, message framing |
| `cmd/hellofs.c` | a 9P2000 file server in a guest process, serving on fd 0 |
| `cmd/exportfs.c` | serves this process's namespace over 9P — binds and all |
| `cmd/forktest.c` | the bare dual-return fork, exercised |
| `cmd/` | `init` (pid 1 + kernel tests), `cat`, `echo`, `ls`, `wc`, `cp`, `mkdir`, `rm`, `bind`, `id`, `ln` |
| `cmd/win.c`, `cmd/dtest.c`, `cmd/bounce.c`, `cmd/scribble.c` | rio's spawn; the window tests; the demos |
| `plan9/` | verbatim 4th-edition sources + NOTICE; `include/` is our shim (u.h, libc.h) |
| `v10/` | verbatim V10 sources + NOTICE; `include/` and `lib/` are libv10, our personality libc |
| `rootfs/` | the boot filesystem; `rc/tests.rc` is the shell test suite; `bin/` is generated |

## The rc subset

Words with `'...'` quoting and `#` comments; `$var`, `$status`, `x=v` assignments (lists
arrive via substitution); `` `{...} `` command substitution; `*`/`?` globbing in the final
path component; pipelines `|`; `;` `&` `&&` `||`; redirections `>` `>>` `<`;
`if(list) pipeline`, `if not pipeline`, `for(x in words) pipeline` — single-pipeline
bodies; **subshells `(...)`**, standalone or as pipeline stages, running in a bare-forked
copy of the interpreter; builtins `cd` and `exit`, and `~` as a syntactic form whose
subject keeps its list identity and whose patterns are never globbed against the
filesystem — the shape real rc gives it. Statements are single-line.

Not there: brace groups and functions (wrap in `( )` instead — the parser says so).
Known v0 warts: quoted `*` still globs, builtins ignore redirections, prefix assignments
persist rather than scope to the command.

## v0 deviations, all deliberate

argv arrives via a boot syscall rather than pre-placed on the stack; `brk` is guest-local
(`memory.grow`); no `..`; `remove` inside a union picks no element (error); `errstr`
reads but does not exchange; pipe writers never block (unbounded queue); gid/groups and
the D1–D4 measurements are deferred per docs/identity.md; proc and wsys files have no stat or
directory reads; draw speaks `b d f L e E y i l s v` (`x`, arcs and compressed images absent; cons output
is still host-rendered), masks apply only to glyphs, `rgb` is a v0 test file;
windows persist until `wctl delete`; nested lazy fork within one Worker refused. On the wire: no `Tauth` (afid is always NOFID), no `Tflush`,
walks are one name per `Twalk`, msize is fixed at 8216, `unmount` is absent, and a failed
mid-walk leaks its intermediate fid. Each is a lifted restriction away, not a redesign.
