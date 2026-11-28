//! Kittens in the app, the words and the state (docs/knowledge/app-shell.md, "Kittens in the app"): which tasks are a thread's own
//! subagents, what a thread's row says about them, the card's status words, the live last line of a subagent, the breadcrumb.
//! The drawing is `subagents_view.rs`.
//!
//! A subagent is a task, but not a thread: it is never listed in the sidebar, Home or the Briefing and never notified about; it is
//! opened from its card or from the thread's waiting line, and read there.

use neko_protocol::{Part, PartBody, QuestionWith, Request, SubPlace, Task, TaskId, TaskStatus};
use rune::TextEdit;

use crate::state::App;

/// How long a card's live line may be, in characters.
const LINE_CHARS: usize = 120;

/// Whether a subagent is still going: queued, working, asking, or resting on subagents of its own.
pub fn is_live(s: TaskStatus) -> bool {
    matches!(s, TaskStatus::New | TaskStatus::Working | TaskStatus::NeedsYou | TaskStatus::Waiting)
}

/// "Kitten 1.2".
pub fn name(number: &str) -> String {
    format!("Kitten {number}")
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

/// The wake note's line: "Woken by Kitten 1 · done"; a question says itself: "Kitten 1 has a question".
pub fn wake_line(number: &str, status: &str) -> String {
    if status == "asked" {
        return format!("{} has a question", name(number));
    }
    format!("Woken by {} · {}", name(number), wake_word(status))
}

/// Who asks, as a "Needs you" question is led: "The agent has a question", or, for a kitten's question the agent passed up, "Kitten 1
/// asks (via the agent)" and "Kitten 1.2 asks (via Kitten 1 and the agent)".
pub fn question_says(kitten: Option<&str>, via: &[String]) -> String {
    let Some(k) = kitten else { return "The agent has a question".to_owned() };
    let mut through: Vec<String> = via.iter().map(|n| name(n)).collect();
    through.push("the agent".to_owned());
    format!("{} asks (via {})", name(k), names_sentence(&through))
}

/// Who a kitten's question is put to: its parent ("the agent" for a thread, "Kitten 1" for a kitten).
pub fn asked_whom(parent: Option<&Task>) -> String {
    parent.and_then(Task::subagent_name).unwrap_or_else(|| "the agent".to_owned())
}

/// The line a kitten's card says while it waits on an answer: "Asks the agent: Which database?", "Passed up: …" once its parent
/// passed it on, "Asks you (via the agent): …" once it reached the person.
pub fn question_line(task: &Task, parent: Option<&Task>) -> Option<String> {
    let q = task.subagent.as_ref()?.question.as_ref()?;
    let lead = match q.with {
        QuestionWith::Parent => format!("Asks {}", asked_whom(parent)),
        QuestionWith::Up => "Passed up".to_owned(),
        QuestionWith::Person => format!("Asks you (via {})", asked_whom(parent)),
    };
    Some(format!("{lead}: {}", q.text))
}

/// The word on a kitten's card and in Overview: its status, or Draft (written, not started) or Discarded (a draft thrown away).
pub fn kitten_word(t: &Task) -> &'static str {
    match t.subagent.as_ref() {
        Some(s) if s.draft => "Draft",
        Some(s) if s.discarded => "Discarded",
        _ => card_word(t.status),
    }
}

/// Where a kitten works, as its card says it: "Shared folder", or "Own copy · branch neko/4f2a-1".
pub fn place_words(t: &Task) -> String {
    match t.subagent.as_ref() {
        Some(s) if s.placement == SubPlace::OwnCopy => match &s.branch {
            Some(b) => format!("Own copy \u{b7} branch {b}"),
            None => "Own copy".to_owned(),
        },
        _ => "Shared folder".to_owned(),
    }
}

/// Where it stands in line: "After Kitten 1" for a stacked kitten that has not started (None otherwise).
pub fn order_words(t: &Task) -> Option<String> {
    let s = t.subagent.as_ref()?;
    let after = s.after.as_ref()?;
    (t.status == TaskStatus::New && !s.discarded).then(|| format!("After {}", name(after)))
}

/// How many kittens work and how many wait their turn, in a sentence that does not claim more than is true: "2 kittens working",
/// "1 kitten working, 1 waiting its turn", "2 kittens waiting their turn". None when there are none.
pub fn crowd_words(working: usize, queued: usize) -> Option<String> {
    let kittens = |n: usize| if n == 1 { "1 kitten".to_owned() } else { format!("{n} kittens") };
    match (working, queued) {
        (0, 0) => None,
        (w, 0) => Some(format!("{} working", kittens(w))),
        (0, 1) => Some("1 kitten waiting its turn".to_owned()),
        (0, q) => Some(format!("{q} kittens waiting their turn")),
        (w, 1) => Some(format!("{} working, 1 waiting its turn", kittens(w))),
        (w, q) => Some(format!("{} working, {q} waiting their turn", kittens(w))),
    }
}

