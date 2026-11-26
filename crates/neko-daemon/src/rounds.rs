//! The rounds scheduler (docs/knowledge/rounds.md): one thread that sleeps until the next round is due, and the requests that change
//! rounds.
//!
//! - **Sleeping.** With no round that is on and automatic the thread parks with no timeout: no timer at all. Otherwise it parks until
//!   the next due time, but never longer than [`LONGEST_SLEEP`]: the Mac's monotonic clock stands still while it sleeps, so a timer
//!   set for the next due time would fire late by the length of the sleep. Each time it wakes it reads the wall clock and deals with
//!   every due time that has passed (the core runs a round that missed times once, if the latest is at most 24 hours old). A change
//!   to a round, a task of a round ending and a stop all unpark it at once.
//! - **Noticing a wake.** The wall clock and the monotonic clock are read together each time the thread goes to sleep and wakes. When
//!   the wall clock moved on by more than the monotonic one by [`JUMP`] the Mac slept (or the clock was set forward): it is noticed
//!   and logged, and the due times that passed are dealt with right then.
//! - **The test clock.** In a debug build started with `NEKO_DEBUG_CLOCK=1`, `debug_clock` moves the scheduler's clock (and no
//!   other) forward by an offset. A release build has no offset at all: the field is not compiled in, and the request is refused.

use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use neko_core::rounds::{Env, RoundError};
use neko_core::{Clock, SystemClock};
use neko_protocol::schedule::SystemZone;
use neko_protocol::{Change, ErrorCode, Reply, Schedule, ServerMessage};

use crate::runs;
use crate::server::{Shared, lock};

/// The longest the thread sleeps at a time while a round is on (see the module documentation).
pub const LONGEST_SLEEP: Duration = Duration::from_secs(5 * 60);
/// How much further the wall clock must move than the monotonic one to count as a jump (a sleep, or the clock set forward).
pub const JUMP: Duration = Duration::from_secs(30);

/// The scheduler's clock and handle on its thread.
pub struct Hub {
    thread: OnceLock<std::thread::Thread>,
    #[cfg(debug_assertions)]
    offset_ms: std::sync::atomic::AtomicI64,
    #[cfg(debug_assertions)]
    debug_clock: bool,
}

impl Hub {
    pub fn new() -> Hub {
        Hub {
            thread: OnceLock::new(),
            #[cfg(debug_assertions)]
            offset_ms: std::sync::atomic::AtomicI64::new(0),
            #[cfg(debug_assertions)]
            debug_clock: std::env::var("NEKO_DEBUG_CLOCK").is_ok_and(|v| v == "1"),
        }
    }

    /// The scheduler's now: the system clock (plus the test offset, in a debug build that allows one).
    pub fn now(&self) -> i64 {
        #[cfg(debug_assertions)]
        {
            SystemClock.now_ms() + self.offset_ms.load(Ordering::SeqCst)
        }
        #[cfg(not(debug_assertions))]
        {
            SystemClock.now_ms()
        }
    }

    /// Wakes the thread: something changed that it should look at now.
    pub fn poke(&self) {
        if let Some(t) = self.thread.get() {
            t.unpark();
        }
    }
}

impl Default for Hub {
    fn default() -> Self {
        Hub::new()
    }
}

/// Whether the wall clock moved on by [`JUMP`] more than the monotonic clock did between two readings.
pub fn jumped(wall_elapsed_ms: i64, monotonic_elapsed: Duration) -> bool {
    wall_elapsed_ms - monotonic_elapsed.as_millis() as i64 > JUMP.as_millis() as i64
}

/// Starts the scheduler thread.
pub fn spawn(shared: Arc<Shared>) -> std::io::Result<()> {
    let for_thread = shared.clone();
    let h = std::thread::Builder::new().name("neko-rounds".into()).spawn(move || watch(&for_thread))?;
    let _ = shared.rounds.thread.set(h.thread().clone());
    Ok(())
}

