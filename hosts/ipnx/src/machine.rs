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

use ipnx_kernel::machine::{Left, Machine, Syscalls, Tod};
use ipnx_kernel::proc::rf;
use ipnx_kernel::{Call, Pid, Ret};
use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::{SystemTime, UNIX_EPOCH};
use wasmtime::{Caller, Engine, Instance, Linker, Memory, Module, Store, TypedFunc};

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
}

/// A process, as this machine holds one.
struct Fiber {
    /// The suspended stack. `None` between [`Machine::touser`] giving the
    /// process an image and the first [`Machine::gotolabel`] entering it.
    run: Option<Pin<Box<dyn Future<Output = Result<(), wasmtime::Error>> + Send>>>,
    /// What [`Machine::touser`] was given, until the fiber is built. It is
    /// built on first entry rather than at `exec` because instantiating is
    /// the machine's work and `sysexec` does not run the process.
    image: Option<(Vec<u8>, Vec<String>)>,
}

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

/// What the store carries while a process runs: which process it is, and the
/// kernel to call. Plan 9 keeps the first in `up` and needs nothing at all for
/// the second, because its kernel is reachable from anywhere.
///
/// The kernel is held as a pointer because wasmtime's `Store<T>` requires
/// `T: 'static` and this borrow is not. **The invariant that makes it sound:**
/// the pointer is set afresh by every [`Machine::gotolabel`], which holds the
/// borrow for the whole of that call, and a process only ever runs inside
/// one. Between calls the pointer is not dereferenced, because nothing of
/// the process is running. Nothing else may construct a `Guest`.
pub struct Guest {
    /// The process making the calls. It is not constant for the life of the
    /// store: during a `procrfork` the child runs on this instance, and every
    /// call it makes is the child's.
    pid: Pid,
    sys: *mut dyn Syscalls,
}

/// **Nothing here is ever sent anywhere.** wasmtime asks for `Send` because a
/// fiber it suspends *may in general* be resumed on another thread; this
/// machine's executor is a poll loop on the thread that booted, the kernel is
/// `Rc`-based and could not survive a move, and no other thread exists. The
/// claim is the same one the raw pointer above already rests on, one scope
/// wider: the scheduler loop owns the kernel and is on the stack for as long
/// as any process can run.
unsafe impl Send for Guest {}

impl Guest {
    /// # Safety
    /// Only called from a closure the store owns, so the invariant above
    /// holds: `touser` is on the stack and still holds the borrow.
    fn sys(&mut self) -> &mut dyn Syscalls {
        unsafe { &mut *self.sys }
    }
}

