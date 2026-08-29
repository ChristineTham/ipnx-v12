# IPNX Edition 12 ("UNIX Reimagined")

> [!NOTE]
> What if UNIX was reimplemented today, on a modern computing ecosystem? What would it look like?

UNIX is the ancestor of nearly every major computing platform today: Linux, originally a student project to create a UNIX "clone", is now the foundation of servers, supercomputers, embedded devices and Android. Apple's operating systems are a direct descendant of Unix, based on BSD and Mach. Even legacy systems like IBM mainframes and Windows computers can embed Unix or Linux. In short, there is not a single computing device today (from data centres to personal devices to home routers and appliances) that did not owe its existence to, or isn't directly influenced by, UNIX.

We all know the legendary story of how UNIX began, at Bell Labs. After the failure of the Multics project, Ken Thompson found a PDP-7 while looking for a home for his game *Space Travel*. He wrote a new disk driver and test suite, and realised he was three weeks away from a full operating system. While his wife was away, he built the first version, and the rest is history.

Unfortunately, much of this history is not pleasant. Although the system spread through universities, and generations of students (including me) were exposed to UNIX, there were issues distributing UNIX outside of academia. AT&T was initially prevented from freely selling UNIX because of an antitrust settlement, so many computer vendors created their own versions of UNIX. Later on, when UNIX became an open standard, attempts to resolve vendor differences led to POSIX, an unwieldy standard and a nightmare to implement.

Today, versions of UNIX still exist. IBM still sells machines running AIX, Oracle still supports Solaris, and HP-UX reached the end of its support life in 2025. Linux is (mostly) POSIX compliant. Apple macOS still holds the official UNIX 03 certification. This means macOS fully conforms to the standard version of the Single UNIX Specification (SUS). However, it is fair to say the world has moved on. Both Linux and macOS/iOS have evolved into operating systems that are very different from UNIX.

