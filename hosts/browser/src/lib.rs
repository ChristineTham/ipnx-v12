// The browser embedding's kernel half: the Rust core compiled to wasm,
// wrapped in a C ABI the JS shim (demo/supervisor/rustkern.mjs) drives.
// Structurally parallel to hosts/macos: entries mirror Ev, the drain
// mirrors run_effects — but here the mach layer is JavaScript (Workers
// as processes, the SAB mailbox), so state crosses as one length-
// prefixed binary blob per drain. Little-endian throughout; strings are
// UTF-8 with u16/u32 length prefixes; u64 crosses as lo,hi u32 pairs.
use kernel::{Cont, Effect, HostEnt, HostOp, HostReply, KAction, KReply, Kernel, Seed};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

#[link(wasm_import_module = "env")]
extern "C" {
    fn bh_log(ptr: *const u8, len: usize);
}

fn log(s: &str) {
    unsafe { bh_log(s.as_ptr(), s.len()) }
}

struct World {
    kern: Kernel,
    pending: Vec<(u32, Receiver<KReply>)>,
    images: HashMap<usize, u32>, // Arc ptr -> image id (JS caches the Module)
    keep: Vec<Arc<Vec<u8>>>,
    next_image: u32,
    drain_buf: Vec<u8>,
}

thread_local! {
    static WORLD: RefCell<Option<World>> = RefCell::new(None);
}

fn with<R>(f: impl FnOnce(&mut World) -> R) -> R {
    WORLD.with(|w| f(w.borrow_mut().as_mut().expect("bh_init first")))
}

// ---- allocation for JS-written inputs ----
#[no_mangle]
pub extern "C" fn bh_alloc(n: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(n.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

#[no_mangle]
pub extern "C" fn bh_free(p: *mut u8, n: usize) {
    unsafe { drop(Vec::from_raw_parts(p, 0, n.max(1))) }
}

unsafe fn bytes<'a>(p: *const u8, n: usize) -> &'a [u8] {
    if n == 0 { &[] } else { std::slice::from_raw_parts(p, n) }
}

// ---- readers ----
struct R<'a> {
    b: &'a [u8],
    o: usize,
}
impl<'a> R<'a> {
    fn u8(&mut self) -> u8 { let v = self.b[self.o]; self.o += 1; v }
    fn u16(&mut self) -> u16 {
        let v = u16::from_le_bytes(self.b[self.o..self.o + 2].try_into().unwrap());
        self.o += 2; v
    }
    fn u32(&mut self) -> u32 {
        let v = u32::from_le_bytes(self.b[self.o..self.o + 4].try_into().unwrap());
        self.o += 4; v
    }
    fn u64(&mut self) -> u64 {
        let lo = self.u32() as u64; let hi = self.u32() as u64; lo | (hi << 32)
    }
    fn take(&mut self, n: usize) -> &'a [u8] {
        let v = &self.b[self.o..self.o + n]; self.o += n; v
    }
    fn str16(&mut self) -> String {
        let n = self.u16() as usize;
        String::from_utf8_lossy(self.take(n)).into_owned()
    }
}

fn parse_seed(r: &mut R, name: String) -> Seed {
    let dir = r.u8() != 0;
    if dir {
        let n = r.u32();
        let mut kids = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let nm = r.str16();
            kids.push(parse_seed(r, nm));
        }
        Seed { name, dir: true, kids, data: Vec::new() }
    } else {
        let n = r.u32() as usize;
        Seed { name, dir: false, kids: Vec::new(), data: r.take(n).to_vec() }
    }
}

// ---- writers ----
fn w16(v: &mut Vec<u8>, x: u16) { v.extend_from_slice(&x.to_le_bytes()) }
fn w32(v: &mut Vec<u8>, x: u32) { v.extend_from_slice(&x.to_le_bytes()) }
fn wi32(v: &mut Vec<u8>, x: i32) { v.extend_from_slice(&x.to_le_bytes()) }
fn w64(v: &mut Vec<u8>, x: u64) { w32(v, x as u32); w32(v, (x >> 32) as u32) }
fn wstr16(v: &mut Vec<u8>, s: &str) { w16(v, s.len() as u16); v.extend_from_slice(s.as_bytes()) }
fn wbytes32(v: &mut Vec<u8>, b: &[u8]) { w32(v, b.len() as u32); v.extend_from_slice(b) }

