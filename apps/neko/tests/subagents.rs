//! Kittens in the app (docs/knowledge/app-shell.md, "Kittens in the app"), on scenes: not listed as threads, the thread's row
//! and its second line, the card and its words for each state, the wake note and its fold, the waiting line and its links, a
//! subagent opened read-only with its breadcrumb, Back and Stop, its permission request and its Needs you item and notification,
//! and no news when one finishes. The Settings switch is checked against a real daemon (`NEKO_DAEMON_BIN`).

mod support;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use neko_app::notify::{Notifier, Recorder, Sent};
use neko_app::state::{App, Daemon, need_key};
use neko_protocol::{Change, PartBody, TaskStatus};
use rune::{Key, Mods, Runtime};
use support::*;

fn scene(name: &str) -> fn(&mut App) {
    scenes().iter().find(|s| s.0 == name).unwrap_or_else(|| panic!("no scene {name}")).1
}

fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn el(rt: &Runtime<App>, key: &str) -> rune::ElementInfo {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key} is not on screen"))
}

/// Every text drawn and every name and description in the tree.
fn names(rt: &Runtime<App>) -> String {
    let mut all: Vec<String> = nodes(&rt.accessibility_json()).iter().flat_map(|(_, n)| [str_of(n, "name").to_owned(), str_of(n, "description").to_owned()]).collect();
    all.extend(rt.elements().into_iter().filter(|e| e.opacity > 0.0).filter_map(|e| e.text));
    all.join("\n")
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter(|e| e.opacity > 0.0).filter_map(|e| e.text).collect()
}

/// The tree's node with this key.
fn node(rt: &Runtime<App>, key: &str) -> serde_json::Value {
    nodes(&rt.accessibility_json()).into_iter().map(|(_, n)| n).find(|n| str_of(n, "key") == key).unwrap_or_else(|| panic!("{key} is not in the tree"))
}

/// A subagent's task id in the scenes: `task_` and 26 digits.
/// The sequence numbers of the wake notes of the open thread, in order.
fn wake_seqs(rt: &Runtime<App>) -> Vec<u64> {
    rt.state.book().unwrap().parts.iter().filter(|p| matches!(p.body, PartBody::Wake { .. })).map(|p| p.seq).collect()
}

fn sub_id(n: u32) -> String {
    format!("task_{:026}", 40 + n)
}

#[test]
fn a_subagent_is_never_a_row_in_the_sidebar_and_the_thread_says_what_it_waits_on() {
    for size in SIZES {
        let rt = runtime(size, false, scene("subagents-waiting"));
        if size.0 < 1000.0 {
            continue;
        }
        let keys: Vec<String> = rt.elements().into_iter().filter_map(|e| e.key).collect();
        assert!(!keys.iter().any(|k| k.contains(&sub_id(1)) || k.contains(&sub_id(2))), "no subagent row anywhere: {keys:?}");
        let row = keys.iter().find(|k| k.starts_with("thread-") && k.contains("00000000000000000000000030")).expect("the thread's row");
        let n = node(&rt, row);
        assert_eq!(str_of(&n, "role"), "list-box-option");
        assert_eq!(str_of(&n, "name"), "Fix login");
        let d = str_of(&n, "description");
        assert!(d.contains("waiting") && d.contains("2 kittens working"), "{d}");
        assert!(texts(&rt).iter().any(|t| t == "2 kittens working"), "the row's second line");
    }
    // Exactly one live subagent: it is named; a thread that is only working says it differently.
    let mut rt = runtime((1200.0, 800.0), false, scene("subagents-waiting"));
    rt.update(|s, _| {
        s.tasks.iter_mut().find(|t| t.id.0 == sub_id(2)).unwrap().status = TaskStatus::Done;
    });
    settle(&mut rt);
    assert!(texts(&rt).iter().any(|t| t == "Waiting on Kitten 1"), "{:?}", texts(&rt));
    rt.update(|s, _| {
        s.tasks.iter_mut().find(|t| t.id.0 == sub_id(1)).unwrap().status = TaskStatus::Done;
    });
    settle(&mut rt);
    assert!(texts(&rt).iter().any(|t| t == "Reading the reports"), "no live subagent, still waiting: {:?}", texts(&rt));
    // A thread with no subagent has no such line, and a subagent is not in Home's lists either.
    let rt = runtime((1200.0, 800.0), false, scene("task-working"));
    assert!(!texts(&rt).iter().any(|t| t.contains("subagent")));
    let mut rt = runtime((1200.0, 800.0), false, scene("subagents-waiting"));
    rt.update(|s, _| s.go_new_thread());
    settle(&mut rt);
    assert!(!names(&rt).contains("Check links"), "a subagent is not on Home: {}", names(&rt));
}

