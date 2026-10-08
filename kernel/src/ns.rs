//! The namespace — what a process's names mean.
//!
//! **A mount point is a FILE, not a path.** Plan 9 keys a mount by the identity
//! of the channel mounted upon — `findmount(Chan**, Mhead**, int type, int dev,
//! Qid qid)` in `plan9/sys/src/9/port/chan.c:855` — and `Mhead.from` is the
//! channel mounted upon while `Mount.to` is the channel replacing it. A walk
//! checks for a mount **at every component**, not once against a prefix.
//!
//! That is not a detail of implementation. Keyed by file, a bind is visible
//! through every path that reaches the file; keyed by text, it is visible only
//! through the spelling used to make it. The first is Plan 9's namespace; the
//! second is a prefix-rewriting table that resembles one until two names reach
//! the same directory.
//!
//! Resolution itself is not here, because it cannot be: walking a name means
//! asking a device, so it belongs where the device table is. This holds the
//! mount table and answers one question — *is anything mounted on this
//! channel?*

use crate::chan::Chan;
use std::cell::RefCell;
use std::rc::Rc;

/// `bind(2)`'s flags: where in the union the new element goes.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Bind {
    /// `MREPL` — replace whatever is there.
    Replace,
    /// `MBEFORE` — this one answers first.
    Before,
    /// `MAFTER` — the existing elements answer first.
    After,
}

/// The mount flags (`libc.h:556`). Kept whole on the element, because
/// `#p/<n>/ns` prints them back with `int2flag` and a namespace that cannot
/// say how it was built cannot be rebuilt from its own dump.
pub mod mflag {
    /// `MREPL` — *"mount replaces object"*. **Zero**, which is why
    /// `int2flag` gives it the empty string and not `-b`.
    pub const MREPL: i32 = 0x0000;
    /// `MBEFORE` — *"mount goes before others in union directory"*.
    pub const MBEFORE: i32 = 0x0001;
    /// `MAFTER` — *"mount goes after others in union directory"*.
    pub const MAFTER: i32 = 0x0002;
    /// `MCREATE` — *"permit creation in mounted directory"*.
    pub const MCREATE: i32 = 0x0004;
    /// `MCACHE` — *"cache some data"*. Nothing here caches, but the bit is
    /// carried so a dump of the namespace is faithful.
    pub const MCACHE: i32 = 0x0010;
    /// `MMASK` — *"all bits on"*.
    pub const MMASK: i32 = 0x0017;
}

/// What a mount point resolves to: Plan 9's `Mount` (`portdat.h:295`).
///
/// Plan 9 keeps `Chan *to`, `int mflag` and `char *spec`. It kept the flag
/// **word**, not a decoded bit, and `#p/<n>/ns` is why: `int2flag`
/// (`devproc.c`) turns it back into `-a`, `-bc`, `-aC` or the empty string
/// for `MREPL`. This carried a `create: bool` instead, so the dump had to
/// guess the flag from an element's position in the list — which cannot tell
/// `MREPL` from `MBEFORE`, and loses `-ac` entirely.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    /// `Mount.to` — the channel replacing the channel mounted upon. **A
    /// reference**, as `newmount`'s *"incref(to)"* (`pgrp.c:273`) makes it:
    /// a copied namespace shares it, and its last reference's close is the
    /// device's.
    pub chan: Rc<Chan>,
    /// `Mount.mflag` — the flag word as given.
    pub mflag: i32,
    /// `Mount.spec` — `mount`'s aname. Empty for a bind, and for a mount
    /// that gave none.
    pub spec: String,
    /// `Mount.mountid` — *"m->mountid = incref(&mountid)"* (`pgrp.c:274`):
    /// the order mounts were made in, which `#p/<n>/ns` lists them by.
    pub mountid: u32,
}

/// `Emount`, `Eunmount` and `Eunion` (`error.h:2`, `:3`, `:5`).
const EMOUNT: &str = "inconsistent mount";
const EUNMOUNT: &str = "not mounted";
const EUNION: &str = "not in union";

/// `static Ref mountid` (`pgrp.c:13`).
static MOUNTID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

