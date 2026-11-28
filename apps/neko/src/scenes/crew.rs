//! The scenes of a Cattain running its crew (docs/knowledge/cats.md, "The crew"; design.md, "A Cattain's crew"): miso after starting
//! three crew threads, resting on them, with one asking the person (the amber count on its row); miso woken by two of them, its digest
//! rewritten; and a crew thread miso started, opening on miso's brief and later taking miso's message. Placeholder data only.

use neko_protocol::cats::{self, CattainFile};
use neko_protocol::{Need, NeedsItem, PartBody, Permission, PermissionId, PermissionState, RunId, RunStatus, Task, TaskId, TaskStatus, ToolState};

use super::{Script, run, start, task, with_workspaces};
use crate::pane::Tab;
use crate::state::{App, Book};

const ASKED: &str = "Fix the three flaky tests in acme-api, one thread each. Tell me when they are ready to merge.";
const FOLDER: &str = "/Users/example/Library/Application Support/neko/cattains/cat_miso";

const TITLES: [&str; 3] = ["Fix the flaky login test", "Fix the flaky upload test", "Fix the flaky search test"];

const BRIEF: &str = "The login test in tests/login.rs fails about one run in ten on CI. Find why, fix it so it passes 20 runs in a row, and commit the fix on your branch. Do not merge: say when it is ready.";

const DIGEST_BEFORE: &str = "## Needs you\n\n- Nothing yet.\n\n## New\n\n- Started three threads, one for each flaky test.\n\n## In progress\n\n- Fix the flaky login test\n- Fix the flaky upload test\n- Fix the flaky search test\n";
const DIGEST_AFTER: &str = "## Needs you\n\n- **Fix the flaky search test** waits for you to allow `npm test -- --runInBand`.\n- **Merge the login and upload fixes?** Both pass 20 runs in a row. Say so in my thread and I will tell them.\n\n## New\n\n- Login: the test raced the session cookie; it now waits for it. Branch `neko/4f2a`.\n- Upload: a fixed port clashed with another test; it takes a free one now. Branch `neko/7c1e`.\n\n## In progress\n\n- Fix the flaky search test: waiting on you, above.\n";

fn file(s: &App, name: &str, text: &str, minutes_ago: i64) -> CattainFile {
    CattainFile { cat: "cat_miso".into(), name: name.into(), text: text.into(), cut: false, previous: true, changed_ms: Some(s.now_ms - minutes_ago * 60_000), path: format!("{FOLDER}/{name}"), editable: cats::editable(name) }
}

fn files(s: &mut App, digest: &str, digest_ago: i64) {
    let all = [
        file(s, cats::RULES, "- Plan in NOTES.md, then hand the work to my crew, one thread for each part.\n- Never merge without asking Asha.\n", 3 * 24 * 60),
        file(s, cats::CREW, "- Commit on your own branch; never merge.\n- Say how you checked the fix.\n", 2 * 24 * 60),
        file(s, cats::NOTES, "# Plan\n\n1. One thread for each flaky test.\n2. When all three pass 20 runs, ask Asha about merging.\n", 30),
        file(s, cats::DIGEST, digest, digest_ago),
    ];
    for f in all {
        s.take_cattain_file(f);
    }
}

/// miso in acme-api, its own thread and its three crew threads; the third waits for the person to allow a command.
fn crew(s: &mut App, done: usize) -> (TaskId, Vec<TaskId>) {
    with_workspaces(s);
    s.cats[0].workspace = Some("ws_acme".into());
    s.cats[0].purpose = Some("Keeps acme-api's tests green.".into());
    let own = super::cattains::own_thread(s, "cat_miso");
    let mut ids = Vec::new();
    for (i, title) in TITLES.iter().enumerate() {
        let mut t = task(80 + i as u32, &BRIEF.replace("login", &title.replace("Fix the flaky ", "").replace(" test", "")), 41 - i as i64);
        t.title = (*title).into();
        t.workspace = "ws_acme".into();
        t.folder = Some("/Users/example/acme-api".into());
        t.cat = Some("cat_miso".into());
        t.started_by = Some("cat_miso".into());
        t.status = if i < done { TaskStatus::Done } else if i == 2 { TaskStatus::NeedsYou } else { TaskStatus::Working };
        t.latest_line = match (i, i < done) {
            (0, true) => "Ready: 20 runs in a row pass.".into(),
            (1, true) => "Ready: it takes a free port now.".into(),
            (2, _) => "I need to run the search test on its own.".into(),
            _ => "Running the test 20 times…".into(),
        };
        t.activity_ms = s.now_ms - (if i < done { 4 - i as i64 } else { 2 }) * 60_000;
        ids.push(t.id.clone());
        s.tasks.push(t);
    }
    let ask = Permission {
        id: PermissionId("perm_search".into()),
        task: ids[2].clone(),
        run: RunId("run_search".into()),
        tool: "Bash".into(),
        title: "Run a command".into(),
        why: "Commands can change anything on this Mac.".into(),
        detail: "npm test -- --runInBand tests/search.test.ts".into(),
        destructive: false,
        state: PermissionState::Pending,
        asked_ms: s.now_ms - 6 * 60_000,
        answered_ms: None,
    };
    s.needs = vec![NeedsItem {
        id: "perm:perm_search".into(),
        task: ids[2].clone(),
        workspace: "ws_acme".into(),
        title: TITLES[2].into(),
        cat: "cat_miso".into(),
        cat_name: "miso".into(),
        since_ms: ask.asked_ms,
        need: Need::Permission { permission: ask },
        subagent: None,
    }];
    s.tasks.sort_by_key(|t| std::cmp::Reverse(t.created_ms));
    (own, ids)
}

