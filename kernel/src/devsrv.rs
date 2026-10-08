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
use crate::dev::{strtoul, Dev, DevId};
use crate::ninep::{Qid, QTDIR};
use crate::proc::Up;
use std::cell::RefCell;
use std::rc::Rc;

const EPERM: &str = "permission denied";

/// **The root file server's channel, as posted** — Plan 9's `#s/boot`
/// (`boot/boot.c:144`, `srvcreate("boot", fd)`), named `root` here
/// (Christine, 2026-10-08), because `boot` is the Unix bootloader's name.
/// The host posts it before the first program runs, and every new
/// namespace mounts the root from it (`/profile/start.ns`).
pub const ROOTSRV: &str = "root";
const EEXIST: &str = "file already exists";
const ENONEXIST: &str = "file does not exist";
/// `Eshutdown` (`error.h:7`).
const ESHUTDOWN: &str = "device shut down";
/// `Eisdir` (`error.h:13`).
const EISDIR: &str = "file is a directory";
/// `Ebadchar` (`error.h:14`).
const EBADCHAR: &str = "bad character in file name";
/// `Eshortstat` (`error.h:48`).
const ESHORTSTAT: &str = "stat buffer too small";
/// `Egreg` (`error.h:44`).
const EGREG: &str = "jmk added reentrancy for threads";
/// A posted name opened as a copy — never Plan 9's, whose `srvopen` answers
/// the posted channel itself ([`SrvDev::srvopen`]).
const SHARED: &str = "#s: a posted channel is opened through the device table, which shares it";

/// Plan 9's `Srv`.
pub struct Srv {
    name: String,
    /// `sp->chan` — nil until something posts one. The poster's own
    /// reference, taken from its descriptor: *"c1 = fdtochan(fd, -1, 0, 1);
    /// /* error check and inc ref */"* (`devsrv.c:315`), so the poster may
    /// close its descriptor and the channel stays open.
    chan: Option<std::rc::Rc<std::cell::RefCell<Chan>>>,
    owner: String,
    perm: u32,
    path: u64,
}

impl Srv {
    /// An entry as `srvcreate` plus `srvwrite` leave it: named, and with a
    /// channel behind it.
    pub fn posted(name: &str, chan: Chan) -> Srv {
        Srv {
            name: name.into(),
            chan: Some(std::rc::Rc::new(std::cell::RefCell::new(chan))),
            owner: "eve".into(),
            perm: 0o600,
            path: 0,
        }
    }
}

/// `static Srv *srv` (`devsrv.c:21`) — **one table for the whole kernel**,
/// a file-scope global in Plan 9. `devproc.c` reaches it through `srvname`
/// (declared in `portfns.h`) to print a `mount` line in `#p/<n>/ns`, so it
/// is not devsrv's alone. Shared here, which is what a Rust kernel writes
/// where Plan 9 writes a global.
pub type Srvtab = Rc<RefCell<Vec<Srv>>>;

/// `srvname(Chan *c)` (`devsrv.c`) — the `#s` name a channel was posted
/// under, or `nil`:
///
/// ```c
/// for(sp = srv; sp; sp = sp->link)
///     if(sp->chan == c){
///         snprint(s, size, "#s/%s", sp->name);
/// ```
///
/// Plan 9 compares POINTERS — the posted channel is the same object. Here a
/// channel is a value, so the comparison is on what identifies the file:
/// the device, the instance and the qid, which is `findmount`'s key too.
pub fn srvname(tab: &Srvtab, c: &Chan) -> Option<String> {
    tab.borrow()
        .iter()
        .find(|s| {
            s.chan.as_ref().is_some_and(|p| {
                let p = p.borrow();
                (p.dev, p.devno, p.qid) == (c.dev, c.devno, c.qid)
            })
        })
        .map(|s| format!("#s/{}", s.name))
}

pub struct SrvDev {
    srv: Srvtab,
    /// `eve` — the machine's owner, for `devpermcheck`.
    eve: crate::dev::Eve,
    /// `qidpath` (`srvinit`, `devsrv.c`), counting from 1.
    next: u64,
    up: Rc<RefCell<Up>>,
    /// The posted channels of names just removed, for the device table to
    /// `cclose` — *"if(sp->chan) cclose(sp->chan)"* (`devsrv.c:227`) — since
    /// a device cannot reach `devtab`, and the close of a last reference is
    /// another device's ([`SrvDev::unposted`]).
    unposted: Vec<Rc<RefCell<Chan>>>,
}

