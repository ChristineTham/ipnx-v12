# The tiled compositor — one implementation of the window manager

**Role: a *what* — the tiled window manager.** How *this* implementation of
emca arranges windows. **It is one implementation, not the contract**: the
contract every implementation must honour is [window.md](window.md), and a
floating, overlapping or Stage-Manager-style implementation would replace
everything in this document while changing nothing above it (decision log,
2026-09-02).

**Duplicate renders as three buttons here** — *as column*, *as row*, *as tab* —
because those are this implementation's placements. The contract
([window.md](window.md)) names **duplicate** once; a floating implementation
would show one control and place the copy itself. Three is what emca renders
today, and may change (Christine, 2026-09-02).

Nothing here is visible to a type manager. A manager receives a rectangle and
a namespace; it never learns whether that rectangle came from the allocation
below, from a window someone dragged, or from a grouping gesture.

Every window is a compositor, and it runs on itself. Given its own
rectangle it decides whether to hold a body or to divide along an
axis, allocating rectangles to some children and leaving the rest as
tabs; each allocated child then does the same. The recursion has no
floor and no ceiling.

THE ROOT WINDOW IS THE SCREEN. The whole browser page — or the macOS
window, or the iPad screen — is one window of type root. It is not a
frame around the windows; it IS a window, and everything visible is
its descendants. So "the layout" is not a thing the surface owns with
windows sitting inside it: the layout is what the root window decided
when it composited itself.

This is the piece that should have been designed first. Every symptom
of getting it wrong was a symptom of the same thing — named fixed
regions. The first implementation had a PANES map with "left", "main"
and "bottom" in it, a `pane <name>` verb, and a `pane` file per type
naming one of those three strings. Once placement is a NAME, the
composition is frozen at three slots, nothing nests, and rows and
columns are decoration. Windows that could not split, a rail that was
really region #1, a bottom pane that existed because a string was
declared: one fault, many symptoms.

### Panes are windows
THE CORRECTION THAT MATTERS MOST, and the one whose absence produced
the names "rail" and "transcript": panes are not special. They are
ordinary windows.

The root window creates them as a LAYOUT CONVENTION and then forgets
they were ever special. Given room it divides into three columns — a
left pane, a main window, a right pane — and the main window divides
itself into two rows. Every one of those is a window: four components,
three controls, closeable, minimisable, maximisable, tabbable and
divisible, and nothing in the system records that it was created by a
convention rather than by a person.

So there is no pane type, no PANES table, no reserved names. A "pane"
is a sentence about how a window came to exist, not a thing.

### The division of labour
```
EMCA OWNS THE TREE. Which windows exist, how they nest, their order,
which are allocated and which are tabs, and the allocations
themselves. It is workspace state, so by the founding test it is
IPNX's.

THE HOST RENDERS THE TREE, and reports geometry.
```

Hers, exactly: "Host tells emca - we have a 800x1024 window. emca says
'Ok, we need to apply this responsive layout' create these windows.
Host says 'Ok'. emca owns the tree, and the host renders the tree."

And the reason emca does the arithmetic rather than the host: "emca
needs to understand the geometry as well, so it does not ask the host
impossible things." A tree that cannot fit is not a layout.

This AMENDS a line in PART ONE — "emca never learns the viewport's
width". It does now, and must.

A gain falls out of it: the responsive rules become TESTABLE
HEADLESSLY. rc can write a geometry and read back the resulting tree,
which nothing could do while the calculation lived in the surface.

### The unit, and why it is not characters
The unit is DEVICE-INDEPENDENT PIXELS, and the host additionally
reports THE TEXT CELL SIZE in those units.

An earlier draft measured breakpoints in characters, for a good reason
that still holds: a pixel breakpoint breaks under accessibility text
sizing. That reason is preserved rather than abandoned — 72 columns is
still the leaf measure, computed by emca as 72 x cellWidth, so raising
the user's text size enlarges the reported cell and the breakpoints
move for free. WCAG 1.4.4 holds by construction.

But characters cannot be the UNIT, because not every window is text.
Hers: emca "needs to be able to know how to fit an image into a
window. It doesn't actually render the image, but it knows what an
image is (or video, or postscript etc) and what aspect ratio is so it
can inform the host to allocate enough space."

