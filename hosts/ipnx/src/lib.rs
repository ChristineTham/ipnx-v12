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
//!
//! It is a **library** as well as a binary: `startboot` boots the whole
//! system on a [`Console`] the caller supplies, which is how the conformance
//! suite drives it the way a person would.


pub mod machine;
pub mod store;
pub mod uart;

use ipnx_kernel::{
    chan,
    devcap::CapDev,
    devcons::{Cons, Console},
    devdup::DupDev,
    devenv::EnvDev,
    devmnt::MntDev,
    devpipe::PipeDev,
    devproc::ProcDev,
    devroot::Root,
    devsrv::SrvDev,
    devuart::UartDev,
    devvirtio9p::{Nineserver, Virtio9p},
    dev::DevId,
    Call, Kernel, Ret,
};
use std::rc::Rc;

/// What `#c` reports about the machine underneath. The kernel names these
/// files; only the host can fill them, which is the arrangement Plan 9 has for
/// every number `devcons` reports — it reads them from the architecture.
pub struct Host;

/// The interrupt key has been pressed and not yet handed over.
static INTERRUPT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What the terminal has typed, a line at a time, read on a thread of its
/// own so that waiting for a key holds up nothing else — Plan 9's keyboard
/// interrupts, as a machine without them has them. `None` is end of input.
static KEYS: std::sync::OnceLock<std::sync::Mutex<std::sync::mpsc::Receiver<Option<Vec<u8>>>>> =
    std::sync::OnceLock::new();

/// The terminal's settings as the host found them.
static TERMIOS: std::sync::OnceLock<libc::termios> = std::sync::OnceLock::new();

impl Host {
    /// **Receive `^C`.** The terminal, in its own cooked mode, turns the key
    /// into `SIGINT` to this process; catching it stops it ending the host,
    /// and the system is told at its next clock tick (Christine,
    /// 2026-09-23: *"^C should be received by host app and then sent to
    /// relevant process as a signal"*).
    pub fn catch_interrupt() {
        extern "C" fn caught(_: libc::c_int) {
            INTERRUPT.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        // SAFETY: the handler only stores to an atomic, which is
        // async-signal-safe.
        unsafe {
            libc::signal(libc::SIGINT, caught as extern "C" fn(libc::c_int) as libc::sighandler_t);
        }
        // **And the key is not echoed.** `rio` shows nothing for its
        // interrupt key: the command ends and rc's prompt is at the start
        // of the line, or — at the prompt — rc prints the newline itself
        // (*"if(Eintr()){ pchr(err, '\n'); …"*, `rc/exec.c:976`). A terminal
        // in cooked mode writes `^C` first, and the prompt lands after it;
        // so its echo of control characters is off while the host runs,
        // and put back by [`Host::restore_terminal`].
        //
        // SAFETY: plain calls on the terminal's settings, with a struct
        // `tcgetattr` filled.
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            if libc::isatty(0) == 1 && libc::tcgetattr(0, &mut t) == 0 {
                let _ = TERMIOS.set(t);
                t.c_lflag &= !libc::ECHOCTL;
                libc::tcsetattr(0, libc::TCSANOW, &t);
            }
        }
    }

    /// Put the terminal back as [`Host::catch_interrupt`] found it.
    pub fn restore_terminal() {
        if let Some(t) = TERMIOS.get() {
            // SAFETY: as in `catch_interrupt`.
            unsafe {
                libc::tcsetattr(0, libc::TCSANOW, t);
            }
        }
    }

    fn keys() -> &'static std::sync::Mutex<std::sync::mpsc::Receiver<Option<Vec<u8>>>> {
        KEYS.get_or_init(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                use std::io::BufRead;
                let stdin = std::io::stdin();
                loop {
                    let mut line = String::new();
                    match stdin.lock().read_line(&mut line) {
                        Ok(0) | Err(_) => {
                            let _ = tx.send(None);
                            return;
                        }
                        Ok(_) => {
                            if tx.send(Some(line.into_bytes())).is_err() {
                                return;
                            }
                        }
                    }
                }
            });
            std::sync::Mutex::new(rx)
        })
    }
}

