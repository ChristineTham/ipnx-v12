# The personas — who it is for

**Role: the *who*.** Who the system is for — the personas, their jobs, needs,
wants and pain points. [identity.md](identity.md) answers who a user *is*
inside the system; this document answers who the system is *for*.

> **Personas are EXTRINSIC. They can never be made stale by a design change or
> an implementation change** (Christine, 2026-09-02). A persona is an archetype
> described *from their own perspective*: their job, their needs, their pains
> exist in the world whether or not this project does. An educator wants a
> readable operating system for a class whether or not anyone writes one.
>
> **The test:** delete this project from the universe and read the card again.
> If anything changes, that line was written from the system's perspective and
> is in the wrong document. So **no persona card may name a milestone, a line
> count, a URL, a version, or anything carrying a date.** What the system has
> built is [when.md](when.md); which milestone serves whom is
> [implementation.md](implementation.md).
>
> These cards do change — people change, and we may learn we were wrong about
> them, or decide to serve someone new. Those are **extrinsic** changes, driven
> by evidence about people, never by system changes.

**Validation status, stated honestly** (measure-don't-assume, applied to
people): **P1 is live user research** — the author is in the room, and this
repository's history is the interview record. **P2–P5 are assumption
personas**, built from communities observed at a distance; each carries a named
validation event, and an insight from an unvalidated persona is a hypothesis,
not a finding.

## P1 — the author-operator *(validated: live)*

Every system in this lineage served its authors first: Unix at the patent
office, Plan 9 at the Labs. The author is the one persona whose test runs
without shipping anything.

- **Job**: think *in* the counterfactual Unix rather than argue it — the system
  as the workbench for its own development.
- **Needs**: persistence, because a system that forgets every boot cannot be
  lived in; a screen to work on; her own files reachable from inside.
- **Wants**: her editor as the daily editor; these documents written in the
  system they describe; the counterfactual *inhabited*, not demonstrated.
- **Pains**: the tools she most wants to live in cannot hold her real work, so
  the interesting system and the working system are two different systems, and
  the day is spent in the duller one.
- **How she adopts**: builds it → first inhabits it → reaches for it for real
  work and finds the walls → daily-drives it → the system hosts its own
  development.
- **Believes when**: this repository's documents are written and edited from
  inside the system.

## P2 — the Plan 9 diaspora and the Unix historians *(assumption; validate on first public exposure)*

TUHS, cat-v, 9front, the retrocomputing audience. They do not become residents;
they become the megaphone.

- **Job**: feel the real thing with zero commitment — arrive, get a shell, try
  the things they remember, leave.
- **Needs**: no installation, because a clone-and-build ask loses nearly all of
  them; authenticity they can verify themselves, at the source.
- **Wants**: the *feel* — the mouse chords, the structural regexps, the filters
  behaving the way their memory insists they behaved; something worth sharing.
- **Pains**: every revival of this lineage demands a VM or an ISO before it
  demands interest; the things that *are* one click away are usually tributes
  rather than the code; and the real article costs a weekend of setup friction
  to reach, which is more than curiosity will pay.
- **How they adopt**: see it mentioned, skeptical-curious → try it immediately
  or not at all → run their own authenticity tests → follow the source to check
  it is the real thing → write the post that brings the next thousand.
- **Believes when**: the software behaves the way they remember, and the
  original sources are one link from the running thing.
- **Validation event**: unsolicited posts and issues arrive from this community
  — or do not, which is equally data.

## P3 — the OS educator and student *(assumption; validate at first course use)*

The xv6/MINIX seat, with a real userland in it.

- **Job**: teach a real, complete operating system that a class can both read
  and run — the concepts demonstrable live in a lecture, not described.
- **Needs**: a kernel small enough to read in one sitting; no lab setup, since
  a lab that takes a week takes the semester; licence clarity, because material
  that cannot be redistributed cannot be taught.
- **Wants**: exercises that use the concepts rather than describe them; things
  that stay put across a semester; a syllabus arc from the first command to the
  whole environment.
- **Pains**: the teachable systems have toy userlands, and the real systems are
  unreadable at classroom scale — so students meet either a system too small to
  be true or one too large to see. The genuinely well-designed alternatives cost
  a week of setup friction that a course cannot spend.
