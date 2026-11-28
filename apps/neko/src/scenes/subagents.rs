//! The subagent scenes (docs/knowledge/app-shell.md, "Kittens in the app"): a thread that started two subagents, one working and one
//! queued; a thread woken by their reports, the note closed and open; a thread that waits on them, with its sidebar row; a subagent
//! opened on its own, read-only, and one asking for a permission; and Needs you with a subagent's request. Placeholder data only.

use neko_protocol::{PartBody, PermissionState, RunStatus, SubPlace, SubagentOf, Task, TaskStatus, ToolState};

use super::{Script, cat_name, diff_file, permission_item, run, start, task, with_workspaces};
use crate::state::{App, Book};

const FOLDER: &str = "/Users/example/acme-api";

/// "Fix login", in acme-api.
fn thread(status: TaskStatus) -> Task {
    let mut t = task(30, "Fix login\n\nThe sign-in page turns away a valid password once the session has expired. Check the links and the images on it while you are in there.", 14);
    t.workspace = "ws_acme".into();
    t.folder = Some(FOLDER.into());
    t.cat = None;
    t.status = status;
    t
}

/// A subagent of `thread`: numbered, no name, the thread's folder.
fn sub(n: u32, of: &Task, number: &str, title: &str, words: &str, status: TaskStatus, minutes: i64) -> Task {
    let mut t = task(40 + n, words, minutes);
    t.title = title.into();
    t.workspace = of.workspace.clone();
    t.folder = of.folder.clone();
    t.cat = of.cat.clone();
    t.status = status;
    t.subagent = Some(SubagentOf::new(of.id.clone(), number, of.id.clone()));
    t
}

/// The sample workspaces with the thread and its subagents in them, and the thread open.
fn stage(s: &mut App, thread: Task, subs: Vec<Task>) {
    with_workspaces(s);
    s.tasks.push(thread.clone());
    s.tasks.extend(subs);
    s.tasks.sort_by_key(|t| std::cmp::Reverse(t.created_ms));
    s.open = Some(thread.id);
}

/// The agent's call that started a kitten, with the words the tool answered it (the number is in them).
fn start_tool(sc: &mut Script, number: &str, title: &str, secs: i64) {
    let said = format!("Subagent {number} (\"{title}\") is started. It is working now. Neko wakes you with its report when it ends. Do not poll or wait. If you have nothing else to do, end your turn now.");
    sc.tool("mcp__neko__start_subagent", title, &format!("{{\n  \"title\": \"{title}\"\n}}"), &said, ToolState::Done, secs);
}

fn card(sc: &mut Script, of: &Task, number: &str) {
    sc.part(PartBody::Subagent { subagent: of.id.clone(), number: number.into(), title: of.title.clone() }, 0, true);
}

/// A subagent's own transcript: its run, and what it did.
fn sub_book(s: &mut App, t: &Task, status: RunStatus, build: impl FnOnce(&mut Script)) {
    let id = format!("run_{}", t.id.0);
    let mut sc = Script::new(t, &id, start(s) + 3 * 60_000);
    build(&mut sc);
    let mut r = run(&id, t, status, start(s) + 3 * 60_000, 40);
    r.changes = None;
    r.branch = None;
    let mut book = Book { runs: vec![r], loaded: true, ..Book::default() };
    book.parts.extend(sc.parts);
    book.permissions.extend(sc.perms);
    s.books.insert(t.id.clone(), book);
}

fn links_work(sc: &mut Script, done: bool) {
    sc.tool("Grep", "href=", "{\n  \"pattern\": \"href=\"\n}", "docs/setup.md:14\ndocs/faq.md:3", ToolState::Done, 2);
    sc.tool("Bash", "npx linkcheck docs/", "npx linkcheck docs/", "docs/setup.md:14 404\ndocs/faq.md:3 404", ToolState::Done, 6);
    if done {
        sc.say("Two links are dead: docs/setup.md line 14 and docs/faq.md line 3. Both point at the old site, and I changed them to the new addresses.", true);
    } else {
        sc.say("Two links are dead so far: docs/setup.md line 14 and docs/faq.md line 3. Checking the rest of the pages", false);
    }
}

