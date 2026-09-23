//! The machine: a WebAssembly engine.
//!
//! It implements `procsetup`, `todget` and `touser` — Plan 9's names for what
//! an architecture must supply (`pc/fns.h:173`, `pc/main.c:742`). Nothing
//! above this file knows what a machine is, which is the property the trait
//! exists for: Dis or the CLR would implement the same three.
//!
//! What a machine must arrange, concretely:
//!
//!   * **the calls.** Plan 9's arch half turns a trap into a `Call`; this one
//!     turns an import into one. The import list here is the counterpart of
//!     `libc/9syscall/mkfile`'s generated assembly, and the guest's half of
//!     it is `userspace/libc/wasm/sys.c`.
//!   * **the arguments.** `sysexec` copies argv onto the new process's stack
//!     and `touser` jumps with SP pointing at it. A module has no stack to
//!     copy onto, so the block is written into the module's own memory and
//!     the entry is called with its address.
//!   * **the end of a process.** `exits` never returns, and on Plan 9 that is
//!     the kernel never scheduling the process again. Here the process is a
//!     call on this thread's stack, so the import unwinds it — a trap out of
//!     the engine, caught below, and NOT a fault.

use ipnx_kernel::machine::{Left, Machine, NoteAt, Notify, Syscalls, Tod, Ureg, MAXSYSARG};
use ipnx_kernel::proc::{noted, NoteFlag, ERRMAX};
use ipnx_kernel::proc::rf;
use ipnx_kernel::{Call, Pid, Ret};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wasmtime::{
    Caller, Engine, Instance, Linker, Memory, Module, Store, StoreContextMut, TypedFunc,
    UpdateDeadline, Val,
};

/// The error an `exits` or a completed `exec` raises to unwind the process it
/// was running. It is not a fault, and [`Wasm::touser`] takes it as the
/// ordinary end of a process.
#[derive(Debug)]
struct Exited;

impl std::fmt::Display for Exited {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the process exited")
    }
}

impl std::error::Error for Exited {}

/// What `noted(NCONT)` raises to leave a note handler: the handler's frames
/// unwind to where the machine entered it, and the process carries on from
/// where the note found it — *"memmove(ureg, nureg, sizeof(Ureg))"*
/// (`pc/trap.c:917`), which on this machine is the frames below still being
/// there.
#[derive(Debug)]
struct Noted;

impl std::fmt::Display for Noted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the note was handled")
    }
}

impl std::error::Error for Noted {}

/// **`notify(Ureg*)`, the machine's half** (`pc/trap.c:834`–`:857`), done on
/// the way back to the process from a call: take what the kernel decided,
/// and act on it until there is nothing left to take.
///
/// For a handler, the note goes onto the process's own stack below its
/// stack pointer — *"sp -= 256; … memmove((char*)sp, up->note[0].msg,
/// ERRMAX)"* — and the handler is entered through the image's
/// `__notestart` with the note's address. There is no `Ureg` to put
/// there: the machine's registers are the engine's.
async fn deliver(c: &mut Caller<'_, Guest>) -> wasmtime::Result<()> {
    loop {
        let pid = c.data().pid;
        // Sound as [`call`] is: a poll, inside `gotolabel`.
        let n = unsafe { (*kernel()).notify(pid, NoteAt::Syscall) };
        match n {
            // A stop at a call's end is the kernel's own: the call answered
            // `Sched`, and `kcall` has already left and come back.
            Notify::No | Notify::Sched => return Ok(()),
            Notify::Pexit => return Err(wasmtime::Error::new(Exited)),
            Notify::Handler { f, msg } => handler(c, f, &msg).await?,
        }
    }
}

/// Enter a note handler, and come back when it calls `noted`.
async fn handler(c: &mut Caller<'_, Guest>, f: u32, msg: &str) -> wasmtime::Result<()> {
    let sp = c.get_export("__stack_pointer").and_then(|e| e.into_global());
    let start = c.get_export("__notestart").and_then(|e| e.into_func());
    let (Some(sp), Some(start)) = (sp, start) else {
        return fault(c, "sys: trap: no note handler entry");
    };
    let start = start.typed::<(i32, i32, i32), ()>(&*c)?;
    let old = sp.get(&mut *c).i32().unwrap_or(0);
    let at = ((old as u32).wrapping_sub(256 + ERRMAX as u32) & !15) as i32;
    let mut b = msg.as_bytes().to_vec();
    b.truncate(ERRMAX - 1);
    b.push(0);
    memory(c)?.write(&mut *c, at as usize, &b)?;
    sp.set(&mut *c, Val::I32(at))?;
    let r = start.call_async(&mut *c, (f as i32, 0, at)).await;
    sp.set(&mut *c, Val::I32(old))?;
    match r {
        Err(e) if e.downcast_ref::<Noted>().is_some() => Ok(()),
        Err(e) if e.downcast_ref::<Exited>().is_some() => Err(e),
        Err(e) => fault(c, &trapmsg(&e)),
        Ok(()) => fault(c, "sys: trap: note handler returned"),
    }
}

/// **A fault** — `trap()`'s *"postnote(up, 1, "sys: trap: …", NDebug)"*
/// (`pc/trap.c:366`), then `notify`. A process cannot go on from a trapped
/// instruction here, so the note ends it whether or not it has a handler.
fn fault(c: &mut Caller<'_, Guest>, msg: &str) -> wasmtime::Result<()> {
    let pid = c.data().pid;
    unsafe {
        (*kernel()).postnote(pid, msg, NoteFlag::NDebug);
        (*kernel()).notify(pid, NoteAt::Fault);
    }
    Err(wasmtime::Error::new(Exited))
}

/// *"sys: trap: "* and this machine's name for what trapped — `excname[]`
/// is the architecture's table (`pc/trap.c`), and a wasm engine's traps are
/// this one's.
fn trapmsg(e: &wasmtime::Error) -> String {
    match e.downcast_ref::<wasmtime::Trap>() {
        Some(t) => format!("sys: trap: {t}"),
        None => format!("sys: trap: {e}"),
    }
}

pub struct Wasm {
    engine: Engine,
    linker: Linker<Guest>,
    /// **One fiber per process** — the label `gotolabel` goes to. A future
    /// that has not finished is a suspended stack, and polling it is the
    /// jump. `Proc.kstack` is Plan 9's counterpart (`portdat.h:661`): the
    /// process's own stack, with its half-finished syscall still on it.
    ///
    /// The future owns its `Store`, so nothing here borrows anything: a
    /// process is a self-contained thing the scheduler can leave alone.
    procs: RefCell<HashMap<Pid, Fiber>>,
    /// The clock: what raises this machine's interrupt. Stopped when the
    /// machine goes.
    _clock: Clock,
}

