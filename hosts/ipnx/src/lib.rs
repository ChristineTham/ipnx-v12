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


#[cfg(not(target_arch = "wasm32"))]
pub mod machine;
pub mod store;

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
    devvirtio9p::{Nineserver, Virtio9p},
    dev::DevId,
    Call, Kernel, Ret,
};
use ipnx_kernel::machine::Machine;
use std::rc::Rc;

/// What `#c` reports about the machine underneath. The kernel names these
/// files; only the host can fill them, which is the arrangement Plan 9 has for
/// every number `devcons` reports — it reads them from the architecture.
#[cfg(not(target_arch = "wasm32"))]
pub struct Host;

/// The interrupt key has been pressed and not yet handed over.
#[cfg(not(target_arch = "wasm32"))]
static INTERRUPT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What the terminal has typed, a line at a time, read on a thread of its
/// own so that waiting for a key holds up nothing else — Plan 9's keyboard
/// interrupts, as a machine without them has them. `None` is end of input.
#[cfg(not(target_arch = "wasm32"))]
static KEYS: std::sync::OnceLock<std::sync::Mutex<std::sync::mpsc::Receiver<Option<Vec<u8>>>>> =
    std::sync::OnceLock::new();

/// The terminal's settings as the host found them.
#[cfg(not(target_arch = "wasm32"))]
static TERMIOS: std::sync::OnceLock<libc::termios> = std::sync::OnceLock::new();

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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
}

