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
    Notify { f: u32 },
    Noted { how: i32 },
    Rendezvous { tag: u64, val: u64 },
    /// `semacquire(long *addr, int block)` — `addr` is an address in the
    /// process's memory, which the kernel reaches through the machine.
    Semacquire { addr: u32, block: bool },
    /// `tsemacquire(long *addr, ulong ms)`.
    Tsemacquire { addr: u32, ms: u64 },
    /// `semrelease(long *addr, long count)`.
    Semrelease { addr: u32, delta: i32 },

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
    /// `up` — the calling process, which the devices that need it read
    /// through. Set before each dispatch.
    pub up: std::rc::Rc<std::cell::RefCell<proc::Up>>,
    /// **`p->sched`** (`portdat.h`) — for each process that left the
    /// processor in the middle of a call, where it goes back in.
    ///
    /// On Plan 9 that is a `Label` into the process's own kernel stack:
    /// `sleep` does `setlabel(&up->sched)`, the frames of the half-finished
    /// call stay where they are, and when the process is entered again
    /// `setlabel` returns 1 and the call carries on from the line after the
    /// `sleep` (`proc.c:830`). Rust cannot leave frames on a stack and come
    /// back to them, so the rest of the call is kept here instead, as the
    /// code that runs it — and it runs when the process is entered again,
    /// having done nothing before the `sleep` twice.
    labels: std::collections::HashMap<Pid, Label>,
    /// What `syscall()`'s `notify` decided at the end of each process's
    /// last call (`pc/trap.c:773`), for the machine to act on.
    notes: std::collections::HashMap<Pid, machine::Notify>,
    /// `clunkq.q` (`chan.c:515`) — one `closeproc` closes at a time.
    clunkq: proc::QLock,
}

/// The rest of a call — what [`Kernel::labels`] holds.
type Label = Box<dyn FnOnce(&mut Kernel, Pid) -> Result<Ret, String>>;

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
        Ok(Kernel {
            procs,
            up,
            tab,
            machine,
            labels: std::collections::HashMap::new(),
            notes: std::collections::HashMap::new(),
            clunkq: proc::QLock::default(),
        })
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
        // *"up->text = elem"* (`sysproc.c:483`) — the last element of the
        // name, as `namec` left it in `up->genbuf`.
        if let Some(p) = self.procs.borrow_mut().get_mut(pid) {
            p.text = path.rsplit('/').next().unwrap_or(path).to_string();
            // *"putseg(up->seg[i])"* and *"up->seg[DSEG] = newseg(SG_DATA,
            // …)"* (`sysproc.c:513`, `:539`): the new image is a new memory,
            // and a process that shared the old one by `RFMEM` no longer
            // shares anything with this one.
            p.seg = std::rc::Rc::new(std::cell::RefCell::new(proc::Segment::default()));
        }
        self.procs.borrow_mut().execnotes(pid);
        // *"if(up->hang) up->procctl = Proc_stopme"* (`sysproc.c:587`).
        if let Some(p) = self.procs.borrow_mut().get_mut(pid) {
            if p.hang {
                p.procctl = Some(proc::Procctl::Stopme);
            }
        }
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
        // `timersinit` (`portclock.c:221`) — Plan 9's `main` calls it before
        // `schedinit`; here the scheduler is where the machine first enters,
        // so it starts the HZ clock the first time it runs.
        if self.procs.borrow().m.hz.is_none() {
            let now = self.machine.todget().nsec;
            self.procs.borrow_mut().timersinit(now);
            // *"kproc("alarm", alarmkproc, 0)"* — `init0`, before the first
            // process reaches user mode (`pc/main.c:264`).
            self.kproc("alarm", alarmkproc);
        }
        let mut left: Option<(Pid, machine::Left)> = None;
        loop {
            // `schedinit`'s switch on the state of the process that left.
            if let Some((pid, how)) = left.take() {
                {
                    // `sched` before the switch (`proc.c:154`, `:159`):
                    // *"up->delaysched = 0;"* and *"m->cs++"*.
                    let mut procs = self.procs.borrow_mut();
                    if let Some(p) = procs.get_mut(pid) {
                        p.delaysched = 0;
                    }
                    procs.m.cs += 1;
                }
                let mut procs = self.procs.borrow_mut();
                match (how, procs.state(pid)) {
                    // *"case Moribund: up->state = Dead"* — and a process
                    // whose image simply ended is one too: it never called
                    // `exits`, so nothing recorded a status.
                    (machine::Left::Exited, _) => {
                        drop(procs);
                        if self.procs.borrow().status(pid).is_none() {
                            self.pexit(pid, "", true);
                        }
                        // A process `pexit` kept `Broken` stays so until it
                        // is killed; nothing of it runs again.
                        if self.procs.borrow().state(pid) != proc::State::Broken {
                            self.procs.borrow_mut().setstate(pid, proc::State::Dead);
                        }
                    }
                    // *"case Running: ready(up)"* — it gave up the processor
                    // without going to sleep, so it goes back on the queue.
                    // A process the clock preempted is one of these.
                    (machine::Left::Sched, proc::State::Running) => procs.ready(pid),
                    (machine::Left::Sched, proc::State::Moribund) => {
                        procs.setstate(pid, proc::State::Dead)
                    }
                    // `Wakeme`, `Ready`, `Stopped` — it said where it went.
                    _ => {}
                }
            }
            // *"if(up) { up->mach = nil; updatecpu(up); up = nil; }"*
            // (`proc.c:105`) — whoever was `up` is not any more, including
            // on the first entry, where it is whoever the boot ran as.
            {
                let mut procs = self.procs.borrow_mut();
                if let Some(up) = procs.up.take() {
                    procs.updatecpu(up, true);
                }
            }

            // `sched()`: `p = runproc()`, and make it `up`.
            let next = self.procs.borrow_mut().sched();
            let Some(pid) = next else {
                // `idlehands()` — halt until the next interrupt. On Plan 9
                // that is the clock, HZ times a second whether or not
                // anything is due; here it is whichever comes first, the HZ
                // clock or a sleeper's timer, and the wait is the machine's
                // `delay`.
                //
                // **With no sleeper at all the system is over** — Plan 9's
                // clock would go on ticking for ever with nothing that could
                // ever make a process runnable, and a hosted machine has
                // somewhere to return to.
                let (alarm, hz) = {
                    let procs = self.procs.borrow();
                    (procs.nextalarm(), procs.m.hz)
                };
                // A reader waiting for the keyboard: a key is coming, and
                // the clock takes it in.
                let keyboard = self
                    .tab
                    .get(dev::DevId::Cons)
                    .and_then(|d| d.as_any().downcast_mut::<devcons::Cons>())
                    .is_some_and(|c| c.waiting());
                let Some(alarm) = alarm.or(if keyboard { hz } else { None }) else {
                    return Ok(());
                };
                let when = hz.map_or(alarm, |h| h.min(alarm));
                let start = self.machine.todget().nsec;
                self.machine.delay(when.saturating_sub(start) / 1_000_000);
                let now = self.machine.todget().nsec.max(when);
                // *"remember how much time we're here"* (`runproc`,
                // `proc.c:558`).
                self.procs.borrow_mut().m.perf.inidle += now - start;
                self.timerintr(now);
                continue;
            };
            self.up.borrow_mut().pid = pid;
            // A kernel process has no image: its body is kernel code, kept
            // as the rest of what it was doing, and entering it is running
            // that (`kprocchild`, `pc/trap.c`).
            let kp = self.procs.borrow().get(pid).is_some_and(|p| p.kp);
            let how = if kp {
                self.runkproc(pid)
            } else {
                let m = self.machine.clone();
                m.gotolabel(pid, self)?
            };
            left = Some((pid, how));
        }
    }

    /// **The clock interrupt**, the kernel's half: what `trap()` does
    /// around it (`pc/trap.c:339`, `m->intr++`; `intrtime`, `:271`) and the
    /// portable `timerintr` it reaches through the machine's `clockintr`
    /// (`kw/clock.c:46`, `i8253clock` on the PC). The counters are `Mach`'s,
    /// and the machine cannot reach `Mach`, so they are counted here.
    pub fn timerintr(&mut self, now: u64) {
        let mut procs = self.procs.borrow_mut();
        procs.m.intr += 1;
        procs.m.perf.intrts = now;
        procs.timerintr(now);
        drop(procs);
        // `addclock0link(kbdputcclock, 22)` (`devcons.c:671`): the console's
        // clock routine, which takes the keyboard in.
        if let Some(c) = self.tab.get(dev::DevId::Cons).and_then(|d| d.as_any().downcast_mut::<devcons::Cons>()) {
            c.kbdputcclock(now);
        }
        let mut procs = self.procs.borrow_mut();
        // `intrtime`: the time spent in the handler, taken out of the idle
        // time if the processor was idle (*"if(up == nil && m->perf.inidle
        // > diff) m->perf.inidle -= diff"*).
        let diff = self.machine.todget().nsec.saturating_sub(now);
        procs.m.perf.intrts = now + diff;
        procs.m.perf.inintr += diff;
        if procs.up.is_none() && procs.m.perf.inidle > diff {
            procs.m.perf.inidle -= diff;
        }
    }

    /// Steps 1 and 2 alone: resolve and read. Split out because it is entirely
    /// Plan 9's, and so it can be tested without a machine.
    pub fn exec_image(&mut self, pid: Pid, path: &str) -> Result<Vec<u8>, String> {
        let (slash, dot, ns) = {
            let procs = self.procs.borrow();
            let p = procs.get(pid).ok_or("no such process")?;
            (p.slash.clone(), p.dot.clone(), p.ns.clone())
        };
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
        // *"'/' processes are higher priority (hack to make /ip more
        // responsive)."* — `if(devtab[tc->type]->dc == L'/') up->basepri =
        // PriRoot; up->priority = up->basepri;` (`sysproc.c:564`), on the
        // line before `cclose(tc)`.
        if let Some(p) = self.procs.borrow_mut().get_mut(pid) {
            if c.dev == dev::DevId::Root {
                p.basepri = proc::pri::ROOT;
            }
            p.priority = p.basepri;
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
        /// The memory `load` and `cmpswap` reach: one, 64K long, shared by
        /// every process a test makes — as `RFMEM` children share it.
        pub(crate) mem: std::collections::HashMap<u32, i32>,
    }

    pub(crate) struct Recorder(Rc<RefCell<Log>>);

    impl machine::Machine for Recorder {
        fn procsetup(&self, _pid: Pid) -> Result<(), String> {
            self.0.borrow_mut().order.push("procsetup");
            Ok(())
        }
        /// A clock that stands still except when the machine is asked to
        /// wait — then it moves by exactly that much, as if it had.
        fn todget(&self) -> machine::Tod {
            let waited: u64 = self.0.borrow().delayed.iter().sum();
            machine::Tod {
                nsec: 1_500_000_000_000_000_000 + waited * 1_000_000,
                ticks: 42,
                hz: 1_000_000,
            }
        }
        /// A test machine does not wait; it records that it was asked to.
        fn delay(&self, ms: u64) {
            self.0.borrow_mut().delayed.push(ms);
        }
        fn load(&self, _pid: Pid, addr: u32) -> Result<i32, String> {
            if addr >= 0x10000 {
                return Err("address out of range".into());
            }
            Ok(self.0.borrow().mem.get(&addr).copied().unwrap_or(0))
        }
        fn cmpswap(&self, pid: Pid, addr: u32, old: i32, new: i32) -> Result<bool, String> {
            if self.load(pid, addr)? != old {
                return Ok(false);
            }
            self.0.borrow_mut().mem.insert(addr, new);
            Ok(true)
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
             WSTAT FWSTAT FVERSION ERRSTR SEMACQUIRE TSEMACQUIRE SEMRELEASE"
            .split_whitespace()
        {
            assert!(plan9.split_whitespace().any(|p| p == c), "{c} is not a Plan 9 syscall");
            for forbidden in ["DRAW", "TIME", "RANDOM", "FETCH", "STORE", "WINDOW", "CONSOLE"] {
                assert_ne!(c, forbidden, "{c} is a file server's job");
            }
        }
    }
}

/// `sysctab[]` (`port/systab.h:114`) — each call's name as `ps` shows it
/// while a process is in it.
fn sysctab(c: &Call) -> &'static str {
    match c {
        Call::Rfork { .. } => "Rfork",
        Call::Exec { .. } => "Exec",
        Call::Exits { .. } => "Exits",
        Call::Await => "Await",
        Call::Sleep { .. } => "Sleep",
        Call::Alarm { .. } => "Alarm",
        Call::Notify { .. } => "Notify",
        Call::Noted { .. } => "Noted",
        Call::Rendezvous { .. } => "Rendez",
        Call::Semacquire { .. } => "Semacquire",
        Call::Tsemacquire { .. } => "Tsemacquire",
        Call::Semrelease { .. } => "Semrelease",
        Call::Bind { .. } => "Bind",
        Call::Mount { .. } => "Mount",
        Call::Unmount { .. } => "Unmount",
        Call::Chdir { .. } => "Chdir",
        Call::Open { .. } => "Open",
        Call::Create { .. } => "Create",
        Call::Close { .. } => "Close",
        Call::Pread { .. } => "Pread",
        Call::Pwrite { .. } => "Pwrite",
        Call::Seek { .. } => "Seek",
        Call::Dup { .. } => "Dup",
        Call::Pipe => "Pipe",
        Call::Remove { .. } => "Remove",
        Call::Stat { .. } => "Stat",
        Call::Fstat { .. } => "Fstat",
        Call::Wstat { .. } => "Wstat",
        Call::Fwstat { .. } => "Fwstat",
        Call::Errstr { .. } => "Errstr",
        Call::Fversion { .. } => "Fversion",
    }
}

