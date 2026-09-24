//! The device table — the file interface as function calls.
//!
//! Inside the kernel a device is not a 9P server: it is a table of functions,
//! and a walk or a read is a call. Wire 9P exists at exactly one boundary, the
//! mount driver, and nowhere else. This is Plan 9's own shape, and it is why
//! the system can have one protocol without paying to marshal it against
//! itself.
//!
//! The trait below is `struct Dev` from `plan9/sys/src/9/port/portdat.h`,
//! minus the entries a hosted kernel has nothing to do with: `reset`, `init`,
//! `shutdown` and `power` are hardware lifecycle, `bread`/`bwrite` are the
//! block fast path, `config` is device configuration at boot.

use crate::chan::Chan;

/// The device letters this kernel has: nine. Plan 9 has twenty-four.
///
/// **A letter earns its place by being Plan 9's.** Christine's rule against
/// growth is about invention, not category — *"that rule was to stop you from
/// adding all sorts of invented stuff in the kernel… it doesn't mean that is
/// the only thing the kernel does. Use Plan 9 as a guide"* (2026-09-18). So
/// the question for any device is *does Plan 9's kernel have it*, not *is it
/// process management*.
///
/// **What is absent: `#i` draw and `#m` mouse**, and only those two of the
/// ones a hosted kernel might want. Plan 9 has them because its kernel drives
/// hardware, and this one drives none. The host serves them — *"I have a
/// screen, a keyboard and a mouse. I will serve these as virtual devices to
/// the IPNX kernel"* — and the kernel reaches them the way it reaches
/// anything, by mounting what a server offers.
///
/// **`#c` is in** (Christine, 2026-09-18: *"we should keep `#c` in since it
/// holds a variety of kernel info"*). It was struck out twice before that, by
/// asking whether its 23 files were *orchestration* rather than whether Plan 9
/// has them — the misreading the clarified rule names. `consdir[]` (`devcons.c:605`) is 23
/// files and only two are the console; the rest is state the kernel has
/// anyway — its name, its uptime, its message log, a process's own pid —
/// shown as files, which is the founding principle rather than an addition.
/// It also carries `/dev/user`, the only way a process drops to `none`
/// (`auth.c:107`), and `/dev/hostowner`.
///
/// Some of the 23 describe hardware this kernel does not have — `swap`,
/// `drivers`, `config`, `sysstat`. Those are absent here for the reason `#i`
/// and `#m` are, not by a separate rule.
///
/// **`#¤`**: it changes a process's one `char *user` (`devcap.c:215`), which
/// is per-process state and nothing else's business.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum DevId {
    /// `#/` — the root a namespace starts from, before anything is mounted.
    Root,
    /// `#|` — a pipe: an attach mints two cross-connected ends. This is how
    /// two processes talk when neither serves the other.
    Pipe,
    /// `#s` — srv: a channel posted under a name, so a process that did not
    /// inherit it can find it. This is how a server becomes reachable.
    Srv,
    /// `#M` — mnt: the mount driver, and the ONLY place wire 9P is marshalled.
    Mnt,
    /// `#p` — proc: processes as files. The kernel holds this state, so the
    /// kernel serves it.
    Proc,
    /// `#d` — dup: a process's own descriptors as files.
    Dup,
    /// `#e` — env: the environment group, which `rfork`'s `ENVG` and `CENVG`
    /// flags exist to share, copy or clear.
    Env,
    /// `#c` — cons: the kernel's own state as files. Two of its 23 are the
    /// console (`cons`, `consctl`) and are the host's; the rest is identity
    /// (`user`, `hostowner`, `hostdomain`), this process's numbers (`pid`,
    /// `ppid`, `pgrpid`, `cputime`), the clock (`time`, `bintime`), the
    /// kernel's own name and log (`sysname`, `osversion`, `kmesg`, `kprint`),
    /// and the generators (`null`, `zero`, `random`).
    Cons,
    /// `#¤` — cap: the only way a process becomes another user. eve mints a
    /// capability into `caphash`; a process spends it through `capuse`
    /// (`devcap.c:215`). One use each — `remcap` unlinks it.
    Cap,
    /// `#9` — virtio9p: a CHANNEL to a 9P server the machine provides, and
    /// nothing else. `pc/devvirtio9p.c:1227`, whose own comment is this
    /// system's situation exactly:
    ///
    /// > *mount a host directory exported by qemu's `-device
    /// > virtio-9p-pci` / `-fsdev local` directly over a virtqueue, with no
    /// > network in the path.*
    /// >
    /// > ```text
    /// > bind -a '#9' /dev
    /// > mount -c '#9/0' /n/host
    /// > ```
    ///
    /// It marshals nothing: `#M` writes a whole T-message down the channel
    /// and reads the R-message back, exactly as it would down a TCP
    /// connection. This is a 9legacy device — not in `plan9-stock` — and it
    /// is configured into the shipped kernels (`pc/pcf:11`).
    Virtio9p,
}

