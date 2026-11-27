//! The words and the order of the sidebar's threads (docs/knowledge/threads.md): how a thread's age reads, which line goes under its
//! title, how rows are ordered inside a group, and how the groups are made. No drawing here, so each rule is a plain test.

use neko_protocol::{Need, NeedsItem, Task, TaskId, ThreadStatus, Workspace};

use crate::state::{App, SECTION_ROWS};

/// How the Threads section is grouped (View ▸ Group by).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Grouping {
    /// One group per project, No project last.
    #[default]
    Project,
    /// One list by last activity.
    None,
}

impl Grouping {
    /// The word kept in the prefs.
    pub fn key(self) -> &'static str {
        match self {
            Grouping::Project => "project",
            Grouping::None => "none",
        }
    }

    pub fn from_key(k: &str) -> Grouping {
        if k == "none" { Grouping::None } else { Grouping::Project }
    }
}

/// One group of the Threads section: a project (its id is the workspace's) or, ungrouped, the one list.
pub struct Group<'a> {
    /// The workspace's id, or [`ALL`] (or [`PINNED`], [`ARCHIVED`]).
    pub id: String,
    pub name: String,
    pub workspace: Option<&'a Workspace>,
    /// The threads that are in the list: not pinned (those are in Pinned), not settled, not archived.
    pub threads: Vec<&'a Task>,
    /// The ones the person settled or a week put away: in the group's Settled fold.
    pub settled: Vec<&'a Task>,
}

/// The id of the one list when the section is not grouped.
pub const ALL: &str = "all";
/// The id of the Pinned group, the first of the Threads (any project).
pub const PINNED: &str = "pinned";
/// The id of the Archived fold View ▸ Show archived adds at the end of the Threads.
pub const ARCHIVED: &str = "archived";

/// What a thread's second line says, and in which ink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    /// `Needs you: <short title>`, amber.
    NeedsYou(String),
    /// `Failed: <reason>`, red.
    Failed(String),
    /// The agent's last line, or a plain word for a thread that has said nothing yet, in the second ink.
    Quiet(String),
}

impl Line {
    /// The line as it is drawn and read: with its "Needs you: " or "Failed: " in front.
    pub fn spoken(&self) -> String {
        match self {
            Line::NeedsYou(w) => format!("Needs you: {w}"),
            Line::Failed(w) => format!("Failed: {w}"),
            Line::Quiet(w) => w.clone(),
        }
    }

    pub fn words(&self) -> &str {
        match self {
            Line::NeedsYou(w) | Line::Failed(w) | Line::Quiet(w) => w,
        }
    }
}

/// What a request or question says, short, for "Needs you: …". A subagent's request says whose it is ("Run a command · Kitten 1").
pub fn need_words(item: &NeedsItem) -> String {
    let n = &item.need;
    match n {
        Need::Permission { permission } => match &item.subagent {
            Some(number) => format!("{} · {}", permission.title, crate::subagents::name(number)),
            None => permission.title.clone(),
        },
        Need::Question { .. } => "a question".to_owned(),
        Need::Failure { .. } => "could not go on".to_owned(),
        Need::Plugin { .. } => "a plugin".to_owned(),
    }
}

/// The line under a thread's title: the first that applies of Needs you, Failed, what it waits on ("Waiting on Kitten 1", "2
/// kittens working": `subagents`, from [`App::thread_line`]), then the agent's latest line. A thread that has said nothing yet says
/// what it is.
pub fn second_line(task: &Task, need: Option<&NeedsItem>, subagents: Option<&str>) -> Line {
    if task.needs_you {
        return Line::NeedsYou(need.map_or_else(|| "waiting for you".to_owned(), |n| need_words(n)));
    }
    if task.thread_status == ThreadStatus::Failed {
        return Line::Failed(if task.failure_line.is_empty() { "the last run did not finish".to_owned() } else { task.failure_line.clone() });
    }
    if let Some(w) = subagents {
        return Line::Quiet(w.to_owned());
    }
    if !task.latest_line.is_empty() {
        return Line::Quiet(task.latest_line.clone());
    }
    Line::Quiet(
        match (task.thread_status, task.status) {
            (ThreadStatus::Working, _) => "Working…",
            (_, neko_protocol::TaskStatus::New) => "Not started",
            (_, neko_protocol::TaskStatus::Failed) => "The last run did not finish",
            (_, neko_protocol::TaskStatus::Stopped) => "Stopped",
            (_, neko_protocol::TaskStatus::Interrupted) => "Interrupted",
            _ => "Done",
        }
        .to_owned(),
    )
}

