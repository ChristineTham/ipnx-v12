# Superseded — `type.md`'s "pkg, template and project — the shapes" and `/store`

**NOTHING HERE IS CURRENT.** Moved out of [`type.md`](../type.md) on
2026-09-24, when Christine redefined a package, a service, a template, a
project and a profile, and removed `/store`: *"/pkg only contains packages.
list of installed packages is /profile/pkg etc"* (`verbatim.md`). It was
Claude's design, and its two blocks headed "DECIDED" record Claude's
decisions, not hers. The current designs are P7 in
[`proposals.md`](../proposals.md).

## pkg, template and project — the shapes

*Settled 2026-09-02. One rule decides each: **a file if a declaration is all
that is needed; a folder when there are files to carry.** The measurement
behind it is RESEARCH §13; what remains open is in
[proposals.md](proposals.md).*

| | shape | why |
|---|---|---|
| **pkg** | a **file** | its content is *fetched* and lives in `/store`. A folder only if patches grow large enough to want separate files |
| **template** | a **directory** | declaration **plus a skeleton** — `.gitignore`, `README`, `package.json` are files, and they must live somewhere |
| **project** | a **directory** | the workspace: your files, plus the `template` it was made from |

**Why a package is a file, measured rather than assumed:** four of the five
things that make a package a folder elsewhere are compensations for
**mutation** — Debian's four maintainer scripts, `md5sums`, `pkg-plist` and
`conffiles` — and this system does not mutate. Installing is a bind, the store
is immutable after verification, removal is an unbind, and your `/home/<x>`
binds *over* the system's rather than replacing it. Only **patches** survive as
a reason for extra files, and Homebrew shows they can be inline.

**Why a template is a directory:** a template is a proto project, and a project
has files. They cannot live in `/store`, whose properties — immutable,
verified, content-addressed, prunable — fit *fetched* content, while a skeleton
is **editable source you iterate on**. And promote decides it: you promote a
project (a directory with files) into a template, and promoting into a file
loses the files. Every project-template mechanism in the industry agrees —
cookiecutter, GitHub template repositories, Yeoman, degit, `.devcontainer/`.

### The principle that separates them

> **Bind what stays shared. Copy what becomes yours.**

A package's content is shared — many projects bind the same Python — so it
lives in `/store` and is **bound**. A template's skeleton becomes *your* files,
divergent from the moment it lands, so instantiating **copies** it. Binding it
would mean editing your `main.py` edited the template's.

### Recognising them

**Path first**, because the system created them there and the location is
*caused by* the type:

```
path /pkg/*  path /home/pkg/*              a package declaration
path /template/*  path /home/template/*    a template
path /home/project/*                       a project
```

**`contains template`** is the fallback for a project cloned somewhere else,
since a project always carries what it was made from. This is what convention
buys: a directory sits in exactly one place, so the ambiguity that marker files
alone would create does not arise.

## `/store` — where verified bytes live

*Accepted by Christine, 2026-09-02.**declaration file** rather than a directory,
the bytes it binds from need a home, and nothing in the design had one.*

### What it holds — and what it does not

**The store holds FETCHED content**: what a declaration names, verifies by
digest, and **binds**. Python's tree, Ruby's, a library's headers.