impl Console for Host {
    /// `screenputs` (`devcons.c:12`). The screen this machine has is the
    /// terminal it was started from, and it is flushed at once: a prompt has
    /// no newline, and a prompt nobody sees is not a prompt.
    fn putstrn(&mut self, s: &[u8]) {
        use std::io::Write;
        let mut out = std::io::stdout();
        let _ = out.write_all(s);
        let _ = out.flush();
    }

    /// The keyboard: what the reading thread has, without waiting.
    ///
    /// A line at a time, because that is what a terminal in its own cooked
    /// mode gives. The kernel runs its OWN discipline over whatever arrives,
    /// which is why `rawon` still works: raw mode is about what the kernel
    /// does with the bytes, not how many arrive at once.
    fn kbdchars(&mut self) -> Option<Vec<u8>> {
        use std::sync::mpsc::TryRecvError;
        let rx = Self::keys().lock().ok()?;
        let mut got = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(Some(line)) => got.extend(line),
                Ok(None) | Err(TryRecvError::Disconnected) => {
                    return if got.is_empty() { None } else { Some(got) };
                }
                Err(TryRecvError::Empty) => return Some(got),
            }
        }
    }

    fn interrupt(&mut self) -> bool {
        INTERRUPT.swap(false, std::sync::atomic::Ordering::SeqCst)
    }

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

    /// `configfile[]` — **the kernel configuration file, verbatim**, which
    /// `portmkfile:53` embeds in the kernel image byte for byte and
    /// `/dev/config` reads back (`devcons.c:871`). It is `$CONF` — `pc/pcf`
    /// and its like — the file `mkdevc` turns into `devtab[]`, and it is NOT
    /// the boot arguments: those are `plan9.ini`, which the bootloader leaves
    /// in memory and which reaches userspace through `#ec`.
    ///
    /// This one is [`LETTERS`], in that file's own shape, because [`LETTERS`] is
    /// what `mkdevc` reads here.
    fn config(&mut self) -> String {
        let mut s = format!("# {CONFFILE} - Saranos on a terminal\ndev\n");
        for d in LETTERS {
            s.push_str(&format!("\t{}\n", d.name()));
        }
        s
    }

    fn reboot(&mut self, cmd: &str) -> Result<(), String> {
        match cmd {
            "halt" => std::process::exit(0),
            _ => Err("this host reboots by being started again".into()),
        }
    }

    /// One serial line, `eia0`, with nothing at its far end yet: the
    /// surface this host serves is the terminal, which reaches the system
    /// through `#c`.
    fn physuart(&mut self) -> Vec<(ipnx_kernel::devuart::Uart, Box<dyn ipnx_kernel::devuart::PhysUart>)> {
        vec![uart::Eia::found("eia0", uart::Line::new())]
    }
}

/// The device table — the letters this kernel carries. Plan 9 writes the same
/// list in a configuration file and `mkdevc` turns it into `devtab[]`.
///
/// Absent, and for one reason each: `#i` (draw) and `#m` (mouse) are hardware
/// this machine has none of.
pub const LETTERS: [DevId; 11] = [
    DevId::Root,
    DevId::Pipe,
    DevId::Srv,
    DevId::Mnt,
    DevId::Proc,
    DevId::Dup,
    DevId::Env,
    DevId::Cons,
    DevId::Cap,
    DevId::Virtio9p,
    DevId::Uart,
];

