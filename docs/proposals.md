# Proposals — designs awaiting review

**META — a REGISTER, not one of the six questions.** It holds proposed answers
to them — designs written but not reviewed — so that specs carry only what is
endorsed.

**Check `plan9/` before writing anything here.** Most questions that look like
design questions are lookups: Plan 9 built this system and the source is in the
tree. A proposal is for what Plan 9 genuinely does not answer. Everything that
was in this register on 2026-09-18 turned out to be answered at file and line.

## Open

**What should `boot` ask, and what answers it?** — proposed 2026-09-20,
revised the same day (RESEARCH §13.2).

`#ec` exists and is empty. plan9.ini is **the stored answers to the questions
`boot` would otherwise ask**: `bootargs` is the default shown in the `root is
from (...)` prompt (`boot/boot.c:354`, *"create default reply"*), and
`nobootprompt` skips the question — *"Suppress the `root from` prompt and use
root as the answer instead"* (`plan9.ini(8)`). `user=` does the same for the
user prompt.

**This `boot` asks nothing.** One method — `#9/0`, `bootvirtio9p.c` entire —
no authentication, `rootdir` a `char*` in the file. So there is nothing for a
configuration to answer, and that, rather than any missing mechanism, is why
`#ec` is empty.

The open questions, in order:

1. **Does `boot` get a second method?** A host directory over `#9` is one.
   A different store, a read-only image, a namespace handed over whole — each
   would make `root is from` a real question.
2. **Is there a user to choose?** `eve` is a compile-time constant here where
   Plan 9 starts it empty and has `boot` write `#c/hostowner` from `$user`,
   defaulting to `"glenda"` (`bootauth.c:56`, `pc/main.c:285`).
3. **If there is something to answer, how does it arrive?** Plan 9's own
   second way in is the multiboot branch (`pc/main.c:49`): **the bootloader's
   command line, spaces turned into newlines, IS plan9.ini** — and `ipnx`'s
   argv is that command line. It is an alternative to the FAT file, not an
   override of it (`if(BOOTARGS[0] == 0)`), which is exactly our situation.
   `ipnx` also takes a command to run, so the two would have to share the
   line.

**Nothing here is built.** The mechanism is: `#ec` attaches, binds under `#e`,
and takes writes from eve.

**The scheduler, and what the machine can actually do** — proposed
2026-09-20, from Christine: *"You can control WASM memory allocation and
scheduling from host… Given we are building effectively a custom host
runtime environment, you can scope this out."* The measurements are
RESEARCH §14.

**This is not a deviation, and that is the finding.** `sched()`
(`port/proc.c:119`) divides at `setlabel(&up->sched)` / `gotolabel(
&m->sched)`, which are declared in `port/portfns.h` and implemented in
`pc/l.s:1000`, `:992` — **the scheduler is Plan 9's portable code and the
stack switch is the architecture's**, exactly as `touser` is. So there is
nothing to authorise: it is `port/proc.c` arriving, with this machine
supplying what `pc/l.s` supplies there.

`Config::async_support` gives wasmtime a real host-stack fiber; a host
function that suspends returns to the scheduler with the guest's stack
intact. `epoch_interruption` plus `epoch_deadline_async_yield_and_update`
makes a guest **yield and continue** on a timer, which is `hzclock()`
calling `sched()`. `ResourceLimiter::memory_growing` is the machine
deciding a grow, which is why `segbrk`/`brk_` are omitted from the call
list at all. Every API is cited at file and line in §14.

**Six stages, each with an acceptance.** They are ordered so that the one
with no dependencies comes first and the one that needs the most decided
comes last.

| | builds | acceptance |
|---|---|---|
| **S0** | `ResourceLimiter` and the bounds — `memory_reservation`, `max_wasm_stack`, a grow policy | a guest asking for more memory than the machine allows is refused, and says so |
| **S1** | the machine's stack switch: a fiber per process, `async_support`, a single-threaded executor. Cooperative only — a switch at a syscall, no timer | two processes alternate at a syscall, and `/proc` shows both `Running` and `Ready` |
| **S2** | `port/proc.c`'s `Rendez`, `sleep`, `wakeup`, `tsleep`, `ready`, `runproc`, `yield`, and the twelve states | `sleep(2)` is a `tsleep` on a `Rendez` and a second process runs during it — where today it is `delay` and nothing else runs |
| **S3** | preemption: a timer thread incrementing the epoch, the deadline set to yield | a guest in a tight loop does not stop the system |
| **S4** | notes: `postnote`, this machine's answer to `notify(Ureg*)`, `#p/<n>/note`, `RFNOTEG`, `alarm` | `^C` interrupts a command, and rc's `Trapinit` stops being a stub |
| **S5** | `rendezvous` | two processes meet and exchange a value |

