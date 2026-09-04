# /dev/window — the kernel device (SUPERSEDED)

*Moved from window.md 2026-09-02, when the window system left the kernel
entirely. This described `/dev/window/<type>/<n>` as a KERNEL device — the
bidirectional interface through which IPNX declared chrome and the host
reported events. Its ideas survive (actions naming a side, the type in the
path, chrome as a declaration); its location does not. The contract that
replaces it is [window.md](../window.md), served by emca.*

## The derivation

Not chosen — forced, by three founding decisions:

| decision | what it forces |
|---|---|
| everything is a file | a window is a file tree |
| 9P is the only IPC | host↔IPNX is 9P; there is no second wire, only conventions |
| per-process namespaces | a window's tree binds into the process that owns it |

The error this corrects is recorded in [canvas.md](../canvas.md): calling
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
  pin              THE PINNED RANGE, at workspace scope — emca declares it,
                   the surface shows it in the status line
  <type>/<n>/
    wctl           layout and lifecycle — rio's file, grown `newrow`,
                   `newcol`, `newtab`, `minimise`, `maximise`, and a
                   `delete` that takes the subtree with it; reads
                   unchanged (rio parses them)
    axis           how this window arranges its children: row (side by
                   side), col (stacked), or empty for one holding a body
    alloc          `allocated` or `tab` — whether the parent has given
                   this window a rectangle. A tab is a WHOLE window with
                   no rectangle, never a reduced one
    kids/<i>/      THE CHILDREN, as a walkable directory, named BY
                   POSITION — because order IS the layout and `ls` sorts,
                   so naming entries by window id would have hidden the
                   arrangement. Each walks to that child's own window
                   directory; its id is in `winid`, and it is reachable
                   equally at #w/<type>/<id>
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
content <type> <n> <path> a window's content file was written
del <n>                   a window is gone
```

**`content` is an event, not a sample** (landed 2026-08-31). A window is minted
*before* its file is known — `clone` first, `content` after — so a watcher that
read `content` once at mint would race whoever fills it in, and emca is a
watcher. Announcing the write closes that race, and the same line is how a
surface reopens an existing window on a different file.

**Per window — `/dev/window/<type>/<n>/events`** (what the user did):

```
exec <label>              a toolbar control was activated
open <text>               a new window whose title is <text>
find <text>               select every item <text> names; dot becomes a set
run <text>                execute <text>, ignoring dot
pipe <text>               selected items to <text>'s stdin, one per line;
                          the results open in a new window
edit <text>               apply <text> as a sam command to each selected
                          item's text
add <text>                put <text> on this window's toolbar as a button
snarf <text>              the surface copied — /dev/snarf is the sync point
insert <q0> <text>        the user typed or pasted
delete <q0> <q1>          the user removed a range
select <q0> <q1>[ <q0> <q1>]...
                          the selection changed. A LIST OF RANGES, not
                          one: Find selects every match, so dot is a
                          SET and the surface's multi-cursor is the
                          same object (emca.md). One pair is the
                          ordinary case; no pairs is a collapsed caret
dirty <0|1>               the buffer's dirty state changed
put                       THE SURFACE PUT — it already wrote the file
seq <n> <hash>            the mirror's sequence and buffer hash
resize <w> <h> [<cellw> <cellh>]
                          two fields is a user drag; four is a surface
                          reporting its VIEWPORT and its TEXT CELL, both
                          device-independent. Only the surface knows the
                          cell, so only it can report one
size <w> <h>              the surface decoded a picture emca could not
                          parse and says how big it is
