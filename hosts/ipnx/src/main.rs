//! `ipnx` — Saranos on a terminal. The binary; the system is [`ipnx`] itself.

use ipnx::{plan9ini, startboot, store, Host, BOOT};
use ipnx_kernel::devvirtio9p::Nineserver;

#[cfg(test)]
use ipnx::{boot, Term, CONFFILE};
#[cfg(test)]
use ipnx_kernel::devroot::Root;


fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("usage: ipnx [<command> [<arg> ...]]");
        eprintln!("  boots Saranos on this terminal; with a command, init runs it");
        eprintln!("  with rc -c before the interactive shell, as Plan 9's init does");
        return;
    }
    // With arguments, the system boots as always and init is given the
    // command, as `init=` in `plan9.ini` gives it one (`initcmd`). What runs
    // it is the system's own rc, in the namespace init built.
    let conf = plan9ini(&args[1..]);
    let argv = vec![BOOT.to_string()];

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

    Host::catch_interrupt();
    let r = startboot(&argv, &[], &conf, Box::new(Host), store);
    Host::restore_terminal();
    match r {
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
        // `exec` gives pid 1 an image; `ready` puts it on the queue; and
        // `schedinit` is what runs anything at all (`proc.c:67`). It was
        // `exec` that ran the process, which is not what `sysexec` does.
        k.exec(1, "/boot/init", &["init".to_string()])?;
        k.procs.borrow_mut().ready(1);
        k.schedinit()?;
        let status = k.procs.borrow().status(1);
        Ok(status.unwrap_or_default())
    }

    /// **A call given a bad address ends the process**, as Plan 9's
    /// `validaddr` does (`fault.c:310`): *"sys: bad address in syscall"*,
    /// `NDebug`, taken on the way out of the call. The module passes `open`
    /// an address far past the end of its memory — which the host once
    /// refused itself, answering -1 with no note; if the call only failed,
    /// the module would go on to exit with "survived".
    #[test]
    fn a_bad_address_to_open_ends_the_process_with_the_note() {
        const BAD: &str = r#"
(module
  (import "sys" "open"  (func $open  (param i32 i32) (result i32)))
  (import "sys" "exits" (func $exits (param i32)))
  (memory (export "memory") 1)
  (data (i32.const 8) "survived\00")
  (func (export "_start") (param i32 i32 i32 i32)
    (drop (call $open (i32.const 0x7fff0000) (i32.const 0)))
    (call $exits (i32.const 8))))
"#;
        assert_eq!(run(BAD, &[]).unwrap(), "init 1: sys: bad address in syscall");
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
  (func (export "_start") (param i32 i32 i32 i32)
    (global.set $n (call $open (i32.const 8) (i32.const 0)))
    (global.set $n
      (call $pread (global.get $n) (i32.const 256) (i32.const 256) (i64.const -1)))
    ;; NUL-terminate what was read, since `exits` takes a string
    (i32.store8 (i32.add (i32.const 256) (global.get $n)) (i32.const 0))
    (call $exits (i32.const 256))))
"#;
        // `pexit`'s wait message is *"%s %lud: %s"* — text, pid, status
        // (`proc.c:1195`), and the text is the file `exec` ran.
        assert_eq!(run(READ, &[("hello", b"the bytes")]).unwrap(), "init 1: the bytes");
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
  (func (export "_start") (param i32 i32 i32 i32)
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
  (func (export "_start") (param i32 i32 i32 i32) (call $exits (i32.const 8))))
"#;
        assert_eq!(run(BYE, &[]).unwrap(), "init 1: oops");
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
  (func (export "_start") (param $argc i32) (param $argv i32) (param $heap i32) (param $tos i32)
    (call $exits (i32.load (i32.add (local.get $argv) (i32.const 4))))))
