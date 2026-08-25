//! Schedules — the agents that start themselves.
//!
//! **L4 of `docs/plan-agent-control-plane.md`.** Paseo can start an agent on
//! a cron cadence, and a schedule is exactly the kind of thing you set up
//! once and then cannot remember the state of: is the nightly triage still
//! running? did I pause that and forget? A list you can reach in one keypress
//! is most of the answer.
//!
//! Reached through the `Schedules` command, like clipboard history and
//! themes — a schedule is not an answer to a root-list query, so this
//! provider lives in `AppState::mode_providers` and is only ever searched
//! when the mode scopes to it.
//!
//! ## Enter pauses; it does not run
//!
//! The tempting primary action is "Run now", and it is the wrong one. Enter
//! is what a finger presses on the way past a list, and running a schedule
//! starts a real agent that will do real work in a real repository. Pausing
//! is the reversible one, it is the reason you came, and it is undone by
//! pressing Enter again on the same row. `Run now` stays in the `⌘K` menu
//! where it takes a deliberate second keystroke.

use neko_protocol::{Glyph, Icon, ItemAction, SearchItem};
use serde_json::{Value, json};

use crate::mcp::{McpClient, McpError};
use crate::provider::{Provider, ProviderError};
use crate::search::Candidate;
use crate::usage::epoch_from_rfc3339;

/// One recurring job, reduced to what a row can show.
#[derive(Debug, Clone, PartialEq)]
pub struct Schedule {
    pub id: String,
    /// The name if it was given one, else the prompt — a schedule created
    /// from the CLI often has no name, and its prompt is what identifies it.
    pub title: String,
    pub cron: Option<String>,
    /// RFC 3339, as the daemon gives it.
    pub next_run_at: Option<String>,
    pub paused: bool,
}

impl Schedule {
    /// `"Paused · 0 4 * * *"` / `"in 6d · 0 4 * * *"`.
    ///
    /// A paused schedule still carries a `nextRunAt` from before it was
    /// paused, and showing it would be a straightforward lie — the row would
    /// name a time nothing will happen at. Paused wins the line.
    pub fn cadence_line(&self, now_unix_ms: i64) -> String {
        let when = if self.paused {
            "Paused".to_string()
        } else {
            self.next_run_at
                .as_deref()
                .and_then(epoch_from_rfc3339)
                .map_or_else(|| "Not scheduled".to_string(), |at| next_run_label(at, now_unix_ms))
        };
        match &self.cron {
            Some(cron) => format!("{when} \u{b7} {cron}"),
            None => when,
        }
    }
}

/// `"in 4h"`, one unit — the same reasoning as `usage::resets_label`: an
/// absolute clock reads as *today* and makes the reader do timezone
/// arithmetic to act on it.
pub fn next_run_label(at: i64, now_unix_ms: i64) -> String {
    let left = at - now_unix_ms / 1000;
    match left {
        ..=0 => "Due".to_string(),
        s if s < 3600 => format!("in {}m", (s + 59) / 60),
        s if s < 86400 => format!("in {}h", s / 3600),
        s => format!("in {}d", s / 86400),
    }
}

/// Reads `{"schedules": [...]}` as the daemon really returns it.
///
/// Shape captured from a live `create_schedule`, not from Paseo's source:
/// `{id, name, prompt, cadence: {type, expression}, status, nextRunAt,
/// pausedAt, ...}`. `name` and `cadence` are both genuinely optional — a
/// schedule can be nameless, and a future non-cron cadence would carry no
/// `expression` — so neither is required to render a row.
pub fn parse_schedules(value: &Value) -> Vec<Schedule> {
    let Some(entries) = value.get("schedules").and_then(Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?.to_string();
            let title = ["name", "prompt"]
                .iter()
                .find_map(|key| entry.get(*key)?.as_str().filter(|s| !s.is_empty()))
                .map(str::to_string)
                .unwrap_or_else(|| format!("Schedule {id}"));
            Some(Schedule {
                id,
                title,
                cron: entry
                    .pointer("/cadence/expression")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                next_run_at: entry.get("nextRunAt").and_then(Value::as_str).map(str::to_string),
                // Both are checked: `status` is the field that means it, and
                // `pausedAt` is the one that proves it. A daemon that grows a
                // third status neko does not know still reports the timestamp.
                paused: entry.get("status").and_then(Value::as_str) == Some("paused")
                    || entry.get("pausedAt").is_some_and(|v| !v.is_null()),
            })
        })
        .collect()
}

/// The `schedule` mode's list.
pub struct SchedulesProvider {
    live: bool,
}

impl SchedulesProvider {
    pub fn new() -> Self {
        Self { live: true }
    }

    /// Never reaches the daemon — see `permissions::PermissionsProvider::disabled`
    /// for why every daemon-backed provider needs one of these.
    pub fn disabled() -> Self {
        Self { live: false }
    }