/// The boot namespace, from `plan9/sys/src/9/port/initcode.c:28`, which is
/// the whole of what Plan 9's first process does before `exec`:
///
/// ```c
/// bind(c, dev, MAFTER);            /* #c -> /dev  */
/// bind(ec, env, MAFTER);           /* #ec -> /env */
/// bind(e, env, MCREATE|MAFTER);    /* #e  -> /env */
/// bind(s, srv, MREPL|MCREATE);     /* #s  -> /srv */
/// ```
///
/// **Nothing else.** Everything the system needs beyond this is the system's
/// own business now: `boot` mounts the root, `/profile/start.ns` says what the
/// namespace is, and `/profile/start.rc` binds the rest. This list used to carry
/// three more, and each of them is now a line in one of those files.
///
/// `#ec` is the configuration environment (`devenv.c:16`), bound under `#e`
/// and without `MCREATE`, so the kernel's configuration reads through `/env`
/// and a process's own `setenv` lands in its own group. **Nothing fills it
/// yet**: it is `plan9.ini` that a Plan 9 kernel copies into it
/// (`pc/main.c:257`), and this host has no counterpart to `plan9.ini` — so
/// `$rootspec` and `$rootdir` are still absent and `boot` still has one root.
pub const BINDS: [(&str, &str, i32); 4] = [
    ("#c", "/dev", MAFTER),
    ("#ec", "/env", MAFTER),
    ("#e", "/env", MCREATE | MAFTER),
    ("#s", "/srv", MREPL | MCREATE),
];

/// `<libc.h>:556`.
pub const MREPL: i32 = 0;
pub const MAFTER: i32 = 2;
pub const MCREATE: i32 = 4;

/// `#c/cons`, opened three times — `initcode.c:28`. Not a dup: three opens,
/// so each descriptor has its own offset, and the first is for reading.
pub const CONS: &str = "#c/cons";

/// `arch->id` — what this machine is called, and therefore where its
/// binaries live: `/$cputype/bin` on Plan 9 (`pc/main.c:252` says `"386"`).
pub const OBJTYPE: &str = "wasm";

/// `conffile` — **the path of the kernel configuration file**, which
/// `mkdevc` writes into the kernel it generates (`port/mkdevc:187`:
/// `printf "char* conffile = \"%s/%s\";\n", pwd, ARGV[1]` — so
/// `/sys/src/9/pc/pcf`). `$terminal` is `arch->id` and this, and nothing
/// else (`pc/main.c:250`). Here `mkdevc`'s input is [`DEVS`], so the file
/// holding it is this one.
pub const CONFFILE: &str = "hosts/ipnx/src/lib.rs";

/// `ksetenv` (`devenv.c:386`) — `namec("#e%s/<name>", Acreate, OWRITE, 0600)`
/// and a write, where `%s` is `"c"` when `conf` is set. The kernel's own way
/// of putting something in the environment, which is how `pc/main.c` hands
/// the architecture's name to userspace — and, for every line of `plan9.ini`,
/// how the configuration reaches `#ec` (`pc/main.c:257`).
pub fn ksetenv(k: &mut Kernel, name: &str, val: &str, conf: bool) -> Result<(), String> {
    let path = format!("#e{}/{name}", if conf { "c" } else { "" });
    let Ret::Fd(fd) = k
        .syscall(1, Call::Create { path: path.clone(), mode: 1, perm: 0o600 })
        .map_err(|e| format!("create {path}: {e}"))?
    else {
        return Err(format!("create {path}"));
    };
    k.syscall(1, Call::Pwrite { fd, data: val.as_bytes().to_vec(), off: -1 })
        .map_err(|e| format!("write {path}: {e}"))?;
    k.syscall(1, Call::Close { fd }).map_err(|e| format!("close {path}: {e}"))?;
    Ok(())
}

