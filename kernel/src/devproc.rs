//! `#p` — proc (`plan9/sys/src/9/port/devproc.c`).
//!
//! **The process table as files.** `/proc/17/status` is a text a program
//! reads; `ps` is a program that reads them and prints them, not a call into
//! the kernel. `/proc/17/ns` prints the process's namespace as the `bind` and
//! `mount` lines that would rebuild it — which is what makes a namespace
//! inspectable at all.
//!
//! `procdir[]` (`devproc.c:79`) is 18 files. Eight are here; the rest —
//! `mem`, `regs`, `fpregs`, `kregs`, `text`, `segment`, `profile` — describe
//! an address space and a register set this kernel does not have, and are
//! absent for the reason `#i` and `#m` are rather than by a separate rule.

use crate::chan::Chan;
use crate::dev::{Dev, DevId, EVE};
use crate::ninep::{Qid, QTDIR};
use crate::proc::{Fd, Pid, Up};
use std::cell::RefCell;
use std::rc::Rc;

const EPERM: &str = "permission denied";
const EPROCDIED: &str = "process exited";

/// `KNAMELEN` (`portdat.h`) and `STATSIZE` (`devproc.c:73`).
pub const KNAMELEN: usize = 28;
pub const STATSIZE: usize = 2 * KNAMELEN + 12 + 9 * 12;

/// The files of one process, as `procdir[]` names them.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Q {
    Root = 0,
    Args,
    Ctl,
    Fd,
    Note,
    Noteid,
    Ns,
    Proc,
    Status,
    Wait,
}

/// name, which file, permission — `procdir[]`'s own values.
pub const PROCDIR: &[(&str, Q, u32)] = &[
    ("args", Q::Args, 0o660),
    ("ctl", Q::Ctl, 0o000),
    ("fd", Q::Fd, 0o444),
    ("note", Q::Note, 0o000),
    ("noteid", Q::Noteid, 0o664),
    ("ns", Q::Ns, 0o444),
    ("proc", Q::Proc, 0o400),
    ("status", Q::Status, 0o444),
    ("wait", Q::Wait, 0o400),
];

/// A qid carries both which process and which of its files: the pid in the
/// high bits, the file in the low. Plan 9 does the same with `mkqid` and
/// `QID(c->qid)`.
const SHIFT: u64 = 8;

fn qid_of(pid: Pid, q: Q) -> u64 {
    ((pid as u64) << SHIFT) | q as u64
}

fn split_qid(path: u64) -> (Pid, Q) {
    let pid = (path >> SHIFT) as Pid;
    let q = PROCDIR
        .iter()
        .map(|e| e.1)
        .find(|q| *q as u64 == path & ((1 << SHIFT) - 1))
        .unwrap_or(Q::Root);
    (pid, q)
}

pub struct ProcDev {
    up: Rc<RefCell<Up>>,
    pub eve: String,
}

impl ProcDev {
    pub fn new(up: Rc<RefCell<Up>>) -> ProcDev {
        ProcDev { up, eve: "eve".into() }
    }

    /// `nonone` (`devproc.c:336`): a process running as `none` cannot read or
    /// write another process's state. The comment says why — *"to contain
    /// access of servers running as none should they be subverted by, for
    /// example, a stack attack."*
    fn nonone(&self, target: Pid) -> Result<(), String> {
        let up = self.up.borrow();
        if target == up.pid {
            return Ok(());
        }
        let user = up.user();
        if user != "none" || user == self.eve {
            return Ok(());
        }
        Err(EPERM.into())
    }

    fn alive(&self, pid: Pid) -> bool {
        self.up.borrow().procs.borrow().get(pid).is_some()
    }
}

