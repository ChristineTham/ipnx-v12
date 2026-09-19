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
use crate::ninep::{Qid, QTDIR};
use crate::proc::{Pid, Procs, Up};
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
    /// `configfile` — how this kernel was started.
    fn config(&mut self) -> String;
    /// `/dev/reboot`: `halt`, or `reboot <path>`.
    fn reboot(&mut self, cmd: &str) -> Result<(), String>;
}

/// The device's own state — what Plan 9 keeps in globals beside `devcons.c`:
/// `eve` (`auth.c:10`), `hostdomain` (`auth.c:11`), `sysname`, the kernel log.
pub struct Cons {
    pub eve: String,
    pub hostdomain: String,
    pub sysname: String,
    pub kmesg: Vec<u8>,
    /// `Mach.syscall` and `Mach.cs` (`pc/dat.h:233`), the two counters this
    /// kernel is in a position to keep honestly.
    pub syscalls: u64,
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

/// Name, qid and permission — `consdir[]` exactly (`devcons.c:606`).
pub const CONSDIR: &[(&str, Q, u32)] = &[
    ("bintime", Q::Bintime, 0o664),
    ("cons", Q::Cons, 0o660),
    ("consctl", Q::Consctl, 0o220),
    ("cputime", Q::Cputime, 0o444),
    ("drivers", Q::Drivers, 0o444),
    ("hostdomain", Q::Hostdomain, 0o664),
    ("hostowner", Q::Hostowner, 0o664),
    ("kmesg", Q::Kmesg, 0o440),
    ("kprint", Q::Kprint, 0o440),
    ("null", Q::Null, 0o666),
    ("osversion", Q::Osversion, 0o444),
    ("pgrpid", Q::Pgrpid, 0o444),
    ("pid", Q::Pid, 0o444),
    ("ppid", Q::Ppid, 0o444),
    ("random", Q::Random, 0o444),
    ("reboot", Q::Reboot, 0o660),
    ("swap", Q::Swap, 0o664),
    ("sysname", Q::Sysname, 0o664),
    ("sysstat", Q::Sysstat, 0o666),
    ("time", Q::Time, 0o664),
    ("user", Q::User, 0o666),
    ("zero", Q::Zero, 0o444),
    ("config", Q::Config, 0o444),
];

impl Q {
    fn from_path(p: u64) -> Option<Q> {
        CONSDIR.iter().map(|e| e.1).find(|q| *q as u64 == p)
    }
    fn name(self) -> &'static str {
        CONSDIR.iter().find(|e| e.1 == self).map(|e| e.0).unwrap_or(".")
    }
    fn perm(self) -> u32 {
        CONSDIR.iter().find(|e| e.1 == self).map(|e| e.2).unwrap_or(0o555)
    }
}

const EPERM: &str = "permission denied";
const EBADARG: &str = "bad arg in system call";

impl Cons {
    pub fn new(
        eve: &str,
        up: Rc<RefCell<Up>>,
        letters: Vec<DevId>,
        host: Box<dyn Console>,
    ) -> Cons {
        Cons {
            eve: eve.to_string(),
            hostdomain: String::new(),
            sysname: String::new(),
            kmesg: Vec::new(),
            syscalls: 0,
            cs: 0,
            letters,
            up,
            host,
        }
    }

    /// `iseve()` — `strcmp(eve, up->user) == 0` (`auth.c:17`). A NAME
    /// comparison, not a bit.
    fn iseve(&self) -> bool {
        self.user() == self.eve
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

    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Qid>, String> {
        if c.qid.path != Q::Dir as u64 {
            return Err("not a directory".into());
        }
        if name == ".." || name == "." {
            return Ok(Some(Qid { qtype: QTDIR, vers: 0, path: Q::Dir as u64 }));
        }
        Ok(CONSDIR
            .iter()
            .find(|e| e.0 == name)
            .map(|e| Qid { qtype: 0, vers: 0, path: e.1 as u64 }))
    }

    fn open(&mut self, mut c: Chan, mode: u16) -> Result<Chan, String> {
        c.mode = mode;
        Ok(c)
    }

    fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
        Err(EPERM.into())
    }

    fn read(&mut self, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String> {
        let q = Q::from_path(c.qid.path).ok_or("no such file")?;
        Ok(match q {
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
            Q::Hostowner => Self::readstr(&self.eve.clone(), n, off),
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

            // the host's
            Q::Cons | Q::Consctl => return Err("served by the host".into()),
            Q::Reboot => return Err(EPERM.into()),
            Q::Dir => Vec::new(),
        })
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
                let old = self.eve.clone();
                let up = self.up.borrow();
                up.procs.borrow_mut().renameuser(&old, &s);
                self.eve = s;
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
            Q::Cons | Q::Consctl => return Err("served by the host".into()),
            _ => return Err(EPERM.into()),
        }
        Ok(data.len())
    }

    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        let q = Q::from_path(c.qid.path).unwrap_or(Q::Dir);
        Ok(crate::ninep::W::new()
            .u16(0)
            .u16(0)
            .u32(0)
            .raw(&c.qid.write(crate::ninep::W::new()).into_body())
            .u32(if q == Q::Dir { 0o555 } else { q.perm() })
            .u32(0)
            .u32(0)
            .u64(0)
            .s(q.name())
            .into_body())
    }

    fn wstat(&mut self, _c: &mut Chan, _e: &[u8]) -> Result<(), String> {
        Err(EPERM.into())
    }

    fn remove(&mut self, _c: &mut Chan) -> Result<(), String> {
        Err(EPERM.into())
    }

    fn close(&mut self, _c: &mut Chan) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chan::mode::{OREAD, OWRITE};
    use crate::dev::DevId;

    /// A host that reports fixed numbers, so a test can tell what came from
    /// where. The kernel computes none of this — it names it.
    struct FakeHost;
    impl Console for FakeHost {
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
        let up = Rc::new(RefCell::new(Up { pid: 1, procs: procs.clone() }));
        let letters = vec![DevId::Root, DevId::Pipe, DevId::Cons];
        (Cons::new("eve", up, letters, Box::new(FakeHost)), procs)
    }

    fn open(d: &mut Cons, name: &str, mode: u16) -> Chan {
        let dir = d.attach("").unwrap();
        let mut c = dir.clone();
        c.qid = d.walk(&dir, name).unwrap().expect(name);
        d.open(c, mode).unwrap()
    }

    fn read(d: &mut Cons, name: &str) -> String {
        let mut c = open(d, name, OREAD);
        String::from_utf8_lossy(&d.read(&mut c, 4096, 0).unwrap()).to_string()
    }

    fn write(d: &mut Cons, name: &str, s: &str) -> Result<usize, String> {
        let mut c = open(d, name, OWRITE);
        d.write(&mut c, s.as_bytes(), 0)
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
        let (now, _, _) = FakeHost.now();
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

    /// `cons` and `consctl` are the two of the 23 the host serves (P4), so
    /// the kernel refuses rather than pretending.
    #[test]
    fn the_console_itself_is_the_hosts() {
        let (mut d, _) = cons();
        let mut c = open(&mut d, "cons", OREAD);
        assert!(d.read(&mut c, 16, 0).is_err());
        assert!(write(&mut d, "consctl", "rawon").is_err());
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
