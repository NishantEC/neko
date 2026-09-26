//! Onboarding-lifecycle settings: whether the first-run arc has been
//! completed, whether the post-onboarding accessibility banner (design
//! report §3, step 08) has been dismissed, and the clipboard-history
//! permission toggle (steps 06-07) neko asks for itself since macOS has no
//! TCC prompt for clipboard reads. All three are daemon-owned KV settings,
//! the same shape as `neko_core::hotkey`'s persisted combo.

const ONBOARDING_COMPLETED_KEY: &str = "onboarding_completed";
const ACCESSIBILITY_BANNER_DISMISSED_KEY: &str = "accessibility_banner_dismissed";
const CLIPBOARD_HISTORY_ENABLED_KEY: &str = "clipboard_history_enabled";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OnboardingState {
    pub completed: bool,
    pub accessibility_banner_dismissed: bool,
}

fn get_bool(db: &crate::Db, key: &str) -> rusqlite::Result<bool> {
    Ok(db.get_setting(key)?.as_deref() == Some("true"))
}

fn set_bool(db: &crate::Db, key: &str, value: bool) -> rusqlite::Result<()> {
    db.set_setting(key, if value { "true" } else { "false" })
}

pub fn get_onboarding_state(db: &crate::Db) -> rusqlite::Result<OnboardingState> {
    Ok(OnboardingState {
        completed: get_bool(db, ONBOARDING_COMPLETED_KEY)?,
        accessibility_banner_dismissed: get_bool(db, ACCESSIBILITY_BANNER_DISMISSED_KEY)?,
    })
}

pub fn set_onboarding_completed(
    db: &crate::Db,
    completed: bool,
) -> rusqlite::Result<OnboardingState> {
    set_bool(db, ONBOARDING_COMPLETED_KEY, completed)?;
    get_onboarding_state(db)
}

pub fn dismiss_accessibility_banner(db: &crate::Db) -> rusqlite::Result<OnboardingState> {
    set_bool(db, ACCESSIBILITY_BANNER_DISMISSED_KEY, true)?;
    get_onboarding_state(db)
}

pub fn get_clipboard_history_enabled(db: &crate::Db) -> rusqlite::Result<bool> {
    get_bool(db, CLIPBOARD_HISTORY_ENABLED_KEY)
}

pub fn set_clipboard_history_enabled(db: &crate::Db, enabled: bool) -> rusqlite::Result<bool> {
    set_bool(db, CLIPBOARD_HISTORY_ENABLED_KEY, enabled)?;
    get_clipboard_history_enabled(db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;

    #[test]
    fn onboarding_defaults_to_not_completed_and_banner_not_dismissed() {
        let db = Db::open_in_memory().unwrap();
        let state = get_onboarding_state(&db).unwrap();
        assert_eq!(state, OnboardingState::default());
    }

    #[test]
    fn completing_onboarding_persists_and_round_trips() {
        let db = Db::open_in_memory().unwrap();
        let state = set_onboarding_completed(&db, true).unwrap();
        assert!(state.completed);
        assert!(get_onboarding_state(&db).unwrap().completed);
    }

    #[test]
    fn onboarding_can_be_reset_for_testing() {
        let db = Db::open_in_memory().unwrap();
        set_onboarding_completed(&db, true).unwrap();
        let state = set_onboarding_completed(&db, false).unwrap();
        assert!(!state.completed);
    }

    #[test]
    fn dismissing_the_accessibility_banner_persists_independently_of_completion() {
        let db = Db::open_in_memory().unwrap();
        set_onboarding_completed(&db, true).unwrap();
        let state = dismiss_accessibility_banner(&db).unwrap();
        assert!(state.completed);
        assert!(state.accessibility_banner_dismissed);
    }

    #[test]
    fn clipboard_history_enabled_defaults_to_false_and_round_trips() {
        let db = Db::open_in_memory().unwrap();
        assert!(!get_clipboard_history_enabled(&db).unwrap());
        set_clipboard_history_enabled(&db, true).unwrap();
        assert!(get_clipboard_history_enabled(&db).unwrap());
        set_clipboard_history_enabled(&db, false).unwrap();
        assert!(!get_clipboard_history_enabled(&db).unwrap());
    }
}
