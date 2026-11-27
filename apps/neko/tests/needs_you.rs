//! "Needs you" in the real app, headless: the list and its keys, the sidebar's counts and the badge's, the deep link; and, against a
//! real daemon on a temporary data folder with the fake CLI, answering from the list (a request, a question, a stale answer) and
//! which notifications would be posted. Notifications go to a recorder: no test posts a real one.
//!
//! The daemon test needs `NEKO_DAEMON_BIN` (scripts/check.sh builds it and sets it). Every daemon started here is stopped by the PID
//! it wrote, pass or fail; the real data folder is never touched.

mod support;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use neko_app::notify::{ALLOW_ONCE, Button, Notifier, Recorder, Sent};
use neko_app::state::{App, Daemon, need_key};
use neko_protocol::{Need, TaskId, TaskStatus};
use rune::{Key, Mods, Runtime};
use support::*;

fn scene(name: &str) -> fn(&mut App) {
    scenes().iter().find(|s| s.0 == name).unwrap().1
}

fn focus(rt: &Runtime<App>) -> Option<String> {
    rt.presented_focus_key().map(str::to_owned)
}

fn el(rt: &Runtime<App>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key))
}

#[test]
fn command_2_opens_the_list_with_the_keyboard_on_its_oldest_item_and_the_arrows_move() {
    let mut rt = runtime((1200.0, 800.0), false, scene("workspaces"));
    press(&mut rt, Key::Command('2'), Mods { cmd: true, ..Mods::default() });
    assert!(rt.state.needs_open);
    assert_eq!(rt.state.window_title(), "Needs you");
    press(&mut rt, Key::Command('1'), Mods { cmd: true, ..Mods::default() });
    assert!(!rt.state.needs_open && rt.state.open.is_none(), "⌘1 is Home");

    let mut rt = runtime((1200.0, 800.0), false, scene("needs-you"));
    let ids: Vec<String> = rt.state.visible_needs().iter().map(|i| i.id.clone()).collect();
    assert_eq!(focus(&rt), Some(need_key(&ids[0])), "the oldest item has the keyboard");
    press(&mut rt, Key::Down, Mods::default());
    assert_eq!(focus(&rt), Some(need_key(&ids[1])));
    press(&mut rt, Key::Down, Mods::default());
    assert_eq!(focus(&rt), Some(need_key(&ids[2])), "Down crosses from one project's items to the next");
    press(&mut rt, Key::Up, Mods::default());
    assert_eq!(focus(&rt), Some(need_key(&ids[1])));
    // Return on a question goes to its reply box; on a failure, to its task.
    press(&mut rt, Key::Enter, Mods::default());
    assert!(rt.state.needs_replies.get(&ids[1]).is_some_and(|r| r.focused), "Return on a question is its reply box");
    let mut rt = runtime((1200.0, 800.0), false, scene("needs-you"));
    press(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.open.as_ref(), Some(&rt.state.needs[0].task), "Return on a failure opens its thread");
    // A destructive request has no default: Return answers nothing.
    let mut rt = runtime((1200.0, 800.0), false, scene("needs-you"));
    let last = ids.last().unwrap().clone();
    for _ in 0..ids.len() {
        press(&mut rt, Key::Down, Mods::default());
    }
    assert_eq!(focus(&rt), Some(need_key(&last)), "the list stops at its end");
    press(&mut rt, Key::Enter, Mods::default());
    assert!(rt.state.needs_open && rt.state.visible_needs().len() == ids.len(), "nothing answered, nothing opened");
    press(&mut rt, Key::Escape, Mods::default());
    assert!(!rt.state.needs_open, "Escape goes back");
}

#[test]
fn the_list_is_grouped_one_column_and_says_how_long_each_item_waited() {
    let rt = runtime((1200.0, 800.0), false, scene("needs-you"));
    let all = nodes(&rt.accessibility_json());
    let lists: Vec<&serde_json::Value> = all.iter().map(|(_, n)| n).filter(|n| str_of(n, "role") == "list" && str_of(n, "name").starts_with("Waiting in")).collect();
    assert_eq!(lists.iter().map(|n| str_of(n, "name")).collect::<Vec<_>>(), ["Waiting in No project", "Waiting in acme-api", "Waiting in Acme marketing website redesign for the spring launch"]);
    let rows: Vec<&serde_json::Value> = all.iter().map(|(_, n)| n).filter(|n| str_of(n, "key").starts_with("need-") && n.get("position").is_some()).collect();
    assert_eq!(rows.len(), 5, "every item is a row with its place");
    assert!(str_of(rows[0], "description").contains("No project, waiting 3 days"), "{}", rows[0]);
    assert!(str_of(rows[3], "name").starts_with("The agent asks: Answer the question about the retry limit"), "led by a mark, saying who asks: {}", rows[3]);
    // The destructive request keeps no default and no Always allow.
    let hero = need_key(&rt.state.needs[4].id);
    assert!(el(&rt, &format!("{hero}-allow")).is_some() && el(&rt, &format!("{hero}-always")).is_none());
    let copy = need_key(&rt.state.needs[0].id);
    assert!(el(&rt, &format!("{copy}-command-copy")).is_some(), "the sign-in line has Copy");
    // One column: every row has the same left edge and width.
    let rects: Vec<[f32; 4]> = rt.state.needs.iter().filter_map(|i| el(&rt, &need_key(&i.id)).map(|e| e.rect)).collect();
    assert!(rects.windows(2).all(|w| (w[0][0] - w[1][0]).abs() < 0.5 && (w[0][2] - w[1][2]).abs() < 0.5), "{rects:?}");
    let empty = runtime((760.0, 560.0), false, scene("needs-you-empty"));
    assert!(el(&empty, "needs-empty").is_some_and(|e| e.text.unwrap_or_default().starts_with("Nothing needs you")), "one quiet line");
}