impl Dev for ProcDev {
    fn id(&self) -> DevId {
        DevId::Proc
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn attach(&mut self, _spec: &str) -> Result<Chan, String> {
        Ok(Chan::attach(DevId::Proc, 0))
    }

    /// Two levels: `#p` holds a directory per pid, and each holds its files.
    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Chan>, String> {
        if !c.qid.is_dir() {
            return Err("not a directory".into());
        }
        if name == "." {
            return Ok(Some(c.walked(name, c.qid)));
        }
        let (pid, _) = split_qid(c.qid.path);
        if pid == 0 {
            // at `#p`, a name is a pid
            if name == ".." {
                return Ok(Some(c.walked(name, Qid { qtype: QTDIR, vers: 0, path: 0 })));
            }
            let Ok(want): Result<Pid, _> = name.parse() else { return Ok(None) };
            if !self.alive(want) {
                return Ok(None);
            }
            let q = Qid { qtype: QTDIR, vers: 0, path: qid_of(want, Q::Root) };
            return Ok(Some(c.walked(name, q)));
        }
        if name == ".." {
            return Ok(Some(c.walked(name, Qid { qtype: QTDIR, vers: 0, path: 0 })));
        }
        Ok(PROCDIR
            .iter()
            .find(|e| e.0 == name)
            .map(|e| c.walked(name, Qid { qtype: 0, vers: 0, path: qid_of(pid, e.1) })))
    }

    fn open(&mut self, mut c: Chan, mode: u16) -> Result<Chan, String> {
        let (pid, q) = split_qid(c.qid.path);
        if pid != 0 {
            if !self.alive(pid) {
                return Err(EPROCDIED.into());
            }
            self.nonone(pid)?;
            if q == Q::Ns && mode & 3 != crate::chan::mode::OREAD {
                return Err(EPERM.into());
            }
        }
        c.mode = mode;
        Ok(c)
    }

    fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
        Err(EPERM.into())
    }

