<h1 align="center">ipnx-v12</h1>

<p align="center"><em>Unix, if it were designed today.<br>
A reimplementation — not a restoration.</em></p>

**A modified Plan 9 kernel, hosted as an ordinary userspace process on macOS, iPadOS and
in the browser, carrying a Research Unix Tenth Edition personality, with WebAssembly as
the executable format.**

There is no emulated VAX here, and no disk image. That is the whole point, and it is why
this is a separate repository.

## Where this came from

[ipnx](https://github.com/ChristineTham/ipnx) restores Bell Labs Research Unix and runs it
under full-system emulation — a real V8 or V10 kernel on an emulated VAX-11/780, displayed
through an emulated DMD 5620. It is a restoration project, and its discipline is
authenticity: the tape decides the contents, and every deviation is argued for in a commit.

**That project ends at the Eleventh Edition.** This one begins where its premise breaks.

The reasoning, in the order it actually arrived:

1. A Twelfth Edition was first imagined as Research Unix *retargeted* — a new `sys/md/` and
   `sys/ml/`, the most traditional act in that repository.
2. But V10's kernel does not survive the trip. `io/` is 27,035 lines of drivers for hardware
   that will not exist; `vm/` is 5,882 lines of demand paging against an MMU that will not
   exist; `ml/` is 3,141 lines of VAX assembly — `swtch.s`, `trap.s`, `setjmp.s` — with no
   expression at all in a machine that has no registers to save.
3. What *would* survive is roughly 3,300 lines of process semantics. At which point you are
   not porting a kernel, you are writing one.
4. And the kernel worth writing already exists. Plan 9 has per-process namespaces, 9P, and
   `rfork` **by design**, not by retrofit — where V10's mount lives on `struct inode` as
   `i_mpoint`/`i_mroot`, a shared object that makes per-process namespaces deep surgery.
5. So: start from Plan 9 and add the Unix semantics back.

## The thesis

> The Unix designers themselves concluded the Unix kernel had become unwieldy, and wrote
> Plan 9. Their biggest mistake was not preserving Unix semantics.

That is not hindsight — it is on the record. From *Plan 9 from Bell Labs*, by Pike,
Presotto, Thompson and Trickey:

> "Compatibility was not a requirement for the system. Where the old commands or notation
> seemed good enough, we kept them. When they didn't, we replaced them."

A choice, not an oversight. **This project undoes it.**

Not with POSIX — POSIX is where APE's ugliness comes from, and every limitation APE
confesses is a POSIX.1-1990 feature Plan 9 refused: `sigprocmask`, controlling ttys,
sessions, `fcntl` locking. **V10 has none of them.** Sixty-eight system calls, V7-style
`ssig`, no job control, no sockets, no `mmap`, no threads. Research Unix and Plan 9 were
written three years apart by overlapping people, and the things V10 asks for are the things
Plan 9's authors still remembered wanting.

Of V10's 68 system calls, **59% are direct or library-only** against Plan 9's 40.

## The shape

| | |
|---|---|
| **Kernel** | Plan 9's, modified — hosted, not native. Built with clang. |
| **Executables** | WebAssembly. Plan 9 native binaries and recompiled V10 binaries, side by side. |
| **IPC** | 9P. The only one. |
| **Namespace** | Per-process. This is the addressing scheme *and* the security model. |
| **Devices** | File servers. There is no hardware to have a driver for. |
| **GUI** | `/dev/draw` and a rio-shaped window server, so `sam` and `acme` run. |
| **Platforms** | macOS, iPadOS, the browser. |

## The precedents, and what each got wrong

This architecture has been built three times. None of them is quite this, and the gaps are
the reason to try again:

| | Kernel | Guest execution | Why it isn't this |
|---|---|---|---|
| [**plan9port**](https://9fans.github.io/plan9port/) | **none** — a library port | native host processes | No `bind`, no namespaces, no kernel. `devdraw` abandons the file interface for graphics entirely, because Unix cannot give each client its own `/dev/draw`. |
| [**9vx**](https://swtch.com/9vx/) | Plan 9's, as a user program | **vx32**, a user-level x86 sandbox | Complete, and x86-only. Dead on Apple silicon. |
| [**Inferno `emu`**](http://doc.cat-v.org/inferno/4th_edition/inferno_ports) | Inferno's, hosted | **Dis** bytecode | The right architecture. The wrong VM — Dis never got an ecosystem. |

**This project is `emu`'s architecture with WebAssembly in place of Dis** — a portable VM
that has toolchains, runtimes and a decade of investment on every platform that matters.

And it can do the one thing plan9port could not: because per-process namespaces are the
design rather than an absence, `/dev/draw` can be an actual file, served per window, per
namespace. That is what Plan 9 does and what every Plan 9 port has had to give up.

## Status

**The architecture runs, boots to a shell, forks both ways, and speaks its protocol.**
[poc/](poc/) is a working slice — a hosted kernel in Node executing freestanding-C wasm
guests in per-process namespaces, with the lazy fork's parent resume *and* the asyncify
path for bare dual-return `rfork` (rc's subshells are forked copies of the interpreter),
pipes, a writable ramfs, a minimal `rc` with nine commands, and wire 9P at a real mount
boundary in both directions: a guest process serving 9P2000 on a pipe, mounted and read
by clients that cannot tell it from a kernel device, and exportfs handing a whole
namespace — private binds included — to another process, union directories completing
the algebra. Forty-seven acceptance tests passing, **on Node and in the browser from one
platform-neutral kernel**. The documents:

- **[RESEARCH.md](RESEARCH.md)** — the living evidence base: Plan 9's complete system call
  list, the `rfork` flags verbatim, APE's confessed limits, the `/dev/draw` message set,
  WASI's proposal phases, the wasm process-state table, the fork resume mechanism and its
  measurements, and the numbers taken against the Research Unix V10 tree. **It is
  self-contained**, because that tree is deliberately not copied here and those numbers
  cannot be re-derived.
- **[docs/v12-plan.md](docs/v12-plan.md)** — the living spec: scope, costs, decisions taken
  and the open questions.
- **[docs/syscalls.md](docs/syscalls.md)** — the first task, done: call by call, which
  system calls survive as kernel calls and which become 9P messages — Plan 9's 40 live
  calls dispositioned, V10's 68 routines mapped onto them.

Work starts from Plan 9. The V10 personality is designed later — its mapping is recorded in
RESEARCH.md §3 so it does not have to be worked out twice. The next design item is the uid
model, the one APE called impossible.

## Licence and estate

Plan 9's copyright passed to the Plan 9 Foundation in March 2021 and all previous editions
were relicensed **MIT**; plan9port carries the same terms. Research Unix is covered by
Nokia's 2017 covenant. Both estates are reasoned about in the parent repository and neither
is an obstacle here — but a V12 image mixes them, and that is answered before code, not
after.

[LICENSE](LICENSE) is therefore **MIT, inherited rather than chosen**: this is a derivative
work of Plan 9, so it takes Plan 9's terms. Plan 9-derived material keeps the Plan 9
Foundation's own copyright notice. Research Unix is a separate estate and is not covered —
no Research Unix source is copied into this repository, and the V10 measurements here are
recorded as data.