THIS IS WHERE EMCA DEPARTS FROM ACME RATHER THAN COMPLETING IT. Acme
could measure in characters because everything was text; a column was
a stack of text bodies, and aspect ratio never came up. The moment a
window can hold an image, a video or PostScript, the compositor needs
a geometric space with ratios in it. Christine's observation names the
precedent: "this is why macos uses postscript underlying" — NeXTSTEP's
Display PostScript and macOS's Quartz are device-independent imaging
models for exactly this reason. emca inherits the problem and takes
the same answer.

```
host -> emca   resize <w> <h> <cellw> <cellh>
emca -> host   the tree: who is allocated what, and who is a tab
host           renders it
```

INTRINSIC SIZE. To allocate for a picture emca needs its natural
dimensions, so it reads them from the file header — PNG IHDR, JPEG
SOF, GIF, SVG viewBox. That is what "knows what an image is" means in
practice, and it is bounded work: a header, never a decode. For
anything it cannot parse — video, PostScript, an unknown format — it
lays out with a stated default aspect and the host corrects it with a
`size <w> <h>` event once it has actually decoded the thing. Layout
never blocks on a decode, and emca is never wrong for long.

### Rows and columns: alternation, enforced
A container's axis is ALWAYS PERPENDICULAR TO ITS PARENT'S. A
container created with the same axis as its parent is FLATTENED into
it.

The axis is STORED, not derived from depth. Deriving it is tempting —
it would make the axis not a property at all — but it does not survive
restructuring: inserting a level above a window shifts every
descendant's depth by one and so flips every axis in the subtree. The
invariant gives the same canonical form without that fragility.

Canonical form is the point. Under free axes a column holding a column
renders identically to one flattened column, so one visible layout has
many possible trees — which breaks Dump/Load round-tripping, breaks
the suite asserting on the tree, and breaks emca and the host agreeing
on what the tree IS. With alternation, one layout, one tree.

WHAT THIS MEANS FOR "SPLIT THIS WINDOW", which is where it gets
counter-intuitive and is worth stating carefully. A window does not
have a free choice of axis: it already IS a row or a column, decided
by its parent. So:

```
the parent is a ROW      -> "split into rows" gives the window
container (window is a      row CHILDREN. Ordinary nesting.
column)

the parent is a COLUMN   -> "split into columns" ADDS A SIBLING.
container (window is        The window keeps what it has; a new
already a column)           column appears beside it.
```

Christine's worry, and it is the right one to have: "we may start with
a window with rows, and then we create a column... the column will
always be on a child window, and not of the window itself." Correct —
and the resolution is that the window is ALREADY a column, so it gains
a neighbour rather than an interior. Nothing is inserted and nothing
re-orients.

THE ROOT has no parent to be perpendicular to, so it picks its axis by
convention: COLUMNS. A rows-only layout is therefore one column
holding rows — canonical, not a special case.

And the verbs stay two, because the user is stating a DIRECTION: `New
row` and `New column`. Whether emca answers by giving the window
children or by giving it a sibling is worked out from the parent's
axis and never asked about.

### Sizing: automatic, and content-aware
Acme promises reasonable initial sizes so that resizing is optional
and by taste. Its rule is more interesting than "split in half", and
the interesting part is what emca keeps. From cols.c's coladd():

```
if(y<r.min.y && c->nw>0){        /* steal half of last window */
    v = c->w[c->nw-1];
    y = v->body.r.min.y+Dy(v->body.r)/2;
}
if(!c->safe || v->body.maxlines<=3){
    colgrow(c, v, 1);            /* if v is too small, grow it */
```

and then the line that does the work:

```
r1.max.y = min(y, v->body.r.min.y + v->body.nlines*v->body.font->height);
```

The victim shrinks to THE SMALLER OF half its height and the space its
content actually occupies. A window holding five lines gives up far
more than half; a full one gives up exactly half; one already down to
three lines is grown before being split rather than sliced thinner.
"Automatic and reasonable" comes from being CONTENT-AWARE, not
fractional.

emca generalises it, because emca knows content size for every window
kind rather than only for text that happens to be loaded. Every window
declares two numbers along its parent's axis:

