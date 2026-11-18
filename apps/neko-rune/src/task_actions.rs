//! What you can do to a session, decided once, as the web's `work/taskActions.ts` does: the session's "..." menu and
//! the sessions column's right-click menu both read [`entries`], so they can never disagree about which item a task in
//! a given state gets. Each item is shown under the conditions the web shows it under; the ones that destroy, restart
//! or stop something go through [`App::confirm`], a modal dialog, and a request is only built inside it.
//!
//! Stop all work (⌘⇧Esc, the account menu) is here too: it counts what runs as the web's `stopAllWork` does and asks
//! "Stop all N running agents?" before it stops everything.

use neko_protocol::workbench::TaskStatus;
use rune::{PointerPress, TextEdit};
use rune_kit::{Menu, MenuItem};

use crate::app::App;
use crate::link::Call;

/// A question asked in a dialog before a press that cannot be taken back; "Yes" sends `call`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    pub title: String,
    pub detail: String,
    pub ok: String,
    pub cancel: String,
    /// The yes button is coral.
    pub destructive: bool,
    pub call: Call,
    /// What the window says once it is sent ("Deleted “Fix it”.").
    pub done: String,
    /// A text box in the dialog (the reason of a closed pull request): its words go into `call` when the answer is yes.
    pub field: Option<AskField>,
    /// What the question is about, one line each (the pull requests a merge will take).
    pub list: Vec<String>,
    /// Calls that go out with `call` when the answer is yes (merging several at once).
    pub more: Vec<Call>,
    /// A checkbox under the question (Uninstall's "also move Neko’s data to the Trash"); its state is `shell.ask_toggled`.
    pub toggle: Option<String>,
}

/// The text box of a dialog that asks for words (`Ask::field`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AskField {
    pub label: String,
    pub placeholder: String,
}

/// The page field a dialog's text box edits.
pub const ASK_FIELD: &str = "ask-field";

/// The title being renamed in the session's header.
pub struct Rename {
    pub id: String,
    pub edit: TextEdit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Rename,
    Retry,
    Publish,
    RunNext,
    LandOnMain,
    MarkUnread,
    CopyLink,
    OpenWeb,
    Stop,
    Cancel,
    Delete,
}

/// One item of a task's menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub act: Act,
    pub label: String,
    /// A checkbox item (Land on main): its state.
    pub checked: Option<bool>,
    pub danger: bool,
}

/// The web's `isRunning`.
pub fn is_running(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Queued | TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing)
}

/// The web's `isFinished`.
pub fn is_finished(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::Failed)
}

fn entry(act: Act, label: impl Into<String>) -> Entry {
    Entry { act, label: label.into(), checked: None, danger: false }
}

/// The last folder of a path ("/Users/me/code/neko" -> "neko"): the web's `repo` label.
pub fn repo_name(path: &str) -> String {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path).to_owned()
}

/// The items for task `id`, in the order of the menu, under the web's conditions. Empty when the task is not known.
pub fn entries(s: &App, id: &str) -> Vec<Entry> {
    let Some(summary) = s.summary.as_ref() else { return Vec::new() };
    let Some(task) = summary.task(id) else { return Vec::new() };
    let meta = summary.meta_of(id);
    let status = task.status;
    let mut items = vec![entry(Act::Rename, "Rename…")];
    if matches!(status, TaskStatus::Failed | TaskStatus::Cancelled) {
        items.push(entry(Act::Retry, "Retry from start…"));
    }
    if status == TaskStatus::ReadyForReview && meta.is_none_or(|m| m.pull_request.is_none()) {
        items.push(entry(Act::Publish, "Publish pull request"));
    }
    if status == TaskStatus::Queued {
        items.push(entry(Act::RunNext, "Run next"));
    }
    if let Some(repository) = meta.map(|m| m.repository.as_str()).filter(|r| !r.is_empty()) {
        items.push(Entry { checked: Some(summary.land_on_main.iter().any(|p| p == repository)), ..entry(Act::LandOnMain, format!("Land on main in {}", repo_name(repository))) });
    }
    items.push(entry(Act::MarkUnread, "Mark as unread"));
    items.push(entry(Act::CopyLink, "Copy link"));
    items.push(entry(Act::OpenWeb, "Open in web UI"));
    if is_running(status) {
        items.push(entry(Act::Stop, "Stop"));
    } else if matches!(status, TaskStatus::AwaitingApproval | TaskStatus::ReadyForReview) {
        items.push(Entry { danger: true, ..entry(Act::Cancel, "Cancel task") });
    }
    if is_finished(status) {
        items.push(Entry { danger: true, ..entry(Act::Delete, "Delete…") });
    }
    items
}

