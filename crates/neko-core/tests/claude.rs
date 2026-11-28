//! Claude Code's events mapped onto Neko's, for every script of the fake CLI: thinking, tools, permissions, a long answer, a crash,
//! the usage limit, signed out, a stall, and detection. The real `claude` is never run.

mod support;
use std::time::Duration;

use neko_core::runtime::{Action, AgentEvent, Answer, Failure, Login, Outcome, Runtime};
use support::*;

const T: Duration = Duration::from_secs(20);

#[test]
fn detection_reads_the_version_and_the_login() {
    let d = claude_on_fake().detect();
    assert!(d.found);
    assert_eq!(d.version.as_deref(), Some("9.9.9"));
    assert_eq!((d.signed_in, d.login), (Some(true), Some(Login::Subscription(Some("max".into())))));
    let missing = neko_core::claude::ClaudeRuntime::new(Some("/nonexistent/claude".into()), fake_cli()).detect();
    assert!(!missing.found);
    let dir = Temp::new("missing");
    assert_eq!(neko_core::claude::ClaudeRuntime::new(Some("/nonexistent/claude".into()), fake_cli()).start(spec("Hi", &dir.0)).err(), Some(Failure::NotInstalled));
}

#[test]
fn thinking_then_words() {
    let dir = Temp::new("thinking");
    let s = claude_on_fake().start(spec("[fake:thinking]", &dir.0)).unwrap();
    let ev = drain(&s, T, |_, _| None);
    let thought: String = ev.iter().filter_map(|e| if let AgentEvent::ThinkingDelta(t) = e { Some(t.as_str()) } else { None }).collect();
    assert!(thought.starts_with("The person wants"), "{thought}");
    let td = ev.iter().position(|e| *e == AgentEvent::ThinkingDone).unwrap();
    let first_text = ev.iter().position(|e| matches!(e, AgentEvent::TextDelta(_))).unwrap();
    assert!(td < first_text, "thinking ends before the answer");
    assert_eq!(text_of(&ev), "Here is a short answer after some thought.", "streamed once, not repeated by the whole message");
    assert!(ev.iter().any(|e| matches!(e, AgentEvent::Usage(u) if u.cost_usd == Some(0.0123) && u.cached_tokens == 5000 && u.model.as_deref() == Some("claude-fake-1"))));
    assert_eq!(finished(&ev), &Outcome::Done);
}

#[test]
fn tools_start_and_finish_and_an_edit_asks() {
    let dir = Temp::new("tools");
    let s = claude_on_fake().start(spec("[fake:tools]", &dir.0)).unwrap();
    let mut asked = Vec::new();
    let ev = drain(&s, T, |_, a| {
        asked.push(a.clone());
        Some(Answer::Allow)
    });
    let started: Vec<(String, String)> = ev.iter().filter_map(|e| if let AgentEvent::ToolStarted { name, summary, .. } = e { Some((name.clone(), summary.clone())) } else { None }).collect();
    assert_eq!(started, vec![("Read".into(), "README.md".into()), ("Edit".into(), "README.md".into()), ("Glob".into(), "src/**/*.rs".into())]);
    assert_eq!(ev.iter().filter(|e| matches!(e, AgentEvent::ToolFinished { ok: true, .. })).count(), 3);
    assert_eq!(asked, vec![Action::Edit { path: format!("{}/README.md", dir.0.display()) }]);
}

#[test]
fn destructive_and_outside_steps_reach_neko() {
    let dir = Temp::new("risky");
    for (script, want) in [("destructive", Action::Command { command: "rm -rf build".into() }), ("outside", Action::Read { path: "/etc/hosts".into() })] {
        let s = claude_on_fake().start(spec(&format!("[fake:{script}]"), &dir.0)).unwrap();
        let mut asked = None;
        let ev = drain(&s, T, |_, a| {
            asked = Some(a.clone());
            Some(Answer::Deny)
        });
        assert_eq!(asked, Some(want));
        assert!(ev.iter().any(|e| matches!(e, AgentEvent::ToolFinished { ok: false, .. })), "denied: the call failed");
    }
}

#[test]
fn a_long_answer_streams_in_many_pieces() {
    let dir = Temp::new("long");
    let s = claude_on_fake().start(spec("[fake:long:2]", &dir.0)).unwrap();
    let ev = drain(&s, T, |_, _| None);
    assert!(ev.iter().filter(|e| matches!(e, AgentEvent::TextDelta(_))).count() > 20);
    assert_eq!(finished(&ev), &Outcome::Done);
}

#[test]
fn failures_say_what_happened() {
    let dir = Temp::new("fail");
    let run = |w: &str| drain(&claude_on_fake().start(spec(w, &dir.0)).unwrap(), T, |_, _| None);
    match finished(&run("[fake:crash]")) {
        Outcome::Failed(Failure::Crashed { code: Some(3), words }) => assert_eq!(words, "fatal: the fake fell over"),
        o => panic!("crash: {o:?}"),
    }
    assert!(matches!(finished(&run("[fake:limit]")), Outcome::Failed(Failure::UsageLimit { resets: Some(r) }) if r.starts_with("at ")));
    assert_eq!(finished(&run("[fake:signin]")), &Outcome::Failed(Failure::NotSignedIn));
}

