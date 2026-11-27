//! Rounds (docs/knowledge/rounds.md): a cat's standing work that starts by itself.
//!
//! This module is the logic and the rows; it never reads a clock or a time zone. The scheduler thread in the daemon hands in an
//! [`Env`] (the time and the zone) and does the I/O: sleeping until [`Tick::wake_at_ms`], starting the agents of the tasks a tick
//! made. Tests hand in whatever time they like.
//!
//! - A round keeps a **cursor**: the last due time dealt with. Every due time after it, up to now, is dealt with once, in one
//!   transaction with what came of it, so a restart in the middle neither runs a time twice nor loses one.
//! - Dealing with a time: it ran (a task was made), was **skipped** (with a reason) or **missed** (too old). When several times are
//!   due at once (the daemon was off, the Mac slept) the round runs once, for the latest, if that is at most 24 hours old; the
//!   earlier ones are one history line saying they were missed.
//! - An unattended run is an ordinary task: the cat's rules, the permission mode, the run guard and checkpoints all apply, and a
//!   step that would ask waits in "Needs you". Nothing here touches permissions.

use neko_protocol::rounds::{self, BRAKE_AFTER, CATCH_UP_MS, HISTORY_KEPT, Round, RoundRun, RoundStatus, RunEnd, RunKind, TaskRound};
use neko_protocol::schedule::{Schedule, Zone};
use neko_protocol::{Change, Task, TaskId, TaskStatus, Tone, Workspace};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::records::log_event;
use crate::store::{Store, StoreError, insert_task, status_of};
use crate::{Core, NewTaskError, workspaces};

/// The time and the time zone for one call.
#[derive(Clone, Copy)]
pub struct Env<'a> {
    pub now: i64,
    pub zone: &'a dyn Zone,
}

/// A round as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundRow {
    pub id: String,
    pub name: String,
    pub words: String,
    pub cat: String,
    pub workspace: String,
    pub schedule: Schedule,
    pub enabled: bool,
    /// Where "every N hours" counts from.
    pub anchor_ms: i64,
    /// The last due time dealt with.
    pub cursor_ms: i64,
    /// Failed runs in a row.
    pub fail_streak: u32,
    /// Set when the brake turned the round off.
    pub brake_note: Option<String>,
    pub created_ms: i64,
    pub changed_ms: i64,
}

const ROUND_COLUMNS: &str = "id, name, words, cat_id, workspace_id, schedule, enabled, anchor_ms, cursor_ms, fail_streak, brake_note, created_ms, changed_ms";

fn round_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RoundRow> {
    let schedule: String = r.get(5)?;
    Ok(RoundRow {
        id: r.get(0)?,
        name: r.get(1)?,
        words: r.get(2)?,
        cat: r.get(3)?,
        workspace: r.get(4)?,
        schedule: serde_json::from_str(&schedule).unwrap_or(Schedule::Manual),
        enabled: r.get::<_, i64>(6)? != 0,
        anchor_ms: r.get(7)?,
        cursor_ms: r.get(8)?,
        fail_streak: r.get::<_, i64>(9)? as u32,
        brake_note: r.get(10)?,
        created_ms: r.get(11)?,
        changed_ms: r.get(12)?,
    })
}

const RUN_COLUMNS: &str = "id, round_id, due_ms, at_ms, kind, task_id, reason, ended";

fn run_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RoundRun> {
    Ok(RoundRun {
        id: r.get(0)?,
        round: r.get(1)?,
        due_ms: r.get(2)?,
        at_ms: r.get(3)?,
        kind: status_of(&r.get::<_, String>(4)?)?,
        task: r.get::<_, Option<String>>(5)?.map(TaskId),
        reason: r.get(6)?,
        end: r.get::<_, Option<String>>(7)?.map(|e| status_of::<RunEnd>(&e)).transpose()?,
    })
}

fn kind_name(k: RunKind) -> String {
    crate::store::status_name(k)
}

/// Why a change to a round was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum RoundError {
    NotFound,
    InvalidName(String),
    InvalidWords(String),
    /// Not allowed now; the words say why.
    Refused(String),
    /// Its last task is still working or waiting for the person.
    Busy(String),
    Store(String),
}