/// The daemon is stopping: the thread ends now.
pub fn stopping(shared: &Shared) {
    shared.rounds.poke();
}

pub(crate) fn env(shared: &Shared) -> Env<'static> {
    Env { now: shared.rounds.now(), zone: &SystemZone }
}

fn watch(shared: &Arc<Shared>) {
    // Tasks a round made whose agent never started (the daemon went down between the two) start now.
    let orphans = lock(&shared.core).store.round_tasks_never_started().unwrap_or_default();
    for t in orphans {
        let _ = runs::start(shared, t, None);
    }
    let mut last = (Instant::now(), SystemClock.now_ms());
    loop {
        if shared.stopping.load(Ordering::SeqCst) {
            return;
        }
        let (mono, wall) = (last.0.elapsed(), SystemClock.now_ms() - last.1);
        if jumped(wall, mono) {
            eprintln!("neko-daemon: the clock moved {} s more than time did (the Mac slept, or the clock was set): looking at the rounds now", (wall - mono.as_millis() as i64) / 1000);
        }
        let (started, wake_at) = {
            // The core stays locked while events leave, so they go out in revision order.
            let mut core = lock(&shared.core);
            let e = env(shared);
            let (started, rounds_wake) = match core.tick_rounds(e) {
                Ok(tick) => {
                    for (revision, change) in tick.events {
                        shared.broadcast(&ServerMessage::Event { revision, change });
                    }
                    (tick.started, tick.wake_at_ms)
                }
                Err(e) => {
                    eprintln!("neko-daemon: rounds: {e}");
                    (Vec::new(), None)
                }
            };
            // The morning notification (docs/knowledge/briefing.md): the same thread, the same clock, the same timer.
            let morning_wake = match core.morning(e, shared.screens()) {
                Ok(m) => {
                    if let Some(counts) = m.due {
                        let revision = core.store.revision().unwrap_or(0);
                        shared.broadcast(&ServerMessage::Event { revision, change: Change::BriefingDue { counts } });
                    }
                    m.wake_at_ms
                }
                Err(e) => {
                    eprintln!("neko-daemon: briefing: {e}");
                    None
                }
            };
            let wake = [rounds_wake, morning_wake].into_iter().flatten().min();
            (started, wake.map(|t| (t - e.now).max(0)))
        };
        for task in started {
            let _ = runs::start(shared, task, None);
        }
        last = (Instant::now(), SystemClock.now_ms());
        match wake_at {
            // Nothing on and automatic: no timer at all, until something changes.
            None => std::thread::park(),
            Some(ms) => std::thread::park_timeout(Duration::from_millis(ms as u64).min(LONGEST_SLEEP)),
        }
    }
}

/// A refused change to a round, as a reply.
fn error(e: RoundError) -> Reply {
    let code = match &e {
        RoundError::NotFound => ErrorCode::NotFound,
        RoundError::InvalidName(_) => ErrorCode::InvalidName,
        RoundError::InvalidWords(_) => ErrorCode::InvalidWords,
        RoundError::Refused(_) => ErrorCode::Refused,
        RoundError::Busy(_) => ErrorCode::Busy,
        RoundError::Store(_) => ErrorCode::Internal,
    };
    Reply::error(code, e.to_string())
}

pub fn list(shared: &Shared) -> Reply {
    let core = lock(&shared.core);
    match core.rounds(env(shared)) {
        Ok((rounds, paused)) => Reply::Rounds { rounds, paused },
        Err(e) => Reply::error(ErrorCode::Internal, e.to_string()),
    }
}