    fn read(&mut self, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String> {
        let (pid, q) = split_qid(c.qid.path);
        let procs = self.up.borrow().procs.clone();

        // `ls /proc` — one directory per living process, each owned by
        // whoever runs it (`procgen`, `devproc.c:300`).
        if pid == 0 {
            let mut pids: Vec<Pid> = procs.borrow().pids();
            pids.sort();
            let p = procs.borrow();
            let entries: Vec<crate::ninep::Dir> = pids
                .iter()
                .map(|pid| {
                    let user = p.user(*pid).unwrap_or_default();
                    let qid = Qid {
                        qtype: QTDIR,
                        vers: 0,
                        path: qid_of(*pid, Q::Root),
                    };
                    crate::dev::devdir(
                        c,
                        qid,
                        &pid.to_string(),
                        0,
                        &user,
                        EVE,
                        crate::ninep::DMDIR | 0o555,
                    )
                })
                .collect();
            drop(p);
            return Ok(crate::dev::devdirread(c, n, &entries));
        }
        // A process's own directory: `procdir[]`, one entry per file.
        if q == Q::Root && c.qid.is_dir() {
            self.nonone(pid)?;
            let user = procs.borrow().user(pid).ok_or(EPROCDIED)?;
            let entries: Vec<crate::ninep::Dir> = PROCDIR
                .iter()
                .map(|(name, q, perm)| {
                    let qid = Qid { qtype: 0, vers: 0, path: qid_of(pid, *q) };
                    crate::dev::devdir(c, qid, name, 0, &user, EVE, *perm)
                })
                .collect();
            return Ok(crate::dev::devdirread(c, n, &entries));
        }

        let s = {
            self.nonone(pid)?;
            let p = procs.borrow();
            let proc = p.get(pid).ok_or(EPROCDIED)?;
            match q {
                Q::Root => String::new(),
                // `status`: text, user, state, then numbers — the fixed-width
                // record `ps` parses (`devproc.c:867`, STATSIZE).
                Q::Status => {
                    let state = if proc.status.is_some() { "Broken" } else { "Running" };
                    let t = p.cputime(pid, 0);
                    format!(
                        "{:<w$}{:<w$}{:<11}{:<12}{:<12}{:<12}",
                        "init", proc.user, state, t[0], t[1], t[2],
                        w = KNAMELEN
                    )
                }
                Q::Proc | Q::Noteid => format!("{pid}\n"),
                Q::Args => String::new(),
                // `fd`: the working directory, then one line per open fd.
                Q::Fd => {
                    let mut s = format!("{}\n", proc.dot.path);
                    let fds = proc.fds.borrow();
                    for fd in 0..fds.slots() {
                        if let Some(cell) = fds.get(fd) {
                            let ch = cell.borrow();
                            s.push_str(&format!(
                                "{:3} {} {:11} {:11} {}\n",
                                fd,
                                ch.dev.letter(),
                                ch.devno,
                                ch.qid.path,
                                ch.path
                            ));
                        }
                    }
                    s
                }
                // `ns`: the namespace as the lines that would rebuild it
                // (`devproc.c:952`) — `cd`, then `bind`/`mount` per element.
                Q::Ns => {
                    let mut s = String::new();
                    for (on, el, how) in proc.ns.borrow().describe() {
                        s.push_str(&format!("bind {how} {} {}\n", el, on));
                    }
                    s.push_str(&format!("cd {}\n", proc.dot.path));
                    s
                }
                Q::Wait => return Err("wait is await's, not a read".into()),
                Q::Ctl | Q::Note => return Err(EPERM.into()),
            }
        };
        let b = s.into_bytes();
        let off = off as usize;
        if off >= b.len() {
            return Ok(Vec::new());
        }
        Ok(b[off..(off + n).min(b.len())].to_vec())
    }

    /// `procctlreq` (`devproc.c:1321`). The verbs this kernel can answer
    /// honestly; the rest want a scheduler and say so.
    fn write(&mut self, c: &mut Chan, data: &[u8], _off: u64) -> Result<usize, String> {
        let (pid, q) = split_qid(c.qid.path);
        if pid == 0 {
            return Err(EPERM.into());
        }
        self.nonone(pid)?;
        let text = String::from_utf8_lossy(data).trim().to_string();
        let mut word = text.split_whitespace();
        let cmd = word.next().unwrap_or("");
        let procs = self.up.borrow().procs.clone();
        match (q, cmd) {
            (Q::Ctl, "kill") => {
                procs.borrow_mut().exits(pid, "killed", None);
            }
            (Q::Ctl, "close") => {
                let fd: Fd = word.next().and_then(|w| w.parse().ok()).ok_or("bad fd")?;
                let p = procs.borrow();
                let proc = p.get(pid).ok_or(EPROCDIED)?;
                // `procctl`'s "close n" (`devproc.c:1010`). The device this
                // channel belongs to is not told here: `#p` cannot reach the
                // device table, which is exactly what `devtab` being a global
                // gives Plan 9 and what this kernel has instead in `namec`.
                // A close through `close(2)` does tell it (`sysfile.c:285`).
                if proc.fds.borrow_mut().close(fd).is_none() {
                    return Err("fd out of range or not open".into());
                }
            }
            (Q::Ctl, "closefiles") => {
                let p = procs.borrow();
                let proc = p.get(pid).ok_or(EPROCDIED)?;
                let mut fds = proc.fds.borrow_mut();
                for fd in 0..fds.slots() {
                    fds.close(fd);
                }
            }
            (Q::Note, _) => {
                // A note is a string delivered to a process. There is no
                // note group yet (P3), so this refuses rather than dropping
                // it silently.
                return Err("no notes yet — P3".into());
            }
            (Q::Ctl, "start" | "stop" | "waitstop" | "hang" | "nohang") => {
                return Err("no scheduler yet — P3".into());
            }
            (Q::Ctl, _) => return Err("unknown control message".into()),
            _ => return Err(EPERM.into()),
        }
        Ok(data.len())
    }

    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        let (pid, q) = split_qid(c.qid.path);
        let (name, perm) = if pid == 0 {
            ("#p".to_string(), crate::ninep::DMDIR | 0o555)
        } else if c.qid.is_dir() {
            (pid.to_string(), crate::ninep::DMDIR | 0o555)
        } else {
            let e = PROCDIR.iter().find(|e| e.1 == q).ok_or("no such file")?;
            (e.0.to_string(), e.2)
        };
        let user = if pid == 0 {
            EVE.to_string()
        } else {
            self.up.borrow().procs.borrow().user(pid).unwrap_or_else(|| EVE.to_string())
        };
        Ok(crate::dev::devdir(c, c.qid, &name, 0, &user, EVE, perm).conv_d2m())
    }