```
MINIMUM   its furniture — title, toolbar, status, about three lines
          — plus a floor of body, or a picture's minimum useful size
NATURAL   what the content wants: line count for a file, entry count
          for a listing, width/aspect for an image, a sensible
          default for a shell
```

Allocation is then three steps:

```
1. every allocated child gets its MINIMUM
2. the remainder is shared in proportion to (NATURAL - MINIMUM),
   capped at NATURAL
3. anything still left over — every window already at its natural —
   is shared EQUALLY, which is what "given to nobody in particular"
   has to mean if it is to be implementable. Privileging one window
   is the kind of help that reads as the layout fighting you.

and if the MINIMUMS do not fit, the excess become TABS. Short
columns need no separate rule; the allocation model already covers
them.
```

Acme's heuristic then FALLS OUT rather than being ported: a new window
takes space from its siblings in proportion to their SLACK, and a
sibling with little content has a lot of slack while a full one has
none.

TWO THINGS THAT WOULD OTHERWISE SURPRISE US:

A user resize is an OVERRIDE, remembered as that window's natural size
until reset. That is what keeps "resizing is optional and by taste"
true — the automatic rule goes on running for every window nobody has
touched.

Images make natural size CIRCULAR, since an image's natural height
depends on the width it is given. It resolves by doing the axes in
order — allocate along the parent's axis first, then compute naturals
for the perpendicular one — and it is the reason emca needs an aspect
ratio rather than a line count.

### Fit and Reset
Two builtins, and neither is a new mechanism: both discard what was
overridden and re-derive.

```
FIT     keep the structure, drop the SIZE overrides in this window's
        subtree, re-run the allocation. Non-destructive. On every
        window's toolbar, and OPERAND DETERMINES SURFACE scopes it
        for free: press it on a column and that column's rows
        re-fit, press it on the root and everything does, press it
        on a childless window and it drops that window's own
        override. One verb, no scope argument to choose.

RESET   discard the STRUCTURE too and rebuild from the convention.
        Destructive, and it is what brings back panes that were
        closed. It belongs to THE ROOT WINDOW'S TYPE, not to the
        core set, because only the root has a default to return to —
        an arbitrary column has no canonical arrangement, so Reset
        on it would either mean nothing or mean Fit. Type verbs are
        the type's; this is one, exactly as Kill belongs to proc.
```

Two things these buy beyond the obvious. They make the automatic rule
SAFE TO BE CONSERVATIVE: because there is always a cheap way back,
emca can respect a resize indefinitely rather than second-guessing it
later. And they give the suite a far stronger assertion than a boot
layout can — Fit is idempotent and deterministic, so a test may resize
a tree arbitrarily, press Fit, and assert the exact allocation. That
is only testable because emca owns the geometry.

### Responsive rules, as the root window compositing itself
The rules are unchanged in substance; what changes is that they are
not a special case in the surface. They are what the ROOT WINDOW does
when it runs the compositor on itself, and every descendant applies
the same rules to its own rectangle.

```
a leaf needs   >= 72 columns  (72 x cellWidth)
a body needs   >= 10 lines + its furniture to be worth allocating
a picture needs its aspect ratio and a stated minimum

breakpoint   the root window divides itself into
----------   -----------------------------------------------
small        one column; one child allocated, the rest tabs
medium       two columns, or one column of two
large        THREE COLUMNS — and the middle one divides itself
             into two rows
xlarge       three or more columns, each dividing further
```

At large this is the arrangement people will recognise: a column
either side and a taller one between, whose lower row is where a shell
usually sits. It is worth being explicit that this is A CONVENTION THE
ROOT WINDOW FOLLOWS, not a structure the system knows about. Nothing
downstream can tell those three columns from any others.

THE RESPONSIVE INVARIANT: nothing disappears, in either direction. A
window that will not fit BECOMES A TAB — still there, still whole,
one click away. The first implementation read "small: no panes" as
"hide them", and deleted the home listing and the console outright:
the exact information loss this invariant forbids.

And the small breakpoint stops being a special case. It is not a
"concertina", a word this document should not have needed: it is the
ordinary allocation rule with room for one rectangle. The same
mechanism a person drives with the minimise button is the one the
compositor uses when space runs out, which is why the compositor can
no longer delete a window by accident — there is no separate path for
it to take.