I have always loved UNIX. And I have always been interested in operating systems. My first significant software project, created when I was in high school, was an operating system for my Commodore 64. It was heavily inspired by and modelled after the Apple \]\[ System Monitor, written by Steve Wozniak.

I showed it to my Chemistry university professor at the University of Sydney in my first year, and he was so intrigued by it he bought it from me, for the princely sum of $200. My first (and only) software sale.

Recently, I have resurrected two editions of Research UNIX to run under open-simh on macOS and iOS, Eighth Edition, which was the actual version of UNIX I used when I was a computing science student at the University of Sydney in the 1980s, and Tenth Edition (only distributed internally within Bell Labs), which I have assembled from various archive tape copies from [The Unix Heritage Society](https://www.tuhs.org). I have affectionately named these reconstructions "IPNX" (originally "iPad is Not UNIX" as my initial goal was UNIX on an iPAD, but also "IP is Not UNIX" - a reference to the UNIX wars and SCO litigation attempts).

## IPNX Possibly Not UNIX

This project is an attempt by me to answer the question "How would UNIX be implemented today?" To coexist in a modern computing ecosystem, with personal devices, cloud containers, the Internet? A citizen in an insecure, non-trustable, locked down world of conflicting and competing interests?

It is no longer possible to find a "PDP 7" equivalent and write a new operating system for it, except on devices like a Raspberry Pi. Most consumer devices are locked from tampering by firmware - it is not possible to replace the operating system on those devices unless they are jail broken.

In the data centre, operating systems rarely run on bare metal, they typically run hosted by a hypervisor or within a container.

IPNX v12 is a project to carry on the UNIX tradition, by reimplementing it to fit into the modern computing ecosystem and modern constraints.

It is based on:

- WebAssembly (WASM) - a virtual machine format that lets you run compiled code at near-native speeds, and
- WASI (WebAssembly System Interface) - a standardised set of APIs that allows that code to securely talk to the outside world (like filesystems, networks, and system clocks).

So IPNX v12 will run everywhere - in a browser on any machine, or using a WASM runtime engine (eg. [wasmtime](https://wasmtime.dev)), or directly inside a container.

IPNX v12 consists of:

- a reimplementation of the Plan 9 kernel in Rust, as an ordinary userspace process on the host system — with a JavaScript twin that runs the same kernel in any browser; the two implementations pass an identical conformance suite — and
- both Plan 9 and UNIX v10 utilities and commands supported as WASM binaries in a per process namespace.
- The kernel supports Plan 9 syscalls natively, and UNIX v10 via a personality layer (in progress — two V10 binaries run today on a thin libc).
- The kernel is compiled using the host Rust toolchain.
- WASM binaries are also compiled using the host toolchain, but are portable and can run everywhere.
- WASI is supported, so IPNX binaries can coexist with non IPNX WASM binaries.
- 9P2000 is used as the interprocess communication mechanism between processes.
- IPNX never implements an on-disk format; today it runs on an in-memory tree seeded from the host at boot, and host and network filesystems arrive as 9P mounts.

IPNX v12 adheres to the three principles behind [Plan 9](https://9p.io/plan9/):

> First, resources are named and accessed like files in a hierarchical file system. Second, there is a standard protocol, called 9P, for accessing these resources. Third, the disjoint hierarchies provided by different services are joined together into a single private hierarchical file name space.

IPNX v12 is not a reimplementation of all of Plan 9. The kernel is fresh, inspired by the Plan 9 kernel, and the commands are partially borrowed from Plan 9. The UNIX personality is inspired by Tenth Edition, and it also lifts from that edition's userspace. The rest of it is new, reimagined for modern times.

In summary, IPNX is not Plan 9, it is not UNIX, and it most definitely is not Linux or macOS. It is what Unix may have become if it was reimplemented today.

## Security model

IPNX v12 processes are secure by design. It assumes from day one that the running application is malicious. Processes never share memory with one another after exec; the only shared regions are the transient fork window and each process's private mailbox to the kernel. They run as separate web workers (OS threads under wasmtime) and communicate with each other only through 9P and pipes. There is no superuser, IPNX inherits WASM's strict capability-based, sandboxed security architecture. Each process is limited by the capabilities and namespace provided to it. The kernel itself can be compromised on the host, so the system is not totally secure, but the attack surface is very specific and technically outside the system itself.

Of course, it is still possible to create dangerous WASM binaries through over-privilege, and the binaries themselves may contain logic vulnerabilities, so it does not eliminate all security risks, but IPNX v12 starts from a zero trust security foundation.

### WASM's Deny-All Sandbox (Isolation)

A WASM module operates inside an entirely isolated environment. It is completely blind to its host operating system.

* No Ambient Authority: A native binary or a Docker container inherits the permissions of the user executing it (e.g., access to your home directory, network, and environment variables). A WASM module has zero ambient authority.
* Explicit Imports Only: A WASM binary cannot access the filesystem, initiate a network request, or even read the system clock unless the host runtime explicitly passes that specific function into the module as an import.
* The WebAssembly System Interface (WASI): WASI manages these imports using capability-based security. Instead of giving a module permission to open any file, you must explicitly pass a file descriptor for a specific folder at startup. The module cannot traverse outside that folder. In IPNX the single preopened directory is the process's namespace root, so the namespace itself is the capability boundary.

However, note that IPNX v12 does allow fork() and rfork() so processes can inherit other processes' capabilities (by design). So practically, IPNX 12 processes may share a common set of capabilities, but a per-process namespace ensures the capabilities can be fine tuned.

### Memory Safety (Linear Memory Isolation)

One of WASM's greatest defenses against classic exploits like buffer overflows is how it structures and isolates its memory.

* The Flat Linear Memory Array: A WASM module is allocated a contiguous, flat block of memory called "linear memory". To the module, this looks like a massive array of raw bytes. It cannot see or read any memory belonging to other WASM modules, the host application, or the underlying runtime.
* Hard Bounds Checking: Every single memory read or write instruction inside WASM is dynamically checked against the maximum size of this linear memory array. If a bug or exploit tries to write data past the allocated boundaries (a classic buffer overflow), the runtime instantly halts execution with a memory trap, preventing any damage.
* No Pointer Arithmetic to Code: In a standard C/C++ program running natively, code and data live in the same address space. An attacker can overwrite a data buffer, corrupt the function return pointer, and force the CPU to execute malicious injected code. In WASM, code and data are completely segregated. Code execution pointers do not exist inside linear memory, meaning an attacker cannot overwrite the stack to jump to arbitrary code.

### Control Flow Integrity (CFI)

WASM prevents attackers from hijacking the execution flow of an application via structural constraints.

* Validated Call Targets: Function pointers do not point to raw memory addresses. Instead, they are represented as integer indices inside a strictly managed Function Table, fixed at instantiation in IPNX binaries.
* Type-Safe Indirect Calls: When a WASM module performs an indirect call (a function pointer call), the runtime verifies at the exact moment of execution that the function signature (arguments and return types) perfectly matches the expected type definition. If they do not match, the execution aborts immediately. This stops "Return-Oriented Programming" (ROP) attacks, where hackers string together random fragments of existing machine code to bypass security boundaries.

### Verification and Validation

Before a single line of WASM code is converted to machine instructions or executed, the runtime performs a mandatory, single-pass validation phase.

* Structural Integrity: The runtime scans the .wasm binary to ensure it follows the strict WebAssembly specification format.
* Type and Stack Safety: The validator performs static type checking on the stack-based operations. It ensures that functions cannot leave dangling variables on the stack, call non-existent functions, or manipulate types illegally. If a binary fails verification, it is rejected entirely before compile time.

## Users, identity and profiles

UNIX was a time-sharing system, supporting multiple users on one machine. Modern systems
are generally the other way around: one person per device, and one person using many
devices. UNIX also invented something else under the same mechanism — the daemon user, a
"user" that owns resources and runs processes but is never a person at all. Today the
daemons outnumber the people: most identities in the modern world are service accounts,
workload roles and agents.

IPNX takes the uid apart into what it actually was:

- **The person** is the owner of a kernel instance — exactly one per instance, many
  instances per person. Timesharing is inverted rather than restored: a kernel now costs
  a browser tab, so multi-tenancy happens by running another instance, not by sharing
  one. There is no `login` and no getty; you do not log into your own machine.
- **The role** is the daemon user, kept deliberately: a name that owns resources and is
  conferred, never logged into. Where a UNIX daemon got only a uid, an IPNX daemon gets
  a reduced namespace — its confinement is simply the binds it was started with.
- **The agent** is a role plus a namespace, and it is the genuinely new population. The
  name is for the audit trail; the namespace is the authority.
- **The network identity** is an authenticated claim, made per connection. There is no
  global registry of users; each server believes a proof. A person, across all their
  devices, is their keyring.

The organising rule: **names are for accounting; namespaces are for authority.**

This is also what `su` means here. It is not "superuser" — there is no superuser to
become. It is an identity transition under the kernel's rules, with no password and no
setuid machinery. The direction that matters most is downward: `su none` starts a shell
with almost nothing, which is exactly what you want before running something you do not
trust.

### The profile

A user's profile is essentially: what rights they can exercise, their namespace
configuration, which services they connect to, and their credentials, passwords and
certificates. Plan 9 built all of this, but in four separate pieces — factotum (the key
agent), secstore (the networked keyring), the per-user profile script, and the namespace
description file — and the industry then rebuilt each piece separately as password
managers, passkeys and dotfile repositories, without ever unifying them.

IPNX unifies them as the profile: a file tree, served like everything else. Namespace
fragments describe what to assemble — a base, a per-device section, a section per
service — so one profile can span a home directory on the local device, NAS mounts, and
remote git repositories, and still work on a device where some of those do not exist.
The profile speaks IPNX names, never host paths, so it is portable across devices; an
unreachable mount degrades gracefully rather than breaking the profile. Credentials are
never plain files: programs use a key by writing a challenge and reading a response, so
secrets stay inside the agent — which also lets each platform keep them in its native
secure storage. The durable copy of a profile can live anywhere mountable, including a
git repository, which makes an identity versioned and diffable.

An AI agent's identity falls out of the same design: it is given a sub-profile — fewer
namespace fragments, scoped credentials, its own name in the audit trail.

## Current Status

A kernel with processes, pipes, windows and a permission model. A
shell — the real `rc`, compiled from Plan 9 source, every fork genuinely
returning twice. An editor — the real `sam`, the one Rob Pike wrote, drawing itself
into a window that is literally a file. Type `win sam &` and 1980s Bell Labs software
paints glyphs in Chrome through `/dev/draw`.

The kernel is about 3,000 lines in JavaScript and 4,000 in Rust — small enough to
read in a sitting, twice over. It carries 61,000 lines of untouched Bell Labs
userspace today, and Go binaries and Python already run beside them — with git
repositories to come — through the same handful of operations on names. A hundred
and thirty-eight tests boot it, exercise everything from fork to fonts to
the compilers to the package manager, and shut it down clean, identically on
Node, in Chrome, on the Rust kernel under wasmtime — and, on every push, in
a 62&nbsp;MB `FROM scratch` container.

## The tricks are the architecture

These are not separate features. They are consequences of two decisions: everything is
a file, and every process composes its own world.

- **A window is a file.** `bind '#w/1' /dev` makes a namespace into a window, and the
  editor draws by writing to it. This is one of the ideas that most Plan 9 ports had to
  abandon. A browser tab provides a place where it works naturally.
- **A process can be given a world.** `exportfs` serves a namespace, including its
  private binds, to another process, container or machine. It is not a copy of the
  namespace; it is the namespace itself, served over one protocol.
- **An AI can be given a namespace rather than an allowlist.** Its visible universe is
  assembled from binds: the folders, tools and services mounted for it, and nothing else
  reachable by construction. The audit trail follows from the same design.
- **The kernel does not need to grow with every personality.** Plan 9, WASI and the
  modern UNIX interface are libc dialects over the same file protocol. The personalities
  can multiply in userspace while the kernel remains small and understandable. There is
  no ioctl interface because there is no separate mechanism to add one.
- **Installing software is a bind.** A package is a subtree under `/pkg`;
  `pkg install` verifies its digests and binds its `bin` into the union
  `/bin`. The namespace is the installation record — there is no package
  database to corrupt — and because namespaces are per-process, a person, a
  role and an agent can each carry a different set of installed software.
  The consequences reach further than convenience. Versions coexist under
  `/pkg/<name>/<version>` and each process binds the ones it wants, so there
  is no dependency solver and no dependency hell. A name that would bind
  over different bytes is refused, so conflicts are caught at install time
  rather than debugged afterwards. And a subshell that does `rfork n` and
  installs its own packages *is* a development environment — private,
  nestable, gone with the process. The tool families this replaces — venv,
  nvm and rbenv on one side, flatpak and snap on the other — are namespace
  emulations, built because their systems could not say `bind`.
- **Every system is a time machine.** A snapshot is a tree and rollback is a
  bind: `echo snap t1 > '#V/ctl'` freezes the running root by structural
  clone — twenty snapshots of the whole filesystem cost nine megabytes and
  under a second, because unchanged bytes are shared rather than copied —
  and `bind '#V/t1/dir' /dir` restores. Put the same line first in a boot
  namespace file and the system boots from its own past. History refuses
  writes from everyone, eve — the host owner — included. Snapshot volumes,
  backup daemons and immutable-distro machinery are what systems grow when
  the filesystem cannot say `snap` and the namespace cannot say `bind`.
- **A computer can be treated as a function call.** The same kernel is intended to run
  in a microVM that boots in about 100 milliseconds, mounts what it needs, and then goes
  away. This is the sort of environment used by services such as Lambda, but with an
  actual operating system underneath.

## The principles

- **Curation over completeness.** Plan 9's `/bin` is its designers' testimony about
  which parts of UNIX were worth keeping, and I am keeping the testimony. The filters
  stay as they are — pipes and files are already this system's paradigm, and `sed 10q`
  remains the answer to `head`. The screen programs are another matter: I have kissed
  compatibility goodbye and am redesigning them for modern surfaces, while the
  originals stay runnable as the exhibit, omissions and all.
- **Measurement over standards.** The modern personality is derived rather than adopted.
  I will port git, CPython and Go, record the interfaces they actually require, and use
  that list as the specification. The useful part of POSIX is earned one interface at a
  time.
- **Refusal and sequencing are different things.** Anything with UNIX ancestry is not
  refused; it is sequenced behind its dependencies. The only genuine exclusions are
  things with neither ancestry nor necessity.
- **Personalities live in userspace, behind 9P,** where they can be composed and
  unmounted. The system can grow without making the kernel more complicated.

## What runs today

The whole stack now runs in a browser tab. The real `rc` supports pipelines, subshells
and functions, with every fork going through the asyncify machinery. The real `sam`
runs in terminal mode and in a window using the real libframe and libdraw. **The real
`acme`** is also running: it is a 9P file server mounted over a pipe, executing commands
under button 2 and opening files under button 3.

There are real `grep`, `sed`, `sort`, `ls`, `wc` and more than twenty other commands,
along with working `setjmp`/`longjmp`, a WASM `libthread`, bidirectional 9P, a uid model
that enforces permissions, and hard and symbolic links implemented as this edition's
own wire types. The uid model is particularly significant, since Plan 9's own
compatibility layer considered this impossible.

The first proof that the modern world can coexist with this system is also working: **a
real Go binary, compiled with ordinary `GOOS=wasip1 go build`, and real CPython 3.14**
can read files, list directories, sleep on timers and run scripts against the kernel.
They know nothing about Plan 9. They use a WASI shim whose single preopened directory is
the process's namespace root. **138 acceptance tests pass on Node, in Chrome — and on
the Rust kernel core under wasmtime: the same suite, identical on the reference
implementation and the native rewrite. The proof of concept is complete, and the
kernel has been built twice.** Alongside it, TUHS-tape V10 `cat` and `echo` run
unmodified in `/v10/bin`, preserving the exhibit that started the project.
Before the build, four review lenses — the deployment ledger (where it runs),
design thinking (who it is for), a six-hats pass (what we had missed), and
virtue ethics (what character the work keeps) — have each been applied and
recorded in the documents below; they now repeat together, on one cadence.

Next: the per-platform
shims around the Rust core — iPadOS, a `FROM scratch` OCI container, the microVM. Then the
personalities, in benchmark order: the Go binary and the Python interpreter
under the WASI shim already run — next teaching Python to actually `fork`, and
`git status` on a real repository. Beyond,
stated as aspiration and admitted by one test — *does it become a file tree in a
namespace?* — the cloud and the cluster: your S3 bucket is a directory, the model is a
file you write prompts into, the pod is a namespace, and the function is this machine,
booted for one request and gone.

## Try it

**The whole system runs in your browser at
[christham.net/ipnx-v12](https://christham.net/ipnx-v12/).** One button boots
it: the desktop appears in a few seconds, and the toolchains — about 260 MB of
them — stream in while you look around. Nothing installs and nothing persists:
the entire machine lives in a tab's memory, and a reload forgets it ever
existed.

You arrive as `kitty`, in a home directory with programs waiting. `cc hello.c`
then `./a.out` compiles with real clang and links with real wasm-ld, driven by
a real `cc(1)` — flags, `-o`, `-c`, `-E` and all. `go run hello.go` compiles
with **the actual Go compiler** — `cmd/compile`, cross-built to WebAssembly and
running as an ordinary process on this kernel, which folklore says is
impossible; the folklore was confusing the orchestrator with the tools.
`python hello.py` is real CPython 3.14 with its full standard library, and
`pip install cowsay` fetches a real wheel from the real PyPI, verifies it and
installs it — the network arriving, naturally, as a file. The example programs
from python.org's front page and the features gobyexample.com teaches run as
written, from `examples/` in your home.

There is a package manager, and the demo is its first registry.
`pkg install ruby` fetches real Ruby 3.2.2 from the site you are already on,
verifies it against a pinned sha256, and binds it into `/bin` — installing
here is a bind, not a copy into global state, and `pkg remove` is an unbind.
`pkg install php` works the same way. `pkg install zlib` is the interesting
one: the registry's zlib was compiled from pinned source by this system's
own `cc`, because the upstream binary came from a newer LLVM than the one in
the tab — a sysroot must match its toolchain's era, and this registry
answers by building with the toolchain it serves. Then
`cc z.c /lib/wasm32-wasi/zlib/*.o` links it, and your program compresses.

The editors are there too: `win acme &` opens the real acme in a window
(option-click is button 2, command- or right-click button 3), `rc /rc/tour`
walks the whole system, and the Test suite button runs the full conformance
suite in the tab you are sitting in.

## Standing on

Three systems built important pieces of this idea: **plan9port**, the userspace without
the kernel, which is why its graphics had to abandon the file interface; **9vx**, a
kernel running in a sandbox that died with x86; and **Inferno's `emu`**, the right
architecture on a virtual machine that never acquired an ecosystem.

IPNX uses `emu`'s architecture with WebAssembly in place of Dis. WebAssembly is the
virtual machine for which the whole world now builds toolchains. The kernel is small
enough that reimplementing it for each substrate is a milestone rather than a lifetime.

The security model also has ancestors, most of them dead: capability operating systems
from Amoeba to EROS. They died of a consistent set of causes — above all, that no
existing software ran on them — and IPNX is designed against that history. The first
thing this system proved was that unmodified Go and Python binaries run; the
capabilities themselves never appear as a concept anyone must learn, because here a
capability is just a namespace, a file descriptor, a bind. Amoeba's best idea, the
cryptographically-checked ticket that became the web's signed URL, is the planned
mechanism for identity across machines. The full history and its lessons are in
[RESEARCH.md](RESEARCH.md).

## The documents

**[RESEARCH.md](RESEARCH.md)** — the living evidence base: every finding with
 provenance, from Plan 9's call table to the wasm toolchain's measured behaviors.
**[docs/design.md](docs/design.md)** — the design: scope, rationale, decisions with
 dates, open questions.
**[docs/architecture.md](docs/architecture.md)** — the architecture: what the system
 is, present tense — the component map and the contracts a host, a guest, and the
 wire must honour.
**[docs/handbook.md](docs/handbook.md)** — the handbook: how to build, run,
 extend and debug it.
**[docs/implementation.md](docs/implementation.md)** — the build plan: milestones
 with dependencies and acceptance, from the tree to the microVM.
**[docs/platforms.md](docs/platforms.md)** — the platforms: where it runs, the
 namespace map, and the deployment reviews.
**[docs/syscalls.md](docs/syscalls.md)** — the derived call list: Plan 9's 40 live
 calls dispositioned, V10's 68 routines mapped onto them.
**[docs/identity.md](docs/identity.md)** — identity: what a user is here, and the
 uid model the compatibility layer could not do, and this kernel can.
**[docs/personas.md](docs/personas.md)** — the personas: who this is for, and
 what each needs to see before believing.
**[docs/design-thinking.md](docs/design-thinking.md)** — the design-thinking
 record: how the personas were derived, what was considered, kept and refused.
**[docs/six-hats.md](docs/six-hats.md)** — the completeness checks: dated
 sweeps for what the plans keep missing.
**[docs/virtue-ethics.md](docs/virtue-ethics.md)** — the character record: the
 virtues this work practises, each held between its two failure modes.
**[docs/poc.md](docs/poc.md)** — the proof of concept's record, frozen: what three
 days built and what they proved.

## Licence and estate

[LICENSE](LICENSE) is **MIT, inherited rather than chosen** — this is a derivative
work of Plan 9, whose copyright passed to the Plan 9 Foundation in March 2021 under
MIT; Plan 9-derived material keeps the Foundation's notice. Research Unix material
appears under Nokia's 2017 covenant, each vendored batch carrying its own NOTICE with
provenance; the V10 measurement tree stays in the parent repository so every quoted
number keeps file-and-line provenance.