/// A menu with the task's items: used by the session's "..." menu and the row's right-click menu. `close` puts the
/// menu away; a destructive item opens its confirm dialog first.
pub fn fill_menu(mut menu: Menu, s: &App, id: &str) -> Menu {
    let items = entries(s, id);
    for item in items {
        if item.danger {
            menu = menu.separator();
        }
        let owner = id.to_owned();
        let act = item.act;
        let mut line = MenuItem::new(item.label.clone()).danger(item.danger);
        if let Some(on) = item.checked {
            line = line.checked(on);
        }
        line = if act == Act::CopyLink {
            line.on_select_tx(move |s: &mut App, tx: &mut rune::Tx| {
                s.close_task_menus();
                if let Some(link) = s.task_link(&owner) {
                    tx.copy(link);
                    s.show_toast("Link copied");
                }
            })
        } else {
            line.on_select(move |s: &mut App| {
                s.close_task_menus();
                s.do_action(&owner, act);
            })
        };
        menu = menu.item(line);
    }
    menu
}

impl App {
    /// Puts both task menus away (the session's "..." and the row's right-click).
    pub fn close_task_menus(&mut self) {
        self.session_menu = false;
        self.row_menu = None;
    }

    /// The task's link in the web UI, without the token (safe to paste anywhere).
    pub fn task_link(&self, id: &str) -> Option<String> {
        self.web.as_ref().map(|w| format!("http://{}/#/tasks?task={id}&view=full", w.display()))
    }

    /// Opens the right-click menu of a row.
    pub fn open_row_menu(&mut self, id: &str, at: PointerPress) {
        self.row_menu = Some((id.to_owned(), at));
    }

    /// Asks in a dialog; the call goes out only when the person says yes.
    pub fn confirm(&mut self, ask: Ask) {
        self.shell.ask_toggled = false;
        self.dialog = Some(ask);
    }

    pub fn ask_yes(&mut self) {
        // A dialog that asks for words does not say yes to none (the view keeps its button off; this is the rule).
        if self.dialog.as_ref().is_some_and(|a| a.field.is_some()) && self.pages.text(ASK_FIELD).trim().is_empty() {
            return;
        }
        if let Some(ask) = self.dialog.take() {
            // Uninstall is local: it stops the work itself (`link::shell::cancel_all_work`), then trashes the app (`shell/uninstall.rs`).
            if ask.title == crate::shell::uninstall::TITLE {
                let include_data = std::mem::take(&mut self.shell.ask_toggled);
                self.run_uninstall(include_data);
                return;
            }
            // The report of a partly failed uninstall: the app is gone either way, so it quits.
            if ask.title == crate::shell::uninstall::PARTLY_TITLE {
                if let Some(h) = &self.shell.handle {
                    h.quit();
                }
                return;
            }
            let mut call = ask.call;
            if ask.field.is_some() {
                // The words of the dialog's box go with the call (a closed pull request's comment).
                call = call.with_text(self.pages.text(ASK_FIELD));
                self.pages.set_field(ASK_FIELD, "");
            }
            self.call(call);
            for more in ask.more {
                self.call(more);
            }
            if !ask.done.is_empty() {
                self.show_toast(&ask.done);
            }
        }
    }

    pub fn ask_no(&mut self) {
        self.shell.ask_toggled = false;
        if self.dialog.as_ref().is_some_and(|a| a.field.is_some()) {
            self.pages.set_field(ASK_FIELD, "");
        }
        self.dialog = None;
    }

    /// The title a task shows now (the name the person gave it, else its own).
    fn title_of(&self, id: &str) -> String {
        self.summary.as_ref().and_then(|m| m.task(id)).map(|t| t.title.clone()).unwrap_or_default()
    }