// ---- lifecycle ----
#[no_mangle]
pub extern "C" fn bh_init(seed_ptr: *const u8, seed_len: usize, interactive: u32,
                          verbose: u32, hostfs: u32) -> u32 {
    std::panic::set_hook(Box::new(|i| log(&format!("KERNEL PANIC: {}", i))));
    let blob = unsafe { bytes(seed_ptr, seed_len) };
    let mut r = R { b: blob, o: 0 };
    let seed = parse_seed(&mut r, "/".into());
    let mut kern = Kernel::new(&seed, "kitty");
    kern.interactive = interactive != 0;
    kern.verbose = verbose != 0;
    if hostfs != 0 {
        kern.set_hostfs(std::path::PathBuf::new(), false); // '#Z': the shim serves
    }
    WORLD.with(|w| {
        *w.borrow_mut() = Some(World {
            kern, pending: Vec::new(), images: HashMap::new(), keep: Vec::new(),
            next_image: 1, drain_buf: Vec::new(),
        })
    });
    1
}

#[no_mangle]
pub extern "C" fn bh_clock(ms: f64) {
    kernel::clock_set(ms as u64);
}

#[no_mangle]
pub extern "C" fn bh_boot(argv_ptr: *const u8, argv_len: usize) -> u32 {
    let raw = unsafe { bytes(argv_ptr, argv_len) };
    let argv: Vec<String> = raw.split(|b| *b == 0).filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned()).collect();
    with(|w| w.kern.boot(argv).map(|_| 1).unwrap_or(0))
}

#[no_mangle]
pub extern "C" fn bh_canvas_caps(p: *const u8, n: usize) {
    let s = String::from_utf8_lossy(unsafe { bytes(p, n) }).into_owned();
    with(|w| w.kern.canvas_caps_set(&s));
}

// ---- Ev mirror ----
#[no_mangle]
pub extern "C" fn bh_syscall(worker_pid: u32, trap: i32, a0: i32, a1: i32, a2: i32,
                             a3: i32, a4: i32, tx_ptr: *const u8, tx_len: usize) {
    let tx = unsafe { bytes(tx_ptr, tx_len) }.to_vec();
    with(|w| {
        let (s, r) = std::sync::mpsc::channel();
        w.kern.syscall(worker_pid as kernel::Pid, trap, [a0, a1, a2, a3, a4], tx, s);
        w.pending.push((worker_pid, r));
    });
}

#[no_mangle]
pub extern "C" fn bh_started(pid: u32, asy: u32) {
    with(|w| w.kern.set_asyncified(pid as kernel::Pid, asy != 0));
}

#[no_mangle]
pub extern "C" fn bh_asyfork(parent: u32, child: u32, snap_ptr: *const u8,
                             snap_len: usize, data_ptr: u32, sp: u32) {
    let cont = cont_pack(unsafe { bytes(snap_ptr, snap_len) }, data_ptr, sp);
    with(|w| w.kern.asyfork(parent as kernel::Pid, child as kernel::Pid, cont));
}

// The continuation THIS substrate mints (kernel::Cont is opaque bytes): the
// guest's whole linear memory, with asyncify's data pointer and the stack
// pointer appended. Nothing outside these two functions knows its shape —
// below them the worker protocol is unchanged.
fn cont_pack(mem: &[u8], data_ptr: u32, sp: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity(mem.len() + 8);
    v.extend_from_slice(mem);
    v.extend_from_slice(&data_ptr.to_le_bytes());
    v.extend_from_slice(&sp.to_le_bytes());
    v
}
fn cont_unpack(c: &[u8]) -> (&[u8], u32, u32) {
    let n = c.len() - 8;
    let data_ptr = u32::from_le_bytes(c[n..n + 4].try_into().unwrap());
    let sp = u32::from_le_bytes(c[n + 4..].try_into().unwrap());
    (&c[..n], data_ptr, sp)
}

