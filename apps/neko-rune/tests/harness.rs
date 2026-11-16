//! The session view, the Neko column and the sessions column driven through Rune's headless
//! `Runtime` (real layout, real hit-testing, scripted pointer and keys, no window) against a real
//! daemon: the verification harness `web_harness`, which runs the daemon's own `server` module over
//! an isolated data folder under `/tmp` with a made-up workbench, no agents and never the person's
//! Neko. Requests are answered on the calling thread (`Dispatch::Blocking`), so a scripted press
//! sees the daemon's answer in the next frame.
//!
//! Needs the harness binary (`cargo build -p neko-daemon --bin web_harness`; set `NEKO_WEB_HARNESS`
//! to its path, or leave it in `~/Library/Caches/neko/target/debug` or the repo's `target/debug`)
//! and `node` (it writes the seed). Without them each test says so and passes, so a Mac that has
//! not built the daemon can still run the rest; `NEKO_REQUIRE_HARNESS=1` makes that a failure.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use neko_protocol::workbench::{Command as Wb, Workspace};
use neko_protocol::{Request, Response};
use neko_rune::app::{App, Dispatch, Steer};
use neko_rune::link::{Call, ask};
use neko_rune::view::view;
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const WINDOW: (f32, f32) = (1360.0, 900.0);

struct Harness {
    data: PathBuf,
    child: Child,
}

impl Drop for Harness {
    fn drop(&mut self) {
        // By the pid this test started, never by name.
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.data);
        let _ = std::fs::remove_dir_all(format!("{}-web", self.data.display()));
        let _ = std::fs::remove_file(format!("{}-seed.json", self.data.display()));
    }
}

fn harness_binary() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("NEKO_WEB_HARNESS") {
        return Some(PathBuf::from(p)).filter(|p| p.exists());
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    [home.join("Library/Caches/neko/target/debug/web_harness"), repo.join("target/debug/web_harness")].into_iter().find(|p| p.exists())
}

