//! The P1 problems the review found, as checks: titles with room, a Neko column whose parts never
//! overlap at any window height, quiet Inbox rows, a goal line that never covers the first line of
//! the conversation, a request bubble with only the person's words, and the seen marks.

use neko_protocol::goals::GoalStatus;
use neko_rune::app::App;
use neko_rune::link::{Call, Reply};
use neko_rune::scenes;
use neko_rune::view::view;
use neko_rune::work::ItemAct;
use rune::{Env, Runtime};

fn scene(name: &str, app: &mut App) {
    let (_, f) = scenes::SCENES.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no scene {name}"));
    f(app);
    app.outbox.clear();
}

fn runtime_at(window: (f32, f32), name: &str, setup: impl FnOnce(&mut App)) -> Runtime<App> {
    let mut app = App::headless();
    scene(name, &mut app);
    setup(&mut app);
    let mut rt = Runtime::new(app, view, window, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element keyed {key}")).rect
}

fn bottom(r: [f32; 4]) -> f32 {
    r[1] + r[3]
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

#[test]
fn session_rows_have_no_id_and_their_titles_get_the_room() {
    let rt = runtime_at((1360.0, 840.0), "live", |_| {});
    // The id is in the session's own top line; the rows do not repeat it.
    assert_eq!(texts(&rt).iter().filter(|t| t.as_str() == "NK-101").count(), 0, "no id on a row");
    assert!(rt.find("top-id").is_some());
    let title = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Fix the empty state on the board")).expect("a row title");
    assert!(title.rect[2] >= 190.0, "about thirty letters fit: {}", title.rect[2]);
    // Only amber rows carry a word, the others their age.
    assert_eq!(rt.elements().into_iter().find(|e| e.key.as_deref() == Some("meta-t2")).and_then(|e| e.text).as_deref(), Some("16m"));
}

#[test]
fn the_neko_column_never_overlaps_at_560_700_840_or_1200_high() {
    for h in [560.0f32, 700.0, 840.0, 1200.0] {
        let rt = runtime_at((1360.0, h), "live", |_| {});
        let (neko, top, chat, foot) = (rect(&rt, "neko"), rect(&rt, "neko-scroll"), rect(&rt, "neko-chat-scroll"), rect(&rt, "neko-foot"));
        assert!(bottom(top) <= chat[1] + 0.5, "{h}: the top block ends at {} where the chat starts at {}", bottom(top), chat[1]);
        assert!(bottom(chat) <= foot[1] + 0.5, "{h}: the chat ends at {} where the ask box starts at {}", bottom(chat), foot[1]);
        assert!(bottom(foot) <= bottom(neko) + 0.5 && foot[1] >= neko[1], "{h}: the foot stays inside the column");
        assert!(chat[3] >= if h <= 600.0 { 160.0 } else { 120.0 }, "{h}: the chat keeps room to read: {}", chat[3]);
        // The sessions column: the list ends where the pinned foot begins, and the foot is on screen.
        let (list, sfoot) = (rect(&rt, "sessions-scroll"), rect(&rt, "sessions-foot"));
        assert!(bottom(list) <= sfoot[1] + 0.5, "{h}: the list scrolls inside its region");
        assert!(bottom(sfoot) <= h + 0.5, "{h}: + New and the foot are pinned on screen");
        let new = rect(&rt, "new-session");
        assert!(new[1] >= bottom(list) - 0.5, "{h}: + New is under the list, not over it");
    }
}

#[test]
fn the_inbox_is_rows_with_a_quiet_default_and_at_most_three_show() {
    let mut rt = runtime_at((1360.0, 900.0), "live", |s| {
        let extra = s.neko.inbox[0].clone();
        for n in 10..13 {
            s.neko.inbox.push(neko_rune::link::InboxItem { seq: n, title: format!("Another thing {n}"), ..extra.clone() });
        }
    });
    let rows = rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("inbox-") && k[6..].chars().all(|c| c.is_ascii_digit()) && k.len() > 6)).count();
    assert_eq!(rows, 3, "three items, then the rest behind one link");
    assert!(texts(&rt).contains(&"and 3 more".to_owned()));
    click(&mut rt, "inbox-more");
    assert!(texts(&rt).contains(&"Show fewer".to_owned()) && rt.find("inbox-10").is_some());
    // No filled button in the column: the default is an icon that appears on the row.
    for e in rt.elements() {
        if e.key.as_deref().is_some_and(|k| k.starts_with("inbox-use-")) {
            assert!(e.rect[2] <= 24.5, "a 24 point icon button, not a loud button: {:?}", e.rect);
        }
    }
}

