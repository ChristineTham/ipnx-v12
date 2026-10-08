//! `namec` — turning a name into a channel.
//!
//! Plan 9's `chan.c:1317`. Every call that takes a path goes through it, which
//! is why it is one function and not one per syscall.
//!
//! The shape, from the source:
//!
//! 1. **Find the starting point.** `/` starts at the process's root channel,
//!    `#` attaches a device, anything else starts at the process's `dot`.
//!    Plan 9 holds both as CHANNELS, not as text, and so does this.
//! 2. **Walk the elements**, and at each one step through a mount point if
//!    there is one — `walk()`'s own comment: *"1. step through a mount point,
//!    if any … 3. move to the first mountpoint along the way. 4. repeat."*
//! 3. **Apply the mode** — open it, create in it, mount on it.
//!
//! A `#` path sets `nomount`: you get the device itself, not whatever is
//! mounted over it.

use crate::chan::Chan;
use crate::dev::{self, Dev, DevId};
use crate::ns::Ns;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// `namec`'s `amode`: what the name is being resolved FOR. Plan 9's set, less
/// `namec`'s access modes — all seven of Plan 9's (`portdat.h:144`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum A {
    /// `Aaccess` — *"as in stat, wstat"*. Resolve only.
    Access,
    /// `Abind` — *"for left-hand-side of bind"*: the thing being bound, and
    /// **not required to be a directory**, so `bind /bin/rc /bin/sh` puts a
    /// file over a file. Plan 9 notes *"no need to maintain path - cannot
    /// dotdot an Abind"*.
    Bind,
    /// `Atodir` — *"as in chdir"*. Must be a directory.
    Todir,
    /// `Aopen` — *"for i/o"*.
    Open,
    /// `Amount` — *"to be mounted or mounted upon"*: `bind`'s right-hand side.
    Mount,
    /// `Acreate` — *"is to be created"*. The PARENT is walked and the last
    /// element created in it (`chan.c`, `e.nelems--`), which is why it goes
    /// through [`create`] rather than this function.
    Create,
    /// `Aremove` — *"will be removed by caller"*. Resolves as `Aaccess` does.
    Remove,
}

/// The kernel's device table: `devtab`, keyed by letter as Plan 9 keys it.
#[derive(Default)]
pub struct Devtab {
    devs: HashMap<DevId, Box<dyn Dev>>,
    /// `up`, for the mount driver's RPCs, which sleep as a pipe read does.
    pub up: Option<std::rc::Rc<std::cell::RefCell<crate::proc::Up>>>,
    /// Each wire's reader and outstanding RPCs — the half of Plan 9's `Mnt`
    /// that `mountio` and `mountmux` keep (`devmnt.c:774`, `:930`), by the
    /// wire's identity, as `c->mux` is the wire's own.
    muxes: HashMap<(DevId, u32, u64), Mux>,
    /// What each process's call has done on the wires so far: see [`Record`].
    records: HashMap<crate::proc::Pid, Record>,
    /// **Each mounted wire, held** — one reference for all the channels
    /// derived from its mounts, each of which holds one in Plan 9
    /// (`devmnt.c:355`, *"incref(m->c)"*), so that the process which mounted
    /// it may close its descriptor — `plumber` does, `fsys.c:221` — without
    /// hanging the server up. Let go of when the last of those channels is
    /// closed ([`Devtab::unwire`]).
    wires: HashMap<(DevId, u32, u64), std::rc::Rc<std::cell::RefCell<Chan>>>,
    /// **Closes to make next**, by process: a channel whose last reference
    /// went where its close could not be made — inside another close
    /// (`chanfree`'s own `cclose`s, `chan.c:453`), or in a call that may run
    /// again from the top — for the closing loop or the call's end to close
    /// before anything else ([`Devtab::cclose`]).
    deferred: HashMap<crate::proc::Pid, Vec<Chan>>,
    /// `char *eve` (`auth.c:10`) — kernel-wide, and handed to every device
    /// as it joins. It starts EMPTY, as `userinit` leaves it
    /// (`pc/main.c:285`); `boot` names the host owner by writing
    /// `#c/hostowner`.
    eve: crate::dev::Eve,
}

impl Devtab {
    pub fn new() -> Devtab {
        Devtab::default()
    }
    /// Add a device, and hand it the kernel's `eve` on the way in — which
    /// is the one place a Rust kernel does what Plan 9 gets from a global.
    pub fn add(&mut self, d: Box<dyn Dev>) {
        let mut d = d;
        d.seteve(self.eve.clone());
        self.devs.insert(d.id(), d);
    }

    /// The kernel-wide `eve`, for whoever else needs it — the boot, and a
    /// test that wants to know who the host owner is.
    pub fn eve(&self) -> crate::dev::Eve {
        self.eve.clone()
    }
    pub fn get(&mut self, id: DevId) -> Option<&mut Box<dyn Dev>> {
        self.devs.get_mut(&id)
    }

    /// Take a device out of the table for the length of one operation, so it
    /// can use the rest of the table. Only the mount driver needs this, and
    /// only because it is the one device that talks to another.
    pub fn take(&mut self, id: DevId) -> Option<Box<dyn Dev>> {
        self.devs.remove(&id)
    }

    pub fn put(&mut self, d: Box<dyn Dev>) {
        self.devs.insert(d.id(), d);
    }

    /// The mount driver, taken out for the length of one operation.
    ///
    /// **This is what makes `#M` possible**, and it is Plan 9's arrangement
    /// rather than a trick. `devtab[]` holds `Dev*` — seventeen function
    /// pointers and no state (`portdat.h`, `struct Dev`) — and devmnt's own
    /// state is `mntalloc` (`devmnt.c:48`), a file-scope global that was never
    /// in the table. So `mountio`'s `devtab[m->c->type]->bwrite` reaches a
    /// vtable, never devmnt's state, and cannot recur into it.
    ///
    /// Here the state IS in the table, so the same property is arranged by
    /// taking the driver out: while it runs, nothing can reach it, and the
    /// rest of the table is free for its wire.
    fn with_mnt<R>(
        &mut self,
        f: impl FnOnce(&mut crate::devmnt::MntDev, &mut Devtab) -> R,
    ) -> Result<R, String> {
        let mut boxed = self.take(DevId::Mnt).ok_or("no mount driver")?;
        let r = match boxed.as_any().downcast_mut::<crate::devmnt::MntDev>() {
            None => Err("#M is not the mount driver".to_string()),
            Some(m) => Ok(f(m, self)),
        };
        self.put(boxed);
        r
    }

    /// `devtab[c->type]->walk(...)` and the rest. Every device operation goes
    /// through these, so `#M` is dispatched the same way as anything else
    /// from a caller's point of view.
    /// `cclone` — see [`crate::dev::Dev::cclone`].
    pub fn dcclone(&mut self, c: &Chan) -> Result<Chan, String> {
        if c.dev == DevId::Mnt {
            let c = c.clone();
            return self.with_mnt(|m, tab| {
                let mut w = Wire::new(&c, m, tab)?;
                m.cclone(&mut w, &c)
            })?;
        }
        self.get(c.dev).ok_or("no such device")?.cclone(c)
    }

    /// `devtab[c->type]->walk(c, nil, names, n)` (`chan.c:1027`): several
    /// names. The mount driver sends them in one `Twalk`; any other device
    /// walks them in turn, as `devwalk` does (`dev.c:169`) — the first
    /// missing is *"error(Enonexist)"*, a later one a short answer.
    pub fn dwalkn(&mut self, c: &Chan, names: &[String]) -> Result<crate::dev::Walkqid, String> {
        if c.dev == DevId::Mnt {
            let (c, names) = (c.clone(), names.to_vec());
            return self.with_mnt(|m, tab| {
                let mut w = Wire::new(&c, m, tab)?;
                m.walkn(&mut w, &c, &names)
            })?;
        }
        let d = self.get(c.dev).ok_or("no such device")?;
        let mut qids = Vec::new();
        let mut at = c.clone();
        for (j, name) in names.iter().enumerate() {
            match d.walk(&at, name) {
                Ok(Some(next)) => {
                    qids.push(next.qid);
                    at = next;
                }
                Ok(None) if j == 0 => return Err(ENONEXIST.into()),
                Err(e) if j == 0 => return Err(e),
                _ => return Ok(crate::dev::Walkqid { qids, clone: None }),
            }
        }
        Ok(crate::dev::Walkqid { qids, clone: Some(at) })
    }

    pub fn dwalk(&mut self, c: &Chan, name: &str) -> Result<Option<Chan>, String> {
        if c.dev == DevId::Mnt {
            let (c, name) = (c.clone(), name.to_string());
            return self.with_mnt(|m, tab| {
                let mut w = Wire::new(&c, m, tab)?;
                m.walk(&mut w, &c, &name)
            })?;
        }
        self.get(c.dev).ok_or("no such device")?.walk(c, name)
    }

    /// `devtab[c->type]->open(c, omode)`, which answers a `Chan*` — and
    /// **two devices answer one that already exists**, with one more
    /// reference: `srvopen` the posted channel (*"incref(sp->chan); …
    /// return sp->chan"*, `devsrv.c:135`) and `dupopen` the descriptor's
    /// (*"fdtochan(fd, openmode(omode), 0, 1)"*, `devdup.c:86` — the `1` is
    /// the reference). So an open answers a reference: the descriptor's own
    /// channel for those two, shared with whoever else holds it, offset
    /// and all; a channel of its own for everything else.
    pub fn dopen(&mut self, c: Chan, mode: u16) -> Result<Rc<RefCell<Chan>>, String> {
        if !c.qid.is_dir() && c.dev == DevId::Srv {
            let d = self.get(DevId::Srv).ok_or("no such device")?;
            let d = d.as_any().downcast_mut::<crate::devsrv::SrvDev>().ok_or("#s is not srv")?;
            return d.srvopen(&c, mode);
        }
        if !c.qid.is_dir() && c.dev == DevId::Dup && (c.qid.path - 1) & 1 == 0 {
            let d = self.get(DevId::Dup).ok_or("no such device")?;
            let d = d.as_any().downcast_mut::<crate::devdup::DupDev>().ok_or("#d is not dup")?;
            return d.dupopen(&c, mode);
        }
        let c = if c.dev == DevId::Mnt {
            self.with_mnt(|m, tab| {
                let mut w = Wire::new(&c, m, tab)?;
                m.open(&mut w, c.clone(), mode)
            })??
        } else {
            self.get(c.dev).ok_or("no such device")?.open(c, mode)?
        };
        Ok(Rc::new(RefCell::new(c)))
    }

