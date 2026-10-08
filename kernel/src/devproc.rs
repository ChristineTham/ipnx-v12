//! `#p` — proc (`plan9/sys/src/9/port/devproc.c`).
//!
//! **The process table as files.** `/proc/17/status` is a text a program
//! reads; `ps` is a program that reads them and prints them, not a call into
//! the kernel. `/proc/17/ns` prints the process's namespace as the `bind` and
//! `mount` lines that would rebuild it — which is what makes a namespace
//! inspectable at all.
//!
//! `procdir[]` (`devproc.c:79`) is 18 files. Ten are here; `PROCDIR` says
//! why each of the other eight is not.

use crate::chan::Chan;
use crate::dev::{Dev, DevId, Eve};
use crate::ninep::{Qid, QTDIR};
use crate::proc::{Fd, Pid, Up};
use std::cell::RefCell;
use std::rc::Rc;

const EPERM: &str = "permission denied";
const EPROCDIED: &str = "process exited";
/// `Einuse` (`error.h:19`).
const EINUSE: &str = "device or object already in use";
/// `Etoosmall`, `Etoobig`, `Ebadarg` (`error.h`).
const ETOOSMALL: &str = "read or write too small";
const ETOOBIG: &str = "read or write too large";
const EBADARG: &str = "bad arg in system call";
/// `Ebadctl` (`error.h`).
const EBADCTL: &str = "bad process or channel control request";
/// `Egreg` (`error.h:44`) — `procopen`'s default (`devproc.c:459`).
const EGREG: &str = "jmk added reentrancy for threads";
/// `Enonexist` (`error.h:9`).
const ENONEXIST: &str = "file does not exist";

/// `proccmd[]` (`devproc.c:102`): each control message, and how many fields
/// it takes (0 is any).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Cm {
    Close,
    Closefiles,
    Fixedpri,
    Hang,
    Nohang,
    Noswap,
    Kill,
    Pri,
    Private,
    Profile,
    Start,
    Startstop,
    Startsyscall,
    Stop,
    Waitstop,
    Wired,
    Trace,
    Period,
    Deadline,
    Cost,
    Sporadic,
    Deadlinenotes,
    Admit,
    Extra,
    Expel,
    Event,
}

const PROCCMD: &[(Cm, &str, usize)] = &[
    (Cm::Close, "close", 2),
    (Cm::Closefiles, "closefiles", 1),
    (Cm::Fixedpri, "fixedpri", 2),
    (Cm::Hang, "hang", 1),
    (Cm::Nohang, "nohang", 1),
    (Cm::Noswap, "noswap", 1),
    (Cm::Kill, "kill", 1),
    (Cm::Pri, "pri", 2),
    (Cm::Private, "private", 1),
    (Cm::Profile, "profile", 1),
    (Cm::Start, "start", 1),
    (Cm::Startstop, "startstop", 1),
    (Cm::Startsyscall, "startsyscall", 1),
    (Cm::Stop, "stop", 1),
    (Cm::Waitstop, "waitstop", 1),
    (Cm::Wired, "wired", 2),
    (Cm::Trace, "trace", 0),
    (Cm::Period, "period", 2),
    (Cm::Deadline, "deadline", 2),
    (Cm::Cost, "cost", 2),
    (Cm::Sporadic, "sporadic", 1),
    (Cm::Deadlinenotes, "deadlinenotes", 1),
    (Cm::Admit, "admit", 1),
    (Cm::Extra, "extra", 1),
    (Cm::Expel, "expel", 1),
    (Cm::Event, "event", 1),
];

/// `procqidwidth` (`devproc.c:543`): how wide the qid's version prints.
pub fn procqidwidth(c: &crate::chan::Chan) -> usize {
    c.qid.vers.to_string().len()
}

/// `procfdprint` (`devproc.c:551`): one descriptor as `/proc/n/fd` and
/// `#d`'s ctl files show it — *"%3d %.2s %C %4ld (%.16llux %*lud %.2ux)
/// %5ld %8lld %s"*: the mode as `&"r w rw"[(c->mode&3)<<1]`, the device,
/// the qid with its version `w` wide (its own width when `w` is 0), the
/// iounit, the offset and the name.
pub fn procfdprint(c: &crate::chan::Chan, fd: i32, w: usize) -> String {
    let w = if w == 0 { procqidwidth(c) } else { w };
    let m = ["r ", "w ", "rw", ""][(c.mode & 3) as usize];
    format!(
        "{:3} {} {} {:4} ({:016x} {:w$} {:02x}) {:5} {:8} {}\n",
        fd,
        m,
        c.dev.letter(),
        c.devno,
        c.qid.path,
        c.qid.vers,
        c.qid.qtype,
        c.iounit,
        c.offset,
        c.path,
        w = w
    )
}

/// `KNAMELEN` (`portdat.h`) and `STATSIZE` (`devproc.c:73`).
pub const KNAMELEN: usize = 28;
pub const STATSIZE: usize = 2 * KNAMELEN + 12 + 9 * 12;

/// The files of one process, as `procdir[]` names them.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Q {
    Root = 0,
    Args,
    Ctl,
    Fd,
    Note,
    Noteid,
    Notepg,
    Ns,
    Proc,
    Status,
    Wait,
    Profile,
    Syscall,
}

/// name, which file, permission — `procdir[]`'s own values.
/// `procdir[]` (`devproc.c:79`) is eighteen; this is **twelve**, and each of
/// the six absent is absent for a stated reason:
///
/// | | |
/// |---|---|
/// | `fpregs` `kregs` `regs` | a register set. There is none — the machine's registers are the engine's and a wasm program has no `Ureg`. This is the narrow case that needs no approval |
/// | `mem` `segment` `text` | an address space. A wasm program has ONE linear memory and no segments, and `text` is the module — what each should mean here is a design question, not a gap to fill in passing |
pub const PROCDIR: &[(&str, Q, u32)] = &[
    ("args", Q::Args, 0o660),
    ("ctl", Q::Ctl, 0o000),
    ("fd", Q::Fd, 0o444),
    ("note", Q::Note, 0o000),
    ("noteid", Q::Noteid, 0o664),
    ("notepg", Q::Notepg, 0o000),
    ("ns", Q::Ns, 0o444),
    ("proc", Q::Proc, 0o400),
    ("status", Q::Status, 0o444),
    ("wait", Q::Wait, 0o400),
    ("profile", Q::Profile, 0o400),
    ("syscall", Q::Syscall, 0o400),
];

/// A file's length as `procgen` gives it: 0, but for `profile`, which is as
/// long as the text segment's profile when one is kept (`devproc.c:267`).
fn proclen(procs: &crate::proc::Procs, pid: Pid, q: Q) -> u64 {
    if q != Q::Profile {
        return 0;
    }
    procs
        .get(pid)
        .and_then(|p| p.tseg.as_ref())
        .and_then(|s| s.borrow().profile.as_ref().map(|v| v.len() as u64 * 4))
        .unwrap_or(0)
}

/// A qid carries both which process and which of its files: the pid in the
/// high bits, the file in the low. Plan 9 does the same with `mkqid` and
/// `QID(c->qid)`.
const SHIFT: u64 = 8;

/// A file's mode as `procgen` gives it (`devproc.c:254`): *"p->procmode
/// determines default mode for files in /proc"* — a file whose `procdir[]`
/// mode is 0 takes the process's `procmode`, any other gains its read bits.
fn procperm(procs: &crate::proc::Procs, pid: Pid, perm: u32) -> u32 {
    let procmode = procs.get(pid).map_or(0, |p| p.procmode);
    if perm == 0 {
        procmode
    } else {
        perm | (procmode & 0o444)
    }
}

fn qid_of(pid: Pid, q: Q) -> u64 {
    ((pid as u64) << SHIFT) | q as u64
}

fn split_qid(path: u64) -> (Pid, Q) {
    let pid = (path >> SHIFT) as Pid;
    let q = PROCDIR
        .iter()
        .map(|e| e.1)
        .find(|q| *q as u64 == path & ((1 << SHIFT) - 1))
        .unwrap_or(Q::Root);
    (pid, q)
}

