//! `Chan` — the object everything else acts on.
//!
//! In Plan 9 a channel is the kernel's handle on a file: a walk produces one,
//! an fd holds one, a mount point is one, a device's operations all take one.
//! Getting this wrong is how a kernel ends up with three parallel notions of
//! "a thing you can read", so it is built first and the rest refers to it.
//!
//! The fields are Plan 9's (`portdat.h`, `struct Chan`), minus what a hosted
//! kernel has no use for: no `Ref` (Rust counts references), no allocation
//! links, no locks, and none of the union-read or cache rock that exists to
//! make a hardware kernel fast.

use crate::dev::DevId;
use crate::ninep::Qid;

/// Open modes, from `<libc.h>`. `OREAD` is 0, which is why a mode is not an
/// `Option`.
pub mod mode {
    pub const OREAD: u16 = 0;
    pub const OWRITE: u16 = 1;
    pub const ORDWR: u16 = 2;
    pub const OEXEC: u16 = 3;
    pub const OTRUNC: u16 = 16;
    pub const ORCLOSE: u16 = 64;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chan {
    /// Which device serves this channel — Plan 9's `type`.
    pub dev: DevId,
    /// Which instance of it — Plan 9's `dev`. Two pipes are the same device
    /// and different instances.
    pub devno: u32,
    pub qid: Qid,
    pub mode: u16,
    pub offset: u64,
    /// The server's name for this file, when the mount driver is serving it.
    /// Meaningless for every other device, and that is Plan 9's arrangement
    /// too.
    pub fid: u32,
    /// The name this channel was reached by. Plan 9 keeps a `Path` so that
    /// `fd2path` can answer and `..` can be resolved without asking a server.
    pub path: String,
}

impl Chan {
    /// A device's root, as `attach` returns it.
    pub fn attach(dev: DevId, devno: u32) -> Chan {
        Chan {
            dev,
            devno,
            qid: Qid { qtype: crate::ninep::QTDIR, vers: 0, path: 0 },
            mode: mode::OREAD,
            offset: 0,
            fid: crate::ninep::NOFID,
            path: format!("#{}", dev.letter()),
        }
    }

    pub fn is_dir(&self) -> bool {
        self.qid.is_dir()
    }

    /// The channel reached by walking one name from this one. The caller
    /// supplies the qid the device gave; the path bookkeeping is here so every
    /// device does not repeat it.
    pub fn walked(&self, name: &str, qid: Qid) -> Chan {
        let path = match name {
            ".." => match self.path.rfind('/') {
                Some(i) if i > 0 => self.path[..i].to_string(),
                _ => self.path.clone(),
            },
            n => {
                if self.path.ends_with('/') {
                    format!("{}{}", self.path, n)
                } else {
                    format!("{}/{}", self.path, n)
                }
            }
        };
        Chan { qid, path, offset: 0, ..self.clone() }
    }
}