    /// `cclose` (`chan.c:490`): one reference fewer — *"if(decref(c))
    /// return;"* — and at the last, the device's close, **made next by
    /// whatever can wait for it**: the closing loop this is inside, or the
    /// end of the call ([`Devtab::deferred`]). A device's close may wait —
    /// the mount driver for `Rclunk`, a serial line for its output — and
    /// may change the device as it goes, so it is made where what is left
    /// of it is kept, never in the middle of something that runs again
    /// from the top. A call that does run again defers it once: the record
    /// says it already has.
    pub fn cclose(&mut self, c: Rc<RefCell<Chan>>) {
        if let Ok(cell) = Rc::try_unwrap(c) {
            self.defer(cell.into_inner());
        }
    }

    /// The same, of a channel that is already the caller's alone.
    pub fn defer(&mut self, c: Chan) {
        let pid = self.uppid();
        if self.once(pid) {
            self.deferred.entry(pid).or_default().push(c);
        }
    }

    /// The closes [`Devtab::cclose`] left for this process to make.
    pub fn deferred(&mut self, pid: crate::proc::Pid) -> Vec<Chan> {
        self.deferred.remove(&pid).unwrap_or_default()
    }

    /// Whether this step of the call is reached for the first time, or
    /// again in a call made again from its record.
    fn once(&mut self, pid: crate::proc::Pid) -> bool {
        let r = self.records.entry(pid).or_default();
        if let Some(Step::Once) = r.steps.get(r.at) {
            r.at += 1;
            return false;
        }
        r.steps.truncate(r.at);
        r.steps.push(Step::Once);
        r.at += 1;
        true
    }

    /// What `srvremove` let go of, closed here because the device cannot
    /// reach the table (`devsrv.c:227`).
    fn unposted(&mut self) {
        let gone = match self.get(DevId::Srv).and_then(|d| d.as_any().downcast_mut::<crate::devsrv::SrvDev>()) {
            Some(d) => d.unposted(),
            None => return,
        };
        for c in gone {
            self.cclose(c);
        }
    }

    pub fn dread(&mut self, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String> {
        if c.dev == DevId::Mnt {
            let mut cc = c.clone();
            return self.with_mnt(|m, tab| {
                let mut w = Wire::new(&cc, m, tab)?;
                m.read(&mut w, &mut cc, n, off)
            })?;
        }
        self.get(c.dev).ok_or("no such device")?.read(c, n, off)
    }

    pub fn dwrite(&mut self, c: &mut Chan, data: &[u8], off: u64) -> Result<usize, String> {
        if c.dev == DevId::Mnt {
            let mut cc = c.clone();
            let data = data.to_vec();
            return self.with_mnt(|m, tab| {
                let mut w = Wire::new(&cc, m, tab)?;
                m.write(&mut w, &mut cc, &data, off)
            })?;
        }
        self.get(c.dev).ok_or("no such device")?.write(c, data, off)
    }

    pub fn dstat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        if c.dev == DevId::Mnt {
            let c = c.clone();
            return self.with_mnt(|m, tab| {
                let mut w = Wire::new(&c, m, tab)?;
                m.stat(&mut w, &c)
            })?;
        }
        self.get(c.dev).ok_or("no such device")?.stat(c)
    }

    pub fn dcreate(&mut self, c: &mut Chan, name: &str, mode: u16, perm: u32) -> Result<(), String> {
        if c.dev == DevId::Mnt {
            let (mut cc, name) = (c.clone(), name.to_string());
            let r = self.with_mnt(|m, tab| {
                let mut w = Wire::new(&cc, m, tab)?;
                m.create(&mut w, &mut cc, &name, mode, perm)
            })?;
            *c = cc;
            return r;
        }
        self.get(c.dev).ok_or("no such device")?.create(c, name, mode, perm)
    }

    pub fn dremove(&mut self, c: &mut Chan) -> Result<(), String> {
        if c.dev == DevId::Mnt {
            let mut cc = c.clone();
            let r = self.with_mnt(|m, tab| {
                let mut w = Wire::new(&cc, m, tab)?;
                m.remove(&mut w, &mut cc)
            })?;
            self.unwire();
            return r;
        }
        let r = self.get(c.dev).ok_or("no such device")?.remove(c);
        if c.dev == DevId::Srv {
            self.unposted();
        }
        r
    }

    pub fn dwstat(&mut self, c: &mut Chan, edir: &[u8]) -> Result<(), String> {
        if c.dev == DevId::Mnt {
            let (mut cc, edir) = (c.clone(), edir.to_vec());
            return self.with_mnt(|m, tab| {
                let mut w = Wire::new(&cc, m, tab)?;
                m.wstat(&mut w, &mut cc, &edir)
            })?;
        }
        self.get(c.dev).ok_or("no such device")?.wstat(c, edir)
    }

    pub fn dclose(&mut self, c: &mut Chan) {
        if c.dev == DevId::Mnt {
            let mut cc = c.clone();
            let _ = self.with_mnt(|m, tab| {
                if let Ok(mut w) = Wire::new(&cc, m, tab) {
                    m.close(&mut w, &mut cc);
                }
                Ok::<(), String>(())
            });
            self.unwire();
        } else if let Some(d) = self.get(c.dev) {
            d.close(c)
        }
        // A mount's message channel at its last reference: *"if(c->mux !=
        // nil){ muxclose(c->mux); …"* (`chanfree`, `chan.c:471`) — the
        // session is over, and a wire made later with the same name begins
        // its own.
        if c.flag & crate::chan::flag::CMSG != 0 {
            let w = (c.dev, c.devno, c.qid.path);
            self.muxes.remove(&w);
            let _ = self.with_mnt(|m, _| m.muxclose(w));
        }
        // `srvclose` of a name opened `ORCLOSE` is `srvremove`.
        if c.dev == DevId::Srv {
            self.unposted();
        }
        // then `chanfree` (`chan.c:467`): *"if(c->umc != nil){
        // cclose(c->umc); c->umc = nil; }"* — the union element a read
        // had open
        if let Some(mut u) = c.umc.take() {
            self.dclose(&mut u);
        }
    }

    /// Attach a 9P server over a channel — `mount(2)`'s device half — with
    /// an authentication file's fid, or NOFID.
    pub fn dmount(&mut self, wire: Chan, uname: &str, aname: &str, afid: u32) -> Result<Chan, String> {
        let (uname, aname) = (uname.to_string(), aname.to_string());
        self.with_mnt(|m, tab| {
            let mut w = Wire { wire: wire.clone(), tab };
            m.mount_auth(wire.clone(), &mut w, &uname, &aname, afid)
        })?
    }

    /// `mntauth` — `fauth(2)`'s device half.
    pub fn dauth(&mut self, wire: Chan, uname: &str, aname: &str) -> Result<Chan, String> {
        let (uname, aname) = (uname.to_string(), aname.to_string());
        self.with_mnt(|m, tab| {
            let mut w = Wire { wire: wire.clone(), tab };
            m.auth(wire.clone(), &mut w, &uname, &aname)
        })?
    }

    /// `mntversion` — `fversion(2)`'s device half.
    pub fn dfversion(&mut self, wire: Chan, msize: u32, version: &str) -> Result<String, String> {
        let version = version.to_string();
        self.with_mnt(|m, tab| {
            let mut w = Wire { wire: wire.clone(), tab };
            m.fversion(&wire, &mut w, msize, &version)
        })?
    }
}

/// Where a walk starts and what it walks.
pub struct Start {
    pub chan: Chan,
    pub nomount: bool,
    /// The reference the channel is, if it is not the walk's own: `slash`
    /// or `dot`. `None` is a `#` name's attach, which the walk made and must
    /// close when it is done with it.
    pub src: Option<Rc<Chan>>,
    /// `Elemlist` (`chan.c:26`), as `parsename` makes it (`:1196`): the
    /// elements, where each ends in the name — for an error to show the
    /// name as far as the one it concerns — and whether the name ends in
    /// `/` or `/.`.
    pub elems: Vec<String>,
    ends: Vec<usize>,
    pub mustbedir: bool,
}

/// `parsename` (`chan.c:1196`) of `aname` after its first `prefix` bytes:
/// each element's span, what `skipslash` (`:1671`) leaves between slashes
/// — `/` and `.` skipped — and whether it ran out after a slash or a `.`,
/// `mustbedir`.
fn parsename(aname: &str, prefix: usize) -> (Vec<std::ops::Range<usize>>, bool) {
    let b = aname.as_bytes();
    let mut spans = Vec::new();
    let mut i = prefix;
    loop {
        // *"while(name[0]=='/' || (name[0]=='.' && (name[1]==0 ||
        // name[1]=='/'))) name++;"*
        while i < b.len() && (b[i] == b'/' || (b[i] == b'.' && (i + 1 == b.len() || b[i + 1] == b'/'))) {
            i += 1;
        }
        if i >= b.len() {
            return (spans, true);
        }
        let end = b[i..].iter().position(|&c| c == b'/').map_or(b.len(), |p| i + p);
        spans.push(i..end);
        if end == b.len() {
            return (spans, false);
        }
        i = end;
    }
}

/// What a name's errors need of it: the name, and where each element ends.
struct Named<'a> {
    aname: &'a str,
    ends: Vec<usize>,
}

impl Named<'_> {
    /// *"Prepare nice error, showing first e.nerror elements of name"*
    /// (`chan.c:1406`): from the walk on, an error in `namec` shows the
    /// name as far as the element it concerns, as `namelenerror` writes
    /// it — the whole of it once every element is walked (`:1131`). None
    /// is the error as it was; a call that has left the processor has no
    /// error at all.
    fn err(&self, nerror: usize, e: String) -> String {
        if nerror == 0 || e == crate::devmnt::SLEPT {
            return e;
        }
        let shown = if nerror >= self.ends.len() { self.aname } else { &self.aname[..self.ends[nerror - 1]] };
        nameerror(shown, &e)
    }

    /// The same, of every element: an error after the walk.
    fn all(&self, e: String) -> String {
        self.err(self.ends.len(), e)
    }
}

