# /dev/window — the control interface (v0, specified 2026-08-31, NOT BUILT)

The contract for the 2026-08-31 redesign (design.md): the bidirectional
device through which IPNX declares a window's chrome and the host reports what
the user did. **Not a protocol** — 9P is the only protocol. This is a device
presenting files, and what varies per window type is the *semantics* of those
files.

Christine's test for the whole design, which is the reason this document exists:
*"This is the only solution that fits the principle (everything is a file, per
process namespace, and 9P is the only protocol)."*

## The derivation

Not chosen — forced, by three founding decisions:

| decision | what it forces |
|---|---|
| everything is a file | a window is a file tree |
| 9P is the only IPC | host↔IPNX is 9P; there is no second wire, only conventions |
| per-process namespaces | a window's tree binds into the process that owns it |

The error this corrects is recorded in [canvas.md](canvas.md): calling
`/dev/canvas` *"the display protocol"* invited treating it as the place all
host/IPNX exchange happens, until it carried layout, chrome, text and drawing.
Three of its four founding benchmarks were never drawing consumers.

## The division of labour

| carried by | what |
|---|---|
| **9P, directly** | the file itself. IPNX names it; the host mounts it and **renders it natively** — text, SVG, HTML, Markdown, PostScript, images. **IPNX implements no renderers.** |
| **`/dev/window/<type>/<n>`** | chrome and control, both directions |
| **`/type`** | what types exist, what command drives each, what that type's exchange means |
| **`/dev/canvas`** | genuine drawing, the exception |

## The tree

`#w` mints windows as it always has; `/dev/window` is where it is conventionally
bound. This is that device grown up — it keeps minting and owning windows and
gains the interface it should have had.

```
/dev/window/<type>/<n>/
  ctl        panes, layout, tabs, lifecycle
  content    the PATH the host opens over 9P and renders
  toolbar    one control per line: <label> <action>
  tag        the tag line — the host's way back into sam
  events     the host speaks: clicks, tag commands, dirty, resize, close
```

**The type is a path component, not an attribute.** `/dev/window/proc/1` tells
the host it is drawing a proc window, exactly as `/net/tcp/0` differs from
`/net/udp/0`. This is the house pattern throughout — `/proc/17`, `/net/tcp/0`,
`/mnt/acme/27` — and it means the whole window system greps.

## Actions name a side

The capability that had no home in canvas, and the reason a control interface
was needed at all: a control must be able to say **which half performs it.**

```
Put      ipnx:Put            round-trips; emca decides what it means
Look     ipnx:Look
Wrap     host:toggle-wrap    never round-trips; layout is the surface's
Split    host:split-right
```

`host:` actions are the two-halves split enforced in the vocabulary: *toggle
wrap, split the pane, show the rail, switch tab, change font size* are the
surface's by rule, and must not cost a round trip. `ipnx:` actions carry the
verbs whose **meaning** is policy — which is IPNX's half.

## Editing, and what `Put` is

The host **edits** (*editing is the surface's*, design.md 2026-08-31 — Monaco,
TextKit, or the platform's own) and holds a **mirror** of the buffer; emca holds
the authoritative copy (see *The buffer* below). On Save the host **streams the
edited file back to IPNX over 9P**: an ordinary write, no new mechanism, and it
doubles as a resync — the whole file arriving is a chance to confirm the two
copies agreed.

Dirty state therefore needs no reporting channel of its own: emca knows the
buffer and knows what was last written, so `Putall` and `Exit`-refuses-if-dirty
are answered locally.

## The buffer: mirrored, with divergence detectable

**emca holds the authoritative buffer; the host mirrors it.** Four independent
reasons force this, only one of which is undo:

- **the suite runs headless** — against a virtual surface there is no host
  buffer, so every acme behaviour test would break
- sam's structural commands operate on text
- `Put` and the `| < >` filters need it
- the `body` file serves client programs

**The risk, named**: emca writes too (sam, `Get`, `+Errors`, steering `sel`), so
this is not replication with one writer — it is two writers on one buffer, which
is collaborative editing. The canvas design already solved it and the solution
is lifted rather than reinvented: *the device applies `insert`/`delete` to the
data **before** queueing the event, so echo and data are one thing.* Single
source of truth, optimistic local echo.

**Divergence must be detectable.** Its failure mode is not a crash but silent
disagreement — one dropped edit and every later `s//` operates on text that is
not there. So each edit carries a **sequence number** and each sync a **hash of
the buffer**; mismatch triggers a resync.

That amends a load-bearing discipline on purpose: con(1)'s *"apps never re-read"*
becomes **"apps never re-read *routinely*"**. A resync on detected divergence is
a repair path, not normal operation.

## Undo: one stack, and it lives in emca

The host's undo is **disabled**; `⌘Z` round-trips to emca, which undoes by
sequence number and pushes the change back.

This is simple rather than a compromise because **emca is not remote** — a
process on the same machine behind a SAB mailbox, so a round trip is
microseconds. Every argument that forces web editors into local optimistic undo
stacks is a *latency* argument, and none apply. Acme's infinite undo survives
exactly, with no interleaving semantics to explain and no "sometimes it undoes a
keystroke, sometimes a command".

The exception, for later: a **remote** surface under M12's distributed story
would feel the trip. Only then is local undo worth its complexity.

**Versioning is not undo.** `#V` is tree-granular and durable; undo is
edit-granular and session-scoped. Neither substitutes, and because versioning is
optional (decision log), **nothing may be built on it** — undo cannot be "walk to
the previous version", or a user with versioning off would have none.

## `/type` — read by both sides

The registry binds a type name to three things:

- **what IPNX command drives it** (optional — a type with no command is rendered
  from its files alone)
- **what chrome it declares** — the toolbar's default contents
- **what the semantics of its host/IPNX exchange are** — still 9P, but a `proc`
  window and a `text` window agree different things about their files

This answers *"how does the host know what this is"* without content sniffing,
and it is the same registry the emca design already required.

## What this replaces

From canvas: `stack` (layout → `ctl`), `text` and `edit` (content → a file over
9P), and the `role=`/`type=` attrs specified earlier the same day. From the app:
every renderer emca would have needed, and most of its display machinery.

## Open

- ~~One window vocabulary or two?~~ **Answered 2026-08-31: one.**
  `/mnt/acme` retires and merges into `/dev/window` — the two were the same files
  pointing opposite ways (a program driving emca; emca driving the host), and the
  only difference was that `/dev/window` declares toolbars and actions, which
  acme's interface arguably should have had. **The obligation this carries:
  `/dev/window` must be usable by ORDINARY PROGRAMS**, not only by emca and the
  host — that client interface is the acme paper's §7 and the thing that made
  acme extensible without plugins. Designed for from the start, not retrofitted.
- **Event granularity on the way back** — a click is obvious; what a tag command,
  a selection, or a dirty transition looks like as a line is not yet specified.
- **The transcript's shape** — a growing file the host tails plus a line
  back-channel is the natural fit, and would delete con(1)'s mark arithmetic, but
  it is not yet confirmed.
