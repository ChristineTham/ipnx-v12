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

/// The demo, as a list of things a person can do.
fn demo() -> Vec<Behaviour> {
    use State::*;
    vec![
        // The CLI — typing `ipnx` in a terminal.
        Behaviour {
            what: "type `ipnx` and get an rc prompt",
            state: Pending("P5"),
            how: "start it, and a prompt appears",
        },
        Behaviour {
            what: "run `ls` and see your files",
            state: Pending("P5"),
            how: "type it at the prompt; the names appear",
        },
        Behaviour {
            what: "`cat /etc/motd`",
            state: Pending("P5"),
            how: "expect the file's text",
        },
        Behaviour {
            what: "pipe two commands together",
            state: Pending("P3"),
            how: "`ls | wc -l` at the prompt; expect a count",
        },
        Behaviour {
            what: "run a Go program",
            state: Pending("P6"),
            how: "a Go program runs and prints what it printed before",
        },
        Behaviour {
            what: "run Python",
            state: Pending("P6"),
            how: "Python starts, imports from its library, and computes",
        },
        // The website — the page at christham.net/ipnx-v12.
        Behaviour {
            what: "open the page and have the system boot in seconds",
            state: Pending("P7"),
            how: "open it; it is usable within seconds",
        },
        Behaviour {
            what: "see your home listed on the left",
            state: Pending("P7"),
            how: "the listing window shows the names the home directory holds",
        },
        Behaviour {
            what: "open a file and have it appear as a tab",
            state: Pending("P7"),
            how: "click a name; a tab appears carrying its text",
        },
        Behaviour {
            what: "type at the rc running below",
            state: Pending("P7"),
            how: "keystrokes reach the shell and its output comes back",
        },
        Behaviour {
            what: "the toolbar and status line behave as the type declares",
            state: Pending("P7"),
            how: "the verbs offered differ by what the window holds",
        },
        Behaviour {
            what: "the toolchains become available without a reload or a wait",
            state: Pending("P7"),
            how: "the page is usable first; the toolchains work later in the same session",
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