impl Start {
    fn named<'a>(&self, aname: &'a str) -> Named<'a> {
        Named { aname, ends: self.ends.clone() }
    }
}

/// `Enonexist` (`error.h:9`).
pub const ENONEXIST: &str = "file does not exist";
/// `Eexist` (`error.h:10`).
pub const EEXIST: &str = "file already exists";

/// Step 1: the starting point.
///
/// `#M` is refused by name, and that is Plan 9's rule, not a policy of ours:
/// `chan.c` has `if(utfrune("M", r)) error(Enoattach);` — the mount driver is
/// reached through `mount()` and no other way.
pub fn start(
    tab: &mut Devtab,
    ns: &Ns,
    name: &str,
    slash: &Rc<Chan>,
    dot: &Rc<Chan>,
) -> Result<Start, String> {
    if name.is_empty() {
        return Err("empty file name".into());
    }
    // *"aname = validnamedup(aname, 1)"* (`chan.c:1330`)
    validname(name, true)?;
    if name.starts_with('#') {
        let (id, spec, below) = dev::split(name).ok_or(EBADSHARP)?;
        // `chan.c:1374`. `#M` is never reachable by name: `mount(2)` supplies
        // the channel, and there is no server to find by writing the letter.
        if id == DevId::Mnt {
            return Err(ENOATTACH.into());
        }
        // `chan.c:1376` — `RFNOMNT`'s sandbox, and the exception list is
        // exactly Plan 9's, with its own reasoning:
        //
        //   the OK exceptions are:
        //     |  it only gives access to pipes you create
        //     d  this process's file descriptors
        //     e  this process's environment
        //   the iffy exceptions are:
        //     c  time and pid, but also cons and consctl
        //     p  control of your own processes (and unfortunately
        //        any others left unprotected)
        if ns.noattach() && !"|decp".contains(id.letter()) {
            return Err(ENOATTACH.into());
        }
        let d = tab.get(id).ok_or(EBADSHARP)?;
        let chan = d.attach(spec)?;
        // *"while(*name != '\0' && (*name != '/' || n < 2))"* (`chan.c:1351`):
        // the `#`, the letter and the spec are the starting point, and the
        // elements begin after them
        let _ = below;
        let prefix = 1 + id.letter().len_utf8() + spec.len();
        return Ok(start_at(chan, true, None, name, prefix));
    }
    if name.starts_with('/') {
        return Ok(start_at((**slash).clone(), false, Some(slash.clone()), name, 0));
    }
    Ok(start_at((**dot).clone(), false, Some(dot.clone()), name, 0))
}

fn start_at(chan: Chan, nomount: bool, src: Option<Rc<Chan>>, name: &str, prefix: usize) -> Start {
    let (spans, mustbedir) = parsename(name, prefix);
    Start {
        chan,
        nomount,
        src,
        elems: spans.iter().map(|r| name[r.clone()].to_string()).collect(),
        ends: spans.iter().map(|r| r.end).collect(),
        mustbedir,
    }
}

/// `domount`: if something is mounted on this channel, step onto it.
///
/// This is where the namespace and the device table meet, and it is checked at
/// EVERY component rather than once against a prefix — which is what makes a
/// bind visible through every path that reaches the file.
/// `domount` — step onto whatever is mounted here, and say what ELSE is
/// mounted here.
///
/// **What comes back first is the caller's OWN copy** (`cunique`,
/// `chan.c:586`). The channel in a mount table is shared by everything that
/// resolves through it, and a `create` MOVES a channel — so handing out the
/// namespace's own would move the mount itself. It did: a `create` through a
/// mount walked the server's root fid onto the new file, and nothing resolved
/// through that mount again.
///
/// With it, the head it was found on (`findmount`'s *"*mp = m"*,
/// `chan.c:875`), whose other elements a walk tries when the first has no
/// such name (`chan.c:1034`), and which `namec` keeps as `umh` for two of
/// the access modes.
fn domount(
    tab: &mut Devtab,
    ns: &Ns,
    c: Chan,
) -> Result<(Chan, Option<crate::ns::Head>), String> {
    let Some(head) = ns.findmount(&c) else { return Ok((c, None)) };
    // the first element's channel, a reference the caller takes from the
    // head — *"cclose(*cp); incref(m->mount->to); *cp = m->mount->to"*
    // (`findmount`, `chan.c:878`) — the mount's own channel, not a clone of
    // it: a walk from it makes its own fid (`Twalk`'s newfid), and
    // `cunique` clones one that is to be opened. A clone here was a fid on
    // the server for every mount crossed, and never clunked.
    let _ = tab;
    let mut m = (*head.borrow().mount[0].chan).clone();
    m.path = c.path.clone(); // the name is how we got here, not where we landed
    Ok((m, Some(head)))
}

/// `walk` (`chan.c:965`): the elements, one at a time, stepping through
/// mounts — and through UNIONS.
pub fn walk(
    tab: &mut Devtab,
    ns: &Ns,
    mut c: Chan,
    src: Option<Rc<Chan>>,
    names: &[String],
    nomount: bool,
) -> Result<(Chan, Option<Rc<Chan>>), (usize, String)> {
    // The error comes with `nerror`, *"the number of names to display in an
    // error message"* (`chan.c:961`): those walked, for a file that is not
    // a directory (`:997`); with the one that failed, for one not found
    // (`:1047`).
    //
    // **A channel the walk made is the walk's to close** when it steps past
    // it — *"cclose(c); c = nc"* (`chan.c:1109`) — or fails: through a
    // mount, each step is a fid the server holds until it is clunked. The
    // channel it was given, and one a mount answers, are someone else's:
    // `src` is the reference those are.
    let mut src = src;
    let mut owned = src.is_none();
    let drop = |tab: &mut Devtab, c: &mut Chan, owned: bool| {
        if owned {
            tab.dclose(c);
        }
    };
    // **The path is carried beside the channel, not taken from it**
    // (`chan.c`, `walk`): `path = c->path` before the loop, `path =
    // addelem(path, names[nhave+i], mtpt)` for each name, and `c->path =
    // path` at the end. `domount` does not touch the text — it only records
    // the mount point — so a name keeps the name it was walked by, and a
    // file on the far side of a mount is `/root/wasm/bin` rather than the
    // mount driver's `#M/wasm/bin`.
    let mut path = c.path.clone();
    // *"While we haven't gotten all the way down the path: 1. step through
    // a mount point, if any 2. send a walk request for initial dotdot or
    // initial prefix without dotdot 3. move to the first mountpoint along
    // the way. 4. repeat."* (`chan.c:981`). `didmount`: `c` is already the
    // mount's, where the last step stopped.
    let mut nhave = 0;
    let mut didmount = false;
    let mut mh: Vec<crate::ns::Element> = Vec::new();
    while nhave < names.len() {
        if !c.is_dir() {
            drop(tab, &mut c, owned);
            return Err((nhave, ENOTDIR.into()));
        }
        // up to `MAXWELEM` at once (`fcall.h:6`), and `..` alone
        let mut ntry = (names.len() - nhave).min(MAXWELEM);
        let mut dotdot = false;
        if let Some(i) = names[nhave..nhave + ntry].iter().position(|n| n == "..") {
            dotdot = i == 0;
            ntry = if i == 0 { 1 } else { i };
        }
        // `..` does not step onto a mount: it goes back out of one. Plan 9
        // undomounts here, which needs the mount head a channel was derived
        // from; this subset does not carry that yet, so `..` walks the device
        // and is honest about only that.
        if !dotdot && !nomount && !didmount {
            let was = c.clone();
            let (first, head) = domount(tab, ns, c).map_err(|e| (nhave, e))?;
            if let Some(h) = head {
                // what the mount replaced is closed if the walk made it
                // (`findmount`'s *"cclose(*cp)"*); the mount's channel is
                // the namespace's
                let mut was = was;
                drop(tab, &mut was, owned);
                owned = false;
                mh = h.borrow().mount.clone();
            } else {
                mh = Vec::new();
            }
            c = first;
        }
        let batch = &names[nhave..nhave + ntry];
        // **`ewalk`** (`chan.c:948`): *"if(waserror()) return nil"* — a
        // device's walk that FAILS is a miss like one that finds nothing, so
        // the union is still tried. A 9P server answers a missing first name
        // with `Rerror`, not an empty `Rwalk`; propagating that error here
        // meant a union whose first element was a mounted directory never
        // reached its second (`/home` over `/usr/kitty`, with `bind -a /etc
        // /home` after it, listed `motd` and could not open it).
        let (wq, on) = match tab.dwalkn(&c, batch) {
            Ok(wq) => (wq, (c.dev, c.devno)),
            Err(e) if e == crate::devmnt::SLEPT => return Err((0, e)),
            Err(e) => {
                // **"try a union mount, if any"** (`chan.c:1027`). The
                // first element is the one just walked, so this starts at
                // the next — `for(f = (f? f->next: f); f; f = f->next)`
                // (`:1034`). Without it a `bind -a` puts an element in a
                // list nothing ever reaches, and a union is a word rather
                // than a thing.
                let mut err = e;
                let mut found = None;
                if !nomount {
                    for alt in mh.iter().skip(1) {
                        match tab.dwalkn(&alt.chan, batch) {
                            Ok(wq) => {
                                found = Some((wq, (alt.chan.dev, alt.chan.devno)));
                                break;
                            }
                            Err(e) if e == crate::devmnt::SLEPT => return Err((0, e)),
                            Err(e) => err = e,
                        }
                    }
                }
                match found {
                    Some(f) => f,
                    // Every element missed: the error is the last device's,
                    // as `walk` returns -1 with it still set (`:1041`) —
                    // `devwalk`'s *"error(Enonexist)"* (`dev.c:230`) for a
                    // device here that answered nothing.
                    None => {
                        drop(tab, &mut c, owned);
                        return Err((nhave + 1, err));
                    }
                }
            }
        };
        // A mount point at any qid the walk answered but the last — which
        // the next step's `domount` or the access mode sees — is found
        // with `findmount` (`chan.c:1067`), and ends the step there, on the
        // mount's channel: *"stopped early, at a mount point"* (`:1094`).
        let mut stop = None;
        if !dotdot && !nomount {
            for (i, q) in wq.qids.iter().enumerate().take(ntry - 1) {
                let mut at = c.clone();
                (at.dev, at.devno, at.qid) = (on.0, on.1, *q);
                if let Some(h) = ns.findmount(&at) {
                    stop = Some((i, h));
                    break;
                }
            }
        }
        let n;
        let nc;
        match stop {
            None => match wq.clone {
                Some(clone) => {
                    n = wq.qids.len();
                    nc = clone;
                    didmount = false;
                }
                // a short walk with no mount along it: *"does not exist"*,
                // or a file with names after it, *"not a directory"*
                // (`chan.c:1078`)
                None => {
                    drop(tab, &mut c, owned);
                    let got = wq.qids.len();
                    return Err(if got == 0 || wq.qids[got - 1].is_dir() {
                        (nhave + got + 1, EDOESNOTEXIST.into())
                    } else {
                        (nhave + got, ENOTDIR.into())
                    });
                }
            },
            Some((i, h)) => {
                // *"if(wq->clone != nil){ cclose(wq->clone); …"*
                if let Some(mut clone) = wq.clone {
                    tab.dclose(&mut clone);
                }
                n = i + 1;
                let m = h.borrow().mount.clone();
                nc = (*m[0].chan).clone();
                mh = m;
                didmount = true;
            }
        }
        for name in &names[nhave..nhave + n] {
            path = crate::chan::addelem(&path, name);
        }
        drop(tab, &mut c, owned);
        c = nc;
        if didmount {
            owned = false;
            src = Some(mh[0].chan.clone());
        } else {
            owned = true;
            src = None;
        }
        nhave += n;
    }
    // `pathclose(c->path); c->path = path;` (`chan.c`, end of `walk`).
    c.path = path;
    // **The last element is NOT domounted here.** `walk` steps onto a mount
    // at the top of each iteration, before walking that component
    // (`chan.c:1020`); what to do about the last one is the access mode's
    // business, and two of the seven answer "nothing".
    let _ = owned;
    Ok((c, src))
}

