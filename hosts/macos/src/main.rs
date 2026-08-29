// The macOS embedding shim: wasmtime guests over the Rust kernel core.
// One OS thread per guest process (the Worker equivalent); syscalls cross an
// mpsc channel and block the guest thread on its reply (Atomics.wait's role).
// The asyncify machinery — the fork guard, bare fork, setjmp/longjmp, thread
// contexts — is guestcore.mjs ported onto wasmtime, with one structural
// simplification the native host affords: the child-catching guard needs no
// hand-written wasm here, because the host-function frame boundary of the
// nested __forkshim call plays catch_all's role (a host error unwinds the
// nested wasm frames and stops exactly at our Rust frame).

mod wasi;

use kernel::{AsySnap, Effect, KAction, KReply, Kernel, Pid, Seed, TXSIZE};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use wasmtime::{Caller, Config, Engine, Extern, Func, Instance, Linker, Memory, MemoryType, Module, Store, Val};

// ---- events into the kernel loop ----
mod ui;

pub enum Ev {
    Sys { worker_pid: Pid, trap: i32, a: [i32; 5], tx: Vec<u8>, reply: Sender<KReply> },
    Started { pid: Pid, asyncified: bool },
    AsyFork { parent: Pid, child: Pid, snap: Vec<u8>, data_ptr: u32, sp: u32 },
    Died { pid: Pid, msg: String },
    Timer { token: u64 },
    Stdin(Vec<u8>),
    StdinClosed,
    WinKey { wid: u32, bytes: Vec<u8> },
    WinMouse { wid: u32, x: i32, y: i32, b: i32 },
    WinClose { wid: u32 },
    WinTick,
    WinAck { wid: u32 },
    FetchDone { url: String, result: Result<Vec<u8>, String> },
}

// ---- how a guest run ends (carried through wasmtime host errors) ----
#[derive(Debug)]
pub enum GuestExit {
    ForkResume(i32),
    Exec(Arc<Vec<u8>>, Vec<String>),
    Retire,
    Die,
}
impl std::fmt::Display for GuestExit {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "guest exit: {:?}", std::mem::discriminant(self))
    }
}
impl std::error::Error for GuestExit {}

// per-guest-thread state living in the Store
struct RState {
    pid: Pid,
    ev: Sender<Ev>,
    memory: Option<Memory>,
    in_note: bool,
    last_aux: i32,
    saved_stack: Option<Vec<u8>>,
    rewinding: bool,
    rewind_return: i32,
    pending_fork: Option<(i32, u32)>, // (child pid, databuf); sp read at unwind
    pending_sj: Option<Sj>,
    sj_resume: Option<i32>,
    sjmap: HashMap<u32, (Vec<u8>, u32)>,          // env -> (frames, sp)
    tcmap: HashMap<u32, (Vec<u8>, u32, Vec<u8>)>, // id -> (frames, sp, shadow)
    last_sjbuf: u32,
    stack_top: u32,
}

#[derive(Clone)]
struct Sj {
    kind: u8, // 0 set, 1 long, 2 tsave, 3 tjump
    env: u32,
    val: i32,
    databuf: u32,
}

// traps whose a0 is a path/string, per guestcore.mjs
const T_STR: [i32; 13] = [2, 3, 7, 8, 14, 22, 25, 41, 42, 44, 60, 61, 62];

fn mem_of(caller: &mut Caller<'_, RState>) -> Memory {
    caller.data().memory.expect("memory set at instantiation")
}

fn cstr_from(mem: &Memory, caller: &Caller<'_, RState>, ptr: u32) -> Vec<u8> {
    let data = mem.data(caller);
    let start = ptr as usize;
    let end = data[start..].iter().position(|&b| b == 0).map(|i| start + i).unwrap_or(data.len());
    data[start..end].to_vec()
}