#[test]
fn the_waiting_thread_still_ring_is_a_state_of_its_own_and_not_the_working_ring() {
    let rt = runtime((1200.0, 800.0), false, scene("subagents-waiting"));
    // Its row's mark is the Waiting one (the still ring, in the second ink), not the working ring and not an amber dot.
    let thread = rt.state.tasks.iter().find(|t| t.id.0.ends_with("30")).expect("the thread");
    assert_eq!(neko_app::status_mark::mark_of(thread), neko_app::status_mark::Mark::Waiting);
    assert_eq!(thread.thread_status, neko_protocol::ThreadStatus::Waiting);
    assert!(rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("thread-") && k.ends_with("-mark"))), "the row has its mark");
    let (words, _) = (neko_app::view::status_mark(TaskStatus::Waiting).1, ());
    assert_eq!(words, "Waiting");
    assert_eq!(neko_app::briefing::task_words(TaskStatus::Waiting), "waiting on its kittens");
}

#[test]
fn the_card_is_one_row_per_subagent_with_the_word_for_each_state() {
    let mut rt = runtime((1200.0, 800.0), false, scene("subagents-cards"));
    let stack = el(&rt, "subagent-cards-1");
    let (c1, c2) = (el(&rt, "subagent-card-1"), el(&rt, "subagent-card-2"));
    assert!(c1.rect[1] < c2.rect[1] && (c1.rect[0] - c2.rect[0]).abs() < 0.5, "stacked rows");
    assert!(c1.rect[0] >= stack.rect[0] && c2.rect[1] + c2.rect[3] <= stack.rect[1] + stack.rect[3] + 0.5, "one card holds both rows");
    for h in [c1.rect[3], c2.rect[3]] {
        assert!((h - 56.0).abs() < 1.0, "about 56 points a row: {h}");
    }
    let t = texts(&rt);
    for want in ["Kitten 1", "Check links", "Working", "Kitten 2", "Check images", "Queued", "Shared folder"] {
        assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
    }
    assert!(t.iter().any(|x| x.starts_with("Two links are dead so far")), "its live last line: {t:?}");
    assert_eq!(str_of(&node(&rt, "subagent-open-1"), "name"), "Open Kitten 1");
    assert_eq!(str_of(&node(&rt, "subagent-open-2"), "name"), "Open Kitten 2");
    assert_eq!(str_of(&node(&rt, "subagent-card-1"), "role"), "list-item");
    // Every state has its word.
    for (st, word) in [
        (TaskStatus::New, "Queued"),
        (TaskStatus::Working, "Working"),
        (TaskStatus::Done, "Done"),
        (TaskStatus::Failed, "Failed"),
        (TaskStatus::Stopped, "Stopped"),
        (TaskStatus::Interrupted, "Interrupted"),
        (TaskStatus::Waiting, "Waiting"),
        (TaskStatus::Asked, "Has a question"),
    ] {
        rt.update(move |s, _| s.tasks.iter_mut().find(|t| t.id.0 == sub_id(2)).unwrap().status = st);
        settle(&mut rt);
        let c = node(&rt, "subagent-card-2");
        assert!(str_of(&c, "name").contains(&format!(", {word}.")), "{st:?}: {}", str_of(&c, "name"));
        assert!(texts(&rt).iter().any(|x| x == word), "{st:?} reads {word}: {:?}", texts(&rt));
    }
    // Open puts the subagent's conversation in the right pane as a tab; the thread stays in the main view.
    click(&mut rt, "subagent-open-1");
    assert!(rt.state.open_subagent().is_none(), "the main view is still the thread's");
    assert!(has_key(&rt, "pane-kitten") && has_key(&rt, "pane-transcript"), "its own conversation is the pane's tab");
    // Open in main view takes it into the main view, read-only there.
    rt.frame(0.3);
    settle(&mut rt);
    click(&mut rt, "pane-kitten-main");
    assert_eq!(rt.state.open.as_ref().map(|i| i.0.clone()), Some(sub_id(1)));
    assert!(has_key(&rt, "subagent-readonly"));
}

