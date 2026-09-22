//! Processes, and the three things a process owns that `rfork` can share,
//! copy or clear: its namespace, its file descriptors, and its environment.
//!
//! This is the kernel's job and very nearly the whole of it. Everything a
//! system does beyond orchestrating processes is done BY processes, talking to
//! each other; so what is here is Plan 9's `rfork`, `exec`, `exits` and
//! `await`, and the tables they act on.

use crate::chan::Chan;
use crate::ns::Ns;
use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

pub type Pid = u32;

/// `ERRMAX` — *"max length of error string"* (`libc.h:553`). `sysexits`
/// copies a status through `char buf[ERRMAX]` (`sysproc.c:668`), so a status
/// is bounded by the same thing an error string is.
pub const ERRMAX: usize = 128;

/// `NFD` — *"per process file descriptors"* (`portdat.h:476`).
pub const NFD: usize = 100;

/// `Proc.time`'s slots (`portdat.h:630`).
pub const TUSER: usize = 0;
pub const TSYS: usize = 1;
pub const TREAL: usize = 2;
pub const TCUSER: usize = 3;
pub type Fd = i32;

/// `rfork(2)`'s flags, with Plan 9's values. They are bits and they compose,
/// and the pattern repeats three times: a `G` bit means "give me my own copy",
/// a `CG` bit means "give me an empty one", and neither means "share".
pub mod rf {
    pub const NAMEG: i32 = 1 << 0;
    pub const ENVG: i32 = 1 << 1;
    pub const FDG: i32 = 1 << 2;
    pub const NOTEG: i32 = 1 << 3;
    pub const PROC: i32 = 1 << 4;
    pub const MEM: i32 = 1 << 5;
    pub const NOWAIT: i32 = 1 << 6;
    pub const CNAMEG: i32 = 1 << 10;
    pub const CENVG: i32 = 1 << 11;
    pub const CFDG: i32 = 1 << 12;
    /// `RFREND` — a new rendezvous group.
    pub const REND: i32 = 1 << 13;
    /// `RFNOMNT` — sandboxing: this namespace may attach no device but
    /// `#| #d #e #c #p` (`chan.c:1376`). Once set it is never unset, and a
    /// copied namespace carries it (`sysproc.c:38`).
    pub const NOMNT: i32 = 1 << 14;
}

/// A process's file descriptors. Shared or copied per `rfork`, which is why it
/// sits behind a reference rather than inside `Proc`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fds {
    slots: Vec<Option<Rc<RefCell<Chan>>>>,
}

impl Fds {
    /// `newfd`. `NFD` bounds the table, so a process that leaks descriptors
    /// fails rather than growing without limit.
    pub fn add(&mut self, c: Chan) -> Fd {
        if self.count() >= NFD {
            return -1;
        }
        let c = Rc::new(RefCell::new(c));
        for (i, s) in self.slots.iter_mut().enumerate() {
            if s.is_none() {
                *s = Some(c);
                return i as Fd;
            }
        }
        self.slots.push(Some(c));
        (self.slots.len() - 1) as Fd
    }
    pub fn get(&self, fd: Fd) -> Option<&Rc<RefCell<Chan>>> {
        self.slots.get(fd as usize).and_then(|s| s.as_ref())
    }
    /// `fdclose` (`sysfile.c:285`): take the descriptor out of the table and
    /// `cclose` the channel.
    ///
    /// **The channel is answered when the LAST reference to it goes**, which
    /// is `cclose`'s `if(decref(c)) return;` (`chan.c:496`). A dup shares the
    /// channel, so closing one name for it must not tell the device the file
    /// is closed — `Rc` is that reference count, and a count of one means this
    /// was the last.
    pub fn close(&mut self, fd: Fd) -> Option<Option<Chan>> {
        let taken = match self.slots.get_mut(fd as usize) {
            Some(s) if s.is_some() => s.take()?,
            _ => return None,
        };
        // `decref(c)`: what is left when this handle is dropped.
        Some(match Rc::try_unwrap(taken) {
            Ok(cell) => Some(cell.into_inner()),
            Err(_) => None,
        })
    }
    /// `dup(2)`: to a given slot, or to the lowest free one when `new` is -1.
    pub fn dup(&mut self, old: Fd, new: Fd) -> Option<Fd> {
        let c = self.get(old)?.clone();
        if new < 0 {
            for (i, s) in self.slots.iter_mut().enumerate() {
                if s.is_none() {
                    *s = Some(c);
                    return Some(i as Fd);
                }
            }
            self.slots.push(Some(c));
            return Some((self.slots.len() - 1) as Fd);
        }
        while self.slots.len() <= new as usize {
            self.slots.push(None);
        }
        self.slots[new as usize] = Some(c);
        Some(new)
    }
    pub fn count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    /// How far the table reaches — `fgrp->maxfd`, which `dupgen` walks to.
    pub fn slots(&self) -> Fd {
        self.slots.len() as Fd
    }
}

/// A process.
/// `Waitmsg` (`portdat.h:645`) — what a reaped child leaves behind. Plan 9's
/// kernel fills one in `pexit` and `sysawait` formats it; the shape is the
/// same one `wait(2)` parses back out (`9sys/wait.c`).
pub struct Waitmsg {
    pub pid: Pid,
    /// user, sys and real, in milliseconds.
    pub time: [u64; 3],
    pub msg: String,
}

impl Waitmsg {
    /// `sysawait` (`sysproc.c:727`), verbatim: `"%d %lud %lud %lud %q"`.
    pub fn format(&self) -> String {
        format!(
            "{} {} {} {} {}",
            self.pid,
            self.time[0],
            self.time[1],
            self.time[2],
            quote(&self.msg)
        )
    }
}

/// `%q` — Plan 9's quoted string (`fmt/fmtquote.c:_quotesetup`). A string is
/// quoted when it is empty or holds any rune `<= ' '` or a quote; inside the
/// quotes a quote doubles. `wait(2)` undoes it with `tokenize`, so getting
/// this wrong loses every status containing a space.
pub fn quote(s: &str) -> String {
    let needs = s.is_empty() || s.chars().any(|c| c <= ' ' || c == '\'');
    if !needs {
        return s.to_string();
    }
    let mut out = String::from("'");
    for c in s.chars() {
        if c == '\'' {
            out.push('\'');
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// **The process states** (`portdat.h:610`), and they are Plan 9's twelve.
/// `/proc/<n>/status` reports one, and `sched` acts on it: a process is put
/// on the run queue when it is `Ready`, entered when it becomes `Running`,
/// and left alone while it is `Wakeme`.
///
/// `Dead` is zero there, and is the state of a slot with no process in it;
/// this table has no empty slots, so it is here for the numbering and for
/// what `pexit` leaves behind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Dead,
    Moribund,
    Ready,
    Scheding,
    Running,
    Queueing,
    QueueingR,
    QueueingW,
    Wakeme,
    Broken,
    Stopped,
    Rendezvous,
    Waitrelease,
}

impl State {
    /// `statename[]` (`proc.c:22`) — what `/proc/<n>/status` prints.
    pub fn name(&self) -> &'static str {
        match self {
            State::Dead => "Dead",
            State::Moribund => "Moribund",
            State::Ready => "Ready",
            State::Scheding => "Scheding",
            State::Running => "Running",
            State::Queueing => "Queueing",
            State::QueueingR => "QueueingR",
            State::QueueingW => "QueueingW",
            State::Wakeme => "Wakeme",
            State::Broken => "Broken",
            State::Stopped => "Stopped",
            State::Rendezvous => "Rendez",
            State::Waitrelease => "Waitrelease",
        }
    }
}

/// `struct Rendez` (`portdat.h:104`) — *"Lock; Proc *p;"*, and that is the
/// whole of it: **a place for one process to wait and another to find it**.
///
/// The `Lock` is absent because this kernel runs on one processor and never
/// in an interrupt: `sleep` and `wakeup` cannot interleave. Everything else
/// is Plan 9's, including the rule that two processes may not sleep on one
/// `Rendez` — `sleep` panics on it there (*"double sleep"*, `proc.c:826`)
/// and this says so too.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rendez {
    /// `r->p` — who is waiting, if anyone.
    pub p: Option<Pid>,
}

