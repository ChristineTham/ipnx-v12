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
use crate::dev::{Dev, DevId};
use crate::ninep::{Qid, QTDIR};

/// One entry. Plan 9's `Dirtab`, with the fields a subset needs.
struct Entry {
    name: String,
    qid: Qid,
    data: Vec<u8>,
    perm: u32,
}

pub struct Root {
    files: Vec<Entry>,
    next_qid: u64,
}

impl Default for Root {
    fn default() -> Self {
        Self::new()
    }
}

impl Root {
    pub fn new() -> Root {
        Root { files: Vec::new(), next_qid: 1 }
    }

    /// `addbootfile`. The only way anything gets in here, and it happens before
    /// the kernel starts running processes.
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

    fn find(&self, qid: Qid) -> Option<&Entry> {
        self.files.iter().find(|e| e.qid == qid)
    }
}

/// `Egreg` — Plan 9's own error for writing where writing makes no sense.
const EGREG: &str = "it's a mystery to me";

impl Dev for Root {
    fn id(&self) -> DevId {
        DevId::Root
    }

    fn attach(&mut self, _spec: &str) -> Result<Chan, String> {
        Ok(Chan::attach(DevId::Root, 0))
    }

    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Qid>, String> {
        if !c.qid.is_dir() {
            return Err("not a directory".into());
        }
        if name == ".." {
            // devroot has one level; `..` from it is itself, as `/..` is `/`.
            return Ok(Some(Qid { qtype: QTDIR, vers: 0, path: 0 }));
        }
        Ok(self.files.iter().find(|e| e.name == name).map(|e| e.qid))
    }

    fn open(&mut self, c: &mut Chan, mode: u16) -> Result<(), String> {
        if mode & 3 != crate::chan::mode::OREAD && mode & 3 != crate::chan::mode::OEXEC {
            return Err(EGREG.into());
        }
        c.mode = mode;
        Ok(())
    }

    fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
        Err(EGREG.into())
    }

    fn read(&mut self, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String> {
        let e = self.find(c.qid).ok_or("no such file")?;
        let off = off as usize;
        if off >= e.data.len() {
            return Ok(Vec::new());
        }
        Ok(e.data[off..(off + n).min(e.data.len())].to_vec())
    }

    fn write(&mut self, _c: &mut Chan, _d: &[u8], _o: u64) -> Result<usize, String> {
        Err(EGREG.into())
    }

    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        let e = self.find(c.qid).ok_or("no such file")?;
        Ok(crate::ninep::W::new()
            .u16(0)
            .u16(0)
            .u32(0)
            .raw(&e.qid.write(crate::ninep::W::new()).into_body())
            .u32(e.perm)
            .u32(0)
            .u32(0)
            .u64(e.data.len() as u64)
            .s(&e.name)
            .into_body())
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
    fn a_boot_file_can_be_walked_to_and_read() {
        let mut r = Root::new();
        r.addbootfile("init", b"the image".to_vec());
        let c = r.attach("").unwrap();
        let qid = r.walk(&c, "init").unwrap().expect("init is there");
        let mut f = c.walked("init", qid);
        r.open(&mut f, crate::chan::mode::OEXEC).unwrap();
        assert_eq!(r.read(&mut f, 100, 0).unwrap(), b"the image");
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
