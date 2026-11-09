//! Pool mechanics with plain closures: no Jev, no network.
//!
//! No test asserts on elapsed time. Every wait that could hang has a
//! generous failsafe deadline that turns a deadlock into a failure.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use super::*;

/// Failsafe for waits that must finish; never a measurement.
const FAILSAFE: Duration = Duration::from_secs(60);

/// A barrier that gives up instead of hanging: `arrive` is true only when
/// `parties` threads were inside it at the same moment.
struct Rendezvous {
    parties: usize,
    arrived: Mutex<usize>,
    all_here: Condvar,
}

impl Rendezvous {
    fn new(parties: usize) -> Self {
        Self {
            parties,
            arrived: Mutex::new(0),
            all_here: Condvar::new(),
        }
    }

    fn arrive(&self) -> bool {
        let mut arrived = self.arrived.lock().unwrap_or_else(PoisonError::into_inner);
        *arrived += 1;
        if *arrived >= self.parties {
            self.all_here.notify_all();
            return true;
        }
        let (arrived, _) = self
            .all_here
            .wait_timeout_while(arrived, FAILSAFE, |count| *count < self.parties)
            .unwrap_or_else(PoisonError::into_inner);
        *arrived >= self.parties
    }
}

fn pool_of<J: Send + 'static, A: Default + Send + 'static>(
    workers: usize,
    queue: usize,
    cancel: &CancelToken,
    run: impl Fn(&J) -> A + Send + Sync + 'static,
    skipped: impl Fn(&J, Skipped) -> A + Send + Sync + 'static,
) -> WorkerPool<J, A> {
    WorkerPool::start(
        workers,
        queue,
        cancel.clone(),
        Arc::new(run),
        Arc::new(skipped),
        Arc::new(|_| A::default()),
    )
}

/// Polls `condition` every millisecond until it holds; fails with `what`
/// after `FAILSAFE`.
fn wait_until(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + FAILSAFE;
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        thread::sleep(Duration::from_millis(1));
    }
}

/// The next answer, failing clearly if none arrives within `FAILSAFE`.
fn next<J: Send + 'static, A: Send + 'static>(pool: &WorkerPool<J, A>) -> Option<A> {
    match pool.recv_within(Some(FAILSAFE)) {
        Popped::Item(answer) => Some(answer),
        Popped::Finished => None,
        Popped::Empty => panic!("no answer within the failsafe"),
    }
}

fn drain<J: Send + 'static, A: Send + 'static>(pool: &WorkerPool<J, A>) -> Vec<A> {
    let mut all = Vec::new();
    while let Some(answer) = next(pool) {
        all.push(answer);
    }
    all
}

/// Joins the workers after waiting (bounded) for all of them to exit.
fn join<J: Send + 'static, A: Send + 'static>(pool: &WorkerPool<J, A>) {
    wait_until("workers to exit", || pool.alive() == 0);
    pool.join();
}

/// N jobs finish only if N workers are inside the run function at once.
#[test]
fn workers_run_jobs_in_parallel_not_in_turn() {
    for workers in [2_usize, 8] {
        let rendezvous = Arc::new(Rendezvous::new(workers));
        let gate = Arc::clone(&rendezvous);
        let pool = pool_of(
            workers,
            workers,
            &CancelToken::new(),
            move |job: &usize| (*job, gate.arrive()),
            |job, _| (*job, false),
        );
        for job in 0..workers {
            assert!(pool.submit(job, Some(FAILSAFE)).is_ok());
        }
        pool.close_input();
        let mut answers = drain(&pool);
        join(&pool);
        answers.sort_unstable();
        let expected: Vec<(usize, bool)> = (0..workers).map(|job| (job, true)).collect();
        assert_eq!(answers, expected, "{workers} workers must overlap");
        assert_eq!(pool.alive(), 0);
    }
}

