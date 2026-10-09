// A WASI program, in its process's worker (docs/architecture.md, "A WASI
// binary runs natively"; Christine, 2026-10-08: "WASI is not a
// personality, we support WASI binaries natively"). WASI preview 1 is
// browser_wasi_shim's (vendor/browser_wasi_shim; Christine, 2026-10-09),
// and the program's files are the process's namespace: every open, read,
// write and stat it makes is this system's call, made by the process. It is
// hosts/ipnx/src/wasi.rs — wasi-common, on the terminal — on this machine,
// and it follows that file part by part; where the two differ, it says so.
//
// The program's memory is its own, and the kernel cannot reach it (an image
// imports its memory: hosts/web/src/machine.rs). The kernel gives the
// process a small memory instead, WASIREGION pages there, and each call is
// made through it: a name or the data copied in, the call made — through
// the mailbox, as a C program's is — and what it answers copied out.
//
// What WASI asks that Plan 9 has another way of saying is answered as APE,
// Plan 9's own POSIX, answers it, as on the terminal: an error string is an
// errno by _syserrno's table; a rename is rename.c's; the environment is
// _envsetup.c's; O_APPEND is a seek to the end before each write; an exit
// status is _exit.c's.
//
// The shim is used as it is. Where one of its calls is wrong for a program
// here, the call is replaced, not the shim patched: poll_oneoff reads a
// clock's flags at the wrong offset, takes one subscription only, never
// says how many events it wrote, and waits by spinning; args_sizes_get
// counts an argument's UTF-16 units where args_get writes its UTF-8 bytes;
// path_rename moves an inode, which the namespace does not have; the
// sockets throw a string; and reads and writes are one call each for every
// buffer a program passes, where the terminal makes one. A file's stat,
// whose times its Filestat writes at the wrong offsets, is this file's own.

import WASI from './vendor/browser_wasi_shim/wasi.js';
import { Fd } from './vendor/browser_wasi_shim/fd.js';
import * as W from './vendor/browser_wasi_shim/wasi_defs.js';
import { CALLS } from './mailbox.mjs';
import { Reader, Writer, QTDIR, DMDIR, OTRUNC } from './ninep.mjs';

const enc = new TextEncoder(), dec = new TextDecoder();

// Plan 9's open modes (libc.h)
const OREAD = 0, OWRITE = 1, ORDWR = 2, OEXCL = 0x1000;

// The region the calls are made through (machine.rs, WASIREGION): a name,
// the error string, a directory entry, and the data one read or write
// moves. Nothing is at 0, which a call reads as nil — `exits(nil)` is the
// empty status, and a name there would be none.
const NAME = 64, NAMEMAX = 4096, ERR = 8192, ERRMAX = 128;
const STAT = 65536, STATMAX = 65536, DATA = 131072, DATAMAX = 1 << 20;

let R;      // the region
let kcall;  // the process's call (proc.mjs): its number and words, its value
let w;      // the shim: its descriptors, and the program's memory

// Whether a module is a WASI program: it imports WASI preview 1's calls
// (hosts/web/src/module.rs, `wasi`, which the kernel asks).
export function iswasi(module) {
  return WebAssembly.Module.imports(module).some((i) => i.module === 'wasi_snapshot_preview1');
}

// ---- the calls -------------------------------------------------------

// A WASI error: the errno WASI has for what failed.
class Err extends Error {
  constructor(errno) {
    super(`errno ${errno}`);
    this.errno = errno;
  }
}

// What a call's failure is caught as — Err, and nothing else: the process
// being ended (proc.mjs, Gone) goes on out of everything.
function wrap(f, fail) {
  try {
    return f();
  } catch (e) {
    if (e instanceof Err) return fail(e.errno);
    throw e;
  }
}

// The process's call; a failure is its errstr, as an errno.
function sys(no, args) {
  const v = kcall(no, args);
  if (v < 0n) throw new Err(errno(errstr()));
  return v;
}

// `errstr`, which exchanges: the error, and none left behind.
function errstr() {
  R[ERR] = 0;
  kcall(CALLS.errstr, [ERR, ERRMAX]);
  const end = R.indexOf(0, ERR);
  return dec.decode(R.slice(ERR, end < 0 || end > ERR + ERRMAX ? ERR + ERRMAX : end));
}

// A name in the region, as every name crosses: ended by a NUL.
function put(at, s) {
  const b = enc.encode(s);
  if (b.length >= NAMEMAX) throw new Err(W.ERRNO_NAMETOOLONG);
  R.set(b, at);
  R[at + b.length] = 0;
  return at;
}

const open = (path, mode) => Number(sys(CALLS.open, [put(NAME, path), mode]));
const create = (path, mode, perm) => Number(sys(CALLS.create, [put(NAME, path), mode, perm]));
const close = (fd) => sys(CALLS.close, [fd]);
const remove = (path) => sys(CALLS.remove, [put(NAME, path)]);
const seek = (fd, off, whence) => sys(CALLS.seek, [fd, BigInt.asIntN(64, BigInt(off)), whence]);

