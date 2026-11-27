//! The rows a running task writes (docs/knowledge/transcript.md): runs, the parts of a transcript, permission requests, each run's
//! usage, where a task works, and the settings. Every change appends one event row in the same transaction, as tasks do.

use neko_protocol::{
    Changes, Part, PartBody, PartId, Permission, PermissionId, PermissionMode, PermissionState, Pricing, Run, RunId, RunStatus, Settings, Task, TaskId, TaskStatus, Usage, UsageTotal,
};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::store::{Store, StoreError, status_name, status_of};

/// Where a task works, once its first run has prepared it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Place {
    /// The folder the agent runs in.
    pub workdir: String,
    /// Its branch, in a worktree.
    pub branch: Option<String>,
    /// The commit the branch started from.
    pub base_commit: Option<String>,
    /// The repository the worktree belongs to.
    pub repo: Option<String>,
    /// A chosen folder that is not a repository: edits there ask first.
    pub plain: bool,
}

/// Appends one event row in `tx` (the same transaction as the change it records).
pub(crate) fn log_event(tx: &Transaction<'_>, now_ms: i64, subject: &str, kind: &str, data: serde_json::Value) -> Result<u64, StoreError> {
    log(tx, now_ms, subject, kind, data)
}

fn log(tx: &Transaction<'_>, now_ms: i64, subject: &str, kind: &str, data: serde_json::Value) -> Result<u64, StoreError> {
    tx.execute(
        "INSERT INTO events (at_ms, workspace_id, subject, kind, data) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![now_ms, crate::DEFAULT_WORKSPACE, subject, kind, data.to_string()],
    )?;
    Ok(tx.last_insert_rowid() as u64)
}

const RUN_COLUMNS: &str = "r.id, r.task_id, r.runtime, r.model, r.status, r.started_ms, r.ended_ms, r.session, r.workdir, r.branch, r.reason, r.files, r.added, r.removed, \
    u.input_tokens, u.output_tokens, u.cached_tokens, u.cost_usd, u.model, u.pricing, u.reported_by, u.context_tokens, u.context_window, \
    r.undo_state, r.undo_why, r.undo_files, r.undo_added, r.undo_removed, r.undo_committed, r.cat_id, r.rules_hash";

fn run_of(row: &rusqlite::Row<'_>) -> rusqlite::Result<Run> {
    let changes = match (row.get::<_, Option<u32>>(11)?, row.get::<_, Option<u32>>(12)?, row.get::<_, Option<u32>>(13)?) {
        (Some(files), Some(added), Some(removed)) => Some(Changes { files, added, removed }),
        _ => None,
    };
    let usage = match row.get::<_, Option<i64>>(14)? {
        Some(input) => Some(Usage {
            input_tokens: input as u64,
            output_tokens: row.get::<_, i64>(15)? as u64,
            cached_tokens: row.get::<_, i64>(16)? as u64,
            cost_usd: row.get(17)?,
            model: row.get(18)?,
            pricing: status_of::<Pricing>(&row.get::<_, String>(19)?)?,
            reported_by: row.get(20)?,
            context_tokens: row.get::<_, Option<i64>>(21)?.map(|n| n as u64),
            context_window: row.get::<_, Option<i64>>(22)?.map(|n| n as u64),
        }),
        None => None,
    };
    Ok(Run {
        id: RunId(row.get(0)?),
        task: TaskId(row.get(1)?),
        runtime: row.get(2)?,
        model: row.get(3)?,
        status: status_of(&row.get::<_, String>(4)?)?,
        started_ms: row.get(5)?,
        ended_ms: row.get(6)?,
        session: row.get(7)?,
        workdir: row.get(8)?,
        branch: row.get(9)?,
        reason: row.get(10)?,
        usage,
        changes,
        undo: crate::history::undo_of(row, 23)?,
        cat: row.get(29)?,
        rules_hash: row.get(30)?,
    })
}

fn part_of(row: &rusqlite::Row<'_>) -> rusqlite::Result<Part> {
    let body: String = row.get(4)?;
    Ok(Part {
        id: PartId(row.get(0)?),
        task: TaskId(row.get(1)?),
        run: row.get::<_, Option<String>>(2)?.map(RunId),
        seq: row.get::<_, i64>(3)? as u64,
        body: serde_json::from_str(&body).map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e)))?,
        at_ms: row.get(5)?,
        ended_ms: row.get(6)?,
    })
}