pub struct ProcDev {
    up: Rc<RefCell<Up>>,
    eve: Eve,
    /// `#s`'s table, for `srvname` — which `devproc.c` calls to name the
    /// server behind a mount, and reaches because Plan 9's is a file-scope
    /// global with the function declared in `portfns.h`.
    srv: crate::devsrv::Srvtab,
    /// Each process asleep in `procstopwait`, and the process it waits for
    /// to stop — where its `ctl` write carries on when it is woken.
    stopwait: std::collections::HashMap<Pid, Pid>,
}

impl ProcDev {
    pub fn new(up: Rc<RefCell<Up>>) -> ProcDev {
        ProcDev { up, eve: Eve::default(), srv: Default::default(), stopwait: Default::default() }
    }

    /// `procstopwait` (`devproc.c:1223`): wait for `p` to stop — asking it to
    /// with `ctl`, or not. `Ok(true)` is it has; `Ok(false)` is the caller
    /// is asleep and the write is not over.
    fn procstopwait(&mut self, p: Pid, ctl: Option<crate::proc::Procctl>) -> Result<bool, String> {
        use crate::proc::{Rid, State, Which};
        let (me, procs) = {
            let up = self.up.borrow();
            (up.pid, up.procs.clone())
        };
        let mut procs = procs.borrow_mut();
        let target = procs.get_mut(p).ok_or(EPROCDIED)?;
        if target.pdbg.is_some() {
            return Err(EINUSE.into());
        }
        if matches!(target.state, State::Stopped | State::Broken) {
            return Ok(true);
        }
        if ctl.is_some() {
            target.procctl = ctl;
        }
        target.pdbg = Some(me);
        if let Some(m) = procs.get_mut(me) {
            m.psstate = Some("Stopwait".into());
        }
        // *"sleep(&up->sleep, procstopped, p)"*.
        if !procs.sleep(me, Rid::Proc(me, Which::Sleep), false) {
            procs.interrupted(me);
            procs.get_mut(p).map(|t| t.pdbg = None);
            return Err(crate::proc::EINTR.into());
        }
        self.stopwait.insert(me, p);
        Ok(false)
    }

    /// Hand it `#s`'s table. Without one, a mount names its wire channel —
    /// which is `srvname` returning nil, and Plan 9's own fallback.
    pub fn with_srv(mut self, srv: crate::devsrv::Srvtab) -> ProcDev {
        self.srv = srv;
        self
    }

    /// `nonone` (`devproc.c:336`): a process running as `none` cannot read or
    /// write another process's state. The comment says why — *"to contain
    /// access of servers running as none should they be subverted by, for
    /// example, a stack attack."*
    fn nonone(&self, target: Pid) -> Result<(), String> {
        let up = self.up.borrow();
        if target == up.pid {
            return Ok(());
        }
        let user = up.user();
        if user != "none" || crate::dev::iseve(&self.eve, &user) {
            return Ok(());
        }
        Err(EPERM.into())
    }

    fn alive(&self, pid: Pid) -> bool {
        self.up.borrow().procs.borrow().get(pid).is_some()
    }
}

/// `int2flag` (`devproc.c`): the flag word as `bind`'s own letters.
///
/// ```c
/// if(flag == 0){ *s = '\0'; return; }
/// *s++ = '-';
/// if(flag & MAFTER)  *s++ = 'a';
/// if(flag & MBEFORE) *s++ = 'b';
/// if(flag & MCREATE) *s++ = 'c';
/// if(flag & MCACHE)  *s++ = 'C';
/// ```
///
/// **`MREPL` is zero, so it prints nothing** — not `-b`, which is what
/// guessing the flag from an element's position produced.
fn int2flag(flag: i32) -> String {
    use crate::ns::mflag::*;
    if flag == 0 {
        return String::new();
    }
    let mut s = String::from("-");
    for (bit, c) in [(MAFTER, 'a'), (MBEFORE, 'b'), (MCREATE, 'c'), (MCACHE, 'C')] {
        if flag & bit != 0 {
            s.push(c);
        }
    }
    s
}

impl Dev for ProcDev {
    fn seteve(&mut self, eve: Eve) {
        self.eve = eve;
    }

    fn id(&self) -> DevId {
        DevId::Proc
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn attach(&mut self, _spec: &str) -> Result<Chan, String> {
        Ok(Chan::attach(DevId::Proc, 0))
    }

    /// Two levels: `#p` holds a directory per pid, and each holds its files.
    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Chan>, String> {
        if !c.qid.is_dir() {
            return Err("not a directory".into());
        }
        if name == "." {
            return Ok(Some(c.walked(name, c.qid)));
        }
        let (pid, _) = split_qid(c.qid.path);
        if pid == 0 {
            // at `#p`, a name is a pid
            if name == ".." {
                return Ok(Some(c.walked(name, Qid { qtype: QTDIR, vers: 0, path: 0 })));
            }
            let Ok(want): Result<Pid, _> = name.parse() else { return Ok(None) };
            if !self.alive(want) {
                return Ok(None);
            }
            let q = Qid { qtype: QTDIR, vers: 0, path: qid_of(want, Q::Root) };
            return Ok(Some(c.walked(name, q)));
        }
        if name == ".." {
            return Ok(Some(c.walked(name, Qid { qtype: QTDIR, vers: 0, path: 0 })));
        }
        Ok(PROCDIR
            .iter()
            .find(|e| e.0 == name)
            .map(|e| c.walked(name, Qid { qtype: 0, vers: 0, path: qid_of(pid, e.1) })))
    }

    fn open(&mut self, mut c: Chan, mode: u16) -> Result<Chan, String> {
        let (pid, q) = split_qid(c.qid.path);
        // *"if(c->qid.type & QTDIR) return devopen(c, omode, 0, 0,
        // procgen)"* (`devproc.c:356`): a directory opens to read, and that
        // is all.
        if c.qid.is_dir() {
            if mode != crate::chan::mode::OREAD {
                return Err(EPERM.into());
            }
            c.mode = mode;
            c.offset = 0;
            return Ok(c);
        }
        if !self.alive(pid) {
            return Err(EPROCDIED.into());
        }
        // *"omode = openmode(omode)"* (`:396`), then each file's own case.
        let mode = crate::chan::openmode(mode)?;
        let read = crate::chan::mode::OREAD;
        match q {
            Q::Proc | Q::Profile | Q::Fd | Q::Ns => {
                if mode != read {
                    return Err(EPERM.into());
                }
            }
            // `Qnote`: *"if(p->privatemem) error(Eperm)"*, and nothing here
            // is private memory.
            Q::Note => {}
            Q::Ctl | Q::Args | Q::Noteid | Q::Status | Q::Wait | Q::Syscall => self.nonone(pid)?,
            // `Qnotepg` (`devproc.c:446`): write only, never the first
            // namespace group's (`pgrpid == 1`), and the note group is remembered in the
            // channel — *"c->pgrpid.vers = p->noteid"* — so a write reaches
            // the group as it was when opened.
            Q::Notepg => {
                self.nonone(pid)?;
                let procs = self.up.borrow().procs.clone();
                let procs = procs.borrow();
                // *"pg = p->pgrp; if(pg == nil) error(Eprocdied)"*
                let pg = procs.pgrpid(pid).ok_or(EPROCDIED)?;
                if mode != crate::chan::mode::OWRITE || pg == 1 {
                    return Err(EPERM.into());
                }
                c.aux = procs.get(pid).map_or(0, |p| p.noteid) as u64;
            }
            Q::Root => return Err(EGREG.into()),
        }
        // *"tc = devopen(c, omode, 0, 0, procgen)"* (`devproc.c:471`) —
        // `devpermcheck` of the file's mode for its process's user: the
        // `procmode` most files take is `0640`, so another user's process is
        // not someone else's to control or kill (`dev.c:371`).
        let (owner, perm) = {
            let procs = self.up.borrow().procs.clone();
            let procs = procs.borrow();
            let owner = procs.user(pid).ok_or(EPROCDIED)?;
            let perm = PROCDIR.iter().find(|e| e.1 == q).map_or(0, |e| procperm(&procs, pid, e.2));
            (owner, perm)
        };
        let user = self.up.borrow().user();
        crate::dev::permcheck(&user, &owner, &self.eve.borrow(), perm, mode)?;
        c.mode = mode;
        c.offset = 0;
        Ok(c)
    }

    fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
        Err(EPERM.into())
    }

