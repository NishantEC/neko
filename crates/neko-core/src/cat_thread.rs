//! A cat's own thread, its wake queue and the rules that turn them into turns.
//!
//! The thread is `cat_messages` (append-only rows, never the Home chat's `neko_chat_v2` row), the queue is
//! `cat_wakes`. Nothing here reads the clock or starts an agent: callers pass `now_ms` in, and the daemon's
//! cat tick (`neko-daemon/src/workbench/cats.rs`) runs the turns. See `docs/knowledge/cats.md`.
use crate::cats;
use crate::db::Db;
use neko_protocol::cats::{CatMessage, CatMessageRole, CatThreadView, CatWake, WakeReason, MAX_SAY_CHARS, THREAD_PAGE};
use neko_protocol::goals::GoalStatus;
use neko_protocol::workbench::{Task, TaskStatus};
use std::collections::BTreeMap;

/// Pending wakes one cat keeps; the oldest go first.
pub const MAX_PENDING_WAKES: usize = 200;
/// The oldest pending wake must be this old before an unattended turn starts, so a burst becomes one turn.
pub const DEBOUNCE_MS: i64 = 20_000;
/// Unattended turns of one cat are at least this far apart.
pub const MIN_GAP_MS: i64 = 60_000;
/// Unattended turns of one cat per local day.
pub const TURNS_PER_DAY: usize = 40;
/// Messages a cat's prompt carries.
pub const PROMPT_HISTORY: usize = 30;
/// A stored message is cut here (a cat's reply; the person's are limited by `MAX_SAY_CHARS`).
pub const MAX_STORED_BYTES: usize = 32 * 1024;

const REV_KEY: &str = "neko_cats_rev_v1";
const SEEN_KEY: &str = "neko_cat_seen_v1:";

// ---------------------------------------------------------------------------
// The change signal
// ---------------------------------------------------------------------------

/// The counter `Snapshot::cats_rev` carries. It is a settings row (not a process-wide static), so two
/// databases never share it and a restart keeps it.
pub fn rev(db: &Db) -> Result<u64, String> {
    Ok(db.get_setting(REV_KEY).map_err(|e| e.to_string())?.and_then(|v| v.parse().ok()).unwrap_or(0))
}