**What it unlocks**, and each is a call or a file Plan 9 has and this
kernel refuses today: `sleep` properly, `alarm`, `notify`, `noted`,
`rendezvous`, `RFNOTEG`, `#p/<n>/note`, `#p/<n>/ctl`'s `start`/`stop`/
`waitstop`/`hang`, and `/proc/<n>/status`'s real states.

**What it does NOT unlock:** `rfork(RFPROC)` still cannot return twice. A
fork duplicates an address space *and* a stack; a fiber's stack holds host
frames pointing into the instance and cannot be copied. `procrfork` stays.

### The decisions this needs before anything is written

1. **How does the kernel survive a suspend mid-syscall?** `touser` is
   handed the kernel *"lent for the duration"*, and a blocking call would
   hold that loan while its fiber sleeps. Either the kernel takes `&self`
   with interior mutability throughout (smaller edit, hides re-entrancy
   bugs best), or **the call answers "blocked" and the machine re-enters
   when it is readied** — which is what `sleep()` returning after `wakeup`
   *is*, and so the shape with a counterpart. **Recommended: the second.**
2. **Is preemption in scope (S3), or only cooperative yielding (S1–S2)?**
   Plan 9 preempts. A system whose guests are all its own may not need to,
   and every preemption point is a place the kernel must be consistent.
3. **What is the browser's counterpart?** There is no wasmtime there:
   JSPI, the stack-switching proposal, or a worker per process. A trait
   method named for a fiber is a boundary P7 cannot implement, so S1's
   naming has to answer to `setlabel`/`gotolabel` and not to wasmtime.
   **Should S1 wait for a browser sketch?**
4. **Where does this sit against P6 and P7?** It is not on the path to the
   demo — the demo needs packages and a surface, not concurrency — but S4
   is what makes `^C` work, and a terminal without `^C` is noticeably not
   a terminal.

**Nothing here is built.** `Machine::delay` (`pc/fns.h:23`) is what stands
in for it today, and it is honest for exactly as long as one process is
runnable at a time.

## Decided, and moved into the specs

**The window type system and the manager interface** — proposed and decided
2026-09-18, five questions answered in one reply. It is now
[type.md](type.md)'s *The design*: a type is a plumb rule, a manager is a file
server on a plumb port, one manager per window, the host half inside the
Saranos app, the root window with a manager like any other, and a declared
`verbs` list.

## Answered by reading `plan9/` — 2026-09-18

**Does a per-operation crossing to the host need batching?** No, and it is not
a protocol question. Plan 9 batches in **userspace**: `bufimage`
(`libdraw/init.c:453`) appends into a buffer whose size is `iounit(datafd)`
(`init.c:291`, falling back to 8000) and calls `doflush` when the next
operation will not fit. The library accumulates, the channel's I/O unit sets
the batch, and the kernel sees whole writes.

**What does a window device leave behind in the kernel?** Nothing — Plan 9 has
no window device. `rio` is an ordinary userspace file server: it posts its
channel to `/srv` (`rio/fsys.c:170`), mounts itself at `/mnt/wsys`
(`fsys.c:237`), and binds that over `/dev` with `MBEFORE` (`fsys.c:241`). A
window is a namespace, assembled by a program with `mount` and `bind`.

**What is a posted server called?** rio's own convention: `/srv/riowctl.%s.%d`
— name, user, pid (`fsys.c:152`). `#s` is one table for the whole kernel, and
the pid is what keeps a second instance from colliding.

**What answers `/` at boot, and how does `/dev` get filled?** `#/` is a fixed
table of two entries, `#/` and `boot` (`devroot.c:27`), plus whatever
`addbootfile` (`devroot.c:80`) embedded. Everything else is mounted by the boot
process, and `/dev` is assembled in the shell:

```
# bind all likely devices (#S was bound in boot)
for(i in f t m v L P u U '$' Σ κ)
	/bin/bind -a '#'^$i /dev
```

— `rc/bin/termrc:11–13`. Note `Σ` and `κ`: non-ASCII device letters are
ordinary in Plan 9, which is why `#¤` is not a special case.

*A proposal is written here, reviewed, and then **leaves**. Adding to this file
instead of emptying it is how stale blocks accumulate and how a reader ends up
re-reading settled material to find what actually needs them.*
