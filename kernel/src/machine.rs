//! The machine-dependent half.
//!
//! Plan 9 splits `port/` from the architecture directories — `pc/`, `k10/`,
//! `arm/`. Portable code calls functions each machine must supply, declared in
//! that machine's `fns.h`, and `touser` is one of them: `pc/fns.h:173`,
//! `void touser(void*)`, implemented in assembly per machine and called at
//! `pc/main.c:266`. `procsetup(Proc*)` is another (`pc/main.c:742`).
//!
//! This trait is that boundary, with Plan 9's names for it. **Nothing here
//! names a machine** — not WebAssembly, not a module, not an engine. A wasm
//! runtime is one implementation; Dis and the CLR would each be another, and
//! the kernel does not change for any of them, which is the whole point:
//!
//! > *"even then it should be done in a machine independent way as we may want
//! > a non WASM kernel in the future … for example, dis, or .NET CLR"*
//!
//! Where it differs from Plan 9, and why: `touser` there jumps to a stack
//! pointer, because by then the image has been mapped into an address space.
//! A machine whose executable unit is a module has no such step — the image
//! *is* the executable state — so the image is what crosses. That is the
//! adaptation Christine authorised, and it is the only one here.

use crate::proc::Pid;
use crate::{Call, Ret};

/// The way back in. Plan 9's process traps and lands in `syscall()`
/// (`pc/trap.c:665`), which reaches the kernel's tables through globals. A
/// machine here is handed this instead, and the reentrancy is explicit:
/// `touser` is running, and the process inside it is calling back.
pub trait Syscalls {
    fn syscall(&mut self, up: Pid, call: Call) -> Result<Ret, String>;

    /// **Back into a call the process left in the middle.** A call answers
    /// [`Ret::Sched`] when it `sleep`s, `qlock`s or `sched()`s part way; the
    /// machine leaves the process then, as `gotolabel(&m->sched)` does, and
    /// when the scheduler enters it again the machine calls this, which is
    /// `setlabel(&up->sched)` answering 1 (`proc.c:830`): the call carries
    /// on from where it stopped and answers as any call does — possibly
    /// [`Ret::Sched`] again.
    fn resume(&mut self, up: Pid) -> Result<Ret, String>;

    /// **The clock interrupt**, taken while a process runs: a machine's
    /// `clockintr` calls the portable `timerintr` (`kw/clock.c:46`;
    /// `i8253clock` on the PC, `pc/i8253.c:262`), and `trap()`'s tail then
    /// asks whether to `sched()` (`pc/trap.c:438`).
    ///
    /// The answer is that question: `true` is *"up->delaysched"*, and the
    /// machine must then leave the process as `sched()` would — it is
    /// still `Running`, so the scheduler puts it back on the queue. No
    /// process is named, as `timerintr(Ureg*, Tval)` names none: the one
    /// interrupted is `up`, which the kernel already has.
    ///
    /// A machine calls this at `HZ` (`proc::HZ`), or as near as it can.
    /// Calling it early is harmless — `timerintr` fires only what is due —
    /// and calling it late loses ticks, as a late interrupt does on Plan 9.
    fn timerintr(&mut self) -> bool;

    /// `postnote` (`proc.c:981`), which a machine's `trap()` calls for a
    /// fault: *"sys: trap: …"*, `NDebug` (`pc/trap.c:366`).
    fn postnote(&mut self, up: Pid, msg: &str, flag: crate::proc::NoteFlag) -> bool;

    /// **`notify(Ureg*)`'s decision** (`pc/trap.c:788`) — the portable half:
    /// whether the process has a note to take and what taking it means.
    /// Writing the note onto the process's stack and pointing it at the
    /// handler is the machine's half, done with what this answers.
    ///
    /// `at` says where the machine is: returning from a call, where
    /// `syscall()` already decided (`pc/trap.c:773`) and this hands the
    /// decision over; at a clock interrupt's tail (`:443`); or at a fault.
    fn notify(&mut self, up: Pid, at: NoteAt) -> Notify;
}

/// Where a machine asks [`Syscalls::notify`] from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteAt {
    /// The end of a call.
    Syscall,
    /// A clock interrupt's tail, in user mode.
    Clock,
    /// A fault the process cannot continue from.
    Fault,
}

/// What [`Syscalls::notify`] answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notify {
    /// Nothing to take (`return 0`).
    No,
    /// *"ureg->pc = up->notify"* with the note on the stack (`pc/trap.c:857`)
    /// — call the handler at `f` with `msg`. It returns through `noted`.
    Handler { f: u32, msg: String },
    /// `pexit` — the process is gone, and the machine must leave it.
    Pexit,
    /// `procctl`'s *"p->state = Stopped; sched();"* (`proc.c:1516`) — the
    /// process has stopped, and the machine must leave it until `start`.
    Sched,
}

