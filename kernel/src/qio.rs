//! Queues — `plan9/sys/src/9/port/qio.c`.
//!
//! **One queue implementation for every device that streams**, as Plan 9 has
//! one: a pipe's two ends are two of these (`devpipe.c:69`), and a uart's
//! input and output are two more (`devuart.c`, `uartenable`). It was a
//! pipe's private type until the uart needed it too.
//!
//! This kernel has no kernel stack per process, so a call that sleeps runs
//! again from the top when the process is entered again; [`At`] is where in
//! `qread` or `qwrite` it was, kept by the device and handed in each time.

use crate::dev::DevId;
use crate::proc::{NoteFlag, Pid, Procs, QLock, Rid, EINTR};
use std::collections::{HashMap, VecDeque};

/// `Maxatomic` (`qio.c:54`) — the most one block holds; a longer write is
/// several (`qwrite`, `qio.c:1280`).
pub const MAXATOMIC: usize = 64 * 1024;

/// `Hdrspc` (`allocb.c:13`) and `BLOCKALIGN` (`pc/mem.h:17`): a block's
/// allocation is the room left for headers and the data rounded up, and
/// that — `BALLOC`, not the data — is what `q->len` counts against the
/// limit (`qio.c:1213`). The allocator's own rounding (`msize`) is the
/// allocator's.
fn balloc(n: usize) -> usize {
    64 + n.div_ceil(8) * 8
}

/// `Ehungup` (`error.h:24`) — *"i/o on hungup channel"*.
pub const EHUNGUP: &str = "i/o on hungup channel";

/// `struct Queue` (`portdat.h`), as much of it as this kernel's devices use.
///
/// **A block is one write** (up to `Maxatomic`), and `qread` answers at
/// most one: *"first = qremove(q); n = BLEN(first)"* (`qio.c:1125`). So
/// writes keep their boundaries at the reader.
pub struct Queue {
    /// The blocks, each with its `BALLOC`.
    blocks: VecDeque<(Vec<u8>, usize)>,
    /// `q->len` — the sum of `BALLOC` over the blocks.
    len: usize,
    /// `q->limit`, and the limit `qopen` was given, which `qreopen`
    /// restores (`qio.c:1447`).
    limit: usize,
    inilim: usize,
    /// `Qclosed`, `Qstarve`, `Qflow` (`portdat.h`).
    pub closed: bool,
    starve: bool,
    flow: bool,
    /// `Qcoalesce` — a read takes as many whole blocks as fit, not one
    /// (`qio.c:1097`). `kprint`'s queue is one (`devcons.c:708`).
    coalesce: bool,
    /// `q->noblock` — *"true if writes return immediately when q full"*
    /// (`qio.c:37`): a write over the limit is dropped rather than waited.
    noblock: bool,
    /// `q->eof` — reads answered 0 since it closed; the fourth is an error
    /// (`qwait`, `qio.c:857`).
    eof: u32,
    /// `q->err`.
    pub err: String,
    /// `q->rlock`, `q->wlock` — one reader and one writer at a time
    /// (`qread`, `qio.c:1075`; `qbwrite`, `:1178`).
    pub rlock: QLock,
    pub wlock: QLock,
}

impl Queue {
    /// `qopen(limit, 0, …)` (`qio.c:798`): *"q->state |= Qstarve"*.
    pub fn new(limit: usize) -> Queue {
        Queue {
            blocks: VecDeque::new(),
            len: 0,
            limit,
            inilim: limit,
            closed: false,
            starve: true,
            flow: false,
            coalesce: false,
            noblock: false,
            eof: 0,
            err: String::new(),
            rlock: QLock::default(),
            wlock: QLock::default(),
        }
    }

    /// `qopen(limit, Qcoalesce, …)`.
    pub fn coalescing(mut self) -> Queue {
        self.coalesce = true;
        self
    }

    /// `qlen` (`qio.c:1461`) — `q->dlen`, the bytes queued.
    pub fn dlen(&self) -> usize {
        self.blocks.iter().map(|b| b.0.len()).sum()
    }