#[test]
fn a_finished_run_keeps_its_card_outside_the_fold_and_the_card_is_not_in_the_steps() {
    let rt = runtime((1200.0, 800.0), false, scene("subagents-waiting"));
    assert!(has_key(&rt, "subagent-card-1") && has_key(&rt, "subagent-card-2"), "the run's fold is closed and its cards show");
    let (fold, card, last) = (el(&rt, "worked-run_1"), el(&rt, "subagent-card-1"), el(&rt, "waiting-line"));
    assert!(fold.rect[1] < card.rect[1] && card.rect[1] < last.rect[1], "fold, then the card, then the waiting line");
}

#[test]
fn the_wake_note_says_who_woke_the_thread_and_folds_the_report() {
    for size in SIZES {
        let mut rt = runtime(size, false, scene("subagents-wake"));
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Woken by Kitten 1 · done") && t.iter().any(|x| x == "Woken by Kitten 2 · done"), "{t:?}");
        // Centred: the note's words sit in the middle of the column, a hairline each side.
        let w = wake_seqs(&rt);
        let (n1, r1) = (format!("wake-{}", w[0]), format!("wake-report-{}", w[0]));
        let note = el(&rt, &n1).rect;
        let line = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Woken by Kitten 1 · done")).unwrap().rect;
        assert!(((line[0] + line[2] / 2.0) - (note[0] + note[2] / 2.0)).abs() < 2.0, "centred: {line:?} in {note:?}");
        // Closed: the report is named, its words are not on the page.
        let fold = node(&rt, &format!("{r1}-trigger"));
        assert_eq!(str_of(&fold, "name"), "Report from Kitten 1");
        assert!(fold["states"].to_string().contains("collapsed"), "closed by default");
        assert!(!names(&rt).contains("Both pointed at the old site"), "closed by default");
        click(&mut rt, &format!("{r1}-trigger"));
        let all = names(&rt);
        assert!(all.contains("Both pointed at the old site"), "the summary: {all}");
        assert!(all.contains("docs/setup.md +1 -1") && all.contains("docs/faq.md +1 -1"), "the files, one each");
        assert!(all.contains("neko/k3x9p1"), "the branch");
        // (The conversation sticks to its end, so the line has moved when the report opened: shut it as the key does.)
        let k = r1.clone();
        rt.update(move |s, _| s.flip(&k));
        for _ in 0..10 {
            rt.frame(0.1);
        }
        assert!(!rt.state.is_open(&r1), "and shut again");
        assert!(!texts(&rt).iter().any(|t| t.contains("Both pointed at the old site")), "{:?}", texts(&rt));
    }
    // In order: the notes come after the first run, before the second run's turn, and the run's own fold is whole.
    let rt = runtime((1200.0, 800.0), false, scene("subagents-wake"));
    let w = wake_seqs(&rt);
    let (a, b, turn) = (el(&rt, &format!("wake-{}", w[0])), el(&rt, &format!("wake-{}", w[1])), el(&rt, "turn-run_2"));
    assert!(a.rect[1] < b.rect[1] && b.rect[1] < turn.rect[1], "in order");
    assert!(has_key(&rt, "worked-run_2") && has_key(&rt, "worked-run_1"), "each run folds as before");
    assert!(texts(&rt).iter().any(|x| x.starts_with("Both reports are in")), "the final words stay open");
}

