# Six hats — the parallel-thinking record

**Role: the completeness check.** De Bono's Six Thinking Hats (the 1985 book;
[the method](https://www.debonogroup.com/services/core-programs/six-thinking-hats/),
[overview](https://en.wikipedia.org/wiki/Six_Thinking_Hats)) runs all
perspectives over the same subject in sequence — facts, feelings, risks,
value, alternatives, process — so nothing hides in a blind spot. It complements
[design-thinking.md](design-thinking.md): design thinking asks *who is this
for and what do they need*; the hats ask *what are we not seeing*. Sessions
are dated below, newest first; what a session catches lands as dispositions
(adopted → the [decision log](design.md) and the
[plan](implementation.md); parked → named here so the next session re-asks).
The rituals share one cadence — see the closing blue hat.

## Session 2026-08-29 — at the declaration

**Subject**: the whole project, the day the PoC was declared, the documents
completed, and the first design-thinking iteration run. The question: before
building starts, what has been missed?

### 🔵 Blue (opening) — the frame

The thinking phase (declare → document roles → design thinking) has been
convergent and productive; the risk of the moment is *comfort in
meta-work*. This session's job: sweep once with every hat, adopt the small
catches, then stop thinking about thinking and build.

### ⚪ White — facts, and the facts we do not have

What we know is well recorded (131×3 hosts, the measured toolchain findings,
the licence chain). The gaps are the finding — **information that does not
exist anywhere**:

1. **Zero runtime performance numbers.** RESEARCH is rich in toolchain
   measurements and has no syscall round-trip, pipe throughput, fork
   latency, boot-to-init time, or per-process memory figure on any host.
   P4's belief test says "boots in under a second" — *we have no baseline
   for our own belief test*. Pulley's slowdown: flagged "measure before
   trusted", still unmeasured.
2. **One browser measured.** Everything browser-side says Chrome 148. The
   iPad WKWebView stopgap *decision* assumes the port runs under WebKit —
   SAB + COOP/COEP + `try_table` under Safari is **unverified**, and a
   dated decision rides on it.
3. **Suite coverage is uncharacterised.** 131 green says nothing about what
   is untested: error paths, resource exhaustion, unicode filenames (for a
   system whose founding adoption is "UTF-8 won"), deep walks, huge files.
4. **Toolchain versions are not pinned.** `mk.sh` uses whatever wasi-sdk,
   binaryen, bison and Node are installed; the measured findings (§9.4–9.5)
   are version-dependent and the versions are recorded nowhere.
5. **No CI exists.** Green-on-three-hosts is a manual discipline.
6. Smaller unknowns, named: the identity.md D1–D4 measurements (recorded
   deferrals); whether "IPNX" collides as a name (never searched).

### 🔴 Red — feelings, no justification required

Pride: it is *real* — acme in a tab still lands every time. The pull toward
building is strong and the day of documents, however necessary, fed a
producer's itch, not the maker's. Unease, named without argument: the agent
bet excites and worries at once; App Store review reads as dread; under
P2's "either result is data" armor sits the ordinary fear that no one will
care. A quiet one: everything runs on one person's enthusiasm, and
enthusiasm has weather. And a warm one: the namespace-as-sandbox story
*feels* true when said aloud to an imagined practitioner.

### ⚫ Black — caution, risk, what could go wrong

1. **Bus factor 1** — mitigated by the document set and the suite (a
   continuation is possible), unmitigated in operations (nothing runs
   without the author).
2. **The frozen oracle can rot.** Frozen means unfixable; Node 30 in 2029
   may break SAB/worker assumptions and the oracle dies of host drift. The
   fix is preservation, not exemption: **the oracle in amber** — a container
   with a pinned Node running `poc/` forever.
3. **Wasm platform drift.** The project already *measured* engines removing
   a feature (legacy EH). Asyncify is binaryen's, with the platform's
   attention on JSPI/stack-switching; wasi-sdk moves; unpinned toolchains
   plus measured-but-version-dependent flags is quiet breakage waiting.
4. **Timing-sensitive tests on slow hosts.** The raster tests poll with
   generous caps tuned on fast engines; Pulley and cheap CI runners may
   blow through them — flakiness that would erode trust in the merge bar.
5. **Trust boundaries are implicit.** The sandbox story is told in
   fragments (the microVM entry records "a runtime escape is a
   whole-system escape"); no single statement says what is trusted, what is
   not, and what a demo visitor, a wire mount, or an agent can reach.
6. **M8 will touch cryptography.** Ticket MACs are a classic
   roll-your-own footgun; the doctrine says modern MACs — the black hat
   says: boring, reviewed primitives only, and no cleverness.
7. **Licence surface of a public demo**: serving V10 binaries (covenant),
   CPython (PSF), Go runtime (BSD) publicly is almost certainly fine and
   *stated* nowhere a visitor can see.
8. **Accessibility**: canvas windows are invisible to screen readers; P3's
   classroom claim quietly excludes some of the class.
9. **The document set itself** — ten living documents with same-commit
   consistency rules is drag on every future change; accepted deliberately,
   watched (blue hat's cadence exists partly for this).

### 🟡 Yellow — value, why this works

The conformance-suite discipline is the project's superpower, already
cashed once: the entire kernel was rewritten in a different language in
two days *because* the spec is executable. The verbatim-sources rule is an
authenticity moat no tribute project can cross. Three external waves (wasm
everywhere, agent-sandbox demand, local-first) align with the personas
without the project bending toward any of them. The docs-plus-suite pair
makes the project continuable — the strongest available answer to the black
hat's bus factor. And the artifact-per-milestone cadence means value lands
continuously, not at the end.

### 🟢 Green — alternatives and new moves

Generated freely; dispositions in the table below. **CI as the container's
first job** (M1 becomes the machine that guards the floor — one stone, two
birds, and the same image preserves the oracle in amber). **A bench pass**:
a self-skipping measurement boot that prints syscall RTT, pipe throughput,
fork latency, boot time, per-process memory — numbers land in RESEARCH per
host, giving P4's belief test its baseline and Pulley its verdict. **A
`VERSIONS` record** the build checks and RESEARCH cites. **A WebKit/Firefox
measurement day** before the iPad stopgap is trusted. **A trust-boundary
statement** as an architecture contract. **A NOTICES page** on the demo.
**Text-mirrored console windows** (the xterm.js path already planned for
M5) as the accessibility answer. Parked with names: 9P-over-WebRTC for
tab-to-tab (P5, someday), the tour as man pages, a public dashboard of the
bench numbers.

### 🔵 Blue (closing) — dispositions and cadence

| catch | disposition |
|---|---|
| CI missing + oracle rot | **adopted** → M1 acceptance: a CI run on push via the container; the oracle preserved in a pinned-Node image |
| toolchain unpinned | **adopted** → M0 ships `VERSIONS`, `mk.sh` warns on drift, RESEARCH records the measured set |
| zero perf numbers | **adopted** → "The bench pass", a small standalone item; numbers to RESEARCH per host |
| WebKit unverified | **adopted** → a measurement gate on M6's stopgap claim (and noted beside the demo) |
| trust boundaries implicit | **adopted** → architecture.md gains "Contract: what is trusted" |
| demo licence surface | **adopted** → NOTICES beside the demo landing |
| timing-sensitive tests | **adopted (watch)** → the bench pass measures margins on slow hosts before CI trusts them |
| accessibility | **noted** → M5's engineering questions (text-mirrored cons); no promise before it |
| M8 crypto | **noted** → the milestone carries "boring primitives only" |
| doc-set drag | **accepted, watched** at each review |

**Cadence, consolidated** (so process does not multiply): one review
ritual, three lenses, run together — the [deployment ledger](platforms.md)
(where), a [design-thinking](design-thinking.md) iteration (who), and a
hats session (what are we missing) — after each shipped form, any
validation event, or quarterly, whichever comes first. And the meta-verdict
of this session: the thinking phase is complete; the next commit should
build.

## Sources

- Edward de Bono, *Six Thinking Hats* (1985) ·
  [the De Bono Group's method summary](https://www.debonogroup.com/services/core-programs/six-thinking-hats/) ·
  [Six Thinking Hats — Wikipedia](https://en.wikipedia.org/wiki/Six_Thinking_Hats)
