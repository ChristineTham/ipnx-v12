# The userland, reimagined

The design record for the 2026-08-30 decision: *redesign sam, acme and the
rest of the Plan 9 utilities to use our new paradigms — it's time to kiss
compatibility goodbye.* The curation survives; the verbatim does not. Each
program's **essence** is named and carried into a native design on the new
paradigms — `/dev/canvas` (design.md 2026-08-30), the verb convention, the
namespace, pkg, and the plumber. The vendored raster world reclassifies as
the **heritage exhibit** beside `/v10`: still built, still run, still
holding the conformance floor — load-bearing for the suite, never for
design.

## The three classes

**1. Filters — already native.** `cat`, `grep`, `sed`, `sort`, `wc`, `tr`,
`ls`, `sum`, `look`, `split`, `cal`, and the rest of the pipe-and-file
world. Their paradigm is files and pipes, which *is* this system's
paradigm; there is nothing to redesign and no compatibility being kept —
they are simply correct. They stay as they are (vendored source and all;
lineage is not the issue, interface is), and get re-sourced only if a
concrete reason ever appears.

**2. Screen programs — redesigned onto canvas.** The whole raster stack
(libdraw, libframe, the wsys raster path, the 5620 grammar) evaporates
from the product and survives in the exhibit. The designs below.

**3. The support cast.** `win` becomes trivial (a canvas window IS a
namespace; the console design below absorbs its job). The plumber returns
to the centre. `pkg`, `pip`, `cc`, `go` are already native — they were
born here.

## The console: an editable transcript

The terminal emulator dies. A console is an `edit` node with a **prompt
discipline**: the transcript is one editable buffer; a mark separates
history from the input region; Enter in the input region sends the line;
everything above is ordinary editable, searchable, selectable text.
acme's `win(1)` was this design's prototype twenty years early. **Amended
2026-08-30 (design log): AND, not XOR** — the transcript is the native
console, and the xterm byte console stays beside it, because familiarity
is a door, not a debt. Consequences of the transcript, all free: infinite
scrollback that is just a buffer; copy/paste that is just selection;
search that is just the editor; no escape-sequence emulation in the
native design (programs that want structure have canvas; programs that
write bytes get a transcript — or the xterm door, their choice).
**Landed 2026-08-30 as `con(1)`** on the wasm libthread: three
coroutines, one canvas writer, shadow-state mark arithmetic, suite-
tested through the virtual surface.

## One editor: acme-today absorbs sam

Acme always contained sam — the `Edit` command speaks sam's language. So
the redesign completes the merger the originals gestured at:

- **acme-today** is a *policy client* of canvas: columns and rows are
  `stack` nodes, bodies are `edit` nodes, tags are text spans with
  `action=execute` (honest buttons at last), file names and addresses are
  `action=look` spans (the plumber activates). Layout, fonts, wrapping,
  scrolling, selection, IME: the presenter's, native on every surface.
  Its 9P file interface (`/mnt/acme`-shaped: `body`, `tag`, `addr`,
  `data`, `event`) is **kept and strengthened** — that interface was
  never raster; it is the part of acme that was always the future, and
  external tooling keeps working against it.
