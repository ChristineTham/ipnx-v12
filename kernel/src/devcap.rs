//! `#¤` — cap (`plan9/sys/src/9/port/devcap.c`, letter `L'¤'` at `:268`).
//!
//! **The only way a process becomes another user.** `/dev/user` takes the four
//! bytes `none` and nothing else (`auth.c:107`), so dropping is free and
//! one-way; going anywhere else needs a capability that eve minted for exactly
//! that transition:
//!
//! | | |
//! |---|---|
//! | `/dev/caphash` `0200` | **eve only** (`devcap.c:95`). Write the HMAC-SHA1 of `from@to@key` — minting |
//! | `/dev/capuse` `0222` | anyone. Write `from@to@key`; if the hash matches a minted capability and `from` is the writer's own name, `up->user` becomes `to` |
//!
//! A capability is **consumed**: `remcap` unlinks it, so each is good once.
//! This is what `auth/newns` and factotum use, and it is why there is no
//! password and no setuid bit — the authorisation happened when eve minted it.

use crate::chan::Chan;
use crate::dev::{Dev, DevId};
use crate::ninep::{Qid, QTDIR};
use crate::proc::Up;
use crate::sha1::{hmac_sha1, HASHLEN};
use std::cell::RefCell;
use std::rc::Rc;

const EPERM: &str = "permission denied";
const ESHORT: &str = "short read or write";

/// `Qhash` and `Quse` (`devcap.c:38`), with `caphash` last as its comment
/// insists.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Q {
    Dir = 0,
    Use,
    Hash,
}

/// `capdir[]` (`devcap.c:45`) — both write-only, and `caphash` unreadable
/// even by eve.
pub const CAPDIR: &[(&str, Q, u32)] = &[("capuse", Q::Use, 0o222), ("caphash", Q::Hash, 0o200)];

pub struct CapDev {
    /// `capalloc.first` — the minted capabilities, by hash.
    caps: Vec<[u8; HASHLEN]>,
    up: Rc<RefCell<Up>>,
    pub eve: String,
}

impl CapDev {
    pub fn new(up: Rc<RefCell<Up>>) -> CapDev {
        CapDev { caps: Vec::new(), up, eve: "eve".into() }
    }

    fn iseve(&self) -> bool {
        self.up.borrow().user() == self.eve
    }

    /// `remcap` — find the matching capability and **unlink it**, so it
    /// cannot be spent twice.
    fn remcap(&mut self, hash: &[u8; HASHLEN]) -> bool {
        match self.caps.iter().position(|h| h == hash) {
            Some(i) => {
                self.caps.remove(i);
                true
            }
            None => false,
        }
    }
}

impl Dev for CapDev {
    fn id(&self) -> DevId {
        DevId::Cap
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn attach(&mut self, _spec: &str) -> Result<Chan, String> {
        Ok(Chan::attach(DevId::Cap, 0))
    }

    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Qid>, String> {
        if !c.qid.is_dir() {
            return Err("not a directory".into());
        }
        if name == ".." || name == "." {
            return Ok(Some(Qid { qtype: QTDIR, vers: 0, path: Q::Dir as u64 }));
        }
        Ok(CAPDIR
            .iter()
            .find(|e| e.0 == name)
            .map(|e| Qid { qtype: 0, vers: 0, path: e.1 as u64 }))
    }

    /// `capopen` (`devcap.c:95`): `caphash` is eve's alone.
    fn open(&mut self, mut c: Chan, mode: u16) -> Result<Chan, String> {
        if c.qid.path == Q::Hash as u64 && !self.iseve() {
            return Err(EPERM.into());
        }
        c.mode = mode;
        Ok(c)
    }

    fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
        Err(EPERM.into())
    }

