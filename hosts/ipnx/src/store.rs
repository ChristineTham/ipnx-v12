//! The 9P server the machine provides — a directory on the host, served to
//! the kernel through `#9`.
//!
//! This is qemu's `-fsdev local` half, the thing on the far side of
//! `devvirtio9p.c`'s virtqueue: *"mount a host directory exported by qemu's
//! `-device virtio-9p-pci` / `-fsdev local`"*. Here there is no virtqueue and
//! no qemu — the host is the machine — so the exchange is a function call and
//! the version is plain 9P2000, which spares the 9P2000.u shim Plan 9's
//! device needs.
//!
//! **It is not part of the kernel and it is not a guest process.** It is what
//! the embedding serves, and Saranos includes the embedding: *"It is a
//! symbiosis between host and WASM, neither can exist without the other."*
//!
//! Nothing here is a file server of this system's own — a file server here is
//! a userspace program, and when there is one (it needs two processes to run
//! at once) this is what it would replace.

use ipnx_kernel::devvirtio9p::Nineserver;
use ipnx_kernel::ninep::{unframe, Dir, Qid, R, T, W, DMDIR, QTDIR};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// `Emaxmsg` — what this server will accept. `mntversion` asks for `MAXRPC`
/// and the server answers what it can do (`devmnt.c:153`: `f.msize = msize`).
const MSIZE: u32 = 8192 + 24;

pub struct Store {
    root: PathBuf,
    /// The fids in play. 9P's whole state is here: a fid is a name the client
    /// chose for a file it has walked to.
    fids: HashMap<u32, Fid>,
}

struct Fid {
    path: PathBuf,
    /// Open for writing, so a `Twrite` is allowed. A server that does not
    /// check this is not a server.
    mode: u8,
    open: bool,
}

impl Store {
    /// Serve `root`, making it if it is not there. A directory the host keeps
    /// is a directory that survives a boot, which is the whole point.
    pub fn new(root: &Path) -> std::io::Result<Store> {
        std::fs::create_dir_all(root)?;
        Ok(Store { root: root.to_path_buf(), fids: HashMap::new() })
    }

    /// Where a fid's path really is. **Every name is checked against the
    /// root**: a walk that climbs out of it is refused, because a client that
    /// can say `..` can otherwise say anything.
    fn real(&self, p: &Path) -> Option<PathBuf> {
        let full = self.root.join(p);
        let mut out = self.root.clone();
        for part in full.strip_prefix(&self.root).ok()?.components() {
            use std::path::Component::*;
            match part {
                Normal(n) => out.push(n),
                ParentDir => {
                    if out == self.root {
                        return None;
                    }
                    out.pop();
                }
                CurDir => {}
                _ => return None,
            }
        }
        Some(out)
    }

    /// A qid for a path. The path's own bytes decide it, hashed, so the same
    /// file answers the same qid across a boot — which is what a qid promises.
    fn qid(&self, p: &Path, dir: bool) -> Qid {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in p.as_os_str().as_encoded_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        Qid { qtype: if dir { QTDIR } else { 0 }, vers: 0, path: h }
    }

    fn dirof(&self, rel: &Path, name: &str) -> Option<Dir> {
        let real = self.real(rel)?;
        let md = std::fs::metadata(&real).ok()?;
        let dir = md.is_dir();
        Some(Dir {
            dtype: '9' as u16,
            dev: 0,
            qid: self.qid(rel, dir),
            mode: if dir { DMDIR | 0o755 } else { 0o644 },
            atime: 0,
            mtime: 0,
            length: if dir { 0 } else { md.len() },
            name: name.to_string(),
            uid: "eve".into(),
            gid: "eve".into(),
            muid: "eve".into(),
        })
    }
}

/// `Rerror` — a server refuses by answering, never by failing the transport.
fn err(msg: &str, tag: u16) -> Vec<u8> {
    W::new().s(msg).frame(T::Error as u8 + 1, tag)
}

