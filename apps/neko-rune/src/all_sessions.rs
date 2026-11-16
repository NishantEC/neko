//! "All sessions": what "View all" in Recent opens. Every session, grouped by the web's stage (`work/model.ts` `stageOf`:
//! Needs you, Ready to review, Running, Up next, Done), newest first inside each, with a search field (`/` focuses it), a
//! workspace filter, J and K to move and Return to open, and in the header's overflow "Show failed and cancelled" and
//! "Clear finished" (after a dialog). The filters are the web's (`work/filters.ts` `matches`): failed and cancelled
//! tasks stay out until asked for.

use neko_protocol::workbench::TaskStatus;
use rune::{Key, KeyEvent, TextEdit};

use crate::app::App;
use crate::link::TaskRow;
use crate::views::FullView;

/// The board's columns, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stage {
    NeedsYou,
    Review,
    Running,
    UpNext,
    Done,
}

impl Stage {
    pub const ALL: [Stage; 5] = [Stage::NeedsYou, Stage::Review, Stage::Running, Stage::UpNext, Stage::Done];

    /// The web's `stageOf`.
    pub fn of(status: TaskStatus) -> Stage {
        match status {
            TaskStatus::AwaitingApproval | TaskStatus::Failed => Stage::NeedsYou,
            TaskStatus::ReadyForReview => Stage::Review,
            TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing => Stage::Running,
            TaskStatus::Queued => Stage::UpNext,
            TaskStatus::Completed | TaskStatus::Cancelled => Stage::Done,
        }
    }

    /// The web's `STAGE_TITLE`.
    pub fn title(self) -> &'static str {
        match self {
            Stage::NeedsYou => "Needs you",
            Stage::Review => "Ready to review",
            Stage::Running => "Running",
            Stage::UpNext => "Up next",
            Stage::Done => "Done",
        }
    }
}

/// What the view keeps between frames.
pub struct AllState {
    pub search: TextEdit,
    /// The row J and K are on (an index into [`App::all_rows`]).
    pub cursor: usize,
    /// J or K was pressed: the cursor row is drawn.
    pub cursor_shown: bool,
    /// "Show failed and cancelled".
    pub include_stopped: bool,
    /// The workspace filter: None is every workspace.
    pub workspace: Option<String>,
    pub menu_open: bool,
}

impl Default for AllState {
    fn default() -> Self {
        AllState { search: TextEdit::new(""), cursor: 0, cursor_shown: false, include_stopped: false, workspace: None, menu_open: false }
    }
}

impl App {
    /// The rows the view shows, in stage order and newest first inside a stage: the web's `byStage` flattened.
    pub fn all_rows(&self) -> Vec<&TaskRow> {
        let Some(summary) = self.summary.as_ref() else { return Vec::new() };
        let q = self.all.search.text.trim().to_lowercase();
        let mut rows: Vec<&TaskRow> = summary
            .tasks
            .iter()
            .filter(|t| self.all.workspace.as_deref().is_none_or(|w| summary.workspace_of(&t.id) == w))
            .filter(|t| self.all.include_stopped || !matches!(t.status, TaskStatus::Failed | TaskStatus::Cancelled))
            .filter(|t| q.is_empty() || t.title.to_lowercase().contains(&q))
            .collect();
        rows.sort_by(|a, b| Stage::of(a.status).cmp_index().cmp(&Stage::of(b.status).cmp_index()).then(b.updated_at_ms.cmp(&a.updated_at_ms)));
        rows
    }

    /// Opens the view with the workspace the window is scoped to.
    pub fn enter_all_sessions(&mut self) {
        self.all.workspace = self.workspace_scope.clone();
        self.all.cursor = 0;
        self.all.menu_open = false;
    }