// A close whose failure changes nothing for the caller.
const quietclose = (fd) => wrap(() => close(fd), () => {});

// `pread`, at `off` or, for -1, where the file is: at most what the region
// holds, which a short read says.
function pread(fd, n, off) {
  const k = Number(sys(CALLS.pread, [fd, DATA, Math.min(n, DATAMAX), BigInt(off)]));
  return R.slice(DATA, DATA + k);
}

// `pwrite`, a region's worth at a time: how much was written.
function pwrite(fd, b, off) {
  let done = 0;
  do {
    const k = Math.min(b.length - done, DATAMAX);
    R.set(b.subarray(done, done + k), DATA);
    const n = Number(sys(CALLS.pwrite, [fd, DATA, k, off < 0 ? -1n : BigInt(off) + BigInt(done)]));
    done += n;
    if (n < k) break;
  } while (done < b.length);
  return done;
}

// A directory entry, stat(5)'s machine-independent form (`convM2D`).
function parsedir(b) {
  const r = new Reader(b);
  r.u16();
  const type = r.u16(), dev = r.u32();
  const qid = { type: r.u8(), vers: r.u32(), path: r.u64() };
  const mode = r.u32(), atime = r.u32(), mtime = r.u32(), length = r.u64();
  return { type, dev, qid, mode, atime, mtime, length, name: r.s(), uid: r.s(), gid: r.s(), muid: r.s() };
}

const stat = (path) => parsedir(R.slice(STAT, STAT + Number(sys(CALLS.stat, [put(NAME, path), STAT, STATMAX]))));
const fstat = (fd) => parsedir(R.slice(STAT, STAT + Number(sys(CALLS.fstat, [fd, STAT, STATMAX]))));
const exists = (path) => wrap(() => (stat(path), true), () => false);

// `nulldir` (libc/9sys/nulldir.c) with what is to change: every other
// field "don't touch", for a wstat.
function edir({ mode = 0xffffffff, atime = 0xffffffff, mtime = 0xffffffff, length = 0xffffffffffffffffn, name = '' }) {
  const b = new Writer()
    .u16(0).u16(0xffff).u32(0xffffffff).qid({ type: 0xff, vers: 0xffffffff, path: 0xffffffffffffffffn })
    .u32(mode).u32(atime).u32(mtime).u64(length).s(name).s('').s('').s('')
    .bytes();
  new DataView(b.buffer).setUint16(0, b.length - 2, true);
  return b;
}

function wstat(path, d) {
  const b = edir(d);
  R.set(b, STAT);
  sys(CALLS.wstat, [put(NAME, path), STAT, b.length]);
}

function fwstat(fd, d) {
  const b = edir(d);
  R.set(b, STAT);
  sys(CALLS.fwstat, [fd, STAT, b.length]);
}

// Everything a directory reads as, in its entries.
function entries(path) {
  const fd = open(path, OREAD);
  const parts = [];
  try {
    for (;;) {
      const b = pread(fd, 8192, -1);
      if (b.length === 0) break;
      parts.push(b);
    }
  } finally {
    quietclose(fd);
  }
  const out = [];
  for (const b of parts) {
    for (let at = 0; at + 2 <= b.length; ) {
      const n = b[at] | (b[at + 1] << 8);
      out.push(parsedir(b.subarray(at, at + 2 + n)));
      at += 2 + n;
    }
  }
  return out;
}

