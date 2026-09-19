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
use crate::ninep::Qid;

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
    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Qid>, String>;

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

/// Split a device path into its device and the path below it: `#s/store` is
/// `Srv` and `store`. `None` if this is not a device path.
///
/// The letter is ONE character; anything after it belongs below.
pub fn split(path: &str) -> Option<(DevId, &str)> {
    let rest = path.strip_prefix('#')?;
    let mut chars = rest.char_indices();
    let (_, letter) = chars.next()?;
    let dev = DevId::from_letter(letter)?;
    let below = match chars.next() {
        Some((i, _)) => &rest[i..],
        None => "",
    };
    Some((dev, below.trim_start_matches('/')))
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
        assert_eq!(split("#s/store"), Some((DevId::Srv, "store")));
        assert_eq!(split("#p/1/ctl"), Some((DevId::Proc, "1/ctl")));
        assert_eq!(split("#|"), Some((DevId::Pipe, "")));
        assert_eq!(split("/srv/store"), None);
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
        assert_eq!(split("#\u{a4}/capuse"), Some((DevId::Cap, "capuse")));
        assert_eq!(split("#\u{a4}"), Some((DevId::Cap, "")));
        assert_eq!(split("#c/user"), Some((DevId::Cons, "user")));
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

/// `eve` at boot (`auth.c:10`: `char *eve = "bootes"` — here, the name the
/// host gives `#c`). A device that cannot reach `#c` uses this for the group
/// of the files it serves, which is what Plan 9's kernel-wide `eve` holds.
pub const EVE: &str = "eve";

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
