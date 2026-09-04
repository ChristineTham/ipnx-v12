# emca — the windowing system and user interface

**Role: a *what* — emca the role.** What a window is here, and which half of
the system owns which part of it. **emca is a role, not an implementation** —
the way "window manager" is a role on X — so this document holds only what is
true of every implementation, and points at the rest.

| | |
|---|---|
| the **contract** every implementation must honour | [window.md](window.md) |
| the **tiled implementation** we designed and built | [compositor.md](compositor.md) |
| what the **surface** owns — chrome, theming, the devices | [surface.md](surface.md) |
| the **type system** — what content is, and what manages it | [type.md](type.md) |
| the **acme port** — an emca-like program running as a client | [acme.md](acme.md) |
| the layers and the names | [saranos.md](saranos.md) |
| why each shape was chosen | [design.md](design.md) |
| what is built, and what is not | [when.md](when.md) |

**emca does the minimum possible, like rio.** It places windows, it handles
resizes, and it hands each manager a rectangle and a namespace. It does not
reach inside. That minimum is what lets the windowing style be replaced —
tiled, overlapping, Stage Manager — without changing a single manager or type.

## The agreed design

Confirmed by Christine, 2026-09-02, and stated here in her terms. **This is the
endorsed baseline** — everything outside it is a proposal awaiting review or a
gap awaiting a proposal (CLAUDE.md's Conventions). Where the build deviates,
the build is wrong.

**1. emca is a windowing system and user interface.**

**2. The surface CONTAINS the root window — it is not itself an emca window.**
An earlier statement had the browser window, the iPad screen and the macOS
window each *being* a window of type `root`. Three attempts to make that
consistent each produced a worse exception (a sliver body, an undecorated pane,
two title bars), and when every route around an exception creates one, **the
exception is real** (Christine, 2026-09-02).

**The outermost window is special in exactly one respect: its chrome is the
host's.** The title, the window controls and the toolbar belong to the platform
— the macOS title bar and menu bar, the browser's tab and its own toolbar, the
iPad's furniture. Everything else about it is ordinary: **its body is children**,
like any container, and allocation, alternation and the tree are untouched.

**The exception is the OUTERMOST SURFACE, not the type.** `inode/system` is an
ordinary type. A nested emca opens *its* `/` with the same manager and gets
**ordinary emca chrome**, because its parent is an emca window rather than the
host — otherwise every nested system would try to claim the host's title bar and
nesting would break on first use.

**3. A window decomposes into child windows, laid out in columns, rows and
tabs — in that order.** Columns hold rows; rows hold tabs. Tabs are not a third
axis but the un-allocated remainder: a parent gives rectangles to some children
and the rest are tabs, which is why *a tabbed window is a maximised one*.

**4. A window has these properties:**

| property | |
|---|---|
| **type** | which type it is; fully specified in `/type/<x>` |
| **id** | the window's identity — a file |
| **behaviour** | its window controls, toolbar, and what they do |
| **content** | what it holds |
| **status line** | what the window reports about itself |

**5. And a specific look and feel, in this order:**

```
title and window controls
tag line and toolbar
content
status line
```

**6. A type `x` is fully specified in `/type/x`** — a folder of configuration
files, plus an associated **manager**. The manager **may live on both the host
and the IPNX side**, and is responsible for rendering the content, editing the
content, providing toolbar buttons, and supplying the semantics of the standard
buttons — Edit, Find and the rest. See [type.md](type.md).

### Not yet defined

- **The window type specification itself** — what a type declares, in what
  files, and what a manager's interface is. The manager is agreed to be
  necessary and its *responsibilities* are agreed; its **definition is not**.
- **Every potential window type** — `root`, `ls`, `edit`, `shell`, `output`,
  `pkg`, `project` are named and **all undesigned**
  ([implementation.md](implementation.md)).

### The acceptance test
"acme intent" stated so it can be checked. Three properties, derived
from the anatomy. A surface that keeps all three delivers acme
whatever it looks like; one that drops any is a different system
wearing the name.

```
1. ANY TEXT CAN BE A VERB'S OPERAND — output, tags, listings alike.
   No read-only regions, no privileged text.
2. CONTEXT IS LOCATION — a command means what the window's
   directory says it means. Acme's own invention (Pike: "the
   context-dependent properties of execution and searching"), and
   the one most easily lost, because no native furniture on any
   platform has a notion of a working directory.
3. THE WORKING SET IS VISIBLE — you see what you are working on,
   not one thing at a time.
```

Property 3 degrades by viewport and that is accepted: at the smallest
size every window's TAG stays visible though only one BODY does.
Reduced, not abandoned. A tabbed interface would abandon it, which is
why emca has no tabs at any size.

WHY PROPERTY 1 SURVIVES A SYSTEM UI, which is the claim this whole
design rests on: a rich system interface normally destroys it. A
process manager with sortable columns is a native table, and a native
table is not text you can sweep — you gain a manager and lose acme.
In IPNX that trade does not arise, BECAUSE THE MANAGERS ARE ALREADY
TEXT FILESYSTEMS. /proc/1/status IS text. A process window is a text
window that happens to carry Kill in its toolbar. The property holds
by construction, not by discipline.

That is why this is an IPNX design and not a portable one. On a
system where the process table is a syscall, you must invent a widget
toolkit and acme dies inside it. Here the filesystem did the work in
1990.

## The split: which half does what

Her instruction (2026-08-31): "I think you need to split emca design
into two halves — a half that lives in IPNX, and a half that is
native to the surface."

### The line, and the test for it
```
IPNX owns STATE, MEANING and POLICY.
The surface owns RENDERING and INPUT.

If the answer differs between a Mac and an iPad — the surface's.
If it differs between one workspace and another — IPNX's.
```

Worked: "how many bodies are expanded" differs by device, so it is
the surface's. "Which windows exist" differs by workspace, so it is
IPNX's. "What Open means when you tap a path" is identical on every
device and differs per namespace — IPNX's. "Where the credential
decryption key lives" differs by host (Keychain, WebCrypto, a
passphrase) — the surface's.