/// `alarmkproc` (`alarm.c:11`): post *"alarm"* to every process whose alarm
/// is due, then sleep on `alarmr` until `checkalarms` wakes it.
fn alarmkproc(k: &mut Kernel, me: Pid) -> Result<Ret, String> {
    let due = k.procs.borrow_mut().duealarms();
    for p in due {
        k.procs.borrow_mut().postnote(p, "alarm", proc::NoteFlag::NUser);
    }
    k.procs.borrow_mut().sleep(me, proc::Rid::Alarmr, false);
    k.labels.insert(me, Box::new(alarmkproc));
    Ok(Ret::Ok)
}

/// `closeproc` (`chan.c:552`): close what `ccloseq` queued, one at a time
/// under `clunkq.q`; with nothing left, wait five seconds for more, and
/// then exit — *"no work"*.
fn closeproc(k: &mut Kernel, me: Pid) -> Result<Ret, String> {
    loop {
        if !k.procs.borrow_mut().qlock(&mut k.clunkq, me) {
            k.labels.insert(me, Box::new(closeprocq));
            return Ok(Ret::Ok);
        }
        if !closeproc1(k, me) {
            return Ok(Ret::Ok);
        }
    }
}

/// `closeproc`, entered again holding `clunkq.q` after waiting for it.
fn closeprocq(k: &mut Kernel, me: Pid) -> Result<Ret, String> {
    if closeproc1(k, me) {
        return closeproc(k, me);
    }
    Ok(Ret::Ok)
}

/// `closeproc`, entered again after its `tsleep`, holding `clunkq.q`.
fn closeprocwoken(k: &mut Kernel, me: Pid) -> Result<Ret, String> {
    k.procs.borrow_mut().interrupted(me);
    if k.procs.borrow().clunkq.is_empty() {
        k.procs.borrow_mut().qunlock(&mut k.clunkq);
        k.pexit(me, "no work", true);
        return Ok(Ret::Ok);
    }
    if closeproc1(k, me) {
        return closeproc(k, me);
    }
    Ok(Ret::Ok)
}

/// The body of `closeproc`'s loop, holding `clunkq.q`: `false` if it went
/// to sleep.
fn closeproc1(k: &mut Kernel, me: Pid) -> bool {
    if k.procs.borrow().clunkq.is_empty() {
        let deadline = k.machine.todget().nsec + 5_000_000_000;
        k.procs.borrow_mut().tsleep(me, proc::Rid::Clunkq, false, deadline);
        k.labels.insert(me, Box::new(closeprocwoken));
        return false;
    }
    let c = k.procs.borrow_mut().clunkq.remove(0);
    k.procs.borrow_mut().qunlock(&mut k.clunkq);
    let mut c = c;
    k.tab.dclose(&mut c);
    true
}

impl machine::Syscalls for Kernel {
    fn syscall(&mut self, up: Pid, call: Call) -> Result<Ret, String> {
        Kernel::syscall(self, up, call)
    }

    fn resume(&mut self, up: Pid) -> Result<Ret, String> {
        Kernel::resume(self, up)
    }

    fn postnote(&mut self, up: Pid, msg: &str, flag: proc::NoteFlag) -> bool {
        self.procs.borrow_mut().postnote(up, msg, flag)
    }

    fn notify(&mut self, up: Pid, at: machine::NoteAt) -> machine::Notify {
        match at {
            machine::NoteAt::Syscall => self.notes.remove(&up).unwrap_or(machine::Notify::No),
            _ => self.notify_(up, at),
        }
    }