    /// Runs one item of a task's menu.
    pub fn do_action(&mut self, id: &str, act: Act) {
        let title = self.title_of(id);
        let task_id = id.to_owned();
        match act {
            Act::Rename => self.start_rename(id),
            Act::Retry => self.confirm(Ask {
                title: format!("Retry “{title}” from the start?"),
                detail: "The agent starts over from the beginning. What it already did stays in its history, and its worktree stays on disk.".into(),
                ok: "Retry from start".into(),
                cancel: "Keep it".into(),
                destructive: true,
                call: Call::Retry { task_id },
                done: format!("Retrying “{title}”."),
                field: None,
                list: Vec::new(),
                more: Vec::new(),
                toggle: None,
            }),
            Act::Publish => {
                self.call(Call::Publish { task_id });
                self.show_toast("Opening a pull request.");
            }
            Act::RunNext => {
                self.call(Call::RunNext { task_id });
                self.show_toast("Moved to the front of the queue.");
            }
            Act::LandOnMain => {
                let Some(repository) = self.summary.as_ref().and_then(|m| m.meta_of(id)).map(|m| m.repository.clone()).filter(|r| !r.is_empty()) else {
                    self.show_toast("This task has no repository yet.");
                    return;
                };
                let enabled = !self.summary.as_ref().is_some_and(|m| m.land_on_main.contains(&repository));
                self.call(Call::SetLandOnMain { repository, enabled });
                self.show_toast(if enabled { "Reviewed tasks here now land on local main." } else { "Reviewed tasks here now open a pull request." });
            }
            Act::MarkUnread => self.mark_unread(id),
            Act::CopyLink => {}
            Act::OpenWeb => self.open_in_web(Some(&format!("tasks?task={id}&view=full"))),
            Act::Stop => self.confirm(Ask {
                title: format!("Stop “{title}”?"),
                detail: "The agent stops where it is. Its files and worktree stay on disk, and you can start it again.".into(),
                ok: "Stop it".into(),
                cancel: "Keep working".into(),
                destructive: true,
                call: Call::Stop { task_id },
                done: format!("Stopped “{title}”."),
                field: None,
                list: Vec::new(),
                more: Vec::new(),
                toggle: None,
            }),
            Act::Cancel => self.confirm(Ask {
                title: format!("Cancel “{title}”?"),
                detail: "It hasn’t been reviewed yet, so this closes it. Its files and worktree stay on disk.".into(),
                ok: "Cancel task".into(),
                cancel: "Keep it".into(),
                destructive: true,
                call: Call::Stop { task_id },
                done: format!("Cancelled “{title}”."),
                field: None,
                list: Vec::new(),
                more: Vec::new(),
                toggle: None,
            }),
            Act::Delete => self.confirm(Ask {
                title: "Delete this task?".into(),
                detail: "Removes the task, its subtasks and their history from Neko. Your files and the task’s worktree on disk stay as they are.".into(),
                ok: "Delete task".into(),
                cancel: "Keep it".into(),
                destructive: true,
                call: Call::Delete { task_id },
                done: format!("Deleted “{title}”."),
                field: None,
                list: Vec::new(),
                more: Vec::new(),
                toggle: None,
            }),
        }
    }

    /// "Rename…": the session opens and its title becomes a field holding the name (Enter saves, Esc cancels).
    pub fn start_rename(&mut self, id: &str) {
        if self.selected.as_deref() != Some(id) || self.view.is_some() {
            self.select(id);
        }
        let initial = self.summary.as_ref().and_then(|m| m.meta_of(id)).and_then(|m| m.display_title.clone()).unwrap_or_else(|| self.title_of(id));
        let mut edit = TextEdit::new(initial.clone());
        edit.focused = true;
        let end = edit.text.len();
        edit.set_selection(0, end);
        self.rename = Some(Rename { id: id.to_owned(), edit });
    }

    /// Enter in the title field: saves it (an empty name lets Neko name the task again).
    pub fn finish_rename(&mut self) {
        let Some(r) = self.rename.take() else { return };
        let title = r.edit.text.trim().to_owned();
        self.call(Call::Rename { task_id: r.id, title: title.clone() });
        self.show_toast(if title.is_empty() { "Neko will name it again." } else { "Renamed." });
    }

    /// Esc in the title field.
    pub fn cancel_rename(&mut self) {
        self.rename = None;
    }

    /// How many agents run, as the web's `stopAllWork` counts them: tasks that are not waiting for review or finished,
    /// and Neko's own reply while it is being written.
    pub fn running_agents(&self) -> (usize, bool) {
        let Some(m) = self.summary.as_ref() else { return (0, false) };
        let tasks = m.tasks.iter().filter(|t| !matches!(t.status, TaskStatus::ReadyForReview | TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled)).count();
        (tasks, m.replying())
    }

