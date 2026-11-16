//! "+ New": start a session with a workspace, the request in the person's words, an optional
//! goal (a check that proves it done) and an optional runtime.
//!
//! The web UI has no form for this (it starts work from Neko's chat), so this uses the requests
//! the daemon already takes for it: `CreateTask` (what the chat's `create_task` tool makes: an
//! ordinary ticket that waits for approval like any other), then for a goal `Goals(Create)`
//! on that ticket (what `/goal` does), and for a runtime `SetConversationRuntime` on the
//! ticket's conversation (what the composer's runtime picker does).

use std::collections::HashSet;
use std::path::Path;

use rune::TextEdit;

use crate::app::App;
use crate::link::{Call, RuntimeChoice, Summary};

/// Which of the composer's selects is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewMenu {
    Workspace,
    Runtime,
    Model,
}

/// Plan: the session plans first and waits for approval. Build: it starts building as soon as it is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Plan,
    Build,
}

/// The runtimes the composer offers, as the daemon names them: (provider id, the name on the select).
pub const RUNTIMES: [(&str, &str); 3] = [("claude", "Claude"), ("codex", "Codex"), ("opencode", "OpenCode")];

/// What Home's composer holds besides its text: where, with which runtime and model, plan or build.
pub struct NewForm {
    /// Index into the summary's workspaces.
    pub workspace: usize,
    pub mode: Mode,
    pub choices: Vec<RuntimeChoice>,
    /// The runtime's provider id; `None` is the one Neko uses by default.
    pub runtime: Option<String>,
    /// The model's id; `None` is Auto (the runtime's own choice).
    pub model: Option<String>,
    pub menu: Option<NewMenu>,
    pub busy: bool,
    pub error: Option<String>,
    /// The workspaces' branches, read from their folders when Home opens: (workspace id, branch).
    pub branches: Vec<(String, String)>,
    /// The tasks there were when "Send" was pressed, to find the one it made.
    known: HashSet<String>,
    /// The title the ticket was asked for with, to tell it from one another window made meanwhile.
    expect: String,
}

impl NewForm {
    pub fn fresh() -> NewForm {
        NewForm { workspace: 0, mode: Mode::Plan, choices: Vec::new(), runtime: None, model: None, menu: None, busy: false, error: None, branches: Vec::new(), known: HashSet::new(), expect: String::new() }
    }

    /// The models the chosen runtime offers (the daemon's own list), for the model select.
    pub fn models_for(&self, provider: &str) -> Vec<&RuntimeChoice> {
        self.choices.iter().filter(|c| c.provider == provider).collect()
    }
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
    /// Home with a clean composer: "New session" and a fresh window both land here.
    pub fn open_new(&mut self) {
        self.new_request = TextEdit::new("");
        self.new_request.focused = true;
        let keep = self.new_form.take().map(|f| (f.workspace, f.mode, f.choices, f.runtime, f.model, f.branches));
        let mut form = NewForm::fresh();
        if let Some((workspace, mode, choices, runtime, model, branches)) = keep {
            (form.workspace, form.mode, form.choices, form.runtime, form.model, form.branches) = (workspace, mode, choices, runtime, model, branches);
        }
        self.new_form = Some(form);
        self.selected = None;
        self.wanted.set(None);
        self.view = None;
        self.plugin_view = None;
        self.call(Call::Models);
        self.read_branches();
    }

    /// The composer's state for a window that starts on Home (nothing selected).
    pub fn ensure_home(&mut self) {
        if self.new_form.is_none() {
            self.new_form = Some(NewForm::fresh());
            self.call(Call::Models);
            self.read_branches();
        }
    }

    /// Reads each workspace's branch from its first folder (one small file each).
    pub fn read_branches(&mut self) {
        let Some(summary) = self.summary.as_ref() else { return };
        let branches: Vec<(String, String)> = summary
            .settings
            .workspaces
            .iter()
            .filter_map(|w| w.folders.iter().find_map(|f| crate::readers::branch_of(Path::new(f))).map(|b| (w.id.clone(), b)))
            .collect();
        if let Some(form) = self.new_form.as_mut() {
            form.branches = branches;
        }
    }

    pub fn close_new(&mut self) {
        self.new_form = None;
    }

    /// Why Send is off, in one calm line, or None when it can go.
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

    /// The ticket exists: open it, pin its runtime and model when asked, and start it when the mode is Build.
    pub(crate) fn after_created(&mut self, id: &str) {
        let Some(form) = self.new_form.take() else { return };
        self.open_task(id);
        if form.runtime.is_some() || form.model.is_some() {
            let provider = form.runtime.clone().unwrap_or_else(|| self.default_provider());
            self.call(Call::SetRuntime { task_id: id.to_owned(), provider, model: form.model.clone().unwrap_or_default() });
        }
        if form.mode == Mode::Build {
            self.call(Call::Start { task_id: id.to_owned() });
        }
        self.new_request = TextEdit::new("");
    }

    /// An image chosen with the composer's "+": its Markdown reference joins the request, as a pasted one does.
    pub fn attach_to_new(&mut self, path: &Path) {
        let sep = if self.new_request.text.is_empty() || self.new_request.text.ends_with('\n') { "" } else { "\n" };
        let text = format!("{}{sep}{}\n", self.new_request.text, crate::attachments::reference(path));
        self.new_request.replace_text(text);
    }

    /// An image chosen with the session composer's "+": its reference goes into the message.
    pub fn attach_to_reply(&mut self, path: &Path) {
        let sep = if self.composer.text.is_empty() || self.composer.text.ends_with('\n') { "" } else { "\n" };
        let text = format!("{}{sep}{}\n", self.composer.text, crate::attachments::reference(path));
        self.composer.replace_text(text);
    }

    /// The same for Neko's ask box.
    pub fn attach_to_ask(&mut self, path: &Path) {
        let sep = if self.ask.text.is_empty() || self.ask.text.ends_with('\n') { "" } else { "\n" };
        let text = format!("{}{sep}{}\n", self.ask.text, crate::attachments::reference(path));
        self.ask.replace_text(text);
    }

    /// The provider id Neko uses when a session names none.
    pub fn default_provider(&self) -> String {
        self.summary.as_ref().map(|m| m.settings.runtime.provider.clone()).filter(|p| !p.is_empty()).unwrap_or_else(|| "codex".to_owned())
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
