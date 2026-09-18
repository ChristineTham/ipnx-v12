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
/// the ones this subset has no caller for yet.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum A {
    /// Resolve and open.
    Open,
    /// Resolve only — the caller wants the channel, not an open file.
    Access,
    /// Resolve, and it must be a directory (`chdir`).
    Todir,
    /// Resolve the thing that will be mounted upon.
    Mount,
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
    name: &str,
    slash: &Chan,
    dot: &Chan,
) -> Result<(Start, Vec<String>), String> {
    if name.is_empty() {
        return Err("empty file name".into());
    }
    if let Some(rest) = name.strip_prefix('#') {
        let (id, below) = dev::split(name).ok_or("bad # in file name")?;
        if id == DevId::Mnt {
            return Err("not attached".into());
        }
        // the spec is what follows the letter up to the first '/'
        let spec: String = rest.chars().skip(1).take_while(|c| *c != '/').collect();
        let d = tab.get(id).ok_or("bad # in file name")?;
        let chan = d.attach(&spec)?;
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
fn domount(ns: &Ns, c: Chan) -> Chan {
    match ns.findmount(&c) {
        Some(els) if !els.is_empty() => {
            let mut m = els[0].chan.clone();
            m.path = c.path.clone(); // the name is how we got here, not where we landed
            m
        }
        _ => c,
    }
}

/// `walk`: the elements, one at a time, stepping through mounts.
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
        if name != ".." && !nomount {
            c = domount(ns, c);
        }
        let d = tab.get(c.dev).ok_or("no such device")?;
        match d.walk(&c, name)? {
            Some(qid) => c = c.walked(name, qid),
            None => return Err(format!("'{}' does not exist", name)),
        }
    }
    if !nomount {
        c = domount(ns, c);
    }
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
    let (s, names) = start(tab, name, slash, dot)?;
    let mut c = walk(tab, ns, s.chan, &names, s.nomount)?;
    match amode {
        A::Access | A::Mount => {}
        A::Todir => {
            if !c.is_dir() {
                return Err("not a directory".into());
            }
        }
        A::Open => {
            let d = tab.get(c.dev).ok_or("no such device")?;
            d.open(&mut c, omode)?;
        }
    }
    Ok(c)
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
        let c = namec(&mut tab, &ns, &slash, &slash, "/init", A::Access, 0).unwrap();
        assert_eq!(c.path, "#/init", "the root device is `#/`, so the name joins without doubling");
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

        let c = namec(&mut tab, &ns, &slash, &slash, "/init", A::Access, 0).expect("resolve");
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
        let on = namec(&mut tab, &Ns::new(), &slash, &slash, "/init", A::Access, 0).unwrap();
        assert_eq!(on.dev, DevId::Root);

        let mut other = Other::new();
        let over = other.attach("").unwrap();
        tab.add(Box::new(other));

        let mut ns = Ns::new();
        ns.mount(&on, Element::new(over), Bind::Replace);

        let c = namec(&mut tab, &ns, &slash, &slash, "/init", A::Access, 0).unwrap();
        assert_eq!(
            c.dev,
            DevId::Srv,
            "a mount made on a walked-to component was not honoured there"
        );
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

        let rooted = namec(&mut tab, &ns, &slash, &slash, "/init", A::Access, 0).unwrap();
        let relative = namec(&mut tab, &ns, &slash, &slash, "init", A::Access, 0)
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
        fn attach(&mut self, _s: &str) -> Result<Chan, String> {
            Ok(Chan::attach(DevId::Srv, 0))
        }
        fn walk(&mut self, _c: &Chan, _n: &str) -> Result<Option<crate::ninep::Qid>, String> {
            Ok(Some(self.qid))
        }
        fn open(&mut self, _c: &mut Chan, _m: u16) -> Result<(), String> {
            Ok(())
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
        let e = namec(&mut tab, &ns, &slash, &slash, "/init", A::Open,
                      crate::chan::mode::OWRITE);
        assert!(e.is_err(), "devroot refuses writing, as rootwrite does");
    }
}
