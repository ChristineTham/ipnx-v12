// The kernel core in Rust — a structural port of supervisor/kernel.mjs (the
// reference implementation; the 130-test guest suite is the conformance
// spec). Same shapes throughout: proc table, per-process namespaces as
// mount maps with longest-prefix walk, refcounted channels, union lists,
// parked device reads — and, like the reference, ASYNC THROUGHOUT: devmnt
// suspends in the middle of a walk while an R-message crosses a pipe, so
// dispatch runs as tasks on the kernel's own single-threaded executor
// (exec.rs; no tokio). The kernel remains a pure state machine to its host:
// syscalls in through `syscall`, replies out through per-call senders, and
// everything platform-bound leaves as an `Effect`.

pub mod draw;
pub mod exec;
pub mod stat9;

use exec::{oneshot, Completer, LocalExec};

// wasm32 has no stderr: shadow eprintln! so diagnostics vanish instead
// of panicking (the embedding provides its own logging).
#[cfg(target_arch = "wasm32")]
macro_rules! eprintln {
    ($($t:tt)*) => {{ let _ = format_args!($($t)*); }};
}
use stat9::{marshal_stat, parse_stat, StatIn, DMDIR, DMSETUID, DMSYMLINK, QTDIR, QTFILE, QTSYMLINK};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
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
    pub const MOUNT: i32 = 46;
    pub const AWAIT: i32 = 47;
    pub const PREAD: i32 = 50;
    pub const PWRITE: i32 = 51;
    pub const NSEC: i32 = 53;
    pub const LINK: i32 = 60;
    pub const SYMLINK: i32 = 61;
    pub const READLINK: i32 = 62;
    pub const ARGS: i32 = 200;
    pub const NOTEGET: i32 = 202;
    pub const AREAD: i32 = 210;
    pub const IOWAIT: i32 = 211;
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

#[derive(Clone)]
pub struct CvSnap {
    pub id: u32,
    pub kind: u8, // 0 stack, 1 text, 2 edit, 3 path
    pub attrs: Vec<(String, String)>,
    pub data: Vec<u8>,
}

// '#Z' host-file operations, delegated to the embedding: the kernel
// speaks root-relative paths; the host owns the real root and the
// canonicalize-under-root security check. Answered via hostop_done —
// webfs's fetch_done pattern, applied to the filesystem.
pub enum HostOp {
    Meta { path: String },
    Read { path: String, off: u64, n: usize },
    Write { path: String, off: u64, data: Vec<u8> },
    Create { path: String, dir: bool, perm: u32 },
    Remove { path: String },
    Trunc { path: String },
    ReadDir { path: String },
}
pub struct HostEnt {
    pub name: String,
    pub dir: bool,
    pub len: u64,
    pub mtime: u32,
    pub ino: u64,
    pub mode: u32,
}
pub enum HostReply {
    Missing,
    Meta { dir: bool, len: u64, mtime: u32, ino: u64, mode: u32 },
    Bytes(Vec<u8>),
    Entries(Vec<HostEnt>),
    Unit,
}

pub enum Effect {
    Spawn { pid: Pid, image: Arc<Vec<u8>>, argv: Vec<String>, asy: Option<AsySnap> },
    // M3: the presentation layer's feed — a window's fresh pixels (r8g8b8a8,
    // w*h*4) or its departure. The host may show them, log them, or drop
    // them; the kernel never knows there is a screen.
    WinUpdate { wid: u32, label: String, x: i32, y: i32, w: i32, h: i32, rgba: Vec<u8> },
    WinGone { wid: u32 },
    WinText { wid: u32, bytes: Vec<u8> },
    // /dev/canvas sync: the window's semantic tree, for a native presenter.
    // Travels ONLY through the credit system (win_dirty/win_inflight/ack) —
    // coalescing changes the rate, only credit changes the bound, and a
    // snapshot built at flush time coalesces by construction.
    WinCanvas { wid: u32, label: String, x: i32, y: i32, w: i32, h: i32, snap: Vec<CvSnap> },
    // /dev/window: the chrome IPNX declared, for the host to render natively.
    // Small and rare (unlike WinCanvas), so it rides outside the credit system.
    WinChrome { wid: u32, wtype: String, content: String, toolbar: String, tag: String },
    // the SURFACE opened a file. In-process this is a function call, not
    // marshalled 9P — "wire 9P at boundaries, a Dev table inside" (design.md).
    // A remote surface marshals; a surface sharing the address space calls.
    ReadDone { token: f64, ok: bool, data: Vec<u8> },
    // Put: the surface streams the edited file back. An ordinary write.
    WriteDone { token: f64, ok: bool, n: u32 },
    // '#H': the host performs the GET off-thread and answers with FetchDone
    Fetch { url: String },
    // /dev/snarf: the host clipboard hears writes; reads ask it first
    SnarfSet { text: String },
    SnarfGet,
    ConsWrite(Vec<u8>),
    Timer { ms: u64, token: u64 },
    Host { tag: u64, op: HostOp },
    Shutdown(i32),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum KAction {
    None,
    ForkResume,
    ExecSelf,
    Retire,
    Die,
}

pub struct KReply {
    pub ret: i32,
    pub aux: i32,
    pub data: Vec<u8>,
    pub action: KAction,
    pub load: Option<(Arc<Vec<u8>>, Vec<String>)>,
    pub note_pending: bool,
}

fn ok(ret: i32) -> KReply {
    KReply { ret, aux: 0, data: Vec::new(), action: KAction::None, load: None, note_pending: false }
}
fn okd(ret: i32, data: Vec<u8>) -> KReply {
    KReply { ret, aux: 0, data, action: KAction::None, load: None, note_pending: false }
}

type KErr = String;
type KRes = Result<KReply, KErr>;

// a parked read completes with data, or an interrupt (postnote's doing)
pub enum RRes {
    Data(Vec<u8>),
    Intr,
}

// ---- channels ----
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
    Host,
    Srv,
    Web,
    Cons,
    Pipe,
    Dup,
    Env,
    Union,
    Proc,
    Mnt,
    Wsys,
    Snap,
}

#[derive(Clone)]
enum Node {
    Ram(RamRef),
    Host(std::path::PathBuf),
    SrvRoot,
    SrvName(String),
    WebRoot,
    Web(String),
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
    Proc { kind: u8, pid: Pid }, // 0 root, 1 dir, 2 ctl, 3 status, 4 note, 5 notepg
    Mnt(MntRef),
    Wsys { kind: WKind, win: Option<WinR>, conn: Option<DConnR>, ty: Option<String> },
    Cv { win: WinR, id: u32, file: u8 }, // canvas node: file 255 dir, 0 kind, 1 attrs, 2 addr, 3 data
    SnapRoot,
    SnapCtl,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum WKind {
    Root,
    Clone,
    WinDir,
    Cons,
    Mouse,
    Wctl,
    Winid,
    Label,
    Rgb,
    Consctl,
    Cursor,
    DrawDir,
    DrawNew,
    ConnDir,
    DrawCtl,
    DrawData,
    DrawRefresh,
    CvDir,
    CvCtl,
    CvEvents,
    // /dev/window (docs/window.md): the control interface
    TypeDir,
    RootEvents,
    WContent,
    WKids,         // the children, as a walkable directory
    WAlloc,        // allocated, or a tab
    WAxis,         // how this window arranges them: row, col, or nothing
    WToolbar,
    WTag,
    WUi,
    WEvents,
    CvCaps,
    Snarf,
}

fn node_eq(a: &Node, b: &Node) -> bool {
    match (a, b) {
        (Node::Ram(x), Node::Ram(y)) => Rc::ptr_eq(x, y),
        (Node::Cv { win: a, id: i1, file: f1 }, Node::Cv { win: b, id: i2, file: f2 }) =>
            Rc::ptr_eq(a, b) && i1 == i2 && f1 == f2,
        (Node::SnapRoot, Node::SnapRoot) => true,
        (Node::SnapCtl, Node::SnapCtl) => true,
        (Node::Host(x), Node::Host(y)) => x == y,
        (Node::ConsRoot, Node::ConsRoot)
        | (Node::ConsCons, Node::ConsCons)
        | (Node::ConsUser, Node::ConsUser)
        | (Node::ConsPid, Node::ConsPid)
        | (Node::ConsNull, Node::ConsNull)
        | (Node::DupRoot, Node::DupRoot)
        | (Node::EnvRoot, Node::EnvRoot) => true,
        (Node::Pipe { p: x, end: e1 }, Node::Pipe { p: y, end: e2 }) => Rc::ptr_eq(x, y) && e1 == e2,
        (Node::EnvVar(x), Node::EnvVar(y)) => x == y,
        (Node::Mnt(x), Node::Mnt(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}

#[derive(Clone)]
struct DN {
    dev: DevId,
    node: Node,
    path: Option<String>, // what bind(2) recorded; unmount matches by it
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
    data: Rc<Vec<u8>>, // Rc so a snapshot shares bytes; Rc::make_mut copies on first write
    kids: Vec<(String, RamRef)>, // insertion order, like the JS Map
    uid: String,
    mode: u32,
    atime: u32,
    mtime: u32,
    symlink: Option<String>,
    ro: bool, // a snapshot node: every write path refuses through ram_access
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

#[derive(Clone)]
enum WaitKind {
    Wake { pid: Pid, c: Completer<RRes> },
    Aread { pid: Pid, tag: u32 },
}

struct Waiter {
    n: usize,
    kind: WaitKind,
}

enum TimerKind {
    Sleep(Completer<RRes>),
    Alarm(Pid),
    Iowait(Pid),
}

// ---- devmnt: the ONE place the kernel marshals wire 9P ----
const MSIZE: usize = 8216; // 8192 data + IOHDRSZ(24)
const NOFID: u32 = 0xffff_ffff;
mod tv {
    pub const VERSION: u8 = 100;
    pub const ATTACH: u8 = 104;
    pub const RERROR: u8 = 107;
    pub const WALK: u8 = 110;
    pub const OPEN: u8 = 112;
    pub const CREATE: u8 = 114;
    pub const READ: u8 = 116;
    pub const WRITE: u8 = 118;
    pub const CLUNK: u8 = 120;
    pub const REMOVE: u8 = 122;
    pub const STAT: u8 = 124;
    pub const WSTAT: u8 = 126;
    // V12 extension messages, minted in the unused >127 range
    pub const LINK: u8 = 128;
    pub const SYMLINK: u8 = 130;
    pub const READLINK: u8 = 132;
}

struct ConnSt {
    chan: ChanR, // the transport (incref'd by mount)
    tags: HashMap<u16, Completer<Result<Vec<u8>, KErr>>>,
    expect: HashMap<u16, u8>,
    nexttag: u16,
    nextfid: u32,
    dead: Option<KErr>,
}
type ConnR = Rc<RefCell<ConnSt>>;

struct MntNode {
    conn: ConnR,
    fid: u32,
    qtype: u8,
    ephemeral: std::cell::Cell<bool>,
    opened: std::cell::Cell<bool>,
}
type MntRef = Rc<MntNode>;

struct W9(Vec<u8>);
impl W9 {
    fn new() -> W9 {
        W9(Vec::new())
    }
    fn u8(mut self, v: u8) -> W9 {
        self.0.push(v);
        self
    }
    fn u16(mut self, v: u16) -> W9 {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn u32(mut self, v: u32) -> W9 {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn u64(mut self, v: u64) -> W9 {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn s(mut self, s: &str) -> W9 {
        self.0.extend_from_slice(&(s.len() as u16).to_le_bytes());
        self.0.extend_from_slice(s.as_bytes());
        self
    }
    fn raw(mut self, b: &[u8]) -> W9 {
        self.0.extend_from_slice(b);
        self
    }
    fn frame(self, ty: u8, tag: u16) -> Vec<u8> {
        let mut out = Vec::with_capacity(7 + self.0.len());
        out.extend_from_slice(&((7 + self.0.len()) as u32).to_le_bytes());
        out.push(ty);
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&self.0);
        out
    }
}

struct R9<'a> {
    b: &'a [u8],
    o: usize,
}
impl<'a> R9<'a> {
    fn new(b: &'a [u8]) -> R9<'a> {
        R9 { b, o: 0 }
    }
    fn u8(&mut self) -> u8 {
        let v = self.b[self.o];
        self.o += 1;
        v
    }
    fn u16(&mut self) -> u16 {
        let v = u16::from_le_bytes(self.b[self.o..self.o + 2].try_into().unwrap());
        self.o += 2;
        v
    }
    fn u32(&mut self) -> u32 {
        let v = u32::from_le_bytes(self.b[self.o..self.o + 4].try_into().unwrap());
        self.o += 4;
        v
    }
    fn s(&mut self) -> String {
        let n = self.u16() as usize;
        let v = String::from_utf8_lossy(&self.b[self.o..self.o + n]).into_owned();
        self.o += n;
        v
    }
    fn qid(&mut self) -> (u8, u32, u64) {
        let ty = self.u8();
        let vers = self.u32();
        let path = u64::from_le_bytes(self.b[self.o..self.o + 8].try_into().unwrap());
        self.o += 8;
        (ty, vers, path)
    }
    fn rest(&self) -> &'a [u8] {
        &self.b[self.o..]
    }
}

// ---- devwsys: '#w' — the window server's kernel half (rio's INTERFACE).
// Reading clone mints a window; a window is a directory of cons, mouse,
// wctl, winid, label, rgb — and draw/, an actual per-window /dev/draw.
// Native v1 is headless: no host presentation effects; the suite reads
// rasters back through rgb and injects input through wctl. ----
// /dev/canvas (docs/canvas.md): a flat tree of semantic nodes per window.
// v0 kinds stack/text/edit/path; addr/data is acme's interface simplified;
// events are %-quoted lines with parked reads. A user edit event is a
// mutation that happened — cv_apply keeps node data the truth.
struct CvNode {
    kind: u8, // 0 stack, 1 text, 2 edit, 3 path
    attrs: Vec<(String, String)>,
    data: Vec<u8>,
    addr: (usize, usize),
}

struct Canvas {
    nodes: HashMap<u32, CvNode>,
    order: u32,
    events: VecDeque<String>,
    parked: Vec<Waiter>,
}

struct Win {
    wid: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    label: String,
    img: draw::ImageR,
    conns: HashMap<u32, DConnR>,
    nextconn: u32,
    consbuf: Vec<u8>,
    consparked: Vec<Waiter>,
    uitext: Vec<u8>,
    mousebuf: VecDeque<Vec<u8>>,
    mouseparked: Vec<Waiter>,
    cv: Option<Canvas>,
    dead: bool,
    // THE TREE (emca.txt PART FOUR): a window contains either a body or
    // child windows. `axis` says how it arranges those children — row means
    // side by side, col means stacked — and it is 0 for a window holding a
    // body. Alternation is the invariant: a container's axis is always
    // perpendicular to its parent's, which is what keeps one layout to one
    // tree (see split() below).
    parent: Option<u32>,
    kids: Vec<u32>,
    axis: u8,              // 0 none, 1 row (side by side), 2 col (stacked)
    // ALLOCATION: a parent gives rectangles to some of its children; the rest
    // appear as tabs. So a tab is not a reduced window — it is a whole window
    // the parent has not allocated to. minimise(me) moves me out of the
    // allocation, maximise(me) moves everyone else out, and `premax` is what
    // makes the second reversible.
    allocated: bool,
    premax: Option<Vec<u32>>,
    // /dev/window: the control interface's per-window files
    wtype: String,
    content: String,
    toolbar: String,
    tag: String,
    uifile: String,
    wevents: VecDeque<String>,
    wparked: Vec<Waiter>,
}
type WinR = Rc<RefCell<Win>>;

// srv(3): a posted channel kept alive by name — create /srv/x, write the fd
// number; opening the name later SHARES the channel itself (JS parity)
enum WebState {
    Pending,
    Done(Result<Vec<u8>, String>),
}
struct WebEntry {
    state: WebState,
    waiters: Vec<(usize, u64, Completer<RRes>)>, // (n, off, completer)
}

struct SrvPost {
    qpath: u64,
    uid: String,
    chan: Option<ChanR>,
}


struct DConn {
    id: u32,
    images: HashMap<u32, draw::ImageR>,
    screens: HashMap<u32, draw::ImageR>, // screen id -> the screen's image
}
type DConnR = Rc<RefCell<DConn>>;

// ---- processes ----
type NsR = Rc<RefCell<HashMap<String, Vec<MountEl>>>>;
type EnvR = Rc<RefCell<HashMap<String, Vec<u8>>>>;

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
    await_wait: Option<(Completer<RRes>, usize)>,
    argv: Vec<String>,
    image: Option<Arc<Vec<u8>>>,
    asyncified: bool,
    borrower: Option<Pid>,
    nomnt: bool,
    nowait: bool,
    note_group: u32,
    notes: Vec<String>,
    has_handler: bool,
    inflight: Option<Completer<RRes>>,
    alarm_token: Option<u64>,
    ioq: VecDeque<(u32, Vec<u8>)>,
    iowait: Option<(Completer<RRes>, Option<u64>)>,
}

#[derive(Clone)]
pub struct Cred {
    pub euid: String,
    pub ruid: String,
}

struct Fdt {
    refs: u32,
    fds: Vec<Option<ChanR>>,
}
type FdtR = Rc<RefCell<Fdt>>;

fn new_fdt() -> FdtR {
    Rc::new(RefCell::new(Fdt { refs: 1, fds: Vec::new() }))
}

// On wasm32 the host feeds the clock (clock_set); native asks the OS.
#[cfg(target_arch = "wasm32")]
static CLOCK_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(target_arch = "wasm32")]
pub fn clock_set(ms: u64) {
    CLOCK_MS.store(ms, std::sync::atomic::Ordering::Relaxed);
}
#[cfg(target_arch = "wasm32")]
fn now_secs() -> u32 {
    (CLOCK_MS.load(std::sync::atomic::Ordering::Relaxed) / 1000) as u32
}
#[cfg(target_arch = "wasm32")]
fn now_nanos() -> u64 {
    CLOCK_MS.load(std::sync::atomic::Ordering::Relaxed) * 1_000_000
}
#[cfg(not(target_arch = "wasm32"))]
fn now_secs() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as u32).unwrap_or(0)
}
#[cfg(not(target_arch = "wasm32"))]
fn now_nanos() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
}

// ---- the kernel state (behind the public facade) ----
struct KState {
    procs: HashMap<Pid, Proc>,
    nextpid: Pid,
    next_note_group: u32,
    eve: String,
    ram_root: RamRef,
    snaps: Vec<(String, u32, RamRef)>, // name, snap time, frozen root — '#V'
    canvas_caps: String, // what the attached surface offers; virtual = the suite's user
    snarf: Vec<u8>,                 // /dev/snarf's buffer — the host clipboard's twin
    host_tags: HashMap<u64, Completer<Result<HostReply, String>>>,
    host_tag_next: u64,
    snarf_waiters: Vec<(usize, u64, Completer<RRes>)>, // n, off, completer
    hostfs_root: Option<std::path::PathBuf>,
    live_root: bool,
    qgen: u64,
    cons_buf: Vec<u8>,
    cons_eof: bool,
    cons_parked: Vec<Waiter>,
    win_dirty: std::collections::HashSet<u32>,
    win_inflight: std::collections::HashSet<u32>,
    srv_posts: HashMap<String, SrvPost>,
    web: HashMap<String, WebEntry>,
    timers: HashMap<u64, TimerKind>,
    next_token: u64,
    effects: Vec<Effect>,
    verbose: bool,
    wins: HashMap<u32, WinR>,
    nextwid: u32,
    // /dev/window's ROOT events: window lifecycle, read by both halves
    winev: VecDeque<String>,
    winev_parked: Vec<Waiter>,
}

type K = Rc<RefCell<KState>>;

enum WalkRes {
    Hit(DN),
    Redirect(String),
}

// ---- devwsys implementation ----
fn wnode(kind: WKind, win: Option<WinR>, conn: Option<DConnR>) -> DN {
    DN { dev: DevId::Wsys, node: Node::Wsys { kind, win, conn, ty: None }, path: None }
}

// a node addressed through #w/<type>/… — the type is a PATH COMPONENT
// (docs/window.md), so it is carried down the walk and validated at the leaf
fn wnode_ty(kind: WKind, win: Option<WinR>, ty: Option<String>) -> DN {
    DN { dev: DevId::Wsys, node: Node::Wsys { kind, win, conn: None, ty }, path: None }
}

fn wsys_walk(k: &K, kind: WKind, win: &Option<WinR>, conn: &Option<DConnR>, ty: &Option<String>, name: &str) -> Option<DN> {
    match kind {
        WKind::Root => {
            if name == "clone" {
                return Some(wnode(WKind::Clone, None, None));
            }
            if name == "snarf" {
                return Some(wnode(WKind::Snarf, None, None));
            }
            if name == "events" {
                return Some(wnode(WKind::RootEvents, None, None));
            }
            if let Ok(wid) = name.parse::<u32>() {
                let w = k.borrow().wins.get(&wid).cloned()?;
                return Some(wnode(WKind::WinDir, Some(w), None));
            }
            // anything else is a TYPE: #w/<type>/… (docs/window.md)
            Some(wnode_ty(WKind::TypeDir, None, Some(name.to_string())))
        }
        // kids/<index> walks to the child at that POSITION — so the tree is
        // walkable AND its order is visible, which matters because ORDER IS THE
        // LAYOUT. Naming entries by window id would have lost it, since ls
        // sorts and sorted ids are not the arrangement. The child's own id is
        // in its winid file, and it is reachable equally at #w/<type>/<id>.
        WKind::WKids => {
            let w = win.clone()?;
            let i: usize = name.parse().ok()?;
            let cid = *w.borrow().kids.get(i)?;
            let c = k.borrow().wins.get(&cid).cloned()?;
            Some(wnode(WKind::WinDir, Some(c), None))
        }
        WKind::TypeDir => {
            let t = ty.clone()?;
            if name == "clone" {
                return Some(wnode_ty(WKind::Clone, None, Some(t)));
            }
            let wid: u32 = name.parse().ok()?;
            let w = k.borrow().wins.get(&wid).cloned()?;
            // the type in the path is not decoration — it must match
            if w.borrow().wtype != t { return None; }
            Some(wnode(WKind::WinDir, Some(w), None))
        }
        WKind::WinDir => {
            let w = win.clone()?;
            let kd = match name {
                "cons" => WKind::Cons,
                "mouse" => WKind::Mouse,
                "wctl" => WKind::Wctl,
                "winid" => WKind::Winid,
                "label" => WKind::Label,
                "rgb" => WKind::Rgb,
                "consctl" => WKind::Consctl,
                "cursor" => WKind::Cursor,
                "draw" => WKind::DrawDir,
                "canvas" => { mkcv(&w); WKind::CvDir }
                "content" => WKind::WContent,
                "axis" => WKind::WAxis,
                "alloc" => WKind::WAlloc,
                "kids" => WKind::WKids,
                "toolbar" => WKind::WToolbar,
                "tag" => WKind::WTag,
                "ui" => WKind::WUi,
                "events" => WKind::WEvents,
                _ => return None,
            };
            Some(wnode(kd, Some(w), None))
        }
        WKind::CvDir => {
            let w = win.clone()?;
            match name {
                "ctl" => return Some(wnode(WKind::CvCtl, Some(w), None)),
                "events" => return Some(wnode(WKind::CvEvents, Some(w), None)),
                "caps" => return Some(wnode(WKind::CvCaps, Some(w), None)),
                _ => {}
            }
            let id: u32 = name.parse().ok()?;
            let has = w.borrow().cv.as_ref().map(|c| c.nodes.contains_key(&id)).unwrap_or(false);
            if !has { return None; }
            Some(DN { dev: DevId::Wsys, node: Node::Cv { win: w, id, file: 255 }, path: None })
        }
        WKind::DrawDir => {
            let w = win.clone()?;
            if name == "new" {
                return Some(wnode(WKind::DrawNew, Some(w), None));
            }
            let id: u32 = name.parse().ok()?;
            let c = w.borrow().conns.get(&id).cloned()?;
            Some(wnode(WKind::ConnDir, Some(w), Some(c)))
        }
        WKind::ConnDir => {
            let kd = match name {
                "ctl" => WKind::DrawCtl,
                "data" => WKind::DrawData,
                "refresh" => WKind::DrawRefresh,
                _ => return None,
            };
            Some(wnode(kd, win.clone(), conn.clone()))
        }
        _ => None,
    }
}

fn pad11(v: impl std::fmt::Display) -> String {
    format!("{:>11} ", v)
}

fn new_window(k: &K) -> WinR {
    let mut kb = k.borrow_mut();
    let wid = kb.nextwid;
    kb.nextwid += 1;
    let (w, h) = (400, 300);
    let win = Rc::new(RefCell::new(Win {
        wid,
        x: 40 + (wid as i32 * 24) % 200,
        y: 40 + (wid as i32 * 24) % 140,
        w, h,
        label: format!("window {}", wid),
        img: draw::new_image([0, 0, w, h], 0, 0x0818_2848, None, Some([255, 255, 255, 255]), None),
        conns: HashMap::new(),
        nextconn: 1,
        consbuf: Vec::new(),
        uitext: Vec::new(),
        consparked: Vec::new(),
        mousebuf: VecDeque::new(),
        mouseparked: Vec::new(),
        cv: None,
        dead: false,
        parent: None,
        kids: Vec::new(),
        axis: 0,
        allocated: true,
        premax: None,
        wtype: String::new(),
        content: String::new(),
        toolbar: String::new(),
        tag: String::new(),
        uifile: String::new(),
        wevents: VecDeque::new(),
        wparked: Vec::new(),
    }));
    kb.wins.insert(wid, win.clone());
    drop(kb);
    win_flush(k, &win);
    win
}

const AX_ROW: u8 = 1;   // children side by side
const AX_COL: u8 = 2;   // children stacked

/* Split W so that a new window appears alongside it along `axis`.
 *
 * ALTERNATION IS THE INVARIANT: a container's axis is always perpendicular to
 * its parent's, which is what keeps one layout to one tree. It falls out of
 * two cases and needs no flattening pass, because a same-axis container is
 * never created in the first place:
 *
 *   the parent already arranges along this axis -> ADD A SIBLING. W is
 *   already a row (or a column); it gains a neighbour, not an interior.
 *
 *   otherwise -> GIVE W CHILDREN. W stops holding a body and holds two
 *   windows: one inheriting what W held, one new. Which is why "New column
 *   duplicates this window" — the new sibling shows what W showed, and you
 *   retitle it, because the title retargets.
 */
/* maximise(me): move every sibling out of the parent's allocation, and
 * remember which were in it so pressing again restores them. Reversible in
 * the way minimise is, and for the same reason — this is one operation.
 */
fn maximise(k: &K, w: &WinR) {
    let (wid, parent) = { let b = w.borrow(); (b.wid, b.parent) };
    let p = match parent.and_then(|p| k.borrow().wins.get(&p).cloned()) {
        Some(p) => p,
        None => return,          // nothing to maximise within
    };
    let (kids, prev) = { let b = p.borrow(); (b.kids.clone(), b.premax.clone()) };
    match prev {
        Some(was) => {           // restore the arrangement we put away
            for cid in &kids {
                let c = k.borrow().wins.get(cid).cloned();
                if let Some(c) = c {
                    c.borrow_mut().allocated = was.contains(cid);
                }
            }
            p.borrow_mut().premax = None;
        }
        None => {
            let mut was = Vec::new();
            for cid in &kids {
                let c = k.borrow().wins.get(cid).cloned();
                if let Some(c) = c {
                    if c.borrow().allocated { was.push(*cid); }
                    c.borrow_mut().allocated = *cid == wid;
                }
            }
            p.borrow_mut().premax = Some(was);
        }
    }
    for cid in kids {
        let c = k.borrow().wins.get(&cid).cloned();
        if let Some(c) = c { win_chrome(k, &c); }
    }
}

fn split(k: &K, w: &WinR, axis: u8, allocated: bool) {
    let (wid, parent) = { let b = w.borrow(); (b.wid, b.parent) };
    let pax = parent.and_then(|p| k.borrow().wins.get(&p).map(|p| p.borrow().axis));

    // a tab has no axis of its own, so it is always a sibling where there is
    // a parent to be a sibling in
    if pax == Some(axis) || (!allocated && parent.is_some()) {
        let p = k.borrow().wins.get(&parent.unwrap()).cloned();
        if let Some(p) = p {
            let sib = clone_window(k, w, Some(p.borrow().wid));
            let s = k.borrow().wins.get(&sib).cloned();
            if let Some(s) = s { s.borrow_mut().allocated = allocated; }
            let mut pb = p.borrow_mut();
            let at = pb.kids.iter().position(|&c| c == wid).map(|i| i + 1).unwrap_or(pb.kids.len());
            pb.kids.insert(at, sib);
        }
        return;
    }
    // W becomes a container: its content moves into a first child, and the
    // duplicate becomes the second.
    let first = clone_window(k, w, Some(wid));
    let second = clone_window(k, w, Some(wid));
    if !allocated {
        let s = k.borrow().wins.get(&second).cloned();
        if let Some(s) = s { s.borrow_mut().allocated = false; }
    }
    {
        let mut b = w.borrow_mut();
        b.axis = axis;
        b.kids = vec![first, second];
        b.content = String::new();
        b.toolbar = String::new();
        b.tag = String::new();
    }
    win_chrome(k, w);
}

/* a new window carrying the same content, type and tag — the duplicate that
 * New column and New row both make */
fn clone_window(k: &K, src: &WinR, parent: Option<u32>) -> u32 {
    let c = new_window(k);
    {
        let s = src.borrow();
        let mut b = c.borrow_mut();
        b.parent = parent;
        b.wtype = s.wtype.clone();
        b.content = s.content.clone();
        b.toolbar = s.toolbar.clone();
        b.tag = s.tag.clone();
    }
    let (cid, ty) = { let b = c.borrow(); (b.wid, b.wtype.clone()) };
    if !ty.is_empty() {
        win_announce(k, format!("new {} {}\n", ty, cid));
    }
    win_chrome(k, &c);
    cid
}

/* close a window, and everything it holds. acme's colcloseall(): a container
 * is a window, so closing it closes its contents — one rule, not two. Depth
 * first, so a child is never left pointing at a parent that is already gone.
 */
fn win_close(k: &K, w: &WinR) {
    let (wid, kids, parent) = {
        let b = w.borrow();
        (b.wid, b.kids.clone(), b.parent)
    };
    for cid in kids {
        let c = k.borrow().wins.get(&cid).cloned();
        if let Some(c) = c { win_close(k, &c); }
    }
    if let Some(p) = parent {
        if let Some(p) = k.borrow().wins.get(&p).cloned() {
            p.borrow_mut().kids.retain(|&c| c != wid);
        }
    }
    cv_push(k, w, "close 0");
    w.borrow_mut().dead = true;
    serve_wcons(k, w);
    serve_wmouse(k, w);
    serve_cv(k, w);
    serve_wev(k, w);
    win_announce(k, format!("del {}\n", wid));
    let mut kb = k.borrow_mut();
    kb.wins.remove(&wid);
    kb.win_dirty.remove(&wid);
    kb.win_inflight.remove(&wid);
    kb.effects.push(Effect::WinGone { wid });
}

fn win_dirty(k: &K, win: &WinR) {
    let wid = win.borrow().wid;
    k.borrow_mut().win_dirty.insert(wid);
}

fn win_flush(k: &K, win: &WinR) {
    // a canvas window's frame is its semantic tree, snapshotted at flush
    // time — intermediate syncs coalesce away by construction
    let cvsnap = {
        let wb = win.borrow();
        wb.cv.as_ref().map(|cv| {
            let mut snap: Vec<CvSnap> = cv.nodes.iter().map(|(id, nd)| CvSnap {
                id: *id, kind: nd.kind, attrs: nd.attrs.clone(), data: nd.data.clone(),
            }).collect();
            snap.sort_by_key(|n| n.id);
            (wb.wid, wb.label.clone(), wb.x, wb.y, wb.w, wb.h, snap)
        })
    };
    if let Some((wid, label, x, y, w, h, snap)) = cvsnap {
        k.borrow_mut().effects.push(Effect::WinCanvas { wid, label, x, y, w, h, snap });
        return;
    }
    let (wid, label, x, y, w, h, back) = {
        let wb = win.borrow();
        let img = wb.img.clone();
        let back = img.borrow().back.clone();
        (wb.wid, wb.label.clone(), wb.x, wb.y, wb.w, wb.h, back)
    };
    let rgba = back.borrow().data.clone();
    k.borrow_mut().effects.push(Effect::WinUpdate { wid, label, x, y, w, h, rgba });
}

fn serve_wcons(k: &K, win: &WinR) {
    loop {
        let fired = {
            let mut w = win.borrow_mut();
            if w.consparked.is_empty() || (w.consbuf.is_empty() && !w.dead) {
                return;
            }
            let waiter = w.consparked.remove(0);
            let take = waiter.n.min(w.consbuf.len());
            let give: Vec<u8> = w.consbuf.drain(0..take).collect();
            (waiter.kind, give)
        };
        wake(k, fired.0, fired.1);
    }
}

fn serve_wmouse(k: &K, win: &WinR) {
    loop {
        let fired = {
            let mut w = win.borrow_mut();
            if w.mouseparked.is_empty() || (w.mousebuf.is_empty() && !w.dead) {
                return;
            }
            let waiter = w.mouseparked.remove(0);
            let give = if w.dead { Vec::new() } else { w.mousebuf.pop_front().unwrap_or_default() };
            (waiter.kind, give)
        };
        wake(k, fired.0, fired.1);
    }
}

fn cv_kindname(kd: u8) -> &'static str {
    match kd { 0 => "stack", 1 => "text", 2 => "edit", _ => "path" }
}

