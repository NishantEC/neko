//! The onboarding arc's pure step-transition logic (design report §3,
//! steps 00-09), kept free of GPUI/async/daemon-IO so it's unit-testable
//! the same way `hotkey_client::HotkeyController` is: every side effect
//! (triggering the real OS accessibility prompt, a daemon round-trip, a
//! live hotkey rebind) is something the view layer performs and then
//! reports back into this state machine, not something this module does
//! itself.
//!
//! Step 03 (the real macOS system dialog) and step 08 (the post-onboarding
//! refusal banner) have no `Step` variant here: 03 is the OS's own native
//! dialog, entirely outside neko's rendering, and 08 is not part of the
//! setup wizard sequence at all — it is a banner the summoned panel shows
//! later (see `panel.rs`), gated on live `AXIsProcessTrusted()` state, not
//! a step this flow walks through.

use neko_protocol::HotkeyCombo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Welcome,
    WhatNekoNeeds,
    AccessibilityAsk,
    AccessibilityWaiting,
    AccessibilityGranted,
    ClipboardAsk,
    ClipboardGranted,
    LearnHotkey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingState {
    /// Showing the current combo, listening for it to be pressed for real.
    Idle,
    /// "Use a different combination" was clicked; listening for the next
    /// keydown to become the candidate.
    Recording,
    /// A candidate was rejected (either the fast local validity check or a
    /// failed live OS rebind) — shown inline, still in the idle keycap view.
    Rejected(String),
}

pub struct Flow {
    pub step: Step,
    pub accessibility_granted: bool,
    pub clipboard_enabled: bool,
    pub hotkey_recording: RecordingState,
    pub current_combo: HotkeyCombo,
    pub finished: bool,
}

impl Flow {
    pub fn new(initial_combo: HotkeyCombo) -> Self {
        Self {
            step: Step::Welcome,
            accessibility_granted: false,
            clipboard_enabled: false,
            hotkey_recording: RecordingState::Idle,
            current_combo: initial_combo,
            finished: false,
        }
    }

    pub fn advance_from_welcome(&mut self) {
        if self.step == Step::Welcome {
            self.step = Step::WhatNekoNeeds;
        }
    }

    pub fn continue_to_accessibility_ask(&mut self) {
        if self.step == Step::WhatNekoNeeds {
            self.step = Step::AccessibilityAsk;
        }
    }

    /// The primary "Open System Settings" action on step 02. The caller is
    /// responsible for actually invoking the OS prompt
    /// (`AccessibilityChecker::request_prompt`) alongside this transition.
    pub fn request_accessibility(&mut self) {
        if self.step == Step::AccessibilityAsk {
            self.step = Step::AccessibilityWaiting;
        }
    }

    /// "Not now" (step 02) or "Continue without it" (step 04) — both skip
    /// straight to the clipboard ask, per the batched-disclosure structure:
    /// each permission can be declined independently.
    pub fn skip_accessibility(&mut self) {
        if matches!(self.step, Step::AccessibilityAsk | Step::AccessibilityWaiting) {
            self.step = Step::ClipboardAsk;
        }
    }

    /// Called by the poller the instant `AXIsProcessTrusted()` flips true.
    pub fn on_accessibility_granted(&mut self) {
        if self.step == Step::AccessibilityWaiting {
            self.accessibility_granted = true;
            self.step = Step::AccessibilityGranted;
        }
    }

    pub fn continue_from_accessibility_granted(&mut self) {
        if self.step == Step::AccessibilityGranted {
            self.step = Step::ClipboardAsk;
        }
    }

    pub fn enable_clipboard(&mut self) {
        if self.step == Step::ClipboardAsk {
            self.clipboard_enabled = true;
            self.step = Step::ClipboardGranted;
        }
    }

    /// "Not now" on step 06 — no OS round-trip either way, so unlike
    /// accessibility there is no "waiting" state; declining just moves on.
    pub fn decline_clipboard(&mut self) {
        if self.step == Step::ClipboardAsk {
            self.clipboard_enabled = false;
            self.step = Step::LearnHotkey;
        }
    }

