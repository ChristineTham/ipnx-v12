# The plan

**Role: a *how* — the plan.** Rewritten on 2026-09-17 as a **redesign and
rebuild**. The plan it replaces —
[archive/implementation-2026-09-04.md](archive/implementation-2026-09-04.md) —
was a refactor: it was written as a sequence of deletions from an existing
tree, sized against that tree's line counts. That tree is not being built on,
so a plan phrased as surgery on it no longer says anything. What survives from
it is the destination and the two demo targets.

**The end of the plan is IPNX and Saranos implemented on every target named:
browser, macOS app, iOS app, container, MicroVM, real hardware.** Gaps are
written down where found and filled in order — designed when reached, not now.
**The demo is the first milestone and the present focus.**

## Vocabulary

Three **layers** are the *what* — the architecture, from
[saranos.md](saranos.md):

| layer | | |
|---|---|---|
| **IPNX** | the kernel and the userspace | **no window, no mouse, no draw** — which is *why* none of those can be in the kernel |
| **emca** | the windowing and UI system | spans the IPNX side and the host side |
| **Saranos** | the operating system as a product | the host and the wasm side together |

**Phases** are the *when* — P0, P1, … Every phase states what it **builds**,
what it **depends on**, its **acceptance**, and the **gaps it exposes**.
Nothing undesigned is built.

**The conformance suite** (`conformance/`) answers one question: **have we
reached functional equivalence with the demo?** It **fails until we have** —
that is what the word means, and an earlier version reported `ok` while
measuring 0 of 12, which is the false signal a suite exists to prevent. Use
`cargo test -p ipnx-kernel` for day-to-day work. It is a checklist of what a
person can DO, taken from the demo itself, and it starts almost entirely
unreached — that is the point, because it is a distance and it shrinks as
phases land.

Equivalence is in **features, not mechanism, and not presentation**: nothing
in the suite says how a thing is done or how it looks. The new surface will be
very different, so a check wired to tabs, panes or placement would hold the
rebuild to the design it exists to replace. It
is **not** there to lock the design — the design is argued in the documents,
and a test asserting "the call list is a subset" would freeze a decision rather
than measure a system. Guards of that kind are unit tests of the code they
guard.

**Everything is built from the design and from `plan9/`.** Those are the only
two inputs. Where the design is silent, that is a gap to be raised, not a space
to fill.

Four rules govern every phase:

1. **The kernel is a SUBSET of Plan 9's, containing process orchestration.**
   Every call is one of Plan 9's, every device letter is one of Plan 9's, every
   flag has Plan 9's value. Where this kernel differs it does so by lacking
   something.
2. **Everything else is communication between userspace processes.** A console,
   a clock, a store, a window system are file servers. None of them is the
   kernel's business.
3. **The kernel does not grow.** It may shrink; that is the only direction.
4. **No inventions.** Not a device letter, not a call, not a word. Before
   coining anything, search `plan9/`.

---

## The first milestone — the demo

