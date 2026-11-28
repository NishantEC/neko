//! Settings as a page list (docs/knowledge/app-shell.md, "Settings"): eleven pages down the left, one page at a time, a search that
//! narrows the list, says what matched and marks it on the page, and every setting there was before still reachable on its page.

mod support;
use neko_app::settings_view::Page;
use neko_app::state::App;
use rune::{Key, Mods, Runtime};
use support::*;

fn scene(name: &str) -> fn(&mut App) {
    scenes().iter().find(|s| s.0 == name).unwrap_or_else(|| panic!("no scene {name}")).1
}

fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<App>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default()
}

fn focus(rt: &Runtime<App>) -> Option<String> {
    rt.presented_focus_key().map(str::to_owned)
}

/// Settings with everything a page needs (the storage scene has the agents, the projects, the cats, undo and the briefing).
fn settings() -> Runtime<App> {
    let mut rt = runtime((1200.0, 800.0), false, scene("settings-storage"));
    rt.update(|s, _| s.briefing_settings = Some(neko_protocol::BriefingSettings::default()));
    settle(&mut rt);
    rt
}

fn type_in_search(rt: &mut Runtime<App>, words: &str) {
    click(rt, "settings-search");
    press(rt, Key::Text(words.into()), Mods::default());
    settle(rt);
}

#[test]
fn the_list_has_eleven_pages_in_order_and_no_segmented_control() {
    let rt = runtime((1200.0, 800.0), false, scene("settings"));
    let rows: Vec<String> = rt.elements().into_iter().filter_map(|e| e.key).filter(|k| k.starts_with("settings-page-") && k != "settings-page-title").collect();
    let want: Vec<String> = Page::ALL.iter().map(|p| p.key()).collect();
    assert_eq!(rows, want);
    let titles: Vec<&str> = Page::ALL.iter().map(|p| p.title()).collect();
    assert_eq!(titles, ["General", "Agents & models", "Permissions", "Projects", "Cattains", "Kittens", "Rounds & wakes", "Notifications", "Plugins", "Shortcuts", "Storage"]);
    assert!(!rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("settings-page-") && k[14..].parse::<usize>().is_ok())), "no numbered segment is left");
    assert_eq!(text_of(&rt, "settings-title"), "Settings");
    assert_eq!(text_of(&rt, "settings-page-title"), "General", "Settings opens on General the first time");
}

#[test]
fn pressing_a_row_shows_its_page_alone() {
    let mut rt = settings();
    for p in Page::ALL {
        click(&mut rt, &p.key());
        settle(&mut rt);
        assert_eq!(rt.state.settings_page, p);
        assert_eq!(text_of(&rt, "settings-page-title"), p.title());
        let pages = rt.elements().into_iter().filter_map(|e| e.key).filter(|k| k.starts_with("settings-") && k.ends_with("-page") && k != "settings-page").count();
        let all: Vec<String> = rt.elements().into_iter().filter_map(|e| e.key).filter(|k| k.starts_with("settings-") && k.ends_with("-page") && k != "settings-page").collect();
        assert_eq!(pages, 1, "one page at a time ({}): {all:?}", p.title());
    }
}

#[test]
fn every_setting_there_was_is_on_its_page() {
    let mut rt = settings();
    let homes: &[(Page, &[&str])] = &[
        (Page::General, &["settings-send", "settings-grouping", "settings-show-settled", "settings-show-archived", "settings-auto-settle"]),
        (Page::Agents, &["agent-claude", "settings-runtime", "settings-model"]),
        (Page::Permissions, &["settings-mode", "settings-rules", "settings-kitten-access"]),
        (Page::Projects, &["settings-add-workspace", "settings-branches"]),
        (Page::Cattains, &["settings-new-cat", "settings-agents-make-cattains", "settings-cat-cat_miso"]),
        (Page::Kittens, &["settings-ask-first-kitten", "settings-kitten-limits"]),
        (Page::Rounds, &["settings-rounds-pause", "settings-waits", "briefing-morning-on", "briefing-morning-time"]),
        (Page::Notifications, &["notify-needs", "notify-finished", "notify-badge", "settings-notify-test"]),
        (Page::Plugins, &["own-mcp", "plugins-add-json", "plugins-add-folder"]),
        (Page::Shortcuts, &["shortcut-0", "shortcuts-threads"]),
        (Page::Storage, &["settings-clear-undo", "settings-worktrees", "settings-archived-count"]),
    ];
    for (page, keys) in homes {
        rt.update(|s, _| s.set_settings_page(*page));
        settle(&mut rt);
        for k in *keys {
            assert!(has_key(&rt, k), "{k} on {}", page.title());
        }
    }
    assert_eq!(text_of(&rt, "settings-archived-count"), "2 archived threads, kept as they were.");
}

