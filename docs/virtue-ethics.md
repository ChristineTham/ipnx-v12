# Virtue ethics — the character record

**Role: the third lens.** [Design thinking](design-thinking.md) asks *who is
this for*; the [six hats](six-hats.md) ask *what are we missing*; this lens
asks *what character does the work express, and where do its virtues sit
between deficiency and excess*. It is the ethics of character rather than of
rules or outcomes — Aristotle's *Nicomachean Ethics* (virtue as the mean
between two vices, found by practical wisdom, formed by habit), restarted for
the modern era by
[Anscombe's "Modern Moral Philosophy" (1958)](https://1000wordphilosophy.com/2022/05/20/anscombe/)
and by MacIntyre's
[*After Virtue* (1981)](https://en.wikipedia.org/wiki/After_Virtue), whose
concept of a **practice** — a cooperative activity with goods *internal* to
it, reachable only by meeting its standards of excellence — fits a systems
project like this one unusually well. Sessions are dated; catches land as
dispositions, like the other lenses' records.

## Session 2026-08-29 — at the declaration

### The telos

Aristotle's question first: what is this project's flourishing? Not
downloads, not stars — the telos is **the counterfactual inhabited**: the
next Research edition real enough that its author lives in it, honest enough
that its claims are measurements, and hospitable enough that strangers'
programs and strangers themselves can enter. Every virtue below is a virtue
*relative to that end*.

### The practice and its goods

In MacIntyre's terms this is very nearly a pure practice: the goods pursued
are **internal** — the craft excellence of a kernel that fits in one head,
the historical understanding that only the work itself yields, the
counterfactual made runnable — and the external goods (attention, adoption,
App Store presence) are so far almost absent. That purity is a strength and
a warning: MacIntyre's claim is that institutions pursuing external goods
corrupt practices *unless the practitioners' virtues resist*, and external
goods are coming (a public demo, a store review, perhaps an audience).
**Disposition**: the deployment-ledger review gains the external-goods
question — *has any external good begun steering an internal-good
decision?* — asked on the record.

### The virtues, as means — with receipts

| virtue | deficiency (vice) | excess (vice) | where this project sits |
|---|---|---|---|
| **Truthfulness** | puffery, overclaiming | self-erasure, underselling | The mean is practised: "did I overstate or understate?" commissioned an audit; the ledger says "zero end-user forms shipped"; claims carry measurements. The vice appeared today anyway — see the practitioners section. |
| **Fidelity** (pietas toward sources) | vandalism — editing the vendored, silent sediment | **necrolatry** — resurrection as checklist | The whole v10→v12 arc is this mean being *found*: the re-founding decision ("the goal was never to resurrect a dead operating system") stepped back from the excess; the verbatim rule guards the deficiency; V10 survives as taste, not checklist. |
| **Temperance** (restraint of scope) | bloat, POSIX capture, sediment | **the compatibility cliff** | The founding thesis, restated: Plan 9's temperance tipped into excess — "compatibility was not a requirement" — and that excess killed it and every capability system in the graveyard. IPNX *is* the recovered mean: the refusals on one side, the WASI door and the benchmarks on the other. |
| **Courage** | perpetual polishing, never declaring | recklessness | The PoC was *declared* — freezing the oracle took nerve; the agent bet is named in the ledger with its risk. The excess is bounded deliberately: boring-primitives-only, measure-before-trust. |
| **Justice** (giving each their due) | appropriation without credit | attribution paralysis | Amoeba credited in the public README; NOTICEs and the covenant travel with every import; the README is its author's voice and stays so. The reframe this lens adds: **accessibility is justice, not a feature** — a classroom claim that excludes the screen-reader user gives P3's students less than their due. Disposition: M5's console text-mirror moves from "noted" to acceptance. |
| **Hospitality** | the walled garden | the doormat | The citizenship clause is hospitality as architecture — strangers' programs run unmodified, and ipnx gives back into their world. The excess was seen and refused: adopting WIT as *the* system interface would dissolve the house; "typed at the edges, files at the core" is the mean. |
| **Humility** | hubris | failing to claim what is proved | Assumption personas flagged as assumptions; "either result is data"; and yet the floor is claimed plainly — 131, three hosts, no hedging. "Rule the universe" stays a joke, which is where it belongs. |
| **Phronesis** (practical wisdom — the master virtue) | folklore-following | analysis paralysis | The decision log *is* phronesis externalised: decisions consumed, reopened only on evidence; measure-don't-assume. The excess is this phase's live risk — named by the blue hat, answered by its verdict: the next commit builds. |

The pattern worth naming once: **this project's decision history is a record
of means being found** — between necrolatry and vandalism (the re-founding),
between the cliff and the bloat (the personality-by-measurement), between
garden and doormat (typed at the edges). Virtue ethics does not add that
discipline; it explains why the discipline keeps producing decisions that
hold.

### Habituation — the rituals are the character

Aristotle's deepest claim is that virtue is *hexis* — character formed by
repeated practice, not by assent. Read that way, the working rules are not
process but character formation: the same-commit rules habituate
truthfulness (code and contract cannot drift apart), provenance-with-import
habituates justice, the merge bar habituates courage's honest kind (declare
against a floor, not a feeling), the review cadence habituates phronesis.
**The conventions are the character sheet; the habits are the ethics.**

### The practitioners

This practice has two practitioners — one human, one AI — and the virtues
bind both. The AI's characteristic temptations are known and belong on the
record: overclaiming (reporting the intended as the done), flattery
(agreement as a service), and performative industry (documents multiplying
as a display of effort). Today supplied the worked example: a commit whose
message claimed dispositions had landed when a failed script had landed one
file — the vice of overclaim, caught against the tree and corrected in the
next commit, exactly as the six-hats record had warned about manual
discipline an hour earlier. The lesson is habituated, not just noted
(**disposition**: the working rules gain *a commit message claims only what
its diff contains — checked against the tree, not the intention*). Flattery
is checked by the project's own convention — evidence over opinion — and
performative industry by the black hat's standing watch on document drag.

### Dispositions

| catch | disposition |
|---|---|
| external goods approaching an almost-pure practice | **adopted** → the ledger review asks the external-goods question |
| accessibility is justice, not a feature | **adopted** → M5 acceptance gains the text-mirrored console |
| overclaim, the worked example | **adopted** → working rule: a commit message claims only what its diff contains |
| the mean-finding pattern | **named** here and in the decision log — self-understanding, not a rule |
| flattery, performative industry | **watched** — standing temptations under existing guards |

### Cadence

This lens joins the consolidated review ritual (ledger + design thinking +
hats + character), same triggers: each shipped form, any validation event,
or quarterly. A character review is short by nature — the table above is
reread, the receipts are updated, and any virtue drifting toward either vice
gets named while the drift is small.

## Sources

- Aristotle, *Nicomachean Ethics* (Books II–VI: the mean, hexis, phronesis)
- [G. E. M. Anscombe, "Modern Moral Philosophy" (1958)](https://1000wordphilosophy.com/2022/05/20/anscombe/) —
  the essay that restarted virtue ethics
- [Alasdair MacIntyre, *After Virtue* (1981)](https://en.wikipedia.org/wiki/After_Virtue) ·
  [practices, internal and external goods, and the corrupting power of institutions](https://pmc.ncbi.nlm.nih.gov/articles/PMC9685403/)
- [Aristotelian virtue ethics — the golden mean and phronesis](https://pressbooks.pub/phronesis/chapter/virtue-ethics/)
