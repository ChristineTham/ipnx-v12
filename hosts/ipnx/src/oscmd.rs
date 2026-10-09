//! **The host's commands** — the 9P server through which a command the host
//! starts reaches a process: its standard input, output and error, and its
//! status when it ends, each a file (docs/saranos.md, *The host's
//! resources*; Christine, 2026-10-09: *"9P over #9"*).
//!
//! It is Inferno's `cmd(3)` device (`emu/port/devcmd.c`) as a 9P server the
//! machine provides through `#9`, as it provides the root: a conversation per
//! command, its files `data`, `stderr` and `wait`, and the command started,
//! waited for and killed as Inferno's emulator does (`emu/MacOSX/cmd.c`).
//! What `devcmd`'s `clone` and `ctl` do — make a conversation, give it a
//! directory and a priority, `exec` — is the host's call ([`start`]), which
//! names the conversation; the kernel attaches this server with that name
//! and opens the files into the process's descriptors
//! ([`ipnx_kernel::Call::Oscmd`]). So a conversation has no `ctl` and no
//! `status`, and the server has no `clone`.
//!
//! **A read waits as any 9P read waits** — its reply is held
//! ([`Nineserver::submit`] answers `None`) and given when the command has
//! written ([`Nineserver::harvest`]). One host `read` per 9P read, as
//! `cmdread` makes one (`devcmd.c:396`), on a thread of its own, so nothing
//! of the command's output is taken from its pipe before it is asked for.
//!
//! **It is not part of the kernel and it is not a wasm process**, as the
//! root's server is not ([`crate::store`]).

use ipnx_kernel::devvirtio9p::Nineserver;
use ipnx_kernel::ninep::{unframe, Dir, Qid, R, T, W, DMDIR, QTDIR};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc;

/// What this server will accept, as the root's does.
const MSIZE: u32 = 8192 + 24;

/// `Enonexist`, `Eperm`, `Ehungup` and `Ebadarg` (`emu/port/error.h`).
const ENONEXIST: &str = "file does not exist";
const EPERM: &str = "permission denied";
const EHUNGUP: &str = "i/o on hungup channel";
const EBADARG: &str = "bad arg in system call";

/// `devcmd`'s files, by the qid it gives them — *"#define QID(c, y)
/// (((c)<<4) | (y))"* (`devcmd.c:23`), its `Qconvdir`, `Qdata`, `Qstderr`
/// and `Qwait` (`:12`). The conversation's directory is the root of an
/// attach.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Q {
    Dir = 3,
    Data = 4,
    Stderr = 5,
    Wait = 8,
}

impl Q {
    fn qid(self, conv: u32) -> Qid {
        let qtype = if self == Q::Dir { QTDIR } else { 0 };
        Qid { qtype, vers: 0, path: ((conv as u64) << 4) | self as u64 }
    }
}

/// `OREAD`, `OWRITE`, `ORDWR` — the low bits of an open's mode.
const OREAD: u8 = 0;
const OWRITE: u8 = 1;
const ORDWR: u8 = 2;

struct Fid {
    conv: u32,
    q: Q,
    /// The mode it was opened with — `c->mode` with `COPEN` — or nothing.
    open: Option<u8>,
}

/// A stream the command writes — `c->fd[1]` or `c->fd[2]` — and the reads
/// waiting on it.
#[derive(Default)]
struct Stream {
    /// The thread that reads it, asked for one read at a time; nothing once
    /// it is closed (*"c->fd[fd] = -1"*, `devcmd.c:300`).
    ask: Option<mpsc::Sender<usize>>,
    /// A read is out.
    asked: bool,
    /// What a read brought for a request that was flushed meanwhile, for
    /// the next.
    left: Vec<u8>,
    /// Its end: every read after it answers nothing (`read(2)` gives 0).
    eof: bool,
    /// The reads waiting, oldest first: tag and count.
    held: VecDeque<(u16, u32)>,
}

/// **A conversation** — `devcmd`'s `Conv` (`devcmd.c:27`), the command and
/// what is open of it.
#[derive(Default)]
struct Conv {
    /// `cv->owner` — who attached it.
    owner: String,
    /// `c->child`: the command, while it runs — the process group
    /// `oscmdkill` signals (`cmd.c:183`).
    child: Option<i32>,
    /// `cv->killed`.
    killed: bool,
    /// **`os -b`**: nothing kills it when what is open of it goes —
    /// *"The -b (background) option suppresses that behaviour"*
    /// (`man/1/os`).
    bg: bool,
    /// `cv->inuse` — what is open of it, and the host's call's own hold
    /// until the kernel has opened what it asked for ([`release`]).
    inuse: usize,
    /// `cv->count` — opens of its standard input for writing, and of its
    /// output and error for reading.
    count: [usize; 3],
    /// `c->fd[0]`: the thread that writes its standard input, or nothing
    /// once that is closed.
    stdin: Option<mpsc::Sender<(u16, Vec<u8>)>>,
    /// `c->fd[1]` and `c->fd[2]`.
    out: [Stream; 2],
    /// `c->waitq` — the status, from when it ends until it is read.
    status: VecDeque<Vec<u8>>,
    /// Reads of `wait` waiting for it: tag and count.
    waiting: VecDeque<(u16, u32)>,
    /// `c->cmd` — the command the host's call asked for, until the attach
    /// starts it.
    cmd: Option<Cmd>,
}

/// What the host's call asks for: `devcmd`'s `exec`, `dir` and `nice`.
struct Cmd {
    argv: Vec<Vec<u8>>,
    env: Vec<Vec<u8>>,
    dir: Option<Vec<u8>>,
    nice: bool,
}

