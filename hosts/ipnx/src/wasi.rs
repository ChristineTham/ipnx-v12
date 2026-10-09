//! **A WASI binary, run natively** (architecture.md, *A WASI binary runs
//! natively*; Christine, 2026-10-08: *"WASI is not a personality, we support
//! WASI binaries natively"*). A module that imports `wasi_snapshot_preview1`
//! runs under `wasi-common`, wasmtime's own WASI preview 1, and **its files
//! are the process's namespace** (Christine, 2026-10-09: *"B: the
//! namespace"*): `/` is preopened as the namespace the process has, and
//! every open, read, write and stat the program makes is the kernel's call,
//! made for it by the machine ([`Syscalls::hostcall`]). Its standard streams
//! are the process's descriptors 0, 1 and 2, its arguments `exec`'s, its
//! environment the process's.
//!
//! **What WASI asks that Plan 9 has another way of saying** is answered as
//! APE, Plan 9's own POSIX, answers it: an error string is an errno by
//! `_syserrno`'s table (`ape/lib/ap/plan9/_errno.c`); a rename is `rename.c`'s;
//! an environment variable is `_envsetup.c`'s; `O_APPEND` is a seek to the
//! end before each write (`write.c`); an exit status is `_exit.c`'s. And a
//! sleep, or a wait for a file to be ready, is the kernel's `sleep` — a
//! read that cannot be answered yet waits in the kernel, as every read does.

use crate::machine::{kernel, Exited, Sched};
use ipnx_kernel::chan::mode::{OEXCL, OREAD, ORDWR, OTRUNC, OWRITE};
use ipnx_kernel::machine::{NoteAt, Notify};
use ipnx_kernel::ninep::{Dir, DMDIR, QTDIR};
use ipnx_kernel::{Call, Pid, Ret};
use std::any::Any;
use std::io::{IoSlice, IoSliceMut, SeekFrom};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wasi_common::dir::{OpenResult, ReaddirCursor, ReaddirEntity};
use wasi_common::file::{Advice, FdFlags, FileType, Filestat, OFlags};
use wasi_common::sched::subscription::{RwEventFlags, Subscription};
use wasi_common::snapshots::preview_1::error::Errno;
use wasi_common::{Error, Poll, SystemTimeSpec, Table, WasiCtx, WasiDir, WasiFile, WasiSched};

/// Whether `module` is a WASI program: it imports WASI's calls, preview 1's
/// or the snapshot before it.
pub fn is_wasi(module: &wasmtime::Module) -> bool {
    module.imports().any(|i| i.module() == "wasi_snapshot_preview1" || i.module() == "wasi_unstable")
}

/// **A call made for `pid`, to its end.** One that leaves the processor part
/// way goes back in through `resume` when the scheduler enters the process
/// again, as a process's own does (`machine.rs`, `kcall`). Then the notes:
/// one that ends the process ends the program, here, by trapping it — a WASI
/// program has no handler to give one to.
async fn call(pid: Pid, c: Call) -> Result<Ret, Error> {
    let mut r = hostcall(pid, c);
    while let Ok(Ret::Sched) = r {
        Sched::new().await;
        r = resume(pid);
    }
    notes(pid)?;
    r.map_err(|e| errno(&e))
}

fn hostcall(pid: Pid, c: Call) -> Result<Ret, String> {
    // Sound as `machine.rs`'s `call` is: this runs inside a poll, inside
    // `gotolabel`, where the kernel is entered and nothing else holds it.
    unsafe { (*kernel()).hostcall(pid, c) }
}

fn resume(pid: Pid) -> Result<Ret, String> {
    // Sound as [`hostcall`] is.
    unsafe { (*kernel()).resume(pid) }
}

/// `notify`'s decision at a call's end (`pc/trap.c:773`): a note that ends
/// the process traps the program, which ends it.
fn notes(pid: Pid) -> Result<(), Error> {
    // Sound as [`hostcall`] is.
    match unsafe { (*kernel()).notify(pid, NoteAt::Syscall) } {
        Notify::Pexit => Err(Error::trap(wasmtime::Error::new(Exited))),
        _ => Ok(()),
    }
}

