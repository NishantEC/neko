//! The Briefing's requests and its pushes (docs/knowledge/briefing.md). The briefing is derived by the core from the store, as "Needs
//! you" is: nothing here stores a briefing, and nothing here asks a model for anything.
//!
//! - `BriefingChanged` is pushed, whole, when the content differs from what screens were last told (the moment it was made does not
//!   count). It is looked at after each marked change, by the same thread that keeps "Needs you" pushed.
//! - The morning notification is decided on the rounds scheduler's thread (`rounds.rs`): one thread, the clock handed in, no timer
//!   while the morning is off and no round is on.

use neko_core::Core;
use neko_protocol::briefing::BriefingSettings;
use neko_protocol::schedule::{format_at, parse_at};
use neko_protocol::{Change, ErrorCode, Reply, ServerMessage};

use crate::rounds::env;
use crate::server::{Shared, lock};

fn internal(e: impl std::fmt::Display) -> Reply {
    Reply::error(ErrorCode::Internal, e.to_string())
}

/// The briefing for the current window, or for the window given.
pub fn get(shared: &Shared, from_ms: Option<i64>, to_ms: Option<i64>) -> Reply {
    let core = lock(&shared.core);
    match core.briefing(from_ms, to_ms, env(shared)) {
        Ok(briefing) => Reply::Briefing { briefing },
        Err(e) => internal(e),
    }
}

/// The person has read it: the next window starts now.
pub fn mark_read(shared: &Shared) -> Reply {
    let mut core = lock(&shared.core);
    match core.mark_briefing_read(env(shared)) {
        Ok((briefing, revision)) => {
            *lock(&shared.briefing_last) = Some(briefing.clone());
            shared.broadcast(&ServerMessage::Event { revision, change: Change::BriefingChanged { briefing: briefing.clone() } });
            // The morning notification has nothing new to say any more: the scheduler looks again.
            shared.rounds.poke();
            Reply::Briefing { briefing }
        }
        Err(e) => internal(e),
    }
}

pub fn settings(shared: &Shared) -> Reply {
    match lock(&shared.core).store.briefing_settings() {
        Ok(settings) => Reply::BriefingSettings { settings },
        Err(e) => internal(e),
    }
}

pub fn set_settings(shared: &Shared, settings: BriefingSettings) -> Reply {
    let Some(minutes) = parse_at(&settings.morning_at) else {
        return Reply::error(ErrorCode::BadRequest, format!("\"{}\" is not a time of day. Use hours and minutes, like 08:00.", settings.morning_at));
    };
    let settings = BriefingSettings { morning_at: format_at(minutes), ..settings };
    let mut core = lock(&shared.core);
    match core.set_briefing_settings(&settings, env(shared)) {
        Ok(revision) => {
            shared.broadcast(&ServerMessage::Event { revision, change: Change::BriefingSettingsChanged { settings: settings.clone() } });
            shared.rounds.poke();
            Reply::BriefingSettings { settings }
        }
        Err(e) => internal(e),
    }
}

/// Pushes the briefing when its content differs from what screens were last told. Called with the core locked, so the push leaves in
/// order with the changes around it.
pub fn push_if_changed(shared: &Shared, core: &Core) {
    let Ok(now) = core.briefing(None, None, env(shared)) else { return };
    let mut last = lock(&shared.briefing_last);
    if last.as_ref().is_some_and(|l| l.same_content(&now)) {
        return;
    }
    *last = Some(now.clone());
    let revision = core.store.revision().unwrap_or(0);
    shared.broadcast(&ServerMessage::Event { revision, change: Change::BriefingChanged { briefing: now } });
}
