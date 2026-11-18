//! The loop both EventKit bridges run (`CalendarBridge.swift` and `PhoneBridge.swift` have the same one): a cycle now, then after the wait it
//! asks for; a burst of EventKit change notifications is sent once, [`SETTLE`] after the last one; and while the Swift app holds the duty
//! nothing runs and nothing stays scheduled. The decisions are in [`Runner::step`], which is given the time and returns how long to wait, so
//! they are tested with a fake clock; [`drive`] is the thread that supplies the real one.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

/// How long after the last change notification the bridge runs once more (`settle`).
pub const SETTLE: Duration = Duration::from_secs(2);
/// How often a loop looks at whether the duty is Rune's (the longest it ever sleeps).
pub const RECHECK: Duration = Duration::from_secs(5);

/// What a bridge does at its two moments.
pub trait Job {
    /// A full cycle; returns how long until the next one.
    fn cycle(&mut self, now_ms: i64) -> Duration;
    /// The burst of changes has settled.
    fn settled(&mut self, now_ms: i64);
}

/// When things are due, in ms of the caller's clock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timers {
    pub cycle_at: Option<i64>,
    pub settle_at: Option<i64>,
}

pub struct Runner<J> {
    pub job: J,
    pub timers: Timers,
    handed_back: bool,
}

impl<J: Job> Runner<J> {
    pub fn new(job: J) -> Runner<J> {
        Runner { job, timers: Timers::default(), handed_back: false }
    }

    /// One turn. `active`: Rune holds this duty now; `changed`: EventKit said something changed since the last turn. What is due is done;
    /// the answer is how long to wait before the next turn (at most [`RECHECK`]).
    pub fn step(&mut self, now_ms: i64, active: bool, changed: bool) -> Duration {
        if !active {
            // The Swift app has the duty: nothing is asked, nothing is sent, nothing is kept scheduled.
            self.timers = Timers::default();
            self.handed_back = true;
            return RECHECK;
        }
        if self.handed_back || (self.timers.cycle_at.is_none() && self.timers.settle_at.is_none()) {
            self.handed_back = false;
            self.timers.cycle_at = Some(now_ms);
        }
        if changed {
            // EventKit posts a burst of changes while an account syncs: act once it settles.
            self.timers.settle_at = Some(now_ms + SETTLE.as_millis() as i64);
        }
        if self.timers.settle_at.is_some_and(|at| at <= now_ms) {
            self.timers.settle_at = None;
            self.job.settled(now_ms);
        }
        if self.timers.cycle_at.is_some_and(|at| at <= now_ms) {
            let wait = self.job.cycle(now_ms);
            self.timers.cycle_at = Some(now_ms + wait.as_millis() as i64);
        }
        let next = [self.timers.cycle_at, self.timers.settle_at].into_iter().flatten().min().unwrap_or(now_ms + RECHECK.as_millis() as i64);
        Duration::from_millis((next - now_ms).clamp(1, RECHECK.as_millis() as i64) as u64)
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Runs `job` for the life of the process: each turn waits for the next change notification on `changes` or for the time [`Runner::step`] named.
pub fn drive<J: Job>(job: J, active: &AtomicBool, changes: &Receiver<()>) {
    let mut runner = Runner::new(job);
    let mut changed = false;
    loop {
        let wait = runner.step(now_ms(), active.load(Ordering::SeqCst), changed);
        changed = match changes.recv_timeout(wait) {
            Ok(()) => true,
            Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => {
                std::thread::sleep(wait);
                false
            }
        };
    }
}
