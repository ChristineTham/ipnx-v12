//! `ipnx` — Saranos on a terminal.
//!
//! What the embedding is for, in Christine's words:
//!
//! > *"Only saranos knows about the host… I am a macOS app. I have a screen, a
//! > keyboard and a mouse. I will serve these as virtual devices to the IPNX
//! > kernel, which I am going to start."*
//!
//! So it starts the kernel and serves the machine; it does not answer for
//! devices the kernel holds, because the kernel holds none of the machine's.

use ipnx_kernel::{devroot::Root, Kernel};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("usage: ipnx [<command> ...]");
        eprintln!("  boots Saranos on this terminal; with no command, to a shell");
        return;
    }

    // The kernel carries a root holding what the first process needs. There is
    // no userspace yet, so there is nothing real to put in it.
    let mut root = Root::new();
    root.addbootfile("init", b"# the first process, when there is one\n".to_vec());

    let mut k = match Kernel::new(root) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("ipnx: {e}");
            std::process::exit(1);
        }
    };

    match k.exec_image(1, "/init") {
        Ok(image) => eprintln!("ipnx: resolved /init through the namespace, {} bytes", image.len()),
        Err(e) => {
            eprintln!("ipnx: /init: {e}");
            std::process::exit(1);
        }
    }

    // And here it stops. Turning an image into a running process is the
    // machine-dependent half of exec — segments and the MMU in Plan 9, module
    // instantiation here — and it is a deviation that has not been approved.
    eprintln!("ipnx: no engine: instantiating a process is not designed yet");
}
