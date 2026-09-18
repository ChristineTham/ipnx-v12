# Identity — the who

> **PROPOSED — not reviewed.** Claude wrote this. Nothing in it is endorsed, and
> nothing in it approves a deviation from Plan 9. What is built is
> [when.md](when.md).

**Role: a *what* — what a user *is* inside the system.** Who the system is
*for* is [personas.md](personas.md).

## What the kernel holds

Plan 9's field and no more: **one name per process**. `eve` is the machine's
owner. A process may become `none` and may not come back.

Permission is the file's own — owner bits, group bits, other — decided by the
device that serves the file. There is no euid, no ruid, no setuid bit and no
credential transition through `/proc`. There is no superuser to become.

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

**Downward is free.** A process may become `none` and may not come back.
Becoming another person needs eve, since their credentials were never yours to
bind.

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
