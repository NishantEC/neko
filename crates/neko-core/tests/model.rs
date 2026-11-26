//! `NEKO_CLAUDE_MODEL` names the model a run asks for (`--model`); unset, the CLI's own default applies. Driven by the fake CLI,
//! which reports the model it was given. One test in this file: it sets the process's environment.

mod support;
use std::time::Duration;

use neko_core::claude::{MODEL_ENV, model_arguments};
use neko_core::runtime::{AgentEvent, Runtime};
use support::*;

const T: Duration = Duration::from_secs(20);

fn model_of(events: &[AgentEvent]) -> Option<String> {
    events.iter().find_map(|e| if let AgentEvent::Usage(u) = e { u.model.clone() } else { None })
}

#[test]
fn the_model_switch_passes_model_only_when_set() {
    assert!(model_arguments(None).is_empty());
    assert!(model_arguments(Some("  ")).is_empty(), "an empty name is no name");
    assert_eq!(model_arguments(Some("haiku")), ["--model", "haiku"]);

    let dir = Temp::new("model");
    // SAFETY: this test binary runs this one test; nothing else reads the environment meanwhile.
    unsafe { std::env::remove_var(MODEL_ENV) };
    let s = claude_on_fake().start(spec("Hi", &dir.0)).unwrap();
    assert_eq!(model_of(&drain(&s, T, |_, _| None)).as_deref(), Some("claude-fake-1"), "unset: the CLI's default");

    // SAFETY: as above.
    unsafe { std::env::set_var(MODEL_ENV, "haiku") };
    let s = claude_on_fake().start(spec("Hi", &dir.0)).unwrap();
    assert_eq!(model_of(&drain(&s, T, |_, _| None)).as_deref(), Some("haiku"), "set: passed through as --model");
    // SAFETY: as above.
    unsafe { std::env::remove_var(MODEL_ENV) };
}
