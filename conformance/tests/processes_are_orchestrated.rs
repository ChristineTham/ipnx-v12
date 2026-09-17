//! CONTRACT: process orchestration — what the kernel is actually for.
//!
//! `rfork(2)`'s rule, in Plan 9's words and with Plan 9's flag values: a `G`
//! bit copies, a `CG` bit clears, neither shares. It applies to all three
//! tables a process owns, identically, which is what makes "install", "dev
//! environment" and "sandbox" the same act with different arguments.

use ipnx_kernel::ns::{Bind, Element};
use ipnx_kernel::proc::{rf, Chan, Procs};
use std::rc::Rc;

#[test]
fn a_system_begins_with_pid_one_and_nothing_else() {
    let t = Procs::new();
    assert_eq!(t.count(), 1);
    assert!(t.get(1).is_some());
}

#[test]
fn neither_bit_shares() {
    // bind(1) depends on this: a child's bind IS the parent's bind unless the
    // parent asked otherwise.
    let mut t = Procs::new();
    let c = t.rfork(1, rf::PROC).unwrap();
    assert!(Rc::ptr_eq(&t.get(1).unwrap().ns, &t.get(c).unwrap().ns));
    assert!(Rc::ptr_eq(&t.get(1).unwrap().fds, &t.get(c).unwrap().fds));
    assert!(Rc::ptr_eq(&t.get(1).unwrap().env, &t.get(c).unwrap().env));
}

#[test]
fn the_g_bit_copies_and_the_cg_bit_clears() {
    let mut t = Procs::new();
    t.get(1).unwrap().ns.borrow_mut().bind("/bin", Element::new("system"), Bind::Replace);
    t.get(1).unwrap().fds.borrow_mut().add(Chan { target: "cons".into(), offset: 0 });

    let copied = t.rfork(1, rf::PROC | rf::NAMEG | rf::FDG).unwrap();
    assert!(!t.get(copied).unwrap().ns.borrow().is_empty(), "a copy keeps what was there");
    assert_eq!(t.get(copied).unwrap().fds.borrow().count(), 1);

    let cleared = t.rfork(1, rf::PROC | rf::CNAMEG | rf::CFDG).unwrap();
    assert!(t.get(cleared).unwrap().ns.borrow().is_empty(), "a cleared table is empty");
    assert_eq!(t.get(cleared).unwrap().fds.borrow().count(), 0);
}

#[test]
fn rfork_without_rfproc_acts_on_the_caller() {
    // One call, not two: this is how a process gives ITSELF a private
    // namespace, and it is why `rfork n` is a thing you can type.
    let mut t = Procs::new();
    let before = t.get(1).unwrap().ns.clone();
    assert_eq!(t.rfork(1, rf::NAMEG), None, "no new process");
    assert_eq!(t.count(), 1);
    assert!(!Rc::ptr_eq(&before, &t.get(1).unwrap().ns));
}

#[test]
fn await_reports_a_child_once_and_reaps_it() {
    let mut t = Procs::new();
    let c = t.rfork(1, rf::PROC).unwrap();
    assert_eq!(t.await_child(1), None, "a living child is not reported");
    t.exits(c, "");
    assert_eq!(t.await_child(1), Some((c, String::new())));
    assert_eq!(t.await_child(1), None, "and not twice");
    assert_eq!(t.count(), 1, "reaping removes it");
}

#[test]
fn rfnowait_leaves_no_zombie() {
    let mut t = Procs::new();
    let c = t.rfork(1, rf::PROC | rf::NOWAIT).unwrap();
    t.exits(c, "gone");
    assert_eq!(t.await_child(1), None);
}
