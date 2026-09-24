//! The persistent native workspace, separate from the quick launcher.
mod tools;

use crate::{text_field::TextField, theme};
use gpui::{
    App, Context, Entity, Global, IntoElement, KeyDownEvent, ParentElement, Render, SharedString,
    Styled, TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, div, prelude::*, px,
};
use neko_client::NekoClient;
use neko_protocol::{
    Request, Response,
    workbench::{Command, Secret, Snapshot, TaskStatus, Workspace},
};
use std::{
    rc::Rc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
struct WorkspaceWindow(Option<WindowHandle<WorkspaceRoot>>);
impl Global for WorkspaceWindow {}

/// Open or focus the persistent app window. Closing it leaves the daemon's
/// work running; opening again fetches its authoritative snapshot.
pub fn open(client: NekoClient, cx: &mut App) {
    let evidence = workspace_evidence();
    if let Some(existing) = cx.try_global::<WorkspaceWindow>().and_then(|slot| slot.0) {
        if existing
            .update(cx, |_, window, _| {
                if evidence {
                    show_evidence_window(window);
                } else {
                    window.activate_window();
                }
            })
            .is_ok()
        {
            if !evidence {
                cx.activate(true);
            }
            return;
        }
    }
    let bounds = gpui::Bounds::centered(None, gpui::size(px(1100.), px(760.)), cx);
    match cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(gpui::size(px(900.), px(600.))),
            titlebar: Some(TitlebarOptions {
                title: Some("Neko Workspace".into()),
                ..Default::default()
            }),
            kind: WindowKind::Normal,
            is_resizable: true,
            is_minimizable: true,
            is_movable: true,
            focus: !evidence,
            show: true,
            window_background: WindowBackgroundAppearance::Opaque,
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| WorkspaceRoot::new(client, cx)),
    ) {
        Ok(window) => {
            cx.set_global(WorkspaceWindow(Some(window)));
            if !evidence {
                cx.activate(true);
            }
            let _ = window.update(cx, |root, window, cx| {
                if evidence {
                    show_evidence_window(window);
                } else {
                    window.activate_window();
                }
                root.request(Command::Snapshot, cx);
            });
            if evidence {
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(750))
                        .await;
                    let _ = window.update(cx, |root, window, _| {
                        report_evidence_window(window);
                        eprintln!("neko: workspace evidence state loaded={} busy={} connected={} watch={}", root.loaded, root.busy, root.client.is_connected(), root.away_enabled);
                    });
                })
                .detach();
            }
        }
        Err(error) => eprintln!("neko: could not open workspace window: {error}"),
    }
}

fn workspace_evidence() -> bool {
    std::env::var_os("NEKO_SHOW_WORKSPACE").is_some()
}

fn evidence_view() -> Option<View> {
    if !workspace_evidence() {
        return None;
    }
    match std::env::var("NEKO_WORKSPACE_VIEW").as_deref() {
        Ok("workspaces") => Some(View::Workspaces),
        Ok("integrations") => Some(View::Integrations),
        _ => None,
    }
}

fn show_evidence_window(window: &Window) {
    if let Err(error) = crate::material::order_front_regardless(window) {
        eprintln!("neko: workspace evidence show failed: {error}");
    }
    report_evidence_window(window);
}

fn report_evidence_window(window: &Window) {
    match crate::material::window_number(window) {
        Ok(number) => eprintln!("neko: workspace window number {number}"),
        Err(error) => eprintln!("neko: workspace window number unavailable: {error}"),
    }
    match crate::material::is_key_window(window) {
        Ok(false) => eprintln!("neko: workspace evidence key window false"),
        Ok(true) => {
            eprintln!("neko: SAFETY WARNING: workspace evidence became key; hiding it immediately");
            let _ = crate::material::order_out(window);
        }
        Err(error) => {
            eprintln!("neko: workspace evidence key state unavailable: {error}; hiding it");
            let _ = crate::material::order_out(window);
        }
    }
}

