//! The 9P server the machine provides — a directory on the host, served to
//! the kernel through `#9`.
//!
//! **The 9P is the same whatever keeps the files** ([`Backend`]): a directory
//! on the host's disk on a terminal ([`HostDir`]), and in the browser the
//! page's own tree (`hosts/web`).
//!
//! This is qemu's `-fsdev local` half, the thing on the far side of
//! `devvirtio9p.c`'s virtqueue: *"mount a host directory exported by qemu's
//! `-device virtio-9p-pci` / `-fsdev local`"*. Here there is no virtqueue and
//! no qemu — the host is the machine — so the exchange is a function call and
//! the version is plain 9P2000, which spares the 9P2000.u shim Plan 9's
//! device needs.
//!
//! **It is not part of the kernel and it is not a wasm process.** It is what
//! the embedding serves, and Saranos includes the embedding: *"It is a
//! symbiosis between host and WASM, neither can exist without the other."*
//!
//! Nothing here is a file server of this system's own — a file server here is
//! a userspace program, and when there is one (it needs two processes to run
//! at once) this is what it would replace.

use ipnx_kernel::devvirtio9p::Nineserver;
use ipnx_kernel::ninep::{unframe, Dir, Qid, R, T, W, DMDIR, QTDIR, QTEXCL};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// `ORCLOSE`, of an open's mode: remove the file when the fid is clunked.
const ORCLOSE: u8 = 0x40;

/// `Emaxmsg` — what this server will accept. `mntversion` asks for `MAXRPC`
/// and the server answers what it can do (`devmnt.c:153`: `f.msize = msize`).
const MSIZE: u32 = 8192 + 24;

/// **What a file is**, as the server reports it — `stat(2)`'s answer, and
/// whatever keeps the files supplies it.
pub struct Meta {
    pub dir: bool,
    /// A device, a fifo or a socket: there is no testing exclusive use.
    pub excl: bool,
    /// The permission bits, `0777`.
    pub perm: u32,
    pub len: u64,
    pub atime: u32,
    pub mtime: u32,
    /// What names the file uniquely: `u9fs`'s inode with the device folded in.
    pub path: u64,
}