    fn read(&mut self, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String> {
        let (pid, q) = split_qid(c.qid.path);
        let procs = self.up.borrow().procs.clone();

        // `ls /proc` — one directory per living process, each owned by
        // whoever runs it (`procgen`, `devproc.c:300`).
        if pid == 0 {
            let mut pids: Vec<Pid> = procs.borrow().pids();
            pids.sort();
            let p = procs.borrow();
            let entries: Vec<crate::ninep::Dir> = pids
                .iter()
                .map(|pid| {
                    let user = p.user(*pid).unwrap_or_default();
                    let qid = Qid {
                        qtype: QTDIR,
                        vers: 0,
                        path: qid_of(*pid, Q::Root),
                    };
                    crate::dev::devdir(
                        c,
                        qid,
                        &pid.to_string(),
                        0,
                        &user,
                        &self.eve.borrow(),
                        crate::ninep::DMDIR | 0o555,
                    )
                })
                .collect();
            drop(p);
            return Ok(crate::dev::devdirread(c, n, &entries));
        }
        // A process's own directory: `procdir[]`, one entry per file, each
        // with `procgen`'s mode. *"if(c->qid.type & QTDIR) return
        // devdirread(…, procgen)"* (`devproc.c:708`) — anyone may list it.
        if q == Q::Root && c.qid.is_dir() {
            let user = procs.borrow().user(pid).ok_or(EPROCDIED)?;
            let p = procs.borrow();
            let entries: Vec<crate::ninep::Dir> = PROCDIR
                .iter()
                .map(|(name, q, perm)| {
                    let qid = Qid { qtype: 0, vers: 0, path: qid_of(pid, *q) };
                    let perm = procperm(&p, pid, *perm);
                    crate::dev::devdir(c, qid, name, proclen(&p, pid, *q), &user, &self.eve.borrow(), perm)
                })
                .collect();
            return Ok(crate::dev::devdirread(c, n, &entries));
        }

        // `procread`'s `Qsyscall` (`devproc.c:747`): the trace, while there
        // is one.
        if q == Q::Syscall {
            self.nonone(pid)?;
            let p = procs.borrow();
            let t = p.get(pid).ok_or(EPROCDIED)?.syscalltrace.clone().unwrap_or_default();
            let b = t.into_bytes();
            let off = (off as usize).min(b.len());
            return Ok(b[off..(off + n).min(b.len())].to_vec());
        }
        // `Qprofile` (`devproc.c:779`): the text segment's counts, as the
        // machine stores a `ulong` — little-endian here, which `tprof`
        // swaps from (`tprof.c:100`).
        if q == Q::Profile {
            let p = procs.borrow();
            let s = p.get(pid).ok_or(EPROCDIED)?.tseg.clone();
            let s = s.ok_or("profile is off")?;
            let s = s.borrow();
            let v = s.profile.as_ref().ok_or("profile is off")?;
            let b: Vec<u8> = v.iter().flat_map(|c| c.to_le_bytes()).collect();
            let off = (off as usize).min(b.len());
            return Ok(b[off..(off + n).min(b.len())].to_vec());
        }

        // `procread`'s `Qnote` (`devproc.c:792`): take the first note —
        // its message and a NUL, as much as fits — or nothing.
        if q == Q::Note {
            self.nonone(pid)?;
            if n < 1 {
                return Err(ETOOSMALL.into());
            }
            let mut p = procs.borrow_mut();
            let proc = p.get_mut(pid).ok_or(EPROCDIED)?;
            if proc.note.is_empty() {
                return Ok(Vec::new());
            }
            let note = proc.note.remove(0);
            let mut b = note.msg.into_bytes();
            b.push(0);
            b.truncate(n);
            if let Some(last) = b.last_mut() {
                *last = 0;
            }
            return Ok(b);
        }

        let s = {
            self.nonone(pid)?;
            let p = procs.borrow();
            let proc = p.get(pid).ok_or(EPROCDIED)?;
            match q {
                Q::Root => String::new(),
                // `status`: text, user, state, then numbers — the fixed-width
                // record `ps` parses (`devproc.c:867`, STATSIZE).
                Q::Status => {
                    // `statename[p->state]` (`devproc.c`, `procstatus`).
                    // It was "Running" or "Broken" and nothing else, which
                    // is a guess where the kernel now knows.
                    //
                    // `readstr` fills a space-padded field and `readnum`
                    // right-justifies in `NUMSIZE`: text, user, state, then
                    // nine numbers — the six times, memory in K, `basepri`,
                    // `priority` (`devproc.c:869`–`:890`). The memory is the
                    // sum of the segments, and a process here has none: its
                    // memory is the machine's, which `#p` cannot see.
                    // *"sps = p->psstate; if(sps == 0) sps = statename[p->state]"*
                    // (`devproc.c:865`).
                    let state = proc.psstate.clone().unwrap_or_else(|| proc.state.name().into());
                    let state = state.as_str();
                    let field = |v: &str, w: usize| {
                        let v: String = v.chars().take(w - 1).collect();
                        format!("{v:<w$}")
                    };
                    let mut s = field(&proc.text, KNAMELEN) + &field(&proc.user, KNAMELEN) + &field(state, 12);
                    let t = p.cputime(pid);
                    for v in t.iter().copied().chain([0, proc.basepri as u64, proc.priority as u64]) {
                        s.push_str(&String::from_utf8_lossy(&crate::devcons::readnum(v, crate::devcons::NUMSIZE)));
                    }
                    s
                }
                Q::Proc => format!("{pid}\n"),
                // `procread`'s `Qnoteid` (`devproc.c:990`).
                Q::Noteid => {
                    let b = crate::devcons::readnum(proc.noteid as u64, crate::devcons::NUMSIZE);
                    String::from_utf8_lossy(&b).into_owned()
                }
                Q::Args => String::new(),
                // `fd`: the working directory, then one line per open fd —
                // `procfds` and `procfdprint` (`devproc.c:566`, `:551`):
                // *"%3d %.2s %C %4ld (%.16llux %*lud %.2ux) %5ld %8lld %s"*
                // — the mode, the device, the qid (its version as wide as
                // the widest, `procqidwidth`), the iounit and the offset.
                //
                // *"f = p->fgrp; if(f == nil){ qunlock(&p->debug); return
                // 0; }"* (`devproc.c:581`): a process that has exited, or a
                // kernel process, has no descriptors and no `fd`.
                Q::Fd => {
                    let Some(fds) = &proc.fds else { return Ok(Vec::new()) };
                    let dot = proc.dot.as_ref().map(|d| d.path.as_str()).unwrap_or_default();
                    let mut s = format!("{dot}\n");
                    let fds = fds.borrow();
                    let chans: Vec<(i32, crate::chan::Chan)> =
                        (0..fds.slots()).filter_map(|fd| fds.get(fd).map(|c| (fd, c.borrow().clone()))).collect();
                    let w = chans.iter().map(|(_, c)| procqidwidth(c)).max().unwrap_or(0);
                    for (fd, ch) in chans {
                        s.push_str(&procfdprint(&ch, fd, w));
                    }
                    s
                }
                // `ns`: the namespace as the lines that would rebuild it
                // (`devproc.c:952`) — `cd`, then `bind`/`mount` per element.
                // `Qns` (`devproc.c`), line for line. A mount is told from
                // a bind by the channel's own path — *"if(strcmp(
                // mw->cm->to->path->s, "#M") == 0)"* — and then it names
                // the server rather than the mount: `srvname(mchan)`, or
                // the wire channel's path when it was never posted.
                //
                // **These are the lines that rebuild the namespace.** They
                // were `#<letter>/<qid>` on both sides, which is a report of
                // the kernel's bookkeeping and not a namespace.
                //
                // **One mount a read, in the order they were made** —
                // `mntscan` finds the next `mountid` after the last one read
                // (`devproc.c:1000`), and when there is none the read is the
                // `cd` line, then nothing. Plan 9 keeps the last id in
                // `c->aux`; here the reads are told apart by offset, which a
                // reader going forward advances by exactly the line it was
                // given — `ns` reads so (`cmd/ns.c:70`), and tokenizes each
                // read as one line.
                //
                // *"if(p->pgrp == nil || p->pid != PID(c->qid))
                // error(Eprocdied)"* (`devproc.c:958`).
                Q::Ns => {
                    let Some(ns) = &proc.ns else { return Err(EPROCDIED.into()) };
                    let mut lines: Vec<(u32, String)> = Vec::new();
                    for h in ns.borrow().heads() {
                        let Some(from) = &h.from else { continue };
                        for e in &h.mount {
                            let flag = int2flag(e.mflag);
                            let line = if e.chan.path == "#M" {
                                let wire = e.chan.mchan.as_deref();
                                let name = wire
                                    .and_then(|w| crate::devsrv::srvname(&self.srv, w))
                                    .or_else(|| wire.map(|w| w.path.clone()))
                                    .unwrap_or_else(|| "#M".to_string());
                                format!("mount {flag} {name} {} {}\n", from.path, e.spec)
                            } else {
                                format!("bind {flag} {} {}\n", e.chan.path, from.path)
                            };
                            lines.push((e.mountid, line));
                        }
                    }
                    lines.sort_by_key(|(id, _)| *id);
                    let dot = proc.dot.as_ref().map(|d| d.path.as_str()).unwrap_or_default();
                    lines.push((0, format!("cd {dot}\n")));
                    let mut at = 0u64;
                    for (_, l) in lines {
                        let end = at + l.len() as u64;
                        if off < end {
                            let b = &l.as_bytes()[(off - at) as usize..];
                            return Ok(b[..n.min(b.len())].to_vec());
                        }
                        at = end;
                    }
                    return Ok(Vec::new());
                }
                Q::Wait => return Err("wait is await's, not a read".into()),
                Q::Ctl | Q::Notepg => return Err(EPERM.into()),
                Q::Note | Q::Syscall | Q::Profile => unreachable!("taken above"),
            }
        };
        let b = s.into_bytes();
        let off = off as usize;
        if off >= b.len() {
            return Ok(Vec::new());
        }
        Ok(b[off..(off + n).min(b.len())].to_vec())
    }

    /// `procwrite` (`devproc.c:1038`), and `procctlreq` (`:1321`) for
    /// `ctl`: the message is `parsecmd`'s fields and `lookupcmd` finds it
    /// in `proccmd[]`, its count of fields checked.
    fn write(&mut self, c: &mut Chan, data: &[u8], _off: u64) -> Result<usize, String> {
        let (pid, q) = split_qid(c.qid.path);
        if pid == 0 {
            return Err(EPERM.into());
        }
        self.nonone(pid)?;
        let text = String::from_utf8_lossy(data).into_owned();
        let procs = self.up.borrow().procs.clone();
        // *"if(p->kp) error(Eperm)"* — *"no ctl requests to kprocs"*
        // (`devproc.c:1331`).
        if q == Q::Ctl && procs.borrow().get(pid).is_some_and(|p| p.kp) {
            return Err(EPERM.into());
        }
        // The end of `procstopwait`, for a writer woken there: a note ends
        // it with `Eintr`; otherwise the process it waited for has stopped,
        // or gone (*"if(p->pid != pid) error(Eprocdied)"*).
        let me = self.up.borrow().pid;
        if let Some(target) = self.stopwait.remove(&me) {
            let mut p = procs.borrow_mut();
            if p.interrupted(me) {
                p.get_mut(target).map(|t| t.pdbg = None);
                return Err(crate::proc::EINTR.into());
            }
            p.get(target).ok_or(EPROCDIED)?;
            return Ok(data.len());
        }
        let cb = if q == Q::Ctl { crate::dev::parsecmd(data) } else { Vec::new() };
        let ct = if q == Q::Ctl { Some(crate::dev::lookupcmd(&cb, PROCCMD)?) } else { None };
        match (q, ct) {
            // `CMkill` (`devproc.c:1352`): *"p->procctl = Proc_exitme;
            // postnote(p, 0, "sys: killed", NExit)"* — the process ends
            // itself, in `procctl`, on its way out of the kernel. A `Broken`
            // or `Stopped` process is started first there; neither state
            // is built.
            (Q::Ctl, Some(Cm::Kill)) => {
                use crate::proc::State;
                let mut p = procs.borrow_mut();
                match p.get(pid).ok_or(EPROCDIED)?.state {
                    State::Broken => p.unbreak(pid),
                    st => {
                        p.get_mut(pid).expect("checked").procctl = Some(crate::proc::Procctl::Exitme);
                        p.postnote(pid, "sys: killed", crate::proc::NoteFlag::NExit);
                        if st == State::Stopped {
                            p.ready(pid);
                        }
                    }
                }
            }
            // `CMstart` (`devproc.c`): only a `Stopped` process.
            (Q::Ctl, Some(Cm::Start)) => {
                let mut p = procs.borrow_mut();
                if p.get(pid).ok_or(EPROCDIED)?.state != crate::proc::State::Stopped {
                    return Err(EBADCTL.into());
                }
                p.get_mut(pid).expect("checked").psstate = None;
                p.ready(pid);
            }
            // `CMstop` and `CMwaitstop`: `procstopwait`, asking or not.
            (Q::Ctl, Some(cm @ (Cm::Stop | Cm::Waitstop))) => {
                let ctl = (cm == Cm::Stop).then_some(crate::proc::Procctl::Stopme);
                drop(procs);
                if !self.procstopwait(pid, ctl)? {
                    return Ok(0);
                }
            }
            (Q::Ctl, Some(Cm::Hang)) => {
                procs.borrow_mut().get_mut(pid).ok_or(EPROCDIED)?.hang = true;
            }
            (Q::Ctl, Some(Cm::Nohang)) => {
                procs.borrow_mut().get_mut(pid).ok_or(EPROCDIED)?.hang = false;
            }
            // `procctlclosefiles(p, 0, atoi(cb->f[1]))` (`devproc.c:1343`):
            // take the channel out and `cclose` it — a descriptor that is
            // not open is passed over (`procctlcloseone`, `:1251`). `#p`
            // cannot reach `devtab`, so a channel whose last reference this
            // was goes on `clunkq`, and the kernel closes it before the
            // write returns.
            (Q::Ctl, Some(Cm::Close)) => {
                let fd: Fd = crate::dev::atoi(&cb[1]);
                let mut p = procs.borrow_mut();
                let proc = p.get(pid).ok_or(EPROCDIED)?;
                // *"f = p->fgrp; if(f == nil) error(Eprocdied)"*
                // (`devproc.c:1272`)
                let last = proc.fds.as_ref().ok_or(EPROCDIED)?.borrow_mut().close(fd).flatten();
                p.clunkq.extend(last);
            }
            (Q::Ctl, Some(Cm::Closefiles)) => {
                let mut p = procs.borrow_mut();
                let proc = p.get(pid).ok_or(EPROCDIED)?;
                let fds = proc.fds.clone().ok_or(EPROCDIED)?;
                let mut fds = fds.borrow_mut();
                for fd in 0..fds.slots() {
                    if let Some(Some(c)) = fds.close(fd) {
                        p.clunkq.push(c);
                    }
                }
            }
            // `pri n` and `fixedpri n` (`devproc.c:1373`, `:1379`): only the
            // host owner may raise a process above `PriNormal`.
            (Q::Ctl, Some(cm @ (Cm::Pri | Cm::Fixedpri))) => {
                // `atoi` reads the one argument, and what is not a number
                // is 0.
                let pri = crate::dev::atoi(&cb[1]).max(0) as usize;
                let user = self.up.borrow().user();
                if pri > crate::proc::pri::NORMAL && !crate::dev::iseve(&self.eve, &user) {
                    return Err(EPERM.into());
                }
                procs.borrow_mut().procpriority(pid, pri, cm == Cm::Fixedpri);
            }
            // `CMnoswap`, `CMprivate`, `CMwired` (`devproc.c:1370`, `:1385`,
            // `:1424`): `p->noswap`, `p->privatemem` and `procwired` act on
            // swapping, `/proc/n/mem` and a choice of processors, and this
            // machine has none of the three, so each is what Plan 9's is
            // here: nothing. `CMtrace` toggles `p->trace` for `#p/trace`,
            // which is not built; its counts are Plan 9's (`:1427`). An
            // `event` is traced only for a process being traced (`:1489`).
            (Q::Ctl, Some(Cm::Noswap | Cm::Private | Cm::Wired | Cm::Event)) => {}
            (Q::Ctl, Some(Cm::Trace)) => {
                if cb.len() > 2 {
                    return Err("args".into());
                }
            }
            // The real-time scheduler, `edf.c`, is not built: `expel` of a
            // process it does not hold is nothing (*"if(p->edf)
            // edfstop(p)"*, `:1485`), `admit` of one it holds no
            // parameters for is *"edf params"* (`:1474`), and the rest
            // would make it hold one.
            (Q::Ctl, Some(Cm::Expel)) => {}
            (Q::Ctl, Some(Cm::Admit)) => return Err("edf params".into()),
            (Q::Ctl, Some(Cm::Period | Cm::Deadline | Cm::Cost | Cm::Sporadic | Cm::Deadlinenotes | Cm::Extra)) => {
                return Err(EBADCTL.into())
            }
            // `procwrite`'s `Qnote` (`devproc.c:1115`).
            (Q::Note, _) => {
                let mut p = procs.borrow_mut();
                if p.get(pid).ok_or(EPROCDIED)?.kp {
                    return Err(EPERM.into());
                }
                if data.len() >= crate::proc::ERRMAX - 1 {
                    return Err(ETOOBIG.into());
                }
                let msg = String::from_utf8_lossy(data).into_owned();
                if !p.postnote(pid, &msg, crate::proc::NoteFlag::NUser) {
                    return Err("note not posted".into());
                }
            }
            // `procwrite`'s `Qnotepg` (`devproc.c:1054`): `pgrpnote` to the
            // group the channel remembered.
            (Q::Notepg, _) => {
                if data.len() >= crate::proc::ERRMAX - 1 {
                    return Err(ETOOBIG.into());
                }
                let msg = String::from_utf8_lossy(data).into_owned();
                let up = self.up.borrow().pid;
                procs.borrow_mut().pgrpnote(up, c.aux as u32, &msg, crate::proc::NoteFlag::NUser);
            }
            // `procwrite`'s `Qnoteid` (`devproc.c:1125`): join a note group —
            // one's own pid, or a group a process of the same user is in.
            (Q::Noteid, _) => {
                let id = crate::dev::atoi(&text) as u32;
                let mut p = procs.borrow_mut();
                let user = p.user(pid).ok_or(EPROCDIED)?;
                if id != pid {
                    let owner = p
                        .pids()
                        .into_iter()
                        .filter_map(|q| p.get(q))
                        .find(|q| q.noteid == id && q.state != crate::proc::State::Dead)
                        .map(|q| q.user.clone())
                        .ok_or(EBADARG)?;
                    if owner != user {
                        return Err(EPERM.into());
                    }
                }
                p.get_mut(pid).ok_or(EPROCDIED)?.noteid = id;
            }
            // `CMstartstop` and `CMstartsyscall` (`devproc.c:1404`, `:1411`):
            // start a `Stopped` process and wait for it to stop again — at
            // its next note, or on its way into or out of its next call.
            (Q::Ctl, Some(cm @ (Cm::Startstop | Cm::Startsyscall))) => {
                use crate::proc::Procctl;
                let ctl = if cm == Cm::Startstop { Procctl::Traceme } else { Procctl::Tracesyscall };
                {
                    let mut p = procs.borrow_mut();
                    let t = p.get_mut(pid).ok_or(EPROCDIED)?;
                    if t.state != crate::proc::State::Stopped {
                        return Err(EBADCTL.into());
                    }
                    t.procctl = Some(ctl);
                    t.psstate = None;
                    p.ready(pid);
                }
                drop(procs);
                if !self.procstopwait(pid, Some(ctl))? {
                    return Ok(0);
                }
            }
            // `CMprofile` (`devproc.c:1388`): start keeping a profile of the
            // text segment, new and zeroed, *"npc = (s->top-s->base)>>LRESPROF"*.
            (Q::Ctl, Some(Cm::Profile)) => {
                let p = procs.borrow();
                let s = p.get(pid).ok_or(EPROCDIED)?.tseg.clone().ok_or(EBADCTL)?;
                let mut s = s.borrow_mut();
                let npc = (s.size >> crate::proc::LRESPROF) as usize;
                s.profile = Some(vec![0; npc]);
            }
            _ => return Err(EPERM.into()),
        }
        Ok(data.len())
    }

    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        // `procstat` is `devstat` over `procgen` (`devproc.c:325`), which
        // names a process's files and never a directory: `#p` and a
        // process's directory are `devstat`'s own — eve's, named from the
        // path — though `ls -l /proc` lists each as its process's user's.
        let (pid, q) = split_qid(c.qid.path);
        if pid == 0 || c.qid.is_dir() {
            return Ok(crate::dev::devstatdir(c, &self.eve.borrow()).conv_d2m());
        }
        let e = PROCDIR.iter().find(|e| e.1 == q).ok_or(ENONEXIST)?;
        let (name, perm) = (e.0.to_string(), procperm(&self.up.borrow().procs.borrow(), pid, e.2));
        let user = self.up.borrow().procs.borrow().user(pid).unwrap_or_else(|| self.eve.borrow().clone());
        let len = proclen(&self.up.borrow().procs.borrow(), pid, q);
        Ok(crate::dev::devdir(c, c.qid, &name, len, &user, &self.eve.borrow(), perm).conv_d2m())
    }