// _syserrno (ape/lib/ap/plan9/_errno.c:15, :110), as wasi.rs has it: the
// first entry whose text the error contains, ignoring case, and EINVAL for
// none; and the host's own words, which a server of a host's files answers.
const ERRNOS = [
  // from /sys/src/9/port/errstr.h
  ['inconsistent mount', W.ERRNO_INVAL], ['not mounted', W.ERRNO_INVAL], ['not in union', W.ERRNO_INVAL],
  ['mount rpc error', W.ERRNO_IO], ['mounted device shut down', W.ERRNO_IO],
  ['mounted directory forbids creation', W.ERRNO_PERM], ['does not exist', W.ERRNO_NOENT],
  ['unknown device in # filename', W.ERRNO_NXIO], ['not a directory', W.ERRNO_NOTDIR],
  ['file is a directory', W.ERRNO_ISDIR], ['bad character in file name', W.ERRNO_INVAL],
  ['file name syntax', W.ERRNO_INVAL], ['permission denied', W.ERRNO_PERM],
  ['inappropriate use of fd', W.ERRNO_PERM], ['bad arg in system call', W.ERRNO_INVAL],
  ['device or object already in use', W.ERRNO_BUSY], ['i/o error', W.ERRNO_IO],
  ['read or write too large', W.ERRNO_IO], ['read or write too small', W.ERRNO_IO],
  ['network port not available', W.ERRNO_ADDRINUSE], ['write to hungup stream', W.ERRNO_PIPE],
  ['i/o on hungup channel', W.ERRNO_PIPE], ['bad process or channel control request', W.ERRNO_INVAL],
  ['no free devices', W.ERRNO_BUSY], ['process exited', W.ERRNO_SRCH], ['no living children', W.ERRNO_CHILD],
  ['i/o error in demand load', W.ERRNO_IO], ['virtual memory allocation failed', W.ERRNO_NOMEM],
  ['fd out of range or not open', W.ERRNO_BADF], ['no free file descriptors', W.ERRNO_MFILE],
  ['seek on a stream', W.ERRNO_SPIPE], ['exec header invalid', W.ERRNO_NOEXEC],
  ['connection timed out', W.ERRNO_TIMEDOUT], ['connection refused', W.ERRNO_CONNREFUSED],
  ['connection in use', W.ERRNO_CONNREFUSED], ['interrupted', W.ERRNO_INTR],
  ['kernel allocate failed', W.ERRNO_NOMEM], ['segments overlap', W.ERRNO_INVAL],
  ['i/o count too small', W.ERRNO_IO], ['ken has left the building', W.ERRNO_IO],
  ['bad attach specifier', W.ERRNO_INVAL],
  // from exhausted() calls in kernel
  ['no free mount devices', W.ERRNO_BUSY], ['no free mount rpc buffer', W.ERRNO_BUSY],
  ['no free segments', W.ERRNO_BUSY], ['no free memory', W.ERRNO_NOMEM], ['no free Blocks', W.ERRNO_NOBUFS],
  ['no free routes', W.ERRNO_BUSY],
  // from ken
  ['attach -- bad specifier', W.ERRNO_INVAL], ['unknown fid', W.ERRNO_BADF],
  ['bad character in directory name', W.ERRNO_INVAL], ['read/write -- on non open fid', W.ERRNO_BADF],
  ['read/write -- count too big', W.ERRNO_IO], ['phase error -- directory entry not allocated', W.ERRNO_IO],
  ['phase error -- qid does not match', W.ERRNO_IO], ['access permission denied', W.ERRNO_ACCES],
  ['directory entry not found', W.ERRNO_NOENT], ['open/create -- unknown mode', W.ERRNO_INVAL],
  ['walk -- in a non-directory', W.ERRNO_NOTDIR], ['create -- in a non-directory', W.ERRNO_NOTDIR],
  ['phase error -- cannot happen', W.ERRNO_IO], ['create -- file exists', W.ERRNO_EXIST],
  ['create -- . and .. illegal names', W.ERRNO_INVAL], ['directory not empty', W.ERRNO_NOTEMPTY],
  ['attach -- privileged user', W.ERRNO_INVAL], ['wstat -- not owner', W.ERRNO_PERM],
  ['wstat -- not in group', W.ERRNO_PERM], ['create/wstat -- bad character in file name', W.ERRNO_INVAL],
  ['walk -- too many (system wide)', W.ERRNO_BUSY], ['file system read only', W.ERRNO_ROFS],
  ['file system full', W.ERRNO_NOSPC], ['read/write -- offset negative', W.ERRNO_INVAL],
  ['open/create -- file is locked', W.ERRNO_BUSY], ['close/read/write -- lock is broken', W.ERRNO_BUSY],
  // from sockets
  ['not a socket', W.ERRNO_NOTSOCK], ['protocol not supported', W.ERRNO_PROTONOSUPPORT],
  ['address family not supported', W.ERRNO_AFNOSUPPORT], ['insufficient buffer space', W.ERRNO_NOBUFS],
  ['operation not supported', W.ERRNO_NOTSUP], ['address in use', W.ERRNO_ADDRINUSE],
  ['unnamed error message', W.ERRNO_IO],
  // a host's, through a server of its files
  ['already exists', W.ERRNO_EXIST], ['file exists', W.ERRNO_EXIST], ['is a directory', W.ERRNO_ISDIR],
  ['no such file or directory', W.ERRNO_NOENT],
].map(([s, n]) => [s.toLowerCase(), n]);

function errno(e) {
  e = e.toLowerCase();
  return ERRNOS.find(([s]) => e.includes(s))?.[1] ?? W.ERRNO_INVAL;
}

// ---- what a file is ---------------------------------------------------

// What a file is, for WASI, from its entry (wasi.rs, `filetype`): a
// directory; a file a server serves, or the environment's; a pipe, which
// WASI has no type for; and otherwise a device's, a character device.
function filetype(d) {
  if (d.qid.type & QTDIR) return W.FILETYPE_DIRECTORY;
  switch (String.fromCharCode(d.type)) {
    case 'M':
    case 'e':
      return W.FILETYPE_REGULAR_FILE;
    case '|':
      return W.FILETYPE_UNKNOWN;
    default:
      return W.FILETYPE_CHARACTER_DEVICE;
  }
}

