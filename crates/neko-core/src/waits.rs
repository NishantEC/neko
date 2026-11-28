//! Timers and watches (docs/knowledge/waits.md): the records, the limits, reading the agent's times and paths, and the words the agent
//! is told. A thread's agent sets a timer (`wake_me`) or a watch (`watch_files`, `watch_command`) and ends its turn; the thread rests
//! as Waiting; the daemon's scheduler thread fires it and the thread is woken with `--resume`, through the same wakes as a kitten's
//! report (`subagents.rs`). Nothing here reads a clock, watches a file or starts a process: the daemon does (`crates/neko-daemon/src/waits.rs`).

use neko_protocol::schedule::{self, Zone};
use neko_protocol::waits::{self as limits, PATHS_MAX, TIMER_MAX_MS, TIMER_MIN_MS, TIMERS_PER_THREAD, WATCH_MAX_MS, WATCHES_IN_ALL, WATCHES_PER_THREAD};
use neko_protocol::{Task, TaskId, Wait, WaitKind};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::Core;
use crate::records::log_event;
use crate::store::{Store, StoreError};
use crate::subagents::{WakeKind, WakeRow};

/// How often a command watch runs its check when the agent does not say: every five minutes.
pub const EVERY_DEFAULT_MS: i64 = 5 * 60_000;
/// The most often a command watch may run its check: once a minute.
pub const EVERY_MIN_MS: i64 = 60_000;
/// The least often: once a day.
pub const EVERY_MAX_MS: i64 = limits::DAY_MS;
/// How long a command watch lasts when the agent does not say: a day. A files watch lasts the longest a watch may.
pub const COMMAND_UNTIL_DEFAULT_MS: i64 = limits::DAY_MS;
/// The longest one check may run before it is stopped (it counts as not yet).
pub const CHECK_TIMEOUT_MS: u64 = 60_000;
/// A files watch wakes its thread at most this often; changes in between go in the next wake.
pub const FILES_GAP_MS: i64 = 60_000;
/// The longest note a timer or watch keeps, in characters.
pub const NOTE_CHARS: usize = 500;
/// The longest check command, in bytes.
pub const COMMAND_BYTES: usize = 2_000;

/// A timer or watch as stored: what a screen sees, and the thread it belongs to, the folder its paths are read from, and its state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaitRow {
    pub wait: Wait,
    pub task: TaskId,
    /// The folder a files watch reads its paths against (the thread's folder when it was set); a command runs there.
    pub root: Option<String>,
    /// `active`, then `fired` (a timer; a command that succeeded), `ended` (a week passed), `stopped` or `skipped` (it came due while
    /// automatic work was paused).
    pub state: String,
    /// When a files watch last woke its thread.
    pub fired_ms: Option<i64>,
}

const COLUMNS: &str = "id, task_id, kind, note, at_ms, until_ms, paths, root, command, every_ms, state, created_ms, checked_ms, last_exit, fired_ms";

fn kind_name(k: WaitKind) -> &'static str {
    match k {
        WaitKind::Timer => "timer",
        WaitKind::Files => "files",
        WaitKind::Command => "command",
    }
}

fn row_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<WaitRow> {
    let kind = match r.get::<_, String>(2)?.as_str() {
        "files" => WaitKind::Files,
        "command" => WaitKind::Command,
        _ => WaitKind::Timer,
    };
    Ok(WaitRow {
        wait: Wait {
            id: r.get(0)?,
            kind,
            note: r.get(3)?,
            at_ms: r.get(4)?,
            until_ms: r.get(5)?,
            paths: r.get::<_, Option<String>>(6)?.and_then(|p| serde_json::from_str(&p).ok()).unwrap_or_default(),
            command: r.get(8)?,
            every_ms: r.get(9)?,
            checked_ms: r.get(12)?,
            last_exit: r.get(13)?,
            created_ms: r.get(11)?,
        },
        task: TaskId(r.get(1)?),
        root: r.get(7)?,
        state: r.get(10)?,
        fired_ms: r.get(14)?,
    })
}

