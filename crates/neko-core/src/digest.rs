//! The morning digest: one calm summary composed once a day from data Neko
//! already has, with no model call. What finished, what is stuck and why, what
//! needs a decision (each with its default), today's calendar and yesterday's
//! spend. Composing is pure (`compose`); the daemon gathers the facts, stores
//! the result in its own row and pushes it once. See
//! docs/knowledge/morning-digest.md and DESIGN.md section 6.
use neko_protocol::digest::{Digest, DigestLine};
use neko_protocol::goals::{Goal, GoalStatus};
use neko_protocol::routines::NeedsYou;
use neko_protocol::workbench::{Task, TaskStatus};
use serde::{Deserialize, Serialize};

pub const ROW: &str = "neko_digest_v1";
/// Lines listed per group; the rest become "and N more".
const MAX_LINES: usize = 5;
/// A digest is not composed later than this after its time (a Mac that slept
/// through the morning skips the day instead of summarising at supper).
pub const LATE_MS: i64 = 6 * 3_600_000;
/// "Since the last digest" never reaches back further than this.
const MAX_LOOKBACK_MS: i64 = 3 * 86_400_000;
const DAY_MS: i64 = 86_400_000;

/// The row: the day last handled (composed or skipped), the digest, and
/// whether it was pushed. Changes about once a day.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// Local day (`YYYY-MM-DD`) already handled.
    pub day: String,
    /// When the last digest was composed; the next one covers since then.
    pub composed_ms: i64,
    pub digest: Option<Digest>,
    pub pushed: bool,
}

pub fn load(db: &crate::Db) -> State {
    db.get_setting(ROW).ok().flatten().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(db: &crate::Db, state: &State) -> Result<(), String> {
    let text = serde_json::to_string(state).map_err(|e| e.to_string())?;
    db.set_setting(ROW, &text).map_err(|e| e.to_string())
}

/// Whether a digest should be composed now: the feature is on, it is past the
/// chosen time but not by more than `LATE_MS`, and today was not handled yet.
pub fn due(enabled: bool, state: &State, day: &str, minutes_into_day: u32, hour: u32, minute: u32) -> bool {
    let at = hour * 60 + minute;
    enabled && state.day != day && minutes_into_day >= at && i64::from(minutes_into_day - at) * 60_000 < LATE_MS
}

/// Everything `compose` reads.
#[derive(Debug, Default)]
pub struct Facts<'a> {
    pub now_ms: i64,
    /// When the previous digest was composed (0 = never).
    pub since_ms: i64,
    pub day: String,
    pub tasks: &'a [Task],
    pub goals: &'a [Goal],
    /// Open needs-you items.
    pub open_items: &'a [NeedsYou],
    /// The daily spend ceiling is holding new work.
    pub spend_held: bool,
    /// Today's events: how many, and the first timed one as (time, title).
    /// `None` when Calendar is off.
    pub calendar: Option<(usize, Option<(String, String)>)>,
    pub spend_yesterday_usd: Option<f64>,
}

fn head(text: &str, max: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= max { text } else { format!("{}…", text.chars().take(max - 1).collect::<String>()) }
}

fn capped(mut lines: Vec<DigestLine>) -> Vec<DigestLine> {
    if lines.len() > MAX_LINES {
        let more = lines.len() - MAX_LINES;
        lines.truncate(MAX_LINES);
        lines.push(DigestLine { text: format!("and {more} more"), default: None });
    }
    lines
}

fn plain(text: String) -> DigestLine {
    DigestLine { text, default: None }
}

fn merged(task: &Task) -> bool {
    task.pull_request.as_ref().and_then(|pr| pr.status.as_ref()).is_some_and(|s| s.state == "MERGED")
}

/// The window the digest covers: since the last one, at most three days, at
/// least until yesterday's start of this window's day is not needed.
pub fn window_start(now_ms: i64, last_composed_ms: i64) -> i64 {
    if last_composed_ms <= 0 { now_ms - DAY_MS } else { last_composed_ms.max(now_ms - MAX_LOOKBACK_MS) }
}

