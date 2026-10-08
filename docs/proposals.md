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

**The four conformance gaps** — proposed 2026-10-08 (RESEARCH §16.31,
§16.34, §16.35). The
suite's four lines that no phase builds: a Go program, Python, a package
available without installing anything, and a toolchain that arrives during a
session. Neither Go nor Python is in either Plan 9 tree, so the first two are
genuinely open; the other two are mostly lookups.

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
   therefore sees the same tree in the browser as in the terminal, sees a
   package that item 2 mounts rather than copies, and stays confined if its
   namespace confines it. Under A each host's storage is a different tree,
   and item 2's mounted packages are invisible to WASI programs.

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
     `StartProcess` returns `ENOSYS`). A prebuilt Go program runs, but the
     real `go` command cannot build: that needs Go ported to `plan9/wasm`,
     or it stays out.
   - `pip install` cannot reach the network: WASI preview 1 has no outbound
     sockets.

   *Not proposed:* WASI answered by the host with our own code — an engine
   of our own, which she calls risky — and porting Go and CPython to Plan 9
   for programs that already exist for WASI.

   *The check:* a Go program built with `GOOS=wasip1 GOARCH=wasm` prints
   what it printed before; Python starts, imports `json` from its library,
   and computes.

2. **A package becomes available without installing: it is mounted from the
   repository and never copied.** Plan 9 does not install software: a tree
   is served, and you bind it in. Plan 9 also serves an archive as a tree
   without unpacking it. `tapefs`'s servers *"mount their contents
   (read-only) into a Plan 9 file system"*, and boot's `paq` method runs a
   whole root from an archive through `paqfs` (`boot/paq.c:58`). `paqfs`
   reads a block only when one is asked for.

   *Proposed:* `pkg install -n` — the namespace scope, *"only valid for
   current process"* — mounts the package's archive from the repository
   instead of copying it, and binds from the mount. Nothing is written into
   the tree. The scopes that last, `-s` and `-u`, still copy into `/pkg`, so
   the system's and a user's packages keep working when the repository is
   gone. The hash is checked as it is now, over the whole file, before
   anything is mounted.

   *What Plan 9 leaves open:* the package file is `disk/mkfs -a`'s archive
   (decided 2026-09-29; packages.md, *The forms*), and no Plan 9 server
   reads that format. Either:
   - **(recommended) the package file becomes a paq archive.** `mkpaqfs`
     writes it and `paqfs` serves it; both are Plan 9's and both are built
     here. It is compressed, checked block by block (`adler32`) and as a
     whole (SHA-1, with `-v`), and read a block at a time. A copy for `-s`
     or `-u` is then the mounted tree copied into `/pkg`, and `disk/mkext`
     is no longer needed.
   - **or keep `mkfs -a`** and write a server for it in `tapefs`'s frame
     (`tapefs/fs.c`, with a reader beside `tarfs.c`): a new program, in
     Plan 9's shape.

   The mount point needs nothing created either. Plan 9 serves `/n` with
   `mntgen`, which makes a mount point when one is walked to (`termrc:6`,
   `lib/namespace:20`); this system's `start.ns` does not run it yet.

   *The check:* after `pkg install -n hello`, `hello` runs and `/pkg` holds
   what it held before.

3. **A toolchain arrives during the session: a `pkg install` in the
   background.** Plan 9 answers the rest. rc's `&` forks without `RFNAMEG`
   (`rc/havefork.c:18`), so a background job shares its shell's namespace,
   and the shell sees each bind the job makes at the moment it is made.

   *Proposed:* the session's `start.rc` — the system's or the user's —
   starts its toolchains in the background, using item 2:
   `pkg install -n python &` — or, under item 1's A, `-u`, because a WASI
   program would see only host directories. The shell is usable at once. The toolchain
   appears whole when its install ends, because a single `bind` makes it
   appear. A command typed before then *"does not exist"*, as any missing
   command does. The check uses Python, because Python needs no second
   process.

   *Open, and P8's:* a process that copied its namespace before the
   toolchain arrived does not see it (`pgrpcpy`, `pgrp.c:128`), and rio
   gives every window's shell a copy (`rio/wind.c:1355`). So whether a
   window opened earlier sees the toolchain depends on whether emca's
   windows copy the session's namespace or share it.

   *Go's and C's own toolchains:* each runs other programs — `go` runs its
   compiler and linker, and `clang` runs `wasm-ld` — and no WASI program can
   (item 1). The earlier demo's `go` was a stand-in written for it, which
   is ruled out (*"no cut-downs"*). The real `go` command needs Go ported
   to `plan9/wasm` (item 1).

   *The check:* the system boots and answers at once, and later in the same
   session `python` runs.

   **The questions:**
   1. Which files does a WASI program see: A, host directories (a pure WASI
      view), or B, its process's namespace through wasmtime's `wasi-common`
      (recommended)?
   2. Which browser engine: `@bjorn3/browser_wasi_shim` (recommended, for
      its one-to-one `Fd`) or `@tybys/wasm-util`?
   3. Should the package file become a paq archive (item 2), or should it
      stay `mkfs -a` with a server written for it?
   4. Should `pkg install -n` mount while `-s` and `-u` copy (item 2)?
   5. The real `go` command needs Go ported to `plan9/wasm` (items 1 and 3):
      now, after the demo, or not at all?

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
