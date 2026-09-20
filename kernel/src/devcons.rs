//! `#c` — cons (`plan9/sys/src/9/port/devcons.c`).
//!
//! **A reporting device.** It holds no machinery of its own: every file is a
//! name for state that already exists, either in the kernel's own tables or in
//! the host. Christine, 2026-09-18: *"we should keep `#c` in since it holds a
//! variety of kernel info"*, and *"just because the functionality is provided
//! by host app doesn't mean it can't be visible and reported by kernel"*.
//!
//! Plan 9 splits the same way — `port/devcons.c` formats, and the numbers come
//! from the architecture: `todget` for the clock, `randomread` for entropy,
//! `conf`/`palloc` for memory. Here that split is the [`Machine`] trait, which
//! the host implements.
//!
//! `consdir[]` (`devcons.c:606`) is 23 files. All of them are here.

use crate::chan::Chan;
use crate::dev::{Dev, DevId};
use crate::ninep::{Qid, QTDIR, QTEXCL};
use crate::proc::Up;
#[cfg(test)]
use crate::proc::Procs;
use std::cell::RefCell;
use std::rc::Rc;

/// `NUMSIZE` — `portdat.h:800`, *"size of formatted number"*. A number is
/// written right-aligned in `NUMSIZE-1` columns with a trailing space, so
/// every one is exactly 12 bytes (`readnum`, `devcons.c:633`).
pub const NUMSIZE: usize = 12;
/// `VLNUMSIZE` — `devcons.c:603`.
pub const VLNUMSIZE: usize = 22;

/// `readnum`, verbatim in behaviour: `%*lud` at `size-1`, then a space.
pub fn readnum(val: u64, size: usize) -> Vec<u8> {
    let mut s = format!("{:>width$}", val, width = size - 1);
    s.push(' ');
    s.into_bytes()
}

/// What the host reports, so the kernel can name it. Plan 9 asks its
/// architecture for exactly these.
pub trait Console {
    /// `screenputs` — `devcons.c:12` declares it as
    /// `void (*screenputs)(char*, int) = nil`, a function pointer the
    /// architecture fills in, and `putstrn0` calls it. This is that pointer.
    fn putstrn(&mut self, s: &[u8]);

    /// What the keyboard has produced, which Plan 9 gets asynchronously:
    /// `kbdputc` (`devcons.c:525`) is called at interrupt time, stages the
    /// runes and `qproduce`s them into `kbdq`; `consread` then BLOCKS in
    /// `qread(kbdq, &ch, 1)` until there are some.
    ///
    /// **This machine has no interrupts**, so nothing can fill a queue behind
    /// the kernel's back. The blocking read is therefore a call outward, made
    /// at the same point in `consread` where Plan 9 blocks, with the same
    /// meaning: return when there is input. An empty answer is end of input —
    /// the terminal is gone — and `consread` treats it as the `^D` that Plan
    /// 9's user would have typed.
    fn kbdchars(&mut self) -> Vec<u8>;

    /// `todget` — nanoseconds since the epoch, and the fast-tick counter.
    fn now(&mut self) -> (u64, u64, u64);
    /// `randomread`.
    fn random(&mut self, n: usize) -> Vec<u8>;
    /// What the host itself provides, one per line. Reported beside the
    /// kernel's own device letters in `/dev/drivers`.
    fn drivers(&mut self) -> Vec<String>;
    /// The memory numbers `/dev/swap` reports: total bytes, page size, bytes
    /// in use.
    fn memory(&mut self) -> (u64, u64, u64);
    /// `configfile[]` (`port/portmkfile:53`) — **the kernel configuration
    /// file, verbatim**: `$CONF`, the file `mkdevc` turns into `devtab[]`,
    /// embedded in the kernel image and read back at `/dev/config`
    /// (`devcons.c:871`). Whoever builds the device table owns it, which is
    /// the machine here.
    ///
    /// **Not the boot arguments.** Those are `plan9.ini`, which the
    /// bootloader leaves at `BOOTARGS` for `options()` to parse
    /// (`pc/main.c:66`) and which reaches userspace through `#ec`.
    fn config(&mut self) -> String;
    /// `/dev/reboot`: `halt`, or `reboot <path>`.
    fn reboot(&mut self, cmd: &str) -> Result<(), String>;
}

/// The device's own state — what Plan 9 keeps in globals beside `devcons.c`:
/// `eve` (`auth.c:10`), `hostdomain` (`auth.c:11`), `sysname`, the kernel log.
pub struct Cons {
    /// `kbd` (`devcons.c:23`) — the console's own state, and the whole of the
    /// line discipline. It is PORTABLE code in Plan 9, in `port/devcons.c`,
    /// not in any architecture directory: what a backspace does is not the
    /// machine's business.
    kbd: Kbd,
    eve: crate::dev::Eve,
    pub hostdomain: String,
    pub sysname: String,
    pub kmesg: Vec<u8>,
    /// `Mach.syscall` and `Mach.cs` (`pc/dat.h:233`), the two counters this
    /// kernel is in a position to keep honestly.
    pub syscalls: u64,
    /// `kprintinuse` (`devcons.c`) — the one-reader lock `consopen` takes
    /// with `tas` and `consclose` releases.
    kprintinuse: bool,
    pub cs: u64,
    /// The device letters this kernel carries, for `/dev/drivers`. The names
    /// come from [`DevId::name`], so there is one list and not two.
    letters: Vec<DevId>,
    /// How a device reaches `up`. Plan 9 uses a per-machine global; this is
    /// the same pointer, shared rather than ambient because Rust has no
    /// ambient mutable global.
    up: Rc<RefCell<Up>>,
    host: Box<dyn Console>,
}