/// **Where a `Rendez` lives.** Plan 9's are fields on the things that own
/// them — `&up->sleep` is *"place for syssleep/debug"* (`portdat.h:720`) and
/// `&up->waitr` is *"Place to hang out in wait"* (`:683`) — and `sleep` takes
/// the address of one. A Rust kernel cannot pass that address around, so a
/// `Rendez` is named by whose it is and which it is, which is the same
/// information.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rid(pub Pid, pub Which);

/// Which of a process's two `Rendez` (`portdat.h:683`, `:720`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    /// `up->sleep` — `syssleep` waits here.
    Sleep,
    /// `up->waitr` — `pwait` waits here and `pexit` wakes it.
    Waitr,
}

/// The scheduler's own constants (`portdat.h:640`).
pub mod pri {
    /// `Npriq` — *"number of scheduler priority levels"*.
    pub const NPRIQ: usize = 20;
    /// `Nrq` — `Npriq+2`, the two above being edf's. Nothing here is edf,
    /// and the queues exist so the numbering does not drift from Plan 9's.
    pub const NRQ: usize = NPRIQ + 2;
    /// `PriNormal` — *"base priority for normal processes"*.
    pub const NORMAL: usize = 10;
    /// `PriKproc` and `PriRoot` — both 13 there.
    pub const KPROC: usize = 13;
}

#[derive(Clone)]
pub struct Proc {
    pub pid: Pid,
    pub ppid: Pid,
    /// The three shareable tables. `Rc` is the sharing: two processes holding
    /// the same `Rc` after `rfork` without the `G` bit is not a metaphor for
    /// sharing, it is the sharing.
    pub ns: Rc<RefCell<Ns>>,
    pub fds: Rc<RefCell<Fds>>,
    /// `up->egrp`. **Values are bytes**, as Plan 9's `Evalue` is — a `char*`
    /// with a length, not a C string — so a variable can hold anything.
    pub env: Rc<RefCell<HashMap<String, Vec<u8>>>>,
    /// `up->user` — Plan 9's whole identity field (`portdat.h:664`). One
    /// name. No uid, no gid, no euid/ruid pair.
    pub user: String,
    /// `Proc.time[6]` (`portdat.h:630`): user, sys, real, and the three
    /// aggregates for exited children. Milliseconds, as `/dev/cputime`
    /// reports them (`TK2MS`, `devcons.c:63`).
    pub time: [u64; 6],
    /// When this process started, in the host's nanoseconds, so `TReal` can
    /// be `now - start` the way Plan 9 computes it from `MACHP(0)->ticks`.
    ///
    /// `None` until something with a clock stamps it. Plan 9 always has one —
    /// `MACHP(0)->ticks` is machine-provided and available kernel-wide — and
    /// this kernel does not yet, because the clock reaches it only through
    /// `#c`'s host. An unstamped process reports `TReal` 0 rather than the
    /// whole of the epoch.
    pub started: Option<u64>,
    /// `up->errstr`. Set when a call fails, and taken by `errstr(2)`.
    pub errstr: String,
    /// `up->slash` and `up->dot`. Plan 9 holds both as CHANNELS, not as text —
    /// a name is resolved from a channel, so the process's idea of "/" and "."
    /// is a channel too. An earlier version here kept `cwd` as a String, which
    /// is the same mistake as keying the namespace by path.
    pub slash: Chan,
    pub dot: Chan,
    /// Set once the process has exited; `await` reports it and reaps.
    pub status: Option<String>,
    /// `RFNOWAIT`: the parent abandoned it, so no wait record is kept.
    pub waited: bool,
    /// `p->state` (`portdat.h:659`).
    pub state: State,
    /// `p->priority` — which run queue it goes on. `PriNormal` to start,
    /// as `newproc` leaves it (`proc.c:710`).
    pub priority: usize,
    /// `p->r` — *"rendezvous point slept on"* (`portdat.h:719`). Set by
    /// `sleep`, cleared by `wakeup`, and the pair is what `wakeup` checks
    /// before readying: *"if(p->state != Wakeme || p->r != r) panic"*.
    pub r: Option<Rid>,
    /// `up->sleep` (`portdat.h:720`) and `up->waitr` (`:683`), the two
    /// `Rendez` a process carries.
    pub sleep: Rendez,
    pub waitr: Rendez,
    /// `p->basepri` — what `reprioritize` may not exceed. `PriNormal` from
    /// `newproc` (`proc.c:710`).
    pub basepri: usize,
    /// `p->cpu` — the decaying average `updatecpu` keeps and
    /// `reprioritize` reads (`proc.c`).
    pub cpu: u32,
    /// `p->lastupdate` — when `cpu` was last decayed, in ticks.
    pub lastupdate: u64,
    /// `p->trend` (`portdat.h:733`) — *"Rendez\* trend"*, the one `twakeup`
    /// wakes when this process's timer fires. `tsleep` sets it and
    /// `twakeup` clears it.
    pub trend: Option<Rid>,
    /// `p->twhen` — when a `tsleep`'s timer fires, in the machine's
    /// nanoseconds. Plan 9 keeps a `Timer` on a per-machine list
    /// (`portclock.c`); one process at a time sleeping on a timer needs no
    /// list, and `timerintr`'s counterpart walks the table.
    pub twhen: Option<u64>,
}

impl Proc {
    fn root(pid: Pid, slash: Chan) -> Proc {
        Proc {
            pid,
            ppid: 0,
            // The first process is eve's — `kstrdup(&p->user, eve)`
            // (`pc/main.c:287`) — and **`eve` is the empty string at that
            // point** (`:285`). So pid 1 starts with no name, `iseve()`
            // compares two empty strings and is true, and `boot` can write
            // `#c/hostowner` once. It was `"eve"`, a constant, which is the
            // role's name rather than anybody's.
            user: String::new(),
            time: [0; 6],
            started: None,
            errstr: String::new(),
            ns: Rc::new(RefCell::new(Ns::new())),
            fds: Rc::new(RefCell::new(Fds::default())),
            env: Rc::new(RefCell::new(HashMap::new())),
            dot: slash.clone(),
            slash,
            status: None,
            waited: true,
            state: State::Running,
            priority: pri::NORMAL,
            r: None,
            trend: None,
            sleep: Rendez::default(),
            waitr: Rendez::default(),
            basepri: pri::NORMAL,
            cpu: 0,
            lastupdate: 0,
            twhen: None,
        }
    }
}

/// **`up`** — the calling process, as Plan 9's per-machine global names it.
///
/// Plan 9's devices reach `up` from anywhere: `up->fgrp` in `devdup`,
/// `up->egrp` in `devenv`, `up->user` in `devsrv`, `devmnt`, `devproc` and
/// `devcap`. Rust has no ambient mutable global, so the same pointer is
/// shared: the kernel sets `pid` before it dispatches, and a device reads
/// through it. Identical information, one indirection more.
pub struct Up {
    pub pid: Pid,
    pub procs: Rc<RefCell<Procs>>,
}

