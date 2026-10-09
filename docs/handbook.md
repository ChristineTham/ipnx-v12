# The handbook

**Role: a *how* — the practice.** What to do, on a fresh machine and every day
after. The *plan* is [implementation.md](implementation.md).

## Prerequisites

A Rust toolchain; **wasi-sdk** at `~/.local/opt/wasi-sdk` (override with
`WASI_SDK`), for its wasm backend and nothing else — no wasi is linked, and a
built binary imports exactly the calls in `userspace/sys/src/libc/wasm/sys.c`;
**Binaryen** at `~/.local/opt/binaryen` (override with `BINARYEN`), whose
`wasm-opt --asyncify` gives every image `setjmp`, `longjmp` and `fork`
(RESEARCH §16.12); **bison**, for rc's grammar; and **Python 3**, for
`userspace/kencc.py`, `mkfile.py` and `weaken.py`.

And the two Plan 9 trees, which are gitignored and never built or edited:

```bash
git clone --depth 1 https://github.com/0intro/9legacy plan9
git clone --depth 1 https://github.com/plan9foundation/plan9 plan9-stock
```

`plan9/` is the one a claim is checked against. `plan9-stock/` exists so a
difference between 9legacy and the final Labs release can be *attributed*.

## Build and run

**The userspace first**, because the tests run the real binaries and `cargo`
cannot build a wasm userspace:

```bash
bash userspace/mk.sh     # libc, the commands, rc  ->  userspace/root/pkg/system/<version>/wasm/bin
cargo test               # the kernel, the host, and the conformance suite
cargo run -p ipnx -- rc /bin/<script>.rc
cargo run -p ipnx -- echo hello   # boot; init runs it with rc -c, then the shell
cargo run -p ipnx -- os uname -a  # a command run on the host, through os
```

**A host command is typed through `os`** ([saranos.md](saranos.md), *The
host's resources*): `os -d <host directory> go build`. Without `-d` it runs
in the host directory the root is served from — `userspace/root`, or
`IPNX_STORE` — and a relative `-d` is relative to the directory `ipnx` was
started in, as Inferno's is to its emulator's. Its input is `os`'s, so give
one that takes none `</dev/null`, as Inferno's manual says.

**The browser host**, `hosts/web`, wants the `wasm32-unknown-unknown`
target (`rustup target add wasm32-unknown-unknown`) and Node 22:

```bash
sh hosts/web/build.sh                  # the kernel for wasm32, the page   ->  hosts/web/dist
node hosts/web/serve.mjs               # http://localhost:8080/, with COOP and COEP
node --test hosts/web/test/web.test.mjs   # the system under Node, typed at
node hosts/web/test/browser.mjs        # the console page in Chromium (Playwright)
node --test hosts/web/test/emca.test.mjs          # emca and its 9P, alone
node --test hosts/web/test/emca-system.test.mjs   # emca with the system, under Node
node hosts/web/test/emca-browser.mjs [check…]     # emca's page in Chromium
```

The site's page, `index.html`, boots into emca, the window manager
(`www/emca.mjs`, drawn by `www/surface.mjs`); `console.html` is the system
with no window manager — the console, and `rc` on it. emca's IPNX half is
rc scripts — `userspace/cmd/emca` and `emcaopen`, started by
`/service/emca` at login where the host is the page and serves `#9/1` — a
terminal's `#9/1` is its host's commands — and its types are
`userspace/type/`. `mk.sh` installs them; a change to one alone can be
copied into `userspace/root/pkg/system/*/rc/bin/` and the page rebuilt.

`hosts/web/dist/` is generated and gitignored; its `root` is a link to
`userspace/root`, so the page serves what `mk.sh` built last.

`userspace/build/` and `userspace/root/` are generated and gitignored. Userspace
binaries carry no `.wasm` extension: exec walks the namespace for `/bin/echo`,
and a freshly built module is indistinguishable from a shipped one.

`cargo test -p conformance -- --nocapture` prints the distance to the demo:
a checklist of what the system can do, and how much of it is reached. **It
fails, and is meant to** — the suite asserts equivalence with the demo and
goes green when all twelve are reached, not before. `cargo test` therefore reports a failure on a
perfectly healthy tree; for day-to-day work run `cargo test -p ipnx-kernel`
and `cargo test -p ipnx`.

### CI

`.github/workflows/ci.yml` does exactly the above on every push: bison and a
pinned wasi-sdk 34.0 and Binaryen 132, `bash userspace/mk.sh`, then the kernel's tests, the
host's tests on the world just built, `RUSTFLAGS=-D warnings cargo build
--workspace --all-targets`, and the browser host — built with warnings
denied, its tests under Node, and the page in Chromium. Conformance runs with `continue-on-error` so its
count is visible without gating the branch.

**A green CI means the system builds and boots, not that it is conformant.**

### Three build flags are load-bearing

| | |
|---|---|
| `-fno-builtin` | clang's libcall recogniser otherwise rewrites `strlen`'s own body into a call to `strlen` (RESEARCH §9.4) |
| `-fms-extensions` | `port/pool.c` is written in kencc's anonymous struct members, and this is clang's name for them |
| `weaken.py` | the wasm backend has no common symbols at all, so rc.h's tentative definitions each become a strong definition and the link fails with a duplicate for every variable rc declares. It sets the weak bit in the `linking` section, keeping strong the one definition that has an initialiser. RESEARCH §11 has the measurements, including why `--allow-multiple-definition` cannot do it |

## The working rules

Christine's, recorded so a session that inherits no conversation still has
them:

- **When I say something is not working, that outranks "it works for me".**
  Reproduce it before explaining it.
- **Measure, don't assume.** A claim about Plan 9 traces to a file and line in
  `plan9/`. A claim about the code traces to a grep or a run you just did.
- **Never invent words.** Not for a thing she has not named, and not a second
  word for a thing already named. Everything is spec'd, proposed, or gap; do
  not build what is not endorsed.
- **Every deviation from Plan 9 needs her approval, and the default answer is
  no.** Find the counterpart in `plan9/` by file and line; if there is none,
  stop and ask. A structure can be Plan 9's in vocabulary and something else in
  mechanism.
- **Execute against the plan.** Find the phase in
  [implementation.md](implementation.md) before building; do not append steps —
  if a decision invalidates an earlier phase, say so and redo it.
- **README.md and demo/index.md are Claude-written**, like everything else.
  Christine edited a few paragraphs of README and has not reviewed the rest.
  Neither file is exempt from correction; fix what has gone false in them.
- **The kernel does not grow, and it is a subset of Plan 9's.** A design that
  adds to it is wrong on sight; do not argue against it on other merits.
- **Every edit you report must be verified on disk** — assert the anchor
  matched, grep the result. A print statement is not evidence.
- **Commit and push only when she says so.** Never deploy. Ask before anything
  outward-facing.

## Where things are written down

- A **finding** goes in [RESEARCH.md](../RESEARCH.md), with provenance.
- A **decision** goes in [design.md](archive/design-log-claude-written.md), dated.
- A **contract change** lands in [architecture.md](architecture.md) in the same
  commit as the code, because that document is present-tense by rule.
- **Build status** goes in [when.md](when.md) and nowhere else.

A commit message claims only what its diff contains, checked against the tree
rather than the intention.
