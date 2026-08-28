# IPNX Edition 12 ("UNIX Reimagined")

> [!NOTE]
> What if UNIX was reimplemented today, on a modern computing ecosystem? What would it look like?

UNIX is the ancestor of nearly every major computing platform today: Linux, originally a student project to create a UNIX "clone", is now the foundation of servers, supercomputers, embedded devices and Android. Apple's operating systems are a direct descendant of Unix, based on BSD and Mach. Even legacy systems like IBM mainframes and Windows computers can embed Unix or Linux. In short, there is not a single computing device today (from data centres to personal devices to home routers and appliances) that did not owe it's existence to, or isn't directly influenced by, UNIX.

We all know the legendary story of how UNIX began, at Bell Labs. After the failure of the Multics project, Ken Thompson found a PDP-7 while looking for a home for his game *Space Travel*. He wrote a new disk driver and test suite, and realised he was three weeks away from a full operating system. While his wife was away, he built the first version, and the rest is history.

Unfortunately, much of this history is not pleasant. Although the system spread through universities, and generations of students (including me) were exposed to UNIX, there were issues distributing UNIX outside of academia. AT&T was initially prevented from freely selling UNIX because of an antitrust settlement, so many computer vendors created their own versions of UNIX. Later on, when UNIX became an open standard, attempts to resolve vendor differences led to POSIX, an unwieldy standard and a nightmare to implement.

Today, versions of UNIX still exist. IBM, HP and Oracle still sell machines that run their versions of the operating system. Linux is (mostly) POSIX compliant. Apple macOS still holds the official UNIX 03 certification. This means macOS fully conforms to the standard version of the Single UNIX Specification (SUS). However, it is fair to say the world has moved on. Both Linux and macOS/iOS have evolved into operating systems that are very different from UNIX.

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

INPX v12 consists of:

- a reimplementation of the Plan 9 kernel in Rust, as an ordinary userspace process on the host system, with
- both Plan 9 and UNIX v10 utilities and commands supported as WASM binaries in a per process namespace.
- The kernel supports Plan 9 syscalls natively, and UNIX v10 via a personality layer.
- The kernel is compiled using the host Rust toolchain.
- WASM binaries are also compiled using the host toolchain, but are portable and can run everywhere.
- WASI is supported, so IPNX binaries can coexist with non IPNX WASM binaries.
- 9P2000 is used as the interprocess communication mechanism between processes.
- IPNX does not implement a filesystem, it leverages existing filesystems from the host and the Internet via 9P.

