//! The menu bar item: a port of `MenuBarIcon.swift` (the glyph and its two marks), `StatusSummary.swift` (the two lines) and the menu of
//! `NekoApp.swift` (`StatusMenu`). Rune has no macOS tray (`tray.rs`), so this is `NSStatusItem` through `objc2-app-kit`.
//!
//! Two halves. The **model** is pure and tested without AppKit: [`Counts`] from the snapshot (Swift's `TaskStage` mapping), [`IconState`],
//! the two lines, and [`menu`], the list of [`Entry`] the menu shows (the same items, order, words, separators and enabling as Swift).
//! The **native half** (macOS only) draws the template image in code (an SVG of a cat's head, rasterised with `resvg` at 2x, so no raster
//! asset ships) and keeps an `NSMenu` equal to the model, changing it in place.

use neko_protocol::workbench::TaskStatus;

use crate::link::Summary;
use crate::shell::open_in::OpenIn;

/// ⌘⇧Esc as Swift prints it in the menu (`StopAllWork.shortcutLabel`).
pub const STOP_LABEL: &str = "⌘⇧Esc";

/// How many tasks stand in each stage Swift's menu bar counts (`TaskStage`, `TaskCounts`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// `needsYou` plus `review`: AwaitingApproval, Failed and ReadyForReview (`TaskCounts.waitingOnYou`).
    pub waiting: usize,
    /// Planning, Building and Reviewing.
    pub working: usize,
    /// Queued.
    pub up_next: usize,
}

impl Counts {
    pub fn of(summary: &Summary) -> Counts {
        let mut c = Counts::default();
        for task in &summary.tasks {
            match task.status {
                TaskStatus::AwaitingApproval | TaskStatus::Failed | TaskStatus::ReadyForReview => c.waiting += 1,
                TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing => c.working += 1,
                TaskStatus::Queued => c.up_next += 1,
                _ => {}
            }
        }
        c
    }

    /// `StatusSummary.attentionLabel`.
    pub fn attention_label(&self) -> String {
        match self.waiting {
            0 => "Nothing is waiting on you".to_owned(),
            n => format!("{n} {} waiting on you", if n == 1 { "task is" } else { "tasks are" }),
        }
    }

    /// `StatusSummary.workingLabel`.
    pub fn working_label(&self) -> String {
        let running = if self.working == 0 { "Nothing running".to_owned() } else { format!("{} running", self.working) };
        if self.up_next == 0 { running } else { format!("{running} · {} up next", self.up_next) }
    }
}

/// What the icon says without opening the menu. Needing you outranks working (`MenuBarState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconState {
    Idle,
    Working,
    NeedsYou(usize),
}

impl IconState {
    pub fn of(c: &Counts) -> IconState {
        if c.waiting > 0 {
            IconState::NeedsYou(c.waiting)
        } else if c.working > 0 {
            IconState::Working
        } else {
            IconState::Idle
        }
    }

    /// Read by VoiceOver in place of the picture (`MenuBarState.accessibilityLabel`).
    pub fn accessibility_label(&self) -> String {
        match self {
            IconState::Idle => "Neko".to_owned(),
            IconState::Working => "Neko, working".to_owned(),
            IconState::NeedsYou(n) => format!("Neko, {n} {} you", if *n == 1 { "task needs" } else { "tasks need" }),
        }
    }
}

/// What a menu item does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    QuickPanel,
    OpenNeko,
    Settings,
    OpenIn(OpenIn),
    StopAll,
    Uninstall,
    Quit,
}

/// One line of the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A line of text that does nothing (SwiftUI's `Text` in a menu: shown, not enabled).
    Info(String),
    Item { action: Action, title: String, enabled: bool, /** A key equivalent that works while the menu is open: ⌘ and this key. */ key: Option<char> },
    /// "Open Neko in": a submenu of radio items.
    Picker { title: String, options: Vec<(Action, String, bool)> },
    Separator,
}

