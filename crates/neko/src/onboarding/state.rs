//! Mac permission and shortcut substates. The six visible setup steps live in setup.rs.
use neko_protocol::HotkeyCombo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    AccessibilityAsk,
    AccessibilityWaiting,
    AccessibilityGranted,
    LearnHotkey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingState {
    Idle,
    Recording,
    Rejected(String),
}

pub struct Flow {
    pub step: Step,
    pub accessibility_granted: bool,
    pub clipboard_enabled: bool,
    pub hotkey_recording: RecordingState,
    pub current_combo: HotkeyCombo,
}
impl Flow {
    pub fn new(current_combo: HotkeyCombo) -> Self {
        Self {
            step: Step::AccessibilityAsk,
            accessibility_granted: false,
            clipboard_enabled: false,
            hotkey_recording: RecordingState::Idle,
            current_combo,
        }
    }
    pub fn request_accessibility(&mut self) {
        if self.step == Step::AccessibilityAsk {
            self.step = Step::AccessibilityWaiting;
        }
    }
    pub fn on_accessibility_granted(&mut self) {
        if self.step == Step::AccessibilityWaiting {
            self.accessibility_granted = true;
            self.step = Step::AccessibilityGranted;
        }
    }
    pub fn start_recording_hotkey(&mut self) {
        if self.step == Step::LearnHotkey {
            self.hotkey_recording = RecordingState::Recording;
        }
    }
    pub fn hotkey_capture_rejected(&mut self, reason: String) {
        if self.step == Step::LearnHotkey {
            self.hotkey_recording = RecordingState::Rejected(reason);
        }
    }
    pub fn hotkey_rebind_succeeded(&mut self, combo: HotkeyCombo) {
        if self.step == Step::LearnHotkey {
            self.current_combo = combo;
            self.hotkey_recording = RecordingState::Idle;
        }
    }
    pub fn hotkey_rebind_failed(&mut self, reason: String) {
        self.hotkey_capture_rejected(reason);
    }
}
pub fn canonicalize_key_name(raw: &str) -> String {
    let mut chars = raw.chars();
    match chars.next() {
        Some(first) if raw.chars().count() == 1 => first.to_uppercase().collect(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// distinct from `neko_core::hotkey::check_known_conflict` (a remote,
/// daemon-side heuristic against reserved combos) and from a live OS
/// registration attempt (the only real proof). This just rejects shapes
/// that can never be a sane global hotkey.
pub fn validate_candidate(
    modifiers: &[neko_protocol::Modifier],
    key: &str,
) -> Result<HotkeyCombo, String> {
    if modifiers.is_empty() {
        return Err("Add at least one modifier key — ⌘, ⌥, ⌃, or ⇧.".to_string());
    }
    if key.is_empty() {
        return Err("Press a key along with the modifier.".to_string());
    }
    if matches!(key, "Escape" | "Return" | "Tab" | "Backspace") {
        return Err(format!("{key} can't be used as a hotkey."));
    }
    Ok(HotkeyCombo::new(modifiers.to_vec(), key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::Modifier;
    #[test]
    fn permission_request_never_implies_permission_or_clipboard_consent() {
        let mut flow = Flow::new(HotkeyCombo::default_summon());
        flow.request_accessibility();
        assert_eq!(flow.step, Step::AccessibilityWaiting);
        assert!(!flow.accessibility_granted && !flow.clipboard_enabled);
        flow.on_accessibility_granted();
        assert!(flow.accessibility_granted && !flow.clipboard_enabled);
    }
    #[test]
    fn recording_success_changes_combo_and_failure_preserves_it() {
        let mut flow = Flow::new(HotkeyCombo::default_summon());
        flow.step = Step::LearnHotkey;
        flow.start_recording_hotkey();
        assert_eq!(flow.hotkey_recording, RecordingState::Recording);
        flow.hotkey_rebind_failed("reserved".into());
        assert_eq!(flow.current_combo, HotkeyCombo::default_summon());
        let combo = HotkeyCombo::new(vec![Modifier::Cmd, Modifier::Shift], "Space");
        flow.hotkey_rebind_succeeded(combo.clone());
        assert_eq!(flow.current_combo, combo);
        assert_eq!(flow.hotkey_recording, RecordingState::Idle);
    }
    #[test]
    fn only_modified_non_escape_keys_are_shortcut_candidates() {
        assert!(validate_candidate(&[], "Space").is_err());
        assert!(validate_candidate(&[Modifier::Cmd], "Escape").is_err());
        assert_eq!(
            validate_candidate(&[Modifier::Cmd, Modifier::Shift], "Space").unwrap(),
            HotkeyCombo::new(vec![Modifier::Cmd, Modifier::Shift], "Space")
        );
    }
}
