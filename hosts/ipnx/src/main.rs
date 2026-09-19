//! `ipnx` — Saranos on a terminal.
//!
//! > *"Only saranos knows about the host… I am a macOS app. I have a screen, a
//! > keyboard and a mouse. I will serve these as virtual devices to the IPNX
//! > kernel, which I am going to start."*
//!
//! So it starts the kernel, gives it a machine ([`machine`]), and builds the
//! device table — which is what a Plan 9 kernel's configuration file does:
//! `mkdevc` reads the `dev` list and emits `Dev* devtab[]`. The letters are
//! chosen here and nowhere else.

mod machine;

use ipnx_kernel::{
    devcap::CapDev,
    devcons::{Cons, Console},
    devdup::DupDev,
    devenv::EnvDev,
    devmnt::MntDev,
    devpipe::PipeDev,
    devproc::ProcDev,
    devroot::Root,
    devsrv::SrvDev,
    dev::DevId,
    Call, Kernel, Ret,
};
use std::rc::Rc;

/// What `#c` reports about the machine underneath. The kernel names these
/// files; only the host can fill them, which is the arrangement Plan 9 has for
/// every number `devcons` reports — it reads them from the architecture.
struct Host;

impl Console for Host {
    fn now(&mut self) -> (u64, u64, u64) {
        let d = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        (d.as_nanos() as u64, d.as_nanos() as u64, 1_000_000_000)
    }

    fn random(&mut self, n: usize) -> Vec<u8> {
        // The host's entropy. `/dev/random` is `#c`'s (`devcons.c`), and the
        // bytes are the machine's wherever it runs.
        let mut out = Vec::with_capacity(n);
        let mut x = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x2545F4914F6CDD1D)
            | 1;
        for _ in 0..n {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            out.push(x as u8);
        }
        out
    }

    fn drivers(&mut self) -> Vec<String> {
        vec!["wasmtime: the machine".into(), "terminal: the surface".into()]
    }

    fn memory(&mut self) -> (u64, u64, u64) {
        (0, 64 * 1024, 0)
    }

    fn config(&mut self) -> String {
        std::env::args().collect::<Vec<_>>().join(" ")
    }

    fn reboot(&mut self, cmd: &str) -> Result<(), String> {
        match cmd {
            "halt" => std::process::exit(0),
            _ => Err("this host reboots by being started again".into()),
        }
    }
}

/// The device table — the letters this kernel carries. Plan 9 writes the same
/// list in a configuration file and `mkdevc` turns it into `devtab[]`.
///
/// Absent, and for one reason each: `#i` (draw) and `#m` (mouse) are hardware
/// this machine has none of.
const LETTERS: [DevId; 9] = [
    DevId::Root,
    DevId::Pipe,
    DevId::Srv,
    DevId::Mnt,
    DevId::Proc,
    DevId::Dup,
    DevId::Env,
    DevId::Cons,
    DevId::Cap,
];

fn boot(root: Root) -> Result<Kernel, String> {
    let m = machine::Wasm::new()?;
    let mut k = Kernel::new(root, Rc::new(m))?;
    k.tab.add(Box::new(PipeDev::new()));
    k.tab.add(Box::new(SrvDev::new(k.up.clone())));
    k.tab.add(Box::new(MntDev::new()));
    k.tab.add(Box::new(ProcDev::new(k.up.clone())));
    k.tab.add(Box::new(DupDev::new(k.up.clone())));
    k.tab.add(Box::new(EnvDev::new(k.up.clone())));
    k.tab.add(Box::new(CapDev::new(k.up.clone())));
    k.tab.add(Box::new(Cons::new(
        "eve",
        k.up.clone(),
        LETTERS.to_vec(),
        Box::new(Host),
    )));
    Ok(k)
}