/// **`_syserrno`** (`ape/lib/ap/plan9/_errno.c:15`, `:110`): the first entry
/// whose text the error contains, and `EINVAL` for none — each errno here
/// the WASI one of the same name. Two of APE's have no WASI name and are
/// the nearest: `ESHUTDOWN`, *"cannot send after transport endpoint
/// shutdown"*, is `EPIPE`; APE's own `EGREG` is `EIO`. And the host's own
/// words, which a server of a host's files answers with — `u9fs` answers
/// `strerror(errno)` — are matched without regard to case, with the
/// `EEXIST`, `EISDIR` and `ENOENT` of a host's: APE's table has no entry
/// that matches them.
fn errno(e: &str) -> Error {
    use Errno::*;
    const MAP: &[(&str, Errno)] = &[
        // from /sys/src/9/port/errstr.h
        ("inconsistent mount", Inval),
        ("not mounted", Inval),
        ("not in union", Inval),
        ("mount rpc error", Io),
        ("mounted device shut down", Io),
        ("mounted directory forbids creation", Perm),
        ("does not exist", Noent),
        ("unknown device in # filename", Nxio),
        ("not a directory", Notdir),
        ("file is a directory", Isdir),
        ("bad character in file name", Inval),
        ("file name syntax", Inval),
        ("permission denied", Perm),
        ("inappropriate use of fd", Perm),
        ("bad arg in system call", Inval),
        ("device or object already in use", Busy),
        ("i/o error", Io),
        ("read or write too large", Io),
        ("read or write too small", Io),
        ("network port not available", Addrinuse),
        ("write to hungup stream", Pipe),
        ("i/o on hungup channel", Pipe),
        ("bad process or channel control request", Inval),
        ("no free devices", Busy),
        ("process exited", Srch),
        ("no living children", Child),
        ("i/o error in demand load", Io),
        ("virtual memory allocation failed", Nomem),
        ("fd out of range or not open", Badf),
        ("no free file descriptors", Mfile),
        ("seek on a stream", Spipe),
        ("exec header invalid", Noexec),
        ("connection timed out", Timedout),
        ("connection refused", Connrefused),
        ("connection in use", Connrefused),
        ("interrupted", Intr),
        ("kernel allocate failed", Nomem),
        ("segments overlap", Inval),
        ("i/o count too small", Io),
        ("ken has left the building", Io),
        ("bad attach specifier", Inval),
        // from exhausted() calls in kernel
        ("no free mount devices", Busy),
        ("no free mount rpc buffer", Busy),
        ("no free segments", Busy),
        ("no free memory", Nomem),
        ("no free Blocks", Nobufs),
        ("no free routes", Busy),
        // from ken
        ("attach -- bad specifier", Inval),
        ("unknown fid", Badf),
        ("bad character in directory name", Inval),
        ("read/write -- on non open fid", Badf),
        ("read/write -- count too big", Io),
        ("phase error -- directory entry not allocated", Io),
        ("phase error -- qid does not match", Io),
        ("access permission denied", Acces),
        ("directory entry not found", Noent),
        ("open/create -- unknown mode", Inval),
        ("walk -- in a non-directory", Notdir),
        ("create -- in a non-directory", Notdir),
        ("phase error -- cannot happen", Io),
        ("create -- file exists", Exist),
        ("create -- . and .. illegal names", Inval),
        ("directory not empty", Notempty),
        ("attach -- privileged user", Inval),
        ("wstat -- not owner", Perm),
        ("wstat -- not in group", Perm),
        ("create/wstat -- bad character in file name", Inval),
        ("walk -- too many (system wide)", Busy),
        ("file system read only", Rofs),
        ("file system full", Nospc),
        ("read/write -- offset negative", Inval),
        ("open/create -- file is locked", Busy),
        ("close/read/write -- lock is broken", Busy),
        // from sockets
        ("not a socket", Notsock),
        ("protocol not supported", Protonosupport),
        ("address family not supported", Afnosupport),
        ("insufficient buffer space", Nobufs),
        ("operation not supported", Notsup),
        ("address in use", Addrinuse),
        ("unnamed error message", Io),
        // a host's, through a server of its files
        ("already exists", Exist),
        ("file exists", Exist),
        ("is a directory", Isdir),
        ("no such file or directory", Noent),
    ];
    let e = e.to_lowercase();
    let n = MAP.iter().find(|(s, _)| e.contains(&s.to_lowercase())).map_or(Inval, |(_, n)| *n);
    n.into()
}

