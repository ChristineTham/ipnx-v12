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

## Saranos

**Saranos is the operating system** — the whole thing, and what you would say
you are running. Its name is Sanskrit *śaraṇa*, refuge: a refuge from the
complexities of the modern computing environment. **IPNX is its kernel and
userspace**, and **emca its windowing and user interface system**. Where Apple
has macOS, Darwin and Aqua, this has Saranos, IPNX and emca.

**Saranos is a symbiosis of a host and WebAssembly.** The host is whatever the
system runs in — a program in a terminal, an app on a Mac or an iPad, a page in
a browser — and it gives the system its processors, memory, files and screen.
The WebAssembly side is the kernel and every program. Neither is anything
without the other, and the host is not kept out: a program here can reach the
host's files and run the host's commands. This is not a guest environment
isolated from its host; it is one system with two halves.

## What it is made of

This is the design; what is built of it is [docs/when.md](docs/when.md).

- **A kernel that is a subset of Plan 9's.** It is reimplemented in Rust, runs
  as an ordinary process on the host, and does the one thing a kernel does
  best — process orchestration. It does not grow by invention: what Plan 9's
  kernel has may be here, and what it does not have belongs to the host or to
  userspace.
- **Plan 9's userland, whole.** Its libraries, its commands, `rc` and APE are
  vendored and compiled to WebAssembly — nothing cut down, nothing rewritten
  smaller.
- **WebAssembly as the executable format.** A program is a module, found in
  the namespace like any other file.
- **Personalities in userspace.** A Unix interface is a library over the same
  calls — Plan 9's own APE is one — never a feature of the kernel.
- **WASI programs as they are.** A Go or Python program built for WASI runs as
  if it were on a plain WASI engine; only the system's own programs know its
  conventions.
- **The host's resources, as files.** The root of the file system is a
  directory the host serves, and its screen, keyboard and mouse are served the
  same way. 9P is the only protocol between processes and the servers they
  use.
- **Host commands, explicitly.** `os go build` runs the host's Go: a host
  command is always typed through `os` — Inferno's command for exactly this —
  and never hidden under a name of the system's own. **Some toolchains depend
  on the host.** Go's and C's run on the host, so they are there in a terminal
  or on a Mac, and not in a browser or on an iPad, where the host runs no
  commands.