/// Load `userspace/root/bin` into `#/boot`, which is where a Plan 9 kernel
/// keeps the files a first process needs (`devroot.c:27`: `addbootfile`) —
/// enough to start something, and that something mounts the real server.
fn loadbin(root: &mut Root, dir: &std::path::Path) -> usize {
    let mut n = 0;
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    for e in entries.flatten() {
        let Ok(bytes) = std::fs::read(e.path()) else { continue };
        let Some(name) = e.file_name().to_str().map(str::to_string) else { continue };
        root.addbootfile(&name, bytes);
        n += 1;
    }
    n
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("usage: ipnx [<command> [<arg> ...]]");
        eprintln!("  boots Saranos on this terminal and runs one command");
        return;
    }

    let mut root = Root::new();
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../userspace/root/bin");
    if loadbin(&mut root, &here) == 0 {
        eprintln!("ipnx: nothing in {}: run userspace/mk.sh", here.display());
        std::process::exit(1);
    }

    let mut k = match boot(root) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("ipnx: {e}");
            std::process::exit(1);
        }
    };

    // `#/boot` onto `/bin`, so a name resolves the way every command expects.
    // Plan 9's own first process does this with binds and nothing else
    // (`init.c`, `termrc`); a `/namespace` file that says so is P5.
    if let Err(e) = k.syscall(
        1,
        Call::Bind { name: "#/boot".into(), old: "/bin".into(), flag: 0 },
    ) {
        eprintln!("ipnx: bind #/boot /bin: {e}");
        std::process::exit(1);
    }

    // Until a console is served (P4) a process still needs to be heard, and
    // fd 1 must be something. A pipe is something: the kernel's own `#|`,
    // drained here when the process is done. Nothing about it is a console —
    // it cannot be read from, and it arrives all at once.
    let out = match k.syscall(1, Call::Pipe) {
        Ok(Ret::Two(a, b)) => {
            // `pipe` answers the two LOWEST free descriptors, which on a
            // fresh process are 0 and 1 — so the write end must be moved
            // before 1 and 2 are made to point at it, or closing the old
            // name closes the new one. (It did: `echo` wrote to a closed
            // descriptor and this printed nothing.)
            let Ok(Ret::Fd(w)) = k.syscall(1, Call::Dup { old: b, new: 15 }) else {
                eprintln!("ipnx: no descriptors");
                std::process::exit(1);
            };
            let Ok(Ret::Fd(r)) = k.syscall(1, Call::Dup { old: a, new: 14 }) else {
                eprintln!("ipnx: no descriptors");
                std::process::exit(1);
            };
            let _ = k.syscall(1, Call::Close { fd: a });
            let _ = k.syscall(1, Call::Close { fd: b });
            let _ = k.syscall(1, Call::Dup { old: w, new: 1 });
            let _ = k.syscall(1, Call::Dup { old: w, new: 2 });
            let _ = k.syscall(1, Call::Close { fd: w });
            r
        }
        _ => {
            eprintln!("ipnx: no pipe");
            std::process::exit(1);
        }
    };

    let argv: Vec<String> = if args.len() > 1 {
        args[1..].to_vec()
    } else {
        vec!["rc".to_string()]
    };
    let path = format!("/bin/{}", argv[0]);
    let status = k.exec(1, &path, &argv);

    // Drain what the process wrote. The write end is the process's, and it is
    // gone; what is in the pipe is what it said.
    let _ = k.syscall(1, Call::Close { fd: 1 });
    let _ = k.syscall(1, Call::Close { fd: 2 });
    loop {
        match k.syscall(1, Call::Pread { fd: out, n: 8192, off: -1 }) {
            Ok(Ret::Data(d)) if !d.is_empty() => {
                print!("{}", String::from_utf8_lossy(&d));
            }
            _ => break,
        }
    }

    match status {
        Ok(s) if s.is_empty() => {}
        Ok(s) => {
            eprintln!("ipnx: {}: {s}", argv[0]);
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("ipnx: {}: {e}", argv[0]);
            std::process::exit(1);
        }
    }
}