    /// `procwstat` (`devproc.c:479`): the process's user or eve may set its
    /// `procmode` — the mode its files take — and only eve may give it to
    /// another user.
    fn wstat(&mut self, c: &mut Chan, edir: &[u8]) -> Result<(), String> {
        if c.qid.is_dir() {
            return Err(EPERM.into());
        }
        let (pid, _) = split_qid(c.qid.path);
        self.nonone(pid)?;
        if !self.alive(pid) {
            return Err(EPROCDIED.into());
        }
        let me = self.up.borrow().user();
        let eve = self.eve.borrow().clone();
        let procs = self.up.borrow().procs.clone();
        let owner = procs.borrow().user(pid).ok_or(EPROCDIED)?;
        if me != owner && me != eve {
            return Err(EPERM.into());
        }
        let d = crate::ninep::Dir::conv_m2d(edir).ok_or("stat buffer too small")?;
        let mut procs = procs.borrow_mut();
        let p = procs.get_mut(pid).ok_or(EPROCDIED)?;
        if !d.uid.is_empty() && d.uid != owner {
            if me != eve {
                return Err(EPERM.into());
            }
            p.user = d.uid.clone();
        }
        if d.mode != !0 {
            p.procmode = d.mode & 0o777;
        }
        Ok(())
    }