impl Store {
    /// The timers and watches a thread still waits on, oldest first.
    pub fn waits_of(&self, task: &TaskId) -> Result<Vec<Wait>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {COLUMNS} FROM waits WHERE task_id = ?1 AND state = 'active' ORDER BY created_ms, id"))?;
        let rows = st.query_map([&task.0], row_of)?;
        Ok(rows.map(|r| r.map(|w| w.wait)).collect::<Result<_, _>>()?)
    }

    /// Every timer and watch still waiting, oldest first.
    pub fn active_waits(&self) -> Result<Vec<WaitRow>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {COLUMNS} FROM waits WHERE state = 'active' ORDER BY created_ms, id"))?;
        let rows = st.query_map([], row_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn wait_row(&self, id: &str) -> Result<Option<WaitRow>, StoreError> {
        Ok(self.conn.query_row(&format!("SELECT {COLUMNS} FROM waits WHERE id = ?1"), [id], row_of).optional()?)
    }

    /// Whether a thread waits on a timer or watch.
    pub fn has_waits(&self, task: &TaskId) -> Result<bool, StoreError> {
        Ok(self.conn.query_row("SELECT 1 FROM waits WHERE task_id = ?1 AND state = 'active' LIMIT 1", [&task.0], |_| Ok(())).optional()?.is_some())
    }

    fn insert_wait(&mut self, w: &WaitRow, now_ms: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        let v = &w.wait;
        tx.execute(
            &format!("INSERT INTO waits ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'active', ?11, NULL, NULL, NULL)"),
            params![v.id, w.task.0, kind_name(v.kind), v.note, v.at_ms, v.until_ms, (!v.paths.is_empty()).then(|| serde_json::to_string(&v.paths).unwrap_or_default()), w.root, v.command, v.every_ms, v.created_ms],
        )?;
        let seq = log_event(&tx, now_ms, &w.task.0, "wait_set", json!({ "wait": v.id, "kind": kind_name(v.kind), "at": v.at_ms, "until": v.until_ms }))?;
        tx.commit()?;
        Ok(seq)
    }

    /// Ends a timer or watch still waiting: `state` is how (`fired`, `ended`, `stopped`, `skipped`), `reason` why in words. Returns
    /// the event's number, or None when it had already ended.
    pub fn end_wait(&mut self, id: &str, state: &str, reason: Option<&str>, now_ms: i64) -> Result<Option<u64>, StoreError> {
        let tx = self.conn.transaction()?;
        let n = tx.execute("UPDATE waits SET state = ?2, ended_ms = ?3, end_reason = ?4 WHERE id = ?1 AND state = 'active'", params![id, state, now_ms, reason])?;
        if n == 0 {
            return Ok(None);
        }
        let task: String = tx.query_row("SELECT task_id FROM waits WHERE id = ?1", [id], |r| r.get(0))?;
        let seq = log_event(&tx, now_ms, &task, &format!("wait_{state}"), json!({ "wait": id, "reason": reason }))?;
        tx.commit()?;
        Ok(Some(seq))
    }

    /// Ends every timer and watch a thread waits on (it was stopped or archived). Returns the ids ended.
    pub fn end_waits_of(&mut self, task: &TaskId, state: &str, reason: &str, now_ms: i64) -> Result<Vec<String>, StoreError> {
        let ids: Vec<String> = self.waits_of(task)?.into_iter().map(|w| w.id).collect();
        for id in &ids {
            self.end_wait(id, state, Some(reason), now_ms)?;
        }
        Ok(ids)
    }

    /// A command watch ran its check: when, and how it exited (None: it did not finish in time).
    pub fn checked(&mut self, id: &str, at_ms: i64, exit: Option<i32>) -> Result<(), StoreError> {
        self.conn.execute("UPDATE waits SET checked_ms = ?2, last_exit = ?3 WHERE id = ?1", params![id, at_ms, exit])?;
        Ok(())
    }

    /// Removes the timers and watches of a thread being deleted.
    pub fn delete_waits_of_task(&mut self, task: &TaskId) -> Result<(), StoreError> {
        self.conn.execute("DELETE FROM waits WHERE task_id = ?1", [&task.0])?;
        Ok(())
    }

    fn count_active(&self, task: Option<&TaskId>, timers: bool) -> Result<usize, StoreError> {
        let kind = if timers { "kind = 'timer'" } else { "kind != 'timer'" };
        let n: i64 = match task {
            Some(t) => self.conn.query_row(&format!("SELECT COUNT(*) FROM waits WHERE state = 'active' AND {kind} AND task_id = ?1"), [&t.0], |r| r.get(0))?,
            None => self.conn.query_row(&format!("SELECT COUNT(*) FROM waits WHERE state = 'active' AND {kind}"), [], |r| r.get(0))?,
        };
        Ok(n as usize)
    }

    /// A timer or watch fires: the wake that carries it is stored, pending, and in the same transaction a timer (and a command that
    /// succeeded, and a watch that ended) is settled, while a files watch stays and notes when it woke its thread.
    pub fn fire_wait(&mut self, w: &WaitRow, wake: &WakeRow, ends: Option<&str>, now_ms: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        match ends {
            Some(state) => {
                tx.execute("UPDATE waits SET state = ?2, ended_ms = ?3, fired_ms = ?3 WHERE id = ?1 AND state = 'active'", params![w.wait.id, state, now_ms])?;
            }
            None => {
                tx.execute("UPDATE waits SET fired_ms = ?2 WHERE id = ?1", params![w.wait.id, now_ms])?;
            }
        }
        crate::subagents::insert_wake_in(&tx, wake, "pending")?;
        let seq = log_event(&tx, now_ms, &w.task.0, "wait_fired", json!({ "wait": w.wait.id, "ends": ends }))?;
        tx.commit()?;
        Ok(seq)
    }
}