fn images_work(sc: &mut Script) {
    sc.tool("Glob", "public/images/**", "{\n  \"pattern\": \"public/images/**\"\n}", "public/images/hero.png\npublic/images/team.jpg", ToolState::Done, 1);
    sc.say("Every image has alt text. Two of them, the hero and the team photo, are over 400 KB.", true);
}

/// The first turn of the thread: it read the handler, started two subagents and went on (`ended`: it ended its turn).
fn first_turn(sc: &mut Script, subs: &[&Task], ended: bool) {
    sc.tool("Read", "src/auth.rs", "{\n  \"file_path\": \"src/auth.rs\"\n}", "pub fn sign_in() {}", ToolState::Done, 1);
    sc.say("The handler checks the session before it checks the password. I will have two helpers check the page's links and images while I fix the order.", true);
    start_tool(sc, "1", "Check links", 1);
    card(sc, subs[0], "1");
    start_tool(sc, "2", "Check images", 1);
    card(sc, subs[1], "2");
    if ended {
        sc.say("Both are started. I will pick this up when their reports arrive.", true);
    } else {
        sc.tool("Edit", "src/auth.rs", "{\n  \"file_path\": \"src/auth.rs\"\n}", "", ToolState::Running, 0);
    }
}

fn open_thread(s: &mut App, t: &Task, runs: Vec<neko_protocol::Run>, scripts: Vec<Script>) {
    let mut book = Book { runs, loaded: true, ..Book::default() };
    for sc in scripts {
        book.parts.extend(sc.parts);
        book.permissions.extend(sc.perms);
    }
    s.books.insert(t.id.clone(), book);
}

/// The thread at work: two cards in one stack, Kitten 1 working with its last line, Kitten 2 queued.
pub fn cards(s: &mut App) {
    let th = thread(TaskStatus::Working);
    let a = sub(1, &th, "1", "Check links", "Look at the links on the docs site and say which are dead", TaskStatus::Working, 8);
    let b = sub(2, &th, "2", "Check images", "Look at the images on the docs site and say which are too big", TaskStatus::New, 8);
    stage(s, th.clone(), vec![a.clone(), b.clone()]);
    let mut sc = Script::new(&th, "run_1", start(s));
    first_turn(&mut sc, &[&a, &b], false);
    let r = run("run_1", &th, RunStatus::Running, start(s), 0);
    open_thread(s, &th, vec![r], vec![sc]);
    sub_book(s, &a, RunStatus::Running, |sc| links_work(sc, false));
}

/// The thread's first turn ended and it rests: both subagents work, the waiting line is its last line.
pub fn waiting(s: &mut App) {
    let th = thread(TaskStatus::Waiting);
    let a = sub(1, &th, "1", "Check links", "Look at the links on the docs site and say which are dead", TaskStatus::Working, 8);
    let b = sub(2, &th, "2", "Check images", "Look at the images on the docs site and say which are too big", TaskStatus::Working, 8);
    stage(s, th.clone(), vec![a.clone(), b.clone()]);
    let mut sc = Script::new(&th, "run_1", start(s));
    first_turn(&mut sc, &[&a, &b], true);
    let mut r = run("run_1", &th, RunStatus::Done, start(s), 31);
    r.changes = None;
    open_thread(s, &th, vec![r], vec![sc]);
    sub_book(s, &a, RunStatus::Running, |sc| links_work(sc, false));
    sub_book(s, &b, RunStatus::Running, |sc| {
        sc.tool("Glob", "public/images/**", "{\n  \"pattern\": \"public/images/**\"\n}", "", ToolState::Running, 0);
    });
}