pub fn create(shared: &Shared, name: &str, words: &str, cat: &str, workspace: Option<&str>, schedule: &Schedule, on: bool) -> Reply {
    let mut core = lock(&shared.core);
    match core.create_round(name, words, cat, workspace, schedule, on, env(shared)) {
        Ok((round, revision)) => {
            shared.broadcast(&ServerMessage::Event { revision, change: Change::RoundAdded { round: round.clone() } });
            shared.rounds.poke();
            Reply::Round { round }
        }
        Err(e) => error(e),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update(shared: &Shared, round: &str, name: &str, words: &str, cat: &str, workspace: Option<&str>, schedule: &Schedule) -> Reply {
    let mut core = lock(&shared.core);
    match core.update_round(round, name, words, cat, workspace, schedule, env(shared)) {
        Ok((round, revision)) => {
            shared.broadcast(&ServerMessage::Event { revision, change: Change::RoundUpdated { round: round.clone() } });
            shared.rounds.poke();
            Reply::Round { round }
        }
        Err(e) => error(e),
    }
}

pub fn set_on(shared: &Shared, round: &str, on: bool) -> Reply {
    let mut core = lock(&shared.core);
    match core.set_round_on(round, on, env(shared)) {
        Ok((round, revision)) => {
            shared.broadcast(&ServerMessage::Event { revision, change: Change::RoundUpdated { round: round.clone() } });
            shared.rounds.poke();
            Reply::Round { round }
        }
        Err(e) => error(e),
    }
}

pub fn remove(shared: &Shared, round: &str) -> Reply {
    let mut core = lock(&shared.core);
    match core.remove_round(round, env(shared)) {
        Ok(revision) => {
            shared.broadcast(&ServerMessage::Event { revision, change: Change::RoundRemoved { round: round.to_owned() } });
            shared.rounds.poke();
            Reply::Ok
        }
        Err(e) => error(e),
    }
}

/// Run now: a task is made and its agent starts, as for any task.
pub fn run_now(shared: &Arc<Shared>, round: &str) -> Reply {
    let made = {
        let mut core = lock(&shared.core);
        let made = core.run_round_now(round, env(shared));
        if let Ok((_, _, events)) = &made {
            for (revision, change) in events {
                shared.broadcast(&ServerMessage::Event { revision: *revision, change: change.clone() });
            }
        }
        made
    };
    match made {
        Ok((_, task, _)) => {
            let _ = runs::start(shared, task.clone(), None);
            shared.rounds.poke();
            Reply::Task { task }
        }
        Err(e) => error(e),
    }
}

pub fn set_paused(shared: &Shared, paused: bool) -> Reply {
    let mut core = lock(&shared.core);
    match core.set_rounds_paused(paused, env(shared)) {
        Ok((events, _)) => {
            for (revision, change) in events {
                shared.broadcast(&ServerMessage::Event { revision, change });
            }
            shared.rounds.poke();
            Reply::Ok
        }
        Err(e) => error(e),
    }
}

pub fn history(shared: &Shared, round: &str) -> Reply {
    match lock(&shared.core).round_history(round) {
        Ok(runs) => Reply::RoundRuns { runs },
        Err(e) => error(e),
    }
}

/// Moves the scheduler's clock, for the QA rig. Only a debug build started with `NEKO_DEBUG_CLOCK=1` does it; any other refuses.
pub fn debug_clock(shared: &Shared, offset_ms: Option<i64>) -> Reply {
    #[cfg(debug_assertions)]
    {
        if !shared.rounds.debug_clock {
            return Reply::error(ErrorCode::Refused, "This daemon was not started with a debug clock.");
        }
        shared.rounds.offset_ms.store(offset_ms.unwrap_or(0), Ordering::SeqCst);
        shared.rounds.poke();
        Reply::Ok
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = (shared, offset_ms);
        Reply::error(ErrorCode::Refused, "This daemon has no debug clock.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_jump_is_the_wall_clock_running_ahead_of_time_by_half_a_minute() {
        assert!(!jumped(300_000, Duration::from_secs(300)), "a normal sleep");
        assert!(!jumped(300_000 + 29_000, Duration::from_secs(300)));
        assert!(jumped(300_000 + 31_000, Duration::from_secs(300)), "a nap");
        assert!(jumped(8 * 3_600_000, Duration::from_secs(2)), "the Mac slept for hours");
        assert!(!jumped(-5_000, Duration::from_secs(1)), "a clock set back is not a jump forward");
    }
}
