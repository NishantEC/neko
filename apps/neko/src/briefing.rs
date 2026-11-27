//! The Briefing in the app (docs/knowledge/briefing.md): the words that say what happened, and what each press sends. The daemon
//! builds the briefing from its records; this file only says it in words and keeps the window's side of it (the page, "Mark as
//! read", the morning settings, the notification).

use neko_protocol::briefing::{FinishedEnd, RoundOutcome};
use neko_protocol::schedule::{Zone, format_at, local_parts, parse_at};
use neko_protocol::{Briefing, BriefingCounts, BriefingSettings, Changes, Need, Request, RunId, TaskId, TaskStatus};
use rune::TextEdit;

use crate::state::{App, Asked};

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// A moment as a person says it next to a time of day, looking back: "today 02:10", "yesterday 18:00", "Wednesday 18:00", then
/// "20 Nov 18:00".
pub fn when(ms: i64, now_ms: i64, zone: &dyn Zone) -> String {
    let (at, now) = (local_parts(zone, ms), local_parts(zone, now_ms));
    let time = format!("{:02}:{:02}", at.hour, at.minute);
    match now.day_number - at.day_number {
        0 => format!("today {time}"),
        1 => format!("yesterday {time}"),
        2..=6 => format!("{} {time}", at.weekday.name()),
        _ => format!("{} {} {time}", at.day, MONTHS[(at.month - 1) as usize]),
    }
}

/// Where a window began, for "Since …": a time of day when it is today ("08:30"), else the day and the time.
pub fn since_phrase(ms: i64, now_ms: i64, zone: &dyn Zone) -> String {
    let w = when(ms, now_ms, zone);
    w.strip_prefix("today ").map_or(w.clone(), str::to_owned)
}

/// The heading: "Since yesterday 18:00".
pub fn heading(b: &Briefing, now_ms: i64, zone: &dyn Zone) -> String {
    format!("Since {}", since_phrase(b.from_ms, now_ms, zone))
}

/// The line under the heading: what there is ("3 finished, 1 needs you"), or that there is nothing.
pub fn summary(b: &Briefing, now_ms: i64, zone: &dyn Zone) -> String {
    let c = b.counts();
    let words = c.headline();
    if c.news && !words.is_empty() {
        return words;
    }
    let since = since_phrase(b.from_ms, now_ms, zone);
    if !c.news && c.needs_you > 0 {
        return format!("Nothing new since {since}.");
    }
    format!("Nothing happened since {since}.")
}

/// How a finished task ended, after its cat's name.
pub fn end_words(e: FinishedEnd) -> &'static str {
    match e {
        FinishedEnd::Done => "finished",
        FinishedEnd::Failed => "could not finish",
        FinishedEnd::Stopped => "was stopped",
        FinishedEnd::Interrupted => "was interrupted",
    }
}

/// "3 files", "1 file".
pub fn files(n: u32) -> String {
    format!("{n} file{}", if n == 1 { "" } else { "s" })
}

/// What the cost line says: "$1.84". Under a cent it says so.
pub fn dollars(usd: f64) -> String {
    if usd > 0.0 && usd < 0.005 { "under $0.01".to_owned() } else { format!("${usd:.2}") }
}

/// The totals in words, without the figures: "5 tasks finished", "14 files changed".
pub fn totals_words(b: &Briefing) -> (String, Option<String>, Option<String>) {
    let t = &b.totals;
    let tasks = format!("{} thread{} finished", t.finished, if t.finished == 1 { "" } else { "s" });
    let changed = (t.files > 0).then(|| format!("{} changed", files(t.files)));
    let cost = t.cost_usd.map(|c| format!("{} as Claude Code reported it", dollars(c)));
    (tasks, changed, cost)
}

/// The note under a cost that is not money charged.
pub const API_EQUIVALENT: &str = "API-equivalent, not charged on a subscription.";

/// A reason in lower case without its full stop ("the last run is still working"); a name keeps its capital.
pub fn reason_words(r: &str) -> String {
    let r = r.trim().trim_end_matches('.');
    match r.split_once(' ') {
        Some((first, rest)) if ["The", "Every", "It", "A", "An", "This"].contains(&first) => format!("{} {rest}", first.to_lowercase()),
        _ => r.to_owned(),
    }
}