/// The agent's words in a part are what a row's second line shows and what makes a thread unread: the task keeps the last line of
/// the newest of them and the time of the newest (in the part's own transaction, so a restart loses none).
fn note_words(tx: &Transaction<'_>, p: &Part, now_ms: i64) -> rusqlite::Result<()> {
    if let PartBody::Agent { text, .. } = &p.body {
        let line = crate::thread::latest_line(text);
        if !line.is_empty() {
            tx.execute("UPDATE tasks SET latest_line = ?2, activity_ms = MAX(activity_ms, ?3) WHERE id = ?1", params![p.task.0, line, now_ms])?;
        }
    }
    Ok(())
}

const PART_COLUMNS: &str = "id, task_id, run_id, seq, body, at_ms, ended_ms";

fn kind_of(b: &PartBody) -> &'static str {
    match b {
        PartBody::Person { .. } => "person",
        PartBody::Agent { .. } => "agent",
        PartBody::Thinking { .. } => "thinking",
        PartBody::Tool { .. } => "tool",
        PartBody::Permission { .. } => "permission",
        PartBody::Notice { .. } => "notice",
        PartBody::RulesChanged { .. } => "rules_changed",
    }
}

fn permission_of(row: &rusqlite::Row<'_>) -> rusqlite::Result<Permission> {
    Ok(Permission {
        id: PermissionId(row.get(0)?),
        task: TaskId(row.get(1)?),
        run: RunId(row.get(2)?),
        tool: row.get(3)?,
        title: row.get(4)?,
        why: row.get(5)?,
        detail: row.get(6)?,
        destructive: row.get::<_, i64>(7)? != 0,
        state: status_of(&row.get::<_, String>(8)?)?,
        asked_ms: row.get(9)?,
        answered_ms: row.get(10)?,
    })
}

const PERMISSION_COLUMNS: &str = "id, task_id, run_id, tool, title, why, detail, destructive, state, asked_ms, answered_ms";

