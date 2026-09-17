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

/// What a mount point resolves to: Plan 9's `Mount`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    /// `Mount.to` — the channel replacing the channel mounted upon.
    pub chan: Chan,
    /// `MCREATE` — a create in this directory lands here. The first element
    /// carrying it wins; a union with none refuses creates, which is how a
    /// read-only union is expressed without a read-only flag.
    pub create: bool,
}

impl Element {
    pub fn new(chan: Chan) -> Self {
        Element { chan, create: false }
    }
    pub fn creatable(chan: Chan) -> Self {
        Element { chan, create: true }
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

/// A process's namespace: Plan 9's `Pgrp`, which is a table of `Mhead`.
#[derive(Clone, Debug, Default)]
pub struct Ns {
    mounts: HashMap<Key, Vec<Element>>,
}

impl Ns {
    pub fn new() -> Self {
        Ns::default()
    }

    /// `bind(2)` and `mount(2)`: put `to` over the file `on`.
    pub fn mount(&mut self, on: &Chan, to: Element, how: Bind) {
        let list = self.mounts.entry(Key::of(on)).or_default();
        match how {
            Bind::Replace => *list = vec![to],
            Bind::Before => list.insert(0, to),
            Bind::After => list.push(to),
        }
    }

    /// `findmount`: is anything mounted on this file? Answered by the file's
    /// identity, so every path that reaches it sees the same answer.
    pub fn findmount(&self, on: &Chan) -> Option<&[Element]> {
        self.mounts.get(&Key::of(on)).map(|v| v.as_slice())
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
                if let Some(list) = self.mounts.get_mut(&key) {
                    list.retain(|e| Key::of(&e.chan) != Key::of(w));
                    if list.is_empty() {
                        self.mounts.remove(&key);
                    }
                }
            }
        }
    }

    /// Where a create in this directory lands: the first element that accepts
    /// one. `None` means creates are refused here.
    pub fn create_element(&self, on: &Chan) -> Option<&Element> {
        self.findmount(on)?.iter().find(|e| e.create)
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