- **sam survives as a language and a filter**: the structural-regexp
  command language is the spec (acme-today's `Edit` implements it whole),
  and a batch CLI — working title `ed`'s true heir, `sam -d`'s job —
  applies it to files and edit nodes from scripts and the tour. The
  interactive raster sam retires to the exhibit with honours.
- Open question, deliberately: whether the command language is also
  exposed as a control file on every `edit` node (write commands, the
  node edits itself) — which would make "sam" a property of text in this
  system rather than a program. Still open leaving M5 — the language
  landed inside `edit(1)` first; the control-file form waits for a
  second consumer.
- **The Edit language returned 2026-08-30**, and the same day —
  Christine reading the acme paper beside the build — the workspace took
  **the paper's own shape**: a root tag over a ROW of columns, every tag
  one *editable* node whose **words execute** (alt-click or middle-click
  the word, on either presenter — acme's accelerator per the input
  convention). The tag's first word is the file name and `Put` writes to
  it (rename the word, Put follows); `Edit` takes the rest of its tag
  line as sam's command; `New` lands a window in the ACTIVE column (the
  paper's placement rule); `Newcol`/`Delcol`/`Putall`/`Exit` all live as
  tag words; `|` separates commands from scratch space, and scratch
  words execute too. Colours are acme's (#eaffff tags, #ffffea bodies)
  via the `bg` attr the canvas spec reserved for the first benchmark
  that demanded it. **Same day, her standard applied — "either acme
  works like acme, or it doesn't" — and the behaviour completed**:
  caret editing everywhere (click positions it, arrows and Home/End
  navigate, a B1 sweep is the native selection and typing replaces it,
  paste inserts at the caret); **sweep-execution with arguments** (the
  swept text's first word is the verb, the rest its arguments — `Edit
  ,x/…/c/…/` swept runs whole); **Look** (B3): in-window literal
  search jumps the caret presenter-side, and a path opens a window in
  the active column app-side; **the dirty box** (black beside the tag
  on edit, cleared by Put and Get); `Zerox`, `Putall`, `Dump`/`Load`.
  The one deliberate divergence, per the stamped input convention:
  the native clipboard IS snarf — Cut/Snarf/Paste are the platform's
  own gestures, not tag words. The web presenter is the reference;
  the Mac presenter renders and clicks today and takes the full
  editing pass with its platform milestone. Suite-proven end-to-file
  on every running host, columns included.
- **First slice landed 2026-08-30 as `edit(1)`**: the editor's shape on
  canvas — a tag row whose verbs are real nodes (the file name is
  `action=look` and re-reads from disk; Put and Del are `action=execute`
  and render as honest buttons on the web), an edit body shadowed from
  its events, the file round-trip suite-proven and verified by hand in
  the browser. Next slices: the Edit language (sam's return), multi-file
  columns, the plumber on look.

## rio-today: policy as a file server

rio-today owns *policy*, presenters own *pixels*. It is a userspace file
server: windows are namespaces (unchanged — the one rio idea that needed
no modernising), `wctl` stays text, layout policy (tiling, stacking,
focus) is files, and any program can be a window manager, recursion
included. The host presenter — the universal SPA in a browser, the
SwiftUI app on Apple platforms — renders frames and chrome natively and
feeds the `events` files. Resize and close are protocol lines, which is
what retires the demo's old deferral. **First proof landed 2026-08-30**:
the window server's root lists windows, and `/rc/tile` is a window
manager in a dozen lines of rc — any program can be one, a shell script
included.

## The plumber, central again

Every `look` is a plumb: the tap on a path, URL, error line, or
identifier becomes a typed message through rules — the file that decides
"what does looking at this mean here". The plumber is the verb
convention's engine and gets designed (rules format, message shape,
port namespace) as part of canvas v0's measurement pass, because
acme-today's right-click… — no: acme-today's *tap* — is unimplementable
without it.

## What the exhibit keeps

The vendored Plan 9 tree (sam, acme, samterm, libdraw, libframe, the
raster wsys path), the V10 binaries, and their tests. They build in every
world, run in every suite, and hold the floor. Nothing is deleted;
nothing gets a vote.

## Sequencing (mirrored in implementation.md, M5)

1. **Measure**: canvas v0 vocabulary derived from acme-today, the
   console, rio-today, one plot. The plumber's message shape rides along.
2. **Console-today + the universal SPA presenter** on the demo kernel —
   the first canvas client is the console, because everything else is
   tested through it.
3. **acme-today** (absorbing sam's language), then **rio-today** policy.
4. The Mac presenter maps the same tree to native views; iPadOS inherits.
