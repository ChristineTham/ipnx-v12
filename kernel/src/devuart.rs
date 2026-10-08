//! `#t` — the uart device (`plan9/sys/src/9/port/devuart.c`).
//!
//! **A serial line, as files**: for each uart, `eia%d` (the line), `eia%dctl`
//! (its settings, written as `uartctl`'s one-letter commands) and
//! `eia%dstatus` (what the hardware reports) — `uartreset`'s directory,
//! `devuart.c:211`. The data file is two [`Queue`]s: what arrives is
//! `qproduce`d into `iq` at clock time and `qread` out of it, and what is
//! written is `qwrite`n into `oq` and `qconsume`d by the transmitter.
//!
//! The device is Plan 9's portable half; the hardware half is a
//! [`PhysUart`] — `struct PhysUart` (`portdat.h:899`), the table of
//! functions `uarti8250.c` fills in for the PC. Here the host supplies it,
//! as it supplies the console behind `#c`, and its far end is the surface
//! (docs/surface.md).
//!
//! **Where this differs, and why.** Three places, each because Plan 9's
//! counterpart cannot exist here:
//!
//! * *No interrupts.* A host cannot interrupt this kernel, so the
//!   `PhysUart`'s interrupt handler — `i8250interrupt`, which calls
//!   `uartrecv` for each character and `uartkick` when the transmitter
//!   empties — is **called from `uartclock`**, the clock routine that
//!   already takes the staged input in every 22ms (`devuart.c:244`). It is
//!   the interrupt, polled.
//! * *No kernel stack.* A call that sleeps runs again from the top when the
//!   process is entered again, so where it was is kept per process
//!   ([`Held`], and [`qio::At`] for the queues).
//! * *A close cannot sleep.* `uartclose` waits in `uartdrainoutput` for the
//!   line to take what is queued (`devuart.c:342`); a close here returns
//!   nothing and cannot leave the processor. It kicks the transmitter once,
//!   and whatever the line would not take is freed by the `qclose` that
//!   follows the wait in Plan 9 too. A host line takes everything it is
//!   given unless the far end has sent `^S` with `x1` set.

use crate::chan::{flag::COPEN, mode, Chan};
use crate::dev::{Dev, DevId, Eve};
use crate::ninep::{Dir, Qid, DMDIR, QTDIR};
use crate::proc::{Pid, Procs, QLock, Rid, Up, EINTR};
use crate::qio::{self, At, Qid3, Queue, MAXATOMIC};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

/// `Stagesize` — `STAGESIZE`, 2048 on the PC (`pc/dat.h:33`): the
/// staging areas between the interrupt and the queues.
pub const STAGESIZE: usize = 2048;

/// *"soft flow control chars"* (`devuart.c:13`).
const CTLS: u8 = 0o23;
const CTLQ: u8 = 0o21;

/// `Ndataqid`, `Nctlqid`, `Nstatqid` (`netif.h:15`) — the uart's files are
/// numbered as a network interface's are, `NETQID(i, t)`.
const NDATAQID: u64 = 5;
const NCTLQID: u64 = 6;
const NSTATQID: u64 = 7;

fn netqid(i: u32, t: u64) -> u64 {
    ((i as u64) << 5) | t
}
fn netid(path: u64) -> u32 {
    (path >> 5) as u32
}
fn nettype(path: u64) -> u64 {
    path & 0x1f
}

/// `NUMSIZE` (`portdat.h:800`) — what `readnum` pads to.
const NUMSIZE: usize = 12;

/// Which of a uart's queues a [`Rid::Rr`] or [`Rid::Wr`] names.
const IQ: usize = 0;
const OQ: usize = 1;

const EPERM: &str = "permission denied";
const EBADARG: &str = "bad arg in system call";
const ENONEXIST: &str = "file does not exist";
const ESHORTSTAT: &str = "stat buffer too small";

/// `struct PhysUart` (`portdat.h:899`) — the hardware, supplied by the
/// host. Every method takes the software [`Uart`] it drives, as Plan 9's
/// take `Uart*`; the hardware's own registers are the implementation's.
///
/// Absent: `pnp`, which is the host handing the uarts over at boot;
/// `power`, because there is no power management; and `getc` and `putc`,
/// the polling versions *"for iprint, rdb"*, because no uart here is the
/// console.
pub trait PhysUart {
    /// `name` — the kind of hardware, `i8250` on the PC.
    fn name(&self) -> &str;
    fn enable(&mut self, u: &mut Uart, ie: bool);
    fn disable(&mut self, u: &mut Uart);
    /// Start output: take bytes with [`Uart::stageoutput`] and send them
    /// while the line will take them — `i8250kick` (`uarti8250.c:438`).
    fn kick(&mut self, u: &mut Uart);
    fn dobreak(&mut self, u: &mut Uart, ms: i32);
    /// `baud`, `bits`, `stop`, `parity` — `Err` is the `-1` that makes the
    /// ctl write `Ebadarg`. Each records the setting in the [`Uart`] on
    /// success, as `i8250baud` sets `uart->baud` (`uarti8250.c:415`).
    fn baud(&mut self, u: &mut Uart, baud: i32) -> Result<(), ()>;
    fn bits(&mut self, u: &mut Uart, bits: i32) -> Result<(), ()>;
    fn stop(&mut self, u: &mut Uart, stop: i32) -> Result<(), ()>;
    fn parity(&mut self, u: &mut Uart, parity: u8) -> Result<(), ()>;
    fn modemctl(&mut self, u: &mut Uart, on: bool);
    fn rts(&mut self, u: &mut Uart, on: bool);
    fn dtr(&mut self, u: &mut Uart, on: bool);
    /// What `eia%dstatus` reads — `i8250status`'s text on the PC.
    fn status(&mut self, u: &Uart) -> String;
    fn fifo(&mut self, u: &mut Uart, level: i32);
    /// **The interrupt handler** — `i8250interrupt` (`uarti8250.c:463`):
    /// each character received goes to [`Uart::recv`], and a change of the
    /// modem lines to `cts`, `dsr`, `dcd` and `dohup`. `true` is
    /// *"Thr Empty"*: the device then calls `uartkick`. See the module's
    /// comment for why it is called at clock time.
    fn interrupt(&mut self, u: &mut Uart) -> bool;
}