impl std::fmt::Display for RoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoundError::NotFound => f.write_str("There is no such round."),
            RoundError::InvalidName(w) | RoundError::InvalidWords(w) | RoundError::Refused(w) | RoundError::Busy(w) => f.write_str(w),
            RoundError::Store(e) => write!(f, "The round could not be saved: {e}"),
        }
    }
}

impl From<StoreError> for RoundError {
    fn from(e: StoreError) -> Self {
        RoundError::Store(e.to_string())
    }
}

impl From<rusqlite::Error> for RoundError {
    fn from(e: rusqlite::Error) -> Self {
        RoundError::Store(e.to_string())
    }
}

impl From<NewTaskError> for RoundError {
    fn from(e: NewTaskError) -> Self {
        match e {
            NewTaskError::Empty | NewTaskError::TooLong => RoundError::InvalidWords(e.to_string()),
            NewTaskError::NoWorkspace => RoundError::NotFound,
            _ => RoundError::Refused(e.to_string()),
        }
    }
}

/// Changes to push, in revision order, and tasks whose agents the caller starts.
#[derive(Debug, Default)]
pub struct Tick {
    pub events: Vec<(u64, Change)>,
    /// Tasks just made by a round: the caller starts their agents.
    pub started: Vec<Task>,
    /// When the next due time is, over every round that may still come due (None: nothing will, so the scheduler sleeps until
    /// something changes).
    pub wake_at_ms: Option<i64>,
}

impl Store {
    pub fn round_rows(&self) -> Result<Vec<RoundRow>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {ROUND_COLUMNS} FROM rounds ORDER BY created_ms, id"))?;
        let rows = st.query_map([], round_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn get_round_row(&self, id: &str) -> Result<Option<RoundRow>, StoreError> {
        Ok(self.conn.query_row(&format!("SELECT {ROUND_COLUMNS} FROM rounds WHERE id = ?1"), [id], round_row).optional()?)
    }

    fn write_round_row(tx: &Transaction<'_>, r: &RoundRow, kind: &str, now: i64) -> Result<u64, StoreError> {
        tx.execute(
            "UPDATE rounds SET name = ?2, words = ?3, cat_id = ?4, workspace_id = ?5, schedule = ?6, enabled = ?7, anchor_ms = ?8, cursor_ms = ?9, fail_streak = ?10, brake_note = ?11, changed_ms = ?12 WHERE id = ?1",
            params![r.id, r.name, r.words, r.cat, r.workspace, serde_json::to_string(&r.schedule).unwrap_or_default(), r.enabled as i64, r.anchor_ms, r.cursor_ms, r.fail_streak, r.brake_note, r.changed_ms],
        )?;
        log_event(tx, now, &r.id, kind, serde_json::json!({ "name": r.name, "on": r.enabled }))
    }

    /// Writes a round whole with an event of `kind`.
    pub fn write_round(&mut self, r: &RoundRow, kind: &str, now: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        let seq = Self::write_round_row(&tx, r, kind, now)?;
        tx.commit()?;
        Ok(seq)
    }

    pub fn insert_round(&mut self, r: &RoundRow, now: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            &format!("INSERT INTO rounds ({ROUND_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"),
            params![r.id, r.name, r.words, r.cat, r.workspace, serde_json::to_string(&r.schedule).unwrap_or_default(), r.enabled as i64, r.anchor_ms, r.cursor_ms, r.fail_streak, r.brake_note, r.created_ms, r.changed_ms],
        )?;
        let seq = log_event(&tx, now, &r.id, "round_added", serde_json::json!({ "name": r.name }))?;
        tx.commit()?;
        Ok(seq)
    }

    /// Removes a round and its history. Its tasks stay.
    pub fn delete_round(&mut self, id: &str, now: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM round_runs WHERE round_id = ?1", [id])?;
        tx.execute("DELETE FROM rounds WHERE id = ?1", [id])?;
        let seq = log_event(&tx, now, id, "round_removed", serde_json::json!({}))?;
        tx.commit()?;
        Ok(seq)
    }