/// The device table — the letters this kernel carries. Plan 9 writes the same
/// list in a configuration file and `mkdevc` turns it into `devtab[]`.
///
/// Absent, and for one reason: `#i` (draw), `#m` (mouse) and `#t` (uart)
/// are hardware this machine has none of, and none is emulated (Christine,
/// 2026-10-08).
pub const LETTERS: [DevId; 10] = [
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
/// **Nothing else.** Beyond this the host does `boot`'s part — the host
/// owner and the root ([`startboot`]) — and the rest is the system's own
/// business: `/profile/start.ns` says what the namespace is, and
/// `/profile/start.rc` binds the rest. This list used to carry three more,
/// and each of them is now a line in one of those files.
///
/// `#ec` is the configuration environment (`devenv.c:16`), bound under `#e`
/// and without `MCREATE`, so the kernel's configuration reads through `/env`
/// and a process's own `setenv` lands in its own group. **Nothing fills it
/// but the lines of [`plan9ini`]**: it is `plan9.ini` that a Plan 9 kernel
/// copies into it (`pc/main.c:257`), and this host's has `user=` and, with a
/// command, `init=` — so `$rootspec` and `$rootdir` are absent and the host
/// attaches the one root it has.
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
/// `init=/$cputype/init -t cmd`. The host reads `$init` and tokenizes it into
/// init's arguments, as Plan 9's boot does ([`execinit`]), and `init` runs its first argument with
/// `rc -c` (`init.c:44`, `:171`) — so the command is one quoted token, and
/// each of its words is quoted within it, both as `tokenize` and rc unquote
/// (`''` for a quote inside quotes).
pub fn initcmd(args: &[String]) -> (String, String) {
    let line: Vec<String> = args.iter().map(|a| rcquote(a)).collect();
    ("init".to_string(), format!("/{OBJTYPE}/init -t {}", rcquote(&line.join(" "))))
}

/// **This system's `plan9.ini`** — the lines `pc/main.c:257` puts into the
/// environment and `#ec`. `user=` names the host owner: the host writes it to
/// `#c/hostowner` and falls back to Plan 9's `"glenda"` only when there is
/// none ([`authentication`], after `bootauth.c:56`). This system's default
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
#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
pub fn boot(
    root: Root,
    host: Box<dyn Console>,
    store: Option<Box<dyn Nineserver>>,
) -> Result<Kernel, String> {
    boot_with(Rc::new(machine::Wasm::new()?), root, host, store.into_iter().collect())
}

/// [`boot`] on a machine the caller supplies — the browser's, or wasmtime —
/// with the 9P servers it provides, which are `#9/0`, `#9/1`, … in order:
/// the root's store first.
pub fn boot_with(
    machine: Rc<dyn Machine>,
    root: Root,
    host: Box<dyn Console>,
    servers: Vec<Box<dyn Nineserver>>,
) -> Result<Kernel, String> {
    let mut k = Kernel::new(root, machine)?;
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
    let mut v9 = Virtio9p::new(k.up.clone());
    for server in servers {
        v9.add(server);
    }
    k.tab.add(Box::new(v9));
    // **`eve` starts EMPTY**, as `userinit` leaves it (`pc/main.c:285`:
    // `kstrdup(&eve, "")`). The host names the host owner by writing
    // `#c/hostowner`, as Plan 9's boot does with no factotum to start
    // (`glenda()`, `bootauth.c:56`), and `hostownerwrite` allows the first
    // one because `iseve()` is then comparing two empty strings
    // (`auth.c:128`).
    k.tab.add(Box::new(Cons::new(k.tab.eve(), k.up.clone(), LETTERS.to_vec(), host)));
    Ok(k)
}

/// **`startboot`** (`plan9/sys/src/9/port/initcode.c:21`), which is the whole
/// of what a Plan 9 kernel's first process does —
///
/// ```c
/// open(cons, OREAD); open(cons, OWRITE); open(cons, OWRITE);
/// bind(c, dev, MAFTER); bind(ec, env, MAFTER);
/// bind(e, env, MCREATE|MAFTER); bind(s, srv, MREPL|MCREATE);
/// exec(boot, argv);
/// ```
///
/// — and then **what Plan 9's `boot` does, done by the host** (Christine,
/// 2026-10-08: *"The host does it"*): name the host owner, attach the root
/// file server, post it as `#s/root` and make it `/`, then start `init`
/// ([`authentication`], [`connectroot`], [`nsinit`], [`execinit`]). There
/// is no `/boot/boot`, because `boot` is the Unix bootloader's name.
///
/// It is a function rather than `main`'s body so that a test can run the real
/// thing — the real kernel, the real namespace, the real binaries — with a
/// terminal it can script and inspect.
#[cfg(not(target_arch = "wasm32"))]
pub fn startboot(
    conf: &[(String, String)],
    host: Box<dyn Console>,
    store: Option<Box<dyn Nineserver>>,
) -> Result<String, String> {
    startboot_with(Rc::new(machine::Wasm::new()?), CONFFILE, conf, host, store.into_iter().collect())
}

/// [`startboot`] on a machine the caller supplies, whose configuration file
/// is `conffile` — what `$terminal` names (`pc/main.c:250`) — and the 9P
/// servers it provides, the root's store first ([`boot_with`]).
pub fn startboot_with(
    machine: Rc<dyn Machine>,
    conffile: &str,
    conf: &[(String, String)],
    host: Box<dyn Console>,
    servers: Vec<Box<dyn Nineserver>>,
) -> Result<String, String> {
    let mut k = boot_with(machine, Root::new(), host, servers)?;

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
        ("terminal", format!("{OBJTYPE} {conffile}")),
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

    // `boot()`'s order (`boot/boot.c:284`–`:296`): the host owner, then the
    // root, then init. What else it does is for hardware this machine does
    // not have — `usbinit`, `kbmap`, `#æ` and `#S`, `partinit`, `swapproc`
    // — or is answered already: `settime`, because the clock is the host's;
    // `rfork(RFNAMEG)`, because there are no processes yet to leave behind;
    // a choice of method, because there is one, `#9/0`.
    authentication(&mut k, conf)?;
    let fd = connectroot(&mut k)?;
    let afd = nsinit(&mut k, fd, conf)?;
    // `close(fd)` (`:290`) has nothing to do: the mount closed it
    // (`bindmount`'s *"fdclose(fd, 0)"*, `sysfile.c:1061`).
    if afd > 0 {
        let _ = k.syscall(1, Call::Close { fd: afd });
    }
    let (path, argv) = execinit(conf);
    k.exec(1, &path, &argv)?;

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

/// A line of the configuration, as `getenv` would find it.
fn confget<'a>(conf: &'a [(String, String)], name: &str) -> Option<&'a str> {
    conf.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}