    fn remove(&mut self, _c: &mut Chan) -> Result<(), String> {
        Err(EPERM.into())
    }

    fn close(&mut self, _c: &mut Chan) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chan::mode::{OREAD, OWRITE};
    use crate::proc::{rf, Procs};

    /// The root channel is named `/`, as the kernel renames it at start-up
    /// (`pc/main.c:242`: `pathclose(up->slash->path); up->slash->path =
    /// newpath("/")`). A fixture that leaves it `#/` tests a path the
    /// running system never has.
    fn root() -> Chan {
        let mut c = Chan::attach(DevId::Root, 0);
        c.path = "/".to_string();
        c
    }

    fn proc() -> (ProcDev, Rc<RefCell<Procs>>) {
        let procs = Rc::new(RefCell::new(Procs::new(root())));
        // The running system starts `eve` empty (`pc/main.c:285`), and the
        // host names the host owner by writing `#c/hostowner`, as Plan 9's
        // boot does (`bootauth.c:56`). A unit test has no host, so it names
        // one.
        procs.borrow_mut().get_mut(1).unwrap().user = "eve".into();
        let up = Rc::new(RefCell::new(Up { pid: 1, procs: procs.clone() }));
        let mut d = ProcDev::new(up);
        d.seteve(crate::dev::Eve::new(RefCell::new("eve".to_string())));
        (d, procs)
    }

