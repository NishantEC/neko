//! Tasks running in the real daemon, with the fake CLI as `claude`: streaming, the stored transcript, permissions in every state,
//! Stop, follow-ups that resume, worktrees that leave the person's checkout alone, failures that read well, the run guard when the
//! daemon dies, an interrupted run after a restart, Settings, and the day's usage. Every wait has a deadline.

mod support;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use neko_protocol::client::Client;
use neko_protocol::*;
use support::*;

const T: Duration = Duration::from_secs(30);

/// A subscribed client watching `task`'s stream (or none yet).
fn watcher(d: &Daemon) -> Client {
    let mut c = d.client();
    assert!(matches!(c.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
    c
}

fn watch(c: &mut Client, task: &TaskId) {
    assert_eq!(c.request(Request::WatchTask { task: Some(task.clone()) }).unwrap(), Reply::Ok);
}

/// Events until `done` says so (the deadline fails the test).
fn events_until(c: &mut Client, what: &str, mut done: impl FnMut(&Change) -> bool) -> Vec<Change> {
    let end = Instant::now() + T;
    let mut got = Vec::new();
    loop {
        assert!(Instant::now() < end, "timed out waiting for {what}; got {got:#?}");
        if let Some(ServerMessage::Event { change, .. }) = c.next_event(Duration::from_millis(200)).unwrap() {
            let stop = done(&change);
            got.push(change);
            if stop {
                return got;
            }
        }
    }
}

fn task_is(id: &TaskId, s: TaskStatus) -> impl FnMut(&Change) -> bool {
    let id = id.clone();
    move |c| matches!(c, Change::TaskUpdated { task } if task.id == id && task.status == s)
}

/// A task in `folder`'s workspace (added first; None: General).
fn create(c: &mut Client, words: &str, folder: Option<&Path>) -> Task {
    let workspace = folder.map(|f| match c.request(Request::AddWorkspace { folder: f.to_string_lossy().into_owned() }).unwrap() {
        Reply::Workspace { workspace, .. } => workspace.id,
        r => panic!("add workspace: {r:?}"),
    });
    match c.request(Request::CreateTask { words: words.into(), workspace, cat: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("create: {r:?}"),
    }
}

fn transcript(c: &mut Client, task: &TaskId) -> Transcript {
    match c.request(Request::GetTranscript { task: task.clone() }).unwrap() {
        Reply::Transcript { transcript } => transcript,
        r => panic!("transcript: {r:?}"),
    }
}

fn agent_words(t: &Transcript) -> String {
    t.parts.iter().filter_map(|p| if let PartBody::Agent { text, .. } = &p.body { Some(text.as_str()) } else { None }).collect::<Vec<_>>().join("\n")
}

fn notices(t: &Transcript) -> Vec<(String, Option<String>)> {
    t.parts.iter().filter_map(|p| if let PartBody::Notice { text, next, .. } = &p.body { Some((text.clone(), next.clone())) } else { None }).collect()
}

fn pending(changes: &[Change]) -> Permission {
    changes.iter().find_map(|c| if let Change::PermissionChanged { permission } = c { (permission.state == PermissionState::Pending).then(|| permission.clone()) } else { None }).expect("a pending request")
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let end = Instant::now() + T;
    while !ok() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_task_runs_streams_and_its_transcript_is_stored() {
    let d = Daemon::start("stream");
    let mut c = watcher(&d);
    let task = create(&mut c, "Think first [fake:thinking]", None);
    watch(&mut c, &task.id);
    let changes = events_until(&mut c, "the task to be done", task_is(&task.id, TaskStatus::Done));
    assert!(changes.iter().any(|c| matches!(c, Change::TaskUpdated { task: t } if t.status == TaskStatus::Working)), "it was working first");
    let deltas: String = changes.iter().filter_map(|c| if let Change::PartDelta { text, .. } = c { Some(text.as_str()) } else { None }).collect();
    assert!(deltas.contains("short answer"), "words streamed to the watcher: {deltas}");
    let t = transcript(&mut c, &task.id);
    assert_eq!(agent_words(&t), "Here is a short answer after some thought.");
    assert!(t.parts.iter().any(|p| matches!(&p.body, PartBody::Thinking { text, done: true } if text.starts_with("The person wants"))));
    let run = &t.runs[0];
    assert_eq!(run.status, RunStatus::Done);
    assert!(run.session.is_some() && run.ended_ms.is_some());
    let u = run.usage.as_ref().expect("usage as reported");
    assert_eq!((u.input_tokens, u.output_tokens, u.cached_tokens, u.cost_usd), (1200, 340, 5000, Some(0.0123)));
    assert_eq!((u.pricing, u.reported_by.as_str()), (Pricing::ApiEquivalent, "Claude Code"), "the fake says it is a subscription");
    assert!(run.workdir.contains("/scratch/"), "no folder: a scratch folder of its own: {}", run.workdir);
    assert!(!Path::new(&run.workdir).exists(), "nothing was written there, so the scratch folder was tidied away after the run");
    let Reply::DayUsage { usage } = c.request(Request::DayUsage { from_ms: 0 }).unwrap() else { panic!("day usage") };
    assert_eq!((usage.runs, usage.cost_usd, usage.api_equivalent), (1, Some(0.0123), true));
}

#[test]
fn permissions_are_asked_in_the_task_and_answered_once() {
    let d = Daemon::start("perms");
    let mut c = watcher(&d);
    let task = create(&mut c, "Run the tests [fake:permission]", None);
    let changes = events_until(&mut c, "the task to need me", task_is(&task.id, TaskStatus::NeedsYou));
    let p = pending(&changes);
    assert_eq!((p.title.as_str(), p.detail.as_str(), p.destructive), ("Run a command", "cargo test", false));
    assert!(changes.iter().any(|ch| matches!(ch, Change::PartChanged { part } if matches!(&part.body, PartBody::Permission { permission } if *permission == p.id))), "the request is a part of the transcript");
    assert_eq!(c.request(Request::AnswerPermission { permission: p.id.clone(), decision: Decision::AllowOnce }).unwrap(), Reply::Ok);
    let again = c.request(Request::AnswerPermission { permission: p.id.clone(), decision: Decision::Deny }).unwrap();
    assert!(matches!(again, Reply::Error { code: ErrorCode::AlreadyAnswered, .. }), "answering twice is refused: {again:?}");
    events_until(&mut c, "the task to be done", task_is(&task.id, TaskStatus::Done));
    let t = transcript(&mut c, &task.id);
    assert!(agent_words(&t).contains("The tests pass."));
    assert_eq!(t.permissions[0].state, PermissionState::Allowed);
}

#[test]
fn always_allow_lasts_for_the_task_only_and_destructive_steps_are_never_always() {
    let d = Daemon::start("always");
    let mut c = watcher(&d);
    let task = create(&mut c, "[fake:permission]", None);
    let p = pending(&events_until(&mut c, "a request", task_is(&task.id, TaskStatus::NeedsYou)));
    assert_eq!(c.request(Request::AnswerPermission { permission: p.id, decision: Decision::AlwaysAllow }).unwrap(), Reply::Ok);
    events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    // The same kind of step in the same task goes ahead.
    assert_eq!(c.request(Request::FollowUp { task: task.id.clone(), words: "Again [fake:permission]".into(), answering: None }).unwrap(), Reply::Ok);
    let changes = events_until(&mut c, "done again", task_is(&task.id, TaskStatus::Done));
    assert!(!changes.iter().any(|ch| matches!(ch, Change::PermissionChanged { .. })), "not asked again in this task");
    // Another task asks.
    let other = create(&mut c, "[fake:permission]", None);
    let p = pending(&events_until(&mut c, "a request in the other task", task_is(&other.id, TaskStatus::NeedsYou)));
    assert_eq!(c.request(Request::AnswerPermission { permission: p.id, decision: Decision::Deny }).unwrap(), Reply::Ok);
    events_until(&mut c, "done", task_is(&other.id, TaskStatus::Done));
    assert!(agent_words(&transcript(&mut c, &other.id)).contains("did not run"), "denied");
    // Destructive: "always" counts once.
    let rm = create(&mut c, "[fake:destructive]", None);
    let p = pending(&events_until(&mut c, "a destructive request", task_is(&rm.id, TaskStatus::NeedsYou)));
    assert!(p.destructive);
    assert_eq!(p.title, "Run a command that deletes or overwrites");
    assert_eq!(c.request(Request::AnswerPermission { permission: p.id.clone(), decision: Decision::AlwaysAllow }).unwrap(), Reply::Ok);
    events_until(&mut c, "done", task_is(&rm.id, TaskStatus::Done));
    assert_eq!(transcript(&mut c, &rm.id).permissions[0].state, PermissionState::Allowed);
    c.request(Request::FollowUp { task: rm.id.clone(), words: "[fake:destructive]".into(), answering: None }).unwrap();
    pending(&events_until(&mut c, "asked again", task_is(&rm.id, TaskStatus::NeedsYou)));
    c.request(Request::StopTask { task: rm.id.clone() }).unwrap();
    events_until(&mut c, "stopped", task_is(&rm.id, TaskStatus::Stopped));
}

#[test]
fn stop_is_quick_and_a_waiting_request_expires() {
    let d = Daemon::start("stop");
    let mut c = watcher(&d);
    let slow = create(&mut c, "[fake:slow]", None);
    watch(&mut c, &slow.id);
    events_until(&mut c, "words", |ch| matches!(ch, Change::PartDelta { .. }));
    let t0 = Instant::now();
    assert_eq!(c.request(Request::StopTask { task: slow.id.clone() }).unwrap(), Reply::Ok);
    events_until(&mut c, "stopped", task_is(&slow.id, TaskStatus::Stopped));
    assert!(t0.elapsed() < Duration::from_secs(1), "Stop took {:?}", t0.elapsed());
    let run = &transcript(&mut c, &slow.id).runs[0];
    assert_eq!(run.status, RunStatus::Stopped);
    assert!(agent_words(&transcript(&mut c, &slow.id)).starts_with("slow0"), "what was said is kept");
    // A request still waiting when the run ends expires, and can no longer be answered.
    let ask = create(&mut c, "[fake:permission]", None);
    let p = pending(&events_until(&mut c, "a request", task_is(&ask.id, TaskStatus::NeedsYou)));
    c.request(Request::StopTask { task: ask.id.clone() }).unwrap();
    let changes = events_until(&mut c, "stopped", task_is(&ask.id, TaskStatus::Stopped));
    assert!(changes.iter().any(|ch| matches!(ch, Change::PermissionChanged { permission } if permission.id == p.id && permission.state == PermissionState::Expired)));
    let late = c.request(Request::AnswerPermission { permission: p.id, decision: Decision::AllowOnce }).unwrap();
    assert!(matches!(late, Reply::Error { code: ErrorCode::AlreadyAnswered, .. }), "{late:?}");
}

#[test]
fn a_follow_up_continues_the_same_conversation() {
    let d = Daemon::start("follow");
    let mut c = watcher(&d);
    let task = create(&mut c, "Hello [fake:plain]", None);
    events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    let first = transcript(&mut c, &task.id).runs[0].session.clone().unwrap();
    assert_eq!(c.request(Request::FollowUp { task: task.id.clone(), words: "Tell me what I said.".into(), answering: None }).unwrap(), Reply::Ok);
    events_until(&mut c, "done again", task_is(&task.id, TaskStatus::Done));
    let t = transcript(&mut c, &task.id);
    assert_eq!(t.runs.len(), 2);
    assert_eq!(t.runs[1].session.as_deref(), Some(first.as_str()), "the CLI's session was resumed");
    assert!(agent_words(&t).contains(&format!("I remember our conversation {first}")));
    assert!(t.parts.iter().any(|p| matches!(&p.body, PartBody::Person { text } if text == "Tell me what I said.")), "the follow-up is in the transcript");
    assert_eq!(t.runs[1].workdir, t.runs[0].workdir, "in the same folder");
}

#[test]
fn two_tasks_run_at_once_and_one_task_runs_once() {
    let d = Daemon::start("two");
    let mut c = watcher(&d);
    let a = create(&mut c, "[fake:slow]", None);
    let b = create(&mut c, "[fake:slow]", None);
    let mut working = std::collections::HashSet::new();
    events_until(&mut c, "both working", |ch| {
        if let Change::RunChanged { run } = ch
            && run.status == RunStatus::Running
        {
            working.insert(run.task.clone());
        }
        working.len() == 2
    });
    for t in [&a, &b] {
        c.request(Request::StopTask { task: t.id.clone() }).unwrap();
    }
    events_until(&mut c, "a stopped", task_is(&a.id, TaskStatus::Stopped));
}

/// Settings: "Ask before risky steps".
fn ask_before_risky_steps(c: &mut Client) {
    let s = Settings { runtime: "claude".into(), permission_mode: PermissionMode::Ask, own_mcp: false };
    assert_eq!(c.request(Request::SetSettings { settings: s.clone() }).unwrap(), Reply::Settings { settings: s });
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).env("GIT_TERMINAL_PROMPT", "0").output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A repository with one commit, on `main`.
fn repo(dir: &Path) -> PathBuf {
    let r = dir.join("acme-api");
    std::fs::create_dir_all(&r).unwrap();
    git(&r, &["init", "-q", "-b", "main"]);
    std::fs::write(r.join("README.md"), "# Acme\nA small project.\n").unwrap();
    git(&r, &["add", "."]);
    git(&r, &["-c", "user.name=Asha", "-c", "user.email=asha@example.com", "commit", "-q", "-m", "first"]);
    r.canonicalize().unwrap()
}

#[test]
fn a_repository_gets_its_own_worktree_and_the_checkout_is_never_touched() {
    let outside = temp_dir("worktree-repo");
    let r = repo(&outside);
    let head = git(&r, &["rev-parse", "HEAD"]);
    let d = Daemon::start("worktree");
    let mut c = watcher(&d);
    ask_before_risky_steps(&mut c);
    let task = create(&mut c, "Tidy the README [fake:tools]", Some(&r));
    assert_eq!(task.folder.as_deref(), Some(r.to_string_lossy().as_ref()));
    events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    let t = transcript(&mut c, &task.id);
    let run = &t.runs[0];
    let short = &task.id.0[task.id.0.len() - 8..].to_lowercase();
    assert_eq!(run.branch.as_deref(), Some(format!("neko/{short}").as_str()));
    assert!(run.workdir.contains("/worktrees/"), "{}", run.workdir);
    assert!(t.permissions.is_empty(), "edits in its own worktree go ahead without asking");
    // README: one line changed (+1 −1); NOTES.md: two new lines.
    assert_eq!(run.changes, Some(Changes { files: 2, added: 3, removed: 1 }));
    // The person's checkout: same branch, same commit, nothing changed, nothing new.
    assert_eq!(git(&r, &["rev-parse", "--abbrev-ref", "HEAD"]), "main");
    assert_eq!(git(&r, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&r, &["status", "--porcelain"]), "");
    assert_eq!(std::fs::read_to_string(r.join("README.md")).unwrap(), "# Acme\nA small project.\n");
    assert!(git(&r, &["branch", "--list", &format!("neko/{short}")]).contains(short.as_str()), "the branch is there, not merged or pushed");
    assert_eq!(git(&r, &["rev-parse", &format!("neko/{short}")]), head, "nothing was committed for the person");
    drop(d);
    let _ = git(&r, &["worktree", "prune"]);
    let _ = std::fs::remove_dir_all(&outside);
}

#[test]
fn a_plain_folder_is_worked_in_directly_and_its_first_edit_asks() {
    let outside = temp_dir("plain-folder");
    let folder = outside.join("notes");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("README.md"), "A small note.\n").unwrap();
    let folder = folder.canonicalize().unwrap();
    let d = Daemon::start("plain");
    let mut c = watcher(&d);
    ask_before_risky_steps(&mut c);
    let task = create(&mut c, "[fake:tools]", Some(&folder));
    let p = pending(&events_until(&mut c, "the first edit to ask", task_is(&task.id, TaskStatus::NeedsYou)));
    assert_eq!(p.title, "Edit files in this folder");
    c.request(Request::AnswerPermission { permission: p.id, decision: Decision::AllowOnce }).unwrap();
    events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    let run = &transcript(&mut c, &task.id).runs[0];
    assert_eq!(run.workdir, folder.to_string_lossy());
    assert_eq!((run.branch.as_ref(), run.changes), (None, None));
    assert_eq!(std::fs::read_to_string(folder.join("README.md")).unwrap(), "A tiny note.\n", "it worked there directly");
    drop(d);
    let _ = std::fs::remove_dir_all(&outside);
}

#[test]
fn failures_are_calm_notices_with_what_to_do_next() {
    let d = Daemon::start("failures");
    let mut c = watcher(&d);
    let check = |c: &mut Client, words: &str, folder: Option<&Path>, says: &str, next: &str| {
        let task = create(c, words, folder);
        events_until(c, says, task_is(&task.id, TaskStatus::Failed));
        let t = transcript(c, &task.id);
        let n = notices(&t);
        assert_eq!(n.len(), 1, "{n:?}");
        assert!(n[0].0.contains(says), "{:?} should say {says:?}", n[0]);
        assert!(n[0].1.as_deref().is_some_and(|x| x.contains(next)), "{:?} should say what to do: {next:?}", n[0]);
        assert!(!n[0].0.contains('{'), "no JSON in a notice: {:?}", n[0]);
    };
    check(&mut c, "[fake:crash]", None, "stopped unexpectedly (exit code 3). It said: fatal: the fake fell over", "follow-up");
    check(&mut c, "[fake:limit]", None, "usage limit", "It resets at ");
    check(&mut c, "[fake:signin]", None, "is not signed in", "claude auth login");
    // A plain folder that went away after the task was made: the follow-up says so.
    let outside = temp_dir("gone-folder");
    let notes = outside.join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    let task = create(&mut c, "Hi [fake:plain]", Some(&notes));
    events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    std::fs::remove_dir_all(&outside).unwrap();
    assert_eq!(c.request(Request::FollowUp { task: task.id.clone(), words: "Again".into(), answering: None }).unwrap(), Reply::Ok);
    events_until(&mut c, "failed", task_is(&task.id, TaskStatus::Failed));
    let n = notices(&transcript(&mut c, &task.id));
    assert!(n.last().is_some_and(|n| n.0.contains("is gone") && n.1.as_deref().is_some_and(|x| x.contains("another folder"))), "{n:?}");
}

#[test]
fn not_installed_and_signed_out_are_said_before_anything_runs() {
    let d = Daemon::start_with(temp_dir("missing"), &[("NEKO_CLAUDE_BIN", "/nonexistent/claude")]);
    let mut c = watcher(&d);
    let task = create(&mut c, "Hi", None);
    events_until(&mut c, "failed", task_is(&task.id, TaskStatus::Failed));
    let n = notices(&transcript(&mut c, &task.id));
    assert!(n[0].0.contains("not installed") && n[0].1.as_deref().unwrap().contains("install.sh"), "{n:?}");
    let Reply::Runtimes { runtimes, .. } = c.request(Request::ListRuntimes).unwrap() else { panic!("runtimes") };
    assert!(!runtimes[0].found);
    drop(c);
    drop(d);
    let d = Daemon::start_with(temp_dir("signed-out"), &[("NEKO_FAKE_AUTH", "out")]);
    let mut c = watcher(&d);
    let task = create(&mut c, "Hi", None);
    events_until(&mut c, "failed", task_is(&task.id, TaskStatus::Failed));
    let n = notices(&transcript(&mut c, &task.id));
    assert!(n[0].0.contains("not signed in"), "{n:?}");
}

#[test]
fn a_silent_run_and_a_long_run_end_as_failed_with_the_reason() {
    let d = Daemon::start_with(temp_dir("limits"), &[("NEKO_RUN_STALL_MS", "1500"), ("NEKO_RUN_LIMIT_MS", "4000")]);
    let mut c = watcher(&d);
    let stalled = create(&mut c, "[fake:stall]", None);
    events_until(&mut c, "the stall check", task_is(&stalled.id, TaskStatus::Failed));
    assert!(notices(&transcript(&mut c, &stalled.id))[0].0.contains("said nothing for 5 minutes"));
    let long = create(&mut c, "[fake:slow]", None);
    events_until(&mut c, "the wall clock", task_is(&long.id, TaskStatus::Failed));
    assert!(notices(&transcript(&mut c, &long.id))[0].0.contains("30 minutes"));
}

#[test]
fn settings_list_the_agents_and_full_access_asks_nothing() {
    let d = Daemon::start("settings");
    let mut c = watcher(&d);
    let Reply::Runtimes { runtimes, settings } = c.request(Request::ListRuntimes).unwrap() else { panic!("runtimes") };
    assert_eq!(settings, Settings { runtime: "claude".into(), permission_mode: PermissionMode::Follow, own_mcp: false }, "a new install follows Claude Code's own rules");
    let claude = &runtimes[0];
    assert_eq!((claude.id.as_str(), claude.found, claude.version.as_deref(), claude.signed_in), ("claude", true, Some("9.9.9"), Some(true)));
    assert_eq!(claude.login.as_deref(), Some("Claude subscription (Max)"));
    assert!(runtimes.iter().any(|r| r.id == "codex" && !r.supported && r.install.contains("codex")));
    let full = Settings { runtime: "claude".into(), permission_mode: PermissionMode::Full, own_mcp: false };
    assert_eq!(c.request(Request::SetSettings { settings: full.clone() }).unwrap(), Reply::Settings { settings: full });
    let refused = c.request(Request::SetSettings { settings: Settings { runtime: "codex".into(), permission_mode: PermissionMode::Ask, own_mcp: false } }).unwrap();
    assert!(matches!(refused, Reply::Error { code: ErrorCode::BadRequest, .. }));
    let task = create(&mut c, "[fake:destructive]", None);
    let changes = events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    assert!(!changes.iter().any(|ch| matches!(ch, Change::PermissionChanged { .. })), "full access asks nothing");
}

#[test]
fn when_the_daemon_dies_the_run_dies_with_it_and_a_restart_marks_it_interrupted() {
    let dir = temp_dir("dies");
    let pidfile = dir.join("grandchild.pid");
    let mut d = Daemon::start_in(dir.clone());
    d.keep_dir = true;
    let mut c = watcher(&d);
    let task = create(&mut c, &format!("Keep going [fake:spawn-stay:{}]", pidfile.display()), None);
    watch(&mut c, &task.id);
    until("the grandchild's pid", || std::fs::read_to_string(&pidfile).is_ok_and(|s| s.trim().parse::<i32>().is_ok()));
    let grandchild: i32 = std::fs::read_to_string(&pidfile).unwrap().trim().parse().unwrap();
    // Words after the spawn: the guard has looked at the tree by then.
    events_until(&mut c, "words after the spawn", |ch| matches!(ch, Change::PartDelta { text, .. } if text.contains("still here 1")));
    // Flushes happen every few seconds; wait for one so there is something on disk to keep.
    std::thread::sleep(Duration::from_millis(3200));
    events_until(&mut c, "more words", |ch| matches!(ch, Change::PartDelta { .. }));
    assert!(alive(grandchild));
    d.signal(libc::SIGKILL);
    until("the grandchild to die with the daemon", || !alive(grandchild));
    let _ = d.child.wait();
    drop(c);
    drop(d);
    let d = Daemon::start_in(dir.clone());
    let mut c = watcher(&d);
    let t = transcript(&mut c, &task.id);
    assert_eq!(t.task.status, TaskStatus::Interrupted);
    assert_eq!(t.runs[0].status, RunStatus::Interrupted);
    assert!(agent_words(&t).contains("still here"), "what was written is kept: {:?}", agent_words(&t));
    assert!(notices(&t).iter().any(|(text, next)| text.contains("Neko stopped unexpectedly while this was running") && text.contains("may be missing") && next.as_deref() == Some("Send a follow-up to carry on.")));
    let session = t.runs[0].session.clone().unwrap();
    c.request(Request::FollowUp { task: task.id.clone(), words: "Carry on".into(), answering: None }).unwrap();
    events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    let t = transcript(&mut c, &task.id);
    assert_eq!(t.runs[1].session.as_deref(), Some(session.as_str()), "the follow-up resumed the CLI's session");
    drop(c);
    drop(d);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_run_that_ends_with_a_question_leaves_the_task_with_the_person() {
    let d = Daemon::start("asked");
    let mut c = watcher(&d);
    let task = create(&mut c, "Hello [fake:plain]", None);
    events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    // The fake repeats the follow-up's words last, so its answer ends with this question.
    assert_eq!(c.request(Request::FollowUp { task: task.id.clone(), words: "Which branch should it go on?".into(), answering: None }).unwrap(), Reply::Ok);
    events_until(&mut c, "the person's turn", task_is(&task.id, TaskStatus::Asked));
    assert!(!TaskStatus::Asked.is_active(), "not working: a follow-up answers it");
    assert_eq!(c.request(Request::FollowUp { task: task.id.clone(), words: "On main.".into(), answering: None }).unwrap(), Reply::Ok);
    events_until(&mut c, "done after the answer", task_is(&task.id, TaskStatus::Done));
}
