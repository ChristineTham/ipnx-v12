// The kernel core in Rust — a structural port of supervisor/kernel.mjs (the
// reference implementation; the 130-test guest suite is the conformance
// spec). Same shapes throughout: proc table, per-process namespaces as
// mount maps with longest-prefix walk, refcounted channels, union lists,
// parked device reads. The kernel is a pure state machine: syscalls come in
// through `syscall`, replies leave through per-call senders, and everything
// the platform must do (spawn a guest, write the console, arm a timer)
// leaves as an `Effect` — the embedding shim's contract.

pub mod stat9;

use stat9::{marshal_stat, parse_stat, StatIn, DMDIR, DMSETUID, DMSYMLINK, QTDIR, QTFILE, QTSYMLINK};
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::Arc;

pub type Pid = u32;

// ---- traps (Plan 9's own numbers; lib9 speaks these) ----
pub mod t {
    pub const BIND: i32 = 2;
    pub const CHDIR: i32 = 3;
    pub const CLOSE: i32 = 4;
    pub const DUP: i32 = 5;
    pub const ALARM: i32 = 6;
    pub const EXEC: i32 = 7;
    pub const EXITS: i32 = 8;
    pub const OPEN: i32 = 14;
    pub const SLEEP: i32 = 17;
    pub const RFORK: i32 = 19;
    pub const PIPE: i32 = 21;
    pub const CREATE: i32 = 22;
    pub const FD2PATH: i32 = 23;
    pub const REMOVE: i32 = 25;
    pub const NOTIFY: i32 = 28;
    pub const NOTED: i32 = 29;
    pub const UNMOUNT: i32 = 35;
    pub const SEEK: i32 = 39;
    pub const ERRSTR: i32 = 41;
    pub const STAT: i32 = 42;
    pub const FSTAT: i32 = 43;
    pub const WSTAT: i32 = 44;
    pub const FWSTAT: i32 = 45;
    pub const AWAIT: i32 = 47;
    pub const PREAD: i32 = 50;
    pub const PWRITE: i32 = 51;
    pub const NSEC: i32 = 53;
    pub const LINK: i32 = 60;
    pub const SYMLINK: i32 = 61;
    pub const READLINK: i32 = 62;
    pub const ARGS: i32 = 200;
    pub const NOTEGET: i32 = 202;
}

mod rf {
    pub const NAMEG: i32 = 1;
    pub const ENVG: i32 = 2;
    pub const FDG: i32 = 4;
    pub const NOTEG: i32 = 8;
    pub const PROC: i32 = 16;
    pub const MEM: i32 = 32;
    pub const NOWAIT: i32 = 64;
    pub const CNAMEG: i32 = 0x400;
    pub const CENVG: i32 = 0x800;
    pub const CFDG: i32 = 0x1000;
    pub const NOMNT: i32 = 0x4000;
}

const OTRUNC: u32 = 16;
pub const TXSIZE: usize = 65536;

// ---- what leaves the kernel ----
pub struct AsySnap {
    pub snap: Vec<u8>,
    pub data_ptr: u32,
    pub sp: u32,
}

pub enum Effect {
    Spawn { pid: Pid, image: Arc<Vec<u8>>, argv: Vec<String>, asy: Option<AsySnap> },
    ConsWrite(Vec<u8>),
    Timer { ms: u64, token: u64 },
    Shutdown(i32),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum KAction {
    None,
    ForkResume, // parent's runner restores [0,sp) and unwinds to the guard
    ExecSelf,   // this thread reloads: a Spawn effect carries its new image
    Retire,     // guest exited; the thread ends
    Die,        // killed
}

pub struct KReply {
    pub ret: i32,
    pub aux: i32,
    pub data: Vec<u8>,
    pub action: KAction,
    /// ExecSelf carries the new image so the calling thread reloads in place
    pub load: Option<(Arc<Vec<u8>>, Vec<String>)>,
}

fn ok(ret: i32) -> KReply {
    KReply { ret, aux: 0, data: Vec::new(), action: KAction::None, load: None }
}
fn okd(ret: i32, data: Vec<u8>) -> KReply {
    KReply { ret, aux: 0, data, action: KAction::None, load: None }
}

type KErr = String;
enum Done {
    Now(KReply),
    Parked, // the reply sender was stored somewhere and fires later
}

// ---- channels ----
#[derive(Clone)]
pub struct Chan {
    dev: DevId,
    node: Node,
    path: Option<String>,
    mode: u32,
    offset: u64,
    refs: u32,
}
type ChanR = Rc<RefCell<Chan>>;

#[derive(Clone, Copy, PartialEq, Debug)]
enum DevId {
    Ram,
    Cons,
    Pipe,
    Dup,
    Env,
    Union,
}

// ---- nodes, one enum across the device set ----
#[derive(Clone)]
enum Node {
    Ram(RamRef),
    ConsRoot,
    ConsCons,
    ConsUser,
    ConsPid,
    ConsNull,
    Pipe { p: PipeR, end: usize },
    DupRoot,
    DupFd(ChanR),
    EnvRoot,
    EnvVar(String),
    Union(Rc<Vec<MountEl>>),
}

#[derive(Clone)]
struct DN {
    dev: DevId,
    node: Node,
    path: Option<String>, // what bind(2) recorded; unmount matches by it
}

fn node_eq(a: &Node, b: &Node) -> bool {
    match (a, b) {
        (Node::Ram(x), Node::Ram(y)) => Rc::ptr_eq(x, y),
        (Node::ConsRoot, Node::ConsRoot)
        | (Node::ConsCons, Node::ConsCons)
        | (Node::ConsUser, Node::ConsUser)
        | (Node::ConsPid, Node::ConsPid)
        | (Node::ConsNull, Node::ConsNull)
        | (Node::DupRoot, Node::DupRoot)
        | (Node::EnvRoot, Node::EnvRoot) => true,
        (Node::Pipe { p: x, end: e1 }, Node::Pipe { p: y, end: e2 }) => Rc::ptr_eq(x, y) && e1 == e2,
        (Node::EnvVar(x), Node::EnvVar(y)) => x == y,
        _ => false,
    }
}

#[derive(Clone)]
struct MountEl {
    dn: DN,
    create: bool,
}

// ---- ramfs ----
pub struct Seed {
    pub name: String,
    pub dir: bool,
    pub kids: Vec<Seed>,
    pub data: Vec<u8>,
}

struct RNode {
    name: String,
    qpath: u64,
    dir: bool,
    data: Vec<u8>,
    kids: Vec<(String, RamRef)>, // insertion order, like the JS Map
    uid: String,
    mode: u32,
    atime: u32,
    mtime: u32,
    symlink: Option<String>,
}
type RamRef = Rc<RefCell<RNode>>;

fn kid(node: &RamRef, name: &str) -> Option<RamRef> {
    node.borrow().kids.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
}

// ---- pipes: bidirectional, per devs.mjs ----
struct Pipe {
    q: [VecDeque<Vec<u8>>; 2],
    nbytes: [usize; 2],
    refs: [u32; 2],
    parked: [Vec<Waiter>; 2],
}
type PipeR = Rc<RefCell<Pipe>>;

struct Waiter {
    n: usize,
    reply: Sender<KReply>,
}

// ---- processes ----
type NsR = Rc<RefCell<HashMap<String, Vec<MountEl>>>>;

struct Proc {
    pid: Pid,
    ppid: Pid,
    ns: NsR,
    fdt: FdtR,
    cwd: String,
    cred: Cred,
    env: EnvR,
    umask: u32,
    errstr: String,
    zombies: Vec<String>,
    await_reply: Option<(Sender<KReply>, usize)>, // parked await: (sender, max)
    argv: Vec<String>,
    image: Option<Arc<Vec<u8>>>,
    asyncified: bool,
    borrower: Option<Pid>,
    nomnt: bool,
    nowait: bool,
    note_group: u32,
}

#[derive(Clone)]
pub struct Cred {
    pub euid: String,
    pub ruid: String,
}

type EnvR = Rc<RefCell<HashMap<String, Vec<u8>>>>;

struct Fdt {
    refs: u32,
    fds: Vec<Option<ChanR>>,
}
type FdtR = Rc<RefCell<Fdt>>;

fn new_fdt() -> FdtR {
    Rc::new(RefCell::new(Fdt { refs: 1, fds: Vec::new() }))
}

// ---- the kernel ----
pub struct Kernel {
    procs: HashMap<Pid, Proc>,
    nextpid: Pid,
    next_note_group: u32,
    eve: String,
    ram_root: RamRef,
    qgen: u64,
    cons_buf: Vec<u8>,
    cons_eof: bool,
    cons_parked: Vec<Waiter>,
    sleep_waiters: HashMap<u64, Sender<KReply>>,
    next_token: u64,
    effects: Vec<Effect>,
    pub interactive: bool,
    pub verbose: bool,
}

fn now_secs() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as u32).unwrap_or(0)
}
fn now_nanos() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
}