    /// ⌘⇧Esc and the account menu's "Stop all work": confirms with how many agents will stop.
    pub fn ask_stop_all(&mut self) {
        let (tasks, replying) = self.running_agents();
        let n = tasks + usize::from(replying);
        if n == 0 {
            self.show_toast("Nothing is running.");
            return;
        }
        let what = match (tasks, replying) {
            (0, _) => "Neko’s current reply".to_owned(),
            (t, false) => format!("{t} {}", if t == 1 { "task" } else { "tasks" }),
            (t, true) => format!("{t} {} and Neko’s current reply", if t == 1 { "task" } else { "tasks" }),
        };
        self.confirm(Ask {
            title: format!("Stop all {n} running {}?", if n == 1 { "agent" } else { "agents" }),
            detail: format!("{what} will stop now. Worktrees and files are kept."),
            ok: "Stop all work".into(),
            cancel: "Keep working".into(),
            destructive: true,
            call: Call::StopAll,
            done: if tasks == 0 { "Stopped Neko’s reply.".into() } else { format!("Stopped {tasks} {}. Worktrees are kept.", if tasks == 1 { "task" } else { "tasks" }) },
            field: None,
            list: Vec::new(),
            more: Vec::new(),
            toggle: None,
        });
    }

    /// "Uninstall Neko…" (`Uninstaller.confirmAndRun`): Swift's words and its checkbox, a coral button. Yes stops all work, then moves the app
    /// (and, if ticked, Neko's data) to the Trash and quits (`run_uninstall`).
    pub fn ask_uninstall(&mut self) {
        use crate::shell::uninstall as u;
        if !u::available() {
            self.show_toast(u::UNAVAILABLE);
            return;
        }
        self.confirm_uninstall();
    }

    /// The question itself (Swift's alert), asked whether or not this is an installed app (`ask_uninstall` checks that).
    pub fn confirm_uninstall(&mut self) {
        use crate::shell::uninstall as u;
        self.confirm(Ask {
            title: u::TITLE.to_owned(),
            detail: u::DETAIL.to_owned(),
            ok: u::OK.to_owned(),
            cancel: u::CANCEL.to_owned(),
            destructive: true,
            call: Call::StopAll,
            done: String::new(),
            field: None,
            list: Vec::new(),
            more: Vec::new(),
            toggle: Some(u::DATA_CHECKBOX.to_owned()),
        });
    }

    /// "Clear finished" in All sessions: removes every completed or cancelled task (in the workspace chosen, or all).
    pub fn ask_clear_finished(&mut self) {
        let workspace = self.workspace_scope.clone();
        let Some(m) = self.summary.as_ref() else { return };
        let count = m.tasks.iter().filter(|t| matches!(t.status, TaskStatus::Completed | TaskStatus::Cancelled) && workspace.as_deref().is_none_or(|w| m.workspace_of(&t.id) == w)).count();
        if count == 0 {
            self.show_toast("Nothing finished to clear.");
            return;
        }
        self.confirm(Ask {
            title: format!("Clear {count} finished {}?", if count == 1 { "task" } else { "tasks" }),
            detail: "Removes every completed or cancelled task from Neko’s history. Failed tasks stay so they can be retried. Your files and worktrees stay as they are.".into(),
            ok: "Clear finished".into(),
            cancel: "Keep them".into(),
            destructive: true,
            call: Call::ClearFinished { workspace_id: workspace },
            done: "Cleared finished tasks.".to_owned(),
            field: None,
            list: Vec::new(),
            more: Vec::new(),
            toggle: None,
        });
    }
}

impl App {
    /// A card dropped on Done while its work is live or waiting: moving it there stops the work, so it asks first (the web's `moveTask`).
    pub fn ask_cancel_by_move(&mut self, id: &str) {
        let title = self.title_of(id);
        self.confirm(Ask {
            title: format!("Cancel “{title}”?"),
            detail: "It hasn’t been reviewed yet, so moving it to Done stops the work. Its files and worktree stay on disk.".into(),
            ok: "Cancel task".into(),
            cancel: "Keep working".into(),
            destructive: true,
            call: Call::Stop { task_id: id.to_owned() },
            done: format!("Cancelled “{title}”. Its files and worktree stay on disk."),
            field: None,
            list: Vec::new(),
            more: Vec::new(),
            toggle: None,
        });
    }

