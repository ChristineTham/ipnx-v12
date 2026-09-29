//! The serial line — `#t`'s hardware on this host.
//!
//! Plan 9's PC has an i8250, and `uarti8250.c` is its `PhysUart`; this host
//! has a line whose far end is a thread of its own — the surface, or a test
//! (docs/surface.md, docs/implementation.md P8). [`Line`] is the wire, both
//! ends; [`Eia`] is the chip at the kernel's end, doing what the i8250's
//! functions do with the parts of it a line with no hardware has.
//!
//! What is not the i8250's, and why: a host line has no baud-rate generator
//! and no FIFO, so a setting is recorded and reported, not programmed; it
//! takes every byte it is given, so *Thr Empty* is always true; and its
//! modem lines are the far end — DSR and DCD are up while something is
//! connected, CTS always, as a null-modem cable wires them.

use ipnx_kernel::devuart::{PhysUart, Uart};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Wire {
    /// Sent by the far end, not yet taken by the interrupt.
    toward: Vec<u8>,
    /// Sent by the kernel, not yet read by the far end.
    from: VecDeque<u8>,
    /// Something is connected at the far end.
    carrier: bool,
}

/// Both ends of a serial line. A clone is the same line.
#[derive(Clone, Default)]
pub struct Line(Arc<(Mutex<Wire>, Condvar)>);

impl Line {
    /// A line with nothing at the far end.
    pub fn new() -> Line {
        Line::default()
    }

    /// The far end connects: DSR and DCD come up.
    pub fn connect(&self) {
        self.0 .0.lock().unwrap().carrier = true;
    }

    /// The far end goes: DSR and DCD drop.
    pub fn hangup(&self) {
        self.0 .0.lock().unwrap().carrier = false;
        self.0 .1.notify_all();
    }

    /// The far end sends.
    pub fn send(&self, b: &[u8]) {
        self.0 .0.lock().unwrap().toward.extend_from_slice(b);
    }

    /// The far end reads exactly `n` bytes, waiting up to `timeout` for
    /// them; fewer is what had come when it ran out.
    pub fn recv(&self, n: usize, timeout: Duration) -> Vec<u8> {
        let end = Instant::now() + timeout;
        let (m, cv) = &*self.0;
        let mut w = m.lock().unwrap();
        while w.from.len() < n {
            let now = Instant::now();
            if now >= end {
                break;
            }
            w = cv.wait_timeout(w, end - now).unwrap().0;
        }
        let k = n.min(w.from.len());
        w.from.drain(..k).collect()
    }
}

/// The registers `i8250status` reads back: `Lcr`'s word length, parity and
/// stop bits, `Mcr`'s DTR and RTS, `Ier`'s modem-status enable, and the
/// FIFO level. A cleared `Lcr` is five bits, no parity, one stop bit.
struct Regs {
    wls: i32,
    parity: u8,
    stb: i32,
    dtr: bool,
    rts: bool,
    ems: bool,
    fena: i32,
}

impl Default for Regs {
    fn default() -> Regs {
        Regs { wls: 5, parity: b'n', stb: 1, dtr: false, rts: false, ems: false, fena: 0 }
    }
}

/// The chip at the kernel's end of a [`Line`].
pub struct Eia {
    line: Line,
    regs: Regs,
}

impl Eia {
    /// What `pnp` hands over: the uart and its hardware.
    pub fn found(name: &str, line: Line) -> (Uart, Box<dyn PhysUart>) {
        (Uart::new(name, 0), Box::new(Eia { line, regs: Regs::default() }))
    }
}

impl PhysUart for Eia {
    fn name(&self) -> &str {
        "ipnx"
    }

    /// `i8250enable` (`uarti8250.c:553`): DTR and RTS on, and the interrupt
    /// handler called once to pick up the line's state.
    fn enable(&mut self, u: &mut Uart, ie: bool) {
        self.dtr(u, true);
        self.rts(u, true);
        if ie {
            self.interrupt(u);
        }
    }

    /// `i8250disable` (`uarti8250.c:531`): DTR, RTS and the FIFO off.
    fn disable(&mut self, u: &mut Uart) {
        self.dtr(u, false);
        self.rts(u, false);
        self.fifo(u, 0);
        self.regs.ems = false;
    }