/// Memory is bounded: with a consumer that is not reading, the producer is
/// stopped at `capacity()` jobs in the pool, however many it has to send.
#[test]
fn a_slow_consumer_stops_the_producer_at_capacity() {
    const JOBS: usize = 500;
    let pool = Arc::new(pool_of(
        3,
        2,
        &CancelToken::new(),
        |job: &usize| *job,
        |job, _| *job,
    ));
    let capacity = pool.capacity();
    assert_eq!(capacity, 2 * 2 + 3);
    let peak = Arc::new(AtomicUsize::new(0));
    let (done_send, done_recv) = mpsc::channel();
    {
        let pool = Arc::clone(&pool);
        let peak = Arc::clone(&peak);
        drop(thread::spawn(move || {
            for job in 0..JOBS {
                if pool.submit(job, Some(FAILSAFE)).is_err() {
                    let _ = done_send.send(false);
                    return;
                }
                peak.fetch_max(pool.in_flight(), Ordering::AcqRel);
            }
            pool.close_input();
            let _ = done_send.send(true);
        }));
    }
    // Consumer reads nothing until the pool is saturated.
    wait_until("the pool to saturate", || pool.in_flight() >= capacity);
    assert_eq!(pool.in_flight(), capacity, "saturated at exactly capacity");
    // Now read everything, slowly: one answer at a time.
    let mut received = 0;
    while next(&pool).is_some() {
        received += 1;
        assert!(pool.in_flight() <= capacity);
    }
    assert!(
        done_recv
            .recv_timeout(FAILSAFE)
            .expect("the producer did not finish"),
        "the producer was refused"
    );
    join(&pool);
    assert_eq!(received, JOBS, "every job was answered");
    // Counts are exact (atomic with push and pop) and sampled submitted-first,
    // so a sample can read low but never above capacity.
    assert!(
        peak.load(Ordering::Acquire) <= capacity,
        "peak {} exceeded capacity {capacity}",
        peak.load(Ordering::Acquire)
    );
}

/// A full queue refuses a timed submit and hands the job back.
#[test]
fn a_timed_submit_to_a_full_queue_returns_the_job() {
    let hold = Arc::new(Rendezvous::new(2));
    let gate = Arc::clone(&hold);
    let pool = pool_of(
        1,
        1,
        &CancelToken::new(),
        move |job: &usize| {
            gate.arrive();
            *job
        },
        |job, _| *job,
    );
    assert!(pool.submit(0, None).is_ok());
    wait_until("a worker to start", || pool.running() > 0);
    // The worker holds job 0; one job fits in the queue; the next does not.
    assert!(pool.submit(1, None).is_ok());
    match pool.submit(2, Some(Duration::from_millis(1))) {
        Err(PushError::Full(job)) => assert_eq!(job, 2),
        _ => panic!("expected a full queue"),
    }
    assert!(hold.arrive());
    pool.close_input();
    let mut answers = drain(&pool);
    join(&pool);
    answers.sort_unstable();
    assert_eq!(answers, vec![0, 1]);
}

/// Cancel stops new work: the running job finishes, queued jobs are
/// answered as `Cancelled` without running, and later submits are refused.
#[test]
fn cancel_skips_queued_jobs_and_refuses_new_ones() {
    let cancel = CancelToken::new();
    let hold = Arc::new(Rendezvous::new(2));
    let gate = Arc::clone(&hold);
    let ran = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&ran);
    let pool = pool_of(
        1,
        8,
        &cancel,
        move |job: &usize| {
            counted.fetch_add(1, Ordering::AcqRel);
            gate.arrive();
            (*job, None)
        },
        |job, why| (*job, Some(why)),
    );
    for job in 0..4 {
        assert!(pool.submit(job, None).is_ok());
    }
    wait_until("a worker to start", || pool.running() > 0);
    pool.cancel();
    assert!(cancel.is_cancelled(), "the engine's token is the pool's");
    assert!(matches!(pool.submit(9, None), Err(PushError::Closed(9))));
    assert!(hold.arrive());
    let mut answers = drain(&pool);
    join(&pool);
    answers.sort_by_key(|(job, _)| *job);
    assert_eq!(
        answers,
        vec![
            (0, None),
            (1, Some(Skipped::Cancelled)),
            (2, Some(Skipped::Cancelled)),
            (3, Some(Skipped::Cancelled)),
        ]
    );
    assert_eq!(ran.load(Ordering::Acquire), 1, "only the running job ran");
}

