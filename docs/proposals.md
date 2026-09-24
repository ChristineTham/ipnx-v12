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

**PROPOSED 2026-09-24 — not reviewed.** Five separate designs, because they
are different things (Christine: *"In my original concept they are
completely different"*; *"services and packages should be different"*).
What each IS is hers, quoted (`verbatim.md`). **How** each works is taken
from Plan 9 and from existing package managers, as she asked — the research
is RESEARCH §16, measured on apt/dpkg, Cargo and `plan9/`, and every answer
below cites it. What is left open at the end is what the research cannot
decide.

### A package

*"like a FreeBSD pkg or apt… a list files to be bound in the namespace, plus
potentially initialisation scripts"*; *"like installing a toolchain or a
library"*; installed *"to the system… to the namespace… or to the user"*.

1. **Where it comes from — a repository is a file server.** Plan 9 fetches
   its distribution by mounting it (`9fs sources`, `replica/pull`; §16.3);
   Cargo names its repository in one file (`config.json`, §16.2). So a
   repository is named by the file server that holds it, mounted like any
   other — a host directory, or a network server once `/net` exists. Nothing
   downloads: `pkg` reads the repository's files over 9P.
2. **Its description** — apt's fields (§16.1): name, version, description,
   dependencies with version bounds, the packages it breaks; and the file.
   The repository's **index** lists every package so, with **the file's
   SHA-256**, as apt's `Packages` does.
3. **Verification** — apt's chain (§16.1): **the index is signed, and the
   index's hashes cover every package.** A package is copied into
   `/store/<name>/<version>` only if its SHA-256 matches the index, and the
   store entry never changes after (*"a store entry never changes after
   verification"*).
4. **Its files appear by `bind`** — lines in a namespace file, as
   `/lib/namespace` is written, which `newns` reads.
5. **Its scripts** — dpkg's four (§16.1): before and after install, before
   and after removal. Configuration it writes is listed as such and **left
   on removal, taken only on purge**, as `conffiles` are.
6. **The three scopes** are where the bind lines go: the calling process's
   namespace now; the user's profile (every login); or the system's
   (`/lib/namespace`, every user).
7. **Pruning** — apt's marking (§16.1): a package installed only because
   another needed it is marked so; a store entry that no profile, project or
   system configuration names — directly or as a dependency — may be
   pruned (*"the store must be prunable"*).
8. **A lock** — Cargo's (§16.2): where a package is installed persistently
   (a profile, a project), the exact version and its SHA-256 are recorded,
   so the same bytes are bound next time.

### A service

*"servers/daemons… system, user or project specific… into system starts a
server when system starts, configurable in /rc. In user, starts when user
logs in, terminates when user logs out. Project - starts when project is
opened, terminates when project is closed."* *"services installs daemons."*

1. **The daemon's programs are a package; the service is separate** —
   exactly Debian's split (§16.1): `redis-server` is the service,
   `redis-tools` the programs; `postgresql-common` holds PostgreSQL's start
   script, `postgresql-16` its programs. A service names the packages it
   needs and they are installed into the same scope.
2. **A service is one rc script** that starts its server and posts it in
   `/srv` — what `cpurc` does for `ndb/cs` (§16.3) and what an init script
   does for `redis-server` (§16.1).
3. **Its settings are a separate file the script reads** — Debian's
   `/etc/default/redis-server`, sourced at start (§16.1) — kept on removal
   as configuration.
4. **It runs in a namespace of its own, as its own user** — Plan 9's
   `listen` runs a service as `none` in a namespace made from a namespace
   file (`/lib/namespace.httpd`, §16.3); Debian runs `redis` as user
   `redis` (§16.1).
5. **Installed is present in the scope's service directory; disabled is
   Plan 9's leading `!`** (`!tcp515`, §16.3). Enabling, starting, stopping
   and disabling are four acts, as in dpkg's scripts (§16.1).
6. **When it runs:**

   | scope | starts | stops |
   |---|---|---|
   | system | at boot, from `/rc` — Plan 9 starts daemons from `termrc`/`cpurc` and, per machine, `/cfg/$sysname/…` (§16.3) | at shutdown |
   | user | at login, from the profile | at logout: *"hangup"* to the login's note group |
   | project | when the project's window opens | when it closes: *"hangup"* to the window's note group, as `rio` does (`wind.c:1111`) |

7. **No restart when it dies** — neither Plan 9's startup scripts nor
   Debian's init scripts restart a daemon (§16.1, §16.3).

### A template

*"a prototype for a project… it may install packages, but contains project
scaffolding"*; it *"instantiates new versions of files (scaffolding), not
just binds of files shared across namespaces"*.

A directory under `/template/<name>`: the scaffolding (`package.json`,
`.gitignore`, editor settings, sample code) and the packages and services
the project needs. **Instantiating copies the scaffolding** into a new
project and writes the lists into its project file — the project's from
then on. A template may include another, as a namespace file includes
another with `.`.

### A project

*"a template install packages into the current project, so is persistent.
opening a project ensures all packages are available."* *"a project is a
type that is instantiated when user opens project file in a new emca window.
Projects live in /project/x but the binding is user/process speccific."*

1. **A project is a window type** ([type.md](type.md)): opening its project
   file opens a new emca window whose namespace has the project's packages
   bound and its services started; closing the window hangs them up.
2. **The project file is a manifest and a lock** — Cargo's `Cargo.toml` and
   `Cargo.lock` (§16.2): the packages and services it wants, and the exact
   versions and SHA-256 of what was installed.
3. **`/project/<x>` is a binding, per user or process**: the files live
   under their owner's `/usr/<name>` as everything a Plan 9 user owns does,
   and each user binds the projects they have at `/project/<x>`. Two users
   binding different directories there have two projects; binding the same
   one, they share it.
4. **Each opening runs its own services**, in its window's note group — the
   only arrangement in which closing a window stops exactly what it
   started.

### A profile

*"how a user wants their namespace organised, user config files, environment
variables, login scripts etc."*; built *"from a template, or user hand
editing config files"*; the user *"can save current namespace config as a
template"*.

**Plan 9's `$home/lib/profile`** (§16.3) — an rc script, the user's to
edit, sourced at login by `init` — with a namespace file beside it for the
binds and a lock for the packages installed to the user. Built from a
template is instantiating one; by hand is editing it. Saving the current
configuration as a template writes `/proc/<pid>/ns` and `/env` into a new
template. The **system's** profile is Plan 9's: `/lib/namespace`,
`/rc/bin/termrc` and `/cfg/$sysname/termrc`.

### Open — what the research cannot decide

1. **Whose signature** the repository index carries, and where its keys
   live (apt's are keyrings on the machine; `/credentials` is proposed in
   `platforms.md`, unreviewed).
2. **The names of the three service directories** — Plan 9's `/bin/service`
   is `listen`'s, for network calls by port, so daemons need their own; and
   whether the system's are a directory or lines in `/cfg/$sysname/termrc`.
3. **Identity**: login, logout, `su`, a daemon's own user — none built, and
   the user scope and a service's user need them. Plan 9's terminal has one
   user, the host owner, and no `su`.
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