    /// A round's history, newest first, at most `limit`.
    pub fn round_runs(&self, round: &str, limit: usize) -> Result<Vec<RoundRun>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {RUN_COLUMNS} FROM round_runs WHERE round_id = ?1 ORDER BY at_ms DESC, id DESC LIMIT ?2"))?;
        let rows = st.query_map(params![round, limit as i64], run_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Whether every round is paused.
    pub fn rounds_paused(&self) -> Result<bool, StoreError> {
        let v: Option<String> = self.conn.query_row("SELECT value FROM settings WHERE key = 'rounds_paused'", [], |r| r.get(0)).optional()?;
        Ok(v.as_deref() == Some("1"))
    }

    pub fn set_rounds_paused(&mut self, paused: bool, now: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute("INSERT INTO settings (key, value) VALUES ('rounds_paused', ?1) ON CONFLICT(key) DO UPDATE SET value = ?1", [if paused { "1" } else { "0" }])?;
        let seq = log_event(&tx, now, "rounds", "rounds_paused", serde_json::json!({ "paused": paused }))?;
        tx.commit()?;
        Ok(seq)
    }

    /// The statuses of a round's tasks that have not ended: working, waiting for a request or for an answer, or not started.
    fn unfinished_of(&self, round: &str) -> Result<Vec<TaskStatus>, StoreError> {
        let mut st = self.conn.prepare_cached("SELECT status FROM tasks WHERE round_id = ?1")?;
        let rows = st.query_map([round], |r| status_of::<TaskStatus>(&r.get::<_, String>(0)?))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?.into_iter().filter(|s| rounds::is_unfinished(*s)).collect())
    }

    /// History lines whose task had not ended when they were last looked at, oldest first.
    fn open_runs(&self) -> Result<Vec<RoundRun>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {RUN_COLUMNS} FROM round_runs WHERE kind = 'started' AND ended IS NULL ORDER BY at_ms, id"))?;
        let rows = st.query_map([], run_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Tasks a round made whose agent never started (the daemon went down between making the task and starting it).
    pub fn round_tasks_never_started(&self) -> Result<Vec<Task>, StoreError> {
        let ids: Vec<String> = {
            let mut st = self.conn.prepare("SELECT t.id FROM tasks t WHERE t.round_id IS NOT NULL AND t.status = 'new' AND NOT EXISTS (SELECT 1 FROM runs r WHERE r.task_id = t.id)")?;
            st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?
        };
        ids.into_iter().filter_map(|id| self.get_task(&TaskId(id)).transpose()).collect()
    }

    /// Writes what came of a due time in one transaction: the history lines, the cursor, and the task when one was made. Returns
    /// the task's creation event (if any) and the round's change event, in order.
    fn commit_due(&mut self, round: &RoundRow, lines: &[RoundRun], task: Option<&Task>, now: i64) -> Result<Vec<u64>, StoreError> {
        let tx = self.conn.transaction()?;
        let mut seqs = Vec::new();
        if let Some(t) = task {
            seqs.push(insert_task(&tx, t)?);
        }
        for l in lines {
            tx.execute(
                &format!("INSERT INTO round_runs ({RUN_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"),
                params![l.id, l.round, l.due_ms, l.at_ms, kind_name(l.kind), l.task.as_ref().map(|t| &t.0), l.reason, l.end.map(crate::store::status_name)],
            )?;
        }
        tx.execute(
            "DELETE FROM round_runs WHERE round_id = ?1 AND id NOT IN (SELECT id FROM round_runs WHERE round_id = ?1 ORDER BY at_ms DESC, id DESC LIMIT ?2)",
            params![round.id, HISTORY_KEPT as i64],
        )?;
        seqs.push(Self::write_round_row(&tx, round, "round_run", now)?);
        tx.commit()?;
        Ok(seqs)
    }
}

/// "3 earlier times" / "1 earlier time".
fn times(n: usize) -> String {
    if n == 1 { "1 earlier time".into() } else { format!("{n} earlier times") }
}

impl Core {
    fn round_row(&self, id: &str) -> Result<RoundRow, RoundError> {
        self.store.get_round_row(id)?.ok_or(RoundError::NotFound)
    }

    /// Why this round does not run by itself now, if it does not: its status and the words for it.
    fn round_status(&self, r: &RoundRow, paused: bool) -> (RoundStatus, Option<String>) {
        if !r.enabled {
            return match &r.brake_note {
                Some(n) => (RoundStatus::Braked, Some(n.clone())),
                None => (RoundStatus::Off, None),
            };
        }
        if paused {
            return (RoundStatus::Paused, None);
        }
        if let Some(w) = self.blocker(r) {
            return w;
        }
        if r.schedule.is_automatic() { (RoundStatus::Scheduled, None) } else { (RoundStatus::Manual, None) }
    }

