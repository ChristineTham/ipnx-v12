# Packages and projects — the first draft (SUPERSEDED)

**NOTHING HERE IS CURRENT.** Written 2026-08-31, before `/home`, before
`/template` was chosen over `/recipe`, before the role vocabulary, and before
the type system. Its paths (`/usr/kitty/...`) and its names are stale
throughout. Kept only for the *shape* of the packages-and-projects argument.

The live proposal is **"pkg, template, project — and promotion as
generalisation"** in [proposals.md](../proposals.md).

> **SUBSTANTIALLY SUPERSEDED, kept as the record of what was proposed.** Written
> before `/home` (the workspace is `/home` and `/home/project`, not
> `/usr/kitty`), before `/template` was chosen over `/recipe`, and before the
> role vocabulary. Its `/usr/kitty/...` paths are the durable names, not what a
> person types. Read it for the *shape* of the packages/projects argument; the
> paths and names are stale, and it has never been reviewed.

*Moved from emca.md 2026-09-02, where it sat inside the windowing spec. `pkg`
and `project` are two of the named-but-undesigned types
([type.md](../type.md)), so this is a proposal about gaps — it was written before
the three-state rule existed and has never been reviewed.*

Her split (2026-08-31): "A package is a toolchain, or command, or
library that installs into ipnx. ... /project are effectively
templates for projects."

### Two types, not one
```
               /pkg                    /project
what           toolchain, command,     a namespace + workspace
               library
mutable        no — versioned          yes, you work in it
composes       no, it is a leaf        yes, combines packages
instantiated   no — BOUND              yes — BECOMES A PROCESS
shared         one copy, many users    yours
```

A two-level graph, not an arbitrary DAG: packages are leaves,
projects combine them. This is what pkg.c already is — "installing
BINDS it; the union directory is the merge mechanism, the namespace
is the installation record" — with no dependency field in its index,
because packages do not need one.

### A project is a proto-process
A namespace plus a command plus env is everything about a process
except that it is running. The three tenses:

```
the parts    /pkg/go            what it is made of
ANTE         /project/ipnx      the process that COULD BE
PRESENT      /proc/1741         the process that IS
```

"Save as project" from a running process is therefore just reading
/proc/N/ns. The registry and the process table are the same
information at two times.

### Instantiate, open, promote
```
TEMPLATES INSTANTIATE; WORKSPACES OPEN. /project/python makes a NEW
instance each time; /home/project/my-app reopens THE one. An
entry says which it is, and that settles persistence: named
workspaces share state, template instantiations do not.

/project is a union of /lib/project (system) and
/usr/$user/project (personal), so PROMOTION IS MOVING A FILE
BETWEEN UNION ELEMENTS — exactly how /bin already works.

PROMOTION PROMOTES THE DECLARATION, NOT YOUR FILES. A stranger
instantiating /project/ipnx should get the packages, binds, env and
a SOURCE (the git URL) — not your working copy and its build
artifacts. Promotion extracts a template from a workspace.
```

### The workspace and its declaration
```
/usr/kitty/ipnx/            the WORKSPACE — the checkout, mutable
/usr/kitty/ipnx/project     its DECLARATION — small, travels with
                            the repo (the devcontainer/flake move)
/usr/kitty/project/ipnx ->  ../ipnx/project     registers it
```

Different lifecycles — two checkouts, one declaration — so they are
different objects. Both readings stay singular and correct.

### Two package systems, and the boundary
```
pkg install go       binds /pkg/go/1.x, recorded in the project's
                     DECLARATION. Declarative, shareable, replayed
                     on instantiation.
pip install requests bytes in the project's WRITABLE LAYER.
                     Content, not declaration.

/PKG DEPENDENCIES ARE DECLARED; LANGUAGE DEPENDENCIES ARE CONTENT.
```

And one property falls out free: /pkg/installed is PER-NAMESPACE.
There is no global install, so reading it inside a project shows THAT
project's packages — same path, different content, per process. The
venv property, the container property, and "which python am I using?"
all answered by the namespace rather than by tooling.

### Trust
A project declaration arriving by git clone is A STRANGER'S
DECLARATION. Docker and devcontainers both have this hole.

```
CLONE AND INSTANTIATE ARE SEPARATE ACTS. Clone fetches content and
opens a DIRECTORY window on it — safe, a listing. Instantiate is an
explicit second act which FIRST SHOWS WHAT THE DECLARATION ASKS
FOR: these binds, this env, these pre-exec commands. Registering
into /project is likewise deliberate, because one click from your
project list is one click from running.
```

This is where IPNX wins rather than mitigates: the whole blast radius
is readable in one small file before you run it, and the namespace
bounds it whether you read it or not. That is P4's pitch made into a
product feature.

### Credentials
/usr/kitty/credentials is a LISTING, not plaintext — factotum's own
model, where reading the ctl file returns keys with secrets elided.
Three layers, and the primary one is structural:

```
the namespace   a project that did not declare credentials cannot
                see them. THE PRIMARY PROTECTION.
the agent       in-namespace processes get the RESULT of using a
                key, never the key
encryption      the bytes leaving the machine — REQUIRED BECAUSE
                THE PROFILE IS PORTABLE (identity.md, M9: "the
                profile is the portable person"). Portability is
                exactly the condition where the namespace stops
                protecting you.
```

Key custody is the SURFACE's half (Keychain, WebCrypto, a
passphrase); the listing and its verbs are IPNX's. Credential windows
are the OPAQUE view mode: metadata visible, bytes never.