#[no_mangle]
pub extern "C" fn bh_died(pid: u32, msg_ptr: *const u8, msg_len: usize) {
    let msg = String::from_utf8_lossy(unsafe { bytes(msg_ptr, msg_len) }).into_owned();
    with(|w| w.kern.proc_died(pid as kernel::Pid, &msg));
}

#[no_mangle]
pub extern "C" fn bh_timer(token: f64) {
    with(|w| w.kern.timer_fired(token as u64));
}

#[no_mangle]
pub extern "C" fn bh_cons_feed(p: *const u8, n: usize) {
    let b = unsafe { bytes(p, n) }.to_vec();
    with(|w| w.kern.cons_feed(&b));
}

#[no_mangle]
pub extern "C" fn bh_cons_end() {
    with(|w| w.kern.cons_end());
}

#[no_mangle]
pub extern "C" fn bh_win_key(wid: u32, p: *const u8, n: usize) {
    let b = unsafe { bytes(p, n) }.to_vec();
    with(|w| w.kern.win_key(wid, &b));
}

#[no_mangle]
pub extern "C" fn bh_win_mouse(wid: u32, x: i32, y: i32, b: i32) {
    with(|w| w.kern.win_mouse(wid, x, y, b));
}

#[no_mangle]
pub extern "C" fn bh_win_close(wid: u32) {
    with(|w| w.kern.win_close(wid));
}

#[no_mangle]
pub extern "C" fn bh_win_tick() {
    with(|w| w.kern.win_tick());
}

#[no_mangle]
pub extern "C" fn bh_win_ack(wid: u32) {
    with(|w| w.kern.win_ack(wid));
}

/// the surface opens a file over the namespace — answered as ReadDone
#[no_mangle]
pub extern "C" fn bh_read(token: f64, p: *const u8, n: usize) {
    let path = String::from_utf8_lossy(unsafe { bytes(p, n) }).into_owned();
    with(|w| w.kern.read_path(token, &path));
}

/// Put: the surface streams the edited file back
#[no_mangle]
pub extern "C" fn bh_write(token: f64, pp: *const u8, pn: usize, dp: *const u8, dn: usize) {
    let path = String::from_utf8_lossy(unsafe { bytes(pp, pn) }).into_owned();
    let data = unsafe { bytes(dp, dn) }.to_vec();
    with(|w| w.kern.write_path(token, &path, data));
}

/// the surface speaks back into /dev/window/<type>/<n>/events
#[no_mangle]
pub extern "C" fn bh_win_event(wid: u32, p: *const u8, n: usize) {
    let line = String::from_utf8_lossy(unsafe { bytes(p, n) }).into_owned();
    with(|w| w.kern.win_event(wid, &line));
}

#[no_mangle]
pub extern "C" fn bh_canvas_event(wid: u32, p: *const u8, n: usize) {
    let s = String::from_utf8_lossy(unsafe { bytes(p, n) }).into_owned();
    with(|w| w.kern.canvas_event(wid, &s));
}

#[no_mangle]
pub extern "C" fn bh_snarf_done(has: u32, p: *const u8, n: usize) {
    let t = if has != 0 {
        Some(String::from_utf8_lossy(unsafe { bytes(p, n) }).into_owned())
    } else {
        None
    };
    with(|w| w.kern.snarf_done(t));
}

#[no_mangle]
pub extern "C" fn bh_snarf_put(p: *const u8, n: usize) {
    let t = String::from_utf8_lossy(unsafe { bytes(p, n) }).into_owned();
    with(|w| w.kern.snarf_put(&t));
}

#[no_mangle]
pub extern "C" fn bh_graft(p: *const u8, n: usize) {
    let blob = unsafe { bytes(p, n) };
    let mut r = R { b: blob, o: 0 };
    let seed = parse_seed(&mut r, "/".into());
    with(|w| w.kern.graft(&seed));
}

