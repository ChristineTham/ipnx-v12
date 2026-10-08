//! **The machine, in a browser**: each process a Web Worker of its own, and
//! the kernel in another.
//!
//! It is `hosts/ipnx/src/machine.rs` on a different processor, and where
//! the two can do a thing the same way they do. What differs is forced by
//! the browser: a worker's code cannot be suspended from outside it, so a
//! process here is not a fiber the kernel polls but a worker that **runs on
//! its own and stops only to call**. Each call is a message in a mailbox in
//! shared memory — its number and its words, as the PC's trap leaves them
//! in `AX` and above the user stack pointer (`pc/trap.c:673`–`:723`) — and
//! the worker waits on the mailbox until the kernel answers. This file is
//! the kernel's side of that mailbox: it turns what a process said into a
//! [`Call`], and the answer back, exactly as the terminal machine's import
//! table does; the process's side is `www/proc.js`.
//!
//! **What the browser changes, said where it changes it:**
//!
//!   * **The clock.** The terminal machine's interrupt lands in the
//!     process's own code; nothing can land in a worker's. The kernel waits
//!     for a process at most one tick (`1000/HZ` ms), and when the tick
//!     comes it takes the interrupt there — `timerintr`, then `notify` — as
//!     `clockintr` does. A process the clock takes the processor from goes
//!     on running in its worker, as a process on another processor would,
//!     until it next calls: the kernel answers it when the scheduler enters
//!     it again. A process `procctl` stops stops at its next call, for the
//!     same reason.
//!   * **`RFMEM`.** Processes sharing a memory share one stack region, as on
//!     the terminal machine (`segment.c:175`), so two of them never run at
//!     once: one is entered only when the one whose stack is in the region
//!     is waiting on the kernel ([`Web::occupy`]).
//!   * **An image imports its memory.** The kernel reaches a process's
//!     memory through the object it made the process with, so an image
//!     that makes its own cannot be run here. Every image `mk.sh` builds
//!     imports one (`--import-memory`).

use crate::js::{self, ev, rep};
use crate::module::memimport;
use ipnx_kernel::machine::{Left, Machine, NoteAt, Notify, Syscalls, Tod, Ureg, MAXSYSARG};
use ipnx_kernel::proc::{noted, rf, NoteFlag, ERRMAX, HZ};
use ipnx_kernel::sysno::*;
use ipnx_kernel::{Call, Pid, Ret};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// `Ebadexec` (`port/error.h:34`).
const EBADEXEC: &str = "exec header invalid";

/// **The `Tos`** (`sys/include/tos.h`): where the pid is in it. The process
/// worker puts the `Tos` at the top of the stack (`www/proc.js`, `settos`).
const TOSPID: u32 = 48;

/// The mailbox word where a process leaves the lowest address of its stack
/// region in use, each time it says something — what the terminal machine's
/// `publish` keeps in `Proc::low`.
const LOW: u32 = 6;

/// A compiled image: the page's module, and the memory it imports.
#[derive(Clone, Copy, Debug)]
struct Image {
    module: i32,
    initial: u32,
    maximum: u32,
}

/// What the machine knows of a process — the counterpart of the terminal
/// machine's `Fiber`, which this machine cannot have: the process's stack
/// is in its worker, not here.
enum St {
    /// [`Machine::touser`] gave it an image, and no worker runs it yet.
    Image(Image, Vec<String>),
    /// Its worker runs, and the next thing is what it says.
    Running,
    /// **In a call that left the processor part way** ([`Ret::Sched`]):
    /// entering it again goes back into the call through `resume`.
    Call { no: u32, w: [i64; MAXSYSARG] },
    /// A fork's parent that has wound its stack back: `sysrfork`'s
    /// *"sched()"*. Entering it again finishes the call.
    Yielded,
    /// The kernel has made it, and readied it — and its parent has not yet
    /// unwound the stack it will run. Entering it puts it back on the queue.
    Forking,
    /// Its parent's stack has unwound and its memory is made: entering it
    /// starts the worker that winds the stack back in.
    Forked,
    /// Its parent could not unwind for it: it never runs.
    Unforked,
}

