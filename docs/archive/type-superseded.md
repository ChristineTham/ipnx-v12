# Superseded sections of type.md

> **ARCHIVED 2026-09-29 — NOTHING HERE IS CURRENT.** These sections of
> docs/type.md were overruled by its *design* (Christine, 2026-09-18):
> recognition is `file(1)` and dispatch the plumber, a type is four files
> (`rules`, `manager`, `namespace`, `verbs`), and type names are MIME types.
> *"Make sure stale decisions that have been overruled are not still there to
> confuse future readers"* (Christine, 2026-09-02). Kept as history.

## What a type declares

| file | |
|---|---|
| `recognise` | how files of this type are identified (see below) |
| `managers` | the **roles** this type offers, one per line — `look`, `edit`, `properties`, `manage`, `shell`. Which resolves to what is per-type; the role name is stable everywhere, so the dropdown reads the same on every window |
| `verbs` | toolbar bindings, one per line |
| `README` | prose for whoever reads the registry; nothing parses it |

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
| **items** | what Find selects and Run's `>` feeds — byte ranges for text, files for a listing, pids for `proc` |
| **verbs** | what the toolbar offers, which `window` already carries |
| **size** | intrinsic dimensions for the kinds with an aspect ratio rather than a line count — already the `size` event |

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
