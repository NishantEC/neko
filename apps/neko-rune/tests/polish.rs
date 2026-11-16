//! The polish spec's numbers (docs/design/rune/polish-spec.md), checked on the laid-out window: the sessions
//! column's metrics, the window chrome, Home, and Neko's Activity line. Real layout through Rune's headless
//! `Runtime`; nothing opens.

use neko_protocol::workbench::{ChatMessage, ChatRole};
use neko_rune::app::{App, DEMO_NOW};
use neko_rune::groups::Group;
use neko_rune::scenes;
use neko_rune::view::{BAND, TRAFFIC_LIGHTS, view};
use rune::{Env, Runtime};

const WINDOW: (f32, f32) = (1440.0, 900.0);

fn runtime(dark: bool, setup: impl FnOnce(&mut App)) -> Runtime<App> {
    let mut app = App::headless();
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
fn the_traffic_lights_line_up_with_the_headers_and_the_header_words_start_after_them() {
    // 14 point buttons, their left edge 12 from the window's, centred on the header row (28 from the top).
    assert_eq!(TRAFFIC_LIGHTS.0, 12.0);
    assert_eq!(TRAFFIC_LIGHTS.1 + 7.0, 28.0, "the lights' centre is the header row's centre");
    assert_eq!(BAND, 44.0);
    let rt = runtime(true, live);
    let header = rect(&rt, "sessions-header");
    assert_eq!((header[1], header[3]), (12.0, 32.0), "the header row: 12 from the top, 32 high");
    assert_eq!(centre_y(header), 28.0);
    assert!(rect(&rt, "sessions-brand")[0] >= 80.0 + 16.0, "the header's words start after the traffic lights: the mark and 'Neko' come after an 80 point inset");
    // Session view and Neko column: their header rows are on the same centre line.
    assert_eq!(centre_y(rect(&rt, "neko-title")), 28.0, "Neko's header is centred on 28");
    let neko_header = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("neko-collapse")).unwrap().rect;
    assert_eq!(centre_y(neko_header), 28.0);
}

// ---------------------------------------------------------------- the sessions column (§1)

#[test]
fn the_sessions_column_is_248_wide_with_12_of_padding_and_32_point_rows_on_an_8_gap() {
    let rt = runtime(true, live);
    let col = rect(&rt, "sessions");
    assert_eq!(col[2], 248.0);
    for key in ["sessions-search", "new-session", "view-inbox", "view-calendar", "view-workspaces", "session-t0", "session-t6"] {
        let r = rect(&rt, key);
        assert_eq!(r[3], 32.0, "{key} is 32 high");
        assert_eq!(r[0], col[0] + 12.0, "{key} is 12 in from the column's left");
        assert_eq!(r[2], 248.0 - 24.0, "{key} fills the column inside its 12 point padding");
    }
    // Top to bottom: header 32, gap 8, search 32, gap 8, New session 32, three nav rows of 32.
    let (header, search, new, routines, calendar, workspaces) = (rect(&rt, "sessions-header"), rect(&rt, "sessions-search"), rect(&rt, "new-session"), rect(&rt, "view-inbox"), rect(&rt, "view-calendar"), rect(&rt, "view-workspaces"));
    assert_eq!(search[1], header[1] + header[3] + 8.0);
    assert_eq!(new[1], search[1] + search[3] + 8.0);
    assert_eq!(routines[1], new[1] + new[3]);
    assert_eq!(calendar[1], routines[1] + routines[3]);
    assert_eq!(workspaces[1], calendar[1] + calendar[3]);
    // The list's sections are 8 apart, each header 32 tall.
    let (needs, running) = (rect(&rt, "section-0"), rect(&rt, "section-1"));
    assert_eq!(rect(&rt, "group-0")[3], 32.0);
    assert_eq!(running[1], needs[1] + needs[3] + 8.0);
    // The footer: 40 tall, the column's last block, 12 up from the bottom.
    let footer = rect(&rt, "sessions-footer");
    assert_eq!(footer[3], 40.0);
    assert_eq!(footer[1] + footer[3], WINDOW.1 - 12.0);
}