### The half that lives in IPNX
```
- the buffers and their text; every edit, every undo record
- the window set: which windows exist, their names, their types
- the tag, as ONE string per window
- context: each window's directory, and every name resolved
  against it
- what every verb MEANS: execute forks a process, look resolves a
  path or an address, the filters pipe dot
- which verbs APPLY to a given range (the plumber's judgement)
- the file interface: /dev/window (a manager's own) and
  /dev/emca/<n>/ (the full set emca serves)
- the running-command table (a view of /proc)
- Dump and Load: the window set
- THE TREE, AND THE COMPOSITION: which windows exist, how they nest,
  their order, and each one's allocation (PART FIVE). Layout is
  workspace state, so it is emca's — and emca must do the geometry
  or it would ask the host for arrangements that cannot fit.
```

### The half that is native to the surface
```
- RENDERING the tree emca computed, and REPORTING GEOMETRY back to
  it: the viewport, and the text cell size in the same unit
- all furniture: window toolbars, the tag line's field, the status
  bar, window controls, scrollbars
- all input: taps, clicks, gestures, keyboard shortcuts, the soft
  keyboard, IME, autocorrect policy
- the caret and the selection, natively owned
- key custody for credentials
- which of the applicable verbs it shows, and where it puts them
```

Consequences, stated so they are not discovered later:

```
- The surface may be replaced entirely without touching emca. The
  SwiftUI surface and the web SPA are peers; divergence between
  them is accepted and named (the input convention, design.md
  2026-08-30).
- emca never learns what a button is, and never will.
- GEOMETRY IS NOT SAVED. Acme's Dump saved a layout; emca's saves
  the window set, because layout is derived from the viewport and
  cannot be restored onto a different one. A dated simplification,
  and a real loss on a single-device workflow.
```

### The channels
Her correction, which stopped a build going the wrong way: "You are
trying to push everything through a protocol that should have been an
exception rather than the rule." And the test the result had to pass:
"This is the only solution that fits the principle (everything is a
file, per process namespace, and 9P is the only protocol)."

FOUR SEMANTICS OVER ONE PROTOCOL. These are not four protocols —
everything is 9P; what differs is what the files mean.

```
9P, directly        THE FILE ITSELF. emca names a path; the host
                    mounts it and RENDERS IT NATIVELY — text, SVG,
                    HTML, Markdown, PostScript, images. IPNX
                    IMPLEMENTS NO RENDERERS AT ALL.
/dev/window/        THE CONTROL INTERFACE, bidirectional, per
  <type>/<n>/       window. IPNX declares the chrome; the host
                    reports what the user did. The TYPE IS IN THE
                    PATH, as /net/tcp/0 differs from /net/udp/0.
                    Specified in docs/window.md.
/type               the registry BOTH SIDES READ: what types exist,
                    what IPNX command drives each, what that type's
                    exchange means.
/dev/canvas         GENUINE DRAWING ONLY — open a rectangle, draw a
                    circle, place text. The exception, for programs
                    that actually draw. Narrowed; docs/canvas.md.
```

THE FRAMING ERROR THIS CORRECTS: canvas.md called itself "the display
protocol", but 9P is the only protocol. Naming it one invited treating
it as the place all host/IPNX exchange happens, and it grew until it
carried layout, chrome, text and drawing. It was always files.

AND THE MEASUREMENT RECORD SHOWS WHERE: canvas v0 was derived from
four benchmarks — console-today is a text file plus a line
back-channel; acme-today is a file plus chrome; rio-today is window
management; ONLY THE PLOT WAS DRAWING. Three of four were never canvas
consumers, which is why frame and image never found one either.

Editing and Put: the host EDITS and holds a mirror; emca holds the
authoritative buffer (decided below, 35). On Save the host streams the
edited file back to IPNX OVER 9P — an ordinary write, no new
mechanism, and it doubles as a resync. Dirty state needs no channel of
its own: emca knows the buffer and knows what was last written.

What this retires, none of it to be written: canvas's stack, text and
edit kinds; the shadow-buffer discipline FOR DISPLAY; every renderer
emca would have needed; most of the display machinery; and the role=
and type= canvas attrs specified earlier the same day, which belong on
/dev/window.

## The world as files

Her statement of the principle (2026-08-31):

```
"Traditional Unix and Linux manages different types differently,
 using separate commands. ps list processes, there are separate
 commands to manage network connections, packages, users, etc. In
 IPNX everything is managed as a file. That's what makes emca
 work."
```

There is no process manager program, no package manager GUI, no
network settings panel. There is a filesystem, a type that says which
verbs its files accept, and a surface that renders those verbs
natively. ADDING A MANAGER TO THE SYSTEM IS ADDING A FILE.

This is more Plan 9 than acme ever was: acme made ITSELF a file
server; emca makes the whole system's UI a view over the file servers
that already exist.

### The roots
```
/proc      processes — the process manager
/dev       devices
/net       network connections
/srv       posted services
/env       environment
/usr       users: /home/{profile,home,credentials,project}
/pkg       packages: toolchains, commands, libraries
/project   project templates and workspaces
/type      window types — the registry emca reads
/lib /bin /tmp /mnt /etc /rc      as they are
```

