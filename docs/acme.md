# acme — the port

**Role: a *what* — the acme port specification.**

## Role: the port spec

**acme is Bell Labs' program.** This document is how it is modified to fit into
emca, with its functionality preserved. It is not a description of emca, and
emca is not descended from it: the four-layer anatomy below was *input* to
emca's design, not parentage (design.md, 2026-09-01).

**The port decision (corrected 2026-09-02).** Christine: *"acme, the original
Bell Labs program, is an emca-like program using `/dev/draw`, running under
emca."* **acme keeps its own composition.** It tiles internally and paints
through libdraw on `/dev/draw`, exactly as it does under rio — and under emca
that device is virtualised into a window, which acme cannot distinguish from
the raw one. **emca nests**, so a program that manages windows inside its own
window is an ordinary client, not an exception.

> **An earlier draft of this line, written 2026-09-01, had it backwards** — it
> said acme *"stops window-managing and delegates composition"*, and dismissed
> keeping its own composition as making acme *"a guest rather than a citizen"*.
> That was the fusion this project keeps rediscovering: it assumed citizenship
> means *being composed by emca*. It does not. **Citizenship is using the
> standard devices**, and acme does. The port is therefore far smaller than
> that draft implied — acme already runs.

**acme keeps its own vocabulary.** `Put`, `Get`, `Snarf`, `Zerox`, the `|` `<`
`>` filters. emca uses the era's names — Save, Revert, Copy — but renaming
acme's buttons would be changing acme rather than porting it.

**One consequence to honour, from emca's dot being a set:** emca's `Find`
selects every match, so dot is a set of ranges, where `curfile->dot` in sam and
acme is a single range. `/mnt/acme`'s `addr` and `data` assume one, so **the
port sees the primary range** — written down here so acme does not break
quietly.

## The anatomy, which was input to emca

What follows is the parts list this port and emca's design both came from: the
four layers, the 38 operations, and the census that found only three verbs
genuinely homeless. Its own conclusions about where those layers *should* go
were superseded by emca's design; the enumeration is what keeps its value.

### The problem, in Christine's words
2026-08-31: "We also need to find a solution for mouse chords. Maybe
extra toolbars? The current acme interface is neither discoverable nor
usable on a touchscreen."

Same day, sharpening it: "The other problem is how to redesign acme
for surfaces like touchscreen where functionality needs to be exposed
via Apple HIG. We need to abandon the three button mouse completely.
... We need to do full design thinking here."

Two complaints, one root: acme's verbs are invisible. A mouse user
cannot discover them (nothing on screen says alt-click executes or
right-click looks, and the chords are pure folklore); a touch user
cannot perform them at all (no alt, no middle, no right, no chords).

The directive: design as if the three-button mouse does not exist.
Mouse users get the same visible grammar as touch users — that is
what makes the verbs discoverable for everyone. (Whether alt/middle/
right remain as invisible accelerators for the initiated is itself a
question for the session; the grammar must be COMPLETE without them.)

### The decomposition, in her words
2026-08-31, setting the axis: "the issue is acme is conflating several
different things, and we need to separate them: the ideas lifted from
oberon (executable text, tiled display layout, interactivity); Plan 9
userspace; the three button mouse."

Then correcting the axis from provenance to anatomy — the four layers
this document is now organised by:

```
"To me acme does multiple things:
 1. tiled window management. a 'window' is either the output of a
    command, or a directory listing, or a file
 2. a tag line associated with a window. This is the 'title' of the
    window (name of file being edited, or directory name etc.), plus
    a list of text words. Text words can be built in commands
    ('snarf'), or user text ('mk') that can be executed as commands.
    Then there is the caret
 3. ... decompose and enumerate the 'operations' that acme supports:
    execute, look, open, jump, etc.
 4. Finally the mouse bindings. what each mouse button combination
    does, and which operation it is mapped to

 Once we have done this, we can see our task is remove the mouse
 bindings, and replace them with modern day equivalents (toolbar,
 context menu, buttons, menu bar etc.)"
```

The distinction that makes this the right axis: a provenance
decomposition (what came from Oberon, from Plan 9, from sam) explains
acme but does not license changing it. A structural one names the
parts, and a part with no dependents can be replaced. Layer 4 has no
dependents.