- **How they adopt**: hear of it → evaluate the documents → trial one lecture →
  assign it → contribute exercises back.
- **Believes when**: a real exercise in the ideas being taught runs inside one
  class period, on the students' own machines, with no setup.
- **Validation event**: one real course adopts it.

## P4 — the agent-platform engineer *(assumption; validate at first external pilot)*

The industry persona, and the timing bet.

- **Job**: give an AI agent a computer without giving it *the* computer.
- **Needs**: confinement provable by construction rather than by configuration;
  an audit question answerable directly rather than reconstructed; startup fast
  enough to be per-task; operation that is automation-first — a file, not a
  wizard.
- **Wants**: tools made available the way filesystems are made available;
  per-agent identity that means something in an audit trail; images small
  enough to move.
- **Pains**: the sandboxes available today cannot answer "what could it have
  touched?", because an allowlist describes intentions rather than reach.
  Containers are heavy, and their isolation story rests on the attack surface
  of a large kernel. And there is no coherent per-agent identity anywhere, so
  every team invents one badly.
- **How they adopt**: meet the claim in agent-infrastructure discourse → read
  the model → pilot something small → integrate → advocate internally.
- **Believes when**: confinement is a few lines they can read, startup is
  under a second, and the audit question is answered by looking rather than
  by inference.
- **Validation event**: one external pilot integration.

## P5 — the self-sovereign personal computerist *(assumption; validate when an external profile exists)*

Local-first, self-hosting, one person across many devices.

- **Job**: own their identity as files — their configuration, their services
  and their keys, in a repository they hold; the same world on every device; no provider in the middle.
- **Needs**: a profile that is portable and readable; graceful degradation when
  something is unreachable, because the plane test is a real test; keys that
  never exist as readable files.
- **Wants**: a versioned, diffable, rollback-able self; delegation to agents as
  a subset of themselves; enrolment that feels like cloning a repository rather
  than enrolling in device management.
- **Pains**: their identity is smeared across dotfiles, a password manager and
  per-app sync, so no single thing *is* them; the providers own every join
  point between the pieces; and their devices drift apart because nothing
  authoritative describes what a device of theirs should look like.
- **How they adopt**: meet the idea in local-first circles → inspect the model
  → first fragment on one device → a second device → live in it, keys and
  agents included.
- **Believes when**: cloning their profile onto a fresh device produces their
  world.
- **Validation event**: a profile repository that is not Christine's.

## The non-users, named

Design thinking without a cut list is marketing. **Not for**: the
POSIX-compatibility seeker (refused at founding), the general-purpose desktop
switcher, the enterprise platform buyer, anyone needing Linux-binary
compatibility, the performance-first HPC user. **No Windows host** stands,
until a persona demands it with evidence.

When one of these asks, the answer is a pointer to the refusals in
[design.md](design.md), not a roadmap promise.

> **OPEN — the phone form factor has no valid answer right now** (2026-09-02).
> It was refused on the extrinsic ground that no persona's journey included a
> phone, then **reversed on 2026-08-31 on a design fact** — that the interface's
> breakpoints were measured in characters rather than pixels. That reasoning was
> invalid twice over: a design fact cannot decide who the system is for, and the
> fact itself was overturned on 2026-09-01 when the unit became
> device-independent pixels. **The reversal's evidence is gone and the question
> is genuinely open.** It can only be closed extrinsically: does any persona
> here need a phone, or is there a person we have not carded who does?

## A candidate persona, not yet earned

**The widening.** Every persona above assumes someone who already wants a
system of this shape: P2 is the diaspora, P3 teaches it, P4 and P5 want its
properties. There is plausibly an audience with none of that background —
someone who has never read the founding papers, who would use the system
because the interface makes its verbs visible rather than because they know
what it descends from.

That is a *new* audience and it deliberately has no card here: **a persona
without a validation event is a wish.** The event that would earn one is
concrete — **someone outside this tradition doing real work in the system and
saying so** — and until it fires, the widening stays a hypothesis.

---

The process that produced these cards — the POV statements, the ideation
record, the will/won't list — is [design-thinking.md](design-thinking.md).