impl Nineserver for Store {
    fn rpc(&mut self, t: &[u8]) -> Result<Vec<u8>, String> {
        let m = match unframe(t) {
            Some(m) => m,
            None => return Err("malformed 9P message".into()),
        };
        let mut r = R::new(m.body);
        let tag = m.tag;

        Ok(match m.ty {
            x if x == T::Version as u8 => {
                let msize = r.u32().unwrap_or(MSIZE);
                let v = r.s().unwrap_or("");
                // `version(5)`: answer a version we speak, or "unknown".
                let v = if v.starts_with("9P2000") { "9P2000" } else { "unknown" };
                W::new().u32(msize.min(MSIZE)).s(v).frame(T::Version.reply(), tag)
            }

            x if x == T::Attach as u8 => {
                let Some(fid) = r.u32() else { return Ok(err("short Tattach", tag)) };
                self.fids.insert(
                    fid,
                    Fid { path: PathBuf::new(), mode: 0, open: false },
                );
                let q = self.qid(Path::new(""), true);
                W::new().raw(&q.write(W::new()).into_body()).frame(T::Attach.reply(), tag)
            }

            x if x == T::Walk as u8 => {
                let (Some(from), Some(newfid), Some(n)) = (r.u32(), r.u32(), r.u16()) else {
                    return Ok(err("short Twalk", tag));
                };
                let Some(start) = self.fids.get(&from).map(|f| f.path.clone()) else {
                    return Ok(err("unknown fid", tag));
                };
                let mut at = start;
                let mut qids = Vec::new();
                for _ in 0..n {
                    let Some(name) = r.s() else { return Ok(err("short Twalk", tag)) };
                    let next = at.join(name);
                    let Some(real) = self.real(&next) else {
                        break;
                    };
                    let Ok(md) = std::fs::metadata(&real) else { break };
                    at = next;
                    qids.push(self.qid(&at, md.is_dir()));
                }
                // `walk(5)`: a walk that got nowhere at all is an error only
                // when it was asked for more than nothing.
                if qids.len() != n as usize && qids.is_empty() && n > 0 {
                    return Ok(err("file does not exist", tag));
                }
                if qids.len() == n as usize {
                    self.fids.insert(newfid, Fid { path: at, mode: 0, open: false });
                }
                let mut w = W::new().u16(qids.len() as u16);
                for q in &qids {
                    w = w.raw(&q.write(W::new()).into_body());
                }
                w.frame(T::Walk.reply(), tag)
            }

            x if x == T::Open as u8 => {
                let (Some(fid), Some(mode)) = (r.u32(), r.u8()) else {
                    return Ok(err("short Topen", tag));
                };
                let Some(f) = self.fids.get_mut(&fid) else {
                    return Ok(err("unknown fid", tag));
                };
                f.mode = mode;
                f.open = true;
                let path = f.path.clone();
                let Some(real) = self.real(&path) else {
                    return Ok(err("file does not exist", tag));
                };
                let Ok(md) = std::fs::metadata(&real) else {
                    return Ok(err("file does not exist", tag));
                };
                if mode & 0x10 != 0 && !md.is_dir() {
                    // `OTRUNC`
                    let _ = std::fs::write(&real, b"");
                }
                let q = self.qid(&path, md.is_dir());
                W::new()
                    .raw(&q.write(W::new()).into_body())
                    .u32(MSIZE - 24)
                    .frame(T::Open.reply(), tag)
            }

            x if x == T::Create as u8 => {
                let (Some(fid), Some(name), Some(perm), Some(mode)) =
                    (r.u32(), r.s().map(str::to_string), r.u32(), r.u8())
                else {
                    return Ok(err("short Tcreate", tag));
                };
                let Some(f) = self.fids.get(&fid) else {
                    return Ok(err("unknown fid", tag));
                };
                let next = f.path.join(&name);
                let Some(real) = self.real(&next) else {
                    return Ok(err("bad name", tag));
                };
                let isdir = perm & DMDIR != 0;
                let made = if isdir {
                    std::fs::create_dir(&real)
                } else {
                    std::fs::write(&real, b"")
                };
                if let Err(e) = made {
                    return Ok(err(&e.to_string(), tag));
                }
                let q = self.qid(&next, isdir);
                if let Some(f) = self.fids.get_mut(&fid) {
                    f.path = next;
                    f.mode = mode;
                    f.open = true;
                }
                W::new()
                    .raw(&q.write(W::new()).into_body())
                    .u32(MSIZE - 24)
                    .frame(T::Create.reply(), tag)
            }

            x if x == T::Read as u8 => {
                let (Some(fid), Some(off), Some(count)) = (r.u32(), r.u64(), r.u32()) else {
                    return Ok(err("short Tread", tag));
                };
                let Some(f) = self.fids.get(&fid) else {
                    return Ok(err("unknown fid", tag));
                };
                let path = f.path.clone();
                let Some(real) = self.real(&path) else {
                    return Ok(err("file does not exist", tag));
                };
                let Ok(md) = std::fs::metadata(&real) else {
                    return Ok(err("file does not exist", tag));
                };
                let data = if md.is_dir() {
                    // A directory reads as `Dir` entries, and an entry is
                    // never split — so the offset must land on one.
                    let mut all = Vec::new();
                    let mut names: Vec<String> = std::fs::read_dir(&real)
                        .map(|rd| {
                            rd.flatten()
                                .filter_map(|e| e.file_name().into_string().ok())
                                .collect()
                        })
                        .unwrap_or_default();
                    names.sort();
                    for name in names {
                        if let Some(d) = self.dirof(&path.join(&name), &name) {
                            all.extend_from_slice(&d.conv_d2m());
                        }
                    }
                    let at = (off as usize).min(all.len());
                    let end = (at + count as usize).min(all.len());
                    all[at..end].to_vec()
                } else {
                    match std::fs::read(&real) {
                        Ok(b) => {
                            let at = (off as usize).min(b.len());
                            let end = (at + count as usize).min(b.len());
                            b[at..end].to_vec()
                        }
                        Err(e) => return Ok(err(&e.to_string(), tag)),
                    }
                };
                W::new()
                    .u32(data.len() as u32)
                    .raw(&data)
                    .frame(T::Read.reply(), tag)
            }

            x if x == T::Write as u8 => {
                let (Some(fid), Some(off), Some(count)) = (r.u32(), r.u64(), r.u32()) else {
                    return Ok(err("short Twrite", tag));
                };
                let data = &r.rest()[..(count as usize).min(r.rest().len())];
                let Some(f) = self.fids.get(&fid) else {
                    return Ok(err("unknown fid", tag));
                };
                if !f.open || f.mode & 3 == 0 {
                    return Ok(err("permission denied", tag));
                }
                let Some(real) = self.real(&f.path) else {
                    return Ok(err("file does not exist", tag));
                };
                let mut b = std::fs::read(&real).unwrap_or_default();
                let at = off as usize;
                if b.len() < at + data.len() {
                    b.resize(at + data.len(), 0);
                }
                b[at..at + data.len()].copy_from_slice(data);
                if let Err(e) = std::fs::write(&real, &b) {
                    return Ok(err(&e.to_string(), tag));
                }
                W::new().u32(data.len() as u32).frame(T::Write.reply(), tag)
            }

            x if x == T::Stat as u8 => {
                let Some(fid) = r.u32() else { return Ok(err("short Tstat", tag)) };
                let Some(f) = self.fids.get(&fid) else {
                    return Ok(err("unknown fid", tag));
                };
                let path = f.path.clone();
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("/")
                    .to_string();
                match self.dirof(&path, &name) {
                    Some(d) => {
                        let b = d.conv_d2m();
                        W::new().u16(b.len() as u16).raw(&b).frame(T::Stat.reply(), tag)
                    }
                    None => err("file does not exist", tag),
                }
            }

            x if x == T::Clunk as u8 => {
                if let Some(fid) = r.u32() {
                    self.fids.remove(&fid);
                }
                W::new().frame(T::Clunk.reply(), tag)
            }

            x if x == T::Remove as u8 => {
                let Some(fid) = r.u32() else { return Ok(err("short Tremove", tag)) };
                let Some(f) = self.fids.remove(&fid) else {
                    return Ok(err("unknown fid", tag));
                };
                let Some(real) = self.real(&f.path) else {
                    return Ok(err("file does not exist", tag));
                };
                let gone = if real.is_dir() {
                    std::fs::remove_dir(&real)
                } else {
                    std::fs::remove_file(&real)
                };
                match gone {
                    Ok(()) => W::new().frame(T::Remove.reply(), tag),
                    Err(e) => err(&e.to_string(), tag),
                }
            }

            _ => err("not implemented", tag),
        })
    }
}
