//! `#M` — the mount driver (`plan9/sys/src/9/port/devmnt.c`).
//!
//! **The only place wire 9P is marshalled.** Every other device presents the
//! file interface as function calls — a walk is a call, a read is a call — and
//! that is why this system can have one protocol without paying to encode it
//! against itself. When a channel crosses to a *server*, it crosses here, and
//! only here.
//!
//! The conversation is Plan 9's:
//!
//! | | |
//! |---|---|
//! | `mntversion` (`:100`) | `Tversion` first, or there is no session. `MAXRPC` is `IOHDRSZ+16*1024` in 9legacy (`:19`), with `MAXCMNRPC` at the old `IOHDRSZ+8192` for the negotiation itself |
//! | `mntattach` (`:303`) | `Tattach` — the channel handed in **is the wire** |
//! | `mntwalk` (`:384`) | `Twalk`, which carries the whole path and answers with the qids it managed |
//! | `mntrdwr` (`:688`) | `Tread`/`Twrite` **in a loop**, each bounded by `msize - IOHDRSZ`. A short answer is not the end |
//! | `mntclose` | `Tclunk` — and a fid leaked here is a fid the server keeps forever |

use crate::chan::Chan;
use crate::dev::{Dev, DevId};
use crate::ninep::{unframe, Qid, IOHDRSZ, R, T, W};

/// `MAXRPC` (`devmnt.c:19`, 9legacy) and `MAXCMNRPC` (`:21`).
pub const MAXRPC: u32 = (IOHDRSZ + 16 * 1024) as u32;
/// The buffer the version exchange itself uses, kept at the old size
/// (`devmnt.c:21`). **Not** what is asked for.
pub const MAXCMNRPC: u32 = (IOHDRSZ + 8192) as u32;
pub const VERSION: &str = "9P2000";

/// The wire. Plan 9 reaches the channel's own device — `devtab[m->c->type]`'s
/// `bread` and `bwrite` — which is the one place a device talks to another
/// device. Here the kernel supplies that as a transport, so the mount driver
/// holds no device table and there is no cycle.
pub trait Transport {
    fn rpc(&mut self, request: &[u8]) -> Result<Vec<u8>, String>;
}

/// One mounted server — Plan 9's `Mnt`.
pub struct Mnt {
    pub msize: u32,
    wire: Box<dyn Transport>,
    tag: u16,
    /// `NFID`-style allocation. A fid is the server's name for a file, and
    /// this side chooses them.
    nextfid: u32,
}

impl Mnt {
    /// `mntversion` then `mntattach`: negotiate, then attach. Neither is
    /// optional — a server that has not agreed a version answers nothing.
    pub fn attach(
        mut wire: Box<dyn Transport>,
        uname: &str,
        aname: &str,
    ) -> Result<(Mnt, Chan), String> {
        // `mntversion` asks for MAXRPC (`devmnt.c:118`, `msize = MAXRPC`).
        // MAXCMNRPC is only the BUFFER the exchange itself uses
        // (`:155`, `msg = malloc(MAXCMNRPC)`) — asking for it instead would
        // cap every session at the old 8K and nothing would look wrong.
        let req = W::new().u32(MAXRPC).s(VERSION).frame(T::Version as u8, !0);
        let reply = wire.rpc(&req)?;
        let m = unframe(&reply).ok_or("malformed Rversion")?;
        if m.ty != T::Version.reply() {
            return Err(rerror(&m.body).unwrap_or_else(|| "not Rversion".into()));
        }
        let mut r = R::new(m.body);
        let msize = r.u32().ok_or("short Rversion")?;
        let version = r.s().ok_or("short Rversion")?;
        if version != VERSION {
            return Err(format!("server speaks {version}, not {VERSION}"));
        }
        let msize = msize.min(MAXRPC);

        let mut mnt = Mnt { msize, wire, tag: 0, nextfid: 1 };
        let fid = mnt.newfid();
        // Tattach: fid, afid (NOFID — no authentication), uname, aname
        let req = W::new()
            .u32(fid)
            .u32(crate::ninep::NOFID)
            .s(uname)
            .s(aname)
            .frame(T::Attach as u8, mnt.newtag());
        let reply = mnt.wire.rpc(&req)?;
        let m = unframe(&reply).ok_or("malformed Rattach")?;
        if m.ty != T::Attach.reply() {
            return Err(rerror(&m.body).unwrap_or_else(|| "not Rattach".into()));
        }
        let qid = Qid::read(&mut R::new(m.body)).ok_or("short Rattach")?;

        let mut c = Chan::attach(DevId::Mnt, 0);
        c.qid = qid;
        c.fid = fid;
        Ok((mnt, c))
    }