/// A memory shared by `RFMEM`: whose stack is in its stack region now, and
/// every other sharer's, kept — `hosts/ipnx/src/machine.rs`'s `Sharing`.
struct Sharing {
    top: u32,
    occupant: Option<Pid>,
    stacks: HashMap<Pid, (u32, Vec<u8>)>,
}

pub struct Web {
    st: RefCell<HashMap<Pid, St>>,
    /// Every image compiled so far, by its content — as the terminal
    /// machine keeps them, and for its reason: the same few images run
    /// over and over.
    modules: RefCell<HashMap<Box<[u8]>, Image>>,
    sharing: RefCell<HashMap<Pid, Rc<RefCell<Sharing>>>>,
    /// **The image an `exec` committed**, until the call that made it ends:
    /// `sysexec` goes on after its point of no return (`sysproc.c:592`), on
    /// the old image's call.
    execd: RefCell<HashMap<Pid, (Image, Vec<String>)>>,
    /// The process whose call the kernel is in, if one is.
    calling: Cell<Option<Pid>>,
    /// When the clock's next interrupt is due, in milliseconds.
    tick: Cell<f64>,
}

impl Default for Web {
    fn default() -> Web {
        Web::new()
    }
}

impl Web {
    pub fn new() -> Web {
        Web {
            st: RefCell::new(HashMap::new()),
            modules: RefCell::new(HashMap::new()),
            sharing: RefCell::new(HashMap::new()),
            execd: RefCell::new(HashMap::new()),
            calling: Cell::new(None),
            tick: Cell::new(0.0),
        }
    }

    fn put(&self, pid: Pid, st: St) {
        self.st.borrow_mut().insert(pid, st);
    }

    /// The image `bytes` is: compiled once, then the same one each time.
    fn compile(&self, bytes: &[u8]) -> Result<Image, String> {
        if let Some(i) = self.modules.borrow().get(bytes) {
            return Ok(*i);
        }
        let (initial, maximum) = memimport(bytes).ok_or(EBADEXEC)?;
        // SAFETY: the page copies `bytes` and keeps nothing of the pointer.
        let module = unsafe { js::compile(bytes.as_ptr(), bytes.len()) };
        if module < 0 {
            return Err(EBADEXEC.into());
        }
        let i = Image { module, initial, maximum };
        self.modules.borrow_mut().insert(bytes.into(), i);
        Ok(i)
    }

    /// Give `pid` a worker running `img` from `_start`: the page ends the
    /// one it had, if any, and makes a new memory.
    fn start(&self, pid: Pid, img: Image, args: &[String]) {
        let mut b = Vec::new();
        for a in args {
            b.extend_from_slice(a.as_bytes());
            b.push(0);
        }
        unsafe { js::start(pid, img.module, img.initial, img.maximum, b.as_ptr(), b.len()) };
    }

    /// **The process is over**: its worker ends where it stands, and the
    /// machine forgets it. It is waiting on the kernel, or has stopped by
    /// itself, so ending it there loses nothing it was doing.
    fn end(&self, pid: Pid) {
        self.leave(pid);
        self.st.borrow_mut().remove(&pid);
        self.execd.borrow_mut().remove(&pid);
        unsafe { js::kill(pid) };
    }

    /// `pid` shares nothing any more: it exited, or `exec`d a new image.
    fn leave(&self, pid: Pid) {
        if let Some(g) = self.sharing.borrow_mut().remove(&pid) {
            let mut g = g.borrow_mut();
            g.stacks.remove(&pid);
            if g.occupant == Some(pid) {
                g.occupant = None;
            }
        }
    }

    /// Whether `pid` is waiting on the kernel — has said something, and not
    /// been answered — or has no worker at all. Its memory is still, then.
    fn quiet(pid: Pid) -> bool {
        unsafe { js::wait(pid, 0) != ev::TIME }
    }

