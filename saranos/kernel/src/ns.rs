//! The namespace — a mount table from paths to file servers, one per process.
//!
//! Every non-process syscall resolves through it. A namespace is not a global
//! with per-process overrides: it is per-process outright, and a child either
//! shares its parent's or gets a copy, which is what makes "install", "dev
//! environment" and "sandbox" the same act with different arguments.
//!
//! A mount point holds a UNION — an ordered list — because `bind -a` and
//! `bind -b` are how two directories become one name. Walks try the elements in
//! order; a create lands in the element that accepts creates, and in no other.

use std::collections::HashMap;

/// Where in the union a new element goes, and whether it may take creates.
/// These are `bind(2)`'s flags and nothing more.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Bind {
    /// Replace whatever is there.
    Replace,
    /// Before the existing elements: this one answers first.
    Before,
    /// After the existing elements: the existing ones answer first.
    After,
}

/// One element of a union: a server, and whether creates may land in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    /// What this element resolves to. A server's identity is the host's
    /// business; the namespace only needs to tell elements apart.
    pub target: String,
    /// `MCREATE` — a create in this directory lands here. At most one element
    /// of a union should carry it, and the first that does wins.
    pub create: bool,
}

impl Element {
    pub fn new(target: &str) -> Self {
        Element { target: target.to_string(), create: false }
    }
    pub fn creatable(target: &str) -> Self {
        Element { target: target.to_string(), create: true }
    }
}

/// A process's namespace.
#[derive(Clone, Debug, Default)]
pub struct Ns {
    mounts: HashMap<String, Vec<Element>>,
}

impl Ns {
    pub fn new() -> Self {
        Ns::default()
    }

    /// `bind(2)`. The mount point is a path; the element is what answers there.
    pub fn bind(&mut self, at: &str, el: Element, how: Bind) {
        let at = clean(at);
        let list = self.mounts.entry(at).or_default();
        match how {
            Bind::Replace => *list = vec![el],
            Bind::Before => list.insert(0, el),
            Bind::After => list.push(el),
        }
    }

    /// `unmount(2)` with a name: remove one element. With `None`: clear the
    /// mount point entirely.
    pub fn unmount(&mut self, at: &str, target: Option<&str>) {
        let at = clean(at);
        match target {
            None => {
                self.mounts.remove(&at);
            }
            Some(t) => {
                if let Some(list) = self.mounts.get_mut(&at) {
                    list.retain(|e| e.target != t);
                    if list.is_empty() {
                        self.mounts.remove(&at);
                    }
                }
            }
        }
    }

    /// Resolve a path to the union that serves it, plus the remainder of the
    /// path below the mount point. LONGEST PREFIX wins: a bind on `/n/z`
    /// answers for `/n/z/store` even though `/n` is also bound.
    pub fn resolve(&self, path: &str) -> Option<(&[Element], String)> {
        let path = clean(path);
        let mut best: Option<(&String, &Vec<Element>)> = None;
        for (at, list) in &self.mounts {
            if !covers(at, &path) {
                continue;
            }
            if best.map_or(true, |(b, _)| at.len() > b.len()) {
                best = Some((at, list));
            }
        }
        let (at, list) = best?;
        let rest = path[at.len()..].trim_start_matches('/').to_string();
        Some((list.as_slice(), rest))
    }

    /// Which element a create at this path lands in: the first that accepts
    /// creates. A union with no such element refuses creates, which is how a
    /// read-only union is expressed without a read-only flag.
    pub fn create_element(&self, path: &str) -> Option<&Element> {
        let (list, _) = self.resolve(path)?;
        list.iter().find(|e| e.create)
    }

    pub fn is_empty(&self) -> bool {
        self.mounts.is_empty()
    }
}

