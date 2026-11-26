//! A run's book with requests from the hook ("Ask before risky steps") and from Claude Code's own control channel.

use neko_core::conductor::{HOOK, Out, RUN_ENDED, RunBook, Who};
use neko_core::runtime::{Action, AgentEvent, Answer, Outcome};
use neko_core::{Core, FixedClock, Place, SequentialIds};
use neko_protocol::{Decision, PartBody, PermissionMode, PermissionState, Pricing, ToolState};

struct Gone(std::path::PathBuf);
impl Drop for Gone {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn book(mode: PermissionMode) -> (Core, RunBook, Gone) {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("neko-conductor-test-{}-{n}", std::process::id()));
    std::fs::create_dir_all(dir.join("w")).unwrap();
    let mut core = Core::open(&dir.join("neko.sqlite3"), Box::new(FixedClock::new(1_764_000_000_000, 1000)), Box::new(SequentialIds::default())).unwrap();
    let (task, _) = core.create_task("Run the tests").unwrap();
    let who = Who { id: "claude".into(), name: "Claude Code".into(), sign_in: "claude auth login".into(), install: "x".into(), pricing: Pricing::ApiEquivalent };
    let place = Place { workdir: dir.join("w").to_string_lossy().into_owned(), branch: None, base_commit: None, repo: None, plain: false };
    let (b, _) = RunBook::begin(&mut core, &task.id, who, place, mode, None, None).unwrap();
    (core, b, Gone(dir))
}

fn ask(id: &str, command: &str, tool_use: &str) -> AgentEvent {
    AgentEvent::PermissionAsked { id: id.into(), tool: "Bash".into(), action: Action::Command { command: command.into() }, tool_use: Some(tool_use.into()) }
}

#[test]
fn a_hooks_request_is_answered_to_the_hook_and_claude_codes_own_ask_for_the_same_call_goes_ahead() {
    let (mut core, mut b, _g) = book(PermissionMode::Ask);
    let outs = b.on(&mut core, ask(&format!("{HOOK}t1"), "cargo test", "t1"), 1).unwrap();
    assert!(!outs.iter().any(|o| matches!(o, Out::HookAnswer { .. })), "a command waits for the person");
    let perm = core.store.pending_of_run(&b.run.id).unwrap().remove(0);
    let outs = b.answer(&mut core, &perm.id, Decision::AllowOnce, 2).unwrap();
    assert!(outs.iter().any(|o| matches!(o, Out::HookAnswer { request, answer: Answer::Allow, .. } if request == "hook:t1")));
    // The person's ask rules make Claude Code ask too: the person already said yes to this call.
    let outs = b.on(&mut core, ask("cli-1", "cargo test", "t1"), 3).unwrap();
    assert_eq!(outs, vec![Out::Answer { request: "cli-1".into(), answer: Answer::Allow }]);
    // Another call is asked.
    let outs = b.on(&mut core, ask("cli-2", "cargo test", "t2"), 4).unwrap();
    assert!(!outs.iter().any(|o| matches!(o, Out::Answer { .. })));
}

#[test]
fn a_hook_still_waiting_when_the_run_ends_hears_no() {
    let (mut core, mut b, _g) = book(PermissionMode::Ask);
    b.on(&mut core, ask(&format!("{HOOK}t1"), "rm -rf build", "t1"), 1).unwrap();
    let outs = b.finish(&mut core, Outcome::Stopped, None, 2).unwrap();
    assert!(outs.iter().any(|o| matches!(o, Out::HookAnswer { answer: Answer::Deny, why, .. } if why == RUN_ENDED)), "{outs:?}");
    assert_eq!(core.store.permissions_of(&b.run.task).unwrap()[0].state, PermissionState::Expired);
}

#[test]
fn a_read_through_the_hook_goes_ahead_and_a_command_that_exits_non_zero_finished() {
    let (mut core, mut b, _g) = book(PermissionMode::Ask);
    let w = b.workdir().to_owned();
    let outs = b.on(&mut core, AgentEvent::PermissionAsked { id: format!("{HOOK}r1"), tool: "Read".into(), action: Action::Read { path: format!("{w}/README.md") }, tool_use: Some("r1".into()) }, 1).unwrap();
    assert!(matches!(&outs[..], [Out::HookAnswer { answer: Answer::Allow, .. }]), "{outs:?}");
    b.on(&mut core, AgentEvent::ToolStarted { id: "b1".into(), name: "Bash".into(), summary: "git diff --quiet".into(), input: "git diff --quiet".into() }, 2).unwrap();
    b.on(&mut core, AgentEvent::ToolFinished { id: "b1".into(), ok: true, output: "Exit code 1".into(), exit: Some(1) }, 3).unwrap();
    let parts = core.store.parts_of(&b.run.task).unwrap();
    assert!(parts.iter().any(|p| matches!(&p.body, PartBody::Tool { state: ToolState::Done, exit: Some(1), .. })));
}

#[test]
fn following_claude_codes_rules_asks_what_it_asks() {
    let (mut core, mut b, _g) = book(PermissionMode::Follow);
    let w = b.workdir().to_owned();
    let outs = b.on(&mut core, AgentEvent::PermissionAsked { id: "cli-1".into(), tool: "Edit".into(), action: Action::Edit { path: format!("{w}/a.rs") }, tool_use: Some("e1".into()) }, 1).unwrap();
    assert!(!outs.iter().any(|o| matches!(o, Out::Answer { .. })), "Claude Code asked, so the person is asked");
    assert_eq!(core.store.pending_of_run(&b.run.id).unwrap().len(), 1);
}