/// **One command, as Plan 9 runs one at boot**: the `plan9.ini` line
/// `init=/$cputype/init -t cmd`. `boot` reads `$init` and tokenizes it into
/// init's arguments (`boot.c:208`), and `init` runs its first argument with
/// `rc -c` (`init.c:44`, `:171`) — so the command is one quoted token, and
/// each of its words is quoted within it, both as `tokenize` and rc unquote
/// (`''` for a quote inside quotes).
pub fn initcmd(args: &[String]) -> (String, String) {
    let line: Vec<String> = args.iter().map(|a| rcquote(a)).collect();
    ("init".to_string(), format!("/{OBJTYPE}/init -t {}", rcquote(&line.join(" "))))
}

/// **This system's `plan9.ini`** — the lines `pc/main.c:257` puts into the
/// environment and `#ec`. `user=` names the host owner: `boot` writes it to
/// `#c/hostowner` and falls back to Plan 9's `"glenda"` only when there is
/// none (`bootauth.c:56`, `userspace/cmd/boot.c`). This system's default
/// user is `kitty`. A command, when there is one, is `init=`.
pub const USER: &str = "kitty";

pub fn plan9ini(cmd: &[String]) -> Vec<(String, String)> {
    let mut conf = vec![("user".to_string(), USER.to_string())];
    if !cmd.is_empty() {
        conf.push(initcmd(cmd));
    }
    conf
}

/// A word as rc and `tokenize` read one: bare if nothing in it is special,
/// otherwise in single quotes with each quote doubled.
fn rcquote(w: &str) -> String {
    let plain = !w.is_empty()
        && w.chars().all(|c| c.is_alphanumeric() || "-_./+,:=@%".contains(c));
    if plain {
        return w.to_string();
    }
    format!("'{}'", w.replace('\'', "''"))
}

/// **A copy of the built root for this process alone** — what the tests and
/// the conformance suite boot on, rather than on `userspace/root` itself.
/// A boot writes (`/tmp`, `/env`, a test's own files), so two test
/// processes at once shared one tree and corrupted each other's files — the
/// same test in each wrote one `/tmp/long.rc` (2026-10-07) — a build that
/// replaced a program under a running test could fail its boot, and what a
/// test wrote was left in the root `ipnx` boots from.
///
/// Made on first use, removed when the process exits; a copy left by a
/// process that is gone — killed by a timeout — is removed by the next.
pub fn rootcopy() -> &'static std::path::Path {
    static COPY: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    extern "C" fn removed() {
        if let Some(p) = COPY.get() {
            let _ = std::fs::remove_dir_all(p);
        }
    }
    COPY.get_or_init(|| {
        let tmp = std::env::temp_dir();
        // the copies of processes that are gone
        if let Ok(entries) = std::fs::read_dir(&tmp) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                let Some(pid) = name.strip_prefix("ipnx-root-").and_then(|p| p.parse::<i32>().ok()) else { continue };
                // SAFETY: `kill` with signal 0 sends nothing; it asks whether
                // the process exists.
                let gone = unsafe { libc::kill(pid, 0) } != 0
                    && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
                if gone {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
        let from = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../userspace/root");
        let to = tmp.join(format!("ipnx-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&to);
        copytree(&from, &to).unwrap_or_else(|e| panic!("{}: {e} — run userspace/mk.sh", from.display()));
        // SAFETY: registers a plain function to run at `exit`.
        unsafe {
            libc::atexit(removed);
        }
        to
    })
}

fn copytree(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dst = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copytree(&e.path(), &dst)?;
        } else {
            std::fs::copy(e.path(), dst)?;
        }
    }
    Ok(())
}

/// The first process, and the only file `#/boot` carries — as a Plan 9
/// kernel carries `/boot/boot` and nothing else (`initcode.c:11`).
pub const BOOT: &str = "/boot/boot";

