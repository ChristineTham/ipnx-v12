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
//!     it is `userspace/sys/src/libc/wasm/sys.c`.
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
use std::rc::Rc;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wasmtime::{
    AsContext, AsContextMut, Caller, Engine, Extern, ExternType, Instance, Linker, Memory,
    MemoryType, Module, SharedMemory, Store, StoreContextMut, UpdateDeadline, Val,
};

/// **A process's memory.** Every image `mk.sh` builds imports one, and the
/// machine makes it — shared, so that `rfork(RFMEM)` can give it to two
/// processes (RESEARCH §16.13). A hand-written module, as the tests here
/// use, declares its own.
#[derive(Clone)]
enum Mem {
    Own(Memory),
    Shared(SharedMemory),
}

impl Mem {
    fn ptr(&self, store: impl AsContext) -> *mut u8 {
        match self {
            Mem::Own(m) => m.data_ptr(&store),
            Mem::Shared(m) => m.data().as_ptr() as *mut u8,
        }
    }
    fn size(&self, store: impl AsContext) -> usize {
        match self {
            Mem::Own(m) => m.data_size(&store),
            Mem::Shared(m) => m.data_size(),
        }
    }
    /// The bytes, while nothing grows the memory. A process's memory is
    /// reached only while it runs — the machine is one thread, and a sharer
    /// runs only when this one does not — so nothing writes it meanwhile.
    fn data(&self, store: impl AsContext) -> &[u8] {
        let (p, n) = (self.ptr(&store), self.size(&store));
        unsafe { std::slice::from_raw_parts(p, n) }
    }
    fn read(&self, store: impl AsContext, at: usize, b: &mut [u8]) -> wasmtime::Result<()> {
        let d = self.data(&store);
        let src = at.checked_add(b.len()).and_then(|e| d.get(at..e)).ok_or_else(|| wasmtime::Error::msg("out of bounds memory access"))?;
        b.copy_from_slice(src);
        Ok(())
    }
    fn write(&self, store: impl AsContextMut, at: usize, b: &[u8]) -> wasmtime::Result<()> {
        let (p, n) = (self.ptr(&store), self.size(&store));
        if at.checked_add(b.len()).is_none_or(|e| e > n) {
            return Err(wasmtime::Error::msg("out of bounds memory access"));
        }
        unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), p.add(at), b.len()) };
        Ok(())
    }
    fn grow(&self, mut store: impl AsContextMut, pages: u64) -> wasmtime::Result<()> {
        match self {
            Mem::Own(m) => m.grow(&mut store, pages).map(|_| ()),
            Mem::Shared(m) => m.grow(pages).map(|_| ()),
        }
    }
    fn of(e: Option<Extern>) -> Option<Mem> {
        match e? {
            Extern::Memory(m) => Some(Mem::Own(m)),
            Extern::SharedMemory(m) => Some(Mem::Shared(m)),
            _ => None,
        }
    }
    fn instance(inst: &Instance, store: impl AsContextMut) -> wasmtime::Result<Mem> {
        Mem::of(inst.get_export(store, "memory")).ok_or_else(|| wasmtime::Error::msg("the module exports no memory"))
    }
}

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
    /// **Every image compiled so far, by its content.** Compiling is most
    /// of what an `exec` costs — Cranelift, around a second a command in a
    /// debug build — and the same few images are run over and over: every
    /// pipeline stage and subshell is `rc` again. A module is immutable and
    /// shared between instances, so an image seen before is not compiled
    /// again.
    ///
    /// Keyed by the bytes themselves: looking one up hashes them and a hit
    /// compares them whole, so what runs is exactly what was read. This is
    /// the machine's; the kernel's image cache (`attachimage`) is by
    /// channel, as Plan 9's is, and serves the text segment. An image that
    /// fails to compile is not kept. Nothing is ever dropped: a system runs
    /// a handful of distinct images, each tens of kilobytes.
    modules: RefCell<HashMap<Box<[u8]>, Module>>,
    /// **The processes sharing a memory by `RFMEM`**, each to its group.
    /// Plan 9 gives each its own stack segment at the same address
    /// (`segment.c:175`); one wasm memory has one stack region, so the
    /// machine keeps each sharer's copy and puts the running one's in place
    /// ([`Wasm::occupy`]). A process not sharing has no entry.
    sharing: RefCell<HashMap<Pid, Rc<RefCell<Sharing>>>>,
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
    if c.data().busy {
        return Ok(UpdateDeadline::Continue(1));
    }
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