impl Up {
    /// `up->user`.
    pub fn user(&self) -> String {
        let p = self.procs.borrow();
        p.user(self.pid).unwrap_or_default()
    }

    /// `up->fgrp`.
    pub fn fgrp(&self) -> Option<Rc<RefCell<Fds>>> {
        self.procs.borrow().get(self.pid).map(|p| p.fds.clone())
    }

    /// `up->egrp`.
    pub fn egrp(&self) -> Option<Rc<RefCell<HashMap<String, Vec<u8>>>>> {
        self.procs.borrow().get(self.pid).map(|p| p.env.clone())
    }
}

/// The process table.
pub struct Procs {
    tab: HashMap<Pid, Proc>,
    next: Pid,
    /// `Schedq runq[Nrq]` (`proc.c:39`) — one queue per priority, and a
    /// process is on exactly one of them when it is `Ready`. Plan 9's
    /// `Schedq` is a head/tail list threaded through `p->rnext`; a queue of
    /// pids is the same list without the threading, which a Rust kernel has
    /// no way to do anyway.
    runq: Vec<Vec<Pid>>,
    /// `nrdy` (`proc.c:41`) — how many are on the queues.
    nrdy: usize,
    /// `m->readied` — *"group scheduling"* (`ready`, `proc.c:428`). The
    /// process just made ready runs next if nothing higher wants to, which
    /// is what makes a `wakeup` hand the processor over rather than merely
    /// queue somebody.
    readied: Option<Pid>,
    /// `MACHP(0)->ticks` — what `updatecpu` measures against. The machine
    /// has the clock; this is the count the scheduler sees.
    pub ticks: u64,
    /// `up` — **the process running now** (`portdat.h`, a global there and
    /// set by `sched()`: `up = p`). `ready` needs it for the one condition
    /// that makes `m->readied` mean anything, and `updatecpu` for the
    /// branch that decays towards 1000 rather than 0.
    pub up: Option<Pid>,
    /// How often `sleep` was called on a `Rendez` somebody else was already
    /// on. Plan 9 prints and dumps the stack (`proc.c:826`); this kernel has
    /// nowhere to print from, so it counts, and a test can read the count.
    pub doublesleep: u32,
    /// `MACHP(0)->load`. **Nothing computes a load average yet**, so it is
    /// zero and `reprioritize` returns `basepri` — which is exactly what
    /// Plan 9 does when load is zero (`proc.c`, first line).
    load: u32,
}

impl Procs {
    /// A fresh table with pid 1 in it. It takes the channel that is pid 1's
    /// root, because a process without one cannot resolve a name at all.
    pub fn new(slash: Chan) -> Self {
        let mut tab = HashMap::new();
        tab.insert(1, Proc::root(1, slash));
        Procs {
            tab,
            next: 2,
            runq: (0..pri::NRQ).map(|_| Vec::new()).collect(),
            nrdy: 0,
            readied: None,
            up: Some(1),
            doublesleep: 0,
            ticks: 0,
            load: 0,
        }
    }

    pub fn get(&self, pid: Pid) -> Option<&Proc> {
        self.tab.get(&pid)
    }
    pub fn get_mut(&mut self, pid: Pid) -> Option<&mut Proc> {
        self.tab.get_mut(&pid)
    }
    pub fn count(&self) -> usize {
        self.tab.len()
    }

    /// `rfork(2)`. With `RFPROC` a new process; without it, the flags act on
    /// the caller — which is how a process gives ITSELF a private namespace,
    /// and why `rfork` is one call rather than two.
    /// `Ebadarg` — Plan 9's error string, checked before anything is
    /// committed (`sysproc.c:43`, *"Check flags before we commit"*).
    pub const EBADARG: &'static str = "bad arg in system call";

    /// The flag checks `sysrfork` makes before it does anything
    /// (`sysproc.c:44–56`). A contradictory pair — copy AND clear the same
    /// table — is an error, not a silent choice between them. So is asking for
    /// `RFMEM` or `RFNOWAIT` without `RFPROC`, since there is no child for
    /// either to describe.
// ---- the scheduler (`port/proc.c`) ------------------------------------
    //
    // **The switch is not here.** `sched()` (`proc.c:119`) ends with
    // `gotolabel(&up->sched)`, which is `pc/l.s:992` — the architecture's,
    // like `touser`. What is here is everything above that line: who is
    // runnable, who runs next, and who is waiting for what.

    /// `queueproc` (`proc.c:348`): onto the tail of its priority's queue.
    fn queueproc(&mut self, pri: usize, pid: Pid) {
        if let Some(p) = self.tab.get_mut(&pid) {
            p.priority = pri;
        }
        self.runq[pri].push(pid);
        self.nrdy += 1;
    }

    /// `dequeueproc` (`proc.c:369`) — take a named process off the queue it
    /// is on. Plan 9 checks it is still there under the lock and gives up if
    /// it is not; here nothing can take it away in between.
    fn dequeueproc(&mut self, pri: usize, pid: Pid) -> bool {
        if let Some(i) = self.runq[pri].iter().position(|&q| q == pid) {
            self.runq[pri].remove(i);
            self.nrdy -= 1;
            return true;
        }
        false
    }

    /// `updatecpu` (`proc.c`) — the decaying average of how much processor a
    /// process has had. `D = schedgain*HZ*Scaling`, and the running process
    /// decays towards 1000 while every other decays towards 0.
    /// `running` is Plan 9's `p != up` (`updatecpu`), inverted.
    fn updatecpu(&mut self, pid: Pid, running: bool) {
        const SCHEDGAIN: u64 = 30;
        const HZ: u64 = 100;
        const SCALING: u64 = 2;
        let d = (SCHEDGAIN * HZ * SCALING) as u32;
        let t = (self.ticks * SCALING + SCALING / 2) as u32;
        let Some(p) = self.tab.get_mut(&pid) else { return };
        let n = t.saturating_sub(p.lastupdate as u32);
        p.lastupdate = t as u64;
        if n == 0 {
            return;
        }
        let n = n.min(d);
        let ocpu = p.cpu;
        p.cpu = if running {
            let x = 1000u32.saturating_sub(ocpu);
            1000 - (x * (d - n)) / d
        } else {
            (ocpu * (d - n)) / d
        };
    }

    /// `reprioritize` (`proc.c`). **Load zero is `basepri`**, which is the
    /// function's own first branch — and load is zero here because nothing
    /// computes a load average yet.
    fn reprioritize(&self, pid: Pid) -> usize {
        let Some(p) = self.tab.get(&pid) else { return pri::NORMAL };
        if self.load == 0 {
            return p.basepri;
        }
        let fairshare = (1000 * 1000) / self.load as usize;
        let n = (p.cpu as usize).max(1);
        ((fairshare + n / 2) / n).min(p.basepri)
    }

