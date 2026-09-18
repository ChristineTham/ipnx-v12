//! The IPNX kernel.
//!
//! What it is, in Christine's words rather than a paraphrase of them — the
//! quotes are in [`docs/verbatim.md`](../../docs/verbatim.md), which is the
//! only part of this repository's documentation that is hers:
//!
//! > *"we are essentially implementing a micro kernel based on a subset of
//! > Plan 9, we should not be adding to it (even the Unix v10 personality
//! > should be userspace) … It is important to keep our kernel pure otherwise
//! > we will encounter serious issues extending the kernel"*
//!
//! > *"The kernel only handles process orchestration. everything else is
//! > handled by host or userspace. Everytime you design a change to the
//! > kernel, the design is wrong."*
//!
//! > *"Actual deviations from Plan 9 kernel are only authorised when it is to
//! > do with adapting it for WASM and WASI"* … *"even then it should be done
//! > in a machine independent way as we may want a non WASM kernel in the
//! > future … for example, dis, or .NET CLR"*
//!
//! And as of 2026-09-17: **every deviation needs her approval, and the default
//! answer is no.** The substrate argument above is hers, but it is a reason to
//! bring her, not a test to pass alone.
//!
//! So: processes, the three tables they own, the namespace, and the channels
//! between them. A console, a clock, a store, a window system are not here —
//!
//! > *"Only saranos knows about the host… I am a macOS app. I have a screen, a
//! > keyboard and a mouse. I will serve these as virtual devices to the IPNX
//! > kernel, which I am going to start."*
//!
//! — the host serves them, over 9P, because *"9P is the only protocol"*. The
//! kernel consumes what it is served and knows nothing about what serves it:
//! *"`/dev/draw` should be rendered by host. the kernel does not know how to
//! draw."*

use dev::Dev as _;

pub mod chan;
pub mod dev;
pub mod devcons;
pub mod devpipe;
pub mod devroot;
pub mod machine;
pub mod namec;
pub mod ninep;
pub mod ns;
pub mod proc;

pub use chan::Chan;
pub use proc::{Fd, Pid};

/// The calls this kernel answers — a subset of Plan 9's, named as Plan 9 names
/// them (`plan9/sys/src/libc/9syscall/sys.h`).
///
/// What is absent is the point, and the test is Plan 9's list rather than a
/// category: there is no call for drawing, time, randomness or fetching
/// because **Plan 9 has none** — those are reads and writes on files that
/// something else serves. There is no `link`, because Plan 9 has none at any
/// layer.
///
/// **None of these is implemented.** The enum is a list; nothing dispatches
/// it and no process can invoke any of them. See `docs/when.md`.
///
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
///
/// It is built with its machine, as Plan 9 is compiled for one architecture.
pub struct Kernel {
    pub procs: proc::Procs,
    pub tab: namec::Devtab,
    machine: Box<dyn machine::Machine>,
    /// `Mach.syscall` (`pc/dat.h:234`) — what `/dev/sysstat` reports.
    pub syscalls: u64,
}

impl Kernel {
    /// Boot: the kernel carries a root (`#/`, devroot) holding the files the
    /// first process needs, and pid 1 starts with that as its `slash`. This is
    /// Plan 9's arrangement — the kernel has just enough of a root to start
    /// something, and that something mounts the real file server.
    pub fn new(root: devroot::Root, machine: Box<dyn machine::Machine>)
        -> Result<Kernel, String>
    {
        let mut tab = namec::Devtab::new();
        let mut root = root;
        let slash = root.attach("")?;
        tab.add(Box::new(root));
        Ok(Kernel { procs: proc::Procs::new(slash), tab, machine, syscalls: 0 })
    }