    /// The keys of the view: `/` focuses the search, J and K move, Return opens. False when the key is not the view's, and
    /// always false while a field has the keyboard (a letter typed in the search is a letter).
    pub fn all_key(&mut self, ev: &KeyEvent, tx: &mut rune::Tx) -> bool {
        if self.view != Some(FullView::AllSessions) || self.all.search.focused || ev.mods.cmd || ev.mods.ctrl || ev.mods.alt {
            return false;
        }
        let rows = self.all_rows().len();
        match &ev.key {
            Key::Text(t) if t == "/" => {
                tx.focus("all-search", true);
                self.all.search.focused = true;
                true
            }
            Key::Text(t) if t == "j" || t == "J" => {
                self.all.cursor = (self.all.cursor + 1).min(rows.saturating_sub(1));
                self.all.cursor_shown = true;
                true
            }
            Key::Text(t) if t == "k" || t == "K" => {
                self.all.cursor = self.all.cursor.saturating_sub(1);
                self.all.cursor_shown = true;
                true
            }
            Key::Enter if rows > 0 => {
                let id = self.all_rows().get(self.all.cursor.min(rows - 1)).map(|t| t.id.clone());
                if let Some(id) = id {
                    self.select(&id);
                }
                true
            }
            _ => false,
        }
    }
}

impl Stage {
    fn cmp_index(self) -> usize {
        Stage::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::{Summary, TaskMeta, TaskRow, Wait};
    use TaskStatus::*;

    fn row(id: &str, title: &str, status: TaskStatus, at: i64) -> TaskRow {
        TaskRow { id: id.into(), key: None, title: title.into(), status, updated_at_ms: at, goal: String::new(), wait: Wait::None, progress: None }
    }

    fn app() -> App {
        let mut app = App::headless();
        let mut summary = Summary {
            tasks: vec![row("a", "Fix login", Building, 5), row("b", "Add search", Completed, 9), row("c", "Old failure", Failed, 7), row("d", "Cancelled one", Cancelled, 3), row("e", "Review me", ReadyForReview, 1), row("f", "Plan this", AwaitingApproval, 2), row("g", "Fix signup", Queued, 4)],
            ..Summary::default()
        };
        for (id, w) in [("a", "w1"), ("b", "w1"), ("c", "w2"), ("d", "w2"), ("e", "w2"), ("f", "w1"), ("g", "w2")] {
            summary.meta.insert(id.into(), TaskMeta { workspace_id: w.into(), ..TaskMeta::default() });
        }
        app.summary = Some(summary);
        app
    }

    fn ids(app: &App) -> Vec<String> {
        app.all_rows().into_iter().map(|t| t.id.clone()).collect()
    }

    #[test]
    fn rows_are_in_stage_order_newest_first_and_failed_and_cancelled_wait_to_be_asked_for() {
        let mut app = app();
        // Needs you (f), Ready to review (e), Running (a), Up next (g), Done (b); the failed and cancelled are hidden.
        assert_eq!(ids(&app), ["f", "e", "a", "g", "b"]);
        app.all.include_stopped = true;
        assert_eq!(ids(&app), ["c", "f", "e", "a", "g", "b", "d"], "failed joins Needs you (newest first), cancelled joins Done");
    }

    #[test]
    fn search_and_workspace_narrow_the_rows() {
        let mut app = app();
        app.all.search.replace_text("fix");
        assert_eq!(ids(&app), ["a", "g"]);
        app.all.workspace = Some("w2".into());
        assert_eq!(ids(&app), ["g"]);
        app.all.search.replace_text("");
        app.all.include_stopped = true;
        assert_eq!(ids(&app), ["c", "e", "g", "d"]);
    }

    #[test]
    fn stages_are_the_webs() {
        assert_eq!(Stage::of(AwaitingApproval), Stage::NeedsYou);
        assert_eq!(Stage::of(Failed), Stage::NeedsYou);
        assert_eq!(Stage::of(ReadyForReview), Stage::Review);
        assert_eq!(Stage::of(Reviewing), Stage::Running);
        assert_eq!(Stage::of(Queued), Stage::UpNext);
        assert_eq!(Stage::of(Cancelled), Stage::Done);
        assert_eq!(Stage::ALL.map(Stage::title), ["Needs you", "Ready to review", "Running", "Up next", "Done"]);
    }
}
