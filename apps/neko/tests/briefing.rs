//! The Briefing in the app (docs/knowledge/briefing.md): its page and its words on scenes (structure, reading order, every state, the
//! Home line, the link, the settings), and the whole flow against a real daemon with the fake CLI and its debug clock: what
//! finished, answering a waiting request from the page, mark as read, caught up, the line coming back, and the morning's notification.

mod support;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use neko_app::notify::{Notifier, Recorder, Sent};
use neko_app::state::{App, Daemon, need_key};
use neko_protocol::{Need, Reply, Request, TaskStatus};
use rune::{Key, Mods, Runtime};
use support::*;

fn scene(name: &str) -> fn(&mut App) {
    scenes().iter().find(|s| s.0 == name).unwrap_or_else(|| panic!("no scene {name}")).1
}

fn focus(rt: &Runtime<App>) -> Option<String> {
    rt.presented_focus_key().map(str::to_owned)
}

fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn texts(rt: &Runtime<App>) -> String {
    nodes(&rt.accessibility_json()).iter().map(|(_, n)| str_of(n, "name").to_owned()).collect::<Vec<_>>().join("\n")
}

fn y_of(rt: &Runtime<App>, key: &str) -> f32 {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key} is not on screen")).rect[1]
}

fn cmd() -> Mods {
    Mods { cmd: true, ..Mods::default() }
}

#[test]
fn command_4_opens_the_briefing_and_the_sidebar_row_is_chosen() {
    let mut rt = runtime((1200.0, 800.0), false, scene("tasks-home"));
    assert!(!has_key(&rt, "briefing-title"));
    press(&mut rt, Key::Command('4'), cmd());
    assert!(rt.state.briefing_open, "⌘4 opens it");
    assert_eq!(rt.state.window_title(), "Briefing");
    press(&mut rt, Key::Command('1'), cmd());
    assert!(!rt.state.briefing_open, "Home leaves it");
    // Escape goes back to where the person came from.
    rt.update(|s, _| s.open_briefing());
    settle(&mut rt);
    press(&mut rt, Key::Escape, Mods::default());
    assert!(!rt.state.briefing_open, "Escape goes back");
}

#[test]
fn the_briefing_link_opens_it_and_only_that_exact_link() {
    let mut rt = runtime((1200.0, 800.0), false, scene("tasks-home"));
    assert!(!rt.state.open_link("neko://briefing/extra"));
    assert!(!rt.state.briefing_open);
    assert!(rt.state.open_link("neko://briefing"));
    assert!(rt.state.briefing_open, "a link opens the page");
}

#[test]
fn a_full_morning_says_its_sections_in_order_each_in_its_words() {
    let rt = runtime((1200.0, 800.0), false, scene("briefing"));
    let all = texts(&rt);
    for line in [
        "Since yesterday 18:00",
        "6 finished, 2 needs you",
        "Renamed the button and its menu item, and kept ⌘, as the shortcut.",
        "Could not finish",
        "Stopped",
        "Interrupted",
        "Undone",
        "Docs check ran",
        "Dependency audit was skipped",
        "the last run is still working",
        "Link check was missed",
        "Backup test turned itself off",
        "after 3 failed runs in a row",
        "Port the backup script to the new server",
        "6 threads finished",
        "4 files changed",
        "$1.84 as Claude Code reported it",
        "API-equivalent, not charged on a subscription.",
    ] {
        assert!(all.contains(line), "missing {line:?} in:\n{all}");
    }
    // The order a person reads: what waits, what the Cattains wrote, what finished, the rounds, what works, the totals, then the one thing to do.
    let order = ["briefing-needs", "briefing-cattains", "briefing-finished", "briefing-rounds", "briefing-working", "briefing-totals", "briefing-read-row"];
    let ys: Vec<f32> = order.iter().map(|k| y_of(&rt, k)).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "{order:?} are at {ys:?}");
    assert_eq!(rt.state.window_title(), "Briefing");
    // One workspace has the activity: one list, no workspace headings.
    assert!(!rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.contains("-ws-"))), "one project: no headings");
}

