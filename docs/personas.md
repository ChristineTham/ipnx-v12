# The personas — who it is for

**Role: the design-thinking half of *who*.** [identity.md](identity.md) answers
who a user *is* inside the system (person, role, agent, network person); this
document answers who the system is *for* — the people whose jobs it does, what
each needs to see before believing, and where the milestone spine serves or
misses them. First done 2026-08-29, the day the set was completed; revisited
whenever the [deployment ledger](platforms.md) is (the two reviews ask the same
question from opposite ends: the ledger asks *where it runs*, this asks *for
whom*).

The method's honest premise: until now every decision here was derived from
constraints, measurements, or principle — never from a person. That mostly
produced the right plan (the coverage map below validates the spine), but
personas exist to catch what constraint-driven design cannot see, and they
caught two things (the gaps, at the end).

## P1 — the author-operator

Every system in this lineage served its authors first: Unix at the patent
office, Plan 9 at the Labs. The author is the one persona whose test runs
without shipping anything.

- **Job**: think *in* the counterfactual Unix rather than argue it — acme on
  macOS, real repositories mounted, the system as the workbench for its own
  development.
- **Today's alternative**: macOS plus terminal plus editor; plan9port for the
  homesick moments.
- **Believes when**: this repository's documents are edited inside IPNX.
- **Served by**: M3 (the app), M4 (real storage), M10 (git).

## P2 — the Plan 9 diaspora and the Unix historians

TUHS, cat-v, 9front, the retrocomputing audience. They do not become
residents; they become the megaphone.

- **Job**: feel the real thing with zero commitment — click a link, get `rc`
  in a tab, chord in acme, leave.
- **Their specific skepticism**: "real Plan 9 code, or a tribute act?" — the
  verbatim-sources rule is aimed exactly here, and the answer must be one
  click away, not a clone away.
- **Believes when**: `sed` in a pipeline in a hosted tab behaves like their
  memory of it; the sources are one link from the demo.
- **Served by**: the browser port (finished, frozen) — *if it is hosted*
  (gap 1 below).

## P3 — the OS educator and student

The xv6/MINIX seat, with a real userland in it.

- **Job**: a real, complete OS a class can read — a ~4,000-line kernel where
  namespaces, 9P, fork and per-window `/dev/draw` are demonstrable live in a
  lecture, from a URL, no lab setup.
- **Today's alternative**: xv6 (real but tiny world), MINIX (aging), Linux
  (unreadable at classroom scale).
- **Believes when**: a namespace exercise ("make your shell's `/bin` differ
  from your neighbour's") runs in the browser during one class.
- **Served by**: the demo (gap 1), the document set (the six roles are,
  accidentally, a course reader), M5 (the Rust core in the tab — one kernel
  to teach twice).

## P4 — the agent-platform engineer

The industry persona, and the timing bet.

- **Job**: give an AI agent a computer without giving it *the* computer —
  today faked with allowlists, container soup, and gVisor.
- **What this system uniquely offers**: the agent's whole visible world
  assembled from binds (nothing else reachable *by construction*), the audit
  log as a mount table plus iostats, an agent identity as a sub-profile —
  [identity.md](identity.md)'s agent quadrant productised.
- **Believes when**: an agent sandbox is five lines of namespace file, boots
  in under a second, and the audit question "what could it touch?" is
  answered by `cat`.
- **Served by**: M7–M9 (+M12) — the deployment ledger already names the risk:
  strongest claim, least substance; if the wave passes before `/net` lands,
  that is our Amoeba scenario.

## P5 — the self-sovereign personal computerist

Local-first, self-hosting, one person across many devices.

- **Job**: own identity as files — namespace, services, keys in their own git
  repository; the same world on tab, laptop and iPad; no identity provider.
- **Today's alternative**: dotfiles plus a password manager plus per-app sync
  — the profile decision's diagnosis is that the industry rebuilt the pieces
  and unified nothing.
- **Believes when**: `git clone` of their profile onto a fresh device
  produces their world; the plane test (unreachable NAS ⇒ degraded namespace,
  not a broken one).
- **Served by**: M2 (fragments), M9 (the whole profile), M6 (the iPad making
  "many devices" true).

## The non-users, named

Design thinking without a cut list is marketing. Not for: the
POSIX-compatibility seeker (refused at founding), the general-purpose desktop
switcher, the enterprise platform buyer, anyone needing Linux-binary
compatibility, the performance-first HPC user. When one of these asks, the
answer is a pointer to the refusals in [design.md](design.md), not a roadmap
promise.

## The coverage map

| milestone | serves | note |
|---|---|---|
| M0, M1 | all (substrate); P4-adjacent ops | the container is also CI |
| M2 | P5 (first fragment), P4 (sandbox syntax) | |
| M3, M4 | **P1** | the author test begins here |
| **demo** | **P2, P3** | finished software, missing URL — gap 1 |
| M5 | P3 (teach the one kernel twice), P2 | |
| M6 | P1, P5 | |
| M7–M9 | **P4, P5** | the timing bet |
| M10 | P1, P3, P5 — and credibility for all | git is every persona's proof |
| M11, M12 | P4, ops | |

Every milestone serves someone concrete — the spine survives the exercise.
The gaps are elsewhere:

## What the exercise caught

1. **P2 and P3 are served by software that is finished and unreachable.**
   Their entire journey is "click a URL, type into rc". The browser port is
   green and frozen; no milestone owned hosting it. Now one does:
   *the public demo* ([implementation.md](implementation.md)) — near-zero
   engineering, the highest reach-per-effort artifact available this year.
2. **The documents serve builders, not visitors.** Everything assumes a
   clone. The first five minutes — a landing line and a two-minute tour next
   to the demo — belong to the demo item, and the README's opening (hers)
   already carries the story once a URL exists to point at.
