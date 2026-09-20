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
/// `Chan.flag`'s bits (`portdat.h:154`). Not decoration: `srvwrite` refuses a
/// posted fd carrying `CCEXEC|CRCLOSE` (`devsrv.c:323`), `srvclose` acts on
/// `CRCLOSE` (`:286`), and `pipewrite` suppresses its note when the pipe is a
/// mounted queue (`devpipe.c:348`).
pub mod flag {
    /// `COPEN` — for i/o.
    pub const COPEN: u16 = 0x0001;
    /// `CMSG` — the message channel for a mount.
    pub const CMSG: u16 = 0x0002;
    /// `CCEXEC` — close on exec.
    pub const CCEXEC: u16 = 0x0008;
    /// `CFREE` — not in use.
    pub const CFREE: u16 = 0x0010;
    /// `CRCLOSE` — remove on close.
    pub const CRCLOSE: u16 = 0x0020;
    /// `CCACHE` — client cache.
    pub const CCACHE: u16 = 0x0080;
}

pub mod mode {
    pub const OREAD: u16 = 0;
    pub const OWRITE: u16 = 1;
    pub const ORDWR: u16 = 2;
    pub const OEXEC: u16 = 3;
    pub const OTRUNC: u16 = 16;
    /// `OCEXEC` — *"or'ed in, close on exec"* (`libc.h:568`).
    pub const OCEXEC: u16 = 32;
    pub const ORCLOSE: u16 = 64;
    /// `OEXCL` — *"or'ed in, exclusive use (create only)"* (`libc.h:570`).
    /// With it, `create` of a name that exists fails instead of truncating.
    pub const OEXCL: u16 = 0x1000;
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
    /// `Chan.flag` (`portdat.h`), the bits in [`flag`].
    pub flag: u16,
    pub offset: u64,
    /// `Chan.dri` (`portdat.h`) — which directory entry the next read
    /// resumes at. A directory is read as whole `Dir` entries and an entry is
    /// never split, so a byte offset cannot say where to carry on; `sysseek`
    /// clears this and allows no offset but 0 on a directory
    /// (`sysfile.c:820`).
    pub dri: u32,
    /// `Chan.umh` (`portdat.h`) — the UNION this channel was reached through,
    /// whole, with the element it landed on first. `namec` keeps it for two
    /// things, and Plan 9 keeps it for the same two:
    ///
    /// * `Abind` (`chan.c:1462`), because `cmount` copies a union when
    ///   binding it onto a directory (`:719`) — so `bind -a /root /` carries
    ///   all of `/root` and not just whichever element answered first;
    /// * `Aopen` (`chan.c:1502`), *"only save the mount head if it's a
    ///   multiple element union"*, because reading such a directory means
    ///   reading every element (`unionread`, `sysfile.c:323`).
    pub umh: Vec<Chan>,
    /// `Chan.uri` — which element of `umh` a union read is on.
    pub uri: u32,
    /// `Chan.umc` — that element, opened. One at a time, and closed when it
    /// runs out (`sysfile.c:356`).
    pub umc: Option<Box<Chan>>,
    /// `iounit` — *"chunk size for i/o; 0==default"*. `mntversion` caps the
    /// negotiated msize by it (`devmnt.c:119`).
    pub iounit: u32,
    /// The server's name for this file, when the mount driver is serving it.
    /// Meaningless for every other device, and that is Plan 9's arrangement
    /// too.
    pub fid: u32,
    /// `mux` — the mount using this channel for its messages. `mntattach`
    /// reads it and only versions the wire when it is nil (`devmnt.c:317`), so
    /// a second mount of the same channel joins the session rather than
    /// starting another. Plan 9 holds a `Mnt*`; here it is that mount's index.
    pub mux: Option<u32>,
    /// `mchan` — the channel to the mounted server, and `mqid`, the qid of the
    /// mount root (`portdat.h`).
    pub mchan: Option<Box<Chan>>,
    pub mqid: Qid,
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
            flag: 0,
            offset: 0,
            dri: 0,
            umh: Vec::new(),
            uri: 0,
            umc: None,
            iounit: 0,
            fid: crate::ninep::NOFID,
            mux: None,
            mchan: None,
            mqid: Qid { qtype: 0, vers: 0, path: 0 },
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
        // A walked channel is not the one the union was found on, so it
        // carries no union of its own (`devclone` copies no `umh`).
        Chan { qid, path, offset: 0, umh: Vec::new(), uri: 0, umc: None, ..self.clone() }
    }
}