/// How a round's task stands, in a word or two.
pub fn task_words(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::New | TaskStatus::Working => "working",
        TaskStatus::NeedsYou | TaskStatus::Asked => "waiting for you",
        TaskStatus::Waiting => "waiting on its kittens",
        TaskStatus::Done => "done",
        TaskStatus::Failed => "could not finish",
        TaskStatus::Stopped => "stopped",
        TaskStatus::Interrupted => "was interrupted",
    }
}

/// A round's line, in two parts: what it did ("Docs check ran") and what came of it ("done", "the last run is still working").
pub fn round_line(l: &neko_protocol::briefing::BriefingRound) -> (String, Option<String>) {
    let what = match l.outcome {
        RoundOutcome::Ran => format!("{} ran", l.name),
        RoundOutcome::Skipped => format!("{} was skipped", l.name),
        RoundOutcome::Missed => format!("{} was missed", l.name),
        RoundOutcome::Braked => format!("{} turned itself off", l.name),
    };
    let after = match l.outcome {
        RoundOutcome::Ran => l.task_status.map(|s| task_words(s).to_owned()),
        RoundOutcome::Braked => l.reason.as_deref().map(|r| reason_words(r.trim_start_matches("Turned itself off ")).to_owned()),
        _ => l.reason.as_deref().map(reason_words),
    };
    (what, after)
}

/// The notification's body: one line of what is first, the thing that needs the person else the newest finished task.
pub fn note_body(b: &Briefing) -> String {
    if let Some(n) = b.needs.first() {
        let what = match &n.need {
            Need::Permission { permission } => format!("{}: {}", permission.title, permission.detail),
            Need::Question { question, .. } => question.clone(),
            Need::Failure { text, .. } => text.clone(),
            Need::Plugin { ask } => crate::plugins::note_words(ask),
        };
        // A plugin's item belongs to no cat: its words stand alone.
        if matches!(n.need, Need::Plugin { .. }) {
            return what;
        }
        let verb = match n.need {
            Need::Failure { .. } => "could not go on",
            _ => "asks",
        };
        return format!("{} {verb}: {what}", n.cat_name);
    }
    match b.finished.first() {
        Some(t) => format!("{} {}: {}", t.cat_name, end_words(t.end), t.title),
        None => String::new(),
    }
}

/// The workspaces that have anything in the briefing, in the order they first appear (needs, finished, rounds, working).
pub fn active_workspaces<'a>(b: &'a Briefing, needs: &'a [&neko_protocol::NeedsItem]) -> Vec<&'a str> {
    let mut out: Vec<&str> = Vec::new();
    let all = needs.iter().map(|n| n.workspace.as_str()).chain(b.finished.iter().map(|t| t.workspace.as_str())).chain(b.rounds.iter().map(|r| r.workspace.as_str())).chain(b.working.iter().map(|w| w.workspace.as_str()));
    for w in all {
        if !out.contains(&w) {
            out.push(w);
        }
    }
    out
}

/// Groups `items` by workspace, in the order each workspace first appears.
pub fn group_by<'a, T>(items: &'a [T], workspace: impl Fn(&T) -> &str) -> Vec<(String, Vec<&'a T>)> {
    let mut out: Vec<(String, Vec<&T>)> = Vec::new();
    for i in items {
        let w = workspace(i);
        match out.iter_mut().find(|(k, _)| k == w) {
            Some((_, v)) => v.push(i),
            None => out.push((w.to_owned(), vec![i])),
        }
    }
    out
}

/// The "Next briefing" line under a caught-up page: "Your next briefing is tomorrow at 08:00", or that mornings are off.
pub fn next_words(b: &Briefing, settings: Option<&BriefingSettings>, now_ms: i64, zone: &dyn Zone) -> String {
    match (settings.map(|s| s.morning_on), b.next_morning_ms) {
        (Some(false), _) => "Morning briefings are off. Turn them on in Settings.".to_owned(),
        (_, Some(ms)) => format!("Your next briefing is {}.", crate::rounds::when_at(ms, now_ms, zone)),
        _ => String::new(),
    }
}

impl App {
    /// The briefing, as the daemon last said, and its morning settings.
    pub fn set_briefing(&mut self, b: Briefing) {
        self.briefing = Some(b);
    }