IPNX adheres to the three principles behind [Plan 9](https://9p.io/plan9/):

> First, resources are named and accessed like files in a hierarchical file system. Second, there is a standard protocol, called 9P, for accessing these resources. Third, the disjoint hierarchies provided by different services are joined together into a single private hierarchical file name space.

In summary, IPNX is not Plan 9, it is not UNIX, and it most definitely is not
Linux or macOS. It is what Unix may have become if it was reimplemented today.

## Trust

**Names are for accounting; namespaces are for authority.** What a program
can name is what it can touch: its namespace is its authority, the host's
files among it wherever the host serves them. A program that types `os` runs a
command as the host's user, and can do what that user can — which is why it is
typed, never hidden.

WebAssembly still does what it does: every load and store is checked against
the bounds of the program's memory, an indirect call is checked against its
type, and a module is validated before it runs. That protects the system from
a broken program. It is not a wall between the system and its host — the host
is half of the system.

The kernel, the host and what the build produces are trusted. A server reached
over the network is not — its client applies the protocol and its own policy —
and nor is a client: the server decides what its name may do.

## People, packages and projects

The default user is **kitty**. `su` changes who you are — `su mimmy` creates a
process with mimmy's home and mimmy's credentials — and it is also how a
package, a service or a template is installed for the whole system rather than
for one user.

- **A profile** is how a user wants their namespace organised: their
  configuration, environment variables and start-up scripts. The system's is
  `/profile`, which holds what Unix keeps in `/etc`; a user's is
  `/home/profile`.
- **A package** is like a FreeBSD pkg or an apt package: a list of files to
  bind into the namespace, and any scripts that set it up. **Installing is a
  bind.** Versions coexist side by side, and a subshell that does `rfork n` and
  installs a package has it to itself.
- **A service** installs a daemon — for the system, a user or a project.
- **A template** is a prototype for a project — a Node or a Python project,
  say: the packages it needs, and its scaffolding.
- **A project** is the working folder for what may become a template, a
  package or a service. Opening a project opens a session with its packages
  there; changing into its directory only shows its files.

## What the design gives

These are consequences of two decisions — everything is a file, and every
process composes its own namespace:

- **A window is a file.** A window's content is always a file, and the host
  decides whether to show it as text, an image, a table or formatted text.
  emca serves every window's files, and any program can write to a window.
- **A process can be given a world.** Plan 9's `exportfs` serves a namespace —
  binds and all — to another process or another machine: not a copy, the
  namespace itself.
- **An agent can be given a namespace rather than an allowlist** — the folders,
  tools and services bound for it, and nothing else it can name.
- **The kernel does not grow with each personality.** Plan 9's interface and
  APE's are libraries over the same calls, and WASI programs run under a WASI
  engine. There is no `ioctl`, because there is no second mechanism to add one
  to.
- **Every screen is the same system.** One kernel and one userspace are meant
  to run in a terminal, a browser page, a Mac app, an iPad app and a container,
  and then in a microVM and on real hardware. Each host lends a place to run, a
  screen and files — and what the host has, the system has.

## The principles

- **Everything is a file, every process has its own namespace, and 9P is the
  only protocol.**
- **Plan 9 is the guide.** The kernel is a subset of Plan 9's. A departure from
  Plan 9 is allowed only to adapt it to WebAssembly and WASI, and then in a way
  that would serve another virtual machine — Dis, or the CLR — as well.
- **Curation over completeness, and nothing cut down.** Plan 9's userland is its
  designers' testimony about which parts of UNIX were worth keeping, so it is
  built whole.
- **Symbiosis, said out loud.** The host is half of the system, and where
  something depends on it — a toolchain, a command — the system says so.
- **As WASI-native as possible.** Software built for WASI runs as it is.
- **Complexity is compensation.** Linux, systemd, Docker and Kubernetes grew
  because the kernels beneath them had no per-process namespace; when a kernel
  lacks a primitive, userspace grows an industry. A system that has the
  primitive does not need the industry.

## What is built, and what is next

What is built is [docs/when.md](docs/when.md) — the only document that says.
The plan is [docs/implementation.md](docs/implementation.md): phases P0–P8 to
the first milestone, the demo — `ipnx` booting to `rc` in a terminal, then the
website, with emca in a browser. How to build and run it is
[docs/handbook.md](docs/handbook.md).

## The documents

Every document answers one of six questions.

| | |
|---|---|
| **why** | this README, and [docs/why.md](docs/why.md) |
| **who** | [docs/personas.md](docs/personas.md) |
| **what** | [docs/architecture.md](docs/architecture.md) — the contracts; [docs/saranos.md](docs/saranos.md) — the layers and the host; [docs/emca.md](docs/emca.md), [docs/window.md](docs/window.md), [docs/compositor.md](docs/compositor.md), [docs/surface.md](docs/surface.md), [docs/type.md](docs/type.md), [docs/canvas.md](docs/canvas.md), [docs/acme.md](docs/acme.md) — the windowing system; [docs/userland.md](docs/userland.md), [docs/syscalls.md](docs/syscalls.md), [docs/identity.md](docs/identity.md), [docs/packages.md](docs/packages.md), [docs/projects.md](docs/projects.md) |
| **where** | [docs/platforms.md](docs/platforms.md) |
| **when** | [docs/when.md](docs/when.md) — what is built, and what is not |
| **how** | [docs/implementation.md](docs/implementation.md) — the plan; [docs/handbook.md](docs/handbook.md) — the practice |

Beside them: [RESEARCH.md](RESEARCH.md), the evidence, every finding with its
provenance; [docs/proposals.md](docs/proposals.md), the designs awaiting review;
and [docs/verbatim.md](docs/verbatim.md), Christine's own words, quoted.

## Licence

[LICENSE](LICENSE) is **MIT, inherited rather than chosen**: Plan 9's copyright
passed to the Plan 9 Foundation in March 2021 under MIT, and the Plan 9 sources
vendored under `userspace/` keep the Foundation's notice. Ghostscript, as Plan 9
distributes it, is under the AFPL or the GPL instead. No Research Unix source is
in this repository; the Tenth Edition measurements in RESEARCH.md are data
taken in the [parent repository](https://github.com/ChristineTham/ipnx).