/// The thread woken by both reports and done: two notes (the first report is open in `wake_open`).
pub fn wake(s: &mut App) {
    let th = thread(TaskStatus::Done);
    let a = sub(1, &th, "1", "Check links", "Look at the links on the docs site and say which are dead", TaskStatus::Done, 8);
    let b = sub(2, &th, "2", "Check images", "Look at the images on the docs site and say which are too big", TaskStatus::Done, 8);
    stage(s, th.clone(), vec![a.clone(), b.clone()]);
    let mut sc = Script::new(&th, "run_1", start(s));
    first_turn(&mut sc, &[&a, &b], true);
    let mut first = run("run_1", &th, RunStatus::Done, start(s), 31);
    first.changes = None;
    let mut sc2 = Script::new(&th, "run_2", start(s) + 12 * 60_000);
    sc2.seq = sc.seq;
    sc2.part(
        PartBody::Wake {
            number: "1".into(),
            title: "Check links".into(),
            status: "done".into(),
            summary: "Two links are dead: `docs/setup.md` line 14 and `docs/faq.md` line 3. Both pointed at the old site, and I changed them to the new addresses.\n\n- The setup page now links to the new install guide\n- The FAQ now links to the new pricing page".into(),
            files: vec!["docs/setup.md +1 -1".into(), "docs/faq.md +1 -1".into()],
            branch: Some("neko/k3x9p1".into()),
            question: None,
        },
        0,
        true,
    );
    sc2.part(
        PartBody::Wake { number: "2".into(), title: "Check images".into(), status: "done".into(), summary: "Every image has alt text. Two of them are over 400 KB: the hero and the team photo.".into(), files: Vec::new(), branch: None, question: None },
        0,
        true,
    );
    sc2.tool("Edit", "src/auth.rs", "{\n  \"file_path\": \"src/auth.rs\"\n}", "The file was edited.", ToolState::Done, 3);
    sc2.say("Both reports are in. The two dead links are fixed, and the sign-in handler now checks the password before the session. The hero and the team photo are over 400 KB; I left them for you.", true);
    let mut second = run("run_2", &th, RunStatus::Done, start(s) + 12 * 60_000, 48);
    second.changes = None;
    open_thread(s, &th, vec![first, second], vec![sc, sc2]);
    sub_book(s, &a, RunStatus::Done, |sc| links_work(sc, true));
    sub_book(s, &b, RunStatus::Done, images_work);
}

/// [`wake`] with the first report open.
pub fn wake_open(s: &mut App) {
    wake(s);
    let seq = s.books.values().flat_map(|b| b.parts.iter()).find(|p| matches!(&p.body, PartBody::Wake { number, .. } if number == "1")).map(|p| p.seq).expect("the first wake");
    s.open_folds.insert(format!("wake-report-{seq}"));
}

/// A subagent opened on its own: the breadcrumb, its transcript, no composer, Back and Stop.
pub fn view(s: &mut App) {
    let th = thread(TaskStatus::Waiting);
    let a = sub(1, &th, "1", "Check links", "Look at the links on the docs site and say which are dead", TaskStatus::Working, 8);
    let b = sub(2, &th, "2", "Check images", "Look at the images on the docs site and say which are too big", TaskStatus::Working, 8);
    stage(s, th.clone(), vec![a.clone(), b.clone()]);
    sub_book(s, &a, RunStatus::Running, |sc| links_work(sc, false));
    sub_book(s, &b, RunStatus::Running, |_| {});
    s.open = Some(a.id.clone());
}

/// A subagent asking for a permission, answered in its own conversation, led by its number and no face.
pub fn asks(s: &mut App) {
    let th = thread(TaskStatus::Waiting);
    let a = sub(1, &th, "1", "Check links", "Look at the links on the docs site and say which are dead", TaskStatus::NeedsYou, 8);
    let b = sub(2, &th, "2", "Check images", "Look at the images on the docs site and say which are too big", TaskStatus::Working, 8);
    stage(s, th.clone(), vec![a.clone(), b]);
    sub_book(s, &a, RunStatus::Running, |sc| {
        sc.tool("Grep", "href=", "{\n  \"pattern\": \"href=\"\n}", "docs/setup.md:14\ndocs/faq.md:3", ToolState::Done, 2);
        sc.ask("Run a command", "Commands can change anything on this Mac.", "npx linkcheck docs/", false, PermissionState::Pending);
    });
    s.open = Some(a.id.clone());
}