    /// **Put `pid`'s stack in place** before it runs, if it shares its
    /// memory — as `hosts/ipnx`'s `occupy` does, with one difference: the
    /// sharer whose stack is there may still be running in its worker, and
    /// then `pid` cannot be entered yet. The answer is whether it can.
    fn occupy(&self, pid: Pid) -> Result<bool, String> {
        let Some(g) = self.sharing.borrow().get(&pid).cloned() else { return Ok(true) };
        let mut g = g.borrow_mut();
        if g.occupant == Some(pid) {
            return Ok(true);
        }
        if let Some(o) = g.occupant {
            if !Web::quiet(o) {
                return Ok(false);
            }
            // Only what its stack is using, `[low, top)`.
            let low = (unsafe { js::word(o, LOW) } as u32).min(g.top);
            let saved = read(o, low, g.top - low)?;
            g.stacks.insert(o, (low, saved));
        }
        if let Some((low, img)) = g.stacks.remove(&pid) {
            write(pid, low, &img)?;
        }
        g.occupant = Some(pid);
        Ok(true)
    }

    /// The child shares its parent's memory, with a stack of its own: the
    /// parent's region from `lo`, with the child's pid in its `Tos`.
    fn share(&self, parent: Pid, child: Pid, top: u32, stack: (u32, Vec<u8>)) {
        let found = self.sharing.borrow().get(&parent).cloned();
        let g = found.unwrap_or_else(|| {
            let g = Rc::new(RefCell::new(Sharing { top, occupant: Some(parent), stacks: HashMap::new() }));
            self.sharing.borrow_mut().insert(parent, g.clone());
            g
        });
        g.borrow_mut().stacks.insert(child, stack);
        self.sharing.borrow_mut().insert(child, g);
    }

    /// **Run the process until it leaves the processor**: wait for what it
    /// says, a tick at most, and answer it.
    fn run(&self, pid: Pid, sys: &mut dyn Syscalls) -> Result<Left, String> {
        let period = 1000.0 / HZ as f64;
        loop {
            let now = unsafe { js::now() };
            if now >= self.tick.get() {
                self.tick.set(now + period);
                // `clockintr` (`kw/clock.c:46`): `timerintr`, then what
                // `trap()`'s tail does — `notify` in user mode, and
                // `sched()` if `up->delaysched` (`pc/trap.c:438`, `:443`).
                let sched = sys.timerintr(&nopc);
                match sys.notify(pid, NoteAt::Clock) {
                    Notify::Pexit => {
                        self.end(pid);
                        return Ok(Left::Exited);
                    }
                    Notify::Sched => return Ok(Left::Sched),
                    // A note for a handler waits for the end of its next
                    // call, as on the terminal machine.
                    _ => {}
                }
                if sched {
                    return Ok(Left::Sched);
                }
            }
            let ms = (self.tick.get() - unsafe { js::now() }).ceil().max(0.0) as i32;
            match unsafe { js::wait(pid, ms) } {
                ev::TIME => {}
                ev::CALL => {
                    let no = unsafe { js::word(pid, 0) } as u32;
                    let mut w = [0i64; MAXSYSARG];
                    for (i, x) in w.iter_mut().enumerate() {
                        *x = unsafe { js::word(pid, i as u32 + 1) };
                    }
                    let call = decode(pid, no, &w);
                    self.calling.set(Some(pid));
                    let r = sys.syscall(pid, call, &Ureg { s: w.map(|x| x as u64), pc: &|| userpc(pid) });
                    self.calling.set(None);
                    if let Some(l) = self.finish(pid, sys, no, w, r)? {
                        return Ok(l);
                    }
                }
                ev::DELIVER => {
                    if let Some(l) = self.deliver(pid, sys, 0)? {
                        return Ok(l);
                    }
                }
                ev::YIELD => {
                    self.put(pid, St::Yielded);
                    return Ok(Left::Sched);
                }
                ev::FORKED => {
                    if let Some(l) = self.forked(pid, sys)? {
                        return Ok(l);
                    }
                }
                ev::FAULT => {
                    // `trap()`'s *"postnote(up, 1, "sys: trap: …",
                    // NDebug)"* (`pc/trap.c:366`), then `notify`: the
                    // instruction that trapped cannot be gone back to.
                    let mut b = vec![0u8; ERRMAX];
                    let n = unsafe { js::xferin(pid, b.as_mut_ptr(), b.len()) }.max(0) as usize;
                    b.truncate(n);
                    sys.postnote(pid, &String::from_utf8_lossy(&b), NoteFlag::NDebug);
                    sys.notify(pid, NoteAt::Fault);
                    self.end(pid);
                    return Ok(Left::Exited);
                }
                // Its image ran to the end, or its worker is gone.
                ev::EXITED => {
                    self.end(pid);
                    return Ok(Left::Exited);
                }
                e => return Err(format!("pid {pid} said {e}, which is nothing this machine says")),
            }
        }
    }

