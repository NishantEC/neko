//! The sessions column's groups: every task across workspaces, by the state that matters to the
//! person (polish spec §0, "Grouping rules"). Pure functions of a snapshot summary and the time,
//! so every boundary is tested without a window.
//!
//! | Group | What goes in it | Shown |
//! |---|---|---|
//! | **Needs you** | waiting on the person with activity in the last 7 days, and failed in the last 48 hours | at most 8, then "Show N more" |
//! | **Running** | planning, building, reviewing (and queued: it runs next) | all |
//! | **Recent** | completed, failed or cancelled in the last 7 days, newest first | at most 5, plus "View all" |
//! | **Waiting a while** | waiting on the person, no activity for more than 7 days | after Recent, collapsed, with a count |
//!
//! A task is in one group only: a failure in its first 48 hours is in Needs you, not also in Recent.
//! Anything older than 7 days that is not waiting on the person is in none, and only "View all"
//! lists it.

use neko_protocol::workbench::TaskStatus;

use crate::link::{Summary, TaskRow};

pub const HOUR_MS: i64 = 60 * 60 * 1000;
pub const DAY_MS: i64 = 24 * HOUR_MS;
/// A failure needs a look for this long, then it is history.
pub const FAILED_NEEDS_YOU_MS: i64 = 48 * HOUR_MS;
/// Work waiting on the person is "Needs you" for this long, then "Waiting a while". Finished work is "Recent" for as long.
pub const WEEK_MS: i64 = 7 * DAY_MS;
/// Needs you shows this many rows, then "Show N more".
pub const NEEDS_CAP: usize = 8;
/// Recent shows this many rows, then "View all".
pub const RECENT_CAP: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    NeedsYou,
    Running,
    Recent,
    WaitingAWhile,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::NeedsYou, Group::Running, Group::Recent, Group::WaitingAWhile];

    pub fn title(self) -> &'static str {
        match self {
            Group::NeedsYou => "Needs you",
            Group::Running => "Running",
            Group::Recent => "Recent",
            Group::WaitingAWhile => "Waiting a while",
        }
    }

    pub fn index(self) -> usize {
        Group::ALL.iter().position(|g| *g == self).unwrap_or(0)
    }

    /// Whether the group starts folded: only "Waiting a while".
    pub fn collapsed_by_default(self) -> bool {
        self == Group::WaitingAWhile
    }

    /// The most rows the group shows before its "Show N more" or "View all", or none for all of them.
    pub fn cap(self) -> Option<usize> {
        match self {
            Group::NeedsYou => Some(NEEDS_CAP),
            Group::Recent => Some(RECENT_CAP),
            Group::Running | Group::WaitingAWhile => None,
        }
    }
}

/// Whether the task waits on the person: a plan or a step to approve, a result to review, a question, a cap.
pub fn waits_on_person(task: &TaskRow) -> bool {
    task.wait.waits() || matches!(task.status, TaskStatus::AwaitingApproval | TaskStatus::ReadyForReview)
}

/// Which group a task belongs to at `now_ms`, or none (older than a week and not waiting on the person:
/// only "View all" lists it).
pub fn group_of(task: &TaskRow, now_ms: i64) -> Option<Group> {
    let age = now_ms - task.updated_at_ms;
    if waits_on_person(task) {
        return Some(if age <= WEEK_MS { Group::NeedsYou } else { Group::WaitingAWhile });
    }
    match task.status {
        TaskStatus::Failed if age <= FAILED_NEEDS_YOU_MS => Some(Group::NeedsYou),
        TaskStatus::Failed | TaskStatus::Completed | TaskStatus::Cancelled => (age <= WEEK_MS).then_some(Group::Recent),
        TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing | TaskStatus::Queued => Some(Group::Running),
        // Only reached by a row that says nothing it waits for: it waits (handled above).
        TaskStatus::AwaitingApproval | TaskStatus::ReadyForReview => Some(Group::NeedsYou),
    }
}

/// The four groups' tasks, each newest first (every task of the group, before any cap). Always four
/// entries, in `Group::ALL` order.
pub fn grouped(summary: &Summary, now_ms: i64) -> Vec<(Group, Vec<&TaskRow>)> {
    Group::ALL
        .iter()
        .map(|g| {
            let mut rows: Vec<&TaskRow> = summary.tasks.iter().filter(|t| group_of(t, now_ms) == Some(*g)).collect();
            rows.sort_by_key(|t| std::cmp::Reverse(t.updated_at_ms));
            (*g, rows)
        })
        .collect()
}