impl DevId {
    pub fn letter(self) -> char {
        match self {
            DevId::Root => '/',
            DevId::Pipe => '|',
            DevId::Srv => 's',
            DevId::Mnt => 'M',
            DevId::Proc => 'p',
            DevId::Dup => 'd',
            DevId::Env => 'e',
            DevId::Cons => 'c',
            DevId::Cap => '\u{a4}',
            DevId::Virtio9p => '9',
        }
    }

    /// `Dev.name` (`portdat.h:242`) — Plan 9 carries it beside the letter, and
    /// `/dev/drivers` prints both: `#%C %s` from `->dc` and `->name`
    /// (`devcons.c:198`).
    pub fn name(self) -> &'static str {
        match self {
            DevId::Root => "root",
            DevId::Pipe => "pipe",
            DevId::Srv => "srv",
            DevId::Mnt => "mnt",
            DevId::Proc => "proc",
            DevId::Dup => "dup",
            DevId::Env => "env",
            DevId::Cons => "cons",
            DevId::Cap => "cap",
            DevId::Virtio9p => "virtio9p",
        }
    }

    pub fn from_letter(c: char) -> Option<DevId> {
        Some(match c {
            '/' => DevId::Root,
            '|' => DevId::Pipe,
            's' => DevId::Srv,
            'M' => DevId::Mnt,
            'p' => DevId::Proc,
            'd' => DevId::Dup,
            'e' => DevId::Env,
            'c' => DevId::Cons,
            '\u{a4}' => DevId::Cap,
            '9' => DevId::Virtio9p,
            _ => return None,
        })
    }
}

/// What a device can be asked. `struct Dev`'s entries, by their Plan 9 names.
///
/// Every one takes a [`Chan`], which is the point: a device does not know about
/// paths, processes or namespaces. It is handed a channel and answers about it.
pub trait Dev {
    fn id(&self) -> DevId;

    /// So the kernel can reach the mount driver's own methods. Plan 9 needs
    /// no equivalent: `devtab[]` is an array of `Dev*` and `mntattach` is
    /// reached directly.
    fn as_any(&mut self) -> &mut dyn std::any::Any;

    /// `attach(2)`'s device half: produce the channel that is this device's
    /// root. `spec` is the text after the letter, which most devices ignore.
    fn attach(&mut self, spec: &str) -> Result<Chan, String>;

    /// Walk one name. `None` means "no such file" — an ordinary answer, not an
    /// error, because that is how a union walk tries the next element.
    /// `walk` — **the device produces the NEW CHANNEL**, as `devwalk` fills
    /// in `nc` (`dev.c:169`, `Walkqid* (*walk)(Chan*, Chan*, char**, int)`).
    ///
    /// It returned a qid and let the caller build the channel, which works
    /// for every device whose channels carry nothing of the device's own —
    /// and not for `#M`, where a walk MINTS A FID and the channel is the only
    /// place to keep it. The mount driver walked to a new fid and dropped it,
    /// so every channel through a mount carried the mount root's fid: a
    /// `create` then moved the root onto the new file, and the next name
    /// resolved through it was "unknown fid".
    ///
    /// `Ok(None)` is *"no such name here"*, which a union walk answers by
    /// trying the next element — not an error.
    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Chan>, String>;