/// A memory shared by `RFMEM`: whose stack is in its stack region now, and
/// every other sharer's, kept.
struct Sharing {
    mem: SharedMemory,
    top: usize,
    occupant: Option<Pid>,
    stacks: HashMap<Pid, Vec<u8>>,
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
    /// The image it is running, which a `fork` child is made from.
    module: Option<Module>,
    /// The words the import being made was called with — what the PC's
    /// `syscall()` finds above the user stack pointer (`Sargs`). Each import
    /// sets it on the way in.
    s: [u64; MAXSYSARG],
    /// **The stack, as asyncify lets this machine hold it** (RESEARCH
    /// §16.12). What an unwinding stack is being unwound for, and the stack
    /// pointer when it began — the compiler keeps that one thing outside
    /// memory, so it is kept here.
    op: Option<(Op, i32)>,
    /// What the call being wound back into answers, when it is reached.
    rewind: Option<i32>,
    /// Each `setjmp`'s stack, by its jmp_buf's address.
    jmps: HashMap<i32, Saved>,
    /// What the process was entered through, which a stack wound back must
    /// be entered through again.
    entry: Entry,
    /// Where its `Tos` is (`sys/include/tos.h`): the top of its stack.
    tos: i32,
    /// The end of its stack region, which is `[0, stacktop)` — the image is
    /// linked stack first — and the one part of a memory `RFMEM` does not
    /// share (`segment.c:175`).
    stacktop: i32,
    /// **Its stack is in the unwind buffer, or being wound back from it**:
    /// the clock does not take the processor now, because a process sharing
    /// this memory would unwind into the same buffer.
    busy: bool,
}

impl Guest {
    fn new(pid: Pid, module: Option<Module>) -> Guest {
        Guest { pid, module, s: [0; MAXSYSARG], op: None, rewind: None, jmps: HashMap::new(), entry: Entry::None, tos: 0, stacktop: 0, busy: false }
    }
}

/// Why a stack is being unwound.
#[derive(Clone, Copy, Debug)]
enum Op {
    /// `setjmp(j)`: keep it under j and wind it straight back.
    Setjmp { env: i32 },
    /// `longjmp(j, v)`: drop it, and wind back the one kept under j.
    Longjmp { env: i32, val: i32 },
    /// `fork`: a copy for the child, wound back in both — sharing this
    /// memory, for `RFMEM`.
    Fork { child: Pid, share: bool },
}

/// A stack kept by `setjmp`: its frames as asyncify wrote them, the stack
/// pointer, and the entry it runs from.
#[derive(Clone, Debug)]
struct Saved {
    frames: Vec<u8>,
    sp: i32,
    entry: Entry,
}

/// Where a process's code is entered — the bottom of every stack it has.
#[derive(Clone, Copy, Debug)]
enum Entry {
    None,
    /// `_start(argc, argv, heap, tos)` — `main9.c`
    Start(i32, i32, i32, i32),
    /// A jmp_buf's pc, called with its stack pointer: libthread's new
    /// thread (`libthread/wasm.c`), as 386's `longjmp` jumps to it.
    Jump(i32, i32),
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

/// A child's memory: a copy of all of its parent's; or, for `RFMEM`, the
/// parent's own — and a copy of the parent's stack region, which is not
/// shared (`segment.c:175`, *"case SG_STACK: n = newseg(…)"*).
enum ForkMem {
    Copy(Vec<u8>),
    Share(SharedMemory, Vec<u8>),
}

/// **A process `fork` made**, waiting for the machine to keep its fiber:
/// its image, a copy of its parent's memory, and the parent's stack
/// pointer and stack to wind back.
struct Fork {
    pid: Pid,
    parent: Pid,
    module: Module,
    mem: ForkMem,
    /// The stack as it unwound, which the child winds back.
    frames: Vec<u8>,
    sp: i32,
    stacktop: i32,
    /// The entry the parent's stack is under, which the child's — the same
    /// stack, wound back from the copy of memory it was given — is under too.
    from: Entry,
    /// The parent's `Tos`, at the same address in the copy, and the stacks
    /// its `setjmp`s kept — a child may `longjmp` to one (`libthread`'s
    /// `_schedfork` does).
    tos: i32,
    jmps: HashMap<i32, Saved>,
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
    let m = memory(c).ok().map(|mem| (pid, mem.ptr(&*c), mem.size(&*c)));
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

fn memory(c: &mut Caller<'_, Guest>) -> Result<Mem, wasmtime::Error> {
    Mem::of(c.get_export("memory")).ok_or_else(|| wasmtime::Error::msg("the module exports no memory"))
}

/// **An argument that cannot be read is not refused here.** Plan 9's
/// calls check their own addresses — `validaddr`, which posts *"sys: bad
/// address in syscall"* and ends the process — and the kernel does that for
/// every call from the words the process passed (`Kernel::validargs`). So an
/// import that cannot read a name or a buffer passes an empty one and makes
/// the call anyway: the kernel refuses it before the empty value is used,
/// because every address this cannot read is one the kernel's check fails.

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
        // **Shared memory**, for `RFMEM` (RESEARCH §16.13). Nothing runs
        // on two threads: a memory is shared by processes, which this
        // machine runs one at a time.
        config.wasm_threads(true);
        let engine = Engine::new(&config).map_err(|e| e.to_string())?;
        let mut linker: Linker<Guest> = Linker::new(&engine);
        imports(&mut linker).map_err(|e| e.to_string())?;
        let clock = Clock::start(engine.clone());
        Ok(Wasm {
            engine,
            linker,
            procs: RefCell::new(HashMap::new()),
            modules: RefCell::new(HashMap::new()),
            sharing: RefCell::new(HashMap::new()),
            _clock: clock,
        })
    }
}

