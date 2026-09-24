# Window types and their managers

> **MIXED.** *The design* below is **decided** — Christine answered its five
> open questions on 2026-09-18. Everything after it is **proposed**: Claude
> wrote it, it is not endorsed, and it approves no deviation from Plan 9. What
> is built is [when.md](when.md).
>
> The device letters below are not Plan 9's: `H`, `Z` and `R` name no device;
> `V` is the TV capture device (`plan9/sys/src/9/pc/devtv.c`) and `w` the
> watchdog (`plan9/sys/src/9/port/devwd.c`).

**Role: a *what* — the type system.** What a window's content *is*, and what handles it.
Separate from [emca.md](emca.md), which is the windowing system and user
interface — emca composites windows and renders furniture; this document says
what fills them.

---

## The design — decided 2026-09-18

**A type is a plumb rule; a manager is a file server on a plumb port.** Both
halves are Plan 9's, so almost nothing here is new.

### Recognition — `file(1)`, unchanged

`file -m` names content from its bytes: `text/plain` for text (`file.c:206`),
`application/octet-stream` for binary (`:205`), magic for the rest. **No suffix
decides**, and the type system adds no recogniser of its own.

### Dispatch — the plumber, unchanged

**A type's name is a plumb port, and opening a window is a plumb.** The
plumber already answers *"what is this and who handles it"*: 22 words of rules
language (`plumb/rules.c`), first match wins, a rule ending `plumb to <port>`
(`/sys/lib/plumb/basic`), ports as files under `/mnt/plumb`
(`plumb/fsys.c:219`). `Plumbmsg` already carries what an open needs
(`include/plumb.h`): `src`, `dst` — the port, empty meaning *let the rules
decide* — `wdir`, `type`, `attr`, `data`.

Christine, on binding the type system to the plumber this way: *"yes for now
until we find an issue."*

### The type — `/type/<name>/`, four text files

| file | language | holds |
|---|---|---|
| `rules` | the plumber's | how content becomes this type; concatenated into the plumber's rule set |
| `manager` | one line | the program to start when nothing is listening on the port |
| `namespace` | `/lib/namespace`'s `bind`/`mount` lines | what a window of this type gets bound into it before the manager runs |
| `verbs` | one per line | what the toolbar offers, and what the manager will be sent |

Nothing declares *rendering*: **the host renders**, and what it renders is the
content file.

**`verbs` is a declared list** (*"declare verb list"*) — and it is a departure
from acme, where the tag is editable text and any word is executable. It exists
because a toolbar must be drawn from something. The tag stays editable
alongside it.

### The manager — acme's shape, one per window

**A manager runs per window** (*"per window"*), not one server for many as acme
and rio do. It serves one directory, and acme's per-window set
(`acme/fsys.c:76`) is that directory:

| file | |
|---|---|
| `addr` `data` | the content, addressed. acme's interface exactly; no new addressing language |
| `ctl` | one verb per line, as `wctl` is |
| `event` | what the person did — a program reading this drives the window |
| `tag` | the window's own text, **editable**, as acme's is |
| `status` | what the window reports about itself |

It posts at `/srv/<manager>.<user>.<pid>` — rio's convention
(`rio/fsys.c:152`), where the pid is what stops a second instance colliding.

### Manager to window manager

*"type managers may communicate with window managers (over 9P of course)."*
rio defines that direction already: **twelve verbs** written to a window's
`wctl` (`rio/wctl.c:35`) — `new resize move scroll noscroll set top bottom
current hide unhide delete` — with thirteen parameters (`:68`). Use them, and
add nothing until something needs it.

### The host half

*"host half lives in saranos app (swiftui, browser app via node etc.)"* — so a
manager's rendering and editing half is part of the Saranos application on each
surface, not a separate IPNX-side program. Its contract with the IPNX half is
the mirror buffer: *"we must notify emca of every edit, so essentially emca and
the host are maintaining mirror buffers."*

### The root window