/// `struct Uart` (`portdat.h:928`) — *"software UART"*: the portable
/// state of one line. `regs` is the [`PhysUart`]'s own.
pub struct Uart {
    /// *"internal name"*.
    pub name: String,
    /// *"clock frequency"*, which `i8250baud` divides.
    pub freq: u32,
    pub bits: i32,
    pub stop: i32,
    /// `'e'`, `'o'` or `'n'`, as `uartctl`'s `p` gives it.
    pub parity: u8,
    pub baud: i32,
    /// `type` — *"??"*, Plan 9's own comment; `i8250status` prints it.
    pub kind: i32,
    pub dev: u32,
    opens: u32,
    pub enabled: bool,
    pub perr: u32,
    pub ferr: u32,
    pub oerr: u32,
    pub berr: u32,
    pub serr: u32,
    iq: Option<Queue>,
    oq: Option<Queue>,
    /// `istage`, `ir`, `iw`, `ie` — the ring `uartrecv` fills at interrupt
    /// time and `uartstageinput` empties into `iq`. `None` until the first
    /// enable sets `iw`: *"maybe the line isn't enabled yet"*.
    istage: Option<VecDeque<u8>>,
    /// `ostage`, `op`, `oe` — what `uartstageoutput` took from `oq` and the
    /// transmitter has not sent. `oe` is its length.
    pub ostage: Vec<u8>,
    pub op: usize,
    drain: bool,
    /// *"hardware flow control on"*, *"software flow control on"*.
    pub modem: bool,
    pub xonoff: bool,
    pub blocked: bool,
    /// *"keep track of modem status"*.
    pub cts: bool,
    pub dsr: bool,
    pub dcd: bool,
    pub ctsbackoff: i32,
    /// *"send hangup upstream?"*
    pub hup_dsr: bool,
    pub hup_dcd: bool,
    pub dohup: bool,
    /// The `QLock` a `Uart` embeds.
    qlock: QLock,
    /// What `qproduce` and `qconsume` asked to be woken, and the
    /// `phys->rts(p, 0)` `uartstageinput` makes on overflow — done by the
    /// device once the method that found them returns, since a [`Uart`]
    /// reaches neither the process table nor its hardware.
    wakerr: bool,
    wakewr: bool,
    rtsoff: bool,
}

impl Uart {
    /// A uart as `pnp` hands one over: named, and nothing set. `uartenable`
    /// gives the defaults — `l8 s1 pn b9600` — on the first open.
    pub fn new(name: &str, freq: u32) -> Uart {
        Uart {
            name: name.to_string(),
            freq,
            bits: 0,
            stop: 0,
            parity: 0,
            baud: 0,
            kind: 0,
            dev: 0,
            opens: 0,
            enabled: false,
            perr: 0,
            ferr: 0,
            oerr: 0,
            berr: 0,
            serr: 0,
            iq: None,
            oq: None,
            istage: None,
            ostage: Vec::new(),
            op: 0,
            drain: false,
            modem: false,
            xonoff: false,
            blocked: false,
            cts: false,
            dsr: false,
            dcd: false,
            ctsbackoff: 0,
            hup_dsr: false,
            hup_dcd: false,
            dohup: false,
            qlock: QLock::default(),
            wakerr: false,
            wakewr: false,
            rtsoff: false,
        }
    }

    /// `uartstageoutput` (`devuart.c:616`) — *"put some bytes into the
    /// local queue to avoid calling qconsume for every character"*. The
    /// number of bytes staged; 0 is nothing to send.
    pub fn stageoutput(&mut self) -> usize {
        let Some(oq) = self.oq.as_mut() else { return 0 };
        let (b, wake) = oq.consume(STAGESIZE);
        self.wakewr |= wake;
        let Some(b) = b else { return 0 };
        let n = b.len();
        self.ostage = b;
        self.op = 0;
        n
    }

    /// `uartrecv` (`devuart.c:682`) — *"receive a character at interrupt
    /// time"*. No uart here is the console or a mouse, so there is no
    /// `putc`: the character is staged.
    pub fn recv(&mut self, ch: u8) {
        // *"software flow control"*.
        if self.xonoff {
            if ch == CTLS {
                self.blocked = true;
            } else if ch == CTLQ {
                self.blocked = false;
                // *"clock gets output going again"*.
                self.ctsbackoff = 2;
            }
        }
        let Some(stage) = self.istage.as_ref() else { return };
        // *"next = p->iw + 1; … if(next == p->ir) uartstageinput(p);
        // if(next != p->ir){ *p->iw = ch; … }"* — the ring holds one less
        // than its size.
        if stage.len() + 1 == STAGESIZE {
            self.stageinput();
        }
        let stage = self.istage.as_mut().expect("staged");
        if stage.len() + 1 < STAGESIZE {
            stage.push_back(ch);
        }
    }

    /// `uartstageinput` (`devuart.c:654`) — *"Move data from the interrupt
    /// staging area to the input Queue."* Plan 9 does it in two
    /// `qproduce`s when the ring wraps; a `VecDeque` is one run.
    fn stageinput(&mut self) {
        let Some(stage) = self.istage.as_mut() else { return };
        if stage.is_empty() {
            return;
        }
        let data: Vec<u8> = stage.drain(..).collect();
        let Some(iq) = self.iq.as_mut() else { return };
        match iq.produce(&data) {
            // *"p->serr++; (*p->phys->rts)(p, 0);"*.
            Err(()) => {
                self.serr += 1;
                self.rtsoff = true;
            }
            Ok(wake) => self.wakerr |= wake,
        }
    }

    /// `uartdrained` (`devuart.c:295`).
    fn drained(&self) -> bool {
        self.oq.as_ref().is_none_or(|q| q.dlen() == 0) && self.op >= self.ostage.len()
    }
}

