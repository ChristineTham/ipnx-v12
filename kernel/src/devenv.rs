//! `#e` — the environment device (`plan9/sys/src/9/port/devenv.c`).
//!
//! The environment is **files**, not a string table a shell parses: each
//! variable is a file in a flat directory, and `$PATH` is what `rc` gets by
//! reading `/env/path`. The group behind it is the one `rfork`'s `RFENVG` and
//! `RFCENVG` share, copy or clear, which is why the device exists at all —
//! there has to be something for those bits to act on.
//!
//! An attach is **not** an allocation here, unlike `#|`: the group belongs to
//! the process, and `envgrp(c)` (`devenv.c:13`) finds it. So the channel
//! carries which group, and the kernel hands it over.
//!
//! **There are two groups, and the attach spec picks one.** `envattach`
//! (`devenv.c:64`) takes `"c"` for `confegrp`, *"the global environment group
//! containing the kernel configuration"* (`:16`), errors `Ebadarg` on any
//! other spec, and takes the calling process's own group for no spec at all.
//! That is what `#ec` is, and `initcode.c:28` binds both over `/env` — the
//! configuration first, the process's own second and with `MCREATE`, so a
//! variable you set is yours and the configuration is read underneath it.
//! `ksetenv(name, val, conf)` (`:386`) is how the kernel writes to either.

use crate::chan::Chan;
use crate::dev::{Dev, DevId};
use crate::ninep::{Qid, QTDIR};
use crate::proc::Up;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// `Maxenvsize` (`devenv.c:10`).
pub const MAXENVSIZE: usize = 16300;

const EPERM: &str = "permission denied";
const EBADARG: &str = "bad arg in system call";
const EEXIST: &str = "file already exists";
const ETOOBIG: &str = "value too big";
const ENONEXIST: &str = "file does not exist";

/// One process's environment group — Plan 9's `Egrp`. Shared through `Rc`,
/// which is what `rfork` without `RFENVG` means.
pub type Egrp = Rc<RefCell<HashMap<String, Vec<u8>>>>;

/// `Chan.aux` for a channel to the configuration group — Plan 9 puts
/// `&confegrp` there and nil for the per-process one (`devenv.c:77`), and
/// `envgrp` and `envwriteable` both read it back as exactly that two-way
/// choice (`:371`, `:379`).
const AUXCONF: u64 = 1;

/// `envgrp(c)` (`devenv.c:13`) finds the group this channel means: the
/// calling process's — here `up`, which the kernel sets before it dispatches
/// — or the configuration group, which the device owns as `static Egrp
/// confegrp` (`devenv.c:16`) owns it.
pub struct EnvDev {
    /// `eve` — the kernel-wide host owner (`auth.c:10`), shared rather than
    /// copied, because writing `#c/hostowner` renames it for everyone.
    eve: crate::dev::Eve,

    up: Rc<RefCell<Up>>,
    /// `static Egrp confegrp` (`devenv.c:16`). One per kernel, not per
    /// process, which is the whole difference between `#ec` and `#e`.
    confegrp: Egrp,
    /// Qid paths, so a file keeps its identity. `envcreate` numbers each from
    /// the group's counter (`devenv.c`).
    names: Vec<String>,
}

impl EnvDev {
    pub fn new(up: Rc<RefCell<Up>>) -> EnvDev {
        EnvDev { up, confegrp: Egrp::default(), names: Vec::new(), eve: Default::default() }
    }

    /// `envgrp(c)` (`devenv.c:369`): *"if(c->aux == nil) return up->egrp;
    /// return c->aux"*.
    fn egrp(&self, c: &Chan) -> Egrp {
        if c.aux == AUXCONF {
            return self.confegrp.clone();
        }
        self.up.borrow().egrp().unwrap_or_default()
    }

    /// `envwriteable(c)` (`devenv.c:377`): *"return iseve() || c->aux ==
    /// nil"*. Your own environment is yours; the kernel's configuration is
    /// eve's, and everyone else reads it.
    fn writeable(&self, c: &Chan) -> bool {
        c.aux != AUXCONF || crate::dev::iseve(&self.eve, &self.up.borrow().user())
    }