#[test]
fn a_stalled_cli_can_be_stopped() {
    let dir = Temp::new("stall");
    let s = claude_on_fake().start(spec("[fake:stall]", &dir.0)).unwrap();
    until("it starts", T, || matches!(s.events.recv_timeout(Duration::from_millis(50)), Ok(AgentEvent::Started { .. })));
    // The account's limit news comes with every start; nothing else does.
    let mut news = Vec::new();
    while let Ok(e) = s.events.recv_timeout(Duration::from_millis(300)) {
        news.push(e);
    }
    assert!(news.iter().all(|e| matches!(e, AgentEvent::RateLimit { status, .. } if status == "allowed")), "then nothing but that: {news:?}");
    s.control.stop();
    assert_eq!(finished(&drain(&s, Duration::from_secs(2), |_, _| None)), &Outcome::Stopped);
}

#[test]
fn a_follow_up_while_it_runs_is_answered_in_the_same_run() {
    let dir = Temp::new("steer");
    let s = claude_on_fake().start(spec("[fake:thinking]", &dir.0)).unwrap();
    s.control.steer("And one more thing [fake:plain]").unwrap();
    let ev = drain(&s, T, |_, _| None);
    let text = text_of(&ev);
    assert!(text.contains("after some thought") && text.contains("Hello from the fake"), "both messages answered: {text}");
    assert_eq!(finished(&ev), &Outcome::Done);
}

/// Starts a `work` run on the fake CLI, waits for its first step, then steers it. Returns every event up to the end of the run and
/// how long it took after the steer.
fn steer_a_working_run(script: &str, name: &str) -> (Vec<AgentEvent>, Duration) {
    let dir = Temp::new(name);
    let s = claude_on_fake().start(spec(script, &dir.0)).unwrap();
    let mut seen = Vec::new();
    let end = std::time::Instant::now() + T;
    while !text_of(&seen).contains("Step 1") {
        assert!(std::time::Instant::now() < end, "no first step; it said {seen:?}");
        if let Ok(e) = s.events.recv_timeout(Duration::from_millis(50)) {
            seen.push(e);
        }
    }
    let steered_at = std::time::Instant::now();
    s.control.steer("Also end with the word meow").unwrap();
    // The run must end by itself: a run that waits for a result that never comes is the bug this guards.
    seen.extend(drain(&s, T, |_, _| None));
    (seen, steered_at.elapsed())
}

fn results(ev: &[AgentEvent]) -> usize {
    ev.iter().filter(|e| matches!(e, AgentEvent::Usage(_))).count()
}

#[test]
fn a_steer_the_cli_folds_into_the_running_turn_ends_the_run_with_its_one_result() {
    let (ev, took) = steer_a_working_run("Do the work [fake:work:4:step=400]", "steer-fold");
    assert_eq!(finished(&ev), &Outcome::Done);
    assert_eq!(results(&ev), 1, "the CLI answered both messages with one result: {ev:?}");
    let text = text_of(&ev);
    assert_eq!(text.matches("Reading your message: Also end with the word meow").count(), 1, "the steer was obeyed in the turn, and its echo drew nothing: {text}");
    assert!(took < Duration::from_secs(10), "the run ended on that one result, not after a second that never came ({took:?})");
}

#[test]
fn a_steer_the_cli_answers_in_a_turn_of_its_own_keeps_the_run_open_for_its_result() {
    let (ev, _) = steer_a_working_run("Do the work [fake:work:3:step=400:separate]", "steer-separate");
    assert_eq!(finished(&ev), &Outcome::Done);
    assert_eq!(results(&ev), 2, "the run waited for the second turn's result: {ev:?}");
    let text = text_of(&ev);
    assert!(text.contains("The work is finished."), "{text}");
    assert!(text.contains("Hello from the fake"), "the steer got a turn of its own: {text}");
    assert!(!text.contains("Reading your message"), "it was not read inside the first turn: {text}");
}

#[test]
fn an_interrupted_turn_ends_the_run_whatever_waits_unread() {
    let dir = Temp::new("steer-interrupt");
    let s = claude_on_fake().start(spec("A long job [fake:work:60:separate]", &dir.0)).unwrap();
    until("the first step", T, || s.events.recv_timeout(Duration::from_millis(50)).is_ok_and(|e| matches!(e, AgentEvent::TextDelta(t) if t.contains("Step 1"))));
    s.control.steer("Words that wait").unwrap();
    s.control.interrupt();
    let ev = drain(&s, T, |_, _| None);
    assert_eq!(finished(&ev), &Outcome::Stopped, "an interrupted run is not a failure and ends on its own result: {ev:?}");
}
