//! A bounded job queue drained by N worker threads (ADR-SAG-0032).
//!
//! The pool is generic over the job and answer types and knows nothing about
//! Jev: it is the "queue plus workers" half of streaming, built from `std`
//! threads, a `Mutex` and two `Condvar`s. No async runtime.
//!
//! Both queues are bounded, so memory is bounded:
//!
//! - [`WorkerPool::submit`] blocks while the job queue is full;
//! - a worker blocks on its answer while the answer queue is full, so a slow
//!   consumer stops the workers, then fills the job queue, then blocks the
//!   submitter.
//!
//! At most [`WorkerPool::capacity`] jobs are therefore in the pool at once
//! (queued, running, or answered and not yet received).
//!
//! # Finished
//!
//! A pool is finished when its input is closed ([`WorkerPool::close_input`]),
//! the job queue is empty and no worker is running a job. The last worker out
//! closes the answer queue, so [`WorkerPool::recv`] returns `None` exactly then,
//! after every answer has been received.
//!
//! # Cancellation
//!
//! [`WorkerPool::cancel`] sets the [`CancelToken`] and closes the input. Jobs
//! already queued are not run: each gets its answer from the `skipped`
//! function with [`Skipped::Cancelled`], so every accepted job still has
//! exactly one answer. A job already running is not interrupted here; the
//! token it carries to the engine ends its busy-retry wait.

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use sc_agent_types::CancelToken;

/// Why a job got its answer without being run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Skipped {
    /// The pool was cancelled before the job started.
    Cancelled,
    /// The run function panicked on this job.
    Panicked,
}

/// A bounded multi-producer multi-consumer FIFO.
struct BoundedQueue<T> {
    state: Mutex<QueueState<T>>,
    not_empty: Condvar,
    not_full: Condvar,
    capacity: usize,
}

struct QueueState<T> {
    items: VecDeque<T>,
    /// No more pushes. Pops drain what is left, then return `None`.
    closed: bool,
    /// Nobody will pop: pushes are refused and queued items are dropped.
    discarded: bool,
    /// Items ever pushed, counted under this lock with the push itself.
    pushed: usize,
    /// Items ever popped, counted under this lock with the pop itself.
    popped: usize,
}

/// Why a push did not enqueue; the item comes back.
pub(crate) enum PushError<T> {
    /// The queue is closed.
    Closed(T),
    /// The queue stayed full for the whole wait.
    Full(T),
}

/// What a timed pop found.
pub(crate) enum Popped<T> {
    /// The next item.
    Item(T),
    /// Nothing arrived in time; the queue is still open.
    Empty,
    /// Closed and drained.
    Finished,
}

impl<T> BoundedQueue<T> {
    fn new(capacity: usize) -> Self {
        Self {
            state: Mutex::new(QueueState {
                items: VecDeque::new(),
                closed: false,
                discarded: false,
                pushed: 0,
                popped: 0,
            }),
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
            capacity,
        }
    }

    fn lock(&self) -> MutexGuard<'_, QueueState<T>> {
        // A poisoned queue is still a consistent queue: no code runs while
        // the lock is held except plain `VecDeque` operations.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Pushes `item`, waiting at most `wait` (forever when `None`) for room.
    fn push(&self, item: T, wait: Option<Duration>) -> Result<(), PushError<T>> {
        let deadline = wait.map(|wait| Instant::now() + wait);
        let mut state = self.lock();
        loop {
            if state.closed || state.discarded {
                return Err(PushError::Closed(item));
            }
            if state.items.len() < self.capacity {
                state.items.push_back(item);
                state.pushed += 1;
                drop(state);
                self.not_empty.notify_one();
                return Ok(());
            }
            state = match deadline {
                None => self
                    .not_full
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner),
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(PushError::Full(item));
                    }
                    self.not_full
                        .wait_timeout(state, left)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0
                }
            };
        }
    }

    /// Pops the next item, waiting at most `wait` (forever when `None`).
    fn pop(&self, wait: Option<Duration>) -> Popped<T> {
        let deadline = wait.map(|wait| Instant::now() + wait);
        let mut state = self.lock();
        loop {
            if let Some(item) = state.items.pop_front() {
                state.popped += 1;
                drop(state);
                self.not_full.notify_one();
                return Popped::Item(item);
            }
            if state.closed || state.discarded {
                return Popped::Finished;
            }
            state = match deadline {
                None => self
                    .not_empty
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner),
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Popped::Empty;
                    }
                    self.not_empty
                        .wait_timeout(state, left)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0
                }
            };
        }
    }

    fn pushed(&self) -> usize {
        self.lock().pushed
    }

    fn popped(&self) -> usize {
        self.lock().popped
    }

    fn close(&self) {
        self.lock().closed = true;
        self.not_empty.notify_all();
        self.not_full.notify_all();
    }

    fn discard(&self) {
        {
            let mut state = self.lock();
            state.discarded = true;
            state.items.clear();
        }
        self.not_empty.notify_all();
        self.not_full.notify_all();
    }
}

