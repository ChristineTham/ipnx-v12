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
}

impl Devtab {
    pub fn new() -> Devtab {
        Devtab::default()
    }
    pub fn add(&mut self, d: Box<dyn Dev>) {
        self.devs.insert(d.id(), d);
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

    /// Attach a 9P server over a channel — `mount(2)`'s device half.
    pub fn dmount(&mut self, wire: Chan, uname: &str, aname: &str) -> Result<Chan, String> {
        let (uname, aname) = (uname.to_string(), aname.to_string());
        self.with_mnt(|m, tab| {
            let mut w = Wire { wire: wire.clone(), tab };
            m.mount(wire.clone(), &mut w, &uname, &aname)
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
fn domount(tab: &mut Devtab, ns: &Ns, c: Chan) -> Result<(Chan, Vec<Chan>), String> {
    let els: Vec<Chan> = match ns.findmount(&c) {
        Some(els) if !els.is_empty() => els.iter().map(|e| e.chan.clone()).collect(),
        _ => return Ok((c, Vec::new())),
    };
    let mut m = tab.dcclone(&els[0])?;
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
        match tab.dwalk(&c, name)? {
            Some(next) => c = next,
            None => {
                // **"try a union mount, if any"** (`chan.c:1027`). The first
                // element is the one just walked, so this starts at the next
                // — `for(f = (f? f->next: f); f; f = f->next)` (`:1034`).
                // Without it a `bind -a` puts an element in a list nothing
                // ever reaches, and a union is a word rather than a thing.
                let mut found = None;
                for alt in union.into_iter().skip(1) {
                    if let Ok(Some(next)) = tab.dwalk(&alt, name) {
                        found = Some(next);
                        break;
                    }
                }
                match found {
                    Some(next) => c = next,
                    None => return Err(format!("'{}' does not exist", name)),
                }
            }
        }
    }
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
        let (first, rest) = domount(tab, ns, c)?;
        c = first;
        // The union comes along for two of the modes, and for the reasons
        // [`Chan::umh`] records. **Only when it has more than one element**
        // (`chan.c:1502`), because one element is not a union.
        if rest.len() > 1 && matches!(amode, A::Bind | A::Open) {
            c.umh = rest;
        }
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
        if let Ok(existing) = walk(tab, ns, parent.clone(), &[last.to_string()], false) {
            return tab.dopen(existing, (omode & !crate::chan::mode::OEXCL) | crate::chan::mode::OTRUNC);
        }
    }

    // The create lands in the create element if this directory is a union.
    let mut target = match ns.create_element(&parent) {
        Some(e) => e.chan.clone(),
        None => parent,
    };
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

impl crate::devmnt::Transport for Wire<'_> {
    fn rpc(&mut self, request: &[u8]) -> Result<Vec<u8>, String> {
        let d = self.tab.get(self.wire.dev).ok_or("no such device")?;
        d.write(&mut self.wire, request, 0)?;
        // A reply is framed, so its first four bytes say how long it is.
        let d = self.tab.get(self.wire.dev).ok_or("no such device")?;
        let mut out = d.read(&mut self.wire, 4, 0)?;
        if out.len() < 4 {
            return Err("short reply".into());
        }
        let size = u32::from_le_bytes([out[0], out[1], out[2], out[3]]) as usize;
        while out.len() < size {
            let d = self.tab.get(self.wire.dev).ok_or("no such device")?;
            let more = d.read(&mut self.wire, size - out.len(), 0)?;
            if more.is_empty() {
                return Err("truncated reply".into());
            }
            out.extend_from_slice(&more);
        }
        Ok(out)
    }
}

/// `Enoattach` (`port/error.h`).
const ENOATTACH: &str = "not attached";