// the runner's half of the syscall: marshal, send, block, copy out
fn sys(caller: &mut Caller<'_, RState>, trap: i32, a0: i32, a1: i32, a2: i32, a3: i32, a4: i32)
       -> Result<i32, wasmtime::Error> {
    let mem = mem_of(caller);
    let mut tx: Vec<u8> = Vec::new();
    let mut a = [a0, a1, a2, a3, a4];
    fn push_str(tx: &mut Vec<u8>, mem: &Memory, caller: &Caller<'_, RState>, ptr: u32) {
        let s = cstr_from(mem, caller, ptr);
        tx.extend_from_slice(&s);
        tx.push(0);
    }
    if T_STR.contains(&trap) {
        push_str(&mut tx, &mem, &*caller, a0 as u32);
        if trap == 7 {
            // exec: path then argc counted argv strings
            let mut argc = 0;
            let mut pp = a1 as u32;
            loop {
                let sp = {
                    let data = mem.data(&*caller);
                    u32::from_le_bytes(data[pp as usize..pp as usize + 4].try_into().unwrap())
                };
                if sp == 0 {
                    break;
                }
                push_str(&mut tx, &mem, &*caller, sp);
                argc += 1;
                pp += 4;
            }
            a[2] = argc; // empty argv strings must survive
        }
        if trap == 2 || trap == 60 || trap == 61 {
            push_str(&mut tx, &mem, &*caller, a1 as u32); // bind/link/symlink: two strings
        }
        if trap == 44 {
            let data = mem.data(&*caller);
            tx.extend_from_slice(&data[a1 as usize..(a1 + a2) as usize]); // wstat record
        }
    }
    if trap == 45 {
        let data = mem.data(&*caller);
        tx.extend_from_slice(&data[a1 as usize..(a1 + a2) as usize]);
    }
    if trap == 35 {
        // unmount: name may be nil
        if a0 != 0 {
            push_str(&mut tx, &mem, &*caller, a0 as u32);
        } else {
            tx.push(0);
        }
        push_str(&mut tx, &mem, &*caller, a1 as u32);
    }
    if trap == 46 {
        // mount: old at a2, aname at a4 (guestcore's rule)
        push_str(&mut tx, &mem, &*caller, a2 as u32);
        push_str(&mut tx, &mem, &*caller, a4 as u32);
    }
    if trap == 51 {
        let data = mem.data(&*caller);
        let n = (a2 as usize).min(TXSIZE);
        tx.extend_from_slice(&data[a1 as usize..a1 as usize + n]); // pwrite buf
    }
    let ev = caller.data().ev.clone();
    let worker_pid = caller.data().pid;
    // a fresh channel per call: an interrupted call's late device completion
    // lands in a dropped receiver instead of poisoning the next syscall
    let (reply_tx, reply_rx) = channel::<KReply>();
    ev.send(Ev::Sys { worker_pid, trap, a, tx, reply: reply_tx })
        .map_err(|_| wasmtime::Error::msg("kernel gone"))?;
    let r = reply_rx.recv().map_err(|_| wasmtime::Error::msg("kernel gone"))?;
    caller.data_mut().last_aux = r.aux;
    match r.action {
        KAction::ForkResume => {
            // child left; restore the parent's scribbled stack and unwind to
            // the guard's host frame
            if let Some(saved) = caller.data_mut().saved_stack.take() {
                mem.write(&mut *caller, 0, &saved).ok();
            }
            return Err(GuestExit::ForkResume(r.aux).into());
        }
        KAction::ExecSelf => {
            let (image, argv) = r.load.expect("ExecSelf carries the image");
            return Err(GuestExit::Exec(image, argv).into());
        }
        KAction::Retire => return Err(GuestExit::Retire.into()),
        KAction::Die => return Err(GuestExit::Die.into()),
        KAction::None => {}
    }
    // copy-outs, per trap (guestcore's table)
    let ret = r.ret;
    if ret > 0 || (trap == 21 && ret == 0) {
        let dst: Option<u32> = match trap {
            50 => Some(a1 as u32),           // pread -> buf
            42 | 43 => Some(a1 as u32),      // stat/fstat -> edir
            41 | 47 | 200 | 202 => Some(a0 as u32), // errstr/await/args/noteget
            62 | 23 => Some(a1 as u32),      // readlink/fd2path
            53 => Some(a0 as u32),           // nsec -> vlong*
            21 => Some(a0 as u32),           // pipe -> fd[2]
            211 => Some(a0 as u32),          // iowait -> tag+data
            _ => None,
        };
        if let Some(dst) = dst {
            mem.write(&mut *caller, dst as usize, &r.data).ok();
        }
    }
    // V7 timing: pending notes dispatch after the call returns — never on the
    // fork return (the guard frame is live there), never reentrantly
    if r.note_pending && trap != 8 && trap != 19 && !caller.data().in_note {
        caller.data_mut().in_note = true;
        if let Some(Extern::Func(f)) = caller.get_export("__notedispatch") {
            let _ = f.typed::<(), ()>(&mut *caller).and_then(|tf| tf.call(&mut *caller, ()));
        }
        caller.data_mut().in_note = false;
    }
    Ok(ret)
}

fn call1(caller: &mut Caller<'_, RState>, name: &str, arg: i32) -> Result<(), wasmtime::Error> {
    if let Some(Extern::Func(f)) = caller.get_export(name) {
        f.typed::<i32, ()>(&mut *caller)?.call(&mut *caller, arg)?;
    }
    Ok(())
}

fn get_sp(caller: &mut Caller<'_, RState>) -> u32 {
    if let Some(Extern::Global(g)) = caller.get_export("__stack_pointer") {
        if let Val::I32(v) = g.get(&mut *caller) {
            return v as u32;
        }
    }
    0
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rootdir = args.get(1).cloned().unwrap_or_else(|| "userspace/rootfs".into());
    let interactive = args.iter().any(|a| a == "-i");
    let app_mode = args.iter().any(|a| a == "--app");
    let verbose = std::env::var("KDBG").is_ok();

    if app_mode {
        // M3: winit must own the MAIN thread on macOS, and the Kernel is
        // Rc-built (!Send) — so the presentation layer runs here and the
        // whole kernel world is constructed inside its own thread.
        let (ev_tx, ev_rx) = channel::<Ev>();
        let (el, mut app) = ui::run(ev_tx.clone(), &rootdir);
        let proxy = el.create_proxy();
        let rootdir2 = rootdir.clone();
        std::thread::Builder::new().name("kernel".into()).stack_size(16 << 20)
            .spawn(move || {
                kernel_world(&rootdir2, interactive, verbose, true, ev_tx, ev_rx, Some(proxy));
            })
            .expect("kernel thread");
        el.run_app(&mut app).expect("event loop");
        return;
    }

    let (ev_tx, ev_rx) = channel::<Ev>();
    kernel_world(&rootdir, interactive, verbose, false, ev_tx, ev_rx, None);
}

fn kernel_world(rootdir: &str, interactive: bool, verbose: bool, app_mode: bool,
                ev_tx: Sender<Ev>, ev_rx: Receiver<Ev>,
                ui: Option<winit::event_loop::EventLoopProxy<ui::UiMsg>>) {
    let args: Vec<String> = std::env::args().collect();

    let live = args.iter().any(|a| a == "--live");
    let seed = load_seed(std::path::Path::new(&rootdir), "/").expect("rootfs seed");
    let mut kern = Kernel::new(&seed, "kitty");
    // M4: '#Z' serves a host directory. --live points it at the rootfs dir
    // itself AND makes it the implicit root — boot from the real tree,
    // writes persist. Otherwise a per-boot temp dir backs '#Z' so the
    // suite's hostfs test has something real to exercise.
    if live {
        kern.set_hostfs(std::fs::canonicalize(&rootdir).expect("rootdir"), true);
    } else {
        let t = std::env::temp_dir().join(format!("ipnx-z-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&t);
        kern.set_hostfs(t, false);
    }
    kern.interactive = interactive;
    kern.verbose = verbose;

    let mut config = Config::new();
    config.wasm_exceptions(true);
    let engine = Arc::new(Engine::new(&config).expect("engine"));

    if app_mode {
        let tick = ev_tx.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(33));
            if tick.send(Ev::WinTick).is_err() {
                break;
            }
        });
    }

    let tty = {
        use std::io::IsTerminal;
        std::io::stdin().is_terminal()
    };
    let init_argv = if app_mode && (tty || interactive) {
        vec!["init".to_string(), "-wi".to_string()]
    } else if app_mode {
        vec!["init".to_string(), "-w".to_string()]
    } else if interactive {
        vec!["init".to_string(), "-i".to_string()]
    } else {
        vec!["init".to_string()]
    };
    kern.boot(init_argv).expect("boot");

    if interactive {
        let tx = ev_tx.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = [0u8; 4096];
            let mut stdin = std::io::stdin();
            loop {
                match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => {
                        let _ = tx.send(Ev::StdinClosed);
                        break;
                    }
                    Ok(n) => {
                        let _ = tx.send(Ev::Stdin(buf[..n].to_vec()));
                    }
                }
            }
        });
    }

    // the kernel loop: pure state machine + effects
    loop {
        run_effects(&mut kern, &engine, &ev_tx, &ui);
        let ev = match ev_rx.recv() {
            Ok(e) => e,
            Err(_) => break,
        };
        match ev {
            Ev::Sys { worker_pid, trap, a, tx, reply } => kern.syscall(worker_pid, trap, a, tx, reply),
            Ev::Started { pid, asyncified } => kern.set_asyncified(pid, asyncified),
            Ev::AsyFork { parent, child, snap, data_ptr, sp } => kern.asyfork(parent, child, snap, data_ptr, sp),
            Ev::Died { pid, msg } => kern.proc_died(pid, &msg),
            Ev::Timer { token } => kern.timer_fired(token),
            Ev::Stdin(b) => kern.cons_feed(&b),
            Ev::StdinClosed => kern.cons_end(),
            Ev::WinKey { wid, bytes } => kern.win_key(wid, &bytes),
            Ev::WinMouse { wid, x, y, b } => kern.win_mouse(wid, x, y, b),
            Ev::WinClose { wid } => kern.win_close(wid),
            Ev::WinTick => kern.win_tick(),
            Ev::WinAck { wid } => kern.win_ack(wid),
            Ev::FetchDone { url, result } => kern.fetch_done(&url, result),
        }
    }
}