    fn newtag(&mut self) -> u16 {
        self.tag = self.tag.wrapping_add(1);
        self.tag
    }

    fn newfid(&mut self) -> u32 {
        let f = self.nextfid;
        self.nextfid += 1;
        f
    }

    fn rpc(&mut self, ty: T, body: W) -> Result<Vec<u8>, String> {
        let tag = self.newtag();
        let reply = self.wire.rpc(&body.frame(ty as u8, tag))?;
        let m = unframe(&reply).ok_or("malformed reply")?;
        if m.ty != ty.reply() {
            return Err(rerror(m.body).unwrap_or_else(|| format!("unexpected reply {}", m.ty)));
        }
        Ok(m.body.to_vec())
    }

    /// `Twalk`. One message carries the whole path, and the reply says how
    /// many elements the server managed — fewer than asked is not an error,
    /// it is how "no such file" is reported partway along.
    pub fn walk(&mut self, from: u32, names: &[&str]) -> Result<(u32, Vec<Qid>), String> {
        let newfid = self.newfid();
        let mut w = W::new().u32(from).u32(newfid).u16(names.len() as u16);
        for n in names {
            w = w.s(n);
        }
        let body = self.rpc(T::Walk, w)?;
        let mut r = R::new(&body);
        let n = r.u16().ok_or("short Rwalk")? as usize;
        let mut qids = Vec::with_capacity(n);
        for _ in 0..n {
            qids.push(Qid::read(&mut r).ok_or("short Rwalk")?);
        }
        Ok((newfid, qids))
    }

    pub fn open(&mut self, fid: u32, mode: u8) -> Result<Qid, String> {
        let body = self.rpc(T::Open, W::new().u32(fid).u8(mode))?;
        Qid::read(&mut R::new(&body)).ok_or_else(|| "short Ropen".into())
    }

    /// `mntrdwr`'s loop (`devmnt.c:688`). **One RPC is not one read**: each is
    /// bounded by `msize - IOHDRSZ`, and the caller asked for `n`.
    pub fn read(&mut self, fid: u32, n: usize, off: u64) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        let mut off = off;
        let mut left = n;
        let cap = (self.msize as usize).saturating_sub(IOHDRSZ);
        while left > 0 {
            let want = left.min(cap);
            let body = self.rpc(T::Read, W::new().u32(fid).u64(off).u32(want as u32))?;
            let mut r = R::new(&body);
            let count = r.u32().ok_or("short Rread")? as usize;
            let data = &r.rest()[..count.min(r.rest().len())];
            if data.is_empty() {
                break;
            }
            out.extend_from_slice(data);
            off += data.len() as u64;
            left -= data.len();
        }
        Ok(out)
    }

    /// The same loop for `Twrite`. A server that accepts less than it was
    /// offered is normal, and the rest goes in the next message.
    pub fn write(&mut self, fid: u32, data: &[u8], off: u64) -> Result<usize, String> {
        let mut done = 0;
        let mut off = off;
        let cap = (self.msize as usize).saturating_sub(IOHDRSZ);
        while done < data.len() {
            let chunk = &data[done..(done + cap).min(data.len())];
            let body = self.rpc(T::Write, W::new().u32(fid).u64(off).u32(chunk.len() as u32).raw(chunk))?;
            let count = R::new(&body).u32().ok_or("short Rwrite")? as usize;
            if count == 0 {
                break;
            }
            done += count;
            off += count as u64;
        }
        Ok(done)
    }

    pub fn stat(&mut self, fid: u32) -> Result<Vec<u8>, String> {
        let body = self.rpc(T::Stat, W::new().u32(fid))?;
        let mut r = R::new(&body);
        let _n = r.u16().ok_or("short Rstat")?;
        Ok(r.rest().to_vec())
    }

    /// `Tclunk`. **A fid not clunked is a fid the server keeps**, and a client
    /// that leaks them runs a server out.
    pub fn clunk(&mut self, fid: u32) -> Result<(), String> {
        self.rpc(T::Clunk, W::new().u32(fid))?;
        Ok(())
    }

    pub fn remove(&mut self, fid: u32) -> Result<(), String> {
        self.rpc(T::Remove, W::new().u32(fid))?;
        Ok(())
    }
}

