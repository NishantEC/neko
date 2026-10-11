//! Pacing for new agents. There is deliberately no fixed limit on how many
//! tickets work at once, but every agent is a full CLI session with its own
//! build and test processes, and many starting together has overloaded this
//! machine before (a load average above 240 restarted it). So new agents are
//! released from a small bucket that refills slowly, and none start while the
//! machine is already busy. Agents that are running are never affected.
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Agents that may start together after a quiet period.
pub(super) const BURST: f64 = 6.0;
/// After the burst, one more agent may start this often.
pub(super) const REFILL_EVERY: Duration = Duration::from_secs(10);
/// No new agent starts while the one-minute load average is at or above this.
pub(super) const LOAD_LIMIT: f64 = 20.0;

struct Bucket {
    tokens: f64,
    updated: Instant,
}

fn bucket() -> &'static Mutex<Option<Bucket>> {
    static BUCKET: Mutex<Option<Bucket>> = Mutex::new(None);
    &BUCKET
}

/// How many of `waiting` eligible tickets may start a worker now.
pub(super) fn admit(waiting: usize, load: Option<f64>, now: Instant) -> usize {
    let mut guard = bucket().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    admit_from(&mut guard, waiting, load, now)
}

fn admit_from(slot: &mut Option<Bucket>, waiting: usize, load: Option<f64>, now: Instant) -> usize {
    let bucket = slot.get_or_insert(Bucket { tokens: BURST, updated: now });
    let refill = now.saturating_duration_since(bucket.updated).as_secs_f64() / REFILL_EVERY.as_secs_f64();
    bucket.tokens = (bucket.tokens + refill).min(BURST);
    bucket.updated = now;
    if load.is_some_and(|load| load >= LOAD_LIMIT) {
        return 0;
    }
    let admitted = (bucket.tokens.floor() as usize).min(waiting);
    bucket.tokens -= admitted as f64;
    admitted
}

/// The one-minute load average, or `None` if the system will not say.
pub(super) fn load_average() -> Option<f64> {
    unsafe extern "C" {
        fn getloadavg(loadavg: *mut f64, nelem: i32) -> i32;
    }
    let mut loads = [0.0_f64; 1];
    // SAFETY: getloadavg writes at most `nelem` doubles into the buffer.
    (unsafe { getloadavg(loads.as_mut_ptr(), 1) } == 1).then_some(loads[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bursts_then_paces_and_holds_back_under_load() {
        let start = Instant::now();
        let mut slot = None;
        assert_eq!(admit_from(&mut slot, 27, Some(3.0), start), 6, "a burst starts together");
        assert_eq!(admit_from(&mut slot, 21, Some(3.0), start + Duration::from_secs(2)), 0);
        assert_eq!(admit_from(&mut slot, 21, Some(3.0), start + Duration::from_secs(10)), 1, "then one every ten seconds");
        assert_eq!(admit_from(&mut slot, 20, Some(25.0), start + Duration::from_secs(60)), 0, "nothing new while overloaded");
        assert_eq!(admit_from(&mut slot, 20, Some(4.0), start + Duration::from_secs(60)), 5, "credit earned while waiting is kept");
        assert_eq!(admit_from(&mut slot, 2, None, start + Duration::from_secs(600)), 2, "an unknown load does not block work");
    }

    #[test]
    fn reads_a_real_load_average() {
        assert!(load_average().is_some_and(|load| load >= 0.0));
    }
}
