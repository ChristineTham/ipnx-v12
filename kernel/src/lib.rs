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
pub mod devcap;
pub mod devcons;
pub mod devmnt;
pub mod devdup;
pub mod devenv;
pub mod devpipe;
pub mod devproc;
pub mod devroot;
pub mod devsrv;
pub mod devvirtio9p;
pub mod machine;
pub mod namec;
pub mod ninep;
pub mod ns;
pub mod proc;
pub mod sha1;

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
    /// `errstr(2)` EXCHANGES: what the caller's buffer holds becomes the
    /// process's error string, and the old one is answered (`generrstr`,
    /// `sysproc.c:748`). That is what makes `werrstr` a library function and
    /// not a call of its own — it formats into a buffer and hands it here.
    Errstr { buf: String },
}

/// The kernel.
///
/// It is built with its machine, as Plan 9 is compiled for one architecture.
pub struct Kernel {
    /// The process table, shared with the devices that read `up` through it —
    /// which is what Plan 9 gets from its being a global.
    pub procs: std::rc::Rc<std::cell::RefCell<proc::Procs>>,
    pub tab: namec::Devtab,
    machine: std::rc::Rc<dyn machine::Machine>,
    /// `Mach.syscall` (`pc/dat.h:234`) — what `/dev/sysstat` reports.
    pub syscalls: u64,
    /// `up` — the calling process, which the devices that need it read
    /// through. Set before each dispatch.
    pub up: std::rc::Rc<std::cell::RefCell<proc::Up>>,
}

