// wasi_snapshot_preview1 on wasmtime — the Rust port of supervisor/wasi1.mjs,
// the WASI second ABI as a shim over the same kernel traps. One preopen,
// fd 3 = "/": THE PREOPEN IS THE NAMESPACE ROOT. Paths resolve shim-side
// (WASI is dirfd-relative, no cwd) and walk kernel-side, so a foreign binary
// crosses symlinks, unions and mounts identically to a native one. The
// deviations travel with the JS shim: environ is empty, offsets are the
// mailbox's 32 bits, poll_oneoff fires every clock after one sleep, and
// fd_readdir snapshots the directory and serves dirents by INDEX cookie
// with the truncated-final-dirent rule (bufused < buflen means EOF).

use crate::{Ev, GuestExit};
use kernel::{KReply, Pid, TXSIZE};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::{channel, Sender};
use wasmtime::{Caller, Linker, Memory};

mod tn {
    pub const CLOSE: i32 = 4;
    pub const DUP: i32 = 5;
    pub const EXITS: i32 = 8;
    pub const OPEN: i32 = 14;
    pub const SLEEP: i32 = 17;
    pub const CREATE: i32 = 22;
    pub const REMOVE: i32 = 25;
    pub const SEEK: i32 = 39;
    pub const ERRSTR: i32 = 41;
    pub const STAT: i32 = 42;
    pub const FSTAT: i32 = 43;
    pub const PREAD: i32 = 50;
    pub const PWRITE: i32 = 51;
    pub const NSEC: i32 = 53;
    pub const LINK: i32 = 60;
    pub const SYMLINK: i32 = 61;
    pub const READLINK: i32 = 62;
    pub const ARGS: i32 = 200;
}

// wasi errno, the preview1 table
mod e {
    pub const SUCCESS: i32 = 0;
    pub const ACCES: i32 = 2;
    pub const BADF: i32 = 8;
    pub const EXIST: i32 = 20;
    pub const INVAL: i32 = 28;
    pub const IO: i32 = 29;
    pub const ISDIR: i32 = 31;
    pub const NOENT: i32 = 44;
    pub const NOSYS: i32 = 52;
    pub const NOTDIR: i32 = 54;
    pub const NOTEMPTY: i32 = 55;
    pub const NOTSUP: i32 = 58;
    pub const SPIPE: i32 = 70;
}

const OREAD: i32 = 0;
const OWRITE: i32 = 1;
const ORDWR: i32 = 2;
const OTRUNC: i32 = 16;
const DMDIR: u32 = 0x8000_0000;

#[derive(Clone)]
struct FdInfo {
    kfd: i32,
    path: String,
    isdir: bool,
    preopen: bool,
    append: bool,
    dirents: Option<Rc<Vec<(String, u8, u64)>>>, // (name, qtype) snapshot
}

pub struct WasiState {
    pub pid: Pid,
    pub ev: Sender<Ev>,
    pub memory: Option<Memory>,
    fds: RefCell<HashMap<i32, FdInfo>>,
    nextfd: RefCell<i32>,
    argv: RefCell<Option<Vec<String>>>,
}

impl WasiState {
    pub fn new(pid: Pid, ev: Sender<Ev>) -> WasiState {
        let mut fds = HashMap::new();
        for i in 0..3 {
            fds.insert(i, FdInfo {
                kfd: i, path: "/dev/cons".into(), isdir: false,
                preopen: false, append: false, dirents: None,
            });
        }
        fds.insert(3, FdInfo {
            kfd: -1, path: "/".into(), isdir: true,
            preopen: true, append: false, dirents: None,
        });
        WasiState {
            pid, ev, memory: None,
            fds: RefCell::new(fds),
            nextfd: RefCell::new(4),
            argv: RefCell::new(None),
        }
    }
}