/// Moves the counter. The write goes through `set_setting`, which bumps the daemon's data revision and the
/// load cache's stamp: a held `WorkbenchIfChanged` wakes, sees `cats_rev` changed in the snapshot's digest and
/// answers. Called on every thread write and when a turn starts or ends; never on a timer.
pub fn bump_rev(db: &Db) -> Result<(), String> {
    let next = rev(db)?.saturating_add(1);
    db.set_setting(REV_KEY, &next.to_string()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// The thread
// ---------------------------------------------------------------------------

fn message_of(cat_id: &str, (seq, role, text, at_ms): (u64, String, String, i64)) -> CatMessage {
    CatMessage { cat_id: cat_id.to_owned(), seq, role: CatMessageRole::parse(&role).unwrap_or(CatMessageRole::Event), text, at_ms }
}

fn cut(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

/// Adds one line to a cat's thread (the cat must exist; archived is fine for a reply already running).
/// Empty text is refused. Moves the change counter.
pub fn append(db: &Db, cat_id: &str, role: CatMessageRole, text: &str, now_ms: i64) -> Result<CatMessage, String> {
    cats::get(db, cat_id)?;
    let text = cut(text.trim(), MAX_STORED_BYTES);
    if text.is_empty() {
        return Err("A message cannot be empty".into());
    }
    let seq = db.append_cat_message(cat_id, role.as_str(), &text, now_ms).map_err(|e| e.to_string())?;
    bump_rev(db)?;
    Ok(CatMessage { cat_id: cat_id.to_owned(), seq, role, text, at_ms: now_ms })
}

/// The person speaks: the message is stored and a `PersonMessage` wake queued, together or not at all.
/// Archived cats do not take messages.
pub fn say(db: &Db, cat_id: &str, text: &str, now_ms: i64) -> Result<CatMessage, String> {
    let cat = cats::get(db, cat_id)?;
    if cat.archived {
        return Err(format!("{} is archived and does not take messages", cat.name));
    }
    let text = text.trim();
    if text.is_empty() {
        return Err("Write something to say".into());
    }
    if text.chars().count() > MAX_SAY_CHARS {
        return Err(format!("Keep it under {MAX_SAY_CHARS} characters"));
    }
    if text.contains('\0') {
        return Err("That text has a character Neko cannot keep".into());
    }
    db.atomic(|| {
        let message = append(db, cat_id, CatMessageRole::Person, text, now_ms)?;
        wake(db, cat_id, WakeReason::PersonMessage, "", now_ms)?;
        Ok(message)
    })
}

/// Messages with `seq > since_seq` (the newest page when `since_seq` is 0), oldest first, with the newest `seq`.
pub fn view(db: &Db, cat_id: &str, since_seq: u64, running: bool) -> Result<CatThreadView, String> {
    cats::get(db, cat_id)?;
    let rows = if since_seq == 0 { db.cat_messages_last(cat_id, THREAD_PAGE) } else { db.cat_messages_after(cat_id, since_seq, THREAD_PAGE) }.map_err(|e| e.to_string())?;
    Ok(CatThreadView {
        cat_id: cat_id.to_owned(),
        messages: rows.into_iter().map(|r| message_of(cat_id, r)).collect(),
        last_seq: db.cat_last_seq(cat_id).map_err(|e| e.to_string())?,
        running,
    })
}

/// The newest `n` messages, oldest first, for the cat's prompt.
pub fn recent(db: &Db, cat_id: &str, n: usize) -> Result<Vec<CatMessage>, String> {
    Ok(db.cat_messages_last(cat_id, n).map_err(|e| e.to_string())?.into_iter().map(|r| message_of(cat_id, r)).collect())
}

// ---------------------------------------------------------------------------
// The wake queue
// ---------------------------------------------------------------------------

/// Queues one wake. At most `MAX_PENDING_WAKES` wait per cat; the oldest are dropped first.
pub fn wake(db: &Db, cat_id: &str, reason: WakeReason, task_id: &str, now_ms: i64) -> Result<u64, String> {
    let id = db.push_cat_wake(cat_id, reason.as_str(), task_id, now_ms).map_err(|e| e.to_string())?;
    let waiting = db.cat_wakes_of(cat_id).map_err(|e| e.to_string())?;
    if waiting.len() > MAX_PENDING_WAKES {
        let keep_from = waiting[waiting.len() - MAX_PENDING_WAKES].0;
        db.delete_cat_wakes(cat_id, keep_from.saturating_sub(1)).map_err(|e| e.to_string())?;
    }
    Ok(id)
}

/// A cat's pending wakes in the order they were written.
pub fn pending(db: &Db, cat_id: &str) -> Result<Vec<CatWake>, String> {
    Ok(db.cat_wakes_of(cat_id).map_err(|e| e.to_string())?.into_iter()
        .filter_map(|(id, reason, task_id, at_ms)| Some(CatWake { id, cat_id: cat_id.to_owned(), reason: WakeReason::parse(&reason)?, task_id, at_ms }))
        .collect())
}

/// The cats with something pending.
pub fn cats_with_wakes(db: &Db) -> Result<Vec<String>, String> {
    db.cats_with_wakes().map_err(|e| e.to_string())
}

/// What one turn takes from the queue: every pending wake, coalesced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    /// The newest wake taken; `take` removes everything up to it.
    pub up_to_id: u64,
    /// The person wrote (the turn is theirs: it answers them and may change RULES.md and TASK_RULES.md).
    pub person: bool,
    /// What the sessions did, one entry per (reason, session), in order of first appearance.
    pub events: Vec<(WakeReason, String)>,
    pub oldest_ms: i64,
    /// Wakes folded into this turn.
    pub count: usize,
}

/// Folds the pending wakes into one batch (`None` when there are none).
pub fn coalesce(wakes: &[CatWake]) -> Option<Batch> {
    let last = wakes.last()?;
    let mut events: Vec<(WakeReason, String)> = Vec::new();
    for wake in wakes.iter().filter(|w| w.reason != WakeReason::PersonMessage) {
        let entry = (wake.reason, wake.task_id.clone());
        if !events.contains(&entry) {
            events.push(entry);
        }
    }
    Some(Batch { up_to_id: last.id, person: wakes.iter().any(|w| w.reason == WakeReason::PersonMessage), events, oldest_ms: wakes.iter().map(|w| w.at_ms).min().unwrap_or(0), count: wakes.len() })
}

/// Removes the wakes a turn took. Wakes written while it ran (higher ids) stay for the next turn.
pub fn take(db: &Db, cat_id: &str, batch: &Batch) -> Result<(), String> {
    db.delete_cat_wakes(cat_id, batch.up_to_id).map(|_| ()).map_err(|e| e.to_string())
}

/// Drops every pending wake of a cat (it was archived).
pub fn drop_all(db: &Db, cat_id: &str) -> Result<(), String> {
    db.delete_cat_wakes(cat_id, u64::MAX >> 1).map(|_| ()).map_err(|e| e.to_string())
}

/// Why an unattended batch is not launched yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wait {
    /// The oldest wake is younger than `DEBOUNCE_MS`: a burst may still be growing.
    Debounce,
    /// The last turn was less than `MIN_GAP_MS` ago.
    TooSoon,
    /// `TURNS_PER_DAY` turns have run today.
    DayCap,
}

/// The launch rule for the queue itself (the machine steward and the spend ceiling are checked by the caller).
/// A batch with the person's message in it always passes: the person is never debounced or capped.
pub fn launch_gate(batch: &Batch, now_ms: i64, last_turn_ms: Option<i64>, turns_today: usize) -> Result<(), Wait> {
    if batch.person {
        return Ok(());
    }
    if now_ms - batch.oldest_ms < DEBOUNCE_MS {
        return Err(Wait::Debounce);
    }
    if last_turn_ms.is_some_and(|last| now_ms - last < MIN_GAP_MS) {
        return Err(Wait::TooSoon);
    }
    if turns_today >= TURNS_PER_DAY {
        return Err(Wait::DayCap);
    }
    Ok(())
}

/// When the cat's last turn was recorded and how many were recorded since local midnight: both read from
/// the usage ledger's `cat:<id>` lines (R3), so no second counter exists to drift.
pub fn turn_stats(db: &Db, cat_id: &str, now_ms: i64) -> Result<(Option<i64>, usize), String> {
    let key = task_key(cat_id);
    let today = crate::spend::day_start_local(now_ms);
    let events = db.usage_events_for_task(&key).map_err(|e| e.to_string())?;
    Ok((events.iter().map(|e| e.at_ms).max(), events.iter().filter(|e| e.at_ms >= today).count()))
}

/// The ledger's `task_id` for a cat's own turns (R3): no migration, no column.
pub fn task_key(cat_id: &str) -> String {
    format!("cat:{cat_id}")
}

// ---------------------------------------------------------------------------
// Noticing what the sessions did
// ---------------------------------------------------------------------------

/// What the cat last saw of one session. Tool events and timestamps are not in it, so a chatty run never wakes anyone.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fingerprint {
    pub status: TaskStatus,
    /// Turns its goal has taken (0 without a goal).
    #[serde(default)]
    pub goal_turns: u32,
    /// Its goal is blocked on a question for the person.
    #[serde(default)]
    pub goal_blocked: bool,
}