fn unique() -> usize {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Starts an isolated daemon, or None (with a line saying why) when this Mac cannot.
fn start(extra_env: &[(&str, &str)]) -> Option<Harness> {
    let Some(binary) = harness_binary() else {
        assert!(std::env::var_os("NEKO_REQUIRE_HARNESS").is_none(), "NEKO_REQUIRE_HARNESS is set and web_harness is not built");
        eprintln!("skipped: web_harness is not built (cargo build -p neko-daemon --bin web_harness)");
        return None;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let data = PathBuf::from(format!("/tmp/nrh-{}-{}", std::process::id(), unique()));
    let web = PathBuf::from(format!("{}-web", data.display()));
    let seed = PathBuf::from(format!("{}-seed.json", data.display()));
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&web).unwrap();
    std::fs::write(web.join("index.html"), "<html></html>").unwrap();
    let wrote = Command::new("node")
        .current_dir(&repo)
        .args(["--input-type=module", "-e", "import { webSeed } from './scripts/fixtures/web-seed.mjs'; import fs from 'node:fs'; fs.writeFileSync(process.argv[1], JSON.stringify(webSeed()));"])
        .arg(&seed)
        .status();
    if !wrote.is_ok_and(|s| s.success()) {
        assert!(std::env::var_os("NEKO_REQUIRE_HARNESS").is_none(), "node could not write the seed");
        eprintln!("skipped: node could not write the harness seed");
        let _ = std::fs::remove_dir_all(&data);
        return None;
    }
    let mut command = Command::new(binary);
    command
        .current_dir(&data)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", &data)
        .env("NEKO_DATA_DIR", &data)
        .env("NEKO_WEB_PORT", "0")
        .env("NEKO_WEB_DIR", &web)
        .env("NEKO_WEB_SEED", &seed)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in extra_env {
        command.env(k, v);
    }
    let child = command.spawn().expect("the harness starts");
    let harness = Harness { data: data.clone(), child };
    let socket = data.join("neko.sock");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !socket.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(socket.exists(), "the harness did not start");
    Some(harness)
}

fn socket(h: &Harness) -> PathBuf {
    h.data.join("neko.sock")
}

/// An app that talks to the harness on the calling thread, and the first things a window reads.
fn app(h: &Harness) -> App {
    let mut app = App::headless();
    app.dispatch = Dispatch::Blocking { socket: socket(h) };
    for call in [Call::Refresh, Call::Unseen, Call::Inbox, Call::Digest] {
        app.call(call);
    }
    app
}

fn runtime(app: App) -> Runtime<App> {
    let mut rt = Runtime::new(app, view, WINDOW, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    settle(&mut rt);
    rt
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

fn press(rt: &mut Runtime<App>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    settle(rt);
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

fn has_text(rt: &Runtime<App>, needle: &str) -> bool {
    texts(rt).iter().any(|t| t.contains(needle))
}

fn keyed(rt: &Runtime<App>, prefix: &str) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.key).filter(|k| k.starts_with(prefix)).collect()
}

fn id_of(rt: &Runtime<App>, title: &str) -> String {
    rt.state.summary.as_ref().unwrap().tasks.iter().find(|t| t.title == title).unwrap_or_else(|| panic!("no task titled {title}")).id.clone()
}

fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    panic!("timed out waiting for {what}");
}

#[test]
fn selecting_a_session_reads_its_turns_from_the_daemon() {
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    let id = id_of(&rt, "Rework the export batching");
    click(&mut rt, &format!("session-{id}"));
    assert_eq!(rt.state.selected.as_deref(), Some(id.as_str()));
    let detail = rt.state.detail().expect("the daemon sent the open session in full");
    assert_eq!(detail.task.id, id);
    // The brief is the first turn, the agents' words follow, and the top line names it.
    assert!(has_text(&rt, "Rework the export batching"));
    assert!(rt.find("turns").is_some() && rt.find("composer-input").is_some());
    assert!(has_text(&rt, "Codex") || has_text(&rt, "default model"), "{:?}", texts(&rt));
}

#[test]
fn a_reply_streams_into_neko_s_column_word_by_word_and_a_tool_card_opens() {
    let Some(h) = start(&[("NEKO_WEB_STREAM", "1")]) else { return };
    let mut rt = runtime(app(&h));
    let length = |rt: &Runtime<App>| rt.state.summary.as_ref().and_then(|m| m.conversation.last()).map(|m| neko_rune::turns::blocks_of_parts(&m.parts)).map_or(0, |b| b.iter().map(|b| if let neko_rune::turns::Block::Text(t) = b { t.len() } else { 0 }).sum::<usize>());
    wait_for("the in-flight reply to start", || {
        rt.update(|s, _| s.call(Call::Refresh));
        length(&rt) > 0
    });
    let first = length(&rt);
    let mut grew = false;
    wait_for("the reply to grow", || {
        rt.update(|s, _| s.call(Call::Refresh));
        grew = length(&rt) > first;
        grew
    });
    settle(&mut rt);
    assert!(grew && rt.state.summary.as_ref().unwrap().replying(), "the reply is still being written and shows more each time");
    assert!(has_text(&rt, "Neko is replying…"), "{:?}", texts(&rt));
    // A finished reply's command card: one line, then its input and output.
    let cards = keyed(&rt, "neko-tool-");
    assert!(!cards.is_empty(), "the seeded reply carries commands: {:?}", keyed(&rt, "neko-"));
    let card = cards.iter().find(|k| k.ends_with("h1")).cloned().unwrap_or_else(|| cards[0].clone());
    assert!(!has_text(&rt, "5 open") && !has_text(&rt, "101 Update the branch"), "closed cards show one line only");
    // The column sticks to the end of what Neko wrote; scroll back up to the earlier reply.
    let region = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("neko-chat-scroll")).map(|e| e.rect).unwrap();
    for _ in 0..40 {
        if rt.find(&card).is_some_and(|(_, y)| y > region[1] && y < region[1] + region[3]) {
            break;
        }
        rt.wheel(region[0] + 100.0, region[1] + 100.0, 120.0);
        settle(&mut rt);
    }
    click(&mut rt, &card);
    assert!(keyed(&rt, "neko-tooldetail-").len() == 1, "the card opened: open={:?}", rt.state.work.expanded);
    assert!(has_text(&rt, "$ gh pr list"), "its input is shown: {:?}", texts(&rt));
}