/// Needs you with a subagent's request among the others: "Kitten 1 asks", in the thread's title.
pub fn needs(s: &mut App) {
    let th = thread(TaskStatus::Waiting);
    let a = sub(1, &th, "1", "Check links", "Look at the links on the docs site and say which are dead", TaskStatus::NeedsYou, 8);
    let b = sub(2, &th, "2", "Check images", "Look at the images on the docs site and say which are too big", TaskStatus::Working, 8);
    stage(s, th.clone(), vec![a.clone(), b]);
    let mut item = permission_item(s, &a, "Run a command", "npx linkcheck docs/", false, 4);
    item.title = th.title.clone();
    item.cat = th.cat.clone().unwrap_or_default();
    item.cat_name = cat_name(s, &th.cat);
    item.subagent = Some("1".into());
    s.needs.push(item);
    s.needs.sort_by_key(|i| i.since_ms);
    s.open_needs();
}

// ---- The right pane ---------------------------------------------------------------------------------------------------------------------

/// The thread's overview as the daemon says it, for the scenes: its folder, its branch and the two files the kittens changed.
fn overview(th: &Task) -> neko_protocol::Overview {
    let files = vec![diff_file("docs/setup.md", neko_protocol::FileStatus::Modified, 1, 1), diff_file("docs/faq.md", neko_protocol::FileStatus::Modified, 1, 1), diff_file("src/auth.rs", neko_protocol::FileStatus::Modified, 12, 6)];
    let (added, removed) = (files.iter().map(|f| f.added).sum(), files.iter().map(|f| f.removed).sum());
    let changes = neko_protocol::DiffSummary { task: th.id.clone(), run: None, source: neko_protocol::DiffSource::Thread, root: FOLDER.into(), files, added, removed };
    neko_protocol::Overview { task: th.id.clone(), folder: Some(FOLDER.into()), branch: Some("neko/4f2a".into()), changes: Some(changes), uncommitted: None }
}

fn open_pane(s: &mut App, th: &Task, tab: crate::pane::Tab) {
    s.pane.choice = Some(true);
    s.pane.sheet = true;
    s.pane.overview.insert(th.id.clone(), overview(th));
    s.pane.tab.insert(th.id.clone(), tab);
}

/// The thread at rest on its kittens: a request from Kitten 1 first, the tree (Kitten 1.1 under Kitten 1), what it waits on, the
/// changes so far, the folder and the usage.
pub fn pane_kittens(s: &mut App) {
    let th = thread(TaskStatus::Waiting);
    let a = sub(1, &th, "1", "Check links", "Look at the links on the docs site and say which are dead", TaskStatus::NeedsYou, 8);
    let b = sub(2, &th, "2", "Check images", "Look at the images on the docs site and say which are too big", TaskStatus::Working, 8);
    let mut c = sub(3, &th, "1.1", "Read the redirects", "Read the redirect file and say where each old link goes", TaskStatus::Done, 6);
    if let Some(o) = &mut c.subagent {
        o.parent = a.id.clone();
    }
    stage(s, th.clone(), vec![a.clone(), b.clone(), c.clone()]);
    let mut sc = Script::new(&th, "run_1", start(s));
    first_turn(&mut sc, &[&a, &b], true);
    let mut r = run("run_1", &th, RunStatus::Done, start(s), 31);
    r.changes = None;
    open_thread(s, &th, vec![r], vec![sc]);
    sub_book(s, &a, RunStatus::Running, |sc| {
        sc.tool("Grep", "href=", "{\n  \"pattern\": \"href=\"\n}", "docs/setup.md:14\ndocs/faq.md:3", ToolState::Done, 2);
        sc.ask("Run a command", "Commands can change anything on this Mac.", "npx linkcheck docs/", false, PermissionState::Pending);
    });
    sub_book(s, &b, RunStatus::Running, |_| {});
    sub_book(s, &c, RunStatus::Done, |sc| {
        sc.say("The redirect file sends both old links to the new site.", true);
    });
    let mut item = permission_item(s, &a, "Run a command", "npx linkcheck docs/", false, 4);
    item.title = th.title.clone();
    item.cat = th.cat.clone().unwrap_or_default();
    item.cat_name = cat_name(s, &th.cat);
    item.subagent = Some("1".into());
    s.needs.push(item);
    open_pane(s, &th, crate::pane::Tab::Overview);
}

