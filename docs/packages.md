# Packages, services, templates and profiles

**Role: a *what* — the design of P7.** Endorsed by Christine on 2026-09-24
(*"yet that's look very symmetrical, let's build"*), after a day of
answering it piece by piece; her words are in [verbatim.md](verbatim.md) and
the research behind it is RESEARCH §16. What is built is
[when.md](when.md).


Five separate designs, because they
are different things (Christine: *"In my original concept they are
completely different"*; *"services and packages should be different"*).
What each IS is hers, quoted (`verbatim.md`). **How** each works is taken
from Plan 9 and from existing package managers, as she asked — the research
is RESEARCH §16, measured on apt/dpkg, Cargo and `plan9/`, and every answer
below cites it. What is left open at the end is what the research cannot
decide. **A project has its own document, [projects.md](projects.md)** —
the working folder any of the others is made from.

## Files are named by what they are

*"maybe we need to be very symmetrics. so rc files must always end in
extension .rc"* (Christine, 2026-09-24). Every file a package, service,
template, profile or project carries says what it is by its extension:

| extension | what it is | read by |
|---|---|---|
| **`.rc`** | an rc script, named by its role — `start.rc`, `shell.rc`, `stop.rc`, `install.rc`, `remove.rc` — or by the kind it makes a project into (projects.md) | rc |
| **`.env`** | environment variables for the script of the same name — `/profile/start.env` for `/profile/start.rc` | the script's starter, before the script |
| **`.cfg`** | the configuration — `pkg.cfg`, `service.cfg`, `template.cfg`, `profile.cfg`, `project.cfg`: *"name of template, version, date other properties"* | `libndb`; `ndb/query` from rc |

**A command written in rc is named by what is typed** — `lc`, `9fs`,
`dircp` — with no `.rc`, as Plan 9 names them (decided 2026-09-24). The rule
is for scripts run by role or by kind.

**A `.cfg` is written in `ndb(6)`** (*"if plan 9 has ndb let's use that
consistently"*) — Plan 9's one configuration format: *"multi-line tuples
made up of attribute/value pairs of the form attr=value … Each line
starting without white space starts a new tuple. Lines starting with # are
comments"* (`plan9/sys/man/6/ndb`); a value with spaces is double-quoted
(`libndb/ndbaux.c:41`). A program reads one with `libndb` (1,787 lines,
`plan9/sys/src/libndb`, on `libbio`); a script with `ndb/query -f pkg.cfg
pkg hello version` (`plan9/sys/src/cmd/ndb/query.c`, 114 lines). For
example, a package's:

```
# pkg.cfg
pkg=hello version=1.0 date=2026-09-24
	description="says hello"
	depend=libbio
```

## A package

*"like a FreeBSD pkg or apt… a list files to be bound in the namespace, plus
potentially initialisation scripts"*; *"like installing a toolchain or a
library"*; installed *"to the system… to the namespace… or to the user"*.

1. **Where it comes from — a repository is a file server.** Plan 9 fetches
   its distribution by mounting it (`9fs sources`, `replica/pull`; §16.3);
   Cargo names its repository in one file (`config.json`, §16.2). So a
   repository is named by the file server that holds it, mounted like any
   other — a host directory, or a network server once `/net` exists. Nothing
   downloads: `pkg` reads the repository's files over 9P.
2. **Its description is `pkg.cfg`** — apt's fields (§16.1): name,
   version, description, dependencies with version bounds, the packages it
   breaks; and the file.
   The repository's **index** lists every package so, with **the file's
   SHA-256**, as apt's `Packages` does.
3. **Verification is Plan 9's** — decided 2026-09-24 (*"plan 9 way"*;
   RESEARCH §16.4): **a hash per package in the repository's index, and the
   index fetched over an authenticated connection** — the 9P session
   authenticated through `factotum`, as `srv` does without `-n`. No signed
   index and no keyring: trust is the connection, as everywhere in Plan 9.
   A package is copied into `/pkg/<name>/<version>/` only if its hash
   matches the index, as 9legacy's `replica` refuses a file whose hash does
   not match its log (`applylog.c:1055`), and the store entry never changes
   after (*"a store entry never changes after verification"*). The hash is
   SHA-256, as apt's and Cargo's are (§16.1, §16.2); 9legacy's is SHA-1.
4. **Its files appear by `bind`** — lines in a namespace file, as
   `/lib/namespace` is written, which `newns` reads.
5. **Its scripts** — `install.rc` and `remove.rc` (below, *the scripts*);
   dpkg has four (§16.1): before and after install, before and after
   removal. Configuration it writes is listed as such and **left
   on removal, taken only on purge**, as `conffiles` are.
6. **The three scopes** are where the bind lines go: the calling process's
   namespace now; the user's profile, `/home/profile` (every login); or the
   system's, `/profile` (every user) — and the package is listed as
   installed in `/profile/pkg` (or `/home/profile/pkg`).
7. **Pruning** — apt's marking (§16.1): a package installed only because
   another needed it is marked so; a version in `/pkg` that no profile or
   project lists — directly or as a dependency — may be
   pruned (*"the store must be prunable"*).
8. **A lock** — Cargo's (§16.2): where a package is installed persistently
   (a profile, a project), the exact version and its SHA-256 are recorded,
   so the same bytes are bound next time.

## A service

*"servers/daemons… system, user or project specific… into system starts a
server when system starts, configurable in /rc. In user, starts when user
logs in, terminates when user logs out. Project - starts when project is
opened, terminates when project is closed."* *"services installs daemons."*

1. **The daemon's programs are a package; the service is separate** —
   exactly Debian's split (§16.1): `redis-server` is the service,
   `redis-tools` the programs; `postgresql-common` holds PostgreSQL's start
   script, `postgresql-16` its programs. A service names the packages it
   needs and they are installed into the same scope.
2. **A service is `start.rc`**, which starts its server and posts it in
   `/srv`, **and `stop.rc`**, described by `service.cfg` — what `cpurc` does for `ndb/cs` (§16.3) and what an init script
   does for `redis-server` (§16.1).
3. **Its settings are a separate file the script reads** — Debian's
   `/etc/default/redis-server`, sourced at start (§16.1) — kept on removal
   as configuration. *Proposed:* that file is its `start.env`.
4. **It runs in a namespace of its own, as its own user** — Plan 9's
   `listen` runs a service as `none` in a namespace made from a namespace
   file (`/lib/namespace.httpd`, §16.3); Debian runs `redis` as user
   `redis` (§16.1).
5. **Installed is listed in `/profile/service`** (the user's in
   `/home/profile/service`);
   **disabled is Plan 9's leading `!`** (`!tcp515`, §16.3). Enabling, starting, stopping
   and disabling are four acts, as in dpkg's scripts (§16.1).
6. **When it runs:**

   | scope | starts | stops |
   |---|---|---|
   | system | at boot, from `/profile`'s init scripts — where Plan 9's `termrc`/`cpurc` and `/cfg/$sysname/…` start daemons (§16.3) | at shutdown |
   | user | at login, from the profile | at logout: *"hangup"* to the login's note group |
   | project | when the project's window opens | when it closes: *"hangup"* to the window's note group, as `rio` does (`wind.c:1111`) |

7. **No restart when it dies** — neither Plan 9's startup scripts nor
   Debian's init scripts restart a daemon (§16.1, §16.3).

## A template

*"a prototype for a project… it may install packages, but contains project
scaffolding"*; it *"instantiates new versions of files (scaffolding), not
just binds of files shared across namespaces"*.

A directory under `/template/<name>`, described by `template.cfg`: the scaffolding (`package.json`,
`.gitignore`, editor settings, sample code) and the packages and services
the project needs. **Instantiating copies the scaffolding** into a new
project and writes the lists into its project file — the project's from
then on. A template may include another, as a namespace file includes
another with `.`.

## A project

In its own document, [projects.md](projects.md): the working folder a
template, a package, a service or a profile is made from, and a window type.

## A profile

*"I envisaged /profile to contain any files required to configure a system -
the kind of stuff in Unix /etc. network config, namespace bindings, init
scripts etc. The user's profile is contained in /home (synonym for
/usr/<username>), in /home/profile."* A profile *"may be built from a
template, but… once it is instantiated it belongs to the user"*, and the
user *"can save current namespace config as a template"*.

