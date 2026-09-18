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
**P0–P7 deliver it.**

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

## P2 — the devices orchestration needs

| | |
|---|---|
| **builds** | `#|` pipe, `#s` srv, `#d` dup, `#p` proc, `#e` env, `#/` root, and `#M` — the mount driver, and the only place wire 9P is marshalled. All Plan 9's, none added |
| **depends on** | P1 |
| **acceptance** | two processes talk over a pipe; one posts a channel at `/srv` and the other mounts it; a namespace built by `bind` and `mount` resolves |
| **exposes** | there is nothing to run yet — no libc, no commands |

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
| **exposes** | packages — `/pkg`, `/store`, `/profile` — and emca |

## P6 — the registries

| | |
|---|---|
| **builds** | `/profile`, `/pkg` and `/template` as one format, three registries: a list of bindings plus commands. `/store` for fetched bytes, served by a userspace file server so verification is IPNX's |
| **depends on** | P5 |
| **acceptance** | a package installs as a bind, `pkg remove` unbinds, and the store entry survives it |
| **exposes** | — |

## P7 — emca, and the browser *(the website)*

| | |
|---|---|
| **builds** | emca in userspace: it mints windows and serves the contract of [window.md](window.md) as files. Then the browser embedding, and the surface that **reads emca's files** and renders natively |
| **depends on** | P5 |
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
