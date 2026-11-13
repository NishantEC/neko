//! What keeps routines and goals honest about time and money: noticing that
//! Neko (or the Mac) was down across due times, deciding whether to catch up
//! once or skip, and the daily spend ceiling's gate. The runner is
//! `routines.rs`; the rules are here so tests need no model and no clock.
//! See docs/knowledge/neko-maintains-itself.md.
use neko_core::routines as core;
use neko_protocol::routines::{Missed, Routine, RoutineState};
use std::sync::Mutex;

/// Its own tiny row, rewritten at most once a minute, quietly (no client wakes for it).
const ALIVE_ROW: &str = "routine_alive_v1";
const ALIVE_EVERY_MS: i64 = 60_000;
/// A gap between ticks longer than this means Neko was not running (the tick is 30 s).
const DOWN_GAP_MS: i64 = 10 * 60 * 1000;

/// When the tick last ran, in memory, and the window of the last downtime.
#[derive(Debug, Default)]
pub(crate) struct Ledger {
    last_tick_ms: i64,
    persisted_ms: i64,
    loaded: bool,
    down: Option<(i64, i64)>,
}

static LEDGER: Mutex<Ledger> = Mutex::new(Ledger { last_tick_ms: 0, persisted_ms: 0, loaded: false, down: None });

impl Ledger {
    /// Called at the start of every tick. After a gap longer than `DOWN_GAP_MS`
    /// (the daemon restarted, or the Mac slept) the gap is the downtime.
    pub(crate) fn tick(&mut self, db: &neko_core::Db, now_ms: i64) {
        if !self.loaded {
            self.loaded = true;
            self.last_tick_ms = db.get_setting(ALIVE_ROW).ok().flatten().and_then(|t| t.trim().parse().ok()).unwrap_or(0);
            self.persisted_ms = self.last_tick_ms;
        }
        if self.last_tick_ms > 0 && now_ms - self.last_tick_ms > DOWN_GAP_MS {
            self.down = Some((self.last_tick_ms, now_ms));
        }
        self.last_tick_ms = now_ms;
        if now_ms - self.persisted_ms >= ALIVE_EVERY_MS && db.set_setting_quiet(ALIVE_ROW, &now_ms.to_string()).is_ok() {
            self.persisted_ms = now_ms;
        }
    }

    pub(crate) fn downtime(&self) -> Option<(i64, i64)> {
        self.down
    }
}

/// The process-wide ledger: records this tick, returns the last downtime window.
pub(crate) fn note_tick(db: &neko_core::Db, now_ms: i64) -> Option<(i64, i64)> {
    let mut ledger = LEDGER.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    ledger.tick(db, now_ms);
    ledger.downtime()
}

/// The downtime has been dealt with: nothing overdue is left to attribute to it.
pub(crate) fn clear_downtime() {
    LEDGER.lock().unwrap_or_else(std::sync::PoisonError::into_inner).down = None;
}

/// What to do with a routine that is due.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Plan {
    /// Run it now; `Some` when due times were missed while Neko was down (one run covers them all).
    Run(Option<Missed>),
    /// Missed while down and this routine does not catch up: skip them, wait for the next due time.
    Skip(Missed),
}

pub(crate) fn plan<Tz: chrono::TimeZone>(routine: &Routine, state: &RoutineState, now_ms: i64, down: Option<(i64, i64)>, tz: &Tz) -> Plan {
    if state.run_requested {
        return Plan::Run(None);
    }
    let (Some(due), Some((since, until))) = (state.next_due_ms, down) else { return Plan::Run(None) };
    match core::missed_in(&routine.cadence, due, since, until.min(now_ms), tz) {
        None => Plan::Run(None),
        Some(missed) if routine.catches_up() => Plan::Run(Some(missed)),
        Some(missed) => Plan::Skip(missed),
    }
}