    /// **A call's answer, as the process takes it** — what each import in
    /// the terminal machine's table does after its `kcall`.
    fn finish(&self, pid: Pid, sys: &mut dyn Syscalls, no: u32, w: [i64; MAXSYSARG], r: Result<Ret, String>) -> Result<Option<Left>, String> {
        // It left the processor part way: on entry again, `resume`.
        if let Ok(Ret::Sched) = r {
            self.put(pid, St::Call { no, w });
            return Ok(Some(Left::Sched));
        }
        match no {
            // `exits` does not return.
            EXITS => {
                self.end(pid);
                return Ok(Some(Left::Exited));
            }
            // **`exec` does not return** (`sysproc.c:259`): the process is
            // the new image now, in a worker of its own.
            EXEC if r.is_ok() => {
                let Some((img, args)) = self.execd.borrow_mut().remove(&pid) else {
                    self.end(pid);
                    return Ok(Some(Left::Exited));
                };
                self.start(pid, img, &args);
                self.put(pid, St::Running);
                return Ok(Some(Left::Sched));
            }
            // **`rfork(RFPROC)` returns twice** (RESEARCH §16.12): the
            // process unwinds its stack for the child, says `FORKED`, and is
            // answered `GO` when the child's memory is made.
            RFORK if w[0] as i32 & rf::PROC != 0 => {
                if let Ok(Ret::Pid(child)) = r {
                    self.put(child, St::Forking);
                    let share = w[0] as i32 & rf::MEM != 0;
                    unsafe { js::reply(pid, rep::FORK, child as i64, share as i32) };
                    return Ok(None);
                }
            }
            // `noted`: back to where the note interrupted (`NCONT`,
            // `NRSTR`), on in the handler (`NSAVE`), or out of a process
            // that is gone.
            NOTED => match r {
                Ok(Ret::N(n)) if n as i32 == noted::NCONT || n as i32 == noted::NRSTR => {
                    unsafe { js::reply(pid, rep::NOTED, 0, 0) };
                    return Ok(None);
                }
                Ok(Ret::N(n)) if n as i32 == noted::NSAVE => {}
                Ok(_) => {
                    self.end(pid);
                    return Ok(Some(Left::Exited));
                }
                Err(_) => {}
            },
            _ => {}
        }
        let v = encode(pid, sys, no, &w, r);
        self.deliver(pid, sys, v)
    }

    /// **`notify(Ureg*)`, the machine's half** (`pc/trap.c:834`–`:857`), on
    /// the way back from a call: nothing to take, and the call answers `v`;
    /// a handler, whose text goes to the process with the call's value —
    /// it runs the handler and says `DELIVER` when the handler is done; or
    /// the end of the process.
    fn deliver(&self, pid: Pid, sys: &mut dyn Syscalls, v: i64) -> Result<Option<Left>, String> {
        match sys.notify(pid, NoteAt::Syscall) {
            // A stop at a call's end is the kernel's own: the call answered
            // `Sched`, and has already left and come back.
            Notify::No | Notify::Sched => {
                unsafe { js::reply(pid, rep::RET, v, 0) };
                Ok(None)
            }
            Notify::Pexit => {
                self.end(pid);
                Ok(Some(Left::Exited))
            }
            Notify::Handler { f, msg } => {
                let mut b = msg.into_bytes();
                b.truncate(ERRMAX - 1);
                b.push(0);
                unsafe {
                    js::xfer(pid, b.as_ptr(), b.len());
                    js::reply(pid, rep::HANDLER, v, f as i32);
                }
                Ok(None)
            }
        }
    }