fn run_effects(kern: &mut Kernel, engine: &Arc<Engine>, ev_tx: &Sender<Ev>,
               ui: &Option<winit::event_loop::EventLoopProxy<ui::UiMsg>>) {
    use std::io::Write;
    for e in kern.take_effects() {
        match e {
            Effect::ConsWrite(bytes) => {
                let mut out = std::io::stdout();
                out.write_all(&bytes).ok();
                out.flush().ok();
            }
            Effect::Spawn { pid, image, argv, asy } => {
                let engine = engine.clone();
                let ev = ev_tx.clone();
                std::thread::Builder::new()
                    .name(format!("guest-{}", pid))
                    .stack_size(8 << 20)
                    .spawn(move || run_guest(engine, ev, pid, image, argv, asy))
                    .expect("spawn guest thread");
            }
            Effect::Timer { ms, token } => {
                let ev = ev_tx.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(ms));
                    let _ = ev.send(Ev::Timer { token });
                });
            }
            Effect::Shutdown(code) => {
                if let Some(p) = ui {
                    let _ = p.send_event(ui::UiMsg::Shutdown);
                }
                std::io::stdout().flush().ok();
                std::process::exit(code);
            }
            Effect::WinUpdate { wid, label, w, h, rgba } => {
                if let Some(p) = ui {
                    let _ = p.send_event(ui::UiMsg::Update { wid, label, w, h, rgba });
                }
            }
            Effect::WinText { wid, bytes } => {
                if let Some(p) = ui {
                    let _ = p.send_event(ui::UiMsg::Text { wid, bytes });
                }
            }
            Effect::WinGone { wid } => {
                if let Some(p) = ui {
                    let _ = p.send_event(ui::UiMsg::Gone { wid });
                }
            }
            Effect::Fetch { url } => {
                let ev = ev_tx.clone();
                std::thread::spawn(move || {
                    let result = (|| -> Result<Vec<u8>, String> {
                        let resp = ureq::get(&url).timeout(std::time::Duration::from_secs(120))
                            .call().map_err(|e| format!("GET {}: {}", url, e))?;
                        let mut body = Vec::new();
                        use std::io::Read;
                        resp.into_reader().take(64 << 20).read_to_end(&mut body)
                            .map_err(|e| e.to_string())?;
                        Ok(body)
                    })();
                    let _ = ev.send(Ev::FetchDone { url, result });
                });
            }
        }
    }
}

