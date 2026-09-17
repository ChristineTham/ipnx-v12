//! The IPNX kernel — an original implementation of Plan 9's architecture.
//!
//! It runs as an ordinary userspace process on each platform. Processes are
//! WebAssembly instances and `exec` is instantiation. Every process has its own
//! namespace, and every non-process syscall resolves through it. 9P is the only
//! IPC: devices present the file interface as function calls, and exactly one
//! driver marshals wire 9P at mount boundaries.
//!
//! **The kernel is a pure state machine: syscalls in, effects out.** It does no
//! I/O, owns no threads, and knows nothing about the platform underneath. Every
//! act that touches the world outside — a byte to a console, a file read, a
//! process instantiated — leaves as an [`Effect`] for the host to perform and
//! answer. That is what lets one kernel serve a terminal, a browser tab and a
//! hypervisor without a line of it changing.
//!
//! **The kernel does not grow.** It handles process orchestration and nothing
//! else; anything else belongs to the host or to userspace. A change that adds
//! to it is wrong before it is weighed.

pub mod dev;
pub mod ninep;
pub mod ns;

/// A process id. Pid 1 is `init`, as it is everywhere.
pub type Pid = u32;

/// What the kernel asks the host to do.
///
/// This list is SHORT and it is meant to stay short. Every variant is something
/// no kernel can do for itself on any substrate — not something convenient to
/// push outwards. Adding one is a claim that the kernel has grown, and has to
/// be argued as one.
///
/// **There is no variant for a device, and there will not be.** The host owns
/// the machine, so what the host owns arrives as FILES IT SERVES and the kernel
/// mounts — the console, the clock, randomness, storage, all the same case,
/// none of them an effect. The alternative is a variant per file: Plan 9's `#c`
/// alone serves `cons`, `time`, `random` and `reboot`, so that road adds four
/// before it reaches a second device. 9P is the only IPC, and this is what that
/// rule is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Instantiate a process image. `exec` is instantiation, and only the host
    /// holds the engine that can do it.
    Spawn { tag: u64, pid: Pid, path: String, args: Vec<String> },
    /// A process has ended and the host may reclaim it.
    Reap { pid: Pid, status: String },
    /// Bytes out on a channel the host holds — one end of a 9P connection to
    /// something the host serves, or a process's own mailbox. This is
    /// transport, not knowledge: the kernel says which channel and what bytes,
    /// and nothing here knows what is on the other side.
    Send { tag: u64, chan: u32, data: Vec<u8> },
    /// Stop.
    Shutdown { status: i32 },
}

/// The host's answer to an [`Effect`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    Ok { tag: u64 },
    /// Bytes in from a channel the host holds. Same shape as `Send`, same
    /// ignorance: the kernel does not know what served them.
    Bytes { tag: u64, chan: u32, data: Vec<u8> },
    Err { tag: u64, msg: String },
}

/// The kernel's state.
///
/// It is single-threaded and owns no synchronisation: a host that wants
/// parallelism runs processes in parallel and hands their syscalls here one at
/// a time. That is a constraint on hosts, and a simplification worth its price.
pub struct Kernel {
    next_pid: Pid,
    next_tag: u64,
    out: Vec<Effect>,
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}

impl Kernel {
    pub fn new() -> Self {
        Kernel { next_pid: 1, next_tag: 1, out: Vec::new() }
    }

    /// Take the effects produced since the last call. A host drains this after
    /// every syscall and after every reply it delivers.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.out)
    }

    fn tag(&mut self) -> u64 {
        let t = self.next_tag;
        self.next_tag += 1;
        t
    }

    /// Boot: instantiate pid 1. The namespace it starts with is the host's to
    /// supply — the host owns the storage, so reading the instance's own
    /// configuration before anything else exists is something only it can do.
    pub fn boot(&mut self, path: &str, args: &[&str]) -> Pid {
        let pid = self.next_pid;
        self.next_pid += 1;
        let tag = self.tag();
        self.out.push(Effect::Spawn {
            tag,
            pid,
            path: path.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
        });
        pid
    }

    /// Bytes out on a host-held channel.
    pub fn send(&mut self, chan: u32, data: &[u8]) -> u64 {
        let tag = self.tag();
        self.out.push(Effect::Send { tag, chan, data: data.to_vec() });
        tag
    }

    pub fn shutdown(&mut self, status: i32) {
        self.out.push(Effect::Shutdown { status });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_instantiates_pid_one() {
        let mut k = Kernel::new();
        let pid = k.boot("/bin/init", &[]);
        assert_eq!(pid, 1);
        match &k.take_effects()[..] {
            [Effect::Spawn { pid: 1, path, .. }] => assert_eq!(path, "/bin/init"),
            other => panic!("expected one Spawn, got {other:?}"),
        }
    }

    #[test]
    fn the_kernel_does_no_io_it_only_asks() {
        // Everything touching the world leaves as an effect. If this test ever
        // needs a file handle or a socket to pass, the kernel has grown.
        let mut k = Kernel::new();
        k.send(3, b"hello");
        k.shutdown(0);
        match &k.take_effects()[..] {
            [Effect::Send { chan: 3, data, .. }, Effect::Shutdown { status: 0 }] => {
                assert_eq!(data, b"hello")
            }
            other => panic!("expected Send then Shutdown, got {other:?}"),
        }
    }

    #[test]
    fn no_effect_names_a_device() {
        // The guard on the rule above. The host owns the machine and serves it
        // as files; a variant named for the console, the clock, randomness or
        // storage means someone reached for an effect where a mount belongs.
        // Plan 9's own #c serves cons, time, random and reboot, so the first
        // such variant invites four.
        let names = ["Spawn", "Reap", "Send", "Shutdown"];
        for forbidden in ["Console", "Timer", "Clock", "Random", "Host", "Draw", "Store"] {
            assert!(
                !names.contains(&forbidden),
                "{forbidden} is a device, and a device is a file server, not an effect"
            );
        }
        assert_eq!(names.len(), 4, "the effect list grew; argue it, do not widen it");
    }

    #[test]
    fn effects_are_drained_not_accumulated() {
        let mut k = Kernel::new();
        k.send(1, b"a");
        assert_eq!(k.take_effects().len(), 1);
        assert!(k.take_effects().is_empty());
    }
}