    /// **The parent has unwound for its child**: make the child's memory —
    /// a copy of the parent's from the lowest live address, or for `RFMEM`
    /// the parent's own with a copy of the stack region — and let the
    /// parent wind its stack back. The child's worker starts when the
    /// scheduler first enters it.
    fn forked(&self, pid: Pid, sys: &mut dyn Syscalls) -> Result<Option<Left>, String> {
        let word = |i| unsafe { js::word(pid, i) };
        let (child, share, lo, tos, top, failed) =
            (word(0) as Pid, word(1) != 0, word(2) as u32, word(3) as u32, word(4) as u32, word(5) != 0);
        if failed {
            self.put(child, St::Unforked);
            return self.deliver(pid, sys, -1);
        }
        unsafe { js::fork(pid, child, share as i32, lo) };
        let at = tos + TOSPID;
        if share {
            let mut stack = read(pid, lo, top.saturating_sub(lo))?;
            if at >= lo && at + 4 <= top {
                let i = (at - lo) as usize;
                stack[i..i + 4].copy_from_slice(&child.to_le_bytes());
            }
            self.share(pid, child, top, (lo, stack));
        } else {
            // its own pid in its `Tos`, as its first `kexit` would write it
            write(child, at, &child.to_le_bytes())?;
        }
        self.put(child, St::Forked);
        unsafe { js::reply(pid, rep::GO, 0, 0) };
        Ok(None)
    }
}

impl Machine for Web {
    fn procsetup(&self, _pid: Pid) -> Result<(), String> {
        Ok(())
    }

    /// `todget`. The page has the clock.
    fn todget(&self) -> Tod {
        let ns = (unsafe { js::now() } * 1e6) as u64;
        Tod { nsec: ns, ticks: ns, hz: 1_000_000_000 }
    }

    /// `delay` (`pc/fns.h:23`): the page's wait, which a key cuts short.
    fn delay(&self, ms: u64) {
        unsafe { js::delay(ms.min(u32::MAX as u64) as u32) }
    }

    /// `*addr`. Wasm is little-endian.
    fn load(&self, pid: Pid, addr: u32) -> Result<i32, String> {
        let mut ok = 0;
        let v = unsafe { js::load(pid, addr, &mut ok) };
        if ok == 0 {
            return Err("address out of range".into());
        }
        Ok(v)
    }

    /// `cmpswap` — an atomic compare-and-swap on the process's memory, which
    /// is shared memory here.
    fn cmpswap(&self, pid: Pid, addr: u32, old: i32, new: i32) -> Result<bool, String> {
        match unsafe { js::cas(pid, addr, old, new) } {
            1 => Ok(true),
            0 => Ok(false),
            _ => Err("address out of range".into()),
        }
    }

    /// `touser` — the image is this process's now. It does not run.
    ///
    /// Compiled here, so an image that is not a module is refused here, as
    /// `sysexec` refuses a bad header before it commits (`sysproc.c:343`).
    fn touser(&self, pid: Pid, image: &[u8], args: &[String]) -> Result<(), String> {
        let img = self.compile(image)?;
        // A new image is a new memory: nothing is shared any more
        // (`sysproc.c:513`).
        self.leave(pid);
        if self.calling.get() == Some(pid) {
            self.execd.borrow_mut().insert(pid, (img, args.to_vec()));
        } else {
            self.put(pid, St::Image(img, args.to_vec()));
        }
        Ok(())
    }