/// `kbd` (`devcons.c:23`), with the fields this kernel has something to do
/// with. The staging buffer is absent because it exists to amortise the cost
/// of `qproduce` at interrupt time, and there are no interrupts here.
#[derive(Default)]
struct Kbd {
    /// *"true if we shouldn't process input"*.
    raw: bool,
    /// `Ref ctl` — *"number of opens to the control file"*. The LAST close of
    /// `consctl` turns raw off (`consclose`, `devcons.c:727`).
    ctl: u32,
    /// `x` and `line[1024]` — the line being edited.
    line: Vec<u8>,
    ctlpoff: bool,
    /// `kbdq` — *"unprocessed console input"* (`devcons.c:14`).
    kbdq: std::collections::VecDeque<u8>,
    /// `lineq` — *"processed console input"* (`devcons.c:15`). What `Qcons`
    /// reads from, and the reason a read answers whole lines.
    lineq: std::collections::VecDeque<u8>,
    /// Set once the machine says there is no more input. Plan 9 has no
    /// counterpart because a keyboard does not end.
    eof: bool,
}

/// `kbd.line`'s size (`devcons.c:29`). A line this long is sent whether or not
/// it has been ended.
const KBDLINE: usize = 1024;

/// Qids, in `consdir[]`'s order (`devcons.c:606`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u64)]
pub enum Q {
    Dir = 0,
    Bintime,
    Cons,
    Consctl,
    Cputime,
    Drivers,
    Hostdomain,
    Hostowner,
    Kmesg,
    Kprint,
    Null,
    Osversion,
    Pgrpid,
    Pid,
    Ppid,
    Random,
    Reboot,
    Swap,
    Sysname,
    Sysstat,
    Time,
    User,
    Zero,
    Config,
}

/// Name, qid, **qid TYPE** and permission — `consdir[]` exactly
/// (`devcons.c:606`). The type column is there for one entry:
/// `"kprint", {Qkprint, 0, QTEXCL}, 0, DMEXCL|0440` (`:616`). `devdir` sets
/// `mode = perm | qid.type<<24` and `DMEXCL` is `QTEXCL << 24`, so the one
/// bit in the qid answers both — the table needs no `DMEXCL` of its own.
pub const CONSDIR: &[(&str, Q, u8, u32)] = &[
    ("bintime", Q::Bintime, 0, 0o664),
    ("cons", Q::Cons, 0, 0o660),
    ("consctl", Q::Consctl, 0, 0o220),
    ("cputime", Q::Cputime, 0, 0o444),
    ("drivers", Q::Drivers, 0, 0o444),
    ("hostdomain", Q::Hostdomain, 0, 0o664),
    ("hostowner", Q::Hostowner, 0, 0o664),
    ("kmesg", Q::Kmesg, 0, 0o440),
    ("kprint", Q::Kprint, QTEXCL, 0o440),
    ("null", Q::Null, 0, 0o666),
    ("osversion", Q::Osversion, 0, 0o444),
    ("pgrpid", Q::Pgrpid, 0, 0o444),
    ("pid", Q::Pid, 0, 0o444),
    ("ppid", Q::Ppid, 0, 0o444),
    ("random", Q::Random, 0, 0o444),
    ("reboot", Q::Reboot, 0, 0o660),
    ("swap", Q::Swap, 0, 0o664),
    ("sysname", Q::Sysname, 0, 0o664),
    ("sysstat", Q::Sysstat, 0, 0o666),
    ("time", Q::Time, 0, 0o664),
    ("user", Q::User, 0, 0o666),
    ("zero", Q::Zero, 0, 0o444),
    ("config", Q::Config, 0, 0o444),
];

impl Q {
    fn from_path(p: u64) -> Option<Q> {
        // **`Qdir` is the directory itself, and it is not in the table.**
        // Plan 9's `consdir[]` has `"."` as its zeroth entry (`devcons.c:607`)
        // and `devgen` skips it; this table holds only the files, so the
        // directory has to be answered here. Without this, reading `#c`
        // itself was "no such file" — and `ls /dev` printed nothing, which
        // looked like an empty directory rather than a failure.
        if p == Q::Dir as u64 {
            return Some(Q::Dir);
        }
        CONSDIR.iter().map(|e| e.1).find(|q| *q as u64 == p)
    }
    fn name(self) -> &'static str {
        CONSDIR.iter().find(|e| e.1 == self).map(|e| e.0).unwrap_or(".")
    }
    fn perm(self) -> u32 {
        CONSDIR.iter().find(|e| e.1 == self).map(|e| e.3).unwrap_or(0o555)
    }
}

const EPERM: &str = "permission denied";
/// `Einuse` (`error.h`) — *"device or object already in use"*.
const EINUSE: &str = "device or object already in use";
const EBADARG: &str = "bad arg in system call";

impl Cons {
    pub fn new(
        eve: crate::dev::Eve,
        up: Rc<RefCell<Up>>,
        letters: Vec<DevId>,
        host: Box<dyn Console>,
    ) -> Cons {
        Cons {
            kbd: Kbd::default(),
            eve,
            hostdomain: String::new(),
            sysname: String::new(),
            kmesg: Vec::new(),
            syscalls: 0,
            kprintinuse: false,
            cs: 0,
            letters,
            up,
            host,
        }
    }

    /// `putstrn0` (`devcons.c:144`) — every route a console write takes.
    /// `kmesgputs` first, so the kernel's log holds it, then the screen.
    fn putstrn(&mut self, s: &[u8]) {
        // `kmesgputs(str, n)` (`devcons.c:155`) — the kernel's log holds
        // everything that reaches the console, which is why `/dev/kmesg`
        // survives a screen that was not there to read.
        self.kmesg.extend_from_slice(s);
        // Plan 9 copies through a 256-byte buffer *"Can't page fault in
        // putstrn"* (`conswrite`, `devcons.c:993`). There are no page faults
        // here and no reason to chop the bytes up.
        self.host.putstrn(s);
    }