// A file's stat, in WASI's layout: dev, ino, filetype, nlink, size and the
// three times at 0, 8, 16, 24, 32, 40, 48 and 56. Plan 9 keeps no change
// time; the last modification is the nearest.
class Stat {
  constructor(d) {
    this.d = d;
  }
  write_bytes(v, p) {
    const d = this.d, ns = (s) => BigInt(s) * 1000000000n;
    v.setBigUint64(p, (BigInt(d.type) << 32n) | BigInt(d.dev), true);
    v.setBigUint64(p + 8, d.qid.path, true);
    v.setUint8(p + 16, filetype(d));
    v.setBigUint64(p + 24, 1n, true);
    v.setBigUint64(p + 32, d.length, true);
    v.setBigUint64(p + 40, ns(d.atime), true);
    v.setBigUint64(p + 48, ns(d.mtime), true);
    v.setBigUint64(p + 56, ns(d.mtime), true);
  }
}

// What a set of times WASI gives changes, in Plan 9's seconds (wasi-common's
// `systimespec`): a time, now, or nothing; both a time and now is wrong.
function times(atim, mtim, flags) {
  const now = Math.floor(Date.now() / 1000);
  const one = (t, set, isnow) => {
    if (set && isnow) throw new Err(W.ERRNO_INVAL);
    if (set) return Number(t / 1000000000n);
    if (isnow) return now;
    return undefined;
  };
  return {
    atime: one(atim, flags & W.FSTFLAGS_ATIM, flags & W.FSTFLAGS_ATIM_NOW),
    mtime: one(mtim, flags & W.FSTFLAGS_MTIM, flags & W.FSTFLAGS_MTIM_NOW),
  };
}

// The rights a descriptor says it has, as wasi-common says them: a file
// what it was opened for, a directory what its wasi-common counterpart
// lists (preview_1.rs, `directory_base_rights`).
const RIGHT = (r) => BigInt(r);
const DIRBASE = [
  W.RIGHTS_PATH_CREATE_DIRECTORY, W.RIGHTS_PATH_CREATE_FILE, W.RIGHTS_PATH_LINK_SOURCE,
  W.RIGHTS_PATH_LINK_TARGET, W.RIGHTS_PATH_OPEN, W.RIGHTS_FD_READDIR, W.RIGHTS_PATH_READLINK,
  W.RIGHTS_PATH_RENAME_SOURCE, W.RIGHTS_PATH_RENAME_TARGET, W.RIGHTS_PATH_SYMLINK,
  W.RIGHTS_PATH_REMOVE_DIRECTORY, W.RIGHTS_PATH_UNLINK_FILE, W.RIGHTS_PATH_FILESTAT_GET,
  W.RIGHTS_PATH_FILESTAT_SET_TIMES, W.RIGHTS_FD_FILESTAT_GET, W.RIGHTS_FD_FILESTAT_SET_TIMES,
].reduce((a, r) => a | RIGHT(r), 0n);
const DIRINHERIT = [
  W.RIGHTS_FD_DATASYNC, W.RIGHTS_FD_READ, W.RIGHTS_FD_SEEK, W.RIGHTS_FD_FDSTAT_SET_FLAGS, W.RIGHTS_FD_SYNC,
  W.RIGHTS_FD_TELL, W.RIGHTS_FD_WRITE, W.RIGHTS_FD_ADVISE, W.RIGHTS_FD_ALLOCATE, W.RIGHTS_FD_FILESTAT_GET,
  W.RIGHTS_FD_FILESTAT_SET_SIZE, W.RIGHTS_FD_FILESTAT_SET_TIMES, W.RIGHTS_POLL_FD_READWRITE,
].reduce((a, r) => a | RIGHT(r), DIRBASE);

function fdstat(filetype, flags, base, inheriting) {
  const s = new W.Fdstat(filetype, flags);
  s.fs_rights_base = base;
  s.fs_rights_inherited = inheriting;
  return s;
}

// ---- a file the process has open ---------------------------------------