    /// The qid path for a name — assigned once and kept, as `++eg->path` does.
    fn qid(&mut self, name: &str) -> u64 {
        if let Some(i) = self.names.iter().position(|n| n == name) {
            return i as u64 + 1;
        }
        self.names.push(name.to_string());
        self.names.len() as u64
    }

    fn name(&self, qid: u64) -> Option<&String> {
        self.names.get(qid as usize - 1)
    }
}

impl EnvDev {
    /// The directory's children, in `devdirread`'s order. `#e` has no static
    /// `Dirtab` — the environment group IS the table — so this is `envgen`
    /// (`devenv.c:33`), which walks the group.
    fn entries(&mut self, c: &Chan) -> Vec<crate::ninep::Dir> {
        let user = self.up.borrow().user();
        let mut named: Vec<(String, usize)> = {
            let eg = self.egrp(c);
            let g = eg.borrow();
            g.iter().map(|(k, v)| (k.clone(), v.len())).collect()
        };
        named.sort();
        named
            .into_iter()
            .map(|(name, len)| {
                let qid = Qid { qtype: 0, vers: 0, path: self.qid(&name) };
                crate::dev::devdir(c, qid, &name, len as u64, &user, &self.eve.borrow(), 0o666)
            })
            .collect()
    }
}

impl Dev for EnvDev {
    fn seteve(&mut self, eve: crate::dev::Eve) {
        self.eve = eve;
    }

