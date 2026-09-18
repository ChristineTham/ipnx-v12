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

pub trait Machine {
    /// `procsetup` — whatever state this machine needs for a new process,
    /// before anything of it runs.
    fn procsetup(&mut self, pid: Pid) -> Result<(), String>;

    /// `touser` — start the process running. It returns when the process has
    /// finished, carrying the status `exits` would have set.
    fn touser(&mut self, pid: Pid, image: &[u8], args: &[String]) -> Result<String, String>;

    /// `todget(&ticks, &mono)` (`port/tod.c:153`) — nanoseconds since the
    /// epoch, the fast-tick counter, and its frequency.
    ///
    /// **The kernel has a clock**, because Plan 9's does: `todget` is called
    /// from `port/` (`devcons.c:1239`, `devloopback.c:563`) and the numbers
    /// come from the architecture. It is here for the same reason `touser`
    /// is — portable code needs it, and only a machine can supply it.
    fn todget(&mut self) -> Tod;
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