#[test]
fn the_waiting_line_names_the_live_subagents_as_links_that_open_them() {
    let mut rt = runtime((1200.0, 800.0), false, scene("subagents-waiting"));
    let line = node(&rt, "waiting-line");
    assert_eq!(str_of(&line, "name"), "Waiting on Kitten 1 and Kitten 2");
    assert_eq!(str_of(&node(&rt, "waiting-link-1"), "name"), "Kitten 1");
    assert_eq!(str_of(&node(&rt, "waiting-link-2"), "role"), "link");
    // It is the conversation's last line.
    let (l, c) = (el(&rt, "waiting-line"), el(&rt, "subagent-card-2"));
    assert!(l.rect[1] > c.rect[1]);
    click(&mut rt, "waiting-link-2");
    let root = rt.state.pane_root().expect("a thread is open");
    assert_eq!(rt.state.tab_of(&root), neko_app::pane::Tab::Kitten(neko_protocol::TaskId(sub_id(2))), "the link opens that subagent's tab");
    assert!(rt.state.open_subagent().is_none(), "the thread stays in the main view");
    // Three: commas, then "and"; nothing waits: the line is gone.
    let mut rt = runtime((1200.0, 800.0), false, scene("subagents-waiting"));
    rt.update(|s, _| {
        let mut third = s.tasks.iter().find(|t| t.id.0 == sub_id(2)).unwrap().clone();
        third.id = neko_protocol::TaskId(sub_id(3));
        third.subagent.as_mut().unwrap().number = "3".into();
        third.title = "Check headings".into();
        s.tasks.push(third);
    });
    settle(&mut rt);
    assert_eq!(str_of(&node(&rt, "waiting-line"), "name"), "Waiting on Kitten 1, Kitten 2 and Kitten 3");
    rt.update(|s, _| s.tasks.iter_mut().find(|t| t.title == "Fix login").unwrap().status = TaskStatus::Done);
    settle(&mut rt);
    assert!(!has_key(&rt, "waiting-line"), "only while the thread waits");
}

#[test]
fn a_waiting_thread_can_be_stopped_like_a_working_one() {
    let rt = runtime((1200.0, 800.0), false, scene("subagents-waiting"));
    assert!(rt.state.open_is_working(), "Stop and Escape reach a thread that rests on its subagents");
    assert_eq!(str_of(&node(&rt, "reply-input"), "placeholder"), "The agent is waiting on its kittens. Press Escape to stop.");
    assert_eq!(str_of(&node(&rt, "reply-submit"), "name"), "Stop", "its composer offers Stop");
}

#[test]
fn a_subagent_opened_is_read_only_with_its_breadcrumb_back_and_stop() {
    for size in SIZES {
        let mut rt = runtime(size, false, scene("subagent-view"));
        assert!(!has_key(&rt, "reply") && !has_key(&rt, "reply-area-composer"), "no composer at {size:?}");
        let said = node(&rt, "subagent-readonly");
        assert_eq!(str_of(&said, "name"), "This is Kitten 1's own conversation, to read. Say what you want changed in Fix login.");
        assert_eq!(str_of(&node(&rt, "subagent-back"), "name"), "Back to Fix login");
        assert_eq!(str_of(&node(&rt, "subagent-stop"), "name"), "Stop Kitten 1");
        // The band: Fix login / Kitten 1, the thread a button, the subagent plain words.
        let title = node(&rt, "band-title");
        assert_eq!(str_of(&title, "name"), "Fix login / Kitten 1");
        assert_eq!(str_of(&title, "role"), "heading");
        assert_eq!(str_of(&node(&rt, "crumb-thread"), "role"), "button");
        let plain = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("crumb-subagent")).unwrap();
        assert_eq!(plain.text.as_deref(), Some("Kitten 1"));
        assert!(!has_key(&rt, "task-cat") && !has_key(&rt, "task-more-button"), "no cat and no task menu in a subagent's band");
        assert_eq!(rt.state.window_title(), "Fix login / Kitten 1 — acme-api");
        // No cat portrait leads its turns.
        assert!(!rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("turn-") && k.ends_with("-cat"))), "no head");
        // The thread's title goes back.
        click(&mut rt, "crumb-thread");
        assert_eq!(rt.state.open.as_ref().map(|i| i.0.clone()), Some(format!("task_{:026}", 30)));
    }
    // Back, and Escape, do the same; Stop shows only while it works.
    let mut rt = runtime((1200.0, 800.0), false, scene("subagent-view"));
    click(&mut rt, "subagent-back");
    assert_eq!(rt.state.open.as_ref().map(|i| i.0.clone()), Some(format!("task_{:026}", 30)));
    let mut rt = runtime((1200.0, 800.0), false, scene("subagent-view"));
    press(&mut rt, Key::Escape, Mods::default());
    assert_eq!(rt.state.open.as_ref().map(|i| i.0.clone()), Some(format!("task_{:026}", 30)), "Escape goes back, it does not stop the subagent");
    let mut rt = runtime((1200.0, 800.0), false, scene("subagent-view"));
    rt.update(|s, _| s.tasks.iter_mut().find(|t| t.id.0 == sub_id(1)).unwrap().status = TaskStatus::Done);
    settle(&mut rt);
    assert!(!has_key(&rt, "subagent-stop") && has_key(&rt, "subagent-back"));
}