/// What a file is, for WASI, from its directory entry: a directory; a file a
/// server serves (`#M`'s) or the environment's; a pipe; and otherwise a
/// device's file, which is what WASI calls a character device — a console
/// is one, and a program asks so to know whether it talks to a person.
fn filetype(d: &Dir) -> FileType {
    if d.qid.qtype & QTDIR != 0 {
        return FileType::Directory;
    }
    match char::from_u32(d.dtype as u32) {
        Some('M') | Some('e') => FileType::RegularFile,
        Some('|') => FileType::Pipe,
        _ => FileType::CharacterDevice,
    }
}

fn filestat(d: &Dir) -> Filestat {
    let t = |s: u32| Some(UNIX_EPOCH + Duration::from_secs(s as u64));
    Filestat {
        device_id: ((d.dtype as u64) << 32) | d.dev as u64,
        inode: d.qid.path,
        filetype: filetype(d),
        nlink: 1,
        size: d.length,
        atim: t(d.atime),
        mtim: t(d.mtime),
        // Plan 9 keeps no change time; the last modification is the
        // nearest
        ctim: t(d.mtime),
    }
}

fn dir(b: &[u8]) -> Result<Dir, Error> {
    Dir::conv_m2d(b).ok_or_else(|| Errno::Io.into())
}

/// `nulldir` (`libc/9sys/nulldir.c`): a `Dir` that changes nothing in a
/// `wstat`, every field *"don't touch"*.
fn nulldir() -> Dir {
    Dir {
        dtype: !0,
        dev: !0,
        qid: ipnx_kernel::ninep::Qid { qtype: !0, vers: !0, path: !0 },
        mode: !0,
        atime: !0,
        mtime: !0,
        length: !0,
        name: String::new(),
        uid: String::new(),
        gid: String::new(),
        muid: String::new(),
    }
}

/// A time WASI gives, in Plan 9's seconds.
fn secs(t: Option<SystemTimeSpec>) -> Option<u32> {
    match t? {
        SystemTimeSpec::SymbolicNow => Some(SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as u32)),
        SystemTimeSpec::Absolute(t) => {
            let t: SystemTime = t.into_std();
            Some(t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as u32))
        }
    }
}

/// **Descriptors a program let go of, to close at its next call.** A WASI
/// file is closed by being dropped, which cannot wait; the kernel's `close`
/// can — for `Rclunk` (`devmnt.c`) — so it is made at the next call the
/// program makes, or by `pexit` at its end.
struct Closer {
    pid: Pid,
    fds: Mutex<Vec<i32>>,
}

impl Closer {
    async fn flush(&self) -> Result<(), Error> {
        let fds = std::mem::take(&mut *self.fds.lock().unwrap());
        for fd in fds {
            let _ = call(self.pid, Call::Close { fd }).await;
        }
        Ok(())
    }
}

/// **A file the process has open** — a descriptor of its own, which every
/// operation on it is a call on.
pub struct File {
    pid: Pid,
    fd: i32,
    kind: FileType,
    flags: Mutex<FdFlags>,
    /// One of the process's standard streams, which are its own and not
    /// the program's to close.
    own: bool,
    closer: Arc<Closer>,
}

impl File {
    async fn open(pid: Pid, fd: i32, own: bool, closer: Arc<Closer>) -> File {
        let kind = match call(pid, Call::Fstat { fd }).await {
            Ok(Ret::Data(b)) => dir(&b).map_or(FileType::Unknown, |d| filetype(&d)),
            _ => FileType::Unknown,
        };
        File { pid, fd, kind, flags: Mutex::new(FdFlags::empty()), own, closer }
    }