impl Wasm {
    /// The module an image is: compiled once, then the same one each time.
    fn compile(&self, image: &[u8]) -> Result<Module, String> {
        if let Some(m) = self.modules.borrow().get(image) {
            return Ok(m.clone());
        }
        let m = Module::new(&self.engine, image).map_err(|_| EBADEXEC.to_string())?;
        self.modules.borrow_mut().insert(image.into(), m.clone());
        Ok(m)
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
    ///
    /// An image compiled before is not compiled again ([`Wasm::modules`]).
    fn touser(&self, pid: Pid, image: &[u8], args: &[String]) -> Result<(), String> {
        let module = self.compile(image)?;
        // A new image is a new memory: nothing is shared any more
        // (`sysproc.c:513`, *"putseg(up->seg[i])"*).
        self.leave(pid);
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
        self.occupy(pid);
        let waker = noop_waker();
        // The kernel is this thread's for the length of the poll, and not a
        // moment longer: `entered` drops before `sys`'s borrow ends.
        let polled = {
            let _entered = enter(sys);
            run.as_mut().poll(&mut Context::from_waker(&waker))
        };
        // The processes `fork` made during the poll: fibers of their own
        // now, before the scheduler can choose one.
        for k in FORKED.with(|q| std::mem::take(&mut *q.borrow_mut())) {
            let child = k.pid;
            if let ForkMem::Share(mem, stack) = &k.mem {
                self.share(k.parent, child, mem, k.stacktop as usize, stack.clone());
            }
            let run = self.child(k)?;
            self.procs.borrow_mut().insert(child, Fiber { run: Some(run), image: None });
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
                self.leave(pid);
                Ok(Left::Exited)
            }
        }
    }
}

impl Wasm {
    /// The imports for one instance of `module`: the calls, and — if the
    /// image imports its memory, as every one `mk.sh` builds does — the
    /// memory: `shared`, for an `RFMEM` child, or a new one.
    fn linker(&self, store: &Store<Guest>, module: &Module, shared: Option<SharedMemory>) -> wasmtime::Result<Linker<Guest>> {
        let mut l = self.linker.clone();
        for imp in module.imports() {
            if let ExternType::Memory(t) = imp.ty() {
                let m = match shared.clone() {
                    Some(m) => m,
                    None => SharedMemory::new(
                        &self.engine,
                        MemoryType::shared(t.minimum() as u32, t.maximum().unwrap_or(65536) as u32),
                    )?,
                };
                l.define(store, imp.module(), imp.name(), m)?;
            }
        }
        Ok(l)
    }