Note on /usr: this is PLAN 9's /usr — home directories, /usr/glenda —
not Unix's retconned "Unix System Resources" holding /usr/bin. Every
Linux-shaped reader will assume the other.

### The naming convention, measured
Measured across the vendored Plan 9 source: sixteen root names, NOT
ONE over four characters. lib bin tmp dev usr mnt etc sys srv env
(3), proc acme draw (4), rc fd n (1-2). acme and draw look like
exceptions and are not — they are words that already fitted.

```
THE RULE: a root name is at most four characters. Abbreviation is
what you do when the word is longer. A directory is named for what
one of its ENTRIES IS, not for the collection — /proc/17 is a proc,
/pkg/go is a pkg — which is why Unix never wrote /procs or /devs.
```

Expanding everything was refused, and not on taste: "modern software
must run" (CPython, Go and git carry hardcoded /tmp, /dev/null, /bin,
/lib, /usr) and "vendored sources are verbatim — never edit them"
(571 references to /lib, 139 to /bin in source committed never to be
touched).

```
THE ONE EXCEPTION: /project, seven characters. Recorded with its
reason so it constrains rather than licenses — /proj is one
keystroke from /proc and adjacent in meaning (templates for
processes beside running processes), ambiguous under tab
completion. A future root claiming this exception must show the
same: a real collision, not merely a longer word.
```

The naming search that produced it, so it is not redone: recipe (6,
too long), rec (opaque), menu (32 uses in the UI sense in these very
documents, and the design's founding quote is "Acme doesn't need
menus"), kit and app (overloaded), spec (geeky), proj (collides with
/proc). The concept split in two and the agony ended — /pkg was
always the ingredients and /project always the dish; "recipe" was
trying to be both.

## Window types

**Moved to [type.md](type.md).** A window type is a folder of text files
declaring a namespace, a manager and a toolbar; the manager is what renders and
edits the content, drives the status line and gives meaning to Find, Edit and
selection. That is a different subject from the windowing system, so it has its
own specification.

The one thing this document depends on: **the type is what the content IS, the
manager is what handles it**, and a manager may live on either side — Monaco is
a host-side manager over a file, which is all that "editing is the surface's"
ever meant.

## The window

There is ONE object, and it is the window.

```
A WINDOW IS A RECTANGLE WITH A TAG, CONTAINING EITHER A BODY OR
CHILD WINDOWS.
```

That is the whole structure. A "pane", a "column", a "leaf", a "rail",
a "tab" and a "tab strip" are not kinds — they are windows, or
arrangements of windows, doing what every window does.

### Where this comes from: acme's own source
An earlier draft of this document called acme "three fixed levels, not
recursive", and treated Row and Column as containers of a DIFFERENT
kind from Window. That was wrong, and acme's own dat.h says so — all
three have the same shape:

```
Row     { Rectangle r; Text tag; Column **col; }
Column  { Rectangle r; Text tag; Row *row; Window **w; }
Window  { Text tag; Text body; Rectangle r; Column *col; }
```

A rectangle, a tag, and either children or a body. And cols.c's
colcloseall() closes a column exactly as a window closes — textclose()
on its tag, then winclose() on everything it holds:

```
colcloseall(Column *c)
{
    textclose(&c->tag);
    for(i=0; i<c->nw; i++)
        winclose(c->w[i]);
    ...
}
```

So an acme column IS a window: one whose content is windows instead of
text, and closing it closes what is inside. Acme stops at depth three.
THAT WAS THE IMPLEMENTATION, NOT THE CONCEPT — the object was already
uniform, and nothing in it required the stop.

Christine's correction, which is the design: "We need a compositor -
something that arranges windows into columns and rows, and it is
recursive. each window itself is a compositor that can further
decompose into windows. A window effectively is an object that knows
how to break itself into columns and rows. It runs the compositor on
itself."

### What every window has
Four components, stacked, and EVERY window has all four whatever it
holds — a file, a listing, a shell, an image, or other windows.

```
+---------------------------------------------------+
| (o)(-)(+)  .../kitty/README                       |  controls, title
+---------------------------------------------------+
| mk test  [New][Open][Run][Find][Edit][Add]  | [Sav |  tag line, ITS
|          e][Revert][Undo][Redo]                   |  buttons | toolbar
+---------------------------------------------------+
|                                                 | |
|  the body, or child windows                     |^|  content,
|                                                 | |  scrollbars
+---------------------------------------------------+
| 12:4    2.1 KB    modified                        |  status bar
+---------------------------------------------------+
```

THE SECOND ROW READS LEFT TO RIGHT AS OPERAND, THEN VERBS THAT CONSUME
IT, THEN A SEPARATOR, THEN VERBS THAT DO NOT. The tag line comes first
because its text is the operand; its six buttons follow because they
act on that text; the rule of ink marks the boundary; the toolbar's
type verbs sit beyond it because the tag line is nothing to them.

That separator is the only place in the design where a line of ink
carries meaning, and it earns it: without it a person cannot see which
buttons will consume what they just typed. It is
operand-determines-surface made visible inside a single row.