IPNX v12 adheres to the three principles behind [Plan 9](https://9p.io/plan9/):

> First, resources are named and accessed like files in a hierarchical file system. Second, there is a standard protocol, called 9P, for accessing these resources. Third, the disjoint hierarchies provided by different services are joined together into a single private hierarchical file name space.

IPNX v12 is not a reimplementation on all of Plan 9. The kernel is fresh, inspired by the Plan 9 kernel, and the commands are partially borrowed from Plan 9. The UNIX personality is inspired by Tenth Edition, and it also lifts from that edition's userspace. The rest of it is new, reimagined for modern times.

In summary, IPNX is not Plan 9, it is not UNIX, and it most definitely is not Linux or macOS. It is what Unix may have become if it was reimplemented today.

## Security model

IPNX v12 processes are secure by design. It assumes from day one that the running application is malicious. Processes do not share memory, there is no shared memory implementation. They run as separate web workers and communicate with each other only through 9P and WASI interfaces. There is no superuser, IPNX inherits WASM's strict capability-based, sandboxed security architecture. Each process is limited by the capabilities and namespace provided to it. The kernel itself can be compromised on the host, so the system in not totally secure, but the attack surface is very specific and technically outside the system itself.

Of course, it is still possible to create dangerous WASM binaries through over-privilege, and the binaries themselves may contain logic vulnerabilities, so it does not eliminate all security risks, but IPNX v12 starts from a zero trust security foundation.

### WASM's Deny-All Sandbox (Isolation)

A WASM module operates inside an entirely isolated environment. It is completely blind to its host operating system.

* No Ambient Authority: A native binary or a Docker container inherits the permissions of the user executing it (e.g., access to your home directory, network, and environment variables). A WASM module has zero ambient authority.
* Explicit Imports Only: A WASM binary cannot access the filesystem, initiate a network request, or even read the system clock unless the host runtime explicitly passes that specific function into the module as an import.
* The WebAssembly System Interface (WASI): WASI manages these imports using capability-based security. Instead of giving a module permission to open any file, you must explicitly pass a file descriptor for a specific folder at startup. The module cannot traverse outside that folder.

However, note that IPNX v12 does allow fork() and rfork() so processes can inherit other processes' capabilities (by design). So practically, IPNX 12 processes may share a common set of capabilities, but a per-process namespace ensures the capabilities can be fine tuned.

### Memory Safety (Linear Memory Isolation)

One of WASM's greatest defenses against classic exploits like buffer overflows is how it structures and isolates its memory.

* The Flat Linear Memory Array: A WASM module is allocated a contiguous, flat block of memory called "linear memory". To the module, this looks like a massive array of raw bytes. It cannot see or read any memory belonging to other WASM modules, the host application, or the underlying runtime.
* Hard Bounds Checking: Every single memory read or write instruction inside WASM is dynamically checked against the maximum size of this linear memory array. If a bug or exploit tries to write data past the allocated boundaries (a classic buffer overflow), the runtime instantly halts execution with a memory trap, preventing any damage.
* No Pointer Arithmetic to Code: In a standard C/C++ program running natively, code and data live in the same address space. An attacker can overwrite a data buffer, corrupt the function return pointer, and force the CPU to execute malicious injected code. In WASM, code and data are completely segregated. Code execution pointers do not exist inside linear memory, meaning an attacker cannot overwrite the stack to jump to arbitrary code.

### Control Flow Integrity (CFI)

WASM prevents attackers from hijacking the execution flow of an application via structural constraints.

* Validated Call Targets: Function pointers do not point to raw memory addresses. Instead, they are represented as integer indices inside a strictly managed, immutable Function Table.
* Type-Safe Indirect Calls: When a WASM module performs an indirect call (a function pointer call), the runtime verifies at the exact moment of execution that the function signature (arguments and return types) perfectly matches the expected type definition. If they do not match, the execution aborts immediately. This stops "Return-Oriented Programming" (ROP) attacks, where hackers string together random fragments of existing machine code to bypass security boundaries.

### Verification and Validation

Before a single line of WASM code is converted to machine instructions or executed, the runtime performs a mandatory, single-pass validation phase.

* Structural Integrity: The runtime scans the .wasm binary to ensure it follows the strict WebAssembly specification format.
* Type and Stack Safety: The validator performs static type checking on the stack-based operations. It ensures that functions cannot leave dangling variables on the stack, call non-existent functions, or manipulate types illegally. If a binary fails verification, it is rejected entirely before compile time.

## Current Status

A kernel with processes, pipes, windows and a permission model. A
shell — the real `rc`, compiled from Plan 9 source, every fork genuinely
returning twice. An editor — the real `sam`, the one Rob Pike wrote, drawing itself
into a window that is literally a file. Type `win sam &` and 1980s Bell Labs software
paints glyphs in Chrome through `/dev/draw`.

The kernel is about 2,700 lines. It carries
46,000 lines of untouched Bell Labs userspace today, and it is built to carry Go
binaries, Python, and git repositories tomorrow — through the same nine
operations on names. A hundred and twenty tests boot it, exercise everything from fork
to fonts, and shut it down clean, identically on Node and in Chrome.

## The tricks are the architecture

These are not separate features. They are consequences of two decisions: everything is
a file, and every process composes its own world.

- **A window is a file.** `bind '#w/1' /dev` makes a namespace into a window, and the
  editor draws by writing to it. This is one of the ideas that most Plan 9 ports had to
  abandon. A browser tab provides a place where it works naturally.
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
  - **A computer can be treated as a function call.** The same kernel is intended to run
  in a microVM that boots in about 100 milliseconds, mounts what it needs, and then goes
  away. This is the sort of environment used by services such as Lambda, but with an
  actual operating system underneath.

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
  - **Curation over completeness.** Plan 9's `/bin` is its designers' testimony about
  which parts of UNIX were worth keeping. I have taken it whole: the filters, `cron`,
  and the games, including its omissions. `sed 10q` remains the answer to `head`.
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
the process's namespace root. **131 acceptance tests pass on Node, in Chrome — and on
the Rust kernel core under wasmtime: the same suite, identical on the reference
implementation and the native rewrite. The proof of concept is complete, and the
kernel has been built twice.** Alongside it, TUHS-tape V10 `cat` and `echo` run
unmodified in `/v10/bin`, preserving the exhibit that started the project.

Next: the per-platform
shims around the Rust core — iPadOS, a `FROM scratch` OCI container, the microVM. Then the
personalities, in benchmark order: the Go binary and the Python interpreter
under the WASI shim already run — next teaching Python to actually `fork`, and
`git status` on a real repository. Beyond,
stated as aspiration and admitted by one test — *does it become a file tree in a
namespace?* — the cloud and the cluster: your S3 bucket is a directory, the model is a
file you write prompts into, the pod is a namespace, and the function is this machine,
booted for one request and gone.

## Standing on

Three systems built important pieces of this idea: **plan9port**, the userspace without
the kernel, which is why its graphics had to abandon the file interface; **9vx**, a
kernel running in a sandbox that died with x86; and **Inferno's `emu`**, the right
architecture on a virtual machine that never acquired an ecosystem.

IPNX uses `emu`'s architecture with WebAssembly in place of Dis. WebAssembly is the
virtual machine for which the whole world now builds toolchains. The kernel is small
enough that reimplementing it for each substrate is a milestone rather than a lifetime.

## The documents

**[RESEARCH.md](RESEARCH.md)** — the living evidence base: every finding with
 provenance, from Plan 9's call table to the wasm toolchain's measured behaviors.
**[docs/v12-plan.md](docs/v12-plan.md)** — the living spec: scope, decisions with
 dates, open questions.
**[docs/syscalls.md](docs/syscalls.md)** — the derived call list: Plan 9's 40 live
 calls dispositioned, V10's 68 routines mapped onto them.
**[docs/uid.md](docs/uid.md)** — the uid model: why the compatibility layer could
 not, and this kernel can.

## Licence and estate

[LICENSE](LICENSE) is **MIT, inherited rather than chosen** — this is a derivative
work of Plan 9, whose copyright passed to the Plan 9 Foundation in March 2021 under
MIT; Plan 9-derived material keeps the Foundation's notice. Research Unix material
appears under Nokia's 2017 covenant, each vendored batch carrying its own NOTICE with
provenance; the V10 measurement tree stays in the parent repository so every quoted
number keeps file-and-line provenance.