/// Where a process that left the processor in the middle of a write to
/// `eia%d` or `eia%dctl` was, holding (or queued for) the uart's `QLock`.
enum Held {
    /// Queued on `qlock(p)`; it holds the lock when entered again.
    Lock,
    /// In `qwrite` on `oq`, where [`qio::At`] says.
    Data,
    /// In `uartctl`, asleep in `uartdrainoutput` before the command at
    /// `field`.
    Drain { field: usize },
}

pub struct UartDev {
    eve: Eve,
    up: Rc<RefCell<Up>>,
    /// `uart[]` and `physuart`, index for index: `uart[i]->phys`.
    uart: Vec<Uart>,
    phys: Vec<Box<dyn PhysUart>>,
    /// The permission of `eia%d` and `eia%dctl`, which `uartwstat`
    /// changes together (`devuart.c:561`).
    perm: Vec<u32>,
    /// `uarttimer` — when `uartclock` next runs, and its period, which
    /// `w` sets in units of 100µs (`devuart.c:478`).
    clock: u64,
    tns: u64,
    /// Where each process in a queue's `qread` or `qwrite` was.
    qat: HashMap<Pid, At>,
    held: HashMap<Pid, Held>,
}

impl UartDev {
    /// `uartreset` (`devuart.c:171`) — the uarts the host found, numbered
    /// in the order given. None is a console or `special`, so none is
    /// enabled until opened.
    pub fn new(up: Rc<RefCell<Up>>, found: Vec<(Uart, Box<dyn PhysUart>)>) -> UartDev {
        let mut uart = Vec::new();
        let mut phys = Vec::new();
        for (i, (mut u, p)) in found.into_iter().enumerate() {
            u.dev = i as u32;
            uart.push(u);
            phys.push(p);
        }
        let n = uart.len();
        UartDev {
            eve: Eve::default(),
            up,
            uart,
            phys,
            perm: vec![0o660; n],
            clock: 0,
            // *"processing it every 22 ms should be fine"*.
            tns: 22_000_000,
            qat: HashMap::new(),
            held: HashMap::new(),
        }
    }

    fn up(&self) -> (Pid, Rc<RefCell<Procs>>) {
        let up = self.up.borrow();
        (up.pid, up.procs.clone())
    }

    fn user(&self) -> String {
        self.up.borrow().user()
    }

    /// The wakeups and the RTS drop that a [`Uart`] method found.
    fn after(&mut self, i: usize, procs: &mut Procs) {
        let u = &mut self.uart[i];
        let dev = u.dev;
        if std::mem::take(&mut u.wakerr) {
            procs.wakeup(Rid::Rr(DevId::Uart, dev, IQ));
        }
        if std::mem::take(&mut u.wakewr) {
            procs.wakeup(Rid::Wr(DevId::Uart, dev, OQ));
        }
        if std::mem::take(&mut u.rtsoff) {
            self.phys[i].rts(u, false);
        }
    }

    /// `uartkick` (`devuart.c:632`) — *"restart output"*.
    fn uartkick(&mut self, i: usize, procs: &mut Procs) {
        if self.uart[i].blocked {
            return;
        }
        self.phys[i].kick(&mut self.uart[i]);
        self.after(i, procs);
        let u = &mut self.uart[i];
        if u.drain && u.drained() {
            u.drain = false;
            procs.wakeup(Rid::Uart(u.dev));
        }
    }

    /// `uartflow` (`devuart.c:602`) — *"restart input if it's off"*: the
    /// input queue's kick.
    fn uartflow(u: &mut Uart, phys: &mut dyn PhysUart) {
        if u.modem {
            phys.rts(u, true);
        }
    }

    /// `uartenable` (`devuart.c:40`).
    fn uartenable(&mut self, i: usize) {
        let u = &mut self.uart[i];
        if u.enabled {
            return;
        }
        match u.iq.as_mut() {
            None => u.iq = Some(Queue::new(8 * 1024)),
            Some(q) => q.reopen(),
        }
        match u.oq.as_mut() {
            None => u.oq = Some(Queue::new(8 * 1024)),
            Some(q) => q.reopen(),
        }
        u.istage = Some(VecDeque::new());
        u.ostage.clear();
        u.op = 0;
        u.hup_dsr = false;
        u.hup_dcd = false;
        u.dsr = false;
        u.dcd = false;
        // *"assume we can send"*.
        u.cts = true;
        u.ctsbackoff = 0;
        // Nothing is queued, so none of these waits for output to drain.
        let (bits, stop, parity, baud) = (u.bits, u.stop, u.parity, u.baud);
        if bits == 0 {
            let _ = self.uartctl_now(i, "l8");
        }
        if stop == 0 {
            let _ = self.uartctl_now(i, "s1");
        }
        if parity == 0 {
            let _ = self.uartctl_now(i, "pn");
        }
        if baud == 0 {
            let _ = self.uartctl_now(i, "b9600");
        }
        self.phys[i].enable(&mut self.uart[i], true);
        self.uart[i].enabled = true;
    }

    /// `uartdisable` (`devuart.c:105`).
    fn uartdisable(&mut self, i: usize) {
        if !self.uart[i].enabled {
            return;
        }
        self.phys[i].disable(&mut self.uart[i]);
        self.uart[i].enabled = false;
    }

    /// `uartctl` on a line with nothing queued — `uartenable`'s own calls.
    fn uartctl_now(&mut self, i: usize, cmd: &str) -> Result<(), ()> {
        let f: Vec<String> = cmd.split_whitespace().map(str::to_string).collect();
        match self.uartctl(i, &f, 0, None, 0) {
            Ctl::Done(r) => r,
            Ctl::Asleep(_) | Ctl::Eintr => Err(()),
        }
    }

    /// `uartdrainoutput` (`devuart.c:304`): `Ok(true)` is drained, and on
    /// with the command; `Ok(false)` is asleep on `p->r`.
    fn drainoutput(&mut self, i: usize, pid: Pid, procs: &mut Procs) -> Result<bool, ()> {
        let u = &mut self.uart[i];
        if !u.enabled {
            return Ok(true);
        }
        u.drain = true;
        // *"sleep(&p->r, uartdrained, p)"* — the condition first.
        if u.drained() {
            return Ok(true);
        }
        if !procs.sleep(pid, Rid::Uart(u.dev), false) {
            // A note already pending: *"p->drain = 0; nexterror();"*.
            procs.interrupted(pid);
            u.drain = false;
            return Err(());
        }
        Ok(false)
    }