```
ID        unique, and it is a PATH: /dev/window/shell/1. The type is
          in the path, so the id names the kind and the instance.

TITLE     designed to LOOK LIKE AN ORDINARY GUI TITLE BAR — coloured
          control buttons, then the name. Three behaviours:

          - EDITABLE, and editing it RETARGETS the window: it is
            replaced by whatever the new title names, and the type
            may change with it. Retitling an /etc/motd window to
            /home makes it a directory window. This is how you
            navigate.
          - ELIDED FROM THE LEFT when the window is narrow —
            ".../kitty/README", never a truncated tail. What is cut
            is the least specific part; the basename, which
            identifies the window, always survives.
          - CLICK MAKES IT AN INPUT FIELD, the title scrollable
            inside it, ordinary text-field behaviour. Not a bespoke
            editor: the platform's field, so selection, IME, undo
            and keyboard navigation come free.

CONTROLS  and the WINDOW OPERATIONS, which belong here rather than
          on the toolbar because their operand is the window as a
          thing in a layout — not what it holds. Close, Minimise,
          Maximise, New column, New row, New tab, Fit; listed in
          full below. ALL OF THEM INFORM THE PARENT.
          A child never resizes or removes itself; it asks. That is
          what makes the recursion work, because a parent owns the
          layout of its children and nothing else does.

          close     closes the window cleanly AND the underlying
                    command with it, if one is running. An rc dies
                    when its window does.
          minimise  asks the parent to take back this window's
                    rectangle. Reversible.
          maximise  asks the parent to take back the rectangles of
                    all the OTHER children. Reversible.

          THOSE TWO ARE ONE OPERATION. See "Allocation" below: a
          parent gives rectangles to some of its children, and the
          rest appear as tabs. minimise(me) moves me out of the
          allocation; maximise(me) moves everyone else out. Same
          mechanism, different argument, and each undone by moving
          back.

          Note what this settles. An earlier draft refused maximise
          ("a third state makes four with normal") — it was
          reasoning about a child in isolation, when maximise is an
          instruction to the PARENT, which already remembers an
          arrangement. And a later draft had maximise MINIMISE every
          sibling, which cost one title bar per sibling and so did
          not actually free much room; with twenty rows the button
          did not do what its name promises. Moving them out of the
          allocation costs one strip however many there are.

TAG LINE  free, editable text, and IT COMES FIRST IN THE ROW because
          it is the OPERAND — NOT A COMMAND LINE. Its own six
          buttons follow it (New, Open, Run, Find, Edit, Add), then
          the separator, then the toolbar's, which have nothing to
          do with it.

          Hers: "'alice' in the tag line and then pressing 'Look'
          searches for alice in window file, 'mk' in tag line and
          then pressing 'Execute' runs mk in the folder where the
          file is."

          This is acme's 2-1 chord — apply a verb to text — with the
          gesture decomposed into two acts a keyboard and a finger
          can both perform. It is why the six can be a fixed set
          without loss: the verbs are fixed, the argument is
          anything you can type.

          AN EMPTY TAG LINE MEANS "USE THE SELECTION". Type
          nothing, select alice in the body, press Find — it finds
          alice. Not an invention: this is macOS's Cmd-E and the
          convention in most editors, and it generalises across all
          six without a special case.

            Find    the selection
            Open    the selected text as a window
            Run     the selection as a command — which is precisely
                    acme's button-2-on-text
            Pipe    the selection into a command
            Edit    a sam command over it

          So THE ALWAYS-VISIBLE BUTTONS ALREADY ACT ON POINTED-AT
          TEXT, and they offer four more verbs than a selection menu
          ever did. Which is what lets the selection menu be
          optional (below) rather than the only road.

          When the window is narrow the row WRAPS, tag line and its
          buttons staying together ahead of the break wherever they
          can, because separating an operand from the verbs that
          consume it is the one break that costs meaning.

TOOLBAR   BEYOND THE SEPARATOR: verbs whose operand is THIS WINDOW'S
          CONTENT, and NOTHING HERE IS UNIVERSAL — not Save, not Revert, not even Undo.
          All of it is the type's, listed by type below. The window
          OPERATIONS are not here; they are with the controls,
          because their operand is different.

          THE CORE IS CLOSED; THE TOOLBAR IS NOT. A type's manager
          declares its verbs, and a person adds their own with `Add`
          — which writes to /type/<x>/verbs, their /home/type binding
          over the system's. What cannot be removed or redefined is
          the core six, because a window's behaviour has to be
          predictable.

          ALL OF THEM ARE ALWAYS VISIBLE. An earlier draft judged
          some verbs "rare" and hid them behind an overflow. That
          invented a hierarchy of importance the design does not
          have. When a window is narrow the toolbar WRAPS, and only
          when it is truly narrow does a disclosure control appear —
          a response to geometry, never a judgement about frequency.

BODY      natively rendered and natively edited, WITH SCROLLBARS —
          or child windows, when this window has composited itself.

STATUS BAR one line at the bottom, and ITS FUNCTION IS THE TYPE'S.
          An edit window shows line:col, size, dirty; a shell window
          shows the running command; an ls window shows the count.
          Read-only, and about THIS window — the operand rule again.
```

### The names: emca uses the era's words, acme keeps its own
Christine, 2026-09-01: "acme names for the builtins are idiosyncratic
(snarf, zerox, put, get, etc.). They have not stood the test of time,
and are against Apple HIG. So I think we need common names like Open,
Cut, Copy, Paste, Save, Run, Search etc. This only applies to emca,
not acme. Acme of course retains it's naming."

So emca's verbs are named for the people using them, and acme's port
(acme.md) keeps Snarf, Zerox, Put and Get unchanged — it is Bell
Labs' program and renaming its buttons would be changing it.