    fn timerintr(&mut self) -> bool {
        let now = self.machine.todget().nsec;
        Kernel::timerintr(self, now);
        // *"if(up && up->delaysched && clockintr && m->ilockdepth == 0)
        // sched();"* (`pc/trap.c:438`) — `ilockdepth` is always 0 here.
        let procs = self.procs.borrow();
        procs.up.and_then(|up| procs.get(up)).is_some_and(|p| p.delaysched > 0)
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
    /// the machine leaves. When `sched` enters the process again the machine
    /// calls [`machine::Syscalls::resume`], and the call carries on from
    /// where it stopped.
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
        // *"m->syscall++; up->insyscall = 1;"* (`pc/trap.c:673`), and
        // *"up->psstate = sysctab[scallnr]"* (`:727`) — what `ps` shows while
        // the call lasts.
        {
            let mut procs = self.procs.borrow_mut();
            procs.m.syscall += 1;
            if let Some(p) = procs.get_mut(up) {
                p.insyscall = true;
                p.psstate = Some(sysctab(&call).to_string());
            }
        }
        // **`up` is the calling process, and it is set on the way in.** Plan 9
        // does not have to: `syscall()` (`pc/trap.c:665`) runs on the trapping
        // process's own kernel stack, so the per-machine `up` already names
        // it. Here `up` is a shared cell the devices read through — `up->user`
        // in `devsrv`, `up->fgrp` in `devdup`, `up->egrp` in `devenv` — and if
        // it is not set, every one of them answers for whoever ran last.
        self.up.borrow_mut().pid = up;
        // `sysrfork` ends *"ready(p); sched();"*, and that `sched` zeroes
        // `up->delaysched` (`proc.c:154`) before `syscall()` reaches its own
        // *"if(up->delaysched) sched();"* — so the switch after an `rfork`
        // is `sysrfork`'s, never a second one. The machine takes it once
        // the child has run on the parent's frames (RESEARCH §5.2).
        let rforked = matches!(call, Call::Rfork { flags } if flags & proc::rf::PROC != 0);
        let rfork = matches!(call, Call::Rfork { .. });
        let r = self.dispatch(up, call);
        self.syscall_tail(up, r, rforked, rfork)
    }

    /// **The process is entered again in the middle of a call it left** —
    /// `sleep`'s `setlabel` answering 1 (`proc.c:830`). The rest of the call
    /// runs, and ends as any call does.
    pub fn resume(&mut self, up: Pid) -> Result<Ret, String> {
        self.up.borrow_mut().pid = up;
        let label = self.labels.remove(&up).ok_or("the process left no call to go back to")?;
        let r = label(self, up);
        self.syscall_tail(up, r, false, false)
    }

    /// The end of `syscall()` (`pc/trap.c:739`–`:780`), after the call's own
    /// work.
    /// `rforked`: the call was `rfork(RFPROC)`, whose own `sched` is still
    /// to come.
    /// `rfork`: it was `rfork` at all, which `notify` is not called after
    /// (`pc/trap.c:773`, *"scallnr!=RFORK"*).
    fn syscall_tail(
        &mut self,
        up: Pid,
        r: Result<Ret, String>,
        rforked: bool,
        rfork: bool,
    ) -> Result<Ret, String> {
        // `ccloseq` could not make a `closeproc` from inside a device; the
        // call's end can.
        if std::mem::take(&mut self.procs.borrow_mut().closeproc) {
            self.kproc("closeproc", closeproc);
        }
        // **The call left the processor in the middle** — `sleep`, `qlock`
        // or `sched` did `setlabel`, and whatever it was doing kept the rest
        // of the call in `labels`. The machine leaves; `resume` goes back.
        if self.procs.borrow_mut().take_setlabel(up) {
            return Ok(Ret::Sched);
        }
        // **A clock interrupt that fell due during the call.** This kernel
        // runs a call to its end with nothing able to interrupt it — the
        // machine's interrupt is only taken in guest code — which is Plan
        // 9's kernel running `splhi`. An interrupt held off by `splhi` is
        // taken at `spllo`, and the call's end is where that is: still
        // `insyscall`, so the tick is `TSys`'s (`accounttime`, `proc.c:1624`).
        let now = self.machine.todget().nsec;
        if self.procs.borrow().m.hz.is_some_and(|h| h <= now) {
            self.timerintr(now);
        }
        // *"up->insyscall = 0; up->psstate = 0;"* (`pc/trap.c:767`).
        if let Some(p) = self.procs.borrow_mut().get_mut(up) {
            p.insyscall = false;
            p.psstate = None;
        }
        if let Err(e) = &r {
            self.procs.borrow_mut().seterrstr(up, e);
        }
        // *"if(scallnr!=RFORK && (up->procctl || up->nnote)) notify(ureg);"*
        // (`pc/trap.c:773`) — decided now, in Plan 9's order, and acted on
        // by the machine on its way back to the process.
        let n = if rfork { machine::Notify::No } else { self.notify_(up, machine::NoteAt::Syscall) };
        // `procctl` stopped it: the call is over, and its answer waits until
        // `start` — the rest of `notify` is taken again then.
        if n == machine::Notify::Sched {
            self.procs.borrow_mut().take_setlabel(up);
            let answer = r.clone();
            self.labels.insert(up, Box::new(move |_, _| answer));
            return Ok(Ret::Sched);
        }
        let gone = n == machine::Notify::Pexit;
        self.notes.insert(up, n);
        if gone {
            return r;
        }
        // *"if we delayed sched because we held a lock, sched now"* —
        // `if(up->delaysched) sched();` (`pc/trap.c:778`). The call is done;
        // its answer waits until the process is entered again.
        //
        // After an `rfork`, `sysrfork`'s own `sched` is still to come, and
        // it is that one.
        let delayed = !rforked && self.procs.borrow().get(up).is_some_and(|p| p.delaysched > 0);
        if delayed && self.procs.borrow().state(up) == proc::State::Running {
            let answer = r.clone();
            self.labels.insert(up, Box::new(move |_, _| answer));
            return Ok(Ret::Sched);
        }
        r
    }

    /// `sleep` and `sched` from inside a call: keep the rest of it, for
    /// when the process is entered again. The caller has already done
    /// `setlabel` — through `sleep`, `qlock`, or directly.
    fn setlabel(&mut self, up: Pid, rest: Label) {
        self.labels.insert(up, rest);
    }

    /// The record `pwait` takes once `haswaitq` is true.
    fn waitrecord(&mut self, up: Pid) -> Result<Ret, String> {
        match self.procs.borrow_mut().await_child(up) {
            Some(w) => Ok(Ret::Str(w.format())),
            None => Err(ENOCHILD.into()),
        }
    }

    /// `sysread` → `read` (`sysfile.c:672`) on a channel.
    ///
    /// **Taken out and put back, not held.** Plan 9 hands the device the
    /// `Chan*` an fd holds and nothing minds that the device may reach the
    /// same channel again through the fd table — `dupgen` reads `c->mode` of
    /// every open descriptor (`devdup.c:34`), which is exactly what `ls
    /// /dev` does when `#d` is in its union. Holding the borrow across the
    /// call makes that a panic; copying only the offset back loses `dri`
    /// and `uri`, which is how a directory read came to start over every
    /// time. The whole channel goes back.
    ///
    /// **A device may leave in the middle** — `qread` on an empty pipe
    /// sleeps. The rest of the read is then this again, on the same
    /// channel: the device knows where it was.
    fn pread(&mut self, up: Pid, cell: std::rc::Rc<std::cell::RefCell<chan::Chan>>, n: usize, off: i64) -> Result<Ret, String> {
        let mut c = cell.borrow().clone();
        let at = if off < 0 { c.offset } else { off as u64 };
        // `read` (`sysfile.c:672`): **a directory reached through a union is
        // read from every element**, one after another. Without this `ls /`
        // shows whichever element answered the walk and nothing else —
        // which, once `/` is a union of the kernel's root and a file server,
        // is most of the system missing.
        let d = if c.is_dir() && !c.umh.is_empty() {
            self.unionread(&mut c, n)?
        } else {
            self.tab.dread(&mut c, n, at)?
        };
        if self.procs.borrow().get(up).is_some_and(|p| p.setlabel) {
            self.setlabel(up, Box::new(move |k, up| k.pread(up, cell, n, off)));
            return Ok(Ret::Ok);
        }
        if off < 0 {
            c.offset += d.len() as u64;
        }
        *cell.borrow_mut() = c;
        Ok(Ret::Data(d))
    }

