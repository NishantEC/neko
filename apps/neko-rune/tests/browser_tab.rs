//! The Browser tab as drawn and driven (docs/knowledge/rune-live-browser.md): the two scenes, the top bar and picker, "Open in your
//! browser" (http and https only), the empty and lost states, and when the thread is asked to watch. No thread runs here: the tab
//! is staged with made-up pages and a made-up frame.

use std::sync::Mutex;

use neko_rune::app::{App, DEMO_NOW};
use neko_rune::browser::{BrowserTab, Follow, PageInfo, Status};
use neko_rune::panel::Tab;
use neko_rune::scenes::SCENES;
use neko_rune::view::view;
use neko_rune::{samples, scenes_panel};
use rune::{Env, Runtime};

const WINDOW: (f32, f32) = (1700.0, 900.0);

static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn record(url: &str) -> std::io::Result<()> {
    OPENED.lock().unwrap().push(url.to_owned());
    Ok(())
}

fn runtime(setup: impl FnOnce(&mut App)) -> Runtime<App> {
    let mut app = App::headless();
    setup(&mut app);
    let mut rt = Runtime::new(app, view, WINDOW, Env { system_dark: true, scale: 2.0 });
    rt.frame(0.0);
    frames(&mut rt, 20);
    rt
}

fn frames(rt: &mut Runtime<App>, n: usize) {
    for _ in 0..n {
        rt.frame(1.0 / 60.0);
    }
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(rt, 20);
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect()
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting).unwrap_or_else(|| panic!("no element keyed {key}")).rect
}

#[test]
fn both_scenes_exist_for_the_structure_crawl() {
    for name in ["panel-browser", "panel-browser-none"] {
        assert!(SCENES.iter().any(|(n, _)| *n == name), "{name}");
    }
}

#[test]
fn the_tab_is_fifth_in_the_order_a_request_and_never_the_default() {
    let rt = runtime(scenes_panel::browser);
    assert_eq!(rt.state.panel_tab(), Some(Tab::Browser));
    let tabs = rt.state.panel_tabs();
    let at = |t: Tab| tabs.iter().position(|x| *x == t).unwrap();
    assert!(at(Tab::Terminal) < at(Tab::Browser) && at(Tab::Browser) < at(Tab::Details), "{tabs:?}");
    assert!(!rt.state.panel_content_tabs().contains(&Tab::Browser), "it never opens the panel by itself");
    // A person who opens the panel on a session that has only this and Details lands on Details, not the browser.
    let mut app = App::headless();
    scenes_panel::browser(&mut app);
    app.panel.sessions.get_mut("t6").unwrap().tab = None;
    assert_ne!(app.panel_tab(), Some(Tab::Browser));
}

#[test]
fn the_bar_shows_the_page_and_the_frame_fits_the_panels_width_with_its_aspect() {
    let rt = runtime(scenes_panel::browser);
    let words = texts(&rt);
    assert!(words.iter().any(|w| w == "Acme docs"), "{words:?}");
    assert!(words.iter().any(|w| w == "docs.acme.test"));
    assert!(words.iter().any(|w| w == "Following the newest page"));
    let bar = rect(&rt, "browser-bar");
    assert_eq!(bar[3], 32.0, "a 32 point top bar");
    let pick = rect(&rt, "browser-pick");
    let open = rect(&rt, "browser-open");
    assert_eq!((pick[2], pick[3], open[2], open[3]), (32.0, 32.0, 32.0, 32.0));
    assert!(pick[0] < open[0] && open[0] + open[2] <= bar[0] + bar[2] - 12.0 + 0.5, "the picker, then Open, at the right edge");
    let frame = rect(&rt, "browser-frame");
    let area = rect(&rt, "browser-area");
    assert!((frame[2] - (area[2] - 24.0)).abs() < 1.0, "as wide as the panel less its padding: {frame:?} in {area:?}");
    assert!(frame[1] >= bar[1] + 32.0, "under the bar");
}

#[test]
fn the_picker_lists_follow_the_newest_then_each_page_and_choosing_a_page_stops_following_the_newest() {
    let mut rt = runtime(scenes_panel::browser);
    assert_eq!(rt.state.browser.snapshot().follow, Follow::Newest);
    click(&mut rt, "browser-pick");
    let words = texts(&rt);
    for want in ["Follow the newest page", "Acme docs — docs.acme.test", "Asha's board — board.acme.test"] {
        assert!(words.iter().any(|w| w == want), "{want}: {words:?}");
    }
    let order: Vec<usize> = ["Follow the newest page", "Acme docs — docs.acme.test", "Asha's board — board.acme.test"].iter().map(|w| words.iter().position(|x| x == w).unwrap()).collect();
    assert!(order.windows(2).all(|p| p[0] < p[1]), "the newest first");
    rt.update(|s, _| s.browser_follow(Follow::Page("pg-board".into())));
    frames(&mut rt, 20);
    let seen = rt.state.browser.snapshot();
    assert_eq!(seen.follow, Follow::Page("pg-board".into()));
    assert!(!texts(&rt).iter().any(|w| w == "Following the newest page"));
}

