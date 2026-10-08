//! `#/` — devroot, the root the kernel carries.
//!
//! Plan 9's `devroot.c`: a small read-only directory compiled into the
//! kernel. `#/` holds the directories `rootreset` adds (`devroot.c:95`) for
//! a first process to bind onto, and nothing else.
//!
//! **Where this differs, and why.** Plan 9's `rootdir[]` has a second entry,
//! `boot` (`devroot.c:27`), holding what a first program needs before any
//! file server exists — `addbootfile` puts one in (`:80`) and
//! `initcode` runs `/boot/boot` from it (`initcode.c:11`). Here there is no
//! such program: **the host attaches the root file server before the first
//! program runs** and starts `init` from it (Christine, 2026-10-08: *"The
//! host does it"*), so the kernel carries no files, and the name `boot`,
//! which Unix gives the bootloader, is not used (2026-09-04: *"we can't call
//! something /boot and refer to something other than a bootloader"*).
//!
//! This is the answer to *"How does plan9 handle the root filesystem if ramfs
//! is userspace?"*: it does not have one. The root is a file server.

use crate::chan::Chan;
use crate::dev::{Dev, DevId, Eve};
use crate::ninep::{Qid, QTDIR};

/// One entry. Plan 9's `Dirtab`, with the fields a subset needs.
struct Entry {
    name: String,
    qid: Qid,
    perm: u32,
}

/// `rootdir[]`'s static entry (`devroot.c:27`): `#/`.
const QROOT: u64 = 0;

/// `rootreset` (`devroot.c:95`) — the ten directories every Plan 9 root has,
/// in that order. They are EMPTY and they are the point: `/bin`, `/dev`,
/// `/env`, `/srv` and the rest exist so that the first process has somewhere
/// to bind onto.
const ROOTDIRS: [&str; 10] = [
    "bin", "dev", "env", "fd", "mnt", "net", "net.alt", "proc", "root", "srv",
];

pub struct Root {
    /// `eve` — the kernel-wide host owner (`auth.c:10`), shared rather than
    /// copied, because writing `#c/hostowner` renames it for everyone.
    eve: Eve,

    /// The empty directories of `rootlist` that `addrootdir` adds.
    dirs: Vec<Entry>,
}

impl Default for Root {
    fn default() -> Self {
        Self::new()
    }
}

impl Root {
    pub fn new() -> Root {
        let mut r = Root { dirs: Vec::new(), eve: Eve::default() };
        // `rootreset` — `reset` is one of `struct Dev`'s seventeen, and this
        // is the whole of devroot's.
        for (i, name) in ROOTDIRS.iter().enumerate() {
            r.dirs.push(Entry {
                name: (*name).to_string(),
                // `addlist`: `d->qid.path = ++l->ndir + l->base`, and
                // rootlist's base is 0. Its static entry, `#/`, is counted
                // already, so these start at 2.
                qid: Qid { qtype: QTDIR, vers: 0, path: (i + 2) as u64 },
                perm: crate::ninep::DMDIR | 0o555,
            });
        }
        r
    }

    /// `rootgen` (`devroot.c:116`) — what `#/` holds: the ten `rootreset`
    /// made. **They are EMPTY**: they exist to be bound over, and `rootgen`
    /// generates nothing for them.
    fn entries(&mut self, c: &Chan) -> Vec<crate::ninep::Dir> {
        if c.qid.path != QROOT {
            return Vec::new();
        }
        self.dirs
            .iter()
            .map(|e| crate::dev::devdir(c, e.qid, &e.name, 0, &self.eve.borrow(), &self.eve.borrow(), e.perm))
            .collect()
    }
}

/// `Egreg` (`error.h:44`) — Plan 9's own error for writing where writing
/// makes no sense; `rootwrite`'s (`devroot.c:237`).
const EGREG: &str = "jmk added reentrancy for threads";

/// `Eperm` — `devopen`'s, `devcreate`'s, `devremove`'s and `devwstat`'s.
const EPERM: &str = "permission denied";

impl Dev for Root {
    fn seteve(&mut self, eve: Eve) {
        self.eve = eve;
    }