impl Kernel {
    pub fn new(seed: &Seed, eve: &str) -> Kernel {
        let boot = now_secs();
        let mut qgen = 1u64;
        let root = Self::load_seed(seed, "/", eve, boot, &mut qgen);
        // a writable corner, always
        if kid(&root, "tmp").is_none() {
            let t = Rc::new(RefCell::new(RNode {
                name: "tmp".into(), qpath: qgen, dir: true, data: Vec::new(),
                kids: Vec::new(), uid: eve.into(), mode: 0o777, atime: boot,
                mtime: boot, symlink: None,
            }));
            qgen += 1;
            root.borrow_mut().kids.push(("tmp".into(), t));
        }
        Kernel {
            procs: HashMap::new(),
            nextpid: 1,
            next_note_group: 1,
            eve: eve.into(),
            ram_root: root,
            qgen,
            cons_buf: Vec::new(),
            cons_eof: false,
            cons_parked: Vec::new(),
            sleep_waiters: HashMap::new(),
            next_token: 1,
            effects: Vec::new(),
            interactive: false,
            verbose: false,
        }
    }

    fn load_seed(s: &Seed, name: &str, eve: &str, boot: u32, qgen: &mut u64) -> RamRef {
        let q = *qgen;
        *qgen += 1;
        if s.dir {
            let node = Rc::new(RefCell::new(RNode {
                name: name.into(), qpath: q, dir: true, data: Vec::new(),
                kids: Vec::new(), uid: eve.into(), mode: 0o755, atime: boot,
                mtime: boot, symlink: None,
            }));
            for k in &s.kids {
                let child = Self::load_seed(k, &k.name, eve, boot, qgen);
                node.borrow_mut().kids.push((k.name.clone(), child));
            }
            node
        } else {
            Rc::new(RefCell::new(RNode {
                name: name.into(), qpath: q, dir: false, data: s.data.clone(),
                kids: Vec::new(), uid: eve.into(), mode: 0o644, atime: boot,
                mtime: boot, symlink: None,
            }))
        }
    }

    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    // ---- boot: pid 1 runs init out of the seeded tree ----
    pub fn boot(&mut self, argv: Vec<String>) -> Result<(), String> {
        let pid = self.new_proc(0, Rc::new(RefCell::new(HashMap::new())), new_fdt(), "/".into(),
            Cred { euid: self.eve.clone(), ruid: self.eve.clone() },
            Rc::new(RefCell::new(HashMap::new())), None);
        let dn = self.walk(pid, "/bin/init", false)?;
        let image = Arc::new(self.read_all(&dn)?);
        let p = self.procs.get_mut(&pid).unwrap();
        p.argv = argv.clone();
        p.image = Some(image.clone());
        p.asyncified = false;
        self.effects.push(Effect::Spawn { pid, image, argv, asy: None });
        Ok(())
    }

    fn new_proc(&mut self, ppid: Pid, ns: NsR, fdt: FdtR,
                cwd: String, cred: Cred, env: EnvR, note_group: Option<u32>) -> Pid {
        let pid = self.nextpid;
        self.nextpid += 1;
        let group = note_group.unwrap_or_else(|| {
            let g = self.next_note_group;
            self.next_note_group += 1;
            g
        });
        self.procs.insert(pid, Proc {
            pid, ppid, ns, fdt, cwd, cred, env,
            umask: 0o22, errstr: String::new(), zombies: Vec::new(),
            await_reply: None, argv: Vec::new(), image: None, asyncified: false,
            borrower: None, nomnt: false, nowait: false, note_group: group,
        });
        pid
    }

    // ---- console plumbing (the host feeds stdin, we hand it to readers) ----
    pub fn cons_feed(&mut self, chunk: &[u8]) {
        self.cons_buf.extend_from_slice(chunk);
        self.cons_serve();
    }
    pub fn cons_end(&mut self) {
        self.cons_eof = true;
        self.cons_serve();
    }
    fn cons_serve(&mut self) {
        while !self.cons_parked.is_empty() && (!self.cons_buf.is_empty() || self.cons_eof) {
            let w = self.cons_parked.remove(0);
            let take = w.n.min(self.cons_buf.len());
            let give: Vec<u8> = self.cons_buf.drain(0..take).collect();
            let _ = w.reply.send(okd(give.len() as i32, give));
        }
    }

    // the runner reports what it learned at instantiation
    pub fn set_asyncified(&mut self, pid: Pid, on: bool) {
        if let Some(p) = self.procs.get_mut(&pid) {
            p.asyncified = on;
        }
    }

    pub fn timer_fired(&mut self, token: u64) {
        if let Some(tx) = self.sleep_waiters.remove(&token) {
            let _ = tx.send(ok(0));
        }
    }

    // a guest thread died outside the syscall path (bad image, runner error)
    pub fn proc_died(&mut self, pid: Pid, msg: &str) {
        if let Some(p) = self.procs.remove(&pid) {
            self.fdt_close(&p.fdt);
            if pid == 1 {
                self.effects.push(Effect::Shutdown(1));
                return;
            }
            self.zombie(p.ppid, pid, msg, p.nowait);
        }
    }

    // ---- namespace ----
    fn canon(path: &str, cwd: &str) -> String {
        let path = if !path.starts_with('/') && !path.starts_with('#') {
            format!("{}/{}", cwd, path)
        } else {
            path.to_string()
        };
        if path.starts_with('#') {
            return path;
        }
        let mut out: Vec<&str> = Vec::new();
        for c in path.split('/') {
            match c {
                "" | "." => {}
                ".." => { out.pop(); } // lexical, per cleanname(2)
                c => out.push(c),
            }
        }
        format!("/{}", out.join("/"))
    }

    fn attach(&mut self, spec: &str, pid: Pid) -> Result<DN, KErr> {
        let letter = spec.chars().nth(1).unwrap_or(' ');
        match letter {
            'c' => Ok(DN { dev: DevId::Cons, node: Node::ConsRoot, path: None }),
            'e' => Ok(DN { dev: DevId::Env, node: Node::EnvRoot, path: None }),
            'd' => Ok(DN { dev: DevId::Dup, node: Node::DupRoot, path: None }),
            'M' => Ok(DN { dev: DevId::Ram, node: Node::Ram(self.ram_root.clone()), path: None }),
            _ => {
                let _ = pid;
                Err(format!("unknown device #{}", letter))
            }
        }
    }