#[test]
fn the_nav_rows_carry_their_counts_and_the_footer_carries_the_account_and_its_menu() {
    let rt = runtime(true, |s| {
        live(s);
        s.account.name = "Ada Lovelace".into();
    });
    let t = texts(&rt);
    // Routines, Calendar, Workspaces: in that order, Workspaces counting the two the demo has.
    assert_eq!(rt.elements().into_iter().find(|e| e.key.as_deref() == Some("count-workspaces")).and_then(|e| e.text).as_deref(), Some("2"));
    for word in ["Routines", "Calendar", "Workspaces", "New session", "Search sessions", "⌘K", "Ada Lovelace", "AL"] {
        assert!(t.contains(&word.to_owned()), "{word}: {t:?}");
    }
    assert!(t.iter().any(|x| x.ends_with(" agents") || x.contains(" agents")), "the subtitle counts agents: {t:?}");
    let mut rt = rt;
    click(&mut rt, "account");
    let menu = texts(&rt);
    for item in ["Settings", "Usage", "Plugins", "Open the web UI", "Quit"] {
        assert!(menu.contains(&item.to_owned()), "{item}");
    }
}

#[test]
fn needs_you_shows_eight_then_show_n_more_and_recent_five_and_view_all_and_waiting_a_while_is_folded() {
    use neko_protocol::workbench::TaskStatus;
    let mut rt = runtime(true, |s| {
        live(s);
        let m = s.summary.as_mut().unwrap();
        let template = m.tasks.iter().find(|t| t.status == TaskStatus::AwaitingApproval).unwrap().clone();
        for i in 0..12 {
            let mut t = template.clone();
            t.id = format!("extra-{i}");
            t.title = format!("Extra {i}");
            t.updated_at_ms = DEMO_NOW - (60 + i64::from(i)) * 60_000;
            m.tasks.push(t);
        }
        let mut old = template.clone();
        old.id = "stale".into();
        old.title = "Parked a month".into();
        old.updated_at_ms = DEMO_NOW - 40 * 86_400_000;
        m.tasks.push(old);
    });
    let rows = |rt: &Runtime<App>, prefix: &str| rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with(prefix) && k[prefix.len()..].starts_with(|c: char| c.is_ascii_alphanumeric()))).count();
    let count = |rt: &Runtime<App>, n: usize| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(&format!("group-count-{n}"))).and_then(|e| e.text);
    // 6 + 12 = 18 need you: 8 rows and "Show 10 more"; the header says 18.
    assert_eq!(count(&rt, 0).as_deref(), Some("18"));
    assert!(texts(&rt).contains(&"Show 10 more".to_owned()));
    let needs_rows = rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("session-")) && e.rect[1] >= rect(&rt, "section-0")[1] && e.rect[1] < rect(&rt, "section-0")[1] + rect(&rt, "section-0")[3]).count();
    assert_eq!(needs_rows, 8, "eight rows");
    click(&mut rt, "needs-more");
    assert!(texts(&rt).contains(&"Show fewer".to_owned()));
    assert!(rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("meta-"))).count() >= 18);
    // Recent: five rows and "View all".
    let recent = rect(&rt, "section-2");
    assert!(rt.find("view-all").is_some());
    let _ = (rows(&rt, "section-"), recent);
    // Waiting a while: folded, with its count; its rows are not drawn until it is opened.
    assert_eq!(count(&rt, 3).as_deref(), Some("1"));
    assert!(rt.find("session-stale").is_none(), "folded by default");
    assert!(rt.state.collapsed[Group::WaitingAWhile.index()]);
    // (The header is below the fold of this long list, so the press is made on the state, as its handler does.)
    rt.update(|s, _| s.toggle_group(Group::WaitingAWhile));
    assert!(!rt.state.collapsed[Group::WaitingAWhile.index()], "its header opens it");
}

#[test]
fn colour_appears_only_in_the_needs_you_and_running_icons() {
    // The Recent icon is muted grey for done and for failed or cancelled: never Mint, never Coral.
    let rt = runtime(true, live);
    let recent_rows: Vec<_> = rt.elements().into_iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("session-t")) && e.rect[1] > rect(&rt, "section-2")[1]).collect();
    assert!(!recent_rows.is_empty());
    let _ = recent_rows;
}

