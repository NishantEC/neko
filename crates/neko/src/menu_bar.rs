//! The menu bar item, and getting neko out of the Dock.
//!
//! **A command palette does not belong in the Dock.** Raycast, Alfred and
//! Spotlight are all accessory apps: no Dock tile, no ⌘Tab entry, a small
//! menu bar icon and nothing else. neko was in the Dock for one reason —
//! gpui's mac backend calls
//! `setActivationPolicy(NSApplicationActivationPolicyRegular)` on every app
//! it starts (`gpui_macos/src/platform.rs:1253`) — and because neko is a
//! bare Mach-O with no bundle, the tile it got was a generic `exec` block
//! with no icon at all.
//!
//! That is not a decision neko has to live with. `setActivationPolicy` is an
//! ordinary runtime call, reachable from `objc2-app-kit`, and this module
//! makes it *after* gpui has had its say. No fork patch, no bundle.
//!
//! ## The Dock icon was doing two real jobs, and both moved here
//!
//! Hiding it without replacing them would have been a regression:
//!
//! - **The attention badge.** `Event::AttentionChanged`'s count lived on the
//!   Dock tile, and an accessory app has no Dock tile. It is now the menu bar
//!   button's own title, sitting next to the icon.
//! - **Click to summon.** `App::on_reopen` fires on a Dock-icon click and is
//!   the documented "never a dead end" path for somebody who declined
//!   Accessibility and therefore has no working hotkey (`AGENTS.md`,
//!   "Onboarding"). Clicking the menu bar item now does the same thing.
//!
//! ## The right-click menu
//!
//! Summon, Preferences…, Quit. The last one is load-bearing: an accessory app
//! has no Dock icon and no ⌘Tab entry, so before this menu existed the only
//! ways to stop neko were `kill` or logging out. Left click keeps the
//! one-click summon; right or ctrl click opens the menu — decided per event in
//! the action handler, because `NSStatusItem.menu` is all-or-nothing (setting
//! it makes every click open the menu and the action never fire).
//!
//! ## How the click reaches gpui
//!
//! It does not, directly, and that is deliberate. An AppKit action fires
//! inside the run loop with no `&mut App` in reach, and there is no supported
//! way to conjure one. So the action sets an [`AtomicBool`] and `main.rs`'s
//! existing 20ms poll — already there for daemon events and connection state
//! — notices it on the next tick and summons through exactly the same path
//! the hotkey uses.
//!
//! That is worth more than a cleverer bridge: no new thread, no new
//! synchronisation, no second summon path that could drift from the first,
//! and 20ms is well under the threshold at which a person could tell.

use std::sync::atomic::{AtomicBool, Ordering};

/// Set by the menu bar item's click handler, cleared by whoever acts on it.
///
/// A flag rather than a callback because the click arrives on AppKit's run
/// loop where gpui's `App` is not reachable — see the module comment.
static CLICKED: AtomicBool = AtomicBool::new(false);

/// Set by the menu's "Preferences…" item. Same flag discipline as [`CLICKED`].
static PREFERENCES_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Set by the menu's "Quit neko" item. Same flag discipline as [`CLICKED`].
///
/// **This item is load-bearing, not convenience.** An accessory app has no
/// Dock icon and no ⌘Tab entry, and the panel quits nothing — before this
/// menu existed, the only ways to stop neko were `kill` or logging out.
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Whether the menu bar item has been clicked since this was last called.
/// Clears the flag, so two consecutive calls cannot summon twice.
pub fn take_click() -> bool {
    CLICKED.swap(false, Ordering::Relaxed)
}

/// Whether the menu's Preferences item was chosen since this was last called.
pub fn take_preferences_request() -> bool {
    PREFERENCES_REQUESTED.swap(false, Ordering::Relaxed)
}

/// Whether the menu's Quit item was chosen since this was last called.
pub fn take_quit_request() -> bool {
    QUIT_REQUESTED.swap(false, Ordering::Relaxed)
}

