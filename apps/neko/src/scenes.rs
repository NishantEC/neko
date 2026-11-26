//! The scenes a headless run can show (`neko --snap out.png --scene task-open`): placeholder data only, never the person's.

use std::collections::HashMap;

use neko_protocol::{
    Changes, Part, PartBody, PartId, Permission, PermissionId, PermissionMode, PermissionState, Pricing, Run, RunId, RunStatus, RuntimeInfo, Settings, Task, TaskId, TaskStatus, Tone, ToolState,
    Usage, UsageTotal,
};
use rune::TextEdit;

use crate::state::{App, Book};

/// A scene: its name and what it does to the fresh state.
pub type Scene = (&'static str, fn(&mut App));

pub const SCENES: &[Scene] = &[
    ("empty-home", |_| {}),
    ("home-typed", home_typed),
    ("home-folder", home_folder),
    ("task-open", task_open),
    ("tasks-home", tasks_home),
    ("task-working", task_working),
    ("task-needs-you", task_needs_you),
    ("task-needs-you-destructive", task_needs_you_destructive),
    ("task-done", task_done),
    ("task-done-open", task_done_open),
    ("task-follow-up", task_follow_up),
    ("task-failed", task_failed),
    ("task-stopped", task_stopped),
    ("task-interrupted", task_interrupted),
    ("settings", settings),
    ("settings-full-access", settings_full),
    ("settings-signed-out", settings_signed_out),
];

/// A task made `minutes_ago` before the fixed clock.
fn task(n: u32, words: &str, minutes_ago: i64) -> Task {
    let (now, _) = crate::fixed_clock();
    let title = words.lines().next().unwrap_or("").to_owned();
    Task { id: TaskId(format!("task_{n:026}")), workspace: "default".into(), words: words.into(), title, created_ms: now - minutes_ago * 60_000, status: TaskStatus::New, folder: None }
}

/// Four tasks: three from today and one from earlier in the week, each in a different state.
pub fn sample_tasks() -> Vec<Task> {
    let mut t = vec![
        task(4, "Draft the release notes for 0.2 and send them to Lee for a read", 12),
        task(3, "Rename the Settings button to Preferences across the app\n\nKeep the keyboard shortcut (⌘,) and check ENG-123 is covered.", 48),
        task(2, "Find out why the nightly build of acme-api is slow", 190),
        task(1, "Tidy the Downloads folder into Archive and Screenshots", 3 * 24 * 60),
    ];
    t[0].status = TaskStatus::Working;
    t[1].status = TaskStatus::New;
    t[2].status = TaskStatus::Done;
    t[3].status = TaskStatus::Failed;
    t
}

fn home_typed(s: &mut App) {
    s.draft = TextEdit::new("Rename the Settings button to Preferences across the app");
    s.draft.focused = true;
}

fn home_folder(s: &mut App) {
    s.tasks = sample_tasks();
    s.draft = TextEdit::new("Find out why the nightly build is slow");
    s.folder = Some("/Users/example/acme-api".into());
    s.day = Some(UsageTotal { runs: 3, input_tokens: 41_200, output_tokens: 6_300, cost_usd: Some(0.58), api_equivalent: true });
}

fn tasks_home(s: &mut App) {
    s.tasks = sample_tasks();
    s.day = Some(UsageTotal { runs: 3, input_tokens: 41_200, output_tokens: 6_300, cost_usd: Some(0.58), api_equivalent: true });
}

fn task_open(s: &mut App) {
    s.tasks = sample_tasks();
    s.open = Some(s.tasks[1].id.clone());
    s.books.insert(s.tasks[1].id.clone(), Book { loaded: true, ..Book::default() });
}

/// Builds a task's transcript in order, at a minute's pace from `start_ms`.
struct Script {
    task: TaskId,
    run: RunId,
    at: i64,
    seq: u64,
    parts: Vec<Part>,
    perms: HashMap<PermissionId, Permission>,
}

impl Script {
    fn new(task: &Task, run: &str, at: i64) -> Script {
        Script { task: task.id.clone(), run: RunId(run.into()), at, seq: 0, parts: Vec::new(), perms: HashMap::new() }
    }

    fn part(&mut self, body: PartBody, secs: i64, done: bool) -> &mut Self {
        self.seq += 1;
        let at = self.at;
        self.at += secs * 1000;
        self.parts.push(Part { id: PartId(format!("part_{}_{}", self.run.0, self.seq)), task: self.task.clone(), run: Some(self.run.clone()), seq: self.seq, at_ms: at, ended_ms: done.then_some(self.at), body });
        self
    }

    fn person(&mut self, words: &str) -> &mut Self {
        self.part(PartBody::Person { text: words.into() }, 1, true)
    }

    fn think(&mut self, words: &str, secs: i64) -> &mut Self {
        self.part(PartBody::Thinking { text: words.into(), done: true }, secs, true)
    }

    fn tool(&mut self, name: &str, summary: &str, input: &str, output: &str, state: ToolState, secs: i64) -> &mut Self {
        let done = state != ToolState::Running;
        self.part(PartBody::Tool { call: format!("call_{}", self.seq), name: name.into(), summary: summary.into(), input: input.into(), output: done.then(|| output.into()), state }, secs, done)
    }

    fn say(&mut self, words: &str, done: bool) -> &mut Self {
        self.part(PartBody::Agent { text: words.into(), done }, 4, done)
    }

    fn ask(&mut self, title: &str, why: &str, detail: &str, destructive: bool, state: PermissionState) -> &mut Self {
        let id = PermissionId(format!("perm_{}_{}", self.run.0, self.seq + 1));
        self.perms.insert(
            id.clone(),
            Permission { id: id.clone(), task: self.task.clone(), run: self.run.clone(), tool: "Bash".into(), title: title.into(), why: why.into(), detail: detail.into(), destructive, state, asked_ms: self.at, answered_ms: None },
        );
        self.part(PartBody::Permission { permission: id }, 6, true)
    }

    fn notice(&mut self, tone: Tone, words: &str, next: &str) -> &mut Self {
        self.part(PartBody::Notice { tone, text: words.into(), next: Some(next.into()) }, 0, true)
    }
}

const ANSWER: &str = "I renamed the button and its menu item, and kept **⌘,** as the shortcut.\n\n- `SettingsButton` is now `PreferencesButton` in `src/toolbar.rs`\n- The menu item reads **Preferences…**\n- ENG-123's check passes: the old label appears nowhere\n\n```rust\nButton::new(\"preferences\", \"Preferences\").keys(\"⌘,\")\n```\n\nNothing was merged or pushed. The work is on its own branch.";

fn run(id: &str, task: &Task, status: RunStatus, started: i64, secs: i64) -> Run {
    Run {
        id: RunId(id.into()),
        task: task.id.clone(),
        runtime: "claude".into(),
        model: Some("claude-model".into()),
        status,
        started_ms: started,
        ended_ms: (status != RunStatus::Running).then_some(started + secs * 1000),
        session: Some("5f0c2a".into()),
        workdir: "/Users/example/Library/Application Support/neko/worktrees/x7k2m9qa".into(),
        branch: Some("neko/x7k2m9qa".into()),
        reason: None,
        usage: (status != RunStatus::Running).then(|| Usage {
            input_tokens: 38_400,
            output_tokens: 5_100,
            cached_tokens: 212_000,
            cost_usd: Some(0.42),
            model: Some("claude-model".into()),
            pricing: Pricing::ApiEquivalent,
            reported_by: "Claude Code".into(),
        }),
        changes: (status == RunStatus::Done).then_some(Changes { files: 3, added: 42, removed: 7 }),
    }
}

/// The open task (the second sample) with these runs and parts.
fn open_with(s: &mut App, status: TaskStatus, runs: Vec<Run>, scripts: Vec<Script>) {
    s.tasks = sample_tasks();
    s.tasks[1].status = status;
    s.tasks[1].folder = Some("/Users/example/acme-app".into());
    let id = s.tasks[1].id.clone();
    let mut book = Book { runs, loaded: true, ..Book::default() };
    for sc in scripts {
        book.parts.extend(sc.parts);
        book.permissions.extend(sc.perms);
    }
    s.books.insert(id.clone(), book);
    s.open = Some(id);
    s.day = Some(UsageTotal { runs: 3, input_tokens: 41_200, output_tokens: 6_300, cost_usd: Some(0.58), api_equivalent: true });
}

fn start(s: &App) -> i64 {
    s.now_ms - 47 * 60_000
}

fn steps(sc: &mut Script) {
    sc.think("The button is drawn in the toolbar and named again in the menu. I will find every place the old label appears before changing any of them, so nothing is left half renamed.", 4);
    sc.tool("Grep", "Settings", "{\n  \"pattern\": \"Settings\"\n}", "src/toolbar.rs:12\nsrc/menu.rs:48\ntests/labels.rs:7", ToolState::Done, 1);
    sc.tool("Read", "src/toolbar.rs", "{\n  \"file_path\": \"src/toolbar.rs\"\n}", "pub fn toolbar() -> El {\n    row().child(Button::new(\"settings\", \"Settings\"))\n}", ToolState::Done, 1);
    sc.tool("Edit", "src/toolbar.rs", "{\n  \"file_path\": \"src/toolbar.rs\"\n}", "The file was edited.", ToolState::Done, 2);
}

fn task_working(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    steps(&mut sc);
    sc.tool("Edit", "src/menu.rs", "{\n  \"file_path\": \"src/menu.rs\"\n}", "The file was edited.", ToolState::Done, 2);
    sc.say("I renamed the button in the toolbar and the menu item. Now I am checking that the old label appears", false);
    let r = run("run_1", task, RunStatus::Running, start(s), 0);
    open_with(s, TaskStatus::Working, vec![r], vec![sc]);
}

fn task_needs_you(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    steps(&mut sc);
    sc.ask("Run a command", "Commands can change anything on this Mac.", "cargo test --package acme-app labels", false, PermissionState::Pending);
    let r = run("run_1", task, RunStatus::Running, start(s), 0);
    open_with(s, TaskStatus::NeedsYou, vec![r], vec![sc]);
}

fn task_needs_you_destructive(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    steps(&mut sc);
    sc.ask("Run a command that deletes or overwrites", "This command deletes or overwrites things.", "rm -rf target/old-screenshots", true, PermissionState::Pending);
    let r = run("run_1", task, RunStatus::Running, start(s), 0);
    open_with(s, TaskStatus::NeedsYou, vec![r], vec![sc]);
}

fn done_script(s: &App, task: &Task) -> Script {
    let mut sc = Script::new(task, "run_1", start(s));
    steps(&mut sc);
    sc.ask("Run a command", "Commands can change anything on this Mac.", "cargo test --package acme-app labels", false, PermissionState::Allowed);
    sc.tool("Bash", "cargo test --package acme-app labels", "cargo test --package acme-app labels", "test result: ok. 14 passed; 0 failed", ToolState::Done, 38);
    sc.say(ANSWER, true);
    sc
}

fn task_done(s: &mut App) {
    let task = &sample_tasks()[1];
    let sc = done_script(s, task);
    let r = run("run_1", task, RunStatus::Done, start(s), 134);
    open_with(s, TaskStatus::Done, vec![r], vec![sc]);
}

fn task_done_open(s: &mut App) {
    task_done(s);
    s.open_folds.insert("worked-run_1".into());
    s.open_folds.insert("part-1".into());
}

fn task_follow_up(s: &mut App) {
    let task = &sample_tasks()[1];
    let sc = done_script(s, task);
    let first = run("run_1", task, RunStatus::Done, start(s), 134);
    let mut sc2 = Script::new(task, "run_2", start(s) + 20 * 60_000);
    sc2.person("Also rename it in the help pages.");
    sc2.tool("Grep", "Settings docs/", "{\n  \"pattern\": \"Settings\",\n  \"path\": \"docs/\"\n}", "docs/help.md:31", ToolState::Done, 1);
    sc2.tool("Edit", "docs/help.md", "{\n  \"file_path\": \"docs/help.md\"\n}", "The file was edited.", ToolState::Done, 1);
    sc2.say("The help page now says **Preferences** too. That was the last place.", true);
    let mut second = run("run_2", task, RunStatus::Done, start(s) + 20 * 60_000, 31);
    second.changes = Some(Changes { files: 4, added: 44, removed: 9 });
    if let Some(u) = &mut second.usage {
        u.cost_usd = Some(0.06);
        u.input_tokens = 9_800;
        u.output_tokens = 640;
    }
    open_with(s, TaskStatus::Done, vec![first, second], vec![sc, sc2]);
}

fn task_failed(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    sc.think("I will look for the button first.", 2);
    sc.tool("Grep", "Settings", "{\n  \"pattern\": \"Settings\"\n}", "src/toolbar.rs:12", ToolState::Done, 1);
    sc.notice(Tone::Error, "Claude Code has reached its usage limit.", "It resets at 17:00. Send a follow-up then to carry on.");
    let mut r = run("run_1", task, RunStatus::Failed, start(s), 9);
    r.reason = Some("Claude Code has reached its usage limit.".into());
    r.usage = Some(Usage { input_tokens: 2_100, output_tokens: 90, cached_tokens: 0, cost_usd: None, model: None, pricing: Pricing::ApiEquivalent, reported_by: "Claude Code".into() });
    open_with(s, TaskStatus::Failed, vec![r], vec![sc]);
}

fn task_stopped(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    steps(&mut sc);
    sc.say("I renamed the button in the toolbar. Next I will", true);
    let mut r = run("run_1", task, RunStatus::Stopped, start(s), 41);
    r.reason = Some("You stopped it.".into());
    r.changes = Some(Changes { files: 1, added: 1, removed: 1 });
    open_with(s, TaskStatus::Stopped, vec![r], vec![sc]);
}

fn task_interrupted(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    steps(&mut sc);
    sc.say("I renamed the button in the toolbar and", true);
    let mut r = run("run_1", task, RunStatus::Interrupted, start(s), 52);
    r.reason = Some("Neko stopped while this was running.".into());
    r.usage = None;
    let mut loose = Script::new(task, "none", start(s) + 60_000);
    loose.notice(Tone::Warning, "Neko stopped while this was running. What was written is kept.", "Send a follow-up to carry on.");
    for p in &mut loose.parts {
        p.run = None;
        p.seq += 100;
    }
    open_with(s, TaskStatus::Interrupted, vec![r], vec![sc, loose]);
}

fn agents(signed_in: bool, mode: PermissionMode) -> (Vec<RuntimeInfo>, Settings) {
    (
        vec![
            RuntimeInfo {
                id: "claude".into(),
                name: "Claude Code".into(),
                found: true,
                version: Some("2.1.0".into()),
                signed_in: Some(signed_in),
                login: signed_in.then(|| "Claude subscription (Max)".into()),
                sign_in: "claude auth login".into(),
                install: "curl -fsSL https://claude.ai/install.sh | bash".into(),
                supported: true,
            },
            RuntimeInfo {
                id: "codex".into(),
                name: "Codex".into(),
                found: false,
                version: None,
                signed_in: None,
                login: None,
                sign_in: "codex login".into(),
                install: "npm install -g @openai/codex".into(),
                supported: false,
            },
        ],
        Settings { runtime: "claude".into(), permission_mode: mode },
    )
}

fn settings(s: &mut App) {
    s.tasks = sample_tasks();
    s.settings_open = true;
    s.runtimes = Some(agents(true, PermissionMode::Ask));
}

fn settings_full(s: &mut App) {
    settings(s);
    s.runtimes = Some(agents(true, PermissionMode::Full));
}

fn settings_signed_out(s: &mut App) {
    settings(s);
    s.runtimes = Some(agents(false, PermissionMode::Ask));
}