/// Kitten 1's own tab while it works: its brief folded, Stop and Open in main view, what it said and did so far.
pub fn pane_kitten_live(s: &mut App) {
    let th = thread(TaskStatus::Waiting);
    let a = sub(1, &th, "1", "Check links", "Look at the links on the docs site and say which are dead", TaskStatus::Working, 8);
    let b = sub(2, &th, "2", "Check images", "Look at the images on the docs site and say which are too big", TaskStatus::Working, 8);
    stage(s, th.clone(), vec![a.clone(), b.clone()]);
    let mut sc = Script::new(&th, "run_1", start(s));
    first_turn(&mut sc, &[&a, &b], true);
    let mut r = run("run_1", &th, RunStatus::Done, start(s), 31);
    r.changes = None;
    open_thread(s, &th, vec![r], vec![sc]);
    sub_book(s, &a, RunStatus::Running, |sc| links_work(sc, false));
    sub_book(s, &b, RunStatus::Running, |_| {});
    s.pane.kittens.insert(th.id.clone(), vec![a.id.clone()]);
    open_pane(s, &th, crate::pane::Tab::Kitten(a.id.clone()));
}

/// Kitten 1's tab once it is done: its report first.
pub fn pane_kitten_done(s: &mut App) {
    let th = thread(TaskStatus::Done);
    let a = sub(1, &th, "1", "Check links", "Look at the links on the docs site and say which are dead", TaskStatus::Done, 8);
    let b = sub(2, &th, "2", "Check images", "Look at the images on the docs site and say which are too big", TaskStatus::Done, 8);
    stage(s, th.clone(), vec![a.clone(), b.clone()]);
    let mut sc = Script::new(&th, "run_1", start(s));
    first_turn(&mut sc, &[&a, &b], true);
    let mut r = run("run_1", &th, RunStatus::Done, start(s), 31);
    r.changes = None;
    open_thread(s, &th, vec![r], vec![sc]);
    sub_book(s, &a, RunStatus::Done, |sc| links_work(sc, true));
    sub_book(s, &b, RunStatus::Done, images_work);
    s.pane.kittens.insert(th.id.clone(), vec![a.id.clone(), b.id.clone()]);
    open_pane(s, &th, crate::pane::Tab::Kitten(a.id.clone()));
}

// ---- Own copy, stacking, drafts, depth, limits (S6) ---------------------------------------------------------------------------------------

/// `t` works in a copy of its own on `branch`.
fn in_copy(mut t: Task, branch: &str) -> Task {
    if let Some(o) = &mut t.subagent {
        o.placement = SubPlace::OwnCopy;
        o.branch = Some(branch.into());
    }
    t
}

/// `t` starts after the kitten numbered `n`.
fn after(mut t: Task, n: &str) -> Task {
    if let Some(o) = &mut t.subagent {
        o.after = Some(n.into());
    }
    t
}

/// `t` is written down for the person to start.
fn as_draft(mut t: Task) -> Task {
    if let Some(o) = &mut t.subagent {
        o.draft = true;
    }
    t
}

const MIGRATION: &str = "Write the migration that adds the sessions table: id, user, expires_at, and an index on expires_at. Follow the style of the existing migrations, and commit it on your branch.";
const TESTS: &str = "Update the sign-in tests for the new sessions table. Start from the migration on the branch you follow. Commit on your branch.";