    /// What stops a round starting a task now (not counting a pause or its switch): a retired cat, a removed workspace, a missing
    /// folder.
    fn blocker(&self, r: &RoundRow) -> Option<(RoundStatus, Option<String>)> {
        if let Ok(Some(c)) = self.store.get_cat(&r.cat)
            && !c.is_active()
        {
            return Some((RoundStatus::CatRetired, Some(format!("{} is retired.", c.name))));
        }
        if let Ok(Some(w)) = self.store.get_workspace(&r.workspace) {
            if !w.is_active() {
                return Some((RoundStatus::WorkspaceRemoved, Some(format!("{} was removed from Neko.", w.name))));
            }
            if workspaces::with_missing(w.clone()).missing {
                return Some((RoundStatus::FolderMissing, Some(format!("The folder of {} was not found.", w.name))));
            }
        }
        None
    }

    /// A round as a screen sees it, with its status, next time and last line.
    pub fn round_view(&self, r: &RoundRow, env: Env<'_>) -> Result<Round, StoreError> {
        let paused = self.store.rounds_paused()?;
        let (status, note) = self.round_status(r, paused);
        let next_due_ms = status.runs().then(|| r.schedule.next_due(r.anchor_ms, r.cursor_ms, env.zone)).flatten();
        Ok(Round {
            id: r.id.clone(),
            name: r.name.clone(),
            words: r.words.clone(),
            cat: r.cat.clone(),
            workspace: r.workspace.clone(),
            schedule: r.schedule.clone(),
            on: r.enabled,
            created_ms: r.created_ms,
            changed_ms: r.changed_ms,
            status,
            next_due_ms,
            note,
            last: self.store.round_runs(&r.id, 1)?.into_iter().next(),
        })
    }

    /// Every round, oldest first, and whether all are paused.
    pub fn rounds(&self, env: Env<'_>) -> Result<(Vec<Round>, bool), StoreError> {
        let paused = self.store.rounds_paused()?;
        let rounds = self.store.round_rows()?.iter().map(|r| self.round_view(r, env)).collect::<Result<_, _>>()?;
        Ok((rounds, paused))
    }

    pub fn round(&self, id: &str, env: Env<'_>) -> Result<Round, RoundError> {
        let row = self.round_row(id)?;
        Ok(self.round_view(&row, env)?)
    }

    pub fn round_history(&self, id: &str) -> Result<Vec<RoundRun>, RoundError> {
        self.round_row(id)?;
        Ok(self.store.round_runs(id, HISTORY_KEPT)?)
    }

    /// The cat and workspace a round may be saved with: an active cat, a workspace that is there (its folder may be missing: the
    /// round then says so and skips).
    fn check_round_parts(&self, cat: &str, workspace: Option<&str>) -> Result<(String, Workspace), RoundError> {
        let c = self.store.get_cat(cat)?.ok_or_else(|| RoundError::Refused("That cat is not there any more. Choose another.".into()))?;
        if !c.is_active() {
            return Err(RoundError::Refused(format!("{} is retired. Choose another cat, or bring {} back from Settings.", c.name, c.name)));
        }
        let ws = workspace.filter(|w| !w.is_empty()).unwrap_or(crate::DEFAULT_WORKSPACE);
        let w = self.store.get_workspace(ws)?.ok_or_else(|| RoundError::Refused("That project is not there any more. Choose another.".into()))?;
        if !w.is_active() {
            return Err(RoundError::Refused(format!("{} was removed from Neko. Choose another project.", w.name)));
        }
        Ok((c.id, w))
    }

    fn check_round_words(words: &str) -> Result<(), RoundError> {
        if words.trim().is_empty() {
            return Err(RoundError::InvalidWords("Say what the round should do.".into()));
        }
        if words.len() > neko_protocol::MAX_WORDS_BYTES {
            return Err(RoundError::InvalidWords("That is longer than a thread can be. Put the long part in a file.".into()));
        }
        Ok(())
    }