    /// `i8250kick` (`uarti8250.c:438`), with a transmitter that is never
    /// full: everything staged goes.
    fn kick(&mut self, u: &mut Uart) {
        if !u.cts || u.blocked {
            return;
        }
        let (m, cv) = &*self.line.0;
        let mut w = m.lock().unwrap();
        loop {
            if u.op >= u.ostage.len() && u.stageoutput() == 0 {
                break;
            }
            w.from.extend(&u.ostage[u.op..]);
            u.op = u.ostage.len();
        }
        cv.notify_all();
    }

    fn dobreak(&mut self, _u: &mut Uart, _ms: i32) {}

    /// `i8250baud` (`uarti8250.c:395`) refuses a rate of 0 or less; there
    /// is no divisor to set.
    fn baud(&mut self, u: &mut Uart, baud: i32) -> Result<(), ()> {
        if baud <= 0 {
            return Err(());
        }
        u.baud = baud;
        Ok(())
    }

    /// `i8250bits` (`uarti8250.c:362`): 5 to 8.
    fn bits(&mut self, u: &mut Uart, bits: i32) -> Result<(), ()> {
        if !(5..=8).contains(&bits) {
            return Err(());
        }
        self.regs.wls = bits;
        u.bits = bits;
        Ok(())
    }

    /// `i8250stop` (`uarti8250.c:336`): 1 or 2.
    fn stop(&mut self, u: &mut Uart, stop: i32) -> Result<(), ()> {
        if !(1..=2).contains(&stop) {
            return Err(());
        }
        self.regs.stb = stop;
        u.stop = stop;
        Ok(())
    }

    /// `i8250parity` (`uarti8250.c:307`): `e`, `o` or `n`.
    fn parity(&mut self, u: &mut Uart, parity: u8) -> Result<(), ()> {
        if !b"eon".contains(&parity) {
            return Err(());
        }
        self.regs.parity = parity;
        u.parity = parity;
        Ok(())
    }

    /// `i8250modemctl` (`uarti8250.c:282`): hardware flow control, reading
    /// CTS — always up here — and the FIFO it needs.
    fn modemctl(&mut self, u: &mut Uart, on: bool) {
        self.regs.ems = on;
        u.modem = on;
        u.cts = true;
        self.fifo(u, on as i32);
    }

    fn rts(&mut self, _u: &mut Uart, on: bool) {
        self.regs.rts = on;
    }

    fn dtr(&mut self, _u: &mut Uart, on: bool) {
        self.regs.dtr = on;
    }

    /// `i8250status` (`uarti8250.c:154`), field for field.
    fn status(&mut self, u: &Uart) -> String {
        let r = &self.regs;
        format!(
            "b{} c{} d{} e{} l{} m{} p{} r{} s{} i{}\n\
             dev({}) type({}) framing({}) overruns({}) berr({}) serr({}){}{}{}{}\n",
            u.baud,
            u.hup_dcd as i32,
            u.dsr as i32,
            u.hup_dsr as i32,
            r.wls,
            r.ems as i32,
            r.parity as char,
            r.rts as i32,
            r.stb,
            r.fena,
            u.dev,
            u.kind,
            u.ferr,
            u.oerr,
            u.berr,
            u.serr,
            if u.cts { " cts" } else { "" },
            if u.dsr { " dsr" } else { "" },
            if u.dcd { " dcd" } else { "" },
            "",
        )
    }

    fn fifo(&mut self, _u: &mut Uart, level: i32) {
        self.regs.fena = level;
    }

    /// `i8250interrupt` (`uarti8250.c:463`): what the far end sent goes to
    /// `uartrecv`; a change in the modem lines is `Ims`, with its hangup
    /// (*"if(uart->hup_dcd && uart->dcd && !old) uart->dohup = 1"*); and
    /// the transmitter is empty, so the device is to kick.
    fn interrupt(&mut self, u: &mut Uart) -> bool {
        let (toward, carrier) = {
            let mut w = self.line.0 .0.lock().unwrap();
            (std::mem::take(&mut w.toward), w.carrier)
        };
        for ch in toward {
            u.recv(ch);
        }
        if carrier != u.dsr {
            if u.hup_dsr && u.dsr && !carrier {
                u.dohup = true;
            }
            u.dsr = carrier;
        }
        if carrier != u.dcd {
            if u.hup_dcd && u.dcd && !carrier {
                u.dohup = true;
            }
            u.dcd = carrier;
        }
        true
    }
}