// A descriptor of the process's own, which every operation on it is a call
// on (wasi.rs, `File`). `own` is a standard stream: the process's, which
// the program closing it does not close.
class File extends Fd {
  constructor(fd, own, read, write) {
    super();
    this.fd = fd;
    this.own = own;
    this.read = read;
    this.write = write;
    this.flags = 0;
    this.kind = wrap(() => filetype(fstat(fd)), () => W.FILETYPE_UNKNOWN);
  }
  fd_close() {
    if (this.own) return 0;
    return wrap(() => (close(this.fd), 0), (n) => n);
  }
  fd_fdstat_get() {
    const base = (this.read ? RIGHT(W.RIGHTS_FD_READ) : 0n) | (this.write ? RIGHT(W.RIGHTS_FD_WRITE) : 0n);
    return { ret: 0, fdstat: fdstat(this.kind, this.flags, base, 0n) };
  }
  fd_fdstat_set_flags(flags) {
    this.flags = flags;
    return 0;
  }
  fd_filestat_get() {
    return wrap(() => ({ ret: 0, filestat: new Stat(fstat(this.fd)) }), (n) => ({ ret: n, filestat: null }));
  }
  fd_filestat_set_size(size) {
    return wrap(() => (fwstat(this.fd, { length: size }), 0), (n) => n);
  }
  fd_filestat_set_times(atim, mtim, flags) {
    return wrap(() => (fwstat(this.fd, times(atim, mtim, flags)), 0), (n) => n);
  }
  fd_read(size) {
    return this.fd_pread(size, -1n);
  }
  fd_pread(size, offset) {
    return wrap(() => ({ ret: 0, data: pread(this.fd, size, offset) }), (n) => ({ ret: n, data: new Uint8Array() }));
  }
  // O_APPEND, as APE writes: "if(f->oflags&O_APPEND) _SEEK(fd, 0, 2)"
  // before each write (ape/lib/ap/plan9/write.c)
  fd_write(data) {
    return wrap(() => {
      if (this.flags & W.FDFLAGS_APPEND) seek(this.fd, 0, 2);
      return { ret: 0, nwritten: pwrite(this.fd, data, -1) };
    }, (n) => ({ ret: n, nwritten: 0 }));
  }
  fd_pwrite(data, offset) {
    return wrap(() => ({ ret: 0, nwritten: pwrite(this.fd, data, offset) }), (n) => ({ ret: n, nwritten: 0 }));
  }
  // WASI's whence is Plan 9's: SET, CUR, END are 0, 1, 2
  fd_seek(offset, whence) {
    if (whence === W.WHENCE_SET && offset < 0n) return { ret: W.ERRNO_INVAL, offset: 0n };
    return wrap(() => ({ ret: 0, offset: BigInt.asUintN(64, seek(this.fd, offset, whence)) }), (n) => ({ ret: n, offset: 0n }));
  }
  fd_tell() {
    return this.fd_seek(0n, W.WHENCE_CUR);
  }
  fd_prestat_get() {
    return { ret: W.ERRNO_BADF, prestat: null };
  }
  fd_readdir_single() {
    return { ret: W.ERRNO_BADF, dirent: null };
  }
}

// ---- a directory of the namespace --------------------------------------