/// The names a waiting line lists, as a sentence: "Kitten 1", "Kitten 1 and Kitten 2", "Kitten 1, Kitten 2 and Kitten 3".
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
            PartBody::Tool { name, summary, input, output, state, .. } => {
                Some(crate::task_view::step_phrase(name, summary, input, output.as_deref(), *state == neko_protocol::ToolState::Running, workdir, home))
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

    /// The ones still going (a draft is not: it waits for the person, not the thread).
    pub fn live_subagents(&self, parent: &TaskId) -> Vec<&Task> {
        self.subagents_of(parent).into_iter().filter(|t| t.is_live_subagent()).collect()
    }

    /// The drafts a thread wrote and the person has not started or thrown away.
    pub fn draft_subagents(&self, parent: &TaskId) -> Vec<&Task> {
        self.subagents_of(parent).into_iter().filter(|t| t.subagent.as_ref().is_some_and(|s| s.draft)).collect()
    }

    /// How many of the live ones work and how many wait their turn (queued, or after another).
    pub fn kitten_counts(&self, parent: &TaskId) -> (usize, usize) {
        let live = self.live_subagents(parent);
        let queued = live.iter().filter(|t| t.status == TaskStatus::New).count();
        (live.len() - queued, queued)
    }

    /// A task came in: a draft kitten gets a place to edit its prompt (kept as the person typed it), anything else loses it.
    pub(crate) fn draft_follows(&mut self, task: &Task) {
        if task.subagent.as_ref().is_some_and(|s| s.draft) {
            self.draft_edits.entry(task.id.clone()).or_insert_with(|| TextEdit::new(&task.words));
        } else {
            self.draft_edits.remove(&task.id);
            self.draft_saved.remove(&task.id);
        }
    }

    /// The prompt being typed in a draft's card (made from the draft's words if the window has none yet).
    pub fn draft_edit(&mut self, id: &TaskId) -> &mut TextEdit {
        let words = self.tasks.iter().find(|t| &t.id == id).map(|t| t.words.clone()).unwrap_or_default();
        self.draft_edits.entry(id.clone()).or_insert_with(|| TextEdit::new(&words))
    }

    /// Sends the prompt of a draft when it has changed from what the daemon has, at most about once a second for each.
    pub fn save_drafts(&mut self, now: f64, force: Option<&TaskId>) {
        let wanted: Vec<(TaskId, String)> = self
            .draft_edits
            .iter()
            .filter_map(|(id, e)| {
                let task = self.tasks.iter().find(|t| &t.id == id)?;
                let (sent, at) = self.draft_saved.get(id).cloned().unwrap_or_else(|| (task.words.clone(), f64::MIN));
                let due = force == Some(id) || now - at >= 0.8;
                (e.text != sent && !e.text.trim().is_empty() && due).then(|| (id.clone(), e.text.clone()))
            })
            .collect();
        for (id, prompt) in wanted {
            if self.send(Request::EditDraft { task: id.clone(), prompt: prompt.clone() }).is_some() {
                self.draft_saved.insert(id, (prompt, now));
            }
        }
    }

    /// Start on a draft's card: its prompt as it stands first, then the start.
    pub fn start_draft(&mut self, id: &TaskId) {
        self.save_drafts(0.0, Some(id));
        self.send(Request::StartDraft { task: id.clone() });
    }

    /// Discard on a draft's card.
    pub fn discard_draft(&mut self, id: &TaskId) {
        self.send(Request::DiscardDraft { task: id.clone() });
    }

    /// The thread's row's second line while it waits on, or has, live kittens (None when it has no use for one). Never says more are
    /// working than are: a kitten that waits its turn is counted so.
    pub fn thread_line(&self, task: &Task) -> Option<String> {
        if !task.is_thread() {
            return None;
        }
        let live = self.live_subagents(&task.id);
        let waiting = task.status == TaskStatus::Waiting;
        let (working, queued) = self.kitten_counts(&task.id);
        let first = live.first().and_then(|t| t.subagent_name());
        match (live.len(), first, waiting) {
            (0, _, true) => Some("Reading the reports".to_owned()),
            (0, ..) => {
                let drafts = self.draft_subagents(&task.id).len();
                (drafts > 0).then(|| if drafts == 1 { "1 draft kitten".to_owned() } else { format!("{drafts} draft kittens") })
            }
            (1, Some(n), true) if queued == 0 => Some(format!("Waiting on {n}")),
            (1, Some(n), false) if queued == 0 => Some(format!("{n} working")),
            (1, Some(n), true) => Some(format!("{n} waiting its turn")),
            _ => crowd_words(working, queued),
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

    /// The band's breadcrumb for a subagent: the thread (its task and title), then each subagent down to this one ("Kitten 1",
    /// "Kitten 1.2"), each with its task when the window has it.
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

    /// The breadcrumb as one line ("Fix login / Kitten 1").
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
        assert_eq!(wake_line("1", "done"), "Woken by Kitten 1 · done");
        assert_eq!(wake_line("1.2", "asked"), "Kitten 1.2 has a question");
        assert_eq!(question_says(None, &[]), "The agent has a question");
        assert_eq!(question_says(Some("1"), &[]), "Kitten 1 asks (via the agent)");
        assert_eq!(question_says(Some("1.2"), &["1".into()]), "Kitten 1.2 asks (via Kitten 1 and the agent)");
        assert_eq!(names_sentence(&["Kitten 1".into(), "Kitten 2".into()]), "Kitten 1 and Kitten 2");
        assert_eq!(names_sentence(&["Kitten 1".into(), "Kitten 2".into(), "Kitten 3".into()]), "Kitten 1, Kitten 2 and Kitten 3");
        assert!(order_of("1.10") > order_of("1.2"));
    }

    #[test]
    fn a_crowd_is_said_as_it_is() {
        assert_eq!(crowd_words(0, 0), None);
        assert_eq!(crowd_words(1, 0).as_deref(), Some("1 kitten working"));
        assert_eq!(crowd_words(2, 0).as_deref(), Some("2 kittens working"));
        assert_eq!(crowd_words(1, 1).as_deref(), Some("1 kitten working, 1 waiting its turn"));
        assert_eq!(crowd_words(3, 2).as_deref(), Some("3 kittens working, 2 waiting their turn"));
        assert_eq!(crowd_words(0, 1).as_deref(), Some("1 kitten waiting its turn"));
        assert_eq!(crowd_words(0, 3).as_deref(), Some("3 kittens waiting their turn"));
    }
}
