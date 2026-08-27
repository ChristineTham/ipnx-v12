<h1 align="center">ipnx-v12</h1>

<p align="center"><em>An operating system you can read in an afternoon,<br>
boot in a browser tab, and hand to an AI one folder at a time.</em></p>

## Open a tab

There is a whole operating system in this repository, and it runs in a browser tab.

Not a demo of one. A kernel with processes, pipes, windows and a permission model. A
shell — the real `rc`, compiled from Bell Labs' own source, every fork genuinely
returning twice. An editor — the real `sam`, the one Rob Pike wrote, drawing itself
into a window that is literally a file. Type `win sam &` and 1980s Bell Labs software
paints glyphs in Chrome through `/dev/draw`, and nobody had to port anything: the
source is verbatim, the system underneath is simply shaped the way software always
wished it was.

The kernel is about 2,700 lines. You can read *all of it* before lunch. It carries
46,000 lines of untouched Bell Labs userspace today, and it is built to carry your Go
binaries, your Python, and your git repositories tomorrow — through the same nine
operations on names. A hundred and twenty tests boot it, exercise everything from fork
to fonts, and shut it down clean, identically on Node and in Chrome.

This is not a museum piece, and it is not another Linux. It is what Unix was supposed
to become.

## The sediment

You know the feeling. A fresh server is fifteen hundred packages you never chose. Boot
is a dependency graph only systemd could love. Your app is a container running a distro
running a runtime, and hello-world ships by the gigabyte. Configuration is archaeology.
POSIX is thousands of pages, and the part anyone uses fits on a napkin.

Under all of it is the idea you fell for in the first place — small tools, plain text,
everything a file, everything composable. That idea didn't fail. **It was buried.**

The people who invented it watched it happen, and wrote their answer: Plan 9 — one
protocol for everything, namespaces per process, the network as a mount point. The
best kernel design ever shipped. And in the same motion they broke compatibility with
Unix — *"Compatibility was not a requirement for the system"* — so the world couldn't
follow, and the sediment won.

**ipnx-v12 takes their kernel and undoes the break.** Their curation of Unix preserved
above it, Unix's interface restored beside it, and the modern world welcomed in:
sockets won, UTF-8 won (they invented it), and git, Python and Go are acceptance
tests, not aspirations. No POSIX. No systemd. No sediment. The counterfactual next
edition — what the Research line would have shipped if it had kept walking.

## The tricks are the architecture

None of these are features. All of them fall out of two decisions — everything is a
file, and every process composes its own world:

- **Your window is a file.** `bind '#w/1' /dev` and a namespace *is* a window; the
  editor draws by writing to it. Every Plan 9 port in history had to give this up. A
  browser tab turns out to be the place it finally works.
- **Hand someone a world.** `exportfs` serves your namespace — private binds included —
  to another process, another container, another machine. Not a copy: the thing itself,
  over one protocol.
- **Give an AI a namespace, not an allowlist.** An agent's visible universe is
  assembled from binds: exactly the folders, tools and services you mounted, nothing
  else *reachable by construction*, with the audit log for free. This is the sandbox
  the agent ecosystem is currently faking with permission prompts.
- **The kernel cannot bloat.** Plan 9 native, WASI, and a modern Unix surface are all
  just libc dialects over the same file protocol. Personalities multiply; the kernel
  stays readable in an afternoon. There is no ioctl swamp because there is nowhere to
  dig one.
- **A computer as a function call.** The same kernel is headed for a microVM that boots
  in ~100 milliseconds, holds nothing it can't remount, and dies without ceremony —
  which is to say: the architecture Lambda runs on, with an actual operating system's
  manners.

## The principles

- **Curation over completeness.** Plan 9's `/bin` is its designers' own testimony about
  which parts of Unix were worth keeping. It is taken entire — the filters, `cron`,
  the games — absences included: `sed 10q` remains the answer to `head`.
- **Measurement over standards.** The modern personality is derived, not adopted: port
  git, CPython and Go, record every interface they actually demand, and that list is
  the specification. The useful fifth of POSIX, earned line by line.
- **Refusal and sequencing are different acts.** Anything with Unix ancestry is never
  refused, only sequenced behind its dependencies. The only true exclusions have
  neither ancestry nor necessity.
- **Personalities live in userspace, behind 9P,** where they compose and can be
  unmounted. Growth happens where it can't hurt anyone.

## What runs today

Both whole editors, on the whole stack, in a browser tab: the real `rc` (pipelines,
subshells, functions — every fork through the asyncify machinery), the real `sam` in
terminal mode and in a window over the real libframe/libdraw, **the real `acme`** —
itself a 9P file server, mounted over a pipe, executing commands under button 2 and
opening files under button 3 — real `grep sed sort ls wc` and twenty more, real
`setjmp/longjmp`, a wasm `libthread` whose blocked reads park a thread while the
process keeps scheduling, wire 9P in both directions, a uid model enforcing real
permissions — the one item Plan 9's own compatibility layer called impossible — and
hard links and symlinks minted as this edition's own wire types. And the first
proof that the modern world is welcome: **a real Go binary — compiled with
ordinary `GOOS=wasip1 go build` — and real CPython 3.14, both knowing nothing of
Plan 9, read files, list directories, sleep on timers and run scripts against
this kernel**, through a WASI shim whose single preopened directory is the
process's namespace root. **130 acceptance tests, green on Node and in Chrome.
The proof of concept is complete.** Beside it
all, TUHS-tape V10 `cat` and `echo` run unmodified in `/v10/bin`: the exhibit that
started the journey, kept in the room.

Next: a Rust kernel core with per-platform
shims — macOS, iPadOS, a `FROM scratch` OCI container, the microVM. Then the
personalities, in benchmark order: the Go binary and the Python interpreter
under the WASI shim already run — next teaching Python to actually `fork`, and
`git status` on a real repository. Beyond,
stated as aspiration and admitted by one test — *does it become a file tree in a
namespace?* — the cloud and the cluster: your S3 bucket is a directory, the model is a
file you write prompts into, the pod is a namespace, and the function is this machine,
booted for one request and gone.

## Standing on

Three systems built pieces of this: **plan9port** (the userspace without the kernel —
which is why its graphics had to abandon the file interface), **9vx** (the kernel on a
sandbox that died with x86), **Inferno's `emu`** (the right architecture on a VM that
never got an ecosystem). This is `emu`'s architecture with WebAssembly where Dis was —
the VM the whole world builds toolchains for — and a kernel small enough that
reimplementing it per-substrate is a milestone, not a lifetime.

## The documents

- **[RESEARCH.md](RESEARCH.md)** — the living evidence base: every finding with
  provenance, from Plan 9's call table to the wasm toolchain's measured behaviors.
- **[docs/v12-plan.md](docs/v12-plan.md)** — the living spec: scope, decisions with
  dates, open questions.
- **[docs/syscalls.md](docs/syscalls.md)** — the derived call list: Plan 9's 40 live
  calls dispositioned, V10's 68 routines mapped onto them.
- **[docs/uid.md](docs/uid.md)** — the uid model: why the compatibility layer could
  not, and this kernel can.

## Licence and estate

[LICENSE](LICENSE) is **MIT, inherited rather than chosen** — this is a derivative
work of Plan 9, whose copyright passed to the Plan 9 Foundation in March 2021 under
MIT; Plan 9-derived material keeps the Foundation's notice. Research Unix material
appears under Nokia's 2017 covenant, each vendored batch carrying its own NOTICE with
provenance; the V10 measurement tree stays in the parent repository so every quoted
number keeps file-and-line provenance.
