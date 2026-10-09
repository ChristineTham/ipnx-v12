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
//! **The conversation is Plan 9's: a write submits, and a read takes the
//! oldest reply the server has completed, sleeping until there is one**
//! (`submit`, `getreply`, `devvirtio9p.c:597`, `:569`). Many requests may be
//! outstanding, and replies come back in the order the server completes
//! them, not the order they were sent — the mount driver hands each to its
//! RPC by tag (`mountmux`). A server that answers at once answers in the
//! submit; one whose answer must wait — a window's console read before
//! anything is typed — answers later.
//!
//! **Where it differs, and why:** Plan 9's submits a descriptor chain to a
//! virtqueue and harvests completions in its interrupt (`v9interrupt`,
//! `vqharvest`). There is no virtqueue and no interrupt here: a submit is a
//! call outward, and the clock harvests what the server has answered since
//! — the same difference, in the same place, as `#c`'s keyboard
//! (`kbdputcclock`). And the queue has no fixed size: a virtqueue's slots
//! are the hardware's, and there is no hardware.
//!
//! Plan 9's also carries a 9P2000.u shim (`tshim`, `rfixup`) *"because qemu's
//! 9pfs speaks only 9P2000.u, Plan 9 speaks plain 9P2000"*. There is none
//! here: the server this machine provides speaks 9P2000, which is the only
//! version this system has (`ninep.rs`).
//!
//! It is a **9legacy** device — absent from `plan9-stock` — and it is
//! configured into the shipped kernels (`pc/pcf:11`, `pc/pccpuf:11`).

use crate::chan::Chan;
use crate::dev::{Dev, DevId, Eve};
use crate::ninep::{Qid, QTDIR};
use crate::proc::{Rid, Up};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

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

    /// **`submit`** (`devvirtio9p.c:597`): one `T`-message onto the queue,
    /// and the reply if the server has it now. A server whose reply must
    /// wait answers `None`, and gives it later through
    /// [`Nineserver::harvest`]. A server that always answers at once
    /// needs only [`Nineserver::rpc`].
    fn submit(&mut self, t: &[u8]) -> Result<Option<Vec<u8>>, String> {
        self.rpc(t).map(Some)
    }

    /// **`vqharvest`** (`devvirtio9p.c:470`), which the device's interrupt
    /// calls (`v9interrupt`): the replies completed since it was last
    /// asked, in the order they were completed.
    fn harvest(&mut self) -> Vec<Vec<u8>> {
        Vec::new()
    }
}

const QDIR: u64 = 0;

/// `#9` — one file per server the machine provides, named `0`, `1`, … as
/// `v9gen` names them (`devvirtio9p.c:1059`: `snprint(buf, sizeof buf, "%d", s)`).
pub struct Virtio9p {
    /// `eve` — the kernel-wide host owner (`auth.c:10`), shared rather than
    /// copied, because writing `#c/hostowner` renames it for everyone.
    eve: Eve,
    servers: Vec<Server>,
    /// `up`, for a read that must wait for a reply.
    up: Rc<RefCell<Up>>,
}

struct Server {
    host: Box<dyn Nineserver>,
    /// `cl->inuse` — `v9open` is `Einuse` if the channel is already open
    /// (`devvirtio9p.c:1110`). One mount at a time, because one reply stream
    /// cannot be shared.
    inuse: bool,
    /// `cl->cur` — the reply being handed out, and how much of it has gone.
    /// `v9read` answers bytes of ONE R-message and never spans two
    /// (`devvirtio9p.c:1171`), which is what lets `#M` reassemble by the
    /// `size[4]` prefix.
    cur: Option<(Vec<u8>, usize)>,
    /// `c->donehd` — replies completed and not yet read, oldest first.
    done: VecDeque<Vec<u8>>,
    /// The requests whose replies have not come, by tag — the chains the
    /// host still holds. A `Tflush` names the tag it flushes: its `Rflush`
    /// is the end of both, since the flushed request is answered no more.
    held: HashMap<u16, Option<u16>>,
}

impl Server {
    /// `v9flush` (`devvirtio9p.c:626`): the queued and half-read replies
    /// go — *"start the conversation clean"*.
    fn flush(&mut self) {
        self.cur = None;
        self.done.clear();
    }

    /// A reply has come: it is no longer held, nor is what it flushed.
    fn answered(&mut self, r: &[u8]) {
        let Some(tag) = r.get(5..7).map(|t| u16::from_le_bytes([t[0], t[1]])) else { return };
        if let Some(Some(flushed)) = self.held.remove(&tag) {
            self.held.remove(&flushed);
        }
    }
}

/// `Tflush`'s type and `Rflush`'s, from `fcall.h`.
const TFLUSH: u8 = 108;

impl Virtio9p {
    pub fn new(up: Rc<RefCell<Up>>) -> Virtio9p {
        Virtio9p { servers: Vec::new(), eve: Eve::default(), up }
    }

    /// `v9probe` (`v9reset`, `devvirtio9p.c:1066`) — what the machine found.
    /// Each one becomes a file, in the order it was added.
    pub fn add(&mut self, host: Box<dyn Nineserver>) {
        self.servers.push(Server { host, inuse: false, cur: None, done: VecDeque::new(), held: HashMap::new() });
    }