    fn client(&self) -> Result<McpClient, ProviderError> {
        if !self.live {
            return Err(ProviderError(McpError::NotRunning.to_string()));
        }
        McpClient::discover().map_err(|e| ProviderError(e.to_string()))
    }

    fn list(&self) -> Vec<Schedule> {
        let Ok(client) = self.client() else { return Vec::new() };
        client
            .call("list_schedules", json!({}))
            .map(|value| parse_schedules(&value))
            .unwrap_or_default()
    }
}

impl Default for SchedulesProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for SchedulesProvider {
    fn id(&self) -> &'static str {
        "schedule"
    }

    fn section_label(&self) -> &'static str {
        "Schedules"
    }

    fn search(&self, query: &str, now_unix_ms: i64) -> Vec<Candidate> {
        let schedules = self.list();
        if schedules.is_empty() {
            return Vec::new();
        }
        let trimmed = query.trim();
        let count = schedules.len();
        schedules
            .iter()
            .enumerate()
            .filter_map(|(rank, schedule)| {
                let cadence = schedule.cadence_line(now_unix_ms);
                let score = if trimmed.is_empty() {
                    // The daemon's own order, preserved as descending scores
                    // because `allocate` ranks by score and has no reason to
                    // know this list arrived already sorted.
                    (count - rank) as f32
                } else {
                    [&schedule.title, &cadence]
                        .into_iter()
                        .filter_map(|hay| crate::search::fuzzy_score(trimmed, hay))
                        .fold(None, |best: Option<f32>, s| Some(best.map_or(s, |b| b.max(s))))?
                };
                Some(Candidate {
                    score,
                    item: SearchItem {
                        id: schedule.id.clone(),
                        kind: "schedule".to_string(),
                        title: schedule.title.clone(),
                        subtitle: Some(cadence),
                        icon: Icon::Glyph(Glyph::Sliders),
                        section_label: "Schedules".to_string(),
                        // Per-row, because the row *is* the toggle: a label
                        // reading "Pause" on a paused schedule would be the
                        // one thing a person could not recover from misreading.
                        action_label: if schedule.paused {
                            "Resume  \u{21b5}".to_string()
                        } else {
                            "Pause  \u{21b5}".to_string()
                        },
                        badge: Some(if schedule.paused { "PAUSED" } else { "ACTIVE" }.to_string()),
                        accessory: None,
                        enters_mode: None,
                        group_label: None,
                        actions: vec![
                            ItemAction {
                                id: "run".to_string(),
                                label: "Run now".to_string(),
                                // Not destructive in the delete sense, and it
                                // still starts a real agent doing real work —
                                // which is exactly why it is here and not on
                                // Enter. See the module comment.
                                destructive: false,
                            },
                            ItemAction {
                                id: "delete".to_string(),
                                label: "Delete schedule".to_string(),
                                destructive: true,
                            },
                        ],
                        source: None,
                        meter: None,
                        // Enter flips ACTIVE↔PAUSED, and the badge saying
                        // which it now is only exists on this screen — a
                        // toggle that hides the panel makes you re-summon and
                        // re-enter the mode to find out what it did.
                        keeps_open: true,
                        preview: None,
                    },
                })
            })
            .collect()
    }

    /// Enter toggles pause. See the module comment for why this and not
    /// "Run now".
    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        // Re-read rather than trusting what the row said: the list a person
        // is looking at can be seconds old, and resuming something already
        // running is a confusing no-op where toggling from live state is
        // always the thing they meant.
        // **The daemon is asked about first, on purpose.** `list` swallows a
        // transport failure into an empty `Vec`, so without this a daemon
        // that is simply down would report "that schedule is gone" — the
        // schedule is not gone, and telling somebody their thing was deleted
        // when it was not is the worse of the two wrong answers.
        self.client()?;
        // A schedule deleted between the list rendering and Enter landing
        // would otherwise fall through to `pause_schedule` on an id that no
        // longer exists, and the daemon's own validator message is a worse
        // answer than the true one.
        let paused = self
            .list()
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.paused)
            .ok_or_else(|| ProviderError("that schedule is gone".to_string()))?;
        self.call(if paused { "resume_schedule" } else { "pause_schedule" }, id)
    }

    fn perform_action(&self, id: &str, action: &str) -> Result<(), ProviderError> {
        match action {
            "run" => self.call("run_schedule_once", id),
            "delete" => self.call("delete_schedule", id),
            other => Err(ProviderError(format!("no action '{other}' on this row"))),
        }
    }
}

