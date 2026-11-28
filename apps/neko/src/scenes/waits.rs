//! The scenes of timers, watches and standing rounds (docs/knowledge/waits.md), and of words steered into a running turn
//! (docs/knowledge/composer.md): a thread resting until 18:00, its activity bar and Overview; one waiting on a file and a check; one
//! woken by its timer; one whose timer came due while automatic work was paused; a standing round's message in mochi's thread; and a
//! run that read the person's words mid-turn, working and finished. Placeholder data only.

use neko_protocol::{EventKind, PartBody, RunStatus, Task, TaskStatus, ToolState, Tone, Wait, WaitKind};

use super::{Script, run, start, task, with_workspaces};
use crate::pane::Tab;
use crate::state::{App, Book};

const HOUR: i64 = 3_600_000;
const ASKED: &str = "Deploy the docs site\n\nThe build on the docs branch takes a while. When it is up, check that the new install page is live and the old links redirect.";

/// Today's 18:00 in the scenes' zone (UTC), later than the scenes' now.
fn six_pm(s: &App) -> i64 {
    let day = s.now_ms - s.now_ms.rem_euclid(86_400_000);
    let at = day + 18 * HOUR;
    if at > s.now_ms { at } else { at + 86_400_000 }
}

fn thread(status: TaskStatus) -> Task {
    let mut t = task(60, ASKED, 25);
    t.title = "Deploy the docs site".into();
    t.workspace = "ws_acme".into();
    t.folder = Some("/Users/example/acme-api".into());
    t.cat = None;
    t.status = status;
    t
}

fn timer(s: &App, note: &str) -> Wait {
    Wait { id: "wait_1".into(), kind: WaitKind::Timer, note: note.into(), at_ms: Some(six_pm(s)), until_ms: None, paths: vec![], command: None, every_ms: None, checked_ms: None, last_exit: None, created_ms: s.now_ms - 20 * 60_000 }
}

fn files_watch(s: &App) -> Wait {
    Wait {
        id: "wait_2".into(),
        kind: WaitKind::Files,
        note: "Read the build log and fix what failed.".into(),
        at_ms: None,
        until_ms: Some(s.now_ms + 7 * 24 * HOUR - 20 * 60_000),
        paths: vec!["build/deploy.log".into()],
        command: None,
        every_ms: None,
        checked_ms: None,
        last_exit: None,
        created_ms: s.now_ms - 20 * 60_000,
    }
}

fn check_watch(s: &App) -> Wait {
    Wait {
        id: "wait_3".into(),
        kind: WaitKind::Command,
        note: "Check the install page once the site answers.".into(),
        at_ms: None,
        until_ms: Some(s.now_ms + 24 * HOUR - 20 * 60_000),
        paths: vec![],
        command: Some("curl -fsI https://docs.example.com/install".into()),
        every_ms: Some(5 * 60_000),
        checked_ms: Some(s.now_ms - 2 * 60_000),
        last_exit: Some(22),
        created_ms: s.now_ms - 20 * 60_000,
    }
}

/// The first turn: the agent starts the deploy, sets what it waits on, and says so.
fn first_turn(sc: &mut Script, tools: &[(&str, &str, &str, &str)], said: &str) {
    sc.tool("Bash", "git push origin docs", "git push origin docs", "To example.com:acme/acme-api.git\n   4f2a1c3..9b7e2d1  docs -> docs", ToolState::Done, 3);
    for (name, summary, input, out) in tools {
        sc.tool(name, summary, input, out, ToolState::Done, 1);
    }
    sc.say(said, true);
}

fn stage(s: &mut App, th: &Task, book: Book) {
    with_workspaces(s);
    s.tasks.push(th.clone());
    s.tasks.sort_by_key(|t| std::cmp::Reverse(t.created_ms));
    s.books.insert(th.id.clone(), book);
    s.open = Some(th.id.clone());
}

fn turn_one(s: &App, th: &Task, tools: &[(&str, &str, &str, &str)], said: &str) -> (Book, u64) {
    let mut sc = Script::new(th, "run_1", start(s) + 20 * 60_000);
    first_turn(&mut sc, tools, said);
    let mut r = run("run_1", th, RunStatus::Done, start(s) + 20 * 60_000, 42);
    r.changes = None;
    let seq = sc.seq;
    (Book { runs: vec![r], parts: sc.parts, loaded: true, ..Book::default() }, seq)
}

