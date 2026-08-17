//! The real macOS Accessibility permission (design report §3, steps 02-05,
//! 08): a genuine TCC grant, checked and requested here rather than in
//! `neko-core` — like live hotkey registration, this only makes sense from
//! the process that actually needs the grant (this one), not the daemon.
//!
//! `AccessibilityChecker` exists so `onboarding::view` (and `panel.rs`'s
//! step-08 banner) can be driven by a fake instead of a real TCC round-trip
//! in tests, the same shape as `hotkey_client::HotkeyRegistrar`.

pub trait AccessibilityChecker {
    /// `AXIsProcessTrusted()` — reads current state, no side effect, no
    /// dialog.
    fn is_trusted(&self) -> bool;
    /// `AXIsProcessTrustedWithOptions` with the prompt option set — shows
    /// the real OS dialog (step 03) if not already trusted. Returns the
    /// trusted state at the moment of the call, same as the real API: the
    /// grant itself only lands later, once the user acts on the dialog, so
    /// callers should keep polling `is_trusted` afterward rather than
    /// trusting this return value.
    fn request_prompt(&self) -> bool;
}

#[cfg(target_os = "macos")]
pub struct SystemAccessibilityChecker;

#[cfg(target_os = "macos")]
impl AccessibilityChecker for SystemAccessibilityChecker {
    fn is_trusted(&self) -> bool {
        // SAFETY: `AXIsProcessTrusted` takes no arguments and has no
        // preconditions beyond linking `ApplicationServices` (via
        // `accessibility-sys`'s framework link directive).
        unsafe { accessibility_sys::AXIsProcessTrusted() }
    }

    fn request_prompt(&self) -> bool {
        use core_foundation::base::TCFType;
        use core_foundation::boolean::CFBoolean;
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::string::CFString;

        // SAFETY: `kAXTrustedCheckOptionPrompt` is a process-lifetime CF
        // constant owned by the system framework; `wrap_under_get_rule`
        // retains it before this wrapper can release it on drop, which is
        // the documented-correct way to hold a "Get"-rule CF constant.
        let key = unsafe { CFString::wrap_under_get_rule(accessibility_sys::kAXTrustedCheckOptionPrompt) };
        let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
        // SAFETY: `options` is a live, correctly-typed `CFDictionaryRef` for
        // the duration of this call; `AXIsProcessTrustedWithOptions` reads
        // it synchronously and does not retain it past return.
        unsafe { accessibility_sys::AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
    }
}

/// Opens System Settings' own Accessibility pane directly. Used by the
/// post-onboarding refusal banner (step 08): once a user has explicitly
/// dismissed the OS's own prompt dialog once, macOS does not reliably show
/// it again from `AXIsProcessTrustedWithOptions`, so the honest "way back"
/// is a direct deep link into the pane itself rather than re-requesting a
/// dialog that may not appear.
#[cfg(target_os = "macos")]
pub fn open_accessibility_settings() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn();
}

#[cfg(not(target_os = "macos"))]
pub fn open_accessibility_settings() {}

#[cfg(test)]
pub struct FakeAccessibilityChecker {
    pub trusted: std::cell::Cell<bool>,
}

#[cfg(test)]
impl FakeAccessibilityChecker {
    pub fn new(trusted: bool) -> Self {
        Self {
            trusted: std::cell::Cell::new(trusted),
        }
    }
}

#[cfg(test)]
impl AccessibilityChecker for FakeAccessibilityChecker {
    fn is_trusted(&self) -> bool {
        self.trusted.get()
    }

    fn request_prompt(&self) -> bool {
        self.trusted.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_checker_is_usable_as_a_trait_object() {
        let checker: Box<dyn AccessibilityChecker> = Box::new(FakeAccessibilityChecker::new(true));
        assert!(checker.is_trusted());
        assert!(checker.request_prompt());
    }

    #[test]
    fn fake_checker_can_simulate_a_grant_arriving_mid_flow() {
        let fake = FakeAccessibilityChecker::new(false);
        assert!(!fake.is_trusted());
        fake.trusted.set(true);
        assert!(fake.is_trusted());
    }
}
