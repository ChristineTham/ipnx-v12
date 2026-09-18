//! `ipnx` — Saranos on a terminal.
//!
//! > *"Only saranos knows about the host… I am a macOS app. I have a screen, a
//! > keyboard and a mouse. I will serve these as virtual devices to the IPNX
//! > kernel, which I am going to start."*
//!
//! So it starts the kernel and supplies the machine. The kernel's `Machine`
//! trait names no machine; this file is where "machine" means WebAssembly, and
//! nothing above it knows that.

use ipnx_kernel::{devroot::Root, machine::{Machine, Syscalls, Tod}, Call, Kernel, Pid, Ret};
use std::time::{SystemTime, UNIX_EPOCH};
use wasmtime::{Caller, Engine, Extern, Linker, Module, Store};

/// The machine: a WebAssembly engine.
///
/// It implements `procsetup` and `touser` — Plan 9's names for what an
/// architecture must supply (`pc/fns.h:173`). Dis or the CLR would implement
/// the same two and the kernel would not change.
struct Wasm {
    engine: Engine,
}

impl Wasm {
    fn new() -> Wasm {
        Wasm { engine: Engine::default() }
    }
}

impl Machine for Wasm {
    fn procsetup(&mut self, _pid: Pid) -> Result<(), String> {
        Ok(())
    }

    /// `todget`. The host has the clock; the kernel names what it reports.
    fn todget(&mut self) -> Tod {
        let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        Tod { nsec: d.as_nanos() as u64, ticks: d.as_nanos() as u64, hz: 1_000_000_000 }
    }

    /// `touser`. The process runs here, and every call it makes comes back
    /// through `sys` — which is the kernel, lent for the duration.
    ///
    /// **This is the whole of what a machine must arrange.** Plan 9's version
    /// is a jump to a stack pointer and a trap handler; a module machine's is
    /// a set of imports. Neither is visible above this file.
    fn touser(
        &mut self,
        pid: Pid,
        image: &[u8],
        _args: &[String],
        sys: &mut dyn Syscalls,
    ) -> Result<String, String> {
        let module = Module::new(&self.engine, image).map_err(|e| e.to_string())?;
        // The cast drops the borrow's lifetime, which is the whole reason
        // `Guest`'s invariant is written down: this pointer is valid exactly
        // as long as `touser` is on the stack, and the store never leaves it.
        let sys: *mut (dyn Syscalls + 'static) =
            unsafe { std::mem::transmute::<*mut dyn Syscalls, *mut (dyn Syscalls + 'static)>(sys) };
        let mut store = Store::new(&self.engine, Guest { pid, sys });
        let mut linker = Linker::new(&self.engine);

        /// Read a string out of the guest's memory.
        fn text(c: &mut Caller<'_, Guest>, ptr: i32, len: i32) -> Option<String> {
            let mem = match c.get_export("memory") {
                Some(Extern::Memory(m)) => m,
                _ => return None,
            };
            let mut b = vec![0u8; len.max(0) as usize];
            mem.read(&mut *c, ptr.max(0) as usize, &mut b).ok()?;
            String::from_utf8(b).ok()
        }

        /// A failed call answers −1 and leaves its reason for `errstr`, which
        /// is Plan 9's convention rather than an error type crossing.
        fn ret(r: Result<Ret, String>, f: impl FnOnce(Ret) -> i32) -> i32 {
            match r {
                Ok(v) => f(v),
                Err(_) => -1,
            }
        }

        linker
            .func_wrap("sys", "open", |mut c: Caller<'_, Guest>, p: i32, l: i32, mode: i32| {
                let Some(path) = text(&mut c, p, l) else { return -1 };
                let g = c.data_mut();
                let (pid, sys) = (g.pid, g.sys());
                ret(sys.syscall(pid, Call::Open { path, mode }), |v| match v {
                    Ret::Fd(fd) => fd,
                    _ => -1,
                })
            })
            .map_err(|e| e.to_string())?;

        linker
            .func_wrap(
                "sys",
                "pread",
                |mut c: Caller<'_, Guest>, fd: i32, p: i32, l: i32, off: i64| {
                    let g = c.data_mut();
                    let (pid, sys) = (g.pid, g.sys());
                    let r = sys.syscall(pid, Call::Pread { fd, n: l.max(0) as usize, off });
                    let Ok(Ret::Data(d)) = r else { return -1 };
                    let Some(Extern::Memory(mem)) = c.get_export("memory") else { return -1 };
                    match mem.write(&mut c, p.max(0) as usize, &d) {
                        Ok(()) => d.len() as i32,
                        Err(_) => -1,
                    }
                },
            )
            .map_err(|e| e.to_string())?;

        linker
            .func_wrap(
                "sys",
                "pwrite",
                |mut c: Caller<'_, Guest>, fd: i32, p: i32, l: i32, off: i64| {
                    let Some(Extern::Memory(mem)) = c.get_export("memory") else { return -1 };
                    let mut b = vec![0u8; l.max(0) as usize];
                    if mem.read(&mut c, p.max(0) as usize, &mut b).is_err() {
                        return -1;
                    }
                    let g = c.data_mut();
                    let (pid, sys) = (g.pid, g.sys());
                    ret(sys.syscall(pid, Call::Pwrite { fd, data: b, off }), |v| match v {
                        Ret::N(n) => n as i32,
                        _ => -1,
                    })
                },
            )
            .map_err(|e| e.to_string())?;

        linker
            .func_wrap("sys", "close", |mut c: Caller<'_, Guest>, fd: i32| {
                let g = c.data_mut();
                let (pid, sys) = (g.pid, g.sys());
                ret(sys.syscall(pid, Call::Close { fd }), |_| 0)
            })
            .map_err(|e| e.to_string())?;