/// **The clock** — this machine's counterpart of the i8253 or the local
/// APIC timer (`pc/i8253.c`, `pc/apic.c:376`): something outside the
/// processor that raises an interrupt HZ times a second.
///
/// The interrupt is wasmtime's epoch. A thread moves the engine's epoch on
/// every `1000/HZ` ms; each store's deadline is one epoch ahead, so the next
/// epoch check compiled into guest code — at a loop header or a function
/// entry — calls [`clockintr`]. **The check is only ever in guest code**,
/// never in a host function, so the interrupt only lands in user mode and
/// the kernel is never entered twice. Plan 9 takes clock interrupts in its
/// own kernel too, and `sched()`s at their tail when no ilock is held
/// (`pc/trap.c:438`, which has no `user` test); this kernel runs each call
/// to its end, so the tick waits for the call to finish.
///
/// It is the one thread this machine has, and it touches nothing but the
/// epoch counter — which is atomic, and is all an interrupt line is.
struct Clock {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Clock {
    fn start(engine: Engine) -> Clock {
        let stop = Arc::new(AtomicBool::new(false));
        let halt = stop.clone();
        let period = Duration::from_millis(1000 / ipnx_kernel::proc::HZ);
        let thread = std::thread::spawn(move || {
            while !halt.load(Ordering::Relaxed) {
                std::thread::sleep(period);
                engine.increment_epoch();
            }
        });
        Clock { stop, thread: Some(thread) }
    }
}

impl Drop for Clock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// **`clockintr`** — the machine's clock interrupt handler, as each
/// architecture has one (`kw/clock.c:46`, `pc/i8253.c:262`): call the
/// portable `timerintr`, then do what `trap()`'s tail does with its answer
/// (`pc/trap.c:438`) — `sched()` if `up->delaysched`, which here is to
/// yield the fiber, so `gotolabel` returns and the scheduler finds the
/// process still `Running` and puts it back on the queue.
///
/// The next interrupt is one epoch on.
///
/// **Then `notify`** — *"if(user){ if(up->procctl || up->nnote)
/// notify(ureg);"* (`pc/trap.c:443`). A note that ends the process ends it
/// here, by trapping out of the guest. A note for a handler waits for the
/// process's next call to end: the guest can be entered from a host call
/// and from nowhere else, and this is not one.
fn clockintr(c: StoreContextMut<'_, Guest>) -> wasmtime::Result<UpdateDeadline> {
    // Sound for the same reason [`call`] is: this runs inside a poll, inside
    // `gotolabel`, where the kernel is entered and nothing else holds it.
    let sched = unsafe { (*kernel()).timerintr(&|| userpc(&c)) };
    let up = c.data().pid;
    match unsafe { (*kernel()).notify(up, NoteAt::Clock) } {
        Notify::Pexit => return Err(wasmtime::Error::new(Exited)),
        // `procctl` stopped it: leave, and it goes on from here when
        // `start` readies it.
        Notify::Sched => return Ok(UpdateDeadline::Yield(1)),
        _ => {}
    }
    Ok(if sched { UpdateDeadline::Yield(1) } else { UpdateDeadline::Continue(1) })
}

/// A process, as this machine holds one.
struct Fiber {
    /// The suspended stack. `None` between [`Machine::touser`] giving the
    /// process an image and the first [`Machine::gotolabel`] entering it.
    run: Option<Pin<Box<dyn Future<Output = Result<(), wasmtime::Error>> + Send>>>,
    /// What [`Machine::touser`] was given, until the fiber is built. It is
    /// built on first entry rather than at `exec` because instantiating is
    /// the machine's work and `sysexec` does not run the process.
    image: Option<(Module, Vec<String>)>,
}

/// `Ebadexec` (`port/error.h:34`).
const EBADEXEC: &str = "exec header invalid";

/// **Leaving the processor**: `gotolabel(&m->sched)` (`pc/l.s:992`), at the
/// end of `sched()`. Pending once, ready after — so the fiber suspends with
/// its frames where they are, and the call goes on from that line when the
/// scheduler enters the process again.
///
/// The kernel has already said where the process went: it is `Wakeme` on a
/// `Rendez`, or `Ready`. So this carries nothing and needs no waker.
struct Sched(bool);

impl Sched {
    fn new() -> Sched {
        Sched(false)
    }
}

impl Future for Sched {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            return Poll::Ready(());
        }
        self.0 = true;
        Poll::Pending
    }
}

/// The waker a poll needs and this machine does not use. **Plan 9's run
/// queue is the waker**: `ready()` puts a process on it and `runproc` takes
/// it off, and nothing else decides who runs. A future that returns
/// `Pending` here has already told the kernel where it went — it is
/// `Wakeme` on a `Rendez`, or `Ready` — so there is nobody for a waker to
/// tell.
fn noop_waker() -> Waker {
    const VTABLE: RawWakerVTable = RawWakerVTable::new(
        |_| RawWaker::new(std::ptr::null(), &VTABLE),
        |_| {},
        |_| {},
        |_| {},
    );
    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
}

/// What the store carries while a process runs: **which process it is**.
/// Plan 9 keeps that in `up`.
///
/// The kernel is not here. It lives in [`KERNEL`], one per thread, which is
/// what makes `Guest` `Send` by itself: a pid is a number, and wasmtime may
/// move a number wherever it likes. It held a raw pointer to the kernel until
/// 2026-09-22, which made it `!Send` and needed `unsafe impl Send` to satisfy
/// `call_async` — a promise the compiler could not check, whose failure would
/// have been memory corruption with no message.
pub struct Guest {
    /// The process making the calls — the one whose fiber this is.
    pid: Pid,
    /// The image it is running, which a `procrfork` child is made from.
    module: Option<Module>,
    /// The words the import being made was called with — what the PC's
    /// `syscall()` finds above the user stack pointer (`Sargs`). Each import
    /// sets it on the way in.
    s: [u64; MAXSYSARG],
}

/// One argument word, as the process passed it: a 32-bit value as its bits,
/// a `vlong` whole.
trait Word {
    fn word(self) -> u64;
}

impl Word for i32 {
    fn word(self) -> u64 {
        self as u32 as u64
    }
}

impl Word for i64 {
    fn word(self) -> u64 {
        self as u64
    }
}

/// `ureg->pc` — where the process is: the innermost wasm frame's offset in
/// its module. Only asked for when a call is traced or a profile kept, as
/// finding it walks the stack.
fn userpc(c: impl wasmtime::AsContext) -> u64 {
    wasmtime::WasmBacktrace::force_capture(c)
        .frames()
        .first()
        .and_then(|f| f.module_offset())
        .unwrap_or(0) as u64
}

