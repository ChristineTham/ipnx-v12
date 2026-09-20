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

use ipnx_kernel::machine::{Machine, Syscalls, Tod};
use ipnx_kernel::proc::rf;
use ipnx_kernel::{Call, Pid, Ret};
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
}

/// What the store carries while a process runs: which process it is, and the
/// kernel to call. Plan 9 keeps the first in `up` and needs nothing at all for
/// the second, because its kernel is reachable from anywhere.
///
/// The kernel is held as a pointer because wasmtime's `Store<T>` requires
/// `T: 'static` and this borrow is not. **The invariant that makes it sound:**
/// the store is created inside `touser` and dropped before it returns, and
/// `sys` is borrowed for the whole of that call, so the pointer cannot outlive
/// what it points at. Nothing else may construct a `Guest`.
pub struct Guest {
    /// The process making the calls. It is not constant for the life of the
    /// store: during a `procrfork` the child runs on this instance, and every
    /// call it makes is the child's.
    pid: Pid,
    sys: *mut dyn Syscalls,
}

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
        let engine = Engine::default();
        let mut linker: Linker<Guest> = Linker::new(&engine);
        imports(&mut linker).map_err(|e| e.to_string())?;
        Ok(Wasm { engine, linker })
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

    fn touser(
        &self,
        pid: Pid,
        image: &[u8],
        args: &[String],
        sys: &mut dyn Syscalls,
    ) -> Result<String, String> {
        let module = Module::new(&self.engine, image).map_err(|e| e.to_string())?;
        // The cast drops the borrow's lifetime, which is the whole reason
        // `Guest`'s invariant is written down: this pointer is valid exactly
        // as long as `touser` is on the stack, and the store never leaves it.
        let sys: *mut (dyn Syscalls + 'static) =
            unsafe { std::mem::transmute::<*mut dyn Syscalls, *mut (dyn Syscalls + 'static)>(sys) };
        let mut store = Store::new(&self.engine, Guest { pid, sys });
        let instance = self
            .linker
            .instantiate(&mut store, &module)
            .map_err(|e| e.to_string())?;
        let (argc, argv, heap) =
            place(&mut store, &instance, args).map_err(|e| e.to_string())?;
        let start: TypedFunc<(i32, i32, i32), ()> = instance
            .get_typed_func(&mut store, "_start")
            .map_err(|e| e.to_string())?;
        match start.call(&mut store, (argc, argv, heap)) {
            Ok(()) => Ok(String::new()),
            // `exits` and a finished `exec` unwind this way, and both are the
            // process ending normally. The status is the kernel's — the
            // process gave it to `exits` before the unwind.
            Err(e) if e.downcast_ref::<Exited>().is_some() => Ok(String::new()),
            Err(e) => Err(e.to_string()),
        }
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
    l.func_wrap(
        "sys",
        "procrfork",
        |mut c: Caller<'_, Guest>, f: i32, arg: i32, _stack: i32, flags: i32| {
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
            // Whatever the child does, it is finished when this returns: it
            // exits, or it execs and the new image runs to its own end. Both
            // unwind as `Exited`, which is not a failure.
            let _ = start.call(&mut c, (f, arg));
            c.data_mut().pid = parent;
            child as i32
        },
    )?;

    l.func_wrap("sys", "exec", |mut c: Caller<'_, Guest>, p: i32, a: i32| -> Result<i32, wasmtime::Error> {
        let (Ok(path), Ok(args)) = (cstr(&mut c, p), cargv(&mut c, a)) else {
            return Ok(-1);
        };
        match call(&mut c, Call::Exec { path, args }) {
            // `exec` does not return (`sysproc.c:302`): the process IS the
            // new image now. Here the new image has already run to its end,
            // so what is left is to unwind the caller that no longer exists.
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

    l.func_wrap("sys", "await", |mut c: Caller<'_, Guest>, p: i32, n: i32| {
        let msg = match call(&mut c, Call::Await) {
            Ok(Ret::Str(s)) => s,
            _ => return -1,
        };
        let b = msg.as_bytes();
        let k = b.len().min(n.max(0) as usize);
        match write(&mut c, p, &b[..k]) {
            Ok(()) => k as i32,
            Err(_) => -1,
        }
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

    l.func_wrap("sys", "sleep", |mut c: Caller<'_, Guest>, ms: i32| {
        or_fail(call(&mut c, Call::Sleep { ms: ms.max(0) as u64 }), |_| 0)
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