/// What the threads that hold a command's pipes and its process say.
enum Event {
    /// A read of stream 0 (output) or 1 (error): its bytes, nothing at the
    /// end, or the host's error.
    Read(u32, usize, Result<Vec<u8>, String>),
    /// A write of its standard input, by the tag that asked — a tag names
    /// one request on the wire, whichever conversation it is for: how much,
    /// or the host's error.
    Wrote(u16, Result<usize, String>),
    /// It ended, and this is its wait record (`oscmdwait`).
    Exit(u32, Vec<u8>),
}

struct State {
    /// **Where a command runs without `-d`**: the host directory the root is
    /// served from, as Inferno's runs in the emulator's root —
    /// *"kstrdup(&c->dir, rootdir)"* (`devcmd.c:291`).
    dir: PathBuf,
    convs: HashMap<u32, Conv>,
    fids: HashMap<u32, Fid>,
    /// The writes of standard input out, by tag. One flushed is taken out,
    /// and its answer, when it comes, is not given.
    writes: HashSet<u16>,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    /// Replies made and not yet harvested.
    done: Vec<Vec<u8>>,
}

/// **The server** — what the machine provides as `#9/<n>`.
pub struct Cmds(Rc<RefCell<State>>);

thread_local! {
    /// **The host's commands, for the host's call** — which `#9` server
    /// they are and the server itself, for the machine's `sys.oscmd` on this
    /// thread: the kernel's, the only one that runs processes, as
    /// `machine.rs`'s `KERNEL` is.
    static SERVED: RefCell<Option<(u32, Rc<RefCell<State>>)>> = const { RefCell::new(None) };
}

impl Cmds {
    /// The server, for `#9/<index>` on this thread, its commands run without
    /// `-d` in `dir` — made absolute, as the emulator's `rootdir` is.
    pub fn new(index: u32, dir: &Path) -> Cmds {
        let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        let (tx, rx) = mpsc::channel();
        let state = Rc::new(RefCell::new(State {
            dir,
            convs: HashMap::new(),
            fids: HashMap::new(),
            writes: HashSet::new(),
            tx,
            rx,
            done: Vec::new(),
        }));
        SERVED.with(|s| *s.borrow_mut() = Some((index, state.clone())));
        Cmds(state)
    }
}

/// **The host's half of the host's call**: a conversation for `argv`, as
/// `cmdclone` makes one and `ctl` gives it its `dir`, `nice` and `exec`
/// (`devcmd.c:533`, `:440`) — and which `#9` server serves it and the
/// conversation's name, for the kernel to attach; or nothing, when this host
/// serves no commands.
///
/// **The command starts at the attach** ([`State::attach`]), so the kernel's
/// own checks — the call's addresses, `RFNOMNT`'s *"if(up->pgrp->noattach)
/// error(Enoattach)"* (`sysfile.c:1011`) — are made before anything runs on
/// the host: a call the kernel refuses starts nothing.
pub fn start(argv: Vec<Vec<u8>>, env: Vec<Vec<u8>>, dir: Option<Vec<u8>>, nice: bool, bg: bool) -> Option<(u32, String)> {
    SERVED.with(|s| {
        let s = s.borrow();
        let (index, state) = s.as_ref()?;
        let n = state.borrow_mut().start(argv, env, dir, nice, bg);
        Some((*index, n.to_string()))
    })
}

/// **The host's call is done with the conversation** — what it held until
/// the kernel had opened what it asked for, let go as a close lets go
/// (`cmdclose`, `devcmd.c:310`): a command nobody has open is killed, and
/// one that has ended is forgotten.
pub fn release(name: &str) {
    SERVED.with(|s| {
        if let (Some((_, state)), Ok(n)) = (s.borrow().as_ref(), name.parse::<u32>()) {
            state.borrow_mut().letgo(n, false);
        }
    });
}