#[test]
fn a_runs_diff_opens_and_undo_asks_first_and_then_takes_the_run_back() {
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    let id = id_of(&rt, "Stop the nightly export running twice");
    click(&mut rt, &format!("session-{id}"));
    assert!(rt.state.work.checkpoints.get(&id).is_some_and(|l| l.runs.len() == 3 && l.restorable), "{:?}", rt.state.work.checkpoints.get(&id));
    for n in 1..=3 {
        assert!(rt.find(&format!("run-{n}")).is_some(), "run {n} has its line");
    }
    assert!(has_text(&rt, "Run 2"));
    // Open run 2: its own diff only, from the daemon.
    click(&mut rt, "run-2");
    assert!(!keyed(&rt, "run-diff-").is_empty() || rt.find("run-diffbox-").is_some() || !keyed(&rt, "run-diffbox-").is_empty());
    assert!(has_text(&rt, "export.sh"), "the file the run changed: {:?}", texts(&rt));
    // Undo run 1 is refused while later runs exist, with the offer the web gives.
    click(&mut rt, "undo-run-1");
    assert!(rt.find("confirm").is_some(), "undo asks first");
    click(&mut rt, "confirm-yes");
    assert!(rt.find("undo-refused").is_some(), "the daemon refused: {:?}", rt.state.work.refusal);
    assert_eq!(rt.state.work.refusal.as_ref().map(|r| r.code.as_str()), Some("later_runs"));
    assert!(rt.find("offer-through").is_some());
    click(&mut rt, "offer-dismiss");
    // Undo the last run: confirm, then the daemon restores the folder, and Redo appears.
    click(&mut rt, "undo-run-3");
    assert!(has_text(&rt, "Undo run 3?"), "{:?}", texts(&rt));
    click(&mut rt, "confirm-no");
    assert!(rt.find("confirm").is_none() && !rt.state.work.checkpoints[&id].runs[2].undone, "Keep it changes nothing");
    click(&mut rt, "undo-run-3");
    click(&mut rt, "confirm-yes");
    wait_for("the undo to show", || {
        rt.update(|s, _| s.call(Call::Checkpoints { task_id: id.clone() }));
        rt.state.work.checkpoints.get(&id).is_some_and(|l| l.runs.iter().any(|r| r.undone))
    });
    settle(&mut rt);
    assert!(rt.find("redo").is_some(), "Redo is offered after an undo");
    click(&mut rt, "redo");
    wait_for("the redo", || {
        rt.update(|s, _| s.call(Call::Checkpoints { task_id: id.clone() }));
        rt.state.work.checkpoints.get(&id).is_some_and(|l| l.runs.iter().all(|r| !r.undone))
    });
}

#[test]
fn a_message_queues_in_each_steer_mode_and_can_be_taken_back() {
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    let id = id_of(&rt, "Rework the export batching");
    click(&mut rt, &format!("session-{id}"));
    assert_eq!(rt.state.detail().map(|d| d.task.status), Some(neko_protocol::workbench::TaskStatus::Building));
    let say = |rt: &mut Runtime<App>, text: &str| {
        click(rt, "composer-input");
        rt.paste(text);
        settle(rt);
    };
    // After this turn (the default), by Enter.
    say(&mut rt, "first, after the turn");
    press(&mut rt, Key::Enter, Mods::default());
    // Steer now, by the menu then the button.
    rt.update(|s, _| s.steer = Steer::Now);
    say(&mut rt, "second, steer now");
    click(&mut rt, "send");
    // Interrupt and restart, from the menu.
    click(&mut rt, "steer-trigger");
    let e = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Interrupt and restart")).unwrap();
    rt.pointer_down(e.rect[0] + 5.0, e.rect[1] + e.rect[3] / 2.0);
    rt.pointer_up(e.rect[0] + 5.0, e.rect[1] + e.rect[3] / 2.0);
    settle(&mut rt);
    assert_eq!(rt.state.steer, Steer::Interrupt);
    say(&mut rt, "third, interrupt");
    click(&mut rt, "send");
    // Interrupt and restart is not queued: the daemon stops the turn and hands the message over at
    // once (`ReplyToTask`), so it leaves the box and never waits in the queue.
    assert!(rt.state.composer.text.is_empty() && rt.state.work.notice.is_none(), "{:?}", rt.state.work.notice);
    // ⌘↩ steers now whatever the menu says.
    rt.update(|s, _| s.steer = Steer::AfterTurn);
    say(&mut rt, "fourth, command return");
    press(&mut rt, Key::Enter, Mods { cmd: true, ..Mods::default() });

    let queued = rt.state.summary.as_ref().unwrap().queued_for(&id).into_iter().map(|m| (m.text.clone(), m.mode)).collect::<Vec<_>>();
    use neko_protocol::crew::DeliveryMode::*;
    assert_eq!(
        queued,
        vec![
            ("first, after the turn".to_owned(), AfterTurn),
            ("second, steer now".to_owned(), SteerNow),
            ("fourth, command return".to_owned(), SteerNow),
        ],
        "after-the-turn and steer-now wait in the daemon's durable queue"
    );
    assert!(rt.state.composer.text.is_empty(), "the text left the box once the daemon took it");
    assert_eq!(keyed(&rt, "queued-").len(), 3, "one line each");
    // Cancel takes one back.
    let first = rt.state.summary.as_ref().unwrap().queued_for(&id)[0].id.clone();
    click(&mut rt, &format!("cancel-queued-{first}"));
    assert_eq!(rt.state.summary.as_ref().unwrap().queued_for(&id).len(), 2);
    assert_eq!(keyed(&rt, "queued-").len(), 2);
}