impl SrvDev {
    pub fn new(up: Rc<RefCell<Up>>) -> SrvDev {
        SrvDev { srv: Srvtab::default(), eve: Default::default(), next: 1, up, unposted: Vec::new() }
    }

    /// The table, for whoever else needs it — `srvname`'s callers.
    pub fn table(&self) -> Srvtab {
        self.srv.clone()
    }

    /// The channels `srvremove` let go of, to be `cclose`d by the caller
    /// that can reach their devices.
    pub fn unposted(&mut self) -> Vec<Rc<RefCell<Chan>>> {
        std::mem::take(&mut self.unposted)
    }

    /// `srvopen` (`devsrv.c:104`) for a posted name: **the posted channel
    /// itself, with one more reference** — *"cclose(c); incref(sp->chan);
    /// … return sp->chan"* (`:134`–`:138`) — so whoever opens the name
    /// shares the poster's channel, its offset included. The device table
    /// reaches it here ([`crate::namec::Devtab::dopen`]), because an open
    /// that answers a channel that already exists answers a reference.
    ///
    /// A name with nothing behind it, or none, is `Eshutdown` (`:126`); a
    /// posted file cannot be truncated, and opens only in the mode the
    /// posted channel is open in unless that is `ORDWR` (`:128`–`:131`);
    /// then `devpermcheck`.
    pub fn srvopen(&mut self, c: &Chan, mode: u16) -> Result<Rc<RefCell<Chan>>, String> {
        let (posted, owner, perm) = {
            let tab = self.srv.borrow();
            let sp = tab.iter().find(|s| s.path == c.qid.path).ok_or(ESHUTDOWN)?;
            (sp.chan.clone().ok_or(ESHUTDOWN)?, sp.owner.clone(), sp.perm)
        };
        if mode & crate::chan::mode::OTRUNC != 0 {
            return Err("srv file already exists".into());
        }
        let m = crate::chan::openmode(mode)?;
        let pm = posted.borrow().mode;
        if m != pm && pm != crate::chan::mode::ORDWR {
            return Err(EPERM.into());
        }
        self.permcheck(&owner, perm, mode)?;
        Ok(posted)
    }

    /// `devpermcheck` (`dev.c:339`), shared in [`crate::dev::permcheck`].
    fn permcheck(&self, owner: &str, perm: u32, mode: u16) -> Result<(), String> {
        let user = self.up.borrow().user();
        crate::dev::permcheck(&user, owner, &self.eve.borrow(), perm, mode)
    }
}

