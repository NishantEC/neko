//! Search (⌘K): the state behind the palette (docs/knowledge/search.md).
//!
//! The palette is rune-ui's Command. What the person typed is sent to the daemon as it changes (never from the view, never waiting:
//! the reply arrives on a later frame and the rows already there stay until it does), and the daemon's threads and kittens are shown
//! with the app's own lists (Cattains, projects, actions), which Command filters itself. A reply that is not for the text now in the
//! field is dropped.

use neko_protocol::{Found, FoundBy, PartId, Reply, Request, TaskId};
use rune::TextEdit;

use crate::state::App;

/// What the palette is asking for: the most the daemon is asked to return.
pub const LIMIT: u32 = 12;

/// Where to scroll once a thread has opened: a message of its transcript (or its first words, `part` None).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reveal {
    pub task: TaskId,
    pub part: Option<PartId>,
    pub seq: u64,
    /// Frames spent: the fold is opened first, the scroll comes a frame after.
    pub step: u8,
}

#[derive(Debug, Default)]
pub struct Palette {
    pub open: bool,
    pub query: TextEdit,
    /// The highlighted row among the rows shown.
    pub at: usize,
    /// What the daemon last found, and the text it found it for.
    pub hits: Vec<Found>,
    pub answered: Option<String>,
    /// The text last sent, and the request that is out for it (None: answered, or nothing asked yet).
    pub sent: Option<String>,
    pub in_flight: Option<u64>,
    /// Why a filter could not be read ("“x” is not a status…"), for the empty list to say.
    pub problem: Option<String>,
    /// How long the daemon said it took, for the tests and the QA rig.
    pub took_ms: Option<u32>,
    pub reveal: Option<Reveal>,
}

/// What a row of the palette stands for, in its key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Thread(TaskId),
    Kitten(TaskId),
    Cattain(String),
    Project(String),
    Action(&'static str),
}

impl Row {
    pub fn key(&self) -> String {
        match self {
            Row::Thread(id) => format!("thread:{}", id.0),
            Row::Kitten(id) => format!("kitten:{}", id.0),
            Row::Cattain(id) => format!("cattain:{id}"),
            Row::Project(id) => format!("project:{id}"),
            Row::Action(a) => format!("action:{a}"),
        }
    }

    pub fn of(key: &str) -> Option<Row> {
        let (kind, rest) = key.split_once(':')?;
        Some(match kind {
            "thread" => Row::Thread(TaskId(rest.to_owned())),
            "kitten" => Row::Kitten(TaskId(rest.to_owned())),
            "cattain" => Row::Cattain(rest.to_owned()),
            "project" => Row::Project(rest.to_owned()),
            "action" => Row::Action(ACTIONS.iter().find(|a| a.id == rest)?.id),
            _ => return None,
        })
    }
}

/// One action of the palette: a verb that is already somewhere in the menus.
pub struct Action {
    pub id: &'static str,
    pub label: &'static str,
    /// Words that find it without being shown.
    pub keywords: &'static str,
    /// Its key, when it has one.
    pub keys: Option<&'static str>,
}

/// The actions, in the order they are listed. Whether each is on offer now is [`App::action_offered`].
pub const ACTIONS: &[Action] = &[
    Action { id: "new-thread", label: "New thread", keywords: "start create compose", keys: Some("⌘ N") },
    Action { id: "needs-you", label: "Needs you", keywords: "waiting inbox questions requests", keys: Some("⌘ 2") },
    Action { id: "briefing", label: "Briefing", keywords: "while you were away summary", keys: Some("⌘ 4") },
    Action { id: "rounds", label: "Rounds", keywords: "schedule scheduled recurring", keys: Some("⌘ 3") },
    Action { id: "pane", label: "Toggle right pane", keywords: "details changes overview kittens", keys: Some("⌥ ⌘ B") },
    Action { id: "sidebar", label: "Toggle sidebar", keywords: "hide show threads", keys: Some("⌃ ⌘ S") },
    Action { id: "pause-rounds", label: "Pause all rounds", keywords: "stop schedule", keys: None },
    Action { id: "resume-rounds", label: "Resume all rounds", keywords: "start schedule unpause", keys: None },
    Action { id: "show-archived", label: "Show archived threads", keywords: "view put away sleeping", keys: None },
    Action { id: "hide-archived", label: "Hide archived threads", keywords: "view put away sleeping", keys: None },
    Action { id: "show-settled", label: "Show settled threads", keywords: "view put away", keys: None },
    Action { id: "hide-settled", label: "Fold settled threads", keywords: "view put away", keys: None },
    Action { id: "settings", label: "Settings", keywords: "preferences options", keys: Some("⌘ ,") },
    Action { id: "settings-0", label: "Settings: Agents", keywords: "claude runtime model permissions", keys: None },
    Action { id: "settings-1", label: "Settings: Projects", keywords: "folders workspaces undo storage", keys: None },
    Action { id: "settings-2", label: "Settings: Notifications", keywords: "alerts badge", keys: None },
    Action { id: "settings-3", label: "Settings: Cattains", keywords: "cats rules faces crew", keys: None },
    Action { id: "settings-4", label: "Settings: Briefing", keywords: "morning notification", keys: None },
    Action { id: "settings-5", label: "Settings: Plugins", keywords: "mcp servers tools", keys: None },
];

