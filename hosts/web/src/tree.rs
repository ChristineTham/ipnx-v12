//! **The page's tree** — what the browser keeps its files in, served to the
//! kernel by the same 9P as a terminal's directory ([`ipnx::store`]).
//!
//! It starts as the built root, named by an index — one line a file — and
//! **fetches a file the first time it is read**, so a page that runs `rc`
//! fetches `rc` and not the 85 MB beside it. What is written stays in the
//! page's memory: this is the tree a session works in. The decision to keep
//! the files in the page is the plan's (P8, `docs/implementation.md`); keeping
//! them across a visit, in the page's own storage, is a later step.

use crate::js;
use ipnx::store::{Backend, Meta};
use std::path::Path;

/// `Enonexist`, `Eexist`, `Enotdir`, `Eisdir` (`port/error.h`).
const ENONEXIST: &str = "file does not exist";
const EEXIST: &str = "file already exists";
const ENOTDIR: &str = "not a directory";
const EISDIR: &str = "file is a directory";
/// What ramfs answers for a directory that is not empty (`ramfs.c`).
const ENOTEMPTY: &str = "directory not empty";

enum Data {
    /// A directory.
    Dir(Vec<usize>),
    /// A file of the built root not yet fetched: its length, and its name
    /// there.
    Remote(u64, String),
    /// A file whose bytes are here.
    Here(Vec<u8>),
}

struct Node {
    name: String,
    parent: usize,
    perm: u32,
    atime: u32,
    mtime: u32,
    /// Its qid path: never reused, so a file made where another was is not
    /// mistaken for it.
    path: u64,
    data: Data,
}

pub struct Tree {
    nodes: Vec<Option<Node>>,
    next: u64,
}

impl Tree {
    /// The built root, from its index: one line a file, `kind perm size
    /// mtime path` — `d` or `f`, the permission in octal, the path relative
    /// to the root. A directory comes before what is in it.
    pub fn new(index: &str) -> Tree {
        let root = Node { name: String::new(), parent: 0, perm: 0o775, atime: 0, mtime: 0, path: 0, data: Data::Dir(Vec::new()) };
        let mut t = Tree { nodes: vec![Some(root)], next: 1 };
        for line in index.lines() {
            let f: Vec<&str> = line.splitn(5, '\t').collect();
            if f.len() < 5 {
                continue;
            }
            let (kind, perm, size, mtime, path) = (f[0], f[1], f[2], f[3], f[4]);
            let perm = u32::from_str_radix(perm, 8).unwrap_or(0o644);
            let mtime = mtime.parse().unwrap_or(0);
            let p = Path::new(path);
            let (Some(parent), Some(name)) = (p.parent(), p.file_name().and_then(|n| n.to_str())) else { continue };
            let Some(dir) = t.find(parent) else { continue };
            let data = if kind == "d" {
                Data::Dir(Vec::new())
            } else {
                Data::Remote(size.parse().unwrap_or(0), path.to_string())
            };
            t.add(dir, name, perm, mtime, data);
        }
        t
    }

    fn node(&self, i: usize) -> &Node {
        self.nodes[i].as_ref().expect("a live node")
    }

    fn node_mut(&mut self, i: usize) -> &mut Node {
        self.nodes[i].as_mut().expect("a live node")
    }

    fn add(&mut self, dir: usize, name: &str, perm: u32, mtime: u32, data: Data) -> usize {
        let i = self.nodes.len();
        let path = self.next;
        self.next += 1;
        self.nodes.push(Some(Node { name: name.to_string(), parent: dir, perm, atime: mtime, mtime, path, data }));
        if let Data::Dir(kids) = &mut self.node_mut(dir).data {
            kids.push(i);
        }
        i
    }

    fn child(&self, dir: usize, name: &str) -> Option<usize> {
        match &self.node(dir).data {
            Data::Dir(kids) => kids.iter().copied().find(|&k| self.node(k).name == name),
            _ => None,
        }
    }

    /// The node a path (already checked never to climb out) names.
    fn find(&self, p: &Path) -> Option<usize> {
        let mut at = 0;
        for c in p.components() {
            let name = c.as_os_str().to_str()?;
            at = self.child(at, name)?;
        }
        Some(at)
    }

    /// The bytes of a file, fetched if they are not here yet.
    fn bytes(&mut self, i: usize) -> Result<&mut Vec<u8>, String> {
        if let Data::Remote(_, at) = &self.node(i).data {
            let at = at.clone();
            // SAFETY: the host copies exactly the length it answered.
            let n = unsafe { js::fetch(at.as_ptr(), at.len()) };
            if n < 0 {
                return Err(format!("{at}: could not be fetched"));
            }
            let mut b = vec![0u8; n as usize];
            unsafe { js::fetched(b.as_mut_ptr(), b.len()) };
            self.node_mut(i).data = Data::Here(b);
        }
        match &mut self.node_mut(i).data {
            Data::Here(b) => Ok(b),
            _ => Err(EISDIR.into()),
        }
    }