/// `strerror(errno)`, as Inferno's emulator reports a host failure — the
/// text alone, without Rust's *" (os error N)"*.
fn strerror(e: &std::io::Error) -> String {
    let s = e.to_string();
    match s.find(" (os error ") {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

/// `Rerror` — a server refuses by answering.
fn err(msg: &str, tag: u16) -> Vec<u8> {
    W::new().s(msg).frame(T::Error as u8, tag)
}

impl State {
    /// `cmdclone`: the lowest free conversation (`devcmd.c:533`), holding
    /// the command until the attach starts it.
    fn start(&mut self, argv: Vec<Vec<u8>>, env: Vec<Vec<u8>>, dir: Option<Vec<u8>>, nice: bool, bg: bool) -> u32 {
        let n = (0..).find(|n| !self.convs.contains_key(n)).expect("a free conversation");
        let cmd = Some(Cmd { argv, env, dir, nice });
        self.convs.insert(n, Conv { bg, inuse: 1, cmd, ..Conv::default() });
        n
    }

    /// **The attach starts the command** — `ctl`'s `exec`, `cmdproc`'s
    /// `oscmd` (`devcmd.c:572`) — and a start that fails is the attach's
    /// error, as `devcmd` answers the `ctl` write that asked for it with it:
    /// *"if(c->error) error(c->error)"* (`:466`). So the kernel's call fails
    /// with Inferno's words, and the process's `errstr` says why; the
    /// conversation is forgotten.
    fn attach(&mut self, n: u32) -> Result<(), String> {
        let Some(cmd) = self.convs.get_mut(&n).and_then(|c| c.cmd.take()) else { return Ok(()) };
        match self.spawn(n, cmd.argv, cmd.env, cmd.dir, cmd.nice) {
            Ok((pid, stdin, [o, e])) => {
                let c = self.convs.get_mut(&n).expect("the conversation");
                c.child = Some(pid);
                c.stdin = Some(stdin);
                c.out[0].ask = Some(o);
                c.out[1].ask = Some(e);
                Ok(())
            }
            Err(e) => {
                self.convs.remove(&n);
                Err(e)
            }
        }
    }

    /// **`oscmd` and `childproc`** (`cmd.c:88`, `:38`): pipes for its
    /// standard input, output and error; a process group of its own
    /// (*"setpgid(0, getpid())"*, `:124`); a lower priority for `nice`
    /// (`oslopri`, `emu/MacOSX/os.c:559`: *"setpriority(PRIO_PROCESS, 0,
    /// getpriority(PRIO_PROCESS,0)+4)"*); its directory; `SIGPIPE` as the
    /// host has it by default (`:77`); and `execvp` (`:79`), so a bare name
    /// is found on the host's `PATH` — the overlaid one, when the process's
    /// environment has a `PATH` — and anything else is a path on the host,
    /// from the directory it runs in.
    ///
    /// Two of `childproc`'s acts are not here. It closes every descriptor
    /// but the three by hand (`:45`); every descriptor this host opens is
    /// close-on-exec, as Rust's library opens them, so `exec` closes them.
    /// And it sets the user and group to the Inferno user's, or the host's
    /// `nobody` (`:58`–`:70`), which takes effect only when the emulator
    /// runs as root; this host runs a command as itself.
    #[allow(clippy::type_complexity)]
    fn spawn(
        &mut self,
        n: u32,
        argv: Vec<Vec<u8>>,
        env: Vec<Vec<u8>>,
        dir: Option<Vec<u8>>,
        nice: bool,
    ) -> Result<(i32, mpsc::Sender<(u16, Vec<u8>)>, [mpsc::Sender<usize>; 2]), String> {
        let Some((name, args)) = argv.split_first() else { return Err(EBADARG.into()) };
        let dir = match dir {
            Some(d) => PathBuf::from(OsString::from_vec(d)),
            None => self.dir.clone(),
        };
        // *"if(t->dir != nil && chdir(t->dir) < 0){ fprint(t->wfd, "can't
        // chdir to %s: %s", t->dir, strerror(errno)); _exit(1); }"* (`:72`)
        // — asked before the fork, so that the one error can be told from
        // the other; `chdir` fails as `access(dir, X_OK)` does on a
        // directory, and on anything else as `ENOTDIR`.
        let chdir = std::fs::metadata(&dir).and_then(|md| {
            if !md.is_dir() {
                return Err(std::io::Error::from_raw_os_error(libc::ENOTDIR));
            }
            let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?;
            // SAFETY: a NUL-terminated string that lives across the call.
            if unsafe { libc::access(c.as_ptr(), libc::X_OK) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
        if let Err(e) = chdir {
            return Err(format!("can't chdir to {}: {}", dir.display(), strerror(&e)));
        }
        let mut cmd = Command::new(OsString::from_vec(name.clone()));
        cmd.args(args.iter().map(|a| OsString::from_vec(a.clone())))
            .current_dir(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        // **The process's environment laid over the host's** (Christine,
        // 2026-10-08: *"yes (overlay)"*), each `name=value` as APE makes one
        // (`_envsetup.c:15`). A name a Unix environment cannot hold — empty,
        // or with a NUL in it — is not passed.
        for e in env {
            let Some(eq) = e.iter().position(|&b| b == b'=') else { continue };
            let (k, v) = (&e[..eq], &e[eq + 1..]);
            if k.is_empty() || k.contains(&0) || v.contains(&0) {
                continue;
            }
            cmd.env(OsString::from_vec(k.to_vec()), OsString::from_vec(v.to_vec()));
        }
        // SAFETY: only async-signal-safe calls — `signal`, `getpriority`,
        // `setpriority` — between the fork and the `exec`.
        unsafe {
            cmd.pre_exec(move || {
                libc::signal(libc::SIGPIPE, libc::SIG_DFL);
                if nice {
                    let p = libc::getpriority(libc::PRIO_PROCESS, 0);
                    libc::setpriority(libc::PRIO_PROCESS, 0, p + 4);
                }
                Ok(())
            });
        }
        // *"fprint(t->wfd, "exec failed: %s", strerror(errno))"* (`:82`)
        let mut child = cmd.spawn().map_err(|e| format!("exec failed: {}", strerror(&e)))?;
        let pid = child.id() as i32;
        let stdin = child.stdin.take().expect("a piped standard input");
        let stdout = child.stdout.take().expect("a piped standard output");
        let stderr = child.stderr.take().expect("a piped standard error");

        // Its standard input: one host `write` per 9P write, as `cmdwrite`
        // makes (`devcmd.c:498`). The sender gone is `close(c->fd[0])`, the
        // command's end of file, once what was asked before it is written.
        let (wtx, wrx) = mpsc::channel::<(u16, Vec<u8>)>();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let mut stdin = stdin;
            for (tag, b) in wrx {
                let r = stdin.write(&b).map_err(|e| strerror(&e));
                if tx.send(Event::Wrote(tag, r)).is_err() {
                    break;
                }
            }
        });
        // Its output and error: one host `read` per 9P read (`devcmd.c:396`).
        let mut asks = Vec::new();
        for (i, mut pipe) in [Box::new(stdout) as Box<dyn Read + Send>, Box::new(stderr)].into_iter().enumerate() {
            let (atx, arx) = mpsc::channel::<usize>();
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                for want in arx {
                    let mut b = vec![0u8; want];
                    let r = pipe.read(&mut b).map(|k| {
                        b.truncate(k);
                        b
                    });
                    let end = matches!(&r, Ok(b) if b.is_empty());
                    if tx.send(Event::Read(n, i, r.map_err(|e| strerror(&e)))).is_err() || end {
                        break;
                    }
                }
            });
            asks.push(atx);
        }
        // **`oscmdwait`** (`cmd.c:186`): the wait record, Inferno's
        // `pid user sys real status`, with the status as the emulator words
        // it.
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let rec = match child.wait() {
                Ok(s) => match (s.code(), s.signal()) {
                    (Some(0), _) => format!("{pid} 0 0 0 ''"),
                    (Some(c), _) => format!("{pid} 0 0 0 'exit: {c}'"),
                    (None, Some(sig)) if sig == libc::SIGTERM || sig == libc::SIGKILL => format!("{pid} 0 0 0 killed"),
                    (None, Some(sig)) => format!("{pid} 0 0 0 'signal: {sig}'"),
                    (None, None) => format!("{pid} 0 0 0 'odd status: {:#x}'", s.into_raw()),
                },
                // *"n = snprint(status, sizeof(status), "0 0 0 0 %q",
                // up->genbuf)"* (`devcmd.c:589`)
                Err(e) => format!("0 0 0 0 '{}'", strerror(&e).replace('\'', "''")),
            };
            let _ = tx.send(Event::Exit(n, rec.into_bytes()));
        });
        let [o, e]: [mpsc::Sender<usize>; 2] = asks.try_into().expect("two streams");
        Ok((pid, wtx, [o, e]))
    }

    /// `oscmdkill` (`cmd.c:177`): *"kill(-t->pid, SIGTERM)"* — the whole
    /// process group.
    fn kill(pid: i32) {
        // SAFETY: a signal to a process group this server made.
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }
    }

    /// One fewer open of conversation `n` — the end of `cmdclose`
    /// (`devcmd.c:333`): *"r = --cc->inuse; if(cc->child != nil){ if(!
    /// cc->killed) if(r == 0 || (cc->killonclose && TYPE(c->qid) == Qctl)){
    /// oscmdkill(cc->child); cc->killed = 1; } }else if(r == 0)
    /// closeconv(cc);"*.
    ///
    /// **`killonclose` is `wait`'s here.** Inferno's `os` asks for it in the
    /// foreground (`os.b:85`) on `ctl`, the file it holds for the command's
    /// whole life, so that `os` going kills the command even while the
    /// processes copying its input and error live on. There is no `ctl`;
    /// `os` holds `wait` so instead, and its copiers do not. And `-b` keeps
    /// the command alive however its files go — *"The -b (background) option
    /// suppresses that behaviour"* (`man/1/os`) — where `devcmd` still kills
    /// at the last close.
    fn letgo(&mut self, n: u32, wait: bool) {
        let Some(c) = self.convs.get_mut(&n) else { return };
        c.inuse = c.inuse.saturating_sub(1);
        let r = c.inuse;
        match c.child {
            Some(pid) => {
                if !c.killed && !c.bg && (r == 0 || wait) {
                    State::kill(pid);
                    c.killed = true;
                }
            }
            None if r == 0 => {
                // `closeconv`: what is left of it goes, and its threads with
                // it.
                self.convs.remove(&n);
            }
            None => {}
        }
    }

    /// `cmdfdclose` (`devcmd.c:296`): *"if(--c->count[fd] == 0 && c->fd[fd]
    /// != -1){ close(c->fd[fd]); c->fd[fd] = -1; }"*.
    fn fdclose(c: &mut Conv, fd: usize) {
        c.count[fd] = c.count[fd].saturating_sub(1);
        if c.count[fd] == 0 {
            match fd {
                0 => c.stdin = None,
                _ => c.out[fd - 1].ask = None,
            }
        }
    }

    /// What the command's threads have said, made into replies.
    fn take(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Read(n, i, r) => {
                    let Some(c) = self.convs.get_mut(&n) else { continue };
                    let s = &mut c.out[i];
                    s.asked = false;
                    match r {
                        Ok(b) if b.is_empty() => s.eof = true,
                        Ok(b) => s.left.extend_from_slice(&b),
                        // *"if(n < 0) oserror()"* — the reader that asked
                        // has the error.
                        Err(e) => {
                            if let Some((tag, _)) = s.held.pop_front() {
                                self.done.push(err(&e, tag));
                            }
                        }
                    }
                    let mut out = Vec::new();
                    State::serve(s, &mut out);
                    self.done.extend(out);
                }
                Event::Wrote(tag, r) => {
                    if self.writes.remove(&tag) {
                        self.done.push(match r {
                            // *"if(r == 0) error(Ehungup)"* (`devcmd.c:504`)
                            Ok(0) => err(EHUNGUP, tag),
                            Ok(k) => W::new().u32(k as u32).frame(T::Write.reply(), tag),
                            Err(e) => err(&e, tag),
                        });
                    }
                }
                // `cmdproc`'s end (`devcmd.c:592`): *"c->child = nil; …
                // if(c->inuse > 0){ c->state = "Done"; if(c->waitq != nil)
                // qproduce(c->waitq, status, n); }else closeconv(c);"*
                Event::Exit(n, rec) => {
                    let Some(c) = self.convs.get_mut(&n) else { continue };
                    c.child = None;
                    if c.inuse == 0 {
                        self.convs.remove(&n);
                        continue;
                    }
                    c.status.push_back(rec);
                    while let Some(&(tag, count)) = c.waiting.front() {
                        let Some(rec) = c.status.pop_front() else { break };
                        c.waiting.pop_front();
                        self.done.push(waitreply(&rec, count, tag));
                    }
                }
            }
        }
    }

    /// The reads of a stream that can be answered now: from what is in
    /// hand, or with nothing at its end; and if one is left waiting, a read
    /// asked of the host for it.
    fn serve(s: &mut Stream, out: &mut Vec<Vec<u8>>) {
        while let Some(&(tag, count)) = s.held.front() {
            if !s.left.is_empty() {
                let k = (count as usize).min(s.left.len());
                let b: Vec<u8> = s.left.drain(..k).collect();
                out.push(W::new().u32(b.len() as u32).raw(&b).frame(T::Read.reply(), tag));
            } else if s.eof || s.ask.is_none() {
                // *"if(c->fd[fd] == -1){ qunlock(&c->l); return 0; }"*
                out.push(W::new().u32(0).frame(T::Read.reply(), tag));
            } else {
                if !s.asked {
                    let want = (count.min(MSIZE - 24) as usize).max(1);
                    if s.ask.as_ref().is_some_and(|a| a.send(want).is_ok()) {
                        s.asked = true;
                    } else {
                        // the reader is gone: its pipe has ended
                        s.eof = true;
                        continue;
                    }
                }
                return;
            }
            s.held.pop_front();
        }
    }

    /// A conversation's directory entry, or one of its files' —
    /// `cmd3gen` (`devcmd.c:58`): `data` the owner's and `0660`, `stderr`
    /// and `wait` `0444`.
    fn dirof(&self, conv: u32, q: Q) -> Dir {
        let owner = self.convs.get(&conv).map(|c| c.owner.clone()).unwrap_or_default();
        let (name, mode) = match q {
            Q::Dir => (conv.to_string(), DMDIR | 0o555),
            Q::Data => ("data".to_string(), 0o660),
            Q::Stderr => ("stderr".to_string(), 0o444),
            Q::Wait => ("wait".to_string(), 0o444),
        };
        Dir {
            dtype: 'C' as u16,
            dev: 0,
            qid: q.qid(conv),
            mode,
            atime: 0,
            mtime: 0,
            length: 0,
            name,
            uid: owner.clone(),
            gid: owner.clone(),
            muid: owner,
        }
    }

    /// Flush a held request: whichever queue holds it lets it go, and a
    /// write's answer, when it comes, is not given (flush(5)).
    fn flush(&mut self, old: u16) {
        self.writes.remove(&old);
        for c in self.convs.values_mut() {
            c.waiting.retain(|&(t, _)| t != old);
            for s in &mut c.out {
                s.held.retain(|&(t, _)| t != old);
            }
        }
    }

    fn submit(&mut self, t: &[u8]) -> Result<Option<Vec<u8>>, String> {
        let m = unframe(t).ok_or("malformed 9P message")?;
        let mut r = R::new(m.body);
        let tag = m.tag;
        let reply = match m.ty {
            x if x == T::Version as u8 => {
                let msize = r.u32().unwrap_or(MSIZE);
                let v = r.s().unwrap_or("");
                let v = if v.starts_with("9P2000") { "9P2000" } else { "unknown" };
                W::new().u32(msize.min(MSIZE)).s(v).frame(T::Version.reply(), tag)
            }
            // No authentication, as the root's server asks none.
            x if x == T::Auth as u8 => err("authentication not required", tag),

            // The attach names the conversation, starts its command, and is
            // answered with its directory — or with why the command did not
            // start.
            x if x == T::Attach as u8 => {
                let (Some(fid), Some(_afid), Some(uname), Some(aname)) = (r.u32(), r.u32(), r.s(), r.s()) else {
                    return Ok(Some(err("short Tattach", tag)));
                };
                let Some(n) = aname.parse::<u32>().ok().filter(|n| self.convs.contains_key(n)) else {
                    return Ok(Some(err(ENONEXIST, tag)));
                };
                if let Err(e) = self.attach(n) {
                    return Ok(Some(err(&e, tag)));
                }
                let c = self.convs.get_mut(&n).expect("the conversation");
                if c.owner.is_empty() {
                    c.owner = uname.to_string();
                }
                self.fids.insert(fid, Fid { conv: n, q: Q::Dir, open: None });
                W::new().raw(&Q::Dir.qid(n).write(W::new()).into_body()).frame(T::Attach.reply(), tag)
            }

            x if x == T::Walk as u8 => {
                let (Some(from), Some(newfid), Some(nw)) = (r.u32(), r.u32(), r.u16()) else {
                    return Ok(Some(err("short Twalk", tag)));
                };
                let Some((conv, mut q)) = self.fids.get(&from).map(|f| (f.conv, f.q)) else {
                    return Ok(Some(err("unknown fid", tag)));
                };
                let mut qids = Vec::new();
                for _ in 0..nw {
                    let Some(name) = r.s() else { return Ok(Some(err("short Twalk", tag))) };
                    let next = match (q, name) {
                        (Q::Dir, "data") => Some(Q::Data),
                        (Q::Dir, "stderr") => Some(Q::Stderr),
                        (Q::Dir, "wait") => Some(Q::Wait),
                        (Q::Dir, "..") | (Q::Dir, ".") => Some(Q::Dir),
                        _ => None,
                    };
                    let Some(next) = next else { break };
                    q = next;
                    qids.push(q.qid(conv));
                }
                if qids.is_empty() && nw > 0 {
                    return Ok(Some(err(ENONEXIST, tag)));
                }
                if qids.len() == nw as usize {
                    self.fids.insert(newfid, Fid { conv, q, open: None });
                }
                let mut w = W::new().u16(qids.len() as u16);
                for qid in &qids {
                    w = w.raw(&qid.write(W::new()).into_body());
                }
                w.frame(T::Walk.reply(), tag)
            }

            // `cmdopen` (`devcmd.c:205`): the directory reads only, `stderr`
            // too, and each open counted.
            x if x == T::Open as u8 => {
                let (Some(fid), Some(mode)) = (r.u32(), r.u8()) else {
                    return Ok(Some(err("short Topen", tag)));
                };
                let Some((conv, q)) = self.fids.get(&fid).map(|f| (f.conv, f.q)) else {
                    return Ok(Some(err("unknown fid", tag)));
                };
                let omode = mode & 3;
                let Some(c) = self.convs.get_mut(&conv) else { return Ok(Some(err(ENONEXIST, tag))) };
                match q {
                    Q::Dir if omode != OREAD => return Ok(Some(err(EPERM, tag))),
                    Q::Stderr if omode != OREAD => return Ok(Some(err(EPERM, tag))),
                    Q::Data => {
                        if omode == OWRITE || omode == ORDWR {
                            c.count[0] += 1;
                        }
                        if omode == OREAD || omode == ORDWR {
                            c.count[1] += 1;
                        }
                    }
                    Q::Stderr => c.count[2] += 1,
                    _ => {}
                }
                if q != Q::Dir {
                    c.inuse += 1;
                }
                if let Some(f) = self.fids.get_mut(&fid) {
                    f.open = Some(omode);
                }
                W::new().raw(&q.qid(conv).write(W::new()).into_body()).u32(MSIZE - 24).frame(T::Open.reply(), tag)
            }

            x if x == T::Create as u8 => err(EPERM, tag),
            x if x == T::Remove as u8 => {
                if let Some(f) = r.u32().and_then(|fid| self.fids.remove(&fid)) {
                    self.clunked(f);
                }
                err(EPERM, tag)
            }
            x if x == T::Wstat as u8 => err(EPERM, tag),

            x if x == T::Read as u8 => {
                let (Some(fid), Some(off), Some(count)) = (r.u32(), r.u64(), r.u32()) else {
                    return Ok(Some(err("short Tread", tag)));
                };
                let Some((conv, q, open)) = self.fids.get(&fid).map(|f| (f.conv, f.q, f.open)) else {
                    return Ok(Some(err("unknown fid", tag)));
                };
                if open.is_none() {
                    return Ok(Some(err("fid not open", tag)));
                }
                match q {
                    Q::Dir => {
                        let mut all = Vec::new();
                        for f in [Q::Data, Q::Stderr, Q::Wait] {
                            all.extend_from_slice(&self.dirof(conv, f).conv_d2m());
                        }
                        let at = (off as usize).min(all.len());
                        let end = (at + count as usize).min(all.len());
                        W::new().u32((end - at) as u32).raw(&all[at..end]).frame(T::Read.reply(), tag)
                    }
                    Q::Data | Q::Stderr => {
                        if open == Some(OWRITE) {
                            return Ok(Some(err(EPERM, tag)));
                        }
                        let Some(c) = self.convs.get_mut(&conv) else { return Ok(Some(err(ENONEXIST, tag))) };
                        let s = &mut c.out[if q == Q::Data { 0 } else { 1 }];
                        s.held.push_back((tag, count));
                        let mut out = Vec::new();
                        State::serve(s, &mut out);
                        // held, or answered — and if answered, it is the
                        // last of `out`, since every earlier one was too
                        let mine = out.iter().position(|b| b.get(5..7) == Some(&tag.to_le_bytes()[..]));
                        let reply = mine.map(|i| out.remove(i));
                        self.done.extend(out);
                        return Ok(reply);
                    }
                    // `qread(c->waitq, a, n)` (`devcmd.c:411`): the status
                    // when it ends, one message a read.
                    Q::Wait => {
                        let Some(c) = self.convs.get_mut(&conv) else { return Ok(Some(err(ENONEXIST, tag))) };
                        match c.status.pop_front() {
                            Some(rec) if c.waiting.is_empty() => waitreply(&rec, count, tag),
                            rec => {
                                if let Some(rec) = rec {
                                    c.status.push_front(rec);
                                }
                                c.waiting.push_back((tag, count));
                                return Ok(None);
                            }
                        }
                    }
                }
            }

            // `cmdwrite` (`devcmd.c:490`): `data` only, to the command's
            // standard input — *"if(c->fd[0] == -1){ …
            // error(Ehungup); }"*.
            x if x == T::Write as u8 => {
                let (Some(fid), Some(_off), Some(count)) = (r.u32(), r.u64(), r.u32()) else {
                    return Ok(Some(err("short Twrite", tag)));
                };
                let data = r.rest()[..(count as usize).min(r.rest().len())].to_vec();
                let Some((conv, q, open)) = self.fids.get(&fid).map(|f| (f.conv, f.q, f.open)) else {
                    return Ok(Some(err("unknown fid", tag)));
                };
                if q != Q::Data || !matches!(open, Some(OWRITE) | Some(ORDWR)) {
                    return Ok(Some(err(EPERM, tag)));
                }
                let Some(c) = self.convs.get_mut(&conv) else { return Ok(Some(err(ENONEXIST, tag))) };
                let Some(w) = c.stdin.as_ref() else { return Ok(Some(err(EHUNGUP, tag))) };
                if w.send((tag, data)).is_err() {
                    return Ok(Some(err(EHUNGUP, tag)));
                }
                self.writes.insert(tag);
                return Ok(None);
            }

            x if x == T::Stat as u8 => {
                let Some(fid) = r.u32() else { return Ok(Some(err("short Tstat", tag))) };
                let Some((conv, q)) = self.fids.get(&fid).map(|f| (f.conv, f.q)) else {
                    return Ok(Some(err("unknown fid", tag)));
                };
                let b = self.dirof(conv, q).conv_d2m();
                W::new().u16(b.len() as u16).raw(&b).frame(T::Stat.reply(), tag)
            }

            x if x == T::Flush as u8 => {
                if let Some(old) = r.u16() {
                    self.flush(old);
                }
                W::new().frame(T::Flush.reply(), tag)
            }

            x if x == T::Clunk as u8 => {
                if let Some(f) = r.u32().and_then(|fid| self.fids.remove(&fid)) {
                    self.clunked(f);
                }
                W::new().frame(T::Clunk.reply(), tag)
            }

            _ => err("not implemented", tag),
        };
        Ok(Some(reply))
    }

    /// `cmdclose` (`devcmd.c:310`), for a fid that was opened: its streams'
    /// counts, and then the conversation's.
    fn clunked(&mut self, f: Fid) {
        let Some(mode) = f.open else { return };
        if f.q == Q::Dir {
            return;
        }
        if let Some(c) = self.convs.get_mut(&f.conv) {
            match f.q {
                Q::Data => {
                    if mode == OWRITE || mode == ORDWR {
                        State::fdclose(c, 0);
                    }
                    if mode == OREAD || mode == ORDWR {
                        State::fdclose(c, 1);
                    }
                }
                Q::Stderr => State::fdclose(c, 2),
                _ => {}
            }
        }
        self.letgo(f.conv, f.q == Q::Wait);
    }
}