```
acme            emca            why
-------------   -------------   -----------------------------
Snarf           Copy            the word everyone else uses
Put             Save
Putall          Save All
Get             Revert          it discards changes and re-reads,
                                which is what Revert means
Del             Close
Delcol          Close           the same verb; the operand differs,
                                which is the whole point
Exit            Close           the ROOT window's close
Newcol / New    New column /    and they duplicate this window in
                New row         that direction (see below)
Zerox           New column      subsumed: duplicating IS what it did
Look            Open / Find /   look was never one operation. It is
                Plumb           a DISPATCHER over four, and emca
                                shows them — with jump and search
                                merged into Find, since : and #
                                already tell the two apart
Edit            Edit            kept: there is no era-common word
                                for "apply a structural editing
                                command", because no mainstream
                                editor has one. Replace would name
                                it after the least of what it does
Sort            Sort
Undo / Redo     Undo / Redo     already the era's words
Cut / Paste     Cut / Paste     likewise
Dump / Load     Save Layout /   root window only
                Restore Layout
Kill            Interrupt       shell windows, and the status bar
ID              —               not a verb; it is in the status bar
```

### The three groups: window, text, type
Sorting every verb by WHAT VARIES gives a tighter cut than sorting by
operand alone, and it is the shape the design settled into:

```
group        surface           universal?     verbs
----------   ---------------   ------------   ---------------------
WINDOW       the title bar     always         Close, Minimise,
operations   row                              Maximise, New column,
                                              New row, New tab, Fit
TEXT         the tag line's    always         Open, Find, Run, Pipe,
operations   buttons                          Edit, Add — acting on
                                              the SELECTION when the
                                              tag line is empty
             a contextual      OPTIONAL, and  whatever the surface's
             menu at the       the surface's  platform puts there
             selection
TYPE         the window        NEVER          the type's, plus
operations   toolbar                          whatever Add put in the verbs file
```

So the universals are what a window can do to ITSELF and to TEXT; the
toolbar is the only surface that varies by window, and — because Add
writes to the verbs file
writes to it — the only one a person can change. Fixed rows, one
extensible row.

OPERAND DETERMINES SURFACE still explains WHERE each sits: the title
bar row acts on the window in its layout, the toolbar on its content,
the tag line on text you COMPOSED — or, when it is empty, on text you
POINTED AT. A selection menu, where a surface offers one, is a
shortcut to the same verbs and not a fifth thing.

### The window operations
Their operand is the window AS A THING IN A LAYOUT, not what it holds,
which is why they sit with the controls and the title rather than on
the toolbar. Every window has all seven.

```
Close        closes the window cleanly and the underlying command
             with it. On the ROOT window this is Exit. Closing a
             container closes what it holds — acme's colcloseall().
Minimise     move out of the parent's allocation. Reversible.
Maximise     move every sibling out. Reversible.
New column   duplicate this window into a new column beside it
New row      duplicate this window into a new row below it
New tab      duplicate this window as a TAB — the same operation with
             the allocation bit flipped, since a tab is a child the
             parent has given no rectangle to. Not a fourth concept.
Fit          drop the size overrides in this subtree, re-derive
```

ROWS AND COLUMNS ARE ONLY EVER CREATED BY A USER. Hers, and it corrects
a rule this document briefly held — that a new window should be
"allocated if it fits, and a tab if it does not". That made THE SAME
GESTURE DO DIFFERENT THINGS AT DIFFERENT WINDOW SIZES, which is
inference of the kind the rest of this design removes, and it let the
machine restructure a workspace nobody asked it to touch.

So OPEN, RUN AND PIPE ALWAYS MAKE A TAB. Predictable, identical
everywhere, and non-destructive of structure — the safest thing a verb
can do to a layout it did not build. The user has two ways to create a
split and the machine has none: the New column and New row buttons,
and dragging a window onto an edge.

And a consequence that is not an extra rule but the model showing
through: AN UNALLOCATED WINDOW HAS NO RECTANGLE, so there is nothing to
render but its title in the strip. The tag line, toolbar and status bar
belong to the allocated window BY CONSTRUCTION — nothing had to say
that inactive tabs do not show them.

MOVE IS DRAG AND DROP, already in acme's census as one of the eight
gesture-only operations, filed under layout as direct manipulation.
Two drop targets, and they differ:

```
onto a window   become a tab of that window's parent — re-parenting,
                and no structural change
onto an edge    become a new row or column there
```

The second is the other way a user creates a split, which keeps "only a
user creates rows and columns" true while leaving two roads to it.
Kernel-side it is one wctl verb — re-parent to a given window at a
given index; the gesture is the surface's.

Move and resize are otherwise NOT buttons. They are direct
manipulation — drag a divider, drag a title — and acme filed them the
same way (layer 1, not layer 3).

### The body is a sequence of items
What makes the text operations universal, rather than universal by
fiat: EVERY WINDOW'S BODY IS A SEQUENCE OF ITEMS. Find selects a
subset. Edit and Pipe operate on the subset. What an ITEM is, is the
type's — and that is the only thing that varies.

```
type     an item is        an item's text is
------   ---------------   ---------------------
edit     a text range      the range itself
ls       a file            its name
shell    a line of output  the line
root     a child window    its title
```

This is sam's structural model — an address selects, a command
transforms — lifted from characters to items of any kind. Which is
also why dot is a SET (see Find): x's loop is the general case, and
single-range dot was the special one.

### The tag line's six verbs
Universal, always present, and always meaningful. Their operand is the
TAG LINE'S TEXT, so they sit with it rather than on the toolbar — a
person needs to see that Run acts on what they just typed and Save
does not.

