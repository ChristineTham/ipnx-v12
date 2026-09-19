//! `#/` — devroot, the root the kernel carries.
//!
//! Plan 9's `devroot.c`: a small read-only directory compiled into the kernel,
//! holding the files boot needs before any file server exists. `addbootfile`
//! puts one in; `rootwrite` is `error(Egreg)` — it cannot be written, ever.
//!
//! This is the answer to *"How does plan9 handle the root filesystem if ramfs
//! is userspace?"*: it does not have one. It carries just enough to start the
//! first process, and that process mounts the real thing.

use crate::chan::Chan;
use crate::dev::{Dev, DevId, EVE};
use crate::ninep::{Qid, QTDIR};

/// One entry. Plan 9's `Dirtab`, with the fields a subset needs.
struct Entry {
    name: String,
    qid: Qid,
    data: Vec<u8>,
    perm: u32,
}

/// `rootdir[]`'s two static entries (`devroot.c:27`): `#/` and `boot`.
const QROOT: u64 = 0;
const QBOOT: u64 = 0x1000;

/// `rootreset` (`devroot.c:95`) — the ten directories every Plan 9 root has,
/// in that order. They are EMPTY and they are the point: `/bin`, `/dev`,
/// `/env`, `/srv` and the rest exist so that the first process has somewhere
/// to bind onto. Without them a boot script's very first `bind #/boot /bin`
/// fails, which is how their absence was found here.
const ROOTDIRS: [&str; 10] = [
    "bin", "dev", "env", "fd", "mnt", "net", "net.alt", "proc", "root", "srv",
];

pub struct Root {
    files: Vec<Entry>,
    /// The empty directories of `rootlist`, which is a different list from
    /// `bootlist`: `addrootdir` adds here, `addbootfile` adds there.
    dirs: Vec<Entry>,
    next_qid: u64,
}

impl Default for Root {
    fn default() -> Self {
        Self::new()
    }
}

impl Root {
    pub fn new() -> Root {
        let mut r = Root { files: Vec::new(), dirs: Vec::new(), next_qid: 1 };
        // `rootreset` — `reset` is one of `struct Dev`'s seventeen, and this
        // is the whole of devroot's.
        for (i, name) in ROOTDIRS.iter().enumerate() {
            r.dirs.push(Entry {
                name: (*name).to_string(),
                // `addlist`: `d->qid.path = ++l->ndir + l->base`, and
                // rootlist's base is 0. The two static entries are already
                // counted, so these start at 3.
                qid: Qid { qtype: QTDIR, vers: 0, path: (i + 3) as u64 },
                data: Vec::new(),
                perm: crate::ninep::DMDIR | 0o555,
            });
        }
        r
    }

    /// `addbootfile`. The only way anything gets in here, and it happens before
    /// the kernel starts running processes.
    /// `addbootfile` (`devroot.c:80`) adds to `bootlist`, whose base is
    /// `Qboot` — so a boot file is at **`#/boot/<name>`**, not `#/<name>`.
    /// `rootdir[]` is two entries, `#/` and `boot`, and both are directories
    /// (`devroot.c:27`).
    pub fn addbootfile(&mut self, name: &str, contents: Vec<u8>) {
        let qid = Qid { qtype: 0, vers: 0, path: self.next_qid };
        self.next_qid += 1;
        self.files.push(Entry {
            name: name.to_string(),
            qid,
            data: contents,
            perm: 0o555,
        });
    }

    /// `rootgen` (`devroot.c:116`) — what each of the two directories holds.
    /// `#/` lists `boot` and the ten `rootreset` made; `boot` lists the files
    /// `addbootfile` put there.
    fn entries(&mut self, c: &Chan) -> Vec<crate::ninep::Dir> {
        let list: Vec<(&str, Qid, u64, u32)> = if c.qid.path == QROOT {
            std::iter::once((
                "boot",
                Qid { qtype: QTDIR, vers: 0, path: QBOOT },
                0,
                crate::ninep::DMDIR | 0o555,
            ))
            .chain(self.dirs.iter().map(|e| (e.name.as_str(), e.qid, 0, e.perm)))
            .collect()
        } else {
            self.files
                .iter()
                .map(|e| (e.name.as_str(), e.qid, e.data.len() as u64, e.perm))
                .collect()
        };
        list.into_iter()
            .map(|(name, qid, len, perm)| crate::dev::devdir(c, qid, name, len, EVE, EVE, perm))
            .collect()
    }

    fn find(&self, qid: Qid) -> Option<&Entry> {
        self.files.iter().chain(self.dirs.iter()).find(|e| e.qid == qid)
    }
}

/// `Egreg` — Plan 9's own error for writing where writing makes no sense.
const EGREG: &str = "it's a mystery to me";

