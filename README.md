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

The system has three layers, and each has its own name. **Saranos** is the operating system — the whole thing, and what you would say you are running. It is Sanskrit *śaraṇa*, refuge: a refuge from the complexities of the modern computing environment. **IPNX** is the kernel and the userspace. **emca** is the windowing and user interface system. Where Apple has macOS, Darwin and Aqua, this has Saranos, IPNX and emca.

IPNX v12 consists of:

- a reimplementation of the Plan 9 kernel in Rust, as an ordinary userspace process on the host system, compiled to WASM for the browser — and
- both Plan 9 and UNIX v10 utilities and commands supported as WASM binaries in a per process namespace.
- The kernel supports Plan 9 syscalls natively; a UNIX personality is userspace, not kernel.
- The kernel is compiled using the host Rust toolchain.
- WASM binaries are also compiled using the host toolchain, but are portable and can run everywhere.
- WASI is supported, so IPNX binaries can coexist with non IPNX WASM binaries.
- 9P2000 is used as the interprocess communication mechanism between processes.
- IPNX never implements an on-disk format; the root is an in-memory tree seeded from the host at boot, and host and network filesystems arrive as 9P mounts.

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
trust. The kernel's own share of this is Plan 9's and no more: one name per process,
droppable only to `none`. `su` is a personality program, and identity is established per
file server at attach.

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

## The tricks are the architecture

These are not separate features. They are consequences of two decisions: everything is
a file, and every process composes its own world.

- **A window is a file.** Bind a window's directory over `/dev` and a namespace *is* a
  window; the editor draws by writing to it. This is one of the ideas that most Plan 9
  ports had to abandon, and a browser tab is a place where it works naturally. The
  window system is not in the kernel: emca, a user program, serves each window's files
  and the host renders them.
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
- **Installing software is a bind.** A package is a declaration — a small file
  under `/pkg` naming what to fetch, what it must hash to, and what to bind;
  `pkg install` verifies the digests, puts the files in a shared store, and
  binds its `bin` into the union `/bin`. The namespace is the installation record — there is no package
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
  bind: freeze the running root by structural clone, and `bind` the frozen
  tree back over `/dir` to restore it — twenty snapshots of the whole
  filesystem cost nine megabytes and under a second, because unchanged bytes
  are shared rather than copied. Put the same line first in a boot namespace
  file and the system boots from its own past. History refuses
  writes from everyone, eve — the host owner — included. Snapshot volumes,
  backup daemons and immutable-distro machinery are what systems grow when
  the filesystem cannot say `snap` and the namespace cannot say `bind`.
- **Every process is a jail, a container and a microVM.** Isolation is not a
  product here; it is what a process *is*. Each process composes its own
  filesystem view, posts its own services, carries its own credentials, and —
  once `/net` lands — binds its own network interfaces, so a whole cluster of
  processes with different roles can run side by side on one kernel. The
  walls are double by construction: the namespace bounds what a process can
  name, and the WebAssembly instance bounds what it can do. And because 9P is
  the only IPC, two processes on one machine already talk the way two
  machines on a wire do; jails, containers and much of the orchestration
  around them are what systems build when isolation is bolted on afterwards.
  Sun's motto was "The network is the computer." Our computer is a network.
- **The orchestrator is a file server.** A Dockerfile here is a directory: a
  namespace file, declared packages, a credential, a command — a declaration,
  not a script, because installing is a bind. `run` instantiates it; `svc`
  keeps N alive; and kubectl is `cat` and `echo`: desired state is a file you
  write, observation is a file you read. Built in two small programs on an
  unmodified kernel, because the kernel already had the primitive.
- **A computer can be treated as a function call.** The same kernel is intended to run
  in a microVM that boots in about 100 milliseconds, mounts what it needs, and then goes
  away. This is the sort of environment used by services such as Lambda, but with an
  actual operating system underneath.
- **Every screen is the same computer.** IPNX runs today in a browser tab in an
  Internet café — securely by construction: nothing installs, nothing persists,
  the whole system lives and dies inside the page's sandbox — and on macOS with
  full JIT. It will run inside VSCode, in a browser included, and on iPad, where
  the app lends WebKit's own JIT to the kernel, SwiftUI to the screen, and its
  file entitlements to the namespace. Every surface is a shell lending the one
  port a place to run, a screen to draw on and files to touch. I think this is
  an achievement no other WASM runtime, development environment or Linux
  distribution can easily claim.

## The stretch: a distributed operating system