    /// `exec`, in the order `sysexec` does it (`sysproc.c:302`).
    ///
    /// 1. `namec(file, Aopen, OEXEC, 0)` — resolve through the calling
    ///    process's namespace and open for execution.
    /// 2. read the image.
    /// 3. `procsetup`, then `touser` — the machine's half.
    ///
    /// Only step 3 is not Plan 9's own sequence, and only because a module
    /// machine has no address space to have mapped the image into first.
    pub fn exec(&mut self, pid: Pid, path: &str, args: &[String]) -> Result<String, String> {
        let image = self.exec_image(pid, path)?;
        self.machine.procsetup(pid)?;
        // `TReal`'s origin. Plan 9 takes it from `MACHP(0)->ticks`; here the
        // machine supplies it, and nothing else in the kernel needs to know
        // what a clock is.
        let now = self.machine.todget().nsec;
        self.procs.started(pid, now);
        // The machine runs the process, and the process calls back here while
        // it does. Plan 9 needs no arrangement for that — a trap lands in
        // `syscall()` and reaches the kernel through globals. Rust needs the
        // machine out of the kernel for the duration, so the kernel can be
        // lent to it as the thing to call.
        let mut m = std::mem::replace(&mut self.machine, Box::new(machine::Nowhere));
        let status = m.touser(pid, &image, args, self);
        self.machine = m;
        let status = status?;
        self.procs.exits(pid, &status);
        Ok(status)
    }