**A minimum viable proposition.** It proves the design can replace the demo
live at [christham.net/ipnx-v12](https://christham.net/ipnx-v12/). It is not a
final state; design resumes after it. **Don't overengineer.**

| | | proves |
|---|---|---|
| **the CLI** | typing `ipnx` in a terminal boots IPNX to `rc`; you run userspace commands | the kernel is a Plan 9 subset, boot is rc plus a namespace file, every personality is userspace, the machine is file servers |
| **the website** | emca in the browser doing what the site does now — a listing on the left, `motd`/`tour`/`README` as tabs, `rc` below — with the windows, toolbar and status line to spec | emca owns windows entirely, the contract and the types hold, the surface renders files and never pixels |

**Not in the demo:** a window on a Mac or an iPad, the raster, `/net`, git.
**P0–P8 deliver it.**

---

## P0 — the kernel's core *(done; rewritten 2026-09-17 against the record)*

**What P0 is for, in Christine's words** — the quotes are in
[verbatim.md](verbatim.md), which is the only documentation that is hers:

> *"we are essentially implementing a micro kernel based on a subset of Plan 9,
> we should not be adding to it (even the Unix v10 personality should be
> userspace) … It is important to keep our kernel pure otherwise we will
> encounter serious issues extending the kernel"*

> *"The kernel only handles process orchestration. everything else is handled
> by host or userspace. Everytime you design a change to the kernel, the design
> is wrong."*

| | |
|---|---|
| **builds** | **`Chan`** — a walk produces one, an fd holds one, a mount point is one, and every device operation takes one; the **device table**, Plan 9's `struct Dev`; the **namespace**, keyed as Plan 9 keys it; the **process table** with `rfork`'s share/copy/clear, its flag checks, `exits` and `await`; the **9P codec**, the only place the wire exists |
| **the letters** | `#/` `#\|` `#s` `#M` `#p` `#d` `#e` `#c` `#¤` — nine. `#i` and `#m` are absent because Plan 9 has them to drive hardware and this kernel drives none |
| **depends on** | — |
| **acceptance** | 14 unit tests of the structures, where they live. **P0 does not pass conformance and cannot**: not one of the twelve behaviours is reachable without `exec`, a device and a userspace. The suite FAILS, and that is correct |
| **exposes** | no device is implemented, and `exec` needs something to instantiate a process |

**Where the machine goes instead**, and this is hers too:

> *"Only saranos knows about the host… I am a macOS app. I have a screen, a
> keyboard and a mouse. I will serve these as virtual devices to the IPNX
> kernel, which I am going to start."*

> *"`/dev/draw` should be rendered by host. the kernel does not know how to
> draw."*

So the host **serves** the machine and the kernel **consumes** it, over 9P
because *"9P is the only protocol"*. `/dev/cons` exists on the IPNX side
without the kernel holding a console driver.

The namespace
keys a mount by the identity of the channel mounted upon (`chan.c:855`,
`findmount`) and checks at every component — not by path text.

## P1 — `exec` *(done)*

`sysexec` starts at `sysproc.c:302` with `tc = namec(file, Aopen, OEXEC, 0)`,
reads the image, and then does the machine's half. All three now happen.

| | |
|---|---|
| **builds** | **`namec`** (`chan.c:1317`): the starting point is a CHANNEL — the process's `slash`, its `dot`, or a device attach — then the elements are walked, stepping through a mount point at **every** component. **`devroot`** (`#/`): the small read-only directory the kernel carries, so the first process has somewhere to be read from. **`exec`**: resolve, read, `procsetup`, `touser` |
| **the machine boundary** | Plan 9 splits `port/` from the architecture directories, and a machine supplies what that directory's `fns.h` declares — `void touser(void*)` at `pc/fns.h:173`. The kernel's `Machine` trait is that boundary with those names, and **names no machine**: not WebAssembly, not a module, not an engine. The kernel has no dependencies |
| **the one adaptation** | `touser` in Plan 9 jumps to a stack pointer, the image having been mapped already. A machine whose executable unit is a module has no such step — the image *is* the executable state — so the image is what crosses. Approved 2026-09-17: *"wasm instantiation is fine, keep it machine independent"* |
| **also fixed** | a process holds `slash` and `dot` as **channels**, as Plan 9 does. They were a `String` cwd — the same error as keying the namespace by path text |
| **depends on** | P0 |
| **acceptance** | `cargo run -p ipnx` → `a process ran, and said so`. A process was resolved through a namespace, read out of a device, instantiated and run. **40 kernel tests**, including the four that prove the claims rather than exercise them: a walk lands on the MOUNTED file and not the one under it; a mount made on a walked-to component is honoured there, which is what "checked at every component" means; a relative name resolves from `dot`; and the machine is given the bytes that were resolved, set up before it is entered, and not touched at all when the name does not resolve |
| **exposes** | one process is not two: nothing can talk to anything. That is P2 |

**It moves nothing on the conformance checklist, and that is right** — every one
of the twelve needs a shell, and there is no userspace. Still 0 of 12.

## P2 — the devices orchestration needs *(done)*

| | |
|---|---|
| **builds** | `#|` pipe, `#s` srv, `#d` dup, `#p` proc, `#e` env, `#/` root, and `#M` — the mount driver, and the only place wire 9P is marshalled. All Plan 9's, none added |
| **depends on** | P1 |
| **acceptance** | **all three pass.** Two processes talk over a pipe; a channel posted at `/srv` is opened by name and mounted; a file resolves through that mount by an ordinary `open` |
| **exposes** | there is nothing to run yet — no libc, no commands. And `#M` forced a correction: the device table held state, where Plan 9's `devtab[]` holds only vtables (`portdat.h`, `struct Dev`) and each device's state is a file-scope global. See [when.md](when.md) |

**`#c` and `#¤` are both in** — Christine, 2026-09-18: *"we should keep `#c`
in since it holds a variety of kernel info"*.

`#c` was excluded twice and restored twice, and the measurement was the same
each time; only the question changed — and two of the three questions were the
wrong question. *Is it hardware?* No. *Is it orchestration?* Mostly no, **and
that is not the test**. Christine, 2026-09-18: *"that rule was to stop you from
adding all sorts of invented stuff in the kernel… it doesn't mean that is the
only thing the kernel does. Use Plan 9 as a guide."* The test is **does Plan 9
have it**, and Plan 9 has all 23.

| | |
|---|---|
| console | `cons` `consctl` — served by the host (P4) |
| identity | `user` `hostowner` `hostdomain` — `/dev/user` is the only drop to `none` (`auth.c:107`) |
| this process | `pid` `ppid` `pgrpid` `cputime` |
| the clock | `time` `bintime` |
| the kernel itself | `sysname` `osversion` `kmesg` `kprint` |
| generators | `null` `zero` `random` |
| **hardware this kernel has none of** | `swap` `drivers` `config` `sysstat` `reboot` — absent for the reason `#i` and `#m` are |

**What each device needs of the current process**, measured rather than
assumed — `up->genbuf` is a scratch buffer for formatting names, not state, and
it is most of the raw count (13 of `devproc`'s 19, 5 of `devdup`'s 6):

| | |
|---|---|
| identity, for a permission check | `#s` `#M` `#p` `#¤` `#c` |
| the tables `rfork` shares, copies or clears | `#d` is `up->fgrp`, `#e` is `up->egrp`, `#p` is the process table |

The second kind is why those are not growth: the kernel holds all three
already, because `rfork` acts on them. They are that state shown as files.

## P3 — the userspace

| | |
|---|---|
| **builds** | a libc over the call list, and `rc` — the shell, because boot is rc. Then the commands the demo needs |
| **depends on** | P2 |
| **acceptance** | `rc` runs a script; a pipeline of two commands works |
| **exposes** | `rc` needs a console, and the console is not in the kernel |

## P4 — the machine, as file servers

| | |
|---|---|
| **builds** | the console and storage as **userspace file servers**, reached by mounting what the embedding serves. `cons` and `consctl` are 2 of `#c`'s 23 and the only ones needing a host behind them |
| **depends on** | P3 |
| **acceptance** | `rc` reads and writes `/dev/cons`; a file written through the storage server survives a boot |
| **exposes** | nothing assembles these at boot yet |

## P5 — boot *(the CLI)*

| | |
|---|---|
| **builds** | `/namespace`, the instance's own configuration, read by the embedding because it owns the storage; `/rc/bin/termrc`, the rc half, which starts the servers — Plan 9's own name at Plan 9's own location; the root itself a file server |
| **depends on** | P4 |
| **acceptance** | **the CLI.** Typing `ipnx` boots to `rc` on the terminal; `ls`, `cat /etc/motd` and the demo's commands run |
| **exposes** | the scheduler, then packages — `/pkg`, `/store`, `/profile` — and emca |

## P6 — the scheduler *(added 2026-09-21; built 2026-09-23)*

**Christine put it before the registries**: *"we will need to implement
before P6/P7."* The measurements are RESEARCH §14, and what Plan 9 does —
twice asked, twice a lookup — is §14.1.

**It is not a deviation.** `sched()` divides at `setlabel(&up->sched)` /
`gotolabel(&m->sched)`, declared in `port/portfns.h` and implemented in
`pc/l.s:1000`, `:992`, with `Label` a machine-dependent type
(`pc/dat.h:51`). The scheduler is Plan 9's PORTABLE code; the stack switch
is the architecture's, exactly as `touser` is. So this phase is
`port/proc.c` arriving and a machine supplying what `pc/l.s` supplies there.

| | |
|---|---|
| **done** | **the kernel above the switch** (2026-09-21): the twelve states, `Rendez` addressed as `(pid, which)` because Plan 9's are fields (`portdat.h:683`, `:720`), `runq[Nrq]`, `queueproc`/`dequeueproc`, `updatecpu`, `reprioritize`, `ready`, `runproc` (cut to one processor — Plan 9's is affinity and load balancing across `MACHP(i)`), `sleep`, `wakeup`, `tsleep`, `timerintr`. `syssleep` goes through it; `pexit` wakes the parent's `waitr`; `/proc/<n>/status` reports the real state. **161 kernel tests** |
| **done** | **the machine's stack switch** (2026-09-22): `Machine::gotolabel`, a `wasmtime-fiber` per process, `schedinit` as the loop, `exec` unwinding the old image's frames, `await` sleeping in `pwait` and `rfork` ending `ready(p); sched()`. **Two processes alternate**: a pipeline is two of them with the shell asleep between |
| **done** | **the clock, and preemption with it** (2026-09-22): `Mach`, `timersinit`, `timerintr` → `hzclock` → `accounttime`, `hzsched`, `rebalance`; the machine's `clockintr` on wasmtime's epoch; `sched()` at the interrupt's tail. **A guest in a tight loop does not stop the system.** Building it exposed two older faults, fixed with it: **pipes never blocked** and **exit never closed descriptors** (`closefgrp`). Then every difference that was not forced was removed (RESEARCH §15.2): **a call that sleeps carries on where it stopped** (`p->sched`, `Syscalls::resume`), `trap.c:778`'s `sched` at every call's end, `TSys`, `fixedpri`, ticks in `time[]`, and pipes as Plan 9's queues with `rlock`/`wlock` and flow control. **185 kernel tests** |
| **done** | **notes** (2026-09-23): `postnote`, `notify`/`noted` — the note written onto the guest's stack and the handler entered through `__notestart` — `Eintr`, `alarm` through an `alarm` kproc, `closeproc` as a kproc, `RFNOTEG`, `#p/<n>/note`, `notepg`, `noteid`, and `kill` as Plan 9's note. rc's `Trapinit` is `plan9.c`'s. RESEARCH §15.3. **191 kernel tests** |
| **done** | **`rendezvous` and the process controls** (2026-09-23): `Rgrp` and `RFREND`, `start`/`stop`/`waitstop`/`hang`/`nohang` with `Stopped` and `procstopwait`, `Broken` and `NBROKEN`, and `psstate`. RESEARCH §15.4. **196 kernel tests** |
| **done** | **the semaphores and tracing** (2026-09-23): `semacquire`, `tsemacquire`, `semrelease` as Plan 9's calls, the word reached through the machine's `load` and `cmpswap` (RESEARCH §15.7); `startstop`, `startsyscall`, `/proc/n/syscall` with `syscallfmt`/`sysretfmt`, and `profile` over a text segment shared by `attachimage` (§15.8); a `procrfork` child as a process of its own from the start, so it may sleep before it `exec`s, and `exec` able to sleep reading its image (§15.9); the host terminal no longer echoing `^C` (§15.10) |
| **the machine boundary** | three hosts, one shape. **wasmtime**: `Config::async_support` is a host-stack fiber and `Func::call_async` runs the guest on it; `epoch_interruption` + `Engine::increment_epoch()` + `Store::epoch_deadline_async_yield_and_update` preempt by yielding rather than trapping. **Node**: a wasm supervisor. **Browser**: a worker per process. A worker is a thread, so the switch there is a message and `Atomics.wait` — which is why the method answers to `setlabel`/`gotolabel` and to nothing else |
| **the suspend shape** | **per-process stacks** (§14.1). `Proc.kstack` is 4096 bytes (`pc/mem.h:26`), a syscall runs on it, and `sleep` leaves its C locals there: `setlabel` returns 1 on the way back and the syscall continues from the line it stopped on. The fiber IS that stack, so a host function that awaits keeps its Rust locals exactly as Plan 9 keeps its C ones. There is no blocked-and-resumed call and no re-entrancy |
| **the one rule** | **no lock held across a sleep** — `sleep` prints a diagnostic when there is (`proc.c:821`), `sched` refuses to switch and sets `up->delaysched` (`:213`), and `unlock` sched's the moment the last one goes (`taslock.c:216`). Here that reads: **no `RefCell` borrow held across a suspension point** |
| **where preemption lands** | `hzclock` calls `hzsched`, which does not `sched()` — it marks `up->delaysched` (`portclock.c:136`, `proc.c:203`). The switch happens at `trap()`'s tail, `syscall()`'s tail, or `unlock` (`pc/trap.c:438`, `:778`, `taslock.c:216`). **Here, in user mode only** — and that is this machine's limit, not Plan 9's rule: `trap.c:438` has no `user` test, so Plan 9 preempts its own kernel at a clock interrupt when no ilock is held (an earlier version of this row said *"user mode only"* as if it were Plan 9's). An epoch check is compiled into GUEST code and never into a host function, so a call always runs to its end first |
| **depends on** | P5 |
| **acceptance** | `sleep(2)` is a `tsleep` on a `Rendez` and a second process runs during it ✓; a guest in a tight loop does not stop the system ✓; **`^C` interrupts a command**, so rc's `Trapinit` stops being a stub ✓ — the host receives the key and the console posts *"interrupt"* to its note group, `rio`'s mechanism (Christine, 2026-09-23; RESEARCH §15.5) |
| **does NOT build** | `rfork(RFPROC)` returning twice. A fork duplicates an address space AND a stack; a fiber's stack holds host frames pointing into the instance, and a worker's is another thread's. `procrfork` stays (RESEARCH §5.2) |
| **exposes** | `alarm`, `notify`, `noted`, `rendezvous`, `RFNOTEG`, `#p/<n>/note`, `#p/<n>/ctl`'s `start`/`stop`/`waitstop`/`hang`, and `/proc/<n>/status`'s real states — every one of them a call or a file Plan 9 has and this kernel refuses today |

## P7 — the registries

| | |
|---|---|
| **builds** | **Three different things, not one format** (Christine, 2026-09-24: *"either these are all the same or they are different thing. In my original concept they are completely different"*). What each is, in her words (2026-09-24, `verbatim.md`): **`/pkg`** — *"like a FreeBSD pkg or apt… a list files to be bound in the namespace, plus potentially initialisation scripts (write out config files, set out environment etc.)"*; **`/template`** — *"a prototype for a project (ie. a NodeJS project, a Python project) - it may install packages, but contains project scaffolding"*, and *"the key difference… is that a templates instantiates new versions of files (scaffolding), not just binds of files shared across namespaces"*; **`/profile`** — *"any files required to configure a system - the kind of stuff in Unix /etc. network config, namespace bindings, init scripts etc."*, with the user's in `/home/profile`, `/home` being `/usr/<username>`; `/rc`'s configuration moves into it and **`/rc` is retired**. **`/pkg`, `/service`, `/template`** — *"a list of packages installed… a list of services installed… a list of templates"*. A package installs **to the system, the user, or the namespace** — *"available to every user, process"*, *"whenever the user logs in"*, or *"only valid for current process"* — and a template installs its packages into the project, which *"ensures all packages are available"* when it is opened. A package is *"like installing a toolchain or a library"*; **services** install daemons, a separate thing with their own spec (*"services and packages should be different"*): started with the system, at login or when a project is opened, and ended with it. **A project** is a window type, opened from a project file, at `/project/<x>` bound per user. The designs are proposals: [proposals.md](proposals.md). **`/store`** — *"a store entry never changes after verification"*, and *"the store must be prunable."* **None of the four is designed beyond that**; each needs a proposal reviewed before anything is built. An earlier version of this row said *"one format, three registries"*, drawn from her remark that a package *"is actually very similar to template"*, which was read as "the same" |
| **depends on** | P6 |
| **acceptance** | a package installs as a bind, `pkg remove` unbinds, and the store entry survives it |
| **exposes** | — |

## P8 — emca, and the browser *(the website)*

| | |
|---|---|
| **builds** | emca in userspace: it mints windows and serves the contract of [window.md](window.md) as files. Then the browser embedding, and the surface that **reads emca's files** and renders natively |
| **depends on** | P6 — the browser host is a worker per process, which is P6's machine boundary on that surface |
| **acceptance** | **the website.** The site shows the listing, the three tabs and `rc`, to spec |
| **exposes** | the other targets |

---

## After the demo — the remaining targets

| target | brings |
|---|---|
| **macOS app** | the `ipnx` embedding with a SwiftUI surface reading files |
| **iOS app** | an embedding and a surface |
| **container** | `FROM scratch`, headless or with the surface served |
| **MicroVM** | the kernel on a hypervisor: no embedding, so the machine is virtual hardware |
| **real hardware** | the kernel on metal: drivers serve what file servers served |

Each is a gap until reached. The kernel does not change for any of them — that
is the claim they exist to test.