    /// `ready` (`proc.c:418`) — put a process on the run queue.
    ///
    /// `m->readied = p` is *"group scheduling"*: whoever was just made ready
    /// is the one `runproc` takes next unless something higher is waiting,
    /// which is how a `wakeup` hands the processor over.
    pub fn ready(&mut self, pid: Pid) {
        // *"if(up != p && …) m->readied = p; /* group scheduling */"*
        // (`proc.c:428`). **`up != p` is load-bearing**: a process readying
        // ITSELF on its way out of `sched` must not claim the slot, or it
        // takes back the processor it just gave up and whatever it readied
        // before never runs. A pipeline's first stage waits for the shell
        // that made it to exit.
        if self.up != Some(pid) {
            self.readied = Some(pid);
        }
        self.updatecpu(pid, self.up == Some(pid));
        let pri = self.reprioritize(pid);
        if let Some(p) = self.tab.get_mut(&pid) {
            p.state = State::Ready;
        }
        self.queueproc(pri, pid);
    }

    /// `runproc` (`proc.c:507`) — who runs next.
    ///
    /// **Cut to one processor**, which is most of it: Plan 9's loop is
    /// affinity (`p->mp`), wiring (`p->wired`) and load balancing across
    /// `MACHP(i)`, and there is one of everything here. What is left is its
    /// first branch — *"cooperative scheduling until the clock ticks"*, the
    /// `m->readied` process — and then the highest non-empty queue.
    ///
    /// `None` is `idlehands()`: nothing to run.
    pub fn runproc(&mut self) -> Option<Pid> {
        // *"cooperative scheduling until the clock ticks"*. The condition is
        // Plan 9's whole condition: the readied process is `Ready`, **and
        // the two top queues are empty** — `runq[Nrq-1].head == nil &&
        // runq[Nrq-2].head == nil`, which are edf's. Nothing else outranks
        // it, so a `wakeup` beats a higher priority and that is deliberate.
        let edf_idle = self.runq[pri::NRQ - 1].is_empty() && self.runq[pri::NRQ - 2].is_empty();
        if let Some(p) = self.readied.filter(|_| edf_idle) {
            if self.tab.get(&p).map(|q| q.state) == Some(State::Ready) {
                let pri = self.tab[&p].priority;
                if self.dequeueproc(pri, p) {
                    self.readied = None;
                    return Some(p);
                }
            }
        }
        self.readied = None;
        for pri in (0..pri::NRQ).rev() {
            if let Some(&p) = self.runq[pri].first() {
                self.dequeueproc(pri, p);
                return Some(p);
            }
        }
        None
    }

    /// `sleep(r, f, arg)` (`proc.c:815`), everything up to the switch.
    ///
    /// The condition is checked first — *"if condition happened or a note is
    /// pending, never mind"* — and only then is the process committed:
    /// `r->p = up`, `up->state = Wakeme`, `up->r = r`. **`true` means the
    /// caller must now leave**, which is `gotolabel(&m->sched)` and is the
    /// machine's.
    ///
    /// Plan 9 unlocks `r` and `up->rlock` on the line before it leaves; the
    /// Rust counterpart is that this returns, dropping every borrow, and the
    /// caller suspends after it.
    pub fn sleep(&mut self, pid: Pid, r: Rid, happened: bool) -> bool {
        // *"double sleep called from %#p"* (`proc.c:826`) — and Plan 9
        // **prints, dumps the stack, and carries on**: `r->p = up` happens
        // either way. This declined to sleep instead, which is a branch Plan
        // 9 does not have; a caller that double-sleeps has a bug, and hiding
        // it is not this function's business.
        if self.rendez(r).p.is_some_and(|q| q != pid) {
            self.doublesleep += 1;
        }
        if happened {
            self.rendez_mut(r).p = None;
            return false;
        }
        self.rendez_mut(r).p = Some(pid);
        if let Some(p) = self.tab.get_mut(&pid) {
            p.state = State::Wakeme;
            p.r = Some(r);
        }
        true
    }

    /// `wakeup(r)` (`proc.c:942`) — ready whoever is sleeping there.
    ///
    /// The check is Plan 9's and is a panic there: *"if(p->state != Wakeme
    /// || p->r != r) panic("wakeup: state")"*. It answers who was woken, as
    /// `wakeup` returns the `Proc*`.
    pub fn wakeup(&mut self, r: Rid) -> Option<Pid> {
        let p = self.rendez(r).p?;
        let ok = self.tab.get(&p).is_some_and(|q| q.state == State::Wakeme && q.r == Some(r));
        if !ok {
            return None;
        }
        self.rendez_mut(r).p = None;
        if let Some(q) = self.tab.get_mut(&p) {
            q.r = None;
            // *"if(up->tt) timerdel(up);"* on the way out of `tsleep`.
            q.trend = None;
            q.twhen = None;
        }
        self.ready(p);
        Some(p)
    }

    /// `tsleep` (`proc.c:910`) — `sleep`, with a timer that will `wakeup` for
    /// you. Plan 9 hangs a `Timer` on the machine's list (`timeradd`,
    /// `portclock.c`) whose function is `twakeup`; here the deadline is on
    /// the process and [`Procs::timerintr`] is what walks it, because one
    /// table is not a list worth keeping twice.
    pub fn tsleep(&mut self, pid: Pid, r: Rid, happened: bool, deadline: u64) -> bool {
        // `up->trend = r; up->tfn = fn; timeradd(up);` (`proc.c:910`) —
        // the timer is armed BEFORE the sleep, so a deadline already passed
        // is not lost.
        if let Some(p) = self.tab.get_mut(&pid) {
            p.trend = Some(r);
            p.twhen = Some(deadline);
        }
        let slept = self.sleep(pid, r, happened);
        if !slept {
            // *"if(up->tt) timerdel(up);"* — the condition happened, so the
            // timer goes.
            if let Some(p) = self.tab.get_mut(&pid) {
                p.trend = None;
                p.twhen = None;
            }
        }
        slept
    }

    /// `timerintr` (`portclock.c:172`) — **fire the timers that are due**.
    /// Plan 9 walks a per-machine sorted list, `timers[machno]`, and calls
    /// each due timer's function; for a `tsleep` that function is `twakeup`
    /// (`proc.c:897`), which is `wakeup(p->trend)`.
    ///
    /// The list is a walk of the table here, because one table is not a
    /// list worth keeping twice on a machine with one processor. `twhen` is
    /// Plan 9's own field: `Proc` embeds a `Timer` (`portdat.h:732`, *"For
    /// tsleep and real-time"*).
    ///
    /// **It was called `checkalarms`, which is a different function.**
    /// `checkalarms` (`alarm.c:47`) walks `alarms.head` and wakes `alarmr`
    /// so that `alarmkproc` can post notes for `procalarm` — nothing to do
    /// with `tsleep`. Borrowing the name made a claim about Plan 9 that was
    /// not true.
    pub fn timerintr(&mut self, now: u64) {
        let due: Vec<Pid> = self
            .tab
            .values()
            .filter(|p| p.state == State::Wakeme && p.twhen.is_some_and(|w| w <= now))
            .map(|p| p.pid)
            .collect();
        for pid in due {
            // `twakeup` (`proc.c:897`): `trend = p->trend; p->trend = 0;
            // if(trend) wakeup(trend);`
            if let Some(trend) = self.tab.get_mut(&pid).and_then(|p| p.trend.take()) {
                self.wakeup(trend);
            }
        }
    }

    /// `anyready()` (`proc.c:188`) — is anything on a run queue?
    pub fn anyready(&self) -> bool {
        self.nrdy > 0
    }

    /// `yield` (`proc.c:454`), everything but the `sched()`: *"pretend we
    /// just used 1/2 tick"*, so yielding does not look like idleness and
    /// raise the yielder's priority. The caller leaves after it.
    pub fn yield_(&mut self, pid: Pid) {
        const SCALING: u64 = 2;
        if let Some(p) = self.tab.get_mut(&pid) {
            p.lastupdate = p.lastupdate.saturating_sub(SCALING / 2);
        }
    }