    /// **Put `pid`'s stack in place** before it runs, if it shares its
    /// memory: the one running keeps its stack in the region; the others'
    /// are kept here. Plan 9's MMU does this by mapping each process's own
    /// stack segment at the same address.
    fn occupy(&self, pid: Pid) {
        let Some(g) = self.sharing.borrow().get(&pid).cloned() else { return };
        let mut g = g.borrow_mut();
        if g.occupant == Some(pid) {
            return;
        }
        // The region is reached only here and by the process whose stack
        // it holds, which is not running now.
        let region = unsafe { std::slice::from_raw_parts_mut(g.mem.data().as_ptr() as *mut u8, g.top) };
        if let Some(o) = g.occupant {
            let saved = region.to_vec();
            g.stacks.insert(o, saved);
        }
        if let Some(img) = g.stacks.remove(&pid) {
            region.copy_from_slice(&img);
        }
        g.occupant = Some(pid);
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

    /// `child` shares `parent`'s memory, with a stack of its own.
    fn share(&self, parent: Pid, child: Pid, mem: &SharedMemory, top: usize, stack: Vec<u8>) {
        let found = self.sharing.borrow().get(&parent).cloned();
        let g = found.unwrap_or_else(|| {
            let g = Rc::new(RefCell::new(Sharing { mem: mem.clone(), top, occupant: Some(parent), stacks: HashMap::new() }));
            self.sharing.borrow_mut().insert(parent, g.clone());
            g
        });
        g.borrow_mut().stacks.insert(child, stack);
        self.sharing.borrow_mut().insert(child, g);
    }

    /// Build the fiber: instantiate, place the arguments, and make the call
    /// to `_start` — as a future that has not begun.
    fn start(
        &self,
        pid: Pid,
        module: Module,
        args: Vec<String>,
    ) -> Result<Pin<Box<dyn Future<Output = Result<(), wasmtime::Error>> + Send>>, String> {
        let mut store = Store::new(&self.engine, Guest::new(pid, Some(module.clone())));
        // Interrupts on: the next epoch calls `clockintr`.
        store.set_epoch_deadline(1);
        store.epoch_deadline_callback(clockintr);
        let linker = self.linker(&store, &module, None).map_err(|e| e.to_string())?;
        Ok(Box::pin(async move {
            let instance = linker.instantiate_async(&mut store, &module).await?;
            let (argc, argv, heap) = place(&mut store, &instance, &args)?;
            let tos = settos(&mut store, &instance, pid)?;
            run(&mut store, &instance, Entry::Start(argc, argv, heap, tos)).await
        }))
    }

    /// Build a child's fiber: a new instance of the parent's image, the
    /// parent's memory copied into it, its stack pointer where the parent's
    /// was — so whatever the parent's frames hold, the child has too — and
    /// the parent's entry again with its stack wound back, where its `rfork`
    /// answers 0.
    ///
    /// A module's data and stack are its memory, and the one other thing
    /// the compiler keeps outside it is the stack pointer, which is set
    /// here. Function pointers are indices into the module's table, which
    /// the same image fills the same way.
    fn child(
        &self,
        k: Fork,
    ) -> Result<Pin<Box<dyn Future<Output = Result<(), wasmtime::Error>> + Send>>, String> {
        let Fork { pid, module, mem, frames, sp, stacktop, from, tos, jmps, .. } = k;
        let mut store = Store::new(&self.engine, Guest::new(pid, Some(module.clone())));
        store.data_mut().tos = tos;
        store.data_mut().stacktop = stacktop;
        store.data_mut().jmps = jmps;
        store.set_epoch_deadline(1);
        store.epoch_deadline_callback(clockintr);
        let shared = match &mem {
            ForkMem::Share(m, _) => Some(m.clone()),
            ForkMem::Copy(_) => None,
        };
        let linker = self.linker(&store, &module, shared).map_err(|e| e.to_string())?;
        Ok(Box::pin(async move {
            let instance = linker.instantiate_async(&mut store, &module).await?;
            let m = Mem::instance(&instance, &mut store)?;
            if let ForkMem::Copy(bytes) = &mem {
                const PAGE: usize = 64 * 1024;
                let have = m.size(&store);
                if bytes.len() > have {
                    m.grow(&mut store, ((bytes.len() - have) / PAGE) as u64)?;
                }
                m.write(&mut store, 0, bytes)?;
                // its own pid in its `Tos`, as its first `kexit` would write
                // it; a sharer's is in the stack it was given
                m.write(&mut store, (tos + TOSPID) as usize, &(pid as u32).to_le_bytes())?;
            }
            instance
                .get_global(&mut store, "__stack_pointer")
                .ok_or_else(|| wasmtime::Error::msg("the module exports no stack pointer"))?
                .set(&mut store, Val::I32(sp))?;
            // The child is its parent's stack, wound back — from the
            // frames as they unwound, which a sharer's buffer may no longer
            // hold — and its `rfork` answers 0.
            let (buf, size) = asyncbuf(&mut store, &instance).await?;
            m.write(&mut store, buf as usize + 8, &frames)?;
            m.write(&mut store, buf as usize, &((buf + 8 + frames.len() as i32) as u32).to_le_bytes())?;
            m.write(&mut store, buf as usize + 4, &((buf + size) as u32).to_le_bytes())?;
            store.data_mut().rewind = Some(0);
            store.data_mut().busy = true;
            instance
                .get_typed_func::<i32, ()>(&mut store, "asyncify_start_rewind")?
                .call_async(&mut store, buf)
                .await?;
            run(&mut store, &instance, from).await
        }))
    }
}

/// **The process runs from its entry until its code is done — or until its
/// stack unwinds** for one of the machine's own operations (RESEARCH
/// §16.12). Then the machine does what the stack was unwound for and winds
/// back the stack that should run next: the same one for `setjmp` and in a
/// `fork` parent, the one kept by the `setjmp` for `longjmp`. The call that
/// began it answers what [`Guest::rewind`] says.
async fn run(store: &mut Store<Guest>, inst: &Instance, mut entry: Entry) -> wasmtime::Result<()> {
    loop {
        store.data_mut().entry = entry;
        let r = match entry {
            Entry::Start(argc, argv, heap, tos) => {
                inst.get_typed_func::<(i32, i32, i32, i32), ()>(&mut *store, "_start")?
                    .call_async(&mut *store, (argc, argv, heap, tos))
                    .await
            }
            Entry::Jump(pc, sp) => {
                // A function returning here has returned into nothing — on
                // the 386 into whatever was above it on its stack, a fault.
                let f = inst
                    .get_table(&mut *store, "__indirect_function_table")
                    .and_then(|t| t.get(&mut *store, pc as u64))
                    .and_then(|r| r.as_func().flatten().copied())
                    .ok_or_else(|| wasmtime::Error::msg("sys: trap: jump to a pc that is no function"))?;
                match f.typed::<i32, ()>(&*store)?.call_async(&mut *store, sp).await {
                    Ok(()) if store.data().op.is_none() => {
                        Err(wasmtime::Error::msg("sys: trap: a started function returned"))
                    }
                    r => r,
                }
            }
            Entry::None => return Err(wasmtime::Error::msg("no entry")),
        };
        let Some((op, sp)) = store.data_mut().op.take() else { return r };
        r?;
        inst.get_typed_func::<(), ()>(&mut *store, "asyncify_stop_unwind")?.call_async(&mut *store, ()).await?;
        let (buf, size) = asyncbuf(store, inst).await?;
        let mem = Mem::instance(inst, &mut *store)?;
        let cur = {
            let d = mem.data(&*store);
            u32::from_le_bytes(d[buf as usize..buf as usize + 4].try_into().unwrap()) as usize
        };
        let frames = mem.data(&*store)[buf as usize + 8..cur].to_vec();
        let (frames, sp, val, next) = match op {
            Op::Setjmp { env } => {
                // What 386's `setjmp.s` stores — SP, and a pc, which here
                // is 0: the frames are the machine's, kept under `env`.
                mem.write(&mut *store, env as usize + 4 * JMPBUFSP, &(sp as u32).to_le_bytes())?;
                mem.write(&mut *store, env as usize + 4 * JMPBUFPC, &0u32.to_le_bytes())?;
                store.data_mut().jmps.insert(env, Saved { frames: frames.clone(), sp, entry });
                (frames, sp, 0, entry)
            }
            Op::Longjmp { env, .. } if jmpbuf(&mem, &*store, env).1 != 0 => {
                // **A stack no `setjmp` made**: libthread's
                // `_threadinitstack` writes a function and a new stack's top
                // into the jmp_buf, as 386's `longjmp` finds them
                // (`libthread/386.c`). Nothing is wound back: the function
                // is called, on that stack, with the stack pointer.
                let (nsp, pc) = jmpbuf(&mem, &*store, env);
                store.data_mut().jmps.remove(&env);
                inst.get_global(&mut *store, "__stack_pointer")
                    .ok_or_else(|| wasmtime::Error::msg("the module exports no stack pointer"))?
                    .set(&mut *store, Val::I32(nsp & !15))?;
                entry = Entry::Jump(pc, nsp);
                store.data_mut().busy = false;
                continue;
            }
            Op::Longjmp { env, val } => {
                let s = store
                    .data()
                    .jmps
                    .get(&env)
                    .cloned()
                    .ok_or_else(|| wasmtime::Error::msg("longjmp to a jmp_buf no setjmp made"))?;
                (s.frames, s.sp, val, s.entry)
            }
            Op::Fork { child, share } => {
                // the child's memory: a copy of all of it; or for `RFMEM`
                // this one, and a copy of the stack region with the child's
                // pid in its `Tos`
                let module = store.data().module.clone().ok_or_else(|| wasmtime::Error::msg("no module"))?;
                let (tos, top, parent) = (store.data().tos, store.data().stacktop, store.data().pid);
                let cmem = match (&mem, share) {
                    (Mem::Shared(m), true) => {
                        let mut stack = mem.data(&*store)[..top as usize].to_vec();
                        let at = (tos + TOSPID) as usize;
                        if at + 4 <= stack.len() {
                            stack[at..at + 4].copy_from_slice(&(child as u32).to_le_bytes());
                        }
                        ForkMem::Share(m.clone(), stack)
                    }
                    _ => ForkMem::Copy(mem.data(&*store).to_vec()),
                };
                let jmps = store.data().jmps.clone();
                FORKED.with(|q| {
                    q.borrow_mut().push(Fork {
                        pid: child,
                        parent,
                        module,
                        mem: cmem,
                        frames: frames.clone(),
                        sp,
                        stacktop: top,
                        from: entry,
                        tos,
                        jmps,
                    })
                });
                (frames, sp, child as i32, entry)
            }
        };
        if frames.len() + 8 > size as usize {
            return Err(wasmtime::Error::msg("sys: trap: stack too deep to unwind"));
        }
        let b = buf as usize;
        // Rewinding reads the frames back from the end, outermost first,
        // so the next free byte is past them, as unwinding left it.
        mem.write(&mut *store, b + 8, &frames)?;
        mem.write(&mut *store, b, &((buf + 8 + frames.len() as i32) as u32).to_le_bytes())?;
        mem.write(&mut *store, b + 4, &((buf + size) as u32).to_le_bytes())?;
        inst.get_global(&mut *store, "__stack_pointer")
            .ok_or_else(|| wasmtime::Error::msg("the module exports no stack pointer"))?
            .set(&mut *store, Val::I32(sp))?;
        store.data_mut().rewind = Some(val);
        inst.get_typed_func::<i32, ()>(&mut *store, "asyncify_start_rewind")?.call_async(&mut *store, buf).await?;
        entry = next;
    }
}

/// **The `Tos`** (`sys/include/tos.h`), on wasm32: six words of profiling,
/// three vlongs of cycles, then `pid`, `clock` and four words of scratch.
const TOSSIZE: i32 = 72;
const TOSPID: i32 = 48;

/// Put the `Tos` at the top of the stack, where Plan 9's kernel puts it
/// (`USTKTOP-sizeof(Tos)`), the stack below it, and the pid in it — what
/// `kexit` writes on every return to user mode (`pc/trap.c:302`). Answers
/// its address, which `_start` is given as `main9.s` is given AX.
fn settos(store: &mut Store<Guest>, inst: &Instance, pid: Pid) -> wasmtime::Result<i32> {
    // An image with no stack of its own to speak of — a test's hand-written
    // module — is given none.
    let Some(sp) = inst.get_global(&mut *store, "__stack_pointer") else { return Ok(0) };
    let top = sp.get(&mut *store).i32().unwrap_or(0);
    store.data_mut().stacktop = top;
    let tos = (top - TOSSIZE) & !15;
    let mem = Mem::instance(inst, &mut *store)?;
    mem.write(&mut *store, tos as usize, &[0u8; TOSSIZE as usize])?;
    mem.write(&mut *store, (tos + TOSPID) as usize, &(pid as u32).to_le_bytes())?;
    sp.set(&mut *store, Val::I32(tos))?;
    store.data_mut().tos = tos;
    Ok(tos)
}

/// A `jmp_buf`'s two words, as `wasm/include/u.h` numbers them.
const JMPBUFSP: usize = 0;
const JMPBUFPC: usize = 1;

/// A `jmp_buf`'s stack pointer and pc.
fn jmpbuf(mem: &Mem, store: impl AsContext, env: i32) -> (i32, i32) {
    let d = mem.data(&store);
    let w = |i: usize| {
        let a = env as usize + 4 * i;
        d.get(a..a + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap())).unwrap_or(0)
    };
    (w(JMPBUFSP), w(JMPBUFPC))
}

