//! The hotkey *setting*: persistence and the fast, side-effect-free
//! conflict check. Live OS-level registration is a client-only concern (see
//! `neko`'s `hotkey_client` module) because that's the process with the run
//! loop `global-hotkey` needs — this module never touches AppKit/Carbon.

use neko_protocol::{HotkeyCombo, HotkeyConfig, Modifier};

const SETTING_KEY: &str = "hotkey_combo";

pub fn get_hotkey(db: &crate::Db) -> rusqlite::Result<HotkeyConfig> {
    match db.get_setting(SETTING_KEY)? {
        Some(json) => match serde_json::from_str::<HotkeyConfig>(&json) {
            Ok(config) => Ok(config),
            // Corrupt/foreign-format setting shouldn't brick the app —
            // fall back to the documented default and let a future
            // CommitHotkey overwrite it.
            Err(_) => Ok(default_config()),
        },
        None => Ok(default_config()),
    }
}

pub fn set_hotkey(db: &crate::Db, combo: HotkeyCombo, now_unix_ms: i64) -> rusqlite::Result<HotkeyConfig> {
    let config = HotkeyConfig {
        combo,
        updated_at_unix_ms: now_unix_ms,
    };
    let json = serde_json::to_string(&config).expect("HotkeyConfig always serializes");
    db.set_setting(SETTING_KEY, &json)?;
    Ok(config)
}

fn default_config() -> HotkeyConfig {
    HotkeyConfig {
        combo: HotkeyCombo::default_summon(),
        updated_at_unix_ms: 0,
    }
}

/// Combinations macOS itself or the launcher's obvious competitors are
/// known to reserve by default, checked before ever attempting a live OS
/// registration. This is a heuristic allowlist-of-badness, not a guarantee
/// — it catches the well-known collisions (Spotlight, Mission Control,
/// screenshot tools, input-source switching) that would otherwise silently
/// fail or double-fire. It cannot see a *third-party* app's custom binding
/// (e.g. a Raycast user who rebound it away from its own default); only a
/// live registration attempt, which only the client can make, can prove
/// that kind of conflict.
fn known_reserved() -> Vec<(HotkeyCombo, &'static str)> {
    use Modifier::*;
    vec![
        (
            HotkeyCombo::new(vec![Cmd], "Space"),
            "Spotlight (default binding)",
        ),
        (
            HotkeyCombo::new(vec![Alt], "Space"),
            "Spotlight's alternate binding on some Mac keyboard layouts, and Raycast's own out-of-the-box default",
        ),
        (HotkeyCombo::new(vec![Cmd], "Tab"), "App Switcher"),
        (
            HotkeyCombo::new(vec![Ctrl], "Space"),
            "Input Source switching",
        ),
        (
            HotkeyCombo::new(vec![Ctrl], "Up"),
            "Mission Control",
        ),
        (
            HotkeyCombo::new(vec![Cmd, Shift], "3"),
            "Screenshot: whole screen",
        ),
        (
            HotkeyCombo::new(vec![Cmd, Shift], "4"),
            "Screenshot: selection",
        ),
        (
            HotkeyCombo::new(vec![Cmd, Shift], "5"),
            "Screenshot/screen-recording panel",
        ),
    ]
}

fn same_combo(a: &HotkeyCombo, b: &HotkeyCombo) -> bool {
    let mut a_mods = a.modifiers.clone();
    let mut b_mods = b.modifiers.clone();
    a_mods.sort_by_key(|m| *m as u8);
    b_mods.sort_by_key(|m| *m as u8);
    a_mods == b_mods && a.key.eq_ignore_ascii_case(&b.key)
}

/// `Some(reason)` if the candidate matches a known-reserved combo.
pub fn check_known_conflict(candidate: &HotkeyCombo) -> Option<String> {
    known_reserved()
        .into_iter()
        .find(|(combo, _)| same_combo(combo, candidate))
        .map(|(_, reason)| reason.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;

    #[test]
    fn default_hotkey_is_option_space() {
        let db = Db::open_in_memory().unwrap();
        let config = get_hotkey(&db).unwrap();
        assert_eq!(config.combo, HotkeyCombo::default_summon());
    }

    #[test]
    fn set_then_get_round_trips_and_bumps_the_timestamp() {
        let db = Db::open_in_memory().unwrap();
        let candidate = HotkeyCombo::new(vec![Modifier::Cmd, Modifier::Shift], "Space");
        let written = set_hotkey(&db, candidate.clone(), 12345).unwrap();
        assert_eq!(written.combo, candidate);
        assert_eq!(written.updated_at_unix_ms, 12345);
        let read_back = get_hotkey(&db).unwrap();
        assert_eq!(read_back, written);
    }

    #[test]
    fn cmd_space_conflicts_with_spotlight() {
        let reason = check_known_conflict(&HotkeyCombo::new(vec![Modifier::Cmd], "Space"));
        assert!(reason.is_some());
    }

    #[test]
    fn conflict_check_is_modifier_order_independent() {
        let a = HotkeyCombo::new(vec![Modifier::Shift, Modifier::Cmd], "3");
        assert!(check_known_conflict(&a).is_some());
    }

    #[test]
    fn an_unreserved_combo_has_no_known_conflict() {
        assert!(check_known_conflict(&HotkeyCombo::new(vec![Modifier::Ctrl, Modifier::Alt], "K")).is_none());
    }
}