    /// `gotolabel(&up->sched)` (`pc/l.s:992`) — enter the process; it
    /// returns when the process leaves.
    fn gotolabel(&self, pid: Pid, sys: &mut dyn Syscalls) -> Result<Left, String> {
        let st = self.st.borrow_mut().remove(&pid).ok_or("no such process")?;
        match st {
            // A child its parent has not finished making: back on the queue.
            St::Forking => {
                self.put(pid, St::Forking);
                return Ok(Left::Sched);
            }
            // A child whose making failed: it never ran, and it is done.
            St::Unforked => return Ok(Left::Exited),
            _ => {}
        }
        if !self.occupy(pid)? {
            self.put(pid, st);
            return Ok(Left::Sched);
        }
        self.put(pid, St::Running);
        match st {
            St::Image(img, args) => self.start(pid, img, &args),
            St::Forked => unsafe { js::child(pid) },
            St::Yielded => {
                if let Some(l) = self.deliver(pid, sys, 0)? {
                    return Ok(l);
                }
            }
            St::Call { no, w } => {
                self.calling.set(Some(pid));
                let r = sys.resume(pid);
                self.calling.set(None);
                if let Some(l) = self.finish(pid, sys, no, w, r)? {
                    return Ok(l);
                }
            }
            St::Running | St::Forking | St::Unforked => {}
        }
        self.run(pid, sys)
    }
}

/// **`ureg->pc`** — where the process made its call: the innermost frame
/// of its image, as the terminal machine finds it by walking the stack
/// (`userpc` there). Only a trace, a bad address or a bad call number asks,
/// so it is asked for, not sent with every call: the process is waiting on
/// the kernel, its call's frames still on its stack, and answers from them.
fn userpc(pid: Pid) -> u64 {
    unsafe { js::reply(pid, rep::PC, 0, 0) };
    match unsafe { js::wait(pid, 1000) } {
        ev::PC => unsafe { js::word(pid, 7) as u64 },
        _ => 0,
    }
}

/// No pc: a clock interrupt finds the process running in its worker, where
/// nothing can ask it.
fn nopc() -> u64 {
    0
}

/// The bytes at `addr` in `pid`'s memory.
fn read(pid: Pid, addr: u32, n: u32) -> Result<Vec<u8>, String> {
    let mut b = vec![0u8; n as usize];
    if unsafe { js::memread(pid, addr, b.as_mut_ptr(), b.len()) } < 0 {
        return Err("address out of range".into());
    }
    Ok(b)
}

fn write(pid: Pid, addr: u32, b: &[u8]) -> Result<(), String> {
    if unsafe { js::memwrite(pid, addr, b.as_ptr(), b.len()) } < 0 {
        return Err("address out of range".into());
    }
    Ok(())
}

/// A NUL-terminated string, which is how every name crosses.
fn cstr(pid: Pid, p: i32) -> Result<String, String> {
    let at = p.max(0) as u32;
    let n = unsafe { js::strlen(pid, at) };
    if n < 0 {
        return Err("address out of range".into());
    }
    Ok(String::from_utf8_lossy(&read(pid, at, n as u32)?).into_owned())
}

/// `char **argv`: pointers until a nil one, each a string.
fn cargv(pid: Pid, p: i32) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut at = p.max(0) as u32;
    loop {
        let b = read(pid, at, 4)?;
        let ptr = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        if ptr == 0 {
            return Ok(out);
        }
        out.push(cstr(pid, ptr)?);
        at += 4;
    }
}

