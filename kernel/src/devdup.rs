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
/// `Eisdir` (`error.h:13`).
const EISDIR: &str = "file is a directory";
/// A descriptor's file opened as a copy — never Plan 9's, whose `dupopen`
/// answers the descriptor's channel itself ([`DupDev::dupopen`]).
const SHARED: &str = "#d: a descriptor's file is opened through the device table, which shares it";

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

    fn chan(&self, fd: Fd) -> Option<Rc<RefCell<Chan>>> {
        let fgrp = self.up.borrow().fgrp()?;
        let cell = fgrp.borrow().get(fd).cloned();
        cell
    }

    /// `dupopen` (`devdup.c:61`) for a descriptor's file: **the channel the
    /// descriptor holds, with one more reference** —
    /// `fdtochan(fd, openmode(omode), 0, 1)` (`:86`) — so `open("#d/3", …)`
    /// and `dup(3, -1)` are the same act by two names, and the two share
    /// one offset. Refused unless the channel is open in the mode asked for:
    /// one open for reading does not become one open for writing by being
    /// opened again here. The device table reaches it here
    /// ([`crate::namec::Devtab::dopen`]).
    pub fn dupopen(&mut self, c: &Chan, mode: u16) -> Result<Rc<RefCell<Chan>>, String> {
        let (fd, _) = Self::slot(c.qid.path).ok_or(EBADFD)?;
        let m = crate::chan::openmode(mode)?;
        let got = self.chan(fd).ok_or(EBADFD)?;
        crate::chan::fdcheck(&got.borrow(), Some(m), false)?;
        Ok(got)
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

    /// `dupopen` (`devdup.c:61`) for the directory, which opens only to
    /// read — *"if(omode != 0) error(Eisdir)"* — and for a ctl file, which
    /// is this device's own. A descriptor's file is [`DupDev::dupopen`]'s,
    /// which answers the descriptor's channel itself.
    fn open(&mut self, mut c: Chan, mode: u16) -> Result<Chan, String> {
        if c.qid.is_dir() {
            if mode != 0 {
                return Err(EISDIR.into());
            }
            c.mode = 0;
            c.flag |= crate::chan::flag::COPEN;
            c.offset = 0;
            return Ok(c);
        }
        let (fd, ctl) = Self::slot(c.qid.path).ok_or(EBADFD)?;
        let m = crate::chan::openmode(mode)?;
        if ctl {
            // the ctl file is this device's own, and reports the mode
            c.mode = m;
            c.flag |= crate::chan::flag::COPEN;
            c.offset = 0;
            return Ok(c);
        }
        let _ = fd;
        Err(SHARED.into())
    }

    fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
        Err(EPERM.into())
    }

    fn read(&mut self, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String> {
        if c.qid.is_dir() {
            // `dupgen` (`devdup.c:34`): every open slot, as `n` and `nctl`,
            // eve's (`:38`).
            let fgrp = self.up.borrow().fgrp().ok_or("no such process")?;
            let open: Vec<Fd> = {
                let g = fgrp.borrow();
                (0..g.slots()).filter(|fd| g.get(*fd).is_some()).collect()
            };
            let eve_ = self.eve.borrow().clone();
            let mut entries = Vec::new();
            for fd in open {
                let perm = self.chan(fd).map(|got| PERM[(got.borrow().mode & 3) as usize]).unwrap_or(0);
                let q = Qid { qtype: 0, vers: 0, path: Self::qid(fd, false) };
                let qctl = Qid { qtype: 0, vers: 0, path: Self::qid(fd, true) };
                entries.push(crate::dev::devdir(c, q, &format!("{fd}"), 0, &eve_, &eve_, perm));
                entries.push(crate::dev::devdir(c, qctl, &format!("{fd}ctl"), 0, &eve_, &eve_, 0o400));
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
            // *"procfdprint(c, fd, 0, buf, sizeof buf)"* (`dupread`): the
            // descriptor as `/proc/n/fd` shows it.
            let got = self.chan(fd).ok_or(EBADFD)?;
            let got = got.borrow();
            crate::devproc::procfdprint(&got, fd, 0)
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
        let eve = self.eve.borrow().clone();
        if c.qid.is_dir() {
            return Ok(crate::dev::devstatdir(c, &eve).conv_d2m());
        }
        let (name, perm) = {
            let (fd, ctl) = Self::slot(c.qid.path).ok_or(EBADFD)?;
            if ctl {
                (format!("{fd}ctl"), 0o400)
            } else {
                let got = self.chan(fd).ok_or(EBADFD)?;
                let m = got.borrow().mode;
                (format!("{fd}"), PERM[(m & 3) as usize])
            }
        };
        Ok(crate::dev::devdir(c, c.qid, &name, 0, &eve, &eve, perm).conv_d2m())
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

    /// The device's whole reason: opening `#d/3` answers with **the channel
    /// fd 3 holds** — the same one, one more reference to it — so a dup is
    /// an ordinary open of an ordinary file (`devdup.c:86`).
    #[test]
    fn opening_the_file_answers_with_the_channel_the_fd_holds() {
        let (mut d, procs) = dup();
        let fd = openfd(&procs, 7, 99);
        let dir = d.attach("").unwrap();
        let c = d.walk(&dir, &fd.to_string()).unwrap().expect("no such slot");
        let got = d.dupopen(&c, OREAD).unwrap();
        let held = procs.borrow().get(1).unwrap().fds.borrow().get(fd).cloned().unwrap();
        assert!(Rc::ptr_eq(&got, &held), "the fd's own channel, not a copy of it");
        assert!(d.open(c, OREAD).is_err(), "and never a copy");
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

    /// The ctl file is the device's own, so opening it does NOT substitute
    /// the fd's channel; it reads as the descriptor's line in
    /// `/proc/n/fd` — *"procfdprint(c, fd, 0, buf, sizeof buf)"*
    /// (`dupread`).
    #[test]
    fn the_ctl_file_stays_this_devices_and_reports_the_descriptor() {
        let (mut d, procs) = dup();
        let fd = openfd(&procs, 7, 99);
        let dir = d.attach("").unwrap();
        let c = d.walk(&dir, &format!("{fd}ctl")).unwrap().unwrap();
        let mut got = d.open(c, OREAD).unwrap();
        assert_eq!(got.dev, DevId::Dup, "the ctl file is #d's own");
        let line = String::from_utf8(d.read(&mut got, 256, 0).unwrap()).unwrap();
        assert!(line.starts_with("  0 rw |    7 (0000000000000063 0 00)     0        0 "), "{line:?}");
        assert!(line.ends_with('\n'));
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