/// `MAXWELEM` (`fcall.h:6`) — the most names one `Twalk` carries.
const MAXWELEM: usize = 16;

/// `Edoesnotexist` (`chan.c:963`) — a walk that stopped partway.
const EDOESNOTEXIST: &str = "does not exist";

/// `namec(name, amode, omode, perm)` for every access mode but `Aopen`,
/// which is [`open`]'s: an open answers a reference, and may answer a
/// channel that already exists. With the channel, **the reference it is if
/// it is not the caller's own**: the process's `slash` or `dot`, or a
/// mount's channel. `None` is a channel this walk made, the caller's to
/// close when it is done with it, as Plan 9's caller `cclose`s what `namec`
/// answers; one that keeps it — a `bind`, a `chdir` — keeps the reference.
pub fn namec(
    tab: &mut Devtab,
    ns: &Ns,
    slash: &Rc<Chan>,
    dot: &Rc<Chan>,
    name: &str,
    amode: A,
    omode: u16,
) -> Result<(Chan, Option<Rc<Chan>>), String> {
    if matches!(amode, A::Open) {
        return Err("Aopen goes through `open`, which answers a reference".into());
    }
    resolve(tab, ns, slash, dot, name, amode, omode).map(|(c, src, _)| (c, src))
}

/// `namec(name, Aopen, omode, 0)`: resolve, then *"c =
/// devtab[c->type]->open(c, omode&~OCEXEC)"* (`chan.c:1513`) — which
/// `dupopen` and `srvopen` answer with a channel that already exists, one
/// more reference to it ([`Devtab::dopen`]) — and the open modes that are
/// the channel's flags (`:1515`), on that channel.
pub fn open(
    tab: &mut Devtab,
    ns: &Ns,
    slash: &Rc<Chan>,
    dot: &Rc<Chan>,
    name: &str,
    omode: u16,
) -> Result<Rc<RefCell<Chan>>, String> {
    let (c, src, named) = resolve(tab, ns, slash, dot, name, A::Open, omode)?;
    openit(tab, c, src, omode).map_err(|e| named.all(e))
}

/// The open itself: close-on-exec is the channel's business, not the
/// device's, and a 9P server is never sent it. An open that fails leaves
/// the walked channel to be closed — *"if(waserror()){ cclose(c);
/// nexterror(); }"*.
fn openit(tab: &mut Devtab, c: Chan, src: Option<Rc<Chan>>, omode: u16) -> Result<Rc<RefCell<Chan>>, String> {
    let keep = c.clone();
    let c = match tab.dopen(c, omode & !crate::chan::mode::OCEXEC) {
        Ok(c) => c,
        Err(e) => {
            if src.is_none() && e != crate::devmnt::SLEPT {
                let mut keep = keep;
                tab.dclose(&mut keep);
            }
            return Err(e);
        }
    };
    opened(&mut c.borrow_mut(), omode);
    Ok(c)
}

/// `namec` up to the access mode's own work: the walk, the mount the last
/// element steps onto, and `cunique` — and what an error after them needs
/// to name the name.
fn resolve<'a>(
    tab: &mut Devtab,
    ns: &Ns,
    slash: &Rc<Chan>,
    dot: &Rc<Chan>,
    name: &'a str,
    amode: A,
    omode: u16,
) -> Result<(Chan, Option<Rc<Chan>>, Named<'a>), String> {
    let s = start(tab, ns, name, slash, dot)?;
    let named = s.named(name);
    // A walk of one element or more answers a channel of its own; none
    // answers the namespace's own `dot` or `slash`, which `cunique` below
    // must copy before anything opens or removes it.
    let (c, src) = walk(tab, ns, s.chan, s.src, &s.elems, s.nomount).map_err(|(n, e)| named.err(n, e))?;
    let refuse = |tab: &mut Devtab, mut c: Chan, owned: bool, e: &str| {
        if owned {
            tab.dclose(&mut c);
        }
        Err(named.all(e.to_string()))
    };
    // *"if(e.mustbedir && !(c->qid.type&QTDIR)) error("not a directory")"*
    // (`chan.c:1450`): `/etc/motd/` is not `/etc/motd`.
    if s.mustbedir && !c.is_dir() {
        return refuse(tab, c, src.is_none(), ENOTDIR);
    }
    // `chan.c:1453`: exec of a directory is refused here rather than by
    // the device, because only `namec` knows the caller asked for `OEXEC`.
    if matches!(amode, A::Open) && omode & 3 == crate::chan::mode::OEXEC && c.is_dir() {
        return refuse(tab, c, src.is_none(), "cannot exec directory");
    }
    let (c, src) = mounted(tab, ns, c, src, s.nomount, amode).map_err(|e| named.all(e))?;
    match amode {
        // `Aaccess`, `Abind`, `Amount` and `Aremove` resolve and stop.
        // **None requires a directory** — which is why `bind` can put a file
        // over a file, and why using `Atodir` for bind's sides was wrong.
        A::Access | A::Bind | A::Mount | A::Remove | A::Open => {}
        // *"case Atodir: … if(!(c->qid.type & QTDIR)) error(Enotdir)"*
        A::Todir => {
            if !c.is_dir() {
                return refuse(tab, c, src.is_none(), ENOTDIR);
            }
        }
        A::Create => {
            return refuse(tab, c, src.is_none(), "Acreate goes through `create`, which walks the parent");
        }
    }
    Ok((c, src, named))
}

/// `namec`'s switch on the access mode, up to its own work: whether the
/// last element steps onto what is mounted there, the mount head kept,
/// and `cunique`.
fn mounted(
    tab: &mut Devtab,
    ns: &Ns,
    mut c: Chan,
    mut src: Option<Rc<Chan>>,
    nomount: bool,
    amode: A,
) -> Result<(Chan, Option<Rc<Chan>>), String> {
    // Whether the LAST element steps onto what is mounted there, per access
    // mode (`chan.c:1456`). Two say no, and each says why:
    //
    // * **`Amount`** — *"When mounting on an already mounted upon directory,
    //   one wants subsequent mounts to be attached to the original directory,
    //   not the replacement"* (`:1532`). Without this a second `bind -a x /n`
    //   attaches to the first bind's channel instead of to `/n`, and a union
    //   can never have more than one element.
    // * **`Atodir`** — *"Directories (e.g. for cd) are left before the mount
    //   point, so one may mount on / or . and see the effect"* (`:1522`).
    if !nomount && !matches!(amode, A::Mount | A::Todir) {
        // *"save&update the name; domount might change c"* (`chan.c:1470`),
        // and after `cunique`: *"now it's our copy anyway, we can put the
        // name back"* — `pathclose(c->path); c->path = path`. `Abind` is the
        // one that does not, and says why: *"no need to maintain path —
        // cannot dotdot an Abind"* (`:1458`).
        let path = c.path.clone();
        let was = c.clone();
        let (first, head) = domount(tab, ns, c)?;
        if let Some(h) = &head {
            // the mount's channel is the namespace's; the one it replaced,
            // if the walk made it, is closed (`findmount`'s `cclose(*cp)`)
            if src.is_none() {
                let mut was = was;
                tab.dclose(&mut was);
            }
            src = Some(h.borrow().mount[0].chan.clone());
        }
        c = first;
        if !matches!(amode, A::Bind) {
            c.path = path;
        }
        // The mount head comes along for two of the modes, and for the
        // reasons [`Chan::umh`] records: for `Abind` always — *"c->umh = m"*
        // (`chan.c:1464`); `cmount` copies the rest of a union and refuses
        // a `-c` bind of one — and for `Aopen` **only when it has more than
        // one element**, *"only save the mount head if it's a multiple
        // element union"* (`:1501`).
        // *"record whether c is on a mount point"* (`chan.c:1485`)
        if matches!(amode, A::Access | A::Remove | A::Open) {
            c.ismtpt = head.is_some();
        }
        if let Some(h) = head {
            let many = h.borrow().mount.len() > 1;
            if matches!(amode, A::Bind) || (matches!(amode, A::Open) && many) {
                c.umh = Some(h);
            }
        }
    }
    // **`c = cunique(c)`** (`chan.c:1479`): *"our own copy to open or
    // remove"* — for `Aaccess`, `Aremove` and `Aopen`, mounted on or not.
    // Opening `.` without it opened the current directory's own fid, and
    // its close clunked it: every name relative to `.` after `ls` was
    // "unknown fid".
    if src.is_some() && matches!(amode, A::Access | A::Remove | A::Open) {
        let path = c.path.clone();
        c = tab.dcclone(&c)?;
        c.path = path;
        src = None;
    }
    Ok((c, src))
}

