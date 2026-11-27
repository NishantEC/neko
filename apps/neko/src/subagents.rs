//! Subagents in the app, the words and the state (docs/knowledge/app-shell.md, "Subagents in the app"): which tasks are a thread's own
//! subagents, what a thread's row says about them, the card's status words, the live last line of a subagent, the breadcrumb.
//! The drawing is `subagents_view.rs`.
//!
//! A subagent is a task, but not a thread: it is never listed in the sidebar, Home or the Briefing and never notified about; it is
//! opened from its card or from the thread's waiting line, and read there.

use neko_protocol::{Part, PartBody, Request, Task, TaskId, TaskStatus};

use crate::state::App;

/// How long a card's live line may be, in characters.
const LINE_CHARS: usize = 120;

/// Whether a subagent is still going: queued, working, asking, or resting on subagents of its own.
pub fn is_live(s: TaskStatus) -> bool {
    matches!(s, TaskStatus::New | TaskStatus::Working | TaskStatus::NeedsYou | TaskStatus::Waiting)
}

/// "Subagent 1.2".
pub fn name(number: &str) -> String {
    format!("Subagent {number}")
}

/// The status word on a card.
pub fn card_word(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::New => "Queued",
        TaskStatus::Working => "Working",
        TaskStatus::NeedsYou => "Needs you",
        TaskStatus::Waiting => "Waiting",
        TaskStatus::Done => "Done",
        TaskStatus::Failed => "Failed",
        TaskStatus::Stopped => "Stopped",
        TaskStatus::Interrupted => "Interrupted",
        TaskStatus::Asked => "Has a question",
    }
}

/// How a wake note says what the report was: `done`, `failed`, `stopped`, `interrupted` or `asked` as words.
pub fn wake_word(status: &str) -> &str {
    match status {
        "asked" => "has a question",
        other => other,
    }
}

/// The wake note's line: "Woken by Subagent 1 · done".
pub fn wake_line(number: &str, status: &str) -> String {
    format!("Woken by {} · {}", name(number), wake_word(status))
}