Pike's own two sentences licence the substitution, and they are worth
having on the record (Pike, "Acme: A User Interface for Programmers",
https://9p.io/sys/doc/acme/acme.html):

```
"Where Oberon uses objects and modules within a programming language
 (also called Oberon), Acme uses files and commands within an
 existing operating system (Plan 9)."
```

The idea is substrate-neutral BY DEMONSTRATION — realised once in a
language, once in an operating system. A third realisation, in a
canvas protocol, breaks no fidelity claim, because there is none to
break. And on what acme added to Oberon:

```
"to the basic idea planted by Oberon, it adds the ability to run on
 different operating systems and hardware, connection to existing
 applications including interactive ones such as shells and
 debuggers, support for multiple processes, THE RIGHT MOUSE BUTTON'S
 FEATURES, the default actions and context-dependent properties of
 execution and searching."
```

Three of acme's four original contributions are substrate; the fourth
is a mouse button. Acme's best invention is the one thing it named
after hardware — which is the conflation this document exists to undo.

Provenance for the measurements below: the paper as cited, and
userspace/cmd/acme.c as built (line references are to that file).

### Layer 1 — tiled window management
Three levels, each carrying a tag:

```
root                 tag: Newcol Kill Putall Dump Exit   (acme.c:2171)
  column             tag: New Cut Paste Snarf Sort
                          Zerox Delcol                    (acme.c:782)
    window           tag: <name> Del Snarf Get Look
                          Edit |                     (acme.c:851,854)
      tag            one line, editable
      body           the text
```

Invariants: windows never overlap; windows fill their column; columns
fill the screen; placement is automatic. Pike files the placement
HEURISTICS as support rather than concept — "features about
interaction rather than core interaction itself" — and the same
sentence covers mouse warping and no-click-to-type. Support may be
dropped without loss; concepts may not.

A window is a NAME plus a BODY. That is the entire type system, and
the consequence is worth stating plainly: acme has no window kind —
kind is INFERRED FROM THE NAME STRING, freshly, at every decision
point.

```
inferred kind   rule                    consequence
-------------   ---------------------   ---------------------------
directory       name ends in '/'        body is a listing; the tag
                                        omits Edit (acme.c:851)
file            name resolves to a      the tag carries Edit
                file                    (acme.c:854)
command output  name ends '+Errors'     append-only in practice,
                                        ordinary editable text in
                                        fact
scratch         name empty or           New with no argument
                unresolvable
```

This bites in the redesign: a toolbar must know what kind of window it
is on, and today nothing holds that — the name is re-parsed each time.
Whether kind becomes explicit (an attr on the window) is an open
question for the session, not a decision here.

### Layer 2 — the tag line
Five segments in one editable buffer, separated by convention alone:

```
/usr/christie/x.c  Del Snarf Get Look Edit  Put Undo  |  mk test
'------ name ---'  '--- fixed builtins ---' '-dynamic-' ^  '-scratch-'
 identity                always present      when apt  bar  user text

segment            what it is                    who writes it
----------------   ---------------------------   -----------------
name               window identity AND the cwd   the app, at
                   for every command run from    creation
                   it — two jobs, one word
fixed builtins     the window's method set       the app, at
                                                 creation
dynamic builtins   Put while dirty; Undo/Redo     the app,
                   while they have work          continuously —
                                                 rebuildauto(),
                                                 acme.c:383
bar '|'            separator                     convention only
scratch            the user's own commands and   THE USER; persists
                   arguments                     per window
```

Two facts constrain everything downstream:

1. The bar is convention, not structure. Words in ANY position
```
execute; the user may delete Del if they wish. The tag is not a
toolbar that happens to be text — it is text that happens to be
usable as a toolbar, and the difference is the whole problem.
```
2. The tag is a text buffer with its own caret, CO-AUTHORED by app and
```
user. The app must edit around the user's text, which is why
rebuildauto tracks an offset/length block (autopos/autolen) rather
than rewriting the line.
```

### Layer 3 — the operations
The contract, enumerated independent of invocation. The load-bearing
column is the last one: does the operation already have a NAME?

Text — operand is a range (dot)

```
select          a range or point                      native
expand          a point -> a range: word, line,       GESTURE ONLY
                quoted, bracketed
insert          text at dot                           native
delete          remove dot                            native
snarf           dot -> clipboard                      Snarf
cut             dot -> clipboard, then remove         Cut
paste           clipboard -> dot                      Paste
undo / redo     unwind by sequence number             Undo Redo
```

Acquisition — the four operations that 'look' conflates

```
open            text naming a file or directory       GESTURE ONLY
                -> a window on it, or reuse
jump            text naming an address (:27,          GESTURE ONLY
                :/re/, #c, compounds) -> dot
                moves, view scrolls
search          literal text -> next occurrence,      Look
                wrapping
plumb           anything else -> rules decide         GESTURE ONLY

'look' is NOT an operation. It is a DISPATCHER over these four,
choosing by parsing the text. The word Look reaches only one of the
four. That the plumber exists at all is Plan 9's own admission that
this dispatch needed factoring out.
```

Execution

```
run builtin     a word -> a method on this window     the word
run external    argv, cwd = the window's directory,   the command
                output -> dir/+Errors                 text
run with        a command PLUS a selection, possibly  GESTURE ONLY
 argument       from another window                   (2-1 chord)
pipe   |        dot -> stdin, stdout replaces dot     |
send   <        stdout replaces dot                   <
receive >       dot -> stdin, output -> +Errors       >
```

File — operand is a window

```
get, put, putall, del, delete, zerox, edit            all named
```

Layout

```
new, newcol, delcol, sort                             all named
move            window -> another column/position     GESTURE ONLY
                                                      (layout box)
grow / shrink   resize within the column              GESTURE ONLY
                                                      (layout box)
scroll          move the body's viewport              native
```

System

```
exit, kill, dump, load, id                            all named
```

### The census, and it is the finding
```
38   operations in total
26   already carry a name — a word, measured in acme.c: Cut Del
     Delcol Delete Dump Edit Exit Get ID Kill Load Look New Newcol
     Paste Put Putall Redo Snarf Sort Undo Zerox (22 builtins),
     plus the three filters | < > and run-external (the command
     text is its own name)
 4   are native on every modern surface: select, insert, delete,
     scroll
 8   are GESTURE ONLY: expand, open, jump, plumb, execute,
     run-with-argument, move, grow/shrink
```

Of those eight:
```
- expand is native double-click for the common case
- plumb is a rules engine already queued as its own work
  (docs/userland.md: "the plumber, central again")
- move and grow are layer-1 direct manipulation — a different
  problem from the verbs, and it should not be solved with them
```

Which leaves THREE invocation points to re-home:

```
execute
look (dispatching open / jump / search / plumb)
execute-with-argument
```

That is the redesign's true surface area. The brief assumed the
inventory was the eleven mouse bindings; the anatomy says it is three
verbs, and the other thirty-five operations are already text, already
named, already touch-performable — because acme made them words in
1994 and never noticed that it had thereby solved most of this.

### Layer 4 — the mouse bindings
Purely a mapping table from gesture to operation. Nothing here is a
concept, and nothing depends on it. (This table supersedes the earlier
"inventory to re-home": it adds the expand column, which is where the
regularity lives.)

```
gesture                 expand?   operation
---------------------   -------   --------------------------------
B1 click                  -       select (collapse to point)
B1 drag                   -       select (sweep)
B1 double-click          yes      select
B2 click                 yes      EXECUTE
B2 drag                   -       EXECUTE (sweep = whole command
                                  line, arguments and all)
B3 click                 yes      LOOK -> open/jump/search/plumb
B3 drag                   -       LOOK (the swept text exactly)
1-2 chord                 -       cut
1-3 chord                 -       paste
2-1 chord                 -       EXECUTE-WITH-ARGUMENT, cross-
                                  window: "apply a command in one
                                  window to text in another"
B1 on layout box          -       grow
B1 drag layout box        -       move
B1/B2/B3 in scrollbar     -       scroll back / jump / forward
```

The table has a shape, and the shape is the point:

```
click = expand, then verb
drag  = verb on the sweep
```

— identically for all three verbs. That 3x2 regularity is why acme
feels coherent rather than arbitrary, and it is the property most
easily lost in translation. A replacement that puts execute in a
toolbar and look in a context menu may cover every operation and still
feel WORSE than the chords, because the grammar stops being one
grammar. Coverage is not the acceptance test; regularity is.

Two bindings are not verb-mapped at all. The layout box and the
scrollbar are direct manipulation of layer 1, and belong to a separate
replacement problem — the scrollbar is already native everywhere, the
layout box is not.