/// **A call, from what the process said** — the decoding half of each
/// import in the terminal machine's table, in the same order. An argument
/// that cannot be read is passed empty, as there: the kernel's own checks
/// (`Kernel::validargs`) refuse it before the empty value is used.
fn decode(pid: Pid, no: u32, w: &[i64; MAXSYSARG]) -> Call {
    let a = |i: usize| w[i] as i32;
    let s = |i: usize| cstr(pid, a(i)).unwrap_or_default();
    let data = |p: i32, n: i32| read(pid, p.max(0) as u32, n.max(0) as u32).unwrap_or_default();
    match no {
        OPEN => Call::Open { path: s(0), mode: a(1) },
        CREATE => Call::Create { path: s(0), mode: a(1), perm: a(2) as u32 },
        CLOSE => Call::Close { fd: a(0) },
        PREAD => Call::Pread { fd: a(0), n: a(2).max(0) as usize, off: w[3] },
        PWRITE => Call::Pwrite { fd: a(0), data: data(a(1), a(2)), off: w[3] },
        SEEK => Call::Seek { fd: a(0), off: w[1], whence: a(2) },
        DUP => Call::Dup { old: a(0), new: a(1) },
        PIPE => Call::Pipe,
        REMOVE => Call::Remove { path: s(0) },
        CHDIR => Call::Chdir { path: s(0) },
        BIND => Call::Bind { name: s(0), old: s(1), flag: a(2) },
        MOUNT => Call::Mount {
            fd: a(0),
            afd: a(1),
            old: s(2),
            flag: a(3),
            aname: if a(4) == 0 { String::new() } else { s(4) },
        },
        UNMOUNT => Call::Unmount { name: if a(0) == 0 { None } else { Some(s(0)) }, old: s(1) },
        STAT => Call::Stat { path: s(0) },
        FSTAT => Call::Fstat { fd: a(0) },
        OLDSTAT => Call::OldStat { path: s(0) },
        OLDFSTAT => Call::OldFstat { fd: a(0) },
        WSTAT => Call::Wstat { path: s(0), edir: data(a(1), a(2)) },
        FWSTAT => Call::Fwstat { fd: a(0), edir: data(a(1), a(2)) },
        FAUTH => Call::Fauth { fd: a(0), aname: s(1) },
        FD2PATH => Call::Fd2path { fd: a(0) },
        FVERSION => Call::Fversion { fd: a(0), msize: a(1) as u32, version: s(2) },
        RFORK => Call::Rfork { flags: a(0) },
        EXEC => Call::Exec { path: s(0), args: cargv(pid, a(1)).unwrap_or_default() },
        // `exits(nil)` is the empty status, and nil is address zero.
        EXITS => Call::Exits { status: if a(0) == 0 { String::new() } else { s(0) } },
        AWAIT => Call::Await,
        ERRSTR => Call::Errstr { buf: s(0) },
        SYSR1 => Call::Sysr1,
        SLEEP => Call::Sleep { ms: a(0).max(0) as u64 },
        // `ulong` in, `long` out: both 32 bits on this architecture.
        ALARM => Call::Alarm { ms: a(0) as u32 as u64 },
        NOTIFY => Call::Notify { f: a(0) as u32 },
        NOTED => Call::Noted { how: a(0) },
        SEMACQUIRE => Call::Semacquire { addr: a(0) as u32, block: a(1) != 0 },
        TSEMACQUIRE => Call::Tsemacquire { addr: a(0) as u32, ms: a(1) as u32 as u64 },
        SEMRELEASE => Call::Semrelease { addr: a(0) as u32, delta: a(1) },
        RENDEZVOUS => Call::Rendezvous { tag: a(0) as u32 as u64, val: a(1) as u32 as u64 },
        // `segattach` and the other calls this kernel's table does not
        // have, and any number at all: the kernel answers as Plan 9's does
        // a call with no `systab` entry (`pc/trap.c:716`).
        n => Call::Bad { n },
    }
}