/// The words a text field holds once its filters (`project:acme`, `status:needs`, `cat:miso`, `since:2d`) are taken out.
pub fn free_words(raw: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for tok in raw.split_whitespace() {
        let key = tok.split_once(':').map(|(k, _)| k.to_lowercase());
        if key.is_some_and(|k| ["project", "status", "cat", "since"].contains(&k.as_str())) {
            continue;
        }
        out.push(tok);
    }
    out.join(" ")
}

/// The value of a filter in the text (`project:acme` gives "acme"), when it has one.
pub fn filter_value(raw: &str, name: &str) -> Option<String> {
    raw.split_whitespace().find_map(|tok| {
        let (k, v) = tok.split_once(':')?;
        (k.eq_ignore_ascii_case(name) && !v.is_empty()).then(|| v.trim_matches('"').to_owned())
    })
}

impl App {
    /// ⌘K: opens the palette with the keyboard in its field, or closes it when it is open.
    pub fn toggle_palette(&mut self) {
        if self.palette.open {
            self.close_palette();
        } else {
            self.open_palette();
        }
    }

    pub fn open_palette(&mut self) {
        self.menu_open = None;
        self.palette = Palette { open: true, ..Palette::default() };
        self.palette.query.focused = true;
    }

    pub fn close_palette(&mut self) {
        self.palette.open = false;
        self.palette.in_flight = None;
    }

    /// Each frame: asks the daemon when the text changed, and carries a reveal along once its thread is open.
    pub fn palette_poll(&mut self) {
        if self.palette.open {
            let text = self.palette.query.text.clone();
            if self.palette.sent.as_deref() != Some(text.as_str()) {
                if let Some(id) = self.send(Request::Search { query: text.clone(), limit: Some(LIMIT) }) {
                    self.palette.in_flight = Some(id);
                    self.palette.sent = Some(text);
                }
                // The first row again: what the new text finds is a new list.
                self.palette.at = 0;
            }
        }
        self.advance_reveal();
    }

    /// The daemon's answer to a search. False when it was not one, or was for a text no longer asked for.
    pub fn take_found(&mut self, id: u64, reply: &Reply) -> bool {
        let Reply::Found { query, hits, problem, took_ms } = reply else { return false };
        if self.palette.in_flight == Some(id) {
            self.palette.in_flight = None;
            self.palette.hits = hits.clone();
            self.palette.answered = Some(query.clone());
            self.palette.problem = problem.clone();
            self.palette.took_ms = Some(*took_ms);
        }
        true
    }

    /// Whether the palette is waiting on the daemon.
    pub fn palette_loading(&self) -> bool {
        self.palette.open && (self.palette.in_flight.is_some() || self.palette.sent.as_deref() != Some(self.palette.query.text.as_str()))
    }

    /// Whether an action is on offer now: a toggle names the state it would change to, and rounds can only be paused when there are some.
    pub fn action_offered(&self, id: &str) -> bool {
        match id {
            "pause-rounds" => !self.rounds.is_empty() && !self.rounds_paused,
            "resume-rounds" => self.rounds_paused,
            "show-archived" => !self.show_archived,
            "hide-archived" => self.show_archived,
            "show-settled" => !self.show_settled,
            "hide-settled" => self.show_settled,
            "pane" => self.pane_root().is_some(),
            _ => true,
        }
    }

    /// Return on a row (`alt`: ⌘Return, the row opened beside what is in view, where that makes sense).
    pub fn palette_choose(&mut self, key: &str, alt: bool) {
        let Some(row) = Row::of(key) else { return };
        // The hit this row came from, for the message to scroll to.
        let hit = |id: &TaskId| self.palette.hits.iter().find(|h| &h.task == id).cloned();
        let found = match &row {
            Row::Thread(id) | Row::Kitten(id) => hit(id),
            _ => None,
        };
        self.close_palette();
        match row {
            Row::Thread(id) => {
                self.open_task_id(&id);
                if alt {
                    self.show_pane();
                    self.open_overview();
                }
                self.begin_reveal(&id, found);
            }
            Row::Kitten(id) => {
                // A kitten opens as a tab beside its thread; ⌘Return opens it in the main view in place of the thread.
                if alt {
                    self.open_kitten_main(&id);
                    self.begin_reveal(&id, found);
                } else {
                    self.open_kitten_tab(&id);
                }
            }
            Row::Cattain(id) => self.open_cattain(&id),
            Row::Project(id) => self.new_thread_in(&id),
            Row::Action(a) => self.run_action(a),
        }
    }