/// `namec`'s `Aopen` and `Acreate` after the device has opened or created:
/// the open modes that are really channel flags (`chan.c:1515`, `:1616`).
fn opened(c: &mut Chan, omode: u16) {
    if omode & crate::chan::mode::OCEXEC != 0 {
        c.flag |= crate::chan::flag::CCEXEC;
    }
    if omode & crate::chan::mode::ORCLOSE != 0 {
        c.flag |= crate::chan::flag::CRCLOSE;
    }
    c.flag |= crate::chan::flag::COPEN;
}

/// `Enocreate` (`port/error.h:8`).
const ENOCREATE: &str = "mounted directory forbids creation";

/// `namec(..., Acreate, ...)` (`chan.c:1540`): walk all but the last
/// element, then create it in the union's **create element** — the one
/// bound with `MCREATE` (`createdir`) — which is why a create can land in a
/// different file server from the one a read of the same directory would
/// answer.
pub fn create(
    tab: &mut Devtab,
    ns: &Ns,
    slash: &Rc<Chan>,
    dot: &Rc<Chan>,
    name: &str,
    omode: u16,
    perm: u32,
) -> Result<Rc<RefCell<Chan>>, String> {
    let s = start(tab, ns, name, slash, dot)?;
    let named = s.named(name);
    let n = s.elems.len();
    let attach = |tab: &mut Devtab, s: &Start| {
        if s.src.is_none() {
            let mut c = s.chan.clone();
            tab.dclose(&mut c);
        }
    };
    // *"perm must have DMDIR if last element is / or /."* (`chan.c:1430`),
    // with every element named; and *"don't try to walk the last path
    // element just yet"* — a name with none is `Eexist` (`:1436`).
    if s.mustbedir && perm & crate::ninep::DMDIR == 0 {
        attach(tab, &s);
        return Err(named.err(n, "create without DMDIR".into()));
    }
    if n == 0 {
        attach(tab, &s);
        return Err(EEXIST.into());
    }
    let last = s.elems[n - 1].clone();
    let (parent, psrc) = walk(tab, ns, s.chan, s.src, &s.elems[..n - 1], s.nomount).map_err(|(k, e)| named.err(k, e))?;
    let powned = psrc.is_none();
    // From here every error names the whole name: *"e.nelems++;
    // e.nerror++"* (`chan.c:1546`).
    let fail = |e: String| named.all(e);
    // What the walk made is closed once it is done with (`chan.c:1109`); a
    // close is skipped only when the call has left the processor, since it
    // runs again from the top and the record gives the closes back then.
    let close = |tab: &mut Devtab, c: &Chan, owned: bool| {
        if owned {
            let mut c = c.clone();
            tab.dclose(&mut c);
        }
    };
    // The last element walked from the parent, which the walk leaves for
    // this to close: *"walk(&c, e.elems+e.nelems-1, 1, nomount, nil)"*.
    let lookup = |tab: &mut Devtab| {
        let held = Some(psrc.clone().unwrap_or_else(|| Rc::new(parent.clone())));
        walk(tab, ns, parent.clone(), held, std::slice::from_ref(&last), s.nomount)
    };
    // **`create(2)` of a name that already exists is an OPEN with
    // `OTRUNC`** (`chan.c:1548`) — *"goto Open"*, `Aopen`'s, `OCEXEC` and
    // all — unless `OEXCL`, which is `Eexist`: the only way to reach
    // `create(5)`'s own semantics.
    //
    // Plan 9's comment names the very case that found this: *"The
    // create/create race is quite common. For example, it happens when two rc
    // subshells simultaneously update the same environment variable."* Here
    // it was not even a race — one rc, writing `/env/status` twice, told it
    // could not create what it had just created.
    let truncated = |tab: &mut Devtab, existing: Chan, esrc: Option<Rc<Chan>>| {
        let omode = omode | crate::chan::mode::OTRUNC;
        let (c, src) = mounted(tab, ns, existing, esrc, s.nomount, A::Open)?;
        openit(tab, c, src, omode)
    };
    match lookup(tab) {
        Ok((existing, esrc)) => {
            close(tab, &parent, powned);
            if omode & crate::chan::mode::OEXCL != 0 {
                close(tab, &existing, esrc.is_none());
                return Err(fail(EEXIST.into()));
            }
            return truncated(tab, existing, esrc).map_err(fail);
        }
        Err((_, e)) if e == crate::devmnt::SLEPT => return Err(e),
        Err(_) => {}
    }

    // **A directory mounted upon is created in through `createdir`**
    // (`chan.c:1590`): the element bound with `MCREATE`, and if there is
    // none, *"mounted directory forbids creation"* (`chan.c:1159`,
    // `Enocreate`). Only a directory nothing is mounted on is created in
    // itself.
    let target = match ns.elements(&parent) {
        Some(els) => match els.iter().find(|e| e.create()) {
            Some(e) => (*e.chan).clone(),
            None => {
                close(tab, &parent, powned);
                return Err(fail(ENOCREATE.into()));
            }
        },
        None => parent.clone(),
    };
    // **`cnew = cunique(cnew)`** (`chan.c:1606`): *"We need our own copy of
    // the Chan because we're about to send a create, which will move it."*
    // Without it a create in `.` moved the current directory's own fid onto
    // the new file — every relative name after `cd /tmp; echo a >x` answered
    // "unknown fid" — and a create in a union moved the mount's channel.
    let mut target = match tab.dcclone(&target) {
        Ok(t) => t,
        Err(e) => {
            close(tab, &parent, powned && e != crate::devmnt::SLEPT);
            return Err(fail(e));
        }
    };
    target.path = parent.path.clone();
    // *"devtab[cnew->type]->create(cnew, …, omode&~(OEXCL|OCEXEC), perm)"*
    // (`chan.c:1613`), and then the flags, as an open has them.
    let mode = omode & !(crate::chan::mode::OEXCL | crate::chan::mode::OCEXEC);
    if let Err(e) = tab.dcreate(&mut target, &last, mode, perm) {
        close(tab, &target, e != crate::devmnt::SLEPT);
        if e == crate::devmnt::SLEPT || omode & crate::chan::mode::OEXCL != 0 {
            close(tab, &parent, powned && e != crate::devmnt::SLEPT);
            return Err(fail(e));
        }
        // *"create failed"* (`chan.c:1630`): someone else's create may
        // have got there first, so the walk is made again, and what it
        // finds opened `OTRUNC`; if nothing, *"report true error"*.
        let again = lookup(tab);
        close(tab, &parent, powned);
        return match again {
            Ok((existing, esrc)) => truncated(tab, existing, esrc).map_err(fail),
            Err((_, w)) if w == crate::devmnt::SLEPT => Err(w),
            Err(_) => Err(fail(e)),
        };
    }
    close(tab, &parent, powned);
    opened(&mut target, omode);
    Ok(Rc::new(RefCell::new(target)))
}