    /// `cclone` (`chan.c:837`) — *"our own copy"*, which Plan 9 gets by
    /// walking NO names: `devtab[c->type]->walk(c, nil, nil, 0)`.
    ///
    /// `namec` takes one before it opens, removes or creates (`chan.c:1479`,
    /// `:1611`), and the comment there says why: *"We need our own copy of
    /// the Chan because we're about to send a create, which will move it."*
    ///
    /// **The default is the copy itself**, because a channel to a device in
    /// this kernel carries nothing the device also knows about — the same
    /// reason `devclone` is a `memmove` for them. `#M` overrides it, where a
    /// channel carries a fid the server holds and a copy of the number is not
    /// a copy of the file.
    fn cclone(&mut self, c: &Chan) -> Result<Chan, String> {
        Ok(c.clone())
    }

    /// Take the kernel's `eve` (`auth.c:10`). **Plan 9's devices just read
    /// the global** — `devdir` passes it as every file's group
    /// (`dev.c:106`), `iseve()` compares it, and `hostownerwrite` renames it
    /// for all of them at once (`auth.c:135`). A Rust kernel cannot have an
    /// ambient mutable global, so the table hands the one cell over as a
    /// device joins it. A device with no use for it ignores this.
    fn seteve(&mut self, _eve: crate::dev::Eve) {}