pub fn boot(
    root: Root,
    host: Box<dyn Console>,
    store: Option<Box<dyn Nineserver>>,
) -> Result<Kernel, String> {
    let m = machine::Wasm::new()?;
    let mut k = Kernel::new(root, Rc::new(m))?;
    k.tab.add(Box::new(PipeDev::new(k.up.clone())));
    // `#s`'s table is shared with `#p`, because `srvname` (`devsrv.c`) is
    // what `#p/<n>/ns` calls to name the server behind a mount.
    let srv = SrvDev::new(k.up.clone());
    let srvtab = srv.table();
    k.tab.add(Box::new(srv));
    k.tab.add(Box::new(MntDev::new()));
    k.tab.add(Box::new(ProcDev::new(k.up.clone()).with_srv(srvtab)));
    k.tab.add(Box::new(DupDev::new(k.up.clone())));
    k.tab.add(Box::new(EnvDev::new(k.up.clone())));
    k.tab.add(Box::new(CapDev::new(k.up.clone())));
    // `v9probe` (`v9reset`, `devvirtio9p.c:1066`) — what the machine found.
    // One server, so one file: `#9/0`.
    let mut v9 = Virtio9p::new();
    if let Some(store) = store {
        v9.add(store);
    }
    k.tab.add(Box::new(v9));
    // `uartreset` (`devuart.c:163`) — the lines the host has.
    let mut host = host;
    k.tab.add(Box::new(UartDev::new(k.up.clone(), host.physuart())));
    // **`eve` starts EMPTY**, as `userinit` leaves it (`pc/main.c:285`:
    // `kstrdup(&eve, "")`). `boot` names the host owner by writing
    // `#c/hostowner` — `glenda()` (`bootauth.c:56`), reached because there
    // is no `/boot/factotum` here — and `hostownerwrite` allows the first
    // one because `iseve()` is then comparing two empty strings
    // (`auth.c:128`).
    k.tab.add(Box::new(Cons::new(k.tab.eve(), k.up.clone(), LETTERS.to_vec(), host)));
    Ok(k)
}

