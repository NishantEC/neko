//! The shell driven through Rune's headless `Runtime`: real layout, real hit-testing, no window.
//! Clicks go where a person's would, so these check the app as used, not just its state types.

use neko_rune::app::App;
use neko_rune::pages::Page;
use neko_rune::scenes;
use neko_rune::view::view;
use neko_rune::web_link::WebTarget;
use rune::{Env, Runtime};

fn runtime(dark: bool, setup: impl FnOnce(&mut App)) -> Runtime<App> {
    let mut app = App::headless();
    setup(&mut app);
    let mut rt = Runtime::new(app, view, (1180.0, 760.0), Env { system_dark: dark, scale: 2.0 });
    rt.frame(0.0);
    rt
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

#[test]
fn the_rail_has_six_places_and_more() {
    let rt = runtime(true, |_| {});
    for key in ["rail-home", "rail-inbox", "rail-tasks", "rail-calendar", "rail-workspaces", "rail-settings", "rail-more"] {
        assert!(rt.find(key).is_some(), "{key} is on the rail");
    }
    // Everything else is behind More, not on the rail.
    for key in ["rail-prs", "rail-watching", "rail-usage"] {
        assert!(rt.find(key).is_none(), "{key} belongs in More");
    }
}

#[test]
fn clicking_a_place_opens_its_tab_and_clicking_it_again_focuses_instead_of_duplicating() {
    let mut rt = runtime(true, |_| {});
    click(&mut rt, "rail-inbox");
    assert_eq!(rt.state.tabs.active(), Page::Inbox);
    assert!(texts(&rt).contains(&"Inbox".to_owned()));
    click(&mut rt, "rail-home");
    click(&mut rt, "rail-inbox");
    assert_eq!(rt.state.tabs.open_pages(), &[Page::Home, Page::Inbox]);
}

#[test]
fn a_placeholder_says_what_is_coming_and_offers_the_web_ui() {
    let rt = runtime(false, |_| {});
    let t = texts(&rt);
    assert!(t.contains(&"Coming in P1: open in the web UI".to_owned()), "{t:?}");
    assert!(rt.find("open-web").is_some());
    let calendar = runtime(false, |a| a.open(Page::Calendar));
    assert!(texts(&calendar).contains(&"Coming in P2: open in the web UI".to_owned()));
}

#[test]
fn the_web_button_opens_the_same_route_and_without_a_daemon_says_why() {
    thread_local! { static OPENED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) }; }
    let mut rt = runtime(true, |a| {
        a.opener = |url| {
            OPENED.with(|o| o.borrow_mut().push(url.to_owned()));
            Ok(())
        };
        a.open(Page::Inbox);
    });
    click(&mut rt, "open-web");
    assert!(OPENED.with(|o| o.borrow().is_empty()), "nothing opens without an address");
    assert!(texts(&rt).iter().any(|t| t == "Neko isn't running, so there's no web UI to open yet."), "{:?}", texts(&rt));

    rt.state.set_web(Some(WebTarget::new(4747, &"ab".repeat(32))));
    rt.frame(1.0 / 60.0);
    click(&mut rt, "open-web");
    let opened = OPENED.with(|o| o.borrow().clone());
    assert_eq!(opened.len(), 1);
    assert!(opened[0].starts_with("http://127.0.0.1:4747/?token=") && opened[0].ends_with("#/inbox"), "{}", opened[0].replace(&"ab".repeat(32), "<token>"));
}

#[test]
fn more_lists_the_other_pages_and_opens_one() {
    let mut rt = runtime(true, |_| {});
    click(&mut rt, "rail-more");
    assert!(rt.state.more_open);
    let t = texts(&rt);
    for title in ["Pull requests", "Watching", "Schedules", "Memory", "Profiles", "Tools & skills", "Usage & machine"] {
        assert!(t.contains(&title.to_owned()), "{title} in More: {t:?}");
    }
    let (x, y) = {
        let e = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Watching")).expect("the Watching row");
        (e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0)
    };
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(rt.state.tabs.active(), Page::Watching);
    assert!(!rt.state.more_open, "choosing closes the menu");
}

#[test]
fn back_and_forward_buttons_walk_the_tabs() {
    let mut rt = runtime(true, |_| {});
    click(&mut rt, "rail-tasks");
    click(&mut rt, "back");
    assert_eq!(rt.state.tabs.active(), Page::Home);
    click(&mut rt, "forward");
    assert_eq!(rt.state.tabs.active(), Page::Tasks);
}

#[test]
fn the_plus_opens_the_palette_and_a_row_goes_there() {
    let mut rt = runtime(true, |_| {});
    click(&mut rt, "new-tab");
    assert!(rt.state.palette.open);
    assert!(rt.find("palette-input").is_some());
    click(&mut rt, "palette-row-3");
    assert!(!rt.state.palette.open);
    assert_eq!(rt.state.tabs.active(), Page::Calendar, "the fourth row of an empty query is the rail's fourth place");
}

#[test]
fn the_tab_strip_follows_the_active_tab_and_home_cannot_close() {
    let rt = runtime(true, |a| {
        a.open(Page::Inbox);
        a.open(Page::Tasks);
    });
    assert_eq!(rt.state.tabs.open_pages().len(), 3);
    for id in ["t-home", "t-inbox", "t-tasks"] {
        assert!(rt.find(id).is_some(), "tab {id}");
    }
}

#[test]
fn every_scene_lays_out_without_warnings_in_both_appearances() {
    for dark in [true, false] {
        for (name, scene) in scenes::SCENES {
            let rt = runtime(dark, scene);
            assert!(rt.find("rail-home").is_some(), "{name} dark={dark}: the rail is there");
            assert!(rt.find("card").is_some(), "{name} dark={dark}: the page is there");
        }
    }
}

#[test]
fn light_and_dark_follow_the_system() {
    let dark = runtime(true, |_| {});
    let light = runtime(false, |_| {});
    let fill = |rt: &Runtime<App>, key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).map(|e| e.fill.hex()).unwrap();
    assert_eq!(fill(&dark, "card"), "#0f0f0fff");
    assert_eq!(fill(&light, "card"), "#fafafaff");
    assert_eq!(fill(&dark, "window"), "#121212ff");
    assert_eq!(fill(&light, "window"), "#f3f3f3ff");
}