    /// `Chan* (*open)(Chan*, int)` (`portdat.h:250`). **It returns a
    /// channel**, which is not ceremony: `devdup`'s open answers with the
    /// channel the fd already holds (`devdup.c`, `dupopen` → `fdtochan`), so a
    /// dup IS an open. A device that just marks its argument returns it.
    fn open(&mut self, c: Chan, mode: u16) -> Result<Chan, String>;
    fn create(&mut self, c: &mut Chan, name: &str, mode: u16, perm: u32) -> Result<(), String>;
    fn read(&mut self, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String>;
    fn write(&mut self, c: &mut Chan, data: &[u8], off: u64) -> Result<usize, String>;
    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String>;
    fn wstat(&mut self, c: &mut Chan, edir: &[u8]) -> Result<(), String>;
    fn remove(&mut self, c: &mut Chan) -> Result<(), String>;
    fn close(&mut self, c: &mut Chan);

    /// **`incref(c)`** on an open channel this kernel has copied rather
    /// than shared. Plan 9 counts references on the `Chan`, and the device
    /// is closed once, at the last; here a channel is a value, and each
    /// copy is closed on its own — so a device that counts its opens counts
    /// the copy. `srvopen` is where it happens: *"incref(sp->chan); return
    /// sp->chan"* (`devsrv.c:135`). Most devices count nothing.
    fn incref(&mut self, _c: &Chan) {}

    // `bread` and `bwrite` are absent, and this is the one kind of difference
    // that needs no approval: **Plan 9's counterpart cannot exist here.**
    //
    // They take a `Block` — the kernel buffer the network stack and the
    // queues pass around (`allocb`, `freeb`, `BLEN`). A device that cares
    // handles one natively and saves a copy; every device that does not gets
    // `devbread`/`devbwrite`, which forward to `read` and `write` and nothing
    // else: `devtab[c->type]->write(c, bp->rp, BLEN(bp), offset)`
    // (`dev.c:418`).
    //
    // This kernel has no `Block`, because it has no network stack and no
    // queues for one to travel through. Adding the pair would add
    // `devbread`/`devbwrite` and nothing more — a forward to the methods
    // above, which callers already use. They return when there is something
    // to pass.
}

/// `devpermcheck` — `dev.c:339`, verbatim in behaviour:
///
/// ```c
/// static int access[] = { 0400, 0200, 0600, 0100 };
/// if(strcmp(up->user, fileuid) == 0)   perm <<= 0;   /* owner */
/// else if(strcmp(up->user, eve) == 0)  perm <<= 3;   /* GROUP bits */
/// else                                 perm <<= 6;   /* other bits */
/// t = access[omode&3];
/// if((t&perm) != t) error(Eperm);
/// ```
///
/// The perm is shifted **left** and tested against the owner's mask, so a
/// non-owner is judged by its own class's bits moved into that position. eve
/// gets the GROUP bits, which is why the host owner is not root: on a file of
/// mode `0700` eve is denied.
pub fn permcheck(user: &str, fileuid: &str, eve: &str, perm: u32, omode: u16) -> Result<(), String> {
    const ACCESS: [u32; 4] = [0o400, 0o200, 0o600, 0o100];
    let perm = if user == fileuid {
        perm
    } else if user == eve {
        perm << 3
    } else {
        perm << 6
    };
    let t = ACCESS[(omode & 3) as usize];
    if perm & t == t {
        Ok(())
    } else {
        Err("permission denied".into())
    }
}

/// Split a device path into its device, its ATTACH SPEC, and the path below:
/// `#s/store` is `Srv`, no spec and `store`; `#ec/cputype` is `Env`, the spec
/// `c` and `cputype`. `None` if this is not a device path.
///
/// `namec` (`chan.c:1348`) takes the letter and the spec together — *"while(
/// *name != '\0' && (*name != '/' || n < 2))"* — so everything up to the
/// first `/` is the device's business and the walk starts after it. The
/// `n < 2` is `#/`, the root device, whose letter IS a slash.
///
/// **The spec is not part of the path below.** It was, and `bind #ec /env`
/// therefore attached `#e` and then walked to a file called `c`.
pub fn split(path: &str) -> Option<(DevId, &str, &str)> {
    let rest = path.strip_prefix('#')?;
    let mut chars = rest.char_indices();
    let (_, letter) = chars.next()?;
    let dev = DevId::from_letter(letter)?;
    let after = match chars.next() {
        Some((i, _)) => i,
        None => rest.len(),
    };
    let tail = &rest[after..];
    let (spec, below) = match tail.find('/') {
        Some(i) => (&tail[..i], &tail[i + 1..]),
        None => (tail, ""),
    };
    Some((dev, spec, below))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `devpermcheck`'s shift is LEFT, and eve gets the group bits. A right
    /// shift passes an owner's own open and fails everything else, which is
    /// how this was first written.
    #[test]
    fn a_permission_check_shifts_the_mode_as_plan_nine_does() {
        // the owner reading and writing its own 0600 file
        assert!(permcheck("kitty", "kitty", "eve", 0o600, 0).is_ok());
        assert!(permcheck("kitty", "kitty", "eve", 0o600, 1).is_ok());
        // eve is judged by the GROUP bits, so 0700 denies it
        assert!(permcheck("eve", "kitty", "eve", 0o700, 0).is_err(), "eve is not root");
        assert!(permcheck("eve", "kitty", "eve", 0o640, 0).is_ok(), "group r");
        // anyone else, by the other bits
        assert!(permcheck("none", "kitty", "eve", 0o640, 0).is_err());
        assert!(permcheck("none", "kitty", "eve", 0o644, 0).is_ok());
        // OEXEC is its own bit, not a read
        assert!(permcheck("kitty", "kitty", "eve", 0o400, 3).is_err());
        assert!(permcheck("kitty", "kitty", "eve", 0o500, 3).is_ok());
    }

    #[test]
    fn a_device_path_splits_at_the_letter() {
        assert_eq!(split("#s/store"), Some((DevId::Srv, "", "store")));
        assert_eq!(split("#p/1/ctl"), Some((DevId::Proc, "", "1/ctl")));
        assert_eq!(split("#|"), Some((DevId::Pipe, "", "")));
        assert_eq!(split("/srv/store"), None);
        // The attach spec is the device's, not a name to walk
        // (`chan.c:1348`).
        assert_eq!(split("#ec"), Some((DevId::Env, "c", "")));
        assert_eq!(split("#ec/cputype"), Some((DevId::Env, "c", "cputype")));
    }

    /// A guard on this kernel, after its author minted three letters in a day.
    /// `H`, `V` and `Z` are not Plan 9's at all; `i` and `m` are, and drive
    /// hardware this kernel does not have.
    #[test]
    fn a_letter_this_kernel_has_no_business_with_is_not_a_device() {
        for absent in ['H', 'V', 'Z', 'i', 'm'] {
            assert!(DevId::from_letter(absent).is_none(), "#{absent} must not be a device here");
        }
    }

    /// `#¤` is a wide character in Plan 9 too (`devcap.c:268`, `L'¤'`), so a
    /// byte-wise split would cut it in half.
    #[test]
    fn the_cap_device_splits_on_a_multibyte_letter() {
        assert_eq!(split("#\u{a4}/capuse"), Some((DevId::Cap, "", "capuse")));
        assert_eq!(split("#\u{a4}"), Some((DevId::Cap, "", "")));
        assert_eq!(split("#c/user"), Some((DevId::Cons, "", "user")));
    }

    #[test]
    fn every_letter_round_trips() {
        for d in [
            DevId::Root, DevId::Pipe, DevId::Srv, DevId::Mnt, DevId::Proc, DevId::Dup,
            DevId::Env, DevId::Cons, DevId::Cap,
        ] {
            assert_eq!(DevId::from_letter(d.letter()), Some(d));
        }
    }
}

/// `char *eve` (`auth.c:10`) — **kernel-wide, mutable, and it starts
/// EMPTY**: `kstrdup(&eve, "")` in `userinit` (`pc/main.c:285`), with the
/// first process's user a copy of it (`:287`). It is `boot` that names the
/// host owner, by writing `#c/hostowner` with `$user` or, failing that,
/// `"glenda"` (`bootauth.c:56`); `hostownerwrite` lets it because `iseve()`
/// is then comparing two empty strings (`auth.c:128`).
///
/// A comment here once cited `auth.c:10` as `char *eve = "bootes"`, which is
/// **not in this tree** — 9legacy has a bare `char *eve;`. The value was a
/// compile-time constant, so the host owner could not be set at all.
///
/// Rust spelling of a global every device reads: one cell, shared.
pub type Eve = std::rc::Rc<std::cell::RefCell<String>>;

/// `iseve()` (`auth.c:17`): *"return strcmp(eve, up->user) == 0"*.
pub fn iseve(eve: &Eve, user: &str) -> bool {
    *eve.borrow() == user
}

/// `devdir` (`dev.c:34`) — fill in a [`Dir`] for one file of a device.
///
/// Every field is Plan 9's:
///
/// * `type` is the device LETTER, from `devtab[c->type]->dc`.
/// * `dev` is `c->dev`, which instance it is.
/// * **`mode` is `perm | qid.type << 24`** — so `DMDIR` (0x80000000) is
///   `QTDIR` (0x80) moved up, and a device sets the directory bit once, in the
///   qid, rather than in two places that can disagree.
/// * `uid` and `muid` are the user, `gid` is `eve`.
///
/// `eve` is a kernel-wide global in Plan 9 (`auth.c:10`); in this kernel it is
/// `#c`'s, beside `/dev/hostowner` which writes it. A device that cannot reach
/// it passes the boot value, and that is the one place where renaming eve
/// would not show.
pub fn devdir(
    c: &Chan,
    qid: crate::ninep::Qid,
    name: &str,
    length: u64,
    user: &str,
    eve: &str,
    perm: u32,
) -> crate::ninep::Dir {
    crate::ninep::Dir {
        dtype: c.dev.letter() as u16,
        dev: c.devno,
        qid,
        mode: perm | ((qid.qtype as u32) << 24),
        // Plan 9 puts `seconds()` and `kerndate` here. This kernel's clock
        // reaches only `#c` (the machine has it), so a device that has no
        // clock reports zero rather than inventing a date.
        atime: 0,
        mtime: 0,
        length,
        name: name.to_string(),
        uid: user.to_string(),
        gid: eve.to_string(),
        muid: user.to_string(),
    }
}

/// `devdirread` (`dev.c:306`) — a directory read is a run of `Dir` entries,
/// packed by `convD2M`, stopping at the last one that fits WHOLE.
///
/// **An entry is never split**, which is what makes `c->dri` necessary: the
/// read answers fewer bytes than asked for, and the next read must resume at
/// the next ENTRY, not the next byte. `sysseek` resets `dri` and refuses any
/// offset but 0 on a directory (`sysfile.c:820`, `Eisdir`), so the index is
/// the whole of a directory's read position.
pub fn devdirread(c: &mut Chan, n: usize, entries: &[crate::ninep::Dir]) -> Vec<u8> {
    let mut out = Vec::new();
    while (c.dri as usize) < entries.len() {
        let b = entries[c.dri as usize].conv_d2m();
        if out.len() + b.len() > n {
            break;
        }
        out.extend_from_slice(&b);
        c.dri += 1;
    }
    out
}
