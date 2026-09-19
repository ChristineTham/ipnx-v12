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

/// The boot namespace. `#/boot` carries the commands until there is a file
/// server (P5); the rest are the devices at the names every Plan 9 program
/// expects to find them under.
const BINDS: [(&str, &str); 5] = [
    ("#/boot", "/bin"),
    ("#c", "/dev"),
    ("#e", "/env"),
    ("#d", "/fd"),
    ("#p", "/proc"),
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

/// Boot, run one command, and answer what it exited with and what it wrote.
///
/// This is the whole of `ipnx`, and it is a function rather than `main`'s body
/// so that a test can run the real thing — the real kernel, the real
/// namespace, the real binaries out of `userspace/root/bin`.
///
/// `extra` puts more files in the boot list, which is how a test gives rc a
/// script to run without writing into a built tree.
fn run(argv: &[String], extra: &[(&str, &[u8])]) -> Result<(String, String), String> {
    let mut root = Root::new();
    let bin = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../userspace/root/bin");
    if loadbin(&mut root, &bin) == 0 {
        return Err(format!("nothing in {}: run userspace/mk.sh", bin.display()));
    }
    for (name, bytes) in extra {
        root.addbootfile(name, bytes.to_vec());
    }

    let mut k = boot(root)?;

    // The boot namespace, built with binds and nothing else — which is what
    // Plan 9's first process does (`init.c`, and `/rc/bin/termrc` after it).
    // Every one of these is a device onto the empty directory `rootreset`
    // made for it. A `/namespace` file that says this instead is P5.
    for (what, at) in BINDS {
        k.syscall(1, Call::Bind { name: what.into(), old: at.into(), flag: 0 })
            .map_err(|e| format!("bind {what} {at}: {e}"))?;
    }

    // Until a console is served (P4) a process still needs to be heard, and
    // fd 1 must be something. A pipe is something: the kernel's own `#|`,
    // drained here when the process is done. Nothing about it is a console —
    // it cannot be read from, and it arrives all at once.
    let Ok(Ret::Two(a, b)) = k.syscall(1, Call::Pipe) else {
        return Err("no pipe".into());
    };
    // `pipe` answers the two LOWEST free descriptors, which on a fresh
    // process are 0 and 1 — so the write end must be moved before 1 and 2 are
    // made to point at it, or closing the old name closes the new one. (It
    // did: `echo` wrote to a closed descriptor and nothing printed.)
    let Ok(Ret::Fd(w)) = k.syscall(1, Call::Dup { old: b, new: 15 }) else {
        return Err("no descriptors".into());
    };
    let Ok(Ret::Fd(r)) = k.syscall(1, Call::Dup { old: a, new: 14 }) else {
        return Err("no descriptors".into());
    };
    let _ = k.syscall(1, Call::Close { fd: a });
    let _ = k.syscall(1, Call::Close { fd: b });
    let _ = k.syscall(1, Call::Dup { old: w, new: 1 });
    let _ = k.syscall(1, Call::Dup { old: w, new: 2 });
    let _ = k.syscall(1, Call::Close { fd: w });

    // **argv[0] is the PATH, not the bare name.** Plan 9's init passes "rc"
    // (`init.c`), and can, because rc forks there. rc cannot fork here, so it
    // re-executes ITSELF for every pipeline stage and subshell
    // (`haventfork.c`, which is Plan 9's own file for systems in exactly this
    // position) — and it finds itself by `argv0`. A bare name leaves it
    // unable to start its own left-hand side, silently.
    let path = format!("/bin/{}", argv[0]);
    let mut argv = argv.to_vec();
    argv[0] = path.clone();
    let status = k.exec(1, &path, &argv);

    // Drain what the process wrote. The write end was the process's, and it
    // is gone; what is in the pipe is what it said.
    let _ = k.syscall(1, Call::Close { fd: 1 });
    let _ = k.syscall(1, Call::Close { fd: 2 });
    let mut out = Vec::new();
    while let Ok(Ret::Data(d)) = k.syscall(1, Call::Pread { fd: r, n: 8192, off: -1 }) {
        if d.is_empty() {
            break;
        }
        out.extend_from_slice(&d);
    }
    Ok((status?, String::from_utf8_lossy(&out).into_owned()))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("usage: ipnx [<command> [<arg> ...]]");
        eprintln!("  boots Saranos on this terminal and runs one command");
        return;
    }
    let argv: Vec<String> =
        if args.len() > 1 { args[1..].to_vec() } else { vec!["rc".to_string()] };

    match run(&argv, &[]) {
        Ok((status, out)) => {
            print!("{out}");
            if !status.is_empty() {
                eprintln!("ipnx: {}: {status}", argv[0]);
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("ipnx: {}: {e}", argv[0]);
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
        let mut k = boot(root)?;
        k.exec(1, "/boot/init", &["init".to_string()])?;
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
  (import "sys" "open"  (func $open  (param i32 i32) (result i32)))
  (import "sys" "pread" (func $pread (param i32 i32 i32 i64) (result i32)))
  (import "sys" "exits" (func $exits (param i32)))
  (memory (export "memory") 1)
  (data (i32.const 8) "/boot/hello\00")
  (data (i32.const 512) "\00")
  (global $n (mut i32) (i32.const 0))
  (func (export "_start") (param i32 i32 i32)
    (global.set $n (call $open (i32.const 8) (i32.const 0)))
    (global.set $n
      (call $pread (global.get $n) (i32.const 256) (i32.const 256) (i64.const -1)))
    ;; NUL-terminate what was read, since `exits` takes a string
    (i32.store8 (i32.add (i32.const 256) (global.get $n)) (i32.const 0))
    (call $exits (i32.const 256))))
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
  (import "sys" "open"   (func $open   (param i32 i32) (result i32)))
  (import "sys" "errstr" (func $errstr (param i32 i32) (result i32)))
  (import "sys" "exits"  (func $exits  (param i32)))
  (memory (export "memory") 1)
  (data (i32.const 8)  "/nothing\00")
  (data (i32.const 64) "opened what is not there\00")
  (data (i32.const 96) "errstr said nothing\00")
  (func (export "_start") (param i32 i32 i32)
    (if (i32.ge_s (call $open (i32.const 8) (i32.const 0)) (i32.const 0))
      (then (call $exits (i32.const 64)) (return)))
    ;; `errstr` answers 0 and EXCHANGES; what it wrote is at 256
    (if (i32.ne (call $errstr (i32.const 256) (i32.const 128)) (i32.const 0))
      (then (call $exits (i32.const 96)) (return)))
    (if (i32.eqz (i32.load8_u (i32.const 256)))
      (then (call $exits (i32.const 96)) (return)))
    (call $exits (i32.const 0))))
"#;
        assert_eq!(run(TRY, &[]).unwrap(), "", "the guest reported a failure of its own");
    }

    /// `exits` reaches the kernel, so the status a process sets is the status
    /// `exec` answers.
    #[test]
    fn the_status_a_process_exits_with_comes_back() {
        const BYE: &str = r#"
(module
  (import "sys" "exits" (func $exits (param i32)))
  (memory (export "memory") 1)
  (data (i32.const 8) "oops\00")
  (func (export "_start") (param i32 i32 i32) (call $exits (i32.const 8))))
"#;
        assert_eq!(run(BYE, &[]).unwrap(), "oops");
    }

    /// The arguments arrive. `sysexec` copies argv onto the new process's
    /// stack; this machine writes the block into the module's own memory and
    /// hands the entry its address. The guest exits with `argv[1]`, so a
    /// block that is one pointer out fails here rather than in a shell.
    #[test]
    fn a_process_is_given_its_arguments() {
        const ARGS: &str = r#"
(module
  (import "sys" "exits" (func $exits (param i32)))
  (memory (export "memory") 1)
  (func (export "_start") (param $argc i32) (param $argv i32) (param $heap i32)
    (call $exits (i32.load (i32.add (local.get $argv) (i32.const 4))))))
"#;
        let mut root = Root::new();
        root.addbootfile("init", wat::parse_str(ARGS).unwrap());
        let mut k = boot(root).unwrap();
        k.exec(1, "/boot/init", &["init".into(), "second".into()]).unwrap();
        assert_eq!(k.procs.borrow().status(1).as_deref(), Some("second"));
    }

    /// **`procrfork` makes a second process and runs a function in it.**
    ///
    /// The child asks the kernel who it is — `#c/pid` — and writes the answer
    /// where the parent can read it, which on this machine is the same memory
    /// (`RFMEM`, declared). The parent then checks that the pid it was told
    /// and the pid the child saw are the same number, and that its own is
    /// different: that is the whole claim, that two processes existed.
    #[test]
    fn procrfork_runs_a_function_as_another_process() {
        const FORK: &str = r##"
(module
  (import "sys" "open"      (func $open      (param i32 i32) (result i32)))
  (import "sys" "pread"     (func $pread     (param i32 i32 i32 i64) (result i32)))
  (import "sys" "close"     (func $close     (param i32) (result i32)))
  (import "sys" "exits"     (func $exits     (param i32)))
  (import "sys" "procrfork" (func $procrfork (param i32 i32 i32 i32) (result i32)))
  (type $fn (func (param i32)))
  (memory (export "memory") 1)
  (table 1 1 funcref)
  (elem (i32.const 0) $child)
  (data (i32.const 8)   "#c/pid\00")
  (data (i32.const 64)  "the child saw a different pid\00")
  (data (i32.const 128) "the child was this process\00")
  (global $childpid (mut i32) (i32.const 0))

  ;; read `#c/pid` into $at, NUL-terminated, and answer its length
  (func $pid (param $at i32) (result i32)
    (local $fd i32) (local $n i32)
    (local.set $fd (call $open (i32.const 8) (i32.const 0)))
    (local.set $n
      (call $pread (local.get $fd) (local.get $at) (i32.const 32) (i64.const -1)))
    (i32.store8 (i32.add (local.get $at) (local.get $n)) (i32.const 0))
    (drop (call $close (local.get $fd)))
    (local.get $n))

  ;; the child: say who you are, at 256
  (func $child (param $arg i32)
    (drop (call $pid (i32.const 256)))
    (call $exits (i32.const 0)))

  (func (export "__childstart") (param $f i32) (param $arg i32)
    (call_indirect (type $fn) (local.get $arg) (local.get $f))
    (call $exits (i32.const 0)))

  (func (export "_start") (param i32 i32 i32)
    (local $me i32)
    ;; RFPROC|RFFDG
    (global.set $childpid
      (call $procrfork (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 4)))
    ;; The child wrote its pid at 256, as `readnum` writes one: right
    ;; justified in NUMSIZE-1 columns with a trailing space (devcons.c), so
    ;; the last digit of a small pid is at offset 10.
    (if (i32.ne
          (i32.load8_u (i32.const 266))
          (i32.add (i32.const 48) (global.get $childpid)))
      (then (call $exits (i32.const 64)) (return)))
    ;; and it must not be us
    (drop (call $pid (i32.const 384)))
    (if (i32.eq (i32.load8_u (i32.const 394)) (i32.load8_u (i32.const 266)))
      (then (call $exits (i32.const 128)) (return)))
    (call $exits (i32.const 0))))
"##;
        assert_eq!(run(FORK, &[]).unwrap(), "");
    }
}