#[test]
fn the_sidebar_and_the_dock_count_what_needs_the_person() {
    let mut rt = runtime((1200.0, 800.0), false, scene("needs-you"));
    let all = nodes(&rt.accessibility_json());
    let row = all.iter().map(|(_, n)| n).find(|n| str_of(n, "key") == "needs-you").expect("the sidebar's row");
    assert_eq!(str_of(row, "description"), "5 waiting");
    let acme = all.iter().map(|(_, n)| n).find(|n| str_of(n, "key") == "ws-ws_acme").expect("acme-api's header");
    assert_eq!(str_of(acme, "description"), "2 need you", "a section counts what needs the person there, not what works");
    let web = all.iter().map(|(_, n)| n).find(|n| str_of(n, "key") == "ws-ws_web").unwrap();
    assert_eq!(str_of(web, "description"), "1 needs you", "its working thread is not counted");
    assert_eq!(rt.state.badge_count(), 5);
    rt.update(|s, _| {
        let first = s.needs[0].id.clone();
        s.needs_hidden.insert(first);
    });
    assert_eq!(rt.state.badge_count(), 4, "an item answered here leaves the count at once");
    rt.update(|s, _| s.notifier.switches.badge = false);
    assert_eq!(rt.state.badge_count(), 0, "the switch clears the badge");
    let empty = runtime((1200.0, 800.0), false, scene("needs-you-empty"));
    assert_eq!(empty.state.badge_count(), 0, "cleared at zero");
}

#[test]
fn a_link_opens_its_task_now_or_once_the_tasks_arrive() {
    let mut rt = runtime((1200.0, 800.0), false, scene("tasks-home"));
    let id = rt.state.tasks[2].id.clone();
    assert!(rt.state.open_link(&neko_app::deeplink::url(&id)));
    assert_eq!(rt.state.open.as_ref(), Some(&id));
    assert!(!rt.state.open_link("https://example.com/task/1"), "not one of Neko's");
    rt.update(|s, _| {
        s.tasks_loaded = true;
        s.open_link("neko://task/task_ZZZZZZZZZZZZZZZZZZZZZZZZZZ");
    });
    assert!(rt.state.open.is_none() && rt.state.notice.as_deref() == Some("That thread is not in Neko any more."));
    // Before the daemon's list: kept, and opened when it comes.
    let mut a = App::default();
    a.open_link(&neko_app::deeplink::url(&id));
    assert_eq!((a.open.as_ref(), a.pending_link.as_ref()), (None, Some(&id)));
    let tasks = neko_app::scenes::sample_tasks();
    a.take(neko_app::link::Incoming::Message(neko_protocol::ServerMessage::Reply { id: 1, reply: neko_protocol::Reply::Tasks { tasks } }));
    assert_eq!(a.open.as_ref(), Some(&id));
}

/// The data folder and every daemon that ran in it, stopped and removed on drop.
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
    let end = Instant::now() + Duration::from_secs(20);
    while !ok(rt) {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
        rt.frame(0.0);
    }
    rt.frame(0.0);
}

fn posts(sent: &[Sent]) -> Vec<&neko_app::notify::Note> {
    sent.iter().filter_map(|s| if let Sent::Post(n) = s { Some(n) } else { None }).collect()
}