/// Composes the digest, or `None` when there is nothing to say (nothing
/// finished, stuck or waiting on a decision).
pub fn compose(f: &Facts) -> Option<Digest> {
    let since = f.since_ms;
    let mut finished = Vec::new();
    for task in f.tasks.iter().filter(|t| t.status == TaskStatus::Completed && t.updated_at_ms >= since && !merged(t)) {
        finished.push(plain(format!("Task done: {}", head(&task.title, 90))));
    }
    for goal in f.goals.iter().filter(|g| g.status == GoalStatus::Done && g.updated_ms >= since) {
        finished.push(plain(format!("Goal done: {}", head(&goal.objective, 90))));
    }
    for task in f.tasks.iter().filter(|t| merged(t) && t.updated_at_ms >= since) {
        finished.push(plain(format!("PR merged: {}", head(&task.title, 90))));
    }

    let mut stuck = Vec::new();
    for goal in f.goals.iter().filter(|g| !g.status.is_finished()) {
        let what = head(&goal.objective, 70);
        match goal.status {
            GoalStatus::Blocked => {
                let (question, default) = goal.blocked.as_ref().map(|b| (head(&b.question, 120), Some(head(&b.default_answer, 120)))).unwrap_or_default();
                stuck.push(DigestLine { text: format!("Blocked: {what}. {question}"), default });
            }
            GoalStatus::Held => stuck.push(plain(format!("Held: {what}. A usage limit is in the way; it resumes when it resets."))),
            GoalStatus::Paused => {
                let why = if goal.paused_reason.trim().is_empty() { "it is paused".to_owned() } else { head(&goal.paused_reason, 100) };
                stuck.push(plain(format!("Paused: {what}. {why}")));
            }
            _ => {}
        }
    }
    for task in f.tasks.iter().filter(|t| t.status == TaskStatus::Failed && t.updated_at_ms >= since) {
        let why = task.events.iter().rev().find(|e| e.role == "error" || e.message.to_lowercase().contains("fail")).map_or_else(String::new, |e| format!(" ({})", head(&e.message, 90)));
        stuck.push(plain(format!("Failing: {}{why}", head(&task.title, 70))));
    }
    if f.spend_held {
        stuck.push(plain("Held: the daily spend ceiling is reached, so new goal turns and routine runs wait until it resets or is raised.".to_owned()));
    }

    let decisions: Vec<DigestLine> = f
        .open_items
        .iter()
        .map(|i| DigestLine { text: head(&i.title, 120), default: i.default.clone() })
        .collect();

    if finished.is_empty() && stuck.is_empty() && decisions.is_empty() {
        return None;
    }

    let status = match finished.len() {
        0 => "Nothing finished".to_owned(),
        1 => "1 thing finished".to_owned(),
        n => format!("{n} things finished"),
    };
    let bottleneck = match stuck.first() {
        None => "nothing is stuck".to_owned(),
        Some(line) if stuck.len() == 1 => format!("1 thing is stuck ({})", head(line.text.split(". ").next().unwrap_or(&line.text), 70)),
        Some(_) => format!("{} things are stuck", stuck.len()),
    };
    let need = match decisions.len() {
        0 => "Nothing needs you right now.".to_owned(),
        1 => "1 thing needs you.".to_owned(),
        n => format!("{n} things need you."),
    };
    let calendar = match &f.calendar {
        Some((0, _)) => "No events today.".to_owned(),
        Some((n, Some((time, title)))) => format!("{n} event{} today, first: {} at {time}.", if *n == 1 { "" } else { "s" }, head(title, 60)),
        Some((n, None)) => format!("{n} event{} today.", if *n == 1 { "" } else { "s" }),
        None => String::new(),
    };
    let spend = f.spend_yesterday_usd.filter(|v| *v > 0.0).map(|v| format!("Yesterday's reported spend: ${v:.2}.")).unwrap_or_default();
    Some(Digest {
        at_ms: f.now_ms,
        day: f.day.clone(),
        headline: format!("{status}, {bottleneck}. {need}"),
        finished: capped(finished),
        stuck: capped(stuck),
        decisions: capped(decisions),
        calendar,
        spend,
        read: false,
    })
}