// hostop_done: payload for ok is a serialized HostReply; for err the message
#[no_mangle]
pub extern "C" fn bh_hostop_done(tag: f64, ok: u32, p: *const u8, n: usize) {
    let blob = unsafe { bytes(p, n) };
    let r = if ok == 0 {
        Err(String::from_utf8_lossy(blob).into_owned())
    } else {
        let mut r = R { b: blob, o: 0 };
        Ok(match r.u8() {
            0 => HostReply::Missing,
            1 => {
                let dir = r.u8() != 0;
                let len = r.u64();
                let mtime = r.u32();
                let ino = r.u64();
                let mode = r.u32();
                HostReply::Meta { dir, len, mtime, ino, mode }
            }
            2 => {
                let n2 = r.u32() as usize;
                HostReply::Bytes(r.take(n2).to_vec())
            }
            3 => {
                let cnt = r.u32();
                let mut ents = Vec::with_capacity(cnt as usize);
                for _ in 0..cnt {
                    let name = r.str16();
                    let dir = r.u8() != 0;
                    let len = r.u64();
                    let mtime = r.u32();
                    let ino = r.u64();
                    let mode = r.u32();
                    ents.push(HostEnt { name, dir, len, mtime, ino, mode });
                }
                HostReply::Entries(ents)
            }
            _ => HostReply::Unit,
        })
    };
    with(|w| w.kern.hostop_done(tag as u64, r));
}

// ---- the drain: completed syscalls + effects, one blob ----
fn image_ref(w: &mut World, v: &mut Vec<u8>, image: &Arc<Vec<u8>>) {
    let key = Arc::as_ptr(image) as usize;
    if let Some(id) = w.images.get(&key) {
        w32(v, *id);
        w32(v, 0);
    } else {
        let id = w.next_image;
        w.next_image += 1;
        w.images.insert(key, id);
        w.keep.push(image.clone());
        w32(v, id);
        wbytes32(v, image);
    }
}

