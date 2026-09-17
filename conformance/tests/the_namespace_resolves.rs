//! CONTRACT: every process has its own namespace, and every non-process call
//! resolves through it.
//!
//! A mount point holds a union — an ordered list — because `bind -a` and
//! `bind -b` are how two directories become one name. This is the mechanism
//! that makes installing a package a bind and a sandbox a namespace, so the
//! order and the create rule are contracts, not details.

use ipnx_kernel::ns::{clean, Bind, Element, Ns};

#[test]
fn the_longest_prefix_answers() {
    let mut ns = Ns::new();
    ns.bind("/n", Element::new("shallow"), Bind::Replace);
    ns.bind("/n/z", Element::new("deep"), Bind::Replace);
    let (els, rest) = ns.resolve("/n/z/store").unwrap();
    assert_eq!(els[0].target, "deep");
    assert_eq!(rest, "store");
}

#[test]
fn a_mount_point_is_whole_components_only() {
    // /n/z must not answer for /n/zebra.
    let mut ns = Ns::new();
    ns.bind("/n", Element::new("shallow"), Bind::Replace);
    ns.bind("/n/z", Element::new("deep"), Bind::Replace);
    assert_eq!(ns.resolve("/n/zebra").unwrap().0[0].target, "shallow");
}

#[test]
fn the_union_answers_in_bind_order() {
    let mut ns = Ns::new();
    ns.bind("/bin", Element::new("system"), Bind::Replace);
    ns.bind("/bin", Element::new("late"), Bind::After);
    ns.bind("/bin", Element::new("early"), Bind::Before);
    let order: Vec<&str> =
        ns.resolve("/bin/rc").unwrap().0.iter().map(|e| e.target.as_str()).collect();
    assert_eq!(order, vec!["early", "system", "late"]);
}

#[test]
fn a_create_lands_in_the_create_element_and_nowhere_else() {
    // Which element takes creates is what decides whether `install` is "for
    // me" or "for everyone". It is not a flag on the command.
    let mut ns = Ns::new();
    ns.bind("/pkg", Element::new("system"), Bind::Replace);
    ns.bind("/pkg", Element::creatable("mine"), Bind::Before);
    assert_eq!(ns.create_element("/pkg/python").unwrap().target, "mine");
}

#[test]
fn a_union_with_no_create_element_refuses_creates() {
    // How a read-only union is expressed: by having no element that accepts
    // creates, rather than by a read-only flag.
    let mut ns = Ns::new();
    ns.bind("/store", Element::new("immutable"), Bind::Replace);
    assert!(ns.create_element("/store/python").is_none());
}

#[test]
fn a_path_is_cleaned_the_way_cleanname_cleans_it() {
    assert_eq!(clean("/n/z/../store"), "/n/store");
    assert_eq!(clean("/n//z/"), "/n/z");
    assert_eq!(clean("/../.."), "/", "a namespace has no above");
}