/// The thread's state in words, for assistive technology (the mark and the bold title carry it to the eye).
pub fn state_words(task: &Task) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if task.needs_you {
        parts.push("needs you");
    }
    match task.thread_status {
        ThreadStatus::Working => parts.push("working"),
        ThreadStatus::Failed => parts.push("failed"),
        ThreadStatus::Waiting => parts.push("waiting"),
        ThreadStatus::Idle => {}
    }
    match task.status {
        neko_protocol::TaskStatus::Stopped => parts.push("stopped"),
        neko_protocol::TaskStatus::Interrupted => parts.push("interrupted"),
        _ => {}
    }
    if task.pinned {
        parts.push("pinned");
    }
    if task.settled {
        parts.push("settled");
    }
    if task.archived {
        parts.push("archived");
    }
    if task.unread() {
        parts.push("unread");
    }
    parts.join(", ")
}

/// How long ago, in as few characters as read: "now", "2m", "3h", the weekday within the week ("Mon"), else "Oct 3". `utc_offset_s`
/// is the local zone's offset from UTC.
pub fn age(now_ms: i64, utc_offset_s: i64, then_ms: i64) -> String {
    let minutes = (now_ms - then_ms).max(0) / 60_000;
    if minutes < 1 {
        return "now".to_owned();
    }
    if minutes < 60 {
        return format!("{minutes}m");
    }
    if minutes < 24 * 60 {
        return format!("{}h", minutes / 60);
    }
    let day = |ms: i64| (ms / 1000 + utc_offset_s).div_euclid(86_400);
    let ago = day(now_ms) - day(then_ms);
    // 1970-01-01 was a Thursday.
    let weekday = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][day(then_ms).rem_euclid(7) as usize];
    if ago < 7 {
        return weekday.to_owned();
    }
    let (_, month, d) = civil(day(then_ms));
    format!("{} {d}", ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][month as usize - 1])
}

/// (year, month, day) of a day count since 1970-01-01.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

impl App {
    /// The time a thread is placed by: its last activity, but held while it works, so a row does not move under the pointer as its
    /// words stream (the time it had when it began to work); it moves when the work ends.
    pub fn sort_ms(&self, t: &Task) -> i64 {
        self.sort_hold.get(&t.id).copied().unwrap_or(t.activity_ms)
    }

