//! `ipnx` — Saranos on a terminal.
//!
//! The host is INSIDE Saranos, not underneath it: the kernel is wasm and cannot
//! run without a host to give it memory, a console and a screen; the host has
//! nothing to do without the kernel. The interface between them is 9P and the
//! effect list, and nothing else.
//!
//! What this host owes the kernel is short, and it is the whole of the host
//! contract: perform each [`Effect`] and answer it, keep the clock, own the
//! console, and own the real root of host storage — including the containment
//! check, which lives here because this is the side that can enforce it.

use ipnx_kernel::{Effect, Kernel};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("usage: ipnx [--host <dir>] [<command> ...]");
        eprintln!("  boots Saranos on this terminal; with no command, to a shell");
        return;
    }

    let mut k = Kernel::new();
    k.boot("/bin/init", &[]);
    let status = pump(&mut k);
    std::process::exit(status);
}

/// Drain the kernel's effects, perform them, and hand back the replies. This
/// loop IS the host: everything else it does is in service of one of these arms.
fn pump(k: &mut Kernel) -> i32 {
    loop {
        let effects = k.take_effects();
        if effects.is_empty() {
            return 0;
        }
        for e in effects {
            match e {
                // Transport, and the host decides what is on the other end.
                // Channel 1 is this terminal's own stdout: the console is a
                // file THIS SIDE serves, not something the kernel knows about.
                Effect::Send { chan: 1, data, .. } => {
                    let mut out = std::io::stdout();
                    let _ = out.write_all(&data);
                    let _ = out.flush();
                }
                Effect::Send { chan, .. } => {
                    eprintln!("ipnx: nothing is serving channel {chan} yet");
                    return 1;
                }
                Effect::Shutdown { status } => return status,
                Effect::Spawn { pid, path, .. } => {
                    // Instantiation is the next piece of work: a wasm engine,
                    // a mailbox per process, and the guard that makes fork
                    // return twice. Until then, say so rather than pretend.
                    eprintln!("ipnx: cannot yet instantiate {path} as pid {pid}");
                    return 1;
                }
                other => {
                    eprintln!("ipnx: unimplemented effect: {other:?}");
                    return 1;
                }
            }
        }
    }
}