    /// Is anything else runnable? `runproc` without taking it — what
    /// `sched` asks before it decides there is nobody and idles.
    pub fn runproc_peek(&self) -> Option<Pid> {
        self.readied
            .filter(|p| self.state(*p) == State::Ready)
            .or_else(|| (0..pri::NRQ).rev().find_map(|pri| self.runq[pri].first().copied()))
    }

    /// When the earliest sleeper is due, if anyone is — so a host with
    /// nothing to run knows how long to wait rather than spinning. Plan 9's
    /// `idlehands()` halts the processor and the next clock interrupt wakes
    /// it; this is the same question asked forwards.
    pub fn nextalarm(&self) -> Option<u64> {
        self.tab
            .values()
            .filter(|p| p.state == State::Wakeme)
            .filter_map(|p| p.twhen)
            .min()
    }

    fn rendez(&self, r: Rid) -> Rendez {
        let Rid(pid, which) = r;
        self.tab
            .get(&pid)
            .map(|p| match which {
                Which::Sleep => p.sleep,
                Which::Waitr => p.waitr,
            })
            .unwrap_or_default()
    }

    fn rendez_mut(&mut self, r: Rid) -> &mut Rendez {
        let Rid(pid, which) = r;
        let p = self.tab.get_mut(&pid).expect("no such process");
        match which {
            Which::Sleep => &mut p.sleep,
            Which::Waitr => &mut p.waitr,
        }
    }

    /// `p->state`, and the one place anything else sets it: `sched` marks
    /// the process it enters `Running` (`proc.c:160`).
    pub fn setstate(&mut self, pid: Pid, st: State) {
        if let Some(p) = self.tab.get_mut(&pid) {
            p.state = st;
        }
    }

    pub fn state(&self, pid: Pid) -> State {
        self.tab.get(&pid).map(|p| p.state).unwrap_or(State::Dead)
    }

    pub fn rforkcheck(flags: i32) -> Result<(), String> {
        for both in [rf::FDG | rf::CFDG, rf::NAMEG | rf::CNAMEG, rf::ENVG | rf::CENVG] {
            if flags & both == both {
                return Err(Self::EBADARG.into());
            }
        }
        if flags & rf::PROC == 0 && flags & (rf::MEM | rf::NOWAIT) != 0 {
            return Err(Self::EBADARG.into());
        }
        Ok(())
    }

    pub fn rfork(&mut self, pid: Pid, flags: i32) -> Option<Pid> {
        Self::rforkcheck(flags).ok()?;
        let parent = self.tab.get(&pid)?.clone();

        let ns = Self::table(&parent.ns, flags & rf::CNAMEG != 0, flags & rf::NAMEG != 0);
        // `sysproc.c:38` — a copied namespace carries `noattach`; `:42` —
        // `RFNOMNT` sets it. It is never cleared, which is what makes it a
        // sandbox rather than a mode.
        if flags & (rf::NAMEG | rf::CNAMEG) != 0 {
            let inherited = parent.ns.borrow().noattach();
            ns.borrow_mut().set_noattach(inherited);
        }
        if flags & rf::NOMNT != 0 {
            ns.borrow_mut().set_noattach(true);
        }
        let env = Self::table(&parent.env, flags & rf::CENVG != 0, flags & rf::ENVG != 0);
        let fds = Self::table(&parent.fds, flags & rf::CFDG != 0, flags & rf::FDG != 0);

        if flags & rf::PROC == 0 {
            let me = self.tab.get_mut(&pid)?;
            me.ns = ns;
            me.env = env;
            me.fds = fds;
            return None;
        }

        let child = Proc {
            pid: self.next,
            ppid: pid,
            user: parent.user.clone(),
            time: [0; 6],
            started: None,
            errstr: String::new(),
            ns,
            fds,
            env,
            slash: parent.slash.clone(),
            dot: parent.dot.clone(),
            status: None,
            waited: flags & rf::NOWAIT != 0,
            // `newproc` leaves a child `Scheding` (`proc.c:681`), not
            // `Ready`: it is `ready(p)` that makes it runnable, and
            // `sysrfork` calls it on the line before it scheds
            // (`sysproc.c`). A child born `Ready` is one nothing ever puts
            // on a queue.
            state: State::Scheding,
            priority: pri::NORMAL,
            r: None,
            trend: None,
            sleep: Rendez::default(),
            waitr: Rendez::default(),
            basepri: pri::NORMAL,
            cpu: 0,
            lastupdate: 0,
            twhen: None,
        };
        let cpid = child.pid;
        self.tab.insert(cpid, child);
        self.next += 1;
        Some(cpid)
    }

    /// Clear, copy or share — the rule all three tables follow.
    fn table<T: Clone + Default>(src: &Rc<RefCell<T>>, clear: bool, copy: bool) -> Rc<RefCell<T>> {
        if clear {
            Rc::new(RefCell::new(T::default()))
        } else if copy {
            Rc::new(RefCell::new(src.borrow().clone()))
        } else {
            src.clone()
        }
    }

    /// `chdir(2)`: `up->dot` is a CHANNEL, so this replaces it.
    pub fn chdir(&mut self, pid: Pid, dot: Chan) {
        if let Some(p) = self.tab.get_mut(&pid) {
            p.dot = dot;
        }
    }

    /// `errstr(2)`: the per-process error string. Plan 9 exchanges it —
    /// reading clears what was there — which is why a second `errstr` after a
    /// failure says nothing.
    /// `generrstr` (`sysproc.c:748`) — an EXCHANGE, not a read: the caller's
    /// buffer becomes `up->errstr` and the old one is answered. Both halves
    /// are used. `rerrstr` reads by exchanging twice, putting back what it
    /// took; `werrstr` writes by exchanging once and dropping what it got.
    pub fn errstr(&mut self, pid: Pid, new: &str) -> String {
        let new = new[..new.len().min(ERRMAX - 1)].to_string();
        match self.tab.get_mut(&pid) {
            Some(p) => std::mem::replace(&mut p.errstr, new),
            None => String::new(),
        }
    }

    pub fn seterrstr(&mut self, pid: Pid, e: &str) {
        if let Some(p) = self.tab.get_mut(&pid) {
            // `ERRMAX` bounds it, as it bounds every error string Plan 9
            // carries.
            p.errstr = e[..e.len().min(ERRMAX - 1)].to_string();
        }
    }

    /// The status a process set with `exits`, before anything reaps it.
    pub fn status(&self, pid: Pid) -> Option<String> {
        self.tab.get(&pid).and_then(|p| p.status.clone())
    }

    /// Every living process, for `ls /proc`.
    pub fn pids(&self) -> Vec<Pid> {
        self.tab.keys().copied().collect()
    }

    /// `up->pgrp->pgrpid` — the namespace group's number.
    pub fn pgrpid(&self, pid: Pid) -> Option<u32> {
        self.tab.get(&pid).map(|p| p.ns.borrow().id())
    }

    /// `up->user`, for the device that reports it.
    pub fn user(&self, pid: Pid) -> Option<String> {
        self.tab.get(&pid).map(|p| p.user.clone())
    }

    pub fn ppid(&self, pid: Pid) -> Option<Pid> {
        self.tab.get(&pid).map(|p| p.ppid)
    }

    /// `userwrite` sets it (`auth.c:113`, `kstrdup(&up->user, "none")`). The
    /// device decides who may; this only carries it out.
    pub fn setuser(&mut self, pid: Pid, name: &str) {
        if let Some(p) = self.tab.get_mut(&pid) {
            p.user = name.to_string();
        }
    }

