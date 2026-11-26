//! The run guard with real processes: a grandchild in its own process group (as an agent's shell command is) dies when the run
//! ends and when it is stopped. (When the daemon itself dies: `crates/neko-daemon/tests/runs.rs`.) Every wait has a deadline.

mod support;
use std::time::Duration;

use neko_core::runtime::{Outcome, Runtime};
use support::*;

#[test]
fn a_deep_descendant_dies_when_the_run_ends() {
    let dir = Temp::new("guard-end");
    let pidfile = dir.0.join("grandchild.pid");
    let s = claude_on_fake().start(spec(&format!("[fake:spawn:{}]", pidfile.display()), &dir.0)).unwrap();
    let pid = pid_in(&pidfile);
    let _reap = Reap(pid);
    let ev = drain(&s, Duration::from_secs(20), |_, _| None);
    assert_eq!(finished(&ev), &Outcome::Done);
    until("the grandchild to die after the run ended", Duration::from_secs(5), || !alive(pid));
}

#[test]
fn a_deep_descendant_dies_when_the_run_is_stopped() {
    let dir = Temp::new("guard-stop");
    let pidfile = dir.0.join("grandchild.pid");
    let s = claude_on_fake().start(spec(&format!("[fake:spawn-stay:{}]", pidfile.display()), &dir.0)).unwrap();
    let pid = pid_in(&pidfile);
    let _reap = Reap(pid);
    assert!(alive(pid));
    // Long enough for the guard to have looked at the tree.
    until("the run to say something after the spawn", Duration::from_secs(10), || matches!(s.events.recv_timeout(Duration::from_millis(50)), Ok(neko_core::runtime::AgentEvent::TextDelta(_))));
    s.control.stop();
    let ev = drain(&s, Duration::from_secs(5), |_, _| None);
    assert_eq!(finished(&ev), &Outcome::Stopped);
    until("the grandchild to die after Stop", Duration::from_secs(5), || !alive(pid));
}