    /// `syswrite` → `write` (`sysfile.c`), the same way.
    fn pwrite(&mut self, up: Pid, cell: std::rc::Rc<std::cell::RefCell<chan::Chan>>, data: Vec<u8>, off: i64) -> Result<Ret, String> {
        let mut c = cell.borrow().clone();
        let at = if off < 0 { c.offset } else { off as u64 };
        let n = self.tab.dwrite(&mut c, &data, at)?;
        if self.procs.borrow().get(up).is_some_and(|p| p.setlabel) {
            self.setlabel(up, Box::new(move |k, up| k.pwrite(up, cell, data, off)));
            return Ok(Ret::Ok);
        }
        if off < 0 {
            c.offset += n as u64;
        }
        *cell.borrow_mut() = c;
        Ok(Ret::N(n))
    }

    /// `pexit(exitstr, freemem)` (`proc.c:1123`), where it needs the
    /// kernel: *"closefgrp(fgrp)"* (`:1160`) — `cclose` each channel whose
    /// last reference went — and, when `freemem` is false, which is a note
    /// that was a fault or a suicide, *"addbroken(up)"* (`:1227`): the
    /// process is kept `Broken` for a debugger. `*nobroken` would stop that,
    /// and it is a configuration this system has no source for.
    pub fn pexit(&mut self, pid: Pid, status: &str, freemem: bool) {
        let last = self.procs.borrow_mut().exits(pid, status);
        for mut c in last {
            self.tab.dclose(&mut c);
        }
        if !freemem {
            self.procs.borrow_mut().addbroken(pid);
        }
    }

    /// `pprint` (`devcons.c:322`) — a message from the kernel to a process's
    /// standard error, prefixed with its text and pid.
    pub fn pprint(&mut self, pid: Pid, msg: &str) {
        let Some(cell) = self.chancell(pid, 2).ok() else { return };
        let mut c = cell.borrow().clone();
        if c.mode & 3 != chan::mode::OWRITE && c.mode & 3 != chan::mode::ORDWR {
            return;
        }
        let text = self.procs.borrow().get(pid).map(|p| p.text.clone()).unwrap_or_default();
        let buf = format!("{text} {pid}: {msg}");
        let at = c.offset;
        if let Ok(n) = self.tab.dwrite(&mut c, buf.as_bytes(), at) {
            c.offset += n as u64;
            *cell.borrow_mut() = c;
        }
    }

    /// `kproc` (`proc.c:1436`) — a process whose body is kernel code. The
    /// body is kept where the rest of a call is (`labels`), and it keeps
    /// itself there each time it sleeps.
    pub fn kproc(&mut self, name: &str, body: fn(&mut Kernel, Pid) -> Result<Ret, String>) -> Pid {
        let up = self.up.borrow().pid;
        let eve = self.tab.eve().borrow().clone();
        let pid = self.procs.borrow_mut().kproc(up, name, &eve);
        self.labels.insert(pid, Box::new(body));
        pid
    }

    /// Enter a kernel process: run what it was doing until it sleeps,
    /// yields, or exits.
    fn runkproc(&mut self, pid: Pid) -> machine::Left {
        if let Some(body) = self.labels.remove(&pid) {
            let _ = body(self, pid);
        }
        self.procs.borrow_mut().take_setlabel(pid);
        match self.procs.borrow().state(pid) {
            proc::State::Moribund | proc::State::Dead => machine::Left::Exited,
            _ => machine::Left::Sched,
        }
    }

