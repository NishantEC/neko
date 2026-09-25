//! The persistent native workspace, separate from the quick launcher.
mod home;
mod tools;
mod skills;
mod schedules;
mod profiles;

use home::TicketFilter;
use crate::{text_field::TextField, theme};
use gpui::{
    App, Context, Entity, Global, IntoElement, KeyDownEvent, ParentElement, Render, SharedString,
    Styled, TitlebarOptions, Window, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, div, point, prelude::*, px,
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
    let bounds = gpui::Bounds::centered(None, gpui::size(px(1240.), px(800.)), cx);
    match cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(gpui::size(px(980.), px(620.))),
            titlebar: Some(TitlebarOptions {
                title: Some("Neko".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(16.), px(16.))),
            }),
            kind: WindowKind::Normal,
            is_resizable: true,
            is_minimizable: true,
            is_movable: true,
            focus: !evidence,
            show: true,
            window_background: crate::material::window_background(),
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
                // The same native glass the quick panel sits on. Without it the
                // window stays opaque and the palette's solid panel colour is used.
                root.translucent = crate::material::install(window).is_ok();
                eprintln!("neko: workspace translucent={}", root.translucent);
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
        Ok("tickets") => Some(View::Tickets),
        Ok("responsibilities") => Some(View::Responsibilities),
        Ok("memory") => Some(View::Memory),
        Ok("profiles") => Some(View::Profiles),
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
            root.view = View::Tickets;
            root.request(Command::Snapshot, cx);
            cx.notify();
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Today,
    Tickets,
    Responsibilities,
    Memory,
    Workspaces,
    Integrations,
    Profiles,
}

pub struct WorkspaceRoot {
    profile_form: profiles::Form,
    memory_editing: Option<neko_protocol::workbench::MemoryEntry>,
    schedule_form: schedules::Form,
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
    workspace_advanced: bool,
    task_title: Entity<TextField>,
    task_goal: Entity<TextField>,
    api_key: Entity<TextField>,
    server_label: Entity<TextField>,
    skill_source: Entity<TextField>,
    skill_filter: Entity<TextField>,
    tools_tab: tools::Tab,
    browse_skills: bool,
    skill_limit: usize,
    opened_skill_audits: Vec<(String, String)>,
    server_target: Entity<TextField>,
    server_args: Entity<TextField>,
    oauth_client_id: Entity<TextField>,
    responsibility_instruction: Entity<TextField>,
    responsibility_connections: Vec<String>,
    responsibility_prepare: bool,
    responsibility_editing: Option<String>,
    local_server: bool,
    global_server: bool,
    trust_server: bool,
    appearance: Option<theme::Appearance>,
    composer: Entity<TextField>,
    note_input: Entity<TextField>,
    memory_input: Entity<TextField>,
    ticket_filter: TicketFilter,
    translucent: bool,
    drag_armed: bool,
    /// User actions that arrived while a background poll was in flight, sent
    /// in order. A failure drops the rest so its error stays visible.
    queued: std::collections::VecDeque<Command>,
}

impl WorkspaceRoot {
    fn new(client: NekoClient, cx: &mut Context<Self>) -> Self {
        let api_key = input("Optional credential JSON (stored in Keychain)", cx);
        let skill_filter = input("Find a skill by name, description or source", cx);
        let initial_snapshot_client = client.clone();
        cx.observe(&skill_filter, |_, _, cx| cx.notify()).detach();
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
        // Search is naturally retried by the next keystroke, but a persistent
        // workspace has to load useful content even when nobody has typed.
        // The daemon can still be indexing applications on a first launch, so
        // wait off the UI executor for its first socket before retrying once.
        cx.spawn(async move |this, cx| {
            let connected = cx
                .background_executor()
                .spawn(async move {
                    initial_snapshot_client.wait_for_connection(Duration::from_secs(5))
                })
                .await;
            if connected {
                let _ = this.update(cx, |root, cx| {
                    if !root.loaded && !root.busy {
                        root.request(Command::Snapshot, cx);
                    }
                });
            }
        })
        .detach();
        Self {
            schedule_form: schedules::Form::new(cx),
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
            view: evidence_view().unwrap_or(View::Today),
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
            workspace_advanced: false,
            task_title: input("What needs doing?", cx),
            task_goal: input("Describe the outcome and how to verify it", cx),
            api_key,
            server_label: input("Name this connection", cx),
            skill_source: input("https://github.com/owner/repo/blob/commit/path/SKILL.md", cx),
            skill_filter,
            profile_form: profiles::Form::new(cx),
            memory_editing: None,
            tools_tab: tools::Tab::Connections,
            browse_skills: false,
            skill_limit: 20,
            opened_skill_audits: Vec::new(),
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
            global_server: false,
            trust_server: false,
            appearance: None,
            composer: input("Ask Neko anything, or tell it what to look after…", cx),
            note_input: input("Steer this ticket…", cx),
            memory_input: input("Something Neko should know, like \"we use pytest in hme\"", cx),
            ticket_filter: TicketFilter::NeedsYou,
            translucent: false,
            drag_armed: false,
            queued: std::collections::VecDeque::new(),
        }
    }

