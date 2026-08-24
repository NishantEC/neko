//! The Dock tile's badge — how many agents are stopped, waiting for you.
//!
//! **L3 of `docs/plan-agent-control-plane.md`, and the only ambient surface
//! neko actually has.** The permission inbox's rows already arrive through
//! the ordinary search path, which covers every moment the panel is open —
//! and the panel is hidden almost all of the time. An agent that blocks while
//! you are working in another app would wait until your next summon to be
//! noticed, which defeats the point of noticing at all.
//!
//! ## Why the Dock tile and not a notification
//!
//! A real notification is the obvious answer and neko cannot post one:
//! `UNUserNotificationCenter` requires a bundle identifier, and neko is a
//! bare Mach-O (`target/release/neko`), not a `.app` — the same fact that
//! sends "Launch at login" through a LaunchAgent plist instead of
//! `SMAppService` (`AGENTS.md`, "Preferences"). A menu-bar item is the other
//! obvious answer and is equally unavailable: gpui's `status_item.rs` is
//! never wired into a `mod` declaration, so it is dead code rather than an
//! API.
//!
//! `NSApp.dockTile.badgeLabel` needs neither. It works for an unbundled
//! process, it is one call, and gpui hardcodes
//! `NSApplicationActivationPolicyRegular` — so this app *has* a Dock icon
//! whether or not it wants one. Using it costs nothing and is the one place
//! a number can sit where a person will see it without asking.
//!
//! ## The rule this follows
//!
//! **Zero is no badge, not a badge that says zero.** A badge is a claim that
//! something needs doing; a "0" is a claim that nothing does, which is the
//! resting state of the world and does not deserve a permanent mark on the
//! Dock.

#[cfg(target_os = "macos")]
pub use macos::set_waiting_count;

/// The label for `count`, or `None` when the badge should be cleared.
///
/// Split out from the AppKit call so the one rule worth testing — and the
/// cap — can be tested at all: `main.rs` is where every other native call in
/// this crate lives precisely because tests never run it.
pub fn badge_label(count: usize) -> Option<String> {
    match count {
        0 => None,
        // A Dock badge is a small circle. Past two digits macOS shrinks the
        // text rather than the circle, and "99+" is both the platform's own
        // convention and more honest than an unreadable exact number — the
        // difference between 99 and 140 blocked agents is not a difference
        // anybody acts on differently.
        1..=99 => Some(count.to_string()),
        _ => Some("99+".to_string()),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_waiting_count(_count: usize) -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::{MainThreadMarker, NSString};

    /// Paints or clears the badge. Silently does nothing off the main
    /// thread, which is where AppKit requires it and where gpui's event loop
    /// already calls this from.
    /// Returns what the tile reads back as afterwards, so a caller can log
    /// it — the same "verified, not trusted" discipline
    /// `material::verify_installed` follows for every other native property
    /// this app depends on. `None` means no badge, which is also what an
    /// off-main-thread call returns, since it did nothing.
    pub fn set_waiting_count(count: usize) -> Option<String> {
        let Some(mtm) = MainThreadMarker::new() else { return None };
        // Pooled like every other AppKit call site in this project — see
        // `AGENTS.md`, "Clipboard capture memory": an unpooled call in
        // something that runs repeatedly is exactly the defect class that
        // took the daemon to 14 GB, and this runs on every change forever.
        autoreleasepool(|_| {
            let tile = NSApplication::sharedApplication(mtm).dockTile();
            match super::badge_label(count) {
                Some(label) => tile.setBadgeLabel(Some(&NSString::from_str(&label))),
                None => tile.setBadgeLabel(None),
            }
            tile.badgeLabel().map(|read| read.to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_waiting_means_no_badge_at_all() {
        // A badge claims something needs doing. "0" claims nothing does,
        // which is the resting state of the world and does not deserve a
        // permanent mark on the Dock.
        assert_eq!(badge_label(0), None);
    }

    #[test]
    fn a_real_count_is_shown_exactly_until_the_circle_runs_out() {
        assert_eq!(badge_label(1).as_deref(), Some("1"));
        assert_eq!(badge_label(12).as_deref(), Some("12"));
        assert_eq!(badge_label(99).as_deref(), Some("99"));
        // Past two digits macOS shrinks the text rather than the circle, and
        // nobody acts differently on 100 than on 140.
        assert_eq!(badge_label(100).as_deref(), Some("99+"));
        assert_eq!(badge_label(4_000).as_deref(), Some("99+"));
    }
}
