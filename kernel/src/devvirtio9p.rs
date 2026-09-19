//! `#9` — virtio9p (`plan9/sys/src/9/pc/devvirtio9p.c`).
//!
//! **A channel to a 9P server the machine provides, and nothing else.** Its
//! own comment says what it is for, and it is this system's situation word
//! for word:
//!
//! > *mount a host directory exported by qemu's `-device virtio-9p-pci` /
//! > `-fsdev local` directly over a virtqueue, with no network in the path.*
//! >
//! > ```text
//! > bind -a '#9' /dev
//! > mount -c '#9/0' /n/host
//! > ```
//! >
//! > *devmnt drives this chan like a tcp connection: it `write()`s a whole
//! > T-message and reads the reply back as a byte stream reassembled by the
//! > `size[4]` prefix.*
//!
//! So the device marshals nothing and understands nothing. `#M` is still the
//! only place wire 9P exists; this is the wire.
//!
//! **Where it differs, and why:** Plan 9's submits a descriptor chain to a
//! virtqueue and harvests completions in an interrupt. There is no virtqueue
//! and no interrupt here, so a submit is a call outward and the reply comes
//! back from it — the same difference, in the same place, as `#c`'s keyboard.
//!
//! Plan 9's also carries a 9P2000.u shim (`tshim`, `rfixup`) *"because qemu's
//! 9pfs speaks only 9P2000.u, Plan 9 speaks plain 9P2000"*. There is none
//! here: the server this machine provides speaks 9P2000, which is the only
//! version this system has (`ninep.rs`).
//!
//! It is a **9legacy** device — absent from `plan9-stock` — and it is
//! configured into the shipped kernels (`pc/pcf:11`, `pc/pccpuf:11`).

use crate::chan::Chan;
use crate::dev::{Dev, DevId, EVE};
use crate::ninep::{Qid, QTDIR};

/// What the machine must supply: one 9P exchange.
///
/// Plan 9's `submit` posts a chain and `getreply` harvests it; the pair is one
/// T-message in and one R-message out, and that is the whole of what the
/// hardware does. This is that pair, with no queue between them because
/// nothing here runs while the caller waits.
pub trait Nineserver {
    /// One `T`-message in, one `R`-message out. An `Err` is the transport
    /// failing — a server that wants to refuse a request answers `Rerror`.
    fn rpc(&mut self, t: &[u8]) -> Result<Vec<u8>, String>;
}

const QDIR: u64 = 0;

/// `#9` — one file per server the machine provides, named `0`, `1`, … as
/// `v9gen` names them (`devvirtio9p.c:1059`: `snprint(buf, sizeof buf, "%d", s)`).
pub struct Virtio9p {
    servers: Vec<Server>,
}

struct Server {
    host: Box<dyn Nineserver>,
    /// `cl->inuse` — `v9open` is `Einuse` if the channel is already open
    /// (`devvirtio9p.c:1110`). One mount at a time, because one reply stream
    /// cannot be shared.
    inuse: bool,
    /// The reply being handed out, and how much of it has gone. `v9read`
    /// answers bytes of ONE R-message and never spans two
    /// (`devvirtio9p.c:1171`), which is what lets `#M` reassemble by the
    /// `size[4]` prefix.
    reply: Vec<u8>,
    rp: usize,
}

impl Default for Virtio9p {
    fn default() -> Self {
        Virtio9p::new()
    }
}

impl Virtio9p {
    pub fn new() -> Virtio9p {
        Virtio9p { servers: Vec::new() }
    }

    /// `v9probe` (`v9reset`, `devvirtio9p.c:1066`) — what the machine found.
    /// Each one becomes a file, in the order it was added.
    pub fn add(&mut self, host: Box<dyn Nineserver>) {
        self.servers.push(Server { host, inuse: false, reply: Vec::new(), rp: 0 });
    }

    /// `ctlrindex(c->qid.path - 1)` — the qid path is the index plus one,
    /// because 0 is the directory (`devvirtio9p.c:1060`).
    fn index(&self, path: u64) -> Option<usize> {
        if path == QDIR {
            return None;
        }
        let i = (path - 1) as usize;
        (i < self.servers.len()).then_some(i)
    }
}

const EPERM: &str = "permission denied";
const ENONEXIST: &str = "file does not exist";
const EINUSE: &str = "device or object already in use";