    async fn stat(&self) -> Result<Dir, Error> {
        match call(self.pid, Call::Fstat { fd: self.fd }).await? {
            Ret::Data(b) => dir(&b),
            _ => Err(Errno::Io.into()),
        }
    }

    async fn wstat(&self, d: Dir) -> Result<(), Error> {
        call(self.pid, Call::Fwstat { fd: self.fd, edir: d.conv_d2m() }).await.map(|_| ())
    }

    async fn read(&self, bufs: &mut [IoSliceMut<'_>], off: i64) -> Result<u64, Error> {
        self.closer.flush().await?;
        let n = bufs.iter().map(|b| b.len()).sum::<usize>();
        let Ret::Data(d) = call(self.pid, Call::Pread { fd: self.fd, n, off }).await? else {
            return Err(Errno::Io.into());
        };
        let mut at = 0;
        for b in bufs.iter_mut() {
            let k = b.len().min(d.len() - at);
            b[..k].copy_from_slice(&d[at..at + k]);
            at += k;
            if at == d.len() {
                break;
            }
        }
        Ok(d.len() as u64)
    }

    async fn write(&self, bufs: &[IoSlice<'_>], off: i64) -> Result<u64, Error> {
        self.closer.flush().await?;
        let mut data = Vec::with_capacity(bufs.iter().map(|b| b.len()).sum());
        for b in bufs {
            data.extend_from_slice(b);
        }
        // `O_APPEND`, as APE writes: *"if(f->oflags&O_APPEND) _SEEK(fd, 0,
        // 2)"* before each write (`ape/lib/ap/plan9/write.c`)
        if off < 0 && self.flags.lock().unwrap().contains(FdFlags::APPEND) {
            call(self.pid, Call::Seek { fd: self.fd, off: 0, whence: 2 }).await?;
        }
        match call(self.pid, Call::Pwrite { fd: self.fd, data, off }).await? {
            Ret::N(n) => Ok(n as u64),
            _ => Err(Errno::Io.into()),
        }
    }
}

impl Drop for File {
    fn drop(&mut self) {
        if !self.own {
            self.closer.fds.lock().unwrap().push(self.fd);
        }
    }
}

#[async_trait::async_trait]
impl WasiFile for File {
    fn as_any(&self) -> &dyn Any {
        self
    }
    async fn get_filetype(&self) -> Result<FileType, Error> {
        Ok(self.kind)
    }
    fn isatty(&self) -> bool {
        self.kind == FileType::CharacterDevice
    }
    async fn get_fdflags(&self) -> Result<FdFlags, Error> {
        Ok(*self.flags.lock().unwrap())
    }
    async fn set_fdflags(&mut self, flags: FdFlags) -> Result<(), Error> {
        *self.flags.lock().unwrap() = flags;
        Ok(())
    }
    async fn get_filestat(&self) -> Result<Filestat, Error> {
        self.stat().await.map(|d| filestat(&d))
    }
    async fn set_filestat_size(&self, size: u64) -> Result<(), Error> {
        let mut d = nulldir();
        d.length = size;
        self.wstat(d).await
    }
    async fn advise(&self, _offset: u64, _len: u64, _advice: Advice) -> Result<(), Error> {
        Ok(())
    }
    async fn set_times(&self, atime: Option<SystemTimeSpec>, mtime: Option<SystemTimeSpec>) -> Result<(), Error> {
        let mut d = nulldir();
        if let Some(t) = secs(atime) {
            d.atime = t;
        }
        if let Some(t) = secs(mtime) {
            d.mtime = t;
        }
        self.wstat(d).await
    }
    async fn read_vectored<'a>(&self, bufs: &mut [IoSliceMut<'a>]) -> Result<u64, Error> {
        self.read(bufs, -1).await
    }
    async fn read_vectored_at<'a>(&self, bufs: &mut [IoSliceMut<'a>], offset: u64) -> Result<u64, Error> {
        self.read(bufs, offset as i64).await
    }
    async fn write_vectored<'a>(&self, bufs: &[IoSlice<'a>]) -> Result<u64, Error> {
        self.write(bufs, -1).await
    }
    async fn write_vectored_at<'a>(&self, bufs: &[IoSlice<'a>], offset: u64) -> Result<u64, Error> {
        self.write(bufs, offset as i64).await
    }
    async fn seek(&self, pos: SeekFrom) -> Result<u64, Error> {
        let (off, whence) = match pos {
            SeekFrom::Start(o) => (o as i64, 0),
            SeekFrom::Current(o) => (o, 1),
            SeekFrom::End(o) => (o, 2),
        };
        match call(self.pid, Call::Seek { fd: self.fd, off, whence }).await? {
            Ret::N(n) => Ok(n as u64),
            _ => Err(Errno::Io.into()),
        }
    }
    async fn readable(&self) -> Result<(), Error> {
        Ok(())
    }
    async fn writable(&self) -> Result<(), Error> {
        Ok(())
    }
}

