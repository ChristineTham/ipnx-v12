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
pub mod qio;
pub mod sha1;
#[cfg(test)]
pub(crate) mod testfs;

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
    /// `_stat` and `_fstat` — the calls before 9P2000's, which Plan 9's
    /// kernel keeps (`systab.h:71`, `:78`) and its emulators still make
    /// (`5i/syscall.c:370`): the stat in the old fixed 116 bytes.
    OldStat { path: String },
    OldFstat { fd: Fd },
    Wstat { path: String, edir: Vec<u8> },
    Fwstat { fd: Fd, edir: Vec<u8> },
    Fversion { fd: Fd, msize: u32, version: String },
    /// `fauth(2)` — a channel for authenticating to the server on `fd`
    /// (`sysfauth`, `auth.c:62`).
    Fauth { fd: Fd, aname: String },
    /// **A call this kernel's table does not have** — `segattach` and the
    /// other memory calls (`docs/syscalls.md`). A process that makes one is
    /// answered as Plan 9's kernel answers a number with no `systab` entry
    /// (`pc/trap.c:716`).
    Bad { n: u32 },
    /// `sysr1` (`sysproc.c:25`) — *"checkpagerefs(); return 0;"*.
    Sysr1,
    /// `fd2path(2)` — the name the channel was reached by (`sysfd2path`,
    /// `sysfile.c:173`).
    Fd2path { fd: Fd },
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
    /// `imagealloc`'s cache (`segment.c:245`) — each image some process is
    /// running, and its text segment. A segment nobody holds any more is
    /// gone, as `putseg` frees it; the entry goes with it.
    images: Vec<(ImageKey, std::rc::Weak<std::cell::RefCell<proc::Segment>>)>,
}

