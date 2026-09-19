//! `#s` — srv (`plan9/sys/src/9/port/devsrv.c`).
//!
//! **How a server becomes reachable by a process that did not inherit it.**
//! A file descriptor is a channel, and a channel is private to a namespace; a
//! program that serves files has no way to hand one to a stranger. `#s` is
//! that way: create a file, write a file descriptor's NUMBER to it, and the
//! kernel keeps the channel behind the name. Anyone who can open the name
//! gets the channel.
//!
//! So the whole device is three moves (`devsrv.c`):
//!
//! | | |
//! |---|---|
//! | `srvcreate` | make an empty entry. `OWRITE` only — there is nothing to read yet |
//! | `srvwrite` | `fd = strtoul(buf)`, then `fdtochan(fd, ...)`. **The text is a number**, not a path |
//! | `srvopen` | `return sp->chan` — the POSTED channel, closing the one it was given |
//!
//! That last line is why `Dev::open` returns a channel.
//!
//! One table for the whole kernel, which is why rio names its posted file
//! `/srv/riowctl.%s.%d` with the user and the pid (`rio/fsys.c:152`): a fixed
//! name would collide the moment a second instance started.

use crate::chan::{flag, Chan};
use crate::dev::{Dev, DevId};
use crate::ninep::{Qid, QTDIR};
use crate::proc::Up;
use std::cell::RefCell;
use std::rc::Rc;

const EPERM: &str = "permission denied";
const EEXIST: &str = "file already exists";
const ENONEXIST: &str = "file does not exist";
const ESHUTDOWN: &str = "channel shut down";

/// Plan 9's `Srv`.
struct Srv {
    name: String,
    /// `sp->chan` — nil until something posts one.
    chan: Option<Chan>,
    owner: String,
    perm: u32,
    path: u64,
}

pub struct SrvDev {
    srv: Vec<Srv>,
    /// `eve` — the machine's owner, for `devpermcheck`.
    pub eve: String,
    /// `qidpath` (`srvinit`, `devsrv.c`), counting from 1.
    next: u64,
    up: Rc<RefCell<Up>>,
}

impl SrvDev {
    pub fn new(up: Rc<RefCell<Up>>) -> SrvDev {
        SrvDev { srv: Vec::new(), eve: "eve".into(), next: 1, up }
    }

    fn lookup(&self, path: u64) -> Option<&Srv> {
        self.srv.iter().find(|s| s.path == path)
    }

    /// `devpermcheck` (`dev.c:339`), shared in [`crate::dev::permcheck`].
    fn permcheck(&self, owner: &str, perm: u32, mode: u16) -> Result<(), String> {
        let user = self.up.borrow().user();
        crate::dev::permcheck(&user, owner, &self.eve, perm, mode)
    }
}