    /// `uartctl` (`devuart.c:379`) — the commands, from field `from`; the
    /// field `drained` has already had its drain.
    fn uartctl(&mut self, i: usize, f: &[String], from: usize, drained: Option<usize>, pid: Pid) -> Ctl {
        let procs = self.up.borrow().procs.clone();
        for (at, field) in f.iter().enumerate().skip(from) {
            if field.starts_with("break") {
                self.phys[i].dobreak(&mut self.uart[i], 0);
                continue;
            }
            let b = field.as_bytes();
            let n = atoi(&field[1..]);
            let cmd = b[0];
            // The commands that let output drain first.
            if b"BbDdIiKkLlMmPpRrSs".contains(&cmd) && drained != Some(at) {
                let mut procs = procs.borrow_mut();
                match self.drainoutput(i, pid, &mut procs) {
                    Ok(true) => {}
                    Ok(false) => return Ctl::Asleep(at),
                    Err(()) => return Ctl::Eintr,
                }
            }
            let (u, p) = (&mut self.uart[i], &mut self.phys[i]);
            match cmd {
                b'B' | b'b' => {
                    if p.baud(u, n).is_err() {
                        return Ctl::Done(Err(()));
                    }
                }
                b'C' | b'c' => u.hup_dcd = n != 0,
                b'D' | b'd' => p.dtr(u, n != 0),
                b'E' | b'e' => u.hup_dsr = n != 0,
                b'F' | b'f' => {
                    if let Some(q) = u.oq.as_mut() {
                        q.flush();
                    }
                }
                b'H' | b'h' => {
                    if let Some(q) = u.iq.as_mut() {
                        q.hangup();
                    }
                    if let Some(q) = u.oq.as_mut() {
                        q.hangup();
                    }
                    let dev = u.dev;
                    wakeall(&mut procs.borrow_mut(), dev);
                }
                b'I' | b'i' => p.fifo(u, n),
                b'K' | b'k' => p.dobreak(u, n),
                b'L' | b'l' => {
                    if p.bits(u, n).is_err() {
                        return Ctl::Done(Err(()));
                    }
                }
                b'M' | b'm' => p.modemctl(u, n != 0),
                b'N' | b'n' => {
                    if let Some(q) = u.oq.as_mut() {
                        q.noblock(n != 0);
                    }
                }
                b'P' | b'p' => {
                    let c = b.get(1).copied().unwrap_or(0);
                    if p.parity(u, c).is_err() {
                        return Ctl::Done(Err(()));
                    }
                }
                b'Q' | b'q' => {
                    if let Some(q) = u.iq.as_mut() {
                        q.setlimit(n.max(0) as usize);
                    }
                    if let Some(q) = u.oq.as_mut() {
                        q.setlimit(n.max(0) as usize);
                    }
                }
                b'R' | b'r' => p.rts(u, n != 0),
                b'S' | b's' => {
                    if p.stop(u, n).is_err() {
                        return Ctl::Done(Err(()));
                    }
                }
                b'W' | b'w' => {
                    if n < 1 {
                        return Ctl::Done(Err(()));
                    }
                    self.tns = n as u64 * 100_000;
                }
                b'X' | b'x' => {
                    if u.enabled {
                        u.xonoff = n != 0;
                    }
                }
                _ => {}
            }
        }
        Ctl::Done(Ok(()))
    }

    /// `uartclock` (`devuart.c:719`) — every `tns`, for each enabled uart:
    /// its interrupt handler (see the module's comment), then *"this
    /// hopefully amortizes cost of qproduce to many chars"*, the hangup,
    /// and the flow-control hysteresis.
    pub fn uartclock(&mut self, now: u64) {
        if self.uart.is_empty() || now < self.clock {
            return;
        }
        self.clock = now + self.tns;
        let procs = self.up.borrow().procs.clone();
        let mut procs = procs.borrow_mut();
        for i in 0..self.uart.len() {
            if !self.uart[i].enabled {
                continue;
            }
            if self.phys[i].interrupt(&mut self.uart[i]) {
                self.uartkick(i, &mut procs);
            }
            self.uart[i].stageinput();
            self.after(i, &mut procs);
            let u = &mut self.uart[i];
            // *"hang up if requested"*.
            if std::mem::take(&mut u.dohup) {
                if let Some(q) = u.iq.as_mut() {
                    q.hangup();
                }
                if let Some(q) = u.oq.as_mut() {
                    q.hangup();
                }
                wakeall(&mut procs, u.dev);
            }
            // *"this adds hysteresis to hardware/software flow control"*.
            if u.ctsbackoff > 0 {
                u.ctsbackoff -= 1;
                if u.ctsbackoff == 0 {
                    self.phys[i].kick(&mut self.uart[i]);
                    self.after(i, &mut procs);
                }
            }
        }
    }

    /// **Whether the clock must keep running for a uart** — an open line
    /// whose far end is there (`dcd`) can still deliver, or take output
    /// that is waiting; one with nobody on it cannot, and a system with
    /// nothing else to do is then over, as it is for the console.
    pub fn waiting(&self) -> bool {
        self.uart.iter().any(|u| u.enabled && u.dcd)
    }

    fn index(&self, c: &Chan) -> Result<usize, String> {
        let i = netid(c.qid.path) as usize;
        if i < self.uart.len() {
            Ok(i)
        } else {
            Err(ENONEXIST.into())
        }
    }

    /// `uartdir[]`'s entry for a file: its name and permission.
    fn entry(&self, path: u64) -> Option<(String, u32)> {
        let i = netid(path);
        let perm = *self.perm.get(i as usize)?;
        match nettype(path) {
            NDATAQID => Some((format!("eia{i}"), perm)),
            NCTLQID => Some((format!("eia{i}ctl"), perm)),
            NSTATQID => Some((format!("eia{i}status"), 0o444)),
            _ => None,
        }
    }

