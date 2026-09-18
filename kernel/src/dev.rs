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

/// The device letters this kernel has: eight. Plan 9 has twenty-four.
///
/// The rule is Christine's — *"The kernel only handles process orchestration.
/// everything else is handled by host or userspace"* — so a letter earns its
/// place by being part of how processes are made, connected and named, and by
/// nothing else.
///
/// **What is absent: `#i` draw and `#m` mouse**, and only those two of the
/// ones a hosted kernel might want. Plan 9 has them because its kernel drives
/// hardware, and this one drives none. The host serves them — *"I have a
/// screen, a keyboard and a mouse. I will serve these as virtual devices to
/// the IPNX kernel"* — and the kernel reaches them the way it reaches
/// anything, by mounting what a server offers.
///
/// **`#c` is absent too, and for the rule rather than the hardware argument**
/// (Christine, 2026-09-18: *"i thought the kernel only handled process
/// orchestration, per rule"* — *"the rule wins"*). `consdir[]`
/// (`devcons.c:605`) is 23 files, and only `pid`, `ppid`, `pgrpid` and
/// `cputime` are orchestration; `sysname`, `osversion`, `drivers`, `kmesg`,
/// `swap`, `reboot`, `random`, `time`, `null` and `zero` are not, and
/// `cons`/`consctl` are the host's. Plan 9 keeps them because its kernel
/// drives the machine. This one does not, so they are served from userspace
/// and the host, and the four orchestration files are read from `#p`.
///
/// **`#¤` stays**: it changes a process's one `char *user` (`devcap.c:215`),
/// which is per-process state and nothing else's business.
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
            DevId::Cap => '\u{a4}',
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

    /// `attach(2)`'s device half: produce the channel that is this device's
    /// root. `spec` is the text after the letter, which most devices ignore.
    fn attach(&mut self, spec: &str) -> Result<Chan, String>;

    /// Walk one name. `None` means "no such file" — an ordinary answer, not an
    /// error, because that is how a union walk tries the next element.
    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Qid>, String>;

    fn open(&mut self, c: &mut Chan, mode: u16) -> Result<(), String>;
    fn create(&mut self, c: &mut Chan, name: &str, mode: u16, perm: u32) -> Result<(), String>;
    fn read(&mut self, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String>;
    fn write(&mut self, c: &mut Chan, data: &[u8], off: u64) -> Result<usize, String>;
    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String>;
    fn wstat(&mut self, c: &mut Chan, edir: &[u8]) -> Result<(), String>;
    fn remove(&mut self, c: &mut Chan) -> Result<(), String>;
    fn close(&mut self, c: &mut Chan);
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

    #[test]
    fn a_device_path_splits_at_the_letter() {
        assert_eq!(split("#s/store"), Some((DevId::Srv, "store")));
        assert_eq!(split("#p/1/ctl"), Some((DevId::Proc, "1/ctl")));
        assert_eq!(split("#|"), Some((DevId::Pipe, "")));
        assert_eq!(split("/srv/store"), None);
    }

    /// A guard on this kernel, after its author minted three letters in a day.
    /// `H`, `V` and `Z` are not Plan 9's at all; `i` and `m` are, and drive
    /// hardware this kernel does not have; `c` is Plan 9's and is 23 files of
    /// which four are orchestration — the rest is userspace's and the host's.
    #[test]
    fn a_letter_this_kernel_has_no_business_with_is_not_a_device() {
        for absent in ['H', 'V', 'Z', 'i', 'm', 'c'] {
            assert!(DevId::from_letter(absent).is_none(), "#{absent} must not be a device here");
        }
    }

    /// `#¤` is a wide character in Plan 9 too (`devcap.c:268`, `L'¤'`), so a
    /// byte-wise split would cut it in half.
    #[test]
    fn the_cap_device_splits_on_a_multibyte_letter() {
        assert_eq!(split("#\u{a4}/capuse"), Some((DevId::Cap, "capuse")));
        assert_eq!(split("#\u{a4}"), Some((DevId::Cap, "")));
        assert_eq!(split("#c/user"), None, "#c is not this kernel's");
    }

    #[test]
    fn every_letter_round_trips() {
        for d in [
            DevId::Root, DevId::Pipe, DevId::Srv, DevId::Mnt, DevId::Proc, DevId::Dup,
            DevId::Env, DevId::Cap,
        ] {
            assert_eq!(DevId::from_letter(d.letter()), Some(d));
        }
    }
}
