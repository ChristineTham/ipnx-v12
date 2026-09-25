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
use ipnx_kernel::ninep::{unframe, Dir, Qid, R, T, W, DMDIR, QTDIR, QTEXCL};
use std::collections::HashMap;
use std::fs::Metadata;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// `Emaxmsg` — what this server will accept. `mntversion` asks for `MAXRPC`
/// and the server answers what it can do (`devmnt.c:153`: `f.msize = msize`).
const MSIZE: u32 = 8192 + 24;

pub struct Store {
    root: PathBuf,
    /// The `uname` of the `Tattach`, and the owner of every file this server
    /// reports. `u9fs` does the same: an exported tree has no users of its
    /// own, so it answers with the one who attached. It was the fixed string
    /// `"eve"`, which stopped being anybody's name once `boot` started
    /// naming the host owner.
    uname: String,
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
        Ok(Store { root: root.to_path_buf(), uname: String::new(), fids: HashMap::new() })
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

    /// A qid and a directory entry for a path, from the host's own `stat` —
    /// or nothing, when there is no such file.
    fn stat(&self, rel: &Path) -> Option<Metadata> {
        std::fs::metadata(self.real(rel)?).ok()
    }

    /// `stat2dir` (`plan9/sys/src/cmd/unix/u9fs/u9fs.c:694`): the times,
    /// the length and the permission bits are the host file's own.
    fn dirof(&self, rel: &Path, name: &str) -> Option<Dir> {
        let md = self.stat(rel)?;
        Some(Dir {
            dtype: '9' as u16,
            dev: 0,
            qid: stat2qid(&md),
            mode: plan9mode(&md),
            atime: md.atime() as u32,
            mtime: md.mtime() as u32,
            length: md.len(),
            name: name.to_string(),
            uid: self.uname.clone(),
            gid: self.uname.clone(),
            muid: self.uname.clone(),
        })
    }
}

/// `modebyte` (`u9fs.c:591`): a directory is `QTDIR`, and a device —
/// there is no testing exclusive use — is marked `QTEXCL`.
fn modebyte(md: &Metadata) -> u8 {
    let ft = md.file_type();
    let mut b = 0;
    if ft.is_dir() {
        b |= QTDIR;
    }
    if ft.is_block_device() || ft.is_char_device() || ft.is_fifo() || ft.is_socket() {
        b |= QTEXCL;
    }
    b
}

/// `plan9mode` (`u9fs.c:609`): *"((ulong)modebyte(st)<<24) | (st->st_mode
/// & 0777)"*.
fn plan9mode(md: &Metadata) -> u32 {
    ((modebyte(md) as u32) << 24) | (md.mode() & 0o777)
}