    fn id(&self) -> DevId {
        DevId::Env
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    /// `envattach` (`devenv.c:64`). **A spec that is not `"c"` is
    /// `Ebadarg`** — it does not quietly mean the same as no spec, which is
    /// what this device used to do, so `#ewhatever` attached the caller's own
    /// environment and nothing said otherwise.
    fn attach(&mut self, spec: &str) -> Result<Chan, String> {
        let aux = match spec {
            "" => 0,
            "c" => AUXCONF,
            _ => return Err(EBADARG.into()),
        };
        let mut c = Chan::attach_spec(DevId::Env, 0, spec);
        c.aux = aux;
        Ok(c)
    }

    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Chan>, String> {
        if !c.qid.is_dir() {
            return Err("not a directory".into());
        }
        if name == ".." || name == "." {
            return Ok(Some(c.walked(name, Qid { qtype: QTDIR, vers: 0, path: 0 })));
        }
        if !self.egrp(c).borrow().contains_key(name) {
            return Ok(None);
        }
        let path = self.qid(name);
        Ok(Some(c.walked(name, Qid { qtype: 0, vers: 0, path })))
    }

    /// `envopen` (`devenv.c:96`): the directory opens for reading only, and a
    /// file opens for writing only if `envwriteable` says so.
    fn open(&mut self, mut c: Chan, mode: u16) -> Result<Chan, String> {
        if mode != crate::chan::mode::OREAD && (c.qid.is_dir() || !self.writeable(&c)) {
            return Err(EPERM.into());
        }
        c.mode = mode;
        Ok(c)
    }

    /// `envcreate` (`devenv.c:141`): only in the directory, and a name that
    /// exists is `Eexist` rather than a silent overwrite.
    fn create(&mut self, c: &mut Chan, name: &str, mode: u16, _perm: u32) -> Result<(), String> {
        if !c.qid.is_dir() {
            return Err(EPERM.into());
        }
        if self.egrp(c).borrow().contains_key(name) {
            return Err(EEXIST.into());
        }
        self.egrp(c).borrow_mut().insert(name.to_string(), Vec::new());
        let path = self.qid(name);
        c.qid = Qid { qtype: 0, vers: 0, path };
        c.mode = mode;
        Ok(())
    }

    fn read(&mut self, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String> {
        if c.qid.is_dir() {
            let entries = self.entries(c);
            return Ok(crate::dev::devdirread(c, n, &entries));
        }
        let name = self.name(c.qid.path).ok_or(ENONEXIST)?.clone();
        let eg = self.egrp(c);
        let g = eg.borrow();
        let v = g.get(&name).ok_or(ENONEXIST)?;
        let off = off as usize;
        if off >= v.len() {
            return Ok(Vec::new());
        }
        Ok(v[off..(off + n).min(v.len())].to_vec())
    }

    /// `envwrite` (`devenv.c:268`): a write at an offset REPLACES from there,
    /// and the value grows to fit. `Maxenvsize` bounds it.
    fn write(&mut self, c: &mut Chan, data: &[u8], off: u64) -> Result<usize, String> {
        if c.qid.is_dir() {
            return Err(EPERM.into());
        }
        let off = off as usize;
        if off > MAXENVSIZE || data.len() > MAXENVSIZE - off {
            return Err(ETOOBIG.into());
        }
        let name = self.name(c.qid.path).ok_or(ENONEXIST)?.clone();
        let eg = self.egrp(c);
        let mut g = eg.borrow_mut();
        let v = g.get_mut(&name).ok_or(ENONEXIST)?;
        if v.len() < off + data.len() {
            v.resize(off + data.len(), 0);
        }
        v[off..off + data.len()].copy_from_slice(data);
        Ok(data.len())
    }

    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        if c.qid.is_dir() {
            let user = self.up.borrow().user();
            let name = if c.aux == AUXCONF { "#ec" } else { "#e" };
            return Ok(crate::dev::devdir(c, c.qid, name, 0, &user, &self.eve.borrow(), 0o775).conv_d2m());
        }
        let name = self.name(c.qid.path).ok_or(ENONEXIST)?.clone();
        let len = self.egrp(c).borrow().get(&name).map(|v| v.len()).unwrap_or(0);
        let user = self.up.borrow().user();
        Ok(crate::dev::devdir(c, c.qid, &name, len as u64, &user, &self.eve.borrow(), 0o666).conv_d2m())
    }

    fn wstat(&mut self, _c: &mut Chan, _e: &[u8]) -> Result<(), String> {
        Err(EPERM.into())
    }

    /// `envremove` — a variable is a file, so `rm /env/x` unsets it.
    fn remove(&mut self, c: &mut Chan) -> Result<(), String> {
        if c.qid.is_dir() {
            return Err(EPERM.into());
        }
        let name = self.name(c.qid.path).ok_or(ENONEXIST)?.clone();
        self.egrp(c).borrow_mut().remove(&name).ok_or(ENONEXIST)?;
        Ok(())
    }

    fn close(&mut self, _c: &mut Chan) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chan::mode::{OREAD, OWRITE};

    fn env() -> (EnvDev, Egrp) {
        let procs = Rc::new(RefCell::new(crate::proc::Procs::new(Chan::attach(
            DevId::Root,
            0,
        ))));
        let g = procs.borrow().get(1).unwrap().env.clone();
        let up = Rc::new(RefCell::new(Up { pid: 1, procs }));
        (EnvDev::new(up), g)
    }

    fn make(d: &mut EnvDev, name: &str, val: &[u8]) -> Chan {
        let mut c = d.attach("").unwrap();
        d.create(&mut c, name, OWRITE, 0o666).unwrap();
        d.write(&mut c, val, 0).unwrap();
        c
    }

    /// A variable is a file: create it, write it, walk to it by name, read it
    /// back. That is the whole of what `$PATH` is here.
    #[test]
    fn a_variable_is_a_file_created_written_and_read() {
        let (mut d, _) = env();
        make(&mut d, "path", b"/bin");
        let dir = d.attach("").unwrap();
        let mut c = d.walk(&dir, "path").unwrap().expect("no /env/path");
        assert_eq!(d.read(&mut c, 64, 0).unwrap(), b"/bin");
    }

    /// The group is what `rfork` shares. Two devices over the same `Egrp` see
    /// each other's writes; that is the sharing, not a metaphor for it.
    #[test]
    fn two_devices_over_one_group_see_the_same_variables() {
        let (mut a, g) = env();
        make(&mut a, "user", b"kitty");
        let procs = Rc::new(RefCell::new(crate::proc::Procs::new(Chan::attach(DevId::Root, 0))));
        procs.borrow_mut().get_mut(1).unwrap().env = g;
        let mut b = EnvDev::new(Rc::new(RefCell::new(Up { pid: 1, procs })));
        let dir = b.attach("").unwrap();
        let mut c = b.walk(&dir, "user").unwrap().expect("not shared");
        assert_eq!(b.read(&mut c, 64, 0).unwrap(), b"kitty");
    }

    /// `envcreate` refuses a name that exists (`Eexist`) rather than
    /// overwriting, which is how a namespace tells you it already has one.
    #[test]
    fn creating_a_name_twice_is_an_error() {
        let (mut d, _) = env();
        make(&mut d, "path", b"/bin");
        let mut dir = d.attach("").unwrap();
        assert!(d.create(&mut dir, "path", OWRITE, 0o666).is_err());
    }

    /// A write at an offset replaces from there and grows the value; it does
    /// not append blindly.
    #[test]
    fn a_write_at_an_offset_replaces_from_there() {
        let (mut d, _) = env();
        let mut c = make(&mut d, "path", b"/bin:/rc/bin");
        d.write(&mut c, b"XXX", 1).unwrap();
        assert_eq!(d.read(&mut c, 64, 0).unwrap(), b"/XXX:/rc/bin");
        d.write(&mut c, b"!", 20).unwrap();
        assert_eq!(d.read(&mut c, 64, 0).unwrap().len(), 21, "it grew to fit");
    }

    /// `Maxenvsize` (`devenv.c:10`) bounds a value.
    #[test]
    fn a_value_has_a_limit() {
        let (mut d, _) = env();
        let mut c = make(&mut d, "big", b"");
        assert!(d.write(&mut c, &vec![b'x'; MAXENVSIZE + 1], 0).is_err());
        assert!(d.write(&mut c, b"x", MAXENVSIZE as u64 + 1).is_err());
    }

    /// Unsetting a variable is removing a file, which is the point of the
    /// environment being files at all.
    #[test]
    fn removing_the_file_unsets_the_variable() {
        let (mut d, g) = env();
        let mut c = make(&mut d, "tmp", b"x");
        d.remove(&mut c).unwrap();
        assert!(!g.borrow().contains_key("tmp"));
        let dir = d.attach("").unwrap();
        assert!(d.walk(&dir, "tmp").unwrap().is_none());
    }

    /// Reading the directory lists the names, so `ls /env` works.
    ///
    /// **A directory reads as `Dir` entries, not as text.** `dirread(2)`
    /// parses exactly this with `convM2D`, and nothing in a Plan 9 userland
    /// can read a list of names. It WAS a list of names here, and rc's
    /// `Vinit` — which reads `/env` to find its variables — got nonsense:
    /// every variable took the value of another.
    #[test]
    fn reading_the_directory_gives_dir_entries() {
        let (mut d, _) = env();
        make(&mut d, "path", b"/bin");
        make(&mut d, "user", b"kitty");
        let mut dir = d.attach("").unwrap();
        let b = d.read(&mut dir, 256, 0).unwrap();

        let first = crate::ninep::Dir::conv_m2d(&b).expect("an entry");
        assert_eq!((first.name.as_str(), first.length), ("path", 4));
        assert_eq!(first.dtype, 'e' as u16, "the device letter, not an index");

        let n = 2 + u16::from_le_bytes([b[0], b[1]]) as usize;
        let second = crate::ninep::Dir::conv_m2d(&b[n..]).expect("the next entry");
        assert_eq!((second.name.as_str(), second.length), ("user", 5));
        assert_eq!(2 + u16::from_le_bytes([b[n], b[n + 1]]) as usize, b.len() - n);
    }

    /// An entry is never split. A read too small for the next one stops, and
    /// the read after it resumes at that ENTRY — which is what `Chan.dri` is
    /// for, a byte offset being no use when entries vary in length.
    #[test]
    fn a_directory_read_stops_at_a_whole_entry_and_resumes_there() {
        let (mut d, _) = env();
        make(&mut d, "path", b"/bin");
        make(&mut d, "user", b"kitty");
        let mut dir = d.attach("").unwrap();
        let whole = d.read(&mut dir, 4096, 0).unwrap();
        let first = 2 + u16::from_le_bytes([whole[0], whole[1]]) as usize;

        let mut dir = d.attach("").unwrap();
        let a = d.read(&mut dir, first + 1, 0).unwrap();
        assert_eq!(a.len(), first, "one whole entry, not a byte more");
        let b = d.read(&mut dir, 4096, a.len() as u64).unwrap();
        assert_eq!([a, b].concat(), whole, "and the rest follows it exactly");
    }

    #[test]
    fn the_directory_itself_is_not_written_or_removed() {
        let (mut d, _) = env();
        let mut dir = d.attach("").unwrap();
        assert!(d.write(&mut dir, b"x", 0).is_err());
        assert!(d.remove(&mut dir).is_err());
    }

    /// `envattach` (`devenv.c:69`): the spec `c` is the configuration group,
    /// no spec is the calling process's own, and **anything else is
    /// `Ebadarg`**. It used to accept every spec and mean the same thing by
    /// all of them.
    #[test]
    fn the_attach_spec_picks_the_group_and_nothing_else_is_accepted() {
        let (mut d, _) = env();
        assert_eq!(d.attach("").unwrap().aux, 0);
        assert_eq!(d.attach("c").unwrap().aux, AUXCONF);
        assert_eq!(d.attach("").unwrap().path, "#e");
        assert_eq!(d.attach("c").unwrap().path, "#ec", "the spec is in the path");
        assert!(d.attach("x").is_err(), "Ebadarg, not the per-process group");
        assert!(d.attach("conf").is_err());
    }

    /// The two groups are different groups. A variable set through `#ec` is
    /// not in the process's own environment, and `rfork`'s `RFENVG` — which
    /// replaces `up->egrp` — cannot touch the configuration.
    #[test]
    fn the_configuration_group_is_not_the_process_group() {
        let (mut d, g) = env();
        let mut conf = d.attach("c").unwrap();
        d.create(&mut conf, "rootdir", OWRITE, 0o666).unwrap();
        d.write(&mut conf, b"/root", 0).unwrap();

        assert!(!g.borrow().contains_key("rootdir"), "not in the process's own");
        let own = d.attach("").unwrap();
        assert!(d.walk(&own, "rootdir").unwrap().is_none());

        let dir = d.attach("c").unwrap();
        let mut c = d.walk(&dir, "rootdir").unwrap().expect("no #ec/rootdir");
        assert_eq!(d.read(&mut c, 64, 0).unwrap(), b"/root");
    }

    /// `envwriteable` (`devenv.c:377`): *"iseve() || c->aux == nil"*. Eve
    /// writes the configuration; everyone else reads it and writes only their
    /// own.
    #[test]
    fn only_eve_writes_the_configuration() {
        let (mut d, _) = env();
        let mut conf = d.attach("c").unwrap();
        d.create(&mut conf, "cputype", OWRITE, 0o666).unwrap();

        let dir = d.attach("c").unwrap();
        let c = d.walk(&dir, "cputype").unwrap().unwrap();
        assert!(d.open(c.clone(), OWRITE).is_ok(), "eve may write it");

        d.up.borrow_mut().procs.borrow_mut().get_mut(1).unwrap().user = "kitty".into();
        assert!(d.open(c.clone(), OREAD).is_ok(), "anyone may read it");
        assert!(d.open(c, OWRITE).is_err(), "kitty is not eve");

        let mut own = d.attach("").unwrap();
        d.create(&mut own, "path", OWRITE, 0o666).unwrap();
        let dir = d.attach("").unwrap();
        let c = d.walk(&dir, "path").unwrap().unwrap();
        assert!(d.open(c, OWRITE).is_ok(), "your own environment is yours");
    }

    /// `envopen` (`devenv.c:103`): the directory is read-only, whichever
    /// group it is.
    #[test]
    fn the_directory_opens_for_reading_only() {
        let (mut d, _) = env();
        let dir = d.attach("").unwrap();
        assert!(d.open(dir.clone(), OREAD).is_ok());
        assert!(d.open(dir, OWRITE).is_err());
    }

}