/// The quick launcher can deep-open a task without guessing its workspace.
/// A fresh snapshot resolves both IDs before the detail is shown.
pub fn open_task(client: NekoClient, task_id: String, cx: &mut App) {
    open(client, cx);
    if let Some(window) = cx.try_global::<WorkspaceWindow>().and_then(|slot| slot.0) {
        let _ = window.update(cx, |root, _, cx| {
            root.pending_task = Some(task_id);
            root.view = View::Tasks;
            root.request(Command::Snapshot, cx);
            cx.notify();
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Tasks,
    Workspaces,
    Integrations,
}

pub struct WorkspaceRoot {
    client: NekoClient,
    snapshot: Snapshot,
    selection: Selection,
    pending_task: Option<String>,
    view: View,
    loaded: bool,
    busy: bool,
    refreshing: bool,
    error: Option<String>,
    transport_error: Option<String>,
    connection_error: Option<String>,
    notice: Option<String>,
    workspace_name: Entity<TextField>,
    repository: Entity<TextField>,
    instructions: Entity<TextField>,
    away_enabled: bool,
    creating_workspace: bool,
    task_title: Entity<TextField>,
    task_goal: Entity<TextField>,
    api_key: Entity<TextField>,
    server_label: Entity<TextField>,
    server_target: Entity<TextField>,
    server_args: Entity<TextField>,
    oauth_client_id: Entity<TextField>,
    responsibility_instruction: Entity<TextField>,
    responsibility_connections: Vec<String>,
    responsibility_prepare: bool,
    responsibility_editing: Option<String>,
    local_server: bool,
    trust_server: bool,
    appearance: Option<theme::Appearance>,
}

impl WorkspaceRoot {
    fn new(client: NekoClient, cx: &mut Context<Self>) -> Self {
        let api_key = input("Optional credential JSON (stored in Keychain)", cx);
        api_key.update(cx, |field, cx| field.set_masked(true, cx));
        // A single in-flight gate covers polling and every mutation. A poll
        // cannot return stale state over a more recent user action.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                if this
                    .update(cx, |root, cx| {
                        if !root.busy {
                            root.request(Command::Snapshot, cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            client,
            snapshot: Snapshot::default(),
            selection: Selection::default(),
            pending_task: if workspace_evidence() {
                std::env::var("NEKO_WORKSPACE_TASK")
                    .ok()
                    .filter(|id| !id.is_empty())
            } else {
                None
            },
            view: evidence_view().unwrap_or(View::Tasks),
            loaded: false,
            busy: false,
            refreshing: false,
            error: None,
            transport_error: None,
            connection_error: None,
            notice: None,
            workspace_name: input("Workspace name", cx),
            repository: input("/Users/you/Projects/repository", cx),
            instructions: input("How Neko should work in this repository", cx),
            away_enabled: false,
            creating_workspace: true,
            task_title: input("What needs doing?", cx),
            task_goal: input("Describe the outcome and how to verify it", cx),
            api_key,
            server_label: input("Name this connection", cx),
            server_target: input("https://server.example/mcp or /absolute/executable", cx),
            server_args: input("[\"/path/to/server.js\"]", cx),
            oauth_client_id: input("Optional registered OAuth client ID", cx),
            responsibility_instruction: input(
                "Watch my assigned issues and plan fixes for low-risk bugs",
                cx,
            ),
            responsibility_connections: vec![],
            responsibility_prepare: false,
            responsibility_editing: None,
            local_server: false,
            trust_server: false,
            appearance: None,
        }
    }

    fn request(&mut self, command: Command, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        let is_poll = matches!(&command, Command::Snapshot);
        self.refreshing = is_poll;
        let connecting = matches!(
            &command,
            Command::Mcp(neko_protocol::mcp_host::McpCommand::AddConnection { .. })
        );
        let saved = match &command {
            Command::SaveWorkspace { workspace } => Some(workspace.clone()),
            _ => None,
        };
        let new_task = matches!(
            &command,
            Command::CreateTask { .. } | Command::PlanIssue { .. }
        );
        let submitted_draft = match &command {
            Command::CreateTask { title, goal, .. } => Some((title.clone(), goal.clone())),
            _ => None,
        };
        let submitted_responsibility = match &command {
            Command::Mcp(neko_protocol::mcp_host::McpCommand::SaveResponsibility {
                responsibility,
            }) => Some(responsibility.clone()),
            _ => None,
        };
        let previous_tasks: Vec<String> = self
            .snapshot
            .tasks
            .iter()
            .map(|task| task.id.clone())
            .collect();
        let previous_workspaces: Vec<String> = self
            .snapshot
            .workspaces
            .iter()
            .map(|workspace| workspace.id.clone())
            .collect();
        if !is_poll {
            self.error = None;
            self.notice = None;
        }
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let result = client.request(Request::Workbench(command)).await;
            let _ = this.update(cx, |root, cx| {
                root.busy = false;
                let was_loaded = root.loaded;
                match result {
                    Ok(Response::Workbench(snapshot)) => {
                        root.snapshot = snapshot;
                        root.loaded = true;
                        root.transport_error = None;
                        if !is_poll { root.error = None; }
                        if connecting { root.connection_error = None; root.notice = Some("MCP connection saved. Discover tools, then explicitly grant access.".into()); }
                        if let Some(saved) = saved {
                            // New workspace IDs are assigned by the daemon. Its
                            // canonical path may differ from a typed ~/ path.
                            let workspace = root.snapshot.workspaces.iter().rev().find(|workspace|
                                if saved.id.is_empty() { workspace.name == saved.name && !previous_workspaces.contains(&workspace.id) } else { workspace.id == saved.id }
                            ).cloned();
                            if let Some(workspace) = workspace { root.select_workspace(&workspace.id, cx); }
                            root.notice = Some("Workspace saved.".into());
                        }
                        if new_task {
                            root.selection.task = root.snapshot.tasks.iter().rev().find(|task|
                                root.selection.includes(&task.workspace_id) && !previous_tasks.contains(&task.id)
                            ).map(|task| task.id.clone());
                            root.notice = Some("Task queued for a read-only plan.".into());
                        }
                        if let Some((title, goal)) = submitted_draft {
                            // Keep edits made during the request, and keep a
                            // failed submission available for a retry.
                            if value(&root.task_title, cx) == title && value(&root.task_goal, cx) == goal {
                                root.task_title.update(cx, |field, cx| field.clear(cx));
                                root.task_goal.update(cx, |field, cx| field.clear(cx));
                            }
                        }
                        if let Some(r) = submitted_responsibility {
                            if root.responsibility_editing.as_deref().unwrap_or("") == r.id
                                && value(&root.responsibility_instruction, cx) == r.instruction
                                && root.responsibility_connections == r.connection_ids
                                && root.responsibility_prepare == r.prepare_low_risk {
                                root.responsibility_instruction.update(cx, |field, cx| field.clear(cx));
                                root.responsibility_connections.clear();
                                root.responsibility_prepare = false;
                                root.responsibility_editing = None;
                                root.connection_error = None;
                                root.notice = Some("Responsibility saved.".into());
                            }
                        }
                        if root.selection.workspace.as_ref().is_some_and(|id|
                            !root.snapshot.workspaces.iter().any(|workspace| &workspace.id == id)
                        ) { root.selection = Selection::default(); }
                        if let Some(task_id) = root.pending_task.take() {
                            let workspace_id = root.snapshot.tasks.iter().find(|task| task.id == task_id).map(|task| task.workspace_id.clone());
                            if let Some(workspace_id) = workspace_id {
                                root.select_workspace(&workspace_id, cx);
                                root.selection.task = Some(task_id);
                                root.view = evidence_view().unwrap_or(View::Tasks);
                            } else { root.error = Some("That task is no longer available.".into()); }
                        }
                    }
                    Ok(Response::Error { message }) => {
                        if connecting {
                            // A remote error must never echo a credential into
                            // a rendered error or diagnostic. Re-enter to retry.
                            root.connection_error = Some("Connection failed. Check the server configuration and credentials, then retry.".into());
                        } else if is_poll { root.transport_error = Some(message); }
                        else { root.error = Some(message); }
                    }
                    Ok(_) => root.transport_error = Some("The daemon returned an unexpected workspace response.".into()),
                    Err(_) => root.transport_error = Some("Neko cannot reach its daemon. Reconnecting automatically…".into()),
                }
                cx.notify();
                // Native evidence showed a loaded snapshot behind stale
                // pre-connection chrome in a non-key window. Refresh this
                // window explicitly after releasing the entity borrow;
                // entity notification alone did not repaint that case.
                let first_evidence_load = workspace_evidence() && !was_loaded && root.loaded;
                cx.defer(move |cx| {
                    if let Some(window) = cx.try_global::<WorkspaceWindow>().and_then(|slot| slot.0) {
                        let _ = window.update(cx, |_, window, _| {
                            window.refresh();
                            // A non-activating evidence window can be occluded
                            // during startup, suspending its native display link.
                            // Bring it forward once after data loads, never key.
                            if first_evidence_load { show_evidence_window(window); }
                        });
                    }
                });
            });
        }).detach();
        cx.notify();
    }

    fn select_workspace(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(workspace) = self
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.id == id)
            .cloned()
        else {
            return;
        };
        self.selection.select_workspace(workspace.id);
        self.creating_workspace = false;
        self.workspace_name
            .update(cx, |field, cx| field.set_content(&workspace.name, cx));
        self.repository
            .update(cx, |field, cx| field.set_content(&workspace.repository, cx));
        self.instructions.update(cx, |field, cx| {
            field.set_content(&workspace.instructions, cx)
        });
        self.away_enabled = false;
        self.trust_server = false;
        self.responsibility_connections.clear();
        self.responsibility_prepare = false;
        self.responsibility_editing = None;
        for field in [
            &self.api_key,
            &self.server_label,
            &self.server_target,
            &self.server_args,
            &self.oauth_client_id,
            &self.responsibility_instruction,
            &self.task_title,
            &self.task_goal,
        ] {
            field.update(cx, |field, cx| field.clear(cx));
        }
        self.error = None;
        self.connection_error = None;
        self.notice = None;
        cx.notify();
    }

    fn new_workspace(&mut self, cx: &mut Context<Self>) {
        self.view = View::Workspaces;
        self.creating_workspace = true;
        self.selection = Selection::default();
        self.away_enabled = false;
        for field in [
            &self.workspace_name,
            &self.repository,
            &self.instructions,
            &self.api_key,
        ] {
            field.update(cx, |field, cx| field.clear(cx));
        }
        self.error = None;
        self.notice = None;
        cx.notify();
    }

    fn save_workspace(&mut self, cx: &mut Context<Self>) {
        let workspace = Workspace {
            id: if self.creating_workspace {
                String::new()
            } else {
                self.selection.workspace.clone().unwrap_or_default()
            },
            name: value(&self.workspace_name, cx),
            repository: value(&self.repository, cx),
            instructions: value(&self.instructions, cx),
            away_enabled: self.away_enabled,
        };
        if workspace.name.is_empty() || workspace.repository.is_empty() {
            self.error = Some("Enter a workspace name and the path to a Git repository.".into());
            cx.notify();
            return;
        }
        self.request(Command::SaveWorkspace { workspace }, cx);
    }

    fn create_task(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(workspace_id) = self.selection.workspace.clone() else {
            return;
        };
        let title = value(&self.task_title, cx);
        let goal = value(&self.task_goal, cx);
        if title.is_empty() || goal.is_empty() {
            self.error = Some("Give the task a title and a concrete outcome.".into());
            cx.notify();
            return;
        }
        self.request(
            Command::CreateTask {
                workspace_id,
                title,
                goal,
            },
            cx,
        );
    }

    fn connect_mcp(&mut self, cx: &mut Context<Self>) {
        use neko_protocol::mcp_host::McpCommand;
        if self.busy {
            return;
        }
        let Some(workspace_id) = self.selection.workspace.clone() else {
            return;
        };
        let config = match tools::config_from_form(
            self.local_server,
            &value(&self.server_target, cx),
            &value(&self.server_args, cx),
        ) {
            Ok(config) => config,
            Err(error) => {
                self.connection_error = Some(error);
                cx.notify();
                return;
            }
        };
        let secret = value(&self.api_key, cx);
        self.api_key.update(cx, |field, cx| field.clear(cx));
        self.request(
            Command::Mcp(McpCommand::AddConnection {
                workspace_id,
                label: value(&self.server_label, cx),
                config,
                trust_local_process: self.trust_server,
                credentials: (!secret.is_empty()).then_some(Secret(secret)),
            }),
            cx,
        );
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut sidebar = div()
            .w(px(210.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(16.))
            .bg(theme::active().surface_raised)
            .border_r_1()
            .border_color(theme::active().border_hairline)
            .child(
                div()
                    .text_size(px(22.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .mb(px(16.))
                    .child("neko"),
            );
        for (view, label, id) in [
            (View::Tasks, "Tasks & inbox", "nav-tasks"),
            (View::Workspaces, "Workspaces", "nav-workspaces"),
            (View::Integrations, "Tools / MCP", "nav-integrations"),
        ] {
            sidebar = sidebar.child(button(
                id,
                label,
                true,
                self.view == view,
                cx,
                move |root, _, cx| {
                    if root.view == View::Integrations && view != View::Integrations {
                        root.api_key.update(cx, |field, cx| field.clear(cx));
                    }
                    root.view = view;
                    cx.notify();
                },
            ));
        }
        sidebar = sidebar.child(
            div()
                .mt(px(24.))
                .mb(px(4.))
                .text_size(px(11.))
                .text_color(theme::active().text_secondary)
                .child("WORKSPACE"),
        );
        let mut scopes = div()
            .id("workspace-scopes")
            .flex()
            .flex_col()
            .gap(px(6.))
            .max_h(px(280.))
            .overflow_y_scroll();
        for workspace in &self.snapshot.workspaces {
            let id = workspace.id.clone();
            scopes = scopes.child(button(
                format!("scope-{}", id),
                workspace.name.clone(),
                !self.busy,
                self.selection.includes(&id),
                cx,
                move |root, _, cx| root.select_workspace(&id, cx),
            ));
        }
        sidebar
            .child(scopes)
            .when(self.snapshot.workspaces.is_empty(), |sidebar| {
                sidebar.child(note("No workspace yet."))
            })
            .child(button(
                "new-workspace",
                "+ New workspace",
                !self.busy,
                false,
                cx,
                |root, _, cx| root.new_workspace(cx),
            ))
            .child(div().flex_1())
            .child(note(if self.busy && !self.refreshing {
                "Working…"
            } else if !self.loaded {
                "Connecting to daemon…"
            } else {
                "Updates every 2 seconds"
            }))
    }

    fn workspaces_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let creating = self.creating_workspace;
        div().id("workspace-editor").flex().flex_col().gap(px(20.)).size_full().overflow_y_scroll()
            .child(heading(if creating { "Create a workspace" } else { "Workspace settings" }, "Give Neko a repository and instructions for the work it owns."))
            .child(field("Name", &self.workspace_name))
            .child(field("Git repository", &self.repository))
            .child(note("Use a local Git repository path. Tasks build in isolated worktrees."))
            .child(field("Workspace instructions", &self.instructions))
            .child(note("Tools and responsibilities are configured per workspace in Tools / MCP. Manual tasks require your approval before editing. Worker reads are not a cross-workspace filesystem privacy boundary."))
            .child(button("save-workspace", if creating { "Create workspace" } else { "Save workspace" }, !self.busy, true, cx, |root, _, cx| root.save_workspace(cx)))
    }

    fn tasks_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.selection.workspace.is_none() {
            return self
                .choose_workspace(
                    "Select a workspace to see its tasks and source history.",
                    cx,
                )
                .into_any_element();
        }
        let mut tasks = div()
            .id("tasks-and-issues")
            .w(px(300.))
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(10.));
        tasks = tasks.child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Tasks"));
        let scoped_tasks: Vec<_> = self
            .snapshot
            .tasks
            .iter()
            .filter(|task| self.selection.includes(&task.workspace_id))
            .collect();
        if scoped_tasks.is_empty() {
            tasks = tasks.child(note("No tasks yet. Describe the first outcome below."));
        }
        for task in scoped_tasks.into_iter().rev() {
            let id = task.id.clone();
            tasks = tasks.child(button(
                format!("task-{}", id),
                format!("{}  ·  {}", task.title, status_label(task.status)),
                true,
                self.selection.task.as_deref() == Some(&id),
                cx,
                move |root, _, cx| {
                    root.selection.task = Some(id.clone());
                    cx.notify();
                },
            ));
        }
        tasks = tasks.child(
            div()
                .mt(px(16.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("Assigned issues"),
        );
        let scoped_issues: Vec<_> = self
            .snapshot
            .issues
            .iter()
            .filter(|issue| issue.assigned && self.selection.includes(&issue.workspace_id))
            .collect();
        if scoped_issues.is_empty() {
            tasks = tasks.child(note(
                "No historical source items. Add your own MCP servers in Tools / MCP.",
            ));
        }
        for issue in scoped_issues {
            let id = issue.id.clone();
            let existing_task = self
                .snapshot
                .tasks
                .iter()
                .find(|task| {
                    task.issue_id.as_deref() == Some(&id)
                        && self.selection.includes(&task.workspace_id)
                })
                .map(|task| task.id.clone());
            tasks = tasks.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .p(px(12.))
                    .rounded(px(8.))
                    .bg(theme::active().surface_raised)
                    .child(note(issue.identifier.clone()))
                    .child(issue.title.clone())
                    .child(button(
                        format!("plan-{}", id),
                        if existing_task.is_some() {
                            "View task"
                        } else {
                            "Plan issue"
                        },
                        !self.busy,
                        false,
                        cx,
                        move |root, _, cx| {
                            if let Some(task_id) = &existing_task {
                                root.selection.task = Some(task_id.clone());
                                cx.notify();
                                return;
                            }
                            root.request(
                                Command::PlanIssue {
                                    issue_id: id.clone(),
                                },
                                cx,
                            )
                        },
                    )),
            );
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(heading(
                "Tasks & inbox",
                "Plans, approvals, and results for the selected workspace.",
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .p(px(16.))
                    .rounded(px(12.))
                    .bg(theme::active().surface_raised)
                    .child(field("New task", &self.task_title))
                    .child(field("Outcome", &self.task_goal))
                    .child(button(
                        "create-task",
                        "Create task & plan",
                        !self.busy,
                        true,
                        cx,
                        |root, _, cx| root.create_task(cx),
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .gap(px(22.))
                    .child(tasks)
                    .child(self.task_detail(cx)),
            )
            .into_any_element()
    }

    fn task_detail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let task = self.snapshot.tasks.iter().find(|task| {
            self.selection.task.as_deref() == Some(&task.id)
                && self.selection.includes(&task.workspace_id)
        });
        let Some(task) = task else {
            return div()
                .flex_1()
                .p(px(20.))
                .child(note(
                    "Select a task to read its plan, timeline, and result.",
                ))
                .into_any_element();
        };
        let mut detail = div()
            .id(SharedString::from(format!("detail-{}", task.id)))
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(heading(&task.title, status_label(task.status)))
            .child(task.goal.clone());
        let mut controls = div().flex().flex_wrap().gap(px(8.));
        if let Some(decision) = &task.supervision {
            let action = match decision.action {
                neko_protocol::workbench::SupervisorAction::PrepareFix => {
                    "Prepare a fix when the standing permission and assignment are current"
                }
                neko_protocol::workbench::SupervisorAction::AskUser => "Needs your decision",
                neko_protocol::workbench::SupervisorAction::Skip => {
                    "Not recommended for this responsibility"
                }
            };
            detail = detail.child(section_text("Supervisor decision", &format!("{action}\nRisk: {:?}\n{}\n\nEvidence:\n{}\n\nProposed files:\n{}\n\nVerification:\n{}\n\nSensitive areas: {}\nUncertainties: {}", decision.risk, decision.reason, decision.evidence.join("\n"), decision.files.join("\n"), decision.tests.join("\n"), decision.sensitive_areas.join(", "), decision.uncertainties.join(", "))));
        }
        if task.status == TaskStatus::AwaitingApproval {
            let id = task.id.clone();
            controls = controls.child(button(
                "approve-task",
                "Approve local build",
                !self.busy,
                true,
                cx,
                move |root, _, cx| {
                    root.request(
                        Command::ApproveTask {
                            task_id: id.clone(),
                        },
                        cx,
                    )
                },
            ));
        }
        if task.status == TaskStatus::ReadyForReview {
            let id = task.id.clone();
            controls = controls.child(button(
                "complete-task",
                "Mark reviewed",
                !self.busy,
                true,
                cx,
                move |root, _, cx| {
                    root.request(
                        Command::CompleteTask {
                            task_id: id.clone(),
                        },
                        cx,
                    )
                },
            ));
        }
        if matches!(task.status, TaskStatus::Failed | TaskStatus::Cancelled) {
            let id = task.id.clone();
            controls = controls.child(button(
                "retry-task",
                "Retry task",
                !self.busy,
                true,
                cx,
                move |root, _, cx| {
                    root.request(
                        Command::RetryTask {
                            task_id: id.clone(),
                        },
                        cx,
                    )
                },
            ));
        }
        if matches!(
            task.status,
            TaskStatus::Queued
                | TaskStatus::Planning
                | TaskStatus::AwaitingApproval
                | TaskStatus::Building
                | TaskStatus::Reviewing
        ) {
            let id = task.id.clone();
            controls = controls.child(button(
                "cancel-task",
                "Cancel task",
                !self.busy,
                false,
                cx,
                move |root, _, cx| {
                    root.request(
                        Command::CancelTask {
                            task_id: id.clone(),
                        },
                        cx,
                    )
                },
            ));
        }
        detail = detail
            .child(controls)
            .when(task.status == TaskStatus::ReadyForReview, |detail| detail.child(note("Mark reviewed completes this task. It does not merge or push changes; the local worktree is preserved.")))
            .when(matches!(task.status, TaskStatus::Failed | TaskStatus::Cancelled), |detail| detail.child(note(
                if task.worktree.is_some() { "Retry runs a new read-only plan using the existing preserved worktree." }
                else { "Retry starts with a new read-only plan. A worktree will be created when needed." }
            )))
            .child(section_text(
                "Plan",
                if task.plan.is_empty() {
                    "The planner has not returned a plan yet."
                } else {
                    &task.plan
                },
            ))
            .child(section_text(
                "Result",
                if task.result.is_empty() {
                    "No result yet. Progress appears in the timeline below."
                } else {
                    &task.result
                },
            ));
        if let Some(path) = &task.worktree {
            let reveal_path = std::path::PathBuf::from(path);
            detail = detail
                .child(
                    div()
                        .min_w(px(0.))
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child("Worktree"),
                        )
                        .child(
                            div()
                                .min_w(px(0.))
                                .w_full()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis_middle()
                                .child(path.clone()),
                        ),
                )
                .child(button(
                    "reveal-worktree",
                    "Show worktree in Finder",
                    true,
                    false,
                    cx,
                    move |_, _, cx| cx.reveal_path(&reveal_path),
                ));
        }
        detail = detail.child(
            div()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("Timeline"),
        );
        if task.events.is_empty() {
            detail = detail.child(note("Waiting for the first task event."));
        }
        for event in &task.events {
            detail = detail.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .border_l_2()
                    .border_color(theme::active().border_hairline_strong)
                    .pl(px(12.))
                    .child(note(format!(
                        "{} · {}",
                        event.role,
                        relative_time(event.at_ms)
                    )))
                    .child(event.message.clone()),
            );
        }
        detail.into_any_element()
    }

    fn integrations_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        tools::view(self, cx)
    }

    fn choose_workspace(&self, message: &str, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .p(px(24.))
            .child(heading("Choose a workspace", message))
            .child(button(
                "empty-new-workspace",
                "Create a workspace",
                !self.busy,
                true,
                cx,
                |root, _, cx| root.new_workspace(cx),
            ))
    }
}

impl Render for WorkspaceRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let appearance = theme::active_theme().appearance;
        if self.appearance != Some(appearance) {
            let _ = crate::material::set_window_appearance(window, appearance);
            self.appearance = Some(appearance);
        }
        let body = match self.view {
            View::Tasks => self.tasks_view(cx).into_any_element(),
            View::Workspaces => self.workspaces_view(cx).into_any_element(),
            View::Integrations => self.integrations_view(cx).into_any_element(),
        };
        div()
            .id("neko-workspace")
            .tab_group()
            .size_full()
            .flex()
            .text_size(px(13.))
            .text_color(theme::active().text_primary)
            .bg(theme::active().surface_panel)
            .on_key_down(|event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "tab" {
                    if event.keystroke.modifiers.shift {
                        window.focus_prev(cx);
                    } else {
                        window.focus_next(cx);
                    }
                    cx.stop_propagation();
                }
            })
            .child(self.sidebar(cx))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .p(px(28.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .when_some(
                        self.transport_error.clone().or_else(|| self.error.clone()),
                        |body, error| {
                            body.child(
                                div()
                                    .rounded(px(8.))
                                    .p(px(12.))
                                    .bg(theme::active().banner_danger_bg)
                                    .text_color(theme::active().state_danger)
                                    .child(error),
                            )
                        },
                    )
                    .when_some(self.notice.clone(), |body, notice| body.child(note(notice)))
                    .child(div().flex_1().min_h(px(0.)).child(body)),
            )
    }
}

fn input(placeholder: &'static str, cx: &mut App) -> Entity<TextField> {
    let input = TextField::new(cx);
    input.update(cx, |field, cx| field.set_placeholder(placeholder, cx));
    input
}

fn value(field: &Entity<TextField>, cx: &App) -> String {
    field.read(cx).content().trim().to_string()
}

fn field(label: &'static str, input: &Entity<TextField>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(7.))
        .child(note(label))
        .child(
            div()
                .h(px(38.))
                .flex()
                .items_center()
                .px(px(12.))
                .rounded(px(8.))
                .bg(theme::active().surface_input)
                .border_1()
                .border_color(theme::active().border_hairline_strong)
                .overflow_hidden()
                .child(input.clone()),
        )
}

fn note(text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_size(px(12.))
        .text_color(theme::active().text_secondary)
        .child(text.into())
}

fn heading(title: &str, subtitle: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .text_size(px(23.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(title.to_owned()),
        )
        .child(note(subtitle.to_owned()))
}

fn section_text(title: &'static str, text: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(title))
        .child(text.to_owned())
}

fn button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    enabled: bool,
    selected: bool,
    cx: &mut Context<WorkspaceRoot>,
    action: impl Fn(&mut WorkspaceRoot, &mut Window, &mut Context<WorkspaceRoot>) + 'static,
) -> impl IntoElement {
    let action = Rc::new(action);
    let keyboard_action = action.clone();
    div()
        .id(id.into())
        .tab_index(0)
        .tab_stop(enabled)
        .px(px(12.))
        .py(px(9.))
        .rounded(px(8.))
        .bg(if selected {
            theme::active().surface_selected
        } else {
            theme::active().surface_input
        })
        .text_color(if enabled {
            theme::active().text_primary
        } else {
            theme::active().text_secondary
        })
        .border_1()
        .border_color(theme::active().border_hairline)
        .focus_visible(|style| style.border_color(theme::active().text_primary))
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(|style| style.bg(theme::active().surface_selected))
                .on_click(cx.listener(move |root, _, window, cx| action(root, window, cx)))
                .on_key_down(cx.listener(move |root, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                        keyboard_action(root, window, cx);
                        cx.stop_propagation();
                    }
                }))
        })
        .child(label.into())
}

