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
use crate::dev::DevId;
use crate::ninep::Qid;
use std::collections::HashMap;

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
    /// `Mount.to` — the channel replacing the channel mounted upon.
    pub chan: Chan,
    /// `Mount.mflag` — the flag word as given.
    pub mflag: i32,
    /// `Mount.spec` — `mount`'s aname. Empty for a bind, and for a mount
    /// that gave none.
    pub spec: String,
}

impl Element {
    pub fn new(chan: Chan) -> Self {
        Element { chan, mflag: mflag::MREPL, spec: String::new() }
    }

    /// The element as `bind`/`mount` made it: the channel, the flag word and
    /// the spec.
    pub fn with(chan: Chan, flag: i32, spec: &str) -> Self {
        Element { chan, mflag: flag & mflag::MMASK, spec: spec.to_string() }
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

/// What identifies the file a mount sits on: Plan 9's `(type, dev, qid)`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    dev: DevId,
    devno: u32,
    qid: Qid,
}

impl Key {
    fn of(c: &Chan) -> Key {
        Key { dev: c.dev, devno: c.devno, qid: c.qid }
    }
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
    pub from: Option<Chan>,
    pub mount: Vec<Element>,
}

/// A process's namespace: Plan 9's `Pgrp`, which is a table of `Mhead`.
#[derive(Debug)]
pub struct Ns {
    mounts: HashMap<Key, Mhead>,
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
impl Clone for Ns {
    fn clone(&self) -> Ns {
        Ns {
            mounts: self.mounts.clone(),
            id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            noattach: self.noattach,
        }
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
            mounts: HashMap::new(),
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

    /// `bind(2)` and `mount(2)`: put `to` over the file `on`.
    pub fn mount(&mut self, on: &Chan, to: Element, how: Bind) {
        let fresh = !self.mounts.contains_key(&Key::of(on));
        let head = self.mounts.entry(Key::of(on)).or_default();
        head.from = Some(on.clone());
        let list = &mut head.mount;
        // **`cmount` (`chan.c:707`), and the comment there is the whole of
        // it:** *"if this is a union mount, add the old node to the mount
        // chain."* Nothing was mounted here before, so the directory itself
        // is what the union's first element must be — otherwise `bind -a x /`
        // does not ADD to `/`, it replaces it, and every name that was there
        // is gone. `bind -a /root /` is the line that found this: it is how a
        // root becomes a file server, and it took `/dev` and `/env` with it.
        //
        // It is added with flags 0 (`newmount(m, old, 0, 0)`), so it never
        // carries `MCREATE`.
        if fresh && how != Bind::Replace {
            list.push(Element::new(on.clone()));
        }
        // **"copy a union when binding it onto a directory"** (`chan.c:719`).
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
        // um = um->next)` (`chan.c:727`).
        //
        // **The copies carry the ORDER's flag and the original's spec** —
        // `newmount(m, um->to, flg, um->spec)` (`chan.c:731`), where `flg =
        // order` with `MREPL` becoming `MAFTER`. Not the original's flag:
        // the new binding says where these go.
        let flg = match how {
            Bind::Replace | Bind::After => mflag::MAFTER,
            Bind::Before => mflag::MBEFORE,
        };
        let mut group = vec![to];
        for extra in group[0].chan.umh.clone().into_iter().skip(1) {
            group.push(Element::with(extra.chan, flg, &extra.spec));
        }
        match how {
            Bind::Replace => *list = group,
            Bind::Before => {
                for (i, e) in group.into_iter().enumerate() {
                    list.insert(i, e);
                }
            }
            Bind::After => list.extend(group),
        }
    }

    /// `findmount`: is anything mounted on this file? Answered by the file's
    /// identity, so every path that reaches it sees the same answer.
    pub fn findmount(&self, on: &Chan) -> Option<&[Element]> {
        self.mounts.get(&Key::of(on)).map(|h| h.mount.as_slice())
    }

    /// Every mount point, for `#p/<n>/ns` to print. Plan 9 walks `mnthash[]`
    /// with `mntscan` (`devproc.c`); the order there is the hash's and the
    /// `mountid` counter's, and here it is whatever `sort` makes stable.
    pub fn heads(&self) -> Vec<&Mhead> {
        let mut v: Vec<&Mhead> = self.mounts.values().collect();
        v.sort_by_key(|h| h.from.as_ref().map(|c| c.path.clone()).unwrap_or_default());
        v
    }

    /// `unmount(2)`. With a channel, remove that element; without, clear the
    /// mount point.
    pub fn unmount(&mut self, on: &Chan, what: Option<&Chan>) {
        let key = Key::of(on);
        match what {
            None => {
                self.mounts.remove(&key);
            }
            Some(w) => {
                if let Some(head) = self.mounts.get_mut(&key) {
                    head.mount.retain(|e| Key::of(&e.chan) != Key::of(w));
                    if head.mount.is_empty() {
                        self.mounts.remove(&key);
                    }
                }
            }
        }
    }

    /// Where a create in this directory lands: the first element that accepts
    /// one. `None` means creates are refused here.
    pub fn create_element(&self, on: &Chan) -> Option<&Element> {
        self.findmount(on)?.iter().find(|e| e.create())
    }

    pub fn is_empty(&self) -> bool {
        self.mounts.is_empty()
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
            ns.findmount(&on).unwrap().iter().map(|e| e.chan.qid.path).collect();
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
        assert_eq!(ns.findmount(&on).unwrap().len(), 1);
    }

    #[test]
    fn a_namespace_is_copied_not_shared_when_it_is_copied() {
        let mut parent = Ns::new();
        let on = file(1);
        parent.mount(&on, Element::new(file(10)), Bind::Replace);
        let mut child = parent.clone();
        child.mount(&on, Element::new(file(20)), Bind::Before);
        assert_eq!(parent.findmount(&on).unwrap().len(), 1);
        assert_eq!(child.findmount(&on).unwrap().len(), 2);
    }

    #[test]
    fn cleanname_resolves_dot_and_dotdot() {
        assert_eq!(clean("/n/z/../store"), "/n/store");
        assert_eq!(clean("/n//z/"), "/n/z");
        assert_eq!(clean("/../.."), "/");
    }
}
