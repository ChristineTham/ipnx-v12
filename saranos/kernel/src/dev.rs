//! The device table — the file interface as function calls.
//!
//! Inside the kernel a device is not a 9P server: it is a table of functions,
//! and a walk or a read is a call. Wire 9P exists at exactly one boundary, the
//! mount driver, and nowhere else. This is the shape Plan 9's own kernel has,
//! and it is why the system can have one protocol without paying to marshal it
//! against itself.
//!
//! **A device exists here only if Plan 9 has it, means the same thing by it,
//! and spells it with the same letter.** The letters are not ours to choose. A
//! device that Plan 9 lacks is not a device: it is a userspace file server,
//! which is what Plan 9 would have made it.

/// Plan 9's device letters, and the one exception.
///
/// The exception is `Z`, host files, and it has a reason no other candidate can
/// borrow: a Plan 9 kernel reaches a disk through `#S`, and a hosted kernel has
/// no disk — the storage it has belongs to the process it runs inside. `#Z` is
/// that boundary made a device, so it is reached by walking a name like
/// everything else rather than by a special call. The exception is the MACHINE
/// being different, not the system, and it is a single boundary rather than a
/// licence.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DevId {
    /// `#/` — the root device: a fixed table of empty mount points.
    Root,
    /// `#c` — cons: the console, and the machine's own facts.
    Cons,
    /// `#e` — env: the environment as files.
    Env,
    /// `#d` — dup: the process's own file descriptors as files.
    Dup,
    /// `#p` — proc: processes as directories.
    Proc,
    /// `#s` — srv: a posted channel, kept alive by name.
    Srv,
    /// `#|` — pipe: an attach mints two cross-connected ends.
    Pipe,
    /// `#M` — mnt: the mount driver, and the only place wire 9P is spoken.
    Mnt,
    /// `#Z` — hostfs. The exception, argued above.
    Host,
}

impl DevId {
    /// The letter, as it appears in a path. `#c/user` is cons's `user` file.
    pub fn letter(self) -> char {
        match self {
            DevId::Root => '/',
            DevId::Cons => 'c',
            DevId::Env => 'e',
            DevId::Dup => 'd',
            DevId::Proc => 'p',
            DevId::Srv => 's',
            DevId::Pipe => '|',
            DevId::Mnt => 'M',
            DevId::Host => 'Z',
        }
    }

    pub fn from_letter(c: char) -> Option<DevId> {
        Some(match c {
            '/' => DevId::Root,
            'c' => DevId::Cons,
            'e' => DevId::Env,
            'd' => DevId::Dup,
            'p' => DevId::Proc,
            's' => DevId::Srv,
            '|' => DevId::Pipe,
            'M' => DevId::Mnt,
            'Z' => DevId::Host,
            _ => return None,
        })
    }
}

/// Split a device path into its device and the path below it: `#Z/store/python`
/// is `Host` and `store/python`. Returns `None` if this is not a device path.
///
/// The letter is ONE character. Everything after it is a path within the
/// device, so a device with a name — `#M` with a mount spec — takes it below,
/// not in the letter.
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
        assert_eq!(split("#Z/store/python"), Some((DevId::Host, "store/python")));
        assert_eq!(split("#c/user"), Some((DevId::Cons, "user")));
        assert_eq!(split("#|"), Some((DevId::Pipe, "")));
    }

    #[test]
    fn an_ordinary_path_is_not_a_device_path() {
        assert_eq!(split("/store/python"), None);
        assert_eq!(split("store"), None);
    }

    #[test]
    fn an_unknown_letter_is_not_a_device() {
        // A letter Plan 9 does not use is not ours to mint.
        assert_eq!(split("#Q/anything"), None);
        assert_eq!(split("#H/http"), None);
    }

    #[test]
    fn every_letter_round_trips() {
        for d in [
            DevId::Root, DevId::Cons, DevId::Env, DevId::Dup, DevId::Proc,
            DevId::Srv, DevId::Pipe, DevId::Mnt, DevId::Host,
        ] {
            assert_eq!(DevId::from_letter(d.letter()), Some(d));
        }
    }
}
