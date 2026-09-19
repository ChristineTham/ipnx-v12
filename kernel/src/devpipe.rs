//! `#|` — the pipe device (`plan9/sys/src/9/port/devpipe.c`).
//!
//! An **attach mints a pipe**: `pipeattach` (`devpipe.c:56`) allocates a
//! `Pipe` with two queues and returns the channel for its directory. That is
//! why a pipe has no name anywhere in the namespace and no `create` — asking
//! the device for a root IS the allocation.
//!
//! The directory is three entries (`pipedir[]`, `devpipe.c:33`): `.`, `data`
//! and `data1`. The two ends are **cross-connected**: `data` reads `q[0]` and
//! writes `q[1]`, `data1` reads `q[1]` and writes `q[0]`
//! (`piperead`/`pipewrite`). Writing one end is therefore readable at the
//! other, which is the whole of what a pipe is.

use crate::chan::Chan;
use crate::dev::{Dev, DevId, EVE};
use crate::ninep::{Qid, QTDIR};
use std::collections::VecDeque;

/// The two queues of one pipe. Plan 9 hangs these off `c->aux`; here the
/// channel's `devno` is the pipe's index, which is the same triple
/// (`type`, `dev`, `qid`) that `findmount` keys on.
#[derive(Default)]
struct Pipe {
    q: [VecDeque<u8>; 2],
}

/// Qids within one pipe, as `devpipe.c:28` enumerates them.
const QDIR: u64 = 0;
const QDATA0: u64 = 1;
const QDATA1: u64 = 2;

#[derive(Default)]
pub struct PipeDev {
    pipes: Vec<Pipe>,
}

impl PipeDev {
    pub fn new() -> PipeDev {
        PipeDev::default()
    }

    /// Which queue a read on this end draws from, and which a write fills.
    /// `data` reads 0 and writes 1; `data1` reads 1 and writes 0.
    fn ends(qid: u64) -> Option<(usize, usize)> {
        match qid {
            QDATA0 => Some((0, 1)),
            QDATA1 => Some((1, 0)),
            _ => None,
        }
    }

    fn pipe(&mut self, c: &Chan) -> Result<&mut Pipe, String> {
        self.pipes.get_mut(c.devno as usize).ok_or_else(|| "no such pipe".into())
    }
}

impl Dev for PipeDev {
    fn id(&self) -> DevId {
        DevId::Pipe
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    /// `pipeattach` — the allocation. Every attach is a NEW pipe.
    fn attach(&mut self, _spec: &str) -> Result<Chan, String> {
        self.pipes.push(Pipe::default());
        Ok(Chan::attach(DevId::Pipe, (self.pipes.len() - 1) as u32))
    }

    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Qid>, String> {
        if c.qid.path != QDIR {
            return Err("not a directory".into());
        }
        Ok(match name {
            "data" => Some(Qid { qtype: 0, vers: 0, path: QDATA0 }),
            "data1" => Some(Qid { qtype: 0, vers: 0, path: QDATA1 }),
            ".." | "." => Some(Qid { qtype: QTDIR, vers: 0, path: QDIR }),
            _ => None,
        })
    }

    fn open(&mut self, mut c: Chan, mode: u16) -> Result<Chan, String> {
        c.mode = mode;
        Ok(c)
    }

    /// A pipe has no `create`: the attach made it.
    fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
        Err("permission denied".into())
    }

    /// `qread`. A pipe is a stream, so the offset is ignored — Plan 9's
    /// `piperead` takes a `vlong` it never names.
    fn read(&mut self, c: &mut Chan, n: usize, _off: u64) -> Result<Vec<u8>, String> {
        // `piperead` (`devpipe.c:243`) answers `pipedir[]` for the directory
        // — two entries, and their lengths are what is queued in each.
        if c.qid.is_dir() {
            let queued = {
                let p = self.pipe(c)?;
                [p.q[0].len() as u64, p.q[1].len() as u64]
            };
            let entries: Vec<crate::ninep::Dir> = [("data", QDATA0, 0), ("data1", QDATA1, 1)]
                .into_iter()
                .map(|(name, path, i)| {
                    let qid = Qid { qtype: 0, vers: 0, path };
                    crate::dev::devdir(c, qid, name, queued[i], EVE, EVE, 0o600)
                })
                .collect();
            return Ok(crate::dev::devdirread(c, n, &entries));
        }
        let (from, _) = Self::ends(c.qid.path).ok_or("cannot read that")?;
        let p = self.pipe(c)?;
        let take = n.min(p.q[from].len());
        Ok(p.q[from].drain(..take).collect())
    }

