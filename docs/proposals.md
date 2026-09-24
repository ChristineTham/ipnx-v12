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

## P7 — a package, a template, a profile

**PROPOSED 2026-09-24 — not reviewed.** Three separate designs, because they
are three different things (Christine: *"In my original concept they are
completely different"*). What each IS is hers, quoted; everything else here
is a proposal. Two Plan 9 mechanisms carry most of it, so each proposal says
where it uses them:

* **the namespace file** — `bind`, `mount`, `cd`, `.` to include another —
  which `newns` reads (`libauth/newns.c:114`) and `/lib/namespace` is written
  in; and
* **`$home/lib/profile`** — the rc script `init` has sourced at login since
  Plan 9 began (`init.c:178`: *"home=/usr/$user; cd; . lib/profile"*).

`/proc/<n>/ns` already prints a process's namespace in the first format
(`devproc.c`'s `Qns`), which is what makes *"save current namespace config as
a template"* a read of one file.

### A package

*"like a FreeBSD pkg or apt… a list files to be bound in the namespace, plus
potentially initialisation scripts (write out config files, set out
environment etc.)"*; installed *"to the system… to the namespace… or to the
user"*.

**Proposed:**

1. **Its files are in `/store/<name>/<version>`**, immutable once verified
   (*"a store entry never changes after verification"*).
2. **The package file is a namespace file** — its `bind` lines say where the
   store's files appear (`bind -a /store/go/1.23/bin /bin`) — **plus an rc
   script** for initialisation, run once at install.
3. **The three scopes are where the lines go:**

   | scope | the binds | the initialisation script |
   |---|---|---|
   | namespace | made now, in the calling process's namespace — shared with rc, as `bind` is | run now, its environment the caller's |
   | user | appended to the user's profile, so every login makes them | run now; what it writes lands in the user's files, and environment it sets goes into the profile |
   | system | appended to the system's profile (today `/lib/namespace`), so every user gets them | run as the host owner; writes land in the system's files (`/lib`, `/rc`) |

4. **Removing** takes the lines out of the profile they went into, and
   unbinds them now. What the script wrote is left — apt's `remove` — unless
   asked (apt's `purge`).

**Open:** where the store's bytes come from (a host directory, fetched by the
host, or `/net` once there is one); what *verification* is (a content hash,
a signature, and whose); what *pruning* keeps (anything some profile or
project names?); how the removal of a script's changes is known if the
script is arbitrary rc.

### A template

*"a prototype for a project (ie. a NodeJS project, a Python project) - it may
install packages, but contains project scaffolding"*; it *"instantiates new
versions of files (scaffolding), not just binds of files shared across
namespaces"*; it *"install packages into the current project, so is
persistent. opening a project ensures all packages are available."*

**Proposed:**

1. **A template is a directory** under `/template/<name>`: the scaffolding
   (`package.json`, `.gitignore`, editor settings, sample code), and a list of
   the packages it installs.
2. **Instantiating copies the scaffolding** into a new project directory and
   writes the project's own list of packages into it — the project's, from
   then on, to change.
3. **Opening a project** installs every package on its list to the namespace
   — the namespace scope above — so they are available for as long as the
   project is open, in the processes working on it, and nowhere else.

**Open:** what *opening* is on the command line (a command run in the
project's directory? `cd`?); where new projects go (`platforms.md` proposed
`/home/project/<name>`, unreviewed); whether a template may include another
(a namespace file's `.` would allow it).

### A profile

*"how a user wants their namespace organised, user config files, environment
variables, login scripts etc."*; it *"may be built from a template, but
essentially once it is instantiated it belongs to the user"*, or from *"user
hand editing config files"*; and *"The user can save current namespace config
as a template for future profiles"*.

**Proposed:**

1. **A profile is Plan 9's `$home/lib/profile`** — an rc script, the user's to
   edit — with a namespace file beside it for the binds, which the script
   applies. `init` sources it at login, as Plan 9's does (`init.c:178`); ours
   does not yet.
2. **Built from a template** is instantiating one into the user's home;
   **by hand** is editing the files; both are the user's afterwards.
3. **Saving the current configuration as a template** writes
   `/proc/<pid>/ns` as the template's namespace file, and the environment
   (`/env`) as the script's assignments.

**Open:** whether there is a separate system profile beside `/lib/namespace`
and `/rc/bin/termrc`, or whether those ARE it (*"alter the system's
environment (/rc, /profile)"*); whether `/home` exists as `platforms.md`
proposed or `$home` stays `/usr/$user` as Plan 9's is; and identity itself —
`su`, users, logins — which is not built, and which the user and system
scopes both need.

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