/// **A directory of the namespace**, by its name: every operation on it is
/// a call on that name, joined to the one WASI gives.
pub struct Directory {
    pid: Pid,
    path: String,
    closer: Arc<Closer>,
}

impl Directory {
    fn join(&self, p: &str) -> String {
        if p.is_empty() || p == "." {
            return self.path.clone();
        }
        if self.path.ends_with('/') {
            format!("{}{p}", self.path)
        } else {
            format!("{}/{p}", self.path)
        }
    }

    async fn stat(&self, path: &str) -> Result<Dir, Error> {
        self.closer.flush().await?;
        match call(self.pid, Call::Stat { path: path.to_string() }).await? {
            Ret::Data(b) => dir(&b),
            _ => Err(Errno::Io.into()),
        }
    }

    async fn wstat(&self, path: &str, d: Dir) -> Result<(), Error> {
        call(self.pid, Call::Wstat { path: path.to_string(), edir: d.conv_d2m() }).await.map(|_| ())
    }

    async fn fd(&self, r: Result<Ret, Error>) -> Result<i32, Error> {
        match r? {
            Ret::Fd(fd) => Ok(fd),
            _ => Err(Errno::Io.into()),
        }
    }

    /// Everything a directory reads as, in its `Dir` entries.
    async fn entries(&self, path: &str) -> Result<Vec<Dir>, Error> {
        let fd = self.fd(call(self.pid, Call::Open { path: path.to_string(), mode: OREAD as i32 }).await).await?;
        let mut all = Vec::new();
        let r = loop {
            match call(self.pid, Call::Pread { fd, n: 8192, off: -1 }).await {
                Ok(Ret::Data(b)) if b.is_empty() => break Ok(()),
                Ok(Ret::Data(b)) => all.extend_from_slice(&b),
                Ok(_) => break Err(Errno::Io.into()),
                Err(e) => break Err(e),
            }
        };
        let _ = call(self.pid, Call::Close { fd }).await;
        r.map(|_| Dir::parse_all(&all))
    }
}

/// The name a path ends in, and the directory it is in.
fn split(p: &str) -> (&str, &str) {
    match p.rfind('/') {
        Some(0) => ("/", &p[1..]),
        Some(i) => (&p[..i], &p[i + 1..]),
        None => (".", p),
    }
}

#[async_trait::async_trait]
impl WasiDir for Directory {
    fn as_any(&self) -> &dyn Any {
        self
    }

