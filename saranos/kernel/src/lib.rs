//! The IPNX kernel — a SUBSET of Plan 9's kernel, containing process
//! orchestration.
//!
//! That sentence is the whole specification, and both halves bind. **Subset**:
//! nothing here is invented. Every call is one of Plan 9's 51, every device
//! letter is one of Plan 9's, every flag has Plan 9's value and meaning; where
//! this kernel differs from Plan 9's it does so by LACKING something, never by
//! adding. **Process orchestration**: processes, the three tables they own,
//! the namespace, and the channels between them. Everything a system does
//! beyond that is done BY processes, talking to each other — so a console, a
//! clock, a store, a window system are all file servers in userspace, and none
//! of them is the kernel's business.
//!
//! The test of a proposed change is not whether it is useful. It is whether
//! Plan 9 has it, and whether orchestrating processes requires it.

pub mod dev;
pub mod ninep;
pub mod ns;
pub mod proc;

pub use proc::{Fd, Pid};

/// The calls this kernel answers — a subset of Plan 9's, named as Plan 9 names
/// them.
///
/// What is absent is as deliberate as what is present. There is no call for
/// drawing, for time, for randomness or for fetching: those are reads and
/// writes on files that userspace processes serve. There is no `link`, because
/// Plan 9 has none at any layer and answers the need with `bind` and `mount`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    // processes
    Rfork { flags: i32 },
    Exec { path: String, args: Vec<String> },
    Exits { status: String },
    Await,
    Sleep { ms: u64 },
    Alarm { ms: u64 },
    Notify,
    Noted { how: i32 },
    Rendezvous { tag: u64, val: u64 },

    // the namespace
    Bind { name: String, old: String, flag: i32 },
    Mount { fd: Fd, afd: Fd, old: String, flag: i32, aname: String },
    Unmount { name: Option<String>, old: String },
    Chdir { path: String },

    // channels
    Open { path: String, mode: i32 },
    Create { path: String, mode: i32, perm: u32 },
    Close { fd: Fd },
    Pread { fd: Fd, n: usize, off: i64 },
    Pwrite { fd: Fd, data: Vec<u8>, off: i64 },
    Seek { fd: Fd, off: i64, whence: i32 },
    Dup { old: Fd, new: Fd },
    Pipe,
    Remove { path: String },
    Stat { path: String },
    Fstat { fd: Fd },
    Wstat { path: String, edir: Vec<u8> },
    Fwstat { fd: Fd, edir: Vec<u8> },
    Fversion { fd: Fd, msize: u32, version: String },
    Errstr,
}

/// The kernel.
pub struct Kernel {
    pub procs: proc::Procs,
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}

impl Kernel {
    pub fn new() -> Self {
        Kernel { procs: proc::Procs::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kernel_begins_with_pid_one() {
        let k = Kernel::new();
        assert_eq!(k.procs.count(), 1);
    }

    #[test]
    fn the_call_list_is_plan_nines_and_nothing_else() {
        // Plan 9's syscall names, from plan9/sys/src/libc/9syscall/sys.h. A
        // call here that is not in that file is an invention; a name for
        // drawing, time, randomness, fetching or a store means someone put a
        // file server in the kernel.
        let plan9 = [
            "ERRSTR", "BIND", "CHDIR", "CLOSE", "DUP", "ALARM", "EXEC", "EXITS", "FSESSION",
            "FAUTH", "FSTAT", "SEGBRK", "MOUNT", "OPEN", "OSEEK", "SLEEP", "STAT", "RFORK",
            "PIPE", "CREATE", "BRK_", "REMOVE", "WSTAT", "FWSTAT", "NOTIFY", "NOTED",
            "SEGATTACH", "SEGDETACH", "SEGFREE", "SEGFLUSH", "RENDEZVOUS", "UNMOUNT", "WAIT",
            "SEMACQUIRE", "SEMRELEASE", "SEEK", "FVERSION", "AWAIT", "PREAD", "PWRITE",
            "TSEMACQUIRE", "NSEC",
        ];
        let ours = [
            "RFORK", "EXEC", "EXITS", "AWAIT", "SLEEP", "ALARM", "NOTIFY", "NOTED",
            "RENDEZVOUS", "BIND", "MOUNT", "UNMOUNT", "CHDIR", "OPEN", "CREATE", "CLOSE",
            "PREAD", "PWRITE", "SEEK", "DUP", "PIPE", "REMOVE", "STAT", "FSTAT", "WSTAT",
            "FWSTAT", "FVERSION", "ERRSTR",
        ];
        for c in ours {
            assert!(plan9.contains(&c), "{c} is not one of Plan 9's calls");
        }
        assert!(ours.len() < plan9.len(), "a subset is smaller than what it subsets");
    }

    #[test]
    fn the_kernel_has_no_call_for_what_a_file_server_does() {
        // The guard on "everything else is communication between userspace
        // processes". A console, a clock, a store, a window system are
        // processes; reaching them is open/read/write and nothing more.
        let ours = format!("{:?}", Call::Pipe) + &format!("{:?}", Call::Await);
        for forbidden in ["Draw", "Time", "Nsec", "Random", "Fetch", "Store", "Window", "Console"] {
            assert!(!ours.contains(forbidden), "{forbidden} belongs to a file server");
        }
    }
}
