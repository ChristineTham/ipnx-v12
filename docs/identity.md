# Identity — the who

> **PROPOSED — not reviewed.** Claude wrote this. Nothing in it is endorsed, and
> nothing in it approves a deviation from Plan 9. What is built is
> [when.md](when.md).

**Role: a *what* — what a user *is* inside the system.** Who the system is
*for* is [personas.md](personas.md).

## What the kernel holds

Plan 9's field and no more: **one `char *user` per process** (`portdat.h:664`).
No uid, no gid, no euid/ruid pair, no setuid bit, no credential transition.

**`eve` is a name, compared as a string.** `char *eve` (`auth.c:10`); `iseve()`
is `strcmp(eve, up->user) == 0` (`auth.c:17`).

**eve gets the group bits.** `devpermcheck` (`dev.c:339`) shifts the file's mode
by who is asking:

```c
if(strcmp(up->user, fileuid) == 0)   perm <<= 0;   /* owner */
else if(strcmp(up->user, eve) == 0)  perm <<= 3;   /* group */
else                                 perm <<= 6;   /* other */
```

So the host owner is **not** root. On a file with mode `0700` eve is denied.
There is no bypass anywhere in the core.

**Three files change it.** Two are `#c`'s, one is `#¤`'s — `devcap`
(`devcap.c:267`, letter `L'¤'`):

| | |
|---|---|
| `/dev/user`, `0666` | accepts the four bytes `none` and nothing else — *"anyone can become none"* (`auth.c:107`). One way; there is no route back |
| `/dev/hostowner`, `0664` | eve only. Writing it renames eve **and every process owned by the old name** (`renameuser`, `proc.c:1601`) |
| `/dev/caphash` `0200`, `/dev/capuse` `0222` | the only way to become *another* user. eve writes an HMAC-SHA1 of `from@to@key` to `caphash`, minting a capability (`devcap.c:206`); anyone writes `from@to@key` to `capuse`, and if the hash matches a minted capability and `from` is the writer's current name, the kernel sets `up->user = to` (`devcap.c:215–252`) |

A capability is **consumed** — `remcap` unlinks it from the list — so each is
good once. This is what `auth/newns` and factotum use. It is also the answer to
`su`: a process cannot name a user and become it; it presents a capability eve
minted for exactly that transition.

**`none` is contained**: it cannot read or write another process's state in
`/proc` — `nonone` (`devproc.c:336`), against a subverted server.

**Nothing is centrally privileged.** `iseve()` is called 27 times, spread
across `devcap`, `devcons`, `devenv`, `devkbin`, `devkbmap`, `devmouse`,
`devproc`, `devsd` and `devsegment`. Each device decides what eve may do at its
own files. Authority lives at the resource.

Everything below is userspace.

## What a "user" is

What the names *mean*. Unix's uid conflated a person at a terminal (a billing construct
before it was a security boundary), a protection domain, a service principal
(the daemon users — `lp`, `uucp`, `bin` — never logged into, and the part of
the design that aged best), and root. IPNX resolves the conflation without adding kernel mechanism:

- **The person is eve** — exactly one per kernel instance, many instances
  per person. The kernel instance is the modern terminal; timesharing is
  inverted rather than restored, because kernels now cost a browser tab:
  multi-tenancy happens by instance, and the kernel instance is the new
  uid. Multi-user survives where Plan 9 put it — at servers, per attach.
  There is no `login` and no getty: a person does not log into their own
  instance.
- **The role is the daemon user, kept and ennobled** — a name that owns
  resources and is conferred, never authenticated: conferred by eve, or by a ticket from a file server.
  Unix daemons got a uid; IPNX daemons get a reduced namespace — systemd's
  forty sandboxing directives are a namespace system described one flag at
  a time, and here a daemon's confinement is simply the binds it started
  with.
- **The agent is a role plus a namespace** — the name for the audit trail,
  the namespace for the authority. `none` is the anonymous agent; `su
  none` is its front door.
- **The network person is an authenticated claim, per connection** — no
  global registry; each server believes a proof. That is the attach-time `uname`. A
  person, across their fleet of instances, is their keyring.

The organising sentence: **names are for accounting; namespaces are for
authority.** `/etc/passwd` is personality-side plumbing.

The person's *configuration* — namespace fragments, service mounts,
credentials — is the **profile**, a file tree served by a userspace agent:
factotum's shape unified with secstore's
store/agent split and the /lib/namespace language, portable across a
person's instances, with secrets on a use-don't-read interface and an AI
agent's identity as a sub-profile. The kernel contributes nothing to it,
which is the point.

## `su`

Once `/home` binds to `/usr/<me>` as a whole tree, `su` stops needing machinery:

```
su mimmy  =  a fresh namespace
             bind /usr/mimmy /home
             apply /home/profile        ← now mimmy's
```

The third step reassembles every union root — `/bin`, `/lib`, `/type`,
`/template`, `/pkg`, `/profile`, `/credentials` — because **the profile is the
list of binds**, so `su` names none of them. It is not a mechanism; it is
*assemble someone else's namespace*.

**A bind resolves a channel, not a path**, so rebinding `/home` alone does not
retarget `/bin`'s union element — the profile must be re-applied. That failure
would pass a test that checked `/home` and fail in use.

**Downward is free** — `echo none > /dev/user`, one way. Becoming another
person needs a capability eve minted for that exact transition, consumed once.
`su` is therefore assembly plus a capability, never a bit in the process.

### Bare `su`, and `sudo`

**`su` with no argument binds no personal half at all**, so every union root
falls back to the system's — including its **create element**. That is the whole
of what root was:

```
su ; pkg install ruby      installs into the SYSTEM /pkg — every user inherits
sudo mk install            the same, for one command instead of a shell
```

**The power is which union element your writes land in**, not a bit in the
process. So `mk install` needs no change and no `--user` flag; **no privileged
program exists** — no setuid binary to be perfect, and a bug in `mk` cannot
escalate because `mk` has nothing to escalate; and the power is **legible**,
since `cat /proc/N/ns` answers *what can this process change?* Authorisation is eve, once, at composition.
