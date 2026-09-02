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
use std::sync::Mutex;

/// Set by the menu bar item's click handler, cleared by whoever acts on it.
///
/// A flag rather than a callback because the click arrives on AppKit's run
/// loop where gpui's `App` is not reachable — see the module comment.
static CLICKED: AtomicBool = AtomicBool::new(false);

/// Set by the menu's "Preferences…" item. Same flag discipline as [`CLICKED`].
static PREFERENCES_REQUESTED: AtomicBool = AtomicBool::new(false);

/// The quota the menu last heard about, and the count beside it.
///
/// Held here rather than passed in per call because the two arrive on
/// different schedules — a blocked agent within a second, a quota every two
/// minutes — and the button carries **one** title. Keeping both lets whichever
/// arrives compose a label from the current value of the other, instead of
/// each clobbering the other's work.
static STATE: Mutex<MenuBarState> = Mutex::new(MenuBarState::EMPTY);

#[derive(Debug, Clone, PartialEq)]
pub struct MenuBarState {
    pub waiting: usize,
    pub quotas: Vec<neko_protocol::QuotaSummary>,
}

impl MenuBarState {
    const EMPTY: Self = Self { waiting: 0, quotas: Vec::new() };
}

/// Below this, a quota is worth interrupting somebody about.
///
/// Ten percent of a five-hour window is roughly half an hour of real work —
/// enough to finish a thought and switch deliberately, which is the whole
/// point of being told early. Warning sooner would make the menu bar a
/// permanent scold; later would mean the wall arrives mid-task, which is the
/// thing this exists to prevent.
pub const QUOTA_WARN_PERCENT: u8 = 10;

/// What the button's title should read, given everything currently known.
///
/// **Blocked agents outrank a low quota**, because one is a thing waiting on
/// you right now and the other is a thing that will matter shortly. Showing
/// both would need two numbers in a space that has room for one and no way to
/// say which is which.
pub fn title_for(state: &MenuBarState) -> Option<String> {
    // **Zero is no label, not a label reading zero** — the rule this replaced
    // `count_label` to keep. A badge claims something needs doing, and the
    // resting state of the world does not deserve a permanent mark in the
    // menu bar. No `99+` cap: that existed for a Dock badge's fixed circle,
    // and the menu bar grows to fit.
    if state.waiting > 0 {
        return Some(state.waiting.to_string());
    }
    // The tightest vendor governs, and only when it has actually crossed the
    // line — an unknown reading is not a warning.
    let lowest = state.quotas.iter().filter_map(|q| q.percent_left).min()?;
    (lowest <= QUOTA_WARN_PERCENT).then(|| format!("{lowest}%"))
}

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

