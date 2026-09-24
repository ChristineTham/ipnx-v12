# Proposals — designs awaiting review

**META — a REGISTER, not one of the six questions.** It holds proposed answers
to them — designs written but not reviewed — so that specs carry only what is
endorsed.

**Check `plan9/` before writing anything here.** Most questions that look like
design questions are lookups: Plan 9 built this system and the source is in the
tree. A proposal is for what Plan 9 genuinely does not answer. Everything that
was in this register on 2026-09-18 turned out to be answered at file and line.

## Open

**What should `boot` ask, and what answers it?** — proposed 2026-09-20,
revised the same day (RESEARCH §13.2).

`#ec` exists and is empty. plan9.ini is **the stored answers to the questions
`boot` would otherwise ask**: `bootargs` is the default shown in the `root is
from (...)` prompt (`boot/boot.c:354`, *"create default reply"*), and
`nobootprompt` skips the question — *"Suppress the `root from` prompt and use
root as the answer instead"* (`plan9.ini(8)`). `user=` does the same for the
user prompt.

**This `boot` asks nothing.** One method — `#9/0`, `bootvirtio9p.c` entire —
no authentication, `rootdir` a `char*` in the file. So there is nothing for a
configuration to answer, and that, rather than any missing mechanism, is why
`#ec` is empty.

The open questions, in order:

1. **Does `boot` get a second method?** A host directory over `#9` is one.
   A different store, a read-only image, a namespace handed over whole — each
   would make `root is from` a real question.
2. **Is there a user to choose?** `eve` is a compile-time constant here where
   Plan 9 starts it empty and has `boot` write `#c/hostowner` from `$user`,
   defaulting to `"glenda"` (`bootauth.c:56`, `pc/main.c:285`).
3. **If there is something to answer, how does it arrive?** Plan 9's own
   second way in is the multiboot branch (`pc/main.c:49`): **the bootloader's
   command line, spaces turned into newlines, IS plan9.ini** — and `ipnx`'s
   argv is that command line. It is an alternative to the FAT file, not an
   override of it (`if(BOOTARGS[0] == 0)`), which is exactly our situation.
   `ipnx` also takes a command to run, so the two would have to share the
   line.

**Nothing here is built.** The mechanism is: `#ec` attaches, binds under `#e`,
and takes writes from eve.

## P7 — a package, a service, a template, a project, a profile

**PROPOSED 2026-09-24 — not reviewed.** Separate designs, because they are
different things (Christine: *"In my original concept they are completely
different"*; and of servers: *"services and packages should be different.
maybe we should use different specs for them"*). What each IS is hers,
quoted (`verbatim.md`); everything else here is a proposal. Two Plan 9
mechanisms carry most of it:

* **the namespace file** — `bind`, `mount`, `cd`, `.` to include another —
  which `newns` reads (`libauth/newns.c:114`) and `/lib/namespace` is written
  in. `/proc/<n>/ns` prints a process's namespace in it (`devproc.c`'s
  `Qns`);
* **rc scripts run at the right moment** — `/rc/bin/termrc` and `cpurc` at
  start, which is where Plan 9 starts its servers (`cpurc:9`, `:60`, `:66`:
  `ndb/cs`, `aux/listen`, `aux/timesync`), and `$home/lib/profile` at login
  (`init.c:178`).

**Not researched as asked.** *"Use existing package managers as an
inspiration for packages primitives"*: the manuals of FreeBSD `pkg`, apt/dpkg
and Homebrew could not be read on 2026-09-24 — this environment's network
proxy refuses those sites. RESEARCH §13 (2026-09-02) has what was read then:
Debian's control file, four maintainer scripts and `conffiles`; the FreeBSD
port's `Makefile`, `distinfo` and `pkg-plist`; Homebrew's single-file
formula. The package primitives below are drawn from that and are to be
checked against the manuals when they can be read.

### A package

*"like a FreeBSD pkg or apt… a list files to be bound in the namespace, plus
potentially initialisation scripts (write out config files, set out
environment etc.)"*; installed *"to the system… to the namespace… or to the
user"*, and it may *"modify a user's profile… It can also alter the system's
environment (/rc, /profile)"*. And it is **a toolchain or a library, not a
daemon**: *"a package should be like installing a toolchain or a library,
services installs daemons"*. A package does not start anything.