/// The digest as plain text: the notes of its reminder and the base of its
/// ntfy message.
pub fn to_text(d: &Digest) -> String {
    let mut out = vec![d.headline.clone()];
    let mut group = |title: &str, lines: &[DigestLine]| {
        if lines.is_empty() {
            return;
        }
        out.push(String::new());
        out.push(format!("{title}:"));
        for line in lines {
            match &line.default {
                Some(default) => out.push(format!("- {} (default: {default})", line.text)),
                None => out.push(format!("- {}", line.text)),
            }
        }
    };
    group("Finished", &d.finished);
    group("Stuck", &d.stuck);
    // A decision with no default says so.
    let decisions: Vec<DigestLine> = d.decisions.iter().map(|l| DigestLine { text: l.text.clone(), default: Some(l.default.clone().unwrap_or_else(|| "No default".into())) }).collect();
    group("Needs a decision", &decisions);
    for extra in [&d.calendar, &d.spend] {
        if !extra.is_empty() {
            out.push(String::new());
            out.push(extra.clone());
        }
    }
    out.join("\n")
}

/// The reminder title.
pub fn title(d: &Digest) -> String {
    format!("Neko morning digest: {} finished, {} stuck, {} for you", d.finished.iter().filter(|l| !l.text.starts_with("and ")).count(), d.stuck.iter().filter(|l| !l.text.starts_with("and ")).count(), d.decisions.iter().filter(|l| !l.text.starts_with("and ")).count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::goals::Blocked;

    fn task(title: &str, status: TaskStatus, at: i64) -> Task {
        let mut t: Task = serde_json::from_value(serde_json::json!({"id": title, "workspace_id": "w", "title": title, "goal": "g", "status": "Queued", "plan": "", "result": "", "worktree": null, "created_at_ms": 0, "updated_at_ms": 0, "events": []})).unwrap();
        t.status = status;
        t.updated_at_ms = at;
        t
    }

    fn facts<'a>(tasks: &'a [Task], goals: &'a [Goal], items: &'a [NeedsYou]) -> Facts<'a> {
        Facts { now_ms: 10_000, since_ms: 1_000, day: "2026-11-14".into(), tasks, goals, open_items: items, ..Default::default() }
    }

    #[test]
    fn nothing_to_say_is_skipped() {
        let old = [task("old", TaskStatus::Completed, 10)];
        assert!(compose(&facts(&old, &[], &[])).is_none(), "work finished before the last digest is not news");
        let calendar_only = Facts { calendar: Some((3, None)), spend_yesterday_usd: Some(1.0), ..facts(&[], &[], &[]) };
        assert!(compose(&calendar_only).is_none(), "a calendar and a spend line alone are not a digest");
    }

    #[test]
    fn it_lists_what_finished_what_is_stuck_and_what_needs_you_with_defaults() {
        let mut merged_task = task("Fix login", TaskStatus::Completed, 5_000);
        merged_task.pull_request = Some(neko_protocol::workbench::PullRequest { url: "https://example.com/pr/1".into(), status: Some(neko_protocol::workbench::PrStatus { state: "MERGED".into(), ..Default::default() }), ..Default::default() });
        let tasks = [task("Write docs", TaskStatus::Completed, 5_000), merged_task, task("Flaky build", TaskStatus::Failed, 6_000)];
        let goals = [
            Goal { objective: "Ship the exporter".into(), status: GoalStatus::Done, updated_ms: 7_000, ..Default::default() },
            Goal { objective: "Migrate the reports".into(), status: GoalStatus::Blocked, blocked: Some(Blocked { question: "Which database?".into(), default_answer: "the primary".into(), inbox_seq: Some(4) }), ..Default::default() },
        ];
        let items = [
            NeedsYou { seq: 4, title: "Extend by 10 turns?".into(), default: Some("Extend by 10 turns".into()), ..Default::default() },
            NeedsYou { seq: 5, title: "Sign in to the tracker?".into(), ..Default::default() },
        ];
        let f = Facts { calendar: Some((3, Some(("09:30".into(), "Standup".into())))), spend_yesterday_usd: Some(1.234), spend_held: true, ..facts(&tasks, &goals, &items) };
        let d = compose(&f).unwrap();
        assert_eq!(d.finished.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), vec!["Task done: Write docs", "Goal done: Ship the exporter", "PR merged: Fix login"]);
        assert_eq!(d.stuck.len(), 3, "a blocked goal, a failing task and the held spend");
        assert_eq!(d.stuck[0].default.as_deref(), Some("the primary"));
        assert_eq!(d.decisions[0].default.as_deref(), Some("Extend by 10 turns"));
        assert_eq!(d.decisions[1].default, None);
        assert_eq!(d.calendar, "3 events today, first: Standup at 09:30.");
        assert_eq!(d.spend, "Yesterday's reported spend: $1.23.");
        assert!(d.headline.starts_with("3 things finished, 3 things are stuck. 2 things need you."), "{}", d.headline);
        let text = to_text(&d);
        assert!(text.contains("- Sign in to the tracker? (default: No default)") && text.contains("(default: the primary)"), "{text}");
    }

    #[test]
    fn when_nothing_needs_you_it_says_exactly_that() {
        let tasks = [task("Write docs", TaskStatus::Completed, 5_000)];
        let d = compose(&facts(&tasks, &[], &[])).unwrap();
        assert_eq!(d.headline, "1 thing finished, nothing is stuck. Nothing needs you right now.");
        assert!(d.stuck.is_empty() && d.decisions.is_empty() && d.calendar.is_empty() && d.spend.is_empty());
    }

    #[test]
    fn long_lists_are_capped_and_the_window_is_bounded() {
        let items: Vec<NeedsYou> = (1..=8).map(|n| NeedsYou { seq: n, title: format!("Q{n}?"), ..Default::default() }).collect();
        let d = compose(&facts(&[], &[], &items)).unwrap();
        assert_eq!((d.decisions.len(), d.decisions.last().unwrap().text.as_str()), (6, "and 3 more"));
        assert_eq!(window_start(10 * DAY_MS, 0), 9 * DAY_MS);
        assert_eq!(window_start(10 * DAY_MS, 2 * DAY_MS), 7 * DAY_MS, "never further back than three days");
        assert_eq!(window_start(10 * DAY_MS, 9 * DAY_MS), 9 * DAY_MS);
    }

    #[test]
    fn it_is_due_once_a_day_after_its_time_and_not_much_later() {
        let mut state = State::default();
        assert!(!due(true, &state, "d1", 8 * 60 + 29, 8, 30), "not before the time");
        assert!(due(true, &state, "d1", 8 * 60 + 30, 8, 30));
        assert!(!due(false, &state, "d1", 9 * 60, 8, 30), "off means off");
        assert!(!due(true, &state, "d1", 15 * 60, 8, 30), "a day slept through is skipped");
        state.day = "d1".into();
        assert!(!due(true, &state, "d1", 9 * 60, 8, 30), "once per day");
        assert!(due(true, &state, "d2", 9 * 60, 8, 30));
    }

    #[test]
    fn the_row_round_trips() {
        let db = crate::Db::open_in_memory().unwrap();
        assert_eq!(load(&db), State::default());
        let state = State { day: "d".into(), composed_ms: 5, digest: Some(Digest { headline: "h".into(), ..Default::default() }), pushed: true };
        save(&db, &state).unwrap();
        assert_eq!(load(&db), state);
    }
}
