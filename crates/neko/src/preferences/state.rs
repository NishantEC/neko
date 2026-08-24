//! The Preferences window's pure state: which tab is showing, and what a
//! recorded key press means. No GPUI, no I/O — see this module's parent doc
//! comment for why that split exists.

use neko_protocol::{HotkeyCombo, Modifier};

/// The window's tabs, in the order they render.
///
/// Deliberately short. Raycast's own settings window has eight, but seven of
/// those exist because it has the features behind them; a tab bar padded out
/// with tabs that say "nothing here yet" is worse than a small one. **The AI
/// tab is not here for exactly that reason** — see `AGENTS.md` for the shape
/// AI would take when it is real.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    General,
    Search,
    Agents,
    About,
}

impl Tab {
    pub const ALL: &'static [Tab] = &[Tab::General, Tab::Search, Tab::Agents, Tab::About];

    /// The next tab along, wrapping — Left/Right on a focused tab, the
    /// ARIA tabs pattern. Wrapping rather than stopping because four tabs
    /// in a row read as a ring, and stopping at the end just feels broken.
    pub fn step(self, forward: bool) -> Tab {
        let all = Tab::ALL;
        let i = all.iter().position(|t| *t == self).unwrap_or(0);
        let n = all.len();
        all[if forward { (i + 1) % n } else { (i + n - 1) % n }]
    }

    pub fn title(self) -> &'static str {
        match self {
            Tab::General => "General",
            Tab::Search => "Search",
            Tab::Agents => "Agents",
            Tab::About => "About",
        }
    }
}

/// Whether the Summon Hotkey control is listening, and what happened to the
/// last thing it heard.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Recording {
    /// Showing the combination in force; not listening.
    #[default]
    Idle,
    /// Listening. Every key press is a candidate.
    Listening,
    /// The last press could not be used, with the reason to show.
    Rejected(String),
}

/// Turns a raw key press into a candidate combination.
///
/// Returns `None` for a press that should be ignored rather than rejected —
/// a bare modifier (there is no key yet, the person is still pressing) and
/// Escape (which cancels recording, and is separately refused as a hotkey by
/// [`validate_candidate`] anyway). Everything else becomes a candidate, valid
/// or not, so that an invalid one can be *explained* rather than swallowed.
pub fn candidate_from_press(
    key: &str,
    cmd: bool,
    alt: bool,
    ctrl: bool,
    shift: bool,
) -> Option<Result<HotkeyCombo, String>> {
    if key == "escape" {
        return None;
    }
    if is_bare_modifier(key) {
        return None;
    }
    let mut modifiers = Vec::new();
    if cmd {
        modifiers.push(Modifier::Cmd);
    }
    if alt {
        modifiers.push(Modifier::Alt);
    }
    if ctrl {
        modifiers.push(Modifier::Ctrl);
    }
    if shift {
        modifiers.push(Modifier::Shift);
    }
    let canonical = crate::onboarding::state::canonicalize_key_name(key);
    Some(crate::onboarding::state::validate_candidate(&modifiers, &canonical))
}

/// A press of a modifier key on its own. gpui reports these as ordinary key
/// presses, and treating one as a candidate would reject "⌘" as "no key"
/// every time the person merely started holding it down.
fn is_bare_modifier(key: &str) -> bool {
    matches!(key, "cmd" | "command" | "alt" | "option" | "ctrl" | "control" | "shift" | "fn" | "function")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_modifier_press_is_ignored_rather_than_rejected() {
        for key in ["cmd", "alt", "ctrl", "shift", "fn"] {
            assert!(
                candidate_from_press(key, true, false, false, false).is_none(),
                "{key} alone must not be reported as an invalid combination"
            );
        }
    }

    #[test]
    fn escape_is_ignored_here_because_it_cancels_recording_instead() {
        assert!(candidate_from_press("escape", false, false, false, false).is_none());
    }

    #[test]
    fn a_real_combination_comes_back_canonicalised() {
        let combo = candidate_from_press("space", true, false, false, true)
            .expect("a real key press is a candidate")
            .expect("cmd-shift-space is a valid hotkey");
        assert_eq!(combo, HotkeyCombo::new(vec![Modifier::Cmd, Modifier::Shift], "Space"));
    }

    #[test]
    fn a_key_with_no_modifier_is_rejected_with_a_reason_not_silently_dropped() {
        let reason = candidate_from_press("space", false, false, false, false)
            .expect("still a candidate — it must be explained, not ignored")
            .expect_err("a bare key is not a usable global hotkey");
        assert!(reason.contains("modifier"), "got: {reason}");
    }

    #[test]
    fn every_tab_has_a_title_and_the_order_is_stable() {
        assert_eq!(
            Tab::ALL.iter().map(|t| t.title()).collect::<Vec<_>>(),
            vec!["General", "Search", "Agents", "About"]
        );
    }

    #[test]
    fn arrow_keys_walk_the_tab_bar_as_a_ring() {
        // Four tabs in a row read as a ring; stopping at the end feels
        // broken rather than protective.
        assert_eq!(Tab::General.step(true), Tab::Search);
        assert_eq!(Tab::About.step(true), Tab::General);
        assert_eq!(Tab::General.step(false), Tab::About);
        // Every tab is reachable from every other, in both directions.
        for start in Tab::ALL {
            let mut seen = vec![*start];
            let mut at = *start;
            for _ in 1..Tab::ALL.len() {
                at = at.step(true);
                seen.push(at);
            }
            assert_eq!(seen.len(), Tab::ALL.len());
            for tab in Tab::ALL {
                assert!(seen.contains(tab), "{tab:?} unreachable from {start:?}");
            }
        }
    }

}
