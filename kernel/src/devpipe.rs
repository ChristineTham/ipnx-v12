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

use crate::chan::{flag::COPEN, Chan};
use crate::dev::{Dev, DevId, Eve};
use crate::ninep::{Qid, QTDIR};
use crate::proc::{Pid, Procs, Rid, Up};
use crate::qio::{self, At, Qid3, Queue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// `conf.pipeqsize` on one processor (`pipeinit`, `devpipe.c:50`) — the
/// limit a writer waits under.
const PIPEQSIZE: usize = 32 * 1024;

/// What `pipewrite` posts to a writer whose write failed (`devpipe.c:349`).
const EPIPENOTE: &str = "sys: write on closed pipe";

/// `struct Pipe` (`devpipe.c:12`): two queues, and how many opens of each
/// end there are. Plan 9 hangs it off `c->aux`; here the channel's `devno`
/// is the pipe's index.
struct Pipe {
    q: [Queue; 2],
    /// `qref[2]` — opens of `data` and of `data1`. When one reaches zero the
    /// other end's queue is hung up (`pipeclose`, `devpipe.c:247`).
    qref: [u32; 2],
}

/// Qids within one pipe, as `devpipe.c:28` enumerates them.
const QDIR: u64 = 0;
const QDATA0: u64 = 1;
const QDATA1: u64 = 2;

pub struct PipeDev {
    /// `eve` — the kernel-wide host owner (`auth.c:10`), shared rather than
    /// copied, because writing `#c/hostowner` renames it for everyone.
    eve: Eve,
    pipes: Vec<Pipe>,
    /// `up`, for `sleep`, `wakeup`, `qlock` and `qunlock`.
    up: Rc<RefCell<Up>>,
    /// Each process in the middle of a read or a write, and where.
    at: HashMap<Pid, At>,
}

impl PipeDev {
    pub fn new(up: Rc<RefCell<Up>>) -> PipeDev {
        PipeDev { eve: Eve::default(), pipes: Vec::new(), up, at: HashMap::new() }
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

    /// `up`, and the table `sleep` and `wakeup` act on.
    fn up(&self) -> (Pid, Rc<RefCell<Procs>>) {
        let up = self.up.borrow();
        (up.pid, up.procs.clone())
    }

    /// `qread` (`qio.c:1070`) on the queue this end reads. The caller has
    /// checked this is a data end.
    fn qread(&mut self, devno: u32, from: usize, n: usize) -> Result<Vec<u8>, String> {
        let (pid, procs) = self.up();
        let mut procs = procs.borrow_mut();
        let q = &mut self.pipes[devno as usize].q[from];
        qio::qread(q, &mut self.at, &mut procs, pid, Qid3 { dev: DevId::Pipe, devno, which: from }, n, &mut |_, _| {})
    }

    /// `qwrite` (`qio.c:1270`) to the queue the other end reads; a failure
    /// posts *"sys: write on closed pipe"* (`devpipe.c:349`).
    fn qwrite(&mut self, devno: u32, to: usize, data: &[u8]) -> Result<usize, String> {
        let (pid, procs) = self.up();
        let mut procs = procs.borrow_mut();
        let q = &mut self.pipes[devno as usize].q[to];
        let id = Qid3 { dev: DevId::Pipe, devno, which: to };
        qio::qwrite(q, &mut self.at, &mut procs, pid, id, data, Some(EPIPENOTE), &mut |_, _| {})
    }

    /// `wakeup(&q->rr); wakeup(&q->wr);` — what `qhangup` and `qclose` end
    /// with.
    fn wakeboth(&self, devno: u32, q: usize) {
        let (_, procs) = self.up();
        let mut procs = procs.borrow_mut();
        procs.wakeup(Rid::Rr(DevId::Pipe, devno, q));
        procs.wakeup(Rid::Wr(DevId::Pipe, devno, q));
    }
}

impl Dev for PipeDev {
    fn seteve(&mut self, eve: Eve) {
        self.eve = eve;
    }

    fn id(&self) -> DevId {
        DevId::Pipe
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    /// `pipeattach` — the allocation. Every attach is a NEW pipe.
    fn attach(&mut self, _spec: &str) -> Result<Chan, String> {
        self.pipes.push(Pipe { q: [Queue::new(PIPEQSIZE), Queue::new(PIPEQSIZE)], qref: [0, 0] });
        Ok(Chan::attach(DevId::Pipe, (self.pipes.len() - 1) as u32))
    }

    fn walk(&mut self, c: &Chan, name: &str) -> Result<Option<Chan>, String> {
        if c.qid.path != QDIR {
            return Err("not a directory".into());
        }
        Ok(match name {
            "data" => Some(Qid { qtype: 0, vers: 0, path: QDATA0 }),
            "data1" => Some(Qid { qtype: 0, vers: 0, path: QDATA1 }),
            ".." | "." => Some(Qid { qtype: QTDIR, vers: 0, path: QDIR }),
            _ => None,
        }
        .map(|q| c.walked(name, q)))
    }

    /// `pipeopen` (`devpipe.c:214`): count the open against its end.
    fn open(&mut self, mut c: Chan, mode: u16) -> Result<Chan, String> {
        if let Some((end, _)) = Self::ends(c.qid.path) {
            self.pipe(&c)?.qref[end] += 1;
        }
        c.mode = mode;
        c.flag |= COPEN;
        Ok(c)
    }

    /// A pipe has no `create`: the attach made it.
    fn create(&mut self, _c: &mut Chan, _n: &str, _m: u16, _p: u32) -> Result<(), String> {
        Err("permission denied".into())
    }

    /// `piperead` (`devpipe.c:299`) is `qread`. A pipe is a stream, so the
    /// offset is ignored.
    ///
    /// **A read may leave the processor** — waiting for `q->rlock`, or in
    /// `qwait` for data (`qio.c:866`) — and then answers nothing, because
    /// the call is not over: when the process is entered again the call
    /// comes back here and carries on from `again:`, holding the lock.
    fn read(&mut self, c: &mut Chan, n: usize, _off: u64) -> Result<Vec<u8>, String> {
        // `piperead` answers `pipedir[]` for the directory — two entries,
        // and their lengths are what is queued in each.
        if c.qid.is_dir() {
            let queued = {
                let p = self.pipe(c)?;
                [p.q[0].dlen() as u64, p.q[1].dlen() as u64]
            };
            let eve_ = self.eve.borrow().clone();
            let entries: Vec<crate::ninep::Dir> = [("data", QDATA0, 0), ("data1", QDATA1, 1)]
                .into_iter()
                .map(|(name, path, i)| {
                    let qid = Qid { qtype: 0, vers: 0, path };
                    crate::dev::devdir(c, qid, name, queued[i], &eve_, &eve_, 0o600)
                })
                .collect();
            return Ok(crate::dev::devdirread(c, n, &entries));
        }
        let (from, _) = Self::ends(c.qid.path).ok_or("cannot read that")?;
        self.pipe(c)?;
        self.qread(c.devno, from, n)
    }

    /// `pipewrite` (`devpipe.c:340`) is `qwrite` to the OTHER queue — this
    /// crossing is the pipe. It may leave the processor as a read may:
    /// waiting for `q->wlock`, waiting under the limit, or letting a
    /// higher-priority reader run first.
    ///
    /// Any error posts *"sys: write on closed pipe"* to the writer, as
    /// `pipewrite`'s `waserror` does (`devpipe.c:346`).
    fn write(&mut self, c: &mut Chan, data: &[u8], _off: u64) -> Result<usize, String> {
        let (_, to) = Self::ends(c.qid.path).ok_or("cannot write that")?;
        self.pipe(c)?;
        self.qwrite(c.devno, to, data)
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
                p.q[from].dlen() as u64
            }
            None => 0,
        };
        Ok(crate::dev::devdir(c, c.qid, name, length, &self.eve.borrow(), &self.eve.borrow(), perm).conv_d2m())
    }

    fn wstat(&mut self, _c: &mut Chan, _e: &[u8]) -> Result<(), String> {
        Err("permission denied".into())
    }

    fn remove(&mut self, _c: &mut Chan) -> Result<(), String> {
        Err("permission denied".into())
    }

    /// `pipeclose` (`devpipe.c:247`) — *"closing either side hangs up the
    /// stream"*. The last close of one end hangs up the queue the OTHER end
    /// reads, so its reader sees end of file once what is queued is gone,
    /// and closes its own; when both ends are closed, both reopen.
    /// Another reference to an open end: one more for `qref` to count down
    /// (`pipeclose`, `devpipe.c:247`).
    fn incref(&mut self, c: &Chan) {
        if c.flag & COPEN == 0 {
            return;
        }
        if let Some((end, _)) = Self::ends(c.qid.path) {
            if let Ok(p) = self.pipe(c) {
                p.qref[end] += 1;
            }
        }
    }

    fn close(&mut self, c: &mut Chan) {
        if c.flag & COPEN == 0 {
            return;
        }
        let Some((end, other)) = Self::ends(c.qid.path) else { return };
        let devno = c.devno;
        let Ok(p) = self.pipe(c) else { return };
        p.qref[end] = p.qref[end].saturating_sub(1);
        let hungup = p.qref[end] == 0;
        if hungup {
            p.q[other].hangup();
            p.q[end].close();
        }
        if p.qref == [0, 0] {
            p.q[0].reopen();
            p.q[1].reopen();
        }
        if hungup {
            self.wakeboth(devno, other);
            self.wakeboth(devno, end);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chan::mode::ORDWR;

    use crate::chan::flag::COPEN;
    use crate::proc::{Procs, State};

    /// `syspipe` (`sysfile.c`): attach `#|`, clone, walk one to `data` and the
    /// other to `data1`, open both. The two channels are the two ends.
    fn pipe() -> (PipeDev, Chan, Chan) {
        pipe_with().0
    }

    /// The same, with the table the device sleeps and wakes processes in.
    /// Pid 1 writes; pid 2, its child, reads — a pipe's two ends are two
    /// processes, and a device that keeps where each one is must be told
    /// which is calling.
    fn pipe_with() -> ((PipeDev, Chan, Chan), Rc<RefCell<Procs>>) {
        let procs = Rc::new(RefCell::new(Procs::new(Chan::attach(DevId::Root, 0))));
        procs.borrow_mut().rfork(1, crate::proc::rf::PROC).unwrap();
        let up = Rc::new(RefCell::new(Up { pid: 1, procs: procs.clone() }));
        let mut d = PipeDev::new(up);
        let dir = d.attach("").unwrap();
        let a = d.walk(&dir, "data").unwrap().unwrap();
        let b = d.walk(&dir, "data1").unwrap().unwrap();
        let a = d.open(a, ORDWR).unwrap();
        let b = d.open(b, ORDWR).unwrap();
        assert!(a.flag & COPEN != 0);
        ((d, a, b), procs)
    }

    /// Make `pid` the caller, as `syscall()` sets `up`.
    fn by(d: &PipeDev, pid: Pid) {
        d.up.borrow_mut().pid = pid;
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
        let ((mut d, mut a, _b), procs) = pipe_with();
        d.write(&mut a, b"hello", 0).unwrap();
        by(&d, 2);
        d.read(&mut a, 16, 0).unwrap();
        assert_eq!(procs.borrow().state(2), State::Wakeme, "it waits for the other end");
    }

    /// **`qread` on an empty queue sleeps** (`qwait`, `qio.c:866`), and the
    /// write at the other end is what wakes it — not end of file, which is
    /// what a reader that ran before its writer used to see.
    #[test]
    fn an_empty_pipe_puts_its_reader_to_sleep_until_a_write() {
        let ((mut d, mut a, mut b), procs) = pipe_with();
        by(&d, 2);
        d.read(&mut b, 16, 0).unwrap();
        assert_eq!(procs.borrow().state(2), State::Wakeme);
        by(&d, 1);
        d.write(&mut a, b"late", 0).unwrap();
        assert_eq!(procs.borrow().state(2), State::Ready, "the write woke it");
        by(&d, 2);
        assert_eq!(d.read(&mut b, 16, 0).unwrap(), b"late", "and the read carries on to find it");
    }

    /// **The last close of one end is end of file at the other**
    /// (`pipeclose`, `devpipe.c:258`) — after what is queued has been
    /// read, which `qhangup` leaves (`qio.c:1418`). A sleeping reader is
    /// woken by it.
    #[test]
    fn closing_the_writer_is_end_of_file_after_what_it_wrote() {
        let ((mut d, mut a, mut b), procs) = pipe_with();
        by(&d, 2);
        d.read(&mut b, 16, 0).unwrap();
        by(&d, 1);
        d.write(&mut a, b"last", 0).unwrap();
        d.close(&mut a);
        assert_eq!(procs.borrow().state(2), State::Ready);
        by(&d, 2);
        assert_eq!(d.read(&mut b, 16, 0).unwrap(), b"last");
        assert_eq!(d.read(&mut b, 16, 0).unwrap(), b"", "then end of file");
        assert!(d.write(&mut b, b"x", 0).is_err(), "and nobody reads what b writes");
    }

    /// Two opens of an end are two references (`qref`, `devpipe.c:231`):
    /// closing one leaves the pipe up.
    #[test]
    fn a_pipe_stays_up_while_any_open_of_the_writing_end_remains() {
        let ((mut d, mut a, mut b), _) = pipe_with();
        let mut a2 = d.open(a.clone(), ORDWR).unwrap();
        d.close(&mut a2);
        d.write(&mut a, b"still", 0).unwrap();
        assert_eq!(d.read(&mut b, 16, 0).unwrap(), b"still");
    }

    /// A read answers one write, not everything queued (`qio.c:1125`).
    #[test]
    fn a_read_answers_one_write() {
        let (mut d, mut a, mut b) = pipe();
        d.write(&mut a, b"one", 0).unwrap();
        d.write(&mut a, b"two", 0).unwrap();
        assert_eq!(d.read(&mut b, 99, 0).unwrap(), b"one");
        assert_eq!(d.read(&mut b, 99, 0).unwrap(), b"two");
    }

    /// Every attach is a new pipe (`pipeattach`, `devpipe.c:56`), so two
    /// pipes never share a queue.
    #[test]
    fn each_attach_mints_its_own_pipe() {
        let ((mut d, mut a, _), procs) = pipe_with();
        let dir = d.attach("").unwrap();
        let mut other = d.walk(&dir, "data1").unwrap().unwrap();
        d.write(&mut a, b"mine", 0).unwrap();
        by(&d, 2);
        d.read(&mut other, 16, 0).unwrap();
        assert_eq!(procs.borrow().state(2), State::Wakeme, "two pipes shared a queue");
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

    /// **Flow control** (`qbwrite`, `qio.c:1250`): a write that takes the
    /// queue to its limit queues its block and THEN waits under the limit;
    /// a read that brings it below half wakes it (`qwakeup_iunlock`,
    /// `:996`), and the write carries on — its block queued once.
    #[test]
    fn a_writer_waits_at_the_limit_and_a_read_below_half_lets_it_go() {
        let ((mut d, mut a, mut b), procs) = pipe_with();
        let big = vec![7u8; PIPEQSIZE];
        assert_eq!(d.write(&mut a, &big, 0).unwrap(), 0, "the call is not over");
        assert_eq!(procs.borrow().state(1), State::Wakeme, "waiting on q->wr");
        by(&d, 2);
        let got = d.read(&mut b, PIPEQSIZE, 0).unwrap();
        assert_eq!(got.len(), PIPEQSIZE, "one block, all of it");
        assert_eq!(procs.borrow().state(1), State::Ready, "below half: the writer is woken");
        by(&d, 1);
        assert_eq!(d.write(&mut a, &big, 0).unwrap(), PIPEQSIZE, "and finishes");
        by(&d, 2);
        d.read(&mut b, 16, 0).unwrap();
        assert_eq!(procs.borrow().state(2), State::Wakeme, "the block was queued once, not twice");
    }

    /// **One reader at a time** — `q->rlock` (`qio.c:1075`). A second reader
    /// waits `Queueing` on the lock, and gets it when the first is done.
    #[test]
    fn a_second_reader_queues_for_the_lock() {
        let ((mut d, mut a, mut b), procs) = pipe_with();
        let third = procs.borrow_mut().rfork(1, crate::proc::rf::PROC).unwrap();
        by(&d, 2);
        d.read(&mut b, 16, 0).unwrap();
        by(&d, third);
        d.read(&mut b, 16, 0).unwrap();
        assert_eq!(procs.borrow().state(third), State::Queueing, "waiting for q->rlock");
        by(&d, 1);
        d.write(&mut a, b"first", 0).unwrap();
        by(&d, 2);
        assert_eq!(d.read(&mut b, 16, 0).unwrap(), b"first");
        assert_eq!(procs.borrow().state(third), State::Ready, "qunlock handed it over");
        by(&d, 1);
        d.write(&mut a, b"second", 0).unwrap();
        by(&d, third);
        assert_eq!(d.read(&mut b, 16, 0).unwrap(), b"second");
    }

    /// *"if we just wokeup a higher priority process, let it run"*
    /// (`qio.c:1234`): the writer leaves the processor, still `Running`, and
    /// finishes when it is entered again.
    #[test]
    fn waking_a_higher_priority_reader_lets_it_run_first() {
        let ((mut d, mut a, mut b), procs) = pipe_with();
        by(&d, 2);
        d.read(&mut b, 16, 0).unwrap();
        procs.borrow_mut().get_mut(2).unwrap().priority = crate::proc::pri::NORMAL + 1;
        procs.borrow_mut().get_mut(2).unwrap().basepri = crate::proc::pri::NORMAL + 1;
        by(&d, 1);
        assert_eq!(d.write(&mut a, b"go", 0).unwrap(), 0, "it left before finishing");
        assert!(procs.borrow_mut().take_setlabel(1), "sched() from inside the write");
        assert_eq!(d.write(&mut a, b"go", 0).unwrap(), 2, "and comes back to finish");
        by(&d, 2);
        assert_eq!(d.read(&mut b, 16, 0).unwrap(), b"go", "queued once");
    }
}
