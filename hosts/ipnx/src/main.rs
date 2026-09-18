//! `ipnx` — Saranos on a terminal.
//!
//! > *"Only saranos knows about the host… I am a macOS app. I have a screen, a
//! > keyboard and a mouse. I will serve these as virtual devices to the IPNX
//! > kernel, which I am going to start."*
//!
//! So it starts the kernel and supplies the machine. The kernel's `Machine`
//! trait names no machine; this file is where "machine" means WebAssembly, and
//! nothing above it knows that.

use ipnx_kernel::{devroot::Root, machine::Machine, Kernel, Pid};
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

    fn touser(&mut self, _pid: Pid, image: &[u8], _args: &[String]) -> Result<String, String> {
        let module = Module::new(&self.engine, image).map_err(|e| e.to_string())?;
        let mut store = Store::new(&self.engine, ());
        let mut linker = Linker::new(&self.engine);

        // The one thing this process can do so far. It is deliberately not a
        // syscall: the kernel has no call for writing to a console, because a
        // console is served, not held. This is the machine lending the process
        // a way to be heard at all until there is a server to talk to.
        linker
            .func_wrap(
                "sys",
                "write",
                |mut caller: Caller<'_, ()>, ptr: i32, len: i32| {
                    let mem = match caller.get_export("memory") {
                        Some(Extern::Memory(m)) => m,
                        _ => return,
                    };
                    let mut buf = vec![0u8; len.max(0) as usize];
                    if mem.read(&mut caller, ptr.max(0) as usize, &mut buf).is_ok() {
                        print!("{}", String::from_utf8_lossy(&buf));
                    }
                },
            )
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

/// The first process, until there is a userspace to hold a real one.
const INIT: &str = r#"
(module
  (import "sys" "write" (func $write (param i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 8) "a process ran, and said so\n")
  (func (export "_start")
    (call $write (i32.const 8) (i32.const 26))))
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

    let mut k = match Kernel::new(root, Box::new(Wasm::new())) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("ipnx: {e}");
            std::process::exit(1);
        }
    };

    match k.exec(1, "/init", &[]) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("ipnx: /init: {e}");
            std::process::exit(1);
        }
    }
}