    pub fn set_briefing_settings(&mut self, s: BriefingSettings) {
        // The field follows the daemon, except while the person is typing in it.
        if !self.briefing_time.focused {
            self.briefing_time = TextEdit::new(&s.morning_at);
        }
        self.briefing_settings = Some(s);
    }

    /// The morning time came and the window has news: the notification (the daemon says when; the app decides whether it is worth
    /// posting, as it does for Needs you).
    pub fn briefing_due(&mut self, counts: &BriefingCounts) {
        let body = self.briefing.as_ref().map(note_body).unwrap_or_default();
        let here = self.here();
        self.notifier.briefing(counts, &body, &here);
    }

    /// The briefing has something new to say (Home's line and the sidebar's mark): News, and not read away.
    pub fn briefing_news(&self) -> Option<BriefingCounts> {
        self.briefing.as_ref().map(Briefing::counts).filter(|c| c.news)
    }

    /// The Briefing page (⌘4, the sidebar's row, `neko://briefing`).
    pub fn open_briefing(&mut self) {
        if !self.briefing_open {
            self.briefing_back = self.open.clone().filter(|_| !self.settings_open && !self.needs_open && !self.rounds_open);
        }
        self.go_home();
        self.briefing_open = true;
        self.notifier.briefing_opened();
        // The window may have been away: what happened since is asked for again.
        self.send(Request::GetBriefing { from_ms: None, to_ms: None });
        self.focus_next = self.first_briefing_stop();
        self.focus_quiet = true;
    }

    /// Escape on the Briefing: back to the task the person came from, else Home.
    pub fn leave_briefing(&mut self) {
        match self.briefing_back.take().filter(|t| self.tasks.iter().any(|x| &x.id == t)) {
            Some(t) => self.open_task_id(&t),
            None => self.go_home(),
        }
    }

    /// The first thing the keyboard goes to when the page opens: its first card, else "Mark as read".
    fn first_briefing_stop(&self) -> Option<String> {
        let b = self.briefing.as_ref()?;
        if let Some(n) = self.visible_needs().first() {
            return Some(crate::state::need_key(&n.id));
        }
        if let Some(t) = b.finished.first() {
            return Some(task_key(&t.task));
        }
        Some("briefing-read".to_owned())
    }

    /// "Mark as read" (⌘↩): the next window starts now.
    pub fn mark_briefing_read(&mut self) {
        if self.briefing.is_none() {
            return;
        }
        if let Some(id) = self.send(Request::MarkBriefingRead) {
            self.asked.insert(id, Asked::BriefingRead);
        }
        self.notifier.briefing_opened();
    }

    /// The daemon marked it read: the page says so, and the keyboard goes to the sidebar's row.
    pub fn caught_up(&mut self) {
        self.announce = Some(("You're caught up.".to_owned(), false));
        if self.briefing_open {
            self.focus_next = Some("briefing".to_owned());
        }
    }

    /// Review on a finished task's row: the task opens at the run's files.
    pub fn review_from_briefing(&mut self, task: &TaskId, run: &RunId) {
        self.open_task_id(task);
        self.open_review(task, Some(run.clone()));
    }

    /// The morning time typed in Settings ▸ Briefing, as the daemon will keep it (None while it is not a time of day).
    pub fn typed_morning(&self) -> Option<String> {
        parse_at(self.briefing_time.text.trim()).map(format_at)
    }

    /// Saves the morning time and its switch.
    pub fn save_briefing_settings(&mut self, on: Option<bool>) {
        let Some(cur) = self.briefing_settings.clone() else { return };
        let at = if on.is_some() { cur.morning_at.clone() } else { self.typed_morning().unwrap_or_else(|| cur.morning_at.clone()) };
        let settings = BriefingSettings { morning_on: on.unwrap_or(cur.morning_on), morning_at: at };
        if let Some(id) = self.send(Request::SetBriefingSettings { settings }) {
            self.asked.insert(id, Asked::BriefingSettings);
        }
        self.briefing_time.focused = false;
    }
}

/// The key of a finished task's row.
pub fn task_key(task: &TaskId) -> String {
    format!("briefing-task-{}", task.0)
}

/// A task's changes as one phrase for assistive technology: "3 files, 42 lines added, 7 removed".
pub fn changes_label(c: Changes) -> String {
    format!("{}, {} lines added, {} removed", files(c.files), c.added, c.removed)
}