#[test]
fn many_workspaces_group_each_section_by_workspace() {
    let rt = runtime((1200.0, 800.0), false, scene("briefing-many"));
    for key in ["briefing-needs-ws-ws_acme", "briefing-needs-ws-ws_web", "briefing-finished-ws-ws_acme", "briefing-finished-ws-default", "briefing-rounds-ws-ws_web", "briefing-working-ws-ws_acme"] {
        assert!(has_key(&rt, key), "{key}");
    }
}

#[test]
fn finished_rows_offer_review_and_undo_where_neko_offers_them_and_redo_for_an_undone_run() {
    let rt = runtime((1200.0, 800.0), false, scene("briefing"));
    let b = rt.state.briefing.clone().unwrap();
    let by = |title: &str| b.finished.iter().find(|f| f.title.starts_with(title)).unwrap().task.0.clone();
    let renamed = by("Rename the Settings");
    for part in ["review", "undo", "open"] {
        assert!(has_key(&rt, &format!("briefing-task-{renamed}-{part}")), "{part} on a run that changed files");
    }
    let failed = by("Find out why");
    assert!(has_key(&rt, &format!("briefing-task-{failed}-open")));
    assert!(!has_key(&rt, &format!("briefing-task-{failed}-undo")) && !has_key(&rt, &format!("briefing-task-{failed}-review")), "nothing changed: nothing to undo");
    let undone = by("Write the pricing");
    assert!(has_key(&rt, &format!("briefing-task-{undone}-redo")) && has_key(&rt, &format!("briefing-task-{undone}-undone")));
}

#[test]
fn tab_goes_through_the_page_in_reading_order_and_ends_on_mark_as_read() {
    let mut rt = runtime((1200.0, 800.0), false, scene("briefing"));
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..60 {
        tab(&mut rt, false);
        let Some(k) = focus(&rt) else { continue };
        if seen.first() == Some(&k) {
            break;
        }
        seen.push(k);
    }
    // The cycle begins at the window's first stop.
    let start = seen.iter().position(|k| k == "sidebar-hide").expect("the sidebar's button");
    seen.rotate_left(start);
    let at = |p: &dyn Fn(&str) -> bool| seen.iter().position(|k| p(k)).unwrap_or_else(|| panic!("no stop matches in {seen:?}"));
    let last = |p: &dyn Fn(&str) -> bool| seen.iter().rposition(|k| p(k)).unwrap_or_else(|| panic!("no stop matches in {seen:?}"));
    let need = at(&|k| k.starts_with("need-"));
    let need_end = last(&|k| k.starts_with("need-"));
    let digest = at(&|k| k.starts_with("briefing-digest-"));
    let fin = at(&|k| k.starts_with("briefing-task-"));
    let fin_end = last(&|k| k.starts_with("briefing-task-"));
    let round = at(&|k| k.starts_with("briefing-round-"));
    let working = at(&|k| k.starts_with("briefing-working-"));
    let read = at(&|k| k == "briefing-read");
    assert!(need < need_end && need_end < digest && digest < fin && fin_end < round && round < working && working < read, "reading order: {seen:?}");
    assert_eq!(seen.iter().filter(|k| *k == "briefing-read").count(), 1, "once round");
    // The sidebar's row for it is a stop of its own, right after Needs you.
    let needs = at(&|k| k == "needs-you");
    assert_eq!(seen[needs + 1], "briefing", "Needs you, then the Briefing row: {seen:?}");
}

#[test]
fn the_arrows_move_between_finished_rows_and_return_opens_the_task() {
    let mut rt = runtime((1200.0, 800.0), false, scene("briefing"));
    let b = rt.state.briefing.clone().unwrap();
    let (first, second) = (&b.finished[0], &b.finished[1]);
    let key = |t: &neko_protocol::briefing::BriefingTask| format!("briefing-task-{}", t.task.0);
    rt.update({
        let k = key(first);
        move |_, tx| tx.focus(k, true)
    });
    settle(&mut rt);
    assert_eq!(focus(&rt), Some(key(first)));
    press(&mut rt, Key::Down, Mods::default());
    assert_eq!(focus(&rt), Some(key(second)), "Down moves to the next row");
    press(&mut rt, Key::Up, Mods::default());
    assert_eq!(focus(&rt), Some(key(first)));
    press(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.open.as_ref(), Some(&first.task), "Return opens its thread");
    assert!(!rt.state.briefing_open);
}

