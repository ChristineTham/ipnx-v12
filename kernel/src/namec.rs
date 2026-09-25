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
use std::collections::HashMap;

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
    /// **Each mounted wire, held** — the `Mnt`'s `m->c`, a reference of its
    /// own (`devmnt.c:355`, *"incref(m->c)"*), so that the process which
    /// mounted it may close its descriptor — `plumber` does, `fsys.c:221` —
    /// without hanging the server up.
    wires: HashMap<(DevId, u32, u64), std::rc::Rc<std::cell::RefCell<Chan>>>,
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

    pub fn dopen(&mut self, c: Chan, mode: u16) -> Result<Chan, String> {
        // `srvopen` answers the posted channel, *"incref(sp->chan)"*
        // (`devsrv.c:135`): a copy here, which its own device counts.
        if c.dev == DevId::Srv && !c.qid.is_dir() {
            let posted = self.get(c.dev).ok_or("no such device")?.open(c, mode)?;
            if posted.dev != DevId::Srv {
                if let Some(d) = self.get(posted.dev) {
                    d.incref(&posted);
                }
            }
            return Ok(posted);
        }
        if c.dev == DevId::Mnt {
            return self.with_mnt(|m, tab| {
                let mut w = Wire::new(&c, m, tab)?;
                m.open(&mut w, c.clone(), mode)
            })?;
        }
        self.get(c.dev).ok_or("no such device")?.open(c, mode)
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
            return self.with_mnt(|m, tab| {
                let mut w = Wire::new(&cc, m, tab)?;
                m.remove(&mut w, &mut cc)
            })?;
        }
        self.get(c.dev).ok_or("no such device")?.remove(c)
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
            return;
        }
        if let Some(d) = self.get(c.dev) {
            d.close(c)
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
}

/// Step 1: the starting point.
///
/// `#M` is refused by name, and that is Plan 9's rule, not a policy of ours:
/// `chan.c` has `if(utfrune("M", r)) error(Enoattach);` — the mount driver is
/// reached through `mount()` and no other way.
pub fn start(
    tab: &mut Devtab,
    ns: &Ns,
    name: &str,
    slash: &Chan,
    dot: &Chan,
) -> Result<(Start, Vec<String>), String> {
    if name.is_empty() {
        return Err("empty file name".into());
    }
    if name.starts_with('#') {
        let (id, spec, below) = dev::split(name).ok_or("bad # in file name")?;
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
        let d = tab.get(id).ok_or("bad # in file name")?;
        let chan = d.attach(spec)?;
        return Ok((Start { chan, nomount: true }, elems(below)));
    }
    if let Some(rest) = name.strip_prefix('/') {
        return Ok((Start { chan: slash.clone(), nomount: false }, elems(rest)));
    }
    Ok((Start { chan: dot.clone(), nomount: false }, elems(name)))
}