Which is the general shape of this design, and worth stating plainly
because it is what the first attempt got wrong everywhere: THE DESIGN
IS NOT A SET OF EXCEPTIONS. It is a few principles applied
consistently. One object, composited recursively. Controls that inform
the parent. Allocation, with the unallocated as tabs. A toolbar of
verbs with the tag line as their argument. Every window the same.
Every mistake so far — the named panes, the rare-verb overflow, the
pin, the single global status line, the rotated minimised strip — was
an exception invented where a principle already applied. (And one, the
floating bar, was RETIRED in error, restored, and then correctly
DEMOTED: the verbs a selection affords are IPNX's, but where they
appear is the surface's, and the empty-tag-line rule means no surface
has to draw anything.)

Defaults and overrides: the breakpoint sets the DEFAULT allocation; an
explicit act overrides it; the override is remembered PER SIZE CLASS.
Without that last clause, resizing silently untabs a window someone
deliberately put away, or puts away one they deliberately opened.

### What this deletes
```
- the PANES map, and named regions entirely
- `pane <name>` in wctl, and /type/<t>/pane
- "rail", "leaf", "bottom pane" and "concertina" as distinct things
- "stack" as a third composition beside row and column
- the pin: "alice in the tag line, then press Run" is
  execute-with-argument, so the pin solved a problem the tag line
  already solves
- the single global status line as the only status: every window has
  one, and the root window's is the workspace's
- the overflow menu, and the idea behind it that some window verbs
  are rare enough to hide. They are the interface. A toolbar wraps
  when the window is narrow — geometry answering geometry, not a
  judgement about how often a verb is wanted
- the rotated minimised strip, and with it the questions it raised:
  a title too cramped to edit, a single button standing for three,
  a close control made unreachable. A tab is a whole window, so
  none of them arise
```

### The column tag, restored
An earlier draft claimed the operand rule "DISSOLVES acme's column
tag" and cited that as evidence the rule was load-bearing. That was
wrong, and for the same reason as the rest: it assumed a column was
not a window. A column IS a window, so it has a tag like every other
window, and operand-determines-surface puts the column's own verbs on
it. The rule is not dissolving the column tag — the rule is what PUTS
it there. New and Sort belong to the column because the column is
their operand, and Delcol is just that window's close control
informing ITS parent.

### Editing is the surface's — emca is not an editor
Her direction (2026-08-31), which is the split taken to its
conclusion: "emca doesn't really need to implement a WYSIWYG editor on
the IPNX side. It can implement sam, a batch editor. The job of emca
is to push a file into a window via /dev/canvas. The host side can
display and scroll the file, and more importantly edit it using Monaco
or TextEdit or similar." And then, extending it: "selecting, copying,
pasting can all be host side operations, with sync to /dev/snarf" —
"even command history and shell command line edit — host implemented"
— "we can even bring xterm/js back".

THE TEST ALREADY SETTLED IT AND THE FIRST DRAFT DID NOT FOLLOW IT
THROUGH. Does editing behaviour differ between a Mac and an iPad?
Profoundly — TextKit is not Monaco is not a hidden input proxy. So
editing is the surface's, and the earlier draft's "the surface owns
text input" was the same rule applied only as far as the caret.

```
IPNX KEEPS                     THE SURFACE TAKES
----------------------------   ---------------------------------
the buffer (authoritative,     the caret, the selection, insertion
  shadowed from events)        and deletion, keystroke undo
the file: Get, Put             syntax highlighting, folding,
commands, +Errors, | < >         multi-cursor, find-in-file
SAM'S STRUCTURAL LANGUAGE:     IME, autocorrect, spell-check,
  x y g v, s//, addresses        dictation, accessibility
context (the directory)        the clipboard, synced to /dev/snarf
what the verbs MEAN            COMMAND HISTORY and line editing
the file interface             TERMINAL EMULATION (xterm.js)
```

So emca is a file server with a workspace, not an editor. It pushes
text into windows, runs commands, and applies transformations. The
editing is somebody else's, natively, on every surface.

