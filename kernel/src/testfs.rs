//! **A file server for the kernel's own tests**: a read-only tree held in
//! memory and answered in 9P2000, so that a test kernel's root is a file
//! server reached over `#9/0` — the way the system's is, since the host
//! attaches the store before the first program runs (Christine,
//! 2026-10-08) and the kernel carries no files of its own (`devroot`).
//!
//! It answers what a walk, an open, a read and a stat need, as walk(5),
//! open(5), read(5) and stat(5) say, and refuses every change.

use crate::devvirtio9p::Nineserver;
use crate::ninep::{unframe, Dir, Qid, R, T, W, DMDIR, QTDIR};
use std::collections::HashMap;

struct Node {
    name: String,
    parent: usize,
    qid: Qid,
    data: Vec<u8>,
}

pub struct Files {
    /// Node 0 is the root.
    nodes: Vec<Node>,
    fids: HashMap<u32, usize>,
}

/// `Eperm`, as a server says it.
const EPERM: &str = "permission denied";

impl Files {
    /// A tree of `files`, each named by its path from the root — `init`,
    /// `lib/x` — with the directories on the way made as they are needed.
    /// Every file is `0555`, so it can be run.
    pub fn new(files: &[(&str, &[u8])]) -> Files {
        let root = Node { name: "/".into(), parent: 0, qid: Qid { qtype: QTDIR, vers: 0, path: 0 }, data: Vec::new() };
        let mut f = Files { nodes: vec![root], fids: HashMap::new() };
        for (path, data) in files {
            let mut at = 0;
            let names: Vec<&str> = path.split('/').filter(|n| !n.is_empty()).collect();
            for (i, name) in names.iter().enumerate() {
                let last = i + 1 == names.len();
                at = match f.child(at, name) {
                    Some(n) => n,
                    None => {
                        let qid = Qid { qtype: if last { 0 } else { QTDIR }, vers: 0, path: f.nodes.len() as u64 };
                        f.nodes.push(Node {
                            name: (*name).to_string(),
                            parent: at,
                            qid,
                            data: if last { data.to_vec() } else { Vec::new() },
                        });
                        f.nodes.len() - 1
                    }
                };
            }
        }
        f
    }

    fn child(&self, of: usize, name: &str) -> Option<usize> {
        match name {
            "." => Some(of),
            ".." => Some(self.nodes[of].parent),
            _ => (0..self.nodes.len()).find(|&i| i != 0 && self.nodes[i].parent == of && self.nodes[i].name == name),
        }
    }

    fn dir(&self, i: usize) -> Dir {
        let n = &self.nodes[i];
        let isdir = n.qid.qtype & QTDIR != 0;
        Dir {
            qid: n.qid,
            mode: if isdir { DMDIR | 0o555 } else { 0o555 },
            length: n.data.len() as u64,
            name: n.name.clone(),
            uid: "bootes".into(),
            gid: "bootes".into(),
            muid: "bootes".into(),
            ..Default::default()
        }
    }

    fn answer(&mut self, t: &[u8]) -> Vec<u8> {
        let Some(m) = unframe(t) else { return Vec::new() };
        let tag = m.tag;
        let mut r = R::new(m.body);
        let err = |e: &str| W::new().s(e).frame(T::Error as u8, tag);
        match m.ty {
            x if x == T::Version as u8 => {
                let msize = r.u32().unwrap_or(8192);
                W::new().u32(msize.min(8192)).s("9P2000").frame(T::Version.reply(), tag)
            }
            x if x == T::Auth as u8 => err("authentication not required"),
            x if x == T::Attach as u8 => {
                let fid = r.u32().unwrap_or(0);
                self.fids.insert(fid, 0);
                W::new().raw(&self.nodes[0].qid.write(W::new()).into_body()).frame(T::Attach.reply(), tag)
            }
            x if x == T::Flush as u8 => W::new().frame(T::Flush.reply(), tag),
            // walk(5): *newfid* is made only when every name was walked, and
            // a first name that cannot be walked is an `Rerror`.
            x if x == T::Walk as u8 => {
                let (Some(from), Some(newfid), Some(n)) = (r.u32(), r.u32(), r.u16()) else {
                    return err("short Twalk");
                };
                let Some(&start) = self.fids.get(&from) else { return err("unknown fid") };
                let mut at = start;
                let mut qids = Vec::new();
                for _ in 0..n {
                    let Some(name) = r.s() else { return err("short Twalk") };
                    if self.nodes[at].qid.qtype & QTDIR == 0 {
                        if qids.is_empty() {
                            return err("not a directory");
                        }
                        break;
                    }
                    match self.child(at, name) {
                        Some(next) => {
                            at = next;
                            qids.push(self.nodes[at].qid);
                        }
                        None => break,
                    }
                }
                if n > 0 && qids.is_empty() {
                    return err("file does not exist");
                }
                if qids.len() == n as usize {
                    self.fids.insert(newfid, at);
                }
                let mut w = W::new().u16(qids.len() as u16);
                for q in &qids {
                    w = w.raw(&q.write(W::new()).into_body());
                }
                w.frame(T::Walk.reply(), tag)
            }
            x if x == T::Open as u8 => {
                let (Some(fid), Some(mode)) = (r.u32(), r.u8()) else { return err("short Topen") };
                let Some(&at) = self.fids.get(&fid) else { return err("unknown fid") };
                // read, or execute; nothing here is written
                if mode & 3 == 1 || mode & 3 == 2 || mode & 0x10 != 0 {
                    return err(EPERM);
                }
                W::new().raw(&self.nodes[at].qid.write(W::new()).into_body()).u32(0).frame(T::Open.reply(), tag)
            }
            x if x == T::Read as u8 => {
                let (Some(fid), Some(off), Some(count)) = (r.u32(), r.u64(), r.u32()) else {
                    return err("short Tread");
                };
                let Some(&at) = self.fids.get(&fid) else { return err("unknown fid") };
                let data = if self.nodes[at].qid.qtype & QTDIR != 0 {
                    // a directory reads as its entries, never one split
                    let mut all = Vec::new();
                    for i in 1..self.nodes.len() {
                        if self.nodes[i].parent == at {
                            all.extend_from_slice(&self.dir(i).conv_d2m());
                        }
                    }
                    all
                } else {
                    self.nodes[at].data.clone()
                };
                let from = (off as usize).min(data.len());
                let to = (from + count as usize).min(data.len());
                W::new().u32((to - from) as u32).raw(&data[from..to]).frame(T::Read.reply(), tag)
            }
            x if x == T::Stat as u8 => {
                let Some(fid) = r.u32() else { return err("short Tstat") };
                let Some(&at) = self.fids.get(&fid) else { return err("unknown fid") };
                let b = self.dir(at).conv_d2m();
                W::new().u16(b.len() as u16).raw(&b).frame(T::Stat.reply(), tag)
            }
            x if x == T::Clunk as u8 => {
                let fid = r.u32().unwrap_or(0);
                if self.fids.remove(&fid).is_none() {
                    return err("unknown fid");
                }
                W::new().frame(T::Clunk.reply(), tag)
            }
            x if x == T::Remove as u8 => {
                // remove(5): the fid is clunked even when the remove fails
                if let Some(fid) = r.u32() {
                    self.fids.remove(&fid);
                }
                err(EPERM)
            }
            _ => err(EPERM),
        }
    }
}

impl Nineserver for Files {
    fn rpc(&mut self, t: &[u8]) -> Result<Vec<u8>, String> {
        Ok(self.answer(t))
    }
}