    /// `devgen` over `uartdir`, with `setlength`'s length on the data file
    /// (`devuart.c:152`): what `iq` holds, if the line is open.
    fn dir(&self, c: &Chan, path: u64) -> Option<Dir> {
        let (name, perm) = self.entry(path)?;
        let u = &self.uart[netid(path) as usize];
        let length = match (nettype(path), &u.iq) {
            (NDATAQID, Some(q)) if u.opens > 0 => q.dlen() as u64,
            _ => 0,
        };
        let eve = self.eve.borrow();
        Some(crate::dev::devdir(c, Qid { qtype: 0, vers: 0, path }, &name, length, &eve, &eve, perm))
    }

    fn paths(&self) -> Vec<u64> {
        (0..self.uart.len() as u32)
            .flat_map(|i| [NDATAQID, NCTLQID, NSTATQID].map(|t| netqid(i, t)))
            .collect()
    }

    /// `uartwrite`'s `Ndataqid` (`devuart.c:512`): *"qlock(p); … n =
    /// qwrite(p->oq, buf, n); qunlock(p);"*.
    fn datawrite(&mut self, i: usize, data: &[u8]) -> Result<usize, String> {
        let (pid, procs) = self.up();
        let mut procs = procs.borrow_mut();
        match self.held.remove(&pid) {
            None => {
                if !procs.qlock(&mut self.uart[i].qlock, pid) {
                    self.held.insert(pid, Held::Lock);
                    return Ok(0);
                }
            }
            Some(Held::Lock) | Some(Held::Data) => {}
            Some(Held::Drain { .. }) => unreachable!("a process is in one call"),
        }
        let dev = self.uart[i].dev;
        let mut oq = self.uart[i].oq.take().ok_or(EBADARG)?;
        let (uart, phys) = (&mut self.uart, &mut self.phys);
        let mut spare = None;
        // `q->kick` is `uartkick`, which reaches the queue through the uart.
        let r = qio::qwrite(&mut oq, &mut self.qat, &mut procs, pid, Qid3 { dev: DevId::Uart, devno: dev, which: OQ }, data, None, &mut |q, procs| {
            let u = &mut uart[i];
            u.oq = Some(std::mem::replace(q, spare.take().unwrap_or_else(|| Queue::new(0))));
            if !u.blocked {
                phys[i].kick(u);
                if std::mem::take(&mut u.wakewr) {
                    procs.wakeup(Rid::Wr(DevId::Uart, dev, OQ));
                }
                if u.drain && u.drained() {
                    u.drain = false;
                    procs.wakeup(Rid::Uart(dev));
                }
            }
            spare = Some(std::mem::replace(q, u.oq.take().expect("put back")));
        });
        self.uart[i].oq = Some(oq);
        if self.qat.contains_key(&pid) {
            self.held.insert(pid, Held::Data);
            return Ok(0);
        }
        procs.qunlock(&mut self.uart[i].qlock);
        r
    }

    /// `uartwrite`'s `Nctlqid` (`devuart.c:531`): under `qlock(p)`,
    /// `uartctl`, and *"if(uartctl(p, cmd) < 0) error(Ebadarg)"*.
    fn ctlwrite(&mut self, i: usize, data: &[u8]) -> Result<usize, String> {
        let (pid, procs) = self.up();
        let (from, drained) = {
            let mut procs = procs.borrow_mut();
            match self.held.remove(&pid) {
                None => {
                    if !procs.qlock(&mut self.uart[i].qlock, pid) {
                        self.held.insert(pid, Held::Lock);
                        return Ok(0);
                    }
                    (0, None)
                }
                Some(Held::Lock) => (0, None),
                Some(Held::Drain { field }) => {
                    // The drain's `sleep` ended by a note.
                    if procs.interrupted(pid) {
                        self.uart[i].drain = false;
                        procs.qunlock(&mut self.uart[i].qlock);
                        return Err(EINTR.into());
                    }
                    (field, Some(field))
                }
                Some(Held::Data) => unreachable!("a process is in one call"),
            }
        };
        let cmd = String::from_utf8_lossy(data);
        let f: Vec<String> = cmd.split_whitespace().map(str::to_string).collect();
        let r = match self.uartctl(i, &f, from, drained, pid) {
            Ctl::Asleep(field) => {
                self.held.insert(pid, Held::Drain { field });
                return Ok(0);
            }
            Ctl::Eintr => Err(EINTR.to_string()),
            Ctl::Done(Ok(())) => Ok(data.len()),
            Ctl::Done(Err(())) => Err(EBADARG.to_string()),
        };
        procs.borrow_mut().qunlock(&mut self.uart[i].qlock);
        r
    }
}

/// What `uartctl` came to.
enum Ctl {
    Done(Result<(), ()>),
    /// Asleep in `uartdrainoutput` before the command at this field.
    Asleep(usize),
    Eintr,
}

/// *"wakeup(&q->rr); wakeup(&q->wr);"* for both queues — what `qhangup`
/// ends with (`qio.c:1430`).
fn wakeall(procs: &mut Procs, dev: u32) {
    for q in [IQ, OQ] {
        procs.wakeup(Rid::Rr(DevId::Uart, dev, q));
        procs.wakeup(Rid::Wr(DevId::Uart, dev, q));
    }
}

/// `atoi` — leading digits, after an optional sign; nothing is 0.
fn atoi(s: &str) -> i32 {
    let s = s.trim_start();
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let n = digits.bytes().take_while(u8::is_ascii_digit).fold(0i32, |n, d| n.wrapping_mul(10).wrapping_add((d - b'0') as i32));
    if neg {
        -n
    } else {
        n
    }
}