```
Open   A NEW WINDOW WHOSE TITLE IS THIS TEXT (hers). Nothing else
       needs saying: the title RETARGETS, so the window shows
       whatever the title names and its type follows — a directory
       becomes an ls window, a file an edit window.

       A NAME THAT DOES NOT EXIST YET is not an error: you get a
       window on a file that is not there, and Save creates it.
       That is acme's behaviour, and it is why no type needs a "New
       File" verb.

       AND A TRAILING SLASH NAMES A DIRECTORY — `tmp/` (hers). So
       "new folder" is not a verb either: it is Open with a slash,
       which is the same rule and not a special case. That empties
       the last creation verb out of a type's toolbar.

       The one asymmetry, and it is forced rather than chosen: a
       file is created by Save, but A DIRECTORY HAS NO CONTENT AND
       THEREFORE NO LATER SAVE at which to create it, so Open with a
       trailing slash creates it immediately.

       Where it lands is the sizing heuristic's business, so Open
       takes no placement argument. And it REUSES an existing window
       on the same title rather than opening a duplicate, as acme's
       look does; emca knows every title, so this is a lookup.

Find   SELECT EVERY ITEM THE TEXT NAMES, in this window. It is
       `,x/foo/` — the loop over the whole body — so dot becomes THE
       SET OF ALL MATCHES, not the next one.

       The notation carries the address/literal distinction: a
       leading : or # means an ADDRESS (:42 a line, #c a rune
       offset), its absence means literal text. `42` finds the text
       "42"; `:42` selects line 42. An address naming one place
       gives a set of one — the degenerate case of the same rule.

       REPEATED FIND IS IDEMPOTENT. With every match already
       selected, moving between them is NAVIGATION — scrolling —
       which is the surface's half. There is no "find next" verb.

Run    execute the text as a command, in the window's directory, AND
       IGNORE DOT. Output goes to an OUTPUT WINDOW — see below.

Pipe   write THE SELECTED ITEMS to a command's stdin, one per line,
       and put the results in an OUTPUT WINDOW — see below.

       One semantic, no per-type behaviour: items are lines on
       stdin, always. Where a command wants arguments instead you
       pipe through xargs, which is exactly what xargs is for —
       `xargs rm` on an ls window, `xargs kill` on a proc window,
       `sort` or `sed` or `awk` on an editor window. WE DO NOT HAVE
       TO INVENT AN ARGUMENT SUBSTITUTION, because Unix solved it
       and the thing that does the conversion is a program you can
       see and change.

       Non-destructive at the source: the selection is not replaced.
       The command may of course have effects of its own.

Edit   apply the text as a SAM COMMAND to each selected item's TEXT.

       And this is what makes Edit universal without hand-waving —
       one semantic, with the type supplying only what an item's
       text IS:

         edit    the range      -> changes the buffer
         ls      the filename   -> RENAMES
         root    the title      -> RETARGETS, because titles do
         shell   an output line -> changes the scrollback

       The root row falls out rather than being designed: select
       every window under /old/branch/, apply s|/old/|/new/|, and
       they all retarget. A type with no meaningful notion of
       changing an item's text simply does not offer Edit.

Add    put the tag line's text on this window's toolbar as a button.
       It WRITES to the manager's verbs file — your own /home/type
       binds over the system's — so unlike acme's tag, the button
       persists. This is how "type Indent in the tag and it works"
       becomes durable.
```

RUN, PIPE AND EDIT ARE ACME'S OWN THREE — execute, the filters, and
Edit — named for what they do. They must stay three, and the dangerous
merge is Run with Pipe: with three files selected, pressing Run meaning
"build the project" must not run rm three times. RUN IGNORES DOT; PIPE
ITERATES OVER IT, and that has to be a button rather than a heuristic,
by the rule below.

### The rules that decided all of this
Two, arrived at repeatedly from different directions:

```
A BUTTON IS WARRANTED EXACTLY WHERE THE TEXT CANNOT SAY WHICH
OPERATION IS MEANT.

  `mk` is both a filename and a word in the body, so Open and Find
  are separate buttons. `x` and `g` are both sam commands and
  plausible program names, so Edit and Run are separate. But `:42`
  and `42` are told apart by the colon, so jump and search are ONE
  button — the notation does the work and a button need not.

A VERB THAT MODIFIES IS NEVER INFERRED.

  Find is safe by construction; Edit announces that it might change
  things. This is also why Run and Pipe do not collapse into one
  verb that behaves differently when something is selected.
```

AND NOTHING COMPUTES APPLICABILITY. An earlier draft had Open and Find
appear only when they applied, which needed emca to judge the text and
answer, and a `verbs` file to carry the answer. They always apply, so
they are always there — which gives "look is a dispatcher, SHOW the
choice" a better answer than computing it: both buttons are permanently
visible, with no round trip and nothing to get wrong.

### Where command output goes: /output is a filesystem
Hers: "/output/mk all" is pretty clear. And it is the Plan 9 answer to
a question that had two bad answers.

```
A WINDOW'S TITLE IS A PATH, because the title retargets. So an output
window needs a path that is REAL.
```

Two candidates were considered and both fail, in opposite directions.
acme's `<dir>/+Errors` gets the CONTEXT right — it names the directory
the command ran in — but the path is a lie: nothing serves a file of
that name there. A real file in /tmp is truthful but gets the CONTEXT
wrong, since the title determines the directory, so pressing Run in
that window would run the command in /tmp. It also dangles.

```
/output IS A FILESYSTEM, served by emca, AND IT TAKES /proc's SHAPE:
a number, with everything else as files inside.

    /output/7/0         stdin — what was fed in
    /output/7/1         stdout
    /output/7/2         stderr — captured separately
    /output/7/cmd       cat /template/Python   — full, unescaped
    /output/7/dir       /template              — where it ran
    /output/7/status    exit code
    /output/7/log  the session as it read — what Run opens

        % cd /template
        % cat Python
        ....
        exit 0

The descriptors are numbered, as /fd already numbers them. The
log is text/plain — no new type — and the prompt line is not
decoration: the prompt is rc's own %, and emca emits the cd line so
the log is self-contained. An
output is a COMPLETED CONVERSATION: the shell role shows a transcript
being written, this is one that finished.

The window's title reads the full command, because emca reads cmd and
a title has no filename rules to obey.
```