const WAKE_ME: (&str, &str, &str, &str) = ("mcp__neko__wake_me", "18:00", "{\n  \"at\": \"18:00\",\n  \"note\": \"Check the install page is live and the old links redirect.\"\n}", "Timer wait_1 is set: Neko wakes you at 18:00.");

/// "Deploy the docs site" rests until 18:00: its row, its waiting line and its one quiet line above the composer.
pub fn timer_waiting(s: &mut App) {
    let mut th = thread(TaskStatus::Waiting);
    th.waits = vec![timer(s, "Check the install page is live and the old links redirect.")];
    let (book, _) = turn_one(s, &th, &[WAKE_ME], "The docs branch is pushed and the site is building; that takes about an hour. I'll check again at 18:00 that the install page is live and the old links redirect.");
    stage(s, &th, book);
}

/// The same with the pane's Overview open: Waiting on, with Stop the timer.
pub fn timer_overview(s: &mut App) {
    timer_waiting(s);
    let id = s.open.clone().expect("the thread");
    s.pane.choice = Some(true);
    s.pane.sheet = true;
    s.pane.tab.insert(id, Tab::Overview);
}

/// A thread waiting on a file and a check: two quiet lines with Stop watching, and Overview.
pub fn watch_waiting(s: &mut App) {
    let mut th = thread(TaskStatus::Waiting);
    th.waits = vec![files_watch(s), check_watch(s)];
    let files = ("mcp__neko__watch_files", "build/deploy.log", "{\n  \"paths\": [\"build/deploy.log\"]\n}", "Watch wait_2 is set.");
    let check = ("mcp__neko__watch_command", "curl -fsI https://docs.example.com/install", "{\n  \"command\": \"curl -fsI https://docs.example.com/install\",\n  \"every\": \"5m\"\n}", "Watch wait_3 is set.");
    let (book, _) = turn_one(s, &th, &[files, check], "The deploy is running. I'm watching its log for a failure, and checking every five minutes whether the install page answers; I'll pick it up from whichever comes first.");
    stage(s, &th, book);
    s.pane.choice = Some(true);
    s.pane.sheet = true;
    s.pane.tab.insert(th.id.clone(), Tab::Overview);
}

/// Woken by its timer at 18:00: the note before the run that read it, and the answer.
pub fn timer_woken(s: &mut App) {
    let mut th = thread(TaskStatus::Done);
    th.latest_line = "The site is up. The install page answers, and the old setup link redirects to it.".into();
    let after = ("mcp__neko__wake_me", "40m", "{\n  \"after\": \"40m\",\n  \"note\": \"Check the install page is live and the old links redirect.\"\n}", "Timer wait_1 is set.");
    let (mut book, seq) = turn_one(s, &th, &[after], "The docs branch is pushed and the site is building; that takes about forty minutes. I'll look again then that the install page is live and the old links redirect.");
    let at = s.now_ms - 4 * 60_000;
    let mut sc = Script::new(&th, "run_2", at);
    sc.seq = seq;
    sc.part(PartBody::EventWake { event: EventKind::Timer, note: "Check the install page is live and the old links redirect.".into(), detail: String::new(), at_ms: Some(at), from: "wait_1".into() }, 0, true);
    sc.tool("Bash", "curl -sI https://docs.example.com/install", "curl -sI https://docs.example.com/install", "HTTP/2 200", ToolState::Done, 2);
    sc.tool("Bash", "curl -sI https://docs.example.com/setup", "curl -sI https://docs.example.com/setup", "HTTP/2 301\nlocation: /install", ToolState::Done, 2);
    sc.say("The site is up. The install page answers, and the old setup link redirects to it. Nothing else to do.", true);
    let mut r = run("run_2", &th, RunStatus::Done, at, 19);
    r.changes = None;
    book.runs.push(r);
    book.parts.extend(sc.parts);
    stage(s, &th, book);
    s.flip("event-wake-said-4");
}

/// Its timer came due while automatic work was paused: no run, and the thread says so.
pub fn timer_paused(s: &mut App) {
    let th = thread(TaskStatus::Done);
    let (mut book, seq) = turn_one(s, &th, &[WAKE_ME], "The docs branch is pushed and the site is building; that takes about an hour. I'll check again at 18:00 that the install page is live and the old links redirect.");
    let mut sc = Script::new(&th, "none", s.now_ms - 2 * 60_000);
    sc.seq = seq;
    sc.notice(Tone::Info, "Not woken at 18:00: automatic work was paused.", "The timer was for: Check the install page is live and the old links redirect. Resume automatic work in Rounds, or write here to carry on.");
    for p in &mut sc.parts {
        p.run = None;
    }
    book.parts.extend(sc.parts);
    stage(s, &th, book);
    s.rounds_paused = true;
}