#[test]
fn answering_from_the_list_and_from_notifications_against_a_real_daemon() {
    let bin = PathBuf::from(std::env::var("NEKO_DAEMON_BIN").expect("set NEKO_DAEMON_BIN to a built neko-daemon (scripts/check.sh does)"));
    let fake = bin.with_file_name("neko-fake-claude");
    assert!(fake.is_file(), "build the fake CLI first ({} is missing)", fake.display());
    // SAFETY: set before any thread of this test reads the environment; this is the one test in this binary that starts a daemon.
    unsafe { std::env::set_var("NEKO_CLAUDE_BIN", &fake) };
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let mut folder = Folder { dir: std::env::temp_dir().join(format!("neko-needs-e2e-{}-{nanos}", std::process::id())), pids: Vec::new() };
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
    until(&mut rt, "the app to connect", |rt| rt.state.daemon == Daemon::Connected && rt.state.needs_loaded);
    folder.pids.extend(neko_protocol::client::find(dir).ok().map(|(i, _)| i.pid as i32));
    assert!(rec.take().is_empty(), "nothing waits yet");

    // Two tasks; the person is in another app.
    for words in ["Run the tests [fake:permission]", "Rename the button [fake:question]"] {
        rt.update(move |s, _| {
            s.go_new_thread();
            s.draft = rune::TextEdit::new(words);
            s.submit();
        });
        until(&mut rt, "the thread to be made", |rt| rt.state.creating.is_none() && rt.state.tasks.iter().any(|t| t.words == words));
    }
    until(&mut rt, "both to need the person", |rt| rt.state.needs.len() == 2);
    assert_eq!(rt.state.badge_count(), 2);
    let sent = rec.take();
    let notes = posts(&sent);
    assert_eq!(notes.len(), 2, "one notification per item: {sent:?}");
    let perm_note = notes.iter().find(|n| n.get("permission").is_some()).expect("the request's notification");
    assert_eq!(perm_note.buttons.first(), Some(&Button::Action { id: ALLOW_ONCE, label: "Allow once" }));
    let q_note = notes.iter().find(|n| n.get("run").is_some()).expect("the question's notification");
    assert!(matches!(q_note.buttons[..], [Button::Reply { .. }]));
    let perm_info = perm_note.info.clone();

    // In the list: Allow once hides it at once, and the question takes the keyboard.
    press(&mut rt, Key::Command('2'), Mods { cmd: true, ..Mods::default() });
    let perm = rt.state.needs.iter().find(|i| matches!(i.need, Need::Permission { .. })).unwrap().clone();
    let question = rt.state.needs.iter().find(|i| matches!(i.need, Need::Question { .. })).unwrap().clone();
    click(&mut rt, &format!("{}-allow", need_key(&perm.id)));
    assert_eq!(rt.state.visible_needs().len(), 1, "answered: gone at once");
    assert_eq!(focus(&rt), Some(need_key(&question.id)), "the next item takes the keyboard");
    until(&mut rt, "the daemon to drop it", |rt| rt.state.needs.len() == 1);
    until(&mut rt, "the run to finish", |rt| rt.state.tasks.iter().any(|t| t.id == perm.task && t.status == TaskStatus::Done));
    let sent = rec.take();
    assert!(sent.contains(&Sent::Withdraw(vec![neko_app::notify::note_id(&perm.task)])), "its notification is taken back: {sent:?}");
    assert!(posts(&sent).iter().any(|n| n.body == "Done: The tests pass."), "the finished run says Done, with its first line, and names no cat: {sent:?}");

    // A notification's Allow once on what was already answered: nothing harmful, said plainly.
    rt.update(move |s, _| s.clicked(Some(ALLOW_ONCE.into()), None, perm_info));
    until(&mut rt, "the stale answer to be refused", |rt| !rt.state.asked.values().any(|a| matches!(a, neko_app::state::Asked::Answer { .. })));
    let sent = rec.take();
    assert!(posts(&sent).iter().any(|n| n.body == "That was already answered."), "{sent:?}");

    // The question, answered in its reply box with Return.
    let reply = format!("{}-reply-input", need_key(&question.id));
    click(&mut rt, &reply);
    press(&mut rt, Key::Text("All four.".into()), Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    assert!(rt.state.visible_needs().is_empty(), "answered: gone at once");
    until(&mut rt, "the list to be empty", |rt| rt.state.needs.is_empty() && rt.elements().iter().any(|e| e.key.as_deref() == Some("needs-empty")));
    assert_eq!(rt.state.badge_count(), 0);
    // The same answer again (from a notification left behind) is refused, not sent to the agent.
    let task = question.task.clone();
    let Need::Question { run, .. } = question.need.clone() else { unreachable!() };
    let info = vec![("task".to_owned(), task.0.clone()), ("run".to_owned(), run.0.clone())];
    rt.update(move |s, _| {
        s.front = true;
        s.clicked(Some("reply".into()), Some("All four, again.".into()), info);
    });
    until(&mut rt, "the stale reply to be refused", |rt| rt.state.needs_notice.as_deref() == Some("That was already answered."));
    let words = |rt: &Runtime<App>, t: &TaskId| rt.state.books.get(t).map_or(0, |b| b.parts.iter().filter(|p| matches!(&p.body, neko_protocol::PartBody::Person { text } if text.starts_with("All four"))).count());
    rt.update(move |s, _| s.open_task_id(&task));
    let t = question.task.clone();
    until(&mut rt, "the transcript", |rt| rt.state.books.get(&t).is_some_and(|b| b.loaded));
    assert_eq!(words(&rt, &question.task), 1, "the answer went once");

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