**It has a manager** (*"yes"*). `/` is `inode/system`, and its manager is a
manager like any other — which is separate from every window also being a
compositor (*"each window itself is a compositor that can further decompose
into windows"*). Being special in type does not make it special in interface.

### Three consequences

**A type may have several managers, and one is default.** *"the same directory
can be an `ls` window or, if you want to edit the listing, an `edit` window."*
Recognition names a type, not a program; several rules may reach different
ports from the same content, and `dst` lets a caller name one.

**The fallback is offered, not silent.** *"file type not displayable, want me
to display as text?"* Unrecognised content is `text/plain`, and the person is
asked.

**Some verbs are not the manager's.** *"may be commands outside manager, eg. a
shell."* A verb no manager claims is a command, run as `Edit` runs one in acme.

### Settled earlier, and unchanged

Type names are **MIME types**; `text/plain` is the default and the fallback;
`/` is **`inode/system`**; **`output` is not a type** — it is `text/plain`
under a path convention; emca hardcodes exactly **two** facts — that `/` is
`inode/system`, and that unrecognised content is `text/plain`.

**STILL NOT DEFINED — gaps, not proposals:**

- **`text/plain`'s remaining pieces.** Settled 2026-09-02: its roles are
  **`look`, `edit`, `properties`** — no `shell` (a store is not a channel) and
  no `manage` (it does not run, though `text/x-python` inherits these and
  **adds** `manage`, since the hierarchy carries capability). `Find` puts a
  cursor at every match, so **typing after a Find is replace-all and there is no
  Replace verb**; the status line must therefore show the **match count**, which
  is the first safety requirement for it. Escape collapses to a single cursor at
  the last match.

  > **Approved 2026-09-18, and it is a departure from acme.** acme's dot is
  > **one range** — `q0` and `q1`, two `uint`s on a `Text` (`acme/dat.h:177`);
  > `Look` moves that single dot, and replace-all is `Edit` with a substitution
  > (`edit.c:149`). Multi-cursor Find is ours, which is also why the match
  > count is needed: acme needs no such indicator because it never has more
  > than one cursor. What was already settled is only its
  *edges*: it is the fallback, it can never fail,
  non-UTF-8 content renders with escapes and opens read-only, and line endings
  are never rewritten. What it *is* — its content model, its verbs, its status
  line, what Find and Edit mean over it — is the next work.
- **The manager interface**, though it shrank considerably once emca's job
  became *"hand over a rectangle and a namespace, and stop"*
  ([window.md](window.md)). What is sketched below is marked as an unreviewed
  proposal and should not be built from.
- **What a type declares about BINDING.** `ns` as implemented is retired
  (decision log), but the need is real and now sharper: a manager populates its
  window by binding, so a type must be able to say what gets bound in. Not
  designed.
- **Every other type** — `inode/directory`, `shell`, `pkg`, and the template
  and project types — waits on `text/plain`.

## Why acme has one window type, and Saranos cannot

Acme has **one** window type — a text file — and that is not minimalism for its
own sake. It is acme adhering to the Unix philosophy that everything should be
text.

**But that philosophy is a fiction, and Unix never actually held it.** Unix
commands are binary executables. `/dev/kmem` is binary. `/etc/passwd` is a text
file, but it is a *structured* text file, which is a different claim. What Unix
really aims at is narrower and true:

> The system is generally **configurable** through text files, and commands are
> **oriented towards processing** text files.

**IPNX holds to that, and should** — it is the Unix layer.

**Saranos does not have to, and that is why it is a separate layer.** It is the
operating system for a world of user interfaces and rich media, and not
everything in that world is text.

This is the same fact that decided the compositor's unit — a picture has an
aspect ratio and no columns, so characters could not be the measure — and the
same fact that makes Undo belong to one type rather than all of them.

> **Acme's universals were artefacts of one content kind.** Text-only is what
> made characters a viable unit, Undo a universal builtin, and *"the window
> exposes a file"* a sufficient contract. They fail together, and the manager
> is what replaces the third.

## Type names are MIME types — and Plan 9 agrees

**`file(1)` already does this** (read 2026-09-18). `file -m` emits MIME types,
and three of this document's decisions are its behaviour rather than a design:

| | |
|---|---|
| **names are MIME types** | `file.c` carries the table — `text/plain`, `image/png`, `image/jpeg`, `application/pdf`, `application/postscript`, `audio/mpeg`, `application/x-elf-executable` and the rest |
| **`text/plain` is the fallback** | `char PLAIN[] = "text/plain\n"` (`file.c:206`), printed for short, ascii, extended-ascii and latin-ascii content (`:367–373`) and for an empty file (`:299`) |
| **recognition is from contents** | the whole of `file.c` is a magic-and-heuristics table over the bytes. A suffix alone never decides |

The binary fallback is `application/octet-stream` (`file.c:205`).

**Two names in this document are not Plan 9's.** `file -m` gives a directory
`application/octet-stream` (`file.c:284`) and a device file the same
(`:288`); in plain mode it says `directory` and `special file #%C/%s`. So:

| | |
|---|---|
| `inode/directory` | freedesktop.org's convention, not Plan 9's and not an IANA type |
| `inode/system` | ours |

**Both approved 2026-09-18.** `file -m` gives a directory
`application/octet-stream` (`file.c:284`), which carries no information, so a
directory needs a name of its own here. `inode/system` names the thing
Christine called out: *"the '/' type and the screen is genuinely special, it is
not a normal window. That's an unescapable fact."*

## What a type declares

| file | |
|---|---|
| `recognise` | how files of this type are identified (see below) |
| `managers` | the **roles** this type offers, one per line — `look`, `edit`, `properties`, `manage`, `shell`. Which resolves to what is per-type; the role name is stable everywhere, so the dropdown reads the same on every window |
| `verbs` | toolbar bindings, one per line |
| `README` | prose for whoever reads the registry; nothing parses it |

> **The default role is derived, not declared: writable → `edit`, not
> writable → `look`.** So "read-only by default" is literally true rather than
> a policy, and it is per-identity for free — an agent with no write permission
> gets `look` on everything, from the permission check the kernel already
> makes. "Why is this read-only?" is answerable with `ls -l`. Reorder it for
> yourself by binding your own `/type`.

**Configuration is text, which is the true part of the Unix philosophy — and
what it declares need not be text at all.** That distinction is the whole
design: the configuration is Unix, the thing configured is not.

Small files rather than one blob, for the reason that has held throughout: each
field is separately bindable, so a personal `/type` bound over the system's
overrides only the files you wrote. The override is a **union element**, not a
merge algorithm.

> **The type declares WHICH managers exist. The manager implements WHAT the
> verbs do.** `New`, `Find` and `Edit` are the *manager's*, because two managers
> of one type must differ — `Edit` under `look` cannot mean what it means under
> `edit`, on the same file. Only `Run`, `Open` and `Add` are universal. The six are **New, Open, Run,
> Find, Edit, Add** — always these, in this order, in every window.

### Verb bindings take three forms

Not two. A verb may **ask the manager**, **ask the surface** (`ipnx:` /
`host:`), or **run a command** — a type may bind a button to something outside
its manager entirely, such as a shell command.

A verb declares a **label and an action, never anything presentational**.
Appearance is the surface's ([surface.md](surface.md)), so a type names a
*standard verb* and each surface maps that name to its own iconography; a
non-standard verb renders as its label. The registry never ships an image.

## How a file's type is recognised

**emca tests for the type and falls back to `text/plain`.** The pipeline is
ordered by cost — and ordering by cost happens to order by certainty too:

| | signal | cost | certain? |
|---|---|---|---|
| 1 | **the serving device** — 9P's stat carries `type` (server) and `dev` (subtype) | free | yes |
| 2 | **the qid bits** — directory, symlink | free | yes |
| 3 | **an exact filename** — `/etc/passwd` | hash lookup | yes |
| 4 | **an extension** | hash lookup | a guess |
| 5 | **magic bytes** | one short read | a good guess |
| 6 | **fallback → `text/plain`** | — | never fails |

That bounds classification at **one stat and one short read**, which is the
answer to "emca may spend too long trying to figure out".

**Step 1 is a signal no desktop system has.** `/proc`'s entries are stamped by
the proc device in every stat, so *"not really an `inode/directory` — an
`inode/mount-point` shaped as a proc filesystem"* is recognisable without
sniffing anything. MIME says what content **is in form**; the serving device
says what it **means**.

### When recognition succeeds but nothing can handle it

**Refused, with a prompt** — *"file type not displayable, want me to display as
text?"* Falling through silently would show a correctly-identified PNG as
mojibake and look like a bug; refusing outright would make a recognised file
unopenable. The prompt is the honest middle, and it keeps `text/plain` as the
universal escape hatch without making it a surprise.

## `shell` — a conversation, and its backends

**`shell` applies to any file that can be appended to**, not only to channels.
`/dev/cons` is not special because it is a device: it is the canonical instance
of *a file two parties append to*. Opening a log with `shell` shows its history
and lets you add to it, which is `tail -f` and annotation as one thing.

**You do not choose a backend — you choose a file, and whatever is on the other
end is the backend:**

| open with `shell` | you are talking to |
|---|---|
| `/dev/cons` | the console |
| a pipe to `rc`, or to `python` | a shell, or a REPL |
| `/net/tcp/N/data` | the far end of a connection |
| a model endpoint file | an LLM |
| a log a daemon is writing | the daemon, and yourself |

So **adding a conversational backend is adding a file** — no new role, no UI
work, no plugin interface.

**Content is always renderable as a file; what `write` means depends on the
role.** `edit` changes the content; `shell` sends to the other end and the
transcript grows. The transcript **stays text** — markdown and code blocks are
drawn by the [surface](surface.md), never baked into the file — or selecting a
path in the output and pressing Open stops working, which was the point of the
transcript being a file. And **history needs no mechanism: the transcript is the
history.**

> **The unresolved cost is line discipline.** When output arrives while someone
> is typing, something must hold the partial line, redraw it below the
> interruption, and keep the cursor where the user thinks it is. It cannot live
> purely on either side — the host has the keystrokes, IPNX has the writer — and
> a model streaming token by token makes it harder than a shell emitting lines.

## The type is what the content *is*. The manager is what handles it

> **The default type is `text`. Its manager is `edit`.**

An earlier draft renamed the type to `edit` and lost the distinction. They are
different things:

| | |
|---|---|
| **A type** | says what a window's content **is** — text, a directory, an image, a process table. A declaration, living in `/type` as text. |
| **A manager** | says what to **do** with it: renders the content and edits it, drives the status line, supplies the toolbar's buttons, and gives meaning to Find, Edit and selection for that kind of thing. |

The type is the interface; the manager is the implementation.

### Managers live on either side

That is the symbiosis showing through rather than an inconsistency. **Monaco is
a manager over a file** — a host-side one.

| type | manager | side |
|---|---|---|
| `text` | `edit` — CodeMirror, Monaco, TextKit, whichever the surface has | host |
| `directory` | `ls` | either |
| image, video, PostScript | the surface's own renderers | host |
| `proc`, `pkg`, `usr` | programs that need the namespace | guest |

Which retroactively explains two rules that had looked like special cases.
**"Editing is the surface's"** was never a fact about text: it is that the
`text` type's manager happens to live host-side. **"IPNX implements no
renderers"** is the same fact for image and video managers. Both are one
principle — **a manager lives wherever it can do its job** — which is the only
rule a two-sided system permits.

## What the `text` manager's interface looks like today

**This is one manager's interface, not *the* manager interface** — the general
one is undefined (see the baseline above). The `text` type's is in the code,
built before it was named: the mirror protocol between the editor component and
emca.

- **up:** `insert`, `delete`, `select`, `dirty`, `seq <n> <hash>`
- **down:** `content`, `toolbar`, `tag`
- **`put`**, which notifies emca that the manager wrote the file and emca should
  re-read — *one writer per file*, settled in [design.md](archive/design-log-claude-written.md)

That is a manager over a file, declaring what it did and being told what to
show.

> **SUPERSEDED 2026-09-02.** This was fenced as an undesigned extrapolation
> until the **file interface** was designed and accepted: emca serves one
> directory per window at `/dev/window/`, and a manager reads and writes files
> in it ([window.md](window.md)). What follows is the earlier sketch, kept only
> because it names what a non-text manager needs — each item now has a file.

The sketch's candidates, and where each landed:

| | |
|---|---|
| **kind** | what the content is, so the surface knows how to render it and emca knows whether it has items at all |
| **items** | what Find selects and Pipe feeds — byte ranges for text, files for a listing, pids for `proc` |
| **verbs** | what the toolbar offers, which `window` already carries |
| **size** | intrinsic dimensions for the kinds with an aspect ratio rather than a line count — already the `size` event |

### Managers may have interfaces, not merely verbs

Acme could refuse them because every window was text and the file interface
carried everything. **That refusal does not survive rich media.**

What preserves property 1 — *any item can be a verb's operand* — is that **a
manager's interface is declared rather than drawn**: controls in a file,
rendered natively by the surface, so the items behind them stay addressable,
greppable and operable by anything else. The moment a manager draws its own
table, that window's contents stop being reachable by anything but that
manager. `/dev/canvas` remains the escape hatch for genuine drawing — the
exception, not the rule.

## Open — what remains

*Most of this list closed on 2026-09-02. What is recorded here is only what is
still genuinely undecided; the settled items are kept with their answers so a
reader is not left wondering whether they were forgotten.*

| the question | |
|---|---|
| **Can one type have several managers?** | **CLOSED.** Yes — that is what the five **roles** are, chosen per window from the title bar's dropdown, with the default derived from permissions |
| **`ns` as executable text** | **CLOSED.** `ns` is retired; a type declares bindings in its declaration and nothing in the registry is `eval`'d |
| **Per-verb invocation, or long-running?** | **OPEN.** A manager writable in six lines of rc is what makes *"adding a manager is adding a file"* nearly true; a long-running one can hold state and serve a computed body. `edit` is long-running, `manage` on `/proc` need not be |
| **How much interface may a manager declare?** | **OPEN.** The toolbar generalised — fields, toggles, lists — needs a vocabulary, and that vocabulary is the part most likely to grow without discipline. The file interface bounds it for now: whatever is not a file in `/dev/window/` cannot be declared |
| **Is the window's CONTENT a file too?** | **CLOSED.** Always — the host may *render* it as an image, a table or formatted text, but the file is the truth ([window.md](window.md)) |
| **`emcaopen`'s interface** | **CLOSED** — one argument, everything else derived; see below |

## The `/type` file syntax

*Reviewed and endorsed by Christine, 2026-09-02.*

```
/type/text/plain/recognise
    # one rule per line, tried in pipeline order. First match wins.
    device  #c              # signal 1: the serving device, from 9P's stat
    qid     dir             # signal 2: the qid bits — dir, symlink, stream
    name    /etc/passwd     # signal 3: an exact path
    ext     .txt            # signal 4: an extension
    magic   0 "%PDF-"       # signal 5: bytes at an offset
    fallback                # signal 6: claims anything unclaimed. text/plain only

/type/text/plain/managers
    look                    # the roles this type offers. First is NOT the
    edit                    # default — the default derives from writability
    properties

/type/text/plain/verbs
    # <label>  <action>, in exactly three forms
    Revert     manager:revert      # ask the manager
    Save       host:save           # ask the surface
    Archive    run:tar cf $file.tar $file   # run a command
```

**A `recognise` file carrying `qid stream` also means "never sniff me"** — the
pipeline must stop after signal 2 for channels, or classifying would consume
the first bytes of a conversation.

**Settled since this was written (archive/design-log-claude-written.md):** `run:` substitutes
**environment variables**, not a templating syntax — emca sets `$file`, `$dir`
and `$window`, and the selection needs none because `|` already pipes it.

> **STILL OPEN:** whether `magic` needs more than
offset-and-literal.

## `properties` as `edit` over a synthetic file

*Reviewed and endorsed by Christine, 2026-09-02.*

The proposal is that it needs no machinery, by the same argument that made a
directory's file manager disappear:

> **`properties` is `edit` over a text rendering of the file's metadata.**

```
name    notes.txt
mode    rw-r--r--
owner   kitty
group   kitty
size    4096
mtime   2026-09-02 14:22:07
qid     0x8a3f1c v3 file
served  a root file server
```

Change the `mode` line and Save, and `chmod` happens. Change `owner`, and the
`wstat` happens. Nothing is applied until Save; Save shows the plan; and you
can only change what you had permission to change anyway — the same four guards
as editing a listing.

It also makes provenance visible for the first time: **`served`** answers *"which
union element actually gave me this file?"*, which is asked constantly in a
namespace system and answerable by nothing today.

> **STILL OPEN:** whether `properties` is genuinely a separate role
or just `edit` on a different *view* of the same object — in which case the role
list is four, not five.

## pkg, template and project

**Moved.** What was here — a package as a single file, `/store` for
verified bytes, and the shapes of templates and projects — was superseded on
2026-09-24 and is in [`archive/type-packages-2026-09-04.md`](archive/type-packages-2026-09-04.md).
A package, a service, a template, a project and a profile are now P7's
proposals in [proposals.md](proposals.md).

## `inode/system` — the system manager, and what it declares

*Accepted by Christine, 2026-09-02; the layout file designed the same day,
closing a gap the lens review found.*

**`inode/system` is an ordinary type; the OUTERMOST WINDOW is the special
case** — its chrome belongs to the host ([window.md](window.md)). A nested emca
opens its own `/` with the same manager and ordinary emca chrome.

**And its manager owns a writable window as well as the composition.** A
container's manager renders by arranging, but this one also needs somewhere to
write, so it opens **one ordinary child** and writes there — conventionally the
first entry in `layout`, `/` itself, listing the root. The surface renders that
child natively: a **left pane** in the browser, a **collapsible sidebar** on
macOS and iPadOS. Collapsing it is `minimise` on that window.

Because `inode/system` **specialises `inode/directory`** — recognised by the
exact path `/` — the listing comes by inheritance, and the type *adds* `manage`
with `Halt`, `Reboot` and `New Shell`. Those sit on the **global toolbar or menu
bar**, since the outermost window's content is the system; the sidebar's own
toolbar carries the listing's verbs.

**The system manager is an ordinary program holding a window.** It needs no
special channel: it opens windows the way anything opens windows, and emca
places them. What it owns is *which children exist* and *how many columns the
root divides into*; emca owns what rectangles they get.

**Three policies, all text, all editable:**

```
/type/inode/system/layout         WHICH windows, and where — see below
/type/inode/system/breakpoints    how many columns at a given width
    # columns   minimum width, in CHARACTERS (measured, not pixels — the
    #           72-column measure keeps WCAG 1.4.4 by construction)
    1     0
    2     144
    3     216

/rc/emca                          starts emca — one line. The WINDOWS come
                                  from `layout` above, not from here
/profile/emca                     a person's own, if it should follow them
                                  across devices — same convention, named for
                                  the program on both sides
```

**Breakpoints belong to the type**, because `/type/<x>/` is where a type's
configuration lives — and that gives the override for free: `/type` is bindable
per-process, so your breakpoints are yours and an agent's are its own, with no
preference store and no new location.

> **Two collisions caught by Christine, and the rules they give:** an earlier
> draft called the startup file **`boot`** — which on Unix means the bootfile,
> the kernel image and the loader — and put it, with `breakpoints`, as **bare
> files in `/profile`** — a root defined
> an hour earlier as the *identity* profile (namespace fragments, services, key
> references). **Do not put configuration into a root that means something
> else**, and **do not coin a name Unix already uses**. A namespaced subtree
> under a root is fine; bare files in it are not. The startup file needs no new
> word at all: `/rc/emca` and `/profile/emca` are named for the program, the
> same convention on both sides.

**Resize needs no new mechanism.** The manager does a blocking read on
`/dev/window/rect`; when it returns, it consults `breakpoints`, and if the
column count should change it writes `leaves <n>` to `/dev/window/ctl`. emca
re-divides the root and reallocates. `Reset` is the same act performed on
demand.

This keeps `leaves = cols / 72` as **data rather than code** — today it is
`convention()` inside `emca.c`, which is a system layout decision compiled into
the window manager.

> **DECIDED at the stated lean, 2026-09-02:** whether `/rc/emca` re-runs on a large resize (it would need to
> be idempotent, or the manager would have to diff), or runs only once and
> resize touches nothing but the column count. The second is simpler and matches what
> M15e built.

## The `shell` type — and where line discipline lives

*Accepted by Christine, 2026-09-02.*

### Line discipline is the SURFACE's, by the same rule as IME composition

When output arrives while someone is mid-line, something must hold the partial
input, redraw it below the interruption, and keep the cursor where the user
thinks it is. **The proposal is that the surface does it**, because it already
does the identical thing for input-method composition:

> **Only committed text crosses.** A half-typed line is the same category as
> marked text — uncommitted, host-side, invisible to IPNX. On Enter it commits:
> the line goes to the channel and appends to the log.

An IPNX-side line discipline would require uncommitted input to cross, which
contradicts a rule already settled for IME.

### Raw mode is one word, not a struct

`vi`, `less` and every ncurses program need character-at-a-time input with no
local editing. Plan 9's answer is `/dev/consctl` and the word `rawon`, and it is
the whole of what termios provides that anything here needs:

```
echo rawon  > /dev/window/ctl     every keystroke crosses immediately;
                                  no local echo, no local editing
echo rawoff > /dev/window/ctl     the surface holds the line again (default)
```

### The window

| | |
|---|---|
| **the log** | a real file, append-only from outside — the same object `/output/<n>/log` holds when a run has finished. `look`'s behaviour applies to it: select a path in a build error, press Open |
| **the input** | at the end, in the body — *not* the tag line, which operates on the window (Find in the log, Open a path you can see) |
| **verbs** | `Interrupt` — sends a note to the process. Nothing else obviously earns a place: `Clear` would truncate the log, which is content, and Save/Revert/Undo are meaningless |

> **DECIDED at the stated lean, 2026-09-02:** whether a model streaming token-by-token can use the same
> mechanism. Cooked mode assumes output arrives in lines; a response arriving
> mid-word for many seconds while the user types is the same problem under more
> pressure, and may need the surface to hold input across a longer interruption
> than a terminal ever does.

## `inode/directory` — the listing format

*Accepted by Christine, 2026-09-02.*

**One name per line, and nothing else.**

```
README
notes.txt
project/
```

Metadata does not belong here: it would have to be parsed, and it invites edits
that mean nothing (what does changing a size *do*?). Sizes and dates belong to
`properties`; `look` may render a richer view, since **a different manager may
render differently** — but the buffer `edit` gives you is names.

**Identity comes from the edit stream, not from a visible column.** The mirror
protocol already carries `insert` and `delete`, so the manager tracks which
current line descends from which original — the same mechanism every editor uses
for multi-cursor and folding. So:

| you do | it means |
|---|---|
| change a line's text | **rename** that qid |
| delete a line | **delete** that qid |
| add a line | **create** |
| reorder lines | nothing — order is not state |

A visible qid column would be honest but unreadable, and would tempt someone to
edit it.

> **DECIDED at the stated lean, 2026-09-02:** cross-window moves. Dragging a file from one directory window
> to another is **one edit spanning two buffers**, and neither buffer's Save
> knows about the other. Within a window this is free; between windows it needs a
> rule, and it is the only part of a file manager that does not fall out.

## The status line

*Designed 2026-09-02, closing a gap the lens review found: the demo requires a
status bar "in accordance to spec" and only one entry had ever been specified.*

### The rule that decides what belongs

> **The status line reports state that is CONSEQUENTIAL and NOT OTHERWISE
> VISIBLE.**

Both halves are load-bearing. The one entry already required — **the match
count** after a Find — qualifies on both: 247 matches may all be scrolled out of
sight, and typing changes every one of them. That is the model for everything
else.

The rule **excludes** by the same reasoning, which is what keeps the line quiet:

| candidate | why it is not there |
|---|---|
| **dirty** | already shown — `Save`'s *presence* is the dirty indicator |
| **read-only** | already shown — the **role** is in the title bar; `look` says it |
| **the file's name** | already shown — the title bar |
| **entry count** in a directory | visible by looking |

### Fields are exceptions, not a dashboard

**A field appears only when it is not the default.** UTF-8 and `\n` are the
system's answers, so they are never shown; anything else is. That makes an
appearance *meaningful* rather than ambient, and it means a normal window's
status line is nearly empty.

### The format

The manager writes `/dev/window/status`, one **`<key> <value>`** per line — the
same shape as `verbs` and the rest of the file interface. The manager says
*what*; **the surface decides how** — which fields to right-align, which to draw
as a warning, whether to elide when narrow.

```
matches 247
pending 3 renames, 1 delete
encoding utf-16
```

### What each manager reports

| manager | fields |
|---|---|
| **`edit` on `text/plain`** | `matches <n>` (required, after Find) · `at <line>:<col>` · `encoding <x>` *only when not UTF-8* · `endings crlf` *only when not `\n`* |
| **`look`** | `at <line>:<col>` — nothing else is consequential in a view you cannot change |
| **`edit` on `inode/directory`** | **`pending <n> renames, <n> deletes, <n> creates`** — the plan, shown *continuously*, so you see what Save will do before pressing it |
| **`shell`** | `running <cmd>` while it runs · `exited <status>` when it stops |
| **`manage` on `/proc/<n>`** | `state running` / `state stopped` |
| **`manage` on `inode/system`** | nothing routine |

**The directory case is the rule paying off twice.** "Save shows the plan" was a
guard we specified for editing a listing; the status line gives it a home, and
turns a confirmation step into something continuously visible.

### The layout file — one file, and no new notation

```
/type/inode/system/layout

/home
tabs
    /etc/motd
    /rc/tour
    /home/README
/bin/rc
```

**Three rules, and that is the whole format:**

1. **Each line is a PATH.** The type is *inferred from content* and the role
   *derived from permissions*, exactly as everywhere else — so a layout names
   things, and never says what they are.
2. **Indentation nests.** The axis is not stated because **alternation already
   determines it**: a container's axis is perpendicular to its parent's, so
   depth decides row-versus-column and writing it down would only create a
   second source of truth.
3. **`tabs` groups.** Its children share one rectangle. This is the only
   keyword, and it is needed because tabs are otherwise the *un-allocated
   remainder* — an outcome of the sizing heuristic, which cannot be relied on to
   produce a shape you asked for.

### A shell window needs no special case

`/bin/rc` is an executable. **`manage` on an executable runs it**, and the
window you get is `shell` on the resulting channel — which is exactly the
two-step already settled: *manage creates the running thing, shell converses
with it.* So the layout starts a shell by naming a program, and nothing in the
format knows what a shell is.

**Stating a role** is the one override, appended when the default is wrong:

```
/home look        a listing you cannot edit by accident
```

### What it does not do

The layout declares the **initial tree**. It is not a live description: once the
system is running, the tree is the tree, and moving a window is a `wctl`
operation on it. `Reset` re-reads this file — which is what makes it *"restore
the root window to its default"*, the builtin specified on 2026-09-01.

Both `layout` and `breakpoints` belong to the `manage` role on `inode/system`:
the system manager says *which children and how many columns*, **emca says what
rectangles they get.**

## `emcaopen` — the scriptable way to open a window

*Designed 2026-09-02, closing the last gap inside the demo's boundary.*

```
emcaopen <path> [role]        prints the window id
```

**One argument, because everything else is derived.** The type comes from
**recognition** — the path, then the content — and the role from
**permissions**, so a caller states neither. The old form took a *type* first
(`emcaopen text /etc/motd`), which was right when a caller chose the type and is
wrong now that content decides it.

**`role` is the single override**, for when the derived default is not what you
meant:

```
emcaopen /home                 a listing — `edit`, since you can write it
emcaopen /home look            the same, read-only on purpose
emcaopen /bin/rc manage        runs it; the window is `shell` on its channel
```

That last line is the whole of "open a terminal": **`manage` on an executable
runs it**, and no part of `emcaopen` knows what a shell is.

### And `/rc/emca` shrinks to one line

An earlier draft had `/rc/emca` opening the startup windows *and*
`/type/inode/system/layout` declaring them — two sources of truth for one fact.
**The layout file wins**, because it belongs to the type and is overridable by
binding. So:

```
/rc/emca        starts emca. That is all it does.
```

emca opens `/`, which is `inode/system`; that type's `manage` manager reads
`layout` and opens what it names. **Boot is a type's configuration, not a
script** — which is what *"the managers are already files"* was for.
