//! The workspace switcher: a 32 point row under the sessions column's search field, "All workspaces" or one workspace. It scopes
//! the sessions list (and its counts), Routines, the Calendar's rows of Neko's own work and Home's composer (its default
//! workspace). It persists: the choice is kept in Rune's own preferences store, beside the window's widths (`app.rs`).

use crate::app::App;
use crate::link::{CalendarData, Summary};

/// The key the choice is kept under.
pub const KEY: &str = "workspace";

impl App {
    /// The workspace the window is scoped to, if it still exists (a workspace that went is "All workspaces").
    pub fn scope(&self) -> Option<&str> {
        let id = self.workspace_scope.as_deref()?;
        self.summary.as_ref().is_none_or(|m| m.workspaces.iter().any(|w| w.id == id)).then_some(id)
    }

    /// What the switcher says: "All workspaces" or the workspace's name.
    pub fn scope_name(&self) -> String {
        self.scope().and_then(|id| self.summary.as_ref().and_then(|m| m.workspaces.iter().find(|w| w.id == id))).map_or_else(|| "All workspaces".to_owned(), |w| w.name.clone())
    }

    /// Chooses the scope (None: all workspaces). Home's composer follows, and the choice is kept.
    pub fn set_scope(&mut self, id: Option<String>) {
        self.workspace_scope = id.clone();
        self.scope_open = false;
        self.all.workspace = id.clone();
        if let (Some(id), Some(summary), Some(form)) = (id.as_deref(), self.summary.as_ref(), self.new_form.as_mut())
            && let Some(at) = summary.workspaces.iter().position(|w| w.id == id)
        {
            form.workspace = at;
        }
        self.scope_dirty = true;
    }

    /// Home's composer starts in the scoped workspace.
    pub fn apply_scope_to_home(&mut self) {
        if let (Some(id), Some(summary), Some(form)) = (self.scope().map(str::to_owned), self.summary.as_ref(), self.new_form.as_mut())
            && let Some(at) = summary.workspaces.iter().position(|w| w.id == id)
        {
            form.workspace = at;
        }
    }

    /// The calendar's rows of Neko's work that belong to the scope: a task by its workspace, a routine by its own; a scheduled
    /// plan's rows carry no workspace and stay.
    pub fn scoped_neko_rows<'a>(&self, data: &'a CalendarData) -> Vec<&'a neko_protocol::calendar::NekoCalendarItem> {
        use neko_protocol::calendar::NekoItemKind;
        let Some(scope) = self.scope() else { return data.neko.iter().collect() };
        let summary: Option<&Summary> = self.summary.as_ref();
        data.neko
            .iter()
            .filter(|item| match item.kind {
                NekoItemKind::Task => {
                    let id = item.route.strip_prefix("tasks?task=").map(|r| r.split('&').next().unwrap_or(r)).unwrap_or(item.id.as_str());
                    summary.is_none_or(|m| m.task(id).is_none() || m.workspace_of(id) == scope)
                }
                NekoItemKind::Routine => self.pages.routines.as_ref().and_then(|r| r.views.iter().find(|v| v.routine.id == item.id || item.route.contains(&v.routine.id))).is_none_or(|v| v.routine.workspace_id == scope),
                NekoItemKind::Schedule => true,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::{TaskMeta, TaskRow, Wait, WorkspaceRow};
    use neko_protocol::workbench::TaskStatus;

    fn app() -> App {
        let mut app = App::headless();
        let row = |id: &str| TaskRow { id: id.into(), key: None, title: id.into(), status: TaskStatus::Building, updated_at_ms: 1, goal: String::new(), wait: Wait::None, progress: None };
        let mut summary = Summary { tasks: vec![row("a"), row("b")], workspaces: vec![WorkspaceRow { id: "w1".into(), name: "One".into() }, WorkspaceRow { id: "w2".into(), name: "Two".into() }], ..Summary::default() };
        summary.meta.insert("a".into(), TaskMeta { workspace_id: "w1".into(), ..TaskMeta::default() });
        summary.meta.insert("b".into(), TaskMeta { workspace_id: "w2".into(), ..TaskMeta::default() });
        app.summary = Some(summary);
        app
    }

    #[test]
    fn the_scope_names_a_workspace_and_falls_back_to_all_when_it_goes() {
        let mut app = app();
        assert_eq!(app.scope_name(), "All workspaces");
        app.set_scope(Some("w2".into()));
        assert_eq!((app.scope(), app.scope_name().as_str()), (Some("w2"), "Two"));
        assert!(app.scope_dirty, "the choice is kept");
        app.summary.as_mut().unwrap().workspaces.retain(|w| w.id != "w2");
        assert_eq!((app.scope(), app.scope_name().as_str()), (None, "All workspaces"), "a workspace that went is no scope");
    }

    #[test]
    fn home_starts_in_the_scoped_workspace() {
        let mut app = app();
        app.open_new();
        app.set_scope(Some("w2".into()));
        assert_eq!(app.new_form.as_ref().unwrap().workspace, 1);
        app.new_form.as_mut().unwrap().workspace = 0;
        app.apply_scope_to_home();
        assert_eq!(app.new_form.as_ref().unwrap().workspace, 1);
    }

    #[test]
    fn calendar_rows_of_other_workspaces_drop_out() {
        use neko_protocol::calendar::{NekoCalendarItem, NekoItemKind};
        let mut app = app();
        let item = |id: &str| NekoCalendarItem { id: id.into(), kind: NekoItemKind::Task, title: id.into(), at_ms: 0, all_day: true, status: String::new(), detail: String::new(), route: format!("tasks?task={id}") };
        let status = neko_protocol::calendar::CalendarStatus { access: neko_protocol::calendar::CalendarAccess::Granted, source: String::new(), last_sync_ms: None, events: 0, truncated: false, enabled: true };
        let data = CalendarData { events: Vec::new(), neko: vec![item("a"), item("b")], status, from_ms: 0, to_ms: 0 };
        assert_eq!(app.scoped_neko_rows(&data).len(), 2);
        app.set_scope(Some("w1".into()));
        let rows: Vec<&str> = app.scoped_neko_rows(&data).iter().map(|i| i.id.as_str()).collect();
        assert_eq!(rows, ["a"]);
    }
}