pub fn fingerprint(task: &Task, goal: Option<&neko_protocol::goals::Goal>) -> Fingerprint {
    Fingerprint { status: task.status, goal_turns: goal.map_or(0, |g| g.turns), goal_blocked: goal.is_some_and(|g| g.status == GoalStatus::Blocked) }
}

fn working(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Queued | TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing)
}

/// What changed between two sightings of one session, as wake reasons. A session seen for the first time wakes
/// nobody (an upgrade, or a session just made, never storms the cat).
pub fn diff(old: Option<&Fingerprint>, new: &Fingerprint) -> Vec<WakeReason> {
    let Some(old) = old else { return Vec::new() };
    let mut reasons = Vec::new();
    if new.status == TaskStatus::Failed && old.status != TaskStatus::Failed {
        reasons.push(WakeReason::Failed);
    }
    if (new.status == TaskStatus::AwaitingApproval && old.status != TaskStatus::AwaitingApproval) || (new.goal_blocked && !old.goal_blocked) {
        reasons.push(WakeReason::NeedsYou);
    }
    let rested = matches!(new.status, TaskStatus::ReadyForReview | TaskStatus::Completed);
    if (working(old.status) && rested) || new.goal_turns > old.goal_turns {
        reasons.push(WakeReason::TurnFinished);
    }
    reasons
}

