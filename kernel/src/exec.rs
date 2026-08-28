// A single-threaded executor of the kernel's own — no tokio, no threads.
// The reference kernel is async throughout because devmnt must suspend in
// the middle of a walk while an R-message crosses a pipe; this is the same
// property with Rust spelling. Tasks are plain boxed futures; wakers push
// task ids onto a ready queue; the host loop polls until quiescent after
// every event. Everything here is !Send by design: one thread owns the
// kernel, exactly as one event loop owns the JS reference.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

type Task = Pin<Box<dyn Future<Output = ()>>>;

pub struct LocalExec {
    tasks: RefCell<Vec<Option<Task>>>,
    ready: Rc<RefCell<VecDeque<usize>>>,
    spawned: RefCell<Vec<Task>>,
}

impl LocalExec {
    pub fn new() -> Rc<LocalExec> {
        Rc::new(LocalExec {
            tasks: RefCell::new(Vec::new()),
            ready: Rc::new(RefCell::new(VecDeque::new())),
            spawned: RefCell::new(Vec::new()),
        })
    }

    pub fn spawn(&self, fut: impl Future<Output = ()> + 'static) {
        // deferred: a task spawned from inside a poll lands in `spawned`
        if std::env::var("KEXEC").is_ok() {
            eprintln!("[X spawn; spawned={} ]", self.spawned.borrow().len() + 1);
        }
        self.spawned.borrow_mut().push(Box::pin(fut));
    }

    /// Poll until no task can make progress. Called by the host loop after
    /// every event; completions queued by the event's handling wake tasks.
    pub fn run_until_stalled(&self) {
        loop {
            // adopt newly spawned tasks
            {
                let mut spawned = self.spawned.borrow_mut();
                if !spawned.is_empty() {
                    let mut tasks = self.tasks.borrow_mut();
                    for t in spawned.drain(..) {
                        let id = tasks.iter().position(|s| s.is_none()).unwrap_or_else(|| {
                            tasks.push(None);
                            tasks.len() - 1
                        });
                        tasks[id] = Some(t);
                        if std::env::var("KEXEC").is_ok() {
                            eprintln!("[X adopt id={}]", id);
                        }
                        self.ready.borrow_mut().push_back(id);
                    }
                }
            }
            let id = match self.ready.borrow_mut().pop_front() {
                Some(id) => id,
                None => {
                    if self.spawned.borrow().is_empty() {
                        return;
                    }
                    continue;
                }
            };
            let task = self.tasks.borrow_mut().get_mut(id).and_then(|s| s.take());
            let Some(mut task) = task else { continue };
            let waker = ready_waker(self.ready.clone(), id);
            let mut cx = Context::from_waker(&waker);
            match task.as_mut().poll(&mut cx) {
                Poll::Ready(()) => {} // slot stays None: free
                Poll::Pending => {
                    self.tasks.borrow_mut()[id] = Some(task);
                }
            }
        }
    }
}

// waker: clone-able handle pushing the task id onto the ready queue
struct WakeState {
    ready: Rc<RefCell<VecDeque<usize>>>,
    id: usize,
}

fn ready_waker(ready: Rc<RefCell<VecDeque<usize>>>, id: usize) -> Waker {
    let state = Rc::new(WakeState { ready, id });
    unsafe { Waker::from_raw(raw_waker(state)) }
}

fn raw_waker(state: Rc<WakeState>) -> RawWaker {
    unsafe fn clone(p: *const ()) -> RawWaker {
        let rc = Rc::from_raw(p as *const WakeState);
        let out = raw_waker(rc.clone());
        std::mem::forget(rc);
        out
    }
    unsafe fn wake(p: *const ()) {
        let rc = Rc::from_raw(p as *const WakeState);
        rc.ready.borrow_mut().push_back(rc.id);
    }
    unsafe fn wake_by_ref(p: *const ()) {
        let rc = Rc::from_raw(p as *const WakeState);
        rc.ready.borrow_mut().push_back(rc.id);
        std::mem::forget(rc);
    }
    unsafe fn drop_raw(p: *const ()) {
        drop(Rc::from_raw(p as *const WakeState));
    }
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop_raw);
    RawWaker::new(Rc::into_raw(state) as *const (), &VTABLE)
}

// ---- oneshot: the parked read's shape. The device holds the Completer;
// the suspended kernel task holds the Waiting future. Completing twice is
// harmless (the slot keeps the first value) — which is exactly what the
// interrupt path needs: postnote completes with Interrupted, and the
// device's late completion finds the slot already taken. ----

pub struct OnceCell<T> {
    slot: RefCell<Option<T>>,
    waker: RefCell<Option<Waker>>,
}

pub struct Completer<T>(Rc<OnceCell<T>>);
impl<T> Clone for Completer<T> {
    fn clone(&self) -> Self {
        Completer(self.0.clone())
    }
}
pub struct Waiting<T>(Rc<OnceCell<T>>);

pub fn oneshot<T>() -> (Completer<T>, Waiting<T>) {
    let c = Rc::new(OnceCell { slot: RefCell::new(None), waker: RefCell::new(None) });
    (Completer(c.clone()), Waiting(c))
}

impl<T> Completer<T> {
    pub fn complete(&self, v: T) {
        {
            let mut slot = self.0.slot.borrow_mut();
            if slot.is_some() {
                return; // first completion wins
            }
            *slot = Some(v);
        }
        if let Some(w) = self.0.waker.borrow_mut().take() {
            w.wake();
        }
    }
    pub fn is_done(&self) -> bool {
        self.0.slot.borrow().is_some()
    }
}

impl<T> Future for Waiting<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        if let Some(v) = self.0.slot.borrow_mut().take() {
            return Poll::Ready(v);
        }
        *self.0.waker.borrow_mut() = Some(cx.waker().clone());
        Poll::Pending
    }
}
