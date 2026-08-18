//! Reads back the summon window's real `NSWindowCollectionBehavior` to
//! establish whether it's actually reachable after a Space switch or while
//! a full-screen app owns the current Space — `data/neko-audit/report.md`
//! Part 4 item 10, deliberately **not** verified live by the audit itself
//! (it would have needed a system-wide Space-switch keystroke on a machine
//! also running the captain's live work, ruled out by the same standing
//! rule this task inherits — see `AGENTS.md`, "Standing rule").
//!
//! **Verdict, established by live readback on the real client process (see
//! `main.rs`'s call site, right after `material::verify_installed`):
//! already correct, nothing to fix.** `main.rs` opens the summon window
//! with `kind: WindowKind::PopUp`, and `gpui = "0.2.2"`'s own mac backend
//! (`gpui-0.2.2/src/platform/mac/window.rs`, `MacWindow::open`,
//! `WindowKind::PopUp` branch) unconditionally sets
//! `NSWindowCollectionBehaviorCanJoinAllSpaces |
//! NSWindowCollectionBehaviorFullScreenAuxiliary` on any window of that
//! kind — before this task, nothing in `neko` itself ever read that back
//! to confirm it, which is exactly why the audit could only grep (zero
//! hits for `NSWindowCollectionBehavior` in this crate) and call the
//! result unverified. Per Apple's documented semantics (`objc2-app-kit`'s
//! own doc comments on `NSWindowCollectionBehavior`, mirroring
//! `NSWindow.h`): `.canJoinAllSpaces` makes a window "visible on every
//! Space" instead of pinned to whichever Space was active when it was
//! created (the exact risk the audit named for a resident, never-recreated
//! window), and `.fullScreenAuxiliary` is what lets a window "appear above
//! a full-screen app's own Space" instead of being hidden behind it. Both
//! bits are the correct, complete fix for both halves of the audit's
//! concern — reading them back live, not just trusting gpui's source,
//! because a setter call silently not taking effect is exactly what
//! `material.rs::verify_installed` already exists to catch for the window
//! material chain.

use gpui::Window;

/// Pure bit-flag check, factored out so the "is this collection behavior
/// good enough" question is unit-testable without a real `NSWindow` —
/// `verify` below just supplies the live readback.
pub(crate) fn joins_all_spaces_and_fullscreen_auxiliary(collection_behavior_bits: u64) -> bool {
    const CAN_JOIN_ALL_SPACES: u64 = 1 << 0;
    const FULL_SCREEN_AUXILIARY: u64 = 1 << 8;
    collection_behavior_bits & CAN_JOIN_ALL_SPACES != 0 && collection_behavior_bits & FULL_SCREEN_AUXILIARY != 0
}

#[cfg(target_os = "macos")]
pub fn verify(window: &Window) -> Result<String, String> {
    macos::verify(window)
}

#[cfg(not(target_os = "macos"))]
pub fn verify(_window: &Window) -> Result<String, String> {
    Err("Spaces/full-screen collection-behavior verification is only implemented on macOS".to_string())
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::rc::Retained;
    use objc2_app_kit::{NSView, NSWindow, NSWindowCollectionBehavior};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use super::{Window, joins_all_spaces_and_fullscreen_auxiliary};

    /// Same walk as `material.rs::native_window`/`display_placement`'s own
    /// copy — duplicated for the same reason `display_placement` duplicates
    /// it rather than reaching into `material`'s private submodule.
    fn native_window(window: &Window) -> Result<Retained<NSWindow>, String> {
        let handle = HasWindowHandle::window_handle(window)
            .map_err(|e| format!("no raw window handle: {e}"))?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return Err("not a macOS AppKit window handle".to_string());
        };
        // SAFETY: same as `material.rs`/`display_placement.rs` — a live
        // rendering `NSView` for as long as this resident `Window` exists,
        // used only to walk up to its owning `NSWindow`.
        let rendering_view: Retained<NSView> =
            unsafe { Retained::retain(appkit.ns_view.as_ptr().cast()) }
                .ok_or_else(|| "raw-window-handle returned a null NSView".to_string())?;
        rendering_view
            .window()
            .ok_or_else(|| "GPUI's rendering view has no owning NSWindow yet".to_string())
    }

    pub fn verify(window: &Window) -> Result<String, String> {
        let native = native_window(window)?;
        let behavior: NSWindowCollectionBehavior = native.collectionBehavior();
        let bits = behavior.0 as u64;
        if !joins_all_spaces_and_fullscreen_auxiliary(bits) {
            return Err(format!(
                "expected NSWindowCollectionBehaviorCanJoinAllSpaces | \
                 NSWindowCollectionBehaviorFullScreenAuxiliary, readback bits={bits:#x} \
                 ({behavior:?}) — the summon window would be pinned to one Space \
                 and/or hidden behind a full-screen app"
            ));
        }
        Ok(format!("readback bits={bits:#x} ({behavior:?}) — reachable from every Space and over full-screen apps"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAN_JOIN_ALL_SPACES: u64 = 1 << 0;
    const FULL_SCREEN_AUXILIARY: u64 = 1 << 8;
    const MOVE_TO_ACTIVE_SPACE: u64 = 1 << 1;

    #[test]
    fn both_required_bits_set_passes() {
        assert!(joins_all_spaces_and_fullscreen_auxiliary(
            CAN_JOIN_ALL_SPACES | FULL_SCREEN_AUXILIARY
        ));
    }

    #[test]
    fn missing_can_join_all_spaces_fails() {
        assert!(!joins_all_spaces_and_fullscreen_auxiliary(FULL_SCREEN_AUXILIARY));
    }

    #[test]
    fn missing_full_screen_auxiliary_fails() {
        assert!(!joins_all_spaces_and_fullscreen_auxiliary(CAN_JOIN_ALL_SPACES));
    }

    #[test]
    fn no_bits_set_fails() {
        assert!(!joins_all_spaces_and_fullscreen_auxiliary(0));
    }

    #[test]
    fn other_unrelated_bits_do_not_substitute_for_the_required_ones() {
        // e.g. `.moveToActiveSpace` alone (a different, narrower behavior:
        // moves the window to whatever Space is active *at creation time*,
        // not "visible on every Space" going forward) must not pass.
        assert!(!joins_all_spaces_and_fullscreen_auxiliary(MOVE_TO_ACTIVE_SPACE));
    }
}