#[test]
fn a_session_is_made_from_the_new_form_with_a_goal_and_opens() {
    let Some(h) = start(&[]) else { return };
    // Arrange the world: a workspace whose folder is a real Git repository (the harness seeds one).
    let repo = h.data.join("seed-repo");
    let before = app(&h);
    let workspace = before.summary.as_ref().unwrap().workspaces[0].clone();
    let _ = workspace;
    let reply = ask(&socket(&h), Request::Workbench(Wb::SaveWorkspaceWithFolders { workspace: Workspace { id: String::new(), name: "Sandbox".into(), repository: repo.to_string_lossy().into_owned(), instructions: String::new(), away_enabled: false }, folders: vec![repo.to_string_lossy().into_owned()] }));
    assert!(matches!(reply, Ok(Response::Workbench(_))), "{reply:?}");
    let mut rt = runtime(app(&h));
    click(&mut rt, "new-session");
    assert!(rt.find("new-form").is_some() && rt.find("new-request").is_some());
    assert!(rt.find("new-start").is_some() && rt.state.new_blocker() == Some("Say what you want done."), "Start waits for words");
    click(&mut rt, "new-request");
    rt.paste("Make the sync job retry when a request fails.\nThree tries, waiting longer each time.");
    settle(&mut rt);
    // Choose the new workspace from the menu.
    click(&mut rt, "new-workspace");
    assert!(has_text(&rt, "Sandbox"));
    let e = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Sandbox")).expect("its menu item");
    rt.pointer_down(e.rect[0] + 5.0, e.rect[1] + e.rect[3] / 2.0);
    rt.pointer_up(e.rect[0] + 5.0, e.rect[1] + e.rect[3] / 2.0);
    settle(&mut rt);
    assert_eq!(rt.state.summary.as_ref().unwrap().workspaces[rt.state.new_form.as_ref().unwrap().workspace].name, "Sandbox");
    // A goal with a check, as `/goal` makes one.
    let tasks_before = rt.state.summary.as_ref().unwrap().tasks.len();
    rt.update(|s, _| s.new_form.as_mut().unwrap().goal = true);
    settle(&mut rt);
    click(&mut rt, "new-check");
    rt.paste("cargo test");
    settle(&mut rt);
    click(&mut rt, "new-start");
    assert!(rt.state.new_form.is_none(), "the form closed: {:?}", rt.state.new_form.as_ref().and_then(|f| f.error.clone()));
    let summary = rt.state.summary.as_ref().unwrap();
    assert_eq!(summary.tasks.len(), tasks_before + 1, "one new session");
    let new = rt.state.selected.clone().unwrap();
    let task = summary.task(&new).unwrap();
    assert_eq!(task.title, "Make the sync job retry when a request fails", "the title is the first line");
    assert!(summary.goals.iter().any(|g| g.ticket_id.as_deref() == Some(new.as_str()) && g.done_check == "cargo test"), "the goal is on the new ticket");
    assert!(has_text(&rt, "Make the sync job retry when a request fails"), "the new session is open");
}