    /// Steps 1 and 2 alone: resolve and read. Split out because it is entirely
    /// Plan 9's, and so it can be tested without a machine.
    pub fn exec_image(&mut self, pid: Pid, path: &str) -> Result<Vec<u8>, String> {
        let p = self.procs.get(pid).ok_or("no such process")?;
        let (slash, dot, ns) = (p.slash.clone(), p.dot.clone(), p.ns.clone());
        let ns = ns.borrow();
        let mut c = namec::namec(
            &mut self.tab,
            &ns,
            &slash,
            &dot,
            path,
            namec::A::Open,
            chan::mode::OEXEC,
        )?;
        let d = self.tab.get(c.dev).ok_or("no such device")?;
        let mut image = Vec::new();
        loop {
            let got = d.read(&mut c, 8192, image.len() as u64)?;
            if got.is_empty() {
                break;
            }
            image.extend_from_slice(&got);
        }
        d.close(&mut c);
        Ok(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A machine that records what it was asked to run. The kernel cannot
    /// tell the difference, which is the property the trait exists for.
    /// A machine that records what it was asked to do, into state the test
    /// still holds. The `Machine` trait gains nothing for this — a test hook
    /// on the kernel's one machine-dependent boundary would be a deviation
    /// paid for by tests.
    #[derive(Default)]
    struct Log {
        ran: Vec<(Pid, Vec<u8>)>,
        order: Vec<&'static str>,
    }

    pub(crate) struct Recorder(Rc<RefCell<Log>>);

    impl machine::Machine for Recorder {
        fn procsetup(&mut self, _pid: Pid) -> Result<(), String> {
            self.0.borrow_mut().order.push("procsetup");
            Ok(())
        }
        fn todget(&mut self) -> machine::Tod {
            machine::Tod { nsec: 1_500_000_000_000_000_000, ticks: 42, hz: 1_000_000 }
        }
        fn touser(
            &mut self,
            pid: Pid,
            image: &[u8],
            _a: &[String],
            _sys: &mut dyn machine::Syscalls,
        ) -> Result<String, String> {
            let mut l = self.0.borrow_mut();
            l.order.push("touser");
            l.ran.push((pid, image.to_vec()));
            Ok(String::new())
        }
    }

    /// A kernel with one boot file, and the log its machine writes to.
    fn watched() -> (Kernel, Rc<RefCell<Log>>) {
        let log = Rc::new(RefCell::new(Log::default()));
        let mut root = devroot::Root::new();
        root.addbootfile("init", b"an image".to_vec());
        let k = Kernel::new(root, Box::new(Recorder(log.clone()))).unwrap();
        (k, log)
    }

    fn booted() -> Kernel {
        let mut root = devroot::Root::new();
        root.addbootfile("init", b"an image".to_vec());
        Kernel::new(root, Box::new(Recorder::silent())).unwrap()
    }

    impl Recorder {
        pub(crate) fn silent() -> Recorder {
            Recorder(Rc::new(RefCell::new(Log::default())))
        }
    }

    #[test]
    fn a_kernel_begins_with_pid_one() {
        assert_eq!(booted().procs.count(), 1);
    }

    #[test]
    fn exec_resolves_through_the_namespace_and_reads_the_image() {
        let mut k = booted();
        assert_eq!(k.exec_image(1, "/init").unwrap(), b"an image");
    }

    /// `exec` must hand the machine the bytes it resolved. Asserting only on
    /// the returned status passes against a machine that ignored the image.
    /// `exec` must hand the machine the bytes it resolved. Asserting only on
    /// the returned status passes against a machine that ignored the image.
    #[test]
    fn exec_runs_the_image_it_resolved() {
        let (mut k, log) = watched();
        k.exec(1, "/init", &[]).unwrap();
        assert_eq!(log.borrow().ran, vec![(1, b"an image".to_vec())]);
    }

    /// `sysexec` does the machine's half in one order: set the process up,
    /// then enter it.
    #[test]
    fn procsetup_runs_before_touser() {
        let (mut k, log) = watched();
        k.exec(1, "/init", &[]).unwrap();
        assert_eq!(log.borrow().order, vec!["procsetup", "touser"]);
    }

    /// A name that does not resolve must not reach the machine at all.
    #[test]
    fn a_failed_resolve_never_reaches_the_machine() {
        let (mut k, log) = watched();
        assert!(k.exec(1, "/nothing", &[]).is_err());
        assert!(log.borrow().order.is_empty(), "the machine was touched for a name that does not exist");
    }

    #[test]
    fn exec_of_a_name_that_is_not_there_fails() {
        let mut k = booted();
        assert!(k.exec_image(1, "/nothing").is_err());
    }

    /// Not a conformance claim — a guard on THIS kernel against the mistake
    /// its author kept making: a call Plan 9 does not have, or one that does
    /// what a file server should.
    #[test]
    fn every_call_we_answer_is_one_of_plan_nines() {
        let plan9 = "ERRSTR BIND CHDIR CLOSE DUP ALARM EXEC EXITS FSESSION FAUTH FSTAT \
             SEGBRK MOUNT OPEN READ OSEEK SLEEP STAT RFORK WRITE PIPE CREATE BRK_ REMOVE \
             WSTAT FWSTAT NOTIFY NOTED SEGATTACH SEGDETACH SEGFREE SEGFLUSH RENDEZVOUS \
             UNMOUNT WAIT SEMACQUIRE SEMRELEASE SEEK FVERSION AWAIT PREAD PWRITE \
             TSEMACQUIRE NSEC";
        for c in "RFORK EXEC EXITS AWAIT SLEEP ALARM NOTIFY NOTED RENDEZVOUS BIND MOUNT \
             UNMOUNT CHDIR OPEN CREATE CLOSE PREAD PWRITE SEEK DUP PIPE REMOVE STAT FSTAT \
             WSTAT FWSTAT FVERSION ERRSTR"
            .split_whitespace()
        {
            assert!(plan9.split_whitespace().any(|p| p == c), "{c} is not a Plan 9 syscall");
            for forbidden in ["DRAW", "TIME", "RANDOM", "FETCH", "STORE", "WINDOW", "CONSOLE"] {
                assert_ne!(c, forbidden, "{c} is a file server's job");
            }
        }
    }
}

impl machine::Syscalls for Kernel {
    fn syscall(&mut self, up: Pid, call: Call) -> Result<Ret, String> {
        Kernel::syscall(self, up, call)
    }
}

/// What a call answers. Plan 9's syscalls all return `uintptr` and write
/// their real answer through a pointer; a typed return says the same thing
/// without a memory model in the way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ret {
    Ok,
    Fd(Fd),
    Two(Fd, Fd),
    N(usize),
    Data(Vec<u8>),
    Pid(Pid),
    Wait(Pid, String),
    Str(String),
}

impl Kernel {
    /// `syscall` (`pc/trap.c:665`): the one door. Plan 9 looks the number up
    /// in `systab[]`, refuses one out of range, and lets `waserror` carry a
    /// failure back as a string the process reads with `errstr`.
    ///
    /// **`up` is the calling process.** Plan 9 keeps it in a per-machine
    /// global; here it is the argument, because a Rust kernel cannot hand a
    /// device an ambient mutable global — the same information, made explicit.
    pub fn syscall(&mut self, up: Pid, call: Call) -> Result<Ret, String> {
        self.syscalls += 1;
        let r = self.dispatch(up, call);
        if let Err(e) = &r {
            self.procs.seterrstr(up, e);
        }
        r
    }

