//! The Briefing (docs/knowledge/briefing.md): what happened while the person was away, derived from the records Neko already keeps.
//! Nothing is stored of the briefing itself except where the person last read it and the morning settings, so it is the same after
//! a restart, and no model is asked to write it: it costs nothing.
//!
//! - The **window** runs from the last "mark read" (the first time, 24 hours before now) to now: start inclusive, end exclusive.
//! - **Needs you** is what still waits, whenever it began ([`needs::derive`]); the rest is what happened inside the window.
//! - **Finished** is a task whose newest run that ended in the window ended in the window and is not waiting for the person.
//! - **Rounds** is every round line (ran, skipped, missed) the scheduler wrote in the window, and every brake.
//! - **The morning time** is a daily schedule on the rounds scheduler's arithmetic; the scheduler thread asks [`Core::morning`].

use std::collections::{HashMap, HashSet};

use neko_protocol::briefing::{BriefingSettings, FIRST_WINDOW_MS, FinishedEnd, MAX_FINISHED, MAX_ROUND_LINES, RoundOutcome, first_sentence};
use neko_protocol::rounds::RunKind;
use neko_protocol::schedule::{Schedule, local_parts};
use neko_protocol::undo::{self, UndoState};
use neko_protocol::{Briefing, BriefingCounts, Changes, PartBody, Pricing, Run, RunStatus, Task, TaskId, TaskStatus};
use neko_protocol::briefing::{BriefingRound, BriefingTask, BriefingTotals, BriefingWorking};
use rusqlite::{OptionalExtension, params};

use crate::records::log_event;
use crate::rounds::Env;
use crate::store::{Store, StoreError, status_of};
use crate::{Core, needs};

/// What a brake says in a briefing.
pub const BRAKE_LINE: &str = "Turned itself off after 3 failed runs in a row.";

/// The longest a morning time may lie in the past and still be noticed: older ones are looked past (they were a day or more ago).
const LOOK_BACK_MS: i64 = 2 * 24 * 3_600_000;

