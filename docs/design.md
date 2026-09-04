# The decisions

**Role: a *what* — the decision record.** Every decision taken, dated, with the
constraint that forced it — reopening one requires new evidence, not a fresh
opinion. The purpose and intent behind them is [why.md](why.md); what the
system *is* is [architecture.md](architecture.md); what is *built* is
[when.md](when.md); the *plan* is [implementation.md](implementation.md);
*where* it runs is [platforms.md](platforms.md); *who* it is for is
[personas.md](personas.md).

## Decisions — the log

**118 decisions, 2026-08-26 to 2026-09-04.** Each entry states the
decision and the constraint that forced it. **Dated entries keep the words they
were written with** — reopening one requires new evidence, not a fresh opinion.

<details>
<summary>Index of all decisions, newest first</summary>

*Titles keep the words they were written with. Where a title itself was later
amended the row says so; where only part of an entry was overtaken, the entry
says what was replaced and by what.*

| date | decision |
|---|---|
| 2026-09-02 | emca is the WINDOW MANAGER; a type manager is what is IN a window |
| 2026-09-02 | emca NESTS, and nesting is another view of the system |
| 2026-09-02 | `su` becomes one line, because the flat `/home` makes it a namespace act |
| 2026-09-02 | `shell` needs a CHANNEL, not a store — and a conversation is not a file |
| 2026-09-02 | `shell` is a new paradigm: the body is an input |
| 2026-09-02 | `shell` applies to FILES, not just channels — and it is a role with many backends |
| 2026-09-02 | `run:` actions substitute ENVIRONMENT VARIABLES, not a templating syntax |
| 2026-09-02 | `ns` as implemented is retired; the need it served is real |
| 2026-09-02 | `/usr/<name>` is where an identity's FILES live — it is not an identity; `/home` binds to `/usr/<name>/home` — *title amended; see the entry* |
| 2026-09-02 | `/template` and `/project`: the declaration is a FILE, the instance is a DIRECTORY |
| 2026-09-02 | `/store` is proposed — the one genuinely new component |
| 2026-09-02 | `/pkg` is a LIST OF BINDINGS, not a directory — and `/profile`, `/pkg` and `/template` are ONE FORMAT, three registries |
| 2026-09-02 | `/output/<n>/` takes `/proc`'s shape — a number, with the command as a FILE |
| 2026-09-02 | `/home`, `/credentials` and `/profile` — three roots, one rule |
| 2026-09-02 | `/home` is the user's home, and it is a BIND |
| 2026-09-02 | `/home/<thing>` bound over `/<thing>` IS the configuration mechanism — there is no other |
| 2026-09-02 | `/dev` IS THE SLOT THAT CAN BE VIRTUALISED |
| 2026-09-02 | When a convenience verb earns its place |
| 2026-09-02 | What a type declares, and how a file is recognised |
| 2026-09-02 | Unrecognised or unrenderable content: fall back, but ask |
| 2026-09-02 | Two of the five tag line verbs are UNIVERSAL; three are implemented by the TYPE MANAGER — *title amended; see the entry* |
| 2026-09-02 | The two-step promotion needs NO mechanism, because the union already does it |
| 2026-09-02 | The three structural proposals are ENDORSED and moved into the specs |
| 2026-09-02 | The surface may not intercept keys a manager needs |
| 2026-09-02 | The shell is substitutable per NAMESPACE, and bash is a script interpreter rather than the interactive shell |
| 2026-09-02 | The documentation is refactored by document KIND |
| 2026-09-02 | The devices are Saranos's, and emca VIRTUALISES them |
| 2026-09-02 | The STATUS LINE and LAYOUT PLACEMENT, designed — the two gaps the bounded lens review found |
| 2026-09-02 | The New flow, traced |
| 2026-09-02 | Tag line beats selection, and it is safe because the tag line is VISIBLE |
| 2026-09-02 | TYPES ARE MIME TYPES. `text/plain` is the default; `/` is `inode/system` |
| 2026-09-02 | THERE IS NO REPLACE VERB |
| 2026-09-02 | THE WINDOW SYSTEM LEAVES THE KERNEL ENTIRELY |
| 2026-09-02 | THE TOOLBAR BELONGS TO THE MANAGER, NOT THE TYPE |
| 2026-09-02 | THE SIX TAG LINE VERBS AND THE WINDOW CONTROLS, landed precisely |
| 2026-09-02 | THE OUTERMOST WINDOW IS SPECIAL — its chrome is the host's — and `inode/system`'s manager owns a writable child |
| 2026-09-02 | Run, manage and shell are three different acts |
| 2026-09-02 | Run supplies a CONTEXT, never an implicit input |
| 2026-09-02 | MANAGERS ARE NAMED BY ROLE, and there are five |
| 2026-09-02 | Everything is SPEC'D, PROPOSED or a GAP — and the third state was missing |
| 2026-09-02 | Do not put configuration into a root that means something else |
| 2026-09-02 | CONVENTION RESOLVES; the system does not defend against deliberate misuse |
| 2026-09-02 | BARE `su` IS "EDIT THE SHARED HALF" — and `sudo` is it with a command |
| 2026-09-02 | ALL PROPOSALS ACCEPTED; the register is empty |
| 2026-09-02 | A type says what content IS; a manager says what to do with it |
| 2026-09-02 | A package is a FILE, measured against apt, ports and brew |
| 2026-09-02 | A manager's window is at `/dev/window/`, not directly in `/dev` |
| 2026-09-03 | A move out of the kernel runs both answers and asserts they agree |
| 2026-09-03 | A posted service name must be instance-qualified, because `#s` is global |
| 2026-09-04 | The suite's floor is what passes on the PURE kernel |
| 2026-09-04 | Replanned: three layers, the demo a milestone, the code legacy |
| 2026-09-03 | A deviation is authorised only for the substrate — and even then must be substrate-independent |
| 2026-09-03 | The kernel is a subset of Plan 9's and nothing more — the personality is userspace |
| 2026-09-03 | The device table is Plan 9's, and the contract is broken in five places |
| 2026-09-03 | The kernel does not grow, and that is a test applied before a design is written |
| 2026-09-03 | The host renders `/dev/draw`; the kernel does not know how to draw |
| 2026-09-03 | `/dev/draw` is for acme; the demo does not need it, so a3 is not the gate |
| 2026-09-03 | The window tree is files, because the founding principle leaves no exemption |
| 2026-09-03 | When state moves, the refusals move with it |
| 2026-09-03 | Identity is the raster's; place is the window manager's |
| 2026-09-02 | A directory's roles: `look`, `edit`, `properties` — and `edit` IS the file manager |
| 2026-09-02 | A WINDOW'S CONTENT IS ALWAYS A FILE; `emcaopen` takes one argument. The demo's boundary is closed |
| 2026-09-01 | acme and emca are two documents about two things |
| 2026-09-01 | The layering, sharpened: Saranos is the OPERATING SYSTEM |
| 2026-09-01 | THE VERBS: the era's names, four surfaces with one operand each, and the floating bar restored |
| 2026-09-01 | THE VERBS SETTLE INTO THREE GROUPS, and two of them are universal |
| 2026-09-01 | THE COMPOSITOR: one object, composited recursively — and the redesign that should have come first |
| 2026-09-01 | TABS ARE THE DEFAULT, AND /output MIRRORS THE FILESYSTEM — *title amended; see the entry* |
| 2026-08-31 | emca is the user interface, and IPNX is a whole operating system |
| 2026-08-31 | `/dev/window` belongs to every program, and both halves watch it |
| 2026-08-31 | `/dev/canvas` was over-derived: the redesign into 9P, `/dev/window`, `/type`, and a canvas narrowed to drawing |
| 2026-08-31 | The system is named Saranos; IPNX is the kernel; emca is the interface |
| 2026-08-31 | The open questions, resolved in one pass — and what it revealed about them |
| 2026-08-31 | The demo runs the Rust core — and the kernel loses its last OS dependency |
| 2026-08-31 | The buffer contract: mirrored with detectable divergence, one undo stack, `/mnt/acme` merged, and versioning as policy |
| 2026-08-31 | Parity is measured against the running reference |
| 2026-08-31 | Editing is the surface's: emca implements sam, not a WYSIWYG editor |
| 2026-08-31 | Building the web surface answered the gating question and corrected the responsive rules' own reading |
| 2026-08-31 | Building emca amended two things the design had settled, and both amendments came from the code refusing to be written the stated way |
| 2026-08-30 | `/dev/canvas` — the display is a semantic file tree; the modern-draw question, decided |
| 2026-08-30 | The versioning layer, v1: a snapshot is a tree, restore is a bind — landed as `#V` |
| 2026-08-30 | The ultimate dev environment, and VSCode as a surface |
| 2026-08-30 | The succession rule: a name is inherited by passing the ancestor's tests |
| 2026-08-30 | The paper is the yardstick: acme answers its own literature |
| 2026-08-30 | The namespace's third dissolution: every process is a jail, a container and a microVM — "our computer is a network." |
| 2026-08-30 | The name on the door: IPNX, not "modified Plan 9" |
| 2026-08-30 | The input convention: roles in the tree, native grammar per platform, verbs never in the hardware |
| 2026-08-30 | The iPad surface, re-aimed: an app that launches WebKit over local files |
| 2026-08-30 | The design stretch: a distributed operating system |
| 2026-08-30 | The console amendment: AND, not XOR |
| 2026-08-30 | The compensation thesis: complexity grows where a primitive is missing |
| 2026-08-30 | No half-working |
| 2026-08-30 | Containerisation and orchestration, planned: a Dockerfile is a process file, the orchestrator is a file server, kubectl is `cat` and `echo` |
| 2026-08-30 | Compatibility, kissed goodbye — the userland is reimagined, and the verbatim world becomes the exhibit |
| 2026-08-29 | su without a superuser — the security landing |
| 2026-08-29 | What a "user" is — the conflation decomposed |
| 2026-08-29 | The virtue-ethics pass — character made explicit |
| 2026-08-29 | The toolchains are real — the time for shims is over |
| 2026-08-29 | The six-hats pass — the blind spots, caught and adopted |
| 2026-08-29 | The profile: identity's configuration, unified |
| 2026-08-29 | The porting inversion — personalities carry the environment, not patches on the source |
| 2026-08-29 | The package model — a package is a subtree, installing is binding |
| 2026-08-29 | The first formal design-thinking iteration — scope re-derived from personas, and the standing decisions survived it |
| 2026-08-29 | The demo is the product |
| 2026-08-29 | The PoC is declared complete; the implementation begins |
| 2026-08-29 | Immutable systems and time travel are namespace operations |
| 2026-08-29 | Dependency hell and package conflicts dissolve in the namespace |
| 2026-08-29 | Capability doctrine, learned from the graveyard |
| 2026-08-27 | iOS local files: always a file server over user-granted subtrees — and the browser sandbox is not the barrier it looks like |
| 2026-08-27 | The native-to-the-new-world aspirations: cloud-native, AI-native, Kubernetes-native — stated now, sequenced later |
| 2026-08-27 | The native host is a Rust kernel core plus per-platform embedding shims — after the PoC completes |
| 2026-08-27 | The engine matrix: wasmtime everywhere, mode per shim; WasmKit stays superseded |
| 2026-08-27 | The dev toolchain: mk, diff, `/cc` as a capability — and the document factory is Plan 9's |
| 2026-08-27 | The curation principle: the Plan 9 command set is in scope entire, because it is the designers' own testimony about Unix |
| 2026-08-27 | The completeness principle for V10 — and upas resequenced, not refused |
| 2026-08-27 | The citizenship clause: ipnx lives in the wasm world in both directions |
| 2026-08-27 | Storage in containers: the invariant and the design |
| 2026-08-27 | OCI is two targets, taken at two different weights |

</details>


- **(2026-09-04) THE SUITE'S FLOOR IS WHAT PASSES ON THE *PURE* KERNEL — AND
  THE FROZEN ORACLE IS THE PoC's RECORD, NOT A CONSTRAINT ON THE KERNEL.**
  P1 step 0, the decision the plan holds open before any test is touched.
  Christine, choosing the plan's own proposal.

  **The constraint that forced it.** The 131 were declared on 2026-08-29 as a
  permanent floor, and the rule that the count only grows was right while the
  system only grew. P1 does not grow it: the purity rule
  (2026-09-03) makes the kernel *shed* features, and **15 of the 164
  assertions assert what the kernel is losing** — 9 for `link`/`symlink`/
  `readlink`, 6 for the `Cred{euid,ruid}` identity model, both of them
  measured unauthorised deviations. A floor defined as a *number* would make
  those fifteen a reason not to purify, which inverts what the suite is for:
  **the tests assert what was built, not what was agreed.**

  So the floor is **what passes on the pure kernel** — a bar that moves with
  the design rather than against it. Three consequences, stated so they are
  not rediscovered:

  - **A deletion must name its assertions.** Removing a feature removes its
    tests *in the same commit*, and the commit says which and why. A silently
    shrinking count is the failure this decision is most exposed to.
  - **The frozen oracle stays frozen, and stops being a gate.** `poc/run.sh`
    is the PoC's record (`docs/poc.md`, 2026-08-29): it will keep passing its
    own 164 because `poc/` never changes. It is no longer evidence about the
    kernel, because it no longer runs the kernel's code.
  - **The three-host agreement survives, and narrows.** The two hosts that
    *do* run the Rust core — wasmtime and the browser — must still agree
    exactly, assertion for assertion. That is the invariant the 131 were
    protecting, and it is the one worth keeping.

  Build status, including the current number, lives in
  [when.md](when.md) and nowhere else.

- **(2026-09-04) REPLANNED: THREE LAYERS, THE DEMO A MILESTONE, THE CODE
  LEGACY.** Christine: *"I noticed you have been adding steps (M17, M18)
  rather than realising earlier phases have been invalidated by design
  decisions, so need to be redone. So let's replan properly, and restart
  implementation rather than continuing."* And the shape: *"a plan that
  gradually implements IPNX (kernel and userspace), then emca, then Saranos."*

  The constraint she named is real and it is the kernel's disease in the plan:
  M14's layout was superseded by M15, M15 by M17a, and the purity rule then
  invalidated M0's links, the identity model and the window device — and at
  each point a milestone was **appended** rather than the earlier one redone.
  **A plan that grows by accretion cannot be executed from, because nothing in
  it is ever finished.**

  **What the demo is:** *"a minimum viable proposition, not a final state …
  take what we have designed to date and prove that it can replace the
  previous demo. We will continue to design after the demo. Don't
  overengineer."* Concretely, **`ipnx` in a macOS terminal and the website**
  — and *"think of the demo as a milestone in the plan"*, so the plan runs
  past it to the end: *"when saranos and ipnx are fully implemented on all
  targets that we have mentioned: browser, macos app, ios app, container, Micro
  VM, real hardware devices."* The gaps between here and there are *"documented
  and filled as part of plan"* — and *"We don't need to design today, we have
  all we need for the demo, and that's what we are focusing on."*

  **What Layer 1 is:** *"no window, no mouse, no draw, no canvas. That's why
  window implementation cannot be in the kernel."* And *"WASI citizens (Go and
  Python etc.) need to be reimplemented as packages."*

  **What the code is:** *"treat existing code as legacy that needs to be
  refactored."* Not thrown away, not continued — refactored toward the design,
  measured against `plan9/`. The old plan is archived whole.

- **(2026-09-03) A DEVIATION IS AUTHORISED ONLY TO ADAPT TO THE EXECUTION
  SUBSTRATE — AND EVEN THEN IT MUST BE SUBSTRATE-INDEPENDENT.** Christine:
  *"Actual deviations from Plan 9 kernel are only authorised when it is to do
  with adapting it for WASM and WASI"*, and *"even then it should be done in a
  machine independent way as we may want a non WASM kernel in the future … for
  example, dis, or .NET CLR etc."*

  **Two tests, in order.** Is the deviation *forced by running on a VM at all*
  — not by wasm in particular? If not, it is unauthorised. If so, is it
  expressed so that **Dis or the CLR could satisfy it** without the kernel
  changing? If not, it is authorised in purpose and wrong in shape.

  **Applying them (measured, [RESEARCH §9.15](../RESEARCH.md)):**

  | | verdict |
  |---|---|
  | `ARGS` 200 — argv at start-up | **authorised.** No VM hands argv on a Plan 9 stack. Shape is generic |
  | `NOTEGET` 202 — collect a note | **authorised.** No VM takes an asynchronous upcall into a running instance. Shape is generic |
  | `AREAD` 210, `IOWAIT` 211 — park cooperatively | **authorised.** A VM without blocking syscalls must yield. Shape is generic |
  | **`AsySnap { snap, data_ptr, sp }`** in the public `Effect::Spawn` | **authorised in purpose, WRONG IN SHAPE.** `data_ptr` is a wasm linear-memory address and `sp` is wasm's `__stack_pointer` global. **Dis has no linear memory and the CLR has no stack pointer to snapshot** — fork means something else in both. It must become an **opaque continuation token the substrate defines** |
  | `#[cfg(target_arch = "wasm32")]` on **the clock** | **wrong in shape.** A compile-time branch on the substrate inside the kernel. Time should arrive the way everything else does — as an operation the embedding answers |
  | `link`/`symlink`/`readlink`, `Cred{euid,ruid}`, `DMSETUID` | **unauthorised.** None is forced by any VM; they are features and a Unix personality |

  **Why the second test matters as much as the first:** the substrate-forced
  deviations are the ones that look permanently justified, so they are the ones
  that quietly acquire wasm's shape. `AsySnap` is the instance — a legitimate
  need expressed in a form only wasm can satisfy, sitting in the interface every
  future host must implement.

- **(2026-09-03) THE KERNEL IS A SUBSET OF PLAN 9'S AND NOTHING MORE — THE
  PERSONALITY IS USERSPACE, INCLUDING V10's.** Christine: *"we are essentially
  implementing a micro kernel based on a subset of Plan 9, we should not be
  adding to it (even the Unix v10 personality should be userspace) … It is
  important to keep our kernel pure otherwise we will encounter serious issues
  extending the kernel"* — to a MicroVM on a hypervisor, then to real hardware
  (Raspberry Pi).

  **The audit is [RESEARCH §9.14](../RESEARCH.md).** Three additions, measured:

  | | |
  |---|---|
  | **`link`/`symlink`/`readlink`** (traps 60–62) | absent from Plan 9 **and** 9legacy. Plan 9 refuses links as a *position* — `bind` and `mount` are its answer |
  | **`Cred { euid, ruid }` + `DMSETUID`** | Plan 9's per-process identity is **one field**, `char *user`, with `iseve()` its only predicate. No euid, no ruid, no setuid anywhere in `9/port`. `DMSETUID` and `DMSYMLINK` are **9P2000.u — a Unix extension** — and Plan 9's bits are only `DMDIR DMAPPEND DMEXCL DMMOUNT DMAUTH DMTMP` |
  | **`Effect`'s thirteen variants** | the hosted boundary. Inferno `emu` is the precedent and it was never written down as one, exactly as `#Z` was not |

  **The root cause is the derivation's DIRECTION.** `docs/syscalls.md` is titled
  *"the kernel call list — derived"* and it derives from **V10's 68 routines**,
  dispositioning each as library, collapse, or kernel — which is how `link`
  became "C: kernel, trap 60". Running the derivation from the Unix personality
  *inward* guarantees the personality ends up in the kernel. **It must run from
  Plan 9's subset outward**, and anything a personality needs beyond it is the
  personality's problem, in userspace.

  **Why now rather than later:** on a hypervisor and on a Pi there is no host,
  so `Effect`'s callbacks have nothing to call and the "hosted kernel inverts
  the trust geometry" argument for euid/ruid ([identity.md](identity.md))
  evaporates — while the mechanism it justified remains. Every addition is a
  thing that must be carried onto hardware or removed there.

- **(2026-09-03) THE DEVICE TABLE IS PLAN 9'S, AND THE CONTRACT IS BROKEN IN
  FIVE PLACES.** Christine: *"the kernel was supposed to be a reimplementation
  of a subset of plan 9 kernel. it sounds like you have broken the contract.
  that needs to be rectified completely."*

  Measured against `plan9/sys/src/9/port/dev*.c` ([RESEARCH
  §9.13](../RESEARCH.md)): five devices match Plan 9 (`#c #e #d #p #s`);
  **`#M` and `#w` COLLIDE** — Plan 9's `'M'` is `mnt`, the mount driver, and
  `'w'` is `watchdog`; **`#H`, `#V` and `#Z` are invented**; **`#/` root is
  missing**; and the mount driver and pipe, both reimplemented, carry **no
  letter at all**.

  **The break is not really about letters.** Plan 9's kernel holds `devdraw`
  (2,218 lines) and `devmouse` (779) because it drives a framebuffer — but its
  window system, **`rio`, is 5,587 lines of USERSPACE**. `#w` bundled the
  raster, the window tree, the canvas, the chrome and the type registry into
  one kernel device. **We put rio in the kernel.**

  **So the rule is: a device exists here only if Plan 9 has it, means the same
  thing by it, and uses the same letter** — with the sole exception of the
  hosted boundary, which must be justified in writing against Inferno `emu`
  rather than assumed. **Rectification is therefore not a design exercise: the
  target is readable in `plan9/`.**

  M17a1 and M17a2 turn out to have been **restoring** this contract without
  naming it — moving the window tree to emca is what makes emca rio. Recorded
  so the remaining work is understood as rectification and not as invention.

- **(2026-09-03) THE KERNEL DOES NOT GROW, AND THAT IS A TEST APPLIED BEFORE A
  DESIGN IS WRITTEN.** Christine: *"you yourself said the kernel does not grow.
  The kernel only handles process orchestration. everything else is handled by
  host or userspace. Everytime you design a change to the kernel, the design is
  wrong."*

  The constraint is the project's own thesis — *the kernel unable to bloat by
  construction* — restated as something to run a design against rather than to
  admire afterwards. **If answering a question needs an addition to `kernel/`,
  the answer is wrong**, and the real one is in the host or in userspace. The
  kernel may shrink; that is the only direction it moves.

  **The instance that prompted it:** M17a3's "Design A" proposed extending
  `HostOp` with draw operations. It was put up as a legitimate option and
  argued against on other grounds — a second IPC beside 9P — when it should
  have been struck out on sight. Arguing against a kernel change on its merits
  is already the mistake.

  **And the rule reaches further than the raster.** Measured the same day: `#w`
  is **~1,100 lines, 22% of the kernel** — 745 in `wsys_*`, `win_*`, `cv_*` and
  `drawmsgs`, plus `draw.rs`'s 364 — and **none of it is process
  orchestration**. M17a1 and M17a2 took the tree out on exactly this reasoning
  before it was written down. The remainder goes the same way, which makes
  M17a3 *"the window device leaves the kernel"* rather than *"the rasteriser
  moves"*.

- **(2026-09-03) THE HOST RENDERS `/dev/draw`; THE KERNEL DOES NOT KNOW HOW TO
  DRAW.** Christine: *"/dev/draw should be rendered by host. the kernel does
  not know how to draw."* So of the two designs put up for M17a3, **the host
  SERVES the raster** — it is not a typed op channel the kernel forwards
  through, and `Effect::WinUpdate`'s finished pixels stop being a thing the
  kernel produces. It makes *"IPNX implements no renderers"* literal rather
  than relocated.

- **(2026-09-03) `/dev/draw` IS FOR ACME. THE DEMO DOES NOT NEED IT, AND a3 IS
  THEREFORE NOT THE GATE.** Christine, correcting the plan: *"we don't use
  /dev/draw — we use /dev/canvas"*, and *"we don't need /dev/draw for the demo,
  text is sent to host, which is responsible for rendering. /dev/draw is only
  needed for acme."*

  **The plan said otherwise and was wrong on two of the three programs it
  named.** Measured 2026-09-03: `con.c` has **zero** draw references; `win.c`
  has one and it is **a comment**; `emca.c` has **zero**; and `/rc/emca` and
  `/rc/emcaopen` touch draw nowhere. What actually opens the raster is **acme**
  and **samterm** — the heritage exhibit — plus the programs that prove them
  (`acmetest`, `drtest`, `samtest`, and `init.c` running them).

  The demo's path is **text**: `Effect::WinChrome` carries content, toolbar and
  tag, `Effect::WinText` carries bytes, and the host renders them natively.
  That is the whole of what a window needs to show a file.

  **Consequence for the sequence: M17b and M17d–h do not wait on a3.** The plan
  had every remaining stage behind "the gate"; the gate was only ever the tree,
  and the tree is out. a3 is heritage work — real, and not on the demo's path.

- **(2026-09-03) THE WINDOW TREE IS FILES, BECAUSE THE FOUNDING PRINCIPLE LEAVES
  NO EXEMPTION.** Christine: *"accept the proposal, then delete the kernel's
  tree."*

  The constraint is the project's own first rule — *everything is managed as a
  file, so there are no manager programs*. A **window manager** whose
  arrangement cannot be read would be exactly the manager program the design
  exists to abolish; the reasoning that makes a process table a filesystem
  makes a window tree one, and there is no reason the exemption would stop
  there.

  **emca serves the tree in the TOOL view only** — `/dev/emca/<n>/`, which
  [window.md](window.md) already sanctioned in shape as *"the full set: what a
  window tool reads"*. A manager's own `/dev/window/` gains **nothing**, so the
  contract is untouched and no manager can see the arrangement it sits in.

  Four names, and they are the kernel's own, so nothing is invented at the
  moment of moving: `parent`, `axis`, `alloc`, and `kids/<i>/` — **positional,
  because order IS the layout and `ls` sorts.** Naming children by window id
  would hide the arrangement in the one listing that should show it. Walking
  into `kids/<i>/` reaches that child's own directory, so the tree is navigable
  without a second vocabulary.

  **And with no emca running there is no tree** — which is correct rather than
  a loss: no window manager, no arrangement. A window still opens bare in its
  type's default pane, exactly as it always did.

- **(2026-09-03) WHEN STATE MOVES, THE REFUSALS MOVE WITH IT.** The constraint,
  found by a test that probes it: the kernel's `reparent` refused a window as
  its own parent and as a child of its own descendant. Neither is state; both
  are guarantees *about* state. emca's first tree kept the fields and dropped
  the refusals, accepted a self-reparent, and corrupted the tree — and a layout
  walk over a cycle does not terminate.

  **So a move enumerates what the old owner REFUSED, not only what it stored.**
  Measured in [RESEARCH §9.10](../RESEARCH.md), and it governs every remaining
  stage of the window system's departure.

- **(2026-09-03) IDENTITY IS THE RASTER'S; PLACE IS THE WINDOW MANAGER'S.** The
  constraint: one `nextwid` serves `#w/<type>/clone` — `acme`, `win` and `con`
  use it — and `newkid` alike, so the window id space is shared and emca cannot
  mint while the raster half is the kernel's.

  This dissolves M17a1's conclusion rather than answering it. **`clone` returns
  the id synchronously**, so emca asks the kernel for a window, receives an
  identity, and decides the structure itself. **The kernel supplies identity;
  emca decides place** — and the M17a2/a3 line runs exactly there, which is why
  a2 could land without waiting for the raster.

- **(2026-09-03) A POSTED SERVICE NAME MUST BE INSTANCE-QUALIFIED, BECAUSE `#s`
  IS GLOBAL.** Christine, on being told the srv table is one map for the whole
  kernel: *"if it is global you need to resolve collision."*

  The constraint, measured: `srv_posts` is a single `HashMap<String, SrvPost>`
  on the kernel (`kernel/src/lib.rs`), so a posted name is **system-wide** while
  every other name a process sees is namespace-local. **emca nests by design**,
  so a fixed `/srv/emca` is a collision by construction — the second emca fails
  to post and serves a door nobody can find.

  The name is therefore **`/srv/emca.<user>.<pid>`** — Plan 9's own answer, and
  already in this tree: the real acme posts `/srv/acme.%s.%d`
  (`plan9/sys/src/cmd/acme/acme.c:321`). Verified: two emcas post
  `emca.kitty.5` and `emca.kitty.9`. It is **removed on exit** — a posted
  channel outliving its server is a door onto nothing.

  **And the sharper half of the answer: `/srv` is not how a manager reaches
  emca at all.** A manager opens `/dev/window/` in its own namespace, which
  emca mounts for it — no name, no window id, nothing global. That is rio's
  shape, it is what [window.md](window.md) already specifies, and it has no
  collision to have. The posted name is the **external** door only: a window
  tool, a debugger, the suite. Naming the two doors separately is what made the
  fix obvious; treating the posted name as the interface is what hid it.

- **(2026-09-03) A MOVE OUT OF THE KERNEL RUNS BOTH ANSWERS AND ASSERTS THEY
  AGREE.** The constraint: the window system's departure (M17a) cannot be a
  deletion, because `acme`, `sam`, `con` and `win` reach the kernel's device
  today and 95 suite assertions ride on it. So each stage **adds** emca's answer
  beside the kernel's, and the suite pins the **equality** — M17a1's test drives
  a resize and asserts emca serves `0 0 900 600` and the kernel holds the same.

  **Two answers that agree can lose one; two answers never compared cannot.**
  That is what turns a2 from a rewrite into a deletion, and it is the general
  shape every remaining stage of the move follows.

  Two constraints found while doing it are recorded with their measurements in
  [RESEARCH §9.9](../RESEARCH.md): **`newkid` returns no window id**, so ids are
  the kernel's and a1 must precede a2; and a **`rect` written to the kernel's
  `wctl` never reaches emca**, while a `resize` on the window's `events` does —
  the second is the contract's direction and the first is the coupling a2
  removes.

- **(2026-09-02) THE OUTERMOST WINDOW IS SPECIAL — its chrome is the host's —
  and `inode/system`'s manager owns a writable child.** Christine, after three
  attempts to normalise it: *"I think the genuine solution to this is that the
  '/' type and the screen is genuinely special, it is not a normal window.
  That's an unescapable fact."* **When every route around an exception creates a
  worse one, the exception is real** — the three routes were a sliver of body
  beside children, an undecorated pane standing in for a body, and a second
  title bar under the native one.

  **The exception is exactly one thing: the outermost window's chrome belongs to
  the host** — the macOS title bar and menu bar, the browser's tab and toolbar,
  the iPad's furniture. Its **body is still children**, and allocation,
  alternation and the tree are untouched, so the invariant *every window has
  chrome and a body; the body holds either content or children* survives
  unaltered.

  **And the exception is the OUTERMOST SURFACE, not the type.** `inode/system`
  is ordinary; a nested emca opens its own `/` with the same manager and
  **ordinary emca chrome**, because its parent is an emca window rather than the
  host. Otherwise every nested system would claim the host's title bar and
  tier-1 nesting would break on first use.

  **`inode/system`'s manager owns a writable child as well as the composition.**
  A container's manager renders by *arranging*, but this one also needs
  somewhere to write, so it opens **one ordinary window** — conventionally
  `layout`'s first entry, `/` itself, listing the root. Nothing about bodies or
  allocation changes. The surface renders it natively: a **left pane** in the
  browser, a **collapsible sidebar** on macOS and iPadOS.

  **Two mappings fall out rather than being designed.** **Collapsing the sidebar
  IS `minimise`** on that window — the native gesture and the contract control
  are one operation. And the toolbars split without ambiguity: the **global
  toolbar or menu bar** carries the system's verbs, because the outermost
  window's content is the system; the **sidebar's own toolbar** carries the
  listing's. `inode/system` **specialises `inode/directory`** (recognised by the
  exact path `/`), so the listing comes by inheritance and the type adds
  `manage`.

  **This is where "chrome is the surface's" pays best**: `NavigationSplitView`,
  `.toolbar` and the menu bar mean the macOS surface is built from **native
  furniture rather than emulated chrome**.