    /// Both files are write-only. There is nothing to read: a capability that
    /// could be read could be copied.
    fn read(&mut self, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String> {
        if !c.qid.is_dir() {
            return Err(EPERM.into());
        }
        let s: String = CAPDIR.iter().map(|e| format!("{}\n", e.0)).collect();
        let b = s.into_bytes();
        let off = off as usize;
        if off >= b.len() {
            return Ok(Vec::new());
        }
        Ok(b[off..(off + n).min(b.len())].to_vec())
    }

    fn write(&mut self, c: &mut Chan, data: &[u8], _off: u64) -> Result<usize, String> {
        match c.qid.path {
            // `addcap` — eve mints. The hash is all that is kept; the kernel
            // never sees the key again.
            x if x == Q::Hash as u64 => {
                if !self.iseve() {
                    return Err(EPERM.into());
                }
                if data.len() < HASHLEN {
                    return Err(ESHORT.into());
                }
                let mut h = [0u8; HASHLEN];
                h.copy_from_slice(&data[..HASHLEN]);
                self.caps.push(h);
                Ok(data.len())
            }
            // `capwrite`'s `Quse` (`devcap.c:215`): split at the LAST `@`, so
            // the key is what follows and `from@to` is what precedes.
            x if x == Q::Use as u64 => {
                let text = String::from_utf8_lossy(data).to_string();
                let at = text.rfind('@').ok_or(ESHORT)?;
                let (from, key) = (&text[..at], &text[at + 1..]);
                let hash = hmac_sha1(from.as_bytes(), key.as_bytes());

                // "if a from user is supplied, make sure it matches"
                let (from_user, to) = match from.find('@') {
                    Some(i) => (&from[..i], &from[i + 1..]),
                    None => (from, from),
                };
                if from.contains('@') && from_user != self.up.borrow().user() {
                    return Err("capability must match user".into());
                }
                let mut h = [0u8; HASHLEN];
                h.copy_from_slice(&hash);
                if !self.remcap(&h) {
                    return Err(format!("invalid capability {from}@{key}"));
                }
                let up = self.up.borrow();
                up.procs.borrow_mut().setuser(up.pid, to);
                Ok(data.len())
            }
            _ => Err(EPERM.into()),
        }
    }

    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        let (name, perm) = if c.qid.is_dir() {
            ("#\u{a4}".to_string(), 0o555)
        } else {
            let e = CAPDIR.iter().find(|e| e.1 as u64 == c.qid.path).ok_or("no such file")?;
            (e.0.to_string(), e.2)
        };
        Ok(crate::ninep::W::new()
            .u16(0)
            .u16(0)
            .u32(0)
            .raw(&c.qid.write(crate::ninep::W::new()).into_body())
            .u32(perm)
            .u32(0)
            .u32(0)
            .u64(0)
            .s(&name)
            .into_body())
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
    use crate::proc::Procs;

    fn cap() -> (CapDev, Rc<RefCell<Procs>>) {
        let procs = Rc::new(RefCell::new(Procs::new(Chan::attach(DevId::Root, 0))));
        let up = Rc::new(RefCell::new(Up { pid: 1, procs: procs.clone() }));
        (CapDev::new(up), procs)
    }

    fn chan(d: &mut CapDev, name: &str) -> Chan {
        let dir = d.attach("").unwrap();
        let mut c = dir.clone();
        c.qid = d.walk(&dir, name).unwrap().expect(name);
        c
    }

    /// Mint, then spend: eve writes the hash of `from@to@key` to `caphash`,
    /// and the process writes the text itself to `capuse` and becomes `to`.
    #[test]
    fn a_minted_capability_changes_the_process_user() {
        let (mut d, procs) = cap();
        let secret = "eve@kitty@s3cret";
        let at = secret.rfind('@').unwrap();
        let h = hmac_sha1(secret[..at].as_bytes(), secret[at + 1..].as_bytes());

        let mut hash = chan(&mut d, "caphash");
        d.write(&mut hash, &h, 0).unwrap();

        let mut use_ = chan(&mut d, "capuse");
        d.write(&mut use_, secret.as_bytes(), 0).unwrap();
        assert_eq!(procs.borrow().user(1).unwrap(), "kitty");
    }

    /// `remcap` unlinks it, so a capability is good ONCE. Without that, a
    /// capability overheard is a capability owned.
    #[test]
    fn a_capability_is_spent_once() {
        let (mut d, _) = cap();
        let secret = "eve@kitty@s3cret";
        let at = secret.rfind('@').unwrap();
        let h = hmac_sha1(secret[..at].as_bytes(), secret[at + 1..].as_bytes());
        let mut hash = chan(&mut d, "caphash");
        d.write(&mut hash, &h, 0).unwrap();

        let mut use_ = chan(&mut d, "capuse");
        d.write(&mut use_, secret.as_bytes(), 0).unwrap();
        assert!(d.write(&mut use_, secret.as_bytes(), 0).is_err(), "spent twice");
    }

    /// Nothing eve did not mint is accepted, which is the whole security of
    /// the thing.
    #[test]
    fn an_unminted_capability_is_refused() {
        let (mut d, procs) = cap();
        let mut use_ = chan(&mut d, "capuse");
        assert!(d.write(&mut use_, b"eve@kitty@guess", 0).is_err());
        assert_eq!(procs.borrow().user(1).unwrap(), "eve", "and nothing changed");
    }

    /// `caphash` is eve's alone (`devcap.c:95`) — minting is the privileged
    /// half, spending is not.
    #[test]
    fn only_eve_can_mint() {
        let (mut d, procs) = cap();
        procs.borrow_mut().setuser(1, "none");
        let c = chan(&mut d, "caphash");
        assert!(d.open(c, OWRITE).is_err(), "none must not mint");
        // spending is open to anyone
        let c = chan(&mut d, "capuse");
        assert!(d.open(c, OWRITE).is_ok());
    }

    /// *"if a from user is supplied, make sure it matches"* (`devcap.c:243`):
    /// a capability minted for someone else cannot be spent by you.
    #[test]
    fn a_capability_for_another_user_cannot_be_spent() {
        let (mut d, procs) = cap();
        let secret = "mimmy@kitty@s3cret";
        let at = secret.rfind('@').unwrap();
        let h = hmac_sha1(secret[..at].as_bytes(), secret[at + 1..].as_bytes());
        let mut hash = chan(&mut d, "caphash");
        d.write(&mut hash, &h, 0).unwrap();

        // we are eve, not mimmy
        let mut use_ = chan(&mut d, "capuse");
        let e = d.write(&mut use_, secret.as_bytes(), 0).unwrap_err();
        assert!(e.contains("must match user"), "{e}");
        assert_eq!(procs.borrow().user(1).unwrap(), "eve");
    }

    /// Both files are write-only: a capability that could be read could be
    /// copied.
    #[test]
    fn neither_file_can_be_read() {
        let (mut d, _) = cap();
        for name in ["caphash", "capuse"] {
            let mut c = chan(&mut d, name);
            assert!(d.read(&mut c, 64, 0).is_err(), "{name}");
        }
        let mut dir = d.attach("").unwrap();
        let s = String::from_utf8(d.read(&mut dir, 64, 0).unwrap()).unwrap();
        assert_eq!(s, "capuse\ncaphash\n");
        let _ = OREAD;
    }
}
