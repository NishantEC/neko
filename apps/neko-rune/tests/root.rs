//! The one view driven through Rune's headless `Runtime`: real layout, real hit-testing, no
//! window. Clicks go where a person's would, so this checks the app as used.

use neko_rune::app::{App, LinkState, down};
use neko_rune::link::{LinkError, LinkEvent};
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

fn keyed(rt: &Runtime<App>, prefix: &str) -> usize {
    rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with(prefix))).count()
}

#[test]
fn before_the_daemon_answers_it_says_connecting_and_shows_no_rows() {
    let rt = runtime(true, |_| {});
    assert!(texts(&rt).contains(&"Connecting to Neko…".to_owned()));
    assert_eq!(keyed(&rt, "task-"), 0);
}

#[test]
fn live_tasks_show_as_rows_with_needs_you_first() {
    let rt = runtime(true, |a| {
        a.apply_link(LinkEvent::Live(scenes::demo_summary()));
    });
    assert_eq!(keyed(&rt, "task-"), neko_rune::app::FEATURED, "a few, not all 25");
    let t = texts(&rt);
    assert!(t.contains(&"Needs you".to_owned()), "the first rows are the ones that need you: {t:?}");
    assert!(t.contains(&"Connected · 25 tasks · 2 need you".to_owned()), "{t:?}");
    assert!(keyed(&rt, "status-") >= neko_rune::app::FEATURED, "a glyph on every row");
}

#[test]
fn a_drop_keeps_the_rows_and_says_so_calmly() {
    let mut rt = runtime(true, |a| {
        a.apply_link(LinkEvent::Live(scenes::demo_summary()));
    });
    // As the link thread does it: through an update, which makes the next frame rebuild.
    rt.update(|s, _| {
        s.apply_link(down(LinkError::NotRunning, 4));
    });
    rt.frame(1.0 / 60.0);
    assert!(matches!(rt.state.link, LinkState::Down { .. }));
    let t = texts(&rt);
    assert!(t.iter().any(|l| l.starts_with("Neko isn't running. Trying again in 4 s. Last known: 25 tasks")), "{t:?}");
    assert!(keyed(&rt, "task-") > 0, "what was known stays on screen");
}

#[test]
fn an_empty_workbench_says_no_tasks_once() {
    let (_, scene) = scenes::SCENES.iter().find(|(n, _)| *n == "empty").unwrap();
    let rt = runtime(false, scene);
    assert_eq!(texts(&rt).iter().filter(|t| *t == "No tasks").count(), 1);
}

#[test]
fn the_web_button_opens_the_web_ui_and_without_a_daemon_says_why() {
    thread_local! { static OPENED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) }; }
    let mut rt = runtime(true, |a| {
        a.opener = |url| {
            OPENED.with(|o| o.borrow_mut().push(url.to_owned()));
            Ok(())
        };
    });
    click(&mut rt, "open-web");
    assert!(OPENED.with(|o| o.borrow().is_empty()), "nothing opens without an address");
    assert!(texts(&rt).iter().any(|t| t == "Neko isn't running, so there's no web UI to open yet."), "{:?}", texts(&rt));

    rt.state.set_web(Some(WebTarget::new(4747, &"ab".repeat(32))));
    rt.frame(1.0 / 60.0);
    click(&mut rt, "open-web");
    let opened = OPENED.with(|o| o.borrow().clone());
    assert_eq!(opened.len(), 1);
    assert!(opened[0].starts_with("http://127.0.0.1:4747/?token="));
    assert!(!texts(&rt).iter().any(|t| t.contains(&"ab".repeat(32))), "the token is never on screen");
}

#[test]
fn there_are_no_tabs_no_rail_and_no_pages() {
    let rt = runtime(true, |_| {});
    for e in rt.elements() {
        let key = e.key.as_deref().unwrap_or("");
        assert!(!key.starts_with("rail") && key != "tabs" && key != "new-tab" && !key.starts_with("page-home"), "the layout is pending an owner decision: {key}");
    }
}

#[test]
fn every_scene_lays_out_in_both_appearances() {
    for dark in [true, false] {
        for (name, scene) in scenes::SCENES {
            let rt = runtime(dark, scene);
            assert!(rt.find("page-scroll").is_some(), "{name} dark={dark}");
            assert!(rt.find("status-text").is_some(), "{name} dark={dark}");
        }
    }
}

#[test]
fn light_and_dark_follow_the_system() {
    let fill = |rt: &Runtime<App>| rt.elements().into_iter().find(|e| e.key.as_deref() == Some("window")).map(|e| e.fill.hex()).unwrap();
    assert_eq!(fill(&runtime(true, |_| {})), "#0f0f0fff");
    assert_eq!(fill(&runtime(false, |_| {})), "#fafafaff");
}
