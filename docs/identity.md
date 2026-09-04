# Identity — the who

> **UNDER REVISION (2026-09-03).** This document argues that a hosted kernel
> should carry an effective/real uid pair because *"a hosted kernel inverts the
> trust geometry"*. **Plan 9's kernel has one identity field — `char *user` —
> and no euid, ruid or setuid anywhere**; `DMSETUID` is 9P2000.u, a Unix
> extension. The argument was made when *hosted* was the only target; **on a
> hypervisor or a Raspberry Pi there is nothing to invert toward, and the
> mechanism would remain without its justification.** The model below is
> coherent and may well be right *as a personality* — the question reopened is
> whether any of it belongs in the kernel. See [design.md](design.md)
> 2026-09-03 and [implementation.md](implementation.md) P1.

**Role: a *what* — the identity model.** What a user *is* inside the
system: person, role, agent, network identity, and the uid model that
implements them. Who the system is *for* is [personas.md](personas.md).

*Role: the **who** — what a "user" is in this system (person, role, agent,
network person), the kernel's credential mechanism, `su`, and where the profile
sits. The mechanism was decided 2026-08-26; the identity architecture
2026-08-29 (both in the [decision log](design.md)). The rest of the set:
why — [design.md](design.md); what — [architecture.md](architecture.md);
how — [handbook.md](handbook.md); when — [implementation.md](implementation.md);
where — [platforms.md](platforms.md); was — [poc.md](poc.md). Who the system
is **for** — the other half of who — is [personas.md](personas.md).*

*The mechanism is the item APE called impossible — "setting the userid,
groupid, effective userid and effective groupid do not do anything useful. The concept is
impossible to simulate in Plan 9." — and the single item the plan said decides whether
V10 compatibility is real or approximate. The mechanism below is implemented in the PoC
and exercised by its acceptance tests; the numbered deferrals at the end are measurements
against V10 source, to be taken in the parent repository when the personality's libc is
built.*

## Why APE could not, and this kernel can

In Plan 9, identity is established per file server at **attach** time and the kernel
holds no mutable credential — file servers own enforcement, authentication establishes
`uname`, and there is deliberately no `setuid`, no setuid bit, no root. APE lived *above*
that kernel; there was nothing for `setuid` to write to.

A hosted kernel inverts the trust geometry. The kernel and its in-process devices are one
trust domain — the Dev-table-inside decision — so the kernel can own a **mutable
per-process credential** and present it to every device operation, which is exactly V10's
own arrangement: `u.u_uid` checked against inodes, in the kernel. Wire mounts keep
Plan 9's per-attach identity. Nothing in either regime is simulated.

## The credential

Per process, in the kernel, inherited across both fork paths and preserved by `exec`:

    cred = { euid, ruid }        — names, not numbers

with `gid`/`groups` following the same design when the personality needs them (deferral
D2). The boot credential is the **host owner** — Plan 9's eve — and the kernel contains
no distinguished name: "root" is not a kernel concept.

