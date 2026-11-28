//! The Cattain scenes (docs/knowledge/cats.md, "Cattains"; design.md, "A Cattain"): a Cattain's own thread with its turns led by its
//! face, the same thread before anyone wrote in it, its Overview, Digest, Files (RULES.md in the editor, NOTES.md to read) and
//! Profile tabs, a crew thread's band, New thread in a crew, and the sidebar with and without a Cattain at work. Placeholder data only.

use neko_protocol::cats::{self, CattainFile};
use neko_protocol::{Need, NeedsItem, Permission, PermissionId, PermissionState, RunId, RunStatus, Task, TaskId, TaskStatus, ToolState};
use rune::TextEdit;

use super::{Script, run, start, task, with_rounds, with_workspaces};
use crate::pane::Tab;
use crate::state::{App, Book};

/// Its own thread, made for `cat`: titled with its name, in its folder.
pub fn own_thread(s: &mut App, cat: &str) -> TaskId {
    let c = s.cat(cat).cloned().expect("a sample cat");
    let id = TaskId(format!("task_own_{cat}"));
    let mut t = Task::blank(id.clone(), "default", "", &c.name, s.now_ms - 3 * 86_400_000, Some(cat));
    t.cattain = true;
    t.status = TaskStatus::Done;
    s.tasks.retain(|x| x.id != id);
    s.tasks.push(t);
    if let Some(x) = s.cats.iter_mut().find(|x| x.id == cat) {
        x.thread = Some(id.clone());
    }
    id
}

fn file(s: &App, cat: &str, name: &str, text: &str, minutes_ago: Option<i64>, previous: bool) -> CattainFile {
    CattainFile {
        cat: cat.into(),
        name: name.into(),
        text: text.into(),
        cut: false,
        previous,
        changed_ms: minutes_ago.map(|m| s.now_ms - m * 60_000),
        path: format!("/Users/example/Library/Application Support/neko/cattains/{cat}/{name}"),
        editable: cats::editable(name),
    }
}

pub const MOCHI_RULES: &str = "# How I run the docs\n\n- Plan first, in NOTES.md.\n- Hand each page to a thread in my crew; one page per thread.\n- Never merge without asking Asha.\n";
const MOCHI_CREW: &str = "- Use British spelling.\n- Short sentences; one idea each.\n- Open a pull request; never merge it.\n- Name a file before you change it.\n";
const MOCHI_NOTES: &str = "# Plan\n\n1. Fix the broken links in the guide (crew: Docs links).\n2. Rewrite the install page once Lee confirms the new steps.\n3. Then the changelog for 0.2.\n";
const MOCHI_DIGEST: &str = "## Needs you\n\n- **Merge acme-docs#12?** The links fix is ready and its checks pass.\n\n## New\n\n- The install page draft is up for Lee to read.\n- Two redirects remain in the guide; both are external.\n\n## In progress\n\n- Docs links: 31 of 33 links fixed.\n- Release notes for 0.2: waiting on the merged changes.\n";

/// mochi's files, as the daemon would say them.
fn mochi_files(s: &mut App) {
    let files = [
        file(s, "cat_mochi", cats::RULES, MOCHI_RULES, Some(3 * 24 * 60), true),
        file(s, "cat_mochi", cats::CREW, MOCHI_CREW, Some(2 * 24 * 60), false),
        file(s, "cat_mochi", cats::NOTES, MOCHI_NOTES, Some(14), true),
        file(s, "cat_mochi", cats::DIGEST, MOCHI_DIGEST, Some(12), true),
    ];
    for f in files {
        s.take_cattain_file(f);
    }
}

/// mochi, with its own thread, a crew of three in acme-api (one asking, one working, one done) and its rounds.
fn mochi(s: &mut App) -> TaskId {
    with_rounds(s);
    s.cats[1].workspace = Some("ws_acme".into());
    let own = own_thread(s, "cat_mochi");
    let crew = |n: u32, words: &str, minutes: i64, status: TaskStatus| {
        let mut t = task(n, words, minutes);
        t.workspace = "ws_acme".into();
        t.folder = Some("/Users/example/acme-api".into());
        t.cat = Some("cat_mochi".into());
        t.status = status;
        t
    };
    let links = crew(60, "Docs links\n\nFix the broken links in the guide.", 40, TaskStatus::NeedsYou);
    let install = crew(61, "Rewrite the install page", 25, TaskStatus::Working);
    let typo = crew(62, "Fix the typos on the home page", 6 * 60, TaskStatus::Done);
    let ask = Permission {
        id: PermissionId("perm_links".into()),
        task: links.id.clone(),
        run: RunId("run_links".into()),
        tool: "Bash".into(),
        title: "Run a command".into(),
        why: "Commands can change anything on this Mac.".into(),
        detail: "gh pr merge 12".into(),
        destructive: false,
        state: PermissionState::Pending,
        asked_ms: s.now_ms - 9 * 60_000,
        answered_ms: None,
    };
    s.needs = vec![NeedsItem {
        id: "perm:perm_links".into(),
        task: links.id.clone(),
        workspace: links.workspace.clone(),
        title: "Docs links".into(),
        cat: "cat_mochi".into(),
        cat_name: "mochi".into(),
        since_ms: ask.asked_ms,
        need: Need::Permission { permission: ask },
        subagent: None,
    }];
    let mut links = links;
    links.title = "Docs links".into();
    s.tasks.extend([links, install, typo]);
    s.tasks.sort_by_key(|t| std::cmp::Reverse(t.created_ms));
    mochi_files(s);
    own
}