/// The ways a timer or watch asked for is refused, in words the agent reads.
pub type Refusal = String;

/// "2h", "45m", "90s", "3d", "1h30m": a length of time. A bare number is refused, so a unit is never guessed.
pub fn parse_after(s: &str) -> Result<i64, Refusal> {
    let s = s.trim().to_lowercase().replace(' ', "");
    let bad = || format!("\"{s}\" is not a length of time. Say it as 90s, 45m, 2h, 1h30m or 3d.");
    if s.is_empty() {
        return Err(bad());
    }
    let mut total: i64 = 0;
    let mut num = String::new();
    let mut any = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        let n: i64 = num.parse().map_err(|_| bad())?;
        num.clear();
        let unit = match c {
            's' => 1_000,
            'm' => 60_000,
            'h' => 3_600_000,
            'd' => limits::DAY_MS,
            _ => return Err(bad()),
        };
        total = total.saturating_add(n.saturating_mul(unit));
        any = true;
    }
    if !num.is_empty() || !any {
        return Err(bad());
    }
    Ok(total)
}

/// A time the agent named, as an instant: "18:00" (the next time the Mac's clock reads it, today or tomorrow), "2026-11-29 18:00" or
/// "2026-11-29T18:00" (local), or with a zone: "2026-11-29T18:00:00Z", "…+05:30".
pub fn parse_at(s: &str, now_ms: i64, zone: &dyn Zone) -> Result<i64, Refusal> {
    let t = s.trim();
    let bad = || format!("\"{t}\" is not a time Neko reads. Say it as 18:00, or 2026-11-29 18:00, or give after instead (2h).");
    if let Some(minutes) = schedule::parse_at(t) {
        let at = format!("{:02}:{:02}", minutes / 60, minutes % 60);
        return neko_protocol::Schedule::Daily { at }.next_due(now_ms, now_ms, zone).ok_or_else(bad);
    }
    let (date, rest) = t.split_once(['T', ' ']).ok_or_else(bad)?;
    let mut d = date.split('-');
    let (Some(y), Some(mo), Some(da), None) = (d.next(), d.next(), d.next(), d.next()) else { return Err(bad()) };
    let (y, mo, da): (i64, u32, u32) = (y.parse().map_err(|_| bad())?, mo.parse().map_err(|_| bad())?, da.parse().map_err(|_| bad())?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&da) {
        return Err(bad());
    }
    // The zone, if one is written: Z, or ±HH:MM.
    let (clock, offset) = if let Some(c) = rest.strip_suffix(['Z', 'z']) {
        (c, Some(0))
    } else if let Some(i) = rest.rfind(['+', '-']).filter(|&i| i >= 4) {
        let (c, z) = rest.split_at(i);
        let sign = if z.starts_with('-') { -1 } else { 1 };
        let m = schedule::parse_at(&z[1..]).ok_or_else(bad)? as i64;
        (c, Some(sign * m * 60))
    } else {
        (rest, None)
    };
    let hm = clock.get(..5).ok_or_else(bad)?;
    let minutes = schedule::parse_at(hm).ok_or_else(bad)? as i64;
    let secs: i64 = match clock.get(5..) {
        None | Some("") => 0,
        Some(x) => x.strip_prefix(':').and_then(|x| x.get(..2)).and_then(|x| x.parse().ok()).ok_or_else(bad)?,
    };
    let local = schedule::days_from_civil(y, mo, da) * 86_400 + minutes * 60 + secs;
    let utc = match offset {
        Some(off) => local - off,
        None => schedule::local_to_utc(zone, local),
    };
    Ok(utc * 1000)
}