/// What `attachimage` compares to find an image already running
/// (`segment.c:262`): *"eqqid(c->qid, i->qid) && eqqid(c->mqid, i->mqid) &&
/// c->mchan == i->mchan && c->type == i->type"*. Plan 9 compares `mchan` by
/// pointer; a channel here is a value, so it is compared by what names it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ImageKey {
    dev: dev::DevId,
    devno: u32,
    qid: (u64, u32),
    mqid: (u64, u32),
    mchan: Option<(dev::DevId, u32, u64)>,
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
        tab.up = Some(up.clone());
        Ok(Kernel {
            procs,
            up,
            tab,
            machine,
            labels: std::collections::HashMap::new(),
            notes: std::collections::HashMap::new(),
            clunkq: proc::QLock::default(),
            images: Vec::new(),
        })
    }

    /// `exec`, in the order `sysexec` does it (`sysproc.c:259`).
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
        let c = self.exec_open(pid, path)?;
        // *"if(!indir) kstrdup(&elem, up->genbuf)"* (`sysproc.c:308`) — the
        // name's last element, which `namec` leaves in `up->genbuf`
        // (`chan.c:1652`), or `.` if it has none.
        let elem = path.split('/').filter(|e| !e.is_empty()).last().unwrap_or(".").to_string();
        self.exec_read(pid, c, Vec::new(), path.to_string(), args.to_vec(), false, elem)
    }

    /// Step 2, **as far as there is to read, however long that takes**: a
    /// file served by a pipe or by a process answers when it answers, and
    /// `sysexec`'s reads sleep there as any read does. The rest of the
    /// `exec` is kept for when the process is entered again, as `pread`
    /// keeps its own.
    ///
    /// `indir` and `elem` are `sysexec`'s: whether this is a `#!` script's
    /// interpreter, and the last element of the name first given, which
    /// stays the process's `text` either way.
    #[allow(clippy::too_many_arguments)]
    fn exec_read(
        &mut self,
        pid: Pid,
        c: std::rc::Rc<std::cell::RefCell<Chan>>,
        mut image: Vec<u8>,
        path: String,
        args: Vec<String>,
        indir: bool,
        elem: String,
    ) -> Result<(), String> {
        // *"devtab[tc->type]->read(tc, …, offset)"* — at an offset of its
        // own, so the channel's is not moved: it may be a descriptor's
        // (`exec /fd/3`).
        let mut cc = c.borrow().clone();
        loop {
            let got = match self.tab.dread(&mut cc, 8192, image.len() as u64) {
                Ok(got) => got,
                Err(e) => {
                    self.tab.cclose(c);
                    return Err(e);
                }
            };
            if self.procs.borrow().get(pid).is_some_and(|p| p.setlabel) {
                self.setlabel(
                    pid,
                    Box::new(move |k, up| k.exec_read(up, c, image, path, args, indir, elem).map(|()| Ret::Ok)),
                );
                return Ok(());
            }
            if got.is_empty() {
                break;
            }
            image.extend_from_slice(&got);
        }
        // *"n = devtab[tc->type]->read(tc, &exec, sizeof(exec), 0); if(n <
        // 2) error(Ebadexec);"* (`sysproc.c:310`–`:312`).
        if image.len() < 2 {
            self.tab.cclose(c);
            return Err(EBADEXEC.into());
        }
        // **Not a binary: perhaps `#!`** (`sysproc.c:340`). Plan 9 tests for
        // its own binary's magic first; this kernel cannot — only the
        // machine knows what it runs — so the test is the other way round,
        // and exact: an image that begins `#!` is a script, and anything else
        // goes to the machine, which refuses what it cannot run with the
        // same `Ebadexec`. No machine's binary begins `#!`: a wasm module
        // begins `\0asm`, and Plan 9's own `Exec` begins with its magic.
        if image.starts_with(b"#!") {
            self.tab.cclose(c);
            // *"if(indir || line[0]!='#' || line[1]!='!') error(Ebadexec)"*
            // — one level: an interpreter that is a script is refused.
            if indir {
                return Err(EBADEXEC.into());
            }
            // *"n = shargs(line, n, progarg); if(n == 0) error(Ebadexec);"*
            let mut progarg = shargs(&image).ok_or(EBADEXEC)?;
            // *"First arg becomes complete file name"*: the script, as it
            // was named, after the interpreter's own arguments; and the
            // caller's `argv[0]` is dropped — *"arg[1] += oBY2WD"* — for
            // the rest of its arguments to follow.
            progarg.push(path.clone());
            // *"file = progarg[0]; if(strlen(elem) >= sizeof progelem)
            // error(Ebadexec); strcpy(progelem, elem); progarg[0] =
            // progelem;"* — the interpreter is found by the name the line
            // gives, and is told the script's own last element as its
            // `argv[0]`.
            let file = std::mem::replace(&mut progarg[0], elem.clone());
            if elem.len() >= PROGELEM {
                return Err(EBADEXEC.into());
            }
            progarg.extend(args.into_iter().skip(1));
            let c = self.exec_open(pid, &file)?;
            return self.exec_read(pid, c, Vec::new(), file, progarg, true, elem);
        }
        let r = {
            let cc = c.borrow().clone();
            self.exec_commit(pid, &cc, &image, &elem, &args)
        };
        if let Err(e) = r {
            self.tab.cclose(c);
            return Err(e);
        }
        // *"poperror(); cclose(tc);"* (`sysproc.c:571`) — past the point of
        // no return, and the close may wait; the rest of `sysexec` is after
        // it.
        let tc = std::rc::Rc::try_unwrap(c).ok().map(|cell| cell.into_inner());
        self.closethen(pid, tc.into_iter().collect(), Box::new(|k, pid| k.exec_closeonexec(pid))).map(|_| ())
    }

    /// Step 3, and what `sysexec` commits. **The machine is asked first**:
    /// Plan 9 reads the header and fails the call with `Ebadexec` before it
    /// touches the process — an image that is neither a binary it knows nor
    /// a `#!` line is *"if(indir || line[0]!='#' || line[1]!='!')
    /// error(Ebadexec)"* (`sysproc.c:343`). Only a machine can tell whether
    /// an image is one it can run, and `touser` is where it says so. Nothing
    /// of the process has changed if it refuses.
    fn exec_commit(&mut self, pid: Pid, c: &Chan, image: &[u8], elem: &str, args: &[String]) -> Result<(), String> {
        self.machine.procsetup(pid)?;
        self.machine.touser(pid, image, args)?;
        // *"img = attachimage(SG_TEXT|SG_RONLY, tc, UTZERO, …)"*
        // (`sysproc.c:530`) — the text segment, shared with whatever else
        // is running this file.
        let tseg = self.attachimage(c, image.len() as u64);
        if let Some(p) = self.procs.borrow_mut().get_mut(pid) {
            // *"up->text = elem"* (`sysproc.c:483`) — the last element of
            // the name first given: a script's, not its interpreter's.
            p.text = elem.to_string();
            p.tseg = Some(tseg);
            // *"putseg(up->seg[i])"* and *"up->seg[DSEG] = newseg(SG_DATA,
            // …)"* (`sysproc.c:513`, `:539`): the new image is a new memory,
            // and a process that shared the old one by `RFMEM` no longer
            // shares anything with this one.
            p.seg = std::rc::Rc::new(std::cell::RefCell::new(proc::Segment::default()));
            // *"'/' processes are higher priority (hack to make /ip more
            // responsive)."* — `if(devtab[tc->type]->dc == L'/') up->basepri
            // = PriRoot; up->priority = up->basepri;` (`sysproc.c:564`), on
            // the line before `cclose(tc)`.
            if c.dev == dev::DevId::Root {
                p.basepri = proc::pri::ROOT;
            }
            p.priority = p.basepri;
        }
        Ok(())
    }

    /// The rest of `sysexec`, after the image's channel is closed
    /// (`sysproc.c:576`–`:594`): the notes go, *"if(up->hang) up->procctl =
    /// Proc_stopme"*, and **close on exec** — *"for(i=0; i<=f->maxfd; i++)
    /// fdclose(i, CCEXEC)"*, every descriptor opened `OCEXEC` or marked so,
    /// as `fauth`'s is. They had been passed on to the new image. Each close
    /// may wait.
    fn exec_closeonexec(&mut self, pid: Pid) -> Result<Ret, String> {
        self.procs.borrow_mut().execnotes(pid);
        if let Some(p) = self.procs.borrow_mut().get_mut(pid) {
            if p.hang {
                p.procctl = Some(proc::Procctl::Stopme);
            }
        }
        let mut last = Vec::new();
        let fds = self.procs.borrow().get(pid).and_then(|p| p.fds.clone());
        if let Some(fds) = fds {
            let n = fds.borrow().slots();
            for fd in 0..n {
                let marked = fds.borrow().get(fd).is_some_and(|c| c.borrow().flag & chan::flag::CCEXEC != 0);
                if marked {
                    last.extend(fds.borrow_mut().close(fd).flatten());
                }
            }
        }
        self.closethen(pid, last, Box::new(|_, _| Ok(Ret::Ok)))
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
                        // In `pexit` is a process whose namespace it has
                        // taken (*"up->pgrp = nil"*, `proc.c:1154`) and whose
                        // wait record is not yet made.
                        let begun = |k: &Kernel| k.procs.borrow().get(pid).is_some_and(|p| p.ns.is_none());
                        let ended = |k: &Kernel| k.procs.borrow().status(pid).is_some();
                        if !begun(self) {
                            let _ = self.pexit(pid, "", true);
                        }
                        if begun(self) && !ended(self) {
                            // Its `pexit` is waiting on a close, and the
                            // image is gone: the rest runs as kernel code.
                            let mut procs = self.procs.borrow_mut();
                            procs.take_setlabel(pid);
                            if let Some(p) = procs.get_mut(pid) {
                                p.noimage = true;
                            }
                        } else if self.procs.borrow().state(pid) != proc::State::Broken {
                            // A process `pexit` kept `Broken` stays so until
                            // it is killed; nothing of it runs again.
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
            // on the first entry, where it is pid 1, whoever the host started.
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
                self.timerintr(now, None);
                continue;
            };
            self.up.borrow_mut().pid = pid;
            // A kernel process has no image: its body is kernel code, kept
            // as the rest of what it was doing, and entering it is running
            // that (`kprocchild`, `pc/trap.c`).
            let kp = self.procs.borrow().get(pid).is_some_and(|p| p.kp || p.noimage);
            let how = if kp {
                self.runkproc(pid)
            } else {
                let m = self.machine.clone();
                // A machine that cannot enter a process ends the system, so
                // say which process it was and what the kernel held of it.
                m.gotolabel(pid, self).map_err(|e| {
                    let procs = self.procs.borrow();
                    let p = procs.get(pid);
                    format!(
                        "{e}: pid {pid} ({}), {:?}, status {:?}",
                        p.map_or("?", |p| p.text.as_str()),
                        procs.state(pid),
                        procs.status(pid),
                    )
                })?
            };
            left = Some((pid, how));
        }
    }

    /// **The clock interrupt**, the kernel's half: what `trap()` does
    /// around it (`pc/trap.c:339`, `m->intr++`; `intrtime`, `:271`) and the
    /// portable `timerintr` it reaches through the machine's `clockintr`
    /// (`kw/clock.c:46`, `i8253clock` on the PC). The counters are `Mach`'s,
    /// and the machine cannot reach `Mach`, so they are counted here.
    ///
    /// `pc` is `ur->pc` for an interrupt taken in user mode, and `None` for
    /// one taken in the kernel.
    pub fn timerintr(&mut self, now: u64, pc: Option<&dyn Fn() -> u64>) {
        let mut procs = self.procs.borrow_mut();
        procs.m.intr += 1;
        procs.m.perf.intrts = now;
        procs.timerintr(now, pc);
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

    /// Step 1: `namec(file, Aopen, OEXEC, 0)` — resolve through the
    /// process's namespace and open for execution. A reference, as every
    /// open answers: `exec` of `/fd/3` reads fd 3's own channel.
    fn exec_open(&mut self, pid: Pid, path: &str) -> Result<std::rc::Rc<std::cell::RefCell<Chan>>, String> {
        let (slash, dot, ns) = self.names(pid)?;
        let ns = ns.borrow();
        namec::open(&mut self.tab, &ns, &slash, &dot, path, chan::mode::OEXEC)
    }

    /// Steps 1 and 2 alone: resolve and read. It is entirely Plan 9's, and
    /// can be tested without a machine.
    ///
    /// **Through the dispatcher, not at the device.** `sysexec` reads the
    /// image with `c->dev->read` reached from `devtab` (`sysproc.c:310`),
    /// which for a mounted file is the mount driver. Reaching the device
    /// directly worked for as long as every binary was carried in `#/`, and
    /// stopped the moment the commands moved onto a file server — which is
    /// what `/bin` IS on Plan 9.
    pub fn exec_image(&mut self, pid: Pid, path: &str) -> Result<Vec<u8>, String> {
        let c = self.exec_open(pid, path)?;
        let mut image = Vec::new();
        let mut cc = c.borrow().clone();
        let r = loop {
            match self.tab.dread(&mut cc, 8192, image.len() as u64) {
                Ok(got) if got.is_empty() => break Ok(image),
                Ok(got) => image.extend_from_slice(&got),
                Err(e) => break Err(e),
            }
        };
        self.tab.cclose(c);
        r
    }

    /// `attachimage` (`segment.c:245`): the text segment of the image `c`
    /// is, found in the cache if something is running it already, else new.
    fn attachimage(&mut self, c: &Chan, len: u64) -> std::rc::Rc<std::cell::RefCell<proc::Segment>> {
        let key = ImageKey {
            dev: c.dev,
            devno: c.devno,
            qid: (c.qid.path, c.qid.vers),
            mqid: (c.mqid.path, c.mqid.vers),
            mchan: c.mchan.as_ref().map(|m| (m.dev, m.devno, m.qid.path)),
        };
        self.images.retain(|(_, s)| s.strong_count() > 0);
        if let Some(s) = self.images.iter().find(|(k, _)| *k == key).and_then(|(_, s)| s.upgrade()) {
            return s;
        }
        let s = std::rc::Rc::new(std::cell::RefCell::new(proc::Segment { size: len, ..Default::default() }));
        self.images.push((key, std::rc::Rc::downgrade(&s)));
        s
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
        /// The arguments each image was given, in the same order.
        pub(crate) args: Vec<Vec<String>>,
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
        fn touser(&self, pid: Pid, image: &[u8], a: &[String]) -> Result<(), String> {
            let mut l = self.0.borrow_mut();
            l.order.push("touser");
            l.ran.push((pid, image.to_vec()));
            l.args.push(a.to_vec());
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

    /// **A kernel standing on a file server, as the system's does**: `init`
    /// and `files` served from memory ([`crate::testfs`]) over `#9/0`, and
    /// mounted by pid 1 as the host mounts the store before the first
    /// program runs — Plan 9's `connectroot` and `nsinit` (`boot.c:124`,
    /// `:151`): the version, `/` made a union of its own, the server on
    /// `/root`, and `/root` after `/`. So `/init` is the server's `init`.
    pub(crate) fn rooted(machine: Rc<dyn machine::Machine>, files: &[(&str, &[u8])]) -> Kernel {
        use crate::ns::mflag::{MAFTER, MCREATE, MREPL};
        let mut all: Vec<(&str, &[u8])> = vec![("init", b"an image")];
        all.extend_from_slice(files);
        let mut k = Kernel::new(devroot::Root::new(), machine).unwrap();
        k.tab.add(Box::new(devmnt::MntDev::new()));
        let mut v9 = devvirtio9p::Virtio9p::new();
        v9.add(Box::new(testfs::Files::new(&all)));
        k.tab.add(Box::new(v9));
        let Ret::Fd(fd) = k.syscall(1, Call::Open { path: "#9/0".into(), mode: 2 }).unwrap() else {
            panic!("#9/0")
        };
        k.syscall(1, Call::Fversion { fd, msize: 0, version: String::new() }).unwrap();
        k.syscall(1, Call::Bind { name: "/".into(), old: "/".into(), flag: MREPL }).unwrap();
        let mount = Call::Mount { fd, afd: -1, old: "/root".into(), flag: MREPL | MCREATE, aname: String::new() };
        k.syscall(1, mount).unwrap();
        k.syscall(1, Call::Bind { name: "/root".into(), old: "/".into(), flag: MAFTER | MCREATE }).unwrap();
        // no `close(fd)`: the mount has closed it (`bindmount`'s *"fdclose(fd,
        // 0)"*, `sysfile.c:1061`)
        k
    }

    /// A kernel on a root holding `init`, and the log its machine writes to.
    pub(crate) fn watched() -> (Kernel, Rc<RefCell<Log>>) {
        watched_with(&[])
    }

    /// The same, with more files in the root.
    pub(crate) fn watched_with(files: &[(&str, &[u8])]) -> (Kernel, Rc<RefCell<Log>>) {
        let log = Rc::new(RefCell::new(Log::default()));
        let k = rooted(Rc::new(Recorder(log.clone())), files);
        (k, log)
    }

    /// **A `#!` script runs its interpreter** (`sysproc.c:340`–`:360`): the
    /// interpreter named on the line, given the script's last element as
    /// `argv[0]`, the line's own arguments, the script's name as it was
    /// given, and the caller's arguments after its `argv[0]`. The process
    /// is called by the script's name, not the interpreter's.
    #[test]
    fn a_script_runs_its_interpreter_with_the_script_as_an_argument() {
        let (mut k, log) = watched_with(&[("s", b"#!/init -x\tyz\necho body\n")]);
        k.exec(1, "/s", &["called".into(), "a".into(), "b".into()]).unwrap();
        let l = log.borrow();
        assert_eq!(l.ran, vec![(1, b"an image".to_vec())], "the interpreter's image");
        assert_eq!(l.args, vec![vec!["s", "-x", "yz", "/s", "a", "b"]]);
        assert_eq!(k.procs.borrow().get(1).unwrap().text, "s");
    }

    /// What `sysexec` refuses with `Ebadexec`: fewer than two bytes; a `#!`
    /// line with no newline within `sizeof(Exec)`, or with nothing on it
    /// (`shargs`, `sysproc.c:601`); and an interpreter that is itself a
    /// script — one level only (`:343`). A missing interpreter is the walk's
    /// own error. The machine is never asked.
    #[test]
    fn what_a_script_cannot_be() {
        let long = format!("#!/init {}\n", "x".repeat(40));
        let (mut k, log) = watched_with(&[
            ("one", b"#"),
            ("long", long.as_bytes()),
            ("blank", b"#!  \t\n"),
            ("inner", b"#!/blank\n"),
            ("outer", b"#!/inner\n"),
            ("nowhere", b"#!/nothing\n"),
        ]);
        for name in ["one", "long", "blank", "outer"] {
            assert_eq!(k.exec(1, &format!("/{name}"), &[name.into()]), Err(EBADEXEC.into()), "{name}");
        }
        assert!(k.exec(1, "/nowhere", &["nowhere".into()]).unwrap_err().contains("does not exist"));
        assert!(log.borrow().ran.is_empty());
    }

    fn booted() -> Kernel {
        rooted(Rc::new(Recorder::silent()), &[])
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
        assert_eq!(k.exec_image(1, "/init").unwrap(), b"an image");
    }

    /// `exec` must hand the machine the bytes it resolved. Asserting only on
    /// the returned status passes against a machine that ignored the image.
    /// `exec` must hand the machine the bytes it resolved. Asserting only on
    /// the returned status passes against a machine that ignored the image.
    #[test]
    fn exec_runs_the_image_it_resolved() {
        let (mut k, log) = tests::watched();
        k.exec(1, "/init", &[]).unwrap();
        assert_eq!(log.borrow().ran, vec![(1, b"an image".to_vec())]);
    }

    /// `sysexec` does the machine's half in one order: set the process up,
    /// then enter it.
    #[test]
    fn procsetup_runs_before_touser() {
        let (mut k, log) = tests::watched();
        k.exec(1, "/init", &[]).unwrap();
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
             WSTAT FWSTAT FVERSION FAUTH ERRSTR SEMACQUIRE TSEMACQUIRE SEMRELEASE"
            .split_whitespace()
        {
            assert!(plan9.split_whitespace().any(|p| p == c), "{c} is not a Plan 9 syscall");
            for forbidden in ["DRAW", "TIME", "RANDOM", "FETCH", "STORE", "WINDOW", "CONSOLE"] {
                assert_ne!(c, forbidden, "{c} is a file server's job");
            }
        }
    }
}

/// The call numbers (`libc/9syscall/sys.h`) — `up->scallnr`.
pub mod sysno {
    pub const BIND: u32 = 2;
    pub const CHDIR: u32 = 3;
    pub const CLOSE: u32 = 4;
    pub const DUP: u32 = 5;
    pub const ALARM: u32 = 6;
    pub const EXEC: u32 = 7;
    pub const EXITS: u32 = 8;
    pub const OPEN: u32 = 14;
    pub const SLEEP: u32 = 17;
    pub const RFORK: u32 = 19;
    pub const PIPE: u32 = 21;
    pub const CREATE: u32 = 22;
    pub const REMOVE: u32 = 25;
    pub const NOTIFY: u32 = 28;
    pub const NOTED: u32 = 29;
    pub const RENDEZVOUS: u32 = 34;
    pub const UNMOUNT: u32 = 35;
    pub const SEMACQUIRE: u32 = 37;
    pub const SEMRELEASE: u32 = 38;
    pub const SEEK: u32 = 39;
    pub const FD2PATH: u32 = 23;
    pub const FVERSION: u32 = 40;
    pub const FAUTH: u32 = 10;
    pub const SYSR1: u32 = 0;
    pub const SEGBRK: u32 = 12;
    pub const SEGATTACH: u32 = 30;
    pub const SEGDETACH: u32 = 31;
    pub const SEGFREE: u32 = 32;
    pub const SEGFLUSH: u32 = 33;
    pub const ERRSTR: u32 = 41;
    pub const STAT: u32 = 42;
    pub const OLDFSTAT: u32 = 11;
    pub const OLDSTAT: u32 = 18;
    pub const FSTAT: u32 = 43;
    pub const WSTAT: u32 = 44;
    pub const FWSTAT: u32 = 45;
    pub const MOUNT: u32 = 46;
    pub const AWAIT: u32 = 47;
    pub const PREAD: u32 = 50;
    pub const PWRITE: u32 = 51;
    pub const TSEMACQUIRE: u32 = 52;
}

/// A call's number, `scallnr`.
fn scallnr(c: &Call) -> u32 {
    use sysno::*;
    match c {
        Call::Bad { n } => *n,
        Call::Sysr1 => SYSR1,
        Call::Rfork { .. } => RFORK,
        Call::Exec { .. } => EXEC,
        Call::Exits { .. } => EXITS,
        Call::Await => AWAIT,
        Call::Sleep { .. } => SLEEP,
        Call::Alarm { .. } => ALARM,
        Call::Notify { .. } => NOTIFY,
        Call::Noted { .. } => NOTED,
        Call::Rendezvous { .. } => RENDEZVOUS,
        Call::Semacquire { .. } => SEMACQUIRE,
        Call::Tsemacquire { .. } => TSEMACQUIRE,
        Call::Semrelease { .. } => SEMRELEASE,
        Call::Bind { .. } => BIND,
        Call::Mount { .. } => MOUNT,
        Call::Unmount { .. } => UNMOUNT,
        Call::Chdir { .. } => CHDIR,
        Call::Open { .. } => OPEN,
        Call::Create { .. } => CREATE,
        Call::Close { .. } => CLOSE,
        Call::Pread { .. } => PREAD,
        Call::Pwrite { .. } => PWRITE,
        Call::Seek { .. } => SEEK,
        Call::Dup { .. } => DUP,
        Call::Pipe => PIPE,
        Call::Remove { .. } => REMOVE,
        Call::Stat { .. } => STAT,
        Call::OldStat { .. } => OLDSTAT,
        Call::OldFstat { .. } => OLDFSTAT,
        Call::Fstat { .. } => FSTAT,
        Call::Wstat { .. } => WSTAT,
        Call::Fwstat { .. } => FWSTAT,
        Call::Fversion { .. } => FVERSION,
        Call::Fauth { .. } => FAUTH,
        Call::Fd2path { .. } => FD2PATH,
        Call::Errstr { .. } => ERRSTR,
    }
}

/// What the process is answered — the value `syscall()` puts in the return
/// register (`pc/trap.c:751`, *"ureg->ax = ret"*), which on this machine the
/// host turns each answer into. A failure is −1.
fn retval(nr: u32, s: &[u64; machine::MAXSYSARG], r: &Result<Ret, String>) -> i64 {
    let Ok(v) = r else { return -1 };
    match v {
        Ret::Ok | Ret::Two(..) | Ret::Sched => 0,
        Ret::Fd(fd) => *fd as i64,
        Ret::N(n) if nr == sysno::SEEK => *n as i64,
        Ret::N(n) => *n as u32 as i32 as i64,
        Ret::Data(d) => d.len() as i64,
        Ret::Pid(p) => *p as i64,
        // `await` answers what fitted in the caller's buffer; `errstr`, 0.
        Ret::Str(m) if nr == sysno::AWAIT => m.len().min(s[1] as u32 as usize) as i64,
        // `sysfversion` answers the version's length (`auth.c:23`)
        Ret::Str(m) if nr == sysno::FVERSION => m.len() as i64,
        Ret::Str(_) | Ret::Wait(..) => 0,
    }
}

/// `Ebadexec` (`port/error.h:34`).
pub const EBADEXEC: &str = "exec header invalid";

/// `pathlast` (`sysfile.c:917`): the last element of a channel's name.
fn pathlast(p: &str) -> Option<&str> {
    if p.is_empty() {
        return None;
    }
    Some(match p.rfind('/') {
        Some(i) => &p[i + 1..],
        None => p,
    })
}

/// `dirsetname` (`sysfile.c:420`): the stat with its name replaced.
fn dirsetname(name: &str, d: Vec<u8>) -> Vec<u8> {
    match ninep::Dir::conv_m2d(&d) {
        Some(mut dir) => {
            dir.name = name.to_string();
            dir.conv_d2m()
        }
        None => d,
    }
}

/// The rest of `sys_stat`/`sys_fstat` (`sysfile.c:1274`–`:1283`), and
/// `packoldstat` (`:1224`): name, uid and gid in 28 bytes each, then the
/// qid's path — with `DMDIR` set only for a directory — its version, the
/// mode, the times, the length, the type and the device.
fn oldstat(c: &chan::Chan, d: Vec<u8>, old: &str) -> Result<Ret, String> {
    // *"uchar buf[128]"*: a stat that does not fit is `BIT16SZ` from the
    // device, and *"if(l <= BIT16SZ) error(old)"*
    if d.len() > 128 {
        return Err(old.into());
    }
    let d = match pathlast(&c.path) {
        Some(name) => dirsetname(name, d),
        None => d,
    };
    let dir = match ninep::Dir::conv_m2d(&d) {
        Some(dir) if d.len() <= 128 => dir,
        _ => return Err(old.into()),
    };
    let mut b = vec![0u8; 116];
    // `strncpy(p, s, 28)`: at most 28 bytes, the rest zero
    for (i, s) in [&dir.name, &dir.uid, &dir.gid].iter().enumerate() {
        let n = s.len().min(28);
        b[i * 28..i * 28 + n].copy_from_slice(&s.as_bytes()[..n]);
    }
    const DMDIR: u32 = 0x8000_0000;
    let mut q = (dir.qid.path as u32) & !DMDIR;
    if dir.qid.qtype & ninep::QTDIR != 0 {
        q |= DMDIR;
    }
    let mut at = 84;
    for w in [q, dir.qid.vers, dir.mode, dir.atime, dir.mtime] {
        b[at..at + 4].copy_from_slice(&w.to_le_bytes());
        at += 4;
    }
    b[at..at + 8].copy_from_slice(&dir.length.to_le_bytes());
    at += 8;
    b[at..at + 2].copy_from_slice(&dir.dtype.to_le_bytes());
    at += 2;
    b[at..at + 2].copy_from_slice(&(dir.dev as u16).to_le_bytes());
    Ok(Ret::Data(b))
}

/// `sizeof(Exec)` (`a.out.h:2`): eight `long`s. `sysexec` reads the header
/// and copies this much of it into `line`, the buffer `shargs` reads a `#!`
/// line from (`sysproc.c:342`), so the line must end within it.
const SIZEOF_EXEC: usize = 32;

/// `sizeof progelem` (`sysproc.c:271`, *"char progelem[64]"*).
const PROGELEM: usize = 64;

/// `shargs` (`sysproc.c:601`): the words of a `#!` line, split at blanks
/// and tabs — the interpreter and its arguments. `None` is *"return 0"*: no
/// newline within the line, or no words on it.
///
/// Plan 9 passes it `n`, the bytes read, which can be `sizeof(exec)` — 40,
/// with the 64-bit entry — while `line` holds `sizeof(Exec)`, 32; a newline
/// past the 32nd byte is looked for beyond the end of `line`. Here the line
/// is what `line` holds, so it must end within its 32 bytes.
fn shargs(image: &[u8]) -> Option<Vec<String>> {
    let line = &image[..image.len().min(SIZEOF_EXEC)];
    // *"s += 2; n -= 2; for(i=0; s[i]!='\n'; i++) if(i == n-1) return 0;"*
    let s = &line[2..];
    let end = s.iter().position(|&b| b == b'\n')?;
    let words: Vec<String> = s[..end]
        .split(|&b| b == b' ' || b == b'\t')
        .filter(|w| !w.is_empty())
        .map(|w| String::from_utf8_lossy(w).into_owned())
        .collect();
    (!words.is_empty()).then_some(words)
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
        Call::OldStat { .. } => "_stat",
        Call::OldFstat { .. } => "_fstat",
        Call::Fstat { .. } => "Fstat",
        Call::Wstat { .. } => "Wstat",
        Call::Fwstat { .. } => "Fwstat",
        Call::Errstr { .. } => "Errstr",
        Call::Fversion { .. } => "Fversion",
        Call::Fauth { .. } => "Fauth",
        // `systab.h:114`'s names: the table of names is whole even where
        // the table of calls is not
        Call::Sysr1 => "Sysr1",
        Call::Bad { n } => match *n {
            sysno::SEGBRK => "Segbrk",
            sysno::SEGATTACH => "Segattach",
            sysno::SEGDETACH => "Segdetach",
            sysno::SEGFREE => "Segfree",
            sysno::SEGFLUSH => "Segflush",
            _ => "huh?",
        },
        Call::Fd2path { .. } => "Fd2path",
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
        let _ = k.pexit(me, "no work", true);
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
    closeproc_close(k, me, c)
}

/// `closeproc`'s *"devtab[c->type]->close(c)"* (`chan.c:575`), which may
/// wait — the mount driver for `Rclunk` — and then the loop goes on: what
/// is kept is this close and the loop after it. `false` if it went to sleep.
fn closeproc_close(k: &mut Kernel, me: Pid, c: Chan) -> bool {
    let mut cc = c.clone();
    k.tab.dclose(&mut cc);
    if k.procs.borrow().get(me).is_some_and(|p| p.setlabel) {
        k.labels.insert(
            me,
            Box::new(move |k, me| if closeproc_close(k, me, c) { closeproc(k, me) } else { Ok(Ret::Ok) }),
        );
        return false;
    }
    k.tab.endcall(me);
    // what that close let go of in turn, closed next
    let more = k.tab.deferred(me);
    k.procs.borrow_mut().clunkq.splice(0..0, more);
    true
}

impl machine::Syscalls for Kernel {
    fn syscall(&mut self, up: Pid, call: Call, ureg: &machine::Ureg) -> Result<Ret, String> {
        if let Some(p) = self.procs.borrow_mut().get_mut(up) {
            p.s = ureg.s;
        }
        self.syscall_(up, call, ureg.pc, true)
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

    fn timerintr(&mut self, pc: &dyn Fn() -> u64) -> bool {
        let now = self.machine.todget().nsec;
        Kernel::timerintr(self, now, Some(pc));
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
    ///
    /// A call made here, rather than trapped from a process, carries no
    /// argument words — the kernel's own calls at boot pass their names
    /// from the kernel's memory — so it is not checked as a process's is
    /// ([`Kernel::validargs`]).
    pub fn syscall(&mut self, up: Pid, call: Call) -> Result<Ret, String> {
        self.syscall_(up, call, &|| 0, false)
    }

    fn syscall_(&mut self, up: Pid, call: Call, pc: &dyn Fn() -> u64, trapped: bool) -> Result<Ret, String> {
        // **A new call has nothing to go back to.** What a call leaves in
        // `labels` is for its own `resume`; one left now belonged to a call
        // that will never be resumed — an `exec`, whose answer was kept for
        // a delayed `sched` while the machine started the new image — and
        // the new image's first call that slept was answered with it.
        self.labels.remove(&up);
        self.tab.endcall(up);
        // *"m->syscall++; up->insyscall = 1;"* and *"up->scallnr =
        // scallnr"* (`pc/trap.c:673`–`:680`).
        let traced = {
            let mut procs = self.procs.borrow_mut();
            procs.m.syscall += 1;
            match procs.get_mut(up) {
                Some(p) => {
                    p.insyscall = true;
                    p.scallnr = scallnr(&call);
                    if let Call::Rfork { flags } = call {
                        p.s[0] = flags as u32 as u64;
                    }
                    p.procctl == Some(proc::Procctl::Tracesyscall)
                }
                None => false,
            }
        };
        // **`up` is the calling process, and it is set on the way in.** Plan 9
        // does not have to: `syscall()` (`pc/trap.c:665`) runs on the trapping
        // process's own kernel stack, so the per-machine `up` already names
        // it. Here `up` is a shared cell the devices read through — `up->user`
        // in `devsrv`, `up->fgrp` in `devdup`, `up->egrp` in `devenv` — and if
        // it is not set, every one of them answers for whoever ran last.
        self.up.borrow_mut().pid = up;
        // Each call's own checks of the addresses it was given, before it
        // does anything — see `validargs` for where they are and why here.
        // *"if(up->procctl == Proc_tracesyscall){ syscallfmt(…); up->procctl
        // = Proc_stopme; procctl(up); … startns = todget(nil, nil); }"*
        // (`pc/trap.c:682`) — stop before the call, with what it is about
        // to do where a tracer reads it; the call runs when it is started.
        if traced {
            // `syscallfmt` reads the process's strings, and a bad address is
            // `validaddr`'s error there, raised outside the call's own
            // `waserror`: the call does not run, and fails with it.
            let t = match self.syscallfmt(up, &call, pc) {
                Ok(t) => t,
                Err(e) => {
                    self.procs.borrow_mut().get_mut(up).map(|p| p.procctl = None);
                    return self.syscall_tail(up, Err(e));
                }
            };
            {
                let mut procs = self.procs.borrow_mut();
                let p = procs.get_mut(up).expect("traced");
                p.syscalltrace = Some(t);
                p.procctl = Some(proc::Procctl::Stopme);
            }
            self.procctl(up);
            self.procs.borrow_mut().take_setlabel(up);
            // The call runs after the stop, and a check of its addresses
            // there wants the pc it was made from.
            let pcv = pc();
            self.labels.insert(up, Box::new(move |k, up| {
                let now = k.machine.todget().nsec;
                if let Some(p) = k.procs.borrow_mut().get_mut(up) {
                    p.syscalltrace = None;
                    p.startns = now;
                }
                k.dispatch_(up, call, &|| pcv, trapped)
            }));
            return Ok(Ret::Sched);
        }
        let r = self.dispatch_(up, call, pc, trapped);
        self.syscall_tail(up, r)
    }

    /// *"up->psstate = sysctab[scallnr]; ret = systab[scallnr](up->s.args);"*
    /// (`pc/trap.c:727`) — what `ps` shows while the call lasts, and the call.
    ///
    /// A trapped call first checks the addresses it was given, as each of
    /// Plan 9's does at its top ([`Kernel::validargs`]) — after the tracer's
    /// stop, as there.
    fn dispatch_(&mut self, up: Pid, call: Call, pc: &dyn Fn() -> u64, trapped: bool) -> Result<Ret, String> {
        // *"if(scallnr >= nsyscall || systab[scallnr] == 0){ pprint("bad sys
        // call number %lud pc %lux\n", …); postnote(up, 1, "sys: bad sys
        // call", NDebug); error(Ebadarg); }"* (`pc/trap.c:716`) — before
        // `psstate` is set, as there.
        if let Call::Bad { n } = call {
            self.pprint(up, &format!("bad sys call number {n} pc {:x}\n", pc()));
            machine::Syscalls::postnote(self, up, "sys: bad sys call", proc::NoteFlag::NDebug);
            return Err(proc::Procs::EBADARG.into());
        }
        if let Some(p) = self.procs.borrow_mut().get_mut(up) {
            p.psstate = Some(sysctab(&call).to_string());
        }
        let mut call = call;
        if trapped {
            self.validargs(up, &call, pc)?;
            // `sysexits` (`sysproc.c:671`–`:675`): *"if(waserror()) status =
            // inval; else { validaddr((uintptr)status, 1, 0); …"* — a bad
            // status is not an error but *"invalid exit string"*, and
            // `validaddr` has posted its note on the way.
            let status = self.procs.borrow().get(up).map_or(0, |p| p.s[0] as u32);
            if matches!(call, Call::Exits { .. }) && status != 0 && self.validaddr(up, status, 1, pc).is_err() {
                call = Call::Exits { status: "invalid exit string".into() };
            }
        }
        // **A call the mount driver left asleep runs again** when it is
        // woken — `mountio`'s sleep, on a stack this kernel does not keep
        // (`devmnt.c:811`; [`namec::Record`]). A call that kept the rest of
        // itself already (a read of a pipe) is not touched.
        self.rerun(up, call)
    }

    /// The call, and if the mount driver leaves it asleep, the same call
    /// again when the process is entered again — as often as it sleeps.
    fn rerun(&mut self, up: Pid, call: Call) -> Result<Ret, String> {
        let again = call.clone();
        let r = self.dispatch(up, call);
        if self.procs.borrow().get(up).is_some_and(|p| p.setlabel) && !self.labels.contains_key(&up) {
            self.setlabel(up, Box::new(move |k, up| k.rerun(up, again)));
        }
        r
    }

    /// **The process is entered again in the middle of a call it left** —
    /// `sleep`'s `setlabel` answering 1 (`proc.c:830`). The rest of the call
    /// runs, and ends as any call does.
    pub fn resume(&mut self, up: Pid) -> Result<Ret, String> {
        self.up.borrow_mut().pid = up;
        let label = self.labels.remove(&up).ok_or("the process left no call to go back to")?;
        self.tab.rewind(up);
        let r = label(self, up);
        self.syscall_tail(up, r)
    }

    /// The end of `syscall()` (`pc/trap.c:739`–`:780`), after the call's own
    /// work. What it does depends on which call it was, `up->scallnr`: an
    /// `rfork` is not followed by `notify` (`:773`, *"scallnr!=RFORK"*), and
    /// after `rfork(RFPROC)` `sysrfork`'s own *"ready(p); sched();"* is
    /// still to come — and that `sched` zeroes `up->delaysched`
    /// (`proc.c:154`) before `syscall()` reaches *"if(up->delaysched)
    /// sched();"*, so the switch after an `rfork` is `sysrfork`'s, never a
    /// second one. The machine takes it when the call has made the child
    /// (RESEARCH §5.2, §15.9).
    fn syscall_tail(&mut self, up: Pid, r: Result<Ret, String>) -> Result<Ret, String> {
        let (rfork, rforked) = {
            let procs = self.procs.borrow();
            let p = procs.get(up);
            let rfork = p.is_some_and(|p| p.scallnr == sysno::RFORK);
            (rfork, rfork && p.is_some_and(|p| p.s[0] as i32 & proc::rf::PROC != 0))
        };
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
        // **The closes the call left to make** ([`namec::Devtab::cclose`]),
        // made before it returns, as Plan 9's are made inside it — each of
        // which may wait.
        let deferred = self.tab.deferred(up);
        if !deferred.is_empty() {
            let r = self.closethen(up, deferred, Box::new(move |_, _| r));
            return self.syscall_tail(up, r);
        }
        // The call is over, and what it did on the wires is forgotten.
        self.tab.endcall(up);
        // **A clock interrupt that fell due during the call.** This kernel
        // runs a call to its end with nothing able to interrupt it — the
        // machine's interrupt is only taken in guest code — which is Plan
        // 9's kernel running `splhi`. An interrupt held off by `splhi` is
        // taken at `spllo`, and the call's end is where that is: still
        // `insyscall`, so the tick is `TSys`'s (`accounttime`, `proc.c:1624`).
        let now = self.machine.todget().nsec;
        if self.procs.borrow().m.hz.is_some_and(|h| h <= now) {
            self.timerintr(now, None);
        }
        // *"if(up->procctl == Proc_tracesyscall){ stopns = todget(nil, nil);
        // up->procctl = Proc_stopme; sysretfmt(…); procctl(up); … }"*
        // (`pc/trap.c:755`) — stop after the call, with what it answered.
        // When the process is started again it comes back here with the
        // trace still set, and *"free(up->syscalltrace); up->syscalltrace =
        // nil"* is what it does then.
        //
        // A process stopped later than this — in `notify`'s `procctl`, or
        // by the `sched` at the very end — is past this point, and has
        // `insyscall` clear already (`pc/trap.c:767`): started again with
        // `startsyscall`, it is traced from its NEXT call, not this one.
        let (back, tracing, startns) = {
            let mut procs = self.procs.borrow_mut();
            match procs.get_mut(up) {
                Some(p) => (
                    p.syscalltrace.take().is_some(),
                    p.insyscall && p.procctl == Some(proc::Procctl::Tracesyscall),
                    p.startns,
                ),
                None => (false, false, 0),
            }
        };
        if !back && tracing {
            let t = self.sysretfmt(up, &r, startns, now);
            {
                let mut procs = self.procs.borrow_mut();
                let p = procs.get_mut(up).expect("tracing");
                p.procctl = Some(proc::Procctl::Stopme);
                p.syscalltrace = Some(t);
            }
            self.procctl(up);
            self.procs.borrow_mut().take_setlabel(up);
            let answer = r.clone();
            self.labels.insert(up, Box::new(move |_, _| answer));
            return Ok(Ret::Sched);
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

    /// A channel the call walked to, used once, and closed — *"if(waserror()){
    /// cclose(c); nexterror(); } … poperror(); cclose(c);"* (`sysstat`,
    /// `sysfile.c:951`) — when it is the call's own. Not when the call has
    /// left the processor: it runs again from the top, and the record gives
    /// the close back then.
    fn done<T>(&mut self, mut c: Chan, owned: bool, f: impl FnOnce(&mut Kernel, &mut Chan) -> Result<T, String>) -> Result<T, String> {
        let r = f(self, &mut c);
        if owned && r.as_ref().err().is_none_or(|e| e != devmnt::SLEPT) {
            self.tab.dclose(&mut c);
        }
        r
    }

    /// **Last closes that wait, then the rest of the call.** `cclose`
    /// (`chan.c:490`) of each channel in turn, and a close may leave the
    /// processor — the mount driver waiting for `Rclunk` (`mntclunk`). Plan 9
    /// waits there on the process's kernel stack and goes on from the line;
    /// here what is kept is the closes not yet finished and `then`, the rest
    /// of the call (`p->sched`). It is for a close made where the call could
    /// not simply run again — after the descriptor is gone, the slot
    /// replaced, the image committed.
    ///
    /// What the call did on the wires before is over, so its record goes;
    /// each close's own RPCs are what the record gives back when the process
    /// is entered again.
    fn closethen(&mut self, up: Pid, cs: Vec<Chan>, then: Label) -> Result<Ret, String> {
        self.tab.endcall(up);
        // what the call already let go of comes first
        let mut cs = cs;
        cs.splice(0..0, self.tab.deferred(up));
        self.closing(up, cs, then)
    }

    fn closing(&mut self, up: Pid, mut cs: Vec<Chan>, then: Label) -> Result<Ret, String> {
        while !cs.is_empty() {
            let mut c = cs[0].clone();
            self.tab.dclose(&mut c);
            if self.procs.borrow().get(up).is_some_and(|p| p.setlabel) {
                self.setlabel(up, Box::new(move |k, up| k.closing(up, cs, then)));
                return Ok(Ret::Ok);
            }
            cs.remove(0);
            self.tab.endcall(up);
            // what that close let go of in turn, closed before the rest
            cs.splice(0..0, self.tab.deferred(up));
        }
        then(self, up)
    }

    /// The record `pwait` takes once `haswaitq` is true.
    fn waitrecord(&mut self, up: Pid) -> Result<Ret, String> {
        match self.procs.borrow_mut().await_child(up) {
            Some(w) => Ok(Ret::Str(w.format())),
            None => Err(ENOCHILD.into()),
        }
    }

    /// `sysread` → `read` (`sysfile.c:627`) on a channel.
    ///
    /// **Taken out and put back, not held.** Plan 9 hands the device the
    /// `Chan*` an fd holds and nothing minds that the device may reach the
    /// same channel again through the fd table — `dupgen` reads `c->mode` of
    /// every open descriptor (`devdup.c:34`), which is exactly what `ls
    /// /dev` does when `#d` is in its union. Holding the borrow across the
    /// call makes that a panic; copying only the offset back loses `dri`
    /// and `uri`, which is how a directory read came to start over every
    /// time. What the device changed goes back — and the offset is added
    /// to as it is then, *"lock(c); … c->offset += nnn"* (`:683`), not put
    /// back as it was when the read began: another process on the same
    /// descriptor may have moved it while this one slept.
    ///
    /// **A device may leave in the middle** — `qread` on an empty pipe
    /// sleeps. The rest of the read is then this again, on the same
    /// channel: the device knows where it was.
    fn pread(&mut self, up: Pid, cell: std::rc::Rc<std::cell::RefCell<chan::Chan>>, n: usize, off: i64) -> Result<Ret, String> {
        // `~0` is the channel's own offset (`syspread`, `sysfile.c:713`);
        // any other below 0 is *"error(Enegoff)"* (`:657`)
        if off < -1 {
            return Err(ENEGOFF.into());
        }
        let mut c = cell.borrow().clone();
        let at = if off == -1 { c.offset } else { off as u64 };
        // *"if(off == 0){ /* rewind to the beginning of the directory */"*
        // (`:660`): the channel's offset, if it is the one read at, and
        // the union's place — `unionrewind`, which closes the element open.
        if at == 0 {
            if off == -1 {
                cell.borrow_mut().offset = 0;
                c.offset = 0;
                c.dri = 0;
            }
            c.uri = 0;
            if let Some(u) = c.umc.take() {
                self.tab.defer(*u);
            }
        }
        // `read` (`sysfile.c:672`): **a directory reached through a union is
        // read from every element**, one after another. Without this `ls /`
        // shows whichever element answered the walk and nothing else —
        // which, once `/` is a union of the kernel's root and a file server,
        // is most of the system missing. Any other directory is read where
        // its channel is: *"if(off != c->offset) error(Edirseek)"* (`:675`).
        let d = if c.is_dir() && c.umh.is_some() {
            self.unionread(&mut c, n)?
        } else {
            if c.is_dir() && at != c.offset {
                return Err(EDIRSEEK.into());
            }
            self.tab.dread(&mut c, n, at)?
        };
        if self.procs.borrow().get(up).is_some_and(|p| p.setlabel) {
            self.setlabel(up, Box::new(move |k, up| k.pread(up, cell, n, off)));
            return Ok(Ret::Ok);
        }
        // *"if(c->qid.type & QTDIR || offp == nil)"* (`:683`): a directory's
        // offset moves whichever read it was
        let mut cur = cell.borrow_mut();
        let now = cur.offset;
        let advance = c.is_dir() || off == -1;
        *cur = c;
        cur.offset = if advance { now + d.len() as u64 } else { now };
        Ok(Ret::Data(d))
    }

    /// `syswrite` → `write` (`sysfile.c:722`). **The range is taken before
    /// the device writes** — *"lock(c); off = c->offset; c->offset += n;
    /// unlock(c);"* (`:744`) — so two processes writing through one
    /// descriptor never write the same bytes, and what the device did not
    /// write is given back after (`:757`, and all of it on an error,
    /// `:729`).
    fn pwrite(&mut self, up: Pid, cell: std::rc::Rc<std::cell::RefCell<chan::Chan>>, data: Vec<u8>, off: i64) -> Result<Ret, String> {
        if cell.borrow().is_dir() {
            return Err(EISDIR.into());
        }
        if off < -1 {
            return Err(ENEGOFF.into());
        }
        let taken = off == -1;
        let at = if taken {
            let mut c = cell.borrow_mut();
            let at = c.offset;
            c.offset += data.len() as u64;
            at
        } else {
            off as u64
        };
        self.pwriteat(up, cell, data, at, taken)
    }

    /// The write itself, at the offset taken — and again, at the same one,
    /// when the device has left the processor in the middle.
    fn pwriteat(&mut self, up: Pid, cell: std::rc::Rc<std::cell::RefCell<chan::Chan>>, data: Vec<u8>, at: u64, taken: bool) -> Result<Ret, String> {
        let mut c = cell.borrow().clone();
        let r = self.tab.dwrite(&mut c, &data, at);
        if self.procs.borrow().get(up).is_some_and(|p| p.setlabel) {
            self.setlabel(up, Box::new(move |k, up| k.pwriteat(up, cell, data, at, taken)));
            return Ok(Ret::Ok);
        }
        let m = *r.as_ref().unwrap_or(&0);
        let mut cur = cell.borrow_mut();
        let now = cur.offset;
        *cur = c;
        cur.offset = if taken { now - (data.len() - m) as u64 } else { now };
        r.map(Ret::N)
    }

    /// `pexit(exitstr, freemem)` (`proc.c:1123`), where it needs the
    /// kernel. **What the process held is closed first** —
    /// *"closefgrp(fgrp)"* (`:1160`), `cclose` of each channel whose last
    /// reference went, each of which may wait — and only then is the parent
    /// told: its `wait` returns once the server has let the files go. Then,
    /// when `freemem` is false, which is a note that was a fault or a
    /// suicide, *"addbroken(up)"* (`:1227`): the process is kept `Broken`
    /// for a debugger. `*nobroken` would stop that, and it is a
    /// configuration this system has no source for.
    ///
    /// A close that waits leaves the rest of `pexit` to run when the process
    /// is entered again; one reached from a note, with the image gone, runs
    /// on as kernel code ([`proc::Proc::noimage`]). A process killed while
    /// it closes waits for nothing: `forceclosefgrp` (`pgrp.c:245`) hands
    /// what is left to the close queue.
    pub fn pexit(&mut self, pid: Pid, status: &str, freemem: bool) -> Result<Ret, String> {
        // *"nil out all the resources under lock (free later)"*
        // (`proc.c:1145`), and of each, what was its last reference
        let (fds, rest) = {
            let mut procs = self.procs.borrow_mut();
            (procs.closefgrp(pid), procs.closedotpgrp(pid))
        };
        let status = status.to_string();
        self.closefgrp(pid, fds, Box::new(move |k, pid| {
            // *"if(dot) cclose(dot); if(pgrp) closepgrp(pgrp);"* (`:1165`)
            k.closing(pid, rest, Box::new(move |k, pid| {
                k.procs.borrow_mut().exits(pid, &status);
                k.tab.forget(pid);
                if !freemem {
                    k.procs.borrow_mut().addbroken(pid);
                }
                Ok(Ret::Ok)
            }))
        }))
    }

    /// `closefgrp` (`pgrp.c:207`) once the table's last reference has
    /// gone: *"up->closingfgrp = f"* … *"up->closingfgrp = nil"*
    /// (`:222`) around the closes, then `then`.
    fn closefgrp(&mut self, up: Pid, cs: Vec<Chan>, then: Label) -> Result<Ret, String> {
        self.tab.endcall(up);
        let mut cs = cs;
        cs.splice(0..0, self.tab.deferred(up));
        if let Some(p) = self.procs.borrow_mut().get_mut(up) {
            p.closingfgrp = true;
        }
        self.closingfgrp(up, cs, Box::new(move |k, up| {
            if let Some(p) = k.procs.borrow_mut().get_mut(up) {
                p.closingfgrp = false;
            }
            then(k, up)
        }))
    }

    /// `closefgrp`'s loop (`pgrp.c:223`), with `forceclosefgrp`'s way out:
    /// *"Called from sleep because up is in the middle of closefgrp and
    /// just got a kill ctl message … To break free, hand the unclosed
    /// channels to the close queue"* (`pgrp.c:234`).
    ///
    /// The close being made when the kill comes is not one of those: it
    /// goes on — `forceclosefgrp` takes only what is still in the table,
    /// and the close in progress was taken out of it first (*"f->fd[i] =
    /// nil; cclose(c)"*, `pgrp.c:225`). `begun` says one is.
    fn closingfgrp(&mut self, up: Pid, cs: Vec<Chan>, then: Label) -> Result<Ret, String> {
        self.closingfgrp_(up, cs, then, false)
    }

    fn closingfgrp_(&mut self, up: Pid, mut cs: Vec<Chan>, then: Label, begun: bool) -> Result<Ret, String> {
        let mut begun = begun;
        while !cs.is_empty() {
            let killed = self.procs.borrow().get(up).is_some_and(|p| p.procctl == Some(proc::Procctl::Exitme));
            if killed {
                let mut procs = self.procs.borrow_mut();
                for c in cs.drain(usize::from(begun)..) {
                    if !procs.ccloseq(c) {
                        procs.closeproc = true;
                    }
                }
                if cs.is_empty() {
                    break;
                }
            }
            let mut c = cs[0].clone();
            self.tab.dclose(&mut c);
            if self.procs.borrow().get(up).is_some_and(|p| p.setlabel) {
                self.setlabel(up, Box::new(move |k, up| k.closingfgrp_(up, cs, then, true)));
                return Ok(Ret::Ok);
            }
            cs.remove(0);
            begun = false;
            self.tab.endcall(up);
            cs.splice(0..0, self.tab.deferred(up));
        }
        then(self, up)
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
            self.tab.rewind(pid);
            let _ = body(self, pid);
        }
        if !self.procs.borrow_mut().take_setlabel(pid) {
            self.tab.endcall(pid);
        }
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
    /// `procctl` (`proc.c:1480`) — do what `p->procctl` asks. `None` is it
    /// asked nothing this can do now, and the caller carries on.
    fn procctl(&mut self, up: Pid) -> Option<machine::Notify> {
        use proc::{Procctl, State};
        let (procctl, nnote) = {
            let procs = self.procs.borrow();
            let p = procs.get(up)?;
            (p.procctl, p.note.len())
        };
        match procctl {
            // *"case Proc_exitme: pexit("Killed", 1)"*.
            Some(Procctl::Exitme) => {
                self.procs.borrow_mut().get_mut(up).map(|p| p.procctl = None);
                let _ = self.pexit(up, "Killed", true);
                Some(machine::Notify::Pexit)
            }
            // *"case Proc_traceme: if(p->nnote == 0) return; /* No break
            // */"* — a tracer asked to see the next note.
            Some(Procctl::Traceme) if nnote == 0 => None,
            // *"case Proc_stopme: p->procctl = 0; … p->psstate = "Stopped";
            // … wakeup(&p->pdbg->sleep) … p->state = Stopped; sched();"*.
            // The `psstate` it had is put back when it is started (`start`
            // clears it here, and the call's own resumes).
            Some(Procctl::Traceme) | Some(Procctl::Stopme) => {
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
                Some(machine::Notify::Sched)
            }
            Some(Procctl::Tracesyscall) | None => None,
        }
    }

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
        // *"if(up->procctl) procctl(up);"* (`pc/trap.c:796`). A fault leaves
        // nothing to stop in: the process is ending.
        if procctl.is_some() && !(at == machine::NoteAt::Fault && procctl != Some(Procctl::Exitme)) {
            if let Some(n) = self.procctl(up) {
                return n;
            }
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
            let _ = self.pexit(up, &n.msg, n.flag != NoteFlag::NDebug);
            return Notify::Pexit;
        }
        if notified {
            return Notify::No;
        }
        if handler == 0 {
            let _ = self.pexit(up, &n.msg, n.flag != NoteFlag::NDebug);
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
                let _ = self.pexit(up, &n.msg, n.flag != NoteFlag::NDebug);
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
            // answered by `dispatch_`, before a call is looked up
            Call::Bad { .. } => Err(proc::Procs::EBADARG.into()),
            // `sysr1` (`sysproc.c:25`): `checkpagerefs` checks the page
            // tables' reference counts, and this kernel has no pages; the
            // call answers 0, as there
            Call::Sysr1 => Ok(Ret::Ok),
            // ---- processes
            // `sysrfork` (`sysproc.c`) ends `ready(p); sched();` — **the
            // child goes on the run queue and the parent gives way to it**.
            // Without the `ready` a child exists and nothing can ever pick
            // it; without the `sched` the parent runs on. The `sched` is
            // the machine's, when the parent's `rfork` returns.
            //
            // Without `RFPROC` the tables it replaced are closed
            // (`closefgrp(ofg)`, `closepgrp(opg)`, `sysproc.c:61`, `:70`),
            // and each close may wait.
            Call::Rfork { flags } => {
                let forked = self.procs.borrow_mut().sysrfork(up, flags)?;
                match forked {
                    proc::Forked::Child(pid) => {
                        self.procs.borrow_mut().ready(pid);
                        Ok(Ret::Pid(pid))
                    }
                    proc::Forked::Same { fds, ns } => self.closefgrp(up, fds, Box::new(move |k, up| {
                        k.closing(up, ns, Box::new(|_, _| Ok(Ret::Pid(0))))
                    })),
                }
            }
            // **`exec` does not return** (`sysproc.c:259`). It gives the
            // process a new image and the process IS that image now; the
            // machine's answer is to leave, and `sched` enters what it
            // left behind.
            Call::Exec { path, args } => {
                self.exec(up, &path, &args)?;
                Ok(Ret::Ok)
            }
            Call::Exits { status } => self.pexit(up, &status, true),
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
            // `bindmount` (`sysfile.c:989`): the SOURCE is `Abind` and the
            // TARGET is `Amount` (`:1039`, `:1048`), in that order. Neither
            // is `Atodir`, so a file binds over a file — `bind /bin/rc
            // /bin/sh`. *"ret = cmount(&c0, c1, flag, spec)"* (`:1054`):
            // the namespace takes its own references to both, the call's
            // are closed — *"cclose(c1); … cclose(c0)"* — and so is what an
            // `MREPL` replaced. It answers the new mount's id (`chan.c:760`).
            Call::Bind { name, old, flag } => {
                bindflag(flag)?;
                let (c0, src0) = self.walk(up, &name, namec::A::Bind, 0)?;
                let union = c0.umh.as_ref().map(|h| h.borrow().mount.clone()).unwrap_or_default();
                let c0 = src0.unwrap_or_else(|| std::rc::Rc::new(c0));
                let c1 = match self.walk(up, &old, namec::A::Mount, 0) {
                    Ok((c1, src1)) => src1.unwrap_or_else(|| std::rc::Rc::new(c1)),
                    Err(e) => return self.unwind(up, e, vec![c0]),
                };
                let r = self.ns(up)?.borrow_mut().cmount(c1.clone(), ns::Element::shared(c0.clone(), flag, ""), union, bind_of(flag));
                self.mounted(up, r, c1, c0, None)
            }
            // `sysunmount` (`sysfile.c:1087`): the name mounted on is
            // `Amount`; the one mounted is *"namec(..., Aopen, ...)
            // because if arg[0] is something like /srv/cs or /fd/0, opening
            // it is the only way to get at the real Chan underneath"*
            // (`:1104`). Then `cunmount`, and both are the call's to close.
            Call::Unmount { name, old } => {
                let (on, onsrc) = self.walk(up, &old, namec::A::Mount, 0)?;
                let on = onsrc.unwrap_or_else(|| std::rc::Rc::new(on));
                let mounted = match &name {
                    Some(n) => match self.walk_open(up, n, chan::mode::OREAD) {
                        Ok(c) => Some(c),
                        Err(e) => return self.unwind(up, e, vec![on]),
                    },
                    None => None,
                };
                let m = mounted.as_ref().map(|c| c.borrow().clone());
                // *"eqchan(f->to, mounted, 1) || (f->to->mchan &&
                // eqchan(f->to->mchan, mounted, 1))"* (`chan.c:812`)
                let matches = m.map(|m| {
                    move |to: &Chan| ns::eqchan(to, &m) || to.mchan.as_deref().is_some_and(|w| ns::eqchan(w, &m))
                });
                let r = self.ns(up)?.borrow_mut().cunmount(&on, matches);
                let (r, mut gone) = match r {
                    Ok(gone) => (Ok(Ret::Ok), gone),
                    Err(e) => (Err(e), Vec::new()),
                };
                gone.push(on);
                let mut last = chan::lastrefs(gone);
                last.extend(mounted.and_then(|c| std::rc::Rc::try_unwrap(c).ok()).map(|c| c.into_inner()));
                self.closethen(up, last, Box::new(move |_, _| r))
            }
            // `syschdir` (`sysfile.c:976`): *"cclose(up->dot); up->dot =
            // c"* — the new `dot` is the reference `namec` answered, and
            // the old one's close may wait.
            Call::Chdir { path } => {
                let (c, src) = self.walk(up, &path, namec::A::Todir, 0)?;
                let c = src.unwrap_or_else(|| std::rc::Rc::new(c));
                let old = self.procs.borrow_mut().chdir(up, c);
                self.closethen(up, chan::lastrefs(old.into_iter().collect()), Box::new(|_, _| Ok(Ret::Ok)))
            }

            // ---- channels
            // *"openmode(arg[1]);	/* error check only */"* (`sysfile.c:270`)
            // — a mode past `OEXEC` is `Ebadarg` before a name is resolved;
            // `create` lets `OEXCL` through (`:1126`).
            Call::Open { path, mode } => {
                // a `ulong` there: nothing above 16 bits is a mode
                if !(0..=0xffff).contains(&mode) {
                    return Err(proc::Procs::EBADARG.into());
                }
                chan::openmode(mode as u16)?;
                let c = self.walk_open(up, &path, mode as u16)?;
                Ok(Ret::Fd(self.newfd(up, c)?))
            }
            Call::Create { path, mode, perm } => {
                if !(0..=0xffff).contains(&mode) {
                    return Err(proc::Procs::EBADARG.into());
                }
                chan::openmode(mode as u16 & !chan::mode::OEXCL)?;
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
                    let r = p.fds.as_ref().ok_or(EBADFD)?.borrow_mut().close(fd);
                    r.ok_or(EBADFD)?
                };
                self.closethen(up, last.into_iter().collect(), Box::new(|_, _| Ok(Ret::Ok)))
            }
            // *"c = fdtochan(fd, OREAD, 1, 1)"* (`sysfile.c:637`) and
            // *"fdtochan(fd, OWRITE, 1, 1)"* (`:728`): the channel must be
            // open for what is asked of it, and not a mount's.
            Call::Pread { fd, n, off } => {
                let cell = self.fdtochan(up, fd, Some(chan::mode::OREAD), true)?;
                self.pread(up, cell, n, off)
            }
            Call::Pwrite { fd, data, off } => {
                let cell = self.fdtochan(up, fd, Some(chan::mode::OWRITE), true)?;
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
                // *"c = fdtochan(arg[1], -1, 1, 1)"* (`sysfile.c:805`).
                let cell = self.fdtochan(up, fd, None, true)?;
                // *"if(devtab[c->type]->dc == '|') error(Eisstream)"*
                // (`sysfile.c:810`).
                if cell.borrow().dev == dev::DevId::Pipe {
                    return Err("seek on a stream".into());
                }
                let is_dir = cell.borrow().is_dir();
                let new: i64 = match whence {
                    0 => {
                        if is_dir && off != 0 {
                            return Err(EISDIR.into());
                        }
                        off
                    }
                    1 => {
                        if is_dir {
                            return Err(EISDIR.into());
                        }
                        cell.borrow().offset as i64 + off
                    }
                    // *"n = devtab[c->type]->stat(c, buf, sizeof buf); …
                    // off = dir.length + o.v"* (`:839`) — the end is the
                    // length the file's server gives it.
                    2 => {
                        if is_dir {
                            return Err(EISDIR.into());
                        }
                        let c = cell.borrow().clone();
                        let b = self.tab.dstat(&c)?;
                        let d = ninep::Dir::conv_m2d(&b).ok_or("internal error: stat error in seek")?;
                        d.length as i64 + off
                    }
                    _ => return Err(proc::Procs::EBADARG.into()),
                };
                if new < 0 {
                    return Err(ENEGOFF.into());
                }
                let new = new as u64;
                let umc = {
                    let mut c = cell.borrow_mut();
                    c.offset = new;
                    c.dri = 0;
                    // `unionrewind` (`sysfile.c:367`) — a rewind starts the
                    // union over too, or the next read carries on from the
                    // element the last one stopped in.
                    c.uri = 0;
                    c.umc.take()
                };
                self.closethen(up, umc.into_iter().map(|b| *b).collect(), Box::new(move |_, _| Ok(Ret::N(new as usize))))
            }
            Call::Dup { old, new } => {
                let fds = self.fgrp(up)?;
                let (r, oc) = fds.borrow_mut().dup(old, new).ok_or(EBADFD)?;
                self.closethen(up, oc.into_iter().collect(), Box::new(move |_, _| Ok(Ret::Fd(r))))
            }
            // `syspipe` (`sysfile.c`): attach `#|`, walk the two ends, open
            // both. The attach IS the allocation.
            Call::Pipe => {
                let dir = self.tab.get(dev::DevId::Pipe).ok_or(ENODEV)?.attach("")?;
                // *"if(waserror()){ cclose(c[0]); if(c[1]) cclose(c[1]);
                // …"* (`sysfile.c:201`): an end opened is closed if the
                // other cannot be.
                let mut ends = Vec::new();
                for name in ["data", "data1"] {
                    let opened = self
                        .tab
                        .get(dev::DevId::Pipe)
                        .ok_or(ENODEV.to_string())
                        .and_then(|d| d.walk(&dir, name)?.ok_or_else(|| "no such file".to_string()))
                        .and_then(|c| self.tab.dopen(c, chan::mode::ORDWR));
                    match opened {
                        Ok(c) => ends.push(c),
                        Err(e) => {
                            for c in ends {
                                self.tab.cclose(c);
                            }
                            return Err(e);
                        }
                    }
                }
                // *"if(newfd2(fd, c) < 0) error(Enofd)"* (`sysfile.c:215`):
                // `data` in the lowest free slot and `data1` in the next, or
                // neither — and both closed.
                let fds = self.fgrp(up)?;
                let data1 = ends.pop().unwrap();
                let data = ends.pop().unwrap();
                let r = fds.borrow_mut().newfd2([data, data1]);
                match r {
                    Ok([a, b]) => Ok(Ret::Two(a, b)),
                    Err([data, data1]) => {
                        self.tab.cclose(data);
                        self.tab.cclose(data1);
                        Err(ENOFD.into())
                    }
                }
            }
            // `sysremove` (`sysfile.c:1141`): *"Remove clunks the fid"*, so
            // the channel is not closed after.
            //
            // *"Removing mount points is disallowed to avoid surprises"*
            // (`:1148`): *"if(c->ismtpt){ cclose(c); error(Eismtpt); }"*.
            Call::Remove { path } => {
                let (mut c, _) = self.walk(up, &path, namec::A::Remove, 0)?;
                if c.ismtpt {
                    return self.closethen(up, vec![c], Box::new(|_, _| Err(namec::EISMTPT.into())));
                }
                self.tab.dremove(&mut c)?;
                Ok(Ret::Ok)
            }
            // `sysstat` (`sysfile.c:951`): the device's stat, then *"name =
            // pathlast(c->path); if(name) l = dirsetname(…)"* — a file is
            // named as it was reached, not as its server calls it. `fstat`
            // does not (`:932`).
            Call::Stat { path } => {
                let (c, src) = self.walk(up, &path, namec::A::Access, 0)?;
                let d = self.done(c.clone(), src.is_none(), |k, c| k.tab.dstat(c))?;
                Ok(Ret::Data(match pathlast(&c.path) {
                    Some(name) => dirsetname(name, d),
                    None => d,
                }))
            }
            // `sys_stat` and `sys_fstat` (`sysfile.c:1258`, `:1292`): into
            // *"uchar buf[128]; /* old DIRLEN plus a little should be
            // plenty */"*, named by `pathlast` — both of them — and laid
            // down by `packoldstat`. What does not fit is *"old stat system
            // call - recompile"*.
            Call::OldStat { path } => {
                let (c, src) = self.walk(up, &path, namec::A::Access, 0)?;
                let d = self.done(c.clone(), src.is_none(), |k, c| k.tab.dstat(c))?;
                oldstat(&c, d, "old stat system call - recompile")
            }
            Call::OldFstat { fd } => {
                let c = self.chan(up, fd)?;
                let d = self.tab.dstat(&c)?;
                oldstat(&c, d, "old fstat system call - recompile")
            }
            Call::Fstat { fd } => {
                let c = self.chan(up, fd)?;
                Ok(Ret::Data(self.tab.dstat(&c)?))
            }
            // `wstat` (`sysfile.c:1172`), for both: *"Renaming mount points
            // is disallowed to avoid surprises"* — a new name for a file on
            // a mount point is `Eismtpt`, with its path (`:1181`).
            Call::Wstat { path, edir } => {
                let (c, src) = self.walk(up, &path, namec::A::Access, 0)?;
                self.done(c, src.is_none(), |k, c| {
                    renamesmtpt(c, &edir)?;
                    k.tab.dwstat(c, &edir)
                })?;
                Ok(Ret::Ok)
            }
            Call::Fwstat { fd, edir } => {
                // *"c = fdtochan(arg[0], -1, 1, 1)"* (`sysfile.c:1219`).
                let mut c = self.fdtochan(up, fd, None, true)?.borrow().clone();
                renamesmtpt(&c, &edir)?;
                self.tab.dwstat(&mut c, &edir)?;
                Ok(Ret::Ok)
            }

            // ---- not yet
            // `sysmount` (`sysfile.c`): the fd is a channel to a SERVER, and
            // `#M` speaks 9P down it. Every other device presents files as
            // function calls; this is the one crossing.
            // `bindmount` (`sysfile.c:989`): the server's channel, and the
            // authentication file's if there is one — its fid is the
            // attach's `afid` (`devmnt.c:344`) — then `cmount`, and
            // *"fdclose(fd, 0)"*: the mount holds the channel now, not the
            // descriptor. It answers the new mount's id (`chan.c:760`).
            //
            // Both descriptors must be open `ORDWR` (`sysfile.c:1015`,
            // `:1024`), and the server's is marked a mount's message
            // channel — *"c->flag |= CMSG"*, at the end of `mntversion`
            // (`devmnt.c:239`) — in the copy the mount keeps and in the
            // descriptor, which are one channel in Plan 9.
            Call::Mount { fd, afd, old, flag, aname } => {
                bindflag(flag)?;
                // *"spec = validnamedup(spec, 1)"* (`sysfile.c:1005`)
                namec::validname(&aname, true)?;
                // *"if(up->pgrp->noattach) error(Enoattach)"* (`:1011`)
                if self.ns(up)?.borrow().noattach() {
                    return Err(namec::ENOATTACH.into());
                }
                let cell = self.fdtochan(up, fd, Some(chan::mode::ORDWR), false)?;
                let mut wire = cell.borrow().clone();
                wire.flag |= chan::flag::CMSG;
                let afid = if afd >= 0 {
                    self.fdtochan(up, afd, Some(chan::mode::ORDWR), false)?.borrow().fid
                } else {
                    ninep::NOFID
                };
                // *"c0 = devtab[ret]->attach((char*)&bogus)"* (`:1031`)
                let user = self.procs.borrow().user(up).unwrap_or_default();
                let c0 = std::rc::Rc::new(self.tab.dmount(wire, &user, &aname, afid)?);
                cell.borrow_mut().flag |= chan::flag::CMSG;
                self.tab.keepwire(cell);
                // *"c1 = namec(arg1, Amount, 0, 0)"* (`:1048`)
                let c1 = match self.walk(up, &old, namec::A::Mount, 0) {
                    Ok((c1, src1)) => src1.unwrap_or_else(|| std::rc::Rc::new(c1)),
                    Err(e) => return self.unwind(up, e, vec![c0]),
                };
                let r = self.ns(up)?.borrow_mut().cmount(c1.clone(), ns::Element::shared(c0.clone(), flag, &aname), Vec::new(), bind_of(flag));
                self.mounted(up, r, c1, c0, Some(fd))
            }
            // `sysfversion` (`auth.c:23`): `mntversion` on the descriptor,
            // answering the version agreed, which the machine copies into
            // the caller's buffer, and its length.
            //
            // *"c = fdtochan(arg[0], ORDWR, 0, 1)"* (`auth.c:36`), and the
            // channel is a mount's from here (`devmnt.c:239`). The session
            // is the wire's, and holds no reference to it: *"Mnts have no
            // reference count; they go away when c goes away"*
            // (`devmnt.c:12`).
            Call::Fversion { fd, msize, version } => {
                let cell = self.fdtochan(up, fd, Some(chan::mode::ORDWR), false)?;
                let mut wire = cell.borrow().clone();
                wire.flag |= chan::flag::CMSG;
                let v = self.tab.dfversion(wire, msize, &version)?;
                cell.borrow_mut().flag |= chan::flag::CMSG;
                Ok(Ret::Str(v))
            }
            // `sysfauth` (`auth.c:62`): `mntauth`, a descriptor on the new
            // channel — *"ac is responsible for keeping c alive"* — and
            // *"always mark it close on exec"*.
            //
            // *"c = fdtochan(arg[0], ORDWR, 0, 1)"* (`auth.c:74`); `mntauth`
            // versions the channel first if it has not been (`devmnt.c:259`).
            Call::Fauth { fd, aname } => {
                let cell = self.fdtochan(up, fd, Some(chan::mode::ORDWR), false)?;
                let mut wire = cell.borrow().clone();
                wire.flag |= chan::flag::CMSG;
                let user = self.procs.borrow().user(up).unwrap_or_default();
                let mut ac = self.tab.dauth(wire, &user, &aname)?;
                cell.borrow_mut().flag |= chan::flag::CMSG;
                self.tab.keepwire(cell);
                ac.flag |= chan::flag::CCEXEC;
                Ok(Ret::Fd(self.newfd(up, std::rc::Rc::new(std::cell::RefCell::new(ac)))?))
            }
            // `sysfd2path` (`sysfile.c:173`): *"snprint((char*)arg[1],
            // arg[2], "%s", chanpath(c))"* — the machine writes it into the
            // caller's buffer, as it does `errstr`'s.
            Call::Fd2path { fd } => {
                let c = self.chan(up, fd)?;
                Ok(Ret::Str(c.path.clone()))
            }

            // `syssleep` (`sysproc.c`), and both of its branches are here:
            //
            // ```c
            // n = arg[0];
            // if(n <= 0) { ... yield(); return 0; }
            // if(n < TK2MS(1)) n = TK2MS(1);
            // tsleep(&up->sleep, return0, 0, n);
            // ```
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
                    let _ = self.pexit(up, "Suicide", false);
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
                        let _ = self.pexit(up, &last.msg, flag != proc::NoteFlag::NDebug);
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
                if self.machine.load(up, addr)? < 0 {
                    return Err(proc::Procs::EBADARG.into());
                }
                self.semacquire(up, addr, if block { None } else { Some(0) })
            }
            // `systsemacquire` (`sysproc.c:1206`).
            Call::Tsemacquire { addr, ms } => {
                if self.machine.load(up, addr)? < 0 {
                    return Err(proc::Procs::EBADARG.into());
                }
                self.semacquire(up, addr, Some(ms))
            }
            // `syssemrelease` (`sysproc.c:1225`): *"delta == 0 is a no-op,
            // not a release"*.
            Call::Semrelease { addr, delta } => {
                if delta < 0 || self.machine.load(up, addr)? < 0 {
                    return Err(proc::Procs::EBADARG.into());
                }
                self.semrelease(up, addr, delta).map(|v| Ret::N(v as u32 as usize))
            }
        }
    }

    // ---- syscall tracing (`port/syscallfmt.c`) -----------------------------

    /// `okaddr` (`fault.c:291`): whether `len` bytes from `addr` are all in
    /// the process's memory. *"(long)len >= 0 && addr+len >= addr"*, then the
    /// segment it is in — here the one memory, whose first and last words
    /// the machine can load or not.
    fn okaddr(&self, up: Pid, addr: u32, len: u32) -> bool {
        if (len as i32) < 0 {
            return false;
        }
        let Some(last) = addr.checked_add(len.max(1) - 1) else { return false };
        self.machine.load(up, addr & !3).is_ok() && self.machine.load(up, last & !3).is_ok()
    }

    /// `validaddr` (`fault.c:310`): `okaddr`, which says *"suicide: invalid
    /// address %#lux/%lud in sys call pc=%#lux"* on the process's console
    /// when it is not, then *"sys: bad address in syscall"*, `NDebug`, and
    /// `Ebadarg` — so the call fails and the process dies of the note on
    /// its way out of it. `pc` is `userpc()`.
    ///
    /// The address is reported as given: `okaddr` reports where it stopped
    /// walking segments, and there is one segment here.
    fn validaddr(&mut self, up: Pid, addr: u32, len: u32, pc: &dyn Fn() -> u64) -> Result<(), String> {
        if !self.okaddr(up, addr, len) {
            self.pprint(up, &format!("suicide: invalid address {addr:#x}/{len} in sys call pc={:#x}\n", pc()));
            self.procs.borrow_mut().postnote(up, "sys: bad address in syscall", proc::NoteFlag::NDebug);
            return Err(proc::Procs::EBADARG.into());
        }
        Ok(())
    }

    /// `validname` of a user pointer (`chan.c:1703`): the name's bytes up to
    /// its NUL, looked for with `vmemchr`, which `validaddr`s each page it
    /// reaches (`fault.c:322`) — so a name that runs off the process's
    /// memory is a bad address where it runs off. *"name too long"* at
    /// `1<<16` without a NUL.
    fn validname(&mut self, up: Pid, a: u32, pc: &dyn Fn() -> u64) -> Result<(), String> {
        self.validaddr(up, a, 1, pc)?;
        for i in 0..(1u32 << 16) {
            let at = a.wrapping_add(i);
            match self.userbyte(up, at) {
                Ok(0) => return Ok(()),
                Ok(_) => {}
                Err(_) => return self.validaddr(up, at, 1, pc),
            }
        }
        Err("name too long".into())
    }

    /// **The addresses a call was given, checked as the call checks them**
    /// — each of Plan 9's calls does its own at its top, with `validaddr`
    /// on each pointer and `validname` on each name as `namec` reaches it.
    /// They are gathered here because here is where the words are
    /// (`up->s`); the checks and their order are each call's:
    ///
    /// | call | checks |
    /// |---|---|
    /// | `open` `create` `remove` `chdir` | the name (`sysfile.c:271`, `:1127`, `:1145`, `:980`), then `namec`'s `validnamedup` (`chan.c:1330`) |
    /// | `stat` / `wstat` | the buffer, then the name (`:958`–`:959`, `:1203`–`:1205`) |
    /// | `fstat` / `fwstat` | the buffer (`:938`, `:1217`) |
    /// | `pread` / `pwrite` | the buffer, `n` long (`:635`, `:726`) |
    /// | `bind` | the flag (`:1000`), then both names (`:1038`, `:1047`) |
    /// | `mount` | the flag, the spec, then the name mounted on (`:1000`, `:1004`–`:1005`, `:1047`) |
    /// | `unmount` | the name, then the mounted one if not nil (`:1093`, `:1109`) |
    /// | `pipe` | two `int`s, aligned (`:193`–`:194`) |
    /// | `exec` | the name (`sysproc.c:286`–`:287`), then `argv`: aligned, each pointer, each string (`:401`–`:408`) |
    /// | `exits` | the status — which, bad, is *"invalid exit string"* and not an error (`:671`–`:675`) |
    /// | `await` | the buffer (`:723`) |
    /// | `errstr` | `nbuf` not 0, then the buffer (`:754`–`:755`) |
    /// | `fversion` | the version buffer, holding a NUL (`auth.c:32`–`:35`) |
    /// | `semacquire` `tsemacquire` `semrelease` | the `long`, aligned (`sysproc.c:1193`, `:1212`, `:1230`) |
    ///
    /// `exec` checks its `argv` after reading the image's header, so a
    /// missing file with a bad `argv` answers the bad address here where
    /// Plan 9 would say the file does not exist.
    ///
    /// **`notify`'s argument is not checked** (*"if(arg[0] != 0)
    /// validaddr(arg[0], sizeof(ulong), 0)"*, `sysproc.c:784`): on the PC
    /// it is the handler's address in the process's memory, and on this
    /// machine a function is an index into the module's table, not an
    /// address at all.
    ///
    /// **Address 0 is in the process's memory here**, where Plan 9 leaves
    /// the page at 0 unmapped, so a nil pointer is not a bad address unless
    /// what it points at is.
    fn validargs(&mut self, up: Pid, call: &Call, pc: &dyn Fn() -> u64) -> Result<(), String> {
        let sa = self.procs.borrow().get(up).ok_or("no such process")?.s;
        let a = |i: usize| sa[i] as u32;
        match call {
            Call::Open { .. } | Call::Create { .. } | Call::Remove { .. } | Call::Chdir { .. } => {
                self.validname(up, a(0), pc)
            }
            Call::Stat { .. } | Call::Wstat { .. } => {
                self.validaddr(up, a(1), a(2), pc)?;
                self.validname(up, a(0), pc)
            }
            Call::Fstat { .. } | Call::Fwstat { .. } => self.validaddr(up, a(1), a(2), pc),
            // `sys_stat`: *"validaddr(arg[1], 116, 1); validaddr(arg[0], 1, 0)"*
            Call::OldStat { .. } => {
                self.validaddr(up, a(1), 116, pc)?;
                self.validname(up, a(0), pc)
            }
            Call::OldFstat { .. } => self.validaddr(up, a(1), 116, pc),
            Call::Pread { .. } | Call::Pwrite { .. } => self.validaddr(up, a(1), a(2), pc),
            // `bindmount` checks the flag before any address (`sysfile.c:1000`)
            Call::Bind { flag, .. } => {
                bindflag(*flag)?;
                self.validname(up, a(0), pc)?;
                self.validname(up, a(1), pc)
            }
            Call::Mount { flag, .. } => {
                bindflag(*flag)?;
                self.validname(up, a(4), pc)?;
                self.validname(up, a(2), pc)
            }
            // `sysfauth`: *"validaddr(arg[1], 1, 0); aname =
            // validnamedup((char*)arg[1], 1)"* (`auth.c:68`)
            Call::Fauth { .. } => self.validname(up, a(1), pc),
            Call::Unmount { .. } => {
                self.validname(up, a(1), pc)?;
                if a(0) != 0 {
                    self.validname(up, a(0), pc)?;
                }
                Ok(())
            }
            Call::Pipe => {
                self.validaddr(up, a(0), 8, pc)?;
                self.validalign(up, a(0), 4)
            }
            Call::Exec { .. } => {
                self.validname(up, a(0), pc)?;
                let mut argp = a(1);
                self.validalign(up, argp, 4)?;
                loop {
                    self.validaddr(up, argp, 4, pc)?;
                    let s = self.machine.load(up, argp)? as u32;
                    if s == 0 {
                        return Ok(());
                    }
                    self.validname(up, s, pc)?;
                    argp = argp.wrapping_add(4);
                }
            }
            Call::Await => self.validaddr(up, a(0), a(1), pc),
            // *"validaddr(arg[1], arg[2], 1)"* (`sysfile.c:177`)
            Call::Fd2path { .. } => self.validaddr(up, a(1), a(2), pc),
            Call::Errstr { .. } => {
                if a(1) == 0 {
                    return Err(proc::Procs::EBADARG.into());
                }
                self.validaddr(up, a(0), a(1), pc)
            }
            Call::Fversion { .. } => {
                self.validaddr(up, a(2), a(3), pc)?;
                let has_nul = (0..a(3)).any(|i| self.userbyte(up, a(2).wrapping_add(i)) == Ok(0));
                if !has_nul {
                    return Err(proc::Procs::EBADARG.into());
                }
                Ok(())
            }
            Call::Semacquire { .. } | Call::Tsemacquire { .. } | Call::Semrelease { .. } => {
                self.validlong(up, a(0), pc)
            }
            _ => Ok(()),
        }
    }

    /// `validalign` (`pc/trap.c:964`): *"sys: odd address"* and `Ebadarg`.
    fn validalign(&mut self, up: Pid, addr: u32, align: u32) -> Result<(), String> {
        if addr & (align - 1) != 0 {
            self.procs.borrow_mut().postnote(up, "sys: odd address", proc::NoteFlag::NDebug);
            return Err(proc::Procs::EBADARG.into());
        }
        Ok(())
    }

    /// The byte at `addr` in the process's memory, by the word it is in —
    /// the only way the machine reaches that memory (`Machine::load`).
    fn userbyte(&self, up: Pid, addr: u32) -> Result<u8, String> {
        let w = self.machine.load(up, addr & !3)?;
        Ok(w.to_le_bytes()[(addr & 3) as usize])
    }

    /// `fmtuserstring` (`syscallfmt.c:37`): *"%#p/\"%s\"%s"*, or *"0/\"\""*
    /// for nil — the string read out of the process up to its NUL
    /// (`vmemchr`).
    fn fmtuserstring(&mut self, up: Pid, f: &mut String, a: u32, suffix: &str, pc: &dyn Fn() -> u64) -> Result<(), String> {
        if a == 0 {
            f.push_str(&format!("0/\"\"{suffix}"));
            return Ok(());
        }
        self.validaddr(up, a, 1, pc)?;
        let mut t = Vec::new();
        let mut at = a;
        loop {
            let b = match self.userbyte(up, at) {
                Ok(b) => b,
                Err(_) => return self.validaddr(up, at, 1, pc),
            };
            if b == 0 {
                break;
            }
            t.push(b);
            at = at.wrapping_add(1);
        }
        f.push_str(&format!("{a:#x}/\"{}\"{suffix}", String::from_utf8_lossy(&t)));
        Ok(())
    }

    /// `fmtrwdata` (`syscallfmt.c:13`): *" %#p/\"%s\"%s"* with anything
    /// that is not printable ASCII as a dot, or *"0x0"* for nil. The bytes
    /// are given: a write's are the call's own, and a read's are what it
    /// answered, which the machine has not yet written into the process
    /// when the trace is made.
    fn fmtrwdata(f: &mut String, a: u32, data: &[u8], suffix: &str) {
        if a == 0 {
            f.push_str(&format!("0x0{suffix}"));
            return;
        }
        let t: String = data.iter().map(|&c| if (0x20..0x7f).contains(&c) { c as char } else { '.' }).collect();
        f.push_str(&format!(" {a:#x}/\"{t}\"{suffix}"));
    }

    /// `syscallfmt` (`syscallfmt.c:55`) — the line a tracer reads from
    /// `/proc/n/syscall` while the process is stopped on its way into a call:
    /// pid, text, the call's name, the pc, and its arguments as the process
    /// passed them, strings read out of its memory.
    fn syscallfmt(&mut self, up: Pid, call: &Call, pc: &dyn Fn() -> u64) -> Result<String, String> {
        let (text, sa) = {
            let procs = self.procs.borrow();
            let p = procs.get(up).ok_or("no such process")?;
            (p.text.clone(), p.s)
        };
        let a = |i: usize| sa[i] as u32;
        let d = |i: usize| sa[i] as u32 as i32;
        let mut f = format!("{up} {text} {} {:x} ", sysctab(call), pc());
        match call {
            // the name and the pc: this kernel does not have the call, and
            // what its arguments mean is the call's
            Call::Bad { .. } | Call::Sysr1 => {}
            Call::Chdir { .. } | Call::Exits { .. } | Call::Remove { .. } => {
                self.fmtuserstring(up, &mut f, a(0), "", pc)?;
            }
            Call::Bind { .. } => {
                self.fmtuserstring(up, &mut f, a(0), " ", pc)?;
                self.fmtuserstring(up, &mut f, a(1), " ", pc)?;
                f.push_str(&format!("{:#x}", a(2)));
            }
            Call::Close { .. } | Call::Noted { .. } => f.push_str(&format!("{}", d(0))),
            Call::Dup { .. } => f.push_str(&format!("{} {}", d(0), d(1))),
            Call::Alarm { .. } => f.push_str(&format!("{} ", a(0))),
            Call::Exec { .. } => {
                self.fmtuserstring(up, &mut f, a(0), "", pc)?;
                let mut argv = a(1);
                self.validalign(up, argv, 4)?;
                loop {
                    self.validaddr(up, argv, 4, pc)?;
                    let s = self.machine.load(up, argv)? as u32;
                    if s == 0 {
                        break;
                    }
                    f.push(' ');
                    self.fmtuserstring(up, &mut f, s, "", pc)?;
                    argv = argv.wrapping_add(4);
                }
            }
            Call::Rendezvous { .. } => f.push_str(&format!("{:#x} {:#x}", a(0), a(1))),
            Call::Open { .. } => {
                self.fmtuserstring(up, &mut f, a(0), " ", pc)?;
                f.push_str(&format!("{:#x}", a(1)));
            }
            Call::Sleep { .. } => f.push_str(&format!("{}", d(0))),
            Call::Rfork { .. } => f.push_str(&format!("{:#x}", a(0))),
            Call::Pipe | Call::Notify { .. } => f.push_str(&format!("{:#x}", a(0))),
            Call::Create { .. } => {
                self.fmtuserstring(up, &mut f, a(0), " ", pc)?;
                f.push_str(&format!("{:#x} {:#x}", a(1), a(2)));
            }
            Call::Fstat { .. } | Call::Fwstat { .. } => {
                f.push_str(&format!("{} {:#x} {}", d(0), a(1), a(2)));
            }
            Call::Unmount { .. } => {
                self.fmtuserstring(up, &mut f, a(0), " ", pc)?;
                self.fmtuserstring(up, &mut f, a(1), "", pc)?;
            }
            Call::Semacquire { .. } | Call::Semrelease { .. } | Call::Tsemacquire { .. } => {
                f.push_str(&format!("{:#x} {}", a(0), d(1)));
            }
            // *"%#p %d %#llux %d"* — the first is where the PC's stub has the
            // kernel write the new offset. This machine answers it instead,
            // so there is no such address, and it is nil.
            Call::Seek { .. } => {
                f.push_str(&format!("{:#x} {} {:#x} {}", 0, d(0), sa[1], d(2)));
            }
            Call::Fversion { .. } => {
                f.push_str(&format!("{} {} ", d(0), d(1)));
                self.fmtuserstring(up, &mut f, a(2), " ", pc)?;
                f.push_str(&format!("{}", a(3)));
            }
            Call::Wstat { .. } | Call::Stat { .. } => {
                self.fmtuserstring(up, &mut f, a(0), " ", pc)?;
                f.push_str(&format!("{:#x} {}", a(1), a(2)));
            }
            // `syscallfmt.c:186`, `:141`
            Call::OldStat { .. } => {
                self.fmtuserstring(up, &mut f, a(0), " ", pc)?;
                f.push_str(&format!("{:#x}", a(1)));
            }
            Call::OldFstat { .. } => f.push_str(&format!("{} {:#x}", d(0), a(1))),
            Call::Errstr { .. } | Call::Await => f.push_str(&format!("{:#x} {}", a(0), a(1))),
            Call::Fd2path { .. } => f.push_str(&format!("{} {:#x} {}", d(0), a(1), a(2))),
            // `syscallfmt.c:147`
            Call::Fauth { .. } => {
                f.push_str(&format!("{}", d(0)));
                self.fmtuserstring(up, &mut f, a(1), "", pc)?;
            }
            Call::Mount { .. } => {
                f.push_str(&format!("{} {} ", d(0), d(1)));
                self.fmtuserstring(up, &mut f, a(2), " ", pc)?;
                f.push_str(&format!("{:#x} ", a(3)));
                self.fmtuserstring(up, &mut f, a(4), "", pc)?;
            }
            Call::Pread { .. } => {
                f.push_str(&format!("{} {:#x} {} {}", d(0), a(1), d(2), sa[3] as i64));
            }
            Call::Pwrite { data, .. } => {
                f.push_str(&format!("{} ", d(0)));
                Self::fmtrwdata(&mut f, a(1), &data[..data.len().min(64)], " ");
                f.push_str(&format!("{} {}", d(2), sa[3] as i64));
            }
        }
        Ok(f)
    }

    /// `sysretfmt` (`syscallfmt.c:338`) — the line for the way out: what the
    /// call answered, its error if it failed, and when it began and ended.
    fn sysretfmt(&mut self, up: Pid, r: &Result<Ret, String>, start: u64, stop: u64) -> String {
        let (nr, sa, syserrstr) = {
            let procs = self.procs.borrow();
            let Some(p) = procs.get(up) else { return String::new() };
            (p.scallnr, p.s, p.errstr.clone())
        };
        let a = |i: usize| sa[i] as u32;
        let e = r.as_ref().err().cloned().unwrap_or_default();
        let ret = retval(nr, &sa, r);
        let mut errstr = "\"\"".to_string();
        let mut f = String::new();
        match nr {
            sysno::EXEC | sysno::RENDEZVOUS => {
                if ret as u32 == u32::MAX {
                    errstr = e;
                }
                f.push_str(&format!(" = {:#x}", ret as u32));
            }
            sysno::AWAIT => {
                let (l, msg) = (a(1), match r {
                    Ok(Ret::Str(m)) => m.clone(),
                    _ => String::new(),
                });
                if ret > 0 {
                    let b = &msg.as_bytes()[..ret as usize];
                    f.push_str(&format!("{:#x}/\"{}\" ", a(0), String::from_utf8_lossy(b)));
                    f.push_str(&format!("{l} = {ret}"));
                } else {
                    f.push_str(&format!("{:#x}/\"\" {l} = {ret}", a(0)));
                    errstr = e;
                }
            }
            // `generrstr` answers 0, so this is always the second branch,
            // and *"errstr = up->syserrstr"* is what the exchange left.
            sysno::ERRSTR => {
                f.push_str(&format!("\"\" {} = {ret}", a(1)));
                errstr = syserrstr;
            }
            sysno::PREAD => {
                match r {
                    Ok(Ret::Data(b)) if ret > 0 => Self::fmtrwdata(&mut f, a(1), &b[..b.len().min(64)], ""),
                    _ => {
                        f.push_str("/\"\"");
                        errstr = e;
                    }
                }
                f.push_str(&format!(" {} {} = {ret}", sa[2] as u32 as i32, sa[3] as i64));
            }
            _ => {
                if ret == -1 {
                    errstr = e;
                }
                f.push_str(&format!(" = {ret}"));
            }
        }
        f.push_str(&format!(" {errstr} {start} {stop}\n"));
        f
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
    fn validlong(&mut self, up: Pid, addr: u32, pc: &dyn Fn() -> u64) -> Result<(), String> {
        self.validaddr(up, addr, 4, pc)?;
        self.validalign(up, addr, 4)
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

    /// The calling process's `slash`, `dot` and namespace, which every name
    /// it gives is resolved through.
    fn names(&self, up: Pid) -> Result<(std::rc::Rc<Chan>, std::rc::Rc<Chan>, std::rc::Rc<std::cell::RefCell<ns::Ns>>), String> {
        let procs = self.procs.borrow();
        let p = procs.get(up).ok_or("no such process")?;
        match (&p.dot, &p.ns) {
            (Some(dot), Some(ns)) => Ok((p.slash.clone(), dot.clone(), ns.clone())),
            _ => Err("no such process".into()),
        }
    }

    /// `up->pgrp`.
    fn ns(&self, up: Pid) -> Result<std::rc::Rc<std::cell::RefCell<ns::Ns>>, String> {
        self.procs.borrow().get(up).and_then(|p| p.ns.clone()).ok_or_else(|| "no such process".into())
    }

    /// `up->fgrp`.
    fn fgrp(&self, up: Pid) -> Result<std::rc::Rc<std::cell::RefCell<proc::Fds>>, String> {
        self.procs.borrow().get(up).and_then(|p| p.fds.clone()).ok_or_else(|| "no such process".into())
    }

    /// `namec` for the calling process: its namespace, its `slash`, its `dot`
    /// — and, with the channel, the reference it is when it is not the
    /// call's own ([`namec::namec`]).
    fn walk(&mut self, up: Pid, path: &str, a: namec::A, mode: u16) -> Result<(Chan, Option<std::rc::Rc<Chan>>), String> {
        // The process table is let go before the walk: a walk through a
        // server a process runs sleeps, and sleeping is the table's.
        let (slash, dot, ns) = self.names(up)?;
        let ns = ns.borrow();
        namec::namec(&mut self.tab, &ns, &slash, &dot, path, a, mode)
    }

    /// `namec(name, Aopen, …)` for the calling process — a reference.
    fn walk_open(&mut self, up: Pid, path: &str, mode: u16) -> Result<std::rc::Rc<std::cell::RefCell<Chan>>, String> {
        let (slash, dot, ns) = self.names(up)?;
        let ns = ns.borrow();
        namec::open(&mut self.tab, &ns, &slash, &dot, path, mode)
    }

    fn walk_create(&mut self, up: Pid, path: &str, mode: u16, perm: u32) -> Result<std::rc::Rc<std::cell::RefCell<Chan>>, String> {
        let (slash, dot, ns) = self.names(up)?;
        let ns = ns.borrow();
        namec::create(&mut self.tab, &ns, &slash, &dot, path, mode, perm)
    }

    /// A call's error with channels of its own to close first — its
    /// *"if(waserror()){ cclose(c); nexterror(); }"* — unless the call has
    /// left the processor: it runs again from the top, and its record makes
    /// the same channels again.
    fn unwind(&mut self, up: Pid, e: String, refs: Vec<std::rc::Rc<Chan>>) -> Result<Ret, String> {
        if e == devmnt::SLEPT {
            return Err(e);
        }
        self.closethen(up, chan::lastrefs(refs), Box::new(move |_, _| Err(e)))
    }

    /// The end of `bindmount` (`sysfile.c:1054`), once `cmount` has
    /// answered: *"cclose(c1); … cclose(c0)"*, after what an `MREPL`
    /// replaced, then for a mount *"fdclose(fd, 0)"* — the mount holds the
    /// server's channel now, not the descriptor. The mount's id, or
    /// `cmount`'s error.
    fn mounted(
        &mut self,
        up: Pid,
        r: Result<(u32, Vec<std::rc::Rc<Chan>>), String>,
        c1: std::rc::Rc<Chan>,
        c0: std::rc::Rc<Chan>,
        fd: Option<Fd>,
    ) -> Result<Ret, String> {
        let (r, mut refs) = match r {
            Ok((id, gone)) => (Ok(Ret::N(id as usize)), gone),
            Err(e) => (Err(e), Vec::new()),
        };
        refs.push(c1);
        refs.push(c0);
        let mut last = chan::lastrefs(refs);
        if let (Ok(_), Some(fd)) = (&r, fd) {
            last.extend(self.fgrp(up)?.borrow_mut().close(fd).flatten());
        }
        self.closethen(up, last, Box::new(move |_, _| r))
    }

    /// `newfd(c)` (`sysfile.c:74`): the reference goes into the lowest free
    /// slot — and with none, *"if(fd < 0) error(Enofd)"*, the caller's
    /// `waserror` closing it (`sysopen`, `sysfile.c:1135`).
    fn newfd(&mut self, up: Pid, c: std::rc::Rc<std::cell::RefCell<Chan>>) -> Result<Fd, String> {
        let fds = self.fgrp(up)?;
        let r = fds.borrow_mut().newfd(c);
        match r {
            Ok(fd) => Ok(fd),
            Err(c) => {
                self.tab.cclose(c);
                Err(ENOFD.into())
            }
        }
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
        let Some(m) = c.umh.clone() else { return Ok(Vec::new()) };
        // *"bring mount in sync with c->uri and c->umc"* (`sysfile.c:334`):
        // the head's list as it is now, not as it was at the open.
        while let Some(to) = m.borrow().mount.get(c.uri as usize).map(|e| e.chan.clone()) {
            // *"Error causes component of union to be skipped"*
            // (`sysfile.c:340`) — unless the error is the process leaving
            // the processor, when the read is made again.
            if c.umc.is_none() {
                let cl = match self.tab.dcclone(&to) {
                    Ok(cl) => cl,
                    Err(e) if e == devmnt::SLEPT => return Err(e),
                    Err(_) => {
                        c.uri += 1;
                        continue;
                    }
                };
                // A union's elements are directories (`bind` refuses a file
                // on a directory, `Emount`), and only `#d`'s and `#s`'s
                // files open as a channel that already exists — so this one
                // is the element's own.
                //
                // An open that fails leaves the clone to be closed, as
                // *"if(c->umc){ cclose(c->umc); …"* (`:356`) closes it.
                let keep = cl.clone();
                match self.tab.dopen(cl, chan::mode::OREAD).map(std::rc::Rc::try_unwrap) {
                    Ok(Ok(o)) => c.umc = Some(Box::new(o.into_inner())),
                    Err(e) if e == devmnt::SLEPT => return Err(e),
                    Ok(Err(_)) | Err(_) => {
                        let mut keep = keep;
                        self.tab.dclose(&mut keep);
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
                Err(e) if e == devmnt::SLEPT => {
                    c.umc = Some(umc);
                    return Err(e);
                }
                // *"Advance to next element"* (`:354`), closing this one
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
        let fds = p.fds.clone().ok_or(EBADFD)?;
        let cell = fds.borrow().get(fd).cloned().ok_or(EBADFD)?;
        Ok(cell)
    }

    /// `fdtochan(fd, mode, chkmnt, iref)` (`sysfile.c:120`): the channel a
    /// descriptor holds, refused — `Ebadusefd` — if it is not open in the
    /// mode the call needs or, with `chkmnt`, if it is a mount's message
    /// channel ([`chan::fdcheck`]). `None` is Plan 9's `-1`.
    fn fdtochan(
        &mut self,
        up: Pid,
        fd: Fd,
        mode: Option<u16>,
        chkmnt: bool,
    ) -> Result<std::rc::Rc<std::cell::RefCell<Chan>>, String> {
        let cell = self.chancell(up, fd)?;
        chan::fdcheck(&cell.borrow(), mode, chkmnt)?;
        Ok(cell)
    }

}

/// `Eisdir` (`error.h`) — *"file is a directory"*.
const EISDIR: &str = "file is a directory";
/// `Enegoff` (`error.h:50`).
const ENEGOFF: &str = "negative i/o offset";
/// `Edirseek` (`error.h:53`).
const EDIRSEEK: &str = "seek in directory";
const EBADFD: &str = "fd out of range or not open";
const ENODEV: &str = "no such device";
/// `Enofd` (`error.h:32`).
const ENOFD: &str = "no free file descriptors";

/// *"if(c->ismtpt){ dirname(d, &namelen); if(namelen)
/// nameerror(chanpath(c), Eismtpt); }"* (`sysfile.c:1181`).
fn renamesmtpt(c: &Chan, edir: &[u8]) -> Result<(), String> {
    if c.ismtpt && ninep::Dir::conv_m2d(edir).is_some_and(|d| !d.name.is_empty()) {
        return Err(namec::nameerror(&c.path, namec::EISMTPT));
    }
    Ok(())
}

/// *"if((flag&~MMASK) || (flag&MORDER)==(MBEFORE|MAFTER))
/// error(Ebadarg)"* (`sysfile.c:1000`).
fn bindflag(flag: i32) -> Result<(), String> {
    let morder = ns::mflag::MBEFORE | ns::mflag::MAFTER;
    if flag & !ns::mflag::MMASK != 0 || flag & morder == morder {
        return Err(proc::Procs::EBADARG.into());
    }
    Ok(())
}

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


#[cfg(test)]
mod syscalls {
    use super::*;
    use crate::ns::mflag::MCREATE;
    use crate::proc::rf;

    fn booted() -> Kernel {
        let mut k = tests::rooted(std::rc::Rc::new(tests::Recorder::silent()), &[("hello", b"greetings")]);
        k.tab.add(Box::new(devpipe::PipeDev::new(k.up.clone())));
        k.tab.add(Box::new(devenv::EnvDev::new(k.up.clone())));
        k
    }

    /// A kernel with no root mounted — `#/` alone — for a test whose own
    /// server must be the machine's first, `#9/0`.
    fn bare() -> Kernel {
        let mut k = Kernel::new(devroot::Root::new(), std::rc::Rc::new(tests::Recorder::silent())).unwrap();
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
        let r = k.syscall(1, Call::Open { path: "/hello".into(), mode: 0 });
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
            k.timerintr(t * 10_000_000, None);
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

    /// Put a NUL-terminated string into the test machine's memory at `at`.
    fn poke(log: &std::rc::Rc<std::cell::RefCell<tests::Log>>, at: u32, text: &str) {
        let mut b = text.as_bytes().to_vec();
        b.push(0);
        while b.len() % 4 != 0 {
            b.push(0);
        }
        for (i, w) in b.chunks(4).enumerate() {
            log.borrow_mut().mem.insert(at + 4 * i as u32, i32::from_le_bytes([w[0], w[1], w[2], w[3]]));
        }
    }

    /// A call as the machine makes it, with the raw words and a pc.
    fn trap(k: &mut Kernel, up: Pid, call: Call, s: [u64; machine::MAXSYSARG]) -> Result<Ret, String> {
        machine::Syscalls::syscall(k, up, call, &machine::Ureg { s, pc: &|| 0x1234 })
    }

    fn trace(k: &Kernel, pid: Pid) -> Option<String> {
        k.procs.borrow().get(pid).and_then(|p| p.syscalltrace.clone())
    }

    /// `startsyscall`: the process stops on its way into its next call with
    /// `syscallfmt`'s line (`pc/trap.c:682`), runs the call when started, and
    /// — started with `startsyscall` again — stops on the way out with
    /// `sysretfmt`'s (`:755`). Started plainly, it goes on.
    #[test]
    fn a_traced_call_stops_on_the_way_in_and_out() {
        let (mut k, log) = tests::watched();
        k.exec(1, "/init", &[]).unwrap();
        poke(&log, 0x40, "/nothing");
        k.procs.borrow_mut().get_mut(1).unwrap().procctl = Some(proc::Procctl::Tracesyscall);
        let open = Call::Open { path: "/nothing".into(), mode: 0 };
        assert_eq!(trap(&mut k, 1, open, [0x40, 0, 0, 0, 0]), Ok(Ret::Sched));
        assert_eq!(k.procs.borrow().state(1), proc::State::Stopped);
        assert_eq!(trace(&k, 1).as_deref(), Some("1 init Open 1234 0x40/\"/nothing\" 0x0"));
        // `startsyscall` again: ready, and traced on the way out.
        k.procs.borrow_mut().get_mut(1).unwrap().procctl = Some(proc::Procctl::Tracesyscall);
        k.procs.borrow_mut().ready(1);
        assert_eq!(k.resume(1), Ok(Ret::Sched));
        assert_eq!(k.procs.borrow().state(1), proc::State::Stopped);
        let t = trace(&k, 1).unwrap();
        assert!(t.starts_with(" = -1 '/nothing' file does not exist ") && t.ends_with('\n'), "{t:?}");
        // `start`: the call ends as it would have, and the trace is gone.
        k.procs.borrow_mut().ready(1);
        assert!(k.resume(1).is_err());
        assert_eq!(trace(&k, 1), None);
        // Nothing asked for more: the next call is not stopped.
        assert_eq!(trap(&mut k, 1, Call::Close { fd: 9 }, [9, 0, 0, 0, 0]).is_err(), true);
    }

    /// The arguments are shown as `syscallfmt` shows them — addresses as
    /// addresses, strings read out of the process, a write's first bytes
    /// with the unprintable as dots — and a read's answer as `sysretfmt`
    /// does.
    #[test]
    fn syscallfmt_shows_the_arguments_as_the_process_passed_them() {
        let (mut k, log) = tests::watched();
        k.exec(1, "/init", &[]).unwrap();
        poke(&log, 0x80, "new");
        poke(&log, 0x90, "old");
        let cases: Vec<(Call, [u64; 5], &str)> = vec![
            (Call::Bind { name: "new".into(), old: "old".into(), flag: 1 }, [0x80, 0x90, 1, 0, 0], "Bind 1234 0x80/\"new\" 0x90/\"old\" 0x1"),
            (Call::Pwrite { fd: 1, data: b"hi\n".to_vec(), off: -1 }, [1, 0x100, 3, u64::MAX, 0], "Pwrite 1234 1  0x100/\"hi.\" 3 -1"),
            (Call::Pread { fd: 0, n: 8, off: 0 }, [0, 0x200, 8, 0, 0], "Pread 1234 0 0x200 8 0"),
            (Call::Unmount { name: None, old: "old".into() }, [0, 0x90, 0, 0, 0], "Unmount 1234 0/\"\" 0x90/\"old\""),
            (Call::Sleep { ms: 10 }, [10, 0, 0, 0, 0], "Sleep 1234 10"),
            (Call::Rfork { flags: 0x20 }, [0x20, 0, 0, 0, 0], "Rfork 1234 0x20"),
        ];
        for (call, s, want) in cases {
            k.procs.borrow_mut().get_mut(1).unwrap().s = s;
            let got = k.syscallfmt(1, &call, &|| 0x1234).unwrap();
            assert_eq!(got, format!("1 init {want}"));
        }
        k.procs.borrow_mut().get_mut(1).unwrap().scallnr = sysno::PREAD;
        k.procs.borrow_mut().get_mut(1).unwrap().s = [0, 0x200, 8, 0, 0];
        let t = k.sysretfmt(1, &Ok(Ret::Data(b"ab\x01".to_vec())), 5, 7);
        assert_eq!(t, " 0x200/\"ab.\" 8 0 = 3 \"\" 5 7\n");
    }

    /// **`_stat` lays the old stat down as `packoldstat` does**
    /// (`sysfile.c:1224`): the name as the channel was reached
    /// (`pathlast`), 28 bytes each of name, uid and gid, `DMDIR` in the qid
    /// path for a directory, then vers, mode, the times, the length, type
    /// and dev — 116 bytes. A stat that does not fit its 128 is the old
    /// call's error.
    #[test]
    fn the_old_stat_is_packoldstats() {
        let dir = ninep::Dir {
            dtype: b'M' as u16,
            dev: 3,
            qid: ninep::Qid { qtype: ninep::QTDIR, vers: 7, path: 0x42 },
            mode: 0x8000_01ed,
            atime: 10,
            mtime: 20,
            length: 0,
            name: "servers-name".into(),
            uid: "kitty".into(),
            gid: "sys".into(),
            muid: "".into(),
        };
        let mut c = chan::Chan::attach(dev::DevId::Mnt, 0);
        c.path = "/usr/kitty/lib".into();
        let Ok(Ret::Data(b)) = oldstat(&c, dir.conv_d2m(), "old") else { panic!("no stat") };
        assert_eq!(b.len(), 116);
        assert_eq!(&b[..4], b"lib\0", "named by pathlast, not by the server");
        assert_eq!(&b[28..34], b"kitty\0");
        assert_eq!(&b[56..60], b"sys\0");
        assert_eq!(u32::from_le_bytes(b[84..88].try_into().unwrap()), 0x8000_0042);
        assert_eq!(u32::from_le_bytes(b[88..92].try_into().unwrap()), 7);
        assert_eq!(u32::from_le_bytes(b[96..100].try_into().unwrap()), 10);
        assert_eq!(u16::from_le_bytes(b[112..114].try_into().unwrap()), b'M' as u16);
        let long = ninep::Dir { uid: "u".repeat(100), ..dir };
        assert_eq!(oldstat(&c, long.conv_d2m(), "old"), Err("old".into()));
    }

    /// A string that runs off the process's memory is `validaddr`'s: the
    /// call fails without running, and the `NDebug` note ends the process on
    /// its way out, as *"sys: bad address in syscall"* does.
    #[test]
    fn a_traced_call_with_a_bad_string_fails() {
        let (mut k, _log) = tests::watched();
        k.exec(1, "/init", &[]).unwrap();
        k.procs.borrow_mut().get_mut(1).unwrap().procctl = Some(proc::Procctl::Tracesyscall);
        let r = trap(&mut k, 1, Call::Chdir { path: "x".into() }, [0x20000, 0, 0, 0, 0]);
        assert_eq!(r, Err(proc::Procs::EBADARG.into()));
        assert_eq!(k.procs.borrow().state(1), proc::State::Broken);
    }

    /// A process `stop`ped at the end of a call and started with
    /// `startsyscall` is traced from its next call: the stop was in
    /// `notify`, past the point where the call's own exit is traced.
    #[test]
    fn startsyscall_after_a_stop_traces_the_next_call() {
        let (mut k, _log) = tests::watched();
        k.exec(1, "/init", &[]).unwrap();
        k.procs.borrow_mut().get_mut(1).unwrap().procctl = Some(proc::Procctl::Stopme);
        assert_eq!(trap(&mut k, 1, Call::Sleep { ms: 0 }, [0; 5]), Ok(Ret::Sched));
        assert_eq!(k.procs.borrow().state(1), proc::State::Stopped);
        k.procs.borrow_mut().get_mut(1).unwrap().procctl = Some(proc::Procctl::Tracesyscall);
        k.procs.borrow_mut().ready(1);
        assert_eq!(k.resume(1), Ok(Ret::Ok), "no trace of the call that had ended");
        assert_eq!(trace(&k, 1), None);
        assert_eq!(trap(&mut k, 1, Call::Sleep { ms: 0 }, [0; 5]), Ok(Ret::Sched));
        assert_eq!(trace(&k, 1).as_deref(), Some("1 init Sleep 1234 0"));
    }

    /// `startstop` is `Proc_traceme`: nothing happens until a note is
    /// pending, and then the process stops before taking it (`proc.c:1498`).
    #[test]
    fn traceme_stops_at_the_next_note() {
        let (mut k, _log) = tests::watched();
        k.exec(1, "/init", &[]).unwrap();
        k.procs.borrow_mut().get_mut(1).unwrap().procctl = Some(proc::Procctl::Traceme);
        assert_eq!(k.syscall(1, Call::Sleep { ms: 0 }), Ok(Ret::Ok), "no note: it runs on");
        k.procs.borrow_mut().postnote(1, "interrupt", proc::NoteFlag::NUser);
        assert_eq!(k.syscall(1, Call::Sleep { ms: 0 }), Ok(Ret::Sched));
        assert_eq!(k.procs.borrow().state(1), proc::State::Stopped);
        assert_eq!(k.procs.borrow().get(1).unwrap().note.len(), 1, "the note is still there to take");
    }

    /// `exec` attaches the text segment, and two processes running the same
    /// file share one (`attachimage`); `rfork` shares it whatever the flags.
    #[test]
    fn processes_running_the_same_image_share_its_text() {
        let (mut k, _log) = tests::watched();
        k.exec(1, "/init", &[]).unwrap();
        let Ret::Pid(c) = k.syscall(1, Call::Rfork { flags: rf::PROC }).unwrap() else { panic!() };
        let Ret::Pid(d) = k.syscall(1, Call::Rfork { flags: rf::PROC }).unwrap() else { panic!() };
        k.exec(d, "/init", &[]).unwrap();
        let p = k.procs.borrow();
        let t = |pid| p.get(pid).unwrap().tseg.clone().unwrap();
        assert!(std::rc::Rc::ptr_eq(&t(1), &t(c)), "rfork shares it");
        assert!(std::rc::Rc::ptr_eq(&t(1), &t(d)), "exec of the same file finds it");
        assert_eq!(t(1).borrow().size, b"an image".len() as u64);
    }

    /// `profclock`: every 113ms in user mode, a tick is charged to the text
    /// segment's profile at the pc — the total in `[0]` — and nothing is
    /// charged for a tick taken in the kernel (`devproc.c:169`).
    #[test]
    fn profclock_charges_the_text_at_the_pc() {
        let (mut k, _log) = tests::watched();
        k.exec(1, "/init", &[]).unwrap();
        let tseg = k.procs.borrow().get(1).unwrap().tseg.clone().unwrap();
        tseg.borrow_mut().profile = Some(vec![0; 1]);
        k.procs.borrow_mut().timersinit(0);
        k.procs.borrow_mut().up = Some(1);
        k.procs.borrow_mut().setstate(1, proc::State::Running);
        k.timerintr(113_000_000, None);
        assert_eq!(tseg.borrow().profile.as_ref().unwrap()[0], 0, "in the kernel: not charged");
        k.timerintr(226_000_000, Some(&|| 4));
        assert_eq!(tseg.borrow().profile.as_ref().unwrap()[0], 20, "total and the pc's slot are both [0] here");
        k.timerintr(250_000_000, Some(&|| 4));
        assert_eq!(tseg.borrow().profile.as_ref().unwrap()[0], 20, "not due yet");
    }

    /// `semrelease` adds and answers the new value; `semacquire` takes one
    /// if there is one, and without `block` answers 0 at once if not
    /// (`sysproc.c:1109`, `:1075`).
    #[test]
    fn a_semaphore_counts() {
        let (mut k, log) = tests::watched();
        k.exec(1, "/init", &[]).unwrap();
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
        k.exec(1, "/init", &[]).unwrap();
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
        k.exec(1, "/init", &[]).unwrap();
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
        k.exec(1, "/init", &[]).unwrap();
        let now = k.machine.todget().nsec;
        k.procs.borrow_mut().timersinit(now);
        assert_eq!(k.syscall(1, Call::Tsemacquire { addr: 8, ms: 30 }), Ok(Ret::Sched));
        for t in 1..=4 {
            k.timerintr(now + t * 10_000_000, None);
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
            k.exec(1, "/init", &[]).unwrap();
            log.borrow_mut().mem.insert(16, -1);
            let s = match call {
                Call::Semacquire { addr, block } => [addr as u64, block as u64, 0, 0, 0],
                Call::Semrelease { addr, delta } => [addr as u64, delta as u32 as u64, 0, 0, 0],
                _ => unreachable!(),
            };
            assert_eq!(trap(&mut k, 1, call.clone(), s), bad, "{call:?}");
            assert_eq!(notes(&k), note.into_iter().map(String::from).collect::<Vec<_>>(), "{call:?}");
        }
    }

    fn notes_of(k: &Kernel, pid: Pid) -> Vec<String> {
        k.procs.borrow().get(pid).unwrap().note.iter().map(|n| n.msg.clone()).collect()
    }

    /// **A call given a bad address fails and posts `validaddr`'s note**
    /// (`fault.c:310`), which ends the process on its way out: a name that
    /// starts outside the process's memory, a buffer that runs past its
    /// end, a name that reaches its end without a NUL (`validname`'s
    /// `vmemchr`), a string in `exec`'s `argv`. The call does not run.
    #[test]
    fn a_bad_address_fails_the_call_with_validaddrs_note() {
        let bad = Err(proc::Procs::EBADARG.to_string());
        let cases: Vec<(Call, [u64; 5], fn(&std::rc::Rc<std::cell::RefCell<tests::Log>>))> = vec![
            (Call::Open { path: String::new(), mode: 0 }, [0x20000, 0, 0, 0, 0], |_| {}),
            (Call::Pread { fd: 0, n: 0x100, off: 0 }, [0, 0xfff0, 0x100, 0, 0], |_| {}),
            (Call::Pwrite { fd: 1, data: vec![], off: 0 }, [1, 0x40, (-1i32) as u32 as u64, 0, 0], |_| {}),
            (Call::Chdir { path: String::new() }, [0xfff8, 0, 0, 0, 0], |l| {
                // the last two words of memory, and no NUL in either
                l.borrow_mut().mem.insert(0xfff8, 0x6161_6161);
                l.borrow_mut().mem.insert(0xfffc, 0x6161_6161);
            }),
            (Call::Exec { path: "/init".into(), args: vec![] }, [0x40, 0x80, 0, 0, 0], |l| {
                poke(l, 0x40, "/init");
                l.borrow_mut().mem.insert(0x80, 0x30000);
            }),
            (Call::Pipe, [0xfffc, 0, 0, 0, 0], |_| {}),
        ];
        for (call, s, setup) in cases {
            let (mut k, log) = tests::watched();
            k.exec(1, "/init", &[]).unwrap();
            setup(&log);
            let ran = log.borrow().args.len();
            assert_eq!(trap(&mut k, 1, call.clone(), s), bad, "{call:?}");
            assert_eq!(notes_of(&k, 1), vec!["sys: bad address in syscall"], "{call:?}");
            assert_eq!(k.procs.borrow().state(1), proc::State::Broken, "{call:?}: ended by the note");
            assert_eq!(log.borrow().args.len(), ran, "{call:?}: exec did not run");
        }
    }

    /// `pipe`'s pair and a semaphore must be aligned: *"sys: odd address"*
    /// (`validalign`); `errstr` with no buffer is `Ebadarg` and no note.
    #[test]
    fn misaligned_and_empty_arguments() {
        let (mut k, _log) = tests::watched();
        k.exec(1, "/init", &[]).unwrap();
        assert_eq!(trap(&mut k, 1, Call::Pipe, [0x42, 0, 0, 0, 0]), Err(proc::Procs::EBADARG.into()));
        assert_eq!(notes_of(&k, 1), vec!["sys: odd address"]);
        let (mut k, _log) = tests::watched();
        k.exec(1, "/init", &[]).unwrap();
        assert_eq!(trap(&mut k, 1, Call::Errstr { buf: String::new() }, [0x40, 0, 0, 0, 0]), Err(proc::Procs::EBADARG.into()));
        assert!(notes_of(&k, 1).is_empty());
    }

    /// `exits` with a bad status is not an error: the process exits with
    /// *"invalid exit string"* (`sysproc.c:671`–`:675`).
    #[test]
    fn exits_with_a_bad_status_is_an_invalid_exit_string() {
        let (mut k, _log) = tests::watched();
        k.exec(1, "/init", &[]).unwrap();
        let _ = trap(&mut k, 1, Call::Exits { status: String::new() }, [0x20000, 0, 0, 0, 0]);
        assert_eq!(k.procs.borrow().status(1).as_deref(), Some("init 1: invalid exit string"));
    }

    /// The kernel's own calls — made at boot with names from the kernel's
    /// memory, not a process's — carry no argument words and are not
    /// checked as a process's are; and `notify`'s argument, a function and
    /// not an address here, is not checked either.
    #[test]
    fn what_is_not_checked() {
        let (mut k, _log) = tests::watched();
        k.exec(1, "/init", &[]).unwrap();
        assert!(k.syscall(1, Call::Open { path: "/init".into(), mode: 0 }).is_ok());
        assert_eq!(trap(&mut k, 1, Call::Notify { f: 0x7fff_0000 }, [0x7fff_0000, 0, 0, 0, 0]), Ok(Ret::Ok));
        assert!(notes_of(&k, 1).is_empty());
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
        let before = k.procs.borrow().m.syscall;
        let _ = k.syscall(1, Call::Errstr { buf: String::new() });
        let _ = k.syscall(1, Call::Open { path: "/nothing".into(), mode: 0 });
        assert_eq!(k.procs.borrow().m.syscall - before, 2, "a failed call is still a call");
    }

    /// **A directory seeks only to 0** (`sysfile.c:820`, `Eisdir`), and a
    /// seek clears `c->dri` so the next read starts at the first entry. A
    /// byte offset cannot say where an entry begins, because entries are not
    /// all the same length.
    #[test]
    fn a_directory_seeks_only_to_the_beginning() {
        let mut k = booted();
        let Ret::Fd(fd) = k.syscall(1, Call::Open { path: "/root".into(), mode: 0 }).unwrap()
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

    /// `sseek` (`sysfile.c:793`): whence 2 is from the end, which is the
    /// length the file's server states — how `getenv` sizes a variable
    /// before reading it (`9sys/getenv.c`); a pipe is `Eisstream`; any other
    /// whence is `Ebadarg`.
    #[test]
    fn seek_from_the_end_is_the_length_stat_gives() {
        let mut k = booted();
        let Ret::Fd(fd) = k.syscall(1, Call::Open { path: "/init".into(), mode: 0 }).unwrap() else {
            panic!()
        };
        assert_eq!(k.syscall(1, Call::Seek { fd, off: 0, whence: 2 }), Ok(Ret::N(b"an image".len())));
        assert_eq!(k.syscall(1, Call::Seek { fd, off: -2, whence: 2 }), Ok(Ret::N(6)));
        assert_eq!(k.syscall(1, Call::Seek { fd, off: -20, whence: 2 }), Err("negative i/o offset".into()));
        assert_eq!(k.syscall(1, Call::Seek { fd, off: 0, whence: 3 }), Err(proc::Procs::EBADARG.into()));
        let Ret::Fd(d) = k.syscall(1, Call::Open { path: "/root".into(), mode: 0 }).unwrap() else { panic!() };
        assert_eq!(k.syscall(1, Call::Seek { fd: d, off: 0, whence: 2 }), Err(EISDIR.into()));
        let Ret::Two(p, _) = k.syscall(1, Call::Pipe).unwrap() else { panic!() };
        assert_eq!(k.syscall(1, Call::Seek { fd: p, off: 0, whence: 0 }), Err("seek on a stream".into()));
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
        fn open(&mut self, mut c: Chan, m: u16) -> Result<Chan, String> {
            // every device's open: *"c->mode = openmode(omode)"*
            c.mode = crate::chan::openmode(m)?;
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
        thread_local! {
            /// What each fid is, as a server keeps it: the root after an
            /// attach, what a whole walk reached, gone at a clunk.
            static FIDS: std::cell::RefCell<std::collections::HashMap<u32, Qid>> = Default::default();
        }
        let root = Qid { qtype: QTDIR, vers: 0, path: 0 };
        let m = unframe(req).expect("malformed");
        let mut r = R::new(m.body);
        let error = |e: &str| W::new().s(e).frame(T::Error as u8, m.tag);
        match m.ty {
            x if x == T::Version as u8 => {
                let msize = r.u32().unwrap();
                let _v = r.s().unwrap();
                W::new().u32(msize.min(8192)).s("9P2000").frame(T::Version.reply(), m.tag)
            }
            x if x == T::Attach as u8 => {
                let fid = r.u32().unwrap();
                FIDS.with(|f| f.borrow_mut().insert(fid, root));
                W::new().raw(&root.write(W::new()).into_body()).frame(T::Attach.reply(), m.tag)
            }
            // walk(5): *"If the first element cannot be walked for any
            // reason, Rerror is returned"*, and *newfid* is made only when
            // every name was walked
            x if x == T::Walk as u8 => {
                let (from, newfid) = (r.u32().unwrap(), r.u32().unwrap());
                let Some(mut at) = FIDS.with(|f| f.borrow().get(&from).copied()) else {
                    return error("unknown fid");
                };
                let n = r.u16().unwrap();
                let mut qids = Vec::new();
                for _ in 0..n {
                    let q = match (at.path, r.s().unwrap()) {
                        (0, "answer") => Qid { qtype: 0, vers: 0, path: 1 },
                        (0, "sub") => Qid { qtype: QTDIR, vers: 0, path: 2 },
                        (2, "answer") => Qid { qtype: 0, vers: 0, path: 3 },
                        _ => break,
                    };
                    qids.push(q);
                    at = q;
                }
                if n > 0 && qids.is_empty() {
                    return error("file does not exist");
                }
                if qids.len() == n as usize {
                    FIDS.with(|f| f.borrow_mut().insert(newfid, at));
                }
                let mut w = W::new().u16(qids.len() as u16);
                for q in &qids {
                    w = w.raw(&q.write(W::new()).into_body());
                }
                w.frame(T::Walk.reply(), m.tag)
            }
            x if x == T::Open as u8 => {
                let fid = r.u32().unwrap();
                let Some(q) = FIDS.with(|f| f.borrow().get(&fid).copied()) else {
                    return error("unknown fid");
                };
                W::new().raw(&q.write(W::new()).into_body()).u32(0).frame(T::Open.reply(), m.tag)
            }
            x if x == T::Read as u8 => {
                let (fid, off, count) = (r.u32().unwrap(), r.u64().unwrap(), r.u32().unwrap());
                let Some(q) = FIDS.with(|f| f.borrow().get(&fid).copied()) else {
                    return error("unknown fid");
                };
                // the root reads as its one file's entry
                let entry = ninep::Dir {
                    qid: Qid { qtype: 0, vers: 0, path: 1 },
                    mode: 0o444,
                    length: 14,
                    name: "answer".into(),
                    ..Default::default()
                }
                .conv_d2m();
                let data: &[u8] = match q.path {
                    0 => &entry,
                    2 => &[],
                    _ => b"served over 9P",
                };
                let off = off as usize;
                let end = (off + count as usize).min(data.len());
                let slice = if off >= data.len() { &[][..] } else { &data[off..end] };
                W::new().u32(slice.len() as u32).raw(slice).frame(T::Read.reply(), m.tag)
            }
            x if x == T::Write as u8 => {
                let (_fid, _off, count) = (r.u32().unwrap(), r.u64().unwrap(), r.u32().unwrap());
                W::new().u32(count).frame(T::Write.reply(), m.tag)
            }
            x if x == T::Clunk as u8 => {
                let fid = r.u32().unwrap();
                if FIDS.with(|f| f.borrow_mut().remove(&fid)).is_none() {
                    return error("unknown fid");
                }
                W::new().frame(T::Clunk.reply(), m.tag)
            }
            _ => error("not implemented"),
        }
    }

    /// **A posted channel is held by `/srv`, and an open of its name is a
    /// reference, not a new open** — *"fdtochan(fd, -1, 0, 1); /* error
    /// check and inc ref */"* (`devsrv.c:315`) and *"incref(sp->chan)"*
    /// (`:135`). The poster closes its descriptor, as `plumber` does, and a
    /// client opens the name and closes it, as `plumb` does: the pipe's other
    /// end is still there to be written to, and not hung up.
    #[test]
    fn a_posted_pipe_outlives_its_poster_and_its_openers() {
        let mut k = booted();
        k.tab.add(Box::new(devsrv::SrvDev::new(k.up.clone())));
        let Ret::Two(a, b) = k.syscall(1, Call::Pipe).unwrap() else { panic!() };
        let Ret::Fd(post) = k
            .syscall(1, Call::Create { path: "#s/p".into(), mode: 1, perm: 0o600 })
            .unwrap()
        else {
            panic!()
        };
        k.syscall(1, Call::Pwrite { fd: post, data: b.to_string().into_bytes(), off: -1 }).unwrap();
        k.syscall(1, Call::Close { fd: b }).unwrap();
        let Ret::Fd(got) = k.syscall(1, Call::Open { path: "#s/p".into(), mode: 2 }).unwrap() else {
            panic!()
        };
        k.syscall(1, Call::Close { fd: got }).unwrap();
        assert_eq!(
            k.syscall(1, Call::Pwrite { fd: a, data: b"still".to_vec(), off: -1 }),
            Ok(Ret::N(5)),
            "the end /srv holds was closed under it"
        );
    }

    /// **P2's acceptance, whole.** A channel is posted at `/srv`; it is opened
    /// by name, mounted, and a file is then resolved THROUGH that mount by an
    /// ordinary `open`. Four devices and the namespace, in one path.
    #[test]
    fn a_posted_channel_is_mounted_and_a_name_resolves_through_it() {
        let mut k = booted();
        k.tab.add(Box::new(Server9P::default()));
        k.tab.add(Box::new(devsrv::SrvDev::new(k.up.clone())));

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

        // `#e` has `x`; `#/` has `dev`. Bind both onto `/mnt`, in order.
        // (`#/` and not `#/dev`: everything between the device letter and
        // the first `/` is the ATTACH SPEC, so `#/dev` attaches the root
        // device with the spec `dev` — see `dev::split`.)
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
            k.syscall(1, Call::Open { path: "/mnt/dev".into(), mode: 0 }).is_ok(),
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

        let mut k = bare();
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
        let mut k = bare();
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
                    name: "/hello".into(),
                    old: "/init".into(),
                    flag: 0
                }
            )
            .map(|r| matches!(r, Ret::N(id) if id > 0)),
            Ok(true),
            "the mount's id, as `bind(2)` answers it"
        );
        // and the name now answers with what was bound over it
        let Ret::Fd(fd) = k.syscall(1, Call::Open { path: "/init".into(), mode: 0 }).unwrap()
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
        let e = k.exec(1, "/dev", &[]).unwrap_err();
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
    /// (`sysproc.c:564`), and any other its parent's. `#/` carries no
    /// images here — the host attaches the root before the first program
    /// runs — so an image is a file server's, and runs at `PriNormal`.
    #[test]
    fn an_image_from_a_file_server_runs_at_prinormal() {
        let mut k = booted();
        k.exec(1, "/init", &[]).unwrap();
        let procs = k.procs.borrow();
        let p = procs.get(1).unwrap();
        assert_eq!((p.basepri, p.priority), (proc::pri::NORMAL, proc::pri::NORMAL));
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


    fn fd(r: Result<Ret, String>) -> Fd {
        match r {
            Ok(Ret::Fd(fd)) => fd,
            r => panic!("{r:?}"),
        }
    }

    /// **A descriptor is used in the mode it was opened in** — `fdtochan`'s
    /// check, which `read` and `write` make with `OREAD` and `OWRITE`
    /// (`sysfile.c:637`, `:728`, `:152`): a file opened for reading is not
    /// written through, nor one opened for writing read; `ORDWR` is both;
    /// `OEXEC` opens for reading (`openmode`, `sysfile.c:167`); and a mode
    /// past `OEXEC` is `Ebadarg`. Reads and writes checked nothing.
    #[test]
    fn a_descriptor_is_used_only_in_the_mode_it_was_opened_in() {
        let mut k = booted();
        let w = fd(k.syscall(1, Call::Create { path: "#e/x".into(), mode: 1, perm: 0o666 }));
        assert_eq!(k.syscall(1, Call::Pread { fd: w, n: 8, off: 0 }), Err(chan::EBADUSEFD.into()));
        k.syscall(1, Call::Pwrite { fd: w, data: b"abc".to_vec(), off: 0 }).unwrap();
        let r = fd(k.syscall(1, Call::Open { path: "#e/x".into(), mode: 0 }));
        assert_eq!(k.syscall(1, Call::Pwrite { fd: r, data: b"z".to_vec(), off: 0 }), Err(chan::EBADUSEFD.into()));
        assert_eq!(k.syscall(1, Call::Pread { fd: r, n: 8, off: 0 }).unwrap(), Ret::Data(b"abc".to_vec()));
        let rw = fd(k.syscall(1, Call::Open { path: "#e/x".into(), mode: 2 }));
        k.syscall(1, Call::Pwrite { fd: rw, data: b"d".to_vec(), off: 3 }).unwrap();
        assert_eq!(k.syscall(1, Call::Pread { fd: rw, n: 8, off: 0 }).unwrap(), Ret::Data(b"abcd".to_vec()));
        let x = fd(k.syscall(1, Call::Open { path: "#e/x".into(), mode: 3 }));
        assert!(k.syscall(1, Call::Pread { fd: x, n: 8, off: 0 }).is_ok(), "OEXEC is OREAD");
        assert!(k.syscall(1, Call::Pwrite { fd: x, data: b"z".to_vec(), off: 0 }).is_err());
        assert_eq!(k.syscall(1, Call::Open { path: "#e/x".into(), mode: 4 }), Err("bad arg in system call".into()));
        assert_eq!(k.syscall(1, Call::Open { path: "#e/x".into(), mode: 0x10000 }), Err("bad arg in system call".into()));
    }

    /// **`#d` answers the descriptor's channel in its own mode**
    /// (`devdup.c:86`): opening it by name for another mode is `Ebadusefd`,
    /// so a descriptor open for reading does not become one open for
    /// writing — which setting the mode on it made it. The directory opens
    /// only to read (*"error(Eisdir)"*).
    #[test]
    fn opening_a_descriptor_by_name_does_not_change_its_mode() {
        let mut k = booted();
        k.tab.add(Box::new(devdup::DupDev::new(k.up.clone())));
        let w = fd(k.syscall(1, Call::Create { path: "#e/x".into(), mode: 1, perm: 0o666 }));
        k.syscall(1, Call::Pwrite { fd: w, data: b"kept".to_vec(), off: 0 }).unwrap();
        let r = fd(k.syscall(1, Call::Open { path: "#e/x".into(), mode: 0 }));
        assert_eq!(
            k.syscall(1, Call::Open { path: format!("#d/{r}"), mode: 1 }),
            Err(format!("'#d/{r}' {}", chan::EBADUSEFD)),
        );
        let again = fd(k.syscall(1, Call::Open { path: format!("#d/{r}"), mode: 0 }));
        assert!(k.syscall(1, Call::Pwrite { fd: again, data: b"z".to_vec(), off: 0 }).is_err());
        assert_eq!(k.syscall(1, Call::Pread { fd: again, n: 8, off: 0 }).unwrap(), Ret::Data(b"kept".to_vec()));
        assert!(k.syscall(1, Call::Open { path: "#d".into(), mode: 1 }).is_err());
    }

    /// **`dup` onto an open descriptor closes it** (`sysfile.c:246`): a
    /// pipe's only writer replaced that way is gone, and the reader reaches
    /// end of file rather than waiting for a writer that cannot write.
    #[test]
    fn dup_onto_the_only_writer_of_a_pipe_ends_its_reader() {
        let mut k = booted();
        let Ok(Ret::Two(a, b)) = k.syscall(1, Call::Pipe) else { panic!() };
        let other = fd(k.syscall(1, Call::Create { path: "#e/x".into(), mode: 1, perm: 0o666 }));
        assert_eq!(k.syscall(1, Call::Dup { old: other, new: a }).unwrap(), Ret::Fd(a));
        assert_eq!(k.syscall(1, Call::Pread { fd: b, n: 8, off: -1 }).unwrap(), Ret::Data(Vec::new()), "end of file");
    }

    /// **`exec` closes what was opened close-on-exec** (`sysproc.c:592`),
    /// and leaves the rest.
    #[test]
    fn exec_closes_the_descriptors_opened_close_on_exec() {
        let mut k = booted();
        let keep = fd(k.syscall(1, Call::Open { path: "/hello".into(), mode: 0 }));
        let ocexec = (chan::mode::OREAD | chan::mode::OCEXEC) as i32;
        let gone = fd(k.syscall(1, Call::Open { path: "/hello".into(), mode: ocexec }));
        k.exec(1, "/init", &[]).unwrap();
        assert!(k.syscall(1, Call::Pread { fd: keep, n: 1, off: 0 }).is_ok());
        assert_eq!(k.syscall(1, Call::Pread { fd: gone, n: 1, off: 0 }), Err(EBADFD.into()), "closed by exec");
    }

    /// **`create` sets the channel flags an open sets** (`chan.c:1616`):
    /// a `/srv` file created `ORCLOSE` goes when its creator closes it
    /// (`srvclose`, `devsrv.c:286`) — which is how a server's entry does not
    /// outlive it — and one created plainly stays.
    #[test]
    fn a_srv_file_created_orclose_goes_with_its_creator() {
        let mut k = booted();
        k.tab.add(Box::new(devsrv::SrvDev::new(k.up.clone())));
        let orclose = (chan::mode::OWRITE | chan::mode::ORCLOSE) as i32;
        let tmp = fd(k.syscall(1, Call::Create { path: "#s/tmp".into(), mode: orclose, perm: 0o600 }));
        let kept = fd(k.syscall(1, Call::Create { path: "#s/kept".into(), mode: 1, perm: 0o600 }));
        k.syscall(1, Call::Close { fd: tmp }).unwrap();
        k.syscall(1, Call::Close { fd: kept }).unwrap();
        assert!(k.walk(1, "#s/tmp", namec::A::Access, 0).is_err(), "removed on close");
        assert!(k.walk(1, "#s/kept", namec::A::Access, 0).is_ok());
    }

    /// **An open of `/fd/n` is the descriptor's own channel** (`dupopen`,
    /// `devdup.c:86`): one more reference to it, not a copy — so the two
    /// share one offset, as a `dup` does, and closing one leaves the other.
    #[test]
    fn a_descriptor_opened_by_name_shares_its_offset() {
        let mut k = booted();
        k.tab.add(Box::new(devdup::DupDev::new(k.up.clone())));
        let w = fd(k.syscall(1, Call::Create { path: "#e/x".into(), mode: 1, perm: 0o666 }));
        k.syscall(1, Call::Pwrite { fd: w, data: b"abcdef".to_vec(), off: 0 }).unwrap();
        let r = fd(k.syscall(1, Call::Open { path: "#e/x".into(), mode: 0 }));
        assert_eq!(k.syscall(1, Call::Pread { fd: r, n: 2, off: -1 }), Ok(Ret::Data(b"ab".to_vec())));
        let d = fd(k.syscall(1, Call::Open { path: format!("#d/{r}"), mode: 0 }));
        assert_eq!(k.syscall(1, Call::Pread { fd: d, n: 2, off: -1 }), Ok(Ret::Data(b"cd".to_vec())));
        assert_eq!(k.syscall(1, Call::Pread { fd: r, n: 2, off: -1 }), Ok(Ret::Data(b"ef".to_vec())));
        k.syscall(1, Call::Close { fd: r }).unwrap();
        assert_eq!(k.syscall(1, Call::Pread { fd: d, n: 2, off: 0 }), Ok(Ret::Data(b"ab".to_vec())));
    }

    /// **An open of a posted name is the posted channel** (`srvopen`,
    /// `devsrv.c:135`): the poster's descriptor and every opener share one
    /// channel, and one offset.
    #[test]
    fn a_posted_channel_and_its_openers_share_its_offset() {
        let mut k = booted();
        k.tab.add(Box::new(devsrv::SrvDev::new(k.up.clone())));
        let w = fd(k.syscall(1, Call::Create { path: "#e/x".into(), mode: 1, perm: 0o666 }));
        k.syscall(1, Call::Pwrite { fd: w, data: b"abcdef".to_vec(), off: 0 }).unwrap();
        let r = fd(k.syscall(1, Call::Open { path: "#e/x".into(), mode: 0 }));
        let post = fd(k.syscall(1, Call::Create { path: "#s/x".into(), mode: 1, perm: 0o666 }));
        k.syscall(1, Call::Pwrite { fd: post, data: r.to_string().into_bytes(), off: -1 }).unwrap();
        let o = fd(k.syscall(1, Call::Open { path: "#s/x".into(), mode: 0 }));
        assert_eq!(k.syscall(1, Call::Pread { fd: o, n: 2, off: -1 }), Ok(Ret::Data(b"ab".to_vec())));
        assert_eq!(k.syscall(1, Call::Pread { fd: r, n: 2, off: -1 }), Ok(Ret::Data(b"cd".to_vec())));
    }

    /// `srvremove` (`devsrv.c:186`): an eve-owned name is eve's to remove,
    /// `root` nobody's, and a name others may not write its owner's or
    /// eve's. Removing one closes what was posted — *"cclose(sp->chan)"*
    /// (`:227`) — so a pipe end nobody else holds hangs up.
    #[test]
    fn a_posted_name_is_removed_by_whoever_may_and_closes_what_was_posted() {
        let mut k = booted();
        k.tab.add(Box::new(devsrv::SrvDev::new(k.up.clone())));
        *k.tab.eve().borrow_mut() = "kitty".into();
        let as_ = |k: &mut Kernel, u: &str| k.procs.borrow_mut().setuser(1, u);
        let mk = |k: &mut Kernel, name: &str, perm: u32| {
            let f = fd(k.syscall(1, Call::Create { path: format!("#s/{name}"), mode: 1, perm }));
            k.syscall(1, Call::Close { fd: f }).unwrap();
        };
        let rm = |k: &mut Kernel, name: &str| k.syscall(1, Call::Remove { path: format!("#s/{name}") });
        as_(&mut k, "kitty");
        mk(&mut k, "system", 0o666);
        mk(&mut k, devsrv::ROOTSRV, 0o666);
        as_(&mut k, "glenda");
        mk(&mut k, "personal", 0o600);
        mk(&mut k, "shared", 0o667);
        as_(&mut k, "other");
        assert_eq!(rm(&mut k, "system"), Err("permission denied".into()), "eve's");
        assert_eq!(rm(&mut k, "personal"), Err("permission denied".into()), "glenda's");
        assert_eq!(rm(&mut k, "shared"), Ok(Ret::Ok), "anyone may write it");
        as_(&mut k, "glenda");
        assert_eq!(rm(&mut k, "personal"), Ok(Ret::Ok));
        as_(&mut k, "kitty");
        assert_eq!(rm(&mut k, devsrv::ROOTSRV), Err("permission denied".into()), "nobody's");
        assert_eq!(rm(&mut k, "system"), Ok(Ret::Ok));

        let Ret::Two(a, b) = k.syscall(1, Call::Pipe).unwrap() else { panic!() };
        let p = fd(k.syscall(1, Call::Create { path: "#s/p".into(), mode: 1, perm: 0o600 }));
        k.syscall(1, Call::Pwrite { fd: p, data: b.to_string().into_bytes(), off: -1 }).unwrap();
        k.syscall(1, Call::Close { fd: b }).unwrap();
        k.syscall(1, Call::Close { fd: p }).unwrap();
        assert_eq!(rm(&mut k, "p"), Ok(Ret::Ok));
        assert_eq!(
            k.syscall(1, Call::Pread { fd: a, n: 8, off: -1 }),
            Ok(Ret::Data(Vec::new())),
            "the end /srv held is closed, so the other reads end of file"
        );
    }

    /// `srvwstat` (`devsrv.c:235`): the owner, or eve, may rename a posted
    /// name and change its mode; a `/` in the name is `Ebadchar`.
    #[test]
    fn a_posted_names_owner_may_rename_it() {
        let mut k = booted();
        k.tab.add(Box::new(devsrv::SrvDev::new(k.up.clone())));
        *k.tab.eve().borrow_mut() = "kitty".into();
        k.procs.borrow_mut().setuser(1, "glenda");
        let f = fd(k.syscall(1, Call::Create { path: "#s/a".into(), mode: 1, perm: 0o600 }));
        k.syscall(1, Call::Close { fd: f }).unwrap();
        let wstat = |name: &str, mode: u32| {
            ninep::Dir { name: name.into(), mode, atime: !0, mtime: !0, length: !0, ..Default::default() }.conv_d2m()
        };
        k.procs.borrow_mut().setuser(1, "other");
        assert!(k.syscall(1, Call::Wstat { path: "#s/a".into(), edir: wstat("b", !0) }).is_err());
        k.procs.borrow_mut().setuser(1, "glenda");
        assert_eq!(
            k.syscall(1, Call::Wstat { path: "#s/a".into(), edir: wstat("x/y", !0) }),
            Err("bad character in file name".into())
        );
        k.syscall(1, Call::Wstat { path: "#s/a".into(), edir: wstat("b", 0o644) }).unwrap();
        assert!(k.walk(1, "#s/a", namec::A::Access, 0).is_err());
        let Ret::Data(st) = k.syscall(1, Call::Stat { path: "#s/b".into() }).unwrap() else { panic!() };
        assert_eq!(ninep::Dir::conv_m2d(&st).unwrap().mode & 0o777, 0o644);
    }

    /// `srvwrite` (`devsrv.c:302`): the descriptor is `strtoul`'s number in
    /// under 32 bytes — `0x` hexadecimal, and what follows the digits
    /// ignored — and a name is posted once (`Ebadusefd`).
    #[test]
    fn a_posted_descriptor_is_strtouls_number() {
        let mut k = booted();
        k.tab.add(Box::new(devsrv::SrvDev::new(k.up.clone())));
        let Ret::Two(_, b) = k.syscall(1, Call::Pipe).unwrap() else { panic!() };
        let post = fd(k.syscall(1, Call::Create { path: "#s/p".into(), mode: 1, perm: 0o600 }));
        let long = format!("{b}{}", " ".repeat(40));
        assert_eq!(
            k.syscall(1, Call::Pwrite { fd: post, data: long.into_bytes(), off: -1 }),
            Err("jmk added reentrancy for threads".into()),
            "Egreg"
        );
        let hex = format!("0x{b:x} and then some");
        k.syscall(1, Call::Pwrite { fd: post, data: hex.into_bytes(), off: -1 }).unwrap();
        assert_eq!(
            k.syscall(1, Call::Pwrite { fd: post, data: b.to_string().into_bytes(), off: -1 }),
            Err(chan::EBADUSEFD.into()),
            "posted once"
        );
    }

    /// `newfd2` (`sysfile.c:94`): `pipe`'s `data` is in the lower descriptor
    /// and `data1` in the next free one.
    #[test]
    fn a_pipes_data_end_is_its_lower_descriptor() {
        let mut k = booted();
        let first = fd(k.syscall(1, Call::Open { path: "#e".into(), mode: 0 }));
        let gap = fd(k.syscall(1, Call::Open { path: "#e".into(), mode: 0 }));
        k.syscall(1, Call::Close { fd: first }).unwrap();
        let Ret::Two(a, b) = k.syscall(1, Call::Pipe).unwrap() else { panic!() };
        assert_eq!((a, b), (first, gap + 1));
        let Ret::Data(st) = k.syscall(1, Call::Fstat { fd: a }).unwrap() else { panic!() };
        assert_eq!(ninep::Dir::conv_m2d(&st).unwrap().name, "data");
    }

    /// The far end of a pipe as a file server process: read the request the
    /// client sent, and answer it as `serve9p` does.
    fn answer(k: &mut Kernel, server: Pid, fd: Fd) -> Vec<u8> {
        let Ok(Ret::Data(req)) = k.syscall(server, Call::Pread { fd, n: 8192, off: -1 }) else {
            panic!("the client sent nothing")
        };
        k.syscall(server, Call::Pwrite { fd, data: serve9p(&req), off: -1 }).unwrap();
        req
    }

    /// A call of `pid`'s through a mount whose server is `server`, run until
    /// it finishes: each time the client leaves the processor, the server
    /// answers what it sent.
    fn served(k: &mut Kernel, pid: Pid, server: Pid, fd: Fd, call: Call) -> Result<Ret, String> {
        let mut r = k.syscall(pid, call);
        while r == Ok(Ret::Sched) {
            answer(k, server, fd);
            r = k.resume(pid);
        }
        r
    }

    /// A mount over `/` whose server is a process on a pipe's far end, and
    /// `/answer` opened through it: the client, the server, the server's end
    /// and the descriptor.
    fn mounted(k: &mut Kernel) -> (Pid, Fd, Fd) {
        let Ret::Two(client, end) = k.syscall(1, Call::Pipe).unwrap() else { panic!() };
        let Ret::Pid(server) = k.syscall(1, Call::Rfork { flags: rf::PROC }).unwrap() else { panic!() };
        let mount = Call::Mount { fd: client, afd: -1, old: "/".into(), flag: 1, aname: String::new() };
        served(k, 1, server, end, mount).expect("mount");
        let Ok(Ret::Fd(fd)) = served(k, 1, server, end, Call::Open { path: "/answer".into(), mode: 0 }) else {
            panic!("the mounted server was not reached")
        };
        (server, end, fd)
    }

    /// **A last close waits for `Rclunk`** (`mntclunk`, `devmnt.c`): the
    /// server is told to let the fid go, and the close returns once it has
    /// — not before, as it did when a clunk waited for nobody.
    #[test]
    fn a_close_through_a_mount_waits_for_the_server_to_clunk() {
        let mut k = booted();
        let (server, end, fd) = mounted(&mut k);
        assert_eq!(k.syscall(1, Call::Close { fd }), Ok(Ret::Sched), "it waits for Rclunk");
        assert_eq!(k.procs.borrow().state(1), proc::State::Wakeme);
        let req = answer(&mut k, server, end);
        assert_eq!(req[4], ninep::T::Clunk as u8);
        assert_eq!(k.procs.borrow().state(1), proc::State::Ready, "the answer woke it");
        assert_eq!(k.resume(1), Ok(Ret::Ok));
        assert_eq!(k.syscall(1, Call::Pread { fd, n: 1, off: 0 }), Err(EBADFD.into()), "and it is closed");
    }

    /// `dup` onto an open descriptor closes what it replaces, waiting as any
    /// last close does (`sysfile.c:246`), and answers the slot after.
    #[test]
    fn dup_waits_for_the_close_of_what_it_replaces() {
        let mut k = booted();
        let (server, end, fd) = mounted(&mut k);
        let other = fd_(k.syscall(1, Call::Open { path: "#e".into(), mode: 0 }));
        assert_eq!(k.syscall(1, Call::Dup { old: other, new: fd }), Ok(Ret::Sched));
        assert_eq!(answer(&mut k, server, end)[4], ninep::T::Clunk as u8);
        assert_eq!(k.resume(1), Ok(Ret::Fd(fd)));
    }

    fn fd_(r: Result<Ret, String>) -> Fd {
        match r {
            Ok(Ret::Fd(f)) => f,
            r => panic!("{r:?}"),
        }
    }

    /// **`pexit` closes before it tells the parent** (`proc.c:1160`, then
    /// `:1219`): a process holding the last reference to a file a server
    /// serves waits in `exits` for `Rclunk`, and its parent's `await`
    /// returns after that, not before.
    #[test]
    fn exits_waits_for_its_closes_before_the_parent_hears() {
        let mut k = booted();
        let (server, end, fd) = mounted(&mut k);
        let Ret::Pid(child) = k.syscall(1, Call::Rfork { flags: rf::PROC | rf::FDG }).unwrap() else { panic!() };
        assert_eq!(k.syscall(1, Call::Close { fd }), Ok(Ret::Ok), "the child still holds it");
        assert_eq!(k.syscall(child, Call::Exits { status: String::new() }), Ok(Ret::Sched), "waits for Rclunk");
        assert_eq!(k.syscall(1, Call::Await), Ok(Ret::Sched), "nothing to reap yet");
        assert_eq!(answer(&mut k, server, end)[4], ninep::T::Clunk as u8);
        assert_eq!(k.resume(child), Ok(Ret::Ok));
        assert_eq!(k.procs.borrow().state(child), proc::State::Moribund);
        let Ok(Ret::Str(w)) = k.resume(1) else { panic!("no wait record") };
        assert!(w.starts_with(&format!("{child} ")), "{w}");
    }

    /// `forceclosefgrp` (`pgrp.c:245`): a process killed while it closes
    /// waits for nothing — what it has not closed goes to the close queue,
    /// and its parent hears at once.
    #[test]
    fn a_process_killed_in_exits_hands_its_closes_to_the_queue() {
        let mut k = booted();
        let (_server, _end, fd) = mounted(&mut k);
        let Ret::Pid(child) = k.syscall(1, Call::Rfork { flags: rf::PROC | rf::FDG }).unwrap() else { panic!() };
        k.syscall(1, Call::Close { fd }).unwrap();
        k.procs.borrow_mut().get_mut(child).unwrap().procctl = Some(proc::Procctl::Exitme);
        assert_eq!(k.syscall(child, Call::Exits { status: "Killed".into() }), Ok(Ret::Ok), "no wait");
        assert_eq!(k.procs.borrow().state(child), proc::State::Moribund);
        assert_eq!(k.procs.borrow().clunkq.len(), 1, "the close queue has it");
    }

    /// `closeproc`'s close waits as any last close does (`chan.c:575`), and
    /// then closes what is queued after it: the queue is drained in order,
    /// each clunk answered before the next is sent.
    #[test]
    fn closeproc_waits_for_each_close_it_makes() {
        let mut k = booted();
        let (server, end, fd) = mounted(&mut k);
        let Ret::Pid(child) = k.syscall(1, Call::Rfork { flags: rf::PROC | rf::FDG }).unwrap() else { panic!() };
        k.syscall(1, Call::Close { fd }).unwrap();
        k.procs.borrow_mut().get_mut(child).unwrap().procctl = Some(proc::Procctl::Exitme);
        k.syscall(child, Call::Exits { status: "Killed".into() }).unwrap();
        let cp = k.kproc("closeproc", closeproc);
        // as `schedinit` enters a process: `up` is it
        k.up.borrow_mut().pid = cp;
        assert_eq!(k.runkproc(cp), machine::Left::Sched, "asleep for Rclunk");
        assert!(k.procs.borrow().clunkq.is_empty(), "taken off the queue");
        assert_eq!(answer(&mut k, server, end)[4], ninep::T::Clunk as u8);
        assert_eq!(k.procs.borrow().state(cp), proc::State::Ready, "the answer woke it");
        k.up.borrow_mut().pid = cp;
        assert_eq!(k.runkproc(cp), machine::Left::Sched, "and it waits for more");
        assert_eq!(k.procs.borrow().state(cp), proc::State::Wakeme);
    }

    /// `exec` closes the close-on-exec descriptors past its point of no
    /// return (`sysproc.c:591`), and each close waits as any last close
    /// does: the call ends when the server has let the file go.
    #[test]
    fn exec_waits_for_its_close_on_exec_descriptors() {
        let mut k = booted();
        let (server, end, _fd) = mounted(&mut k);
        let mode = chan::mode::OCEXEC as i32;
        let Ok(Ret::Fd(cx)) = served(&mut k, 1, server, end, Call::Open { path: "/answer".into(), mode }) else {
            panic!()
        };
        // `/` is the mount before the root, so the walk asks the server too;
        // the last thing the call waits for is the clunk.
        let exec = Call::Exec { path: "/init".into(), args: vec!["init".into()] };
        let (r, ts) = served_t(&mut k, 1, server, end, exec);
        assert_eq!(r, Ok(Ret::Ok));
        assert_eq!(ts.last(), Some(&(ninep::T::Clunk as u8)), "the call ended once the server let the file go");
        assert_eq!(k.syscall(1, Call::Pread { fd: cx, n: 1, off: 0 }), Err(EBADFD.into()), "closed on exec");
    }

    /// [`served`], and the type of every request the server answered.
    fn served_t(k: &mut Kernel, pid: Pid, server: Pid, fd: Fd, call: Call) -> (Result<Ret, String>, Vec<u8>) {
        let mut r = k.syscall(pid, call);
        let mut ts = Vec::new();
        while r == Ok(Ret::Sched) {
            ts.push(answer(k, server, fd)[4]);
            r = k.resume(pid);
        }
        (r, ts)
    }

    fn clunks(ts: &[u8]) -> usize {
        ts.iter().filter(|&&t| t == ninep::T::Clunk as u8).count()
    }

    /// **`pexit` closes `dot` and the namespace** (*"cclose(dot)"*,
    /// `closepgrp`, `proc.c:1166`): a file bound into a namespace of the
    /// process's own is the namespace's to close, and the server is told
    /// before the parent hears.
    #[test]
    fn exits_closes_the_namespace_it_held_last() {
        let mut k = booted();
        let (server, end, fd) = mounted(&mut k);
        assert_eq!(served_t(&mut k, 1, server, end, Call::Close { fd }).0, Ok(Ret::Ok));
        let flags = rf::PROC | rf::NAMEG | rf::FDG;
        let Ret::Pid(child) = k.syscall(1, Call::Rfork { flags }).unwrap() else { panic!() };
        let bind = Call::Bind { name: "/answer".into(), old: "/init".into(), flag: 0 };
        let (r, _) = served_t(&mut k, child, server, end, bind);
        assert!(matches!(r, Ok(Ret::N(_))), "{r:?}");
        let (r, ts) = served_t(&mut k, child, server, end, Call::Exits { status: String::new() });
        assert_eq!(r, Ok(Ret::Ok));
        assert_eq!(clunks(&ts), 1, "the bound file, which only the child's namespace held");
        let Ok(Ret::Str(w)) = k.syscall(1, Call::Await) else { panic!("no wait record") };
        assert!(w.starts_with(&format!("{child} ")), "{w}");
    }

    /// `syschdir`'s *"cclose(up->dot)"* (`sysfile.c:983`): the directory
    /// left is closed if this was its last reference.
    #[test]
    fn chdir_closes_the_directory_it_leaves() {
        let mut k = booted();
        let (server, end, _fd) = mounted(&mut k);
        let (r, ts) = served_t(&mut k, 1, server, end, Call::Chdir { path: "/sub".into() });
        assert_eq!((r, clunks(&ts)), (Ok(Ret::Ok), 0));
        let (r, ts) = served_t(&mut k, 1, server, end, Call::Chdir { path: "/".into() });
        assert_eq!((r, clunks(&ts)), (Ok(Ret::Ok), 1), "the server's directory let go");
    }

    /// `cmount`'s *"mountfree(m->mount)"* (`chan.c:739`): an `MREPL` bind
    /// closes what it replaces.
    #[test]
    fn a_bind_that_replaces_closes_what_was_there() {
        let mut k = booted();
        let (server, end, _fd) = mounted(&mut k);
        let bind = |name: &str| Call::Bind { name: name.into(), old: "/init".into(), flag: 0 };
        let (r, ts) = served_t(&mut k, 1, server, end, bind("/answer"));
        assert!(matches!(r, Ok(Ret::N(_))));
        let before = clunks(&ts);
        let (r, ts) = served_t(&mut k, 1, server, end, bind("/hello"));
        assert!(matches!(r, Ok(Ret::N(_))));
        assert_eq!(clunks(&ts), before + 1, "the file the first bind put there");
    }

    /// `cunmount` (`chan.c:764`): with nothing mounted it is `Eunmount`,
    /// with no such element `Eunion`, and without a name the whole mount
    /// point goes — its channels closed, the server's root among them.
    #[test]
    fn unmount_refuses_what_is_not_mounted_and_closes_what_it_takes() {
        let mut k = booted();
        let (server, end, fd) = mounted(&mut k);
        let (r, _) = served_t(&mut k, 1, server, end, Call::Unmount { name: None, old: "/dev".into() });
        assert_eq!(r, Err("not mounted".into()));
        let un = Call::Unmount { name: Some("/hello".into()), old: "/".into() };
        let (r, _) = served_t(&mut k, 1, server, end, un);
        assert_eq!(r, Err("not in union".into()));
        let (r, _) = served_t(&mut k, 1, server, end, Call::Close { fd });
        assert_eq!(r, Ok(Ret::Ok));
        let (r, ts) = served_t(&mut k, 1, server, end, Call::Unmount { name: None, old: "/".into() });
        assert_eq!((r, clunks(&ts)), (Ok(Ret::Ok), 1), "the mount's root");
        assert!(k.syscall(1, Call::Open { path: "/answer".into(), mode: 0 }).is_err());
    }

    /// `rfork` without `RFPROC` closes the tables it replaces
    /// (`closefgrp(ofg)`, `closepgrp(opg)`, `sysproc.c:61`, `:70`) — what
    /// they held last, and nothing another process still shares.
    #[test]
    fn rfork_without_a_child_closes_the_tables_it_replaces() {
        let mut k = booted();
        let (server, end, _fd) = mounted(&mut k);
        // the server shares pid 1's tables: copies of its own first, then a
        // file open only in the copy, and one bound only into it
        let (r, _) = served_t(&mut k, 1, server, end, Call::Rfork { flags: rf::FDG | rf::NAMEG });
        assert_eq!(r, Ok(Ret::Pid(0)));
        let (r, _) = served_t(&mut k, 1, server, end, Call::Open { path: "/answer".into(), mode: 0 });
        assert!(matches!(r, Ok(Ret::Fd(_))), "{r:?}");
        let bind = Call::Bind { name: "/answer".into(), old: "/init".into(), flag: 0 };
        let (r, _) = served_t(&mut k, 1, server, end, bind);
        assert!(matches!(r, Ok(Ret::N(_))), "{r:?}");
        let (r, ts) = served_t(&mut k, 1, server, end, Call::Rfork { flags: rf::CFDG });
        assert_eq!((r, clunks(&ts)), (Ok(Ret::Pid(0)), 1), "the file open only in the table replaced");
        let (r, ts) = served_t(&mut k, 1, server, end, Call::Rfork { flags: rf::CNAMEG });
        assert_eq!((r, clunks(&ts)), (Ok(Ret::Pid(0)), 1), "the file bound only into the namespace replaced");
    }

    /// `chanfree`'s *"cclose(c->umc)"* (`chan.c:467`): a union directory
    /// closed in the middle of a read closes the element it had open.
    #[test]
    fn closing_a_union_mid_read_closes_the_element_it_had_open() {
        let mut k = booted();
        let (server, end, _fd) = mounted(&mut k);
        let (r, _) = served_t(&mut k, 1, server, end, Call::Open { path: "/".into(), mode: 0 });
        let Ok(Ret::Fd(dir)) = r else { panic!("{r:?}") };
        let (r, _) = served_t(&mut k, 1, server, end, Call::Pread { fd: dir, n: 512, off: -1 });
        assert!(matches!(r, Ok(Ret::Data(ref d)) if !d.is_empty()), "{r:?}");
        let (r, ts) = served_t(&mut k, 1, server, end, Call::Close { fd: dir });
        assert_eq!((r, clunks(&ts)), (Ok(Ret::Ok), 2), "the directory, and the element read");
    }

    /// `bindmount`'s first check (`sysfile.c:1000`): a flag outside `MMASK`,
    /// or both `MBEFORE` and `MAFTER`, is `Ebadarg`. And `bind` answers the
    /// mount's id, as `mount` does (`:1054`).
    #[test]
    fn bind_checks_its_flag_and_answers_the_mounts_id() {
        let mut k = booted();
        for flag in [3, 0x100] {
            let r = k.syscall(1, Call::Bind { name: "/hello".into(), old: "/init".into(), flag });
            assert_eq!(r, Err(proc::Procs::EBADARG.into()), "{flag:#x}");
        }
        let id = |r| match r {
            Ok(Ret::N(id)) => id,
            r => panic!("{r:?}"),
        };
        let a = id(k.syscall(1, Call::Bind { name: "/hello".into(), old: "/init".into(), flag: 0 }));
        let b = id(k.syscall(1, Call::Bind { name: "/hello".into(), old: "/init".into(), flag: 0 }));
        assert!(b > a, "{a} then {b}");
    }

    /// *"if(up->pgrp->noattach) error(Enoattach)"* (`sysfile.c:1011`):
    /// `RFNOMNT`'s sandbox refuses `mount`, before any descriptor is looked
    /// at.
    #[test]
    fn a_namespace_without_attach_refuses_mount() {
        let mut k = booted();
        k.syscall(1, Call::Rfork { flags: rf::NOMNT }).unwrap();
        let r = k.syscall(1, Call::Mount { fd: 99, afd: -1, old: "/".into(), flag: 0, aname: String::new() });
        assert_eq!(r, Err(namec::ENOATTACH.into()));
    }

    /// `namec`'s *"aname = validnamedup(aname, 1)"* (`chan.c:1330`): a
    /// control character is `Ebadchar`, with the name quoted.
    #[test]
    fn a_name_with_a_control_character_is_refused() {
        let mut k = booted();
        let r = k.syscall(1, Call::Open { path: "/i\u{1}nit".into(), mode: 0 });
        assert_eq!(r, Err("bad character in file name: '/i\u{1}nit'".into()));
    }

    /// **Kernel processes share one namespace group** (`kpgrp`,
    /// `proc.c:1469`) and have no descriptor table.
    #[test]
    fn kernel_processes_share_a_namespace_group() {
        let mut k = booted();
        let a = k.kproc("one", alarmkproc);
        let b = k.kproc("two", alarmkproc);
        let procs = k.procs.borrow();
        assert_eq!(procs.pgrpid(a), procs.pgrpid(b));
        assert_ne!(procs.pgrpid(a), procs.pgrpid(1));
        assert!(procs.get(a).unwrap().fds.is_none());
    }

    /// `sysremove` and `wstat` (`sysfile.c:1151`, `:1181`): a mount point
    /// is not removed, nor renamed — *"which should be removed: the mount
    /// point or the mounted Chan?"* — and a rename says which name.
    #[test]
    fn a_mount_point_is_neither_removed_nor_renamed() {
        let mut k = booted();
        k.syscall(1, Call::Bind { name: "/hello".into(), old: "/init".into(), flag: 0 }).unwrap();
        assert_eq!(k.syscall(1, Call::Remove { path: "/init".into() }), Err(namec::EISMTPT.into()));
        let rename = ninep::Dir { name: "other".into(), mode: !0, ..Default::default() }.conv_d2m();
        assert_eq!(
            k.syscall(1, Call::Wstat { path: "/init".into(), edir: rename }),
            Err("'/init' is a mount point".into())
        );
    }

    /// **A mount's wire is let go with its last channel** — `chanfree`'s
    /// *"cclose(c->mchan)"* (`chan.c:475`) — and with nothing else holding
    /// it, a server on a pipe reads end of file. Until then it is held,
    /// though `mount` closed the descriptor that named it: the unmount
    /// below is answered over it.
    #[test]
    fn a_wire_is_let_go_with_the_last_channel_through_its_mount() {
        let mut k = booted();
        let (server, end, fd) = mounted(&mut k);
        assert_eq!(served_t(&mut k, 1, server, end, Call::Close { fd }).0, Ok(Ret::Ok));
        let (r, ts) = served_t(&mut k, 1, server, end, Call::Unmount { name: None, old: "/".into() });
        assert_eq!((r, clunks(&ts)), (Ok(Ret::Ok), 1), "the mount's root, its last");
        assert_eq!(
            k.syscall(server, Call::Pread { fd: end, n: 1, off: -1 }),
            Ok(Ret::Data(Vec::new())),
            "the server reads end of file"
        );
    }

    /// A kill while `exits` waits on a close lets that close finish —
    /// `forceclosefgrp` hands the queue only what is still in the table
    /// (`pgrp.c:245`), and the close in progress was taken out first
    /// (`:225`) — and queues the rest. It queued the one in progress too:
    /// a second clunk of a fid already clunked.
    #[test]
    fn a_kill_in_exits_queues_only_the_closes_not_begun() {
        let mut k = booted();
        let (server, end, fd) = mounted(&mut k);
        let Ret::Pid(child) = k.syscall(1, Call::Rfork { flags: rf::PROC | rf::FDG }).unwrap() else { panic!() };
        let (r, _) = served_t(&mut k, child, server, end, Call::Open { path: "/answer".into(), mode: 0 });
        assert!(matches!(r, Ok(Ret::Fd(_))), "{r:?}");
        assert_eq!(k.syscall(1, Call::Close { fd }), Ok(Ret::Ok), "the child holds it");
        assert_eq!(k.syscall(child, Call::Exits { status: String::new() }), Ok(Ret::Sched), "waits for Rclunk");
        k.procs.borrow_mut().get_mut(child).unwrap().procctl = Some(proc::Procctl::Exitme);
        assert_eq!(answer(&mut k, server, end)[4], ninep::T::Clunk as u8);
        assert_eq!(k.resume(child), Ok(Ret::Ok));
        assert_eq!(k.procs.borrow().clunkq.len(), 1, "only the close not begun");
        assert_eq!(k.procs.borrow().state(child), proc::State::Moribund);
    }

    /// `namec`'s errors name the name as far as the element they concern
    /// (`chan.c:1406`): the one not found, the file that is not a
    /// directory, the whole name for what follows the walk — and a name
    /// ending in `/` must be a directory (`:1450`).
    #[test]
    fn an_error_names_the_name_as_far_as_it_went() {
        let mut k = booted();
        let open = |k: &mut Kernel, p: &str| k.syscall(1, Call::Open { path: p.into(), mode: 0 });
        // the first name a device cannot walk is its `Enonexist`; one
        // further along a walk of several is `walk`'s own *"does not
        // exist"* (`chan.c:963`)
        assert_eq!(open(&mut k, "/nothing"), Err("'/nothing' file does not exist".into()));
        assert_eq!(open(&mut k, "/dev/nothing/x"), Err("'/dev/nothing' does not exist".into()));
        assert_eq!(open(&mut k, "/init/x"), Err("'/init' not a directory".into()));
        assert_eq!(open(&mut k, "/init/"), Err("'/init/' not a directory".into()));
        assert_eq!(open(&mut k, "#Q"), Err("unknown device in # filename".into()), "before the walk: as it is");
    }

    /// `parsename` keeps every byte of an element but `/` (`chan.c:1196`):
    /// `x.` is a name, and creating it creates it — the trailing dots were
    /// trimmed, so `x.` made `x` — and `OEXCL` on one that exists is
    /// `Eexist`, without asking the device (`:1550`).
    #[test]
    fn a_create_makes_the_name_it_is_given() {
        let mut k = booted();
        fd(k.syscall(1, Call::Create { path: "#e/x.".into(), mode: 1, perm: 0o666 }));
        assert!(k.syscall(1, Call::Stat { path: "#e/x.".into() }).is_ok());
        assert!(k.syscall(1, Call::Stat { path: "#e/x".into() }).is_err());
        let excl = chan::mode::OWRITE as i32 | chan::mode::OEXCL as i32;
        assert_eq!(
            k.syscall(1, Call::Create { path: "#e/x.".into(), mode: excl, perm: 0o666 }),
            Err("'#e/x.' file already exists".into())
        );
    }

    /// The next `n` requests a server has been sent — one a read, as a
    /// pipe gives back each write whole.
    fn requests(k: &mut Kernel, server: Pid, fd: Fd, n: usize) -> Vec<Vec<u8>> {
        (0..n)
            .map(|_| match k.syscall(server, Call::Pread { fd, n: 8192, off: -1 }) {
                Ok(Ret::Data(b)) => b,
                r => panic!("nothing sent: {r:?}"),
            })
            .collect()
    }

    /// **An interrupted RPC is flushed** — `mountio`'s *"r =
    /// mntflushalloc(r, m->msize)"* (`devmnt.c:782`): a `Tflush` naming its
    /// tag, and the process waits for the answer to that. `Rflush` alone
    /// is *"error(Eintr)"* (`mountrpc`); the server has let the read go,
    /// and a later answer to it is nobody's.
    #[test]
    fn an_interrupted_rpc_is_flushed_and_waits_for_the_flush() {
        let mut k = booted();
        let (server, end, fd) = mounted(&mut k);
        assert_eq!(k.syscall(1, Call::Pread { fd, n: 64, off: 0 }), Ok(Ret::Sched), "the read goes out");
        assert!(k.procs.borrow_mut().postnote(1, "interrupt", proc::NoteFlag::NUser));
        assert_eq!(k.resume(1), Ok(Ret::Sched), "flushed, and waiting for Rflush");
        let sent = requests(&mut k, server, end, 2);
        let (read, flush) = (ninep::unframe(&sent[0]).unwrap(), ninep::unframe(&sent[1]).unwrap());
        assert_eq!((read.ty, flush.ty), (ninep::T::Read as u8, ninep::T::Flush as u8));
        assert_eq!(u16::from_le_bytes([flush.body[0], flush.body[1]]), read.tag, "oldtag is the read's");
        let rflush = ninep::W::new().frame(ninep::T::Flush.reply(), flush.tag);
        k.syscall(server, Call::Pwrite { fd: end, data: rflush, off: -1 }).unwrap();
        assert_eq!(k.resume(1), Err(proc::EINTR.into()));
    }

    /// A reply that comes before its flush's is the RPC's answer: the flush
    /// is still waited for, and `mntflushfree` finds the RPC done
    /// (`devmnt.c:1004`).
    #[test]
    fn a_reply_before_the_flush_is_the_answer() {
        let mut k = booted();
        let (server, end, fd) = mounted(&mut k);
        assert_eq!(k.syscall(1, Call::Pread { fd, n: 64, off: 0 }), Ok(Ret::Sched));
        assert!(k.procs.borrow_mut().postnote(1, "interrupt", proc::NoteFlag::NUser));
        assert_eq!(k.resume(1), Ok(Ret::Sched));
        let sent = requests(&mut k, server, end, 2);
        let flush = ninep::unframe(&sent[1]).unwrap();
        let mut answers = serve9p(&sent[0]);
        answers.extend(ninep::W::new().frame(ninep::T::Flush.reply(), flush.tag));
        k.syscall(server, Call::Pwrite { fd: end, data: answers, off: -1 }).unwrap();
        assert_eq!(k.resume(1), Ok(Ret::Data(b"served over 9P".to_vec())));
    }

    /// `walk` sends up to `MAXWELEM` names in one `Twalk` (`chan.c:1006`),
    /// where this sent one a name: `/sub/answer` through a mount is one
    /// walk of two names, then the open.
    #[test]
    fn a_walk_through_a_mount_is_one_twalk_of_its_names() {
        let mut k = booted();
        let (server, end, _fd) = mounted(&mut k);
        let mut r = k.syscall(1, Call::Open { path: "/sub/answer".into(), mode: 0 });
        let mut walks = Vec::new();
        while r == Ok(Ret::Sched) {
            let req = answer(&mut k, server, end);
            if req[4] == ninep::T::Walk as u8 {
                walks.push(u16::from_le_bytes([req[15], req[16]]));
            }
            r = k.resume(1);
        }
        assert!(matches!(r, Ok(Ret::Fd(_))), "{r:?}");
        assert_eq!(walks, vec![2], "one Twalk, of two names");
    }

    /// `write` takes its range before the device writes (`sysfile.c:744`):
    /// two processes writing through one descriptor, the first still
    /// waiting on the server, write at 0 and at 4 — not both at 0 — and
    /// the descriptor ends past both.
    #[test]
    fn writers_on_one_descriptor_take_their_own_ranges() {
        let mut k = booted();
        let (server, end, _fd) = mounted(&mut k);
        let w = fd_(served(&mut k, 1, server, end, Call::Open { path: "/answer".into(), mode: 1 }));
        let Ret::Pid(child) = k.syscall(1, Call::Rfork { flags: rf::PROC }).unwrap() else { panic!() };
        assert_eq!(k.syscall(1, Call::Pwrite { fd: w, data: b"aaaa".to_vec(), off: -1 }), Ok(Ret::Sched));
        assert_eq!(k.syscall(child, Call::Pwrite { fd: w, data: b"bb".to_vec(), off: -1 }), Ok(Ret::Sched));
        let sent = requests(&mut k, server, end, 2);
        let offset = |m: &[u8]| u64::from_le_bytes(m[11..19].try_into().unwrap());
        assert_eq!((offset(&sent[0]), offset(&sent[1])), (0, 4));
        for m in &sent {
            k.syscall(server, Call::Pwrite { fd: end, data: serve9p(m), off: -1 }).unwrap();
        }
        assert_eq!(k.resume(1), Ok(Ret::N(4)));
        assert_eq!(k.resume(child), Ok(Ret::N(2)));
        assert_eq!(k.syscall(1, Call::Seek { fd: w, off: 0, whence: 1 }), Ok(Ret::N(6)));
    }

    /// `read`'s checks (`sysfile.c:657`, `:675`): an offset below 0 but
    /// `~0` is `Enegoff`, and a directory is read only where its channel
    /// is.
    #[test]
    fn a_read_refuses_a_negative_offset_and_a_seek_in_a_directory() {
        let mut k = booted();
        let f = fd(k.syscall(1, Call::Open { path: "/init".into(), mode: 0 }));
        assert_eq!(k.syscall(1, Call::Pread { fd: f, n: 4, off: -2 }), Err(ENEGOFF.into()));
        let d = fd(k.syscall(1, Call::Open { path: "/root".into(), mode: 0 }));
        assert_eq!(k.syscall(1, Call::Pread { fd: d, n: 512, off: 5 }), Err(EDIRSEEK.into()));
        assert!(matches!(k.syscall(1, Call::Pread { fd: d, n: 512, off: 0 }), Ok(Ret::Data(_))));
    }
}
