# The plan

**Role: a *how* — the plan.** Replanned in full on 2026-09-04 (Christine); the
plan it replaces is
[archive/implementation-2026-08-29.md](archive/implementation-2026-08-29.md),
retired for growing by accretion, and the reason is in [design.md](design.md)
under that date.

**The end of the plan is IPNX and Saranos fully implemented on every target
named: browser, macOS app, iOS app, container, MicroVM, real hardware.** The
gaps between here and there are documented in it and filled in order — designed
when reached, not now. **The demo is the first milestone and the present
focus**; we have all we need for it.

## Vocabulary

Three **layers** are the *what* — the architecture, from
[saranos.md](saranos.md):

| layer | | |
|---|---|---|
| **IPNX** | the kernel and the userspace | **no window, no mouse, no draw, no canvas** — which is *why* none of those can be in the kernel |
| **emca** | the windowing and UI system | spans the IPNX side and the host side |
| **Saranos** | the operating system as a product | the host and the wasm side together |

**Phases** are the *when* — P0, P1, … — and a phase may advance more than one
layer. Every phase states what it **builds**, what it **deletes**, what it
**depends on**, its **acceptance**, and the **gaps it exposes**. A gap is
written down where it is found and filled in a later phase; nothing undesigned
is built.

**The existing code is legacy**, refactored toward the design and never built
upon as-is. What is right is kept; what is wrong goes; the measure for the
kernel is `plan9/`. The frozen PoC stays the record of what the PoC did.

Three rules from [design.md](design.md) govern every phase: **the kernel is a
subset of Plan 9's**, deviating only to run on a VM at all and then only in a
form Dis or the CLR could satisfy; **the kernel does not grow**; **a
personality — V10's included — is userspace.**

---

## Milestone D — THE DEMO