    /// `consread`'s line discipline (`devcons.c:762`), which is portable code
    /// in Plan 9 and portable here: characters come off `kbdq` one at a time,
    /// and a line goes onto `lineq` when it is ended.
    ///
    /// In raw mode every character ends the line, which is how a program that
    /// wants keystrokes gets them.
    fn linedisc(&mut self) {
        while self.kbd.lineq.is_empty() {
            if self.kbd.kbdq.is_empty() {
                if self.kbd.eof {
                    return;
                }
                let got = self.host.kbdchars();
                if got.is_empty() {
                    // End of input. Plan 9 never reaches this — a keyboard
                    // does not end — so it is treated as the `^D` its user
                    // would have typed: send what there is, and the empty
                    // line after it is the end-of-file a reader sees.
                    self.kbd.eof = true;
                    self.sendline();
                    return;
                }
                self.kbd.kbdq.extend(got);
            }
            let ch = match self.kbd.kbdq.pop_front() {
                Some(ch) => ch,
                None => return,
            };
            let mut send = false;
            if ch == 0 {
                // *"flush output on rawoff -> rawon"* (`devcons.c:772`).
                if !self.kbd.line.is_empty() {
                    send = self.kbd.kbdq.is_empty();
                }
            } else if self.kbd.raw {
                self.kbd.line.push(ch);
                send = self.kbd.kbdq.is_empty();
            } else {
                match ch {
                    0x08 => {
                        self.kbd.line.pop();
                    }
                    0x15 => self.kbd.line.clear(), // ^U
                    b'\n' | 0x04 => {
                        // ^D ends the line and is NOT part of it, which is
                        // what makes an empty line mean end of file.
                        if ch != 0x04 {
                            self.kbd.line.push(ch);
                        }
                        send = true;
                    }
                    _ => self.kbd.line.push(ch),
                }
            }
            if send || self.kbd.line.len() == KBDLINE {
                self.sendline();
            }
        }
    }

    /// `qwrite(lineq, kbd.line, kbd.x); kbd.x = 0;`
    fn sendline(&mut self) {
        let line = std::mem::take(&mut self.kbd.line);
        self.kbd.lineq.extend(line);
    }

    /// `conswrite`'s `Qconsctl` (`devcons.c:1008`): space-separated words,
    /// and the four it knows.
    fn consctl(&mut self, s: &str) {
        for word in s.split(' ') {
            if word.starts_with("rawon") {
                self.kbd.raw = true;
                // *"clumsy hack - wake up reader"* — a zero byte into `kbdq`,
                // which the discipline above treats as a flush.
                self.kbd.kbdq.push_back(0);
            } else if word.starts_with("rawoff") {
                self.kbd.raw = false;
            } else if word.starts_with("ctlpon") {
                self.kbd.ctlpoff = false;
            } else if word.starts_with("ctlpoff") {
                self.kbd.ctlpoff = true;
            }
        }
    }

    /// `iseve()` — `strcmp(eve, up->user) == 0` (`auth.c:17`). A NAME
    /// comparison, not a bit.
    fn iseve(&self) -> bool {
        crate::dev::iseve(&self.eve, &self.user())
    }

    fn user(&self) -> String {
        let (pid, procs) = {
            let up = self.up.borrow();
            (up.pid, up.procs.clone())
        };
        let u = procs.borrow().user(pid);
        u.unwrap_or_default()
    }

    /// `readstr` — the read honours the offset into a generated string.
    fn readstr(s: &str, n: usize, off: u64) -> Vec<u8> {
        let b = s.as_bytes();
        let off = off as usize;
        if off >= b.len() {
            return Vec::new();
        }
        b[off..(off + n).min(b.len())].to_vec()
    }
}

impl Dev for Cons {
    fn id(&self) -> DevId {
        DevId::Cons
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn attach(&mut self, _spec: &str) -> Result<Chan, String> {
        Ok(Chan::attach(DevId::Cons, 0))
    }

    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Chan>, String> {
        if c.qid.path != Q::Dir as u64 {
            return Err("not a directory".into());
        }
        if name == ".." || name == "." {
            return Ok(Some(c.walked(name, Qid { qtype: QTDIR, vers: 0, path: Q::Dir as u64 })));
        }
        Ok(CONSDIR
            .iter()
            .find(|e| e.0 == name)
            .map(|e| c.walked(name, Qid { qtype: e.2, vers: 0, path: e.1 as u64 })))
    }

    /// `consopen` (`devcons.c:692`) — one file needs to know it was opened.
    fn open(&mut self, mut c: Chan, mode: u16) -> Result<Chan, String> {
        // `devopen`'s tail (`dev.c`): *"c->offset = 0; c->mode =
        // openmode(omode); c->flag |= COPEN;"*. `close` acts on that bit,
        // so a device that does not set it has a `close` that never fires.
        c.offset = 0;
        c.mode = mode;
        c.flag |= crate::chan::flag::COPEN;
        match Q::from_path(c.qid.path) {
            Some(Q::Consctl) => {
                // `incref(&kbd.ctl)`.
                self.kbd.ctl += 1;
            }
            // `consopen` (`devcons.c`): *"if(tas(&kprintinuse) != 0){
            // c->flag &= ~COPEN; error(Einuse); }"*. `kprint` drains the
            // kernel's log to ONE reader — two would each get part of it —
            // which is what `DMEXCL` on the file announces.
            Some(Q::Kprint) => {
                if self.kprintinuse {
                    return Err(EINUSE.into());
                }
                self.kprintinuse = true;
            }
            _ => {}
        }
        Ok(c)
    }

    fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
        Err(EPERM.into())
    }

