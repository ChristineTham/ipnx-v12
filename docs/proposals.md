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
   therefore sees the same tree in the browser as in the terminal, and sees
   the packages its project binds. Under A each host's storage is a different tree, and a WASI program
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
     standalone file, and runs; Go's toolchain built for WASI cannot build
     one, because `go` starts `compile` and `link` (RESEARCH §16.36). The
     host's own toolchain builds, run as a host command — *"There no need to
     port the go and c toolchain to WASM when the host can do it so much
     better"* (Christine) — below, *Running host commands*.
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

**Running host commands** — what is open, proposed 2026-10-08 (RESEARCH
§16.36). Decided the same day, and now in [saranos.md](saranos.md), *The
host's resources*: a call to the host starts a host command; the program gets
its input, output and error back as descriptors and carries on; the host runs
the binary from its own file; its environment is the host's with `/env` laid
over it.

1. **A command is named as `execvp` names one**: a bare name the host finds
   on its `PATH` — the overlaid environment's, so the host's own unless the
   program sets one — and a name with a `/` in it resolved in the program's
   namespace, the kernel handing the host that file rather than its bytes.
2. **Its exit status, and stopping it, through a fourth descriptor.** Reading
   it waits for the command to end and gives its status, a non-zero code as
   its number, as APE does (`ape/lib/ap/plan9/_exit.c:27`–`:28`); writing a
   note to it signals the command, `interrupt` as `SIGINT` and `kill` as
   `SIGKILL` (APE's table, `signal.c:16`–`:36`). A Plan 9 process has the
   same two: its `wait` file, *"read to recover records from the exiting
   children"*, and its `note` file, whose strings are *"posted as a note to
   the process"* (`man/3/proc:163`, `:202`).
3. **Its directory is the program's current one**, which the kernel hands
   the host as it hands a binary. A directory the host does not serve has no
   host path, and there the call fails, saying so.
4. **rc's way in is Inferno's `os`**: `os [-d dir] cmd [arg …]` makes the
   call and copies between its own descriptors and the command's, as
   Inferno's does (*"Os copies the standard input to the remote command's
   standard input"*, `man/1/os`), and exits with the command's status. So
   `os go build | grep error` and `cat x | os sort` are ordinary rc, and rc
   does not change.

*What does not work, or not yet:*
- **How the host fills the descriptors** is to be built: the only host input
  the kernel takes today is the console's, at clock time (`kbdputcclock`,
  `devcons.c:556`).
- **No terminal.** The command's input and output are pipes, so one that
  wants a terminal — `vi`, `top`, Python's prompt — has none.
- **A WASI program cannot make the call**: unmodified, it knows only WASI's
  calls, and WASI has none that starts a process, so Python's `subprocess`
  fails under WASI (*Go and Python*, above).
- **The browser, the iPad and the iPhone have no host commands**: a page
  cannot start a process, and iOS does not let an app. There the call fails.

*The questions:*
1. Naming as `execvp` does — a bare name on the host's `PATH`, a path in the
   namespace handed over as the file — yes?
2. Exit status and stopping it through a fourth descriptor, read for the
   status and written with a note — yes?
3. Inferno's `os` as rc's way in — yes?

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