    /// `qhangup` (`qio.c:1418`): closed, but what is queued stays readable.
    pub fn hangup(&mut self) {
        self.closed = true;
        self.err = EHUNGUP.into();
    }

    /// `qclose` (`qio.c:1386`): closed, `Qflow` and `Qstarve` cleared,
    /// `noblock` off, and the blocks freed.
    pub fn close(&mut self) {
        self.hangup();
        self.flow = false;
        self.starve = false;
        self.noblock = false;
        self.blocks.clear();
        self.len = 0;
    }

    /// `qreopen` (`qio.c:1447`).
    pub fn reopen(&mut self) {
        self.closed = false;
        self.starve = true;
        self.eof = 0;
        self.limit = self.inilim;
    }

    /// `qsetlimit` (`qio.c:1493`).
    pub fn setlimit(&mut self, limit: usize) {
        self.limit = limit;
    }

    /// `qnoblock` (`qio.c:1502`).
    pub fn noblock(&mut self, on: bool) {
        self.noblock = on;
    }

    /// `qflush` (`qio.c:1432`) — *"mark it"* and free what is queued.
    pub fn flush(&mut self) {
        self.blocks.clear();
        self.len = 0;
    }

    /// `qproduce` (`qio.c:692`) — what arrives at interrupt time, which
    /// never waits: *"no waiting receivers, room in buffer?"* — past the
    /// limit it answers -1 and the bytes are the caller's to drop. Answers
    /// whether a starving reader must be woken on `Rr(dev, devno, which)`.
    pub fn produce(&mut self, data: &[u8]) -> Result<bool, ()> {
        if self.len >= self.limit {
            self.flow = true;
            return Err(());
        }
        let alloc = balloc(data.len());
        self.blocks.push_back((data.to_vec(), alloc));
        self.len += alloc;
        let wake = std::mem::take(&mut self.starve);
        if self.len >= self.limit {
            self.flow = true;
        }
        Ok(wake)
    }

    /// `qconsume` (`qio.c:498`) — take up to `len` bytes without waiting,
    /// as an output interrupt takes the next bytes to send. `None` is
    /// *"q->state |= Qstarve; return -1"*: nothing queued. The second value
    /// is whether a writer waiting for room must be woken.
    ///
    /// An empty block — a write of nothing — is freed and passed over, as
    /// `qconsume`'s loop does (*"n = BLEN(b); if(n > 0) break;"*,
    /// `qio.c:517`).
    pub fn consume(&mut self, len: usize) -> (Option<Vec<u8>>, bool) {
        let (mut b, alloc) = loop {
            let Some((b, alloc)) = self.blocks.pop_front() else {
                self.starve = true;
                return (None, false);
            };
            if !b.is_empty() {
                break (b, alloc);
            }
            self.len -= alloc;
        };
        if b.len() > len {
            let rest = b.split_off(len);
            self.blocks.push_front((rest, alloc));
        } else {
            self.len -= alloc;
        }
        let wake = self.flow && self.len < self.limit / 2;
        if wake {
            self.flow = false;
        }
        (Some(b), wake)
    }
}

/// **Where a process that left in the middle of a read or a write was** —
/// the part of its kernel stack that is the queue's. Plan 9 keeps it on the
/// stack itself; a process entered again comes back through the same
/// method, and this says which line it is on.
pub enum At {
    /// `qread`, holding `q->rlock`, at `again:` (`qio.c:1082`) — whether it
    /// waited for the lock or `slept` in `qwait`, it holds the lock now. A
    /// `sleep` a note ended is `Eintr`; waiting for a `QLock` is not a
    /// sleep and no note ends it.
    Read { slept: bool },
    /// `qwrite`, with `sofar` written, waiting for `q->wlock`: the next
    /// block is not queued yet.
    Wlock { sofar: usize },
    /// `qwrite`, with `sofar` written and the next block queued: in
    /// `qbwrite`'s flow-control loop (`qio.c:1250`), having `slept` there,
    /// or just back from the `sched()` that let a higher-priority reader run
    /// (`:1235`).
    Flow { sofar: usize, slept: bool },
}

