# Proposals — designs awaiting review

**META — a REGISTER, not one of the six questions.** It holds proposed answers
to them — designs written but not reviewed — so that specs carry only what is
endorsed.

**Check `plan9/` before writing anything here.** Most questions that look like
design questions are lookups: Plan 9 built this system and the source is in the
tree. A proposal is for what Plan 9 genuinely does not answer. Everything that
was in this register on 2026-09-18 turned out to be answered at file and line.

## Open

Nothing.

## Decided, and moved into the specs

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