/// mochi's thread with one exchange: the person's question, its answer led by its face.
fn talked(s: &mut App) -> TaskId {
    let own = mochi(s);
    let task = s.tasks.iter().find(|t| t.id == own).cloned().expect("its thread");
    let mut sc = Script::new(&task, "run_m1", start(s));
    sc.person("Where are we with the docs?");
    sc.tool("Read", "NOTES.md", "{}", "# Plan", ToolState::Done, 1);
    sc.tool("Read", "DIGEST.md", "{}", "## Needs you", ToolState::Done, 1);
    sc.say("Two threads are working for you.\n\n- **Docs links** is ready: 31 of 33 links fixed, and it waits for you to allow the merge.\n- **Rewrite the install page** is under way; Lee still has to confirm the new steps.\n\nThe typo fix on the home page is done. I updated the digest.", true);
    let mut r = run("run_m1", &task, RunStatus::Done, start(s), 38);
    r.changes = None;
    r.workdir = "/Users/example/Library/Application Support/neko/cattains/cat_mochi".into();
    r.branch = None;
    s.books.insert(own.clone(), Book { runs: vec![r], loaded: true, parts: sc.parts, ..Book::default() });
    s.open = Some(own.clone());
    own
}

fn show(s: &mut App, own: &TaskId, tab: Tab) {
    s.open = Some(own.clone());
    s.pane.choice = Some(true);
    s.pane.sheet = true;
    s.pane.tab.insert(own.clone(), tab);
}

/// mochi's own thread, its Overview open: Needs you, its crew, its files and its rounds.
pub fn thread(s: &mut App) {
    let own = talked(s);
    show(s, &own, Tab::Overview);
}

/// miso's thread before anyone wrote in it: who it is.
pub fn thread_new(s: &mut App) {
    with_workspaces(s);
    s.cats[0].purpose = Some("Looks after the release: notes, version bumps and the changelog.".into());
    let own = own_thread(s, "cat_miso");
    s.tasks.iter_mut().find(|t| t.id == own).expect("its thread").status = TaskStatus::New;
    s.books.insert(own.clone(), Book { loaded: true, ..Book::default() });
    for name in cats::FILES {
        let f = file(s, "cat_miso", name, "", None, false);
        s.take_cattain_file(f);
    }
    show(s, &own, Tab::Overview);
}

pub fn digest(s: &mut App) {
    let own = talked(s);
    show(s, &own, Tab::Digest);
}

pub fn files(s: &mut App) {
    let own = talked(s);
    s.cattains.chosen.insert("cat_mochi".into(), cats::RULES.into());
    show(s, &own, Tab::Files);
}

/// RULES.md with words not saved yet.
pub fn files_editing(s: &mut App) {
    files(s);
    let d = s.file_draft_mut("cat_mochi", cats::RULES);
    *d = TextEdit::new(&format!("{MOCHI_RULES}- Link ENG-123 in every changelog line.\n"));
    d.focused = true;
}

/// NOTES.md, which mochi keeps: to read.
pub fn files_notes(s: &mut App) {
    let own = talked(s);
    s.cattains.chosen.insert("cat_mochi".into(), cats::NOTES.into());
    show(s, &own, Tab::Files);
}

pub fn profile(s: &mut App) {
    let own = talked(s);
    s.cats[1].model = Some("sonnet".into());
    show(s, &own, Tab::Profile);
}

pub fn retire(s: &mut App) {
    profile(s);
    s.ask_retire_cat("cat_mochi");
}

/// A thread in mochi's crew: its band names the crew.
pub fn crew_thread(s: &mut App) {
    mochi(s);
    let id = s.tasks.iter().find(|t| t.title == "Rewrite the install page").map(|t| t.id.clone()).expect("the crew thread");
    s.books.insert(id.clone(), Book { loaded: true, ..Book::default() });
    s.open = Some(id);
}

/// New thread, started from mochi's Overview: it goes to its crew.
pub fn new_thread_crew(s: &mut App) {
    mochi(s);
    s.open = None;
    s.new_thread.crew = Some("cat_mochi".into());
    s.workspace = "ws_acme".into();
    s.draft = TextEdit::new("Write the changelog for 0.2 from the merged changes");
    s.draft.focused = true;
}

/// A Cattain's Profile (the plugin scenes).
pub fn show_profile(s: &mut App, cat: &str) {
    let own = own_thread(s, cat);
    s.books.insert(own.clone(), Book { loaded: true, ..Book::default() });
    show(s, &own, Tab::Profile);
}

/// The sidebar with mochi at work (its crew asks, and works) and its own thread writing.
pub fn sidebar_at_work(s: &mut App) {
    let own = mochi(s);
    if let Some(t) = s.tasks.iter_mut().find(|t| t.id == own) {
        t.status = TaskStatus::Working;
        t.latest_line = "Reading the crew's reports.".into();
    }
}

/// Nothing at work: the Cattains section is absent, and New thread is as it always is.
pub fn sidebar_quiet(s: &mut App) {
    mochi(s);
    s.needs.clear();
    for t in s.tasks.iter_mut().filter(|t| t.cat.as_deref() == Some("cat_mochi")) {
        t.status = TaskStatus::Done;
    }
}