/// Which queue: the device, its instance, and which of its queues — what
/// `q->rr` and `q->wr` are the addresses of.
#[derive(Clone, Copy)]
pub struct Qid3 {
    pub dev: DevId,
    pub devno: u32,
    pub which: usize,
}

/// `qread` (`qio.c:1070`).
///
/// `kick` is the queue's `q->kick`, which `qwakeup_iunlock` calls when a
/// read takes a flow-controlled queue below half its limit (`qio.c:1005`)
/// — a uart's `uartflow`, which raises RTS again.
///
/// **A read may leave the processor** — waiting for `q->rlock`, or in
/// `qwait` for data (`qio.c:866`) — and then answers nothing, because the
/// call is not over: when the process is entered again the call comes back
/// here and carries on from `again:`, holding the lock.
pub fn qread(
    q: &mut Queue,
    at: &mut HashMap<Pid, At>,
    procs: &mut Procs,
    pid: Pid,
    id: Qid3,
    n: usize,
    kick: &mut dyn FnMut(&mut Queue, &mut Procs),
) -> Result<Vec<u8>, String> {
    let was = at.remove(&pid);
    // `qwait`'s `sleep` ended by a note: *"error(Eintr)"* (`proc.c:884`),
    // and `qread`'s `waserror` lets go of the lock (`qio.c:1076`).
    if matches!(was, Some(At::Read { slept: true })) && procs.interrupted(pid) {
        procs.qunlock(&mut q.rlock);
        return Err(EINTR.into());
    }
    // *"qlock(&q->rlock)"*.
    if was.is_none() && !procs.qlock(&mut q.rlock, pid) {
        at.insert(pid, At::Read { slept: false });
        return Ok(Vec::new());
    }
    // *"when coalescing, 0 length blocks just go away"* (`qio.c:1098`).
    if q.coalesce {
        while q.blocks.front().is_some_and(|b| b.0.is_empty()) {
            let (_, alloc) = q.blocks.pop_front().unwrap_or_default();
            q.len -= alloc;
        }
    }
    // `again:` — `qwait` (`qio.c:849`).
    if q.blocks.is_empty() {
        if q.closed {
            q.eof += 1;
            let r = if q.eof > 3 || q.err != EHUNGUP { Err(q.err.clone()) } else { Ok(Vec::new()) };
            procs.qunlock(&mut q.rlock);
            return r;
        }
        // *"q->state |= Qstarve; … sleep(&q->rr, notempty, q);"*
        q.starve = true;
        if !procs.sleep(pid, Rid::Rr(id.dev, id.devno, id.which), false) {
            // A note already pending: `sleep` did not commit.
            procs.interrupted(pid);
            procs.qunlock(&mut q.rlock);
            return Err(EINTR.into());
        }
        at.insert(pid, At::Read { slept: true });
        return Ok(Vec::new());
    }
    // One block — or, coalescing, *"the first block plus as many following
    // blocks as will completely fit in the read"* — and what of the first
    // does not fit goes back (`qputback`), still counted at its whole
    // allocation.
    let mut b = Vec::new();
    while let Some((mut next, alloc)) = q.blocks.pop_front() {
        let room = n - b.len();
        if next.len() > room {
            let rest = next.split_off(room);
            q.blocks.push_front((rest, alloc));
            b.extend(next);
            break;
        }
        q.len -= alloc;
        b.extend(next);
        if !q.coalesce || q.blocks.front().is_none_or(|f| b.len() + f.0.len() > n) {
            break;
        }
    }
    // `qwakeup_iunlock` (`qio.c:991`): *"if writer flow controlled,
    // restart"* once the queue is below half its limit.
    if q.flow && q.len < q.limit / 2 {
        q.flow = false;
        kick(q, procs);
        procs.wakeup(Rid::Wr(id.dev, id.devno, id.which));
    }
    procs.qunlock(&mut q.rlock);
    Ok(b)
}