/// `Enotdir` (`error.h:12`).
pub const ENOTDIR: &str = "not a directory";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devroot::Root;
    use crate::ns::{Bind, Element};

    fn tab_with_root() -> (Devtab, Rc<Chan>) {
        let mut tab = Devtab::new();
        let mut r = Root::new();
        r.addbootfile("init", b"the image".to_vec());
        let slash = Rc::new(r.attach("").unwrap());
        tab.add(Box::new(r));
        (tab, slash)
    }

    #[test]
    fn a_rooted_name_resolves_from_the_processs_root_channel() {
        let (mut tab, slash) = tab_with_root();
        let ns = Ns::new();
        let c = namec(&mut tab, &ns, &slash, &slash, "/boot/init", A::Access, 0).unwrap().0;
        assert_eq!(c.path, "#/boot/init", "the root device is `#/`, so the name joins without doubling");
    }

    #[test]
    fn a_name_that_is_not_there_says_so() {
        let (mut tab, slash) = tab_with_root();
        let ns = Ns::new();
        let e = namec(&mut tab, &ns, &slash, &slash, "/nothing", A::Access, 0).unwrap_err();
        assert!(e.contains("does not exist"), "{e}");
    }

    #[test]
    fn the_mount_driver_cannot_be_attached_by_name() {
        // chan.c: if(utfrune("M", r)) error(Enoattach). It is reached through
        // mount() and no other way.
        let (mut tab, slash) = tab_with_root();
        let ns = Ns::new();
        assert!(namec(&mut tab, &ns, &slash, &slash, "#M", A::Access, 0).is_err());
    }

    #[test]
    fn a_hash_path_ignores_what_is_mounted_over_it() {
        // nomount: `#/` gives you the device, not the namespace's view of it.
        let (mut tab, slash) = tab_with_root();
        let mut ns = Ns::new();
        let mut elsewhere = (*slash).clone();
        elsewhere.qid.path = 999;
        ns.mount(&slash, Element::new(elsewhere), Bind::Replace);

        let c = namec(&mut tab, &ns, &slash, &slash, "#/", A::Access, 0).unwrap().0;
        assert_eq!(c.qid, slash.qid, "the device itself, not the mount");
    }

    #[test]
    fn a_walk_steps_through_a_mount_point() {
        // The property the namespace exists for: a name resolves to what is
        // mounted on it, and the answer must be the MOUNTED file, not the one
        // underneath — which a bare is_ok() cannot tell apart.
        let mut tab = Devtab::new();
        let mut r = Root::new();
        r.addbootfile("init", b"under".to_vec());
        let slash = Rc::new(r.attach("").unwrap());
        tab.add(Box::new(r));

        let mut other = Other::new();
        let over = other.attach("").unwrap();
        tab.add(Box::new(other));

        let mut ns = Ns::new();
        ns.mount(&slash, Element::new(over), Bind::Replace);

        let c = namec(&mut tab, &ns, &slash, &slash, "/boot/init", A::Access, 0).expect("resolve").0;
        assert_eq!(
            c.dev,
            DevId::Srv,
            "the walk landed on the file under the mount, not the mounted one"
        );
    }

    /// Plan 9 checks `findmount` at EVERY component (the loop in `namec`,
    /// `chan.c:1317`), not once at the start. A mount made on a file that is
    /// reached by walking has to be honoured where it was made.
    #[test]
    fn the_mount_is_checked_at_every_component_not_only_the_first() {
        let mut tab = Devtab::new();
        let mut r = Root::new();
        r.addbootfile("init", b"under".to_vec());
        let slash = Rc::new(r.attach("").unwrap());
        tab.add(Box::new(r));

        // the channel for /init — reached by a walk, not the starting point
        let on = namec(&mut tab, &Ns::new(), &slash, &slash, "/boot/init", A::Access, 0).unwrap().0;
        assert_eq!(on.dev, DevId::Root);

        // a file over a file: `cmount` refuses a directory over one
        // (`chan.c:654`)
        let mut other = Other::new();
        let mut over = other.attach("").unwrap();
        over.qid.qtype = 0;
        tab.add(Box::new(other));

        let mut ns = Ns::new();
        ns.mount(&on, Element::new(over), Bind::Replace);

        let c = namec(&mut tab, &ns, &slash, &slash, "/boot/init", A::Access, 0).unwrap().0;
        assert_eq!(
            c.dev,
            DevId::Srv,
            "a mount made on a walked-to component was not honoured there"
        );
    }

    /// `chan.c:1376` — `RFNOMNT`'s sandbox, and the exception list is exactly
    /// `"|decp"`. A list that allowed anything more would let a sandboxed
    /// process attach a server and escape through it.
    #[test]
    fn noattach_permits_exactly_pipe_dup_env_cons_and_proc() {
        let mut tab = Devtab::new();
        let mut r = Root::new();
        r.addbootfile("init", b"x".to_vec());
        let slash = Rc::new(r.attach("").unwrap());
        tab.add(Box::new(r));
        tab.add(Box::new(Other::new()));

        let mut sandboxed = Ns::new();
        sandboxed.set_noattach(true);
        let open = Ns::new();

        // `#s` is not in "|decp", so it is refused in the sandbox and not
        // outside it. The device exists either way — this is the namespace
        // saying no, not the table.
        assert!(namec(&mut tab, &open, &slash, &slash, "#s", A::Access, 0).is_ok());
        let e = namec(&mut tab, &sandboxed, &slash, &slash, "#s", A::Access, 0).unwrap_err();
        assert_eq!(e, ENOATTACH);

        // and `#M` is refused in both, always
        for ns in [&open, &sandboxed] {
            assert_eq!(
                namec(&mut tab, ns, &slash, &slash, "#M", A::Access, 0).unwrap_err(),
                ENOATTACH
            );
        }
    }

    /// The starting point of a relative name is the process's `dot`, not its
    /// `slash`. Both are channels, and `namec` takes both.
    #[test]
    fn a_relative_name_resolves_from_dot() {
        let mut tab = Devtab::new();
        let mut r = Root::new();
        r.addbootfile("init", b"an image".to_vec());
        let slash = Rc::new(r.attach("").unwrap());
        tab.add(Box::new(r));
        let ns = Ns::new();

        let rooted = namec(&mut tab, &ns, &slash, &slash, "/boot/init", A::Access, 0).unwrap().0;
        let relative = namec(&mut tab, &ns, &slash, &slash, "boot/init", A::Access, 0)
            .expect("a relative name must resolve from dot").0;
        assert_eq!(rooted.qid, relative.qid);
    }

    /// A one-file device under a second letter, so a test can tell which of
    /// two files a walk landed on. One `Dev` per letter is Plan 9's own
    /// arrangement (`devtab[]`), so two instances of one device is not a
    /// thing a test can ask for.
    struct Other {
        qid: crate::ninep::Qid,
    }
    impl Other {
        fn new() -> Other {
            Other { qid: crate::ninep::Qid { qtype: crate::ninep::QTDIR, vers: 0, path: 0 } }
        }
    }
    impl crate::dev::Dev for Other {
        fn id(&self) -> DevId {
            DevId::Srv
        }
        fn as_any(&mut self) -> &mut dyn std::any::Any {
            self
        }
        fn attach(&mut self, _s: &str) -> Result<Chan, String> {
            Ok(Chan::attach(DevId::Srv, 0))
        }
        fn walk(&mut self, c: &Chan, n: &str) -> Result<Option<Chan>, String> {
            Ok(Some(c.walked(n, self.qid)))
        }
        fn open(&mut self, c: Chan, _m: u16) -> Result<Chan, String> {
            Ok(c)
        }
        fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
            Err("no".into())
        }
        fn read(&mut self, _c: &mut Chan, _n: usize, _o: u64) -> Result<Vec<u8>, String> {
            Ok(b"over".to_vec())
        }
        fn write(&mut self, _c: &mut Chan, _d: &[u8], _o: u64) -> Result<usize, String> {
            Err("no".into())
        }
        fn stat(&mut self, _c: &Chan) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
        fn wstat(&mut self, _c: &mut Chan, _e: &[u8]) -> Result<(), String> {
            Err("no".into())
        }
        fn remove(&mut self, _c: &mut Chan) -> Result<(), String> {
            Err("no".into())
        }
        fn close(&mut self, _c: &mut Chan) {}
    }

    /// A device whose walk FAILS for every name — as a 9P server answers a
    /// missing first name with `Rerror` rather than an empty `Rwalk`.
    struct Refuses;
    impl crate::dev::Dev for Refuses {
        fn id(&self) -> DevId {
            DevId::Env
        }
        fn as_any(&mut self) -> &mut dyn std::any::Any {
            self
        }
        fn attach(&mut self, _s: &str) -> Result<Chan, String> {
            let mut c = Chan::attach(DevId::Env, 0);
            c.qid.qtype = crate::ninep::QTDIR;
            Ok(c)
        }
        fn walk(&mut self, _c: &Chan, _n: &str) -> Result<Option<Chan>, String> {
            Err("file does not exist".into())
        }
        fn open(&mut self, c: Chan, _m: u16) -> Result<Chan, String> {
            Ok(c)
        }
        fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
            Err("no".into())
        }
        fn read(&mut self, _c: &mut Chan, _n: usize, _o: u64) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
        fn write(&mut self, _c: &mut Chan, _d: &[u8], _o: u64) -> Result<usize, String> {
            Err("no".into())
        }
        fn stat(&mut self, _c: &Chan) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
        fn wstat(&mut self, _c: &mut Chan, _e: &[u8]) -> Result<(), String> {
            Err("no".into())
        }
        fn remove(&mut self, _c: &mut Chan) -> Result<(), String> {
            Err("no".into())
        }
        fn close(&mut self, _c: &mut Chan) {}
    }

    /// **A walk that fails is a miss, and the union is still tried** —
    /// `ewalk`'s *"if(waserror()) return nil"* (`chan.c:948`), then *"try a
    /// union mount, if any"* (`:1027`). When the first element errs the
    /// walk goes on to the next; when every element misses, the error is
    /// the last device's.
    #[test]
    fn a_union_is_tried_when_its_first_element_errs() {
        let (mut tab, slash) = tab_with_root();
        let first = Refuses.attach("").unwrap();
        tab.add(Box::new(Refuses));
        let mut ns = Ns::new();
        ns.mount(&slash, Element::new(first), Bind::Replace);
        ns.mount(&slash, Element::shared(slash.clone(), crate::ns::mflag::MAFTER, ""), Bind::After);

        let c = namec(&mut tab, &ns, &slash, &slash, "/boot/init", A::Access, 0)
            .expect("the second element has /boot/init").0;
        assert_eq!(c.dev, DevId::Root);
        let e = namec(&mut tab, &ns, &slash, &slash, "/nothing", A::Access, 0).unwrap_err();
        assert!(e.contains("does not exist"), "{e}");
    }

    /// **A union with no `MCREATE` element forbids creation** —
    /// `createdir`'s *"error(Enocreate)"* (`chan.c:1159`) — rather than
    /// creating in the directory mounted upon.
    #[test]
    fn a_union_without_a_create_element_forbids_creation() {
        let (mut tab, slash) = tab_with_root();
        let refuses = Refuses.attach("").unwrap();
        tab.add(Box::new(Refuses));
        let mut ns = Ns::new();
        ns.mount(&slash, Element::new(refuses), Bind::After);
        let e = create(&mut tab, &ns, &slash, &slash, "/new", crate::chan::mode::OWRITE, 0o666).unwrap_err();
        assert_eq!(e, format!("'/new' {ENOCREATE}"));
    }

    #[test]
    fn opening_for_writing_is_refused_by_the_root() {
        let (mut tab, slash) = tab_with_root();
        let ns = Ns::new();
        let e = open(&mut tab, &ns, &slash, &slash, "/boot/init", crate::chan::mode::OWRITE);
        assert_eq!(e.unwrap_err(), "'/boot/init' permission denied", "devopen's Eperm: every file in #/ is 0555");
    }

    /// `namelenerror` (`chan.c:1250`): a short name whole and quoted; a
    /// long one as `...` and a suffix from a `/`; the whole in `ERRMAX`.
    #[test]
    fn a_name_in_an_error_is_quoted_and_kept_short() {
        assert_eq!(nameerror("/n/x", EISMTPT), "'/n/x' is a mount point");
        assert_eq!(nameerror("/it's", EISMTPT), "'/it''s' is a mount point");
        let long = format!("/{}/{}/end", "a".repeat(40), "b".repeat(40));
        let e = nameerror(&long, EISMTPT);
        assert!(e.starts_with("'.../") && e.ends_with("/end' is a mount point"), "{e}");
        assert!(e.len() < crate::proc::ERRMAX);
    }
    /// `parsename` (`chan.c:1196`): `/` and `.` are skipped, `..` and `x.`
    /// are elements, and a name ending in `/` or `/.` must be a directory.
    #[test]
    fn a_name_parses_as_parsename_parses_it() {
        fn elems(n: &str) -> (Vec<&str>, bool) {
            let (spans, dir) = parsename(n, 0);
            (spans.into_iter().map(|r| &n[r]).collect(), dir)
        }
        assert_eq!(elems("/a/./b//c"), (vec!["a", "b", "c"], false));
        assert_eq!(elems("x."), (vec!["x."], false));
        assert_eq!(elems("/tmp/.."), (vec!["tmp", ".."], false));
        assert_eq!(elems("a/b/."), (vec!["a", "b"], true));
        assert_eq!(elems("/"), (Vec::<&str>::new(), true));
        assert_eq!(elems("."), (Vec::<&str>::new(), true));
    }
}