/// `authentication` (`boot/bootauth.c:10`), and the branch it takes: with
/// no factotum to start, `glenda()` (`:56`) — **this names the host
/// owner**, writing `$user`, or Plan 9's `"glenda"` when there is none, to
/// `#c/hostowner`. `eve` is the empty string until now (`pc/main.c:285`),
/// so `hostownerwrite` permits it: `iseve()` compares two empty strings
/// (`auth.c:128`).
pub fn authentication(k: &mut Kernel, conf: &[(String, String)]) -> Result<(), String> {
    let s = confget(conf, "user").unwrap_or("glenda");
    let Ret::Fd(fd) = k.syscall(1, Call::Open { path: "#c/hostowner".into(), mode: chan::mode::OWRITE as i32 })?
    else {
        return Ok(());
    };
    if let Err(e) = k.syscall(1, Call::Pwrite { fd, data: s.as_bytes().to_vec(), off: -1 }) {
        return Err(format!("setting #c/hostowner to {s}: {e}"));
    }
    k.syscall(1, Call::Close { fd })?;
    Ok(())
}

/// `connectroot` (`boot/boot.c:125`): connect — `bootvirtio9p.c`'s whole
/// body, *"fd = open("#9/0", ORDWR)"* — negotiate the version, and post
/// the channel at `#s/root` (`srvcreate`, `boot/aux.c:125`; Plan 9's
/// `#s/boot`, renamed by Christine, 2026-10-08) **before** anything mounts
/// it, because the version is negotiated once per connection and whoever
/// mounts it next joins that session. The descriptor stays open for
/// [`nsinit`].
pub fn connectroot(k: &mut Kernel) -> Result<i32, String> {
    let Ret::Fd(fd) = k
        .syscall(1, Call::Open { path: "#9/0".into(), mode: chan::mode::ORDWR as i32 })
        .map_err(|e| format!("can't connect to file server: {e}"))?
    else {
        return Err("can't connect to file server".into());
    };
    k.syscall(1, Call::Fversion { fd, msize: 0, version: String::new() })
        .map_err(|e| format!("can't init 9P: {e}"))?;
    let name = format!("#s/{}", ipnx_kernel::devsrv::ROOTSRV);
    let Ret::Fd(f) = k.syscall(1, Call::Create { path: name.clone(), mode: chan::mode::OWRITE as i32, perm: 0o666 })?
    else {
        return Err(format!("create {name}"));
    };
    k.syscall(1, Call::Pwrite { fd: f, data: fd.to_string().into_bytes(), off: -1 })
        .map_err(|e| format!("write {name}: {e}"))?;
    k.syscall(1, Call::Close { fd: f })?;
    Ok(fd)
}

/// `nsinit` (`boot/boot.c:152`). The order is the whole of it:
///
/// ```c
/// bind("/", "/", MREPL)                      make the root a union of its own
/// mount(fd, afd, "/root", MREPL|MCREATE, rp)  the server, somewhere to stand
/// bind(rootdir, "/", MAFTER|MCREATE)
/// ```
///
/// That last line is what makes **the root a file server**: after it, `/`
/// answers from `#/` first and from the server after. `fauth` asks the
/// server whether it wants authentication; the host's does not
/// (`u9fs`'s `authnone`), and if one did, Plan 9's `auth_proxy` would need
/// a factotum this machine has not started — so the mount goes ahead, as
/// boot's does when that fails (`:169`). The answer is the authentication
/// descriptor, or -1.
pub fn nsinit(k: &mut Kernel, fd: i32, conf: &[(String, String)]) -> Result<i32, String> {
    k.syscall(1, Call::Bind { name: "/".into(), old: "/".into(), flag: MREPL })
        .map_err(|e| format!("bind /: {e}"))?;
    let rp = confget(conf, "rootspec").unwrap_or("").to_string();
    let afd = match k.syscall(1, Call::Fauth { fd, aname: rp.clone() }) {
        Ok(Ret::Fd(a)) => a,
        _ => -1,
    };
    k.syscall(1, Call::Mount { fd, afd, old: "/root".into(), flag: MREPL | MCREATE, aname: rp })
        .map_err(|e| format!("mount /: {e}"))?;
    // `$rootdir`, and failing a bind of it, the same under `/root`
    // (`:174`–`:194`). The installer's own case after that, `/plan9`, is
    // for a Plan 9 installation and has nothing to undo here.
    let mut rp = confget(conf, "rootdir").unwrap_or(ROOTDIR).to_string();
    let bind = |k: &mut Kernel, rp: &str| k.syscall(1, Call::Bind { name: rp.into(), old: "/".into(), flag: MAFTER | MCREATE });
    if let Err(e) = bind(k, &rp) {
        if rp.starts_with("/root") {
            return Err(format!("couldn't bind $rootdir={rp} to root: {e}"));
        }
        rp = format!("/root/{rp}");
        bind(k, &rp).map_err(|e| format!("couldn't bind $rootdir={rp} to root: {e}"))?;
    }
    // *"setenv("rootdir", rp)"* (`:196`)
    ksetenv(k, "rootdir", &rp, false)?;
    Ok(afd)
}

