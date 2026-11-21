//! What Home and the sidebar say about the day, derived from the data the client already has (the snapshot, the Routines answer and its
//! usage figure, the cats): no request of its own. Counts and rows only; `view/home.rs` and `view/sessions.rs` draw them.

use neko_protocol::cats::Cat;
use neko_protocol::workbench::TaskStatus;

use crate::app::App;
use crate::clock;
use crate::groups::{Group, group_of};
use crate::link::{InboxItem, TaskRow};

impl App {
    /// "Home" in the sidebar: the composer page, left as it is when it is already showing.
    pub fn go_home(&mut self) {
        if !self.home_shown() {
            self.open_new();
        }
    }

    /// The open Inbox items (what a routine or a goal needs from the person) of the workspace in scope, newest first.
    pub fn open_inbox(&self) -> Vec<InboxItem> {
        let Some(data) = self.pages.routines.as_ref() else { return Vec::new() };
        let views: Vec<_> = data.views.iter().filter(|v| self.scope().is_none_or(|w| v.routine.workspace_id == w)).cloned().collect();
        crate::link::inbox_items(&views, self.now_ms).into_iter().filter(|i| i.closed_ms.is_none()).collect()
    }

    /// Tasks that wait on the person for something other than a merge (a plan to approve, a question, a failure): Home's Needs you rows.
    pub fn needs_you_tasks(&self) -> Vec<&TaskRow> {
        let Some(m) = self.summary.as_ref() else { return Vec::new() };
        let mut rows: Vec<&TaskRow> = m.tasks.iter().filter(|t| t.status != TaskStatus::ReadyForReview && group_of(t, self.now_ms) == Some(Group::NeedsYou) && self.scope().is_none_or(|w| m.workspace_of(&t.id) == w)).collect();
        rows.sort_by_key(|t| std::cmp::Reverse(t.updated_at_ms));
        rows
    }

    /// How many things need the person: Inbox items and tasks waiting on a decision. The sidebar's Home count and Home's status line.
    pub fn needs_you_count(&self) -> usize {
        self.open_inbox().len() + self.needs_you_tasks().len()
    }

    /// How many tasks are being worked on right now.
    pub fn working_count(&self) -> usize {
        let Some(m) = self.summary.as_ref() else { return 0 };
        m.tasks.iter().filter(|t| matches!(t.status, TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing) && self.scope().is_none_or(|w| m.workspace_of(&t.id) == w)).count()
    }

    /// Today's spend in dollars (tasks and routines), from the usage ledger's figure in the Routines answer; none until it arrives.
    pub fn spend_today(&self) -> Option<f64> {
        self.pages.routines.as_ref().map(|d| d.spend_today.tasks_usd + d.spend_today.routines_usd)
    }

    /// Tasks whose pull request is ready to merge, newest first.
    pub fn ready_to_merge(&self) -> Vec<&TaskRow> {
        let Some(m) = self.summary.as_ref() else { return Vec::new() };
        let mut rows: Vec<&TaskRow> = m.tasks.iter().filter(|t| t.status == TaskStatus::ReadyForReview && self.scope().is_none_or(|w| m.workspace_of(&t.id) == w)).collect();
        rows.sort_by_key(|t| std::cmp::Reverse(t.updated_at_ms));
        rows
    }

    /// Tasks that finished today (since local midnight), newest first.
    pub fn done_today(&self) -> Vec<&TaskRow> {
        let Some(m) = self.summary.as_ref() else { return Vec::new() };
        let start = clock::day_start(self.now_ms);
        let mut rows: Vec<&TaskRow> = m.tasks.iter().filter(|t| t.status == TaskStatus::Completed && t.updated_at_ms >= start && self.scope().is_none_or(|w| m.workspace_of(&t.id) == w)).collect();
        rows.sort_by_key(|t| std::cmp::Reverse(t.updated_at_ms));
        rows
    }

    /// The cat that started a task, when one did.
    pub fn cat_of(&self, task_id: &str) -> Option<&Cat> {
        let id = self.summary.as_ref()?.meta.get(task_id)?.cat_id.as_deref()?;
        self.cat(id)
    }

    /// The cats in the order the sidebar and Home show them: pinned first, then the rest (`cats::display`).
    pub fn cats_in_order(&self) -> Vec<&Cat> {
        let Some(list) = self.cats.list.as_ref() else { return Vec::new() };
        let (mut faces, rows) = crate::cats::display(list);
        faces.extend(rows);
        faces
    }

    /// The cat whose tasks the sidebar lists under the Cats section: the open one, else the first.
    pub fn sidebar_cat(&self) -> Option<&Cat> {
        self.open_cat_row().or_else(|| self.cats_in_order().into_iter().next())
    }

    /// The open pull request of a task, as `#211`, when it has one.
    pub fn pr_of(&self, task: &TaskRow) -> Option<String> {
        let own = task.line.pr.trim();
        if !own.is_empty() {
            return Some(own.to_owned());
        }
        self.summary.as_ref()?.meta.get(&task.id)?.pull_request.clone().filter(|p| !p.is_empty())
    }
}