"#;
        let mut root = Root::new();
        root.addbootfile("init", wat::parse_str(ARGS).unwrap());
        let mut k = boot(root, Box::new(Term::default()), None).unwrap();
        k.exec(1, "/boot/init", &["init".into(), "second".into()]).unwrap();
        k.procs.borrow_mut().ready(1);
        k.schedinit().unwrap();
        assert_eq!(k.procs.borrow().status(1).as_deref(), Some("init 1: second"));
    }
}

/// **The demo, run against the real thing.**
///
/// Not a model of anything: these boot the kernel, run `boot`, `init`,
/// `/profile/start.ns` and `/profile/start.rc`, and then TYPE at the console —
/// which is the whole of what `ipnx` does.
///
/// They need `userspace/mk.sh` to have run. `cargo test` cannot do it (it
/// needs wasi-sdk, which is a prerequisite and not a dependency), so a
/// missing rootfs fails here and says which command to run.
#[cfg(test)]
mod userspace {
    use super::*;
    use ipnx::initcmd;

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
            &plan9ini(&[]),
            Box::new(term.clone()),
            Some(Box::new(store)),
        ) {
            Ok(_) => term.screen(),
            Err(e) => panic!("{e}"),
        }
    }

    /// `ipnx <command> <arg>…`: the system boots as always, with the
    /// command as `init=` in its configuration, as `plan9.ini` would give it.
    pub(super) fn commanding(args: &[&str], keys: &str) -> String {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let term = Term::typing(keys);
        let store = store::Store::new(&rootfs()).expect("a store");
        match startboot(&[BOOT.to_string()], &[], &plan9ini(&args), Box::new(term.clone()), Some(Box::new(store))) {
            Ok(_) => term.screen(),
            Err(e) => panic!("{e}"),
        }
    }

    /// **One command from the host's command line** (`cargo run -p ipnx --
    /// echo hello`). `boot` reads `$init` and hands it to `init`
    /// (`boot.c:208`), which runs it with `rc -c` (`init.c:171`) in the
    /// namespace it built — `$objtype` is init's own, and `/bin` is
    /// `/profile/start.ns`'s — and a word with a quote and a space in it
    /// arrives whole. Then the interactive shell, as Plan 9's `init` goes
    /// on to; with no input it ends at once.
    #[test]
    fn a_command_on_the_host_command_line_runs_through_boot_and_init() {
        let out = commanding(&["echo", "hello", "it's here"], "");
        assert!(out.contains("hello it's here\n"), "{out:?}");
        let out = commanding(&["cat", "/env/objtype"], "");
        assert!(out.contains("init: starting /bin/rc\nwasm"), "{out:?}");
    }

    /// The `init=` line: one token for `tokenize`, holding the command's
    /// words each quoted for rc.
    #[test]
    fn initcmd_quotes_the_command_for_tokenize_and_rc() {
        let (name, val) = initcmd(&["echo".into(), "it's".into(), "a b".into()]);
        assert_eq!(name, "init");
        assert_eq!(val, "/wasm/init -t 'echo ''it''''s'' ''a b'''");
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
        for name in ["etc", "home", "lib", "profile", "usr", "wasm"] {
            assert!(out.contains(&format!("{name}\n")), "no {name} from the server");
        }
        assert!(!out.contains("\nrc\n"), "/rc is retired (docs/packages.md): {out:?}");
    }

    /// `/bin` is what `/profile/start.ns` makes it: `bind /$objtype/bin
    /// /bin`. Plan 9's next line, `bind -a /rc/bin /bin`, went with `/rc`.
    #[test]
    fn bin_is_what_the_namespace_file_makes() {
        let out = typing("ls /bin\n");
        assert!(out.contains("echo\n"), "no commands: {out:?}");
        assert!(out.contains("test\n"), "no test, which rcmain calls: {out:?}");
        assert!(!out.contains("termrc\n"), "/rc/bin is retired: {out:?}");
    }

    /// The devices are where `/profile/start.ns` and `/profile/start.rc` put them,
    /// and `/dev` is `#c` with the terminal's own bound after it.
    #[test]
    fn the_devices_are_bound_where_the_files_say() {
        let out = typing("ls /dev\n");
        assert!(out.contains("cons\n"), "no #c: {out:?}");
        assert!(out.contains("random\n"), "no #c: {out:?}");
        assert!(out.contains("0ctl\n"), "no #d, which start.rc binds: {out:?}");
    }

    /// The environment the boot set: `$objtype` from the machine
    /// (`pc/main.c:252`), `$user` from `#c/user`, `$sysname` from start.rc.
    #[test]
    fn the_environment_is_what_the_boot_put_there() {
        // `kitty`, not `eve`: `eve` is the empty string until `boot` writes
        // `#c/hostowner` (`bootauth.c:56`), and the name it writes is
        // `$user` from the configuration — this system's `plan9.ini` says
        // `user=kitty` — or Plan 9's own fallback, `glenda`. `eve` is the
        // role, not a person.
        assert!(typing("echo $objtype $user $sysname\n").contains("wasm kitty gnot"));
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

    /// **`#p/1/ns` is the namespace as `/profile/start.ns` would write it.**
    /// It was device letters and qid numbers on both sides, which is the
    /// kernel's bookkeeping and not a namespace.
    #[test]
    fn ns_reads_back_as_the_namespace_file() {
        let out = typing("cat /proc/1/ns\n");
        for line in [
            "bind -a /root /\n",
            "mount -aC #s/boot /root \n",
            "bind  /pkg/system/2026.09.24/wasm/bin /bin\n",
            "bind -a /pkg/system/2026.09.24/lib /lib\n",
            "bind -c /usr/kitty /home\n",
            "bind  #c /dev\n",
            "bind -c #e /env\n",
            "cd /\n",
        ] {
            assert!(out.contains(line), "no `{}` in {out}", line.trim_end());
        }
        assert!(!out.contains("#M"), "a mount names its server, not `#M`: {out}");
        assert!(!out.contains("#//"), "paths, not device letters and qids: {out}");
    }

    /// **Two processes really do alternate.** A pipeline is the everyday
    /// proof: `echo` and `tr` are separate processes, the shell sleeps in
    /// `await` while they run, and the second sees what the first wrote.
    /// Before the switch, a child ran to its end inside the call that made
    /// it and there was never more than one runnable process.
    #[test]
    fn a_pipeline_is_two_processes_and_the_shell_waits_for_them() {
        let out = typing("echo shouting | tr a-z A-Z\ncat /proc/1/status\n");
        assert!(out.contains("SHOUTING"), "{out}");
        // Pid 1 is `init`, asleep in `await` for as long as a shell runs —
        // `pwait`'s `sleep(&up->waitr, haswaitq, up)` — and `status` shows
        // the call it is in, `psstate`, as Plan 9's does (`devproc.c:865`).
        assert!(out.contains("Await"), "init waits in pwait: {out}");
    }

    /// `sleep` leaves the processor: the process is `Wakeme` with a
    /// deadline and `timerintr` is what ends it. With nothing else to run
    /// `schedinit` reaches `idlehands()`, and the shell comes back.
    #[test]
    fn a_sleeping_process_comes_back() {
        assert!(typing("sleep 1\necho awake\n").contains("awake"));
    }

    /// **A forked child may sleep before it `exec`s.** The file rc runs
    /// is the read end of a pipe, so the child sleeps in its `exec` until
    /// `cat` has written the image. A child that ran on its parent's frames
    /// could not sleep there without leaving neither of them enterable
    /// (RESEARCH §15.9).
    #[test]
    fn a_child_may_sleep_before_it_execs() {
        let out = typing(
            "mkdir -p /tmp/p; bind '#|' /tmp/p\n\
             cat /bin/echo >/tmp/p/data &\n\
             /tmp/p/data1 it ran\n\
             echo after\n",
        );
        assert!(out.contains("it ran\n"), "{out}");
        assert!(out.contains("after\n"), "{out}");
    }

    /// **`sed`, and `setjmp` under it.** Plan 9's `libregexp` recovers from
    /// a malformed expression with `longjmp` (`regcomp.c`), which this
    /// machine does by unwinding the stack (`libc/wasm/setjmp.c`, RESEARCH
    /// §16.12): the bad expression unwinds to sed's own complaint, and the
    /// shell goes on.
    #[test]
    fn sed_edits_and_a_bad_expression_unwinds() {
        let out = typing("echo hello world | sed s/world/kitty/\necho x | sed 's/[/y/'\nseq 5 | sed -n '2,3p'\necho still here\n");
        assert!(out.contains("hello kitty\n"), "{out}");
        assert!(out.contains("sed: r.e.-using command garbled"), "{out}");
        assert!(out.contains("2\n3\n"), "{out}");
        assert!(out.contains("still here"), "{out}");
    }

    /// **`longjmp`, more than once to one `setjmp`.** `ed` answers every
    /// error by `longjmp(savej, 1)` back to its command loop (`ed.c`,
    /// `error`), so two bad commands unwind to the same place twice and the
    /// third still runs. Its temporary file is named by its own `mktemp`
    /// writing into a string literal (`ed.c:159`), which is kencc's meaning
    /// and must be this compiler's too (`kencc.py`, step 4).
    #[test]
    fn ed_recovers_from_errors_by_longjmp() {
        let f = format!("/tmp/ed{}", std::process::id());
        let out = typing(&format!("echo a >{f}; {{echo zz; echo zz; echo 1p; echo q}} | ed {f}; rm {f}\n"));
        assert!(out.contains("2\n?\n?\na\n"), "{out:?}");
    }

    /// **A fork is a copy.** rc's `@{…}` forks (`havefork.c`): what the
    /// subshell sets is in its copy of memory and not its parent's.
    #[test]
    fn a_subshell_is_a_copy() {
        let out = typing("x=parent; @{x=child; echo in $x}; echo out $x\n");
        assert!(out.contains("in child\nout parent\n"), "{out:?}");
    }

    /// **`fork` returns twice** (`fork(2)`): `time` forks, the child `exec`s
    /// the command and the parent waits for it and reports its times
    /// (`time.c`). The stack unwinds, the child is wound back in a copy of
    /// memory and the parent where it was (RESEARCH §16.12).
    #[test]
    fn fork_returns_twice() {
        let out = typing("time echo forked\n");
        assert!(out.contains("forked\n"), "{out:?}");
        assert!(out.contains("r \t echo forked"), "{out:?}");
    }

    /// **A file server that is a process, mounted and used by others** —
    /// `plumber` (`cmd/plumb`): libthread procs sharing one memory serve 9P
    /// down a pipe, `mount` sleeps in the mount driver for the replies
    /// (`devmnt.c:811`), one process at a time reads the wire and each
    /// reply goes to the RPC with its tag (`mountmux`). A rule is written, a
    /// port is read, and `plumb` delivers a message to it.
    #[test]
    fn plumber_serves_and_plumb_delivers() {
        let out = typing(
            "plumber -p /dev/null\n\
             {echo 'type is text'; echo 'data matches hello'; echo 'plumb to edit'} >/mnt/plumb/rules\n\
             ls /mnt/plumb\n\
             cat /mnt/plumb/edit >/tmp/plumbed &\n\
             sleep 1\n\
             plumb -d edit -s me hello\n\
             sleep 1\n\
             cat /tmp/plumbed\n",
        );
        assert!(out.contains("/mnt/plumb/edit\n/mnt/plumb/rules\n/mnt/plumb/send\n"), "{out:?}");
        assert!(out.contains("me\nedit\n/\ntext\n\n5\nhello"), "{out:?}");
    }

    /// **An rc script runs by name** (`sysproc.c:340`): its `#!` line names
    /// the interpreter, which is given the script's name and the caller's
    /// arguments — so rc's `$0` is the script and `$*` what it was called
    /// with.
    #[test]
    fn an_rc_script_runs_by_name() {
        use std::os::unix::fs::PermissionsExt;
        let name = format!("script{}.rc", std::process::id());
        let path = rootfs().join("tmp").join(&name);
        std::fs::write(&path, "#!/bin/rc\necho script $0 $*\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let out = typing(&format!("/tmp/{name} a 'b c'\n"));
        let _ = std::fs::remove_file(&path);
        assert!(out.contains(&format!("script /tmp/{name} a b c\n")), "{out:?}");
    }

    /// An image the machine cannot run fails `exec` — `Ebadexec`, as
    /// `sysexec` refuses a bad header — and the shell goes on. It used to
    /// end the system from inside the scheduler.
    #[test]
    fn exec_of_something_that_is_not_a_module_fails_and_the_system_goes_on() {
        let out = typing("echo junk >/tmp/j\n/tmp/j || echo refused\necho still here\n");
        assert!(out.contains("refused"), "{out}");
        assert!(out.contains("still here"), "{out}");
    }

    /// **A process in a tight loop does not stop the system** — P6's
    /// acceptance. The loop makes no call, so nothing but the clock can take
    /// the processor from it: `hzsched` marks it once its quantum is up and
    /// the interrupt's tail `sched()`s (`proc.c:209`, `pc/trap.c:438`). The
    /// shell gets the processor back, kills the loop, and says so.
    #[test]
    fn a_process_in_a_tight_loop_does_not_stop_the_system() {
        let out = typing(
            "{while(~ 1 1) x=1} &\n\
             echo kill >/proc/$apid/ctl\n\
             echo still here\n",
        );
        assert!(out.contains("still here"), "{out}");
    }

    /// **rc catches a note.** `Trapinit` is `plan9.c`'s now, `notify(notifyf)`;
    /// a note to the shell interrupts what it is waiting in, the kernel hands
    /// it to `notifyf` on the stack as `notify(Ureg*)` does, `noted(NCONT)`
    /// goes back, and rc runs its `sigint` function (`rc/plan9.c:513`).
    #[test]
    fn rc_catches_a_note_with_its_own_handler() {
        let out = typing("fn sigint { echo caught }\necho interrupt >/proc/$pid/note\necho after\n");
        assert!(out.contains("caught"), "{out}");
        assert!(out.contains("after"), "the shell went on: {out}");
    }

    /// **`kill` is a note** (`devproc.c:1363`): it wakes a sleeping process
    /// at once — `postnote` takes it off its `Rendez` — and the process ends
    /// itself in `procctl` on its way out of the kernel. The sleep was for
    /// thirty seconds; the test takes nothing like that.
    #[test]
    fn kill_ends_a_sleeping_process_at_once() {
        let t = std::time::Instant::now();
        // `exec`, so `$apid` is the sleeping process and not an rc around it.
        let out = typing("{exec sleep 30} &\necho kill >/proc/$apid/ctl\nwait\necho done\n");
        assert!(out.contains("done"), "{out}");
        assert!(t.elapsed() < std::time::Duration::from_secs(25), "it waited out the sleep");
    }

    /// **A tracer's view of a call** (`devproc.c:1411`, `pc/trap.c:682`):
    /// stop `sleep` after its first second, start it with `startsyscall`,
    /// and it stops again on its way into the next `sleep(1000)` — which
    /// `/proc/n/syscall` shows as `syscallfmt` does, with the pc the call was
    /// made from.
    #[test]
    fn startsyscall_shows_the_next_call() {
        let out = typing(
            "{exec sleep 2} &\n\
             echo stop >/proc/$apid/ctl\n\
             echo startsyscall >/proc/$apid/ctl\n\
             cat /proc/$apid/syscall; echo\n\
             echo start >/proc/$apid/ctl\n\
             wait\necho done\n",
        );
        let line = out.lines().find(|l| l.contains(" sleep Sleep ")).unwrap_or_else(|| panic!("{out}"));
        let w: Vec<&str> = line.split_whitespace().collect();
        let at = w.iter().position(|&x| x == "Sleep").unwrap();
        assert_eq!(w[at - 1], "sleep", "{line}");
        assert!(u64::from_str_radix(w[at + 1], 16).is_ok_and(|pc| pc > 0), "a pc: {line}");
        assert_eq!(w[at + 2], "1000", "{line}");
        assert!(out.contains("done"), "{out}");
    }

    /// **`^C` interrupts a command.** The host receives the key and the
    /// console posts *"interrupt"* to its note group — the shell's, which
    /// `init` made with `RFNOTEG` — so `sleep`, which has no handler, ends,
    /// and rc, which has, carries on (P6's acceptance test).
    #[test]
    fn control_c_interrupts_a_command_and_the_shell_carries_on() {
        let t = std::time::Instant::now();
        let out = typing("sleep 30\n\x03echo after\n");
        assert!(out.contains("after"), "{out}");
        assert!(t.elapsed() < std::time::Duration::from_secs(25), "the sleep ran its course");
    }

    /// And a command waiting for the keyboard: `cat` asleep in `qread` is
    /// woken by the note and ends.
    #[test]
    fn control_c_interrupts_a_command_reading_the_console() {
        let out = typing("cat\n\x03echo after\n");
        assert!(out.contains("after"), "{out}");
    }

    /// `/dev/sysstat` reads the machine: the clock has interrupted and
    /// the kernel has answered calls, so neither count is zero.
    #[test]
    fn sysstat_counts_interrupts_and_calls() {
        let out = typing("sleep 1\ncat /dev/sysstat\n");
        // The line follows the prompts it was typed after; the ten numbers
        // are its last ten words.
        let line = out.lines().find(|l| l.split_whitespace().count() >= 10).expect(&out);
        let words: Vec<&str> = line.split_whitespace().collect();
        let v: Vec<u64> = words[words.len() - 10..].iter().map(|n| n.parse().unwrap()).collect();
        assert!(v[2] > 0, "interrupts: {line}");
        assert!(v[3] > 0, "syscalls: {line}");
    }

    /// `date -n` is `nsec()/1e9`, and `nsec` assembles `/dev/bintime`'s low
    /// word from `uchar`s. kencc promotes those to unsigned int
    /// (`cc/sub.c:688`), clang to int, so under clang the word sign-extended
    /// whenever its bit 31 was set and `date` printed 0 or -1. That bit
    /// flips every 2.1 seconds; the readings here span more than one flip.
    #[test]
    fn date_reads_the_clock_every_time() {
        let out = typing("date -n; sleep 1; date -n; sleep 1; date -n; sleep 1; date -n\n");
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        let v: Vec<i64> = out.split(|c: char| !c.is_ascii_digit() && c != '-').filter_map(|w| w.parse().ok()).collect();
        assert_eq!(v.len(), 4, "{out:?}");
        for t in v {
            assert!((now - t).abs() < 120, "{t} against {now}: {out:?}");
        }
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
        let out = typing("echo shouting | tr a-z A-Z\n");
        assert!(out.contains("SHOUTING\n"), "{out:?}");
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
        // rc's `$status` is the wait message, which Plan 9 begins with the
        // command's name and pid (`proc.c:1195`).
        assert!(out.contains(": can't open /nothing"), "{out:?}");
        assert!(out.contains("after [cat "), "{out:?}");
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
    pub(super) struct Scratch(std::path::PathBuf);

    impl Scratch {
        pub(super) fn new(name: &str) -> Scratch {
            let d = std::env::temp_dir().join(format!("ipnx-test-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            let from =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../userspace/root");
            copy(&from, &d).expect("a rootfs to boot from");
            Scratch(d)
        }
        pub(super) fn path(&self) -> &std::path::Path {
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

    /// **A create sends its own copy of the directory's channel** —
    /// `cunique` (`chan.c:1606`), *"because we're about to send a create,
    /// which will move it"*. Sent on dot's own, a create in `.` moved the
    /// current directory's fid onto the new file, and every relative name
    /// after it answered "unknown fid".
    #[test]
    fn a_create_in_dot_leaves_dot_where_it_was() {
        let s = Scratch::new("createdot");
        let out = typing_at("cd /tmp\necho a >x\necho b >>x\ncat x\nmkdir d; cd d; echo e >f; cd ..; cat d/f\n", s.path());
        assert!(out.contains("a\nb\n"), "{out:?}");
        assert!(out.contains("e\n"), "{out:?}");
        assert!(!out.contains("unknown fid"), "{out:?}");
    }

    /// **A stat carries the machine file's own times and mode, and a wstat
    /// changes them** — `stat2dir` and `rwstat` in `u9fs`
    /// (`u9fs.c:694`, `:909`). The store reported every time as 0 and had no
    /// `Twstat`, so `touch` could not date a file, `chmod` could not change
    /// one and Plan 9's `mv`, which renames with `dirwstat`, could not move
    /// one.
    #[test]
    fn stat_is_the_machines_and_wstat_changes_it() {
        use std::os::unix::fs::PermissionsExt;
        let s = Scratch::new("wstat");
        let out = typing_at(
            "echo x >/tmp/f
touch -t 86400 /tmp/f
chmod 600 /tmp/f
ls -l /tmp/f
mv /tmp/f /tmp/g
cat /tmp/g
",
            s.path(),
        );
        assert!(out.contains("--rw------- ") && out.contains(" 2 Jan  2  1970 /tmp/f\n"), "{out:?}");
        assert!(out.contains("x\n"), "{out:?}");
        let md = std::fs::metadata(s.path().join("tmp/g")).expect("renamed on the machine");
        assert!(!s.path().join("tmp/f").exists());
        assert_eq!(md.permissions().mode() & 0o777, 0o600);
        let when = md.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap();
        assert_eq!(when.as_secs(), 86400);
        let fresh = typing_at("echo y >/tmp/h\nls -l /tmp/h\n", s.path());
        assert!(!fresh.contains("1970"), "a new file is dated now: {fresh:?}");
    }
}

/// **P7's first step: the profiles** (docs/packages.md). `/profile` is the
/// system's configuration and `/home/profile` the user's; each has a
/// `start.rc`, a `shell.rc` and a `stop.rc`, run by role.
#[cfg(test)]
mod profiles {
    use super::storage::Scratch;
    use super::userspace::{typing, typing_at};

    /// `/profile` holds the namespace file and the three scripts, `/home` is
    /// `/usr/$user` with the user's in it, and `/rc` is gone.
    #[test]
    fn the_profiles_are_where_the_design_puts_them() {
        // Each directory on its own: this `ls` prints bare names, where
        // Plan 9's would prefix them (`ls.c:115`).
        let out = typing("ls /profile\necho --\nls /home/profile\nls /rc\n");
        let (system, user) = out.split_once("--\n").expect(&out);
        for name in ["start.ns", "start.env", "start.rc", "shell.env", "shell.rc", "stop.env", "stop.rc"] {
            assert!(system.contains(&format!("{name}\n")), "no /profile/{name}: {out:?}");
        }
        for name in ["start.ns", "start.env", "start.rc", "shell.env", "shell.rc", "stop.env", "stop.rc"] {
            assert!(user.contains(&format!("{name}\n")), "no /home/profile/{name}: {out:?}");
        }
        assert!(out.contains("/rc: file does not exist"), "/rc is retired: {out:?}");
        assert!(!out.contains("unknown fid"), "{out:?}");
    }

    /// **The order**: the system starts before the user logs in (Plan 9's
    /// `termrc` then `$home/lib/profile`, `init.c:178`), and at the end the
    /// user logs out before the system stops.
    #[test]
    fn start_and_stop_run_system_then_user_then_user_then_system() {
        let root = Scratch::new("profile-order");
        let add = |path: &str, line: &str| {
            let p = root.path().join(path);
            let mut s = std::fs::read_to_string(&p).unwrap();
            s.push_str(line);
            std::fs::write(&p, s).unwrap();
        };
        add("profile/start.rc", "echo system start\n");
        add("usr/kitty/profile/start.rc", "test -r profile/start.rc && echo user start at home\n");
        add("usr/kitty/profile/stop.rc", "echo user stop\n");
        add("profile/stop.rc", "echo system stop\n");
        let out = typing_at("echo typed\n", root.path());
        let at = |s: &str| out.find(s).unwrap_or_else(|| panic!("no `{s}` in {out:?}"));
        assert!(at("system start") < at("user start at home"), "after `cd`: {out:?}");
        assert!(at("user start") < at("typed"), "{out:?}");
        assert!(at("typed") < at("user stop"), "{out:?}");
        assert!(at("user stop") < at("system stop"), "{out:?}");
    }

    /// **At each scope, `start.ns`, then `start.env`, then `start.rc`** —
    /// `init`'s own order: the namespace, the environment, rc
    /// (docs/packages.md). The user's `start.ns` is added to the system's at
    /// login (`addns`), so the user's `start.rc` sees it; each `start.env`
    /// is read just before its `start.rc`, the system's before the user's.
    #[test]
    fn start_ns_then_start_env_then_start_rc() {
        let root = Scratch::new("profile-trio");
        let add = |path: &str, line: &str| {
            let p = root.path().join(path);
            let mut s = std::fs::read_to_string(&p).unwrap();
            s.push_str(line);
            std::fs::write(&p, s).unwrap();
        };
        add("profile/start.env", "sysenv=system\n");
        add("profile/start.rc", "echo system rc sees $sysenv\n");
        add("usr/kitty/profile/start.ns", "bind -a /etc /home\n");
        add("usr/kitty/profile/start.env", "userenv=($sysenv 'and user')\n");
        add("usr/kitty/profile/start.rc", "test -r /home/motd && echo user rc sees $userenv and the motd\n");
        let out = typing_at("echo typed $userenv\n", root.path());
        assert!(out.contains("system rc sees system\n"), "{out:?}");
        assert!(out.contains("user rc sees system and user and the motd\n"), "{out:?}");
        assert!(out.contains("typed system and user\n"), "the environment outlives the startup: {out:?}");
    }

    /// **`shell.rc` runs in every new shell**, the system's and then the
    /// user's — not only the login one, which is all Plan 9's
    /// `$home/lib/profile` gets. A child shell sets its own `$shellpid`; a
    /// fork — a pipeline's stage — does not run it again (`havefork.c`).
    #[test]
    fn shell_rc_runs_in_every_shell_system_then_user() {
        let root = Scratch::new("profile-shell");
        let add = |path: &str, line: &str| {
            let p = root.path().join(path);
            let mut s = std::fs::read_to_string(&p).unwrap();
            s.push_str(line);
            std::fs::write(&p, s).unwrap();
        };
        add("profile/shell.env", "envorder=system\n");
        add("usr/kitty/profile/shell.env", "envorder=$envorder^user\n");
        add("profile/shell.rc", "order=system\n");
        add("usr/kitty/profile/shell.rc", "order=$order^user; shellpid=$pid; runs=($runs x)\n");
        let out = typing_at(
            "echo order $order $envorder\nrc -c 'echo child $shellpid $pid'\necho shell $#runs\n{echo fork $#runs} | cat\n",
            root.path(),
        );
        assert!(out.contains("order systemuser systemuser\n"), "each .env before its .rc: {out:?}");
        let line = out.lines().find_map(|l| l.split("child ").nth(1)).expect(&out);
        let pids: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(pids.len(), 2, "{out:?}");
        assert_eq!(pids[0], pids[1], "the child ran shell.rc itself: {out:?}");
        // a fork is a copy of the shell, and has run shell.rc as often as
        // the shell has; a new shell would have run it once more
        let count = |w: &str| out.lines().find_map(|l| l.split(w).nth(1).map(str::to_string)).expect(&out);
        assert_eq!(count("shell "), count("fork "), "a fork is not a new shell: {out:?}");
    }
}