    fn wstat(&mut self, _c: &mut Chan, _e: &[u8]) -> Result<(), String> {
        Err(EPERM.into())
    }

    fn remove(&mut self, _c: &mut Chan) -> Result<(), String> {
        Err(EPERM.into())
    }

    fn close(&mut self, _c: &mut Chan) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chan::mode::{OREAD, OWRITE};
    use crate::proc::{rf, Procs};

    fn proc() -> (ProcDev, Rc<RefCell<Procs>>) {
        let procs = Rc::new(RefCell::new(Procs::new(Chan::attach(DevId::Root, 0))));
        let up = Rc::new(RefCell::new(Up { pid: 1, procs: procs.clone() }));
        (ProcDev::new(up), procs)
    }

    fn open(d: &mut ProcDev, pid: Pid, name: &str, mode: u16) -> Chan {
        let root = d.attach("").unwrap();
        let dir = d.walk(&root, &pid.to_string()).unwrap().expect("no such pid");
        let c = d.walk(&dir, name).unwrap().expect(name);
        d.open(c, mode).unwrap()
    }

    fn read(d: &mut ProcDev, pid: Pid, name: &str) -> String {
        let mut c = open(d, pid, name, OREAD);
        String::from_utf8_lossy(&d.read(&mut c, 4096, 0).unwrap()).to_string()
    }

    /// `ls /proc` is the living processes, and a pid that is not there has no
    /// directory — the tree is generated from the table, not stored.
    #[test]
    fn the_directory_is_one_per_living_process() {
        fn names(d: &mut ProcDev) -> Vec<String> {
            let mut root = d.attach("").unwrap();
            let b = d.read(&mut root, 4096, 0).unwrap();
            crate::ninep::Dir::parse_all(&b).into_iter().map(|e| e.name).collect()
        }

        let (mut d, procs) = proc();
        assert_eq!(names(&mut d), ["1"]);
        let c = procs.borrow_mut().rfork(1, rf::PROC).unwrap();
        assert_eq!(names(&mut d), ["1", &c.to_string()]);

        // Each is a DIRECTORY, owned by whoever runs it.
        let mut root = d.attach("").unwrap();
        let b = d.read(&mut root, 4096, 0).unwrap();
        let first = &crate::ninep::Dir::parse_all(&b)[0];
        assert!(first.qid.is_dir());
        assert_eq!(first.uid, "eve");

        // A read goes on where the last one stopped — `Chan.dri`, not a byte
        // offset — so reading this channel again answers nothing more.
        assert!(d.read(&mut root, 4096, 0).unwrap().is_empty());

        let root = d.attach("").unwrap();
        assert!(d.walk(&root, "99").unwrap().is_none());
    }

    /// `/proc/n/status` carries the user, which is how `ps` shows an owner
    /// without asking the kernel anything.
    #[test]
    fn status_reports_the_user_and_the_state() {
        let (mut d, procs) = proc();
        let s = read(&mut d, 1, "status");
        assert!(s.contains("eve"), "{s}");
        assert!(s.contains("Running"), "{s}");
        procs.borrow_mut().exits(1, "", None);
        assert!(read(&mut d, 1, "status").contains("Broken"));
    }