**Names are canonical; numbers are the personality's.** Plan 9 and 9P2000 speak `uname`
strings, and the kernel stays in Plan 9's type system. V10's sixteen-bit uids live in the
personality's libc, mapped through V10's own `/etc/passwd` — `getuid()` is
read-`/dev/user`-then-look-it-up, and uid 0 is whatever name the passwd file maps it to.
This is the class-B philosophy applied to identity: restoring V10's *interface* without
un-eleganting the kernel. (9P2000.L went numeric instead — `n_uname` — recorded as the
road not taken; strings keep 9P2000 wire compatibility and Plan 9's model.)

## No new system calls

The seven V10 identity calls land on files, in Plan 9's own idiom:

| V10 call | lands on |
|---|---|
| `getuid` | read `/dev/user` (the kernel's `#c/user`), map through passwd |
| `getgid` `getgroups` | same shape, deferral D2 |
| `setuid(n)` | libc maps n → name, writes `user <name>` to `/proc/<pid>/ctl` |
| `setruid` `setgid` `setgroups` | same vehicle, deferral D2/D3 |
| `getlogname` | read `/dev/user` (already class B) |

`#p`, the proc device, is the vehicle — the same device V10's class-B rows already
wanted for `nice` (write `ctl`) and `times` (read `status`). Its minimum is
`#p/<pid>/{ctl,status}` plus `self`.

**The transition rule**, enforced by the kernel at the `ctl` write:

1. The host owner's processes (`euid == eve`) may become anyone — V10's
   "effective uid 0 may setuid to anything", with eve in the root role.
2. Any process may set `euid` to its own `ruid` — the V7 `setuid(getuid())` idiom that
   setuid executables use to drop privilege.
3. A successful eve-transition sets both `ruid` and `euid` (V10 `setuid`); `setruid`
   moves only the real uid (deferral D1 records the exact V10 packing to imitate).

## The setuid bit

V10's elevation mechanism is filesystem metadata, and 9P2000's mode word has room the
way 9P2000.L had room for `Tlink`: **9P2000.u defined `DMSETUID 0x00080000` and
`DMSETGID 0x00040000`**, and this system adopts those bit positions. At `exec`, the
kernel stats the image through the caller's namespace; if the mode carries `DMSETUID`,
the process's `euid` becomes the image's owner. `login`, `su` and `passwd` are this bit
plus rule 2 above.

The bit is set the way every other mode bit is set — `wstat` — so `chmod` stays class B
and the kernel gains nothing new.

## Enforcement: two regimes, stated honestly

- **In-process devices** (ramfs, cons, pipes — the personality's own image) receive the
  requesting credential on every operation and enforce V10 semantics in full: rwx by
  owner/other against per-node `uid` and `mode`, eve bypassing as root does, `wstat`
  letting owners chmod and only eve chown, `umask` applied at create (a per-process
  kernel field, as the call list already assigned it).
- **Wire mounts** (devmnt) carry identity per **attach**: the kernel stamps `Tattach`'s
  `uname` with the mounting process's `euid` — the "stamp every attach" the plan
  promised — and the server enforces from there, which is Plan 9 semantics. A process
  walking a mount someone else made uses the mounter's remote identity; that is the
  namespace-as-capability model working as designed, and `rfork(RFNOMNT|RFCNAMEG)`
  remains the containment tool. `exportfs` needs no rule at all: it answers with real
  syscalls under its own credential, so exported permissions are enforced by the
  exporter's kernel.

The two regimes are not a compromise; they are V10's kernel and Plan 9's protocol each
enforcing where each is authoritative.

## su without a superuser (2026-08-29)

There is no superuser, structurally, at three boundaries: above the kernel
(eve's total authority is the hosting process's sandbox — there is nothing to
escalate to), at every mount (eve-ness does not serialize; a 9P server sees a
`uname` string and applies its own policy, so the superuser's reach ends at
the mount table, where root's reach always ended in practice — NFS
root-squash was Unix admitting it), and inside the kernel (the eve bypass is
each device's own policy, not the core's — authority lives at the resource).

`su` is therefore **identity transition under the two rules, never
escalation**: a personality-layer command (`cmd/su.c`, ~50 lines of sugar
over `/proc/self/ctl`) with no password and no setuid machinery, because
there is no root to set. `su user cmd` works when rule 1 or 2 allows it; the
celebrated direction is **`su none` — the privilege-drop shell**, the
primitive the per-agent-namespace aspiration runs on. Killing comes free and
orthogonally: notes through `/proc/<pid>/note` are permitted to eve or a
matching euid — V10's own kill rule.

Two recorded impossibilities, both features: a non-eve user cannot become
anyone (no rule allows it until authentication exists), and **"a shell with
write permission across the whole namespace" is not a grantable thing** —
namespace-wide write is the union of per-server grants fixed at attach time,
so su re-evaluates local authority instantly and can only *request* more from
a server by re-attaching under the new identity (Plan 9's `auth/as` shape).
The namespace unions services; it cannot union their trust.

The earmarked future mechanism is Plan 9's **devcap** (`#¤`,
`/dev/caphash` + `/dev/capuse`): the host owner mints a one-shot capability
`user1@user2@hash`, hands it over, and possession authorizes the switch —
su-without-a-superuser as a third ctl rule ("user X, bearing proof"),
sequenced with `/net` and factotum-shaped authentication. Until then, uids
remain per-server names: the attach-time `uname` records who a mount speaks
for, and the personality owns any name↔number mapping.

## What a "user" is (2026-08-29)

The uid machinery above is mechanism; this section records what the names
*mean*. Unix's uid conflated a person at a terminal (a billing construct
before it was a security boundary), a protection domain, a service principal
(the daemon users — `lp`, `uucp`, `bin` — never logged into, and the part of
the design that aged best), and root. IPNX resolves the conflation without
adding kernel mechanism:

- **The person is eve** — exactly one per kernel instance, many instances
  per person. The kernel instance is the modern terminal; timesharing is
  inverted rather than restored, because kernels now cost a browser tab:
  multi-tenancy happens by instance, and the kernel instance is the new
  uid. Multi-user survives where Plan 9 put it — at servers, per attach.
  There is no `login` and no getty: a person does not log into their own
  instance.
- **The role is the daemon user, kept and ennobled** — a name that owns
  resources and is conferred, never authenticated: assumed by `DMSETUID`
  exec, by eve's grant (rule 1), or by a devcap ticket when that lands.
  Unix daemons got a uid; IPNX daemons get a reduced namespace — systemd's
  forty sandboxing directives are a namespace system described one flag at
  a time, and here a daemon's confinement is simply the binds it started
  with.
- **The agent is a role plus a namespace** — the name for the audit trail,
  the namespace for the authority. `none` is the anonymous agent; `su
  none` is its front door.
- **The network person is an authenticated claim, per connection** — no
  global registry; each server believes a proof. Today that is the
  attach-time `uname`; with /net it becomes factotum-shaped tickets. A
  person, across their fleet of instances, is their keyring.

The organising sentence: **names are for accounting; namespaces are for
authority.** `/etc/passwd` remains personality-side plumbing — V10's
numbers, and the curated V10 role names as heritage — and groups (D2)
matter less than they did, because roles absorb most of what groups were
for.

The person's *configuration* — namespace fragments, service mounts,
credentials — is the **profile**, a file tree served by a userspace agent
(the decision log, 2026-08-29): factotum's shape unified with secstore's
store/agent split and the /lib/namespace language, portable across a
person's instances, with secrets on a use-don't-read interface and an AI
agent's identity as a sub-profile. The kernel contributes nothing to it,
which is the point.

## What this closes

Of the ten identity-family calls docs/syscalls.md classed as *design*: the seven uid/gid
calls land above; `umask` is the per-process field; `lstat` and the `link`/`symlink`
family remain with the protocol-extension decision they always shared (9P2000.u/.L both
prove the wire room; choosing extension-vs-mint is its own small task now that the uid
question no longer gates it).

## Deferrals — measured 2026-08-26 (provenance in RESEARCH §3)

- **D1 — closed.** `getuid` returns both ids in two registers (`os/sys4.c:90`;
  `_geteuid` is the same trap plus `movl r1,r0` in `libc/sys/getuid.s`), `setuid` sets
  real and effective together and **silently no-ops on denial**, `setruid` is root-only.
  The personality's libc mirrors exactly that: one ctl write, both fields; denial
  swallowed, not errored.
- **D2 — measured, implementation deferred.** `NGROUPS 32`, `short` gids,
  `NOGROUP`-terminated, `setgroups` root-only. The kernel design (a `groups` list in the
  credential, same ctl vehicle) is unchanged.
- **D3 — closed, and the default stands.** V10's non-root owner cannot give a file away
  (`chown1` requires the uid to be unchanged for non-root; `accowner` + root gates the
  rest), so the PoC's eve-only chown **is** V10 semantics for the uid half; non-root gid
  changes within membership arrive with D2.
- **D4 — closed.** A created file takes the creator's effective uid and gid
  (`os/iget.c:314`), not the directory's — matching what ramfs already does.

## `su`, concretely (2026-09-02)

Once `/home` binds to `/usr/<me>` as a whole tree, `su` stops needing machinery:

```
su mimmy  =  a fresh namespace
             bind /usr/mimmy /home
             apply /home/profile        ← now mimmy's
             set the credentials
```

The third step reassembles every union root — `/bin`, `/lib`, `/type`,
`/template`, `/pkg`, `/profile`, `/credentials` — because **the profile is the
list of binds**, so `su` names none of them. It is not a mechanism; it is
*assemble someone else's namespace*.

**A bind resolves a channel, not a path**, so rebinding `/home` alone does not
retarget `/bin`'s union element — the profile must be re-applied. That failure
would pass a test that checked `/home` and fail in use.

**Downward is free** (`su none`); becoming another person needs the eve/ruid
check, since their credentials were never yours to bind.

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
since `cat /proc/N/ns` answers *what can this process change?* Authorisation is
the eve/ruid check, once, at composition.