// A directory, by its name: every operation on it is a call on that name,
// joined to the one WASI gives (wasi.rs, `Directory`). `preopen` is the
// namespace's root, which the program is given as `/`.
class Dir extends Fd {
  constructor(path, preopen = false) {
    super();
    this.path = path;
    this.preopen = preopen;
    this.listing = null;
  }
  join(p) {
    if (p === '' || p === '.') return this.path;
    return this.path.endsWith('/') ? `${this.path}${p}` : `${this.path}/${p}`;
  }
  fd_close() {
    return 0;
  }
  fd_fdstat_get() {
    return { ret: 0, fdstat: fdstat(W.FILETYPE_DIRECTORY, 0, DIRBASE, DIRINHERIT) };
  }
  fd_filestat_get() {
    return wrap(() => ({ ret: 0, filestat: new Stat(stat(this.path)) }), (n) => ({ ret: n, filestat: null }));
  }
  fd_filestat_set_times(atim, mtim, flags) {
    return wrap(() => (wstat(this.path, times(atim, mtim, flags)), 0), (n) => n);
  }
  fd_prestat_get() {
    if (!this.preopen) return { ret: W.ERRNO_NOTSUP, prestat: null };
    return { ret: 0, prestat: W.Prestat.dir('/') };
  }
  // A file's operations, which a directory is not: wasi-common's answer
  fd_read() { return { ret: W.ERRNO_BADF, data: new Uint8Array() }; }
  fd_pread() { return { ret: W.ERRNO_BADF, data: new Uint8Array() }; }
  fd_write() { return { ret: W.ERRNO_BADF, nwritten: 0 }; }
  fd_pwrite() { return { ret: W.ERRNO_BADF, nwritten: 0 }; }
  fd_seek() { return { ret: W.ERRNO_BADF, offset: 0n }; }
  fd_tell() { return { ret: W.ERRNO_BADF, offset: 0n }; }
  fd_fdstat_set_flags() { return W.ERRNO_BADF; }
  fd_filestat_set_size() { return W.ERRNO_BADF; }
  // The directory's entries after `.` and `..`, as wasi-common's own
  // directories give them — read again from the start, and kept while the
  // program reads on.
  fd_readdir_single(cookie) {
    return wrap(() => {
      if (cookie === 0n || !this.listing) {
        const me = stat(this.path);
        this.listing = [
          ['.', me.qid.path, W.FILETYPE_DIRECTORY],
          ['..', me.qid.path, W.FILETYPE_DIRECTORY],
          ...entries(this.path).map((d) => [d.name, d.qid.path, filetype(d)]),
        ];
      }
      const i = Number(cookie);
      if (i >= this.listing.length) return { ret: 0, dirent: null };
      const [name, ino, type] = this.listing[i];
      return { ret: 0, dirent: new W.Dirent(BigInt(i + 1), ino, name, type) };
    }, (n) => ({ ret: n, dirent: null }));
  }
  // An open: `open` for a file there, and — with O_CREAT — `create` for one
  // that is not, which is how APE's open makes one; O_EXCL is create's
  // OEXCL. A directory, asked for or found, is answered as one.
  path_open(dirflags, path, oflags, base, inheriting, fdflags) {
    return wrap(() => {
      const full = this.join(path);
      if (oflags & W.OFLAGS_DIRECTORY) {
        if (!(stat(full).qid.type & QTDIR)) throw new Err(W.ERRNO_NOTDIR);
        return { ret: 0, fd_obj: new Dir(full) };
      }
      const read = (base & RIGHT(W.RIGHTS_FD_READ)) !== 0n, write = (base & RIGHT(W.RIGHTS_FD_WRITE)) !== 0n;
      let mode = write ? (read ? ORDWR : OWRITE) : OREAD;
      if (oflags & W.OFLAGS_TRUNC) mode |= OTRUNC;
      let fd;
      if (oflags & W.OFLAGS_CREAT && oflags & W.OFLAGS_EXCL) fd = create(full, mode | OEXCL, 0o666);
      else if (oflags & W.OFLAGS_CREAT) {
        try {
          fd = open(full, mode);
        } catch (e) {
          if (!(e instanceof Err) || e.errno !== W.ERRNO_NOENT) throw e;
          fd = create(full, mode, 0o666);
        }
      } else fd = open(full, mode);
      const f = new File(fd, false, read, write);
      if (f.kind === W.FILETYPE_DIRECTORY) {
        quietclose(fd);
        return { ret: 0, fd_obj: new Dir(full) };
      }
      f.flags = fdflags;
      return { ret: 0, fd_obj: f };
    }, (n) => ({ ret: n, fd_obj: null }));
  }
  // `create` with DMDIR (sysfile.c's syscreate of a directory)
  path_create_directory(path) {
    return wrap(() => (close(create(this.join(path), OREAD, DMDIR | 0o777)), 0), (n) => n);
  }
  path_filestat_get(flags, path) {
    return wrap(() => ({ ret: 0, filestat: new Stat(stat(this.join(path))) }), (n) => ({ ret: n, filestat: null }));
  }
  path_filestat_set_times(flags, path, atim, mtim, fst) {
    return wrap(() => (wstat(this.join(path), times(atim, mtim, fst)), 0), (n) => n);
  }
  // Plan 9 has no symbolic links, so nothing is one: readlink(2)'s answer
  // for a file that is not, EINVAL
  path_readlink() {
    return { ret: W.ERRNO_INVAL, data: null };
  }
  path_remove_directory(path) {
    return wrap(() => (remove(this.join(path)), 0), (n) => n);
  }
  path_unlink_file(path) {
    return wrap(() => (remove(this.join(path)), 0), (n) => n);
  }
  // APE's rename (ape/lib/ap/plan9/rename.c): what is at the new name is
  // removed first; then, in the same directory, the name is changed by
  // wstat; in another, the file is copied and the old one removed — Plan 9
  // renames only within a directory.
  rename(path, dest, destpath) {
    const from = this.join(path), to = dest.join(destpath);
    if (exists(to)) {
      remove(to);
      if (exists(to)) throw new Err(W.ERRNO_EXIST);
    }
    const d = stat(from);
    const [fdir] = split(from), [tdir, tname] = split(to);
    if (fdir === tdir) {
      wstat(from, { name: tname });
      return;
    }
    const ffd = open(from, OREAD);
    let tfd;
    try {
      tfd = create(to, OWRITE, d.mode);
    } catch (e) {
      quietclose(ffd);
      throw e;
    }
    let err = null;
    try {
      for (;;) {
        const b = pread(ffd, 8192, -1);
        if (b.length === 0) break;
        if (pwrite(tfd, b, -1) !== b.length) throw new Err(W.ERRNO_IO);
      }
    } catch (e) {
      err = e;
    }
    quietclose(ffd);
    quietclose(tfd);
    if (err) throw err;
    remove(from);
  }
}

// The directory a path is in, and the name it ends in.
function split(p) {
  const i = p.lastIndexOf('/');
  if (i === 0) return ['/', p.slice(1)];
  if (i > 0) return [p.slice(0, i), p.slice(i + 1)];
  return ['.', p];
}

// ---- the process ---------------------------------------------------------