thread_local! {
    /// **The kernel, for whatever process is running on this thread.** Plan
    /// 9's counterpart is per processor: `up` and `m` are `Mach` fields, one
    /// per CPU (`pc/dat.h`), and a process runs on exactly one of them at a
    /// time. A thread is this machine's processor.
    ///
    /// [`Machine::gotolabel`] sets it for exactly as long as it holds the
    /// kernel's borrow, and puts back what was there when it returns — so
    /// every call reads a pointer taken from the borrow that is live NOW, not
    /// one kept from an earlier entry.
    ///
    /// The kernel is `Rc`-based and belongs to the thread that booted it.
    /// **A fiber resumed on any other thread finds this empty and stops with a
    /// message**, where before it would have used the kernel from the wrong
    /// thread. Under `cargo test` each test is its own thread and sees only
    /// its own kernel, which is the isolation that was only assumed before.
    static KERNEL: Cell<Option<*mut (dyn Syscalls + 'static)>> = const { Cell::new(None) };
}

thread_local! {
    /// **The memory of the process whose call the kernel is in** — its pid,
    /// and where its linear memory is and how long. [`Machine::load`] and
    /// [`Machine::cmpswap`] reach the process's words through it, which is
    /// what Plan 9's kernel does by dereferencing a user address: the
    /// segments are mapped in its own address space (`sysproc.c:1098`).
    ///
    /// It is set for exactly the length of a kernel call ([`call`], and the
    /// `resume` in [`kcall`]), and in that time no guest code runs, so the
    /// memory can neither grow nor move and the pointer stays good. Outside
    /// a call it is empty and both answer an error.
    static UMEM: Cell<Option<(Pid, *mut u8, usize)>> = const { Cell::new(None) };
}

/// **A process `procrfork` made**, waiting for the machine to keep its
/// fiber: its image, a copy of its parent's memory, and where it starts —
/// the parent's stack pointer, and the function it was given.
struct Fork {
    pid: Pid,
    module: Module,
    mem: Vec<u8>,
    sp: i32,
    f: i32,
    arg: i32,
}

thread_local! {
    /// The children made during the poll now in progress. The import that
    /// makes one cannot reach the machine's table — it holds only its own
    /// store — so [`Machine::gotolabel`] takes them in when the poll ends,
    /// before the scheduler can choose one to run.
    static FORKED: RefCell<Vec<Fork>> = const { RefCell::new(Vec::new()) };
}

/// Puts [`UMEM`] back as it was.
struct Umem(Option<(Pid, *mut u8, usize)>);

impl Drop for Umem {
    fn drop(&mut self) {
        UMEM.with(|m| m.set(self.0));
    }
}

/// Make the calling process's memory the one the kernel reaches, until the
/// guard drops.
fn umem(c: &mut Caller<'_, Guest>) -> Umem {
    let pid = c.data().pid;
    let m = memory(c).ok().map(|mem| (pid, mem.data_ptr(&*c), mem.data_size(&*c)));
    Umem(UMEM.with(|u| u.replace(m)))
}

/// The four bytes at `addr` in `pid`'s memory — which must be the process
/// whose call this is, because no other's memory is reachable now.
fn word(pid: Pid, addr: u32) -> Result<*mut [u8; 4], String> {
    let (p, base, len) = UMEM.with(|m| m.get()).ok_or("no process is in a call")?;
    if p != pid {
        return Err(format!("pid {pid}'s memory is not reachable: pid {p} is in the call"));
    }
    let at = addr as usize;
    if at.checked_add(4).is_none_or(|end| end > len) {
        return Err("address out of range".into());
    }
    // In bounds, checked above; valid while the call lasts ([`UMEM`]).
    Ok(unsafe { base.add(at) } as *mut [u8; 4])
}

/// Puts [`KERNEL`] back as it was, however the poll ends — a return, an
/// error or a panic.
struct Entered(Option<*mut (dyn Syscalls + 'static)>);

impl Drop for Entered {
    fn drop(&mut self) {
        KERNEL.with(|k| k.set(self.0));
    }
}

/// Make `sys` the kernel for this thread until the returned guard drops.
fn enter(sys: &mut dyn Syscalls) -> Entered {
    // The cast drops the borrow's lifetime, because a thread-local has to be
    // `'static`. **The invariant that makes it sound:** the guard is dropped
    // before `sys`'s borrow ends — `gotolabel` holds both, in that order — so
    // the pointer is never read after the borrow it came from.
    let p: *mut (dyn Syscalls + 'static) = unsafe {
        std::mem::transmute::<*mut dyn Syscalls, *mut (dyn Syscalls + 'static)>(sys)
    };
    Entered(KERNEL.with(|k| k.replace(Some(p))))
}

/// The kernel for the process running on this thread.
///
/// # Panics
/// If there is none: a process is running where nobody entered it, which
/// can only mean a fiber was resumed on a thread other than the one that
/// owns its kernel. That is a bug in whatever resumed it, and saying so is
/// the whole point of keeping the kernel here.
fn kernel() -> *mut (dyn Syscalls + 'static) {
    KERNEL.with(|k| k.get()).expect(
        "a process ran on a thread with no kernel: a fiber was resumed on a thread \
         other than the one that booted its kernel",
    )
}

/// The one shape every import has: which process is calling, and the kernel
/// to call.
fn call(c: &mut Caller<'_, Guest>, k: Call) -> Result<Ret, String> {
    let pid = c.data().pid;
    let _umem = umem(c);
    let s = c.data().s;
    let c = &*c;
    let pc = || userpc(c);
    let ureg = Ureg { s, pc: &pc };
    // Sound for as long as [`enter`]'s invariant holds: this runs inside a
    // poll, inside `gotolabel`, and nothing else makes a `&mut` to the kernel
    // while it does — every call finishes before control returns to the
    // guest.
    unsafe { (*kernel()).syscall(pid, k, &ureg) }
}

/// **A call, as the process makes it: to the end, however many times it
/// leaves the processor on the way.** A call that `sleep`s, `qlock`s or
/// `sched()`s part way answers [`Ret::Sched`]; the process leaves, as
/// `gotolabel(&m->sched)` does — the fiber suspends with this frame on it —
/// and when the scheduler enters it again the call goes back in through
/// `resume` and carries on from where it stopped.
///
/// Every import comes through here, because any call may end that way:
/// `syscall()`'s own last act is *"if(up->delaysched) sched();"*
/// (`pc/trap.c:778`).
async fn kcall(c: &mut Caller<'_, Guest>, k: Call) -> Result<Ret, String> {
    let mut r = call(c, k);
    while let Ok(Ret::Sched) = r {
        Sched::new().await;
        let pid = c.data().pid;
        let _umem = umem(c);
        // Sound as [`call`] is: a poll, inside `gotolabel`.
        r = unsafe { (*kernel()).resume(pid) };
    }
    r
}

fn memory(c: &mut Caller<'_, Guest>) -> Result<Memory, wasmtime::Error> {
    c.get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| wasmtime::Error::msg("the module exports no memory"))
}

/// Read a NUL-terminated string, which is how every name crosses: the guest's
/// stub passes `char*` and nothing else, exactly as Plan 9's does.
fn cstr(c: &mut Caller<'_, Guest>, p: i32) -> Result<String, wasmtime::Error> {
    let mem = memory(c)?;
    let d = mem.data(&*c);
    let start = p.max(0) as usize;
    if start > d.len() {
        return Err(wasmtime::Error::msg("address out of range"));
    }
    let end = d[start..]
        .iter()
        .position(|&b| b == 0)
        .map(|n| start + n)
        .unwrap_or(d.len());
    Ok(String::from_utf8_lossy(&d[start..end]).into_owned())
}

/// Read `char **argv`: pointers until a nil one, each a string.
fn cargv(c: &mut Caller<'_, Guest>, p: i32) -> Result<Vec<String>, wasmtime::Error> {
    let mut out = Vec::new();
    let mut at = p.max(0) as usize;
    loop {
        let mem = memory(c)?;
        let d = mem.data(&*c);
        if at + 4 > d.len() {
            return Err(wasmtime::Error::msg("address out of range"));
        }
        let ptr = i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]);
        if ptr == 0 {
            return Ok(out);
        }
        out.push(cstr(c, ptr)?);
        at += 4;
    }
}

fn read(c: &mut Caller<'_, Guest>, p: i32, n: i32) -> Result<Vec<u8>, wasmtime::Error> {
    let mem = memory(c)?;
    let mut b = vec![0u8; n.max(0) as usize];
    mem.read(&*c, p.max(0) as usize, &mut b)?;
    Ok(b)
}

fn write(c: &mut Caller<'_, Guest>, p: i32, b: &[u8]) -> Result<(), wasmtime::Error> {
    let mem = memory(c)?;
    mem.write(&mut *c, p.max(0) as usize, b)?;
    Ok(())
}

/// A failed call answers −1 and leaves its reason for `errstr`. That is Plan
/// 9's convention rather than an error type crossing, and it is why no import
/// below returns a trap for an ordinary failure.
fn or_fail(r: Result<Ret, String>, f: impl FnOnce(Ret) -> i32) -> i32 {
    match r {
        Ok(v) => f(v),
        Err(_) => -1,
    }
}

impl Wasm {
    pub fn new() -> Result<Wasm, String> {
        let mut config = wasmtime::Config::new();
        // **`async_support` is this machine's stack switch.** It is
        // `wasmtime-fiber`: the guest runs on a fiber of its own, and a host
        // function that does not finish suspends it with its frames intact —
        // which is `setlabel(&up->sched)` and `gotolabel(&m->sched)`
        // (`pc/l.s:1000`, `:992`) in the one arrangement this machine has.
        config.async_support(true);
        // **The interrupt line** — see [`Clock`].
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(|e| e.to_string())?;
        let mut linker: Linker<Guest> = Linker::new(&engine);
        imports(&mut linker).map_err(|e| e.to_string())?;
        let clock = Clock::start(engine.clone());
        Ok(Wasm { engine, linker, procs: RefCell::new(HashMap::new()), _clock: clock })
    }
}

impl Machine for Wasm {
    fn procsetup(&self, _pid: Pid) -> Result<(), String> {
        Ok(())
    }

    /// `todget`. The host has the clock; the kernel names what it reports.
    fn todget(&self) -> Tod {
        let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        Tod { nsec: d.as_nanos() as u64, ticks: d.as_nanos() as u64, hz: 1_000_000_000 }
    }

    /// `delay` (`pc/fns.h:23`). The PC spins on the TSC; this machine is a
    /// process on an operating system that can be asked to wait, so it asks.
    fn delay(&self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }

    /// `*addr`. Wasm is little-endian, whatever the host is.
    fn load(&self, pid: Pid, addr: u32) -> Result<i32, String> {
        let w = word(pid, addr)?;
        // Sound: [`word`] checked the bounds and the memory is still.
        Ok(i32::from_le_bytes(unsafe { *w }))
    }

    /// `cmpswap`. The guest cannot run while the kernel does, and a module's
    /// memory is not shared with any other thread, so a plain compare and a
    /// plain store are the whole of the atomicity `cmpswap386` provides by
    /// turning interrupts off (`pc/devarch.c:544`).
    fn cmpswap(&self, pid: Pid, addr: u32, old: i32, new: i32) -> Result<bool, String> {
        let w = word(pid, addr)?;
        // Sound as in [`Wasm::load`].
        unsafe {
            if i32::from_le_bytes(*w) != old {
                return Ok(false);
            }
            *w = new.to_le_bytes();
        }
        Ok(true)
    }

    /// `touser` — the image is this process's now. It does not run.
    ///
    /// **Compiled here, so an image that is not a module is refused here**,
    /// as `sysexec` refuses a bad header before it commits: *"exec header
    /// invalid"*, `Ebadexec` (`sysproc.c:343`), and the process goes on in
    /// its old image. Compiled any later, the failure came out of the
    /// scheduler and ended the system.
    fn touser(&self, pid: Pid, image: &[u8], args: &[String]) -> Result<(), String> {
        let module = Module::new(&self.engine, image).map_err(|_| EBADEXEC.to_string())?;
        // Replacing an entry IS `exec`: the old fiber goes, with whatever
        // was on it, because the process is the new image now.
        self.procs.borrow_mut().insert(pid, Fiber { run: None, image: Some((module, args.to_vec())) });
        Ok(())
    }

    /// `gotolabel(&up->sched)` (`pc/l.s:992`) — enter the process; it
    /// returns when the process leaves.
    ///
    /// **One poll is the jump.** A future that comes back `Pending` is a
    /// stack suspended inside a host call, which is a process that called
    /// `sched()`; one that comes back `Ready` is an image that ended.
    ///
    /// The fiber is taken out of the table for the length of the poll,
    /// because the process may `exec` while it runs — and an `exec` puts a
    /// new fiber in, which this must not then overwrite with the one it is
    /// holding.
    fn gotolabel(&self, pid: Pid, sys: &mut dyn Syscalls) -> Result<Left, String> {
        let mut f = self.procs.borrow_mut().remove(&pid).ok_or("no such process")?;
        if f.run.is_none() {
            let (image, args) = f.image.take().ok_or("the process has no image")?;
            f.run = Some(self.start(pid, image, args)?);
        }
        let mut run = f.run.take().expect("a fiber");
        let waker = noop_waker();
        // The kernel is this thread's for the length of the poll, and not a
        // moment longer: `entered` drops before `sys`'s borrow ends.
        let polled = {
            let _entered = enter(sys);
            run.as_mut().poll(&mut Context::from_waker(&waker))
        };
        // The processes `procrfork` made during the poll: fibers of their own
        // now, before the scheduler can choose one.
        for k in FORKED.with(|q| std::mem::take(&mut *q.borrow_mut())) {
            let run = self.child(k.pid, k.module.clone(), k.mem, k.sp, k.f, k.arg)?;
            self.procs.borrow_mut().insert(k.pid, Fiber { run: Some(run), image: None });
        }
        // Whatever `exec` left in the table wins; otherwise the fiber goes
        // back, suspended where it stopped.
        let replaced = self.procs.borrow().contains_key(&pid);
        match polled {
            Poll::Pending => {
                if !replaced {
                    self.procs.borrow_mut().insert(pid, Fiber { run: Some(run), image: None });
                }
                Ok(Left::Sched)
            }
            Poll::Ready(r) => {
                // `exits` and a finished `exec` unwind as `Exited`, and both
                // are the process leaving rather than a failure.
                match r {
                    Ok(()) => {}
                    Err(e) if e.downcast_ref::<Exited>().is_some() => {}
                    // **A fault** — `trap()` posts *"sys: trap: …"*
                    // (`pc/trap.c:366`) and `notify` ends the process: the
                    // instruction that trapped cannot be gone back to.
                    Err(e) => {
                        let msg = trapmsg(&e);
                        sys.postnote(pid, &msg, NoteFlag::NDebug);
                        sys.notify(pid, NoteAt::Fault);
                    }
                }
                if replaced {
                    // It `exec`d: the process lives on in its new image.
                    return Ok(Left::Sched);
                }
                Ok(Left::Exited)
            }
        }
    }
}

impl Wasm {
    /// Build the fiber: instantiate, place the arguments, and make the call
    /// to `_start` — as a future that has not begun.
    fn start(
        &self,
        pid: Pid,
        module: Module,
        args: Vec<String>,
    ) -> Result<Pin<Box<dyn Future<Output = Result<(), wasmtime::Error>> + Send>>, String> {
        let mut store = Store::new(&self.engine, Guest { pid, module: Some(module.clone()), s: [0; MAXSYSARG] });
        // Interrupts on: the next epoch calls `clockintr`.
        store.set_epoch_deadline(1);
        store.epoch_deadline_callback(clockintr);
        let linker = self.linker.clone();
        Ok(Box::pin(async move {
            let instance = linker.instantiate_async(&mut store, &module).await?;
            let (argc, argv, heap) = place(&mut store, &instance, &args)?;
            let start: TypedFunc<(i32, i32, i32), ()> =
                instance.get_typed_func(&mut store, "_start")?;
            start.call_async(&mut store, (argc, argv, heap)).await
        }))
    }

    /// Build a `procrfork` child's fiber: a new instance of the parent's
    /// image, the parent's memory copied into it, its stack pointer where
    /// the parent's was — so whatever the parent's frames hold, the child
    /// has too — and a call to `__childstart(f, arg)`.
    ///
    /// A module's data and stack are its memory, and the one other thing
    /// the compiler keeps outside it is the stack pointer, which is set
    /// here. Function pointers are indices into the module's table, which
    /// the same image fills the same way.
    fn child(
        &self,
        pid: Pid,
        module: Module,
        mem: Vec<u8>,
        sp: i32,
        f: i32,
        arg: i32,
    ) -> Result<Pin<Box<dyn Future<Output = Result<(), wasmtime::Error>> + Send>>, String> {
        let mut store = Store::new(&self.engine, Guest { pid, module: Some(module.clone()), s: [0; MAXSYSARG] });
        store.set_epoch_deadline(1);
        store.epoch_deadline_callback(clockintr);
        let linker = self.linker.clone();
        Ok(Box::pin(async move {
            let instance = linker.instantiate_async(&mut store, &module).await?;
            let m = instance
                .get_memory(&mut store, "memory")
                .ok_or_else(|| wasmtime::Error::msg("the module exports no memory"))?;
            const PAGE: usize = 64 * 1024;
            let have = m.data_size(&store);
            if mem.len() > have {
                m.grow(&mut store, ((mem.len() - have) / PAGE) as u64)?;
            }
            m.write(&mut store, 0, &mem)?;
            instance
                .get_global(&mut store, "__stack_pointer")
                .ok_or_else(|| wasmtime::Error::msg("the module exports no stack pointer"))?
                .set(&mut store, Val::I32(sp))?;
            let start: TypedFunc<(i32, i32), ()> = instance.get_typed_func(&mut store, "__childstart")?;
            start.call_async(&mut store, (f, arg)).await
        }))
    }
}

/// Place the argument block, the way `sysexec` places one on the new stack
/// (`sysproc.c:302`): the strings, then the `char*` array that points at them,
/// then a nil.
///
/// It goes ABOVE everything the module itself uses. `memory.grow` answers the
/// old size in pages, so the new pages are by definition untouched, and the
/// address just past the block is where this process's heap begins — which is
/// the third argument the entry takes and `sbrk` starts from.
fn place(
    store: &mut Store<Guest>,
    instance: &Instance,
    args: &[String],
) -> Result<(i32, i32, i32), wasmtime::Error> {
    let mem = instance
        .get_memory(&mut *store, "memory")
        .ok_or_else(|| wasmtime::Error::msg("the module exports no memory"))?;

    let mut block: Vec<u8> = Vec::new();
    let ptrs = (args.len() + 1) * 4;
    let mut at = ptrs;
    let mut offs = Vec::new();
    for a in args {
        offs.push(at);
        block.extend_from_slice(a.as_bytes());
        block.push(0);
        at += a.len() + 1;
    }
    let base = mem.data_size(&*store);
    let mut head: Vec<u8> = Vec::with_capacity(ptrs);
    for o in &offs {
        head.extend_from_slice(&((base + o) as u32).to_le_bytes());
    }
    head.extend_from_slice(&0u32.to_le_bytes());
    head.extend_from_slice(&block);

    const PAGE: usize = 64 * 1024;
    let pages = (head.len() + PAGE - 1) / PAGE;
    mem.grow(&mut *store, pages.max(1) as u64)?;
    mem.write(&mut *store, base, &head)?;

    let heap = (base + head.len() + 7) & !7;
    Ok((args.len() as i32, base as i32, heap as i32))
}

/// The import table — this machine's `9syscall`.
fn imports(l: &mut Linker<Guest>) -> Result<(), wasmtime::Error> {
    l.func_wrap_async("sys", "open", |mut c: Caller<'_, Guest>, (p, mode): (i32, i32)| Box::new(async move {
        c.data_mut().s = [p.word(), mode.word(), 0, 0, 0];
        let r = async {
        let Ok(path) = cstr(&mut c, p) else { return -1 };
        or_fail(kcall(&mut c, Call::Open { path, mode }).await, |v| match v {
            Ret::Fd(fd) => fd,
            _ => -1,
        })
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "create", |mut c: Caller<'_, Guest>, (p, mode, perm): (i32, i32, i32)| Box::new(async move {
        c.data_mut().s = [p.word(), mode.word(), perm.word(), 0, 0];
        let r = async {
        let Ok(path) = cstr(&mut c, p) else { return -1 };
        or_fail(
            kcall(&mut c, Call::Create { path, mode, perm: perm as u32 }).await,
            |v| match v {
                Ret::Fd(fd) => fd,
                _ => -1,
            },
        )
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "close", |mut c: Caller<'_, Guest>, (fd,): (i32,)| Box::new(async move {
        c.data_mut().s = [fd.word(), 0, 0, 0, 0];
        let r = async {
        or_fail(kcall(&mut c, Call::Close { fd }).await, |_| 0)
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async(
        "sys",
        "pread",
        |mut c: Caller<'_, Guest>, (fd, p, n, off): (i32, i32, i32, i64)| {
            Box::new(async move {
        c.data_mut().s = [fd.word(), p.word(), n.word(), off.word(), 0];
        let r = async {
                let d = match kcall(&mut c, Call::Pread { fd, n: n.max(0) as usize, off }).await {
                    Ok(Ret::Data(d)) => d,
                    _ => return -1,
                };
                match write(&mut c, p, &d) {
                    Ok(()) => d.len() as i32,
                    Err(_) => -1,
                }
            }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    })
        },
    )?;

    l.func_wrap_async("sys", "pwrite", |mut c: Caller<'_, Guest>, (fd, p, n, off): (i32, i32, i32, i64)| Box::new(async move {
        c.data_mut().s = [fd.word(), p.word(), n.word(), off.word(), 0];
        let r = async {
            let Ok(data) = read(&mut c, p, n) else { return -1 };
            or_fail(kcall(&mut c, Call::Pwrite { fd, data, off }).await, |v| match v {
                Ret::N(n) => n as i32,
                _ => -1,
            })
        }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "seek", |mut c: Caller<'_, Guest>, (fd, off, whence): (i32, i64, i32)| Box::new(async move {
        c.data_mut().s = [fd.word(), off.word(), whence.word(), 0, 0];
        let r = async {
        match kcall(&mut c, Call::Seek { fd, off, whence }).await {
            Ok(Ret::N(n)) => n as i64,
            _ => -1,
        }
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "dup", |mut c: Caller<'_, Guest>, (old, new): (i32, i32)| Box::new(async move {
        c.data_mut().s = [old.word(), new.word(), 0, 0, 0];
        let r = async {
        or_fail(kcall(&mut c, Call::Dup { old, new }).await, |v| match v {
            Ret::Fd(fd) => fd,
            _ => -1,
        })
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "pipe", |mut c: Caller<'_, Guest>, (p,): (i32,)| Box::new(async move {
        c.data_mut().s = [p.word(), 0, 0, 0, 0];
        let r = async {
        let (a, b) = match kcall(&mut c, Call::Pipe).await {
            Ok(Ret::Two(a, b)) => (a, b),
            _ => return -1,
        };
        let mut two = [0u8; 8];
        two[..4].copy_from_slice(&a.to_le_bytes());
        two[4..].copy_from_slice(&b.to_le_bytes());
        match write(&mut c, p, &two) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "remove", |mut c: Caller<'_, Guest>, (p,): (i32,)| Box::new(async move {
        c.data_mut().s = [p.word(), 0, 0, 0, 0];
        let r = async {
        let Ok(path) = cstr(&mut c, p) else { return -1 };
        or_fail(kcall(&mut c, Call::Remove { path }).await, |_| 0)
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "chdir", |mut c: Caller<'_, Guest>, (p,): (i32,)| Box::new(async move {
        c.data_mut().s = [p.word(), 0, 0, 0, 0];
        let r = async {
        let Ok(path) = cstr(&mut c, p) else { return -1 };
        or_fail(kcall(&mut c, Call::Chdir { path }).await, |_| 0)
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "bind", |mut c: Caller<'_, Guest>, (n, o, flag): (i32, i32, i32)| Box::new(async move {
        c.data_mut().s = [n.word(), o.word(), flag.word(), 0, 0];
        let r = async {
        let (Ok(name), Ok(old)) = (cstr(&mut c, n), cstr(&mut c, o)) else { return -1 };
        or_fail(kcall(&mut c, Call::Bind { name, old, flag }).await, |_| 0)
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "mount", |mut c: Caller<'_, Guest>, (fd, afd, o, flag, a): (i32, i32, i32, i32, i32)| Box::new(async move {
        c.data_mut().s = [fd.word(), afd.word(), o.word(), flag.word(), a.word()];
        let r = async {
            let Ok(old) = cstr(&mut c, o) else { return -1 };
            let aname = if a == 0 { String::new() } else { cstr(&mut c, a).unwrap_or_default() };
            or_fail(kcall(&mut c, Call::Mount { fd, afd, old, flag, aname }).await, |_| 0)
        }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "unmount", |mut c: Caller<'_, Guest>, (n, o): (i32, i32)| Box::new(async move {
        c.data_mut().s = [n.word(), o.word(), 0, 0, 0];
        let r = async {
        let Ok(old) = cstr(&mut c, o) else { return -1 };
        let name = if n == 0 { None } else { Some(cstr(&mut c, n).unwrap_or_default()) };
        or_fail(kcall(&mut c, Call::Unmount { name, old }).await, |_| 0)
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "stat", |mut c: Caller<'_, Guest>, (p, e, n): (i32, i32, i32)| Box::new(async move {
        c.data_mut().s = [p.word(), e.word(), n.word(), 0, 0];
        let r = async {
        let Ok(path) = cstr(&mut c, p) else { return -1 };
        let r = kcall(&mut c, Call::Stat { path }).await;
        statlike(&mut c, r, e, n)
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "fstat", |mut c: Caller<'_, Guest>, (fd, e, n): (i32, i32, i32)| Box::new(async move {
        c.data_mut().s = [fd.word(), e.word(), n.word(), 0, 0];
        let r = async {
        let r = kcall(&mut c, Call::Fstat { fd }).await;
        statlike(&mut c, r, e, n)
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "wstat", |mut c: Caller<'_, Guest>, (p, e, n): (i32, i32, i32)| Box::new(async move {
        c.data_mut().s = [p.word(), e.word(), n.word(), 0, 0];
        let r = async {
        let (Ok(path), Ok(edir)) = (cstr(&mut c, p), read(&mut c, e, n)) else { return -1 };
        or_fail(kcall(&mut c, Call::Wstat { path, edir }).await, |_| 0)
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "fwstat", |mut c: Caller<'_, Guest>, (fd, e, n): (i32, i32, i32)| Box::new(async move {
        c.data_mut().s = [fd.word(), e.word(), n.word(), 0, 0];
        let r = async {
        let Ok(edir) = read(&mut c, e, n) else { return -1 };
        or_fail(kcall(&mut c, Call::Fwstat { fd, edir }).await, |_| 0)
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "fversion", |mut c: Caller<'_, Guest>, (fd, m, v, n): (i32, i32, i32, i32)| Box::new(async move {
        c.data_mut().s = [fd.word(), m.word(), v.word(), n.word(), 0];
        let r = async {
        let Ok(version) = cstr(&mut c, v) else { return -1 };
        let _ = n;
        or_fail(
            kcall(&mut c, Call::Fversion { fd, msize: m as u32, version }).await,
            |_| 0,
        )
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "rfork", |mut c: Caller<'_, Guest>, (flags,): (i32,)| Box::new(async move {
        c.data_mut().s = [flags.word(), 0, 0, 0, 0];
        let r = async {
        // **A bare `rfork(RFPROC)` cannot be answered on this machine**, and
        // saying so is better than pretending. It returns twice — once into
        // the parent and once into the child — and a wasm call returns into
        // the engine's own stack, which nothing outside the engine can
        // duplicate (RESEARCH §5.2; the stack-switching proposal excludes
        // process duplication by name). Everything else `rfork` does is table
        // work in the kernel and is answered here.
        if flags & rf::PROC != 0 {
            let _ = call(
                &mut c,
                Call::Errstr { buf: "rfork: this machine cannot return twice; use procrfork".into() },
            );
            return -1;
        }
        or_fail(kcall(&mut c, Call::Rfork { flags }).await, |v| match v {
            Ret::Pid(p) => p as i32,
            _ => -1,
        })
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    // `procrfork(f, arg, stacksize, rforkflag)` — Plan 9's own shape for
    // making a process that runs a FUNCTION (`libthread/create.c:103`), and
    // the one shape a machine without a duplicable stack can honour. The
    // kernel call underneath is `rfork`, unchanged; what differs is that the
    // child is told where to start instead of resuming a copy of the parent.
    //
    // It answers the child's pid — where libthread answers a thread id,
    // because there are threads there and none here.
    //
    // **The child is a process of its own from the start**: a new instance of
    // the parent's image with a COPY of the parent's memory, starting at
    // `f(arg)` from the parent's stack pointer, so what the parent's frames
    // hold — `arg` may point into them — the child's copy holds too. It runs
    // on a fiber of its own, so it may sleep before it `exec`s — in a pipe,
    // in a file server's reply — as any process may.
    //
    // A copy is `rfork` without `RFMEM`, which is what the calls here ask
    // for. `RFMEM` itself cannot be given: one wasm memory cannot be in two
    // instances, and one instance cannot run on two stacks. Asked for, it
    // is refused rather than quietly not done.
    l.func_wrap_async(
        "sys",
        "procrfork",
        |mut c: Caller<'_, Guest>, (f, arg, _stack, flags): (i32, i32, i32, i32)| {
            Box::new(async move {
        c.data_mut().s = [f.word(), arg.word(), _stack.word(), flags.word(), 0];
        let r: Result<i32, wasmtime::Error> = async {
                if flags & rf::MEM != 0 {
                    let _ = call(
                        &mut c,
                        Call::Errstr { buf: "procrfork: this machine cannot share one memory between two processes".into() },
                    );
                    return Ok(-1i32);
                }
                let Some(module) = c.data().module.clone() else { return Ok(-1i32) };
                let child = match kcall(&mut c, Call::Rfork { flags: flags | rf::PROC }).await {
                    Ok(Ret::Pid(p)) => p,
                    _ => return Ok(-1i32),
                };
                let mem = memory(&mut c)?.data(&c).to_vec();
                let sp = c
                    .get_export("__stack_pointer")
                    .and_then(|e| e.into_global())
                    .and_then(|g| g.get(&mut c).i32())
                    .ok_or_else(|| wasmtime::Error::msg("the module exports no stack pointer"))?;
                FORKED.with(|q| q.borrow_mut().push(Fork { pid: child, module, mem, sp, f, arg }));
                // **`ready(p); sched();`** — `sysrfork`'s last two lines
                // (`sysproc.c`). The kernel did the `ready`; this is the
                // `sched`, and it is not an optimisation. Without it the
                // parent runs on, and `m->readied` hands the processor to
                // whatever it forks NEXT — so the first stage of a pipeline
                // sits `Ready` until the shell that made it has exited, and
                // writes to a console that is gone.
                Sched::new().await;
                Ok(child as i32)
            }
        .await;
        let r = r?;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    })
        },
    )?;

    l.func_wrap_async("sys", "exec", |mut c: Caller<'_, Guest>, (p, a): (i32, i32)| Box::new(async move {
        c.data_mut().s = [p.word(), a.word(), 0, 0, 0];
        let r: Result<i32, wasmtime::Error> = async {
        let (Ok(path), Ok(args)) = (cstr(&mut c, p), cargv(&mut c, a)) else {
            return Ok(-1);
        };
        match kcall(&mut c, Call::Exec { path, args }).await {
            // **`exec` does not return** (`sysproc.c:302`): the process IS
            // the new image now, and the frames of the old one are nobody's.
            // Unwinding them is what a machine with no address space to
            // discard does instead; the new image is a fiber the scheduler
            // will enter, not something that runs from in here.
            Ok(_) => Err(wasmtime::Error::new(Exited)),
            Err(_) => Ok(-1),
        }
    }.await;
        let r = r?;
        deliver(&mut c).await?;
        Ok(r)
    }))?;

    l.func_wrap_async("sys", "exits", |mut c: Caller<'_, Guest>, (p,): (i32,)| Box::new(async move {
        c.data_mut().s = [p.word(), 0, 0, 0, 0];
        let r: Result<(), wasmtime::Error> = async {
        // `exits(nil)` is the empty status, and nil is address zero.
        let status = if p == 0 { String::new() } else { cstr(&mut c, p).unwrap_or_default() };
        let _ = kcall(&mut c, Call::Exits { status }).await;
        Err(wasmtime::Error::new(Exited))
    }.await;
        let r = r?;
        deliver(&mut c).await?;
        Ok(r)
    }))?;

    l.func_wrap_async("sys", "await", |mut c: Caller<'_, Guest>, (p, n): (i32, i32)| {
        Box::new(async move {
        c.data_mut().s = [p.word(), n.word(), 0, 0, 0];
        let r = async {
            let msg = match kcall(&mut c, Call::Await).await {
                Ok(Ret::Str(s)) => s,
                _ => return -1,
            };
            let b = msg.as_bytes();
            let k = b.len().min(n.max(0) as usize);
            match write(&mut c, p, &b[..k]) {
                Ok(()) => k as i32,
                Err(_) => -1,
            }
        }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    })
    })?;

    l.func_wrap_async("sys", "errstr", |mut c: Caller<'_, Guest>, (p, n): (i32, i32)| Box::new(async move {
        c.data_mut().s = [p.word(), n.word(), 0, 0, 0];
        let r = async {
        // `generrstr` (`sysproc.c:748`) EXCHANGES and answers 0, never a
        // length: what the buffer held becomes the process's error string and
        // the old one is written back. `werrstr` is that, and nothing else.
        let Ok(buf) = cstr(&mut c, p) else { return -1 };
        let old = match kcall(&mut c, Call::Errstr { buf }).await {
            Ok(Ret::Str(s)) => s,
            _ => return -1,
        };
        let mut b = old.into_bytes();
        b.truncate((n.max(1) as usize) - 1);
        b.push(0);
        match write(&mut c, p, &b) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "sleep", |mut c: Caller<'_, Guest>, (ms,): (i32,)| {
        Box::new(async move {
        c.data_mut().s = [ms.word(), 0, 0, 0, 0];
        let r = async {
            match kcall(&mut c, Call::Sleep { ms: ms.max(0) as u64 }).await {
                Ok(_) => 0,
                Err(_) => -1,
            }
        }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    })
    })?;

    l.func_wrap_async("sys", "alarm", |mut c: Caller<'_, Guest>, (ms,): (i32,)| Box::new(async move {
        c.data_mut().s = [ms.word(), 0, 0, 0, 0];
        let r = async {
        // `ulong` in, `long` out: both 32 bits on this architecture.
        match kcall(&mut c, Call::Alarm { ms: ms as u32 as u64 }).await {
            Ok(Ret::N(n)) => n as i32,
            _ => -1,
        }
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "notify", |mut c: Caller<'_, Guest>, (f,): (i32,)| Box::new(async move {
        c.data_mut().s = [f.word(), 0, 0, 0, 0];
        let r = async {
        or_fail(kcall(&mut c, Call::Notify { f: f as u32 }).await, |_| 0)
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;
    // `noted`: the kernel says how, and the machine does it — back to where
    // the note interrupted (`NCONT`, `NRSTR`), on in the handler (`NSAVE`),
    // or out of a process that is gone.
    l.func_wrap_async("sys", "noted", |mut c: Caller<'_, Guest>, (v,): (i32,)| Box::new(async move {
        c.data_mut().s = [v.word(), 0, 0, 0, 0];
        let r = async {
        let r: Result<i32, wasmtime::Error> = async {
            match kcall(&mut c, Call::Noted { how: v }).await {
                Ok(Ret::N(n)) if n as i32 == noted::NCONT || n as i32 == noted::NRSTR => {
                    Err(wasmtime::Error::new(Noted))
                }
                Ok(Ret::N(n)) if n as i32 == noted::NSAVE => Ok(0),
                Ok(Ret::N(_)) => Err(wasmtime::Error::new(Exited)),
                _ => Ok(-1),
            }
        }
        .await;
        r
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;
    // `semacquire`, `tsemacquire`, `semrelease` (`sysproc.c:1187`, `:1206`,
    // `:1225`). The address crosses as a number; the kernel reads and
    // swaps the word through [`Machine::load`] and [`Machine::cmpswap`].
    l.func_wrap_async("sys", "semacquire", |mut c: Caller<'_, Guest>, (addr, block): (i32, i32)| Box::new(async move {
        c.data_mut().s = [addr.word(), block.word(), 0, 0, 0];
        let r = or_fail(kcall(&mut c, Call::Semacquire { addr: addr as u32, block: block != 0 }).await, |v| match v {
            Ret::N(n) => n as i32,
            _ => -1,
        });
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;
    l.func_wrap_async("sys", "tsemacquire", |mut c: Caller<'_, Guest>, (addr, ms): (i32, i32)| Box::new(async move {
        c.data_mut().s = [addr.word(), ms.word(), 0, 0, 0];
        let r = or_fail(kcall(&mut c, Call::Tsemacquire { addr: addr as u32, ms: ms as u32 as u64 }).await, |v| match v {
            Ret::N(n) => n as i32,
            _ => -1,
        });
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;
    l.func_wrap_async("sys", "semrelease", |mut c: Caller<'_, Guest>, (addr, delta): (i32, i32)| Box::new(async move {
        c.data_mut().s = [addr.word(), delta.word(), 0, 0, 0];
        let r = or_fail(kcall(&mut c, Call::Semrelease { addr: addr as u32, delta }).await, |v| match v {
            Ret::N(n) => n as i32,
            _ => -1,
        });
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;
    l.func_wrap_async("sys", "rendezvous", |mut c: Caller<'_, Guest>, (tag, val): (i32, i32)| Box::new(async move {
        c.data_mut().s = [tag.word(), val.word(), 0, 0, 0];
        let r = async {
        or_fail(
            kcall(&mut c, Call::Rendezvous { tag: tag as u32 as u64, val: val as u32 as u64 }).await,
            // The other's value — or `~0`, pulled out by a note.
            |v| match v {
                Ret::N(n) => n as i32,
                _ => -1,
            },
        )
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    Ok(())
}

/// `stat` and `fstat` answer the directory entry's bytes, and the call answers
/// how many. A buffer too small is `Eshort` on Plan 9 — here the kernel says
/// so and the call fails, rather than a short entry being written.
fn statlike(
    c: &mut Caller<'_, Guest>,
    r: Result<Ret, String>,
    p: i32,
    n: i32,
) -> i32 {
    let d = match r {
        Ok(Ret::Data(d)) => d,
        _ => return -1,
    };
    if d.len() > n.max(0) as usize {
        return -1;
    }
    match write(c, p, &d) {
        Ok(()) => d.len() as i32,
        Err(_) => -1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `call_async` needs the store's data to be `Send`, and `Guest` now is
    /// by construction — this fails to compile if a field ever makes it not.
    #[test]
    fn guest_is_send_without_a_promise() {
        fn send<T: Send>() {}
        send::<Guest>();
    }

    /// **Every stub in `sys.c` has an import here of the same type.** The
    /// module is only checked against the linker when a program is
    /// instantiated, and only for the imports it uses — so a mismatch in a
    /// call nothing yet makes waits for the first program that does. `alarm`
    /// answered `i64` for `long` until 2026-09-23 and nobody noticed, because
    /// no program called it.
    ///
    /// `vlong` is `i64`; every other type in the list is 32 bits on wasm32.
    #[test]
    fn every_stub_in_sys_c_matches_its_import() {
        use wasmtime::ValType;
        let src = include_str!("../../../userspace/libc/wasm/sys.c");
        let w = Wasm::new().unwrap();
        let mut store = Store::new(&w.engine, Guest { pid: 0, module: None, s: [0; MAXSYSARG] });
        let ty = |t: &str| if t.trim() == "vlong" { "i64" } else { "i32" };
        let mut n = 0;
        for line in src.lines().filter(|l| l.starts_with("SYS(")) {
            let name = &line[4..line.find(')').unwrap()];
            let decl = line.split_once("extern").unwrap().1.trim();
            let ret = decl.split_whitespace().next().unwrap();
            let args = &decl[decl.find('(').unwrap() + 1..decl.rfind(')').unwrap()];
            let want_p: Vec<&str> = args.split(',').filter(|a| !a.trim().is_empty() && a.trim() != "void").map(ty).collect();
            let want_r: Vec<&str> = if ret == "void" { vec![] } else { vec![ty(ret)] };
            let f = w.linker.get(&mut store, "sys", name).unwrap_or_else(|| panic!("no import for {name}"));
            let wasmtime::ExternType::Func(ft) = f.ty(&store) else { panic!("{name} is not a function") };
            let name_of = |v: ValType| match v {
                ValType::I32 => "i32",
                ValType::I64 => "i64",
                _ => "other",
            };
            let got_p: Vec<&str> = ft.params().map(name_of).collect();
            let got_r: Vec<&str> = ft.results().map(name_of).collect();
            assert_eq!((got_p, got_r), (want_p, want_r), "{name}: {line}");
            n += 1;
        }
        assert!(n >= 30, "read {n} stubs");
    }

    /// `load` and `cmpswap` reach the calling process's memory, little-endian,
    /// within its bounds, and nobody else's.
    #[test]
    fn load_and_cmpswap_reach_the_calling_process_and_no_other() {
        let w = Wasm::new().unwrap();
        let mut mem = vec![0u8; 16];
        mem[4..8].copy_from_slice(&7i32.to_le_bytes());
        assert!(w.load(3, 4).is_err(), "outside a call");
        let _g = Umem(UMEM.with(|u| u.replace(Some((3, mem.as_mut_ptr(), mem.len())))));
        assert_eq!(w.load(3, 4), Ok(7));
        assert_eq!(w.cmpswap(3, 4, 6, 1), Ok(false));
        assert_eq!(w.cmpswap(3, 4, 7, -2), Ok(true));
        assert_eq!(w.load(3, 4), Ok(-2));
        assert!(w.load(3, 13).is_err(), "the word runs past the end");
        assert!(w.load(4, 4).is_err(), "another process's");
        drop(_g);
        assert_eq!(&mem[4..8], &(-2i32).to_le_bytes());
    }

    /// A thread nobody entered has no kernel, and says so rather than using
    /// another thread's.
    #[test]
    fn a_thread_with_no_kernel_stops_with_a_message() {
        let r = std::thread::spawn(|| {
            kernel();
        })
        .join();
        let e = r.expect_err("kernel() returned on a thread nobody entered");
        let msg = e.downcast_ref::<&str>().map(|s| s.to_string())
            .or_else(|| e.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        assert!(msg.contains("no kernel"), "{msg}");
    }
}
