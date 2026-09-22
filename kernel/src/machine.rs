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
    /// **It stands in for a scheduler, and only while one process is
    /// runnable at a time.** `syssleep`'s other branch is `tsleep` on a
    /// `Rendez`, which needs `sched()` and therefore `setlabel`/`gotolabel`
    /// — machine-dependent too (`pc/l.s:1000`, `:992`), and something this
    /// machine could supply. RESEARCH §14 measures what with, §14.1 says
    /// what Plan 9 does with it, and it is `implementation.md`'s P6.
    fn delay(&self, ms: u64);
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