#[test]
fn open_in_your_browser_opens_http_and_https_only() {
    OPENED.lock().unwrap().clear();
    let mut rt = runtime(|s| {
        scenes_panel::browser(s);
        s.opener = record;
    });
    click(&mut rt, "browser-open");
    assert_eq!(OPENED.lock().unwrap().clone(), ["https://docs.acme.test/start"]);
    // A page that is not http(s) is never handed to the system.
    rt.update(|s, _| {
        let pages = vec![PageInfo { target_id: "pg-file".into(), title: "A file".into(), url: "file:///tmp/notes.html".into() }, PageInfo { target_id: "pg-js".into(), title: "Script".into(), url: "javascript:alert(1)".into() }];
        s.browser = BrowserTab::staged(Status::Connected, pages, Some("pg-file"), None);
        s.browser_open_current();
        s.browser = BrowserTab::staged(Status::Connected, vec![PageInfo { target_id: "pg-js".into(), title: "Script".into(), url: "javascript:alert(1)".into() }], Some("pg-js"), None);
        s.browser_open_current();
    });
    assert_eq!(OPENED.lock().unwrap().len(), 1, "{:?}", OPENED.lock().unwrap());
}

#[test]
fn with_no_browser_it_says_what_to_start_and_looks_again_on_request() {
    let mut rt = runtime(scenes_panel::browser_none);
    let words = texts(&rt);
    assert!(words.iter().any(|w| w == "No browser to watch"), "{words:?}");
    assert!(words.iter().any(|w| w == "Neko watches a Chrome on this Mac that was started with --remote-debugging-port (it looks at ports 9333 and 9222)."), "{words:?}");
    assert!(rt.find("browser-bar").is_none(), "no bar without a browser");
    let width = rect(&rt, "browser-none")[2];
    assert!(width <= 320.0, "{width}");
    click(&mut rt, "browser-again");
    assert_eq!(rt.state.browser.snapshot().status, Status::Looking);
}

#[test]
fn a_lost_browser_and_a_page_still_drawing_say_so() {
    let rt = runtime(|s| {
        scenes_panel::browser(s);
        s.browser = BrowserTab::staged(Status::Lost { retry_at_ms: DEMO_NOW + 1000 }, Vec::new(), None, None);
    });
    assert!(texts(&rt).iter().any(|w| w == "Lost the browser. Trying again…"));
    let rt = runtime(|s| {
        scenes_panel::browser(s);
        s.browser = BrowserTab::staged(Status::Connected, scenes_panel::browser_pages(), Some("pg-docs"), None);
    });
    assert!(texts(&rt).iter().any(|w| w == "Waiting for the page to draw…"));
    assert!(rt.find("browser-frame").is_none());
}

#[test]
fn a_page_that_closed_says_so_for_three_seconds() {
    let words = |closed_ago_ms: i64| {
        let rt = runtime(move |s| {
            scenes_panel::browser(s);
            s.clock = || DEMO_NOW;
            s.browser.view.lock().unwrap().closed_at_ms = Some(DEMO_NOW - closed_ago_ms);
        });
        texts(&rt)
    };
    assert!(words(500).iter().any(|w| w == "That page closed."));
    assert!(!words(500).iter().any(|w| w == "Following the newest page"));
    assert!(!words(3500).iter().any(|w| w == "That page closed."));
}

#[test]
fn the_tab_asks_to_watch_only_while_it_is_the_visible_tab_of_an_active_window() {
    let mut rt = runtime(scenes_panel::browser);
    assert!(rt.state.browser_on());
    rt.update(|s, _| s.focused = false);
    assert!(!rt.state.browser_on(), "an inactive window watches nothing");
    rt.update(|s, _| s.focused = true);
    rt.update(|s, _| s.panel_choose_tab(Tab::Details));
    assert!(!rt.state.browser_on(), "another tab is on screen");
    rt.update(|s, _| s.panel_choose_tab(Tab::Browser));
    assert!(rt.state.browser_on());
    rt.update(|s, _| s.panel_set_open(false));
    assert!(!rt.state.browser_on(), "a closed panel");
}

#[test]
fn the_picture_is_let_go_when_the_tab_leaves_the_screen() {
    let mut rt = runtime(|s| {
        scenes_panel::browser(s);
        assert!(s.browser.size.is_some());
    });
    rt.update(|s, _| s.panel_choose_tab(Tab::Details));
    rt.update(|s, tx| {
        s.browser_tick(0.0, tx);
    });
    assert!(rt.state.browser.size.is_none(), "no frame is kept for a tab nobody sees");
    // The synthetic frame is generated, never a capture: it is a gradient with shapes.
    let px = samples::browser_frame(64, 40);
    assert_eq!(px.len(), 64 * 40 * 4);
    assert_ne!(px[..4], px[px.len() - 4..], "not a flat colour");
}
