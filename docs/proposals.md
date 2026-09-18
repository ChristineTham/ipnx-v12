# Proposals — designs awaiting review

**META — a REGISTER, not one of the six questions.** It holds proposed answers
to them — designs written but not reviewed — so that specs carry only what is
endorsed.

**Check `plan9/` before writing anything here.** Most questions that look like
design questions are lookups: Plan 9 built this system and the source is in the
tree. A proposal is for what Plan 9 genuinely does not answer. Everything that
was in this register on 2026-09-18 turned out to be answered at file and line.

## Open

### The window type system and the manager interface

**PROPOSED 2026-09-18. Not agreed. Nothing here may be built.** Written because
Plan 9 has no counterpart — rio serves one kind of window and acme one kind of
content — so this is the one place in the documents where a lookup does not
answer. It is built from what Christine has said (quoted throughout) and from
Plan 9's nearest mechanisms, so that as little as possible is new.

#### What is already hers

> *"a window type is encapsulating things that are not text, that's why we need
> a manager, which understands how to render/edit the type, knows what to do
> with the status line, supplies toolbar buttons, etc."*

> *"emca is a window manager. it controls the placement of windows on the
> screen. a type manager controls what is in a window… type managers may
> communicate with window managers (over 9P of course)."*

> *"The default type is 'text' which you have been calling. But edit is really
> the manager of text."*

> *"the same directory can be an `ls` window or, if you want to edit the
> listing, an `edit` window"*

> *"a window's content is always a file, but the host may choose not to render
> it as a file but as an image, structured/formatted text, a table, etc."*

> *"file type not displayable, want me to display as text?"*

> *"may be commands outside manager, eg. a shell"*

#### The proposal, in one line

**A type is a plumb rule; a manager is a file server on a plumb port.**

Both halves already exist in Plan 9 and neither needs inventing.

#### Recognition — `file(1)`, unchanged

`file -m` names content from its bytes: `text/plain` for text
(`file.c:206`), `application/octet-stream` for binary (`:205`), and a table of
magic for the rest. No suffix decides. The type system adds no recogniser.

#### Dispatch — the plumber, unchanged

The plumber already does *"what is this, and who handles it"*. Its rules
language is 22 words (`plumb/rules.c`), first match wins, and a rule ends by
naming a port: `plumb to <port>` (`/sys/lib/plumb/basic`). Ports are files
under `/mnt/plumb` (`plumb/fsys.c:219`), and a program that reads its port is
the handler.

**So a type's name is a plumb port, and opening a window is a plumb.** Nothing
new is written to match content, choose a handler or deliver the request. A
type folder contributes rules in the plumber's own language.

`Plumbmsg` already carries what an open needs (`include/plumb.h`): `src`,
`dst` (the port — empty means "let the rules decide"), `wdir`, `type`, `attr`,
`data`.

#### The type — a folder of text files

Hers: *"A type is a folder of text files."* Proposed contents of `/type/<name>/`,
four files, each in a language that already exists:

| file | language | holds |
|---|---|---|
| `rules` | the plumber's (`rules.c`, 22 words) | how content becomes this type. Concatenated into the plumber's rule set |
| `manager` | one line | the program to start if no manager is listening on the port |
| `namespace` | `/lib/namespace`'s (`bind`/`mount` lines) | what a window of this type gets bound into it before the manager runs |
| `verbs` | one per line | what the toolbar offers, and what the manager will be sent |

Nothing declares *rendering*: **the host renders**, and what it renders is the
content file.

#### The manager — an acme-shaped file server

A manager serves one directory per window, and acme's per-window set
(`acme/fsys.c:76`) is the shape:

| file | |
|---|---|
| `addr` `data` | the content, addressed. acme's interface exactly; no new addressing |
| `ctl` | one verb per line, as `wctl` is (`rio/wctl.c:35`) |
| `event` | what the person did. A program reading this drives the window |
| `tag` | the window's own text — **editable**, as acme's is, not a fixed toolbar |
| `status` | what the window reports about itself (hers: the manager *"knows what to do with the status line"*) |

It posts itself at `/srv/<manager>.<user>.<pid>` — rio's convention
(`rio/fsys.c:152`), where the pid is what stops a second instance colliding.

#### The manager ↔ window manager channel

Hers: *"type managers may communicate with window managers (over 9P of
course)."* rio already defines that direction: twelve verbs written to a
window's `wctl` (`wctl.c:35`) — `new resize move scroll noscroll set top
bottom current hide unhide delete` — with thirteen parameters (`:68`).
**Proposed: use them, and add nothing until something needs it.**

#### Three consequences worth stating

**A type may have several managers, and one is default.** Hers: *"the same
directory can be an `ls` window or, if you want to edit the listing, an `edit`
window."* So recognition names a type, not a program. The plumber already
allows this — several rules may reach different ports from the same content,
and `dst` lets a caller name one.

**The fallback is offered, not silent.** Hers: *"file type not displayable,
want me to display as text?"* `text/plain` is where an unrecognised file lands,
and the person is asked rather than surprised.

**Some verbs are not the manager's.** Hers: *"may be commands outside manager,
eg. a shell."* A verb that no manager claims is a command, run as `Edit` runs
one in acme.

#### What this does NOT answer — for Christine

1. **Is a type a plumb port?** It makes the whole dispatch half free, but it
   binds the type system to the plumber, and a type with no plumb rule (the
   root window) then needs an answer.
2. **`inode/system` and the root window.** Hers: *"the '/' type and the screen
   is genuinely special, it is not a normal window. That's an unescapable
   fact."* Does the root window have a manager at all, or is it the
   compositor?
3. **Does a manager run per window or per type?** acme is one server for many
   windows; rio is one server for many windows. Per-type is the Plan 9 shape
   and is cheaper; per-window is simpler to reason about.
4. **Where does the host half of a manager live?** Hers: *"The manager may live
   on both the host and the IPNX side."* This proposal describes the IPNX
   half only. The host half — Monaco, TextEdit — has a mirror-buffer contract
   (*"we must notify emca of every edit, so essentially emca and the host are
   maintaining mirror buffers"*) that is not designed here.
5. **`verbs` versus the tag.** acme has no verb list: the tag is editable text
   and any word is executable. A declared `verbs` file is a departure, and it
   exists only because a toolbar has to be drawn from something.

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