/// **What the call answers the process** — the other half of each import:
/// what goes back into its memory, and the value. A failed call answers −1
/// and leaves its reason for `errstr`.
fn encode(pid: Pid, sys: &mut dyn Syscalls, no: u32, w: &[i64; MAXSYSARG], r: Result<Ret, String>) -> i64 {
    let a = |i: usize| w[i] as i32;
    let put = |p: i32, b: &[u8]| write(pid, p.max(0) as u32, b).is_ok();
    match no {
        OPEN | CREATE | DUP | FAUTH => match r {
            Ok(Ret::Fd(fd)) => fd as i64,
            _ => -1,
        },
        PREAD => match r {
            Ok(Ret::Data(d)) if put(a(1), &d) => d.len() as i64,
            _ => -1,
        },
        PWRITE | ALARM | SEMACQUIRE | TSEMACQUIRE | SEMRELEASE | RENDEZVOUS => match r {
            Ok(Ret::N(n)) => n as i32 as i64,
            _ => -1,
        },
        SEEK => match r {
            Ok(Ret::N(n)) => n as i64,
            _ => -1,
        },
        PIPE => match r {
            Ok(Ret::Two(x, y)) => {
                let mut two = [0u8; 8];
                two[..4].copy_from_slice(&x.to_le_bytes());
                two[4..].copy_from_slice(&y.to_le_bytes());
                if put(a(0), &two) { 0 } else { -1 }
            }
            _ => -1,
        },
        // *"return nm->mountid"* (`chan.c:760`), and `bind` the same
        // (`sysfile.c:1054`).
        BIND | MOUNT => match r {
            Ok(Ret::N(id)) => id as i32 as i64,
            Ok(_) => 0,
            Err(_) => -1,
        },
        STAT | FSTAT => statlike(pid, r, a(1), a(2)),
        // `_stat` and `_fstat` (`sysfile.c:1258`): the old stat's fixed 116
        // bytes, and 0.
        OLDSTAT | OLDFSTAT => {
            if statlike(pid, r, a(1), 116) < 0 { -1 } else { 0 }
        }
        // `fd2path` (`sysfile.c:173`): the name, written as `snprint`
        // would, cut to fit.
        FD2PATH => match r {
            Ok(Ret::Str(path)) => {
                let n = a(2);
                if n <= 0 {
                    return 0;
                }
                let mut b = path.into_bytes();
                b.truncate(n as usize - 1);
                b.push(0);
                if put(a(1), &b) { 0 } else { -1 }
            }
            _ => -1,
        },
        // `sysfversion` (`auth.c:23`): the version agreed goes back into
        // the caller's buffer — *"if(returnlen < k) error(Eshort)"*
        // (`devmnt.c:142`) — and its length is the answer.
        FVERSION => match r {
            Ok(Ret::Str(got)) => {
                let b = got.into_bytes();
                let n = a(3);
                if n > 0 && (n as usize) < b.len() {
                    let ureg = Ureg { s: w.map(|x| x as u64), pc: &|| userpc(pid) };
                    let _ = sys.syscall(pid, Call::Errstr { buf: "i/o count too small".into() }, &ureg);
                    return -1;
                }
                if n > 0 && !put(a(2), &b) {
                    return -1;
                }
                b.len() as i64
            }
            _ => -1,
        },
        // A fork without `RFPROC`, and one whose unwinding failed.
        RFORK => match r {
            Ok(Ret::Pid(p)) => p as i64,
            _ => -1,
        },
        AWAIT => match r {
            Ok(Ret::Str(msg)) => {
                let b = msg.as_bytes();
                let k = b.len().min(a(1).max(0) as usize);
                if put(a(0), &b[..k]) { k as i64 } else { -1 }
            }
            _ => -1,
        },
        // `generrstr` (`sysproc.c:748`) EXCHANGES and answers 0.
        ERRSTR => match r {
            Ok(Ret::Str(old)) => {
                let mut b = old.into_bytes();
                b.truncate((a(1).max(1) as usize) - 1);
                b.push(0);
                if put(a(0), &b) { 0 } else { -1 }
            }
            _ => -1,
        },
        // A pointer's -1 is an int's here.
        SEGBRK | SEGATTACH | SEGDETACH | SEGFREE | SEGFLUSH => -1,
        // `exec` reaches here only when it failed.
        EXEC => -1,
        _ => match r {
            Ok(_) => 0,
            Err(_) => -1,
        },
    }
}

/// `stat` and `fstat` answer the directory entry's bytes, and the call
/// answers how many; one larger than the buffer is its size alone (*"if(ss
/// > nbuf) return BIT16SZ"*, `convD2M.c`), so `dirstat` asks again.
fn statlike(pid: Pid, r: Result<Ret, String>, p: i32, n: i32) -> i64 {
    let Ok(Ret::Data(d)) = r else { return -1 };
    let n = n.max(0) as usize;
    let at = p.max(0) as u32;
    if d.len() > n {
        if n < 2 {
            return -1;
        }
        return if write(pid, at, &d[..2]).is_ok() { 2 } else { -1 };
    }
    if write(pid, at, &d).is_ok() { d.len() as i64 } else { -1 }
}