    fn run_action(&mut self, id: &str) {
        match id {
            "new-thread" => self.go_new_thread_and_focus(),
            "needs-you" => self.open_needs(),
            "briefing" => self.open_briefing(),
            "rounds" => self.open_rounds(),
            "pane" => self.toggle_pane(),
            "sidebar" => self.toggle_sidebar(),
            "pause-rounds" => self.set_rounds_paused(true),
            "resume-rounds" => self.set_rounds_paused(false),
            "show-archived" | "hide-archived" => self.set_show_archived(id == "show-archived"),
            "show-settled" | "hide-settled" => self.set_show_settled(id == "show-settled"),
            "settings" => self.open_settings(),
            other => {
                if let Some(page) = other.strip_prefix("settings-").and_then(|n| n.parse::<usize>().ok()) {
                    self.open_settings();
                    self.settings_page = page;
                }
            }
        }
    }

    /// Tab: what is typed becomes the words of a new thread, in the project a `project:` filter names.
    pub fn palette_new_thread(&mut self) {
        let raw = self.palette.query.text.clone();
        self.close_palette();
        let project = filter_value(&raw, "project").map(|p| p.to_lowercase()).and_then(|p| self.workspaces.iter().find(|w| w.removed_ms.is_none() && w.folder.is_some() && w.name.to_lowercase().contains(&p)).map(|w| w.id.clone()));
        if let Some(id) = project {
            self.choose_workspace(&id);
        }
        self.go_new_thread_and_focus();
        let words = free_words(&raw);
        if !words.is_empty() {
            self.draft = TextEdit::new(&words);
            self.draft.focused = true;
        }
    }

    /// A message was chosen: once its thread is open and read, its fold opens if it hides it, and the page scrolls to it.
    fn begin_reveal(&mut self, task: &TaskId, found: Option<Found>) {
        let Some(h) = found.filter(|h| h.by == FoundBy::Message) else { return };
        self.palette.reveal = Some(Reveal { task: task.clone(), part: h.part, seq: h.seq, step: 0 });
    }

    fn advance_reveal(&mut self) {
        let Some(r) = self.palette.reveal.clone() else { return };
        // Gone elsewhere, or the thread is not in view any more: nothing to do.
        if self.open.as_ref() != Some(&r.task) {
            self.palette.reveal = None;
            return;
        }
        let Some(book) = self.books.get(&r.task).filter(|b| b.loaded) else { return };
        let part = r.part.as_ref().and_then(|id| book.parts.iter().find(|p| &p.id == id));
        match r.step {
            0 => {
                // Agent words that a finished run folded away are in a closed fold: open it, then scroll next frame.
                if let Some(p) = part
                    && let Some(run) = &p.run
                    && crate::task_view::in_fold(book, p)
                {
                    self.open_folds.insert(format!("worked-{}", run.0));
                }
                self.palette.reveal = Some(Reveal { step: 1, ..r });
            }
            _ => {
                let key = match part {
                    Some(p) if matches!(p.body, neko_protocol::PartBody::Person { .. }) => Some(format!("part-{}-words", p.seq)),
                    Some(p) => Some(format!("part-{}-answer", p.seq)),
                    // The first words are the task's own: not a part.
                    None if r.part.is_none() => Some("task-words".to_owned()),
                    None => None,
                };
                if let Some(k) = key {
                    self.effects.push(crate::state::Effect::ScrollTo(k));
                }
                self.palette.reveal = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_an_action_is_always_one_of_the_listed() {
        for row in [Row::Thread(TaskId("task_1".into())), Row::Kitten(TaskId("task_2".into())), Row::Cattain("cat_miso".into()), Row::Project("ws_1".into()), Row::Action("settings")] {
            assert_eq!(Row::of(&row.key()), Some(row));
        }
        assert_eq!(Row::of("action:nonsense"), None);
        assert_eq!(Row::of("nothing"), None);
        let mut ids: Vec<&str> = ACTIONS.iter().map(|a| a.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), ACTIONS.len(), "action ids are unique");
    }

    #[test]
    fn filters_come_out_of_the_words_and_the_project_is_read() {
        assert_eq!(free_words("fix login project:acme status:needs"), "fix login");
        assert_eq!(free_words("status:"), "");
        assert_eq!(free_words("note:this stays"), "note:this stays");
        assert_eq!(filter_value("fix project:Acme", "project").as_deref(), Some("Acme"));
        assert_eq!(filter_value("fix project:", "project"), None);
    }
}