// the syscall: strings already marshalled into tx; the reply's data is
// returned raw (the tx-readback of the JS sysTx)
fn ksys(caller: &mut Caller<'_, WasiState>, trap: i32, tx: Vec<u8>, a: [i32; 5])
        -> Result<(i32, Vec<u8>), wasmtime::Error> {
    let (ev, pid) = {
        let st = caller.data();
        (st.ev.clone(), st.pid)
    };
    let (rtx, rrx) = channel::<KReply>();
    ev.send(Ev::Sys { worker_pid: pid, trap, a, tx, reply: rtx })
        .map_err(|_| wasmtime::Error::msg("kernel gone"))?;
    let r = rrx.recv().map_err(|_| wasmtime::Error::msg("kernel gone"))?;
    match r.action {
        kernel::KAction::Retire | kernel::KAction::Die => {
            return Err(GuestExit::Retire.into());
        }
        _ => {}
    }
    Ok((r.ret, r.data))
}

fn kerrno(caller: &mut Caller<'_, WasiState>) -> i32 {
    let msg = match ksys(caller, tn::ERRSTR, vec![0], [0, 128, 0, 0, 0]) {
        Ok((_, data)) => String::from_utf8_lossy(&data).into_owned(),
        Err(_) => return e::IO,
    };
    let m = msg.as_str();
    if m.contains("does not exist") || m.contains("not found") {
        e::NOENT
    } else if m.contains("exists") || m.contains("in use") {
        e::EXIST
    } else if m.contains("permission") || m.contains("denied") {
        e::ACCES
    } else if m.contains("is a directory") {
        e::ISDIR
    } else if m.contains("not a directory") {
        e::NOTDIR
    } else if m.contains("not empty") {
        e::NOTEMPTY
    } else if m.contains("not open") || m.contains("fd ") {
        e::BADF
    } else if m.contains("not a symlink") || m.contains("readlink not supported") {
        e::INVAL // POSIX callers probe with readlink
    } else {
        e::IO
    }
}

fn strs(list: &[&str]) -> Vec<u8> {
    let mut tx = Vec::new();
    for s in list {
        tx.extend_from_slice(s.as_bytes());
        tx.push(0);
    }
    tx
}

fn mem(caller: &Caller<'_, WasiState>) -> Memory {
    caller.data().memory.expect("wasi memory")
}

fn read_guest(caller: &Caller<'_, WasiState>, ptr: u32, len: u32) -> Vec<u8> {
    let m = mem(caller);
    m.data(caller)[ptr as usize..(ptr + len) as usize].to_vec()
}

fn write_guest(caller: &mut Caller<'_, WasiState>, ptr: u32, data: &[u8]) {
    let m = mem(caller);
    m.write(&mut *caller, ptr as usize, data).ok();
}

fn u32g(caller: &Caller<'_, WasiState>, ptr: u32) -> u32 {
    let m = mem(caller);
    u32::from_le_bytes(m.data(caller)[ptr as usize..ptr as usize + 4].try_into().unwrap())
}

fn resolve(caller: &Caller<'_, WasiState>, dirfd: i32, ptr: u32, len: u32) -> Option<String> {
    let base = caller.data().fds.borrow().get(&dirfd)?.path.clone();
    let mut p = String::from_utf8_lossy(&read_guest(caller, ptr, len)).into_owned();
    if !p.starts_with('/') {
        p = if base == "/" { format!("/{}", p) } else { format!("{}/{}", base, p) };
    }
    let mut out: Vec<&str> = Vec::new();
    for c in p.split('/') {
        match c {
            "" | "." => {}
            ".." => { out.pop(); }
            c => out.push(c),
        }
    }
    Some(format!("/{}", out.join("/")))
}

// one 9P stat record: (qtype, qid.path, atime, mtime, length, name)
fn parse9(b: &[u8], off: usize) -> (usize, u8, u64, u32, u32, u64, String) {
    let size = u16::from_le_bytes(b[off..off + 2].try_into().unwrap()) as usize + 2;
    let qtype = b[off + 8];
    let qpath = u64::from_le_bytes(b[off + 13..off + 21].try_into().unwrap());
    let atime = u32::from_le_bytes(b[off + 25..off + 29].try_into().unwrap());
    let mtime = u32::from_le_bytes(b[off + 29..off + 33].try_into().unwrap());
    let length = u64::from_le_bytes(b[off + 33..off + 41].try_into().unwrap());
    let nlen = u16::from_le_bytes(b[off + 41..off + 43].try_into().unwrap()) as usize;
    let name = String::from_utf8_lossy(&b[off + 43..off + 43 + nlen]).into_owned();
    (size, qtype, qpath, atime, mtime, length, name)
}

// clang's FileManager deduplicates by (dev,ino): every file must carry a
// DISTINCT inode (RESEARCH §9.7 — ino=0 made hello.c cache as stdio.h).
// The qid.path is the kernel's own identity; a server that reports none
// gets an FNV-1a of the path instead.
fn ino_for(qpath: u64, path: &str) -> u64 {
    if qpath != 0 {
        return qpath;
    }
    let mut h: u64 = 0xcbf29ce484222325;
    for b in path.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h | 1
}

fn ftype(qtype: u8, path: &str) -> u8 {
    if qtype & 0x80 != 0 {
        3 // dir
    } else if qtype & 0x02 != 0 {
        7 // symlink
    } else if path.starts_with("/dev/") {
        2 // chardev
    } else {
        4 // regular
    }
}

fn put_filestat(caller: &mut Caller<'_, WasiState>, ptr: u32, qtype: u8, qpath: u64,
                atime: u32, mtime: u32, length: u64, path: &str) {
    let mut b = [0u8; 64];
    b[8..16].copy_from_slice(&ino_for(qpath, path).to_le_bytes());
    b[16] = ftype(qtype, path);
    b[24..32].copy_from_slice(&1u64.to_le_bytes());
    b[32..40].copy_from_slice(&length.to_le_bytes());
    b[40..48].copy_from_slice(&((atime as u64) * 1_000_000_000).to_le_bytes());
    b[48..56].copy_from_slice(&((mtime as u64) * 1_000_000_000).to_le_bytes());
    b[56..64].copy_from_slice(&((mtime as u64) * 1_000_000_000).to_le_bytes());
    write_guest(caller, ptr, &b);
}

fn get_args(caller: &mut Caller<'_, WasiState>) -> Result<Vec<String>, wasmtime::Error> {
    if let Some(a) = caller.data().argv.borrow().clone() {
        return Ok(a);
    }
    let (n, data) = ksys(caller, tn::ARGS, Vec::new(), [0, TXSIZE as i32, 0, 0, 0])?;
    let raw = String::from_utf8_lossy(&data[..n.max(0) as usize]).into_owned();
    let argv: Vec<String> = raw.split('\0').filter(|s| !s.is_empty()).map(|s| s.into()).collect();
    *caller.data_mut().argv.borrow_mut() = Some(argv.clone());
    Ok(argv)
}

fn alloc_fd(caller: &Caller<'_, WasiState>, info: FdInfo) -> i32 {
    let st = caller.data();
    let fd = *st.nextfd.borrow();
    *st.nextfd.borrow_mut() += 1;
    st.fds.borrow_mut().insert(fd, info);
    fd
}

pub fn link_wasi(linker: &mut Linker<WasiState>) -> Result<(), wasmtime::Error> {
    let m = "wasi_snapshot_preview1";

    linker.func_wrap(m, "proc_exit", |mut caller: Caller<'_, WasiState>, code: i32|
        -> Result<(), wasmtime::Error> {
        let msg = if code == 0 { String::new() } else { format!("exit {}", code) };
        let _ = ksys(&mut caller, tn::EXITS, strs(&[&msg]), [0; 5]);
        Err(GuestExit::Retire.into())
    })?;
    linker.func_wrap(m, "proc_raise", |_: Caller<'_, WasiState>, _: i32| e::NOSYS)?;
    linker.func_wrap(m, "sched_yield", |_: Caller<'_, WasiState>| e::SUCCESS)?;

    linker.func_wrap(m, "random_get", |mut caller: Caller<'_, WasiState>, ptr: i32, len: i32| {
        let mut buf = vec![0u8; len as usize];
        // std's thread_rng shape without the dependency: hash the clock
        let mut x = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1)
            | 1;
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = x as u8;
        }
        write_guest(&mut caller, ptr as u32, &buf);
        e::SUCCESS
    })?;

    linker.func_wrap(m, "args_sizes_get",
        |mut caller: Caller<'_, WasiState>, argc_p: i32, size_p: i32| -> Result<i32, wasmtime::Error> {
        let a = get_args(&mut caller)?;
        let total: usize = a.iter().map(|s| s.len() + 1).sum();
        write_guest(&mut caller, argc_p as u32, &(a.len() as u32).to_le_bytes());
        write_guest(&mut caller, size_p as u32, &(total as u32).to_le_bytes());
        Ok(e::SUCCESS)
    })?;
    linker.func_wrap(m, "args_get",
        |mut caller: Caller<'_, WasiState>, argv_p: i32, buf_p: i32| -> Result<i32, wasmtime::Error> {
        let a = get_args(&mut caller)?;
        let mut o = buf_p as u32;
        for (i, s) in a.iter().enumerate() {
            write_guest(&mut caller, argv_p as u32 + 4 * i as u32, &o.to_le_bytes());
            write_guest(&mut caller, o, s.as_bytes());
            write_guest(&mut caller, o + s.len() as u32, &[0]);
            o += s.len() as u32 + 1;
        }
        Ok(e::SUCCESS)
    })?;
    linker.func_wrap(m, "environ_sizes_get",
        |mut caller: Caller<'_, WasiState>, count_p: i32, size_p: i32| {
        write_guest(&mut caller, count_p as u32, &0u32.to_le_bytes());
        write_guest(&mut caller, size_p as u32, &0u32.to_le_bytes());
        e::SUCCESS
    })?;
    linker.func_wrap(m, "environ_get", |_: Caller<'_, WasiState>, _: i32, _: i32| e::SUCCESS)?;

    linker.func_wrap(m, "clock_time_get",
        |mut caller: Caller<'_, WasiState>, _id: i32, _prec: i64, out_p: i32|
        -> Result<i32, wasmtime::Error> {
        let (_, data) = ksys(&mut caller, tn::NSEC, Vec::new(), [0; 5])?;
        write_guest(&mut caller, out_p as u32, &data[..8.min(data.len())]);
        Ok(e::SUCCESS)
    })?;
    linker.func_wrap(m, "clock_res_get",
        |mut caller: Caller<'_, WasiState>, _id: i32, out_p: i32| {
        write_guest(&mut caller, out_p as u32, &1_000_000u64.to_le_bytes());
        e::SUCCESS
    })?;

    linker.func_wrap(m, "fd_prestat_get",
        |mut caller: Caller<'_, WasiState>, fd: i32, out_p: i32| {
        let pre = caller.data().fds.borrow().get(&fd).map(|f| (f.preopen, f.path.len()));
        match pre {
            Some((true, plen)) => {
                write_guest(&mut caller, out_p as u32, &[0, 0, 0, 0]);
                write_guest(&mut caller, out_p as u32 + 4, &(plen as u32).to_le_bytes());
                e::SUCCESS
            }
            _ => e::BADF,
        }
    })?;
    linker.func_wrap(m, "fd_prestat_dir_name",
        |mut caller: Caller<'_, WasiState>, fd: i32, ptr: i32, len: i32| {
        let path = caller.data().fds.borrow().get(&fd)
            .filter(|f| f.preopen).map(|f| f.path.clone());
        match path {
            Some(p) => {
                let b = p.as_bytes();
                write_guest(&mut caller, ptr as u32, &b[..b.len().min(len as usize)]);
                e::SUCCESS
            }
            None => e::BADF,
        }
    })?;

    linker.func_wrap(m, "fd_close",
        |mut caller: Caller<'_, WasiState>, fd: i32| -> Result<i32, wasmtime::Error> {
        let info = caller.data().fds.borrow_mut().remove(&fd);
        match info {
            Some(f) => {
                if !f.preopen && f.kfd >= 0 {
                    ksys(&mut caller, tn::CLOSE, Vec::new(), [f.kfd, 0, 0, 0, 0])?;
                }
                Ok(e::SUCCESS)
            }
            None => Ok(e::BADF),
        }
    })?;
    linker.func_wrap(m, "fd_fdstat_get",
        |mut caller: Caller<'_, WasiState>, fd: i32, out_p: i32| {
        let info = caller.data().fds.borrow().get(&fd).cloned();
        match info {
            Some(f) => {
                let mut b = [0u8; 24];
                b[0] = if f.isdir { 3 } else if fd <= 2 { 2 } else { 4 };
                b[2] = if f.append { 1 } else { 0 };
                b[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
                b[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
                write_guest(&mut caller, out_p as u32, &b);
                e::SUCCESS
            }
            None => e::BADF,
        }
    })?;
    linker.func_wrap(m, "fd_fdstat_set_flags",
        |caller: Caller<'_, WasiState>, fd: i32, flags: i32| {
        match caller.data().fds.borrow_mut().get_mut(&fd) {
            Some(f) => {
                f.append = flags & 1 != 0;
                e::SUCCESS
            }
            None => e::BADF,
        }
    })?;
    linker.func_wrap(m, "fd_filestat_get",
        |mut caller: Caller<'_, WasiState>, fd: i32, out_p: i32| -> Result<i32, wasmtime::Error> {
        let info = caller.data().fds.borrow().get(&fd).cloned();
        let Some(f) = info else { return Ok(e::BADF) };
        let (n, data) = if f.preopen {
            ksys(&mut caller, tn::STAT, strs(&[&f.path]), [0, -1, 4096, 0, 0])?
        } else {
            ksys(&mut caller, tn::FSTAT, Vec::new(), [f.kfd, -1, 4096, 0, 0])?
        };
        if n < 0 {
            return Ok(kerrno(&mut caller));
        }
        let (_, qtype, qpath, atime, mtime, length, _) = parse9(&data, 0);
        put_filestat(&mut caller, out_p as u32, qtype, qpath, atime, mtime, length, &f.path);
        Ok(e::SUCCESS)
    })?;
    linker.func_wrap(m, "fd_filestat_set_size",
        |_: Caller<'_, WasiState>, _: i32, _: i64| e::NOTSUP)?;
    linker.func_wrap(m, "fd_filestat_set_times",
        |_: Caller<'_, WasiState>, _: i32, _: i64, _: i64, _: i32| e::SUCCESS)?;

    linker.func_wrap(m, "fd_seek",
        |mut caller: Caller<'_, WasiState>, fd: i32, off: i64, whence: i32, out_p: i32|
        -> Result<i32, wasmtime::Error> {
        let info = caller.data().fds.borrow().get(&fd).cloned();
        let Some(f) = info else { return Ok(e::BADF) };
        if f.kfd <= 2 {
            return Ok(e::SPIPE);
        }
        let (r, _) = ksys(&mut caller, tn::SEEK, Vec::new(),
                          [f.kfd, off as u32 as i32, (off >> 32) as i32, whence, 0])?;
        if r < 0 {
            return Ok(kerrno(&mut caller));
        }
        write_guest(&mut caller, out_p as u32, &(r as u32 as u64).to_le_bytes());
        Ok(e::SUCCESS)
    })?;
    linker.func_wrap(m, "fd_tell",
        |mut caller: Caller<'_, WasiState>, fd: i32, out_p: i32| -> Result<i32, wasmtime::Error> {
        let info = caller.data().fds.borrow().get(&fd).cloned();
        let Some(f) = info else { return Ok(e::BADF) };
        let (r, _) = ksys(&mut caller, tn::SEEK, Vec::new(), [f.kfd, 0, 0, 1, 0])?;
        write_guest(&mut caller, out_p as u32, &(r.max(0) as u64).to_le_bytes());
        Ok(e::SUCCESS)
    })?;
    linker.func_wrap(m, "fd_renumber",
        |caller: Caller<'_, WasiState>, from: i32, to: i32| {
        let mut fds = caller.data().fds.borrow_mut();
        match fds.remove(&from) {
            Some(f) => {
                fds.insert(to, f);
                e::SUCCESS
            }
            None => e::BADF,
        }
    })?;
    linker.func_wrap(m, "fd_sync", |_: Caller<'_, WasiState>, _: i32| e::SUCCESS)?;
    linker.func_wrap(m, "fd_datasync", |_: Caller<'_, WasiState>, _: i32| e::SUCCESS)?;
    linker.func_wrap(m, "fd_advise",
        |_: Caller<'_, WasiState>, _: i32, _: i64, _: i64, _: i32| e::SUCCESS)?;
    linker.func_wrap(m, "fd_allocate",
        |_: Caller<'_, WasiState>, _: i32, _: i64, _: i64| e::NOTSUP)?;

    linker.func_wrap(m, "fd_read",
        |mut caller: Caller<'_, WasiState>, fd: i32, iovs: i32, niovs: i32, out_p: i32|
        -> Result<i32, wasmtime::Error> {
        rw_iovs(&mut caller, fd, iovs, niovs, out_p, None, false)
    })?;
    linker.func_wrap(m, "fd_write",
        |mut caller: Caller<'_, WasiState>, fd: i32, iovs: i32, niovs: i32, out_p: i32|
        -> Result<i32, wasmtime::Error> {
        rw_iovs(&mut caller, fd, iovs, niovs, out_p, None, true)
    })?;
    linker.func_wrap(m, "fd_pread",
        |mut caller: Caller<'_, WasiState>, fd: i32, iovs: i32, niovs: i32, off: i64, out_p: i32|
        -> Result<i32, wasmtime::Error> {
        rw_iovs(&mut caller, fd, iovs, niovs, out_p, Some(off as u64), false)
    })?;
    linker.func_wrap(m, "fd_pwrite",
        |mut caller: Caller<'_, WasiState>, fd: i32, iovs: i32, niovs: i32, off: i64, out_p: i32|
        -> Result<i32, wasmtime::Error> {
        rw_iovs(&mut caller, fd, iovs, niovs, out_p, Some(off as u64), true)
    })?;

    linker.func_wrap(m, "fd_readdir",
        |mut caller: Caller<'_, WasiState>, fd: i32, buf_p: i32, buflen: i32, cookie: i64, out_p: i32|
        -> Result<i32, wasmtime::Error> {
        let info = caller.data().fds.borrow().get(&fd).cloned();
        let Some(mut f) = info else { return Ok(e::NOTDIR) };
        if !f.isdir {
            return Ok(e::NOTDIR);
        }
        if f.kfd < 0 {
            // the preopen opens lazily
            let (kfd, _) = ksys(&mut caller, tn::OPEN, strs(&[&f.path]), [0, OREAD, 0, 0, 0])?;
            if kfd < 0 {
                return Ok(kerrno(&mut caller));
            }
            f.kfd = kfd;
            caller.data().fds.borrow_mut().get_mut(&fd).unwrap().kfd = kfd;
        }
        let mut idx = cookie as usize;
        if idx == 0 || f.dirents.is_none() {
            let mut entries: Vec<(String, u8, u64)> = Vec::new();
            let mut pos: u64 = 0;
            loop {
                let (n, data) = ksys(&mut caller, tn::PREAD, Vec::new(),
                    [f.kfd, -1, TXSIZE as i32,
                     pos as u32 as i32, (pos >> 32) as i32])?;
                if n < 0 {
                    return Ok(kerrno(&mut caller));
                }
                if n == 0 {
                    break;
                }
                let mut off = 0usize;
                while off + 2 <= data.len() {
                    let (size, qtype, qpath, _, _, _, name) = parse9(&data, off);
                    entries.push((name, qtype, qpath));
                    off += size;
                }
                pos += n as u64;
            }
            let rc = Rc::new(entries);
            f.dirents = Some(rc.clone());
            caller.data().fds.borrow_mut().get_mut(&fd).unwrap().dirents = Some(rc);
        }
        let entries = f.dirents.clone().unwrap();
        let mut used: usize = 0;
        let buflen = buflen as usize;
        while idx < entries.len() {
            let (name, qtype, qpath) = &entries[idx];
            let nb = name.as_bytes();
            let need = 24 + nb.len();
            let mut rec = vec![0u8; need];
            rec[0..8].copy_from_slice(&((idx as u64) + 1).to_le_bytes());
            rec[8..16].copy_from_slice(&ino_for(*qpath, name).to_le_bytes());
            rec[16..20].copy_from_slice(&(nb.len() as u32).to_le_bytes());
            rec[20] = ftype(*qtype, "/");
            rec[24..].copy_from_slice(nb);
            // preview1's contract: bufused < buflen means END OF DIRECTORY,
            // so a dirent that doesn't fit ships TRUNCATED to fill exactly
            let take = need.min(buflen - used);
            write_guest(&mut caller, buf_p as u32 + used as u32, &rec[..take]);
            used += take;
            if take < need {
                break;
            }
            idx += 1;
        }
        write_guest(&mut caller, out_p as u32, &(used as u32).to_le_bytes());
        Ok(e::SUCCESS)
    })?;

    linker.func_wrap(m, "path_open",
        |mut caller: Caller<'_, WasiState>, dirfd: i32, _dirflags: i32, ptr: i32, len: i32,
         oflags: i32, rights: i64, _rights_inh: i64, fdflags: i32, out_p: i32|
        -> Result<i32, wasmtime::Error> {
        let Some(path) = resolve(&caller, dirfd, ptr as u32, len as u32) else {
            return Ok(e::BADF);
        };
        let want_w = rights as u64 & (1 << 6) != 0;
        let want_r = rights as u64 & (1 << 1) != 0 || !want_w;
        let mut mode = if want_w && want_r { ORDWR } else if want_w { OWRITE } else { OREAD };
        if oflags & 8 != 0 {
            mode |= OTRUNC;
        }
        let kfd = if oflags & 1 != 0 {
            // O_CREAT; O_EXCL probes first (create(2) truncates existing)
            if oflags & 4 != 0 {
                let (sn, _) = ksys(&mut caller, tn::STAT, strs(&[&path]), [0, -1, 4096, 0, 0])?;
                if sn >= 0 {
                    return Ok(e::EXIST);
                }
            }
            let (kfd, _) = ksys(&mut caller, tn::CREATE, strs(&[&path]), [0, mode, 0o666, 0, 0])?;
            kfd
        } else {
            let (kfd, _) = ksys(&mut caller, tn::OPEN, strs(&[&path]), [0, mode, 0, 0, 0])?;
            kfd
        };
        if kfd < 0 {
            return Ok(kerrno(&mut caller));
        }
        let mut isdir = false;
        let (sn, data) = ksys(&mut caller, tn::FSTAT, Vec::new(), [kfd, -1, 4096, 0, 0])?;
        if sn > 0 {
            isdir = parse9(&data, 0).1 & 0x80 != 0;
        }
        if oflags & 2 != 0 && !isdir {
            // O_DIRECTORY
            ksys(&mut caller, tn::CLOSE, Vec::new(), [kfd, 0, 0, 0, 0])?;
            return Ok(e::NOTDIR);
        }
        let fd = alloc_fd(&caller, FdInfo {
            kfd, path, isdir, preopen: false, append: fdflags & 1 != 0, dirents: None,
        });
        write_guest(&mut caller, out_p as u32, &(fd as u32).to_le_bytes());
        Ok(e::SUCCESS)
    })?;

    linker.func_wrap(m, "path_filestat_get",
        |mut caller: Caller<'_, WasiState>, dirfd: i32, _flags: i32, ptr: i32, len: i32, out_p: i32|
        -> Result<i32, wasmtime::Error> {
        let Some(path) = resolve(&caller, dirfd, ptr as u32, len as u32) else {
            return Ok(e::BADF);
        };
        let (n, data) = ksys(&mut caller, tn::STAT, strs(&[&path]), [0, -1, 4096, 0, 0])?;
        if n < 0 {
            return Ok(kerrno(&mut caller));
        }
        let (_, qtype, qpath, atime, mtime, length, _) = parse9(&data, 0);
        put_filestat(&mut caller, out_p as u32, qtype, qpath, atime, mtime, length, &path);
        Ok(e::SUCCESS)
    })?;
    linker.func_wrap(m, "path_filestat_set_times",
        |_: Caller<'_, WasiState>, _: i32, _: i32, _: i32, _: i32, _: i64, _: i64, _: i32| e::NOTSUP)?;

    linker.func_wrap(m, "path_create_directory",
        |mut caller: Caller<'_, WasiState>, dirfd: i32, ptr: i32, len: i32|
        -> Result<i32, wasmtime::Error> {
        let Some(path) = resolve(&caller, dirfd, ptr as u32, len as u32) else {
            return Ok(e::BADF);
        };
        let (kfd, _) = ksys(&mut caller, tn::CREATE, strs(&[&path]),
                            [0, OREAD, (DMDIR | 0o777) as i32, 0, 0])?;
        if kfd < 0 {
            return Ok(kerrno(&mut caller));
        }
        ksys(&mut caller, tn::CLOSE, Vec::new(), [kfd, 0, 0, 0, 0])?;
        Ok(e::SUCCESS)
    })?;
    linker.func_wrap(m, "path_unlink_file",
        |mut caller: Caller<'_, WasiState>, dirfd: i32, ptr: i32, len: i32|
        -> Result<i32, wasmtime::Error> {
        path_remove(&mut caller, dirfd, ptr, len)
    })?;
    linker.func_wrap(m, "path_remove_directory",
        |mut caller: Caller<'_, WasiState>, dirfd: i32, ptr: i32, len: i32|
        -> Result<i32, wasmtime::Error> {
        path_remove(&mut caller, dirfd, ptr, len)
    })?;
    linker.func_wrap(m, "path_rename",
        |mut caller: Caller<'_, WasiState>, dirfd: i32, ptr: i32, len: i32,
         ndirfd: i32, nptr: i32, nlen: i32| -> Result<i32, wasmtime::Error> {
        let (Some(from), Some(to)) = (resolve(&caller, dirfd, ptr as u32, len as u32),
                                      resolve(&caller, ndirfd, nptr as u32, nlen as u32)) else {
            return Ok(e::BADF);
        };
        // V10's rule, alive here: rename is link + unlink in userland
        let (r, _) = ksys(&mut caller, tn::LINK, strs(&[&from, &to]), [0; 5])?;
        if r < 0 {
            return Ok(kerrno(&mut caller));
        }
        let (r, _) = ksys(&mut caller, tn::REMOVE, strs(&[&from]), [0; 5])?;
        if r < 0 {
            return Ok(kerrno(&mut caller));
        }
        Ok(e::SUCCESS)
    })?;
    linker.func_wrap(m, "path_link",
        |mut caller: Caller<'_, WasiState>, odirfd: i32, _oflags: i32, optr: i32, olen: i32,
         ndirfd: i32, nptr: i32, nlen: i32| -> Result<i32, wasmtime::Error> {
        let (Some(from), Some(to)) = (resolve(&caller, odirfd, optr as u32, olen as u32),
                                      resolve(&caller, ndirfd, nptr as u32, nlen as u32)) else {
            return Ok(e::BADF);
        };
        let (r, _) = ksys(&mut caller, tn::LINK, strs(&[&from, &to]), [0; 5])?;
        Ok(if r < 0 { kerrno(&mut caller) } else { e::SUCCESS })
    })?;
    linker.func_wrap(m, "path_symlink",
        |mut caller: Caller<'_, WasiState>, optr: i32, olen: i32, dirfd: i32, nptr: i32, nlen: i32|
        -> Result<i32, wasmtime::Error> {
        let target = String::from_utf8_lossy(&read_guest(&caller, optr as u32, olen as u32)).into_owned();
        let Some(nu) = resolve(&caller, dirfd, nptr as u32, nlen as u32) else {
            return Ok(e::BADF);
        };
        let (r, _) = ksys(&mut caller, tn::SYMLINK, strs(&[&target, &nu]), [0; 5])?;
        Ok(if r < 0 { kerrno(&mut caller) } else { e::SUCCESS })
    })?;
    linker.func_wrap(m, "path_readlink",
        |mut caller: Caller<'_, WasiState>, dirfd: i32, ptr: i32, len: i32,
         buf_p: i32, buflen: i32, out_p: i32| -> Result<i32, wasmtime::Error> {
        let Some(path) = resolve(&caller, dirfd, ptr as u32, len as u32) else {
            return Ok(e::BADF);
        };
        let (n, data) = ksys(&mut caller, tn::READLINK, strs(&[&path]),
                             [0, -1, buflen.min(4096), 0, 0])?;
        if n < 0 {
            return Ok(kerrno(&mut caller));
        }
        let take = (n as usize).min(buflen as usize).min(data.len());
        write_guest(&mut caller, buf_p as u32, &data[..take]);
        write_guest(&mut caller, out_p as u32, &(take as u32).to_le_bytes());
        Ok(e::SUCCESS)
    })?;

    linker.func_wrap(m, "poll_oneoff",
        |mut caller: Caller<'_, WasiState>, in_p: i32, out_p: i32, nsub: i32, out_n: i32|
        -> Result<i32, wasmtime::Error> {
        let mut min_ms: i64 = -1;
        let mut nfd = 0;
        let mut subs: Vec<(u64, u8)> = Vec::new();
        for i in 0..nsub {
            let p = in_p as u32 + 48 * i as u32;
            let raw = read_guest(&caller, p, 48);
            let userdata = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let tag = raw[8];
            if tag == 0 {
                let mut ns = u64::from_le_bytes(raw[24..32].try_into().unwrap());
                let flags = u16::from_le_bytes(raw[40..42].try_into().unwrap());
                if flags & 1 != 0 {
                    // abstime: subtract now
                    let (_, data) = ksys(&mut caller, tn::NSEC, Vec::new(), [0; 5])?;
                    let now = u64::from_le_bytes(data[0..8].try_into().unwrap());
                    ns = ns.saturating_sub(now);
                }
                let ms = (ns / 1_000_000) as i64;
                if min_ms < 0 || ms < min_ms {
                    min_ms = ms;
                }
                subs.push((userdata, 0));
            } else {
                nfd += 1;
                subs.push((userdata, tag));
            }
        }
        if nfd == 0 && min_ms > 0 {
            ksys(&mut caller, tn::SLEEP, Vec::new(), [min_ms as i32, 0, 0, 0, 0])?;
        }
        let mut o = out_p as u32;
        let mut count = 0u32;
        for (userdata, ty) in subs {
            if ty != 0 && nfd == 0 {
                continue;
            }
            let mut evb = [0u8; 32];
            evb[0..8].copy_from_slice(&userdata.to_le_bytes());
            evb[10] = ty;
            let nbytes: u64 = if ty == 0 { 0 } else { 1 };
            evb[16..24].copy_from_slice(&nbytes.to_le_bytes());
            write_guest(&mut caller, o, &evb);
            o += 32;
            count += 1;
        }
        write_guest(&mut caller, out_n as u32, &count.to_le_bytes());
        Ok(e::SUCCESS)
    })?;

    linker.func_wrap(m, "sock_accept",
        |_: Caller<'_, WasiState>, _: i32, _: i32, _: i32| e::NOSYS)?;
    linker.func_wrap(m, "sock_recv",
        |_: Caller<'_, WasiState>, _: i32, _: i32, _: i32, _: i32, _: i32, _: i32| e::NOSYS)?;
    linker.func_wrap(m, "sock_send",
        |_: Caller<'_, WasiState>, _: i32, _: i32, _: i32, _: i32, _: i32| e::NOSYS)?;
    linker.func_wrap(m, "sock_shutdown",
        |_: Caller<'_, WasiState>, _: i32, _: i32| e::NOSYS)?;

    let _ = tn::DUP;
    Ok(())
}

fn path_remove(caller: &mut Caller<'_, WasiState>, dirfd: i32, ptr: i32, len: i32)
               -> Result<i32, wasmtime::Error> {
    let Some(path) = resolve(caller, dirfd, ptr as u32, len as u32) else {
        return Ok(e::BADF);
    };
    let (r, _) = ksys(caller, tn::REMOVE, strs(&[&path]), [0; 5])?;
    Ok(if r < 0 { kerrno(caller) } else { e::SUCCESS })
}

fn rw_iovs(caller: &mut Caller<'_, WasiState>, fd: i32, iovs: i32, niovs: i32, out_p: i32,
           off: Option<u64>, write: bool) -> Result<i32, wasmtime::Error> {
    let info = caller.data().fds.borrow().get(&fd).cloned();
    let Some(f) = info else { return Ok(e::BADF) };
    if f.kfd < 0 {
        return Ok(e::BADF);
    }
    if write && f.append && off.is_none() {
        ksys(caller, tn::SEEK, Vec::new(), [f.kfd, 0, 0, 2, 0])?;
    }
    let mut total = 0usize;
    let mut pos = off;
    for i in 0..niovs {
        let ptr = u32g(caller, iovs as u32 + 8 * i as u32);
        let len = u32g(caller, iovs as u32 + 8 * i as u32 + 4) as usize;
        let mut done = 0usize;
        while done < len {
            let take = (len - done).min(TXSIZE);
            let (lo, hi) = match pos {
                Some(p) => (p as u32 as i32, (p >> 32) as i32),
                None => (-1, -1),
            };
            let n = if write {
                let data = read_guest(caller, ptr + done as u32, take as u32);
                let (n, _) = ksys(caller, tn::PWRITE, data, [f.kfd, 0, take as i32, lo, hi])?;
                n
            } else {
                let (n, data) = ksys(caller, tn::PREAD, Vec::new(), [f.kfd, 0, take as i32, lo, hi])?;
                if n > 0 {
                    write_guest(caller, ptr + done as u32, &data[..n as usize]);
                }
                n
            };
            if n < 0 {
                return Ok(kerrno(caller));
            }
            done += n as usize;
            total += n as usize;
            if let Some(p) = pos {
                pos = Some(p + n as u64);
            }
            if n == 0 || (n as usize) < take {
                break;
            }
        }
        if done < len {
            break;
        }
    }
    write_guest(caller, out_p as u32, &(total as u32).to_le_bytes());
    Ok(e::SUCCESS)
}
