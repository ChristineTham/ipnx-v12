# Proposals — designs awaiting review

**META — a REGISTER, not one of the six questions.** It holds proposed answers to
them — designs written but not reviewed — so that specs carry only what is
endorsed.

**Role: the middle state.** Christine's rule (2026-09-02, CLAUDE.md's
Conventions): everything is **spec'd** (discussed and endorsed), **proposed** (a
design exists, unreviewed), or a **gap** (undesigned). Specs carry only what is
endorsed, so proposals live here until they are reviewed — then they move into
the relevant spec, or they are dropped.

**Nothing in this document is agreed. Do not build from it.**

## What the host needs when the window device leaves the kernel (after the demo)

**Reframed 2026-09-03 by the rule that the kernel does not grow.** a3 was
scoped as *"the rasteriser moves to the host"*. Applying the test — **the
kernel only orchestrates processes; everything else is host or userspace** —
says the scope is larger and simpler: **`#w` leaves the kernel entirely.**

**Measured:** `#w` is **~1,100 lines, 22% of the kernel** — 745 in `wsys_*`,
`win_*`, `cv_*` and `drawmsgs`, plus `draw.rs`'s 364 — across **29 `WKind`
variants**. None of it is process orchestration. the legacy a1 and a2 steps already took
the tree out this way; the rest goes the same way.

**Two questions of mine dissolved when the rule was applied**, which is worth
recording because it is what the rule is for:

- *"Should the host also serve `/dev/window/<n>/rgb`?"* — malformed. `rgb` in
  the kernel is already wrong; it is a raster file. The question was only ever
  where it lands, and there is one answer.
- *"Should the kernel forward draw ops through `HostOp`?"* — struck out. It
  grows the kernel.

### What is still genuinely open

| | |
|---|---|
| **A shared rasteriser, or native per host?** | `hosts/macos` and `hosts/browser` are both Rust and could share a crate; `rustkern.mjs` needs JS, with the frozen oracle's `poc/supervisor/draw.mjs` (220 lines) as a readable reference. Sharing is much cheaper; rendering natively — Core Graphics, Canvas2D — is the point of moving it. **These pull opposite ways and the answer decides how large a3 is** |
| **Chattiness** | libdraw issues many small operations; today each is a direct call. Whether a per-op crossing needs batching is **unmeasured**, and worth measuring against real acme rather than designing around |
| **What `#w` leaves behind, if anything** | a window still needs an identity a process can name. Whether that is a thin kernel device or nothing at all is undesigned — and the rule says to try nothing first |

**None of this is urgent.** a3 is heritage work for acme and samterm; the demo
carries text and the host renders it, so nothing waits on this.


## GAP — where the ramfs goes, and what answers `/` at boot

**Undesigned, named 2026-09-03** by auditing the kernel against *"process
orchestration only"* ([RESEARCH §9.11](../RESEARCH.md)), then sharpened by
reading how Plan 9 actually does it ([§9.12](../RESEARCH.md)).

**Plan 9 splits two roles that `#M` currently combines.** Its kernel root
device `#/` is **261 lines, a fixed 32-entry table, mode 0555**, and writing to
it is `error(Egreg)` — it holds `/boot` and empty mount points and nothing
else. The **real** filesystem is a user program (cwfs, kfs, fossil) or a remote
machine, attached over 9P by `bootrc` and bound over `/`. Our `#M` is both at
once, read-write, with `#V` snapshots on top.

**So the question is not "should the ramfs leave" but "where is the seam".** A
tiny read-only bootstrap root has Plan 9's own precedent for staying. The
read-write filesystem does not — and if it leaves, something must answer `/`
before anything runs: the host through `#Z`, or a userspace file server started
before any process needs a filesystem, which is a bootstrap **ordering**
problem rather than a relocation.

**No design exists. This is a gap and it wants a proposal before it wants a
review.** It is not urgent: nothing on the demo's path depends on moving it.

---

## Resolved, and therefore gone

Four proposals were made and settled on 2026-09-03: **that the host renders
`/dev/draw`** (the kernel does not know how to draw — the shape of the raster's move, after the demo, with
three questions inside it still open above); **how the window tree is
observed from outside emca** (it is files, at `/dev/emca/<n>/`, positional kids
— the founding principle leaves no exemption for a window manager), **where
emca's file server
is posted** (`/srv/emca.<user>.<pid>` — `#s` is one table for the whole kernel,
so a fixed name collides the moment emca nests) and everything proposed on
2026-09-02 — the manager interface, the `/type` file syntax, `properties`,
`pkg`/`template`/`project`, `/store`, `inode/system`'s layout, the `shell` type
and `inode/directory`'s listing. All of it moved into the specs; the reasoning
is dated in [design.md](design.md).

*A proposal is written here, reviewed, and then **leaves**. Adding to this file
instead of emptying it is how stale blocks accumulate and how a reader ends up
re-reading settled material to find what actually needs them.*