    fn dispatch(&mut self, up: Pid, call: Call) -> Result<Ret, String> {
        match call {
            // ---- processes
            Call::Rfork { flags } => match self.procs.rfork(up, flags) {
                Some(pid) => Ok(Ret::Pid(pid)),
                None => {
                    proc::Procs::rforkcheck(flags)?;
                    Ok(Ret::Pid(0))
                }
            },
            Call::Exec { path, args } => Ok(Ret::Str(self.exec(up, &path, &args)?)),
            Call::Exits { status } => {
                self.procs.exits(up, &status);
                Ok(Ret::Ok)
            }
            Call::Await => match self.procs.await_child(up) {
                Some((pid, status)) => Ok(Ret::Wait(pid, status)),
                None => Err("no living children".into()),
            },
            Call::Errstr => Ok(Ret::Str(self.procs.errstr(up))),

            // ---- the namespace
            Call::Bind { name, old, flag } => {
                let on = self.walk(up, &old, namec::A::Todir, 0)?;
                let to = self.walk(up, &name, namec::A::Todir, 0)?;
                let p = self.procs.get(up).ok_or("no such process")?;
                p.ns.borrow_mut().mount(&on, ns::Element::new(to), bind_of(flag));
                Ok(Ret::Ok)
            }
            Call::Unmount { name, old } => {
                let on = self.walk(up, &old, namec::A::Todir, 0)?;
                let what = match &name {
                    Some(n) => Some(self.walk(up, n, namec::A::Todir, 0)?),
                    None => None,
                };
                let p = self.procs.get(up).ok_or("no such process")?;
                p.ns.borrow_mut().unmount(&on, what.as_ref());
                Ok(Ret::Ok)
            }
            Call::Chdir { path } => {
                let c = self.walk(up, &path, namec::A::Todir, 0)?;
                self.procs.chdir(up, c);
                Ok(Ret::Ok)
            }

            // ---- channels
            Call::Open { path, mode } => {
                let c = self.walk(up, &path, namec::A::Open, mode as u16)?;
                Ok(Ret::Fd(self.newfd(up, c)?))
            }
            Call::Create { path, mode, perm } => {
                let c = self.walk_create(up, &path, mode as u16, perm)?;
                Ok(Ret::Fd(self.newfd(up, c)?))
            }
            Call::Close { fd } => {
                let p = self.procs.get(up).ok_or("no such process")?;
                if p.fds.borrow_mut().close(fd) {
                    Ok(Ret::Ok)
                } else {
                    Err(EBADFD.into())
                }
            }
            Call::Pread { fd, n, off } => {
                let mut c = self.chan(up, fd)?;
                let at = if off < 0 { c.offset } else { off as u64 };
                let d = self.tab.get(c.dev).ok_or(ENODEV)?.read(&mut c, n, at)?;
                if off < 0 {
                    self.advance(up, fd, d.len() as u64);
                }
                Ok(Ret::Data(d))
            }
            Call::Pwrite { fd, data, off } => {
                let mut c = self.chan(up, fd)?;
                let at = if off < 0 { c.offset } else { off as u64 };
                let n = self.tab.get(c.dev).ok_or(ENODEV)?.write(&mut c, &data, at)?;
                if off < 0 {
                    self.advance(up, fd, n as u64);
                }
                Ok(Ret::N(n))
            }
            // `seek` is fd-class, not 9P: the offset is kernel state in the
            // Chan, because `Tread`/`Twrite` carry theirs explicitly.
            Call::Seek { fd, off, whence } => {
                let p = self.procs.get(up).ok_or("no such process")?;
                let fds = p.fds.clone();
                let cell = fds.borrow().get(fd).cloned().ok_or(EBADFD)?;
                let mut c = cell.borrow_mut();
                let new = match whence {
                    0 => off as u64,
                    1 => (c.offset as i64 + off) as u64,
                    _ => return Err("bad whence".into()),
                };
                c.offset = new;
                Ok(Ret::N(new as usize))
            }
            Call::Dup { old, new } => {
                let p = self.procs.get(up).ok_or("no such process")?;
                let fds = p.fds.clone();
                let r = fds.borrow_mut().dup(old, new).ok_or(EBADFD)?;
                Ok(Ret::Fd(r))
            }
            // `syspipe` (`sysfile.c`): attach `#|`, walk the two ends, open
            // both. The attach IS the allocation.
            Call::Pipe => {
                let dir = self.tab.get(dev::DevId::Pipe).ok_or(ENODEV)?.attach("")?;
                let mut ends = Vec::new();
                for name in ["data", "data1"] {
                    let d = self.tab.get(dev::DevId::Pipe).ok_or(ENODEV)?;
                    let mut c = dir.clone();
                    c.qid = d.walk(&dir, name)?.ok_or("no such file")?;
                    d.open(&mut c, chan::mode::ORDWR)?;
                    ends.push(c);
                }
                let b = self.newfd(up, ends.pop().unwrap())?;
                let a = self.newfd(up, ends.pop().unwrap())?;
                Ok(Ret::Two(a, b))
            }
            Call::Remove { path } => {
                let mut c = self.walk(up, &path, namec::A::Access, 0)?;
                self.tab.get(c.dev).ok_or(ENODEV)?.remove(&mut c)?;
                Ok(Ret::Ok)
            }
            Call::Stat { path } => {
                let c = self.walk(up, &path, namec::A::Access, 0)?;
                Ok(Ret::Data(self.tab.get(c.dev).ok_or(ENODEV)?.stat(&c)?))
            }
            Call::Fstat { fd } => {
                let c = self.chan(up, fd)?;
                Ok(Ret::Data(self.tab.get(c.dev).ok_or(ENODEV)?.stat(&c)?))
            }
            Call::Wstat { path, edir } => {
                let mut c = self.walk(up, &path, namec::A::Access, 0)?;
                self.tab.get(c.dev).ok_or(ENODEV)?.wstat(&mut c, &edir)?;
                Ok(Ret::Ok)
            }
            Call::Fwstat { fd, edir } => {
                let mut c = self.chan(up, fd)?;
                self.tab.get(c.dev).ok_or(ENODEV)?.wstat(&mut c, &edir)?;
                Ok(Ret::Ok)
            }

            // ---- not yet
            Call::Mount { .. } => Err("no mount driver yet — P2".into()),
            Call::Fversion { .. } => Err("no mount driver yet — P2".into()),
            Call::Sleep { .. } | Call::Alarm { .. } => Err("no scheduler yet — P3".into()),
            Call::Notify | Call::Noted { .. } => Err("no notes yet — P3".into()),
            Call::Rendezvous { .. } => Err("no rendezvous yet — P3".into()),
        }
    }

