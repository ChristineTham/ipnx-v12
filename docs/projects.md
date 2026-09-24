# Projects

**Role: a *what* — the design of a project**, the working folder a
template, a package, a service or a profile is made from. Christine's words
are in [verbatim.md](verbatim.md); the things a project becomes are
[packages.md](packages.md). What is built is [when.md](when.md).

## What a project is

*"a "project" is the working folder for what may become a template, a pkg
or service. a project can be "promoted into template, pkg, service, user
profile (which is just really a template, but may contain other things as
well.)"* (Christine, 2026-09-24).

*"A project can be many things: template, pkg, service etc. in which case
the the various configuration files are distinct."*

So a project is a directory in which something is made, and which may be
made into one or more of the four — promoted. Promoting does not end the
project: it goes on being where the thing is worked on.

**Promotion is to the user by default, and to the system with `su`**
(*"promotion to user by default, to system with su"*): what `pkg.rc` makes
goes into `/home/pkg/<name>/<version>/`, and so `/home/service`,
`/home/template` and `/home/profile` for the others; with `su`, into
`/pkg`, `/service`, `/template` and `/profile` — as any install to the
system is (packages.md, *Where things are*).

## Its files

Named by the symmetry in [packages.md](packages.md), *Files are named by
what they are*: a `.cfg` is `ndb(6)`, a `.rc` is an rc script.

| file | what it is |
|---|---|
| **`project.cfg`** | *"tells us what is in the project -name, version, etc."* |
| **`project.lock`** | what was installed for it — exact versions and SHA-256 — written by `pkg` |
| **`template.cfg`** | the template it is promoted into — *"name of template, version, date other properties"* |
| **`template.rc`** | *"the set of commands to convert a project into a template"* |
| **`pkg.cfg`**, **`pkg.rc`** | the same, for a package — *"same for pkg, service etc."* |
| **`service.cfg`**, **`service.rc`** | the same, for a service |
| **`profile.cfg`**, **`profile.rc`** | the same, for a user's profile |

A project that is a package and a template carries both pairs, each with its
own configuration. `pkg.cfg` is the package's description — apt's fields,
as packages.md gives them — and is carried into what `pkg.rc` makes, so the
package is described by the same file it was made from.

For example:

```
# project.cfg
project=hello version=1.0 date=2026-09-24
	description="says hello"
```

## Plan 9's nearest

Plan 9 has no projects. What it has for turning a directory into something
installed is the **proto file**: a list of which files of a tree go into a
distribution (`plan9/sys/lib/sysconfig/proto/portproto`, eight in all), read
by `disk/mkfs`, which *"copies files from the file tree source"* and under
`-a` *"write[s] an archive file to standard output"*
(`plan9/sys/man/8/mkfs`). *Proposed:* `pkg.rc` is written with those — a
proto file naming the package's files, and `disk/mkfs -a` making it.

## A project is a window type

*"This practically means a project is a type that is instantiated when user
opens project file in a new emca window. Projects live in /project/x but the
binding is user/process speccific."*

1. **A project is a window type** ([type.md](type.md)): opening its
   `project.cfg` opens a new emca window whose namespace has the project's
   packages bound and its services started; closing the window hangs them
   up.
2. **`project.cfg` is the manifest** — Cargo's `Cargo.toml` (RESEARCH
   §16.2): the packages and services it wants. **The lock is `project.lock`**, beside it
   (decided 2026-09-24: *"lock can be project.lock"*) — the exact versions
   and SHA-256 of what was installed, as Cargo keeps `Cargo.lock` beside
   `Cargo.toml`. It is `ndb`, written by `pkg` and never by hand.
3. **`/project/<x>` is a binding, per user or process**: the files live
   under their owner's `/usr/<name>` as everything a Plan 9 user owns does,
   and each user binds the projects they have at `/project/<x>`. Two users
   binding different directories there have two projects; binding the same
   one, they share it.
4. **Each opening runs its own services**, in its window's note group — the
   only arrangement in which closing a window stops exactly what it
   started.
5. **A template instantiated makes a project**: its scaffolding copied in,
   its packages and services written into the new project's `project.cfg`
   (packages.md, *A template*).