    /// `qwrite`, to the OTHER queue. This crossing is the pipe.
    fn write(&mut self, c: &mut Chan, data: &[u8], _off: u64) -> Result<usize, String> {
        let (_, to) = Self::ends(c.qid.path).ok_or("cannot write that")?;
        let p = self.pipe(c)?;
        p.q[to].extend(data.iter().copied());
        Ok(data.len())
    }

    fn stat(&mut self, c: &Chan) -> Result<Vec<u8>, String> {
        let name = match c.qid.path {
            QDATA0 => "data",
            QDATA1 => "data1",
            _ => ".",
        };
        let perm = if c.qid.path == QDIR {
            crate::ninep::DMDIR | 0o500
        } else {
            0o600
        };
        // A pipe end's LENGTH is what is queued in it, which is how a reader
        // can tell there is something there (`pipestat`, `devpipe.c:196`).
        let length = match Self::ends(c.qid.path) {
            Some((from, _)) => {
                let p = self.pipe(c)?;
                p.q[from].len() as u64
            }
            None => 0,
        };
        Ok(crate::dev::devdir(c, c.qid, name, length, EVE, EVE, perm).conv_d2m())
    }

    fn wstat(&mut self, _c: &mut Chan, _e: &[u8]) -> Result<(), String> {
        Err("permission denied".into())
    }

    fn remove(&mut self, _c: &mut Chan) -> Result<(), String> {
        Err("permission denied".into())
    }

    fn close(&mut self, _c: &mut Chan) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chan::mode::ORDWR;

    /// `syspipe` (`sysfile.c`): attach `#|`, clone, walk one to `data` and the
    /// other to `data1`, open both. The two channels are the two ends.
    fn pipe() -> (PipeDev, Chan, Chan) {
        let mut d = PipeDev::new();
        let dir = d.attach("").unwrap();
        let mut a = dir.clone();
        a.qid = d.walk(&dir, "data").unwrap().unwrap();
        let mut b = dir.clone();
        b.qid = d.walk(&dir, "data1").unwrap().unwrap();
        let a = d.open(a, ORDWR).unwrap();
        let b = d.open(b, ORDWR).unwrap();
        (d, a, b)
    }

    #[test]
    fn what_is_written_at_one_end_is_read_at_the_other() {
        let (mut d, mut a, mut b) = pipe();
        d.write(&mut a, b"hello", 0).unwrap();
        assert_eq!(d.read(&mut b, 16, 0).unwrap(), b"hello");
    }

    #[test]
    fn the_crossing_goes_both_ways() {
        let (mut d, mut a, mut b) = pipe();
        d.write(&mut b, b"back", 0).unwrap();
        assert_eq!(d.read(&mut a, 16, 0).unwrap(), b"back");
    }

    /// What goes in one end does NOT come back out of it. A pipe whose
    /// queues were not crossed would pass every test above and fail this one.
    #[test]
    fn a_write_is_not_readable_at_the_end_that_wrote_it() {
        let (mut d, mut a, _b) = pipe();
        d.write(&mut a, b"hello", 0).unwrap();
        assert!(d.read(&mut a, 16, 0).unwrap().is_empty());
    }

    /// Every attach is a new pipe (`pipeattach`, `devpipe.c:56`), so two
    /// pipes never share a queue.
    #[test]
    fn each_attach_mints_its_own_pipe() {
        let (mut d, mut a, _) = pipe();
        let dir = d.attach("").unwrap();
        let mut other = dir.clone();
        other.qid = d.walk(&dir, "data1").unwrap().unwrap();
        d.write(&mut a, b"mine", 0).unwrap();
        assert!(d.read(&mut other, 16, 0).unwrap().is_empty(), "two pipes shared a queue");
    }

    /// A pipe is a stream: a short read leaves the rest, and the next read
    /// continues from there without any offset being tracked.
    #[test]
    fn a_short_read_leaves_the_rest_queued() {
        let (mut d, mut a, mut b) = pipe();
        d.write(&mut a, b"abcdef", 0).unwrap();
        assert_eq!(d.read(&mut b, 2, 0).unwrap(), b"ab");
        assert_eq!(d.read(&mut b, 99, 0).unwrap(), b"cdef");
    }

    /// The attach made it; there is nothing to create.
    #[test]
    fn a_pipe_cannot_be_created_or_removed() {
        let (mut d, mut a, _) = pipe();
        assert!(d.create(&mut a, "x", 0, 0).is_err());
        assert!(d.remove(&mut a).is_err());
    }
}
