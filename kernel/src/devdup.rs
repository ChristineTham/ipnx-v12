//! `#d` — dup (`plan9/sys/src/9/port/devdup.c`).
//!
//! **A process's own file descriptors as files**, and the reason it exists is
//! `dupopen`: opening `#d/3` calls `fdtochan(3, ...)` and returns **the
//! channel fd 3 already holds**, closing the one it was given. So `dup` is not
//! a special call here — it is an ordinary open of an ordinary file, and
//! `/fd/0` is standard input because `bind #d /fd` put it there
//! (`lib/namespace`).
//!
//! The directory is generated, not stored: `dupgen` (`devdup.c:11`) walks
//! `up->fgrp` and names slot `n` twice — `n` for the channel and `nctl` for
//! its mode. The qid is `2n+1` for the file and `2n+2` for its ctl, which is
//! `mkqid(&q, s+1, ...)` over a doubled index.

use crate::chan::Chan;
use crate::dev::{Dev, DevId, Eve};
use crate::ninep::{Qid, QTDIR};
use crate::proc::{Fd, Up};
use std::cell::RefCell;
use std::rc::Rc;

const EPERM: &str = "permission denied";

/// What a descriptor's file allows, by the mode it was opened with
/// (`devdup.c:44`: `dupgen` reads `c->mode` and answers 0400, 0200 or 0600).
const PERM: [u32; 4] = [0o400, 0o200, 0o600, 0];
const EBADFD: &str = "fd out of range or not open";

pub struct DupDev {
    /// `eve` — the kernel-wide host owner (`auth.c:10`), shared rather than
    /// copied, because writing `#c/hostowner` renames it for everyone.
    eve: Eve,

    up: Rc<RefCell<Up>>,
}

impl DupDev {
    pub fn new(up: Rc<RefCell<Up>>) -> DupDev {
        DupDev { up, eve: Eve::default() }
    }

    /// `twicefd = c->qid.path - 1; fd = twicefd/2` — and the odd one is the
    /// ctl file (`dupopen`, `devdup.c`).
    fn slot(qid: u64) -> Option<(Fd, bool)> {
        if qid == 0 {
            return None;
        }
        let twice = qid - 1;
        Some(((twice / 2) as Fd, twice & 1 == 1))
    }

    fn qid(fd: Fd, ctl: bool) -> u64 {
        (fd as u64) * 2 + if ctl { 2 } else { 1 }
    }

    fn chan(&self, fd: Fd) -> Option<Chan> {
        let fgrp = self.up.borrow().fgrp()?;
        let cell = fgrp.borrow().get(fd).cloned()?;
        let c = cell.borrow().clone();
        Some(c)
    }
}

impl Dev for DupDev {
    fn seteve(&mut self, eve: Eve) {
        self.eve = eve;
    }

    fn id(&self) -> DevId {
        DevId::Dup
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn attach(&mut self, _spec: &str) -> Result<Chan, String> {
        Ok(Chan::attach(DevId::Dup, 0))
    }

    /// Names are `n` and `nctl`, generated from the fd table rather than
    /// stored — so a walk to a slot that is not open finds nothing.
    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Chan>, String> {
        if !c.qid.is_dir() {
            return Err("not a directory".into());
        }
        if name == ".." || name == "." {
            return Ok(Some(c.walked(name, Qid { qtype: QTDIR, vers: 0, path: 0 })));
        }
        let (digits, ctl) = match name.strip_suffix("ctl") {
            Some(d) => (d, true),
            None => (name, false),
        };
        let fd: Fd = match digits.parse() {
            Ok(fd) => fd,
            Err(_) => return Ok(None),
        };
        if self.chan(fd).is_none() {
            return Ok(None);
        }
        Ok(Some(c.walked(name, Qid { qtype: 0, vers: 0, path: Self::qid(fd, ctl) })))
    }

    /// **The whole device.** `dupopen` returns the channel the fd holds, so
    /// `open("#d/3", ...)` and `dup(3, -1)` are the same act by two names.
    fn open(&mut self, mut c: Chan, mode: u16) -> Result<Chan, String> {
        if c.qid.is_dir() {
            c.mode = mode;
            return Ok(c);
        }
        let (fd, ctl) = Self::slot(c.qid.path).ok_or(EBADFD)?;
        if ctl {
            // the ctl file is this device's own, and reports the mode
            c.mode = mode;
            return Ok(c);
        }
        let mut got = self.chan(fd).ok_or(EBADFD)?;
        got.mode = mode;
        Ok(got)
    }

    fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
        Err(EPERM.into())
    }

