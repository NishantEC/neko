//! The scenes of a Cattain made by asking (docs/knowledge/cats.md, "Made by asking"; design.md, "A Cattain"): the thread whose agent
//! made yuzu, with its note; the toast with Undo; yuzu's own thread before anyone wrote in it; Settings ▸ Cattains with yuzu; and the
//! agent refused at sixteen. Placeholder data only.

use neko_protocol::cats::{self, CattainFile};
use neko_protocol::{Cat, PartBody, RunStatus, Task, TaskId, TaskStatus, ToolState};

use super::{Script, run, start, task, with_workspaces, ws};
use crate::state::{App, Book};

const ASKED: &str = "Make a Cattain for the iOS work in acme-ios. It should own the beta builds and the release notes.";

fn ios(s: &mut App) {
    with_workspaces(s);
    s.workspaces.push(ws("ws_ios", "acme-ios", Some("/Users/example/acme-ios"), 3));
}

/// The thread that asked, in acme-ios.
fn asking(s: &mut App) -> Task {
    let mut t = task(70, ASKED, 6);
    t.title = "Make a Cattain for the iOS work".into();
    t.workspace = "ws_ios".into();
    t.folder = Some("/Users/example/acme-ios".into());
    t.status = TaskStatus::Done;
    s.tasks.push(t.clone());
    t
}

/// yuzu, as the agent made it: the first pool name, the first free face, the asking thread's project.
fn yuzu(made_in: &TaskId, created_ms: i64) -> Cat {
    Cat {
        id: "cat_yuzu".into(),
        name: "yuzu".into(),
        face: "tabby".into(),
        purpose: Some("iOS work".into()),
        workspace: Some("ws_ios".into()),
        thread: None,
        model: None,
        access: None,
        created_ms,
        retired_ms: None,
        made_in: Some(made_in.clone()),
    }
}

const MADE: &str = "Made a Cattain: yuzu (iOS work) in the project acme-ios. Address it as yuzu. The person sees it in Neko (Settings ▸ Cattains, Search) and talks to it in its own thread; it has no work until they write to it there. Tell the person you made yuzu and what it is for.";

const HOUSE: &str = "3 of at most 16 Cattains are at work:\n- miso: at rest\n- mochi (Writes and tidies the docs.): at rest\n- tofu (Runs the tests and fixes what breaks.): at rest\nThe person talks to each in its own thread in Neko. Reuse one whose purpose fits rather than making another.";

/// The thread whose agent looked at the house, made yuzu and said so: the note stands between its steps and its words.
fn made(s: &mut App) -> TaskId {
    ios(s);
    let t = asking(s);
    let at = start(s) + 41 * 60_000;
    let mut sc = Script::new(&t, "run_made", at);
    sc.tool("mcp__neko__list_cattains", "", "{}", HOUSE, ToolState::Done, 1);
    sc.tool("mcp__neko__make_cattain", "iOS work", "{\n  \"purpose\": \"iOS work\",\n  \"project\": \"acme-ios\",\n  \"rules\": \"Own the beta builds and the release notes.\"\n}", MADE, ToolState::Done, 1);
    sc.part(PartBody::CattainMade { cat: "cat_yuzu".into(), name: "yuzu".into(), purpose: Some("iOS work".into()) }, 0, true);
    sc.say("I made **yuzu**, a Cattain for the iOS work in acme-ios. Its rules say it owns the beta builds and the release notes.\n\nWrite to yuzu in its own thread to give it work; until then it does nothing. You can change its rules in its Files tab.", true);
    let mut r = run("run_made", &t, RunStatus::Done, at, 9);
    r.changes = None;
    r.branch = None;
    r.workdir = "/Users/example/acme-ios".into();
    s.books.insert(t.id.clone(), Book { runs: vec![r], loaded: true, parts: sc.parts, ..Book::default() });
    s.cats.push(yuzu(&t.id, at + 3000));
    s.open = Some(t.id.clone());
    t.id
}

/// The note in the thread that asked.
pub fn note(s: &mut App) {
    made(s);
}

