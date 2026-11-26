//! "Needs you", derived from the store: each kind of item, each way it ends, the order, and a restart.

use neko_core::conductor::{RunBook, Who};
use neko_core::needs::{self, derive};
use neko_core::runtime::{Action, AgentEvent, Failure, Outcome};
use neko_core::{Core, FixedClock, Place, SequentialIds};
use neko_protocol::{Decision, FailureKind, Need, NeedsItem, PermissionMode, Pricing, TaskId, TaskStatus};

struct Gone(std::path::PathBuf);
impl Drop for Gone {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir() -> Gone {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("neko-needs-test-{}-{n}", std::process::id()));
    std::fs::create_dir_all(d.join("w")).unwrap();
    Gone(d)
}

fn open(d: &Gone) -> Core {
    Core::open(&d.0.join("neko.sqlite3"), Box::new(FixedClock::new(1_764_000_000_000, 1000)), Box::new(SequentialIds::default())).unwrap()
}

fn who() -> Who {
    Who { id: "claude".into(), name: "Claude Code".into(), sign_in: "claude auth login".into(), install: "curl -fsSL https://claude.ai/install.sh | bash".into(), pricing: Pricing::ApiEquivalent }
}

fn begin(core: &mut Core, d: &Gone, task: &TaskId) -> RunBook {
    let place = Place { workdir: d.0.join("w").to_string_lossy().into_owned(), branch: None, base_commit: None, repo: None, plain: false };
    RunBook::begin(core, task, who(), place, PermissionMode::Follow, None, None).unwrap().0
}

fn say(core: &mut Core, b: &mut RunBook, words: &str) {
    let now = core.now();
    b.on(core, AgentEvent::TextDelta(words.into()), now).unwrap();
    let now = core.now();
    b.on(core, AgentEvent::TextDone, now).unwrap();
}

fn finish(core: &mut Core, b: &mut RunBook, o: Outcome) {
    let now = core.now();
    b.finish(core, o, None, now).unwrap();
}

fn ask(core: &mut Core, b: &mut RunBook, id: &str) {
    let now = core.now();
    b.on(core, AgentEvent::PermissionAsked { id: id.into(), tool: "Bash".into(), action: Action::Command { command: "cargo test".into() }, tool_use: None }, now).unwrap();
}

fn status(core: &Core, t: &TaskId) -> TaskStatus {
    core.store.get_task(t).unwrap().unwrap().status
}

#[test]
fn a_pending_request_needs_the_person_until_it_is_answered_or_its_run_ends() {
    let d = dir();
    let mut core = open(&d);
    let (task, _) = core.create_task("Run the tests").unwrap();
    let mut b = begin(&mut core, &d, &task.id);
    assert!(derive(&core.store).unwrap().is_empty(), "working is not needing the person");
    ask(&mut core, &mut b, "cli-1");
    let items = derive(&core.store).unwrap();
    assert_eq!(items.len(), 1);
    let Need::Permission { permission } = &items[0].need else { panic!("{items:?}") };
    assert_eq!((items[0].id.as_str(), items[0].title.as_str(), items[0].since_ms), (format!("perm:{}", permission.id.0).as_str(), "Run the tests", permission.asked_ms));
    let now = core.now();
    b.answer(&mut core, &permission.id, Decision::AllowOnce, now).unwrap();
    assert!(derive(&core.store).unwrap().is_empty(), "answered: gone at once");
    // A second request, and the run ends before anyone answers: it expires, and the task needs nothing.
    ask(&mut core, &mut b, "cli-2");
    assert_eq!(derive(&core.store).unwrap().len(), 1);
    say(&mut core, &mut b, "The tests pass.");
    finish(&mut core, &mut b, Outcome::Done);
    assert_eq!(status(&core, &task.id), TaskStatus::Done);
    assert!(derive(&core.store).unwrap().is_empty(), "an expired request and a done task need nothing");
}

#[test]
fn a_question_waits_until_a_follow_up_starts_a_new_run() {
    let d = dir();
    let mut core = open(&d);
    let (task, _) = core.create_task("Rename the button").unwrap();
    let mut b = begin(&mut core, &d, &task.id);
    say(&mut core, &mut b, "I found four places.\n\nShould I rename **all four**, or keep `Settings`?");
    finish(&mut core, &mut b, Outcome::Done);
    let items = derive(&core.store).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].need, Need::Question { run: b.run.id.clone(), question: "Should I rename all four, or keep Settings?".into() });
    assert_eq!(items[0].since_ms, b.run.ended_ms.unwrap(), "it has waited since the run ended");
    // The answer is a follow-up: a new run, working, then done.
    let mut next = begin(&mut core, &d, &task.id);
    assert!(derive(&core.store).unwrap().is_empty(), "answered: gone at once");
    say(&mut core, &mut next, "Renamed all four.");
    finish(&mut core, &mut next, Outcome::Done);
    assert!(derive(&core.store).unwrap().is_empty(), "a task that is done needs nothing");
}

