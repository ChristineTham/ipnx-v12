# /dev/canvas — the display protocol (v0)

The contract for the 2026-08-30 canvas decision (design.md: six kinds, four
clauses, the tripwire). **v0 is measured**: every element below is justified
by one of the four benchmarks — console-today, acme-today, rio-today, one
plot — and everything none of them demanded is absent. Dated simplifications
are recorded inline; each is a decision, not an accident.

*Revised 2026-08-30 after the benchmarks landed: the four consumers —
con(1), acme(1) (acme-today), `/rc/tile`, and the path plot — now run
against this contract on every host, and the spec below states the
protocol as built, additions dated in place. Revised again the same
day by the acme fidelity pass (the paper's examples as the yardstick):
one event (`select`) and one attr (`sel`) joined, each carried by a
measured need; both pass through the devices untouched, so neither
kernel changed.*

## The derivation (the measurement pass, 2026-08-30)

| benchmark | demands | and nothing else |
|---|---|---|
| **console-today** | one `edit` node (the transcript); insert/delete events with offsets and text (typing echo is presenter-local — the app *observes* edits, acme's discipline); addressed writes (append output, rewrite the input region); `label` | no key events — typed text arrives as edit events; arrows/scroll are presenter-local |
| **acme-today** | `stack` (columns and rows: orientation, order, proportion attrs); `edit` bodies and tags; `execute`/`look` events carrying node, range and the text under the verb | no pre-marked span roles in v0 — the event carries the target text and policy decides (span attrs arrive with the web presenter's real links, later) |
| **rio-today** | `resize` and `close` as events — the protocol lines that retire the demo's oldest deferral; windows themselves stay `#w`'s | no nested windowing in v0 (that is the remote-surface story) |
| **one plot** | `path` (SVG path data verbatim — adopt notation, own the model); `text` labels; a `viewbox` attr for coordinates; stroke/fill attrs | no gradients, no clipping, no transforms in v0 |

Kinds shipped in v0: **`stack · text · edit · path`**. The vocabulary's
remaining two stay declared but unimplemented until a benchmark demands
them: `frame` (pixels-in-a-grid, honestly opaque — video's home; the
raster `rgb` file is its ancestor) and `image`, which v0 provisionally
**folds into `frame`** (the decision's open question, resolved this way
until a consumer separates them). Four live kinds against the
dozen-kinds tripwire.

## The tree

A window's canvas is a flat directory of numbered nodes — acme's shape:
structure lives in attrs, not filesystem nesting, so the whole UI greps.

```
/dev/canvas/            (bind '#w/N' /dev makes this a window's)
  ctl                   new · del · sync · event
  events                the surface speaks: one line per event, reads park
  caps                  what the attached surface offers (ro)
  0/                    the root node, a stack, always present
  <id>/
    kind                stack | text | edit | path   (ro after new)
    attrs               k=v lines; write merges       (parent, order, …)
    addr                "q0,q1" byte offsets; $ is end (write)
    data                content: read the addr range, write replaces it
```

**ctl verbs** (one per line):
- `new <id> <kind>` — the app chooses ids (positive integers, fresh);
  the node lands under `parent=0` at the end unless attrs say otherwise.
- `del <id>` — the node and its subtree.
- `sync` — commit: the surface receives the tree atomically. Nothing is
  promised to render before sync.
- `event <line>` — the virtual-surface door: injects a line into
  `events` exactly as a surface would (the suite's user; the same
  house precedent as wctl's `type`).

**attrs** (v0 set as landed, all optional): `parent=<id>` `order=<n>`
(siblings sort by order, ties by id) · stack: `dir=col|row`; children of
a `row` share width by `prop` — `prop=0` hugs its content, any other
value is the flex share (default 1) · any content node: `bg=<colour>`
(acme-today was the consumer that earned styling its first attr; the
tags' `#eaffff` and bodies' `#ffffea` ride it) · path:
`viewbox="x y w h"` `stroke=<colour>` `fill=<colour>` `width=<n>` · any
node: `action=execute|look` (a whole-node role; the presenter renders
it honestly — a real link or button on the web) · edit:
`sel=<q0>,<q1>` (added 2026-08-30, the acme fidelity pass: the app
steers the surface's selection — byte offsets; the paper's "selects
line 112 and places the mouse there" demands it, and Undo, Look and
`file:27` all ride it. The surface scrolls the range into view; how
much keyboard focus follows is the surface's policy — the browser
presenter retargets only for a node new in that render, so `+Errors`
tails scroll without stealing the caret).

**addr/data** — acme's buffer interface, simplified for v0 (recorded
open: verbatim addr language returns with sam-today): `addr` accepts only
`q0,q1`, `q0` (empty selection) or `$` (end); byte offsets into UTF-8
content. Writing `data` **replaces the addressed range** and moves addr
to the insertion's end — so append is `$` then write, and the whole file
is `0,$` then write. Reading `data` returns the addressed range
(honouring the read offset within it). Every content kind speaks
addr/data — a path's data is its SVG path string, replaced the same way.

## Events

One line each, parked reads, oldest first. Text fields are %-quoted
(`%25` `%20` `%0A`) so a line stays a line and rc can read the file:

```
insert <id> <q0> <text>       the user typed/pasted into an edit node
delete <id> <q0> <q1>         the user removed a range
select <id> <q0> <q1>         the user's selection (or collapsed
                              caret) in an edit node changed — added
                              2026-08-30: Cut/Snarf/Paste-as-words and
                              the | < > selection filters need the app
                              to know dot. Not a mutation: the device
                              queues it untouched
execute <id> <q0> <q1> <text> the execute verb landed on this range
look <id> <q0> <q1> <text>    the look verb (tap) landed
resize 0 <w> <h>              the surface resized the window
close 0                       the surface asked to close (advisory:
                              the app exits, or does not; wctl delete
                              remains the imperative)
```

The app's own writes never echo back as events — events are the user.
**And a user edit event is a mutation that happened**: the device
applies `insert` and `delete` to the node's data *before* queueing the
event, so presenter echo and node data are one thing and typed-ahead
text survives the app's next sync. Apps shadow their buffers from
events and never re-read them — con(1)'s discipline, load-bearing.

**Transport**: `sync` marks the window dirty; the snapshot travels to an
interactive surface only under the host's credit system — one frame in
flight per window, acked after paint, the LATEST tree snapshotted at
flush time so superseded syncs coalesce away by construction
(coalescing changes the rate, only credit changes the bound). The
browser presenter's equivalent is one render per animation frame.

## Surfaces

`caps` reads what is attached: `virtual` (the headless host: syncs are
acknowledged, `event` is the user — the suite's surface, and the reason
pixel censuses retire) or `interactive input` from a live presenter. The browser presenter is the universal
SPA: stacks become flex containers, text becomes text, edit nodes accept
typing and report it as insert/delete events, paths become inline SVG,
`action` nodes become real links and buttons (the input convention's
web grammar). Layout resolution is the surface's; divergence between
surfaces is accepted and named. SwiftUI (M6) and VSCode panels (M13)
consume the same tree over their bridges.

## What v0 refuses, and where the rest lives

No markup crosses the boundary (the protocol speaks four kinds whatever
the app knows); no behaviour lives in the surface; no styling vocabulary
until a benchmark demands one, and if the kinds ever pass a dozen the
HTML refusal is re-litigated (the tripwire, design.md). The plumber's
message shape — where `look` lines go — is the next measurement's work;
v0 delivers the event and stops. Wire format for remote surfaces: the
tree is already files, so exportfs already ships it.