WHY THIS IS "BEST OF BOTH WORLDS" AND NOT A CONCESSION: Monaco and sam
are COMPLEMENTARY, not competing. Monaco is interactive, local and
cursor-scoped; sam is batch, structural and file-scoped — x/re/ over
every match, s//repl/ through an address, transformations no cursor
can express. Every other system has one or the other, or bolts a macro
language onto an editor that owns the buffer. Here the buffer is a
FILE and the surface is a VIEW, so the two compose by construction.
Nobody has shipped both halves well together because nobody else had
this boundary.

WHAT THIS RETIRES, none of it to be written: the presenter's
hand-rolled caret; guest-side line editing and readline; escape-
sequence handling for history; syntax highlighting, folding,
multi-cursor and find-in-file. Inherited, not implemented.

AND IT RECONCILES THE CONSOLE'S "AND, NOT XOR" (userland.md,
2026-08-30 — the transcript console AND the xterm byte console,
"because familiarity is a door, not a debt"). Under this architecture
that stops being a compromise and becomes the consistent answer: the
TRANSCRIPT is the line-oriented door, where history and line editing
are the host's; XTERM.JS IS THE RAW-INPUT DOOR, for programs that want
keystrokes rather than lines. Two doors, one principle, and the
architecture explains why both are needed.

### Text input and the soft keyboard
THE SURFACE OWNS TEXT INPUT NATIVELY. A native text view on SwiftUI;
Monaco or a native text view on the web. The presenter's hand-rolled
caret is DELETED.

This is a prerequisite, not a refinement: the hand-rolled caret never
summons the iOS on-screen keyboard, because nothing is focusable-for-
input, so touch typing is impossible without it.

```
- the surface owns caret, selection, IME and autocorrect, and
  generates insert/delete/select events from native input callbacks
  rather than from synthesised keystrokes
- emca's shadow-buffer discipline is UNCHANGED — it already only
  observes edits and never re-reads (con(1)'s discipline)
- the sel attr still steers, and still retargets focus only for a
  node new in that render
- AUTOCORRECT AND SMART PUNCTUATION OFF in body and tag bar. Smart
  quotes in source code are a defect, not a nicety.
```

### Single tap, and when two are needed
Acme's B2/B3 click is ONE action. Select-then-tap is two, which is
fine for rare operations and bad for the most common one in the
system: tapping a filename in a listing to open it.

TYPE DRIVES INTERACTIVITY, and that is what buys the tap back:

```
dir, errors, shell, proc,        every LINE is a look target;
pkg, project, usr, net, type     one tap (role=look on the line)
tag bar, toolbar                 words are buttons; one tap
file, scratch                    select-then-tap; two acts
```

The split is honest: structured output is one tap, arbitrary prose is
two. Acme charged one tap everywhere and paid for it with a mouse
button nobody could find.

### The keyboard grammar
Keyboard-complete is a stated law (WCAG 2.1.1, the input convention),
and iPad-with-keyboard and Mac are primary surfaces.

```
THE RULE: every floating-bar verb and every toolbar button has a
shortcut, and Tab reaches everything. No verb is keyboard-
unreachable.

cut / copy / paste            ⌘X ⌘C ⌘V
undo / redo                   ⌘Z ⌘⇧Z
Put / Putall                  ⌘S ⌘⌥S
Del                           ⌘W
New / split                   ⌘N ⌘\
search                        ⌘F
EXECUTE selection or word     ⌘↵    (the input convention's acme
                                     tempo, already decided)
LOOK selection or word        ⌘⇧↵   (symmetric — the two acme
                                     verbs paired)
pin the selection             ⌘⌥↵
focus the tag bar             ⌘T
focus the global tag          ⌘⇧T
toggle rail / bottom / right  ⌘B ⌘J ⌘⌥B
nth window in leaf            ⌘1..9
next window / next leaf       ⌃⇥  ⌘⌥→ ←
```

On iPad, hold-⌘ shows the HUD.

### Focus
One focused window per workspace; its leaf is the focused leaf. Focus
follows EXPLICIT interaction only — tap, click, keyboard. Never
hover. Never the app, except through a show request, and even then
the surface decides whether to take the keyboard. That is the
+Errors lesson generalised: an output tail scrolls into view and
never steals the caret.