**`/profile` is the system's configuration** — Unix's `/etc` — and
**`/home/profile` the user's**, `/home` being `/usr/<username>`. A user's
profile *"is just really a template, but may contain other things as
well"*, described by `profile.cfg`, and made from a project as a template
is (projects.md).

| | `/profile` | `/home/profile` |
|---|---|---|
| namespace bindings | what `/lib/namespace` is on Plan 9 | the binds Plan 9 users make in `$home/lib/profile` |
| init scripts | what `/rc/bin/termrc`, `cpurc` and `/cfg/$sysname/*` are on Plan 9 (§16.3) | what `$home/lib/profile` is (`init.c:178`) |
| network configuration | what `/lib/ndb` is on Plan 9 | — |
| environment | every user's | the user's own |

At boot the system's is applied; at login the user's after it, so the
user's binds win — Plan 9's order (`newns` runs `/lib/namespace`, then
`$home/lib/profile` runs). Built from a template is instantiating one into
`/home/profile`; by hand is editing it. Saving the current configuration as
a template writes `/proc/<pid>/ns` and `/env` into a new template.

## `/rc` retired

*"the info in /rc probably should be in /profile and we should retire the
concept of /rc."* Plan 9's `/rc` holds three kinds of thing (`plan9/rc`):

1. **startup configuration** — `termrc`, `cpurc`, `cpurc.local`, and
   `listen`'s `service` directory — which moves to `/profile`;