#[test]
fn a_subagents_request_is_answered_in_place_and_says_who_asks() {
    let rt = runtime((1200.0, 800.0), false, scene("subagent-asks"));
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "Kitten 1 asks"), "{t:?}");
    assert!(!t.iter().any(|x| x == "The agent asks"), "the kitten asks, not the agent");
    for k in ["allow", "always", "deny"] {
        assert!(rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|key| key.starts_with("perm-") && key.ends_with(&format!("-{k}")))), "{k}");
    }
    assert!(!rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("perm-") && k.ends_with("-face"))), "no cat portrait on the request");
}

#[test]
fn needs_you_reads_subagent_1_asks_with_the_thread_as_its_quiet_line_and_opens_the_subagent() {
    let mut rt = runtime((1200.0, 800.0), false, scene("needs-subagent"));
    let item = rt.state.needs.iter().find(|i| i.subagent.is_some()).unwrap().clone();
    let key = need_key(&item.id);
    let row = node(&rt, &key);
    assert!(str_of(&row, "name").starts_with("Kitten 1 asks: ") && str_of(&row, "name").contains("Run a command"), "{}", str_of(&row, "name"));
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "Kitten 1 asks") && t.iter().any(|x| x == "in Fix login"), "{t:?}");
    assert_eq!(str_of(&node(&rt, &format!("{key}-open")), "name"), "Open Kitten 1");
    click(&mut rt, &format!("{key}-open"));
    assert_eq!(rt.state.open.as_ref(), Some(&item.task), "the row opens the subagent");
    assert!(has_key(&rt, "subagent-readonly"));
    // Its notification: the number asks, not a cat.
    let note = neko_app::notify::note_for(&item);
    assert!(note.body.starts_with("Needs you: Kitten 1 asks: "), "{}", note.body);
}

#[test]
fn a_subagent_finishing_never_notifies_but_its_thread_finishing_does() {
    let rec = Recorder::default();
    let mut rt = runtime((1200.0, 800.0), false, scene("subagents-waiting"));
    let r = rec.clone();
    rt.update(move |s, _| {
        s.notifier = Notifier::new(Box::new(r));
        s.front = false;
    });
    let sub = rt.state.tasks.iter().find(|t| t.id.0 == sub_id(1)).cloned().unwrap();
    let thread = rt.state.tasks.iter().find(|t| t.title == "Fix login").cloned().unwrap();
    let _ = rec.take();
    rt.update(move |s, _| {
        let mut done = sub.clone();
        done.status = TaskStatus::Done;
        s.change(Change::TaskUpdated { task: done });
    });
    assert!(rec.take().iter().all(|x| !matches!(x, Sent::Post(_))), "nothing posted for a subagent");
    rt.update(move |s, _| {
        let mut working = thread.clone();
        working.status = TaskStatus::Working;
        s.change(Change::TaskUpdated { task: working.clone() });
        working.status = TaskStatus::Done;
        s.change(Change::TaskUpdated { task: working });
    });
    assert!(rec.take().iter().any(|x| matches!(x, Sent::Post(_))), "the thread's own end is news");
}

#[test]
fn settings_plugins_has_the_switch_for_the_persons_own_mcp_servers_off_by_default() {
    let mut rt = runtime((1200.0, 800.0), false, scene("settings"));
    rt.update(|s, _| s.open_plugins(None));
    settle(&mut rt);
    let sw = node(&rt, "own-mcp");
    assert_eq!(str_of(&sw, "name"), "Also give runs my own Claude Code MCP servers");
    assert!(!sw.get("states").and_then(|s| s.as_array()).is_some_and(|s| s.iter().any(|x| x == "checked")), "off by default: {sw}");
    assert!(names(&rt).contains("Off: a run gets only Neko's own tool and the plugins you approved, which keeps runs quick. On: runs also load the MCP servers in your Claude Code settings."));
    // It is the first thing on the page, above the plugins.
    let (a, b) = (el(&rt, "own-mcp"), el(&rt, "plugins-empty"));
    assert!(a.rect[1] < b.rect[1]);
}