#[test]
fn a_goals_inbox_item_offers_the_goals_own_actions_not_the_routines_default() {
    let mut app = App::headless();
    scene("goal-open", &mut app);
    // The goal is paused on its budget, as its Inbox item says.
    let summary = app.summary.as_mut().unwrap();
    summary.goals[0].status = GoalStatus::Paused;
    let item = app.neko.inbox.iter().find(|i| i.goal.is_some()).cloned().expect("a goal's item");
    let labels: Vec<String> = app.item_actions(&item).into_iter().map(|a| a.label).collect();
    assert!(labels.iter().all(|l| !l.starts_with("Use the default")), "{labels:?}");
    assert!(labels.iter().any(|l| l.starts_with("Extend by")) && labels.iter().any(|l| l == "Resume") && labels.iter().any(|l| l == "Stop"), "{labels:?}");
    let extend = app.item_actions(&item).into_iter().find(|a| a.label.starts_with("Extend")).unwrap();
    assert!(matches!(extend.act, ItemAct::Call(Call::GoalExtend { .. })));
    // A routine's item keeps its default, with the words in the menu.
    let routine = app.neko.inbox.iter().find(|i| i.goal.is_none() && i.default.is_some()).cloned().unwrap();
    assert!(app.item_actions(&routine)[0].label.starts_with("Use the default: "));
}

#[test]
fn a_refusal_shows_next_to_the_item_or_goal_it_belongs_to() {
    let mut rt = runtime_at((1360.0, 900.0), "goal-open", |s| {
        s.apply_reply(&Call::AnswerNeedsYou { seq: 3 }, Reply::Refused("That item is already closed.".into()));
        s.apply_reply(&Call::GoalExtend { id: "g1".into(), turns: 10 }, Reply::Refused("The goal is finished.".into()));
    });
    let t = texts(&rt);
    assert!(t.contains(&"That item is already closed.".to_owned()), "under its Inbox item: {t:?}");
    assert!(rt.find("inbox-note-3").is_some() && rt.find("inbox-note-2").is_some(), "both items show their own note");
    assert!(rt.find("goal-note-g1").is_some(), "and the goal says it next to its own line");
    assert!(rt.state.work.notice.is_none(), "not as a line above the composer");
    // A later success clears it.
    rt.update(|s, _| {
        s.apply_reply(&Call::AnswerNeedsYou { seq: 3 }, Reply::Answered("done".into()));
    });
    settle(&mut rt);
    assert!(rt.find("inbox-note-3").is_none());
}

#[test]
fn the_conversation_starts_below_the_sticky_goal_line() {
    let rt = runtime_at((1360.0, 1300.0), "running", |_| {});
    let goals = rect(&rt, "goals");
    let conversation = rect(&rt, "conversation");
    assert!(conversation[1] >= bottom(goals) + 8.0, "the first line starts at {} below the goal line's end {}", conversation[1], bottom(goals));
}

#[test]
fn the_request_bubble_shows_only_the_persons_words() {
    let rt = runtime_at((1360.0, 900.0), "live", |s| {
        let d = s.summary.as_mut().unwrap().detail.as_mut().unwrap();
        d.task.goal = "Make the login button work on Safari. tests pass · turn 4/20".into();
        d.task.request = Some("the login button does nothing on Safari".into());
    });
    let t = texts(&rt);
    assert!(t.contains(&"the login button does nothing on Safari".to_owned()), "{t:?}");
    assert!(!t.iter().any(|x| x.contains("turn 4/20")), "the goal line is not repeated in the bubble: {t:?}");
    assert!(!t.iter().any(|x| x.starts_with("In your words")));
}