/// **Where the files are kept.** The 9P below is the same whatever keeps
/// them; this is what it asks of the keeper, each path relative to the root
/// and already checked never to climb out of it ([`Store::real`]).
pub trait Backend {
    fn meta(&mut self, p: &Path) -> Option<Meta>;
    /// The names in a directory.
    fn list(&mut self, p: &Path) -> Vec<String>;
    /// At most `n` bytes from `off`.
    fn read(&mut self, p: &Path, off: u64, n: usize) -> Result<Vec<u8>, String>;
    fn write(&mut self, p: &Path, off: u64, data: &[u8]) -> Result<(), String>;
    fn truncate(&mut self, p: &Path, len: u64) -> Result<(), String>;
    fn mkdir(&mut self, p: &Path, perm: u32) -> Result<(), String>;
    /// A new file — an error if there is one already.
    fn mkfile(&mut self, p: &Path, perm: u32) -> Result<(), String>;
    fn chmod(&mut self, p: &Path, perm: u32) -> Result<(), String>;
    fn touch(&mut self, p: &Path, mtime: u32) -> Result<(), String>;
    fn rename(&mut self, from: &Path, to: &Path) -> Result<(), String>;
    fn remove(&mut self, p: &Path) -> Result<(), String>;
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Store<B: Backend = HostDir> {
    files: B,
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

#[cfg(target_arch = "wasm32")]
pub struct Store<B: Backend> {
    files: B,
    uname: String,
    fids: HashMap<u32, Fid>,
}

struct Fid {
    path: PathBuf,
    /// Open for writing, so a `Twrite` is allowed. A server that does not
    /// check this is not a server.
    mode: u8,
    open: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl Store<HostDir> {
    /// Serve `root`, making it if it is not there. A directory the host keeps
    /// is a directory that survives a boot, which is the whole point.
    pub fn new(root: &Path) -> std::io::Result<Store<HostDir>> {
        std::fs::create_dir_all(root)?;
        Ok(Store::with(HostDir { root: root.to_path_buf() }))
    }
}

impl<B: Backend> Store<B> {
    /// Serve whatever `files` keeps.
    pub fn with(files: B) -> Store<B> {
        Store { files, uname: String::new(), fids: HashMap::new() }
    }

    /// Where a fid's path really is, relative to the root. **Every name is
    /// checked against the root**: a walk that climbs out of it is refused,
    /// because a client that can say `..` can otherwise say anything.
    fn real(&self, p: &Path) -> Option<PathBuf> {
        let mut out = PathBuf::new();
        for part in p.components() {
            use std::path::Component::*;
            match part {
                Normal(n) => out.push(n),
                ParentDir => {
                    if !out.pop() {
                        return None;
                    }
                }
                CurDir => {}
                _ => return None,
            }
        }
        Some(out)
    }

    /// A qid and a directory entry for a path, from the keeper's own `stat`
    /// — or nothing, when there is no such file.
    fn stat(&mut self, rel: &Path) -> Option<Meta> {
        let real = self.real(rel)?;
        self.files.meta(&real)
    }

    /// `stat2dir` (`plan9/sys/src/cmd/unix/u9fs/u9fs.c:694`): the times,
    /// the length and the permission bits are the file's own.
    fn dirof(&mut self, rel: &Path, name: &str) -> Option<Dir> {
        let md = self.stat(rel)?;
        Some(Dir {
            dtype: '9' as u16,
            dev: 0,
            qid: stat2qid(&md),
            mode: plan9mode(&md),
            atime: md.atime,
            mtime: md.mtime,
            length: md.len,
            name: name.to_string(),
            uid: self.uname.clone(),
            gid: self.uname.clone(),
            muid: self.uname.clone(),
        })
    }
}

/// **A directory on the host's disk** — what a terminal keeps its files in.
#[cfg(not(target_arch = "wasm32"))]
pub struct HostDir {
    root: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl Backend for HostDir {
    fn meta(&mut self, p: &Path) -> Option<Meta> {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        let md = std::fs::metadata(self.root.join(p)).ok()?;
        let ft = md.file_type();
        Some(Meta {
            dir: ft.is_dir(),
            excl: ft.is_block_device() || ft.is_char_device() || ft.is_fifo() || ft.is_socket(),
            perm: md.mode() & 0o777,
            len: md.len(),
            atime: md.atime() as u32,
            mtime: md.mtime() as u32,
            path: md.ino() ^ (md.dev() << 48),
        })
    }
    fn list(&mut self, p: &Path) -> Vec<String> {
        std::fs::read_dir(self.root.join(p))
            .map(|rd| rd.flatten().filter_map(|e| e.file_name().into_string().ok()).collect())
            .unwrap_or_default()
    }
    fn read(&mut self, p: &Path, off: u64, n: usize) -> Result<Vec<u8>, String> {
        use std::io::{Read, Seek, SeekFrom};
        let mut b = vec![0; n];
        let got = std::fs::File::open(self.root.join(p)).and_then(|mut fd| {
            fd.seek(SeekFrom::Start(off))?;
            let mut k = 0;
            while k < b.len() {
                match fd.read(&mut b[k..])? {
                    0 => break,
                    m => k += m,
                }
            }
            Ok(k)
        });
        match got {
            Ok(k) => {
                b.truncate(k);
                Ok(b)
            }
            Err(e) => Err(e.to_string()),
        }
    }
    fn write(&mut self, p: &Path, off: u64, data: &[u8]) -> Result<(), String> {
        use std::io::{Seek, SeekFrom, Write};
        std::fs::OpenOptions::new()
            .write(true)
            .open(self.root.join(p))
            .and_then(|mut fd| {
                fd.seek(SeekFrom::Start(off))?;
                fd.write_all(data)
            })
            .map_err(|e| e.to_string())
    }
    fn truncate(&mut self, p: &Path, len: u64) -> Result<(), String> {
        std::fs::OpenOptions::new()
            .write(true)
            .open(self.root.join(p))
            .and_then(|fd| fd.set_len(len))
            .map_err(|e| e.to_string())
    }
    fn mkdir(&mut self, p: &Path, perm: u32) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let real = self.root.join(p);
        std::fs::create_dir(&real)
            .and_then(|_| std::fs::set_permissions(&real, std::fs::Permissions::from_mode(perm)))
            .map_err(|e| e.to_string())
    }
    fn mkfile(&mut self, p: &Path, perm: u32) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let real = self.root.join(p);
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&real)
            .and_then(|_| std::fs::set_permissions(&real, std::fs::Permissions::from_mode(perm)))
            .map_err(|e| e.to_string())
    }
    fn chmod(&mut self, p: &Path, perm: u32) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(self.root.join(p), std::fs::Permissions::from_mode(perm)).map_err(|e| e.to_string())
    }
    fn touch(&mut self, p: &Path, mtime: u32) -> Result<(), String> {
        let real = self.root.join(p);
        let dir = real.is_dir();
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(mtime as u64);
        std::fs::File::options()
            .write(!dir)
            .read(dir)
            .open(&real)
            .and_then(|fd| fd.set_modified(t))
            .map_err(|e| e.to_string())
    }
    fn rename(&mut self, from: &Path, to: &Path) -> Result<(), String> {
        std::fs::rename(self.root.join(from), self.root.join(to)).map_err(|e| e.to_string())
    }
    fn remove(&mut self, p: &Path) -> Result<(), String> {
        let real = self.root.join(p);
        let gone = if real.is_dir() { std::fs::remove_dir(&real) } else { std::fs::remove_file(&real) };
        gone.map_err(|e| e.to_string())
    }
}