    /// A new round. A schedule counts from `env.now`: its first run is the first due time after it.
    pub fn create_round(&mut self, name: &str, words: &str, cat: &str, workspace: Option<&str>, schedule: &Schedule, on: bool, env: Env<'_>) -> Result<(Round, u64), RoundError> {
        let name = rounds::clean_name(name).map_err(RoundError::InvalidName)?;
        Self::check_round_words(words)?;
        let schedule = schedule.normalized().map_err(RoundError::Refused)?;
        let (cat, w) = self.check_round_parts(cat, workspace)?;
        if self.store.round_rows()?.len() >= rounds::MAX_ROUNDS {
            return Err(RoundError::Refused(format!("Neko keeps at most {} rounds. Remove one first.", rounds::MAX_ROUNDS)));
        }
        let enabled = on || !schedule.is_automatic();
        let row = RoundRow {
            id: self.new_id("round"),
            name,
            words: words.to_owned(),
            cat,
            workspace: w.id,
            schedule,
            // A manual round has nothing to switch: it is always on, and runs when the person presses Run now.
            enabled,
            anchor_ms: env.now,
            cursor_ms: env.now,
            fail_streak: 0,
            brake_note: None,
            created_ms: env.now,
            changed_ms: env.now,
        };
        let rev = self.store.insert_round(&row, env.now)?;
        Ok((self.round_view(&row, env)?, rev))
    }

    /// Changes a round whole. A different schedule (or cat or workspace) counts from now, so nothing that was due under the old
    /// one is caught up. The brake stays as it was: turning the round on clears it.
    pub fn update_round(&mut self, id: &str, name: &str, words: &str, cat: &str, workspace: Option<&str>, schedule: &Schedule, env: Env<'_>) -> Result<(Round, u64), RoundError> {
        let mut row = self.round_row(id)?;
        let name = rounds::clean_name(name).map_err(RoundError::InvalidName)?;
        Self::check_round_words(words)?;
        let schedule = schedule.normalized().map_err(RoundError::Refused)?;
        // A cat that is retired may stay the round's until it is changed: only a new choice is checked.
        let (cat, w) = if cat == row.cat && workspace.map_or(crate::DEFAULT_WORKSPACE, |w| if w.is_empty() { crate::DEFAULT_WORKSPACE } else { w }) == row.workspace {
            (row.cat.clone(), self.store.get_workspace(&row.workspace)?.ok_or(RoundError::Refused("That project is not there any more. Choose another.".into()))?)
        } else {
            self.check_round_parts(cat, workspace)?
        };
        if schedule != row.schedule {
            row.anchor_ms = env.now;
            row.cursor_ms = env.now;
        }
        row.name = name;
        row.words = words.to_owned();
        if !schedule.is_automatic() {
            row.enabled = true;
            row.brake_note = None;
        }
        row.cat = cat;
        row.workspace = w.id;
        row.schedule = schedule;
        row.changed_ms = env.now;
        let rev = self.store.write_round(&row, "round_updated", env.now)?;
        Ok((self.round_view(&row, env)?, rev))
    }

    /// Turns a round on or off. Turning on counts from now and clears the brake.
    pub fn set_round_on(&mut self, id: &str, on: bool, env: Env<'_>) -> Result<(Round, u64), RoundError> {
        let mut row = self.round_row(id)?;
        if on && !row.enabled {
            row.anchor_ms = env.now;
            row.cursor_ms = env.now;
            row.fail_streak = 0;
            row.brake_note = None;
        }
        if !on {
            // The brake's note belongs to the brake: switching off by hand after it keeps the round simply off.
            row.brake_note = None;
        }
        row.enabled = on;
        row.changed_ms = env.now;
        let rev = self.store.write_round(&row, if on { "round_on" } else { "round_off" }, env.now)?;
        Ok((self.round_view(&row, env)?, rev))
    }

    /// Removes a round and its history; its tasks stay.
    pub fn remove_round(&mut self, id: &str, env: Env<'_>) -> Result<u64, RoundError> {
        self.round_row(id)?;
        Ok(self.store.delete_round(id, env.now)?)
    }

    /// Pauses every round, or lets them go on. While paused each due time passes as a skipped line, so nothing is caught up after.
    pub fn set_rounds_paused(&mut self, paused: bool, env: Env<'_>) -> Result<(Vec<(u64, Change)>, u64), RoundError> {
        let rev = self.store.set_rounds_paused(paused, env.now)?;
        let mut events = vec![(rev, Change::RoundsPaused { paused })];
        for r in self.store.round_rows()? {
            events.push((rev, Change::RoundUpdated { round: self.round_view(&r, env)? }));
        }
        Ok((events, rev))
    }

