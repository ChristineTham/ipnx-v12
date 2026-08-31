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
/dev/window/
  clone            mint a window; reading it returns the new number
  events           reads PARK — one line per window lifecycle change
  <type>/<n>/
    ctl            panes, layout, tabs, lifecycle
    content        the PATH the host opens over 9P and renders
    toolbar        one control per line: <label> <action>
    tag            the tag line — the host's way back into sam
    events         the host speaks: clicks, tag commands, dirty, resize, close
```

`clone` and the root `events` are the house shape — `/net/tcp/clone`,
`/dev/draw/new`, `#s`. Two levels of `events`: the **root's** carries window
lifecycle (a window appeared, was destroyed, changed type); a **window's** carries
what the user did inside it.

## Any program may open a window

`/dev/window` is not emca's. **Any program writes to it, and that is how emca
itself operates** — emca has no privilege, only a job.

The practical shape this gives every tool: one binary, and a flag is the only
difference between a command-line utility and a system manager.

```
pkg              lists packages to stdout
pkg --emca       mints a window and lists them there
```

## How anyone learns a window exists — and why there is no `/dev/emca`

The device mints, so nothing needs to *announce*. The real difficulty is
narrower and is already named in the plan (M13): **9P has no change
notification** — *"poll, or a synthetic event file"*. The house answer is the
second, and canvas already uses it: a file whose reads park.

So the root `events` file is read by **both** the host and emca, and that
dissolves the question of who tells whom:

| | role |
|---|---|
| **the device** | mints the window — **mechanism** |
| **emca** | watches; *places* the window (which leaf, tab or pane) by writing its `ctl` — **policy** |
| **the host** | watches; renders it where emca placed it |

emca is a **watcher, not a gatekeeper.** Programs mint directly, so emca holds no
privilege; but emca is not bypassed either, and keeps the workspace it owns
(layout, session, `Dump`/`Load`). Policy in a userspace program watching a device
is the Plan 9 move — the same shape as `/rc/tile` being a window manager in a
dozen lines of rc.

**Two rejected alternatives**, recorded so they are not revisited: *programs write
and only the host watches* loses emca's workspace, since a window it never learns
of is outside the session it owns; *programs ask emca, and emca tells the host*
makes emca a required intermediary and contradicts "any program may write to
`/dev/window`" — it is what acme did through `/mnt/acme/new`, and it worked only
because acme **was** the file server. Here the device is.

**And it degrades correctly**: with emca not running, the window still exists and
still renders — in its type's default pane, since the type is in the path. So
`pkg --emca` works with no shell at all. Neither alternative had that property.

**One race, named rather than discovered**: mint → the host renders → emca places,
so for an instant a window sits in its type's *default* pane rather than its
considered one. Benign by construction — type already determines default
placement, so a window never appears somewhere *wrong*, only somewhere
provisional, and at most it moves once.

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

## The event vocabulary

Reused from canvas rather than invented — the verbs and their fields are the
ones already in service, which keeps one vocabulary across the system.

**Root — `/dev/window/events`** (window lifecycle; read by both emca and the
host):

```
new <type> <n>            a window was minted
del <n>                   a window is gone
```

**Per window — `/dev/window/<type>/<n>/events`** (what the user did):

```
exec <label>              a toolbar control was activated
tag <text>                a command typed in the tag line — sam's language
execute <q0> <q1> <text>  the execute verb landed on this range
look <q0> <q1> <text>     the look verb landed
insert <q0> <text>        the user typed or pasted
delete <q0> <q1>          the user removed a range
select <q0> <q1>          the selection (or collapsed caret) changed
dirty <0|1>               the buffer's dirty state changed
seq <n> <hash>            the mirror's sequence and buffer hash
resize <w> <h>
close
```

`seq` is the divergence check from *The buffer* above: emca compares and
resyncs on mismatch. Everything else is a user action.

## `ui` — what the surface actually rendered

Each window also carries a **`ui`** file: the controls the surface produced, one
per line — label, role, keyboard path, enabled state.

This exists so the **grammar is testable**, which nothing else in the design
made possible. On a real surface it is derived from the **platform accessibility
tree** — which is precisely the "can this be reached" answer, on the web and on
Apple alike — and the virtual surface synthesises it. So one assertion (*every
floating-bar verb and every toolbar button is present, named and
keyboard-reachable*) runs on every surface, headless included.

It also converts a claim into a measurement: the input convention holds that
accessibility is *"enforced by construction"*, and until now nothing checked it.

## The transcript

A transcript window is not a special mechanism. Its `content` is a **growing
file the host tails**; typed lines arrive back through the window's `events` as
ordinary `insert` lines terminated by a newline. History and line editing are the
host's (*editing is the surface's*), which **deletes con(1)'s mark arithmetic** —
the input region is just the host's editable tail.

For programs wanting keystrokes rather than lines, the raw-input door is
xterm.js, per the console's "AND, not XOR" (userland.md).

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
  acme's interface arguably should have had. The obligation it carried —
  *`/dev/window` must be usable by ordinary programs*, that being the acme paper's
  §7 and the thing that made acme extensible without plugins — is **discharged
  above**: any program mints through `clone`, and emca is a watcher rather than a
  gatekeeper.
- ~~Event granularity on the way back~~ — **specified above**, reusing canvas's
  vocabulary rather than inventing one.
- ~~The transcript's shape~~ — **specified above**: a growing file the host tails,
  typed lines back as `insert`, con(1)'s mark arithmetic deleted.

Nothing is open in this contract. What remains is that **none of it is built** —
`/dev/window` is specified and M14a is unstarted.