    fn request(&mut self, command: Command, cx: &mut Context<Self>) {
        if self.busy {
            // Only a background refresh is worth waiting behind; another user
            // action in flight already disables the controls.
            if self.refreshing && !matches!(command, Command::Snapshot) && self.queued.len() < 8 {
                self.queued.push_back(command);
            }
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
            // Scheduled planning is separately recorded by its schedule.
            &command,
            Command::CreateTask { .. } | Command::PlanIssue { .. }
        );
        let submitted_schedule=match &command {Command::Schedules(neko_protocol::scheduled_plans::ScheduleCommand::Save{schedule})=>Some(schedule.clone()),_=>None};
        let previous_schedules=self.snapshot.schedules.iter().map(|s|s.id.clone()).collect::<Vec<_>>();
        let submitted_profile=match &command {Command::AgentProfiles(neko_protocol::agent_profiles::ProfileCommand::Save{profile})=>Some(profile.clone()),_=>None};
        let previous_profiles=self.snapshot.agent_profiles.profiles.iter().map(|p|p.id.clone()).collect::<Vec<_>>();
        let submitted_draft = match &command {
            Command::CreateTask { title, goal, .. } => Some((title.clone(), goal.clone())),
            _ => None,
        };
        let submitted_message = match &command {
            Command::SendMessage { text, .. } => Some(text.clone()),
            _ => None,
        };
        let submitted_note = match &command {
            Command::AddTicketNote { text, .. } => Some(text.clone()),
            _ => None,
        };
        let submitted_memory = match &command {
            Command::SaveMemory { entry } => Some(entry.text.clone()),
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
                        if let Some(schedule)=submitted_schedule {
                            root.schedule_form.saved(&schedule,&root.snapshot,&previous_schedules,cx);
                            root.notice=Some("Schedule saved paused. Review it, then enable when ready.".into());
                        }
                        if let Some(profile)=submitted_profile {
                            root.profile_form.saved(&profile,&root.snapshot,&previous_profiles);
                            root.notice=Some("Agent profile saved.".into());
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
                        // Clear what was sent, unless the user kept typing.
                        if let Some(text) = submitted_message {
                            if value(&root.composer, cx) == text { root.composer.update(cx, |field, cx| field.clear(cx)); }
                        }
                        if let Some(text) = submitted_note {
                            if value(&root.note_input, cx) == text { root.note_input.update(cx, |field, cx| field.clear(cx)); }
                        }
                        if let Some(text) = submitted_memory {
                            if value(&root.memory_input, cx) == text { root.memory_input.update(cx, |field, cx| field.clear(cx)); root.memory_editing=None; }
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
                                root.view = evidence_view().unwrap_or(View::Tickets);
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
                let failed = root.error.is_some() || root.connection_error.is_some() || root.transport_error.is_some();
                if failed && !is_poll {
                    root.queued.clear();
                } else if let Some(next) = root.queued.pop_front() {
                    root.request(next, cx);
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
        self.workspace_advanced = false;
        self.workspace_name
            .update(cx, |field, cx| field.set_content(&workspace.name, cx));
        self.repository
            .update(cx, |field, cx| field.set_content(&workspace.repository, cx));
        self.instructions.update(cx, |field, cx| {
            field.set_content(&workspace.instructions, cx)
        });
        self.away_enabled = false;
        self.trust_server = false;
        self.global_server = false;
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
        self.workspace_advanced = false;
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
        let mut workspace = match workspace_from_draft(
            &value(&self.workspace_name, cx),
            &value(&self.repository, cx),
            &value(&self.instructions, cx),
            self.creating_workspace,
            self.selection.workspace.as_deref(),
        ) {
            Ok(workspace) => workspace,
            Err(message) => {
                self.error = Some(message.into());
                cx.notify();
                return;
            }
        };
        workspace.away_enabled = self.away_enabled;
        if workspace.name.is_empty() {
            self.error = Some("Give this workspace a name.".into());
            cx.notify();
            return;
        }
        self.request(Command::SaveWorkspace { workspace }, cx);
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
                workspace_id: if self.global_server { String::new() } else { workspace_id },
                label: value(&self.server_label, cx),
                config,
                trust_local_process: self.trust_server,
                credentials: (!secret.is_empty()).then_some(Secret(secret)),
            }),
            cx,
        );
    }

    fn workspaces_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let creating = self.creating_workspace;
        let t = theme::active();
        let mut form = div()
            .w_full()
            .max_w(px(if creating { 600. } else { 680. }))
            .flex()
            .flex_col()
            .gap(px(16.));

        if creating {
            form = form
                .child(heading("Add a workspace", "Start with where the work lives. You can refine Neko's instructions after it is added."))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(14.))
                        .p(px(18.))
                        .rounded(px(14.))
                        .bg(alpha(t.surface_raised, 0.76))
                        .border_1()
                        .border_color(t.border_hairline)
                        .child(field("Workspace name", &self.workspace_name))
                        .child(field("Local repository folder", &self.repository))
                        .child(note("Paste the folder path. Neko uses isolated worktrees, so your checkout stays untouched."))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(12.))
                                .child(button("workspace-advanced", if self.workspace_advanced { "Hide instructions" } else { "Add instructions" }, !self.busy, false, cx, |root, _, cx| {
                                    root.workspace_advanced = !root.workspace_advanced;
                                    cx.notify();
                                }))
                                .child(button("save-workspace", "Add workspace", !self.busy, true, cx, |root, _, cx| root.save_workspace(cx))),
                        )
                        .when(self.workspace_advanced, |card| {
                            card.child(field("What Neko should know", &self.instructions))
                                .child(note("Optional. Keep it short: conventions, checks, or things Neko should avoid."))
                        }),
                );
        } else {
            form = form
                .child(heading("Workspace settings", "Update where this workspace lives or the standing guidance Neko uses."))
                .child(field("Name", &self.workspace_name))
                .child(field("Local repository folder", &self.repository))
                .child(field("Workspace instructions", &self.instructions))
                .child(note("Tools and responsibilities are scoped separately. Editing work still waits for your approval."))
                .child(div().flex().justify_end().child(button("save-workspace", "Save changes", !self.busy, true, cx, |root, _, cx| root.save_workspace(cx))));
        }

        div()
            .id("workspace-editor")
            .size_full()
            .overflow_y_scroll()
            .px(px(32.))
            .py(px(28.))
            .flex()
            .justify_center()
            .child(crate::motion::fade_in("workspace-editor-reveal", crate::motion::system_reduce_motion(), form))
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
            View::Today => self.today_view(cx),
            View::Tickets => self.tickets_page(cx),
            View::Responsibilities => self.responsibilities_page(cx),
            View::Memory => self.memory_page(cx),
            View::Profiles => {
                let content=profiles::view(self,cx);
                self.settings_page("Agent profiles",content,cx)
            }
            View::Workspaces => {
                let content = self.workspaces_view(cx).into_any_element();
                self.settings_page("Workspace", content, cx)
            }
            View::Integrations => {
                let content = self.integrations_view(cx).into_any_element();
                self.settings_page("Tools & skills", content, cx)
            }
        };
        let background = if self.translucent {
            theme::active().surface_panel_translucent
        } else {
            theme::active().surface_panel
        };
        div()
            .id("neko-workspace")
            .tab_group()
            .size_full()
            .flex()
            .text_size(px(13.))
            .text_color(theme::active().text_primary)
            .bg(background)
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
            .child(self.home_sidebar(cx))
            .child(div().flex_1().min_w(px(0.)).h_full().child(body))
    }
}