// The process's environment, as APE makes one (_envsetup.c:30): each file
// of #e as a variable, its value's last 0 byte dropped and any other made 1.
function environment() {
  const out = [];
  const names = wrap(() => entries('#e'), () => []);
  for (const e of names) {
    if (e.name === '' || e.name.includes('=')) continue;
    const v = wrap(() => {
      const fd = open(`#e/${e.name}`, OREAD);
      try {
        const b = pread(fd, Number(e.length), -1);
        return b.length === Number(e.length) ? b : new Uint8Array();
      } finally {
        quietclose(fd);
      }
    }, () => null);
    if (v === null) continue;
    let b = v;
    if (b.length && b[b.length - 1] === 0) b = b.subarray(0, b.length - 1);
    b = b.map((x) => (x === 0 ? 1 : x));
    out.push(`${e.name}=${dec.decode(b)}`);
  }
  return out;
}

// The program's memory, and a string in it.
const view = () => new DataView(w.inst.exports.memory.buffer);
const bytes = () => new Uint8Array(w.inst.exports.memory.buffer);
const str = (p, n) => dec.decode(bytes().slice(p, p + n));

// The buffers a call names, as (address, length): iovec and ciovec are the
// same two words.
function iovecs(p, n) {
  const v = view(), out = [];
  for (let i = 0; i < n; i++) out.push([v.getUint32(p + 8 * i, true), v.getUint32(p + 8 * i + 4, true)]);
  return out;
}

// Sleep for `ns` nanoseconds, as the terminal's Sleeper does: the kernel's
// sleep, in its milliseconds, rounded up.
function sleep(ns) {
  sys(CALLS.sleep, [Number((ns + 999999n) / 1000000n)]);
}

const monotonic = () => BigInt(Math.round(performance.now() * 1e6));

// **The calls replaced**, as the terminal's are made by wasi-common.
function calls(imp) {
  const shim = { clock_res_get: imp.clock_res_get };
  return {
    // UTF-8's bytes, as args_get writes them
    args_sizes_get(argc, size) {
      const v = view();
      v.setUint32(argc, w.args.length, true);
      v.setUint32(size, w.args.reduce((n, a) => n + enc.encode(a).length + 1, 0), true);
      return 0;
    },
    // wasi-common's clocks: the realtime clock, and the monotonic one since
    // the program began; a process's or a thread's CPU time it does not have
    clock_res_get(id, p) {
      if (id === W.CLOCKID_PROCESS_CPUTIME_ID || id === W.CLOCKID_THREAD_CPUTIME_ID) return W.ERRNO_BADF;
      return shim.clock_res_get(id, p);
    },
    clock_time_get(id, precision, p) {
      let t;
      if (id === W.CLOCKID_REALTIME) t = BigInt(Math.round((performance.timeOrigin + performance.now()) * 1000)) * 1000n;
      else if (id === W.CLOCKID_MONOTONIC) t = monotonic();
      else return W.ERRNO_BADF;
      view().setBigUint64(p, t, true);
      return 0;
    },
    // One call for all a program's buffers, as on the terminal.
    fd_read(fd, iovs, n, nreadp) {
      return scatter(w.fds[fd], iovs, n, nreadp, (f, size) => f.fd_read(size));
    },
    fd_pread(fd, iovs, n, off, nreadp) {
      return scatter(w.fds[fd], iovs, n, nreadp, (f, size) => f.fd_pread(size, off));
    },
    fd_write(fd, iovs, n, nwrittenp) {
      return gather(w.fds[fd], iovs, n, nwrittenp, (f, data) => f.fd_write(data));
    },
    fd_pwrite(fd, iovs, n, off, nwrittenp) {
      return gather(w.fds[fd], iovs, n, nwrittenp, (f, data) => f.fd_pwrite(data, off));
    },
    path_rename(fd, op, ol, nfd, np, nl) {
      const from = w.fds[fd], to = w.fds[nfd];
      if (!(from instanceof Dir) || !(to instanceof Dir)) return W.ERRNO_BADF;
      return wrap(() => (from.rename(str(op, ol), to, str(np, nl)), 0), (e) => e);
    },
    // WASI's waiting, as the kernel's sleep (wasi.rs, `Sleeper`). A file is
    // ready at once: a read or write that cannot be answered yet waits in
    // the kernel, as every read does. A clock is slept for until it is due.
    poll_oneoff(inp, outp, nsubs, neventsp) {
      if (nsubs === 0) return W.ERRNO_INVAL;
      const v = view();
      const subs = [];
      for (let i = 0; i < nsubs; i++) {
        const p = inp + 48 * i;
        const s = { userdata: v.getBigUint64(p, true), type: v.getUint8(p + 8) };
        if (s.type > W.EVENTTYPE_FD_WRITE) return W.ERRNO_INVAL;
        if (s.type === W.EVENTTYPE_CLOCK) {
          s.id = v.getUint32(p + 16, true);
          s.timeout = v.getBigUint64(p + 24, true);
          s.abs = (v.getUint16(p + 40, true) & W.SUBCLOCKFLAGS_SUBSCRIPTION_CLOCK_ABSTIME) !== 0;
        } else s.fd = v.getUint32(p + 16, true);
        subs.push(s);
      }
      const event = (i, s, nbytes) => {
        const p = outp + 32 * i;
        v.setBigUint64(p, s.userdata, true);
        v.setUint16(p + 8, 0, true);
        v.setUint8(p + 10, s.type);
        v.setBigUint64(p + 16, nbytes, true);
        v.setUint16(p + 24, 0, true);
      };
      return wrap(() => {
        // one relative clock: a sleep, whatever its clock
        if (nsubs === 1 && subs[0].type === W.EVENTTYPE_CLOCK && !subs[0].abs) {
          sleep(subs[0].timeout);
          event(0, subs[0], 0n);
          v.setUint32(neventsp, 1, true);
          return 0;
        }
        const now = monotonic();
        for (const s of subs) {
          if (s.type === W.EVENTTYPE_CLOCK) {
            if (s.id === W.CLOCKID_MONOTONIC) s.deadline = s.abs ? s.timeout : now + s.timeout;
            else if (s.id === W.CLOCKID_REALTIME) {
              if (s.abs) throw new Err(W.ERRNO_NOTSUP);
              s.deadline = now + s.timeout;
            } else throw new Err(W.ERRNO_INVAL);
          } else if (!(w.fds[s.fd] instanceof File)) throw new Err(W.ERRNO_BADF);
        }
        const files = subs.some((s) => s.type !== W.EVENTTYPE_CLOCK);
        if (!files) {
          const due = subs.reduce((a, s) => (s.deadline < a ? s.deadline : a), subs[0].deadline);
          for (let t = monotonic(); t < due; t = monotonic()) sleep(due - t);
        }
        const at = monotonic();
        let n = 0;
        for (const s of subs) {
          if (s.type !== W.EVENTTYPE_CLOCK) event(n++, s, 1n);
          else if (s.deadline <= at) event(n++, s, 0n);
        }
        v.setUint32(neventsp, n, true);
        return 0;
      }, (e) => e);
    },
    // sleep(0), which is yield() (sysproc.c's syssleep)
    sched_yield() {
      return wrap(() => (sys(CALLS.sleep, [0]), 0), (e) => e);
    },
    // exits with APE's status for it — "if(status){ cp = _ultoa(exitstatus,
    // status & 0xFF); …" (_exit.c:26) — none for 0; and, as wasi-common
    // has it, no status from 126 on (preview_1.rs, proc_exit)
    proc_exit(code) {
      if (code >>> 0 >= 126) throw new Error('exit with invalid exit status outside of [0..126)');
      kcall(CALLS.exits, [code === 0 ? 0 : put(NAME, String(code & 0xff))]);
      throw new Error('exits returned');
    },
    proc_raise() {
      throw new Error('proc_raise unsupported');
    },
    // no file here is a socket: wasi-common's answer for one that is not
    sock_accept: () => W.ERRNO_BADF,
    sock_recv: () => W.ERRNO_BADF,
    sock_send: () => W.ERRNO_BADF,
    sock_shutdown: () => W.ERRNO_BADF,
  };
}