/// How many rows of a group of `len` show, given whether the person asked for the rest, and how many are
/// beyond the cap (what "Show N more" / "View all" is about; zero when the group is within it).
pub fn shown(group: Group, len: usize, expanded: bool) -> (usize, usize) {
    match group.cap() {
        Some(cap) => (if expanded { len } else { len.min(cap) }, len.saturating_sub(cap)),
        None => (len, 0),
    }
}

/// Every task, newest first: what "View all" lists.
pub fn all_sessions(summary: &Summary) -> Vec<&TaskRow> {
    let mut rows: Vec<&TaskRow> = summary.tasks.iter().collect();
    rows.sort_by_key(|t| std::cmp::Reverse(t.updated_at_ms));
    rows
}

/// The task a fresh window could select: the first row of the first group that has one. (A window opens
/// on Home; this is for callers that need a session.)
pub fn first_session(summary: &Summary, now_ms: i64) -> Option<String> {
    grouped(summary, now_ms).into_iter().find_map(|(_, rows)| rows.first().map(|t| t.id.clone()))
}

/// Sessions whose title or key matches `query`, best first (`frizbee`: typo-tolerant, like fzf).
/// An empty query lists the sessions in the column's order. At most `limit`.
pub fn fuzzy_sessions<'a>(summary: &'a Summary, query: &str, now_ms: i64, limit: usize) -> Vec<&'a TaskRow> {
    let query = query.trim();
    if query.is_empty() {
        return grouped(summary, now_ms).into_iter().flat_map(|(_, rows)| rows).take(limit).collect();
    }
    let hay: Vec<String> = summary.tasks.iter().map(|t| format!("{} {}", t.key.as_deref().unwrap_or(""), t.title)).collect();
    let refs: Vec<&str> = hay.iter().map(String::as_str).collect();
    let mut matcher = frizbee::Matcher::new(query, &frizbee::Config::default());
    let mut found = matcher.match_list(&refs);
    found.sort_by(|a, b| b.score.cmp(&a.score).then(summary.tasks[b.index as usize].updated_at_ms.cmp(&summary.tasks[a.index as usize].updated_at_ms)));
    found.into_iter().take(limit).map(|m| &summary.tasks[m.index as usize]).collect()
}

/// The word beside a row's age: what the glyph alone cannot say, since a task that needs you and
/// one an agent is working on share the amber half circle (as in the web UI).
pub fn status_word(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "Queued",
        TaskStatus::Planning => "Planning",
        TaskStatus::AwaitingApproval => "Needs you",
        TaskStatus::Building => "Building",
        TaskStatus::Reviewing => "Reviewing",
        TaskStatus::ReadyForReview => "Review",
        TaskStatus::Completed => "Done",
        TaskStatus::Failed => "Failed",
        TaskStatus::Cancelled => "Cancelled",
    }
}

/// What an amber row says in the column instead of its age: "Needs you" or "Building" (a plan or
/// step waits on the person, or an agent is at work). None for the glyphs that need no word.
pub fn amber_word(status: TaskStatus) -> Option<&'static str> {
    match status {
        TaskStatus::AwaitingApproval => Some("Needs you"),
        TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing => Some("Building"),
        _ => None,
    }
}

