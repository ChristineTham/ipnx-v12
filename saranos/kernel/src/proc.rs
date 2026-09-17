//! Processes, and the three things a process owns that `rfork` can share,
//! copy or clear: its namespace, its file descriptors, and its environment.
//!
//! This is the kernel's job and very nearly the whole of it. Everything a
//! system does beyond orchestrating processes is done BY processes, talking to
//! each other; so what is here is Plan 9's `rfork`, `exec`, `exits` and
//! `await`, and the tables they act on.

use crate::ns::Ns;
use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

pub type Pid = u32;
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
}

/// An open file. What it refers to is a channel; the kernel cares that it has
/// an offset and that closing the last reference closes the channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chan {
    /// What this channel talks to. A name here, a mount or device behind it.
    pub target: String,
    pub offset: u64,
}

/// A process's file descriptors. Shared or copied per `rfork`, which is why it
/// sits behind a reference rather than inside `Proc`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fds {
    slots: Vec<Option<Rc<RefCell<Chan>>>>,
}

impl Fds {
    pub fn add(&mut self, c: Chan) -> Fd {
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
        self.slots.get(fd as usize)?.as_ref()
    }
    pub fn close(&mut self, fd: Fd) -> bool {
        match self.slots.get_mut(fd as usize) {
            Some(s @ Some(_)) => {
                *s = None;
                true
            }
            _ => false,
        }
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
}

/// A process.
#[derive(Clone)]
pub struct Proc {
    pub pid: Pid,
    pub ppid: Pid,
    /// The three shareable tables. `Rc` is the sharing: two processes holding
    /// the same `Rc` after `rfork` without the `G` bit is not a metaphor for
    /// sharing, it is the sharing.
    pub ns: Rc<RefCell<Ns>>,
    pub fds: Rc<RefCell<Fds>>,
    pub env: Rc<RefCell<HashMap<String, String>>>,
    pub cwd: String,
    /// Set once the process has exited; `await` reports it and reaps.
    pub status: Option<String>,
    /// `RFNOWAIT`: the parent abandoned it, so no wait record is kept.
    pub waited: bool,
}

impl Proc {
    fn root(pid: Pid) -> Proc {
        Proc {
            pid,
            ppid: 0,
            ns: Rc::new(RefCell::new(Ns::new())),
            fds: Rc::new(RefCell::new(Fds::default())),
            env: Rc::new(RefCell::new(HashMap::new())),
            cwd: "/".to_string(),
            status: None,
            waited: true,
        }
    }
}

/// The process table.
pub struct Procs {
    tab: HashMap<Pid, Proc>,
    next: Pid,
}

impl Default for Procs {
    fn default() -> Self {
        Self::new()
    }
}

impl Procs {
    /// A fresh table with pid 1 in it, as every system has.
    pub fn new() -> Self {
        let mut tab = HashMap::new();
        tab.insert(1, Proc::root(1));
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
    pub fn rfork(&mut self, pid: Pid, flags: i32) -> Option<Pid> {
        let parent = self.tab.get(&pid)?.clone();

        let ns = Self::table(&parent.ns, flags & rf::CNAMEG != 0, flags & rf::NAMEG != 0);
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
            ns,
            fds,
            env,
            cwd: parent.cwd.clone(),
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

    /// `exits(2)`.
    pub fn exits(&mut self, pid: Pid, status: &str) {
        if let Some(p) = self.tab.get_mut(&pid) {
            p.status = Some(status.to_string());
        }
    }

    /// `await(2)`: reap one exited child, youngest pid first. A child forked
    /// with `RFNOWAIT` is never reported and leaves no zombie.
    pub fn await_child(&mut self, pid: Pid) -> Option<(Pid, String)> {
        let mut found = None;
        for (cpid, p) in &self.tab {
            if p.ppid == pid && !p.waited && p.status.is_some() {
                if found.map_or(true, |f: Pid| *cpid < f) {
                    found = Some(*cpid);
                }
            }
        }
        let cpid = found?;
        let p = self.tab.remove(&cpid)?;
        Some((cpid, p.status.unwrap_or_default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_table_has_pid_one_and_nothing_else() {
        let p = Procs::new();
        assert_eq!(p.count(), 1);
        assert!(p.get(1).is_some());
    }

    #[test]
    fn without_rfnameg_the_namespace_is_shared() {
        // rfork(2)'s rule, and bind(1) depends on it: a child's bind is the
        // parent's bind unless the parent asked for a copy.
        let mut t = Procs::new();
        let c = t.rfork(1, rf::PROC).unwrap();
        assert!(Rc::ptr_eq(&t.get(1).unwrap().ns, &t.get(c).unwrap().ns));
    }

    #[test]
    fn rfnameg_copies_and_rfcnameg_clears() {
        let mut t = Procs::new();
        t.get(1).unwrap().ns.borrow_mut().bind(
            "/bin",
            crate::ns::Element::new("system"),
            crate::ns::Bind::Replace,
        );

        let copied = t.rfork(1, rf::PROC | rf::NAMEG).unwrap();
        assert!(!Rc::ptr_eq(&t.get(1).unwrap().ns, &t.get(copied).unwrap().ns));
        assert!(!t.get(copied).unwrap().ns.borrow().is_empty(), "a copy keeps the binds");

        let cleared = t.rfork(1, rf::PROC | rf::CNAMEG).unwrap();
        assert!(t.get(cleared).unwrap().ns.borrow().is_empty(), "a cleared namespace is empty");
    }

    #[test]
    fn rfork_without_rfproc_acts_on_the_caller() {
        // How a process gives ITSELF a private namespace. No new process.
        let mut t = Procs::new();
        let before = t.get(1).unwrap().ns.clone();
        assert_eq!(t.rfork(1, rf::NAMEG), None);
        assert_eq!(t.count(), 1);
        assert!(!Rc::ptr_eq(&before, &t.get(1).unwrap().ns));
    }

    #[test]
    fn fds_are_shared_copied_or_cleared_the_same_way() {
        let mut t = Procs::new();
        t.get(1).unwrap().fds.borrow_mut().add(Chan { target: "cons".into(), offset: 0 });

        let shared = t.rfork(1, rf::PROC).unwrap();
        assert!(Rc::ptr_eq(&t.get(1).unwrap().fds, &t.get(shared).unwrap().fds));

        let copied = t.rfork(1, rf::PROC | rf::FDG).unwrap();
        assert_eq!(t.get(copied).unwrap().fds.borrow().count(), 1);
        assert!(!Rc::ptr_eq(&t.get(1).unwrap().fds, &t.get(copied).unwrap().fds));

        let cleared = t.rfork(1, rf::PROC | rf::CFDG).unwrap();
        assert_eq!(t.get(cleared).unwrap().fds.borrow().count(), 0);
    }

    #[test]
    fn await_reaps_an_exited_child_and_reports_its_status() {
        let mut t = Procs::new();
        let c = t.rfork(1, rf::PROC).unwrap();
        assert_eq!(t.await_child(1), None, "a living child is not reaped");
        t.exits(c, "");
        assert_eq!(t.await_child(1), Some((c, String::new())));
        assert_eq!(t.count(), 1, "reaping removes it");
    }

    #[test]
    fn rfnowait_leaves_no_zombie() {
        let mut t = Procs::new();
        let c = t.rfork(1, rf::PROC | rf::NOWAIT).unwrap();
        t.exits(c, "gone");
        assert_eq!(t.await_child(1), None, "an abandoned child is never reported");
    }

    #[test]
    fn dup_to_a_named_slot_and_to_the_lowest_free_one() {
        let mut f = Fds::default();
        let a = f.add(Chan { target: "cons".into(), offset: 0 });
        assert_eq!(f.dup(a, 9), Some(9));
        assert!(f.close(a));
        assert_eq!(f.dup(9, -1), Some(0), "the lowest free slot");
    }

    #[test]
    fn a_dup_shares_the_offset_it_does_not_copy_it() {
        let mut f = Fds::default();
        let a = f.add(Chan { target: "f".into(), offset: 0 });
        let b = f.dup(a, -1).unwrap();
        f.get(a).unwrap().borrow_mut().offset = 42;
        assert_eq!(f.get(b).unwrap().borrow().offset, 42);
    }
}