/// When a timer asked for with `at` or `after` wakes, checked against the limits.
pub fn timer_due(at: Option<&str>, after: Option<&str>, now_ms: i64, zone: &dyn Zone) -> Result<i64, Refusal> {
    let due = match (at.map(str::trim).filter(|s| !s.is_empty()), after.map(str::trim).filter(|s| !s.is_empty())) {
        (Some(_), Some(_)) => return Err("Give at or after, not both.".into()),
        (Some(a), None) => parse_at(a, now_ms, zone)?,
        (None, Some(a)) => now_ms + parse_after(a)?,
        (None, None) => return Err("wake_me needs at (a time, 18:00) or after (a length of time, 2h).".into()),
    };
    if due - now_ms < TIMER_MIN_MS {
        return Err("That is less than a minute away (or already past). Wait inside your turn instead, or set a later time.".into());
    }
    if due - now_ms > TIMER_MAX_MS {
        return Err("Neko wakes a thread at most 7 days ahead. Set a sooner time; you can set another timer then.".into());
    }
    Ok(due)
}

/// The paths a files watch was given, as the agent wrote them, checked: relative to the thread's folder (an absolute path inside it is
/// made relative), never outside it, at most [`PATHS_MAX`].
pub fn clean_paths(paths: &[String], root: &str) -> Result<Vec<String>, Refusal> {
    let root = root.trim_end_matches('/');
    let mut out = Vec::new();
    for p in paths {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        let rel = if let Some(abs) = p.strip_prefix('/') {
            let inside = abs.strip_prefix(root.trim_start_matches('/')).filter(|r| r.is_empty() || r.starts_with('/'));
            match inside {
                Some(r) => r.trim_start_matches('/').to_owned(),
                None => return Err(format!("{p} is outside this thread's folder ({root}). Watch files inside it.")),
            }
        } else {
            p.trim_start_matches("./").to_owned()
        };
        if rel.split('/').any(|seg| seg == "..") {
            return Err(format!("{p} leaves this thread's folder. Watch files inside it."));
        }
        let rel = if rel.is_empty() { "**".to_owned() } else { rel };
        if !out.contains(&rel) {
            out.push(rel);
        }
    }
    if out.is_empty() {
        return Err("watch_files needs at least one path or pattern (src/**, build.log).".into());
    }
    if out.len() > PATHS_MAX {
        return Err(format!("A watch takes at most {PATHS_MAX} paths. Use a pattern (src/**) instead."));
    }
    Ok(out)
}

fn note_of(note: Option<&str>) -> String {
    note.unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ").chars().take(NOTE_CHARS).collect()
}

/// What a watch was asked for.
#[derive(Clone, Debug)]
pub enum WatchAsk {
    Files { paths: Vec<String> },
    Command { command: String, every_ms: i64 },
}

impl Core {
    fn may_wait(&self, task: &TaskId) -> Result<Task, Refusal> {
        let t = self.store.get_task(task).map_err(|e| e.to_string())?.ok_or("This thread is gone.")?;
        if t.subagent.is_some() {
            return Err("A subagent cannot set timers or watches: end your turn with your report, and the agent that started you decides when to look again.".into());
        }
        if t.archived {
            return Err("This thread is archived, so nothing wakes it.".into());
        }
        Ok(t)
    }

    /// A timer on `task` that wakes it at `at_ms`. Refused past the limits, in words the agent reads.
    pub fn set_timer(&mut self, task: &TaskId, at_ms: i64, note: Option<&str>, now_ms: i64) -> Result<(Wait, u64), Refusal> {
        self.may_wait(task)?;
        if self.store.count_active(Some(task), true).map_err(|e| e.to_string())? >= TIMERS_PER_THREAD {
            return Err(format!("This thread has {TIMERS_PER_THREAD} timers set, the most Neko keeps for one. Stop one with stop_watching first."));
        }
        let wait = Wait { id: self.new_id("wait"), kind: WaitKind::Timer, note: note_of(note), at_ms: Some(at_ms), until_ms: None, paths: Vec::new(), command: None, every_ms: None, checked_ms: None, last_exit: None, created_ms: now_ms };
        let row = WaitRow { wait: wait.clone(), task: task.clone(), root: None, state: "active".into(), fired_ms: None };
        let rev = self.store.insert_wait(&row, now_ms).map_err(|e| e.to_string())?;
        Ok((wait, rev))
    }