    fn read(&mut self, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String> {
        let q = Q::from_path(c.qid.path).ok_or("no such file")?;
        // **A file ends.** `readnum` and `readstr` both take the offset and
        // answer 0 past the end (`devcons.c:640`, `:652`); the numbers here
        // were formatted and handed back whole however far in the reader
        // was, so `cat /dev/pid` printed the pid for ever. The generators and
        // the console are the exception: they are streams, and Plan 9's
        // `consread` gives them no offset either.
        let stream = matches!(q, Q::Random | Q::Zero | Q::Null | Q::Cons | Q::Dir);
        let got = match q {
            // `up`'s own numbers.
            Q::Pid => readnum(self.up.borrow().pid as u64, NUMSIZE),
            Q::Ppid => {
                let up = self.up.borrow();
                let ppid = up.procs.borrow().ppid(up.pid).unwrap_or(0);
                readnum(ppid as u64, NUMSIZE)
            }
            // `up->pgrp->pgrpid` (`devcons.c:99`) — the NAMESPACE group's
            // number. `struct Pgrp` holds `mnthash[]` and nothing about
            // signals or job control, so this is not a process group in the
            // Unix sense at all.
            Q::Pgrpid => {
                let (pid, procs) = {
                    let up = self.up.borrow();
                    (up.pid, up.procs.clone())
                };
                let id = procs.borrow().pgrpid(pid).unwrap_or(0);
                readnum(id as u64, NUMSIZE)
            }
            // Six numbers, `NUMSIZE` each (`devcons.c:63`), in milliseconds.
            Q::Cputime => {
                let (nsec, _, _) = self.host.now();
                let (pid, procs) = {
                    let up = self.up.borrow();
                    (up.pid, up.procs.clone())
                };
                let t = procs.borrow().cputime(pid, nsec);
                t.iter().flat_map(|v| readnum(*v, NUMSIZE)).collect()
            }

            // identity
            Q::User => Self::readstr(&self.user(), n, off),
            Q::Hostowner => Self::readstr(&self.eve.borrow().clone(), n, off),
            Q::Hostdomain => Self::readstr(&self.hostdomain.clone(), n, off),

            // the kernel itself
            Q::Sysname => Self::readstr(&self.sysname.clone(), n, off),
            Q::Osversion => Self::readstr("2000", n, off),
            Q::Kmesg | Q::Kprint => {
                let off = off as usize;
                if off >= self.kmesg.len() {
                    Vec::new()
                } else {
                    self.kmesg[off..(off + n).min(self.kmesg.len())].to_vec()
                }
            }
            // `#%C %s\n` per device (`devcons.c:198`), then the host's own.
            Q::Drivers => {
                let mut s = String::new();
                for d in &self.letters {
                    s.push_str(&format!("#{} {}\n", d.letter(), d.name()));
                }
                for d in self.host.drivers() {
                    s.push_str(&d);
                    s.push('\n');
                }
                Self::readstr(&s, n, off)
            }
            Q::Config => {
                let s = self.host.config();
                Self::readstr(&s, n, off)
            }

            // the clock — `secs nanosecs fastticks fasthz mono`
            Q::Time => {
                let (nsec, ticks, hz) = self.host.now();
                let mut s = String::new();
                s.push_str(&String::from_utf8_lossy(&readnum(nsec / 1_000_000_000, NUMSIZE)));
                for v in [nsec, ticks, hz, nsec] {
                    s.push_str(&String::from_utf8_lossy(&readnum(v, VLNUMSIZE)));
                }
                Self::readstr(&s, n, off)
            }
            // big-endian: nsec, ticks, fasthz, mono (`readbintime`)
            Q::Bintime => {
                let (nsec, ticks, hz) = self.host.now();
                let mut b = Vec::new();
                for v in [nsec, ticks, hz, nsec] {
                    b.extend_from_slice(&v.to_be_bytes());
                }
                b.truncate(n - n % 8);
                b
            }

            // memory, as `/dev/swap` reports it
            Q::Swap => {
                let (mem, pagesize, used) = self.host.memory();
                let s = format!(
                    "{mem} memory\n{pagesize} pagesize\n0 kernel\n{used}/{mem} user\n\
                     0/0 swap\n{used}/{mem} kernel malloc\n0/0 kernel draw\n"
                );
                Self::readstr(&s, n, off)
            }
            // Per-processor counters (`devcons.c:129`), one line per machine:
            // id, context switches, interrupts, syscalls, page faults, tlb
            // faults, tlb purges, load. This kernel has one machine and
            // counts what it actually does; the rest stay zero rather than
            // being invented.
            Q::Sysstat => {
                let mut s = String::new();
                for v in [0, self.cs, 0, self.syscalls, 0, 0, 0, 0] {
                    s.push_str(&String::from_utf8_lossy(&readnum(v, NUMSIZE)));
                }
                s.push('\n');
                Self::readstr(&s, n, off)
            }

            // generators
            Q::Random => self.host.random(n),
            Q::Zero => vec![0u8; n],
            Q::Null => Vec::new(),

            // `consread`'s `Qcons` (`devcons.c:762`): run the line
            // discipline until there is a processed line, then take from
            // `lineq`. A read answers a WHOLE line and no more, which is why
            // a shell gets a command rather than a character.
            //
            // An empty answer is end of file, and it is reached exactly where
            // Plan 9 would have a `^D`: the line before it was sent, and this
            // one is empty.
            Q::Cons => {
                self.linedisc();
                let take = n.min(self.kbd.lineq.len());
                self.kbd.lineq.drain(..take).collect()
            }
            // `consctl` is 0220 — write-only, and a read of it is `Eperm`
            // as it is of anything else this device will not read
            // (`consread`'s default, `devcons.c:968`).
            Q::Consctl => return Err(EPERM.into()),
            Q::Reboot => return Err(EPERM.into()),
            // `devdirread` over `consdir[]` — the twenty-three files, in the
            // table's own order (`devcons.c:606`). Plan 9's table has "."
            // first and `devgen` skips it; this one holds only the files, so
            // there is nothing to skip.
            Q::Dir => {
                let user = self.up.borrow().user();
                let eve = self.eve.borrow().clone();
                let entries: Vec<crate::ninep::Dir> = CONSDIR
                    .iter()
                    .map(|(name, q, qtype, perm)| {
                        let qid = Qid { qtype: *qtype, vers: 0, path: *q as u64 };
                        crate::dev::devdir(c, qid, name, 0, &user, &eve, *perm)
                    })
                    .collect();
                return Ok(crate::dev::devdirread(c, n, &entries));
            }
        };
        if stream {
            return Ok(got);
        }
        let off = off as usize;
        if off >= got.len() {
            return Ok(Vec::new());
        }
        Ok(got[off..(off + n).min(got.len())].to_vec())
    }

    fn write(&mut self, c: &mut Chan, data: &[u8], _off: u64) -> Result<usize, String> {
        let q = Q::from_path(c.qid.path).ok_or("no such file")?;
        let s = String::from_utf8_lossy(data).trim_end_matches('\n').to_string();
        match q {
            // `userwrite` (`auth.c:107`): the four bytes "none", and nothing
            // else. *"anyone can become none"*, and there is no way back.
            Q::User => {
                if s != "none" {
                    return Err(EPERM.into());
                }
                let up = self.up.borrow();
                up.procs.borrow_mut().setuser(up.pid, "none");
            }
            // `hostownerwrite` (`auth.c:126`): eve only, and it renames eve
            // AND every process owned by the old name (`renameuser`,
            // `proc.c:1601`).
            Q::Hostowner => {
                if !self.iseve() {
                    return Err(EPERM.into());
                }
                if s.is_empty() {
                    return Err(EBADARG.into());
                }
                let old = self.eve.borrow().clone();
                let up = self.up.borrow();
                up.procs.borrow_mut().renameuser(&old, &s);
                *self.eve.borrow_mut() = s;
            }
            Q::Hostdomain => {
                if !self.iseve() {
                    return Err(EPERM.into());
                }
                if s.is_empty() {
                    return Err(EBADARG.into());
                }
                self.hostdomain = s;
            }
            Q::Sysname => {
                if s.is_empty() {
                    return Err(EBADARG.into());
                }
                self.sysname = s;
            }
            Q::Reboot => {
                if !self.iseve() {
                    return Err(EPERM.into());
                }
                self.host.reboot(&s)?;
            }
            Q::Kmesg | Q::Kprint => self.kmesg.extend_from_slice(data),
            // `/dev/null` swallows, and that is its whole job.
            Q::Null => {}
            // `conswrite`'s Qsysstat zeroes the counters (`devcons.c:107`).
            Q::Sysstat => {
                self.syscalls = 0;
                self.cs = 0;
            }
            Q::Swap | Q::Time | Q::Bintime => {}
            // `conswrite`'s `Qcons` (`devcons.c:992`).
            Q::Cons => self.putstrn(data),
            Q::Consctl => {
                let s = String::from_utf8_lossy(data).to_string();
                self.consctl(&s);
            }
            _ => return Err(EPERM.into()),
        }
        Ok(data.len())
    }

    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        let q = Q::from_path(c.qid.path).unwrap_or(Q::Dir);
        let (name, perm) = if q == Q::Dir {
            ("#c", crate::ninep::DMDIR | 0o555)
        } else {
            (q.name(), q.perm())
        };
        let user = self.up.borrow().user();
        Ok(crate::dev::devdir(c, c.qid, name, 0, &user, &self.eve.borrow().clone(), perm).conv_d2m())
    }

