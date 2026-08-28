# The platforms — where it runs, and where everything lives

**Role: the *where*.** Two maps — the deployment forms the one kernel runs in,
and the canonical namespace a booted system presents — plus the **deployment
review ledger** that capability doctrine #5 ([design.md](design.md),
2026-08-29) mandates: the deployment story is re-examined periodically with
the same honesty as the code, and this file is where those reviews land, dated.
How to build and run each form: [handbook.md](handbook.md). What a host must
provide: [architecture.md](architecture.md).

## Where it runs

One kernel, one rootfs, one suite — per platform, only the host shim and the
engine change (the engine matrix is a dated decision in
[design.md](design.md)):

| form | host | engine (guests) | state (2026-08-29) |
|---|---|---|---|
| **Node** | `poc/supervisor` (frozen) | V8, Workers + SAB | **green, 131** — the oracle; also the dev loop |
| **Browser tab** | `poc/browser` (frozen) → `hosts/browser` (M5) | the page's own JIT, Workers + SAB | **green, 131** in Chrome 148 on the reference; Rust core in wasm is M5 |
| **macOS** | `hosts/macos` | wasmtime 37, Cranelift JIT | **green, 131**, headless; the app shell is M3 |
| **OCI container** | `hosts/oci` (M1) | wasmtime, Cranelift (or AOT `.cwasm`) | scaffolded — `FROM scratch`, musl-static, the CI machine |
| **iPadOS** | `hosts/ipados` (M6) | wasmtime, **Pulley**, `signals_based_traps(false)` | scaffolded — no JIT, no runtime-AOT on iOS; WKWebView stopgap runs the browser port with full JIT today |
| **microVM** | (M11) | wasmtime over Firecracker/virtio, 9P-over-vsock | aspiration, research-first — the second OCI weight |

Platform constraints that shape everything: the browser needs COOP/COEP for
SharedArrayBuffer (that is `serve.mjs`'s whole job); iOS forbids JIT *and*
runtime-loaded AOT (hence Pulley); iPad-Safari's OPFS world is private and
evictable, so real user files arrive only through granted subtrees
(a security-scoped bookmark **is** a bind).

## Where everything lives — the canonical namespace

The boot namespace, as init assembles it today (and as `/lib/namespace` will
declare it after M2). Mount points are prefix-map entries and need not exist
in any underlying tree (`/proc` has no directory in the rootfs; the bind is
the directory):

| path | served by | contents |
|---|---|---|
| `/` | ramfs (`#M`), seeded from `rootfs/` | V10 permissions enforced |
| `/bin` | seed | the Plan 9 userland — rc, sam, acme, the twenty-four, the harnesses |
| `/rc` | seed | rc's library and `tests.rc`, the shell half of the suite |
| `/lib` | seed | `font/` (real subfonts + `*default*`), `python3.14/` (the measured 21-file stdlib), `alt/` (union-test fixture) |
| `/etc` | seed | personality-side files as they land (`/etc/passwd` is future, personality-owned) |
| `/tmp` | seed | scratch; wiped per boot |
| `/v10/bin` | seed | the V10 exhibit — TUHS-tape `cat`, `echo` on `libv10` |
| `/mnt` | convention | parking for `mount` targets (`/mnt/profile` is the profile's decided seat) |
| `/dev` | `bind #c` | `cons`; a window replaces it wholesale — `bind '#w/N' /dev` makes a namespace a window (`cons ctl mouse wctl label rgb draw/…`) |
| `/env` | `bind #e` | environment as files |
| `/fd` | `bind #d` | dup by open |
| `/srv` | `bind #s` | posted channels, alive by name |
| `/proc` | `bind #p` | status, ctl (identity transitions — [identity.md](identity.md)), notes |
| `/net` | — | **does not exist yet** (M7); its absence is what the suite's future network tests will probe |

Where the pieces live in the **repository** is the tree in
[implementation.md](implementation.md) — one copy, there.

## The deployment review ledger

Doctrine #5's cadence: a dated, honest entry after each shipped form, and at
least when a milestone changes the story. Amoeba and Plan 9 died of their
deployment wave, not their kernels; the ledger exists so ours is examined on
the record, not assumed. Each review also asks the **external-goods
question** ([virtue-ethics.md](virtue-ethics.md)): has any external good —
attention, adoption, a store's approval — begun steering a decision that
belongs to the practice's internal goods?

### Review 2026-08-29 (at declaration — the baseline)

**The wave we are betting on**: the browser tab, the laptop app, the
container, and the agent sandbox — the claim is that these are this era's
workstation, as the diskless terminal was 1990's.
**What has actually shipped**: developer artifacts only — three green hosts
behind `bash`/`cargo` commands. Zero end-user forms. The bet is entirely
ahead of the evidence, which is the honest reading of a baseline.
**Nearest tests of the bet**: M1 (does the container form find use beyond
CI?) and M3 (does the macOS app survive daily driving by its own author?).
**Standing risk, named**: the agent-sandbox form has the strongest
market-timing claim and the least built substance (it waits on M7–M9); if
the wave passes before `/net` lands, that is this project's Amoeba scenario.
**Next review**: after M1 and M3 land, or 2026-11 — whichever is first.