#[test]
fn the_cattains_section_quotes_each_changed_digest_and_only_cattains_have_faces() {
    let mut rt = runtime((1200.0, 1500.0), false, scene("briefing-cattains"));
    let all = texts(&rt);
    for line in [
        "miso's digest",
        "Merge the three flaky-test branches? Each one is green on its own, and together they pass twice in a row.",
        "Fix test one: done. The clock is handed in now, so it no longer waits on the wall clock.",
        "Fix test three: done, with a retry left out on purpose.",
        "and 1 more in its digest",
        "mochi's digest",
        "The install guide's two broken links are fixed.",
        "New Cattain: yuzu (iOS work)",
    ] {
        assert!(all.contains(line), "missing {line:?} in:\n{all}");
    }
    assert!(y_of(&rt, "briefing-cattains") < y_of(&rt, "briefing-finished"), "the Cattains come before what finished");
    // A face means a Cattain: the Cattains section has them, and no finished row is led by one; each is led by its status mark.
    let keys: Vec<String> = rt.elements().iter().filter_map(|e| e.key.clone()).collect();
    let faces: Vec<&String> = keys.iter().filter(|k| k.starts_with("briefing-") && k.ends_with("-face")).collect();
    assert!(!faces.is_empty() && faces.iter().all(|k| k.starts_with("briefing-digest-") || k.starts_with("briefing-new-cattain-")), "{faces:?}");
    let b = rt.state.briefing.clone().unwrap();
    for f in &b.finished {
        assert!(has_key(&rt, &format!("briefing-task-{}-mark", f.task.0)), "{} is led by its mark", f.title);
    }
    // A crew thread says whose crew it is in; a thread of the person's does not.
    let crew: Vec<&neko_protocol::briefing::BriefingTask> = b.finished.iter().filter(|f| f.cat == "cat_miso").collect();
    assert!(!crew.is_empty() && crew.iter().all(|f| has_key(&rt, &format!("briefing-task-{}-crew", f.task.0))), "each crew thread says miso's crew");
    let mine = b.finished.iter().find(|f| f.cat.is_empty()).unwrap();
    assert!(!has_key(&rt, &format!("briefing-task-{}-crew", mine.task.0)));
    // The keyboard: the page opens on miso's block; Down goes to mochi's, then yuzu's line; Return on a block opens its digest.
    assert_eq!(focus(&rt).as_deref(), Some("briefing-digest-cat_miso"), "nothing waits: the first digest has the keyboard");
    press(&mut rt, Key::Down, Mods::default());
    assert_eq!(focus(&rt).as_deref(), Some("briefing-digest-cat_mochi"));
    press(&mut rt, Key::Down, Mods::default());
    assert_eq!(focus(&rt).as_deref(), Some("briefing-new-cattain-cat_yuzu"));
    press(&mut rt, Key::Up, Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    assert!(!rt.state.briefing_open, "Return opens mochi's thread");
    assert_eq!(rt.state.open_cattain_cat().map(|c| c.id.as_str()), Some("cat_mochi"));
    let open = rt.state.open.clone().unwrap();
    assert_eq!(rt.state.pane.tab.get(&open), Some(&neko_app::pane::Tab::Digest), "on its Digest tab");
}

#[test]
fn a_cattains_own_turn_posts_no_done_and_a_crew_thread_nobody_hears_says_whose_crew() {
    let mut rt = runtime((1200.0, 800.0), false, scene("briefing-cattains"));
    let rec = Recorder::default();
    let finish = |rt: &mut Runtime<App>, pick: &dyn Fn(&neko_protocol::Task) -> bool| {
        let mut t = rt.state.tasks.iter().find(|t| pick(t)).cloned().unwrap();
        t.status = TaskStatus::Working;
        rt.update(move |s, _| s.change(neko_protocol::Change::TaskUpdated { task: t.clone() }));
        let mut t = rt.state.tasks.iter().find(|t| pick(t)).cloned().unwrap();
        t.status = TaskStatus::Done;
        rt.update(move |s, _| s.change(neko_protocol::Change::TaskUpdated { task: t.clone() }));
    };
    rt.update({
        let rec = rec.clone();
        move |s, _| s.notifier = Notifier::new(Box::new(rec))
    });
    finish(&mut rt, &|t| t.cattain && t.cat.as_deref() == Some("cat_miso"));
    assert!(posts(&rec.take()).is_empty(), "nothing of a Cattain's notifies but its ask");
    // miso has run, so it hears its crew: a crew thread finishing is miso's news, not the person's.
    finish(&mut rt, &|t| t.title.starts_with("Fix test four"));
    assert!(posts(&rec.take()).is_empty(), "a crew thread finishing posts nothing when its Cattain hears it");
    // A Cattain nobody has talked to hears nothing: the person does, told whose crew it is.
    rt.update(|s, _| {
        for t in s.tasks.iter_mut().filter(|t| t.cattain) {
            t.status = TaskStatus::New;
        }
    });
    finish(&mut rt, &|t| t.title.starts_with("Fix test one"));
    let sent = rec.take();
    let p = posts(&sent);
    assert_eq!((p.len(), p[0].title.as_str()), (1, "Fix test one: the clock in the retry test (miso's crew)"));
    finish(&mut rt, &|t| t.title.starts_with("Ask for a Cattain"));
    let sent = rec.take();
    assert_eq!(posts(&sent)[0].title, "Ask for a Cattain for the iOS beta", "a thread of the person's names no cat");
}

#[test]
fn a_digest_with_nothing_under_needs_you_or_new_still_shows_it_changed_in_one_quiet_line() {
    let rt = runtime((1200.0, 800.0), false, scene("briefing-cattains-quiet"));
    assert!(has_key(&rt, "briefing-digest-cat_mochi-quiet") && texts(&rt).contains("Nothing under Needs you or New."));
    assert!(!has_key(&rt, "briefing-digest-cat_mochi-new"));
}

#[test]
fn an_empty_window_says_so_in_one_line_and_a_caught_up_page_says_when_the_next_one_is() {
    let rt = runtime((1200.0, 800.0), false, scene("briefing-empty"));
    let all = texts(&rt);
    assert!(all.contains("Nothing happened since yesterday 08:05."), "{all}");
    assert!(all.contains("Your next briefing is tomorrow at 08:00."), "{all}");
    assert!(!has_key(&rt, "briefing-read"), "nothing to read: no Mark as read");
    assert!(!has_key(&rt, "briefing-caught-up"));

    let rt = runtime((1200.0, 800.0), false, scene("briefing-caught-up"));
    let all = texts(&rt);
    assert!(all.contains("You're caught up") && all.contains("Your next briefing is tomorrow at 08:00."), "{all}");
    assert!(!has_key(&rt, "briefing-read") && has_key(&rt, "briefing-caught-up"));

    let rt = runtime((1200.0, 800.0), false, scene("briefing-morning-off"));
    assert!(texts(&rt).contains("Morning briefings are off. Turn them on in Settings."));

    // Read, nothing new, a request that waited before still waits: it stays in sight, under the caught-up line.
    let rt = runtime((1200.0, 800.0), false, scene("briefing-caught-up-waiting"));
    let all = texts(&rt);
    assert!(all.contains("You're caught up") && all.contains("Still waiting for you"), "{all}");
    assert!(has_key(&rt, "briefing-needs") && !has_key(&rt, "briefing-read"));
    assert!(y_of(&rt, "briefing-caught-up") < y_of(&rt, "briefing-needs"));
}

#[test]
fn home_has_one_quiet_line_when_there_is_news_and_nothing_otherwise() {
    let rt = runtime((1200.0, 800.0), false, scene("home-briefing"));
    assert!(texts(&rt).contains("While you were away: 6 finished, 2 needs you"));
    assert!(has_key(&rt, "home-briefing") && has_key(&rt, "home-briefing-open"));
    for name in ["briefing-empty", "briefing-caught-up", "new-thread"] {
        let mut rt = runtime((1200.0, 800.0), false, scene(name));
        rt.update(|s, _| s.go_new_thread());
        settle(&mut rt);
        assert!(!has_key(&rt, "home-briefing"), "{name}: nothing to say");
    }
    // The line opens the page.
    let mut rt = runtime((1200.0, 800.0), false, scene("home-briefing"));
    click(&mut rt, "home-briefing-open");
    assert!(rt.state.briefing_open);
}

#[test]
fn settings_has_a_briefing_page_with_the_morning_switch_and_time() {
    let rt = runtime((1200.0, 800.0), false, scene("settings-briefing"));
    let all = texts(&rt);
    assert!(all.contains("No model writes it") && all.contains("What it shows") && all.contains("Tell me when my briefing is ready"), "{all}");
    assert!(has_key(&rt, "briefing-morning-on") && has_key(&rt, "briefing-morning-time") && has_key(&rt, "briefing-morning-save"));
    assert_eq!(rt.state.typed_morning().as_deref(), Some("08:00"));
    let rt = runtime((1200.0, 800.0), false, scene("settings-briefing-invalid"));
    assert!(texts(&rt).contains("Write the time as hours and minutes, like 08:00."));
    assert_eq!(rt.state.typed_morning(), None);
    let mut rt = runtime((1200.0, 800.0), false, scene("settings-briefing"));
    rt.update(|s, _| s.briefing_time = rune::TextEdit::new("7:05"));
    assert_eq!(rt.state.typed_morning().as_deref(), Some("07:05"), "kept as HH:MM");
}

#[test]
fn nothing_is_clipped_at_the_smallest_window_and_every_control_has_a_name() {
    for name in ["briefing", "briefing-many", "briefing-cattains", "briefing-cattains-quiet", "briefing-finished", "briefing-caught-up", "briefing-caught-up-waiting", "briefing-empty", "home-briefing", "settings-briefing"] {
        let rt = runtime((760.0, 560.0), false, scene(name));
        for e in rt.elements() {
            if let Some(k) = e.key.as_deref().filter(|k| k.starts_with("briefing-") || k.starts_with("home-briefing")) {
                let r = e.rect;
                assert!(r[0] >= 0.0 && r[0] + r[2] <= 760.5, "{name}: {k} is cut by the window's edge ({r:?})");
            }
        }
        let tree = rt.accessibility_json();
        for (_, n) in nodes(&tree) {
            if matches!(str_of(&n, "role"), "button" | "switch" | "textInput" | "listItem") {
                assert!(!str_of(&n, "name").trim().is_empty(), "{name}: a control with no name: {n}");
            }
        }
        assert_eq!(nodes(&tree).iter().filter(|(_, n)| str_of(n, "role") == "heading" && n["level"].as_u64() == Some(1)).count(), 1, "{name}: one level-1 heading");
    }
}

#[test]
fn the_briefings_inks_reach_four_and_a_half_to_one_on_its_surfaces() {
    use rune_ui_style::contrast;
    rune_ui_style::fonts::install(neko_app::ui::FONTS);
    for dark in [false, true] {
        let t = neko_app::view::theme(dark, false);
        // The well its lists sit in, the paper under its headings, and the raised cards of Needs you.
        for (surface, bg) in [("well", rune_ui_style::over(t.c.page.bg, t.c.input.bg)), ("paper", t.c.page.bg), ("raised", neko_app::view::raised_fill(&t))] {
            for (name, ink) in [("ink", t.ink()), ("second", t.ink_muted()), ("third", t.ink_faint()), ("warning", t.c.warning.fg), ("failed", t.c.danger.fg), ("working", t.c.accent.fg), ("added", t.c.success.fg)] {
                let c = contrast(ink, rune::Color { a: 1.0, ..bg });
                assert!(c >= 4.5, "{} {name} on {surface}: {c:.2}:1", if dark { "dark" } else { "light" });
            }
        }
    }
}

// ---- the whole flow against a real daemon ----

struct Folder {
    dir: PathBuf,
    pids: Vec<i32>,
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

impl Drop for Folder {
    fn drop(&mut self) {
        for &pid in &self.pids {
            if alive(pid) {
                // SAFETY: a daemon this test started, by the PID it wrote.
                unsafe { libc::kill(pid, libc::SIGKILL) };
            }
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn until(rt: &mut Runtime<App>, what: &str, ok: impl Fn(&Runtime<App>) -> bool) {
    let end = Instant::now() + Duration::from_secs(30);
    while !ok(rt) {
        assert!(Instant::now() < end, "timed out waiting for {what}: briefing {:?}, threads {:?}, home line {}", rt.state.briefing.as_ref().map(|b| (b.finished.len(), b.from_ms, b.to_ms, b.totals.finished)), rt.state.tasks.iter().map(|t| t.status).collect::<Vec<_>>(), has_key(rt, "home-briefing"));
        std::thread::sleep(Duration::from_millis(20));
        rt.frame(0.0);
    }
    rt.frame(0.0);
}

fn make_task(rt: &mut Runtime<App>, words: &'static str) {
    rt.update(move |s, _| {
        s.go_new_thread();
        s.draft = rune::TextEdit::new(words);
        s.submit();
    });
    until(rt, "the thread to be made", |rt| rt.state.creating.is_none() && rt.state.tasks.iter().any(|t| t.words == words));
    // A new task opens in the task view: the person is back on Home.
    rt.update(|s, _| s.go_new_thread());
}

fn posts(sent: &[Sent]) -> Vec<&neko_app::notify::Note> {
    sent.iter().filter_map(|s| if let Sent::Post(n) = s { Some(n) } else { None }).collect()
}

#[test]
fn the_whole_flow_against_a_real_daemon() {
    let bin = PathBuf::from(std::env::var("NEKO_DAEMON_BIN").expect("set NEKO_DAEMON_BIN to a built neko-daemon (scripts/check.sh does)"));
    let fake = bin.with_file_name("neko-fake-claude");
    assert!(fake.is_file(), "build the fake CLI first ({} is missing)", fake.display());
    // SAFETY: set before any thread of this test reads the environment; this is the one test in this binary that starts a daemon.
    unsafe {
        std::env::set_var("NEKO_CLAUDE_BIN", &fake);
        std::env::set_var("NEKO_DEBUG_CLOCK", "1");
        std::env::set_var("TZ", "UTC");
    }
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let mut folder = Folder { dir: std::env::temp_dir().join(format!("neko-briefing-e2e-{}-{nanos}", std::process::id())), pids: Vec::new() };
    let dir: &Path = &folder.dir.clone();
    let home = dir.join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    // SAFETY: as above.
    unsafe { std::env::set_var("HOME", &home) };

    let mut rt = runtime((1200.0, 800.0), false, |_| {});
    let rec = Recorder::default();
    let conn = neko_app::link::Conn::start(dir.to_path_buf(), Some(bin), || {});
    let r = rec.clone();
    rt.update(move |s, _| {
        s.notifier = Notifier::new(Box::new(r));
        s.front = false;
        s.daemon = Daemon::Starting;
        s.conn = Some(conn);
    });
    until(&mut rt, "the app to connect", |rt| rt.state.daemon == Daemon::Connected && rt.state.briefing.is_some() && rt.state.briefing_settings.is_some());
    folder.pids.extend(neko_protocol::client::find(dir).ok().map(|(i, _)| i.pid as i32));
    assert!(!has_key(&rt, "home-briefing"), "nothing happened yet: Home says nothing");

    // A task finishes: the briefing hears it, and Home has its one line.
    make_task(&mut rt, "Check the docs [fake:plain]");
    until(&mut rt, "the briefing to list it", |rt| rt.state.briefing.as_ref().is_some_and(|b| b.finished.len() == 1) && has_key(rt, "home-briefing"));
    assert!(texts(&rt).contains("While you were away: 1 finished"));

    // The page: the task, its first sentence, its cat.
    press(&mut rt, Key::Command('4'), cmd());
    assert!(rt.state.briefing_open);
    let all = texts(&rt);
    assert!(all.contains("Hello from the fake Claude Code.") && all.contains("1 thread finished") && all.contains("API-equivalent, not charged on a subscription."), "{all}");

    // A request waits: it is in the Briefing, answered there with the Needs you card.
    make_task(&mut rt, "Run the tests [fake:permission]");
    rt.update(|s, _| s.open_briefing());
    until(&mut rt, "the request in the briefing", |rt| rt.state.briefing.as_ref().is_some_and(|b| b.needs.len() == 1) && rt.state.visible_needs().len() == 1);
    let item = rt.state.visible_needs()[0].clone();
    assert!(matches!(item.need, Need::Permission { .. }));
    assert!(has_key(&rt, &need_key(&item.id)), "the card is on the Briefing");
    click(&mut rt, &format!("{}-allow", need_key(&item.id)));
    assert!(rt.state.visible_needs().is_empty(), "answered: gone at once");
    until(&mut rt, "the daemon to drop it and the thread to finish", |rt| rt.state.needs.is_empty() && rt.state.tasks.iter().any(|t| t.id == item.task && t.status == TaskStatus::Done));
    until(&mut rt, "both finished in the briefing", |rt| rt.state.briefing.as_ref().is_some_and(|b| b.finished.len() == 2 && b.needs.is_empty()));

    // Mark as read with ⌘↩: caught up, and the Home line is gone.
    press(&mut rt, Key::Enter, cmd());
    until(&mut rt, "caught up", |rt| rt.state.briefing.as_ref().is_some_and(|b| b.read_ms.is_some() && b.finished.is_empty()) && has_key(rt, "briefing-caught-up"));
    assert!(texts(&rt).contains("You're caught up"));
    assert!(!has_key(&rt, "briefing-read"));
    rt.update(|s, _| s.go_new_thread());
    settle(&mut rt);
    assert!(!has_key(&rt, "home-briefing"), "read: nothing to say on Home");

    // Something new finishes: the line comes back.
    make_task(&mut rt, "Look at the docs again [fake:plain]");
    until(&mut rt, "the line to return", |rt| has_key(rt, "home-briefing"));
    assert!(texts(&rt).contains("While you were away: 1 finished"));

    // The morning: three hours from now by the scheduler's clock, moved to just after it. One notification with the counts; none
    // more the same day; opening the Briefing takes it back.
    let _ = rec.take();
    let (info, token) = neko_protocol::client::find(dir).unwrap();
    let mut c = neko_protocol::client::Client::connect(info.port, &token, "briefing-test").unwrap();
    let in_3h = {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        let m = (now + 3 * 3_600_000).div_euclid(60_000).rem_euclid(24 * 60);
        format!("{:02}:{:02}", m / 60, m % 60)
    };
    assert!(matches!(c.request(Request::SetBriefingSettings { settings: neko_protocol::BriefingSettings { morning_on: true, morning_at: in_3h } }).unwrap(), Reply::BriefingSettings { .. }));
    assert_eq!(c.request(Request::DebugClock { offset_ms: Some(3 * 3_600_000 + 2 * 60_000) }).unwrap(), Reply::Ok);
    until(&mut rt, "the morning's notification", |_| posts(&rec.0.borrow()).iter().any(|n| n.id == "neko.briefing"));
    let sent = rec.take();
    let note = posts(&sent).into_iter().find(|n| n.id == "neko.briefing").expect("posted");
    assert_eq!(note.title, "Your briefing: 1 finished");
    assert!(note.info.contains(&("url".to_owned(), "neko://briefing".to_owned())));
    for extra in [4 * 60_000, 3_600_000, 5 * 3_600_000] {
        assert_eq!(c.request(Request::DebugClock { offset_ms: Some(3 * 3_600_000 + 2 * 60_000 + extra) }).unwrap(), Reply::Ok);
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(20));
            rt.frame(0.0);
        }
        assert!(posts(&rec.take()).iter().all(|n| n.id != "neko.briefing"), "once a day");
    }
    rt.update(|s, _| s.open_briefing());
    assert!(rec.take().contains(&Sent::Withdraw(vec!["neko.briefing".to_owned()])), "opening the Briefing takes the notification back");
    c.close();

    rt.update(|s, _| s.conn = None);
    for &pid in &folder.pids {
        // SAFETY: the daemon this test started, by its PID.
        unsafe { libc::kill(pid, libc::SIGTERM) };
        let end = Instant::now() + Duration::from_secs(10);
        while alive(pid) && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive(pid), "the daemon stopped cleanly");
    }
}