/// Does the mount point `at` cover `path`? `/` covers everything; otherwise the
/// path must be `at` exactly, or start with `at` followed by a separator — so
/// `/n/z` does not cover `/n/zebra`.
fn covers(at: &str, path: &str) -> bool {
    if at == "/" {
        return true;
    }
    path == at || (path.starts_with(at) && path.as_bytes().get(at.len()) == Some(&b'/'))
}

/// `cleanname(3)`'s rule, as much of it as a mount point needs: collapse
/// repeated separators, resolve `.` and `..`, drop a trailing separator. `..`
/// at the root stays at the root — a namespace has no above.
pub fn clean(path: &str) -> String {
    let rooted = path.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if !out.is_empty() && *out.last().unwrap() != ".." {
                    out.pop();
                } else if !rooted {
                    out.push("..");
                }
            }
            p => out.push(p),
        }
    }
    let joined = out.join("/");
    if rooted {
        format!("/{}", joined)
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_mount_point_is_a_whole_component() {
        // /n/z must not answer for /n/zebra
        let mut ns = Ns::new();
        ns.bind("/n", Element::new("shallow"), Bind::Replace);
        ns.bind("/n/z", Element::new("deep"), Bind::Replace);
        let (els, rest) = ns.resolve("/n/zebra").unwrap();
        assert_eq!(els[0].target, "shallow");
        assert_eq!(rest, "zebra");
    }

    #[test]
    fn bind_before_and_after_decide_who_answers_first() {
        let mut ns = Ns::new();
        ns.bind("/bin", Element::new("system"), Bind::Replace);
        ns.bind("/bin", Element::new("late"), Bind::After);
        ns.bind("/bin", Element::new("early"), Bind::Before);
        let (els, _) = ns.resolve("/bin/rc").unwrap();
        let order: Vec<&str> = els.iter().map(|e| e.target.as_str()).collect();
        assert_eq!(order, vec!["early", "system", "late"]);
    }

    #[test]
    fn a_create_lands_in_the_create_element_and_nowhere_else() {
        let mut ns = Ns::new();
        ns.bind("/pkg", Element::new("readonly"), Bind::Replace);
        ns.bind("/pkg", Element::creatable("mine"), Bind::After);
        assert_eq!(ns.create_element("/pkg/python").unwrap().target, "mine");
    }

    #[test]
    fn a_union_with_no_create_element_refuses_creates() {
        let mut ns = Ns::new();
        ns.bind("/store", Element::new("immutable"), Bind::Replace);
        assert!(ns.create_element("/store/python").is_none());
    }

    #[test]
    fn unmount_removes_one_element_and_the_rest_stand() {
        let mut ns = Ns::new();
        ns.bind("/bin", Element::new("a"), Bind::Replace);
        ns.bind("/bin", Element::new("b"), Bind::After);
        ns.unmount("/bin", Some("a"));
        let (els, _) = ns.resolve("/bin/x").unwrap();
        assert_eq!(els.len(), 1);
        assert_eq!(els[0].target, "b");
    }

    #[test]
    fn unmount_with_no_name_clears_the_mount_point() {
        let mut ns = Ns::new();
        ns.bind("/bin", Element::new("a"), Bind::Replace);
        ns.unmount("/bin", None);
        assert!(ns.resolve("/bin/x").is_none());
    }

    #[test]
    fn a_namespace_is_copied_not_shared_when_it_is_copied() {
        let mut parent = Ns::new();
        parent.bind("/bin", Element::new("system"), Bind::Replace);
        let mut child = parent.clone();
        child.bind("/bin", Element::new("mine"), Bind::Before);
        assert_eq!(parent.resolve("/bin/x").unwrap().0.len(), 1);
        assert_eq!(child.resolve("/bin/x").unwrap().0.len(), 2);
    }

    #[test]
    fn dot_dot_pops_a_component_and_the_root_has_no_above() {
        assert_eq!(clean("/n/z/../store"), "/n/store");
        assert_eq!(clean("/../.."), "/");
        assert_eq!(clean("/n//z/"), "/n/z");
    }
}
