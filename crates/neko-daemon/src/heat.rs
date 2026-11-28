//! Whether this Mac is too hot or too short on memory for Neko to start more by itself (docs/knowledge/subagents.md, "Limits and heat").
//!
//! Three levels, the same words the person's own tooling uses: `Ok`, `Warm` (do not add more parallel work: a kitten that has not
//! started waits its turn) and `Wait` (nothing Neko starts by itself goes ahead: kittens and wakes both wait). Only what Neko starts
//! by itself is held back; a run the person starts is never.
//!
//! What it reads, on macOS: the kernel's memory pressure (`kern.memorystatus_vm_pressure_level`: 1 normal, 2 warning, 4 critical) and
//! the CPU speed limit macOS has put on a hot machine (`pmset -g therm`, `CPU_Speed_Limit`, 100 when none). Elsewhere it says `Ok`.
//! It reads at most every 15 seconds, and only when something waits for it.
//!
//! A debug daemon (the tests and the QA rig) may be told the level instead: `NEKO_HEAT` (`ok`, `warm`, `wait`), or `NEKO_HEAT_FILE`,
//! a file holding the word, read each time so a test can change it. A release daemon ignores both.

use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How hot the machine is, for Neko's purposes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heat {
    Ok,
    Warm,
    Wait,
}

/// How long a reading is kept.
const KEEP: Duration = Duration::from_secs(15);

static LAST: Mutex<Option<(Instant, Heat)>> = Mutex::new(None);

fn word(s: &str) -> Option<Heat> {
    match s.trim().to_lowercase().as_str() {
        "ok" => Some(Heat::Ok),
        "warm" => Some(Heat::Warm),
        "wait" => Some(Heat::Wait),
        _ => None,
    }
}

/// The level now.
pub fn level() -> Heat {
    if cfg!(debug_assertions) {
        if let Some(h) = std::env::var("NEKO_HEAT_FILE").ok().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|s| word(&s)) {
            return h;
        }
        if let Some(h) = std::env::var("NEKO_HEAT").ok().and_then(|s| word(&s)) {
            return h;
        }
    }
    let mut last = LAST.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((at, h)) = *last
        && at.elapsed() < KEEP
    {
        return h;
    }
    let h = sample();
    *last = Some((Instant::now(), h));
    h
}

/// How long to wait before looking again while something is held back (tests shorten it).
pub fn recheck() -> Duration {
    Duration::from_millis(std::env::var("NEKO_HEAT_RECHECK_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(30_000))
}

fn read(program: &str, args: &[&str]) -> Option<String> {
    let mut c = Command::new(program);
    c.args(args).stdin(Stdio::null()).stderr(Stdio::null());
    let out = neko_core::find::run_for(c, Duration::from_secs(3))?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(target_os = "macos")]
fn sample() -> Heat {
    let pressure = read("/usr/sbin/sysctl", &["-n", "kern.memorystatus_vm_pressure_level"]).and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(1);
    let limit = read("/usr/bin/pmset", &["-g", "therm"]).and_then(|s| speed_limit(&s)).unwrap_or(100);
    classify(pressure, limit)
}

#[cfg(not(target_os = "macos"))]
fn sample() -> Heat {
    let _ = read;
    Heat::Ok
}

/// The CPU speed limit `pmset -g therm` reports, as a percentage.
fn speed_limit(text: &str) -> Option<u32> {
    text.lines().find_map(|l| l.trim().strip_prefix("CPU_Speed_Limit").and_then(|r| r.trim().trim_start_matches('=').trim().parse().ok()))
}

fn classify(pressure: u32, limit: u32) -> Heat {
    if pressure >= 4 || limit < 50 {
        Heat::Wait
    } else if pressure >= 2 || limit < 100 {
        Heat::Warm
    } else {
        Heat::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_and_a_speed_limit_make_it_warm_then_wait() {
        assert_eq!(classify(1, 100), Heat::Ok);
        assert_eq!(classify(2, 100), Heat::Warm);
        assert_eq!(classify(1, 80), Heat::Warm);
        assert_eq!(classify(4, 100), Heat::Wait);
        assert_eq!(classify(1, 40), Heat::Wait);
    }

    #[test]
    fn the_speed_limit_is_read_from_pmset() {
        assert_eq!(speed_limit("Note: No thermal warning level has been recorded\nCPU_Scheduler_Limit\t= 100\nCPU_Available_CPUs\t= 14\nCPU_Speed_Limit \t= 85\n"), Some(85));
        assert_eq!(speed_limit("Note: No performance warning level has been recorded\n"), None);
    }

    #[test]
    fn words_are_levels() {
        assert_eq!(word(" Warm\n"), Some(Heat::Warm));
        assert_eq!(word("nope"), None);
    }
}
