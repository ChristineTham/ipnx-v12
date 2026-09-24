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
    /// `++chanalloc.fid`, which the transport sees so that a call run again
    /// after a sleep is given the fids it was given the first time
    /// ([`crate::namec::Devtab`]'s record). `next` is the counter's next.
    fn fid(&mut self, next: u32) -> u32 {
        next
    }
}

/// **The process is asleep waiting for a server's reply** — `mountio`'s
/// *"sleep(&r->r, rpcattn, r)"* (`devmnt.c:811`). Not an error: a call that
/// meets it leaves the processor, and runs again when the reply has come,
/// with the RPCs it already made answered from the record rather than sent
/// again. Nothing reports it to a process.
pub const SLEPT: &str = "asleep for a reply";

/// One mounted server — Plan 9's `Mnt`.
///
/// **It holds the wire CHANNEL, not a transport.** Plan 9's `Mnt` keeps
/// `m->c` and looks the device up at each RPC — `devtab[m->c->type]->bwrite`
/// (`mountio`) — because `devtab` is a global it can reach from anywhere.
/// Storing a transport instead would mean holding a borrow on the device
/// table for the life of the mount, which is the thing Plan 9 never does.
pub struct Mnt {
    pub msize: u32,
    pub wire: Chan,
    tag: u16,
}

impl Mnt {
    /// `mntversion` then `mntattach`: negotiate, then attach. Neither is
    /// optional — a server that has not agreed a version answers nothing.
    /// `mntversion` (`devmnt.c:100`), and only when the wire has no session:
    /// `mntattach` does it once and joins afterwards (`:317`, `m = c->mux`).
    pub fn version(wire: &Chan, t: &mut dyn Transport) -> Result<u32, String> {
        // `mntversion` asks for MAXRPC (`devmnt.c:118`, `msize = MAXRPC`).
        // MAXCMNRPC is only the BUFFER the exchange itself uses
        // (`:155`, `msg = malloc(MAXCMNRPC)`) — asking for it instead would
        // cap every session at the old 8K and nothing would look wrong.
        let mut msize = MAXRPC;
        // `if(msize > c->iounit && c->iounit != 0) msize = c->iounit`
        // (`devmnt.c:119`): the wire's own chunk size bounds the session.
        if wire.iounit != 0 && msize > wire.iounit {
            msize = wire.iounit;
        }
        let req = W::new().u32(msize).s(VERSION).frame(T::Version as u8, !0);
        let reply = t.rpc(&req)?;
        let m = unframe(&reply).ok_or("malformed Rversion")?;
        if m.ty != T::Version.reply() {
            return Err(rerror(m.body).unwrap_or_else(|| "not Rversion".into()));
        }
        let mut r = R::new(m.body);
        let got = r.u32().ok_or("short Rversion")?;
        let version = r.s().ok_or("short Rversion")?;
        if version != VERSION {
            return Err(format!("server speaks {version}, not {VERSION}"));
        }
        Ok(got.min(msize))
    }

    pub fn attach(
        wire: Chan,
        msize: u32,
        t: &mut dyn Transport,
        uname: &str,
        aname: &str,
        fid: u32,
    ) -> Result<(Mnt, Chan), String> {
        let mut mnt = Mnt { msize, wire, tag: 0 };
        // Tattach: fid, afid (NOFID — no authentication), uname, aname
        let req = W::new()
            .u32(fid)
            .u32(crate::ninep::NOFID)
            .s(uname)
            .s(aname)
            .frame(T::Attach as u8, mnt.newtag());
        let reply = t.rpc(&req)?;
        let m = unframe(&reply).ok_or("malformed Rattach")?;
        if m.ty != T::Attach.reply() {
            return Err(rerror(&m.body).unwrap_or_else(|| "not Rattach".into()));
        }
        let qid = Qid::read(&mut R::new(m.body)).ok_or("short Rattach")?;

        let mut c = Chan::attach(DevId::Mnt, 0);
        c.qid = qid;
        c.fid = fid;
        // `c->mchan = m->c` (`devmnt.c:354`), and the file's own comment
        // (`:14`): *"Each channel derived from the mount point has mchan set
        // to c"*. It is how `#p/<n>/ns` names the server a mount speaks to —
        // `srvname(mw->cm->to->mchan)` — and it was never set, so every
        // mount printed as `#M`.
        c.mchan = Some(Box::new(mnt.wire.clone()));
        c.mqid = qid;
        Ok((mnt, c))
    }