impl Dev for SrvDev {
    fn id(&self) -> DevId {
        DevId::Srv
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn attach(&mut self, _spec: &str) -> Result<Chan, String> {
        Ok(Chan::attach(DevId::Srv, 0))
    }

    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Chan>, String> {
        if !c.qid.is_dir() {
            return Err("not a directory".into());
        }
        if name == ".." || name == "." {
            return Ok(Some(c.walked(name, Qid { qtype: QTDIR, vers: 0, path: 0 })));
        }
        Ok(self
            .srv
            .iter()
            .find(|s| s.name == name)
            .map(|s| c.walked(name, Qid { qtype: 0, vers: 0, path: s.path })))
    }

    /// `srvopen`: **returns the posted channel**, not the one it was given.
    /// A name with nothing behind it is `Eshutdown` — the entry exists and
    /// the server is gone, which is a different thing from no such file.
    fn open(&mut self, mut c: Chan, mode: u16) -> Result<Chan, String> {
        if c.qid.is_dir() {
            c.mode = mode;
            return Ok(c);
        }
        let (posted, owner, perm) = {
            let sp = self.lookup(c.qid.path).ok_or(ENONEXIST)?;
            (sp.chan.clone().ok_or(ESHUTDOWN)?, sp.owner.clone(), sp.perm)
        };
        self.permcheck(&owner, perm, mode)?;
        Ok(posted)
    }

    /// `srvcreate`: `OWRITE` only, because the file is a place to post into
    /// and there is nothing there to read.
    fn create(&mut self, c: &mut Chan, name: &str, mode: u16, perm: u32) -> Result<(), String> {
        if !c.qid.is_dir() {
            return Err(EPERM.into());
        }
        if mode & 3 != crate::chan::mode::OWRITE {
            return Err(EPERM.into());
        }
        if self.srv.iter().any(|s| s.name == name) {
            return Err(EEXIST.into());
        }
        let path = self.next;
        self.next += 1;
        self.srv.push(Srv {
            name: name.to_string(),
            chan: None,
            owner: self.up.borrow().user(),
            perm,
            path,
        });
        c.qid = Qid { qtype: 0, vers: 0, path };
        c.mode = mode;
        Ok(())
    }

    fn read(&mut self, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String> {
        if !c.qid.is_dir() {
            // A posted file is opened, never read: `srvopen` handed the
            // caller the server's own channel, so a read here would be a
            // read of the door rather than through it.
            return Err(EPERM.into());
        }
        let _ = off;
        // `srvgen` (`devsrv.c:60`) — one entry per posted name, owned by
        // whoever posted it.
        let mut list: Vec<(String, u64, String, u32)> = self
            .srv
            .iter()
            .map(|sp| (sp.name.clone(), sp.path, sp.owner.clone(), sp.perm))
            .collect();
        list.sort();
        let eve = self.eve.clone();
        let entries: Vec<crate::ninep::Dir> = list
            .into_iter()
            .map(|(name, path, owner, perm)| {
                let qid = crate::ninep::Qid { qtype: 0, vers: 0, path };
                crate::dev::devdir(c, qid, &name, 0, &owner, &eve, perm)
            })
            .collect();
        Ok(crate::dev::devdirread(c, n, &entries))
    }

    /// `srvwrite`: the text is a **file descriptor number**. `fdtochan` turns
    /// it into a channel, and that channel is what the name now means.
    fn write(&mut self, c: &mut Chan, data: &[u8], _off: u64) -> Result<usize, String> {
        if c.qid.is_dir() {
            return Err(EPERM.into());
        }
        let text = String::from_utf8_lossy(data);
        let fd: i32 = text.trim().parse().map_err(|_| "bad fd")?;
        let fgrp = self.up.borrow().fgrp().ok_or("no such process")?;
        let cell = fgrp.borrow().get(fd).cloned().ok_or("fd out of range or not open")?;
        let posted = cell.borrow().clone();
        // `srvwrite` (`devsrv.c:323`). A channel that goes away on exec or on
        // close cannot be left behind a name: whoever opens the name later
        // would get a channel its poster no longer holds.
        if posted.flag & (flag::CCEXEC | flag::CRCLOSE) != 0 {
            return Err("posted fd has remove-on-close or close-on-exec".into());
        }
        let path = c.qid.path;
        let sp = self.srv.iter_mut().find(|s| s.path == path).ok_or(ENONEXIST)?;
        if sp.chan.is_some() {
            return Err(EEXIST.into());
        }
        sp.chan = Some(posted);
        Ok(data.len())
    }

    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        let (name, owner, perm) = if c.qid.is_dir() {
            ("#s".to_string(), self.eve.clone(), crate::ninep::DMDIR | 0o555)
        } else {
            let sp = self.lookup(c.qid.path).ok_or(ENONEXIST)?;
            (sp.name.clone(), sp.owner.clone(), sp.perm)
        };
        Ok(crate::dev::devdir(c, c.qid, &name, 0, &owner, &self.eve.clone(), perm).conv_d2m())
    }

    fn wstat(&mut self, _c: &mut Chan, _e: &[u8]) -> Result<(), String> {
        Err(EPERM.into())
    }

    /// `srvremove`: unposting. The name goes and the channel with it, so a
    /// later open finds nothing rather than a dead server.
    fn remove(&mut self, c: &mut Chan) -> Result<(), String> {
        if c.qid.is_dir() {
            return Err(EPERM.into());
        }
        let path = c.qid.path;
        let i = self.srv.iter().position(|s| s.path == path).ok_or(ENONEXIST)?;
        self.srv.remove(i);
        Ok(())
    }

    /// `srvclose` (`devsrv.c:280`): a channel opened with `ORCLOSE` unposts
    /// its name when it closes. The comment there notes why no re-check is
    /// needed — only the owner is checked, and an owner is immutable.
    fn close(&mut self, c: &mut Chan) {
        if c.flag & flag::CRCLOSE != 0 {
            let _ = self.remove(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chan::mode::{OREAD, OWRITE};
    use crate::proc::Procs;

    fn srv() -> (SrvDev, Rc<RefCell<Procs>>) {
        let procs = Rc::new(RefCell::new(Procs::new(Chan::attach(DevId::Root, 0))));
        let up = Rc::new(RefCell::new(Up { pid: 1, procs: procs.clone() }));
        (SrvDev::new(up), procs)
    }

    /// Post a channel and pick it up again by name. This is the whole point:
    /// the second process never had the channel and did not inherit it.
    #[test]
    fn a_posted_channel_is_what_an_open_of_the_name_answers() {
        let (mut d, procs) = srv();

        // something worth posting — a channel with a recognisable qid
        let mut served = Chan::attach(DevId::Pipe, 7);
        served.qid = Qid { qtype: 0, vers: 0, path: 99 };
        let fd = procs.borrow().get(1).unwrap().fds.borrow_mut().add(served);

        let mut dir = d.attach("").unwrap();
        d.create(&mut dir, "store", OWRITE, 0o600).unwrap();
        d.write(&mut dir, fd.to_string().as_bytes(), 0).unwrap();

        // a walk from a fresh attach, as a process that inherited nothing does
        let dir2 = d.attach("").unwrap();
        let c = d.walk(&dir2, "store").unwrap().expect("not posted");
        let got = d.open(c, OREAD).unwrap();
        assert_eq!((got.dev, got.devno, got.qid.path), (DevId::Pipe, 7, 99));
    }

    /// The file is a place to post INTO, so it is created for writing only.
    #[test]
    fn a_srv_file_is_created_for_writing_only() {
        let (mut d, _) = srv();
        let mut dir = d.attach("").unwrap();
        assert!(d.create(&mut dir, "x", OREAD, 0o600).is_err());
        assert!(d.create(&mut dir, "x", OWRITE, 0o600).is_ok());
        assert!(d.create(&mut dir, "x", OWRITE, 0o600).is_err(), "Eexist");
    }

    /// A name with nothing behind it is Eshutdown, which is not the same as
    /// no such file — the entry exists and the server is gone.
    #[test]
    fn an_unposted_name_is_shut_down_not_missing() {
        let (mut d, _) = srv();
        let mut dir = d.attach("").unwrap();
        d.create(&mut dir, "empty", OWRITE, 0o600).unwrap();
        let c = dir.clone();
        assert_eq!(d.open(c, OREAD).unwrap_err(), ESHUTDOWN);
    }

    /// What is written is a file descriptor NUMBER, not a path.
    #[test]
    fn the_text_written_is_a_file_descriptor_number() {
        let (mut d, _) = srv();
        let mut dir = d.attach("").unwrap();
        d.create(&mut dir, "x", OWRITE, 0o600).unwrap();
        assert!(d.write(&mut dir, b"/some/path", 0).is_err(), "not a path");
        assert!(d.write(&mut dir, b"41", 0).is_err(), "no such fd");
    }

    /// One table for the whole kernel — so the directory lists everything
    /// posted, and that is why rio puts its pid in the name.
    #[test]
    fn the_directory_lists_everything_posted() {
        let (mut d, _) = srv();
        let mut dir = d.attach("").unwrap();
        d.create(&mut dir, "riowctl.kitty.12", OWRITE, 0o600).unwrap();
        let mut dir2 = d.attach("").unwrap();
        d.create(&mut dir2, "factotum", OWRITE, 0o600).unwrap();
        let mut root = d.attach("").unwrap();
        let b = d.read(&mut root, 256, 0).unwrap();
        let names: Vec<String> =
            crate::ninep::Dir::parse_all(&b).into_iter().map(|d| d.name).collect();
        assert_eq!(names, ["factotum", "riowctl.kitty.12"]);
    }

    /// `srvwrite` (`devsrv.c:323`) refuses a channel that goes away on exec
    /// or on close: whoever opens the name later would get a channel its
    /// poster no longer holds.
    #[test]
    fn a_channel_that_vanishes_cannot_be_posted() {
        for bad in [flag::CCEXEC, flag::CRCLOSE] {
            let (mut d, procs) = srv();
            let mut c = Chan::attach(DevId::Pipe, 7);
            c.flag = bad;
            let fd = procs.borrow().get(1).unwrap().fds.borrow_mut().add(c);
            let mut dir = d.attach("").unwrap();
            d.create(&mut dir, "x", OWRITE, 0o600).unwrap();
            let e = d.write(&mut dir, fd.to_string().as_bytes(), 0).unwrap_err();
            assert!(e.contains("remove-on-close or close-on-exec"), "{e}");
        }
    }

    /// `srvclose` (`devsrv.c:286`): a name opened with `ORCLOSE` unposts
    /// itself when the channel closes.
    #[test]
    fn a_name_opened_with_orclose_unposts_itself() {
        let (mut d, _) = srv();
        let mut dir = d.attach("").unwrap();
        d.create(&mut dir, "gone", OWRITE, 0o600).unwrap();
        dir.flag |= flag::CRCLOSE;
        d.close(&mut dir);
        let root = d.attach("").unwrap();
        assert!(d.walk(&root, "gone").unwrap().is_none(), "close did not unpost it");
    }

    /// Unposting removes the name, so a later open finds nothing at all.
    #[test]
    fn removing_the_file_unposts_it() {
        let (mut d, _) = srv();
        let mut dir = d.attach("").unwrap();
        d.create(&mut dir, "gone", OWRITE, 0o600).unwrap();
        d.remove(&mut dir).unwrap();
        let root = d.attach("").unwrap();
        assert!(d.walk(&root, "gone").unwrap().is_none());
    }
}