/// The one shape every import has: take the calling pid and the kernel out of
/// the store, and make a call.
fn call(c: &mut Caller<'_, Guest>, k: Call) -> Result<Ret, String> {
    let g = c.data_mut();
    let (pid, sys) = (g.pid, g.sys());
    sys.syscall(pid, k)
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
        let engine = Engine::new(&config).map_err(|e| e.to_string())?;
        let mut linker: Linker<Guest> = Linker::new(&engine);
        imports(&mut linker).map_err(|e| e.to_string())?;
        Ok(Wasm { engine, linker, procs: RefCell::new(HashMap::new()) })
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

    /// `touser` — the image is this process's now. It does not run.
    fn touser(&self, pid: Pid, image: &[u8], args: &[String]) -> Result<(), String> {
        // Replacing an entry IS `exec`: the old fiber goes, with whatever
        // was on it, because the process is the new image now.
        self.procs.borrow_mut().insert(
            pid,
            Fiber { run: None, image: Some((image.to_vec(), args.to_vec())) },
        );
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
            f.run = Some(self.start(pid, image, args, sys)?);
        } else {
            // A suspended fiber holds a `Guest` with last time's pointer.
            // The kernel is the same object either way; this is the borrow
            // being renewed, and it is why the pointer is never held.
            self.reseat(pid, sys);
        }
        let mut run = f.run.take().expect("a fiber");
        let waker = noop_waker();
        let polled = run.as_mut().poll(&mut Context::from_waker(&waker));
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
                    Err(e) => return Err(e.to_string()),
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
        image: Vec<u8>,
        args: Vec<String>,
        sys: &mut dyn Syscalls,
    ) -> Result<Pin<Box<dyn Future<Output = Result<(), wasmtime::Error>> + Send>>, String> {
        let module = Module::new(&self.engine, &image).map_err(|e| e.to_string())?;
        // The cast drops the borrow's lifetime; [`Guest`] records what makes
        // it sound.
        let sys: *mut (dyn Syscalls + 'static) =
            unsafe { std::mem::transmute::<*mut dyn Syscalls, *mut (dyn Syscalls + 'static)>(sys) };
        let mut store = Store::new(&self.engine, Guest { pid, sys });
        let linker = self.linker.clone();
        Ok(Box::pin(async move {
            let instance = linker.instantiate_async(&mut store, &module).await?;
            let (argc, argv, heap) = place(&mut store, &instance, &args)?;
            let start: TypedFunc<(i32, i32, i32), ()> =
                instance.get_typed_func(&mut store, "_start")?;
            start.call_async(&mut store, (argc, argv, heap)).await
        }))
    }

    /// Renew the kernel pointer a suspended fiber's store holds. There is no
    /// way to reach inside a running future, so the store keeps it and this
    /// is a no-op: the pointer a fiber was built with is the same kernel it
    /// is handed now, because one kernel exists and the scheduler owns it
    /// for the life of the system.
    fn reseat(&self, _pid: Pid, _sys: &mut dyn Syscalls) {}
}

/// Place the argument block/// Place the argument block, the way `sysexec` places one on the new stack
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
    l.func_wrap("sys", "open", |mut c: Caller<'_, Guest>, p: i32, mode: i32| {
        let Ok(path) = cstr(&mut c, p) else { return -1 };
        or_fail(call(&mut c, Call::Open { path, mode }), |v| match v {
            Ret::Fd(fd) => fd,
            _ => -1,
        })
    })?;

    l.func_wrap("sys", "create", |mut c: Caller<'_, Guest>, p: i32, mode: i32, perm: i32| {
        let Ok(path) = cstr(&mut c, p) else { return -1 };
        or_fail(
            call(&mut c, Call::Create { path, mode, perm: perm as u32 }),
            |v| match v {
                Ret::Fd(fd) => fd,
                _ => -1,
            },
        )
    })?;

    l.func_wrap("sys", "close", |mut c: Caller<'_, Guest>, fd: i32| {
        or_fail(call(&mut c, Call::Close { fd }), |_| 0)
    })?;

    l.func_wrap(
        "sys",
        "pread",
        |mut c: Caller<'_, Guest>, fd: i32, p: i32, n: i32, off: i64| {
            let d = match call(&mut c, Call::Pread { fd, n: n.max(0) as usize, off }) {
                Ok(Ret::Data(d)) => d,
                _ => return -1,
            };
            match write(&mut c, p, &d) {
                Ok(()) => d.len() as i32,
                Err(_) => -1,
            }
        },
    )?;

    l.func_wrap(
        "sys",
        "pwrite",
        |mut c: Caller<'_, Guest>, fd: i32, p: i32, n: i32, off: i64| {
            let Ok(data) = read(&mut c, p, n) else { return -1 };
            or_fail(call(&mut c, Call::Pwrite { fd, data, off }), |v| match v {
                Ret::N(n) => n as i32,
                _ => -1,
            })
        },
    )?;

    l.func_wrap("sys", "seek", |mut c: Caller<'_, Guest>, fd: i32, off: i64, whence: i32| {
        match call(&mut c, Call::Seek { fd, off, whence }) {
            Ok(Ret::N(n)) => n as i64,
            _ => -1,
        }
    })?;

    l.func_wrap("sys", "dup", |mut c: Caller<'_, Guest>, old: i32, new: i32| {
        or_fail(call(&mut c, Call::Dup { old, new }), |v| match v {
            Ret::Fd(fd) => fd,
            _ => -1,
        })
    })?;

    l.func_wrap("sys", "pipe", |mut c: Caller<'_, Guest>, p: i32| {
        let (a, b) = match call(&mut c, Call::Pipe) {
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
    })?;

    l.func_wrap("sys", "remove", |mut c: Caller<'_, Guest>, p: i32| {
        let Ok(path) = cstr(&mut c, p) else { return -1 };
        or_fail(call(&mut c, Call::Remove { path }), |_| 0)
    })?;

    l.func_wrap("sys", "chdir", |mut c: Caller<'_, Guest>, p: i32| {
        let Ok(path) = cstr(&mut c, p) else { return -1 };
        or_fail(call(&mut c, Call::Chdir { path }), |_| 0)
    })?;

    l.func_wrap("sys", "bind", |mut c: Caller<'_, Guest>, n: i32, o: i32, flag: i32| {
        let (Ok(name), Ok(old)) = (cstr(&mut c, n), cstr(&mut c, o)) else { return -1 };
        or_fail(call(&mut c, Call::Bind { name, old, flag }), |_| 0)
    })?;

    l.func_wrap(
        "sys",
        "mount",
        |mut c: Caller<'_, Guest>, fd: i32, afd: i32, o: i32, flag: i32, a: i32| {
            let Ok(old) = cstr(&mut c, o) else { return -1 };
            let aname = if a == 0 { String::new() } else { cstr(&mut c, a).unwrap_or_default() };
            or_fail(call(&mut c, Call::Mount { fd, afd, old, flag, aname }), |_| 0)
        },
    )?;

    l.func_wrap("sys", "unmount", |mut c: Caller<'_, Guest>, n: i32, o: i32| {
        let Ok(old) = cstr(&mut c, o) else { return -1 };
        let name = if n == 0 { None } else { Some(cstr(&mut c, n).unwrap_or_default()) };
        or_fail(call(&mut c, Call::Unmount { name, old }), |_| 0)
    })?;

    l.func_wrap("sys", "stat", |mut c: Caller<'_, Guest>, p: i32, e: i32, n: i32| {
        let Ok(path) = cstr(&mut c, p) else { return -1 };
        let r = call(&mut c, Call::Stat { path });
        statlike(&mut c, r, e, n)
    })?;

    l.func_wrap("sys", "fstat", |mut c: Caller<'_, Guest>, fd: i32, e: i32, n: i32| {
        let r = call(&mut c, Call::Fstat { fd });
        statlike(&mut c, r, e, n)
    })?;

    l.func_wrap("sys", "wstat", |mut c: Caller<'_, Guest>, p: i32, e: i32, n: i32| {
        let (Ok(path), Ok(edir)) = (cstr(&mut c, p), read(&mut c, e, n)) else { return -1 };
        or_fail(call(&mut c, Call::Wstat { path, edir }), |_| 0)
    })?;

    l.func_wrap("sys", "fwstat", |mut c: Caller<'_, Guest>, fd: i32, e: i32, n: i32| {
        let Ok(edir) = read(&mut c, e, n) else { return -1 };
        or_fail(call(&mut c, Call::Fwstat { fd, edir }), |_| 0)
    })?;

    l.func_wrap("sys", "fversion", |mut c: Caller<'_, Guest>, fd: i32, m: i32, v: i32, n: i32| {
        let Ok(version) = cstr(&mut c, v) else { return -1 };
        let _ = n;
        or_fail(
            call(&mut c, Call::Fversion { fd, msize: m as u32, version }),
            |_| 0,
        )
    })?;

    l.func_wrap("sys", "rfork", |mut c: Caller<'_, Guest>, flags: i32| {
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
        or_fail(call(&mut c, Call::Rfork { flags }), |v| match v {
            Ret::Pid(p) => p as i32,
            _ => -1,
        })
    })?;

    // `procrfork(f, arg, stacksize, rforkflag)` — Plan 9's own shape for
    // making a process that runs a FUNCTION (`libthread/create.c:103`), and
    // the one shape a machine without a duplicable stack can honour. The
    // kernel call underneath is `rfork`, unchanged; what differs is that the
    // child is told where to start instead of resuming a copy of the parent.
    //
    // It answers the child's pid — where libthread answers a thread id,
    // because there are threads there and none here.
    //
    // The child runs on the PARENT'S INSTANCE, which is `RFMEM`: one memory,
    // two processes, and only one of them running at a time. That is vfork's
    // discipline, and the kernel tables the child needs of its own — fds,
    // namespace, environment — are the kernel's, asked for by flag.
    l.func_wrap_async(
        "sys",
        "procrfork",
        |mut c: Caller<'_, Guest>, (f, arg, _stack, flags): (i32, i32, i32, i32)| {
            Box::new(async move {
                let child = match call(&mut c, Call::Rfork { flags: flags | rf::PROC | rf::MEM }) {
                    Ok(Ret::Pid(p)) => p,
                    _ => return -1i32,
                };
                let Some(start) = c.get_export("__childstart").and_then(|e| e.into_func()) else {
                    return -1i32;
                };
                let Ok(start) = start.typed::<(i32, i32), ()>(&c) else { return -1i32 };
                let parent = c.data().pid;
                c.data_mut().pid = child;
                // **The child runs here only as far as its `exec`**, and
                // that is the whole of `RFMEM`: one memory, and only one of
                // the two running at a time. The `exec` gives the child an
                // image and a fiber of its own, then unwinds these frames as
                // `Exited` — which is not a failure and is caught here, not
                // by the scheduler, because the frames belong to the parent.
                //
                // A child that exits instead of execing unwinds the same
                // way, having already told the kernel.
                let ended = start.call_async(&mut c, (f, arg)).await;
                // **A child that simply returns has ended.** Our own
                // `__childstart` calls `exits("child returned")` when the
                // function comes back (`libc/wasm/procrfork.c`), which is
                // what `libthread`'s `threadexits` does; a module that
                // exports its own must not leave a process the scheduler
                // will try to enter and find nothing of. `Err` here is
                // `exits` or `exec` unwinding, and both already told the
                // kernel.
                if ended.is_ok() {
                    let _ = call(&mut c, Call::Exits { status: String::new() });
                }
                c.data_mut().pid = parent;
                // **`ready(p); sched();`** — `sysrfork`'s last two lines
                // (`sysproc.c`). The kernel did the `ready`; this is the
                // `sched`, and it is not an optimisation. Without it the
                // parent runs on, and `m->readied` hands the processor to
                // whatever it forks NEXT — so the first stage of a pipeline
                // sits `Ready` until the shell that made it has exited, and
                // writes to a console that is gone.
                Sched::new().await;
                child as i32
            })
        },
    )?;

    l.func_wrap("sys", "exec", |mut c: Caller<'_, Guest>, p: i32, a: i32| -> Result<i32, wasmtime::Error> {
        let (Ok(path), Ok(args)) = (cstr(&mut c, p), cargv(&mut c, a)) else {
            return Ok(-1);
        };
        match call(&mut c, Call::Exec { path, args }) {
            // **`exec` does not return** (`sysproc.c:302`): the process IS
            // the new image now, and the frames of the old one are nobody's.
            // Unwinding them is what a machine with no address space to
            // discard does instead; the new image is a fiber the scheduler
            // will enter, not something that runs from in here.
            Ok(_) => Err(wasmtime::Error::new(Exited)),
            Err(_) => Ok(-1),
        }
    })?;

    l.func_wrap("sys", "exits", |mut c: Caller<'_, Guest>, p: i32| -> Result<(), wasmtime::Error> {
        // `exits(nil)` is the empty status, and nil is address zero.
        let status = if p == 0 { String::new() } else { cstr(&mut c, p).unwrap_or_default() };
        let _ = call(&mut c, Call::Exits { status });
        Err(wasmtime::Error::new(Exited))
    })?;

    // `pwait` (`proc.c:1288`) is `sleep(&up->waitr, haswaitq, up)` and then
    // taking the record. The loop is that: ask, and if the kernel says the
    // process must sleep, leave — and ask again when it is entered, which is
    // `sleep` returning and `haswaitq` being true at last.
    l.func_wrap_async("sys", "await", |mut c: Caller<'_, Guest>, (p, n): (i32, i32)| {
        Box::new(async move {
            let msg = loop {
                match call(&mut c, Call::Await) {
                    Ok(Ret::Str(s)) => break s,
                    Ok(Ret::Sched) => Sched::new().await,
                    _ => return -1,
                }
            };
            let b = msg.as_bytes();
            let k = b.len().min(n.max(0) as usize);
            match write(&mut c, p, &b[..k]) {
                Ok(()) => k as i32,
                Err(_) => -1,
            }
        })
    })?;

    l.func_wrap("sys", "errstr", |mut c: Caller<'_, Guest>, p: i32, n: i32| {
        // `generrstr` (`sysproc.c:748`) EXCHANGES and answers 0, never a
        // length: what the buffer held becomes the process's error string and
        // the old one is written back. `werrstr` is that, and nothing else.
        let Ok(buf) = cstr(&mut c, p) else { return -1 };
        let old = match call(&mut c, Call::Errstr { buf }) {
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
    })?;

    // `syssleep` ends in `tsleep`, which ends in `sched()`. The kernel has
    // put the process on its own `Rendez` with a deadline; this is the going.
    l.func_wrap_async("sys", "sleep", |mut c: Caller<'_, Guest>, (ms,): (i32,)| {
        Box::new(async move {
            match call(&mut c, Call::Sleep { ms: ms.max(0) as u64 }) {
                Ok(Ret::Sched) => {
                    Sched::new().await;
                    0
                }
                Ok(_) => 0,
                Err(_) => -1,
            }
        })
    })?;

    l.func_wrap("sys", "alarm", |mut c: Caller<'_, Guest>, ms: i32| {
        or_fail(call(&mut c, Call::Alarm { ms: ms.max(0) as u64 }), |_| 0) as i64
    })?;

    // The three the kernel refuses. They are imports all the same, so a
    // program that calls one gets −1 and the kernel's reason in `errstr` —
    // which is what a Plan 9 program does with a call that fails, and is
    // not the same as a program that would not link.
    l.func_wrap("sys", "notify", |mut c: Caller<'_, Guest>, _f: i32| {
        or_fail(call(&mut c, Call::Notify), |_| 0)
    })?;
    l.func_wrap("sys", "noted", |mut c: Caller<'_, Guest>, v: i32| {
        or_fail(call(&mut c, Call::Noted { how: v }), |_| 0)
    })?;
    l.func_wrap("sys", "rendezvous", |mut c: Caller<'_, Guest>, tag: i32, val: i32| {
        or_fail(
            call(&mut c, Call::Rendezvous { tag: tag as u64, val: val as u64 }),
            |_| 0,
        )
    })?;

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
