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
reached functional equivalence with the demo?** It is a checklist of what a
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

## P0 — the kernel's core *(done, rebuilt 2026-09-17)*

| | |
|---|---|
| **builds** | **`Chan`** — the object everything acts on, since a walk produces one, an fd holds one and a mount point is one; the **device table**, which is Plan 9's `struct Dev` (`attach walk stat open create close read write remove wstat`); the **namespace** over channels — longest-prefix walk, unions, the create element; the **process table** with `rfork`'s share/copy/clear; the **9P codec**, the only place the wire exists |
| **the device letters** | `#/` root, `#\|` pipe, `#s` srv, `#M` mnt, `#p` proc, `#d` dup, `#e` env — seven, each because orchestrating processes needs it. **`#c` cons is absent**: Plan 9 has it because its kernel drives a uart and a screen; this one drives nothing, so a console is a file server |
| **depends on** | — |
| **acceptance** | the codec, the namespace, the device letters and `rfork`'s rule are tested where they live. Nothing on the demo's checklist is reached, and P0 was never going to reach any of it |
| **exposes** | no device is implemented yet, and `exec` needs an engine |

## P1 — `exec`

| | |
|---|---|
| **builds** | `exec` resolves a path through the calling process's namespace, reads the image, and the engine instantiates it. The engine belongs to the embedding: it is the one thing the kernel cannot do for itself, and it is supplied as plumbing rather than named in the call list |
| **depends on** | P0 |
| **acceptance** | a process runs, exits, and `await` reports its status |
| **exposes** | how a process reaches anything at all — the devices |

## P2 — the devices orchestration needs

| | |
|---|---|
| **builds** | `#|` pipe, `#s` srv, `#d` dup, `#p` proc, `#e` env, `#/` root, and `#M` — the mount driver, and the only place wire 9P is marshalled. All Plan 9's, none added |
| **depends on** | P1 |
| **acceptance** | two processes talk over a pipe; one posts a channel at `/srv` and the other mounts it; a namespace built by `bind` and `mount` resolves |
| **exposes** | there is nothing to run yet — no libc, no commands |

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
| **builds** | the console, storage and the clock as **userspace file servers**, reached by mounting what the embedding serves. Plan 9 puts these in `#c` because it drives hardware; this system has none, so they are processes |
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
