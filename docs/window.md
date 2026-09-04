# The window manager contract

**Role: a *what* — the contract.** What a window manager gives a type manager,
and what a type manager gives back. **Every window manager implementation must
honour this and nothing more**; the tiled implementation we built is
[compositor.md](compositor.md), what the surface owns is
[surface.md](surface.md), and the types themselves are [type.md](type.md).

*Derived with Christine on 2026-09-02; the decisions are dated in
[design.md](design.md). The kernel is not party to any of it.*

## The contract in one line

> **emca gives a manager a rectangle and a namespace, and stops.**

Everything inside is the manager's: it renders there, binds into it,
subdivides it if it wants. emca does not reach inside a manager's window, in
the same way rio does not reach inside acme's columns.

## What emca gives a manager

| | |
|---|---|
| **a rectangle** | the **content** rectangle — chrome is already subtracted. A manager never learns what furniture exists around it, or where |
| **notification when it changes** | *resize*. The only thing crossing continuously — and the **cause** is never communicated, because the cause is the implementation's business: allocation, a drag, a grouping gesture |
| **a namespace** | the window's own. Populating a window is **binding**: a debugger binds the debuggee's `/dev/cons` into one pane, its control file into another |

## What a manager gives back

| | |
|---|---|
| **minimum and natural size** | so any implementation can size it sensibly — tiled allocation and a floating window's initial size both need it |
| **a status line** | what the window reports about itself. The manager says *what*; the surface decides *how* it is drawn |
| **verbs** | what the toolbar offers. **The MANAGER declares these, not the type** — `look` and `edit` are the same type and must differ, since Save and Undo are meaningless under `look` ([type.md](type.md)) |
| **dirty state** | whether unsaved work exists |

## The controls

**close**, **minimise**, **maximise**, **duplicate** — a child *informs its
parent*, which acts. The first three are in the contract because they are
universal: every window system under every style has had them since 1984.
Duplicate joins them because copying a window is equally style-neutral.

**How many BUTTONS a person sees is the implementation's.** The tiled
implementation renders duplicate as **three** — *as column*, *as row*, *as tab*
— because those are its placements; a floating implementation would show one
and place the copy itself. So the contract names **duplicate**, once, and
[compositor.md](compositor.md) names the three. (Three buttons is what emca
renders today, and may change.)

What they *mean* is the implementation's. Under tiling, maximise minimises
every sibling; under a floating manager it zooms. The manager asking never
knows which.

## The four operands

A verb needs something to act on, and there are exactly four things it can be:

| operand | |
|---|---|
| **the window in its layout** | close, minimise, maximise, duplicate |
| **the window's content** | the manager's verbs |
| **the tag line's text** | **New, Open, Run, Find, Edit, Add** — always these six, in this order, in every window. The tag line is an *operand*, not a command line; empty means "use the selection" |
| **the selection** | verbs offered contextually |

**The operands are the contract; where each appears is the surface's.** A title
bar row, a toolbar, a floating bar at the selection — those are rendering
decisions, and a target with a side-mounted toolbar needs no change on this
side of the line.

## What is NOT in the contract

Naming these matters as much as naming what is, because each was at some point
mistaken for universal:

| not in the contract | where it belongs |
|---|---|
| axis, alternation, allocation, slack, tabs, `leaves = cols / 72`, Fit, Reset | [compositor.md](compositor.md) — the tiled implementation only |
| how chrome is drawn, icons, colours, light/dark, toolbar placement | [surface.md](surface.md) — native to each target |
| what the content *is*, and how it is rendered or edited | the type and its manager ([type.md](type.md)) |
| anything at all | the kernel — it is not party to the window system |

## The one exception: the outermost window

**Every window in the tree obeys this contract. The outermost one obeys it
except for its chrome, which belongs to the host.**

| | |
|---|---|
| title, window controls, toolbar | **the host's** — the macOS title bar and menu bar, the browser's tab and toolbar, the iPad's furniture |
| its body | **children**, exactly like any container |
| allocation, alternation, the tree | **unchanged** |

It is not a special *type*: `inode/system` is ordinary, and a **nested** emca
opens its own `/` with ordinary emca chrome, because its parent is an emca
window rather than the host. **The exception is the outermost surface.**

Three attempts to remove it each produced a worse one — a sliver of body beside
children, an undecorated pane standing in for a body, and a second title bar
under the native one. Declared once, it costs less than any of them.

