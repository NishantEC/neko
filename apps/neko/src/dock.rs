//! Registering with the Dock on the main thread, before Rune starts (see "Known Rune issue on macOS" in rune-ui's README).
//!
//! AppKit registers a process with the Dock once, on whichever thread first makes a symbol image. Rune makes one on a short-lived
//! launch thread; when that thread wins, the Dock's link to the app dies with it, and the Dock heads the app's menu "Application Not
//! Responding" although the app is fine (about four launches in five). Asking for a symbol image here first ties the registration to
//! the main thread. Call it only for a real window (a headless run must not appear in the Dock), and remove it once Rune stops making
//! that image off the main thread.

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub fn register_on_main_thread() {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{MainThreadMarker, class, msg_send};
    use objc2_foundation::NSString;
    if MainThreadMarker::new().is_none() {
        return;
    }
    let name = NSString::from_str("info.circle");
    // SAFETY: a class method taking an NSString and a nullable NSString, returning an autoreleased NSImage or nil; main thread.
    let _: Option<Retained<AnyObject>> = unsafe { msg_send![class!(NSImage), imageWithSystemSymbolName: &*name, accessibilityDescription: None::<&NSString>] };
}

#[cfg(not(target_os = "macos"))]
pub fn register_on_main_thread() {}

#[cfg(test)]
mod tests {
    /// The registration is asked for on the main thread before Rune starts, and only on the path a real window takes: a headless
    /// run returns before it.
    #[test]
    fn the_dock_registration_comes_before_rune_starts_and_only_for_a_real_window() {
        let code = include_str!("lib.rs");
        let headless = code.find("if headless() {").expect("lib.rs returns early for a headless run");
        let register = code.find("dock::register_on_main_thread();").expect("lib.rs registers with the Dock");
        let start = code.find("rune::run_windows(").expect("lib.rs starts Rune through run_windows");
        assert!(headless < register && register < start, "headless return, then the Dock registration, then Rune");
        assert_eq!(code.matches("rune::run_windows(").count(), 1, "one place starts the real window");
    }
}