- **(2026-09-02) A WINDOW'S CONTENT IS ALWAYS A FILE; `emcaopen` takes one
  argument. The demo's boundary is closed.** Hers: *"a window's content is
  always a file, but the host may choose not to render it as a file but as an
  image, structured/formatted text, a table, etc."* So `body` exists for every
  window, including one whose manager is host-side, and the mirror protocol
  (`insert`, `delete`, `seq`) is an **optimisation for keeping a host editor in
  step** — not a substitute for the file and not a second source of truth. A
  guest reading `/dev/window/body` gets the content whatever is drawing it.
  This is the rendering rule the system already uses everywhere: **the file is
  the truth, the drawing is the surface's.**

  **`emcaopen <path> [role]`** — one argument, because everything else is
  derived: the **type** from recognition, the **role** from permissions. The
  old form took a type first, which was right when a caller chose the type and
  is wrong now that content decides it. `role` is the single override, and
  `emcaopen /bin/rc manage` is the whole of "open a terminal" — **`manage` on an
  executable runs it**, so no part of `emcaopen` knows what a shell is.

  **And `/rc/emca` shrinks to one line.** An earlier draft had it opening the
  startup windows *while* `/type/inode/system/layout` declared them — two
  sources of truth for one fact. The **layout file wins**, because it belongs to
  the type and is overridable by binding. `/rc/emca` starts emca; emca opens `/`;
  `inode/system`'s `manage` manager reads `layout`. **Boot becomes a type's
  configuration rather than a script**, which is what *"the managers are already
  files"* was for.

- **(2026-09-02) The STATUS LINE and LAYOUT PLACEMENT, designed — the two gaps
  the bounded lens review found.** The review asked *"is the design enough to
  rebuild the demo?"* and answered **no, by two named gaps**
  ([reviews/2026-09-02.md](reviews/2026-09-02.md)). Both are now closed, and
  neither needed new machinery.

  **The status line reports state that is CONSEQUENTIAL and NOT OTHERWISE
  VISIBLE.** Both halves decide: the match count qualifies because 247 matches
  may be scrolled out of sight and typing changes all of them; **dirty** does
  not, because `Save`'s presence is already the indicator; **read-only** does
  not, because the *role* is in the title bar. **Fields are exceptions, not a
  dashboard** — UTF-8 and `\n` are never shown, anything else is, so an
  appearance is meaningful and a normal window's line is nearly empty. Format:
  `<key> <value>` per line to `/dev/window/status`, the same shape as `verbs`;
  the manager says what, the surface says how. **The directory case pays the
  rule off twice**: `pending 3 renames, 1 delete` gives *"Save shows the plan"*
  a home and turns a confirmation step into something continuously visible.

  **The layout is one file naming PATHS, with three rules and one keyword.**
  Each line is a path — type inferred, role derived — so a layout never says
  what a thing is. **Indentation nests, and the axis is not stated** because
  alternation already determines it; writing it would create a second source of
  truth. **`tabs` groups**, and is the only keyword, because tabs are otherwise
  the un-allocated *remainder* of the sizing heuristic and cannot be relied on
  to produce a shape you asked for. **A shell window needs no special case**:
  `/bin/rc` is an executable, `manage` on an executable runs it, and the window
  is `shell` on the resulting channel — the two-step already settled, so the
  format knows nothing about shells. `Reset` re-reads this file, which is what
  makes it *"restore the root window to its default"*.

- **(2026-09-02) THE SIX TAG LINE VERBS AND THE WINDOW CONTROLS, landed
  precisely.** Christine, after finding the documents disagreeing three
  different ways: *"We need to land on precisely what the window controls are
  and what the standard toolbar buttons are. We cannot have inconsistency."*

  **Tag line — six, always, in this order, in every window:**
  **`New` `Open` `Run` `Find` `Edit` `Add`**. Membership is standardised; the
  *manager* implements the meaning of New, Find and Edit, while Run, Open and
  Add are universal. `|`, `<` and `>` are syntax within Run — **there is no
  Pipe button** — and `:` and `#` are syntax within Find.

  **`Add` was mine and is now hers.** I invented it, never raised it, and it sat
  unendorsed in the specs until this review found it — *"you did not name Add in
  our discussions"*. She then **adopted** it: `Add` puts the tag line's text on
  the toolbar as a button, **writing to the manager's verbs file**, so unlike
  acme's tag the button *persists* — and your `/home/type` binds over the
  system's, so it is yours alone. That is how *"type Indent in the tag and it
  works"* becomes durable.

  **Window controls:** the **contract** names **close, minimise, maximise,
  duplicate** — style-neutral, since a floating implementation has no columns.
  **How many buttons a person sees is the implementation's**, and the tiled one
  renders duplicate as **three** — as column, as row, as tab. Hers: *"duplicate
  is three buttons on our current emca implementation but may change."*

  **The toolbar is deliberately NOT standardised** — it is the manager's, so
  `look` and `edit` on one type must differ.

- **(2026-09-02) ALL PROPOSALS ACCEPTED; the register is empty.** Christine:
  *"accept your proposals. review and update all documentation to reflect agreed
  design. Make sure stale decisions that have been overruled are not still there
  to confuse future readers."* Accepted and moved into [type.md](type.md): the
  **manager interface** as a file interface, the **`/type` file syntax**,
  **`properties`** as `edit` over the stat, the **pkg/template/project** shapes
  and recognition, **`/store`**, **`inode/system`**'s layout declaration, the
  **`shell`** type with line discipline host-side, and **`inode/directory`**'s
  listing format. Every open question inside them is decided at the lean stated
  in the proposal — including the **third registry level** for a directory that
  is also a project (keeping the inheritance that was the reason for choosing
  MIME), **one declaration named `template`** in a workspace (because
  `/proc/N/ns` is the live state and promote serialises from it), and
  **"anything still named"** as the prune retention policy.

  **And the sweep it required.** Stale claims were corrected in three live
  documents — `emca.md` still named `/mnt/emca` as the file interface, `when.md`
  recorded the kernel window device without saying it had gone, and README:186
  still said *"a package is a subtree under `/pkg`"*. Superseded *dated* entries
  keep their words but now carry markers: the watcher/gatekeeper property and
  its "default pane" degradation, `/output` mirroring the filesystem, undo
  living in emca, and `/mnt/emca` as acme's client path. **A dated log entry is
  history and keeps its wording; a live spec must simply be right.**

- **(2026-09-02) A package is a FILE, measured against apt, ports and brew.**
  Christine: *"Research existing package implementations before answering… That
  will tell you what is needed, rather than me guessing on your behalf."*
  Measured: Debian ships `control` plus **four maintainer scripts**, `md5sums`
  and `conffiles`; FreeBSD ports are **directories** (`Makefile`, `distinfo`,
  `pkg-plist`, `files/`); Homebrew formulae are **single files** with patches
  inline via `patch :DATA`. **Four of the five reasons a package is a folder do
  not exist here**: the maintainer scripts manage *mutation* and installing is a
  bind; `md5sums` guards drift and the store is immutable after verification;
  `pkg-plist` tells removal what to delete and removal is an unbind; `conffiles`
  stops your config being overwritten and your `/home/<x>` binds *over* the
  system's, so it never is. **Only patches survive**, and Homebrew shows they
  need not force a folder. So `/pkg/<name>` is a **file**.

  **But a TEMPLATE is a DIRECTORY** — corrected by Christine the same day: *"a
  template really is a proto project… What about .gitignore, README,
  package.json?"* Those cannot live in `/store`: its properties (immutable,
  verified, content-addressed, prunable) fit *fetched* content, while a skeleton
  is **editable source you iterate on**, and behind a digest, editing it changes
  its identity. **Promote decides it**: you promote a project — a directory with
  files — into a template, and promoting into a file loses the files. The
  industry is unanimous where it was split on packages: cookiecutter, GitHub
  template repositories, Yeoman, degit and `.devcontainer/` are all directories.

  **Converting an existing project into a template:** the hard part is
  *essential versus incidental*, which no mechanism can know — `docker commit`'s
  problem. So **promote copies faithfully and making it general is an edit**,
  which works here because the result is a **directory of files** opening in a
  system whose editor handles directories and text natively. Two halves, two
  sources: the declaration serialised from the live namespace, the skeleton
  copied from the project. **One heuristic is not a guess** — skip what
  `.gitignore` declares derived, since a project with one has already answered
  "what is generated" in a declaration its author wrote; plus `.git`, since
  history is not a skeleton. The declaration's own specifics — absolute paths, a
  personal bind, the project's name — come out by the same editing. Promote
  lands in `/home/template/<name>`, which is exactly where that tidying belongs
  before the deliberate publish to `/template`.

  **The principle underneath: BIND WHAT STAYS SHARED, COPY WHAT BECOMES
  YOURS.** A package's content stays shared, so it lives in `/store` and is
  bound; a template's skeleton becomes your files, so instantiating **copies**
  it — binding would mean editing your `main.py` edited the template's. Same
  declaration format, same registry shape, different outcome because the need
  differs — which is her rule, confirmed by measurement (the measured table and its sources are
  **RESEARCH §13**). **The finding exceeds the answer**: the format is small because it carries no compensations, which is
  *"complexity is compensation"* with the deleted parts enumerated.

- **(2026-09-02) `/store` is proposed — the one genuinely new component.** Once
  `/pkg/<name>` is a **declaration file** rather than a directory, the bytes it
  binds from need a home, and nothing in the design had one. Hers, twice: *"I do
  understand pkg needs to wrap up a collection of files but that is not what the
  package file actually is"*, and *"the store must be prunable."* Proposed in
  [proposals.md](proposals.md): `/store/<name>/<version>/`, a union like every
  other root (`/home/store` over the system's), **name-and-version as the
  interface** so content-addressing stays an invisible implementation choice,
  **immutable after verification** so a pinned digest never becomes a false
  claim, and **not package-specific** — a template's skeleton files need the
  same store. **Prune needs no new machinery**: reachability is readable (so the
  listing IS the dry run), safety is **unlink-while-open** on channels the
  kernel already refcounts, and pruning discards *materialisation, never
  intent* — a pruned entry refetches, because the digest is pinned in the
  declaration. Which is why it is categorically safer than `docker prune`, where
  losing an image can lose something unreproducible.

- **(2026-09-02) `/pkg` is a LIST OF BINDINGS, not a directory — and
  `/profile`, `/pkg` and `/template` are ONE FORMAT, three registries.** Hers:
  *"pkg is not a directory. it is a list of bindings… plus commands that may
  need to be invoked during install… It is actually very similar to template."*

  | | assembles | when its commands run |
  |---|---|---|
  | `/profile` | *your* namespace | at `su` / login |
  | `/pkg` | a *tool's* availability | at **install** — prepare |
  | `/template` | a *project's* world | at **instantiate** — start |

  Each is *a list of bindings plus commands*. **A package is nearly a template
  with no `cmd`**: its commands prepare the thing, a template's runs *in* the
  result — the Dockerfile `RUN`/`CMD` split appearing a second time in a place
  designed separately, where namespace directives build the world and the
  command runs in it. `/lib/namespace`'s own header already half-claims this
  (*"this file format is also the profile's namespace-fragment format"*); this
  extends it to all three, so **three formats do not need designing** and `pkg`
  stops being a subsystem.

  **Consequence for `su`:** `pkg install` under `su` appends to the **system's
  list**, not to a directory of files — cleaner than the earlier framing, since
  what the union decides is whose *declaration* grows. Every user then inherits
  the system's list plus their own.

  **To flag rather than change** (her document): README:186 says *"A package is a
  subtree under `/pkg`"*. Under this reading `/pkg/<name>` is a declaration and
  the bytes live wherever it binds *from*. The load-bearing claim — *installing
  is a bind* — is untouched; the wording would want a touch.

- **(2026-09-02) BARE `su` IS "EDIT THE SHARED HALF" — and `sudo` is it with a
  command.** Hers: *"su. followed by pkg install installs a package into the
  system namespace, which is inherited by every user… su creates a process
  where we can change the system namespace, credentials and profile"*, and
  *"sudo mk install works exactly as we would imagine."*

  **It is not about having less; it is about which union element your writes
  land in.** With no `/home/pkg` bound in front, the **MCREATE element is the
  system's** — so *"install for everyone"* is not a flag, a helper or a special
  operation, it is the same command in a namespace whose create element
  differs. `mk install` needs **no change and no `--user` flag**, which
  dissolves the flag pip, npm, gem and cargo all carry because they cannot say
  `bind`.

  **There is no privileged program.** Unix's `sudo` is a setuid binary that must
  be perfect — its CVE history is long precisely because it is privileged code
  parsing user input, and a flaw is total compromise. Here the authorisation
  happens once at namespace composition, under the eve/ruid rule, and the
  command runs with **no special status at all**: a bug in `mk` cannot escalate,
  because `mk` has nothing to escalate. Its reach is its namespace.

  **The power is real but LEGIBLE**, which root's was not: bounded by which
  create elements you hold, and readable — `cat /proc/N/ns` answers *what can
  this process change?* And the audit exceeds `sudo`'s log by accident:
  `/output/<n>/` already records `cmd`, `dir`, `status` **and the output**,
  which sudo never captures. Three transitions, three authorisations:
  `su none` (nothing), `su mimmy` (mimmy's), `su` (**eve's**).

  Root is retired without losing what root was *for* — the daemon environment,
  the system-wide install, changing what everyone inherits — all as consequences
  of *where writes land* rather than as exemptions from the rules.

- **(2026-09-02) `su` becomes one line, because the flat `/home` makes it a
  namespace act.** Hers: *"su mimmy is 'Become mimmy, create a process with
  /usr/mimmy mapped to home and mimmy's credentials'."*

  ```
  su mimmy  =  a fresh namespace
               bind /usr/mimmy /home
               apply /home/profile        ← now mimmy's
               set the credentials
  ```

  **The third step does every union root for free**, because the profile *is*
  the list of binds — `/bin`, `/lib`, `/type`, `/template`, `/pkg`,
  `/credentials` all reassemble from mimmy's half without `su` naming any of
  them. So **`su` is not a mechanism**: it is *"assemble someone else's
  namespace"*, which is what a profile is for. No setuid, no privileged helper,
  no `sudoers`, which is what identity.md asserted was possible without
  spelling out.

  **A caution, because the cheaper version looks like it works.** A bind
  resolves a **channel, not a path** — namespaces here are per-process mount
  maps with refcounted channels — so rebinding `/home` does NOT retarget
  `/bin`'s union element, which still points at the previous person's
  directory. That is why `su` is *a fresh namespace plus the profile* rather
  than one rebind, and it would pass a test that checked `/home` while failing
  in use, where you would be running kitty's `/bin` as mimmy.

  The direction rule is unchanged: **downward is free** — `su none` needs no
  permission — while becoming mimmy needs the eve/ruid check, since mimmy's
  credentials were never yours to bind.

- **(2026-09-02) `/home/<thing>` bound over `/<thing>` IS the configuration
  mechanism — there is no other.** Hers: *"and `/home/bin`, etc. all standard
  conventions."* Not three special cases for the registries but **one rule for
  the whole system**, and Plan 9's own practice generalised — its standard
  profile already does `bind $home/bin/rc /bin`.

  ```
  /home/bin  →  /bin        your commands, found first
  /home/lib  →  /lib
  /home/type /home/template /home/pkg  →  their system twins
  ```

  **What it dissolves** is larger than `PATH`, which the README already claims:
  `~/.config`, `XDG_CONFIG_HOME`, `/etc` versus `~/.foorc` precedence, per-
  application config directories, and *"where does this program look for its
  settings"* as a question anyone must answer. A program looks in `/lib`;
  whether that is yours or the system's is the **namespace's** business, not the
  program's. It also gives the profile a concrete job — the profile is a list of
  these binds, which is what `/lib/namespace`'s format already is and what
  identity.md means by *"namespace fragments describe what to assemble"*. And
  **`ls /home` answers "what have I customised?"**, which no dotfile system can:
  there you would have to know every application's convention and search for
  each.

  **`/usr/<name>` IS `/home` — flat — and `/profile` and `/credentials` are
  ordinary union roots.** Hers, following the inconsistency to its end: *"if
  there is a system /profile, then the user profile should genuinely be
  /home/profile. Which means we are back to /usr/kitty being a synonym for
  /home."* Correct, and it leaves **zero special cases**: every root is a union
  of the system's and yours, and every personal half lives at `/home/<x>`.

  ```
  /usr/kitty/           the person's tree; /home binds here
      bin/ lib/ type/ template/ pkg/     your half of those system roots
      project/ document/                 your work
      profile/                           your half of /profile
      credentials                        your half of /credentials
  ```

  **This retires `/usr/<name>/home`**, which I introduced when she asked whether
  it was a synonym, justified as separating *what you are* from *what you own*.
  That separation survives as **subdirectories** (`/home/profile` versus
  `/home/project`) and did not need a tree split, which cost the uniformity. It
  is also more conventional, not less: a Unix home has always held all three —
  `~/.ssh` is credentials, `~/.config` is profile, `~/Documents` is work. The
  permission argument is unchanged: someone may read `/usr/mimmy/project/foo`
  and never `/usr/mimmy/credentials`, different subtrees with different modes.

  *(An earlier draft of this entry called `/profile` and `/credentials`
  exceptions to the `/home/<x>` rule, then a rule "with one parameter". Both
  were wrong: there is one rule and no parameter.)* An earlier draft said so; Christine corrected it: *"you
  could argue there are system `/profile` and system `/credentials`."* Both
  exist. identity.md's profile design already assembles *"a base, a per-device
  section, a section per service"* — **the base is the system's**, and
  `/lib/namespace` is that base today. System credentials are equally ordinary:
  CA roots, host identity, service keys, which no user owns.

  So **the union rule is universal — `/x` is always the system's plus yours —
  and what varies is only where "yours" is STORED**: `/home/<x>` for workspace
  things, `/usr/<me>/<x>` for identity things. Hers, setting that parameter:
  **"home is a workspace."** Identity's personal half lives in the identity
  tree, not among your work. One rule with one parameter, not a rule with
  exceptions.

- **(2026-09-02) The two-step promotion needs NO mechanism, because the union
  already does it.** Hers,
  completing the pattern. Each personal root is bound `-b` over its system twin,
  and the union-list machinery is already built and tested: *"walks try elements
  in order, directory reads concatenate integrally, **creates land in the
  MCREATE element**."* So `ls /template` shows the system's and yours as one
  listing; a collision resolves to yours; and **`Promote` writes to
  `/template/<name>` and it lands in `/home/template/<name>`** without knowing
  there are two places. *"Promote to system level"* is then moving it between
  union elements — one visible operation, and the only step needing
  deliberation. **`/home/project` has no system twin**, because a project is
  always someone's. This also retires the special framing of *"bind your own
  `/type` over the system's"*: that is not an arrangement for the registry, it
  is the general shape, and `/home/type` is where yours lives.

- **(2026-09-02) CONVENTION RESOLVES; the system does not defend against
  deliberate misuse.** Hers, on whether marker-file recognition needs a
  tie-break when a directory matches several types: *"this is what convention
  buys. someone creating a template in `/home/project` is an idiot."* The path
  settles it, because a thing sits in exactly one place and **`New` creates
  projects at `/home/project/<name>`** — the location is *caused by* the type,
  which is why reading it backwards is valid here where deriving the default
  *role* from location was not (nothing about a path causes writability).
  Recognition is therefore **path first, `contains` as the fallback** for a repo
  cloned elsewhere — and the residue, a directory genuinely of two kinds outside
  the conventional roots, needs no new mechanism because the type is already
  shown in the title bar and switchable. **The general form: prefer a convention
  that makes the bad case obvious over machinery that makes it impossible.**

- **(2026-09-02) Do not put configuration into a root that means something
  else.** Caught by Christine twice in one day: `/mnt/emca` for a window's files
  (when `/mnt` is for trees you attached and `/dev` is the virtualisable slot),
  and `/profile/boot` + `/profile/breakpoints` as **bare files** in a root
  defined an hour earlier as the *identity* profile — where `boot` also **takes
  a name Unix already owns**: the bootfile, the kernel image, the loader.
  **A namespaced subtree under a root is fine; bare files in it are not**, and
  **a name Unix already uses is not available.** All three slips were the same
  shape — reaching for a plausible-sounding name without checking what it
  already carried. The startup file needed no new word: **`/rc/emca`** and
  **`/profile/emca`**, named for the program, the same convention on both
  sides.

- **(2026-09-02) `/home`, `/credentials` and `/profile` — three roots, one
  rule.** Each is a **bind** onto a subtree of `/usr/<me>`, so each means
  *"mine"* and an agent's are its own. Chosen over a container (`/me/home`,
  `/me/profile`) because **`/home` was already decided**, and a container would
  undo a settled path to gain symmetry with two that did not yet exist — and
  because `/me` was my coinage, not hers. They fit the root's existing style
  (every root is one meaningful word) and, being system-defined, do not
  reintroduce the mess `/home` exists to contain. **`/credentials` is a listing,
  not the mechanism** — a program uses a key by challenge and response, so
  secrets stay in the agent. **The rule that stops roots proliferating**: a
  subtree of `/usr/<me>` earns a root when a process needs it **by name** and
  would otherwise have to know whose it is.

- **(2026-09-02) `/output/<n>/` takes `/proc`'s shape — a number, with the
  command as a FILE.** Hers, after I rejected two of her forms and argued my way
  back to the first: *"you mentioned that /output could be served by emca which
  means its /output/XXX is the shortest where XXX could be just a number."*

  ```
  /output/7/0         stdin — WHAT WAS FED IN
  /output/7/1         stdout
  /output/7/2         stderr — captured separately
  /output/7/3         …and anything else the command held open
  /output/7/cmd       cat /template/Python   — full, unescaped
  /output/7/dir       /template              — where it ran
  /output/7/status    exit code
  /output/7/log  the session as it read, text/plain — what Run opens:

      % cd /template
      % cat Python
      ....
      exit 0
  ```

  **Descriptors by number, which is `/fd`'s own pattern** (`bind #d /fd`, *"dup
  by open"*) and generalises where `in`/`out` stop at two. **stderr stops being
  conflated** — acme's `+Errors` merges them, which is why `2>/dev/null` exists
  as a coping mechanism. **stdin is preserved**, which almost nothing does:
  `|sort` on a selection records *what was sorted*, so an output window is a
  complete record of a transformation rather than half of one — and `cmd`, `dir`
  and `0` together are everything needed to run it again. **The one loss is
  interleaving**: 1 and 2 captured apart cannot reconstruct the order they
  arrived, and a build log is read in that order. Since emca serves this it
  holds both streams with their arrival order, so it also serves
  **`log`** — the session as it read, which is what `Run` opens, with
  the numbered files underneath when you want one (`grep /output/7/2` for just
  the errors). **The prompt is rc's own `%`**, unchanged from `exec.c:906` — no
  configuration, and it signals correctly that this is not a Bourne shell. The
  directory is **not** in the prompt; emca emits a `cd` line, so the log is
  **self-contained**: paste it and it is complete, with `/output/<n>/dir` the
  machine-readable copy of a fact the log already states. Selecting a command
  line and pressing Run works, because Run takes the selection and you select
  the command rather than the prompt. It is also what a person pastes into a bug report, and
  it is complete, which three separate files are not.

  **And it unifies output with `shell`: an output is a COMPLETED
  CONVERSATION.** The `shell` role shows a transcript being written;
  `/output/<n>/log` is one that finished. Same format, same rendering,
  same property that selecting a path in it and pressing Open works — so a Run
  window and a shell window are not two designs but one window at two points in
  its life. `text/plain` throughout, so **`output` stays retired as a type**.

  **This supersedes the mirror** (`/output/usr/kitty/ipnx/mk all`), which broke
  on its own justification: **a command line contains `/` and therefore cannot
  be a filename.** My defence of it rested on examples that happened to avoid
  slashes — `mk all`, `cat Python` — and any command naming an absolute path
  breaks it, which is most of them. Escaping or substituting (`cat :etc:motd`)
  made every access require quoting, in a filesystem made typeable on purpose.

  **The pattern is the system's own**: `/proc/<pid>/` is a number with metadata
  as files, and an output is the residue of a process. So the fact the mirror
  encoded becomes just another file — and answers the question *better*:
  `grep -l /template /output/*/dir` finds what ran there without needing to know
  the path first, which navigating a mirrored tree cannot do. The window's title
  still reads the full command, because emca reads `cmd` and a title has no
  filename rules.

- **(2026-09-02) Two of the five tag line verbs are UNIVERSAL; three are
  implemented by the TYPE MANAGER.** *(AMENDED later the same day: Christine
  added a sixth, **`Add`**, so the split is three universal — Run, Open, Add —
  and three the manager's. See the entry above.)* `New` (instantiate a template; create a
  file in a directory), `Find` (text, filenames, a process list) and `Edit` (a
  sam command for text, `resize 1024x1024` for an image) are **the manager's,
  not the type's** — which matters because two managers of one type differ:
  `Edit` under `look` cannot mean what it means under `edit`, on the same file.
  **(Christine has had to make this correction three times: the toolbar, the
  verbs, and these. The type declares WHICH managers exist; the manager
  implements WHAT the verbs do.)** **`Run` and `Open` do not vary** —
  Run always runs the tag line in the window's directory with `$file` and `$dir`
  set, output to `/output`; Open always opens what the tag line names. So
  *"behaviour specified by the type definition"* holds for three of the five.

  **The property that gives: no window is a dead end.** Whatever you are looking
  at — a template, an image, a process table, a binary shown as bytes — there is
  a working command line with the subject already in the environment. `xxd
  $file` in a `look` window is how you get past what the manager chose to show
  you, and it exists in every window by construction.

  **`/output` keeps the directory**: `cat Python` in a template window lands at
  **`/output/template/cat Python`**, not `/output/cat Python` — two commands
  with the same text in different directories must not collide. *(SUPERSEDED
  hours later: a command line cannot be a filename. `/output/<n>/` with `cmd`
  and `dir` as files inside — see the entry above.)*

- **(2026-09-02) The New flow, traced.** Open `/template` — `inode/directory`,
  and **`look` falls out of permissions** since the system registry is not
  yours to write. **Open the template** — you do not merely select it, because
  `New` in the *directory* window means "create a new thing of this type here",
  which in `/template` is a new **template**. Once the template is the window's
  *content*, `New` unambiguously means *instantiate me* — hers, 2026-08-31:
  *"Opening a recipe shows the recipe and launches a recipe manager — one of its
  buttons is New that instantiates a process with that recipe."* Type a name in
  the tag line (never prefilled), press **New**, and the project is created at
  **`/home/project/<name>`**. **You see the spec before instantiating it**,
  which is the audit property arriving where it matters: *what will this touch*
  is answered by the window you are standing in.

- **(2026-09-02) `/usr/<name>` is where an identity's FILES live — it is not an
  identity; `/home` binds to `/usr/<name>/home`.** *(AMENDED later the same day:
  `/usr/<name>` IS `/home` — flat, no `home/` subdirectory — see the entry
  below.)* Hers: *"/usr/kitty/home is a
  synonym for /home?"* Yes. And she then caught a phrasing error of mine —
  *"/usr/kitty is an identity?"* — no: it is a **directory named after** one.
  The identity is credentials plus a namespace ([identity.md](identity.md)) and
  exists whether or not a directory does; an agent may have a full identity and
  no `/usr/<name>` at all, and `/usr/mimmy` may be a **mount of Mimmy's
  system**, in which case the identity is over there and this is a view, made
  readable by the per-attach identity at the mount rather than by the folder's
  name. Calling the directory "the identity" would make a *name* look like it
  confers something, against the standing rule that **names are for accounting;
  namespaces are for authority**. Even `credentials` is a *listing* — hers,
  2026-08-31: *"not exposing kitty's credentials in plaintext"* — the keys stay
  in the agent. What the directory does hold: `home/` (the workspace),
  `credentials`, `profile`; `/home` binds to the `home/` subtree only. That makes permissions natural rather than arranged: someone may read
  `/usr/mimmy/home/project/foo` and never `/usr/mimmy/credentials`, because they
  are different subtrees with different modes — not one tree with exceptions.

- **(2026-09-02) A manager's window is at `/dev/window/`, not directly in
  `/dev`.** Hers. Ten generic names — `ctl`, `events`, `size`, `type`, `role` —
  sitting beside `cons`, `draw` and `canvas` would collide and read as nothing.
  And it is **already this project's path**: `/lib/namespace` binds `#w` at
  `/dev/window`, and the archived device spec used it. The full set is
  **`/dev/emca/<n>/`** — named for the ROLE, so it survives the tiled
  compositor being replaced; rio's own word was `wsys`, which would point a
  reader at a different system. A manager sees only
  `/dev/window/`, so no window id appears in any path a manager uses and it
  cannot reach another window's files — they are not in its namespace.

- **(2026-09-02) `/dev` IS THE SLOT THAT CAN BE VIRTUALISED.** Hers: *"the
  convention is `/dev` can be virtualised."* Not "where devices live" — the
  place whose contents may be substituted underneath a process **without it
  being able to tell**. Without emca, `/dev/cons` and `/dev/draw` are the host's
  real screen and keyboard; with emca they are virtualised per window; the
  client cannot distinguish them. `/mnt` carries no such convention: a tree you
  attached stays the tree you attached. **So a window's files live in `/dev`,
  not `/mnt`** — a manager reading `/dev/window/rect` must not learn who provided it,
  which is what makes emca nest, keeps the no-windows CLI case working, and will
  make a floating implementation substitutable for the tiled one. rio, not acme,
  is the analogue — it serves `/dev/cons`, `/dev/mouse`, `/dev/wctl` with the
  full set at `/dev/emca/<n>/`, and this project already followed it: `bind
  '#w/N' /dev`, and a supervisor file named `devwsys.mjs`. **Each manager's own
  window is bound at `/dev/window`** — already this project's path, since
  `/lib/namespace` binds `#w` there — so no window id appears in any path a manager
  uses, and a manager cannot reach another window's files — they are not in its
  namespace.

