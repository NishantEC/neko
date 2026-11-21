//! The polish spec's numbers (docs/design/rune/polish-spec.md), checked on the laid-out window: the sessions
//! column's metrics, the window chrome, Home, and Neko's Activity line. Real layout through Rune's headless
//! `Runtime`; nothing opens.

use neko_protocol::workbench::{ChatMessage, ChatRole};
use neko_rune::app::{App, DEMO_NOW};
use neko_rune::scenes;
use neko_rune::view::{BAND, TRAFFIC_LIGHTS, view};
use rune::{Env, Runtime};

const WINDOW: (f32, f32) = (1440.0, 900.0);

fn runtime(dark: bool, setup: impl FnOnce(&mut App)) -> Runtime<App> {
    let mut app = App::headless_neko();
    setup(&mut app);
    let mut rt = Runtime::new(app, view, WINDOW, Env { system_dark: dark, scale: 2.0 });
    rt.frame(0.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    rt
}

fn live(app: &mut App) {
    let (_, scene) = scenes::SCENES.iter().find(|(n, _)| *n == "live").unwrap();
    scene(app);
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element keyed {key}")).rect
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn centre_y(r: [f32; 4]) -> f32 {
    r[1] + r[3] / 2.0
}

// ---------------------------------------------------------------- window chrome (§0)

#[test]
fn the_traffic_lights_line_up_with_the_headers_and_the_brand_row_is_the_prototypes() {
    // 14 point buttons, their left edge 12 from the window's, centred on the header row (28 from the top).
    assert_eq!(TRAFFIC_LIGHTS.0, 12.0);
    assert_eq!(TRAFFIC_LIGHTS.1 + 7.0, 28.0, "the lights' centre is the header row's centre");
    assert_eq!(BAND, 44.0);
    let rt = runtime(true, live);
    // The prototype's brand row sits 45 below the window's top (the lights have the room above it): an 18 point mark and 14 below it.
    let header = rect(&rt, "sessions-header");
    assert_eq!((header[1], header[3]), (45.0, 32.0), "the brand row: 45 from the top, 32 with its 14 of space below");
    assert!(rect(&rt, "sessions-brand")[0] >= 10.0 + 8.0 + 18.0, "'Neko' follows the mark");
    // Neko's column (opened by choice) keeps its header row on the 28 centre line.
    assert_eq!(centre_y(rect(&rt, "neko-title")), 28.0, "Neko's header is centred on 28");
    let neko_header = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("neko-collapse")).unwrap().rect;
    assert_eq!(centre_y(neko_header), 28.0);
}

#[test]
fn the_sidebar_is_248_wide_flat_with_10_of_padding_and_32_point_rows() {
    let rt = runtime(true, live);
    let col = rect(&rt, "sessions");
    assert_eq!(col[2], 248.0);
    // The column is flat on the window: no inset, no card (the prototype's `.side`).
    assert_eq!((col[0], col[1], col[3]), (0.0, 0.0, WINDOW.1), "flush with the window's edges");
    for key in ["view-home", "sessions-search", "view-calendar", "view-all", "view-inbox", "view-settings"] {
        let r = rect(&rt, key);
        assert_eq!(r[3], 32.0, "{key}: every nav row is 32 high");
        assert_eq!(r[0], 10.0, "{key} is 10 in from the column's left");
        assert_eq!(r[2], 248.0 - 20.0, "{key} fills the column inside its 10 point padding");
    }
    // Top to bottom: the brand row, then Home, Search, Calendar and Tasks with no gap between them.
    let (home, search, calendar, tasks) = (rect(&rt, "view-home"), rect(&rt, "sessions-search"), rect(&rt, "view-calendar"), rect(&rt, "view-all"));
    assert_eq!(search[1], home[1] + home[3]);
    assert_eq!(calendar[1], search[1] + search[3]);
    assert_eq!(tasks[1], calendar[1] + calendar[3]);
    // Rounds and Settings are pinned to the bottom, above the footer; the footer is 30 high and 10 up from the window's bottom.
    let (rounds, settings, footer) = (rect(&rt, "view-inbox"), rect(&rt, "view-settings"), rect(&rt, "account"));
    assert_eq!(settings[1], rounds[1] + rounds[3]);
    assert_eq!(footer[3], 30.0);
    assert_eq!(footer[1] + footer[3], WINDOW.1 - 10.0);
}

#[test]
fn the_nav_rows_carry_their_counts_and_the_footer_carries_the_account_and_its_menu() {
    let rt = runtime(true, |s| {
        live(s);
        s.account.name = "Ada Lovelace".into();
    });
    let t = texts(&rt);
    for word in ["Home", "Search", "Calendar", "Tasks", "Rounds", "Settings", "⌘K", "Ada", "AL"] {
        assert!(t.contains(&word.to_owned()), "{word}: {t:?}");
    }
    // Home counts what needs you, in amber.
    assert!(rt.find("count-view-home").is_some(), "Home carries the count of what needs you");
    let mut rt = rt;
    click(&mut rt, "account");
    let menu = texts(&rt);
    for item in ["Usage", "Plugins", "Pull requests", "Open the web UI", "Quit"] {
        assert!(menu.contains(&item.to_owned()), "{item}");
    }
}

#[test]
fn home_is_a_720_column_with_the_greeting_a_status_line_and_the_composer() {
    let rt = runtime(true, |s| {
        live(s);
        s.account.name = "Ada Lovelace".into();
        s.open_new();
    });
    assert!(rt.state.home_shown());
    let title = texts(&rt).into_iter().find(|t| t.starts_with("Good ")).expect("a greeting");
    assert!(title.ends_with(", Ada"), "{title}");
    let (home, composer) = (rect(&rt, "home"), rect(&rt, "home-composer"));
    assert!(composer[2] <= 720.0 + 0.5 && composer[2] > 600.0, "{}", composer[2]);
    assert!((composer[0] + composer[2] / 2.0 - (home[0] + home[2] / 2.0)).abs() < 1.0, "centred");
    // The prototype's Home: the greeting 44 below the top, the composer's bar in the order attach, cat, model, send.
    assert!(rect(&rt, "home-title")[1] >= home[1] + 44.0 - 2.0, "the greeting sits 44 below the top");
    let order = ["new-attach", "new-cat", "new-model", "new-start"];
    let xs: Vec<f32> = order.iter().map(|k| rect(&rt, k)[0]).collect();
    assert!(xs.windows(2).all(|w| w[0] < w[1]), "attach, cat, model, send in that order: {xs:?}");
    for key in ["new-attach", "new-start"] {
        let r = rect(&rt, key);
        let side = if key == "new-start" { 30.0 } else { 26.0 };
        assert_eq!((r[2], r[3]), (side, side), "{key} is a {side} point square (Send is the 30 point ink)");
    }
    assert!(rect(&rt, "new-request-box")[3] >= 2.0 * 17.0, "two lines at least");
    // "9 working" and "6 need you" are plain words in the status line.
    let t = texts(&rt);
    assert!(t.iter().any(|x| x.ends_with(" working")) && t.iter().any(|x| x.ends_with(" you")), "{t:?}");
}

#[test]
fn home_says_nothing_running_when_both_counts_are_zero_and_the_window_opens_on_it() {
    let rt = runtime(true, |s| {
        s.apply_link(neko_rune::link::LinkEvent::Live(neko_rune::link::Summary { digest: "empty".into(), ..Default::default() }));
    });
    assert!(rt.state.home_shown(), "a window opens on Home");
    assert!(texts(&rt).contains(&"Nothing running".to_owned()));
}

#[test]
fn the_send_button_is_ink_only_when_there_is_text() {
    let mut rt = runtime(true, |s| {
        live(s);
        s.open_new();
    });
    let fill = |rt: &Runtime<App>| rt.elements().into_iter().find(|e| e.key.as_deref() == Some("new-start")).map(|e| e.fill).unwrap();
    let mat = neko_rune::theme::theme(true).mat;
    assert_eq!(fill(&rt).hex(), neko_rune::theme::theme(true).surface.tint.hex(), "no text: the tint");
    rt.update(|s, _| s.new_request.replace_text("Make the sync job retry"));
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(fill(&rt).hex(), mat.ink_bottom.hex(), "text: the ink");
}

// ---------------------------------------------------------------- Neko's chat (§0)

fn notice(id: &str, text: &str) -> ChatMessage {
    ChatMessage {
        agent_profile_revision: 0,
        agent_profile_id: "default".into(),
        workspace_id: None,
        tool_calls: vec![],
        id: id.into(),
        at_ms: DEMO_NOW - 5_000,
        role: ChatRole::Neko,
        parts: ChatMessage::text_parts(text),
        ticket_ids: vec![],
        pending: false,
        queued: false,
        failed: false,
        responsibility_ids: vec![],
        actions: vec![],
        started_ms: 0,
    }
}

#[test]
fn notices_never_draw_as_chat_messages_and_fold_into_one_activity_line() {
    let mut rt = runtime(true, |s| {
        live(s);
        let m = s.summary.as_mut().unwrap();
        m.conversation.push(notice("n1", "Neko updated itself to abc123 (UI live)."));
        m.conversation.push(notice("n2", "Reinstalling Neko at abc123 now that it is idle."));
        m.conversation.push(notice("n3", "Neko updated itself to abc123 (app reinstalled)."));
    });
    let words = texts(&rt);
    // The chat's turns do not contain them: the only place one appears is the Activity line, the latest only.
    let hits: Vec<_> = words.iter().filter(|t| t.contains("updated itself") || t.contains("Reinstalling")).collect();
    assert_eq!(hits.len(), 1, "one line, not three messages: {hits:?}");
    assert_eq!(rt.elements().into_iter().find(|e| e.key.as_deref() == Some("activity-latest")).and_then(|e| e.text).as_deref(), Some("Neko updated itself to abc123 (app reinstalled)."));
    assert_eq!(rt.elements().into_iter().find(|e| e.key.as_deref() == Some("activity-more")).and_then(|e| e.text).as_deref(), Some("+2"));
    // The line is 32 high and collapsed; pressing it opens the list in place.
    assert_eq!(rect(&rt, "activity")[3], 32.0);
    assert!(rt.find("activity-list").is_none());
    click(&mut rt, "activity");
    assert!(rt.find("activity-list").is_some());
    assert!(texts(&rt).iter().filter(|t| t.contains("updated itself") || t.contains("Reinstalling")).count() >= 3, "the opened list has every notice");
    click(&mut rt, "activity");
    assert!(rt.find("activity-list").is_none(), "pressing it again folds it");
}

#[test]
fn the_chat_shows_30_messages_and_earlier_ones_load_on_request() {
    let mut rt = runtime(true, |s| {
        live(s);
        let m = s.summary.as_mut().unwrap();
        m.conversation.clear();
        for i in 0..50 {
            m.conversation.push(ChatMessage { id: format!("u{i}"), at_ms: DEMO_NOW - 200_000 + i, role: ChatRole::User, parts: ChatMessage::text_parts(&format!("question {i}")), ..notice("x", "") });
            m.conversation.push(ChatMessage { id: format!("a{i}"), at_ms: DEMO_NOW - 100_000 + i, started_ms: 1, parts: ChatMessage::text_parts(&format!("answer {i}")), ..notice("x", "") });
        }
    });
    let words = texts(&rt);
    assert!(words.contains(&"answer 49".to_owned()) && words.contains(&"question 35".to_owned()) && !words.contains(&"question 34".to_owned()), "the last 30 messages");
    assert!(texts(&rt).contains(&"Earlier messages (70)".to_owned()), "100 messages, 30 shown: {:?}", texts(&rt).iter().filter(|t| t.starts_with("Earlier")).collect::<Vec<_>>());
    // The row is at the top of a chat that follows its end; its press is the state change its handler makes.
    rt.update(|s, _| s.chat_pages += 1);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(texts(&rt).contains(&"Earlier messages (40)".to_owned()), "30 more load");
    assert_eq!(rt.state.chat_pages, 2);
}

// ---------------------------------------------------------------- the web panel's download toast (P3b review)

#[test]
fn a_finished_download_says_saved_to_downloads_for_four_seconds() {
    let mut rt = runtime(true, live);
    assert!(!texts(&rt).contains(&"Saved to Downloads".to_owned()));
    rt.update(|s, _| s.show_toast("Saved to Downloads"));
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }
    assert!(texts(&rt).contains(&"Saved to Downloads".to_owned()), "the toast shows");
    // Its clock starts the first tick after it shows, and it goes four seconds on.
    rt.update(|s, tx| {
        s.tick_toast(10.0, tx);
    });
    rt.update(|s, tx| {
        assert!(!s.tick_toast(13.9, tx), "still up");
        assert!(s.tick_toast(14.1, tx), "taken down");
    });
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    assert!(!texts(&rt).contains(&"Saved to Downloads".to_owned()), "and gone");
}