**Proposed primitives** (after RESEARCH §13's three formats):

| primitive | from | here |
|---|---|---|
| name, version, description | Debian `control`, the port `Makefile`, the formula | lines of the package file |
| dependencies | `Depends:`, `RUN_DEPENDS`, `depends_on` | packages installed first, into the same scope |
| payload | `data.tar`, the port's staged files, the bottle | a directory in `/store/<name>/<version>`, immutable once verified |
| where the payload appears | implicit in the archive / `pkg-plist` | **`bind` lines** — a namespace file |
| checksum | `md5sums`, `distinfo`, `sha256` | the store entry's verification — **open** |
| scripts | `preinst` `postinst` `prerm` `postrm`; `post_install` | an rc script at install and one at removal |
| configuration a user may change | `conffiles` | written by the install script into the scope's own files, never into the store; left by removal unless purged |
| a service | Debian's init scripts, a port's `rc.d` script, a formula's `service` block | **not in a package** — a service is installed on its own (below). Those three formats put a daemon's start script inside its package; here the two are separate |

**The three scopes** are where the lines go:

| scope | the binds | the install script |
|---|---|---|
| namespace | made now, in the calling process's namespace | run now |
| user | appended to the user's profile, so every login makes them | run now; environment it sets goes into the profile |
| system | appended to the system's (`/lib/namespace` today) | run as the host owner; writes land in `/lib`, `/rc` |

**Open:** where the store's bytes come from; what verification is (a hash,
a signature, and whose); what pruning keeps; how a removal undoes what an
arbitrary script changed.

### A service

*"servers/daemons… system, user or project specific… into system starts a
server when system starts, configurable in /rc. In user, starts when user
logs in, terminates when user logs out. Project - starts when project is
opened, terminates when project is closed."*

**Proposed:**

1. **A service is an rc script** that starts a server and posts it in
   `/srv` — what `cpurc` does for `ndb/cs` today — **installed on its own**
   (*"services installs daemons"*), to a scope. A service may need packages
   — PostgreSQL's server needs PostgreSQL's binaries — and installs them
   into the same scope.