- **(2026-09-02) The three structural proposals are ENDORSED and moved into the
  specs.** Reviewed by Christine, with `/mnt/emca` corrected to `/dev`. **The
  manager interface is a file interface** — emca serves one directory per
  window, so it and the emca↔surface protocol are the same thing, and no new
  protocol is needed because 9P is already the only IPC (now in
  [window.md](window.md)). **The `/type` file syntax** — `recognise` one rule
  per line in pipeline order, `managers` listing roles, `verbs` in three action
  forms (now in [type.md](type.md)). **`properties` is `edit` over a text
  rendering of the stat** — change the `mode` line and Save, and `chmod`
  happens; it also makes provenance visible for the first time, a `served` line
  answering *"which union element actually gave me this file?"* Three questions
  stay **open inside them**, marked in place: whether the *content* is a file
  too (for `text/plain` it lives in the host's editor, so `body` would be a
  mirror — the one place the file interface does not collapse cleanly); whether
  `magic` needs more than offset-and-literal; and whether `properties` is
  genuinely a fifth role or just `edit` on a different *view*, in which case the
  list is four.

- **(2026-09-02) `shell` applies to FILES, not just channels — and it is a role
  with many backends.** Hers: *"shell on a file is really append a shell
  conversation to the file… I can open a log file, and append to it. In other
  words, treat the file as a pseudo `/dev/cons`."* This corrects the earlier
  entry below: a store *does* have another end — **you**, plus whatever else is
  appending. Which inverts the usual framing: **`/dev/cons` is not special
  because it is a device; it is the canonical instance of *a file two parties
  append to*.** `tail -f` dissolves: open the log with `shell`, watch lines
  arrive, type your own into the same stream, and the record stays one thing.

  **Content is always renderable as a file; what `write` MEANS depends on the
  role** — `edit` changes the content, `shell` sends to the other end and the
  transcript grows. Same shape as `read` differing between a store and a
  channel. acme's `win` proves it: its body is a file, typing at the end goes to
  the process, output appends.

  **You do not choose a backend — you choose a file, and whatever is on the
  other end is the backend**: `rc`, a language REPL, a remote connection, a
  daemon writing a log, **or an LLM**. So *adding a conversational backend is
  adding a file* — "adding a manager is adding a file", one level out — and it
  delivers what the README already claimed: *"the model is a file you write
  prompts into"*. **History needs no mechanism: the transcript IS the history**,
  which is one more argument for it being a real file.

  **The cost, named rather than discovered: line discipline.** *"both sides have
  to manage the result"* is exactly what a terminal driver does — hold the
  partial line, redraw it below interrupting output, keep the cursor where the
  user thinks it is. It cannot live purely on either side: the host has the
  keystrokes, IPNX has the writer, the interleaving happens between them. **A
  model makes it worse**, arriving token by token rather than line by line.
  **And rendering must not eat the transcript**: markdown and code blocks are
  drawn by the *surface*, while the file stays text — otherwise selecting a path
  in the output and pressing Open stops working, which was the point of the
  transcript being a file.

- **(2026-09-02) `run:` actions substitute ENVIRONMENT VARIABLES, not a
  templating syntax.** emca sets `$file`, `$dir` and `$window`; a verb is then
  an ordinary command — `run:tar cf $file.tar $file`. No new language; `/env` is
  already a filesystem (`bind '#e' /env`) so the variables are **inspectable
  files**; and quoting, escaping and spaces-in-filenames are rc's problem, which
  is solved. **The selection needs no variable**, because `|` already pipes it —
  one mechanism, not two.

- **(2026-09-02) When a convenience verb earns its place.** *"A convenience verb
  earns its place when it does something the general mechanism doesn't, or when
  it teaches something the mechanism hides. Otherwise it is a second path, and
  second paths drift."* **Replace passes**: Find-then-type replaces *every*
  match, while "replace the selection with the tag line" replaces *one thing*,
  which has no other expression. **The Pipe affordance passes** on the second
  clause — it exists to teach `|`. **chmod fails both**: `Run` does it and
  `properties` does it more legibly. And the stakes are low, because **the
  toolbar is declared in a file**: anyone can add `Chmod run:chmod $tag $file`
  to their own `/type`. The only question is what ships by default.

- **(2026-09-02) The shell is substitutable per NAMESPACE, and bash is a script
  interpreter rather than the interactive shell.** Because `/bin` is a union and
  namespaces are per-process, **different windows can have different shells** —
  so "which shell" is a namespace fact, not a setting: `chsh`, `/etc/shells` and
  the shell field in `passwd` all dissolve. But **`run:` actions execute in the
  SYSTEM shell** while the interactive shell is personal — the separation Unix
  already draws between `$SHELL` and `#!/bin/sh` — or a `/type` written for one
  shell breaks for a user who chose another.

  **bash and zsh bring behaviour built for a world this system removed**, and
  every piece of it is a compensation for a missing primitive: **job control**
  compensates for having one terminal (here: windows); **`PATH`** for no union
  mount (here: `bind`); **terminal control** — termios, raw mode, ANSI escapes —
  for output not being addressable content (here: a transcript that is a file);
  **`screen`/`tmux`** for sessions not being namespaces. It cannot be suppressed
  from inside bash; what can be done is **not providing the mechanisms** — no
  termios, no process groups, no controlling terminal (`/dev/tty` deliberately
  does not exist) — so the interactive half fails to initialise and the script
  interpreter still runs. **So: bash for existing scripts, never as the
  interactive shell**, the same posture the V10 exhibit gets. And note the
  closure: remove job control, `PATH` and terminal control, and what remains is
  close to **what rc already is** — Plan 9 removed the same compensations for
  the same reasons, one system earlier, which is why the curation principle kept
  it.

- **(2026-09-02) `shell` needs a CHANNEL, not a store — and a conversation is
  not a file.** Hers: *"the shell breaks the convention that everything is a
  file. It is a conversation, not a file"*, then *"the conversation can be
  displayed as a file though"*. Both are right, and the resolution is that
  **"everything is a file" was always an INTERFACE claim, not an ontological
  one** — Unix has had two things behind one `read()` since the start: a
  **store** (reading is idempotent) and a **channel** (reading consumes; two
  readers race). Same shape as *"everything is text" is a fiction*: true about
  the interface, false about the things. A conversation has **two file-shaped
  aspects** — the **channel** you read and write, and the **transcript**, which
  is ordinary content. Those are exactly the shell body's two regions: the past
  is immutable, the end is live. **The transcript should be a real file**, so
  selecting a path in a build error and pressing Open works, and a session can
  be grepped and kept — acme's `win` already does this, its body being
  `/mnt/acme/N/body`. Bell Labs never wrote a terminal emulator; a terminal
  emulator is a pile of features compensating for scrollback not being content.
  **CORRECTED later the same day: `shell` is NOT restricted to channels** — see
  the entry above on shell backends. The reasoning here was that a store has
  no other end; the other end can be *you*, plus whatever else appends. What
  survives is that running a script is `Run` or `manage`, never `shell`: running a script is `Run` (output) or `manage` (under control), and
  *interactive with a file* is the two-step, `manage` creating the process and
  `shell` conversing with it.

- **(2026-09-02) A directory's roles: `look`, `edit`, `properties` — and `edit`
  IS the file manager.** A directory's **content is its entries**, so renaming,
  moving and deleting are *editing the listing*, not managing the object —
  hers, on 2026-08-31: *"the same directory can be an `ls` window or, if you
  want to edit the listing, an `edit` window"*. Precedent that it works:
  Emacs' `wdired` and `vidir`. **There is no file manager program**; Finder is
  `look` and `edit` on `inode/directory`. The rename-versus-delete ambiguity a
  text diff cannot resolve is settled by **identity, not diffing** — 9P's
  **qid**, *"unique id from the server"*, is exactly the field for it. Four
  guards, three already built: nothing happens until **Save**; Save shows the
  **plan** ("3 renames, 1 delete"); **`#V` snapshots** make a destructive save
  genuinely reversible rather than Finder's simulated undo; and read-only from
  permissions means the interface **grants no authority you did not have**.
  `manage` on a directory is **serving it** — `exportfs` is the one sense in
  which a directory runs. And batch rename falls out of parts designed
  separately: select lines, `|sed s/old/new/` (acme's `|` replaces the
  selection with the output), Save applies the renames.

- **(2026-09-02) Run supplies a CONTEXT, never an implicit input.** `grep foo *`
  in a directory window greps **the files** — the `*` expands in the window's
  directory. `|grep foo` greps **the content** — the listing — because the
  operator gives the selection its own slot. Bare `grep foo` waits on stdin and
  is useless, which is honest. Implicit stdin would make every command's
  behaviour depend on which window you were in, which is the hidden state
  removed from Run earlier, arriving by another door.

- **(2026-09-02) Tag line beats selection, and it is safe because the tag line
  is VISIBLE.** The rule: **the tag line is the operand; the selection is the
  fallback when it is empty, and the input when an operator asks for it.** They
  rarely compete — `Run |sort` uses both, in different slots. The one bad case
  is a stale tag line silently beating a fresh selection, and the only thing
  preventing it is that the winning text is on screen. **So a populated tag line
  must READ as populated** — a rendering decision, but load-bearing rather than
  cosmetic. Precedence is only ever a question *within* the tag line's five:
  selection verbs always take the selection, toolbar verbs always take the
  content.

- **(2026-09-02) THERE IS NO REPLACE VERB.** `Find` with the tag line searches
  for its text *or an address*; `Find` with a selection and an empty tag line
  finds every occurrence of the selection. Because **dot is a set**, Find puts a
  cursor at every match — **so typing after a Find is replace-all**. The Find
  and Replace dialogue (two fields, a direction, match-case, whole-word,
  Replace versus Replace All) dissolves into one verb and the keyboard;
  VS Code's Ctrl-D and Sublime's multi-select prove the gesture. **This creates
  the first genuine requirement for the status line: it must show the match
  count**, because typing after a Find that matched 247 occurrences changes 247
  things that may all be scrolled out of view. Every other status-line candidate
  is a convenience; this one is a safety property. **Escape collapses to a
  single cursor at the last match** — a deterministic exit; clicking is the
  other. Find is always **within the window**: cross-file searching is `grep`
  plus Run, because *Find selects* (you are about to edit) while *grep reports*
  (you are about to read).

- **(2026-09-02) The surface may not intercept keys a manager needs.** Escape
  collapses multi-cursor **in the `edit` role** — it cannot be a surface-wide
  binding, because in a `shell` window Escape must reach the process or vi,
  `less`, readline and ncurses all break. The general rule, which is the
  keyboard form of *"emca does not reach inside a manager's window"*: **the
  surface owns chrome and window-level gestures; the manager owns the keyboard
  inside its rectangle.** The reserved set must be small and stated, or every
  new manager discovers by accident which keys it is not allowed to have.

- **(2026-09-02) MANAGERS ARE NAMED BY ROLE, and there are five.** Not by type:
  `textmgr` and `ipnxmgr` named a type where a ROLE was meant,
  and both are retired. A role **means the same thing wherever it applies** —
  which is the test, not "on every type", since an unoffered role is no more a
  hole than `edit` on a PNG.

  | role | the file is | |
  |---|---|---|
  | **look** | content to read | universal; delegates to the platform's native preview — QuickLook on macOS, the browser's own viewers. Always read-only |
  | **edit** | content to change | |
  | **properties** | an object with attributes | permissions, owner, timestamps, provenance |
  | **manage** | something **running**, to control | *runs the thing's own runtime on it*: the debugger for a binary, the interpreter's debugger for a `.py`, reboot/halt/new-shell for `inode/system`, kill/note for `/proc/N` |
  | **shell** | **a conversation** | a two-way stream: `/dev/cons`, or a network connection exposed as a file — so a remote shell needs no ssh client, no telnet, no minicom |

  **`manage` creates the running thing; `shell` converses with it.** So "open a
  terminal" is `manage` on `/`, and the window you get is `shell` on the
  resulting stream — which keeps `shell` from ever meaning "create", the way
  "terminal *here*" would have made it.

  **The role dropdown lives in the window's top bar**, beside the type, and
  switching it re-opens the window under a different manager. Read-only stops
  being a mode with a warning modal and becomes *which manager is in charge* —
  the safety is visible, and there is no hidden state.

  **The default role is derived from PERMISSIONS, not from location or a
  list**: writable → `edit`, not writable → `look`. So "read-only by default"
  is literally true rather than a policy, it is per-identity for free (an agent
  with no write permission gets `look` on everything, from the uid model that
  already runs), and "why is this read-only?" is answerable with `ls -l`. A
  person who edits all day binds their own `/type` to reorder it.

  **And the type hierarchy carries CAPABILITY, not just manager names**:
  `text/plain` offers look/edit/properties, while `text/x-python` inherits
  those and **adds `manage`**, because a Python file is text *and* runnable.
  That is a stronger justification for MIME nesting than avoiding duplication.

- **(2026-09-02) `shell` is a new paradigm: the body is an input.** Every other
  role treats the body as content — `look` reads it whole, `edit` writes it
  whole. A shell's body is a **transcript**: the past is immutable, the end is
  live, and there is nothing to save, which is why Save, Revert and Undo are
  meaningless there. **Two regions with different rules inside one body**, which
  nothing else in the system has. A shell window therefore has two places to
  type, and they **coexist with different scopes**: the *body* talks to the
  process, the *tag line* operates on the window — Find in the scrollback, Open
  a path visible in the output. Unifying them would move commands out of the
  transcript, and a transcript that does not record what you typed is not a
  record. Because the scrollback is ordinary content, `look`'s behaviour applies
  to it: select a path in a build error and press Open. That property survives
  precisely because the history is content rather than a terminal grid.

- **(2026-09-02) Run, manage and shell are three different acts.** **Run**
  produces *output* — a finished result, shown in a `text/plain` window under
  the `look` role. **`/output` therefore dissolves completely**: not a type, not
  a special window, just text viewed. **manage** changes *state*. **shell** is
  *interactive*. Python shows all three without collision: `python hello.py` in
  the tag line is Run; `manage` on `hello.py` is the debugger; a REPL is
  `shell`.

- **(2026-09-02) THE TOOLBAR BELONGS TO THE MANAGER, NOT THE TYPE.** `look` and
  `edit` are the same type and must offer different toolbars — Save, Undo and
  Redo are meaningless under `look`. An earlier entry the same day said the type
  declares its verbs; it does not.

- **(2026-09-02) What a type declares, and how a file is recognised.** A type
  definition holds: the **name** (a MIME type); **how files of it are
  recognised**; a **list of managers**, the first being the default; and its
  **toolbar verb bindings**. Recognition is an ordered pipeline, and ordering by
  *cost* happens to order by *certainty* — (1) the **serving device**, free in
  9P's stat, which carries `type` and `dev`, and is the signal no desktop has;
  (2) the **qid bits**, directory and symlink; (3) an **exact filename**
  (`/etc/passwd`); (4) an **extension**; (5) **magic bytes**; (6) **fallback to
  `text/plain`**, which never fails. That bounds classification at one stat and
  one short read, which is the answer to *"emca may spend too long trying to
  figure out"*. Verb bindings take three forms, not two: ask the manager, ask
  the surface (`ipnx:` / `host:`), or **run a command** — hers: *"may be
  commands outside manager, eg. a shell"*. The default manager needs no file of
  its own: it is the first line of the managers list, so changing a default is
  editing or binding one line.

- **(2026-09-02) `ns` as implemented is retired; the need it served is real.**
  `/type/<x>/ns` was a file of bind lines `eval`'d as rc on window open — my
  interpretation of her *"each window type is associated with a namespace (eg,
  `/recipe`)"*, which meant a **place**, not a script. As built it was nearly
  vestigial: ten of thirteen were empty, and two of the three that weren't
  belonged to types she never asked for. It also made the registry contain
  **executable text**, which nobody decided. But the concept became *more*
  important, not less: **"a manager owns a window, and populating it is
  binding"** — the debugger binds the debuggee's `/dev/cons` into one pane —
  so a type declaring what is bound into its windows is genuinely needed and
  must be designed rather than inherited.

- **(2026-09-02) Unrecognised or unrenderable content: fall back, but ask.**
  emca **tests** for type and **falls back** to `text/plain`, which therefore
  can never fail. **Recognised but unmanaged is refused for safety, with a
  prompt** — hers: *"file type not displayable, want me to display as text?"*
  Content that is not valid UTF-8 renders with **escapes** and the window opens
  **read-only**, because the danger was never display but round-tripping: open
  a binary, touch nothing, save, and it is destroyed. Save on such a window
  prompts a warning modal, once. **Line endings are never rewritten** — the
  bytes on disk are not normalised, both endings display as a line break, a
  file keeps what it had when saved, and new files get `\n`. That is the text
  manager's specification, not the type's.

- **(2026-09-02) `/template` and `/project`: the declaration is a FILE, the
  instance is a DIRECTORY.** A template is a text file — like a Dockerfile, it
  inherits another template, binds packages and files into a namespace, runs
  commands, attaches a network interface — and **the syntax is ours, not
  Docker's**, because Docker's tokens encode a *mutation* model (`RUN` makes a
  layer) and ours is declaration (`bind`); borrowing them would lie familiarly.
  **Inheritance is free here**: binds compose, so "inherits another template"
  is *apply the parent's binds first*, with no layer format, no union
  filesystem, no storage driver — a whole Docker subsystem that exists only to
  simulate what `bind` already does. Opening a template runs the template
  manager, whose **New** instantiates it into a workspace named by the tag
  line. The name **`/template`** was chosen over `/recipe` because the design's
  own prose already said it five times — *"Templates instantiate, workspaces
  open"* — and the filesystem must agree with the documentation; her earlier
  objection to "recipe" (*"a user cloning ipnx is genuinely cloning a
  project"*) dissolves once the instance is what carries the name "project".
  **`New` on a template creates the project at `/home/project/<name>`**, and
  **instantiating creates a PROCESS and a NAMESPACE**, not merely a directory —
  so New on a template is `run` on a process spec, the same act as the
  orchestration suite. The template persists as a file in the project's
  namespace. **Promotion writes a file that RECREATES the namespace and
  process** — a serialisation of the live state, which `/proc/N/ns` already
  provides, and which Docker structurally cannot do (`docker commit` yields an
  opaque layer, never a Dockerfile). **Not a directory move** *(and not a copy —
  corrected later the same day, when Christine confirmed **project and template
  are different objects**: a `project` declaration is specific to one workspace,
  a `template` is generic, so promoting strips what is true of this project —
  the name, the path, the remote — and keeps what is true of the kind: the
  inherited base, the packages, the commands)*:
  `/home/project/ipnx/template` → `/template/ipnx`, which is her recorded
  principle — *"promotion promotes the declaration, not your files"* — and
  which dissolves the dir-versus-file mismatch, because no directory was ever
  in the operation. A project therefore always carries the declaration it came
  from, so "what is this environment?" is `cat`, and changing it is editing
  that file.

- **(2026-09-02) `/home` is the user's home, and it is a BIND.** `/usr/<name>`
  is V7's convention, kept by Plan 9 — both timesharing systems. This one is
  not: *"exactly one per instance… you do not log into your own machine"*, so a
  directory whose job is to hold many homes is machinery for a problem that
  does not exist here. **`/home` means "mine", resolved by the namespace** — an
  agent's `/home` is the agent's, a role's is the role's — so **no path ever
  contains a username**, which is *"names are for accounting; namespaces are
  for authority"* holding. It must be a **bind, not a symlink**: a symlink
  stores its target, so `/home → /usr/kitty` would mean *kitty* for everyone
  who walked it, and namespace-relative symlink resolution does not save it
  because the target text is still fixed. `/usr/<name>` survives as the
  **durable, addressable** name — `/usr/kitty`, `/usr/mimmy`, `/usr/daniel`,
  readable subject to permission — because `/home` is relative by construction
  and cannot name someone else's. `/usr` then holds **files belonging to identities, not logins**:
  a mount of another person's system (per-attach identity already enforced), or
  a resident role or agent. An agent's namespace need not contain `/usr` at
  all — its world is `/home` plus what it was given.

- **(2026-09-02) THE WINDOW SYSTEM LEAVES THE KERNEL ENTIRELY.** Hers: *"the
  kernel should not be involved at all"*, and *"The kernel is completely out of
  this."* **Saranos is the app the user launches**; it instantiates the kernel
  and the emca host-side app. The kernel handles the IPNX side — processes and
  namespaces — and one of the processes it launches is emca IPNX-side. The two
  emca halves speak 9P to each other. What this removes from the kernel:
  `Win.{parent,kids,axis,allocated,premax}`, `split`, `maximise`, `reparent`,
  recursive close, the four window file kinds, the nine `wctl` verbs, and
  `kernel/src/draw.rs` — 363 lines of raster engine. The constraint that forced
  it is the founding rule the kernel had already broken: *"the kernel unable to
  bloat by construction"*. **Plan 9 never did this either** — acme's own
  `fsys.c` (in the tree, verbatim) is a userspace 9P server that mounts itself
  at `/mnt/acme`, and rio serves its clients' device files the same way. The
  kernel device was the deviation. NOTE the sentence someone will later use to
  undo this: the kernel does carry the 9P transport across the host boundary,
  because only it can reach the host — but **carrying is not participating**,
  the way a pipe carries HTTP without understanding it.

- **(2026-09-02) The devices are Saranos's, and emca VIRTUALISES them.** Hers:
  *"Only saranos knows about the host… I am a macos app. I have a screen,
  keyboard and mouse. I will serve these as virtual devices to the IPNX
  kernel."* emca-host is a **Kit** inside the Saranos app. On the IPNX side
  `/dev/cons`, `/dev/draw` and `/dev/canvas` exist either way: **without emca
  they connect straight to the host's screen, keyboard and mouse** — which is
  how IPNX runs as a CLI with no windows — and **with emca they are virtualised
  per window**, indistinguishable to the client. That substitutability is the
  load-bearing property, and it is rio's: a Plan 9 client cannot tell rio's
  `/dev/draw` from the hardware's, which is exactly why rio nests. It also
  gives *"IPNX implements no renderers"* its precise meaning at last —
  renderers are in Saranos, and emca-host is the part of Saranos that
  virtualises them.

- **(2026-09-02) emca is the WINDOW MANAGER; a type manager is what is IN a
  window.** Hers: *"emca is a window manager. it controls the placement of
  windows on the screen. a type manager controls what is in a window… type
  managers may communicate with window managers (over 9P of course)."* emca
  owns every relationship between windows — the tree, allocation, fit, and each
  child→parent instruction (*minimise me*). A type manager owns rendering,
  editing, the status line, the toolbar's verbs, and the semantics of Find,
  Edit and selection. **This retires "emca is a watcher, not a gatekeeper"**
  and the *"a window still opens in its type's default pane"* degradation — both
  were artefacts of the hardcoded-pane design M15 replaced. The real
  degradation is better: no window manager, no windows, and the devices go
  straight to the screen.

- **(2026-09-02) TYPES ARE MIME TYPES. `text/plain` is the default; `/` is
  `inode/system`.** Everything emca opens into a window is a file in the
  namespace, and **emca infers the type from the file's contents** — not from a
  suffix. Unknown content is `text/plain`, which is the Unix answer. Adopting
  MIME is consistent with the founding rather than a departure: the refusal is
  of POSIX-*the-standard* as an interface to implement, while *"sockets won"*
  and *"UTF-8 won"* are the precedent for adopting a vocabulary that won.
  `inode/directory` is shared-mime-info's own name, not an invention. **The
  two-part form matters structurally** — the registry is a path, so
  `/type/text/plain/` nests under `/type/text/`, and a future `text/x-csrc`
  that declares nothing inherits `text/`'s manager with no algorithm and no
  merge rule. `/` is **`inode/system`** — `inode/*` being freedesktop's space
  for filesystem objects that are not content — its manager is **`ipnxmgr`**
  *(superseded later the same day: managers are named by ROLE, so this is the
  `manage` role on `inode/system`; `ipnxmgr` as a name is retired)*,
  and its content is the layout: **ipnxmgr owns the responsive breakpoints and
  which windows exist at boot; emca owns what rectangles they get.** `output`
  **is not a type**: it is `text/plain` under a path convention. Two facts, and
  only two, are hardcoded in emca: `/` is `inode/system`, and unrecognised
  content is `text/plain`.

- **(2026-09-02) emca NESTS, and nesting is another view of the system.** Hers:
  *"acme… is an emca like program using /dev/draw running under emca"*, and
  *"we could instantiate a new emca in a window - it will open '/', does a
  layout in that window"*. **There is only one host emca**; it manages
  recursive emcas as further windows. Because namespaces are per-process, a
  nested emca opened after `rfork n` composes a **different world** — the
  visual form of *"a process can be given a world."* Two tiers, and the ideal
  is delivered by both rather than by engine nesting: **tier 1 — same kernel,
  different namespace** — easy, and the work to do now; **tier 2 — a separate
  kernel instance** shown in a window, requiring Saranos and emca-host to be
  **multi-system**, deferred to the distributed-OS stretch, where *"Saranos may
  need to switch between multiple local and remote systems, or even display
  them side by side on a single emca interface."* Literal IPNX-inside-IPNX
  would need a wasm engine reachable from a guest (there is none — measured);
  the path if ever wanted is to delegate instantiation upward as a host
  operation. It buys little, since both kernels are equally confined by the
  same host already.

- **(2026-09-02) Everything is SPEC'D, PROPOSED or a GAP — and the third state
  was missing.** Hers: *"everything that we have not explicitly discussed and
  endorsed should be a gap (or proposed if you have created a design). proposed
  designs need to be reviewed."* And the response she wants to "implement the
  demo": *"X is speced, Y is proposed and Z is gap. Would you like me to review
  Y with you before implementing, and would you like me to propose Z, before we
  implement."* The constraint that forced it: with only two states — specified,
  or forbidden — both failure modes fired in one day. Types were built that she
  never mentioned (`env`, `errors`, `srv`), *and* `implementation.md` acquired
  the invented line **"Types: root, ls, edit, shell, output, and no others"**,
  which closed a list she had left open and erased the `/pkg` and `/project` she
  had named as *types we will need to design*. **Naming a type is not
  specifying one**, and in emca a type becomes a toolbar button — so an
  undesigned type ships as a control that does nothing. Recorded in CLAUDE.md's
  Conventions; the middle state's home is [proposals.md](proposals.md).

- **(2026-09-02) A type says what content IS; a manager says what to do with
  it.** Hers: *"The default type is 'text' which you have been calling. But edit
  is really the manager of text."* A type is a folder of text files — *"in full
  alignment with Unix philosophy"* — but *"a window type is encapsulating things
  that are not text, that's why we need a manager, which understands how to
  render/edit the type, knows what to do with the status line, supplies toolbar
  buttons, etc."* The reasoning that forced it: **"everything is text" is a
  fiction and Unix never held it** — *"Unix commands are binary executables.
  /dev/kmem is binary. /etc/passwd is a text file, but it is a structured text
  file."* What Unix aims at is narrower and true: the system is configurable
  through text files, and commands are oriented towards processing them. **IPNX
  holds to that; Saranos need not** — *"it understands we live in a modern world
  of user interfaces, rich media"* — which is why they are separate layers. A
  **manager interface** is therefore a requirement, not merely managers:
  *"Acme was deliberately a minimalist design - every window exposes a file. We
  have already broken this minimalism."* Managers live on either side —
  *"monaco is a manager over a file"* — which is all that "editing is the
  surface's" and "IPNX implements no renderers" ever meant. The design is
  [type.md](type.md); the interface itself is a **gap**.

- **(2026-09-02) The documentation is refactored by document KIND.** Hers, on
  finding emca.md conflating four kinds at once: *"You are conflating a
  description of saranos and components, which should live in a separate file…
  next you are describing a spec as a transcript of what we discussed rather
  than as a spec document. Next you are evaluating what has been built vs what
  was designed. That does not belong in a spec document."* And: *"There are
  issues throughout. You need to completely refactor everything."* The
  separation was already agreed in CLAUDE.md's table and was simply not being
  kept. Applied: **[saranos.md](saranos.md)** is new and is the single home of
  the layer names (moved out of architecture.md, its duplicate removed from
  emca.md); decision records and the superseded canvas protocol moved here;
  build status and "what remains" moved to
  [implementation.md](implementation.md); the packages/projects design moved to
  [proposals.md](proposals.md) as the unreviewed proposal it is; acme.md's stale
  session brief moved to [design-thinking.md](design-thinking.md); and the
  `PART ONE…PART TEN` banners — plain-text convention that survived the
  Markdown conversion — became a heading hierarchy. **Specs are present tense
  and carry neither chronology nor build status.**

- **(2026-08-31) emca is the user interface, and IPNX is a whole operating
  system.** The largest reframe since the re-founding, and it reorders much of
  what follows. Christine, after four days of designing what everyone assumed
  was an editor: *"What we have been designing is not acme, or a replacement
  for acme. It is the shell that IPNX boots into, it is the IPNX primary user
  interface, it is the browser surface, the macos app, the ios app. The system
  boots into emca. Emca is the user interface."* And on what that makes the
  project: *"It's not just a barebones UNIX reimagined, it is a full operating
  system with our own semantics, user interface, artifacts."* The full design is
  [emca.md](emca.md); the parts list it derives from is [acme.md](acme.md).
  **The mechanism, and why it is an IPNX design rather than a portable one**:
  *"Traditional Unix and Linux manages different types differently, using
  separate commands. ps list processes, there are separate commands to manage
  network connections, packages, users, etc. In IPNX everything is managed as a
  file. That's what makes emca work."* There is no process manager program, no
  package-manager GUI, no network panel — there is a filesystem, a **window
  type** declaring which verbs its files accept, and a surface rendering those
  verbs natively. Adding a manager to the system is adding a file. A rich system
  UI normally destroys acme's central property (any text can be a verb's
  operand) because a process table becomes a native widget; **here it cannot,
  because the managers are already text filesystems** — `/proc/1/status` *is*
  text, so a process window is a text window that happens to carry `Kill`. The
  property holds by construction. On a system where the process table is a
  syscall, the widget toolkit returns and acme dies inside it. **The
  architecture, hers**: *"you need to split emca design into two halves — a half
  that lives in IPNX, and a half that is native to the surface."* IPNX owns
  state, meaning and policy; the surface owns rendering and input; the test that
  settles any case is *differs between a Mac and an iPad → the surface's;
  differs between one workspace and another → IPNX's*. It earned its keep
  immediately on an unanticipated question — credential key custody is the
  surface's (Keychain, WebCrypto, a passphrase), the credential listing and its
  verbs are IPNX's. **A window type is a triple** — namespace, *optional*
  command, window configuration — and the command being optional dissolves the
  types-vs-programs fork: no command means emca renders the tree, a command
  means the program drives the window through `/mnt/emca` *(the path became
  `/dev/window` on 2026-09-02, when the window system left the kernel and
  `/dev` was recognised as the virtualisable slot; the client model stands)*
  (acme's client model,
  unchanged), so `/proc` can start as a one-line type and grow a live `ps`
  without the type system changing. `/type` is itself a type, so the interface
  is configured by editing files *in* the interface — no plugin API, no manifest,
  no restart; one built-in type (`dir`) is the bootstrap floor. **`/pkg` and
  `/project` are different kinds** (hers, replacing an earlier single "recipe"
  registry that was trying to be both): a package is a toolchain, command or
  library and is a *leaf*, bound not instantiated; a project *combines* packages
  into a namespace and becomes a process. A two-level graph, not an arbitrary
  DAG — and it is what `pkg.c` already was ("installing BINDS it; the namespace
  is the installation record"). **A project is a proto-process**, which names the
  symmetry: `/pkg/go` is what it is made of, `/project/ipnx` the process that
  could be, `/proc/1741` the process that is — so "save as project" is reading
  `/proc/N/ns`. Templates instantiate, workspaces open; `/project` is a union of
  system and personal, so promotion is moving a file between union elements, and
  **promotion promotes the declaration, not your files**. Clone and instantiate
  are separate acts, and instantiate shows the declaration first — a cloned
  project file is a stranger's declaration, the hole Docker and devcontainers
  both have, and IPNX's answer is not a mitigation: the blast radius is readable
  in one small file, and the namespace bounds it whether you read it or not.
  **Naming, measured** (the house rule beating four rounds of opinion): sixteen
  root names across the vendored Plan 9 source, **not one over four characters**
  — so a root is ≤4 characters, abbreviation is what you do when the word is
  longer, and a directory is named for what one of its *entries is* (`/proc/17`
  is a proc), which is why Unix never wrote `/procs`. Expanding everything was
  refused by two standing decisions, not by taste: "modern software must run"
  (CPython, Go and git carry hardcoded `/tmp`, `/bin`, `/lib`, `/usr`) and
  "vendored sources are verbatim" (571 references to `/lib`). `/project` is the
  **one exception**, recorded with its reason so it constrains rather than
  licenses: `/proj` is one keystroke from `/proc` and adjacent in meaning, so it
  is a tab-completion footgun rather than merely a longer word. Rejected en
  route, with reasons kept so the search is not redone: `recipe` (6), `rec`
  (opaque), `menu` (32 uses in the UI sense in these documents, and the design's
  founding quote is *"Acme doesn't need menus"*), `kit`/`app` (overloaded),
  `spec` (geeky). **The protocol amendment**: acme.md's constraint 2 expected
  "little or no protocol change" and this exceeds it — four additions to
  `/dev/canvas` and no more (structure roles, window type, verb applicability,
  show request), recorded as deliberate. Canvas v0 anticipated the direction
  ("span attrs arrive with the web presenter's real links, later"); emca is the
  benchmark that demands them, which is how every element of v0 was derived.
  **Consequences, dispositioned**: "one editor" ([userland.md](userland.md))
  becomes **one surface** — the editor is one window type; **rio-today retires**
  as a separate design, absorbed (`#w` still mints windows; a program's own
  canvas window appears as an emca window of a canvas type); the browser page,
  the macOS app and the iPadOS app **stop being hosts that run a demo and become
  surfaces of emca**, so the public page is the system's face rather than a
  demonstration of it; and **the "no phone form factor" refusal is reversed** —
  it was recorded 2026-08-29 on the evidence that no persona's journey contained
  one, and the new evidence is that responsive design in *characters rather than
  pixels* makes the phone one value of a knob built anyway, not a separate
  design. Reversal by evidence, per the standing rule, not by fresh opinion.
  Largest open item, named rather than buried: **the surface half has no test
  story** — emca's behaviour stays testable through the virtual surface, but
  nothing in the tree proves a button was reachable or that Tab reached it, and
  "no half-working" plus the keyboard-complete law make that the design's biggest
  untested area, with no precedent in this repository to borrow from.

- **(2026-08-31) The system is named Saranos; IPNX is the kernel; emca is the
  interface.** The companion to the reframe above: once emca became the user
  interface and the project became "a full operating system with our own
  semantics, user interface, artifacts", the layer that had no name was the
  system itself. Christine's observation that opened it: Apple names XNU, Darwin,
  SwiftUI, macOS and MacBook separately, and IPNX had one name doing several
  jobs. **The layering, decided**:

  | | |
  |---|---|
  | **Saranos** | **the user experience** — emca the shell ([emca.md](emca.md)), the window types, the presenters, the surfaces' furniture |
  | **IPNX** | **the kernel AND the userspace** — the file world (Darwin's slot, not XNU's) |
  | wasm, and the surfaces | the machine it runs on |

  Christine's correction, which settled where the line falls: *"IPNX is still
  there, it is still the userspace and kernel. Saranos is what goes on the top —
  a user experience."* So IPNX keeps everything it already named — kernel,
  libcs, commands, `/pkg`, the whole file world — and Saranos names only what
  a person sees and touches. `/dev/canvas` is the interface between them: an
  IPNX device that Saranos consumes.

  A symmetry worth recording, because it says the layering is not arbitrary:
  **XNU stands for "X is Not Unix", and IPNX is "IP is Not UNIX"** (README's own
  etymology) — the two kernels carry the same joke, and Apple's answer to the
  layer above it was a human name, not a second acronym. **Saranos** is from
  Sanskrit *śaraṇa* (शरण) — **refuge, shelter, sanctuary**. Her reading, which
  is the naming rationale and leads: it is *"a refuge from the complexities of
  the modern computing environment"* — a refuge for the PERSON, which is what
  makes it a name for the experience layer rather than for the kernel. Her
  criteria, in her words: *"easy to pronounce, mixed etymology, it sounds like a
  word but isn't, and conveys serenity"* — the phonetic reading (*saran-* /
  *seren-*) and the Sanskrit agree, which is rare in a coinage. A second reading
  runs underneath and is structural rather than felt: a *process* also runs in a
  refuge bounded by what it was given — from the README, *"IPNX v12 processes are
  secure by design. It assumes from day one that the running application is
  malicious"* — which is P4's *"what could it touch?"* answerable by
  construction. The two readings are one word doing both jobs, at the two layers
  the name now separates. **The mixed etymology is a feature, not a compromise**:
  a Sanskrit root with a Greek-shaped ending, for a system that is itself a
  synthesis — Plan 9's architecture, Unix's interface, wasm's world. The name's
  construction mirrors the thing's. **Searched before committing, and clean**: zero
  software, zero operating systems, zero developer tools; the hits are a South
  African restaurant franchise, a surname, and an Elder Scrolls NPC.
  **The rejections, kept so the search is never re-run** — nine candidates over
  seven rounds, every one killed by measurement rather than taste:
  `Ecma` (Ecma International / ECMAScript, in a system built on JS engines — and
  acme backwards is *emca*, not *ecma*) · `Kitty`/`KittyOS` (kitty the terminal
  emulator, plus two existing operating systems, the most prominent headlined
  *"Writing A Toy OS"*) · `Mimmy`/`Mimi` (Sanrio's actively enforced portfolio —
  and *Sanrio v Dong-A Pencil* found against **KITTY** alone; naming the system
  after a defended character would turn IPNX's own IP-litigation joke into the
  thing) · `MimiOS` (free, but roots in *mimic* — which argues against the
  2026-08-30 decision that IPNX is not a modified Plan 9 — and files beside
  Minix, Mimix, Mimiker, all teaching systems) · `Miaos` (two existing OSes, one
  of them a recursive acronym, the same genus as IPNX) · `Ailuros` (a live
  local-first AI-agent studio at ailouros.io, in P4/P5's territory — and it
  begins with `AI`, which in 2026 is read before any Greek) · `namastos` (a weld,
  homage names nothing about the system, and reads as a yoga pun) · `loka`
  (Sanskrit *world*, apt — but Loka Inc. is an AWS Innovation Partner of the Year
  selling to the same audience since 2004) · `viharos` (Sanskrit *dwelling* — but
  already a word in **Hungarian**, meaning *stormy*) · `marjaros` (Sanskrit
  *cat*, clean in search — but two characters from **Manjaro**, an operating
  system) · and `topos` (Greek *place*, free, apt, seriously-neighboured — and
  declined; recorded because it cleared every test and still was not the name,
  which is data about how naming actually resolves). **The transferable rule,
  measured across all of them**: *the company a name keeps in search results is a
  positioning decision made before anyone reads a word you wrote* — and for a
  project whose recorded anxiety is "I feel like we are underselling what has
  been achieved", that is not a tiebreaker but an argument. **And the cat goes
  where Bell Labs always kept it**: glenda is Plan 9's mascot and Plan 9 is the
  name. Six of the nine rejections were cats; the affection was fighting for the
  wrong slot. A cat becomes Saranos's mascot, costs nothing, clears nothing, and
  `/usr/kitty` stays exactly as it reads in every example.
  **Scope of the rename, and its limit**: dated entries in this log and in the
  other records are HISTORY and keep the words they were written with — a log is
  not retroactively renamed. Only present-tense statements of what the system
  *is* take the new layering ([CLAUDE.md](../CLAUDE.md), [emca.md](emca.md),
  [platforms.md](platforms.md), and architecture.md when the contracts land).
  README.md is Christine's and is not touched.