An earlier form mirrored the filesystem — `/output/<dir>/<command>` —
and broke on its own justification: **a command line contains `/` and
therefore cannot be a filename**. Escaping or substituting made every
access require quoting, in a filesystem made typeable on purpose. The
number answers better as well: `grep -l /template /output/*/dir` finds
what ran there without needing to know the path first, which navigating
a mirrored tree cannot do.

A real path: it cats, greps, pipes and retargets like anything else.
Nothing is written to disk, so nothing dangles and nothing persists
behind your back. The basename is the command, so the title bar reads
"mk all" with "/output/home/project/ipnx/" dimmed before it — the same
two-element rendering every other window gets, and the dimmed half
happens to say where it ran.

MIRRORING EARNS ITS KEEP TWICE. It resolves the collision — `mk all`
in two projects is two paths, so two windows, and reuse still gives one
window per command per directory. And IT REMOVES AN EXCEPTION rather
than accommodating one: an earlier draft had the output type STORE the
command's directory, because context normally comes from the title and
/output would have been the wrong context. With the mirror, the context
is DERIVED from the title after all — strip the /output prefix — so it
is deterministic, visible in the path, and no type has to keep a
private field.

A parallel tree indexed by path is the house shape: #V/<snapshot>/…
does it for versions, /n/ for mounted worlds. /output does it for what
commands said.

### The selection's verbs, and where they appear
WHICH VERBS A SELECTION AFFORDS IS IPNX'S. WHERE THEY APPEAR IS THE
SURFACE'S. That is the founding division applied to the one surface
that had been over-specified as "a floating bar".

A selection affords Open and Find — and, through the empty tag line,
the other four as well. Cut, Copy and Paste are THE PLATFORM'S: they
carry the clipboard, the IME and the permissions, and reimplementing
them would replace working behaviour with a worse copy.

So the surface renders them wherever its platform puts verbs for
selected text: the native callout on iPadOS, the context menu on the
web, a small floating bar where a component's menu cannot be extended,
or NOTHING AT ALL. Different surfaces should differ here, which is the
whole reason the division exists.

THIS SURFACE IS OPTIONAL, and that is the change. Three surfaces are
required — the title bar row, the tag line and toolbar row, the status
bar — and this is a shortcut, because the empty-tag-line rule already
puts every verb on permanently visible buttons in a fixed place. An
earlier draft made a bespoke floating bar mandatory, which collided
with the platform's own selection callout: two popovers, or suppress
the native one and lose what it does.

But a surface that DOES offer one is bound by three requirements,
because a bad answer here is a defect and not a matter of taste:

```
1. IT MUST NEVER REFLOW THE BODY. Overlay, or live in chrome that
   already exists — never take space from the content. Text jumping
   under the reader is the worst of the failures and the only one
   that can lose your place.
2. IT MUST NEVER COVER THE SELECTION. Above by preference, below
   when there is no room.
3. IT MUST DISMISS CHEAPLY — on scroll, on typing, on a click
   elsewhere. Anything that lingers over content is occlusion under
   another name.
```

Timing, the known trap for any of them: THE RANGE IS SNAPSHOTTED WHEN
THE MENU APPEARS. The tap that presses a button collapses the native
selection first. Underneath is a conflation — dot is insertion point,
operand and clipboard source at once, three lifetimes that a mouse
hides and touch cannot.

TESTABILITY SURVIVES, and improves. A bespoke bar was testable because
it was our own DOM; a native callout is not drivable from a headless
test. But the `ui` file already declares what the surface rendered, so
the suite asserts THE VERB SET WAS DECLARED AND IS KEYBOARD-REACHABLE
— the property that actually matters — and that holds identically
whether the surface drew a popover, a menu, or nothing.

### The window toolbar, by type
NOTHING IS UNIVERSAL HERE. Not Save (a shell has nothing to save), not
Revert (a shell does not), and not Undo.

UNDO IS NOT UNIVERSAL, and the rule is:

```
UNDO IS AVAILABLE EXACTLY WHERE EVERY OPERATION THE WINDOW OFFERS IS
CONFINED TO A BUFFER EMCA HOLDS.
```

A killed process does not come back, a deleted file does not, a sent
request does not. An Undo button that silently does nothing on three
types of four is worse than no Undo. This is the same root cause as
the unit question in PART FIVE: acme could make Undo a universal
builtin because EVERY ACME WINDOW WAS A TEXT BUFFER, just as it could
measure layout in characters. Both universals were artefacts of a
system with one content kind, and neither survives four types.

For the irreversible verbs the design already has acme's answer, and
it generalises: acme's Del REFUSES on a dirty window — you must Put or
Delete explicitly. So DESTRUCTIVE VERBS REFUSE WHEN THERE IS
UNRECOVERABLE STATE, and otherwise simply happen. No confirmation
dialogs: they train people to click through, and protect nothing.

A type ships DELIBERATELY LITTLE, because `Add` exists and the
toolbar is a text file. Anything wanted occasionally is added by the
person who wants it — `Add` writes it to /type/<x>/verbs, which their
/home/type binds over — and the type carries only what it needs.

