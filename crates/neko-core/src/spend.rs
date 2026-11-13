//! What the day's model use has cost, from what agents reported, and the daily
//! ceiling that pauses new goal turns and routine runs. Reuses the existing
//! machinery: a ticket's `USAGE {"cost_usd": …}` events (the same ones
//! Settings → Budget's per-task limit reads) and, new, the cost a routine's run
//! reports (`RoutineRun::cost_micro_usd`). Only runtimes that report a price
//! count (Claude Code, OpenCode); Codex subscriptions do not.
//! See docs/knowledge/neko-maintains-itself.md.
use neko_protocol::workbench::{Snapshot, Task};
use serde::{Deserialize, Serialize};

/// Its own small row: which day's ceiling notice is open, and its Inbox item.
const NOTICE_ROW: &str = "neko_spend_notice_v1";

/// The start of the local day containing `now_ms`.
pub fn day_start_ms<Tz: chrono::TimeZone>(now_ms: i64, tz: &Tz) -> i64 {
    use chrono::Datelike;
    let Some(now) = tz.timestamp_millis_opt(now_ms).single() else { return now_ms - now_ms.rem_euclid(86_400_000) };
    tz.with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0).earliest().map_or(now_ms - now_ms.rem_euclid(86_400_000), |t| t.timestamp_millis())
}

pub fn day_start_local(now_ms: i64) -> i64 {
    day_start_ms(now_ms, &chrono::Local)
}

fn usage_usd(message: &str) -> Option<f64> {
    let json = message.strip_prefix("USAGE ")?;
    let cost = serde_json::from_str::<serde_json::Value>(json).ok()?["cost_usd"].as_f64()?;
    (cost.is_finite() && cost >= 0.0).then_some(cost)
}

/// What a ticket's reported usage events add up to from `since_ms` on.
pub fn task_usd_since(task: &Task, since_ms: i64) -> f64 {
    task.events.iter().filter(|e| e.at_ms >= since_ms).filter_map(|e| usage_usd(&e.message)).sum()
}

pub fn tasks_usd_since(snapshot: &Snapshot, since_ms: i64) -> f64 {
    snapshot.tasks.iter().map(|t| task_usd_since(t, since_ms)).sum()
}

/// What routine runs started from `since_ms` on reported, across every routine.
pub fn routines_usd_since(db: &crate::Db, since_ms: i64) -> f64 {
    let Ok(store) = crate::routines::load(db) else { return 0.0 };
    store
        .routines
        .iter()
        .filter_map(|r| crate::routines::load_runs(db, &r.id).ok())
        .flatten()
        .filter(|run| run.started_ms >= since_ms)
        .map(|run| run.cost_micro_usd as f64 / 1_000_000.0)
        .sum()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Today {
    pub since_ms: i64,
    pub tasks_usd: f64,
    pub routines_usd: f64,
    pub ceiling_cents: Option<u32>,
}

impl Today {
    pub fn total_usd(&self) -> f64 {
        self.tasks_usd + self.routines_usd
    }
    /// The ceiling is set and today's reported spend has reached it.
    pub fn reached(&self) -> bool {
        self.ceiling_cents.is_some_and(|cents| self.total_usd() >= f64::from(cents) / 100.0)
    }
}

pub fn today(db: &crate::Db, snapshot: &Snapshot, now_ms: i64) -> Today {
    let since_ms = day_start_local(now_ms);
    Today { since_ms, tasks_usd: tasks_usd_since(snapshot, since_ms), routines_usd: routines_usd_since(db, since_ms), ceiling_cents: snapshot.daily_budget_cents }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Notice {
    /// The local day (its start) the open notice is for.
    pub day_ms: i64,
    pub seq: Option<u64>,
}

pub fn notice(db: &crate::Db) -> Notice {
    db.get_setting(NOTICE_ROW).ok().flatten().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_notice(db: &crate::Db, notice: &Notice) -> Result<(), String> {
    let text = serde_json::to_string(notice).map_err(|e| e.to_string())?;
    db.set_setting(NOTICE_ROW, &text).map_err(|e| e.to_string())
}

/// The sum of a USAGE-style cost in millionths of a dollar, for the run record.
pub fn micro_usd(usd: f64) -> u64 {
    if usd.is_finite() && usd > 0.0 { (usd * 1_000_000.0).round() as u64 } else { 0 }
}

/// The cost a USAGE event message reports, for callers that watch a run's events.
pub fn event_cost_usd(message: &str) -> Option<f64> {
    usage_usd(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::workbench::TaskEvent;

    fn task_with(events: &[(i64, &str)]) -> Task {
        let mut task: Task = serde_json::from_value(serde_json::json!({"id": "t", "workspace_id": "w", "title": "t", "goal": "g", "status": "Completed", "plan": "", "result": "", "worktree": null, "created_at_ms": 0, "updated_at_ms": 0, "events": []})).unwrap_or_else(|_| panic!("task fixture"));
        task.events = events.iter().map(|(at, m)| TaskEvent { at_ms: *at, role: "agent".into(), message: (*m).into() }).collect();
        task
    }

    #[test]
    fn a_days_spend_counts_only_that_days_reported_usage() {
        let task = task_with(&[(100, "USAGE {\"cost_usd\":0.50}"), (5_000, "USAGE {\"cost_usd\":0.25}"), (6_000, "USAGE not json"), (7_000, "Command completed")]);
        assert!((task_usd_since(&task, 0) - 0.75).abs() < 1e-9);
        assert!((task_usd_since(&task, 1_000) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn the_day_starts_at_local_midnight() {
        let tz = chrono::FixedOffset::east_opt(5 * 3600 + 1800).unwrap();
        // 2026-11-14 01:00 local is 2026-11-13 19:30 UTC.
        let now = 1_794_598_800_000;
        let start = day_start_ms(now, &tz);
        assert!(start <= now && now - start < 86_400_000);
        use chrono::TimeZone;
        assert_eq!(tz.timestamp_millis_opt(start).unwrap().format("%H:%M:%S").to_string(), "00:00:00");
    }

    #[test]
    fn the_ceiling_is_reached_by_tasks_and_routines_together() {
        let mut today = Today { since_ms: 0, tasks_usd: 0.6, routines_usd: 0.3, ceiling_cents: Some(100) };
        assert!(!today.reached());
        today.routines_usd = 0.4;
        assert!(today.reached());
        today.ceiling_cents = None;
        assert!(!today.reached(), "off by default");
    }
}