**And its manager owns an ordinary child.** A container's manager renders by
arranging, but `inode/system`'s also wants somewhere to write — so it opens one
**ordinary window** and writes there. Nothing about bodies or allocation
changes; it is a child like any other, and *collapsing it is `minimise` on that
window*.

## Why the contract is this small

Because a manager receives a rectangle and a namespace and nothing else, it
cannot depend on how the rectangle was chosen. **That is what makes the
windowing implementation replaceable** — tiled, overlapping, Stage
Manager-style — with no change to any manager or any type. A contract that
leaked allocation, or chrome, or the cause of a resize, would freeze one
windowing style into every program that ever ran here.

## The manager interface — a file interface

*Reviewed and endorsed by Christine, 2026-09-02.*

### The claim

> **The manager interface is a file interface. emca serves one directory per
> window, and a manager reads and writes files in it.**

This needs no new protocol, because 9P is already the only IPC. It works
identically for a host-side manager (`edit` over CodeMirror) and a guest-side
one (`/proc`), because one reaches the files through emca-host↔emca-IPNX and
the other mounts them directly — the symbiosis doing what it is for.

And it is what the tradition already does: acme serves `addr`, `body`, `ctl`,
`data`, `event`, `tag` per window at `/mnt/acme/N/`; rio serves its clients
`/dev/cons`, `/dev/mouse` and `/dev/wctl`. **The archived `/dev/window` device
was this design already** — the proposal is to keep its shape and change its
server from the kernel to emca.

### The per-window directory

**The path is `/dev`, not `/mnt`** (Christine, 2026-09-02): *"the convention is
`/dev` can be virtualised."* That is the reason, and it is stronger than
precedent — **`/dev` is not "where devices live", it is the slot that can be
substituted underneath you.** Without emca, `/dev/cons` and `/dev/draw` are the
host's real screen and keyboard; with emca they are virtualised per window; and
the client cannot tell. `/mnt` carries no such convention — a tree you attached
stays the tree you attached.

So a window's files belong in `/dev` for the same reason `/dev/draw` does: **a
manager reading `/dev/window/rect` must not be able to tell who provided it.** That is
what makes emca nest, what keeps the no-windows CLI case working, and what will
make a floating implementation substitutable for the tiled one.

rio — not acme — is therefore the analogue: it serves `/dev/cons`,
`/dev/mouse` and `/dev/wctl` to its clients, with the full set at
`/dev/emca/<n>/`. This project already follows it: `bind '#w/N' /dev` makes a
namespace a window, and the supervisor's file is named `devwsys.mjs`.

```
/dev/window/      each manager's OWN window — the common case, no id in the path
/dev/emca/<n>/    the full set: what emca serves, what a window tool reads

A manager opens /dev/window/rect, never /dev/emca/3/rect: no window id appears
in any path a manager uses, because its namespace contains only its own window.
It cannot reach another window's files — they are not there.

/dev/window is ALREADY this project's path: /lib/namespace binds '#w' there,
and the archived device spec used it. Ten generic names directly in /dev
(ctl, events, size, type...) would collide with cons, draw and canvas.

    rect      read  x y w h — the CONTENT rectangle, chrome already subtracted.
              A blocking read returns when it changes: that IS resize, and the
              cause is never reported
    size      write minw minh natw nath — what the implementation needs to lay
              you out, tiled or floating alike
    verbs     write the toolbar, one per line, same grammar as /type/*/verbs
    status    write the status line's text
    dirty     write 0 or 1
    type      read  the MIME type this window holds
    role      read  which role this manager is serving
    title     read  the window's title. emca OWNS it; the manager may look
    events    read  input not consumed by the surface — the manager owns the
              keyboard inside its rectangle
    ctl       write window-level requests: close, minimise, maximise, duplicate
```

> **RESOLVED (Christine, 2026-09-02): the window's content is ALWAYS a file.**
> What varies is only how the host **renders** it — as text, an image,
> structured or formatted text, a table. That is the rendering rule the system
> uses everywhere ([surface.md](surface.md)): the file is the truth, the drawing
> is the surface's.
>
> So `body` exists for every window, including one whose manager is host-side.
> The mirror protocol (`insert`, `delete`, `seq`) is then an **optimisation for
> keeping a host editor in step** — not a substitute for the file and not a
> second source of truth. A guest reading `/dev/window/body` gets the content,
> whatever is drawing it.