    /// An open: `open` for a file there, and — with `O_CREAT` — `create`
    /// for one that is not, which is how APE's `open` makes one; `O_EXCL`
    /// is `create`'s `OEXCL`. A directory, asked for or found, is answered
    /// as one.
    async fn open_file(
        &self,
        _symlink_follow: bool,
        path: &str,
        oflags: OFlags,
        read: bool,
        write: bool,
        fdflags: FdFlags,
    ) -> Result<OpenResult, Error> {
        self.closer.flush().await?;
        let full = self.join(path);
        if oflags.contains(OFlags::DIRECTORY) {
            let d = self.stat(&full).await?;
            if d.qid.qtype & QTDIR == 0 {
                return Err(Errno::Notdir.into());
            }
            return Ok(OpenResult::Dir(Box::new(Directory { pid: self.pid, path: full, closer: self.closer.clone() })));
        }
        let mut mode = match (read, write) {
            (_, false) => OREAD,
            (false, true) => OWRITE,
            (true, true) => ORDWR,
        };
        if oflags.contains(OFlags::TRUNCATE) {
            mode |= OTRUNC;
        }
        let create = |mode: u16| Call::Create { path: full.clone(), mode: mode as i32, perm: 0o666 };
        let r = if oflags.contains(OFlags::CREATE) && oflags.contains(OFlags::EXCLUSIVE) {
            call(self.pid, create(mode | OEXCL)).await
        } else if oflags.contains(OFlags::CREATE) {
            match call(self.pid, Call::Open { path: full.clone(), mode: mode as i32 }).await {
                Err(e) if e.downcast_ref() == Some(&Errno::Noent) => call(self.pid, create(mode)).await,
                r => r,
            }
        } else {
            call(self.pid, Call::Open { path: full.clone(), mode: mode as i32 }).await
        };
        let fd = self.fd(r).await?;
        let f = File::open(self.pid, fd, false, self.closer.clone()).await;
        if f.kind == FileType::Directory {
            drop(f);
            return Ok(OpenResult::Dir(Box::new(Directory { pid: self.pid, path: full, closer: self.closer.clone() })));
        }
        *f.flags.lock().unwrap() = fdflags;
        Ok(OpenResult::File(Box::new(f)))
    }

    /// `create` with `DMDIR` (`sysfile.c`'s `syscreate` of a directory).
    async fn create_dir(&self, path: &str) -> Result<(), Error> {
        self.closer.flush().await?;
        let fd = self
            .fd(call(self.pid, Call::Create { path: self.join(path), mode: OREAD as i32, perm: DMDIR | 0o777 }).await)
            .await?;
        call(self.pid, Call::Close { fd }).await.map(|_| ())
    }

    /// The directory's entries, after `.` and `..`, as `wasi-common`'s own
    /// directories give them (`sync/dir.rs`).
    async fn readdir(
        &self,
        cursor: ReaddirCursor,
    ) -> Result<Box<dyn Iterator<Item = Result<ReaddirEntity, Error>> + Send>, Error> {
        self.closer.flush().await?;
        let me = self.stat(&self.path).await?;
        let mut all = vec![(".".to_string(), me.qid.path, FileType::Directory), ("..".to_string(), me.qid.path, FileType::Directory)];
        for d in self.entries(&self.path).await? {
            all.push((d.name.clone(), d.qid.path, filetype(&d)));
        }
        let from = u64::from(cursor) as usize;
        let out: Vec<Result<ReaddirEntity, Error>> = all
            .into_iter()
            .enumerate()
            .skip(from)
            .map(|(i, (name, inode, filetype))| Ok(ReaddirEntity { next: ReaddirCursor::from(i as u64 + 1), inode, name, filetype }))
            .collect();
        Ok(Box::new(out.into_iter()))
    }

    async fn remove_dir(&self, path: &str) -> Result<(), Error> {
        self.closer.flush().await?;
        call(self.pid, Call::Remove { path: self.join(path) }).await.map(|_| ())
    }

    async fn unlink_file(&self, path: &str) -> Result<(), Error> {
        self.closer.flush().await?;
        call(self.pid, Call::Remove { path: self.join(path) }).await.map(|_| ())
    }

    /// Plan 9 has no symbolic links, so nothing is one: `readlink(2)`'s
    /// answer for a file that is not, `EINVAL`.
    async fn read_link(&self, _path: &str) -> Result<std::path::PathBuf, Error> {
        Err(Errno::Inval.into())
    }

