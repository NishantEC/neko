//! The prototype's look (docs/knowledge/rune-look-2026-11.md), checked on the laid-out window with the prototype's own placeholder data
//! (`scenes_look.rs`): the shell is flat (a sidebar on the window colour, the content flush with one hairline, no Neko column, no art, no
//! glass), Home and the cat page have the prototype's parts, and the words are Neko's. The pixel comparison with the prototype's pictures is
//! `scripts/look-pairs.sh` (it needs the prototype folder, which is not in this repository).

use neko_rune::app::App;
use neko_rune::scenes;
use neko_rune::view::view;
use rune::{Env, Runtime};

const WINDOW: (f32, f32) = (1440.0, 900.0);

fn runtime(scene: &str, dark: bool) -> Runtime<App> {
    let (_, f) = scenes::SCENES.iter().find(|(n, _)| *n == scene).unwrap_or_else(|| panic!("no scene {scene}"));
    let mut app = App::headless();
    f(&mut app);
    let mut rt = Runtime::new(app, view, WINDOW, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    rt
}

fn info(rt: &Runtime<App>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    info(rt, key).unwrap_or_else(|| panic!("no element keyed {key}")).rect
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect()
}

#[test]
fn the_shell_is_flat_the_sidebar_on_the_window_colour_and_the_content_flush_with_no_neko_column() {
    for dark in [true, false] {
        let rt = runtime("look-home", dark);
        let t = neko_rune::theme::theme(dark);
        let (side, main) = (rect(&rt, "sessions"), rect(&rt, "session-region"));
        assert_eq!((side[0], side[1], side[2], side[3]), (0.0, 0.0, 248.0, 900.0), "the sidebar fills its column: no card, no gutter");
        assert_eq!((main[0], main[1], main[3]), (248.0, 0.0, 900.0), "the content is flush with the sidebar and the window's edges");
        assert_eq!(main[0] + main[2], WINDOW.0, "and with the right edge");
        assert_eq!(info(&rt, "window").unwrap().fill.hex(), t.surface.sidebar.hex(), "the window is the chrome colour");
        assert_eq!(info(&rt, "session-region").unwrap().fill.hex(), t.surface.window.hex(), "the content is the surface colour");
        for key in ["neko", "neko-title", "composer-glass", "home-art", "home-scrim"] {
            assert!(info(&rt, key).is_none(), "{key}: the prototype's shell has no Neko column, art or glass");
        }
    }
}

#[test]
fn home_has_the_prototypes_parts_in_the_prototypes_order() {
    let rt = runtime("look-home", true);
    let t = texts(&rt);
    for words in ["Good evening, Asha", "1 working", "1 needs you", "$6.20 today", "Briefing", "Let Neko choose", "Claude Sonnet", "Needs you", "Ready to merge", "Done", "3 merged"] {
        assert!(t.iter().any(|x| x == words), "{words}: {t:?}");
    }
    let y = |key: &str| rect(&rt, key)[1];
    let order = ["home-title", "home-status", "home-briefing", "home-composer", "home-cats", "home-sec-needs", "home-sec-ready", "home-sec-done"];
    for pair in order.windows(2) {
        assert!(y(pair[0]) < y(pair[1]), "{} is above {}", pair[0], pair[1]);
    }
    // Three cats in a row, each 62 high; the question waits with its time; the pull request has its Merge button.
    let cards: Vec<_> = ["c1", "c2", "c3"].iter().map(|c| rect(&rt, &format!("home-cat-{c}"))).collect();
    assert!(cards.windows(2).all(|w| w[0][0] < w[1][0] && w[0][1] == w[1][1]) && cards.iter().all(|c| c[3] == 62.0), "{cards:?}");
    assert!(info(&rt, "home-need-task-s2").is_some() && info(&rt, "home-open-s2").is_some() && info(&rt, "home-merge-s1").is_some());
    assert_eq!(["d1", "d2", "d3"].iter().filter(|id| info(&rt, &format!("home-done-{id}")).is_some()).count(), 3);
    // The words are Neko's: no session, routine, captain, crew or thread in what is shown.
    for word in t.iter().flat_map(|x| x.split_whitespace()).map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()) {
        assert!(!["session", "sessions", "routine", "routines", "captain", "crew", "thread"].contains(&word.as_str()), "a word Neko does not use: {word}");
    }
}

#[test]
fn the_sidebar_names_its_rows_in_neko_s_words() {
    let rt = runtime("look-home", true);
    for (key, label) in [("view-home", "Home"), ("sessions-search", "Search"), ("view-calendar", "Calendar"), ("view-all", "Tasks"), ("view-inbox", "Rounds"), ("view-settings", "Settings")] {
        assert_eq!(info(&rt, key).and_then(|e| e.label).as_deref(), Some(label), "{key}");
    }
    // The open cat's tasks nest under it; the amber dot is on the cat that has something waiting.
    assert!(info(&rt, "cat-head-c1").is_some() && info(&rt, "session-s1").is_some() && info(&rt, "session-d3").is_some());
    assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("badge-needs")), "mochi's face carries the amber dot");
    assert!(texts(&rt).iter().any(|x| x == "$6.20 today"), "the footer says what today has cost");
}

#[test]
fn the_cat_page_is_the_prototypes_bar_feed_dock_and_372_point_panel() {
    for dark in [true, false] {
        let rt = runtime("look-cat", dark);
        let panel = rect(&rt, "context");
        assert_eq!((panel[2], panel[0] + panel[2]), (372.0, WINDOW.0), "the panel is 372 wide at the window's right edge");
        assert_eq!(rect(&rt, "cat-head")[3], 48.0, "the bar is 48 high");
        assert_eq!(rect(&rt, "cat-panel-head")[3], 48.0, "and so is the panel's head");
        let t = neko_rune::theme::theme(dark);
        assert_eq!(info(&rt, "context").unwrap().fill.hex(), t.surface.sidebar.hex(), "the panel is on the chrome colour");
        for i in 0..3 {
            assert!(info(&rt, &format!("cat-tabs-{i}")).is_some(), "tab {i}");
        }
        let tabs = texts(&rt);
        for words in ["Overview", "Files", "Rounds", "Briefing · 17:40", "Tell miso what to work on…", "Try an example request", "Needs you", "Ready to merge"] {
            assert!(tabs.iter().any(|x| x == words), "{words}: {tabs:?}");
        }
        assert!(rect(&rt, "cat-dock")[1] > rect(&rt, "cat-conversation")[1], "the composer is under the feed");
    }
}