fn newmountid() -> u32 {
    MOUNTID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// `MOUNTH(p, qid)` (`portdat.h:481`) — which chain.
fn mounth(c: &Chan) -> usize {
    (c.qid.path & (MNTHASH as u64 - 1)) as usize
}

impl Element {
    pub fn new(chan: Chan) -> Self {
        Element::shared(Rc::new(chan), mflag::MREPL, "")
    }

    /// The element as `bind`/`mount` made it: the channel, the flag word and
    /// the spec.
    pub fn with(chan: Chan, flag: i32, spec: &str) -> Self {
        Element::shared(Rc::new(chan), flag, spec)
    }

    /// The same, holding a reference to a channel that already has one.
    /// Its `mountid` is `cmount`'s to give, as `newmount` gives it.
    pub fn shared(chan: Rc<Chan>, flag: i32, spec: &str) -> Self {
        Element { chan, mflag: flag & mflag::MMASK, spec: spec.to_string(), mountid: 0 }
    }

    pub fn creatable(chan: Chan) -> Self {
        Element::with(chan, mflag::MCREATE, "")
    }

    /// `MCREATE` — a create in this directory lands here. The first element
    /// carrying it wins; a union with none refuses creates, which is how a
    /// read-only union is expressed without a read-only flag.
    pub fn create(&self) -> bool {
        self.mflag & mflag::MCREATE != 0
    }
}

/// `eqchan(a, b, 1)` (`chan.c:606`) — through `eqchantdqid` (`:620`),
/// which `findmount` calls the same way (`:869`): the same device, instance,
/// qid path and qid type, **and not the qid's version**, the `1` skipping
/// it. A directory whose contents change is a new version of itself, and the
/// host's server says so (*"st_mtime ^ (st_size << 8)"*); compared by the
/// whole qid, a bind onto it vanished the first time anything was put in it.
pub fn eqchan(a: &Chan, b: &Chan) -> bool {
    a.qid.path == b.qid.path && a.qid.qtype == b.qid.qtype && a.dev == b.dev && a.devno == b.devno
}

/// `Mhead` (`portdat.h:307`) — one mount POINT: *"`Chan* from;` — channel
/// mounted upon"*, and the list of what is mounted on it.
///
/// `from` is not decoration. `#p/<n>/ns` prints `mh->from->path->s` as the
/// second operand of every `bind` line it writes (`devproc.c`), so a
/// namespace that does not keep the channel it was mounted upon cannot say
/// what it is.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Mhead {
    /// A reference, as `newmhead`'s *"incref(from)"* makes it.
    pub from: Option<Rc<Chan>>,
    pub mount: Vec<Element>,
}

/// A mount head by reference — Plan 9's `Mhead*`, counted (`Mhead.ref`):
/// the namespace holds one, and so does a union directory's open channel
/// (`Chan.umh`), until `putmhead`. **What it lists is the namespace's**: an
/// unmount or a closed namespace empties it, and an open union reads what is
/// there when it reads (`unionread`, `sysfile.c:323`), not what was there
/// when it was opened.
pub type Head = Rc<RefCell<Mhead>>;

/// `MNTLOG` and `MNTHASH` (`portdat.h:474`).
const MNTLOG: u32 = 5;
const MNTHASH: usize = 1 << MNTLOG;

/// A process's namespace: Plan 9's `Pgrp`, which is a table of `Mhead`.
#[derive(Debug)]
pub struct Ns {
    /// `Pgrp.mnthash` (`portdat.h:490`): `MNTHASH` chains, a head on the one
    /// `MOUNTH` picks — *"(p)->mnthash[(qid).path&((1<<MNTLOG)-1)]"*
    /// (`:481`) — at the end of it, in the order heads are made (`cmount`,
    /// `chan.c:706`).
    mnthash: Vec<Vec<Head>>,
    /// `Pgrp.pgrpid` (`portdat.h`, `struct Pgrp`). **A namespace group is what
    /// Plan 9 calls a process group** — `Pgrp` holds `mnthash[]`, the mount
    /// table, and nothing about signals or job control. `newpgrp` numbers each
    /// one from a global counter (`pgrp.c:53`, `incref(&pgrpid)`), which is
    /// what `/dev/pgrpid` reports.
    id: u32,
    /// `Pgrp.noattach` (`portdat.h`, `struct Pgrp`).
    noattach: bool,
}

