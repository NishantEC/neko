//! The menu bar (docs/knowledge/app-shell.md), through Rune's native menus.
//!
//! * Neko: **About Neko**, Services, Hide Neko ⌘H, Hide Others ⌥⌘H, Show All, Quit Neko ⌘Q
//! * File: **New Task ⌘N**, **Close Window ⌘W**
//! * Edit: Undo ⌘Z, Redo ⇧⌘Z, Cut, Copy, Paste, Select All, then AppKit's Start Dictation and Emoji & Symbols ⌃⌘Space
//! * View: **Home ⌘1**, **Needs You ⌘2**, **Rounds ⌘3**, **Briefing ⌘4**, **Hide Sidebar / Show Sidebar ⌃⌘S**, **Enter / Exit Full Screen ⌃⌘F**
//! * Window: Minimize ⌘M, Zoom, Bring All to Front, the window list
//! * Help: **Neko Help** (the public README), registered as the help menu so macOS adds its search field
//!
//! Neko's items are in bold. The app menu's Hide, Hide Others, Show All and Quit are winit's; Edit and Window are Rune's
//! `standard_menus`. Three things Rune does not do at this rev go through raw AppKit ([`native::fix_up`], run again for a moment after
//! every menu change, since Rune rebuilds its items then): registering the Help menu, removing Rune's own Window ▸ Close (File ▸
//! Close Window has ⌘W, as in Mail and Notes), and an Enter Full Screen AppKit may add by itself (kept off with
//! `NSFullScreenMenuItemEverywhere`, as Electron does, and removed if it shows anyway).

use rune::{MenuItem, Shortcut, Tx};

use crate::state::App;

/// What Help ▸ Neko Help opens.
pub const HELP_URL: &str = "https://github.com/NishantEC/neko#readme"; // privacy-ok: Neko's own public repository

/// One of Neko's items: its id, title, menu, key, and whether the window's own keys see the chord first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: &'static str,
    pub title: &'static str,
    /// None is the application menu.
    pub menu: Option<&'static str>,
    pub key: Option<&'static str>,
    pub keys_first: bool,
    pub separator: bool,
}

/// The items Rune and AppKit put in the menu bar themselves, with their keys: listed so a test can prove no Neko key takes one of
/// theirs. Rune's Window ▸ Close ⌘W is not here: [`native::fix_up`] removes it.
pub const NATIVE: [(&str, &str, Option<&str>); 16] = [
    ("Neko", "Services", None),
    ("Neko", "Hide Neko", Some("cmd+h")),
    ("Neko", "Hide Others", Some("cmd+alt+h")),
    ("Neko", "Show All", None),
    ("Neko", "Quit Neko", Some("cmd+q")),
    ("Edit", "Undo", Some("cmd+z")),
    ("Edit", "Redo", Some("cmd+shift+z")),
    ("Edit", "Cut", Some("cmd+x")),
    ("Edit", "Copy", Some("cmd+c")),
    ("Edit", "Paste", Some("cmd+v")),
    ("Edit", "Select All", Some("cmd+a")),
    ("Edit", "Start Dictation", None),
    ("Edit", "Emoji & Symbols", Some("ctrl+cmd+space")),
    ("Window", "Minimize", Some("cmd+m")),
    ("Window", "Zoom", None),
    ("Window", "Bring All to Front", None),
];

fn item(id: &'static str, title: &'static str, menu: Option<&'static str>, key: Option<&'static str>) -> Item {
    Item { id, title, menu, key, keys_first: true, separator: false }
}

/// Every item Neko adds, in menu order, for a window whose sidebar shows or not, whose right pane shows or not, and which is full screen or not.
pub fn model(sidebar: bool, pane: bool, full_screen: bool) -> Vec<Item> {
    vec![
        Item { keys_first: false, ..item("neko-about", "About Neko", None, None) },
        // After About in the app menu, where Mac apps keep it.
        item("neko-settings", "Settings…", None, Some("cmd+,")),
        item("neko-new-task", "New Thread", Some("File"), Some("cmd+n")),
        Item { separator: true, ..item("neko-file-sep", "", Some("File"), None) },
        item("neko-close-window", "Close Window", Some("File"), Some("cmd+w")),
        item("neko-home", "Home", Some("View"), Some("cmd+1")),
        item("neko-needs", "Needs You", Some("View"), Some("cmd+2")),
        item("neko-rounds", "Rounds", Some("View"), Some("cmd+3")),
        item("neko-briefing", "Briefing", Some("View"), Some("cmd+4")),
        Item { separator: true, ..item("neko-view-sep", "", Some("View"), None) },
        item("neko-sidebar", if sidebar { "Hide Sidebar" } else { "Show Sidebar" }, Some("View"), Some("ctrl+cmd+s")),
        item("neko-pane", if pane { "Hide Pane" } else { "Show Pane" }, Some("View"), Some("alt+cmd+b")),
        item("neko-changes", "Changes", Some("View"), Some("shift+cmd+g")),
        Item { keys_first: false, ..item("neko-full-screen", if full_screen { "Exit Full Screen" } else { "Enter Full Screen" }, Some("View"), Some("ctrl+cmd+f")) },
        Item { keys_first: false, ..item("neko-help", "Neko Help", Some("Help"), None) },
    ]
}