    fn newtag(&mut self) -> u16 {
        self.tag = self.tag.wrapping_add(1);
        self.tag
    }

    fn rpc(&mut self, t: &mut dyn Transport, ty: T, body: W) -> Result<Vec<u8>, String> {
        let tag = self.newtag();
        let reply = t.rpc(&body.frame(ty as u8, tag))?;
        let m = unframe(&reply).ok_or("malformed reply")?;
        if m.ty != ty.reply() {
            return Err(rerror(m.body).unwrap_or_else(|| format!("unexpected reply {}", m.ty)));
        }
        Ok(m.body.to_vec())
    }

    /// `Twalk`. One message carries the whole path, and the reply says how
    /// many elements the server managed — fewer than asked is not an error,
    /// it is how "no such file" is reported partway along.
    pub fn walk(&mut self, t: &mut dyn Transport, from: u32, newfid: u32, names: &[&str]) -> Result<(u32, Vec<Qid>), String> {
        let mut w = W::new().u32(from).u32(newfid).u16(names.len() as u16);
        for n in names {
            w = w.s(n);
        }
        let body = self.rpc(t, T::Walk, w)?;
        let mut r = R::new(&body);
        let n = r.u16().ok_or("short Rwalk")? as usize;
        let mut qids = Vec::with_capacity(n);
        for _ in 0..n {
            qids.push(Qid::read(&mut r).ok_or("short Rwalk")?);
        }
        Ok((newfid, qids))
    }

    pub fn open(&mut self, t: &mut dyn Transport, fid: u32, mode: u8) -> Result<Qid, String> {
        let body = self.rpc(t, T::Open, W::new().u32(fid).u8(mode))?;
        Qid::read(&mut R::new(&body)).ok_or_else(|| "short Ropen".into())
    }

