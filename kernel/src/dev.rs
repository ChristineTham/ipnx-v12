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
    /// `#t` — uart: serial lines, `eia0` and its `ctl` and `status` for each
    /// (`port/devuart.c`). The hardware behind one is a `PhysUart` the host
    /// supplies, as Plan 9's is the i8250 the PC has; its far end is the
    /// surface.
    Uart,
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
            DevId::Uart => 't',
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
            DevId::Uart => "uart",
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
            't' => DevId::Uart,
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
            DevId::Env, DevId::Cons, DevId::Cap, DevId::Virtio9p, DevId::Uart,
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

/// `devstat`'s answer for a directory its generator does not name
/// (`dev.c:281`) — which is every device directory: named for the last
/// element of the path it was reached by, `/` for the root and `???` for
/// none; eve's; `DMDIR|0555`.
pub fn devstatdir(c: &Chan, eve: &str) -> crate::ninep::Dir {
    let elem = if c.path.is_empty() {
        "???"
    } else if c.path == "/" {
        "/"
    } else {
        c.path.rsplit('/').next().unwrap_or(&c.path)
    };
    devdir(c, c.qid, elem, 0, eve, eve, crate::ninep::DMDIR | 0o555)
}

/// `atoi(s)`, which is `atol` (`libc/port/atol.c`): blanks and tabs, a
/// sign, then `0x` hexadecimal, a leading `0` octal, or decimal — the
/// digits up to the first that is not one. Nothing is 0.
pub fn atoi(s: &str) -> i32 {
    let b = s.as_bytes();
    let at = |i: usize| b.get(i).copied().unwrap_or(0);
    let mut p = 0;
    while at(p) == b' ' || at(p) == b'\t' {
        p += 1;
    }
    let mut neg = false;
    if at(p) == b'-' || at(p) == b'+' {
        neg = at(p) == b'-';
        p += 1;
        while at(p) == b' ' || at(p) == b'\t' {
            p += 1;
        }
    }
    let mut n: i32 = 0;
    if at(p) == b'0' && at(p + 1) != 0 {
        if at(p + 1) == b'x' || at(p + 1) == b'X' {
            p += 2;
            while let Some(v) = (at(p) as char).to_digit(16) {
                n = n.wrapping_mul(16).wrapping_add(v as i32);
                p += 1;
            }
        } else {
            while (b'0'..=b'7').contains(&at(p)) {
                n = n.wrapping_mul(8).wrapping_add((at(p) - b'0') as i32);
                p += 1;
            }
        }
    } else {
        while at(p).is_ascii_digit() {
            n = n.wrapping_mul(10).wrapping_add((at(p) - b'0') as i32);
            p += 1;
        }
    }
    if neg {
        n.wrapping_neg()
    } else {
        n
    }
}

/// `tokenize(s, args, maxargs)` (`libc/port/tokenize.c`): fields
/// separated by blanks, tabs, returns and newlines, where a quoted section
/// belongs to its field and a doubled quote inside one is a quote.
pub fn tokenize(s: &str) -> Vec<String> {
    let sep = |c: char| matches!(c, ' ' | '\t' | '\r' | '\n');
    let t: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        while i < t.len() && sep(t[i]) {
            i += 1;
        }
        if i >= t.len() {
            return out;
        }
        // `qtoken`
        let mut tok = String::new();
        let mut quoting = false;
        while i < t.len() && (quoting || !sep(t[i])) {
            if t[i] != '\'' {
                tok.push(t[i]);
                i += 1;
            } else if !quoting {
                quoting = true;
                i += 1;
            } else if t.get(i + 1) != Some(&'\'') {
                quoting = false;
                i += 1;
            } else {
                tok.push('\'');
                i += 2;
            }
        }
        out.push(tok);
    }
}

/// `parsecmd` (`parse.c:37`): a control message's fields — its last
/// newline dropped, then `tokenize`.
pub fn parsecmd(data: &[u8]) -> Vec<String> {
    let s = String::from_utf8_lossy(data);
    tokenize(s.strip_suffix('\n').unwrap_or(&s))
}

/// `cmderror` (`parse.c:74`): *"%s \""*, the fields as `%q`, *"\""* —
/// within `ERRMAX-10` bytes, as `up->genbuf` holds it.
pub fn cmderror(cb: &[String], s: &str) -> String {
    let mut out = format!("{s} \"");
    for (i, f) in cb.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&crate::proc::quote(f));
    }
    let max = crate::proc::ERRMAX - 10 - 1;
    if out.len() > max {
        let mut cut = max;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
    }
    out.push('"');
    out
}

/// `Ecmdargs` (`error.h:51`).
pub const ECMDARGS: &str = "wrong #args in control message";