/// **P3's acceptance, run against the real thing.**
///
/// Not a model of rc and not a mock kernel: these boot the kernel, bind the
/// namespace, and execute the rc that `userspace/mk.sh` built — Plan 9's rc,
/// compiled to wasm, over a libc whose system calls are this machine's
/// imports.
///
/// They need that build. `cargo test` cannot do it (it needs wasi-sdk, which
/// is a prerequisite and not a dependency), so a missing binary fails here
/// and says which command to run rather than passing quietly.
#[cfg(test)]
mod userspace {
    use super::*;

    fn rc(script: &str) -> String {
        let argv = ["rc".to_string(), "/bin/test.rc".to_string()];
        match run(
            &argv,
            &[("test.rc", script.as_bytes()), ("greeting", b"a file in the boot list\n")],
        ) {
            Ok((status, out)) => {
                assert_eq!(status, "", "rc failed: {out}");
                out
            }
            Err(e) => panic!("{e}"),
        }
    }

    /// **`rc` runs a script.** The first half of P3's acceptance.
    #[test]
    fn rc_runs_a_script() {
        assert_eq!(rc("echo hello from rc\n"), "hello from rc\n");
    }

    /// **A pipeline of two commands works.** The second half — and the one
    /// that needs a second process, which this machine cannot make by
    /// returning twice from `rfork`. rc starts its left-hand stage by
    /// re-executing itself (`haventfork.c`), and that re-execution is
    /// `procrfork`.
    #[test]
    fn a_pipeline_of_two_commands_works() {
        assert_eq!(
            rc("echo hello from a pipeline | tr a-z A-Z\n"),
            "HELLO FROM A PIPELINE\n"
        );
    }

    /// rc's own control flow, over commands that are separate processes.
    #[test]
    fn rc_runs_a_script_with_a_loop_and_a_variable() {
        assert_eq!(
            rc("x=world\nfor(i in a b c) echo $i $x\n"),
            "a world\nb world\nc world\n"
        );
    }

    /// A command reads a file the kernel resolved by name, and the shell
    /// redirects where its output goes.
    #[test]
    fn redirection_and_a_command_that_reads_a_file() {
        assert_eq!(rc("cat /bin/greeting\n"), "a file in the boot list\n");
    }

    /// The environment is `#e`, bound at `/env`, and rc keeps its variables
    /// there — so a variable set in one process is read by another.
    #[test]
    fn a_variable_reaches_a_child_through_the_environment_device() {
        assert_eq!(rc("x=through\necho `{echo $x}\n"), "through\n");
    }
}