/// `stat2qid` (`u9fs.c:624`): the path is the inode, the device number
/// folded into its top; the version is *"st->st_mtime ^ (st->st_size <<
/// 8)"*, so a file that changes is a new version of itself.
fn stat2qid(md: &Metadata) -> Qid {
    Qid {
        qtype: modebyte(md),
        vers: (md.mtime() as u32) ^ ((md.size() as u32) << 8),
        path: md.ino() ^ (md.dev() << 48),
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

            // `rauth` (`u9fs.c:380`) with `authnone` (`authnone.c:10`): this
            // server asks for no authentication, and says so.
            x if x == T::Auth as u8 => err("u9fs authnone: no authentication required", tag),

            x if x == T::Attach as u8 => {
                let Some(fid) = r.u32() else { return Ok(err("short Tattach", tag)) };
                // `afid[4] uname[s] aname[s]` — the afid is skipped and the
                // uname kept.
                r.u32();
                if let Some(u) = r.s() {
                    self.uname = u.to_string();
                }
                self.fids.insert(
                    fid,
                    Fid { path: PathBuf::new(), mode: 0, open: false },
                );
                let Some(md) = self.stat(Path::new("")) else {
                    return Ok(err("no root", tag));
                };
                let q = stat2qid(&md);
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
                    qids.push(stat2qid(&md));
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
                let q = stat2qid(&md);
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
                // `usercreate` (`u9fs.c:1605`): the permission asked for,
                // masked by the directory's — *"m = (perm & DMDIR) ? 0777 :
                // 0666; perm = perm & (~m | (fid->st.st_mode & m))"* — and a
                // directory is made at least readable by its owner.
                let isdir = perm & DMDIR != 0;
                let parent = self.stat(&f.path).map(|md| md.mode()).unwrap_or(0o777);
                let m = if isdir { 0o777 } else { 0o666 };
                let perm = perm & (!m | (parent & m));
                let made = if isdir {
                    std::fs::create_dir(&real).and_then(|_| {
                        std::fs::set_permissions(&real, std::fs::Permissions::from_mode((perm | 0o400) & 0o777))
                    })
                } else {
                    std::fs::OpenOptions::new().write(true).create_new(true).open(&real).and_then(|_| {
                        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(perm & 0o777))
                    })
                };
                if let Err(e) = made {
                    return Ok(err(&e.to_string(), tag));
                }
                let Ok(md) = std::fs::metadata(&real) else {
                    return Ok(err("file does not exist", tag));
                };
                let q = stat2qid(&md);
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
                    // `pread` (`rread`, `u9fs.c:717`): what is asked for,
                    // from where it is asked, and no more.
                    let mut b = vec![0; count.min(MSIZE - 24) as usize];
                    let got = std::fs::File::open(&real).and_then(|mut fd| {
                        fd.seek(SeekFrom::Start(off))?;
                        let mut n = 0;
                        while n < b.len() {
                            match fd.read(&mut b[n..])? {
                                0 => break,
                                k => n += k,
                            }
                        }
                        Ok(n)
                    });
                    match got {
                        Ok(n) => {
                            b.truncate(n);
                            b
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
                // `pwrite` (`rwrite`, `u9fs.c:809`), at the offset asked.
                let put = std::fs::OpenOptions::new().write(true).open(&real).and_then(|mut fd| {
                    fd.seek(SeekFrom::Start(off))?;
                    fd.write_all(data)
                });
                if let Err(e) = put {
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

            // `rwstat` (`u9fs.c:909`). A field of all ones is *"don't
            // touch"*, and the changes are made *"in increasing order of harm
            // to the file"*: the mode, the mtime, the name, the length.
            x if x == T::Wstat as u8 => {
                let (Some(fid), Some(n)) = (r.u32(), r.u16()) else {
                    return Ok(err("short Twstat", tag));
                };
                let rest = r.rest();
                let Some(d) = Dir::conv_m2d(&rest[..(n as usize).min(rest.len())]) else {
                    return Ok(err("bad stat buffer", tag));
                };
                let Some(f) = self.fids.get(&fid) else {
                    return Ok(err("unknown fid", tag));
                };
                let path = f.path.clone();
                let Some(md) = self.stat(&path) else {
                    return Ok(err("file does not exist", tag));
                };
                let Some(mut real) = self.real(&path) else {
                    return Ok(err("file does not exist", tag));
                };
                if d.mode != !0 && ((d.mode & DMDIR != 0) != md.is_dir()) {
                    return Ok(err("can't change directory bit", tag));
                }
                if path.as_os_str().is_empty() {
                    return Ok(err("no wstat of root", tag));
                }
                if d.mode != !0 {
                    if let Err(e) = std::fs::set_permissions(&real, std::fs::Permissions::from_mode(d.mode & 0o777)) {
                        return Ok(err(&e.to_string(), tag));
                    }
                }
                if d.mtime != !0 {
                    let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(d.mtime as u64);
                    let set = std::fs::File::options()
                        .write(!md.is_dir())
                        .read(md.is_dir())
                        .open(&real)
                        .and_then(|fd| fd.set_modified(t));
                    if let Err(e) = set {
                        return Ok(err(&e.to_string(), tag));
                    }
                }
                if !d.name.is_empty() {
                    let new = path.with_file_name(&d.name);
                    let Some(to) = self.real(&new) else {
                        return Ok(err("bad name", tag));
                    };
                    if new != path {
                        if let Err(e) = std::fs::rename(&real, &to) {
                            return Ok(err(&e.to_string(), tag));
                        }
                        if let Some(f) = self.fids.get_mut(&fid) {
                            f.path = new;
                        }
                        real = to;
                    }
                }
                if d.length != !0 {
                    let cut = std::fs::OpenOptions::new().write(true).open(&real).and_then(|fd| fd.set_len(d.length));
                    if let Err(e) = cut {
                        return Ok(err(&e.to_string(), tag));
                    }
                }
                W::new().frame(T::Wstat.reply(), tag)
            }

            // `Tflush`: every request here is answered before the next is
            // read, so there is never one outstanding to abandon.
            x if x == T::Flush as u8 => W::new().frame(T::Flush.reply(), tag),

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
