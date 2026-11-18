//! The application menu: Swift's `.commands` in `NekoApp.swift` through Rune's `native_menu` (`AppHandle::set_menu`).
//!
//! | Swift | Here |
//! | --- | --- |
//! | `CommandGroup(after: .newItem)`: Quick panel ⌘K | File ▸ Quick panel ⌘K |
//! | `CommandGroup(replacing: .appSettings)`: Settings… ⌘, | the app menu's Settings… ⌘, |
//! | View ▸ Appearance picker (System, Light, Dark) | View ▸ "Appearance: System / Light / Dark", the current one checked (Rune's menus have no submenus) |
//! | `CommandMenu("Work")`: Stop all work ⌘⇧Esc, Clear finished tickets | Work ▸ the same two |
//!
//! Stop all work and Clear finished are destructive: they ask first, through the window's own confirmation (`ask_stop_all`,
//! `ask_clear_finished`), as the web does. The model ([`model`]) is pure and compared with what was last set, so the menu is only
//! replaced when something in it changed (the appearance).

use rune::{MenuItem, Shortcut, Tx};

use crate::app::App;

/// What an item does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    QuickPanel,
    Settings,
    Appearance(&'static str),
    StopAll,
    ClearFinished,
}

/// One item of the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: &'static str,
    pub title: String,
    /// The top-level menu it goes in; None is the application menu.
    pub menu: Option<&'static str>,
    pub key: Option<&'static str>,
    pub checked: bool,
    pub act: Act,
    /// The window's own key handling sees the chord first (⌘K and ⌘⇧Esc are handled there already).
    pub keys_first: bool,
}

/// The appearance modes, as the daemon keeps them and Swift's `NekoAppearance` titles them.
pub const APPEARANCES: [(&str, &str); 3] = [("system", "System"), ("light", "Light"), ("dark", "Dark")];

/// `mode` is the daemon's saved appearance (`None` until it has answered: System).
pub fn model(mode: Option<&str>) -> Vec<Item> {
    let current = mode.filter(|m| APPEARANCES.iter().any(|(k, _)| k == m)).unwrap_or("system");
    let mut out = vec![
        Item { id: "neko-quick-panel", title: "Quick panel".into(), menu: Some("File"), key: Some("cmd+k"), checked: false, act: Act::QuickPanel, keys_first: true },
        Item { id: "neko-settings", title: "Settings…".into(), menu: None, key: Some("cmd+comma"), checked: false, act: Act::Settings, keys_first: false },
    ];
    for (key, title) in APPEARANCES {
        out.push(Item { id: key_id(key), title: format!("Appearance: {title}"), menu: Some("View"), key: None, checked: key == current, act: Act::Appearance(key), keys_first: false });
    }
    out.push(Item { id: "neko-stop-all", title: "Stop all work".into(), menu: Some("Work"), key: Some("cmd+shift+escape"), checked: false, act: Act::StopAll, keys_first: true });
    out.push(Item { id: "neko-clear-finished", title: "Clear finished tickets".into(), menu: Some("Work"), key: None, checked: false, act: Act::ClearFinished, keys_first: false });
    out
}

fn key_id(mode: &str) -> &'static str {
    match mode {
        "light" => "neko-appearance-light",
        "dark" => "neko-appearance-dark",
        _ => "neko-appearance-system",
    }
}

fn run(a: Act) -> fn(&mut App, &mut Tx) {
    match a {
        Act::QuickPanel => |s, tx| {
            s.quick_panel();
            tx.instant();
        },
        Act::Settings => |s, tx| {
            s.open_neko(Some("settings"));
            tx.instant();
        },
        Act::Appearance("light") => |s, tx| {
            s.set_appearance("light");
            tx.instant();
        },
        Act::Appearance("dark") => |s, tx| {
            s.set_appearance("dark");
            tx.instant();
        },
        Act::Appearance(_) => |s, tx| {
            s.set_appearance("system");
            tx.instant();
        },
        Act::StopAll => |s, tx| {
            s.stop_all_key();
            tx.instant();
        },
        Act::ClearFinished => |s, tx| {
            s.ask_clear_finished();
            if let Some(h) = &s.shell.handle {
                h.open();
            }
            tx.instant();
        },
    }
}

/// The model as Rune's menu items.
pub fn items(model: &[Item]) -> Vec<MenuItem<App>> {
    model
        .iter()
        .map(|i| {
            let mut item = MenuItem::new(i.title.clone(), run(i.act)).id(i.id).checked(i.checked);
            if let Some(menu) = i.menu {
                item = item.menu(menu);
            }
            if let Some(key) = i.key.and_then(|k| k.parse::<Shortcut>().ok()) {
                item = item.key_equivalent(key);
            }
            if i.keys_first {
                item = item.keys_first();
            }
            item
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_has_swifts_items_keys_and_menus() {
        let m = model(None);
        let line = |i: &Item| format!("{} / {} / {}", i.menu.unwrap_or("app"), i.title, i.key.unwrap_or("-"));
        assert_eq!(
            m.iter().map(line).collect::<Vec<_>>(),
            [
                "File / Quick panel / cmd+k",
                "app / Settings… / cmd+comma",
                "View / Appearance: System / -",
                "View / Appearance: Light / -",
                "View / Appearance: Dark / -",
                "Work / Stop all work / cmd+shift+escape",
                "Work / Clear finished tickets / -",
            ]
        );
        let mut ids: Vec<&str> = m.iter().map(|i| i.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), m.len(), "Rune refuses duplicate ids");
    }

    #[test]
    fn every_key_equivalent_parses_and_the_items_build() {
        for i in model(None) {
            if let Some(k) = i.key {
                assert!(k.parse::<Shortcut>().is_ok(), "{k}");
            }
        }
        assert_eq!(items(&model(None)).len(), 7);
    }

    #[test]
    fn the_current_appearance_is_the_one_checked() {
        let checked = |mode: Option<&str>| model(mode).into_iter().filter(|i| i.checked).map(|i| i.title).collect::<Vec<_>>();
        assert_eq!(checked(None), ["Appearance: System"]);
        assert_eq!(checked(Some("dark")), ["Appearance: Dark"]);
        assert_eq!(checked(Some("light")), ["Appearance: Light"]);
        assert_eq!(checked(Some("nonsense")), ["Appearance: System"]);
        assert_ne!(model(Some("dark")), model(Some("light")), "a change is noticed, so the menu is replaced");
    }
}
