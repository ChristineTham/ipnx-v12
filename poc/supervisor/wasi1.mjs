// wasi_snapshot_preview1 over the mailbox: the WASI second ABI as a shim
// (decision log: "9P is the system interface; WASI is a shim",
// `wasi:cli/command` only). A wasip1 module — Go's wasip1 target, wasi-libc —
// exports its memory, never forks, and sees exactly one preopen: fd 3 is "/",
// **the preopen is the namespace root**. That single line is the citizenship
// clause made literal: a foreign binary's whole visible universe is whatever
// this process's namespace shows, assembled by bind like anyone else's.
//
// Paths are resolved shim-side (WASI is dirfd-relative and has no cwd), then
// walked by the kernel — so symlinks, unions and mounts behave identically
// for citizens of either ABI. Errors come back as errstr text and leave as
// wasi errno numbers; the mapping is honest but lossy, like every errno.
//
// Deviations, deliberate (v0): environ is empty; offsets are 32-bit (the
// mailbox word); poll_oneoff fires all clock subscriptions after one sleep.

const T = { CLOSE: 4, DUP: 5, EXITS: 8, OPEN: 14, SLEEP: 17, CREATE: 22,
  REMOVE: 25, SEEK: 39, ERRSTR: 41, STAT: 42, FSTAT: 43, PREAD: 50,
  PWRITE: 51, NSEC: 53, LINK: 60, SYMLINK: 61, READLINK: 62, ARGS: 200 };
const NOCOPY = -1;            // guest-pointer slot when no copy-out is wanted
const TXSIZE = 65536;
const OREAD = 0, OWRITE = 1, ORDWR = 2, OTRUNC = 16;
const DMDIR = 0x80000000;

// wasi errno, the preview1 table
const E = { success: 0, acces: 2, again: 6, badf: 8, exist: 20, fault: 21,
  inval: 28, io: 29, isdir: 31, nametoolong: 37, noent: 44, nosys: 52,
  notdir: 54, notempty: 55, notsup: 58, perm: 63, spipe: 70 };

// filetypes
const FT = { unknown: 0, block: 1, chardev: 2, dir: 3, regular: 4, symlink: 7 };

export class WasiExit { constructor(code) { this.code = code; } }