#[test]
fn only_a_failure_the_person_can_act_on_needs_them() {
    let d = dir();
    let mut core = open(&d);
    let mut fail = |words: &str, f: Failure| {
        let (task, _) = core.create_task(words).unwrap();
        let mut b = begin(&mut core, &d, &task.id);
        finish(&mut core, &mut b, Outcome::Failed(f));
        task.id
    };
    let signin = fail("Sign in", Failure::NotSignedIn);
    let limit = fail("Limit", Failure::UsageLimit { resets: Some("at 17:00".into()) });
    let gone = fail("Gone", Failure::FolderGone);
    let missing = fail("Missing", Failure::NotInstalled);
    for f in [Failure::Crashed { code: Some(3), words: "fatal".into() }, Failure::Reported("no".into()), Failure::TimedOut, Failure::Stalled, Failure::HookMissing, Failure::Unchecked { step: None }] {
        fail("Other", f);
    }
    let (stopped, _) = core.create_task("Stopped").unwrap();
    let mut b = begin(&mut core, &d, &stopped.id);
    finish(&mut core, &mut b, Outcome::Stopped);
    let items = derive(&core.store).unwrap();
    let kinds: Vec<(TaskId, FailureKind, Option<String>)> = items
        .iter()
        .map(|i| match &i.need {
            Need::Failure { failure, command, .. } => (i.task.clone(), *failure, command.clone()),
            n => panic!("only failures here: {n:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            (signin, FailureKind::NotSignedIn, Some("claude auth login".into())),
            (limit.clone(), FailureKind::UsageLimit, None),
            (gone, FailureKind::FolderMissing, None),
            (missing, FailureKind::NotInstalled, Some("curl -fsSL https://claude.ai/install.sh | bash".into())),
        ],
        "a crash, a reported failure, the limits, a missing hook, an unchecked step and a stop need nothing"
    );
    let Need::Failure { text, next, .. } = &items[1].need else { unreachable!() };
    assert_eq!((text.as_str(), next.as_deref()), ("Claude Code has reached its usage limit.", Some("It resets at 17:00. Send a follow-up then to carry on.")));
    // A follow-up that fails again for another reason replaces it; one that starts a run ends it.
    let mut b = begin(&mut core, &d, &limit);
    assert!(derive(&core.store).unwrap().iter().all(|i| i.task != limit));
    finish(&mut core, &mut b, Outcome::Failed(Failure::Crashed { code: None, words: String::new() }));
    assert!(derive(&core.store).unwrap().iter().all(|i| i.task != limit), "its newest failure is not one to act on");
}

fn item(id: &str, ws: &str, since: i64) -> NeedsItem {
    NeedsItem { id: id.into(), task: TaskId(format!("task_{id}")), workspace: ws.into(), title: id.into(), since_ms: since, need: Need::Question { run: neko_protocol::RunId("r".into()), question: "?".into() } }
}

#[test]
fn the_list_is_oldest_first_grouped_by_workspace() {
    let got = needs::order(vec![item("a", "ws_web", 30), item("b", "default", 20), item("c", "ws_web", 10), item("d", "default", 40), item("e", "ws_api", 25)]);
    let ids: Vec<&str> = got.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["c", "a", "b", "d", "e"], "the workspace waiting longest first, its items in order");
}

#[test]
fn items_in_two_workspaces_are_grouped_and_survive_a_restart() {
    let d = dir();
    let folder = d.0.join("acme-api");
    std::fs::create_dir_all(&folder).unwrap();
    let mut core = open(&d);
    let ws = core.add_workspace(&folder.to_string_lossy()).unwrap().workspace.id;
    let (first, _) = core.create_task("First, in General").unwrap();
    let (second, _) = core.create_task_in("Second, in acme-api", Some(&ws)).unwrap();
    let (third, _) = core.create_task("Third, in General").unwrap();
    let (waiting, _) = core.create_task("Waiting on a request").unwrap();
    for (t, words) in [(&first, "Which one?"), (&second, "Shall I push it?"), (&third, "Keep it?")] {
        let mut b = begin(&mut core, &d, &t.id);
        say(&mut core, &mut b, words);
        finish(&mut core, &mut b, Outcome::Done);
    }
    let mut w = begin(&mut core, &d, &waiting.id);
    ask(&mut core, &mut w, "cli-9");
    let titles = |items: &[NeedsItem]| items.iter().map(|i| i.title.clone()).collect::<Vec<_>>();
    let before = derive(&core.store).unwrap();
    assert_eq!(titles(&before), ["First, in General", "Third, in General", "Waiting on a request", "Second, in acme-api"]);
    assert_eq!(before[3].workspace, ws);
    drop(w);
    drop(core);
    // A restart: the daemon marks the run that was waiting interrupted, and its request expires. The rest is as it was.
    let mut core = open(&d);
    let now = core.now();
    core.store.interrupt_leftovers(now).unwrap();
    let after = derive(&core.store).unwrap();
    assert_eq!(titles(&after), ["First, in General", "Third, in General", "Second, in acme-api"]);
    assert_eq!(after, before.into_iter().filter(|i| i.task != waiting.id).collect::<Vec<_>>(), "the same items, the same order");
    assert_eq!(status(&core, &waiting.id), TaskStatus::Interrupted, "an interrupted task needs nothing: a follow-up carries on");
}