/// A standing round's words in mochi's own thread, "From the round Morning check", and mochi's answer.
pub fn round_standing(s: &mut App) {
    let own = super::cattains::talked(s);
    let task = s.tasks.iter().find(|t| t.id == own).cloned().expect("its thread");
    let seq = s.books.get(&own).map_or(0, |b| b.parts.len() as u64);
    let at = s.now_ms - 6 * 60_000;
    let mut sc = Script::new(&task, "run_m2", at);
    sc.seq = seq;
    sc.part(PartBody::EventWake { event: EventKind::Round, note: "Morning check".into(), detail: "Look over the open pull requests on acme-api and tell me which are ready to merge.".into(), at_ms: Some(at), from: "round_00000000000000000000000009".into() }, 0, true);
    sc.tool("Bash", "gh pr list --state open", "gh pr list --state open", "12  Fix the install page  docs\n14  Bump the version  release", ToolState::Done, 2);
    sc.tool("Write", "DIGEST.md", "{}", "Wrote the file.", ToolState::Done, 1);
    sc.say("Two pull requests are open. **acme-api#12** (the install page) passes its checks and is ready; **#14** waits on a review from Lee. I put both under Needs you in the digest.", true);
    let mut r = run("run_m2", &task, RunStatus::Done, at, 24);
    r.changes = None;
    r.branch = None;
    r.workdir = "/Users/example/Library/Application Support/neko/cattains/cat_mochi".into();
    if let Some(b) = s.books.get_mut(&own) {
        b.runs.push(r);
        b.parts.extend(sc.parts);
    }
    s.open = Some(own);
}

const STEERED: &str = "Also make the warning red, not orange.";

/// A finished run that read the person's words mid-turn: they stand where it read them, before what it said about them, in one
/// run with one fold.
pub fn steered_done(s: &mut App) {
    let th = steered_thread(TaskStatus::Done);
    let (book, _) = steered_book(s, &th, true);
    stage(s, &th, book);
}

/// The same while it works: the turn cut at the person's words, the live line after.
pub fn steered_working(s: &mut App) {
    let th = steered_thread(TaskStatus::Working);
    let (book, _) = steered_book(s, &th, false);
    stage(s, &th, book);
}

fn steered_thread(status: TaskStatus) -> Task {
    let mut t = task(61, "Show a warning when the session is about to expire", 12);
    t.workspace = "ws_acme".into();
    t.folder = Some("/Users/example/acme-api".into());
    t.cat = None;
    t.status = status;
    t
}

fn steered_book(s: &App, th: &Task, done: bool) -> (Book, u64) {
    let at = s.now_ms - 9 * 60_000;
    let mut sc = Script::new(th, "run_1", at);
    sc.tool("Grep", "session_expires", "{\n  \"pattern\": \"session_expires\"\n}", "src/session.rs:41\nsrc/banner.rs:12", ToolState::Done, 2);
    sc.tool("Read", "src/banner.rs", "{\n  \"file_path\": \"src/banner.rs\"\n}", "pub fn banner() -> El {}", ToolState::Done, 1);
    sc.tool("Edit", "src/banner.rs", "{\n  \"file_path\": \"src/banner.rs\"\n}", "The file was edited.", ToolState::Done, 3);
    // The person's words, steered in while the edit was made; the daemon moved them to where the next step read them.
    sc.part(PartBody::Person { text: STEERED.into() }, 1, true);
    if let Some(p) = sc.parts.last_mut() {
        p.run = None;
    }
    sc.say("Reading your message: the warning is red now.", true);
    sc.tool("Edit", "src/theme.rs", "{\n  \"file_path\": \"src/theme.rs\"\n}", "The file was edited.", ToolState::Done, 2);
    if done {
        sc.tool("Bash", "cargo test", "cargo test", "test result: ok. 41 passed", ToolState::Done, 6);
        sc.say("The banner now warns five minutes before the session expires, in red as you asked. The tests pass.", true);
    } else {
        sc.tool("Bash", "cargo test", "cargo test", "", ToolState::Running, 0);
    }
    let r = run("run_1", th, if done { RunStatus::Done } else { RunStatus::Running }, at, 41);
    let seq = sc.seq;
    (Book { runs: vec![r], parts: sc.parts, loaded: true, ..Book::default() }, seq)
}
