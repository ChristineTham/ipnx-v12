# When — what is built, and what is not

**Role: the *when*.** The single authoritative statement of the system's
current state. **No other document carries build status** — a *when* fact
written into a *why* or a *what* document goes stale there and contradicts this
one, which is how four different test counts came to exist across the
repository (2026-09-02).

| | |
|---|---|
| the sequence and its milestones | [implementation.md](implementation.md) — *how* |
| what the system is | [architecture.md](architecture.md) and the specs — *what* |
| the frozen PoC's own record | [poc.md](poc.md) |

## The suite

**164 tests, 0 failures**, identical on both hosts — and **measured in Chromium on the Rust core on 2026-09-04** (the browser run takes ~5 minutes against ~1 under wasmtime; see RESEARCH §9.16):

```
node demo/supervisor/main-rust.mjs userspace/rootfs   # the Rust kernel core
bash poc/run.sh                                       # the frozen JS oracle
```

The permanent floor is **131** (the PoC's declaration count, 2026-08-29); the
count grows as features add self-skipping tests. `poc: all 64 tests passed` is
init.c's C-level tranche only, not the whole suite — the two numbers measure
different things and both are correct.

## Replanned 2026-09-04 — what follows is LEGACY state

The plan was rewritten in three layers with the demo as a milestone
([implementation.md](implementation.md)); everything below this line describes
**what the legacy code does today**, which the plan refactors toward the design
rather than builds on. The milestones named below are from the archived plan
and are **not** the current sequence. The one number that carries forward is
the suite's — and its floor is being redefined as *what passes on the pure
kernel*, since the 164 assert several things the kernel is losing.

## Landed — under the archived plan

| | |
|---|---|
| **M0** the tree completes | 2026-08-29 |
| **the package manager, v1** | 2026-08-29 |
| **M1** the `FROM scratch` container | 2026-08-29 |
| **M2** the namespace-file boot | 2026-08-29 |
| **M3** the macOS app | 2026-08-30 |
| **M4** host storage (v1) | 2026-08-29 |
| **M5** the browser host on the Rust core | 2026-08-30 |
| **the public demo** — [christham.net/ipnx-v12](https://christham.net/ipnx-v12/) | 2026-08-29 |
| **M14a–d** the control interface, `/type`, emca's IPNX half, the web surface | 2026-08-31 |
| **M15a–e** the tree, allocation and sizing, geometry, the root convention | 2026-09-01 |
| **M15f** *part* — the types and the toolbar | 2026-09-01 |
| **M17c** *part* — the MIME registry, the layout file, `emcaopen` by path | 2026-09-02 |
| **M17a1** emca is a 9P file server — its windows served as files from its own state, at an instance-qualified door | 2026-09-03 |
| **M17a2** the window tree leaves the kernel entirely — emca decides it and serves it at `/dev/emca/<n>/`; 285 lines deleted from the kernel | 2026-09-03 |

Also running: the real Plan 9 userspace (rc, sam, samterm, acme, twenty-four
commands over `libp9.a`), the V10 exhibit in `/v10/bin`, and the WASI second
ABI's three citizens — wasi-libc, Go `wasip1`, CPython 3.14.

## Not built

**Everything after the demo is a documented gap in the plan**, filled in order
and designed when reached — see [implementation.md](implementation.md), *"After
the demo — to the end, on every target"*, where each phase lists the gaps it exposes. The demo itself (D1 the CLI, D2 the
website) is what is being built now; nothing in it is undesigned.

## Superseded

**M14's layout half was superseded by M15** (2026-09-01). M14a–d built emca's
surface on three hardcoded named regions; the compositor replaced that. What
survives: the control interface (`/dev/window`), `/type` as a registry, the
editor component, content over 9P, and emca as a watching file server. What
does not: every part that assumed named panes.

### Defects against emca's agreed baseline — 2026-09-02

Recorded 2026-09-02, found by checking the above against the code. These are
defects against an agreed design, not open questions:

| the baseline | the build |
|---|---|
| controls are **close, minimise, maximise** and **duplicate** (three buttons today: as column, as row, as tab), available at all times | the surface renders **close** and **collapse** only; no maximise control exists |
| **minimise**, reversible | renamed **collapse**, with a code comment arguing maximise away as "a third state" — a rename and a design change, neither agreed |
| every window has a **status line** | no per-window status line exists (`wstatus`: zero occurrences) |

The kernel already implements `minimise`, `maximise`, `newrow`, `newcol` and
`newtab`. The gap is entirely in the surface.

## emca: status and what remains

Moved here from emca.md 2026-09-02 — a spec does not carry its own build status.

### What actually remains

```
- THE FLOATING BAR'S ORDER AND GROUPING — empirical, tuned against
  use once there is use. The set is settled.
- ~~THE EDITOR COMPONENT'S SPIKE~~ — ANSWERED 2026-08-31 by
  observation. CodeMirror 6 is vendored (374KB, offline) and satisfies
  the criterion: it has no menu of its own and uses the PLATFORM's,
  which is where the verbs belong. A selection right-clicked gives the
  closed set, and choosing Look put `look 29 36 surface` in the
  window's events for a plain guest to read. Byte offsets are correct
  across a multi-byte character. PROPERTY 1 SURVIVES A RICH EDITOR.
- AND THE WHOLE OF IT IS UNBUILT. That is the real open item:
  every decision in PART EIGHT is design, M14 is unstarted, and the
  suite's 151 pass because the code is exactly where it was.
```

Status: DESIGN, complete first pass (2026-08-31). Nothing has shipped.
This document is the design record for emca; the decision that adopts
it is dated in design.md, and the milestones that build it are in
implementation.md.

emca is not an editor, and not a replacement for acme. In Christine's
words, the reframe that produced this document (2026-08-31):

```
"What we have been designing is not acme, or a replacement for
 acme. It is the shell that IPNX boots into, it is the IPNX primary
 user interface, it is the browser surface, the macos app, the ios
 app. The system boots into emca. Emca is the user interface."
```

And what it makes the system:

```
"It's not just a barebones UNIX reimagined, it is a full operating
 system with our own semantics, user interface, artifacts."
```

The name is acme backwards, because the mouse comes off and the verbs
go on. An editor is one window type inside it.

The parts list this design works from is docs/acme.md — acme's four
layers, its 38 operations, and the census that found only three verbs
genuinely homeless. Read that first; this document does not repeat it,
and the split it discovered runs right through emca:

```
acme's L1 (tiled window management) and L4 (the bindings) belong to
EMCA-THE-SHELL — system-wide.
acme's L2 (the tag) and L3 (the operations) belong to THE WINDOW
TYPE — per-window.
```

The anatomy already had the seam; nobody knew what was on the far
side of it.

The governing directive, hers (2026-08-31): "We implement the
concepts and the benefits, not the shape."

### /dev/window — what landed (moved from window.md, 2026-09-02)

> **SUPERSEDED THE SAME DAY.** What follows records the KERNEL device as it
> stood on 2026-08-31. The window system left the kernel entirely on
> 2026-09-02 — emca serves these files now, at `/dev/window/` (a manager's own)
> and `/dev/emca/<n>/` (the full set). The interface's *shape* survives; its
> server does not. Read this as history: [window.md](window.md) is the
> contract.

> **The device half LANDED 2026-08-31.** `#w` grew the control interface and the
> suite proves it headlessly: a window minted through `#w/pkg/clone`, the mint
> announced on the root `events`, a toolbar declared with `ipnx:` and `host:`
> actions, content set to a path, the type in the path **validated** (`#w/text/<n>`
> correctly misses), the plain `#w/<n>` still answering (nothing broke), and the
> surface's voice round-tripping. **152 PASS / 0 FAIL** on the Rust host; the
> frozen oracle self-skips.
>
> **The host half landed the same day too.** Chrome crosses to the host as an
> ordinary effect (`WinChrome`), the browser shell renders it as **native
> furniture** — content path, real buttons, a tag field — and a click returns
> through `bh_win_event`. Actions naming a side are honoured in the surface:
> `host:toggle-wrap` never leaves it. Proved headlessly and repeatably by
> `node demo/supervisor/winproof.mjs userspace/rootfs`, which mints a window
> from rc, reads the toolbar the host was handed, clicks the `ipnx:` control,
> and watches a plain guest read `exec Install` out of the window's events —
> **with no emca anywhere in the loop**, which is the claim that most
> distinguishes this design from acme's.
>
> **And the content half landed too.** The surface *opens the file itself* —
> `readPath` on the bridge, resolved in the namespace, answered as
> `Effect::ReadDone` — and **the browser renders it with its own engines**: SVG
> as SVG, images as images, HTML in a sandboxed frame, anything else as text.
> `IPNX implements no renderers` is now a fact about the code, not a claim.
> Verified both ways: `winproof.mjs` opens a 602-byte SVG headlessly, and in the
> browser the same file is *drawn* beside its declared toolbar.
>
> **On 9P and the wire**: in-process this is a function call, per the founding
> shape — *"wire 9P at boundaries, a Dev table inside"*. A surface sharing the
> address space calls; a **remote** surface (M12) marshals. The host decides what
> to open and when, and renders it, which is the property that matters.
>
> **And the editor component landed, which answers the spike.** Text windows use
> **CodeMirror 6** (374KB, vendored, offline). Caret, selection, clipboard, IME,
> wrapping and find are the component's and none were written. What crosses to
> IPNX is only what IPNX owns: the buffer as `insert`/`delete` with a **sequence
> number and a hash per change**, the selection, and the verbs. `⌘Z` is
> intercepted and sent to emca — **one undo stack**, as decided. `⌘S`/`Put`
> streams the file back through `writePath`, truncating.
>
> **The spike's question — can the verbs ride a rich editor's grammar? — is
> answered YES, and observed.** CodeMirror has no menu of its own; it uses the
> platform's, which is exactly where the verbs belong. Selecting a word and
> right-clicking gives the closed set (Execute · Look · Pin · Cut · Copy ·
> Paste), and choosing one puts `look 29 36 surface` in the window's events,
> where a plain guest read it. **Property 1 survives a rich editor**: any text is
> still a verb's operand.
>
> One detail worth keeping: those byte offsets are correct **across a multi-byte
> character** — the file opens with an em-dash, and "surface" genuinely begins at
> byte 29. The protocol's coordinate and the editor's are reconciled, not
> assumed.