/// Where the process's stack unwinds to (`libc/wasm/setjmp.c`), and how big.
async fn asyncbuf(store: &mut Store<Guest>, inst: &Instance) -> wasmtime::Result<(i32, i32)> {
    let b = inst.get_typed_func::<(), i32>(&mut *store, "__asyncbuf")?.call_async(&mut *store, ()).await?;
    let n = inst.get_typed_func::<(), i32>(&mut *store, "__asyncbufsize")?.call_async(&mut *store, ()).await?;
    Ok((b, n))
}

/// Begin unwinding the calling process's stack, for `op`. The import
/// returns, and the process's code returns frame by frame to [`run`].
async fn unwind(c: &mut Caller<'_, Guest>, op: Op) -> wasmtime::Result<()> {
    let sp = c
        .get_export("__stack_pointer")
        .and_then(|e| e.into_global())
        .and_then(|g| g.get(&mut *c).i32())
        .ok_or_else(|| wasmtime::Error::msg("the module exports no stack pointer"))?;
    let func = |c: &mut Caller<'_, Guest>, n: &str| {
        c.get_export(n).and_then(|e| e.into_func()).ok_or_else(|| wasmtime::Error::msg(format!("the module exports no {n}: not asyncified")))
    };
    let b = func(c, "__asyncbuf")?.typed::<(), i32>(&*c)?.call_async(&mut *c, ()).await?;
    let n = func(c, "__asyncbufsize")?.typed::<(), i32>(&*c)?.call_async(&mut *c, ()).await?;
    let mem = memory(c)?;
    mem.write(&mut *c, b as usize, &((b + 8) as u32).to_le_bytes())?;
    mem.write(&mut *c, b as usize + 4, &((b + n) as u32).to_le_bytes())?;
    c.data_mut().op = Some((op, sp));
    c.data_mut().busy = true;
    func(c, "asyncify_start_unwind")?.typed::<i32, ()>(&*c)?.call_async(&mut *c, b).await?;
    Ok(())
}