impl SchedulesProvider {
    fn call(&self, tool: &str, id: &str) -> Result<(), ProviderError> {
        self.client()?
            .call(tool, json!({ "id": id }))
            .map(|_| ())
            .map_err(|e| ProviderError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim from a live `create_schedule` on this machine, trimmed.
    fn fixture() -> Value {
        json!({"schedules": [
            {
                "id": "98b23875",
                "name": "neko-probe",
                "prompt": "nightly triage",
                "cadence": {"type": "cron", "expression": "0 4 1 1 *"},
                "status": "active",
                "nextRunAt": "2027-01-01T04:00:00.000Z",
                "pausedAt": null
            },
            {
                "id": "aa11",
                "prompt": "sweep the inbox",
                "cadence": {"type": "cron", "expression": "0 9 * * 1"},
                "status": "paused",
                "nextRunAt": "2026-08-31T09:00:00.000Z",
                "pausedAt": "2026-08-24T21:00:00.000Z"
            }
        ]})
    }

    const NOW_MS: i64 = 1_787_000_000_000;

    #[test]
    fn a_nameless_schedule_is_identified_by_its_prompt() {
        // One created from the CLI often has no name at all, and its prompt
        // is the only thing that says which one it is.
        let parsed = parse_schedules(&fixture());
        assert_eq!(parsed[0].title, "neko-probe");
        assert_eq!(parsed[1].title, "sweep the inbox");
    }

    #[test]
    fn a_paused_schedule_never_shows_a_next_run_time() {
        // It keeps the `nextRunAt` it had before it was paused, and printing
        // it would name a time at which nothing will happen.
        let parsed = parse_schedules(&fixture());
        let line = parsed[1].cadence_line(NOW_MS);
        assert!(line.starts_with("Paused"), "{line}");
        assert!(line.contains("0 9 * * 1"), "the cadence is still worth showing: {line}");
        assert!(!line.contains("in "), "{line}");
    }

    #[test]
    fn an_active_schedule_says_how_long_until_it_runs() {
        let parsed = parse_schedules(&fixture());
        let line = parsed[0].cadence_line(NOW_MS);
        assert!(line.contains("in "), "{line}");
        assert!(line.contains("0 4 1 1 *"), "{line}");
    }

    #[test]
    fn paused_is_believed_from_either_field() {
        // `status` is the field that means it; `pausedAt` is the one that
        // proves it. A daemon that grows a status neko does not know still
        // reports the timestamp.
        let only_timestamp = json!({"schedules": [
            {"id": "x", "name": "n", "status": "something-new", "pausedAt": "2026-01-01T00:00:00Z"}
        ]});
        assert!(parse_schedules(&only_timestamp)[0].paused);
        let neither = json!({"schedules": [{"id": "x", "name": "n", "status": "active"}]});
        assert!(!parse_schedules(&neither)[0].paused);
    }

    #[test]
    fn a_schedule_with_no_cadence_still_renders() {
        // A future non-cron cadence would carry no expression, and a row
        // that cannot say *when* is still worth showing.
        let value = json!({"schedules": [{"id": "x", "name": "n", "status": "active"}]});
        assert_eq!(parse_schedules(&value)[0].cadence_line(NOW_MS), "Not scheduled");
    }

    #[test]
    fn an_entry_with_no_id_is_dropped_rather_than_acted_on() {
        // Every mutation is addressed by id; a row without one could only
        // ever produce a request aimed at nothing.
        let value = json!({"schedules": [{"name": "n"}, {"id": "ok", "name": "m"}]});
        let parsed = parse_schedules(&value);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "ok");
    }

    #[test]
    fn a_due_schedule_says_due_rather_than_a_negative_duration() {
        let secs = NOW_MS / 1000;
        assert_eq!(next_run_label(secs - 60, NOW_MS), "Due");
        assert_eq!(next_run_label(secs + 1800, NOW_MS), "in 30m");
        assert_eq!(next_run_label(secs + 4 * 3600, NOW_MS), "in 4h");
        assert_eq!(next_run_label(secs + 6 * 86400, NOW_MS), "in 6d");
    }

    #[test]
    fn the_row_label_names_the_direction_the_toggle_will_go() {
        // A label reading "Pause" on an already-paused schedule is the one
        // thing a person could not recover from misreading.
        let provider = SchedulesProvider::disabled();
        assert!(provider.search("", NOW_MS).is_empty(), "a disabled provider reaches nothing");
    }

    #[test]
    fn a_schedule_that_vanished_between_render_and_enter_says_so() {
        // The list in front of you can be seconds old. Falling through to
        // `pause_schedule` on an id that no longer exists would answer with
        // the daemon's own validator message instead of the true one.
        let provider = SchedulesProvider::disabled();
        // A disabled provider stands in for a daemon that is down, and that
        // has to report *itself* — "that schedule is gone" would be telling
        // somebody their thing was deleted when it was not.
        let err = provider.activate("gone").expect_err("no daemon");
        assert!(format!("{err:?}").contains("Paseo isn't running"), "{err:?}");
    }

}