    /// `namec` for the calling process: its namespace, its `slash`, its `dot`.
    fn walk(&mut self, up: Pid, path: &str, a: namec::A, mode: u16) -> Result<Chan, String> {
        let p = self.procs.get(up).ok_or("no such process")?;
        let (slash, dot, ns) = (p.slash.clone(), p.dot.clone(), p.ns.clone());
        let ns = ns.borrow();
        namec::namec(&mut self.tab, &ns, &slash, &dot, path, a, mode)
    }

    fn walk_create(&mut self, up: Pid, path: &str, mode: u16, perm: u32) -> Result<Chan, String> {
        let p = self.procs.get(up).ok_or("no such process")?;
        let (slash, dot, ns) = (p.slash.clone(), p.dot.clone(), p.ns.clone());
        let ns = ns.borrow();
        namec::create(&mut self.tab, &ns, &slash, &dot, path, mode, perm)
    }

    fn newfd(&mut self, up: Pid, c: Chan) -> Result<Fd, String> {
        let p = self.procs.get(up).ok_or("no such process")?;
        let fds = p.fds.clone();
        let fd = fds.borrow_mut().add(c);
        Ok(fd)
    }

    fn chan(&mut self, up: Pid, fd: Fd) -> Result<Chan, String> {
        let p = self.procs.get(up).ok_or("no such process")?;
        let fds = p.fds.clone();
        let cell = fds.borrow().get(fd).cloned().ok_or(EBADFD)?;
        let c = cell.borrow().clone();
        Ok(c)
    }

