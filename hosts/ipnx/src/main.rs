//! `ipnx` — Saranos on a terminal. The binary; the system is [`ipnx`] itself.

use ipnx::{startboot, store, Host, BOOT};
use ipnx_kernel::devvirtio9p::Nineserver;

#[cfg(test)]
use ipnx::{boot, Term, CONFFILE};
#[cfg(test)]
use ipnx_kernel::devroot::Root;


fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("usage: ipnx [<command> [<arg> ...]]");
        eprintln!("  boots Saranos on this terminal and runs one command");
        return;
    }
    // With arguments, run that command instead of booting — which is how a
    // single command is tried without a shell. Without, boot.
    let argv: Vec<String> = if args.len() > 1 {
        let mut v = vec![format!("/bin/{}", args[1])];
        v.extend(args[2..].iter().cloned());
        v
    } else {
        vec![BOOT.to_string()]
    };

    // The machine's filesystem. `-fsdev local` names a host directory; this
    // one is `$HOME/lib/ipnx`, and what a program writes under `/root`
    // is a file there when the process is gone.
    // The machine's filesystem — qemu's `-fsdev local` names a host
    // directory, and `IPNX_STORE` names this one. The built rootfs is the
    // default, so a fresh checkout boots into something.
    let dir = std::env::var_os("IPNX_STORE").map(std::path::PathBuf::from).unwrap_or_else(|| {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../userspace/root")
    });
    let store: Option<Box<dyn Nineserver>> = match store::Store::new(&dir) {
        Ok(s) => Some(Box::new(s)),
        Err(e) => {
            eprintln!("ipnx: {}: {e}", dir.display());
            None
        }
    };

    match startboot(&argv, &[], Box::new(Host), store) {
        Ok(status) if status.is_empty() => {}
        Ok(status) => {
            eprintln!("ipnx: {}: {status}", argv[0]);
            std::process::exit(1);
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
        let mut k = boot(root, Box::new(Term::default()), None)?;
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
        let mut k = boot(root, Box::new(Term::default()), None).unwrap();
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

/// **The demo, run against the real thing.**
///
/// Not a model of anything: these boot the kernel, run `boot`, `init`,
/// `/lib/namespace` and `/rc/bin/termrc`, and then TYPE at the console —
/// which is the whole of what `ipnx` does.
///
/// They need `userspace/mk.sh` to have run. `cargo test` cannot do it (it
/// needs wasi-sdk, which is a prerequisite and not a dependency), so a
/// missing rootfs fails here and says which command to run.
#[cfg(test)]
mod userspace {
    use super::*;

    /// The rootfs `mk.sh` built — the machine's filesystem, as `ipnx` serves
    /// it by default.
    fn rootfs() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../userspace/root")
    }

    /// Boot, type, and answer with what the console was shown.
    pub(super) fn typing(keys: &str) -> String {
        typing_at(keys, &rootfs())
    }

    /// The same, on a filesystem of this test's own.
    pub(super) fn typing_at(keys: &str, store: &std::path::Path) -> String {
        let term = Term::typing(keys);
        let store = store::Store::new(store).expect("a store");
        match startboot(
            &[BOOT.to_string()],
            &[],
            Box::new(term.clone()),
            Some(Box::new(store)),
        ) {
            Ok(_) => term.screen(),
            Err(e) => panic!("{e}"),
        }
    }

    /// **P5's acceptance: typing `ipnx` boots to `rc` on the terminal.** The
    /// prompt is the proof that a shell is waiting for a command.
    #[test]
    fn it_boots_to_a_shell() {
        let out = typing("echo it booted\n");
        assert!(out.contains("% "), "no prompt: {out:?}");
        assert!(out.contains("it booted\n"), "{out:?}");
    }

    /// **`cat /etc/motd`** — a file that exists only on the file server, read
    /// through the union `boot` made of `/`.
    #[test]
    fn cat_etc_motd() {
        assert!(typing("cat /etc/motd\n").contains("Saranos."), "no motd");
    }

    /// **`ls`** — and what it lists is the union: the kernel's root AND the
    /// server's, merged by `unionread`.
    #[test]
    fn ls_shows_both_halves_of_the_root() {
        let out = typing("ls /\n");
        for name in ["boot", "dev", "proc", "srv"] {
            assert!(out.contains(&format!("{name}\n")), "no {name} from #/: {out:?}");
        }
        for name in ["etc", "lib", "rc", "wasm"] {
            assert!(out.contains(&format!("{name}\n")), "no {name} from the server");
        }
    }

    /// `/bin` is a union too, and it is the one `/lib/namespace` makes:
    /// `bind /$objtype/bin /bin` then `bind -a /rc/bin /bin`.
    #[test]
    fn bin_is_the_union_the_namespace_file_makes() {
        let out = typing("ls /bin\n");
        assert!(out.contains("echo\n"), "no commands: {out:?}");
        assert!(out.contains("termrc\n"), "no /rc/bin: {out:?}");
    }

    /// The devices are where `/lib/namespace` and `/rc/bin/termrc` put them,
    /// and `/dev` is `#c` with the terminal's own bound after it.
    #[test]
    fn the_devices_are_bound_where_the_files_say() {
        let out = typing("ls /dev\n");
        assert!(out.contains("cons\n"), "no #c: {out:?}");
        assert!(out.contains("random\n"), "no #c: {out:?}");
        assert!(out.contains("0ctl\n"), "no #d, which termrc binds: {out:?}");
    }

    /// The environment the boot set: `$objtype` from the machine
    /// (`pc/main.c:252`), `$user` from `#c/user`, `$sysname` from termrc.
    #[test]
    fn the_environment_is_what_the_boot_put_there() {
        assert!(typing("echo $objtype $user $sysname\n").contains("wasm eve gnot"));
    }

    /// **`/dev/config` is the KERNEL configuration file** — `$CONF`, the one
    /// `mkdevc` turns into `devtab[]` (`portmkfile:53`, `devcons.c:871`) —
    /// and `$terminal` is `arch->id` and that file's path, nothing else
    /// (`pc/main.c:250`). It used to be the command line for both, which is
    /// `plan9.ini`'s job and not this one.
    #[test]
    fn dev_config_is_the_kernel_configuration_and_not_the_command_line() {
        let out = typing("cat /dev/config\n");
        assert!(out.contains("\ndev\n"), "the `dev` section: {out}");
        for name in ["root", "cons", "env", "pipe", "proc", "mnt", "srv", "dup", "virtio9p"] {
            assert!(out.contains(&format!("\t{name}\n")), "no `{name}` in {out}");
        }
        assert!(typing("echo $terminal\n").contains(&format!("wasm {CONFFILE}")));
    }

    /// **`#p/1/ns` is the namespace as `/lib/namespace` would write it.**
    /// It was device letters and qid numbers on both sides, which is the
    /// kernel's bookkeeping and not a namespace.
    #[test]
    fn ns_reads_back_as_the_namespace_file() {
        let out = typing("cat /proc/1/ns\n");
        for line in [
            "bind -a /root /\n",
            "mount -aC #s/boot /root \n",
            "bind  /wasm/bin /bin\n",
            "bind -a /rc/bin /bin\n",
            "bind  #c /dev\n",
            "bind -c #e /env\n",
            "cd /\n",
        ] {
            assert!(out.contains(line), "no `{}` in {out}", line.trim_end());
        }
        assert!(!out.contains("#M"), "a mount names its server, not `#M`: {out}");
        assert!(!out.contains("#//"), "paths, not device letters and qids: {out}");
    }

    /// `#ec` is bound under `#e` (`initcode.c:28`) and is EMPTY, because what
    /// fills it on a Plan 9 machine is every line of `plan9.ini`
    /// (`pc/main.c:257`) and this host has no counterpart to one. The three
    /// the kernel sets itself go to `#e`, with `conf` 0 (`:251`).
    #[test]
    fn the_configuration_environment_is_reachable_and_empty() {
        assert!(typing("ls '#ec' && echo none\n").contains("none"));
        assert!(typing("bind -a '#ec' /env && echo bound\n").contains("bound"));
        assert!(typing("ls '#ex'\n").contains("bad arg"), "any other spec is Ebadarg");
        assert!(typing("echo $cputype\n").contains("wasm"), "#e still answers");
    }

    /// A pipeline, typed — two processes and the pipe between them.
    #[test]
    fn a_pipeline_typed_at_the_console() {
        assert!(typing("echo shouting | tr a-z A-Z\n").contains("SHOUTING\n"));
    }

    /// rc's own control flow, over commands that are separate processes.
    #[test]
    fn a_loop_and_a_variable() {
        let out = typing("x=world\nfor(i in a b c) echo $i $x\n");
        assert!(out.contains("a world\nb world\nc world\n"), "{out:?}");
    }

    /// Command substitution — a second rc, its output read back through a
    /// pipe, split on `$ifs`.
    #[test]
    fn command_substitution() {
        assert!(typing("echo `{echo through}\n").contains("through\n"));
    }

    /// **The exit status of a command reaches the shell.**
    #[test]
    fn a_commands_exit_status_reaches_the_shell() {
        let out = typing("cat /nothing\necho after [$status]\necho ok\necho then [$status]\n");
        assert!(out.contains("after [can't open /nothing"), "{out:?}");
        assert!(out.contains("then []"), "a command that worked clears it: {out:?}");
    }

    /// A number in `#c` is a FILE, and a file ends — so `cat` of one returns.
    #[test]
    fn cat_of_a_device_file_ends() {
        let out = typing("cat /dev/pid\necho done\n");
        assert!(out.contains("done\n"), "cat never returned: {out:?}");
    }
}

/// **P4's storage acceptance, and P5's boot on top of it**: what is written
/// survives, because the file server is the machine's.
#[cfg(test)]
mod storage {
    use super::*;
    use userspace::typing_at;

    /// A rootfs of this test's own, copied so two boots can share it and
    /// nothing else can.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let d = std::env::temp_dir().join(format!("ipnx-test-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            let from =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../userspace/root");
            copy(&from, &d).expect("a rootfs to boot from");
            Scratch(d)
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    fn copy(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            let dst = to.join(e.file_name());
            if e.file_type()?.is_dir() {
                copy(&e.path(), &dst)?;
            } else {
                std::fs::copy(e.path(), dst)?;
            }
        }
        Ok(())
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// **Two boots: one writes, the other reads.** Nothing carries over but
    /// the filesystem itself.
    #[test]
    fn a_file_written_through_the_store_survives_a_boot() {
        let s = Scratch::new("survives");
        typing_at("echo kept > /tmp/note\n", s.path());
        let second = typing_at("cat /tmp/note\n", s.path());
        assert!(second.contains("kept\n"), "the second boot did not find it: {second:?}");
    }

    /// And it is a file on the machine, not a story the kernel tells.
    #[test]
    fn what_was_written_is_a_file_the_machine_holds() {
        let s = Scratch::new("realfile");
        typing_at("echo on the disk > /tmp/thing\n", s.path());
        let real = std::fs::read_to_string(s.path().join("tmp/thing")).expect("a real file");
        assert_eq!(real, "on the disk\n");
    }

    /// A directory made through the mount is a directory on the machine.
    #[test]
    fn a_directory_made_through_the_mount_is_one() {
        let s = Scratch::new("dirs");
        typing_at("mkdir /tmp/sub\necho deep > /tmp/sub/file\n", s.path());
        assert!(s.path().join("tmp/sub").is_dir(), "no directory on the machine");
        assert!(typing_at("cat /tmp/sub/file\n", s.path()).contains("deep\n"));
    }
}

