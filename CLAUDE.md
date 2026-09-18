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

Build the guest binaries (requires wasi-sdk at `~/.local/opt/wasi-sdk`, binaryen's
wasm-opt at `~/.local/opt/binaryen` — override with `WASI_SDK`/`BINARYEN` — the
host's `bison` for vendored yacc grammars, `go` for the wasip1 citizen, and
network once for the CPython wasi build (a 26MB download cached in `build/`);
Apple's clang has no wasm backend). `mk.sh`'s
`ASYNCIFY` list names the binaries that may bare-fork. Compiling vendored libc source
REQUIRES `-fno-builtin`: clang's libcall recogniser otherwise rewrites strlen's own body
into a self-call (measured; RESEARCH §9.4). Two more findings are load-bearing
(RESEARCH §9.5): `--table-base=4096` keeps wasm's small function-table indexes disjoint
from rc's operand integers (`codefree` tells ops from operands by comparing `.f` slots),
and `weaken.mjs` restores common-symbol semantics for rc.h's pre-ANSI tentative
definitions — the wasm backend refuses `-fcommon` and wasm-ld's
`--allow-multiple-definition` keeps the *first* definition even over a later
initialized one (measured: plan9.o's zero `havefork` beat `havefork.c`'s `= 1`):

```bash
bash userspace/mk.sh
```

The real implementation — the Rust kernel core (`kernel/`, RESEARCH §9.6; OS-free:
'#Z' host files are ops delegated to the embedding) under the macOS wasmtime host
(`hosts/macos/`), compiled to wasm for the browser (`hosts/browser/` +
`demo/supervisor/rustkern.mjs` — THE DEMO'S KERNEL since 2026-08-31; the JS demo
lineage is reference-in-tree), and driven headless by Node
(`node demo/supervisor/main-rust.mjs userspace/rootfs`, which loads
`target/wasm32-unknown-unknown/release/browserhost.wasm` and so needs the wasm
build first) — builds and runs from the root Cargo workspace:

```bash
cargo run --release -p host -- userspace/rootfs
cargo build --release --target wasm32-unknown-unknown -p browserhost
```

Node ≥ 22 (`worker_threads`, SAB, wasm `try_table` exception handling — the legacy EH
encoding is *rejected* by these engines, so any new wasm emission must use `try_table`).
`userspace/build/` and `userspace/rootfs/bin/` are generated and gitignored; `userspace/VERSIONS` records the measured toolchain and `mk.sh` warns on drift. Guest binaries carry no
`.wasm` extension: exec walks the namespace for `/bin/cat`, and a freshly produced module
is indistinguishable from a shipped one.

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
| `docs/implementation.md` | **the plan — a redesign and rebuild (2026-09-17)**: phases P0–P7 to the demo (the CLI, then the website), built from the design and from `plan9/`. Superseded plans are in `docs/archive/` |
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
plan, deliberately, and a change to it changes both. **README.md is Claude's, like every other document here.** Christine edited a few paragraphs of it and has not reviewed the rest; the claim that it was hers, in her voice and not to be touched, was Claude's invention and was used to exempt it from an audit she had asked for. Correct it like anything else. Prefer a
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

Note the distinction from `userspace/plan9/`, which is the **vendored** subset
actually compiled into the system (libc, libdraw, libframe, the commands) and
IS committed.

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
as hers. `#H`, `#V`, `#Z`, an effect list at the embedding boundary, a
namespace keyed by path text: all of them became "decided" that way. The log is
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

## The tree (post-declaration, 2026-08-29)

The implementation lives at the top level: `kernel/`
(the Rust core), `hosts/macos/` (wasmtime host; the workspace root is `Cargo.toml`),
with `hosts/{oci,ipados,browser}/` scaffolded per implementation.md's milestones. The
guest world lives at `userspace/`: the
libcs, the vendored trees (verbatim), `cmd/`, `wasi/`, the rootfs seed, `mk.sh`
and `VERSIONS` — the real userspace shared by every host, not frozen. Work is
sequenced by
`docs/implementation.md`. The target is **functional equivalence to the demo**.

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

- **THE KERNEL DOES NOT GROW — AND A DESIGN THAT CHANGES IT IS WRONG**
  (Christine's rule, 2026-09-03). *"you yourself said the kernel does not grow.
  The kernel only handles process orchestration. everything else is handled by
  host or userspace. Everytime you design a change to the kernel, the design is
  wrong."*

  **It is a TEST, applied before a design is written down, not a preference
  weighed against others.** If answering a question requires adding to
  `kernel/`, the answer is wrong and the real answer is in the host or in
  userspace. The kernel may **shrink**; that is the only direction it moves.

  The failure it names is specific and recent: the raster proposal's "Design A" proposed
  extending `HostOp` with draw operations. It was offered as a legitimate
  option and argued against on *other* grounds — a second IPC beside 9P — when
  it should have been struck out on sight for growing the kernel.

  **What the rule says about what is already there:** `#w` — the window device —
  is **~1,100 lines, 22% of the kernel** (745 in `wsys_*`/`win_*`/`cv_*`/
  `drawmsgs`, 364 in `draw.rs`), and none of it is process orchestration. the legacy a1
  and a2 steps removed the tree by this reasoning without the rule being written
  down; the rest follows the same way.
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

## Current state — REPLANNED 2026-09-04

**The code is legacy.** Everything below describes what it does today and is
refactored toward the design under `docs/implementation.md`'s three layers, not
built upon. Layer 1 has no window, no mouse, no draw, no canvas; the kernel is
cut to a subset of Plan 9's measured against `plan9/`; Go and Python become
packages; the demo is `ipnx` in a terminal plus the website, and iOS and the
macOS app come after it.

### What the legacy code does (design: 2026-08-31; code: 2026-09-03)

**emca is designed AND the IPNX half of it runs** (2026-08-31): emca is the IPNX
user interface — `/dev/canvas` narrows to genuine drawing, `/pkg` and `/project`
split the old package concept, and the hosts become *surfaces* rather than demos
([docs/emca.md](docs/emca.md), decision log 2026-08-31). Landed the same day
(implementation.md M14a–c): the `/dev/window/<type>/<n>` control interface, `/type`
as a real registry of four small files per type, the browser surface rebuilt as
emca itself (top toolbar of managers, panes, tabs, a status line carrying the
global tag), the editor component (CodeMirror behind a mirrored buffer), and
**`userspace/cmd/emca.c`** — a file server with a workspace, not an editor: the
window set, each window's one tag string, the core verbs merged with the type's,
dirty state, aliasing buffers, and placement. It is a **watcher, not a
gatekeeper** — with no emca running a window still opens in its type's default
pane. **And the web surface (M14d)**: the floating bar at the selection with
emca answering verb applicability (a path offers Open, an address Jump, a word
neither — acme's `look` decomposed and SHOWN), the pin replacing the 2-1 chord,
the responsive rules measured in CHARACTERS not pixels, and the keyboard grammar
entire. **164 PASS / 0 FAIL** on all three hosts, plus two headless surface proofs
(`winproof.mjs` with no emca, `emcaproof.mjs` with it). Still design-only:
`/project` (M14e) and the SwiftUI surface (M14g).


**The public demo is live: <https://christham.net/ipnx-v12/>** — the frozen
browser port on GitHub Pages behind a COI service worker; redeploy per the
handbook. **Work now follows `docs/implementation.md` as replanned on 2026-09-04**: the demo
is the first milestone — **D1**, `ipnx` in a macOS terminal booting to `rc` with
every Layer-1 command, and **D2**, the website with emca to spec — built from what
is designed today and nothing undesigned. Layer 1 (the kernel cut to a subset of
Plan 9's, the root the Plan 9 way, the personalities as userspace, the `ipnx` host)
precedes Layer 2 (emca on the pure kernel, the browser surface reading files).
After the demo the plan runs to the end state — IPNX and Saranos on browser, macOS
app, iOS app, container, MicroVM and real hardware — through gaps documented in
the plan and designed only when reached. The load-bearing
engineering lessons live where they always did: RESEARCH §5 (fork, transport, SAB
TextDecoder), §9.4–9.6 (toolchain, kencc call-site adjustment, the native core's
findings), and the decision log for everything chosen.