impl Dev for SrvDev {
    fn seteve(&mut self, eve: crate::dev::Eve) {
        self.eve = eve;
    }

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
        let tab = self.srv.borrow();
        Ok(tab
            .iter()
            .find(|s| s.name == name)
            .map(|s| c.walked(name, Qid { qtype: 0, vers: 0, path: s.path })))
    }

    /// `srvopen` (`devsrv.c:104`) for the directory: it opens only to
    /// read, and never `ORCLOSE`. A posted name is [`SrvDev::srvopen`]'s,
    /// which answers the posted channel itself; a copy of it is not a
    /// channel this device can answer.
    fn open(&mut self, mut c: Chan, mode: u16) -> Result<Chan, String> {
        if c.qid.is_dir() {
            if mode & crate::chan::mode::ORCLOSE != 0 {
                return Err(EPERM.into());
            }
            if mode != crate::chan::mode::OREAD {
                return Err(EISDIR.into());
            }
            c.mode = mode;
            c.flag |= crate::chan::flag::COPEN;
            c.offset = 0;
            return Ok(c);
        }
        Err(SHARED.into())
    }

    /// `srvcreate`: `OWRITE` only, because the file is a place to post into
    /// and there is nothing there to read.
    fn create(&mut self, c: &mut Chan, name: &str, mode: u16, perm: u32) -> Result<(), String> {
        if !c.qid.is_dir() {
            return Err(EPERM.into());
        }
        // *"if(openmode(omode) != OWRITE) error(Eperm)"* (`devsrv.c:147`).
        if crate::chan::openmode(mode)? != crate::chan::mode::OWRITE {
            return Err(EPERM.into());
        }
        if self.srv.borrow().iter().any(|s| s.name == name) {
            return Err(EEXIST.into());
        }
        let path = self.next;
        self.next += 1;
        self.srv.borrow_mut().push(Srv {
            name: name.to_string(),
            chan: None,
            owner: self.up.borrow().user(),
            // *"sp->perm = perm&0777"* (`devsrv.c:179`).
            perm: perm & 0o777,
            path,
        });
        c.qid = Qid { qtype: 0, vers: 0, path };
        c.flag |= crate::chan::flag::COPEN;
        c.mode = crate::chan::mode::OWRITE;
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
            .borrow()
            .iter()
            .map(|sp| (sp.name.clone(), sp.path, sp.owner.clone(), sp.perm))
            .collect();
        list.sort();
        let eve = self.eve.borrow().clone();
        let entries: Vec<crate::ninep::Dir> = list
            .into_iter()
            .map(|(name, path, owner, perm)| {
                let qid = crate::ninep::Qid { qtype: 0, vers: 0, path };
                crate::dev::devdir(c, qid, &name, 0, &owner, &eve, perm)
            })
            .collect();
        Ok(crate::dev::devdirread(c, n, &entries))
    }

    /// `srvwrite` (`devsrv.c:302`): the text is a **file descriptor number**.
    /// `fdtochan` turns it into a channel — the descriptor's own, with one
    /// more reference (*"error check and inc ref"*, `:315`) — and that
    /// channel is what the name now means.
    ///
    /// The number is `strtoul(buf, 0, 0)` of fewer than 32 bytes (`:309`–
    /// `:313`): `0x` is hexadecimal, a leading `0` octal, and what follows
    /// the digits is ignored.
    fn write(&mut self, c: &mut Chan, data: &[u8], _off: u64) -> Result<usize, String> {
        if c.qid.is_dir() {
            return Err(EPERM.into());
        }
        // *"if(n >= sizeof buf) error(Egreg)"*, `char buf[32]`.
        if data.len() >= 32 {
            return Err(EGREG.into());
        }
        let fd = strtoul(data) as i32;
        let fgrp = self.up.borrow().fgrp().ok_or("no such process")?;
        let cell = fgrp.borrow().get(fd).cloned().ok_or("fd out of range or not open")?;
        // A channel that goes away on exec or on close cannot be left behind
        // a name: whoever opens the name later would get a channel its
        // poster no longer holds (`:323`); nor can an authentication file
        // (`:325`).
        {
            let posted = cell.borrow();
            if posted.flag & (flag::CCEXEC | flag::CRCLOSE) != 0 {
                return Err("posted fd has remove-on-close or close-on-exec".into());
            }
            if posted.qid.qtype & crate::ninep::QTAUTH != 0 {
                return Err("cannot post auth file in srv".into());
            }
        }
        let path = c.qid.path;
        let mut tab = self.srv.borrow_mut();
        let sp = tab.iter_mut().find(|s| s.path == path).ok_or(ENONEXIST)?;
        // *"if(sp->chan) error(Ebadusefd)"* (`:331`).
        if sp.chan.is_some() {
            return Err(crate::chan::EBADUSEFD.into());
        }
        sp.chan = Some(cell);
        Ok(data.len())
    }

    /// `srvstat` (`devsrv.c:81`): `devstat` over `srvgen`.
    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        if c.qid.is_dir() {
            return Ok(crate::dev::devstatdir(c, &self.eve.borrow()).conv_d2m());
        }
        let (name, owner, perm) = {
            let tab = self.srv.borrow();
            let sp = tab.iter().find(|s| s.path == c.qid.path).ok_or(ENONEXIST)?;
            (sp.name.clone(), sp.owner.clone(), sp.perm)
        };
        Ok(crate::dev::devdir(c, c.qid, &name, 0, &owner, &self.eve.borrow().clone(), perm).conv_d2m())
    }

    /// `srvwstat` (`devsrv.c:235`): a posted name's owner, or eve, may
    /// change its mode, its owner and its name — in that order, so a name
    /// with a `/` in it is refused with `Ebadchar` (`:269`) after the rest
    /// has been changed.
    fn wstat(&mut self, c: &mut Chan, edir: &[u8]) -> Result<(), String> {
        if c.qid.is_dir() {
            return Err(EPERM.into());
        }
        let user = self.up.borrow().user();
        let iseve = crate::dev::iseve(&self.eve, &user);
        let mut tab = self.srv.borrow_mut();
        let sp = tab.iter_mut().find(|s| s.path == c.qid.path).ok_or(ENONEXIST)?;
        if sp.owner != user && !iseve {
            return Err(EPERM.into());
        }
        let d = crate::ninep::Dir::conv_m2d(edir).ok_or(ESHORTSTAT)?;
        if d.mode != !0 {
            sp.perm = d.mode & 0o777;
        }
        if !d.uid.is_empty() {
            sp.owner = d.uid.clone();
        }
        if !d.name.is_empty() && d.name != sp.name {
            if d.name.contains('/') {
                return Err(EBADCHAR.into());
            }
            sp.name = d.name.clone();
        }
        Ok(())
    }

    /// `srvremove` (`devsrv.c:186`): unposting, by whoever may. *"Only eve
    /// can remove system services. No one can remove #s/boot."* (`:209`) —
    /// the root's channel, which this system posts as `#s/root` (Christine,
    /// 2026-10-08), so that is the name no one can remove —
    /// and *"No removing personal services"* (`:218`): a name others may
    /// not write is its owner's or eve's to remove. The name goes, so a
    /// later open finds nothing rather than a dead server, and the posted
    /// channel is closed (`:227`) — a last reference tells its device, so a
    /// posted pipe end nobody else holds hangs up.
    fn remove(&mut self, c: &mut Chan) -> Result<(), String> {
        if c.qid.is_dir() {
            return Err(EPERM.into());
        }
        let user = self.up.borrow().user();
        let iseve = crate::dev::iseve(&self.eve, &user);
        let eve = self.eve.borrow().clone();
        let path = c.qid.path;
        let mut tab = self.srv.borrow_mut();
        let i = tab.iter().position(|s| s.path == path).ok_or(ENONEXIST)?;
        let sp = &tab[i];
        if sp.owner == eve && !iseve {
            return Err(EPERM.into());
        }
        if sp.name == ROOTSRV {
            return Err(EPERM.into());
        }
        if sp.perm & 7 != 7 && sp.owner != user && !iseve {
            return Err(EPERM.into());
        }
        if let Some(ch) = tab.remove(i).chan {
            self.unposted.push(ch);
        }
        Ok(())
    }

    /// `srvclose` (`devsrv.c:279`): a channel opened with `ORCLOSE` unposts
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
        let fd = procs.borrow().get(1).unwrap().fds.as_ref().unwrap().borrow_mut().add(served);

        let mut dir = d.attach("").unwrap();
        d.create(&mut dir, "store", OWRITE, 0o600).unwrap();
        d.write(&mut dir, fd.to_string().as_bytes(), 0).unwrap();

        // a walk from a fresh attach, as a process that inherited nothing does
        let dir2 = d.attach("").unwrap();
        let c = d.walk(&dir2, "store").unwrap().expect("not posted");
        let got = d.srvopen(&c, OREAD).unwrap();
        let held = procs.borrow().get(1).unwrap().fds.as_ref().unwrap().borrow().get(fd).cloned().unwrap();
        assert!(Rc::ptr_eq(&got, &held), "the posted channel itself, not a copy");
        assert!(d.open(c, OREAD).is_err(), "and never a copy");
    }

    /// **A posted channel opens only in its own mode** (`devsrv.c:130`):
    /// one open for reading is not opened for writing by name — unless it is
    /// `ORDWR` — and a posted file cannot be truncated (`:128`); the
    /// directory opens only to read.
    #[test]
    fn a_posted_channel_opens_only_in_the_mode_it_is_open_in() {
        let (mut d, procs) = srv();
        let mut served = Chan::attach(DevId::Pipe, 7);
        served.mode = OREAD;
        let fd = procs.borrow().get(1).unwrap().fds.as_ref().unwrap().borrow_mut().add(served);
        let mut dir = d.attach("").unwrap();
        d.create(&mut dir, "ro", OWRITE, 0o666).unwrap();
        d.write(&mut dir, fd.to_string().as_bytes(), 0).unwrap();
        let dir2 = d.attach("").unwrap();
        let c = d.walk(&dir2, "ro").unwrap().unwrap();
        assert!(d.srvopen(&c, OWRITE).is_err(), "posted for reading");
        assert!(d.srvopen(&c, OREAD | crate::chan::mode::OTRUNC).is_err());
        assert!(d.srvopen(&c, OREAD).is_ok());
        let top = d.attach("").unwrap();
        assert!(d.open(top, OWRITE).is_err(), "Eisdir");
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
        assert_eq!(d.srvopen(&dir, OREAD).unwrap_err(), ESHUTDOWN);
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
            let fd = procs.borrow().get(1).unwrap().fds.as_ref().unwrap().borrow_mut().add(c);
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