impl Dev for UartDev {
    fn id(&self) -> DevId {
        DevId::Uart
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn seteve(&mut self, eve: Eve) {
        self.eve = eve;
    }

    /// `uartattach` — *"return devattach('t', spec);"*.
    fn attach(&mut self, spec: &str) -> Result<Chan, String> {
        Ok(Chan::attach_spec(DevId::Uart, 0, spec))
    }

    /// `devwalk` over `uartdir` (`devuart.c:256`).
    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Chan>, String> {
        if !c.qid.is_dir() {
            return Err("not a directory".into());
        }
        if name == "." || name == ".." {
            return Ok(Some(c.walked(name, Qid { qtype: QTDIR, vers: 0, path: 0 })));
        }
        Ok(self
            .paths()
            .into_iter()
            .find(|&p| self.entry(p).is_some_and(|(n, _)| n == name))
            .map(|path| c.walked(name, Qid { qtype: 0, vers: 0, path })))
    }

    /// `uartopen` (`devuart.c:270`): `devopen`, then the first open of a
    /// line's data or ctl file enables it.
    ///
    /// `devopen` checks the permission (`dev.c:371`) and refuses to open a
    /// directory for anything but reading (`:379`).
    ///
    /// `qlock(p)` around the count is not taken: a device call here runs
    /// to its end unless it sleeps, and the only calls that sleep holding
    /// the lock — a write, a ctl — hold an open of the line, so the count
    /// is never 0 under them and `uartenable` has nothing to do.
    fn open(&mut self, mut c: Chan, omode: u16) -> Result<Chan, String> {
        if c.qid.is_dir() {
            if omode != mode::OREAD {
                return Err(EPERM.into());
            }
        } else {
            let (_, perm) = self.entry(c.qid.path).ok_or(ENONEXIST)?;
            let eve = self.eve.borrow().clone();
            crate::dev::permcheck(&self.user(), &eve, &eve, perm, omode)?;
            if matches!(nettype(c.qid.path), NDATAQID | NCTLQID) {
                let i = self.index(&c)?;
                self.uart[i].opens += 1;
                if self.uart[i].opens == 1 {
                    self.uartenable(i);
                }
            }
        }
        c.offset = 0;
        c.mode = crate::chan::openmode(omode)?;
        c.flag |= COPEN;
        // *"c->iounit = qiomaxatomic"*.
        c.iounit = MAXATOMIC as u32;
        Ok(c)
    }

    fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
        Err(EPERM.into())
    }

    /// `uartread` (`devuart.c:355`).
    fn read(&mut self, c: &mut Chan, n: usize, off: u64) -> Result<Vec<u8>, String> {
        if c.qid.is_dir() {
            let entries: Vec<Dir> = self.paths().into_iter().filter_map(|p| self.dir(c, p)).collect();
            return Ok(crate::dev::devdirread(c, n, &entries));
        }
        let i = self.index(c)?;
        match nettype(c.qid.path) {
            // *"return qread(p->iq, buf, n);"*
            NDATAQID => {
                let (pid, procs) = self.up();
                let mut procs = procs.borrow_mut();
                let dev = self.uart[i].dev;
                let mut iq = self.uart[i].iq.take().ok_or(EBADARG)?;
                let (u, phys) = (&mut self.uart[i], &mut self.phys[i]);
                let r = qio::qread(&mut iq, &mut self.qat, &mut procs, pid, Qid3 { dev: DevId::Uart, devno: dev, which: IQ }, n, &mut |_, _| {
                    Self::uartflow(u, phys.as_mut());
                });
                self.uart[i].iq = Some(iq);
                r
            }
            NCTLQID => Ok(slice(&crate::devcons::readnum(i as u64, NUMSIZE), n, off)),
            NSTATQID => {
                let s = self.phys[i].status(&self.uart[i]);
                Ok(slice(s.as_bytes(), n, off))
            }
            _ => Ok(Vec::new()),
        }
    }

    /// `uartwrite` (`devuart.c:494`).
    fn write(&mut self, c: &mut Chan, data: &[u8], _off: u64) -> Result<usize, String> {
        if c.qid.is_dir() {
            return Err(EPERM.into());
        }
        let i = self.index(c)?;
        match nettype(c.qid.path) {
            NDATAQID => self.datawrite(i, data),
            NCTLQID => self.ctlwrite(i, data),
            _ => Ok(data.len()),
        }
    }

    /// `uartstat` (`devuart.c:262`) — `devstat` over `uartdir`; a
    /// directory is not in the table, and is named from its path
    /// (`dev.c:272`).
    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        if c.qid.is_dir() {
            let elem = c.path.rsplit('/').next().unwrap_or(&c.path).to_string();
            let eve = self.eve.borrow();
            return Ok(crate::dev::devdir(c, c.qid, &elem, 0, &eve, &eve, DMDIR | 0o555).conv_d2m());
        }
        self.dir(c, c.qid.path).map(|d| d.conv_d2m()).ok_or_else(|| ENONEXIST.into())
    }

    /// `uartwstat` (`devuart.c:544`): eve may change the mode of a line's
    /// data and ctl files, together.
    fn wstat(&mut self, c: &mut Chan, edir: &[u8]) -> Result<(), String> {
        if !crate::dev::iseve(&self.eve, &self.user()) {
            return Err(EPERM.into());
        }
        if c.qid.is_dir() || nettype(c.qid.path) == NSTATQID {
            return Err(EPERM.into());
        }
        let i = self.index(c)?;
        let d = Dir::conv_m2d(edir).ok_or(ESHORTSTAT)?;
        if d.mode != !0 {
            self.perm[i] = d.mode;
        }
        Ok(())
    }

    fn remove(&mut self, _c: &mut Chan) -> Result<(), String> {
        Err(EPERM.into())
    }

    /// `uartclose` (`devuart.c:319`): the last close of a line's data or
    /// ctl file closes its input, hangs up and drains its output, and
    /// disables it. The drain cannot wait here — see the module's comment.
    fn close(&mut self, c: &mut Chan) {
        if c.qid.is_dir() || c.flag & COPEN == 0 {
            return;
        }
        if !matches!(nettype(c.qid.path), NDATAQID | NCTLQID) {
            return;
        }
        let Ok(i) = self.index(c) else { return };
        let u = &mut self.uart[i];
        u.opens = u.opens.saturating_sub(1);
        if u.opens > 0 {
            return;
        }
        let procs = self.up.borrow().procs.clone();
        let mut procs = procs.borrow_mut();
        if let Some(q) = u.iq.as_mut() {
            q.close();
        }
        if let Some(s) = u.istage.as_mut() {
            s.clear();
        }
        if let Some(q) = u.oq.as_mut() {
            q.hangup();
        }
        self.uartkick(i, &mut procs);
        let u = &mut self.uart[i];
        if let Some(q) = u.oq.as_mut() {
            q.close();
        }
        let dev = u.dev;
        wakeall(&mut procs, dev);
        self.uartdisable(i);
        let u = &mut self.uart[i];
        u.dcd = false;
        u.dsr = false;
        u.dohup = false;
    }

    /// Another reference to an open line (`incref`): one more for `opens`
    /// to count down.
    fn incref(&mut self, c: &Chan) {
        if c.flag & COPEN == 0 || !matches!(nettype(c.qid.path), NDATAQID | NCTLQID) {
            return;
        }
        if let Ok(i) = self.index(c) {
            self.uart[i].opens += 1;
        }
    }
}