    /// "Close…" on a pull request: a dialog with the reason (posted on the pull request as a comment) and a coral button.
    pub fn ask_close_pr(&mut self, id: &str) {
        let Some(row) = self.summary.as_ref().and_then(|m| m.pull_requests.iter().find(|r| r.id == id)).cloned() else { return };
        self.pages.set_field(ASK_FIELD, row.reason.as_deref().unwrap_or(""));
        let number = if row.number.is_empty() { "the pull request".to_owned() } else { row.number.clone() };
        self.confirm(Ask {
            title: format!("Close {number}?"),
            detail: "The comment goes on the pull request, so the next person knows why. Its task is cancelled once GitHub confirms.".into(),
            ok: "Close pull request".into(),
            cancel: "Keep it".into(),
            destructive: true,
            call: Call::ClosePr { task_id: id.to_owned(), comment: String::new() },
            done: format!("Closing {number}. Its task is cancelled once GitHub confirms."),
            field: Some(AskField { label: "Why (posted as a comment)".into(), placeholder: "Why is it being closed?".into() }),
            list: Vec::new(),
            more: Vec::new(),
            toggle: None,
        });
    }

    /// "Merge all ready": lists the pull requests, and on yes asks Neko to merge each once its checks pass (the web's `mergeAll`).
    pub fn ask_merge_all(&mut self) {
        let ready = self.ready_pull_requests();
        let Some((first, rest)) = ready.split_first() else {
            self.show_toast("No pull request is ready to merge.");
            return;
        };
        let n = ready.len();
        let noun = if n == 1 { "pull request" } else { "pull requests" };
        let list = ready.iter().map(|r| format!("{} · {} · {}", r.number, if r.repository.is_empty() { "—" } else { r.repository.as_str() }, r.title)).collect();
        let call = |r: &crate::link::PrRow| Call::SetMergeWhenReady { task_id: r.id.clone(), enabled: true };
        let (call_first, more) = (call(first), rest.iter().map(call).collect());
        self.confirm(Ask {
            title: format!("Merge {n} {noun}?"),
            detail: "Each is squashed with an empty body once its checks pass and nothing conflicts. A failure or a conflict stops that one and tells you.".into(),
            ok: format!("Merge {n}"),
            cancel: "Cancel".into(),
            destructive: false,
            call: call_first,
            done: "Neko merges each one as soon as its checks pass.".into(),
            field: None,
            list,
            more,
            toggle: None,
        });
    }

    /// "Review pull requests": hands every open one back to its agent to re-check (the web's "Re-check all").
    pub fn ask_review_prs(&mut self) {
        let n = self.pr_rows().iter().filter(|r| matches!(r.verdict, crate::link::Verdict::Ready | crate::link::Verdict::Close)).count();
        if n == 0 {
            self.show_toast("No open pull request to re-check.");
            return;
        }
        let workspace_id = self.scope().map(str::to_owned);
        self.confirm(Ask {
            title: format!("Re-check {n} open pull {}?", if n == 1 { "request" } else { "requests" }),
            detail: "Each task’s agent updates its pull request with main, re-checks the cause against the evidence, cuts it to the smallest fix, and says whether it is ready, should be closed, or needs your decision. Agents run a few at a time per repository.".into(),
            ok: "Re-check them".into(),
            cancel: "Cancel".into(),
            destructive: false,
            call: Call::ReviewPullRequests { workspace_id },
            done: "Each agent is re-checking its pull request. They run a few at a time, so this takes a while.".into(),
            field: None,
            list: Vec::new(),
            more: Vec::new(),
            toggle: None,
        });
    }

    /// "Merge without review" for a repository: turning it on asks first (it skips the approving review GitHub requires);
    /// turning it off is one press.
    pub fn toggle_merge_without_review(&mut self, repository: &str) {
        let on = self.summary.as_ref().is_some_and(|m| m.merge_without_review.iter().any(|r| r == repository));
        let name = repo_name(repository);
        if on {
            self.call(Call::SetMergeWithoutReview { repository: repository.to_owned(), enabled: false });
            self.show_toast(&format!("Neko will wait for an approving review in {name}."));
            return;
        }
        self.confirm(Ask {
            title: format!("Merge without review in {name}?"),
            detail: "Neko may then merge this repository’s pull requests without the approving review GitHub requires (an administrator merge), once checks pass. You can turn it off again here.".into(),
            ok: "Allow it".into(),
            cancel: "Keep requiring review".into(),
            destructive: true,
            call: Call::SetMergeWithoutReview { repository: repository.to_owned(), enabled: true },
            done: format!("Neko may now merge pull requests in {name} without review."),
            field: None,
            list: Vec::new(),
            more: Vec::new(),
            toggle: None,
        });
    }
}

// ---- parity batch D: the confirmations of Settings, the theme files and the plugins -----------------------------------

