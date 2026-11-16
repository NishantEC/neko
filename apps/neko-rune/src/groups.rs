//! The sessions column's groups: every task across workspaces, by the state that matters to the
//! person. Pure functions of a snapshot summary and the time, so the rules are tested without a
//! window.
//!
//! * **Needs you**: a plan or step waits for approval, a result waits for review, or the task
//!   failed. Amber, sky and coral in the glyphs; all of them want a look.
//! * **Running**: an agent is on it, or it waits its turn.
//! * **Done today**: finished or cancelled in the last 24 hours. Older work leaves the column
//!   (the Tasks view of the web UI keeps it; a full view will).

use neko_protocol::workbench::TaskStatus;

use crate::link::{Summary, TaskRow};

pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    NeedsYou,
    Running,
    DoneToday,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::NeedsYou, Group::Running, Group::DoneToday];

    pub fn title(self) -> &'static str {
        match self {
            Group::NeedsYou => "Needs you",
            Group::Running => "Running",
            Group::DoneToday => "Done today",
        }
    }

    pub fn index(self) -> usize {
        Group::ALL.iter().position(|g| *g == self).unwrap_or(0)
    }
}

/// Which group a task belongs to at `now_ms`, or none (finished more than a day ago).
pub fn group_of(task: &TaskRow, now_ms: i64) -> Option<Group> {
    match task.status {
        TaskStatus::AwaitingApproval | TaskStatus::ReadyForReview | TaskStatus::Failed => Some(Group::NeedsYou),
        TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing | TaskStatus::Queued => Some(Group::Running),
        TaskStatus::Completed | TaskStatus::Cancelled => (now_ms - task.updated_at_ms < DAY_MS).then_some(Group::DoneToday),
    }
}

/// The three groups' tasks, each newest first. Always three entries, in `Group::ALL` order.
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

/// The task a fresh window selects: the first row of the first group that has one.
pub fn first_session(summary: &Summary, now_ms: i64) -> Option<String> {
    grouped(summary, now_ms).into_iter().find_map(|(_, rows)| rows.first().map(|t| t.id.clone()))
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

    const NOW: i64 = 1_800_000_000_000;

    fn row(id: &str, status: TaskStatus, ago_ms: i64) -> TaskRow {
        TaskRow { id: id.into(), key: None, title: id.into(), status, updated_at_ms: NOW - ago_ms, goal: String::new() }
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
            (Completed, Some(Group::DoneToday)),
            (Cancelled, Some(Group::DoneToday)),
        ];
        for (status, group) in table {
            assert_eq!(group_of(&row("t", status, 1000), NOW), group, "{status:?}");
        }
    }

    #[test]
    fn done_leaves_the_column_after_a_day_but_what_needs_you_stays() {
        assert_eq!(group_of(&row("a", TaskStatus::Completed, DAY_MS - 1), NOW), Some(Group::DoneToday));
        assert_eq!(group_of(&row("b", TaskStatus::Completed, DAY_MS), NOW), None);
        assert_eq!(group_of(&row("c", TaskStatus::Failed, 30 * DAY_MS), NOW), Some(Group::NeedsYou), "old failures still need a look");
    }

    #[test]
    fn groups_are_always_three_and_newest_first() {
        let s = Summary {
            digest: String::new(),
            runtime: String::new(),
            tasks: vec![row("old", TaskStatus::Building, 5000), row("new", TaskStatus::Building, 1000), row("ask", TaskStatus::AwaitingApproval, 9000)],
        };
        let g = grouped(&s, NOW);
        assert_eq!(g.iter().map(|(g, _)| *g).collect::<Vec<_>>(), Group::ALL);
        assert_eq!(g[0].1.len(), 1);
        assert_eq!(g[1].1.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["new", "old"]);
        assert!(g[2].1.is_empty());
        assert_eq!(first_session(&s, NOW).as_deref(), Some("ask"), "a window opens on what needs you");
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