/// The counter `newpgrp` draws from (`pgrp.c:12`, `static Ref pgrpid`).
static NEXT_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

/// **A copied namespace is a NEW group.** `sysrfork` calls `newpgrp()` and
/// then `pgrpcpy` (`sysproc.c:140`), so the mounts come across and the id does
/// not. Only sharing keeps the id, which is the point of the number.
///
/// `pgrpcpy` (`pgrp.c:128`): a new head for every head and a new mount for
/// every mount, sharing the channels — `newmhead`'s and `newmount`'s
/// *"incref"* — and each numbered twice: by `newmount` as it is made, then
/// *"Allocate mount ids in the same sequence as the parent group"* (`:156`).
impl Clone for Ns {
    fn clone(&self) -> Ns {
        let mut order = Vec::new();
        let mnthash: Vec<Vec<Head>> = self
            .mnthash
            .iter()
            .map(|chain| {
                chain
                    .iter()
                    .map(|f| {
                        let mut mh = f.borrow().clone();
                        for (i, m) in mh.mount.iter_mut().enumerate() {
                            order.push((m.mountid, i));
                            m.mountid = newmountid();
                        }
                        Rc::new(RefCell::new(mh))
                    })
                    .collect()
            })
            .collect();
        // `pgrpinsert` sorts by the parent's id; the copy is found again by
        // where it was made
        let mut copies: Vec<(u32, Head, usize)> = Vec::new();
        let mut k = 0;
        for chain in &mnthash {
            for h in chain {
                for i in 0..h.borrow().mount.len() {
                    copies.push((order[k].0, h.clone(), i));
                    debug_assert_eq!(order[k].1, i);
                    k += 1;
                }
            }
        }
        copies.sort_by_key(|c| c.0);
        for (_, h, i) in copies {
            h.borrow_mut().mount[i].mountid = newmountid();
        }
        Ns { mnthash, id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed), noattach: self.noattach }
    }
}

/// A cleared namespace is `newpgrp()` with no `pgrpcpy` — also a new group.
impl Default for Ns {
    fn default() -> Ns {
        Ns::new()
    }
}

impl Ns {
    pub fn new() -> Self {
        Ns {
            mnthash: vec![Vec::new(); MNTHASH],
            id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            noattach: false,
        }
    }

    /// The namespace as the lines that would rebuild it, which is what
    /// `/proc/n/ns` prints (`devproc.c:952`): one per mount element, with the
    /// flag `bind` would have been given.

    /// `Pgrp.noattach` — `RFNOMNT`'s sandbox. Set, never cleared.
    pub fn noattach(&self) -> bool {
        self.noattach
    }

    pub fn set_noattach(&mut self, on: bool) {
        self.noattach |= on;
    }

    /// `pgrpid` — this namespace group's number.
    pub fn id(&self) -> u32 {
        self.id
    }

    /// [`Ns::cmount`] of a channel nothing else holds, for its id alone.
    pub fn mount(&mut self, on: &Chan, to: Element, how: Bind) -> u32 {
        let union = to.chan.umh.as_ref().map(|h| h.borrow().mount.clone()).unwrap_or_default();
        self.cmount(Rc::new(on.clone()), to, union, how).map_or(0, |r| r.0)
    }