#[test]
fn undo_narrows_the_list_to_storage_and_shortcuts_shows_storage_and_marks_undo() {
    let mut rt = settings();
    rt.update(|s, _| s.set_settings_page(Page::General));
    settle(&mut rt);
    type_in_search(&mut rt, "undo");
    assert_eq!(rt.state.settings_page, Page::Storage, "the page that fits best takes the place of one the list no longer has");
    assert!(has_key(&rt, "settings-page-storage") && has_key(&rt, "settings-page-shortcuts"));
    assert!(!has_key(&rt, "settings-page-general") && !has_key(&rt, "settings-page-plugins"));
    assert!(has_key(&rt, "settings-undo-found"), "the option is marked on the page");
    let names: Vec<String> = rt.elements().into_iter().filter(|e| e.key.as_deref() == Some("settings-page-storage")).filter_map(|e| e.description).collect();
    assert!(names.iter().any(|d| d.contains("Undo history")), "the row says what matched: {names:?}");
    // Pressing the other row shows it, with its own match marked.
    click(&mut rt, "settings-page-shortcuts");
    settle(&mut rt);
    assert_eq!(rt.state.settings_page, Page::Shortcuts);
    assert!(rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("shortcut-") && k.ends_with("-found"))));
}

#[test]
fn a_search_with_no_match_says_so_and_keeps_the_page() {
    let mut rt = settings();
    rt.update(|s, _| s.set_settings_page(Page::Kittens));
    settle(&mut rt);
    type_in_search(&mut rt, "zebra");
    assert_eq!(text_of(&rt, "settings-no-results"), "No results for \u{201c}zebra\u{201d}");
    assert!(!Page::ALL.iter().any(|p| has_key(&rt, &p.key())));
    assert_eq!(rt.state.settings_page, Page::Kittens, "the page in view stays");
    // Escape empties the field first, and every page is back.
    press(&mut rt, Key::Escape, Mods::default());
    settle(&mut rt);
    assert!(rt.state.settings_query.text.is_empty() && rt.state.settings_open);
    assert!(Page::ALL.iter().all(|p| has_key(&rt, &p.key())));
}

#[test]
fn a_cats_name_finds_the_cattains_page_and_return_opens_the_best_page() {
    let mut rt = settings();
    type_in_search(&mut rt, "miso");
    assert_eq!(rt.state.settings_page, Page::Cattains);
    assert!(has_key(&rt, "settings-cat-cat_miso-found") && !has_key(&rt, "settings-cat-cat_mochi-found"));
    rt.update(|s, _| s.set_settings_page(Page::Cattains));
    // Typing more: "badge" is on Notifications only.
    rt.update(|s, _| s.settings_query = rune::TextEdit::new("badge"));
    settle(&mut rt);
    press(&mut rt, Key::Enter, Mods::default());
    settle(&mut rt);
    assert_eq!(rt.state.settings_page, Page::Notifications);
    assert!(has_key(&rt, "notify-badge-found"));
}

#[test]
fn command_f_goes_to_the_search_and_opening_settings_empties_it() {
    let mut rt = settings();
    press(&mut rt, Key::Command('f'), Mods { cmd: true, ..Mods::default() });
    settle(&mut rt);
    assert_eq!(focus(&rt).as_deref(), Some("settings-search-input"));
    press(&mut rt, Key::Text("plug".into()), Mods::default());
    settle(&mut rt);
    assert_eq!(rt.state.settings_page, Page::Plugins);
    rt.update(|s, _| {
        s.settings_open = false;
        s.open_settings();
    });
    settle(&mut rt);
    assert!(rt.state.settings_query.text.is_empty());
    assert_eq!(rt.state.settings_page, Page::Plugins, "⌘, comes back to the page last shown");
}

#[test]
fn searchs_settings_actions_open_each_page() {
    let mut rt = runtime((1200.0, 800.0), false, scene("tasks-home"));
    for p in Page::ALL {
        let id = format!("settings-{}", p.slug());
        assert!(neko_app::search::ACTIONS.iter().any(|a| a.id == id), "Search offers {id}");
        rt.update(|s, _| {
            s.settings_open = false;
            s.palette_choose(&format!("action:{id}"), false);
        });
        assert!(rt.state.settings_open && rt.state.settings_page == p, "{id}");
    }
}
