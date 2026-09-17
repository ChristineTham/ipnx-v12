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
            state: Pending("P5"),
            how: "start it; a shell takes what you type and answers",
        },
        Behaviour {
            what: "list a directory",
            state: Pending("P5"),
            how: "the names it holds come back",
        },
        Behaviour {
            what: "read a file's contents",
            state: Pending("P5"),
            how: "the text comes back",
        },
        Behaviour {
            what: "run a command, and pipe one into another",
            state: Pending("P3"),
            how: "the second sees what the first wrote",
        },
        Behaviour {
            what: "run a Go program",
            state: Pending("P6"),
            how: "it runs and prints what it printed before",
        },
        Behaviour {
            what: "run Python",
            state: Pending("P6"),
            how: "it starts, imports from its library, and computes",
        },
        Behaviour {
            what: "a package becomes available without installing anything into the tree",
            state: Pending("P6"),
            how: "the program runs afterwards; the tree is no bigger than before",
        },
        Behaviour {
            what: "processes have their own namespaces and do not disturb each other",
            state: Pending("P2"),
            how: "one changes what a name means; the other still sees the old one",
        },
        Behaviour {
            what: "the windowing system can show several things at once and act on them",
            state: Pending("P7"),
            how: "more than one is open; acting on one leaves the others alone",
        },
        Behaviour {
            what: "what you can do with a thing depends on what it is",
            state: Pending("P7"),
            how: "two kinds of content offer different actions",
        },
        Behaviour {
            what: "the whole system runs in a browser as well as a terminal",
            state: Pending("P7"),
            how: "the same userspace, reached through a page",
        },
        Behaviour {
            what: "a language toolchain becomes usable during a session, not before it",
            state: Pending("P7"),
            how: "the system is usable first; the toolchain works later in the same session",
        },
    ]
}

#[test]
fn distance_to_the_demo() {
    let all = demo();
    let reached = all.iter().filter(|b| b.state == State::Reached).count();

    println!("\n  functional equivalence to the demo: {reached} of {}\n", all.len());
    for b in &all {
        let mark = if b.state == State::Reached { "x" } else { " " };
        println!("  [{mark}] {:<52} {}", b.what, b.state);
    }
    println!();

    // Each behaviour claimed as reached must be demonstrated by the check its
    // `how` describes. None is claimed yet, so there is nothing to run — and
    // when one is, this is where the running goes. A line moved to `Reached`
    // without a check here is a claim with nothing behind it.
    for b in all.iter().filter(|b| b.state == State::Reached) {
        panic!(
            "'{}' is claimed reached but has no check wired up here ({})",
            b.what, b.how
        );
    }
}
