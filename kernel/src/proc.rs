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
}

impl Proc {
    fn root(pid: Pid, slash: Chan) -> Proc {
        Proc {
            pid,
            ppid: 0,
            // The first process is eve's. `proc.c:1467`: `kstrdup(&p->user, eve)`.
            user: "eve".to_string(),
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
}

impl Procs {
    /// A fresh table with pid 1 in it. It takes the channel that is pid 1's
    /// root, because a process without one cannot resolve a name at all.
    pub fn new(slash: Chan) -> Self {
        let mut tab = HashMap::new();
        tab.insert(1, Proc::root(1, slash));
        Procs { tab, next: 2 }
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
    }

    /// `await(2)`: reap one exited child. Plan 9 states no order and neither
    /// does this — an earlier version promised "youngest pid first", which was
    /// a rule invented rather than found. A caller that needs a particular
    /// child waits for its pid.
    ///
    /// A child forked with `RFNOWAIT` is never reported and leaves no zombie.
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