    async fn get_filestat(&self) -> Result<Filestat, Error> {
        self.stat(&self.path).await.map(|d| filestat(&d))
    }

    async fn get_path_filestat(&self, path: &str, _follow_symlinks: bool) -> Result<Filestat, Error> {
        self.stat(&self.join(path)).await.map(|d| filestat(&d))
    }

    /// **APE's `rename`** (`ape/lib/ap/plan9/rename.c`): what is at the new
    /// name is removed first; then, in the same directory, the name is
    /// changed by `wstat`; in another, the file is copied and the old one
    /// removed — Plan 9 renames only within a directory.
    async fn rename(&self, path: &str, dest_dir: &dyn WasiDir, dest_path: &str) -> Result<(), Error> {
        self.closer.flush().await?;
        let dest = dest_dir.as_any().downcast_ref::<Directory>().ok_or(Errno::Xdev)?;
        let (from, to) = (self.join(path), dest.join(dest_path));
        if self.stat(&to).await.is_ok() {
            call(self.pid, Call::Remove { path: to.clone() }).await?;
            if self.stat(&to).await.is_ok() {
                return Err(Errno::Exist.into());
            }
        }
        let d = self.stat(&from).await?;
        let ((fdir, _), (tdir, tname)) = (split(&from), split(&to));
        if fdir == tdir {
            let mut nd = nulldir();
            nd.name = tname.to_string();
            return self.wstat(&from, nd).await;
        }
        let ffd = self.fd(call(self.pid, Call::Open { path: from.clone(), mode: OREAD as i32 }).await).await?;
        let tfd = match self.fd(call(self.pid, Call::Create { path: to.clone(), mode: OWRITE as i32, perm: d.mode }).await).await {
            Ok(fd) => fd,
            Err(e) => {
                let _ = call(self.pid, Call::Close { fd: ffd }).await;
                return Err(e);
            }
        };
        let copied = loop {
            match call(self.pid, Call::Pread { fd: ffd, n: 8192, off: -1 }).await {
                Ok(Ret::Data(b)) if b.is_empty() => break Ok(()),
                Ok(Ret::Data(b)) => {
                    let n = b.len();
                    match call(self.pid, Call::Pwrite { fd: tfd, data: b, off: -1 }).await {
                        Ok(Ret::N(k)) if k == n => {}
                        Ok(_) => break Err(Errno::Io.into()),
                        Err(e) => break Err(e),
                    }
                }
                Ok(_) => break Err(Errno::Io.into()),
                Err(e) => break Err(e),
            }
        };
        let _ = call(self.pid, Call::Close { fd: ffd }).await;
        let _ = call(self.pid, Call::Close { fd: tfd }).await;
        copied?;
        call(self.pid, Call::Remove { path: from }).await.map(|_| ())
    }

    async fn set_times(
        &self,
        path: &str,
        atime: Option<SystemTimeSpec>,
        mtime: Option<SystemTimeSpec>,
        _follow_symlinks: bool,
    ) -> Result<(), Error> {
        self.closer.flush().await?;
        let mut d = nulldir();
        if let Some(t) = secs(atime) {
            d.atime = t;
        }
        if let Some(t) = secs(mtime) {
            d.mtime = t;
        }
        self.wstat(&self.join(path), d).await
    }
}

/// **WASI's waiting, as the kernel's `sleep`.** A file is ready at once: a
/// read or write that cannot be answered yet waits in the kernel, as every
/// read does, so there is nothing to wait for before it. A clock is slept
/// for until it is due.
struct Sleeper {
    pid: Pid,
}