/// The quota lines the menu shows, newest reading first-come-first-served.
///
/// A vendor with no number is still listed, saying so — "Grok — unknown"
/// distinguishes "I could not read it" from "not configured", and only one of
/// those is worth doing something about.
pub fn quota_lines(state: &MenuBarState) -> Vec<String> {
    state
        .quotas
        .iter()
        .map(|q| match q.percent_left {
            Some(left) => format!("{} — {left}% left", q.label),
            None => format!("{} — no quota reading", q.label),
        })
        .collect()
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
    use super::{CLICKED, PREFERENCES_REQUESTED, QUIT_REQUESTED};
    use std::sync::atomic::Ordering;

    use std::cell::RefCell;

    use objc2::rc::{Retained, autoreleasepool};
    use objc2::runtime::{AnyObject, Sel};
    use objc2::{AnyThread, MainThreadOnly, define_class, msg_send, sel};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSBitmapImageRep, NSColor,
        NSDeviceRGBColorSpace, NSEventMask, NSEventModifierFlags, NSEventType, NSGraphicsContext,
        NSImage, NSMenu, NSMenuItem, NSRectFill, NSStatusBar, NSStatusItem,
        NSVariableStatusItemLength,
    };
    use objc2_foundation::{MainThreadMarker, NSObject, NSPoint, NSRect, NSSize, NSString};

    /// The SF Symbol the item falls back to.
    ///
    /// This used to be what the item drew, on the reasoning that a symbol
    /// inherits the menu bar's tint automatically and an asset would not.
    /// That second half turned out to be avoidable: [`mark_image`] builds
    /// neko's own mark as a **template** image, which AppKit tints exactly
    /// the same way. The symbol stays as the fallback only.
    const SYMBOL: &str = "command";

    /// The mark, as the cells of its module grid.
    ///
    /// **This is a second copy of geometry that also lives in
    /// `assets/icons/neko/mark.svg`, and that is deliberate.** The status
    /// item is not a gpui surface: it wants a real `NSImage`, and the SVG
    /// pipeline (`AssetSource` + `gpui::svg()`) never reaches it. Rather than
    /// rasterise the file and plumb bitmap planes through AppKit, the mark is
    /// nothing but squares, so it is cheaper to fill them directly.
    ///
    /// Duplication is made safe by test rather than by discipline:
    /// `mark_cells_match_the_shipped_svg` reproduces the file's path data
    /// from this table and fails if the two ever drift.
    const MARK_ROWS: &[&[u8]] = &[
        &[1, 7],
        &[1, 2, 6, 7],
        &[1, 2, 3, 5, 6, 7],
        &[1, 2, 3, 4, 5, 6, 7],
        &[1, 3, 4, 5, 7],
        &[0, 1, 2, 3, 4, 5, 6, 7, 8],
        &[2, 3, 4, 5, 6],
        &[3, 4, 5],
    ];
    /// The grid the rows above are laid out on — matches the exported SVG.
    const MARK_BOX: f64 = 24.0;
    const MARK_COLS: f64 = 9.0;
    const MARK_MARGIN: f64 = 1.8;
    const MARK_GAP: f64 = 0.16;
    /// Point size of the status-item image. The menu bar is ~22pt tall.
    const MARK_POINTS: f64 = 17.0;

    /// Every filled cell as `(x, y, side)` on the [`MARK_BOX`] grid, with `y`
    /// measured **from the top** — the SVG's convention, not AppKit's.
    pub(super) fn mark_cells() -> Vec<(f64, f64, f64)> {
        let cell = (MARK_BOX - 2.0 * MARK_MARGIN) / MARK_COLS;
        let rows = MARK_ROWS.len() as f64;
        let oy = (MARK_BOX - cell * rows) / 2.0;
        let gap = cell * MARK_GAP;
        let side = cell - gap;
        let mut out = Vec::new();
        for (r, cols) in MARK_ROWS.iter().enumerate() {
            for &c in *cols {
                let x = MARK_MARGIN + f64::from(c) * cell + gap / 2.0;
                let y = oy + r as f64 * cell + gap / 2.0;
                out.push((x, y, side));
            }
        }
        out
    }

    /// The mark as a **template** image, so the menu bar tints it itself in
    /// light and dark exactly as it does an SF Symbol.
    ///
    /// `NSRectFill` rather than `NSBezierPath`: the mark is only rectangles,
    /// and `NSGraphics` is already a feature this crate enables. The image is
    /// built in points; AppKit handles the backing scale, so this needs no
    /// retina branch.
    pub(super) fn mark_image() -> Option<Retained<NSImage>> {
        // Drawn at 2x and handed back as a rep sized in points, so the menu
        // bar gets real pixels on a retina display. `lockFocus` would have
        // been shorter and is deprecated for exactly this reason: it cannot
        // draw resolution-independently.
        const SCALE: usize = 2;
        let px = (MARK_POINTS as usize) * SCALE;
        let scale = (MARK_POINTS * SCALE as f64) / MARK_BOX;
        unsafe {
            let bitmap = NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(),
                std::ptr::null_mut(),
                px as isize,
                px as isize,
                8,
                4,
                true,
                false,
                NSDeviceRGBColorSpace,
                0,
                0,
            )?;
            bitmap.setSize(NSSize { width: MARK_POINTS, height: MARK_POINTS });

            let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap)?;
            NSGraphicsContext::saveGraphicsState_class();
            NSGraphicsContext::setCurrentContext(Some(&context));
            NSColor::blackColor().setFill();
            for (x, y, side) in mark_cells() {
                // AppKit's origin is bottom-left; the grid's is top-left.
                let flipped = MARK_BOX - y - side;
                NSRectFill(NSRect {
                    origin: NSPoint { x: x * scale, y: flipped * scale },
                    size: NSSize { width: side * scale, height: side * scale },
                });
            }
            NSGraphicsContext::restoreGraphicsState_class();

            let image = NSImage::initWithSize(
                NSImage::alloc(),
                NSSize { width: MARK_POINTS, height: MARK_POINTS },
            );
            image.addRepresentation(&bitmap);
            // A template image is tinted by the menu bar rather than drawn as
            // authored, which is the whole reason the SF Symbol looked right.
            image.setTemplate(true);
            Some(image)
        }
    }

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
        /// Kept addressable, not just alive: rebuilding the menu when a quota
        /// changes needs the same target the original items were wired to, or
        /// the new ones would fire at nothing.
        target: Retained<Target>,
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
                let description = NSString::from_str("neko");
                // neko's own mark, drawn here rather than loaded: the status
                // item wants an `NSImage` and never touches the SVG pipeline.
                let mark = mark_image();
                if let Some(mark) = &mark {
                    mark.setAccessibilityDescription(Some(&description));
                }
                // **The item must never be an invisible gap.** A status item
                // with neither image nor title is live, clickable and
                // completely blank — the worst failure available, because
                // nothing looks wrong. So the drawn mark is checked for real
                // extent, then the SF Symbol, then a plain title.
                let drawn = mark
                    .as_ref()
                    .filter(|m| m.size().width > 0.0 && m.size().height > 0.0);
                if let Some(mark) = drawn {
                    button.setImage(Some(mark));
                } else {
                    let symbol = NSString::from_str(SYMBOL);
                    let fallback = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                        &symbol,
                        Some(&description),
                    );
                    match &fallback {
                        Some(image) => button.setImage(Some(image)),
                        None => button.setTitle(&NSString::from_str("neko")),
                    }
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
            let held = MenuBarItem { item, menu, target };
            let state = held.readback();
            ITEM.with(|slot| *slot.borrow_mut() = Some(held));
            state
        })
    }

    /// Puts the waiting count beside the icon, or clears it. A no-op before
    /// `install`, which is the state every test and every non-macOS build is
    /// in.
    pub fn set_waiting_count(count: usize) {
        super::STATE.lock().unwrap().waiting = count;
        refresh_title();
    }

    /// Records the latest quota reading and re-renders the item.
    pub fn set_quotas(quotas: Vec<neko_protocol::QuotaSummary>) {
        super::STATE.lock().unwrap().quotas = quotas;
        refresh_title();
        rebuild_menu();
    }

    /// Recomposes the one title from everything currently known.
    fn refresh_title() {
        let label = super::title_for(&super::STATE.lock().unwrap()).unwrap_or_default();
        ITEM.with(|slot| {
            if let Some(item) = slot.borrow().as_ref() {
                item.set_title(&label);
            }
        });
    }

    /// Rebuilds the menu so its quota lines match the latest reading.
    ///
    /// Whole-menu rather than mutating items in place: the number of vendors
    /// signed in changes, so the *shape* changes, and rebuilding is both
    /// simpler and impossible to leave half-updated.
    fn rebuild_menu() {
        let Some(mtm) = MainThreadMarker::new() else { return };
        ITEM.with(|slot| {
            if let Some(item) = slot.borrow_mut().as_mut() {
                item.menu = build_menu(mtm, &item.target);
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
        // **Quota first, because it is the thing you came to check.** The
        // lines are inert — no target, disabled — since there is nothing to
        // *do* to a reading here; the Usage mode is where it can be acted on.
        // A disabled item still reads clearly, which is exactly what a status
        // line should be.
        let quotas = super::quota_lines(&super::STATE.lock().unwrap());
        if !quotas.is_empty() {
            for line in quotas {
                let item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(&line),
                        None,
                        &NSString::from_str(""),
                    )
                };
                item.setEnabled(false);
                menu.addItem(&item);
            }
            menu.addItem(&NSMenuItem::separatorItem(mtm));
        }
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
        fn set_title(&self, label: &str) {
            let Some(mtm) = MainThreadMarker::new() else { return };
            // Pooled like every other repeated AppKit call in this project —
            // `AGENTS.md`, "Clipboard capture memory".
            autoreleasepool(|_| {
                if let Some(button) = self.item.button(mtm) {
                    button.setTitle(&NSString::from_str(label));
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

    /// Geometry sanity, independent of AppKit: every cell must be a real
    /// square that lands inside the grid. A zero-side or out-of-bounds cell
    /// would fill nothing and the item would be a blank gap.
    #[test]
    fn every_mark_cell_is_a_real_square_inside_the_grid() {
        let cells = super::macos::mark_cells();
        assert_eq!(cells.len(), 41, "the mark is 41 filled cells");
        for (x, y, side) in &cells {
            assert!(*side > 0.0, "a cell with no side fills nothing");
            assert!(*x >= 0.0 && *y >= 0.0, "cell starts outside the grid");
            assert!(x + side <= 24.0, "cell runs past the right edge");
            assert!(y + side <= 24.0, "cell runs past the bottom edge");
        }
    }

    /// The item is drawn, not loaded, so this is the only place that proves
    /// the AppKit path produces a real image rather than silently nothing.
    #[test]
    fn the_status_item_mark_is_a_template_image_with_real_extent() {
        let image = super::macos::mark_image().expect("the mark failed to draw");
        assert_eq!(image.size().width, 17.0);
        assert_eq!(image.size().height, 17.0);
        assert!(image.isTemplate(), "not a template - the menu bar would not tint it");
        assert!(
            image.representations().count() > 0,
            "no representation attached, the item would be blank"
        );
    }

    /// The status item draws the mark from [`MARK_ROWS`], while every other
    /// surface renders `assets/icons/neko/mark.svg`. Two copies of one
    /// geometry is a drift hazard, so this reproduces the file's path data
    /// from the table and fails the moment they disagree.
    #[test]
    fn mark_cells_match_the_shipped_svg() {
        let svg = include_str!("../assets/icons/neko/mark.svg");

        fn trim(v: f64) -> String {
            let s = format!("{v:.3}");
            let s = s.trim_end_matches('0').trim_end_matches('.');
            s.to_string()
        }

        let rebuilt: String = super::macos::mark_cells()
            .into_iter()
            .map(|(x, y, side)| {
                format!(
                    "M{} {}h{}v{}h-{}z",
                    trim(x),
                    trim(y),
                    trim(side),
                    trim(side),
                    trim(side)
                )
            })
            .collect();

        assert!(
            svg.contains(&rebuilt),
            "MARK_ROWS no longer reproduces mark.svg - the drawn mark and the \
             rendered one have drifted apart"
        );
    }
    use super::*;

    #[test]
    fn a_quiet_world_earns_no_mark_at_all() {
        // A badge claims something needs doing; the resting state of the
        // world does not deserve a permanent one.
        assert_eq!(title_for(&MenuBarState::EMPTY), None);
    }

    #[test]
    fn the_count_is_honest_because_the_menu_bar_grows_to_fit_it() {
        // The Dock badge capped at "99+" because a badge is a small fixed
        // circle. This is not, so there is nothing to protect.
        let one = MenuBarState { waiting: 1, quotas: Vec::new() };
        assert_eq!(title_for(&one).as_deref(), Some("1"));
        let many = MenuBarState { waiting: 250, quotas: Vec::new() };
        assert_eq!(title_for(&many).as_deref(), Some("250"));
    }

    fn summary(vendor: &str, left: Option<u8>) -> neko_protocol::QuotaSummary {
        neko_protocol::QuotaSummary {
            vendor: vendor.into(),
            label: vendor.into(),
            percent_left: left,
        }
    }

    #[test]
    fn a_blocked_agent_outranks_a_low_quota_for_the_one_title() {
        // One is waiting on you now; the other will matter shortly. There is
        // room for one number and no way to say which it is.
        let state = MenuBarState {
            waiting: 2,
            quotas: vec![summary("claude", Some(3))],
        };
        assert_eq!(title_for(&state).as_deref(), Some("2"));
    }

    #[test]
    fn only_a_quota_past_the_line_is_worth_a_title() {
        let plenty = MenuBarState { waiting: 0, quotas: vec![summary("claude", Some(60))] };
        assert_eq!(title_for(&plenty), None, "comfortable is not news");
        let low = MenuBarState { waiting: 0, quotas: vec![summary("claude", Some(4))] };
        assert_eq!(title_for(&low).as_deref(), Some("4%"));
        let edge = MenuBarState {
            waiting: 0,
            quotas: vec![summary("claude", Some(QUOTA_WARN_PERCENT))],
        };
        assert!(edge.quotas[0].percent_left.is_some());
        assert_eq!(title_for(&edge).as_deref(), Some(format!("{QUOTA_WARN_PERCENT}%").as_str()));
    }

    #[test]
    fn the_tightest_vendor_governs_and_an_unknown_one_never_warns() {
        // An unreadable vendor is not a warning — "unknown" and "empty" are
        // the two answers acted on most differently.
        let state = MenuBarState {
            waiting: 0,
            quotas: vec![summary("claude", Some(70)), summary("grok", None), summary("codex", Some(5))],
        };
        assert_eq!(title_for(&state).as_deref(), Some("5%"));
        let unreadable = MenuBarState { waiting: 0, quotas: vec![summary("grok", None)] };
        assert_eq!(title_for(&unreadable), None);
    }

    #[test]
    fn the_menu_lists_a_vendor_it_could_not_read_rather_than_hiding_it() {
        let state = MenuBarState {
            waiting: 0,
            quotas: vec![summary("Claude Code", Some(34)), summary("Grok", None)],
        };
        let lines = quota_lines(&state);
        assert_eq!(lines[0], "Claude Code — 34% left");
        assert!(lines[1].contains("no quota reading"), "{}", lines[1]);
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
