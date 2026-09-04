# The surface — what the host side owns

**Role: a *what* — the host half.** What emca-host owns and decides, and why
nothing above it needs to know. The contract it serves is
[window.md](window.md); the tiled arrangement it renders is
[compositor.md](compositor.md); the system it belongs to is
[saranos.md](saranos.md).

*Derived with Christine on 2026-09-02; decisions dated in [design.md](design.md).*

## Where the surface sits

```
Saranos              the app the user launches. ONLY Saranos knows about the host
  emca-host          a "Kit" inside it — macOS SwiftUI, the browser page, iPadOS
    emca-IPNX        a process the kernel launched; speaks 9P to emca-host
```

**There is exactly one emca-host.** Nested emcas are IPNX-side processes, and
the host sees them as further windows.

## Saranos serves the hardware as devices

Saranos says, in effect: *I am a macOS app. I have a screen, a keyboard and a
mouse. I will serve these as virtual devices to the IPNX kernel, which I am
going to start.*

So on the IPNX side `/dev/cons`, `/dev/draw` and `/dev/canvas` exist **whether
or not emca is running**:

| | |
|---|---|
| **without emca** | they connect straight to the host's screen, keyboard and mouse. IPNX is a CLI with no windows — a mode that must keep working |
| **with emca** | emca **virtualises** them per window, and a client cannot tell the difference |

> **The convention: `/dev` is the slot that can be virtualised.** Not "where
> devices live" — the place whose contents may be substituted underneath a
> process without it being able to tell. That is why a window's files belong
> there rather than under `/mnt`, and why `bind '#w/N' /dev` was the right shape
> from the start.

That indistinguishability is the load-bearing property, and it is rio's: a
Plan 9 client cannot tell rio's `/dev/draw` from the hardware's, which is
exactly why rio nests. Here it is why **emca nests**, why acme runs unmodified
as an ordinary client, and why a whole second kernel instance could one day
draw into a window with no special path.

It also gives *"IPNX implements no renderers"* its precise meaning: **renderers
are in Saranos**, and emca-host is the part of Saranos that virtualises them.

## The outermost window's chrome is the host's outright

Not merely *rendered* by the surface — **owned** by it. The title, the window
controls and the toolbar of the outermost window are the platform's furniture:

| | title & controls | the system's verbs |
|---|---|---|
| **macOS** | the native title bar | the **menu bar**, or a native window toolbar |
| **browser** | the tab | a global toolbar at the top of the page |
| **iPadOS** | the platform's | the platform's |

**This is where "chrome is the surface's" pays best.** `NavigationSplitView`
gives the collapsible sidebar, `.toolbar` gives native placement, the menu bar
is free, and the status line is native — so the macOS surface is built from
**native furniture rather than emulated chrome**, which is what the rule was
for.

**Two mappings that fall out rather than being designed:**

- **Collapsing the sidebar *is* `minimise` on that window.** The native gesture
  and the contract control are the same operation, so SwiftUI's collapse drives
  the window control instead of sitting beside it.
- **The toolbars split without ambiguity:** the **global toolbar or menu bar**
  carries the *system's* verbs — `Halt`, `Reboot`, `New Shell` — because the
  outermost window's content is the system; the **sidebar's own toolbar**
  carries the *listing's*, because that window's content is `/`'s entries.

**The exception is the outermost surface, not the type** — a nested emca gets
ordinary emca chrome ([window.md](window.md)).

## Chrome is the surface's, and native to the target

**A type manager does not dictate how a window looks, apart from its content.**
Buttons look like buttons on macOS and on iOS; a toolbar is a macOS toolbar.
The declaration is IPNX's, the rendering is the host's — the same rule that
governs content and, now, theming. One principle, not three accommodations.

The surface therefore owns:

| | |
|---|---|
| **how chrome is drawn** | native controls, native toolbars, native furniture |
| **theming** | colours, light and dark, materials, iconography — every target already has these, so there is nothing here for IPNX to build |
| **placement** | where each of the four operands appears: a title bar row, a toolbar top or bottom or side, a floating bar at the selection |

### The declaration must stay semantic

Because the surface decides appearance, a type's `window` file declares a
**label and an action** and never anything presentational. The place
presentation will try to leak in is **icons** — macOS wants an SF Symbol, iOS
its own, the browser neither. The rule that fits every target: **a type names a
standard verb, and each surface maps that name to its own iconography**; a
non-standard verb is rendered as its label. The registry never ships an image.

## The surface reports the CONTENT rectangle

Chrome placement changes how much room is left, so the surface subtracts its
own furniture before telling the IPNX side what space exists. **The rectangle
that crosses is the content rectangle, never the window rectangle.**

This is what makes chrome placement free: a target that mounts its toolbar down
the side reports a narrower content rectangle and needs no change on the IPNX
side at all.

> **The instinct is already in the code, with the leak still in it** —
> `demo/shell/shell.mjs` reports `offsetHeight - 30`, subtracting chrome height
> as a hardcoded constant. Right idea, one chrome layout baked in.

## The surface may not intercept keys a manager needs

**The surface owns chrome and window-level gestures; the manager owns the
keyboard inside its rectangle.** This is the keyboard form of *"emca does not
reach inside a manager's window"* ([window.md](window.md)) — the same boundary,
applied to input.

The first instance decides it: **Escape** collapses multi-cursor in the `edit`
role, but in a `shell` window Escape **must reach the process**, or vi, `less`,
readline and ncurses all break the first time someone uses them. So Escape
cannot be a surface-wide binding.

It will not be the last: Tab, ⌘K, arrow keys with modifiers and every
emacs-style chord all want to be both a global shortcut and something a manager
needs. **The reserved set must be small and written down**, or every new manager
discovers by accident which keys it is not allowed to have.

## A populated tag line must read as populated

Not cosmetic. The tag line beats the selection when both are present
([design.md](design.md)), and the only thing preventing a stale search from
silently overriding a fresh selection is that the winning text is **on screen**.
Style it like an empty placeholder and that defence disappears, leaving hidden
state.
