<h1 align="center">ipnx-v12</h1>

<p align="center"><em>Unix, written afresh.<br>
Plan 9's kernel. Unix's interface. Today's conventions. None of the accretions.</em></p>

**ipnx-v12 is a reimagining of Unix** — the system Research Unix would have become if its
authors had kept walking: their own next kernel underneath, Unix's interface restored on
top, and the modern world's settled conventions adopted without ceremony. It is not a
restoration, not a distribution, and not POSIX. It is a clean, fresh Unix on a modern
substrate:

- **a modified Plan 9 kernel, hosted as an ordinary userspace process** — in the browser,
  on macOS and iPadOS, in an OCI container, and eventually directly on a hypervisor;
- **WebAssembly as the executable format** — every program, every personality, one
  substrate;
- **9P as the only IPC**, and **per-process namespaces** as both the addressing scheme and
  the security model;
- **personalities as libc dialects** over that one kernel — Plan 9's own, a WASI ABI, and
  a modern Unix surface — so the kernel cannot bloat, ever, by construction.

Three refusals define the edges: **no POSIX** (the standard, with its accreted
bureaucracy — the useful fifth of it is derived by measurement instead), **no systemd**
(boot is an rc script and a namespace file; there is no service-manager-shaped hole to
fill badly), and **no Linux/BSD sediment** (there is no `ioctl` swamp to grow — the
kernel's entire interface is a file protocol). Three adoptions define the present:
**sockets won** (the BSD API rides `/net` files underneath), **UTF-8 won** (Plan 9's
authors invented it; this system is rune-native end to end), and **modern software must
run** — git, Python, Go are the acceptance tests, not aspirations.

## The lineage

[ipnx](https://github.com/ChristineTham/ipnx) resurrected Research Unix — real V8/V10
kernels on an emulated VAX, the tape as the source of truth. The resurrection succeeded,
and proved that resurrection is a dead end: the hardware is gone, the world moved, and a
museum is not an operating system. **v12 is the counterfactual next edition instead** —
the question "what would the Research line have shipped next?" answered with the benefit
of knowing what its authors actually did next. They wrote Plan 9. From their own paper:

> "Compatibility was not a requirement for the system. Where the old commands or notation
> seemed good enough, we kept them. When they didn't, we replaced them."

That was the pivot: a better kernel, and a break with Unix that kept it from mattering.
**This project takes the kernel and undoes the break.** Everything they carried through
the pivot is kept — their `/bin` is treated as the definitive curation of Unix, refusals
included (`sed 10q` remains the answer to `head`). Everything the break discarded that
the world still runs on is restored as a personality above the kernel, never surgery
inside it. Starting from Plan 9 is not a compromise: it is paying respect to Unix's
creators by starting from where they finished.

## The design

| | |
|---|---|
| **Kernel** | Plan 9's semantics, reimplemented hosted — ~2,700 lines carrying 46,000+ lines of verbatim 4th-edition userspace |
| **Executables** | WebAssembly, no exceptions |
| **IPC** | 9P. The only one. |
| **Namespace** | Per-process; unions, binds, and `exportfs` hand whole worlds between processes — and, in containers, between machines |
| **Devices** | File servers. There is no hardware to have a driver for. |
| **GUI** | `/dev/draw` as an actual per-window, per-namespace file — the thing every Plan 9 port had to give up — with the real `sam`, `samterm`, and (next) `acme` drawing on it |
| **Personalities** | Plan 9 native (running now) · WASI (Go `wasip1`, CPython's wasi builds) · a measured modern-Unix libc for source ports, git first |
| **Platforms** | Browser and Node today; then a Rust kernel core with per-platform shims — macOS, iPadOS, `FROM scratch` OCI, and a hypervisor-direct microVM |

## The principles

- **Curation over completeness.** Plan 9's command set is the designers' own testimony
  about what Unix was worth; it is taken entire — small filters, `cron`, the games — and
  its deliberate absences are honored too.
- **Measurement over standards.** The modern personality is not POSIX adopted but a
  surface *derived*: port git, CPython, and Go's runtime expectations, record every
  interface they actually demand, and that list is the specification. (APE chased the
  whole standard and confessed its failures; the lesson is kept, re-aimed.)
- **Refusal and sequencing are different acts.** Software with Research ancestry is never
  refused, only sequenced behind its dependencies — the float door, `/net`, a device not
  yet offered. The only true exclusions have neither ancestry nor necessity: the web-era
  period pieces, and the pivot's own infrastructure (fossil, factotum), whose roles the
  host layer fills.
- **The kernel cannot bloat.** Every personality is a libc dialect over the same nine-ish
  file operations. Growth happens in userspace, behind 9P, where it composes and can be
  unmounted.

## What runs today

**The whole editor, on the whole stack, in a browser tab.** `poc/` boots a hosted kernel
(Node and browser from one neutral core) running the **real Plan 9 userspace compiled
from its own 4th-edition source**: the real `rc` (bison over its own grammar, every fork
genuinely returning twice through the asyncify machinery), the real `sam` — terminal
mode and windowed, `win sam &` opening samterm over the real libframe/libdraw — real
`grep sed sort ls wc` and twenty more, real `setjmp/longjmp`, a wasm `libthread` whose
blocked reads park a thread while the process keeps scheduling, wire 9P in both
directions with `exportfs` handing private namespaces across processes, the uid model
(the item APE called impossible) enforcing V10 permissions, and hard links and symlinks
minted as this edition's own wire types. **120 acceptance tests pass identically on Node
and in Chrome.** Beside it all, TUHS-tape V10 `cat` and `echo` run unmodified in
`/v10/bin` — the exhibit that started the journey, kept in the room.

Next: `acme` closes the proof of concept; then the Rust kernel core and its shims; then
the personalities in benchmark order — a Go binary, a Python interpreter, and `git
status` on a real repository, each through its own dialect of the same kernel.

## Standing on

Three prior systems built pieces of this architecture: **plan9port** (the userspace,
minus the kernel — which is why its `devdraw` had to abandon the file interface),
**9vx** (the kernel, on a sandbox that died with x86), and **Inferno's `emu`** (the
right architecture, on a VM that never got an ecosystem). This project is `emu`'s
architecture with WebAssembly where Dis was — the VM with the toolchains — and a kernel
small enough (~2,700 lines) that reimplementing it per-substrate is a milestone, not a
lifetime.

## The documents

- **[RESEARCH.md](RESEARCH.md)** — the living evidence base: every finding with
  provenance, from Plan 9's call table to the wasm toolchain's measured behaviors.
- **[docs/v12-plan.md](docs/v12-plan.md)** — the living spec: scope, decisions with
  dates, the open questions.
- **[docs/syscalls.md](docs/syscalls.md)** — the derived call list: Plan 9's 40 live
  calls dispositioned, V10's 68 routines mapped onto them.
- **[docs/uid.md](docs/uid.md)** — the uid model: why APE could not and this kernel can.

## Licence and estate

[LICENSE](LICENSE) is **MIT, inherited rather than chosen** — this is a derivative work
of Plan 9, whose copyright passed to the Plan 9 Foundation in March 2021 under MIT, and
Plan 9-derived material keeps the Foundation's notice. Research Unix material appears
under Nokia's 2017 covenant, each vendored batch carrying its own NOTICE with
provenance; the V10 measurement tree itself stays in the parent repository so every
quoted number keeps file-and-line provenance.