    /// A watch on `task`, read against `root` (the thread's folder), lasting until `until_ms`. Refused past the limits.
    pub fn set_watch(&mut self, task: &TaskId, root: &str, ask: WatchAsk, until_ms: i64, note: Option<&str>, now_ms: i64) -> Result<(Wait, u64), Refusal> {
        self.may_wait(task)?;
        if self.store.count_active(Some(task), false).map_err(|e| e.to_string())? >= WATCHES_PER_THREAD {
            return Err(format!("This thread has {WATCHES_PER_THREAD} watches, the most Neko keeps for one. Stop one with stop_watching first."));
        }
        if self.store.count_active(None, false).map_err(|e| e.to_string())? >= WATCHES_IN_ALL {
            return Err(format!("Neko keeps at most {WATCHES_IN_ALL} watches in all, and has that many. Tell the person; they can stop some."));
        }
        if until_ms - now_ms > WATCH_MAX_MS {
            return Err("A watch lasts at most 7 days. Give a sooner until.".into());
        }
        if until_ms - now_ms < TIMER_MIN_MS {
            return Err("That until is less than a minute away. Give a later one.".into());
        }
        let (kind, paths, command, every_ms) = match ask {
            WatchAsk::Files { paths } => (WaitKind::Files, paths, None, None),
            WatchAsk::Command { command, every_ms } => (WaitKind::Command, Vec::new(), Some(command), Some(every_ms)),
        };
        let wait = Wait { id: self.new_id("wait"), kind, note: note_of(note), at_ms: None, until_ms: Some(until_ms), paths, command, every_ms, checked_ms: None, last_exit: None, created_ms: now_ms };
        let row = WaitRow { wait: wait.clone(), task: task.clone(), root: Some(root.to_owned()), state: "active".into(), fired_ms: None };
        let rev = self.store.insert_wait(&row, now_ms).map_err(|e| e.to_string())?;
        Ok((wait, rev))
    }
}

/// What the scheduler finds due among the timers and watches at `now`.
#[derive(Debug, Default)]
pub struct Due {
    /// Timers whose time came.
    pub timers: Vec<WaitRow>,
    /// Watches whose week (or `until`) is over.
    pub ended: Vec<WaitRow>,
    /// Command watches whose next check is due.
    pub checks: Vec<WaitRow>,
    /// The soonest time something else comes due (None: nothing will by itself).
    pub next_ms: Option<i64>,
}

/// When a command watch's next check is: `every` after the last one, or `every` after it was set.
pub fn next_check(w: &Wait) -> i64 {
    w.checked_ms.unwrap_or(w.created_ms) + w.every_ms.unwrap_or(EVERY_DEFAULT_MS)
}

/// Sorts what is waiting by what is due at `now`.
pub fn due(rows: Vec<WaitRow>, now_ms: i64) -> Due {
    let mut d = Due::default();
    let soonest = |t: i64, d: &mut Due| d.next_ms = Some(d.next_ms.map_or(t, |n| n.min(t)));
    for r in rows {
        match r.wait.kind {
            WaitKind::Timer => match r.wait.at_ms {
                Some(at) if at <= now_ms => d.timers.push(r),
                Some(at) => soonest(at, &mut d),
                None => {}
            },
            WaitKind::Files | WaitKind::Command => {
                let until = r.wait.until_ms.unwrap_or(i64::MAX);
                if until <= now_ms {
                    d.ended.push(r);
                    continue;
                }
                soonest(until, &mut d);
                if r.wait.kind == WaitKind::Command {
                    let next = next_check(&r.wait);
                    if next <= now_ms {
                        d.checks.push(r);
                    } else {
                        soonest(next, &mut d);
                    }
                }
            }
        }
    }
    d
}

/// "18:00" today, "tomorrow 09:00", "Tue 18:00", "29 Nov 18:00": a time near `now` in a few words, as the wakes and refusals say it.
pub fn when_words(at_ms: i64, now_ms: i64, zone: &dyn Zone) -> String {
    let a = schedule::local_parts(zone, at_ms);
    let n = schedule::local_parts(zone, now_ms);
    let hm = format!("{:02}:{:02}", a.hour, a.minute);
    match a.day_number - n.day_number {
        0 => hm,
        1 => format!("tomorrow {hm}"),
        2..=6 => format!("{} {hm}", a.weekday.short()),
        _ => format!("{}-{:02}-{:02} {hm}", a.year, a.month, a.day),
    }
}