/// `readstr`'s and `readnum`'s offset: the part of `s` from `off`.
fn slice(s: &[u8], n: usize, off: u64) -> Vec<u8> {
    let off = off as usize;
    if off >= s.len() {
        return Vec::new();
    }
    s[off..(off + n).min(s.len())].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chan::mode::{OREAD, ORDWR, OWRITE};
    use crate::proc::State;
    use std::sync::{Arc, Mutex};

    /// A line to nowhere but the test: what the kernel sends lands in
    /// `out`, what the test puts in `wire` arrives at the next interrupt.
    #[derive(Default)]
    struct Wire {
        wire: Vec<u8>,
        out: Vec<u8>,
        carrier: bool,
        /// How much the transmitter takes per kick; `None` is all of it.
        take: Option<usize>,
    }

    struct Line(Arc<Mutex<Wire>>);

    impl PhysUart for Line {
        fn name(&self) -> &str {
            "test"
        }
        fn enable(&mut self, u: &mut Uart, _ie: bool) {
            u.dcd = self.0.lock().unwrap().carrier;
        }
        fn disable(&mut self, _u: &mut Uart) {}
        fn kick(&mut self, u: &mut Uart) {
            if !u.cts || u.blocked {
                return;
            }
            let mut w = self.0.lock().unwrap();
            let mut room = w.take.unwrap_or(usize::MAX);
            while room > 0 {
                if u.op >= u.ostage.len() && u.stageoutput() == 0 {
                    break;
                }
                let n = (u.ostage.len() - u.op).min(room);
                w.out.extend_from_slice(&u.ostage[u.op..u.op + n]);
                u.op += n;
                room -= n;
            }
        }
        fn dobreak(&mut self, _u: &mut Uart, _ms: i32) {}
        fn baud(&mut self, u: &mut Uart, baud: i32) -> Result<(), ()> {
            if baud <= 0 {
                return Err(());
            }
            u.baud = baud;
            Ok(())
        }
        fn bits(&mut self, u: &mut Uart, bits: i32) -> Result<(), ()> {
            if !(5..=8).contains(&bits) {
                return Err(());
            }
            u.bits = bits;
            Ok(())
        }
        fn stop(&mut self, u: &mut Uart, stop: i32) -> Result<(), ()> {
            if !(1..=2).contains(&stop) {
                return Err(());
            }
            u.stop = stop;
            Ok(())
        }
        fn parity(&mut self, u: &mut Uart, parity: u8) -> Result<(), ()> {
            if !b"eon".contains(&parity) {
                return Err(());
            }
            u.parity = parity;
            Ok(())
        }
        fn modemctl(&mut self, u: &mut Uart, on: bool) {
            u.modem = on;
        }
        fn rts(&mut self, _u: &mut Uart, _on: bool) {}
        fn dtr(&mut self, _u: &mut Uart, _on: bool) {}
        fn status(&mut self, u: &Uart) -> String {
            format!("b{} l{} p{} s{}\n", u.baud, u.bits, u.parity as char, u.stop)
        }
        fn fifo(&mut self, _u: &mut Uart, _level: i32) {}
        fn interrupt(&mut self, u: &mut Uart) -> bool {
            let mut w = self.0.lock().unwrap();
            for ch in std::mem::take(&mut w.wire) {
                u.recv(ch);
            }
            if u.hup_dcd && u.dcd && !w.carrier {
                u.dohup = true;
            }
            u.dcd = w.carrier;
            true
        }
    }

    fn uart() -> (UartDev, Arc<Mutex<Wire>>, Rc<RefCell<Procs>>) {
        let procs = Rc::new(RefCell::new(Procs::new(Chan::attach(DevId::Root, 0))));
        procs.borrow_mut().rfork(1, crate::proc::rf::PROC).unwrap();
        let up = Rc::new(RefCell::new(Up { pid: 1, procs: procs.clone() }));
        let wire = Arc::new(Mutex::new(Wire { carrier: true, ..Wire::default() }));
        let mut d = UartDev::new(up, vec![(Uart::new("test", 0), Box::new(Line(wire.clone())) as Box<dyn PhysUart>)]);
        // The process is eve, so the 0660 files are its own.
        let eve = Eve::default();
        *eve.borrow_mut() = procs.borrow().user(1).unwrap_or_default();
        d.seteve(eve);
        (d, wire, procs)
    }

    fn open(d: &mut UartDev, name: &str, omode: u16) -> Chan {
        let dir = d.attach("").unwrap();
        let c = d.walk(&dir, name).unwrap().expect(name);
        d.open(c, omode).unwrap()
    }

    fn by(d: &UartDev, pid: Pid) {
        d.up.borrow_mut().pid = pid;
    }

    /// `uartreset`'s directory: three files a line, `.` not among them
    /// (`devgen` skips it, `dev.c:100`).
    #[test]
    fn the_directory_is_three_files_a_line() {
        let (mut d, _, _) = uart();
        let dir = d.attach("").unwrap();
        let mut dir = d.open(dir, OREAD).unwrap();
        let b = d.read(&mut dir, 4096, 0).unwrap();
        let names: Vec<String> = Dir::parse_all(&b).into_iter().map(|d| d.name).collect();
        assert_eq!(names, ["eia0", "eia0ctl", "eia0status"]);
    }

    /// What is written goes out on the line; what comes in on the line is
    /// read, once the clock has taken it in.
    #[test]
    fn a_write_goes_out_and_what_arrives_is_read() {
        let (mut d, wire, _) = uart();
        let mut c = open(&mut d, "eia0", ORDWR);
        assert_eq!(d.write(&mut c, b"hello", 0).unwrap(), 5);
        assert_eq!(wire.lock().unwrap().out, b"hello");
        wire.lock().unwrap().wire.extend_from_slice(b"back");
        d.uartclock(1);
        assert_eq!(d.read(&mut c, 64, 0).unwrap(), b"back");
    }

    /// A reader of an empty line sleeps, and the clock that takes the input
    /// in wakes it.
    #[test]
    fn a_reader_sleeps_until_the_clock_takes_input_in() {
        let (mut d, wire, procs) = uart();
        let mut c = open(&mut d, "eia0", ORDWR);
        d.read(&mut c, 64, 0).unwrap();
        assert_eq!(procs.borrow().state(1), State::Wakeme);
        wire.lock().unwrap().wire.extend_from_slice(b"x");
        d.uartclock(1);
        assert_eq!(procs.borrow().state(1), State::Ready);
        assert_eq!(d.read(&mut c, 64, 0).unwrap(), b"x");
    }

    /// `uartenable`'s defaults, and the ctl commands setting the line —
    /// `Ebadarg` for one the hardware refuses.
    #[test]
    fn the_ctl_file_sets_the_line() {
        let (mut d, _, _) = uart();
        let mut ctl = open(&mut d, "eia0ctl", ORDWR);
        let mut st = open(&mut d, "eia0status", OREAD);
        assert_eq!(d.read(&mut st, 99, 0).unwrap(), b"b9600 l8 pn s1\n", "uartenable's defaults");
        d.write(&mut ctl, b"b115200 l7 pe s2", 0).unwrap();
        assert_eq!(d.read(&mut st, 99, 0).unwrap(), b"b115200 l7 pe s2\n");
        assert_eq!(d.write(&mut ctl, b"l9", 0), Err(EBADARG.into()));
        // *"readnum(offset, buf, n, NETID(c->qid.path), NUMSIZE)"*.
        assert_eq!(d.read(&mut ctl, 99, 0).unwrap(), b"          0 ");
    }

    /// `c1`: the carrier dropping hangs the line up, and a reader sees the
    /// end of file.
    #[test]
    fn with_c1_losing_the_carrier_is_end_of_file() {
        let (mut d, wire, procs) = uart();
        let mut ctl = open(&mut d, "eia0ctl", ORDWR);
        let mut c = open(&mut d, "eia0", ORDWR);
        d.write(&mut ctl, b"c1", 0).unwrap();
        d.read(&mut c, 64, 0).unwrap();
        assert_eq!(procs.borrow().state(1), State::Wakeme);
        wire.lock().unwrap().carrier = false;
        d.uartclock(1);
        assert_eq!(procs.borrow().state(1), State::Ready, "the hangup woke it");
        assert_eq!(d.read(&mut c, 64, 0).unwrap(), b"");
        assert!(!d.waiting(), "nobody on the line: nothing to wait for");
    }

    /// `x1` and `^S`: output stops until `^Q`, and a ctl command that
    /// drains output waits for it — then carries on from that command.
    #[test]
    fn xoff_holds_output_and_a_draining_command_waits_for_it() {
        let (mut d, wire, procs) = uart();
        let mut ctl = open(&mut d, "eia0ctl", ORDWR);
        let mut c = open(&mut d, "eia0", ORDWR);
        d.write(&mut ctl, b"x1", 0).unwrap();
        wire.lock().unwrap().wire.push(CTLS);
        d.uartclock(1);
        d.write(&mut c, b"held", 0).unwrap();
        assert!(wire.lock().unwrap().out.is_empty(), "blocked by ^S");
        let pid = procs.borrow_mut().rfork(1, crate::proc::rf::PROC).unwrap();
        by(&d, pid);
        assert_eq!(d.write(&mut ctl, b"c1 b300", 0).unwrap(), 0, "asleep in uartdrainoutput");
        assert_eq!(procs.borrow().state(pid), State::Wakeme);
        wire.lock().unwrap().wire.push(CTLQ);
        d.uartclock(100_000_000);
        // `ctsbackoff` is 2: the second clock kicks.
        d.uartclock(200_000_000);
        assert_eq!(wire.lock().unwrap().out, b"held");
        assert_eq!(procs.borrow().state(pid), State::Ready, "drained: woken");
        assert_eq!(d.write(&mut ctl, b"c1 b300", 0).unwrap(), 7);
        assert_eq!(d.uart[0].baud, 300);
    }

    /// Only eve may open a line: its files are eve's, `0660`
    /// (`devpermcheck` through `devopen`, `dev.c:371`).
    #[test]
    fn a_line_is_eves() {
        let (mut d, _, procs) = uart();
        procs.borrow_mut().get_mut(1).unwrap().user = "none".into();
        let dir = d.attach("").unwrap();
        let c = d.walk(&dir, "eia0").unwrap().unwrap();
        assert!(d.open(c, OWRITE).is_err());
    }

    /// The last close hangs the line up and disables it; the next open
    /// starts clean.
    #[test]
    fn the_last_close_disables_the_line() {
        let (mut d, wire, _) = uart();
        let mut c = open(&mut d, "eia0", ORDWR);
        wire.lock().unwrap().wire.extend_from_slice(b"stale");
        d.uartclock(1);
        d.close(&mut c);
        assert!(!d.uart[0].enabled);
        let mut c = open(&mut d, "eia0", ORDWR);
        wire.lock().unwrap().wire.extend_from_slice(b"new");
        d.uartclock(100_000_000);
        assert_eq!(d.read(&mut c, 64, 0).unwrap(), b"new", "what was queued went with the close");
    }
}
