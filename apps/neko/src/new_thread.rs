//! New thread (docs/knowledge/threads.md): the state behind the start screen. A thread needs no cat, so the screen asks for three
//! things only: the project, where in it the thread works (a worktree of its own, or the folder itself) and the branch a worktree
//! starts from. Return starts the thread and opens it; ⌘Return starts it and keeps a fresh screen open, to start several in a row.
//!
//! The folder race (found in a real run): "Add folder…" then Return at once used to start the thread in No project, because the
//! project became the composer's choice only when the daemon answered. The chip now names the folder at once ([`App::chip_project_name`]),
//! and a send while the folder is still being added is held until the answer, then goes to that project.

use std::collections::HashMap;

use neko_protocol::{Reply, Request};
use rune::TextEdit;

use neko_protocol::GENERAL;

use crate::state::App;

/// Where a thread in a project can start from, as the daemon last said ([`Reply::Branches`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BranchList {
    /// The folder is a repository with a commit: only then is there a worktree or a branch to choose.
    pub git: bool,
    /// The branch the checkout is on (None: detached).
    pub current: Option<String>,
    pub names: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct NewThread {
    /// A Cattain's id: the thread starts in its crew and reads its CREW.md (docs/knowledge/cats.md). Cleared once one starts and opens.
    pub crew: Option<String>,
    /// Work in the project's folder itself, not in a worktree of its own. Kept while Neko runs; back to a worktree (the safe
    /// default) when the project changes.
    pub in_folder: bool,
    /// The branch a worktree starts from (None: the one the checkout is on).
    pub from_branch: Option<String>,
    /// Each project's branches, by workspace id.
    pub branches: HashMap<String, BranchList>,
    /// Which chip's menu is open.
    pub place_open: bool,
    pub branch_open: bool,
    /// The folder the person chose from the project menu, until the daemon has made it a project (its path).
    pub adding: Option<String>,
    /// A send that came while a folder was being added; the answer sends it. `true`: it was ⌘Return.
    pub held: Option<bool>,
    /// Threads started with ⌘Return, by request id, with their words (put back in the box if the daemon refuses them).
    pub background: HashMap<u64, String>,
    /// The daemon was not up yet when ⌘Return came: the words are sent in the background once it is.
    pub queued_background: bool,
}

/// The last part of a path, for a chip: "/Users/example/acme-web/" is "acme-web".
pub fn folder_name(path: &str) -> String {
    path.trim_end_matches('/').rsplit('/').find(|p| !p.is_empty()).unwrap_or(path).to_owned()
}

impl App {
    /// The project the chip names: the folder being added (its name, at once), else the chosen project.
    pub fn chip_project_name(&self) -> String {
        if let Some(p) = &self.new_thread.adding {
            return folder_name(p);
        }
        self.chosen().map_or_else(|| "No project".to_owned(), |w| w.name.clone())
    }

    /// Whether the chip is waiting for the daemon to make the folder a project.
    pub fn adding_folder(&self) -> bool {
        self.new_thread.adding.is_some()
    }

    /// The chosen project's branches, when its folder is a repository with a commit (None: no worktree or branch to choose).
    pub fn branch_list(&self) -> Option<&BranchList> {
        if self.new_thread.adding.is_some() {
            return None;
        }
        self.new_thread.branches.get(&self.workspace).filter(|b| b.git)
    }

    /// The branch a worktree would start from now: the person's choice, else the checkout's own.
    pub fn start_branch(&self) -> Option<String> {
        let list = self.branch_list()?;
        self.new_thread.from_branch.clone().filter(|b| list.names.contains(b)).or_else(|| list.current.clone())
    }

    /// Asks the daemon where a thread in the chosen project can start from. A project with no folder has nothing to ask.
    pub fn ask_branches(&mut self) {
        let Some(w) = self.chosen().filter(|w| w.folder.is_some() && !w.missing).map(|w| w.id.clone()) else { return };
        self.send(Request::ListBranches { workspace: w });
    }

    /// The daemon's answer. A project that is not a repository is remembered as such, so its chips stay away.
    pub fn take_branches(&mut self, workspace: String, git: bool, current: Option<String>, names: Vec<String>) {
        if let Some(b) = &self.new_thread.from_branch
            && workspace == self.workspace
            && !names.contains(b)
        {
            self.new_thread.from_branch = None;
        }
        self.new_thread.branches.insert(workspace, BranchList { git, current, names });
    }

    /// "New worktree" (false) or "In the folder" (true).
    pub fn choose_place(&mut self, in_folder: bool) {
        self.new_thread.place_open = false;
        self.new_thread.in_folder = in_folder;
    }