    fn wstat(&mut self, _c: &mut Chan, _e: &[u8]) -> Result<(), String> {
        Err(EPERM.into())
    }

    fn remove(&mut self, _c: &mut Chan) -> Result<(), String> {
        Err(EPERM.into())
    }

    /// `consclose` (`devcons.c:722`) — *"last close of control file turns off
    /// raw"*. A program that set raw mode and died must not leave the console
    /// in it, and nothing else would put it back.
    fn close(&mut self, c: &mut Chan) {
        if c.flag & crate::chan::flag::COPEN == 0 {
            return;
        }
        match Q::from_path(c.qid.path) {
            Some(Q::Consctl) => {
                self.kbd.ctl = self.kbd.ctl.saturating_sub(1);
                if self.kbd.ctl == 0 {
                    self.kbd.raw = false;
                }
            }
            // `consclose`: *"case Qkprint: if(c->flag & COPEN){ kprintinuse
            // = 0; ... }"*. The next open gets it.
            Some(Q::Kprint) => self.kprintinuse = false,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chan::mode::{OREAD, OWRITE};
    use crate::dev::DevId;

    /// The machine's terminal, as a test can inspect it: what the kernel put
    /// on the screen, and what the keyboard will produce, one answer per
    /// `kbdchars` call.
    #[derive(Default)]
    struct Term {
        out: Vec<u8>,
        keys: std::collections::VecDeque<Vec<u8>>,
    }

    /// A host that reports fixed numbers, so a test can tell what came from
    /// where. The kernel computes none of this — it names it.
    #[derive(Default, Clone)]
    struct FakeHost(Rc<RefCell<Term>>);

    impl Console for FakeHost {
        fn putstrn(&mut self, s: &[u8]) {
            self.0.borrow_mut().out.extend_from_slice(s);
        }
        fn kbdchars(&mut self) -> Vec<u8> {
            self.0.borrow_mut().keys.pop_front().unwrap_or_default()
        }
        fn now(&mut self) -> (u64, u64, u64) {
            (1_500_000_000_000_000_000, 42, 1_000_000)
        }
        fn random(&mut self, n: usize) -> Vec<u8> {
            vec![7u8; n]
        }
        fn drivers(&mut self) -> Vec<String> {
            vec!["host screen".into(), "host keyboard".into()]
        }
        fn memory(&mut self) -> (u64, u64, u64) {
            (1024 * 1024, 4096, 512 * 1024)
        }
        fn config(&mut self) -> String {
            "ipnx userspace/rootfs\n".into()
        }
        fn reboot(&mut self, cmd: &str) -> Result<(), String> {
            if cmd == "halt" {
                Ok(())
            } else {
                Err("bad reboot".into())
            }
        }
    }

    fn cons() -> (Cons, Rc<RefCell<Procs>>) {
        let procs = Rc::new(RefCell::new(Procs::new(Chan::attach(DevId::Root, 0))));
        // The running system starts `eve` empty (`pc/main.c:285`) and has
        // `boot` name the host owner by writing `#c/hostowner`
        // (`bootauth.c:56`). A unit test has no boot, so it names one.
        procs.borrow_mut().get_mut(1).unwrap().user = "eve".into();
        let up = Rc::new(RefCell::new(Up { pid: 1, procs: procs.clone() }));
        let letters = vec![DevId::Root, DevId::Pipe, DevId::Cons];
        (Cons::new(eve_(), up, letters, Box::new(FakeHost::default())), procs)
    }

    /// A console whose terminal the test still holds.
    fn cons_term(keys: &[&str]) -> (Cons, FakeHost) {
        let procs = Rc::new(RefCell::new(Procs::new(Chan::attach(DevId::Root, 0))));
        let up = Rc::new(RefCell::new(Up { pid: 1, procs }));
        let host = FakeHost::default();
        host.0.borrow_mut().keys = keys.iter().map(|k| k.as_bytes().to_vec()).collect();
        (Cons::new(eve_(), up, vec![DevId::Cons], Box::new(host.clone())), host)
    }

    fn open(d: &mut Cons, name: &str, mode: u16) -> Chan {
        let dir = d.attach("").unwrap();
        let c = d.walk(&dir, name).unwrap().expect(name);
        d.open(c, mode).unwrap()
    }

    /// The fixture's host owner. The running system starts `eve` empty and
    /// has `boot` write `#c/hostowner` (`bootauth.c:56`); a unit test wants
    /// somebody there already.
    fn eve_() -> crate::dev::Eve {
        crate::dev::Eve::new(std::cell::RefCell::new("eve".to_string()))
    }

    fn read(d: &mut Cons, name: &str) -> String {
        let mut c = open(d, name, OREAD);
        let got = String::from_utf8_lossy(&d.read(&mut c, 4096, 0).unwrap()).to_string();
        d.close(&mut c);
        got
    }

    fn write(d: &mut Cons, name: &str, s: &str) -> Result<usize, String> {
        let mut c = open(d, name, OWRITE);
        let got = d.write(&mut c, s.as_bytes(), 0);
        d.close(&mut c);
        got
    }

    /// All 23 of `consdir[]` (`devcons.c:606`) are here, and walk to a qid.
    #[test]
    fn every_file_plan_nine_has_is_here() {
        let (mut d, _) = cons();
        let names: Vec<&str> = CONSDIR.iter().map(|e| e.0).collect();
        assert_eq!(names.len(), 23, "consdir[] is 23 files");
        let dir = d.attach("").unwrap();
        for n in names {
            assert!(d.walk(&dir, n).unwrap().is_some(), "#c has no {n}");
        }
        assert!(d.walk(&dir, "nosuchfile").unwrap().is_none());
    }

    /// `readnum` (`devcons.c:633`): right-aligned in `size-1`, then a space.
    /// Exactly `NUMSIZE` bytes, which is what makes `/dev/pid` seekable.
    #[test]
    fn a_number_is_formatted_as_plan_nine_formats_it() {
        assert_eq!(readnum(1, NUMSIZE), b"          1 ");
        assert_eq!(readnum(1, NUMSIZE).len(), NUMSIZE);
        assert_eq!(readnum(12345, NUMSIZE), b"      12345 ");
    }

    #[test]
    fn pid_and_ppid_report_the_current_process() {
        let (mut d, procs) = cons();
        assert_eq!(read(&mut d, "pid"), "          1 ");
        let c = procs.borrow_mut().rfork(1, crate::proc::rf::PROC).unwrap();
        d.up.borrow_mut().pid = c;
        assert_eq!(read(&mut d, "pid").trim(), c.to_string());
        assert_eq!(read(&mut d, "ppid").trim(), "1");
    }

    /// `/dev/drivers` is `#%C %s\n` per device (`devcons.c:198`) — and, here,
    /// what the host reports beside them. Christine: *"it can also report
    /// drivers provided by the host app"*.
    #[test]
    fn drivers_reports_the_kernels_letters_and_the_hosts_own() {
        let (mut d, _) = cons();
        let s = read(&mut d, "drivers");
        assert!(s.contains("#/ root\n"), "{s}");
        assert!(s.contains("#| pipe\n"), "{s}");
        assert!(s.contains("host screen\n"), "the host's are reported too: {s}");
    }

    /// The kernel computes no time, no entropy, no memory figure. It names
    /// what the host reports.
    #[test]
    fn the_host_supplies_what_the_kernel_only_reports() {
        let (mut d, _) = cons();
        assert!(read(&mut d, "time").starts_with(" 1500000000 "), "seconds first");
        assert_eq!(read(&mut d, "random").len(), 4096);
        assert!(read(&mut d, "swap").contains("1048576 memory\n"));
        assert!(read(&mut d, "swap").contains("4096 pagesize\n"));
        assert_eq!(read(&mut d, "config"), "ipnx userspace/rootfs\n");
        assert_eq!(read(&mut d, "osversion"), "2000");
    }

    /// `/dev/bintime` is big-endian, 8 bytes each (`readbintime`).
    #[test]
    fn bintime_is_big_endian_and_starts_with_the_nanoseconds() {
        let (mut d, _) = cons();
        let mut c = open(&mut d, "bintime", OREAD);
        let b = d.read(&mut c, 32, 0).unwrap();
        assert_eq!(b.len(), 32);
        assert_eq!(u64::from_be_bytes(b[0..8].try_into().unwrap()), 1_500_000_000_000_000_000);
    }

    #[test]
    fn null_swallows_and_zero_fills() {
        let (mut d, _) = cons();
        assert_eq!(write(&mut d, "null", "anything at all").unwrap(), 15);
        assert_eq!(read(&mut d, "null"), "");
        let mut c = open(&mut d, "zero", OREAD);
        assert_eq!(d.read(&mut c, 16, 0).unwrap(), vec![0u8; 16]);
    }

    /// `userwrite` (`auth.c:107`): *"anyone can become none"* — and only
    /// `none`, and only one way.
    #[test]
    fn dev_user_takes_none_and_nothing_else() {
        let (mut d, _) = cons();
        assert_eq!(read(&mut d, "user"), "eve");
        assert!(write(&mut d, "user", "mimmy").is_err(), "only none");
        write(&mut d, "user", "none").unwrap();
        assert_eq!(read(&mut d, "user"), "none");
        assert!(write(&mut d, "user", "eve").is_err(), "there is no way back");
    }

    /// `hostownerwrite` (`auth.c:126`) renames eve AND every process owned by
    /// the old name (`renameuser`, `proc.c:1601`).
    #[test]
    fn writing_hostowner_carries_eves_processes_with_it() {
        let (mut d, procs) = cons();
        let c = procs.borrow_mut().rfork(1, crate::proc::rf::PROC).unwrap();
        assert_eq!(procs.borrow().user(c).unwrap(), "eve");
        write(&mut d, "hostowner", "kitty").unwrap();
        assert_eq!(read(&mut d, "hostowner"), "kitty");
        assert_eq!(procs.borrow().user(1).unwrap(), "kitty");
        assert_eq!(procs.borrow().user(c).unwrap(), "kitty", "the child too");
    }

    /// Everything eve-only refuses everyone else, and `iseve` is a NAME
    /// comparison (`auth.c:17`).
    #[test]
    fn the_eve_only_files_refuse_anyone_else() {
        let (mut d, _) = cons();
        write(&mut d, "user", "none").unwrap();
        for f in ["hostowner", "hostdomain", "reboot"] {
            assert!(write(&mut d, f, "halt").is_err(), "{f} must be eve's alone");
        }
    }

    #[test]
    fn reboot_reaches_the_host_and_only_for_eve() {
        let (mut d, _) = cons();
        assert!(write(&mut d, "reboot", "halt").is_ok());
        assert!(write(&mut d, "reboot", "nonsense").is_err(), "the host refused it");
    }

    #[test]
    fn sysname_and_hostdomain_are_set_by_writing_them() {
        let (mut d, _) = cons();
        write(&mut d, "sysname", "gnot\n").unwrap();
        assert_eq!(read(&mut d, "sysname"), "gnot", "the trailing newline is stripped");
        write(&mut d, "hostdomain", "example.net").unwrap();
        assert_eq!(read(&mut d, "hostdomain"), "example.net");
        assert!(write(&mut d, "sysname", "").is_err());
    }

    /// The kernel's log is a file, and it appends.
    /// `kprint` is **exclusive use**: `consopen` takes `kprintinuse` with
    /// `tas` and answers `Einuse` to a second opener (`devcons.c`), and the
    /// file says so — `{Qkprint, 0, QTEXCL}` with `DMEXCL|0440` (`:616`),
    /// which `devdir` reports as one bit shifted into the mode. It drains
    /// the kernel's log, so two readers would each get part of it.
    #[test]
    fn kprint_is_exclusive_use_and_says_so() {
        let (mut d, _) = cons();
        let dir = d.attach("").unwrap();
        let c = d.walk(&dir, "kprint").unwrap().unwrap();
        assert_eq!(c.qid.qtype, QTEXCL, "the qid carries it");

        let stat = crate::ninep::Dir::conv_m2d(&d.stat(&c).unwrap()).unwrap();
        assert!(stat.mode & crate::ninep::DMEXCL != 0, "and the mode reports it");

        let mut first = d.open(c.clone(), OREAD).unwrap();
        assert!(d.open(c.clone(), OREAD).is_err(), "a second open is Einuse");
        d.close(&mut first);
        assert!(d.open(c, OREAD).is_ok(), "and the next one gets it");
    }

    #[test]
    fn kmesg_accumulates_and_reads_back() {
        let (mut d, _) = cons();
        write(&mut d, "kprint", "boot\n").unwrap();
        write(&mut d, "kprint", "ready\n").unwrap();
        assert_eq!(read(&mut d, "kmesg"), "boot\nready\n");
    }

    /// `/dev/pgrpid` is the NAMESPACE group's number, not a Unix process
    /// group: `struct Pgrp` holds `mnthash[]` and nothing else. So processes
    /// sharing a namespace share the number, and `rfork` with `RFNAMEG`
    /// changes it — `sysrfork` calls `newpgrp()` then `pgrpcpy`
    /// (`sysproc.c:140`), so a copy is a NEW group.
    #[test]
    fn pgrpid_is_the_namespace_group_and_a_copy_is_a_new_one() {
        let (mut d, procs) = cons();
        let mine = read(&mut d, "pgrpid").trim().to_string();

        // shared — RFPROC alone shares the namespace
        let shared = procs.borrow_mut().rfork(1, crate::proc::rf::PROC).unwrap();
        d.up.borrow_mut().pid = shared;
        assert_eq!(read(&mut d, "pgrpid").trim(), mine, "a shared namespace is the same group");

        // copied — RFNAMEG makes a new group
        let copied = procs.borrow_mut().rfork(1, crate::proc::rf::PROC | crate::proc::rf::NAMEG).unwrap();
        d.up.borrow_mut().pid = copied;
        assert_ne!(read(&mut d, "pgrpid").trim(), mine, "a copied namespace is a new group");
    }

    /// A process nothing has stamped reports `TReal` 0, not the whole epoch.
    /// The test below sets the origin, so without this one it would pass
    /// while every real process reported about forty-seven years.
    #[test]
    fn an_unstarted_process_reports_no_real_time() {
        let (mut d, _) = cons();
        let v: Vec<String> =
            read(&mut d, "cputime").split_whitespace().map(|s| s.to_string()).collect();
        assert_eq!(v[crate::proc::TREAL], "0");
    }

    /// Six numbers of `NUMSIZE` each (`devcons.c:63`), in milliseconds, and
    /// `TReal` is wall time rather than a zero.
    #[test]
    fn cputime_reports_six_numbers_and_real_time_advances() {
        let (mut d, procs) = cons();
        // the fake host's clock is fixed; start the process one second before it
        let (now, _, _) = FakeHost::default().now();
        procs.borrow_mut().started(1, now - 1_500_000_000);
        let s = read(&mut d, "cputime");
        assert_eq!(s.len(), 6 * NUMSIZE);
        let v: Vec<&str> = s.split_whitespace().collect();
        assert_eq!(v.len(), 6);
        assert_eq!(v[crate::proc::TREAL], "1500", "TReal is now - started, in ms");
    }

    /// An exited child's times fold into its parent's TCUser/TCSys/TCReal,
    /// which is what makes the last three numbers mean anything.
    #[test]
    fn a_childs_time_is_added_to_its_parents_when_it_exits() {
        let (mut d, procs) = cons();
        let c = procs.borrow_mut().rfork(1, crate::proc::rf::PROC).unwrap();
        procs.borrow_mut().charge(c, crate::proc::TUSER, 250);
        procs.borrow_mut().exits(c, "", None);
        let s = read(&mut d, "cputime");
        let v: Vec<&str> = s.split_whitespace().collect();
        assert_eq!(v[crate::proc::TCUSER], "250", "the child's user time came across");
    }

    /// `/dev/sysstat` is one line per machine, eight numbers, and writing it
    /// zeroes the counters (`devcons.c:107`).
    #[test]
    fn sysstat_counts_what_the_kernel_does_and_a_write_zeroes_it() {
        let (mut d, _) = cons();
        d.syscalls = 17;
        d.cs = 4;
        let s = read(&mut d, "sysstat");
        let v: Vec<&str> = s.split_whitespace().collect();
        assert_eq!(v.len(), 8, "eight counters");
        assert_eq!(v[1], "4", "context switches");
        assert_eq!(v[3], "17", "syscalls");
        write(&mut d, "sysstat", "").unwrap();
        assert_eq!(read(&mut d, "sysstat").split_whitespace().nth(3).unwrap(), "0");
    }

    /// **A read of `cons` answers a whole line**, because `consread` runs the
    /// line discipline until `lineq` has one (`devcons.c:762`). A shell gets a
    /// command, not a character.
    #[test]
    fn a_read_of_cons_answers_a_line_at_a_time() {
        let (mut d, _) = cons_term(&["echo hi\nls\n"]);
        let mut c = open(&mut d, "cons", OREAD);
        assert_eq!(d.read(&mut c, 256, 0).unwrap(), b"echo hi\n");
        assert_eq!(d.read(&mut c, 256, 0).unwrap(), b"ls\n");
    }

    /// The discipline itself: backspace erases, `^U` kills the line, and
    /// neither reaches the reader. `^D` ends a line WITHOUT being part of it,
    /// so an empty one is end of file.
    #[test]
    fn backspace_kill_and_end_of_file() {
        let (mut d, _) = cons_term(&["abc\x08\x08X\n", "junk\x15kept\n", "\x04"]);
        let mut c = open(&mut d, "cons", OREAD);
        assert_eq!(d.read(&mut c, 256, 0).unwrap(), b"aX\n");
        assert_eq!(d.read(&mut c, 256, 0).unwrap(), b"kept\n");
        assert!(d.read(&mut c, 256, 0).unwrap().is_empty(), "^D on an empty line is EOF");
    }

    /// A terminal that ends is the `^D` its user never typed: what was typed
    /// is delivered, and the read after it is the end.
    #[test]
    fn input_that_stops_delivers_the_last_line_then_ends() {
        let (mut d, _) = cons_term(&["half typed"]);
        let mut c = open(&mut d, "cons", OREAD);
        assert_eq!(d.read(&mut c, 256, 0).unwrap(), b"half typed");
        assert!(d.read(&mut c, 256, 0).unwrap().is_empty());
    }

    /// `rawon` turns the discipline off, and then every keystroke is a read.
    /// That is how a program that wants keys gets them.
    #[test]
    fn rawon_delivers_keystrokes_and_the_last_close_turns_it_off() {
        let (mut d, _) = cons_term(&["ab"]);
        let mut ctl = open(&mut d, "consctl", OWRITE);
        d.write(&mut ctl, b"rawon", 0).unwrap();
        let mut c = open(&mut d, "cons", OREAD);
        assert_eq!(d.read(&mut c, 256, 0).unwrap(), b"ab", "no line ending needed");

        // *"last close of control file turns off raw"* (`devcons.c:722`).
        ctl.flag |= crate::chan::flag::COPEN;
        d.close(&mut ctl);
        assert!(!d.kbd.raw, "a program that died in raw mode leaves a cooked console");
    }

    /// **A number is a FILE, and a file ends.** `readnum` answers 0 past the
    /// end (`devcons.c:640`), so a second read of `/dev/pid` gives nothing.
    /// Handing the formatted number back whatever the offset made `cat
    /// /dev/pid` print the pid until something stopped it.
    #[test]
    fn a_number_file_ends() {
        let (mut d, _) = cons_term(&[]);
        let mut c = open(&mut d, "pid", OREAD);
        let first = d.read(&mut c, 256, 0).unwrap();
        assert_eq!(first.len(), NUMSIZE);
        assert!(d.read(&mut c, 256, first.len() as u64).unwrap().is_empty());
    }

    /// **`#c` itself reads as a directory of 23.** `Qdir` is not in
    /// `consdir[]` here — Plan 9's table has `"."` and `devgen` skips it — so
    /// a read of the directory answered "no such file" and `ls /dev` showed
    /// nothing at all.
    #[test]
    fn the_device_itself_lists_its_twenty_three_files() {
        let (mut d, _) = cons_term(&[]);
        let dir = d.attach("").unwrap();
        let mut dir = d.open(dir, OREAD).unwrap();
        let b = d.read(&mut dir, 8192, 0).unwrap();
        let names: Vec<String> =
            crate::ninep::Dir::parse_all(&b).into_iter().map(|e| e.name).collect();
        assert_eq!(names.len(), CONSDIR.len());
        assert!(names.contains(&"cons".to_string()));
        assert!(names.contains(&"random".to_string()));
    }

    /// A write of `cons` reaches the screen — `screenputs` (`devcons.c:12`),
    /// the function pointer the architecture fills — and the kernel's log,
    /// because `putstrn0` calls `kmesgputs` first.
    #[test]
    fn a_write_of_cons_reaches_the_screen_and_the_log() {
        let (mut d, host) = cons_term(&[]);
        write(&mut d, "cons", "hello\n").unwrap();
        assert_eq!(host.0.borrow().out, b"hello\n");
        assert_eq!(read(&mut d, "kmesg"), "hello\n");
    }

    /// `consctl` is 0220. Reading it is refused, as `consread`'s default is.
    #[test]
    fn consctl_is_write_only() {
        let (mut d, _) = cons_term(&[]);
        let mut c = open(&mut d, "consctl", OWRITE);
        assert!(d.read(&mut c, 16, 0).is_err());
    }

    /// Nothing in `#c` is created or removed: the files are the kernel's
    /// state, and the set is fixed.
    #[test]
    fn nothing_here_is_created_or_removed() {
        let (mut d, _) = cons();
        let mut c = open(&mut d, "null", OWRITE);
        assert!(d.create(&mut c, "mine", 0, 0).is_err());
        assert!(d.remove(&mut c).is_err());
        assert!(d.wstat(&mut c, &[]).is_err());
    }
}