/// The same, a moment after: the toast with Undo (sticky here, so a picture can be taken).
pub fn toast(s: &mut App) {
    made(s);
    // As `App::cattain_made_toast` words it; the scene's stays, the app's goes after five seconds.
    s.toasts.push(crate::ui::toast::ToastSpec::new("Made yuzu").description("A Cattain for iOS work").action("Undo", "unmake:cat_yuzu").sticky());
}

/// yuzu's own thread, opened from the note: it opens on who it is.
pub fn first_thread(s: &mut App) {
    made(s);
    let own = super::cattains::own_thread(s, "cat_yuzu");
    if let Some(t) = s.tasks.iter_mut().find(|t| t.id == own) {
        t.status = TaskStatus::New;
        t.workspace = "ws_ios".into();
    }
    s.books.insert(own.clone(), Book { loaded: true, ..Book::default() });
    let rules = "Own the beta builds and the release notes.\n";
    for &name in cats::FILES {
        let text = if name == cats::RULES { rules } else { "" };
        let f = CattainFile { cat: "cat_yuzu".into(), name: name.into(), text: text.into(), cut: false, previous: false, changed_ms: (name == cats::RULES).then_some(s.now_ms - 60_000), path: format!("/Users/example/Library/Application Support/neko/cattains/cat_yuzu/{name}"), editable: cats::editable(name) };
        s.take_cattain_file(f);
    }
    s.open = Some(own.clone());
    s.pane.choice = Some(true);
    s.pane.sheet = true;
    s.pane.tab.insert(own, crate::pane::Tab::Overview);
}

/// Settings ▸ Cattains with yuzu, made in that thread, and "Agents may make Cattains".
pub fn settings(s: &mut App) {
    made(s);
    s.open = None;
    s.settings_open = true;
    s.settings_page = 3;
    s.runtimes = Some(super::agents(true, neko_protocol::PermissionMode::Follow));
}

/// At sixteen: the agent is refused with the house listed, and says so to the person. No note, no new Cattain.
pub fn limit(s: &mut App) {
    ios(s);
    let names = ["yuzu", "kuro", "sesame", "azuki", "kinako", "nori", "ume", "sumi", "hana", "momo", "sora", "mikan", "shiro"];
    for (i, n) in names.iter().enumerate() {
        let mut c = yuzu(&TaskId("task_old".into()), 10 + i as i64);
        c.id = format!("cat_{n}");
        c.name = (*n).into();
        c.face = cats::FACES[(i + 3) % cats::FACES.len()].into();
        c.purpose = Some(format!("Work {}", i + 1));
        c.made_in = None;
        s.cats.push(c);
    }
    let t = asking(s);
    let at = start(s) + 41 * 60_000;
    let refused = "Neko keeps at most 16 Cattains at work, and 16 are:\n- miso\n- mochi (Writes and tidies the docs.)\n- tofu (Runs the tests and fixes what breaks.)\n- yuzu (Work 1)\n- kuro (Work 2)\n- sesame (Work 3)\n…\nNo Cattain was made.\nAsk the person whether one of these should take the work,\nor whether they will retire one in Settings ▸ Cattains.";
    let mut sc = Script::new(&t, "run_limit", at);
    sc.tool_end("mcp__neko__make_cattain", "iOS work", "{\n  \"purpose\": \"iOS work\",\n  \"project\": \"acme-ios\"\n}", refused, ToolState::Failed, None, 1);
    sc.say("I could not make a new Cattain: Neko keeps at most 16 at work, and you have 16.\n\nShould one of them take the iOS work, or will you retire one in Settings ▸ Cattains? Then I'll make it.", true);
    let mut r = run("run_limit", &t, RunStatus::Done, at, 7);
    r.changes = None;
    r.branch = None;
    r.workdir = "/Users/example/acme-ios".into();
    s.books.insert(t.id.clone(), Book { runs: vec![r], loaded: true, parts: sc.parts, ..Book::default() });
    s.open = Some(t.id.clone());
    s.flip("worked-run_limit");
    s.flip("part-1");
}