    /// `renameuser` (`proc.c:1601`): walk the whole table and rename every
    /// process owned by the old name. This is why writing `/dev/hostowner`
    /// carries eve's processes with it.
    pub fn renameuser(&mut self, old: &str, new: &str) {
        for p in self.tab.values_mut() {
            if p.user == old {
                p.user = new.to_string();
            }
        }
    }

    /// The six numbers `/dev/cputime` reports, in milliseconds. `TReal` is
    /// wall time, which Plan 9 computes as `MACHP(0)->ticks - l`
    /// (`devcons.c:63`) — here, from the host's clock.
    pub fn cputime(&self, pid: Pid, now_nsec: u64) -> [u64; 6] {
        match self.tab.get(&pid) {
            None => [0; 6],
            Some(p) => {
                let mut t = p.time;
                t[TREAL] = match p.started {
                    Some(start) => now_nsec.saturating_sub(start) / 1_000_000,
                    None => 0,
                };
                t
            }
        }
    }

    /// Add to one of a process's time slots. The kernel charges `TUser` and
    /// `TSys` as it does work; `TReal` is computed, not charged.
    pub fn charge(&mut self, pid: Pid, slot: usize, ms: u64) {
        if let Some(p) = self.tab.get_mut(&pid) {
            p.time[slot] += ms;
        }
    }

    /// Record that a process has begun, so `TReal` has an origin.
    pub fn started(&mut self, pid: Pid, now_nsec: u64) {
        if let Some(p) = self.tab.get_mut(&pid) {
            p.started = Some(now_nsec);
        }
    }

    /// `exits(2)`. Plan 9 folds an exited child's times into its parent's
    /// `TCUser`/`TCSys`/`TCReal`, which is what makes those three mean
    /// anything.
    pub fn exits(&mut self, pid: Pid, status: &str, now_nsec: Option<u64>) {
        let status = &status[..status.len().min(ERRMAX - 1)];
        let (ppid, time) = match self.tab.get_mut(&pid) {
            None => return,
            Some(p) => {
                p.status = Some(status.to_string());
                // `TReal` stops here. Plan 9 fills the wait message in
                // `pexit` from `up->time` (`proc.c:1149`), and wall time is
                // meaningless once the process is gone, so it is fixed at the
                // moment it goes rather than recomputed from a clock later.
                // `None` where nothing at hand has a clock — `/proc/n/ctl`'s
                // kill runs inside a device, and a device cannot read the
                // machine. Plan 9's `pexit` always can (`MACHP(0)->ticks`).
                if let (Some(start), Some(now)) = (p.started, now_nsec) {
                    p.time[TREAL] = now.saturating_sub(start) / 1_000_000;
                }
                (p.ppid, p.time)
            }
        };
        if let Some(parent) = self.tab.get_mut(&ppid) {
            for i in 0..3 {
                parent.time[TCUSER + i] += time[i];
            }
        }
        // `pexit`'s own last acts (`proc.c:1219`, `:1244`): the wait record
        // goes on the parent's queue and **`wakeup(&p->waitr)`** — the
        // parent is asleep in `pwait` until one arrives. Without this a
        // `await` would sleep for ever.
        // **Off the run queue.** On Plan 9 a process that exits is `up` and
        // so is on no queue — `runproc` took it off to enter it. Here a
        // `procrfork` child can be readied and then end before the scheduler
        // ever enters it, because it runs its few instructions on the
        // parent's own instance; leaving it queued means `sched` picks a
        // process the machine has nothing of.
        if self.state(pid) == State::Ready {
            let pri = self.tab[&pid].priority;
            self.dequeueproc(pri, pid);
        }
        if self.readied == Some(pid) {
            self.readied = None;
        }
        self.setstate(pid, State::Moribund);
        self.wakeup(Rid(ppid, Which::Waitr));
    }

    /// `await(2)`: reap one exited child. Plan 9 states no order and neither
    /// does this — an earlier version promised "youngest pid first", which was
    /// a rule invented rather than found. A caller that needs a particular
    /// child waits for its pid.
    ///
    /// A child forked with `RFNOWAIT` is never reported and leaves no zombie.
    /// `haswaitq` (`proc.c:1271`) — the condition `pwait` sleeps on: is
    /// there a wait record yet? A reaped child is one with a status that
    /// nobody has taken.
    pub fn haswaitq(&self, pid: Pid) -> bool {
        self.tab.values().any(|p| p.ppid == pid && !p.waited && p.status.is_some())
    }

    /// `up->nchild` (`pwait`, `proc.c:1294`): *"if(up->nchild == 0 &&
    /// up->waitq == 0) error(Enochild)"*. A process with neither a living
    /// child nor a record waiting has nothing to wait for, and waiting would
    /// be for ever.
    pub fn nchild(&self, pid: Pid) -> usize {
        self.tab.values().filter(|p| p.ppid == pid && !p.waited).count()
    }