**It does not hold a template's skeleton.** An earlier draft of this section
said it did; that was overturned the same day (see *"a TEMPLATE is a
directory"* above). A skeleton is **editable source you iterate on**, and the
store's properties — immutable after verification, content-addressed, prunable
— are exactly wrong for it. The principle that separates them:

> **Bind what stays shared. Copy what becomes yours.**

The store is the *bind* side. A template's skeleton lives in the template's own
directory and is **copied** at `New`.

### The path: `/store`, a union like every other root

```
/store/<name>/<version>/     a verified tree: bin/ lib/ include/ as it provides
```

System entries in the system's half, yours in `/home/store` bound over it — so
`pkg install` as a user writes to yours and under `su` writes to the system's,
with **no code in `pkg` aware of the difference**. Exactly as `/bin`, `/type`
and `/template` already work.

**Name-and-version is the interface; content-addressing is not.** A declaration
names `/store/python/3.14`, and whether the store implements that as a real
directory or as a bind to a hash-named one is **invisible to everything above**.
So dedup can arrive later without changing a single declaration — and a
snapshot server
already proves the system can share unchanged bytes structurally.

### What a declaration does with it

```
/pkg/python                  a list of bindings plus commands
    fetch  <url>  sha256:a3f1…  →  /store/python/3.14
    bind   /store/python/3.14/bin  /bin
    env    PYTHONHOME /store/python/3.14
```

The digest is **pinned in the declaration**, which is what makes the whole thing
auditable: `cat /pkg/python` tells you what will be fetched, what it must hash
to, and what it will bind. Nothing is discovered at install time.

### Content that is a TREE — one digest over a manifest

*Endorsed by Christine, 2026-09-15. The gap this closes was in the sketch
above: its `fetch` line points a single digest at `/store/python/3.14`, which
is a **directory** — and nothing said how one digest becomes 539 files.*

```
/pkg/python
    tree   python/3.14/manifest  <sha256>  /store/python/3.14
    bind   /store/python/3.14/bin  /bin
    env    PYTHONHOME /store/python/3.14
```

**The digest pins the manifest; the manifest pins every file.** The manifest is
`sha256sum`'s own output —

```
e3b0c442…  bin/python
a3f1d29e…  lib/python3.14/os.py
```

— so authoring one is a command that already exists, not a format invented
here. Entry paths are relative to the manifest's own directory on the registry
side and to the store entry on the store side, which is what makes the two
trees the same tree.

**The audit chain stays closed.** `cat /pkg/python` still names one digest and
still discovers nothing at install time; `cat /store/python/3.14/manifest`
names the rest. The declaration stays a page rather than 539 lines nobody
reads — and *"539 lines is not an audit anyone reads"* is the property the
format exists for, so a shape that destroys it fails on its own terms.

**The manifest is fetched first, verified, and then kept in the store** at
`<entry>/manifest`. Both halves are load-bearing: verified first, because
nothing else may be believed on its word; kept, because `pkg verify` must
re-check all 539 **offline, from the store, with the registry unreachable** —
which is the plane test applied to verification rather than to installation.
One consequence, stated rather than discovered: `manifest` is the single name
a tree may not itself contain at its root, and an entry claiming it is refused.

**And an entry names a place inside the store entry — nothing else.** The
digest proves the manifest is the one the packager published; it does not make
its paths benign, and the manifest is the half of the audit nobody reads line
by line. An absolute path, or one carrying `..`, is refused on install and on
verify alike: *a digest authenticates bytes, it does not authorise what they
say.*

**Why a manifest rather than an archive.** An archive — one fetched blob,
unpacked — would also give one digest, and it is what every other system does.
It loses on the measured constraint and on the property above:

| | manifest | archive |
|---|---|---|
| memory | streams file by file; nothing is ever held | 12 MB materialised, then unpacked, inside a **16 MB** guest ([RESEARCH §9.24](../RESEARCH.md)) |
| the store | holds the tree, once | holds the archive *and* the tree, or discards the thing the digest names |
| granularity | every file carries its own digest, so `verify` names the altered file | the digest covers the blob; a changed file is detectable but not nameable |
| new vocabulary | none — `sha256sum` already emits this | an archive format, and an unpacker to go with it |


### Immutability, and why it is enforceable

**A store entry never changes after verification** — otherwise the digest lies
and every declaration referencing it becomes a false claim. The store's server
refuses writes to an existing entry, which is a one-line rule rather than a
convention, and it is what makes *"a name that would bind over different bytes
is refused"* checkable at install rather than debuggable afterwards.

It also means **`pkg remove` is an unbind, not a delete** (already the design's
claim): the entry survives, so re-installing is free and rollback costs nothing.

### The store must be PRUNABLE — and prune needs no new machinery

**Reachability is readable**, so what is unused is *computed*, not estimated:

```
grep -rl /store/python/3.14 /pkg /home/pkg /template /home/template   # named?
grep -l  /store/python/3.14 /proc/*/ns                                 # bound?
```

Which means **the dry run is just the listing** — no separate `--dry-run`
mode, and no "why is this still here" that cannot be answered by looking.

**Safety is unlink-while-open.** Remove the entry; processes already holding
binds keep working; the bytes go when the last channel closes. The kernel
already has refcounted channels closed at exit, so prune refuses nothing and
consults no root set — it is `rm` with the semantics Unix has always had. That
is the whole of what Nix needs GC roots for.

**And prune throws away materialisation, never intent.** The declarations are
the truth; the store holds only the bytes they name, so **a pruned entry
refetches**. That is why it can be run aggressively, and why it is categorically
safer than `docker prune`, where losing an image can lose something
unreproducible once its Dockerfile is gone. Nothing here is unreproducible: the
digest is pinned in the declaration.

> **DECIDED at the stated lean, 2026-09-02 — the retention policy, and the offline case.** *Refetches* is
> only true with the network. Pruning a version you cannot re-obtain — the
> registry moved, you are on a plane, the upstream vanished — is the one way
> this bites, and it is exactly the case P5's *plane test* cares about. Whether
> prune keeps the last N versions, anything bound in the last N days, or
> anything a declaration still names, is undecided; **"anything still named"**
> is the safest default and costs little, since a declaration you deleted was a
> decision you made.

> **DECIDED 2026-09-04 — `/store` IS SERVED BY A USERSPACE FILE SERVER over a
> host directory.** Christine, choosing between the two this paragraph had
> weighed without stating a lean: **verification lives inside IPNX**, not in a
> promise the host makes. `storefs` reads the host's directory over 9P and
> serves `/store` over 9P, so *"a store entry never changes after verification"*
> is a rule a program we own enforces, and *"a name that would bind over
> different bytes is refused"* is checkable at install rather than debuggable
> afterwards. It costs one process at boot, and it **streams** — it must never
> hold the bytes, because a guest's linear memory caps at 16 MB and one package
> is 29 MB ([RESEARCH §9.24](../RESEARCH.md)).
>
> The original weighing, kept because it is what the decision chose between:
>
> **2026-09-02.** It must be immutable-after-write,
> reachable by every process that binds from it, and durable across boots — so
> it is either host storage the host serves over 9P (durable, but the host owns the
> integrity guarantee) or a userspace file server over a host directory
> (integrity in IPNX, one more process). The first is simpler; the second keeps
> the verification where the design says trust lives.

> **DECIDED at the stated lean, 2026-09-02 — whether `/store` should be visible at all.** Every path above
> could be hidden behind the declaration, with `/store` unmounted in ordinary
> namespaces. Hiding it makes the audit commands impossible; showing it means a
> person can `cd` into a place nothing should be edited. Given the system's bias
> — *everything is a file, and the audit answers by looking* — I propose
> **visible and read-only**, but it is a real trade.