**A minimum viable proposition.** It takes what is designed as of today and
proves it can replace the demo live at
[christham.net/ipnx-v12](https://christham.net/ipnx-v12/). It is not a final
state. Design resumes after it. **Don't overengineer.**

| | | proves |
|---|---|---|
| **D1 — the CLI** | typing `ipnx` in a macOS terminal boots IPNX to `rc`; you run userspace commands | the kernel is a Plan 9 subset, the root is the Plan 9 way, every personality is userspace, the host is a console and storage |
| **D2 — the website** | emca in the browser doing what the site does now — a listing on the left, `motd`/`tour`/`README` as tabs, `rc` below — with the windows, toolbar and status line to spec | emca owns windows entirely, the contract and the types hold, the surface renders files and never pixels |

**Not in the demo:** a window on a Mac or an iPad, the raster, acme, samterm,
`/net`, the profile, git. **Phases P0–P5 deliver it.** P6 onward is the rest of
the plan.

---

## P0 — ground *(S)* — done 2026-09-03/04

**Builds:** the reference trees — `plan9/` (9legacy) and `plan9-stock/`,
gitignored — so a claim about Plan 9 traces to file and line. The audits:
[RESEARCH §9.11–9.15](../RESEARCH.md). The archive of the old plan.
**Deletes:** nothing. **Acceptance:** met.

**Exposes:** the suite's floor. The **164** assertions include ones for what P1
removes — measured on 2026-09-04: **9 uid**, **9 link/symlink**, **2 snarf**,
and **7 tree/raster**, of which 5 are emca's tree and are retargeted rather than
lost. *Proposed:* the floor becomes **what passes on the pure kernel**, and the
frozen oracle stays the PoC's record rather than a constraint on the kernel.
**Decided in P1 step 0, before any test is touched.**

---

## P1 — the kernel, cut to the subset *(L; almost entirely deletion)*

**Advances IPNX.** The kernel is `kernel/src/lib.rs` (4,967 lines) and
`draw.rs` (364). What leaves is measured in [RESEARCH §9.13–9.15](../RESEARCH.md);
what stays — the namespace machinery (574 lines), the mount driver, dispatch,
`rfork`/`exec`/`exits`/`await`, notes, pipes, `#c #d #e #p #s` — is Plan 9's
shape and is correct.

**The order is chosen so every step is a deletion or a rename, the suite stays
green through it, and a test that must be rewritten has found a real
dependency.**

| step | builds / deletes | depends on | acceptance |
|---|---|---|---|
| **0** | the floor decision (P0's gap) recorded in [design.md](design.md) | — | a number, and which assertions it excludes |
| **1** | **`AsySnap { snap, data_ptr, sp }` → `Cont(Vec<u8>)`**, opaque, never inspected by the kernel; the three hosts produce and consume it. **The clock** becomes an operation the embedding answers; the two load-bearing `#[cfg(target_arch = "wasm32")]` go | — | `grep -c wasm kernel/src` is zero outside the six stderr shims; 164 green |
| **2** | **`#\|`** takes its letter. **`'M'` returns to the mount driver**; the ramfs is reachable by another name until P2 removes it | — | the letter table is a subset of `plan9/sys/src/9/port/dev*.c` except `#Z` and the temporary ramfs name |
| **3** | **`link`/`symlink`/`readlink`** (60–62), wire types 128/130/132, `DMSYMLINK`, and the kernel's walk-time symlink resolution leave. The nine link assertions leave with them | step 0 | `sys.h` ∪ {`ARGS NOTEGET AREAD IOWAIT`} contains every trap |
| **4** | **identity narrows to Plan 9's**: `user` per process, `eve` for the machine, `iseve()`. `#c/user` readable, writable only as `"none"` (`auth.c:109`); `#c/hostowner` eve-only, and writing it sets `user`. Permission is `devpermcheck` by name. **`Cred{euid,ruid}`, `DMSETUID`, setuid-at-exec, and credential transitions through `/proc/<pid>/ctl` leave**. Of the nine uid assertions, three are Plan 9's own and survive (*`/dev/user` names the host owner*, *mode 0600 denies another user*, *the owner still reads it*); six leave. `cmd/su.c` becomes a personality program (P2) | step 0 | one identity field; the three assertions pass; `identity.md` carries its **UNDER REVISION** banner until P2 rewrites it |
| **5** | **`#H`** leaves, with `Effect::Fetch`. `pkg`'s registry fetch — today over `#H` — moves to a userspace `webfs` **or a local registry**; the demo uses a local one (`registries` allows *"a local path"*). `webfs` is **not undesigned**: Plan 9 ships it as a userspace program, `plan9/sys/src/cmd/webfs` | — | no `DevId::Web` |
| **6** | **`#w` leaves entirely** — 29 `WKind` variants, `wsys_*`, `win_*`, `cv_*`, `drawmsgs`, `draw.rs`, and `Effect::WinUpdate/WinGone/WinText/WinCanvas/WinChrome`. The 2 snarf assertions and the 2 allocimage/draw assertions leave; the 5 emca tree assertions are retargeted to `/dev/emca` | **P4 step 1** — emca must already mint windows and serve `/dev/cons` per window, or the demo's shell has nowhere to run. **This step lands inside P4** | `#w` unreachable; no `Win*` effect; the kernel is ~3,600 lines |
| **7** | **`Effect` shrinks to machine facts**: `Spawn{…, Cont}`, `ConsWrite`, `Timer`, `Host`, `ReadDone`/`WriteDone`, `Shutdown`. `SnarfSet/Get` leave with `#w` | steps 5, 6 | every variant is something a driver on a Pi could answer |

**Acceptance for P1 as a whole:** every device letter is in
`plan9/sys/src/9/port/dev*.c` with the same meaning, or is `#Z` with its
written justification ([architecture.md](architecture.md)); every trap number
is in `plan9/sys/src/libc/9syscall/sys.h` or is one of the four VM traps; the
per-process identity is one name; the kernel has no window, mouse, draw or
canvas concept; the floor passes on all three hosts.

**Exposes:** identity as a personality (P2). **Not exposed — measured and
closed:** whether links survive as a *capability* in a userspace file server.
There is nothing to relocate: no `Tlink`/`Rlink`/`Tsymlink` in
`plan9/sys/include/fcall.h`, no `syslink` in `plan9/sys/src/9/port/`, nothing
in `9syscall/sys.h`. Plan 9 answers this with `bind` and `mount`, at every
layer.

---

## P2 — the root and the personalities, as userspace *(M)*

**Advances IPNX.** Today the root is implicit — `/lib/namespace` says *"an
empty namespace resolves absolute paths through the ramfs (#M)"* — and there is
no `/boot`. **Plan 9 designed this; we read it rather than invent it.**

| | builds | deletes | depends on |
|---|---|---|---|
| **1 `#/`** | Plan 9's root device: a fixed small read-only table, `rootwrite` is `error(Egreg)`, holding empty mount points (`devroot.c`, 261 lines — Plan 9's table also holds `/boot`, which **this system has no name for yet**: step 3's gap reaches back into this table). **The plan's one addition to the kernel**, replacing `#M`+`#V` (243 lines) — net, the kernel shrinks | — | P1 step 2 |
| **2 the root file server** | a **userspace** 9P server for the rootfs — `plan9/sys/src/cmd/ramfs.c` (**945 lines**; plan9-stock's is 907 and the two differ — 9legacy is what a claim is checked against) is the reference shape. It serves the seed the host provides through `#Z`, so the host stays a storage box and the tree is a process | `#M`, `#V`, `ram_*`, `snap_*` from the kernel | 1 |
| **3 the boot path — ⚠ THE NAME IS A GAP** | The **work** is settled: something attaches the root server, binds it over `/`, and execs `init`; `/lib/namespace` loses its `bind #w /dev/window` line. Three things it is **not**. It is **not `/boot`** — *"we can't call something /boot and refer to something other than a bootloader"*, and *"not that we should implement a bootloader"* (Christine, 2026-09-04; the 2026-09-02 naming rule, [design.md](design.md)). It is **not rc** — the earlier claim; in the reference the equivalent is C and there is **no `bootrc` in 9legacy at all**. It does **not** do `newns` — that is `init`'s (`plan9/sys/src/libauth/newns.c:60`, called by `plan9/sys/src/cmd/init.c`). **What it is called, and whether it is a program at all or falls to the kernel or to `init`, is UNDESIGNED — a gap, and needs a proposal before it needs a review.** | `init.c`'s implicit-root assumption; `-w` and every window path in `init.c` | 2 |
| **4 V10, as it is** | `libv10` over the kernel, `/v10/bin/{cat,echo}` — already userspace. **`su`** becomes a V10/Unix program: identity is established per file server at attach, so `su` is *attach with a different identity*. [identity.md](identity.md) is rewritten for userspace | the kernel's V10 semantics went in P1 step 4 | P1 |
| **5 Go and Python, as packages** | the `pkg` design is spec'd ([type.md](type.md)): `/pkg/<name>/<version>` subtrees, bind-to-install, `/lib/pkg/registries`. Each ships as a package — the binary, its stdlib (Python's measured 21-file subset), its bindings, its install commands — from a **local registry** for the demo | — | `pkg`'s `#H` fetch (P1 step 5) |

**The WASI ABI's home — a gap this phase exposes, and must not silently
resolve.** Today it is served **on the host**: `hosts/macos/src/wasi.rs` (829
lines) for wasmtime, `wasi1.mjs` for Node and the browser. It is not the
kernel's — that much the purity rule settles — and it should not be the
host's either, or every host reimplements it. *Proposed:* a **userspace WASI
personality** — a libc dialect over 9P, as the modern personality is — so a
Go or Python package runs on any host unchanged. **Unreviewed.** For the demo
the host-side shim stays, because it exists and the personality does not.

**Acceptance:** `ipnx` boots through `#/` → *(the unnamed step 3 — its name is
a gap)* → the root server → `init` (which does `newns`) → `rc`; `pkg install go` and `pkg install python` from the local
registry; `gohello` and the Python test run; the floor passes.

---

## P3 — the `ipnx` host *(S–M)* → **D1**

**Advances IPNX and Saranos.** `hosts/macos/` is 2,444 lines: `main.rs`
(1,005), `ui.rs` (610, a SwiftUI window), `wasi.rs` (829). It consumes all
fifteen `Effect` variants.

| | |
|---|---|
| **builds** | a command, `ipnx`, installed on the path. `#c` to stdio; `#Z` to a directory (the rootfs seed, and the user's storage); `Cont` for fork; a timer; shutdown. `ipnx` boots to `rc` on the terminal; `ipnx <cmd>` runs a command; `ipnx -h` says so |
| **deletes** | `ui.rs` entire; every `Win*`, `Snarf*`, `Fetch` arm in `main.rs`; the pixel feed |
| **keeps** | `wasi.rs`, until P2's gap is filled |
| **depends on** | P1, P2 |
| **acceptance** | **D1.** Christine types `ipnx`, gets `rc`, runs `ls`, `cat /etc/motd`, `sam -d`, `/v10/bin/cat`, `gohello`, the Python test, and the suite. **Shown, not described.** |

**Exposes:** the container is this host with `#c` unattached (P8).

---

## P4 — emca on the pure kernel *(M–L)*

**Advances emca.** `cmd/emca.c` is already a 9P server and already owns the
tree ([window.md](window.md), [RESEARCH §9.9–9.10](../RESEARCH.md)). It still
touches `#w` at **22 points**: `clone` (×4) to mint a window, `content` (×4),
`wctl` (×3), `events` (×2), and generic paths. All of that becomes emca's own.

| step | builds | depends on |
|---|---|---|
| **1 windows are emca's** | emca **mints** a window — an id, a type, a title, a content path — and serves it at `/dev/emca/<n>/`. It serves **`/dev/cons` per window**, virtualised ([surface.md](surface.md)): a shell window is `rc` with its fds on emca's cons files. **P1 step 6 lands here** | P1 steps 1–5, P2 |
| **2 the contract, whole** | the ten files of [window.md](window.md) plus `body`; **a blocking read on `rect` is resize**; `ctl` takes close, minimise, maximise, duplicate; `/dev/window/` is emca's directory for the manager, **bound into the manager's namespace at exec** so no id appears in any path a manager uses | 1 |
| **3 the types and managers** | `text/plain` (`look`, `edit`), `inode/directory` (`look` — one name per line), `inode/system` (`manage` — executes `/type/inode/system/layout`: paths, indentation nests, `tabs` the keyword). Each type's `recognise`, `managers`, `verbs` as on disk today. The `shell` role: `manage` on `/bin/rc` runs it in a window, line discipline host-side | 1 |
| **4 the tag line and the status line** | the six verbs — New, Open, Run, Find, Edit, Add — and the status rule *consequential and not otherwise visible* ([type.md](type.md)). **Only what the demo's four windows need**: Run and Find on text, Open on the listing, the Find count on status | 3 |

**Acceptance:** with no surface at all, a headless proof opens the demo's four
windows from the layout file, reads every contract file of each through
`/dev/emca`, resizes one and sees `rect` return, closes a container and sees
its children go; the 5 retargeted tree assertions pass; the kernel has no
`#w`.

**Exposes:** the raster — acme and samterm have no `/dev/draw` after P1 step
6, and are **out of the demo** (P7); cross-window moves, the properties role,
`magic`'s grammar — parked as before.

---

## P5 — the browser surface renders files *(M)* → **D2**

**Advances emca and Saranos.** `demo/shell/` is the legacy surface and
`demo/supervisor/rustkern.mjs` its supervisor, which decodes nine `Effect` tags
(`Spawn`, `WinUpdate`, `WinCanvas`, `Fetch`, `SnarfGet`, `Host`, `WinChrome`,
`ReadDone`, `WriteDone`). `hosts/browser/src/lib.rs` consumes all fifteen.

| | |
|---|---|
| **builds** | the surface **reads emca's files** — mounting `/srv/emca.<user>.<pid>` over the host↔IPNX 9P link — and renders natively: CodeMirror on `body` for text, a list for a directory, a terminal on a shell window's cons; the toolbar from `verbs`; the status line from `status`; the panes and tabs from `/dev/emca/<n>/kids`. Events go back on `events` and `ctl`. Global toolbar: Halt, Reboot, New Shell (`/type/inode/system/verbs`); the sidebar is `inode/system`'s own window, and collapsing it is `minimise` |
| **deletes** | every `WinUpdate`/`WinCanvas`/`WinChrome`/`WinText` consumer; the pixel canvas; the hardcoded chrome subtraction (`offsetHeight - 30`) in favour of reporting the content rectangle; `winproof.mjs`/`emcaproof.mjs` rewritten against `/dev/emca` |
| **keeps** | the editor component; the COI service worker; the `gh-pages` deploy of `demo/dist` as a single parentless commit |
| **depends on** | P4 |
| **acceptance** | **D2.** The site at christham.net/ipnx-v12 shows the listing, the three tabs and `rc`, to spec, with nothing the old site did missing. The headless proofs pass. The old `demo/` lineage is archived, not kept as a parallel path |

**→ THE DEMO IS DELIVERED. Design resumes.**

---

## After the demo — to the end, on every target

**Nothing below is designed today, and none of it is designed until its phase
is reached.** Every cell that is not the demo is a **gap**; the gaps are the
plan, not an appendix to it.

| target | IPNX | emca | Saranos |
|---|---|---|---|
| **macOS terminal** | **D1** | — | — |
| **browser** | the kernel in a page (P5's host) | **D2** | **P6** the website as a product |
| **macOS app** | the `ipnx` host, embedded | **P7** the SwiftUI surface, reading files — `ui.rs` was built on a pixel feed and is rewritten, not refactored | **P7** the app |
| **iOS app** | **P9** a host — none exists | **P9** the surface | **P9** the app |
| **container** | **P8** `FROM scratch` — legacy M1 exists; becomes P3's host with `#c` unattached and `#Z` a volume | headless, or emca over the wire — *gap* | *gap* |
| **MicroVM** | **P10** the kernel on a hypervisor: there is no host, so the embedding is virtual hardware | *gap* | *gap* |
| **real hardware** | **P11** the kernel on metal (Raspberry Pi): drivers replace `Effect`; `#S` replaces `#Z` | *gap* | *gap* |

**The purity rule is what makes P10 and P11 possible at all, and it is why P1
is first.** A kernel that passes P1's acceptance has nothing in it a
hypervisor or a Pi cannot supply.

### P6 — the website as a product *(M)*
Saranos on the browser: the refuge as a place someone arrives at. Identity on
the site, storage that persists (the browser's), the registry served from it.
**Gaps:** identity on the wire; the profile ([identity.md](identity.md) stage
2); what "a user of the website" is. Designed when reached.

### P7 — the macOS app, and the raster *(L)*
The SwiftUI surface reading emca's files — `NavigationSplitView` for the
sidebar, `.toolbar` for the global verbs, the menu bar free
([surface.md](surface.md)). **The raster returns here**: the host renders
`/dev/draw` for acme and samterm. **Not a gap, decided:** *"the host renders `/dev/draw`; the kernel does not
know how to draw"* and *"we don't use `/dev/draw` — we use `/dev/canvas`"*
(both 2026-09-03, [design.md](design.md)), so there is **no rasteriser inside
IPNX** to place. `/dev/mouse` comes back as the host's; its letter, if one is
wanted, is Plan 9's `'m'` (`plan9/sys/src/9/port/devmouse.c`). **Still open:**
chattiness, unmeasured — worth measuring against real acme rather than
designing around.

### P8 — the container *(S–M)*
P3's host with no console attached; `#Z` on a volume; the suite as the image's
health check. **Gaps:** what a container *does* with no surface — a file
server, a build box — and how emca reaches it over the wire.

### P9 — iOS *(L)*
A host from nothing, then the surface, then the app. **Gaps:** everything —
the runtime (WebKit launcher, or a native host), storage, the surface on touch,
the App Store's rules against JIT.

### P10 — the MicroVM *(L, research-first)*
The kernel boots from a hypervisor with no host process. **Partly answered by
the reference:** Plan 9 already boots over virtio-9p
(`plan9/sys/src/9/boot/bootvirtio9p.c`), so storage-and-boot on a hypervisor
has a shape to read rather than invent. **Gaps:** what the rest of the
embedding is when it is virtual hardware — the console, the timer — and whether the kernel is still wasm there or a native build of the
same source, which is what P1 step 1's substrate independence was for.

### P11 — real hardware *(L)*
The Raspberry Pi. **Gaps:** drivers as the embedding; `#S` storage; a console;
the boot; and every `Effect` variant answered by a driver, which P1 step 7
guaranteed is possible.

### Cross-cutting, and designed when a phase needs them
`/net` (the founding *"sockets won"* adoption); the modern personality and git
(the third benchmark); the WASI personality (P2's gap);
the profile; identity on the wire; `/project` and `template`.

---

## Sizes and sequence

| phase | size | delivers |
|---|---|---|
| P0 | S | done |
| P1 | L | a pure kernel |
| P2 | M | the root and the personalities in userspace |
| P3 | S–M | **D1** |
| P4 | M–L | emca whole |
| P5 | M | **D2 — the demo** |
| P6–P11 | M–L each | the end state, one target at a time |

**P1 steps 1–5 can start now.** P1 step 6 lands inside P4. P2 follows P1
step 2. P3 follows P2. P4 follows P3, P5 follows P4, and D is delivered.
