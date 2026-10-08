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
    // a copy of the built root, this process's own ([`ipnx::rootcopy`])
    let store = ipnx::rootcopy();
    let term = ipnx::Term::typing(keys);
    let fs = ipnx::store::Store::new(store).expect("no userspace/root — run userspace/mk.sh");
    ipnx::startboot(&ipnx::plan9ini(&[]), Box::new(term.clone()), Some(Box::new(fs)))
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
    /// Not yet, and **no phase of `docs/implementation.md` delivers it** —
    /// a gap, in the sense of the triage rule (CLAUDE.md): undesigned, and
    /// needing a proposal before it is built. Naming a phase that does not
    /// build it was a claim with nothing behind it.
    ///
    /// No line is one on 2026-10-08; the state stays, because the triage
    /// has three.
    #[allow(dead_code)]
    Gap,
    /// Not yet: **a design is proposed** in `docs/proposals.md` and awaits
    /// Christine's review. No phase builds it until she endorses it — the
    /// triage rule's middle state.
    Proposed,
    /// Not yet: **designed and endorsed** — the triage rule's first state —
    /// and no phase of `docs/implementation.md` builds it yet.
    Specd,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::Reached => write!(f, "reached"),
            State::Pending(p) => write!(f, "not yet  ({p})"),
            State::Gap => write!(f, "not yet  (gap: no phase builds it)"),
            State::Proposed => write!(f, "not yet  (proposed: awaiting review)"),
            State::Specd => write!(f, "not yet  (spec'd: no phase builds it yet)"),
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
                // each name a line of its own: `ls` prints one a line, and a
                // name found inside another (`rc` in `proc`) is not there
                let out = typing("ls /\n");
                for name in ["bin", "dev", "env", "proc", "srv", "etc", "pkg", "profile"] {
                    if !out.lines().any(|l| l.trim_end() == format!("/{name}") || l.trim_end() == name) {
                        return Err(format!("no {name:?} in {out:?}"));
                    }
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
            state: Proposed,
            how: "it runs and prints what it printed before",
            check: None,
        },
        Behaviour {
            what: "run Python",
            state: Proposed,
            how: "it starts, imports from its library, and computes",
            check: None,
        },
        Behaviour {
            // The demo: *"installing is a bind … and a subshell that does
            // `rfork n` owns a private environment that vanishes with it"*.
            // Opening a project brings its packages (docs/projects.md); this
            // is the mechanism under it, P7's `pkg`.
            what: "installing a package is a bind, and a subshell can have packages of its own",
            state: Reached,
            how: "a package installed in a subshell runs there, and outside it is not there",
            check: Some(|| {
                // a repository of one package, made as a packager makes one
                // (docs/packages.md, *The forms*), then installed to a
                // subshell's namespace only
                let out = typing(
                    "mkdir -p /usr/kitty/repo/src/hello/wasm/bin\n\
                     cd /usr/kitty/repo\n\
                     echo '#!/bin/rc' >src/hello/wasm/bin/hello\n\
                     echo 'echo hello from a package' >>src/hello/wasm/bin/hello\n\
                     chmod +x src/hello/wasm/bin/hello\n\
                     echo 'pkg=hello version=1.0' >src/hello/pkg.cfg\n\
                     {echo +; echo '\tpkg.cfg'; echo '\twasm'; echo '\t\tbin'; echo '\t\t\thello'} >proto\n\
                     disk/mkfs -a -s src/hello proto >hello-1.0.mkfs >[2]/dev/null\n\
                     sum=`{sha1sum -2 256 hello-1.0.mkfs}\n\
                     echo 'pkg=hello version=1.0 sha256='$sum(1)' file=hello-1.0.mkfs' >index\n\
                     echo 'bind /usr/kitty/repo /n/pkg' >/profile/repository\n\
                     cd /\n\
                     @{rfork n; pkg install -n hello >/dev/null; echo IN `{hello}}\n\
                     echo OUT; hello\n",
                );
                wants(&out, "IN hello from a package")?;
                let outside = out.split("OUT").last().unwrap_or_default();
                if outside.contains("hello from a package") {
                    return Err(format!("the package escaped its subshell: {out:?}"));
                }
                wants(outside, "does not exist")
            }),
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
            // The demo: *"`cc hello.c` then `./a.out` is real clang and real
            // wasm-ld, as wasm programs. `go run hello.go` drives the real gc
            // compiler and linker"*. (It streamed them in after boot, which
            // was how the page fetched 260 MB, not a feature.) Here the
            // toolchain is the host's, and the system says so: *"some
            // toolchains depend on the hsot"* (Christine, 2026-10-08). It is
            // typed through `os`, where the host runs commands, and is not in
            // the browser (docs/saranos.md, "The host's resources").
            what: "build a program with a language toolchain, and run it",
            state: Specd,
            how: "`os go build` makes it with the host's toolchain, and it runs; where the host runs commands",
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