    /// Threads in a group's order: those that need the person first, then the rest by last activity (newest first).
    pub fn ordered<'a>(&self, mut threads: Vec<&'a Task>) -> Vec<&'a Task> {
        threads.sort_by(|a, b| (!a.needs_you, std::cmp::Reverse(self.sort_ms(a)), std::cmp::Reverse(&a.id)).cmp(&(!b.needs_you, std::cmp::Reverse(self.sort_ms(b)), std::cmp::Reverse(&b.id))));
        threads
    }

    /// The threads of the section that are listed in a group: pinned ones are in Pinned, archived ones are out of the lists.
    fn listed(&self, of: impl Fn(&Task) -> bool) -> (Vec<&Task>, Vec<&Task>) {
        let mine = |t: &&Task| t.is_thread() && !t.pinned && !t.archived && of(t);
        let all: Vec<&Task> = self.tasks.iter().filter(mine).collect();
        let (settled, active): (Vec<&Task>, Vec<&Task>) = all.into_iter().partition(|t| t.settled);
        (self.ordered(active), self.ordered(settled))
    }

    /// The Threads section's groups. Grouped by project: the projects as the person ordered them, the removed ones that still have
    /// threads, No project last; a project with no threads is still a group (its heading is how a thread is started in it). Not
    /// grouped: one list, with no heading of its own. Pinned and archived threads are in neither (see [`App::pinned_group`]).
    pub fn thread_groups(&self) -> Vec<Group<'_>> {
        match self.grouping {
            Grouping::None => {
                let (threads, settled) = self.listed(|_| true);
                vec![Group { id: ALL.to_owned(), name: "Threads".to_owned(), workspace: None, threads, settled }]
            }
            Grouping::Project => self
                .sections()
                .into_iter()
                .map(|w| {
                    let (threads, settled) = self.listed(|t| t.workspace == w.id);
                    Group { id: w.id.clone(), name: self.project_name(&w), workspace: self.workspaces.iter().find(|x| x.id == w.id), threads, settled }
                })
                .collect(),
        }
    }

    /// The Pinned group: every pinned thread of any project (archived ones are out), those that need the person first. None when
    /// nothing is pinned.
    pub fn pinned_group(&self) -> Option<Group<'_>> {
        let threads = self.ordered(self.tasks.iter().filter(|t| t.is_thread() && t.pinned && !t.archived).collect());
        (!threads.is_empty()).then(|| Group { id: PINNED.to_owned(), name: "Pinned".to_owned(), workspace: None, threads, settled: Vec::new() })
    }

    /// The archived threads, newest activity first, for the fold View ▸ Show archived adds. None when there are none.
    pub fn archived_group(&self) -> Option<Group<'_>> {
        let threads = self.ordered(self.tasks.iter().filter(|t| t.is_thread() && t.archived).collect());
        (!threads.is_empty()).then(|| Group { id: ARCHIVED.to_owned(), name: "Archived".to_owned(), workspace: None, threads, settled: Vec::new() })
    }

    /// The rows a group shows: all of them, or the newest [`SECTION_ROWS`] until "Show more" is pressed.
    pub fn shown_rows<'a>(&self, g: &Group<'a>) -> Vec<&'a Task> {
        if self.show_all.contains(&g.id) || g.threads.len() <= SECTION_ROWS + 1 { g.threads.clone() } else { g.threads.iter().take(SECTION_ROWS).copied().collect() }
    }

    /// Whether a group's Settled fold shows its threads: pressed open, or View ▸ Show settled is on.
    pub fn settled_shown(&self, group: &str) -> bool {
        self.show_settled || self.settled_open.contains(group)
    }

    /// The threads in the order the sidebar draws them, for ⌥1 to ⌥9 and ⌘⌥↑ / ⌘⌥↓: Pinned, then each group that is not folded
    /// (its rows, then its settled ones when that fold is open), then the archived ones when they are shown.
    pub fn thread_order(&self) -> Vec<TaskId> {
        let mut out: Vec<TaskId> = Vec::new();
        if let Some(p) = self.pinned_group() {
            out.extend(self.shown_rows(&p).into_iter().map(|t| t.id.clone()));
        }
        for g in self.thread_groups() {
            if g.workspace.is_some() && self.folded.contains(&g.id) {
                continue;
            }
            out.extend(self.shown_rows(&g).into_iter().map(|t| t.id.clone()));
            if self.settled_shown(&g.id) {
                out.extend(g.settled.iter().map(|t| t.id.clone()));
            }
        }
        if self.show_archived
            && let Some(a) = self.archived_group()
            && self.settled_open.contains(ARCHIVED)
        {
            out.extend(a.threads.iter().map(|t| t.id.clone()));
        }
        out
    }

    /// A project's name as the person reads it: General is "No project".
    pub fn project_name(&self, w: &Workspace) -> String {
        if w.is_general() { "No project".to_owned() } else { w.name.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::{TaskId, TaskStatus};

    fn t(id: &str, activity: i64) -> Task {
        let mut t = Task::blank(TaskId(id.into()), "default", "words", id, 1, None);
        t.activity_ms = activity;
        t.last_read_ms = activity;
        t
    }

    #[test]
    fn an_age_reads_in_as_few_characters_as_it_can() {
        let now = 1_764_000_000_000; // Mon 2025-11-24 16:00 UTC
        let m = 60_000;
        assert_eq!(age(now, 0, now - 20_000), "now");
        assert_eq!(age(now, 0, now - 2 * m), "2m");
        assert_eq!(age(now, 0, now - 59 * m), "59m");
        assert_eq!(age(now, 0, now - 3 * 60 * m), "3h");
        assert_eq!(age(now, 0, now - 2 * 24 * 60 * m), "Sat");
        assert_eq!(age(now, 0, now - 30 * 24 * 60 * m), "Oct 25");
    }

    #[test]
    fn needs_you_comes_first_then_by_last_activity_and_a_working_row_keeps_its_place() {
        let mut a = App::default();
        let mut asks = t("asks", 10);
        asks.needs_you = true;
        asks.status = TaskStatus::Asked;
        let mut working = t("working", 50);
        working.status = TaskStatus::Working;
        working.thread_status = ThreadStatus::Working;
        let newer = t("newer", 90);
        let older = t("older", 20);
        a.tasks = vec![older.clone(), newer.clone(), working.clone(), asks.clone()];
        let order = |a: &App| a.ordered(a.tasks.iter().collect()).iter().map(|t| t.id.0.clone()).collect::<Vec<_>>();
        assert_eq!(order(&a), ["asks", "newer", "working", "older"]);
        // Its words stream: its activity moves on, its place does not.
        a.sort_hold.insert(working.id.clone(), 50);
        a.tasks[2].activity_ms = 200;
        assert_eq!(order(&a), ["asks", "newer", "working", "older"]);
        // The work ends: it takes the time it ended.
        a.sort_hold.remove(&working.id);
        assert_eq!(order(&a), ["asks", "working", "newer", "older"]);
    }

    #[test]
    fn the_second_line_is_the_first_that_applies() {
        let mut task = t("a", 1);
        task.latest_line = "Renamed the label.".into();
        assert_eq!(second_line(&task, None, None), Line::Quiet("Renamed the label.".into()));
        assert_eq!(second_line(&task, None, Some("Waiting on Kitten 1")), Line::Quiet("Waiting on Kitten 1".into()), "what it waits on comes before its last words");
        task.thread_status = ThreadStatus::Failed;
        task.failure_line = "Claude Code stopped unexpectedly.".into();
        assert_eq!(second_line(&task, None, Some("2 kittens working")), Line::Failed("Claude Code stopped unexpectedly.".into()));
        task.needs_you = true;
        assert_eq!(second_line(&task, None, None), Line::NeedsYou("waiting for you".into()), "Needs you comes before Failed");
        let mut quiet = t("b", 1);
        assert_eq!(second_line(&quiet, None, None), Line::Quiet("Not started".into()));
        quiet.status = TaskStatus::Working;
        quiet.thread_status = ThreadStatus::Working;
        assert_eq!(second_line(&quiet, None, None), Line::Quiet("Working…".into()));
    }

    #[test]
    fn a_thread_says_its_state_in_words() {
        let mut task = t("a", 5);
        assert_eq!(state_words(&task), "");
        task.last_read_ms = 1;
        task.thread_status = ThreadStatus::Working;
        task.needs_you = true;
        assert_eq!(state_words(&task), "needs you, working, unread");
        task.thread_status = ThreadStatus::Waiting;
        task.needs_you = false;
        assert_eq!(state_words(&task), "waiting, unread");
    }
}
