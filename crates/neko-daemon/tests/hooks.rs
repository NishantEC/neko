//! The three permission modes in the real daemon, with the fake CLI as `claude` and a HOME of the test's own holding the person's
//! Claude Code settings. In "Ask before risky steps" the fake runs the `PreToolUse` hook Neko passes with `--settings`, exactly as
//! Claude Code's hooks documentation says, and the hook helper (`neko-daemon --hook`) asks the daemon. Every wait has a deadline.

mod support;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use neko_protocol::client::Client;
use neko_protocol::*;
use support::*;

const T: Duration = Duration::from_secs(30);

fn watcher(d: &Daemon) -> Client {
    let mut c = d.client();
    assert!(matches!(c.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
    c
}

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

fn create(c: &mut Client, words: &str) -> Task {
    match c.request(Request::CreateTask { words: words.into(), workspace: None }).unwrap() {
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

fn pending(changes: &[Change]) -> Permission {
    changes.iter().find_map(|c| if let Change::PermissionChanged { permission } = c { (permission.state == PermissionState::Pending).then(|| permission.clone()) } else { None }).expect("a pending request")
}

fn set_mode(c: &mut Client, m: PermissionMode) {
    let s = Settings { runtime: "claude".into(), permission_mode: m };
    assert_eq!(c.request(Request::SetSettings { settings: s.clone() }).unwrap(), Reply::Settings { settings: s });
}

/// The person's own Claude Code settings: Bash, Edit and Write allowed outright, a hook of their own, auto mode.
const PERSON: &str = r#"{"permissions":{"allow":["Bash","Edit","Write"],"defaultMode":"auto"},"hooks":{"Stop":[{"hooks":[{"type":"command","command":"true"}]}]}}"#;

/// A daemon whose HOME holds the person's settings; the fake logs every hook input it sends.
fn daemon(name: &str, env: &[(&str, &str)]) -> (Daemon, std::path::PathBuf) {
    let dir = temp_dir(name);
    let home = home_of(&dir);
    std::fs::write(home.join(".claude/settings.json"), PERSON).unwrap();
    let log = dir.join("hooks.log");
    let mut all: Vec<(&str, &str)> = env.to_vec();
    let log_s = log.to_string_lossy().into_owned();
    all.push(("NEKO_FAKE_HOOK_LOG", &log_s));
    (Daemon::start_with(dir, &all), log)
}

/// Every file under `dir` with its bytes.
fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push((p.to_string_lossy().into_owned(), std::fs::read(&p).unwrap_or_default()));
            }
        }
    }
    out.sort();
    out
}

