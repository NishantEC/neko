//! The four-region shell driven through Rune's headless `Runtime`: real layout, real hit-testing,
//! no window. Clicks and drags go where a person's would, so this checks the app as used.

use neko_rune::app::{App, Steer, down};
use neko_rune::layout::{MIDDLE_MIN, Region};
use neko_rune::link::{LinkError, LinkEvent};
use neko_rune::scenes;
use neko_rune::view::view;
use neko_rune::views::FullView;
use neko_rune::web_link::WebTarget;
use rune::{Env, Runtime};

const WINDOW: (f32, f32) = (1280.0, 800.0);

fn runtime(dark: bool, setup: impl FnOnce(&mut App)) -> Runtime<App> {
    let mut app = App::headless();
    setup(&mut app);
    let mut rt = Runtime::new(app, view, WINDOW, Env { system_dark: dark, scale: 2.0 });
    rt.frame(0.0);
    rt
}

fn live(app: &mut App) {
    let (_, scene) = scenes::SCENES.iter().find(|(n, _)| *n == "live").unwrap();
    scene(app);
}

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

fn keyed(rt: &Runtime<App>, prefix: &str) -> usize {
    rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with(prefix))).count()
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element keyed {key}")).rect
}

#[test]
fn the_four_regions_stand_left_to_right_and_there_are_no_tabs() {
    let rt = runtime(true, live);
    let (sessions, session, neko) = (rect(&rt, "sessions"), rect(&rt, "session-region"), rect(&rt, "neko"));
    assert!(sessions[0] < session[0] && session[0] < neko[0], "sessions, session, Neko");
    assert!(rt.find("context").is_none(), "the context panel is closed until a session has something");
    for e in rt.elements() {
        let key = e.key.as_deref().unwrap_or("");
        assert!(!key.starts_with("rail") && key != "tabs" && key != "new-tab" && !key.starts_with("t-"), "no tabs, no rail: {key}");
    }
    // The decided defaults: 300 for sessions (titles show about thirty letters), 340 for Neko, the rest for the session.
    assert_eq!(sessions[2], 300.0);
    assert_eq!(neko[2], 340.0);
    assert!(session[2] > 600.0, "{}", session[2]);
    assert!(session[2] > MIDDLE_MIN);
}

#[test]
fn sessions_are_grouped_by_state_with_counts_and_the_first_one_that_needs_you_is_open() {
    let rt = runtime(true, live);
    let t = texts(&rt);
    for title in ["Needs you", "Running", "Done today"] {
        assert!(t.contains(&title.to_owned()), "{title}: {t:?}");
    }
    // 2 awaiting approval + 3 ready for review + 1 failed; 3 + 1 + 1 + 4 queued; 6 done + 1 cancelled.
    assert_eq!(keyed(&rt, "session-t"), 22, "every task has a row");
    assert_eq!(keyed(&rt, "group-"), 3);
    assert!(rt.state.selected.is_some());
    assert_eq!(rt.state.session().map(|t| t.status), Some(neko_protocol::workbench::TaskStatus::AwaitingApproval));
}

#[test]
fn a_row_is_one_line_with_a_glyph_a_faint_id_a_title_and_quiet_meta() {
    let rt = runtime(true, live);
    let t = texts(&rt);
    assert!(t.iter().any(|x| x.starts_with("NK-1")), "ids: {t:?}");
    assert!(t.iter().any(|x| x == "Add a retry to the sync job"));
    assert!(t.iter().any(|x| x.ends_with('m')), "quiet meta in minutes");
    assert!(keyed(&rt, "status-") >= 20, "a glyph on every row");
    let row = rect(&rt, "session-t0");
    assert_eq!(row[3], 32.0, "one line");
}

