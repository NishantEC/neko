//! The words and the order of the sidebar's threads (docs/knowledge/threads.md): how a thread's age reads, which line goes under its
//! title, how rows are ordered inside a group, and how the groups are made. No drawing here, so each rule is a plain test.

use neko_protocol::{Need, NeedsItem, Task, ThreadStatus, Workspace};

use crate::state::App;

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
    /// The workspace's id, or [`ALL`].
    pub id: String,
    pub name: String,
    pub workspace: Option<&'a Workspace>,
    pub threads: Vec<&'a Task>,
}

/// The id of the one list when the section is not grouped.
pub const ALL: &str = "all";

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

/// What a request or question says, short, for "Needs you: …".
pub fn need_words(n: &Need) -> String {
    match n {
        Need::Permission { permission } => permission.title.clone(),
        Need::Question { .. } => "a question".to_owned(),
        Need::Failure { .. } => "could not go on".to_owned(),
        Need::Plugin { .. } => "a plugin".to_owned(),
    }
}

/// The line under a thread's title: the first that applies of Needs you, Failed, then the agent's latest line (Waiting on subagents
/// and "n subagents working" come before the latest line when subagents exist). A thread that has said nothing yet says what it is.
pub fn second_line(task: &Task, need: Option<&NeedsItem>) -> Line {
    if task.needs_you {
        return Line::NeedsYou(need.map_or_else(|| "waiting for you".to_owned(), |n| need_words(&n.need)));
    }
    if task.thread_status == ThreadStatus::Failed {
        return Line::Failed(if task.failure_line.is_empty() { "the last run did not finish".to_owned() } else { task.failure_line.clone() });
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
        ThreadStatus::Idle => {}
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

    /// The Threads section's groups. Grouped by project: the projects as the person ordered them, the removed ones that still have
    /// threads, No project last; a project with no threads is still a group (its heading is how a thread is started in it). Not
    /// grouped: one list, with no heading of its own.
    pub fn thread_groups(&self) -> Vec<Group<'_>> {
        match self.grouping {
            Grouping::None => vec![Group { id: ALL.to_owned(), name: "Threads".to_owned(), workspace: None, threads: self.ordered(self.tasks.iter().collect()) }],
            Grouping::Project => self
                .sections()
                .into_iter()
                .map(|w| {
                    let threads = self.ordered(self.tasks.iter().filter(|t| t.workspace == w.id).collect());
                    Group { id: w.id.clone(), name: self.project_name(&w), workspace: self.workspaces.iter().find(|x| x.id == w.id), threads }
                })
                .collect(),
        }
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
        let mut t = Task::blank(TaskId(id.into()), "default", "words", id, 1, "cat_miso");
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
        assert_eq!(second_line(&task, None), Line::Quiet("Renamed the label.".into()));
        task.thread_status = ThreadStatus::Failed;
        task.failure_line = "Claude Code stopped unexpectedly.".into();
        assert_eq!(second_line(&task, None), Line::Failed("Claude Code stopped unexpectedly.".into()));
        task.needs_you = true;
        assert_eq!(second_line(&task, None), Line::NeedsYou("waiting for you".into()), "Needs you comes before Failed");
        let mut quiet = t("b", 1);
        assert_eq!(second_line(&quiet, None), Line::Quiet("Not started".into()));
        quiet.status = TaskStatus::Working;
        quiet.thread_status = ThreadStatus::Working;
        assert_eq!(second_line(&quiet, None), Line::Quiet("Working…".into()));
    }

    #[test]
    fn a_thread_says_its_state_in_words() {
        let mut task = t("a", 5);
        assert_eq!(state_words(&task), "");
        task.last_read_ms = 1;
        task.thread_status = ThreadStatus::Working;
        task.needs_you = true;
        assert_eq!(state_words(&task), "needs you, working, unread");
    }
}