    /// `cmount` (`chan.c:646`), `bind(2)` and `mount(2)`'s half: put `to`
    /// over the file `on`, each held by reference. Answers the new mount's
    /// id — *"return nm->mountid"* (`chan.c:760`) — and the channels it let
    /// go of, for the caller to close: an `MREPL` mount frees the list it
    /// replaces (*"mountfree(m->mount)"*, `:739`).
    ///
    /// `union` is the mount head the new channel was reached through
    /// (`new->umh`), whose other elements come along. Refused, as
    /// `Emount`: a directory over a file or a file over a directory
    /// (`:654`), a union onto a file (`:662`), and a `-c` bind of a union or
    /// of a mount that is not itself creatable (`:686`).
    pub fn cmount(&mut self, on: Rc<Chan>, to: Element, union: Vec<Element>, how: Bind) -> Result<(u32, Vec<Rc<Chan>>), String> {
        if (on.qid.qtype ^ to.chan.qid.qtype) & crate::ninep::QTDIR != 0 {
            return Err(EMOUNT.into());
        }
        if !on.is_dir() && how != Bind::Replace {
            return Err(EMOUNT.into());
        }
        if to.create() && !union.is_empty() && (union.len() > 1 || !union[0].create()) {
            return Err(EMOUNT.into());
        }
        // **`cmount` (`chan.c:708`), and the comment there is the whole of
        // it:** *"if this is a union mount, add the old node to the mount
        // chain."* Nothing was mounted here before, so the directory itself
        // is what the union's first element must be — otherwise `bind -a x /`
        // does not ADD to `/`, it replaces it, and every name that was there
        // is gone. `bind -a /root /` is the line that found this: it is how a
        // root becomes a file server, and it took `/dev` and `/env` with it.
        //
        // It is added with flags 0 (`newmount(m, old, 0, 0)`), so it never
        // carries `MCREATE`. The head holds the channel mounted upon
        // (`newmhead`'s *"incref(from)"*); one that exists keeps its own.
        let head = match self.lookup(&on) {
            Some(h) => h,
            None => {
                let mut mh = Mhead { from: Some(on.clone()), mount: Vec::new() };
                if how != Bind::Replace {
                    let mut old = Element::shared(on.clone(), 0, "");
                    old.mountid = newmountid();
                    mh.mount.push(old);
                }
                let h = Rc::new(RefCell::new(mh));
                let b = mounth(&on);
                self.mnthash[b].push(h.clone());
                h
            }
        };
        let mut gone = Vec::new();
        let mut m = head.borrow_mut();
        let list = &mut m.mount;
        // **"copy a union when binding it onto a directory"** (`chan.c:725`).
        // The source channel landed on one element of a union and `Abind`
        // kept the rest; all of them come along, or `bind -a /root /` binds
        // whichever element happened to answer first and the others become
        // unreachable. `/root` is a union — `/lib/namespace` mounts the
        // server onto it with `-a` — so that is every file on the server.
        //
        // They follow the new element, and `MREPL` becomes `MAFTER` for
        // them: `flg = order; if(order == MREPL) flg = MAFTER;`
        // The union's FIRST element is the channel itself — `domount` landed
        // on it — so the copy starts at the second: `for(um = um->next; um;
        // um = um->next)` (`chan.c:732`).
        //
        // **The copies carry the ORDER's flag and the original's spec** —
        // `newmount(m, um->to, flg, um->spec)` (`chan.c:733`), where `flg =
        // order` with `MREPL` becoming `MAFTER`. Not the original's flag:
        // the new binding says where these go.
        let flg = match how {
            Bind::Replace | Bind::After => mflag::MAFTER,
            Bind::Before => mflag::MBEFORE,
        };
        let mut group = vec![to];
        for extra in union.into_iter().skip(1) {
            group.push(Element::shared(extra.chan, flg, &extra.spec));
        }
        // `newmount` numbers them as `cmount` makes them: the old node,
        // then the new one, then the union it brought (`chan.c:713`, `:722`,
        // `:733`).
        for e in group.iter_mut() {
            e.mountid = newmountid();
        }
        let id = group[0].mountid;
        // *"if(m->mount && order == MREPL){ mountfree(m->mount); …"*
        // (`chan.c:739`): what is replaced is let go of.
        match how {
            Bind::Replace => gone.extend(std::mem::replace(list, group).into_iter().map(|e| e.chan)),
            Bind::Before => {
                for (i, e) in group.into_iter().enumerate() {
                    list.insert(i, e);
                }
            }
            Bind::After => list.extend(group),
        }
        Ok((id, gone))
    }

    /// The head on this file, if there is one: `MOUNTH`'s chain, searched with
    /// *"eqchan(m->from, old, 1)"* (`chan.c:695`).
    fn lookup(&self, on: &Chan) -> Option<Head> {
        self.mnthash[mounth(on)]
            .iter()
            .find(|h| h.borrow().from.as_deref().is_some_and(|f| eqchan(f, on)))
            .cloned()
    }