    fn open(d: &mut ProcDev, pid: Pid, name: &str, mode: u16) -> Chan {
        let root = d.attach("").unwrap();
        let dir = d.walk(&root, &pid.to_string()).unwrap().expect("no such pid");
        let c = d.walk(&dir, name).unwrap().expect(name);
        d.open(c, mode).unwrap()
    }

    /// The open alone, for a test that wants its refusal.
    fn try_open(d: &mut ProcDev, pid: Pid, name: &str, mode: u16) -> Result<Chan, String> {
        let root = d.attach("").unwrap();
        let dir = d.walk(&root, &pid.to_string()).unwrap().expect("no such pid");
        let c = d.walk(&dir, name).unwrap().expect(name);
        d.open(c, mode)
    }

    /// **Another user's process is not someone else's to control**:
    /// `procopen` ends in `devopen` (`devproc.c:471`), which checks the
    /// file's mode as `procgen` gives it — `ctl`'s and `note`'s are the
    /// process's `procmode`, `0640` (`pc/main.c:283`), so eve, judged by
    /// the group bits, may read and not write, and anyone else may do
    /// neither. The process's user may change `procmode` with `wstat`
    /// (`devproc.c:518`), and a child inherits it (`sysproc.c:172`).
    #[test]
    fn another_users_process_is_not_someone_elses_to_control() {
        let (mut d, procs) = proc();
        let c = procs.borrow_mut().rfork(1, rf::PROC).unwrap();
        procs.borrow_mut().get_mut(c).unwrap().user = "glenda".into();
        assert!(try_open(&mut d, c, "ctl", OWRITE).is_err(), "eve may not write it");
        assert!(try_open(&mut d, c, "note", OWRITE).is_err());
        assert!(try_open(&mut d, c, "status", OREAD).is_ok(), "0444 is anyone's to read");
        // its own user may, and may open the process up
        d.up.borrow_mut().pid = c;
        assert!(try_open(&mut d, c, "ctl", OWRITE).is_ok());
        let mut ctl = open(&mut d, c, "ctl", OWRITE);
        let mut dir = crate::ninep::Dir::default();
        dir.mode = 0o666;
        dir.atime = !0;
        dir.mtime = !0;
        dir.length = !0;
        d.wstat(&mut ctl, &dir.conv_d2m()).unwrap();
        assert_eq!(procs.borrow().get(c).unwrap().procmode, 0o666);
        d.up.borrow_mut().pid = 1;
        assert!(try_open(&mut d, c, "ctl", OWRITE).is_ok(), "now anyone's");
        // `stat` reports the mode the open is checked against
        let st = crate::ninep::Dir::conv_m2d(&d.stat(&ctl).unwrap()).unwrap();
        assert_eq!(st.mode & 0o777, 0o666);
        let grandchild = procs.borrow_mut().rfork(c, rf::PROC).unwrap();
        assert_eq!(procs.borrow().get(grandchild).unwrap().procmode, 0o666, "inherited");
    }

    /// `procwstat` (`devproc.c:479`): only the process's user or eve may
    /// change it, and only eve may give it away.
    #[test]
    fn only_a_processs_user_or_eve_may_wstat_it() {
        let (mut d, procs) = proc();
        let c = procs.borrow_mut().rfork(1, rf::PROC).unwrap();
        procs.borrow_mut().get_mut(c).unwrap().user = "glenda".into();
        let other = procs.borrow_mut().rfork(1, rf::PROC).unwrap();
        procs.borrow_mut().get_mut(other).unwrap().user = "kitty".into();
        let mut st = try_open(&mut d, c, "status", OREAD).unwrap();
        let mut dir = crate::ninep::Dir::default();
        dir.mode = !0;
        dir.atime = !0;
        dir.mtime = !0;
        dir.length = !0;
        dir.uid = "kitty".into();
        d.up.borrow_mut().pid = other;
        assert!(d.wstat(&mut st, &dir.conv_d2m()).is_err(), "not kitty's process");
        d.up.borrow_mut().pid = c;
        assert!(d.wstat(&mut st, &dir.conv_d2m()).is_err(), "its user may not give it away");
        d.up.borrow_mut().pid = 1;
        d.wstat(&mut st, &dir.conv_d2m()).unwrap();
        assert_eq!(procs.borrow().user(c).as_deref(), Some("kitty"), "eve may");
    }

    /// `procread` lists a process's directory for anyone (`devproc.c:708`)
    /// — `none` included, which was refused.
    #[test]
    fn none_may_list_a_process() {
        let (mut d, procs) = proc();
        let c = procs.borrow_mut().rfork(1, rf::PROC).unwrap();
        procs.borrow_mut().get_mut(c).unwrap().user = "none".into();
        d.up.borrow_mut().pid = c;
        let root = d.attach("").unwrap();
        let dir = d.walk(&root, "1").unwrap().unwrap();
        let mut dir = d.open(dir, OREAD).unwrap();
        assert!(!d.read(&mut dir, 4096, 0).unwrap().is_empty());
        assert!(try_open(&mut d, 1, "ctl", OWRITE).is_err(), "and nonone still keeps it from ctl");
    }

    fn read(d: &mut ProcDev, pid: Pid, name: &str) -> String {
        let mut c = open(d, pid, name, OREAD);
        String::from_utf8_lossy(&d.read(&mut c, 4096, 0).unwrap()).to_string()
    }

    /// `ls /proc` is the living processes, and a pid that is not there has no
    /// directory — the tree is generated from the table, not stored.
    #[test]
    fn the_directory_is_one_per_living_process() {
        fn names(d: &mut ProcDev) -> Vec<String> {
            let mut root = d.attach("").unwrap();
            let b = d.read(&mut root, 4096, 0).unwrap();
            crate::ninep::Dir::parse_all(&b).into_iter().map(|e| e.name).collect()
        }

        let (mut d, procs) = proc();
        assert_eq!(names(&mut d), ["1"]);
        let c = procs.borrow_mut().rfork(1, rf::PROC).unwrap();
        assert_eq!(names(&mut d), ["1", &c.to_string()]);

        // Each is a DIRECTORY, owned by whoever runs it.
        let mut root = d.attach("").unwrap();
        let b = d.read(&mut root, 4096, 0).unwrap();
        let first = &crate::ninep::Dir::parse_all(&b)[0];
        assert!(first.qid.is_dir());
        assert_eq!(first.uid, "eve");

        // A read goes on where the last one stopped — `Chan.dri`, not a byte
        // offset — so reading this channel again answers nothing more.
        assert!(d.read(&mut root, 4096, 0).unwrap().is_empty());

        let root = d.attach("").unwrap();
        assert!(d.walk(&root, "99").unwrap().is_none());
    }

    /// `/proc/n/status` carries the user, which is how `ps` shows an owner
    /// without asking the kernel anything.
    #[test]
    fn status_reports_the_user_and_the_state() {
        let (mut d, procs) = proc();
        let s = read(&mut d, 1, "status");
        assert!(s.contains("eve"), "{s}");
        assert!(s.contains("Running"), "{s}");
        // `Moribund`, which is what `pexit` leaves (`proc.c`). It said
        // `Broken` before, and `Broken` is a process that took a fatal note
        // and was kept for a debugger (`broken()`) — not one that exited.
        procs.borrow_mut().exits(1, "");
        assert!(read(&mut d, 1, "status").contains("Moribund"));
    }

    /// `/proc/n/ns` prints the namespace as the lines that would rebuild it.
    /// That is what makes a per-process namespace inspectable at all.
    #[test]
    fn ns_prints_the_namespace_as_bind_lines() {
        let (mut d, procs) = proc();
        let on = root();
        let mut to = Chan::attach(DevId::Pipe, 3);
        to.qid = Qid { qtype: QTDIR, vers: 0, path: 5 };
        procs
            .borrow()
            .get(1)
            .unwrap()
            .ns
            .as_ref()
            .unwrap()
            .borrow_mut()
            .mount(&on, crate::ns::Element::new(to), crate::ns::Bind::Replace);
        // read as `ns` reads (`cmd/ns.c:70`): until a read answers nothing,
        // each read one line (`devproc.c:952`)
        let mut c = open(&mut d, 1, "ns", OREAD);
        let (mut reads, mut off) = (Vec::new(), 0u64);
        loop {
            let b = d.read(&mut c, 4096, off).unwrap();
            if b.is_empty() {
                break;
            }
            off += b.len() as u64;
            reads.push(String::from_utf8(b).unwrap());
        }
        // `bind %s %s %s\n` with `to->path` then `from->path`
        // (`devproc.c`). **Paths, not device letters and qids** — these are
        // the lines that rebuild the namespace, and `#|/5` is not a name any
        // shell can be given.
        assert_eq!(reads, ["bind  #| /\n", "cd /\n"]);
    }