/// `lookupcmd` (`parse.c:95`): the entry of `tab` the first field names,
/// or `*`'s, which matches anything — refused unless the message has its
/// count of fields (`0` is any); an unknown one is `cmderror`'s.
pub fn lookupcmd<T: Copy>(cb: &[String], tab: &[(T, &str, usize)]) -> Result<T, String> {
    if cb.is_empty() {
        return Err("empty control message".into());
    }
    for &(index, cmd, narg) in tab {
        if cmd != "*" && cmd != cb[0] {
            continue;
        }
        if narg != 0 && narg != cb.len() {
            return Err(cmderror(cb, ECMDARGS));
        }
        return Ok(index);
    }
    Err(cmderror(cb, "unknown control message"))
}

/// `strtoul(s, nil, 0)` (`libc/port/strtoul.c`), which the kernel links:
/// leading white space, a sign, then the base from the digits — `0x` is
/// hexadecimal, a leading `0` octal, anything else decimal — and the digits
/// up to the first that is not one. No digits is 0; overflow is
/// `ULONG_MAX`; a `-` negates. A `ulong` is 32 bits.
pub fn strtoul(s: &[u8]) -> u32 {
    let mut p = 0;
    while p < s.len() && matches!(s[p], b' ' | b'\t' | b'\n' | b'\x0c' | b'\r' | b'\x0b') {
        p += 1;
    }
    let mut neg = false;
    if p < s.len() && (s[p] == b'-' || s[p] == b'+') {
        neg = s[p] == b'-';
        p += 1;
    }
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    let base: u32 = if at(p) != b'0' {
        10
    } else if at(p + 1) == b'x' || at(p + 1) == b'X' {
        16
    } else {
        8
    };
    if base == 16 && at(p) == b'0' && (at(p + 1) == b'x' || at(p + 1) == b'X') && at(p + 2).is_ascii_hexdigit() {
        p += 2;
    }
    let (mut n, mut ovfl) = (0u32, false);
    let m = u32::MAX / base;
    loop {
        let c = at(p);
        let v = match c {
            b'0'..=b'9' => (c - b'0') as u32,
            b'a'..=b'z' => (c - b'a') as u32 + 10,
            b'A'..=b'Z' => (c - b'A') as u32 + 10,
            _ => base,
        };
        if v >= base {
            break;
        }
        if n > m {
            ovfl = true;
        }
        let nn = n.wrapping_mul(base).wrapping_add(v);
        if nn < n {
            ovfl = true;
        }
        n = nn;
        p += 1;
    }
    if ovfl {
        return u32::MAX;
    }
    if neg {
        return n.wrapping_neg();
    }
    n
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
    // *"if(c->flag&CMSG) qid.type |= QTMOUNT"* (`dev.c:37`).
    let mut qid = qid;
    if c.flag & crate::chan::flag::CMSG != 0 {
        qid.qtype |= crate::ninep::QTMOUNT;
    }
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

#[cfg(test)]
mod parse_tests {
    use super::*;

    /// `atoi` is `atol` (`libc/port/atol.c`): hexadecimal after `0x`,
    /// octal after a leading `0`, and the digits up to the first that is
    /// not one.
    #[test]
    fn atoi_is_atol() {
        assert_eq!(atoi("12"), 12);
        assert_eq!(atoi("  -7x"), -7);
        assert_eq!(atoi("0x1f"), 31);
        assert_eq!(atoi("010"), 8);
        assert_eq!(atoi("0"), 0);
        assert_eq!(atoi("none"), 0);
    }

    /// `strtoul(s, nil, 0)` (`libc/port/strtoul.c`).
    #[test]
    fn strtoul_reads_its_base_from_the_digits() {
        assert_eq!(strtoul(b"3\n"), 3);
        assert_eq!(strtoul(b" 0x10"), 16);
        assert_eq!(strtoul(b"017"), 15);
        assert_eq!(strtoul(b"/some/path"), 0);
        assert_eq!(strtoul(b"99999999999"), u32::MAX, "overflow");
    }

    /// `tokenize` (`libc/port/tokenize.c`): a quoted section is part of its
    /// field, and a doubled quote inside one is a quote.
    #[test]
    fn tokenize_keeps_a_quoted_field_whole() {
        assert_eq!(tokenize("  a  b\tc\n"), ["a", "b", "c"]);
        assert_eq!(tokenize("x 'a b' c"), ["x", "a b", "c"]);
        assert_eq!(tokenize("'it''s'"), ["it's"]);
        assert_eq!(tokenize("pre'mid'post"), ["premidpost"]);
        assert!(tokenize(" \t ").is_empty());
    }
}