/// The title beside the icon: the number of agents waiting, or nothing.
///
/// Same rule the Dock badge followed — **zero is no label, not a label
/// reading zero.** A count claims something needs doing, and the resting
/// state of the world does not deserve a permanent mark in the menu bar.
///
/// No `99+` cap here, unlike the Dock badge: that cap existed because a Dock
/// badge is a small fixed circle. The menu bar grows to fit its content, so
/// an honest number costs nothing.
pub fn count_label(count: usize) -> Option<String> {
    (count > 0).then(|| count.to_string())
}

#[cfg(not(target_os = "macos"))]
pub use stub::*;


#[cfg(not(target_os = "macos"))]
mod stub {
    pub fn install() -> Option<String> {
        None
    }
    pub fn set_waiting_count(_count: usize) {}
    pub fn hide_from_dock() -> Result<(), String> {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "macos")]
mod macos {
    use super::{CLICKED, PREFERENCES_REQUESTED, QUIT_REQUESTED, count_label};
    use std::sync::atomic::Ordering;

    use std::cell::RefCell;

    use objc2::rc::{Retained, autoreleasepool};
    use objc2::runtime::{AnyObject, Sel};
    use objc2::{MainThreadOnly, define_class, msg_send, sel};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSEventMask, NSEventModifierFlags,
        NSEventType, NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
        NSVariableStatusItemLength,
    };
    use objc2_foundation::{MainThreadMarker, NSObject, NSString};

    /// The SF Symbol the item draws.
    ///
    /// `command` rather than something cat-shaped: it is what this app *is*,
    /// it exists on every macOS this can run on, and as a symbol it inherits
    /// the menu bar's own tint in light and dark automatically — which a
    /// vendored asset would not, and which matters more in the menu bar than
    /// anywhere else in the app.
    const SYMBOL: &str = "command";

    // The action target. Exists only to own one selector: `NSControl`'s
    // target/action pair needs a real Objective-C object with a real method,
    // which is what `define_class!` is for. It carries no state — the click
    // sets a global flag (see the module comment) rather than reaching into
    // anything.
    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "NekoMenuBarTarget"]
        #[thread_kind = MainThreadOnly]
        struct Target;

        impl Target {
            #[unsafe(method(nekoMenuBarClicked:))]
            fn clicked(&self, _sender: Option<&AnyObject>) {
                // **One button, two gestures.** A left click summons — the
                // documented "never a dead end" path for somebody with no
                // hotkey — and a right or ctrl click opens the menu. Decided
                // here, from the event that fired the action, because
                // `NSStatusItem.menu` is all-or-nothing: setting it makes
                // *every* click open the menu and the action never fire, which
                // would take the one-click summon away to gain the menu.
                let mtm = MainThreadMarker::from(self);
                let event = NSApplication::sharedApplication(mtm).currentEvent();
                let wants_menu = event.as_ref().is_some_and(|event| {
                    matches!(
                        event.r#type(),
                        NSEventType::RightMouseUp | NSEventType::RightMouseDown
                    ) || event.modifierFlags().contains(NSEventModifierFlags::Control)
                });
                if !wants_menu {
                    CLICKED.store(true, Ordering::Relaxed);
                    return;
                }
                if let Some(event) = event {
                    show_menu(mtm, &event);
                }
            }

            #[unsafe(method(nekoMenuSummon:))]
            fn menu_summon(&self, _sender: Option<&AnyObject>) {
                CLICKED.store(true, Ordering::Relaxed);
            }

            #[unsafe(method(nekoMenuPreferences:))]
            fn menu_preferences(&self, _sender: Option<&AnyObject>) {
                PREFERENCES_REQUESTED.store(true, Ordering::Relaxed);
            }

            #[unsafe(method(nekoMenuQuit:))]
            fn menu_quit(&self, _sender: Option<&AnyObject>) {
                QUIT_REQUESTED.store(true, Ordering::Relaxed);
            }
        }
    );

    /// Pops the item's menu at the cursor.
    ///
    /// `popUpContextMenu:withEvent:forView:` rather than assigning
    /// `NSStatusItem.menu` — see `clicked` above for why the assignment is
    /// all-or-nothing. This runs AppKit's menu-tracking loop synchronously;
    /// the chosen item's own action fires before this returns, setting its
    /// flag for the 20ms poll exactly like a plain click does.
    fn show_menu(mtm: MainThreadMarker, event: &objc2_app_kit::NSEvent) {
        ITEM.with(|slot| {
            if let Some(held) = slot.borrow().as_ref()
                && let Some(button) = held.item.button(mtm)
            {
                NSMenu::popUpContextMenu_withEvent_forView(&held.menu, event, &button);
            }
        });
    }

    impl Target {
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            unsafe { msg_send![Self::alloc(mtm), init] }
        }
    }

    /// A live menu bar item.
    ///
    /// **Must be kept alive for the whole process**, which is why the module
    /// owns it rather than handing it back. `NSStatusItem` is removed from
    /// the menu bar the moment its last reference goes, so a version of this
    /// that returned the handle and let a caller drop it would compile, run,
    /// and show an item that vanished immediately. The target is held for
    /// the same reason — `setTarget:` does not retain, so a dropped target
    /// leaves the button firing a selector at freed memory.
    struct MenuBarItem {
        item: Retained<NSStatusItem>,
        /// Held for the same reason the target is: nothing else retains it,
        /// and `popUpContextMenu` borrows it per click rather than owning it.
        menu: Retained<NSMenu>,
        _target: Retained<Target>,
    }

    thread_local! {
        /// Main-thread-only by construction: `Retained<NSStatusItem>` is not
        /// `Sync`, and every AppKit call here has to be on the main thread
        /// anyway. A `thread_local` says both things at once.
        static ITEM: RefCell<Option<MenuBarItem>> = const { RefCell::new(None) };
    }

    /// Creates the item and keeps it. Returns what it reads back as, for the
    /// launch log — `None` when it could not be created at all.
    pub fn install() -> Option<String> {
        let mtm = MainThreadMarker::new()?;
        autoreleasepool(|_| {
            let item =
                NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
            let target = Target::new(mtm);
            if let Some(button) = item.button(mtm) {
                let symbol = NSString::from_str(SYMBOL);
                let description = NSString::from_str("neko");
                let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                    &symbol,
                    Some(&description),
                );
                // **A missing symbol must not produce an invisible item.**
                // `imageWithSystemSymbolName` returns `nil` for a name the
                // running OS does not have, and a status item with neither
                // image nor title is a live, clickable, completely blank gap
                // in the menu bar — the worst possible failure, because
                // nothing looks wrong.
                match &image {
                    Some(image) => button.setImage(Some(image)),
                    None => button.setTitle(&NSString::from_str("neko")),
                }
                unsafe {
                    button.setTarget(Some(&*target as &AnyObject));
                    button.setAction(Some(action_selector()));
                    // The action fires on left mouse-up by default; the menu
                    // needs the right button to reach it too.
                    button.sendActionOn(
                        NSEventMask::LeftMouseUp
                            | NSEventMask::RightMouseUp
                            | NSEventMask::RightMouseDown,
                    );
                }
            }
            let menu = build_menu(mtm, &target);
            let held = MenuBarItem { item, menu, _target: target };
            let state = held.readback();
            ITEM.with(|slot| *slot.borrow_mut() = Some(held));
            state
        })
    }

    /// Puts the waiting count beside the icon, or clears it. A no-op before
    /// `install`, which is the state every test and every non-macOS build is
    /// in.
    pub fn set_waiting_count(count: usize) {
        ITEM.with(|slot| {
            if let Some(item) = slot.borrow().as_ref() {
                item.set_waiting_count(count);
            }
        });
    }

    fn action_selector() -> Sel {
        sel!(nekoMenuBarClicked:)
    }

    /// The right-click menu: Summon, Preferences…, Quit.
    ///
    /// **`setAutoenablesItems(false)`, then every item enabled explicitly.**
    /// Auto-enabling asks the responder chain to validate each item, and this
    /// target deliberately lives outside any responder chain — under
    /// auto-enable the whole menu renders greyed out, which looks exactly like
    /// a permissions problem and is really a wiring one.
    fn build_menu(mtm: MainThreadMarker, target: &Target) -> Retained<NSMenu> {
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        let add = |title: &str, action: Sel| {
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(title),
                    Some(action),
                    &NSString::from_str(""),
                )
            };
            unsafe { item.setTarget(Some(target as &AnyObject)) };
            item.setEnabled(true);
            menu.addItem(&item);
        };
        add("Summon neko", sel!(nekoMenuSummon:));
        add("Preferences\u{2026}", sel!(nekoMenuPreferences:));
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let quit = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str("Quit neko"),
                Some(sel!(nekoMenuQuit:)),
                &NSString::from_str(""),
            )
        };
        unsafe { quit.setTarget(Some(target as &AnyObject)) };
        quit.setEnabled(true);
        menu.addItem(&quit);
        menu
    }

    impl MenuBarItem {
        fn set_waiting_count(&self, count: usize) {
            let Some(mtm) = MainThreadMarker::new() else { return };
            // Pooled like every other repeated AppKit call in this project —
            // `AGENTS.md`, "Clipboard capture memory".
            autoreleasepool(|_| {
                if let Some(button) = self.item.button(mtm) {
                    let label = count_label(count).unwrap_or_default();
                    button.setTitle(&NSString::from_str(&label));
                }
            });
        }

        /// What the button and menu read back as — the same "verified, not
        /// trusted" discipline `material::verify_installed` follows. The menu
        /// is listed item by item because a real right-click cannot be
        /// synthesised under this repo's rules: this line in the launch log is
        /// the whole proof the menu exists and holds what it should.
        fn readback(&self) -> Option<String> {
            let mtm = MainThreadMarker::new()?;
            let button = self.item.button(mtm)?;
            let items: Vec<String> = (0..self.menu.numberOfItems())
                .filter_map(|i| self.menu.itemAtIndex(i))
                .map(|item| {
                    if item.isSeparatorItem() {
                        "—".to_string()
                    } else {
                        format!(
                            "{}{}",
                            item.title(),
                            if item.isEnabled() { "" } else { " (DISABLED)" }
                        )
                    }
                })
                .collect();
            Some(format!(
                "title {:?}, image {}, menu [{}]",
                button.title().to_string(),
                if button.image().is_some() { "set" } else { "MISSING" },
                items.join(", ")
            ))
        }
    }

    /// Drops the Dock icon and the ⌘Tab entry.
    ///
    /// Called *after* gpui has started, because gpui sets
    /// `NSApplicationActivationPolicyRegular` itself during startup and the
    /// last call wins. Read back rather than trusted: a silently-ignored
    /// policy change would look exactly like the Dock icon being a gpui
    /// limitation, which is the belief this module exists to correct.
    pub fn hide_from_dock() -> Result<(), String> {
        let mtm = MainThreadMarker::new().ok_or("not on the main thread")?;
        autoreleasepool(|_| {
            let app = NSApplication::sharedApplication(mtm);
            app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
            match app.activationPolicy() {
                NSApplicationActivationPolicy::Accessory => Ok(()),
                other => Err(format!("activation policy is {other:?}, not Accessory")),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_waiting_means_no_label_at_all() {
        // A count claims something needs doing; the resting state of the
        // world does not deserve a permanent mark in the menu bar.
        assert_eq!(count_label(0), None);
    }

    #[test]
    fn the_count_is_honest_because_the_menu_bar_grows_to_fit_it() {
        // The Dock badge capped at "99+" because a badge is a small fixed
        // circle. This is not, so there is nothing to protect.
        assert_eq!(count_label(1).as_deref(), Some("1"));
        assert_eq!(count_label(250).as_deref(), Some("250"));
    }

    #[test]
    fn each_menu_flag_is_consumed_exactly_once() {
        // All three ride the same 20ms poll; a flag that stayed set would
        // reopen Preferences (or quit!) on every tick forever.
        PREFERENCES_REQUESTED.store(true, Ordering::Relaxed);
        assert!(take_preferences_request());
        assert!(!take_preferences_request());
        QUIT_REQUESTED.store(true, Ordering::Relaxed);
        assert!(take_quit_request());
        assert!(!take_quit_request());
    }

    #[test]
    fn a_click_is_consumed_exactly_once() {
        // `main.rs` polls this every 20ms; a flag that stayed set would
        // summon the panel on every tick forever.
        assert!(!take_click(), "nothing clicked yet");
        CLICKED.store(true, Ordering::Relaxed);
        assert!(take_click(), "the click is seen");
        assert!(!take_click(), "and not seen twice");
    }
}