impl Dev for Virtio9p {
    fn id(&self) -> DevId {
        DevId::Virtio9p
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    /// `v9attach`: `devattach('9', spec)`.
    fn attach(&mut self, _spec: &str) -> Result<Chan, String> {
        Ok(Chan::attach(DevId::Virtio9p, 0))
    }

    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Chan>, String> {
        if !c.qid.is_dir() {
            return Err("not a directory".into());
        }
        if name == ".." || name == "." {
            return Ok(Some(c.walked(name, Qid { qtype: QTDIR, vers: 0, path: QDIR })));
        }
        Ok(name
            .parse::<usize>()
            .ok()
            .filter(|i| *i < self.servers.len())
            .map(|i| c.walked(name, Qid { qtype: 0, vers: 0, path: i as u64 + 1 })))
    }

    /// `v9open` (`devvirtio9p.c:1090`): the directory reads only, and a
    /// server takes one client.
    fn open(&mut self, mut c: Chan, mode: u16) -> Result<Chan, String> {
        if c.qid.is_dir() {
            if mode & 3 != crate::chan::mode::OREAD {
                return Err(EPERM.into());
            }
            c.mode = mode;
            c.offset = 0;
            return Ok(c);
        }
        let i = self.index(c.qid.path).ok_or(ENONEXIST)?;
        if self.servers[i].inuse {
            return Err(EINUSE.into());
        }
        self.servers[i].inuse = true;
        c.mode = mode;
        Ok(c)
    }

    fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
        Err(EPERM.into())
    }

    /// `v9read` (`devvirtio9p.c:1152`): bytes of the reply in hand, and never
    /// across two. When one is used up the next read takes the next.
    fn read(&mut self, c: &mut Chan, n: usize, _off: u64) -> Result<Vec<u8>, String> {
        if c.qid.is_dir() {
            let user = EVE.to_string();
            let entries: Vec<crate::ninep::Dir> = (0..self.servers.len())
                .map(|i| {
                    let qid = Qid { qtype: 0, vers: 0, path: i as u64 + 1 };
                    crate::dev::devdir(c, qid, &i.to_string(), 0, &user, EVE, 0o660)
                })
                .collect();
            return Ok(crate::dev::devdirread(c, n, &entries));
        }
        let i = self.index(c.qid.path).ok_or(ENONEXIST)?;
        let s = &mut self.servers[i];
        if s.rp >= s.reply.len() {
            return Ok(Vec::new());
        }
        let k = n.min(s.reply.len() - s.rp);
        let out = s.reply[s.rp..s.rp + k].to_vec();
        s.rp += k;
        if s.rp >= s.reply.len() {
            s.reply.clear();
            s.rp = 0;
        }
        Ok(out)
    }

    /// `v9write` (`devvirtio9p.c:1184`): one whole T-message, submitted. A
    /// message too short to hold `size[4] type[1] tag[2]` is refused before
    /// anything is done with it.
    fn write(&mut self, c: &mut Chan, data: &[u8], _off: u64) -> Result<usize, String> {
        if c.qid.is_dir() {
            return Err(EPERM.into());
        }
        if data.len() < 4 + 1 + 2 {
            return Err("invalid 9P message".into());
        }
        let i = self.index(c.qid.path).ok_or(ENONEXIST)?;
        let reply = self.servers[i].host.rpc(data)?;
        let s = &mut self.servers[i];
        s.reply = reply;
        s.rp = 0;
        Ok(data.len())
    }

    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        let (name, perm) = if c.qid.is_dir() {
            ("#9".to_string(), crate::ninep::DMDIR | 0o555)
        } else {
            let i = self.index(c.qid.path).ok_or(ENONEXIST)?;
            (i.to_string(), 0o660)
        };
        Ok(crate::dev::devdir(c, c.qid, &name, 0, EVE, EVE, perm).conv_d2m())
    }

    fn wstat(&mut self, _c: &mut Chan, _e: &[u8]) -> Result<(), String> {
        Err(EPERM.into())
    }

    fn remove(&mut self, _c: &mut Chan) -> Result<(), String> {
        Err(EPERM.into())
    }

    /// `v9close` (`devvirtio9p.c:1134`): the server takes a client again.
    fn close(&mut self, c: &mut Chan) {
        if c.qid.is_dir() || c.flag & crate::chan::flag::COPEN == 0 {
            return;
        }
        if let Some(i) = self.index(c.qid.path) {
            let s = &mut self.servers[i];
            s.inuse = false;
            s.reply.clear();
            s.rp = 0;
        }
    }
}