fn elems(path: &str) -> Vec<String> {
    path.split('/').filter(|s| !s.is_empty() && *s != ".").map(|s| s.to_string()).collect()
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
/// The rest are the union's other elements, in order, which a walk tries when
/// the first has no such name (`chan.c:1034`). They are NOT cloned: walking a
/// channel does not move it, and a walk is all they are used for.
fn domount(
    tab: &mut Devtab,
    ns: &Ns,
    c: Chan,
) -> Result<(Chan, Vec<crate::ns::Element>), String> {
    let els: Vec<crate::ns::Element> = match ns.findmount(&c) {
        Some(els) if !els.is_empty() => els.to_vec(),
        _ => return Ok((c, Vec::new())),
    };
    let mut m = tab.dcclone(&els[0].chan)?;
    m.path = c.path.clone(); // the name is how we got here, not where we landed
    Ok((m, els))
}

/// `walk` (`chan.c:965`): the elements, one at a time, stepping through
/// mounts — and through UNIONS.
pub fn walk(
    tab: &mut Devtab,
    ns: &Ns,
    mut c: Chan,
    names: &[String],
    nomount: bool,
) -> Result<Chan, String> {
    // **The path is carried beside the channel, not taken from it**
    // (`chan.c`, `walk`): `path = c->path` before the loop, `path =
    // addelem(path, names[nhave+i], mtpt)` for each name, and `c->path =
    // path` at the end. `domount` does not touch the text — it only records
    // the mount point — so a name keeps the name it was walked by, and a
    // file on the far side of a mount is `/root/wasm/bin` rather than the
    // mount driver's `#M/wasm/bin`.
    let mut path = c.path.clone();
    for name in names {
        if !c.is_dir() {
            return Err("not a directory".into());
        }
        // `..` does not step onto a mount: it goes back out of one. Plan 9
        // undomounts here, which needs the mount head a channel was derived
        // from; this subset does not carry that yet, so `..` walks the device
        // and is honest about only that.
        let mut union = Vec::new();
        if name != ".." && !nomount {
            let (first, rest) = domount(tab, ns, c)?;
            c = first;
            union = rest;
        }
        // **`ewalk`** (`chan.c:948`): *"if(waserror()) return nil"* — a
        // device's walk that FAILS is a miss like one that finds nothing, so
        // the union is still tried. A 9P server answers a missing first name
        // with `Rerror`, not an empty `Rwalk`; propagating that error here
        // meant a union whose first element was a mounted directory never
        // reached its second (`/home` over `/usr/kitty`, with `bind -a /etc
        // /home` after it, listed `motd` and could not open it).
        match tab.dwalk(&c, name) {
            Ok(Some(next)) => c = next,
            Err(e) if e == crate::devmnt::SLEPT => return Err(e),
            miss => {
                let mut err = miss.err();
                // **"try a union mount, if any"** (`chan.c:1027`). The first
                // element is the one just walked, so this starts at the next
                // — `for(f = (f? f->next: f); f; f = f->next)` (`:1034`).
                // Without it a `bind -a` puts an element in a list nothing
                // ever reaches, and a union is a word rather than a thing.
                let mut found = None;
                for alt in union.into_iter().skip(1) {
                    match tab.dwalk(&alt.chan, name) {
                        Ok(Some(next)) => {
                            found = Some(next);
                            break;
                        }
                        Ok(None) => {}
                        Err(e) if e == crate::devmnt::SLEPT => return Err(e),
                        Err(e) => err = Some(e),
                    }
                }
                match found {
                    Some(next) => c = next,
                    // Every element missed: the error is the last device's,
                    // as `walk` returns -1 with it still set (`:1041`).
                    None => return Err(err.unwrap_or_else(|| format!("'{}' does not exist", name))),
                }
            }
        }
        path = crate::chan::addelem(&path, name);
    }
    // `pathclose(c->path); c->path = path;` (`chan.c`, end of `walk`).
    c.path = path;
    // **The last element is NOT domounted here.** `walk` steps onto a mount
    // at the top of each iteration, before walking that component
    // (`chan.c:1020`); what to do about the last one is the access mode's
    // business, and two of the seven answer "nothing".
    Ok(c)
}

/// `namec(name, amode, omode, perm)`.
pub fn namec(
    tab: &mut Devtab,
    ns: &Ns,
    slash: &Chan,
    dot: &Chan,
    name: &str,
    amode: A,
    omode: u16,
) -> Result<Chan, String> {
    let (s, names) = start(tab, ns, name, slash, dot)?;
    // A walk of one element or more answers a channel of its own; none
    // answers the namespace's own `dot` or `slash`, which `cunique` below
    // must copy before anything opens or removes it.
    let mut owned = !names.is_empty();
    let mut c = walk(tab, ns, s.chan, &names, s.nomount)?;
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
    if !s.nomount && !matches!(amode, A::Mount | A::Todir) {
        // *"save&update the name; domount might change c"* (`chan.c:1470`),
        // and after `cunique`: *"now it's our copy anyway, we can put the
        // name back"* — `pathclose(c->path); c->path = path`. `Abind` is the
        // one that does not, and says why: *"no need to maintain path —
        // cannot dotdot an Abind"* (`:1458`).
        let path = c.path.clone();
        let (first, rest) = domount(tab, ns, c)?;
        c = first;
        owned |= !rest.is_empty();
        if !matches!(amode, A::Bind) {
            c.path = path;
        }
        // The union comes along for two of the modes, and for the reasons
        // [`Chan::umh`] records. **Only when it has more than one element**
        // (`chan.c:1502`), because one element is not a union.
        if rest.len() > 1 && matches!(amode, A::Bind | A::Open) {
            c.umh = rest;
        }
    }
    // **`c = cunique(c)`** (`chan.c:1479`): *"our own copy to open or
    // remove"* — for `Aaccess`, `Aremove` and `Aopen`, mounted on or not.
    // Opening `.` without it opened the current directory's own fid, and
    // its close clunked it: every name relative to `.` after `ls` was
    // "unknown fid".
    if !owned && matches!(amode, A::Access | A::Remove | A::Open) {
        let path = c.path.clone();
        c = tab.dcclone(&c)?;
        c.path = path;
    }
    match amode {
        // `Aaccess`, `Abind`, `Amount` and `Aremove` resolve and stop.
        // **None requires a directory** — which is why `bind` can put a file
        // over a file, and why using `Atodir` for bind's sides was wrong.
        A::Access | A::Bind | A::Mount | A::Remove => {}
        A::Todir => {
            if !c.is_dir() {
                return Err("not a directory".into());
            }
        }
        A::Create => {
            return Err("Acreate goes through `create`, which walks the parent".into());
        }
        A::Open => {
            // `chan.c`: exec of a directory is refused here rather than by the
            // device, because only `namec` knows the caller asked for `OEXEC`.
            if omode & 3 == crate::chan::mode::OEXEC && c.is_dir() {
                return Err("cannot exec directory".into());
            }
            c = tab.dopen(c, omode)?;
            // `namec`'s `Aopen`: the open modes that are really channel flags
            // (`<libc.h>`, and `devdup.c`'s `if(omode & OCEXEC)`).
            if omode & crate::chan::mode::ORCLOSE != 0 {
                c.flag |= crate::chan::flag::CRCLOSE;
            }
            if omode & crate::chan::mode::OCEXEC != 0 {
                c.flag |= crate::chan::flag::CCEXEC;
            }
            c.flag |= crate::chan::flag::COPEN;
        }
    }
    Ok(c)
}

/// `Enocreate` (`port/error.h:8`).
const ENOCREATE: &str = "mounted directory forbids creation";

/// `namec(..., Acreate, ...)`: walk the parent, then create in the union's
/// **create element** — the one bound with `MCREATE`. Plan 9 resolves the
/// last element's parent and creates there (`chan.c`, `namec`'s `Acreate`
/// case), which is why a create can land in a different file server from the
/// one a read of the same directory would answer.
pub fn create(
    tab: &mut Devtab,
    ns: &Ns,
    slash: &Chan,
    dot: &Chan,
    name: &str,
    omode: u16,
    perm: u32,
) -> Result<Chan, String> {
    // `Acreate`'s own checks (`chan.c`), before anything is walked:
    // a name ending in `/` or `/.` must be created with `DMDIR`, and creating
    // the root itself is `Eexist`.
    let mustbedir = name.ends_with('/') || name.ends_with("/.");
    if mustbedir && perm & crate::ninep::DMDIR == 0 {
        return Err("create without DMDIR".into());
    }
    let name = name.trim_end_matches('.').trim_end_matches('/');
    if name.is_empty() || name == "#" {
        return Err("file already exists".into());
    }
    let (dir, last) = match name.rfind('/') {
        Some(i) => (&name[..i.max(1)], &name[i + 1..]),
        None => (".", name),
    };
    if last.is_empty() || last == "." || last == ".." {
        return Err("bad create name".into());
    }
    let parent = namec(tab, ns, slash, dot, dir, A::Todir, 0)?;

    // **`create(2)` of a name that already exists is an OPEN with `OTRUNC`**
    // (`chan.c:1540`): namec walks the last element first, and if it is
    // there, opens it truncated — unless `OEXCL`, which is the only way to
    // reach `create(5)`'s own semantics, where an existing name fails.
    //
    // Plan 9's comment names the very case that found this: *"The
    // create/create race is quite common. For example, it happens when two rc
    // subshells simultaneously update the same environment variable."* Here
    // it was not even a race — one rc, writing `/env/status` twice, told it
    // could not create what it had just created.
    if omode & crate::chan::mode::OEXCL == 0 {
        match walk(tab, ns, parent.clone(), &[last.to_string()], false) {
            Ok(existing) => {
                return tab.dopen(existing, (omode & !crate::chan::mode::OEXCL) | crate::chan::mode::OTRUNC);
            }
            Err(e) if e == crate::devmnt::SLEPT => return Err(e),
            Err(_) => {}
        }
    }

    // **A directory mounted upon is created in through `createdir`**
    // (`chan.c:1590`): the element bound with `MCREATE`, and if there is
    // none, *"mounted directory forbids creation"* (`chan.c:1159`,
    // `Enocreate`). Only a directory nothing is mounted on is created in
    // itself.
    let target = match ns.findmount(&parent) {
        Some(els) => match els.iter().find(|e| e.create()) {
            Some(e) => e.chan.clone(),
            None => return Err(ENOCREATE.into()),
        },
        None => parent.clone(),
    };
    // **`cnew = cunique(cnew)`** (`chan.c:1606`): *"We need our own copy of
    // the Chan because we're about to send a create, which will move it."*
    // Without it a create in `.` moved the current directory's own fid onto
    // the new file — every relative name after `cd /tmp; echo a >x` answered
    // "unknown fid" — and a create in a union moved the mount's channel.
    let mut target = tab.dcclone(&target)?;
    target.path = parent.path.clone();
    tab.dcreate(&mut target, last, omode & !crate::chan::mode::OEXCL, perm)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devroot::Root;
    use crate::ns::{Bind, Element};

    fn tab_with_root() -> (Devtab, Chan) {
        let mut tab = Devtab::new();
        let mut r = Root::new();
        r.addbootfile("init", b"the image".to_vec());
        let slash = r.attach("").unwrap();
        tab.add(Box::new(r));
        (tab, slash)
    }

    #[test]
    fn a_rooted_name_resolves_from_the_processs_root_channel() {
        let (mut tab, slash) = tab_with_root();
        let ns = Ns::new();
        let c = namec(&mut tab, &ns, &slash, &slash, "/boot/init", A::Access, 0).unwrap();
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
        let mut elsewhere = slash.clone();
        elsewhere.qid.path = 999;
        ns.mount(&slash, Element::new(elsewhere), Bind::Replace);

        let c = namec(&mut tab, &ns, &slash, &slash, "#/", A::Access, 0).unwrap();
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
        let slash = r.attach("").unwrap();
        tab.add(Box::new(r));

        let mut other = Other::new();
        let over = other.attach("").unwrap();
        tab.add(Box::new(other));

        let mut ns = Ns::new();
        ns.mount(&slash, Element::new(over), Bind::Replace);

        let c = namec(&mut tab, &ns, &slash, &slash, "/boot/init", A::Access, 0).expect("resolve");
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
        let slash = r.attach("").unwrap();
        tab.add(Box::new(r));

        // the channel for /init — reached by a walk, not the starting point
        let on = namec(&mut tab, &Ns::new(), &slash, &slash, "/boot/init", A::Access, 0).unwrap();
        assert_eq!(on.dev, DevId::Root);

        let mut other = Other::new();
        let over = other.attach("").unwrap();
        tab.add(Box::new(other));

        let mut ns = Ns::new();
        ns.mount(&on, Element::new(over), Bind::Replace);

        let c = namec(&mut tab, &ns, &slash, &slash, "/boot/init", A::Access, 0).unwrap();
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
        let slash = r.attach("").unwrap();
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
        let slash = r.attach("").unwrap();
        tab.add(Box::new(r));
        let ns = Ns::new();

        let rooted = namec(&mut tab, &ns, &slash, &slash, "/boot/init", A::Access, 0).unwrap();
        let relative = namec(&mut tab, &ns, &slash, &slash, "boot/init", A::Access, 0)
            .expect("a relative name must resolve from dot");
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
        ns.mount(&slash, Element::new(slash.clone()), Bind::After);

        let c = namec(&mut tab, &ns, &slash, &slash, "/boot/init", A::Access, 0)
            .expect("the second element has /boot/init");
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
        assert_eq!(e, ENOCREATE);
    }

    #[test]
    fn opening_for_writing_is_refused_by_the_root() {
        let (mut tab, slash) = tab_with_root();
        let ns = Ns::new();
        let e = namec(&mut tab, &ns, &slash, &slash, "/boot/init", A::Open,
                      crate::chan::mode::OWRITE);
        assert!(e.is_err(), "devroot refuses writing, as rootwrite does");
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
    /// One RPC: the tag it went with, whether it has gone, and its reply
    /// once it came.
    Rpc { tag: u16, sent: bool, reply: Option<Vec<u8>> },
}

/// A wire's reader and its outstanding RPCs — `m->rip`, `m->queue` and the
/// replies `mountmux` has taken for others (`devmnt.c:930`).
#[derive(Default)]
struct Mux {
    /// `m->rip` — the one process reading the wire.
    rip: Option<crate::proc::Pid>,
    /// `m->queue` — each outstanding RPC by its tag, and who waits for it.
    /// A clunk waits for nobody.
    queue: Vec<(u16, Option<crate::proc::Pid>)>,
    /// Replies read by another process, waiting for their owner.
    done: HashMap<u16, Vec<u8>>,
    /// A message read in part — `m->q`.
    inbuf: Vec<u8>,
    /// The next tag to try.
    tag: u16,
}

impl Devtab {
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
        let clunk = request.get(4) == Some(&(crate::ninep::T::Clunk as u8));

        // The record: a reply had already, or an RPC under way.
        let rec = self.tab.records.entry(pid).or_default();
        let at = rec.at;
        let (tag, sent) = match rec.steps.get(at) {
            Some(Step::Rpc { reply: Some(r), .. }) => {
                let r = r.clone();
                rec.at += 1;
                return Ok(r);
            }
            Some(Step::Rpc { tag, sent, reply: None }) => (*tag, *sent),
            _ => {
                rec.steps.truncate(at);
                // `mntralloc`'s tag: unique among the wire's outstanding
                // RPCs, whichever mount of the wire made them. `Tversion`
                // keeps its NOTAG.
                let asked = u16::from_le_bytes([request.get(5).copied().unwrap_or(0), request.get(6).copied().unwrap_or(0)]);
                let tag = if asked == !0 {
                    asked
                } else {
                    let m = self.tab.muxes.entry(key).or_default();
                    loop {
                        m.tag = m.tag.wrapping_add(1);
                        if m.tag != !0 && !m.queue.iter().any(|e| e.0 == m.tag) {
                            break m.tag;
                        }
                    }
                };
                let rec = self.tab.records.entry(pid).or_default();
                rec.steps.push(Step::Rpc { tag, sent: false, reply: None });
                (tag, false)
            }
        };
        let finish = |tab: &mut Devtab, reply: Vec<u8>| {
            let rec = tab.records.entry(pid).or_default();
            rec.steps[at] = Step::Rpc { tag, sent: true, reply: Some(reply.clone()) };
            rec.at = at + 1;
            reply
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
            m.queue.push((tag, if clunk { None } else { Some(pid) }));
            if let Some(Step::Rpc { sent, .. }) = self.tab.records.entry(pid).or_default().steps.get_mut(at) {
                *sent = true;
            }
            // **A clunk waits for nobody.** Plan 9's `mntclunk` waits for
            // `Rclunk`; a clunk here is made where a call cannot be run
            // again — a channel's last close — so its reply is left for
            // whoever reads the wire next, and dropped (RESEARCH §16.14).
            if clunk {
                return Ok(finish(self.tab, crate::ninep::W::new().frame(crate::ninep::T::Clunk.reply(), tag)));
            }
        }

        loop {
            // `mountmux` gave it to us while we slept.
            if let Some(r) = self.tab.muxes.entry(key).or_default().done.remove(&tag) {
                return Ok(finish(self.tab, r));
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
                        self.drop_rpc(key, tag);
                        return Err(crate::proc::EINTR.into());
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
                    self.drop_rpc(key, tag);
                    return Err("short reply".into());
                }
                if have >= size && have >= 4 {
                    break m.inbuf.drain(..size).collect::<Vec<u8>>();
                }
                let d = self.tab.get(self.wire.dev).ok_or("no such device")?;
                let got = match d.read(&mut self.wire, size - have, 0) {
                    Ok(b) => b,
                    Err(e) => {
                        self.drop_rpc(key, tag);
                        return Err(e);
                    }
                };
                if self.tab.asleep(pid) {
                    return Err(SLEPT.into());
                }
                if got.is_empty() {
                    // `Emountrpc` (`devmnt.c:822`): the wire is hung up.
                    self.drop_rpc(key, tag);
                    return Err("mount rpc error".into());
                }
                self.tab.muxes.entry(key).or_default().inbuf.extend_from_slice(&got);
            };
            // `mountmux` (`devmnt.c:930`): the reply goes to its RPC.
            let rtag = u16::from_le_bytes([msg[5], msg[6]]);
            let m = self.tab.muxes.entry(key).or_default();
            let owner = m.queue.iter().position(|e| e.0 == rtag).map(|i| m.queue.remove(i));
            if rtag == tag {
                self.tab.gate(key);
                return Ok(finish(self.tab, msg));
            }
            // Someone else's, or nobody's — a clunk's, or one whose
            // process is gone, which Plan 9 prints as *"unexpected reply
            // tag"* and drops.
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
    /// An RPC that will not be waited for any more: out of the queue, and
    /// the wire's reader leaves if it was us (`mntflushfree`, `mntgate`).
    fn drop_rpc(&mut self, key: (DevId, u32, u64), tag: u16) {
        let pid = self.tab.uppid();
        if let Some(m) = self.tab.muxes.get_mut(&key) {
            m.queue.retain(|e| e.0 != tag);
            if m.rip == Some(pid) {
                self.tab.gate(key);
            }
        }
    }
}

/// `Enoattach` (`port/error.h`).
const ENOATTACH: &str = "not attached";