    /// **`notify(Ureg*)`'s decision** (`pc/trap.c:794`–`:865`): act on
    /// `procctl`, then take the first note — ending the process if nothing
    /// can catch it, leaving it queued if a handler is already running, and
    /// otherwise handing it to the handler.
    ///
    /// What is not here: *"sys:"* notes gain *" pc=0x…"* on Plan 9 (`:809`)
    /// and there is no program counter to report; and at a clock interrupt
    /// a note for a handler stays queued until the process's next call ends,
    /// because a machine whose guest can only be entered from a host call
    /// cannot enter it from an interrupt. A note that ends the process does
    /// not wait.
    fn notify_(&mut self, up: Pid, at: machine::NoteAt) -> machine::Notify {
        use machine::Notify;
        use proc::{NoteFlag, Procctl, State};
        let (procctl, state) = {
            let procs = self.procs.borrow();
            let Some(p) = procs.get(up) else { return Notify::No };
            (p.procctl, p.state)
        };
        if matches!(state, State::Moribund | State::Dead) {
            return Notify::Pexit;
        }
        // `procctl` (`proc.c:1494`): *"case Proc_exitme: pexit("Killed", 1)"*.
        if procctl == Some(Procctl::Exitme) {
            self.procs.borrow_mut().get_mut(up).map(|p| p.procctl = None);
            self.pexit(up, "Killed", true);
            return Notify::Pexit;
        }
        // `procctl`'s `Proc_stopme` (`proc.c:1503`): *"Stopped"*, free a
        // waiting debugger, and `sched()`.
        if procctl == Some(Procctl::Stopme) {
            if at == machine::NoteAt::Fault {
                return Notify::No;
            }
            let mut procs = self.procs.borrow_mut();
            let p = procs.get_mut(up).expect("checked");
            p.procctl = None;
            p.psstate = Some("Stopped".into());
            let pdbg = p.pdbg.take();
            p.state = State::Stopped;
            if let Some(d) = pdbg {
                procs.wakeup(proc::Rid::Proc(d, proc::Which::Sleep));
            }
            procs.setlabel(up);
            return Notify::Sched;
        }
        let (n, notified, handler) = {
            let procs = self.procs.borrow();
            let p = procs.get(up).expect("checked");
            let Some(n) = p.note.first().cloned() else { return Notify::No };
            (n, p.notified, p.notify)
        };
        if n.flag != NoteFlag::NUser && (notified || handler == 0) {
            if n.flag == NoteFlag::NDebug {
                self.pprint(up, &format!("suicide: {}\n", n.msg));
            }
            self.pexit(up, &n.msg, n.flag != NoteFlag::NDebug);
            return Notify::Pexit;
        }
        if notified {
            return Notify::No;
        }
        if handler == 0 {
            self.pexit(up, &n.msg, n.flag != NoteFlag::NDebug);
            return Notify::Pexit;
        }
        match at {
            machine::NoteAt::Clock => return Notify::No,
            // A fault leaves nothing to go back to: the handler could only
            // `noted(NCONT)` into the instruction that faulted.
            machine::NoteAt::Fault => {
                if n.flag == NoteFlag::NDebug {
                    self.pprint(up, &format!("suicide: {}\n", n.msg));
                }
                self.pexit(up, &n.msg, n.flag != NoteFlag::NDebug);
                return Notify::Pexit;
            }
            machine::NoteAt::Syscall => {}
        }
        let mut procs = self.procs.borrow_mut();
        let p = procs.get_mut(up).expect("checked");
        p.notepending = false;
        p.notified = true;
        p.lastnote = p.note.remove(0);
        Notify::Handler { f: handler, msg: n.msg }
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
                self.pexit(up, &status, true);
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
            // `pwait` (`proc.c:1271`): *"sleep(&up->waitr, haswaitq, up)"*,
            // then take the record `pexit` left — which is what the rest of
            // the call does when the process is entered again.
            Call::Await => {
                let ready = self.procs.borrow().haswaitq(up);
                if !ready {
                    if self.procs.borrow().nchild(up) == 0 {
                        return Err(ENOCHILD.into());
                    }
                    let r = proc::Rid::Proc(up, proc::Which::Waitr);
                    if !self.procs.borrow_mut().sleep(up, r, false) && self.procs.borrow_mut().interrupted(up) {
                        return Err(proc::EINTR.into());
                    }
                    self.setlabel(up, Box::new(|k, up| {
                        if k.procs.borrow_mut().interrupted(up) {
                            return Err(proc::EINTR.into());
                        }
                        k.waitrecord(up)
                    }));
                    return Ok(Ret::Ok);
                }
                self.waitrecord(up)
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
                self.pread(up, cell, n, off)
            }
            Call::Pwrite { fd, data, off } => {
                let cell = self.chancell(up, fd)?;
                self.pwrite(up, cell, data, off)
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
                        let mut procs = self.procs.borrow_mut();
                        procs.yield_(up);
                        procs.setlabel(up);
                        drop(procs);
                        self.setlabel(up, Box::new(|_, _| Ok(Ret::Ok)));
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
                let r = proc::Rid::Proc(up, proc::Which::Sleep);
                if !self.procs.borrow_mut().tsleep(up, r, false, deadline) {
                    // A note was already pending: `sleep` never committed,
                    // and leaves with `Eintr`.
                    self.procs.borrow_mut().interrupted(up);
                    return Err(proc::EINTR.into());
                }
                // After the `tsleep`, `syssleep` returns 0 — unless a note
                // woke it (`proc.c:879`).
                self.setlabel(up, Box::new(|k, up| {
                    if k.procs.borrow_mut().interrupted(up) {
                        return Err(proc::EINTR.into());
                    }
                    Ok(Ret::Ok)
                }));
                Ok(Ret::Ok)
            }

            // `sysalarm` is `procalarm` (`sysproc.c:658`, `alarm.c:60`):
            // the note comes from `alarmkproc` when it is due.
            Call::Alarm { ms } => Ok(Ret::N(self.procs.borrow_mut().procalarm(up, ms) as usize)),
            // `sysnotify` (`sysproc.c:782`): *"up->notify = arg[0]"*.
            Call::Notify { f } => {
                if let Some(p) = self.procs.borrow_mut().get_mut(up) {
                    p.notify = f;
                }
                Ok(Ret::Ok)
            }
            // `sysnoted` (`sysproc.c:791`), and then what `syscall()` does
            // for `NOTED` on its way out, `noted(ureg, arg0)`
            // (`pc/trap.c:770`, `:872`) — all of it but restoring the
            // registers, which is the machine's: it answers how, and the
            // machine goes back to where the note interrupted
            // (`NCONT`, `NRSTR`), carries on in the handler (`NSAVE`), or
            // leaves a process that is gone.
            Call::Noted { how } => {
                use proc::noted::*;
                let notified = self.procs.borrow().get(up).is_some_and(|p| p.notified);
                if how != NRSTR && !notified {
                    self.pprint(up, "call to noted() when not notified\n");
                    self.pexit(up, "Suicide", false);
                    return Ok(Ret::N(NDFLT as usize));
                }
                let last = {
                    let mut procs = self.procs.borrow_mut();
                    let p = procs.get_mut(up).expect("checked");
                    p.notified = false;
                    p.lastnote.clone()
                };
                match how {
                    NCONT | NRSTR | NSAVE => Ok(Ret::N(how as usize)),
                    _ => {
                        let mut flag = last.flag;
                        if how != NDFLT {
                            self.pprint(up, &format!("unknown noted arg {how:#x}\n"));
                            flag = proc::NoteFlag::NDebug;
                        }
                        if flag == proc::NoteFlag::NDebug {
                            self.pprint(up, &format!("suicide: {}\n", last.msg));
                        }
                        self.pexit(up, &last.msg, flag != proc::NoteFlag::NDebug);
                        Ok(Ret::N(NDFLT as usize))
                    }
                }
            }
            // `sysrendezvous` (`sysproc.c:910`): find a process in this
            // rendezvous group waiting on the same tag, swap values with it
            // and ready it; or wait, `Rendezvous`, to be found.
            Call::Rendezvous { tag, val } => {
                let mut procs = self.procs.borrow_mut();
                let rgrp = procs.get(up).ok_or("no such process")?.rgrp.clone();
                procs.get_mut(up).expect("checked").rendval = !0;
                let found = rgrp.borrow().iter().position(|q| procs.get(*q).is_some_and(|q| q.rendtag == tag));
                if let Some(i) = found {
                    let q = rgrp.borrow_mut().remove(i);
                    let other = procs.get_mut(q).expect("waiting");
                    let got = other.rendval;
                    other.rendval = val;
                    procs.ready(q);
                    return Ok(Ret::N(got as usize));
                }
                let me = procs.get_mut(up).expect("checked");
                me.rendtag = tag;
                me.rendval = val;
                me.state = proc::State::Rendezvous;
                rgrp.borrow_mut().push(up);
                procs.setlabel(up);
                drop(procs);
                // *"sched(); return up->rendval;"*
                self.setlabel(up, Box::new(|k, up| {
                    Ok(Ret::N(k.procs.borrow().get(up).map_or(!0, |p| p.rendval) as usize))
                }));
                Ok(Ret::Ok)
            }
            // `syssemacquire` (`sysproc.c:1187`).
            Call::Semacquire { addr, block } => {
                self.validlong(up, addr)?;
                if self.machine.load(up, addr)? < 0 {
                    return Err(proc::Procs::EBADARG.into());
                }
                self.semacquire(up, addr, if block { None } else { Some(0) })
            }
            // `systsemacquire` (`sysproc.c:1206`).
            Call::Tsemacquire { addr, ms } => {
                self.validlong(up, addr)?;
                if self.machine.load(up, addr)? < 0 {
                    return Err(proc::Procs::EBADARG.into());
                }
                self.semacquire(up, addr, Some(ms))
            }
            // `syssemrelease` (`sysproc.c:1225`): *"delta == 0 is a no-op,
            // not a release"*.
            Call::Semrelease { addr, delta } => {
                self.validlong(up, addr)?;
                if delta < 0 || self.machine.load(up, addr)? < 0 {
                    return Err(proc::Procs::EBADARG.into());
                }
                self.semrelease(up, addr, delta).map(|v| Ret::N(v as u32 as usize))
            }
        }
    }

    // ---- semaphores (`sysproc.c:954`–`:1240`) ----------------------------

    /// *"validaddr(arg[0], sizeof(long), 1); validalign(arg[0],
    /// sizeof(long));"* — the checks each semaphore call opens with.
    ///
    /// `validaddr` (`fault.c:310`) asks `okaddr`, which says *"suicide:
    /// invalid address"* on the process's console, and then posts *"sys: bad
    /// address in syscall"*; `validalign` (`pc/trap.c:964`) posts *"sys: odd
    /// address"*. Both notes are `NDebug`, so the process dies of them on
    /// its way out of the call, and both calls fail with `Ebadarg`.
    fn validlong(&mut self, up: Pid, addr: u32) -> Result<(), String> {
        if self.machine.load(up, addr).is_err() {
            self.pprint(up, &format!("suicide: invalid address {addr:#x}/4 in sys call\n"));
            self.procs.borrow_mut().postnote(up, "sys: bad address in syscall", proc::NoteFlag::NDebug);
            return Err(proc::Procs::EBADARG.into());
        }
        if addr & 3 != 0 {
            self.procs.borrow_mut().postnote(up, "sys: odd address", proc::NoteFlag::NDebug);
            return Err(proc::Procs::EBADARG.into());
        }
        Ok(())
    }

    /// `canacquire` (`sysproc.c:1085`): *"while((value=*addr) > 0) if(cmpswap(addr,
    /// value, value-1)) return 1;"*.
    fn canacquire(&self, up: Pid, addr: u32) -> Result<bool, String> {
        loop {
            let value = self.machine.load(up, addr)?;
            if value <= 0 {
                return Ok(false);
            }
            if self.machine.cmpswap(up, addr, value, value - 1)? {
                return Ok(true);
            }
        }
    }

    /// `semwakeup` (`sysproc.c:1057`): wake up `n` waiters on `addr`, oldest
    /// first — *"p->waiting = 0; … wakeup(p);"*.
    fn semwakeup(&mut self, seg: &std::rc::Rc<std::cell::RefCell<proc::Segment>>, addr: u32, mut n: i64) {
        let mut woken = Vec::new();
        for p in seg.borrow_mut().sema.iter_mut() {
            if n <= 0 {
                break;
            }
            if p.addr == addr && p.waiting {
                p.waiting = false;
                woken.push(p.p);
                n -= 1;
            }
        }
        let mut procs = self.procs.borrow_mut();
        for p in woken {
            procs.wakeup(proc::Rid::Sema(p));
        }
    }

    /// `semrelease` (`sysproc.c:1075`): add `delta` by compare-and-swap, wake
    /// that many, and answer the new value.
    fn semrelease(&mut self, up: Pid, addr: u32, delta: i32) -> Result<i32, String> {
        let value = loop {
            let value = self.machine.load(up, addr)?;
            if self.machine.cmpswap(up, addr, value, value.wrapping_add(delta))? {
                break value;
            }
        };
        let seg = self.procs.borrow().get(up).ok_or("no such process")?.seg.clone();
        self.semwakeup(&seg, addr, delta as i64);
        Ok(value.wrapping_add(delta))
    }

    /// `semacquire` (`sysproc.c:1109`) and `tsemacquire` (`:1143`), which are
    /// the same function but for the clock: `ms` is `None` to wait for as
    /// long as it takes, and otherwise how long to wait, `Some(0)` being
    /// *"if(!block) return 0"* and *"if(ms == 0) return 0"* both.
    fn semacquire(&mut self, up: Pid, addr: u32, ms: Option<u64>) -> Result<Ret, String> {
        if self.canacquire(up, addr)? {
            return Ok(Ret::N(1));
        }
        if ms == Some(0) {
            return Ok(Ret::N(0));
        }
        // `semqueue` (`sysproc.c:1033`): onto the segment's list, at the
        // tail. `phore` is zeroed first, so it is not yet waiting.
        let seg = self.procs.borrow().get(up).ok_or("no such process")?.seg.clone();
        seg.borrow_mut().sema.push(proc::Sema { addr, waiting: false, p: up });
        self.semwait(up, addr, ms)
    }

    /// The `for(;;)` of `semacquire` and `tsemacquire`, from its top:
    ///
    /// ```c
    /// phore.waiting = 1;
    /// if(canacquire(addr)){ acquired = 1; break; }
    /// if(waserror()) break;
    /// t = m->ticks;                                   /* tsemacquire */
    /// tsleep(&phore, semawoke, &phore, ms);
    /// elms = TK2MS(m->ticks - t);
    /// poperror();
    /// if(elms >= ms){ timedout = 1; break; }
    /// ms -= elms;
    /// ```
    ///
    /// What follows the sleep runs when the process is entered again, as
    /// every call's rest does here ([`Kernel::labels`]).
    fn semwait(&mut self, up: Pid, addr: u32, ms: Option<u64>) -> Result<Ret, String> {
        self.setwaiting(up, true);
        match self.canacquire(up, addr) {
            Ok(true) => return self.semdone(up, addr, Ok(Ret::N(1))),
            Ok(false) => {}
            Err(e) => return self.semdone(up, addr, Err(e)),
        }
        let r = proc::Rid::Sema(up);
        let t = self.procs.borrow().m.ticks;
        // `semawoke`: *"return !((Sema*)p)->waiting"* — false, having just
        // been set.
        let slept = match ms {
            None => self.procs.borrow_mut().sleep(up, r, false),
            Some(ms) => {
                let deadline = self.machine.todget().nsec + ms * 1_000_000;
                self.procs.borrow_mut().tsleep(up, r, false, deadline)
            }
        };
        if !slept {
            // A note was pending, so `sleep` did not commit and raised
            // `Eintr`: *"if(waserror()) break;"*.
            self.procs.borrow_mut().interrupted(up);
            return self.semdone(up, addr, Err(proc::EINTR.into()));
        }
        self.setlabel(up, Box::new(move |k, up| {
            if k.procs.borrow_mut().interrupted(up) {
                return k.semdone(up, addr, Err(proc::EINTR.into()));
            }
            let Some(ms) = ms else { return k.semwait(up, addr, None) };
            let elms = proc::tk2ms(k.procs.borrow().m.ticks.saturating_sub(t));
            if elms >= ms {
                return k.semdone(up, addr, Ok(Ret::N(0)));
            }
            k.semwait(up, addr, Some(ms - elms))
        }));
        Ok(Ret::Ok)
    }

    /// `phore.waiting`, for the process's own `Sema`.
    fn setwaiting(&mut self, up: Pid, waiting: bool) {
        let Some(seg) = self.procs.borrow().get(up).map(|p| p.seg.clone()) else { return };
        let mut s = seg.borrow_mut();
        if let Some(p) = s.sema.iter_mut().find(|p| p.p == up) {
            p.waiting = waiting;
        }
    }

    /// The end of both: *"semdequeue(s, &phore); if(!phore.waiting)
    /// semwakeup(s, addr, 1);"* — a waiter that was woken and then did not
    /// take the semaphore (it was interrupted, timed out, or another took
    /// it first) passes the wakeup on — and then the answer.
    fn semdone(&mut self, up: Pid, addr: u32, r: Result<Ret, String>) -> Result<Ret, String> {
        let Some(seg) = self.procs.borrow().get(up).map(|p| p.seg.clone()) else { return r };
        let phore = {
            let mut s = seg.borrow_mut();
            s.sema.iter().position(|p| p.p == up).map(|i| s.sema.remove(i))
        };
        if phore.is_some_and(|p| !p.waiting) {
            self.semwakeup(&seg, addr, 1);
        }
        r
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

/// `TK2MS(1)` — the shortest sleep there is.
const TK2MS1: u64 = proc::tk2ms(1);


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
        k.tab.add(Box::new(devpipe::PipeDev::new(k.up.clone())));
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

    /// **A read of an empty pipe sleeps**, and the call answers `Sched` so
    /// the machine leaves (`qwait`, `qio.c:866`) — not an empty read, which
    /// is end of file.
    #[test]
    fn a_read_of_an_empty_pipe_leaves_the_processor() {
        let mut k = booted();
        let Ret::Two(_a, b) = k.syscall(1, Call::Pipe).unwrap() else { panic!() };
        assert_eq!(k.syscall(1, Call::Pread { fd: b, n: 16, off: -1 }), Ok(Ret::Sched));
        assert_eq!(k.procs.borrow().state(1), proc::State::Wakeme);
    }

    /// **A call that left carries on where it stopped** when the process is
    /// entered again — `sleep` returning into `qread` (`proc.c:830`) — and
    /// answers what it read. Nothing before the sleep happens twice.
    #[test]
    fn a_read_that_left_carries_on_when_resumed() {
        let mut k = booted();
        let Ret::Two(a, b) = k.syscall(1, Call::Pipe).unwrap() else { panic!() };
        let Ret::Pid(child) = k.syscall(1, Call::Rfork { flags: rf::PROC }).unwrap() else {
            panic!()
        };
        assert_eq!(k.syscall(child, Call::Pread { fd: b, n: 16, off: -1 }), Ok(Ret::Sched));
        k.syscall(1, Call::Pwrite { fd: a, data: b"x".to_vec(), off: -1 }).unwrap();
        assert_eq!(k.procs.borrow().state(child), proc::State::Ready, "the write woke it");
        assert_eq!(k.resume(child), Ok(Ret::Data(b"x".to_vec())));
        assert!(k.resume(child).is_err(), "and there is nothing left to go back to");
    }

    /// **A tick that falls due during a call is taken at its end, still
    /// `insyscall`**, so it is `TSys`'s (`accounttime`, `proc.c:1624`;
    /// `pc/trap.c:674`, `:767`) — the interrupt `splhi` held off, taken at
    /// `spllo`.
    #[test]
    fn a_tick_during_a_call_is_system_time() {
        let mut k = booted();
        {
            let mut p = k.procs.borrow_mut();
            p.timersinit(0);
            p.up = Some(1);
        }
        k.syscall(1, Call::Errstr { buf: String::new() }).unwrap();
        let p = k.procs.borrow();
        assert_eq!(p.m.ticks, 1);
        assert_eq!(p.get(1).unwrap().time[proc::TSYS], 1);
        assert_eq!(p.get(1).unwrap().time[proc::TUSER], 0);
        assert!(!p.get(1).unwrap().insyscall, "and cleared on the way out");
    }

    /// *"if(up->delaysched) sched();"* at the end of every call
    /// (`pc/trap.c:778`): the call's work is done, the process leaves, and
    /// the answer is given when it is entered again.
    #[test]
    fn a_delayed_sched_is_taken_at_the_end_of_a_call() {
        let mut k = booted();
        {
            let mut p = k.procs.borrow_mut();
            let me = p.get_mut(1).unwrap();
            me.delaysched = 1;
            me.state = proc::State::Running;
        }
        let r = k.syscall(1, Call::Open { path: "/boot/hello".into(), mode: 0 });
        assert_eq!(r, Ok(Ret::Sched), "it leaves");
        k.procs.borrow_mut().get_mut(1).unwrap().delaysched = 0;
        assert!(matches!(k.resume(1), Ok(Ret::Fd(_))), "and then answers");
    }

    /// **A note ends a sleep with `Eintr`** (`proc.c:879`), and at the end
    /// of the call it is taken: with no handler, `pexit` with the note as the
    /// status (`pc/trap.c:830`).
    #[test]
    fn a_note_interrupts_a_sleep_and_ends_a_process_without_a_handler() {
        let mut k = booted();
        let Ret::Pid(c) = k.syscall(1, Call::Rfork { flags: rf::PROC }).unwrap() else { panic!() };
        assert_eq!(k.syscall(c, Call::Sleep { ms: 10_000 }), Ok(Ret::Sched));
        assert!(k.procs.borrow_mut().postnote(c, "interrupt", proc::NoteFlag::NUser));
        assert_eq!(k.procs.borrow().state(c), proc::State::Ready, "postnote woke it");
        assert_eq!(k.resume(c), Err(proc::EINTR.into()), "sleep leaves with Eintr");
        use machine::Syscalls;
        assert_eq!(k.notify(c, machine::NoteAt::Syscall), machine::Notify::Pexit);
        assert_eq!(k.procs.borrow().status(c).as_deref(), Some("*init* 2: interrupt"));
    }

    /// With a handler the note is handed over (`pc/trap.c:857`), and a
    /// second one waits while the first is being handled (`:824`);
    /// `noted(NCONT)` ends the handling and the next is taken at the end of
    /// that call.
    #[test]
    fn a_handler_takes_notes_one_at_a_time() {
        use machine::{NoteAt, Notify, Syscalls};
        let mut k = booted();
        k.syscall(1, Call::Notify { f: 42 }).unwrap();
        k.procs.borrow_mut().postnote(1, "one", proc::NoteFlag::NUser);
        k.procs.borrow_mut().postnote(1, "two", proc::NoteFlag::NUser);
        k.syscall(1, Call::Errstr { buf: String::new() }).unwrap();
        assert_eq!(k.notify(1, NoteAt::Syscall), Notify::Handler { f: 42, msg: "one".into() });
        k.syscall(1, Call::Errstr { buf: String::new() }).unwrap();
        assert_eq!(k.notify(1, NoteAt::Syscall), Notify::No, "notified: the second waits");
        assert_eq!(k.syscall(1, Call::Noted { how: proc::noted::NCONT }), Ok(Ret::N(0)));
        assert_eq!(k.notify(1, NoteAt::Syscall), Notify::Handler { f: 42, msg: "two".into() });
    }

    /// `noted(NDFLT)` ends the process with the note it was handling
    /// (`pc/trap.c:953`); `noted` when nothing is being handled is
    /// *"Suicide"* (`:878`).
    #[test]
    fn noted_ndflt_exits_and_noted_unnotified_is_suicide() {
        use machine::{NoteAt, Notify, Syscalls};
        let mut k = booted();
        let Ret::Pid(c) = k.syscall(1, Call::Rfork { flags: rf::PROC }).unwrap() else { panic!() };
        k.syscall(c, Call::Notify { f: 7 }).unwrap();
        k.procs.borrow_mut().postnote(c, "hangup", proc::NoteFlag::NUser);
        k.syscall(c, Call::Errstr { buf: String::new() }).unwrap();
        assert!(matches!(k.notify(c, NoteAt::Syscall), Notify::Handler { .. }));
        k.syscall(c, Call::Noted { how: proc::noted::NDFLT }).unwrap();
        assert_eq!(k.notify(c, NoteAt::Syscall), Notify::Pexit);
        assert_eq!(k.procs.borrow().status(c).as_deref(), Some("*init* 2: hangup"));

        let Ret::Pid(d) = k.syscall(1, Call::Rfork { flags: rf::PROC }).unwrap() else { panic!() };
        k.syscall(d, Call::Noted { how: proc::noted::NCONT }).unwrap();
        assert_eq!(k.procs.borrow().status(d).as_deref(), Some(&*format!("*init* {d}: Suicide")));
    }

    /// `alarm` (`alarm.c:60`): the `alarm` kproc posts *"alarm"* when it is
    /// due, and a second `alarm` answers what was left of the first.
    #[test]
    fn an_alarm_is_a_note_from_the_alarm_kproc() {
        let (mut k, _) = tests::watched();
        let alarm = k.kproc("alarm", alarmkproc);
        assert_eq!(k.syscall(1, Call::Alarm { ms: 30 }), Ok(Ret::N(0)));
        assert_eq!(k.syscall(1, Call::Alarm { ms: 30 }), Ok(Ret::N(30)), "the old one's time left");
        // Run the kproc once so it sleeps on alarmr, then start the clock
        // and tick it.
        k.runkproc(alarm);
        k.procs.borrow_mut().timersinit(0);
        for t in 1..=3 {
            k.timerintr(t * 10_000_000);
        }
        assert_eq!(k.procs.borrow().state(alarm), proc::State::Ready, "checkalarms woke it");
        k.runkproc(alarm);
        let p = k.procs.borrow();
        assert_eq!(p.get(1).unwrap().note.first().map(|n| n.msg.as_str()), Some("alarm"));
    }

    /// `rendezvous` (`sysproc.c:910`): the first to arrive waits; the second
    /// with the same tag finds it, and each gets the other's value. A
    /// different rendezvous group (`RFREND`) is not met.
    #[test]
    fn rendezvous_swaps_values_between_two_processes() {
        let mut k = booted();
        let Ret::Pid(c) = k.syscall(1, Call::Rfork { flags: rf::PROC }).unwrap() else { panic!() };
        let Ret::Pid(other) = k.syscall(1, Call::Rfork { flags: rf::PROC | rf::REND }).unwrap() else {
            panic!()
        };
        assert_eq!(k.syscall(c, Call::Rendezvous { tag: 7, val: 100 }), Ok(Ret::Sched));
        assert_eq!(k.procs.borrow().state(c), proc::State::Rendezvous);
        assert_eq!(
            k.syscall(other, Call::Rendezvous { tag: 7, val: 5 }),
            Ok(Ret::Sched),
            "another group: it waits too"
        );
        assert_eq!(k.syscall(1, Call::Rendezvous { tag: 7, val: 200 }), Ok(Ret::N(100)));
        assert_eq!(k.resume(c), Ok(Ret::N(200)));
    }

    /// `semrelease` adds and answers the new value; `semacquire` takes one
    /// if there is one, and without `block` answers 0 at once if not
    /// (`sysproc.c:1109`, `:1075`).
    #[test]
    fn a_semaphore_counts() {
        let (mut k, log) = tests::watched();
        k.exec(1, "/boot/init", &[]).unwrap();
        assert_eq!(k.syscall(1, Call::Semrelease { addr: 64, delta: 2 }), Ok(Ret::N(2)));
        assert_eq!(k.syscall(1, Call::Semacquire { addr: 64, block: true }), Ok(Ret::N(1)));
        assert_eq!(k.syscall(1, Call::Semacquire { addr: 64, block: false }), Ok(Ret::N(1)));
        assert_eq!(k.syscall(1, Call::Semacquire { addr: 64, block: false }), Ok(Ret::N(0)));
        assert_eq!(k.syscall(1, Call::Tsemacquire { addr: 64, ms: 0 }), Ok(Ret::N(0)));
        assert_eq!(log.borrow().mem.get(&64), Some(&0));
        assert_eq!(k.syscall(1, Call::Semrelease { addr: 64, delta: 0 }), Ok(Ret::N(0)), "a no-op, not a release");
    }

    /// A process with nothing to take sleeps on its own `Sema`, and a
    /// release by a process sharing the memory wakes it, oldest first, one
    /// per unit released. A process with a memory of its own is not woken:
    /// the list is the segment's (`sysproc.c:1057`).
    #[test]
    fn semrelease_wakes_the_oldest_waiter_in_the_segment() {
        let (mut k, _log) = tests::watched();
        k.exec(1, "/boot/init", &[]).unwrap();
        let Ret::Pid(a) = k.syscall(1, Call::Rfork { flags: rf::PROC | rf::MEM }).unwrap() else { panic!() };
        let Ret::Pid(b) = k.syscall(1, Call::Rfork { flags: rf::PROC | rf::MEM }).unwrap() else { panic!() };
        let Ret::Pid(other) = k.syscall(1, Call::Rfork { flags: rf::PROC }).unwrap() else { panic!() };
        assert_eq!(k.syscall(other, Call::Semacquire { addr: 8, block: true }), Ok(Ret::Sched));
        assert_eq!(k.syscall(a, Call::Semacquire { addr: 8, block: true }), Ok(Ret::Sched));
        assert_eq!(k.syscall(b, Call::Semacquire { addr: 8, block: true }), Ok(Ret::Sched));
        assert_eq!(k.procs.borrow().state(a), proc::State::Wakeme);
        assert_eq!(k.syscall(1, Call::Semrelease { addr: 8, delta: 1 }), Ok(Ret::N(1)));
        assert_eq!(k.procs.borrow().state(a), proc::State::Ready, "the oldest");
        assert_eq!(k.procs.borrow().state(b), proc::State::Wakeme);
        assert_eq!(k.procs.borrow().state(other), proc::State::Wakeme, "another segment");
        assert_eq!(k.resume(a), Ok(Ret::N(1)));
        assert!(k.procs.borrow().get(1).unwrap().seg.borrow().sema.iter().all(|p| p.p != a), "dequeued");
    }

    /// A waiter woken that then finds nothing — another took it first —
    /// sleeps again, and one that leaves without taking it passes the
    /// wakeup on (*"if(!phore.waiting) semwakeup(s, addr, 1)"*).
    #[test]
    fn a_woken_waiter_that_does_not_take_it_passes_the_wakeup_on() {
        let (mut k, _log) = tests::watched();
        k.exec(1, "/boot/init", &[]).unwrap();
        let Ret::Pid(a) = k.syscall(1, Call::Rfork { flags: rf::PROC | rf::MEM }).unwrap() else { panic!() };
        let Ret::Pid(b) = k.syscall(1, Call::Rfork { flags: rf::PROC | rf::MEM }).unwrap() else { panic!() };
        k.syscall(a, Call::Semacquire { addr: 8, block: true }).unwrap();
        k.syscall(b, Call::Semacquire { addr: 8, block: true }).unwrap();
        k.syscall(1, Call::Semrelease { addr: 8, delta: 1 }).unwrap();
        // Before `a` runs, a note: it leaves with `Eintr`, having been woken,
        // so `b` is woken in its place.
        k.procs.borrow_mut().postnote(a, "interrupt", proc::NoteFlag::NUser);
        assert_eq!(k.resume(a), Err(proc::EINTR.into()));
        assert_eq!(k.procs.borrow().state(b), proc::State::Ready);
        assert_eq!(k.resume(b), Ok(Ret::N(1)));
    }

    /// `tsemacquire` answers 0 when its time is up (`sysproc.c:1143`).
    #[test]
    fn tsemacquire_times_out() {
        let (mut k, _log) = tests::watched();
        k.exec(1, "/boot/init", &[]).unwrap();
        let now = k.machine.todget().nsec;
        k.procs.borrow_mut().timersinit(now);
        assert_eq!(k.syscall(1, Call::Tsemacquire { addr: 8, ms: 30 }), Ok(Ret::Sched));
        for t in 1..=4 {
            k.timerintr(now + t * 10_000_000);
        }
        assert_eq!(k.procs.borrow().state(1), proc::State::Ready, "the timer woke it");
        assert_eq!(k.resume(1), Ok(Ret::N(0)));
        assert!(k.procs.borrow().get(1).unwrap().seg.borrow().sema.is_empty());
    }

    /// An address outside the process's memory is `validaddr`'s: *"sys: bad
    /// address in syscall"* and `Ebadarg`; one not on a `long` boundary is
    /// `validalign`'s, *"sys: odd address"*. A negative semaphore is
    /// `Ebadarg` alone, as is a negative release.
    #[test]
    fn semaphore_calls_check_their_address() {
        let bad = Err(proc::Procs::EBADARG.to_string());
        let notes = |k: &Kernel| -> Vec<String> {
            k.procs.borrow().get(1).unwrap().note.iter().map(|n| n.msg.clone()).collect()
        };
        for (call, note) in [
            (Call::Semacquire { addr: 0x20000, block: true }, Some("sys: bad address in syscall")),
            (Call::Semrelease { addr: 6, delta: 1 }, Some("sys: odd address")),
            (Call::Semacquire { addr: 16, block: false }, None),
            (Call::Semrelease { addr: 20, delta: -1 }, None),
        ] {
            let (mut k, log) = tests::watched();
            k.exec(1, "/boot/init", &[]).unwrap();
            log.borrow_mut().mem.insert(16, -1);
            assert_eq!(k.syscall(1, call.clone()), bad, "{call:?}");
            assert_eq!(notes(&k), note.into_iter().map(String::from).collect::<Vec<_>>(), "{call:?}");
        }
    }

    /// A note pulls a process out of a rendezvous with `~0` (`proc.c:1039`).
    #[test]
    fn a_note_pulls_a_process_out_of_a_rendezvous() {
        let mut k = booted();
        let Ret::Pid(c) = k.syscall(1, Call::Rfork { flags: rf::PROC }).unwrap() else { panic!() };
        k.syscall(c, Call::Rendezvous { tag: 1, val: 1 }).unwrap();
        k.procs.borrow_mut().postnote(c, "interrupt", proc::NoteFlag::NUser);
        assert_eq!(k.procs.borrow().state(c), proc::State::Ready);
        assert_eq!(k.resume(c), Ok(Ret::N(!0usize)));
    }

    /// A process `pexit` ends with a fault or a suicide stays `Broken`
    /// (`proc.c:1227`) — at most `NBROKEN` of them — and `kill` lets it go.
    #[test]
    fn a_suicide_is_kept_broken_until_it_is_killed() {
        let mut k = booted();
        let Ret::Pid(c) = k.syscall(1, Call::Rfork { flags: rf::PROC }).unwrap() else { panic!() };
        k.syscall(c, Call::Noted { how: proc::noted::NCONT }).unwrap();
        assert_eq!(k.procs.borrow().state(c), proc::State::Broken);
        k.procs.borrow_mut().unbreak(c);
        assert_eq!(k.procs.borrow().state(c), proc::State::Dead);
    }

    /// `ps` shows the call a process is in (`pc/trap.c:727`, `devproc.c:865`).
    #[test]
    fn a_process_in_a_call_shows_the_call() {
        let mut k = booted();
        let Ret::Pid(c) = k.syscall(1, Call::Rfork { flags: rf::PROC }).unwrap() else { panic!() };
        k.syscall(c, Call::Sleep { ms: 1000 }).unwrap();
        assert_eq!(k.procs.borrow().get(c).unwrap().psstate.as_deref(), Some("Sleep"));
        k.syscall(1, Call::Errstr { buf: String::new() }).unwrap();
        assert_eq!(k.procs.borrow().get(1).unwrap().psstate, None, "cleared on the way out");
    }

    /// `RFNOTEG` gives a new note group (`sysproc.c:87`, `:188`); without
    /// it a child is in its parent's.
    #[test]
    fn rfnoteg_makes_a_new_note_group() {
        let mut k = booted();
        let Ret::Pid(c) = k.syscall(1, Call::Rfork { flags: rf::PROC }).unwrap() else { panic!() };
        let Ret::Pid(d) = k.syscall(1, Call::Rfork { flags: rf::PROC | rf::NOTEG }).unwrap() else {
            panic!()
        };
        let p = k.procs.borrow();
        let id = |x| p.get(x).unwrap().noteid;
        assert_eq!(id(c), id(1));
        assert_ne!(id(d), id(1));
    }

    /// **`pexit` closes the descriptors** (`closefgrp`, `proc.c:1160`), so
    /// when the last writer exits the reader gets what was written and then
    /// end of file — which is how `tr` learns that `echo` is done.
    #[test]
    fn a_writer_that_exits_closes_its_end_of_the_pipe() {
        let mut k = booted();
        let Ret::Two(a, b) = k.syscall(1, Call::Pipe).unwrap() else { panic!() };
        let Ret::Pid(child) = k.syscall(1, Call::Rfork { flags: rf::PROC | rf::FDG }).unwrap()
        else {
            panic!()
        };
        k.syscall(1, Call::Close { fd: a }).unwrap();
        k.syscall(child, Call::Pwrite { fd: a, data: b"bye".to_vec(), off: -1 }).unwrap();
        k.syscall(child, Call::Exits { status: String::new() }).unwrap();
        let read = |k: &mut Kernel| k.syscall(1, Call::Pread { fd: b, n: 16, off: -1 });
        assert_eq!(read(&mut k), Ok(Ret::Data(b"bye".to_vec())));
        assert_eq!(read(&mut k), Ok(Ret::Data(Vec::new())), "then end of file");
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
        assert_eq!(k.procs.borrow().m.syscall, 0);
        let _ = k.syscall(1, Call::Errstr { buf: String::new() });
        let _ = k.syscall(1, Call::Open { path: "/nothing".into(), mode: 0 });
        assert_eq!(k.procs.borrow().m.syscall, 2, "a failed call is still a call");
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
    /// machine waits for the next interrupt, which is the HZ clock every
    /// tick until the sleeper's own timer comes due; `timerintr` readies it
    /// and `sched` enters it.
    #[test]
    fn schedinit_idles_a_tick_at_a_time_until_a_sleeper_is_due() {
        let (mut k, log) = tests::watched();
        k.syscall(1, Call::Sleep { ms: 250 }).unwrap();
        k.schedinit().unwrap();

        assert_eq!(log.borrow().delayed, vec![10; 25], "twenty-five ticks of 10ms");
        assert!(log.borrow().order.contains(&"gotolabel"), "and then entered it");
        assert_eq!(k.procs.borrow().state(1), proc::State::Dead, "which ran to its end");
        let procs = k.procs.borrow();
        assert_eq!(procs.m.ticks, 25, "the clock ticked while nothing ran");
        assert_eq!(procs.m.intr, 25, "each tick an interrupt");
        assert_eq!(procs.get(1).unwrap().time[proc::TUSER], 0, "and charged nobody");
        assert_eq!(procs.m.cs, 2, "the alarm kproc going to sleep, and out of the image");
    }

    /// `sysexec` gives a process whose image came from `#/` `PriRoot`
    /// (`sysproc.c:564`).
    #[test]
    fn an_image_from_the_root_device_runs_at_priroot() {
        let mut k = booted();
        k.exec_image(1, "/boot/init").unwrap();
        let procs = k.procs.borrow();
        let p = procs.get(1).unwrap();
        assert_eq!((p.basepri, p.priority), (proc::pri::ROOT, proc::pri::ROOT));
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
