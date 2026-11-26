//! Time and ids, handed in from outside so logic stays deterministic.

use std::io::Read;
use std::sync::atomic::{AtomicI64, Ordering};

/// Where the time comes from.
pub trait Clock: Send {
    /// Milliseconds since the Unix epoch.
    fn now_ms(&self) -> i64;
}

/// The system's clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
    }
}

/// A clock that says what it is told: each reading moves it on by `step` milliseconds.
pub struct FixedClock {
    now: AtomicI64,
    step: i64,
}

impl FixedClock {
    pub fn new(start_ms: i64, step: i64) -> FixedClock {
        FixedClock { now: AtomicI64::new(start_ms), step }
    }
}

impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        self.now.fetch_add(self.step, Ordering::Relaxed)
    }
}

/// Where new ids come from.
pub trait Ids: Send {
    /// A new id, unique in the store: 26 letters and digits that sort by time.
    fn next_id(&mut self, now_ms: i64) -> String;
}

/// Crockford's base 32: no I, L, O or U.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// 48 bits of time then 80 bits of `random`, as 26 base-32 characters (the ULID layout).
pub fn encode(now_ms: i64, random: [u8; 10]) -> String {
    let mut value: u128 = (now_ms.max(0) as u128 & ((1 << 48) - 1)) << 80;
    for (i, b) in random.iter().enumerate() {
        value |= u128::from(*b) << (8 * (9 - i));
    }
    (0..26).rev().map(|i| ALPHABET[((value >> (5 * i)) & 31) as usize] as char).collect()
}

/// Ids with 80 random bits from the system's random source.
pub struct RandomIds;

impl Ids for RandomIds {
    fn next_id(&mut self, now_ms: i64) -> String {
        let mut random = [0u8; 10];
        if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut random)).is_err() {
            // No random source: the time's nanoseconds and the process id still make a clash all but impossible.
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
            random[..8].copy_from_slice(&(nanos as u64 ^ u64::from(std::process::id())).to_be_bytes());
        }
        encode(now_ms, random)
    }
}

/// Ids counted up from one, for tests.
#[derive(Default)]
pub struct SequentialIds(pub u64);

impl Ids for SequentialIds {
    fn next_id(&mut self, now_ms: i64) -> String {
        self.0 += 1;
        let mut random = [0u8; 10];
        random[2..].copy_from_slice(&self.0.to_be_bytes());
        encode(now_ms, random)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_26_characters_and_sort_by_time() {
        let a = encode(1_764_000_000_000, [0; 10]);
        let b = encode(1_764_000_000_001, [0; 10]);
        assert_eq!(a.len(), 26);
        assert!(a < b);
        assert_eq!(encode(0, [0; 10]), "0".repeat(26));
        assert_eq!(encode(0, [255; 10]), format!("{}{}", "0".repeat(10), "Z".repeat(16)));
    }

    #[test]
    fn sequential_ids_are_distinct_and_random_ids_differ() {
        let mut s = SequentialIds::default();
        assert_ne!(s.next_id(5), s.next_id(5));
        assert_ne!(RandomIds.next_id(5), RandomIds.next_id(5));
    }

    #[test]
    fn the_fixed_clock_steps() {
        let c = FixedClock::new(100, 10);
        assert_eq!((c.now_ms(), c.now_ms()), (100, 110));
    }
}