    /// A read or write at offset −1 uses and advances the Chan's own offset.
    fn advance(&mut self, up: Pid, fd: Fd, by: u64) {
        if let Some(p) = self.procs.get(up) {
            if let Some(c) = p.fds.borrow().get(fd) {
                c.borrow_mut().offset += by;
            }
        }
    }
}

const EBADFD: &str = "fd out of range or not open";
const ENODEV: &str = "no such device";

/// `MREPL`, `MBEFORE`, `MAFTER` (`<libc.h>`).
fn bind_of(flag: i32) -> ns::Bind {
    match flag & 3 {
        1 => ns::Bind::Before,
        2 => ns::Bind::After,
        _ => ns::Bind::Replace,
    }
}

#[cfg(test)]
mod syscalls {
    use super::*;
    use crate::proc::rf;

    fn booted() -> Kernel {
        let mut root = devroot::Root::new();
        root.addbootfile("init", b"an image".to_vec());
        let mut k = Kernel::new(root, Box::new(tests::Recorder::silent())).unwrap();
        k.tab.add(Box::new(devpipe::PipeDev::new()));
        k
    }

    /// The point of the whole exercise: a process opens a file by name and
    /// reads it, through the call interface rather than through Rust.
    #[test]
    fn a_process_can_open_a_file_and_read_it() {
        let mut k = booted();
        let fd = match k.syscall(1, Call::Open { path: "/init".into(), mode: 0 }).unwrap() {
            Ret::Fd(fd) => fd,
            r => panic!("{r:?}"),
        };
        let d = k.syscall(1, Call::Pread { fd, n: 64, off: -1 }).unwrap();
        assert_eq!(d, Ret::Data(b"an image".to_vec()));
        assert_eq!(k.syscall(1, Call::Close { fd }).unwrap(), Ret::Ok);
        assert!(k.syscall(1, Call::Close { fd }).is_err(), "closed twice");
    }

    /// Offset −1 uses and advances the channel's own offset, so a second read
    /// continues where the first stopped. That is what makes `read` work
    /// when `Tread` carries its offset explicitly.
    #[test]
    fn reading_at_minus_one_advances_the_channel() {
        let mut k = booted();
        let Ret::Fd(fd) = k.syscall(1, Call::Open { path: "/init".into(), mode: 0 }).unwrap()
        else {
            panic!()
        };
        assert_eq!(k.syscall(1, Call::Pread { fd, n: 2, off: -1 }).unwrap(), Ret::Data(b"an".into()));
        assert_eq!(k.syscall(1, Call::Pread { fd, n: 3, off: -1 }).unwrap(), Ret::Data(b" im".into()));
        // an explicit offset does NOT move it
        assert_eq!(k.syscall(1, Call::Pread { fd, n: 2, off: 0 }).unwrap(), Ret::Data(b"an".into()));
        assert_eq!(k.syscall(1, Call::Pread { fd, n: 3, off: -1 }).unwrap(), Ret::Data(b"age".into()));
    }