#[no_mangle]
pub extern "C" fn bh_drain() -> *const u8 {
    with(|w| {
        let mut v = Vec::new();
        w32(&mut v, 0); // length, patched at the end
        // completed syscalls
        let mut done = Vec::new();
        let mut still = Vec::new();
        for (pid, rx) in w.pending.drain(..) {
            match rx.try_recv() {
                Ok(r) => done.push((pid, r)),
                Err(std::sync::mpsc::TryRecvError::Empty) => still.push((pid, rx)),
                Err(_) => {} // sender dropped without reply: proc killed mid-call
            }
        }
        w.pending = still;
        w32(&mut v, done.len() as u32);
        for (pid, r) in done {
            w32(&mut v, pid);
            wi32(&mut v, r.ret);
            wi32(&mut v, r.aux);
            v.push(match r.action {
                KAction::None => 0,
                KAction::ForkResume => 1,
                KAction::ExecSelf => 2,
                KAction::Retire => 3,
                KAction::Die => 4,
            });
            v.push(r.note_pending as u8);
            wbytes32(&mut v, &r.data);
            match r.load {
                Some((image, _argv)) => {
                    v.push(1);
                    image_ref(w, &mut v, &image);
                }
                None => v.push(0),
            }
        }
        // effects
        let effects = w.kern.take_effects();
        let mut ne = 0u32;
        let mut ev = Vec::new();
        for e in effects {
            ne += 1;
            match e {
                Effect::Spawn { pid, image, argv: _, cont } => {
                    ev.push(1);
                    w32(&mut ev, pid as u32);
                    image_ref(w, &mut ev, &image);
                    match cont {
                        Some(Cont(c)) => {
                            let (snap, data_ptr, sp) = cont_unpack(&c);
                            ev.push(1);
                            wbytes32(&mut ev, snap);
                            w32(&mut ev, data_ptr);
                            w32(&mut ev, sp);
                        }
                        None => ev.push(0),
                    }
                }
                Effect::ConsWrite(b) => {
                    ev.push(2);
                    wbytes32(&mut ev, &b);
                }
                Effect::Timer { ms, token } => {
                    ev.push(3);
                    w64(&mut ev, ms);
                    w64(&mut ev, token);
                }
                Effect::Shutdown(code) => {
                    ev.push(4);
                    wi32(&mut ev, code);
                }
                Effect::WinUpdate { wid, label, x, y, w: ww, h, rgba } => {
                    ev.push(5);
                    w32(&mut ev, wid);
                    wstr16(&mut ev, &label);
                    wi32(&mut ev, x);
                    wi32(&mut ev, y);
                    wi32(&mut ev, ww);
                    wi32(&mut ev, h);
                    wbytes32(&mut ev, &rgba);
                }
                Effect::WinGone { wid } => {
                    ev.push(6);
                    w32(&mut ev, wid);
                }
                Effect::WinText { wid, bytes } => {
                    ev.push(7);
                    w32(&mut ev, wid);
                    wbytes32(&mut ev, &bytes);
                }
                Effect::WinCanvas { wid, label, x, y, w: ww, h, snap } => {
                    ev.push(8);
                    w32(&mut ev, wid);
                    wstr16(&mut ev, &label);
                    wi32(&mut ev, x);
                    wi32(&mut ev, y);
                    wi32(&mut ev, ww);
                    wi32(&mut ev, h);
                    w32(&mut ev, snap.len() as u32);
                    for nd in snap {
                        w32(&mut ev, nd.id);
                        ev.push(nd.kind);
                        w16(&mut ev, nd.attrs.len() as u16);
                        for (k2, v2) in &nd.attrs {
                            wstr16(&mut ev, k2);
                            wstr16(&mut ev, v2);
                        }
                        wbytes32(&mut ev, &nd.data);
                    }
                }
                Effect::ReadDone { token, ok, data } => {
                    ev.push(14);
                    ev.extend_from_slice(&token.to_le_bytes());
                    ev.push(if ok { 1 } else { 0 });
                    wbytes32(&mut ev, &data);
                }
                Effect::WriteDone { token, ok, n } => {
                    ev.push(15);
                    ev.extend_from_slice(&token.to_le_bytes());
                    ev.push(if ok { 1 } else { 0 });
                    w32(&mut ev, n);
                }
                Effect::WinChrome { wid, wtype, content, toolbar, tag } => {
                    ev.push(13);
                    w32(&mut ev, wid);
                    wstr16(&mut ev, &wtype);
                    wstr16(&mut ev, &content);
                    wstr16(&mut ev, &toolbar);
                    wstr16(&mut ev, &tag);
                }
                Effect::SnarfSet { text } => {
                    ev.push(10);
                    wbytes32(&mut ev, text.as_bytes());
                }
                Effect::SnarfGet => ev.push(11),
                Effect::Host { tag, op } => {
                    ev.push(12);
                    w64(&mut ev, tag);
                    match op {
                        HostOp::Meta { path } => { ev.push(1); wstr16(&mut ev, &path) }
                        HostOp::Read { path, off, n } => {
                            ev.push(2);
                            wstr16(&mut ev, &path);
                            w64(&mut ev, off);
                            w32(&mut ev, n as u32);
                        }
                        HostOp::Write { path, off, data } => {
                            ev.push(3);
                            wstr16(&mut ev, &path);
                            w64(&mut ev, off);
                            wbytes32(&mut ev, &data);
                        }
                        HostOp::Create { path, dir, perm } => {
                            ev.push(4);
                            wstr16(&mut ev, &path);
                            ev.push(dir as u8);
                            w32(&mut ev, perm);
                        }
                        HostOp::Remove { path } => { ev.push(5); wstr16(&mut ev, &path) }
                        HostOp::Trunc { path } => { ev.push(6); wstr16(&mut ev, &path) }
                        HostOp::ReadDir { path } => { ev.push(7); wstr16(&mut ev, &path) }
                    }
                }
            }
        }
        w32(&mut v, ne);
        v.extend_from_slice(&ev);
        let len = (v.len() - 4) as u32;
        v[0..4].copy_from_slice(&len.to_le_bytes());
        w.drain_buf = v;
        w.drain_buf.as_ptr()
    })
}
