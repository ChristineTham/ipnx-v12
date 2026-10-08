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
     host's own toolchain builds, run as a host binary — *"There no need to
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

**Running host commands** — proposed 2026-10-08 (RESEARCH §16.36). Christine:
*"I want our wasm binary to be able to instantiate and run a host command"*,
*"Also be able to pipe in and out of host commands etc?"*, and the model it
follows from: *"we are not trying to create some … artificial or synthetic
guest environment isolated from the host. We are in symbiosis with the host.
Our wasm binaries can access host resources and invoke host binaries"*;
*"That's why I said we are not creating or emulating devices."* So there is
no device for it, no file to open and no command in front of it, and no
toolchain built for wasm: a host binary is run the way every binary is.

1. **`exec` runs it.** rc runs `go build` as it runs `cat`. The kernel
   already hands every image that is not a `#!` script to the host, which
   alone knows what it can run and refuses the rest with `Ebadexec`
   (`kernel/src/lib.rs`, `exec_read`; Plan 9 tests its own binary's magic,
   `sysproc.c:313`–`:316`, then `#!`, `:343`). The host knows its own
   binaries — ELF on Linux, Mach-O on a Mac — and starts one as a host
   process where it would have instantiated a module. No program changes;
   the kernel changes only as question 2 asks.
2. **Its standard input, output and error are the process's descriptors 0,
   1 and 2** — a pipe, a file, the console — so piping in and out is
   ordinary rc: `ls | sort`, `sort <x | uniq -c >y`, any of them the host's.
   The host copies between those and the host program's own, as Inferno's
   `os` does (*"Os copies the standard input to the remote command's
   standard input"*, `man/1/os`). The process stays a process — its pid,
   its notes, its exit status — so `$status`, `&&` and `||` work, and an
   interrupt reaches the host program.
3. **What has to be translated, Plan 9 already translates for a Unix
   program** — APE, its layer for running them:
   - the environment from `/env`, a file to a variable, the zero bytes
     that separate a list made ones (`ape/lib/ap/plan9/_envsetup.c:15`,
     `:82`–`:84`);
   - the exit status: a non-zero code is its number as a string, zero is
     success (`_exit.c:27`–`:28`);
   - notes as signals: `interrupt` is `SIGINT`, `hangup` `SIGHUP`, `kill`
     `SIGKILL` (`signal.c:16`–`:36`).
4. **Its files are the host's.** It starts in the process's directory,
   which is a host directory when it is on the store — the home, a
   project. The host serves its own root as it serves the store, for a
   profile to bind — Inferno's `#U*`, *"the root of the host system"*
   (`man/3/fs`) — and a profile adds the host's directories of programs to
   `/bin` with `bind -a`, so a name in both is ours and a name only the
   host has is the host's: `ls` is IPNX's, `go` the host's.
5. **The precedent is WSL's**: *"WSL can run Windows tools directly from
   the WSL command line"*, and *"ipconfig.exe | grep IPv4 | cut -d: -f2"*
   (`MicrosoftDocs/WSL`, `WSL/filesystems.md:115`, `:129`). Plan 9 has no
   host to run anything on. Inferno, which has, made it a device, `cmd(3)`
   — which is the model this is not.

*What does not work, or not yet:*
- **The host needs the binary's own path, and `touser` is given its
  bytes.** A host program finds its files from where it is: a Mac
  program's libraries are named from it (`@executable_path`, dyld(1)), and
  `go` looks for its `GOROOT` above itself before it falls back to the one
  it was built with (`cmd/go/internal/cfg/cfg.go:519`–`:560`). Plan 9's
  exec keeps the image's channel (`attachimage(SG_TEXT|SG_RONLY, tc, …)`,
  `sysproc.c:530`), and its qid tells the server that served it which file
  it is. Question 2.
- **`PATH`.** By APE's rule the host program gets rc's variables, and rc's
  `path` is a list of IPNX directories, in lower case; a host program that
  finds another by `PATH` — `make` running `cc` — needs the host's.
  Question 3.
- **A host script's `#!` line names a host path** — `#!/usr/bin/env
  python3` — and the kernel reads `#!` itself and resolves the name in the
  namespace, where `/usr` holds the users' directories. `python3 script`
  works.
- **No terminal.** The host program's input and output are pipes, so one
  that wants a terminal — `vi`, `top`, Python's prompt — has none.
- **`exec` reads the whole image first** — 14.3 MB for `go` — before the
  host sees that it is one of its own.
- **A WASI program cannot run one**: WASI has no way to start a process, so
  Python's `subprocess` fails under WASI, even for a host binary (*Go and
  Python*, above).
- **The browser, the iPad and the iPhone have no host binaries**: a page
  cannot start a process, and iOS does not let an app. There, `exec` of a
  host binary is `Ebadexec`, as for any image the host cannot run.

*The questions:*
1. A host binary run by `exec`, like any other — yes?
2. Its path: give `touser` the image's channel as well as its bytes, as
   Plan 9's exec keeps it (recommended; a change to the kernel's interface
   to the host), or run a copy of the bytes (no change; a program that
   looks beside itself does not find what it looks for)?
3. Its environment: the host's own with the process's `/env` over it, each
   variable by APE's rule (recommended — `x=y cmd` reaches the program, and
   `PATH` stays the host's unless set), or `/env` alone, as APE has it?

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
