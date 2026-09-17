use ipnx_kernel::Kernel;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("usage: ipnx [<command> ...]");
        eprintln!("  boots Saranos on this terminal; with no command, to a shell");
        return;
    }

    // The host gives the kernel a machine to run on and then gets out of the
    // way. What it does NOT do is answer for devices: a console, a clock, a
    // store are file servers in userspace, reached by processes over 9P, and
    // the kernel has no call that names any of them.
    //
    // Next: instantiation. `exec` resolves a path through the process's
    // namespace, reads the image, and the engine turns it into a running
    // process. That engine is the host's, and it is the only thing here the
    // kernel cannot do for itself.
    let k = Kernel::new();
    eprintln!("ipnx: kernel up, {} process", k.procs.count());
    eprintln!("ipnx: no engine yet — exec is the next piece of work");
}