/// `rootdir`: where the root server is mounted — `/root`, which `mkboot`
/// writes into every boot it makes (`boot/mkboot:65`, `:78`).
pub const ROOTDIR: &str = "/root";

/// `execinit` (`boot/boot.c:202`): `$init` — a line of `plan9.ini` — is
/// init's command line, `tokenize`d, its first word's last element
/// `argv[0]`. With none, it is Plan 9's default, *"/%s/init -%s%s"*:
/// `$cputype`, `t` for a terminal or `c` for a cpu server, and `m` for
/// `boot -m`, which nothing passes here. NOT `/bin/init`, because `/bin` is
/// a union `/profile/start.ns` makes and nothing has read that file yet.
pub fn execinit(conf: &[(String, String)]) -> (String, Vec<String>) {
    let cmd = match confget(conf, "init") {
        Some(c) => c.to_string(),
        None => format!("/{OBJTYPE}/init -t"),
    };
    let mut argv = tokenize(&cmd);
    let path = argv.first().cloned().unwrap_or_default();
    if let Some(first) = argv.first_mut() {
        // *"make iargv[0] basename(iargv[0])"*
        if let Some(i) = first.rfind('/') {
            *first = first[i + 1..].to_string();
        }
    }
    (path, argv)
}

/// `tokenize` (`libc/port/tokenize.c:93`): words separated by blanks, tabs
/// and newlines; `'…'` quotes, and `''` within quotes is one quote.
pub fn tokenize(s: &str) -> Vec<String> {
    let sep = |c: char| matches!(c, ' ' | '\t' | '\r' | '\n');
    let mut out = Vec::new();
    let mut it = s.chars().peekable();
    loop {
        while it.peek().is_some_and(|&c| sep(c)) {
            it.next();
        }
        if it.peek().is_none() {
            break;
        }
        let mut word = String::new();
        let mut quoting = false;
        while let Some(&c) = it.peek() {
            if !quoting && sep(c) {
                break;
            }
            it.next();
            if c != '\'' {
                word.push(c);
            } else if !quoting {
                quoting = true;
            } else if it.peek() == Some(&'\'') {
                // doubled quote; fold one quote into two
                it.next();
                word.push('\'');
            } else {
                quoting = false;
            }
        }
        out.push(word);
    }
    out
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
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Default)]
pub struct Term(std::rc::Rc<std::cell::RefCell<Script>>);

#[cfg(not(target_arch = "wasm32"))]
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
}

/// How long after the line before it a scripted `^C` is pressed.
#[cfg(not(target_arch = "wasm32"))]
const INTERRUPT_AFTER: std::time::Duration = std::time::Duration::from_secs(2);

#[cfg(not(target_arch = "wasm32"))]
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
    /// What the console was shown.
    pub fn screen(&self) -> String {
        String::from_utf8_lossy(&self.0.borrow().screen).into_owned()
    }
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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
}