fn mkcv(win: &WinR) {
    let mut wb = win.borrow_mut();
    if wb.cv.is_none() {
        let mut nodes = HashMap::new();
        nodes.insert(0, CvNode {
            kind: 0, attrs: vec![("dir".into(), "col".into())],
            data: Vec::new(), addr: (0, 0),
        });
        wb.cv = Some(Canvas { nodes, order: 1, events: VecDeque::new(), parked: Vec::new() });
    }
}

fn cv_unq(t: &str) -> Vec<u8> {
    let b = t.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let two = &t[i + 1..i + 3];
            match two {
                "20" => { out.push(b' '); i += 3; continue; }
                "0A" | "0a" => { out.push(b'\n'); i += 3; continue; }
                "25" => { out.push(b'%'); i += 3; continue; }
                _ => {}
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

fn serve_cv(k: &K, win: &WinR) {
    loop {
        let fired = {
            let mut w = win.borrow_mut();
            let dead = w.dead;
            let cv = match w.cv.as_mut() { Some(c) => c, None => return };
            if cv.parked.is_empty() || (cv.events.is_empty() && !dead) {
                return;
            }
            let waiter = cv.parked.remove(0);
            let give = cv.events.pop_front().unwrap_or_default().into_bytes();
            (waiter.kind, give)
        };
        wake(k, fired.0, fired.1);
    }
}

// /dev/window's ROOT events: window lifecycle, read by the host AND by emca
// (docs/window.md — the device mints, both halves watch)
fn win_announce(k: &K, line: String) {
    k.borrow_mut().winev.push_back(line);
    loop {
        let fired = {
            let mut kb = k.borrow_mut();
            if kb.winev_parked.is_empty() || kb.winev.is_empty() { return; }
            let waiter = kb.winev_parked.remove(0);
            let give = kb.winev.pop_front().unwrap_or_default().into_bytes();
            (waiter.kind, give)
        };
        wake(k, fired.0, fired.1);
    }
}

// the host learns a window's chrome the moment IPNX declares it
fn win_chrome(k: &K, w: &WinR) {
    let (wid, wtype, content, toolbar, tag) = {
        let wb = w.borrow();
        (wb.wid, wb.wtype.clone(), wb.content.clone(), wb.toolbar.clone(), wb.tag.clone())
    };
    k.borrow_mut().effects.push(Effect::WinChrome { wid, wtype, content, toolbar, tag });
}

// a window's own events: what the user did inside it
fn serve_wev(k: &K, win: &WinR) {
    loop {
        let fired = {
            let mut w = win.borrow_mut();
            let dead = w.dead;
            if w.wparked.is_empty() || (w.wevents.is_empty() && !dead) { return; }
            let waiter = w.wparked.remove(0);
            let give = w.wevents.pop_front().unwrap_or_default().into_bytes();
            (waiter.kind, give)
        };
        wake(k, fired.0, fired.1);
    }
}

// a user edit event mutates the node before notifying — the tree is the truth
fn cv_apply(win: &WinR, line: &str) {
    let f: Vec<&str> = line.trim().split_whitespace().collect();
    if f.len() < 4 { return; }
    let id: u32 = match f[1].parse() { Ok(v) => v, Err(_) => return };
    let mut wb = win.borrow_mut();
    let cv = match wb.cv.as_mut() { Some(c) => c, None => return };
    let nd = match cv.nodes.get_mut(&id) { Some(n) => n, None => return };
    if f[0] == "insert" {
        let ins = cv_unq(f[3]);
        let q0: usize = f[2].parse::<usize>().unwrap_or(0).min(nd.data.len());
        nd.data.splice(q0..q0, ins);
    } else if f[0] == "delete" {
        let q0: usize = f[2].parse::<usize>().unwrap_or(0).min(nd.data.len());
        let q1: usize = f[3].parse::<usize>().unwrap_or(0).min(nd.data.len());
        if q1 > q0 { nd.data.drain(q0..q1); }
    }
}

fn cv_push(k: &K, win: &WinR, line: &str) {
    if win.borrow().cv.is_none() { return; }
    cv_apply(win, line);
    {
        let mut wb = win.borrow_mut();
        let cv = wb.cv.as_mut().unwrap();
        cv.events.push_back(format!("{}\n", line));
        if cv.events.len() > 512 { cv.events.pop_front(); }
    }
    serve_cv(k, win);
}

fn cv_ctl(k: &K, win: &WinR, raw: &str) -> Result<(), KErr> {
    for line in raw.split('\n') {
        let t = line.trim();
        if t.is_empty() { continue; }
        let f: Vec<&str> = t.split_whitespace().collect();
        match f[0] {
            "new" => {
                if f.len() < 3 { return Err("new wants id and kind".into()); }
                let id: u32 = f[1].parse().map_err(|_| "new: bad id")?;
                let kd = match f[2] {
                    "stack" => 0u8, "text" => 1, "edit" => 2, "path" => 3,
                    other => return Err(format!("new: unknown kind '{}'", other)),
                };
                let mut wb = win.borrow_mut();
                let cv = wb.cv.as_mut().ok_or("no canvas")?;
                if id == 0 || cv.nodes.contains_key(&id) {
                    return Err(format!("new: bad or taken id '{}'", f[1]));
                }
                let ord = cv.order; cv.order += 1;
                cv.nodes.insert(id, CvNode {
                    kind: kd,
                    attrs: vec![("parent".into(), "0".into()), ("order".into(), ord.to_string())],
                    data: Vec::new(), addr: (0, 0),
                });
            }
            "del" => {
                if f.len() < 2 { return Err("del wants id".into()); }
                let id: u32 = f[1].parse().map_err(|_| "del: bad id")?;
                let mut wb = win.borrow_mut();
                let cv = wb.cv.as_mut().ok_or("no canvas")?;
                if id == 0 || !cv.nodes.contains_key(&id) {
                    return Err(format!("del: no node '{}'", f[1]));
                }
                let mut doomed = vec![id];
                let mut i = 0;
                while i < doomed.len() {
                    let pid = doomed[i].to_string();
                    for (k2, nd) in cv.nodes.iter() {
                        if nd.attrs.iter().any(|(a, v)| a == "parent" && *v == pid) {
                            doomed.push(*k2);
                        }
                    }
                    i += 1;
                }
                for k2 in doomed { cv.nodes.remove(&k2); }
            }
            "sync" => {
                // under credit: mark dirty, the tick flushes the LATEST tree
                let interactive = k.borrow().canvas_caps != "virtual";
                if interactive {
                    let wid = win.borrow().wid;
                    k.borrow_mut().win_dirty.insert(wid);
                }
            }
            "event" => {
                let rest = t.strip_prefix("event").unwrap_or("").trim().to_string();
                cv_push(k, win, &rest);
            }
            other => return Err(format!("canvas ctl: unknown verb '{}'", other)),
        }
    }
    Ok(())
}

// msec really advances, because double-click detection is msec arithmetic
fn inject_mouse(k: &K, win: &WinR, x: i32, y: i32, buttons: i32) {
    let msec = (now_nanos() / 1_000_000) & 0x3fff_ffff;
    let msg = format!("m{}{}{}{}", pad11(x), pad11(y), pad11(buttons), pad11(msec));
    {
        let mut w = win.borrow_mut();
        w.mousebuf.push_back(msg.into_bytes());
        if w.mousebuf.len() > 256 {
            w.mousebuf.pop_front();
        }
    }
    serve_wmouse(k, win);
}

async fn wsys_read(k: &K, kind: WKind, win: &Option<WinR>, conn: &Option<DConnR>, ty: &Option<String>,
                   chan: &ChanR, n: usize, off: u64) -> Result<RRes, KErr> {
    let one = |s: String| -> RRes {
        RRes::Data(if off == 0 { s.into_bytes() } else { Vec::new() })
    };
    match kind {
        WKind::Root => {
            // the windows, listed — rio-today's ls '#w' (policy needs an enumeration)
            let mut skip = off as usize;
            let mut out = Vec::new();
            let mut ents: Vec<(String, bool)> = vec![("clone".into(), false)];
            {
                let kb = k.borrow();
                let mut ids: Vec<u32> = kb.wins.keys().cloned().collect();
                ids.sort();
                for w in ids { ents.push((w.to_string(), true)); }
            }
            for (i, (nm, isdir)) in ents.iter().enumerate() {
                let rec = marshal_stat(&StatIn {
                    name: nm, uid: "wsys", gid: "wsys", qpath: 7000 + i as u64,
                    qtype: if *isdir { QTDIR } else { 0 },
                    mode: if *isdir { DMDIR | 0o555 } else { 0o444 },
                    ..Default::default()
                });
                if skip >= rec.len() { skip -= rec.len(); continue; }
                if out.len() + rec.len() > n { break; }
                out.extend_from_slice(&rec);
            }
            return Ok(RRes::Data(out));
        }
        // the children, in order — so the tree is `ls`-able and the order,
        // which is the layout's order, is visible rather than inferred
        WKind::WKids => {
            let w = win.as_ref().ok_or("no window")?;
            let kids: Vec<u32> = w.borrow().kids.clone();
            let mut skip = off as usize;
            let mut out = Vec::new();
            for (i, cid) in kids.iter().enumerate() {
                let rec = marshal_stat(&StatIn {
                    name: &i.to_string(), uid: "wsys", gid: "wsys",
                    qpath: 7600 + *cid as u64, qtype: QTDIR, mode: DMDIR | 0o555,
                    ..Default::default()
                });
                if skip >= rec.len() { skip -= rec.len(); continue; }
                if out.len() + rec.len() > n { break; }
                out.extend_from_slice(&rec);
            }
            return Ok(RRes::Data(out));
        }
        WKind::CvCaps => {
            let caps = k.borrow().canvas_caps.clone();
            return Ok(one(format!("{}\n", caps)));
        }
        WKind::Snarf => {
            // ask the host clipboard; it answers through snarf_done (the
            // kernel buffer is the answer where no bridge exists)
            let (c, wt) = oneshot::<RRes>();
            {
                let mut kb = k.borrow_mut();
                kb.snarf_waiters.push((n, off, c));
                kb.effects.push(Effect::SnarfGet);
            }
            return Ok(wt.await);
        }
        WKind::CvEvents => {
            let w = win.as_ref().ok_or("no window")?;
            let now = {
                let mut wb = w.borrow_mut();
                let dead = wb.dead;
                let cv = wb.cv.as_mut().ok_or("no canvas")?;
                if let Some(line) = cv.events.pop_front() {
                    Some(line.into_bytes())
                } else if dead {
                    Some(Vec::new())
                } else {
                    None
                }
            };
            if let Some(give) = now {
                return Ok(RRes::Data(give));
            }
            let (c, wt) = oneshot::<RRes>();
            w.borrow_mut().cv.as_mut().unwrap().parked.push(Waiter { n, kind: WaitKind::Wake { pid: 0, c } });
            return Ok(wt.await);
        }
        WKind::CvDir | WKind::CvCtl => return Ok(RRes::Data(Vec::new())),
        WKind::TypeDir => return Ok(RRes::Data(Vec::new())),
        WKind::WContent => {
            let w = win.as_ref().ok_or("no window")?;
            let s = w.borrow().content.clone();
            return Ok(one(s));
        }
        WKind::WAxis => {
            let w = win.as_ref().ok_or("no window")?;
            let s = match w.borrow().axis { 1 => "row\n", 2 => "col\n", _ => "" };
            return Ok(one(s.to_string()));
        }
        // allocated, or a tab — the one bit that decides whether this window
        // has a rectangle at all, and therefore whether it shows its furniture
        WKind::WAlloc => {
            let w = win.as_ref().ok_or("no window")?;
            let s = if w.borrow().allocated { "allocated\n" } else { "tab\n" };
            return Ok(one(s.to_string()));
        }
        WKind::WToolbar => {
            let w = win.as_ref().ok_or("no window")?;
            let s = w.borrow().toolbar.clone();
            return Ok(one(s));
        }
        WKind::WTag => {
            let w = win.as_ref().ok_or("no window")?;
            let s = w.borrow().tag.clone();
            return Ok(one(s));
        }
        WKind::WUi => {
            let w = win.as_ref().ok_or("no window")?;
            let s = w.borrow().uifile.clone();
            return Ok(one(s));
        }
        WKind::RootEvents => {
            let now = {
                let mut kb = k.borrow_mut();
                kb.winev.pop_front().map(|l| l.into_bytes())
            };
            if let Some(give) = now { return Ok(RRes::Data(give)); }
            let (c, wt) = oneshot::<RRes>();
            k.borrow_mut().winev_parked.push(Waiter { n, kind: WaitKind::Wake { pid: 0, c } });
            return Ok(wt.await);
        }
        WKind::WEvents => {
            let w = win.as_ref().ok_or("no window")?;
            let now = {
                let mut wb = w.borrow_mut();
                let dead = wb.dead;
                if let Some(line) = wb.wevents.pop_front() { Some(line.into_bytes()) }
                else if dead { Some(Vec::new()) } else { None }
            };
            if let Some(give) = now { return Ok(RRes::Data(give)); }
            let (c, wt) = oneshot::<RRes>();
            w.borrow_mut().wparked.push(Waiter { n, kind: WaitKind::Wake { pid: 0, c } });
            return Ok(wt.await);
        }
        WKind::Clone => {
            // reading clone mints the window and pins it on this chan
            let have = {
                let cb = chan.borrow();
                match &cb.node {
                    Node::Wsys { win: Some(w), .. } => Some(w.clone()),
                    _ => None,
                }
            };
            let w = match have {
                Some(w) => w,
                None => {
                    let w = new_window(k);
                    // the type is a PATH COMPONENT: #w/<type>/clone mints one
                    // (docs/window.md). The device announces it; BOTH halves
                    // are watching, which is why emca needs no privilege.
                    if let Some(t) = ty {
                        w.borrow_mut().wtype = t.clone();
                        let wid = w.borrow().wid;
                        win_announce(k, format!("new {} {}\n", t, wid));
                        win_chrome(k, &w);
                    }
                    let mut cb = chan.borrow_mut();
                    cb.node = Node::Wsys { kind: WKind::Clone, win: Some(w.clone()), conn: None, ty: ty.clone() };
                    w
                }
            };
            let wid = w.borrow().wid;
            Ok(one(format!("{}", wid)))
        }
        WKind::Winid => {
            let w = win.as_ref().ok_or("no window")?;
            let wid = w.borrow().wid;
            Ok(one(format!("{}", wid)))
        }
        WKind::Label => {
            let w = win.as_ref().ok_or("no window")?;
            let l = w.borrow().label.clone();
            Ok(one(l))
        }
        WKind::Wctl => {
            let w = win.as_ref().ok_or("no window")?;
            let wb = w.borrow();
            Ok(one(format!("{} {} {} {} current visible
", wb.x, wb.y, wb.w, wb.h)))
        }
        WKind::DrawNew | WKind::DrawCtl => {
            let w = win.as_ref().ok_or("no window")?;
            let wb = w.borrow();
            let id = conn.as_ref().map(|c| c.borrow().id).unwrap_or(0);
            Ok(one(format!(
                "{}{}r8g8b8a8    {}{}{}{}{}{}{}{}{}
",
                pad11(id), pad11(0), pad11(0), pad11(0), pad11(0),
                pad11(wb.w), pad11(wb.h), pad11(0), pad11(0), pad11(wb.w), pad11(wb.h)
            )))
        }
        WKind::Rgb => {
            let w = win.as_ref().ok_or("no window")?;
            let img = w.borrow().img.clone();
            let back = img.borrow().back.clone();
            let b = back.borrow();
            let start = (off as usize).min(b.data.len());
            let end = (off as usize + n).min(b.data.len());
            Ok(RRes::Data(b.data[start..end].to_vec()))
        }
        WKind::Cons => {
            let w = win.as_ref().ok_or("no window")?;
            let now = {
                let mut wb = w.borrow_mut();
                if !wb.consbuf.is_empty() || wb.dead {
                    let take = n.min(wb.consbuf.len());
                    Some(wb.consbuf.drain(0..take).collect::<Vec<u8>>())
                } else {
                    None
                }
            };
            if let Some(give) = now {
                return Ok(RRes::Data(give));
            }
            let (c, wt) = oneshot::<RRes>();
            w.borrow_mut().consparked.push(Waiter { n, kind: WaitKind::Wake { pid: 0, c } });
            Ok(wt.await)
        }
        WKind::Mouse => {
            let w = win.as_ref().ok_or("no window")?;
            let now = {
                let mut wb = w.borrow_mut();
                if let Some(m) = wb.mousebuf.pop_front() {
                    Some(m)
                } else if wb.dead {
                    Some(Vec::new())
                } else {
                    None
                }
            };
            if let Some(give) = now {
                return Ok(RRes::Data(give));
            }
            let (c, wt) = oneshot::<RRes>();
            w.borrow_mut().mouseparked.push(Waiter { n: 0, kind: WaitKind::Wake { pid: 0, c } });
            Ok(wt.await)
        }
        WKind::DrawRefresh => {
            // refresh events: none in v0; readers wait forever (the dropped
            // completer never fires — deliberately)
            let (_c, wt) = oneshot::<RRes>();
            Ok(wt.await)
        }
        _ => Err(format!("no read on {:?}", kind)),
    }
}

fn wsys_write(k: &K, kind: WKind, win: &Option<WinR>, conn: &Option<DConnR>,
              data: &[u8]) -> Result<usize, KErr> {
    match kind {
        // /dev/window: IPNX declares the chrome; the surface speaks back
        WKind::WContent | WKind::WToolbar | WKind::WTag | WKind::WUi => {
            let w = win.as_ref().ok_or("no window")?;
            let s = String::from_utf8_lossy(data).to_string();
            {
                let mut wb = w.borrow_mut();
                match kind {
                    WKind::WContent => wb.content = s,
                    WKind::WToolbar => wb.toolbar = s,
                    WKind::WTag => wb.tag = s,
                    _ => wb.uifile = s,
                }
            }
            // Content is an EVENT, not a sample. A window is minted before its
            // content is known, so a watcher that read `content` once at mint
            // would race the program that fills it in. Announcing the write is
            // what lets emca stay a watcher rather than a gatekeeper — and it
            // is the same door by which the surface may open a different file
            // in a window that already exists.
            if matches!(kind, WKind::WContent) {
                let (ty, wid, c) = {
                    let wb = w.borrow();
                    (wb.wtype.clone(), wb.wid, wb.content.clone())
                };
                win_announce(k, format!("content {} {} {}\n", ty, wid, c.trim()));
            }
            win_chrome(k, w);
            return Ok(data.len());
        }
        // writing a window's events is the surface's voice — and the virtual
        // surface's door, the same house precedent as canvas's `event` verb
        WKind::WEvents => {
            let w = win.as_ref().ok_or("no window")?;
            for line in String::from_utf8_lossy(data).lines() {
                if line.trim().is_empty() { continue; }
                w.borrow_mut().wevents.push_back(format!("{}\n", line.trim()));
            }
            serve_wev(k, w);
            return Ok(data.len());
        }
        WKind::Cons => {
            // window text: buffered per window, emitted at the credited tick
            // (a direct effect per write is unbounded when the UI stalls —
            // macOS drags run a modal loop; measured as the second OOM path)
            if let Some(w) = win {
                let mut wb = w.borrow_mut();
                wb.uitext.extend_from_slice(data);
                let over = wb.uitext.len().saturating_sub(1 << 20);
                if over > 0 {
                    wb.uitext.drain(0..over);   // keep the newest 1MB
                }
                drop(wb);
                win_dirty(k, w);
            }
            Ok(data.len())
        }
        WKind::Consctl | WKind::Cursor => Ok(data.len()),
        WKind::Label => {
            let w = win.as_ref().ok_or("no window")?;
            w.borrow_mut().label = String::from_utf8_lossy(data).trim().to_string();
            win_dirty(k, w);
            Ok(data.len())
        }
        WKind::Wctl => {
            let w = win.as_ref().ok_or("no window")?;
            let raw = String::from_utf8_lossy(data).into_owned();
            if let Some(rest) = raw.strip_prefix("type ") {
                let mut wb = w.borrow_mut();
                wb.consbuf.extend_from_slice(rest.as_bytes());
                drop(wb);
                serve_wcons(k, w);
                return Ok(data.len());
            }
            let t: Vec<&str> = raw.trim().split_whitespace().collect();
            match t.as_slice() {
                // NEW ROW / NEW COLUMN. The user states a DIRECTION; whether
                // that means "give this window children" or "give it a
                // sibling" is worked out from the parent's axis and never
                // asked about (emca.txt, Rows and columns).
                ["newrow"] => { split(k, w, AX_COL, true); }
                ["newcol"] => { split(k, w, AX_ROW, true); }
                // a tab is the same operation with the allocation bit
                // flipped, which is why it is not a fourth concept
                ["newtab"] => { split(k, w, AX_ROW, false); }
                // MINIMISE AND MAXIMISE ARE ONE OPERATION with different
                // arguments: minimise(me) moves me out of the allocation,
                // maximise(me) moves everyone else out. Both toggle.
                ["minimise"] | ["minimize"] => {
                    let now = { let mut b = w.borrow_mut(); b.allocated = !b.allocated; b.allocated };
                    let _ = now;
                    win_chrome(k, w);
                }
                ["maximise"] | ["maximize"] => { maximise(k, w); }
                // THE ALLOCATION emca computed. Distinct from `resize`,
                // which reallocates a raster — a tree window has no raster,
                // and allocating one per layout pass would be pure waste.
                // Reads come back through wctl unchanged, so `cat wctl` is
                // how rc asserts a layout.
                ["rect", x, y, ww, hh] => {
                    let mut wb = w.borrow_mut();
                    wb.x = x.parse().unwrap_or(wb.x);
                    wb.y = y.parse().unwrap_or(wb.y);
                    wb.w = ww.parse().unwrap_or(wb.w);
                    wb.h = hh.parse().unwrap_or(wb.h);
                }
                ["move", x, y] => {
                    let mut wb = w.borrow_mut();
                    wb.x = x.parse().unwrap_or(wb.x);
                    wb.y = y.parse().unwrap_or(wb.y);
                }
                ["resize", nw, nh] => {
                    let (nw, nh): (i32, i32) =
                        (nw.parse().map_err(|_| "bad resize")?, nh.parse().map_err(|_| "bad resize")?);
                    cv_push(k, w, &format!("resize 0 {} {}", nw, nh));
                    let img = draw::new_image([0, 0, nw, nh], 0, 0x0818_2848, None,
                                              Some([255, 255, 255, 255]), None);
                    let mut wb = w.borrow_mut();
                    wb.w = nw;
                    wb.h = nh;
                    wb.img = img.clone();
                    for c in wb.conns.values() {
                        c.borrow_mut().images.insert(0, img.clone());
                    }
                    drop(wb);
                    win_dirty(k, w);
                }
                ["mouse", x, y, b] => {
                    inject_mouse(k, w,
                        x.parse().map_err(|_| "bad mouse")?,
                        y.parse().map_err(|_| "bad mouse")?,
                        b.parse().map_err(|_| "bad mouse")?);
                }
                // CLOSING A CONTAINER CLOSES WHAT IT HOLDS, which is acme's
                // own colcloseall(): textclose() on the tag, then winclose()
                // over every window inside. A column is a window, so this is
                // one rule and not two.
                ["delete"] => { win_close(k, w); }
                _ => return Err(format!("wctl: bad message '{}'", raw.trim())),
            }
            Ok(data.len())
        }
        WKind::CvCtl => {
            let w = win.as_ref().ok_or("no window")?;
            let raw = String::from_utf8_lossy(data).into_owned();
            cv_ctl(k, w, &raw)?;
            Ok(data.len())
        }
        WKind::Snarf => {
            let mut kb = k.borrow_mut();
            let text = {
                kb.snarf.extend_from_slice(data);
                String::from_utf8_lossy(&kb.snarf).into_owned()
            };
            kb.effects.push(Effect::SnarfSet { text });
            Ok(data.len())
        }
        WKind::DrawData => {
            let w = win.as_ref().ok_or("no window")?;
            let c = conn.as_ref().ok_or("no draw connection")?;
            drawmsgs(w, c, data)?;
            win_dirty(k, w);
            Ok(data.len())
        }
        _ => Err(format!("no write on {:?}", kind)),
    }
}

fn wsys_stat(kind: WKind, win: &Option<WinR>, conn: &Option<DConnR>, ty: &Option<String>) -> Vec<u8> {
    let dir = matches!(kind, WKind::Root | WKind::WinDir | WKind::DrawDir | WKind::ConnDir | WKind::CvDir | WKind::TypeDir | WKind::WKids);
    let name = match kind {
        WKind::Root => "wsys".to_string(),
        WKind::WinDir => win.as_ref().map(|w| w.borrow().wid.to_string()).unwrap_or_default(),
        WKind::DrawDir => "draw".to_string(),
        WKind::ConnDir => conn.as_ref().map(|c| c.borrow().id.to_string()).unwrap_or_default(),
        WKind::DrawNew => "new".to_string(),
        WKind::DrawCtl => "ctl".to_string(),
        WKind::DrawData => "data".to_string(),
        WKind::DrawRefresh => "refresh".to_string(),
        WKind::Clone => "clone".to_string(),
        WKind::Cons => "cons".to_string(),
        WKind::Mouse => "mouse".to_string(),
        WKind::Wctl => "wctl".to_string(),
        WKind::Winid => "winid".to_string(),
        WKind::Label => "label".to_string(),
        WKind::Rgb => "rgb".to_string(),
        WKind::Consctl => "consctl".to_string(),
        WKind::Cursor => "cursor".to_string(),
        WKind::CvDir => "canvas".to_string(),
        WKind::CvCtl => "ctl".to_string(),
        WKind::CvEvents => "events".to_string(),
        WKind::CvCaps => "caps".to_string(),
        WKind::Snarf => "snarf".to_string(),
        WKind::TypeDir => ty.clone().unwrap_or_else(|| "type".to_string()),
        WKind::RootEvents | WKind::WEvents => "events".to_string(),
        WKind::WContent => "content".to_string(),
        WKind::WAxis => "axis".to_string(),
        WKind::WAlloc => "alloc".to_string(),
        WKind::WKids => "kids".to_string(),
        WKind::WToolbar => "toolbar".to_string(),
        WKind::WTag => "tag".to_string(),
        WKind::WUi => "ui".to_string(),
    };
    let wid = win.as_ref().map(|w| w.borrow().wid).unwrap_or(0) as u64;
    let cid = conn.as_ref().map(|c| c.borrow().id).unwrap_or(0) as u64;
    let length = if kind == WKind::Rgb {
        win.as_ref()
            .map(|w| {
                let img = w.borrow().img.clone();
                let back = img.borrow().back.clone();
                let n = back.borrow().data.len() as u64;
                n
            })
            .unwrap_or(0)
    } else {
        0
    };
    marshal_stat(&StatIn {
        name: &name,
        qtype: if dir { QTDIR } else { 0 },
        qpath: wid * 1024 + cid * 16 + name.len() as u64,
        mode: if dir { DMDIR | 0o555 } else { 0o666 },
        length,
        ..Default::default()
    })
}

// the draw(3) message subset; values low-order byte first
fn drawmsgs(win: &WinR, conn: &DConnR, b: &[u8]) -> Result<(), KErr> {
    let u32a = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let s32 = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let u16a = |o: usize| u16::from_le_bytes(b[o..o + 2].try_into().unwrap());
    let img = |id: u32| -> Result<draw::ImageR, KErr> {
        conn.borrow().images.get(&id).cloned().ok_or_else(|| format!("draw: no image {}", id))
    };
    let mut o = 0usize;
    while o < b.len() {
        let op = b[o] as char;
        match op {
            'b' => {
                // id[4] screen[4] refresh[1] chan[4] repl[1] r[16] clipr[16] color[4]
                let id = u32a(o + 1);
                let screenid = u32a(o + 5);
                let chan = u32a(o + 10);
                let repl = b[o + 14];
                let r = [s32(o + 15), s32(o + 19), s32(o + 23), s32(o + 27)];
                let clipr = [s32(o + 31), s32(o + 35), s32(o + 39), s32(o + 43)];
                let color = draw::rgba32(u32a(o + 47));
                if screenid != 0 {
                    let scr = conn.borrow().screens.get(&screenid).cloned()
                        .ok_or_else(|| format!("draw: no screen {}", screenid))?;
                    // a window on a screen: a view sharing the screen's backing
                    let (schan, sback) = {
                        let si = scr.borrow();
                        (si.chan, si.back.clone())
                    };
                    let im = draw::new_image(r, repl, schan, Some(clipr), Some(color), Some(sback));
                    conn.borrow_mut().images.insert(id, im);
                } else {
                    let im = draw::new_image(r, repl, chan, Some(clipr), Some(color), None);
                    conn.borrow_mut().images.insert(id, im);
                }
                o += 51;
            }
            'A' => {
                // allocscreen: id[4] image[4] fill[4] public[1]
                let sid = u32a(o + 1);
                let im = img(u32a(o + 5))?;
                conn.borrow_mut().screens.insert(sid, im);
                o += 14;
            }
            'F' => {
                conn.borrow_mut().screens.remove(&u32a(o + 1));
                o += 5;
            }
            'c' => {
                // replclipr: id[4] repl[1] clipr[16]
                let im = img(u32a(o + 1))?;
                let mut ib = im.borrow_mut();
                ib.repl = b[o + 5];
                ib.clipr = [s32(o + 6), s32(o + 10), s32(o + 14), s32(o + 18)];
                o += 22;
            }
            't' => {
                o += 4 + 4 * u16a(o + 2) as usize; // top/bottom: one window per screen
            }
            'd' => {
                // dst[4] src[4] mask[4] dstr[16] srcpt[8] maskpt[8]
                let dst = img(u32a(o + 1))?;
                let src = img(u32a(o + 5))?;
                let dstr = [s32(o + 13), s32(o + 17), s32(o + 21), s32(o + 25)];
                draw::draw_op(&dst, dstr, &src, [s32(o + 29), s32(o + 33)]);
                o += 45;
            }
            'f' => {
                conn.borrow_mut().images.remove(&u32a(o + 1));
                o += 5;
            }
            'L' => {
                // dst[4] p0[8] p1[8] end0[4] end1[4] thick[4] src[4] sp[8]
                let dst = img(u32a(o + 1))?;
                let src = img(u32a(o + 33))?;
                draw::line(&dst, [s32(o + 5), s32(o + 9)], [s32(o + 13), s32(o + 17)],
                           &src, [s32(o + 37), s32(o + 41)]);
                o += 45;
            }
            'e' | 'E' => {
                // dst[4] c[8] a[4] b[4] thick[4] src[4] sp[8]
                let dst = img(u32a(o + 1))?;
                let src = img(u32a(o + 25))?;
                draw::ellipse(&dst, [s32(o + 5), s32(o + 9)], s32(o + 13), s32(o + 17),
                              &src, [s32(o + 29), s32(o + 33)], op == 'E');
                o += 37;
            }
            'y' => {
                // id[4] r[16] data in the image's chan (draw(6))
                let im = img(u32a(o + 1))?;
                let r = [s32(o + 5), s32(o + 9), s32(o + 13), s32(o + 17)];
                let nb = draw::load_rows(&im, r, &b[o + 21..]);
                o += 21 + nb;
            }
            'i' => {
                // id[4] nchars[4] ascent[1]: the image becomes a font
                let im = img(u32a(o + 1))?;
                im.borrow_mut().font = Some(draw::FontInfo {
                    nchars: u32a(o + 5),
                    ascent: b[o + 9],
                    slots: HashMap::new(),
                });
                o += 10;
            }
            'l' => {
                // cache[4] src[4] index[2] r[16] p[8] left[1] width[1]
                let cache = img(u32a(o + 1))?;
                let src = img(u32a(o + 5))?;
                if cache.borrow().font.is_none() {
                    return Err("draw: l on a non-font image (send i first)".into());
                }
                let index = u16a(o + 9);
                let r = [s32(o + 11), s32(o + 15), s32(o + 19), s32(o + 23)];
                draw::copy_rect(&cache, r, &src, [s32(o + 27), s32(o + 31)]);
                cache.borrow_mut().font.as_mut().unwrap().slots.insert(index, draw::FontSlot {
                    r,
                    left: b[o + 35] as i8,
                    width: b[o + 36],
                });
                o += 37;
            }
            's' | 'x' => {
                // dst[4] src[4] font[4] p[8] clipr[16] sp[8] [x: bg[4] bgp[8]] ni[2] ni*index[2]
                let dst = img(u32a(o + 1))?;
                let src = img(u32a(o + 5))?;
                let fontim = img(u32a(o + 9))?;
                if fontim.borrow().font.is_none() {
                    return Err(format!("draw: {} needs a font image", op));
                }
                let mut x = s32(o + 13);
                let y = s32(o + 17);
                let sp = [s32(o + 37), s32(o + 41)];
                let ni = u16a(o + 45) as usize;
                let (mut base, bg, mut bgp) = if op == 'x' {
                    let bg = img(u32a(o + 47))?;
                    (o + 59, Some(bg), [s32(o + 51), s32(o + 55)])
                } else {
                    (o + 47, None, [0, 0])
                };
                let font_h = fontim.borrow().h;
                let ascent = fontim.borrow().font.as_ref().unwrap().ascent as i32;
                for kx in 0..ni {
                    let idx = u16a(base + 2 * kx);
                    let slot = fontim.borrow().font.as_ref().unwrap().slots.get(&idx)
                        .map(|s| (s.r, s.left as i32, s.width as i32));
                    if let Some((sr, left, width)) = slot {
                        if let Some(bgim) = &bg {
                            draw::draw_op(&dst, [x, y - ascent, x + width, y - ascent + font_h],
                                          bgim, bgp);
                            bgp = [bgp[0] + width, bgp[1]];
                        }
                        draw::glyph(&dst, [x + left, y - ascent], &fontim, sr, &src, sp);
                        x += width;
                    }
                }
                base += 2 * ni;
                o = base;
            }
            'O' => o += 2, // set compositing op: S-over-D is all v0 does
            'v' => {
                // present: headless native has no host surface; the raster
                // stays readable through rgb
                let _ = win;
                o += 1;
            }
            other => {
                return Err(format!(
                    "draw: message '{}' not implemented (have b d f L e E y i l s x c A F t O v)",
                    other
                ));
            }
        }
    }
    Ok(())
}

// ---- the public facade: the same surface the host already speaks ----
pub struct Kernel {
    k: K,
    ex: Rc<LocalExec>,
    pub interactive: bool,
    pub verbose: bool,
}

impl Kernel {
    /// M4: expose a host directory as '#Z' (hostfs). With `live`, the
    /// implicit root becomes that directory too — boot FROM the host tree,
    /// writes persisting across restarts.
    pub fn set_hostfs(&self, dir: std::path::PathBuf, live: bool) {
        let mut kb = self.k.borrow_mut();
        let _ = dir;                    // the HOST keeps the real root
        kb.hostfs_root = Some(std::path::PathBuf::new());
        kb.live_root = live;
    }

    /// The embedding answers a delegated '#Z' operation.
    pub fn hostop_done(&mut self, tag: u64, r: Result<HostReply, String>) {
        let c = self.k.borrow_mut().host_tags.remove(&tag);
        if let Some(c) = c {
            c.complete(r);
        }
        self.ex.run_until_stalled();
    }

    /// Merge a seed-shaped subtree into the live ramfs — the demo's
    /// streamed toolchain overlays land through this.
    pub fn graft(&mut self, seed: &Seed) {
        fn add(k: &K, node: &RamRef, sk: &Seed, eve: &str) {
            for kd in &sk.kids {
                if kd.dir {
                    let d = kid(node, &kd.name).filter(|d| d.borrow().dir);
                    let d = match d {
                        Some(d) => d,
                        None => {
                            let q = {
                                let mut kb = k.borrow_mut();
                                let q = kb.qgen;
                                kb.qgen += 1;
                                q
                            };
                            let n = Rc::new(RefCell::new(RNode {
                                name: kd.name.clone(), qpath: q, dir: true,
                                data: Rc::default(), kids: Vec::new(),
                                uid: eve.into(), mode: 0o755,
                                atime: now_secs(), mtime: now_secs(),
                                symlink: None, ro: false,
                            }));
                            node.borrow_mut().kids.push((kd.name.clone(), n.clone()));
                            n
                        }
                    };
                    add(k, &d, kd, eve);
                } else {
                    let q = {
                        let mut kb = k.borrow_mut();
                        let q = kb.qgen;
                        kb.qgen += 1;
                        q
                    };
                    let n = Rc::new(RefCell::new(RNode {
                        name: kd.name.clone(), qpath: q, dir: false,
                        data: Rc::new(kd.data.clone()), kids: Vec::new(),
                        uid: eve.into(), mode: 0o755,
                        atime: now_secs(), mtime: now_secs(),
                        symlink: None, ro: false,
                    }));
                    let mut nb = node.borrow_mut();
                    nb.kids.retain(|(n2, _)| n2 != &kd.name);
                    nb.kids.push((kd.name.clone(), n));
                }
            }
        }
        let (root, eve) = {
            let kb = self.k.borrow();
            (kb.ram_root.clone(), kb.eve.clone())
        };
        add(&self.k, &root, seed, &eve);
    }

    /// The host mirrors its clipboard into /dev/snarf (the shell's cmd-C).
    pub fn snarf_put(&mut self, text: &str) {
        self.k.borrow_mut().snarf = text.as_bytes().to_vec();
    }

    /// M3: the presentation layer's input — keys into a window's cons,
    /// mouse into its mouse file, a close request. wid from WinUpdate.
    /// M3: the host's frame tick — emit ONE WinUpdate per dirty window.
    /// Unbounded flush-per-draw-write was measured at 640GB of queued
    /// frames during acme's boot; coalescing to the tick is the cure.
    pub fn win_tick(&self) {
        let dirty: Vec<u32> = self.k.borrow().win_dirty.iter().cloned().collect();
        for wid in dirty {
            if self.k.borrow().win_inflight.contains(&wid) {
                continue;                     // credit spent: retry next tick
            }
            self.k.borrow_mut().win_dirty.remove(&wid);
            let w = self.k.borrow().wins.get(&wid).cloned();
            if let Some(w) = w {
                let text: Vec<u8> = std::mem::take(&mut w.borrow_mut().uitext);
                if !text.is_empty() {
                    self.k.borrow_mut().effects.push(Effect::WinText { wid, bytes: text });
                }
                win_flush(&self.k, &w);
                self.k.borrow_mut().win_inflight.insert(wid);
            }
        }
    }

    /// '#H': the host's GET finished — settle the entry, wake the readers
    pub fn fetch_done(&mut self, url: &str, result: Result<Vec<u8>, String>) {
        let waiters = {
            let mut kb = self.k.borrow_mut();
            match kb.web.get_mut(url) {
                Some(e) => {
                    e.state = WebState::Done(result);
                    std::mem::take(&mut e.waiters)
                }
                None => Vec::new(),
            }
        };
        for (n, off, c) in waiters {
            let r = {
                let kb = self.k.borrow();
                match &kb.web.get(url).unwrap().state {
                    WebState::Done(Ok(body)) => {
                        let start = (off as usize).min(body.len());
                        let end = (off as usize + n).min(body.len());
                        RRes::Data(body[start..end].to_vec())
                    }
                    _ => RRes::Data(Vec::new()),   // error: readers see EOF; the
                                                    // next read reports the error
                }
            };
            c.complete(r);
        }
        self.ex.run_until_stalled();
    }

    /// the UI painted this window's last batch: restore its credit
    // the host clipboard answered (None = no bridge: serve our buffer)
    pub fn snarf_done(&self, text: Option<String>) {
        let (waiters, body) = {
            let mut kb = self.k.borrow_mut();
            if let Some(t) = text {
                kb.snarf = t.into_bytes();
            }
            let w = std::mem::take(&mut kb.snarf_waiters);
            (w, kb.snarf.clone())
        };
        for (n, off, c) in waiters {
            let o = (off as usize).min(body.len());
            let give = body[o..(o + n).min(body.len())].to_vec();
            c.complete(RRes::Data(give));
        }
        self.ex.run_until_stalled();
    }

    pub fn canvas_caps_set(&self, caps: &str) {
        self.k.borrow_mut().canvas_caps = caps.to_string();
    }

    // an interactive surface speaks: deliver one event line to a window's canvas
    pub fn canvas_event(&self, wid: u32, line: &str) {
        let win = self.k.borrow().wins.get(&wid).cloned();
        if let Some(w) = win {
            mkcv(&w);
            cv_push(&self.k, &w, line);
        }
    }

    /// the surface opens a file. Answered later with Effect::ReadDone —
    /// asynchronous because a read may park, exactly as a guest's would.
    /// Resolved in INIT'S NAMESPACE (pid 1) for now; a window does not yet
    /// record which process owns it.
    pub fn read_path(&mut self, token: f64, path: &str) {
        let k = self.k.clone();
        let path = path.to_string();
        self.ex.spawn(async move {
            let r = read_whole(&k, 1, &path).await;
            let (ok, data) = match r { Ok(d) => (true, d), Err(_) => (false, Vec::new()) };
            k.borrow_mut().effects.push(Effect::ReadDone { token, ok, data });
        });
        self.ex.run_until_stalled();
    }

    /// Put: the surface streams the edited file back over the namespace
    pub fn write_path(&mut self, token: f64, path: &str, data: Vec<u8>) {
        let k = self.k.clone();
        let path = path.to_string();
        self.ex.spawn(async move {
            let r = write_whole(&k, 1, &path, &data).await;
            let (ok, n) = match r { Ok(n) => (true, n as u32), Err(_) => (false, 0) };
            k.borrow_mut().effects.push(Effect::WriteDone { token, ok, n });
        });
        self.ex.run_until_stalled();
    }

    /// the surface's voice: a line into /dev/window/<type>/<n>/events
    pub fn win_event(&self, wid: u32, line: &str) {
        let win = self.k.borrow().wins.get(&wid).cloned();
        if let Some(w) = win {
            w.borrow_mut().wevents.push_back(format!("{}\n", line.trim()));
            serve_wev(&self.k, &w);
            self.ex.run_until_stalled();
        }
    }

    pub fn win_ack(&self, wid: u32) {
        self.k.borrow_mut().win_inflight.remove(&wid);
        self.ex.run_until_stalled();
    }

    pub fn win_key(&self, wid: u32, bytes: &[u8]) {
        let w = self.k.borrow().wins.get(&wid).cloned();
        if let Some(w) = w {
            w.borrow_mut().consbuf.extend_from_slice(bytes);
            serve_wcons(&self.k, &w);
        }
        self.ex.run_until_stalled();
    }

    pub fn win_mouse(&self, wid: u32, x: i32, y: i32, buttons: i32) {
        let w = self.k.borrow().wins.get(&wid).cloned();
        if let Some(w) = w {
            inject_mouse(&self.k, &w, x, y, buttons);
        }
        self.ex.run_until_stalled();
    }

    pub fn win_close(&self, wid: u32) {
        let w = self.k.borrow().wins.get(&wid).cloned();
        if let Some(w) = w {
            {
                let mut wb = w.borrow_mut();
                wb.dead = true;
            }
            serve_wcons(&self.k, &w);
            serve_wmouse(&self.k, &w);
            serve_wev(&self.k, &w);
            win_announce(&self.k, format!("del {}\n", wid));
            let mut kb = self.k.borrow_mut();
            kb.wins.remove(&wid);
            kb.win_dirty.remove(&wid);
            kb.win_inflight.remove(&wid);
            kb.effects.push(Effect::WinGone { wid });
        }
        self.ex.run_until_stalled();
    }

    pub fn new(seed: &Seed, eve: &str) -> Kernel {
        let boot = now_secs();
        let mut qgen = 1u64;
        let root = load_seed(seed, "/", eve, boot, &mut qgen);
        if kid(&root, "tmp").is_none() {
            let t = Rc::new(RefCell::new(RNode {
                name: "tmp".into(), qpath: qgen, dir: true, data: Rc::default(),
                kids: Vec::new(), uid: eve.into(), mode: 0o777, atime: boot,
                mtime: boot, symlink: None, ro: false,
            }));
            qgen += 1;
            root.borrow_mut().kids.push(("tmp".into(), t));
        }
        Kernel {
            k: Rc::new(RefCell::new(KState {
                winev: VecDeque::new(),
                winev_parked: Vec::new(),
                procs: HashMap::new(),
                nextpid: 1,
                next_note_group: 1,
                eve: eve.into(),
                ram_root: root,
                snaps: Vec::new(),
                canvas_caps: "virtual".into(),
                snarf: Vec::new(),
                host_tags: HashMap::new(),
                host_tag_next: 1,
                snarf_waiters: Vec::new(),
                hostfs_root: None,
                live_root: false,
                qgen,
                cons_buf: Vec::new(),
                cons_eof: false,
                cons_parked: Vec::new(),
                win_dirty: std::collections::HashSet::new(),
                win_inflight: std::collections::HashSet::new(),
                srv_posts: HashMap::new(),
                web: HashMap::new(),
                timers: HashMap::new(),
                next_token: 1,
                effects: Vec::new(),
                verbose: false,
                wins: HashMap::new(),
                nextwid: 1,
            })),
            ex: LocalExec::new(),
            interactive: false,
            verbose: false,
        }
    }

    pub fn take_effects(&mut self) -> Vec<Effect> {
        self.ex.run_until_stalled();
        std::mem::take(&mut self.k.borrow_mut().effects)
    }

    pub fn boot(&mut self, argv: Vec<String>) -> Result<(), String> {
        self.k.borrow_mut().verbose = self.verbose;
        let k = self.k.clone();
        let eve = k.borrow().eve.clone();
        let pid = new_proc(&k, 0, Rc::new(RefCell::new(HashMap::new())), new_fdt(),
                           "/".into(), Cred { euid: eve.clone(), ruid: eve },
                           Rc::new(RefCell::new(HashMap::new())), None);
        let argv2 = argv.clone();
        self.ex.spawn(async move {
            let r: Result<(), KErr> = async {
                let dn = walk(&k, pid, "/bin/init", false).await?;
                let image = Arc::new(read_all(&k, &dn, pid).await?);
                let mut kb = k.borrow_mut();
                let p = kb.procs.get_mut(&pid).unwrap();
                p.argv = argv2.clone();
                p.image = Some(image.clone());
                kb.effects.push(Effect::Spawn { pid, image, argv: argv2.clone(), asy: None });
                Ok(())
            }
            .await;
            if let Err(e) = r {
                eprintln!("boot: {}", e);
                k.borrow_mut().effects.push(Effect::Shutdown(1));
            }
        });
        self.ex.run_until_stalled();
        Ok(())
    }

    pub fn syscall(&mut self, worker_pid: Pid, trap: i32, a: [i32; 5], tx: Vec<u8>,
                   reply: Sender<KReply>) {
        let k = self.k.clone();
        let ex = self.ex.clone();
        self.ex.spawn(async move {
            let pid = k.borrow().procs.get(&worker_pid).and_then(|p| p.borrower).unwrap_or(worker_pid);
            if k.borrow().verbose {
                eprintln!("[K sys pid={} trap={} a0={}]", pid, trap, a[0]);
            }
            if trap != t::EXITS {
                let doom = {
                    let kb = k.borrow();
                    kb.procs.get(&pid).and_then(|p| {
                        if !p.notes.is_empty() && !p.has_handler {
                            Some(format!("note: {}", p.notes[0]))
                        } else {
                            None
                        }
                    })
                };
                if let Some(msg) = doom {
                    let r = kill_proc(&k, worker_pid, pid, &msg);
                    let _ = reply.send(r);
                    return;
                }
            }
            let r = match dispatch(&k, &ex, worker_pid, pid, trap, a, tx).await {
                Ok(r) => {
                    if trap == t::EXITS || trap == t::RFORK {
                        r
                    } else {
                        stamp(&k, pid, r)
                    }
                }
                Err(msg) => {
                    if k.borrow().verbose {
                        eprintln!("[K err pid={} trap={}: {}]", pid, trap, msg);
                    }
                    if let Some(p) = k.borrow_mut().procs.get_mut(&pid) {
                        p.errstr = msg;
                    }
                    stamp(&k, pid, ok(-1))
                }
            };
            let _ = reply.send(r);
        });
        self.ex.run_until_stalled();
    }

    pub fn set_asyncified(&mut self, pid: Pid, on: bool) {
        if let Some(p) = self.k.borrow_mut().procs.get_mut(&pid) {
            p.asyncified = on;
        }
    }

    pub fn cons_feed(&mut self, chunk: &[u8]) {
        self.k.borrow_mut().cons_buf.extend_from_slice(chunk);
        cons_serve(&self.k);
        self.ex.run_until_stalled();
    }

    pub fn cons_end(&mut self) {
        self.k.borrow_mut().cons_eof = true;
        cons_serve(&self.k);
        self.ex.run_until_stalled();
    }

    pub fn timer_fired(&mut self, token: u64) {
        let kind = self.k.borrow_mut().timers.remove(&token);
        match kind {
            Some(TimerKind::Sleep(c)) => c.complete(RRes::Data(Vec::new())),
            Some(TimerKind::Alarm(pid)) => {
                let armed = self.k.borrow().procs.get(&pid)
                    .map(|p| p.alarm_token == Some(token)).unwrap_or(false);
                if armed {
                    self.k.borrow_mut().procs.get_mut(&pid).unwrap().alarm_token = None;
                    postnote(&self.k, pid, "alarm");
                }
            }
            Some(TimerKind::Iowait(pid)) => {
                let c = {
                    let mut kb = self.k.borrow_mut();
                    kb.procs.get_mut(&pid).and_then(|p| {
                        p.inflight = None;
                        p.iowait.take().map(|(c, _)| c)
                    })
                };
                if let Some(c) = c {
                    c.complete(RRes::Data(Vec::new())); // iowait timeout: 0 bytes
                }
            }
            None => {}
        }
        self.ex.run_until_stalled();
    }

    pub fn asyfork(&mut self, parent_pid: Pid, child_pid: Pid, snap: Vec<u8>, data_ptr: u32, sp: u32) {
        let mut kb = self.k.borrow_mut();
        let argv = kb.procs.get(&parent_pid).map(|p| p.argv.clone()).unwrap_or_default();
        if let Some(c) = kb.procs.get_mut(&child_pid) {
            c.argv = argv.clone();
            if let Some(image) = c.image.clone() {
                kb.effects.push(Effect::Spawn {
                    pid: child_pid, image, argv,
                    asy: Some(AsySnap { snap, data_ptr, sp }),
                });
            }
        }
    }

    pub fn proc_died(&mut self, pid: Pid, msg: &str) {
        let taken = self.k.borrow_mut().procs.remove(&pid);
        let Some(p) = taken else { return };
        fdt_close(&self.k, &p.fdt);
        if pid == 1 {
            self.k.borrow_mut().effects.push(Effect::Shutdown(1));
            return;
        }
        zombie(&self.k, p.ppid, pid, msg, p.nowait);
        self.ex.run_until_stalled();
    }
}

fn load_seed(s: &Seed, name: &str, eve: &str, boot: u32, qgen: &mut u64) -> RamRef {
    let q = *qgen;
    *qgen += 1;
    if s.dir {
        let node = Rc::new(RefCell::new(RNode {
            name: name.into(), qpath: q, dir: true, data: Rc::default(),
            kids: Vec::new(), uid: eve.into(), mode: 0o755, atime: boot,
            mtime: boot, symlink: None, ro: false,
        }));
        for k in &s.kids {
            let child = load_seed(k, &k.name, eve, boot, qgen);
            node.borrow_mut().kids.push((k.name.clone(), child));
        }
        node
    } else {
        Rc::new(RefCell::new(RNode {
            name: name.into(), qpath: q, dir: false, data: Rc::new(s.data.clone()),
            kids: Vec::new(), uid: eve.into(), mode: 0o644, atime: boot,
            mtime: boot, symlink: None, ro: false,
        }))
    }
}

fn new_proc(k: &K, ppid: Pid, ns: NsR, fdt: FdtR, cwd: String, cred: Cred, env: EnvR,
            note_group: Option<u32>) -> Pid {
    let mut kb = k.borrow_mut();
    let pid = kb.nextpid;
    kb.nextpid += 1;
    let group = note_group.unwrap_or_else(|| {
        let g = kb.next_note_group;
        kb.next_note_group += 1;
        g
    });
    kb.procs.insert(pid, Proc {
        pid, ppid, ns, fdt, cwd, cred, env,
        umask: 0o22, errstr: String::new(), zombies: Vec::new(),
        await_wait: None, argv: Vec::new(), image: None, asyncified: false,
        borrower: None, nomnt: false, nowait: false, note_group: group,
        notes: Vec::new(), has_handler: false, inflight: None,
        alarm_token: None, ioq: VecDeque::new(), iowait: None,
    });
    pid
}

fn stamp(k: &K, pid: Pid, mut r: KReply) -> KReply {
    if let Some(p) = k.borrow().procs.get(&pid) {
        r.note_pending = !p.notes.is_empty() && p.has_handler;
    }
    r
}

// ---- notes ----
fn postnote(k: &K, pid: Pid, msg: &str) {
    let c = {
        let mut kb = k.borrow_mut();
        let Some(p) = kb.procs.get_mut(&pid) else { return };
        let mut m = msg.to_string();
        m.truncate(120);
        p.notes.push(m);
        p.iowait = None;
        p.await_wait = None;
        p.inflight.take()
    };
    if let Some(c) = c {
        if let Some(p) = k.borrow_mut().procs.get_mut(&pid) {
            p.errstr = "interrupted".into();
        }
        c.complete(RRes::Intr);
    }
}

fn kill_proc(k: &K, worker_pid: Pid, pid: Pid, msg: &str) -> KReply {
    let (ppid, nowait, fdt) = {
        let kb = k.borrow();
        match kb.procs.get(&pid) {
            Some(p) => (p.ppid, p.nowait, p.fdt.clone()),
            None => return ok(-1),
        }
    };
    k.borrow_mut().procs.remove(&pid);
    fdt_close(k, &fdt);
    let was_borrowed = k.borrow().procs.get(&worker_pid).and_then(|p| p.borrower) == Some(pid);
    if was_borrowed {
        k.borrow_mut().procs.get_mut(&worker_pid).unwrap().borrower = None;
        zombie(k, ppid, pid, msg, nowait);
        return KReply { ret: -1000, aux: pid as i32, data: Vec::new(),
                        action: KAction::ForkResume, load: None, note_pending: false };
    }
    if pid == 1 {
        k.borrow_mut().effects.push(Effect::Shutdown(1));
    } else {
        zombie(k, ppid, pid, msg, nowait);
    }
    KReply { ret: -3000, aux: 0, data: Vec::new(), action: KAction::Die,
             load: None, note_pending: false }
}

fn zombie(k: &K, ppid: Pid, pid: Pid, msg: &str, nowait: bool) {
    if nowait {
        return;
    }
    let fire = {
        let mut kb = k.borrow_mut();
        let Some(parent) = kb.procs.get_mut(&ppid) else { return };
        parent.zombies.push(format!("{} 0 0 0 '{}'", pid, msg));
        parent.inflight = None;
        parent.await_wait.take().map(|(c, max)| {
            let s = parent.zombies.remove(0);
            (c, s, max)
        })
    };
    if let Some((c, s, max)) = fire {
        let mut bytes = s.into_bytes();
        bytes.truncate(max.saturating_sub(1));
        c.complete(RRes::Data(bytes));
    }
}

// ---- console ----
fn cons_serve(k: &K) {
    loop {
        let fired = {
            let mut kb = k.borrow_mut();
            if kb.cons_parked.is_empty() || (kb.cons_buf.is_empty() && !kb.cons_eof) {
                return;
            }
            let w = kb.cons_parked.remove(0);
            let take = w.n.min(kb.cons_buf.len());
            let give: Vec<u8> = kb.cons_buf.drain(0..take).collect();
            (w.kind, give)
        };
        wake(k, fired.0, fired.1);
    }
}

fn wake(k: &K, kind: WaitKind, data: Vec<u8>) {
    match kind {
        WaitKind::Wake { pid, c } => {
            if let Some(p) = k.borrow_mut().procs.get_mut(&pid) {
                p.inflight = None;
            }
            c.complete(RRes::Data(data));
        }
        WaitKind::Aread { pid, tag } => aread_done(k, pid, tag, data),
    }
}

fn aread_done(k: &K, pid: Pid, tag: u32, data: Vec<u8>) {
    let fire = {
        let mut kb = k.borrow_mut();
        let Some(p) = kb.procs.get_mut(&pid) else { return };
        p.ioq.push_back((tag, data));
        p.iowait.take().map(|(c, timer)| {
            p.inflight = None;
            let (tag, data) = p.ioq.pop_front().unwrap();
            (c, timer, tag, data)
        })
    };
    if let Some((c, timer, tag, data)) = fire {
        if let Some(tok) = timer {
            k.borrow_mut().timers.remove(&tok);
        }
        let mut out = tag.to_le_bytes().to_vec();
        out.extend_from_slice(&data);
        c.complete(RRes::Data(out));
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

fn attach(k: &K, spec: &str) -> Result<DN, KErr> {
    let letter = spec.chars().nth(1).unwrap_or(' ');
    match letter {
        'c' => Ok(DN { dev: DevId::Cons, node: Node::ConsRoot, path: None }),
        'e' => Ok(DN { dev: DevId::Env, node: Node::EnvRoot, path: None }),
        'd' => Ok(DN { dev: DevId::Dup, node: Node::DupRoot, path: None }),
        'p' => Ok(DN { dev: DevId::Proc, node: Node::Proc { kind: 0, pid: 0 }, path: None }),
        'M' => Ok(DN { dev: DevId::Ram, node: Node::Ram(k.borrow().ram_root.clone()), path: None }),
        'w' => Ok(DN { dev: DevId::Wsys, node: Node::Wsys { kind: WKind::Root, win: None, conn: None, ty: None }, path: None }),
        's' => Ok(DN { dev: DevId::Srv, node: Node::SrvRoot, path: None }),
        'H' => Ok(DN { dev: DevId::Web, node: Node::WebRoot, path: None }),
        'V' => Ok(DN { dev: DevId::Snap, node: Node::SnapRoot, path: None }),
        'Z' => {
            k.borrow().hostfs_root.clone()
                .ok_or_else(|| "no host directory configured (#Z)".to_string())?;
            Ok(DN { dev: DevId::Host, node: Node::Host(std::path::PathBuf::new()), path: None })
        }
        _ => Err(format!("unknown device #{}", letter)),
    }
}

// walk one name on a CONCRETE (non-union) node
async fn dev_walk_one(k: &K, dn: &DN, name: &str, pid: Pid) -> Result<Option<DN>, KErr> {
    match (&dn.dev, &dn.node) {
        (DevId::Web, Node::WebRoot) => {
            // '#H/<hex-of-url>' — webfs's spirit, the demo shim's exact walk
            if name.len() % 2 != 0 || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Ok(None);
            }
            let bytes: Vec<u8> = (0..name.len()).step_by(2)
                .map(|i| u8::from_str_radix(&name[i..i + 2], 16).unwrap_or(0)).collect();
            let url = String::from_utf8_lossy(&bytes).into_owned();
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Ok(None);
            }
            Ok(Some(DN { dev: DevId::Web, node: Node::Web(url), path: None }))
        }
        (DevId::Srv, Node::SrvRoot) => {
            Ok(if k.borrow().srv_posts.contains_key(name) {
                Some(DN { dev: DevId::Srv, node: Node::SrvName(name.into()), path: None })
            } else { None })
        }
        (DevId::Host, Node::Host(hp)) => {
            k.borrow().hostfs_root.clone().ok_or("no host root")?;
            let next = hp.join(name);
            match hostq(k, HostOp::Meta { path: hpstr(&next) }).await? {
                HostReply::Missing => Ok(None),
                _ => Ok(Some(DN { dev: DevId::Host, node: Node::Host(next), path: None })),
            }
        }
        (DevId::Ram, Node::Ram(r)) => {
            Ok(kid(r, name).map(|kd| DN { dev: DevId::Ram, node: Node::Ram(kd), path: None }))
        }
        (DevId::Wsys, Node::Cv { win, id, file: 255 }) => {
            let file = match name {
                "kind" => 0u8, "attrs" => 1, "addr" => 2, "data" => 3,
                _ => return Ok(None),
            };
            Ok(Some(DN { dev: DevId::Wsys, node: Node::Cv { win: win.clone(), id: *id, file }, path: None }))
        }
        (DevId::Snap, Node::SnapRoot) => {
            if name == "ctl" {
                return Ok(Some(DN { dev: DevId::Snap, node: Node::SnapCtl, path: None }));
            }
            Ok(k.borrow().snaps.iter().find(|(nm, _, _)| nm == name).map(|(_, _, root)| {
                // hand the walk to the ram dispatch: read/stat/walk come free,
                // and the ro flag on every node refuses the rest
                DN { dev: DevId::Ram, node: Node::Ram(root.clone()), path: None }
            }))
        }
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
            let fdt = k.borrow().procs.get(&pid).ok_or("no proc")?.fdt.clone();
            let c = fdt.borrow().fds.get(fdn).cloned().flatten();
            Ok(c.map(|c| DN { dev: DevId::Dup, node: Node::DupFd(c), path: None }))
        }
        (DevId::Env, Node::EnvRoot) => {
            let env = k.borrow().procs.get(&pid).ok_or("no proc")?.env.clone();
            let has = env.borrow().contains_key(name);
            Ok(if has {
                Some(DN { dev: DevId::Env, node: Node::EnvVar(name.into()), path: None })
            } else {
                None
            })
        }
        (DevId::Proc, Node::Proc { kind: 0, .. }) => {
            let target = if name == "self" {
                Some(pid) // the WALKER, never the binder
            } else {
                name.parse::<Pid>().ok().filter(|q| k.borrow().procs.contains_key(q))
            };
            Ok(target.map(|q| DN { dev: DevId::Proc, node: Node::Proc { kind: 1, pid: q }, path: None }))
        }
        (DevId::Proc, Node::Proc { kind: 1, pid: q }) => {
            let kd = match name {
                "ctl" => 2,
                "status" => 3,
                "note" => 4,
                "notepg" => 5,
                // proc(3)'s own file: the argument list. The data was always
                // in the proc record; nothing could read it, so nothing could
                // name a running command.
                "args" => 6,
                _ => return Ok(None),
            };
            Ok(Some(DN { dev: DevId::Proc, node: Node::Proc { kind: kd, pid: *q }, path: None }))
        }
        (DevId::Wsys, Node::Wsys { kind, win, conn, ty }) => Ok(wsys_walk(k, *kind, win, conn, ty, name)),
        (DevId::Mnt, Node::Mnt(m)) => {
            let conn = m.conn.clone();
            let newfid = {
                let mut cb = conn.borrow_mut();
                let f = cb.nextfid;
                cb.nextfid += 1;
                f
            };
            let body = W9::new().u32(m.fid).u32(newfid).u16(1).s(name);
            match rpc(k, &conn, tv::WALK, body).await {
                Ok(rb) => {
                    let mut r = R9::new(&rb);
                    let nw = r.u16();
                    if nw != 1 {
                        if k.borrow().verbose {
                            eprintln!("[K mnt walk '{}': nwqid={}]", name, nw);
                        }
                        return Ok(None);
                    }
                    let (qt, _, _) = r.qid();
                    if m.ephemeral.get() {
                        clunk_fid(k, &conn, m.fid);
                    }
                    Ok(Some(DN {
                        dev: DevId::Mnt,
                        node: Node::Mnt(Rc::new(MntNode {
                            conn, fid: newfid, qtype: qt,
                            ephemeral: std::cell::Cell::new(true),
                            opened: std::cell::Cell::new(false),
                        })),
                        path: None,
                    }))
                }
                Err(e) => {
                    if k.borrow().verbose {
                        eprintln!("[K mnt walk '{}' rpc err: {}]", name, e);
                    }
                    Ok(None)
                }
            }
        }
        _ => Ok(None),
    }
}

async fn dev_walk(k: &K, dn: &DN, name: &str, pid: Pid) -> Result<Option<DN>, KErr> {
    if let (DevId::Union, Node::Union(list)) = (&dn.dev, &dn.node) {
        let els: Vec<MountEl> = list.iter().cloned().collect();
        for el in els {
            if let Ok(Some(dn2)) = Box::pin(dev_walk_one(k, &el.dn, name, pid)).await {
                return Ok(Some(dn2)); // leaving the union: the real dev takes over
            }
        }
        return Ok(None);
    }
    dev_walk_one(k, dn, name, pid).await
}

async fn symtarget(k: &K, dn: &DN) -> Option<String> {
    if let Node::Ram(r) = &dn.node {
        return r.borrow().symlink.clone();
    }
    if let Node::Mnt(m) = &dn.node {
        if m.qtype & 0x02 != 0 {
            // QTSYMLINK over the wire: ask the server (minted Treadlink)
            let conn = m.conn.clone();
            if let Ok(rb) = rpc(k, &conn, tv::READLINK, W9::new().u32(m.fid)).await {
                return Some(R9::new(&rb).s());
            }
        }
    }
    None
}

async fn walk(k: &K, pid: Pid, path: &str, nofollow_last: bool) -> Result<DN, KErr> {
    let cwd = k.borrow().procs.get(&pid).map(|p| p.cwd.clone()).unwrap_or_else(|| "/".into());
    let mut full = canon(path, &cwd);
    for depth in 0.. {
        if depth > 8 {
            return Err("too many levels of symlinks".into());
        }
        match walk_once(k, pid, &full, nofollow_last).await? {
            WalkRes::Hit(dn) => return Ok(dn),
            WalkRes::Redirect(r) => full = canon(&r, &cwd),
        }
    }
    unreachable!()
}

async fn walk_once(k: &K, pid: Pid, path: &str, nofollow_last: bool) -> Result<WalkRes, KErr> {
    if path.starts_with('#') {
        let nomnt = k.borrow().procs.get(&pid).map(|p| p.nomnt).unwrap_or(false);
        if nomnt {
            return Err("'#' names disallowed (RFNOMNT)".into());
        }
        let slash = path.find('/');
        let spec = match slash {
            Some(i) => &path[..i],
            None => path,
        };
        let mut dn = attach(k, spec)?;
        if let Some(i) = slash {
            for name in path[i + 1..].split('/').filter(|s| !s.is_empty()) {
                dn = dev_walk(k, &dn, name, pid).await?
                    .ok_or_else(|| format!("'{}' does not exist", path))?;
            }
        }
        return Ok(WalkRes::Hit(dn));
    }
    let (best, list) = {
        let kb = k.borrow();
        let p = kb.procs.get(&pid).ok_or("no proc")?;
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
        None => {
            let kb = k.borrow();
            if kb.live_root {
                if kb.hostfs_root.is_some() {
                    drop(kb);
                    DN { dev: DevId::Host, node: Node::Host(std::path::PathBuf::new()), path: None }
                } else {
                    let r = kb.ram_root.clone();
                    drop(kb);
                    DN { dev: DevId::Ram, node: Node::Ram(r), path: None }
                }
            } else {
                let r = kb.ram_root.clone();
                drop(kb);
                DN { dev: DevId::Ram, node: Node::Ram(r), path: None }
            }
        }
    };
    let full_comps: Vec<String> =
        path.split('/').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
    let rest: Vec<String> =
        path[best.len()..].split('/').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
    for (i, name) in rest.iter().enumerate() {
        let next = dev_walk(k, &dn, name, pid).await?
            .ok_or_else(|| format!("'{}' does not exist", path))?;
        dn = next;
        if i == rest.len() - 1 && nofollow_last {
            break;
        }
        if let Some(target) = symtarget(k, &dn).await {
            let here = full_comps.len() - rest.len() + i;
            let base = if target.starts_with('/') {
                target
            } else {
                let mut parts: Vec<&str> = full_comps[..here].iter().map(|s| s.as_str()).collect();
                parts.push(&target);
                format!("/{}", parts.join("/"))
            };
            let rem = rest[i + 1..].join("/");
            let redirect = if rem.is_empty() { base } else { format!("{}/{}", base, rem) };
            return Ok(WalkRes::Redirect(redirect));
        }
    }
    dn.path = Some(path.to_string());
    Ok(WalkRes::Hit(dn))
}

async fn walk_parent(k: &K, pid: Pid, path: &str) -> Result<(DN, String), KErr> {
    let cwd = k.borrow().procs.get(&pid).map(|p| p.cwd.clone()).unwrap_or_else(|| "/".into());
    let path = canon(path, &cwd);
    let i = path.rfind('/').unwrap_or(0);
    let base = path[i + 1..].to_string();
    if base.is_empty() || path.starts_with('#') {
        return Err(format!("bad path '{}'", path));
    }
    let parent = walk(k, pid, if i == 0 { "/" } else { &path[..i] }, false).await?;
    Ok((parent, base))
}

async fn ns_insert(k: &K, pid: Pid, old: &str, dn: DN, flag: i32) -> Result<(), KErr> {
    let mode = flag & 3;
    let create = flag & 4 != 0;
    let el = MountEl { dn, create };
    let ns = k.borrow().procs.get(&pid).ok_or("no proc")?.ns.clone();
    if mode == 0 {
        ns.borrow_mut().insert(old.into(), vec![el]);
        return Ok(());
    }
    let have = ns.borrow().get(old).cloned();
    let mut list = match have {
        Some(l) => l,
        None => {
            let under = walk(k, pid, old, false).await?; // must exist, per bind(2)
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
    ns.borrow_mut().insert(old.into(), list);
    Ok(())
}

// ---- devmnt plumbing ----
fn clunk_fid(k: &K, conn: &ConnR, fid: u32) {
    // fire and forget: nothing to do with the answer
    let tag = {
        let mut cb = conn.borrow_mut();
        if cb.dead.is_some() {
            return;
        }
        let tag = cb.nexttag;
        cb.nexttag = cb.nexttag.wrapping_add(1).max(1);
        let (c, _w) = oneshot::<Result<Vec<u8>, KErr>>();
        cb.tags.insert(tag, c);
        cb.expect.insert(tag, tv::CLUNK + 1);
        tag
    };
    let frame = W9::new().u32(fid).frame(tv::CLUNK, tag);
    let chan = conn.borrow().chan.clone();
    let _ = dev_write_sync(k, &chan, &frame, u64::MAX, 1);
}

async fn rpc(k: &K, conn: &ConnR, ty: u8, body: W9) -> Result<Vec<u8>, KErr> {
    let (tag, w) = {
        let mut cb = conn.borrow_mut();
        if let Some(d) = &cb.dead {
            return Err(d.clone());
        }
        let tag = cb.nexttag;
        cb.nexttag = cb.nexttag.wrapping_add(1).max(1);
        let (c, w) = oneshot::<Result<Vec<u8>, KErr>>();
        cb.tags.insert(tag, c);
        cb.expect.insert(tag, ty + 1);
        (tag, w)
    };
    let frame = body.frame(ty, tag);
    let chan = conn.borrow().chan.clone();
    dev_write_sync(k, &chan, &frame, u64::MAX, 1)?;
    w.await
}

// the per-connection reader task: frames in, tags completed
async fn conn_reader(k: K, conn: ConnR) {
    let chan = conn.borrow().chan.clone();
    let mut rbuf: Vec<u8> = Vec::new();
    loop {
        let res = dev_read_async(&k, &chan, MSIZE, u64::MAX, 1).await;
        let chunk = match res {
            Ok(RRes::Data(d)) if !d.is_empty() => d,
            _ => {
                let dead = "mount server closed".to_string();
                let mut cb = conn.borrow_mut();
                cb.dead = Some(dead.clone());
                for (_, c) in cb.tags.drain() {
                    c.complete(Err(dead.clone()));
                }
                cb.expect.clear();
                return;
            }
        };
        rbuf.extend_from_slice(&chunk);
        loop {
            if rbuf.len() < 4 {
                break;
            }
            let size = u32::from_le_bytes(rbuf[0..4].try_into().unwrap()) as usize;
            if rbuf.len() < size {
                break;
            }
            let msg: Vec<u8> = rbuf.drain(0..size).collect();
            let ty = msg[4];
            let tag = u16::from_le_bytes(msg[5..7].try_into().unwrap());
            let (pend, expect) = {
                let mut cb = conn.borrow_mut();
                (cb.tags.remove(&tag), cb.expect.remove(&tag))
            };
            let Some(pend) = pend else { continue }; // stray tag: drop
            if ty == tv::RERROR {
                let emsg = R9::new(&msg[7..]).s();
                pend.complete(Err(emsg));
            } else if Some(ty) != expect {
                pend.complete(Err(format!("9P: expected R{}, got {}", expect.unwrap_or(0), ty)));
            } else {
                pend.complete(Ok(msg[7..].to_vec()));
            }
        }
    }
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

// A snapshot is a structural clone of the ram tree: O(nodes), zero bytes copied.
// Every node is marked ro; every data Rc is shared. The live side's next write
// to a shared buffer copies it (Rc::make_mut) — versioning by copy-on-write.
fn snap_tree(n: &RamRef) -> RamRef {
    let nb = n.borrow();
    Rc::new(RefCell::new(RNode {
        name: nb.name.clone(), qpath: nb.qpath, dir: nb.dir, data: nb.data.clone(),
        kids: nb.kids.iter().map(|(k2, v)| (k2.clone(), snap_tree(v))).collect(),
        uid: nb.uid.clone(), mode: nb.mode, atime: nb.atime, mtime: nb.mtime,
        symlink: nb.symlink.clone(), ro: true,
    }))
}

fn ram_access(k: &K, node: &RamRef, cred: &Cred, want: u32) -> Result<(), KErr> {
    // V10's shape: policy concentrates here. The ro gate outranks even eve —
    // nobody rewrites history, which is what makes a snapshot a snapshot.
    if want & 2 != 0 && node.borrow().ro {
        return Err("read-only snapshot".into());
    }
    if cred.euid == k.borrow().eve {
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

async fn dev_stat(k: &K, dn: &DN, pid: Pid) -> Result<Vec<u8>, KErr> {
    match (&dn.dev, &dn.node) {
        (DevId::Ram, Node::Ram(r)) => Ok(ram_stat(r)),
        (DevId::Snap, Node::SnapRoot) => Ok(marshal_stat(&StatIn {
            name: "V", uid: "eve", gid: "eve", qtype: QTDIR, mode: DMDIR | 0o555,
            ..Default::default()
        })),
        (DevId::Snap, Node::SnapCtl) => Ok(marshal_stat(&StatIn {
            name: "ctl", uid: "eve", gid: "eve", mode: 0o666, ..Default::default()
        })),
        (DevId::Host, Node::Host(hp)) => host_stat_bytes(k, hp).await,
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
            Box::pin(dev_stat(k, &first, pid)).await
        }
        (DevId::Env, Node::EnvVar(name)) => {
            let env = k.borrow().procs.get(&pid).ok_or("no proc")?.env.clone();
            let len = env.borrow().get(name).map(|v| v.len()).unwrap_or(0);
            Ok(marshal_stat(&StatIn { name, mode: 0o664, length: len as u64, ..Default::default() }))
        }
        (DevId::Env, Node::EnvRoot) => Ok(marshal_stat(&StatIn {
            name: "/", qtype: QTDIR, mode: DMDIR | 0o775, ..Default::default()
        })),
        (DevId::Pipe, _) => Ok(marshal_stat(&StatIn { name: "data", mode: 0o600, ..Default::default() })),
        (DevId::Mnt, Node::Mnt(m)) => {
            let conn = m.conn.clone();
            let rb = rpc(k, &conn, tv::STAT, W9::new().u32(m.fid)).await?;
            let mut r = R9::new(&rb);
            r.u16(); // outer size, per stat(5)'s double count
            Ok(r.rest().to_vec())
        }
        (DevId::Wsys, Node::Cv { id, file, .. }) => Ok(marshal_stat(&StatIn {
            name: match file { 255 => "node", 0 => "kind", 1 => "attrs", 2 => "addr", _ => "data" },
            uid: "wsys", gid: "wsys", qpath: 7100 + *id as u64 * 8 + *file as u64,
            qtype: if *file == 255 { QTDIR } else { 0 },
            mode: if *file == 255 { DMDIR | 0o555 } else { 0o666 },
            ..Default::default()
        })),
        (DevId::Wsys, Node::Wsys { kind, win, conn, ty }) => Ok(wsys_stat(*kind, win, conn, ty)),
        (DevId::Web, Node::WebRoot) => Ok(marshal_stat(&StatIn {
            name: "web", qtype: QTDIR, mode: DMDIR | 0o555, ..Default::default()
        })),
        (DevId::Web, Node::Web(_)) => Ok(marshal_stat(&StatIn {
            name: "get", mode: 0o444, ..Default::default()
        })),
        (DevId::Srv, Node::SrvRoot) => Ok(marshal_stat(&StatIn {
            name: "srv", qtype: QTDIR, mode: DMDIR | 0o777, ..Default::default()
        })),
        (DevId::Srv, Node::SrvName(nm)) => {
            let kb = k.borrow();
            let post = kb.srv_posts.get(nm).ok_or("gone")?;
            Ok(marshal_stat(&StatIn {
                name: nm, qpath: post.qpath, mode: 0o600, uid: &post.uid,
                gid: &post.uid, ..Default::default()
            }))
        }
        (DevId::Proc, Node::Proc { kind: 0, .. }) => Ok(marshal_stat(&StatIn {
            name: "proc", qtype: QTDIR, mode: DMDIR | 0o555, ..Default::default()
        })),
        (DevId::Proc, Node::Proc { kind: 1, pid: q }) => Ok(marshal_stat(&StatIn {
            name: &q.to_string(), qtype: QTDIR, qpath: *q as u64,
            mode: DMDIR | 0o555, ..Default::default()
        })),
        (DevId::Proc, Node::Proc { kind, pid: q }) => {
            let name = match kind { 2 => "ctl", 3 => "status", 4 => "note", 6 => "args", _ => "notepg" };
            Ok(marshal_stat(&StatIn { name, qpath: *q as u64, mode: 0o600, ..Default::default() }))
        }
        _ => Err("no stat on this device (v0)".into()),
    }
}

// async read: sync devices answer now; pipe/cons park on a oneshot; mnt rpcs.
// off_in == u64::MAX means "current offset" (and advances it).
async fn dev_read_async(k: &K, chan: &ChanR, n: usize, off_in: u64, pid: Pid) -> Result<RRes, KErr> {
    let (dev, node, cur_off) = {
        let c = chan.borrow();
        (c.dev, c.node.clone(), c.offset)
    };
    let cur = off_in == u64::MAX;
    let off = if cur { cur_off } else { off_in };
    fn advance(chan: &ChanR, cur: bool, len: usize) {
        if cur {
            chan.borrow_mut().offset += len as u64;
        }
    }
    match (dev, node) {
        (DevId::Host, Node::Host(hp)) => {
            let dir = matches!(hostq(k, HostOp::Meta { path: hpstr(&hp) }).await?,
                               HostReply::Meta { dir: true, .. });
            let out = if dir {
                host_dir_read(k, &hp, n, off).await?
            } else {
                match hostq(k, HostOp::Read { path: hpstr(&hp), off, n }).await? {
                    HostReply::Bytes(b) => b,
                    _ => return Err(format!("{}: read failed", hp.display())),
                }
            };
            advance(chan, cur, out.len());
            Ok(RRes::Data(out))
        }
        (DevId::Wsys, Node::Cv { win, id, file }) => {
            let idv: u32 = id;
            let wb = win.borrow();
            let cv = wb.cv.as_ref().ok_or("no canvas")?;
            let nd = cv.nodes.get(&idv).ok_or("node gone")?;
            let out: Vec<u8> = match file {
                0 => if off == 0 { format!("{}\n", cv_kindname(nd.kind)).into_bytes() } else { Vec::new() },
                1 => if off == 0 {
                    let mut t = String::new();
                    for (a, v) in &nd.attrs { t.push_str(&format!("{}={}\n", a, v)); }
                    t.into_bytes()
                } else { Vec::new() },
                2 => if off == 0 { format!("{},{}\n", nd.addr.0, nd.addr.1).into_bytes() } else { Vec::new() },
                3 => {
                    let (q0, q1) = nd.addr;
                    let seg = &nd.data[q0.min(nd.data.len())..q1.min(nd.data.len())];
                    let o = (off as usize).min(seg.len());
                    seg[o..(o + n).min(seg.len())].to_vec()
                }
                _ => Vec::new(),
            };
            advance(chan, cur, out.len());
            Ok(RRes::Data(out))
        }
        (DevId::Snap, Node::SnapRoot) => {
            let mut skip = off as usize;
            let mut out = Vec::new();
            let mut ents: Vec<(String, u32, bool)> =
                vec![("ctl".into(), 0, false)];
            ents.extend(k.borrow().snaps.iter().map(|(nm, t, _)| (nm.clone(), *t, true)));
            for (i, (nm, t, isdir)) in ents.iter().enumerate() {
                let rec = marshal_stat(&StatIn {
                    name: nm, uid: "eve", gid: "eve", qpath: 0x56_0000 + i as u64,
                    atime: *t, mtime: *t,
                    qtype: if *isdir { QTDIR } else { 0 },
                    mode: if *isdir { DMDIR | 0o555 } else { 0o666 },
                    ..Default::default()
                });
                if skip >= rec.len() {
                    skip -= rec.len();
                    continue;
                }
                if out.len() + rec.len() > n {
                    break;
                }
                out.extend_from_slice(&rec);
            }
            advance(chan, cur, out.len());
            Ok(RRes::Data(out))
        }
        (DevId::Snap, Node::SnapCtl) => {
            let mut txt = String::new();
            for (nm, t, _) in &k.borrow().snaps {
                txt.push_str(&format!("{} {}\n", nm, t));
            }
            let b = txt.into_bytes();
            let start = (off as usize).min(b.len());
            let end = (off as usize + n).min(b.len());
            let out = b[start..end].to_vec();
            advance(chan, cur, out.len());
            Ok(RRes::Data(out))
        }
        (DevId::Ram, Node::Ram(r)) => {
            let out = if r.borrow().dir {
                let mut skip = off as usize;
                let mut out = Vec::new();
                let kids: Vec<RamRef> = r.borrow().kids.iter().map(|(_, v)| v.clone()).collect();
                for kd in kids {
                    let rec = ram_stat(&kd);
                    if skip >= rec.len() {
                        skip -= rec.len();
                        continue;
                    }
                    if out.len() + rec.len() > n {
                        break;
                    }
                    out.extend_from_slice(&rec);
                }
                out
            } else {
                let d = r.borrow();
                let start = (off as usize).min(d.data.len());
                let end = (off as usize + n).min(d.data.len());
                d.data[start..end].to_vec()
            };
            advance(chan, cur, out.len());
            Ok(RRes::Data(out))
        }
        (DevId::Cons, Node::ConsUser) => {
            let euid = k.borrow().procs.get(&pid).map(|p| p.cred.euid.clone()).unwrap_or_default();
            let out = if off == 0 { euid.into_bytes() } else { Vec::new() };
            advance(chan, cur, out.len());
            Ok(RRes::Data(out))
        }
        (DevId::Cons, Node::ConsPid) => {
            let out = if off == 0 { pid.to_string().into_bytes() } else { Vec::new() };
            advance(chan, cur, out.len());
            Ok(RRes::Data(out))
        }
        (DevId::Cons, Node::ConsNull) => Ok(RRes::Data(Vec::new())),
        (DevId::Cons, Node::ConsCons) => {
            let now = {
                let mut kb = k.borrow_mut();
                if !kb.cons_buf.is_empty() || kb.cons_eof {
                    let take = n.min(kb.cons_buf.len());
                    Some(kb.cons_buf.drain(0..take).collect::<Vec<u8>>())
                } else {
                    None
                }
            };
            if let Some(give) = now {
                advance(chan, cur, give.len());
                return Ok(RRes::Data(give));
            }
            let (c, w) = oneshot::<RRes>();
            {
                let mut kb = k.borrow_mut();
                kb.cons_parked.push(Waiter { n, kind: WaitKind::Wake { pid, c: c.clone() } });
                if let Some(p) = kb.procs.get_mut(&pid) {
                    p.inflight = Some(c);
                }
            }
            let r = w.await;
            if let RRes::Data(d) = &r {
                advance(chan, cur, d.len());
            }
            Ok(r)
        }
        (DevId::Pipe, Node::Pipe { p, end }) => {
            let d = 1 ^ end;
            let now = {
                let mut pb = p.borrow_mut();
                if pb.nbytes[d] > 0 {
                    Some(pipe_drain(&mut pb, d, n))
                } else if pb.refs[d] == 0 {
                    Some(Vec::new()) // EOF
                } else {
                    None
                }
            };
            if let Some(give) = now {
                advance(chan, cur, give.len());
                return Ok(RRes::Data(give));
            }
            let (c, w) = oneshot::<RRes>();
            {
                p.borrow_mut().parked[d].push(Waiter { n, kind: WaitKind::Wake { pid, c: c.clone() } });
                if let Some(pr) = k.borrow_mut().procs.get_mut(&pid) {
                    pr.inflight = Some(c);
                }
            }
            let r = w.await;
            if let RRes::Data(dd) = &r {
                advance(chan, cur, dd.len());
            }
            Ok(r)
        }
        (DevId::Env, Node::EnvVar(name)) => {
            let env = k.borrow().procs.get(&pid).ok_or("no proc")?.env.clone();
            let data = env.borrow().get(&name).cloned().unwrap_or_default();
            let start = (off as usize).min(data.len());
            let end = (off as usize + n).min(data.len());
            let out = data[start..end].to_vec();
            advance(chan, cur, out.len());
            Ok(RRes::Data(out))
        }
        (DevId::Env, Node::EnvRoot) => {
            let env = k.borrow().procs.get(&pid).ok_or("no proc")?.env.clone();
            let mut skip = off as usize;
            let mut out = Vec::new();
            let entries: Vec<(String, usize)> =
                env.borrow().iter().map(|(kk, v)| (kk.clone(), v.len())).collect();
            for (kk, vlen) in entries {
                let rec = marshal_stat(&StatIn {
                    name: &kk, mode: 0o664, length: vlen as u64, ..Default::default()
                });
                if skip >= rec.len() { skip -= rec.len(); continue; }
                if out.len() + rec.len() > n { break; }
                out.extend_from_slice(&rec);
            }
            advance(chan, cur, out.len());
            Ok(RRes::Data(out))
        }
        (DevId::Proc, Node::Proc { kind: 6, pid: q }) => {
            if off > 0 {
                return Ok(RRes::Data(Vec::new()));
            }
            let s = {
                let kb = k.borrow();
                kb.procs.get(&q).map(|tp| format!("{}\n", tp.argv.join(" "))).unwrap_or_default()
            };
            let out = s.into_bytes();
            advance(chan, cur, out.len());
            Ok(RRes::Data(out))
        }
        (DevId::Proc, Node::Proc { kind: 3, pid: q }) => {
            if off > 0 {
                return Ok(RRes::Data(Vec::new()));
            }
            let s = {
                let kb = k.borrow();
                kb.procs.get(&q)
                    .map(|tp| format!("{} {} {} {}\n", tp.pid, tp.cred.euid, tp.cred.ruid, tp.ppid))
                    .unwrap_or_default()
            };
            let out = s.into_bytes();
            advance(chan, cur, out.len());
            Ok(RRes::Data(out))
        }
        (DevId::Web, Node::Web(url)) => {
            enum Now { Data(Vec<u8>), Err(String), Park }
            let now = {
                let mut kb = k.borrow_mut();
                match kb.web.get_mut(&url) {
                    Some(e) => match &e.state {
                        WebState::Done(Ok(body)) => {
                            let start = (off as usize).min(body.len());
                            let end = (off as usize + n).min(body.len());
                            Now::Data(body[start..end].to_vec())
                        }
                        WebState::Done(Err(m)) => Now::Err(m.clone()),
                        WebState::Pending => Now::Park,
                    },
                    None => Now::Err("not opened".into()),
                }
            };
            match now {
                Now::Data(d) => {
                    advance(chan, cur, d.len());
                    Ok(RRes::Data(d))
                }
                Now::Err(m) => Err(m),
                Now::Park => {
                    let (c, wt) = oneshot::<RRes>();
                    k.borrow_mut().web.get_mut(&url).unwrap().waiters.push((n, off, c));
                    let r = wt.await;
                    if let RRes::Data(ref d) = r {
                        advance(chan, cur, d.len());
                    }
                    Ok(r)
                }
            }
        }
        (DevId::Srv, Node::SrvRoot) => {
            let recs: Vec<Vec<u8>> = {
                let kb = k.borrow();
                kb.srv_posts.iter().map(|(nm, post)| marshal_stat(&StatIn {
                    name: nm, qpath: post.qpath, mode: 0o600, uid: &post.uid,
                    gid: &post.uid, ..Default::default()
                })).collect()
            };
            let mut skip = off as usize;
            let mut out = Vec::new();
            for rec in recs {
                if skip >= rec.len() { skip -= rec.len(); continue; }
                if out.len() + rec.len() > n { break; }
                out.extend_from_slice(&rec);
            }
            advance(chan, cur, out.len());
            Ok(RRes::Data(out))
        }
        (DevId::Proc, Node::Proc { kind: 0, .. }) => {
            // read(5)'s integral-records rule over the live pid list
            let mut pids: Vec<Pid> = k.borrow().procs.keys().cloned().collect();
            pids.sort_unstable();
            let mut skip = off as usize;
            let mut out = Vec::new();
            for q in pids {
                let rec = marshal_stat(&StatIn {
                    name: &q.to_string(), qtype: QTDIR, qpath: q as u64,
                    mode: DMDIR | 0o555, ..Default::default()
                });
                if skip >= rec.len() { skip -= rec.len(); continue; }
                if out.len() + rec.len() > n { break; }
                out.extend_from_slice(&rec);
            }
            advance(chan, cur, out.len());
            Ok(RRes::Data(out))
        }
        // a pid's own directory. It was walkable but not listable, so /proc/N
        // read as empty — which a process manager notices immediately.
        (DevId::Proc, Node::Proc { kind: 1, pid: q }) => {
            if !k.borrow().procs.contains_key(&q) { return Ok(RRes::Data(Vec::new())); }
            let mut skip = off as usize;
            let mut out = Vec::new();
            for (nm, qp) in [("ctl", 2u64), ("status", 3), ("note", 4), ("notepg", 5), ("args", 6)] {
                let rec = marshal_stat(&StatIn {
                    name: nm, qpath: ((q as u64) << 8) | qp, mode: 0o600, ..Default::default()
                });
                if skip >= rec.len() { skip -= rec.len(); continue; }
                if out.len() + rec.len() > n { break; }
                out.extend_from_slice(&rec);
            }
            advance(chan, cur, out.len());
            Ok(RRes::Data(out))
        }
        (DevId::Proc, _) => Ok(RRes::Data(Vec::new())),
        (DevId::Union, Node::Union(list)) => {
            let mut skip = off as usize;
            let mut out = Vec::new();
            let els: Vec<MountEl> = list.iter().cloned().collect();
            'outer: for el in els {
                let listing = list_dir(k, &el.dn, pid).await?;
                let mut o = 0usize;
                while o + 2 <= listing.len() {
                    let size = u16::from_le_bytes([listing[o], listing[o + 1]]) as usize + 2;
                    let rec = &listing[o..(o + size).min(listing.len())];
                    o += size;
                    if skip >= rec.len() { skip -= rec.len(); continue; }
                    if out.len() + rec.len() > n { break 'outer; }
                    out.extend_from_slice(rec);
                }
            }
            advance(chan, cur, out.len());
            Ok(RRes::Data(out))
        }
        (DevId::Wsys, Node::Wsys { kind, win, conn, ty }) => {
            let out = wsys_read(k, kind, &win, &conn, &ty, chan, n, off).await?;
            if let RRes::Data(d) = &out {
                advance(chan, cur, d.len());
            }
            Ok(out)
        }
        (DevId::Mnt, Node::Mnt(m)) => {
            let count = n.min(MSIZE - 24);
            let conn = m.conn.clone();
            let rb = rpc(k, &conn, tv::READ, W9::new().u32(m.fid).u64(off).u32(count as u32)).await?;
            let mut r = R9::new(&rb);
            let got = r.u32() as usize;
            let out = r.rest()[..got.min(r.rest().len())].to_vec();
            advance(chan, cur, out.len());
            Ok(RRes::Data(out))
        }
        _ => Err("read not supported here (v0)".into()),
    }
}

async fn list_dir(k: &K, dn: &DN, pid: Pid) -> Result<Vec<u8>, KErr> {
    // full listing of one union element; a mnt element clones + opens its
    // own fid, per the JS listDir
    let dn = if let Node::Mnt(m) = &dn.node {
        let cloned = mnt_clone(k, m).await?;
        mnt_open(k, &cloned, 0).await?;
        DN { dev: DevId::Mnt, node: Node::Mnt(cloned), path: None }
    } else {
        dn.clone()
    };
    let chan = Rc::new(RefCell::new(Chan {
        dev: dn.dev, node: dn.node.clone(), path: None, mode: 0, offset: 0, refs: 1,
    }));
    let mut out = Vec::new();
    let mut off = 0u64;
    loop {
        match Box::pin(dev_read_async(k, &chan, 8192, off, pid)).await? {
            RRes::Data(chunk) if chunk.is_empty() => break,
            RRes::Data(chunk) => {
                off += chunk.len() as u64;
                out.extend_from_slice(&chunk);
            }
            RRes::Intr => return Err("interrupted".into()),
        }
    }
    if let Node::Mnt(m) = &dn.node {
        clunk_fid(k, &m.conn.clone(), m.fid);
    }
    Ok(out)
}

async fn mnt_clone(k: &K, m: &MntRef) -> Result<MntRef, KErr> {
    // Twalk, zero names: a fresh fid — the attach fid is never consumed
    let conn = m.conn.clone();
    let newfid = {
        let mut cb = conn.borrow_mut();
        let f = cb.nextfid;
        cb.nextfid += 1;
        f
    };
    rpc(k, &conn, tv::WALK, W9::new().u32(m.fid).u32(newfid).u16(0)).await?;
    Ok(Rc::new(MntNode {
        conn, fid: newfid, qtype: m.qtype,
        ephemeral: std::cell::Cell::new(true),
        opened: std::cell::Cell::new(false),
    }))
}

async fn mnt_open(k: &K, m: &MntRef, mode: u32) -> Result<(), KErr> {
    let conn = m.conn.clone();
    rpc(k, &conn, tv::OPEN, W9::new().u32(m.fid).u8(mode as u8)).await?;
    m.ephemeral.set(false);
    m.opened.set(true);
    Ok(())
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

fn pipe_serve(k: &K, p: &PipeR, d: usize) {
    loop {
        let fired = {
            let mut pb = p.borrow_mut();
            if pb.parked[d].is_empty() || (pb.nbytes[d] == 0 && pb.refs[d] != 0) {
                return;
            }
            let w = pb.parked[d].remove(0);
            let give = if pb.nbytes[d] > 0 { pipe_drain(&mut pb, d, w.n) } else { Vec::new() };
            (w.kind, give)
        };
        wake(k, fired.0, fired.1);
    }
}

// sync write (ram/cons/pipe/env/proc); mnt goes through dev_write_async
fn dev_write_sync(k: &K, chan: &ChanR, data: &[u8], off_in: u64, pid: Pid) -> Result<usize, KErr> {
    let (dev, node, cur_off) = {
        let c = chan.borrow();
        (c.dev, c.node.clone(), c.offset)
    };
    let cur = off_in == u64::MAX;
    let off = if cur { cur_off } else { off_in };
    let wrote = match (dev, node) {
        (DevId::Srv, Node::SrvName(nm)) => {
            let already = k.borrow().srv_posts.get(nm.as_str()).and_then(|p| p.chan.clone()).is_some();
            if already {
                return Err("already posted".into());
            }
            let s = String::from_utf8_lossy(data);
            let fd: i32 = s.trim().parse().map_err(|_| "srv: write the fd number")?;
            let c = {
                let kb = k.borrow();
                let p = kb.procs.get(&pid).ok_or("no proc")?;
                let f = p.fdt.borrow();
                f.fds.get(fd as usize).and_then(|o| o.clone()).ok_or("srv: no such fd")?
            };
            c.borrow_mut().refs += 1;      // the name holds a reference
            if let Some(post) = k.borrow_mut().srv_posts.get_mut(nm.as_str()) {
                post.chan = Some(c);
            }
            data.len()
        }

        (DevId::Ram, Node::Ram(r)) => {
            let off = off as usize;
            let mut n = r.borrow_mut();
            if n.dir {
                return Err("write on directory".into());
            }
            if n.ro {
                return Err("read-only snapshot".into());
            }
            let end = off + data.len();
            let dat = Rc::make_mut(&mut n.data);
            if end > dat.len() {
                dat.resize(end, 0);
            }
            dat[off..end].copy_from_slice(data);
            n.mtime = now_secs();
            data.len()
        }
        (DevId::Snap, Node::SnapCtl) => {
            let txt = String::from_utf8_lossy(data);
            let mut it = txt.split_whitespace();
            match it.next() {
                Some("snap") => {
                    let nm = match it.next() {
                        Some(x) => x.to_string(),
                        None => format!("s{}", k.borrow().snaps.len() + 1),
                    };
                    if k.borrow().snaps.iter().any(|(n2, _, _)| n2 == &nm) {
                        return Err(format!("snapshot '{}' exists", nm));
                    }
                    let root = k.borrow().ram_root.clone();
                    let tree = snap_tree(&root);
                    k.borrow_mut().snaps.push((nm, now_secs(), tree));
                }
                Some("del") => {
                    let nm = it.next().ok_or("usage: del name")?.to_string();
                    let before = k.borrow().snaps.len();
                    k.borrow_mut().snaps.retain(|(n2, _, _)| n2 != &nm);
                    if k.borrow().snaps.len() == before {
                        return Err(format!("no snapshot '{}'", nm));
                    }
                }
                _ => return Err("usage: snap [name] | del name".into()),
            }
            data.len()
        }
        (DevId::Cons, Node::ConsNull) => data.len(),
        (DevId::Cons, _) => {
            k.borrow_mut().effects.push(Effect::ConsWrite(data.to_vec()));
            data.len()
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
            pipe_serve(k, &p, end);
            data.len()
        }
        (DevId::Env, Node::EnvVar(name)) => {
            let env = k.borrow().procs.get(&pid).ok_or("no proc")?.env.clone();
            env.borrow_mut().insert(name.clone(), data.to_vec());
            data.len()
        }
        (DevId::Proc, Node::Proc { kind, pid: q }) if kind >= 2 => {
            proc_write(k, kind, q, data, pid)?
        }
        (DevId::Wsys, Node::Wsys { kind, win, conn, .. }) => {
            wsys_write(k, kind, &win, &conn, data)?
        }
        (DevId::Wsys, Node::Cv { win, id, file }) => {
            let mut wb = win.borrow_mut();
            let cv = wb.cv.as_mut().ok_or("no canvas")?;
            let nd = cv.nodes.get_mut(&id).ok_or("node gone")?;
            match file {
                1 => {
                    for line in String::from_utf8_lossy(data).split('\n') {
                        if let Some(eq) = line.find('=') {
                            let (a, v) = (line[..eq].trim().to_string(), line[eq + 1..].trim().to_string());
                            if a.is_empty() { continue; }
                            if let Some(slot) = nd.attrs.iter_mut().find(|(k2, _)| *k2 == a) {
                                slot.1 = v;
                            } else {
                                nd.attrs.push((a, v));
                            }
                        }
                    }
                }
                2 => {
                    let t = String::from_utf8_lossy(data).trim().to_string();
                    let end = nd.data.len();
                    if t == "$" {
                        nd.addr = (end, end);
                    } else {
                        let (a, b) = match t.split_once(',') {
                            Some((x, y)) => (x.to_string(), y.to_string()),
                            None => (t.clone(), t.clone()),
                        };
                        let q0 = a.parse::<usize>().map_err(|_| "bad addr")?.min(end);
                        let q1 = if b == "$" { end } else { b.parse::<usize>().map_err(|_| "bad addr")?.min(end) };
                        if q1 < q0 { return Err("addr reversed".into()); }
                        nd.addr = (q0, q1);
                    }
                }
                3 => {
                    let (q0, q1) = nd.addr;
                    let repl: Vec<u8> = data.to_vec();
                    let end = q0 + repl.len();
                    nd.data.splice(q0..q1, repl);
                    nd.addr = (end, end);
                }
                _ => return Err("read-only canvas file".into()),
            }
            data.len()
        }
        _ => return Err("write not supported here (v0)".into()),
    };
    if cur {
        chan.borrow_mut().offset += wrote as u64;
    }
    Ok(wrote)
}

// the host's read: walk a path in a namespace and take the whole file.
// The host decides WHAT to open and WHEN, and renders it — which is the
// property that matters; the kernel is only the filesystem.
async fn read_whole(k: &K, pid: Pid, path: &str) -> Result<Vec<u8>, KErr> {
    let dn = walk(k, pid, path, false).await?;
    let chan = Rc::new(RefCell::new(Chan {
        dev: dn.dev, node: dn.node, path: Some(path.to_string()),
        mode: 0, offset: 0, refs: 1,
    }));
    let mut out: Vec<u8> = Vec::new();
    let mut off: u64 = 0;
    loop {
        match dev_read_async(k, &chan, 65536, off, pid).await? {
            RRes::Data(b) => {
                if b.is_empty() { break; }
                off += b.len() as u64;
                out.extend_from_slice(&b);
            }
            RRes::Intr => break,
        }
        if out.len() > 4_000_000 { break; }   // a surface is not a pipe
    }
    Ok(out)
}

// Put: the surface hands the edited file back. Truncating, because the buffer
// IS the file now — not an append.
async fn write_whole(k: &K, pid: Pid, path: &str, data: &[u8]) -> Result<usize, KErr> {
    let dn = walk(k, pid, path, false).await?;
    open_perm(k, &dn, 1 | OTRUNC, pid).await?;          // OWRITE | OTRUNC
    let chan = Rc::new(RefCell::new(Chan {
        dev: dn.dev, node: dn.node, path: Some(path.to_string()),
        mode: 1, offset: 0, refs: 1,
    }));
    let mut off: u64 = 0;
    let mut wrote: usize = 0;
    while wrote < data.len() {
        let n = dev_write_async(k, &chan, &data[wrote..], off, pid).await?;
        if n == 0 { break; }
        wrote += n;
        off += n as u64;
    }
    Ok(wrote)
}

async fn dev_write_async(k: &K, chan: &ChanR, data: &[u8], off_in: u64, pid: Pid) -> Result<usize, KErr> {
    let mnt = {
        let c = chan.borrow();
        match &c.node {
            Node::Mnt(m) => Some(m.clone()),
            _ => None,
        }
    };
    if let Some(m) = mnt {
        let cur = off_in == u64::MAX;
        let off = if cur { chan.borrow().offset } else { off_in };
        let d = &data[..data.len().min(MSIZE - 24)];
        let conn = m.conn.clone();
        let rb = rpc(k, &conn, tv::WRITE,
                     W9::new().u32(m.fid).u64(off).u32(d.len() as u32).raw(d)).await?;
        let wrote = R9::new(&rb).u32() as usize;
        if cur {
            chan.borrow_mut().offset += wrote as u64;
        }
        return Ok(wrote);
    }
    let hostp = {
        let c = chan.borrow();
        match &c.node {
            Node::Host(hp) => Some((hp.clone(), c.offset)),
            _ => None,
        }
    };
    if let Some((hp, cur_off)) = hostp {
        let cur = off_in == u64::MAX;
        let off = if cur { cur_off } else { off_in };
        hostq(k, HostOp::Write { path: hpstr(&hp), off, data: data.to_vec() }).await?;
        if cur {
            chan.borrow_mut().offset += data.len() as u64;
        }
        return Ok(data.len());
    }
    dev_write_sync(k, chan, data, off_in, pid)
}

fn proc_write(k: &K, kind: u8, q: Pid, data: &[u8], pid: Pid) -> Result<usize, KErr> {
    let (cred, tcred, eve) = {
        let kb = k.borrow();
        let cred = kb.procs.get(&pid).ok_or("no proc")?.cred.clone();
        let tcred = kb.procs.get(&q).ok_or("process gone")?.cred.clone();
        (cred, tcred, kb.eve.clone())
    };
    if cred.euid != eve && cred.euid != tcred.euid {
        return Err("not your process".into());
    }
    let text = String::from_utf8_lossy(data).trim().to_string();
    match kind {
        4 => {
            postnote(k, q, &text);
            Ok(data.len())
        }
        5 => {
            // the group, writer excepted (pgrpnote's rule)
            let targets: Vec<Pid> = {
                let kb = k.borrow();
                let group = kb.procs.get(&q).map(|t| t.note_group).unwrap_or(0);
                kb.procs.values().filter(|t| t.note_group == group && t.pid != pid).map(|t| t.pid).collect()
            };
            for t in targets {
                postnote(k, t, &text);
            }
            Ok(data.len())
        }
        _ => {
            let mut it = text.split_whitespace();
            let (verb, arg) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
            if verb != "user" || arg.is_empty() {
                return Err(format!("bad ctl message '{}'", text));
            }
            let mut kb = k.borrow_mut();
            let tp = kb.procs.get_mut(&q).ok_or("process gone")?;
            if cred.euid == eve {
                tp.cred.euid = arg.into(); // rule 1: eve -> anyone
                tp.cred.ruid = arg.into();
            } else if arg == tp.cred.ruid {
                tp.cred.euid = arg.into(); // rule 2: back to ruid
            } else {
                return Err(format!("'{}' may not become '{}'", cred.euid, arg));
            }
            Ok(data.len())
        }
    }
}

fn dev_len(chan: &ChanR) -> u64 {
    let c = chan.borrow();
    match (&c.dev, &c.node) {
        (DevId::Ram, Node::Ram(r)) => {
            let n = r.borrow();
            if n.dir { 0 } else { n.data.len() as u64 }
        }
        (DevId::Host, Node::Host(_)) => 0, // length is the host's; seek(2) end-relative on '#Z' reads as 0 (recorded)
        (DevId::Wsys, Node::Wsys { kind: WKind::Rgb, win: Some(w), .. }) => {
            let img = w.borrow().img.clone();
            let back = img.borrow().back.clone();
            let n = back.borrow().data.len() as u64;
            n
        }
        _ => 0,
    }
}

fn clunk(k: &K, chan: &ChanR) {
    let node = {
        let mut c = chan.borrow_mut();
        c.refs -= 1;
        if c.refs > 0 {
            return;
        }
        c.node.clone()
    };
    match node {
        Node::Pipe { p, end } => {
            {
                let mut pb = p.borrow_mut();
                pb.refs[end] -= 1;
            }
            let dry = p.borrow().refs[end] == 0;
            if dry {
                pipe_serve(k, &p, end); // wake readers: data then EOF
            }
        }
        Node::Mnt(m) => clunk_fid(k, &m.conn.clone(), m.fid),
        _ => {}
    }
}

fn fdt_close(k: &K, fdt: &FdtR) {
    let done = {
        let mut f = fdt.borrow_mut();
        f.refs -= 1;
        f.refs == 0
    };
    if done {
        let fds: Vec<ChanR> = fdt.borrow_mut().fds.drain(..).flatten().collect();
        for c in fds {
            clunk(k, &c);
        }
    }
}

fn fdt_copy(fdt: &FdtR) -> FdtR {
    let fds: Vec<Option<ChanR>> = fdt.borrow().fds.clone();
    for c in fds.iter().flatten() {
        c.borrow_mut().refs += 1;
    }
    Rc::new(RefCell::new(Fdt { refs: 1, fds }))
}

fn fd_alloc(k: &K, pid: Pid, chan: ChanR, at: Option<usize>) -> i32 {
    let fdt = k.borrow().procs.get(&pid).unwrap().fdt.clone();
    if let Some(at) = at {
        let old = {
            let mut f = fdt.borrow_mut();
            if f.fds.len() <= at {
                f.fds.resize(at + 1, None);
            }
            f.fds[at].take()
        };
        if let Some(old) = old {
            clunk(k, &old);
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

fn fdchk(k: &K, pid: Pid, fd: i32) -> Result<ChanR, KErr> {
    let fdt = k.borrow().procs.get(&pid).ok_or("no proc")?.fdt.clone();
    if fd < 0 {
        return Err(format!("fd {} not open", fd));
    }
    let c = fdt.borrow().fds.get(fd as usize).cloned().flatten();
    c.ok_or_else(|| format!("fd {} not open", fd))
}

async fn read_all(k: &K, dn: &DN, pid: Pid) -> Result<Vec<u8>, KErr> {
    // exec's image read; open first if the dev needs it (mnt does)
    let dn = if let Node::Mnt(m) = &dn.node {
        if !m.opened.get() {
            mnt_open(k, m, 0).await?;
        }
        dn.clone()
    } else {
        dn.clone()
    };
    let chan = Rc::new(RefCell::new(Chan {
        dev: dn.dev, node: dn.node.clone(), path: None, mode: 0, offset: 0, refs: 1,
    }));
    let mut out = Vec::new();
    let mut off = 0u64;
    loop {
        match dev_read_async(k, &chan, 65536, off, pid).await? {
            RRes::Data(chunk) if chunk.is_empty() => break,
            RRes::Data(chunk) => {
                off += chunk.len() as u64;
                out.extend_from_slice(&chunk);
            }
            RRes::Intr => return Err("interrupted".into()),
        }
    }
    if let Node::Mnt(m) = &dn.node {
        clunk_fid(k, &m.conn.clone(), m.fid);
    }
    Ok(out)
}

fn txstr(tx: &[u8], off: usize) -> String {
    if off >= tx.len() {
        return String::new();
    }
    let end = tx[off..].iter().position(|&b| b == 0).map(|i| off + i).unwrap_or(tx.len());
    String::from_utf8_lossy(&tx[off..end]).into_owned()
}

// ---- the dispatcher ----
async fn dispatch(k: &K, ex: &Rc<LocalExec>, worker_pid: Pid, pid: Pid, trap: i32,
                  a: [i32; 5], tx: Vec<u8>) -> KRes {
    use t::*;
    match trap {
        BIND => {
            if k.borrow().procs.get(&pid).ok_or("no proc")?.nomnt {
                return Err("mounting disallowed (RFNOMNT)".into());
            }
            let name = txstr(&tx, 0);
            let old = txstr(&tx, name.len() + 1);
            let src = walk(k, pid, &name, false).await?; // resolved now, per bind(2)
            let cwd = k.borrow().procs.get(&pid).unwrap().cwd.clone();
            let oldc = canon(&old, &cwd);
            ns_insert(k, pid, &oldc, src, a[2]).await?;
            Ok(ok(0))
        }
        MOUNT => {
            if k.borrow().procs.get(&pid).ok_or("no proc")?.nomnt {
                return Err("mounting disallowed (RFNOMNT)".into());
            }
            let old = txstr(&tx, 0);
            let aname = txstr(&tx, old.len() + 1);
            let c = fdchk(k, pid, a[0])?;
            c.borrow_mut().refs += 1; // the kernel holds its own reference
            let conn = Rc::new(RefCell::new(ConnSt {
                chan: c, tags: HashMap::new(), expect: HashMap::new(),
                nexttag: 1, nextfid: 1, dead: None,
            }));
            ex.spawn(conn_reader(k.clone(), conn.clone()));
            let rb = rpc(k, &conn, tv::VERSION, W9::new().u32(MSIZE as u32).s("9P2000")).await?;
            let mut r = R9::new(&rb);
            let _msize = r.u32();
            let ver = r.s();
            if ver != "9P2000" {
                return Err(format!("server speaks '{}', not 9P2000", ver));
            }
            let uname = k.borrow().procs.get(&pid).unwrap().cred.euid.clone();
            let fid = {
                let mut cb = conn.borrow_mut();
                let f = cb.nextfid;
                cb.nextfid += 1;
                f
            };
            let rb = rpc(k, &conn, tv::ATTACH,
                         W9::new().u32(fid).u32(NOFID).s(&uname).s(&aname)).await?;
            let (qt, _, _) = R9::new(&rb).qid();
            let node = Node::Mnt(Rc::new(MntNode {
                conn, fid, qtype: qt,
                ephemeral: std::cell::Cell::new(false),
                opened: std::cell::Cell::new(false),
            }));
            let cwd = k.borrow().procs.get(&pid).unwrap().cwd.clone();
            let oldc = canon(&old, &cwd);
            ns_insert(k, pid, &oldc, DN { dev: DevId::Mnt, node, path: None }, a[3]).await?;
            Ok(ok(0))
        }
        UNMOUNT => {
            let name = txstr(&tx, 0);
            let old = txstr(&tx, name.len() + 1);
            if k.borrow().procs.get(&pid).ok_or("no proc")?.nomnt {
                return Err("mounting disallowed (RFNOMNT)".into());
            }
            let cwd = k.borrow().procs.get(&pid).unwrap().cwd.clone();
            let key = canon(&old, &cwd);
            let ns = k.borrow().procs.get(&pid).unwrap().ns.clone();
            let have = ns.borrow().get(&key).cloned();
            let Some(mut list) = have else {
                return Err(format!("'{}' is not a mount point", key));
            };
            if name.is_empty() {
                ns.borrow_mut().remove(&key);
                return Ok(ok(0));
            }
            let nm = if name.starts_with('#') { name.clone() } else { canon(&name, &cwd) };
            let mut i = list.iter().position(|el| el.dn.path.as_deref() == Some(nm.as_str()));
            if i.is_none() {
                let src = walk(k, pid, &name, false).await?;
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
            Ok(ok(0))
        }
        CHDIR => {
            let path = txstr(&tx, 0);
            let cwd = k.borrow().procs.get(&pid).unwrap().cwd.clone();
            let full = canon(&path, &cwd);
            walk(k, pid, &full, false).await?;
            k.borrow_mut().procs.get_mut(&pid).unwrap().cwd = full;
            Ok(ok(0))
        }
        CLOSE => {
            let c = fdchk(k, pid, a[0])?;
            let fdt = k.borrow().procs.get(&pid).unwrap().fdt.clone();
            fdt.borrow_mut().fds[a[0] as usize] = None;
            clunk(k, &c);
            Ok(ok(0))
        }
        DUP => {
            let c = fdchk(k, pid, a[0])?;
            c.borrow_mut().refs += 1;
            let fd = if a[1] >= 0 {
                fd_alloc(k, pid, c, Some(a[1] as usize))
            } else {
                fd_alloc(k, pid, c, None)
            };
            Ok(ok(fd))
        }
        OPEN => {
            let path = txstr(&tx, 0);
            let dn = walk(k, pid, &path, false).await?;
            if let Node::DupFd(target) = &dn.node {
                target.borrow_mut().refs += 1;
                let fd = fd_alloc(k, pid, target.clone(), None);
                return Ok(ok(fd));
            }
            if let Node::Web(url) = &dn.node {
                let mut kb = k.borrow_mut();
                if !kb.web.contains_key(url) {
                    kb.web.insert(url.clone(), WebEntry { state: WebState::Pending, waiters: Vec::new() });
                    let u = url.clone();
                    kb.effects.push(Effect::Fetch { url: u });
                }
            }
            if let Node::SrvName(nm) = &dn.node {
                let posted = k.borrow().srv_posts.get(nm.as_str()).and_then(|p| p.chan.clone());
                if let Some(target) = posted {
                    target.borrow_mut().refs += 1;
                    let fd = fd_alloc(k, pid, target, None);
                    return Ok(ok(fd));
                }
                // an unposted name opens as itself (the poster's own open)
            }
            // wsys: opening draw/new mints a fresh connection; image 0 is
            // the window itself
            let dn = if let Node::Wsys { kind: WKind::DrawNew, win: Some(w), .. } = &dn.node {
                let conn = {
                    let mut wb = w.borrow_mut();
                    let id = wb.nextconn;
                    wb.nextconn += 1;
                    let mut images = HashMap::new();
                    images.insert(0u32, wb.img.clone());
                    let c = Rc::new(RefCell::new(DConn { id, images, screens: HashMap::new() }));
                    wb.conns.insert(id, c.clone());
                    c
                };
                DN {
                    dev: DevId::Wsys,
                    node: Node::Wsys { kind: WKind::DrawNew, win: Some(w.clone()), conn: Some(conn), ty: None },
                    path: dn.path.clone(),
                }
            } else {
                dn
            };
            // mnt: clone before open, so the attach fid is never consumed
            let dn = if let Node::Mnt(m) = &dn.node {
                let m2 = if m.ephemeral.get() { m.clone() } else { mnt_clone(k, m).await? };
                mnt_open(k, &m2, a[1] as u32 & 0x0f).await?;
                DN { dev: DevId::Mnt, node: Node::Mnt(m2), path: dn.path.clone() }
            } else {
                open_perm(k, &dn, a[1] as u32, pid).await?;
                dn
            };
            let cwd = k.borrow().procs.get(&pid).unwrap().cwd.clone();
            let chan = Rc::new(RefCell::new(Chan {
                dev: dn.dev, node: dn.node, path: Some(canon(&path, &cwd)),
                mode: a[1] as u32, offset: 0, refs: 1,
            }));
            let fd = fd_alloc(k, pid, chan, None);
            Ok(ok(fd))
        }
        CREATE => {
            let cpath = txstr(&tx, 0);
            let mode = a[1] as u32;
            let perm = a[2] as u32;
            let isdir = perm & DMDIR != 0;
            if !isdir {
                if let Ok(dn) = walk(k, pid, &cpath, false).await {
                    // create(2): an existing file opens and truncates
                    if let Node::Mnt(m) = &dn.node {
                        let m2 = if m.ephemeral.get() { m.clone() } else { mnt_clone(k, m).await? };
                        mnt_open(k, &m2, (mode & 0x0f) | OTRUNC).await?;
                        let chan = Rc::new(RefCell::new(Chan {
                            dev: DevId::Mnt, node: Node::Mnt(m2), path: Some(cpath.clone()),
                            mode, offset: 0, refs: 1,
                        }));
                        return Ok(ok(fd_alloc(k, pid, chan, None)));
                    }
                    open_perm(k, &dn, mode | OTRUNC, pid).await?;
                    if let Node::Ram(r) = &dn.node {
                        Rc::make_mut(&mut r.borrow_mut().data).clear();
                    }
                    let chan = Rc::new(RefCell::new(Chan {
                        dev: dn.dev, node: dn.node, path: Some(cpath.clone()),
                        mode, offset: 0, refs: 1,
                    }));
                    return Ok(ok(fd_alloc(k, pid, chan, None)));
                }
            }
            let (parent, base) = walk_parent(k, pid, &cpath).await?;
            let parent = match (&parent.dev, &parent.node) {
                (DevId::Union, Node::Union(list)) => list
                    .iter()
                    .find(|e| e.create)
                    .map(|e| e.dn.clone())
                    .ok_or("create in a union needs an element bound with -c (MCREATE)")?,
                _ => parent,
            };
            if let Node::EnvRoot = &parent.node {
                let env = k.borrow().procs.get(&pid).ok_or("no proc")?.env.clone();
                env.borrow_mut().insert(base.clone(), Vec::new());
                let chan = Rc::new(RefCell::new(Chan {
                    dev: DevId::Env, node: Node::EnvVar(base), path: Some(cpath),
                    mode, offset: 0, refs: 1,
                }));
                return Ok(ok(fd_alloc(k, pid, chan, None)));
            }
            if let Node::Mnt(m) = &parent.node {
                // Tcreate: the fid comes to represent (and opens) the new file
                let m2 = if m.ephemeral.get() { m.clone() } else { mnt_clone(k, m).await? };
                let conn = m2.conn.clone();
                rpc(k, &conn, tv::CREATE,
                    W9::new().u32(m2.fid).s(&base).u32(perm).u8((mode & 0x0f) as u8)).await?;
                m2.ephemeral.set(false);
                m2.opened.set(true);
                let chan = Rc::new(RefCell::new(Chan {
                    dev: DevId::Mnt, node: Node::Mnt(m2), path: Some(cpath),
                    mode, offset: 0, refs: 1,
                }));
                return Ok(ok(fd_alloc(k, pid, chan, None)));
            }
            let (umask, cred) = {
                let kb = k.borrow();
                let p = kb.procs.get(&pid).unwrap();
                (p.umask, p.cred.clone())
            };
            if let Node::SrvRoot = &parent.node {
                let cred = k.borrow().procs.get(&pid).ok_or("no proc")?.cred.clone();
                {
                    let mut kb = k.borrow_mut();
                    if kb.srv_posts.contains_key(&base) {
                        return Err(format!("'{}' in use", base));
                    }
                    let q = kb.qgen;
                    kb.qgen += 1;
                    kb.srv_posts.insert(base.clone(), SrvPost { qpath: q, uid: cred.euid.clone(), chan: None });
                }
                let chan = Rc::new(RefCell::new(Chan {
                    dev: DevId::Srv, node: Node::SrvName(base.clone()), path: Some(cpath),
                    mode, offset: 0, refs: 1,
                }));
                return Ok(ok(fd_alloc(k, pid, chan, None)));
            }
            if let Node::Host(hp) = &parent.node {
                k.borrow().hostfs_root.clone().ok_or("no host root")?;
                let target = hp.join(&base);
                hostq(k, HostOp::Create {
                    path: hpstr(&target), dir: isdir, perm: perm & !umask & 0o777,
                }).await?;
                let chan = Rc::new(RefCell::new(Chan {
                    dev: DevId::Host, node: Node::Host(target), path: Some(cpath),
                    mode, offset: 0, refs: 1,
                }));
                return Ok(ok(fd_alloc(k, pid, chan, None)));
            }
            let node = ram_create(k, &parent, &base, perm & !umask, isdir, &cred)?;
            let chan = Rc::new(RefCell::new(Chan {
                dev: DevId::Ram, node: Node::Ram(node), path: Some(cpath),
                mode, offset: 0, refs: 1,
            }));
            Ok(ok(fd_alloc(k, pid, chan, None)))
        }
        REMOVE => {
            let path = txstr(&tx, 0);
            let (parent, base) = walk_parent(k, pid, &path).await?;
            if let Node::EnvRoot = &parent.node {
                let env = k.borrow().procs.get(&pid).ok_or("no proc")?.env.clone();
                env.borrow_mut().remove(&base);
                return Ok(ok(0));
            }
            if let Node::Mnt(_) = &parent.node {
                // walk to the name, then Tremove (which clunks, per remove(5))
                let target = dev_walk_one(k, &parent, &base, pid).await?
                    .ok_or_else(|| format!("'{}' does not exist", base))?;
                if let Node::Mnt(tm) = &target.node {
                    let conn = tm.conn.clone();
                    rpc(k, &conn, tv::REMOVE, W9::new().u32(tm.fid)).await?;
                }
                return Ok(ok(0));
            }
            if let Node::SrvRoot = &parent.node {
                let post = k.borrow_mut().srv_posts.remove(&base)
                    .ok_or_else(|| format!("'{}' does not exist", base))?;
                if let Some(c) = post.chan {
                    clunk(k, &c);
                }
                return Ok(ok(0));
            }
            if let Node::Host(hp) = &parent.node {
                let target = hp.join(&base);
                hostq(k, HostOp::Remove { path: hpstr(&target) }).await
                    .map_err(|e| format!("{}: {}", base, e))?;
                return Ok(ok(0));
            }
            let cred = k.borrow().procs.get(&pid).unwrap().cred.clone();
            if let Node::Ram(pr) = &parent.node {
                ram_access(k, pr, &cred, 2)?;
                let found = kid(pr, &base).ok_or_else(|| format!("'{}' does not exist", base))?;
                if found.borrow().dir && !found.borrow().kids.is_empty() {
                    return Err("directory not empty".into());
                }
                pr.borrow_mut().kids.retain(|(kk, _)| kk != &base);
                Ok(ok(0))
            } else {
                Err("remove not supported on this device".into())
            }
        }
        SEEK => {
            let c = fdchk(k, pid, a[0])?;
            let off = ((a[2] as i64) << 32) | (a[1] as u32 as i64);
            let len = dev_len(&c);
            let mut cb = c.borrow_mut();
            cb.offset = match a[3] {
                0 => off as u64,
                1 => (cb.offset as i64 + off) as u64,
                _ => (len as i64 + off) as u64,
            };
            Ok(ok(cb.offset as i32))
        }
        PREAD => {
            let c = fdchk(k, pid, a[0])?;
            let n = (a[2] as usize).min(TXSIZE);
            let cur = a[3] == -1 && a[4] == -1;
            let off = if cur { u64::MAX } else { ((a[4] as u32 as u64) << 32) | a[3] as u32 as u64 };
            match dev_read_async(k, &c, n, off, pid).await? {
                RRes::Data(data) => Ok(okd(data.len() as i32, data)),
                RRes::Intr => Err("interrupted".into()),
            }
        }
        PWRITE => {
            let c = fdchk(k, pid, a[0])?;
            let n = (a[2] as usize).min(TXSIZE).min(tx.len());
            let cur = a[3] == -1 && a[4] == -1;
            let off = if cur { u64::MAX } else { ((a[4] as u32 as u64) << 32) | a[3] as u32 as u64 };
            let wrote = dev_write_async(k, &c, &tx[..n], off, pid).await?;
            Ok(ok(wrote as i32))
        }
        STAT => {
            let path = txstr(&tx, 0);
            let dn = walk(k, pid, &path, a[3] == 1).await?; // a3: lstat's nofollow
            let rec = dev_stat(k, &dn, pid).await?;
            if let Node::Mnt(m) = &dn.node {
                if m.ephemeral.get() {
                    clunk_fid(k, &m.conn.clone(), m.fid);
                }
            }
            let n = rec.len() as i32;
            Ok(okd(n, rec))
        }
        FSTAT => {
            let c = fdchk(k, pid, a[0])?;
            let dn = {
                let cb = c.borrow();
                DN { dev: cb.dev, node: cb.node.clone(), path: None }
            };
            let rec = dev_stat(k, &dn, pid).await?;
            let n = rec.len() as i32;
            Ok(okd(n, rec))
        }
        WSTAT => {
            let path = txstr(&tx, 0);
            let rec = tx[path.len() + 1..(path.len() + 1 + a[2] as usize).min(tx.len())].to_vec();
            let st = parse_stat(&rec).ok_or("bad stat record")?;
            let (parent, base) = walk_parent(k, pid, &path).await?;
            let dn = walk(k, pid, &path, true).await?;
            if let Node::Mnt(m) = &dn.node {
                let conn = m.conn.clone();
                let body = W9::new().u32(m.fid).u16(rec.len() as u16).raw(&rec);
                rpc(k, &conn, tv::WSTAT, body).await?;
                if m.ephemeral.get() {
                    clunk_fid(k, &conn, m.fid);
                }
                return Ok(ok(0));
            }
            let (cred, eve) = {
                let kb = k.borrow();
                (kb.procs.get(&pid).ok_or("no proc")?.cred.clone(), kb.eve.clone())
            };
            let Node::Ram(node) = &dn.node else {
                return Err("wstat not supported on this device".into());
            };
            wstat_ram(node, &parent, &base, &st, &cred, &eve)?;
            Ok(ok(0))
        }
        FWSTAT => {
            let c = fdchk(k, pid, a[0])?;
            let rec = tx[..(a[2] as usize).min(tx.len())].to_vec();
            let st = parse_stat(&rec).ok_or("bad stat record")?;
            let (cred, eve) = {
                let kb = k.borrow();
                (kb.procs.get(&pid).ok_or("no proc")?.cred.clone(), kb.eve.clone())
            };
            enum Target {
                Ram(RamRef),
                Mnt(MntRef),
            }
            let target = {
                let cb = c.borrow();
                match &cb.node {
                    Node::Ram(r) => Target::Ram(r.clone()),
                    Node::Mnt(m) => Target::Mnt(m.clone()),
                    _ => return Err("wstat not supported on this device".into()),
                }
            };
            match target {
                Target::Mnt(m) => {
                    let conn = m.conn.clone();
                    let body = W9::new().u32(m.fid).u16(rec.len() as u16).raw(&rec);
                    rpc(k, &conn, tv::WSTAT, body).await?;
                    Ok(ok(0))
                }
                Target::Ram(node) => {
                    if st.mode != 0xffff_ffff {
                        if cred.euid != eve && cred.euid != node.borrow().uid {
                            return Err(format!("not owner of '{}'", node.borrow().name));
                        }
                        node.borrow_mut().mode = st.mode & (0o7777 | 0x000C_0000);
                    }
                    if !st.uid.is_empty() {
                        if cred.euid != eve {
                            return Err("only the host owner may chown (docs/identity.md D3)".into());
                        }
                        node.borrow_mut().uid = st.uid.clone();
                    }
                    Ok(ok(0))
                }
            }
        }
        ERRSTR => {
            let newe = txstr(&tx, 0);
            let mut kb = k.borrow_mut();
            let p = kb.procs.get_mut(&pid).ok_or("no proc")?;
            let olde = std::mem::replace(&mut p.errstr, newe);
            let cap = (a[1] as usize).max(1);
            let mut bytes: Vec<u8> = olde.into_bytes();
            bytes.truncate(cap - 1);
            bytes.push(0);
            Ok(okd(bytes.len() as i32 - 1, bytes))
        }
        FD2PATH => {
            let c = fdchk(k, pid, a[0])?;
            let path = c.borrow().path.clone().unwrap_or_default();
            let mut bytes = path.into_bytes();
            bytes.truncate((a[2] as usize).saturating_sub(1));
            bytes.push(0);
            Ok(okd(bytes.len() as i32 - 1, bytes))
        }
        NSEC => Ok(okd(8, now_nanos().to_le_bytes().to_vec())),
        SLEEP => {
            let ms = a[0].max(0) as u64;
            if ms == 0 {
                return Ok(ok(0));
            }
            let (c, w) = oneshot::<RRes>();
            {
                let mut kb = k.borrow_mut();
                let token = kb.next_token;
                kb.next_token += 1;
                kb.timers.insert(token, TimerKind::Sleep(c.clone()));
                kb.effects.push(Effect::Timer { ms, token });
                if let Some(p) = kb.procs.get_mut(&pid) {
                    p.inflight = Some(c);
                }
            }
            match w.await {
                RRes::Data(_) => Ok(ok(0)),
                RRes::Intr => Err("interrupted".into()),
            }
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
            let fd0 = fd_alloc(k, pid, mk(0), None);
            let fd1 = fd_alloc(k, pid, mk(1), None);
            let mut data = Vec::with_capacity(8);
            data.extend_from_slice(&fd0.to_le_bytes());
            data.extend_from_slice(&fd1.to_le_bytes());
            Ok(okd(0, data))
        }
        ARGS => {
            let kb = k.borrow();
            let p = kb.procs.get(&pid).ok_or("no proc")?;
            let mut block = Vec::new();
            for s in &p.argv {
                block.extend_from_slice(s.as_bytes());
                block.push(0);
            }
            block.truncate(a[1] as usize);
            Ok(okd(block.len() as i32, block))
        }
        NOTIFY => {
            k.borrow_mut().procs.get_mut(&pid).ok_or("no proc")?.has_handler = a[0] != 0;
            Ok(ok(0))
        }
        NOTEGET => {
            let mut kb = k.borrow_mut();
            let p = kb.procs.get_mut(&pid).ok_or("no proc")?;
            if p.notes.is_empty() {
                return Ok(ok(0));
            }
            let mut b = p.notes.remove(0).into_bytes();
            let n = b.len() as i32;
            b.push(0);
            Ok(okd(n, b))
        }
        NOTED => {
            if a[0] != 0 {
                return Ok(kill_proc(k, worker_pid, pid, "note: unhandled")); // NDFLT
            }
            Ok(ok(0)) // NCONT
        }
        ALARM => {
            let mut kb = k.borrow_mut();
            let tok = kb.procs.get_mut(&pid).ok_or("no proc")?.alarm_token.take();
            if let Some(tok) = tok {
                kb.timers.remove(&tok);
            }
            if a[0] > 0 {
                let token = kb.next_token;
                kb.next_token += 1;
                kb.procs.get_mut(&pid).unwrap().alarm_token = Some(token);
                kb.timers.insert(token, TimerKind::Alarm(pid));
                kb.effects.push(Effect::Timer { ms: a[0] as u64, token });
            }
            Ok(ok(0))
        }
        AREAD => {
            let c = fdchk(k, pid, a[1])?;
            let n = (a[2] as usize).min(TXSIZE - 4);
            let tag = a[0] as u32;
            // fire and forget: the read completes into the ioq; the async
            // read makes AREAD device-blind (mounts included) for free
            let k2 = k.clone();
            let vb = k.borrow().verbose;
            ex.spawn(async move {
                if vb {
                    eprintln!("[K aread task start pid={} tag={} fd={}]", pid, tag, a[1]);
                }
                let r = dev_read_async(&k2, &c, n, u64::MAX, pid).await;
                let data = match r {
                    Ok(RRes::Data(d)) => d,
                    Err(ref e) if vb => {
                        eprintln!("[K aread err: {}]", e);
                        Vec::new()
                    }
                    _ => Vec::new(),
                };
                if vb {
                    eprintln!("[K aread done pid={} tag={} n={}]", pid, tag, data.len());
                }
                aread_done(&k2, pid, tag, data);
            });
            Ok(ok(0))
        }
        IOWAIT => {
            {
                let mut kb = k.borrow_mut();
                let vb = kb.verbose;
                let p = kb.procs.get_mut(&pid).ok_or("no proc")?;
                if vb {
                    eprintln!("[K iowait pid={} ms={} ioq={}]", pid, a[2], p.ioq.len());
                }
                if let Some((tag, data)) = p.ioq.pop_front() {
                    let mut out = tag.to_le_bytes().to_vec();
                    out.extend_from_slice(&data);
                    let n = out.len() as i32;
                    return Ok(okd(n, out));
                }
            }
            let (c, w) = oneshot::<RRes>();
            {
                let mut kb = k.borrow_mut();
                let timer = if a[2] > 0 {
                    let token = kb.next_token;
                    kb.next_token += 1;
                    kb.timers.insert(token, TimerKind::Iowait(pid));
                    kb.effects.push(Effect::Timer { ms: a[2] as u64, token });
                    Some(token)
                } else {
                    None
                };
                let p = kb.procs.get_mut(&pid).unwrap();
                p.iowait = Some((c.clone(), timer));
                p.inflight = Some(c);
            }
            match w.await {
                RRes::Data(out) => Ok(okd(out.len() as i32, out)),
                RRes::Intr => Err("interrupted".into()),
            }
        }
        RFORK => rfork(k, worker_pid, pid, a[0], a[2]),
        EXEC => exec_call(k, worker_pid, pid, &tx, a[2]).await,
        EXITS => exits(k, worker_pid, pid, &tx),
        AWAIT => {
            {
                let mut kb = k.borrow_mut();
                let p = kb.procs.get_mut(&pid).ok_or("no proc")?;
                if a[2] == 1 && p.zombies.is_empty() {
                    return Ok(ok(0)); // nohang, nothing yet
                }
                let max = a[1] as usize;
                if !p.zombies.is_empty() {
                    let s = p.zombies.remove(0);
                    let mut bytes = s.into_bytes();
                    bytes.truncate(max.saturating_sub(1));
                    bytes.push(0);
                    let n = bytes.len() as i32 - 1;
                    return Ok(okd(n, bytes));
                }
            }
            let (c, w) = oneshot::<RRes>();
            {
                let mut kb = k.borrow_mut();
                let p = kb.procs.get_mut(&pid).unwrap();
                p.await_wait = Some((c.clone(), a[1] as usize));
                p.inflight = Some(c);
            }
            match w.await {
                RRes::Data(mut bytes) => {
                    let n = bytes.len() as i32;
                    bytes.push(0);
                    Ok(okd(n, bytes))
                }
                RRes::Intr => Err("interrupted".into()),
            }
        }
        LINK => {
            let old = txstr(&tx, 0);
            let nu = txstr(&tx, old.len() + 1);
            let o = walk(k, pid, &old, true).await?; // link the name, not its target
            let (parent, base) = walk_parent(k, pid, &nu).await?;
            if let (Node::Mnt(pm), Node::Mnt(om)) = (&parent.node, &o.node) {
                if !Rc::ptr_eq(&pm.conn, &om.conn) {
                    return Err("cross-device link".into());
                }
                let conn = pm.conn.clone();
                rpc(k, &conn, tv::LINK, W9::new().u32(pm.fid).u32(om.fid).s(&base)).await?;
                return Ok(ok(0));
            }
            let cred = k.borrow().procs.get(&pid).unwrap().cred.clone();
            match (&parent.node, &o.node) {
                (Node::Ram(pr), Node::Ram(onode)) => {
                    ram_access(k, pr, &cred, 2)?;
                    if kid(pr, &base).is_some() {
                        return Err(format!("'{}' already exists", base));
                    }
                    if onode.borrow().dir {
                        return Err("cannot hard-link a directory".into());
                    }
                    pr.borrow_mut().kids.push((base, onode.clone()));
                    Ok(ok(0))
                }
                _ => Err("link not supported on this device".into()),
            }
        }
        SYMLINK => {
            let target = txstr(&tx, 0);
            let nu = txstr(&tx, target.len() + 1);
            let (parent, base) = walk_parent(k, pid, &nu).await?;
            if let Node::Mnt(pm) = &parent.node {
                let conn = pm.conn.clone();
                rpc(k, &conn, tv::SYMLINK, W9::new().u32(pm.fid).s(&base).s(&target)).await?;
                return Ok(ok(0));
            }
            let cred = k.borrow().procs.get(&pid).unwrap().cred.clone();
            if let Node::Ram(pr) = &parent.node {
                ram_access(k, pr, &cred, 2)?;
                if kid(pr, &base).is_some() {
                    return Err(format!("'{}' already exists", base));
                }
                let q = {
                    let mut kb = k.borrow_mut();
                    let q = kb.qgen;
                    kb.qgen += 1;
                    q
                };
                let node = Rc::new(RefCell::new(RNode {
                    name: base.clone(), qpath: q, dir: false, data: Rc::default(),
                    kids: Vec::new(), uid: cred.euid.clone(), mode: 0o777,
                    atime: now_secs(), mtime: now_secs(), symlink: Some(target), ro: false,
                }));
                pr.borrow_mut().kids.push((base, node));
                Ok(ok(0))
            } else {
                Err("symlink not supported on this device".into())
            }
        }
        READLINK => {
            let path = txstr(&tx, 0);
            let dn = walk(k, pid, &path, true).await?;
            if let Node::Mnt(m) = &dn.node {
                let conn = m.conn.clone();
                let rb = rpc(k, &conn, tv::READLINK, W9::new().u32(m.fid)).await?;
                let target = R9::new(&rb).s();
                if m.ephemeral.get() {
                    clunk_fid(k, &conn, m.fid);
                }
                let mut bytes = target.into_bytes();
                let n = bytes.len() as i32;
                bytes.push(0);
                return Ok(okd(n, bytes));
            }
            if let Node::Ram(r) = &dn.node {
                if let Some(tg) = r.borrow().symlink.clone() {
                    let mut bytes = tg.into_bytes();
                    let n = bytes.len() as i32;
                    bytes.push(0);
                    return Ok(okd(n, bytes));
                }
                return Err("not a symlink".into());
            }
            Err("readlink not supported on this device".into())
        }
        _ => Err(format!("bad syscall {} (native)", trap)),
    }
}

fn wstat_ram(node: &RamRef, parent: &DN, base: &str, st: &stat9::StatOut,
             cred: &Cred, eve: &str) -> Result<(), KErr> {
    if node.borrow().ro {
        return Err("read-only snapshot".into());
    }
    if st.mode != 0xffff_ffff {
        if cred.euid != eve && cred.euid != node.borrow().uid {
            return Err(format!("not owner of '{}'", node.borrow().name));
        }
        node.borrow_mut().mode = st.mode & (0o7777 | 0x000C_0000);
    }
    if !st.uid.is_empty() {
        if cred.euid != eve {
            return Err("only the host owner may chown (docs/identity.md D3)".into());
        }
        node.borrow_mut().uid = st.uid.clone();
    }
    if !st.name.is_empty() && st.name != base {
        if cred.euid != eve && cred.euid != node.borrow().uid {
            return Err(format!("not owner of '{}'", node.borrow().name));
        }
        if let Node::Ram(pr) = &parent.node {
            if kid(pr, &st.name).is_some() {
                return Err(format!("'{}' already exists", st.name));
            }
            pr.borrow_mut().kids.retain(|(kk, _)| kk != base);
            node.borrow_mut().name = st.name.clone();
            pr.borrow_mut().kids.push((st.name.clone(), node.clone()));
            pr.borrow_mut().mtime = now_secs();
        }
    }
    if st.mtime != 0xffff_ffff {
        node.borrow_mut().mtime = st.mtime;
    }
    Ok(())
}

async fn open_perm(k: &K, dn: &DN, mode: u32, pid: Pid) -> Result<(), KErr> {
    if let Node::Wsys { kind: WKind::Snarf, .. } = &dn.node {
        if mode & OTRUNC != 0 {
            k.borrow_mut().snarf.clear();
        }
        return Ok(());
    }
    if let Node::Host(hp) = &dn.node {
        let _ = pid;
        if mode & OTRUNC != 0 {
            hostq(k, HostOp::Trunc { path: hpstr(hp) }).await?;
        }
        return Ok(());
    }
    if let Node::Ram(r) = &dn.node {
        let cred = k.borrow().procs.get(&pid).ok_or("no proc")?.cred.clone();
        let rw = mode & 3;
        let mut want = match rw {
            1 => 2,
            2 => 6,
            _ => 4,
        };
        if mode & OTRUNC != 0 {
            want |= 2;
        }
        ram_access(k, r, &cred, want)?;
        if mode & OTRUNC != 0 && !r.borrow().dir {
            Rc::make_mut(&mut r.borrow_mut().data).clear();
        }
    }
    Ok(())
}

fn ram_create(k: &K, parent: &DN, name: &str, perm: u32, isdir: bool,
              cred: &Cred) -> Result<RamRef, KErr> {
    let pr = match &parent.node {
        Node::Ram(r) => r.clone(),
        _ => return Err("create not supported on this device".into()),
    };
    if !pr.borrow().dir {
        return Err("create in non-directory".into());
    }
    ram_access(k, &pr, cred, 2)?;
    if let Some(old) = kid(&pr, name) {
        if old.borrow().dir || isdir {
            return Err(format!("'{}' already exists", name));
        }
        ram_access(k, &old, cred, 2)?;
        Rc::make_mut(&mut old.borrow_mut().data).clear();
        return Ok(old);
    }
    let q = {
        let mut kb = k.borrow_mut();
        let q = kb.qgen;
        kb.qgen += 1;
        q
    };
    let node = Rc::new(RefCell::new(RNode {
        name: name.into(), qpath: q, dir: isdir, data: Rc::default(),
        kids: Vec::new(), uid: cred.euid.clone(), mode: perm & 0o7777,
        atime: now_secs(), mtime: now_secs(), symlink: None, ro: false,
    }));
    pr.borrow_mut().kids.push((name.into(), node.clone()));
    pr.borrow_mut().mtime = now_secs();
    Ok(node)
}

// ---- hostfs: '#Z' — a host directory as files (M4) ----
// uid mapping, decided 2026-08-29 (design.md): host files present as EVE's;
// mode bits map 1:1 (perm & 0o777); sidecar metadata is deferred until the
// '#Z' asks the embedding and parks: hostq queues the op as an effect,
// the caller awaits the completer, hostop_done resumes it. The kernel
// holds no std::fs — the host owns the root, the bytes, and the
// symlink-escape check (the V12 rule's enforcement moved with it).
fn hostq(k: &K, op: HostOp) -> exec::Waiting<Result<HostReply, String>> {
    let (c, w) = oneshot();
    let mut kb = k.borrow_mut();
    let tag = kb.host_tag_next;
    kb.host_tag_next += 1;
    kb.host_tags.insert(tag, c);
    kb.effects.push(Effect::Host { tag, op });
    w
}

fn hpstr(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

// marshal a stat record from the host's metadata answer
fn host_stat_from(k: &K, path: &std::path::Path, dir: bool, len: u64, mtime: u32,
                  ino: u64, mode: u32) -> Vec<u8> {
    let eve = k.borrow().eve.clone();
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".into());
    marshal_stat(&StatIn {
        name: &name,
        qtype: if dir { QTDIR } else { QTFILE },
        qpath: ino,
        mode: (mode & 0o777) | if dir { DMDIR } else { 0 },
        atime: mtime,
        mtime,
        length: if dir { 0 } else { len },
        uid: &eve, gid: &eve, muid: &eve,
        ..Default::default()
    })
}

async fn host_stat_bytes(k: &K, path: &std::path::Path) -> Result<Vec<u8>, KErr> {
    match hostq(k, HostOp::Meta { path: hpstr(path) }).await? {
        HostReply::Meta { dir, len, mtime, ino, mode } =>
            Ok(host_stat_from(k, path, dir, len, mtime, ino, mode)),
        _ => Err(format!("{}: does not exist", path.display())),
    }
}

async fn host_dir_read(k: &K, path: &std::path::Path, n: usize, off: u64)
    -> Result<Vec<u8>, KErr> {
    let ents = match hostq(k, HostOp::ReadDir { path: hpstr(path) }).await? {
        HostReply::Entries(e) => e,
        _ => return Err(format!("{}: not a directory", path.display())),
    };
    let mut skip = off as usize;
    let mut out = Vec::new();
    for e in ents {
        let rec = host_stat_from(k, &path.join(&e.name), e.dir, e.len, e.mtime,
                                 e.ino, e.mode);
        if skip >= rec.len() {
            skip -= rec.len();
            continue;
        }
        if out.len() + rec.len() > n {
            break;
        }
        out.extend_from_slice(&rec);
    }
    Ok(out)
}

// ---- rfork / exec / exits (the lifecycle) ----
fn rfork(k: &K, worker_pid: Pid, pid: Pid, flags: i32, marker: i32) -> KRes {
    if flags & rf::PROC == 0 {
        let (ns2, env2) = {
            let kb = k.borrow();
            let p = kb.procs.get(&pid).ok_or("no proc")?;
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
            let old = k.borrow().procs.get(&pid).unwrap().fdt.clone();
            fdt_close(k, &old);
            k.borrow_mut().procs.get_mut(&pid).unwrap().fdt = new_fdt();
        }
        let mut kb = k.borrow_mut();
        let group = if flags & rf::NOTEG != 0 {
            let g = kb.next_note_group;
            kb.next_note_group += 1;
            Some(g)
        } else {
            None
        };
        let p = kb.procs.get_mut(&pid).unwrap();
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
        return Ok(ok(0));
    }
    if k.borrow().procs.get(&worker_pid).and_then(|p| p.borrower).is_some() {
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
        let kb = k.borrow();
        let p = kb.procs.get(&pid).ok_or("no proc")?;
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
        let kb = k.borrow();
        let p = kb.procs.get(&pid).unwrap();
        if flags & rf::CFDG != 0 {
            new_fdt()
        } else if flags & rf::FDG != 0 {
            fdt_copy(&p.fdt)
        } else {
            let f = p.fdt.clone();
            f.borrow_mut().refs += 1;
            f
        }
    };
    if marker == 2 {
        if !bits.asyncified {
            return Err("not an asyncify build — add it to ASYNCIFY in poc/mk.sh, or use procrfork".into());
        }
        if flags & rf::MEM != 0 {
            return Err("RFMEM is the lazy path's flag; a bare fork copies".into());
        }
        let child = new_proc(k, pid, bits.ns, fdt, bits.cwd, bits.cred, bits.env, bits.group);
        let mut kb = k.borrow_mut();
        let c = kb.procs.get_mut(&child).unwrap();
        c.nomnt = bits.nomnt || flags & rf::NOMNT != 0;
        c.nowait = flags & rf::NOWAIT != 0;
        c.image = bits.image;
        c.asyncified = true;
        return Ok(ok(child as i32)); // one return; the runner makes two
    }
    if marker != 1 {
        return Err("bare rfork(RFPROC) needs an asyncify build; procrfork is the exec path".into());
    }
    if flags & rf::MEM == 0 {
        return Err("plain fork needs asyncify — the guard path is lazy".into());
    }
    let child = new_proc(k, pid, bits.ns, fdt, bits.cwd, bits.cred, bits.env, bits.group);
    {
        let mut kb = k.borrow_mut();
        let c = kb.procs.get_mut(&child).unwrap();
        c.nomnt = bits.nomnt || flags & rf::NOMNT != 0;
        c.nowait = flags & rf::NOWAIT != 0;
        kb.procs.get_mut(&worker_pid).unwrap().borrower = Some(child);
    }
    Ok(KReply {
        ret: 0, aux: child as i32, data: Vec::new(), action: KAction::None,
        load: None, note_pending: false,
    })
}

async fn exec_call(k: &K, worker_pid: Pid, pid: Pid, tx: &[u8], argc: i32) -> KRes {
    let path = txstr(tx, 0);
    let mut argv = Vec::new();
    let mut o = path.len() + 1;
    for _ in 0..argc {
        let s = txstr(tx, o);
        o += s.len() + 1;
        argv.push(s);
    }
    let dn = walk(k, pid, &path, false).await?;
    if let Ok(rec) = dev_stat(k, &dn, pid).await {
        if let Some(st) = parse_stat(&rec) {
            if st.mode & DMDIR != 0 {
                return Err(format!("'{}' is a directory", path));
            }
            if st.mode & DMSETUID != 0 {
                k.borrow_mut().procs.get_mut(&pid).unwrap().cred.euid = st.uid.clone();
            }
        }
    }
    let image = read_all(k, &dn, pid).await?;
    if image.len() < 4 || image[0..4] != [0x00, 0x61, 0x73, 0x6d] {
        return Err(format!("'{}' exec format error", path));
    }
    let image = Arc::new(image);
    let argv = if argv.is_empty() { vec![path.clone()] } else { argv };
    {
        let mut kb = k.borrow_mut();
        let p = kb.procs.get_mut(&pid).unwrap();
        p.argv = argv.clone();
        p.image = Some(image.clone());
        p.has_handler = false; // fresh image, no handler yet
    }
    let is_borrowed = k.borrow().procs.get(&worker_pid).and_then(|p| p.borrower) == Some(pid);
    if is_borrowed {
        let mut kb = k.borrow_mut();
        kb.procs.get_mut(&worker_pid).unwrap().borrower = None;
        kb.effects.push(Effect::Spawn { pid, image, argv, asy: None });
        return Ok(KReply {
            ret: -1000, aux: pid as i32, data: Vec::new(),
            action: KAction::ForkResume, load: None, note_pending: false,
        });
    }
    Ok(KReply {
        ret: -1001, aux: 0, data: Vec::new(), action: KAction::ExecSelf,
        load: Some((image, argv)), note_pending: false,
    })
}

fn exits(k: &K, worker_pid: Pid, pid: Pid, tx: &[u8]) -> KRes {
    let msg = txstr(tx, 0);
    let (ppid, nowait, fdt) = {
        let kb = k.borrow();
        let p = kb.procs.get(&pid).ok_or("no proc")?;
        (p.ppid, p.nowait, p.fdt.clone())
    };
    k.borrow_mut().procs.remove(&pid);
    fdt_close(k, &fdt); // pipes learn their EOF here
    let was_borrowed = k.borrow().procs.get(&worker_pid).and_then(|p| p.borrower) == Some(pid);
    if was_borrowed {
        k.borrow_mut().procs.get_mut(&worker_pid).unwrap().borrower = None;
        zombie(k, ppid, pid, &msg, nowait);
        return Ok(KReply {
            ret: -1000, aux: pid as i32, data: Vec::new(),
            action: KAction::ForkResume, load: None, note_pending: false,
        });
    }
    if pid == 1 {
        k.borrow_mut().effects.push(Effect::Shutdown(if msg.is_empty() { 0 } else { 1 }));
        return Ok(KReply {
            ret: -3000, aux: 0, data: Vec::new(), action: KAction::Die,
            load: None, note_pending: false,
        });
    }
    zombie(k, ppid, pid, &msg, nowait);
    Ok(KReply {
        ret: -2000, aux: 0, data: Vec::new(), action: KAction::Retire,
        load: None, note_pending: false,
    })
}
