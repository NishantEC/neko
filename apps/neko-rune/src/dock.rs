//! The Dock badge: the same count the web's title shows (`shell/attention.ts`: "(2) Neko"), put on the app's Dock tile
//! with `NSApp.dockTile.badgeLabel` through objc2. A window that is not the real one (a headless run, a test) has no Dock
//! and does nothing: the function is a field of `App` (`App::dock`), which only the real window sets.

/// The label the Dock shows for `count`: nothing for none (it clears the badge), the number up to 99, then "99+".
pub fn label(count: usize) -> Option<String> {
    match count {
        0 => None,
        1..=99 => Some(count.to_string()),
        _ => Some("99+".to_owned()),
    }
}

/// Sets the badge on the Dock tile. Only the main thread may touch AppKit, so off it this does nothing (the real window calls it
/// from its frame loop, which is the main thread).
#[cfg(target_os = "macos")]
pub fn set_badge(count: usize) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;
    let Some(mtm) = MainThreadMarker::new() else { return };
    let app = NSApplication::sharedApplication(mtm);
    let tile = app.dockTile();
    let words = label(count).map(|l| NSString::from_str(&l));
    tile.setBadgeLabel(words.as_deref());
}

#[cfg(not(target_os = "macos"))]
pub fn set_badge(_count: usize) {}

/// What a headless run does: nothing.
pub fn none(_count: usize) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_is_the_count_capped_and_empty_for_none() {
        assert_eq!(label(0), None);
        assert_eq!(label(3).as_deref(), Some("3"));
        assert_eq!(label(99).as_deref(), Some("99"));
        assert_eq!(label(100).as_deref(), Some("99+"));
    }
}