impl WorkspaceRoot {
    /// The existing workspace and tool editors, inside the new chrome.
    fn settings_page(&self, title: &str, content: gpui::AnyElement, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.page_header(title, None, cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
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
                    .child(div().flex_1().min_h(px(0.)).child(content)),
            )
            .into_any_element()
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

fn alpha(mut color: gpui::Rgba, a: f32) -> gpui::Rgba {
    color.a = a;
    color
}

/// The compact create flow has one non-negotiable value: a real local folder.
/// A name is optional because the last path component is already the most
/// useful default and asking twice makes the first step feel like a settings
/// form. This stays pure so the UI and its tests cannot drift apart.
fn workspace_from_draft(
    name: &str,
    repository: &str,
    instructions: &str,
    creating: bool,
    existing_id: Option<&str>,
) -> Result<Workspace, &'static str> {
    let repository = repository.trim().trim_end_matches('/');
    if repository.is_empty() {
        return Err("Choose the local repository folder for this workspace.");
    }
    let name = if name.trim().is_empty() {
        std::path::Path::new(repository)
            .file_name()
            .and_then(|part| part.to_str())
            .filter(|part| !part.is_empty())
            .ok_or("Give this workspace a name.")?
            .to_owned()
    } else {
        name.trim().to_owned()
    };
    Ok(Workspace {
        id: if creating {
            String::new()
        } else {
            existing_id.unwrap_or_default().to_owned()
        },
        name,
        repository: repository.to_owned(),
        instructions: instructions.trim().to_owned(),
        away_enabled: false,
    })
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
    fn compact_workspace_draft_uses_the_folder_name_when_name_is_blank() {
        let workspace = workspace_from_draft("", "/Users/nish/Projects/triage-fe/", "", true, None)
            .expect("a folder is enough to create a compact workspace");
        assert_eq!(workspace.name, "triage-fe");
        assert_eq!(workspace.repository, "/Users/nish/Projects/triage-fe");
    }

    #[test]
    fn compact_workspace_draft_keeps_an_explicit_name() {
        let workspace = workspace_from_draft("CareConnect", "/Users/nish/Projects/triage-fe", "", true, None)
            .expect("an explicit name remains valid");
        assert_eq!(workspace.name, "CareConnect");
    }

    #[test]
    fn compact_workspace_draft_explains_when_the_folder_is_missing() {
        assert_eq!(workspace_from_draft("", "", "", true, None).unwrap_err(), "Choose the local repository folder for this workspace.");
    }
}