#[async_trait::async_trait]
impl WasiSched for Sleeper {
    async fn poll_oneoff<'a>(&self, poll: &mut Poll<'a>) -> Result<(), Error> {
        let mut files = false;
        for s in poll.rw_subscriptions() {
            if let Subscription::Read(r) | Subscription::Write(r) = s {
                r.complete(1, RwEventFlags::empty());
                files = true;
            }
        }
        if files {
            return Ok(());
        }
        if let Some(t) = poll.earliest_clock_deadline() {
            while let Some(d) = t.duration_until() {
                let ms = (d.as_nanos().div_ceil(1_000_000) as u64).max(1);
                call(self.pid, Call::Sleep { ms }).await?;
            }
        }
        Ok(())
    }

    /// `sleep(0)`, which is `yield()` (`sysproc.c`'s `syssleep`).
    async fn sched_yield(&self) -> Result<(), Error> {
        call(self.pid, Call::Sleep { ms: 0 }).await.map(|_| ())
    }

    async fn sleep(&self, d: Duration) -> Result<(), Error> {
        let ms = d.as_nanos().div_ceil(1_000_000) as u64;
        call(self.pid, Call::Sleep { ms }).await.map(|_| ())
    }
}

/// **The process's environment, as APE makes one** (`_envsetup.c:30`):
/// each file of `#e` as a variable, its value's last 0 byte dropped and
/// any other made 1.
async fn environment(pid: Pid) -> Vec<(String, String)> {
    let d = Directory { pid, path: "#e".into(), closer: Arc::new(Closer { pid, fds: Mutex::new(Vec::new()) }) };
    let Ok(names) = d.entries("#e").await else { return Vec::new() };
    let mut out = Vec::new();
    for e in names {
        let path = format!("#e/{}", e.name);
        let Ok(Ret::Fd(fd)) = call(pid, Call::Open { path, mode: OREAD as i32 }).await else { continue };
        let mut v = match call(pid, Call::Pread { fd, n: e.length as usize, off: -1 }).await {
            Ok(Ret::Data(b)) if b.len() as u64 == e.length => b,
            _ => Vec::new(),
        };
        let _ = call(pid, Call::Close { fd }).await;
        if v.last() == Some(&0) {
            v.pop();
        }
        for b in v.iter_mut() {
            if *b == 0 {
                *b = 1;
            }
        }
        out.push((e.name, String::from_utf8_lossy(&v).into_owned()));
    }
    out
}

/// **A WASI program's context**: its arguments, the process's environment,
/// its descriptors 0, 1 and 2 as its standard streams, and the namespace
/// preopened as `/`.
pub async fn ctx(pid: Pid, args: &[String]) -> wasmtime::Result<WasiCtx> {
    let closer = Arc::new(Closer { pid, fds: Mutex::new(Vec::new()) });
    let mut ctx = WasiCtx::new(
        wasi_common::sync::random_ctx(),
        wasi_common::sync::clocks_ctx(),
        Box::new(Sleeper { pid }),
        Table::new(),
    );
    for a in args {
        ctx.push_arg(a)?;
    }
    for (k, v) in environment(pid).await {
        if !k.is_empty() && !k.contains('=') {
            ctx.push_env(&k, &v)?;
        }
    }
    ctx.set_stdin(Box::new(File::open(pid, 0, true, closer.clone()).await));
    ctx.set_stdout(Box::new(File::open(pid, 1, true, closer.clone()).await));
    ctx.set_stderr(Box::new(File::open(pid, 2, true, closer.clone()).await));
    ctx.push_preopened_dir(Box::new(Directory { pid, path: "/".into(), closer }), "/")?;
    Ok(ctx)
}

/// **A WASI program's end**, as the process's: its `_start` returning is
/// the image ending, which the kernel makes an exit with no status; and
/// `proc_exit(n)` is `exits` with APE's status for it — *"if(status){ cp =
/// _ultoa(exitstatus, status & 0xFF); …"* (`_exit.c:26`) — none for 0.
pub async fn ended(pid: Pid, r: wasmtime::Result<()>) -> wasmtime::Result<()> {
    let e = match r {
        Ok(()) => return Ok(()),
        Err(e) => e,
    };
    let Some(code) = e.downcast_ref::<wasi_common::I32Exit>().map(|x| x.0) else { return Err(e) };
    let status = if code == 0 { String::new() } else { (code & 0xFF).to_string() };
    let _ = call(pid, Call::Exits { status }).await;
    Err(wasmtime::Error::new(Exited))
}