/// `StatusMenu`, item for item: the two lines, a divider, Quick panel, Open Neko, Settings…, the Open-in picker, a divider, Stop all work,
/// a divider, Uninstall Neko… (only from an installed app), Quit Neko (⌘Q).
pub fn menu(c: &Counts, open_in: OpenIn, uninstall_available: bool) -> Vec<Entry> {
    let item = |action, title: &str, key| Entry::Item { action, title: title.to_owned(), enabled: true, key };
    let mut out = vec![
        Entry::Info(c.attention_label()),
        Entry::Info(c.working_label()),
        Entry::Separator,
        item(Action::QuickPanel, "Quick panel", None),
        item(Action::OpenNeko, "Open Neko", None),
        item(Action::Settings, "Settings…", None),
        Entry::Picker { title: "Open Neko in".to_owned(), options: OpenIn::ALL.iter().map(|o| (Action::OpenIn(*o), o.title().to_owned(), *o == open_in)).collect() },
        Entry::Separator,
        item(Action::StopAll, &format!("Stop all work ({STOP_LABEL})"), None),
        Entry::Separator,
    ];
    if uninstall_available {
        out.push(item(Action::Uninstall, "Uninstall Neko…", None));
    }
    out.push(item(Action::Quit, "Quit Neko", Some('q')));
    out
}

// ---- The glyph ----------------------------------------------------------------------------------

/// The picture's size in points (`MenuBarIcon`: 20 by 16).
pub const ICON_W: u32 = 20;
pub const ICON_H: u32 = 16;
/// Drawn at this many pixels per point (a Retina menu bar).
pub const ICON_SCALE: u32 = 2;

/// The icon as an SVG in a 20 by 16 box: a cat's head in black (a template image uses only the alpha), and the state's mark in the corner
/// where Swift puts it (a 5 point square 1 point from the top and right edges): a filled dot when something needs you, a ring while work runs.
pub fn icon_svg(state: IconState) -> String {
    let cat = r##"<path d="M2.2 6.4 V2.2 L5.6 4.1 Q8 3.4 10.4 4.1 L13.8 2.2 V6.4 Q14.8 8 14.1 10.1 Q12.9 13.7 8.1 13.9 Q3.3 13.7 2.1 10.1 Q1.4 8 2.2 6.4 Z" fill="none" stroke="#000" stroke-width="1.3" stroke-linejoin="round"/><circle cx="5.9" cy="8.4" r="0.95" fill="#000"/><circle cx="10.3" cy="8.4" r="0.95" fill="#000"/><path d="M7.2 10.4 H9 L8.1 11.4 Z" fill="#000"/>"##;
    // The mark's box: x from 14 to 19, y from 1 to 6 (Swift: `width - 6`, `height - 6` from the bottom of a 16 point box, 5 by 5).
    let mark = match state {
        IconState::Idle => String::new(),
        IconState::NeedsYou(_) => r##"<circle cx="16.5" cy="3.5" r="2.5" fill="#000"/>"##.to_owned(),
        IconState::Working => r##"<circle cx="16.5" cy="3.5" r="2" fill="none" stroke="#000" stroke-width="1"/>"##.to_owned(),
    };
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{ICON_W}" height="{ICON_H}" viewBox="0 0 {ICON_W} {ICON_H}">{cat}{mark}</svg>"##)
}