    /// `/proc/n/ns` prints the namespace as the lines that would rebuild it.
    /// That is what makes a per-process namespace inspectable at all.
    #[test]
    fn ns_prints_the_namespace_as_bind_lines() {
        let (mut d, procs) = proc();
        let on = Chan::attach(DevId::Root, 0);
        let mut to = Chan::attach(DevId::Pipe, 3);
        to.qid = Qid { qtype: QTDIR, vers: 0, path: 5 };
        procs
            .borrow()
            .get(1)
            .unwrap()
            .ns
            .borrow_mut()
            .mount(&on, crate::ns::Element::new(to), crate::ns::Bind::Replace);
        let s = read(&mut d, 1, "ns");
        assert!(s.contains("bind"), "{s}");
        assert!(s.contains("#|/5"), "the mounted channel is named: {s}");
        assert!(s.contains("cd "), "{s}");
    }

    /// `/proc/n/fd` lists the open descriptors, so `ls` of it is what `lsof`
    /// is elsewhere.
    #[test]
    fn fd_lists_the_open_descriptors() {
        let (mut d, procs) = proc();
        procs.borrow().get(1).unwrap().fds.borrow_mut().add(Chan::attach(DevId::Pipe, 2));
        let s = read(&mut d, 1, "fd");
        assert!(s.lines().count() >= 2, "the cwd, then one line per fd: {s}");
        assert!(s.contains('|'), "the device letter is there: {s}");
    }

    /// `kill` through `/proc/n/ctl` — a process is killed by writing to a
    /// file, which is the whole point of the device.
    #[test]
    fn a_process_is_killed_by_writing_to_its_ctl_file() {
        let (mut d, procs) = proc();
        let c = procs.borrow_mut().rfork(1, rf::PROC).unwrap();
        let mut ctl = open(&mut d, c, "ctl", OWRITE);
        d.write(&mut ctl, b"kill", 0).unwrap();
        let w = procs.borrow_mut().await_child(1).expect("the killed child is reaped");
        assert_eq!((w.pid, w.msg.as_str()), (c, "killed"));
    }

    /// `close` and `closefiles` act on the target's fd table.
    #[test]
    fn ctl_can_close_one_descriptor_or_all_of_them() {
        let (mut d, procs) = proc();
        let fds = procs.borrow().get(1).unwrap().fds.clone();
        fds.borrow_mut().add(Chan::attach(DevId::Pipe, 1));
        fds.borrow_mut().add(Chan::attach(DevId::Pipe, 2));
        let mut ctl = open(&mut d, 1, "ctl", OWRITE);
        d.write(&mut ctl, b"close 0", 0).unwrap();
        assert_eq!(fds.borrow().count(), 1);
        d.write(&mut ctl, b"closefiles", 0).unwrap();
        assert_eq!(fds.borrow().count(), 0);
        assert!(d.write(&mut ctl, b"close 9", 0).is_err());
    }

    /// `nonone` (`devproc.c:336`): a process running as `none` cannot touch
    /// another process's state, against a subverted server.
    #[test]
    fn none_cannot_reach_another_process() {
        let (mut d, procs) = proc();
        let other = procs.borrow_mut().rfork(1, rf::PROC).unwrap();
        procs.borrow_mut().setuser(1, "none");
        assert!(read(&mut d, 1, "status").contains("none"), "its own is fine");
        let root = d.attach("").unwrap();
        let dir = d.walk(&root, &other.to_string()).unwrap().unwrap();
        let c = d.walk(&dir, "status").unwrap().unwrap();
        assert!(d.open(c, OREAD).is_err(), "none must not read another's state");
    }

    /// What wants a scheduler says so rather than pretending to work.
    #[test]
    fn the_verbs_that_need_a_scheduler_refuse() {
        let (mut d, _) = proc();
        let mut ctl = open(&mut d, 1, "ctl", OWRITE);
        for v in ["start", "stop", "waitstop", "hang"] {
            assert!(d.write(&mut ctl, v.as_bytes(), 0).is_err(), "{v}");
        }
        assert!(d.write(&mut ctl, b"nonsense", 0).is_err());
    }
}