/// A thread that split its work three ways: two writers each in a copy of their own, the second stacked on the first, and a reader in
/// the shared folder.
pub fn kittens_placement(s: &mut App) {
    let th = thread(TaskStatus::Working);
    let a = in_copy(sub(1, &th, "1", "Write the migration", MIGRATION, TaskStatus::Working, 8), "neko/4f2a-1");
    let b = after(in_copy(sub(2, &th, "2", "Update the tests", TESTS, TaskStatus::New, 8), "neko/4f2a-2"), "1");
    let c = sub(3, &th, "3", "Read the docs", "Read the docs folder and list every page that mentions sessions.", TaskStatus::Working, 8);
    stage(s, th.clone(), vec![a.clone(), b.clone(), c.clone()]);
    let mut sc = Script::new(&th, "run_1", start(s));
    sc.tool("Read", "src/auth.rs", "{\n  \"file_path\": \"src/auth.rs\"\n}", "pub fn sign_in() {}", ToolState::Done, 1);
    sc.say("The sign-in needs a sessions table. The migration and the tests that follow it write files, so each gets a copy of the project of its own; the docs only need reading.", true);
    start_tool(&mut sc, "1", "Write the migration", 1);
    card(&mut sc, &a, "1");
    start_tool(&mut sc, "2", "Update the tests", 1);
    card(&mut sc, &b, "2");
    start_tool(&mut sc, "3", "Read the docs", 1);
    card(&mut sc, &c, "3");
    sc.tool("Edit", "src/auth.rs", "{\n  \"file_path\": \"src/auth.rs\"\n}", "", ToolState::Running, 0);
    let r = run("run_1", &th, RunStatus::Running, start(s), 0);
    open_thread(s, &th, vec![r], vec![sc]);
    sub_book(s, &a, RunStatus::Running, |sc| {
        sc.tool("Write", "migrations/0004_sessions.sql", "{\n  \"file_path\": \"migrations/0004_sessions.sql\"\n}", "", ToolState::Running, 0);
    });
    sub_book(s, &c, RunStatus::Running, |sc| {
        sc.tool("Grep", "sessions", "{\n  \"pattern\": \"sessions\"\n}", "docs/auth.md:12", ToolState::Done, 2);
    });
}

/// The thread ended its turn after writing two drafts: nothing waits on them, and each has its prompt to change, Start and Discard.
pub fn kittens_drafts(s: &mut App) {
    let th = thread(TaskStatus::Done);
    let a = as_draft(in_copy(sub(1, &th, "1", "Write the migration", MIGRATION, TaskStatus::New, 8), "neko/4f2a-1"));
    let b = as_draft(after(in_copy(sub(2, &th, "2", "Update the tests", TESTS, TaskStatus::New, 8), "neko/4f2a-2"), "1"));
    stage(s, th.clone(), vec![a.clone(), b.clone()]);
    s.draft_follows(&a);
    s.draft_follows(&b);
    let mut sc = Script::new(&th, "run_1", start(s));
    sc.tool("Read", "src/auth.rs", "{\n  \"file_path\": \"src/auth.rs\"\n}", "pub fn sign_in() {}", ToolState::Done, 1);
    sc.say("The sign-in needs a sessions table. I wrote down two kittens for it and left them as drafts, so you can read what each will be told before anything starts.", true);
    sc.tool("mcp__neko__start_subagent", "Write the migration", "{\n  \"draft\": true,\n  \"title\": \"Write the migration\"\n}", "Subagent 1 (\"Write the migration\") is saved as a draft. It does not run until the person starts it or you call start_drafts.", ToolState::Done, 1);
    card(&mut sc, &a, "1");
    sc.tool("mcp__neko__start_subagent", "Update the tests", "{\n  \"draft\": true,\n  \"title\": \"Update the tests\"\n}", "Subagent 2 (\"Update the tests\") is saved as a draft. It does not run until the person starts it or you call start_drafts.", ToolState::Done, 1);
    card(&mut sc, &b, "2");
    sc.say("Start the first one when you are ready; the second follows it.", true);
    let mut r = run("run_1", &th, RunStatus::Done, start(s), 31);
    r.changes = None;
    open_thread(s, &th, vec![r], vec![sc]);
}