2. **commands written in rc** — most of `/rc/bin`'s 118 files (`9fs`,
   `Kill`, `dircp`, `diffy`, `replica/*` …), bound onto `/bin` by
   `/lib/namespace:27` (*"bind -a /rc/bin /bin"*). Plan 9 keeps them in a
   directory of their own only because it has no packages. **Here they are
   a package like any other** — their files in `/pkg`, bound onto `/bin`
   (*"Why are these not stored in /store and bound to /bin like a
   package?"* — asked before `/store` was dropped);
3. **`/rc/lib/rcmain`** — rc's startup file.

**The scripts are named by role, not by whose they are** (*"I am thinking
we should actually name them by role rather than distinguishing between
system and user"*):

| script | the system's — `/profile/…` | the user's — `/home/profile/…` | a service's — `/service/<name>/…` | Plan 9's |
|---|---|---|---|---|
| **`start.rc`** | *"executes when system boots"* | at login | when its scope starts | `termrc`, `cpurc`, `/cfg/$sysname/*`; `$home/lib/profile`; a `cpurc` line (§16.3) |
| **`shell.rc`** | *"executes with every new shell"* | with every new shell of the user's | — | `rcmain`'s settings — prompt, `$path` (`plan9/rc/lib/rcmain`) |
| **`stop.rc`** | *"execute when system shuts down"* | at logout | when its scope ends | none: Plan 9 ends a window's processes with *"hangup"* (`rio/wind.c:1111`) |

**`shell.rc` is `rcmain`'s configuration, taken out of it.** Plan 9's rc runs
`rcmain` in every shell (`plan9/rc/lib/rcmain`); part of it is settings —
the default prompt and `$path` — and part is rc's own machinery, choosing
whether to read `-c`, a file or the terminal. The machinery stays rc's code,
in rc's package; it runs `/profile/shell.rc` and then
`/home/profile/shell.rc`, so the user's settings come last. A fork is not a
new shell: a subshell or a pipeline stage inherits what `shell.rc` set, as
a forked Plan 9 rc never runs its startup again.

**Packages and templates have `install.rc` and `remove.rc`** (*"Packages and
templates have configrc instead"*, then *"or maybe installrc and
removerc"*) — they are not started or stopped, they are installed and
removed:

| script | a package's — `/pkg/<name>/<version>/…` | a template's — `/template/<name>/…` | dpkg's (§16.1) |
|---|---|---|---|
| **`install.rc`** | at install, in the scope installed to | when a project is instantiated from it, in the new project | `postinst configure` |
| **`remove.rc`** | at removal — undoing what `install.rc` set up, leaving configuration unless purged | when the template is removed from `/template` | `prerm`, `postrm` |

These replace `pkgrc` and, a message later, `configrc`.

**emca is a service** (*"emca is a service"*): its scripts are
`/service/emca/start.rc` and `/service/emca/stop.rc`, and `emcarc` is not
needed. Plan 9 starts its window system from the user's profile — a new
user's is written to end in *"exec rio"* (`sys/lib/newuser:32`) — which is
the user scope: emca starting at login and ending at logout.

## Where things are — no `/store`

*"even better still, /pkg only contains packages. list of installed
packages is /profile/pkg etc"* — decided 2026-09-24, replacing `/store`:

| | the system's | the user's own (*"user defined pkg services are in /home/pkg etc"*) | installed — system | installed — user |
|---|---|---|---|---|
| packages | `/pkg/<name>/<version>/` — downloaded, verified, never changed after | `/home/pkg/<name>/<version>/` | `/profile/pkg` | `/home/profile/pkg` |
| services | `/service/<name>/` | `/home/service/<name>/` | `/profile/service` | `/home/profile/service` |
| templates | `/template/<name>/` | `/home/template/<name>/` | `/profile/template` | `/home/profile/template` |

A package installed to the namespace scope is in that process's binds and
needs no list.

**Only the host owner writes `/pkg`, `/service`, `/template` and
`/profile`; anyone else installs into them with `su` or `sudo`** (*"user can
install packages, services etc by using su or sudo"*). **What a user
defines is in their own** — `/home/pkg`, `/home/service`, `/home/template`
— which they write without either; and their lists, in `/home/profile`,
name things in both.

**Proposed:** each user half is bound after the system's —
`bind -a /home/pkg /pkg`, and so for the others — so `ls /pkg` shows both,
and a name in both resolves to the system's unless the user binds theirs
before it. This is the union `platforms.md` proposed for every root
(unreviewed). **What is installed is configuration, so it is in the
profiles**; `/pkg`, `/service` and `/template` hold only the things
themselves. **The lists are files of their own** — `/profile/pkg`,
`/profile/service`, `/profile/template` — and not entries in
`profile.cfg` (decided 2026-09-24).

`/store` added nothing `/pkg/<name>/<version>/` does not have: it is named
uniquely because a repository publishes one set of bytes per name and
version, it is immutable once verified, installing still binds from it, and
a version no profile or project lists may be pruned (*"the store must be
prunable"* — now of `/pkg`). Plan 9 has no store; Cargo keeps what it
downloads by name and version (`registry/cache/wasmtime-39.0.2.crate`,
RESEARCH §16.2). A store addressed by content is needed only where one
version can exist several times, built against different dependencies —
not the case here.

## Decided

**Verification** — the Plan 9 way (Christine, 2026-09-24: *"plan 9 way"*),
now in the package design above.

**Identity is Plan 9's** (RESEARCH §16.4), and the kernel already has the
capability device it runs on: a terminal's user is the host owner, named at
boot; **login** is `auth/login` — a new namespace, a `factotum`, and a login
shell under the new id — and **logout** is that shell ending; **`su`** is
`auth/login` naming the host owner, which asks for the host owner's
password, and **`sudo`** is one command run that way; `auth/as` is the
other direction — the host owner running a command as another user
(`as.c:3`, *"must be hostowner for this to work"*); **a daemon's own user**
is `none`, as
`auth/none` and `listen` run services. What remains is to build them.