// A content-keyed .cwasm cache: cranelift compiles acme's 20MB module in
// 60-150s cold; a cache hit deserializes in milliseconds. The key is an
// FNV-1a of the bytes plus length plus the wasmtime major, the directory is
// the platform cache dir (falling back to the temp dir; a scratch container
// with neither just compiles). Module::deserialize_file is unsafe by
// contract — the cache holds only files this same binary wrote.
fn module_cached(engine: &Engine, image: &[u8]) -> Result<Module, wasmtime::Error> {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in image {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    let dir = std::env::var_os("HOME")
        .map(|home| std::path::PathBuf::from(home).join("Library/Caches/ipnx-v12"))
        .filter(|d| std::fs::create_dir_all(d).is_ok())
        .or_else(|| {
            let t = std::env::temp_dir().join("ipnx-cwasm");
            std::fs::create_dir_all(&t).ok().map(|_| t)
        });
    let path = dir.as_ref().map(|d| d.join(format!("{:016x}-{}-wt48.cwasm", h, image.len())));
    if let Some(pth) = &path {
        if pth.exists() {
            if let Ok(m) = unsafe { Module::deserialize_file(engine, pth) } {
                return Ok(m);
            }
        }
    }
    let m = Module::new(engine, image)?;
    if let (Some(pth), Ok(bytes)) = (&path, m.serialize()) {
        let tmp = pth.with_extension("tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, pth);
        }
    }
    Ok(m)
}

fn load_seed(path: &std::path::Path, name: &str) -> std::io::Result<Seed> {
    let meta = std::fs::metadata(path)?;
    if meta.is_dir() {
        let mut kids = Vec::new();
        let mut entries: Vec<_> = std::fs::read_dir(path)?.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let n = e.file_name().to_string_lossy().into_owned();
            if let Ok(k) = load_seed(&e.path(), &n) {
                kids.push(k);
            }
        }
        Ok(Seed { name: name.into(), dir: true, kids, data: Vec::new() })
    } else {
        Ok(Seed { name: name.into(), dir: false, kids: Vec::new(), data: std::fs::read(path)? })
    }
}