/// Quiet meta for a row: how long ago it last changed ("now", "5m", "3h", "2d").
pub fn age(now_ms: i64, then_ms: i64) -> String {
    let secs = ((now_ms - then_ms) / 1000).max(0);
    match secs {
        0..=44 => "now".to_owned(),
        45..=3_599 => format!("{}m", (secs + 30) / 60),
        3_600..=86_399 => format!("{}h", secs / 3_600),
        _ => format!("{}d", secs / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::Wait;

    const NOW: i64 = 1_800_000_000_000;

    fn row(id: &str, status: TaskStatus, ago_ms: i64) -> TaskRow {
        TaskRow { id: id.into(), key: None, title: id.into(), status, updated_at_ms: NOW - ago_ms, goal: String::new(), wait: Wait::None, progress: None }
    }

    fn waiting(id: &str, status: TaskStatus, wait: Wait, ago_ms: i64) -> TaskRow {
        TaskRow { wait, ..row(id, status, ago_ms) }
    }

    #[test]
    fn every_status_lands_in_the_group_that_matters() {
        use TaskStatus::*;
        let table = [
            (AwaitingApproval, Some(Group::NeedsYou)),
            (ReadyForReview, Some(Group::NeedsYou)),
            (Failed, Some(Group::NeedsYou)),
            (Planning, Some(Group::Running)),
            (Building, Some(Group::Running)),
            (Reviewing, Some(Group::Running)),
            (Queued, Some(Group::Running)),
            (Completed, Some(Group::Recent)),
            (Cancelled, Some(Group::Recent)),
        ];
        for (status, group) in table {
            assert_eq!(group_of(&row("t", status, 1000), NOW), group, "{status:?}");
        }
    }

    #[test]
    fn a_failure_needs_you_for_48_hours_exactly_then_is_recent_for_a_week_then_only_view_all() {
        let failed = |ago: i64| group_of(&row("f", TaskStatus::Failed, ago), NOW);
        assert_eq!(failed(FAILED_NEEDS_YOU_MS - 1), Some(Group::NeedsYou));
        assert_eq!(failed(FAILED_NEEDS_YOU_MS), Some(Group::NeedsYou), "48 hours old is still within 48 hours");
        assert_eq!(failed(FAILED_NEEDS_YOU_MS + 1), Some(Group::Recent), "failures older than 48 hours never appear in Needs you");
        assert_eq!(failed(WEEK_MS), Some(Group::Recent));
        assert_eq!(failed(WEEK_MS + 1), None, "older than 7 days: View all only");
        assert_eq!(failed(30 * DAY_MS), None);
    }

    #[test]
    fn waiting_on_the_person_is_needs_you_for_7_days_then_waiting_a_while_and_never_leaves() {
        for (status, wait) in [(TaskStatus::AwaitingApproval, Wait::Plan), (TaskStatus::AwaitingApproval, Wait::Question), (TaskStatus::ReadyForReview, Wait::Review), (TaskStatus::Building, Wait::Cap)] {
            let at = |ago: i64| group_of(&waiting("w", status, wait, ago), NOW);
            assert_eq!(at(0), Some(Group::NeedsYou), "{status:?} {wait:?}");
            assert_eq!(at(WEEK_MS), Some(Group::NeedsYou), "exactly 7 days is within 7 days");
            assert_eq!(at(WEEK_MS + 1), Some(Group::WaitingAWhile), "{status:?} {wait:?}");
            assert_eq!(at(90 * DAY_MS), Some(Group::WaitingAWhile), "still waiting on the person: it stays, folded");
        }
        // A goal that hit its cap on a running ticket waits on the person, not on the agent.
        assert_eq!(group_of(&waiting("g", TaskStatus::Building, Wait::Cap, 1000), NOW), Some(Group::NeedsYou));
        assert_eq!(group_of(&waiting("g", TaskStatus::Building, Wait::None, 1000), NOW), Some(Group::Running));
    }

    #[test]
    fn finished_work_is_recent_for_7_days_then_only_view_all() {
        for status in [TaskStatus::Completed, TaskStatus::Cancelled] {
            assert_eq!(group_of(&row("c", status, WEEK_MS), NOW), Some(Group::Recent));
            assert_eq!(group_of(&row("c", status, WEEK_MS + 1), NOW), None);
        }
    }

    #[test]
    fn a_task_is_in_one_group_only_and_the_four_are_always_there_newest_first() {
        use TaskStatus::*;
        let s = Summary {
            tasks: vec![
                row("old", Building, 5000),
                row("new", Building, 1000),
                row("ask", AwaitingApproval, 9000),
                row("fresh-fail", Failed, HOUR_MS),
                row("stale-fail", Failed, 3 * DAY_MS),
                row("ancient", Completed, 40 * DAY_MS),
                waiting("parked", AwaitingApproval, Wait::Plan, 20 * DAY_MS),
            ],
            ..Summary::default()
        };
        let g = grouped(&s, NOW);
        assert_eq!(g.iter().map(|(g, _)| *g).collect::<Vec<_>>(), Group::ALL);
        let ids = |i: usize| g[i].1.iter().map(|t| t.id.as_str()).collect::<Vec<_>>();
        assert_eq!(ids(0), ["ask", "fresh-fail"], "needs you, newest first: ask (9s old) before the failure (1h old)");
        assert_eq!(ids(1), ["new", "old"]);
        assert_eq!(ids(2), ["stale-fail"]);
        assert_eq!(ids(3), ["parked"]);
        let total: usize = g.iter().map(|(_, r)| r.len()).sum();
        assert_eq!(total, s.tasks.len() - 1, "only the ancient finished task is in no group");
        assert_eq!(all_sessions(&s).len(), s.tasks.len(), "View all lists every task");
        assert_eq!(first_session(&s, NOW).as_deref(), Some("ask"));
    }

    #[test]
    fn caps_are_eight_for_needs_you_and_five_for_recent_and_the_rest_waits_behind_a_link() {
        assert_eq!(Group::NeedsYou.cap(), Some(8));
        assert_eq!(Group::Recent.cap(), Some(5));
        assert_eq!((Group::Running.cap(), Group::WaitingAWhile.cap()), (None, None));
        // Needs you: 8 fit, 9 show 8 and say "Show 1 more", 20 say "Show 12 more"; expanded shows all.
        assert_eq!(shown(Group::NeedsYou, 8, false), (8, 0));
        assert_eq!(shown(Group::NeedsYou, 9, false), (8, 1));
        assert_eq!(shown(Group::NeedsYou, 20, false), (8, 12));
        assert_eq!(shown(Group::NeedsYou, 20, true), (20, 12));
        assert_eq!(shown(Group::Recent, 5, false), (5, 0));
        assert_eq!(shown(Group::Recent, 6, false), (5, 1));
        assert_eq!(shown(Group::Running, 40, false), (40, 0), "Running shows all");
        assert_eq!(shown(Group::WaitingAWhile, 40, false), (40, 0));
        assert!(Group::WaitingAWhile.collapsed_by_default());
        assert!(Group::ALL.iter().filter(|g| g.collapsed_by_default()).count() == 1);
    }

    #[test]
    fn the_palette_finds_a_session_by_a_few_letters_of_its_title() {
        let mut s = Summary { tasks: vec![row("a", TaskStatus::Building, 1000), row("b", TaskStatus::Building, 2000)], ..Summary::default() };
        s.tasks[0].title = "Add dark mode to the settings page".into();
        s.tasks[1].title = "Fix the retry in the sync job".into();
        let hit = |q: &str| fuzzy_sessions(&s, q, NOW, 5).iter().map(|t| t.id.clone()).collect::<Vec<_>>();
        assert_eq!(hit("dark"), ["a"]);
        assert_eq!(hit("retry sync").first().map(String::as_str), Some("b"));
        assert_eq!(hit("").len(), 2, "an empty query lists the sessions");
        assert!(hit("zzzqq").is_empty());
    }

    #[test]
    fn a_row_says_which_amber_it_is() {
        assert_eq!((status_word(TaskStatus::AwaitingApproval), status_word(TaskStatus::Building)), ("Needs you", "Building"));
        // Same glyph, so the words must differ (DESIGN.md section 3: the glyph is never the only cue).
        let words: std::collections::HashSet<_> = [TaskStatus::AwaitingApproval, TaskStatus::Planning, TaskStatus::Building, TaskStatus::Reviewing].map(status_word).into_iter().collect();
        assert_eq!(words.len(), 4);
    }

    #[test]
    fn only_amber_rows_say_a_word_in_the_column() {
        use TaskStatus::*;
        assert_eq!([AwaitingApproval, Planning, Building, Reviewing].map(amber_word), [Some("Needs you"), Some("Building"), Some("Building"), Some("Building")]);
        for s in [Queued, ReadyForReview, Completed, Failed, Cancelled] {
            assert_eq!(amber_word(s), None, "{s:?} shows its age");
        }
    }

    #[test]
    fn ages_are_quiet_and_short() {
        let a = |ms_ago: i64| age(NOW, NOW - ms_ago);
        assert_eq!(a(10_000), "now");
        assert_eq!(a(5 * 60_000), "5m");
        assert_eq!(a(3 * 3_600_000), "3h");
        assert_eq!(a(2 * DAY_MS), "2d");
        assert_eq!(age(NOW, NOW + 5_000), "now", "a clock a little behind is not negative");
    }
}