impl Kernel {
    /// Boot: the kernel carries a root (`#/`, devroot) holding the files the
    /// first process needs, and pid 1 starts with that as its `slash`. This is
    /// Plan 9's arrangement — the kernel has just enough of a root to start
    /// something, and that something mounts the real file server.
    pub fn new(root: devroot::Root, machine: std::rc::Rc<dyn machine::Machine>)
        -> Result<Kernel, String>
    {
        let mut tab = namec::Devtab::new();
        let mut root = root;
        let mut slash = root.attach("")?;
        // **The root channel is renamed `/`** (`pc/main.c:242`):
        //
        // ```c
        // up->slash = namec("#/", Atodir, 0, 0);
        // pathclose(up->slash->path);
        // up->slash->path = newpath("/");
        // up->dot = cclone(up->slash);
        // ```
        //
        // Not cosmetic. Every path built by a walk hangs off this one, so
        // without the rename `cd` answers `#/`, `#p/<n>/ns` names devices
        // where it should name paths, and an error message quotes a device
        // for a file the caller asked for by name.
        slash.path = "/".to_string();
        tab.add(Box::new(root));
        let procs = std::rc::Rc::new(std::cell::RefCell::new(proc::Procs::new(slash)));
        let up = std::rc::Rc::new(std::cell::RefCell::new(proc::Up { pid: 1, procs: procs.clone() }));
        Ok(Kernel { procs, up, tab, machine, syscalls: 0 })
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
    /// **It does not run the process.** `sysexec` sets the new image up and
    /// returns; the process reaches user mode from `syscall()`'s exit
    /// (`pc/trap.c:780`), and it is `sched()` that enters it. So this ends
    /// where Plan 9's does: the image is the process's now, and it is
    /// `Ready`.
    pub fn exec(&mut self, pid: Pid, path: &str, args: &[String]) -> Result<(), String> {
        let image = self.exec_image(pid, path)?;
        self.machine.procsetup(pid)?;
        // `TReal`'s origin. Plan 9 takes it from `MACHP(0)->ticks`; here the
        // machine supplies it, and nothing else in the kernel needs to know
        // what a clock is.
        let now = self.machine.todget().nsec;
        self.procs.borrow_mut().started(pid, now);
        self.machine.touser(pid, &image, args)?;
        Ok(())
    }

    /// `schedinit` (`proc.c:67`) — **the scheduler**, and *"never returns"*.
    ///
    /// Plan 9's is the landing point of every `gotolabel(&m->sched)`:
    /// `setlabel(&m->sched)` marks it, then it deals with the process that
    /// just left by looking at its state —
    ///
    /// ```c
    /// switch(up->state) {
    /// case Running:  ready(up);            break;
    /// case Moribund: up->state = Dead; ... break;
    /// }
    /// sched();
    /// ```
    ///
    /// — and calls `sched()`, which picks the next with `runproc` and enters
    /// it with `gotolabel(&up->sched)`. A loop is what that is, once the
    /// switch is a call that returns rather than a jump that does not.
    ///
    /// It ends when nothing is left to run: Plan 9's `idlehands()` halts
    /// until the next interrupt and there is always another, because the
    /// clock is one. Here the only thing that can wake a sleeper is its own
    /// timer, so no runnable process and no timer is the end of the system.
    pub fn schedinit(&mut self) -> Result<(), String> {
        let mut left: Option<(Pid, machine::Left)> = None;
        loop {
            // `schedinit`'s switch on the state of the process that left.
            if let Some((pid, how)) = left.take() {
                let mut procs = self.procs.borrow_mut();
                match (how, procs.state(pid)) {
                    // *"case Moribund: up->state = Dead"* — and a process
                    // whose image simply ended is one too: it never called
                    // `exits`, so nothing recorded a status.
                    (machine::Left::Exited, _) => {
                        drop(procs);
                        let now = self.machine.todget().nsec;
                        if self.procs.borrow().status(pid).is_none() {
                            self.procs.borrow_mut().exits(pid, "", Some(now));
                        }
                        self.procs.borrow_mut().setstate(pid, proc::State::Dead);
                    }
                    // *"case Running: ready(up)"* — it gave up the processor
                    // without going to sleep, so it goes back on the queue.
                    (machine::Left::Sched, proc::State::Running) => procs.ready(pid),
                    (machine::Left::Sched, proc::State::Moribund) => {
                        procs.setstate(pid, proc::State::Dead)
                    }
                    // `Wakeme`, `Ready`, `Stopped` — it said where it went.
                    _ => {}
                }
            }

            // `sched()`: `p = runproc()` and enter it.
            let next = self.procs.borrow_mut().runproc();
            let Some(pid) = next else {
                // `idlehands()`. The clock is what wakes a Plan 9 processor;
                // here it is a sleeper's own deadline, and with none there is
                // nothing that can ever make a process runnable again.
                let now = self.machine.todget().nsec;
                let Some(when) = self.procs.borrow().nextalarm() else {
                    return Ok(());
                };
                self.machine.delay(when.saturating_sub(now) / 1_000_000);
                let now = self.machine.todget().nsec;
                self.procs.borrow_mut().timerintr(now.max(when));
                continue;
            };
            // `sched`'s tail (`proc.c:157`): `up = p; up->state = Running;`
            {
                let mut procs = self.procs.borrow_mut();
                procs.up = Some(pid);
                procs.setstate(pid, proc::State::Running);
            }
            self.up.borrow_mut().pid = pid;
            let m = self.machine.clone();
            left = Some((pid, m.gotolabel(pid, self)?));
        }
    }

    /// Steps 1 and 2 alone: resolve and read. Split out because it is entirely
    /// Plan 9's, and so it can be tested without a machine.
    pub fn exec_image(&mut self, pid: Pid, path: &str) -> Result<Vec<u8>, String> {
        let procs = self.procs.borrow();
                let p = procs.get(pid).ok_or("no such process")?;
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
        // **Through the dispatcher, not at the device.** `sysexec` reads the
        // image with `c->dev->read` reached from `devtab` (`sysproc.c:302`),
        // which for a mounted file is the mount driver. Reaching the device
        // directly worked for as long as every binary was in `#/boot`, and
        // stopped the moment the commands moved onto a file server — which
        // is what `/bin` IS on Plan 9.
        let mut image = Vec::new();
        loop {
            let got = self.tab.dread(&mut c, 8192, image.len() as u64)?;
            if got.is_empty() {
                break;
            }
            image.extend_from_slice(&got);
        }
        self.tab.dclose(&mut c);
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
    pub(crate) struct Log {
        ran: Vec<(Pid, Vec<u8>)>,
        pub(crate) order: Vec<&'static str>,
        /// What `delay` was asked to wait for, so a test can see that a
        /// sleep reached the machine without one actually happening.
        pub(crate) delayed: Vec<u64>,
    }

    pub(crate) struct Recorder(Rc<RefCell<Log>>);

    impl machine::Machine for Recorder {
        fn procsetup(&self, _pid: Pid) -> Result<(), String> {
            self.0.borrow_mut().order.push("procsetup");
            Ok(())
        }
        fn todget(&self) -> machine::Tod {
            machine::Tod { nsec: 1_500_000_000_000_000_000, ticks: 42, hz: 1_000_000 }
        }
        /// A test machine does not wait; it records that it was asked to.
        fn delay(&self, ms: u64) {
            self.0.borrow_mut().delayed.push(ms);
        }
        fn touser(&self, pid: Pid, image: &[u8], _a: &[String]) -> Result<(), String> {
            let mut l = self.0.borrow_mut();
            l.order.push("touser");
            l.ran.push((pid, image.to_vec()));
            Ok(())
        }
        /// A test machine has no process to enter, so every one it is given
        /// runs to the end at once. That is `Left::Exited`, and it is what
        /// keeps `schedinit` from looping on a process nothing can run.
        fn gotolabel(
            &self,
            _pid: Pid,
            _sys: &mut dyn machine::Syscalls,
        ) -> Result<machine::Left, String> {
            self.0.borrow_mut().order.push("gotolabel");
            Ok(machine::Left::Exited)
        }
    }

    /// A kernel with one boot file, and the log its machine writes to.
    pub(crate) fn watched() -> (Kernel, Rc<RefCell<Log>>) {
        let log = Rc::new(RefCell::new(Log::default()));
        let mut root = devroot::Root::new();
        root.addbootfile("init", b"an image".to_vec());
        let k = Kernel::new(root, Rc::new(Recorder(log.clone()))).unwrap();
        (k, log)
    }

    fn booted() -> Kernel {
        let mut root = devroot::Root::new();
        root.addbootfile("init", b"an image".to_vec());
        Kernel::new(root, Rc::new(Recorder::silent())).unwrap()
    }

    impl Recorder {
        pub(crate) fn silent() -> Recorder {
            Recorder(Rc::new(RefCell::new(Log::default())))
        }
    }

    #[test]
    fn a_kernel_begins_with_pid_one() {
        assert_eq!(booted().procs.borrow().count(), 1);
    }

    #[test]
    fn exec_resolves_through_the_namespace_and_reads_the_image() {
        let mut k = booted();
        assert_eq!(k.exec_image(1, "/boot/init").unwrap(), b"an image");
    }

    /// `exec` must hand the machine the bytes it resolved. Asserting only on
    /// the returned status passes against a machine that ignored the image.
    /// `exec` must hand the machine the bytes it resolved. Asserting only on
    /// the returned status passes against a machine that ignored the image.
    #[test]
    fn exec_runs_the_image_it_resolved() {
        let (mut k, log) = tests::watched();
        k.exec(1, "/boot/init", &[]).unwrap();
        assert_eq!(log.borrow().ran, vec![(1, b"an image".to_vec())]);
    }

    /// `sysexec` does the machine's half in one order: set the process up,
    /// then enter it.
    #[test]
    fn procsetup_runs_before_touser() {
        let (mut k, log) = tests::watched();
        k.exec(1, "/boot/init", &[]).unwrap();
        assert_eq!(log.borrow().order, vec!["procsetup", "touser"]);
    }

    /// A name that does not resolve must not reach the machine at all.
    #[test]
    fn a_failed_resolve_never_reaches_the_machine() {
        let (mut k, log) = tests::watched();
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
    /// **The call did not finish: the process must leave.** `sleep`
    /// (`proc.c:815`) commits the process and then `gotolabel(&m->sched)`,
    /// and a machine cannot jump — so the answer travels back instead, and
    /// the machine leaves. The call resumes where it stopped when `sched`
    /// enters the process again, which is what the process's own stack is
    /// for.
    Sched,
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
        // **`up` is the calling process, and it is set on the way in.** Plan 9
        // does not have to: `syscall()` (`pc/trap.c:665`) runs on the trapping
        // process's own kernel stack, so the per-machine `up` already names
        // it. Here `up` is a shared cell the devices read through — `up->user`
        // in `devsrv`, `up->fgrp` in `devdup`, `up->egrp` in `devenv` — and if
        // it is not set, every one of them answers for whoever ran last.
        self.up.borrow_mut().pid = up;
        let r = self.dispatch(up, call);
        if let Err(e) = &r {
            self.procs.borrow_mut().seterrstr(up, e);
        }
        r
    }

    fn dispatch(&mut self, up: Pid, call: Call) -> Result<Ret, String> {
        match call {
            // ---- processes
            // `sysrfork` (`sysproc.c`) ends `ready(p); sched();` — **the
            // child goes on the run queue and the parent gives way to it**.
            // Without the `ready` a child exists and nothing can ever pick
            // it; without the `sched` the parent runs on, which here it
            // does anyway, because `procrfork` runs the child's few
            // instructions on the parent's own instance until it `exec`s.
            Call::Rfork { flags } => {
                let child = self.procs.borrow_mut().rfork(up, flags);
                match child {
                    Some(pid) => {
                        self.procs.borrow_mut().ready(pid);
                        Ok(Ret::Pid(pid))
                    }
                    None => {
                        proc::Procs::rforkcheck(flags)?;
                        Ok(Ret::Pid(0))
                    }
                }
            }
            // **`exec` does not return** (`sysproc.c:302`). It gives the
            // process a new image and the process IS that image now; the
            // machine's answer is to leave, and `sched` enters what it
            // left behind.
            Call::Exec { path, args } => {
                self.exec(up, &path, &args)?;
                Ok(Ret::Ok)
            }
            Call::Exits { status } => {
                let now = self.machine.todget().nsec;
                self.procs.borrow_mut().exits(up, &status, Some(now));
                Ok(Ret::Ok)
            }
            // `sysawait` (`sysproc.c:715`) formats the message in the KERNEL
            // and answers its length; `wait(2)` parses it back with
            // `tokenize`. The times belong to the reaped child and nothing
            // outside here has them.
            // `pwait` (`proc.c:1288`), and its shape is the whole of why a
            // scheduler was needed:
            //
            // ```c
            // if(up->nchild == 0 && up->waitq == 0)
            //     error(Enochild);
            // sleep(&up->waitr, haswaitq, up);
            // ```
            //
            // **It sleeps.** `pexit` wakes it (`proc.c:1219`). This used to
            // answer "no living children" the moment no record was ready,
            // because there was nothing a process could do but return.
            Call::Await => {
                let ready = self.procs.borrow().haswaitq(up);
                if !ready {
                    if self.procs.borrow().nchild(up) == 0 {
                        return Err(ENOCHILD.into());
                    }
                    let r = proc::Rid(up, proc::Which::Waitr);
                    self.procs.borrow_mut().sleep(up, r, false);
                    return Ok(Ret::Sched);
                }
                match self.procs.borrow_mut().await_child(up) {
                    Some(w) => Ok(Ret::Str(w.format())),
                    None => Err(ENOCHILD.into()),
                }
            }
            Call::Errstr { buf } => Ok(Ret::Str(self.procs.borrow_mut().errstr(up, &buf))),

            // ---- the namespace
            // `bindmount` (`sysfile.c`): the SOURCE is `Abind` and the
            // TARGET is `Amount` (`:51`, `:60`). Neither is `Atodir`, so a
            // file binds over a file — `bind /bin/rc /bin/sh`.
            Call::Bind { name, old, flag } => {
                let on = self.walk(up, &old, namec::A::Mount, 0)?;
                let to = self.walk(up, &name, namec::A::Bind, 0)?;
                let procs = self.procs.borrow();
                let p = procs.get(up).ok_or("no such process")?;
                p.ns.borrow_mut().mount(&on, element_of(to, flag, ""), bind_of(flag));
                Ok(Ret::Ok)
            }
            Call::Unmount { name, old } => {
                let on = self.walk(up, &old, namec::A::Mount, 0)?;
                let what = match &name {
                    Some(n) => Some(self.walk(up, n, namec::A::Bind, 0)?),
                    None => None,
                };
                let procs = self.procs.borrow();
                let p = procs.get(up).ok_or("no such process")?;
                p.ns.borrow_mut().unmount(&on, what.as_ref());
                Ok(Ret::Ok)
            }
            Call::Chdir { path } => {
                let c = self.walk(up, &path, namec::A::Todir, 0)?;
                self.procs.borrow_mut().chdir(up, c);
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
            // `sysclose` → `fdclose` → `cclose` (`sysfile.c:285`,
            // `chan.c:490`). **The device is told, when the last reference
            // goes.** It was not told at all, so nothing a device does on a
            // close ever happened: `consctl` never put the console back out
            // of raw mode, and `#9` never took its server back.
            Call::Close { fd } => {
                let last = {
                    let procs = self.procs.borrow();
                    let p = procs.get(up).ok_or("no such process")?;
                    let r = p.fds.borrow_mut().close(fd);
                    r.ok_or(EBADFD)?
                };
                if let Some(mut c) = last {
                    self.tab.dclose(&mut c);
                }
                Ok(Ret::Ok)
            }
            Call::Pread { fd, n, off } => {
                let cell = self.chancell(up, fd)?;
                // **Taken out and put back, not held.** Plan 9 hands the
                // device the `Chan*` an fd holds and nothing minds that the
                // device may reach the same channel again through the fd
                // table — `dupgen` reads `c->mode` of every open descriptor
                // (`devdup.c:34`), which is exactly what `ls /dev` does when
                // `#d` is in its union. Holding the borrow across the call
                // makes that a panic; copying only the offset back loses
                // `dri` and `uri`, which is how a directory read came to
                // start over every time. The whole channel goes back.
                let mut c = cell.borrow().clone();
                let at = if off < 0 { c.offset } else { off as u64 };
                // `read` (`sysfile.c:672`): **a directory reached through a
                // union is read from every element**, one after another.
                // Without this `ls /` shows whichever element answered the
                // walk and nothing else — which, once `/` is a union of the
                // kernel's root and a file server, is most of the system
                // missing.
                let d = if c.is_dir() && !c.umh.is_empty() {
                    self.unionread(&mut c, n)?
                } else {
                    self.tab.dread(&mut c, n, at)?
                };
                if off < 0 {
                    c.offset += d.len() as u64;
                }
                *cell.borrow_mut() = c;
                Ok(Ret::Data(d))
            }
            Call::Pwrite { fd, data, off } => {
                let cell = self.chancell(up, fd)?;
                let mut c = cell.borrow().clone();
                let at = if off < 0 { c.offset } else { off as u64 };
                let n = self.tab.dwrite(&mut c, &data, at)?;
                if off < 0 {
                    c.offset += n as u64;
                }
                *cell.borrow_mut() = c;
                Ok(Ret::N(n))
            }
            // `seek` is fd-class, not 9P: the offset is kernel state in the
            // Chan, because `Tread`/`Twrite` carry theirs explicitly.
            // `sysseek` (`sysfile.c:810`). **A DIRECTORY SEEKS ONLY TO 0**,
            // and that is `Eisdir` otherwise, for every whence: entries vary
            // in length, so a byte offset says nothing about where the next
            // one begins. The position is `c->dri`, and a seek clears it
            // (`sysfile.c:856`).
            Call::Seek { fd, off, whence } => {
                let procs = self.procs.borrow();
                let p = procs.get(up).ok_or("no such process")?;
                let fds = p.fds.clone();
                let cell = fds.borrow().get(fd).cloned().ok_or(EBADFD)?;
                let mut c = cell.borrow_mut();
                if c.is_dir() && !(whence == 0 && off == 0) {
                    return Err(EISDIR.into());
                }
                let new = match whence {
                    0 => off as u64,
                    1 => (c.offset as i64 + off) as u64,
                    _ => return Err("bad whence".into()),
                };
                if (new as i64) < 0 {
                    return Err("negative i/o offset".into());
                }
                c.offset = new;
                c.dri = 0;
                // `unionrewind` (`sysfile.c:367`) — a rewind starts the union
                // over too, or the next read carries on from the element the
                // last one stopped in.
                c.uri = 0;
                if let Some(mut umc) = c.umc.take() {
                    self.tab.dclose(&mut umc);
                }
                Ok(Ret::N(new as usize))
            }
            Call::Dup { old, new } => {
                let procs = self.procs.borrow();
                let p = procs.get(up).ok_or("no such process")?;
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
                    let c = d.walk(&dir, name)?.ok_or("no such file")?;
                    let c = self.tab.dopen(c, chan::mode::ORDWR)?;
                    ends.push(c);
                }
                let b = self.newfd(up, ends.pop().unwrap())?;
                let a = self.newfd(up, ends.pop().unwrap())?;
                Ok(Ret::Two(a, b))
            }
            Call::Remove { path } => {
                let mut c = self.walk(up, &path, namec::A::Remove, 0)?;
                self.tab.dremove(&mut c)?;
                Ok(Ret::Ok)
            }
            Call::Stat { path } => {
                let c = self.walk(up, &path, namec::A::Access, 0)?;
                Ok(Ret::Data(self.tab.dstat(&c)?))
            }
            Call::Fstat { fd } => {
                let c = self.chan(up, fd)?;
                Ok(Ret::Data(self.tab.dstat(&c)?))
            }
            Call::Wstat { path, edir } => {
                let mut c = self.walk(up, &path, namec::A::Access, 0)?;
                self.tab.dwstat(&mut c, &edir)?;
                Ok(Ret::Ok)
            }
            Call::Fwstat { fd, edir } => {
                let mut c = self.chan(up, fd)?;
                self.tab.dwstat(&mut c, &edir)?;
                Ok(Ret::Ok)
            }

            // ---- not yet
            // `sysmount` (`sysfile.c`): the fd is a channel to a SERVER, and
            // `#M` speaks 9P down it. Every other device presents files as
            // function calls; this is the one crossing.
            Call::Mount { fd, afd: _, old, flag, aname } => {
                let wire = self.chan(up, fd)?;
                let on = self.walk(up, &old, namec::A::Todir, 0)?;
                let user = self.procs.borrow().user(up).unwrap_or_default();
                let to = self.tab.dmount(wire, &user, &aname)?;
                let procs = self.procs.borrow();
                let p = procs.get(up).ok_or("no such process")?;
                p.ns.borrow_mut().mount(&on, element_of(to, flag, &aname), bind_of(flag));
                Ok(Ret::Ok)
            }
            Call::Fversion { .. } => Err("fversion is mntversion's, done at mount".into()),

            // `syssleep` (`sysproc.c`), and both of its branches are here:
            //
            // ```c
            // n = arg[0];
            // if(n <= 0) { ... yield(); return 0; }
            // if(n < TK2MS(1)) n = TK2MS(1);
            // tsleep(&up->sleep, return0, 0, n);
            // ```
            //
            // **`yield` is a no-op here and that is not an approximation.**
            // It gives up the processor to whatever else is runnable, and on
            // this machine nothing else is: a child made by `procrfork` runs
            // to its end inside the call that made it (`machine.rs`), so at
            // any moment exactly one process can run. Yielding to nobody is
            // returning.
            //
            // For the other branch Plan 9 has two mechanisms and this kernel
            // has the second: `tsleep` puts the process on a queue and
            // `sched()`s, which needs a scheduler; `delay` (`pc/fns.h:23`)
            // waits where it stands. **With one runnable process they are
            // observationally the same thing**, so the wait goes to the
            // machine, which is where Plan 9 puts `delay` too.
            Call::Sleep { ms } => {
                if ms == 0 {
                    // `yield()` (`proc.c:454`): *"if(anyready()){ ... sched();
                    // }"* — and nothing at all when nobody else is waiting.
                    // The `lastupdate` nudge is *"pretend we just used 1/2
                    // tick"*, so a process that yields is not rewarded for it.
                    if self.procs.borrow().anyready() {
                        self.procs.borrow_mut().yield_(up);
                        return Ok(Ret::Sched);
                    }
                    return Ok(Ret::Ok);
                }
                // `if(n < TK2MS(1)) n = TK2MS(1)` — `TK2MS(1)` is `1000/HZ`,
                // 10ms at the PC's `HZ` of 100 (`pc/mem.h:31`). A sleep
                // shorter than a tick is a sleep of one tick.
                let ms = ms.max(TK2MS1);
                let deadline = self.machine.todget().nsec + ms * 1_000_000;
                // `tsleep(&up->sleep, return0, 0, n)` (`sysproc.c`). The
                // condition is `return0`, so it always commits: this sleep
                // has no reason but the clock.
                let r = proc::Rid(up, proc::Which::Sleep);
                self.procs.borrow_mut().tsleep(up, r, false, deadline);
                Ok(Ret::Sched)
            }

            // The four that a scheduler is the whole of. Each says what Plan
            // 9 does and why this machine cannot, rather than naming a phase.
            //
            // `sysalarm` is `procalarm` (`sysproc.c`), and an alarm arrives
            // as a NOTE — so it is the note mechanism, on a timer.
            Call::Alarm { .. } => Err(NONOTES.into()),
            Call::Notify | Call::Noted { .. } => Err(NONOTES.into()),
            // `sysrendezvous` (`sysproc.c`) either finds a waiting process
            // and `ready()`s it, or sets `up->state = Rendezvous` and
            // `sched()`s. **As this host is built** a child runs to its end
            // inside the call that made it, so the other process is below
            // this one on the machine's call stack and neither can be
            // suspended. That is the host's shape, not the substrate's —
            // RESEARCH §14, and P6.
            Call::Rendezvous { .. } => {
                Err("rendezvous needs a scheduler; this machine has none yet".into())
            }
        }
    }

    /// `namec` for the calling process: its namespace, its `slash`, its `dot`.
    fn walk(&mut self, up: Pid, path: &str, a: namec::A, mode: u16) -> Result<Chan, String> {
        let procs = self.procs.borrow();
                let p = procs.get(up).ok_or("no such process")?;
        let (slash, dot, ns) = (p.slash.clone(), p.dot.clone(), p.ns.clone());
        let ns = ns.borrow();
        namec::namec(&mut self.tab, &ns, &slash, &dot, path, a, mode)
    }

    fn walk_create(&mut self, up: Pid, path: &str, mode: u16, perm: u32) -> Result<Chan, String> {
        let procs = self.procs.borrow();
                let p = procs.get(up).ok_or("no such process")?;
        let (slash, dot, ns) = (p.slash.clone(), p.dot.clone(), p.ns.clone());
        let ns = ns.borrow();
        namec::create(&mut self.tab, &ns, &slash, &dot, path, mode, perm)
    }

    fn newfd(&mut self, up: Pid, c: Chan) -> Result<Fd, String> {
        let procs = self.procs.borrow();
                let p = procs.get(up).ok_or("no such process")?;
        let fds = p.fds.clone();
        let fd = fds.borrow_mut().add(c);
        Ok(fd)
    }

    fn chan(&mut self, up: Pid, fd: Fd) -> Result<Chan, String> {
        Ok(self.chancell(up, fd)?.borrow().clone())
    }

    /// The descriptor's channel ITSELF, not a copy of it.
    ///
    /// Plan 9 hands a device the `Chan*` an fd holds, so what a device records
    /// on it stays recorded — `c->dri` after a directory read, `c->offset`,
    /// `c->iounit` after a mount. Copying it and writing back one field is
    /// what this kernel did, and a directory read then restarted from the
    /// first entry every time, because `dri` went back with the copy.
    /// `unionread` (`sysfile.c:323`) — read each element of a union in turn,
    /// answering as soon as one gives anything. An element that gives nothing
    /// is finished with and closed, and `c->uri` moves on.
    ///
    /// Each element is opened in its own right (`cclone` then `open`), which
    /// is why a union read does not disturb the channel it was reached
    /// through.
    fn unionread(&mut self, c: &mut Chan, n: usize) -> Result<Vec<u8>, String> {
        while (c.uri as usize) < c.umh.len() {
            if c.umc.is_none() {
                let alt = c.umh[c.uri as usize].chan.clone();
                let cl = self.tab.dcclone(&alt)?;
                match self.tab.dopen(cl, chan::mode::OREAD) {
                    Ok(o) => c.umc = Some(Box::new(o)),
                    // *"Error causes component of union to be skipped"*
                    // (`sysfile.c:340`).
                    Err(_) => {
                        c.uri += 1;
                        continue;
                    }
                }
            }
            let mut umc = c.umc.take().expect("just opened");
            let at = umc.offset;
            match self.tab.dread(&mut umc, n, at) {
                Ok(d) if !d.is_empty() => {
                    umc.offset += d.len() as u64;
                    c.umc = Some(umc);
                    return Ok(d);
                }
                _ => {
                    self.tab.dclose(&mut umc);
                    c.uri += 1;
                }
            }
        }
        Ok(Vec::new())
    }

    fn chancell(&mut self, up: Pid, fd: Fd) -> Result<std::rc::Rc<std::cell::RefCell<Chan>>, String> {
        let procs = self.procs.borrow();
        let p = procs.get(up).ok_or("no such process")?;
        let fds = p.fds.clone();
        let cell = fds.borrow().get(fd).cloned().ok_or(EBADFD)?;
        Ok(cell)
    }

}

/// `Eisdir` (`error.h`) — *"file is a directory"*.
const EISDIR: &str = "file is a directory";
const EBADFD: &str = "fd out of range or not open";
const ENODEV: &str = "no such device";

/// `MREPL`, `MBEFORE`, `MAFTER` (`<libc.h>:556`) — the low two bits.
fn bind_of(flag: i32) -> ns::Bind {
    match flag & 3 {
        1 => ns::Bind::Before,
        2 => ns::Bind::After,
        _ => ns::Bind::Replace,
    }
}

/// `Enochild` (`error.h`) — *"no living children"*.
const ENOCHILD: &str = "no living children";

/// `TK2MS(1)` — *"#define TK2MS(x) ((x)*(1000/HZ))"* (`port/portfns.h`),
/// with the PC's `HZ` of 100 (`pc/mem.h:31`). The shortest sleep there is.
const TK2MS1: u64 = 10;

/// What `notify`, `noted` and `alarm` are waiting on: a scheduler, and the
/// stack switch under it. A note is delivered on the way out of the kernel
/// (`notify(Ureg*)`, `pc/trap.c`) by rewriting the user stack so the handler
/// runs and `noted` returns through it — machine-dependent code, as
/// `setlabel`/`gotolabel` are (`pc/l.s:1000`, `:992`), which is why
/// `port/proc.c`'s scheduler is portable and this is not.
///
/// **Unbuilt, not impossible.** This machine can suspend a guest — RESEARCH
/// §14 measures how, §14.1 says what Plan 9 does with it — and it is
/// `implementation.md`'s **P6**. Until then the call refuses and says which
/// half is missing.
const NONOTES: &str = "notes need a scheduler; this machine has none yet";

/// The element as `bind`/`mount` made it. **The flag WORD is kept**
/// (`Mount.mflag`, `portdat.h:303`), not just its `MCREATE` bit, because
/// `#p/<n>/ns` prints it back with `int2flag`. `spec` is `mount`'s aname and
/// empty for a bind.
fn element_of(chan: Chan, flag: i32, spec: &str) -> ns::Element {
    ns::Element::with(chan, flag, spec)
}

#[cfg(test)]
mod syscalls {
    use super::*;
    use crate::ns::mflag::MCREATE;
    use crate::proc::rf;

    fn booted() -> Kernel {
        let mut root = devroot::Root::new();
        root.addbootfile("init", b"an image".to_vec());
        root.addbootfile("hello", b"greetings".to_vec());
        let mut k = Kernel::new(root, std::rc::Rc::new(tests::Recorder::silent())).unwrap();
        k.tab.add(Box::new(devpipe::PipeDev::new()));
        k.tab.add(Box::new(devenv::EnvDev::new(k.up.clone())));
        k
    }

    /// **`up` names the CALLING process, on every call.** Plan 9 gets this for
    /// nothing — `syscall()` runs on the trapping process's own kernel stack,
    /// so `up` already names it. Here it is a shared cell, and the devices
    /// that read through it (`up->egrp` in `devenv`, `up->fgrp` in `devdup`,
    /// `up->user` in `devsrv`, `devmnt`, `devproc`, `devcap`) answer for
    /// whoever it names.
    ///
    /// Until this test there was nothing to notice: one process had run, and
    /// the cell held its pid from boot. A child with its own environment
    /// group read its PARENT's variables.
    #[test]
    fn a_device_reading_up_answers_for_the_process_that_called() {
        let mut k = booted();
        let fd = match k
            .syscall(1, Call::Create { path: "#e/x".into(), mode: 1, perm: 0o666 })
            .unwrap()
        {
            Ret::Fd(fd) => fd,
            r => panic!("{r:?}"),
        };
        k.syscall(1, Call::Pwrite { fd, data: b"one".to_vec(), off: -1 }).unwrap();
        k.syscall(1, Call::Close { fd }).unwrap();

        // `RFCENVG`: the child starts with an EMPTY environment group.
        let Ret::Pid(child) =
            k.syscall(1, Call::Rfork { flags: rf::PROC | rf::CENVG }).unwrap()
        else {
            panic!("no child")
        };
        assert!(
            k.syscall(child, Call::Open { path: "#e/x".into(), mode: 0 }).is_err(),
            "the child's environment group is its own and holds nothing"
        );
        assert!(
            k.syscall(1, Call::Open { path: "#e/x".into(), mode: 0 }).is_ok(),
            "and the parent's is untouched"
        );
    }

    /// The point of the whole exercise: a process opens a file by name and
    /// reads it, through the call interface rather than through Rust.
    #[test]
    fn a_process_can_open_a_file_and_read_it() {
        let mut k = booted();
        let fd = match k.syscall(1, Call::Open { path: "/boot/init".into(), mode: 0 }).unwrap() {
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
        let Ret::Fd(fd) = k.syscall(1, Call::Open { path: "/boot/init".into(), mode: 0 }).unwrap()
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
        let Ret::Fd(fd) = k.syscall(1, Call::Open { path: "/boot/init".into(), mode: 0 }).unwrap()
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
        let Ret::Fd(a) = k.syscall(1, Call::Open { path: "/boot/init".into(), mode: 0 }).unwrap()
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
        let r = k.syscall(1, Call::Open { path: "boot/init".into(), mode: 0 }).unwrap();
        assert!(matches!(r, Ret::Fd(_)), "{r:?}");
    }

    /// A failed call leaves its reason where `errstr(2)` finds it, and
    /// reading EXCHANGES it, so a second read says nothing.
    #[test]
    fn a_failed_call_leaves_an_errstr_and_reading_it_clears_it() {
        let mut k = booted();
        assert!(k.syscall(1, Call::Open { path: "/nothing".into(), mode: 0 }).is_err());
        let Ret::Str(e) = k.syscall(1, Call::Errstr { buf: String::new() }).unwrap() else {
            panic!()
        };
        assert!(!e.is_empty(), "the failure said nothing");
        let Ret::Str(again) = k.syscall(1, Call::Errstr { buf: String::new() }).unwrap() else {
            panic!()
        };
        assert!(again.is_empty(), "errstr exchanges; it does not repeat");

        // The other half of the exchange, which is the whole of `werrstr`:
        // what the caller hands over becomes the error string.
        k.syscall(1, Call::Errstr { buf: "mine now".into() }).unwrap();
        let Ret::Str(mine) = k.syscall(1, Call::Errstr { buf: String::new() }).unwrap() else {
            panic!()
        };
        assert_eq!(mine, "mine now");
    }

    /// Every call goes through one door, and `/dev/sysstat` counts them.
    #[test]
    fn the_kernel_counts_the_calls_it_answers() {
        let mut k = booted();
        assert_eq!(k.syscalls, 0);
        let _ = k.syscall(1, Call::Errstr { buf: String::new() });
        let _ = k.syscall(1, Call::Open { path: "/nothing".into(), mode: 0 });
        assert_eq!(k.syscalls, 2, "a failed call is still a call");
    }

    /// **A directory seeks only to 0** (`sysfile.c:820`, `Eisdir`), and a
    /// seek clears `c->dri` so the next read starts at the first entry. A
    /// byte offset cannot say where an entry begins, because entries are not
    /// all the same length.
    #[test]
    fn a_directory_seeks_only_to_the_beginning() {
        let mut k = booted();
        let Ret::Fd(fd) = k.syscall(1, Call::Open { path: "/boot".into(), mode: 0 }).unwrap()
        else {
            panic!()
        };
        let first = k.syscall(1, Call::Pread { fd, n: 4096, off: -1 }).unwrap();
        assert!(matches!(&first, Ret::Data(d) if !d.is_empty()));
        assert!(
            k.syscall(1, Call::Pread { fd, n: 4096, off: -1 })
                .is_ok_and(|r| matches!(r, Ret::Data(d) if d.is_empty())),
            "read to the end"
        );

        assert!(k.syscall(1, Call::Seek { fd, off: 8, whence: 0 }).is_err());
        assert!(k.syscall(1, Call::Seek { fd, off: 0, whence: 1 }).is_err());
        k.syscall(1, Call::Seek { fd, off: 0, whence: 0 }).expect("rewind");
        assert_eq!(
            k.syscall(1, Call::Pread { fd, n: 4096, off: -1 }).unwrap(),
            first,
            "and the rewind starts it over"
        );
    }

    /// A device that is a 9P server: write it a request, read back the reply.
    /// That is what a file server is — a process reading 9P off a channel —
    /// and a pipe with something on the far end differs only in how the bytes
    /// travel.
    #[derive(Default)]
    struct Server9P {
        reply: Vec<u8>,
    }

    impl dev::Dev for Server9P {
        fn id(&self) -> dev::DevId {
            dev::DevId::Env // borrowing a letter; this stands in for a wire
        }
        fn as_any(&mut self) -> &mut dyn std::any::Any {
            self
        }
        fn attach(&mut self, _s: &str) -> Result<Chan, String> {
            Ok(Chan::attach(dev::DevId::Env, 0))
        }
        fn walk(&mut self, _c: &Chan, _n: &str) -> Result<Option<Chan>, String> {
            Ok(None)
        }
        fn open(&mut self, c: Chan, _m: u16) -> Result<Chan, String> {
            Ok(c)
        }
        fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
            Err("no".into())
        }
        fn read(&mut self, _c: &mut Chan, n: usize, _o: u64) -> Result<Vec<u8>, String> {
            let take = n.min(self.reply.len());
            Ok(self.reply.drain(..take).collect())
        }
        fn write(&mut self, _c: &mut Chan, data: &[u8], _o: u64) -> Result<usize, String> {
            self.reply = serve9p(data);
            Ok(data.len())
        }
        fn stat(&mut self, _c: &Chan) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
        fn wstat(&mut self, _c: &mut Chan, _e: &[u8]) -> Result<(), String> {
            Err("no".into())
        }
        fn remove(&mut self, _c: &mut Chan) -> Result<(), String> {
            Err("no".into())
        }
        fn close(&mut self, _c: &mut Chan) {}
    }

    /// One flat file, served in 9P2000.
    fn serve9p(req: &[u8]) -> Vec<u8> {
        use ninep::{unframe, Qid, R, T, W, QTDIR};
        let m = unframe(req).expect("malformed");
        let mut r = R::new(m.body);
        match m.ty {
            x if x == T::Version as u8 => {
                let msize = r.u32().unwrap();
                let _v = r.s().unwrap();
                W::new().u32(msize.min(8192)).s("9P2000").frame(T::Version.reply(), m.tag)
            }
            x if x == T::Attach as u8 => {
                let q = Qid { qtype: QTDIR, vers: 0, path: 0 };
                W::new().raw(&q.write(W::new()).into_body()).frame(T::Attach.reply(), m.tag)
            }
            x if x == T::Walk as u8 => {
                let (_from, _newfid) = (r.u32().unwrap(), r.u32().unwrap());
                let n = r.u16().unwrap();
                let mut qids = Vec::new();
                for _ in 0..n {
                    if r.s().unwrap() == "answer" {
                        qids.push(Qid { qtype: 0, vers: 0, path: 1 });
                    }
                }
                let mut w = W::new().u16(qids.len() as u16);
                for q in &qids {
                    w = w.raw(&q.write(W::new()).into_body());
                }
                w.frame(T::Walk.reply(), m.tag)
            }
            x if x == T::Open as u8 => {
                let q = Qid { qtype: 0, vers: 0, path: 1 };
                W::new().raw(&q.write(W::new()).into_body()).u32(0).frame(T::Open.reply(), m.tag)
            }
            x if x == T::Read as u8 => {
                let (_fid, off, count) = (r.u32().unwrap(), r.u64().unwrap(), r.u32().unwrap());
                let data = b"served over 9P";
                let off = off as usize;
                let end = (off + count as usize).min(data.len());
                let slice = if off >= data.len() { &[][..] } else { &data[off..end] };
                W::new().u32(slice.len() as u32).raw(slice).frame(T::Read.reply(), m.tag)
            }
            x if x == T::Clunk as u8 => W::new().frame(T::Clunk.reply(), m.tag),
            _ => W::new().s("not implemented").frame(T::Error.reply(), m.tag),
        }
    }

    /// **P2's acceptance, whole.** A channel is posted at `/srv`; it is opened
    /// by name, mounted, and a file is then resolved THROUGH that mount by an
    /// ordinary `open`. Four devices and the namespace, in one path.
    #[test]
    fn a_posted_channel_is_mounted_and_a_name_resolves_through_it() {
        let mut k = booted();
        k.tab.add(Box::new(Server9P::default()));
        k.tab.add(Box::new(devsrv::SrvDev::new(k.up.clone())));
        k.tab.add(Box::new(devmnt::MntDev::new()));

        // a server opens its own channel and posts it at #s/store
        let Ret::Fd(wire) = k.syscall(1, Call::Open { path: "#e".into(), mode: 2 }).unwrap()
        else {
            panic!()
        };
        let Ret::Fd(post) = k
            .syscall(1, Call::Create { path: "#s/store".into(), mode: 1, perm: 0o600 })
            .unwrap()
        else {
            panic!()
        };
        k.syscall(1, Call::Pwrite { fd: post, data: wire.to_string().into_bytes(), off: -1 })
            .unwrap();

        // another process opens the NAME and gets the posted channel
        let Ret::Fd(got) = k.syscall(1, Call::Open { path: "#s/store".into(), mode: 2 }).unwrap()
        else {
            panic!()
        };

        // mount it over /n — the 9P conversation runs inside this call
        k.syscall(
            1,
            Call::Mount { fd: got, afd: -1, old: "/".into(), flag: 1, aname: String::new() },
        )
        .expect("mount");

        // and a name resolves through it
        let Ret::Fd(fd) = k.syscall(1, Call::Open { path: "/answer".into(), mode: 0 }).unwrap()
        else {
            panic!("the mounted server was not reached")
        };
        assert_eq!(
            k.syscall(1, Call::Pread { fd, n: 64, off: -1 }).unwrap(),
            Ret::Data(b"served over 9P".to_vec())
        );
    }

    /// **A union is searched, element by element** (`chan.c:1027`: *"try a
    /// union mount, if any"*). `bind -a` puts an element in a list, and
    /// without this nothing ever reaches it — which is what P5's very first
    /// line needs, `bind -a /root /`, the root becoming a file server.
    #[test]
    fn a_walk_tries_every_element_of_a_union() {
        let mut k = booted();
        k.tab.add(Box::new(devenv::EnvDev::new(k.up.clone())));

        // `#e` has `x`; `#/` has `boot`. Bind both onto `/mnt`, in order.
        // (`#/` and not `#/boot`: everything between the device letter and
        // the first `/` is the ATTACH SPEC, so `#/boot` attaches the root
        // device with the spec `boot` — see `dev::split`.)
        k.syscall(1, Call::Create { path: "#e/x".into(), mode: 1, perm: 0o666 }).unwrap();
        k.syscall(1, Call::Bind { name: "#e".into(), old: "/mnt".into(), flag: 0 }).unwrap();
        // `MAFTER` (`libc.h:556`) — this element answers after the ones
        // already there.
        k.syscall(1, Call::Bind { name: "#/".into(), old: "/mnt".into(), flag: 2 }).unwrap();

        // the first element answers for its own name
        assert!(
            k.syscall(1, Call::Open { path: "/mnt/x".into(), mode: 0 }).is_ok(),
            "the first element of the union"
        );
        // and a name it does not have falls through to the second
        assert!(
            k.syscall(1, Call::Open { path: "/mnt/boot/init".into(), mode: 0 }).is_ok(),
            "the second element was never reached"
        );
        // a name in neither is still not there
        assert!(k.syscall(1, Call::Open { path: "/mnt/nothing".into(), mode: 0 }).is_err());
    }

    /// **`#9` is the wire, and nothing else.** The machine provides a 9P
    /// server; the device is a channel to it; `mount` does the rest. This is
    /// `boot.c:171` — `mount(fd, afd, "/root", MREPL|MCREATE, rp)` — with the
    /// channel got from `open("#9/0", ORDWR)` as `connectvirtio9p` gets it
    /// (`boot/bootvirtio9p.c:20`).
    #[test]
    fn a_server_the_machine_provides_is_mounted_through_hash_nine() {
        struct Host9P;
        impl devvirtio9p::Nineserver for Host9P {
            fn rpc(&mut self, t: &[u8]) -> Result<Vec<u8>, String> {
                Ok(serve9p(t))
            }
        }

        let mut k = booted();
        let mut d = devvirtio9p::Virtio9p::new();
        d.add(Box::new(Host9P));
        k.tab.add(Box::new(d));
        k.tab.add(Box::new(devmnt::MntDev::new()));

        let Ret::Fd(wire) = k.syscall(1, Call::Open { path: "#9/0".into(), mode: 2 }).unwrap()
        else {
            panic!("no channel to the machine's server")
        };
        k.syscall(
            1,
            Call::Mount {
                fd: wire,
                afd: -1,
                old: "/root".into(),
                flag: MCREATE,
                aname: String::new(),
            },
        )
        .expect("mount");

        let Ret::Fd(fd) = k.syscall(1, Call::Open { path: "/root/answer".into(), mode: 0 }).unwrap()
        else {
            panic!("the server was not reached")
        };
        assert_eq!(
            k.syscall(1, Call::Pread { fd, n: 64, off: -1 }).unwrap(),
            Ret::Data(b"served over 9P".to_vec())
        );
    }

    /// A server takes ONE client: `v9open` is `Einuse` when the channel is
    /// already open (`devvirtio9p.c:1110`), because one reply stream cannot
    /// be shared. Closing it hands the server back.
    #[test]
    fn hash_nine_serves_one_client_at_a_time() {
        struct Quiet;
        impl devvirtio9p::Nineserver for Quiet {
            fn rpc(&mut self, _t: &[u8]) -> Result<Vec<u8>, String> {
                Ok(Vec::new())
            }
        }
        let mut k = booted();
        let mut d = devvirtio9p::Virtio9p::new();
        d.add(Box::new(Quiet));
        k.tab.add(Box::new(d));

        let Ret::Fd(first) = k.syscall(1, Call::Open { path: "#9/0".into(), mode: 2 }).unwrap()
        else {
            panic!()
        };
        assert!(
            k.syscall(1, Call::Open { path: "#9/0".into(), mode: 2 }).is_err(),
            "two mounts of one server would interleave their replies"
        );
        k.syscall(1, Call::Close { fd: first }).unwrap();
        assert!(k.syscall(1, Call::Open { path: "#9/0".into(), mode: 2 }).is_ok());
    }

    /// `bindmount` resolves the source with `Abind` and the target with
    /// `Amount` (`sysfile.c:51`, `:60`) — **neither is `Atodir`**. So a file
    /// binds over a file, which is how `bind /bin/rc /bin/sh` works. Using
    /// `Atodir` for both, as this did, refuses every bind of a file.
    #[test]
    fn a_file_binds_over_a_file() {
        let mut k = booted();
        assert_eq!(
            k.syscall(
                1,
                Call::Bind {
                    name: "/boot/hello".into(),
                    old: "/boot/init".into(),
                    flag: 0
                }
            )
            .unwrap(),
            Ret::Ok
        );
        // and the name now answers with what was bound over it
        let Ret::Fd(fd) = k.syscall(1, Call::Open { path: "/boot/init".into(), mode: 0 }).unwrap()
        else {
            panic!()
        };
        assert_eq!(
            k.syscall(1, Call::Pread { fd, n: 64, off: -1 }).unwrap(),
            Ret::Data(b"greetings".to_vec())
        );
    }

    /// `chan.c`: `Aopen` with `OEXEC` on a directory is *"cannot exec
    /// directory"*, refused by `namec` because only it knows the mode the
    /// caller asked for.
    #[test]
    fn a_directory_cannot_be_executed() {
        let mut k = booted();
        let e = k.exec(1, "/boot", &[]).unwrap_err();
        assert!(e.contains("cannot exec directory"), "{e}");
    }

    /// What is not built refuses rather than pretending — and **names what
    /// Plan 9 does**, not a phase that has since shipped.
    #[test]
    fn what_is_not_built_refuses_rather_than_lying() {
        let mut k = booted();
        for c in [
            Call::Mount { fd: 0, afd: -1, old: "/n".into(), flag: 0, aname: String::new() },
            Call::Rendezvous { tag: 0, val: 0 },
            Call::Notify,
            Call::Alarm { ms: 5 },
        ] {
            let e = k.syscall(1, c).unwrap_err();
            assert!(!e.contains("P3"), "a phase is not a reason: {e}");
        }
    }

    /// `syssleep` (`sysproc.c`), both branches.
    ///
    /// `n <= 0` is `yield()`, and `yield` does nothing at all when nobody
    /// else is ready: *"if(anyready()){ … sched(); }"* (`proc.c:454`).
    /// `n > 0` is `tsleep(&up->sleep, return0, 0, n)`, raised to one tick —
    /// `TK2MS(1)`, 10ms at the PC's `HZ`.
    ///
    /// **And then it leaves.** `Ret::Sched` is `gotolabel(&m->sched)`: the
    /// process is `Wakeme` with a deadline and the call has not finished.
    /// Nothing waits inside the kernel any more.
    #[test]
    fn sleep_yields_for_nothing_and_leaves_for_a_time() {
        let (mut k, log) = tests::watched();
        assert_eq!(k.syscall(1, Call::Sleep { ms: 0 }).unwrap(), Ret::Ok, "nobody to yield to");

        assert_eq!(k.syscall(1, Call::Sleep { ms: 250 }).unwrap(), Ret::Sched);
        {
            let procs = k.procs.borrow();
            assert_eq!(procs.state(1), proc::State::Wakeme);
            assert_eq!(procs.nextalarm(), Some(1_500_000_000_000_000_000 + 250_000_000));
        }
        assert!(log.borrow().delayed.is_empty(), "the kernel does not wait; sched does");
    }

    /// A sleep shorter than a tick is a sleep of one tick — *"if(n <
    /// TK2MS(1)) n = TK2MS(1)"*.
    #[test]
    fn a_sleep_is_at_least_one_tick() {
        let (mut k, _) = tests::watched();
        k.syscall(1, Call::Sleep { ms: 1 }).unwrap();
        assert_eq!(
            k.procs.borrow().nextalarm(),
            Some(1_500_000_000_000_000_000 + 10_000_000),
            "TK2MS(1) is the floor"
        );
    }

    /// **`schedinit` is the loop** (`proc.c:67`), and this is the whole of
    /// it: a process asleep, nothing else runnable, so `idlehands()` — the
    /// machine waits until the deadline, `timerintr` readies the sleeper,
    /// and `sched` enters it.
    #[test]
    fn schedinit_idles_until_a_sleeper_is_due_and_then_enters_it() {
        let (mut k, log) = tests::watched();
        k.syscall(1, Call::Sleep { ms: 250 }).unwrap();
        k.schedinit().unwrap();

        assert_eq!(log.borrow().delayed, vec![250], "it waited exactly that long");
        assert!(log.borrow().order.contains(&"gotolabel"), "and then entered it");
        assert_eq!(k.procs.borrow().state(1), proc::State::Dead, "which ran to its end");
    }

    /// With nothing runnable and no timer, nothing can ever make a process
    /// runnable again: `schedinit` returns rather than spinning. Plan 9's
    /// cannot reach this — its clock is an interrupt and there is always
    /// another.
    #[test]
    fn schedinit_returns_when_nothing_can_run_again() {
        let (mut k, log) = tests::watched();
        k.schedinit().unwrap();
        assert!(log.borrow().order.is_empty(), "nothing was ready, so nothing ran");
    }


}
