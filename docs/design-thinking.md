# Design thinking — the iteration record

**Role: the process record of the design-thinking iterations.** The durable
empathy artifacts (personas with needs, wants, pains, journeys, belief tests)
live in [personas.md](personas.md); this document records how each iteration
ran — the method, the definitions, the ideas including the rejected ones, the
rederived scope and its reconciliation with the standing decisions, and the
documentation review — so the next iteration starts from evidence rather than
memory. Scope *changes* land where scope lives: the
[decision log](design.md).

## The method, adapted and cited

The frame is the Stanford d.school's five modes — Empathize, Define, Ideate,
Prototype, Test ([Design Thinking Bootleg](https://dschool.stanford.edu/tools/design-thinking-bootleg);
[the process guide](https://www.web.stanford.edu/~mshanks/MichaelShanks/files/509554.pdf)) —
run through the Double Diamond's diverge/converge discipline
([Design Council, 2005](https://www.designcouncil.org.uk/resources/the-double-diamond/)):
open wide on people and problems, converge on definitions; open wide on
ideas, converge on scope. Adaptations for a one-author systems project,
stated plainly:

- **One persona is live, four are assumptions.** P1 is genuine user research
  — the author in the room, the repository's history as the interview
  record. P2–P5 are proxy personas from communities observed at a distance;
  each carries a validation event in personas.md, and until it fires, that
  persona's insights are hypotheses. This is the project's
  measure-don't-assume rule applied to people.
- **Prototypes here are milestones.** A systems project's prototypes are its
  runnable artifacts — the demo is P2/P3's prototype, the M9 quickstart is
  P4's — so Prototype/Test map onto the milestone spine and its acceptance
  criteria rather than onto paper mock-ups.
- **Design thinking is evidence, not authority.** Where a persona-derived
  conclusion conflicts with a dated decision, it arrives as a *proposal in
  the decision log with the persona evidence attached* — never a silent
  change. (This iteration produced zero such conflicts.)

## Iteration 1 (2026-08-29)

### Empathize — the evidence

**P1, live**: the README rejection ("I did not like your README and rewrote
it") — the system's public voice is personal, not manifesto; the security
question arriving *from writing the README* — inhabiting precedes trusting;
the profile request — portability is a felt need, not a feature idea; "let's
finish the poc, then rule the universe" — ambition with humour, wanting the
real thing. **P2–P5, proxy**: TUHS/cat-v/9front discourse (authenticity
skepticism, VM fatigue); xv6's course adoption pattern (real-but-toy-world);
agent-infrastructure discourse (allowlist sandboxes, audit anxiety); the
local-first movement's essays (files over providers). The resulting needs /
wants / pains tables are in [personas.md](personas.md).

### Define — the points of view

In the d.school's form (*user needs X because insight Y*):

- **P1** needs to daily-drive IPNX for real work, because a system whose own
  author will not live in it convinces nobody — including her.
- **P2** needs the verbatim userland one click away, because their belief
  hinges on authenticity tests they can run in minutes and will not install
  anything to run.
- **P3** needs a real OS small enough to hold in one head and runnable from
  a URL, because teaching operating systems is constrained by setup friction
  and unreadable scale, not by concepts.
- **P4** needs agent confinement that is provable by construction and
  auditable in one command, because allowlist sandboxes cannot answer "what
  could it touch?" and that unanswerable question is their daily risk.
- **P5** needs their computing identity as files they own, because
  portability today means re-assembling yourself on every device through
  providers who own the join points.

### Ideate — how might we, and what came of it

| HMW | ideas | disposition |
|---|---|---|
| …put real acme one click away? | host the frozen browser port | **adopted** — the public demo (already in the plan from the informal pass) |
| | an interactive **tour as an rc script in the rootfs** (`tour` at the demo prompt; chapters double as P3's seed exercises; a self-skipping test can run it) | **adopted** — folded into the demo item |
| | an embedded source browser | deferred — a link to the verbatim tree suffices |
| …make the kernel a course reader? | the six-role document set | exists |
| | a full exercise set / courseware | **won't** — the tour's chapters are the seed; courseware is a course's job, not an OS's |
| …make the sandbox the namespace? | the five-line **sandbox quickstart as M9's shipped artifact** (not just its acceptance test) | **adopted** — M9 amended |
| | an iostats audit demo | deferred to the curation sweep that ports iostats |
| …make a person a mountable tree? | the profile decisions | exist (M2, M9) |
| …make the native host livable? | M3 + M4 | exist — journey evidence: **land them as a pair** (sequencing note added) |
| — (divergence sweep) | a phone form factor; a Windows host; a community platform; an in-system compiler for teaching | **won't / won't / parked (not engineering) / already answered by `/cc`** |

### Converge — the rederived will / won't

Derived from needs first, then diffed against the standing plan and refusals.

**Will** (all but two were already on the spine — the spine survives):
everything in [implementation.md](implementation.md) M0–M12 stands, each
milestone now traceable to at least one persona (the coverage map in
personas.md); **new from this iteration**: the tour in the demo rootfs, and
the M9 quickstart as a shipped artifact; **resequencing evidence**: M3+M4 as
a pair (P1's journey).

**Won't** (every standing refusal was independently re-derived from the
personas — none needed persona evidence to survive, and none contradicted
it): POSIX-the-standard, Linux-binary compatibility, systemd-shaped boot, a
global identity provider, package management, self-hosted compilation,
in-kernel multi-user timesharing. **New explicit won'ts from this
iteration**: a phone form factor (no persona's journey contains one), a
Windows host (until a persona demands it with evidence), courseware beyond
the tour's chapters.

**Reconciliation**: zero conflicts with dated decisions; two additions and
three explicit won'ts recorded in the [decision log](design.md) (2026-08-29).

### Prototype and test — the validation plan

| persona | prototype | test (belief event) | earliest |
|---|---|---|---|
| P1 | M3+M4 build | these documents edited inside IPNX | M4 |
| P2 | the public demo | unsolicited posts/issues (either result is data) | demo |
| P3 | demo + tour | one real course adopts it | demo |
| P4 | M9 quickstart | one external pilot integration | M9 |
| P5 | M2 fragment → M9 profile | a profile repo that is not Christine's | M9 |

### The documentation review

Each document read through the personas it serves:

| document | lens | finding | action |
|---|---|---|---|
| README | P2/P3 first touch | sound (hers); awaits the demo link as a status fact | none now |
| design.md | all | scope matched the rederivation | decision entry added |
| architecture.md | P3, P4 | the contracts answer P4's provability question directly | none |
| handbook.md | P1, contributors | complete for its journey stage | none |
| implementation.md | all | two gaps | tour added to the demo; M9 quickstart clause; M3+M4 pairing note |
| platforms.md | P4, ops | the ledger already carries the timing risk | none |
| identity.md | P4, P5 | the agent quadrant is P4's landing page | none |
| personas.md | — | was cards only | enriched: needs/wants/pains, journeys, validation status |

### Cadence

An iteration reruns with each [deployment ledger](platforms.md) review (the
two ask the same question from opposite ends), and immediately after any
validation event fires — a validated persona's insights get promoted from
hypothesis to finding, and the scope diff reruns against real evidence.

## Sources

- [Design Thinking Bootleg — Stanford d.school](https://dschool.stanford.edu/tools/design-thinking-bootleg) ·
  [An Introduction to Design Thinking: Process Guide](https://www.web.stanford.edu/~mshanks/MichaelShanks/files/509554.pdf) ·
  [The 5 Stages in the Design Thinking Process — IxDF](https://ixdf.org/literature/article/5-stages-in-the-design-thinking-process)
- [The Double Diamond — Design Council](https://www.designcouncil.org.uk/resources/the-double-diamond/) ·
  [Framework for Innovation — Design Council](https://www.designcouncil.org.uk/resources/framework-for-innovation/)


## Iteration record: the canvas brainstorm (2026-08-30)

The modern-draw question sat queued for a day and was settled in one
five-round adversarial session — recorded here because the METHOD carried
it: Christine pushed the same challenge four times at increasing depth, and
each push produced a clause the decision needed.

1. **Opening reframe (hers):** abandon running raster sam/acme unmodified —
   "we are not trying to preserve the past; the real answer is how sam and
   acme would be implemented today." Cited NeXT: the display-is-PostScript
   leap was right, PostScript the wrong vehicle. This dissolved the
   compatibility-shaped three-tier hybrid of the first draft.
2. **The answer from the other direction:** the modern consensus (browser,
   SwiftUI) is a retained SEMANTIC tree + events — and Plan 9 already built
   its file-native form once: acme, whose clients never draw. Generalise
   acme, not libdraw. She named it: acme is less an editor than a canvas
   manager; `/dev/canvas`; surfaces generalised to browser/Mac/iOS/SVG/PDF.
3. **Push: "aren't we just describing SVG?"** → the flowed-text/editing/
   protocol deltas; the web needing both HTML and SVG as the empirical
   proof; the adopt-notation-own-model rule.
4. **Push: "aren't we describing HTML+SVG? HTML already has canvas."** →
   the concession made explicit (the tree is the web's discovery, adopted)
   plus the re-housing (files not JS-API; no behaviour in the surface;
   small enough that non-browser surfaces are peers); `frame` confirmed by
   the web's own quarantine of `<canvas>`; the dozen-kinds tripwire.
5. **Push: "isn't the surface basically WebKit?"** → yes, and that is the
   founding pattern: borrow the era's engines through a narrow waist (V8/
   wasmtime under the kernel; WebKit/TextKit under canvas; NeXT licensing
   Adobe's interpreter). The subfont saga stands as the measured proof that
   hand-rolling text stacks fails here.
6. **Push, to the thesis: "the modern ecosystem IS HTML/CSS/JS + wasm —
   are we not building for this world?"** → the ecosystem statement: the
   web platform is our VAX; built ON it and FOR its people, not OF its
   model; with the for-this-world commitments (embeddable surfaces, JS/TS
   client library, afternoon-learnable vocabulary).
7. **Her settlement**, agreed with two precisions: a protocol wearing the
   feel of a framework; and the browser realisation is ONE universal SPA —
   ours, singular, cached. Her closing correction scoped the refusal
   precisely: apps may understand the web (fetch, parse, generate HTML);
   it is /dev/canvas that never carries markup.

Naming en route: `draw` rejected for the pixel leaf ("draw implies a
pencil — a vector operation"); `frame` chosen (a framebuffer holds pixels
in a grid; video falls out). The full decision: design.md 2026-08-30.