    fn dev_walk(&mut self, dn: &DN, name: &str, pid: Pid) -> Result<Option<DN>, KErr> {
        match (&dn.dev, &dn.node) {
            (DevId::Ram, Node::Ram(r)) => Ok(kid(r, name).map(|k| DN { dev: DevId::Ram, node: Node::Ram(k), path: None })),
            (DevId::Cons, Node::ConsRoot) => Ok(match name {
                "cons" => Some(Node::ConsCons),
                "user" => Some(Node::ConsUser),
                "pid" => Some(Node::ConsPid),
                "null" => Some(Node::ConsNull),
                _ => None,
            }
            .map(|n| DN { dev: DevId::Cons, node: n, path: None })),
            (DevId::Dup, Node::DupRoot) => {
                let fdn: usize = match name.parse() {
                    Ok(v) => v,
                    Err(_) => return Ok(None),
                };
                let p = self.procs.get(&pid).ok_or("no proc")?;
                let c = p.fdt.borrow().fds.get(fdn).cloned().flatten();
                Ok(c.map(|c| DN { dev: DevId::Dup, node: Node::DupFd(c), path: None }))
            }
            (DevId::Env, Node::EnvRoot) => {
                let p = self.procs.get(&pid).ok_or("no proc")?;
                if p.env.borrow().contains_key(name) {
                    Ok(Some(DN { dev: DevId::Env, node: Node::EnvVar(name.into()), path: None }))
                } else {
                    Ok(None)
                }
            }
            (DevId::Union, Node::Union(list)) => {
                let els: Vec<MountEl> = list.iter().cloned().collect();
                for el in els {
                    if let Ok(Some(dn2)) = self.dev_walk(&el.dn, name, pid) {
                        return Ok(Some(dn2)); // leaving the union: real dev takes over
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn symtarget(&self, dn: &DN) -> Option<String> {
        if let Node::Ram(r) = &dn.node {
            return r.borrow().symlink.clone();
        }
        None
    }

    fn walk(&mut self, pid: Pid, path: &str, nofollow_last: bool) -> Result<DN, KErr> {
        let cwd = self.procs.get(&pid).map(|p| p.cwd.clone()).unwrap_or_else(|| "/".into());
        let mut full = Self::canon(path, &cwd);
        for depth in 0.. {
            if depth > 8 {
                return Err("too many levels of symlinks".into());
            }
            match self.walk_once(pid, &full, nofollow_last)? {
                WalkRes::Hit(dn) => return Ok(dn),
                WalkRes::Redirect(r) => full = Self::canon(&r, &cwd),
            }
        }
        unreachable!()
    }

    fn walk_once(&mut self, pid: Pid, path: &str, nofollow_last: bool) -> Result<WalkRes, KErr> {
        if path.starts_with('#') {
            let nomnt = self.procs.get(&pid).map(|p| p.nomnt).unwrap_or(false);
            if nomnt {
                return Err("'#' names disallowed (RFNOMNT)".into());
            }
            let slash = path.find('/');
            let spec = match slash {
                Some(i) => &path[..i],
                None => path,
            };
            let mut dn = self.attach(spec, pid)?;
            if let Some(i) = slash {
                for name in path[i + 1..].split('/').filter(|s| !s.is_empty()) {
                    dn = self
                        .dev_walk(&dn, name, pid)?
                        .ok_or_else(|| format!("'{}' does not exist", path))?;
                }
            }
            return Ok(WalkRes::Hit(dn));
        }
        // longest-prefix bind
        let (best, list) = {
            let p = self.procs.get(&pid).ok_or("no proc")?;
            let ns = p.ns.borrow();
            let mut best = String::new();
            let mut list: Option<Vec<MountEl>> = None;
            for (pfx, l) in ns.iter() {
                let matches = path == pfx
                    || path.starts_with(&(if pfx == "/" { "/".to_string() } else { format!("{}/", pfx) }));
                if matches && pfx.len() > best.len() {
                    best = pfx.clone();
                    list = Some(l.clone());
                }
            }
            (best, list)
        };
        let mut dn = match &list {
            Some(l) if l.len() == 1 => l[0].dn.clone(),
            Some(l) => DN { dev: DevId::Union, node: Node::Union(Rc::new(l.clone())), path: None },
            None => DN { dev: DevId::Ram, node: Node::Ram(self.ram_root.clone()), path: None },
        };
        let full_comps: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let rest: Vec<&str> = path[best.len()..].split('/').filter(|s| !s.is_empty()).collect();
        for (i, name) in rest.iter().enumerate() {
            let next = self
                .dev_walk(&dn, name, pid)?
                .ok_or_else(|| format!("'{}' does not exist", path))?;
            dn = next;
            if i == rest.len() - 1 && nofollow_last {
                break;
            }
            if let Some(target) = self.symtarget(&dn) {
                let here = full_comps.len() - rest.len() + i;
                let base = if target.starts_with('/') {
                    target
                } else {
                    format!("/{}", full_comps[..here].iter().chain([target.as_str()].iter()).cloned().collect::<Vec<_>>().join("/"))
                };
                let rem = rest[i + 1..].join("/");
                let redirect = if rem.is_empty() { base } else { format!("{}/{}", base, rem) };
                return Ok(WalkRes::Redirect(redirect));
            }
        }
        dn.path = Some(path.to_string());
        Ok(WalkRes::Hit(dn))
    }

    fn walk_parent(&mut self, pid: Pid, path: &str) -> Result<(DN, String), KErr> {
        let cwd = self.procs.get(&pid).map(|p| p.cwd.clone()).unwrap_or_else(|| "/".into());
        let path = Self::canon(path, &cwd);
        let i = path.rfind('/').unwrap_or(0);
        let base = path[i + 1..].to_string();
        if base.is_empty() || path.starts_with('#') {
            return Err(format!("bad path '{}'", path));
        }
        let parent = self.walk(pid, if i == 0 { "/" } else { &path[..i] }, false)?;
        Ok((parent, base))
    }

    fn ns_insert(&mut self, pid: Pid, old: &str, dn: DN, flag: i32) -> Result<(), KErr> {
        let mode = flag & 3;
        let create = flag & 4 != 0;
        let el = MountEl { dn, create };
        if mode == 0 {
            let p = self.procs.get_mut(&pid).ok_or("no proc")?;
            p.ns.borrow_mut().insert(old.into(), vec![el]);
            return Ok(());
        }
        let have = self.procs.get(&pid).ok_or("no proc")?.ns.borrow().get(old).cloned();
        let mut list = match have {
            Some(l) => l,
            None => {
                let under = self.walk(pid, old, false)?; // must exist, per bind(2)
                match (&under.dev, &under.node) {
                    (DevId::Union, Node::Union(l)) => l.as_ref().clone(),
                    _ => vec![MountEl { dn: under, create: false }],
                }
            }
        };
        if mode == 2 {
            list.push(el); // MAFTER
        } else {
            list.insert(0, el); // MBEFORE
        }
        self.procs.get_mut(&pid).ok_or("no proc")?.ns.borrow_mut().insert(old.into(), list);
        Ok(())
    }

    // ---- device I/O ----
    fn ram_stat(node: &RamRef) -> Vec<u8> {
        let n = node.borrow();
        let qtype = if n.dir { QTDIR } else if n.symlink.is_some() { QTSYMLINK } else { QTFILE };
        let dm = if n.dir { DMDIR } else if n.symlink.is_some() { DMSYMLINK } else { 0 };
        let length = if n.dir { 0 } else if let Some(s) = &n.symlink { s.len() as u64 } else { n.data.len() as u64 };
        marshal_stat(&StatIn {
            name: &n.name, uid: &n.uid, gid: &n.uid, qpath: n.qpath,
            atime: n.atime, mtime: n.mtime, qtype, mode: dm | n.mode, length,
            ..Default::default()
        })
    }

    fn ram_access(&self, node: &RamRef, cred: &Cred, want: u32) -> Result<(), KErr> {
        if cred.euid == self.eve {
            return Ok(());
        }
        let n = node.borrow();
        let bits = if n.uid == cred.euid { n.mode >> 6 } else { n.mode };
        if bits & want != want {
            return Err(format!(
                "permission denied ('{}' is {}'s, mode {:o})",
                n.name, n.uid, n.mode & 0o777
            ));
        }
        Ok(())
    }

    fn dev_stat(&mut self, dn: &DN, pid: Pid) -> Result<Vec<u8>, KErr> {
        match (&dn.dev, &dn.node) {
            (DevId::Ram, Node::Ram(r)) => Ok(Self::ram_stat(r)),
            (DevId::Cons, n) => {
                let (name, dir) = match n {
                    Node::ConsRoot => ("/", true),
                    Node::ConsCons => ("cons", false),
                    Node::ConsUser => ("user", false),
                    Node::ConsPid => ("pid", false),
                    Node::ConsNull => ("null", false),
                    _ => return Err("no stat".into()),
                };
                Ok(marshal_stat(&StatIn {
                    name,
                    qtype: if dir { QTDIR } else { QTFILE },
                    mode: if dir { DMDIR | 0o555 } else { 0o666 },
                    ..Default::default()
                }))
            }
            (DevId::Union, Node::Union(list)) => {
                let first = list[0].dn.clone();
                self.dev_stat(&first, pid)
            }
            (DevId::Env, Node::EnvVar(name)) => {
                let p = self.procs.get(&pid).ok_or("no proc")?;
                let len = p.env.borrow().get(name).map(|v| v.len()).unwrap_or(0);
                Ok(marshal_stat(&StatIn { name, mode: 0o664, length: len as u64, ..Default::default() }))
            }
            (DevId::Env, Node::EnvRoot) => Ok(marshal_stat(&StatIn {
                name: "/", qtype: QTDIR, mode: DMDIR | 0o775, ..Default::default()
            })),
            (DevId::Pipe, _) => Ok(marshal_stat(&StatIn { name: "data", mode: 0o600, ..Default::default() })),
            _ => Err("no stat on this device (v0)".into()),
        }
    }

    // read; may PARK (registering the reply sender) — the JS ctx.done shape
    fn dev_read(&mut self, chan: &ChanR, n: usize, off: u64, pid: Pid,
                reply: &Sender<KReply>) -> Result<Option<Vec<u8>>, KErr> {
        let (dev, node) = {
            let c = chan.borrow();
            (c.dev, c.node.clone())
        };
        match (dev, node) {
            (DevId::Ram, Node::Ram(r)) => {
                if r.borrow().dir {
                    // read(5): an integral number of directory entries
                    let mut skip = off as usize;
                    let mut out = Vec::new();
                    let kids: Vec<RamRef> = r.borrow().kids.iter().map(|(_, v)| v.clone()).collect();
                    for k in kids {
                        let rec = Self::ram_stat(&k);
                        if skip >= rec.len() {
                            skip -= rec.len();
                            continue;
                        }
                        if out.len() + rec.len() > n {
                            break;
                        }
                        out.extend_from_slice(&rec);
                    }
                    Ok(Some(out))
                } else {
                    let d = r.borrow();
                    let start = (off as usize).min(d.data.len());
                    let end = (off as usize + n).min(d.data.len());
                    Ok(Some(d.data[start..end].to_vec()))
                }
            }
            (DevId::Cons, Node::ConsUser) => {
                let euid = self.procs.get(&pid).map(|p| p.cred.euid.clone()).unwrap_or_default();
                Ok(Some(if off == 0 { euid.into_bytes() } else { Vec::new() }))
            }
            (DevId::Cons, Node::ConsPid) => {
                Ok(Some(if off == 0 { pid.to_string().into_bytes() } else { Vec::new() }))
            }
            (DevId::Cons, Node::ConsNull) => Ok(Some(Vec::new())),
            (DevId::Cons, Node::ConsCons) => {
                if !self.cons_buf.is_empty() || self.cons_eof {
                    let take = n.min(self.cons_buf.len());
                    let give: Vec<u8> = self.cons_buf.drain(0..take).collect();
                    Ok(Some(give))
                } else {
                    self.cons_parked.push(Waiter { n, reply: reply.clone() });
                    Ok(None)
                }
            }
            (DevId::Pipe, Node::Pipe { p, end }) => {
                let d = 1 ^ end;
                let mut pb = p.borrow_mut();
                if pb.nbytes[d] > 0 {
                    Ok(Some(Self::pipe_drain(&mut pb, d, n)))
                } else if pb.refs[d] == 0 {
                    Ok(Some(Vec::new())) // EOF
                } else {
                    pb.parked[d].push(Waiter { n, reply: reply.clone() });
                    Ok(None)
                }
            }
            (DevId::Env, Node::EnvVar(name)) => {
                let p = self.procs.get(&pid).ok_or("no proc")?;
                let env = p.env.borrow();
                let data = env.get(&name).cloned().unwrap_or_default();
                let start = (off as usize).min(data.len());
                let end = (off as usize + n).min(data.len());
                Ok(Some(data[start..end].to_vec()))
            }
            (DevId::Env, Node::EnvRoot) => {
                let p = self.procs.get(&pid).ok_or("no proc")?;
                let mut skip = off as usize;
                let mut out = Vec::new();
                let entries: Vec<(String, usize)> = p.env.borrow().iter()
                    .map(|(k, v)| (k.clone(), v.len())).collect();
                for (k, vlen) in entries {
                    let rec = marshal_stat(&StatIn {
                        name: &k, mode: 0o664, length: vlen as u64, ..Default::default()
                    });
                    if skip >= rec.len() { skip -= rec.len(); continue; }
                    if out.len() + rec.len() > n { break; }
                    out.extend_from_slice(&rec);
                }
                Ok(Some(out))
            }
            (DevId::Union, Node::Union(list)) => {
                // concatenated listings, still integral records
                let mut skip = off as usize;
                let mut out = Vec::new();
                let els: Vec<MountEl> = list.iter().cloned().collect();
                for el in els {
                    let listing = self.list_dir(&el.dn, pid)?;
                    let mut o = 0usize;
                    while o + 2 <= listing.len() {
                        let size = u16::from_le_bytes([listing[o], listing[o + 1]]) as usize + 2;
                        let rec = &listing[o..(o + size).min(listing.len())];
                        o += size;
                        if skip >= rec.len() { skip -= rec.len(); continue; }
                        if out.len() + rec.len() > n { return Ok(Some(out)); }
                        out.extend_from_slice(rec);
                    }
                }
                Ok(Some(out))
            }
            _ => Err("read not supported here (v0)".into()),
        }
    }

    fn list_dir(&mut self, dn: &DN, pid: Pid) -> Result<Vec<u8>, KErr> {
        // full listing of one union element (synchronous devices only in v1)
        let chan = Rc::new(RefCell::new(Chan {
            dev: dn.dev, node: dn.node.clone(), path: None, mode: 0, offset: 0, refs: 1,
        }));
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut out = Vec::new();
        let mut off = 0u64;
        loop {
            match self.dev_read(&chan, 8192, off, pid, &tx)? {
                Some(chunk) if chunk.is_empty() => break,
                Some(chunk) => {
                    off += chunk.len() as u64;
                    out.extend_from_slice(&chunk);
                }
                None => return Err("parked in list_dir".into()),
            }
        }
        Ok(out)
    }

    fn pipe_drain(p: &mut Pipe, d: usize, want: usize) -> Vec<u8> {
        let mut out = Vec::new();
        while let Some(head) = p.q[d].front_mut() {
            if out.len() >= want {
                break;
            }
            let take = head.len().min(want - out.len());
            out.extend_from_slice(&head[..take]);
            if take == head.len() {
                p.q[d].pop_front();
            } else {
                head.drain(0..take);
            }
        }
        p.nbytes[d] -= out.len();
        out
    }

    fn pipe_serve(p: &PipeR, d: usize) {
        loop {
            let fired = {
                let mut pb = p.borrow_mut();
                if pb.parked[d].is_empty() || (pb.nbytes[d] == 0 && pb.refs[d] != 0) {
                    return;
                }
                let w = pb.parked[d].remove(0);
                let give = if pb.nbytes[d] > 0 { Self::pipe_drain(&mut pb, d, w.n) } else { Vec::new() };
                (w.reply, give)
            };
            let _ = fired.0.send(okd(fired.1.len() as i32, fired.1));
        }
    }

    fn dev_write(&mut self, chan: &ChanR, data: &[u8], off_in: u64, pid: Pid) -> Result<usize, KErr> {
        let (dev, node) = {
            let c = chan.borrow();
            (c.dev, c.node.clone())
        };
        match (dev, node) {
            (DevId::Ram, Node::Ram(r)) => {
                let off = off_in as usize;
                let mut n = r.borrow_mut();
                if n.dir {
                    return Err("write on directory".into());
                }
                let end = off + data.len();
                if end > n.data.len() {
                    n.data.resize(end, 0);
                }
                n.data[off..end].copy_from_slice(data);
                n.mtime = now_secs();
                Ok(data.len())
            }
            (DevId::Cons, Node::ConsNull) => Ok(data.len()),
            (DevId::Cons, _) => {
                self.effects.push(Effect::ConsWrite(data.to_vec()));
                Ok(data.len())
            }
            (DevId::Pipe, Node::Pipe { p, end }) => {
                {
                    let mut pb = p.borrow_mut();
                    if pb.refs[1 ^ end] == 0 {
                        return Err("write on closed pipe".into());
                    }
                    pb.q[end].push_back(data.to_vec());
                    pb.nbytes[end] += data.len();
                }
                Self::pipe_serve(&p, end);
                Ok(data.len())
            }
            (DevId::Env, Node::EnvVar(name)) => {
                let p = self.procs.get(&pid).ok_or("no proc")?;
                p.env.borrow_mut().insert(name.clone(), data.to_vec());
                Ok(data.len())
            }
            _ => Err("write not supported here (v0)".into()),
        }
    }

    fn dev_len(&self, chan: &ChanR) -> u64 {
        let c = chan.borrow();
        match (&c.dev, &c.node) {
            (DevId::Ram, Node::Ram(r)) => {
                let n = r.borrow();
                if n.dir { 0 } else { n.data.len() as u64 }
            }
            _ => 0,
        }
    }

    fn clunk(&mut self, chan: &ChanR) {
        let node = {
            let mut c = chan.borrow_mut();
            c.refs -= 1;
            if c.refs > 0 {
                return;
            }
            c.node.clone()
        };
        if let Node::Pipe { p, end } = node {
            {
                let mut pb = p.borrow_mut();
                pb.refs[end] -= 1;
            }
            let dry = p.borrow().refs[end] == 0;
            if dry {
                Self::pipe_serve(&p, end); // wake readers: data then EOF
            }
        }
    }

    fn fdt_close(&mut self, fdt: &FdtR) {
        let done = {
            let mut f = fdt.borrow_mut();
            f.refs -= 1;
            f.refs == 0
        };
        if done {
            let fds: Vec<ChanR> = fdt.borrow_mut().fds.drain(..).flatten().collect();
            for c in fds {
                self.clunk(&c);
            }
        }
    }

    fn fdt_copy(&mut self, fdt: &FdtR) -> FdtR {
        let fds: Vec<Option<ChanR>> = fdt.borrow().fds.clone();
        for c in fds.iter().flatten() {
            c.borrow_mut().refs += 1;
        }
        Rc::new(RefCell::new(Fdt { refs: 1, fds }))
    }

    fn fd_alloc(&mut self, pid: Pid, chan: ChanR, at: Option<usize>) -> i32 {
        let fdt = self.procs.get(&pid).unwrap().fdt.clone();
        if let Some(at) = at {
            let old = {
                let mut f = fdt.borrow_mut();
                if f.fds.len() <= at {
                    f.fds.resize(at + 1, None);
                }
                f.fds[at].take()
            };
            if let Some(old) = old {
                self.clunk(&old);
            }
            fdt.borrow_mut().fds[at] = Some(chan);
            return at as i32;
        }
        let mut f = fdt.borrow_mut();
        for (i, slot) in f.fds.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(chan);
                return i as i32;
            }
        }
        f.fds.push(Some(chan));
        (f.fds.len() - 1) as i32
    }

    fn fdchk(&self, pid: Pid, fd: i32) -> Result<ChanR, KErr> {
        let p = self.procs.get(&pid).ok_or("no proc")?;
        if fd < 0 {
            return Err(format!("fd {} not open", fd));
        }
        let c = p.fdt.borrow().fds.get(fd as usize).cloned().flatten();
        c.ok_or_else(|| format!("fd {} not open", fd))
    }

    fn zombie(&mut self, ppid: Pid, pid: Pid, msg: &str, nowait: bool) {
        if nowait {
            return;
        }
        if let Some(parent) = self.procs.get_mut(&ppid) {
            parent.zombies.push(format!("{} 0 0 0 '{}'", pid, msg));
            if let Some((tx, max)) = parent.await_reply.take() {
                let s = parent.zombies.remove(0);
                let mut bytes = s.into_bytes();
                bytes.truncate(max.saturating_sub(1));
                bytes.push(0);
                let n = bytes.len() as i32 - 1;
                let _ = tx.send(okd(n, bytes));
            }
        }
    }

    fn read_all(&mut self, dn: &DN) -> Result<Vec<u8>, KErr> {
        let chan = Rc::new(RefCell::new(Chan {
            dev: dn.dev, node: dn.node.clone(), path: None, mode: 0, offset: 0, refs: 1,
        }));
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut out = Vec::new();
        let mut off = 0u64;
        loop {
            match self.dev_read(&chan, 65536, off, 1, &tx)? {
                Some(chunk) if chunk.is_empty() => break,
                Some(chunk) => {
                    off += chunk.len() as u64;
                    out.extend_from_slice(&chunk);
                }
                None => return Err("parked reading an image".into()),
            }
        }
        Ok(out)
    }

    // ---- the dispatcher ----
    // tx holds the marshalled strings/buffers, exactly as the JS transfer SAB
    // did; replies carry copy-out bytes the runner writes to guest memory.
    pub fn syscall(&mut self, worker_pid: Pid, trap: i32, a: [i32; 5], tx: Vec<u8>,
                   reply: Sender<KReply>) {
        // borrowed-worker routing: a lazy-fork child issues syscalls on the
        // parent's thread; the borrower record owns them
        let pid = self
            .procs
            .get(&worker_pid)
            .and_then(|p| p.borrower)
            .unwrap_or(worker_pid);
        if self.verbose {
            eprintln!("[K sys pid={} trap={} a0={}]", pid, trap, a[0]);
        }
        match self.dispatch(worker_pid, pid, trap, a, &tx, &reply) {
            Ok(Done::Now(r)) => {
                let _ = reply.send(r);
            }
            Ok(Done::Parked) => {}
            Err(msg) => {
                if self.verbose {
                    eprintln!("[K err pid={} trap={}: {}]", pid, trap, msg);
                }
                if let Some(p) = self.procs.get_mut(&pid) {
                    p.errstr = msg;
                }
                let _ = reply.send(ok(-1));
            }
        }
    }

    fn txstr(tx: &[u8], off: usize) -> String {
        if off >= tx.len() {
            return String::new();
        }
        let end = tx[off..].iter().position(|&b| b == 0).map(|i| off + i).unwrap_or(tx.len());
        String::from_utf8_lossy(&tx[off..end]).into_owned()
    }

    fn dispatch(&mut self, worker_pid: Pid, pid: Pid, trap: i32, a: [i32; 5], tx: &[u8],
                reply: &Sender<KReply>) -> Result<Done, KErr> {
        use t::*;
        match trap {
            BIND => {
                if self.procs.get(&pid).ok_or("no proc")?.nomnt {
                    return Err("mounting disallowed (RFNOMNT)".into());
                }
                let name = Self::txstr(tx, 0);
                let old = Self::txstr(tx, name.len() + 1);
                let src = self.walk(pid, &name, false)?; // resolved now, per bind(2)
                let cwd = self.procs.get(&pid).unwrap().cwd.clone();
                let oldc = Self::canon(&old, &cwd);
                self.ns_insert(pid, &oldc, src, a[2])?;
                Ok(Done::Now(ok(0)))
            }
            CHDIR => {
                let path = Self::txstr(tx, 0);
                let cwd = self.procs.get(&pid).unwrap().cwd.clone();
                let full = Self::canon(&path, &cwd);
                self.walk(pid, &full, false)?;
                self.procs.get_mut(&pid).unwrap().cwd = full;
                Ok(Done::Now(ok(0)))
            }
            CLOSE => {
                let c = self.fdchk(pid, a[0])?;
                let p = self.procs.get(&pid).unwrap();
                p.fdt.borrow_mut().fds[a[0] as usize] = None;
                self.clunk(&c);
                Ok(Done::Now(ok(0)))
            }
            DUP => {
                let c = self.fdchk(pid, a[0])?;
                c.borrow_mut().refs += 1;
                let fd = if a[1] >= 0 {
                    self.fd_alloc(pid, c, Some(a[1] as usize))
                } else {
                    self.fd_alloc(pid, c, None)
                };
                Ok(Done::Now(ok(fd)))
            }
            OPEN => {
                let path = Self::txstr(tx, 0);
                let dn = self.walk(pid, &path, false)?;
                // '#d/N' (and posted names later): share the chan itself
                if let Node::DupFd(target) = &dn.node {
                    target.borrow_mut().refs += 1;
                    let fd = self.fd_alloc(pid, target.clone(), None);
                    return Ok(Done::Now(ok(fd)));
                }
                self.open_perm(&dn, a[1] as u32, pid)?;
                let cwd = self.procs.get(&pid).unwrap().cwd.clone();
                let chan = Rc::new(RefCell::new(Chan {
                    dev: dn.dev, node: dn.node, path: Some(Self::canon(&path, &cwd)),
                    mode: a[1] as u32, offset: 0, refs: 1,
                }));
                let fd = self.fd_alloc(pid, chan, None);
                Ok(Done::Now(ok(fd)))
            }
            CREATE => {
                let cpath = Self::txstr(tx, 0);
                let mode = a[1] as u32;
                let perm = a[2] as u32;
                let isdir = perm & DMDIR != 0;
                // create(2): an existing file (not a dir create) opens + truncates
                if !isdir {
                    if let Ok(dn) = self.walk(pid, &cpath, false) {
                        self.open_perm(&dn, mode | OTRUNC, pid)?;
                        if let Node::Ram(r) = &dn.node {
                            r.borrow_mut().data.clear();
                        }
                        let chan = Rc::new(RefCell::new(Chan {
                            dev: dn.dev, node: dn.node, path: Some(cpath.clone()),
                            mode, offset: 0, refs: 1,
                        }));
                        let fd = self.fd_alloc(pid, chan, None);
                        return Ok(Done::Now(ok(fd)));
                    }
                }
                let (parent, base) = self.walk_parent(pid, &cpath)?;
                let parent = match (&parent.dev, &parent.node) {
                    (DevId::Union, Node::Union(list)) => list
                        .iter()
                        .find(|e| e.create)
                        .map(|e| e.dn.clone())
                        .ok_or("create in a union needs an element bound with -c (MCREATE)")?,
                    _ => parent,
                };
                if let Node::EnvRoot = &parent.node {
                    // env vars are created into the walker's group
                    let p = self.procs.get(&pid).ok_or("no proc")?;
                    p.env.borrow_mut().insert(base.clone(), Vec::new());
                    let chan = Rc::new(RefCell::new(Chan {
                        dev: DevId::Env, node: Node::EnvVar(base), path: Some(cpath),
                        mode, offset: 0, refs: 1,
                    }));
                    let fd = self.fd_alloc(pid, chan, None);
                    return Ok(Done::Now(ok(fd)));
                }
                let umask = self.procs.get(&pid).unwrap().umask;
                let cred = self.procs.get(&pid).unwrap().cred.clone();
                let node = self.ram_create(&parent, &base, perm & !umask, isdir, &cred)?;
                let chan = Rc::new(RefCell::new(Chan {
                    dev: DevId::Ram, node: Node::Ram(node), path: Some(cpath),
                    mode, offset: 0, refs: 1,
                }));
                let fd = self.fd_alloc(pid, chan, None);
                Ok(Done::Now(ok(fd)))
            }
            REMOVE => {
                let path = Self::txstr(tx, 0);
                let (parent, base) = self.walk_parent(pid, &path)?;
                if let Node::EnvRoot = &parent.node {
                    let p = self.procs.get(&pid).ok_or("no proc")?;
                    p.env.borrow_mut().remove(&base);
                    return Ok(Done::Now(ok(0)));
                }
                let cred = self.procs.get(&pid).unwrap().cred.clone();
                if let Node::Ram(pr) = &parent.node {
                    self.ram_access(pr, &cred, 2)?;
                    let found = kid(pr, &base).ok_or_else(|| format!("'{}' does not exist", base))?;
                    if found.borrow().dir && !found.borrow().kids.is_empty() {
                        return Err("directory not empty".into());
                    }
                    pr.borrow_mut().kids.retain(|(k, _)| k != &base);
                    Ok(Done::Now(ok(0)))
                } else {
                    Err("remove not supported on this device".into())
                }
            }
            SEEK => {
                let c = self.fdchk(pid, a[0])?;
                let off = ((a[2] as i64) << 32) | (a[1] as u32 as i64);
                let len = self.dev_len(&c);
                let mut cb = c.borrow_mut();
                cb.offset = match a[3] {
                    0 => off as u64,
                    1 => (cb.offset as i64 + off) as u64,
                    _ => (len as i64 + off) as u64,
                };
                Ok(Done::Now(ok(cb.offset as i32)))
            }
            PREAD => {
                let c = self.fdchk(pid, a[0])?;
                let n = (a[2] as usize).min(TXSIZE);
                let cur = a[3] == -1 && a[4] == -1;
                let off = if cur { c.borrow().offset } else { ((a[4] as u32 as u64) << 32) | a[3] as u32 as u64 };
                match self.dev_read(&c, n, off, pid, reply)? {
                    Some(data) => {
                        if cur {
                            c.borrow_mut().offset += data.len() as u64;
                        }
                        Ok(Done::Now(okd(data.len() as i32, data)))
                    }
                    None => Ok(Done::Parked), // stream devices; offsets don't apply
                }
            }
            PWRITE => {
                let c = self.fdchk(pid, a[0])?;
                let n = (a[2] as usize).min(TXSIZE).min(tx.len());
                let cur = a[3] == -1 && a[4] == -1;
                let off = if cur { c.borrow().offset } else { ((a[4] as u32 as u64) << 32) | a[3] as u32 as u64 };
                let wrote = self.dev_write(&c, &tx[..n], off, pid)?;
                if cur {
                    c.borrow_mut().offset += wrote as u64;
                }
                Ok(Done::Now(ok(wrote as i32)))
            }
            STAT => {
                let path = Self::txstr(tx, 0);
                let dn = self.walk(pid, &path, a[3] == 1)?; // a3: lstat's nofollow
                let rec = self.dev_stat(&dn, pid)?;
                let n = rec.len() as i32;
                Ok(Done::Now(okd(n, rec)))
            }
            FSTAT => {
                let c = self.fdchk(pid, a[0])?;
                let dn = {
                    let cb = c.borrow();
                    DN { dev: cb.dev, node: cb.node.clone(), path: None }
                };
                let rec = self.dev_stat(&dn, pid)?;
                let n = rec.len() as i32;
                Ok(Done::Now(okd(n, rec)))
            }
            ERRSTR => {
                // exchange, per errstr(2)
                let newe = Self::txstr(tx, 0);
                let p = self.procs.get_mut(&pid).ok_or("no proc")?;
                let olde = std::mem::replace(&mut p.errstr, newe);
                let cap = (a[1] as usize).max(1);
                let mut bytes: Vec<u8> = olde.into_bytes();
                bytes.truncate(cap - 1);
                bytes.push(0);
                Ok(Done::Now(okd(bytes.len() as i32 - 1, bytes)))
            }
            FD2PATH => {
                let c = self.fdchk(pid, a[0])?;
                let path = c.borrow().path.clone().unwrap_or_default();
                let mut bytes = path.into_bytes();
                bytes.truncate((a[2] as usize).saturating_sub(1));
                bytes.push(0);
                Ok(Done::Now(okd(bytes.len() as i32 - 1, bytes)))
            }
            NSEC => Ok(Done::Now(okd(8, now_nanos().to_le_bytes().to_vec()))),
            SLEEP => {
                let ms = a[0].max(0) as u64;
                if ms == 0 {
                    return Ok(Done::Now(ok(0)));
                }
                let token = self.next_token;
                self.next_token += 1;
                self.sleep_waiters.insert(token, reply.clone());
                self.effects.push(Effect::Timer { ms, token });
                Ok(Done::Parked)
            }
            PIPE => {
                let p = Rc::new(RefCell::new(Pipe {
                    q: [VecDeque::new(), VecDeque::new()],
                    nbytes: [0, 0],
                    refs: [1, 1],
                    parked: [Vec::new(), Vec::new()],
                }));
                let mk = |end: usize| {
                    Rc::new(RefCell::new(Chan {
                        dev: DevId::Pipe, node: Node::Pipe { p: p.clone(), end },
                        path: Some("#|/data".into()), mode: 2, offset: 0, refs: 1,
                    }))
                };
                let c0 = mk(0);
                let c1 = mk(1);
                let fd0 = self.fd_alloc(pid, c0, None);
                let fd1 = self.fd_alloc(pid, c1, None);
                let mut data = Vec::with_capacity(8);
                data.extend_from_slice(&fd0.to_le_bytes());
                data.extend_from_slice(&fd1.to_le_bytes());
                Ok(Done::Now(okd(0, data)))
            }
            ARGS => {
                let p = self.procs.get(&pid).ok_or("no proc")?;
                let mut block = Vec::new();
                for s in &p.argv {
                    block.extend_from_slice(s.as_bytes());
                    block.push(0);
                }
                block.truncate(a[1] as usize);
                Ok(Done::Now(okd(block.len() as i32, block)))
            }
            RFORK => self.rfork(worker_pid, pid, a[0], a[2]),
            EXEC => self.exec_call(worker_pid, pid, tx, a[2]),
            EXITS => self.exits(worker_pid, pid, tx),
            AWAIT => {
                let p = self.procs.get_mut(&pid).ok_or("no proc")?;
                if a[2] == 1 && p.zombies.is_empty() {
                    return Ok(Done::Now(ok(0))); // nohang, nothing yet
                }
                let max = a[1] as usize;
                if !p.zombies.is_empty() {
                    let s = p.zombies.remove(0);
                    let mut bytes = s.into_bytes();
                    bytes.truncate(max.saturating_sub(1));
                    bytes.push(0);
                    let n = bytes.len() as i32 - 1;
                    return Ok(Done::Now(okd(n, bytes)));
                }
                p.await_reply = Some((reply.clone(), max));
                Ok(Done::Parked)
            }
            LINK => {
                let old = Self::txstr(tx, 0);
                let nu = Self::txstr(tx, old.len() + 1);
                let o = self.walk(pid, &old, true)?; // link the name given, not its target
                let (parent, base) = self.walk_parent(pid, &nu)?;
                let cred = self.procs.get(&pid).unwrap().cred.clone();
                match (&parent.node, &o.node) {
                    (Node::Ram(pr), Node::Ram(onode)) => {
                        self.ram_access(pr, &cred, 2)?;
                        if kid(pr, &base).is_some() {
                            return Err(format!("'{}' already exists", base));
                        }
                        if onode.borrow().dir {
                            return Err("cannot hard-link a directory".into());
                        }
                        pr.borrow_mut().kids.push((base, onode.clone()));
                        Ok(Done::Now(ok(0)))
                    }
                    _ => Err("link not supported on this device".into()),
                }
            }
            SYMLINK => {
                let target = Self::txstr(tx, 0);
                let nu = Self::txstr(tx, target.len() + 1);
                let (parent, base) = self.walk_parent(pid, &nu)?;
                let cred = self.procs.get(&pid).unwrap().cred.clone();
                if let Node::Ram(pr) = &parent.node {
                    self.ram_access(pr, &cred, 2)?;
                    if kid(pr, &base).is_some() {
                        return Err(format!("'{}' already exists", base));
                    }
                    let q = self.qgen;
                    self.qgen += 1;
                    let node = Rc::new(RefCell::new(RNode {
                        name: base.clone(), qpath: q, dir: false, data: Vec::new(),
                        kids: Vec::new(), uid: cred.euid.clone(), mode: 0o777,
                        atime: now_secs(), mtime: now_secs(), symlink: Some(target),
                    }));
                    pr.borrow_mut().kids.push((base, node));
                    Ok(Done::Now(ok(0)))
                } else {
                    Err("symlink not supported on this device".into())
                }
            }
            READLINK => {
                let path = Self::txstr(tx, 0);
                let dn = self.walk(pid, &path, true)?;
                if let Node::Ram(r) = &dn.node {
                    if let Some(t) = r.borrow().symlink.clone() {
                        let mut bytes = t.into_bytes();
                        let n = bytes.len() as i32;
                        bytes.push(0);
                        return Ok(Done::Now(okd(n, bytes)));
                    }
                    return Err("not a symlink".into());
                }
                Err("readlink not supported on this device".into())
            }
            WSTAT => {
                let path = Self::txstr(tx, 0);
                let rec = &tx[path.len() + 1..(path.len() + 1 + a[2] as usize).min(tx.len())];
                let st = parse_stat(rec).ok_or("bad stat record")?;
                let (parent, base) = self.walk_parent(pid, &path)?;
                let dn = self.walk(pid, &path, true)?;
                let cred = self.procs.get(&pid).ok_or("no proc")?.cred.clone();
                let Node::Ram(node) = &dn.node else {
                    return Err("wstat not supported on this device".into());
                };
                if st.mode != 0xffff_ffff {
                    if cred.euid != self.eve && cred.euid != node.borrow().uid {
                        return Err(format!("not owner of '{}'", node.borrow().name));
                    }
                    node.borrow_mut().mode = st.mode & (0o7777 | 0x000C_0000);
                }
                if !st.uid.is_empty() {
                    if cred.euid != self.eve {
                        return Err("only the host owner may chown (docs/uid.md D3)".into());
                    }
                    node.borrow_mut().uid = st.uid.clone();
                }
                if !st.name.is_empty() && st.name != base {
                    if cred.euid != self.eve && cred.euid != node.borrow().uid {
                        return Err(format!("not owner of '{}'", node.borrow().name));
                    }
                    if let Node::Ram(pr) = &parent.node {
                        if kid(pr, &st.name).is_some() {
                            return Err(format!("'{}' already exists", st.name));
                        }
                        pr.borrow_mut().kids.retain(|(k, _)| k != &base);
                        node.borrow_mut().name = st.name.clone();
                        pr.borrow_mut().kids.push((st.name.clone(), node.clone()));
                        pr.borrow_mut().mtime = now_secs();
                    }
                }
                if st.mtime != 0xffff_ffff {
                    node.borrow_mut().mtime = st.mtime;
                }
                Ok(Done::Now(ok(0)))
            }
            FWSTAT => {
                let c = self.fdchk(pid, a[0])?;
                let rec = &tx[..(a[2] as usize).min(tx.len())];
                let st = parse_stat(rec).ok_or("bad stat record")?;
                let cred = self.procs.get(&pid).ok_or("no proc")?.cred.clone();
                let node = {
                    let cb = c.borrow();
                    match &cb.node {
                        Node::Ram(r) => r.clone(),
                        _ => return Err("wstat not supported on this device".into()),
                    }
                };
                if st.mode != 0xffff_ffff {
                    if cred.euid != self.eve && cred.euid != node.borrow().uid {
                        return Err(format!("not owner of '{}'", node.borrow().name));
                    }
                    node.borrow_mut().mode = st.mode & (0o7777 | 0x000C_0000);
                }
                if !st.uid.is_empty() {
                    if cred.euid != self.eve {
                        return Err("only the host owner may chown (docs/uid.md D3)".into());
                    }
                    node.borrow_mut().uid = st.uid.clone();
                }
                Ok(Done::Now(ok(0)))
            }
            UNMOUNT => {
                let name = Self::txstr(tx, 0);
                let old = Self::txstr(tx, name.len() + 1);
                if self.procs.get(&pid).ok_or("no proc")?.nomnt {
                    return Err("mounting disallowed (RFNOMNT)".into());
                }
                let cwd = self.procs.get(&pid).unwrap().cwd.clone();
                let key = Self::canon(&old, &cwd);
                let ns = self.procs.get(&pid).unwrap().ns.clone();
                let have = ns.borrow().get(&key).cloned();
                let Some(mut list) = have else {
                    return Err(format!("'{}' is not a mount point", key));
                };
                if name.is_empty() {
                    // unmount(nil, old): the whole point
                    ns.borrow_mut().remove(&key);
                    return Ok(Done::Now(ok(0)));
                }
                let nm = if name.starts_with('#') { name.clone() } else { Self::canon(&name, &cwd) };
                let mut i = list.iter().position(|el| el.dn.path.as_deref() == Some(nm.as_str()));
                if i.is_none() {
                    let src = self.walk(pid, &name, false)?;
                    i = list.iter().position(|el| el.dn.dev == src.dev && node_eq(&el.dn.node, &src.node));
                }
                let Some(i) = i else {
                    return Err(format!("'{}' is not bound at '{}'", name, key));
                };
                list.remove(i);
                if list.is_empty() {
                    ns.borrow_mut().remove(&key);
                } else {
                    ns.borrow_mut().insert(key, list);
                }
                Ok(Done::Now(ok(0)))
            }
            NOTIFY | NOTED | ALARM | NOTEGET => {
                // the note machinery is a later tranche; rc's suite needs it,
                // the early conformance run does not
                Err(format!("trap {} not in the native tranche yet", trap))
            }
            _ => Err(format!("bad syscall {} (native v1)", trap)),
        }
    }

    fn open_perm(&mut self, dn: &DN, mode: u32, pid: Pid) -> Result<(), KErr> {
        if let Node::Ram(r) = &dn.node {
            let cred = self.procs.get(&pid).ok_or("no proc")?.cred.clone();
            let rw = mode & 3;
            let mut want = match rw {
                1 => 2,
                2 => 6,
                _ => 4,
            };
            if mode & OTRUNC != 0 {
                want |= 2;
            }
            self.ram_access(r, &cred, want)?;
            if mode & OTRUNC != 0 && !r.borrow().dir {
                r.borrow_mut().data.clear();
            }
        }
        Ok(())
    }

    fn ram_create(&mut self, parent: &DN, name: &str, perm: u32, isdir: bool,
                  cred: &Cred) -> Result<RamRef, KErr> {
        let pr = match &parent.node {
            Node::Ram(r) => r.clone(),
            _ => return Err("create not supported on this device".into()),
        };
        if !pr.borrow().dir {
            return Err("create in non-directory".into());
        }
        self.ram_access(&pr, cred, 2)?;
        if let Some(old) = kid(&pr, name) {
            if old.borrow().dir || isdir {
                return Err(format!("'{}' already exists", name));
            }
            self.ram_access(&old, cred, 2)?;
            old.borrow_mut().data.clear();
            return Ok(old);
        }
        let q = self.qgen;
        self.qgen += 1;
        let node = Rc::new(RefCell::new(RNode {
            name: name.into(), qpath: q, dir: isdir, data: Vec::new(),
            kids: Vec::new(), uid: cred.euid.clone(), mode: perm & 0o7777,
            atime: now_secs(), mtime: now_secs(), symlink: None,
        }));
        pr.borrow_mut().kids.push((name.into(), node.clone()));
        pr.borrow_mut().mtime = now_secs();
        Ok(node)
    }

    // ---- rfork / exec / exits (the lifecycle, ported shape for shape) ----
    fn rfork(&mut self, worker_pid: Pid, pid: Pid, flags: i32, marker: i32) -> Result<Done, KErr> {
        if flags & rf::PROC == 0 {
            // flag changes on self
            let (ns2, env2) = {
                let p = self.procs.get(&pid).ok_or("no proc")?;
                let ns2 = if flags & rf::CNAMEG != 0 {
                    Some(Rc::new(RefCell::new(HashMap::new())))
                } else if flags & rf::NAMEG != 0 {
                    Some(Rc::new(RefCell::new(p.ns.borrow().clone())))
                } else {
                    None
                };
                let env2 = if flags & rf::CENVG != 0 {
                    Some(Rc::new(RefCell::new(HashMap::new())))
                } else if flags & rf::ENVG != 0 {
                    Some(Rc::new(RefCell::new(p.env.borrow().clone())))
                } else {
                    None
                };
                (ns2, env2)
            };
            if flags & rf::CFDG != 0 {
                let old = self.procs.get(&pid).unwrap().fdt.clone();
                self.fdt_close(&old);
                self.procs.get_mut(&pid).unwrap().fdt = new_fdt();
            }
            let group = if flags & rf::NOTEG != 0 {
                let g = self.next_note_group;
                self.next_note_group += 1;
                Some(g)
            } else {
                None
            };
            let p = self.procs.get_mut(&pid).unwrap();
            if let Some(ns) = ns2 {
                p.ns = ns;
            }
            if let Some(env) = env2 {
                p.env = env;
            }
            if let Some(g) = group {
                p.note_group = g;
            }
            if flags & rf::NOMNT != 0 {
                p.nomnt = true;
            }
            return Ok(Done::Now(ok(0)));
        }
        if self.procs.get(&worker_pid).and_then(|p| p.borrower).is_some() {
            return Err("nested lazy fork unsupported in v0".into());
        }
        struct Bits {
            ns: NsR,
            env: EnvR,
            cwd: String,
            cred: Cred,
            group: Option<u32>,
            nomnt: bool,
            image: Option<Arc<Vec<u8>>>,
            asyncified: bool,
        }
        let bits = {
            let p = self.procs.get(&pid).ok_or("no proc")?;
            Bits {
                ns: if flags & rf::CNAMEG != 0 {
                    Rc::new(RefCell::new(HashMap::new()))
                } else if flags & rf::NAMEG != 0 {
                    Rc::new(RefCell::new(p.ns.borrow().clone()))
                } else {
                    p.ns.clone() // SHARED, per rfork(2) — /bin/bind depends on it
                },
                env: if flags & rf::CENVG != 0 {
                    Rc::new(RefCell::new(HashMap::new()))
                } else if flags & rf::ENVG != 0 {
                    Rc::new(RefCell::new(p.env.borrow().clone()))
                } else {
                    p.env.clone()
                },
                cwd: p.cwd.clone(),
                cred: p.cred.clone(),
                group: if flags & rf::NOTEG != 0 { None } else { Some(p.note_group) },
                nomnt: p.nomnt,
                image: p.image.clone(),
                asyncified: p.asyncified,
            }
        };
        let fdt = {
            let p = self.procs.get(&pid).unwrap();
            if flags & rf::CFDG != 0 {
                new_fdt()
            } else if flags & rf::FDG != 0 {
                let f = p.fdt.clone();
                self.fdt_copy(&f)
            } else {
                let f = p.fdt.clone();
                f.borrow_mut().refs += 1;
                f
            }
        };
        if marker == 2 {
            // asyncify: bare dual return
            if !bits.asyncified {
                return Err("not an asyncify build — add it to ASYNCIFY in poc/mk.sh, or use procrfork".into());
            }
            if flags & rf::MEM != 0 {
                return Err("RFMEM is the lazy path's flag; a bare fork copies".into());
            }
            let child = self.new_proc(pid, bits.ns, fdt, bits.cwd, bits.cred, bits.env, bits.group);
            let c = self.procs.get_mut(&child).unwrap();
            c.nomnt = bits.nomnt || flags & rf::NOMNT != 0;
            c.nowait = flags & rf::NOWAIT != 0;
            c.image = bits.image;
            c.asyncified = true;
            return Ok(Done::Now(ok(child as i32))); // one return; the runner makes two
        }
        if marker != 1 {
            return Err("bare rfork(RFPROC) needs an asyncify build; procrfork is the exec path".into());
        }
        if flags & rf::MEM == 0 {
            return Err("plain fork needs asyncify — the guard path is lazy".into());
        }
        let child = self.new_proc(pid, bits.ns, fdt, bits.cwd, bits.cred, bits.env, bits.group);
        {
            let c = self.procs.get_mut(&child).unwrap();
            c.nomnt = bits.nomnt || flags & rf::NOMNT != 0;
            c.nowait = flags & rf::NOWAIT != 0;
        }
        self.procs.get_mut(&worker_pid).unwrap().borrower = Some(child);
        Ok(Done::Now(KReply {
            ret: 0, aux: child as i32, data: Vec::new(), action: KAction::None, load: None,
        }))
    }

    // the runner (guest thread) tells us a bare fork unwound: spawn the child
    pub fn asyfork(&mut self, parent_pid: Pid, child_pid: Pid, snap: Vec<u8>, data_ptr: u32, sp: u32) {
        let argv = self.procs.get(&parent_pid).map(|p| p.argv.clone()).unwrap_or_default();
        if let Some(c) = self.procs.get_mut(&child_pid) {
            c.argv = argv.clone();
            if let Some(image) = c.image.clone() {
                self.effects.push(Effect::Spawn {
                    pid: child_pid, image, argv,
                    asy: Some(AsySnap { snap, data_ptr, sp }),
                });
            }
        }
    }

    fn exec_call(&mut self, worker_pid: Pid, pid: Pid, tx: &[u8], argc: i32) -> Result<Done, KErr> {
        let path = Self::txstr(tx, 0);
        let mut argv = Vec::new();
        let mut o = path.len() + 1;
        for _ in 0..argc {
            let s = Self::txstr(tx, o);
            o += s.len() + 1;
            argv.push(s);
        }
        let dn = self.walk(pid, &path, false)?;
        // directory and setuid checks, from the stat record
        if let Ok(rec) = self.dev_stat(&dn, pid) {
            if let Some(st) = parse_stat(&rec) {
                if st.mode & DMDIR != 0 {
                    return Err(format!("'{}' is a directory", path));
                }
                if st.mode & DMSETUID != 0 {
                    let p = self.procs.get_mut(&pid).unwrap();
                    p.cred.euid = st.uid.clone();
                }
            }
        }
        let image = self.read_all(&dn)?;
        if image.len() < 4 || image[0..4] != [0x00, 0x61, 0x73, 0x6d] {
            return Err(format!("'{}' exec format error", path));
        }
        let image = Arc::new(image);
        let argv = if argv.is_empty() { vec![path.clone()] } else { argv };
        {
            let p = self.procs.get_mut(&pid).unwrap();
            p.argv = argv.clone();
            p.image = Some(image.clone());
        }
        let is_borrowed = self.procs.get(&worker_pid).and_then(|p| p.borrower) == Some(pid);
        if is_borrowed {
            // lazy-fork child leaves the borrowed stack for its own thread;
            // the parent's runner restores [0,sp) and unwinds to the guard
            self.procs.get_mut(&worker_pid).unwrap().borrower = None;
            self.effects.push(Effect::Spawn { pid, image, argv, asy: None });
            return Ok(Done::Now(KReply {
                ret: -1000, aux: pid as i32, data: Vec::new(),
                action: KAction::ForkResume, load: None,
            }));
        }
        // exec-in-place: the reply hands this thread its new image
        Ok(Done::Now(KReply {
            ret: -1001, aux: 0, data: Vec::new(), action: KAction::ExecSelf,
            load: Some((image, argv)),
        }))
    }

    fn exits(&mut self, worker_pid: Pid, pid: Pid, tx: &[u8]) -> Result<Done, KErr> {
        let msg = Self::txstr(tx, 0);
        let (ppid, nowait, fdt) = {
            let p = self.procs.get(&pid).ok_or("no proc")?;
            (p.ppid, p.nowait, p.fdt.clone())
        };
        self.procs.remove(&pid);
        self.fdt_close(&fdt); // pipes learn their EOF here
        let was_borrowed = self.procs.get(&worker_pid).and_then(|p| p.borrower) == Some(pid);
        if was_borrowed {
            self.procs.get_mut(&worker_pid).unwrap().borrower = None;
            self.zombie(ppid, pid, &msg, nowait);
            return Ok(Done::Now(KReply {
                ret: -1000, aux: pid as i32, data: Vec::new(),
                action: KAction::ForkResume, load: None,
            }));
        }
        if pid == 1 {
            self.effects.push(Effect::Shutdown(if msg.is_empty() { 0 } else { 1 }));
            return Ok(Done::Now(KReply {
                ret: -3000, aux: 0, data: Vec::new(), action: KAction::Die, load: None,
            }));
        }
        self.zombie(ppid, pid, &msg, nowait);
        Ok(Done::Now(KReply {
            ret: -2000, aux: 0, data: Vec::new(), action: KAction::Retire, load: None,
        }))
    }
}

enum WalkRes {
    Hit(DN),
    Redirect(String),
}