/// The wake a timer, a watch or its end makes: for `task`, about the timer or watch, with what it carried.
pub fn wake_of(core: &mut Core, w: &WaitRow, kind: WakeKind, status: &str, files: Vec<String>, now_ms: i64) -> WakeRow {
    WakeRow {
        id: core.new_id("wake"),
        task: w.task.clone(),
        subagent: w.task.clone(),
        number: String::new(),
        title: w.wait.subject(),
        status: status.to_owned(),
        summary: w.wait.note.clone(),
        files,
        branch: None,
        created_ms: now_ms,
        run: None,
        question: None,
        chain: Vec::new(),
        kind,
        about: Some(w.wait.id.clone()),
        at_ms: w.wait.at_ms.or(w.wait.until_ms),
    }
}

// ---- What the agent is told ---------------------------------------------------------------------------------------------------

/// The paragraph about timers and watches, added to a thread's system text after the subagents paragraph. The same bytes every run.
pub const PARAGRAPH: &str = "When you must wait for something that takes a while (a build, a review, a time of day), do not wait inside your turn: set a timer with \
mcp__neko__wake_me (at a time, or after a length of time, up to 7 days) or a watch (mcp__neko__watch_files for files in your folder changing, mcp__neko__watch_command for a \
read-only check command that exits 0 once the thing is ready), then end your turn. Neko wakes you with a <neko-wake kind=\"timer\"> or <neko-wake kind=\"watch\"> message. \
A watch lasts at most 7 days. Stop a timer or watch you no longer need with mcp__neko__stop_watching. Tell the person in one line what you are waiting for and when.";

/// What `wake_me` answers.
pub fn timer_text(w: &Wait, when: &str) -> String {
    format!(
        "Timer {} is set: Neko wakes you at {when}. The thread rests as \"Waiting until {when}\". Do not poll or wait. Tell the person in one line what you will do then, and end your turn now.",
        w.id
    )
}

/// What `watch_files` and `watch_command` answer.
pub fn watch_text(w: &Wait, until: &str) -> String {
    let what = match w.kind {
        WaitKind::Files => format!("Neko wakes you when {} changes (at most once a minute; changes made while you work do not count)", w.paths.join(", ")),
        _ => format!("Neko runs `{}` every {} in your folder and wakes you when it exits 0", w.command.as_deref().unwrap_or(""), every_words(w.every_ms.unwrap_or(EVERY_DEFAULT_MS))),
    };
    format!("Watch {} is set: {what}. It ends by itself at {until}, and Neko wakes you then too. Stop it with stop_watching (id \"{}\"). Do not poll or wait. End your turn now.", w.id, w.id)
}

/// "5 minutes", "1 hour", "90 seconds".
pub fn every_words(ms: i64) -> String {
    let s = ms / 1000;
    let (n, unit) = if s % 3600 == 0 { (s / 3600, "hour") } else if s % 60 == 0 { (s / 60, "minute") } else { (s, "second") };
    if n == 1 { format!("1 {unit}") } else { format!("{n} {unit}s") }
}

/// The tools for timers and watches, as MCP's `tools/list` gives them. Offered to a thread's run, not a subagent's.
pub fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "wake_me",
            "description": "Sets a timer that wakes this thread later, up to 7 days ahead, after your turn has ended. Give at (a time: 18:00, or 2026-11-29 18:00) or after (a length of time: 45m, 2h, 1d). \
Returns at once. Then end your turn: Neko resumes you at that time with a <neko-wake kind=\"timer\"> message carrying your note. Do not poll or wait.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "at": {"type": "string", "description": "When to wake: 18:00 (the next time the clock reads it), or 2026-11-29 18:00 (local time)."},
                    "after": {"type": "string", "description": "How long from now: 90s, 45m, 2h, 1h30m, 3d."},
                    "note": {"type": "string", "description": "What to do then, in a line. It comes back to you in the wake."}
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "watch_files",
            "description": "Watches files in this thread's folder and wakes this thread when they change, after your turn has ended. Paths are relative to your folder; patterns may use * and **. \
Changes made while you are working do not wake you. Lasts until you stop it with stop_watching, or at most 7 days. Returns at once; then end your turn. Do not poll.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "paths": {"type": "array", "items": {"type": "string"}, "description": "Files, folders or patterns: build.log, out/, src/**/*.rs."},
                    "until": {"type": "string", "description": "When it ends by itself: a length of time (2d) or a time. Default and longest: 7 days."},
                    "note": {"type": "string", "description": "What to do when they change, in a line."}
                },
                "required": ["paths"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "watch_command",
            "description": "Runs a read-only check command in this thread's folder every few minutes and wakes this thread the first time it exits 0 (gh pr checks 12, test -f out/done). \
The person may be asked to allow it first. Each check may run for a minute. Returns at once; then end your turn. Do not poll.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "A shell command that only reads, and exits 0 when the thing is ready."},
                    "every": {"type": "string", "description": "How often to run it: 1m at the most often, 1d at the least. Default 5m."},
                    "until": {"type": "string", "description": "When it gives up: a length of time (6h) or a time. Default 1 day, at most 7 days."},
                    "note": {"type": "string", "description": "What to do when it succeeds, in a line."}
                },
                "required": ["command"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "stop_watching",
            "description": "Stops a timer or watch of this thread's by its id (as wake_me, watch_files or watch_command returned it).",
            "inputSchema": {
                "type": "object",
                "properties": { "id": {"type": "string", "description": "The timer's or watch's id (wait_…)."} },
                "required": ["id"],
                "additionalProperties": false
            }
        }),
    ]
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;")
}