fn status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "Queued",
        TaskStatus::Planning => "Planning",
        TaskStatus::AwaitingApproval => "Needs approval",
        TaskStatus::Building => "Building",
        TaskStatus::Reviewing => "Reviewing",
        TaskStatus::ReadyForReview => "Ready for review",
        TaskStatus::Completed => "Completed",
        TaskStatus::Failed => "Failed",
        TaskStatus::Cancelled => "Cancelled",
    }
}

fn scope_label(ids: &[String]) -> String {
    if ids.is_empty() {
        "all visible".into()
    } else {
        ids.join(", ")
    }
}

fn relative_time(at_ms: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let seconds = now.saturating_sub(at_ms).max(0) / 1000;
    if seconds < 60 {
        "just now".into()
    } else if seconds < 3600 {
        format!("{} min ago", seconds / 60)
    } else if seconds < 86400 {
        format!("{} h ago", seconds / 3600)
    } else {
        format!("{} days ago", seconds / 86400)
    }
}

#[derive(Default)]
struct Selection {
    workspace: Option<String>,
    task: Option<String>,
}

impl Selection {
    fn select_workspace(&mut self, id: String) {
        self.workspace = Some(id);
        self.task = None;
    }

    fn includes(&self, workspace_id: &str) -> bool {
        self.workspace.as_deref() == Some(workspace_id)
    }
}

fn parse_ids(value: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for id in value.split(',').map(str::trim).filter(|id| !id.is_empty()) {
        if !ids.iter().any(|existing| existing == id) {
            ids.push(id.to_string());
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_workspace_clears_the_previous_task() {
        let mut selection = Selection {
            workspace: Some("one".into()),
            task: Some("private-task".into()),
        };
        selection.select_workspace("two".into());
        assert!(selection.task.is_none());
        assert!(!selection.includes("one"));
        assert!(selection.includes("two"));
    }

    #[test]
    fn no_selection_never_means_all_workspaces() {
        let selection = Selection::default();
        assert!(!selection.includes("one"));
        assert!(!selection.includes("two"));
    }

    #[test]
    fn scope_filters_trim_empty_and_duplicate_ids() {
        assert_eq!(parse_ids(" a, ,b,a,  b , c "), vec!["a", "b", "c"]);
        assert!(parse_ids(" , ").is_empty());
    }
}
