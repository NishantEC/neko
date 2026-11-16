//! "+ New": start a session with a workspace, the request in the person's words, an optional
//! goal (a check that proves it done) and an optional runtime.
//!
//! The web UI has no form for this (it starts work from Neko's chat), so this uses the requests
//! the daemon already takes for it: `CreateTask` (what the chat's `create_task` tool makes: an
//! ordinary ticket that waits for approval like any other), then for a goal `Goals(Create)`
//! on that ticket (what `/goal` does), and for a runtime `SetConversationRuntime` on the
//! ticket's conversation (what the composer's runtime picker does).

use std::collections::HashSet;

use rune::TextEdit;

use crate::app::App;
use crate::link::{Call, RuntimeChoice, Summary};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewMenu {
    Workspace,
    Runtime,
}

pub struct NewForm {
    /// Index into the summary's workspaces.
    pub workspace: usize,
    pub goal: bool,
    pub choices: Vec<RuntimeChoice>,
    /// `None` is the default runtime.
    pub runtime: Option<usize>,
    pub menu: Option<NewMenu>,
    pub busy: bool,
    pub error: Option<String>,
    /// The tasks there were when "Start" was pressed, to find the one it made.
    known: HashSet<String>,
    /// The title the ticket was asked for with, to tell it from one another window made meanwhile.
    expect: String,
}

/// The longest request the daemon keeps (`MAX_TASK_REQUEST`).
pub const MAX_REQUEST: usize = neko_protocol::workbench::MAX_TASK_REQUEST;

/// A title from the request's first line, cut short.
pub fn title_of(request: &str) -> String {
    let line = request.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let line: String = line.chars().take(80).collect();
    line.trim_end_matches(|c: char| matches!(c, '.' | ':') || c.is_whitespace()).to_owned()
}

impl App {
    pub fn open_new(&mut self) {
        self.new_request = TextEdit::new("");
        self.new_request.focused = true;
        self.new_check = TextEdit::new("");
        self.new_form = Some(NewForm { workspace: 0, goal: false, choices: Vec::new(), runtime: None, menu: None, busy: false, error: None, known: HashSet::new(), expect: String::new() });
        self.view = None;
        self.call(Call::Models);
    }

    pub fn close_new(&mut self) {
        self.new_form = None;
    }

    /// Why Start is off, in one calm line, or None when it can go.
    pub fn new_blocker(&self) -> Option<&'static str> {
        let _form = self.new_form.as_ref()?;
        let workspaces = self.summary.as_ref().map_or(0, |s| s.workspaces.len());
        if workspaces == 0 {
            return Some("Add a workspace in the web UI first.");
        }
        if self.new_request.text.trim().is_empty() {
            return Some("Say what you want done.");
        }
        if self.new_request.text.len() > MAX_REQUEST {
            return Some("That is too long for one request.");
        }
        None
    }

    pub fn submit_new(&mut self) {
        if self.new_blocker().is_some() || self.new_form.as_ref().is_none_or(|f| f.busy) || !self.work.busy.insert("new".to_owned()) {
            return;
        }
        let Some(summary) = self.summary.as_ref() else { return };
        let known: HashSet<String> = summary.tasks.iter().map(|t| t.id.clone()).collect();
        let request = self.new_request.text.trim().to_owned();
        let Some(form) = self.new_form.as_mut() else { return };
        let Some(workspace) = summary.workspaces.get(form.workspace) else { return };
        form.busy = true;
        form.error = None;
        form.known = known;
        form.expect = title_of(&request);
        let call = Call::CreateTask { workspace_id: workspace.id.clone(), title: title_of(&request), goal: request };
        self.call(call);
    }

    /// The task the last `CreateTask` made: the one the snapshot has that the form did not know.
    pub(crate) fn created_task(&mut self, summary: &Summary) -> Option<String> {
        let form = self.new_form.as_ref()?;
        let new: Vec<&crate::link::TaskRow> = summary.tasks.iter().filter(|t| !form.known.contains(&t.id)).collect();
        // A task filed by a watch or by chat in the same moment is not ours: ours has our title.
        let ours = new.iter().filter(|t| t.title == form.expect).max_by_key(|t| t.updated_at_ms).or(if new.len() == 1 { new.first() } else { None });
        match ours {
            Some(t) => Some(t.id.clone()),
            None => {
                self.new_error("Neko made the session, but this app couldn’t tell which one it is. It is in the sessions list.".to_owned());
                None
            }
        }
    }

    /// The ticket exists: open it, then pin its runtime and attach its goal when asked.
    pub(crate) fn after_created(&mut self, id: &str) {
        let Some(form) = self.new_form.take() else { return };
        let workspace_id = self.summary.as_ref().and_then(|s| s.workspaces.get(form.workspace)).map(|w| w.id.clone()).unwrap_or_default();
        self.open_task(id);
        if let Some(choice) = form.runtime.and_then(|i| form.choices.get(i)) {
            self.call(Call::SetRuntime { task_id: id.to_owned(), provider: choice.provider.clone(), model: choice.model.clone() });
        }
        if form.goal {
            let objective = self.new_request.text.trim().to_owned();
            let check = self.new_check.text.trim().to_owned();
            self.call(Call::GoalCreate { workspace_id, objective, done_check: check, ticket_id: id.to_owned() });
        }
    }

    pub(crate) fn new_error(&mut self, message: String) {
        if let Some(form) = self.new_form.as_mut() {
            form.busy = false;
            form.error = Some(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_is_the_first_line_cut_short_without_its_full_stop() {
        assert_eq!(title_of("\n  Fix the login button.\nIt does nothing."), "Fix the login button");
        assert!(title_of(&"word ".repeat(40)).chars().count() <= 80);
        assert_eq!(title_of(""), "");
    }
}