/// A read of `wait`: the record, as much as was asked for — a queue of
/// messages gives one a read, and what does not fit is not kept (`qread`
/// on a `Qmsg` queue).
fn waitreply(rec: &[u8], count: u32, tag: u16) -> Vec<u8> {
    let k = rec.len().min(count as usize);
    W::new().u32(k as u32).raw(&rec[..k]).frame(T::Read.reply(), tag)
}

impl Nineserver for Cmds {
    /// Every request goes through [`Nineserver::submit`]; a reply that must
    /// wait cannot be given here.
    fn rpc(&mut self, t: &[u8]) -> Result<Vec<u8>, String> {
        self.submit(t)?.ok_or_else(|| "this server answers a read when the command has written".to_string())
    }

    fn submit(&mut self, t: &[u8]) -> Result<Option<Vec<u8>>, String> {
        let mut s = self.0.borrow_mut();
        s.take();
        s.submit(t)
    }

    fn harvest(&mut self) -> Vec<Vec<u8>> {
        let mut s = self.0.borrow_mut();
        s.take();
        std::mem::take(&mut s.done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// A client of the server, as the kernel's mount driver is one: each
    /// request submitted, and a held reply waited for at the clock.
    struct Client {
        s: Cmds,
        tag: u16,
        ready: Vec<Vec<u8>>,
    }

    impl Client {
        fn new(dir: &Path) -> Client {
            let mut c = Client { s: Cmds::new(0, dir), tag: 0, ready: Vec::new() };
            c.rpc(T::Version, W::new().u32(MSIZE).s("9P2000"));
            c
        }

        fn send(&mut self, ty: T, w: W) -> u16 {
            self.tag += 1;
            let tag = self.tag;
            if let Some(r) = self.s.submit(&w.frame(ty as u8, tag)).unwrap() {
                self.ready.push(r);
            }
            tag
        }

        fn wait(&mut self, tag: u16) -> Vec<u8> {
            let t = Instant::now();
            loop {
                if let Some(i) = self.ready.iter().position(|r| r[5..7] == tag.to_le_bytes()) {
                    return self.ready.remove(i);
                }
                assert!(t.elapsed() < Duration::from_secs(20), "no reply for tag {tag}");
                std::thread::sleep(Duration::from_millis(5));
                let got = self.s.harvest();
                self.ready.extend(got);
            }
        }

        fn rpc(&mut self, ty: T, w: W) -> Vec<u8> {
            let tag = self.send(ty, w);
            self.wait(tag)
        }

        /// Attach conversation `n` as fid 0 and open `files` as fids 1, 2, …
        fn open(&mut self, n: &str, files: &[(&str, u8)]) -> Result<(), String> {
            let r = self.rpc(T::Attach, W::new().u32(0).u32(!0).s("kitty").s(n));
            if r[4] == T::Error as u8 {
                return Err(String::from_utf8_lossy(&r[9..]).into_owned());
            }
            for (i, (name, mode)) in files.iter().enumerate() {
                let fid = i as u32 + 1;
                let r = self.rpc(T::Walk, W::new().u32(0).u32(fid).u16(1).s(name));
                assert_ne!(r[4], T::Error as u8, "walk {name}");
                let r = self.rpc(T::Open, W::new().u32(fid).u8(*mode));
                assert_ne!(r[4], T::Error as u8, "open {name}");
            }
            Ok(())
        }

        /// Everything fid `fid` reads until its end.
        fn readall(&mut self, fid: u32) -> Vec<u8> {
            let mut all = Vec::new();
            loop {
                let r = self.rpc(T::Read, W::new().u32(fid).u64(0).u32(8192));
                assert_eq!(r[4], T::Read.reply(), "{}", String::from_utf8_lossy(&r[9..]));
                let n = u32::from_le_bytes([r[7], r[8], r[9], r[10]]) as usize;
                if n == 0 {
                    return all;
                }
                all.extend_from_slice(&r[11..11 + n]);
            }
        }
    }

    fn argv(a: &[&str]) -> Vec<Vec<u8>> {
        a.iter().map(|s| s.as_bytes().to_vec()).collect()
    }

    fn dir() -> PathBuf {
        std::env::temp_dir()
    }

    /// A command's output, read as `data`; its status, as `wait` gives it,
    /// in Inferno's form (`oscmdwait`, `cmd.c:198`).
    #[test]
    fn a_commands_output_and_status_are_files() {
        let mut c = Client::new(&dir());
        let (_, n) = start(argv(&["sh", "-c", "echo hello; echo oops >&2; exit 3"]), vec![], None, false, false).unwrap();
        c.open(&n, &[("data", OREAD), ("stderr", OREAD), ("wait", OREAD)]).unwrap();
        release(&n);
        assert_eq!(c.readall(1), b"hello\n");
        assert_eq!(c.readall(2), b"oops\n");
        let r = c.rpc(T::Read, W::new().u32(3).u64(0).u32(1024));
        let rec = String::from_utf8_lossy(&r[11..]).into_owned();
        assert!(rec.ends_with(" 0 0 0 'exit: 3'"), "{rec:?}");
    }

    /// What is written to `data` is the command's standard input, and its
    /// end is the last close of `data` for writing (`cmdfdclose`).
    #[test]
    fn data_written_is_its_input_and_the_last_close_its_end() {
        let mut c = Client::new(&dir());
        let (_, n) = start(argv(&["sort"]), vec![], None, false, false).unwrap();
        c.open(&n, &[("data", OWRITE), ("data", OREAD), ("wait", OREAD)]).unwrap();
        release(&n);
        let r = c.rpc(T::Write, W::new().u32(1).u64(0).u32(7).raw(b"b\na\nc\n\n"));
        assert_eq!(r[4], T::Write.reply());
        c.rpc(T::Clunk, W::new().u32(1));
        assert_eq!(c.readall(2), b"\na\nb\nc\n");
        let r = c.rpc(T::Read, W::new().u32(3).u64(0).u32(1024));
        assert!(String::from_utf8_lossy(&r[11..]).ends_with(" 0 0 0 ''"));
    }

    /// The environment laid over the host's, and the directory: the root's
    /// without `-d`; a bad one is the attach's error, in Inferno's words.
    #[test]
    fn its_environment_and_directory() {
        let d = std::env::temp_dir().join(format!("ipnx-oscmd-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let mut c = Client::new(&d);
        let env = vec![b"greeting=hi\x01there".to_vec(), b"=skipped".to_vec()];
        let (_, n) = start(argv(&["sh", "-c", "echo $greeting; pwd"]), env, None, false, false).unwrap();
        c.open(&n, &[("data", OREAD)]).unwrap();
        release(&n);
        let out = c.readall(1);
        let want = format!("hi\x01there\n{}\n", std::fs::canonicalize(&d).unwrap().display());
        assert_eq!(String::from_utf8_lossy(&out), want);
        let (_, n) = start(argv(&["true"]), vec![], Some(b"/nonexistent/dir".to_vec()), false, false).unwrap();
        assert_eq!(c.open(&n, &[]), Err("can't chdir to /nonexistent/dir: No such file or directory".into()));
        let (_, n) = start(argv(&["/nonexistent/program"]), vec![], None, false, false).unwrap();
        assert_eq!(c.open(&n, &[]), Err("exec failed: No such file or directory".into()));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **The last close kills it** — the whole process group, with
    /// `SIGTERM` (`oscmdkill`, `cmd.c:183`) — unless it was started for
    /// `-b`; and its status says so.
    #[test]
    fn the_last_close_kills_it_unless_it_is_in_the_background() {
        let mut c = Client::new(&dir());
        let (_, n) = start(argv(&["sleep", "30"]), vec![], None, false, false).unwrap();
        c.open(&n, &[("data", OREAD)]).unwrap();
        release(&n);
        let pid = c.s.0.borrow().convs[&n.parse::<u32>().unwrap()].child.unwrap();
        let t = Instant::now();
        c.rpc(T::Clunk, W::new().u32(1));
        // SAFETY: probing a process this test started.
        while unsafe { libc::kill(pid, 0) } == 0 {
            assert!(t.elapsed() < Duration::from_secs(10), "sleep was not killed");
            std::thread::sleep(Duration::from_millis(10));
            c.s.harvest();
        }
        // `wait` is `os`'s hold on it: its close alone kills it, though
        // `data` is still open
        let (_, n) = start(argv(&["sleep", "30"]), vec![], None, false, false).unwrap();
        c.open(&n, &[("data", OREAD), ("wait", OREAD)]).unwrap();
        release(&n);
        let pid = c.s.0.borrow().convs[&n.parse::<u32>().unwrap()].child.unwrap();
        c.rpc(T::Clunk, W::new().u32(2));
        let t = Instant::now();
        // SAFETY: probing a process this test started.
        while unsafe { libc::kill(pid, 0) } == 0 {
            assert!(t.elapsed() < Duration::from_secs(10), "closing wait did not kill it");
            std::thread::sleep(Duration::from_millis(10));
            c.s.harvest();
        }
        c.rpc(T::Clunk, W::new().u32(1));
        let (_, n) = start(argv(&["sleep", "1"]), vec![], None, false, true).unwrap();
        c.open(&n, &[("wait", OREAD)]).unwrap();
        release(&n);
        c.rpc(T::Clunk, W::new().u32(1));
        let pid = c.s.0.borrow().convs[&n.parse::<u32>().unwrap()].child.unwrap();
        // SAFETY: as above.
        assert_eq!(unsafe { libc::kill(pid, 0) }, 0, "-b's command was killed");
    }

    /// **Nothing starts until the attach**: a conversation the kernel
    /// never attaches — its call refused — is forgotten when the call lets
    /// it go, and its command never ran.
    #[test]
    fn a_command_starts_at_the_attach_and_not_before() {
        let d = std::env::temp_dir().join(format!("ipnx-oscmd-attach-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let mut c = Client::new(&d);
        let (_, n) = start(argv(&["sh", "-c", "echo ran >ran"]), vec![], None, false, true).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(!d.join("ran").exists(), "it ran before the attach");
        release(&n);
        assert!(c.s.0.borrow().convs.is_empty(), "the conversation outlived the call");
        let (_, n) = start(argv(&["sh", "-c", "echo ran >ran"]), vec![], None, false, false).unwrap();
        c.open(&n, &[("wait", OREAD)]).unwrap();
        release(&n);
        let r = c.rpc(T::Read, W::new().u32(1).u64(0).u32(1024));
        assert!(String::from_utf8_lossy(&r[11..]).ends_with(" 0 0 0 ''"));
        assert!(d.join("ran").exists(), "the attach did not start it");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A read waits for the command to write, and a flushed one is answered
    /// no more — what it would have had goes to the next.
    #[test]
    fn a_read_waits_and_a_flushed_one_is_answered_no_more() {
        let mut c = Client::new(&dir());
        let (_, n) = start(argv(&["sh", "-c", "read x; echo got $x"]), vec![], None, false, false).unwrap();
        c.open(&n, &[("data", OWRITE), ("data", OREAD)]).unwrap();
        release(&n);
        let first = c.send(T::Read, W::new().u32(2).u64(0).u32(100));
        assert!(c.ready.is_empty(), "a read answered before anything was written");
        c.rpc(T::Flush, W::new().u16(first));
        c.rpc(T::Write, W::new().u32(1).u64(0).u32(3).raw(b"it\n"));
        assert_eq!(c.readall(2), b"got it\n");
        assert!(!c.ready.iter().any(|r| r[5..7] == first.to_le_bytes()), "the flushed read was answered");
    }
}