    /// `findmount` (`chan.c:855`): is anything mounted on this file? Answered
    /// by the file's identity, so every path that reaches it sees the same
    /// answer — and answered with the head itself, which `domount` hands on
    /// (*"incref(m); … *mp = m"*, `:872`).
    pub fn findmount(&self, on: &Chan) -> Option<Head> {
        self.lookup(on).filter(|h| !h.borrow().mount.is_empty())
    }

    /// What is mounted on this file, in order.
    pub fn elements(&self, on: &Chan) -> Option<Vec<Element>> {
        self.findmount(on).map(|h| h.borrow().mount.clone())
    }

    /// Every mount point, for `#p/<n>/ns` to print: the heads as they stand,
    /// chain by chain (`mntscan`, `devproc.c:1000`, which looks for the next
    /// `mountid` through all of them).
    pub fn heads(&self) -> Vec<Mhead> {
        self.mnthash.iter().flatten().map(|h| h.borrow().clone()).collect()
    }

    /// [`Ns::cunmount`] of a channel by its identity alone.
    pub fn unmount(&mut self, on: &Chan, what: Option<&Chan>) {
        let _ = self.cunmount(on, what.map(|w| move |c: &Chan| eqchan(c, w)));
    }

    /// `cunmount` (`chan.c:764`), `unmount(2)`'s half. Without a channel,
    /// the mount point is cleared: its list freed and the channel mounted
    /// upon closed. With one, the first element that IS it — `matches` is
    /// `eqchan(f->to, mounted, 1)`, or the same of the wire a mount's
    /// channel is on (`f->to->mchan`) — goes, and the head with it if it was
    /// the last. Answers what was let go of, for the caller to close.
    /// Nothing mounted there is `Eunmount`; no such element, `Eunion`.
    pub fn cunmount(&mut self, on: &Chan, matches: Option<impl Fn(&Chan) -> bool>) -> Result<Vec<Rc<Chan>>, String> {
        let Some(head) = self.lookup(on) else { return Err(EUNMOUNT.into()) };
        let mut m = head.borrow_mut();
        // *"mountfree(m->mount); m->mount = nil; cclose(m->from)"*
        // (`chan.c:801`) — the whole head, unlinked from its chain.
        let Some(matches) = matches else {
            let mut gone: Vec<Rc<Chan>> = std::mem::take(&mut m.mount).into_iter().map(|e| e.chan).collect();
            gone.extend(m.from.take());
            drop(m);
            self.unlink(&head);
            return Ok(gone);
        };
        let i = m.mount.iter().position(|e| matches(&e.chan)).ok_or(EUNION)?;
        let mut gone = vec![m.mount.remove(i).chan];
        // *"if(m->mount == nil){ *l = m->hash; cclose(m->from); …"* (`:817`)
        if m.mount.is_empty() {
            gone.extend(m.from.take());
            drop(m);
            self.unlink(&head);
        }
        Ok(gone)
    }

    /// *"*l = m->hash"* — the head off its chain.
    fn unlink(&mut self, head: &Head) {
        for chain in self.mnthash.iter_mut() {
            chain.retain(|h| !Rc::ptr_eq(h, head));
        }
    }

    /// `closepgrp` (`pgrp.c:75`): the last reference to the namespace is
    /// going. Chain by chain, *"cclose(f->from); mountfree(f->mount);
    /// f->mount = nil;"* (`:90`) — every channel it holds, for the caller to
    /// close, and every head emptied, for a union directory still open on one.
    pub fn closepgrp(&mut self) -> Vec<Rc<Chan>> {
        let mut out = Vec::new();
        for chain in self.mnthash.iter_mut() {
            for h in chain.drain(..) {
                let mut f = h.borrow_mut();
                out.extend(f.from.take());
                out.extend(std::mem::take(&mut f.mount).into_iter().map(|e| e.chan));
            }
        }
        out
    }

    /// Where a create in this directory lands: the first element that accepts
    /// one. `None` means creates are refused here.
    pub fn create_element(&self, on: &Chan) -> Option<Element> {
        self.elements(on)?.into_iter().find(|e| e.create())
    }