/// A thread that waits on five kittens: three work, two wait their turn. The sidebar row, the activity bar (open) and the cards say so.
pub fn kittens_limits(s: &mut App) {
    let th = thread(TaskStatus::Waiting);
    let mk = |n: u32, title: &str, st: TaskStatus| sub(n, &th, &n.to_string(), title, "Check one section of the docs site and say what is wrong.", st, 8);
    let ks = vec![mk(1, "Check setup", TaskStatus::Working), mk(2, "Check faq", TaskStatus::Working), mk(3, "Check pricing", TaskStatus::Working), mk(4, "Check blog", TaskStatus::New), mk(5, "Check changelog", TaskStatus::New)];
    stage(s, th.clone(), ks.clone());
    let mut sc = Script::new(&th, "run_1", start(s));
    sc.tool("Read", "docs/index.md", "{\n  \"file_path\": \"docs/index.md\"\n}", "# Docs", ToolState::Done, 1);
    sc.say("Five sections to check, and they do not depend on each other. I will start one kitten for each.", true);
    for (i, k) in ks.iter().enumerate() {
        start_tool(&mut sc, &(i + 1).to_string(), &k.title, 1);
        card(&mut sc, k, &(i + 1).to_string());
    }
    sc.say("All five are started. I will pick this up when their reports arrive.", true);
    let mut r = run("run_1", &th, RunStatus::Done, start(s), 31);
    r.changes = None;
    open_thread(s, &th, vec![r], vec![sc]);
    for k in ks.iter().filter(|k| k.status == TaskStatus::Working) {
        sub_book(s, k, RunStatus::Running, |sc| {
            sc.tool("Grep", "href=", "{\n  \"pattern\": \"href=\"\n}", "docs/a.md:4", ToolState::Done, 2);
        });
    }
    s.composer.activity_open = true;
}

/// The right pane's Overview with kittens three levels deep (1, 1.1, 1.1.1, 1.2), one in the shared folder, one stacked and queued, one
/// draft, and the thread waiting on them.
pub fn pane_kittens_deep(s: &mut App) {
    let th = thread(TaskStatus::Waiting);
    let a = in_copy(sub(1, &th, "1", "Write the migration", MIGRATION, TaskStatus::Waiting, 8), "neko/4f2a-1");
    let mut b = in_copy(sub(2, &th, "1.1", "Seed the table", "Write the seed data the tests need.", TaskStatus::Waiting, 7), "neko/4f2a-1-1");
    let mut c = sub(3, &th, "1.1.1", "List the columns", "Read the schema and list every column of the users table.", TaskStatus::Working, 6);
    let mut d = after(in_copy(sub(4, &th, "1.2", "Index the table", "Add the index.", TaskStatus::New, 6), "neko/4f2a-1-2"), "1.1");
    let e = sub(5, &th, "2", "Read the docs", "Read the docs folder and list every page that mentions sessions.", TaskStatus::Done, 8);
    let f = as_draft(in_copy(sub(6, &th, "3", "Write the changelog", "Add a changelog entry for the sessions table.", TaskStatus::New, 4), "neko/4f2a-3"));
    let (aid, bid) = (a.id.clone(), b.id.clone());
    for (t, parent) in [(&mut b, aid.clone()), (&mut c, bid), (&mut d, aid)] {
        if let Some(o) = &mut t.subagent {
            o.parent = parent;
        }
    }
    stage(s, th.clone(), vec![a.clone(), b.clone(), c.clone(), d.clone(), e.clone(), f.clone()]);
    s.draft_follows(&f);
    let mut sc = Script::new(&th, "run_1", start(s));
    sc.tool("Read", "src/auth.rs", "{\n  \"file_path\": \"src/auth.rs\"\n}", "pub fn sign_in() {}", ToolState::Done, 1);
    sc.say("Three parts. I will start them and pick this up when they report.", true);
    start_tool(&mut sc, "1", "Write the migration", 1);
    card(&mut sc, &a, "1");
    let mut r = run("run_1", &th, RunStatus::Done, start(s), 31);
    r.changes = None;
    open_thread(s, &th, vec![r], vec![sc]);
    for k in [&a, &b, &c, &e] {
        sub_book(s, k, if k.status == TaskStatus::Done { RunStatus::Done } else { RunStatus::Running }, |_| {});
    }
    open_pane(s, &th, crate::pane::Tab::Overview);
}