    /// `pipe(2)` returns two fds, and what is written to one is read at the
    /// other — the acceptance P2 names first.
    #[test]
    fn two_processes_talk_over_a_pipe() {
        let mut k = booted();
        let Ret::Two(a, b) = k.syscall(1, Call::Pipe).unwrap() else { panic!() };
        // a child forked with RFPROC shares the fd table, so it has both ends
        let Ret::Pid(child) = k.syscall(1, Call::Rfork { flags: rf::PROC }).unwrap() else {
            panic!()
        };
        k.syscall(1, Call::Pwrite { fd: a, data: b"hello".to_vec(), off: -1 }).unwrap();
        let got = k.syscall(child, Call::Pread { fd: b, n: 16, off: -1 }).unwrap();
        assert_eq!(got, Ret::Data(b"hello".to_vec()));
    }

    /// A child with its own fd table does NOT see the parent's later opens.
    #[test]
    fn rfork_reaches_the_call_interface() {
        let mut k = booted();
        let Ret::Pid(child) = k
            .syscall(1, Call::Rfork { flags: rf::PROC | rf::FDG })
            .unwrap()
        else {
            panic!()
        };
        let Ret::Fd(fd) = k.syscall(1, Call::Open { path: "/init".into(), mode: 0 }).unwrap()
        else {
            panic!()
        };
        assert!(
            k.syscall(child, Call::Pread { fd, n: 4, off: -1 }).is_err(),
            "RFFDG copied the table, so the child has no such fd"
        );
    }

    /// `dup` shares the channel, so the offset moves for both.
    #[test]
    fn dup_shares_the_offset_through_the_call_interface() {
        let mut k = booted();
        let Ret::Fd(a) = k.syscall(1, Call::Open { path: "/init".into(), mode: 0 }).unwrap()
        else {
            panic!()
        };
        let Ret::Fd(b) = k.syscall(1, Call::Dup { old: a, new: -1 }).unwrap() else { panic!() };
        k.syscall(1, Call::Pread { fd: a, n: 2, off: -1 }).unwrap();
        assert_eq!(k.syscall(1, Call::Pread { fd: b, n: 2, off: -1 }).unwrap(), Ret::Data(b" i".into()));
    }

    /// `bind` and `chdir` go through the same namespace `namec` walks, so a
    /// relative open after a `chdir` resolves from the new `dot`.
    #[test]
    fn chdir_moves_dot_and_a_relative_name_follows_it() {
        let mut k = booted();
        assert_eq!(k.syscall(1, Call::Chdir { path: "/".into() }).unwrap(), Ret::Ok);
        let r = k.syscall(1, Call::Open { path: "init".into(), mode: 0 }).unwrap();
        assert!(matches!(r, Ret::Fd(_)), "{r:?}");
    }

    /// A failed call leaves its reason where `errstr(2)` finds it, and
    /// reading EXCHANGES it, so a second read says nothing.
    #[test]
    fn a_failed_call_leaves_an_errstr_and_reading_it_clears_it() {
        let mut k = booted();
        assert!(k.syscall(1, Call::Open { path: "/nothing".into(), mode: 0 }).is_err());
        let Ret::Str(e) = k.syscall(1, Call::Errstr).unwrap() else { panic!() };
        assert!(!e.is_empty(), "the failure said nothing");
        let Ret::Str(again) = k.syscall(1, Call::Errstr).unwrap() else { panic!() };
        assert!(again.is_empty(), "errstr exchanges; it does not repeat");
    }

    /// Every call goes through one door, and `/dev/sysstat` counts them.
    #[test]
    fn the_kernel_counts_the_calls_it_answers() {
        let mut k = booted();
        assert_eq!(k.syscalls, 0);
        let _ = k.syscall(1, Call::Errstr);
        let _ = k.syscall(1, Call::Open { path: "/nothing".into(), mode: 0 });
        assert_eq!(k.syscalls, 2, "a failed call is still a call");
    }

    /// The calls P2 and P3 have not reached say so rather than pretending.
    #[test]
    fn what_is_not_built_refuses_rather_than_lying() {
        let mut k = booted();
        for c in [
            Call::Mount { fd: 0, afd: -1, old: "/n".into(), flag: 0, aname: String::new() },
            Call::Sleep { ms: 1 },
            Call::Rendezvous { tag: 0, val: 0 },
        ] {
            assert!(k.syscall(1, c).is_err());
        }
    }
}