- **(2026-08-31) Editing is the surface's: emca implements sam, not a WYSIWYG
  editor.** The two-halves split taken to its conclusion — and the first draft
  of [emca.md](emca.md) had the rule and stopped short of it. Christine:
  *"emca doesn't really need to implement a WYSIWYG editor on the IPNX side. It
  can implement sam, a batch editor. The job of emca is to push a file into a
  window via /dev/canvas. The host side can display and scroll the file, and
  more importantly edit it using Monaco or TextEdit or similar."* Extended over
  three further messages: *"selecting, copying, pasting can all be host side
  operations, with sync to /dev/snarf"* · *"even command history and shell
  command line edit — host implemented"* · *"we can even bring xterm/js back"*.
  **The split test already decided this and was not followed through**: does
  editing behaviour differ between a Mac and an iPad? Profoundly — TextKit is
  not Monaco. So editing is the surface's, and "the surface owns text input"
  was the same rule applied only as far as the caret. **What moves**: the caret,
  selection, insertion and deletion, keystroke undo, syntax highlighting,
  folding, multi-cursor, find-in-file, IME, autocorrect, dictation, the
  clipboard (synced to `/dev/snarf`), **command history and line editing**, and
  **terminal emulation**. **What IPNX keeps**: the authoritative buffer
  (shadowed from events), the file (`Get`/`Put`), commands and `+Errors` and the
  `| < >` filters, **sam's structural language**, context, what the verbs *mean*,
  and the file interface. **So emca is a file server with a workspace, not an
  editor** — it pushes text into windows, runs commands, applies
  transformations. **Why this is "best of both worlds" rather than a
  concession**: Monaco and sam are *complementary, not competing*. Monaco is
  interactive, local and cursor-scoped; sam is batch, structural and file-scoped
  — `x/re/` over every match, `s//repl/` through an address, transformations no
  cursor can express. Every other system has one or the other, or bolts a macro
  language onto an editor that owns the buffer; here the buffer is a **file** and
  the surface is a **view**, so the halves compose by construction. Nobody has
  shipped both well together because nobody else had this boundary. **What it
  retires, none of it to be written**: the hand-rolled caret, guest-side
  readline, escape-sequence history, syntax highlighting, folding, multi-cursor,
  find-in-file — inherited rather than implemented, which is the same move as
  borrowing V8 and wasmtime under the kernel (the founding pattern: borrow the
  era's engines through a narrow waist). **And it reconciles the console's "AND,
  not XOR"** ([userland.md](userland.md), 2026-08-30): the transcript is the
  **line-oriented door**, where history and editing are the host's; **xterm.js is
  the raw-input door**, for programs wanting keystrokes rather than lines. That
  was recorded as a concession to familiarity and is now the consistent answer,
  with an architectural reason. **Four risks, recorded as open rather than
  waved past**: (1) **property 1** — *any text can be a verb's operand* is the
  claim the design rests on, and Monaco owns the body's text, so `execute` and
  `look` must ride *its* action and context-menu API; it looks possible and must
  be PROVEN before adoption. (2) **Undo has two owners now** — the surface's at
  keystroke granularity, emca's at command granularity (`acme.c`'s sequence
  numbers); acme had one, and which answers ⌘Z is undecided and will be felt
  immediately. (3) **Buffer fidelity** — the shadow discipline is *apps never
  re-read* (con(1)'s, load-bearing); Monaco must report every edit, multi-cursor
  emits many ranges per keystroke, and if surface and app ever diverge the only
  repair is a re-read, which breaks the discipline. (4) **The surfaces now differ
  in dependencies**, not only in grammar — Monaco (~5MB) on the web, TextKit on
  Apple. Consequence for the plan: **M14c shrinks substantially** (emca's IPNX
  half is a file-pusher plus sam) and **M14d gains the editor component**;
  implementation.md is amended in the same commit.

- **(2026-08-31) `/dev/canvas` was over-derived: the redesign into 9P, `/dev/window`,
  `/type`, and a canvas narrowed to drawing.** The largest correction to a landed
  contract so far, and it was forced by the founding decisions rather than chosen.
  Christine, stopping a build that was going the wrong way: *"You are trying to
  push everything through a protocol that should have been an exception rather
  than the rule."* And the test she applied to the result: *"This is the only
  solution that fits the principle (everything is a file, per process namespace,
  and 9P is the only protocol)."* **The framing error, named**: canvas.md's own
  title is *"the display protocol"* — but **9P is the only protocol** (founding).
  Calling canvas a protocol invited treating it as the place all host/IPNX
  exchange happens, and it grew until it carried layout, chrome, text and
  drawing. It was always files; the name is what made it universal.
  **The measurement record shows exactly where it went wrong.** v0 was derived
  from four benchmarks — console-today, acme-today, rio-today, one plot. Under
  the split: console-today is a text file plus a line back-channel; acme-today is
  a file plus chrome; rio-today is window management; **only the plot was
  drawing.** Three of the four were never canvas consumers, so `stack`, `text`
  and `edit` are generalisations from things that wanted files and chrome — which
  is also why `frame` and `image` never found a consumer. The measurement
  discipline caught its own over-derivation, a day late, which is the system
  working. **The redesign, four semantics over ONE protocol** (9P throughout —
  these are not four protocols but four conventions):
  **(1) Content is 9P, directly.** emca names a file; the host mounts it and
  renders it natively — text, SVG, HTML, Markdown, PostScript, images.
  **IPNX IMPLEMENTS NO RENDERERS AT ALL**, which is the deletion that makes the
  redesign worth doing, and is the founding pattern again (borrow the era's
  engines through a narrow waist — the host's renderers *are* the era's engines,
  as V8 and wasmtime are under the kernel).
  **(2) `/dev/window/<type>/<n>` is the control interface**, bidirectional, a
  numbered directory of small files in the house shape (`/proc/17`,
  `/net/tcp/0`, `/mnt/acme/27`): IPNX declares the chrome — these buttons, this
  one runs `Put` in IPNX, that one toggles wrap host-side — and the host reports
  back what the user did. **The window's TYPE IS IN THE PATH**, not an attribute:
  `/dev/window/proc/1` tells the host it is drawing a proc window, exactly as
  `/net/tcp/0` differs from `/net/udp/0`. This is `#w` grown up — the device
  keeps minting and owning windows and gains the interface it should have had.
  **(3) `/type` is the registry both sides read**: what window types exist, what
  IPNX command drives each, and what the semantics of that type's host/IPNX
  exchange are. It answers "how does the host know what this is" without
  sniffing, and it is the same registry the emca design already needed.
  **(4) `/dev/canvas` narrows to its name** — genuine drawing, the classic
  turtle vocabulary: open a rectangle, draw a circle, place text. The
  **exception**, for programs that actually draw. `path` survives (it came from
  the one real benchmark); `stack`, `text` and `edit` retire to `/dev/window`
  and to files.
  **Editing and `Put`** *(refined by the next entry: emca holds the AUTHORITATIVE
  buffer and the host mirrors it — the phrasing here predates that question being
  raised)*: the host holds the edited buffer, and on Save *"tells
  IPNX here is the edited file and streams it over 9P"* — so writing back is an
  ordinary 9P write, and dirty state is reported for `Putall` and for `Exit`
  refusing. This completes the 2026-08-31 *editing is the surface's* decision:
  not only does the host edit, it returns the result through the one protocol.
  **What retires, none of it to be written**: canvas's `stack`/`text`/`edit`
  kinds, the shadow-buffer discipline *for display*, every renderer emca would
  have needed, most of acme.c's canvas-writing machinery, and the `role=`/`type=`
  canvas attrs added earlier the same day (they belong on `/dev/window`).
  **Open, and worth deciding early**: whether `/dev/window/<type>/<n>` and
  acme's own `/mnt/acme/<n>` are ONE window vocabulary or two that rhyme — same
  files, opposite directions, and the only difference today is that
  `/dev/window` declares toolbars and actions, which acme's interface arguably
  should have had. Consequence for the plan: canvas.md is narrowed with its
  over-derivation stated in place, `/dev/window` is specified, and M14 is
  restructured — its protocol stage shrinks to *narrow canvas, specify
  `/dev/window`*, and its surface stage gains the host's renderers for free.

- **(2026-08-31) The buffer contract: mirrored with detectable divergence, one
  undo stack, `/mnt/acme` merged, and versioning as policy.** Four consequences
  of *editing is the surface's*, settled together because they interlock.
  **(1) emca holds the authoritative buffer; the host mirrors it.** Christine:
  *"we can't just let host edit and send the completed file to emca. We must
  notify emca of every edit, so essentially emca and the host are maintaining
  mirror buffers."* Four independent reasons force it, only one of which is
  undo: **the suite runs headless** (against a virtual surface there is no host
  buffer, so every acme behaviour test would break), sam's structural commands
  operate on text, `Put` and the `| < >` filters need it, and the `body` file
  serves client programs. **The risk this creates, named**: emca writes too (sam,
  `Get`, `+Errors`, steering `sel`), so mirroring is not replication with one
  writer — it is two writers on one buffer, which is collaborative editing and
  OT/CRDT territory. **The existing canvas design already solves it** and the
  solution is lifted rather than reinvented: *"a user edit event is a mutation
  that happened — the device applies insert and delete to the node's data BEFORE
  queueing the event, so presenter echo and node data are one thing."* Single
  source of truth, optimistic local echo. **And divergence must be detectable**,
  because its failure mode is not a crash but silent disagreement — one dropped
  or misapplied edit and every later `s//` operates on text that is not there.
  Each edit carries a **sequence number**, each sync a **hash of the buffer**;
  mismatch triggers resync. Which amends a load-bearing discipline deliberately:
  con(1)'s *"apps never re-read"* becomes **"apps never re-read ROUTINELY"** — a
  resync on detected divergence is a repair path, not normal operation.
  **(2) One undo stack, and it lives in emca.** The host's undo is DISABLED;
  `⌘Z` round-trips. The realisation that makes this simple rather than a
  compromise: **emca is not remote.** It is a process on the same machine
  reached through a SAB mailbox, so a round trip is microseconds. *Every*
  argument that forces web editors into local optimistic undo stacks is a
  latency argument, and none of them apply — so the two-stack problem is one we
  would be inventing rather than one we are forced into. Acme's infinite undo is
  preserved exactly, with no interleaving semantics to explain and no "sometimes
  it undoes a keystroke, sometimes a command". The one exception, flagged for
  later: a *remote* surface under M12's distributed story would feel the trip,
  and only then is local undo worth its complexity.
  **(3) `/mnt/acme` retires, merged into `/dev/window`.** Christine: *"I think we
  may need to retire /mnt/acme."* It is a merge, not a loss — the two are the
  same files pointing opposite ways (a program driving emca; emca driving the
  host), and the only difference was that `/dev/window` declares toolbars and
  actions, which acme's interface arguably should have had. This answers
  window.md's open question in favour of **one window vocabulary**, and it
  carries an obligation: **`/dev/window` must be usable by ordinary programs**,
  not only by emca and the host, because that client interface is the paper's §7
  and the thing that made acme extensible without plugins. Designed for from the
  start, not retrofitted.
  **(4) Versioning is policy over `#V`, and nothing may depend on it.** The
  distinction that settles it: **undo is edit-granular and session-scoped**
  ("un-type that"); **versioning is tree-granular and durable** ("what did this
  look like on Tuesday?"). Neither substitutes — hourly snapshots cannot unpick a
  keystroke, and editor undo cannot recover last week. `#V` as built is already
  the right shape and already explicitly triggered (`#V/ctl` takes `snap [name]`;
  *"a snapshot is a tree; restore is a bind"*, architecture.md), so **the
  mechanism exists and only policy is missing — and policy is a file.** Plan 9
  made the same separation with fossil and venti: the live filesystem and the
  archival store are different concerns and the editor knows about neither.
  Christine's shape, adopted: **optional, off by default, user-configurable, and
  not realtime.** Two trigger axes rather than one — **on `Put`** for anything a
  human authors (a clock snapshot catches a half-finished edit; a version at
  `Put` catches a state somebody *meant*), and **hourly or daily** for things
  that change with no human intention to align to: logs, state, databases. Off
  for `/tmp`, scratch and build output. **Per-namespace**, which falls out of
  per-process namespaces for free and yields a product feature: **an agent's
  sandbox can be versioned aggressively as audit** — P4's *"what could it
  touch?"* becomes *"what did it change?"*, with `restore is a bind` as the undo
  button. **And the rule that keeps the layers independent: because versioning is
  optional, nothing may be built on it.** Undo cannot be "walk to the previous
  version", since a user with versioning off would then have none. `#V` is a
  safety net underneath, never a mechanism anything requires — which is what
  protects (2).

- **(2026-08-31) `/dev/window` belongs to every program, and both halves watch it.**
  Christine: *"any program can write to /dev/window, and in fact it is how emca
  operates"* — emca holds no privilege, only a job. The shape that gives every
  tool: **one binary, and a flag is the only difference between a command-line
  utility and a system manager** — `pkg` lists to stdout, `pkg --emca` mints a
  window and lists them there. **The question this raised** — *"how does the host
  know there is a new window? … either way, we need a `/dev/emca` channel"* — had
  a false premise, and the answer is **no new channel**. The device already mints
  (`#w/clone` does it today), so nothing needs to announce; the real difficulty is
  narrower and was already named in the plan at M13: **9P has no change
  notification**, *"poll, or a synthetic event file"*. The house answer is the
  second, and canvas already uses it. So `/dev/window` grows a **root `clone` and
  a root `events` whose reads park** — `/net/tcp/clone` and `/dev/draw/new`'s
  shape — with two levels of event file: the root's for window lifecycle, each
  window's for what the user did inside it. **Both the host and emca read the
  root's**, which dissolves the choice that was put as either/or: the **device**
  mints (mechanism), **emca watches and places** the window by writing its `ctl`
  (policy), the **host watches and renders** where emca placed it. emca is a
  **watcher, not a gatekeeper** *(RETIRED 2026-09-02: emca is the WINDOW
  MANAGER — without it there are no windows, which is ordinary and is how X
  behaves with no WM running. The phrase and its "default pane" degradation
  were artefacts of the hardcoded-pane design M15 replaced)* — programs mint
  directly so emca has no privilege,
  and emca is not bypassed so it keeps the workspace it owns. Policy in a
  userspace program watching a device is the Plan 9 move, the same shape as
  `/rc/tile` being a window manager in a dozen lines of rc. **Both offered
  alternatives are recorded as rejected**: *programs write, only the host watches*
  loses emca's workspace (a window it never learns of is outside the session it
  owns); *programs ask emca, emca tells the host* makes emca a required
  intermediary and contradicts the premise — it is what acme did through
  `/mnt/acme/new`, and worked only because acme **was** the file server, where here
  the device is. **The property that decided it** *(both the pane model and this
  property were RETIRED 2026-09-02: panes are gone, and the real degradation is
  that with no window manager there are no windows, the devices going straight
  to the host's screen)*: with emca not running the window
  still exists and still renders, in its type's default pane, because the type is
  in the path — so `pkg --emca` works with no shell at all. Neither alternative had
  that. **One race, named**: mint → host renders → emca places, so a window sits
  briefly in its type's default pane rather than its considered one; benign by
  construction, since type already determines default placement, so it never
  appears somewhere *wrong* — only somewhere provisional, moving at most once.

- **(2026-08-31) Building emca amended two things the design had settled, and
  both amendments came from the code refusing to be written the stated way.**
  Recorded because each is a genuine correction, not a detail.
  **(1) `content` is an EVENT, not a sample.** The window contract had emca and
  the surface both *reading* a window's `content` file. But a window is minted
  before its file is known — `clone` first, `content` after — so a watcher that
  reads it once at mint races whoever fills it in, and emca is a watcher by the
  decision immediately above. The device now announces the write on the root
  `events` file. What the fix revealed: the race was already there for the
  surface, hidden only because the surface is push-driven; and the same line
  turns out to be how a surface reopens an existing window on a different file,
  which the design had no mechanism for.
  **(2) `put` notifies; it does not command — ONE WRITER PER FILE.** The buffer
  contract above is right that emca holds the authoritative buffer, and the four
  reasons for it stand. But it does not follow that emca should perform the
  *write*: with a real editor component behind the window, the surface holds
  byte-exact text where emca's copy is RECONSTRUCTED from the change stream. The
  first implementation had the surface write the file and then send `exec Put`,
  so emca wrote again — meaning any reconstruction error would silently overwrite
  correct bytes, which is precisely the failure the sequence-and-hash exists to
  catch, arriving too late to help. Inverted: the surface writes, then notifies,
  and emca re-reads what landed. `exec Put` remains the road for a window with no
  editor behind it, where emca's buffer is the only copy. The divergence hash
  becomes a pure diagnostic — it can no longer corrupt a file — which is what
  makes it cheap enough to always send. **And a measurement fell out**: the hash
  was over UTF-16 code units while the offsets beside it were byte offsets, so it
  would have false-positived on exactly the multi-byte content it exists to
  protect. Both now run over the bytes.
  **A third, smaller**: a window type may not redeclare a core verb, compared by
  LABEL rather than by whole line. Six of the eleven shipped type files declared
  one — mostly `Look` and `Get`, harmlessly, but `text` declared `Put`, which made
  Put appear in a clean window and so made the dirty indicator lie. The rule is
  now enforced in emca and asserted in the suite over `/type/*/window`.

- **(2026-08-31) Building the web surface answered the gating question and
  corrected the responsive rules' own reading.** M14d's stated precondition was
  *can `execute` and `look` ride Monaco's action and context-menu API?* — to be
  proven first, not assumed. Answered, and with a different component:
  **CodeMirror 6, which has no menu of its own and uses the platform's.** That
  is *stronger* than the Monaco route, not a compromise: the verbs ride the
  surface the user already right-clicks, rather than a component-specific menu a
  different surface would have to reimplement. Property 1 survives a rich editor.
  **The floating bar moved from right-click to the selection**, which is what
  *operand determines surface* actually requires — position encodes what a verb
  acts on. And splitting `look` into Open/Jump/Search **re-divided the labour**:
  Open, Execute, Pin and Edit are IPNX's; Jump and Search are the surface's,
  because they move a caret inside a buffer it already holds; Cut/Copy/Paste stay
  the platform's. Two ambiguities had to be settled to make the bar honest, and
  both resolve to acme's own order rather than to a guess: **a path that exists
  beats an address**, and **a regexp address is delimited at both ends** —
  without that second rule every absolute path reads as `/regexp/` and
  `/etc/motd` offers Jump, which is precisely the silent misjudgement the bar
  exists to expose.
  **The correction worth recording** is in the responsive rules. The table's
  `panes: none` at small was implemented as *hide them*, and the result deleted
  the home listing and the console outright — the exact information loss
  "nothing disappears as the viewport grows" was written to forbid, arriving from
  the other direction. **Collapsed is not hidden**: small INLINES both panes as
  concertina rows showing their tags. Which exposed the structural bug beneath
  it — only windows IPNX had declared chrome for owned a tag row, so the console
  had nothing to collapse *to* and vanished at zero height. Every window now
  carries one and IPNX's chrome **enriches** it. That is what *"a concertina row,
  a rail entry, a tab and a minimised window are the same object"* costs in code,
  and the design asserted it without anything enforcing it.
  **Two channels the design had not named.** `verbs` per window is the sanctioned
  third protocol addition (verb applicability) in the only form the narrowed
  architecture allows — a file, the parallel of `toolbar` one operand narrower;
  its stated form, "an attr in response to a select event", belonged to the
  canvas protocol that has since narrowed. `/dev/window/pin` is a genuine **gap
  the build found**: the pin is IPNX's by the design's own test and `execute` is
  emca's, so emca must hold it — but nothing carried workspace-scope state to a
  surface, and a status line cannot show what it cannot read. And naming a
  running command needed **`/proc/<pid>/args`**, proc(3)'s own file, whose data
  sat in the proc record all along with nothing able to read it.

- **(2026-09-01) THE COMPOSITOR: one object, composited recursively — and
  the redesign that should have come first.** Christine, stopping a build that
  was going wrong: *"We need a compositor - something that arranges windows into
  columns and rows, and it is recursive. each window itself is a compositor that
  can further decompose into windows... The entire browser surface (or macos/ios
  screen) starts off as one giant window, of type root."* And the diagnosis:
  *"The first thing we should have designed was the compositor and the root
  window. If we get this right then we have correct behaviour, window controls,
  resizing, tabs, panes, etc."*
  **The evidence, from acme's own source.** An earlier reading here called acme
  "three fixed levels, not recursive" and treated Row and Column as containers of
  a different kind from Window. `dat.h` refutes it — all three are a rectangle, a
  tag, and either children or a body — and `cols.c`'s `colcloseall()` closes a
  column exactly as a window closes, `textclose()` on its tag then `winclose()`
  on everything inside. **A column IS a window**, one whose content is windows.
  Acme stops at depth three, but nothing in the object requires the stop.
  **What this makes wrong, and it is most of a day's work.** The implementation
  had a `PANES` map holding `left`, `main`, `bottom`, a `pane <name>` verb in
  `wctl`, and a `pane` file per type naming one of those strings. Once placement
  is a NAME, composition is frozen at three slots, nothing nests, and rows and
  columns are decoration. Every symptom traced to it: windows that could not
  split, a "rail" that was really region #1, a bottom pane that existed because a
  string was declared. Her own diagnosis of the naming — *"Where you have been
  confused (judging by you naming panes as Rail, Transcript etc) is that panes
  are not special, they are just normal windows"* — is the same fault seen from
  the vocabulary end.
  **ALLOCATION, which arrived last and simplified the rest.** The first pass at
  the controls had maximise MINIMISE every sibling — and Christine caught that it
  does not work: *"a window with minimised rows still take up space (one line per
  row). so maximising a window may not actually give much extra room."* True, and
  the fix she pointed at — *"if we are adopting a stack metaphor, then minimise
  should be minimising into a stack"* — collapses the whole layout model into one
  sentence: **a parent allocates rectangles to some of its children along an
  axis, and those it does not allocate to appear as tabs.** minimise(me) moves me
  out of the allocation; maximise(me) moves everyone else out; each is undone by
  moving back. One mechanism, two arguments, O(1) space either way. What falls
  out of it, none of which had to be written: a tabbed window IS a maximised one;
  "tabs display only when there is more than one" is not a rule but an empty
  strip having nothing to draw; stack stops being a third composition beside row
  and column; a minimised container takes its contents with it because they are
  inside it; and the small breakpoint stops being a "concertina" — it is the
  ordinary allocation rule with room for one rectangle, driven by the same
  mechanism a person drives with the minimise button, which is why the compositor
  can no longer delete a window by accident. It also retired, unbuilt, an
  elaborate design for a rotated minimised strip: a tab is a WHOLE window the
  parent has not given a rectangle to, so the questions that design agonised over
  — a title too cramped to edit, one button standing for three, a close control
  made unreachable — simply do not arise.
  **The principles, and the sentence that governs them**: *"The design is not a
  set of exceptions, it's a few principles applied consistently."* One object.
  Every window composites itself into a row, a column or a stack of tabs.
  Controls INFORM THE PARENT — which is what makes recursion work, and which
  retires the earlier refusal of maximise (that objection reasoned about a child
  in isolation; maximise is an instruction to the parent, and the parent already
  remembers an arrangement). The tag line is an OPERAND, not a command line: its
  text is the argument to whichever toolbar verb is pressed, which is the 2-1
  chord decomposed for keyboard and finger — **and which retires both the
  floating bar and the pin**, two mechanisms built for one job. Every window has
  the same four components and its own status bar. Panes are windows the root
  created by convention, and nothing downstream can tell them from any other.
  **All window verbs are always visible.** The implementation hid Newcol and
  friends behind an overflow on the judgement that they were rare. Hers: *"They
  are not, all the window controls need to be available at all times, they are
  part of the UI."* A narrow toolbar WRAPS — geometry answering geometry, never a
  ranking of importance.
  **The unit is device-independent pixels, plus a reported text cell.** Characters
  remain the leaf measure (72 x cellWidth) so accessibility sizing still moves the
  breakpoints, but they cannot be the unit: *"not all windows display text. emca
  needs to be able to know how to fit an image into a window... it knows what an
  image is (or video, or postscript etc) and what aspect ratio is."* Acme could
  measure in characters because everything was text. Her precedent: *"this is why
  macos uses postscript underlying"* — Display PostScript and Quartz are
  device-independent imaging models for exactly this reason.
  **And emca owns the tree.** *"Host tells emca - we have a 800x1024 window. emca
  says 'Ok, we need to apply this responsive layout' create these windows... emca
  owns the tree, and the host renders the tree."* With the reason: *"emca needs to
  understand the geometry as well, so it does not ask the host impossible
  things."* This amends PART ONE's *"emca never learns the viewport's width"* —
  it does now, and must. A gain falls out: the responsive rules become testable
  headlessly, since rc can write a geometry and read back the tree.

- **(2026-09-01) THE VERBS: the era's names, four surfaces with one operand
  each, and the floating bar restored.** Christine: *"acme names for the
  builtins are idiosyncratic (snarf, zerox, put, get, etc.). They have not stood
  the test of time, and are against Apple HIG… This only applies to emca, not
  acme. Acme of course retains it's naming."* So emca says Copy, Save, Revert,
  Open; acme's port keeps Snarf, Put, Get and Zerox, because renaming acme's
  buttons would be changing acme rather than porting it.
  **The grouping is by operand, and it resolves an overlap the first draft had.**
  Hers: some operations are window operations (newcol, newrow), some operate on
  the body (cut, copy, paste), some on the tag line itself (run, search, add).
  Four surfaces, each with exactly one operand: the window as a thing in a layout
  → the **title bar row**, beside the controls, since these are not about what
  the window holds; the window's **content** → the toolbar; the **tag line's
  text** → its own buttons, kept separate so it is visible that Run acts on what
  you just typed and Save does not; a **selection in the body** → the floating
  bar.
  **THE FLOATING BAR WAS RETIRED IN ERROR AND IS RESTORED.** The reasoning for
  dropping it — that the tag line plus the toolbar already did the job — missed
  that they take different operands: the tag line holds text you COMPOSE, the bar
  acts on text you POINT AT, and acme's chord was the second. Hers, restoring it:
  *"The floating toolbar is still the floating toolbar, so it is context
  sensitive to the window body."* That the same three words appear on two
  surfaces is not duplication but the rule working.
  **Two corrections fell out of the audit.** The toolbar is NOT a closed set: her
  `Add` verb puts the tag line's text on it as a button, which is how acme's
  "type Indent in the tag and it works" becomes durable — so the CORE is closed
  and the toolbar is extensible. And **New column / New row DUPLICATE** this
  window rather than creating an empty one (*"new col (in reality duplicate
  horizontally)"*), which subsumes Zerox entirely: you duplicate, then retitle,
  because the title retargets. One verb where acme had three.
  **`Run` and `Open` are acme's two mouse buttons**, and the distinction is who
  decides what the text is: `mk` is both a command and a file in `/bin`, so acme
  spent a button on the choice and emca spends a button on it. Christine's
  statement of Open is the one the spec now carries, because it is expressed in
  machinery already present: *"open is open a new window with tagline as title"*
  — the title retargets, so the window shows what the title names and its type
  follows. Open and New-column-then-retitle become the same act arriving two
  ways, which leaves exactly one way a window comes to show something.
  **`jump` and `search` merge into `Find`**, on her question *"why can't goto
  and search be the same operation?"* — they can, because sam's address language
  already includes regexp search as an address form, so `:/alice/` and `alice`
  land in the same place by two notations for one act. The distinction is
  explicit USER syntax (`:` and `#`), not a system guess, so it does not weaken
  "look is a dispatcher, show the choice" — that principle is about the system
  choosing silently. `Open` still needs its own button precisely because no
  notation separates a filename from a word in the text: `mk` is both, which is
  what acme spent a mouse button on. Named Find, not Go to, because that is the
  era's word for the common case.
  **DOT IS A SET, and that dissolves multi-cursor as a separate problem.** Her
  proposal, and it is better than the answer it replaced: *"Find by default sets
  multi-cursor and selects all matches. So a replace is just the user typing in
  next text… Edit by default operates on selected text. s/a/b/ means for all
  selected instances, replace a with b."* So **Find is `,x/foo/`** — the loop
  over the whole file — and **Edit's default address is dot**, which is already
  sam's own default. The commonest edit anyone makes then needs no sam at all:
  Find, then type. THE SURFACE'S MULTI-CURSOR AND EMCA'S MULTI-RANGE DOT BECOME
  ONE OBJECT rather than two notions to keep in step — the no-exceptions rule
  again, arriving where I had been about to write a division of labour instead.
  The recommendation it overturned was mine: single-range dot with multi-cursor
  left to the surface and range verbs acting on the primary selection. That
  would have bought a weaker version of something sam already had.
  **Four consequences, none of them hidden.** `select` carries a LIST of ranges
  (window.md). Repeated Find is idempotent, so next/previous match is navigation
  and therefore the surface's — there is no "find next" verb. This EXTENDS sam,
  where `curfile->dot` is one range and `x` iterates, so **the acme port sees the
  primary range**, since `/mnt/acme`'s addr and data assume a single dot — written
  down or acme breaks quietly. And selective replace is what is given up, covered
  better than it was lost: `x/foo/ g/bar/ c/baz/` is strictly more expressive
  than a Replace/Skip button.
  **`Edit` came back**, having been dropped too fast on the reasoning that sam's
  language "is just a command, so type it and press Run". It is not: Run execs a
  program and `s/a/b/` is not one, and no notation separates them — `x`, `y`, `g`
  and `v` are sam commands and plausible program names both. Which is the rule
  the previous entry produced, applied: a button is warranted exactly where the
  text cannot say which operation is meant. A stronger form of it also emerged —
  **a verb that modifies is never inferred**. Find is safe by construction; Edit
  announces that it might change things.
  **`Sort` is declined**, the one operation of the 38 not placed. acme's
  `colsort()` reorders a column alphabetically — a one-shot tidy answering a
  problem emca solves with tabs, and the only operation in the set that destroys
  a deliberate arrangement with no way back. `Fit` earns its destructiveness
  because sizes drift accidentally; order does not drift, it is set on purpose.
  **The other 37 are accounted for in the spec**, and three
  acme builtins disappear as buttons because something else already does the
  work: Zerox (New column duplicates), Edit (sam's language is a command, so you
  type it and press Run), and ID (it is state, so it lives in the status bar).

- **(2026-09-01) THE VERBS SETTLE INTO THREE GROUPS, and two of them are
  universal.** Sorting by what VARIES rather than by operand alone: WINDOW
  operations (the title bar row) are always universal; TEXT operations (the tag
  line's six, and the floating bar's five) are universal too; and the TOOLBAR is
  never universal — it is entirely the type's, and the only surface a person can
  change, since `Add` writes to it. Fixed rows, one extensible row.
  **What makes the text operations universal is not fiat**: every window's body
  is A SEQUENCE OF ITEMS. Find selects a subset, Edit and Pipe operate on it, and
  only what an ITEM is varies — a text range in `edit`, a file in `ls`, an output
  line in `shell`, a child window in `root`. That is sam's structural model
  lifted from characters to items of any kind, and it is the same reason dot is a
  set: `x`'s loop is the general case and single-range dot was the special one.
  **Three command verbs, which are acme's own three** (execute, the filters,
  Edit), named for what they do. **Run ignores dot; Pipe iterates over it; Edit
  changes an item's text.** Pipe writes the selected items to a command's stdin,
  one per line, and opens the results in a new window — one semantic with no
  per-type behaviour, because where a command wants arguments you pipe through
  `xargs`, which is exactly what `xargs` is for. We did not have to invent a `{}`
  substitution: Unix already solved it, and the converter is a program you can
  see. And Edit is universal because it has ONE semantic with the type supplying
  only what an item's text is — in `ls` that renames, and in `root` it RETARGETS,
  since titles retarget. The root case fell out rather than being designed.
  **Nothing computes applicability, and that deleted a mechanism.** An earlier
  draft had Open and Find appear only when they applied, which needed emca to
  judge the text and a `verbs` file to carry the answer. Christine: *"open and
  find should always apply… Both should always be true."* They do, so they are
  always there — which gives "look is a dispatcher, SHOW the choice" a better
  answer than computing it: both buttons are permanently visible, no round trip,
  nothing to get wrong. The `verbs` file stripped in M15a stays stripped.
  **Creation stops being a verb at all.** `Open` on a name that does not exist
  gives a window Save will create; `Open tmp/` — a trailing slash — makes a
  directory. So neither "New File" nor "New Folder" exists anywhere, and `ls`
  carries exactly one verb of its own. The single asymmetry is forced rather than
  chosen: a directory has no content and therefore no later Save at which to
  create it, so the slash form creates immediately.
  **AN EMPTY TAG LINE MEANS "USE THE SELECTION", and it demoted a whole
  surface.** The floating bar had been specified as a required fourth surface,
  which collided with the platform's own selection callout on every touch
  system — two popovers, or suppress the native one and lose what it does. Her
  three residual worries about it were the right ones: *"visibility, occluding,
  and layout shift"*. The rule answers the first and reframes the rest. Select
  text, leave the tag line empty, press any of its six buttons and they act on
  the selection — macOS's Cmd-E convention, not an invention, and it generalises
  with no special case. So the always-visible buttons already act on pointed-at
  text, and they offer FOUR MORE VERBS than the floating bar ever did (Run on a
  selection is exactly acme's button-2-on-text; Pipe and Edit were never reachable
  from the bar at all).
  **Which makes the selection surface OPTIONAL.** Three surfaces are required —
  the title bar row, the tag line and toolbar row, the status bar. Which verbs a
  selection affords is IPNX's; WHERE THEY APPEAR is the surface's, so it is the
  native callout on iPadOS, the context menu on the web, or nothing at all. That
  is the founding division applied to the one surface that had been
  over-specified. A surface that does offer one is bound by three requirements,
  because a bad answer is a defect rather than a matter of taste: never reflow
  the body (text jumping under the reader is the only failure that loses your
  place), never cover the selection, dismiss cheaply. And testability improves:
  a native callout is not drivable from a headless test, but the `ui` file
  already declares what was rendered, so the suite asserts the verb set was
  declared and is keyboard-reachable — the property that matters — identically
  whether the surface drew a popover, a menu, or nothing.
  **The window's second row is reordered, and gains a separator.** Hers: the
  window is controls-and-title / tag line, tag line buttons, separator, toolbar
  buttons / content and scrollbars / status bar. So the row reads left to right
  as OPERAND, then the verbs that consume it, then a rule of ink, then the verbs
  that do not — the toolbar's type verbs, to which the tag line is nothing. That
  separator is the only place in this design where a line of ink carries meaning,
  and it earns it: without it a person cannot see which buttons will consume what
  they just typed. Operand-determines-surface, made visible inside a single row.
  **UNDO IS NOT UNIVERSAL**, on her question *"how do we undo a process kill?"* —
  you cannot. The rule: Undo is available exactly where every operation the
  window offers is confined to a buffer emca holds, which is `edit` alone of the
  four. Same root cause as the unit decision: acme could make Undo a universal
  builtin because every acme window was a text buffer, just as it could measure
  layout in characters. Both universals were artefacts of one content kind, and
  neither survives four types. For the irreversible verbs, acme's `Del` rule
  generalises — destructive verbs REFUSE when there is unrecoverable state and
  otherwise simply happen. No confirmation dialogs: they train people to click
  through and protect nothing.