/// A 9P wire over an ordinary channel — `devtab[m->c->type]`'s `bwrite` and
/// `bread` (`devmnt.c`, `mountio`), with the table passed rather than global.
///
/// The mount driver is out of `tab` while this exists, so a wire can never be
/// a mounted channel served by the driver using it.
struct Wire<'a> {
    wire: Chan,
    tab: &'a mut Devtab,
}

impl<'a> Wire<'a> {
    fn new(
        c: &Chan,
        m: &crate::devmnt::MntDev,
        tab: &'a mut Devtab,
    ) -> Result<Wire<'a>, String> {
        let wire = m.wire_of(c).ok_or("not mounted")?;
        Ok(Wire { wire, tab })
    }
}

/// **What a call has done on the wires, kept while it sleeps.** Plan 9's
/// `mountio` sleeps in the middle of a call on the process's own kernel
/// stack (`devmnt.c:811`) and carries on from there. This kernel has no
/// stack per process: a call that sleeps leaves, and runs again from the
/// top when it is woken — as `qread` does. So what it did the first time is
/// kept, in order — the fids it was given and the replies it had — and
/// given back the same way when it runs again, so nothing is sent twice and
/// the server sees one conversation. The record ends with the call.
#[derive(Default)]
pub struct Record {
    steps: Vec<Step>,
    at: usize,
}

enum Step {
    /// `++chanalloc.fid`'s answer.
    Fid(u32),
    /// A close [`Devtab::cclose`] deferred.
    Once,
    /// One RPC: the tag it went with, whether it has gone, and how it
    /// ended once it has — its reply, or `Eintr` for one flushed.
    Rpc { tag: u16, sent: bool, reply: Option<Result<Vec<u8>, String>>, flush: Flush },
}

/// An RPC interrupted by a note: *"r = mntflushalloc(r, m->msize)"*
/// (`devmnt.c:782`) — a `Tflush` for it, and another for each note after,
/// newest last; whether the newest has gone; and the RPC's own reply, if it
/// came first.
#[derive(Clone, Default)]
struct Flush {
    tags: Vec<u16>,
    sent: bool,
    got: Option<Vec<u8>>,
}

/// A wire's reader and its outstanding RPCs — `m->rip`, `m->queue` and the
/// replies `mountmux` has taken for others (`devmnt.c:930`).
#[derive(Default)]
struct Mux {
    /// `m->rip` — the one process reading the wire.
    rip: Option<crate::proc::Pid>,
    /// `m->queue` — each outstanding RPC by its tag, and who waits for it:
    /// nobody once its process has gone.
    queue: Vec<(u16, Option<crate::proc::Pid>)>,
    /// Replies read by another process, waiting for their owner.
    done: HashMap<u16, Vec<u8>>,
    /// A message read in part — `m->q`.
    inbuf: Vec<u8>,
    /// The next tag to try.
    tag: u16,
}

impl Devtab {
    /// `mntralloc`'s tag (`devmnt.c:1047`): one no RPC outstanding on the
    /// wire has, whichever mount of the wire made it; never `NOTAG`.
    fn newtag(&mut self, key: (DevId, u32, u64)) -> u16 {
        let m = self.muxes.entry(key).or_default();
        loop {
            m.tag = m.tag.wrapping_add(1);
            if m.tag != !0 && !m.queue.iter().any(|e| e.0 == m.tag) {
                break m.tag;
            }
        }
    }

    /// The process in the call.
    fn uppid(&self) -> crate::proc::Pid {
        self.up.as_ref().map_or(0, |u| u.borrow().pid)
    }

    /// Whether the process has left the processor in this call.
    fn asleep(&self, pid: crate::proc::Pid) -> bool {
        // A caller holding the table for writing is on a path where nothing
        // sleeps — a server the machine answers at once.
        self.up.as_ref().is_some_and(|u| {
            u.borrow().procs.try_borrow().is_ok_and(|procs| procs.get(pid).is_some_and(|p| p.setlabel))
        })
    }

    /// Hold a mounted wire (see [`Devtab::wires`]).
    pub fn keepwire(&mut self, cell: std::rc::Rc<std::cell::RefCell<Chan>>) {
        let key = {
            let w = cell.borrow();
            (w.dev, w.devno, w.qid.path)
        };
        self.wires.entry(key).or_insert(cell);
    }

    /// Let go of each wire whose last channel the mount driver has just
    /// closed — *"if(c->mchan != nil){ cclose(c->mchan); …"* (`chanfree`,
    /// `chan.c:475`) — which, if nothing else holds it, closes it: a pipe's
    /// server reads end of file.
    fn unwire(&mut self) {
        let gone = self.with_mnt(|m, _| m.released()).unwrap_or_default();
        for w in gone {
            if let Some(cell) = self.wires.remove(&w) {
                self.cclose(cell);
            }
        }
    }

    /// A call entered again: its record is given back from the start.
    pub fn rewind(&mut self, pid: crate::proc::Pid) {
        if let Some(r) = self.records.get_mut(&pid) {
            r.at = 0;
        }
    }

    /// The call is over.
    pub fn endcall(&mut self, pid: crate::proc::Pid) {
        self.records.remove(&pid);
    }

    /// The process is gone: nothing waits for its replies, and if it was
    /// reading a wire, the next may (`mntgate`).
    pub fn forget(&mut self, pid: crate::proc::Pid) {
        self.records.remove(&pid);
        let keys: Vec<_> = self.muxes.keys().copied().collect();
        for k in keys {
            let m = self.muxes.get_mut(&k).expect("key");
            for e in m.queue.iter_mut() {
                if e.1 == Some(pid) {
                    e.1 = None;
                }
            }
            if m.rip == Some(pid) {
                self.gate(k);
            }
        }
    }

    /// `mntgate` (`devmnt.c:918`): the reader leaves, and the first RPC
    /// still waiting is woken to read in its place.
    fn gate(&mut self, key: (DevId, u32, u64)) {
        let Some(m) = self.muxes.get_mut(&key) else { return };
        m.rip = None;
        let waiting: Vec<_> = m.queue.iter().filter_map(|e| e.1).collect();
        if waiting.is_empty() {
            return;
        }
        if let Some(u) = &self.up {
            let u = u.borrow();
            let mut procs = u.procs.borrow_mut();
            for p in waiting {
                if procs.wakeup(crate::proc::Rid::Mntrpc(p)).is_some() {
                    break;
                }
            }
        }
    }
}

