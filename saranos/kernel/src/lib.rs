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

/// What the kernel asks the host to do. The host performs it and answers with a
/// [`Reply`] carrying the same tag.
///
/// This list is deliberately short and deliberately dull. Every variant is
/// something a kernel cannot do for itself on any substrate — not something
/// convenient to push outwards. A new variant is a claim that the kernel has
/// grown, and must be argued as one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Instantiate a process image. `exec` is instantiation, so this is `exec`.
    Spawn { tag: u64, pid: Pid, path: String, args: Vec<String> },
    /// A process has ended and the host may reclaim it.
    Reap { pid: Pid, status: String },
    /// Bytes to the console the host owns.
    Console { data: Vec<u8> },
    /// An operation on the host's own storage, reached as `#Z`. Paths are
    /// ROOT-RELATIVE: the host keeps the real root and enforces containment,
    /// because that is the side that can.
    Host { tag: u64, op: HostOp },
    /// Wake the kernel after a delay. Time is the host's to keep.
    Timer { tag: u64, ms: u64 },
    /// Stop.
    Shutdown { status: i32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostOp {
    Meta { path: String },
    Read { path: String, off: u64, n: usize },
    Write { path: String, off: u64, data: Vec<u8> },
    Create { path: String, dir: bool, perm: u32 },
    Remove { path: String },
    ReadDir { path: String },
}

/// The host's answer to an [`Effect`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    Ok { tag: u64 },
    Bytes { tag: u64, data: Vec<u8> },
    Meta { tag: u64, dir: bool, len: u64 },
    Entries { tag: u64, names: Vec<String> },
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

    pub fn console(&mut self, data: &[u8]) {
        self.out.push(Effect::Console { data: data.to_vec() });
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
        // Everything that touches the world leaves as an effect. If this test
        // ever needs a file handle or a socket to pass, the kernel has grown.
        let mut k = Kernel::new();
        k.console(b"hello");
        k.shutdown(0);
        assert_eq!(
            k.take_effects(),
            vec![Effect::Console { data: b"hello".to_vec() }, Effect::Shutdown { status: 0 }]
        );
    }

    #[test]
    fn effects_are_drained_not_accumulated() {
        let mut k = Kernel::new();
        k.console(b"a");
        assert_eq!(k.take_effects().len(), 1);
        assert!(k.take_effects().is_empty());
    }
}