    /// `mntrdwr`'s loop (`devmnt.c:688`). **One RPC is not one read**: each is
    /// bounded by `msize - IOHDRSZ`, and the caller asked for `n`.
    pub fn read(&mut self, t: &mut dyn Transport, fid: u32, n: usize, off: u64) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        let mut off = off;
        let mut left = n;
        let cap = (self.msize as usize).saturating_sub(IOHDRSZ);
        while left > 0 {
            let want = left.min(cap);
            let body = self.rpc(t, T::Read, W::new().u32(fid).u64(off).u32(want as u32))?;
            let mut r = R::new(&body);
            let count = r.u32().ok_or("short Rread")? as usize;
            let data = &r.rest()[..count.min(r.rest().len()).min(want)];
            out.extend_from_slice(data);
            off += data.len() as u64;
            left -= data.len();
            // *"if(nr != nreq || n == 0 || up->nnote) break"*
            // (`devmnt.c:733`): **a short reply ends the read.** A server
            // answers a read with what it has — a plumb port with one
            // message — and asking again waits for the next.
            if data.len() != want {
                break;
            }
        }
        Ok(out)
    }

    /// The same loop for `Twrite`. A server that accepts less than it was
    /// offered is normal, and the rest goes in the next message.
    pub fn write(&mut self, t: &mut dyn Transport, fid: u32, data: &[u8], off: u64) -> Result<usize, String> {
        let mut done = 0;
        let mut off = off;
        let cap = (self.msize as usize).saturating_sub(IOHDRSZ);
        while done < data.len() {
            let chunk = &data[done..(done + cap).min(data.len())];
            let body = self.rpc(t, T::Write, W::new().u32(fid).u64(off).u32(chunk.len() as u32).raw(chunk))?;
            let count = (R::new(&body).u32().ok_or("short Rwrite")? as usize).min(chunk.len());
            done += count;
            off += count as u64;
            // `devmnt.c:733`, as for a read
            if count != chunk.len() {
                break;
            }
        }
        Ok(done)
    }

    pub fn stat(&mut self, t: &mut dyn Transport, fid: u32) -> Result<Vec<u8>, String> {
        let body = self.rpc(t, T::Stat, W::new().u32(fid))?;
        let mut r = R::new(&body);
        let _n = r.u16().ok_or("short Rstat")?;
        Ok(r.rest().to_vec())
    }

    /// `Tclunk`. **A fid not clunked is a fid the server keeps**, and a client
    /// that leaks them runs a server out.
    pub fn clunk(&mut self, t: &mut dyn Transport, fid: u32) -> Result<(), String> {
        self.rpc(t, T::Clunk, W::new().u32(fid))?;
        Ok(())
    }

    pub fn remove(&mut self, t: &mut dyn Transport, fid: u32) -> Result<(), String> {
        self.rpc(t, T::Remove, W::new().u32(fid))?;
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
///
/// **It does not implement [`Dev`].** Every other device answers from what it
/// holds; this one has to reach the device serving its wire, which in Plan 9
/// is `devtab[m->c->type]` — a global. Here the table dispatches to it
/// specially, handing it a transport, and while it runs it is OUT of the
/// table: see [`crate::namec::Devtab`]. That is the same property Plan 9 gets
/// for free, since `mntalloc` (`devmnt.c:48`) was never in `devtab` either.
#[derive(Default)]
pub struct MntDev {
    mounts: Vec<Mnt>,
    /// `chanalloc.fid` (`chan.c:20`). **A fid is unique across the whole
    /// kernel, not per mount**: Plan 9 gives every channel its own when it
    /// is allocated — *"c->fid = ++chanalloc.fid"* (`chan.c:250`) — and
    /// sends that as the fid (`devmnt.c:344`, `:425`). Two mounts of one
    /// wire share one 9P session, and so one fid space: with a counter per
    /// mount, each began at 1, and a clunk through one mount took a file
    /// from under the other. Plan 9 recycles channels and with them fids;
    /// this counts up, which a u32 affords.
    fid: u32,
}

impl MntDev {
    pub fn new() -> MntDev {
        MntDev::default()
    }

    /// `++chanalloc.fid`.
    fn newfid(&mut self, t: &mut dyn Transport) -> u32 {
        let f = t.fid(self.fid + 1);
        self.fid = self.fid.max(f);
        f
    }

    /// `mntattach` (`devmnt.c:303`). The channel handed in **is the wire**.
    ///
    /// **A wire already carrying a session is joined, not re-versioned**:
    /// `m = c->mux`, and `mntversion` runs only when that is nil (`:317`).
    /// Two mounts of one channel are two attaches over one 9P session.
    pub fn mount(
        &mut self,
        mut wire: Chan,
        t: &mut dyn Transport,
        uname: &str,
        aname: &str,
    ) -> Result<Chan, String> {
        let msize = match wire.mux.and_then(|i| self.mounts.get(i as usize)) {
            Some(m) => m.msize,
            None => Mnt::version(&wire, t)?,
        };
        let joined = wire.mux;
        wire.mux = Some(self.mounts.len() as u32);
        let fid = self.newfid(t);
        let (m, mut c) = Mnt::attach(wire, msize, t, uname, aname, fid)?;
        self.mounts.push(m);
        c.devno = (self.mounts.len() - 1) as u32;
        c.mux = joined.or(Some(c.devno));
        Ok(c)
    }

    /// Which mount a channel belongs to, and the wire it speaks down.
    pub fn wire_of(&self, c: &Chan) -> Option<Chan> {
        self.mounts.get(c.devno as usize).map(|m| m.wire.clone())
    }

    fn mnt(&mut self, c: &Chan) -> Result<&mut Mnt, String> {
        self.mounts.get_mut(c.devno as usize).ok_or_else(|| "not mounted".into())
    }

    /// **The walked channel carries the FID the walk minted**, which is the
    /// whole reason a device produces the new channel rather than a qid
    /// (`devwalk` fills `nc`, `dev.c:169`). A `Twalk` names a new fid for the
    /// file it reached; drop it and every channel through this mount carries
    /// the mount ROOT's fid, so a `Tcreate` moves the root onto the new file
    /// and nothing resolves through the mount again.
    pub fn walk(
        &mut self,
        t: &mut dyn Transport,
        c: &Chan,
        name: &str,
    ) -> Result<Option<Chan>, String> {
        let newfid = self.newfid(t);
        let m = self.mnt(c)?;
        let (fid, qids) = m.walk(t, c.fid, newfid, &[name])?;
        if qids.is_empty() {
            // The server managed none: no such file. Clunk the fid we asked
            // for, or the server keeps it.
            let _ = m.clunk(t, fid);
            return Ok(None);
        }
        let mut nc = c.walked(name, qids[0]);
        nc.fid = fid;
        Ok(Some(nc))
    }

    /// `cclone` — a `Twalk` with NO names, which is how 9P says *"another
    /// name for this same file"* (`cclone`, `chan.c:842`:
    /// `devtab[c->type]->walk(c, nil, nil, 0)`).
    pub fn cclone(&mut self, t: &mut dyn Transport, c: &Chan) -> Result<Chan, String> {
        let newfid = self.newfid(t);
        let m = self.mnt(c)?;
        let (fid, _) = m.walk(t, c.fid, newfid, &[])?;
        let mut nc = c.clone();
        nc.fid = fid;
        Ok(nc)
    }

    pub fn open(&mut self, t: &mut dyn Transport, mut c: Chan, mode: u16) -> Result<Chan, String> {
        c.qid = self.mnt(&c)?.open(t, c.fid, mode as u8)?;
        c.mode = mode;
        Ok(c)
    }

    pub fn create(
        &mut self,
        t: &mut dyn Transport,
        c: &mut Chan,
        name: &str,
        mode: u16,
        perm: u32,
    ) -> Result<(), String> {
        let fid = c.fid;
        let body = self.mnt(c)?.rpc(t, T::Create, W::new().u32(fid).s(name).u32(perm).u8(mode as u8))?;
        c.qid = Qid::read(&mut R::new(&body)).ok_or("short Rcreate")?;
        c.mode = mode;
        Ok(())
    }

    pub fn read(&mut self, t: &mut dyn Transport, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String> {
        let fid = c.fid;
        self.mnt(c)?.read(t, fid, n, off)
    }

    pub fn write(&mut self, t: &mut dyn Transport, c: &mut Chan, data: &[u8], off: u64) -> Result<usize, String> {
        let fid = c.fid;
        self.mnt(c)?.write(t, fid, data, off)
    }

    pub fn stat(&mut self, t: &mut dyn Transport, c: &Chan) -> Result<Vec<u8>, String> {
        let fid = c.fid;
        let m = self.mounts.get_mut(c.devno as usize).ok_or("not mounted")?;
        m.stat(t, fid)
    }

    pub fn wstat(&mut self, t: &mut dyn Transport, c: &mut Chan, edir: &[u8]) -> Result<(), String> {
        let fid = c.fid;
        self.mnt(c)?.rpc(t, T::Wstat, W::new().u32(fid).u16(edir.len() as u16).raw(edir))?;
        Ok(())
    }

    pub fn remove(&mut self, t: &mut dyn Transport, c: &mut Chan) -> Result<(), String> {
        let fid = c.fid;
        self.mnt(c)?.remove(t, fid)
    }

    /// Clunking is not optional bookkeeping: the server holds the fid until it
    /// is told to let go.
    pub fn close(&mut self, t: &mut dyn Transport, c: &mut Chan) {
        let fid = c.fid;
        if let Ok(m) = self.mnt(c) {
            let _ = m.clunk(t, fid);
        }
    }
}

/// `#M` lives in the table like any device so that `take`/`put` and the
/// channel's letter work uniformly — but every operation reaches it through
/// [`crate::namec::Devtab`]'s dispatcher, which hands it a transport. These
/// say so loudly rather than failing quietly, because a direct call means the
/// dispatcher was bypassed.
impl Dev for MntDev {
    fn id(&self) -> DevId {
        DevId::Mnt
    }
    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
    /// `#M` cannot be attached by name: `mount(2)` supplies the channel, and
    /// there is no server to reach by writing `#M` in a path.
    fn attach(&mut self, _spec: &str) -> Result<Chan, String> {
        Err("#M cannot be attached by name — use mount(2)".into())
    }
    fn walk(&mut self, _c: &Chan, _n: &str) -> Result<Option<Chan>, String> {
        Err(DIRECT.into())
    }
    fn open(&mut self, _c: Chan, _m: u16) -> Result<Chan, String> {
        Err(DIRECT.into())
    }
    fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
        Err(DIRECT.into())
    }
    fn read(&mut self, _c: &mut Chan, _n: usize, _o: u64) -> Result<Vec<u8>, String> {
        Err(DIRECT.into())
    }
    fn write(&mut self, _c: &mut Chan, _d: &[u8], _o: u64) -> Result<usize, String> {
        Err(DIRECT.into())
    }
    fn stat(&mut self, _c: &Chan) -> Result<Vec<u8>, String> {
        Err(DIRECT.into())
    }
    fn wstat(&mut self, _c: &mut Chan, _e: &[u8]) -> Result<(), String> {
        Err(DIRECT.into())
    }
    fn remove(&mut self, _c: &mut Chan) -> Result<(), String> {
        Err(DIRECT.into())
    }
    fn close(&mut self, _c: &mut Chan) {}
}

const DIRECT: &str = "#M reached directly: it needs a transport, so it is \
                      dispatched through Devtab";

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
        /// How many times each was asked for, so joining a session is visible.
        versions: usize,
        attaches: usize,
        reads: usize,
    }

    impl Server {
        fn new(msize: u32) -> Server {
            let mut files = HashMap::new();
            files.insert("hello".into(), b"from a server".to_vec());
            files.insert("big".into(), vec![b'x'; 5000]);
            Server { files, fids: HashMap::new(), msize, handed: 0, versions: 0, attaches: 0, reads: 0 }
        }

        fn reply(&mut self, req: &[u8]) -> Vec<u8> {
            let m = unframe(req).expect("malformed request");
            let mut r = R::new(m.body);
            let ty = m.ty;
            let tag = m.tag;
            let err = |e: &str| W::new().s(e).frame(T::Error.reply(), tag);

            match ty {
                x if x == T::Version as u8 => {
                    self.versions += 1;
                    let asked = r.u32().unwrap();
                    let v = r.s().unwrap();
                    let msize = asked.min(self.msize);
                    if v != VERSION {
                        return W::new().u32(msize).s("unknown").frame(T::Version.reply(), tag);
                    }
                    W::new().u32(msize).s(VERSION).frame(T::Version.reply(), tag)
                }
                x if x == T::Attach as u8 => {
                    self.attaches += 1;
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
                    self.reads += 1;
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

    fn mounted(msize: u32) -> (MntDev, Loopback, Chan, std::rc::Rc<std::cell::RefCell<Server>>) {
        let s = std::rc::Rc::new(std::cell::RefCell::new(Server::new(msize)));
        let mut d = MntDev::new();
        let mut t = Loopback(s.clone());
        // the wire is a channel; a loopback stands in for the device serving it
        let c = d.mount(Chan::attach(DevId::Pipe, 0), &mut t, "kitty", "").unwrap();
        (d, t, c, s)
    }

    /// Version, attach, walk, open, read — the whole conversation, over a
    /// server that speaks real 9P.
    #[test]
    fn a_mounted_server_answers_a_walk_and_a_read() {
        let (mut d, mut t, root, _) = mounted(MAXRPC);
        let mut c = d.walk(&mut t, &root, "hello").unwrap().expect("no hello");
        c.fid = 2;
        let mut c = d.open(&mut t, c, 0).unwrap();
        assert_eq!(d.read(&mut t, &mut c, 64, 0).unwrap(), b"from a server");
    }

    /// **Two mounts of one wire share one fid space**, so no fid may be
    /// handed out twice across them (`chan.c:250`, *"c->fid =
    /// ++chanalloc.fid"*). With a counter per mount both began at 1: a clunk
    /// through the first took the second's file away, and `newns`'s `/home`
    /// answered *"unknown fid"* because `boot`'s mount of the same wire had
    /// closed a channel.
    #[test]
    fn two_mounts_of_one_wire_never_share_a_fid() {
        let (mut d, mut t, a, _) = mounted(MAXRPC);
        let mut wire = Chan::attach(DevId::Pipe, 0);
        wire.mux = a.mux;
        let b = d.mount(wire, &mut t, "kitty", "").unwrap();
        assert_ne!(a.fid, b.fid, "two attaches, one session");
        let mut x = d.walk(&mut t, &a, "hello").unwrap().unwrap();
        let y = d.walk(&mut t, &b, "hello").unwrap().unwrap();
        assert_ne!(x.fid, y.fid);
        d.close(&mut t, &mut x);
        let mut y = d.open(&mut t, y, 0).unwrap();
        assert_eq!(d.read(&mut t, &mut y, 64, 0).unwrap(), b"from a server");
    }

    /// `Tversion` first, or there is no session. The negotiated msize is the
    /// smaller of the two.
    #[test]
    fn the_msize_is_the_smaller_of_what_each_side_offers() {
        let (d, _, _, _) = mounted(2048);
        assert_eq!(d.mounts[0].msize, 2048, "the server's, being smaller");
        let (d, _, _, _) = mounted(1 << 20);
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
        let (mut d, mut t, root, _) = mounted(IOHDRSZ as u32 + 1024);
        let mut c = d.walk(&mut t, &root, "big").unwrap().unwrap();
        c.fid = 2;
        let mut c = d.open(&mut t, c, 0).unwrap();
        let got = d.read(&mut t, &mut c, 5000, 0).unwrap();
        assert_eq!(got.len(), 5000, "the loop must keep going past one message");
        assert!(got.iter().all(|b| *b == b'x'));
    }

    /// **A short reply ends the read** — *"if(nr != nreq …) break"*
    /// (`devmnt.c:733`). Asking again after one would wait on a server
    /// that answers a read with what it has, as a plumb port does, until
    /// its next message.
    #[test]
    fn a_short_reply_ends_the_read() {
        let (mut d, mut t, root, s) = mounted(MAXRPC);
        let c = d.walk(&mut t, &root, "hello").unwrap().unwrap();
        let mut c = d.open(&mut t, c, 0).unwrap();
        assert_eq!(d.read(&mut t, &mut c, 64, 0).unwrap(), b"from a server");
        assert_eq!(s.borrow().reads, 1, "one Tread, answered short, and no second");
    }

    /// A failed walk must clunk the fid it asked for. A server keeps every
    /// fid it hands out, so a client that leaks them runs it out — which is a
    /// bug that only shows under load.
    #[test]
    fn a_failed_walk_does_not_leak_a_fid() {
        let (mut d, mut t, root, s) = mounted(MAXRPC);
        for _ in 0..50 {
            assert!(d.walk(&mut t, &root, "nothing").unwrap().is_none());
        }
        assert!(s.borrow().handed > 50, "the server did hand them out");
        assert_eq!(s.borrow().fids.len(), 1, "only the attach fid is still held");
    }

    /// `Rerror` carries a sentence, which is why a 9P failure says what went
    /// wrong rather than answering a number.
    #[test]
    fn an_error_from_the_server_arrives_as_its_own_words() {
        let (mut d, mut t, root, _) = mounted(MAXRPC);
        let mut c = root.clone();
        c.fid = 99; // a fid the server never handed out
        let e = d.read(&mut t, &mut c, 8, 0).unwrap_err();
        assert!(e.contains("unknown fid"), "{e}");
    }

    /// `mntattach` joins a session already on the wire rather than versioning
    /// again: `m = c->mux`, and `mntversion` runs only when that is nil
    /// (`devmnt.c:317`). Two mounts of one channel are two ATTACHES over one
    /// session — a second `Tversion` would reset the server's state.
    #[test]
    fn a_second_mount_of_one_wire_joins_its_session() {
        let s = std::rc::Rc::new(std::cell::RefCell::new(Server::new(MAXRPC)));
        let mut d = MntDev::new();
        let mut t = Loopback(s.clone());
        let wire = Chan::attach(DevId::Pipe, 0);

        let first = d.mount(wire.clone(), &mut t, "kitty", "").unwrap();
        let versions = s.borrow().versions;
        assert_eq!(versions, 1);

        // the wire now carries the session
        let mut again = wire.clone();
        again.mux = first.mux;
        d.mount(again, &mut t, "kitty", "").unwrap();
        assert_eq!(s.borrow().versions, 1, "a second Tversion was sent");
        assert_eq!(s.borrow().attaches, 2, "and the second attach was not");
    }

    /// `iounit` bounds the session: `if(msize > c->iounit && c->iounit != 0)`
    /// (`devmnt.c:119`). A wire that can only carry so much per message says
    /// so, and the mount must not ask for more.
    #[test]
    fn the_wires_iounit_bounds_the_session() {
        let s = std::rc::Rc::new(std::cell::RefCell::new(Server::new(MAXRPC)));
        let mut d = MntDev::new();
        let mut t = Loopback(s.clone());
        let mut wire = Chan::attach(DevId::Pipe, 0);
        wire.iounit = 4096;
        d.mount(wire, &mut t, "kitty", "").unwrap();
        assert_eq!(d.mounts[0].msize, 4096, "the wire's own limit, not MAXRPC");
    }

    /// `#M` is not reachable by writing its letter in a path: `mount(2)`
    /// supplies the channel, and there is no server to find by name.
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
        let e = MntDev::new()
            .mount(Chan::attach(DevId::Pipe, 0), &mut Rude, "kitty", "")
            .unwrap_err();
        assert!(e.contains("9P1000"), "{e}");
        let _ = NOFID;
    }
}