impl crate::devmnt::Transport for Wire<'_> {
    fn fid(&mut self, next: u32) -> u32 {
        let pid = self.tab.uppid();
        let r = self.tab.records.entry(pid).or_default();
        if let Some(Step::Fid(f)) = r.steps.get(r.at) {
            let f = *f;
            r.at += 1;
            return f;
        }
        r.steps.truncate(r.at);
        r.steps.push(Step::Fid(next));
        r.at += 1;
        next
    }

    /// `mountio` (`devmnt.c:774`): send, then take the wire's reader's
    /// place if it is free and read until this RPC's reply has come —
    /// handing each other reply to its owner (`mountmux`) — or, if another
    /// process is reading, sleep until it has handed this one over or left
    /// the wire free (`mntgate`).
    fn rpc(&mut self, request: &[u8]) -> Result<Vec<u8>, String> {
        use crate::devmnt::SLEPT;
        let pid = self.tab.uppid();
        // A call that has already left does nothing more before it runs
        // again: everything after the sleep is done then.
        if self.tab.asleep(pid) {
            return Err(SLEPT.into());
        }
        let key = (self.wire.dev, self.wire.devno, self.wire.qid.path);

        // The record: an RPC over, or one under way.
        let rec = self.tab.records.entry(pid).or_default();
        let at = rec.at;
        let (tag, sent) = match rec.steps.get(at) {
            Some(Step::Rpc { reply: Some(r), .. }) => {
                let r = r.clone();
                rec.at += 1;
                return r;
            }
            Some(Step::Rpc { tag, sent, .. }) => (*tag, *sent),
            _ => {
                rec.steps.truncate(at);
                // `Tversion` keeps its NOTAG; anything else is given one
                // free on the wire
                let asked = u16::from_le_bytes([request.get(5).copied().unwrap_or(0), request.get(6).copied().unwrap_or(0)]);
                let tag = if asked == !0 { asked } else { self.tab.newtag(key) };
                let rec = self.tab.records.entry(pid).or_default();
                rec.steps.push(Step::Rpc { tag, sent: false, reply: None, flush: Flush::default() });
                (tag, false)
            }
        };

        if !sent {
            let mut req = request.to_vec();
            if req.len() >= 7 {
                req[5..7].copy_from_slice(&tag.to_le_bytes());
            }
            let d = self.tab.get(self.wire.dev).ok_or("no such device")?;
            d.write(&mut self.wire, &req, 0)?;
            if self.tab.asleep(pid) {
                return Err(SLEPT.into());
            }
            let m = self.tab.muxes.entry(key).or_default();
            m.queue.push((tag, Some(pid)));
            if let Some(Step::Rpc { sent, .. }) = self.tab.records.entry(pid).or_default().steps.get_mut(at) {
                *sent = true;
            }
        }

        'mountio: loop {
            // A `Tflush` made and not yet on the wire: *"Transmit a file
            // system rpc"* (`devmnt.c:794`), the flush being one.
            let flush = self.flushof(pid, at);
            if let (Some(&f), false) = (flush.tags.last(), flush.sent) {
                let req = crate::ninep::W::new().u16(tag).frame(crate::ninep::T::Flush as u8, f);
                let d = self.tab.get(self.wire.dev).ok_or("no such device")?;
                if let Err(e) = d.write(&mut self.wire, &req, 0) {
                    return self.dropped(key, pid, at, tag, e);
                }
                if self.tab.asleep(pid) {
                    return Err(SLEPT.into());
                }
                self.flushing(pid, at, |f| f.sent = true);
            }
            // What `mountmux` gave it while it slept: its own reply, which
            // ends it unless a flush is out — then it is kept — and the
            // newest flush's, which ends it either way.
            let flushes = self.flushof(pid, at).tags;
            if let Some(r) = self.tab.muxes.entry(key).or_default().done.remove(&tag) {
                if flushes.is_empty() {
                    return Ok(self.finish(pid, at, r));
                }
                self.flushing(pid, at, |f| f.got = Some(r));
            }
            if let Some(&f) = flushes.last() {
                if self.tab.muxes.entry(key).or_default().done.remove(&f).is_some() {
                    return self.flushed(key, pid, at, tag);
                }
            }
            // `m->rip`: one reader at a time.
            let m = self.tab.muxes.entry(key).or_default();
            match m.rip {
                Some(p) if p != pid => {
                    let slept = match &self.tab.up {
                        Some(u) => u.borrow().procs.borrow_mut().sleep(pid, crate::proc::Rid::Mntrpc(pid), false),
                        None => false,
                    };
                    if !slept {
                        // `sleep`'s *"up->notepending = 0; … error(Eintr)"*
                        // (`proc.c:880`), and `mountio` flushes
                        self.flush(key, pid, at);
                        continue 'mountio;
                    }
                    return Err(SLEPT.into());
                }
                _ => m.rip = Some(pid),
            }
            // `mntrpcread`: the size, then the rest.
            let msg = loop {
                let m = self.tab.muxes.entry(key).or_default();
                let have = m.inbuf.len();
                let size = if have >= 4 {
                    u32::from_le_bytes([m.inbuf[0], m.inbuf[1], m.inbuf[2], m.inbuf[3]]) as usize
                } else {
                    4
                };
                if have >= 4 && size < 7 {
                    return self.dropped(key, pid, at, tag, "short reply".into());
                }
                if have >= size && have >= 4 {
                    break m.inbuf.drain(..size).collect::<Vec<u8>>();
                }
                let d = self.tab.get(self.wire.dev).ok_or("no such device")?;
                let got = match d.read(&mut self.wire, size - have, 0) {
                    Ok(b) => b,
                    // *"if(m->rip == up) mntgate(m)"*, then the flush
                    // (`devmnt.c:777`); the wire's read took the note
                    Err(e) if e == crate::proc::EINTR => {
                        self.tab.gate(key);
                        self.flush(key, pid, at);
                        continue 'mountio;
                    }
                    Err(e) => return self.dropped(key, pid, at, tag, e),
                };
                if self.tab.asleep(pid) {
                    return Err(SLEPT.into());
                }
                if got.is_empty() {
                    // `Emountrpc` (`devmnt.c:822`): the wire is hung up.
                    return self.dropped(key, pid, at, tag, "mount rpc error".into());
                }
                self.tab.muxes.entry(key).or_default().inbuf.extend_from_slice(&got);
            };
            // `mountmux` (`devmnt.c:940`): the reply goes to its RPC.
            let rtag = u16::from_le_bytes([msg[5], msg[6]]);
            let m = self.tab.muxes.entry(key).or_default();
            let owner = m.queue.iter().position(|e| e.0 == rtag).map(|i| m.queue.remove(i));
            let flushes = self.flushof(pid, at).tags;
            if rtag == tag {
                if flushes.is_empty() {
                    self.tab.gate(key);
                    return Ok(self.finish(pid, at, msg));
                }
                // answered before its flush was: kept, and the flush still
                // waited for (`mntflushfree` finds it done)
                self.flushing(pid, at, |f| f.got = Some(msg));
                continue;
            }
            if flushes.last() == Some(&rtag) {
                self.tab.gate(key);
                return self.flushed(key, pid, at, tag);
            }
            // an older flush's answer: the chain's, waited for no longer
            if flushes.contains(&rtag) {
                continue;
            }
            // Someone else's, or nobody's — one whose process is gone,
            // which Plan 9 prints as *"unexpected reply tag"* and drops.
            if let Some((_, Some(p))) = owner {
                self.tab.muxes.entry(key).or_default().done.insert(rtag, msg);
                if let Some(u) = &self.tab.up {
                    u.borrow().procs.borrow_mut().wakeup(crate::proc::Rid::Mntrpc(p));
                }
            }
        }
    }

}

impl Wire<'_> {
    /// The RPC over, with its reply.
    fn finish(&mut self, pid: crate::proc::Pid, at: usize, reply: Vec<u8>) -> Vec<u8> {
        let rec = self.tab.records.entry(pid).or_default();
        if let Some(Step::Rpc { reply: r, .. }) = rec.steps.get_mut(at) {
            *r = Some(Ok(reply.clone()));
        }
        rec.at = at + 1;
        reply
    }

    fn flushof(&mut self, pid: crate::proc::Pid, at: usize) -> Flush {
        match self.tab.records.entry(pid).or_default().steps.get(at) {
            Some(Step::Rpc { flush, .. }) => flush.clone(),
            _ => Flush::default(),
        }
    }

    fn flushing(&mut self, pid: crate::proc::Pid, at: usize, f: impl FnOnce(&mut Flush)) {
        if let Some(Step::Rpc { flush, .. }) = self.tab.records.entry(pid).or_default().steps.get_mut(at) {
            f(flush);
        }
    }

    /// `mntflushalloc` (`devmnt.c:980`): a `Tflush` for the RPC — its
    /// `oldtag` the RPC's own, the first time and every time after — on the
    /// wire's queue, to be sent.
    fn flush(&mut self, key: (DevId, u32, u64), pid: crate::proc::Pid, at: usize) {
        if let Some(u) = &self.tab.up {
            u.borrow().procs.borrow_mut().interrupted(pid);
        }
        let f = self.tab.newtag(key);
        self.tab.muxes.entry(key).or_default().queue.push((f, Some(pid)));
        self.flushing(pid, at, |fl| {
            fl.tags.push(f);
            fl.sent = false;
        });
    }

    /// `mntflushfree` (`devmnt.c:1004`) once the newest flush is answered:
    /// the RPC and its flushes off the queue, and the RPC answered by its
    /// own reply if it came, or *"case Rflush: error(Eintr)"* (`mountrpc`,
    /// `devmnt.c:754`).
    fn flushed(&mut self, key: (DevId, u32, u64), pid: crate::proc::Pid, at: usize, tag: u16) -> Result<Vec<u8>, String> {
        let flush = self.flushof(pid, at);
        let m = self.tab.muxes.entry(key).or_default();
        m.queue.retain(|e| e.0 != tag && !flush.tags.contains(&e.0));
        m.done.remove(&tag);
        for f in &flush.tags {
            m.done.remove(f);
        }
        let r = flush.got.ok_or_else(|| crate::proc::EINTR.to_string());
        let rec = self.tab.records.entry(pid).or_default();
        if let Some(Step::Rpc { reply, .. }) = rec.steps.get_mut(at) {
            *reply = Some(r.clone());
        }
        rec.at = at + 1;
        r
    }

    /// An RPC the wire failed: it, and any flush of it, off the queue —
    /// and the wire left for the next reader.
    fn dropped(&mut self, key: (DevId, u32, u64), pid: crate::proc::Pid, at: usize, tag: u16, e: String) -> Result<Vec<u8>, String> {
        let flush = self.flushof(pid, at);
        if let Some(m) = self.tab.muxes.get_mut(&key) {
            m.queue.retain(|q| q.0 != tag && !flush.tags.contains(&q.0));
            if m.rip == Some(pid) {
                self.tab.gate(key);
            }
        }
        Err(e)
    }
}

/// `Enoattach` (`error.h:47`).
pub const ENOATTACH: &str = "mount/attach disallowed";
/// `Ebadsharp` (`error.h:11`) — a `#` name whose letter is no device's
/// (`chan.c:1380`).
const EBADSHARP: &str = "unknown device in # filename";

/// `Ebadchar` (`error.h:14`).
pub const EBADCHAR: &str = "bad character in file name";

/// `Eismtpt` (`error.h:4`).
pub const EISMTPT: &str = "is a mount point";

/// `nameerror` → `namelenerror` (`chan.c:1250`): the name — a suffix of it
/// from a `/`, after `...`, when the whole would not leave room (`:1262`) —
/// as *"%#q %s"* with the error (`:1289`), in `ERRMAX`. `%#q` quotes
/// whatever the name holds.
pub fn nameerror(name: &str, err: &str) -> String {
    const ERRMAX: usize = crate::proc::ERRMAX;
    let fits = |len: usize| len < ERRMAX / 3 || len + err.len() < 2 * ERRMAX / 3;
    let shown = if fits(name.len()) {
        name.to_string()
    } else {
        // *"Print a suffix of the name, but try to get a little info"*
        let ename = name.len();
        let mut next = ename;
        let mut at;
        loop {
            at = next;
            next = name.as_bytes()[..at].iter().rposition(|&b| b == b'/').unwrap_or(0);
            if !fits(ename - next) {
                break;
            }
        }
        // *"If the name is ridiculously long, chop it"*, out of the UTF
        // sequence it lands in
        if at == ename {
            at = ename - ERRMAX / 4;
            while !name.is_char_boundary(at) {
                at += 1;
            }
        }
        format!("...{}", &name[at..])
    };
    let e = format!("'{}' {err}", shown.replace('\'', "''"));
    let mut n = e.len().min(ERRMAX - 1);
    while !e.is_char_boundary(n) {
        n -= 1;
    }
    e[..n].to_string()
}

/// `validname0`'s look at each character (`chan.c:1731`): below `Runeself`,
/// one `isfrog` marks — the controls, `/` and DEL (`:1678`) — is refused,
/// `/` only without `slashok`, and the error carries the name: *"%s: %q",
/// Ebadchar, aname* (`:1739`), in `up->genbuf`'s 128 bytes.
pub fn validname(name: &str, slashok: bool) -> Result<(), String> {
    let isfrog = |c: u8| c < 0x20 || c == b'/' || c == 0x7f;
    if name.bytes().any(|c| c < 0x80 && isfrog(c) && (!slashok || c != b'/')) {
        // `%q` (`fmtquote.c:54`): quoted when a character is `' '` or
        // below, or is `'`, which is doubled — and here one always is
        let e = format!("{EBADCHAR}: '{}'", name.replace('\'', "''"));
        let mut n = e.len().min(127);
        while !e.is_char_boundary(n) {
            n -= 1;
        }
        return Err(e[..n].to_string());
    }
    Ok(())
}