impl App {
    /// "Start over…" (Settings, AI and agents): the web's exact warning.
    pub fn ask_start_over(&mut self) {
        self.confirm(Ask {
            title: "Start over?".into(),
            detail: "Tasks without a pull request are removed, along with their worktrees and unpublished changes. Tasks with a pull request stay. This can’t be undone.".into(),
            ok: "Remove tasks and start over".into(),
            cancel: "Cancel".into(),
            destructive: true,
            call: Call::WipeTasks,
            done: "Started over. Tasks without a pull request were removed.".into(),
            field: None,
            list: Vec::new(),
            more: Vec::new(),
            toggle: None,
        });
    }

    /// "Roll back" (Settings, Neko updates): back to the web UI build the last swap replaced.
    pub fn ask_roll_back_web(&mut self) {
        self.confirm(Ask {
            title: "Roll back the web UI?".into(),
            detail: "The web UI goes back to the previous build. Pages reload on their own. The next published change brings the newer one back.".into(),
            ok: "Roll back".into(),
            cancel: "Cancel".into(),
            destructive: true,
            call: Call::PreviousWebBuild,
            done: "Rolled the web UI back.".into(),
            field: None,
            list: Vec::new(),
            more: Vec::new(),
            toggle: None,
        });
    }

    /// Remove one of the person's theme files (the web's `ConfirmDialog` in Appearance).
    pub fn ask_remove_theme(&mut self, id: &str) {
        let Some(theme) = self.theme_entries().into_iter().find(|t| t.id == id && t.file.is_some()) else { return };
        let file = theme.file.clone().unwrap_or_default();
        self.confirm(Ask {
            title: format!("Remove {}?", theme.name),
            detail: format!("Deletes {file} from your themes folder. {}", if theme.overrides { "The built-in theme with this name comes back." } else { "You can ask Neko to make it again, but this cannot be undone." }),
            ok: "Remove".into(),
            cancel: "Cancel".into(),
            destructive: true,
            call: Call::DropTheme { id: theme.id.clone() },
            done: format!("Removed {}.{}", theme.name, if theme.overrides { " The built-in came back." } else { "" }),
            field: None,
            list: Vec::new(),
            more: Vec::new(),
            toggle: None,
        });
    }

    /// Remove a plugin: its folder and everything in it (the web's `remove` in Customisations).
    pub fn ask_remove_plugin(&mut self, id: &str) {
        let (label, path) = self.plugin_ui.listings.as_ref().and_then(|l| l.iter().find(|p| p.id == id)).map(|p| (if p.title.is_empty() { p.id.clone() } else { p.title.clone() }, p.path.clone())).unwrap_or_else(|| (id.to_owned(), String::new()));
        self.confirm(Ask {
            title: format!("Remove “{label}”?"),
            detail: format!("This deletes its folder{} and everything in it. It cannot be undone. Turn it off instead if you may want it back.", if path.is_empty() { String::new() } else { format!(" ({path})") }),
            ok: "Remove".into(),
            cancel: "Cancel".into(),
            destructive: true,
            call: Call::RemovePlugin { id: id.to_owned() },
            done: format!("Removed {label}."),
            field: None,
            list: Vec::new(),
            more: Vec::new(),
            toggle: None,
        });
    }

    /// "Make me a theme": what should it look like, then one message to Neko (the web's ThemeDialog).
    pub fn ask_make_theme(&mut self) {
        self.pages.set_field(ASK_FIELD, "");
        let dir = self.extra.themes_dir.clone().unwrap_or_default();
        self.confirm(Ask {
            title: "Make me a theme".into(),
            detail: "Describe how it should look. Neko opens a task, writes a theme file into your themes folder and checks it. It appears in the pickers when the file is saved, and you can remove it any time.".into(),
            ok: "Ask Neko".into(),
            cancel: "Cancel".into(),
            destructive: false,
            call: Call::MakeTheme { dir, wish: String::new() },
            done: "Asked Neko to make a theme.".into(),
            field: Some(AskField { label: "What should it look like?".into(), placeholder: "A dark theme in deep teal with a soft gold accent…".into() }),
            list: Vec::new(),
            more: Vec::new(),
            toggle: None,
        });
    }

