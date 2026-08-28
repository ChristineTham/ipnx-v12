# The personas — who it is for

**Role: the design-thinking half of *who*.** [identity.md](identity.md) answers
who a user *is* inside the system; this document answers who the system is
*for* — the empathy artifacts of the design-thinking iteration recorded in
[design-thinking.md](design-thinking.md): each persona's job, needs, wants,
pains, journey, and the belief test that converts them. First drafted
2026-08-29 informally; enriched the same day by the first formal iteration.
Revisited whenever the [deployment ledger](platforms.md) is.

**Validation status, stated honestly** (the project's measure-don't-assume rule
applied to people): **P1 is live user research** — the author is in the room,
and this repository's history is the interview record. **P2–P5 are assumption
personas**, built from communities observed at a distance; each carries a
named validation event, and an insight from an unvalidated persona is a
hypothesis, not a finding.

## P1 — the author-operator *(validated: live)*

Every system in this lineage served its authors first: Unix at the patent
office, Plan 9 at the Labs. The author is the one persona whose test runs
without shipping anything.

- **Job**: think *in* the counterfactual Unix rather than argue it — acme on
  macOS, real repositories mounted, the system as the workbench for its own
  development.
- **Needs**: persistence (a system that forgets every boot cannot be lived
  in); a screen (the native host is headless); her real files reachable.
- **Wants**: acme as the daily editor; these documents edited inside IPNX;
  the counterfactual *inhabited*, not demonstrated.
- **Pains today**: the native host paints into memory nobody shows; the
  rootfs is a seed, not a home; real work lives outside the walls.