/// If the calling import is the one a stack was wound back into, finish the
/// winding and answer what it answers.
async fn rewound(c: &mut Caller<'_, Guest>) -> wasmtime::Result<Option<i32>> {
    let Some(v) = c.data_mut().rewind.take() else { return Ok(None) };
    c.get_export("asyncify_stop_rewind")
        .and_then(|e| e.into_func())
        .ok_or_else(|| wasmtime::Error::msg("not asyncified"))?
        .typed::<(), ()>(&*c)?
        .call_async(&mut *c, ())
        .await?;
    c.data_mut().busy = false;
    Ok(Some(v))
}

/// Place the argument block, the way `sysexec` places one on the new stack
/// (`sysproc.c:436`): the strings, then the `char*` array that points at them,
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
    let mem = Mem::instance(instance, &mut *store)?;

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
    let base = mem.size(&*store);
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
        let path = cstr(&mut c, p).unwrap_or_default();
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
        let path = cstr(&mut c, p).unwrap_or_default();
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
            let data = read(&mut c, p, n).unwrap_or_default();
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
        let path = cstr(&mut c, p).unwrap_or_default();
        or_fail(kcall(&mut c, Call::Remove { path }).await, |_| 0)
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "chdir", |mut c: Caller<'_, Guest>, (p,): (i32,)| Box::new(async move {
        c.data_mut().s = [p.word(), 0, 0, 0, 0];
        let r = async {
        let path = cstr(&mut c, p).unwrap_or_default();
        or_fail(kcall(&mut c, Call::Chdir { path }).await, |_| 0)
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "bind", |mut c: Caller<'_, Guest>, (n, o, flag): (i32, i32, i32)| Box::new(async move {
        c.data_mut().s = [n.word(), o.word(), flag.word(), 0, 0];
        let r = async {
        let (name, old) = (cstr(&mut c, n).unwrap_or_default(), cstr(&mut c, o).unwrap_or_default());
        or_fail(kcall(&mut c, Call::Bind { name, old, flag }).await, |_| 0)
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "mount", |mut c: Caller<'_, Guest>, (fd, afd, o, flag, a): (i32, i32, i32, i32, i32)| Box::new(async move {
        c.data_mut().s = [fd.word(), afd.word(), o.word(), flag.word(), a.word()];
        let r = async {
            let old = cstr(&mut c, o).unwrap_or_default();
            let aname = if a == 0 { String::new() } else { cstr(&mut c, a).unwrap_or_default() };
            // *"return nm->mountid"* (`chan.c:760`)
            or_fail(kcall(&mut c, Call::Mount { fd, afd, old, flag, aname }).await, |v| match v {
                Ret::N(id) => id as i32,
                _ => 0,
            })
        }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "unmount", |mut c: Caller<'_, Guest>, (n, o): (i32, i32)| Box::new(async move {
        c.data_mut().s = [n.word(), o.word(), 0, 0, 0];
        let r = async {
        let old = cstr(&mut c, o).unwrap_or_default();
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
        let path = cstr(&mut c, p).unwrap_or_default();
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
        let (path, edir) = (cstr(&mut c, p).unwrap_or_default(), read(&mut c, e, n).unwrap_or_default());
        or_fail(kcall(&mut c, Call::Wstat { path, edir }).await, |_| 0)
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "fwstat", |mut c: Caller<'_, Guest>, (fd, e, n): (i32, i32, i32)| Box::new(async move {
        c.data_mut().s = [fd.word(), e.word(), n.word(), 0, 0];
        let r = async {
        let edir = read(&mut c, e, n).unwrap_or_default();
        or_fail(kcall(&mut c, Call::Fwstat { fd, edir }).await, |_| 0)
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    // `fd2path` (`sysfile.c:173`): the kernel answers the name, and it is
    // written into the caller's buffer as `snprint` would, cut to fit.
    // `fauth(2)` (`auth.c:62`): a descriptor for the authentication file.
    l.func_wrap_async("sys", "fauth", |mut c: Caller<'_, Guest>, (fd, a): (i32, i32)| Box::new(async move {
        c.data_mut().s = [fd.word(), a.word(), 0, 0, 0];
        let r = async {
            let aname = cstr(&mut c, a).unwrap_or_default();
            or_fail(kcall(&mut c, Call::Fauth { fd, aname }).await, |v| match v {
                Ret::Fd(f) => f,
                _ => -1,
            })
        }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "fd2path", |mut c: Caller<'_, Guest>, (fd, p, n): (i32, i32, i32)| Box::new(async move {
        c.data_mut().s = [fd.word(), p.word(), n.word(), 0, 0];
        let r = async {
        let path = match kcall(&mut c, Call::Fd2path { fd }).await {
            Ok(Ret::Str(s)) => s,
            _ => return -1,
        };
        if n <= 0 {
            return 0;
        }
        let mut b = path.into_bytes();
        b.truncate(n as usize - 1);
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

    l.func_wrap_async("sys", "fversion", |mut c: Caller<'_, Guest>, (fd, m, v, n): (i32, i32, i32, i32)| Box::new(async move {
        c.data_mut().s = [fd.word(), m.word(), v.word(), n.word(), 0];
        let r = async {
        let version = cstr(&mut c, v).unwrap_or_default();
        // `sysfversion` (`auth.c:23`): the version agreed goes back into
        // the caller's buffer — *"if(returnlen < k) error(Eshort);
        // memmove(version, buf, k)"* (`devmnt.c:142`) — and its length is
        // the answer.
        match kcall(&mut c, Call::Fversion { fd, msize: m as u32, version }).await {
            Ok(Ret::Str(got)) => {
                let b = got.into_bytes();
                if n > 0 && (n as usize) < b.len() {
                    let _ = call(&mut c, Call::Errstr { buf: "i/o count too small".into() });
                    return -1;
                }
                if n > 0 && write(&mut c, v, &b).is_err() {
                    return -1;
                }
                b.len() as i32
            }
            _ => -1,
        }
    }
        .await;
        deliver(&mut c).await?;
        Ok::<_, wasmtime::Error>(r)
    }))?;

    l.func_wrap_async("sys", "rfork", |mut c: Caller<'_, Guest>, (flags,): (i32,)| Box::new(async move {
        c.data_mut().s = [flags.word(), 0, 0, 0, 0];
        let r = async {
        // **`rfork(RFPROC)` returns twice** — 0 in the child, the child's
        // pid in the parent — and on this machine that is asyncify's
        // (RESEARCH §16.12): the stack unwinds, the child is a new instance
        // with a copy of memory and the stack wound back in it, and the
        // parent's is wound back here. This is the second time through, in
        // either: the answer, and `sysrfork`'s *"ready(p); sched();"*.
        match rewound(&mut c).await {
            Ok(Some(v)) => {
                if v != 0 {
                    Sched::new().await;
                }
                return v;
            }
            Ok(None) => {}
            Err(_) => return -1,
        }
        if flags & rf::PROC != 0 {
            // `RFMEM` shares the memory, which a hand-written module's own
            // memory cannot be
            let share = flags & rf::MEM != 0;
            if share && !matches!(memory(&mut c), Ok(Mem::Shared(_))) {
                let _ = call(&mut c, Call::Errstr { buf: "rfork: this image's memory cannot be shared".into() });
                return -1;
            }
            let child = match kcall(&mut c, Call::Rfork { flags }).await {
                Ok(Ret::Pid(p)) => p,
                _ => return -1,
            };
            if unwind(&mut c, Op::Fork { child, share }).await.is_err() {
                return -1;
            }
            return 0;
        }
        or_fail(kcall(&mut c, Call::Rfork { flags }).await, |v| match v {
            Ret::Pid(p) => p as i32,
            _ => -1,
        })
    }
        .await;
        // A stack unwinding runs no code of the process's until it is
        // wound back — a note waits for the second time through.
        if c.data().op.is_none() {
            deliver(&mut c).await?;
        }
        Ok::<_, wasmtime::Error>(r)
    }))?;

    // `setjmp` and `longjmp` (`libc/wasm/setjmp.c`): the machine keeps the
    // stack (RESEARCH §16.12). Each is called twice: once to unwind, and —
    // setjmp's own call, found again — once as the stack is wound back.
    l.func_wrap_async("sys", "setjmp", |mut c: Caller<'_, Guest>, (env,): (i32,)| Box::new(async move {
        if let Some(v) = rewound(&mut c).await? {
            return Ok(v);
        }
        unwind(&mut c, Op::Setjmp { env }).await?;
        Ok::<_, wasmtime::Error>(0)
    }))?;
    l.func_wrap_async("sys", "longjmp", |mut c: Caller<'_, Guest>, (env, val): (i32, i32)| Box::new(async move {
        let mem = memory(&mut c)?;
        if jmpbuf(&mem, &c, env).1 == 0 && !c.data().jmps.contains_key(&env) {
            return Err(wasmtime::Error::msg("sys: trap: longjmp to a jmp_buf no setjmp made"));
        }
        unwind(&mut c, Op::Longjmp { env, val }).await?;
        Ok::<_, wasmtime::Error>(())
    }))?;

    l.func_wrap_async("sys", "exec", |mut c: Caller<'_, Guest>, (p, a): (i32, i32)| Box::new(async move {
        c.data_mut().s = [p.word(), a.word(), 0, 0, 0];
        let r: Result<i32, wasmtime::Error> = async {
        let (path, args) = (cstr(&mut c, p).unwrap_or_default(), cargv(&mut c, a).unwrap_or_default());
        match kcall(&mut c, Call::Exec { path, args }).await {
            // **`exec` does not return** (`sysproc.c:259`): the process IS
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
        let buf = cstr(&mut c, p).unwrap_or_default();
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
        let src = include_str!("../../../userspace/sys/src/libc/wasm/sys.c");
        let w = Wasm::new().unwrap();
        let mut store = Store::new(&w.engine, Guest::new(0, None));
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

    /// An image is compiled once: the second `touser` of the same bytes
    /// gets the same module, and different bytes a different one. An image
    /// that is not a module is refused with `Ebadexec` and not kept.
    #[test]
    fn an_image_is_compiled_once() {
        let w = Wasm::new().unwrap();
        let a = wat::parse_str("(module (memory (export \"memory\") 1))").unwrap();
        let b = wat::parse_str("(module (memory (export \"memory\") 2))").unwrap();
        w.touser(1, &a, &[]).unwrap();
        w.touser(2, &a, &[]).unwrap();
        w.touser(3, &b, &[]).unwrap();
        // One compilation is one image of machine code in memory; the same
        // module shares it, another compilation has its own.
        let code = |pid| w.procs.borrow()[&pid].image.as_ref().unwrap().0.image_range();
        assert_eq!(code(1), code(2), "compiled again");
        assert_ne!(code(1), code(3));
        assert_eq!(w.modules.borrow().len(), 2);
        assert_eq!(w.touser(4, b"junk", &[]), Err(EBADEXEC.to_string()));
        assert_eq!(w.modules.borrow().len(), 2, "a refused image is not kept");
        assert!(!w.procs.borrow().contains_key(&4));
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