fn hook_inputs(log: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(log).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

#[test]
fn asking_before_risky_steps_asks_a_command_the_persons_own_rules_allow_and_never_touches_their_settings() {
    let (d, log) = daemon("hook-shell", &[]);
    let home = home_of(&d.dir);
    let before = snapshot(&home);
    let mut c = watcher(&d);
    // Following Claude Code's own rules: the person's settings allow Bash, so nothing is asked, as in the terminal.
    let follow = create(&mut c, "[fake:hook-shell]");
    let changes = events_until(&mut c, "done following the person's rules", task_is(&follow.id, TaskStatus::Done));
    assert!(!changes.iter().any(|ch| matches!(ch, Change::PermissionChanged { .. })), "mode 1 adds nothing");
    assert!(hook_inputs(&log).is_empty(), "no hook in mode 1");
    assert!(agent_words(&transcript(&mut c, &follow.id)).contains("The tests pass."));
    // Asking before risky steps: the same command is asked, whatever the person's settings say.
    set_mode(&mut c, PermissionMode::Ask);
    let task = create(&mut c, "[fake:hook-shell]");
    let p = pending(&events_until(&mut c, "the command to be asked", task_is(&task.id, TaskStatus::NeedsYou)));
    assert_eq!((p.title.as_str(), p.detail.as_str(), p.tool.as_str(), p.destructive), ("Run a command", "cargo test", "Bash", false));
    let inputs = hook_inputs(&log);
    assert_eq!(inputs.len(), 1);
    assert_eq!((inputs[0]["hook_event_name"].as_str(), inputs[0]["tool_name"].as_str(), inputs[0]["tool_input"]["command"].as_str()), (Some("PreToolUse"), Some("Bash"), Some("cargo test")));
    assert_eq!(c.request(Request::AnswerPermission { permission: p.id.clone(), decision: Decision::AllowOnce }).unwrap(), Reply::Ok);
    events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    let t = transcript(&mut c, &task.id);
    assert!(agent_words(&t).contains("The tests pass."), "{}", agent_words(&t));
    assert_eq!(t.permissions[0].state, PermissionState::Allowed);
    // The person's settings files: not one byte changed, nothing added.
    assert_eq!(snapshot(&home), before);
}

#[test]
fn a_read_in_the_folder_is_not_asked() {
    let (d, log) = daemon("hook-read", &[]);
    let mut c = watcher(&d);
    set_mode(&mut c, PermissionMode::Ask);
    let task = create(&mut c, "[fake:hook-read]");
    let changes = events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    assert!(!changes.iter().any(|ch| matches!(ch, Change::PermissionChanged { .. })), "reads go ahead");
    assert_eq!(hook_inputs(&log).iter().map(|v| v["tool_name"].as_str().unwrap_or("").to_owned()).collect::<Vec<_>>(), ["Read"], "the hook was asked, and said yes");
    assert!(agent_words(&transcript(&mut c, &task.id)).contains("I read the README."));
}

#[test]
fn a_denied_step_does_not_run_and_the_agent_hears_why() {
    let (d, _) = daemon("hook-deny", &[]);
    let mut c = watcher(&d);
    set_mode(&mut c, PermissionMode::Ask);
    let task = create(&mut c, "[fake:hook-shell]");
    let p = pending(&events_until(&mut c, "a request", task_is(&task.id, TaskStatus::NeedsYou)));
    assert_eq!(c.request(Request::AnswerPermission { permission: p.id, decision: Decision::Deny }).unwrap(), Reply::Ok);
    events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    let t = transcript(&mut c, &task.id);
    assert!(agent_words(&t).contains("I did not run the tests. Claude Code said: The person said no to this step in Neko."), "{}", agent_words(&t));
    assert_eq!(t.permissions[0].state, PermissionState::Denied);
    let states: Vec<ToolState> = t.parts.iter().filter_map(|p| if let PartBody::Tool { state, .. } = &p.body { Some(*state) } else { None }).collect();
    assert_eq!(states, [ToolState::Denied], "the person's choice, not a failure");
}

#[test]
fn always_allow_is_scoped_to_the_task_and_the_kind_of_step() {
    let (d, log) = daemon("hook-always", &[]);
    let mut c = watcher(&d);
    set_mode(&mut c, PermissionMode::Ask);
    let task = create(&mut c, "[fake:hook-twice]");
    let p = pending(&events_until(&mut c, "the first command", task_is(&task.id, TaskStatus::NeedsYou)));
    assert_eq!(p.detail, "cargo test");
    assert_eq!(c.request(Request::AnswerPermission { permission: p.id, decision: Decision::AlwaysAllow }).unwrap(), Reply::Ok);
    events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    let t = transcript(&mut c, &task.id);
    assert_eq!(t.permissions.len(), 1, "`cargo build` goes ahead after always allowing `cargo`");
    assert_eq!(hook_inputs(&log).len(), 2, "both calls went through the hook");
    // Another task asks again.
    let other = create(&mut c, "[fake:hook-shell]");
    let p = pending(&events_until(&mut c, "asked in another task", task_is(&other.id, TaskStatus::NeedsYou)));
    c.request(Request::AnswerPermission { permission: p.id, decision: Decision::AllowOnce }).unwrap();
    events_until(&mut c, "done", task_is(&other.id, TaskStatus::Done));
}

#[test]
fn a_destructive_command_has_no_always_through_the_hook() {
    let (d, _) = daemon("hook-destructive", &[]);
    let mut c = watcher(&d);
    set_mode(&mut c, PermissionMode::Ask);
    let task = create(&mut c, "[fake:hook-destructive]");
    let p = pending(&events_until(&mut c, "a destructive request", task_is(&task.id, TaskStatus::NeedsYou)));
    assert!(p.destructive);
    assert_eq!((p.title.as_str(), p.detail.as_str()), ("Run a command that deletes or overwrites", "git reset --hard HEAD~1"));
    assert_eq!(c.request(Request::AnswerPermission { permission: p.id, decision: Decision::AlwaysAllow }).unwrap(), Reply::Ok);
    events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    let t = transcript(&mut c, &task.id);
    assert_eq!(t.permissions[0].state, PermissionState::Allowed, "always counts once");
    assert!(agent_words(&t).contains("I reset the branch."));
}

#[test]
fn an_unanswered_step_expires_and_is_denied() {
    let (d, _) = daemon("hook-expiry", &[("NEKO_HOOK_EXPIRE_MS", "1500")]);
    let mut c = watcher(&d);
    set_mode(&mut c, PermissionMode::Ask);
    let task = create(&mut c, "[fake:hook-shell]");
    let changes = events_until(&mut c, "done after the request expired", task_is(&task.id, TaskStatus::Done));
    let p = pending(&changes);
    assert!(changes.iter().any(|ch| matches!(ch, Change::PermissionChanged { permission } if permission.id == p.id && permission.state == PermissionState::Expired)));
    let t = transcript(&mut c, &task.id);
    assert!(agent_words(&t).contains("No one answered in Neko in time"), "{}", agent_words(&t));
    let late = c.request(Request::AnswerPermission { permission: p.id, decision: Decision::AllowOnce }).unwrap();
    assert!(matches!(late, Reply::Error { code: ErrorCode::AlreadyAnswered, .. }), "{late:?}");
}

#[test]
fn the_run_ending_while_a_step_waits_expires_it() {
    let (d, _) = daemon("hook-ended", &[]);
    let mut c = watcher(&d);
    set_mode(&mut c, PermissionMode::Ask);
    let task = create(&mut c, "[fake:hook-shell]");
    let p = pending(&events_until(&mut c, "a request", task_is(&task.id, TaskStatus::NeedsYou)));
    c.request(Request::StopTask { task: task.id.clone() }).unwrap();
    let changes = events_until(&mut c, "stopped", task_is(&task.id, TaskStatus::Stopped));
    assert!(changes.iter().any(|ch| matches!(ch, Change::PermissionChanged { permission } if permission.id == p.id && permission.state == PermissionState::Expired)));
    let late = c.request(Request::AnswerPermission { permission: p.id, decision: Decision::AllowOnce }).unwrap();
    assert!(matches!(late, Reply::Error { code: ErrorCode::AlreadyAnswered, .. }), "{late:?}");
    // The task takes a follow-up at once: nothing of the old run is left waiting.
    assert_eq!(c.request(Request::FollowUp { task: task.id.clone(), words: "[fake:hook-read]".into(), answering: None }).unwrap(), Reply::Ok);
    events_until(&mut c, "the follow-up done", task_is(&task.id, TaskStatus::Done));
}

/// Runs the hook helper as Claude Code would, with this secret.
fn helper(port: u16, secret: &str) -> (Option<i32>, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_neko-daemon"))
        .arg("--hook")
        .env("NEKO_HOOK_PORT", port.to_string())
        .env("NEKO_HOOK_SECRET", secret)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(br#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"rm -rf /tmp/x"},"tool_use_id":"toolu_x","cwd":"/tmp"}"#).unwrap();
    let out = child.wait_with_output().unwrap();
    (out.status.code(), String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

#[test]
fn the_helper_refuses_a_wrong_secret() {
    let (d, _) = daemon("hook-secret", &[]);
    let mut c = watcher(&d);
    set_mode(&mut c, PermissionMode::Ask);
    // A run is live with a secret of its own.
    let task = create(&mut c, "[fake:hook-shell]");
    let p = pending(&events_until(&mut c, "a request", task_is(&task.id, TaskStatus::NeedsYou)));
    for wrong in ["wrong", ""] {
        let (code, out) = helper(d.port(), wrong);
        assert_eq!(code, Some(0), "exit 0 so Claude Code reads the decision");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap_or_else(|_| panic!("JSON: {out}"));
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny", "{out}");
    }
    // The install token is not a run's secret either.
    let (_, out) = helper(d.port(), &d.token());
    assert!(out.contains("\"deny\""), "{out}");
    assert_eq!(transcript(&mut c, &task.id).permissions.len(), 1, "a wrong secret asks nobody");
    c.request(Request::AnswerPermission { permission: p.id, decision: Decision::Deny }).unwrap();
    events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
}

#[test]
fn a_real_runs_shapes_are_stored_calmly() {
    let (d, _) = daemon("real-shapes", &[]);
    let mut c = watcher(&d);
    let task = create(&mut c, "[fake:real]");
    events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    let t = transcript(&mut c, &task.id);
    assert!(!t.parts.iter().any(|p| matches!(p.body, PartBody::Thinking { .. })), "empty thoughts store nothing");
    let tools: Vec<(&str, ToolState, Option<i32>)> = t.parts.iter().filter_map(|p| if let PartBody::Tool { name, state, exit, .. } = &p.body { Some((name.as_str(), *state, *exit)) } else { None }).collect();
    assert_eq!(tools, [("Read", ToolState::Done, None), ("Bash", ToolState::Done, None), ("Bash", ToolState::Done, Some(1))], "a check that exits 1 finished");
    assert_eq!(t.parts.iter().filter(|p| matches!(p.body, PartBody::Agent { .. })).count(), 3, "words between the tools, and the answer");
    let u = t.runs[0].usage.as_ref().unwrap();
    assert_eq!((u.context_window, u.context_tokens.is_some()), (Some(200_000), true));
}

#[test]
fn settings_say_in_one_line_what_the_persons_rules_allow() {
    let (d, _) = daemon("hook-rules", &[]);
    let mut c = watcher(&d);
    let Reply::Runtimes { runtimes, .. } = c.request(Request::ListRuntimes).unwrap() else { panic!("runtimes") };
    assert_eq!(runtimes[0].rules.as_deref(), Some("Your Claude Code settings allow Bash, Edit and Write without asking. Its default mode is auto: Claude Code decides the rest by itself."));
}

fn notices(t: &Transcript) -> Vec<(String, Option<String>)> {
    t.parts.iter().filter_map(|p| if let PartBody::Notice { text, next, .. } = &p.body { Some((text.clone(), next.clone())) } else { None }).collect()
}

#[test]
fn a_step_claude_code_ran_unchecked_stops_the_run_when_asking_before_risky_steps() {
    let (d, _) = daemon("hook-no-verdict", &[]);
    let mut c = watcher(&d);
    // Following Claude Code's own rules, the person's own hooks are theirs: the run goes on.
    let follow = create(&mut c, "[fake:hook-no-verdict]");
    events_until(&mut c, "done", task_is(&follow.id, TaskStatus::Done));
    set_mode(&mut c, PermissionMode::Ask);
    let task = create(&mut c, "[fake:hook-no-verdict]");
    events_until(&mut c, "the run to stop", task_is(&task.id, TaskStatus::Failed));
    let n = notices(&transcript(&mut c, &task.id));
    assert_eq!(n.len(), 1, "{n:?}");
    assert_eq!(n[0].0, "Neko could not check a step, so it stopped the run.");
    assert!(n[0].1.as_deref().is_some_and(|x| x.contains("ran unchecked: Bash cargo test")), "the step is named: {n:?}");
}

#[test]
fn without_its_helper_a_run_that_asks_before_risky_steps_does_not_start() {
    let (d, log) = daemon("hook-missing", &[("NEKO_HOOK_HELPER", "/nonexistent/neko-daemon")]);
    let mut c = watcher(&d);
    set_mode(&mut c, PermissionMode::Ask);
    let task = create(&mut c, "[fake:hook-shell]");
    events_until(&mut c, "refused", task_is(&task.id, TaskStatus::Failed));
    let t = transcript(&mut c, &task.id);
    assert!(notices(&t)[0].0.contains("permission check is missing"), "{:?}", notices(&t));
    assert!(agent_words(&t).is_empty() && hook_inputs(&log).is_empty(), "nothing ran");
}

#[test]
fn the_helper_says_no_when_it_cannot_reach_neko_or_runs_outside_a_run() {
    // A port nothing listens on.
    let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    for (port, secret) in [(free, "s3cret"), (0, "")] {
        let (code, out) = helper(port, secret);
        assert_eq!(code, Some(0));
        let v: serde_json::Value = serde_json::from_str(&out).unwrap_or_else(|_| panic!("a verdict on every path: {out}"));
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny", "{out}");
    }
}

#[test]
fn screens_hear_when_the_agent_thinks() {
    let (d, _) = daemon("thinking", &[]);
    let mut c = watcher(&d);
    let task = create(&mut c, "[fake:real]");
    assert_eq!(c.request(Request::WatchTask { task: Some(task.id.clone()) }).unwrap(), Reply::Ok);
    let changes = events_until(&mut c, "done", task_is(&task.id, TaskStatus::Done));
    let thinking: Vec<bool> = changes.iter().filter_map(|ch| if let Change::Activity { thinking, .. } = ch { Some(*thinking) } else { None }).collect();
    assert!(thinking.contains(&true) && thinking.last() == Some(&false), "{thinking:?}");
}
