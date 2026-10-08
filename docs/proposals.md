# Proposals — designs awaiting review

**META — a REGISTER, not one of the six questions.** It holds proposed answers
to them — designs written but not reviewed — so that specs carry only what is
endorsed.

**Check what Christine has said — [verbatim.md](verbatim.md) — and then
`plan9/`, before writing anything here.** Most questions that look like design
questions are lookups: Plan 9 built this system and the source is in the tree.
Others she has already answered: a question about `boot` sat here from
2026-09-20, although on 2026-09-04 she had ruled the name out (*"we can't call
something /boot and refer to something other than a bootloader"*). A proposal
is for what neither answers.

## Open

**Go and Python** — proposed 2026-10-08 (RESEARCH §16.31, §16.34, §16.35),
for two of the conformance suite's lines that no phase builds. Two more
were proposed with them — a package mounted rather than installed, and a
toolchain arriving during a session — and withdrawn the same day, because
neither is needed: *"Surely the whole point of having a template is to
specify which packages need to be preinstalled … Opening a project
effectively opens a session with the right packages preinstalled. cd into a
project does not install anything, it just views the files"* (Christine).
That is projects.md's design already: opening a project's `project.cfg`
opens a window whose namespace has the project's packages bound.

1. **Go and Python run as WASI programs, under an existing WASI engine.**
   The direction is Christine's (2026-10-08): *"I think we need to aim to be
   as WASI native as possible. A package like go which is compiled as a WASI
   binary must be allowed to run as if it is on a vanilla WASI engine. It's
   only our native binaries that understand our conventions."* Building a
   WASI engine of our own is the alternative, and *"that is risky"*. Go's
   wasm ports are `js/wasm` and `wasip1/wasm`, and CPython's wasm build is a
   WASI build, so both already exist as WASI programs, unmodified (RESEARCH
   §9.5, §16.31).

   What remains open is which files such a program sees. Two engines answer
   that differently:

   - **A. Host directories, a pure WASI view.** `wasmtime-wasi`, the
     engine's current WASI, with the store's directories preopened (*"We
     can bind enough of the host filesystem for them to operate"*). It can
     show nothing else: its files are host directories, and nothing can be
     put in their place (RESEARCH §16.34). A WASI program then sees no
     `/dev`, `/proc`, `/env` or `/srv`, and none of the binds — which, in
     her words, it does not need.
   - **B. The process's namespace as files (recommended).** `wasi-common`,
     wasmtime's own implementation of WASI preview 1, is released with
     every wasmtime (39.0.2 beside ours; 46.0.3 is the latest). It takes its
     directories and files from the host, as objects the host writes
     (`WasiDir`, `WasiFile`; RESEARCH §16.35). So the host can preopen `/`
     as the process's own namespace, every open, read and stat made with
     the kernel's calls. The program is still an unmodified WASI program,
     run by wasmtime's WASI code; it simply finds `/dev` and `/proc` among
     its files. This answers *"Is there a way to expose /dev, /proc as
     files to wasmtime?"* Its calls can also wait (its `async` linking), so
     a WASI program waiting for input waits in the kernel like any other
     process, and a note reaches it.

   *Why B:* the files come from the kernel on every host. A WASI program
   therefore sees the same tree in the browser as in the terminal, sees the
   packages its project binds, and stays confined if its namespace confines
   it. Under A each host's storage is a different tree, and a WASI program
   sees a package only where it lies in that storage, not where a project
   binds it.

   *The cost of B:* `wasi-common` is the implementation its maintainers call
   legacy. They recommend `wasmtime-wasi`, and keep `wasi-common` because
   `wasi-threads` needs it. The file objects are ours to write.

   *In the browser, either way, an existing engine* (*"For the browser we
   just need to find a WASI engine"*); there is no wasmtime there. Two of
   them let the page supply the files:
   - `@bjorn3/browser_wasi_shim` (0.4.2, MIT or Apache-2.0): a file is a
     subclass of its `Fd`, which maps one to one onto a kernel descriptor.
     Quiet since June 2025.
   - `@tybys/wasm-util` (0.10.4, MIT, released 2026-09-13): the files are an
     object in Node's `fs` shape.

   For A they would serve the page's storage; for B the kernel's files, as
   natively. Others are listed in RESEARCH §16.35.

   *Either way:*
   - A WASI program's standard streams are its process's descriptors 0, 1
     and 2 — both engines let a host supply them — so pipelines and the
     console work.
   - Its arguments are `exec`'s, and its environment is the process's.
   - It cannot start a process, because WASI has none (on `wasip1`, Go's
     `StartProcess` returns `ENOSYS`). A program built with Go or C is one
     standalone file, and runs. What cannot run is *building* one inside
     the system: the command a person types is a driver that starts the
     other tools as separate programs. `go build` starts `compile` for each
     package and then `link`; `clang` starts the linker, `wasm-ld`.
   - `pip install` cannot reach the network: WASI preview 1 has no outbound
     sockets.

   *Not proposed:* WASI answered by the host with our own code — an engine
   of our own, which she calls risky — and porting Go and CPython to Plan 9
   for programs that already exist for WASI.

   *The check:* a Go program built with `GOOS=wasip1 GOARCH=wasm` prints
   what it printed before; Python starts, imports `json` from its library,
   and computes.

   **The questions:**
   1. Which files does a WASI program see: A, host directories (a pure WASI
      view), or B, its process's namespace through wasmtime's `wasi-common`
      (recommended)?
   2. Which browser engine: `@bjorn3/browser_wasi_shim` (recommended, for
      its one-to-one `Fd`) or `@tybys/wasm-util`?
   3. Building inside the system — the suite's line *"build a program with
      a language toolchain, and run it"*, the demo's `cc hello.c` and `go
      run hello.go` — needs the driver to be a native program, which can
      start others (`rfork` and `exec`). *Proposed:*
      - **C:** Plan 9's own shape, `pcc`, which runs the preprocessor, the
        compiler and the loader as three programs (`cmd/pcc.c:172`, `:179`,
        `:192`, through `doexec`'s `fork` and `exec`, `:213`–`:226`). A
        native `cc` in that shape would run the real `clang -c` and the real
        `wasm-ld`, each a standalone WASI program.
      - **Go:** its driver is `go` itself, which does far more than start
        two tools — it resolves packages and modules and keeps a build
        cache — so a small native stand-in would be a cut-down. The real
        `go` needs **Go for Plan 9 on wasm**. Go names a target by system
        and processor: it has `plan9/386`, `plan9/amd64` and `plan9/arm`,
        and `js/wasm` and `wasip1/wasm`, but no `plan9/wasm`. Adding it
        means joining Go's own Plan 9 runtime and system calls — the calls
        this kernel answers — to Go's own wasm code generator. Go programs,
        and `go` itself, would then be native programs here, like the C
        commands. The cost is a change to Go's runtime that we would carry
        ourselves, unless Go took it.

      Should these be built, and when: now, after the demo, or not at all?

**The wasm32 `Ureg`, and APE's signal trampoline** — proposed 2026-09-25.
Built, and awaiting review, because each is a machine-dependent file Plan 9
has for every architecture and this one's cannot be 386's:

1. **`wasm/include/ureg.h` and `wasm/include/ape/ureg.h`** hold two words,
   `pc` and `sp`. Every architecture has a `Ureg` — the registers a trap
   saves (`386/include/ureg.h`) — and libap's `plan9/lib.h` includes it.
   This machine has no register set a program can see, and the machine
   hands a note handler nil (`libc/wasm/main9.c`). The two words are the
   two a jmp_buf holds here (`wasm/include/u.h`): a function-table index
   and the `__stack_pointer` global. The alternative is an empty-in-effect
   struct with 386's names, which would compile code that could not work.
   `aux/vmware` reads 386's registers by name and is not for this machine
   either way.
2. **`ape/lib/ap/wasm/notetramp.c`** calls the signal handler inside the
   note and leaves with `NCONT`. 386's points the Ureg's pc at `notecont`
   and leaves with `NSAVE`, so the handler runs outside the note, then
   `NRSTR` (`ape/lib/ap/386/notetramp.c`). With no Ureg to point, the
   handler runs where the note found it. `siglongjmp` from a handler is
   recorded and made by the interrupted call's stub on its way back, as
   `libc/wasm/notejmp.c` already does for `notejmp`.

**The `template` command** — proposed 2026-09-29, built, awaiting review.
The design says what instantiating a template does (packages.md, *A
template*) and names no command for it. Built as
`template instantiate [-s|-u] name dir` and `template remove [-s|-u] name`
— the design's own verbs — with `template.cfg` in ndb: `template=<name>
version=<v>`, and `packages=`, `services=` and `include=` lines, the last
instantiated first. The project it makes gets `project.cfg`
(`project=<dir's name> version=0.1 date=<today>`, then a `template=` line
for each template and the packages and services). The alternative is a
verb of `pkg`'s.

*A proposal is written here, reviewed, and then **leaves**. Adding to this file
instead of emptying it is how stale blocks accumulate and how a reader ends up
re-reading settled material to find what actually needs them.*