fn action(id: &str) -> fn(&mut App, &mut Tx) {
    match id {
        "neko-about" => |_, _| crate::about::show(),
        "neko-settings" => |s, tx| {
            s.open_settings();
            tx.instant();
        },
        "neko-new-task" => |s, tx| crate::new_thread(s, tx),
        "neko-home" => |s, tx| {
            s.go_new_thread();
            tx.instant();
        },
        "neko-needs" => |s, tx| {
            s.open_needs();
            tx.instant();
        },
        "neko-rounds" => |s, tx| {
            s.open_rounds();
            tx.instant();
        },
        "neko-briefing" => |s, tx| {
            s.open_briefing();
            tx.instant();
        },
        "neko-close-window" => |_, tx| tx.close_window(),
        "neko-sidebar" => |s, tx| {
            s.toggle_sidebar();
            tx.instant();
        },
        "neko-pane" => |s, tx| {
            s.toggle_pane();
            tx.instant();
        },
        "neko-changes" => |s, tx| {
            s.open_changes(None);
            tx.instant();
        },
        "neko-full-screen" => |_, tx| tx.toggle_full_screen(),
        "neko-help" => |_, tx| {
            let _ = tx.open_url_then(HELP_URL, |_: &mut App, _, _| {});
        },
        _ => |_, _| {},
    }
}

/// The model as Rune's menu items.
pub fn items(model: &[Item]) -> Vec<MenuItem<App>> {
    model
        .iter()
        .map(|i| {
            if i.separator {
                return MenuItem::separator().menu(i.menu.unwrap_or("File"));
            }
            // In the app menu: About replaces AppKit's own; anything else follows it (Rune's default place).
            let mut m = if i.id == "neko-about" { MenuItem::about(i.title, action(i.id)) } else { MenuItem::new(i.title, action(i.id)) };
            m = m.id(i.id);
            if let Some(menu) = i.menu {
                m = m.menu(menu);
            }
            if let Some(key) = i.key.and_then(|k| k.parse::<Shortcut>().ok()) {
                m = m.key_equivalent(key);
            }
            if i.keys_first {
                m = m.keys_first();
            }
            m
        })
        .collect()
}

/// Raw AppKit, for what Rune does not do at this rev.
pub mod native {
    /// Before the first window: keep AppKit from adding an Enter Full Screen item of its own to the View menu.
    pub fn prepare() {
        #[cfg(target_os = "macos")]
        mac::prepare();
    }

    /// After Rune has put Neko's items in: registers the Help menu, removes Rune's Window ▸ Close (File ▸ Close Window has ⌘W) and
    /// an AppKit Enter Full Screen if one appeared. True once the Help menu is found and registered.
    pub fn fix_up() -> bool {
        #[cfg(target_os = "macos")]
        {
            mac::fix_up()
        }
        #[cfg(not(target_os = "macos"))]
        {
            true
        }
    }

    #[cfg(target_os = "macos")]
    #[allow(unsafe_code)]
    mod mac {
        use objc2::rc::Retained;
        use objc2::runtime::AnyObject;
        use objc2::{MainThreadMarker, msg_send, sel};
        use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
        use objc2_foundation::NSString;

        /// What Rune's `MenuItem::menu` names a top-level menu it made (`rune.menu.<name>`), else its title.
        fn name(top: &NSMenuItem) -> String {
            // SAFETY: NSMenuItem implements NSUserInterfaceItemIdentification; main thread.
            let id: Option<Retained<NSString>> = unsafe { msg_send![top, identifier] };
            id.and_then(|id| id.to_string().strip_prefix("rune.menu.").map(str::to_owned)).unwrap_or_else(|| top.title().to_string())
        }