    pub fn is_empty(&self) -> bool {
        self.mnthash.iter().all(|c| c.is_empty())
    }
}

/// `cleanname(3)`'s rule: collapse repeated separators, resolve `.` and `..`,
/// drop a trailing separator. `..` at the root stays at the root.
///
/// This is textual and it is only ever used on a name BEFORE it is walked —
/// it is not how a mount point is found.
pub fn clean(path: &str) -> String {
    let rooted = path.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if !out.is_empty() && *out.last().unwrap() != ".." {
                    out.pop();
                } else if !rooted {
                    out.push("..");
                }
            }
            p => out.push(p),
        }
    }
    let joined = out.join("/");
    if rooted {
        format!("/{}", joined)
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dev::DevId;
    use crate::ninep::Qid;

    fn file(qid: u64) -> Chan {
        let mut c = Chan::attach(DevId::Root, 0);
        c.qid = Qid { qtype: crate::ninep::QTDIR, vers: 0, path: qid };
        c
    }

    #[test]
    fn a_mount_is_found_by_the_files_identity_not_its_name() {
        // The whole point. The same file reached by two different names must
        // see the same mount — which is true here because the name is not
        // part of the key.
        let mut ns = Ns::new();
        let mut by_one_name = file(7);
        by_one_name.path = "/n/z".into();
        let mut by_another = file(7);
        by_another.path = "/somewhere/else".into();

        ns.mount(&by_one_name, Element::new(file(99)), Bind::Replace);
        assert!(ns.findmount(&by_another).is_some(), "a bind follows the file, not the spelling");
    }

    #[test]
    fn a_different_file_is_not_mounted_upon() {
        let mut ns = Ns::new();
        ns.mount(&file(7), Element::new(file(99)), Bind::Replace);
        assert!(ns.findmount(&file(8)).is_none());
    }

    #[test]
    fn the_union_answers_in_bind_order() {
        let mut ns = Ns::new();
        let on = file(1);
        ns.mount(&on, Element::new(file(10)), Bind::Replace);
        ns.mount(&on, Element::new(file(20)), Bind::After);
        ns.mount(&on, Element::new(file(30)), Bind::Before);
        let order: Vec<u64> =
            ns.elements(&on).unwrap().iter().map(|e| e.chan.qid.path).collect();
        assert_eq!(order, vec![30, 10, 20]);
    }

    #[test]
    fn a_create_lands_in_the_create_element_and_nowhere_else() {
        let mut ns = Ns::new();
        let on = file(1);
        ns.mount(&on, Element::new(file(10)), Bind::Replace);
        ns.mount(&on, Element::creatable(file(20)), Bind::Before);
        assert_eq!(ns.create_element(&on).unwrap().chan.qid.path, 20);
    }

    #[test]
    fn a_union_with_no_create_element_refuses_creates() {
        let mut ns = Ns::new();
        let on = file(1);
        ns.mount(&on, Element::new(file(10)), Bind::Replace);
        assert!(ns.create_element(&on).is_none());
    }

    #[test]
    fn unmount_removes_one_element_and_the_rest_stand() {
        let mut ns = Ns::new();
        let on = file(1);
        ns.mount(&on, Element::new(file(10)), Bind::Replace);
        ns.mount(&on, Element::new(file(20)), Bind::After);
        ns.unmount(&on, Some(&file(10)));
        assert_eq!(ns.elements(&on).unwrap().len(), 1);
    }

    #[test]
    fn a_namespace_is_copied_not_shared_when_it_is_copied() {
        let mut parent = Ns::new();
        let on = file(1);
        parent.mount(&on, Element::new(file(10)), Bind::Replace);
        let mut child = parent.clone();
        child.mount(&on, Element::new(file(20)), Bind::Before);
        assert_eq!(parent.elements(&on).unwrap().len(), 1);
        assert_eq!(child.elements(&on).unwrap().len(), 2);
    }

    #[test]
    fn cleanname_resolves_dot_and_dotdot() {
        assert_eq!(clean("/n/z/../store"), "/n/store");
        assert_eq!(clean("/n//z/"), "/n/z");
        assert_eq!(clean("/../.."), "/");
    }
}