#[test]
fn clicking_a_session_selects_it_and_the_top_line_names_it() {
    let mut rt = runtime(true, live);
    click(&mut rt, "session-t6");
    assert_eq!(rt.state.selected.as_deref(), Some("t6"));
    assert!(rt.state.outbox.contains(&neko_rune::link::Call::Refresh), "choosing a session asks the daemon for its events");
    // The daemon's answer carries the open session in full.
    rt.update(|s, _| {
        s.apply_link(LinkEvent::Live(scenes::demo_full("t6")));
    });
    settle(&mut rt);
    let t = texts(&rt);
    let title = rt.state.session().unwrap().title.clone();
    assert!(t.contains(&title));
    let meta = rect(&rt, "top-meta");
    assert!(meta[2] > 0.0);
    let line = t.iter().find(|x| x.starts_with("Codex · default model")).unwrap_or_else(|| panic!("runtime and goal line in the top line: {t:?}"));
    assert_eq!(line, "Codex · default model · the board shows a message when it is empty", "only runtime, model and goal line: no status, no repeated title");
    assert!(!line.contains(&title) && !line.contains("Needs you") && !line.contains("In progress"));
}

#[test]
fn groups_collapse_and_expand_with_their_count_kept() {
    let mut rt = runtime(true, live);
    let before = keyed(&rt, "session-t");
    click(&mut rt, "group-1");
    assert!(rt.state.collapsed[1]);
    assert_eq!(keyed(&rt, "session-t"), before - 9, "the nine running rows are hidden");
    assert!(texts(&rt).contains(&"Running".to_owned()), "the heading stays");
    click(&mut rt, "group-1");
    assert_eq!(keyed(&rt, "session-t"), before);
}

#[test]
fn the_foot_opens_full_views_in_the_sessions_place_and_a_session_brings_it_back() {
    let mut rt = runtime(true, live);
    for key in ["view-calendar", "view-inbox", "view-settings", "more", "new-session"] {
        assert!(rt.find(key).is_some(), "{key} is at the foot of the sessions column");
    }
    click(&mut rt, "view-settings");
    assert_eq!(rt.state.view, Some(FullView::Settings));
    assert!(rt.find("full-settings").is_some() && rt.find("composer").is_none(), "the view replaces the session and its composer");
    assert!(rt.find("settings-search").is_some() && texts(&rt).contains(&"AI and agents".to_owned()), "Settings is its own page now");
    click(&mut rt, "session-t0");
    assert_eq!(rt.state.view, None);
    assert!(rt.find("composer").is_some());
}

#[test]
fn more_lists_the_other_views_and_opens_one() {
    let mut rt = runtime(true, live);
    click(&mut rt, "more");
    assert!(rt.state.more_open);
    let t = texts(&rt);
    for title in ["Usage", "Workspaces", "Plugins", "Open the web UI"] {
        assert!(t.contains(&title.to_owned()), "{title} in More: {t:?}");
    }
    let e = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Usage")).unwrap();
    let (x, y) = (e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.view, Some(FullView::Usage));
    assert!(!rt.state.more_open);
}

#[test]
fn the_context_panel_toggles_from_the_top_line_and_says_when_there_is_nothing() {
    let mut rt = runtime(true, live);
    // A session that has produced nothing (the open one has a command, which is a tab of its own).
    rt.update(|s, _| s.select("t10"));
    settle(&mut rt);
    click(&mut rt, "context-toggle");
    assert!(rt.state.layout.context.is_open());
    assert!(rt.find("context").is_some() && rt.find("context-expand").is_some() && rt.find("context-close").is_some());
    // In a 1280 window the session keeps its minimum: the panel narrows to its own minimum and Neko folds to its icon.
    assert_eq!(rect(&rt, "context")[2], 320.0);
    // The session has produced nothing: no tabs, and one calm line.
    assert!(rt.find("context-picker").is_none(), "only tabs that have content show");
    assert!(texts(&rt).contains(&"Nothing to show yet.".to_owned()), "{:?}", texts(&rt));
    click(&mut rt, "context-close");
    assert!(!rt.state.layout.context.is_open());
    assert!(rt.find("context").is_none());
}