/// **Every method takes `&self`, and that is load-bearing.** Plan 9's machine
/// half is a set of functions, not an object: `touser` is reachable from
/// anywhere and re-entering it is not a special case. Taking `&mut self` here
/// made the kernel lend the machine out for the duration of a process, and an
/// `exec` from inside that process then found no machine to run the new image
/// on — a shell could not start a command. `&self` restores what Plan 9 has:
/// the machine is always there, and a machine needing state of its own keeps
/// it behind a cell.
/// How a process left the processor — the answer [`Machine::gotolabel`]
/// brings back.
#[derive(Debug, PartialEq, Eq)]
pub enum Left {
    /// It called `sched()`: `gotolabel(&m->sched)`. Its state says why —
    /// `Ready` if it yielded, `Wakeme` if it slept.
    Sched,
    /// Its image ran to the end. Plan 9 cannot reach this: a Plan 9 process
    /// leaves by a trap or a syscall and nothing else, and `_start` there
    /// ends in `exits`. Here a module's exported function can simply return,
    /// and a machine must say so rather than leave a process that will never
    /// run again on the queue.
    Exited,
}

pub trait Machine {
    /// `procsetup` — whatever state this machine needs for a new process,
    /// before anything of it runs.
    fn procsetup(&self, pid: Pid) -> Result<(), String>;

    /// `touser` — **give this process its image and its arguments, ready to
    /// start**. It does not run: `sysexec` sets the new image up and returns,
    /// and the process reaches user mode from `syscall()`'s exit, not from
    /// inside `sysexec` (`pc/trap.c:780`, `kexit`).
    ///
    /// Calling it again on a running process is what `exec` is: the old
    /// image is gone and the next [`Machine::gotolabel`] enters the new one.
    fn touser(&self, pid: Pid, image: &[u8], args: &[String]) -> Result<(), String>;

    /// **`gotolabel(&up->sched)`** (`pc/l.s:992`) — enter this process and
    /// run it until it leaves.
    ///
    /// Plan 9's is two instructions: restore SP, push the saved PC, return.
    /// The process comes back out the same way, by its own
    /// `gotolabel(&m->sched)` at the end of `sched()` — **which is this
    /// returning**. So the whole of a scheduler's machine half is here, and
    /// the whole of a machine's is a stack switch.
    ///
    /// It says nothing about WHY the process left, because the kernel
    /// already knows: `sched()` sets the state before it goes, and
    /// `schedinit` reads it on the way back (`proc.c:67`). `Left::Exited` is
    /// the one thing the state cannot say, because a process that ran off
    /// the end of its image never called `exits`.
    ///
    /// `sys` is how the process calls back while it runs, and it is the
    /// whole of what a machine must arrange: turn whatever its trap looks
    /// like into a [`Call`], hand it over, and turn the answer back.
    fn gotolabel(&self, pid: Pid, sys: &mut dyn Syscalls) -> Result<Left, String>;

    /// `todget(&ticks, &mono)` (`port/tod.c:153`) — nanoseconds since the
    /// epoch, the fast-tick counter, and its frequency.
    ///
    /// **The kernel has a clock**, because Plan 9's does: `todget` is called
    /// from `port/` (`devcons.c:1239`, `devloopback.c:563`) and the numbers
    /// come from the architecture. It is here for the same reason `touser`
    /// is — portable code needs it, and only a machine can supply it.
    fn todget(&self) -> Tod;

    /// `delay(int millisecs)` — the machine-dependent wait, declared beside
    /// `touser` in the same list (`pc/fns.h:23`, `port/portfns.h:61`) and
    /// implemented per architecture: the PC spins on the TSC
    /// (`pc/i8253.c:320`, `aamloop`). A hosted machine has a better way and
    /// uses it; the kernel does not know or care which.
    ///
    /// `schedinit`'s idle loop waits in it until the next interrupt is due
    /// — `idlehands()`, which on the PC halts until the clock.
    fn delay(&self, ms: u64);

    /// `*addr` — the `long` at `addr` in the process's memory, which is
    /// what `syssemacquire`'s *"if(*addr < 0)"* and `canacquire`'s
    /// *"value=*addr"* compile to (`sysproc.c:1098`, `:1199`). Plan 9's
    /// kernel reads a user address directly, because the process's
    /// segments are mapped in its address space; on a machine whose
    /// processes' memories are not the kernel's, only the machine can reach
    /// one, so the process is named.
    ///
    /// An address outside the process's memory is `Err`: `okaddr`'s
    /// answer (`fault.c:291`), which the kernel turns into `validaddr`'s
    /// note and `Ebadarg`.
    fn load(&self, pid: Pid, addr: u32) -> Result<i32, String>;

    /// `cmpswap(long *addr, long old, long new)` (`pc/fns.h:16`,
    /// `pc/devarch.c:544`) — Plan 9's machine function too: if `*addr` is
    /// `old`, make it `new` and answer true. Named by process for the reason
    /// [`Machine::load`] is.
    fn cmpswap(&self, pid: Pid, addr: u32, old: i32, new: i32) -> Result<bool, String>;
}

/// What `todget` answers.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct Tod {
    /// Nanoseconds since the epoch.
    pub nsec: u64,
    /// `fastticks` — a monotonic counter.
    pub ticks: u64,
    /// `fasthz` — that counter's frequency.
    pub hz: u64,
}