close
```

`put` is a **notification, not a command**, and the distinction is load-bearing:
the surface holds the real editor's byte-exact text, where emca's buffer is
reconstructed from the change stream above. So the surface writes the file and
emca **re-reads what landed** — one writer per file. Were it the other way round
(surface writes, then `exec Put`), any reconstruction error would silently
overwrite correct bytes. `exec Put` remains the road for a window with no editor
component behind it, where emca's buffer *is* the only copy.

`seq` is the divergence check from *The buffer* above: emca compares and
reports on mismatch. Because `put` re-reads, a divergence can no longer corrupt
a file — it is a diagnostic, which is what makes it cheap enough to always send.
The hash is FNV-1a **over the bytes**, not over UTF-16 code units: the offsets
in this vocabulary are byte offsets, and a check measured in different units
than its coordinates false-positives on exactly the multi-byte content it exists
to protect. Everything else here is a user action.

## The range verbs, and which side runs them

Splitting acme's `look` into **Open / Jump / Search** is not only a display
change — it **re-divides the labour**, along the line the toolbar already draws
between `ipnx:` and `host:`.

| verb | side | why |
|---|---|---|
| Open | IPNX | mints a window and resolves against the namespace |
| Execute | IPNX | runs a command in the window's directory, and consumes the pin |
| Pin | IPNX | workspace state, and `execute` is emca's, so emca must hold it |
| Edit | IPNX | sam's structural language |
| Jump, Search | surface | moving a caret inside a buffer the surface already holds |
| Cut, Copy, Paste | surface | the platform's clipboard, IME and permissions |
| where selection verbs appear | surface | a native callout, a context menu, a small bar, or nothing — an EMPTY TAG LINE already reaches all six from fixed, visible buttons, so this surface is optional |

Sending Jump or Search down would be a round trip to accomplish nothing; taking
Cut/Copy/Paste would replace working platform behaviour with a worse copy.
`/dev/snarf` stays the sync point, written by the surface after a copy.

**Applicability is emca's judgement, and the bar SHOWS it.** A path that exists
offers Open; an address offers Jump; a word offers neither. **A path takes
precedence over an address**, which is acme's own order — `look` tries the file
first — so `/usr/kitty/` is a directory and not the regexp `usr/kitty`. And a
regexp address is **delimited at both ends**: without that rule every absolute
path reads as one, and `/etc/motd` offers Jump, which is exactly the silent
misjudgement the bar exists to expose.

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

## The console window

A console window is not a special mechanism. Its `content` is a **growing
file the host tails**; typed lines arrive back through the window's `events` as
ordinary `insert` lines terminated by a newline. History and line editing are the
host's (*editing is the surface's*), which **deletes con(1)'s mark arithmetic** —
the input region is just the host's editable tail.

For programs wanting keystrokes rather than lines, the raw-input door is
xterm.js, per the console's "AND, not XOR" (userland.md).

## `/type` — read by both sides

The registry binds a type name to three things:

- **what manages it** — the program or component that renders the content,
  edits it, and gives Find, Edit and selection their meaning for that kind of
  thing. It may live on either side: `text`'s manager is host-side, `proc`'s is
  a guest program ([type.md](../type.md)).
- **what chrome it declares** — the toolbar's static verbs
- **what the semantics of its host/IPNX exchange are** — still 9P, but a `proc`
  window and a `text` window agree different things about their files

This answers *"how does the host know what this is"* without content sniffing,
and it is the same registry the emca design already required.

**The registry's shape.** `/type/<name>/` is small text files, and only `window`
is required:

| file | |
|---|---|
| `ns` | bind lines, `eval`'d as rc by `/rc/emcaopen` — so a type's namespace file is executable text |
| `manager` | what handles the content: renders it, edits it, drives the status line, gives meaning to Find and Edit |
| `window` | the type's STATIC verbs, one per line, `<label> <side>:<verb>` |
| `path` | optional default operand, used when `emcaopen` is given no path |
| `README` | prose for the person reading the registry; nothing parses it |

`/rc/emcaopen` knows *nothing* about any type: it reads the registry, applies the
`ns`, and mints the window. **Thirteen types ship**, and **`/type` is itself a
type**, so the interface is configured by editing files inside it. The full
design, including what a manager is and why a type needs one, is
[type.md](../type.md).

> **Superseded since.** The registry landed on 2026-08-31 as `ns` / `cmd` /
> `window` / `pane`. `cmd` became **`manager`** when the type/manager
> distinction was drawn (type.md, 2026-09-02) — the type says what content
> *is*, the manager says what to do with it. **`pane` is gone**: placement was
> a hint to a surface that owned a two-region layout, and M15 replaced that
> with the compositor's own allocation, so the surface no longer needs telling.

The demonstration that matters: clicking **Processes** opens `/proc` with `Kill
Note Ns` and a live pid list, drilling into `ctl · status · note · notepg` — and
**no process manager program exists anywhere.** There is a filesystem, a few
small files, and a surface.


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
- ~~The console window's shape~~ — **specified above**: a growing file the host tails,
  typed lines back as `insert`, con(1)'s mark arithmetic deleted.

**Nothing is open in this contract, and every half is built** — the device, the
host's chrome, the host's content, and the editor. The design's last named risk
(property 1 under a rich editor) is retired by observation, not argument.