/// Load `userspace/root/bin` into `#/boot`, which is where a Plan 9 kernel
/// keeps the files a first process needs (`devroot.c:27`: `addbootfile`) —
/// enough to start something, and that something mounts the real server.
pub fn loadbin(root: &mut Root, dir: &std::path::Path) -> usize {
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

/// **`startboot`** (`plan9/sys/src/9/port/initcode.c:21`), which is the whole
/// of what a Plan 9 kernel's first process does — and now the whole of what
/// this embedding does before the system takes over:
///
/// ```c
/// open(cons, OREAD); open(cons, OWRITE); open(cons, OWRITE);
/// bind(c, dev, MAFTER); bind(ec, env, MAFTER);
/// bind(e, env, MCREATE|MAFTER); bind(s, srv, MREPL|MCREATE);
/// exec(boot, argv);
/// ```
///
/// It is a function rather than `main`'s body so that a test can run the real
/// thing — the real kernel, the real namespace, the real binaries — with a
/// terminal it can script and inspect.
///
/// `extra` puts more files in the boot list, for a test that wants the first
/// process to be something other than `boot`.
pub fn startboot(
    argv: &[String],
    extra: &[(&str, &[u8])],
    conf: &[(String, String)],
    host: Box<dyn Console>,
    store: Option<Box<dyn Nineserver>>,
) -> Result<String, String> {
    let mut root = Root::new();
    let bootdir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../userspace/build/boot");
    if loadbin(&mut root, &bootdir) == 0 && extra.is_empty() {
        return Err(format!("nothing in {}: run userspace/mk.sh", bootdir.display()));
    }
    for (name, bytes) in extra {
        root.addbootfile(name, bytes.to_vec());
    }

    let mut k = boot(root, host, store)?;

    // `open(cons, OREAD); open(cons, OWRITE); open(cons, OWRITE);` — three
    // opens of `#c/cons`, before the binds, because `/dev` does not exist
    // yet. They land on 0, 1 and 2 because those are the lowest free.
    for mode in [chan::mode::OREAD, chan::mode::OWRITE, chan::mode::OWRITE] {
        k.syscall(1, Call::Open { path: CONS.into(), mode: mode as i32 })
            .map_err(|e| format!("open {CONS}: {e}"))?;
    }

    for (what, at, flag) in BINDS {
        k.syscall(1, Call::Bind { name: what.into(), old: at.into(), flag })
            .map_err(|e| format!("bind {what} {at}: {e}"))?;
    }

    // **The machine names itself** (`pc/main.c:250`):
    //
    // ```c
    // snprint(buf, sizeof(buf), "%s %s", arch->id, conffile);
    // ksetenv("terminal", buf, 0);
    // ksetenv("cputype", "386", 0);
    // ksetenv("service", "terminal", 0);
    // ```
    //
    // `ksetenv` is `namec("#e/<name>", Acreate, OWRITE, 0600)` and a write
    // (`devenv.c:386`). These three are the whole of what a Plan 9 kernel
    // puts in the environment, and `$objtype` — which `/profile/start.ns` uses
    // to find the binaries — is init's copy of `cputype`.
    for (name, val) in [
        ("terminal", format!("{OBJTYPE} {CONFFILE}")),
        ("cputype", OBJTYPE.to_string()),
        ("service", "terminal".to_string()),
    ] {
        ksetenv(&mut k, name, &val, false)?;
    }
    // *"for(i = 0; i < nconf; i++){ if(confname[i][0] != '*')
    // ksetenv(confname[i], confval[i], 0); ksetenv(confname[i], confval[i],
    // 1); }"* (`pc/main.c:257`) — each line of `plan9.ini`, into the
    // environment and into `#ec`. The host's configuration is its
    // `plan9.ini`.
    for (name, val) in conf {
        if !name.starts_with('*') {
            ksetenv(&mut k, name, val, false)?;
        }
        ksetenv(&mut k, name, val, true)?;
    }

    // `exec(boot, argv)` — `initcode`'s last line. It no longer runs
    // anything: the image is pid 1's now and pid 1 is `Ready`.
    let path = argv.first().map(String::as_str).unwrap_or(BOOT).to_string();
    k.exec(1, &path, argv)?;

    // **`schedinit()`, which never returns** (`proc.c:67`). Plan 9 reaches
    // it from `main` on every processor and the system is whatever the
    // processes do from there. Here it returns when nothing is left to run,
    // which is the end of the session, and the status is pid 1's.
    // Pid 1 has an image and is not on a queue: `ready` is what puts it
    // there, as `newproc`'s caller does for every other process.
    k.procs.borrow_mut().ready(1);
    k.schedinit()?;
    let status = k.procs.borrow().status(1).unwrap_or_default();
    Ok(status)
}
/// A console that answers with a script and remembers what it was shown —
/// the counterpart of a person at a terminal, for a test or a suite. The
/// keys go in, the screen comes out.
///
/// **`^C` in the script is the interrupt key**, pressed a moment after what
/// comes before it was typed — or, given a **marker** ([`Term::marked`]), a
/// moment after the screen shows it and then stays still: once the command
/// the line ends with is running and quiet, which is when a person presses
/// the key. Timed from the typing alone, a machine still compiling an image
/// was still starting the command when the key came — the note ended what
/// was being started, or reached the shell's child before its `exec`, which
/// clears notes (`sysproc.c:579`) — and the command ran its course. The
/// keys are typed when the console is first polled, early in the boot, so
/// only what the script itself prints can say how far it has got. What
/// comes after the key is typed once the interrupt has been handed over.
#[derive(Clone, Default)]
pub struct Term(std::rc::Rc<std::cell::RefCell<Script>>);

#[derive(Default)]
pub struct Script {
    screen: Vec<u8>,
    /// The keys, split at each `^C`; the first is what is being typed now.
    keys: std::collections::VecDeque<Vec<u8>>,
    /// Whether the first has been typed.
    typed: bool,
    /// When the next `^C` is pressed, at the earliest.
    at: Option<std::time::Instant>,
    /// When the screen last changed.
    shown: Option<std::time::Instant>,
    /// What the screen must show, since the keys before the next `^C` were
    /// typed, before the key is pressed; and where on the screen they were.
    marker: Option<Vec<u8>>,
    from: usize,
    /// The serial line `eia0` is on, if the test has one.
    line: Option<uart::Line>,
}

/// How long after the line before it a scripted `^C` is pressed.
const INTERRUPT_AFTER: std::time::Duration = std::time::Duration::from_secs(2);

impl Term {
    pub fn typing(keys: &str) -> Term {
        let t = Term::default();
        t.0.borrow_mut().keys = keys.split('\x03').map(|k| k.as_bytes().to_vec()).collect();
        t
    }
    /// The same, each `^C` pressed only once the screen has shown `marker`
    /// since the keys before it were typed, and been still a moment.
    pub fn marked(keys: &str, marker: &str) -> Term {
        let t = Term::typing(keys);
        t.0.borrow_mut().marker = Some(marker.as_bytes().to_vec());
        t
    }
    /// The same, with `eia0` on this line — whose far end the test holds.
    pub fn with_line(keys: &str, line: uart::Line) -> Term {
        let t = Term::typing(keys);
        t.0.borrow_mut().line = Some(line);
        t
    }
    /// What the console was shown.
    pub fn screen(&self) -> String {
        String::from_utf8_lossy(&self.0.borrow().screen).into_owned()
    }
}

impl Script {
    /// Whether a marked `^C` may be pressed: the marker shown since the
    /// keys were typed, and the screen still since.
    fn ready(&self) -> bool {
        let Some(m) = &self.marker else { return true };
        let since = &self.screen[self.from.min(self.screen.len())..];
        since.windows(m.len()).any(|w| w == m.as_slice())
            && self.shown.is_some_and(|s| std::time::Instant::now() >= s + INTERRUPT_AFTER)
    }
}

impl Console for Term {
    fn putstrn(&mut self, s: &[u8]) {
        let mut t = self.0.borrow_mut();
        t.screen.extend_from_slice(s);
        t.shown = Some(std::time::Instant::now());
    }
    /// What is typed before the next `^C`, all at once; then nothing until
    /// it is pressed; and after the last, the end of input — which is what
    /// a terminal being closed looks like.
    fn kbdchars(&mut self) -> Option<Vec<u8>> {
        let mut t = self.0.borrow_mut();
        let first = t.keys.front()?.clone();
        if !t.typed {
            t.typed = true;
            if t.keys.len() > 1 {
                t.at = Some(std::time::Instant::now() + INTERRUPT_AFTER);
                t.from = t.screen.len();
            }
            return Some(first);
        }
        if t.keys.len() == 1 {
            return None;
        }
        Some(Vec::new())
    }
    fn interrupt(&mut self) -> bool {
        let mut t = self.0.borrow_mut();
        match t.at {
            Some(at) if std::time::Instant::now() >= at && t.ready() => {
                t.at = None;
                t.keys.pop_front();
                t.typed = false;
                true
            }
            _ => false,
        }
    }
    fn now(&mut self) -> (u64, u64, u64) {
        Host.now()
    }
    fn random(&mut self, n: usize) -> Vec<u8> {
        Host.random(n)
    }
    fn drivers(&mut self) -> Vec<String> {
        Host.drivers()
    }
    fn memory(&mut self) -> (u64, u64, u64) {
        Host.memory()
    }
    fn config(&mut self) -> String {
        Host.config()
    }
    fn reboot(&mut self, cmd: &str) -> Result<(), String> {
        Host.reboot(cmd)
    }
    fn physuart(&mut self) -> Vec<(ipnx_kernel::devuart::Uart, Box<dyn ipnx_kernel::devuart::PhysUart>)> {
        let line = self.0.borrow().line.clone().unwrap_or_default();
        vec![uart::Eia::found("eia0", line)]
    }
}
