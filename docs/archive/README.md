# Archive — superseded specifications

**NOTHING IN THIS DIRECTORY IS CURRENT.** Every document here described how
some part of the system worked at a point in the past and has since been
replaced. They are kept because the *reasoning* in them is often still good
even when the location or mechanism is wrong — but they must never be read as
describing the system, and nothing should be built from them.

A superseded spec belongs **here**, never inside a live document. Folding one
into the decision record puts a *what* inside a *why*, and leaves outdated
text where someone will read it as current (2026-09-02).

| document | described | replaced by |
|---|---|---|
| [dev-window-device.md](dev-window-device.md) | `/dev/window/<type>/<n>` as a **kernel device** — the bidirectional interface through which IPNX declared a window's chrome and the host reported events | [window.md](../window.md) — the same contract, served by emca. The window system left the kernel entirely on 2026-09-02 |
| [dev-canvas-protocol.md](dev-canvas-protocol.md) | the original `/dev/canvas` protocol, before it narrowed to genuine drawing | [canvas.md](../canvas.md) |
| [implementation-2026-08-29.md](implementation-2026-08-29.md) | the plan M0–M18, grown by accretion — a new milestone appended for each decision that invalidated an earlier one | [implementation.md](../implementation.md), replanned 2026-09-04 in three layers with the demo as a milestone |
| [packages-and-projects-v0.md](packages-and-projects-v0.md) | the first packages/projects draft — `/usr/kitty` paths, `/recipe`, no roles | the live proposal in [proposals.md](../proposals.md) |

Deleting these instead would lose nothing that `git log` does not hold; they
are kept in the tree only so superseded design is findable without archaeology.