    /// `int2flag` (`devproc.c`) composes the letters and gives **`MREPL` the
    /// empty string**. Guessing the flag from an element's position, as this
    /// did, calls `MREPL` `-b` and cannot say `-ac` at all.
    #[test]
    fn the_flag_word_is_printed_as_binds_own_letters() {
        use crate::ns::mflag::*;
        assert_eq!(int2flag(MREPL), "");
        assert_eq!(int2flag(MAFTER), "-a");
        assert_eq!(int2flag(MBEFORE), "-b");
        assert_eq!(int2flag(MAFTER | MCREATE), "-ac");
        assert_eq!(int2flag(MBEFORE | MCREATE | MCACHE), "-bcC");
    }

    /// A mount prints `mount <flag> <server> <on> <spec>` and names the
    /// server, not the mount: *"if(strcmp(mw->cm->to->path->s, "#M") == 0)"*
    /// then `srvname(mw->cm->to->mchan)`. It printed `bind` for everything.
    #[test]
    fn a_mount_prints_as_a_mount_and_names_the_server() {
        use crate::ns::mflag::{MCREATE, MREPL};
        let (mut d, procs) = proc();
        let on = root();

        // `#M`, carrying the wire it speaks down — as `mntattach` leaves it.
        let mut to = Chan::attach(DevId::Mnt, 0);
        to.qid = Qid { qtype: QTDIR, vers: 0, path: 0 };
        let mut wire = Chan::attach(DevId::Pipe, 7);
        wire.qid = Qid { qtype: 0, vers: 0, path: 3 };
        to.mchan = Some(Box::new(wire.clone()));

        procs.borrow().get(1).unwrap().ns.as_ref().unwrap().borrow_mut().mount(
            &on,
            crate::ns::Element::with(to, MREPL | MCREATE, "main"),
            crate::ns::Bind::Replace,
        );

        // With no `#s` table, `srvname` is nil and Plan 9 falls back to the
        // wire channel's own path.
        let s = read(&mut d, 1, "ns");
        assert!(s.starts_with("mount -c #| / main\n"), "{s}");

        // Posted at `#s/root`, it is named by the name it was posted under.
        let tab = crate::devsrv::Srvtab::default();
        tab.borrow_mut().push(crate::devsrv::Srv::posted(crate::devsrv::ROOTSRV, wire));
        let (mut d, _) = (d.with_srv(tab), ());
        let s = read(&mut d, 1, "ns");
        assert!(s.starts_with("mount -c #s/root / main\n"), "{s}");
    }

    /// `/proc/n/fd` lists the open descriptors, so `ls` of it is what `lsof`
    /// is elsewhere.
    #[test]
    fn fd_lists_the_open_descriptors() {
        let (mut d, procs) = proc();
        procs.borrow().get(1).unwrap().fds.as_ref().unwrap().borrow_mut().add(Chan::attach(DevId::Pipe, 2));
        let s = read(&mut d, 1, "fd");
        assert!(s.lines().count() >= 2, "the cwd, then one line per fd: {s}");
        assert!(s.contains('|'), "the device letter is there: {s}");
        // `procfdprint`'s columns (`devproc.c:557`): mode, device, devno,
        // the qid in parentheses, iounit, offset, name
        let line = s.lines().find(|l| l.contains('|')).unwrap();
        let w: Vec<&str> = line.split_whitespace().collect();
        assert!(line.contains('(') && line.contains(')'), "the qid: {line}");
        assert_eq!(w.len(), 10, "fd mode dev devno (path vers type) iounit offset name: {line}");
    }

    /// `kill` through `/proc/n/ctl` — a process is killed by writing to a
    /// file, which is the whole point of the device.
    #[test]
    fn a_process_is_killed_by_writing_to_its_ctl_file() {
        let (mut d, procs) = proc();
        let c = procs.borrow_mut().rfork(1, rf::PROC).unwrap();
        let mut ctl = open(&mut d, c, "ctl", OWRITE);
        d.write(&mut ctl, b"kill", 0).unwrap();
        // `CMkill` asks; the process ends itself in `procctl` on its way
        // out of the kernel (`devproc.c:1363`, `proc.c:1494`).
        let p = procs.borrow();
        let p = p.get(c).unwrap();
        assert_eq!(p.procctl, Some(crate::proc::Procctl::Exitme));
        assert_eq!(p.note.first().map(|n| n.msg.as_str()), Some("sys: killed"));
        assert!(p.notepending);
    }

    /// `note` (`devproc.c:792`, `:1115`): a write posts, a read takes the
    /// first, with its NUL. `notepg` posts to every process of the group
    /// but the writer (`pgrp.c:16`), and is write only.
    #[test]
    fn notes_are_posted_and_read_through_note_and_notepg() {
        let (mut d, procs) = proc();
        let c = procs.borrow_mut().rfork(1, rf::PROC).unwrap();
        let mut note = open(&mut d, c, "note", OWRITE | 0);
        d.write(&mut note, b"hello", 0).unwrap();
        let mut rd = open(&mut d, c, "note", crate::chan::mode::OREAD);
        assert_eq!(d.read(&mut rd, 64, 0).unwrap(), b"hello\0");
        assert!(d.read(&mut rd, 64, 0).unwrap().is_empty(), "taken");

        // pid 1's namespace group is the first, which `notepg` refuses; a
        // child with its own group can be written to.
        let g = procs.borrow_mut().rfork(1, rf::PROC | rf::NAMEG | rf::NOTEG).unwrap();
        let h = procs.borrow_mut().rfork(g, rf::PROC).unwrap();
        let root = d.attach("").unwrap();
        let dir = d.walk(&root, &g.to_string()).unwrap().unwrap();
        let pg = d.walk(&dir, "notepg").unwrap().unwrap();
        assert!(d.open(pg.clone(), crate::chan::mode::OREAD).is_err(), "write only");
        let mut pg = d.open(pg, OWRITE).unwrap();
        d.write(&mut pg, b"interrupt", 0).unwrap();
        let p = procs.borrow();
        for x in [g, h] {
            assert_eq!(p.get(x).unwrap().note.first().map(|n| n.msg.as_str()), Some("interrupt"));
        }
        assert!(p.get(1).unwrap().note.is_empty(), "another group");
    }

    /// `pri n` and `fixedpri n` (`devproc.c:1373`, `:1379`) are
    /// `procpriority`; only the host owner may go above `PriNormal`.
    #[test]
    fn ctl_sets_priority_and_fixedpri() {
        let (mut d, procs) = proc();
        let c = procs.borrow_mut().rfork(1, rf::PROC).unwrap();
        let mut ctl = open(&mut d, c, "ctl", OWRITE);
        d.write(&mut ctl, b"fixedpri 5", 0).unwrap();
        {
            let p = procs.borrow();
            let p = p.get(c).unwrap();
            assert_eq!((p.basepri, p.priority, p.fixedpri), (5, 5, true));
        }
        d.write(&mut ctl, b"pri 13", 0).unwrap();
        {
            let p = procs.borrow();
            let p = p.get(c).unwrap();
            assert_eq!((p.basepri, p.fixedpri), (13, false), "eve may raise it");
        }
        procs.borrow_mut().get_mut(1).unwrap().user = "glenda".into();
        procs.borrow_mut().get_mut(c).unwrap().user = "glenda".into();
        assert!(d.write(&mut ctl, b"pri 14", 0).is_err(), "nobody else may");
    }

