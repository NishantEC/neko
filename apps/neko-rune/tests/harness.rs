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
    // The daemon's rows are as old as the seed run made them: read them against the real clock, as the window does.
    app.clock = neko_rune::app::system_ms;
    app.now_ms = neko_rune::app::system_ms();
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

/// Opens a session: pressed in the column when its row is there; the column shows at most 8 of what needs you and
/// 5 of what is recent, so a row may be behind "Show more" (the open session is what these tests are about).
fn pick(rt: &mut Runtime<App>, id: &str) {
    if rt.find(&format!("session-{id}")).is_some() {
        click(rt, &format!("session-{id}"));
    } else {
        let id = id.to_owned();
        rt.update(move |s, _| s.select(&id));
        settle(rt);
    }
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

/// Clicks a row of a long page (Settings shows a description under every row): scrolls the page until the row is in view.
fn click_in_page(rt: &mut Runtime<App>, key: &str) {
    let mut at = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    for _ in 0..40 {
        if at.1 > 260.0 && at.1 < 800.0 {
            break;
        }
        rt.wheel(800.0, 500.0, if at.1 >= 800.0 { -300.0 } else { 300.0 });
        settle(rt);
        at = rt.find(key).unwrap_or(at);
    }
    click(rt, key);
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
    pick(&mut rt, &id);
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
    pick(&mut rt, &id);
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
    pick(&mut rt, &id);
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
fn a_session_is_made_from_home_and_opens() {
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
    assert!(rt.find("home-composer").is_some() && rt.find("new-request").is_some());
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
    let tasks_before = rt.state.summary.as_ref().unwrap().tasks.len();
    click(&mut rt, "new-start");
    assert!(rt.state.new_form.is_none(), "the form closed: {:?}", rt.state.new_form.as_ref().and_then(|f| f.error.clone()));
    let summary = rt.state.summary.as_ref().unwrap();
    assert_eq!(summary.tasks.len(), tasks_before + 1, "one new session");
    let new = rt.state.selected.clone().unwrap();
    let task = summary.task(&new).unwrap();
    assert_eq!(task.title, "Make the sync job retry when a request fails", "the title is the first line");
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
    let label = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("inbox-use-1")).and_then(|e| e.label).unwrap_or_default();
    assert_eq!(label, "Use the default: Approve it with a comment", "the quick action names what it will do");
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
    pick(&mut rt, &id);
    assert!(!rt.state.unseen.contains(&id));
    rt.update(|s, _| s.mark_unread(&id.clone()));
    settle(&mut rt);
    // The row may be behind "View all" (Recent shows five), so the dot is asserted where it lives: in the unseen set.
    assert!(rt.state.unseen.contains(&id) || rt.find(&format!("unseen-{id}")).is_some(), "the dot is back");
    rt.update(|s, _| s.call(Call::Unseen));
    settle(&mut rt);
    assert!(rt.state.unseen.contains(&id), "the daemon keeps it");
    // Opening another and coming back marks it seen.
    let other = id_of(&rt, "Rework the export batching");
    pick(&mut rt, &other);
    pick(&mut rt, &id);
    rt.update(|s, _| s.call(Call::Unseen));
    settle(&mut rt);
    assert!(!rt.state.unseen.contains(&id) && rt.find(&format!("unseen-{id}")).is_none());
}

// ---- The P2 pages, each main action against the daemon's own answers.

fn open_page(rt: &mut Runtime<App>, view: neko_rune::views::FullView) {
    rt.update(|s, _| s.open_view(view));
    settle(rt);
}

fn first_routine(rt: &Runtime<App>) -> (String, bool, String) {
    let data = rt.state.pages.routines.as_ref().expect("the daemon sent its routines");
    let v = data.views.iter().find(|v| !neko_rune::pages::is_goal_routine(v)).expect("a routine");
    (v.routine.id.clone(), v.routine.enabled, v.routine.workspace_id.clone())
}

#[test]
fn the_routines_page_lists_the_daemons_routines_and_its_controls_ask_it() {
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    click(&mut rt, "view-inbox");
    let (id, was, workspace) = first_routine(&rt);
    assert!(rt.find(&format!("routine-{id}")).is_some(), "a line for each routine");
    // The switch asks the daemon and the line follows its answer.
    click_in_page(&mut rt, &format!("routine-switch-{id}"));
    assert_eq!(rt.state.pages.routine(&id).map(|v| v.routine.enabled), Some(!was));
    click_in_page(&mut rt, &format!("routine-switch-{id}"));
    assert_eq!(rt.state.pages.routine(&id).map(|v| v.routine.enabled), Some(was));
    // Opening a routine shows its editor; Run now makes it due.
    click_in_page(&mut rt, &format!("routine-{id}"));
    assert!(rt.find("rt-run").is_some() && rt.find("rt:instruction").is_some() && rt.find("rt-cadence").is_some() && rt.find("rt-catch-up-switch").is_some());
    click_in_page(&mut rt, "rt-run");
    assert!(rt.state.pages.routine(&id).is_some_and(|v| v.state.run_requested || v.state.running), "the daemon was asked to run it");
    // An edit is saved with the whole routine, and Save goes away.
    assert!(rt.find("rt-save").is_none(), "nothing to save yet");
    rt.update(|s, _| s.pages.set_field("rt:instruction", "A new instruction written by a test."));
    settle(&mut rt);
    click_in_page(&mut rt, "rt-save");
    assert_eq!(rt.state.pages.routine(&id).map(|v| v.routine.instruction.clone()).as_deref(), Some("A new instruction written by a test."));
    assert!(rt.find("rt-save").is_none(), "saved, so quiet again");
    // Standing orders are saved per workspace.
    rt.update(|s, _| {
        s.pages.set_field(&format!("orders:{workspace}"), "Keep replies short.");
        s.save_orders(&workspace);
    });
    settle(&mut rt);
    assert_eq!(rt.state.pages.routines.as_ref().and_then(|d| d.standing_orders.get(&workspace)).map(String::as_str), Some("Keep replies short."));
}

#[test]
fn settings_change_the_daemons_budgets_and_switches_and_search_finds_rows() {
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    rt.update(|s, _| s.open_view(neko_rune::views::FullView::Settings));
    settle(&mut rt);
    // Budget: the daily ceiling is a field with Save; empty takes the ceiling off.
    // Budget is a group of "AI & agents" in the web, so it is under that section.
    click(&mut rt, "settings-section-ai");
    rt.update(|s, _| s.pages.set_field("daily-budget", "7.50"));
    settle(&mut rt);
    click_in_page(&mut rt, "daily-budget-save");
    assert_eq!(rt.state.summary.as_ref().and_then(|m| m.settings.daily_budget_cents), Some(750));
    rt.update(|s, _| s.pages.set_field("daily-budget", "abc"));
    settle(&mut rt);
    click_in_page(&mut rt, "daily-budget-save");
    assert!(rt.state.pages.notes.contains_key("setting:daily-budget"), "an amount that is not one is refused next to its field");
    assert_eq!(rt.state.summary.as_ref().and_then(|m| m.settings.daily_budget_cents), Some(750), "and nothing was sent");
    rt.update(|s, _| s.pages.set_field("daily-budget", ""));
    settle(&mut rt);
    click_in_page(&mut rt, "daily-budget-save");
    assert_eq!(rt.state.summary.as_ref().and_then(|m| m.settings.daily_budget_cents), None);
    // AI and agents: plan before working.
    click(&mut rt, "settings-section-ai");
    let before = rt.state.summary.as_ref().is_some_and(|m| m.settings.plan_first);
    click_in_page(&mut rt, "set-plan-first-switch");
    assert_eq!(rt.state.summary.as_ref().map(|m| m.settings.plan_first), Some(!before));
    // General: Calendar on and off.
    click(&mut rt, "settings-section-general");
    click_in_page(&mut rt, "set-calendar-switch");
    assert!(rt.state.pages.calendar.as_ref().is_some_and(|c| c.status.enabled), "{:?}", rt.state.pages.calendar.as_ref().map(|c| &c.status));
    click_in_page(&mut rt, "set-calendar-switch");
    assert!(rt.state.pages.calendar.as_ref().is_some_and(|c| !c.status.enabled));
    // Phone: Reminders is a switch on the daemon's own settings.
    click(&mut rt, "settings-section-phone");
    let reminders = rt.state.pages.phone.as_ref().map(|p| p.settings.reminders).expect("the phone settings arrived");
    click_in_page(&mut rt, "set-reminders-switch");
    assert_eq!(rt.state.pages.phone.as_ref().map(|p| p.settings.reminders), Some(!reminders));
    click_in_page(&mut rt, "set-reminders-switch");
    // Search finds a row in another section, and says when nothing matches.
    rt.update(|s, _| s.pages.set_field("settings:search", "pause new work"));
    settle(&mut rt);
    assert!(rt.find("set-daily-budget").is_some() && rt.find("set-reminders").is_none());
    rt.update(|s, _| s.pages.set_field("settings:search", "zzzzqq"));
    settle(&mut rt);
    assert!(rt.find("settings-none").is_some());
}

#[test]
fn calendar_usage_workspaces_and_plugins_read_what_the_daemon_has() {
    use neko_rune::views::FullView;
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    open_page(&mut rt, FullView::Calendar);
    assert!(rt.state.pages.calendar.is_some(), "the week came from the daemon");
    assert!(rt.find("cal-grid").is_some() && rt.find("cal-mode").is_some());
    assert!(rt.find("cal-access").is_some(), "Calendar is off in a fresh data folder, and the page says so");
    click(&mut rt, "cal-next");
    click(&mut rt, "cal-today");
    open_page(&mut rt, FullView::Usage);
    assert!(rt.find("usage-today").is_some() && rt.state.pages.routines.is_some());
    assert!(rt.find("quota").is_some());
    open_page(&mut rt, FullView::Setup);
    let first = rt.state.summary.as_ref().and_then(|m| m.setup.workspaces.first()).map(|w| w.id.clone());
    if let Some(first) = first {
        assert!(rt.find(&format!("ws-{first}")).is_some(), "a line for each workspace");
        assert!(rt.find(&format!("ws-details-{first}")).is_some(), "a workspace shows expanded, with its details");
        click(&mut rt, &format!("ws-{first}"));
        assert!(rt.find(&format!("ws-details-{first}")).is_none(), "and folds when pressed");
    }
    open_page(&mut rt, FullView::Plugins);
    assert!(rt.find("full-plugins").is_some());
}

/// Saves a file as an artifact through the daemon, as an agent would; its id.
fn save_artifact(h: &Harness, title: &str, file: &Path, artifact_id: Option<String>) -> String {
    let request = Request::SaveArtifact { title: title.into(), file: file.to_string_lossy().into_owned(), workspace_id: None, artifact_id, note: String::new(), show: Some(false) };
    match ask(&socket(h), request) {
        Ok(Response::ArtifactSaved { artifact, .. }) => artifact.id,
        other => panic!("the daemon did not keep {title}: {other:?}"),
    }
}

#[test]
fn artifacts_of_every_built_in_kind_open_in_the_canvas_from_the_daemons_store() {
    use neko_rune::link::LinkEvent;
    use neko_rune::panel::{Content, Tab};
    use neko_rune::samples;
    let Some(h) = start(&[]) else { return };
    let dir = h.data.join("samples");
    std::fs::create_dir_all(&dir).unwrap();
    let files: Vec<(&str, &str, Vec<u8>, &str)> = vec![
        ("Board mockup", "png", samples::png(640, 400), "viewer-image"),
        ("Loading loop", "gif", samples::gif(6), "viewer-image"),
        ("Logo", "svg", samples::svg(), "viewer-svg"),
        ("Release notes", "md", samples::markdown().into_bytes(), "viewer-markdown"),
        ("Word count", "rs", samples::code().into_bytes(), "viewer-code"),
        ("Events", "csv", samples::csv(100_000).into_bytes(), "viewer-table"),
        ("Quarterly report", "pdf", samples::pdf(16), "viewer-pdf"),
        ("Torus knot", "glb", samples::glb(160, 24), "viewer-model3d"),
        ("Landing page", "html", b"<html><body>hello</body></html>".to_vec(), "viewer-web"),
        ("Dump", "bin", vec![0, 159, 146, 150, 0, 1, 2, 3], "viewer-fallback"),
    ];
    let mut saved = Vec::new();
    for (title, kind, bytes, key) in &files {
        let file = dir.join(format!("{}.{kind}", title.replace(' ', "-")));
        std::fs::write(&file, bytes).unwrap();
        saved.push((save_artifact(&h, title, &file, None), *title, *kind, *key));
    }
    // The board mockup gets a second version.
    std::fs::write(dir.join("Board-mockup-2.png"), samples::png(320, 200)).unwrap();
    save_artifact(&h, "", &dir.join("Board-mockup-2.png"), Some(saved[0].0.clone()));

    let mut app = app(&h);
    app.panel.data_dir = h.data.clone();
    let mut rt = runtime(app);
    // An artifact is shown for the session that is open.
    let session = id_of(&rt, "Rework the export batching");
    pick(&mut rt, &session);
    for (id, title, kind, key) in &saved {
        // The daemon shows it (an agent's `ShowArtifact`): the panel opens on the Canvas and reads it.
        rt.update(|s, _| {
            s.apply_link(LinkEvent::ShowArtifact { id: id.clone(), version: None });
        });
        settle(&mut rt);
        assert_eq!(rt.state.panel_tab(), Some(Tab::Canvas), "{title}");
        let state = rt.state.panel.open.as_ref().map(|o| match &o.content {
            Content::Loading => "loading".to_owned(),
            Content::Failed(m) => m.clone(),
            Content::Ready(_) => "ready".to_owned(),
            Content::TooLarge { .. } => "too large".to_owned(),
        });
        assert!(
            rt.find(key).is_some(),
            "{title} ({kind}) did not open in {key}: {:?} {state:?} {:?} selected={:?} shown={:?} open={} artifacts={:?} target={:?}",
            keyed(&rt, "viewer"),
            rt.state.outbox,
            rt.state.selected,
            rt.state.panel.session(&session).map(|p| (p.shown.len(), p.canvas.clone())),
            rt.state.layout.context.is_open(),
            rt.state.panel.artifacts.keys().collect::<Vec<_>>(),
            rt.state.panel_target()
        );
        assert!(matches!(rt.state.panel.open.as_ref().map(|o| &o.content), Some(Content::Ready(_))), "{title}");
    }
    // Every artifact is a row; the one with two versions opens on its newest and offers both.
    assert_eq!(keyed(&rt, "artifact-").len(), saved.len());
    rt.update(|s, _| {
        s.apply_link(LinkEvent::ShowArtifact { id: saved[0].0.clone(), version: None });
    });
    settle(&mut rt);
    let open = rt.state.panel.open.as_ref().unwrap();
    assert_eq!(open.target, neko_rune::panel::Target::Artifact { id: saved[0].0.clone(), version: 2 });
    assert!(has_text(&rt, "v2"));
    rt.update(|s, _| s.panel_pick_version(&saved[0].0, 1));
    settle(&mut rt);
    let open = rt.state.panel.open.as_ref().unwrap();
    assert_eq!(open.target, neko_rune::panel::Target::Artifact { id: saved[0].0.clone(), version: 1 }, "version 1 was read from the store");
    assert!(matches!(&open.content, Content::Ready(l) if l.size == samples::png(640, 400).len() as u64));
}

#[test]
fn the_files_tab_reads_the_sessions_folder_through_the_daemon_and_cannot_leave_it() {
    use neko_rune::panel::{Content, Tab, Target};
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    rt.state.panel.data_dir = h.data.clone();
    let id = id_of(&rt, "Stop the nightly export running twice");
    pick(&mut rt, &id);
    // The daemon names the folder; the client lists it and marks what the whole task changed.
    let folder = rt.state.panel.session(&id).and_then(|p| p.folder.clone()).flatten().expect("the daemon named the task's folder");
    assert_eq!(Path::new(&folder), h.data.join("task-worktrees").join(&id).canonicalize().unwrap());
    let meta = rt.state.panel.files_meta.as_ref().expect("the folder was listed");
    assert_eq!(meta.task_id, id);
    assert!(rt.state.panel.files_tree.contains("export.sh") && rt.state.panel.files_tree.contains("NOTES.md"));
    assert!(!rt.state.panel.files_tree.contains(".git"), "git's own files are not listed");
    assert_eq!(rt.state.panel.files_tree.get("export.sh").and_then(|n| n.data), Some(true), "a changed file");
    assert_eq!(rt.state.panel.files_tree.get("NOTES.md").and_then(|n| n.data), Some(true));
    assert_eq!(rt.state.panel_content_tabs(), [Tab::Diff, Tab::Files, Tab::Terminal].into_iter().filter(|t| rt.state.panel_content_tabs().contains(t)).collect::<Vec<_>>());
    // Open a file of the folder: the code viewer shows it.
    rt.update(|s, _| s.panel_choose_tab(Tab::Files));
    settle(&mut rt);
    rt.update(|s, _| s.panel_open_file("export.sh"));
    settle(&mut rt);
    assert!(rt.find("viewer-code").is_some() && has_text(&rt, "run_export"), "{:?}", texts(&rt));
    // The folder's own links and `..` lead nowhere outside it.
    let worktree = Path::new(&folder);
    std::os::unix::fs::symlink(h.data.join("seed-repo/export.sh"), worktree.join("leak.sh")).unwrap();
    for refused in ["leak.sh", "../seed-repo/export.sh", ".git"] {
        rt.update(|s, _| s.panel_open_file(refused));
        settle(&mut rt);
        let open = rt.state.panel.open.as_ref().expect("something is open");
        assert_eq!(open.target, Target::File { task_id: id.clone(), path: refused.into() });
        assert!(matches!(open.content, Content::Failed(_)), "{refused} was refused: {:?}", texts(&rt));
        assert!(!has_text(&rt, "run_export"), "{refused} showed a file it should not");
    }
    // A task with no folder has none to read.
    let other = id_of(&rt, "Rework the export batching");
    assert!(matches!(ask(&socket(&h), Request::TaskFolder { task_id: other }), Ok(Response::TaskFolder { path: None })));
    assert!(matches!(ask(&socket(&h), Request::TaskFolder { task_id: "no-such-task".into() }), Ok(Response::Error { .. })));
}

// ------------------------------------------------------------------------------------------------ parity batch A (the owner's brief of 2026-11-16)

/// A free loopback port for the harness's web server (the harness turns the web bridge off unless it is given one).
fn free_port() -> String {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port().to_string()
}

#[test]
fn batch_a_files_are_uploaded_over_http_and_found_for_a_mention() {
    let port = free_port();
    let Some(h) = start(&[("NEKO_WEB_PORT", port.as_str())]) else { return };
    wait_for("the web token", || h.data.join("web-token").exists());
    let mut rt = runtime(app(&h));
    let id = id_of(&rt, "Rework the export batching");
    pick(&mut rt, &id);
    let dir = std::env::temp_dir().join(format!("nrh-a-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("notes.txt");
    std::fs::write(&file, b"hello from the harness").unwrap();
    rt.update(|s, _| s.attach_files(neko_rune::compose::Dest::Reply, std::slice::from_ref(&file)));
    settle(&mut rt);
    assert!(rt.state.http.is_some(), "the daemon signed its address (SignedWebUrl) and the client took the token from it");
    let chips = rt.state.chips(neko_rune::compose::Dest::Reply).to_vec();
    assert_eq!(chips.len(), 1);
    let neko_rune::compose::ChipState::Ready { reference, is_image } = &chips[0].state else { panic!("{:?}", chips[0]) };
    assert!(!is_image && reference.starts_with("[notes.txt](file://") && reference.contains("/attachments/"), "{reference}");
    let kept = std::fs::read_dir(h.data.join("attachments")).unwrap().count();
    assert_eq!(kept, 1, "the daemon kept the file in its own attachments folder");
    // It rides in the message exactly as the web's composer sends it.
    rt.update(|s, _| s.composer.replace_text("see the notes"));
    let composed = rt.state.composed(neko_rune::compose::Dest::Reply, "see the notes");
    assert_eq!(composed, format!("see the notes\n{reference}"));
    // The `@` search is the web's `/api/files`: an answer (empty here: the harness's workspaces have no folders), never an error.
    let http = rt.state.http.clone().unwrap();
    assert!(http.files("note", "").is_ok());
    // A refused token is one calm sentence, not a panic.
    let wrong = neko_rune::link::http::Http::new(port.parse().unwrap(), &"0".repeat(64));
    assert!(wrong.upload("x.txt", b"x").unwrap_err().0.contains("401") || wrong.upload("x.txt", b"x").is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn batch_a_a_session_is_renamed_pinned_and_stopped_through_the_daemon() {
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    let id = id_of(&rt, "Rework the export batching");
    pick(&mut rt, &id);
    // Rename: Enter saves what the title field holds; the daemon's answer is the new title.
    rt.update(|s, _| s.do_action(&id, neko_rune::task_actions::Act::Rename));
    settle(&mut rt);
    rt.update(|s, _| s.rename.as_mut().unwrap().edit.replace_text("Batch the exports"));
    press(&mut rt, Key::Enter, Mods::default());
    assert!(rt.state.session().is_some_and(|t| t.title == "Batch the exports"), "{:?}", rt.state.session().map(|t| t.title.clone()));
    assert!(has_text(&rt, "Batch the exports"));
    // The model chip's request: pins on `task:<id>`. The harness has no connected model, so the daemon refuses an effort pin in its own
    // words, and the popover shows them verbatim (the web's `applyError`).
    rt.update(|s, _| {
        s.runtime_toggle();
        s.runtime.draft.reasoning_effort = Some("high".into());
        s.runtime.saving = true;
        let prefs = neko_protocol::workbench::RuntimePreferences { reasoning_effort: Some("high".into()), ..Default::default() };
        s.call(Call::SetRuntimePrefs { conversation_id: format!("task:{id}"), prefs });
    });
    settle(&mut rt);
    assert_eq!(rt.state.runtime.apply_error.as_deref(), Some("No connected model supports these settings. Refresh models or reset to Neko decides."));
    assert!(!rt.state.runtime.saving && has_text(&rt, "Settings were not applied."));
    // Resetting to "Neko decides" is accepted: the daemon's snapshot comes back and the pin stays empty.
    rt.update(|s, _| {
        s.runtime.reset();
        s.runtime.saved.reasoning_effort = Some("high".into());
    });
    rt.update(|s, _| s.runtime_apply());
    settle(&mut rt);
    assert!(rt.state.runtime.apply_error.is_none(), "{:?}", rt.state.runtime.apply_error);
    assert_eq!(rt.state.detail().map(|d| d.prefs.reasoning_effort.clone()), Some(None));
    // Mark unread and Copy link are the app's own; Stop all asks first and the daemon stops what runs.
    let (running, _) = rt.state.running_agents();
    assert!(running > 0, "the seed has work in flight");
    rt.update(|s, tx| {
        let ev = KeyEvent { key: Key::Escape, mods: Mods { cmd: true, shift: true, ..Mods::default() } };
        assert!(neko_rune::on_key(s, &ev, tx));
    });
    settle(&mut rt);
    assert!(rt.state.dialog.is_some(), "Stop all N running agents? is asked first");
    click(&mut rt, "ask-yes");
    assert_eq!(rt.state.running_agents().0, 0, "CancelAllWork stopped them");
}

#[test]
fn batch_a_a_finished_session_is_deleted_only_after_the_dialog_says_yes() {
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    let finished = rt.state.summary.as_ref().unwrap().tasks.iter().find(|t| matches!(t.status, neko_protocol::workbench::TaskStatus::Completed | neko_protocol::workbench::TaskStatus::Cancelled)).map(|t| t.id.clone());
    let Some(id) = finished else {
        eprintln!("skipped: the seed has no finished session");
        return;
    };
    let before = rt.state.summary.as_ref().unwrap().tasks.len();
    rt.update(|s, _| s.do_action(&id, neko_rune::task_actions::Act::Delete));
    settle(&mut rt);
    assert!(has_text(&rt, "Delete this task?") && rt.state.summary.as_ref().unwrap().tasks.len() == before, "asked, nothing deleted yet");
    click(&mut rt, "ask-no");
    assert_eq!(rt.state.summary.as_ref().unwrap().tasks.len(), before);
    rt.update(|s, _| s.do_action(&id, neko_rune::task_actions::Act::Delete));
    settle(&mut rt);
    click(&mut rt, "ask-yes");
    assert_eq!(rt.state.summary.as_ref().unwrap().tasks.len(), before - 1, "the daemon removed it after the yes");
}

#[test]
fn batch_a_an_inbox_item_is_marked_done_closes_under_closed_recently_and_reopens() {
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    let item = rt.state.neko.inbox.iter().find(|i| i.default.is_none()).cloned().expect("an open item without a default");
    let open_before = rt.state.neko.inbox.len();
    click(&mut rt, &format!("inbox-done-{}", item.seq));
    assert_eq!(rt.state.neko.inbox.len(), open_before - 1, "the item left the open list");
    let closed = rt.state.neko.closed.iter().find(|c| c.seq == item.seq).cloned().expect("it is under Closed recently");
    assert_eq!(closed.reason, "Done by you", "the daemon says why it closed");
    click(&mut rt, "closed-toggle");
    assert!(has_text(&rt, "Done by you ·"), "{:?}", texts(&rt));
    click(&mut rt, &format!("closed-reopen-{}", item.seq));
    assert_eq!(rt.state.neko.inbox.len(), open_before, "reopened: it is open again");
    assert!(!rt.state.neko.closed.iter().any(|c| c.seq == item.seq));
}

#[test]
fn batch_a_a_routine_is_saved_with_the_new_cadence_the_way_the_web_saves_it() {
    use neko_protocol::routines::Cadence;
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    open_page(&mut rt, neko_rune::views::FullView::Routines);
    let (id, _, _) = first_routine(&rt);
    rt.update(|s, _| s.open_routine(&id));
    settle(&mut rt);
    rt.update(|s, _| {
        s.set_cadence_mode(false);
        s.pages.set_field("rt:time", "07:45");
        s.cadence_time_typed();
    });
    settle(&mut rt);
    click_in_page(&mut rt, "rt-save");
    let saved = rt.state.pages.routines.as_ref().unwrap().views.iter().find(|v| v.routine.id == id).map(|v| v.routine.cadence.clone());
    assert_eq!(saved, Some(Cadence::Daily { hour: 7, minute: 45 }), "the daemon saved the cadence");
    rt.update(|s, _| {
        s.set_cadence_mode(true);
        s.pages.set_field("rt:minutes", "30");
        s.cadence_minutes_typed();
        s.cadence_hours(Some(9), Some(18));
    });
    settle(&mut rt);
    click_in_page(&mut rt, "rt-save");
    let saved = rt.state.pages.routines.as_ref().unwrap().views.iter().find(|v| v.routine.id == id).map(|v| v.routine.cadence.clone());
    assert_eq!(saved, Some(Cadence::Every { minutes: 30, from_hour: 9, to_hour: 18 }));
}

// ------------------------------------------------------------------------------------------------ parity batch B: the work views

/// Board layout in All sessions, with the sessions and Neko columns folded so the first columns are in the pointer's reach.
fn board_open(rt: &mut Runtime<App>) {
    rt.update(|s, _| {
        s.layout.sessions.set_open(false);
        s.layout.set_neko_open(false);
        s.open_all_sessions();
        s.all.include_stopped = true;
        s.set_all_layout(neko_rune::board::Layout::Board);
    });
    settle(rt);
}

/// Takes the card keyed `key`, carries it `dx` points sideways with the pointer and lets go.
fn carry_card(rt: &mut Runtime<App>, key: &str, dx: f32) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    for i in 1..=6 {
        rt.pointer_move(x + dx * i as f32 / 6.0, y);
        rt.frame(1.0 / 60.0);
    }
    rt.pointer_up(x + dx, y);
    settle(rt);
}

fn status_of(rt: &Runtime<App>, id: &str) -> neko_protocol::workbench::TaskStatus {
    rt.state.summary.as_ref().unwrap().task(id).unwrap_or_else(|| panic!("no task {id}")).status
}

#[test]
fn batch_b_a_board_drag_starts_a_stopped_task_through_the_daemon_and_a_refused_one_changes_nothing() {
    use neko_protocol::workbench::TaskStatus;
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    board_open(&mut rt);
    // Refused first: a task being built, carried to Needs you, is not moved, and the daemon is not asked.
    let building = id_of(&rt, "Rework the export batching");
    let before: Vec<_> = rt.state.summary.as_ref().unwrap().tasks.iter().map(|t| (t.id.clone(), t.status)).collect();
    carry_card(&mut rt, &format!("board-card-{building}"), -2.0 * 292.0);
    assert_eq!(status_of(&rt, &building), TaskStatus::Building);
    let after: Vec<_> = rt.state.summary.as_ref().unwrap().tasks.iter().map(|t| (t.id.clone(), t.status)).collect();
    assert_eq!(before, after, "nothing changed");
    assert!(rt.state.toast_words().contains("can’t go back to waiting"), "{}", rt.state.toast_words());
    // Allowed: a stopped task, carried from Needs you to Running, is started by the daemon (`StartTask`).
    let stopped = id_of(&rt, "Stop the nightly export running twice");
    assert_eq!(status_of(&rt, &stopped), TaskStatus::Failed);
    // Narrowed by the search so its card is the one in view (Needs you holds ten cards, which scroll).
    rt.update(|s, _| s.all.search.replace_text("nightly export"));
    settle(&mut rt);
    carry_card(&mut rt, &format!("board-card-{stopped}"), 2.0 * 292.0);
    assert_ne!(status_of(&rt, &stopped), TaskStatus::Failed, "the daemon started it");
    assert!(rt.find(&format!("board-card-{stopped}")).is_some(), "it is still on the board, in its new column");
}

#[test]
fn batch_b_the_layouts_show_the_daemons_tasks_and_the_tree_lists_each_once() {
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    board_open(&mut rt);
    let total = rt.state.all_rows().len();
    assert!(total > 10, "the seed has work: {total}");
    assert!(!keyed(&rt, "board-card-").is_empty(), "the board draws cards");
    rt.update(|s, _| s.set_all_layout(neko_rune::board::Layout::Tree));
    settle(&mut rt);
    assert_eq!(rt.state.tree_visible().len(), total, "every task is a row, once");
    assert!(!keyed(&rt, "tree-row-").is_empty());
}

fn pr_of(rt: &Runtime<App>, id: &str) -> neko_rune::link::PrRow {
    rt.state.summary.as_ref().unwrap().pull_requests.iter().find(|r| r.id == id).unwrap_or_else(|| panic!("no pull request for {id}")).clone()
}

#[test]
fn batch_b_merge_when_ready_is_toggled_and_merge_all_ready_is_confirmed_through_the_daemon() {
    use neko_rune::link::Verdict;
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    open_page(&mut rt, neko_rune::views::FullView::PullRequests);
    let ready: Vec<String> = rt.state.summary.as_ref().unwrap().pull_requests.iter().filter(|r| r.verdict == Verdict::Ready).map(|r| r.id.clone()).collect();
    assert!(ready.len() >= 3, "the seed has pull requests ready to merge: {ready:?}");
    assert!(keyed(&rt, "pr-group-").iter().any(|k| k == "pr-group-Ready to merge"));
    // The switch: on, the daemon records the request; off, it is taken back.
    let one = ready[0].clone();
    click(&mut rt, &format!("pr-merge-{one}"));
    assert_eq!(pr_of(&rt, &one).verdict, Verdict::Merging, "the daemon has it merging when ready");
    assert!(rt.find(&format!("pr-row-{one}")).is_some());
    click(&mut rt, &format!("pr-merge-{one}"));
    assert_eq!(pr_of(&rt, &one).verdict, Verdict::Ready, "taken back");
    // Merge all ready: the dialog lists them; nothing is asked of the daemon before yes.
    click(&mut rt, "pr-merge-all");
    assert!(has_text(&rt, &format!("Merge {} pull requests?", ready.len())), "{:?}", texts(&rt));
    assert!(ready.iter().all(|id| pr_of(&rt, id).verdict == Verdict::Ready), "nothing before yes");
    click(&mut rt, "ask-yes");
    for id in &ready {
        assert_eq!(pr_of(&rt, id).verdict, Verdict::Merging, "{id} is merging when ready");
    }
    assert!(rt.find("pr-merge-all").is_none(), "nothing is left to merge");
}

#[test]
fn batch_b_close_with_a_reason_is_asked_first_and_goes_to_the_daemon_only_when_confirmed() {
    use neko_rune::link::Verdict;
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    open_page(&mut rt, neko_rune::views::FullView::PullRequests);
    let id = rt.state.summary.as_ref().unwrap().pull_requests.iter().find(|r| r.verdict == Verdict::Ready).map(|r| r.id.clone()).expect("a ready pull request");
    let before = rt.state.summary.as_ref().unwrap().tasks.iter().map(|t| (t.id.clone(), t.status)).collect::<Vec<_>>();
    click(&mut rt, &format!("pr-close-{id}"));
    assert!(rt.find("ask-yes").is_some());
    click(&mut rt, "ask-no");
    assert_eq!(before, rt.state.summary.as_ref().unwrap().tasks.iter().map(|t| (t.id.clone(), t.status)).collect::<Vec<_>>(), "Keep it changes nothing");
    click(&mut rt, &format!("pr-close-{id}"));
    rt.update(|s, _| s.pages.set_field(neko_rune::task_actions::ASK_FIELD, "A duplicate of #99"));
    settle(&mut rt);
    click(&mut rt, "ask-yes");
    assert!(rt.find("ask-yes").is_none(), "the dialog is closed once the request went");
    // The harness has no GitHub: the daemon answers in its own words (and never reaches a real pull request).
    eprintln!("close answered: {:?} / {:?}", rt.state.work.notice, rt.state.toast_words());
}

#[test]
fn batch_b_the_details_tab_lists_its_sections_for_a_daemon_task() {
    use neko_rune::panel::Tab;
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    let id = id_of(&rt, "Rework the export batching");
    pick(&mut rt, &id);
    rt.update(|s, _| {
        s.panel_choose_tab(Tab::Details);
        s.panel_set_open(true);
    });
    settle(&mut rt);
    for section in ["Working copy", "Info", "Activity log"] {
        assert!(has_text(&rt, section), "{section}: {:?}", texts(&rt));
    }
    assert!(!has_text(&rt, "Risk assessment"), "no assessment was written for it");
    assert!(rt.find("info-workspace").is_some() && rt.find("info-runtime").is_some());
    assert!(has_text(&rt, "No working copy yet."), "{:?}", texts(&rt));
    // The log, once opened, has a row for each event the daemon kept, newest first.
    click(&mut rt, "details-log");
    assert!(rt.find("log-row-0").is_some(), "{:?}", texts(&rt));
}

#[test]
fn batch_b_slash_in_the_session_composer_runs_the_webs_commands() {
    use neko_protocol::workbench::TaskStatus::{Cancelled, Completed};
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    let id = id_of(&rt, "Rework the export batching");
    pick(&mut rt, &id);
    click(&mut rt, "composer-input");
    rt.update(|s, _| s.composer.replace_text("/cl"));
    settle(&mut rt);
    assert!(rt.find("reply-popover").is_some() && rt.find("reply-pop-0").is_some(), "{:?}", texts(&rt));
    let finished = |rt: &Runtime<App>| rt.state.summary.as_ref().unwrap().tasks.iter().filter(|t| matches!(t.status, Completed | Cancelled)).count();
    assert!(finished(&rt) > 0);
    press(&mut rt, Key::Enter, Mods::default());
    assert!(rt.state.composer.text.is_empty(), "the command left the box");
    assert!(has_text(&rt, "finished"), "/clear asks first: {:?}", texts(&rt));
    click(&mut rt, "ask-yes");
    assert_eq!(finished(&rt), 0, "the daemon cleared them");
}

// ---- parity batch D, against the isolated daemon -------------------------------------------------------------------

fn extra(rt: &mut Runtime<App>, call: neko_rune::link::ExtraCall) {
    rt.update(move |s, _| s.extra_call(call));
    settle(rt);
}

#[test]
fn batch_d_themes_are_listed_picked_and_removed_through_the_daemon() {
    use neko_rune::link::ExtraCall;
    let Some(h) = start(&[]) else { return };
    let dir = h.data.join("themes");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("ocean.json"), r##"{"id":"ocean","name":"Ocean","appearance":"dark","colors":{"background":"#0b1f2a","foreground":"#d7e9f0","primary":"#3ab0d0"}}"##).unwrap();
    let mut rt = runtime(app(&h));
    extra(&mut rt, ExtraCall::Themes);
    assert_eq!(rt.state.extra.user_themes.len(), 1, "the daemon listed the person's theme file");
    assert!(rt.state.theme_entries().iter().any(|t| t.id == "ocean" && t.source == neko_rune::themes::Source::User) && rt.state.theme_entries().len() == 13);
    // Picking it is the daemon's setting too: asked again, it says ocean.
    rt.update(|s, _| s.pick_theme(true, "ocean"));
    settle(&mut rt);
    rt.update(|s, _| s.call(Call::Appearance));
    assert_eq!(rt.state.theme_choice().1, "ocean");
    let now = rt.state.theme_now(true);
    assert_eq!(&now.surface.window.hex()[..7], "#0b1f2a", "the window is painted in the picked theme");
    // Removing: asked first (the file is still there), deleted after yes, the listing comes back without it.
    rt.update(|s, _| s.ask_remove_theme("ocean"));
    settle(&mut rt);
    assert!(dir.join("ocean.json").exists());
    click(&mut rt, "ask-yes");
    assert!(!dir.join("ocean.json").exists(), "the daemon deleted the file");
    extra(&mut rt, ExtraCall::Themes);
    assert!(rt.state.extra.user_themes.is_empty());
}

#[test]
fn batch_d_the_summon_shortcut_full_power_clipboard_and_diagnostics_round_trip() {
    use neko_protocol::{HotkeyCombo, Modifier};
    use neko_rune::link::ExtraCall;
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    extra(&mut rt, ExtraCall::Hotkey);
    assert!(rt.state.extra.hotkey.is_some(), "the daemon says what the summon shortcut is");
    // A reserved combination is refused with the daemon's reason; a free one is committed.
    let reserved = HotkeyCombo::new(vec![Modifier::Cmd], "Space");
    rt.update(move |s, _| s.extra_call(ExtraCall::HotkeyCheck(reserved)));
    settle(&mut rt);
    assert!(rt.state.extra.hotkey_say.as_ref().is_some_and(|s| !s.ok), "{:?}", rt.state.extra.hotkey_say);
    let free = HotkeyCombo::new(vec![Modifier::Ctrl, Modifier::Alt], "KeyJ");
    rt.update({
        let free = free.clone();
        move |s, _| s.extra_call(ExtraCall::HotkeyCheck(free))
    });
    settle(&mut rt);
    assert_eq!(rt.state.extra.hotkey.as_ref().map(|c| c.combo.clone()), Some(free), "checked, then committed");
    // Full power and the clipboard switch read and write the daemon's own settings.
    extra(&mut rt, ExtraCall::SuperAgent(None));
    let before = rt.state.extra.super_agent.expect("read");
    extra(&mut rt, ExtraCall::SuperAgent(Some(!before)));
    assert_eq!(rt.state.extra.super_agent, Some(!before));
    extra(&mut rt, ExtraCall::Clipboard(Some(true)));
    assert_eq!(rt.state.extra.clipboard, Some(true));
    // Diagnostics: Ping, then the checks.
    rt.update(|s, _| s.run_diagnostics());
    settle(&mut rt);
    assert!(!rt.state.extra.diag_running && rt.state.extra.diag.is_some());
    // The machine reading is asked for (a harness may have none yet).
    extra(&mut rt, ExtraCall::Machine);
}

#[test]
fn batch_d_the_rulebook_the_update_source_and_the_model_check_go_to_the_daemon() {
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    rt.update(|s, _| s.pages.section = neko_rune::pages::Section::Rulebook);
    open_page(&mut rt, neko_rune::views::FullView::Settings);
    rt.update(|s, _| s.pages.set_field("rulebook", "Only small changes.\n"));
    settle(&mut rt);
    assert!(rt.state.rulebook_dirty());
    click(&mut rt, "rulebook-save");
    assert_eq!(rt.state.summary.as_ref().unwrap().settings.rulebook, "Only small changes.", "the daemon kept it (trimmed, as it keeps every text)");
    assert!(!rt.state.rulebook_dirty() && has_text(&rt, "Using your own rules"));
    click(&mut rt, "rulebook-restore");
    click(&mut rt, "ask-yes");
    assert!(rt.state.summary.as_ref().unwrap().settings.rulebook.is_empty(), "restored to the default");
    // The update source: a github address is kept as owner/name.
    rt.update(|s, _| s.pages.section = neko_rune::pages::Section::Updates);
    settle(&mut rt);
    click_in_page(&mut rt, "update-source-edit");
    rt.update(|s, _| s.pages.set_field("update-source", "https://github.com/acme/neko-fork.git"));
    settle(&mut rt);
    click_in_page(&mut rt, "update-source-save");
    assert_eq!(rt.state.summary.as_ref().unwrap().settings.self_update.as_ref().map(|u| u.source.clone()).as_deref(), Some("acme/neko-fork"));
    // The model check: one real prompt through the runner; the harness has no agents, so the answer is the daemon's own words, shown.
    rt.update(|s, _| s.check_model());
    settle(&mut rt);
    assert!(!rt.state.extra.checking && rt.state.extra.model_check.is_some(), "{:?}", rt.state.extra.model_check);
}

#[test]
fn batch_d_start_over_and_roll_back_ask_first_and_the_daemon_answers_in_its_words() {
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    let before = rt.state.summary.as_ref().unwrap().tasks.len();
    rt.update(|s, _| s.ask_start_over());
    settle(&mut rt);
    assert_eq!(rt.state.summary.as_ref().unwrap().tasks.len(), before, "nothing before yes");
    click(&mut rt, "ask-no");
    assert_eq!(rt.state.summary.as_ref().unwrap().tasks.len(), before);
    rt.update(|s, _| s.ask_start_over());
    settle(&mut rt);
    click(&mut rt, "ask-yes");
    assert!(rt.find("ask-yes").is_none(), "the dialog is closed once the request went");
    eprintln!("start over: {before} tasks before, {} after, notice {:?}", rt.state.summary.as_ref().unwrap().tasks.len(), rt.state.work.notice);
    rt.update(|s, _| s.ask_roll_back_web());
    settle(&mut rt);
    click(&mut rt, "ask-yes");
    assert!(rt.find("ask-yes").is_none());
}

#[test]
fn batch_d_the_first_run_state_and_done_go_to_the_daemon() {
    use neko_rune::link::ExtraCall;
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    extra(&mut rt, ExtraCall::Onboarding);
    if rt.state.onboarding.completed == Some(false) {
        assert!(rt.state.onboarding_shown(), "a fresh data folder opens the setup in Home's place");
    }
    rt.update(|s, _| s.open_onboarding(neko_rune::onboarding::Step::Done));
    settle(&mut rt);
    click(&mut rt, "onb-next");
    assert_eq!(rt.state.onboarding.completed, Some(true), "the daemon marked it complete");
    assert!(!rt.state.onboarding_shown());
    let Ok(Response::OnboardingState { completed, .. }) = ask(&socket(&h), Request::GetOnboardingState) else { panic!("no state") };
    assert!(completed);
}

#[test]
fn batch_d_inspect_app_lists_rune_with_its_size_apart_from_the_web() {
    let Some(h) = start(&[]) else { return };
    let mut rt = runtime(app(&h));
    rt.update(|s, _| {
        s.focused = true;
        s.linked = true;
        // The report is built and sent through the blocking dispatch.
        let _ = s.tick_report(Some([1360, 900]));
    });
    settle(&mut rt);
    let Ok(Response::App { window: Some(window), windows, .. }) = ask(&socket(&h), Request::InspectApp) else { panic!("the daemon heard no report") };
    // A harness built before the `client` field existed files the report as the web's: skip instead of failing (rebuild it).
    if window.client == "web" && window.size.is_none() && std::env::var_os("NEKO_REQUIRE_HARNESS").is_none() {
        eprintln!("skipped: this web_harness predates AppReport::client; rebuild it");
        return;
    }
    assert_eq!((window.client.as_str(), window.size), ("rune", Some([1360, 900])));
    assert!(window.viewers.iter().any(|v| v.id == "image"), "the registered viewers");
    assert!(window.pages.iter().any(|p| p.id == "settings") && window.theme.is_some());
    assert!(windows.iter().any(|w| w.client == "rune" && w.reporting));
}