```
root     Save All        write every descendant with unsaved changes
         Reset           rebuild the default arrangement
         Save Layout     the window set, to a file
         Restore Layout
         Home…           where files live: the shipped examples,
                         browser storage, or a granted local folder
                         (a granted directory IS a bind)

ls       Revert          re-read the directory

         And nothing else. Open covers both creations — a name for a
         file, a name with a trailing slash for a directory — so ls
         carries exactly one verb of its own.

edit     Save            only while there are unsaved changes, and
                         its appearance IS the dirty indicator
                         (acme's rule, kept — acme.c:383)
         Revert
         Undo  Redo      here and nowhere else

shell    Interrupt       stop the running command
         Clear
```

Two that were considered and REFUSED, because the design already
covers them: `Up` on ls — the title is editable and its directory
segment is tappable, and that IS navigation — and `Wrap` on edit,
which is a view mode and therefore the surface's, not a verb that
crosses.

### Every operation accounted for
acme.md counts 38 operations. Where each one went:

```
native (4)        select, insert, delete, scroll — the component's
gesture (4)       expand (double-click), move, grow/shrink (drag),
                  plumb (the fallback when rules decide)
title bar (6)     Close, Minimise, Maximise, and Duplicate as column,
                  as row, as tab — three buttons in the tiled
                  implementation today, which may change. The
                  CONTRACT names one: duplicate
tag line (6)      New, Open, Run, Find, Edit, Add — ALWAYS these six,
                  in this order, in every window. |, < and > are
                  syntax within Run (there is no Pipe button); : and
                  # are syntax within Find
the selection     Cut, Copy and Paste, plus whatever the manager
                  offers. The tag line's five reach the selection
                  when the tag line is empty. Where any of them
                  appear is the surface's
toolbar           the per-type lists above
status bar        id, and the running command with Interrupt
```

Two acme builtins disappear as BUTTONS because something else already
does the work: Zerox (New column duplicates) and ID (it is state, so
it belongs in the status bar). New disappears too — Open a name that
does not exist, and Open a name ending in `/` for a directory, so
neither "New File" nor "New Folder" is a verb anywhere. And `jump` and
`search` become ONE button, Find, because the notation distinguishes
them.

ONE IS DROPPED OUTRIGHT: Sort. acme's colsort() reorders a column
alphabetically by filename — a one-shot tidy, undone by the next
window opened. It answered a real problem in acme (a column of a dozen
windows with no other way to find one) which emca answers with TABS, a
list of names already. And it is the only operation in the set that
DESTROYS A DELIBERATE ARRANGEMENT WITH NO WAY BACK: minimise, maximise
and Fit are reversible or re-derivable, where the previous order is
simply not remembered. Fit earns its destructiveness because sizes
drift accidentally; order does not drift, it is set on purpose.

### Allocation, and what a tab is
```
A PARENT ALLOCATES RECTANGLES TO SOME OF ITS CHILDREN, ALONG AN
AXIS. THE CHILDREN IT DOES NOT ALLOCATE TO APPEAR AS TABS IN A
STRIP.
```

That is the entire layout model, and it is worth checking against what
people already recognise:

```
what you see                    what it is
-----------------------------   ------------------------------
three editors side by side      3 allocated, strip empty
minimise one                    2 allocated, 1 in the strip
maximise one                    1 allocated, 2 in the strip
an ordinary tabbed editor       1 allocated, N-1 in the strip
```

So A TABBED WINDOW IS A MAXIMISED ONE, and "tabs display only when
there is more than one" is not a display rule to be enforced — it
falls out, because an empty strip has nothing to draw. Row and column
remain, as the AXIS. "Stack" disappears as a separate composition.

A TAB IS NOT A REDUCED WINDOW. It is a window the parent has not
given a rectangle to. Nothing about it is diminished — it keeps its
type, its buffer, its toolbar, its tag line, its running command —
and selecting it gives the rectangle back, whole. Which is why the
questions an earlier draft agonised over simply do not arise: there
is no minimised strip too narrow for three controls, no title that
cannot be edited because it is rotated, no close button made
unreachable by lack of room.

And the new-tab button on every toolbar is the same mechanism once
more: add a child, unallocated.

MINIMISING A CONTAINER takes its contents with it, without needing to
be said: a column that loses its rectangle is one tab, and its rows
are inside it.

### One tag string underneath
The model keeps acme's single tag string:

```
/home/x.c  Del Snarf Get Look Edit  Put Undo  |  alice
'--- name ----' '--- fixed builtins ---' '-dynamic-' ^ '-operand-'
```

It stays one string because the 9P tag file exposes it as one buffer
and the behaviour suite reads and writes it. THE SPLIT INTO TITLE /
TOOLBAR / TAG LINE IS PRESENTATIONAL. rebuildauto()'s tracked
auto-block (acme.c:383) survives unchanged.

What the split fixes: acme's conflation of methods with subprocesses.
Del and mk are typographically identical in acme and behave completely
differently — one a method on the window, one a subprocess. The
toolbar/tag-line boundary IS that distinction, made visible.

## Boot and the session

The system boots into emca. There is no login shell that later starts
a UI; emca IS the session, and rc is a window inside it.

The default workspace is a NAMESPACE FILE — declarative, which is the
"boot is rc plus a namespace file" refusal of systemd paying out:

```
motd            an editor window, main area
a listing       the user's home, left pane
an rc           bottom pane, shell type
the top toolbar the system's managers
```

Consequences:

```
- the browser page, the macOS app and the iPadOS app stop being
  hosts that run a demo and become SURFACES OF EMCA. The demo is
  the system's face, not a demonstration of it.
- emca absorbs rio's job. #w still mints windows; a program that
  opens its own canvas window appears as an emca window of a canvas
  type. rio-today as a separate design retires.
- a window IS a namespace view, which makes per-window confinement
  a headline feature rather than an architectural claim — the
  sandbox story with a face on it.
```