#[test]
fn the_composer_has_a_steer_menu_and_send_says_what_it_will_do() {
    let mut rt = runtime(true, live);
    assert!(rt.find("composer-input").is_some() && rt.find("steer-trigger").is_some());
    assert_eq!(rt.state.steer, Steer::AfterTurn);
    click(&mut rt, "steer-trigger");
    assert!(rt.state.steer_open);
    let t = texts(&rt);
    for mode in Steer::ALL {
        assert!(t.contains(&mode.label().to_owned()), "{}", mode.label());
    }
    let e = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Interrupt and restart")).unwrap();
    let (x, y) = (e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.steer, Steer::Interrupt);
    let send = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("send")).unwrap();
    assert_eq!(send.label.as_deref(), Some("Send, as a reply"), "a task that is not in a turn takes a reply, whatever the mode");
}

#[test]
fn neko_shows_the_crew_the_inbox_and_collapses_to_an_icon_and_back() {
    let mut rt = runtime(true, live);
    let t = texts(&rt);
    assert!(t.contains(&"5 agents working · 6 need you".to_owned()), "5 working (3 building, 1 planning, 1 reviewing), 6 waiting: {t:?}");
    assert!(t.contains(&"Inbox".to_owned()) && t.contains(&"Digest".to_owned()));
    // The Neko column holds what the sessions column does not: routine and goal items with their
    // defaults, never the tasks that need you (those are the sessions column's first group).
    assert!(rt.find("inbox-3").is_some() && rt.find("inbox-use-3").is_some(), "an item with its default and a button");
    assert!(rt.find("inbox-t0").is_none() && rt.find("inbox-t1").is_none(), "the Needs you tasks are not repeated here");
    assert!(rt.find("inbox-use-1").is_none(), "no default, no button");
    assert!(rt.find("inbox-use-2").is_none(), "a goal's item has the goal's own actions in its menu, not the routines' default button");
    assert!(!t.iter().any(|x| x.starts_with("default:")), "the default is on the button and in the menu, not a line of its own: {t:?}");
    assert!(rt.find("ask").is_some());
    click(&mut rt, "inbox-use-3");
    assert_eq!(rt.state.outbox.last(), Some(&neko_rune::link::Call::AnswerNeedsYou { seq: 3 }), "Use the default asks the daemon the way the Inbox does");
    rt.update(|s, _| s.neko.inbox.truncate(1));
    settle(&mut rt);
    click(&mut rt, "handoff-t6");
    assert_eq!(rt.state.selected.as_deref(), Some("t6"), "a handoff line opens its session");

    click(&mut rt, "neko-collapse");
    assert!(!rt.state.layout.neko.is_open());
    assert!((rect(&rt, "neko")[2] - 44.0).abs() < 1.0, "collapsed to an icon");
    assert!(rt.find("neko-title").is_none());
    assert!(rt.elements().iter().any(|e| e.label.as_deref() == Some("Neko, 6 need you")), "the icon carries the count");
    click(&mut rt, "neko-expand");
    assert!((rect(&rt, "neko")[2] - 340.0).abs() < 1.0);
}

#[test]
fn dragging_an_edge_resizes_its_region_within_the_range() {
    let mut rt = runtime(true, live);
    let (x, y) = rt.find("grip-sessions").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_move(x + 60.0, y);
    rt.pointer_up(x + 60.0, y);
    settle(&mut rt);
    assert_eq!(rt.state.layout.sessions.width(), 360.0);
    assert_eq!(rect(&rt, "sessions")[2], 360.0);

    let (x, y) = rt.find("grip-neko").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_move(x - 80.0, y);
    rt.pointer_up(x - 80.0, y);
    settle(&mut rt);
    assert_eq!(rt.state.layout.neko.width(), 420.0, "dragging a right-hand edge left widens it");
    assert!(rt.state.layout.middle(WINDOW.0) >= MIDDLE_MIN);
    rt.update(|s, _| s.layout.drag(Region::Sessions, 360.0, 900.0));
    settle(&mut rt);
    assert_eq!(rt.state.layout.sessions.width(), 420.0);
}