    /// Makes the round's task: the round's words under its name, for its cat in its workspace, marked as the round's. `due_ms` None
    /// is Run now. Everything an ordinary task has applies; nothing about permission is set here.
    fn plan_round_task(&mut self, r: &RoundRow, due_ms: Option<i64>) -> Result<Task, NewTaskError> {
        let (id, w, cat, now) = self.plan_task(&r.words, Some(&r.workspace), Some(&r.cat))?;
        Ok(Task {
            id,
            workspace: w.id,
            words: r.words.clone(),
            title: r.name.clone(),
            created_ms: now,
            status: TaskStatus::New,
            folder: w.folder,
            cat: cat.map(|c| c.id),
            in_folder: false,
            from_branch: None,
            round: Some(TaskRound { id: r.id.clone(), name: r.name.clone(), due_ms }),
            // A round's task is unread until the person opens it.
            last_read_ms: 0,
            activity_ms: now,
            latest_line: String::new(),
            failure_line: String::new(),
            thread_status: neko_protocol::ThreadStatus::Idle,
            needs_you: false,
            subagent: None,
        })
    }

    /// The words that say the round's last task is not finished, when it is not.
    fn overlap(&self, r: &RoundRow) -> Result<Option<String>, StoreError> {
        let open = self.store.unfinished_of(&r.id)?;
        Ok(open.first().map(|s| {
            if matches!(s, TaskStatus::NeedsYou | TaskStatus::Asked) { "The last run is waiting for you.".to_owned() } else { "The last run is still working.".to_owned() }
        }))
    }

    /// Runs a round now, whatever its schedule, switch or pause say. Refused while its last task is unfinished, and when its cat is
    /// retired or its workspace is gone.
    pub fn run_round_now(&mut self, id: &str, env: Env<'_>) -> Result<(Round, Task, Vec<(u64, Change)>), RoundError> {
        let row = self.round_row(id)?;
        if let Some(why) = self.overlap(&row)? {
            return Err(RoundError::Busy(format!("{why} Run it again when that is done.")));
        }
        if let Some((_, Some(why))) = self.blocker(&row) {
            return Err(RoundError::Refused(why));
        }
        let task = self.plan_round_task(&row, None)?;
        let line = RoundRun { id: self.new_id("rrun"), round: row.id.clone(), due_ms: None, at_ms: env.now, kind: RunKind::Started, task: Some(task.id.clone()), reason: None, end: None };
        let mut row = row;
        row.changed_ms = env.now;
        let seqs = self.store.commit_due(&row, &[line], Some(&task), env.now)?;
        let view = self.round_view(&row, env)?;
        let events = vec![(seqs[0], Change::TaskCreated { task: task.clone() }), (seqs[1], Change::RoundUpdated { round: view.clone() })];
        Ok((view, task, events))
    }

    /// Deals with every due time up to `env.now`, settles the tasks that ended, and says when to look again. See the module
    /// documentation for what dealing with a time means.
    pub fn tick_rounds(&mut self, env: Env<'_>) -> Result<Tick, RoundError> {
        let mut tick = Tick { events: self.settle_rounds(env)?, ..Tick::default() };
        let paused = self.store.rounds_paused()?;
        for row in self.store.round_rows()? {
            if !row.enabled || !row.schedule.is_automatic() {
                continue;
            }
            let dues = row.schedule.due_between(row.anchor_ms, row.cursor_ms, env.now, env.zone, 20_000);
            let Some(&latest) = dues.last() else { continue };
            let earlier = dues.len() - 1;
            let mut row = row;
            row.cursor_ms = latest;
            row.changed_ms = env.now;
            let mut lines = Vec::new();
            let line = |core: &mut Core, due: i64, kind: RunKind, reason: Option<String>| RoundRun { id: core.new_id("rrun"), round: row.id.clone(), due_ms: Some(due), at_ms: env.now, kind, task: None, reason, end: None };
            let mut task = None;
            if env.now - latest > CATCH_UP_MS {
                let n = dues.len();
                let why = if n == 1 { "Neko was not running, or this Mac was asleep".to_owned() } else { format!("{n} times passed while Neko was not running or this Mac was asleep") };
                lines.push(line(self, latest, RunKind::Missed, Some(why)));
            } else {
                if earlier > 0 {
                    let why = format!("{} passed while Neko was not running or this Mac was asleep; it ran once, for the latest", times(earlier));
                    lines.push(line(self, dues[0], RunKind::Missed, Some(why)));
                }
                let skip = if paused {
                    Some("Every round was paused.".to_owned())
                } else if let Some((_, why)) = self.blocker(&row) {
                    why
                } else {
                    self.overlap(&row)?
                };
                match skip {
                    Some(why) => lines.push(line(self, latest, RunKind::Skipped, Some(why))),
                    None => match self.plan_round_task(&row, Some(latest)) {
                        Ok(t) => {
                            let mut l = line(self, latest, RunKind::Started, None);
                            l.task = Some(t.id.clone());
                            lines.push(l);
                            task = Some(t);
                        }
                        Err(e) => lines.push(line(self, latest, RunKind::Skipped, Some(e.to_string()))),
                    },
                }
            }
            let seqs = self.store.commit_due(&row, &lines, task.as_ref(), env.now)?;
            if let Some(t) = task {
                tick.events.push((seqs[0], Change::TaskCreated { task: t.clone() }));
                tick.started.push(t);
            }
            tick.events.push((*seqs.last().unwrap_or(&0), Change::RoundUpdated { round: self.round_view(&row, env)? }));
        }
        tick.wake_at_ms = self.next_wake(env)?;
        Ok(tick)
    }