    fn read(&mut self, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String> {
        if c.qid.is_dir() {
            // `dupgen` (`devdup.c:34`): every open slot, as `n` and `nctl`.
            let user = self.up.borrow().user();
            let fgrp = self.up.borrow().fgrp().ok_or("no such process")?;
            let open: Vec<Fd> = {
                let g = fgrp.borrow();
                (0..g.slots()).filter(|fd| g.get(*fd).is_some()).collect()
            };
            let eve_ = self.eve.borrow().clone();
            let mut entries = Vec::new();
            for fd in open {
                let perm = self.chan(fd).map(|got| PERM[(got.mode & 3) as usize]).unwrap_or(0);
                let q = Qid { qtype: 0, vers: 0, path: Self::qid(fd, false) };
                let qctl = Qid { qtype: 0, vers: 0, path: Self::qid(fd, true) };
                entries.push(crate::dev::devdir(c, q, &format!("{fd}"), 0, &user, &eve_, perm));
                entries.push(crate::dev::devdir(
                    c,
                    qctl,
                    &format!("{fd}ctl"),
                    0,
                    &user,
                    &self.eve.borrow(),
                    0o400,
                ));
            }
            return Ok(crate::dev::devdirread(c, n, &entries));
        }
        let s = {
            let (fd, ctl) = Self::slot(c.qid.path).ok_or(EBADFD)?;
            if !ctl {
                // Reading `#d/3` itself never happens: the open answered with
                // fd 3's channel, so a read goes to whatever that serves.
                return Err(EPERM.into());
            }
            let got = self.chan(fd).ok_or(EBADFD)?;
            format!("{}\n", got.mode & 3)
        };
        let b = s.into_bytes();
        let off = off as usize;
        if off >= b.len() {
            return Ok(Vec::new());
        }
        Ok(b[off..(off + n).min(b.len())].to_vec())
    }

    fn write(&mut self, _c: &mut Chan, _d: &[u8], _o: u64) -> Result<usize, String> {
        Err(EPERM.into())
    }

    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        // `perm[] = { 0400, 0200, 0600, 0 }` indexed by the channel's mode
        // (`dupgen`, `devdup.c:14`); a ctl file is 0400.
        let (name, perm) = if c.qid.is_dir() {
            ("#d".to_string(), crate::ninep::DMDIR | 0o555)
        } else {
            let (fd, ctl) = Self::slot(c.qid.path).ok_or(EBADFD)?;
            if ctl {
                (format!("{fd}ctl"), 0o400)
            } else {
                let got = self.chan(fd).ok_or(EBADFD)?;
                (format!("{fd}"), PERM[(got.mode & 3) as usize])
            }
        };
        let user = self.up.borrow().user();
        Ok(crate::dev::devdir(c, c.qid, &name, 0, &user, &self.eve.borrow(), perm).conv_d2m())
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
    use crate::chan::mode::{OREAD, ORDWR};
    use crate::proc::Procs;

    fn dup() -> (DupDev, Rc<RefCell<Procs>>) {
        let procs = Rc::new(RefCell::new(Procs::new(Chan::attach(DevId::Root, 0))));
        let up = Rc::new(RefCell::new(Up { pid: 1, procs: procs.clone() }));
        (DupDev::new(up), procs)
    }

    fn openfd(procs: &Rc<RefCell<Procs>>, devno: u32, qid: u64) -> Fd {
        let mut c = Chan::attach(DevId::Pipe, devno);
        c.qid = Qid { qtype: 0, vers: 0, path: qid };
        c.mode = ORDWR;
        procs.borrow().get(1).unwrap().fds.borrow_mut().add(c)
    }

    /// The device's whole reason: opening `#d/3` answers with the channel fd
    /// 3 holds, so a dup is an ordinary open of an ordinary file.
    #[test]
    fn opening_the_file_answers_with_the_channel_the_fd_holds() {
        let (mut d, procs) = dup();
        let fd = openfd(&procs, 7, 99);
        let dir = d.attach("").unwrap();
        let c = d.walk(&dir, &fd.to_string()).unwrap().expect("no such slot");
        let got = d.open(c, OREAD).unwrap();
        assert_eq!(
            (got.dev, got.devno, got.qid.path),
            (DevId::Pipe, 7, 99),
            "the open must answer with the fd's own channel, not the #d file"
        );
    }

    /// A slot that is not open has no file, because the directory is
    /// generated from the fd table rather than stored.
    #[test]
    fn a_slot_that_is_not_open_has_no_file() {
        let (mut d, procs) = dup();
        let dir = d.attach("").unwrap();
        assert!(d.walk(&dir, "0").unwrap().is_none());
        openfd(&procs, 1, 1);
        assert!(d.walk(&dir, "0").unwrap().is_some());
        assert!(d.walk(&dir, "9").unwrap().is_none());
        assert!(d.walk(&dir, "notanumber").unwrap().is_none());
    }

    /// `2n+1` for the file, `2n+2` for its ctl (`dupopen`'s `twicefd`).
    #[test]
    fn the_qid_encodes_the_slot_and_which_of_the_two_files() {
        assert_eq!(DupDev::slot(1), Some((0, false)));
        assert_eq!(DupDev::slot(2), Some((0, true)));
        assert_eq!(DupDev::slot(7), Some((3, false)));
        assert_eq!(DupDev::slot(8), Some((3, true)));
        assert_eq!(DupDev::slot(0), None, "the directory is not a slot");
    }

    /// The ctl file is the device's own and reports the channel's mode, so
    /// opening it does NOT substitute the fd's channel.
    #[test]
    fn the_ctl_file_stays_this_devices_and_reports_the_mode() {
        let (mut d, procs) = dup();
        let fd = openfd(&procs, 7, 99);
        let dir = d.attach("").unwrap();
        let c = d.walk(&dir, &format!("{fd}ctl")).unwrap().unwrap();
        let mut got = d.open(c, OREAD).unwrap();
        assert_eq!(got.dev, DevId::Dup, "the ctl file is #d's own");
        assert_eq!(d.read(&mut got, 16, 0).unwrap(), b"2\n", "ORDWR");
    }

    /// Reading the directory names every open slot twice, which is what
    /// `ls /fd` shows.
    #[test]
    fn the_directory_names_every_open_slot_twice() {
        let (mut d, procs) = dup();
        openfd(&procs, 1, 1);
        openfd(&procs, 2, 2);
        let mut dir = d.attach("").unwrap();
        let b = d.read(&mut dir, 256, 0).unwrap();
        let all = crate::ninep::Dir::parse_all(&b);
        let names: Vec<&str> = all.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["0", "0ctl", "1", "1ctl"]);
        // The mode a descriptor was opened with, as `dupgen` reports it.
        assert_eq!(all[0].mode, 0o600, "fd 0 was opened ORDWR");
        assert_eq!(all[1].mode, 0o400, "and its ctl file is read-only");
        assert_eq!(all[0].dtype, 'd' as u16, "the device letter");
    }

    /// Nothing here is created, written or removed: the files ARE the fd
    /// table, and it is changed by opening and closing things.
    #[test]
    fn the_files_are_the_fd_table_and_not_editable_here() {
        let (mut d, procs) = dup();
        openfd(&procs, 1, 1);
        let mut dir = d.attach("").unwrap();
        assert!(d.create(&mut dir, "9", 0, 0).is_err());
        assert!(d.write(&mut dir, b"x", 0).is_err());
        assert!(d.remove(&mut dir).is_err());
    }
}
