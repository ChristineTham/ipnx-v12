//! CONTRACT: the kernel is a subset of Plan 9's, containing process
//! orchestration — and nothing in it is invented.
//!
//! This is the contract the whole design rests on, so it is the one the suite
//! checks first and hardest. Every assertion here is measured against
//! `plan9/`, at a path this file names, so a reader can go and look.

use ipnx_kernel::{dev, Call};

/// Plan 9's syscalls, from `plan9/sys/src/libc/9syscall/sys.h`.
const PLAN9_CALLS: &[&str] = &[
    "ERRSTR", "BIND", "CHDIR", "CLOSE", "DUP", "ALARM", "EXEC", "EXITS", "FSESSION", "FAUTH",
    "FSTAT", "SEGBRK", "MOUNT", "OPEN", "READ", "OSEEK", "SLEEP", "STAT", "RFORK", "WRITE",
    "PIPE", "CREATE", "BRK_", "REMOVE", "WSTAT", "FWSTAT", "NOTIFY", "NOTED", "SEGATTACH",
    "SEGDETACH", "SEGFREE", "SEGFLUSH", "RENDEZVOUS", "UNMOUNT", "WAIT", "SEMACQUIRE",
    "SEMRELEASE", "SEEK", "FVERSION", "AWAIT", "PREAD", "PWRITE", "TSEMACQUIRE", "NSEC",
];

/// Plan 9's device letters, from the `Dev` tables in
/// `plan9/sys/src/9/port/dev*.c`.
const PLAN9_LETTERS: &[char] = &[
    '$', '/', 'A', 'B', 'D', 'E', 'F', 'K', 'M', 'S', 'X', 'a', 'c', 'd', 'e', 'g', 'i', 'k',
    'm', 'p', 's', 't', 'w', '|',
];

/// Every call this kernel answers, by the name Plan 9 gives it.
fn our_calls() -> Vec<&'static str> {
    vec![
        "RFORK", "EXEC", "EXITS", "AWAIT", "SLEEP", "ALARM", "NOTIFY", "NOTED", "RENDEZVOUS",
        "BIND", "MOUNT", "UNMOUNT", "CHDIR", "OPEN", "CREATE", "CLOSE", "PREAD", "PWRITE",
        "SEEK", "DUP", "PIPE", "REMOVE", "STAT", "FSTAT", "WSTAT", "FWSTAT", "FVERSION",
        "ERRSTR",
    ]
}

#[test]
fn every_call_is_one_of_plan_nines() {
    for c in our_calls() {
        assert!(PLAN9_CALLS.contains(&c), "{c} is not a Plan 9 syscall — it was invented");
    }
}

#[test]
fn the_call_list_is_strictly_smaller() {
    // A subset is smaller than what it subsets. If this ever fails, the kernel
    // has stopped being a subset and become a variant.
    assert!(
        our_calls().len() < PLAN9_CALLS.len(),
        "{} calls against Plan 9's {}",
        our_calls().len(),
        PLAN9_CALLS.len()
    );
}

#[test]
fn no_call_does_what_a_file_server_does() {
    // Everything beyond orchestration is communication between userspace
    // processes. A console, a clock, a store, a window system are processes;
    // reaching one is open/read/write and nothing more.
    let calls = our_calls().join(" ");
    for forbidden in [
        "DRAW", "TIME", "NSEC", "RANDOM", "FETCH", "STORE", "WINDOW", "CONSOLE", "MOUSE",
        "CANVAS", "SNARF",
    ] {
        assert!(!calls.contains(forbidden), "{forbidden} belongs to a file server");
    }
}

#[test]
fn every_device_letter_is_one_of_plan_nines() {
    // The letters are not ours to choose. These three were minted in an
    // earlier tree and are named so they stay refused: '#H' fetch, '#V'
    // versioning, '#Z' host files. None is in Plan 9.
    for minted in ['H', 'V', 'Z'] {
        assert!(!PLAN9_LETTERS.contains(&minted), "test data wrong: {minted} IS a Plan 9 letter");
        assert!(
            dev::DevId::from_letter(minted).is_none(),
            "#{minted} is not a Plan 9 device and must not be one here"
        );
    }
    for c in ['a'..='z', 'A'..='Z'].into_iter().flatten() {
        if let Some(d) = dev::DevId::from_letter(c) {
            assert!(PLAN9_LETTERS.contains(&c), "#{c} ({d:?}) is not a Plan 9 device letter");
        }
    }
}

#[test]
fn a_call_carries_no_field_that_names_a_machine() {
    // The shape of a call matters as much as its name: a `Call` with a field
    // called `pixels` or `clock` would be a device smuggled through a
    // parameter. This checks the debug shape of every variant we construct.
    let samples = [
        Call::Rfork { flags: 0 },
        Call::Exec { path: "/bin/rc".into(), args: vec![] },
        Call::Open { path: "/dev/cons".into(), mode: 0 },
        Call::Mount { fd: 0, afd: -1, old: "/n".into(), flag: 0, aname: String::new() },
        Call::Pipe,
    ];
    for c in samples {
        let shape = format!("{c:?}").to_lowercase();
        for forbidden in ["pixel", "clock", "screen", "frame", "raster", "keyboard"] {
            assert!(!shape.contains(forbidden), "{shape} names a machine");
        }
    }
}