/// Counters that are not tied to a queue lock.
struct Counters {
    /// Workers currently running a job.
    running: AtomicUsize,
    /// Workers still alive.
    alive: AtomicUsize,
}

type RunFn<J, A> = dyn Fn(&J) -> A + Send + Sync;
type SkipFn<J, A> = dyn Fn(&J, Skipped) -> A + Send + Sync;
type LostFn<J, A> = dyn Fn(&J) -> A + Send + Sync;

/// A fixed set of worker threads draining a bounded job queue.
pub(crate) struct WorkerPool<J, A> {
    jobs: Arc<BoundedQueue<J>>,
    answers: Arc<BoundedQueue<A>>,
    handles: Mutex<Vec<JoinHandle<()>>>,
    cancel: CancelToken,
    counters: Arc<Counters>,
    workers: usize,
    queue_capacity: usize,
}

impl<J: Send + 'static, A: Send + 'static> WorkerPool<J, A> {
    /// Starts `workers` threads (at least one). Each queue holds
    /// `queue_capacity` items (at least one). `run` turns a job into its
    /// answer; `skipped` does so for a job that is not run. `lost` is the
    /// last resort when `skipped` itself panics: it must not panic, so every
    /// accepted job gets exactly one answer.
    pub(crate) fn start(
        workers: usize,
        queue_capacity: usize,
        cancel: CancelToken,
        run: Arc<RunFn<J, A>>,
        skipped: Arc<SkipFn<J, A>>,
        lost: Arc<LostFn<J, A>>,
    ) -> Self {
        let workers = workers.max(1);
        let queue_capacity = queue_capacity.max(1);
        let jobs = Arc::new(BoundedQueue::new(queue_capacity));
        let answers = Arc::new(BoundedQueue::new(queue_capacity));
        let counters = Arc::new(Counters {
            running: AtomicUsize::new(0),
            alive: AtomicUsize::new(workers),
        });
        let handles = (0..workers)
            .map(|_| {
                let worker = Worker {
                    jobs: Arc::clone(&jobs),
                    answers: Arc::clone(&answers),
                    counters: Arc::clone(&counters),
                    cancel: cancel.clone(),
                    run: Arc::clone(&run),
                    skipped: Arc::clone(&skipped),
                    lost: Arc::clone(&lost),
                };
                thread::spawn(move || worker.work())
            })
            .collect();
        Self {
            jobs,
            answers,
            handles: Mutex::new(handles),
            cancel,
            counters,
            workers,
            queue_capacity,
        }
    }

    /// Submits `job`, waiting at most `wait` (forever when `None`) for room.
    pub(crate) fn submit(&self, job: J, wait: Option<Duration>) -> Result<(), PushError<J>> {
        if self.cancel.is_cancelled() {
            return Err(PushError::Closed(job));
        }
        self.jobs.push(job, wait)
    }

    /// No more jobs will be submitted; the pool finishes when it drains.
    pub(crate) fn close_input(&self) {
        self.jobs.close();
    }

    /// Stops new work: see the module documentation.
    pub(crate) fn cancel(&self) {
        self.cancel.cancel();
        self.jobs.close();
    }

    /// Waits for the next answer; `None` once the pool is finished.
    pub(crate) fn recv(&self) -> Option<A> {
        match self.recv_within(None) {
            Popped::Item(answer) => Some(answer),
            Popped::Empty | Popped::Finished => None,
        }
    }