/// `modebyte` (`u9fs.c:591`): a directory is `QTDIR`, and a device —
/// there is no testing exclusive use — is marked `QTEXCL`.
fn modebyte(md: &Meta) -> u8 {
    let mut b = 0;
    if md.dir {
        b |= QTDIR;
    }
    if md.excl {
        b |= QTEXCL;
    }
    b
}

/// `plan9mode` (`u9fs.c:609`): *"((ulong)modebyte(st)<<24) | (st->st_mode
/// & 0777)"*.
fn plan9mode(md: &Meta) -> u32 {
    ((modebyte(md) as u32) << 24) | (md.perm & 0o777)
}

/// `stat2qid` (`u9fs.c:624`): the path is the inode, the device number
/// folded into its top; the version is *"st->st_mtime ^ (st->st_size <<
/// 8)"*, so a file that changes is a new version of itself.
fn stat2qid(md: &Meta) -> Qid {
    Qid { qtype: modebyte(md), vers: md.mtime ^ ((md.len as u32) << 8), path: md.path }
}

/// `Rerror` — a server refuses by answering, never by failing the transport.
fn err(msg: &str, tag: u16) -> Vec<u8> {
    W::new().s(msg).frame(T::Error as u8, tag)
}

impl<B: Backend> Nineserver for Store<B> {
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
                self.fids.insert(fid, Fid { path: PathBuf::new(), mode: 0, open: false });
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
                    let Some(md) = self.stat(&next) else { break };
                    at = self.real(&next).unwrap_or(next);
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
                let Some(md) = self.stat(&path) else {
                    return Ok(err("file does not exist", tag));
                };
                if mode & 0x10 != 0 && !md.dir {
                    // `OTRUNC`
                    let _ = self.files.truncate(&path, 0);
                }
                let q = stat2qid(&md);
                W::new().raw(&q.write(W::new()).into_body()).u32(MSIZE - 24).frame(T::Open.reply(), tag)
            }

            x if x == T::Create as u8 => {
                let (Some(fid), Some(name), Some(perm), Some(mode)) =
                    (r.u32(), r.s().map(str::to_string), r.u32(), r.u8())
                else {
                    return Ok(err("short Tcreate", tag));
                };
                let Some(dir) = self.fids.get(&fid).map(|f| f.path.clone()) else {
                    return Ok(err("unknown fid", tag));
                };
                let Some(next) = self.real(&dir.join(&name)) else {
                    return Ok(err("bad name", tag));
                };
                // `usercreate` (`u9fs.c:1605`): the permission asked for,
                // masked by the directory's — *"m = (perm & DMDIR) ? 0777 :
                // 0666; perm = perm & (~m | (fid->st.st_mode & m))"* — and a
                // directory is made at least readable by its owner.
                let isdir = perm & DMDIR != 0;
                let parent = self.stat(&dir).map(|md| md.perm).unwrap_or(0o777);
                let m = if isdir { 0o777 } else { 0o666 };
                let perm = perm & (!m | (parent & m));
                let made = if isdir {
                    self.files.mkdir(&next, (perm | 0o400) & 0o777)
                } else {
                    self.files.mkfile(&next, perm & 0o777)
                };
                if let Err(e) = made {
                    return Ok(err(&e, tag));
                }
                let Some(md) = self.stat(&next) else {
                    return Ok(err("file does not exist", tag));
                };
                let q = stat2qid(&md);
                if let Some(f) = self.fids.get_mut(&fid) {
                    f.path = next;
                    f.mode = mode;
                    f.open = true;
                }
                W::new().raw(&q.write(W::new()).into_body()).u32(MSIZE - 24).frame(T::Create.reply(), tag)
            }

            x if x == T::Read as u8 => {
                let (Some(fid), Some(off), Some(count)) = (r.u32(), r.u64(), r.u32()) else {
                    return Ok(err("short Tread", tag));
                };
                let Some(path) = self.fids.get(&fid).map(|f| f.path.clone()) else {
                    return Ok(err("unknown fid", tag));
                };
                let Some(md) = self.stat(&path) else {
                    return Ok(err("file does not exist", tag));
                };
                let data = if md.dir {
                    // **A directory reads as whole `Dir` entries**, from
                    // where the last read ended, as many as fit and never
                    // part of one: `rread` (`u9fs.c:752`) — *"if((n=convD2M(&d,
                    // p, ep-p)) <= BIT16SZ) break;"* (`:791`) — as read(5)
                    // requires, and as the mount driver checks (`mntread`'s
                    // *"if(p != e) error(Esbadstat)"*). It had cut the listing
                    // at the count, so a read of one longer than that ended
                    // in the middle of an entry.
                    let mut names = self.files.list(&path);
                    names.sort();
                    let mut out = Vec::new();
                    let mut at = 0u64;
                    for name in names {
                        let Some(d) = self.dirof(&path.join(&name), &name) else { continue };
                        let e = d.conv_d2m();
                        if at < off {
                            at += e.len() as u64;
                            continue;
                        }
                        if out.len() + e.len() > count as usize {
                            break;
                        }
                        out.extend_from_slice(&e);
                    }
                    out
                } else {
                    // `pread` (`rread`, `u9fs.c:717`): what is asked for,
                    // from where it is asked, and no more.
                    match self.files.read(&path, off, count.min(MSIZE - 24) as usize) {
                        Ok(b) => b,
                        Err(e) => return Ok(err(&e, tag)),
                    }
                };
                W::new().u32(data.len() as u32).raw(&data).frame(T::Read.reply(), tag)
            }

            x if x == T::Write as u8 => {
                let (Some(fid), Some(off), Some(count)) = (r.u32(), r.u64(), r.u32()) else {
                    return Ok(err("short Twrite", tag));
                };
                let data = r.rest()[..(count as usize).min(r.rest().len())].to_vec();
                let Some(f) = self.fids.get(&fid) else {
                    return Ok(err("unknown fid", tag));
                };
                if !f.open || f.mode & 3 == 0 {
                    return Ok(err("permission denied", tag));
                }
                let path = f.path.clone();
                // `pwrite` (`rwrite`, `u9fs.c:809`), at the offset asked.
                if let Err(e) = self.files.write(&path, off, &data) {
                    return Ok(err(&e, tag));
                }
                W::new().u32(data.len() as u32).frame(T::Write.reply(), tag)
            }

            x if x == T::Stat as u8 => {
                let Some(fid) = r.u32() else { return Ok(err("short Tstat", tag)) };
                let Some(path) = self.fids.get(&fid).map(|f| f.path.clone()) else {
                    return Ok(err("unknown fid", tag));
                };
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("/").to_string();
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
                let Some(mut path) = self.fids.get(&fid).map(|f| f.path.clone()) else {
                    return Ok(err("unknown fid", tag));
                };
                let Some(md) = self.stat(&path) else {
                    return Ok(err("file does not exist", tag));
                };
                if d.mode != !0 && ((d.mode & DMDIR != 0) != md.dir) {
                    return Ok(err("can't change directory bit", tag));
                }
                if path.as_os_str().is_empty() {
                    return Ok(err("no wstat of root", tag));
                }
                if d.mode != !0 {
                    if let Err(e) = self.files.chmod(&path, d.mode & 0o777) {
                        return Ok(err(&e, tag));
                    }
                }
                if d.mtime != !0 {
                    if let Err(e) = self.files.touch(&path, d.mtime) {
                        return Ok(err(&e, tag));
                    }
                }
                if !d.name.is_empty() {
                    let Some(new) = self.real(&path.with_file_name(&d.name)) else {
                        return Ok(err("bad name", tag));
                    };
                    if new != path {
                        if let Err(e) = self.files.rename(&path, &new) {
                            return Ok(err(&e, tag));
                        }
                        if let Some(f) = self.fids.get_mut(&fid) {
                            f.path = new.clone();
                        }
                        path = new;
                    }
                }
                if d.length != !0 {
                    if let Err(e) = self.files.truncate(&path, d.length) {
                        return Ok(err(&e, tag));
                    }
                }
                W::new().frame(T::Wstat.reply(), tag)
            }

            // `Tflush`: every request here is answered before the next is
            // read, so there is never one outstanding to abandon.
            x if x == T::Flush as u8 => W::new().frame(T::Flush.reply(), tag),

            // `rclunk` (`u9fs.c:866`): a file opened or made `ORCLOSE` is
            // removed when its fid goes — *"else if(fid->omode != -1 &&
            // fid->omode&ORCLOSE) … remove(rpath)"*. sam's temporary file is
            // made so (`sam/disk.c:16`).
            x if x == T::Clunk as u8 => {
                if let Some(f) = r.u32().and_then(|fid| self.fids.remove(&fid)) {
                    if f.open && f.mode & ORCLOSE != 0 {
                        let _ = self.files.remove(&f.path);
                    }
                }
                W::new().frame(T::Clunk.reply(), tag)
            }

            x if x == T::Remove as u8 => {
                let Some(fid) = r.u32() else { return Ok(err("short Tremove", tag)) };
                let Some(f) = self.fids.remove(&fid) else {
                    return Ok(err("unknown fid", tag));
                };
                match self.files.remove(&f.path) {
                    Ok(()) => W::new().frame(T::Remove.reply(), tag),
                    Err(e) => err(&e, tag),
                }
            }

            _ => err("not implemented", tag),
        })
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    /// **A directory read gives whole entries**, however small the count,
    /// and the next read the ones after them (`u9fs.c:752`).
    #[test]
    fn a_directory_reads_as_whole_entries() {
        let dir = std::env::temp_dir().join(format!("ipnx-dirread-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..40 {
            std::fs::write(dir.join(format!("file-with-a-longish-name-{i:02}")), "x").unwrap();
        }
        let mut s = Store::new(&dir).unwrap();
        let mut rpc = |b: W, t: T| {
            let r = s.rpc(&b.frame(t as u8, 1)).unwrap();
            assert_ne!(r[4], T::Error as u8, "{t:?}: {}", String::from_utf8_lossy(&r[9..]));
            r
        };
        rpc(W::new().u32(MSIZE).s("9P2000"), T::Version);
        rpc(W::new().u32(0).u32(!0).s("kitty").s(""), T::Attach);
        rpc(W::new().u32(0).u8(0), T::Open);
        let (mut off, mut names) = (0u64, Vec::new());
        loop {
            let r = rpc(W::new().u32(0).u64(off).u32(1000), T::Read);
            let n = u32::from_le_bytes([r[7], r[8], r[9], r[10]]) as usize;
            if n == 0 {
                break;
            }
            let entries = Dir::parse_all(&r[11..11 + n]);
            let whole: usize = entries.iter().map(|d| d.conv_d2m().len()).sum();
            assert_eq!(whole, n, "a read ended inside an entry");
            names.extend(entries.into_iter().map(|d| d.name));
            off += n as u64;
        }
        assert_eq!(names.len(), 40, "{names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file made `ORCLOSE` is there while it is open and gone when its fid
    /// is clunked; one made without it stays (`u9fs.c:866`).
    #[test]
    fn a_file_made_orclose_goes_when_its_fid_is_clunked() {
        let dir = std::env::temp_dir().join(format!("ipnx-orclose-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = Store::new(&dir).unwrap();
        let mut rpc = |b: W, t: T| {
            let r = s.rpc(&b.frame(t as u8, 1)).unwrap();
            assert_ne!(r[4], T::Error as u8, "{t:?}: {}", String::from_utf8_lossy(&r[9..]));
        };
        rpc(W::new().u32(MSIZE).s("9P2000"), T::Version);
        rpc(W::new().u32(0).u32(!0).s("kitty").s(""), T::Attach);
        for (fid, name, mode) in [(1, "gone", 1 | ORCLOSE), (2, "kept", 1)] {
            rpc(W::new().u32(0).u32(fid).u16(0), T::Walk);
            rpc(W::new().u32(fid).s(name).u32(0o644).u8(mode), T::Create);
            assert!(dir.join(name).exists(), "{name} was not made");
            rpc(W::new().u32(fid), T::Clunk);
        }
        assert!(!dir.join("gone").exists(), "a file made ORCLOSE outlived its fid");
        assert!(dir.join("kept").exists(), "a file made without ORCLOSE went");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