/// The spend ceiling's gate: whether new goal turns and routine runs wait.
/// Off (no ceiling) costs a cached snapshot read. When today's reported spend has
/// reached the ceiling, raises one Inbox item for the day that offers raising it, and
/// closes it again once the ceiling is raised or the day turns.
pub(crate) fn spend_held(db: &neko_core::Db, now_ms: i64) -> bool {
    let Ok(snapshot) = neko_core::workbench::load(db) else { return false };
    if snapshot.daily_budget_cents.is_none() && neko_core::spend::notice(db).seq.is_none() {
        return false;
    }
    let today = neko_core::spend::today(db, &snapshot, now_ms);
    let notice = neko_core::spend::notice(db);
    if !today.reached() {
        if let Some(seq) = notice.seq {
            let _ = crate::routines::close_seqs(db, &[seq], "Ceiling raised or a new day began");
            let _ = neko_core::spend::save_notice(db, &neko_core::spend::Notice::default());
        }
        return false;
    }
    if notice.seq.is_none() || notice.day_ms != today.since_ms {
        if let Some(old) = notice.seq {
            let _ = crate::routines::close_seqs(db, &[old], "A new day began");
        }
        let ceiling = f64::from(today.ceiling_cents.unwrap_or(0)) / 100.0;
        let detail = format!(
            "Today's reported spend is ${:.2} of the ${ceiling:.2} daily ceiling. New goal turns and routine runs are paused; work already running finishes. Raise the ceiling in Settings, AI and agents, Budget, or leave it: it resets at midnight.",
            today.total_usd()
        );
        let seq = snapshot.workspaces.first().and_then(|w| crate::routines::raise_goal_item(db, &w.id, "Daily spend ceiling reached", &detail, "").ok());
        let _ = neko_core::spend::save_notice(db, &neko_core::spend::Notice { day_ms: today.since_ms, seq });
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::routines::{Cadence, RoutineKind};

    fn routine(kind: RoutineKind, catch_up: Option<bool>) -> Routine {
        Routine { id: "r".into(), workspace_id: "w".into(), kind, name: "r".into(), instruction: "x".into(), cadence: Cadence::Every { minutes: 20, from_hour: 0, to_hour: 0 }, enabled: true, model: String::new(), paseo_schedule: None, browser: false, computer: false, customized: false, catch_up }
    }

    const MIN: i64 = 60_000;

    #[test]
    fn a_gap_in_ticks_is_downtime_and_a_normal_tick_is_not() {
        let db = neko_core::Db::open_in_memory().unwrap();
        let mut ledger = Ledger::default();
        ledger.tick(&db, 100 * MIN);
        ledger.tick(&db, 100 * MIN + 30_000);
        assert_eq!(ledger.downtime(), None);
        ledger.tick(&db, 200 * MIN);
        assert_eq!(ledger.downtime(), Some((100 * MIN + 30_000, 200 * MIN)));
        // A restart reads the row the last process wrote.
        let mut fresh = Ledger::default();
        fresh.tick(&db, 400 * MIN);
        assert_eq!(fresh.downtime().map(|(since, _)| since), Some(ledger.persisted_ms));
    }

    #[test]
    fn a_routine_down_across_several_due_times_runs_once_with_the_count() {
        let utc = chrono::FixedOffset::east_opt(0).unwrap();
        let down = Some((10 * MIN, 65 * MIN));
        let mut state = RoutineState { next_due_ms: Some(20 * MIN), ..Default::default() };
        let messages = routine(RoutineKind::Messages, None);
        let Plan::Run(Some(missed)) = plan(&messages, &state, 70 * MIN, down, &utc) else { panic!("expected one catch-up run") };
        assert_eq!(missed.count, 3);
        assert_eq!(missed.since_ms, 20 * MIN);
        // The run happened: the next due time is in the future, so it is not picked a second time.
        state.next_due_ms = Some(core::next_due(&messages.cadence, 0, 70 * MIN, &utc));
        assert!(state.next_due_ms.unwrap() > 70 * MIN);
        let picked = crate::routines::pick_due(&core::Store { routines: vec![messages.clone()], ..Default::default() }, &[("r".into(), state.clone())], 70 * MIN, &|_| true);
        assert_eq!(picked, None, "run once, not N times");
    }

    #[test]
    fn a_cleared_downtime_no_longer_marks_anything_missed() {
        let db = neko_core::Db::open_in_memory().unwrap();
        let mut ledger = Ledger::default();
        ledger.tick(&db, 100 * MIN);
        ledger.tick(&db, 200 * MIN);
        assert!(ledger.downtime().is_some());
        ledger.down = None;
        assert_eq!(ledger.downtime(), None);
    }

    #[test]
    fn a_routine_with_catch_up_off_skips_what_it_missed() {
        let utc = chrono::FixedOffset::east_opt(0).unwrap();
        let state = RoutineState { next_due_ms: Some(20 * MIN), ..Default::default() };
        let down = Some((10 * MIN, 65 * MIN));
        assert!(matches!(plan(&routine(RoutineKind::Custom, None), &state, 70 * MIN, down, &utc), Plan::Skip(m) if m.count == 3));
        assert!(matches!(plan(&routine(RoutineKind::Custom, Some(true)), &state, 70 * MIN, down, &utc), Plan::Run(Some(_))));
        assert!(matches!(plan(&routine(RoutineKind::Messages, Some(false)), &state, 70 * MIN, down, &utc), Plan::Skip(_)));
        // Not down, or asked to run now: just run.
        assert_eq!(plan(&routine(RoutineKind::Custom, None), &state, 70 * MIN, None, &utc), Plan::Run(None));
        let asked = RoutineState { run_requested: true, ..state };
        assert_eq!(plan(&routine(RoutineKind::Custom, None), &asked, 70 * MIN, down, &utc), Plan::Run(None));
    }

    #[test]
    fn the_spend_ceiling_pauses_once_and_asks_once_per_day() {
        let db = neko_core::Db::open_in_memory().unwrap();
        let now = neko_core::workbench::now_ms();
        // No ceiling: nothing is held.
        assert!(!spend_held(&db, now));
        let repo = tempfile::tempdir().unwrap();
        let saved = neko_core::workbench::apply(&db, neko_protocol::workbench::Command::SaveWorkspace { workspace: neko_protocol::workbench::Workspace { id: String::new(), name: "w".into(), repository: repo.path().to_string_lossy().into_owned(), instructions: String::new(), away_enabled: false } }).unwrap();
        neko_core::workbench::apply(&db, neko_protocol::workbench::Command::SetDailyBudget { cents: Some(100) }).unwrap();
        assert!(!spend_held(&db, now), "nothing spent yet");
        // A ticket that reported $1.20 today.
        let _ = saved;
        let mut snapshot = neko_core::workbench::load(&db).unwrap();
        let mut task: neko_protocol::workbench::Task = serde_json::from_value(serde_json::json!({"id": "t1", "workspace_id": snapshot.workspaces[0].id, "title": "t", "goal": "g", "status": "Completed", "plan": "", "result": "", "worktree": null, "events": [], "created_at_ms": 0, "updated_at_ms": 0})).unwrap();
        task.events.push(neko_protocol::workbench::TaskEvent { at_ms: now, role: "builder".into(), message: "USAGE {\"cost_usd\":1.2}".into() });
        snapshot.tasks.push(task);
        neko_core::workbench::save(&db, &snapshot).unwrap();
        assert!(spend_held(&db, now), "ceiling reached: goal turns and routine runs wait");
        assert!(spend_held(&db, now + 1000));
        let items = crate::routines::all_items(&db).unwrap();
        assert_eq!(items.iter().filter(|i| i.title == "Daily spend ceiling reached" && !i.done).count(), 1, "one Inbox item, however often it is asked");
        // Raising the ceiling releases it and closes the item.
        neko_core::workbench::apply(&db, neko_protocol::workbench::Command::SetDailyBudget { cents: Some(500) }).unwrap();
        assert!(!spend_held(&db, now));
        let items = crate::routines::all_items(&db).unwrap();
        assert_eq!(items.iter().filter(|i| i.title == "Daily spend ceiling reached" && !i.done).count(), 0);
        // Yesterday's spend does not count.
        neko_core::workbench::apply(&db, neko_protocol::workbench::Command::SetDailyBudget { cents: Some(100) }).unwrap();
        assert!(!spend_held(&db, now + 3 * 86_400_000), "a new day starts at zero");
    }
}