/// The icon's pixels (straight RGBA, `ICON_W * ICON_SCALE` by `ICON_H * ICON_SCALE`).
pub fn icon_pixels(state: IconState) -> Result<(Vec<u8>, (u32, u32)), String> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_data(icon_svg(state).as_bytes(), &usvg::Options::default()).map_err(|e| e.to_string())?;
    let (w, h) = (ICON_W * ICON_SCALE, ICON_H * ICON_SCALE);
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("empty icon")?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(ICON_SCALE as f32, ICON_SCALE as f32), &mut pixmap.as_mut());
    let mut rgba = pixmap.take();
    for p in rgba.chunks_exact_mut(4) {
        let a = u32::from(p[3]);
        if a != 0 && a != 255 {
            for c in &mut p[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Ok((rgba, (w, h)))
}

/// The icon as PNG bytes, for `NSImage`.
pub fn icon_png(state: IconState) -> Result<Vec<u8>, String> {
    let (rgba, (w, h)) = icon_pixels(state)?;
    let image = image::RgbaImage::from_raw(w, h, rgba).ok_or("icon size")?;
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

// ---- The native half ----------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod native {
    use std::cell::RefCell;

    use objc2::rc::Retained;
    use objc2::runtime::NSObject;
    use objc2::{define_class, msg_send, sel, AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{NSControlStateValueOff, NSControlStateValueOn, NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength};
    use objc2_foundation::{NSData, NSObjectProtocol, NSSize, NSString};
    use rune::AppHandle;

    use super::{icon_png, Action, Entry, IconState, ICON_H, ICON_W};
    use crate::app::App;

    /// The actions' order: a menu item's tag is an index into this list (rebuilt with the menu).
    pub struct Target {
        handle: AppHandle<App>,
        actions: RefCell<Vec<Action>>,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "NekoRuneStatusTarget"]
        #[ivars = Target]
        pub struct StatusTarget;

        unsafe impl NSObjectProtocol for StatusTarget {}

        impl StatusTarget {
            #[unsafe(method(act:))]
            fn act(&self, sender: &NSMenuItem) {
                let tag = sender.tag();
                let action = usize::try_from(tag).ok().and_then(|i| self.ivars().actions.borrow().get(i).copied());
                if let Some(action) = action {
                    self.ivars().handle.update(move |s: &mut App, tx| {
                        s.shell_action(action, tx);
                        tx.instant();
                    });
                }
            }
        }
    );

    struct Native {
        item: Retained<NSStatusItem>,
        menu: Retained<NSMenu>,
        target: Retained<StatusTarget>,
        shown: Vec<Entry>,
        icon: Option<IconState>,
    }

    thread_local! {
        static NATIVE: RefCell<Option<Native>> = const { RefCell::new(None) };
    }

    fn ns(s: &str) -> Retained<NSString> {
        NSString::from_str(s)
    }

    fn image(state: IconState) -> Option<Retained<NSImage>> {
        let png = icon_png(state).ok()?;
        let data = NSData::with_bytes(&png);
        let image = NSImage::initWithData(NSImage::alloc(), &data)?;
        image.setSize(NSSize::new(f64::from(ICON_W), f64::from(ICON_H)));
        image.setTemplate(true);
        image.setAccessibilityDescription(Some(&ns(&state.accessibility_label())));
        Some(image)
    }

    fn make_item(mtm: MainThreadMarker, target: &StatusTarget, actions: &mut Vec<Action>, entry: &Entry) -> Retained<NSMenuItem> {
        match entry {
            Entry::Separator => NSMenuItem::separatorItem(mtm),
            Entry::Info(words) => {
                let item = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(mtm.alloc(), &ns(words), None, &ns("")) };
                item.setEnabled(false);
                item
            }
            Entry::Item { action, title, enabled, key } => {
                let item = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(mtm.alloc(), &ns(title), Some(sel!(act:)), &ns(&key.map(String::from).unwrap_or_default())) };
                unsafe { item.setTarget(Some(target)) };
                item.setTag(actions.len() as isize);
                actions.push(*action);
                item.setEnabled(*enabled);
                item
            }
            Entry::Picker { title, options } => {
                let item = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(mtm.alloc(), &ns(title), None, &ns("")) };
                let sub = NSMenu::initWithTitle(mtm.alloc(), &ns(title));
                for (action, words, checked) in options {
                    let o = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(mtm.alloc(), &ns(words), Some(sel!(act:)), &ns("")) };
                    unsafe { o.setTarget(Some(target)) };
                    o.setTag(actions.len() as isize);
                    actions.push(*action);
                    o.setState(if *checked { NSControlStateValueOn } else { NSControlStateValueOff });
                    sub.addItem(&o);
                }
                item.setSubmenu(Some(&sub));
                item
            }
        }
    }

    /// Makes the item on first call; afterwards brings the icon and the menu to what the model says.
    /// Removes the item from the status bar and drops it, its menu and its target.
    pub fn remove() -> bool {
        NATIVE.with(|cell| match cell.borrow_mut().take() {
            Some(n) => {
                NSStatusBar::systemStatusBar().removeStatusItem(&n.item);
                true
            }
            None => false,
        })
    }

    pub fn sync(handle: &AppHandle<App>, state: IconState, entries: &[Entry]) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        NATIVE.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.is_none() {
                let bar = NSStatusBar::systemStatusBar();
                let item = bar.statusItemWithLength(NSVariableStatusItemLength);
                let target: Retained<StatusTarget> = {
                    let this = mtm.alloc::<StatusTarget>().set_ivars(Target { handle: handle.clone(), actions: RefCell::new(Vec::new()) });
                    unsafe { msg_send![super(this), init] }
                };
                let menu = NSMenu::initWithTitle(mtm.alloc(), &ns("Neko"));
                menu.setAutoenablesItems(false);
                item.setMenu(Some(&menu));
                *slot = Some(Native { item, menu, target, shown: Vec::new(), icon: None });
            }
            let Some(n) = slot.as_mut() else { return };
            if n.icon != Some(state) {
                if let Some(button) = n.item.button(mtm) {
                    button.setImage(image(state).as_deref());
                }
                n.icon = Some(state);
            }
            if n.shown != entries {
                let mut actions = Vec::new();
                n.menu.removeAllItems();
                for entry in entries {
                    n.menu.addItem(&make_item(mtm, &n.target, &mut actions, entry));
                }
                *n.target.ivars().actions.borrow_mut() = actions;
                n.shown = entries.to_vec();
            }
        });
    }
}