    /// Where a new name goes: its directory, which must be one, and the name,
    /// which must not be there already.
    fn place<'a>(&self, p: &'a Path) -> Result<(usize, &'a str), String> {
        let dir = p.parent().and_then(|d| self.find(d)).ok_or(ENONEXIST)?;
        if !matches!(self.node(dir).data, Data::Dir(_)) {
            return Err(ENOTDIR.into());
        }
        let name = p.file_name().and_then(|n| n.to_str()).ok_or(ENONEXIST)?;
        if self.child(dir, name).is_some() {
            return Err(EEXIST.into());
        }
        Ok((dir, name))
    }

    fn now() -> u32 {
        // SAFETY: a plain call.
        (unsafe { js::now() } / 1000.0) as u32
    }
}

impl Backend for Tree {
    fn meta(&mut self, p: &Path) -> Option<Meta> {
        let i = self.find(p)?;
        let n = self.node(i);
        let (dir, len) = match &n.data {
            Data::Dir(_) => (true, 0),
            Data::Remote(len, _) => (false, *len),
            Data::Here(b) => (false, b.len() as u64),
        };
        Some(Meta { dir, excl: false, perm: n.perm, len, atime: n.atime, mtime: n.mtime, path: n.path })
    }

    fn list(&mut self, p: &Path) -> Vec<String> {
        let Some(i) = self.find(p) else { return Vec::new() };
        match &self.node(i).data {
            Data::Dir(kids) => kids.iter().map(|&k| self.node(k).name.clone()).collect(),
            _ => Vec::new(),
        }
    }

    fn read(&mut self, p: &Path, off: u64, n: usize) -> Result<Vec<u8>, String> {
        let i = self.find(p).ok_or(ENONEXIST)?;
        let b = self.bytes(i)?;
        let from = (off as usize).min(b.len());
        let to = (from + n).min(b.len());
        Ok(b[from..to].to_vec())
    }

    fn write(&mut self, p: &Path, off: u64, data: &[u8]) -> Result<(), String> {
        let i = self.find(p).ok_or(ENONEXIST)?;
        let b = self.bytes(i)?;
        let end = off as usize + data.len();
        if b.len() < end {
            b.resize(end, 0);
        }
        b[off as usize..end].copy_from_slice(data);
        self.node_mut(i).mtime = Tree::now();
        Ok(())
    }

    fn truncate(&mut self, p: &Path, len: u64) -> Result<(), String> {
        let i = self.find(p).ok_or(ENONEXIST)?;
        self.bytes(i)?.resize(len as usize, 0);
        self.node_mut(i).mtime = Tree::now();
        Ok(())
    }

    fn mkdir(&mut self, p: &Path, perm: u32) -> Result<(), String> {
        let (dir, name) = self.place(p)?;
        self.add(dir, name, perm, Tree::now(), Data::Dir(Vec::new()));
        Ok(())
    }

    fn mkfile(&mut self, p: &Path, perm: u32) -> Result<(), String> {
        let (dir, name) = self.place(p)?;
        self.add(dir, name, perm, Tree::now(), Data::Here(Vec::new()));
        Ok(())
    }

    fn chmod(&mut self, p: &Path, perm: u32) -> Result<(), String> {
        let i = self.find(p).ok_or(ENONEXIST)?;
        self.node_mut(i).perm = perm;
        Ok(())
    }

    fn touch(&mut self, p: &Path, mtime: u32) -> Result<(), String> {
        let i = self.find(p).ok_or(ENONEXIST)?;
        self.node_mut(i).mtime = mtime;
        Ok(())
    }

    fn rename(&mut self, from: &Path, to: &Path) -> Result<(), String> {
        let i = self.find(from).ok_or(ENONEXIST)?;
        let (dir, name) = self.place(to)?;
        let old = self.node(i).parent;
        if let Data::Dir(kids) = &mut self.node_mut(old).data {
            kids.retain(|&k| k != i);
        }
        if let Data::Dir(kids) = &mut self.node_mut(dir).data {
            kids.push(i);
        }
        let n = self.node_mut(i);
        n.name = name.to_string();
        n.parent = dir;
        Ok(())
    }

    fn remove(&mut self, p: &Path) -> Result<(), String> {
        let i = self.find(p).ok_or(ENONEXIST)?;
        if i == 0 {
            return Err("permission denied".into());
        }
        if let Data::Dir(kids) = &self.node(i).data {
            if !kids.is_empty() {
                return Err(ENOTEMPTY.into());
            }
        }
        let parent = self.node(i).parent;
        if let Data::Dir(kids) = &mut self.node_mut(parent).data {
            kids.retain(|&k| k != i);
        }
        self.nodes[i] = None;
        Ok(())
    }
}