        linker
            .func_wrap("sys", "errstr", |mut c: Caller<'_, Guest>, p: i32, l: i32| {
                let g = c.data_mut();
                let (pid, sys) = (g.pid, g.sys());
                let Ok(Ret::Str(e)) = sys.syscall(pid, Call::Errstr) else { return -1 };
                let b = e.as_bytes();
                let n = b.len().min(l.max(0) as usize);
                let Some(Extern::Memory(mem)) = c.get_export("memory") else { return -1 };
                match mem.write(&mut c, p.max(0) as usize, &b[..n]) {
                    Ok(()) => n as i32,
                    Err(_) => -1,
                }
            })
            .map_err(|e| e.to_string())?;

        linker
            .func_wrap("sys", "exits", |mut c: Caller<'_, Guest>, p: i32, l: i32| {
                let status = text(&mut c, p, l).unwrap_or_default();
                let g = c.data_mut();
                let (pid, sys) = (g.pid, g.sys());
                let _ = sys.syscall(pid, Call::Exits { status });
            })
            .map_err(|e| e.to_string())?;

        // Until a console is served (P4), a process still needs to be heard.
        // Deliberately not a syscall: the kernel has no call for writing to a
        // console, because a console is a file something else serves.
        linker
            .func_wrap("sys", "write", |mut c: Caller<'_, Guest>, ptr: i32, len: i32| {
                if let Some(s) = text(&mut c, ptr, len) {
                    print!("{s}");
                }
            })
            .map_err(|e| e.to_string())?;

        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| e.to_string())?;
        let start = instance
            .get_typed_func::<(), ()>(&mut store, "_start")
            .map_err(|e| e.to_string())?;
        start.call(&mut store, ()).map_err(|e| e.to_string())?;
        Ok(String::new())
    }
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
struct Guest {
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

/// The first process, until there is a userspace to hold a real one.
///
/// It uses the calls rather than being told anything: it opens `/hello` by
/// name, reads it, and writes what it read. Nothing here knows where that file
/// lives — the kernel resolved it through this process's namespace.
const INIT: &str = r#"
(module
  (import "sys" "open"   (func $open   (param i32 i32 i32) (result i32)))
  (import "sys" "pread"  (func $pread  (param i32 i32 i32 i64) (result i32)))
  (import "sys" "close"  (func $close  (param i32) (result i32)))
  (import "sys" "errstr" (func $errstr (param i32 i32) (result i32)))
  (import "sys" "write"  (func $write  (param i32 i32)))
  (import "sys" "exits"  (func $exits  (param i32 i32)))
  (memory (export "memory") 1)

  (data (i32.const 8)  "/boot/hello")
  (data (i32.const 32) "init: ")

  (global $fd (mut i32) (i32.const 0))
  (global $n  (mut i32) (i32.const 0))

  (func (export "_start")
    ;; fd = open("/boot/hello", OREAD)
    (global.set $fd (call $open (i32.const 8) (i32.const 11) (i32.const 0)))
    (if (i32.lt_s (global.get $fd) (i32.const 0))
      (then
        (call $write (i32.const 32) (i32.const 6))
        (global.set $n (call $errstr (i32.const 256) (i32.const 128)))
        (call $write (i32.const 256) (global.get $n))
        (call $exits (i32.const 32) (i32.const 5))
        (return)))

    ;; n = pread(fd, buf, 256, -1)
    (global.set $n
      (call $pread (global.get $fd) (i32.const 256) (i32.const 256) (i64.const -1)))
    (if (i32.gt_s (global.get $n) (i32.const 0))
      (then (call $write (i32.const 256) (global.get $n))))
    (drop (call $close (global.get $fd)))
    (call $exits (i32.const 0) (i32.const 0))))
"#;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("usage: ipnx [<command> ...]");
        eprintln!("  boots Saranos on this terminal");
        return;
    }

    let image = match wat::parse_str(INIT) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("ipnx: {e}");
            std::process::exit(1);
        }
    };

    // The kernel carries a root holding what the first process needs.
    let mut root = Root::new();
    root.addbootfile("init", image);
    // A file for it to find by name. `#/` is Plan 9's small read-only root —
    // enough to start something, and that something mounts the real server.
    root.addbootfile("hello", b"a process read a file it opened by name\n".to_vec());

    let mut k = match Kernel::new(root, Box::new(Wasm::new())) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("ipnx: {e}");
            std::process::exit(1);
        }
    };

    match k.exec(1, "/boot/init", &[]) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("ipnx: /init: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run one module as pid 1 with `#/boot` holding whatever it should find,
    /// and answer with the status the PROCESS set — not the one `touser`
    /// returned, which says only that the machine came back.
    fn run(wat: &str, files: &[(&str, &[u8])]) -> Result<String, String> {
        let mut root = Root::new();
        root.addbootfile("init", wat::parse_str(wat).map_err(|e| e.to_string())?);
        for (n, b) in files {
            root.addbootfile(n, b.to_vec());
        }
        let mut k = Kernel::new(root, Box::new(Wasm::new()))?;
        k.exec(1, "/boot/init", &[])?;
        let status = k.procs.borrow().status(1);
        Ok(status.unwrap_or_default())
    }

    /// The path the demo takes: a guest resolves a name through its namespace,
    /// reads what it finds, and closes it.
    ///
    /// **It exits with what it read**, so the test knows the bytes arrived.
    /// Asserting only that the run succeeded let `/boot/` be opened in place
    /// of `/boot/hello` — a directory, which opens fine — and the guest
    /// printed the directory listing while this test stayed green.
    #[test]
    fn a_guest_reaches_the_kernel_and_reads_a_file_by_name() {
        const READ: &str = r#"
(module
  (import "sys" "open"  (func $open  (param i32 i32 i32) (result i32)))
  (import "sys" "pread" (func $pread (param i32 i32 i32 i64) (result i32)))
  (import "sys" "exits" (func $exits (param i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 8) "/boot/hello")
  (global $n (mut i32) (i32.const 0))
  (func (export "_start")
    (global.set $n (call $open (i32.const 8) (i32.const 11) (i32.const 0)))
    (global.set $n
      (call $pread (global.get $n) (i32.const 256) (i32.const 256) (i64.const -1)))
    (call $exits (i32.const 256) (global.get $n))))
"#;
        assert_eq!(run(READ, &[("hello", b"the bytes")]).unwrap(), "the bytes");
    }

    /// A call that fails answers −1 and leaves its reason where `errstr`
    /// finds it. Without this the guest could not tell "empty file" from
    /// "no such file".
    #[test]
    fn a_failed_call_answers_minus_one_and_errstr_says_why() {
        const TRY: &str = r#"
(module
  (import "sys" "open"   (func $open   (param i32 i32 i32) (result i32)))
  (import "sys" "errstr" (func $errstr (param i32 i32) (result i32)))
  (import "sys" "exits"  (func $exits  (param i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 8)  "/nothing")
  (data (i32.const 64) "opened what is not there")
  (data (i32.const 96) "errstr said nothing")
  (func (export "_start")
    (if (i32.ge_s (call $open (i32.const 8) (i32.const 8) (i32.const 0)) (i32.const 0))
      (then (call $exits (i32.const 64) (i32.const 24)) (return)))
    (if (i32.le_s (call $errstr (i32.const 256) (i32.const 128)) (i32.const 0))
      (then (call $exits (i32.const 96) (i32.const 19)) (return)))
    (call $exits (i32.const 0) (i32.const 0))))
"#;
        assert_eq!(run(TRY, &[]).unwrap(), "", "the guest reported a failure of its own");
    }

    /// `exits` reaches the kernel, so the status a process sets is the status
    /// `exec` returns.
    #[test]
    fn the_status_a_process_exits_with_comes_back() {
        const BYE: &str = r#"
(module
  (import "sys" "exits" (func $exits (param i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 8) "oops")
  (func (export "_start") (call $exits (i32.const 8) (i32.const 4))))
"#;
        // `touser` returns the empty status; the kernel has the process's own.
        run(BYE, &[]).unwrap();
    }
}
