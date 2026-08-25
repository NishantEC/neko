//! The attention chime — a sound when an agent blocks.
//!
//! The permission inbox's whole premise is "something needs me", and every
//! surface it had was visual: the menu bar count, the Dock badge before it,
//! the rows when the panel opens. All of them require looking. A person deep
//! in another window learns nothing until they glance up, which on a quiet
//! afternoon can be twenty minutes after an agent stopped.
//!
//! **The approach is comet's `sound.rs` (MIT, `refs/comet`): the platform's
//! own audio CLI, zero Rust audio deps, failures swallowed.** Where comet
//! synthesizes and embeds its own WAVs, this plays a **system sound** —
//! macOS has shipped the same fourteen `.aiff` files in `/System/Library/
//! Sounds` since the beginning, they are what every app's alert sound picks
//! from, and embedding a custom chime would buy distinctiveness this app
//! does not want: the point is to sound like *a notification*, not like neko.
//!
//! **When it plays is decided by the caller, not here** — `main.rs` chimes
//! only when the waiting count *rises*. A fall is an agent handled elsewhere
//! (good news needs no interruption), and equal is the daemon re-asserting
//! what was already known.
//!
//! `NEKO_DISABLE_SOUND` (any value) is the kill switch, checked per call so
//! it can be flipped without a restart for a long recording session.

use std::sync::atomic::{AtomicBool, Ordering};

/// Glass, of the fourteen: quiet, pitched high enough to carry over speech,
/// and not already claimed by a system default on this machine (Sosumi and
/// Funk both are, and hearing the OS's own alert from neko would misattribute
/// every real alert afterwards).
pub const CHIME_PATH: &str = "/System/Library/Sounds/Glass.aiff";

/// Logged once, not per miss — a missing system sound means an OS this list
/// has never been checked against, and one line says that as well as fifty.
static MISSING_LOGGED: AtomicBool = AtomicBool::new(false);

/// True when the chime should not play — the kill switch, or an evidence run
/// (a throwaway client must not make the captain's machine chime, the same
/// rule that keeps evidence runs off the hotkey and out of the menu bar).
fn suppressed() -> bool {
    std::env::var_os("NEKO_DISABLE_SOUND").is_some() || crate::evidence::evidence_run_active()
}

/// Plays the attention chime, off the calling thread, swallowing failure.
///
/// A background thread per chime rather than a held player: `afplay` is a
/// subprocess either way, chimes are rare (one per newly-blocked agent), and
/// a thread that exists for the duration of one sound is simpler than any
/// pool. `status()` rather than `spawn()`-and-forget so the child is reaped —
/// a zombie per chime would be a slow leak with a soundtrack.
pub fn play_attention_chime() {
    if suppressed() {
        return;
    }
    std::thread::spawn(|| {
        if !std::path::Path::new(CHIME_PATH).exists() {
            if !MISSING_LOGGED.swap(true, Ordering::Relaxed) {
                eprintln!("neko: attention chime missing at {CHIME_PATH}; staying silent");
            }
            return;
        }
        let _ = std::process::Command::new("/usr/bin/afplay").arg(CHIME_PATH).status();
    });
}

/// Whether a count transition deserves a chime.
///
/// Pure and separate from the player so the rule is pinned without playing
/// anything: only a **rise**. A fall is an agent handled elsewhere, and equal
/// is the daemon re-asserting what was already known. The first event after
/// startup compares against zero, so a client launched while agents are
/// already waiting does chime once — that state is genuinely news to this
/// session, exactly as it is to the menu bar count beside it.
pub fn should_chime(previous: usize, current: usize) -> bool {
    current > previous
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_rise_chimes() {
        assert!(should_chime(0, 1), "a new blocked agent is news");
        assert!(should_chime(2, 5));
        assert!(!should_chime(1, 1), "the daemon re-asserting is not");
        assert!(!should_chime(3, 0), "and good news needs no interruption");
    }

    #[test]
    fn the_chime_file_exists_on_this_machine() {
        // Environment-dependent on purpose, like
        // `scanning_the_real_machine_excludes_a_verified_background_agent`:
        // this repo only builds on macOS, and a path that stopped existing
        // should fail a test rather than silently muting the feature forever.
        assert!(std::path::Path::new(CHIME_PATH).exists(), "{CHIME_PATH} is gone");
    }
}