/// A panicking job gets an answer and does not take its worker down.
#[test]
fn a_panicking_job_is_answered_and_the_worker_survives() {
    let pool = pool_of(
        1,
        4,
        &CancelToken::new(),
        |job: &usize| {
            assert_ne!(*job, 1, "boom");
            (*job, None)
        },
        |job, why| (*job, Some(why)),
    );
    for job in 0..3 {
        assert!(pool.submit(job, None).is_ok());
    }
    pool.close_input();
    let answers = drain(&pool);
    join(&pool);
    assert_eq!(
        answers,
        vec![(0, None), (1, Some(Skipped::Panicked)), (2, None)]
    );
}

/// Finished means input closed, queue empty, workers idle: until the input
/// is closed `recv_within` times out as `Empty`; after, it ends.
#[test]
fn finished_needs_closed_input_empty_queue_and_idle_workers() {
    let pool = pool_of(2, 2, &CancelToken::new(), |job: &usize| *job, |job, _| *job);
    assert!(matches!(
        pool.recv_within(Some(Duration::from_millis(1))),
        Popped::Empty
    ));
    assert!(pool.submit(7, None).is_ok());
    assert!(matches!(pool.recv_within(Some(FAILSAFE)), Popped::Item(7)));
    // Open input, empty queue, idle workers: still not finished.
    assert!(matches!(
        pool.recv_within(Some(Duration::from_millis(1))),
        Popped::Empty
    ));
    assert_eq!(pool.alive(), 2);
    pool.close_input();
    assert!(next(&pool).is_none());
    join(&pool);
    assert_eq!(pool.alive(), 0);
    assert_eq!(pool.running(), 0);
}

/// Dropping the receiving side must not leave a worker blocked forever.
#[test]
fn abandon_releases_workers_blocked_on_a_full_answer_queue() {
    let pool = pool_of(2, 1, &CancelToken::new(), |job: &usize| *job, |job, _| *job);
    // Saturate: queue 1 + answers 1 + 2 running; nothing is received.
    for job in 0..pool.capacity() {
        assert!(pool.submit(job, Some(FAILSAFE)).is_ok());
    }
    pool.abandon();
    join(&pool);
    assert_eq!(pool.alive(), 0);
}

/// A panic inside `skipped` (where logging will live) must not lose the
/// answer or hang the stream: the last-resort answer stands in, and the
/// worker survives to answer the next job.
#[test]
fn a_panicking_skipped_function_still_gives_one_answer_per_job() {
    let cancel = CancelToken::new();
    let pool: WorkerPool<usize, (usize, &'static str)> = WorkerPool::start(
        1,
        4,
        cancel.clone(),
        Arc::new(|job: &usize| {
            assert_ne!(*job, 1, "run panics");
            (*job, "ran")
        }),
        Arc::new(|_: &usize, _| panic!("skipped panics")),
        Arc::new(|job: &usize| (*job, "lost")),
    );
    for job in 0..3 {
        assert!(pool.submit(job, Some(FAILSAFE)).is_ok());
    }
    pool.close_input();
    let answers = drain(&pool);
    join(&pool);
    assert_eq!(answers, vec![(0, "ran"), (1, "lost"), (2, "ran")]);
    assert_eq!(pool.alive(), 0);
}

/// Even when the last-resort function panics too, the worker's exit still
/// closes the answer queue: the stream ends instead of hanging.
#[test]
fn a_dead_worker_still_lets_the_stream_finish() {
    let pool: WorkerPool<usize, usize> = WorkerPool::start(
        1,
        4,
        CancelToken::new(),
        Arc::new(|_: &usize| panic!("run panics")),
        Arc::new(|_: &usize, _| panic!("skipped panics")),
        Arc::new(|_: &usize| panic!("lost panics")),
    );
    assert!(pool.submit(0, Some(FAILSAFE)).is_ok());
    pool.close_input();
    assert!(next(&pool).is_none(), "no answer, but the stream ends");
    join(&pool);
    assert_eq!(pool.alive(), 0);
}