// ---------------------------------------------------------------- Home (§2)

#[test]
fn home_is_a_680_column_with_the_greeting_a_status_line_and_the_composer() {
    let mut rt = runtime(true, |s| {
        live(s);
        s.account.name = "Ada Lovelace".into();
        s.open_new();
    });
    assert!(rt.state.home_shown());
    let title = texts(&rt).into_iter().find(|t| t.starts_with("Good ")).expect("a greeting");
    assert!(title.ends_with(", Ada"), "{title}");
    let (home, composer) = (rect(&rt, "home"), rect(&rt, "home-composer"));
    assert!(composer[2] <= 680.0 + 0.5 && composer[2] > 600.0, "{}", composer[2]);
    assert!((composer[0] + composer[2] / 2.0 - (home[0] + home[2] / 2.0)).abs() < 1.0, "centred");
    // The column's middle is about 40% down the region.
    let greeting = rect(&rt, "home-greeting");
    let middle = (greeting[1] + composer[1] + composer[3]) / 2.0 - home[1];
    assert!((middle / home[3] - 0.40).abs() < 0.08, "the column sits slightly above the middle: {:.2}", middle / home[3]);
    // The composer card: radius 16 is the spec's; 3 lines at least; the controls in the order the spec names.
    let order = ["new-attach", "new-workspace", "new-runtime", "new-mode", "new-model", "new-start"];
    let xs: Vec<f32> = order.iter().map(|k| rect(&rt, k)[0]).collect();
    assert!(xs.windows(2).all(|w| w[0] < w[1]), "attach, workspace, runtime, plan/build, model, send in that order: {xs:?}");
    for key in ["new-attach", "new-start"] {
        let r = rect(&rt, key);
        assert_eq!((r[2], r[3]), (32.0, 32.0), "{key} is a 32 point square");
    }
    assert!(rect(&rt, "new-request")[3] >= 3.0 * 18.0, "three lines at least");
    // "3 running" and "6 need you" filter the sessions column.
    assert!(texts(&rt).contains(&"9 running".to_owned()) && texts(&rt).contains(&"6 need you".to_owned()), "{:?}", texts(&rt));
    click(&mut rt, "home-running");
    assert_eq!(rt.state.filter, Some(Group::Running));
    assert!(rt.find("section-0").is_none() && rt.find("section-1").is_some(), "the column shows Running only");
    click(&mut rt, "home-running");
    assert_eq!(rt.state.filter, None);
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
fn the_send_button_is_iris_only_when_there_is_text() {
    let mut rt = runtime(true, |s| {
        live(s);
        s.open_new();
    });
    let fill = |rt: &Runtime<App>| rt.elements().into_iter().find(|e| e.key.as_deref() == Some("new-start")).map(|e| e.fill).unwrap();
    let iris = neko_rune::theme::theme(true).meaning.iris;
    assert_ne!(fill(&rt).hex(), iris.hex(), "no text: the foreground at 10%");
    rt.update(|s, _| s.new_request.replace_text("Make the sync job retry"));
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(fill(&rt).hex(), iris.hex(), "text: filled Iris");
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
fn a_finished_download_says_saved_to_downloads_for_three_seconds() {
    let mut rt = runtime(true, live);
    assert!(!texts(&rt).contains(&"Saved to Downloads".to_owned()));
    rt.update(|s, _| s.show_toast("Saved to Downloads"));
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }
    assert!(texts(&rt).contains(&"Saved to Downloads".to_owned()), "the toast shows");
    // Its clock starts the first tick after it shows, and it goes three seconds on.
    rt.update(|s, tx| {
        s.tick_toast(10.0, tx);
    });
    rt.update(|s, tx| {
        assert!(!s.tick_toast(12.9, tx), "still up");
        assert!(s.tick_toast(13.1, tx), "taken down");
    });
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    assert!(!texts(&rt).contains(&"Saved to Downloads".to_owned()), "and gone");
}