    /// The soonest due time of any round that is on and automatic (a paused round counts: its time passes as a skipped line).
    pub fn next_wake(&self, env: Env<'_>) -> Result<Option<i64>, StoreError> {
        Ok(self.store.round_rows()?.iter().filter(|r| r.enabled).filter_map(|r| r.schedule.next_due(r.anchor_ms, r.cursor_ms, env.zone)).min())
    }

    /// Looks at the tasks of started runs that were still open: records how each ended, counts failures in a row, and turns a round
    /// off after [`BRAKE_AFTER`] of them, with a notice in the last task and a line on the round.
    pub fn settle_rounds(&mut self, env: Env<'_>) -> Result<Vec<(u64, Change)>, RoundError> {
        let mut events = Vec::new();
        for run in self.store.open_runs()? {
            let Some(task_id) = run.task.clone() else { continue };
            let task = self.store.get_task(&task_id)?;
            let end = match task.as_ref().map(|t| t.status) {
                None => RunEnd::Removed,
                Some(TaskStatus::Done) => RunEnd::Done,
                Some(TaskStatus::Asked) => RunEnd::Asked,
                Some(TaskStatus::Failed) => RunEnd::Failed,
                Some(TaskStatus::Stopped) => RunEnd::Stopped,
                Some(TaskStatus::Interrupted) => RunEnd::Interrupted,
                Some(_) => continue,
            };
            // The `asked` state ends a run's work but not its wait: the line closes once, as asked.
            self.store.conn.execute("UPDATE round_runs SET ended = ?2 WHERE id = ?1", params![run.id, crate::store::status_name(end)])?;
            let Some(mut row) = self.store.get_round_row(&run.round)? else { continue };
            match end {
                RunEnd::Failed => row.fail_streak += 1,
                RunEnd::Done | RunEnd::Asked => row.fail_streak = 0,
                _ => {}
            }
            let mut notice = None;
            // The brake is for rounds that start by themselves: a manual round only runs when the person presses Run now.
            if end == RunEnd::Failed && row.fail_streak >= BRAKE_AFTER && row.enabled && row.schedule.is_automatic() {
                let note = format!("Turned itself off after {BRAKE_AFTER} failed runs in a row. Turn it back on when it is fixed.");
                row.enabled = false;
                row.brake_note = Some(note.clone());
                notice = task.map(|t| (t, note));
            }
            row.changed_ms = env.now;
            let rev = self.store.write_round(&row, if notice.is_some() { "round_braked" } else { "round_settled" }, env.now)?;
            if let Some((t, note)) = notice {
                let outs = crate::conductor::standalone_notice(self, &t, Tone::Warning, format!("This round {}", note.replacen("Turned", "turned", 1)), Some("Fix what failed, then turn the round back on.".into()), None)?;
                for o in outs {
                    if let crate::conductor::Out::Change(rev, change) = o {
                        events.push((rev, change));
                    }
                }
            }
            events.push((rev, Change::RoundUpdated { round: self.round_view(&row, env)? }));
        }
        Ok(events)
    }
}
