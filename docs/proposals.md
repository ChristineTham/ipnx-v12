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

`#ec` holds `user=kitty`, and `init=` when `ipnx` is given a command. plan9.ini is **the stored answers to the questions
`boot` would otherwise ask**: `bootargs` is the default shown in the `root is
from (...)` prompt (`boot/boot.c:354`, *"create default reply"*), and
`nobootprompt` skips the question — *"Suppress the `root from` prompt and use
root as the answer instead"* (`plan9.ini(8)`). `user=` does the same for the
user prompt.

**This `boot` asks nothing.** One method — `#9/0`, `bootvirtio9p.c` entire —
no authentication, `rootdir` a `char*` in the file. So there is nothing for a
configuration to answer beyond the user, and that, rather than any missing
mechanism, is why `#ec` holds so little.

The open questions, in order:

1. **Does `boot` get a second method?** A host directory over `#9` is one.
   A different store, a read-only image, a namespace handed over whole — each
   would make `root is from` a real question.
2. **If there is something to answer, how does it arrive?** Plan 9's own
   second way in is the multiboot branch (`pc/main.c:49`): **the bootloader's
   command line, spaces turned into newlines, IS plan9.ini** — and `ipnx`'s
   argv is that command line. It is an alternative to the FAT file, not an
   override of it (`if(BOOTARGS[0] == 0)`), which is exactly our situation.
   `ipnx` also takes a command to run, so the two would have to share the
   line.

**Built:** the mechanism — `#ec` attaches, binds under `#e`, takes writes
from eve — and one answer: **the default user is `kitty`** (Christine,
2026-09-24: *"our default user is called kitty, not glenda"*), given as
`plan9.ini`'s `user=kitty` by the host (`plan9ini`,
`hosts/ipnx/src/lib.rs`). `eve` starts empty and `boot` writes
`#c/hostowner` from `$user`, as Plan 9's does (`bootauth.c:56`,
`pc/main.c:285`); Plan 9's fallback, `"glenda"`, is kept for a
configuration that names none.

**Two differences in how the mount driver sleeps** — proposed 2026-09-24
(RESEARCH §16.14). Built, and awaiting review, because each departs from
Plan 9's mechanism though not from what a program sees:

1. **A clunk waits for nobody.** `mntclunk` waits for `Rclunk`. Here the
   `Tclunk` is sent and the reply left for whoever reads the wire next,
   which drops it. The reason: this kernel has no stack per process, so a
   call that sleeps runs again from the top, and a clunk is made where a
   call cannot run again — a descriptor's last close, a process's exit.
   The alternative is a continuation for each such close.
2. **`Dev::incref`.** Plan 9 counts references on the `Chan`, and closes the
   device at the last; a channel here is a value, and each copy is closed
   on its own. `srvopen`'s *"incref(sp->chan)"* (`devsrv.c:135`) is
   therefore a call telling the device serving the posted channel that
   there is another reference — devpipe counts it in `qref`. `struct Dev`
   has no such function; the alternative is reference-counted channels
   throughout the kernel.

## Decided, and moved into the specs

**P7 — a package, a service, a template, a project, a profile** — proposed
and answered piece by piece on 2026-09-24, and endorsed the same day
(*"yet that's look very symmetrical, let's build"*). It is now
[packages.md](packages.md) and, since the same day, [projects.md](projects.md);
the names were made symmetric — `start.rc`, `start.env`, `pkg.cfg` — and
every `.cfg` made `ndb`. Of what that left open, decided the same day: a command
written in rc keeps its bare name; the installed lists stay files of their
own, not entries in `profile.cfg`; the lock is not in `project.cfg`; and
promotion is to the user by default, to the system with `su`. And `rcmain` ships in rc's package, keeping its name. A project's lock is `project.lock`. A `.env` is rc's own form, what `whatis` prints. The namespace file is `start.ns`.

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