    /// Waits at most `wait` (forever when `None`) for the next answer.
    pub(crate) fn recv_within(&self, wait: Option<Duration>) -> Popped<A> {
        self.answers.pop(wait)
    }

    /// Jobs submitted whose answer was not yet received.
    ///
    /// Both counts are kept under their queue's lock, atomic with the push
    /// and the pop. They are read one after the other, `submitted` first, so
    /// the sample can read **low** (a job was submitted in between) but never
    /// high: it never exceeds [`Self::capacity`].
    pub(crate) fn in_flight(&self) -> usize {
        let submitted = self.jobs.pushed();
        let received = self.answers.popped();
        submitted.saturating_sub(received)
    }

    /// Workers running a job right now.
    pub(crate) fn running(&self) -> usize {
        self.counters.running.load(Ordering::Acquire)
    }

    /// Worker threads still alive.
    pub(crate) fn alive(&self) -> usize {
        self.counters.alive.load(Ordering::Acquire)
    }

    /// The configured number of workers.
    pub(crate) fn workers(&self) -> usize {
        self.workers
    }

    /// The most jobs that can be in the pool at once: a full job queue, one
    /// job per worker, and a full answer queue.
    pub(crate) fn capacity(&self) -> usize {
        self.queue_capacity * 2 + self.workers
    }

    /// Joins the worker threads. Call after [`Self::recv`] returned `None`,
    /// or after [`Self::abandon`]; the workers have exited or are about to.
    pub(crate) fn join(&self) {
        let handles =
            std::mem::take(&mut *self.handles.lock().unwrap_or_else(PoisonError::into_inner));
        for handle in handles {
            // A worker catches its job's panic, so a join error is a bug in
            // the worker loop itself; there is nobody left to tell.
            let _ = handle.join();
        }
    }

    /// Stops everything and drops unreceived answers, so no worker can stay
    /// blocked on a consumer that is gone. Does not wait for the workers.
    pub(crate) fn abandon(&self) {
        self.cancel();
        self.answers.discard();
    }
}

impl<J, A> Drop for WorkerPool<J, A> {
    fn drop(&mut self) {
        // The workers finish on their own; they are not joined here because
        // a running job may be an in-flight request.
        self.cancel.cancel();
        self.jobs.close();
        self.answers.discard();
    }
}

struct Worker<J, A> {
    jobs: Arc<BoundedQueue<J>>,
    answers: Arc<BoundedQueue<A>>,
    counters: Arc<Counters>,
    cancel: CancelToken,
    run: Arc<RunFn<J, A>>,
    skipped: Arc<SkipFn<J, A>>,
    lost: Arc<LostFn<J, A>>,
}

/// Runs on every exit of a worker, a panic included: the last worker out
/// closes the answer queue, so the stream cannot hang on a dead worker.
struct Exit<A> {
    counters: Arc<Counters>,
    answers: Arc<BoundedQueue<A>>,
}

impl<A> Drop for Exit<A> {
    fn drop(&mut self) {
        if self.counters.alive.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.answers.close();
        }
    }
}

impl<J, A> Worker<J, A> {
    fn work(self) {
        let _exit = Exit {
            counters: Arc::clone(&self.counters),
            answers: Arc::clone(&self.answers),
        };
        while let Popped::Item(job) = self.jobs.pop(None) {
            let answer = if self.cancel.is_cancelled() {
                self.skip(&job, Skipped::Cancelled)
            } else {
                self.counters.running.fetch_add(1, Ordering::AcqRel);
                let outcome = catch_unwind(AssertUnwindSafe(|| (self.run)(&job)));
                self.counters.running.fetch_sub(1, Ordering::AcqRel);
                outcome.unwrap_or_else(|_| self.skip(&job, Skipped::Panicked))
            };
            // A refused push means the consumer is gone: drop the answer.
            let _ = self.answers.push(answer, None);
        }
    }

    /// The answer for a job that was not run; `skipped` may panic (it logs).
    fn skip(&self, job: &J, why: Skipped) -> A {
        catch_unwind(AssertUnwindSafe(|| (self.skipped)(job, why)))
            .unwrap_or_else(|_| (self.lost)(job))
    }
}

#[cfg(test)]
mod tests;