    pub fn await_child(&mut self, pid: Pid) -> Option<Waitmsg> {
        let cpid = *self
            .tab
            .iter()
            .find(|(_, p)| p.ppid == pid && !p.waited && p.status.is_some())
            .map(|(cpid, _)| cpid)?;
        let p = self.tab.remove(&cpid)?;
        Some(Waitmsg {
            pid: cpid,
            time: [p.time[TUSER], p.time[TSYS], p.time[TREAL]],
            msg: p.status.unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dup_to_a_named_slot_and_to_the_lowest_free_one() {
        let mut f = Fds::default();
        let a = f.add(Chan::attach(crate::dev::DevId::Root, 0));
        assert_eq!(f.dup(a, 9), Some(9));
        assert!(
            f.close(a).expect("an open descriptor").is_none(),
            "fd 9 still holds the channel, so this was not the last reference"
        );
        assert_eq!(f.dup(9, -1), Some(0), "the lowest free slot");
    }

    #[test]
    fn a_dup_shares_the_offset_it_does_not_copy_it() {
        let mut f = Fds::default();
        let a = f.add(Chan::attach(crate::dev::DevId::Root, 0));
        let b = f.dup(a, -1).unwrap();
        f.get(a).unwrap().borrow_mut().offset = 42;
        assert_eq!(f.get(b).unwrap().borrow().offset, 42);
    }

    fn one() -> Procs {
        Procs::new(Chan::attach(crate::dev::DevId::Root, 0))
    }

    // ---- the scheduler ----------------------------------------------------

    /// **Two processes on one `Rendez` is counted, not refused.**
    /// `proc.c:826` prints and dumps the stack, and `r->p = up` happens
    /// anyway; declining to sleep was a branch Plan 9 does not have.
    #[test]
    fn a_double_sleep_is_noticed_and_not_prevented() {
        let mut p = one();
        let c = p.rfork(1, rf::PROC).unwrap();
        let r = Rid(1, Which::Sleep);
        assert!(p.sleep(1, r, false));
        assert!(p.sleep(c, r, false), "it sleeps, as Plan 9's does");
        assert_eq!(p.doublesleep, 1, "and it is noticed");
    }

    /// `sleep` commits the process (`proc.c:815`): `r->p = up`, `up->state =
    /// Wakeme`, `up->r = r`. **And it checks the condition first** — *"if
    /// condition happened … never mind"* — so a `sleep` whose reason has
    /// already passed does not sleep at all, which is the whole reason
    /// `sleep` takes a function.
    #[test]
    fn sleep_commits_only_when_the_condition_has_not_happened() {
        let mut p = one();
        let r = Rid(1, Which::Sleep);

        assert!(!p.sleep(1, r, true), "the condition happened: never mind");
        assert_eq!(p.state(1), State::Running);

        assert!(p.sleep(1, r, false), "committed, and the caller must leave");
        assert_eq!(p.state(1), State::Wakeme);
        assert_eq!(p.tab[&1].r, Some(r));
    }

    /// `wakeup` (`proc.c:942`) readies the sleeper and clears both ends.
    /// The pair check is Plan 9's panic — *"if(p->state != Wakeme || p->r !=
    /// r)"* — so a wakeup on the wrong `Rendez` readies nobody.
    #[test]
    fn wakeup_readies_the_sleeper_and_only_on_its_own_rendez() {
        let mut p = one();
        let (sleep, waitr) = (Rid(1, Which::Sleep), Rid(1, Which::Waitr));
        p.sleep(1, sleep, false);

        assert_eq!(p.wakeup(waitr), None, "nobody sleeps there");
        assert_eq!(p.state(1), State::Wakeme);

        assert_eq!(p.wakeup(sleep), Some(1));
        assert_eq!(p.state(1), State::Ready);
        assert_eq!(p.tab[&1].r, None);
    }

    /// `ready` puts a process on the queue and `runproc` takes it off. **The
    /// one just readied runs next** — `m->readied = p`, *"group
    /// scheduling"* (`proc.c:428`) — which is what makes a `wakeup` hand the
    /// processor to whoever it woke.
    #[test]
    fn the_process_just_readied_is_the_one_that_runs_next() {
        let mut p = one();
        let a = p.rfork(1, rf::PROC).expect("a child");
        let b = p.rfork(1, rf::PROC).expect("another");
        // `rfork` leaves a child `Ready` but on no queue; put both on.
        p.setstate(a, State::Scheding);
        p.setstate(b, State::Scheding);
        p.ready(a);
        p.ready(b);
        assert_eq!(p.runproc(), Some(b), "the last readied, not the first queued");
        assert_eq!(p.runproc(), Some(a));
        assert_eq!(p.runproc(), None, "and then idlehands");
    }

    /// A higher priority queue is emptied first — `runproc`'s loop runs the
    /// queues downwards from `Nrq-1`.
    ///
    /// **But not before `m->readied`**, whose condition is only that the two
    /// edf queues are empty: a `wakeup` beats a higher priority, and that is
    /// what *"cooperative scheduling"* means. So the readied one goes first
    /// and the priority order decides everything after it.
    #[test]
    fn readied_runs_first_and_then_the_highest_priority() {
        let mut p = one();
        let hi = p.rfork(1, rf::PROC).unwrap();
        let lo1 = p.rfork(1, rf::PROC).unwrap();
        let lo2 = p.rfork(1, rf::PROC).unwrap();
        p.tab.get_mut(&hi).unwrap().basepri = pri::KPROC;
        for q in [hi, lo1, lo2] {
            p.setstate(q, State::Scheding);
            p.ready(q);
        }
        assert_eq!(p.runproc(), Some(lo2), "the last readied, whatever its priority");
        assert_eq!(p.runproc(), Some(hi), "then PriKproc 13 before PriNormal 10");
        assert_eq!(p.runproc(), Some(lo1));
        assert_eq!(p.runproc(), None, "and then idlehands");
    }

    /// `tsleep`'s timer: `timerintr` wakes whoever is due, and nobody
    /// else. Plan 9 hangs a `Timer` per sleeper; the deadline is on the
    /// process here, and this is the walk.
    #[test]
    fn a_tsleep_is_woken_by_its_deadline_and_not_before() {
        let mut p = one();
        let r = Rid(1, Which::Sleep);
        assert!(p.tsleep(1, r, false, 500));
        assert_eq!(p.nextalarm(), Some(500));

        p.timerintr(499);
        assert_eq!(p.state(1), State::Wakeme, "not yet");
        p.timerintr(500);
        assert_eq!(p.state(1), State::Ready, "due");
        assert_eq!(p.nextalarm(), None);
    }

    /// `pexit` wakes the parent (`proc.c:1219`, `:1244`: `wakeup(&p->waitr)`)
    /// — a parent asleep in `pwait` is waiting for exactly this, and without
    /// it an `await` sleeps for ever.
    #[test]
    fn a_child_exiting_wakes_the_parent_out_of_wait() {
        let mut p = one();
        let c = p.rfork(1, rf::PROC).unwrap();
        assert!(p.sleep(1, Rid(1, Which::Waitr), false));
        assert_eq!(p.state(1), State::Wakeme);

        p.exits(c, "", None);
        assert_eq!(p.state(1), State::Ready, "the parent is runnable again");
        assert_eq!(p.state(c), State::Moribund);
    }

    /// The three-way rule, on file descriptors: a `G` bit copies, a `CG` bit
    /// clears, and neither shares. Sharing is the default because that is what
    /// `fork` without flags means in Plan 9.
    #[test]
    fn rfork_shares_copies_or_clears_the_descriptors() {
        // share — the parent's later open is visible to the child
        let mut p = one();
        p.tab.get(&1).unwrap().fds.borrow_mut().add(Chan::attach(crate::dev::DevId::Root, 0));
        let c = p.rfork(1, rf::PROC).unwrap();
        p.tab.get(&1).unwrap().fds.borrow_mut().add(Chan::attach(crate::dev::DevId::Root, 0));
        assert_eq!(p.tab.get(&c).unwrap().fds.borrow().count(), 2, "no bit means SHARE");

        // copy — the child starts with what the parent had, and diverges
        let mut p = one();
        p.tab.get(&1).unwrap().fds.borrow_mut().add(Chan::attach(crate::dev::DevId::Root, 0));
        let c = p.rfork(1, rf::PROC | rf::FDG).unwrap();
        p.tab.get(&1).unwrap().fds.borrow_mut().add(Chan::attach(crate::dev::DevId::Root, 0));
        assert_eq!(p.tab.get(&c).unwrap().fds.borrow().count(), 1, "RFFDG means COPY");
        assert_eq!(p.tab.get(&1).unwrap().fds.borrow().count(), 2);

        // clear — the child starts with none
        let mut p = one();
        p.tab.get(&1).unwrap().fds.borrow_mut().add(Chan::attach(crate::dev::DevId::Root, 0));
        let c = p.rfork(1, rf::PROC | rf::CFDG).unwrap();
        assert_eq!(p.tab.get(&c).unwrap().fds.borrow().count(), 0, "RFCFDG means CLEAR");
    }

    /// The same rule, on the environment group — which is the whole reason
    /// `#e` is a device and `RFENVG`/`RFCENVG` exist.
    #[test]
    fn rfork_shares_copies_or_clears_the_environment() {
        let mut p = one();
        p.tab.get(&1).unwrap().env.borrow_mut().insert("a".into(), "1".into());
        let c = p.rfork(1, rf::PROC).unwrap();
        p.tab.get(&1).unwrap().env.borrow_mut().insert("b".into(), "2".into());
        assert_eq!(p.tab.get(&c).unwrap().env.borrow().len(), 2, "no bit means SHARE");

        let mut p = one();
        p.tab.get(&1).unwrap().env.borrow_mut().insert("a".into(), "1".into());
        let c = p.rfork(1, rf::PROC | rf::ENVG).unwrap();
        p.tab.get(&1).unwrap().env.borrow_mut().insert("b".into(), "2".into());
        assert_eq!(p.tab.get(&c).unwrap().env.borrow().len(), 1, "RFENVG means COPY");

        let mut p = one();
        p.tab.get(&1).unwrap().env.borrow_mut().insert("a".into(), "1".into());
        let c = p.rfork(1, rf::PROC | rf::CENVG).unwrap();
        assert!(p.tab.get(&c).unwrap().env.borrow().is_empty(), "RFCENVG means CLEAR");
    }

    /// And on the namespace, through `rfork` rather than through `Ns` alone —
    /// the bit has to be wired to the table.
    #[test]
    fn rfork_shares_copies_or_clears_the_namespace() {
        let mut p = one();
        let c = p.rfork(1, rf::PROC).unwrap();
        assert!(Rc::ptr_eq(&p.tab[&1].ns, &p.tab[&c].ns), "no bit means SHARE");

        let mut p = one();
        let c = p.rfork(1, rf::PROC | rf::NAMEG).unwrap();
        assert!(!Rc::ptr_eq(&p.tab[&1].ns, &p.tab[&c].ns), "RFNAMEG means COPY");

        let mut p = one();
        let c = p.rfork(1, rf::PROC | rf::CNAMEG).unwrap();
        assert!(!Rc::ptr_eq(&p.tab[&1].ns, &p.tab[&c].ns), "RFCNAMEG means CLEAR");
    }

    /// `RFNOMNT` sets `noattach` and it is never cleared; a copied namespace
    /// carries it (`sysproc.c:38`, `:42`). A sandbox that a child could shed
    /// by copying its namespace would not be one.
    #[test]
    fn nomnt_sets_noattach_and_a_copy_carries_it() {
        let mut p = one();
        assert!(!p.tab[&1].ns.borrow().noattach());
        p.rfork(1, rf::NOMNT);
        assert!(p.tab[&1].ns.borrow().noattach(), "RFNOMNT sets it");

        let c = p.rfork(1, rf::PROC | rf::NAMEG).unwrap();
        assert!(p.tab[&c].ns.borrow().noattach(), "a copy carries it");
        let c2 = p.rfork(1, rf::PROC | rf::CNAMEG).unwrap();
        assert!(p.tab[&c2].ns.borrow().noattach(), "and so does a cleared one");
    }

    /// `ERRMAX` (`libc.h:553`) bounds an error string and an exit status —
    /// `sysexits` copies through `char buf[ERRMAX]` (`sysproc.c:668`).
    #[test]
    fn errmax_bounds_an_error_string_and_a_status() {
        let mut p = one();
        p.seterrstr(1, &"x".repeat(1000));
        assert!(p.errstr(1, "").len() < ERRMAX);
        let c = p.rfork(1, rf::PROC).unwrap();
        p.exits(c, &"y".repeat(1000), None);
        assert!(p.await_child(1).unwrap().msg.len() < ERRMAX);
    }

    /// `NFD` = 100 (`portdat.h:476`). A process that leaks descriptors fails
    /// rather than growing without limit.
    #[test]
    fn nfd_bounds_the_descriptor_table() {
        let mut f = Fds::default();
        for _ in 0..NFD {
            assert!(f.add(Chan::attach(crate::dev::DevId::Root, 0)) >= 0);
        }
        assert_eq!(f.add(Chan::attach(crate::dev::DevId::Root, 0)), -1);
    }

    /// `sysrfork` checks its flags before it commits (`sysproc.c:43`). Asking
    /// to copy AND clear the same table is `Ebadarg`, not a silent choice
    /// between them; and `RFMEM` or `RFNOWAIT` without `RFPROC` describes a
    /// child that is not being made.
    #[test]
    fn contradictory_rfork_flags_are_an_error() {
        for bad in [
            rf::PROC | rf::FDG | rf::CFDG,
            rf::PROC | rf::NAMEG | rf::CNAMEG,
            rf::PROC | rf::ENVG | rf::CENVG,
            rf::MEM,
            rf::NOWAIT,
        ] {
            assert!(
                Procs::rforkcheck(bad).is_err(),
                "rfork({bad:#x}) must be Ebadarg, as sysproc.c:44-56 has it"
            );
            assert_eq!(one().rfork(1, bad), None, "and rfork must make nothing");
        }
        assert!(Procs::rforkcheck(rf::PROC | rf::NOWAIT).is_ok());
    }

    /// `rfork` without `RFPROC` makes no child: it changes the CALLER's own
    /// tables. That is how a shell gets a private namespace without forking.
    #[test]
    fn rfork_without_rfproc_changes_the_caller_and_makes_no_child() {
        let mut p = one();
        let before = p.tab[&1].ns.clone();
        assert_eq!(p.rfork(1, rf::NAMEG), None, "no child");
        assert_eq!(p.count(), 1);
        assert!(!Rc::ptr_eq(&p.tab[&1].ns, &before), "the caller's own namespace was replaced");
    }

    /// `exits` then `await`: the parent reaps the child and gets its status.
    #[test]
    fn a_child_that_exits_is_reaped_by_its_parent_with_its_status() {
        let mut p = one();
        let c = p.rfork(1, rf::PROC).unwrap();
        assert!(p.await_child(1).is_none(), "nothing to reap before it exits");
        p.exits(c, "oops", None);
        let w = p.await_child(1).expect("the exited child is reaped");
        assert_eq!((w.pid, w.msg.as_str()), (c, "oops"));
        assert!(p.await_child(1).is_none(), "reaped once, and gone");
    }

    /// `RFNOWAIT`: the child is never reported and leaves no zombie.
    #[test]
    fn a_child_forked_with_nowait_is_never_reaped() {
        let mut p = one();
        let c = p.rfork(1, rf::PROC | rf::NOWAIT).unwrap();
        p.exits(c, "gone", None);
        assert!(p.await_child(1).is_none());
    }

    /// The wait message is `sysawait`'s, verbatim (`sysproc.c:727`):
    /// `"%d %lud %lud %lud %q"`. `wait(2)` splits it with `tokenize` into
    /// exactly five fields (`9sys/wait.c`), so a status holding a space must
    /// come back as ONE field or every such status is lost.
    #[test]
    fn the_wait_message_is_the_one_wait_2_parses() {
        let w = Waitmsg { pid: 7, time: [1, 2, 3], msg: String::new() };
        assert_eq!(w.format(), "7 1 2 3 ''", "an empty status is quoted");

        let w = Waitmsg { pid: 7, time: [0, 0, 0], msg: "oops".into() };
        assert_eq!(w.format(), "7 0 0 0 oops", "a plain word is not");

        let w = Waitmsg { pid: 7, time: [0, 0, 0], msg: "no such file".into() };
        assert_eq!(w.format(), "7 0 0 0 'no such file'");

        let w = Waitmsg { pid: 7, time: [0, 0, 0], msg: "it's gone".into() };
        assert_eq!(w.format(), "7 0 0 0 'it''s gone'", "a quote doubles");
    }

    /// A process reaps its own children and nobody else's.
    #[test]
    fn await_reaps_only_this_processs_children() {
        let mut p = one();
        let a = p.rfork(1, rf::PROC).unwrap();
        let b = p.rfork(a, rf::PROC).unwrap();
        p.exits(b, "b", None);
        assert!(p.await_child(1).is_none(), "b is a's child, not 1's");
        let w = p.await_child(a).expect("a reaps its own child");
        assert_eq!((w.pid, w.msg.as_str()), (b, "b"));
    }
}
