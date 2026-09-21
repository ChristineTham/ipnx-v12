//! Have we reached functional equivalence with the demo?
//!
//! That is the only question this suite asks. It is not here to lock the
//! design — the design is argued in the documents and enforced by review, and
//! a test that asserts "the call list is a subset" would freeze a decision
//! rather than measure a system. What is measured here is what a person can
//! DO, taken from the demo itself: `demo/index.md` says *"your home listed on
//! the left, files open in tabs, `rc` running below"* and that the C, Go and
//! Python toolchains stream in behind you.
//!
//! So this file is a checklist against the live demo at
//! <https://christham.net/ipnx-v12/>, and it starts almost entirely unreached.
//! That is the point: it is a distance, and it shrinks as phases land.
//!
//! **Moving a line to `Reached` is a claim that a person can do that thing.**
//! Not that the code exists, not that a unit test passes — that it works.
//!
//! Equivalence is in FEATURES, not in mechanism. Nothing here says how a thing
//! is done: the rebuild is free to reach any of these by another route, and
//! several of them it should. A check that pinned the method would be locking
//! the old implementation in by the back door.

use std::fmt;

/// Boot the system on a scripted console and hand back what the screen
/// showed — the whole of it, a person's view. Every check below is this and
/// nothing else: **no kernel internals, no unit-test fixtures**, because a
/// behaviour is reached when a person can do it.
///
/// It needs `userspace/mk.sh` to have run, as the host's own tests do.
fn typing(keys: &str) -> String {
    let store = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../userspace/root");
    let term = ipnx::Term::typing(keys);
    let fs = ipnx::store::Store::new(&store).expect("no userspace/root — run userspace/mk.sh");
    ipnx::startboot(
        &[ipnx::BOOT.to_string()],
        &[],
        Box::new(term.clone()),
        Some(Box::new(fs)),
    )
    .expect("the system did not boot");
    term.screen()
}

/// A check: `Ok(())` if a person really can do the thing.
type Check = fn() -> Result<(), String>;

fn wants(out: &str, want: &str) -> Result<(), String> {
    if out.contains(want) {
        Ok(())
    } else {
        Err(format!("no {want:?} in {out:?}"))
    }
}

#[derive(PartialEq)]
enum State {
    /// A person can do this today.
    Reached,
    /// Not yet, and this is the phase that will deliver it.
    Pending(&'static str),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::Reached => write!(f, "reached"),
            State::Pending(p) => write!(f, "not yet  ({p})"),
        }
    }
}

struct Behaviour {
    /// What a person does, in their words rather than ours.
    what: &'static str,
    state: State,
    /// How we would know: the check that decides `Reached`, described so the
    /// claim can be audited rather than taken on trust.
    how: &'static str,
    /// And the check itself. **`Reached` without one is refused** — that is
    /// the rule this file has always stated, and until this field existed
    /// there was no way to satisfy it, so the count could only ever be zero.
    check: Option<Check>,
}