impl Store {
    /// The newest error notice of a task: why its last attempt failed.
    pub fn newest_error_notice(&self, task: &TaskId) -> Result<Option<Part>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {PART_COLUMNS} FROM parts WHERE task_id = ?1 AND kind = 'notice' ORDER BY seq DESC"))?;
        let mut rows = st.query_map([&task.0], part_of)?;
        while let Some(p) = rows.next().transpose()? {
            if matches!(p.body, PartBody::Notice { tone: neko_protocol::Tone::Error, .. }) {
                return Ok(Some(p));
            }
        }
        Ok(None)
    }

    /// The person opened the task (`read`), or asked for it to be unread again: it is read as of `now_ms`, or one tick before its
    /// last activity. Returns the task whole with its event.
    pub fn set_read(&mut self, id: &TaskId, read: bool, now_ms: i64) -> Result<(Task, u64), StoreError> {
        let tx = self.conn.transaction()?;
        if read {
            tx.execute("UPDATE tasks SET last_read_ms = MAX(last_read_ms, ?2) WHERE id = ?1", params![id.0, now_ms])?;
        } else {
            tx.execute("UPDATE tasks SET last_read_ms = MAX(activity_ms - 1, 0) WHERE id = ?1", [&id.0])?;
        }
        let seq = log(&tx, now_ms, &id.0, if read { "task_read" } else { "task_unread" }, serde_json::json!({}))?;
        tx.commit()?;
        let task = self.get_task(id)?.ok_or(StoreError::Sqlite(rusqlite::Error::QueryReturnedNoRows))?;
        Ok((task, seq))
    }

    /// A task's status, with its event.
    pub fn set_task_status(&mut self, id: &TaskId, status: TaskStatus, now_ms: i64) -> Result<(Task, u64), StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute("UPDATE tasks SET status = ?2 WHERE id = ?1", params![id.0, status_name(status)])?;
        // A run that ended, a question or a request is something that happened in the thread; a stop is the person's own doing, and
        // a run starting is too.
        if matches!(status, TaskStatus::Done | TaskStatus::Failed | TaskStatus::Asked | TaskStatus::NeedsYou | TaskStatus::Interrupted) {
            tx.execute("UPDATE tasks SET activity_ms = MAX(activity_ms, ?2) WHERE id = ?1", params![id.0, now_ms])?;
        }
        let seq = log(&tx, now_ms, &id.0, "task_status", serde_json::json!({ "status": status_name(status) }))?;
        tx.commit()?;
        let task = self.get_task(id)?.ok_or(StoreError::Sqlite(rusqlite::Error::QueryReturnedNoRows))?;
        Ok((task, seq))
    }

    /// Where the task works, if a run has prepared it.
    pub fn place(&self, id: &TaskId) -> Result<Option<Place>, StoreError> {
        let row = self
            .conn
            .query_row("SELECT workdir, branch, base_commit, repo, plain FROM tasks WHERE id = ?1", [&id.0], |r| {
                Ok(r.get::<_, Option<String>>(0)?.map(|workdir| -> rusqlite::Result<Place> {
                    Ok(Place { workdir, branch: r.get(1)?, base_commit: r.get(2)?, repo: r.get(3)?, plain: r.get::<_, i64>(4)? != 0 })
                }))
            })
            .optional()?;
        Ok(row.flatten().transpose()?)
    }

    pub fn set_place(&mut self, id: &TaskId, p: &Place, now_ms: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE tasks SET workdir = ?2, branch = ?3, base_commit = ?4, repo = ?5, plain = ?6 WHERE id = ?1",
            params![id.0, p.workdir, p.branch, p.base_commit, p.repo, p.plain as i64],
        )?;
        let seq = log(&tx, now_ms, &id.0, "task_place", serde_json::json!({ "branch": p.branch }))?;
        tx.commit()?;
        Ok(seq)
    }

    /// Stores a new run.
    pub fn insert_run(&mut self, r: &Run, now_ms: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO runs (id, task_id, runtime, model, status, started_ms, ended_ms, session, workdir, branch, reason, cat_id, rules_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![r.id.0, r.task.0, r.runtime, r.model, status_name(r.status), r.started_ms, r.ended_ms, r.session, r.workdir, r.branch, r.reason, r.cat, r.rules_hash],
        )?;
        let seq = log(&tx, now_ms, &r.id.0, "run_started", serde_json::json!({ "task": r.task.0, "runtime": r.runtime }))?;
        tx.commit()?;
        Ok(seq)
    }

    /// Writes a run's changing fields: model, status, end, session, reason, changes and usage.
    pub fn update_run(&mut self, r: &Run, now_ms: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        let (files, added, removed) = r.changes.map_or((None, None, None), |c| (Some(c.files), Some(c.added), Some(c.removed)));
        tx.execute(
            "UPDATE runs SET model = ?2, status = ?3, ended_ms = ?4, session = ?5, reason = ?6, files = ?7, added = ?8, removed = ?9 WHERE id = ?1",
            params![r.id.0, r.model, status_name(r.status), r.ended_ms, r.session, r.reason, files, added, removed],
        )?;
        if let Some(u) = &r.usage {
            tx.execute(
                "INSERT INTO usage (run_id, input_tokens, output_tokens, cached_tokens, cost_usd, model, pricing, reported_by, context_tokens, context_window) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(run_id) DO UPDATE SET input_tokens = ?2, output_tokens = ?3, cached_tokens = ?4, cost_usd = ?5, model = ?6, pricing = ?7, reported_by = ?8, context_tokens = ?9, context_window = ?10",
                params![
                    r.id.0,
                    u.input_tokens as i64,
                    u.output_tokens as i64,
                    u.cached_tokens as i64,
                    u.cost_usd,
                    u.model,
                    status_name(u.pricing),
                    u.reported_by,
                    u.context_tokens.map(|n| n as i64),
                    u.context_window.map(|n| n as i64)
                ],
            )?;
        }
        let seq = log(&tx, now_ms, &r.id.0, "run_changed", serde_json::json!({ "status": status_name(r.status) }))?;
        tx.commit()?;
        Ok(seq)
    }

    pub fn get_run(&self, id: &RunId) -> Result<Option<Run>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {RUN_COLUMNS} FROM runs r LEFT JOIN usage u ON u.run_id = r.id WHERE r.id = ?1"))?;
        Ok(st.query_row([&id.0], run_of).optional()?)
    }

    /// A task's runs, oldest first.
    pub fn runs_of(&self, task: &TaskId) -> Result<Vec<Run>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {RUN_COLUMNS} FROM runs r LEFT JOIN usage u ON u.run_id = r.id WHERE r.task_id = ?1 ORDER BY r.started_ms, r.id"))?;
        let rows = st.query_map([&task.0], run_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The next place in a task's transcript.
    pub fn next_part_seq(&self, task: &TaskId) -> Result<u64, StoreError> {
        Ok(self.conn.query_row("SELECT COALESCE(MAX(seq), 0) + 1 FROM parts WHERE task_id = ?1", [&task.0], |r| r.get::<_, i64>(0))? as u64)
    }

    pub fn insert_part(&mut self, p: &Part, now_ms: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            &format!("INSERT INTO parts ({PART_COLUMNS}, kind) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"),
            params![p.id.0, p.task.0, p.run.as_ref().map(|r| &r.0), p.seq as i64, serde_json::to_string(&p.body).unwrap_or_default(), p.at_ms, p.ended_ms, kind_of(&p.body)],
        )?;
        note_words(&tx, p, now_ms)?;
        let seq = log(&tx, now_ms, &p.id.0, "part_added", serde_json::json!({ "task": p.task.0, "kind": kind_of(&p.body) }))?;
        tx.commit()?;
        Ok(seq)
    }

    /// Rewrites a part whole (its words grew, it ended, its tool finished). One row, one event.
    pub fn update_part(&mut self, p: &Part, now_ms: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute("UPDATE parts SET body = ?2, ended_ms = ?3 WHERE id = ?1", params![p.id.0, serde_json::to_string(&p.body).unwrap_or_default(), p.ended_ms])?;
        note_words(&tx, p, now_ms)?;
        let seq = log(&tx, now_ms, &p.id.0, "part_changed", serde_json::json!({ "bytes": p.body.text().len() }))?;
        tx.commit()?;
        Ok(seq)
    }

    /// A task's transcript, in order.
    pub fn parts_of(&self, task: &TaskId) -> Result<Vec<Part>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {PART_COLUMNS} FROM parts WHERE task_id = ?1 ORDER BY seq"))?;
        let rows = st.query_map([&task.0], part_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Stores a permission request with the rule an "always" answer would allow.
    pub fn insert_permission(&mut self, p: &Permission, rule: &str, now_ms: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            &format!("INSERT INTO permissions ({PERMISSION_COLUMNS}, rule) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"),
            params![p.id.0, p.task.0, p.run.0, p.tool, p.title, p.why, p.detail, p.destructive as i64, status_name(p.state), p.asked_ms, p.answered_ms, rule],
        )?;
        let seq = log(&tx, now_ms, &p.id.0, "permission_asked", serde_json::json!({ "task": p.task.0, "tool": p.tool }))?;
        tx.commit()?;
        Ok(seq)
    }

    /// Moves a pending request to `state`. Refused (None) when it is no longer pending: a request is answered once.
    pub fn settle_permission(&mut self, id: &PermissionId, state: PermissionState, now_ms: i64) -> Result<Option<(Permission, u64)>, StoreError> {
        let tx = self.conn.transaction()?;
        let changed = tx.execute("UPDATE permissions SET state = ?2, answered_ms = ?3 WHERE id = ?1 AND state = 'pending'", params![id.0, status_name(state), now_ms])?;
        if changed == 0 {
            return Ok(None);
        }
        let seq = log(&tx, now_ms, &id.0, "permission_settled", serde_json::json!({ "state": status_name(state) }))?;
        tx.commit()?;
        Ok(self.get_permission(id)?.map(|p| (p, seq)))
    }

    pub fn get_permission(&self, id: &PermissionId) -> Result<Option<Permission>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {PERMISSION_COLUMNS} FROM permissions WHERE id = ?1"))?;
        Ok(st.query_row([&id.0], permission_of).optional()?)
    }

    pub fn permissions_of(&self, task: &TaskId) -> Result<Vec<Permission>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {PERMISSION_COLUMNS} FROM permissions WHERE task_id = ?1 ORDER BY asked_ms, id"))?;
        let rows = st.query_map([&task.0], permission_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The run's requests still waiting.
    pub fn pending_of_run(&self, run: &RunId) -> Result<Vec<Permission>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {PERMISSION_COLUMNS} FROM permissions WHERE run_id = ?1 AND state = 'pending' ORDER BY asked_ms"))?;
        let rows = st.query_map([&run.0], permission_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Every request still waiting, in any task, oldest first ("Needs you").
    pub fn pending_permissions(&self) -> Result<Vec<Permission>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {PERMISSION_COLUMNS} FROM permissions WHERE state = 'pending' ORDER BY asked_ms, id"))?;
        let rows = st.query_map([], permission_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The rules the person answered "Always allow" for, in this task only.
    pub fn always_rules(&self, task: &TaskId) -> Result<Vec<String>, StoreError> {
        let mut st = self.conn.prepare_cached("SELECT rule FROM permissions WHERE task_id = ?1 AND state = 'always_allowed'")?;
        let rows = st.query_map([&task.0], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The settings, with their defaults for what was never set.
    pub fn settings(&self) -> Result<Settings, StoreError> {
        let get = |k: &str| self.conn.query_row("SELECT value FROM settings WHERE key = ?1", [k], |r| r.get::<_, String>(0)).optional();
        let runtime = get("runtime")?.unwrap_or_else(|| "claude".to_owned());
        let permission_mode = get("permission_mode")?.and_then(|m| status_of::<PermissionMode>(&m).ok()).unwrap_or_default();
        Ok(Settings { runtime, permission_mode })
    }

    pub fn set_settings(&mut self, s: &Settings, now_ms: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        for (k, v) in [("runtime", s.runtime.clone()), ("permission_mode", status_name(s.permission_mode))] {
            tx.execute("INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2", params![k, v])?;
        }
        let seq = log(&tx, now_ms, "settings", "settings_changed", serde_json::json!({ "runtime": s.runtime, "permission_mode": status_name(s.permission_mode) }))?;
        tx.commit()?;
        Ok(seq)
    }

    /// What the runs started since `from_ms` used, as reported.
    pub fn day_usage(&self, from_ms: i64) -> Result<UsageTotal, StoreError> {
        let mut st = self.conn.prepare_cached("SELECT u.input_tokens, u.output_tokens, u.cost_usd, u.pricing FROM usage u JOIN runs r ON r.id = u.run_id WHERE r.started_ms >= ?1")?;
        let mut total = UsageTotal::default();
        let mut rows = st.query([from_ms])?;
        while let Some(r) = rows.next()? {
            total.runs += 1;
            total.input_tokens += r.get::<_, i64>(0)? as u64;
            total.output_tokens += r.get::<_, i64>(1)? as u64;
            if let Some(c) = r.get::<_, Option<f64>>(2)? {
                total.cost_usd = Some(total.cost_usd.unwrap_or(0.0) + c);
                if r.get::<_, String>(3)? != "charged" {
                    total.api_equivalent = true;
                }
            }
        }
        Ok(total)
    }

    /// After a daemon restart: every run still marked running was cut off. It becomes interrupted, its task too, its pending
    /// requests expire and its open parts close, keeping every word written. Returns the tasks touched.
    pub fn interrupt_leftovers(&mut self, now_ms: i64) -> Result<Vec<TaskId>, StoreError> {
        let mut st = self.conn.prepare("SELECT id, task_id FROM runs WHERE status = 'running'")?;
        let left: Vec<(String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        drop(st);
        let mut tasks = Vec::new();
        for (run, task) in left {
            let tx = self.conn.transaction()?;
            tx.execute(
                "UPDATE runs SET status = 'interrupted', ended_ms = COALESCE(ended_ms, ?2), reason = COALESCE(reason, 'Neko stopped while this was running.') WHERE id = ?1",
                params![run, now_ms],
            )?;
            tx.execute("UPDATE tasks SET status = 'interrupted', activity_ms = MAX(activity_ms, ?2) WHERE id = ?1", params![task, now_ms])?;
            tx.execute("UPDATE permissions SET state = 'expired', answered_ms = ?2 WHERE run_id = ?1 AND state = 'pending'", params![run, now_ms])?;
            let open: Vec<(String, String)> = {
                let mut q = tx.prepare("SELECT id, body FROM parts WHERE run_id = ?1 AND ended_ms IS NULL")?;
                q.query_map([&run], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?
            };
            for (id, body) in open {
                let Ok(mut b) = serde_json::from_str::<PartBody>(&body) else { continue };
                match &mut b {
                    PartBody::Agent { done, .. } | PartBody::Thinking { done, .. } => *done = true,
                    PartBody::Tool { state, .. } if *state == neko_protocol::ToolState::Running => *state = neko_protocol::ToolState::Failed,
                    _ => {}
                }
                tx.execute("UPDATE parts SET body = ?2, ended_ms = ?3 WHERE id = ?1", params![id, serde_json::to_string(&b).unwrap_or_default(), now_ms])?;
            }
            log(&tx, now_ms, &run, "run_interrupted", serde_json::json!({ "task": task }))?;
            tx.commit()?;
            tasks.push(TaskId(task));
        }
        Ok(tasks)
    }

    /// Runs still marked running.
    pub fn running_runs(&self) -> Result<Vec<RunId>, StoreError> {
        let mut st = self.conn.prepare("SELECT id FROM runs WHERE status = ?1")?;
        let rows = st.query_map([status_name(RunStatus::Running)], |r| Ok(RunId(r.get(0)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