impl Store {
    fn setting(&self, key: &str) -> Result<Option<String>, StoreError> {
        Ok(self.conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    fn put_setting(tx: &rusqlite::Transaction<'_>, key: &str, value: &str) -> Result<(), StoreError> {
        tx.execute("INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2", params![key, value])?;
        Ok(())
    }

    /// When the person last marked a briefing read.
    pub fn briefing_read_ms(&self) -> Result<Option<i64>, StoreError> {
        Ok(self.setting("briefing_read_ms")?.and_then(|v| v.parse().ok()))
    }

    /// The next window starts at `now`.
    pub fn mark_briefing_read(&mut self, now: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        Self::put_setting(&tx, "briefing_read_ms", &now.to_string())?;
        let seq = log_event(&tx, now, "briefing", "briefing_read", serde_json::json!({}))?;
        tx.commit()?;
        Ok(seq)
    }

    pub fn briefing_settings(&self) -> Result<BriefingSettings, StoreError> {
        let d = BriefingSettings::default();
        let on = self.setting("briefing_morning_on")?.map(|v| v == "1").unwrap_or(d.morning_on);
        let at = self.setting("briefing_morning_at")?.filter(|a| neko_protocol::schedule::parse_at(a).is_some()).unwrap_or(d.morning_at);
        Ok(BriefingSettings { morning_on: on, morning_at: at })
    }

    /// Saves the settings and restarts the morning's count from `now`: a time that has already passed today is not caught up.
    pub fn set_briefing_settings(&mut self, s: &BriefingSettings, now: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        Self::put_setting(&tx, "briefing_morning_on", if s.morning_on { "1" } else { "0" })?;
        Self::put_setting(&tx, "briefing_morning_at", &s.morning_at)?;
        Self::put_setting(&tx, "briefing_morning_cursor_ms", &now.to_string())?;
        let seq = log_event(&tx, now, "briefing", "briefing_settings", serde_json::json!({ "morning_on": s.morning_on, "morning_at": s.morning_at }))?;
        tx.commit()?;
        Ok(seq)
    }

    fn morning_cursor(&self) -> Result<Option<i64>, StoreError> {
        Ok(self.setting("briefing_morning_cursor_ms")?.and_then(|v| v.parse().ok()))
    }

    /// The first look at the morning: where counting starts. It is bookkeeping, not a change anyone hears, so it writes no event
    /// (a store nobody has touched stays at revision 0).
    fn start_morning_cursor(&mut self, now: i64) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        Self::put_setting(&tx, "briefing_morning_cursor_ms", &now.to_string())?;
        tx.commit()?;
        Ok(())
    }

    fn write_morning_cursor(&mut self, cursor: i64, posted: bool, now: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        Self::put_setting(&tx, "briefing_morning_cursor_ms", &cursor.to_string())?;
        let seq = log_event(&tx, now, "briefing", "briefing_morning", serde_json::json!({ "cursor_ms": cursor, "posted": posted }))?;
        tx.commit()?;
        Ok(seq)
    }

    /// Tasks with a run that ended in `[from, to)`, as ids.
    fn tasks_ended_between(&self, from: i64, to: i64) -> Result<Vec<TaskId>, StoreError> {
        let mut st = self.conn.prepare_cached("SELECT DISTINCT task_id FROM runs WHERE ended_ms >= ?1 AND ended_ms < ?2 AND status != 'running'")?;
        let rows = st.query_map([from, to], |r| Ok(TaskId(r.get(0)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The last words the agent said in a run.
    fn final_answer(&self, run: &str) -> Result<Option<String>, StoreError> {
        let mut st = self.conn.prepare_cached("SELECT body FROM parts WHERE run_id = ?1 AND kind = 'agent' ORDER BY seq DESC LIMIT 5")?;
        let bodies = st.query_map([run], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
        Ok(bodies.into_iter().find_map(|b| match serde_json::from_str::<PartBody>(&b) {
            Ok(PartBody::Agent { text, .. }) if !text.trim().is_empty() => Some(text),
            _ => None,
        }))
    }

    /// What the runs that ended in the window reported they cost: (sum, some of it API-equivalent). None when none reported.
    fn cost_between(&self, from: i64, to: i64) -> Result<Option<(f64, bool)>, StoreError> {
        let mut st = self.conn.prepare_cached("SELECT u.cost_usd, u.pricing FROM usage u JOIN runs r ON r.id = u.run_id WHERE r.ended_ms >= ?1 AND r.ended_ms < ?2 AND u.cost_usd IS NOT NULL")?;
        let mut rows = st.query([from, to])?;
        let (mut sum, mut any, mut api) = (0.0, false, false);
        while let Some(r) = rows.next()? {
            any = true;
            sum += r.get::<_, f64>(0)?;
            api |= status_of::<Pricing>(&r.get::<_, String>(1)?)? == Pricing::ApiEquivalent;
        }
        Ok(any.then_some((sum, api)))
    }

    /// The round lines the scheduler wrote in `[from, to)` for due times (a Run now is the person's own doing), newest first, with the
    /// round's name, workspace and cat.
    #[allow(clippy::type_complexity)]
    fn round_lines_between(&self, from: i64, to: i64) -> Result<Vec<(i64, Option<i64>, RunKind, Option<TaskId>, Option<String>, String, String, String, String)>, StoreError> {
        let mut st = self.conn.prepare_cached(
            "SELECT rr.at_ms, rr.due_ms, rr.kind, rr.task_id, rr.reason, r.id, r.name, r.workspace_id, r.cat_id FROM round_runs rr JOIN rounds r ON r.id = rr.round_id \
             WHERE rr.at_ms >= ?1 AND rr.at_ms < ?2 AND rr.due_ms IS NOT NULL ORDER BY rr.at_ms DESC, rr.id DESC",
        )?;
        let rows = st.query_map([from, to], |r| {
            Ok((r.get(0)?, r.get(1)?, status_of::<RunKind>(&r.get::<_, String>(2)?)?, r.get::<_, Option<String>>(3)?.map(TaskId), r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Rounds that turned themselves off in `[from, to)`: (when, id, name, workspace, cat).
    fn brakes_between(&self, from: i64, to: i64) -> Result<Vec<(i64, String, String, String, String)>, StoreError> {
        let mut st = self.conn.prepare_cached(
            "SELECT e.at_ms, r.id, r.name, r.workspace_id, r.cat_id FROM events e JOIN rounds r ON r.id = e.subject \
             WHERE e.kind = 'round_braked' AND e.at_ms >= ?1 AND e.at_ms < ?2 ORDER BY e.at_ms DESC",
        )?;
        let rows = st.query_map([from, to], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

/// How a run's end reads in a briefing; None for a run still going.
fn ended(status: RunStatus) -> Option<FinishedEnd> {
    match status {
        RunStatus::Running => None,
        RunStatus::Done => Some(FinishedEnd::Done),
        RunStatus::Failed => Some(FinishedEnd::Failed),
        RunStatus::Stopped => Some(FinishedEnd::Stopped),
        RunStatus::Interrupted => Some(FinishedEnd::Interrupted),
    }
}

/// What a run changed: its own changes when its checkpoints say so, else what its branch changed. None when nothing changed.
fn changes_of(run: &Run) -> Option<Changes> {
    let own = run.undo.as_ref().filter(|u| u.ready()).and_then(|u| u.changes);
    own.or(run.changes).filter(|c| c.files > 0)
}

impl Store {
    /// The briefing for `[from, to)`: see the module documentation. `first`/`read_ms` say where `from` came from.
    pub fn briefing_between(&self, from: i64, to: i64, first: bool, read_ms: Option<i64>, env: Env<'_>) -> Result<Briefing, StoreError> {
        let needs = needs::derive(self)?;
        let waiting: HashSet<TaskId> = needs.iter().map(|n| n.task.clone()).collect();
        let tasks: HashMap<TaskId, Task> = self.all_tasks()?.into_iter().map(|t| (t.id.clone(), t)).collect();
        let cats: HashMap<String, String> = self.list_cats()?.into_iter().map(|c| (c.id, c.name)).collect();
        let cat_name = |id: &str| cats.get(id).cloned().unwrap_or_else(|| "Neko".to_owned());
        // A thread with no cat (version 14) has an empty id and an empty name.
        let task_cat = |c: &Option<String>| -> (String, String) { c.as_ref().map(|c| (c.clone(), cat_name(c))).unwrap_or_default() };

        // Finished: the newest run of a task that ended in the window; not for a task that waits for the person or is working again.
        let mut finished: Vec<BriefingTask> = Vec::new();
        let mut totals = BriefingTotals::default();
        for id in self.tasks_ended_between(from, to)? {
            let Some(task) = tasks.get(&id) else { continue };
            if waiting.contains(&id) || matches!(task.status, TaskStatus::Working | TaskStatus::NeedsYou) {
                continue;
            }
            let runs = self.runs_of(&id)?;
            let Some((i, run)) = runs.iter().enumerate().rev().find(|(_, r)| r.ended_ms.is_some_and(|e| e >= from && e < to) && ended(r.status).is_some()) else { continue };
            let Some(end) = ended(run.status) else { continue };
            let answer = self.final_answer(&run.id.0)?.map(|a| first_sentence(&a)).filter(|a| !a.is_empty());
            // A run that did not finish and said nothing gives its reason instead.
            let answer = answer.or_else(|| (end != FinishedEnd::Done).then(|| run.reason.clone()).flatten());
            let changes = changes_of(run);
            let offer = undo::offers(&runs).swap_remove(i);
            let undone = offer.undone || run.undo.as_ref().is_some_and(|u| matches!(u.state, UndoState::Undone | UndoState::Dropped));
            totals.finished += 1;
            if let Some(c) = changes.filter(|_| !undone) {
                totals.files += c.files;
                totals.added += c.added;
                totals.removed += c.removed;
            }
            finished.push(BriefingTask {
                task: id,
                workspace: task.workspace.clone(),
                title: task.title.clone(),
                cat: task_cat(&task.cat).0,
                cat_name: task_cat(&task.cat).1,
                ended_ms: run.ended_ms.unwrap_or(to),
                end,
                answer,
                changes,
                branch: run.branch.clone(),
                run: run.id.clone(),
                offer,
            });
        }
        finished.sort_by(|a, b| (b.ended_ms, &b.task).cmp(&(a.ended_ms, &a.task)));
        finished.truncate(MAX_FINISHED);

        // Rounds.
        let mut rounds: Vec<BriefingRound> = Vec::new();
        for (at_ms, due_ms, kind, task, reason, round, name, workspace, cat) in self.round_lines_between(from, to)? {
            let t = task.as_ref().and_then(|t| tasks.get(t));
            rounds.push(BriefingRound {
                round,
                name,
                workspace,
                cat_name: cat_name(&cat),
                cat,
                outcome: kind.into(),
                at_ms,
                due_ms,
                task_title: t.map(|t| t.title.clone()),
                task_status: t.map(|t| t.status),
                task,
                reason,
            });
        }
        for (at_ms, round, name, workspace, cat) in self.brakes_between(from, to)? {
            rounds.push(BriefingRound { round, name, workspace, cat_name: cat_name(&cat), cat, outcome: RoundOutcome::Braked, at_ms, due_ms: None, task: None, task_title: None, task_status: None, reason: Some(BRAKE_LINE.to_owned()) });
        }
        rounds.sort_by(|a, b| (b.at_ms, &b.round).cmp(&(a.at_ms, &a.round)));
        let rounds_ran = rounds.iter().filter(|r| r.outcome == RoundOutcome::Ran).count() as u32;
        let rounds_not_run = rounds.len() as u32 - rounds_ran;
        rounds.truncate(MAX_ROUND_LINES);

        // Working now: its latest run's start.
        let mut working = Vec::new();
        // A Cattain's own thread is new until the person first writes in it: it waits, it is not starting.
        for t in tasks.values().filter(|t| t.status == TaskStatus::Working || t.status == TaskStatus::New && !t.cattain) {
            let since = self.runs_of(&t.id)?.last().map(|r| r.started_ms).unwrap_or(t.created_ms);
            working.push(BriefingWorking { task: t.id.clone(), workspace: t.workspace.clone(), title: t.title.clone(), cat: task_cat(&t.cat).0, cat_name: task_cat(&t.cat).1, since_ms: since });
        }
        working.sort_by(|a, b| (a.since_ms, &a.task).cmp(&(b.since_ms, &b.task)));

        if let Some((sum, api)) = self.cost_between(from, to)? {
            totals.cost_usd = Some(sum);
            totals.api_equivalent = api;
        }

        let settings = self.briefing_settings()?;
        let next_morning_ms = settings.morning_on.then(|| Schedule::Daily { at: settings.morning_at }.next_due(0, to.max(env.now), env.zone)).flatten();
        Ok(Briefing { from_ms: from, to_ms: to, first, read_ms, next_morning_ms, needs, finished, rounds, rounds_ran, rounds_not_run, working, totals })
    }
}

/// What the scheduler is to do about the morning notification.
#[derive(Debug, Default, PartialEq)]
pub struct Morning {
    /// The notification is due now, with these numbers: the caller sends it to the screens.
    pub due: Option<BriefingCounts>,
    /// When to look again: the next morning time. None while the morning is off, or while a notification waits for a screen
    /// (the next screen that connects looks again).
    pub wake_at_ms: Option<i64>,
}

impl Core {
    /// The briefing for the current window (the last "mark read" up to now; the first time, the last 24 hours), or for a window
    /// given: `from` inclusive, `to` exclusive, either left out for the current one's.
    pub fn briefing(&self, from: Option<i64>, to: Option<i64>, env: Env<'_>) -> Result<Briefing, StoreError> {
        let read = self.store.briefing_read_ms()?;
        let to = to.unwrap_or(env.now);
        let default_from = match read {
            Some(r) => r.min(to),
            None => to - FIRST_WINDOW_MS,
        };
        let first = read.is_none() && from.is_none();
        self.store.briefing_between(from.unwrap_or(default_from), to, first, read, env)
    }

    /// The person has read it: the window starts at `now`. Returns the briefing for the new window and the event's revision.
    pub fn mark_briefing_read(&mut self, env: Env<'_>) -> Result<(Briefing, u64), StoreError> {
        let rev = self.store.mark_briefing_read(env.now)?;
        Ok((self.briefing(None, None, env)?, rev))
    }

    pub fn set_briefing_settings(&mut self, s: &BriefingSettings, env: Env<'_>) -> Result<u64, StoreError> {
        self.store.set_briefing_settings(s, env.now)
    }

    /// Looks at the morning time. A due time since the last look is dealt with once: when it is today's and the window has news and
    /// a screen is there (`screen`), the notification is due (once: the cursor moves past it); an earlier day's (a Mac that slept
    /// through) is passed over, so a wake never stacks up notifications; a window with no news posts nothing. With no screen
    /// connected a due notification waits, and is due again at the next call, until the day ends.
    pub fn morning(&mut self, env: Env<'_>, screen: bool) -> Result<Morning, StoreError> {
        let s = self.store.briefing_settings()?;
        if !s.morning_on {
            return Ok(Morning::default());
        }
        let daily = Schedule::Daily { at: s.morning_at };
        let Some(cursor) = self.store.morning_cursor()? else {
            // First look: times before now are not news.
            self.store.start_morning_cursor(env.now)?;
            return Ok(Morning { due: None, wake_at_ms: daily.next_due(0, env.now, env.zone) });
        };
        let dues = daily.due_between(0, cursor.max(env.now - LOOK_BACK_MS), env.now, env.zone, 10);
        let Some(&latest) = dues.last() else {
            return Ok(Morning { due: None, wake_at_ms: daily.next_due(0, cursor.max(env.now), env.zone) });
        };
        let next = daily.next_due(0, latest, env.zone);
        let same_day = local_parts(env.zone, latest).day_number == local_parts(env.zone, env.now).day_number;
        if !same_day {
            self.store.write_morning_cursor(latest, false, env.now)?;
            return Ok(Morning { due: None, wake_at_ms: next });
        }
        let counts = self.briefing(None, None, env)?.counts();
        if !counts.news {
            self.store.write_morning_cursor(latest, false, env.now)?;
            return Ok(Morning { due: None, wake_at_ms: next });
        }
        if !screen {
            // Waiting for a screen: no timer until one connects (or the day ends and the next call passes it over).
            return Ok(Morning { due: None, wake_at_ms: None });
        }
        self.store.write_morning_cursor(latest, true, env.now)?;
        Ok(Morning { due: Some(counts), wake_at_ms: next })
    }
}