The design stretch for IPNX is a distributed operating system — which is a
return, not a departure: Plan 9 was one first, and what runs today is the
single-kernel half of its circle. There is no reason the process
orchestrator here cannot orchestrate processes on other systems, so we can
truly replace the orchestration layer if we choose to — within the honesty
already on the record about what never dissolves. That needs three things,
and their seeds are already in the tree: **process migration and
rehosting** (a process file is declarative, so rehosting is running the
same spec elsewhere; and every fork in this system already serialises a
whole process — live migration is that snapshot shipped to another kernel
instead of a sibling worker); **discovery** of other systems and their
capabilities, as files, where "what can you do" is `ls`; and **a user
identity that spans systems** — tickets on the wire, keys in an agent, the
profile as the portable person. Sun said the network is the computer. Our
computer is a network, and the network of our computers is one system.

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
- **Complexity is compensation.** Linux, systemd, Docker and Kubernetes are overly
  complex because the kernel beneath them never had a per-process namespace; when a
  kernel lacks a primitive, userspace grows an industry. Each trick above is one of
  those industries, dissolved. We are not only living in the modern ecosystem — we
  are simplifying and replacing it.

## What runs today

A kernel of Plan 9's own shape, 1,617 lines of Rust with no dependencies:
`Chan` as the object every name resolves to, a device table that is Plan 9's
`struct Dev`, a namespace keyed by the identity of the channel mounted upon,
`namec` checking that namespace at every path component, and `rfork`'s
share-copy-clear over namespace, file descriptors and environment alike. 27
tests.

Under it, `hosts/ipnx` implements the machine-dependent half — `procsetup` and
`touser`, the only two calls that know what a machine is — over wasmtime.
`cargo run -p ipnx` resolves a name through a namespace, reads the image out of
`#/`, instantiates it and runs it.

There is no filesystem beyond `#/` yet, no pipe or mount device, no shell and
no userspace. [docs/when.md](docs/when.md) is the only place that says what is
built, and it says so exactly.

The target is the demo: `rc` in a terminal with every command, then the website
with emca. Twelve capabilities, measured by a conformance suite that fails
until all twelve are reached.

## The documents

**[RESEARCH.md](RESEARCH.md)** — the living evidence base: every finding with
 provenance, from Plan 9's call table to the wasm toolchain's measured behaviors.
**[docs/implementation.md](docs/implementation.md)** — the plan: phases P0–P7,
 from the kernel to the demo to hardware.
**[docs/when.md](docs/when.md)** — what is built, and what is not. The only
 document that says.
**[docs/architecture.md](docs/architecture.md)** — the architecture: what the system
 is, present tense — the component map and the contracts a host, a guest, and the
 wire must honour.
**[docs/handbook.md](docs/handbook.md)** — the handbook: how to build, run,
 extend and debug it.
**[docs/implementation.md](docs/implementation.md)** — the plan, replanned 2026-09-04:
 phases P0–P11 in three layers — IPNX, emca, Saranos — with the demo (`ipnx` in a
 terminal, and the website) as the first milestone and every target to real hardware
 as the last.
**[docs/platforms.md](docs/platforms.md)** — the platforms: where it runs, the
 namespace map, and the deployment reviews.
**[docs/syscalls.md](docs/syscalls.md)** — the derived call list: Plan 9's 40 live
 calls dispositioned, V10's 68 routines mapped onto them. Under revision: the derivation
 ran from V10 inward, which is how a link syscall Plan 9 does not have got in.
**[docs/identity.md](docs/identity.md)** — identity: what a user is here, and the
 uid model the compatibility layer could not do, and this kernel can. Under revision:
 the model may be right as a personality; what the kernel holds is narrowing to Plan 9's.
**[docs/personas.md](docs/personas.md)** — the personas: who this is for, and
 what each needs to see before believing.
**[docs/design-thinking.md](docs/design-thinking.md)** — the design-thinking
 record: how the personas were derived, what was considered, kept and refused.
**[docs/six-hats.md](docs/six-hats.md)** — the completeness checks: dated
 sweeps for what the plans keep missing.
**[docs/virtue-ethics.md](docs/virtue-ethics.md)** — the character record: the
 virtues this work practises, each held between its two failure modes.
**[docs/verbatim.md](docs/verbatim.md)** — every statement of Christine's, quoted
 from the session record. The one part of the documents she has actually said.

## Licence and estate

[LICENSE](LICENSE) is **MIT, inherited rather than chosen** — this is a derivative
work of Plan 9, whose copyright passed to the Plan 9 Foundation in March 2021 under
MIT; Plan 9-derived material keeps the Foundation's notice. Research Unix material
appears under Nokia's 2017 covenant, each vendored batch carrying its own NOTICE with
provenance; the V10 measurement tree stays in the parent repository so every quoted
number keeps file-and-line provenance.