/// `Rerror` carries a string, which is why a 9P failure is a sentence and not
/// a number.
fn rerror(body: &[u8]) -> Option<String> {
    R::new(body).s().map(|s| s.to_string())
}

/// The device itself. Each mount is an instance, named by the channel's
/// `devno` — Plan 9's `c->dev`.
#[derive(Default)]
pub struct MntDev {
    mounts: Vec<Mnt>,
}

impl MntDev {
    pub fn new() -> MntDev {
        MntDev::default()
    }

    /// Attach a server over a transport, and answer with the root channel.
    /// This is what `mount(2)` calls once the fd has been turned into a wire.
    pub fn mount(
        &mut self,
        wire: Box<dyn Transport>,
        uname: &str,
        aname: &str,
    ) -> Result<Chan, String> {
        let (m, mut c) = Mnt::attach(wire, uname, aname)?;
        self.mounts.push(m);
        c.devno = (self.mounts.len() - 1) as u32;
        Ok(c)
    }

    fn mnt(&mut self, c: &Chan) -> Result<&mut Mnt, String> {
        self.mounts.get_mut(c.devno as usize).ok_or_else(|| "not mounted".into())
    }
}

impl Dev for MntDev {
    fn id(&self) -> DevId {
        DevId::Mnt
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    /// `#M` cannot be attached by name. `mount(2)` supplies a channel to a
    /// server; there is no server to reach by writing `#M` in a path, which
    /// is why `namec` refuses the letter (`chan.c`, and `namec.rs`).
    fn attach(&mut self, _spec: &str) -> Result<Chan, String> {
        Err("#M cannot be attached by name — use mount(2)".into())
    }

    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Qid>, String> {
        let m = self.mnt(c)?;
        let (fid, qids) = m.walk(c.fid, &[name])?;
        if qids.is_empty() {
            // the server managed none: no such file. Clunk the fid we asked
            // for, or the server keeps it.
            let _ = m.clunk(fid);
            return Ok(None);
        }
        Ok(Some(qids[0]))
    }

    fn open(&mut self, mut c: Chan, mode: u16) -> Result<Chan, String> {
        let m = self.mnt(&c)?;
        c.qid = m.open(c.fid, mode as u8)?;
        c.mode = mode;
        Ok(c)
    }

    fn create(&mut self, c: &mut Chan, name: &str, mode: u16, perm: u32) -> Result<(), String> {
        let m = self.mnt(c)?;
        let body = m.rpc(
            T::Create,
            W::new().u32(c.fid).s(name).u32(perm).u8(mode as u8),
        )?;
        c.qid = Qid::read(&mut R::new(&body)).ok_or("short Rcreate")?;
        c.mode = mode;
        Ok(())
    }

    fn read(&mut self, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String> {
        let fid = c.fid;
        self.mnt(c)?.read(fid, n, off)
    }

    fn write(&mut self, c: &mut Chan, data: &[u8], off: u64) -> Result<usize, String> {
        let fid = c.fid;
        self.mnt(c)?.write(fid, data, off)
    }

    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        let (fid, devno) = (c.fid, c.devno);
        let m = self.mounts.get_mut(devno as usize).ok_or("not mounted")?;
        m.stat(fid)
    }

    fn wstat(&mut self, c: &mut Chan, edir: &[u8]) -> Result<(), String> {
        let fid = c.fid;
        self.mnt(c)?
            .rpc(T::Wstat, W::new().u32(fid).u16(edir.len() as u16).raw(edir))?;
        Ok(())
    }

    fn remove(&mut self, c: &mut Chan) -> Result<(), String> {
        let fid = c.fid;
        self.mnt(c)?.remove(fid)
    }

    /// Clunking is not optional bookkeeping: the server holds the fid until
    /// it is told to let go.
    fn close(&mut self, c: &mut Chan) {
        let fid = c.fid;
        if let Ok(m) = self.mnt(c) {
            let _ = m.clunk(fid);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ninep::{NOFID, QTDIR};
    use std::collections::HashMap;

    /// A 9P server, in process, so the test exercises the CONVERSATION and
    /// not just this side of it. It serves one flat directory and keeps fids
    /// the way a real server does — which is what makes the clunk test mean
    /// something.
    struct Server {
        files: HashMap<String, Vec<u8>>,
        fids: HashMap<u32, String>,
        msize: u32,
        /// Every fid ever handed out, so a leak is visible.
        handed: usize,
    }

    impl Server {
        fn new(msize: u32) -> Server {
            let mut files = HashMap::new();
            files.insert("hello".into(), b"from a server".to_vec());
            files.insert("big".into(), vec![b'x'; 5000]);
            Server { files, fids: HashMap::new(), msize, handed: 0 }
        }

        fn reply(&mut self, req: &[u8]) -> Vec<u8> {
            let m = unframe(req).expect("malformed request");
            let mut r = R::new(m.body);
            let ty = m.ty;
            let tag = m.tag;
            let err = |e: &str| W::new().s(e).frame(T::Error.reply(), tag);

            match ty {
                x if x == T::Version as u8 => {
                    let asked = r.u32().unwrap();
                    let v = r.s().unwrap();
                    let msize = asked.min(self.msize);
                    if v != VERSION {
                        return W::new().u32(msize).s("unknown").frame(T::Version.reply(), tag);
                    }
                    W::new().u32(msize).s(VERSION).frame(T::Version.reply(), tag)
                }
                x if x == T::Attach as u8 => {
                    let fid = r.u32().unwrap();
                    let _afid = r.u32().unwrap();
                    self.fids.insert(fid, String::new());
                    self.handed += 1;
                    let q = Qid { qtype: QTDIR, vers: 0, path: 0 };
                    W::new().raw(&q.write(W::new()).into_body()).frame(T::Attach.reply(), tag)
                }
                x if x == T::Walk as u8 => {
                    let from = r.u32().unwrap();
                    let newfid = r.u32().unwrap();
                    let n = r.u16().unwrap();
                    if !self.fids.contains_key(&from) {
                        return err("unknown fid");
                    }
                    let mut qids = Vec::new();
                    let mut at = String::new();
                    for _ in 0..n {
                        let name = r.s().unwrap();
                        if !self.files.contains_key(name) {
                            break;
                        }
                        at = name.to_string();
                        qids.push(Qid { qtype: 0, vers: 0, path: 1 });
                    }
                    // a fid is handed out even for a partial walk
                    self.fids.insert(newfid, at);
                    self.handed += 1;
                    let mut w = W::new().u16(qids.len() as u16);
                    for q in &qids {
                        w = w.raw(&q.write(W::new()).into_body());
                    }
                    w.frame(T::Walk.reply(), tag)
                }
                x if x == T::Open as u8 => {
                    let _fid = r.u32().unwrap();
                    let q = Qid { qtype: 0, vers: 0, path: 1 };
                    W::new().raw(&q.write(W::new()).into_body()).u32(0).frame(T::Open.reply(), tag)
                }
                x if x == T::Read as u8 => {
                    let fid = r.u32().unwrap();
                    let off = r.u64().unwrap() as usize;
                    let count = r.u32().unwrap() as usize;
                    let Some(name) = self.fids.get(&fid) else { return err("unknown fid") };
                    let Some(data) = self.files.get(name) else { return err("no such file") };
                    let end = (off + count).min(data.len());
                    let slice = if off >= data.len() { &[][..] } else { &data[off..end] };
                    W::new().u32(slice.len() as u32).raw(slice).frame(T::Read.reply(), tag)
                }
                x if x == T::Clunk as u8 => {
                    let fid = r.u32().unwrap();
                    self.fids.remove(&fid);
                    W::new().frame(T::Clunk.reply(), tag)
                }
                _ => err("not implemented by this server"),
            }
        }
    }

    /// The transport: hand a request straight to the server. A pipe or a
    /// network connection differs only in how the bytes travel.
    struct Loopback(std::rc::Rc<std::cell::RefCell<Server>>);

    impl Transport for Loopback {
        fn rpc(&mut self, request: &[u8]) -> Result<Vec<u8>, String> {
            Ok(self.0.borrow_mut().reply(request))
        }
    }

    fn mounted(msize: u32) -> (MntDev, Chan, std::rc::Rc<std::cell::RefCell<Server>>) {
        let s = std::rc::Rc::new(std::cell::RefCell::new(Server::new(msize)));
        let mut d = MntDev::new();
        let c = d.mount(Box::new(Loopback(s.clone())), "kitty", "").unwrap();
        (d, c, s)
    }

    /// Version, attach, walk, open, read — the whole conversation, over a
    /// server that speaks real 9P.
    #[test]
    fn a_mounted_server_answers_a_walk_and_a_read() {
        let (mut d, root, _) = mounted(MAXRPC);
        let mut c = root.clone();
        c.qid = d.walk(&root, "hello").unwrap().expect("no hello");
        c.fid = 2;
        let mut c = d.open(c, 0).unwrap();
        assert_eq!(d.read(&mut c, 64, 0).unwrap(), b"from a server");
    }

    /// `Tversion` first, or there is no session. The negotiated msize is the
    /// smaller of the two.
    #[test]
    fn the_msize_is_the_smaller_of_what_each_side_offers() {
        let (d, _, _) = mounted(2048);
        assert_eq!(d.mounts[0].msize, 2048, "the server's, being smaller");
        let (d, _, _) = mounted(1 << 20);
        assert_eq!(
            d.mounts[0].msize, MAXRPC,
            "ours, being smaller — and MAXRPC, not MAXCMNRPC: asking for the \
             negotiation buffer's size would cap every session at the old 8K"
        );
        assert_ne!(MAXRPC, MAXCMNRPC);
    }

    /// **One RPC is not one read.** `mntrdwr` loops, bounded by
    /// `msize - IOHDRSZ`, so a file bigger than a message still arrives
    /// whole. A driver that sent one message would return a short read here
    /// and pass every other test in this file.
    #[test]
    fn a_read_larger_than_one_message_is_still_whole() {
        let (mut d, root, _) = mounted(IOHDRSZ as u32 + 1024);
        let mut c = root.clone();
        c.qid = d.walk(&root, "big").unwrap().unwrap();
        c.fid = 2;
        let mut c = d.open(c, 0).unwrap();
        let got = d.read(&mut c, 5000, 0).unwrap();
        assert_eq!(got.len(), 5000, "the loop must keep going past one message");
        assert!(got.iter().all(|b| *b == b'x'));
    }

    /// A failed walk must clunk the fid it asked for. A server keeps every
    /// fid it hands out, so a client that leaks them runs it out — which is a
    /// bug that only shows under load.
    #[test]
    fn a_failed_walk_does_not_leak_a_fid() {
        let (mut d, root, s) = mounted(MAXRPC);
        for _ in 0..50 {
            assert!(d.walk(&root, "nothing").unwrap().is_none());
        }
        assert!(s.borrow().handed > 50, "the server did hand them out");
        assert_eq!(s.borrow().fids.len(), 1, "only the attach fid is still held");
    }

    /// `Rerror` carries a sentence, which is why a 9P failure says what went
    /// wrong rather than answering a number.
    #[test]
    fn an_error_from_the_server_arrives_as_its_own_words() {
        let (mut d, root, _) = mounted(MAXRPC);
        let mut c = root.clone();
        c.fid = 99; // a fid the server never handed out
        let e = d.read(&mut c, 8, 0).unwrap_err();
        assert!(e.contains("unknown fid"), "{e}");
    }

    /// `#M` is not reachable by writing its letter in a path: `mount(2)`
    /// supplies the channel, and there is no server to find by name.
    #[test]
    fn the_mount_driver_cannot_be_attached_by_name() {
        assert!(MntDev::new().attach("").is_err());
    }

    /// A server that will not speak 9P2000 is refused, rather than being
    /// talked at in a protocol it does not know.
    #[test]
    fn a_server_that_speaks_another_version_is_refused() {
        struct Rude;
        impl Transport for Rude {
            fn rpc(&mut self, req: &[u8]) -> Result<Vec<u8>, String> {
                let m = unframe(req).unwrap();
                Ok(W::new().u32(8192).s("9P1000").frame(T::Version.reply(), m.tag))
            }
        }
        let e = MntDev::new().mount(Box::new(Rude), "kitty", "").unwrap_err();
        assert!(e.contains("9P1000"), "{e}");
        let _ = NOFID;
    }
}