#[test]
fn a_finished_session_is_seen_only_while_the_window_is_focused_and_the_session_is_on_screen() {
    let mut app = App::headless();
    scene("live", &mut app);
    let id = "t2".to_owned();
    app.unseen.insert(id.clone());
    app.selected = None;
    // Behind another app: opening it does not mark it seen.
    app.set_focused(false);
    app.select(&id);
    assert!(app.unseen.contains(&id) && !app.outbox.iter().any(|c| matches!(c, Call::MarkSeen { .. })));
    // Under a full view it is not on screen either, even once the window is focused.
    app.view = Some(neko_rune::views::FullView::Settings);
    app.set_focused(true);
    assert!(app.unseen.contains(&id), "a session under another page is not being looked at");
    // On screen in a focused window: seen.
    app.view = None;
    app.set_focused(true);
    assert!(!app.unseen.contains(&id));
    assert!(app.outbox.contains(&Call::MarkSeen { task_id: id, seen: true }));
}

#[test]
fn a_short_window_shows_two_inbox_rows_and_a_small_ask_box() {
    let rt = runtime_at((1360.0, 560.0), "live", |_| {});
    let rows = rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("inbox-") && k.len() > 6 && k[6..].chars().all(|c| c.is_ascii_digit()))).count();
    assert_eq!(rows, 2, "two items, the third behind \"and 1 more\"");
    assert!(texts(&rt).contains(&"and 1 more".to_owned()));
    assert!(rect(&rt, "ask")[3] <= 80.5, "the ask box stops at 80: {}", rect(&rt, "ask")[3]);
}

#[test]
fn an_open_goal_never_covers_the_composer_even_in_a_short_window() {
    for h in [560.0f32, 700.0, 840.0] {
        let rt = runtime_at((1360.0, h), "goal-open", |_| {});
        let body = rect(&rt, "goal-body-g1");
        assert!(body[3] <= neko_rune::view::goal_body_cap(h) + 0.5, "{h}: the open goal is {} tall", body[3]);
        let composer = rect(&rt, "composer");
        assert!(bottom(body) <= composer[1] + 0.5, "{h}: the goal ends at {} where the composer starts at {}", bottom(body), composer[1]);
    }
}

#[test]
fn the_inbox_says_when_it_could_not_be_read_and_offers_retry() {
    let mut rt = runtime_at((1360.0, 900.0), "default", |s| {
        s.apply_reply(&Call::Inbox, Reply::Refused("The daemon is busy.".into()));
    });
    assert!(rt.find("inbox-failed").is_some() && texts(&rt).contains(&"Couldn’t read the Inbox".to_owned()));
    assert!(!texts(&rt).iter().any(|t| t.starts_with("Looking for what needs you")));
    click(&mut rt, "inbox-retry");
    assert_eq!(rt.state.outbox.last(), Some(&Call::Inbox));
    assert!(rt.find("inbox-failed").is_none());
}

#[test]
fn a_failed_page_read_never_leaks_into_the_session() {
    let mut app = App::headless();
    scene("live", &mut app);
    app.open_view(neko_rune::views::FullView::Routines);
    app.apply_reply(&Call::RoutinesFull, Reply::Routines(Box::new(neko_rune::scenes_pages::routines_data())));
    app.work.notice = None;
    // The page has data; a later read fails: nothing is said anywhere.
    app.apply_reply(&Call::RoutinesFull, Reply::Refused("Neko is slow to answer.".into()));
    assert!(app.work.notice.is_none() && app.pages.notes.is_empty(), "{:?} {:?}", app.work.notice, app.pages.notes);
    // With no data, the page says why in place of "Looking for…".
    let mut fresh = App::headless();
    scene("live", &mut fresh);
    fresh.open_view(neko_rune::views::FullView::Settings);
    fresh.apply_reply(&Call::PhoneSettings, Reply::Refused("Neko is slow to answer.".into()));
    assert_eq!(fresh.pages.notes.get("page:phone").map(String::as_str), Some("Neko is slow to answer."));
    assert!(fresh.work.notice.is_none());
}