    /// The branch a worktree starts from (None: the checkout's own).
    pub fn choose_branch(&mut self, branch: Option<String>) {
        self.new_thread.branch_open = false;
        self.new_thread.from_branch = branch;
    }

    /// A folder the person picked from the project menu is on its way to becoming a project: the chip says so now.
    pub fn begin_adding(&mut self, folder: &str) {
        self.new_thread.adding = Some(folder.to_owned());
    }

    /// The daemon answered the folder: it is the chosen project, and a send that waited goes now.
    pub fn folder_added(&mut self, workspace: &str) {
        self.new_thread.adding = None;
        self.choose_workspace(workspace);
        if let Some(background) = self.new_thread.held.take() {
            self.start_thread(background);
        }
    }

    /// The daemon refused the folder (or the link went): the chip goes back to the project it had, and a held send stays in the box.
    pub fn folder_failed(&mut self) {
        self.new_thread.adding = None;
        self.new_thread.held = None;
    }

    /// Return in the composer: the thread starts and opens.
    pub fn submit(&mut self) {
        self.start_thread(false);
    }

    /// ⌘Return in the composer: the thread starts, and this screen stays open with an empty box.
    pub fn submit_in_background(&mut self) {
        self.start_thread(true);
    }

    /// Sends the words as a new thread in the chosen project. They leave the box at once and come back if the daemon refuses them.
    pub fn start_thread(&mut self, background: bool) {
        let words = self.draft.text.clone();
        if words.trim().is_empty() || self.creating.is_some() || self.new_thread.held.is_some() {
            return;
        }
        // The folder race: the project is not known to the daemon yet, so the words wait for it (the box is disabled meanwhile).
        if self.new_thread.adding.is_some() {
            self.new_thread.held = Some(background);
            self.notice = None;
            return;
        }
        let workspace = Some(self.workspace.clone()).filter(|w| w != GENERAL);
        // Only a repository has a choice to make; the checkout's own branch is the default, so it is not sent.
        let (in_folder, from_branch) = match self.branch_list() {
            Some(_) if self.new_thread.in_folder => (true, None),
            Some(list) => (false, self.new_thread.from_branch.clone().filter(|b| list.names.contains(b) && Some(b) != list.current.as_ref())),
            None => (false, None),
        };
        let crew = self.new_thread.crew.clone().filter(|c| self.cat(c).is_some_and(neko_protocol::Cat::is_active));
        match self.send(Request::CreateTask { crew, words: words.clone(), workspace, in_folder, from_branch }) {
            Some(id) => {
                if background {
                    self.new_thread.background.insert(id, words);
                } else {
                    self.creating = Some((id, words));
                    self.new_thread.crew = None;
                }
                self.draft = TextEdit::default();
                // The screen stays: the keyboard stays in its box for the next thread.
                self.draft.focused = background;
                self.notice = None;
                self.queued = false;
            }
            // The link is coming up: the words stay in the box and go as soon as the daemon answers.
            None if self.conn.is_some() => {
                self.queued = true;
                self.new_thread.queued_background = background;
                self.notice = Some("Neko is starting. Your thread is sent as soon as it is ready.".to_owned());
            }
            None => self.notice = Some("Neko is not running. Your words are kept.".to_owned()),
        }
    }

    /// The reply to a thread started in the background: it is in the sidebar; this screen only says so, quietly.
    pub fn background_started(&mut self, task: neko_protocol::Task) {
        self.notice = Some(format!("Started {}", neko_protocol::title::title(&task.title)));
        self.upsert(task);
    }

    /// A refused background start: the words go back in the box when it is empty.
    pub fn background_refused(&mut self, id: u64, message: String) {
        if let Some(words) = self.new_thread.background.remove(&id)
            && self.draft.text.is_empty()
        {
            self.draft = TextEdit::new(&words);
        }
        self.notice = Some(message);
    }

    /// Whether this reply answers a thread started in the background.
    pub fn is_background(&self, id: u64) -> bool {
        self.new_thread.background.contains_key(&id)
    }

    /// Takes the words of a background start that was answered.
    pub fn background_answered(&mut self, id: u64) {
        self.new_thread.background.remove(&id);
    }

    /// A reply that only New thread cares about. True when it was one.
    pub fn new_thread_reply(&mut self, reply: &Reply) -> bool {
        match reply {
            Reply::Branches { workspace, git, current, branches } => {
                self.take_branches(workspace.clone(), *git, current.clone(), branches.clone());
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_chip_names_the_last_part_of_the_path() {
        assert_eq!(folder_name("/Users/example/acme-web"), "acme-web");
        assert_eq!(folder_name("/Users/example/acme-web/"), "acme-web");
        assert_eq!(folder_name("/"), "/");
    }
}