export function makeWasi(ctx) {
  const { sys, sysTx, txView, m8 } = ctx;
  const dv = () => new DataView(m8().buffer);

  // ---- the fd table: 0/1/2 pass through; 3 is the preopen "/" ----
  const fds = new Map([
    [0, { kfd: 0, path: "/dev/cons", isdir: false, append: false }],
    [1, { kfd: 1, path: "/dev/cons", isdir: false, append: false }],
    [2, { kfd: 2, path: "/dev/cons", isdir: false, append: false }],
    [3, { kfd: -1, path: "/", isdir: true, preopen: true, append: false }],
  ]);
  let nextfd = 4;
  const fdAlloc = (info) => { const n = nextfd++; fds.set(n, info); return n; };

  // ---- errors: errstr text -> errno number ----
  function kerrno() {
    const n = sysTx(T.ERRSTR, [], 0, 128, 0, 0, 0);   // a1 is errstr(2)'s cap
    // slice, not subarray: a browser TextDecoder refuses SAB-backed views
    const msg = new TextDecoder().decode(txView().slice(0, Math.max(n, 0)));
    if (globalThis.process?.env?.KWASI) console.error(`[kerrno n=${n} msg=${JSON.stringify(msg)}]`);
    if (/does not exist|not found/.test(msg)) return E.noent;
    if (/exists|in use/.test(msg)) return E.exist;
    if (/permission|denied/.test(msg)) return E.acces;
    if (/is a directory/.test(msg)) return E.isdir;
    if (/not a directory/.test(msg)) return E.notdir;
    if (/not empty/.test(msg)) return E.notempty;
    if (/bad fd|fd out of range|not open/.test(msg)) return E.badf;
    if (/not a symlink|readlink not supported/.test(msg)) return E.inval;  // POSIX callers probe with readlink
    return E.io;
  }

  // ---- paths: dirfd-relative, resolved lexically to one absolute walk ----
  function resolve(dirfd, ptr, len) {
    const f = fds.get(dirfd);
    if (!f) return null;
    let p = new TextDecoder().decode(m8().subarray(ptr, ptr + len));
    if (!p.startsWith("/")) p = (f.path === "/" ? "" : f.path) + "/" + p;
    const out = [];
    for (const c of p.split("/")) {
      if (c === "" || c === ".") continue;
      if (c === "..") { out.pop(); continue; }
      out.push(c);
    }
    return "/" + out.join("/");
  }

  // ---- one 9P stat record out of tx -> the fields wasi wants ----
  function parse9(u8, off) {
    const v = new DataView(u8.buffer, u8.byteOffset, u8.byteLength);
    const size = v.getUint16(off, true);
    const qtype = v.getUint8(off + 8);
    const mode = v.getUint32(off + 21, true);
    const atime = v.getUint32(off + 25, true);
    const mtime = v.getUint32(off + 29, true);
    const length = v.getBigUint64(off + 33, true);
    const nlen = v.getUint16(off + 41, true);
    const name = new TextDecoder().decode(u8.subarray(off + 43, off + 43 + nlen));
    return { size: size + 2, qtype, mode, atime, mtime, length, name };
  }
  const ftype = (qtype, path) =>
    (qtype & 0x80) ? FT.dir : (qtype & 0x02) ? FT.symlink :
    path.startsWith("/dev/") ? FT.chardev : FT.regular;

  // filestat: dev u64, ino u64, filetype u8, nlink u64, size u64, atim/mtim/ctim u64 (ns)
  function putFilestat(ptr, st, path) {
    const d = dv();
    d.setBigUint64(ptr, 0n, true);
    d.setBigUint64(ptr + 8, 0n, true);
    d.setUint8(ptr + 16, ftype(st.qtype, path));
    d.setBigUint64(ptr + 24, 1n, true);
    d.setBigUint64(ptr + 32, st.length, true);
    d.setBigUint64(ptr + 40, BigInt(st.atime) * 1000000000n, true);
    d.setBigUint64(ptr + 48, BigInt(st.mtime) * 1000000000n, true);
    d.setBigUint64(ptr + 56, BigInt(st.mtime) * 1000000000n, true);
  }

  // ---- args, fetched once through the kernel's ARGS trap ----
  let argv = null;
  function args() {
    if (argv) return argv;
    const n = sysTx(T.ARGS, [], 0, TXSIZE, 0, 0, 0);
    const raw = new TextDecoder().decode(txView().slice(0, Math.max(n, 0)));
    argv = raw.length ? raw.split("\0").filter((s, i, a) => i < a.length - 1 || s !== "") : [];
    return argv;
  }

  function chunked(fn, total) {                     // TXSIZE at a time
    let done = 0;
    while (done < total) {
      const n = fn(done, Math.min(total - done, TXSIZE));
      if (n < 0) return done > 0 ? done : n;
      done += n;
      if (n === 0) break;
    }
    return done;
  }

  const imports = {
    // ---- process ----
    proc_exit: (code) => { throw new WasiExit(code); },
    proc_raise: () => E.nosys,
    sched_yield: () => E.success,
    random_get: (ptr, len) => {
      for (let o = 0; o < len; o += 65536)
        crypto.getRandomValues(m8().subarray(ptr + o, ptr + Math.min(len, o + 65536)));
      return E.success;
    },

    // ---- args and environ ----
    args_sizes_get: (argcP, sizeP) => {
      const a = args();
      dv().setUint32(argcP, a.length, true);
      dv().setUint32(sizeP, a.reduce((t, s) => t + s.length + 1, 0), true);
      return E.success;
    },
    args_get: (argvP, bufP) => {
      const a = args(); const enc = new TextEncoder();
      let o = bufP;
      for (let i = 0; i < a.length; i++) {
        dv().setUint32(argvP + 4 * i, o, true);
        const b = enc.encode(a[i]);
        m8().set(b, o); o += b.length; m8()[o++] = 0;
      }
      return E.success;
    },
    environ_sizes_get: (countP, sizeP) => {
      dv().setUint32(countP, 0, true); dv().setUint32(sizeP, 0, true);
      return E.success;
    },
    environ_get: () => E.success,

    // ---- clocks ----
    clock_time_get: (id, prec, outP) => {           // kernel writes u64 LE ns: wasi's exact shape
      sys(T.NSEC, outP, 0, 0, 0, 0);
      return E.success;
    },
    clock_res_get: (id, outP) => { dv().setBigUint64(outP, 1000000n, true); return E.success; },

    // ---- the preopen ----
    fd_prestat_get: (fd, outP) => {
      const f = fds.get(fd);
      if (!f?.preopen) return E.badf;
      dv().setUint8(outP, 0);                       // preopen_dir
      dv().setUint32(outP + 4, f.path.length, true);
      return E.success;
    },
    fd_prestat_dir_name: (fd, ptr, len) => {
      const f = fds.get(fd);
      if (!f?.preopen) return E.badf;
      m8().set(new TextEncoder().encode(f.path).subarray(0, len), ptr);
      return E.success;
    },

    // ---- fds ----
    fd_close: (fd) => {
      const f = fds.get(fd);
      if (!f) return E.badf;
      if (!f.preopen && f.kfd >= 0) sys(T.CLOSE, f.kfd, 0, 0, 0, 0);
      fds.delete(fd);
      return E.success;
    },
    fd_fdstat_get: (fd, outP) => {
      const f = fds.get(fd);
      if (!f) return E.badf;
      const d = dv();
      d.setUint8(outP, f.isdir ? FT.dir : fd <= 2 ? FT.chardev : FT.regular);
      d.setUint16(outP + 2, f.append ? 1 : 0, true);
      d.setBigUint64(outP + 8, ~0n, true);          // rights: not a capability shim's job
      d.setBigUint64(outP + 16, ~0n, true);
      return E.success;
    },
    fd_fdstat_set_flags: (fd, flags) => {
      const f = fds.get(fd);
      if (!f) return E.badf;
      f.append = !!(flags & 1);
      return E.success;
    },
    fd_filestat_get: (fd, outP) => {
      const f = fds.get(fd);
      if (!f) return E.badf;
      if (f.preopen) {
        const n = sysTx(T.STAT, [f.path], 0, NOCOPY, 4096, 0, 0);
        if (n < 0) return kerrno();
        putFilestat(outP, parse9(txView().slice(0, n), 0), f.path);
        return E.success;
      }
      const n = sysTx(T.FSTAT, [], f.kfd, NOCOPY, 4096, 0, 0);
      if (n < 0) return kerrno();
      putFilestat(outP, parse9(txView().slice(0, n), 0), f.path);
      return E.success;
    },
    fd_seek: (fd, off, whence, outP) => {
      const f = fds.get(fd);
      if (!f || f.kfd < 0) return E.badf;
      if (f.kfd <= 2) return E.spipe;
      const o = BigInt.asIntN(64, off);
      const r = sys(T.SEEK, f.kfd, Number(o & 0xffffffffn) | 0, Number(o >> 32n) | 0, whence, 0);
      if (r < 0) return kerrno();
      dv().setBigUint64(outP, BigInt(r >>> 0), true);
      return E.success;
    },
    fd_tell: (fd, outP) => imports.fd_seek(fd, 0n, 1, outP),
    fd_renumber: (from, to) => {
      const f = fds.get(from);
      if (!f) return E.badf;
      imports.fd_close(to);
      fds.set(to, f); fds.delete(from);
      return E.success;
    },
    fd_filestat_set_size: () => E.notsup,           // ftruncate: no wstat-length path yet
    fd_filestat_set_times: () => E.success,         // utimes: accepted, not recorded
    fd_sync: () => E.success,
    fd_datasync: () => E.success,
    fd_advise: () => E.success,
    fd_allocate: () => E.notsup,

    // ---- read and write: iovs against the mailbox, TXSIZE chunks ----
    fd_read: (fd, iovsP, niovs, outP) => {
      const f = fds.get(fd);
      if (!f || f.kfd < 0) return E.badf;
      let total = 0;
      for (let i = 0; i < niovs; i++) {
        const ptr = dv().getUint32(iovsP + 8 * i, true);
        const len = dv().getUint32(iovsP + 8 * i + 4, true);
        const n = chunked((off, take) => sys(T.PREAD, f.kfd, ptr + off, take, -1, -1), len);
        if (n < 0) return kerrno();
        total += n;
        if (n < len) break;
      }
      dv().setUint32(outP, total, true);
      return E.success;
    },
    fd_write: (fd, iovsP, niovs, outP) => {
      const f = fds.get(fd);
      if (!f || f.kfd < 0) return E.badf;
      if (f.append) sys(T.SEEK, f.kfd, 0, 0, 2, 0);
      let total = 0;
      for (let i = 0; i < niovs; i++) {
        const ptr = dv().getUint32(iovsP + 8 * i, true);
        const len = dv().getUint32(iovsP + 8 * i + 4, true);
        const n = chunked((off, take) => sys(T.PWRITE, f.kfd, ptr + off, take, -1, -1), len);
        if (n < 0) return kerrno();
        total += n;
        if (n < len) break;
      }
      dv().setUint32(outP, total, true);
      return E.success;
    },
    fd_pread: (fd, iovsP, niovs, off, outP) => {
      const f = fds.get(fd);
      if (!f || f.kfd < 0) return E.badf;
      let pos = BigInt.asIntN(64, off), total = 0;
      for (let i = 0; i < niovs; i++) {
        const ptr = dv().getUint32(iovsP + 8 * i, true);
        const len = dv().getUint32(iovsP + 8 * i + 4, true);
        const n = chunked((o, take) => sys(T.PREAD, f.kfd, ptr + o,
          take, Number((pos + BigInt(o)) & 0xffffffffn) | 0, Number((pos + BigInt(o)) >> 32n) | 0), len);
        if (n < 0) return kerrno();
        total += n; pos += BigInt(n);
        if (n < len) break;
      }
      dv().setUint32(outP, total, true);
      return E.success;
    },
    fd_pwrite: (fd, iovsP, niovs, off, outP) => {
      const f = fds.get(fd);
      if (!f || f.kfd < 0) return E.badf;
      let pos = BigInt.asIntN(64, off), total = 0;
      for (let i = 0; i < niovs; i++) {
        const ptr = dv().getUint32(iovsP + 8 * i, true);
        const len = dv().getUint32(iovsP + 8 * i + 4, true);
        const n = chunked((o, take) => sys(T.PWRITE, f.kfd, ptr + o,
          take, Number((pos + BigInt(o)) & 0xffffffffn) | 0, Number((pos + BigInt(o)) >> 32n) | 0), len);
        if (n < 0) return kerrno();
        total += n; pos += BigInt(n);
        if (n < len) break;
      }
      dv().setUint32(outP, total, true);
      return E.success;
    },

    // ---- directories: 9P records in, wasi dirents out. The directory is
    // snapshotted whole on the first read (one continuous enumeration at
    // the offsets the kernel itself returned) and served by INDEX cookie —
    // byte-offset cookies across separate enumerations proved fragile, and
    // a directory can change between reads (pyc writes, measured). ----
    fd_readdir: (fd, bufP, buflen, cookie, outP) => {
      const f = fds.get(fd);
      if (!f || !f.isdir) return E.notdir;
      let kfd = f.kfd;
      if (kfd < 0) {                                 // the preopen opens lazily
        kfd = sysTx(T.OPEN, [f.path], 0, OREAD, 0, 0, 0);
        if (kfd < 0) return kerrno();
        f.kfd = kfd;
      }
      let idx = Number(BigInt.asUintN(64, cookie));
      if (idx === 0 || !f.dirents) {
        f.dirents = [];
        let pos = 0;
        for (;;) {
          const n = sysTx(T.PREAD, [], kfd, NOCOPY, TXSIZE, pos | 0, 0);
          if (globalThis.process?.env?.KWASI) console.error(`[snap pos=${pos} n=${n}]`);
          if (n < 0) return kerrno();
          if (n === 0) break;
          const chunk = txView().slice(0, n);
          let off = 0;
          while (off + 2 <= n) {
            const st = parse9(chunk, off);
            f.dirents.push({ name: st.name, qtype: st.qtype });
            off += st.size;
          }
          pos += n;
        }
      }
      const d = dv();
      const enc = new TextEncoder();
      let used = 0;
      for (; idx < f.dirents.length; idx++) {
        const e = f.dirents[idx];
        const name = enc.encode(e.name);
        const need = 24 + name.length;
        const rec = new Uint8Array(need);
        const rv = new DataView(rec.buffer);
        rv.setBigUint64(0, BigInt(idx + 1), true);   // d_next: index cookie
        rv.setBigUint64(8, 0n, true);
        rv.setUint32(16, name.length, true);
        rv.setUint8(20, ftype(e.qtype, "/"));
        rec.set(name, 24);
        // preview1's contract: bufused < buflen means END OF DIRECTORY, so
        // a dirent that doesn't fit is written TRUNCATED to fill the buffer
        // exactly — the caller resumes from the last whole entry's cookie
        // (measured: breaking early read as exhaustion at entry ~118/185,
        // and importlib's cached scan then swore /lib had no re)
        const take = Math.min(need, buflen - used);
        m8().set(rec.subarray(0, take), bufP + used);
        used += take;
        if (take < need) break;
      }
      d.setUint32(outP, used, true);
      return E.success;
    },

    // ---- paths ----
    path_open: (dirfd, dirflags, ptr, len, oflags, rBase, rInherit, fdflags, outP) => {
      const path = resolve(dirfd, ptr, len);
      if (path === null) return E.badf;
      const rights = BigInt.asUintN(64, rBase);
      const wantW = !!(rights & (1n << 6n));         // fd_write
      const wantR = !!(rights & (1n << 1n)) || !wantW;
      let mode = wantW && wantR ? ORDWR : wantW ? OWRITE : OREAD;
      if (oflags & 8) mode |= OTRUNC;                // O_TRUNC
      let kfd;
      if (oflags & 1) {                              // O_CREAT
        if (oflags & 4) {                            // O_EXCL: create(2) truncates, so probe
          if (sysTx(T.STAT, [path], 0, NOCOPY, 4096, 0, 0) >= 0) return E.exist;
        }
        kfd = sysTx(T.CREATE, [path], 0, mode, 0o666, 0, 0);
      } else {
        kfd = sysTx(T.OPEN, [path], 0, mode, 0, 0, 0);
      }
      if (kfd < 0) return kerrno();
      if (globalThis.process?.env?.KWASI) console.error(`[open ${path}]`);
      let isdir = false;
      const sn = sysTx(T.FSTAT, [], kfd, NOCOPY, 4096, 0, 0);
      if (sn >= 0) isdir = !!(parse9(txView().slice(0, sn), 0).qtype & 0x80);
      if ((oflags & 2) && !isdir) {                  // O_DIRECTORY
        sys(T.CLOSE, kfd, 0, 0, 0, 0);
        return E.notdir;
      }
      dv().setUint32(outP, fdAlloc({ kfd, path, isdir, append: !!(fdflags & 1) }), true);
      return E.success;
    },
    path_filestat_get: (dirfd, flags, ptr, len, outP) => {
      const path = resolve(dirfd, ptr, len);
      if (path === null) return E.badf;
      const n = sysTx(T.STAT, [path], 0, NOCOPY, 4096, 0, 0);
      if (n < 0) return kerrno();
      putFilestat(outP, parse9(txView().slice(0, n), 0), path);
      return E.success;
    },
    path_filestat_set_times: () => E.notsup,
    path_create_directory: (dirfd, ptr, len) => {
      const path = resolve(dirfd, ptr, len);
      if (path === null) return E.badf;
      const kfd = sysTx(T.CREATE, [path], 0, OREAD, (DMDIR | 0o777) | 0, 0, 0);
      if (kfd < 0) return kerrno();
      sys(T.CLOSE, kfd, 0, 0, 0, 0);
      return E.success;
    },
    path_unlink_file: (dirfd, ptr, len) => {
      const path = resolve(dirfd, ptr, len);
      if (path === null) return E.badf;
      return sysTx(T.REMOVE, [path], 0, 0, 0, 0, 0) < 0 ? kerrno() : E.success;
    },
    path_remove_directory: (dirfd, ptr, len) => imports.path_unlink_file(dirfd, ptr, len),
    path_rename: (dirfd, ptr, len, ndirfd, nptr, nlen) => {
      const from = resolve(dirfd, ptr, len), to = resolve(ndirfd, nptr, nlen);
      if (from === null || to === null) return E.badf;
      // V10's rule, alive here: rename is link + unlink in userland
      if (sysTx(T.LINK, [from, to], 0, 0, 0, 0, 0) < 0) return kerrno();
      if (sysTx(T.REMOVE, [from], 0, 0, 0, 0, 0) < 0) return kerrno();
      return E.success;
    },
    path_link: (odirfd, oflags, optr, olen, ndirfd, nptr, nlen) => {
      const from = resolve(odirfd, optr, olen), to = resolve(ndirfd, nptr, nlen);
      if (from === null || to === null) return E.badf;
      return sysTx(T.LINK, [from, to], 0, 0, 0, 0, 0) < 0 ? kerrno() : E.success;
    },
    path_symlink: (optr, olen, dirfd, nptr, nlen) => {
      const target = new TextDecoder().decode(m8().subarray(optr, optr + olen));
      const nu = resolve(dirfd, nptr, nlen);
      if (nu === null) return E.badf;
      return sysTx(T.SYMLINK, [target, nu], 0, 0, 0, 0, 0) < 0 ? kerrno() : E.success;
    },
    path_readlink: (dirfd, ptr, len, bufP, buflen, outP) => {
      const path = resolve(dirfd, ptr, len);
      if (path === null) return E.badf;
      const n = sysTx(T.READLINK, [path], 0, NOCOPY, Math.min(buflen, 4096), 0, 0);
      if (n < 0) return kerrno();
      m8().set(txView().subarray(0, Math.min(n, buflen)), bufP);
      dv().setUint32(outP, Math.min(n, buflen), true);
      return E.success;
    },

    // ---- poll: clock subscriptions sleep on the kernel; fds are ready ----
    poll_oneoff: (inP, outP, nsub, outN) => {
      const d = dv();
      let minMs = -1, nfd = 0;
      const subs = [];
      for (let i = 0; i < nsub; i++) {
        const p = inP + 48 * i;
        const userdata = d.getBigUint64(p, true);
        const tag = d.getUint8(p + 8);
        if (tag === 0) {
          let ns = d.getBigUint64(p + 24, true);
          const flags = d.getUint16(p + 40, true);
          if (flags & 1) {                           // abstime: subtract now
            sys(T.NSEC, outP, 0, 0, 0, 0);           // borrow outP as scratch
            const now = d.getBigUint64(outP, true);
            ns = ns > now ? ns - now : 0n;
          }
          const ms = Number(ns / 1000000n);
          if (minMs < 0 || ms < minMs) minMs = ms;
          subs.push({ userdata, type: 0 });
        } else {
          nfd++;
          subs.push({ userdata, type: tag });
        }
      }
      if (nfd === 0 && minMs > 0) sys(T.SLEEP, minMs, 0, 0, 0, 0);
      let o = outP, count = 0;
      for (const s of subs) {
        if (s.type !== 0 && nfd === 0) continue;
        d.setBigUint64(o, s.userdata, true);
        d.setUint16(o + 8, 0, true);
        d.setUint8(o + 10, s.type);
        d.setBigUint64(o + 16, s.type === 0 ? 0n : 1n, true);  // nbytes for fds
        d.setUint16(o + 24, 0, true);
        o += 32; count++;
      }
      d.setUint32(outN, count, true);
      return E.success;
    },

    // ---- sockets: another edition's work ----
    sock_accept: () => E.nosys,
    sock_recv: () => E.nosys,
    sock_send: () => E.nosys,
    sock_shutdown: () => E.nosys,
  };

  return { imports };
}