#[test]
fn a_part_that_marks_a_subagent_or_a_wake_is_not_a_step_and_does_not_count_as_one() {
    let rt = runtime((1200.0, 800.0), false, scene("subagents-wake"));
    let book = rt.state.book().unwrap();
    assert!(book.parts.iter().any(|p| matches!(p.body, PartBody::Subagent { .. })) && book.parts.iter().any(|p| matches!(p.body, PartBody::Wake { .. })));
    // The first run has the two start calls and the read: three steps, as the fold says (the cards are not steps).
    assert!(names(&rt).contains("3 steps"), "{}", names(&rt));
}

// ---- The switch against a real daemon ---------------------------------------------------------------------------------------------------

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
    let end = Instant::now() + Duration::from_secs(40);
    while !ok(rt) {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
        rt.frame(0.0);
    }
    rt.frame(0.0);
}

#[test]
fn the_switch_sends_set_settings_with_own_mcp_and_keeps_the_other_fields() {
    let bin = PathBuf::from(std::env::var("NEKO_DAEMON_BIN").expect("set NEKO_DAEMON_BIN to a built neko-daemon (scripts/check.sh does)"));
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let mut folder = Folder { dir: std::env::temp_dir().join(format!("neko-subagents-e2e-{}-{nanos}", std::process::id())), pids: Vec::new() };
    let dir = folder.dir.clone();
    let home = dir.join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    // SAFETY: set before any thread of this test reads the environment; this is the one test in this binary that starts a daemon.
    unsafe {
        std::env::set_var("NEKO_CLAUDE_BIN", bin.with_file_name("neko-fake-claude"));
        std::env::set_var("HOME", &home);
    }
    let mut rt = runtime((1200.0, 800.0), false, |_| {});
    let conn = neko_app::link::Conn::start(dir.clone(), Some(bin), || {});
    rt.update(move |s, _| {
        s.daemon = Daemon::Starting;
        s.conn = Some(conn);
    });
    until(&mut rt, "the app to connect", |rt| rt.state.daemon == Daemon::Connected && rt.state.runtimes.is_some());
    folder.pids.extend(neko_protocol::client::find(&dir).ok().map(|(i, _)| i.pid as i32));
    let before = rt.state.runtimes.as_ref().unwrap().1.clone();
    assert!(!before.own_mcp, "off by default");
    rt.update(|s, _| s.open_plugins(None));
    settle(&mut rt);
    click(&mut rt, "own-mcp");
    until(&mut rt, "the setting to change", |rt| rt.state.runtimes.as_ref().is_some_and(|(_, s)| s.own_mcp));
    let after = rt.state.runtimes.as_ref().unwrap().1.clone();
    assert_eq!((after.runtime.clone(), after.permission_mode), (before.runtime.clone(), before.permission_mode), "the other fields are kept");
    click(&mut rt, "own-mcp");
    until(&mut rt, "the setting to go off again", |rt| rt.state.runtimes.as_ref().is_some_and(|(_, s)| !s.own_mcp));
}

#[test]
fn a_subagents_request_turns_its_threads_mark_amber_and_the_right_side_says_the_same_as_the_mark() {
    let rt = runtime((1200.0, 800.0), false, scene("needs-subagent"));
    let thread = rt.state.tasks.iter().find(|t| t.subagent.is_none() && t.status == TaskStatus::Waiting).expect("the waiting thread");
    assert!(thread.needs_you, "a request one of its subagents has waiting is the thread's to show");
    assert_eq!(neko_app::status_mark::mark_of(thread), neko_app::status_mark::Mark::NeedsYou, "the amber dot, not the still ring");
    let all = texts(&rt);
    assert!(all.iter().any(|t| t.starts_with("Needs you: ") && t.ends_with("· Kitten 1")), "the row's second line names the subagent: {all:?}");
    // The right of a row says what its mark says: a thread behind an amber dot shows its age, never "Working".
    let key = format!("thread-{}", thread.id.0);
    let n = node(&rt, &key);
    assert!(!str_of(&n, "description").ends_with("working"), "{}", str_of(&n, "description"));
}
