# The uid model — the design

*Decided 2026-08-26. This is the item APE called impossible — "setting the userid,
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

## What this closes

Of the ten identity-family calls docs/syscalls.md classed as *design*: the seven uid/gid
calls land above; `umask` is the per-process field; `lstat` and the `link`/`symlink`
family remain with the protocol-extension decision they always shared (9P2000.u/.L both
prove the wire room; choosing extension-vs-mint is its own small task now that the uid
question no longer gates it).

## Deferrals — measurements to take in ../ipnx, not open questions

- **D1**: V10's exact `getuid`/`setuid` return packing and `setruid` semantics, from
  `os/sys1.c` and libc, before the personality libc freezes its ABI.
- **D2**: the gid/groups surface (`setgroups`/`getgroups` width, NGROUPS) — same design,
  measured limits.
- **D3**: whether V10 lets an owner give a file away (`chown` semantics) — enforcement
  default here is eve-only until measured.
- **D4**: group of a created file (creator's gid vs directory's) — V7 says creator's;
  confirm against V10's `maknode`.
