# Saranos — the operating system

> **PROPOSED — not reviewed.** Claude wrote this. Nothing in it is endorsed, and
> nothing in it approves a deviation from Plan 9. What is built is
> [when.md](when.md).

**Role: a *what* — the system's identity.** What Saranos is, what each layer is called,
and where the boundaries fall. The technical contracts are
[architecture.md](architecture.md); the windowing system is
[emca.md](emca.md); the dated decisions that produced these names are in
[design.md](archive/design-log-claude-written.md)'s log (2026-08-31, sharpened twice on 2026-09-01).

## What Saranos is

Not a barebones Unix reimagined, but **a whole operating system with its own
semantics, user interface and artifacts** (Christine, 2026-08-31). It is what
the system boots into on every surface — the browser page, the macOS app, the
iPadOS app.

## The three layers

**Saranos is the operating system** — the whole thing, and what someone would
say they are running. **IPNX is the kernel and the userspace** — the wasm side.
**emca is the windowing and UI system**, and it spans both sides by
construction: `emca` the program is on the wasm side, the surface that renders
its tree is the host's.

| Apple | here | |
|---|---|---|
| macOS | **Saranos** | the operating system: **host and wasm together** |
| Darwin | **IPNX** | the kernel and the userspace — the wasm side |
| Aqua | **emca** | the windowing and UI system — a half on each side |

## Why Saranos needs a name of its own

**Saranos is a symbiosis, and that is why it needs its own name.** It
encompasses the host side — the Rust host under wasmtime, the browser runtime,
the surface — *and* the wasm side. Neither exists without the other: the kernel
is wasm and cannot run without a host to give it workers, memory and a screen;
the host has nothing to do without the kernel. IPNX names only the wasm half,
which is exactly why a second name was needed rather than a qualifier.

## The host's resources

**Christine's, 2026-10-08:** *"The whole point is we are not trying to create
some of of artificial or synthetic guest environment isolated from the host.
We are in symbiosis with the host. Our wasm binaries can access host resources
and invoke host binaries"*; *"That's why I said we are not creating or
emulating devices."*

**Host commands are explicit, and some toolchains depend on the host**
(Christine, 2026-10-08: *"I think host commands should be explicit rather
than invisible. We should be explicitly acknowledging that some toolchains
depend on the hsot"*). A host command is always typed through `os` — `os go
build`, `os cc -c x.c` — and never under a name of ours: no host directory is
bound into `/bin`, and `go` and `cc` are not wrapped. Go's and C's toolchains
are the host's, so they are there where the host runs commands — the terminal,
the Mac — and not in the browser, on the iPad or on the iPhone.

**`os` is Inferno's** (*"Use inferno's os command as a guideline"*):
`os [-b] [-d dir] [-n] [-N level] cmd [arg …]` (`inferno-os`, `man/1/os`,
`appl/cmd/os.b`), without `-m mountpoint`, which names the device Inferno's
uses; here `os` makes a call to the host.

- **The call** — one, beside the system calls, decided the same day over
  running a host binary by `exec`. The host starts `cmd` as Inferno's emulator
  does, with `execvp` (`emu/MacOSX/cmd.c:79`): a bare name found on the host's
  `PATH`, or a path on the host — so the binary is always the host's own file,
  never a copy of its bytes. It runs in `dir`, a host directory — *"an error
  results and the command will not run if dir does not exist or is
  inaccessible"* (`man/1/os`) — or without `-d` in the host directory the root
  is served from, as Inferno's runs in emu's root (`emu/port/devcmd.c:291`).
  `-n` and `-N` lower its priority. The program gets the command's standard
  input, output and error back as descriptors, and a fourth that reads its
  status when it ends, as Inferno's `wait` file does, and carries on.
- **Its environment** is the host's own with the process's `/env` laid over it
  (*"yes (overlay)"*), each variable as APE makes one
  (`ape/lib/ap/plan9/_envsetup.c:15`). Here this departs from Inferno, whose
  command gets the emulator's environment.
- **Pipes run through it.** `os` copies its standard input to the command's
  and the command's output and error to its own (`os.b:99`–`:108`), so
  `cat x | os sort | wc` is ordinary rc. A command that takes no input is
  given `/dev/null`, as Inferno's page says: *"redirect os's input to
  /dev/null if there is no input to the command"* (`man/1/os`).
- **Its status is the command's.** `os` ends when the command's output does
  and exits with nothing when it succeeded, otherwise `host: ` and what the
  host reports — `exit: 1`, `killed`, `signal: 11` (`os.b:121`–`:136`;
  `oscmdwait`, `emu/MacOSX/cmd.c:198`–`:207`).
- **Stopping it**: *"If the os command is killed or exits … the host's own
  process control operations are used to (attempt to) kill cmd, if it is still
  running"* (`man/1/os`) — `SIGTERM` to its process group (`cmd.c:183`). So an
  interrupt that kills `os` kills the command. `-b` suppresses that, and leaves
  the command's input and output unconnected (`os.b:85`, `:91`).
- **No terminal**: the command's input and output are pipes, as they are in
  Inferno, so a program that wants a terminal — `vi`, `top`, Python's prompt —
  has none.
- **A WASI program cannot make the call**: unmodified, it knows only WASI's
  calls, and WASI has none that starts a process.

Not built; it is P9 of the plan ([implementation.md](implementation.md)),
which begins with a way for the host to fill the descriptors — the only host
input the kernel takes today is the console's, at clock time (`kbdputcclock`,
`devcons.c:556`).

## The names

Saranos is Sanskrit *śaraṇa*, refuge — Christine's reading: a refuge from the complexities of the modern
computing environment, a refuge for the PERSON, which is why it names the
system someone uses rather than the kernel underneath. A process also runs in
a refuge bounded by what it was given; one word, both layers.

emca is acme backwards. Note the symmetry that forced the
layering: XNU is "X is Not Unix" and IPNX is "IP is Not UNIX" — the same joke,
so the layer above wanted a human name rather than a second acronym, exactly as
Darwin did. **Dated entries in the records keep the words they were written
with**; only present-tense statements of what the system *is* carry these names.