/// The demo's FEATURES — what the system can do, with no claim about how any
/// of it looks.
///
/// The new surface will look very different, so nothing here says "on the
/// left", "as a tab" or "below". Those are the current demo's presentation,
/// and wiring conformance to them would hold the rebuild to a design it is
/// meant to replace. What survives the surface changing is the capability.
fn demo() -> Vec<Behaviour> {
    use State::*;
    vec![
        Behaviour {
            what: "the system boots and gives you a shell",
            state: Reached,
            how: "start it; a shell takes what you type and answers",
            check: Some(|| {
                let out = typing("echo hello from the shell\n");
                wants(&out, "hello from the shell")?;
                wants(&out, "%")
            }),
        },
        Behaviour {
            what: "list a directory",
            state: Reached,
            how: "the names it holds come back",
            check: Some(|| {
                let out = typing("ls /\n");
                for name in ["boot", "bin", "dev", "env", "proc", "srv", "etc", "rc"] {
                    wants(&out, name)?;
                }
                Ok(())
            }),
        },
        Behaviour {
            what: "read a file's contents",
            state: Reached,
            how: "the text comes back",
            check: Some(|| wants(&typing("cat /etc/motd\n"), "Saranos")),
        },
        Behaviour {
            what: "run a command, and pipe one into another",
            state: Reached,
            how: "the second sees what the first wrote",
            check: Some(|| wants(&typing("echo shouting | tr a-z A-Z\n"), "SHOUTING")),
        },
        Behaviour {
            what: "run a Go program",
            state: Pending("P7"),
            how: "it runs and prints what it printed before",
            check: None,
        },
        Behaviour {
            what: "run Python",
            state: Pending("P7"),
            how: "it starts, imports from its library, and computes",
            check: None,
        },
        Behaviour {
            what: "a package becomes available without installing anything into the tree",
            state: Pending("P7"),
            how: "the program runs afterwards; the tree is no bigger than before",
            check: None,
        },
        Behaviour {
            what: "processes have their own namespaces and do not disturb each other",
            state: Reached,
            how: "one changes what a name means; the other still sees the old one",
            check: Some(|| {
                // `rfork n` gives the subshell its own namespace; the bind
                // inside it is invisible outside.
                let out = typing(
                    "mkdir /tmp/alt\n\
                     echo ALT >/tmp/alt/motd\n\
                     @{rfork n; bind /tmp/alt /etc; echo IN `{cat /etc/motd}}\n\
                     echo OUT `{cat /etc/motd}\n",
                );
                wants(&out, "IN ALT")?;
                if out.contains("OUT ALT") {
                    return Err(format!("the bind escaped its process: {out:?}"));
                }
                wants(&out, "OUT Saranos")
            }),
        },
        Behaviour {
            what: "the windowing system can show several things at once and act on them",
            state: Pending("P8"),
            how: "more than one is open; acting on one leaves the others alone",
            check: None,
        },
        Behaviour {
            what: "what you can do with a thing depends on what it is",
            state: Pending("P8"),
            how: "two kinds of content offer different actions",
            check: None,
        },
        Behaviour {
            what: "the whole system runs in a browser as well as a terminal",
            state: Pending("P8"),
            how: "the same userspace, reached through a page",
            check: None,
        },
        Behaviour {
            what: "a language toolchain becomes usable during a session, not before it",
            state: Pending("P8"),
            how: "the system is usable first; the toolchain works later in the same session",
            check: None,
        },
    ]
}

#[test]
fn functional_equivalence_with_the_demo() {
    let all = demo();
    let reached = all.iter().filter(|b| b.state == State::Reached).count();

    println!("\n  functional equivalence to the demo: {reached} of {}\n", all.len());
    for b in &all {
        let mark = if b.state == State::Reached { "x" } else { " " };
        println!("  [{mark}] {:<58} {}", b.what, b.state);
    }
    println!();

    // A behaviour claimed as reached must be demonstrated by the check its
    // `how` describes. A line moved here without one is a claim with nothing
    // behind it — and it is RUN, not merely present, so the claim is made
    // good on this boot of this system and not on a memory of one.
    let mut broken = Vec::new();
    for b in all.iter().filter(|b| b.state == State::Reached) {
        match b.check {
            None => {
                panic!("'{}' is claimed reached but has no check wired up ({})", b.what, b.how)
            }
            Some(f) => {
                if let Err(e) = f() {
                    broken.push(format!("  '{}' is claimed reached and is not: {e}", b.what));
                }
            }
        }
    }
    assert!(broken.is_empty(), "\n{}\n", broken.join("\n"));

    // AND THE SUITE FAILS UNTIL IT IS CONFORMANT. This is not pessimism; it is
    // what the word means. An earlier version of this file printed "0 of 12"
    // and then reported `ok`, so `cargo test` was green while the system did
    // nothing at all — the same false signal the suite exists to prevent.
    //
    // For day-to-day work run the unit tests: `cargo test -p ipnx-kernel`.
    assert_eq!(
        reached,
        all.len(),
        "not conformant: {reached} of {} behaviours reached",
        all.len()
    );
}
