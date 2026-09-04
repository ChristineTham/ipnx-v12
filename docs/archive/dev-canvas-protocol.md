# /dev/canvas — the original protocol (SUPERSEDED)

Moved from emca.md 2026-09-02 — a superseded protocol is history, not spec.

What must cross the line is exactly what neither half can know
alone. Four additions to /dev/canvas (docs/canvas.md). acme.md's
constraint 2 expected "little or no protocol change"; this exceeds
it, recorded as a deliberate amendment rather than an oversight.
Canvas v0 anticipated the direction — "no pre-marked span roles in v0
... span attrs arrive with the web presenter's real links, later" —
and emca is the benchmark that demands them, which is how every
element of v0 was derived in the first place.

```
1. STRUCTURE ROLES — so a generic surface can recognise a window
   and give it native furniture:

     role=window    A WINDOW. There is no other structural role,
                    because there is no other structural thing:
                    split, leaf and rail were roles for regions
                    that do not exist (PART FOUR). A window that
                    holds windows carries dir=row|col and its
                    allocation; one that holds a body does not.
     role=title     the name segment
     role=toolbar   the builtin segment
     role=tagline   the operand segment
     role=body      the content
     role=status    THIS window's ambient line

   Generic, not emca-specific: con(1) and any future client get the
   same furniture from the same roles.

2. WINDOW TYPE — type=<name>, resolved through /type. Drives
   placement, furniture and interactivity.

3. VERB APPLICABILITY — for a given range, which of the closed verb
   set applies. Sent as an attr in response to a select event; the
   surface renders what it is told.

4. SHOW REQUEST — show=1 on a window: "the user needs to see this".
   The app asks, the surface decides how. The sel attr's precedent
   exactly — the app steers and does not command, so an +Errors
   tail never steals focus.
```

Nothing else crosses. The surface never learns what a command is.

AMENDED (2026-09-01): emca DOES learn the viewport, because it owns
the tree and must not compose an arrangement that cannot fit. What it
learns is a size and a text-cell metric, both device-independent —
never pixels of a particular screen, and never anything about how the
host draws. See PART FIVE.