#[test]
fn an_inbox_default_is_answered_the_way_the_inbox_does() {
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    assert!(rt.state.neko.loaded && !rt.state.neko.inbox.is_empty(), "the routines' open items are in Neko's column");
    let with_default: Vec<_> = rt.state.neko.inbox.iter().filter(|i| i.default.is_some()).map(|i| i.seq).collect();
    assert_eq!(with_default, vec![1], "one seeded item carries a default");
    assert!(rt.find("inbox-use-1").is_some() && rt.find("inbox-use-2").is_none(), "only an item with a default has the button");
    assert!(has_text(&rt, "default: Approve it with a comment") && has_text(&rt, "No default"));
    click(&mut rt, "inbox-use-1");
    // The daemon answered; the note says what it did, and the item is gone from the open list.
    assert!(rt.state.neko.note.as_deref().is_some_and(|n| n.starts_with("Done:")), "{:?}", rt.state.neko.note);
    assert!(!rt.state.neko.inbox.iter().any(|i| i.seq == 1), "answered items leave the Inbox");
}

#[test]
fn asking_neko_streams_a_reply_into_the_column() {
    let Some(h) = start(&[("NEKO_WEB_SCRIPTED", "1")]) else { return };
    let mut rt = runtime(app(&h));
    // The seeded workbench has a reply in flight that never ends; Stop ends it, as it does for a person.
    assert!(rt.find("ask-stop").is_some(), "a reply in flight offers Stop");
    click(&mut rt, "ask-stop");
    wait_for("the in-flight reply to stop", || {
        rt.update(|s, _| s.call(Call::Refresh));
        !rt.state.summary.as_ref().unwrap().replying()
    });
    wait_for("the daemon to take a question", || {
        click(&mut rt, "ask");
        rt.paste("what needs me today?");
        settle(&mut rt);
        press(&mut rt, Key::Enter, Mods::default());
        rt.state.summary.as_ref().is_some_and(|m| m.conversation.iter().any(|c| c.role == neko_protocol::workbench::ChatRole::User && c.text().contains("what needs me today?")))
    });
    assert!(rt.state.ask.text.is_empty(), "the box emptied once the daemon took it");
    let mut lengths = Vec::new();
    wait_for("the scripted reply to finish", || {
        rt.update(|s, _| s.call(Call::Refresh));
        let last = rt.state.summary.as_ref().and_then(|m| m.conversation.last()).cloned();
        if let Some(m) = last.filter(|m| m.role == neko_protocol::workbench::ChatRole::Neko) {
            lengths.push(m.text().len());
            return !m.pending && m.text().contains("Three of the five");
        }
        false
    });
    settle(&mut rt);
    lengths.dedup();
    assert!(lengths.len() >= 2, "the reply was seen growing: {lengths:?}");
    assert!(has_text(&rt, "Three of the five only need an approving review"), "{:?}", texts(&rt));
}

#[test]
fn marking_unread_and_opening_move_the_dot_through_the_daemon() {
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    let id = id_of(&rt, "Add a retry to the webhook sender");
    click(&mut rt, &format!("session-{id}"));
    assert!(!rt.state.unseen.contains(&id));
    rt.update(|s, _| s.mark_unread(&id.clone()));
    settle(&mut rt);
    assert!(rt.find(&format!("unseen-{id}")).is_some(), "the dot is back");
    rt.update(|s, _| s.call(Call::Unseen));
    settle(&mut rt);
    assert!(rt.state.unseen.contains(&id), "the daemon keeps it");
    // Opening another and coming back marks it seen.
    let other = id_of(&rt, "Rework the export batching");
    click(&mut rt, &format!("session-{other}"));
    click(&mut rt, &format!("session-{id}"));
    rt.update(|s, _| s.call(Call::Unseen));
    settle(&mut rt);
    assert!(!rt.state.unseen.contains(&id) && rt.find(&format!("unseen-{id}")).is_none());
}