/// The names a waiting line lists, as a sentence: "Subagent 1", "Subagent 1 and Subagent 2", "Subagent 1, Subagent 2 and Subagent 3".
pub fn names_sentence(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// A number's parts as figures, so "1.10" comes after "1.2".
fn order_of(number: &str) -> Vec<u32> {
    number.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// The first line of some words, cut to fit a card.
fn first_line(words: &str) -> Option<String> {
    let line = words.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(crate::task_view::cut_middle(line, LINE_CHARS))
}

/// What a subagent is doing, from its parts: the newest agent words (their first line), else the newest step's summary.
pub fn line_of(parts: &[Part], workdir: &str, home: Option<&str>) -> Option<String> {
    let said = parts.iter().rev().find_map(|p| match &p.body {
        PartBody::Agent { text, .. } => first_line(text),
        _ => None,
    });
    said.or_else(|| {
        parts.iter().rev().find_map(|p| match &p.body {
            PartBody::Tool { name, summary, input, state, .. } => {
                Some(crate::task_view::phrase(name, summary, input, *state == neko_protocol::ToolState::Running, workdir, home))
            }
            _ => None,
        })
    })
}

impl App {
    /// A thread's own subagents (the ones it started), in number order.
    pub fn subagents_of(&self, parent: &TaskId) -> Vec<&Task> {
        let mut v: Vec<&Task> = self.tasks.iter().filter(|t| t.subagent.as_ref().is_some_and(|s| &s.parent == parent)).collect();
        v.sort_by_key(|t| order_of(&t.subagent.as_ref().map_or("", |s| s.number.as_str())));
        v
    }

    /// The ones still going.
    pub fn live_subagents(&self, parent: &TaskId) -> Vec<&Task> {
        self.subagents_of(parent).into_iter().filter(|t| is_live(t.status)).collect()
    }

    /// The thread's row's second line while it waits on, or has, live subagents (None when it has no use for one).
    pub fn thread_line(&self, task: &Task) -> Option<String> {
        if !task.is_thread() {
            return None;
        }
        let live = self.live_subagents(&task.id);
        let waiting = task.status == TaskStatus::Waiting;
        let first = live.first().and_then(|t| t.subagent_name());
        match (live.len(), first, waiting) {
            (0, _, true) => Some("Reading the reports".to_owned()),
            (0, ..) => None,
            (1, Some(n), true) => Some(format!("Waiting on {n}")),
            (1, Some(n), false) => Some(format!("{n} working")),
            (n, ..) => Some(format!("{n} subagents working")),
        }
    }

    /// What a subagent is doing now, for its card: from the parts this window has of it (its events reach every screen; the whole
    /// transcript is asked once for a card the window met after the fact).
    pub fn subagent_line(&self, id: &TaskId) -> Option<String> {
        let b = self.books.get(id)?;
        let workdir = b.runs.iter().rev().find(|r| !r.workdir.is_empty()).map_or("", |r| r.workdir.as_str());
        line_of(&b.parts, workdir, self.home.as_deref())
    }

    /// Asks, once each, for the transcripts of the subagents a thread's cards show and this window has not read, so a card opened
    /// after a restart shows what its subagent said last.
    pub fn ask_subagent_lines(&mut self, thread: &TaskId) {
        let wanted: Vec<TaskId> = self
            .books
            .get(thread)
            .map(|b| b.parts.iter().filter_map(|p| if let PartBody::Subagent { subagent, .. } = &p.body { Some(subagent.clone()) } else { None }).collect())
            .unwrap_or_default();
        for id in wanted {
            if self.sub_asked.contains(&id) || self.books.get(&id).is_some_and(|b| b.loaded) {
                continue;
            }
            if self.send(Request::GetTranscript { task: id.clone() }).is_some() {
                self.sub_asked.insert(id);
            }
        }
    }

    /// Whether a thread's sidebar row is the open one: the thread is open, or one of its subagents is (it has no row of its own).
    pub fn row_is_open(&self, task: &Task) -> bool {
        self.open.as_ref().is_some_and(|o| o == &task.id || self.tasks.iter().any(|t| &t.id == o && t.subagent.as_ref().is_some_and(|s| s.root == task.id)))
    }

    /// The task a subagent belongs to (the thread, or the subagent above it).
    pub fn parent_of(&self, task: &Task) -> Option<&Task> {
        let p = &task.subagent.as_ref()?.parent;
        self.tasks.iter().find(|t| &t.id == p)
    }

    /// The thread a subagent belongs to, at the top.
    pub fn thread_of(&self, task: &Task) -> Option<&Task> {
        let root = &task.subagent.as_ref()?.root;
        self.tasks.iter().find(|t| &t.id == root)
    }

    /// The band's breadcrumb for a subagent: the thread (its task and title), then each subagent down to this one ("Subagent 1",
    /// "Subagent 1.2"), each with its task when the window has it.
    pub fn crumbs(&self, task: &Task) -> Vec<(Option<TaskId>, String)> {
        let Some(of) = &task.subagent else { return vec![(Some(task.id.clone()), task.title.clone())] };
        let thread = self.thread_of(task);
        let mut out = vec![(thread.map(|t| t.id.clone()), thread.map_or_else(|| "Thread".to_owned(), |t| t.title.clone()))];
        let parts: Vec<&str> = of.number.split('.').collect();
        for i in 1..=parts.len() {
            let number = parts[..i].join(".");
            let id = self.tasks.iter().find(|t| t.subagent.as_ref().is_some_and(|s| s.number == number && s.root == of.root)).map(|t| t.id.clone());
            out.push((id, name(&number)));
        }
        out
    }

    /// The breadcrumb as one line ("Fix login / Subagent 1").
    pub fn crumb_words(&self, task: &Task) -> String {
        self.crumbs(task).into_iter().map(|(_, w)| w).collect::<Vec<_>>().join(" / ")
    }

    /// Back from a subagent to the thread (or the subagent) it belongs to.
    pub fn open_parent(&mut self) {
        if let Some(p) = self.open_task().and_then(|t| self.parent_of(t)).map(|t| t.id.clone()) {
            self.open_task_id(&p);
        }
    }

    /// The subagent opened in the main view, if it is one.
    pub fn open_subagent(&self) -> Option<&Task> {
        self.open_task().filter(|t| t.subagent.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_for_a_card_and_a_note() {
        assert_eq!(card_word(TaskStatus::New), "Queued");
        assert_eq!(card_word(TaskStatus::Asked), "Has a question");
        assert_eq!(wake_line("1", "done"), "Woken by Subagent 1 · done");
        assert_eq!(wake_line("1.2", "asked"), "Woken by Subagent 1.2 · has a question");
        assert_eq!(names_sentence(&["Subagent 1".into(), "Subagent 2".into()]), "Subagent 1 and Subagent 2");
        assert_eq!(names_sentence(&["Subagent 1".into(), "Subagent 2".into(), "Subagent 3".into()]), "Subagent 1, Subagent 2 and Subagent 3");
        assert!(order_of("1.10") > order_of("1.2"));
    }
}