#[test]
fn before_and_without_a_daemon_it_says_so_calmly_in_the_column() {
    let rt = runtime(true, |_| {});
    assert!(texts(&rt).iter().filter(|t| *t == "Connecting to Neko…").count() >= 1);
    assert_eq!(keyed(&rt, "session-t"), 0);
    let mut rt = runtime(true, live);
    rt.update(|s, _| {
        s.apply_link(down(LinkError::NotRunning, 4));
    });
    settle(&mut rt);
    let t = texts(&rt);
    assert!(t.iter().any(|l| l.starts_with("Neko isn't running. Trying again in 4 s. Last known: 22 tasks")), "{t:?}");
    assert!(keyed(&rt, "session-t") > 0, "what was known stays on screen");
}

#[test]
fn an_empty_workbench_says_no_sessions_once() {
    let (_, scene) = scenes::SCENES.iter().find(|(n, _)| *n == "empty").unwrap();
    let rt = runtime(false, scene);
    assert_eq!(texts(&rt).iter().filter(|t| *t == "No sessions").count(), 1);
    assert!(texts(&rt).contains(&"Nothing needs you right now".to_owned()));
    assert!(texts(&rt).contains(&"Nothing needs you".to_owned()), "the empty digest says so");
    assert!(rt.find("empty-new").is_some(), "the empty view offers the one thing to do");
    let empty = rect(&rt, "empty");
    let button = rect(&rt, "empty-new");
    assert!(((button[0] + button[2] / 2.0) - (empty[0] + empty[2] / 2.0)).abs() < 1.0, "centred, not off to one side");
}

#[test]
fn the_web_buttons_open_the_same_place_and_without_a_daemon_say_why() {
    thread_local! { static OPENED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) }; }
    let mut rt = runtime(true, |a| {
        live(a);
        a.set_web(None);
        a.opener = |url| {
            OPENED.with(|o| o.borrow_mut().push(url.to_owned()));
            Ok(())
        };
        a.open_view(FullView::Settings);
        a.pages.section = neko_rune::pages::Section::About;
    });
    click(&mut rt, "about-open-web");
    assert!(OPENED.with(|o| o.borrow().is_empty()), "nothing opens without an address");
    assert!(texts(&rt).iter().any(|t| t.starts_with("Neko is running, but its web address isn't known yet")), "{:?}", texts(&rt));

    rt.update(|s, _| {
        s.set_web(Some(WebTarget::new(4747, &"ab".repeat(32))));
    });
    settle(&mut rt);
    click(&mut rt, "about-open-web");
    let opened = OPENED.with(|o| o.borrow().clone());
    assert_eq!(opened.len(), 1);
    assert!(opened[0].starts_with("http://127.0.0.1:4747/?token=") && !opened[0].contains('#'), "the web UI's home");
    assert!(!texts(&rt).iter().any(|t| t.contains(&"ab".repeat(32))), "the token is never on screen");
}

#[test]
fn every_scene_lays_out_in_both_appearances() {
    for dark in [true, false] {
        for (name, scene) in scenes::SCENES {
            let rt = runtime(dark, scene);
            // An expanded panel stands where the session region was.
            let middle = if *name == "p3a-expanded" { "context" } else { "session-region" };
            assert!(rt.find("sessions").is_some() && rt.find(middle).is_some() && rt.find("neko").is_some(), "{name} dark={dark}");
        }
    }
}

#[test]
fn light_and_dark_follow_the_system() {
    let fill = |rt: &Runtime<App>, key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).map(|e| e.fill.hex()).unwrap();
    let (dark, light) = (runtime(true, |_| {}), runtime(false, |_| {}));
    assert_eq!(fill(&dark, "window"), "#121212ff");
    assert_eq!(fill(&light, "window"), "#f3f3f3ff");
    assert_eq!(fill(&dark, "session-region"), "#0f0f0fff");
    assert_eq!(fill(&light, "session-region"), "#fafafaff");
}