// A read into the program's buffers, made as one.
function scatter(f, iovs, n, nreadp, read) {
  if (!f) return W.ERRNO_BADF;
  const bufs = iovecs(iovs, n);
  const { ret, data } = read(f, bufs.reduce((t, [, l]) => t + l, 0));
  if (ret !== 0) return ret;
  const m = bytes();
  let at = 0;
  for (const [p, l] of bufs) {
    if (at === data.length) break;
    const k = Math.min(l, data.length - at);
    m.set(data.subarray(at, at + k), p);
    at += k;
  }
  view().setUint32(nreadp, data.length, true);
  return 0;
}

// A write of the program's buffers, made as one.
function gather(f, iovs, n, nwrittenp, write) {
  if (!f) return W.ERRNO_BADF;
  const bufs = iovecs(iovs, n);
  const data = new Uint8Array(bufs.reduce((t, [, l]) => t + l, 0));
  const m = bytes();
  let at = 0;
  for (const [p, l] of bufs) {
    data.set(m.subarray(p, p + l), at);
    at += l;
  }
  const { ret, nwritten } = write(f, data);
  if (ret !== 0) return ret;
  view().setUint32(nwrittenp, nwritten, true);
  return 0;
}

// **Run a WASI program** in this process: its arguments `exec`'s, the
// process's environment, descriptors 0, 1 and 2 as its standard streams,
// and the namespace preopened as `/`. Returns when its `_start` does — the
// image ending, as a C program's does; `proc_exit` is `exits`, which does
// not return.
export function runwasi(module, region, args, call) {
  R = new Uint8Array(region.buffer);
  kcall = call;
  const fds = [new File(0, true, true, false), new File(1, true, false, true), new File(2, true, false, true), new Dir('/', true)];
  // its log is on unless it is told otherwise (debug.js, `enable`)
  w = new WASI(args, environment(), fds, { debug: false });
  Object.assign(w.wasiImport, calls(w.wasiImport));
  const instance = new WebAssembly.Instance(module, { wasi_snapshot_preview1: w.wasiImport });
  w.start(instance);
}