- **(2026-09-01) TABS ARE THE DEFAULT, AND /output MIRRORS THE FILESYSTEM.**
  *(The second half was SUPERSEDED on 2026-09-02: a command line contains `/`
  and cannot be a filename, so `/output/<n>/` took `/proc`'s shape — a number,
  with `cmd`, `dir`, `0`, `1`, `2`, `status` and `log` as files inside. Tabs
  stand.)*
  Two late corrections, both hers, and both removing something rather than
  adding it.
  **Rows and columns are only ever created by a user.** The draft rule was mine
  — a new window "allocated if it fits, a tab if it does not" — and it was
  wrong twice over: it made THE SAME GESTURE DO DIFFERENT THINGS AT DIFFERENT
  WINDOW SIZES, which is inference of the kind this design keeps deleting, and it
  let the machine restructure a workspace nobody asked it to touch. Hers: *"new
  rows and new columns should only ever be created by a user, so an open window
  opens a new tab by default… It is also the safest option, that does not destroy
  current layout."* So Open, Run and Pipe always tab. `New tab` joins the title
  bar row as the third creation verb — the same operation with the allocation bit
  flipped, since a tab is a child with no rectangle. And her consequence turned
  out to need no rule: *"only the active tab shows the tagline, toolbar and status
  bar"* is what an unallocated window already IS.
  **Drag and drop is acme's `move`**, already in the census as gesture-only. Two
  drop targets: onto a window re-parents it as a tab, onto an edge makes a row or
  column — which is the OTHER way a user creates a split, keeping "only a user
  creates rows and columns" true with two roads to it.
  **`/output` is a filesystem, and it mirrors the filesystem.** A window's title
  is a path, because titles retarget, so an output window needs a path that is
  REAL. Two candidates failed in opposite directions: acme's `<dir>/+Errors` gets
  the context right but lies about what is there, and a file in `/tmp` is truthful
  but gives the wrong context (the title determines the directory, so Run would
  re-run in /tmp) and dangles. Hers, resolving it *(the mirror form here was
  superseded 2026-09-02 by `/output/<n>/`; what survives is that output is a
  filesystem served by emca and that WHERE it ran must be recorded)*:
  *"/output/home/project/ipnx/mk
  all" so we know which directory mk was run in.* The mirror pays twice — it
  resolves the collision of one command run in two projects, and it REMOVES an
  exception rather than accommodating one: with the directory in the path, context
  is derived from the title by stripping `/output`, so the type keeps no private
  field. A parallel tree indexed by path is the house shape already: `#V/<snap>/…`
  for versions, `/n/` for mounted worlds.

- **(2026-09-01, later) SARANOS IS A SYMBIOSIS OF HOST AND WASM, and that is why
  it needs its own name.** The entry below got the naming right and the SCOPE
  wrong, and the wrong version was propagated into CLAUDE.md, architecture.md
  and the landing page before it was caught. Hers, spelling it out: *"Saranos as
  an OPERATING SYSTEM encompasses host side and WASM side as well… that's why
  it's different from IPNX, which only describes the kernel and userspace, and
  that's why Saranos is a different name. It is a symbiosis between host and
  WASM, neither can exist without the other."*
  **What was wrong**: CLAUDE.md said *"wasm and the surfaces are the machine it
  runs on"*, which puts the host UNDERNEATH the system as substrate. It is not
  underneath, it is INSIDE. The kernel is wasm and cannot run without a host to
  give it workers, memory and a screen; the host has nothing to do without the
  kernel. Calling the host "the machine" makes Saranos a synonym for IPNX plus a
  UI, and then the second name is decoration — which is exactly the reading the
  name exists to prevent.
  **What it explains, immediately**: Rust is already in the system, because the
  kernel IS Rust compiled to wasm. Hers again: *"We already have a kernel in
  Rust that compiles to WASM, clearly Rust is in the system. Creating a Rust
  package is a to do."* So the earlier framing — "there is no Rust toolchain" —
  described the absence correctly and the system wrongly. And `emca` stops being
  a layer that sits on one side: the program is a guest, the surface is the
  host's, and it is one system precisely because those two halves are one thing.

- **(2026-09-01) The layering, sharpened: Saranos is the OPERATING SYSTEM.** The
  2026-08-31 entry made Saranos "the user experience on top of" IPNX — which was
  right that it needed a name and wrong about which name. Hers, settling it:
  *"saranos is the name of the operating system, emca is the windowing and UI
  system, IPNX is the kernel and userspace."* So the three-way maps exactly onto
  **macOS / Darwin / Aqua**, and emca — not Saranos — is the layer whose job is
  the interface. Propagated: `docs/architecture.md` states it up front, the
  landing page and the browser tab say Saranos rather than IPNX v12, and
  `/etc/motd` is hers: *"Welcome to Saranos, a modern operating system based on
  IPNX - a reimagining of UNIX."*
  **And it caught a fragility worth recording.** Six C-level assertions proved a
  namespace was intact by looking for the word "hello" IN THE MOTD — so changing
  the greeting broke seven tests. They now look for "Saranos", which is a better
  assertion as well as a stabler one: it says the REAL motd is in view, where the
  alt view used by those tests says "the child's private view of /etc/motd" and
  never names the system. A test that depends on a greeting's wording is a test
  that will drift.

- **(2026-09-01) acme and emca are two documents about two things.** Hers:
  *"acme is Bell Labs program. We are going to update it to fit emca, but not
  change functionality. emca is effectively our new windowing system and UI.
  Don't confuse between the two."* So `docs/acme.md` stops being "the anatomy
  emca derives from" and becomes **the port spec**: how acme is modified to fit
  into emca, functionality preserved. `docs/emca.md` is the windowing system —
  what a window is, how the compositor works, window types. The anatomy was
  input, not parentage, and describing emca as "derived from acme" invited
  exactly the confusion that had me editing acme's own record to justify emca's
  design.
  **The port decision, taken**: acme's Row/Column/Window **become emca windows**.
  Acme today is its own compositor — it tiles internally and paints through
  libdraw on `/dev/draw`. Under emca it stops window-managing and delegates
  composition, keeping its verbs, its executable text and its `/mnt/acme` file
  server intact. The alternative — acme as one opaque window that draws itself —
  is less work but makes it a guest rather than a citizen, and none of emca's
  furniture would reach its columns.

- **(2026-08-31) The open questions, resolved in one pass — and what it revealed
  about them.** Eleven items stood open across [emca.md](emca.md) and
  [window.md](window.md); worked through together on Christine's instruction,
  they resolved almost entirely by reasoning from decisions already taken, which
  is itself the finding: **most of them were consequences waiting to be noticed
  rather than questions waiting to be answered.** **Two were stale** — *who owns
  undo* and *buffer fidelity* had been settled by the buffer-contract decision
  and never struck out; a list that keeps answered items is a list that stops
  being read. **One dissolved**: *how a type declares a view mode without
  becoming a widget toolkit* has no answer because a type declares **nothing
  about rendering** — the host opens the file and renders it natively, so there
  is no vocabulary to grow into and the dozen-kinds tripwire has nothing to trip.
  The canvas redesign removed the risk rather than bounding it, which is worth
  noting: the right architecture deletes questions instead of answering them.
  **One resolved by adoption rather than design**: the plumber takes Plan 9's
  **plumb(6) rules syntax and message format verbatim** — *adopt the notation,
  own the model*, the same move that took SVG path data, and the standing
  instruction to maximise reuse of existing protocols applied where it was
  cheapest. **One resolved by reframing**: *property 1 under Monaco* is not a
  risk about Monaco but a **selection criterion for any editor component** —
  expose the selection, accept custom context-menu commands, or be disqualified.
  Monaco, CodeMirror and TextKit all qualify. The spike still runs to verify the
  chosen one; it no longer gates the design. **And one produced genuinely new
  mechanism**: *the surface's own suite*, flagged as the largest untested area
  with no precedent to borrow. Resolved by having **the surface publish what it
  rendered** — a `ui` file per window listing each control's label, role,
  keyboard path and enabled state, **derived from the platform accessibility
  tree** on real surfaces and synthesised on the virtual one. The a11y tree is
  precisely the "can this be reached" answer, on the web and on Apple alike, so
  one assertion — *every floating-bar verb and every toolbar button is present,
  named and keyboard-reachable* — runs on every surface including headless. That
  converts a claim into a measurement: the input convention has held since
  2026-08-30 that accessibility is *"enforced by construction"*, and nothing
  checked it until now. **The remainder, resolved plainly**: snapshot-vs-replay
  is snapshot and it is *free*, because a workspace's writable layer already is a
  tree and keeping it is the snapshot (replay stays available as recorded
  provenance); an unnamed instance gets its layer at birth under
  `/usr/$user/.inst/<id>` so saving is a **rename, not a copy**, and there is no
  window in which work sits somewhere it could be lost; cross-window at small is
  a **stated degradation** — operation works via the pin at every size, viewing
  two bodies on one small screen is impossible for any design and is named rather
  than solved; the floating bar's **set** is closed and derived from acme's layer
  3 while its **order and grouping** stay empirical; and surface dependency
  divergence was never open, being the input convention working as intended.
  **Also specified in the same pass** ([window.md](window.md)): the event
  vocabulary, reusing canvas's verbs rather than inventing a second set, with a
  root `events` for lifecycle and a per-window one for user action; and the
  **transcript**, which is not a special mechanism at all — its content is a
  growing file the host tails, typed lines return as ordinary `insert`, and
  con(1)'s mark arithmetic is **deleted** because the input region is just the
  host's editable tail. **What actually remains** is small and honest: the
  floating bar's arrangement, the editor component's verification spike, and the
  fact that **none of it is built** — every decision here is design, M14 is
  unstarted, and the suite's 151 pass because the code has not moved.

- **(2026-08-27) The native host is a Rust kernel core plus per-platform embedding
  shims — after the PoC completes.** The kernel never executes guest code, so the
  core (proc table, namespaces, devices, 9P, the draw engine) compiles once in Rust
  and twice over: native for macOS/iPadOS (guests on wasmtime or wasmi — interpreter
  paths, iOS-legal) and to wasm for the browser (guests stay on the browser's own
  engine). Each platform keeps a thin mach layer — Workers/SharedArrayBuffer glue in
  JS for the browser, threads plus the runtime embedding natively — which is Plan 9's
  own `port/`-vs-`pc/` split with the browser as just another machine. This
  supersedes the WasmKit-in-Swift candidate (kept in the table above as history);
  WasmKit remains an option for the shim's app shell, not the kernel. The 124-test
  suite is guest-side and language-blind: it is the conformance spec any second
  kernel must pass. With the PoC closed (same day), `kernel.mjs` is the reference
  implementation the Rust core is measured against.

