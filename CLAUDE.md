# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repository is

**ipnx-v12 is a reimagining of Unix** — the counterfactual next Research edition:
**the IPNX kernel — Plan 9's architecture, none of its code — hosted as an ordinary
userspace process** (browser, macOS,
iPadOS, OCI, eventually hypervisor-direct); **9P as the only IPC**; **per-process
namespaces**; **WebAssembly as the executable format**; and **personalities as libc
dialects** over that one kernel — Plan 9's own userland (taken entire, by the curation
principle), a WASI ABI (Go `wasip1`, CPython's wasi builds), and a **modern Unix
personality** whose surface is derived by measurement against three benchmarks — git,
CPython, Go — never adopted from POSIX. Three refusals: no POSIX-the-standard, no
systemd (boot is rc plus a namespace file), no Linux/BSD sediment. Three adoptions:
sockets won (the BSD API over `/net` files), UTF-8 won (its authors invented it),
modern software must run. The V10 exhibit (`/v10/bin` cat and echo, TUHS tapes) stays
as heritage; V10 completeness is not a goal (re-founded 2026-08-27, decision log).

The thesis, from the README: *the Plan 9 authors' pivot gave Unix its best kernel and
broke compatibility in the same act — "Compatibility was not a requirement for the
system" — a choice, which is what makes it undoable.* This project takes that kernel's architecture — reimplemented, no code inherited — and
undoes the break: their curation of the userland preserved above, Unix's interface
restored as personalities, the kernel unable to bloat by construction.

**And as of 2026-08-31 the claim is larger, and deliberately so** (decision log):
ipnx-v12 is not a barebones Unix reimagined but **a whole operating system with its
own semantics, user interface and artifacts**. The face of it is **`emca`** — the
IPNX user interface, what the system *boots into* on every surface: the browser
page, the macOS app, the iPadOS app. Not an editor; an editor is one window type
inside it. Its mechanism is the founding principle taken to its limit — **everything
is managed as a file, so there are no manager programs**: no `ps`, no package-manager
GUI, no network panel. There is a filesystem, a **window type** declaring which verbs
its files accept, and a surface rendering those verbs natively. Adding a manager to
the system is adding a file. A rich system UI normally destroys acme's central
property (any text can be a verb's operand) because a process table becomes a widget;
here it cannot, **because the managers are already text filesystems**. The design is
[docs/emca.md](docs/emca.md); the acme anatomy in [docs/acme.md](docs/acme.md)
was input to it, not parentage.

**The layers have names** (decision log, 2026-08-31; sharpened twice on
2026-09-01): **Saranos is the operating system** — the whole thing, what you
would say you are running. **IPNX is the kernel and the userspace** — the wasm
side, Darwin's slot rather than XNU's. **emca is the windowing and UI system** —
the compositor, the window types, the surfaces' furniture, and it spans both
sides by construction. The parallel holds: **macOS / Darwin / Aqua** is
**Saranos / IPNX / emca**.

**AND SARANOS ENCOMPASSES THE HOST SIDE AS WELL AS THE WASM SIDE** — Christine,
spelling out what an earlier draft of this paragraph got wrong: *"Saranos as an
OPERATING SYSTEM encompasses host side and WASM side as well… that's why it's
different from IPNX, which only describes the kernel and userspace, and that's
why Saranos is a different name. It is a symbiosis between host and WASM,
neither can exist without the other."* So the host — the Rust host on macOS,
the browser runtime, the surface — is **inside** Saranos, not underneath it as
substrate. The kernel is wasm and cannot run without a host to give it workers,
memory and a screen; the host has nothing to do without the kernel. The earlier
wording, "wasm and the surfaces are the machine it runs on", had the host below
the system rather than part of it, and that is the distinction the name exists
to carry. **The interface between them is 9P and nothing else**
(redesigned 2026-08-31): content is a file the host mounts and renders natively
(so **IPNX implements no renderers**), `/dev/window/<type>/<n>` is the
bidirectional control interface with the type in the path, `/type` is the
registry both sides read, and `/dev/canvas` narrows to genuine drawing — the
exception, not the rule.
Saranos is Sanskrit *śaraṇa* (शरण), *refuge* — Christine's reading: *a refuge
from the complexities of the modern computing environment*, a refuge for the
person, which is why it names the system someone uses and not the kernel
underneath. A process
also runs in a refuge bounded by what it was given; one word, both layers. Note the symmetry that produced the layering: **XNU is
"X is Not Unix" and IPNX is "IP is Not UNIX"** — the same joke, so the layer
above wanted a human name rather than a second acronym, exactly as Darwin did.
**Dated entries across the records keep the words they were written with** — a
log is not retroactively renamed; only present-tense statements of what the
system *is* carry the new layering.

The architecture runs — the Rust kernel core under a host, booting to `rc`;
what is built is [docs/when.md](docs/when.md). Everything else is design documents.

## Commands

Build the guest binaries — the libc, the commands and `rc` — into
`userspace/root/bin` (requires wasi-sdk at `~/.local/opt/wasi-sdk`, overridable
with `WASI_SDK`, for its wasm backend alone: nothing here links against wasi,
and a built binary imports exactly the calls in `userspace/libc/wasm/sys.c`;
plus `bison` for rc's grammar and Python 3 for `weaken.py`). Apple's clang has
no wasm backend, which is why the SDK is a prerequisite:

```bash
bash userspace/mk.sh
```

Four flags are load-bearing, each measured (RESEARCH §9.4, §11, §16.9):
`-fno-builtin`, because clang's libcall recogniser rewrites strlen's own body
into a self-call; `-fms-extensions`, because `port/pool.c` is written in
kencc's anonymous struct members; and `weaken.py`, because **the wasm backend
has no common symbols at all** — `-fcommon` is an error — so rc.h's tentative
definitions each become a strong definition and the link fails with a duplicate
for every variable rc declares. It sets the weak bit in the `linking` section,
keeping strong the one definition that carries an initialiser; `wasm-ld
--allow-multiple-definition` cannot do the job, because it keeps the FIRST
definition even over a later initialised one and two different files here
initialise something. And `-mllvm -wasm-enable-sjlj` (with
`-mexception-handling` and `-wasm-use-legacy-eh=false`), because this machine
has no stack a program can save: `setjmp`/`longjmp` are wasm exceptions, and
`libc/wasm/setjmp.c` is the library's half — so the host enables wasmtime's
exceptions (`gc`, `gc-null`).

`userspace/build/` and `userspace/root/` are generated and gitignored. Guest
binaries carry no `.wasm` extension: exec walks the namespace for `/bin/cat`,
and a freshly produced module is indistinguishable from a shipped one.

Then the system itself — the Rust kernel core (`kernel/`, RESEARCH §9.6) under
`hosts/ipnx`, which supplies the machine (wasmtime) and builds the device
table. It runs the userspace built above:

```bash
cargo test                                  # the kernel, the host, conformance
cargo run -p ipnx                           # boot to rc on this terminal
cargo run -p ipnx -- echo hello             # boot; init runs it with rc -c
cargo run -p ipnx -- rc /bin/<script>.rc
```

A command boots the whole system: it is given to `init` as Plan 9's
`plan9.ini` line `init=/wasm/init -t 'cmd'` would give it, and `init` runs
it with `rc -c` in the namespace it built, then starts the interactive
shell, which ends at once when there is no input.

**Build the userspace first**: the host's tests run the real binaries, and
`cargo` cannot build a wasm userspace.

Node ≥ 22 where it is used (`worker_threads`, SAB, wasm `try_table` exception
handling — the legacy EH encoding is *rejected* by these engines, so any new
wasm emission must use `try_table`).

## The documents

**Every document answers ONE of six questions, and one only** (Christine's rule,
2026-09-02). Several documents may answer the same question from different
perspectives; none may answer two. A *when* fact written into a *why* document
goes stale there — that is how four different test counts came to exist.

### why — why are we doing this?

| | |
|---|---|
| `README.md` | the project's front page. **Claude-written like everything else** — Christine edited a few paragraphs and has not reviewed the rest; it is not exempt from correction |
| `demo/index.md` | the landing page. Claude-written on the same footing |
| `docs/why.md` | the codified purpose, and the intent of each component |

### who — who are we doing this for?

| | |
|---|---|
| `docs/personas.md` | the five personas: jobs, needs, wants, pains, and the belief test that converts each |

### what — design, architecture, specifications

| | |
|---|---|
| `docs/architecture.md` | present-tense invariants and contracts; changes only with a contract, in the same commit |
| `docs/saranos.md` | the system's identity — the three layers and what each name covers |
| `docs/emca.md` | **emca the ROLE** — what a window is, and which half owns which part. A role, not an implementation |
| `docs/window.md` | **the window manager CONTRACT** — emca gives a manager a rectangle and a namespace, and stops. Every implementation honours this and nothing more |
| `docs/compositor.md` | **the tiled implementation** — allocation, alternation, tabs, sizing. Replaceable without touching managers or types |
| `docs/surface.md` | **the host half** — chrome, theming, placement, and the devices Saranos serves |
| `docs/type.md` | the type system: a type is a folder of text files; a MANAGER renders and edits it |
| `docs/canvas.md` | `/dev/canvas`, narrowed 2026-08-31 to genuine drawing |
| `docs/acme.md` | the acme port — Bell Labs' program fitted into emca, functionality preserved |
| `docs/userland.md` | the userland's shape; the heritage exhibit's scope |
| `docs/syscalls.md` | the derived call list — Plan 9's 40 live calls dispositioned |
| `docs/identity.md` | the identity model — what a user *is* inside the system (not who it is *for*) |
| `docs/packages.md` | P7's design — a package, a service, a template, a profile, and where each lives |
| `docs/projects.md` | a project — the working folder a template, package, service or profile is made from; a window type |

### where — surfaces and targets

| | |
|---|---|
| `docs/platforms.md` | deployment forms with engine and state, the canonical boot namespace, the dated deployment ledger |

### when — what is built, and what is not

| | |
|---|---|
| `docs/when.md` | **the single authoritative statement of build status.** No other document carries it |

### how — the plan, and the practice

| | |
|---|---|
| `docs/implementation.md` | **the plan — a redesign and rebuild (2026-09-17)**: phases P0–P8 to the demo (the CLI, then the website), built from the design and from `plan9/`. Superseded plans are in `docs/archive/` |
| `docs/handbook.md` | the practice: prerequisites, build/run, load-bearing flags, how to add a command/test/device/host |

### meta — documents that inform and guide the six

| | |
|---|---|
| `RESEARCH.md` | the evidence base: every finding with provenance. What *why* and *what* are built on |
| `docs/design-thinking.md` | **a lens** — *who is this for, what do they need* |
| `docs/six-hats.md` | **a lens** — *what are we not seeing* |
| `docs/virtue-ethics.md` | **a lens** — *what character does the work express* |
| `docs/reviews/` | **the readings** — one dated log per application of the lenses to the whole project |
| `docs/proposals.md` | **a register, not a question** — proposed answers awaiting review, kept out of the specs |
| `docs/archive/` | **superseded specs — NOTHING HERE IS CURRENT.** A replaced spec goes here, never folded into a live document |

**A LENS NEVER CHANGES BECAUSE THE PROJECT CHANGED** (2026-09-02). The
instrument is stable; each application produces a dated *reading* in
`docs/reviews/`. Never add a session record to a lens document. The same rule
one level down governs `docs/personas.md`: a persona is **extrinsic** and
cannot be made stale by a design change — if deleting this project from the
universe would change a card, that line is in the wrong document.

Findings go in RESEARCH.md with provenance; decisions go in the decision record (docs/archive/design-log-claude-written.md);
contract changes go in docs/architecture.md in the same commit as the code; deployment-story
reviews land dated in docs/platforms.md's ledger; build status goes in docs/when.md
and NOWHERE else; all of these are living.
Keep them consistent — the architecture statement appears in RESEARCH's TL;DR and the
plan, deliberately, and a change to it changes both. **README.md is Claude's, like every other document here**, and is corrected like any other. Prefer a
pointer over a copy for anything else ("a list that appears twice will disagree").

## The Plan 9 reference tree

**Plan 9's source is kept locally and gitignored** — never built, never edited,
never committed. It exists for the same reason `../ipnx` holds the V10 tree:
**a claim about Plan 9 must trace to file and line**, and it cannot if the
source is not here to cite. Added 2026-09-03, after a claim about Plan 9's
ramfs turned out to be right by luck rather than by reading (RESEARCH §9.12).

| | |
|---|---|
| **`plan9/`** | **the 9legacy tree** — 4th edition with the maintained patches applied. **Check claims against this one** |
| `plan9-stock/` | the raw final Labs release, so a difference can be *attributed*: **778 files under `sys/src` differ** between them |

```bash
git clone --depth 1 https://github.com/0intro/9legacy plan9
git clone --depth 1 https://github.com/plan9foundation/plan9 plan9-stock
```

**9legacy changes nothing the device audit rests on** (verified 2026-09-03):
every device letter — `M` mnt, `w` watchdog, `/` root, `i` draw, `s` srv, `p`
proc, `c` cons, `e` env, `d` dup, `|` pipe — is unchanged, and `devroot.c`,
`devwd.c` and `devdraw.c` are byte-identical to stock. What it does change
nearby is tuning: `devmnt.c`'s `MAXRPC` grows from `IOHDRSZ+8192` to
`IOHDRSZ+16*1024`, with a `MAXCMNRPC` kept at the old size for initial
negotiation.

Note the distinction from the **vendored** subset under `userspace/` — the
parts of `libc/` and `cmd/` actually compiled into the system, copied verbatim
from `plan9/` and committed. `plan9/` is read; `userspace/` is built.

## The parent repository

[ipnx](https://github.com/ChristineTham/ipnx) restores Research Unix under full-system VAX
emulation. It is checked out beside this one at `../ipnx`, but referenced from the docs
only by URL — deliberately.

**The V10 measurement tree lives in `../ipnx`, not here.** Every V10 number quoted
(61,072 kernel lines, 239 `fork(` sites, `sh/xec.c:432`, `sysent.c` slot 66) was measured
in `../ipnx` and is recorded **as data** because it cannot be re-derived here. A new
measurement is taken in `../ipnx` against `v10/usr/src/…` and recorded with file-and-line
provenance in RESEARCH.md.

## What is authoritative — and what is not

**Every document in this repository was written by Claude.** Measured
2026-09-17: `docs/design.md` 31 commits, `implementation.md` 44, `RESEARCH.md`
21, `architecture.md` 17, this file 20, `README.md` 3 — **all of them Claude's,
none of them Christine's.** She has not seen most of it.

That matters because of what the decision log was doing. It recorded
"decisions", quoted her in them, and was then cited — by later sessions, and by
this file — as settled fact not to be re-derived. A session would invent
something, write it down as a decision, and every session after would defend it
as hers. Three device letters that exist in no Plan 9 kernel — `#H`, `#V`,
`#Z`, named here **only so they are recognised if they reappear** — plus an
effect list at the embedding boundary and a namespace keyed by path text: all
of them became "decided" that way. The log is
archived at `docs/archive/design-log-claude-written.md` and **is not
authority**.

**What IS authority, in this order:**

1. **What Christine says.** In conversation, now. Not a transcription of it in
   a document written by a previous session.
2. **`plan9/`, at file and line.** Every claim about Plan 9 traces there or it
   is not a claim.
3. **The code and its tests**, for what the system actually does.

**What the documents are for:** notes, so a session that inherits nothing has
somewhere to start. They are a record of what was *thought*, not of what was
*agreed*. A statement in one of them is a lead to check, never a decision to
cite — and **it cannot make a deviation from Plan 9 approved**. Only she can do
that, and her default answer is no.

## The tree

The implementation lives at the top level: `kernel/` (the Rust core, no
dependencies) and `hosts/ipnx/` (the machine — wasmtime — and the boot; the
workspace root is `Cargo.toml`). The other hosts named in
`docs/implementation.md` are not built yet.

The guest world lives at `userspace/`: `include/` (the wasm32 architecture
header, and the vendored Plan 9 headers), `libc/wasm/` (the machine-dependent
half — this architecture's `9syscall`), `libc/{port,fmt,9sys}/` and `rc/`
(vendored verbatim, except rc's own platform file `ipnx.c`), `cmd/`, and
`mk.sh` with `weaken.py`. `userspace/build/` and `userspace/root/` are
generated. Work is sequenced by `docs/implementation.md`; what is built is
`docs/when.md`. The target is **functional equivalence to the demo**.

## Conventions

- **THE KERNEL IS A SUBSET OF PLAN 9'S; DEVIATIONS ARE AUTHORISED ONLY FOR THE
  SUBSTRATE, AND EVEN THEN MUST BE SUBSTRATE-INDEPENDENT** (Christine's rule,
  2026-09-03). *"we are essentially implementing a micro kernel based on a
  subset of Plan 9, we should not be adding to it (even the Unix v10
  personality should be userspace)"*; *"Actual deviations … are only authorised
  when it is to do with adapting it for WASM and WASI"*; *"even then it should
  be done in a machine independent way as we may want a non WASM kernel in the
  future … for example, dis, or .NET CLR"*.

  **Two tests, in order.** Is it forced by running on **a VM at all** — not by
  wasm in particular? If not, unauthorised. If so, could **Dis or the CLR**
  satisfy it without the kernel changing? If not, it is right in purpose and
  wrong in shape. The reason is concrete: **MicroVM on a hypervisor, then real
  hardware (Raspberry Pi)** — where there is no host, so every addition must be
  carried onto the metal or removed there, and removing it there is harder.

  A personality — including V10's — is **userspace**, always.

- **THE KERNEL DOES NOT GROW BY INVENTION — AND PLAN 9 IS THE GUIDE**
  (Christine's rule, 2026-09-03, **clarified 2026-09-18**). The rule as first
  written was *"you yourself said the kernel does not grow. The kernel only
  handles process orchestration. everything else is handled by host or
  userspace. Everytime you design a change to the kernel, the design is
  wrong."*

  **What it means, in her words:** *"that rule was to stop you from adding all
  sorts of invented stuff in the kernel. the kernel focuses on the one thing it
  does best — process orchestration, but it doesn't mean that is the only thing
  the kernel does. Use Plan 9 as a guide."*

  So the test is **provenance, not category**. Ask: *does Plan 9's kernel have
  this?* If it does, it may be here — `#c`'s 23 files, a clock, a syscall
  table, `up`, whatever it is. If it does not, it is an invention and the
  answer is in the host or in userspace, whatever it looks like.

  **This was read too literally for most of 2026-09-18**, and it cost real
  work: `#c` was struck out, restored, struck out again and restored again,
  each time by asking *is this category orchestration?* rather than *does Plan
  9 have it?*. Reading it as a ban on everything but process management makes
  the kernel smaller than Plan 9's, which is a deviation in the other
  direction and needs approval exactly as adding would.

  The failure it actually names is concrete: a proposal once offered *"extend
  the host op list with draw operations"* as a legitimate option, and it was
  argued against on other grounds — a second IPC beside 9P — when it should
  have been struck out on sight, because Plan 9 has no such thing.

  A window device is the standing example of an invention. Plan 9 has none:
  rio is an ordinary userspace file server that posts to `/srv`, mounts at
  `/mnt/wsys` and binds that over `/dev` (`rio/fsys.c:170`, `:237`, `:241`).
  Anything that would put windows in the kernel is answered there.
- **READ THE PLAN 9 SOURCE BEFORE WRITING ANYTHING, EVERY TIME** (Christine's
  rule, 2026-09-18). Not when a deviation is suspected — **before implementing
  anything at all.**

  The rule below it, *find the counterpart by file and line*, has been here
  since 2026-09-17 and did not stop seven deviations on 2026-09-18, because it
  **fires on suspicion** and not one of them was suspected: `#c` was excluded
  while I thought I was applying a rule; `devpermcheck` was shifted the wrong
  way while I thought I was implementing what I had read; `mntversion` asked
  for the wrong constant with both constants on screen; `Dev::open` dropped its
  return value because I had copied the method *names* out of `struct Dev` and
  never read the struct. **You cannot suspect what you have not looked at.**

  So the trigger is the act of implementing, not the feeling of doubt:

  1. **Open the counterpart and read the body — at the moment of writing.**
     Not the name, not a grep hit, not something read earlier in the session.
     **A grep hit is not reading.** `mntversion` was implemented from four grep
     hits on `MAXRPC|MAXCMNRPC`; what decides it is `f.msize = msize`
     (`devmnt.c:153`), a line containing neither constant, which no such grep
     can ever show. `devpermcheck` was read correctly hours earlier and then
     written from memory with the shift reversed.
  2. **Read what is around it.** `struct Dev` is seventeen function pointers
     and **no state**, which is the whole reason `devtab[]` can be reached from
     anywhere; taking the method list without reading the struct produced a
     table that owns its devices and a mount driver that could not work.
  3. **Cite it in the code, at file and line**, so the next reader checks the
     claim in one command.
  4. **Write the test that fails if the behaviour differs** — a second net, not
     a substitute. `devpermcheck` shifting the wrong way passes an owner's own
     open and fails every other case; `mntversion` asking for the wrong
     constant caps every session at the old 8K with nothing looking wrong.

  **All seven deviations of 2026-09-18 were step 1**, and none was "read it and
  got it wrong". There is no case on record where reading the body at the
  moment of writing was done and the deviation happened anyway. A first draft
  of this rule claimed otherwise — that reading "is necessary and not
  sufficient" — which was false and had the effect of weakening the rule on the
  day it was written.

  **Where Plan 9's counterpart cannot exist here** — no address space, no
  register set, no hardware — say so at the point of the difference, name what
  Plan 9 does, and keep everything else identical. That is not licence: it is
  the only kind of difference that needs no approval, and it is narrow.

- **EVERY DEVIATION FROM PLAN 9 NEEDS CHRISTINE'S APPROVAL, AND THE DEFAULT
  ANSWER IS NO** (her rule, 2026-09-17). Not "is it defensible", not "is it
  forced by the substrate", not "would Dis need it too" — those are arguments
  for putting to her, not tests to pass on your own. **Find the counterpart in
  `plan9/` by file and line. If there is none, stop and ask.**

  This is not about names alone. A structure can be Plan 9's in vocabulary and
  something else in mechanism: the namespace here was keyed by path text and
  resolved by longest prefix, while Plan 9 keys a mount by the identity of the
  channel mounted upon (`chan.c:855`, `findmount`) and checks at every
  component. Same word, different system.

- **NO CUT-DOWNS, AND A MISSING LIBRARY IS NOT AN EXCUSE** (Christine's rule,
  2026-09-24). *"Your cut down commands came from you being lazy, even though
  I told you to implement them properly"*; *"That's why we have to move away
  from the POC - you made too many shortcuts"*; *"Why are you not building all
  commands. You need to build all the libraries as well. a missing library is
  not an excuse"*. Plan 9's code is vendored whole and built as it is; what
  does not build is a cause to fix — a library to vendor, a machine-dependent
  piece to write (`setjmp` was one) — never a reason to write a smaller
  version or to leave the command out. `userspace/build/failed` is the list of
  what is left, with the reason for each.

- **NEVER INVENT WORDS** (Christine's rule, restated 2026-09-17). Not for a
  thing she has not named, and not a second word for a thing already named —
  a coinage that went into this file came back as jargon in every reply until
  she asked what it meant. Say what a thing is. Search the reference first: the boot script was reported as a gap when
  `plan9/sys/src/cmd/init.c:178` already names it `/rc/bin/termrc`.

- **SPEC'D, PROPOSED or GAP — triage before building** (Christine's rule,
  2026-09-02). Everything sits in one of three states: **spec'd** (explicitly
  discussed *and endorsed* — may be implemented), **proposed** (a design exists
  but she has not reviewed it — needs review first), **gap** (undesigned —
  needs a proposal first). *"everything that we have not explicitly discussed
  and endorsed should be a gap (or proposed if you have created a design).
  proposed designs need to be reviewed."* So the answer to **"implement the
  demo"** is a triage, not a build: *"X is speced, Y is proposed and Z is gap.
  Would you like me to review Y with you before implementing, and would you
  like me to propose Z, before we implement."* Mark proposals **in the document
  itself** — never as settled prose — and **never write an exclusionary
  constraint** (*"and no others"*) she did not state: absence of endorsement is
  a gap, not a prohibition. Naming a thing is not specifying it, and a green
  suite is no defence — the tests assert what was built, not what was agreed.
- **REWRITE THE PROPOSAL AFTER EVERY DECISION** (Christine's rule, 2026-09-02).
  *"The decision itself moves off the proposal — so I am reviewing genuine open
  decisions rather than settled decisions."* `docs/proposals.md` is a **register
  that shrinks**, not a document that grows: the moment something is decided it
  moves into the relevant spec and the decision log, and **leaves** the
  proposal. Adding to a proposal instead of emptying it is how stale blocks
  accumulate and how she ends up re-reading settled material to find what
  actually needs her.
- **Cite primary sources, quote verbatim in blockquotes; numbers are load-bearing** —
  line counts, call counts and file:line references carry the argument, and each traces to
  a measurement or source. State the decision, then the constraint that forced it.
- **Measure rather than assume** — engine capabilities here contradicted folklore (legacy
  EH gone, `try_table` on); RESEARCH.md records measured tables with dates.
- **Upstream source may be brought in; provenance and notices travel with it.** Plan 9,
  plan9port and APE are MIT (Foundation transfer, March 2021) and the design calls APE "a
  source to cut down" — importing such source is expected, keeping the Foundation's
  notice per `LICENSE`. Research Unix source is governed by the covenant reasoning in
  the parent repository; when any lands here, update `LICENSE`'s scope note in the same
  commit. Separate from licensing: **the V10 tree used for measurements stays in
  `../ipnx`** so every quoted number keeps file-and-line provenance (RESEARCH.md's own
  rule).
- Prose is British-inflected (`licence`, `rasterise`), em-dashed; tables carry comparisons.
  Guest C is Plan 9 style (tabs, `nil`, no const clutter); the build silences the
  builtin-redeclaration warnings that style causes.

## Current state

**[docs/when.md](docs/when.md) is the single authoritative statement of build
status, and no other document carries it** — including this one. What was here
described a tree that no longer exists.

The plan is [docs/implementation.md](docs/implementation.md): P0–P8 to the
demo. The load-bearing engineering lessons live where they always did:
RESEARCH §5 (fork, the transport), §9.4–9.6 (the toolchain, kencc call-site
adjustment, the native core's findings), §10 (the deviation audit) and §11
(what building the userspace measured).