impl Dev for Root {
    fn id(&self) -> DevId {
        DevId::Root
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn attach(&mut self, _spec: &str) -> Result<Chan, String> {
        Ok(Chan::attach(DevId::Root, 0))
    }

    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Qid>, String> {
        if !c.qid.is_dir() {
            return Err("not a directory".into());
        }
        if name == ".." || name == "." {
            // Two levels, and `..` from either lands at `#/`, as `/..` is `/`.
            return Ok(Some(Qid { qtype: QTDIR, vers: 0, path: QROOT }));
        }
        // At `#/` the only name is `boot`; the files are inside it.
        if c.qid.path == QROOT {
            if name == "boot" {
                return Ok(Some(Qid { qtype: QTDIR, vers: 0, path: QBOOT }));
            }
            return Ok(self.dirs.iter().find(|e| e.name == name).map(|e| e.qid));
        }
        Ok(self.files.iter().find(|e| e.name == name).map(|e| e.qid))
    }

    fn open(&mut self, mut c: Chan, mode: u16) -> Result<Chan, String> {
        if mode & 3 != crate::chan::mode::OREAD && mode & 3 != crate::chan::mode::OEXEC {
            return Err(EGREG.into());
        }
        c.mode = mode;
        Ok(c)
    }

    fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
        Err(EGREG.into())
    }

    fn read(&mut self, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String> {
        if c.qid.is_dir() {
            let entries = self.entries(c);
            return Ok(crate::dev::devdirread(c, n, &entries));
        }
        let e = self.find(c.qid).ok_or("no such file")?;
        let data = &e.data;
        let off = off as usize;
        if off >= data.len() {
            return Ok(Vec::new());
        }
        Ok(data[off..(off + n).min(data.len())].to_vec())
    }

    fn write(&mut self, _c: &mut Chan, _d: &[u8], _o: u64) -> Result<usize, String> {
        Err(EGREG.into())
    }

    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        // `#/` and `boot` are `rootdir[]`'s own two entries, and neither is
        // in a list this device can look up.
        let (name, qid, len, perm) = match c.qid.path {
            QROOT => ("#/", c.qid, 0, crate::ninep::DMDIR | 0o555),
            QBOOT => ("boot", c.qid, 0, crate::ninep::DMDIR | 0o555),
            _ => {
                let e = self.find(c.qid).ok_or("no such file")?;
                (e.name.as_str(), e.qid, e.data.len() as u64, e.perm)
            }
        };
        Ok(crate::dev::devdir(c, qid, name, len, EVE, EVE, perm).conv_d2m())
    }

    fn wstat(&mut self, _c: &mut Chan, _e: &[u8]) -> Result<(), String> {
        Err(EGREG.into())
    }

    fn remove(&mut self, _c: &mut Chan) -> Result<(), String> {
        Err(EGREG.into())
    }

    fn close(&mut self, _c: &mut Chan) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// `addbootfile` puts a file in `boot`, not at the root — `bootlist`'s
    /// base is `Qboot` (`devroot.c:80`), and `rootdir[]` is `#/` and `boot`,
    /// both directories (`:27`).
    fn a_boot_file_can_be_walked_to_and_read() {
        let mut r = Root::new();
        r.addbootfile("init", b"the image".to_vec());
        let c = r.attach("").unwrap();
        assert!(r.walk(&c, "init").unwrap().is_none(), "not at the root");
        let bq = r.walk(&c, "boot").unwrap().expect("#/boot");
        let b = c.walked("boot", bq);
        let qid = r.walk(&b, "init").unwrap().expect("#/boot/init");
        let f = b.walked("init", qid);
        let mut f = r.open(f, crate::chan::mode::OEXEC).unwrap();
        assert_eq!(r.read(&mut f, 100, 0).unwrap(), b"the image");
    }

    /// `rootreset` (`devroot.c:95`) adds ten empty directories, and they are
    /// what a first process binds onto. `bind #/boot /bin` is the first thing
    /// any boot does, and before this it failed with "'bin' does not exist".
    #[test]
    fn the_root_carries_the_ten_directories_rootreset_adds() {
        let mut r = Root::new();
        let c = r.attach("").unwrap();
        for name in ROOTDIRS {
            let q = r.walk(&c, name).unwrap().unwrap_or_else(|| panic!("#/{name}"));
            assert!(q.is_dir(), "#/{name} is a directory");
        }
        let mut c = r.open(c, crate::chan::mode::OREAD).unwrap();
        let b = r.read(&mut c, 4096, 0).unwrap();
        let mut names = Vec::new();
        let mut at = 0;
        while at < b.len() {
            let d = crate::ninep::Dir::conv_m2d(&b[at..]).expect("an entry");
            at += 2 + u16::from_le_bytes([b[at], b[at + 1]]) as usize;
            names.push(d.name);
        }
        assert_eq!(names.len(), 11, "boot, and the ten: {names:?}");
        assert_eq!(names[0], "boot");
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
