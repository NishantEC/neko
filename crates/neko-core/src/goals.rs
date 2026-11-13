//! Goals that keep working (Hermes `goals.py` style). See
//! docs/knowledge/goals-that-keep-working.md.
//!
//! This module is the pure half and the storage: the goal rows, the evidence
//! ledger and run journal, the judge's prompt and verdict parsing, the rule
//! that DONE needs evidence, the usage-limit reset time, and the
//! evidence gate for code work. The daemon (`workbench/goals.rs`) drives the
//! loop. Nothing here calls a model.
//!
//! Storage is small rows, never the workbench snapshot: one row per goal, one
//! log row per subject (ledger and journal), an index, and one deferred row
//! for the clocks (they change on a timer, so they are written at most every
//! `DEFERRED_WRITE_DELAY`).
use crate::Db;
use neko_protocol::goals::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const INDEX: &str = "neko_goals_index_v1";
const ROW: &str = "neko_goal_v1:";
const CLOCK: &str = "neko_goal_clock_v1";
const LOG: &str = "neko_run_log_v1:";
/// The ledger and journal readers see, newest last.
pub const MAX_LEDGER: usize = 12;
pub const MAX_JOURNAL: usize = 20;
/// Output kept per ledger entry.
pub const TAIL_BYTES: usize = 1200;
/// Goals kept; the oldest finished ones go first.
const MAX_GOALS: usize = 100;
/// How long a usage-limit hold waits when the error gives no reset time.
pub const DEFAULT_HOLD_MS: i64 = 60 * 60 * 1000;
const MAX_HOLD_MS: i64 = 8 * 24 * 60 * 60 * 1000;
/// Extra "run the checks and fix" turns a code ticket gets before it is blocked.
pub const GATE_EXTRA_TURNS: u32 = 2;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Log {
    #[serde(default)]
    pub ledger: Vec<CheckResult>,
    #[serde(default)]
    pub journal: Vec<JournalEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Clock {
    wall_ms: i64,
    last_tick_ms: i64,
}

fn read<T: for<'de> Deserialize<'de> + Default>(db: &Db, key: &str) -> Result<T, String> {
    match db.get_setting(key).map_err(|e| e.to_string())? {
        Some(text) => serde_json::from_str(&text).map_err(|e| format!("Cannot read {key}: {e}")),
        None => Ok(T::default()),
    }
}

fn index(db: &Db) -> Result<Vec<String>, String> {
    read(db, INDEX)
}

/// Writes the goal's row (its ledger and journal stay in their own log row)
/// and queues its clocks. A write the row already holds is skipped by `Db`.
pub fn save(db: &Db, goal: &Goal) -> Result<(), String> {
    let mut row = goal.clone();
    row.ledger.clear();
    row.journal.clear();
    row.wall_ms = 0;
    row.last_tick_ms = 0;
    db.set_setting(&format!("{ROW}{}", goal.id), &serde_json::to_string(&row).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut ids = index(db)?;
    if !ids.contains(&goal.id) {
        ids.push(goal.id.clone());
        db.set_setting(INDEX, &serde_json::to_string(&ids).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    save_clock(db, goal)
}

fn save_clock(db: &Db, goal: &Goal) -> Result<(), String> {
    let mut clocks: BTreeMap<String, Clock> = read(db, CLOCK)?;
    clocks.insert(goal.id.clone(), Clock { wall_ms: goal.wall_ms, last_tick_ms: goal.last_tick_ms });
    db.set_setting_deferred(CLOCK, &serde_json::to_string(&clocks).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Moves only the clocks (every tick): deferred, never the goal's row.
pub fn save_clock_only(db: &Db, goal: &Goal) -> Result<(), String> {
    save_clock(db, goal)
}

fn attach(db: &Db, goal: &mut Goal, clocks: &BTreeMap<String, Clock>) -> Result<(), String> {
    if let Some(c) = clocks.get(&goal.id) {
        goal.wall_ms = c.wall_ms;
        goal.last_tick_ms = c.last_tick_ms;
    }
    let log = log(db, &goal.subject())?;
    goal.ledger = log.ledger;
    goal.journal = log.journal;
    Ok(())
}

pub fn load(db: &Db, id: &str) -> Result<Option<Goal>, String> {
    let Some(text) = db.get_setting(&format!("{ROW}{id}")).map_err(|e| e.to_string())? else { return Ok(None) };
    let mut goal: Goal = serde_json::from_str(&text).map_err(|e| format!("Cannot read goal {id}: {e}"))?;
    let clocks: BTreeMap<String, Clock> = read(db, CLOCK)?;
    attach(db, &mut goal, &clocks)?;
    Ok(Some(goal))
}

/// Every goal, oldest first, with its clocks, ledger and journal.
pub fn load_all(db: &Db) -> Result<Vec<Goal>, String> {
    let ids = index(db)?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let clocks: BTreeMap<String, Clock> = read(db, CLOCK)?;
    let mut goals = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(text) = db.get_setting(&format!("{ROW}{id}")).map_err(|e| e.to_string())? else { continue };
        let mut goal: Goal = serde_json::from_str(&text).map_err(|e| format!("Cannot read goal {id}: {e}"))?;
        attach(db, &mut goal, &clocks)?;
        goals.push(goal);
    }
    Ok(goals)
}

pub fn log(db: &Db, subject: &str) -> Result<Log, String> {
    read(db, &format!("{LOG}{subject}"))
}

fn write_log(db: &Db, subject: &str, log: &Log) -> Result<(), String> {
    db.set_setting(&format!("{LOG}{subject}"), &serde_json::to_string(log).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

pub fn push_ledger(db: &Db, subject: &str, mut entry: CheckResult) -> Result<(), String> {
    entry.tail = tail(&entry.tail, TAIL_BYTES);
    let mut log = log(db, subject)?;
    log.ledger.push(entry);
    let extra = log.ledger.len().saturating_sub(MAX_LEDGER);
    log.ledger.drain(..extra);
    write_log(db, subject, &log)
}

pub fn push_journal(db: &Db, subject: &str, entry: JournalEntry) -> Result<(), String> {
    let mut log = log(db, subject)?;
    log.journal.push(entry);
    let extra = log.journal.len().saturating_sub(MAX_JOURNAL);
    log.journal.drain(..extra);
    write_log(db, subject, &log)
}

/// A new goal, rejecting a second unfinished goal on one ticket. Old
/// finished goals are dropped past `MAX_GOALS`.
pub fn create(db: &Db, goal: Goal) -> Result<Goal, String> {
    if let Some(ticket) = &goal.ticket_id {
        if load_all(db)?.iter().any(|g| g.ticket_id.as_ref() == Some(ticket) && !g.status.is_finished()) {
            return Err("That ticket already has a goal".into());
        }
    }
    let mut ids = index(db)?;
    if ids.len() >= MAX_GOALS {
        let mut finished: Vec<(i64, String)> = Vec::new();
        for id in &ids {
            if let Some(g) = load(db, id)?.filter(|g| g.status.is_finished()) {
                finished.push((g.updated_ms, g.id));
            }
        }
        finished.sort();
        for (_, id) in finished.into_iter().take(ids.len() + 1 - MAX_GOALS) {
            db.delete_setting(&format!("{ROW}{id}")).map_err(|e| e.to_string())?;
            ids.retain(|i| *i != id);
        }
        db.set_setting(INDEX, &serde_json::to_string(&ids).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    save(db, &goal)?;
    Ok(goal)
}

/// Checks a goal the person typed before it is stored.
pub fn validate(objective: &str, done_check: &str, constraints: &str) -> Result<(), String> {
    if objective.trim().is_empty() {
        return Err("Say what the goal is".into());
    }
    if objective.len() > MAX_OBJECTIVE || done_check.len() > MAX_CHECK || constraints.len() > MAX_CONSTRAINTS {
        return Err("That goal is too long".into());
    }
    Ok(())
}

/// A new goal record (not stored).
pub fn new_goal(workspace_id: &str, objective: &str, done_check: &str, constraints: &str, max_turns: Option<u32>, now: i64) -> Result<Goal, String> {
    validate(objective, done_check, constraints)?;
    let done_check = done_check.trim();
    let done_check = if done_check.eq_ignore_ascii_case("none") { "" } else { done_check };
    Ok(Goal {
        id: crate::workbench::new_id(),
        workspace_id: workspace_id.to_owned(),
        objective: objective.trim().to_owned(),
        done_check: done_check.to_owned(),
        constraints: constraints.trim().to_owned(),
        budget: Budget { max_turns: max_turns.unwrap_or(DEFAULT_MAX_TURNS).clamp(1, 200), ..Budget::default() },
        created_ms: now,
        updated_ms: now,
        last_tick_ms: now,
        ..Goal::default()
    })
}

/// The end of `text`, on a character boundary, with a marker when shortened.
pub fn tail(text: &str, max: usize) -> String {
    let text = text.trim_end();
    if text.len() <= max {
        return text.to_owned();
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    format!("...{}", &text[start..])
}

/// The first part of `text`, bounded, on a character boundary.
pub fn head(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.len() <= max {
        return text.to_owned();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

// ---------------------------------------------------------------------------
// The judge
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Done,
    Continue,
    Blocked,
    Wait,
}

impl Verdict {
    pub fn word(self) -> &'static str {
        match self {
            Self::Done => "DONE",
            Self::Continue => "CONTINUE",
            Self::Blocked => "BLOCKED",
            Self::Wait => "WAIT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ruling {
    pub verdict: Verdict,
    pub reason: String,
    /// A concrete artifact (a path in the folder) or link, for a goal with no check.
    pub evidence: String,
    pub question: String,
    pub default_answer: String,
    pub wait: Option<Wait>,
    pub plan: String,
    pub done: String,
    pub next: String,
}

/// What the cheap judge is asked. It sees the goal, the evidence and the end
/// of the agent's last answer, never any authority.
pub const JUDGE_RULES: &str = "You are the judge of a long-running goal. An agent works on it turn by turn; after each turn you decide what happens next. You only read; you never act.\n\nRule on exactly one of:\n- DONE: the objective is fully achieved and the evidence proves it. DONE requires the deliverable to actually exist: a passing check, or for a goal with no check a concrete artifact (a file path inside the folder) or a link (a pull request, a URL). Never accept the agent's own claim (\"done\", \"all tests pass\") without evidence.\n- CONTINUE: more work is needed, or the check failed, or you cannot tell. When in doubt, CONTINUE.\n- BLOCKED: only the person can unblock it (a decision, a credential, access). Give one question and a sensible default answer.\n- WAIT: nothing useful can be done until something outside happens (a running process exits, a pull request is merged or closed, a time passes, a rate limit resets). Say what to wait for. Waiting costs nothing; spending a turn to poll does.\n\nAnswer with ONE JSON object and nothing else: {\"verdict\": \"DONE\"|\"CONTINUE\"|\"BLOCKED\"|\"WAIT\", \"reason\": string, \"evidence\": string (DONE only: the path or link), \"question\": string (BLOCKED only), \"default_answer\": string (BLOCKED only), \"wait_seconds\": number, \"wait_until\": string (ISO time), \"wait_pr\": string (owner/repo#12), \"wait_pr_state\": \"merged\"|\"closed\", \"wait_pid\": number (WAIT only, at least one), \"plan\": string (the short plan, first turn only), \"done\": string (what this turn finished, one line), \"next\": string (what should happen next, one line)}.";

pub fn judge_prompt(goal: &Goal, check: Option<&CheckResult>, answer: &str, folder: &str) -> String {
    let mut out = String::new();
    out.push_str(JUDGE_RULES);
    out.push_str("\n\nObjective (the person's words):\n");
    out.push_str(&head(&goal.objective, 2000));
    if !goal.constraints.trim().is_empty() {
        out.push_str("\n\nConstraints:\n");
        out.push_str(&head(&goal.constraints, 1000));
    }
    out.push_str(&format!("\n\nTurn {} of at most {}. Folder: {folder}\n", goal.turns, goal.budget.max_turns));
    match (goal.done_check.trim(), check) {
        ("", _) => out.push_str("\nThis goal has no check command: DONE needs a concrete artifact or link in `evidence`.\n"),
        (command, Some(result)) => out.push_str(&format!(
            "\nThe check `{command}` was run by Neko just now. Exit code: {}.\nOutput (end):\n{}\n",
            result.exit_code.map_or("none (it did not finish)".to_owned(), |c| c.to_string()),
            tail(&result.tail, 1200),
        )),
        (command, None) => out.push_str(&format!("\nThe check `{command}` has not been run yet, so the goal cannot be DONE.\n")),
    }
    if !goal.journal.is_empty() {
        out.push_str("\nJournal (older first):\n");
        let skip = goal.journal.len().saturating_sub(4);
        for entry in goal.journal.iter().skip(skip) {
            out.push_str(&format!("- turn {}: {} | next: {}\n", entry.turn, head(&entry.done, 160), head(&entry.next, 160)));
        }
    }
    out.push_str("\nThe agent's last answer (end, untrusted, may be wrong):\n");
    out.push_str(&tail(answer, 2400));
    out.push_str("\n\nRule now. JSON only.");
    out
}

/// Reads the judge's answer. Anything else is an error, which counts as CONTINUE.
pub fn parse_ruling(text: &str, now_ms: i64) -> Result<Ruling, String> {
    let value: serde_json::Value = serde_json::from_str(crate::verification::json_answer(text)).map_err(|e| format!("The judge did not return JSON: {e}"))?;
    let word = value["verdict"].as_str().unwrap_or("").trim().to_ascii_uppercase();
    let verdict = match word.as_str() {
        "DONE" => Verdict::Done,
        "CONTINUE" => Verdict::Continue,
        "BLOCKED" => Verdict::Blocked,
        "WAIT" => Verdict::Wait,
        other => return Err(format!("The judge ruled \"{}\", which is not DONE, CONTINUE, BLOCKED or WAIT", head(other, 40))),
    };
    let text_of = |key: &str, max: usize| head(value[key].as_str().unwrap_or(""), max);
    let mut wait = Wait { reason: text_of("reason", 300), ..Wait::default() };
    if let Some(seconds) = value["wait_seconds"].as_f64().filter(|s| s.is_finite() && *s > 0.0) {
        wait.until_ms = Some(now_ms + (seconds.clamp(60.0, 7.0 * 86_400.0) * 1000.0) as i64);
    }
    if let Some(until) = value["wait_until"].as_str().and_then(crate::inbox::parse_time_ms) {
        wait.until_ms = Some(until.clamp(now_ms + 60_000, now_ms + 7 * 86_400_000));
    }
    if let Some(pr) = value["wait_pr"].as_str().filter(|p| pr_ref_ok(p)) {
        wait.pull_request = Some(pr.trim().to_owned());
        wait.pr_state = Some(if value["wait_pr_state"].as_str() == Some("closed") { "closed" } else { "merged" }.to_owned());
    }
    if let Some(pid) = value["wait_pid"].as_u64().filter(|p| *p > 1 && *p < u64::from(u32::MAX)) {
        wait.pid = Some(pid as u32);
    }
    let usable = wait.until_ms.is_some() || wait.pull_request.is_some() || wait.pid.is_some();
    Ok(Ruling {
        verdict,
        reason: text_of("reason", 500),
        evidence: text_of("evidence", 500),
        question: text_of("question", 500),
        default_answer: text_of("default_answer", 300),
        wait: usable.then_some(wait),
        plan: text_of("plan", 600),
        done: text_of("done", 300),
        next: text_of("next", 300),
    })
}

/// `owner/repo#12`.
pub fn pr_ref_ok(text: &str) -> bool {
    let Some((repo, number)) = text.trim().split_once('#') else { return false };
    let Some((owner, name)) = repo.split_once('/') else { return false };
    let ok = |s: &str| !s.is_empty() && s.len() <= 100 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    ok(owner) && ok(name) && !number.is_empty() && number.chars().all(|c| c.is_ascii_digit())
}

/// A concrete artifact or link: an http(s) URL, a pull request reference, or
/// a path that exists (`exists` answers for a path relative to the folder).
pub fn evidence_ok(evidence: &str, exists: &dyn Fn(&str) -> bool) -> bool {
    evidence
        .split(['\n', ','])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .any(|part| {
            let url = part.starts_with("https://") || part.starts_with("http://");
            (url && part.len() > 12) || pr_ref_ok(part) || (!part.contains("..") && !part.starts_with('/') && exists(part))
        })
}

/// Applies the evidence rule to the judge's ruling. The judge may say DONE;
/// Neko only believes it with a passing check, or, for a goal with no check,
/// a concrete artifact or link. A BLOCKED with no question, or a WAIT with
/// nothing to wait for, is a CONTINUE.
pub fn enforce(goal: &Goal, check: Option<&CheckResult>, mut ruling: Ruling, exists: &dyn Fn(&str) -> bool) -> Ruling {
    match ruling.verdict {
        Verdict::Done => {
            if !goal.done_check.trim().is_empty() {
                match check {
                    Some(c) if c.passed() => {}
                    Some(c) => {
                        ruling.verdict = Verdict::Continue;
                        ruling.reason = format!("The judge said done, but `{}` still fails (exit {}).", head(&goal.done_check, 120), c.exit_code.map_or("none".into(), |c| c.to_string()));
                    }
                    None => {
                        ruling.verdict = Verdict::Continue;
                        ruling.reason = "The judge said done, but the check has not passed.".into();
                    }
                }
            } else if !evidence_ok(&ruling.evidence, exists) {
                ruling.verdict = Verdict::Continue;
                ruling.reason = "The judge said done without a concrete artifact or link. Say where the result is.".into();
            }
        }
        Verdict::Blocked if ruling.question.trim().is_empty() => {
            ruling.verdict = Verdict::Continue;
            ruling.reason = "Blocked without a question for the person; keep going.".into();
        }
        Verdict::Wait if ruling.wait.is_none() => {
            ruling.verdict = Verdict::Continue;
            ruling.reason = "Asked to wait without saying for what; keep going.".into();
        }
        _ => {}
    }
    if ruling.verdict == Verdict::Blocked && ruling.default_answer.trim().is_empty() {
        ruling.default_answer = "Use your best judgement and keep going.".into();
    }
    ruling
}

/// The next turn's message: what failed and why the judge asked for more.
pub fn continue_prompt(goal: &Goal, check: Option<&CheckResult>, ruling_reason: &str, error: Option<&str>) -> String {
    let mut out = format!("Keep working toward the goal. Turn {} of at most {}.\n\nGoal: {}\n", goal.turns + 1, goal.budget.max_turns, head(&goal.objective, 2000));
    if !goal.constraints.trim().is_empty() {
        out.push_str(&format!("Constraints: {}\n", head(&goal.constraints, 1000)));
    }
    if let (false, Some(c)) = (goal.done_check.trim().is_empty(), check) {
        out.push_str(&format!("\nNeko ran the check `{}`: exit {}.\n{}\n", goal.done_check.trim(), c.exit_code.map_or("none".into(), |x| x.to_string()), tail(&c.tail, 1200)));
    }
    if let Some(error) = error.filter(|e| !e.trim().is_empty()) {
        out.push_str(&format!("\nThe last turn stopped with: {}\n", head(error, 600)));
    }
    if !ruling_reason.trim().is_empty() {
        out.push_str(&format!("\nJudge: {}\n", head(ruling_reason, 500)));
    }
    out.push_str("\nFix what is failing and finish the goal. Neko will run the check again after you stop; do not claim it passes without running it. If you are done, say where the result is (a file path or a link).");
    out
}

/// What a fresh or resumed agent session needs to know: the goal, where it
/// stands, and the evidence so far. Added to the ticket's prompt.
pub fn context(goal: &Goal) -> String {
    let mut out = format!("\nThis ticket works a goal that keeps going until its evidence says done.\nGoal (the person's words): {}\n", head(&goal.objective, 2000));
    if !goal.constraints.trim().is_empty() {
        out.push_str(&format!("Constraints: {}\n", head(&goal.constraints, 1000)));
    }
    match goal.done_check.trim() {
        "" => out.push_str("Check: none. Done needs a concrete artifact or link; say where the result is.\n"),
        check => out.push_str(&format!("Check (Neko runs it in this folder after each turn; passing is what done means): {check}\n")),
    }
    out.push_str(&format!("Turn {} of at most {}.\n", goal.turns.max(1), goal.budget.max_turns));
    if !goal.journal.is_empty() {
        out.push_str("Journal so far (older first):\n");
        let skip = goal.journal.len().saturating_sub(5);
        for entry in goal.journal.iter().skip(skip) {
            out.push_str(&format!("- turn {}: done: {} | next: {}{}\n", entry.turn, head(&entry.done, 200), head(&entry.next, 200), if entry.note.is_empty() { String::new() } else { format!(" ({})", head(&entry.note, 80)) }));
        }
    }
    if !goal.ledger.is_empty() {
        out.push_str("Evidence ledger (newest last):\n");
        let skip = goal.ledger.len().saturating_sub(3);
        for entry in goal.ledger.iter().skip(skip) {
            out.push_str(&format!("- `{}` exit {}\n", head(&entry.command, 120), entry.exit_code.map_or("none".into(), |c| c.to_string())));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Holds and budgets
// ---------------------------------------------------------------------------

/// The time a usage-limit error says the limit resets, when it says one:
/// Claude's `usage limit reached|1731500000` (unix seconds) or "resets 3pm"
/// (the next 3pm on the local clock).
pub fn limit_reset_ms(error: &str, now_ms: i64) -> Option<i64> {
    if let Some((_, rest)) = error.split_once('|') {
        let digits: String = rest.trim().chars().take_while(char::is_ascii_digit).collect();
        if let Ok(seconds) = digits.parse::<i64>() {
            if seconds > 1_000_000_000 {
                return Some(if seconds > 100_000_000_000 { seconds } else { seconds * 1000 });
            }
        }
    }
    let lower = error.to_ascii_lowercase();
    let at = lower.find("resets ")? + "resets ".len();
    let word: String = lower[at..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == ':').collect();
    let (clock, pm) = if let Some(c) = word.strip_suffix("pm") { (c, Some(true)) } else if let Some(c) = word.strip_suffix("am") { (c, Some(false)) } else { (word.as_str(), None) };
    let (hour, minute) = match clock.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None => (clock.parse::<u32>().ok()?, 0),
    };
    let hour = match pm {
        Some(true) if hour < 12 => hour + 12,
        Some(false) if hour == 12 => 0,
        _ => hour,
    };
    if hour > 23 || minute > 59 {
        return None;
    }
    use chrono::{Local, TimeZone};
    let now = Local.timestamp_millis_opt(now_ms).single()?;
    let mut when = now.date_naive().and_hms_opt(hour, minute, 0)?;
    if Local.from_local_datetime(&when).single()? <= now {
        when += chrono::Duration::days(1);
    }
    Some(Local.from_local_datetime(&when).single()?.timestamp_millis())
}

/// When a goal held by a usage limit may try again: the reset time if the
/// error gives one (a little after it), else an hour.
pub fn hold_until_ms(error: &str, now_ms: i64) -> i64 {
    match limit_reset_ms(error, now_ms) {
        Some(reset) if reset > now_ms => reset.min(now_ms + MAX_HOLD_MS) + 30_000,
        _ => now_ms + DEFAULT_HOLD_MS,
    }
}

/// The cap a goal has reached, if any.
pub fn budget_reached(goal: &Goal, cost_cents: f64) -> Option<String> {
    if goal.turns >= goal.budget.max_turns {
        return Some(format!("it used its {} turns", goal.budget.max_turns));
    }
    if goal.budget.max_wall_ms > 0 && goal.wall_ms >= goal.budget.max_wall_ms {
        return Some(format!("it ran for {} hours", goal.budget.max_wall_ms / 3_600_000));
    }
    match goal.budget.max_cost_cents {
        Some(cap) if cost_cents >= f64::from(cap) => Some(format!("it reported ${:.2} of agent cost", cost_cents / 100.0)),
        _ => None,
    }
}

/// The error a run ended with when Neko's quiet watchdog stopped it.
pub const STALL_MARKER: &str = "looked stalled";

pub fn is_stall(error: &str) -> bool {
    error.contains(STALL_MARKER)
}

// ---------------------------------------------------------------------------
// The evidence gate for code work
// ---------------------------------------------------------------------------

const PROSE_EXTENSIONS: &[&str] = &["md", "mdx", "markdown", "txt", "rst", "adoc", "org", "png", "jpg", "jpeg", "gif", "svg", "webp", "pdf", "lock"];

/// A change that is only prose or media needs no check.
pub fn is_prose_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    if matches!(name, "license" | "notice" | "authors" | "changelog" | "readme") {
        return true;
    }
    lower.rsplit_once('.').is_some_and(|(_, ext)| PROSE_EXTENSIONS.contains(&ext))
}

/// Whether the changed files include code, so the gate applies.
pub fn needs_check(changed: &[String]) -> bool {
    changed.iter().any(|f| !is_prose_path(f))
}

/// A command that proves something: tests, builds, type checks, linters.
pub fn check_like(command: &str) -> bool {
    let c = command.to_ascii_lowercase();
    const WORDS: &[&str] = &["test", "check", "clippy", "build", "lint", "tsc", "pytest", "jest", "vitest", "mypy", "ruff", "make", "gradle", "mvn", "xcodebuild", "rspec", "phpunit", "verify", "compile"];
    c.split(|ch: char| !ch.is_ascii_alphanumeric()).any(|w| WORDS.contains(&w))
}

/// Ledger entries from the host's own command receipts. `reviewed` entries
/// (the independent review's proven checks) count whatever the command is
/// called; an agent's own commands count only when they look like checks.
pub fn entries_from_receipts(receipts: &[String], source: &str, reviewed: bool, now_ms: i64) -> Vec<CheckResult> {
    receipts
        .iter()
        .filter_map(|r| r.strip_prefix(crate::workbench::VERIFICATION_COMMAND_PREFIX))
        .map(crate::workbench::VerificationReceipt::decode)
        .filter(|r| r.has_complete_command() && r.exit_code.is_some())
        .filter_map(|r| {
            let command = r.command?;
            (reviewed || check_like(&command)).then(|| CheckResult { command: head(&command, 300), exit_code: r.exit_code, tail: tail(r.output.as_deref().unwrap_or(""), TAIL_BYTES), at_ms: now_ms, source: source.to_owned() })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    /// Only prose changed: no check needed.
    Skip,
    Pass,
    /// Code changed and no passing check is in the ledger; the failing output
    /// (or why there is none) goes to the next turn.
    Missing(String),
}

/// A ticket that changed code cannot reach review without a passing check in
/// its ledger, and the goal's own check (when it has one) must pass.
pub fn gate(changed: &[String], ledger: &[CheckResult], goal_check: Option<&CheckResult>) -> Gate {
    if let Some(check) = goal_check.filter(|c| !c.passed()) {
        return Gate::Missing(format!("The goal's check `{}` fails (exit {}):\n{}", check.command, check.exit_code.map_or("none".into(), |c| c.to_string()), tail(&check.tail, 1200)));
    }
    if !needs_check(changed) {
        return Gate::Skip;
    }
    if goal_check.is_some_and(CheckResult::passed) || ledger.iter().any(CheckResult::passed) {
        return Gate::Pass;
    }
    match ledger.iter().rev().find(|c| !c.passed()) {
        Some(c) => Gate::Missing(format!("`{}` fails (exit {}):\n{}", c.command, c.exit_code.map_or("none".into(), |x| x.to_string()), tail(&c.tail, 1200))),
        None => Gate::Missing("No check was run for the code you changed.".into()),
    }
}

/// The extra turn a gate failure sends the builder.
pub fn gate_instruction(reason: &str) -> String {
    format!("Evidence gate: this change touches code, and Neko has no passing check for it. Run the repository's checks (tests, build, linters) as their own commands, fix what fails, and run them again until they pass. Do not claim a check you did not run. What Neko found:\n{}", head(reason, 1600))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal(check: &str) -> Goal {
        new_goal("w", "Make the tests pass", check, "", None, 1_000).unwrap()
    }
    fn check(code: i64) -> CheckResult {
        CheckResult { command: "cargo test".into(), exit_code: Some(code), tail: "out".into(), at_ms: 1, source: "neko".into() }
    }
    fn ruling(verdict: Verdict) -> Ruling {
        Ruling { verdict, reason: "r".into(), evidence: String::new(), question: String::new(), default_answer: String::new(), wait: None, plan: String::new(), done: String::new(), next: String::new() }
    }

    #[test]
    fn rows_are_small_and_the_log_and_clock_have_their_own() {
        let db = Db::open_in_memory().unwrap();
        let mut g = goal("cargo test");
        g.ticket_id = Some("t1".into());
        g.wall_ms = 5_000;
        let g = create(&db, g).unwrap();
        push_ledger(&db, "t1", check(1)).unwrap();
        push_journal(&db, "t1", JournalEntry { turn: 1, done: "x".into(), ..Default::default() }).unwrap();
        let row = db.get_setting(&format!("{ROW}{}", g.id)).unwrap().unwrap();
        assert!(!row.contains("exit_code\":1"), "the ledger is not in the goal's row: {row}");
        assert!(!row.contains("\"wall_ms\":5000"), "the clock is not in the goal's row");
        let all = load_all(&db).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].wall_ms, 5_000);
        assert_eq!(all[0].ledger.len(), 1);
        assert_eq!(all[0].journal[0].turn, 1);
        for n in 0..30 {
            push_ledger(&db, "t1", check(n)).unwrap();
        }
        assert_eq!(log(&db, "t1").unwrap().ledger.len(), MAX_LEDGER);
        assert!(create(&db, Goal { ticket_id: Some("t1".into()), ..goal("") }).is_err(), "one unfinished goal per ticket");
    }

    #[test]
    fn the_judge_is_read_strictly_and_a_bad_answer_is_an_error() {
        let now = 1_000_000;
        let r = parse_ruling("```json\n{\"verdict\":\"done\",\"reason\":\"ok\",\"evidence\":\"out/report.md\"}\n```", now).unwrap();
        assert_eq!(r.verdict, Verdict::Done);
        let w = parse_ruling(r#"{"verdict":"WAIT","reason":"build running","wait_seconds":5,"wait_pid":4242}"#, now).unwrap();
        assert_eq!(w.wait.as_ref().unwrap().until_ms, Some(now + 60_000), "waits are at least a minute");
        assert_eq!(w.wait.as_ref().unwrap().pid, Some(4242));
        assert!(parse_ruling("done!", now).is_err());
        assert!(parse_ruling(r#"{"verdict":"MAYBE"}"#, now).is_err());
        let b = parse_ruling(r#"{"verdict":"BLOCKED","question":"Which account?","default_answer":"the work one"}"#, now).unwrap();
        assert_eq!(b.question, "Which account?");
    }

    #[test]
    fn done_needs_evidence_never_the_agents_claim() {
        let exists = |p: &str| p == "out/report.md";
        // With a check: only a passing one.
        let g = goal("cargo test");
        assert_eq!(enforce(&g, Some(&check(0)), ruling(Verdict::Done), &exists).verdict, Verdict::Done);
        let failed = enforce(&g, Some(&check(101)), ruling(Verdict::Done), &exists);
        assert_eq!(failed.verdict, Verdict::Continue);
        assert!(failed.reason.contains("still fails"));
        assert_eq!(enforce(&g, None, ruling(Verdict::Done), &exists).verdict, Verdict::Continue);
        // No check: a concrete artifact or link.
        let g = goal("none");
        assert!(g.done_check.is_empty());
        assert_eq!(enforce(&g, None, ruling(Verdict::Done), &exists).verdict, Verdict::Continue);
        for evidence in ["out/report.md", "https://github.com/acme/app/pull/12", "acme/app#12"] {
            assert_eq!(enforce(&g, None, Ruling { evidence: evidence.into(), ..ruling(Verdict::Done) }, &exists).verdict, Verdict::Done, "{evidence}");
        }
        assert_eq!(enforce(&g, None, Ruling { evidence: "../../etc/passwd".into(), ..ruling(Verdict::Done) }, &|_| true).verdict, Verdict::Continue);
        // Empty question or an empty wait is a continue.
        assert_eq!(enforce(&g, None, ruling(Verdict::Blocked), &exists).verdict, Verdict::Continue);
        assert_eq!(enforce(&g, None, ruling(Verdict::Wait), &exists).verdict, Verdict::Continue);
        let b = enforce(&g, None, Ruling { question: "Which?".into(), ..ruling(Verdict::Blocked) }, &exists);
        assert_eq!((b.verdict, b.default_answer.is_empty()), (Verdict::Blocked, false));
    }

    #[test]
    fn a_usage_limit_parks_until_its_reset_else_an_hour() {
        let now = 1_731_000_000_000;
        assert_eq!(hold_until_ms("Claude AI usage limit reached|1731500000", now), 1_731_500_000_000 + 30_000);
        assert_eq!(hold_until_ms("You've hit your limit", now), now + DEFAULT_HOLD_MS);
        assert_eq!(hold_until_ms("usage limit reached|1000", now), now + DEFAULT_HOLD_MS, "a time in the past is not a reset");
        let local = limit_reset_ms("You've hit your limit · resets 3pm", now).unwrap();
        assert!(local > now && local <= now + 24 * 3_600_000);
        assert!(limit_reset_ms("resets 25pm", now).is_none());
    }

    #[test]
    fn budgets_stop_on_turns_time_or_cost() {
        let mut g = goal("");
        assert!(budget_reached(&g, 0.0).is_none());
        g.turns = 20;
        assert!(budget_reached(&g, 0.0).unwrap().contains("20 turns"));
        g.turns = 1;
        g.wall_ms = DEFAULT_MAX_WALL_MS;
        assert!(budget_reached(&g, 0.0).unwrap().contains("hours"));
        g.wall_ms = 0;
        g.budget.max_cost_cents = Some(500);
        assert!(budget_reached(&g, 499.0).is_none());
        assert!(budget_reached(&g, 500.0).is_some());
    }

    #[test]
    fn code_needs_a_passing_check_and_prose_does_not() {
        let code = vec!["src/a.rs".to_owned(), "README.md".to_owned()];
        let prose = vec!["README.md".to_owned(), "docs/x.md".to_owned(), "logo.png".to_owned()];
        assert_eq!(gate(&prose, &[], None), Gate::Skip);
        assert!(matches!(gate(&code, &[], None), Gate::Missing(m) if m.contains("No check was run")));
        assert!(matches!(gate(&code, &[check(1)], None), Gate::Missing(m) if m.contains("fails")));
        assert_eq!(gate(&code, &[check(1), check(0)], None), Gate::Pass);
        // The goal's own check must pass even for prose.
        assert!(matches!(gate(&prose, &[check(0)], Some(&check(2))), Gate::Missing(_)));
        assert_eq!(gate(&code, &[], Some(&check(0))), Gate::Pass);
    }

    #[test]
    fn only_check_like_agent_commands_and_every_reviewed_one_enter_the_ledger() {
        let receipt = |command: &str, code: i64| format!("VERIFICATION_COMMAND {}", serde_json::json!({"command": command, "exit_code": code, "output": "ok"}));
        let receipts = vec![receipt("ls -la", 0), receipt("cargo test -p neko-core", 0), receipt("npm run lint", 1)];
        let agent = entries_from_receipts(&receipts, "agent", false, 5);
        assert_eq!(agent.iter().map(|e| e.command.as_str()).collect::<Vec<_>>(), ["cargo test -p neko-core", "npm run lint"]);
        let reviewed = entries_from_receipts(&receipts, "review", true, 5);
        assert_eq!(reviewed.len(), 3);
        assert!(check_like("pnpm run build") && check_like("cd web && npx tsc --noEmit") && !check_like("git status"));
    }

    #[test]
    fn prompts_and_context_name_the_goal_the_evidence_and_the_next_step() {
        let mut g = goal("cargo test");
        g.turns = 2;
        g.journal.push(JournalEntry { turn: 1, done: "wrote the parser".into(), next: "fix the failing test".into(), ..Default::default() });
        let prompt = judge_prompt(&g, Some(&check(101)), "I think it works", "/tmp/work");
        assert!(prompt.contains("Exit code: 101") && prompt.contains("fix the failing test") && prompt.contains("untrusted"));
        let next = continue_prompt(&g, Some(&check(101)), "the parser still fails", None);
        assert!(next.contains("Turn 3") && next.contains("exit 101") && next.contains("the parser still fails"));
        assert!(context(&g).contains("Check (Neko runs it"));
    }
}