fn run_guest(engine: Arc<Engine>, ev: Sender<Ev>, pid: Pid, image: Arc<Vec<u8>>,
             _argv: Vec<String>, asy: Option<AsySnap>) {
    let mut image = image;
    let mut asy = asy;
    loop {
        match run_one(&engine, &ev, pid, &image, asy.take()) {
            RunEnd::Exec(next, _argv) => {
                image = next;
            }
            RunEnd::Done => return,
            RunEnd::Crash(msg) => {
                let _ = ev.send(Ev::Died { pid, msg });
                return;
            }
        }
    }
}

enum RunEnd {
    Exec(Arc<Vec<u8>>, Vec<String>),
    Done,
    Crash(String),
}

// a wasip1 command: exports its memory, forks never, one preopen — the
// namespace root. The shim is wasi.rs; syscalls cross the same channel.
fn run_wasi(engine: &Arc<Engine>, ev: &Sender<Ev>, pid: Pid, module: &Module) -> RunEnd {
    let _ = ev.send(Ev::Started { pid, asyncified: false });
    let mut store = Store::new(engine, wasi::WasiState::new(pid, ev.clone()));
    let mut linker: Linker<wasi::WasiState> = Linker::new(engine);
    if let Err(e) = wasi::link_wasi(&mut linker) {
        return RunEnd::Crash(format!("wasi link: {}", e));
    }
    let instance = match linker.instantiate(&mut store, module) {
        Ok(i) => i,
        Err(e) => return RunEnd::Crash(format!("wasi instantiate: {}", e)),
    };
    let memory = match instance.get_memory(&mut store, "memory") {
        Some(m) => m,
        None => return RunEnd::Crash("wasi module exports no memory".into()),
    };
    store.data_mut().memory = Some(memory);
    let start = match instance.get_typed_func::<(), ()>(&mut store, "_start") {
        Ok(f) => f,
        Err(e) => return RunEnd::Crash(format!("no _start: {}", e)),
    };
    match start.call(&mut store, ()) {
        Ok(()) => {
            // fell off main: exit 0
            let (rtx, _rrx) = channel();
            let _ = ev.send(Ev::Sys {
                worker_pid: pid, trap: 8, a: [0; 5], tx: vec![0], reply: rtx,
            });
            RunEnd::Done
        }
        Err(e) => match e.downcast::<GuestExit>() {
            Ok(GuestExit::Retire) | Ok(GuestExit::Die) => RunEnd::Done,
            Ok(other) => RunEnd::Crash(format!("wasi guest: {}", other)),
            Err(e) => {
                // a runtime trap: report and exit unclean
                let (rtx, _rrx) = channel();
                let mut tx = b"trap".to_vec();
                tx.push(0);
                let _ = ev.send(Ev::Sys {
                    worker_pid: pid, trap: 8, a: [0; 5], tx, reply: rtx,
                });
                RunEnd::Crash(format!("wasi trap: {}", e))
            }
        },
    }
}