- **(2026-08-27) The engine matrix: wasmtime everywhere, mode per shim; WasmKit
  stays superseded.** iOS forbids *two* things, not one: JIT (the
  `dynamic-codesigning` entitlement is Apple-only) **and runtime-loaded AOT** —
  every executable page must come from a signed binary in the bundle, so
  `wasmtime compile` artifacts mapped at runtime are just as illegal as
  Cranelift. Wasmtime's answer is **Pulley**, its portable-bytecode
  interpreter, with `Config::signals_based_traps(false)` so traps are explicit
  rather than guard-page signals: in that mode wasmtime executes no unsigned
  native code at all. The matrix: **macOS — Cranelift JIT** (legal even in the
  Mac App Store under the `allow-jit` entitlement); **Linux/OCI/microVM —
  Cranelift, or AOT `.cwasm` for cold-start**; **iOS/iPadOS — Pulley, signals
  off**. Caveats recorded: `aarch64-apple-ios` is a tier-3 wasmtime target, and
  Pulley is an interpreter — several times slower than Cranelift; fine for rc
  and the editors, *measured* before it is trusted for CPython and Go. The
  engine sits behind one trait in the Rust core, and the like-for-like fallback
  is **wasmi** (which deliberately mirrors the wasmtime API), not WasmKit — the
  Swift candidate would only return if the Rust core itself were abandoned.
  None of the load-bearing machinery cares which engine runs: the fork guard,
  `setjmp/longjmp` and libthread are asyncify — instrumentation inside the
  modules, engine-blind — and fuel/epoch preemption (the OCI entry's need)
  exists in both wasmtime-with-Pulley and wasmi. App Store precedent for the
  interpreter shape is settled practice (iSH ships a usermode x86 Linux
  emulator; UTM SE an interpreter-only VM); wasm binaries shipped in the bundle
  are unambiguous, and user-loaded programs are the standard interpreted-code
  gray zone (guideline 2.5.2 / agreement 3.3.1B) every scriptable app lives in
  — a policy risk, not a technical one.

- **(2026-08-27) iOS local files: always a file server over user-granted
  subtrees — and the browser sandbox is not the barrier it looks like.** iOS
  has no full-disk access for any app, ever: an app sees its own container
  (Documents, surfaceable in the Files app) plus exactly the subtrees the user
  picks through the document picker, persisted as security-scoped bookmarks —
  iCloud Drive, USB drives and file-provider shares included. That consent
  model *is* the ipnx worldview: **a security-scoped bookmark is a capability
  to a subtree, and a granted folder enters the system as a bind** (`bind
  /host/ProjectX /usr/work`) — the platform enforces what the namespace design
  argues for. It composes with the storage invariant: the grant arrives as a
  host-backed file server; ipnx never learns an on-disk format. The three
  deployment forms differ only in plumbing: **plain Safari** — OPFS-only
  private world plus one-shot imports (WebKit never implemented the
  File System Access API), fine for the synthetic rootfs, no real user files;
  **WKWebView shell (the iPad stopgap)** — web content sandboxed as in Safari,
  but the app's native half serves granted files across the bridge
  (`WKURLSchemeHandler`, already needed for COOP/COEP, plus the script-message
  channel), i.e. the native half is literally a file server the kernel mounts —
  the cost is serialization per byte, measured before `git status` meets a big
  repository; **native app (the real milestone)** — the same consent model with
  no web layer. The WKWebView shell also carries a performance irony worth
  keeping: WebKit's content process holds the JIT entitlement, so the
  already-green browser port runs on iPad *with full JIT today* — faster
  execution than the native Pulley build it is a stopgap for.

- **(2026-08-29) su without a superuser — the security landing.** The
  README's "there is no superuser" is structural, at three boundaries: the
  sandbox above (nothing to escalate to), device policy within (the eve
  bypass belongs to each device, not the core), and the mount table outward
  (eve-ness does not serialize; each server applies its own policy to the
  attach-time `uname` — the superuser's reach ends where root's always did
  in practice, at the NFS boundary). `su` is identity transition under
  docs/identity.md's two rules, never escalation: a personality-layer command
  over `/proc/self/ctl`, no password, no setuid machinery; `su none` — the
  privilege-drop shell — is the celebrated direction and the per-agent
  primitive; kill comes free through note permissions (V10's euid rule).
  Recorded impossibility, a feature: namespace-wide write is the union of
  per-server grants fixed at attach, so it is not a grantable thing — su
  re-evaluates local authority instantly and can only request more by
  re-attaching (auth/as's shape). **The namespace unions services; it
  cannot union their trust.** Earmarked: Plan 9's devcap (`#¤`,
  caphash/capuse) as the authenticated third rule ("user X, bearing
  proof"), sequenced with /net and factotum-shaped work; uids stay
  per-server names with the personality owning name↔number. Landed with
  the entry: `cmd/su.c`, `#p` in the boot namespace, and the suite's 131st
  test (`su none id` → `none none`) green on all three hosts.

- **(2026-08-29) What a "user" is — the conflation decomposed.** Unix's uid
  merged four things: a person at a terminal (originally a *billing*
  construct), a protection domain, a service principal (`lp`, `uucp` — the
  daemon users, never logged into, the part that aged best), and root, the
  anti-user. IPNX keeps the kernel's mechanism minimal (a name pair per
  process, ownership, the transition rules, `DMSETUID`, the per-attach
  `uname` — all already running) and resolves the CONCEPT four ways:
  **(1) the person is eve, exactly one per kernel instance, many instances
  per person** — timesharing is inverted, not restored: kernels cost a
  browser tab, so multi-tenancy happens by instance and *the kernel
  instance is the new uid*; "fast user switching" is a different instance;
  in-kernel multi-user survives only where Plan 9 put it, at servers, per
  attach. **(2) The role is the daemon user, kept deliberately and
  ennobled**: a name that owns resources and is conferred, never logged
  into — assumed by DMSETUID exec, eve's grant, or a future devcap ticket
  — and where Unix daemons got only a uid, IPNX daemons get a reduced
  namespace (systemd's forty sandboxing directives are a namespace system
  described one flag at a time; here confinement is the binds a daemon
  started with). **(3) The agent is a role plus a namespace** — the
  genuinely new population, already the largest identity population in
  the industry: the name is for the audit trail, the namespace is the
  authority; `none` is the anonymous agent and `su none` its front door.
  **(4) The network person is an authenticated claim, per connection** —
  no global registry (NIS/LDAP sediment, refused); servers believe
  proofs, today the attach-time uname, later factotum-shaped tickets
  with /net; a person across their instances is their keyring. The
  organising sentence: **names are for accounting; namespaces are for
  authority.** Consequences at zero mechanism cost: `login` never exists
  (no getty — a person does not log into their own instance),
  `/etc/passwd` stays personality-side (V10 numbers, plus the curated
  V10 role names as heritage), authentication stays sequenced with /net,
  and groups (deferral D2) get less urgent because roles absorb most of
  what groups did.

- **(2026-08-29) The profile: identity's configuration, unified.** Plan 9
  built the user profile in four scattered pieces nobody named as one
  thing: factotum (the key agent — secrets in memory only, protocol steps
  performed on the owner's behalf, mounted as a file server), secstore
  (the durable *networked* keyring, one strong unlock at boot — agent in
  RAM, store on the network: a distributed password manager in 1999),
  `$home/lib/profile` (the per-user namespace script), and
  `/lib/namespace` (the declarative bind/mount language `newns`
  interprets). The industry rebuilt each piece separately — password
  managers and platform keychains are secstore, passkeys are factotum's
  sign-the-challenge model winning late, dotfiles and kubeconfig contexts
  are lib/profile — and unified nothing. IPNX unifies them as **the
  profile: a file tree served by a userspace file server** (the kernel's
  fifth consecutive identity decision costing it zero lines), mounted at
  `/mnt/profile`:
  `namespace/` holds fragment files in the /lib/namespace little language
  — `base` + `device/<name>` + `service/<svc>`, unioned by context
  (ssh_config's Host blocks, kubeconfig's contexts, done as files);
  `services/` is the fstab-of-the-person (dial address, protocol, aname,
  mountpoint, credential *reference*); `keys/` is the agent interface and
  **secrets are never ordinary readable files** — programs write a
  challenge and read a response, use-don't-read, which also lets each
  platform shim back the store with its native secure enclave while the
  profile stays portable text. "Rights" is deliberately NOT a subtree:
  by the user decision, rights are namespaces plus roles, so the profile
  records how to *exercise* them (which fragments, which role tickets)
  and grants nothing — servers still decide. Three portability rules:
  **the profile speaks IPNX names, never host paths** (each shim maps
  "the home directory" to its device — the profile never learns a host
  path, as the kernel never learns an on-disk format); **construction is
  best-effort** (an unreachable NAS mounts nothing; a laptop on a plane
  is not a broken profile); **the store is remote and durable, the agent
  local and volatile** — the durable profile is text plus encrypted
  blobs and lives in anything mountable, including a git repo (versioned,
  diffable, rollback-able identity for free), unlocked once per device
  (PAK then, passkey-shaped now). Delegation falls out: an AI agent's
  identity (role + namespace) is exactly *a sub-profile* — fewer
  fragments, scoped tickets, its own audit name. Refused: kernel
  mechanism, a global identity provider (the profile presents proofs;
  servers verify per connection), plaintext secrets, and package
  management (this is not Nix). Factotum is adopted as a SHAPE, not a
  program — its protocols are period pieces, its design (agent as file
  server, use-don't-read, delegation) is the ancestor. **Sequencing in
  two stages, neither throwaway**: the namespace half needs nothing and
  can land now (boot already wants "rc plus a namespace file"; per-agent
  sub-profiles included); the credential half needs /net to reach stores
  and services, and pairs with devcap.

- **(2026-08-29) The PoC is declared complete; the implementation begins.**
  Nothing was left on the PoC's own list: 131 acceptance tests pass identically
  on Node, in Chrome, and on the Rust kernel core under wasmtime; both editors
  run; both foreign runtimes run; the identity architecture is recorded. Three
  consequences, structural: **(1) `poc/` freezes** — the JS supervisor is the
  reference implementation and conformance oracle, valuable precisely because
  it still runs; it changes no more (the guest world inside it is not frozen
  and graduates out as implementation milestone M0). **(2) The documents
  split** — [poc.md](poc.md) is the PoC's frozen record;
  [implementation.md](implementation.md) is the living build plan (milestones
  M0–M12: the tree, the scratch container, the namespace-file boot, the macOS
  app, host storage, the browser host on the Rust core, iPadOS on Pulley,
  `/net`, identity on the wire, the profile, the modern personality and git,
  the microVM, the world); this document remains design and decisions only.
  **(3) The tree rearranges around the real implementation** — `native/kernel`
  becomes `kernel/` (the one kernel), `native/host` becomes `hosts/macos/`
  (first of four hosts), and the Cargo workspace moves to the repository root.
  The conformance suite is the contract across all of it: the 131 are the
  permanent floor, and new features add self-skipping tests so one rootfs
  serves every host including the frozen oracle.

- **(2026-08-29) Capability doctrine, learned from the graveyard.** The
  fifty-year history of capability operating systems (RESEARCH §12:
  Plessey 250, CAL-TSS, HYDRA, System/38, the iAPX 432, KeyKOS, Amoeba,
  EROS, and the disguised survivors — Mach ports, seL4, Capsicum, signed
  URLs) yields five causes of death and this project's five answers,
  recorded as doctrine: **(1) the compatibility cliff killed more
  capability systems than everything else combined** — the WASI ABI and
  the benchmark discipline are the anti-Amoeba posture and are never
  compromised; **(2) capabilities stay invisible** — the capability IS the
  namespace and LOOKS like a filesystem, the handle is an fd, the grant is
  a bind, and no "capability" noun ever reaches a user (the System/38
  lesson: caps succeeded while invisible, and the AS/400 kept them only
  beneath the surface); **(3) devcap adopts Amoeba's mechanism** — the
  sparse, cryptographically checked, self-attenuating ticket (Amoeba's
  128-bit port/object/rights/check design, one-way-function protected, the
  ancestor of the signed URL) with modern MACs, over kernel capability
  tables; **(4) revocation is answered by expiry, re-attach, and unmount,
  never a revocation registry** — the AS/400 retreat proved held bits
  cannot be recalled, and modern practice (short-lived tokens) beat
  revocation lists; audits stay possible because authority is legible in
  the mount table, not scattered in held bits; **(5) the deployment story
  is reviewed periodically with the same honesty as the code** — Amoeba
  and Plan 9 died of their deployment wave (processor pools, CPU servers),
  not their kernels; ours (tab, laptop, container, agent sandbox) is
  today's wave, and the lesson is not "we are safe" but "re-examine."

- **(2026-08-29) The first formal design-thinking iteration — scope
  re-derived from personas, and the standing decisions survived it.** The
  method (d.school five modes through the Double Diamond, adapted and cited)
  and the full record are [design-thinking.md](design-thinking.md); the
  empathy artifacts are [personas.md](personas.md). Its epistemology, made
  standing: **P1 (the author) is live user research; P2–P5 are assumption
  personas whose insights stay hypotheses until their named validation
  events fire** — and design thinking is *evidence, not authority*: a
  persona-derived conflict with a dated decision arrives as a proposal with
  evidence attached, never a silent change. This iteration produced zero
  such conflicts — every standing refusal was independently re-derived from
  the personas — and five deltas, adopted: **the tour** (an rc script in the
  demo rootfs; chapters double as the educator's seed exercises), **the M9
  sandbox quickstart as a shipped artifact** (P4's belief test made
  deliverable), **M3+M4 as a pair** (P1's journey: a screen without
  persistence is a demo, not a home), and two explicit won'ts joining the
  refusals: **no phone form factor** (no persona's journey contains one) and
  **no Windows host** (until a persona demands it with evidence); courseware
  beyond the tour's chapters is likewise declined. Cadence: an iteration
  reruns with each deployment-ledger review and after any validation event.

- **(2026-08-31) The demo runs the Rust core — and the kernel loses its
  last OS dependency.** Her directive ("now continue with replacing the
  demo with the browser surface") executed the 2026-08-27 decision's
  second half: the one kernel crate now compiles native for macOS and
  to wasm for the page, and the page is the product. The load-bearing
  architecture note is what the landing forced: '#Z' hostfs was the
  kernel's only std::fs, and the browser cannot block on a filesystem —
  so every host-file operation became a delegated effect
  (Effect::Host{tag,op} out, hostop_done back), the exact webfs/fetch
  pattern already in the kernel. Consequences, all measured: the
  kernel is now literally free of OS calls ("no OS dependencies" was
  the design line; now it is a fact of the build); the macOS host
  gained the op server the kernel lost and stayed 151; the browser
  serves OPFS and picked directories over the same op protocol; a
  Node harness drives the identical wasm kernel headless (151, the
  fast iteration loop the browser lacks); and the guest world runs
  UNCHANGED — same worker.mjs, same guestcore, same mailboxes — the
  proof that the mach-layer seam sits exactly where the 2026-08-27
  decision drew it. Chrome runs the suite at 151 on the Rust core;
  cc-to-Hello-Kitty and reload-surviving browser storage verified by
  hand. The demo's JS kernel lineage retires to reference-in-tree;
  the frozen oracle remains the oracle.

- **(2026-08-31) Parity is measured against the running reference.**
  Christine: "I am comparing the acme from the demo vs acme from
  plan9port, and there seem to be differences" — and she separated the
  problems: parity first; the touchscreen/HIG redesign is PARKED for a
  full design-thinking session (mouse chords and discoverability
  belong there; nothing of it ships in this pass — the brief is
  docs/acme.md). Method, per the
  house rule that measurement beats memory: her plan9port acme was
  launched beside the demo, driven through its own /mnt/acme file
  interface (dirty, selection, tag states staged with 9p), and
  captured by CGWindowID — the screenshots are the spec. Adopted from
  the reference: the tag seeds (root `Newcol Kill Putall Dump Exit`,
  column `New Cut Paste Snarf Sort Zerox Delcol`, window
  `Del Snarf Get Look Edit |` — column Cut/Paste/Snarf/Zerox act on
  the last selection); Undo/Put appearing between the fixed words and
  the bar; the Medblue dirty box and white clean box on a Purpleblue
  column button (draw.h's own constants); black hairline separators
  and no gaps; the proportional Lucida face; acme's selection colours
  (#eeee9e bodies, #9eeeee tags); a static chunky caret; and the
  screen shape — columns fill the window, windows divide the column,
  bodies clip and scroll (the canvas spec grew column-prop for it).
  The pass also caught two real presenter bugs the comparison exposed:
  the window chrome's user-select:none had made native selection
  impossible inside every canvas view (sweeps, double-click,
  select-to-replace all dead — very likely the felt "difference"),
  and the caret repaint on mousedown destroyed the browser's selection
  anchor mid-gesture; both fixed (select on mouseup, selection owned
  by the view). Still divergent, stamped: the scrollbar sits right
  (its colours are acme's), no drag-layout between columns yet, look
  warps focus not the pointer, Look sits before the bar (the
  tag-scratch owns Edit's arguments), and tag wrap is the surface's.

- **(2026-08-30) The paper is the yardstick: acme answers its own
  literature.** Christine, after the succession: "I still feel like
  acme does not behave like real acme. Have you read the acme paper? Do
  you know if all the examples in the paper will work?" The honest
  answer was no — acme-today had the paper's shape and not its engine —
  so the paper (Pike, *Acme: A User Interface for Programmers*) was
  inventoried example by example and the gaps closed in one pass:
  **external commands** run in the window's directory (`mk` in a tag,
  the §19 workflow) with output to a `dir/+Errors` window created
  towards the right; the **selection filters `| < >`** (4th-edition
  exec.c:118 provenance) pipe dot through real commands; **Undo/Redo**
  unwind by sequence number, typing coalesced, exactly the paper's
  two-list algorithm; **Put appears in the tag only while dirty** (and
  Undo/Redo only when they have work), maintained as a tracked auto
  block that user tag edits shift; **B3 takes sam addresses** —
  `dat.h:27`, `:/re/`, `#c`, compounds — opening at the line, reusing
  an existing window, `<header>` resolving through /sys/include; **Cut,
  Snarf, Paste, Look, Sort, ID, Kill, Delete are words** (Cut/Paste
  compose with the host clipboard through /dev/snarf itself, so the
  stamped divergence and the paper's builtins reconcile in one buffer);
  and **the editor serves its file interface** — `index`, `new`,
  `N/{tag,body,addr,data,ctl,event}` — as 9P posted at `/srv/acme`,
  the paper's §7: `grep -n var *.c > /mnt/acme/new/body` works, `addr`
  speaks the same address language, `data` replaces the addressed
  range, and the `event` file delegates execute/look to a client in
  the paper's own format (`MI15 19 0 4 time`), writeback applying the
  default interpretation. The canvas protocol grew exactly two
  elements for all this — the `select` event and the `sel` attr, both
  device-transparent (neither kernel changed). What remains divergent
  is stamped in the matrix delivered with the pass: chords are the
  host clipboard's gestures (decided earlier), scrollbars and fonts
  are the surface's, Zerox copies rather than aliasing one buffer, the
  guide-file tools of §6 are superseded by the built-in Edit (as later
  acme did itself), and the Mac presenter still renders v0
  (deferral already on record). The suite grew one composite test
  driving every behaviour headlessly through the virtual surface and a
  real mount of /srv/acme — green on the Rust host and the demo
  kernel, self-skipped on the frozen oracle.

- **(2026-08-30) The name on the door: IPNX, not "modified Plan 9".**
  Christine, in her words: "We shouldn't call it a modified Plan 9 kernel.
  You yourself stated it does not inherit any code from Plan 9. We should
  be promoting IPNX, not Plan 9. IPNX isn't Plan 9 and will never be" —
  and, minutes later: "I feel like we are underselling what has been
  achieved." Both corrections are of fact, not of taste. The kernel was
  never a modification: it is an original implementation of Plan 9's
  *architecture*, written twice over — the Rust core and the JS
  supervisor, neither sharing a line with Plan 9's C — and proven one
  system by one conformance suite. Her README said it first ("The kernel
  is fresh… IPNX is not Plan 9, it is not UNIX"); this decision brings
  the working documents and the public page into line. The statement now
  opens with the IPNX kernel; Plan 9 is credited for the architecture
  and for the vendored heritage userland (which really is its code, and
  says so, notices intact); and the demo page leads with the achievement
  — a complete operating system, compilers and editors included, running
  client-side in a browser tab — rather than with the ancestor.

- **(2026-08-30) The input convention: roles in the tree, native grammar
  per platform, verbs never in the hardware.** The three-button replacement,
  articulated (the canvas decision's input addendum). The canvas declares
  SEMANTICS — spans with `action=look target=…`, `action=execute`, nodes
  with `menu=…` — and each presenter activates them with its platform's
  own grammar; apps receive verb events only (`look`, `execute`, `menu`,
  and edit verbs), never gestures (one scoped exception: `frame`/`path`
  leaves may opt into a raw pointer stream, Pointer-Events-shaped, for
  games). **Web bindings — web best practice literally**: look-spans are
  real `role=link`, execute-spans real `role=button`, so click/Tab/Enter/
  focus-rings/screen-readers arrive free because the things ARE native;
  context menus from `contextmenu`; selection verbs on the selection;
  ⌘-click a look target = look in a new window (the browsers' own grammar,
  inherited). **Apple bindings — HIG literally**: SwiftUI `Button`/`Link`/
  `.contextMenu` (touch-and-hold with preview on iPad; right-click/⌃-click
  on Mac); selection verbs in the NATIVE edit menu beside Cut/Copy/Paste;
  `.keyboardShortcut` ⌘-conventions; and HIG's own rule honoured — no
  action lives only in a context menu, so the Mac presenter surfaces menus
  in the real menu bar. **Hardware mouse, modern not nostalgic**: left
  activates, right menus, middle looks-in-new-window; the Plan 9 b2/b3
  semantics survive only in the raster exhibit's input synthesis —
  "buttons as accelerators" is REFUSED as convention pollution.
  **Keyboard**: Tab traverses, Enter/Space activate, ⌘Enter executes the
  selection or word at caret (acme-today's tempo), hold-⌘ shows the iPad
  shortcuts HUD. **The laws, lifted not composed**: keyboard-complete
  (WCAG 2.1.1); hover decorates, never gates; activation on release with
  slide-off cancel (2.5.2); no gesture-only functions (2.5.1); target
  sizes max(44pt HIG, 24px WCAG); destructive actions confirm. And the
  structural claim: accessibility is enforced BY CONSTRUCTION — apps never
  render controls, presenters always render them natively, so an
  inaccessible button cannot ship. Refused: custom gestures in v0; the
  nostalgic button mapping. The verbs in one line: b1 was always
  universal; look = tapping the thing; execute = tapping the tag or
  ⌘Enter; the chords were always the clipboard.

- **(2026-08-30) The succession rule: a name is inherited by passing
  the ancestor's tests.** Christine's directive — "get sam-today
  working, then we can retire the heritage items" — executed as a
  rule worth keeping: sam-today took `/bin/sam` only by passing the
  three existing sam tests UNCHANGED (structural regexps, the `g//`
  guard on class ranges, and `,| tr` — the buffer piped through a
  real command, fork machinery and all); the raster original stepped
  back to `sam9`, still built, still driven by samtest (the whole
  sam/samterm/libframe stack stays proven), still holding its share
  of the floor. The demo menu's heritage wing retired the same hour —
  the exhibit remains runnable by name (`sam9`, `acme`), present in
  the tree and the suite, absent from the product face. Retirement
  means the successor answers to the ancestor's name and the
  ancestor's proof; it never means deletion. **The second clause,
  from acme's succession the same hour**: an ancestor's tests divide
  into BEHAVIOURAL (file in, commands, file out — sam's, which the
  heir passed verbatim) and SUBSTRATE (raster pixels, the draw
  protocol — acmetest's, which certify the stack, not the editor).
  A name passes on the behavioural tests; the substrate tests step
  back with the ancestor (`acme9`, still driven by acmetest, the
  sam/samterm/libframe and acme raster stacks staying proven). So
  `acme` now means acme-today, held to the workspace behaviour suite
  — columns, editable tags everywhere, word execution, Put-by-name,
  the look browser, the Edit language — and both originals answer to
  their stepped-back names.

- **(2026-08-30) No half-working.** Christine, on acme-today's deferral
  list, in her words: "I don't understand why we have deferrals. Either
  acme works like acme, or it doesn't. We can't have half working." The
  standard, recorded: slices may BUILD in sequence, but what ships to a
  user carries whole behaviour — a deferral list is a build-planning
  tool, never a licence for a shipped half-experience. Applied the same
  hour: acme-today gained caret editing everywhere (click positions,
  arrows navigate, selection replaces, paste inserts), sweep-execution
  with arguments, Look (in-window literal search presenter-side; paths
  open windows app-side), the dirty box that Put clears, Zerox, Dump
  and Load — leaving exactly one deliberate divergence, already
  stamped: the native clipboard IS snarf (the input convention's
  "chords die into the host clipboard"), so Cut/Snarf/Paste live in
  the platform's own gestures rather than as tag words.

- **(2026-08-30) The design stretch: a distributed operating system.**
  Christine's articulation, recorded in her words: "the design stretch
  for IPNX is a distributed operating system. There is no reason why the
  process orchestrator in IPNX can't orchestrate processes on other
  systems, so we can truly replace kubernetes if we wanted to. We need
  to be able to support process migration and rehosting, discovery of
  other systems and capabilities, and a user identity that can span
  systems." The framing that makes it honest: this is a **return, not a
  departure** — Plan 9 was a distributed operating system first (cpu
  servers, file servers, terminals, import and export), M0–M6 built the
  single-kernel half, and the stretch names what completes the circle.
  The three capabilities, mapped to mechanisms already in the tree:
  **Rehosting** is the orchestrator's normal move once specs exist — a
  process file is declarative, so "run it there instead" is kill-here,
  run-there with the namespace re-applied on arrival; wasm makes the
  binary host-neutral by construction (the same image runs under V8,
  JavaScriptCore and wasmtime today — rehosting across ARCHITECTURES is
  already routine in this repository). **Migration** — the live form —
  is uniquely plausible here because *every bare fork already serialises
  a whole process*: the asyncify unwind snapshots memory and stack, and
  a fresh Worker rewinds it; migration is that snapshot shipped to
  another kernel instead of a sibling Worker. The honest edge, named
  where classic migration died: open fds do not travel — wire mounts
  must re-dial and pipes must proxy or drain; the declarative namespace
  makes the FILE half re-bindable, and the fd half is the engineering.
  **Discovery** stays files, per the founding principle: Plan 9's cs(8)
  and ndb are the precedent — a neighbourhood is a mounted directory,
  a system's capabilities are read from its files (/svc, caps, /proc),
  and "what can you do" is `ls`. **Spanning identity** is M8+M9 seen
  whole: tickets authenticate the wire, the factotum-shaped agent holds
  the keys, and the profile is the portable person — one identity
  booting constrained instances on any kernel, "the kernel instance is
  the new uid" extended to a person who owns many. The kubernetes
  clause keeps its recorded honesty: the orchestration entry's
  consensus debt (a union of /svc trees is not a quorum) stands — the
  stretch does not waive it. Sequenced, not new milestones: M7 carries
  the wire, M8 the identity, M9 the person, M12's cluster stage the
  orchestration; the stretch names their sum and aims them.

- **(2026-08-30) The console amendment: AND, not XOR.** Christine, mid-
  build, in her words: "we should allow xterm.js as well - it is what
  people are used to... and not xor." The retirement clause softens on
  the record: console-today (the editable transcript, `con(1)`) is the
  NATIVE design and the doctrine's direction, and the xterm byte console
  stays beside it as the familiar door — the same reasoning that made
  VSCode a surface (meet people where they live) applied to our own
  terminal, and the exhibit philosophy applied to the present: nothing
  is deleted, surfaces multiply. Programs that write bytes get either
  console; programs that want structure get canvas.

- **(2026-08-30) The ultimate dev environment, and VSCode as a surface.**
  Christine's realisation, recorded in her words: "we have created the
  ultimate dev environment. A system can build and run a cluster - in a
  browser, without relying on VMs or containers... VSCode needs to be a
  surface." The first half is a shipped fact, not an aspiration: the
  live demo tab holds five toolchains, pkg, and — since the local stage
  landed — a supervised replica set; the cluster-in-a-tab runs today.
  The consideration, worked through: **VSCode is two surfaces, arriving
  at two times.** *The namespace surface, buildable now*: VSCode's
  FileSystemProvider API is nearly 9P — stat/readDirectory/readFile/
  writeFile/delete map to Tstat/dirread/Topen+Tread/Twrite/Tremove, and
  rename is `wstat` carrying a name, exactly the V10 shape we already
  landed. And the transport is *nothing*: the extension host is Node,
  and `demo/supervisor/kernel.mjs` is platform-neutral — the kernel
  boots inside the extension process, FileSystemProvider methods call
  straight into kernel walks (kernel-as-a-library, the JS twin's third
  host). Terminals are the Pseudoterminal API wired to `/dev/cons`;
  tasks run process files; the debugger's substrate is `/proc`. The
  doctrinal kicker: **devcontainer.json is a process file, badly** —
  Remote-Containers is a large extension plus Docker because the OS
  beneath had no per-process namespace; Remote-IPNX needs neither.
  *The canvas surface, after M5*: webview panels presenting canvas
  trees — acme-today can live in a VSCode tab. No conflict with the
  one-editor doctrine: VSCode is a surface a developer already
  inhabits, not our editor; IPNX mounts into their world, and surfaces
  multiply while the protocol stays one. Honest engineering questions,
  named: 9P has no change-notification (FileSystemProvider.watch needs
  polling or a synthetic event file — decide when building);
  vscode.dev runs extensions in a web worker (nested-worker and SAB
  constraints to measure, WebKit's especially). Sequenced as its own
  small milestone (M13), interleavable like the local stage was.

- **(2026-08-30) The iPad surface, re-aimed: an app that launches
  WebKit over local files.** Christine's call, in her words: "the ipad
  surface has changed. it is simply an app that launches webkit,
  connecting to local files. It circumvents JIT restrictions." The
  stopgap becomes the design. WKWebView's content process carries
  Apple's own JIT entitlement — third-party apps cannot JIT in-process
  but may host WKWebView, so the browser port runs at full JavaScriptCore
  speed inside the app, sanctioned. The shell shrinks to a few hundred
  lines of Swift: a WKURLSchemeHandler serving the bundled dist with
  **real COOP/COEP headers on every response** — no service worker, no
  register race, and plausibly no WebKit module-worker serialisation
  defect, since that measured failure was specific to loads through a
  service worker (a measurement to retake in-app); local files mean
  first boot is offline, the 260MB stream gone; and the app bridges
  real files inward — a security-scoped bookmark IS a bind
  (platforms.md), served to the kernel's hostfs over the script-message
  channel. Pulley demotes from the M6 plan to recorded fallback
  research (the engine-matrix decision stands if store policy or
  WKWebView limits ever bite; App Store honesty: this is a full local
  system that works offline, not a remote-site wrapper). The
  precision, hers in the same breath: "the ipad app runs the kernel
  and binaries inside webkit, but **/dev/canvas connects to swiftui**"
  — and "we can give webkit entitlement to access the local ipad
  filesystem." So the webview is an **engine room, not a display**:
  compute (kernel + wasm guests) runs in WKWebView for the JIT; the
  canvas tree crosses the script-message bridge and the presenter is
  native SwiftUI (stack→layout containers, text→Text, edit→the native
  editor, path→Path, events flowing back per the verb convention) —
  exactly the canvas doctrine's split, and cheap on the wire because
  semantic trees are small where raster frames were the 640GB lesson;
  and the app's entitlements serve the local filesystem inward to
  hostfs over the same bridge. The unification, stated once: **the
  browser port is the universal embedding, and every surface is a
  shell that lends it three things — a place to run, a screen to draw
  on, files to touch.** VSCode lends Node, its own panes, and a file
  API; the iPad app lends WebKit's JIT, SwiftUI, and its file
  entitlements. The ecosystem statement, cashed out.

- **(2026-08-30) The compensation thesis: complexity grows where a
  primitive is missing.** Christine's capstone over the dissolution
  series, recorded in her words: "the real benefit of this is that we
  are avoiding all the mistakes of linux, systemd, docker and
  kubernetes. These are overly complex systems because they did not
  have a per process namespace. We are not only living in the modern
  ecosystem, we are simplifying and replacing it." The causal argument,
  made precise so the claim can defend itself: **when a kernel lacks a
  primitive, userspace grows an industry** — and every such industry
  ships its own config dialect, its own daemon, and its own privilege
  model. Linux kept the global filesystem view; namespaces arrived
  piecemeal as `CLONE_` flags (2002–2013), root-only for a decade,
  disjoint from the file model — so Docker exists to *assemble* them
  (a privileged daemon, image formats to cache mutation, overlay
  filesystems to fake composition); systemd's unit files grew dozens of
  sandboxing directives (`PrivateTmp`, `ProtectHome`,
  `RootDirectory`…), each a hand-cut slice of what one `bind` verb
  gives uniformly, and imperative boot ordering that a declarative
  namespace file does not need (M2); Kubernetes then re-glues what the
  layer below fractured — pods to group processes namespaces would have
  grouped, CNI to give pods what per-process `/net` gives, service
  meshes to interpose what a 9P proxy does at the file layer,
  ConfigMaps to inject what a bind injects. The venv/flatpak entry
  (2026-08-29) was this same theorem's first instance; Docker and
  Kubernetes are its industrial form. The scope of "replacing,"
  reconciled with the ecosystem statement ("the web platform is our
  VAX"): we adopt the *surfaces* — browser, wasm, serverless — and
  simplify away the *middle* of the stack, the distro-systemd-docker-
  kubernetes plumbing between hardware and surface. Two honesty
  clauses, kept beside the claim: not all of that complexity is
  compensation — metering (cgroups' quota half, our standing open
  question), consensus (etcd exists because a cluster must *agree*;
  `bind -a` over `/svc` trees is a union, not a quorum, and the
  cluster stage owes this its real engineering answer), and hardware's
  own mess are essential complexity we inherit like everyone else. And
  the practitioners' temptation, named per virtue-ethics.md: those
  systems' bulk also encodes operational scar tissue earned under load
  we have not yet borne — we avoid their *structural* mistake, the
  missing primitive; our own scars are still ahead.

- **(2026-08-30) Containerisation and orchestration, planned: a Dockerfile
  is a process file, the orchestrator is a file server, kubectl is `cat`
  and `echo`.** Christine's directive, recorded in her words: "a
  Dockerfile simply sets up what packages need to be installed for a
  process and what commands needs to be executed - it is effectively a
  process instantiation specification. we can implement an entire process
  orchestration suite (in userspace). Effectively, we can do everything
  kubernetes can do, within the constraints of our architecture." The
  design, worked out from that observation:

  **The spec is a directory — the process file.** M2 already made boot a
  namespace file, and boot is just the instantiation of pid 1; the
  general case is a spec directory: `namespace` (M2's dialect verbatim),
  `packages` (name/version pairs — pkg verifies digests and refuses
  conflicts), `user` (an identity.md transition), `env`, `cmd`, and
  optionally `replicas` and `health` (a path to read — a liveness probe
  is a file read). The structural claim under it: **a Dockerfile is a
  script because installing is mutation; a process file is a declaration
  because installing is a bind.** `RUN` steps and image baking exist to
  cache filesystem mutation — with namespace assembly instant and
  package trees immutable under `/pkg`, there is no build step to cache.
  And the symmetry that makes it honest: a spec directory stands to a
  live process as `/proc/<pid>` stands to it at runtime — instantiation
  is introspection's mirror.

  **`run(1)` is docker run** — ~a hundred lines of userspace: rfork,
  install the declared packages, `newns()` the namespace file, set env,
  transition the credential through `/proc/<pid>/ctl` (the eve/ruid rule
  unchanged — no new mechanism), exec. Runs on today's kernel; nothing
  is missing after M4.

  **`svc(4)` is the control plane, as a file server.** Desired state is
  files you write, observation is files you read: `/svc/ctl` takes
  `start name spec` / `stop` / `scale name n`; `/svc/<name>/` holds
  `spec`, `replicas`, `pids`, `status`, `log`. A reconciler keeps
  desired and live equal with backoff — a Deployment is a directory
  entry, and kubectl is `cat` and `echo`. **A Service is a `/srv`
  post**: svc posts one name serving a 9P proxy that fans attaches
  across replicas — a load balancer is a 9P multiplexer, and consumers
  just `mount` it, oblivious. Rollout composes with what already
  exists: blue-green is repointing the post; rollback is binding the
  previous `/pkg` versions — or a `#V` snapshot.

  **The cluster stage** consumes M7 (`/net`) and M8 (identity on the
  wire), with `cpu(1)` as the precedent: a spec instantiated on a
  remote kernel that imports the caller's namespace pieces back over
  9P; a cluster's control plane is a union — `bind -a` each kernel's
  `/svc` into one tree, and the scheduler is any userspace program
  choosing which `ctl` to write. Policy lives in userspace; the
  mechanism is file writes.

  **What does not dissolve, kept on the record**: metering and quotas
  (the standing open question from the third-dissolution entry);
  bin-packing and affinity — genuinely policy, admissible as userspace
  programs, never core; inter-kernel networking, which belongs to the
  hosts. And the tripwire travels: if the spec dialect grows past its
  few files, we have rebuilt YAML Kubernetes and must stop. One
  pleasing inversion closes it: M1 ships the kernel *inside* a
  `FROM scratch` container; this suite makes processes the contained
  unit — and the two compose, a fleet manager running IPNX kernels,
  each orchestrating thousands of processes, pods of pods with none of
  the machinery. Sequenced at M12, re-aimed accordingly: the local
  stage (spec, `run`, `svc` on one kernel) is pure userspace and may
  interleave early; the cluster stage waits on M7+M8.

- **(2026-08-30) The namespace's third dissolution: every process is a
  jail, a container and a microVM — "our computer is a network."**
  Christine's articulation, recorded in her words: "per process namespace
  solves not only package dependency management and backups, it also
  significantly lessens the need for jails, containers, even kubernetes.
  every process is a jail, and a container, and a microvm. processes are
  isolated from each other, and it is possible to run a whole cluster of
  processes with different roles, network interfaces, sockets etc. side
  by side… Our computer is a network." (Sun's exact motto, inverted:
  "The network is the computer" — John Gage.) The argument, built out:
  isolation here is not a product but what a process *is*, and it is
  **double-walled by construction** — the namespace bounds what a process
  can *name* (its visible universe is its mount table, nothing else
  reachable), and the wasm instance bounds what it can *do* (no ambient
  syscalls, no ambient memory; only the mailbox). And because **9P is the
  only IPC**, every process boundary is already a wire boundary: two
  processes on one kernel differ from two machines on a network only in
  latency — `exportfs` demonstrates the equivalence, serving one
  process's world to another across any wire. That is what makes the
  inversion literal rather than rhetorical: one computer decomposes into
  a network of small machines, each with its own filesystem view, its
  own posted services (`/srv`), its own credentials (identity.md's
  roles), and — when M7 lands `/net` — its own network interfaces and
  socket space, Plan 9's own trick (bind a different `/net` and you are
  on a different network). Against each tool, honestly: chroot and the
  jail are one-shot, root-only namespaces without composition — ours is
  the general case of what they special-case. The container's isolation
  half dissolves (the image is a pkg subtree; isolating is just process
  creation), but its *metering* half — cgroups' cpu/memory quotas — does
  not, and stays a named open question (whether per-process quotas
  belong in this kernel, or whether the host OS metering the one kernel
  process suffices per deployment form). Kubernetes' service discovery
  is `/srv` plus binds; its scheduling and replication remain genuinely
  orchestration — "lessens," her verb, kept deliberately over
  "replaces." Status and standing test: the file half runs today
  (`rfork n`, private binds, pkg, `#V`); the network half is M7's
  mechanism, and this entry is measured the day two processes hold
  different `/net` binds side by side.

- **(2026-08-30) The versioning layer, v1: a snapshot is a tree, restore
  is a bind — landed as `#V`.** The immutable-systems doctrine
  (2026-08-29) gets its first mechanism. `echo snap t1 > '#V/ctl'`
  freezes the ram root by **structural clone**: nodes copied shallow,
  every byte buffer shared, the live side copying a buffer only on its
  next write to it (`Rc::make_mut` in the Rust core; a `dshared` mark in
  the demo kernel — the frozen oracle self-skips). Measured: twenty
  whole-root snapshots of the 710-node rootfs cost 8.9 MB and under a
  second, against ~50 MB of file data a copying design would have
  duplicated per snapshot (RESEARCH §9.8). The interface is three files
  and a verb pair — `ctl` takes `snap [name]` and `del name`, `#V/<name>`
  walks the past read-only, and **rollback is `bind '#V/t1/dir' /dir`**;
  a whole-system rollback is the same line first in `/lib/namespace`,
  which makes booting from a snapshot pure M2 machinery. Enforcement is
  one gate: `ram_access` refuses writes on snapshot nodes *before* the
  eve bypass — nobody rewrites history, eve included. Honest scope:
  these are epoch snapshots (taken when asked), kernel-resident, gone at
  shutdown. The doctrine's asymptote — *every write* an incremental
  version — and persistence across boots are M4's remaining storage
  question (a content-addressed store under hostfs); the interface is
  designed so both slot in behind `#V` unchanged. Alongside it, `ar(1)`
  landed on a measurement (wasm-ld links index-less archives, RESEARCH
  §9.8), closing the static-library gap: `ar r libx.a x.o` then
  `cc main.c -lx` now works end to end.

- **(2026-08-30) Compatibility, kissed goodbye — the userland is
  reimagined, and the verbatim world becomes the exhibit.** Christine's
  call, in her words: "Let's redesign sam, acme and the rest of the Plan 9
  utilities to use our new paradigms. It's time to kiss compatibility
  goodbye." This SUPERSEDES half of the re-founded userspace objective
  (2026-08-27): "the real Plan 9 userspace entire — the designers'
  curation of Unix" ends as a product goal. What replaces it: the
  CURATION survives, the VERBATIM does not — the essences are carried
  forward into native designs (docs/userland.md) and the vendored raster
  world reclassifies wholesale as heritage exhibit beside /v10 (still
  building, still running, still holding the suite's floor — the exhibit
  is load-bearing for conformance, never for design). The three classes:
  filters (cat, grep, sed, sort…) were ALWAYS native — their paradigm is
  files and pipes, which is our paradigm; nothing to redesign. Screen
  programs redesign onto canvas: ONE editor — acme-today absorbs sam
  wholesale (acme always contained sam's Edit language; two editors
  become one editor plus a language, with sam surviving as the language
  spec and a batch CLI); the console becomes an editable transcript (an
  edit node with a prompt discipline — acme's win(1) was the prototype;
  tty emulation retires); rio-today is the window-policy file server over
  host presenters; the plumber returns to the centre as the look verb's
  engine. Full designs and sequencing: docs/userland.md; M5 carries the
  build order.

- **(2026-08-30) `/dev/canvas` — the display is a semantic file tree; the
  modern-draw question, decided.** Settled in a five-round brainstorm
  (Christine's adversarial pushes each carved a clause; the full iteration
  record is in design-thinking.md). The decision, whole:
  **The model.** A window is a filesystem of semantic nodes — acme's file
  interface generalised, not libdraw modernised. Six node kinds in v0:
  `stack · text · edit · image · path · frame`. Content is greppable plain
  UTF-8 (styling in sidecar `attrs`; a live UI can be grepped, tested, and
  read by an agent without any engine). `edit` lifts acme/sam's addr/data
  buffer interface — the crown jewel; typing echo, selection, scrolling and
  IME are presenter-local, apps observe via events (acme's own discipline).
  `path` carries SVG path data verbatim; `frame` is pixels-in-a-grid,
  honestly named and honestly opaque (video is a frame updated at cadence).
  `draw` the name retires with its era. One `events` file per window
  (resize · close · tap · execute · look · key) — resize/close as protocol
  is the target that retires the demo's deferral. `sync` commits atomically.
  **Surfaces.** A surface is anything that renders the tree, and it
  NEGOTIATES capabilities (interactive/reflow/input/snapshot): browser, Mac
  and iPad presenters; SVG, PDF, PostScript as write-only document surfaces
  (layout resolution is a surface property — "print" is attaching a
  document surface); virtual surfaces — a test asserts on the tree (pixel
  censuses retire), the accessibility reader IS a render; remote surfaces
  over 9P (exportfs a window = a semantic remote UI); multiple surfaces on
  one canvas = mirroring for free. To the browser, the surface is ONE
  universal SPA, ours and cached, consuming tree-files and emitting DOM.
  The scope of the refusal is the PROTOCOL, not the apps' knowledge
  (corrected 2026-08-30): an ipnx app may of course understand the web —
  fetch HTML over '#H', parse it, generate it, even be a browser — but
  markup never crosses /dev/canvas; the display protocol speaks the six
  kinds regardless of what the app knows.
  **The four clauses from the pushback rounds.** (1) *SVG*: for the marks
  corner we ARE describing SVG and adopt it outright (path data, transform
  and colour notation); the whole is not SVG — no flowed text, no editing,
  no protocol (SVG 1.2's flowed text died; the web itself needed HTML+SVG).
  Rule: adopt notation, own the model. (2) *HTML*: the semantic retained
  tree is the web's discovery and we adopt it — re-housed: the tree as
  files not a JS-API, events as files, no behaviour inside the surface,
  and a vocabulary small enough that a phone, a PDF and a test are peer
  surfaces (an HTML-subset model would make every surface a browser).
  (3) *Borrowed engines*: the hard four-fifths of a visual surface is the
  era's text engine, and we borrow the local one — WebKit on the web,
  CoreText/TextKit on Apple — as RENDERERS, never runtimes; the canvas
  protocol is the narrow waist above them, exactly as 9P is the waist above
  V8/wasmtime (NeXT licensed Adobe's interpreter: "display is PostScript"
  was always own-the-protocol, borrow-the-engine). Layout divergence
  between engines is accepted and named; document surfaces choose their
  authoritative resolver. (4) *The ecosystem statement*: **the web platform
  is our VAX** — the modern stack (browser, WebKit, wasm, serverless) is
  the hardware of the era; we port to hardware and do not adopt its
  operating system, because we are the operating system. The invariant that
  is IPNX: adopt substrates, engines and notations; refuse object models
  (wasm yes, WIT no; sockets yes, POSIX no; V8 yes, JS-as-model no).
  **Input: the verbs leave the buttons.** The three-button mouse encoded
  verbs — b1 point/select, b2 execute, b3 look — and the verbs become
  system-level: tap plumbs (look is the free gesture), tags are genuinely
  tappable, selections raise the native popover (Execute · Look · Cut ·
  Copy · Paste), ⌘Enter executes at the keyboard; chords die into the host
  clipboard via /dev/snarf. Real button hardware maps straight onto the
  verbs — the paradigm is discarded as a requirement, kept as an
  accelerator. A compat layer may synthesize /dev/mouse for raster-era
  clients.
  **The reclassification.** Raster sam/acme stay runnable as they run
  today, reclassified as heritage exhibit beside /v10 — not a constraint on
  canvas. sam-today is its command language over the server's edit buffers
  (libframe evaporates); acme-today is a policy client of stacks, edit
  nodes and actionable tags, still serving its own 9P interface. Their
  essences — structural regexps; everything-is-text-and-text-is-actionable
  — intensify rather than survive.
  **Commitments.** Canvas surfaces embeddable in web pages; a first-class
  JS/TS client library; the six kinds learnable in an afternoon; a
  quarantined `web` leaf stays a personality-shaped future door (not v0).
  **The tripwire.** v0's vocabulary is measured against four benchmarks —
  sam-today, acme-today, rio-today, one plot. If it ever grows past
  roughly a dozen kinds, the HTML refusal is declared wrong and adoption
  of a real subset is re-litigated. Open: whether `image` folds into
  `frame`; edit's addr/data taken verbatim vs simplified.

- **(2026-08-29) Immutable systems and time travel are namespace operations.**
  Christine, in her words: "If files are tagged with version numbers, and
  every write results in an incremental version, then backup is simply a
  snapshot of a namespace at a given time, and can be restored simply by
  binding the correct versions. We can truly roll back to any point of time
  in the past." This is Plan 9's dump filesystem and Venti given the
  general form: there, the snapshot was the file server's nightly gift and
  yesterday(1) bound you into it; here it falls out of the same two
  primitives everything else uses — versioned files are just files, a
  snapshot is a recorded set of binds (a namespace fragment — M2's format
  again), and restore/rollback is applying it. Immutability is the same
  fact seen forward: a process bound to fixed versions cannot be changed
  underneath. Sequenced, not built: this shapes M4's storage design
  question (a content-addressed or versioning layer under hostfs) and the
  profile milestone (a profile snapshot IS a backup of an identity); the
  fragment format is already in the tree.

- **(2026-08-29) Dependency hell and package conflicts dissolve in the
  namespace.** Christine's articulation, recorded in her words: "Since every
  process has its own namespace, and the package manager simply binds files,
  we don't have to do dependency management — every package installs exactly
  the files it needs. We can also resolve package conflicts: if a package
  binds a different file to the same name, we can reject the install." The
  two classic package-manager problems are artifacts of a GLOBAL filesystem,
  and this system does not have one: (1) versions coexist by construction —
  /pkg/<name>/<version> keeps every version's subtree, and a namespace binds
  the one it wants, so "A needs libfoo 1, B needs libfoo 2" is two binds in
  two worlds, not a solver problem. The OS layer therefore carries NO
  dependency machinery (language ecosystems keep their own graphs — pip
  still walks Requires-Dist — but that is the personality's business, not
  the package layer's). (2) A conflict is well-defined and CHECKABLE: a name
  about to be bound that already resolves to DIFFERENT bytes is a genuine
  collision, and pkg refuses the install (same bytes = idempotent, allowed).
  Union order already makes deliberate shadowing expressible (bind -b);
  refusal guards only the accidental case. And the third consequence, hers
  in the same breath: "it also means we can have multiple dev environments
  coexisting as different processes — each process can manage its own
  package versions." What venv, nvm, rbenv and every toolchain manager
  simulate with PATH shims and activation scripts, the namespace gives
  natively: `rfork n` (or newns with a fragment) and install — the
  environment IS the process's namespace, it nests, it composes with
  everything else namespaces do, and it vanishes with the process. All
  three consequences implemented and pinned in pkg(1) and the suite the
  same day.

- **(2026-08-29) The package model — a package is a subtree, installing is
  binding.** Investigated at Christine's direction before M1 ("genericise
  this personality to allow us to import from external registries"), with
  the survey and the proof in RESEARCH (WLR's ruby-3.2.2.wasm, fetched,
  digest-verified, ran on the unmodified WASI personality — the import
  machinery is already generic for well-behaved preview1 commands). The
  design, in six commitments:
  1. **No package database.** A package is a file tree under
     `/pkg/<name>/<version>/`; installing binds it (`bind -a …/bin /bin` —
     the union directory is the merge mechanism); uninstalling unbinds. The
     namespace is the installation record, which makes installs per-process
     by construction and per-identity via the profile: "installed software"
     persists as a profile namespace fragment (identity.md), so a person, a
     role and an agent each carry their own package set. An agent can have
     packages its person does not.
  2. **The personality declaration rides the package**: a `meta` file in the
     namespace-fragment dialect (bind lines, plus small extensions: `abi
     wasip1|wasi_unstable|plan9`, `env K=V`). The three provisioning layers
     map onto package content — the ABI names the shim; `libs/` packages
     land as layer-2 sysroot trees (`/lib/wasm32-wasi`, `/include`) that
     `cc -l` links; runtime-support trees and env are layer 3.
  3. **`pkg(1)` is the v1**: `pkg install <registry>/<name>[@ver]`, `list`,
     `remove`, `verify`. Fetch over `#H` (later `/net`); **sha256 verified
     always** (every surveyed registry publishes digests; refusal on
     mismatch, as pip already does); unpack; bind per `meta`. pip remains
     the Python ecosystem's arm; pkg handles wasm commands and sysroots.
  4. **Registries as filesystems is the end-state**: a registryfs (a
     userspace 9P server per registry, the hellofs/exportfs lineage)
     translating walks into API calls — then installation decays into
     `cp -r /n/wlr/ruby/latest /pkg/… ` plus a bind, and the package
     manager nearly disappears into the file model. Where the wasm world
     links components, ipnx mounts servers; WIT/warg stay refused per the
     founding decision.
  5. **The demo ships a curated same-origin mirror** — GitHub asset
     downloads send no CORS (measured), so the browser host cannot fetch
     them directly; a small `/registry/` on the demo site (verified
     runtimes plus digests, each under the 50MB line) makes `pkg install
     ruby` work in the tab, makes the demo the first ipnx registry, and is
     the curation principle applied to acquisition. Native hosts reach the
     real registries unconstrained.
  6. **Trust is digests now, identity later**: v1 pins sha256; signature
     schemes (warg's logs, sigstore) wait until the identity milestone
     gives them an anchor. The capability doctrine already does the heavy
     lifting — an installed binary gets nothing but the namespace it is
     started with, so trying untrusted software is safer here by
     construction than on any system where install grants ambient
     authority.
  Open, deliberately: the command's name (`pkg` is the placeholder); the
  wasix personality (plausible — ipnx has real fork/exec — but unplanned);
  OCI artifacts as a registry (aligns with M1, native-first).

- **(2026-08-29) The toolchains are real — the time for shims is over.**
  Christine's directive: "We need the toolchains to work, not demo shims…
  this is for real. We need the ability to install packages, etc. so pip
  needs to work as well. All this is what a user would expect from a demo."
  Acceptance is external and measured: Python runs the example programs from
  python.org; Go runs the examples from gobyexample.com (verified 2026-08-29:
  the python.org front-page programs verbatim, and seventeen gobyexample
  features — closures, generics, interfaces, goroutines, select, atomics,
  regexp, text/template, base64, net/url — compiled in-guest and run).
  What landed: the FULL 529-file CPython stdlib (the wasi build's own Lib) in
  place of the PoC's 21-file subset; a pure-Python `zlib` (a puff-shaped
  inflate; this CPython build has no zlib builtin — measured) as a
  personality file; `pip` installing real pure-Python wheels from the real
  PyPI — sha256-verified, RECORD-listed, `python -m pip` the same program —
  over `#H`, a webfs-shaped kernel device (the network as files; the /net
  milestone's forerunner); **the real gc compiler and linker cross-built to
  wasip1 and run as guest processes**, orchestrated by a real `go` driver
  (build/run/fmt/version/env) exactly as cc(1) drives clang, with the
  stdlib's export archives for the gobyexample-derived package set (115
  packages, 35.3MB; net/http refused at +31MB until sockets exist); and
  cc(1) grown to -E/-S/-lm. Upstream pip itself still cannot run (it demands
  C-level ssl+socket, absent from this CPython build) — pip here is ipnx's
  own installer speaking the real protocol to the real index; when /net
  lands, upstream pip becomes the target.

- **(2026-08-29) The porting inversion — personalities carry the
  environment, not patches on the source.** Articulated by Christine after
  the demo's C toolchain landed. The universal way to port software is to
  change the *source* until it matches the target's environment (its libc,
  its headers, its syscalls). **IPNX inverts this: build a port *personality*
  — a libc.a, its headers, and the runtime environment — so that the
  *unmodified* source compiles and runs.** Personalities are cheap here (they
  are libc dialects over the one kernel — the founding decision), so IPNX can
  afford a whole shelf of them: a GNU/glibc personality, a musl personality, a
  Linux personality, a BSD personality — each an environment against which
  real-world packages build with no edits. This is strictly more useful than
  any single Linux or BSD distribution, which binds you to one libc and one
  ABI: IPNX presents whichever environment the source expects, per process,
  by namespace. The demo already contains the first instance — a wasi-libc /
  POSIX personality (the wasi sysroot the in-tab `cc` compiles against): stock
  C compiles unmodified because the *environment* was supplied, not because
  the source was bent. Relationship to the measured modern personality
  (2026-08-27): the two compose and do not conflict. **`libunix` is the
  *native* modern-Unix personality, derived by measurement** (the 20% of
  POSIX that git/CPython/Go actually call) — the surface IPNX offers as *its
  own* Unix. **The port personalities are fuller environments whose telos is
  compiling foreign source unmodified** — measured by "does the package
  build," not by taste. A program picks its personality the way it picks its
  libc; the kernel underneath is unchanged. This is the deep reason
  compilation-as-a-capability (`/cc`) and the toolchain work matter beyond the
  demo: each port personality plus a compiler is a machine for absorbing the
  existing software world without forking it.

- **(2026-08-29) The demo is the product.** Christine's formulation, verbatim
  doctrine: *"It's like a game demo. A lousy game demo means no one will buy
  the game."* The public demo is every visitor's one and only encounter with
  the system — for them it IS the implementation, and it has to work BETTER
  than what ships later, because a visitor who sees faults leaves and never
  returns. Consequences: **(1)** demo quality items (genuine guest window
  resize, clean close, first-five-minutes polish) are product work, never
  deferred as "waiting for the real implementation"; **(2)** the demo stops
  being confined to page-level derivations of the frozen kernel — `poc/`
  remains the untouchable conformance oracle, but the demo ships its own
  supervisor lineage (forked from the reference) where visitor-facing kernel
  needs land first: window-refresh events for true guest resize, close
  semantics, and whatever the experience demands; the fork is judged by the
  same suite plus its own additions. **(3)** The deployment ledger's review
  treats demo-quality regressions as shipped-form failures, not cosmetics.

- **(2026-08-29) The six-hats pass — the blind spots, caught and adopted.**
  De Bono's parallel-thinking sweep run over the whole project at the
  declaration; the record is [six-hats.md](six-hats.md). The catches, each
  now in the plan: **CI on push via the M1 container** (the floor stops
  being a manual discipline) and **the oracle in amber** (a pinned-Node
  image, because frozen means unfixable and host drift would kill it);
  **toolchain pinning at M0** (`VERSIONS` — the §9.4–9.5 findings are
  version-dependent and the versions were recorded nowhere); **the bench
  pass** (zero runtime performance numbers existed — P4's belief test had
  no baseline); **the WebKit gate on M6's stopgap claim** (a dated decision
  rode on an unverified assumption); **the trust-boundary contract in
  architecture.md**; **NOTICES on the demo**; accessibility and M8's
  boring-primitives rule noted where they will be paid. Meta-verdict,
  recorded: the thinking phase is complete — declare, roles, design
  thinking, hats — and the review rituals consolidate into one cadence
  (ledger + design-thinking + hats; after each shipped form, any validation
  event, or quarterly). The next commit builds.

- **(2026-08-29) The virtue-ethics pass — character made explicit.** The
  third lens ([virtue-ethics.md](virtue-ethics.md)): the telos named (the
  counterfactual *inhabited*), the project read as a MacIntyrean practice
  whose goods are internal, and eight virtues located as means between
  vices with receipts from this log — the founding thesis restated as
  ethics: **Plan 9's temperance tipped into excess at the compatibility
  break, and this project is the recovered mean**; the log itself
  recognised as a record of means being found (necrolatry↔vandalism at the
  re-founding, cliff↔bloat in the measured personality, garden↔doormat in
  typed-at-the-edges). Three dispositions adopted: the **external-goods
  question** joins the ledger review (MacIntyre's corruption warning,
  asked on the record before the demo brings an audience); **M5's
  acceptance gains the text-mirrored console** (accessibility reframed
  from feature to justice); and the working rules gain **"a commit message
  claims only what its diff contains"** — habituated from the same day's
  worked example, an overclaimed commit caught against the tree. The
  practitioners section binds both practitioners, human and AI, and names
  the AI's standing temptations: overclaim, flattery, performative
  industry. The lens joins the consolidated review cadence.

- **(2026-08-27) OCI is two targets, taken at two different weights.**
  *The scratch container is a stated target of the Rust milestone, for free*: the
  kernel as a static musl binary, PID 1 in a `FROM scratch` image — no distro, no
  userland, a few megabytes. Linux is the thinnest mach layer of any host: `futex`
  is what `Atomics.wait` has been emulating all along, threads are Workers without
  ceremony, and with no JIT ban the guests run on full wasmtime/Cranelift — the
  fastest deployment of the system anywhere. It is also the natural CI machine.
  *The cloud machine — no OS underneath — is a named aspiration, sequenced after
  macOS and iPadOS*: the kernel booting directly on a hypervisor (Firecracker/KVM;
  Kata- or Unikraft-style OCI packaging of the microVM), `pc/` directory number
  four. Its mach layer is the first to add mechanism rather than glue — a timer
  tick driving **epoch/fuel preemption** in the runtime (Workers gave preemption
  free until now), **virtio-9p** serving the root straight into `devmnt`'s native
  tongue, virtio-console for `#c`, and eventually virtio-net under **smoltcp** in a
  `devip`-shaped device, the largest single lift. What it never adds is the part
  that makes bare-metal kernels brutal: wasm's sandbox replaces the MMU (one
  address space, no page tables, no context-switch assembly — recording the risk
  plainly: a runtime escape is then a whole-system escape, with no hardware
  backstop), and 9P replaces the VFS and the driver zoo. The wasm-native OCI shims
  (runwasi, the component model) are watched, not targeted: they cannot yet host a
  kernel that spawns instances with shared-memory mailboxes. The strategic point of
  the cloud machine: a microVM that boots in ~100ms to a per-process-namespace
  world, `exportfs` handing private namespaces between containers over wire 9P —
  namespaces as the cloud primitive the sidecar pattern keeps reinventing badly.

- **(2026-08-27) Storage in containers: the invariant and the design.** First the
  inversion that makes this easy: an OCI image is a union filesystem, a volume is a
  bind mount, and container startup is namespace assembly performed by the runtime —
  the container ecosystem reimplemented Plan 9's namespace operations one layer
  down, so ipnx in a container sits above a machine that already thinks in binds
  and unions. The invariant, held on every rung: **ipnx never learns an on-disk
  format.** Durability is always somebody else's filesystem, reached through 9P or
  a host Dev; virtio-blk plus a filesystem of our own is the door that stays
  closed, and fossil goes unported even in spirit. Per rung:
  *Scratch container* — the image carries the kernel and the rootfs seed; the seed
  becomes the ramfs (the running root is synthesized, never mutated — the container
  ideal, already the PoC's behaviour). Volumes enter through the Linux mach layer
  as a host-fs Dev or a mach-side 9P server, and init's profile binds them into
  place — Plan 9's `/lib/namespace` reborn as the container's mount configuration,
  the YAML of volumeMounts replaced by a namespace script. Recorded honestly:
  Plan 9 unions are not overlayfs — creates land in the MCREATE element but there
  is no copy-up — and the resolution is Unix's own layout discipline, a read-only
  seed (`/bin`, `/lib`, `/rc`) with the writable trees (`/usr`, `/tmp`, `/data`)
  mounted whole from the volume, which is how V10 machines were actually laid out.
  *Cloud machine* — no Linux below, so durability arrives as 9P over a virtio
  transport: virtio-9p under QEMU/Kata, and under Firecracker (which has no 9p
  device) **wire 9P over vsock** — a stream transport plus the mount driver that
  already exists, since `devmnt` mounts any fd speaking 9P. The host agent — a
  hundred lines against the host's real filesystem — owns ext4 or whatever lies
  beneath. Storage therefore adds almost nothing to the cloud machine's mach-layer
  heft; the heft stays in preemption and networking, because 9P absorbs storage.
  *Between containers* — `exportfs` already hands a namespace, private binds
  included, to another process; over a container network it is the same bytes on a
  TCP or vsock stream. Persistent-volume claims, storage sidecars and CSI drivers
  collapse into: one container exports `/data`, another mounts it — per-process
  namespaces as the composition primitive between containers, the CPU-server model
  in OCI clothing, needing nothing beyond a network transport the PoC does not
  already demonstrate.

- **(2026-08-27) The dev toolchain: mk, diff, `/cc` as a capability — and the
  document factory is Plan 9's.** `mk` joins the workbench tier: it is the one
  tool both heritages own — Hume's, born in Research Unix (Ninth Edition) and
  carried into Plan 9 — so building either userspace with mk inside ipnx is
  historically correct twice over (cite mk(1) from the Tenth Edition manual via a
  `../ipnx` measurement when it lands). Its port profile is sam-shaped: libbio, a
  hand-written parser, recipes run through the real rc, out-of-date decisions on
  ramfs mtimes, bare forks so it rides asyncify. `diff` joins beside it — the one
  hole in the daily command set. Compilation stays host-side per the self-hosting
  non-goal, exposed as **`/cc`, a mach-layer file server**: write source, poke
  ctl, read the object or the errors back; a ten-line guest `cc` gives mkfiles
  something to call, and a process without `/cc` in its namespace cannot compile —
  the workflow self-hosts, the compiler does not. acid/db/prof are not ported
  (they read Plan 9 a.out symbols; wasm has none — host tooling stands in, and
  any future in-system debugger is a `/proc` view of guest memory, not an acid
  port); nm/ar/strip/cpp stay llvm, host-side; yacc is optional-later for
  in-system regeneration through `/cc`.
  **The document factory — troff, eqn, tbl, pic, grap, dpost — is taken from
  Plan 9, superseding the earlier V10-side disposition.** The reasoning: Plan 9
  troff is not a rival to Research troff but its own later life — Ossanna to
  Kernighan's device-independent rewrite to Ninth/Tenth Edition to Plan 9, the
  same lineage, rune-aware to match a system that is UTF-8 end to end; V10's is
  the identical machine frozen earlier, pre-Unicode, in K&R. Taking Plan 9's
  moves the document factory OFF the parent project's ANSI-conversion dependency
  entirely (the V10-growth rule itself is unchanged). Staging: troff with the man
  macros and terminal output first — `man(1)` readable in a win — then eqn and
  tbl, pic and grap behind the float door they need, dpost when paper matters;
  font and device tables ship in the rootfs seed.

- **(2026-08-27) The curation principle: the Plan 9 command set is in scope
  entire, because it is the designers' own testimony about Unix.** The earlier
  capped-workbench framing is superseded. Plan 9's `/bin` was assembled by
  Unix's own authors at the one moment compatibility no longer protected the
  accidents — it is what they chose to carry, and this project undoes their
  kernel pivot while preserving their curation. So every Plan 9 utility with
  Unix ancestry replaces the equivalently named Unix tool in `/bin`, including
  the small filters (`join`, `comm`, `look`, `fmt`, `fold`, `freq`, `split`,
  `strings`, `sum`, `cal`, `factor`, `primes`, `units` and kin), `cron` in its
  local-execution slice (the dial-out half waits for `/net`), and **the games**
  — Unix's humanity, kept by the designers, and incidentally the draw stack's
  best free stress tests, being interactive libdraw clients of a kind samterm
  is not. The principle cuts both ways: **the absences are curation too** —
  `/bin` inherits the refusals (no `head`: the Research line's answer was
  `sed 10q`; no `find`: `du -a` piped to `grep` is the curated idiom), and the
  namespace reconciles philosophy without argument, since V10's own `find`
  will live in `/v10/bin` and bind order chooses. The boundary: the principle
  covers the utility set — things a person types — not the pivot's
  infrastructure, which stays dispositioned at the mach layer (fossil/venti,
  factotum, ndb/cs, the compilers on self-hosting grounds); `upas` parks on
  the line with a note that mail could someday arrive as a lib9p mailfs.
  **The detailed edge set is accepted with it**: `ed` (by the troff reasoning —
  Research ed's own later life, rune-aware; V10's ed will sit beside it as cat
  and echo already do), the real **lib9p** (newly portable on this week's
  libthread and bare fork; brings iostats — 9P tracing for this project's own
  development — ramfs(1), srvfs with a ~50-line `#s` device, mntgen; our
  lib9p.[ch] retires; Tflush stays a kernel-side deviation servers simply
  never see), **tar + libflate + gzip** paired with the storage decision's
  host-ingress Dev as the door in and out, the **observability pair** (`ps`
  over a devproc brought up to the real status format — the kernel conforms,
  vendored code does not bend — and `ns` over a new `/proc/n/ns` file whose
  hard half, recorded source paths on union elements, unmount already built),
  the **float door** (un-exclude fltfmt/strtod, vendor port's own Cody-Waite
  era math C; opens awk, dc, bc, hoc, seq, units, pic, grap), the **digest
  slice** of libsec (md5/sha1 only; TLS stays mach-side), real **font files**
  in the rootfs seed, `xd`, `p`, `dd`, `time` (await's times vector gains real
  wall-clock deltas). Flagged for the Rust milestone, not decided now:
  adopting the real **libmemdraw/libmemlayer** as the kernel's compositor in
  place of a draw-engine transliteration — verbatim-beats-reimplementation
  reaching into the kernel, with the suite's pixel tests making engines
  swappable. Port-time rule as ever: verify contents against the 4e tree and
  measure before claiming (the games list, cron's exact shape, tarfs's
  provenance).

- **(2026-08-27) The completeness principle for V10 — and upas resequenced, not
  refused.** Refusal and sequencing are different acts, and only the second is
  ever applied to software with Research ancestry. Stated for the specimen side
  as the curation principle is stated for `/bin`: **the V10 personality aims at
  the whole Tenth Edition userland; nothing is refused; the only exceptions are
  dead substrates, represented by their living descendants** — Datakit (its
  role passes to `/net` over IP, the `dial` abstraction preserved) and the Blit
  hardware (whose interactive layer survives as its own descendants: sam, and
  the mux lineage the window server implements; the Blit was optional
  equipment, so the text userland is the complete common experience). Under
  this principle **upas is in on both sides** — Presotto's, born in Research
  Unix, the later Research editions' mail system, carried by its author into
  Plan 9, and by design small programs over mailbox files rather than a
  monolith. Decomposed by dependency: the **local core** (marshal, mailbox
  reading, local delivery, aliases) needs no network and is meaningful now —
  ipnx is already multi-user; Plan 9's compiles on the open toolchain door,
  V10's follows the ANSI conversion, side by side per the cat-and-echo
  pattern. **Transport** (smtp, qer/runq) parks behind `/net` beside cron's
  dial-out half. **upas/fs** — mail as a 9P filesystem — rides lib9p and
  libthread, and is the most this-system-shaped piece in the garden. The
  garden sorts by the same ancestry test: in — `faces` (vismon's descendant),
  **`proof`** (the troff previewer, Blit-era roots — typeset preview in a
  window without ghostscript, closing the document factory's loop visually),
  `calendar`, `news`, the clock-tier trinkets under the games precedent;
  sequenced behind absent substrate — `page` (ghostscript), `vt`/`con`
  (`/net`), `juke`/audio (no devaudio in any mach layer yet); out on
  principled grounds — `mothra`/`abaco` fail both tests (the web postdates
  V10: no ancestry to curate, no completeness claim) and carry the heaviest
  dependency chain in the tree. The never-list's basis is henceforth: no
  Research ancestry AND not required infrastructure — discretionary rather
  than forbidden should `/net` and appetite ever coincide.

- **(2026-08-27, the re-founding) v12 is a reimagining of Unix; the V10
  completeness principle is dropped.** The project line's own teleology, stated
  plainly: ipnx v10 was the resurrection, v11 the reckoning that resurrection is
  a logical dead end, and **v12 is the counterfactual next edition** — Unix
  written afresh on a modern stack, starting from where its creators finished
  (Plan 9), undoing the compatibility break while honouring Plan 9 semantics,
  and avoiding the nightmare of Linux, BSD, POSIX-the-standard and systemd.
  Consequences, each superseding where it conflicts:
  *The V10 completeness principle (earlier today) is superseded* — the goal was
  never to resurrect a dead operating system, and by the same period-piece
  logic that excluded mothra/abaco, much of V10 would fall anyway. The V10
  personality becomes **the exhibit**: the TUHS-tape binaries stay in
  `/v10/bin` as heritage, the parent repository remains the museum, V10's
  *sensibility* — small, sharp, anti-bureaucratic — survives as taste rather
  than checklist, and V10 growth is no longer a goal (the ANSI-conversion
  dependency dissolves). Upas and the garden dispositions stand on their Plan 9
  curation-side merits.
  *The modern personality is curated by measurement, not adopted from POSIX*:
  port the three benchmarks, record every interface they demand, and that
  derived list — the 20% of POSIX that is actually Unix: fds, fork/exec/pipes,
  dirents, errno, sockets, mmap-enough — is the specification, the
  `docs/syscalls.md` method aimed at the modern surface. APE's lesson kept,
  re-aimed: it failed by chasing the whole standard.
  *WASI is elevated from footnote to second ABI*: `wasi_snapshot_preview1`
  implemented as a syscall dialect over the same chans (preopens map onto
  per-process binds; no second VFS, no second process model), which carries
  **Go** (`GOOS=wasip1` — the gc runtime is never ported natively) and
  **CPython's official wasi builds** essentially on arrival. The §6 finding is
  unchanged: the system interface is 9P; a dialect is not an interface.
  *`libunix` takes libv10's seat* as the native modern personality for source
  ports, **git the flagship** — famously portable C, NO_MMAP fallbacks,
  fork/exec-heavy, and local git needs no sockets: `git status` on a
  namespace-mounted repository is the single most persuasive demo available. A
  native CPython against libunix later exceeds the wasi build, because
  `subprocess` needs the fork/exec this kernel genuinely has.
  *Sockets win the API; files keep the implementation*: when `/net` lands, the
  personality exposes BSD socket calls translating to dial strings against
  `/net` files — the winner's interface over the elegant loser's architecture.
  *Acceptance is operational*: v12 supports modern Unix when, measurably, stock
  git does init/status/commit/log/diff on a real repository through the
  namespace; CPython runs a script that forks a pipeline through `subprocess`;
  and a Go `wasip1` binary passes its own tests under the shim. Three programs,
  three dialects, one kernel.
  *Sequencing*: the PoC still closes with acme; post-PoC the WASI shim jumps
  the curation sweeps in priority (small kernel-adjacent work, two benchmark
  languages as the prize), then libunix-and-git as the personality milestone,
  sockets with `/net`. The README and CLAUDE.md carry the manifesto from this
  date; RESEARCH's TL;DR is annotated in place.

- **(2026-08-27) The citizenship clause: ipnx lives in the wasm world in both
  directions.** WASI-as-second-ABI was accommodation — the inbound direction,
  foreign software running here. This clause adds citizenship: ipnx also
  *provides into* the component ecosystem, a neighbour among Rust, Python, Go
  and Node components rather than a walled garden that imports them. The
  reasoning: the component model answers "how do many modules make one
  program" — typed interfaces, linking — and lacks everything an operating
  system contributes: processes, identity, names, mounts, runtime composition.
  There is no `bind` in the component world; that is this kernel's entire
  inventory. **The component model has linkers; it needs an operating
  system.** Outbound, concretely: a **9P → `wasi:filesystem` bridge** serving
  an ipnx namespace — unions, private binds, exportfs-imported trees — as a
  foreign component's world, per-request namespaces for an ecosystem whose
  preopens only gesture at them; **the Rust kernel core packaged as a wasm
  component**, embeddable in other people's runtimes as a library OS (the
  runwasi *hosting* verdict stands — watched, not targeted — providing is a
  different act from being hosted); and **preview2 with `/net`**, sockets
  arriving in WASI's timeline exactly where the network milestone sits. The
  standing tension stays stated: WIT's typed interfaces and 9P's uniform
  untyped one do not compose as system interfaces (RESEARCH §6, unchanged) —
  the resolution is topological, **typed at the edges, files at the core** —
  dialects and adapters at the boundary, nine-ish operations on names within.
  Node needs no new posture: it cannot be a guest and was the first mach
  layer — living beside the modern components is the architecture's origin
  story, now policy. This is genuinely new and beyond Plan 9, and embraced as
  such.

- **(2026-08-27) The native-to-the-new-world aspirations: cloud-native,
  AI-native, Kubernetes-native — stated now, sequenced later.** All
  aspirational, none blocking, each admitted only because it passes the one
  test: *does it become a file tree in a namespace?* **S3 over 9P** — the
  webfs pattern (Plan 9 served FTP and HTTP as file trees) aimed at a bucket:
  objects as files, an `s3fs` guest behind `/net` or a mach service before
  it, and the same treatment for any cloud API with nouns. **Lambdas both
  ways** — functions as files (write payload, read response), and the deeper
  symmetry: Lambda runs on Firecracker, and the cloud-machine rung IS the
  Firecracker architecture — ipnx *as* the function is the already-specified
  deployment form meeting its market. **Inside k8s** — mach-layer courtesies:
  logs to stdout (cons already is), config as namespace scripts (decided),
  SIGTERM as a note, probes as an exec'd rc script or a served health file,
  PVCs as 9P mounts (the storage decision verbatim). **Hosting workloads** —
  honesty draws the credible line: ipnx executes wasm, not Linux ELF, and
  never pretends to host arbitrary Docker images; the honest form is **a
  Kubernetes node for wasm workloads** — a CRI shim mapping pods onto
  processes plus namespaces, a stronger isolation and composition story than
  the existing wasm shims, because pods are namespace assembly and that is
  this kernel's native verb. **AI-native** — models as files (`/mnt/llm`:
  write prompt, read completion; sessions as directories) is the easy half;
  the strong half is that **a per-agent namespace is the capability model the
  agent world is groping toward**: an agent's whole visible world assembled
  from binds and unions, nothing else reachable by construction, iostats as
  the audit log, MCP-shaped tool access as "mount this tree". That one is not
  ipnx keeping up with AI; it is ipnx holding the answer to agent sandboxing
  that the ecosystem currently fakes with allowlists. Sequencing: post-PoC,
  mostly post-`/net`, none ahead of the benchmarks.

Evidence for each is in RESEARCH.md at the cited section.

- **The kernel call list is derived** — [docs/syscalls.md](syscalls.md), call by call.
  Of Plan 9's 40 live calls, 29 never leave the supervisor; 11 are the file interface, ten
  of them one 9P message each and `mount` the wire boundary itself.
- **9P2000** (§2). `version(5)`: "Currently, the only defined version is the 6 characters
  ″9P2000″." Original 9P survives only in period servers this system never has to speak to.
- **Wire 9P at boundaries, a Dev table inside** (syscalls.md). Plan 9's own kernel shape:
  devices present the file interface as function calls; only the mount driver marshals 9P.
- **Base: Plan 9 4th edition as reference, 9front consulted for fixes** (§10) — both MIT,
  and the kernel is transcribed structure, not a forked tree.
- **The lazy fork has its resume mechanism, and its bound** (RESEARCH §5.2): the child's
  `exec` throws, a hand-assembled `try_table`/`catch_all` guard frame catches, the
  supervisor restores the `[0, sp)` stack region it saved at fork, the guard returns the
  child's pid. The catch frame must be live when the child execs, so the child's pre-exec
  code runs inside the guard's extent — **`procrfork(flags, fn, arg)`**, Plan 9's own
  thread-library shape. Bare dual-return `rfork(RFPROC)` on a JS engine stays asyncify's
  case; the native interpreter owns its frames and can lift the restriction. Proven
  end-to-end in [poc/](../poc/).
- **Syscall transport: a Worker is a process** (§5.3). Blocking calls are an unanswered
  SAB mailbox plus `Atomics.wait` in the Worker; no asyncify anywhere in the base system.
  In the browser this costs cross-origin isolation (COOP/COEP); in Node nothing.
- **One guest substrate.** Wasm on every platform; the JS-engine path is built first.
- **`/dev/tty`: there is none.** The console is `/dev/cons`; the V10 personality's libc
  aliases the name. The fd-3 accident stays history.

## emca's decisions and resolved questions (2026-08-31)

Moved here from emca.md 2026-09-02: the spec states what emca *is*, and the
record of what was decided and which questions closed belongs with the other
decisions. **Dated entries keep the words they were written with.**

### What was decided

```
1.  emca is the IPNX user interface — what the system boots into,
    on every surface. Not an editor; an editor is one window type.
2.  Two halves: IPNX owns state, meaning, policy; the surface owns
    rendering and input. Differs by device -> surface; differs by
    workspace -> IPNX.
3.  Four protocol additions and no more: structure roles, window
    type, verb applicability, show request.
4.  Everything is managed as a file; there are no manager programs,
    only filesystems, types and a surface.
5.  A window type is a triple: namespace, optional command, window
    configuration. The command being optional dissolves the types-
    vs-programs fork.
6.  Core verbs are emca's; extra verbs are the type's. They live on
    the window's toolbar, and the tag line supplies their argument.
7.  Text is the default; a type that renders non-text owes a
    reason.
8.  /type is itself a type: the UI is configured by editing files
    in the UI. One built-in type, dir, is the bootstrap floor.
9.  Root names are <= 4 characters; /project is the one exception,
    with its reason recorded.
10. /pkg and /project are different types: ingredients and dishes.
    Packages are leaves, projects combine them.
11. A project is a proto-process; /project and /proc are the same
    information at two times.
12. Templates instantiate; workspaces open. Promotion promotes the
    declaration, not your files.
13. Clone and instantiate are separate acts, and instantiate shows
    the declaration first.
14. /pkg dependencies are declared; language dependencies are
    content.
15. Breakpoints in characters, not pixels — 72 columns a leaf, 10
    lines a body. Nothing disappears as the viewport grows.
16. One structure, two knobs: a concertina row, a rail entry and a
    minimised window are the same object. No tabs at any size.
17. The window in four parts, backed by ONE tag string, so the 9P
    interface and the suite are untouched.
18. The toolbar is closed and type-derived; the tag bar is open and
    the user's. This fixes acme's conflation of methods with
    subprocesses.
19. Operand determines surface, and each surface sits where its
    operand is.
20. ONE OBJECT: a window is a rectangle with a tag, containing
    either a body or child windows. Acme's own dat.h says a column
    is a window; depth three was its implementation, not its
    concept. (2026-09-01)
20a. ALLOCATION IS THE LAYOUT MODEL: a parent gives rectangles to
    some of its children along an axis, and those it does not
    allocate to appear as TABS. So a tabbed window is a maximised
    one, "tabs show only when there is more than one" falls out
    rather than being enforced, and stack disappears as a third
    composition beside row and column. (2026-09-01)
21. EVERY WINDOW IS A COMPOSITOR and runs it on itself. The root
    window IS the screen. Named regions — pane, rail, leaf — do not
    exist. (2026-09-01)
22. THE TAG LINE IS AN OPERAND, not a command line: its text is the
    argument to its own buttons — Run, Add, Open, Find. That is
    the 2-1 chord decomposed, and it retires the pin. It does NOT
    retire the selection's verbs: composing text and pointing at
    text are different operands. AN EMPTY TAG LINE MEANS "USE THE
    SELECTION", which reaches all six from permanently visible
    buttons, so a menu at the selection is an optional surface
    shortcut and not a required fifth surface. (2026-09-01)
22a. FOUR SURFACES, ONE OPERAND EACH: the window in its layout ->
    the title bar row; the window's content -> the toolbar; the tag
    line's text -> its own buttons; a selection in the body -> the
    floating bar. And EMCA USES THE ERA'S NAMES — Copy, Save,
    Revert, Open — while acme's port keeps Snarf, Put, Get and
    Zerox, because renaming acme's buttons would be changing acme.
    (2026-09-01)
23. Every window has its own status bar, carrying that window's
    state. The root window's is the workspace's.
24. The surface owns text input natively; the hand-rolled caret is
    deleted; autocorrect off.
25. Type buys back the single tap for structured output.
26. N windows may view one buffer; Zerox aliases.
27. Window controls INFORM THE PARENT — close, minimise, maximise.
    A child never resizes or removes itself, which is what makes
    the recursion work. MINIMISE AND MAXIMISE ARE ONE OPERATION:
    minimise(me) moves me out of the allocation, maximise(me) moves
    everyone else out, and each is undone by moving back. Both cost
    one strip however many windows are in it — which the discarded
    "maximise minimises every sibling" did not, since that cost a
    title bar per sibling and so freed little room. A TAB IS A WHOLE
    WINDOW the parent has not given a rectangle to, never a reduced
    one. (2026-09-01)
28. The system boots into emca; the default workspace is a
    namespace file; the hosts become surfaces; rio-today retires.
29. EDITING IS THE SURFACE'S. emca implements sam (batch,
    structural), not a WYSIWYG editor; the host displays, scrolls
    and EDITS with Monaco, TextKit or the platform's own. Selection,
    clipboard, command history, line editing and terminal emulation
    are all host-side; /dev/snarf is the sync point.
30. emca is a file server with a workspace, not an editor.
31. xterm.js returns as the RAW-INPUT door beside the console's line
    door — the "AND, not XOR" given its architectural reason.
32a. THE UNIT IS DEVICE-INDEPENDENT PIXELS, plus a reported text
    cell. Characters stay the leaf measure (72 x cellWidth) so
    accessibility sizing still moves the breakpoints, but they
    cannot be the unit: acme could measure in characters because
    everything was text, and emca holds images, video and
    PostScript, which have aspect ratios. Display PostScript and
    Quartz are the precedent. (2026-09-01)
32. FOUR SEMANTICS, ONE PROTOCOL. Content is 9P directly and the
    host renders it natively — IPNX implements no renderers.
    /dev/window/<type>/<n> is the bidirectional control interface,
    with the TYPE IN THE PATH. /type is the registry both sides
    read. /dev/canvas narrows to genuine drawing, the exception.
33. Controls name a side: ipnx:Put round-trips, host:toggle-wrap
    never does. The two-halves split, enforced in the vocabulary.
34. Put is the host streaming the edited file back over 9P.
35. emca HOLDS THE AUTHORITATIVE BUFFER; the host mirrors it, with
    optimistic local echo. Forced by four things, only one of which
    is undo: the headless suite, sam, Put and the filters, and the
    body file for client programs. Each edit carries a sequence
    number, each sync a hash; mismatch triggers resync. con(1)'s
    "apps never re-read" becomes "never re-read ROUTINELY".
36. ONE UNDO STACK, IN EMCA. The host's undo is disabled and Cmd-Z
    round-trips — emca is not remote, so every argument for a local
    optimistic stack is a latency argument that does not apply here.
    Acme's infinite undo survives exactly.
37. /mnt/acme RETIRES, merged into /dev/window — one window
    vocabulary, and /dev/window must therefore be usable by ordinary
    programs, not only by emca and the host.
38. VERSIONING IS POLICY OVER #V: optional, off by default,
    per-namespace, triggered on Put for authored files and on a
    clock for things that change without human intention. Nothing
    may be built on it, because it can be off — which is what
    protects 36.
39. /dev/window BELONGS TO EVERY PROGRAM — emca has no privilege,
    only a job. One binary serves both worlds: `pkg` lists to
    stdout, `pkg --emca` mints a window and lists them there.
40. NO /dev/emca. *(SUPERSEDED 2026-09-02 on both its premises: there
    is no device — the window system left the kernel entirely — and
    "emca is a watcher, not a gatekeeper" was retired when emca became
    the window manager. `/dev/emca/<n>/` now names the WINDOW SET that
    emca serves, which is a different object from the control channel
    rejected here. WHAT SURVIVES is the mechanism below: 9P has no
    change notification, so a root `events` whose reads PARK is still
    how anyone learns a window appeared.)*
    The device already mints; the difficulty is that
    9P has no change notification. So /dev/window grows a root
    `clone` and a root `events` whose reads PARK — and BOTH the
    host and emca read it. The device mints (mechanism), emca
    watches and places (policy), the host watches and renders.
    emca is a watcher, not a gatekeeper.
41. It degrades correctly: with emca not running the window still
    exists and still renders, in its type's default pane, because
    the type is in the path. *(RETIRED 2026-09-02 with item 40: panes
    are gone, and the real degradation is better — no window manager
    means no windows, with the devices going straight to the host's
    screen and keyboard. IPNX as a CLI is a mode that must keep
    working, and that is what "degrades correctly" now means.)*
```

### The open questions, resolved (2026-08-31)

Worked through in one pass. Most resolved by reasoning from decisions
already taken; two were stale (answered by later decisions and never
struck out); one resolved by ADOPTING an existing protocol rather than
designing one; one DISSOLVED by the canvas redesign. What genuinely
remains is listed at the end and is small.

SNAPSHOT VS REPLAY — RESOLVED: snapshot, and it is free.
```
A project's writable layer IS a tree, so keeping it is the snapshot;
nothing is captured because nothing was ever anywhere else. Replay
is the optional extra: the declaration records provenance (which
/pkg entries, which language installs) so a workspace can be audited
and rebuilt from scratch when someone wants that. Docker's answer by
default, Nix's available, neither implemented as machinery.
```

THE PLUMBER — RESOLVED BY ADOPTION, not design.
```
Adopt Plan 9's plumb(6) rules syntax and message format verbatim;
own the model. The house move exactly — the same "adopt the
notation, own the model" that took SVG path data for canvas — and it
answers the standing instruction to maximise reuse of existing
protocols. Look's dispatch IS a plumb rule evaluating; nothing new
is invented, and the plumber finally has a face: the FLOATING BAR
shows which of open/go-to/search/plumb the rules chose, instead of
look deciding silently.
```

THE SURFACE'S OWN SUITE — RESOLVED: the surface publishes what it
```
rendered, and the suite asserts over that.
Each window grows a `ui` file listing the controls the surface
actually produced — label, role, keyboard path, enabled state. On a
real surface it is DERIVED FROM THE PLATFORM ACCESSIBILITY TREE (the
a11y tree is precisely the "can this be reached" answer, on the web
and on Apple alike); on the virtual surface it is synthesised. So
ONE test asserts "every floating-bar verb and every toolbar button
is present, named, and keyboard-reachable" and it runs on every
surface including headless.
This also turns an assertion into a measurement: the input
convention claims accessibility is "enforced BY CONSTRUCTION", and
until now nothing checked it. Now the suite does.
```

CROSS-WINDOW AT SMALL — RESOLVED as a stated degradation.
```
Cross-window OPERATION works at every size: the tag line carries the
argument, and it survives navigation because it is the window's own
text (2026-09-01: this was the pin's job before the tag line
subsumed it). Cross-window VIEWING does not and cannot at one
visible body — no design can show two bodies on one small screen.
So it is named rather than solved: at small, emca is a reduced emca,
and the reduction is exactly one thing.
```

UNNAMED INSTANCE STORAGE — RESOLVED: it is a rename, not a copy.
```
An instantiated template gets its writable layer at birth, under
/usr/$user/.inst/<id>. "Save as workspace" RENAMES that tree into
/usr/$user/<name>. Nothing migrates, nothing is copied, and there is
no window during which work is somewhere it might be lost.
```

THE FLOATING BAR'S CONTENTS — RESOLVED in the part that is derivable.
```
The SET is closed and comes from acme's layer 3, not from taste:
cut, copy, paste, execute, look (open/jump/search/plumb), pin, Edit.
What is not derivable is ORDER and GROUPING, which is genuinely
empirical and stays a measurement task rather than a design
question. The set can ship; the arrangement is tuned against use.
```

HOW A TYPE DECLARES A VIEW MODE — DISSOLVED by the canvas redesign.
```
It does not. The host opens the file over 9P and RENDERS IT
NATIVELY according to what it is; a type declares a namespace, an
optional command, and chrome — and nothing whatever about
rendering. So there is no vocabulary for a type to grow into, and
the widget-toolkit tripwire has nothing to trip. The redesign
removed the risk rather than bounding it.
```

WHO OWNS UNDO — STALE. Answered by decision 36: one stack, in emca,
```
the host's undo disabled, Cmd-Z round-trips because emca is not
remote. *(SUPERSEDED 2026-09-02: undo is content state, and emca
became the WINDOW manager while a TYPE manager owns what is in a
window — so the stack belongs to the text manager, not to emca.)*
```

BUFFER FIDELITY — STALE. Answered by decision 35: sequence number per
```
edit, hash per sync, resync as a repair path, and "apps never
re-read" relaxed to "never re-read ROUTINELY".
```

PROPERTY 1 UNDER MONACO — RESOLVED by reframing: it is a SELECTION
```
CRITERION, not a risk.
The design does not depend on Monaco; it depends on the editor
component exposing the selection and accepting custom commands in
its context menu. Monaco does (addAction, getSelection), CodeMirror
does, TextKit does. A component that does not is DISQUALIFIED — so
the question is how a component is chosen, not whether the approach
works. The spike still runs, to verify the chosen one; it no longer
gates the design.
```

SURFACE DEPENDENCY DIVERGENCE — NOT OPEN. Accepted and named by the
```
input convention (2026-08-30): divergence between surfaces is
expected, and Monaco on the web against TextKit on Apple is that
convention working, not a problem to solve.
```

## Open questions

- ~~The uid model~~ — **decided and running**: [docs/identity.md](identity.md). Per-process
  credentials in the kernel, `/proc/<pid>/ctl` transitions with no new syscalls,
  9P2000.u's `DMSETUID` at exec, V10 enforcement in-process and per-attach identity on
  the wire. The answer to "is V10 compatibility real?" is yes.
- ~~Hard links and the symlink family~~ — **decided and running**. The choice was
  *mint*: `Tlink`/`Tsymlink`/`Treadlink` live at types 128/130/132, a range no 9P
  dialect uses, so the base version stays plain `9P2000` and a server without the
  extension answers `Rerror` — the client degrades gracefully (tested against one).
  Symlink identity rides 9P2000.u's `QTSYMLINK`/`DMSYMLINK` bit positions, like
  `DMSETUID` before it. Three kernel calls join the interface (traps 60–62; `lstat` is
  `stat` with a nofollow flag, not a call), and resolution follows **V10's rule: the
  kernel resolves symlinks in the walking process's own namespace** — no server knows
  the client's namespace, which is why this cannot be delegated. A symlink created
  through a mount into an exporter's tree, read back through the mount, resolves to the
  *client's* `/etc/motd` — the acceptance test that settles it.
- **The modern-draw question** (raised by the author's demo review,
  2026-08-29): character windows are DOM-first (xterm.js — settled and now
  shipped in the demo shell); should draw(3) itself gain a modern backend —
  the `d`/`L`/`e` ops mapped to canvas/SVG/SwiftUI vectors rather than a
  raster — or a modern *dialect* for future personality apps, while the
  verbatim editors keep the raster path their code emits? Bitmapped displays
  are not how UIs are built now; the file-server *interface* is the
  invariant, the backend is per-platform by decision. Needs a dated decision
  before M3's presentation layer hardens.
- kencc or clang for the *ported* userspace, given `extern register` and anonymous struct
  members? Fresh code is clang (§9.4 is the measured recipe).
- Does the `d` message's alpha compositing map cleanly onto canvas and Metal, or does the
  window server rasterise? Decides whether the backend is thin.
- Heap sizing where guest memory is *shared* with the supervisor for zero-copy I/O — a
  shared memory must declare its maximum up front. The unshared default grows freely.

## The proof of concept — complete, and recorded elsewhere

The PoC ran 2026-08-26 → 2026-08-29 and was **declared complete** at 131
acceptance tests green on three hosts (Node, Chrome, the Rust core under
wasmtime), with the real Plan 9 userspace — rc, sam, acme — the V10 exhibit,
and the WASI citizens (Go, CPython) running. Its full chronology and final
state are frozen in [poc.md](poc.md); `poc/` itself is frozen as the reference
implementation and conformance oracle. **The build sequence from here is
[implementation.md](implementation.md)** — this document remains the design
and the decision log.

## Sources

- [Plan 9 from Bell Labs (design paper)](https://9p.io/sys/doc/9.html) ·
  [`intro(2)`](https://9p.io/magic/man2html/2/intro) ·
  [syscall numbers](https://raw.githubusercontent.com/0intro/plan9/master/sys/src/libc/9syscall/sys.h) ·
  [`rfork(2)`](https://9p.io/magic/man2html/2/fork) · [`rio(4)`](https://9p.io/magic/man2html/4/rio) ·
  [`draw(3)`](https://9p.io/magic/man2html/3/draw) · [`cpu(1)`](https://9p.io/magic/man2html/1/cpu) ·
  [`a.out(6)`](https://9p.io/magic/man2html/6/a.out)
- [APE — The ANSI/POSIX Environment](https://9p.io/sys/doc/ape.html) ·
  [Plan 9 C Compilers](https://9p.io/sys/doc/compiler.html) ·
  [How to Use the Plan 9 C Compiler](https://9p.io/sys/doc/comp.html) ·
  [Adding Application Support for a New Architecture](https://9p.io/sys/doc/libmach.html)
- [9vx](https://swtch.com/9vx/) · [9VX wiki](https://9p.io/wiki/plan9/9vx/index.html) ·
  [Vx32 (USENIX '08)](https://pdos.csail.mit.edu/papers/vx32:usenix08.pdf) ·
  [Inferno ports: hosted and native](http://doc.cat-v.org/inferno/4th_edition/inferno_ports) ·
  [Harvey OS / APEX](https://github.com/Harvey-OS/apex/wiki) ·
  [plan9port `devdraw`](https://9fans.github.io/plan9port/man/man1/devdraw.html)
- [WasmFX explainer](https://wasmfx.dev/specs/explainer/) ·
  [Binaryen's Asyncify](https://kripken.github.io/blog/wasm/2019/07/16/asyncify.html) ·
  [Emscripten: Asynchronous Code](https://emscripten.org/docs/porting/asyncify.html) ·
  [WASIX `proc_fork`](https://wasix.org/docs/api-reference/wasix/proc_fork)
- [WASI proposals and phases](https://github.com/WebAssembly/WASI/blob/main/docs/Proposals.md) ·
  [WASI roadmap](https://wasi.dev/roadmap) · [WASI 0.3 launched](https://bytecodealliance.org/articles/WASI-0.3) ·
  [wasi-filesystem README](https://github.com/WebAssembly/wasi-filesystem)
- [WasmKit](https://github.com/swiftwasm/WasmKit) ·
  [Pulley — wasmtime's portable interpreter](https://docs.wasmtime.dev/examples-pulley.html) ·
  [wasmtime tiers of support](https://docs.wasmtime.dev/stability-tiers.html) ·
  [wasmi](https://github.com/wasmi-labs/wasmi) ·
  [wasm3 performance](https://github.com/wasm3/wasm3/blob/main/docs/Performance.md) ·
  [LLVM D46141 — `--stack-first`](https://reviews.llvm.org/D46141) ·
  [goken9cc](https://github.com/aryx/goken9cc) · [Wanix](https://github.com/tractordev/wanix)
- [ZenFS](https://zenfs.dev/core/) · [OPFS](https://web.dev/articles/origin-private-file-system) ·
  [`createSyncAccessHandle()`](https://developer.mozilla.org/en-US/docs/Web/API/FileSystemFileHandle/createSyncAccessHandle) ·
  [xterm.js](https://github.com/xtermjs/xterm.js) ·
  [iOS sandbox: no child processes](https://developer.apple.com/forums/thread/747499)
- Measurements against Research Unix V10 come from the parent repository
  ([ipnx](https://github.com/ChristineTham/ipnx)): `usr/src/sys/os/{sysent.c,mount.c}`,
  `usr/src/sys/{io,vm,md,ml,fs}/`, `usr/src/cmd/sh/xec.c`, `usr/src/libc/sys/open.s`