    pub fn continue_from_clipboard_granted(&mut self) {
        if self.step == Step::ClipboardGranted {
            self.step = Step::LearnHotkey;
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

    /// A candidate passed local validation and a live OS rebind
    /// (`HotkeyController::rebind`) succeeded.
    pub fn hotkey_rebind_succeeded(&mut self, new_combo: HotkeyCombo) {
        if self.step == Step::LearnHotkey {
            self.current_combo = new_combo;
            self.hotkey_recording = RecordingState::Idle;
        }
    }

    pub fn hotkey_rebind_failed(&mut self, reason: String) {
        if self.step == Step::LearnHotkey {
            self.hotkey_recording = RecordingState::Rejected(reason);
        }
    }

    /// The real hotkey combination was pressed while on step 09 in its
    /// idle (not-recording) state — "pressing the real combination
    /// dismisses the screen."
    pub fn hotkey_pressed(&mut self) {
        if self.step == Step::LearnHotkey && self.hotkey_recording == RecordingState::Idle {
            self.finished = true;
        }
    }

    /// The explicit "Done — take me in" fallback — the only way to finish
    /// if accessibility was declined (no live hotkey to press).
    pub fn finish(&mut self) {
        if self.step == Step::LearnHotkey {
            self.finished = true;
        }
    }

    /// The persistent "Skip setup" control — ends onboarding immediately
    /// from any step, unlike `finish` (gated on reaching `LearnHotkey`
    /// first). Deliberately does not retroactively grant or enable
    /// anything: whatever `accessibility_granted`/`clipboard_enabled`
    /// already are is exactly what a caller sees after this — the same
    /// honest, graceful-degradation state the accessibility/clipboard
    /// decline paths already produce.
    pub fn skip_onboarding(&mut self) {
        self.finished = true;
    }
}

/// Fast, local, side-effect-free validity check on a captured key chord —
/// `gpui` reports a keystroke's key in its own lower-case spelling
/// (`"space"`, `"f13"`); `neko_protocol::HotkeyCombo` and `global-hotkey`
/// both name it capitalised (`"Space"`, `"F13"`). One conversion, shared by
/// onboarding's step 09 and the Summon Hotkey preferences screen, so the two
/// can never disagree about what a given physical key is called.
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
pub fn validate_candidate(modifiers: &[neko_protocol::Modifier], key: &str) -> Result<HotkeyCombo, String> {
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

    fn combo() -> HotkeyCombo {
        HotkeyCombo::default_summon()
    }

    #[test]
    fn walks_the_full_happy_path_in_order() {
        let mut flow = Flow::new(combo());
        assert_eq!(flow.step, Step::Welcome);

        flow.advance_from_welcome();
        assert_eq!(flow.step, Step::WhatNekoNeeds);

        flow.continue_to_accessibility_ask();
        assert_eq!(flow.step, Step::AccessibilityAsk);

        flow.request_accessibility();
        assert_eq!(flow.step, Step::AccessibilityWaiting);

        flow.on_accessibility_granted();
        assert_eq!(flow.step, Step::AccessibilityGranted);
        assert!(flow.accessibility_granted);

        flow.continue_from_accessibility_granted();
        assert_eq!(flow.step, Step::ClipboardAsk);

        flow.enable_clipboard();
        assert_eq!(flow.step, Step::ClipboardGranted);
        assert!(flow.clipboard_enabled);

        flow.continue_from_clipboard_granted();
        assert_eq!(flow.step, Step::LearnHotkey);

        assert!(!flow.finished);
        flow.hotkey_pressed();
        assert!(flow.finished);
    }

    #[test]
    fn declining_accessibility_from_the_ask_skips_straight_to_clipboard() {
        let mut flow = Flow::new(combo());
        flow.advance_from_welcome();
        flow.continue_to_accessibility_ask();
        flow.skip_accessibility();
        assert_eq!(flow.step, Step::ClipboardAsk);
        assert!(!flow.accessibility_granted);
    }

    #[test]
    fn continuing_without_it_while_waiting_also_skips_to_clipboard() {
        let mut flow = Flow::new(combo());
        flow.advance_from_welcome();
        flow.continue_to_accessibility_ask();
        flow.request_accessibility();
        assert_eq!(flow.step, Step::AccessibilityWaiting);
        flow.skip_accessibility();
        assert_eq!(flow.step, Step::ClipboardAsk);
        assert!(!flow.accessibility_granted);
    }

    #[test]
    fn declining_clipboard_skips_the_granted_confirmation() {
        let mut flow = Flow::new(combo());
        flow.step = Step::ClipboardAsk;
        flow.decline_clipboard();
        assert_eq!(flow.step, Step::LearnHotkey);
        assert!(!flow.clipboard_enabled);
    }

    #[test]
    fn finish_works_without_ever_pressing_the_hotkey() {
        let mut flow = Flow::new(combo());
        flow.step = Step::LearnHotkey;
        flow.finish();
        assert!(flow.finished);
    }

    #[test]
    fn skip_onboarding_finishes_from_the_very_first_step() {
        let mut flow = Flow::new(combo());
        assert_eq!(flow.step, Step::Welcome);
        flow.skip_onboarding();
        assert!(flow.finished);
        assert!(!flow.accessibility_granted);
        assert!(!flow.clipboard_enabled);
    }

    #[test]
    fn skip_onboarding_does_not_retroactively_grant_accessibility() {
        let mut flow = Flow::new(combo());
        flow.step = Step::AccessibilityAsk;
        flow.skip_onboarding();
        assert!(flow.finished);
        assert!(!flow.accessibility_granted);
    }

    #[test]
    fn skip_onboarding_preserves_whatever_was_already_granted() {
        let mut flow = Flow::new(combo());
        flow.advance_from_welcome();
        flow.continue_to_accessibility_ask();
        flow.request_accessibility();
        flow.on_accessibility_granted();
        assert!(flow.accessibility_granted);
        flow.skip_onboarding();
        assert!(flow.finished);
        assert!(flow.accessibility_granted);
    }

    #[test]
    fn hotkey_press_is_ignored_outside_learn_hotkey_step() {
        let mut flow = Flow::new(combo());
        flow.hotkey_pressed();
        assert!(!flow.finished);
    }

    #[test]
    fn recording_flow_updates_the_current_combo_on_success() {
        let mut flow = Flow::new(combo());
        flow.step = Step::LearnHotkey;
        flow.start_recording_hotkey();
        assert_eq!(flow.hotkey_recording, RecordingState::Recording);

        let new_combo = HotkeyCombo::new(vec![Modifier::Cmd, Modifier::Shift], "Space");
        flow.hotkey_rebind_succeeded(new_combo.clone());
        assert_eq!(flow.hotkey_recording, RecordingState::Idle);
        assert_eq!(flow.current_combo, new_combo);

        // The now-current (new) combo is what dismisses the screen.
        flow.hotkey_pressed();
        assert!(flow.finished);
    }

    #[test]
    fn a_rejected_candidate_leaves_the_old_combo_registered() {
        let mut flow = Flow::new(combo());
        flow.step = Step::LearnHotkey;
        flow.start_recording_hotkey();
        flow.hotkey_rebind_failed("already reserved by another application".to_string());
        assert!(matches!(flow.hotkey_recording, RecordingState::Rejected(_)));
        assert_eq!(flow.current_combo, combo());
    }

    #[test]
    fn pressing_the_hotkey_while_recording_does_not_finish() {
        let mut flow = Flow::new(combo());
        flow.step = Step::LearnHotkey;
        flow.start_recording_hotkey();
        flow.hotkey_pressed();
        assert!(!flow.finished);
    }

    #[test]
    fn validate_candidate_rejects_a_bare_key_with_no_modifier() {
        assert!(validate_candidate(&[], "Space").is_err());
    }

    #[test]
    fn validate_candidate_rejects_escape() {
        assert!(validate_candidate(&[Modifier::Cmd], "Escape").is_err());
    }

    #[test]
    fn validate_candidate_accepts_a_sane_combo() {
        let result = validate_candidate(&[Modifier::Cmd, Modifier::Shift], "Space");
        assert_eq!(
            result.unwrap(),
            HotkeyCombo::new(vec![Modifier::Cmd, Modifier::Shift], "Space")
        );
    }
}