fn run_one(engine: &Arc<Engine>, ev: &Sender<Ev>, pid: Pid, image: &Arc<Vec<u8>>,
           asy: Option<AsySnap>) -> RunEnd {
    let module = match module_cached(engine, image.as_slice()) {
        Ok(m) => m,
        Err(e) => return RunEnd::Crash(format!("exec format error: {}", e)),
    };
    if module.imports().any(|i| i.module() == "wasi_snapshot_preview1") {
        return run_wasi(engine, ev, pid, &module);
    }
    let asyncified = module.exports().any(|e| e.name() == "asyncify_start_unwind");
    let _ = ev.send(Ev::Started { pid, asyncified });
    let state = RState {
        pid,
        ev: ev.clone(),
        memory: None,
        in_note: false,
        last_aux: 0,
        saved_stack: None,
        rewinding: false,
        rewind_return: 0,
        pending_fork: None,
        pending_sj: None,
        sj_resume: None,
        sjmap: HashMap::new(),
        tcmap: HashMap::new(),
        last_sjbuf: 0,
        stack_top: 0,
    };
    let mut store = Store::new(engine, state);

    // guest memory: ours, per the import's declared shape, capped like the JS
    // hosts (80 pages plain, 256 asyncified — asyncified snapshots pay per page)
    let declared_min = module
        .imports()
        .find(|i| i.module() == "env" && i.name() == "memory")
        .and_then(|i| i.ty().memory().map(|m| m.minimum()))
        .unwrap_or(32);
    let min_pages = if let Some(a) = &asy {
        declared_min.max(((a.snap.len() + 65535) / 65536) as u64)
    } else {
        declared_min
    };
    let max_pages = 256u64.max(min_pages + 32);
    let memory = match Memory::new(&mut store, MemoryType::new(min_pages as u32, Some(max_pages as u32))) {
        Ok(m) => m,
        Err(e) => return RunEnd::Crash(format!("memory: {}", e)),
    };
    store.data_mut().memory = Some(memory);

    let mut linker: Linker<RState> = Linker::new(engine);
    linker.define(&mut store, "env", "memory", memory).unwrap();

    linker.func_wrap("env", "sys",
        |mut caller: Caller<'_, RState>, trap: i32, a0: i32, a1: i32, a2: i32, a3: i32, a4: i32|
        -> Result<i32, wasmtime::Error> { sys(&mut caller, trap, a0, a1, a2, a3, a4) }).unwrap();

    linker.func_wrap("env", "forka",
        |mut caller: Caller<'_, RState>, flags: i32, databuf: i32| -> Result<i32, wasmtime::Error> {
            if caller.data().rewinding {
                // the second return
                if let Some(Extern::Func(f)) = caller.get_export("asyncify_stop_rewind") {
                    f.typed::<(), ()>(&mut caller)?.call(&mut caller, ())?;
                }
                caller.data_mut().rewinding = false;
                return Ok(caller.data().rewind_return);
            }
            let pid = sys(&mut caller, 19, flags, 0, 2, 0, 0)?;
            if pid < 0 {
                return Ok(-1);
            }
            caller.data_mut().pending_fork = Some((pid, databuf as u32));
            call1(&mut caller, "asyncify_start_unwind", databuf)?;
            Ok(0)
        }).unwrap();

    linker.func_wrap("env", "setj",
        |mut caller: Caller<'_, RState>, env: i32| -> Result<i32, wasmtime::Error> {
            if caller.get_export("asyncify_start_unwind").is_none() {
                return Ok(0); // uninstrumented binary: arm nothing
            }
            if let Some(v) = caller.data_mut().sj_resume.take() {
                if let Some(Extern::Func(f)) = caller.get_export("asyncify_stop_rewind") {
                    f.typed::<(), ()>(&mut caller)?.call(&mut caller, ())?;
                }
                caller.data_mut().rewinding = false;
                return Ok(v);
            }
            let databuf = caller.data().last_sjbuf;
            caller.data_mut().pending_sj = Some(Sj { kind: 0, env: env as u32, val: 0, databuf });
            call1(&mut caller, "asyncify_start_unwind", databuf as i32)?;
            Ok(0)
        }).unwrap();

    linker.func_wrap("env", "longj",
        |mut caller: Caller<'_, RState>, env: i32, val: i32| -> Result<i32, wasmtime::Error> {
            let databuf = caller.data().last_sjbuf;
            caller.data_mut().pending_sj = Some(Sj { kind: 1, env: env as u32, val, databuf });
            call1(&mut caller, "asyncify_start_unwind", databuf as i32)?;
            Ok(0)
        }).unwrap();

    linker.func_wrap("env", "sjbuf",
        |mut caller: Caller<'_, RState>, p: i32| {
            caller.data_mut().last_sjbuf = p as u32;
        }).unwrap();

    linker.func_wrap("env", "tsave",
        |mut caller: Caller<'_, RState>, id: i32| -> Result<i32, wasmtime::Error> {
            if let Some(v) = caller.data_mut().sj_resume.take() {
                if let Some(Extern::Func(f)) = caller.get_export("asyncify_stop_rewind") {
                    f.typed::<(), ()>(&mut caller)?.call(&mut caller, ())?;
                }
                caller.data_mut().rewinding = false;
                return Ok(v);
            }
            let databuf = caller.data().last_sjbuf;
            caller.data_mut().pending_sj = Some(Sj { kind: 2, env: id as u32, val: 0, databuf });
            call1(&mut caller, "asyncify_start_unwind", databuf as i32)?;
            Ok(0)
        }).unwrap();

    linker.func_wrap("env", "tjump",
        |mut caller: Caller<'_, RState>, id: i32, val: i32| -> Result<i32, wasmtime::Error> {
            let databuf = caller.data().last_sjbuf;
            caller.data_mut().pending_sj = Some(Sj { kind: 3, env: id as u32, val, databuf });
            call1(&mut caller, "asyncify_start_unwind", databuf as i32)?;
            Ok(0)
        }).unwrap();

    linker.func_wrap("env", "tdrop",
        |mut caller: Caller<'_, RState>, id: i32| {
            caller.data_mut().tcmap.remove(&(id as u32));
        }).unwrap();

    // the guard, native style: the nested __forkshim call's host frame IS the
    // catch_all — a GuestExit unwinds the child's wasm frames and stops here
    linker.func_wrap("guard", "rfork",
        |mut caller: Caller<'_, RState>, flags: i32, sp: i32, fn_: i32, arg: i32|
        -> Result<i32, wasmtime::Error> {
            let ret = sys(&mut caller, 19, flags, sp, 1, 0, 0)?;
            if ret == 0 && caller.data().last_aux > 0 {
                let child_pid = caller.data().last_aux;
                // child branch: save the scribble region, run the child inline
                let mem = mem_of(&mut caller);
                let saved = mem.data(&caller)[0..sp as usize].to_vec();
                caller.data_mut().saved_stack = Some(saved);
                let shim = match caller.get_export("__forkshim") {
                    Some(Extern::Func(f)) => f,
                    _ => return Err(wasmtime::Error::msg("no __forkshim export")),
                };
                let r = shim.typed::<(i32, i32), ()>(&mut caller)?.call(&mut caller, (fn_, arg));
                return match r {
                    Ok(()) => Err(wasmtime::Error::msg("forkshim returned")),
                    Err(e) => match e.downcast::<GuestExit>() {
                        Ok(GuestExit::ForkResume(pid)) => Ok(pid), // the guard's return
                        Ok(other) => Err(other.into()),
                        Err(e) => Err(e),
                    },
                };
                #[allow(unreachable_code)]
                {
                    let _ = child_pid;
                    unreachable!()
                }
            }
            Ok(ret)
        }).unwrap();

    let instance = match linker.instantiate(&mut store, &module) {
        Ok(i) => i,
        Err(e) => return RunEnd::Crash(format!("instantiate: {}", e)),
    };
    if let Some(g) = instance.get_global(&mut store, "__stack_pointer") {
        if let Val::I32(v) = g.get(&mut store) {
            store.data_mut().stack_top = v as u32;
        }
    }
    if let Some(a) = asy {
        // a freshly forked child: copy the snapshot in and rewind
        memory.write(&mut store, 0, &a.snap).ok();
        store.data_mut().rewind_return = 0;
        store.data_mut().rewinding = true;
        if let Some(g) = instance.get_global(&mut store, "__stack_pointer") {
            g.set(&mut store, Val::I32(a.sp as i32)).ok();
        }
        if let Some(f) = instance.get_typed_func::<i32, ()>(&mut store, "asyncify_start_rewind").ok() {
            f.call(&mut store, a.data_ptr as i32).ok();
        }
    }

    let start = match instance.get_typed_func::<(), ()>(&mut store, "_start") {
        Ok(f) => f,
        Err(e) => return RunEnd::Crash(format!("no _start: {}", e)),
    };

    // callStart, ported: the loop that services asyncify unwinds
    loop {
        match start.call(&mut store, ()) {
            Ok(()) => {
                if let Some(sj) = store.data_mut().pending_sj.take() {
                    stop_unwind(&instance, &mut store);
                    handle_sj(&instance, &mut store, &memory, sj);
                    continue;
                }
                if let Some((child, databuf)) = store.data_mut().pending_fork.take() {
                    stop_unwind(&instance, &mut store);
                    let sp = global_sp(&instance, &mut store);
                    let snap = memory.data(&store).to_vec();
                    let _ = ev.send(Ev::AsyFork {
                        parent: pid, child: child as Pid, snap, data_ptr: databuf, sp,
                    });
                    store.data_mut().rewind_return = child;
                    store.data_mut().rewinding = true;
                    set_global_sp(&instance, &mut store, sp); // exact, not drifted
                    if let Ok(f) = instance.get_typed_func::<i32, ()>(&mut store, "asyncify_start_rewind") {
                        f.call(&mut store, databuf as i32).ok();
                    }
                    continue;
                }
                // _start returned without exits: treat as clean exit
                let (rtx, _rrx) = channel();
                let _ = ev.send(Ev::Sys {
                    worker_pid: pid, trap: 8, a: [0; 5], tx: vec![0], reply: rtx,
                });
                return RunEnd::Done;
            }
            Err(e) => {
                return match e.downcast::<GuestExit>() {
                    Ok(GuestExit::Exec(image, argv)) => RunEnd::Exec(image, argv),
                    Ok(GuestExit::Retire) | Ok(GuestExit::Die) => RunEnd::Done,
                    Ok(GuestExit::ForkResume(_)) => RunEnd::Crash("fork resume escaped the guard".into()),
                    Err(e) => RunEnd::Crash(format!("guest trap: {}", e)),
                };
            }
        }
    }
}