    /// `close` and `closefiles` act on the target's fd table; a descriptor
    /// that is not open is passed over (`procctlcloseone`, `devproc.c:1251`).
    #[test]
    fn ctl_can_close_one_descriptor_or_all_of_them() {
        let (mut d, procs) = proc();
        let fds = procs.borrow().get(1).unwrap().fds.clone().unwrap();
        fds.borrow_mut().add(Chan::attach(DevId::Pipe, 1));
        fds.borrow_mut().add(Chan::attach(DevId::Pipe, 2));
        let mut ctl = open(&mut d, 1, "ctl", OWRITE);
        d.write(&mut ctl, b"close 0", 0).unwrap();
        assert_eq!(fds.borrow().count(), 1);
        d.write(&mut ctl, b"closefiles", 0).unwrap();
        assert_eq!(fds.borrow().count(), 0);
        assert!(d.write(&mut ctl, b"close 9", 0).is_ok());
    }

    /// **A control message is `parsecmd`'s and `lookupcmd`'s** (`parse.c`):
    /// fields as `tokenize` splits them, quotes and all; a known message
    /// with the wrong count of fields is `Ecmdargs`, an unknown one says
    /// so with the fields it was given, as `%q` quotes them.
    #[test]
    fn a_control_message_is_looked_up_in_proccmd() {
        let (mut d, _) = proc();
        let mut ctl = open(&mut d, 1, "ctl", OWRITE);
        assert_eq!(
            d.write(&mut ctl, b"close", 0).unwrap_err(),
            "wrong #args in control message \"close\""
        );
        assert_eq!(
            d.write(&mut ctl, b"frob 'a b'\n", 0).unwrap_err(),
            "unknown control message \"frob 'a b'\""
        );
        assert_eq!(d.write(&mut ctl, b"\n", 0).unwrap_err(), "empty control message");
        assert!(d.write(&mut ctl, b"noswap", 0).is_ok(), "nothing to swap");
        assert_eq!(d.write(&mut ctl, b"admit", 0).unwrap_err(), "edf params");
    }

    /// `nonone` (`devproc.c:336`): a process running as `none` cannot touch
    /// another process's state, against a subverted server.
    #[test]
    fn none_cannot_reach_another_process() {
        let (mut d, procs) = proc();
        let other = procs.borrow_mut().rfork(1, rf::PROC).unwrap();
        procs.borrow_mut().setuser(1, "none");
        assert!(read(&mut d, 1, "status").contains("none"), "its own is fine");
        let root = d.attach("").unwrap();
        let dir = d.walk(&root, &other.to_string()).unwrap().unwrap();
        let c = d.walk(&dir, "status").unwrap().unwrap();
        assert!(d.open(c, OREAD).is_err(), "none must not read another's state");
    }

    /// `stop` (`devproc.c:1230`): the writer waits until the process stops,
    /// which it does in `procctl` on its way out of the kernel; `start`
    /// readies it; `start` of a process that is not stopped is `Ebadctl`.
    #[test]
    fn stop_waits_for_the_process_to_stop_and_start_readies_it() {
        use crate::proc::State;
        let (mut d, procs) = proc();
        let c = procs.borrow_mut().rfork(1, rf::PROC).unwrap();
        let mut ctl = open(&mut d, c, "ctl", OWRITE);
        assert!(d.write(&mut ctl, b"start", 0).is_err(), "not stopped");
        assert_eq!(d.write(&mut ctl, b"stop", 0).unwrap(), 0, "the writer waits");
        assert_eq!(procs.borrow().state(1), State::Wakeme);
        assert_eq!(procs.borrow().get(c).unwrap().procctl, Some(crate::proc::Procctl::Stopme));
        // What `procctl` does when the child next leaves the kernel.
        {
            let mut p = procs.borrow_mut();
            let child = p.get_mut(c).unwrap();
            child.procctl = None;
            child.state = State::Stopped;
            let dbg = child.pdbg.take().unwrap();
            p.wakeup(crate::proc::Rid::Proc(dbg, crate::proc::Which::Sleep));
        }
        assert_eq!(d.write(&mut ctl, b"stop", 0).unwrap(), 4, "the write carries on and ends");
        d.write(&mut ctl, b"start", 0).unwrap();
        assert_eq!(procs.borrow().state(c), State::Ready);
        assert!(d.write(&mut ctl, b"nonsense", 0).is_err());
    }

    /// `startsyscall` and `startstop` (`devproc.c:1404`, `:1411`): only a
    /// stopped process; it is readied with the tracer's `procctl` and the
    /// writer waits for it to stop again. `syscall` reads the trace while
    /// there is one, and nothing otherwise.
    #[test]
    fn startsyscall_and_startstop_start_a_stopped_process_and_wait() {
        use crate::proc::{Procctl, State};
        let (mut d, procs) = proc();
        let c = procs.borrow_mut().rfork(1, rf::PROC).unwrap();
        let mut ctl = open(&mut d, c, "ctl", OWRITE);
        assert_eq!(d.write(&mut ctl, b"startsyscall", 0), Err(EBADCTL.into()), "not stopped");
        for (verb, want) in [(&b"startsyscall"[..], Procctl::Tracesyscall), (b"startstop", Procctl::Traceme)] {
            procs.borrow_mut().get_mut(c).unwrap().state = State::Stopped;
            assert_eq!(d.write(&mut ctl, verb, 0).unwrap(), 0, "the writer waits");
            assert_eq!(procs.borrow().state(c), State::Ready);
            assert_eq!(procs.borrow().get(c).unwrap().procctl, Some(want));
            let mut p = procs.borrow_mut();
            let dbg = p.get_mut(c).unwrap().pdbg.take().unwrap();
            p.wakeup(crate::proc::Rid::Proc(dbg, crate::proc::Which::Sleep));
            drop(p);
            assert!(d.write(&mut ctl, verb, 0).is_ok(), "the write carries on and ends");
        }
        assert_eq!(read(&mut d, c, "syscall"), "");
        procs.borrow_mut().get_mut(c).unwrap().syscalltrace = Some("2 x Close 0 3".into());
        assert_eq!(read(&mut d, c, "syscall"), "2 x Close 0 3");
    }

    /// `profile` (`devproc.c:1388`) starts a profile of the text segment,
    /// one count per eight bytes; `/proc/n/profile` is those counts and is
    /// that long, and before it is started it is *"profile is off"*. A
    /// process with no text segment cannot be profiled.
    #[test]
    fn profile_keeps_a_count_per_eight_bytes_of_text() {
        let (mut d, procs) = proc();
        let mut ctl = open(&mut d, 1, "ctl", OWRITE);
        assert_eq!(d.write(&mut ctl, b"profile", 0), Err(EBADCTL.into()), "no text");
        let seg = Rc::new(RefCell::new(crate::proc::Segment { size: 64, ..Default::default() }));
        procs.borrow_mut().get_mut(1).unwrap().tseg = Some(seg.clone());
        let mut c = open(&mut d, 1, "profile", OREAD);
        assert_eq!(d.read(&mut c, 64, 0), Err("profile is off".into()));
        d.write(&mut ctl, b"profile", 0).unwrap();
        seg.borrow_mut().profile.as_mut().unwrap()[1] = 30;
        let b = d.read(&mut c, 64, 0).unwrap();
        assert_eq!(b.len(), 8 * 4);
        assert_eq!(&b[4..8], &30u32.to_le_bytes());
        let st = crate::ninep::Dir::parse_all(&d.stat(&c).unwrap()).remove(0);
        assert_eq!(st.length, 32);
        let root = d.attach("").unwrap();
        let dir = d.walk(&root, "1").unwrap().unwrap();
        let pf = d.walk(&dir, "profile").unwrap().unwrap();
        assert!(d.open(pf, OWRITE).is_err(), "read only");
    }

    /// `hang` is kept on the process and inherited (`sysproc.c:171`).
    #[test]
    fn hang_is_set_by_ctl_and_inherited() {
        let (mut d, procs) = proc();
        let mut ctl = open(&mut d, 1, "ctl", OWRITE);
        d.write(&mut ctl, b"hang", 0).unwrap();
        let c = procs.borrow_mut().rfork(1, rf::PROC).unwrap();
        assert!(procs.borrow().get(c).unwrap().hang);
        d.write(&mut ctl, b"nohang", 0).unwrap();
        assert!(!procs.borrow().get(1).unwrap().hang);
    }
}