    /// "Restore default" (Settings, Rulebook): the person's own rules go and Neko's are used again.
    pub fn ask_restore_rulebook(&mut self) {
        self.confirm(Ask {
            title: "Restore the default rulebook?".into(),
            detail: "Your own rules are removed and Neko’s built-in rules are used again from the next agent that starts.".into(),
            ok: "Restore default".into(),
            cancel: "Cancel".into(),
            destructive: false,
            call: Call::SetRulebook { text: String::new() },
            done: "Restored the default rulebook.".into(),
            field: None,
            list: Vec::new(),
            more: Vec::new(),
            toggle: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::{Summary, TaskMeta, TaskRow, Wait};
    use neko_protocol::workbench::TaskStatus::*;

    fn app_with(status: TaskStatus, repository: &str, pr: Option<&str>, land: &[&str]) -> App {
        let mut app = App::headless();
        let row = TaskRow { id: "t".into(), key: None, title: "Fix it".into(), status, updated_at_ms: 0, goal: String::new(), wait: Wait::None, progress: None, line: Default::default() };
        let mut summary = Summary { tasks: vec![row], land_on_main: land.iter().map(|s| (*s).to_owned()).collect(), ..Summary::default() };
        summary.meta.insert("t".into(), TaskMeta { repository: repository.into(), pull_request: pr.map(str::to_owned), ..TaskMeta::default() });
        app.summary = Some(summary);
        app
    }

    fn labels(app: &App) -> Vec<String> {
        entries(app, "t").into_iter().map(|e| e.label).collect()
    }

    #[test]
    fn each_item_shows_under_the_webs_condition() {
        // A running task: Stop, not Delete or Retry.
        let running = app_with(Building, "", None, &[]);
        assert_eq!(labels(&running), ["Rename…", "Mark as unread", "Copy link", "Open in web UI", "Stop"]);
        // A queued task can be moved up.
        assert!(labels(&app_with(Queued, "", None, &[])).contains(&"Run next".to_owned()));
        // Failed and cancelled: retry, and delete (finished).
        let failed = labels(&app_with(Failed, "", None, &[]));
        assert!(failed.contains(&"Retry from start…".to_owned()) && failed.contains(&"Delete…".to_owned()) && !failed.contains(&"Stop".to_owned()));
        // Completed: delete but no retry.
        let done = labels(&app_with(Completed, "", None, &[]));
        assert!(done.contains(&"Delete…".to_owned()) && !done.contains(&"Retry from start…".to_owned()));
        // Ready for review: publish only while there is no open pull request; cancel instead of stop.
        let review = labels(&app_with(ReadyForReview, "", None, &[]));
        assert!(review.contains(&"Publish pull request".to_owned()) && review.contains(&"Cancel task".to_owned()));
        assert!(!labels(&app_with(ReadyForReview, "", Some("https://github.com/x/y/pull/1"), &[])).contains(&"Publish pull request".to_owned()));
        // Awaiting approval can be cancelled.
        assert!(labels(&app_with(AwaitingApproval, "", None, &[])).contains(&"Cancel task".to_owned()));
    }

    #[test]
    fn land_on_main_is_a_checkbox_per_repository_shown_once_it_has_one() {
        assert!(!labels(&app_with(Building, "", None, &[])).iter().any(|l| l.starts_with("Land on main")));
        let off = entries(&app_with(Building, "/Users/me/code/neko", None, &[]), "t");
        let item = off.iter().find(|e| e.act == Act::LandOnMain).unwrap();
        assert_eq!((item.label.as_str(), item.checked), ("Land on main in neko", Some(false)));
        let on = entries(&app_with(Building, "/Users/me/code/neko", None, &["/Users/me/code/neko"]), "t");
        assert_eq!(on.iter().find(|e| e.act == Act::LandOnMain).unwrap().checked, Some(true));
    }

    #[test]
    fn delete_is_last_and_dangerous_and_nothing_is_sent_before_the_dialog_says_yes() {
        let mut app = app_with(Completed, "", None, &[]);
        let items = entries(&app, "t");
        assert!(items.last().is_some_and(|e| e.act == Act::Delete && e.danger));
        app.do_action("t", Act::Delete);
        assert!(app.outbox.is_empty(), "the request waits for the dialog");
        let ask = app.dialog.clone().expect("a dialog is open");
        assert_eq!(ask.call, Call::Delete { task_id: "t".into() });
        assert!(ask.destructive);
        app.ask_no();
        assert!(app.dialog.is_none() && app.outbox.is_empty(), "Keep it sends nothing");
        app.do_action("t", Act::Delete);
        app.ask_yes();
        assert_eq!(app.outbox, vec![Call::Delete { task_id: "t".into() }]);
    }

    #[test]
    fn retry_and_stop_ask_first_and_publish_and_run_next_do_not() {
        let mut app = app_with(Failed, "", None, &[]);
        app.do_action("t", Act::Retry);
        assert!(app.outbox.is_empty() && app.dialog.as_ref().is_some_and(|a| a.call == Call::Retry { task_id: "t".into() }));
        let mut app = app_with(Building, "", None, &[]);
        app.do_action("t", Act::Stop);
        assert!(app.outbox.is_empty() && app.dialog.as_ref().is_some_and(|a| a.call == Call::Stop { task_id: "t".into() }));
        let mut app = app_with(ReadyForReview, "", None, &[]);
        app.do_action("t", Act::Publish);
        assert_eq!(app.outbox, vec![Call::Publish { task_id: "t".into() }]);
        let mut app = app_with(Queued, "", None, &[]);
        app.do_action("t", Act::RunNext);
        assert_eq!(app.outbox, vec![Call::RunNext { task_id: "t".into() }]);
    }

    #[test]
    fn land_on_main_flips_the_repositorys_delivery() {
        let mut app = app_with(Building, "/r/neko", None, &[]);
        app.do_action("t", Act::LandOnMain);
        assert_eq!(app.outbox, vec![Call::SetLandOnMain { repository: "/r/neko".into(), enabled: true }]);
        let mut app = app_with(Building, "/r/neko", None, &["/r/neko"]);
        app.do_action("t", Act::LandOnMain);
        assert_eq!(app.outbox, vec![Call::SetLandOnMain { repository: "/r/neko".into(), enabled: false }]);
    }

    #[test]
    fn rename_edits_the_title_and_enter_saves_escape_cancels() {
        let mut app = app_with(Building, "", None, &[]);
        app.do_action("t", Act::Rename);
        assert_eq!(app.selected.as_deref(), Some("t"));
        let r = app.rename.as_mut().expect("the title is a field now");
        assert_eq!(r.edit.text, "Fix it");
        r.edit.replace_text("Better name".to_owned());
        app.finish_rename();
        assert!(app.rename.is_none());
        assert!(app.outbox.contains(&Call::Rename { task_id: "t".into(), title: "Better name".into() }));
        app.do_action("t", Act::Rename);
        app.cancel_rename();
        assert!(app.rename.is_none());
        assert_eq!(app.outbox.iter().filter(|c| matches!(c, Call::Rename { .. })).count(), 1, "Esc sends nothing");
        // An empty name lets Neko name it again: it is sent as an empty title.
        app.do_action("t", Act::Rename);
        app.rename.as_mut().unwrap().edit.replace_text(String::new());
        app.finish_rename();
        assert!(app.outbox.contains(&Call::Rename { task_id: "t".into(), title: String::new() }));
    }

    #[test]
    fn stop_all_counts_what_runs_and_says_nothing_is_running_when_nothing_is() {
        let mut app = app_with(Completed, "", None, &[]);
        app.ask_stop_all();
        assert!(app.dialog.is_none() && app.toast_words().contains("Nothing is running."));
        let mut app = app_with(Building, "", None, &[]);
        let row = |id: &str, status| TaskRow { id: id.into(), key: None, title: id.into(), status, updated_at_ms: 0, goal: String::new(), wait: Wait::None, progress: None, line: Default::default() };
        app.summary.as_mut().unwrap().tasks.extend([row("a", Planning), row("b", AwaitingApproval), row("c", ReadyForReview), row("d", Completed)]);
        app.ask_stop_all();
        let ask = app.dialog.clone().unwrap();
        assert_eq!(ask.title, "Stop all 3 running agents?", "building, planning and awaiting approval run; review and done do not");
        assert_eq!(ask.call, Call::StopAll);
        assert!(app.outbox.is_empty());
    }

    #[test]
    fn clear_finished_counts_completed_and_cancelled_only_and_stays_in_the_workspace() {
        let mut app = app_with(Completed, "", None, &[]);
        let row = |id: &str, status| TaskRow { id: id.into(), key: None, title: id.into(), status, updated_at_ms: 0, goal: String::new(), wait: Wait::None, progress: None, line: Default::default() };
        app.summary.as_mut().unwrap().tasks.extend([row("a", Cancelled), row("b", Failed), row("c", Building)]);
        app.ask_clear_finished();
        let ask = app.dialog.clone().unwrap();
        assert_eq!(ask.title, "Clear 2 finished tasks?", "failed tasks stay so they can be retried");
        assert_eq!(ask.call, Call::ClearFinished { workspace_id: None });
    }
}