type Seen = BTreeMap<String, Fingerprint>;

fn load_seen(db: &Db, cat_id: &str) -> Result<Seen, String> {
    Ok(db.get_setting(&format!("{SEEN_KEY}{cat_id}")).map_err(|e| e.to_string())?.and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default())
}

fn save_seen(db: &Db, cat_id: &str, seen: &Seen) -> Result<(), String> {
    // Quiet: not part of the snapshot, so no reader wakes and nothing is rewritten when it did not change.
    db.set_setting_quiet(&format!("{SEEN_KEY}{cat_id}"), &serde_json::to_string(seen).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Notes a session the cat itself just made, so its first change counts as a change.
pub fn note_new_session(db: &Db, cat_id: &str, task: &Task) -> Result<(), String> {
    let mut seen = load_seen(db, cat_id)?;
    seen.insert(task.id.clone(), fingerprint(task, None));
    save_seen(db, cat_id, &seen)
}

/// Compares every session of the cat with what it last saw, queues a wake for each change and remembers
/// what it saw, in one transaction. Returns the wakes it queued. Sessions that are gone are forgotten.
pub fn notice_sessions(db: &Db, cat_id: &str, tasks: &[Task], goals: &[neko_protocol::goals::Goal], now_ms: i64) -> Result<Vec<(WakeReason, String)>, String> {
    let mine: Vec<&Task> = tasks.iter().filter(|t| t.cat_id.as_deref() == Some(cat_id)).collect();
    let before = load_seen(db, cat_id)?;
    let mut after = Seen::new();
    let mut wakes = Vec::new();
    for task in mine {
        let goal = goals.iter().find(|g| g.ticket_id.as_deref() == Some(task.id.as_str()));
        let now = fingerprint(task, goal);
        for reason in diff(before.get(&task.id), &now) {
            wakes.push((reason, task.id.clone()));
        }
        after.insert(task.id.clone(), now);
    }
    if wakes.is_empty() && after == before {
        return Ok(wakes);
    }
    db.atomic(|| {
        for (reason, task_id) in &wakes {
            wake(db, cat_id, *reason, task_id, now_ms)?;
        }
        save_seen(db, cat_id, &after)
    })?;
    Ok(wakes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cats::NewCat;
    use neko_protocol::cats::{FaceRef, ModelPolicy};
    use neko_protocol::workbench::Workspace;

    fn workspace() -> Workspace {
        Workspace { id: "w1".into(), name: "Acme web".into(), repository: "/tmp/acme-web".into(), instructions: String::new(), away_enabled: false }
    }

    fn cat(db: &Db, root: &std::path::Path, name: &str, id: &str) -> String {
        let input = NewCat { name: name.into(), face: FaceRef("black-glasses".into()), workspace_id: "w1".into(), area: "Web".into(), model_policy: ModelPolicy::default(), daily_limit_micro_usd: None, purpose: "Look after the web app.".into() };
        cats::create(db, root, &[workspace()], input, id.into(), 1).unwrap().id
    }

    fn task(id: &str, cat: Option<&str>, status: TaskStatus) -> Task {
        Task {
            cat_id: cat.map(str::to_owned),
            id: id.into(), workspace_id: "w1".into(), issue_id: None, title: "ENG-123 export".into(), goal: String::new(), status, plan: String::new(), result: String::new(),
            worktree: None, events: Vec::new(), created_at_ms: 1, updated_at_ms: 1, source_revision: None, supervision: None, base_commit: None, pull_request: None,
            landed_commit: None, display_title: None, parked: None, request: None,
        }
    }

    fn fixture() -> (Db, tempfile::TempDir, String) {
        let db = Db::open_in_memory().unwrap();
        let root = tempfile::tempdir().unwrap();
        let id = cat(&db, root.path(), "miso", "c1");
        (db, root, id)
    }

    fn wake_of(reason: WakeReason, task: &str, at_ms: i64) -> CatWake {
        CatWake { id: at_ms as u64, cat_id: "c1".into(), reason, task_id: task.into(), at_ms }
    }

    #[test]
    fn the_thread_is_append_only_and_numbers_from_one_per_cat() {
        let (db, root, miso) = fixture();
        let tofu = cat(&db, root.path(), "tofu", "c2");
        append(&db, &miso, CatMessageRole::Person, "  hello  ", 10).unwrap();
        append(&db, &miso, CatMessageRole::Cat, "hi", 11).unwrap();
        append(&db, &tofu, CatMessageRole::Person, "other", 12).unwrap();
        let all = view(&db, &miso, 0, false).unwrap();
        assert_eq!(all.messages.iter().map(|m| (m.seq, m.role, m.text.as_str())).collect::<Vec<_>>(), [(1, CatMessageRole::Person, "hello"), (2, CatMessageRole::Cat, "hi")]);
        assert_eq!((all.last_seq, all.running), (2, false));
        assert_eq!(view(&db, &miso, 1, true).unwrap().messages.len(), 1, "since_seq returns what is newer");
        assert_eq!(view(&db, &tofu, 0, false).unwrap().messages[0].seq, 1, "another cat starts at 1");
        assert!(append(&db, &miso, CatMessageRole::Cat, "   ", 13).unwrap_err().contains("empty"));
        assert!(view(&db, "nobody", 0, false).is_err());
    }

    #[test]
    fn a_page_of_the_thread_is_the_newest_two_hundred_and_a_reply_is_cut_at_32_kb() {
        let (db, _root, miso) = fixture();
        for n in 0..THREAD_PAGE + 5 {
            append(&db, &miso, CatMessageRole::Event, &format!("line {n}"), n as i64).unwrap();
        }
        let page = view(&db, &miso, 0, false).unwrap();
        assert_eq!((page.messages.len(), page.messages[0].seq, page.last_seq), (THREAD_PAGE, 6, 205));
        let long = append(&db, &miso, CatMessageRole::Cat, &"é".repeat(40_000), 1).unwrap();
        assert!(long.text.len() <= MAX_STORED_BYTES && long.text.chars().all(|c| c == 'é'));
    }

    #[test]
    fn a_person_message_is_stored_and_queues_one_wake_together() {
        let (db, _root, miso) = fixture();
        let stored = say(&db, &miso, "  split this into two sessions ", 100).unwrap();
        assert_eq!((stored.seq, stored.role, stored.text.as_str()), (1, CatMessageRole::Person, "split this into two sessions"));
        let wakes = pending(&db, &miso).unwrap();
        assert_eq!(wakes.iter().map(|w| (w.reason, w.task_id.as_str(), w.at_ms)).collect::<Vec<_>>(), [(WakeReason::PersonMessage, "", 100)]);
        assert!(say(&db, &miso, " ", 1).is_err() && say(&db, &miso, &"x".repeat(MAX_SAY_CHARS + 1), 1).is_err() && say(&db, &miso, "a\0b", 1).is_err());
        assert_eq!((view(&db, &miso, 0, false).unwrap().messages.len(), pending(&db, &miso).unwrap().len()), (1, 1), "a refused message stores nothing and wakes nobody");
        cats::archive(&db, &miso).unwrap();
        assert!(say(&db, &miso, "hello?", 2).unwrap_err().contains("archived"));
    }

    #[test]
    fn the_person_never_touches_the_home_chat_row() {
        let (db, _root, miso) = fixture();
        db.set_setting("neko_chat_v2", "[]").unwrap();
        say(&db, &miso, "hello", 1).unwrap();
        append(&db, &miso, CatMessageRole::Cat, "hi", 2).unwrap();
        assert_eq!(db.get_setting("neko_chat_v2").unwrap().as_deref(), Some("[]"));
        assert!(crate::workbench::load(&db).unwrap().conversation.is_empty());
    }

    #[test]
    fn every_thread_write_moves_the_counter_the_snapshot_carries_and_the_data_revision() {
        let (db, _root, miso) = fixture();
        let before = (crate::workbench::load(&db).unwrap().cats_rev, crate::db::data_revision());
        say(&db, &miso, "hello", 1).unwrap();
        let after_say = crate::workbench::load(&db).unwrap();
        assert!(after_say.cats_rev > before.0 && crate::db::data_revision() > before.1, "{before:?} -> {}", after_say.cats_rev);
        append(&db, &miso, CatMessageRole::Cat, "hi", 2).unwrap();
        assert!(crate::workbench::load(&db).unwrap().cats_rev > after_say.cats_rev);
        // It is read for readers and never written into the stored snapshot row.
        let stored = db.get_setting("workbench_snapshot_v1").unwrap().unwrap_or_default();
        assert!(!stored.contains("cats_rev"), "the counter lives in its own row");
    }

    #[test]
    fn three_wakes_coalesce_into_one_batch_and_taking_leaves_later_ones() {
        let (db, _root, miso) = fixture();
        wake(&db, &miso, WakeReason::TurnFinished, "t1", 100).unwrap();
        wake(&db, &miso, WakeReason::Failed, "t2", 105).unwrap();
        wake(&db, &miso, WakeReason::TurnFinished, "t1", 110).unwrap();
        let batch = coalesce(&pending(&db, &miso).unwrap()).unwrap();
        assert_eq!(batch.events, [(WakeReason::TurnFinished, "t1".to_owned()), (WakeReason::Failed, "t2".to_owned())], "a repeat folds into the first");
        assert_eq!((batch.count, batch.person, batch.oldest_ms), (3, false, 100));
        wake(&db, &miso, WakeReason::NeedsYou, "t3", 120).unwrap();
        take(&db, &miso, &batch).unwrap();
        assert_eq!(pending(&db, &miso).unwrap().iter().map(|w| w.task_id.as_str()).collect::<Vec<_>>(), ["t3"], "a wake written while the turn ran waits for the next one");
        assert!(coalesce(&[]).is_none());
        assert!(coalesce(&[wake_of(WakeReason::PersonMessage, "", 5)]).unwrap().person);
    }

    #[test]
    fn the_queue_keeps_the_newest_two_hundred_wakes() {
        let (db, _root, miso) = fixture();
        for n in 0..MAX_PENDING_WAKES + 7 {
            wake(&db, &miso, WakeReason::TurnFinished, &format!("t{n}"), n as i64).unwrap();
        }
        let waiting = pending(&db, &miso).unwrap();
        assert_eq!((waiting.len(), waiting[0].task_id.as_str()), (MAX_PENDING_WAKES, "t7"));
        drop_all(&db, &miso).unwrap();
        assert!(pending(&db, &miso).unwrap().is_empty() && cats_with_wakes(&db).unwrap().is_empty());
    }

    #[test]
    fn an_unattended_batch_waits_for_the_debounce_the_gap_and_the_day_cap_and_the_person_never_does() {
        let batch = coalesce(&[wake_of(WakeReason::TurnFinished, "t1", 1_000)]).unwrap();
        assert_eq!(launch_gate(&batch, 1_000 + DEBOUNCE_MS - 1, None, 0), Err(Wait::Debounce));
        assert_eq!(launch_gate(&batch, 1_000 + DEBOUNCE_MS, None, 0), Ok(()));
        let late = 1_000 + DEBOUNCE_MS + 5 * MIN_GAP_MS;
        assert_eq!(launch_gate(&batch, late, Some(late - MIN_GAP_MS + 1), 0), Err(Wait::TooSoon));
        assert_eq!(launch_gate(&batch, late, Some(late - MIN_GAP_MS), 0), Ok(()));
        assert_eq!(launch_gate(&batch, late, None, TURNS_PER_DAY), Err(Wait::DayCap));
        let person = coalesce(&[wake_of(WakeReason::PersonMessage, "", 1_000)]).unwrap();
        assert_eq!(launch_gate(&person, 1_000, Some(999), TURNS_PER_DAY), Ok(()));
    }

    #[test]
    fn diff_wakes_for_a_finished_turn_a_failure_and_a_question_and_for_nothing_else() {
        let f = |status, goal_turns, goal_blocked| Fingerprint { status, goal_turns, goal_blocked };
        use TaskStatus::*;
        assert_eq!(diff(None, &f(Failed, 0, false)), [], "a session first seen wakes nobody");
        assert_eq!(diff(Some(&f(Building, 0, false)), &f(ReadyForReview, 0, false)), [WakeReason::TurnFinished]);
        assert_eq!(diff(Some(&f(Reviewing, 0, false)), &f(Completed, 0, false)), [WakeReason::TurnFinished]);
        assert_eq!(diff(Some(&f(Building, 0, false)), &f(Failed, 0, false)), [WakeReason::Failed]);
        assert_eq!(diff(Some(&f(Planning, 0, false)), &f(AwaitingApproval, 0, false)), [WakeReason::NeedsYou]);
        assert_eq!(diff(Some(&f(Building, 1, false)), &f(Building, 1, true)), [WakeReason::NeedsYou], "a goal that blocks on a question");
        assert_eq!(diff(Some(&f(Building, 1, false)), &f(Building, 2, false)), [WakeReason::TurnFinished], "a goal turn ended");
        assert_eq!(diff(Some(&f(Building, 0, false)), &f(Building, 0, false)), []);
        assert_eq!(diff(Some(&f(Failed, 0, false)), &f(Failed, 0, false)), [], "already failed is not news");
        assert_eq!(diff(Some(&f(ReadyForReview, 0, false)), &f(Completed, 0, false)), [], "rest to rest is not a turn");
    }

    #[test]
    fn noticing_queues_wakes_for_my_sessions_only_and_a_first_sighting_or_my_own_new_session_is_quiet() {
        let (db, root, miso) = fixture();
        let tofu = cat(&db, root.path(), "tofu", "c2");
        let mut tasks = vec![task("mine", Some(&miso), TaskStatus::Building), task("theirs", Some(&tofu), TaskStatus::Building), task("loose", None, TaskStatus::Building)];
        assert!(notice_sessions(&db, &miso, &tasks, &[], 10).unwrap().is_empty(), "first sighting");
        note_new_session(&db, &tofu, &tasks[1]).unwrap();
        tasks[0].status = TaskStatus::Failed;
        tasks[1].status = TaskStatus::ReadyForReview;
        tasks[2].status = TaskStatus::Failed;
        assert_eq!(notice_sessions(&db, &miso, &tasks, &[], 20).unwrap(), [(WakeReason::Failed, "mine".to_owned())]);
        assert!(notice_sessions(&db, &miso, &tasks, &[], 30).unwrap().is_empty(), "noticed once");
        assert_eq!(pending(&db, &miso).unwrap().len(), 1);
        assert_eq!(notice_sessions(&db, &tofu, &tasks, &[], 40).unwrap(), [(WakeReason::TurnFinished, "theirs".to_owned())], "a session its cat made counts from its creation");
        assert!(pending(&db, &miso).unwrap().iter().all(|w| w.task_id == "mine"));
    }

    #[test]
    fn a_database_from_before_the_thread_gains_the_tables_and_keeps_its_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("neko.db");
        {
            let db = Db::open(&path).unwrap();
            db.set_setting("some_setting", "kept").unwrap();
        }
        rusqlite::Connection::open(&path).unwrap().execute_batch("DROP TABLE cat_messages; DROP TABLE cat_wakes;").unwrap();
        let db = Db::open(&path).unwrap();
        assert_eq!(db.get_setting("some_setting").unwrap().as_deref(), Some("kept"));
        let root = tempfile::tempdir().unwrap();
        let id = cat(&db, root.path(), "miso", "c1");
        say(&db, &id, "hello", 1).unwrap();
        drop(db);
        let again = Db::open(&path).unwrap();
        assert_eq!((view(&again, &id, 0, false).unwrap().messages.len(), pending(&again, &id).unwrap().len()), (1, 1), "and both survive a reopen");
    }

    #[test]
    fn cat_turns_are_ledger_lines_under_the_cat_key_and_count_toward_the_daily_ceiling() {
        let (db, _root, miso) = fixture();
        let day = 1_800_000_000_000_i64;
        crate::usage_ledger::record_cat_turn(&db, &miso, day, day + 5_000, "claude", "sonnet", 2_500_000);
        crate::usage_ledger::record_cat_turn(&db, &miso, day, day + 5_000, "claude", "sonnet", 2_500_000);
        let lines = db.usage_events_for_task("cat:c1").unwrap();
        assert_eq!(lines.len(), 1, "one line per turn: the run key repeats");
        assert_eq!((lines[0].kind.as_str(), lines[0].task_id.as_str(), lines[0].cost_micro_usd, lines[0].duration_ms), ("cat", "cat:c1", Some(2_500_000), 5_000));
        assert_eq!(turn_stats(&db, &miso, day + 6_000).unwrap().0, Some(day + 5_000));
        assert!((crate::spend::routines_usd_since(&db, day - 1) - 2.5).abs() < 1e-9, "a turn's cost is automation spend");
    }
    #[test]
    fn cat_id_round_trips_through_the_task_rows_and_a_task_from_before_it_loads_with_none() {
        let (db, _root, miso) = fixture();
        let mut snapshot = crate::workbench::load(&db).unwrap();
        snapshot.workspaces.push(workspace());
        snapshot.tasks = vec![task("mine", Some(&miso), TaskStatus::Queued), task("loose", None, TaskStatus::Queued)];
        for t in &mut snapshot.tasks {
            t.goal = "Fix the export".into();
        }
        crate::workbench::save(&db, &snapshot).unwrap();
        let loaded = crate::workbench::load(&db).unwrap();
        assert_eq!(loaded.tasks.iter().map(|t| t.cat_id.as_deref()).collect::<Vec<_>>(), [Some("c1"), None]);
        // None is not written, so a row from before cats and a row of a catless task are the same bytes.
        let loose = serde_json::to_string(&loaded.tasks[1]).unwrap();
        assert!(!loose.contains("cat_id"));
        assert_eq!(serde_json::from_str::<Task>(&loose).unwrap().cat_id, None);
        assert_eq!(serde_json::from_str::<Task>(&serde_json::to_string(&loaded.tasks[0]).unwrap()).unwrap().cat_id.as_deref(), Some("c1"));
    }
}