        fn menu_named(app: &NSApplication, wanted: &str) -> Option<Retained<NSMenu>> {
            let root = app.mainMenu()?;
            (0..root.numberOfItems()).filter_map(|i| root.itemAtIndex(i)).find(|i| name(i) == wanted).and_then(|i| i.submenu())
        }

        /// Removes the items of `menu` with `action` that AppKit routes (no target): Rune's and AppKit's own, never Neko's (each of
        /// Neko's items has Rune's target).
        fn remove_untargeted(menu: &NSMenu, action: objc2::runtime::Sel) {
            let found: Vec<_> = (0..menu.numberOfItems()).filter_map(|i| menu.itemAtIndex(i)).filter(|i| i.action() == Some(action) && i.target().is_none()).collect();
            for item in found {
                menu.removeItem(&item);
            }
        }

        pub fn prepare() {
            // SAFETY: NSUserDefaults is thread-safe; the dictionary and its key and value are retained for the call.
            unsafe {
                let defaults: Retained<AnyObject> = msg_send![objc2::class!(NSUserDefaults), standardUserDefaults];
                let no: Retained<AnyObject> = msg_send![objc2::class!(NSNumber), numberWithBool: false];
                let key = NSString::from_str("NSFullScreenMenuItemEverywhere");
                let dict: Retained<AnyObject> = msg_send![objc2::class!(NSDictionary), dictionaryWithObject: &*no, forKey: &*key];
                let _: () = msg_send![&*defaults, registerDefaults: &*dict];
            }
        }

        pub fn fix_up() -> bool {
            let Some(mtm) = MainThreadMarker::new() else { return false };
            let app = NSApplication::sharedApplication(mtm);
            if let Some(window) = menu_named(&app, "Window") {
                remove_untargeted(&window, sel!(performClose:));
            }
            if let Some(view) = menu_named(&app, "View") {
                remove_untargeted(&view, sel!(toggleFullScreen:));
            }
            let Some(help) = menu_named(&app, "Help") else { return false };
            // SAFETY: main thread; the menu is retained by the menu bar and by `help`.
            unsafe {
                let current: Option<Retained<NSMenu>> = msg_send![&*app, helpMenu];
                if current.as_deref().is_none_or(|c| !std::ptr::eq(c, &*help)) {
                    let _: () = msg_send![&*app, setHelpMenu: &*help];
                }
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menus_hold_what_a_mac_user_expects() {
        let lines: Vec<String> = model(true, false, false).iter().filter(|i| !i.separator).map(|i| format!("{} / {} / {}", i.menu.unwrap_or("Neko"), i.title, i.key.unwrap_or("-"))).collect();
        assert_eq!(
            lines,
            ["Neko / About Neko / -", "Neko / Settings… / cmd+,", "File / New Thread / cmd+n", "File / Close Window / cmd+w", "View / Home / cmd+1", "View / Needs You / cmd+2", "View / Rounds / cmd+3", "View / Briefing / cmd+4", "View / Hide Sidebar / ctrl+cmd+s", "View / Show Pane / alt+cmd+b", "View / Changes / shift+cmd+g", "View / Enter Full Screen / ctrl+cmd+f", "Help / Neko Help / -"]
        );
        let folded = model(false, true, true);
        assert!(folded.iter().any(|i| i.title == "Show Sidebar") && folded.iter().any(|i| i.title == "Hide Pane") && folded.iter().any(|i| i.title == "Exit Full Screen"), "titles follow the window");
    }

    #[test]
    fn no_two_items_share_a_key_neither_neko_s_nor_the_system_s() {
        let mut seen: Vec<(Shortcut, String)> = Vec::new();
        let native = NATIVE.iter().filter_map(|(menu, title, key)| key.map(|k| (k, format!("{menu} ▸ {title}"))));
        let ours = model(true, false, false).into_iter().filter_map(|i| i.key.map(|k| (k, format!("{:?} ▸ {}", i.menu, i.title))));
        for (key, name) in native.chain(ours) {
            let parsed = key.parse::<Shortcut>().unwrap_or_else(|_| panic!("{key} parses"));
            if let Some((_, other)) = seen.iter().find(|(k, _)| *k == parsed) {
                panic!("{name} and {other} both take {key}");
            }
            seen.push((parsed, name));
        }
    }

    #[test]
    fn ids_are_unique() {
        let m = model(true, false, false);
        let mut ids: Vec<&str> = m.iter().map(|i| i.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), m.len());
    }
}