2. **Starting is running the script at the scope's beginning:**

   | scope | starts | stops |
   |---|---|---|
   | system | at boot, from `/rc` (`termrc`, as Plan 9's servers are) | at shutdown |
   | user | at login, from the profile | at logout |
   | project | when the project is opened | when it is closed |

3. **Stopping is a note to a note group** — Plan 9's own way: when a `rio`
   window is deleted it writes *"hangup"* to the window's note group
   (`rio/wind.c:1111`), and when `rio` exits it posts *"hangup"* to every
   window's (`rio/rio.c:329`). A user's services run in the login's note
   group and a project's in the project window's, so closing either sends
   *"hangup"* and its servers end with it. The kernel has note groups and
   `notepg` already.

**Open:** what installs a service — `pkg` with a flag, or a command of its
own; what *logout* is (there is no login yet); whether a service is
restarted when it dies; whether `/rc` holds one file per system service or
lines in `termrc`; how a service's configuration is changed.

### A template

*"a prototype for a project (ie. a NodeJS project, a Python project) - it may
install packages, but contains project scaffolding"*; it *"instantiates new
versions of files (scaffolding), not just binds of files shared across
namespaces"*.

**Proposed:** a directory under `/template/<name>` — the scaffolding
(`package.json`, `.gitignore`, editor settings, sample code) and a list of
packages. Instantiating copies the scaffolding into a new project and writes
the package list into it, the project's from then on.

**Open:** whether a template may include another (a namespace file's `.`
would allow it).

### A project

*"a template install packages into the current project, so is persistent.
opening a project ensures all packages are available."* *"a project is a type
that is instantiated when user opens project file in a new emca window.
Projects live in /project/x but the binding is user/process speccific. for 2
different users, it could be two projects. Or alternatively two users share
a project."*

**Proposed:**

1. **A project is a window type** ([type.md](type.md)): opening a project
   file opens a new emca window whose manager makes the project's namespace
   — its packages installed to the namespace scope, its services started —
   and closing the window ends them (*"hangup"*, above).
2. **`/project/<x>` is a name in a namespace, not a place on disk.** Each
   user's profile binds its own projects there; two users binding different
   directories at `/project/x` have two projects, and binding the same one
   share it — Plan 9's per-process namespace doing exactly what it is for.

**Open:** what the project file is (the list of packages and services, as a
namespace file?); where a project's files are when no one has it bound;
what two users sharing a project share of its services — one server, or
one each.

### A profile

*"how a user wants their namespace organised, user config files, environment
variables, login scripts etc."*; it *"may be built from a template, but
essentially once it is instantiated it belongs to the user"*, or by *"user
hand editing config files"*; and *"The user can save current namespace config
as a template for future profiles"*.

**Proposed:** Plan 9's `$home/lib/profile` — an rc script, the user's —
with a namespace file beside it for the binds, which the script applies;
`init` sources it at login, as Plan 9's does (`init.c:178`), which ours does
not yet. Built from a template is instantiating one; by hand is editing it.
Saving the current configuration as a template writes `/proc/<pid>/ns` and
the environment (`/env`) into a new template.

**Open:** whether the system profile is `/lib/namespace` and `/rc/bin/termrc`
or a separate `/profile`; and identity itself — `su`, users, login, logout —
which is not built, and which the user scopes of packages and services need.

## Decided, and moved into the specs

**The scheduler** — proposed and decided 2026-09-21, and it is now
[implementation.md](implementation.md)'s **P6**, before the registries,
because Christine put it there: *"we will need to implement before P6/P7."*

Two of its four questions came back as *"what does plan 9 do?"*, which is
the answer to a question that is a lookup, and both were. They are settled
in RESEARCH §14.1 and stated in the phase:

* **the suspend shape** — per-process kernel stacks (`Proc.kstack`, 4096
  bytes), so `sleep` leaves its locals where they are and `setlabel`
  returns 1 on the way back. Neither option the proposal offered;
* **the borrow rule** — `up->nlocks`: no lock held across a sleep, which
  here reads no `RefCell` borrow held across a suspension point;
* **preemption** — yes, and of user mode only. `hzclock` marks
  `up->delaysched`; the switch happens at `trap()`'s tail, `syscall()`'s
  tail, or `unlock`. An epoch check is compiled into guest code and never
  into a host function, so a yield lands exactly where Plan 9's would.

And the third question was hers to answer, and she did: **the browser
counterpart is a Node wasm supervisor and browser workers.** A worker is a
thread, so the switch there is a message and `Atomics.wait` rather than a
fiber — which is why the machine's method answers to `setlabel`/`gotolabel`
and to no engine's word for it.

**The window type system and the manager interface** — proposed and decided
2026-09-18, five questions answered in one reply. It is now
[type.md](type.md)'s *The design*: a type is a plumb rule, a manager is a file
server on a plumb port, one manager per window, the host half inside the
Saranos app, the root window with a manager like any other, and a declared
`verbs` list.

## Answered by reading `plan9/` — 2026-09-18

**Does a per-operation crossing to the host need batching?** No, and it is not
a protocol question. Plan 9 batches in **userspace**: `bufimage`
(`libdraw/init.c:453`) appends into a buffer whose size is `iounit(datafd)`
(`init.c:291`, falling back to 8000) and calls `doflush` when the next
operation will not fit. The library accumulates, the channel's I/O unit sets
the batch, and the kernel sees whole writes.

**What does a window device leave behind in the kernel?** Nothing — Plan 9 has
no window device. `rio` is an ordinary userspace file server: it posts its
channel to `/srv` (`rio/fsys.c:170`), mounts itself at `/mnt/wsys`
(`fsys.c:237`), and binds that over `/dev` with `MBEFORE` (`fsys.c:241`). A
window is a namespace, assembled by a program with `mount` and `bind`.

**What is a posted server called?** rio's own convention: `/srv/riowctl.%s.%d`
— name, user, pid (`fsys.c:152`). `#s` is one table for the whole kernel, and
the pid is what keeps a second instance from colliding.

**What answers `/` at boot, and how does `/dev` get filled?** `#/` is a fixed
table of two entries, `#/` and `boot` (`devroot.c:27`), plus whatever
`addbootfile` (`devroot.c:80`) embedded. Everything else is mounted by the boot
process, and `/dev` is assembled in the shell:

```
# bind all likely devices (#S was bound in boot)
for(i in f t m v L P u U '$' Σ κ)
	/bin/bind -a '#'^$i /dev
```

— `rc/bin/termrc:11–13`. Note `Σ` and `κ`: non-ASCII device letters are
ordinary in Plan 9, which is why `#¤` is not a special case.

*A proposal is written here, reviewed, and then **leaves**. Adding to this file
instead of emptying it is how stale blocks accumulate and how a reader ends up
re-reading settled material to find what actually needs them.*