fn attr(s: &str) -> String {
    escape(s).replace('"', "&quot;")
}

/// One timer's or watch's wake, for the agent. `when` reads the wake's time in words (the timer's time, the watch's end).
pub fn wake_element(w: &WakeRow, when: Option<String>) -> String {
    let id = w.about.as_deref().unwrap_or("");
    let at = when.map(|t| format!(" at=\"{}\"", attr(&t))).unwrap_or_default();
    let note = if w.summary.trim().is_empty() { String::new() } else { format!("<note>{}</note>\n", escape(&w.summary)) };
    match (w.kind, w.status.as_str()) {
        (WakeKind::Timer, _) => format!("<neko-wake kind=\"timer\" id=\"{id}\"{at}>\n{note}The timer you set is up. Carry on with what you set it for.\n</neko-wake>"),
        (_, "ended") => format!(
            "<neko-wake kind=\"watch\" id=\"{id}\" status=\"ended\" on=\"{}\"{at}>\n{note}This watch reached the end of its time with nothing to report, so Neko ended it. Set it again if you still need it.\n</neko-wake>",
            attr(&w.title)
        ),
        (_, "succeeded") => format!(
            "<neko-wake kind=\"watch\" id=\"{id}\" status=\"succeeded\" command=\"{}\">\n{note}<output>{}</output>\nThe check exited 0, so the watch is over.\n</neko-wake>",
            attr(&w.title),
            escape(&w.files.join("\n"))
        ),
        _ => format!(
            "<neko-wake kind=\"watch\" id=\"{id}\" status=\"changed\" paths=\"{}\">\n{note}<changed>{}</changed>\nThese changed since your turn ended. The watch goes on and wakes you at the next change; stop it with mcp__neko__stop_watching (id \"{id}\") when you no longer need it.\n</neko-wake>",
            attr(&w.title),
            escape(&w.files.join("; "))
        ),
    }
}

