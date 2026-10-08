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

**The four conformance gaps** — proposed 2026-10-08 (RESEARCH §16.31). The
suite's four lines that no phase builds: a Go program, Python, a package
available without installing anything, and a toolchain that arrives during a
session. Neither Go nor Python is in either Plan 9 tree, so the first two are
genuinely open; the other two are mostly lookups.

1. **Go and Python run as their own WASI builds, and the host answers WASI's
   calls with the kernel's.** Go builds for Plan 9 and for wasm, but not for
   both at once: its wasm ports are `js/wasm` and `wasip1/wasm`. CPython's
   wasm build is a WASI build. So both already exist as WASI programs,
   unmodified (the earlier demo ran both, RESEARCH §9.5); something only has
   to answer WASI's calls.

   *Proposed:* the host answers them. A module that imports
   `wasi_snapshot_preview1` gets those imports from the host's machine,
   beside `sys`. Each one is made of the kernel's own calls, for the process
   making it:

   | WASI call | the kernel's |
   |---|---|
   | `fd_read`, `fd_write` | `pread`, `pwrite` |
   | `path_open` | `open` or `create`, from the process's `/` |
   | `fd_readdir` | a read of the directory |
   | `clock_time_get` | the clock `#c` keeps |
   | `random_get` | a read of `#c/random` |
   | `proc_exit` | `exits` |

   The program sees its process's namespace, a note ends it, and the kernel
   does not change. That is *"everything else is handled by host or
   userspace"* (2026-09-03). Where Plan 9 has nothing to answer with, such as
   a link or a rename across directories, the answer is WASI's *not
   supported*, never a substitute (RESEARCH §9.20).

   *What WASI cannot do:* it has no processes. On `wasip1`, Go's
   `StartProcess` returns `ENOSYS`, so a WASI program cannot run another
   program. In particular the real `go` command cannot run its compiler and
   linker. That limits item 3, not this one.

   *The alternatives:*
   - **Port Go to `plan9/wasm` and CPython to APE.** For Go, that joins its
     own Plan 9 runtime and syscall package to its wasm backend. Both then
     become Plan 9 programs that make only Plan 9's calls, `os/exec` is
     `rfork` and `exec`, and the real `go` command runs. The cost is two
     ports to write and carry.
   - **WASI as a library in userspace,** linked into each WASI program when
     it is packaged (Binaryen's `wasm-merge`), so the host's imports stay
     Plan 9's list. This is unproven: the program owns its memory, so the
     library has nowhere of its own to keep anything.
   - **wasmtime's own WASI (`wasmtime-wasi`), considered and not proposed**
     (RESEARCH §16.34). It is WASI for the host computer: its files are host
     directories, and that is the one part a host cannot replace. A program
     under it would not see its process's namespace, and a process confined
     by its namespace would reach the host directory. It would also wait
     outside the kernel, where a note cannot reach it, and it does not exist
     in the browser, which has no wasmtime. The proposal answers WASI in the
     same place, the host, but with the kernel's calls rather than the host
     computer's.

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
   `pkg install -n python &`. The shell is usable at once. The toolchain
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
   is ruled out (*"no cut-downs"*). The real `go` command needs item 1's
   first alternative, Go on `plan9/wasm`.

   *The check:* the system boots and answers at once, and later in the same
   session `python` runs.

   **The questions:**
   1. Should the host answer WASI's calls (item 1), or should one of the
      alternatives be used?
   2. Should the package file become a paq archive (item 2), or should it
      stay `mkfs -a` with a server written for it?
   3. Should `pkg install -n` mount while `-s` and `-u` copy (item 2)?
   4. The real `go` command needs Go ported to `plan9/wasm` (item 3): now,
      after the demo, or not at all?

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