fn started_answer(title: &str, n: u32) -> String {
    format!("\"{title}\" is started in your crew in the project acme-api (thread task_{n:026}). It is working now. It reads your CREW.md on every run. Neko wakes you when its turn ends or when it needs the person. Do not poll or wait. If you have nothing else to do, update DIGEST.md and NOTES.md and end your turn now.")
}

/// miso's first turn: it read its files, started three crew threads and said so.
fn first_turn(s: &App, own: &Task) -> (Vec<neko_protocol::Run>, Vec<neko_protocol::Part>) {
    let at = start(s);
    let mut sc = Script::new(own, "run_miso_1", at);
    sc.person(ASKED);
    sc.tool("Read", "NOTES.md", "{}", "# Plan", ToolState::Done, 1);
    for (i, title) in TITLES.iter().enumerate() {
        sc.tool("mcp__neko__start_crew_thread", title, &format!("{{\n  \"title\": \"{title}\",\n  \"brief\": \"…\"\n}}"), &started_answer(title, 80 + i as u32), ToolState::Done, 1);
    }
    sc.tool("Write", "DIGEST.md", "{}", "Wrote the file.", ToolState::Done, 1);
    sc.say("I started three threads in my crew, one for each flaky test. Each fixes its test on its own branch and does not merge.\n\nI will tell you when they are ready; nothing is merged unless you say so.", true);
    let mut r = run("run_miso_1", own, RunStatus::Done, at, 26);
    r.changes = None;
    r.branch = None;
    r.workdir = FOLDER.into();
    (vec![r], sc.parts)
}

fn show(s: &mut App, own: &TaskId, tab: Tab) {
    s.open = Some(own.clone());
    s.pane.choice = Some(true);
    s.pane.sheet = true;
    s.pane.tab.insert(own.clone(), tab);
}

/// miso rests on its crew: three at work, one asking the person (the amber count on its row and first in its Overview).
pub fn at_work(s: &mut App) {
    let (own, _) = crew(s, 0);
    let task = s.tasks.iter().find(|t| t.id == own).cloned().expect("its thread");
    let (runs, parts) = first_turn(s, &task);
    if let Some(t) = s.tasks.iter_mut().find(|t| t.id == own) {
        t.status = TaskStatus::Waiting;
    }
    s.books.insert(own.clone(), Book { runs, loaded: true, parts, ..Book::default() });
    files(s, DIGEST_BEFORE, 40);
    show(s, &own, Tab::Overview);
}