- **Journey**: built it → first inhabited it (`run.sh -i`, `win acme` in a
  browser) → *reaches for it for real work and finds the walls* (the present
  stage, and the pain that names M3+M4 as a pair) → daily driving → the
  system hosting its own development (M10's git, `/cc`).
- **Believes when**: this repository's documents are edited inside IPNX.
- **Served by**: M3 + M4 (the pair the journey says to land together), M10.

## P2 — the Plan 9 diaspora and the Unix historians *(assumption; validate at the demo)*

TUHS, cat-v, 9front, the retrocomputing audience. They do not become
residents; they become the megaphone.

- **Job**: feel the real thing with zero commitment — click a link, get `rc`
  in a tab, chord in acme, leave.
- **Needs**: zero install (a clone-and-build ask loses nearly all of them);
  authenticity they can verify (the sources one click from the demo).
- **Wants**: the *feel* — mouse chords, structural regexps, `sed` behaving
  like their memory of it; a link worth sharing.
- **Pains today**: every Plan 9 revival demands a VM or an ISO; web "Plan 9"
  toys are usually tributes, not the code; this project's browser port is
  real and has no URL.
- **Journey**: sees a post (skeptical-curious) → clicks the demo *(today:
  there is none — the 99% bounce)* → pokes rc and acme, runs their own
  authenticity tests → follows the source link to the verbatim tree →
  writes the post that brings the next thousand.
- **Believes when**: hosted `sed` in a pipeline matches their memory; the
  verbatim sources are one link away.
- **Validation event**: the demo ships; unsolicited posts and issues arrive
  (or do not — either result is data).
- **Served by**: the public demo (+ the tour in the rootfs).

## P3 — the OS educator and student *(assumption; validate at first course use)*

The xv6/MINIX seat, with a real userland in it.

- **Job**: a real, complete OS a class can read and run — namespaces, 9P,
  fork, per-window `/dev/draw` demonstrable live in a lecture, from a URL.
- **Needs**: a kernel readable in one sitting (held: ~4,000 lines); no lab
  setup (the demo URL); licence clarity for classroom redistribution (held:
  MIT + NOTICEs).
- **Wants**: exercises with the concepts ("make your shell's `/bin` differ
  from your neighbour's"); stable URLs across a semester; a syllabus arc
  from `cat` to acme.
- **Pains today**: xv6 is real but its world is a toy; Linux is unreadable
  at classroom scale; Plan 9 proper costs a week of setup friction; this
  project has the material and no front door.
- **Journey**: hears of it → evaluates the docs (the six roles are,
  accidentally, a course reader) → trials one lecture from the demo URL →
  assigns it *(pain: no exercise set — the tour's chapters are the seed)* →
  contributes exercises back.
- **Believes when**: a namespace exercise runs in the browser during one
  class period.
- **Validation event**: one real course adopts it.
- **Served by**: the demo + tour, M5 (teach the one kernel twice), the
  document set.

## P4 — the agent-platform engineer *(assumption; validate at the M9 quickstart)*

The industry persona, and the timing bet.

- **Job**: give an AI agent a computer without giving it *the* computer —
  today faked with allowlists, container soup, and gVisor.
- **Needs**: confinement provable by construction (the namespace IS the
  reachable world); an audit answerable in one command; boot fast enough to
  be per-task; automation-first operation (a file, not a wizard).
- **Wants**: tools mounted in like filesystems (MCP-shaped access as "mount
  this tree"); per-agent identity with an audit name; images measured in
  megabytes.
- **Pains today**: allowlist sandboxes cannot answer "what could it touch?";
  containers are heavy and their isolation story is a kernel's attack
  surface; no coherent per-agent identity anywhere; *this system's answer
  is designed and not yet runnable* (waits on M7–M9).
- **Journey**: meets the claim in agent-infrastructure discourse → reads
  [identity.md](identity.md)'s agent quadrant and the architecture →
  *blocked: cannot pilot today* (the honest present) → pilots the five-line
  sandbox (M9's quickstart artifact) → integrates via OCI/microVM →
  advocates internally.
- **Believes when**: a sandbox is five lines of namespace file, boots in
  under a second, and `cat` answers the audit question.
- **Validation event**: one external pilot integration after M9.
- **Served by**: M2, M7–M9, M12 — the ledger's named risk: strongest claim,
  least substance.

## P5 — the self-sovereign personal computerist *(assumption; validate when an external profile repo exists)*

Local-first, self-hosting, one person across many devices.

- **Job**: own identity as files — namespace, services, keys in their own
  git repository; the same world on tab, laptop and iPad; no provider.
- **Needs**: the profile portable and text (held: the profile decision);
  graceful degradation offline (the plane test); keys that never exist as
  readable files (use-don't-read, enclave-backed).
- **Wants**: a versioned, diffable, rollback-able self; agent delegation as
  sub-profiles; enrolment that feels like `git clone`, not device management.
- **Pains today**: identity is smeared across dotfiles, a password manager,
  and per-app sync; providers own the join points; devices drift apart —
  and *nothing of this project's answer is usable yet* (earliest touchpoint
  is M2).
- **Journey**: meets the profile idea in local-first circles → inspects the
  model (a profile as a git repo resonates) → first fragment on one device
  (M2) → the second device (M9's enrolment; the plane test) → lives in it,
  keys and agents included.
- **Believes when**: cloning their profile onto a fresh device produces
  their world.
- **Validation event**: a profile repository that is not Christine's.
- **Served by**: M2, M9, M6.

## The non-users, named

Design thinking without a cut list is marketing. Not for: the
POSIX-compatibility seeker (refused at founding), the general-purpose desktop
switcher, the enterprise platform buyer, anyone needing Linux-binary
compatibility, the performance-first HPC user — and, made explicit by the
formal iteration: **no phone form factor** (no persona's journey includes
one; iPad is the smallest screen here), and **no Windows host** until a
persona demands it with evidence. When one of these asks, the answer is a
pointer to the refusals in [design.md](design.md), not a roadmap promise.

## The coverage map

| milestone | serves | note |
|---|---|---|
| M0, M1 | all (substrate); P4-adjacent ops | the container is also CI |
| M2 | P5 (first fragment), P4 (sandbox syntax) | |
| M3 + M4 | **P1** | the journey says: land as a pair |
| **demo + tour** | **P2, P3** | finished software, missing URL — gap 1 |
| M5 | P3 (teach the one kernel twice), P2 | |
| M6 | P1, P5 | |
| M7–M9 | **P4, P5** | the timing bet; M9 ships the quickstart |
| M10 | P1, P3, P5 — and credibility for all | git is every persona's proof |
| M11, M12 | P4, ops | |

Every milestone serves someone concrete — the spine survives the exercise.
The full process that produced this page — POV statements, the ideation
record, the rederived will/won't list and its reconciliation with standing
decisions, the documentation review — is
[design-thinking.md](design-thinking.md).