/// Puts the icon and the menu on the menu bar (macOS), creating the item the first time.
#[cfg(target_os = "macos")]
pub fn sync(handle: &rune::AppHandle<crate::app::App>, state: IconState, entries: &[Entry]) {
    native::sync(handle, state, entries);
}
/// Takes the item out of the menu bar (the Swift app holds it now). A later `sync` makes a new one. True when there was one.
#[cfg(target_os = "macos")]
pub fn remove() -> bool {
    native::remove()
}
#[cfg(not(target_os = "macos"))]
pub fn remove() -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn sync(_handle: &rune::AppHandle<crate::app::App>, _state: IconState, _entries: &[Entry]) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::{Summary, TaskRow, Wait};

    fn row(id: &str, status: TaskStatus) -> TaskRow {
        TaskRow { id: id.into(), key: None, title: id.into(), status, updated_at_ms: 0, goal: String::new(), wait: Wait::None, progress: None, line: Default::default() }
    }

    fn summary(statuses: &[TaskStatus]) -> Summary {
        Summary { tasks: statuses.iter().enumerate().map(|(i, st)| row(&format!("t{i}"), *st)).collect(), ..Summary::default() }
    }

    use TaskStatus::*;

    #[test]
    fn counts_follow_swifts_task_stages() {
        let c = Counts::of(&summary(&[AwaitingApproval, Failed, ReadyForReview, Planning, Building, Reviewing, Queued, Queued, Completed, Cancelled]));
        assert_eq!(c, Counts { waiting: 3, working: 3, up_next: 2 });
        assert_eq!(Counts::of(&Summary::default()), Counts::default());
    }

    #[test]
    fn the_two_lines_use_swifts_words() {
        let z = Counts::default();
        assert_eq!(z.attention_label(), "Nothing is waiting on you");
        assert_eq!(z.working_label(), "Nothing running");
        let c = Counts { waiting: 1, working: 2, up_next: 0 };
        assert_eq!(c.attention_label(), "1 task is waiting on you");
        assert_eq!(c.working_label(), "2 running");
        let c = Counts { waiting: 4, working: 0, up_next: 3 };
        assert_eq!(c.attention_label(), "4 tasks are waiting on you");
        assert_eq!(c.working_label(), "Nothing running · 3 up next");
        assert_eq!(Counts { waiting: 0, working: 1, up_next: 1 }.working_label(), "1 running · 1 up next");
    }

    #[test]
    fn needing_you_outranks_working_and_each_state_has_its_words() {
        assert_eq!(IconState::of(&Counts { waiting: 2, working: 5, up_next: 0 }), IconState::NeedsYou(2));
        assert_eq!(IconState::of(&Counts { waiting: 0, working: 1, up_next: 0 }), IconState::Working);
        assert_eq!(IconState::of(&Counts { waiting: 0, working: 0, up_next: 9 }), IconState::Idle, "queued work is not working");
        assert_eq!(IconState::Idle.accessibility_label(), "Neko");
        assert_eq!(IconState::Working.accessibility_label(), "Neko, working");
        assert_eq!(IconState::NeedsYou(1).accessibility_label(), "Neko, 1 task needs you");
        assert_eq!(IconState::NeedsYou(3).accessibility_label(), "Neko, 3 tasks need you");
    }

    fn titles(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| match e {
                Entry::Info(t) => format!("info:{t}"),
                Entry::Item { title, enabled, key, .. } => format!("{}{title}{}", if *enabled { "" } else { "(off) " }, key.map(|k| format!(" [⌘{k}]")).unwrap_or_default()),
                Entry::Picker { title, options } => format!("{title} > {}", options.iter().map(|(_, t, on)| format!("{}{t}", if *on { "✓" } else { "" })).collect::<Vec<_>>().join(" | ")),
                Entry::Separator => "---".to_owned(),
            })
            .collect()
    }

    #[test]
    fn the_menu_is_swifts_items_in_swifts_order() {
        let c = Counts { waiting: 2, working: 1, up_next: 1 };
        assert_eq!(
            titles(&menu(&c, OpenIn::Window, true)),
            [
                "info:2 tasks are waiting on you",
                "info:1 running · 1 up next",
                "---",
                "Quick panel",
                "Open Neko",
                "Settings…",
                "Open Neko in > Browser | ✓Neko window",
                "---",
                "Stop all work (⌘⇧Esc)",
                "---",
                "Uninstall Neko…",
                "Quit Neko [⌘q]",
            ]
        );
        let plain = titles(&menu(&Counts::default(), OpenIn::Browser, false));
        assert!(!plain.iter().any(|t| t.contains("Uninstall")), "Uninstall is offered only from an installed app");
        assert_eq!(plain[6], "Open Neko in > ✓Browser | Neko window");
        assert_eq!(plain.last().unwrap(), "Quit Neko [⌘q]");
    }

    #[test]
    fn the_menu_changes_with_the_snapshot_and_nothing_else() {
        let a = menu(&Counts::default(), OpenIn::Window, false);
        assert_eq!(a, menu(&Counts::default(), OpenIn::Window, false));
        assert_ne!(a, menu(&Counts { waiting: 1, ..Counts::default() }, OpenIn::Window, false));
        assert_ne!(a, menu(&Counts::default(), OpenIn::Browser, false));
    }

    /// The alpha of the pixel at point (x, y) of the 2x picture.
    fn alpha(p: &[u8], x: f32, y: f32) -> u8 {
        let (px, py) = ((x * ICON_SCALE as f32) as usize, (y * ICON_SCALE as f32) as usize);
        p[(py * (ICON_W * ICON_SCALE) as usize + px) * 4 + 3]
    }

    #[test]
    fn the_glyph_has_a_cat_and_the_marks_sit_in_swifts_corner() {
        let (idle, (w, h)) = icon_pixels(IconState::Idle).unwrap();
        assert_eq!((w, h), (40, 32));
        assert!(idle.chunks_exact(4).filter(|p| p[3] > 128).count() > 60, "the cat is drawn");
        assert_eq!(alpha(&idle, 16.5, 3.5), 0, "idle: nothing in the corner");
        let (needs, _) = icon_pixels(IconState::NeedsYou(1)).unwrap();
        assert!(alpha(&needs, 16.5, 3.5) > 250, "needs you: a filled dot");
        let (working, _) = icon_pixels(IconState::Working).unwrap();
        assert_eq!(alpha(&working, 16.5, 3.5), 0, "working: a ring, empty in the middle");
        assert!(alpha(&working, 14.5, 3.5) > 100 || alpha(&working, 18.5, 3.5) > 100, "working: the ring's stroke");
        // Template: only the alpha matters, and everything drawn is black.
        assert!(needs.chunks_exact(4).filter(|p| p[3] > 0).all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
        // The mark stays inside the box and never leaves it.
        let png = icon_png(IconState::NeedsYou(1)).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        if std::env::var_os("NEKO_P5A_ICON_DIR").is_some() {
            let dir = std::path::PathBuf::from(std::env::var_os("NEKO_P5A_ICON_DIR").unwrap());
            for (name, st) in [("idle", IconState::Idle), ("needs", IconState::NeedsYou(1)), ("working", IconState::Working)] {
                let _ = std::fs::write(dir.join(format!("{name}.png")), icon_png(st).unwrap());
            }
        }
    }
}