/// miso woken by two crew threads' turn ends (one note each), its digest rewritten; the third still asks the person.
fn woken(s: &mut App) -> TaskId {
    let (own, ids) = crew(s, 2);
    let task = s.tasks.iter().find(|t| t.id == own).cloned().expect("its thread");
    let (mut runs, mut parts) = first_turn(s, &task);
    let at = s.now_ms - 3 * 60_000;
    let mut sc = Script::new(&task, "run_miso_2", at);
    sc.seq = parts.len() as u64;
    sc.part(PartBody::CrewWake { thread: ids[0].clone(), title: TITLES[0].into(), status: "done".into(), summary: "The test raced the session cookie. It now waits for the cookie before it signs in; 20 runs in a row pass. Committed on neko/4f2a.".into(), files: vec!["tests/login.rs +6 -2".into()], question: None }, 0, true);
    sc.part(PartBody::CrewWake { thread: ids[1].clone(), title: TITLES[1].into(), status: "done".into(), summary: "A fixed port clashed with the server test. It asks for a free port now; 20 runs pass. Committed on neko/7c1e.".into(), files: vec!["tests/upload.rs +4 -4".into()], question: None }, 0, true);
    sc.part(PartBody::CrewWake { thread: ids[2].clone(), title: TITLES[2].into(), status: "needs_you".into(), summary: "Run a command: npm test -- --runInBand tests/search.test.ts".into(), files: Vec::new(), question: None }, 0, true);
    sc.tool("Read", "DIGEST.md", "{}", "## Needs you", ToolState::Done, 1);
    sc.tool("Write", "DIGEST.md", "{}", "Wrote the file.", ToolState::Done, 1);
    sc.tool("Write", "NOTES.md", "{}", "Wrote the file.", ToolState::Done, 1);
    sc.say("Two of the three are ready: the login and upload tests pass 20 runs in a row on their branches. The search test waits for you to allow a command.\n\nShall I tell the two to merge? I will not merge anything until you say so.", true);
    let mut r = run("run_miso_2", &task, RunStatus::Done, at, 18);
    r.changes = None;
    r.branch = None;
    r.workdir = FOLDER.into();
    runs.push(r);
    parts.extend(sc.parts);
    if let Some(t) = s.tasks.iter_mut().find(|t| t.id == own) {
        t.status = TaskStatus::Waiting;
    }
    s.books.insert(own.clone(), Book { runs, loaded: true, parts, ..Book::default() });
    files(s, DIGEST_AFTER, 1);
    s.flip("crew-wake-said-8");
    own
}

/// miso woken by its crew, the Overview open.
pub fn woken_overview(s: &mut App) {
    let own = woken(s);
    show(s, &own, Tab::Overview);
}

/// The same, on its Digest: rewritten a minute ago.
pub fn digest(s: &mut App) {
    let own = woken(s);
    show(s, &own, Tab::Digest);
}

/// A crew thread miso started: "Started by miso" above, miso's brief, its turn, then miso passing on the person's decision.
pub fn started_by(s: &mut App) {
    let (_, ids) = crew(s, 2);
    let t = s.tasks.iter().find(|t| t.id == ids[0]).cloned().expect("the crew thread");
    let at = start(s) + 60_000;
    let mut sc = Script::new(&t, "run_login_1", at);
    sc.tool("Read", "tests/login.rs", "{}", "#[test]\nfn signs_in() {", ToolState::Done, 1);
    sc.tool("Bash", "cargo test signs_in -- --test-threads 1", "{}", "test result: ok. 1 passed", ToolState::Done, 9);
    sc.tool("Edit", "tests/login.rs", "{}", "The file was edited.", ToolState::Done, 2);
    sc.tool("Bash", "git commit -am 'Wait for the session cookie in the login test'", "{}", "1 file changed", ToolState::Done, 1);
    sc.say("The test raced the session cookie: it signed in before the cookie was set. It now waits for the cookie. 20 runs in a row pass.\n\nCommitted on `neko/4f2a`. I did not merge.", true);
    let mut r1 = run("run_login_1", &t, RunStatus::Done, at, 120);
    r1.workdir = "/Users/example/Library/Application Support/neko/worktrees/4f2a".into();
    r1.branch = Some("neko/4f2a".into());
    let at2 = s.now_ms - 60_000;
    let mut sc2 = Script::new(&t, "run_login_2", at2);
    sc2.seq = sc.parts.len() as u64;
    sc2.person(&cats::from_cattain("miso", "Asha said in my thread: \u{201c}merge the login and upload fixes\u{201d}. Merge your branch into main and say when it is done."));
    sc2.tool("Bash", "git merge neko/4f2a", "{}", "Fast-forward", ToolState::Done, 2);
    sc2.say("Merged `neko/4f2a` into main, as Asha asked. The login test passes.", true);
    let mut r2 = run("run_login_2", &t, RunStatus::Done, at2, 20);
    r2.workdir = r1.workdir.clone();
    r2.branch = r1.branch.clone();
    let mut parts = sc.parts;
    parts.extend(sc2.parts);
    s.books.insert(t.id.clone(), Book { runs: vec![r1, r2], loaded: true, parts, ..Book::default() });
    s.open = Some(t.id.clone());
}

/// The sidebar while miso's crew works: miso's row with its three crew threads nested and the amber count of what waits on the person.
pub fn sidebar(s: &mut App) {
    let (own, _) = crew(s, 0);
    if let Some(t) = s.tasks.iter_mut().find(|t| t.id == own) {
        t.status = TaskStatus::Waiting;
    }
    s.books.insert(own.clone(), Book { loaded: true, ..Book::default() });
    s.open = None;
}