/// `qwrite` (`qio.c:1270`) — blocks of at most `Maxatomic`, each through
/// `qbwrite` (`:1165`). A write of nothing is one empty block, as the
/// `do … while` makes it.
///
/// `failnote` is what the device's `waserror` posts to a writer whose write
/// failed — a pipe's *"sys: write on closed pipe"* (`devpipe.c:349`).
/// `kick` is the queue's `q->kick` (`qopen`'s third argument), called
/// *"if(q->kick && (dowakeup || (q->state&Qkick)))"* (`qio.c:1226`) — a
/// uart's `uartkick`, which starts output.
#[allow(clippy::too_many_arguments)]
pub fn qwrite(
    q: &mut Queue,
    at: &mut HashMap<Pid, At>,
    procs: &mut Procs,
    pid: Pid,
    id: Qid3,
    data: &[u8],
    failnote: Option<&str>,
    kick: &mut dyn FnMut(&mut Queue, &mut Procs),
) -> Result<usize, String> {
    let fail = |procs: &mut Procs| {
        if let Some(note) = failnote {
            procs.postnote(pid, note, NoteFlag::NUser);
        }
    };
    let (mut sofar, mut resumed) = match at.remove(&pid) {
        Some(At::Wlock { sofar }) => (sofar, Some(false)),
        Some(At::Flow { sofar, slept }) => {
            // The flow-control `sleep` ended by a note: `qbwrite`'s
            // `waserror` lets go of `q->wlock` (`qio.c:1179`), and the
            // device's posts its note.
            if slept && procs.interrupted(pid) {
                procs.qunlock(&mut q.wlock);
                fail(procs);
                return Err(EINTR.into());
            }
            (sofar, Some(true))
        }
        _ => (0, None),
    };
    loop {
        let n = (data.len() - sofar).min(MAXATOMIC);
        let queued = match resumed.take() {
            Some(queued) => queued,
            None => {
                // *"qlock(&q->wlock)"*.
                if !procs.qlock(&mut q.wlock, pid) {
                    at.insert(pid, At::Wlock { sofar });
                    return Ok(0);
                }
                false
            }
        };
        if !queued {
            // *"give up if the queue is closed"* — and `waserror` unlocks
            // on the way out.
            if q.closed {
                procs.qunlock(&mut q.wlock);
                fail(procs);
                return Err(q.err.clone());
            }
            // *"if nonblocking, don't queue over the limit"* — the block
            // is dropped and counted as written.
            if q.len >= q.limit && q.noblock {
                procs.qunlock(&mut q.wlock);
                sofar += n;
                if sofar >= data.len() {
                    return Ok(data.len());
                }
                continue;
            }
            let alloc = balloc(n);
            q.blocks.push_back((data[sofar..sofar + n].to_vec(), alloc));
            q.len += alloc;
            // *"make sure other end gets awakened"* — only a reader that
            // said it was starving; *"get output going again"* first.
            if std::mem::take(&mut q.starve) {
                kick(q, procs);
                let woke = procs.wakeup(Rid::Rr(id.dev, id.devno, id.which));
                // *"if we just wokeup a higher priority process, let it
                // run"* — and come back to the flow-control loop.
                let pri = |p: Pid| procs.get(p).map_or(0, |p| p.priority);
                if woke.is_some_and(|p| pri(p) > pri(pid)) {
                    procs.setlabel(pid);
                    at.insert(pid, At::Flow { sofar, slept: false });
                    return Ok(0);
                }
            }
        }
        // *"flow control, wait for queue to get below the limit"* —
        // `qnotfull` (`qio.c:1152`).
        if !(q.noblock || q.len < q.limit || q.closed) {
            q.flow = true;
            if !procs.sleep(pid, Rid::Wr(id.dev, id.devno, id.which), false) {
                procs.interrupted(pid);
                procs.qunlock(&mut q.wlock);
                fail(procs);
                return Err(EINTR.into());
            }
            at.insert(pid, At::Flow { sofar, slept: true });
            return Ok(0);
        }
        procs.qunlock(&mut q.wlock);
        sofar += n;
        if sofar >= data.len() {
            return Ok(data.len());
        }
    }
}