    fn id(&self) -> DevId {
        DevId::Root
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn attach(&mut self, _spec: &str) -> Result<Chan, String> {
        Ok(Chan::attach(DevId::Root, 0))
    }

    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Chan>, String> {
        if !c.qid.is_dir() {
            return Err("not a directory".into());
        }
        if name == ".." || name == "." {
            // `..` from any of them lands at `#/`, as `/..` is `/`.
            return Ok(Some(c.walked(name, Qid { qtype: QTDIR, vers: 0, path: QROOT })));
        }
        if c.qid.path != QROOT {
            return Ok(None);
        }
        Ok(self.dirs.iter().find(|e| e.name == name).map(|e| c.walked(name, e.qid)))
    }

    /// `rootopen` is `devopen` (`devroot.c:179`). Every entry here is eve's
    /// and `0555` (`addrootdir`), so `devpermcheck` (`dev.c:371`) grants
    /// reading to everyone and writing to nobody, whoever asks; and a
    /// directory opens only to read (`dev.c:379`).
    fn open(&mut self, mut c: Chan, mode: u16) -> Result<Chan, String> {
        if c.qid.is_dir() && mode != crate::chan::mode::OREAD {
            return Err(EPERM.into());
        }
        if mode & 3 != crate::chan::mode::OREAD && mode & 3 != crate::chan::mode::OEXEC {
            return Err(EPERM.into());
        }
        c.mode = crate::chan::openmode(mode)?;
        c.offset = 0;
        Ok(c)
    }

    /// `devcreate` (`dev.c:387`).
    fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
        Err(EPERM.into())
    }

    fn read(&mut self, c: &mut Chan, n: usize, _off: u64) -> Result<Vec<u8>, String> {
        let entries = self.entries(c);
        Ok(crate::dev::devdirread(c, n, &entries))
    }

    fn write(&mut self, _c: &mut Chan, _d: &[u8], _o: u64) -> Result<usize, String> {
        Err(EGREG.into())
    }

    /// `rootstat` is `devstat` over `rootgen` (`devroot.c:171`); every
    /// entry is a directory, which `devstat` names from the path.
    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        Ok(crate::dev::devstatdir(c, &self.eve.borrow()).conv_d2m())
    }

    /// `devwstat` (`dev.c:432`).
    fn wstat(&mut self, _c: &mut Chan, _e: &[u8]) -> Result<(), String> {
        Err(EPERM.into())
    }

    /// `devremove` (`dev.c:426`).
    fn remove(&mut self, _c: &mut Chan) -> Result<(), String> {
        Err(EPERM.into())
    }

    fn close(&mut self, _c: &mut Chan) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `rootreset` (`devroot.c:95`) adds ten empty directories, and they are
    /// what a first process binds onto — and they are all `#/` holds.
    #[test]
    fn the_root_carries_the_ten_directories_rootreset_adds_and_nothing_else() {
        let mut r = Root::new();
        let c = r.attach("").unwrap();
        for name in ROOTDIRS {
            let q = r.walk(&c, name).unwrap().unwrap_or_else(|| panic!("#/{name}"));
            assert!(q.is_dir(), "#/{name} is a directory");
        }
        let mut c = r.open(c, crate::chan::mode::OREAD).unwrap();
        let b = r.read(&mut c, 4096, 0).unwrap();
        let names: Vec<String> = crate::ninep::Dir::parse_all(&b).into_iter().map(|d| d.name).collect();
        assert_eq!(names, ROOTDIRS.to_vec(), "the ten, in rootreset's order");
    }

    /// There is no `boot`: the host attaches the root before the first
    /// program runs (Christine, 2026-10-08), so nothing is carried for one.
    #[test]
    fn there_is_no_boot_directory() {
        let mut r = Root::new();
        let c = r.attach("").unwrap();
        assert_eq!(r.walk(&c, "boot").unwrap(), None);
    }

    /// `rootreset`'s ten are empty: `rootgen` (`devroot.c:116`) generates
    /// nothing for them.
    #[test]
    fn the_empty_root_directories_are_empty() {
        let mut r = Root::new();
        let c = r.attach("").unwrap();
        let mnt = r.walk(&c, "mnt").unwrap().expect("#/mnt");
        assert_eq!(r.walk(&mnt, "anything").unwrap(), None);
        let mut mnt = r.open(mnt, crate::chan::mode::OREAD).unwrap();
        assert!(r.read(&mut mnt, 4096, 0).unwrap().is_empty());
    }

    #[test]
    fn a_name_that_is_not_there_is_not_an_error() {
        // Ok(None) rather than Err: a union walk tries the next element, so
        // "no such name here" is an ordinary answer.
        let mut r = Root::new();
        let c = r.attach("").unwrap();
        assert_eq!(r.walk(&c, "nothing").unwrap(), None);
    }

    #[test]
    fn the_root_cannot_be_written_to() {
        // rootwrite is error(Egreg) in Plan 9, and so is create, wstat and
        // remove here: the kernel's root is what it was built with.
        let mut r = Root::new();
        let mut c = r.attach("").unwrap();
        assert!(r.write(&mut c, b"x", 0).is_err());
        assert!(r.create(&mut c, "x", 0, 0).is_err());
        assert!(r.wstat(&mut c, b"").is_err());
        assert!(r.remove(&mut c).is_err());
    }
}