/// A standing round's words for its Cattain, as the message that wakes its thread.
pub fn round_element(w: &WakeRow, when: Option<String>) -> String {
    let at = when.map(|t| format!(" due=\"{}\"", attr(&t))).unwrap_or_default();
    format!(
        "<neko-wake kind=\"round\" round=\"{}\"{at}>\n{}\n</neko-wake>\nThis is your standing round, written by the person. Do it now, in this thread, then update DIGEST.md and NOTES.md and end your turn.",
        attr(&w.title),
        escape(w.summary.trim())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::schedule::Fixed;

    const NOW: i64 = 1_764_324_000_000; // 2025-11-28 10:00 UTC

    #[test]
    fn lengths_of_time_need_a_unit() {
        assert_eq!(parse_after("2h"), Ok(7_200_000));
        assert_eq!(parse_after("1h30m"), Ok(5_400_000));
        assert_eq!(parse_after(" 45 m "), Ok(2_700_000));
        assert_eq!(parse_after("3d"), Ok(3 * limits::DAY_MS));
        assert!(parse_after("90").is_err() && parse_after("soon").is_err() && parse_after("").is_err() && parse_after("2x").is_err());
    }

    #[test]
    fn a_time_is_the_next_time_the_clock_reads_it() {
        let z = Fixed(0);
        assert_eq!(parse_at("18:00", NOW, &z), Ok(NOW + 8 * 3_600_000), "later today");
        assert_eq!(parse_at("09:00", NOW, &z), Ok(NOW + 23 * 3_600_000), "tomorrow, the hour has passed");
        assert_eq!(parse_at("2025-11-29 18:00", NOW, &z), Ok(NOW + 32 * 3_600_000));
        assert_eq!(parse_at("2025-11-29T18:00", NOW, &Fixed(3600)), Ok(NOW + 31 * 3_600_000), "local time, an hour ahead of UTC");
        assert_eq!(parse_at("2025-11-29T18:00:00Z", NOW, &Fixed(3600)), Ok(NOW + 32 * 3_600_000), "a written zone wins");
        assert_eq!(parse_at("2025-11-29T18:00+05:30", NOW, &z), Ok(NOW + 32 * 3_600_000 - 5 * 3_600_000 - 1_800_000));
        assert!(parse_at("tomorrow", NOW, &z).is_err() && parse_at("25:00", NOW, &z).is_err());
    }

    #[test]
    fn a_timer_is_a_minute_to_a_week_away() {
        let z = Fixed(0);
        assert_eq!(timer_due(None, Some("2h"), NOW, &z), Ok(NOW + 7_200_000));
        assert!(timer_due(None, Some("30s"), NOW, &z).is_err(), "sooner than a minute");
        assert!(timer_due(None, Some("8d"), NOW, &z).is_err(), "past a week");
        assert!(timer_due(None, Some("7d"), NOW, &z).is_ok(), "a week exactly");
        assert!(timer_due(Some("18:00"), Some("2h"), NOW, &z).is_err() && timer_due(None, None, NOW, &z).is_err());
    }

    #[test]
    fn paths_stay_inside_the_folder() {
        let root = "/Users/example/acme";
        assert_eq!(clean_paths(&["src/**".into(), "./build.log".into(), "/Users/example/acme/out".into()], root), Ok(vec!["src/**".into(), "build.log".into(), "out".into()]));
        assert!(clean_paths(&["../other".into()], root).is_err());
        assert!(clean_paths(&["/etc/hosts".into()], root).is_err());
        assert!(clean_paths(&["/Users/example/acme-other/x".into()], root).is_err(), "a sibling with the same start is outside");
        assert!(clean_paths(&[" ".into()], root).is_err());
    }

    #[test]
    fn what_is_due_and_when_to_look_again() {
        let w = |id: &str, kind: WaitKind, at: Option<i64>, until: Option<i64>| WaitRow {
            wait: Wait { id: id.into(), kind, note: String::new(), at_ms: at, until_ms: until, paths: vec![], command: None, every_ms: Some(60_000), checked_ms: None, last_exit: None, created_ms: NOW - 120_000 },
            task: TaskId("t".into()),
            root: None,
            state: "active".into(),
            fired_ms: None,
        };
        let d = due(vec![w("a", WaitKind::Timer, Some(NOW), None), w("b", WaitKind::Timer, Some(NOW + 5_000), None), w("c", WaitKind::Files, None, Some(NOW - 1)), w("d", WaitKind::Command, None, Some(NOW + 9_000_000))], NOW);
        assert_eq!(d.timers.iter().map(|r| r.wait.id.as_str()).collect::<Vec<_>>(), ["a"]);
        assert_eq!(d.ended.iter().map(|r| r.wait.id.as_str()).collect::<Vec<_>>(), ["c"]);
        assert_eq!(d.checks.iter().map(|r| r.wait.id.as_str()).collect::<Vec<_>>(), ["d"], "a minute's check, set two minutes ago");
        assert_eq!(d.next_ms, Some(NOW + 5_000));
    }

    #[test]
    fn times_read_in_a_few_words() {
        let z = Fixed(0);
        assert_eq!(when_words(NOW + 8 * 3_600_000, NOW, &z), "18:00");
        assert_eq!(when_words(NOW + 23 * 3_600_000, NOW, &z), "tomorrow 09:00");
        assert_eq!(every_words(300_000), "5 minutes");
        assert_eq!(every_words(3_600_000), "1 hour");
    }

    #[test]
    fn the_tools_are_four_and_say_not_to_poll() {
        let t = tools();
        let names: Vec<&str> = t.iter().filter_map(|x| x["name"].as_str()).collect();
        assert_eq!(names, ["wake_me", "watch_files", "watch_command", "stop_watching"]);
        assert!(t.iter().take(3).all(|x| x["description"].as_str().unwrap().contains("Do not poll")));
    }
}