    /// **`v9interrupt`** (`devvirtio9p.c:508`) — what the host has answered,
    /// queued, and the reader waiting for it woken (`vqharvest`'s
    /// *"wakeup(&c->rwait)"*). The machine has no interrupt for it, so the
    /// clock calls this, as it takes in the keyboard.
    pub fn interrupt(&mut self) {
        let mut woken = Vec::new();
        for (i, s) in self.servers.iter_mut().enumerate() {
            let got = s.host.harvest();
            if got.is_empty() {
                continue;
            }
            for r in got {
                s.answered(&r);
                s.done.push_back(r);
            }
            woken.push(i);
        }
        if woken.is_empty() {
            return;
        }
        let procs = self.up.borrow().procs.clone();
        let mut procs = procs.borrow_mut();
        for i in woken {
            procs.wakeup(Rid::Rr(DevId::Virtio9p, i as u32, 0));
        }
    }

    /// Whether a conversation is waiting on its server: a reply is still
    /// to come on a channel that is open. The machine must go on taking
    /// clock ticks while one is, because the clock is what brings it in.
    pub fn waiting(&self) -> bool {
        self.servers.iter().any(|s| s.inuse && !s.held.is_empty())
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
    fn seteve(&mut self, eve: Eve) {
        self.eve = eve;
    }

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
        // `v9open` (`pc/devvirtio9p.c:1090`): *"if(omode != OREAD)
        // error(Eperm)"* for the directory, the mode as given.
        if c.qid.is_dir() {
            if mode != crate::chan::mode::OREAD {
                return Err(EPERM.into());
            }
            c.mode = crate::chan::openmode(mode)?;
            c.flag |= crate::chan::flag::COPEN;
            c.offset = 0;
            return Ok(c);
        }
        let m = crate::chan::openmode(mode)?;
        let i = self.index(c.qid.path).ok_or(ENONEXIST)?;
        if self.servers[i].inuse {
            return Err(EINUSE.into());
        }
        self.servers[i].inuse = true;
        // *"v9flush(cl); /* start the conversation clean */"*
        self.servers[i].flush();
        c.mode = m;
        c.flag |= crate::chan::flag::COPEN;
        c.offset = 0;
        Ok(c)
    }

    fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
        Err(EPERM.into())
    }

    /// `v9read` (`devvirtio9p.c:1152`): bytes of the reply in hand, and never
    /// across two. When one is used up the next read takes the next.
    fn read(&mut self, c: &mut Chan, n: usize, _off: u64) -> Result<Vec<u8>, String> {
        if c.qid.is_dir() {
            let user = self.eve.borrow().clone();
            let entries: Vec<crate::ninep::Dir> = (0..self.servers.len())
                .map(|i| {
                    let qid = Qid { qtype: 0, vers: 0, path: i as u64 + 1 };
                    crate::dev::devdir(c, qid, &i.to_string(), 0, &user, &user, 0o660)
                })
                .collect();
            return Ok(crate::dev::devdirread(c, n, &entries));
        }
        if n == 0 {
            return Ok(Vec::new());
        }
        let i = self.index(c.qid.path).ok_or(ENONEXIST)?;
        if self.servers[i].cur.is_none() {
            // `getreply` (`devvirtio9p.c:569`): the oldest completed reply,
            // harvesting first *"in case the irq was missed"*, and sleeping
            // until one comes.
            if self.servers[i].done.is_empty() {
                self.interrupt();
            }
            let Some(r) = self.servers[i].done.pop_front() else { return self.getreply(i) };
            self.servers[i].cur = Some((r, 0));
        }
        let s = &mut self.servers[i];
        let (reply, rp) = s.cur.as_mut().expect("a reply in hand");
        let k = n.min(reply.len() - *rp);
        let out = reply[*rp..*rp + k].to_vec();
        *rp += k;
        if *rp >= reply.len() {
            s.cur = None;
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
        let s = &mut self.servers[i];
        let tag = u16::from_le_bytes([data[5], data[6]]);
        // a `Tflush` names the tag it flushes: `oldtag[2]`
        let flushes = (data[4] == TFLUSH && data.len() >= 9).then(|| u16::from_le_bytes([data[7], data[8]]));
        s.held.insert(tag, flushes);
        match s.host.submit(data) {
            Ok(Some(r)) => {
                s.answered(&r);
                s.done.push_back(r);
            }
            Ok(None) => {}
            Err(e) => {
                s.held.remove(&tag);
                return Err(e);
            }
        }
        Ok(data.len())
    }

    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        // `v9stat` is `devstat` over `v9gen` (`pc/devvirtio9p.c:1086`).
        if c.qid.is_dir() {
            return Ok(crate::dev::devstatdir(c, &self.eve.borrow()).conv_d2m());
        }
        let i = self.index(c.qid.path).ok_or(ENONEXIST)?;
        Ok(crate::dev::devdir(c, c.qid, &i.to_string(), 0, &self.eve.borrow(), &self.eve.borrow(), 0o660).conv_d2m())
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
            s.flush();
            s.inuse = false;
        }
    }
}

impl Virtio9p {
    /// **Wait for a reply** — `getreply`'s *"tsleep(&c->rwait, havereply,
    /// c, 1000)"* — and come back here when the clock has brought one in.
    /// The read answers nothing now, and is made again when the process
    /// is entered.
    ///
    /// A note does not end the wait: *"while(waserror()) ;"* takes it and
    /// sleeps again, and the note is delivered when the call is over. The
    /// server's reply is what ends it.
    fn getreply(&mut self, i: usize) -> Result<Vec<u8>, String> {
        let (pid, procs) = {
            let up = self.up.borrow();
            (up.pid, up.procs.clone())
        };
        let mut procs = procs.borrow_mut();
        let r = Rid::Rr(DevId::Virtio9p, i as u32, 0);
        while !procs.sleep(pid, r, false) {
            if !procs.interrupted(pid) {
                return Err("no reply from the server".into());
            }
        }
        Ok(Vec::new())
    }
}