fn stop_unwind(instance: &Instance, store: &mut Store<RState>) {
    if let Ok(f) = instance.get_typed_func::<(), ()>(&mut *store, "asyncify_stop_unwind") {
        f.call(&mut *store, ()).ok();
    }
}

fn global_sp(instance: &Instance, store: &mut Store<RState>) -> u32 {
    instance
        .get_global(&mut *store, "__stack_pointer")
        .and_then(|g| match g.get(&mut *store) {
            Val::I32(v) => Some(v as u32),
            _ => None,
        })
        .unwrap_or(0)
}

fn set_global_sp(instance: &Instance, store: &mut Store<RState>, sp: u32) {
    if let Some(g) = instance.get_global(&mut *store, "__stack_pointer") {
        g.set(&mut *store, Val::I32(sp as i32)).ok();
    }
}

// the setjmp/longjmp and thread-context state machine (guestcore's callStart)
fn handle_sj(instance: &Instance, store: &mut Store<RState>, memory: &Memory, sj: Sj) {
    let read_u32 = |store: &Store<RState>, addr: u32| {
        let d = memory.data(&*store);
        u32::from_le_bytes(d[addr as usize..addr as usize + 4].try_into().unwrap())
    };
    match sj.kind {
        0 => {
            // setjmp: frames live at databuf+8..end
            let end = read_u32(store, sj.databuf);
            let frames = memory.data(&*store)[(sj.databuf + 8) as usize..end as usize].to_vec();
            let sp = global_sp(instance, store);
            store.data_mut().sjmap.insert(sj.env, (frames, sp));
            store.data_mut().sj_resume = Some(0);
        }
        2 => {
            // tsave: the shadow stack region [sp, stackTop) travels too
            let end = read_u32(store, sj.databuf);
            let frames = memory.data(&*store)[(sj.databuf + 8) as usize..end as usize].to_vec();
            let sp = global_sp(instance, store);
            let top = store.data().stack_top;
            let shadow = memory.data(&*store)[sp as usize..top as usize].to_vec();
            store.data_mut().tcmap.insert(sj.env, (frames, sp, shadow));
            store.data_mut().sj_resume = Some(0);
        }
        3 => {
            // tjump
            let saved = store.data().tcmap.get(&sj.env).cloned();
            let Some((frames, sp, shadow)) = saved else {
                store.data_mut().sj_resume = Some(0);
                return;
            };
            memory.write(&mut *store, (sj.databuf + 8) as usize, &frames).ok();
            let end = sj.databuf + 8 + frames.len() as u32;
            memory.write(&mut *store, sj.databuf as usize, &end.to_le_bytes()).ok();
            memory.write(&mut *store, sp as usize, &shadow).ok();
            set_global_sp(instance, store, sp);
            store.data_mut().sj_resume = Some(sj.val);
        }
        _ => {
            // longjmp
            let saved = store.data().sjmap.get(&sj.env).cloned();
            let Some((frames, sp)) = saved else {
                store.data_mut().sj_resume = Some(sj.val);
                return;
            };
            memory.write(&mut *store, (sj.databuf + 8) as usize, &frames).ok();
            let end = sj.databuf + 8 + frames.len() as u32;
            memory.write(&mut *store, sj.databuf as usize, &end.to_le_bytes()).ok();
            set_global_sp(instance, store, sp);
            store.data_mut().sj_resume = Some(sj.val);
        }
    }
    store.data_mut().rewinding = true;
    if let Ok(f) = instance.get_typed_func::<i32, ()>(&mut *store, "asyncify_start_rewind") {
        f.call(&mut *store, sj.databuf as i32).ok();
    }
}
