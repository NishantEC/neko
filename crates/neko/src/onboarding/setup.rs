//! Three visible setup steps; permission asks remain substates, not extra screens.
use super::*;
use crate::text_field::TextField;
use neko_protocol::{
    mcp_host::{McpCommand, Responsibility},
    setup_import::{ImportCandidateKind, ImportCommand},
    workbench::{ChatRole, Command, Snapshot, Workspace},
};
#[cfg(test)]
use std::collections::BTreeMap;
use std::collections::HashSet;

pub(super) fn evidence() -> bool {
    std::env::var_os("NEKO_SHOW_SETUP").is_some()
}
fn evidence_step(value: Option<&str>) -> SetupStep {
    value
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|i| (1..=3).contains(i))
        .map(|i| SetupStep::ALL[i - 1])
        .unwrap_or(SetupStep::Welcome)
}

pub(super) struct Setup {
    pub step: SetupStep,
    pub busy: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    clipboard_ready: bool,
    snapshot: Snapshot,
    pub(super) workspace: Option<String>,
    repository: Entity<TextField>,
    responsibility: Entity<TextField>,
    server_name: Entity<TextField>,
    server_target: Entity<TextField>,
    server_args: Entity<TextField>,
    server_local: bool,
    server_global: bool,
    server_trust: bool,
    selected_connections: HashSet<String>,
    global_connection_ids: HashSet<String>,
    selected_repositories: HashSet<String>,
    selected_skills: HashSet<String>,
    selected_workspaces: HashSet<String>,
    selected_schedules: HashSet<String>,
    import_view: ImportView,
    import_source_name: String,
    show_new_workspace: bool,
    show_other_folders: bool,
    import_discovery_started: bool,
    advance_after_import: bool,
    credentials: bool,
    trust: bool,
    brief_turn: Option<String>,
}
impl Setup {
    fn item_selected(&self, item: &neko_protocol::setup_import::ImportCandidate) -> bool {
        match item.kind {
            ImportCandidateKind::Connection => self.selected_connections.contains(&item.id),
            ImportCandidateKind::Skill => self.selected_skills.contains(&item.id),
            ImportCandidateKind::Workspace => self.selected_workspaces.contains(&item.id),
            ImportCandidateKind::Schedule => self.selected_schedules.contains(&item.id),
        }
    }
    fn reset_import_consent(&mut self) {
        self.credentials = false;
        self.trust = false;
    }
    pub fn new(cx: &mut Context<OnboardingRoot>) -> Self {
        fn input(placeholder: &str, cx: &mut App) -> Entity<TextField> {
            let field = TextField::new(cx);
            field.update(cx, |f, cx| f.set_placeholder(placeholder.to_owned(), cx));
            field
        }
        let server_target = input("https://server.example/mcp or /absolute/executable", cx);
        let server_args = input("[\"/path/to/server.js\"]", cx);
        for field in [&server_target, &server_args] {
            cx.subscribe(
                field,
                |root, _, _: &crate::text_field::ContentChanged, cx| {
                    root.setup.server_trust = false;
                    cx.notify();
                },
            )
            .detach();
        }
        Self {
            step: if evidence() {
                evidence_step(std::env::var("NEKO_SETUP_STEP").ok().as_deref())
            } else {
                SetupStep::Welcome
            },
            busy: false,
            clipboard_ready: false,
            error: None,
            notice: None,
            snapshot: Snapshot::default(),
            workspace: None,
            repository: input("/Users/you/Projects/folder", cx),
            responsibility: input("What should Neko keep an eye on?", cx),
            server_name: input("Connection name", cx),
            server_target,
            server_args,
            server_local: false,
            server_global: false,
            server_trust: false,
            selected_connections: HashSet::new(),
            global_connection_ids: HashSet::new(),
            selected_repositories: HashSet::new(),
            selected_skills: HashSet::new(),
            selected_workspaces: HashSet::new(),
            selected_schedules: HashSet::new(),
            import_view: ImportView::Sources,
            import_source_name: String::new(),
            show_new_workspace: false,
            show_other_folders: false,
            import_discovery_started: false,
            advance_after_import: false,
            credentials: false,
            trust: false,
            brief_turn: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImportCategory {
    Skills,
    Connections,
    Workspaces,
    Schedules,
}
impl ImportCategory {
    const ALL: [Self; 4] = [
        Self::Skills,
        Self::Connections,
        Self::Workspaces,
        Self::Schedules,
    ];
    fn title(self) -> &'static str {
        match self {
            Self::Skills => "Skills",
            Self::Connections => "MCP connections",
            Self::Workspaces => "Workspaces",
            Self::Schedules => "Schedules",
        }
    }
    fn matches(self, kind: &ImportCandidateKind) -> bool {
        matches!(
            (self, kind),
            (Self::Skills, ImportCandidateKind::Skill)
                | (Self::Connections, ImportCandidateKind::Connection)
                | (Self::Workspaces, ImportCandidateKind::Workspace)
                | (Self::Schedules, ImportCandidateKind::Schedule)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImportView {
    Sources,
    Scanning,
    Overview,
    Items(ImportCategory),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SetupStep {
    Welcome,
    Mac,
    Import,
    Tools,
    Responsibility,
    Sweep,
}
impl SetupStep {
    pub(super) const ALL: [Self; 3] = [Self::Welcome, Self::Mac, Self::Import];
    pub(super) fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap()
    }
    pub(super) fn next(self) -> Self {
        Self::ALL[(self.index() + 1).min(Self::ALL.len() - 1)]
    }
    pub(super) fn back(self) -> Self {
        Self::ALL[self.index().saturating_sub(1)]
    }
}

fn brief_in_scope(actual: Option<&str>, selected: Option<&str>) -> bool {
    selected.is_some() && actual == selected
}

fn begin_clipboard_change(busy: &mut bool, ready: bool) -> bool {
    if *busy || !ready {
        return false;
    }
    *busy = true;
    true
}

fn should_start_import_discovery(step: SetupStep, busy: bool, started: bool) -> bool {
    step == SetupStep::Import && !busy && !started
}

fn begin_import_discovery(busy: bool, started: &mut bool, force: bool) -> bool {
    if busy {
        return false;
    }
    if *started && !force {
        return false;
    }
    *started = true;
    true
}

fn candidate_is_unavailable(
    kind: &neko_protocol::setup_import::ImportCandidateKind,
    problem: Option<&str>,
    workspace: Option<&str>,
    workspace_selected: bool,
    import_globally: bool,
) -> bool {
    if matches!(
        kind,
        neko_protocol::setup_import::ImportCandidateKind::Schedule
    ) {
        return false;
    }
    problem.is_some()
        || (workspace.is_some()
            && !workspace_selected
            && match kind {
                neko_protocol::setup_import::ImportCandidateKind::Connection => !import_globally,
                neko_protocol::setup_import::ImportCandidateKind::Skill => true,
                _ => false,
            })
}

fn candidate_review_badge(
    already_in_neko: bool,
    has_problem: bool,
    needs_workspace: bool,
    selected: bool,
    different_setup: bool,
) -> &'static str {
    if already_in_neko {
        "Already in Neko"
    } else if has_problem {
        "Review setup"
    } else if needs_workspace {
        "Add workspace first"
    } else if selected {
        "Selected"
    } else if different_setup {
        "Different setup"
    } else {
        "Add"
    }
}

fn workspace_review_rows(
    candidates: &[neko_protocol::setup_import::ImportCandidate],
    show_other: bool,
) -> Vec<&neko_protocol::setup_import::ImportCandidate> {
    let mut rows = candidates
        .iter()
        .filter(|item| {
            item.kind == ImportCandidateKind::Workspace
                && item.metadata.get("review_group").map(String::as_str) != Some("other")
        })
        .collect::<Vec<_>>();
    if show_other {
        rows.extend(candidates.iter().filter(|item| {
            item.kind == ImportCandidateKind::Workspace
                && item.metadata.get("review_group").map(String::as_str) == Some("other")
        }));
    }
    rows
}

fn import_candidate_label(kind: &neko_protocol::setup_import::ImportCandidateKind) -> &'static str {
    match kind {
        neko_protocol::setup_import::ImportCandidateKind::Connection => "MCP server",
        neko_protocol::setup_import::ImportCandidateKind::Skill => "Skill",
        neko_protocol::setup_import::ImportCandidateKind::Workspace => "Workspace",
        neko_protocol::setup_import::ImportCandidateKind::Schedule => "Schedule",
    }
}

#[cfg(test)]
fn import_source_groups<'a>(
    candidates: &'a [neko_protocol::setup_import::ImportCandidate],
) -> BTreeMap<String, Vec<&'a neko_protocol::setup_import::ImportCandidate>> {
    let mut groups: BTreeMap<String, Vec<_>> = BTreeMap::new();
    for candidate in candidates {
        groups
            .entry(candidate.source.clone())
            .or_default()
            .push(candidate);
    }
    for entries in groups.values_mut() {
        entries.sort_by(|left, right| {
            left.workspace
                .is_some()
                .cmp(&right.workspace.is_some())
                .then_with(|| left.workspace.cmp(&right.workspace))
                .then_with(|| {
                    import_candidate_label(&left.kind).cmp(import_candidate_label(&right.kind))
                })
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                .then_with(|| left.id.cmp(&right.id))
        });
    }
    groups
}

fn workspace_scope_selected(
    candidate_workspace: Option<&str>,
    selected_candidate: bool,
    existing_workspace: bool,
) -> bool {
    candidate_workspace.is_none() || selected_candidate || existing_workspace
}

fn uses_legacy_import_rows(candidate_count: usize) -> bool {
    candidate_count == 0
}

fn reset_import_discovery_after_failure(discover: bool, started: &mut bool) {
    if discover {
        *started = false;
    }
}

fn candidate_has_credentials(metadata: &std::collections::BTreeMap<String, String>) -> bool {
    metadata
        .get("has_credentials")
        .is_some_and(|value| value == "true")
}

#[derive(Default)]
struct ImportSelection {
    connections: HashSet<String>,
    skills: HashSet<String>,
    workspaces: HashSet<String>,
    schedules: HashSet<String>,
}

/// Import is opt-in at the individual item level.
fn default_import_selection(
    _preview: &neko_protocol::setup_import::ImportPreview,
) -> ImportSelection {
    ImportSelection::default()
}

fn recording_after_navigation(step: SetupStep, recording: RecordingState) -> RecordingState {
    if step == SetupStep::Mac {
        recording
    } else {
        RecordingState::Idle
    }
}

enum FolderChoice {
    Existing(String),
    New(Workspace),
}

fn folder_choice(path: &std::path::Path, workspaces: &[Workspace]) -> Result<FolderChoice, String> {
    let directory = path
        .canonicalize()
        .map_err(|_| "That folder is no longer available".to_owned())?;
    if !directory.is_dir() {
        return Err("Choose a folder, not a file".into());
    }
    let repository = directory
        .to_str()
        .ok_or("This folder path cannot be read")?
        .to_owned();
    if let Some(existing) = workspaces.iter().find(|w| w.repository == repository) {
        return Ok(FolderChoice::Existing(existing.id.clone()));
    }
    let name = directory
        .file_name()
        .and_then(|part| part.to_str())
        .filter(|name| !name.is_empty())
        .ok_or("Choose a project folder, not a filesystem root")?
        .to_owned();
    Ok(FolderChoice::New(Workspace {
        id: String::new(),
        name,
        repository,
        instructions: String::new(),
        away_enabled: false,
    }))
}

impl OnboardingRoot {
    fn setup_select_folder(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        match folder_choice(&path, &self.setup.snapshot.workspaces) {
            Ok(FolderChoice::Existing(id)) => {
                self.setup.workspace = Some(id);
                self.setup.error = None;
                self.setup.notice = Some("Workspace ready. Open Neko to start there.".into());
                cx.notify();
            }
            Ok(FolderChoice::New(workspace)) => {
                self.setup_request(Command::SaveWorkspace { workspace }, cx);
            }
            Err(error) => {
                self.setup.error = Some(error);
                cx.notify();
            }
        }
    }

    fn setup_choose_folder(&mut self, cx: &mut Context<Self>) {
        if self.setup.busy || evidence() {
            return;
        }
        self.setup.busy = true;
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose workspace".into()),
        });
        cx.spawn(async move |this, cx| {
            let result = receiver.await;
            let _ = this.update(cx, |root, cx| {
                root.setup.busy = false;
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            root.setup_select_folder(path, cx);
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => root.setup.error = Some(error.to_string()),
                    Err(_) => root.setup.error = Some("Folder chooser closed unexpectedly".into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn setup_workspace_view(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut body = div().flex().flex_col().gap(px(20.));
        if !self.setup.snapshot.workspaces.is_empty() {
            body = body.child(
                div()
                    .flex()
                    .justify_between()
                    .child("Your workspaces")
                    .child(note(format!(
                        "{} available",
                        self.setup.snapshot.workspaces.len()
                    ))),
            );
            let mut cards = div().flex().flex_wrap().gap(px(14.));
            for workspace in self.setup.snapshot.workspaces.iter().take(6) {
                let id = workspace.id.clone();
                let selected = self.setup.workspace.as_ref() == Some(&id);
                cards = cards.child(
                    div()
                        .id(format!("setup-folder-{id}"))
                        .tab_index(0)
                        .tab_stop(!self.setup.busy)
                        .w(px(300.))
                        .h(px(180.))
                        .p(px(20.))
                        .rounded(px(12.))
                        .border_1()
                        .border_color(if selected {
                            theme::active().text_primary
                        } else {
                            theme::active().border_hairline_strong
                        })
                        .bg(theme::active().surface_panel)
                        .cursor_pointer()
                        .hover(|style| style.bg(theme::active().surface_raised))
                        .on_click(cx.listener(move |root, _, _, cx| {
                            root.setup.workspace = Some(id.clone());
                            root.setup.error = None;
                            cx.notify();
                        }))
                        .flex()
                        .flex_col()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .text_size(px(18.))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(workspace.name.clone()),
                                )
                                .child(note(workspace.repository.clone())),
                        )
                        .child(note(if selected {
                            "Selected"
                        } else {
                            "Select workspace →"
                        })),
                );
            }
            body = body.child(cards);
        } else {
            body = body.child(note(
                "Choose one folder where you work. Neko will leave its files and setup in place.",
            ));
        }
        body.child(button(
            "setup-choose-folder",
            "Choose another folder…",
            !self.setup.busy && !evidence(),
            cx,
            |root, _, cx| root.setup_choose_folder(cx),
        ))
        .child(note("Neko reads available workspace guidance when you work there. External tools still require your approval before use."))
    }

    fn setup_discover_import(&mut self, cx: &mut Context<Self>, force: bool) {
        if !begin_import_discovery(
            self.setup.busy,
            &mut self.setup.import_discovery_started,
            force,
        ) {
            return;
        }
        self.setup.import_view = ImportView::Sources;
        self.setup_request(Command::SetupImport(ImportCommand::ListSources), cx);
    }

    fn setup_scan_source(&mut self, source_id: String, cx: &mut Context<Self>) {
        self.setup.import_source_name = self
            .setup
            .snapshot
            .import_preview
            .sources
            .iter()
            .find(|source| source.id == source_id)
            .map(|source| source.name.clone())
            .unwrap_or_else(|| source_id.clone());
        self.setup.import_view = ImportView::Scanning;
        let repository = self.setup.repository.read(cx).content().trim().to_owned();
        self.setup_request(
            Command::SetupImport(ImportCommand::Discover {
                repositories: if repository.is_empty() {
                    vec![]
                } else {
                    vec![repository]
                },
                source_id: Some(source_id),
            }),
            cx,
        );
    }

    fn setup_read_clipboard(&mut self, cx: &mut Context<Self>) {
        if self.setup.busy {
            return;
        }
        self.setup.busy = true;
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let result = client.request(Request::GetClipboardHistoryEnabled).await;
            let _ = this.update(cx, |root, cx| {
                root.setup.busy = false;
                if let Ok(Response::ClipboardHistoryEnabled { enabled }) = result {
                    root.flow.clipboard_enabled = enabled;
                    root.setup.clipboard_ready = true;
                    root.setup_request(Command::Snapshot, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn setup_start(&mut self, cx: &mut Context<Self>) {
        self.flow.accessibility_granted = self.accessibility.is_trusted();
        self.setup_read_clipboard(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                if this
                    .update(cx, |root, cx| {
                        let was_trusted = root.flow.accessibility_granted;
                        root.flow.accessibility_granted = root.accessibility.is_trusted();
                        if !was_trusted && root.flow.accessibility_granted && !evidence() {
                            root.ensure_hotkey_registered();
                        }
                        if !root.setup.busy {
                            if !root.setup.clipboard_ready {
                                root.setup_read_clipboard(cx);
                            } else {
                                root.setup_request(Command::Snapshot, cx);
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn setup_request(&mut self, command: Command, cx: &mut Context<Self>) {
        if self.setup.busy {
            return;
        }
        self.setup.busy = true;
        let poll = matches!(command, Command::Snapshot);
        let discover = matches!(
            command,
            Command::SetupImport(ImportCommand::Discover { .. })
        );
        let list_sources = matches!(command, Command::SetupImport(ImportCommand::ListSources));
        let apply = matches!(command, Command::SetupImport(ImportCommand::Apply { .. }));
        let brief = matches!(command, Command::SendMessage { .. });
        let saved_repository = match &command {
            Command::SaveWorkspace { workspace } => Some(workspace.repository.clone()),
            _ => None,
        };
        let connection_io = matches!(
            command,
            Command::Mcp(
                McpCommand::AddConnection { .. }
                    | McpCommand::Authenticate { .. }
                    | McpCommand::Discover { .. }
            )
        );
        if !poll {
            self.setup.error = None;
        }
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let result = client.request(Request::Workbench(command)).await;
            let _ = this.update(cx, |root, cx| {
                root.setup.busy = false;
                match result {
                    Ok(Response::Workbench(snapshot)) => {
                        if root.setup.workspace.is_none() { root.setup.workspace = snapshot.workspaces.first().map(|w| w.id.clone()); }
                        if discover {
                            root.setup.import_view = ImportView::Overview;
                            if evidence() {
                                root.setup.import_view = match std::env::var("NEKO_IMPORT_PREVIEW_TAB").ok().as_deref() {
                                    Some("skills") => ImportView::Items(ImportCategory::Skills),
                                    Some("connections") => ImportView::Items(ImportCategory::Connections),
                                    Some("workspaces") => ImportView::Items(ImportCategory::Workspaces),
                                    Some("schedules") => ImportView::Items(ImportCategory::Schedules),
                                    _ => ImportView::Overview,
                                };
                            }
                            root.setup.reset_import_consent();
                            let selection = default_import_selection(&snapshot.import_preview);
                            root.setup.selected_connections = selection.connections;
                            root.setup.global_connection_ids.clear();
                            root.setup.selected_repositories.clear();
                            root.setup.selected_skills = selection.skills;
                            root.setup.selected_workspaces = selection.workspaces;
                            root.setup.selected_schedules = selection.schedules;
                            root.setup.notice = Some("Choose each item to import. Tools and skills remain disabled until you enable them later.".into());
                        }
                        if apply {
                            root.setup.notice = Some("Import applied. MCP connections remain untrusted, skills remain disabled, and schedules stay paused until you opt in later.".into());
                            root.setup.advance_after_import = false;
                            root.setup.import_discovery_started = false;
                            root.setup.snapshot = snapshot;
                            root.setup_discover_import(cx, true);
                            return;
                        }
                        if list_sources { root.setup.notice = None; root.setup.import_view = ImportView::Sources; }
                        if brief { root.setup.brief_turn = snapshot.conversation.iter().rev().find(|m| m.role == ChatRole::Neko && m.pending).map(|m| m.id.clone()); }
                        if let Some(repository) = saved_repository.as_deref() {
                            root.setup.workspace = snapshot.workspaces.iter().find(|w| w.repository == repository).map(|w| w.id.clone());
                            root.setup.notice = Some("Workspace added. Neko will use what is already in that folder.".into());
                        }
                        root.setup.snapshot = snapshot;
                        if list_sources && evidence() {
                            if let Ok(source_id) = std::env::var("NEKO_IMPORT_PREVIEW_SOURCE") {
                                root.setup_scan_source(source_id, cx);
                                return;
                            }
                        }
                    }
                    Ok(Response::Error { message }) => {
                        if discover { root.setup.import_view = ImportView::Sources; }
                        reset_import_discovery_after_failure(discover || list_sources, &mut root.setup.import_discovery_started);
                        if apply { root.setup.advance_after_import = false; }
                        root.setup.error = Some(if connection_io {"Connection failed. Review the server setup or sign in, then retry.".into()} else {message})
                    },
                    Err(error) => {
                        if discover { root.setup.import_view = ImportView::Sources; }
                        reset_import_discovery_after_failure(discover || list_sources, &mut root.setup.import_discovery_started);
                        if apply { root.setup.advance_after_import = false; }
                        root.setup.error = Some(error.to_string())
                    },
                    _ => {
                        if discover { root.setup.import_view = ImportView::Sources; }
                        reset_import_discovery_after_failure(discover || list_sources, &mut root.setup.import_discovery_started);
                        if apply { root.setup.advance_after_import = false; }
                        root.setup.error = Some("Unexpected setup response; try again.".into())
                    },
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    pub(super) fn setup_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.setup.busy || matches!(self.flow.hotkey_recording, RecordingState::Recording) {
            return;
        }
        if self.setup.step == SetupStep::Import {
            self.close_and_persist_completion(window, cx);
            return;
        }
        self.setup.step = self.setup.step.next();
        self.flow.hotkey_recording =
            recording_after_navigation(self.setup.step, self.flow.hotkey_recording.clone());
        self.setup.error = None;
        self.setup.notice = None;
        cx.notify();
    }

    fn setup_workspace_picker(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut choices = div().flex().flex_wrap().gap(px(8.));
        for workspace in &self.setup.snapshot.workspaces {
            let id = workspace.id.clone();
            choices = choices.child(button(
                format!("setup-workspace-{id}"),
                format!(
                    "{}{}",
                    if self.setup.workspace.as_ref() == Some(&id) {
                        "✓ "
                    } else {
                        ""
                    },
                    workspace.name
                ),
                !self.setup.busy,
                cx,
                move |r, _, cx| {
                    if r.setup.workspace.as_ref() != Some(&id) {
                        r.setup.brief_turn = None;
                    }
                    r.setup.workspace = Some(id.clone());
                    cx.notify();
                },
            ));
        }
        if self.setup.snapshot.workspaces.is_empty() {
            choices = choices.child(note("Add or import a workspace on the Import step first."));
        }
        choices
    }

    fn setup_import_view(&self, cx: &mut Context<Self>) -> gpui::Div {
        match self.setup.import_view {
            ImportView::Sources => self.setup_import_sources_view(cx),
            ImportView::Scanning => self.setup_import_scanning_view(),
            ImportView::Overview => self.setup_import_overview_view(cx),
            ImportView::Items(category) => self.setup_import_items_view(category, cx),
        }
    }

    fn setup_import_sources_view(&self, cx: &mut Context<Self>) -> gpui::Div {
        let preview = &self.setup.snapshot.import_preview;
        let count = preview.sources.len();
        let columns = if count == 4 { 2 } else { 3 };
        let tile_width = if columns == 2 { 474. } else { 312. };
        let tile_height = if columns == 2 { 132. } else { 224. };
        let mut tiles = div().flex().flex_wrap().gap(px(14.));
        for source in &preview.sources {
            let source_id = source.id.clone();
            tiles = tiles.child(source_tile(
                source.id.clone(),
                source.name.clone(),
                tile_width,
                tile_height,
                !self.setup.busy,
                cx,
                move |r, _, cx| r.setup_scan_source(source_id.clone(), cx),
            ));
        }
        let mut content = div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_size(px(14.))
                            .child("Found on this Mac"),
                    )
                    .child(note(format!("{} available", count))),
            )
            .child(tiles);
        if count == 0 {
            content = content.child(note(
                "No supported setup found yet. You can choose a folder below or continue.",
            ));
        }
        content = content.child(
            div()
                .id("setup-another-source")
                .tab_index(0)
                .tab_stop(!self.setup.busy)
                .h(px(50.))
                .px(px(18.))
                .flex()
                .items_center()
                .justify_between()
                .rounded(px(10.))
                .border_1()
                .border_color(gpui::rgb(0x45474d))
                .bg(gpui::rgb(0x222428))
                .cursor_pointer()
                .hover(|style| style.bg(gpui::rgb(0x2b2d32)))
                .on_click(cx.listener(|r, _, _, cx| {
                    r.setup.show_new_workspace = !r.setup.show_new_workspace;
                    cx.notify();
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(
                            div()
                                .text_size(px(22.))
                                .text_color(gpui::rgb(0x9da1aa))
                                .child("+"),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .child("Use another source"),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(gpui::rgb(0x92969f))
                                        .child("Choose a folder on this Mac"),
                                ),
                        ),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(gpui::rgb(0xcbd5e8))
                        .child("Choose →"),
                ),
        );
        if self.setup.show_new_workspace {
            content = content.child(note("Enter an absolute folder path. Neko will look for local skills and workspace setup there; nothing is imported yet."))
                .child(field("Folder path", &self.setup.repository))
                .child(button("setup-scan-folder", "Look through folder →", !self.setup.busy, cx, |r, _, cx| {
                    let repository = r.setup.repository.read(cx).content().trim().to_owned();
                    if !repository.starts_with('/') {
                        r.setup.error = Some("Enter an absolute folder path to continue.".into());
                        cx.notify();
                        return;
                    }
                    r.setup_scan_source("agents".into(), cx);
                }));
        }
        content
    }

    fn setup_import_scanning_view(&self) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .flex_1()
            .gap(px(18.))
            .child(
                div()
                    .size(px(64.))
                    .rounded(px(14.))
                    .bg(gpui::rgb(0xe8e9e6))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(gpui::rgb(0x161719))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_size(px(30.))
                    .child(
                        self.setup
                            .import_source_name
                            .chars()
                            .next()
                            .unwrap_or('N')
                            .to_string(),
                    ),
            )
            .child(
                div()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_size(px(30.))
                    .child(format!("Looking through {}", self.setup.import_source_name)),
            )
            .child(note(
                "Reading local setup only. Neko won't run tools or change the source.",
            ))
            .child(
                div()
                    .w(px(400.))
                    .h(px(4.))
                    .rounded(px(2.))
                    .bg(gpui::rgb(0x303136))
                    .child(
                        div()
                            .w(px(230.))
                            .h(px(4.))
                            .rounded(px(2.))
                            .bg(gpui::rgb(0xb6c5e3)),
                    ),
            )
            .child(note("Finding tools, skills and workspaces…"))
    }

    fn setup_import_overview_view(&self, cx: &mut Context<Self>) -> gpui::Div {
        let preview = &self.setup.snapshot.import_preview;
        let mut categories = div()
            .flex()
            .flex_col()
            .rounded(px(12.))
            .border_1()
            .border_color(gpui::rgb(0x44464d))
            .overflow_hidden();
        for category in ImportCategory::ALL {
            let count = preview
                .candidates
                .iter()
                .filter(|item| category.matches(&item.kind))
                .count();
            if count == 0 {
                continue;
            }
            let selected = preview
                .candidates
                .iter()
                .filter(|item| category.matches(&item.kind))
                .filter(|item| self.setup.item_selected(item))
                .count();
            let activate = move |r: &mut OnboardingRoot, cx: &mut Context<OnboardingRoot>| {
                r.setup.import_view = ImportView::Items(category);
                cx.notify();
            };
            categories = categories.child(
                div()
                    .id(format!("setup-category-{:?}", category))
                    .tab_index(0)
                    .tab_stop(!self.setup.busy)
                    .h(px(72.))
                    .px(px(20.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(gpui::rgb(0x3b3d43))
                    .bg(gpui::rgb(0x27292e))
                    .cursor_pointer()
                    .hover(|style| style.bg(gpui::rgb(0x303238)))
                    .on_click(cx.listener(move |r, _, _, cx| activate(r, cx)))
                    .on_key_down(cx.listener(move |r, e: &KeyDownEvent, _, cx| {
                        if e.keystroke.modifiers == gpui::Modifiers::default()
                            && matches!(e.keystroke.key.as_str(), "enter" | "space")
                        {
                            r.setup.import_view = ImportView::Items(category);
                            cx.notify();
                            cx.stop_propagation();
                        }
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_size(px(15.))
                                    .child(category.title()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(gpui::rgb(0xa0a3ac))
                                    .child(format!("{count} found · {selected} selected")),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(19.))
                            .text_color(gpui::rgb(0xa0a3ac))
                            .child("›"),
                    ),
            );
        }
        div()
            .flex()
            .gap(px(24.))
            .child(div().flex_1().min_w_0().child(categories))
            .child(
                div()
                    .w(px(312.))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap(px(18.))
                    .p(px(22.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(gpui::rgb(0x44464d))
                    .bg(gpui::rgb(0x27292e))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Before you bring it over"),
                    )
                    .child(note(format!(
                        "Neko will make its own copy. Your {} setup stays untouched.",
                        self.setup.import_source_name
                    )))
                    .child(note("✓  Connections start off"))
                    .child(note("✓  Schedules stay paused"))
                    .child(note("✓  Credentials need your approval")),
            )
    }

    fn setup_import_items_view(
        &self,
        category: ImportCategory,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let preview = &self.setup.snapshot.import_preview;
        let mut content = div().flex().flex_col().gap(px(14.));
        let mut tabs = div()
            .flex()
            .gap(px(24.))
            .border_b_1()
            .border_color(gpui::rgb(0x44464d));
        for tab in ImportCategory::ALL {
            let count = preview
                .candidates
                .iter()
                .filter(|item| tab.matches(&item.kind))
                .count();
            tabs = tabs.child(
                div()
                    .id(format!("setup-tab-{:?}", tab))
                    .tab_index(0)
                    .tab_stop(!self.setup.busy)
                    .pb(px(12.))
                    .pt(px(5.))
                    .border_b_2()
                    .border_color(if tab == category {
                        gpui::rgb(0xe8e9e6)
                    } else {
                        gpui::rgb(0x1c1d20)
                    })
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(if tab == category {
                        gpui::rgb(0xe8e9e6)
                    } else {
                        gpui::rgb(0x9699a2)
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |r, _, _, cx| {
                        r.setup.import_view = ImportView::Items(tab);
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(move |r, e: &KeyDownEvent, _, cx| {
                        if e.keystroke.modifiers == gpui::Modifiers::default()
                            && matches!(e.keystroke.key.as_str(), "enter" | "space")
                        {
                            r.setup.import_view = ImportView::Items(tab);
                            cx.notify();
                            cx.stop_propagation();
                        }
                    }))
                    .child(format!("{}  {}", tab.title(), count)),
            );
        }
        content = content.child(tabs);
        if preview.candidates.is_empty() {
            content = content.child(note("Nothing importable was found in this source. Go back and try another source, or continue."));
        }
        for workspace_group in [false, true] {
            let group_candidates = if category == ImportCategory::Workspaces {
                workspace_review_rows(&preview.candidates, self.setup.show_other_folders)
                    .into_iter()
                    .filter(|item| {
                        (item.metadata.get("review_group").map(String::as_str) == Some("other"))
                            == workspace_group
                    })
                    .collect::<Vec<_>>()
            } else {
                preview
                    .candidates
                    .iter()
                    .filter(|item| {
                        category.matches(&item.kind) && item.workspace.is_some() == workspace_group
                    })
                    .collect::<Vec<_>>()
            };
            if category == ImportCategory::Workspaces && workspace_group {
                let other_count = preview
                    .candidates
                    .iter()
                    .filter(|item| {
                        item.kind == ImportCandidateKind::Workspace
                            && item.metadata.get("review_group").map(String::as_str)
                                == Some("other")
                    })
                    .count();
                if other_count > 0 {
                    let expanded = self.setup.show_other_folders;
                    content = content.child(button(
                        "setup-import-other-folders",
                        format!(
                            "{} Other folders ({other_count})",
                            if expanded { "▾" } else { "▸" }
                        ),
                        !self.setup.busy,
                        cx,
                        move |r, _, cx| {
                            r.setup.show_other_folders = !r.setup.show_other_folders;
                            cx.notify();
                        },
                    ));
                }
            }
            if !group_candidates.is_empty()
                && !(category == ImportCategory::Workspaces && workspace_group)
            {
                content = content.child(
                    div()
                        .pt(px(10.))
                        .text_size(px(11.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(gpui::rgb(0x999da6))
                        .child(if category == ImportCategory::Workspaces {
                            "PROJECT FOLDERS"
                        } else if workspace_group {
                            "IN A WORKSPACE"
                        } else {
                            "EVERYWHERE"
                        }),
                );
            }
            for candidate in group_candidates {
                let id = candidate.id.clone();
                let candidate_kind = candidate.kind.clone();
                let selected = match &candidate_kind {
                    neko_protocol::setup_import::ImportCandidateKind::Connection => {
                        self.setup.selected_connections.contains(&id)
                    }
                    neko_protocol::setup_import::ImportCandidateKind::Skill => {
                        self.setup.selected_skills.contains(&id)
                    }
                    neko_protocol::setup_import::ImportCandidateKind::Workspace => {
                        self.setup.selected_workspaces.contains(&id)
                    }
                    neko_protocol::setup_import::ImportCandidateKind::Schedule => {
                        self.setup.selected_schedules.contains(&id)
                    }
                };
                let kind = import_candidate_label(&candidate.kind);
                let scope = candidate.scope.clone();
                let credentials = candidate_has_credentials(&candidate.metadata);
                let already_in_neko = candidate
                    .metadata
                    .get("already_in_neko")
                    .is_some_and(|value| value == "true");
                let different_setup = candidate
                    .metadata
                    .get("same_name_different_setup")
                    .is_some_and(|value| value == "true");
                let problem = candidate.problem.clone();
                let selected_candidate = self.setup.selected_workspaces.iter().any(|selected| {
                    preview.candidates.iter().any(|workspace| {
                        workspace.id == *selected && workspace.workspace == candidate.workspace
                    })
                });
                let existing_workspace = self.setup.snapshot.workspaces.iter().any(|workspace| {
                    candidate.workspace.as_deref() == Some(workspace.repository.as_str())
                });
                let workspace_selected = workspace_scope_selected(
                    candidate.workspace.as_deref(),
                    selected_candidate,
                    existing_workspace,
                );
                let import_globally = self.setup.global_connection_ids.contains(&id);
                let disabled = candidate_is_unavailable(
                    &candidate_kind,
                    problem.as_deref(),
                    candidate.workspace.as_deref(),
                    workspace_selected,
                    import_globally,
                );
                let show_global_fallback = candidate_kind == ImportCandidateKind::Connection
                    && candidate.workspace.is_some()
                    && (!workspace_selected || import_globally)
                    && problem.is_none();
                let subtitle = if candidate_kind == ImportCandidateKind::Workspace {
                    candidate.workspace.clone().unwrap_or_else(|| scope.clone())
                } else {
                    format!(
                        "{kind} · {scope}{}",
                        if credentials {
                            " · Sign-in available"
                        } else {
                            ""
                        }
                    )
                };
                content = content.child(import_item_button(
                    format!("setup-import-candidate-{id}"),
                    candidate.name.clone(),
                    subtitle,
                    if selected && import_globally {
                        "Selected · global"
                    } else if show_global_fallback {
                        "Choose scope"
                    } else {
                        candidate_review_badge(
                            already_in_neko,
                            problem.is_some(),
                            disabled && problem.is_none(),
                            selected,
                            different_setup,
                        )
                    },
                    !self.setup.busy && !disabled && !already_in_neko,
                    cx,
                    move |r, _, cx| {
                        let set = match &candidate_kind {
                            neko_protocol::setup_import::ImportCandidateKind::Connection => {
                                &mut r.setup.selected_connections
                            }
                            neko_protocol::setup_import::ImportCandidateKind::Skill => {
                                &mut r.setup.selected_skills
                            }
                            neko_protocol::setup_import::ImportCandidateKind::Workspace => {
                                &mut r.setup.selected_workspaces
                            }
                            neko_protocol::setup_import::ImportCandidateKind::Schedule => {
                                &mut r.setup.selected_schedules
                            }
                        };
                        if !set.remove(&id) {
                            set.insert(id.clone());
                        } else {
                            r.setup.global_connection_ids.remove(&id);
                        }
                        r.setup.reset_import_consent();
                        cx.notify();
                    },
                ));
                if show_global_fallback {
                    let fallback_id = candidate.id.clone();
                    let fallback_selected = import_globally;
                    content = content.child(button(
                        format!("setup-import-global-fallback-{fallback_id}"),
                        if fallback_selected {
                            "Use workspace scope instead"
                        } else {
                            "Keep as global connection (paused)"
                        },
                        !self.setup.busy && !already_in_neko,
                        cx,
                        move |r, _, cx| {
                            if r.setup.global_connection_ids.remove(&fallback_id) {
                                if !workspace_selected {
                                    r.setup.selected_connections.remove(&fallback_id);
                                }
                            } else {
                                r.setup.global_connection_ids.insert(fallback_id.clone());
                                r.setup.selected_connections.insert(fallback_id.clone());
                            }
                            r.setup.reset_import_consent();
                            cx.notify();
                        },
                    ));
                    if fallback_selected {
                        content = content.child(note("Saved as a global definition only. It stays paused, with no tool grants; you can authorize it in a workspace later."));
                    }
                }
                if let Some(problem) = problem {
                    content = content.child(note(problem));
                } else if disabled {
                    if let Some(workspace) = candidate.workspace.as_deref() {
                        content = content.child(note(format!(
                            "Add {workspace} as a workspace to import this item."
                        )));
                    }
                } else if candidate.kind == ImportCandidateKind::Workspace && workspace_group {
                    if let Some(reason) = candidate.metadata.get("review_reason") {
                        content = content.child(note(reason.clone()));
                    }
                }
            }
        }
        for warning in &preview.warnings {
            content = content.child(note(warning.clone()));
        }
        if preview.active_source.is_none() && uses_legacy_import_rows(preview.candidates.len()) {
            for repository in &preview.repositories {
                let path = repository.clone();
                content = content.child(button(
                    format!("setup-import-repo-{path}"),
                    format!(
                        "{} Workspace · {path}",
                        if self.setup.selected_repositories.contains(&path) {
                            "✓"
                        } else {
                            "+"
                        }
                    ),
                    !self.setup.busy,
                    cx,
                    move |r, _, cx| {
                        if !r.setup.selected_repositories.remove(&path) {
                            r.setup.selected_repositories.insert(path.clone());
                        }
                        r.setup.reset_import_consent();
                        cx.notify();
                    },
                ));
            }
            for connection in &preview.connections {
                let id = connection.id.clone();
                content = content.child(button(
                    format!("setup-import-connection-{id}"),
                    format!(
                        "{} {} · {}{}",
                        if self.setup.selected_connections.contains(&id) {
                            "✓"
                        } else {
                            "+"
                        },
                        connection.name,
                        connection.repository.as_deref().unwrap_or("Global"),
                        if connection.has_credentials {
                            " · credentials found"
                        } else {
                            ""
                        }
                    ),
                    !self.setup.busy && connection.problem.is_none(),
                    cx,
                    move |r, _, cx| {
                        if !r.setup.selected_connections.remove(&id) {
                            r.setup.selected_connections.insert(id.clone());
                        }
                        r.setup.reset_import_consent();
                        cx.notify();
                    },
                ));
                if let Some(problem) = &connection.problem {
                    content = content.child(note(problem.clone()));
                }
            }
            for schedule in &preview.schedules {
                let id = schedule.id.clone();
                content = content
                    .child(button(
                        format!("setup-import-schedule-{id}"),
                        format!(
                            "{} Schedule · {} · import paused",
                            if self.setup.selected_schedules.contains(&id) {
                                "✓"
                            } else {
                                "+"
                            },
                            schedule.name
                        ),
                        !self.setup.busy,
                        cx,
                        move |r, _, cx| {
                            if !r.setup.selected_schedules.remove(&id) {
                                r.setup.selected_schedules.insert(id.clone());
                            }
                            r.setup.reset_import_consent();
                            cx.notify();
                        },
                    ))
                    .child(note(format!(
                        "{} · {}",
                        schedule.source,
                        schedule.warnings.join(" · ")
                    )));
            }
        }
        content=content.child(note(format!("{} items found in this source. Imported tools and skills stay off until you enable them.", preview.candidates.len())));
        if !preview.preview_id.is_empty() {
            content = content
                .child(button(
                    "setup-import-credentials",
                    format!(
                        "{} Copy selected credentials into Keychain",
                        if self.setup.credentials { "✓" } else { "○" }
                    ),
                    !self.setup.busy,
                    cx,
                    |r, _, cx| {
                        r.setup.credentials = !r.setup.credentials;
                        cx.notify();
                    },
                ))
                .child(button(
                    "setup-import-trust",
                    format!(
                        "{} Trust selected local MCP processes",
                        if self.setup.trust { "✓" } else { "○" }
                    ),
                    !self.setup.busy,
                    cx,
                    |r, _, cx| {
                        r.setup.trust = !r.setup.trust;
                        cx.notify();
                    },
                ));
        }
        content
    }

    fn setup_apply_import(&mut self, cx: &mut Context<Self>) {
        self.setup_request(
            Command::SetupImport(ImportCommand::Apply {
                schedule_ids: self.setup.selected_schedules.iter().cloned().collect(),
                skill_ids: self.setup.selected_skills.iter().cloned().collect(),
                workspace_ids: self.setup.selected_workspaces.iter().cloned().collect(),
                preview_id: self.setup.snapshot.import_preview.preview_id.clone(),
                connection_ids: self.setup.selected_connections.iter().cloned().collect(),
                global_connection_ids: self.setup.global_connection_ids.iter().cloned().collect(),
                repositories: self.setup.selected_repositories.iter().cloned().collect(),
                include_credentials: self.setup.credentials,
                trust_local_processes: self.setup.trust,
            }),
            cx,
        );
    }

    fn setup_tools_view(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut body=div().flex().flex_col().gap(px(12.)).child(self.setup_workspace_picker(cx))
            .child(div().flex().gap(px(8.)).child(button("setup-registry","Browse MCP Registry",true,cx,|_,_,cx|cx.open_url("https://registry.modelcontextprotocol.io"))).child(button("setup-skills-catalog","Browse skills.sh",true,cx,|_,_,cx|cx.open_url("https://skills.sh"))))
            .child(note("Catalogs are external. Review publishers and instructions before installing; listings are not an approval or safety guarantee."));
        let Some(workspace) = &self.setup.workspace else {
            return body;
        };
        body = body.child(button(
            "setup-refresh-skills",
            "Discover local skills",
            !self.setup.busy,
            cx,
            |r, _, cx| {
                r.setup_request(
                    Command::Skills(neko_protocol::skills::SkillCommand::Refresh),
                    cx,
                )
            },
        ));
        for skill in self
            .setup
            .snapshot
            .skills
            .available
            .iter()
            .filter(|s| s.workspace_id.as_ref().is_none_or(|w| w == workspace))
            .take(12)
        {
            let path = skill.path.clone();
            let review_path = path.clone();
            let workspace_id = workspace.clone();
            let hash = skill.content_hash.clone();
            let enabled =
                self.setup.snapshot.skills.enabled.iter().any(|s| {
                    s.workspace_id == *workspace && s.path == path && s.content_hash == hash
                });
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(note(format!(
                        "Skill · {} · {}",
                        skill.name, skill.description
                    )))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(button(
                                format!("setup-review-skill-{path}"),
                                "Open instructions",
                                true,
                                cx,
                                move |_, _, cx| {
                                    cx.open_with_system(std::path::Path::new(&review_path))
                                },
                            ))
                            .child(button(
                                format!("setup-enable-skill-{path}"),
                                if enabled {
                                    "Disable here"
                                } else {
                                    "Enable reviewed instructions here"
                                },
                                !self.setup.busy,
                                cx,
                                move |r, _, cx| {
                                    r.setup_request(
                                        Command::Skills(
                                            neko_protocol::skills::SkillCommand::SetEnabled {
                                                workspace_id: workspace_id.clone(),
                                                path: path.clone(),
                                                content_hash: hash.clone(),
                                                enabled: !enabled,
                                            },
                                        ),
                                        cx,
                                    )
                                },
                            )),
                    ),
            );
        }
        if self.setup.snapshot.skills.available.len() > 12 {
            body=body.child(note("Showing the first 12 scoped skills. Search the complete library in Tools & skills after setup."));
        }
        for connection in self
            .setup
            .snapshot
            .mcp
            .connections
            .iter()
            .filter(|c| c.available_in(workspace))
        {
            let id = connection.id.clone();
            let auth_id = id.clone();
            body = body
                .child(div().text_size(px(16.)).child(format!(
                    "{} · {}",
                    connection.label,
                    if connection.workspace_id.is_empty() {
                        "Global definition"
                    } else {
                        "Workspace"
                    }
                )))
                .child(button(
                    format!("setup-discover-{id}"),
                    "Connect and discover tools",
                    !self.setup.busy && connection.enabled,
                    cx,
                    move |r, _, cx| {
                        r.setup_request(
                            Command::Mcp(McpCommand::Discover {
                                connection_id: id.clone(),
                            }),
                            cx,
                        )
                    },
                ));
            if matches!(
                connection.config,
                neko_protocol::mcp_host::ServerConfig::Http { .. }
            ) {
                body = body.child(button(
                    format!("setup-auth-{auth_id}"),
                    "Sign in through browser",
                    !self.setup.busy && connection.enabled,
                    cx,
                    move |r, _, cx| {
                        r.setup_request(
                            Command::Mcp(McpCommand::Authenticate {
                                connection_id: auth_id.clone(),
                                client_id: None,
                            }),
                            cx,
                        )
                    },
                ));
            }
            if let Some(error) = &connection.error {
                body = body.child(note(error.clone()));
            }
            for tool in &connection.tools {
                let connection_id = connection.id.clone();
                let tool_name = tool.name.clone();
                let schema_hash = tool.schema_hash.clone();
                let workspace_id = workspace.clone();
                let allowed = self.setup.snapshot.mcp.grants.iter().any(|g| {
                    g.workspace_id == *workspace
                        && g.connection_id == connection.id
                        && g.tool_name == tool.name
                        && g.schema_hash == tool.schema_hash
                });
                body = body.child(button(
                    format!("setup-grant-{connection_id}-{tool_name}"),
                    format!(
                        "{} {} · {}",
                        if allowed { "✓" } else { "+" },
                        tool.name,
                        if tool.read_only {
                            "declared read-only"
                        } else {
                            "actions require approval in chat"
                        }
                    ),
                    !self.setup.busy && connection.enabled,
                    cx,
                    move |r, _, cx| {
                        r.setup_request(
                            Command::Mcp(McpCommand::SetWorkspaceToolGrant {
                                workspace_id: workspace_id.clone(),
                                connection_id: connection_id.clone(),
                                tool_name: tool_name.clone(),
                                schema_hash: schema_hash.clone(),
                                allowed: !allowed,
                            }),
                            cx,
                        )
                    },
                ));
            }
        }
        body = body
            .child(div().text_size(px(16.)).child("Add your own MCP"))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(button(
                        "setup-transport",
                        if self.setup.server_local {
                            "Local process · change"
                        } else {
                            "HTTP server · change"
                        },
                        !self.setup.busy,
                        cx,
                        |r, _, cx| {
                            r.setup.server_local = !r.setup.server_local;
                            r.setup.server_trust = false;
                            cx.notify();
                        },
                    ))
                    .child(button(
                        "setup-scope",
                        if self.setup.server_global {
                            "Global definition · change"
                        } else {
                            "This workspace · change"
                        },
                        !self.setup.busy,
                        cx,
                        |r, _, cx| {
                            r.setup.server_global = !r.setup.server_global;
                            r.setup.server_trust = false;
                            cx.notify();
                        },
                    )),
            )
            .child(field("Name", &self.setup.server_name))
            .child(field("Server", &self.setup.server_target));
        if self.setup.server_local {
            body = body
                .child(field("Arguments as a JSON list", &self.setup.server_args))
                .child(button(
                    "setup-local-trust",
                    if self.setup.server_trust {
                        "✓ Local process trusted"
                    } else {
                        "Trust this local process to run"
                    },
                    !self.setup.busy,
                    cx,
                    |r, _, cx| {
                        r.setup.server_trust = !r.setup.server_trust;
                        cx.notify();
                    },
                ));
        }
        body.child(button("setup-add-server","Add definition",!self.setup.busy,cx,|r,_,cx| {
            let target=r.setup.server_target.read(cx).content().trim().to_owned();
            let config=if r.setup.server_local {
                let raw=r.setup.server_args.read(cx).content().trim();
                let args=if raw.is_empty(){Ok(vec![])}else{serde_json::from_str::<Vec<String>>(raw)};
                let args=match args{Ok(args)=>args,Err(_)=>{r.setup.error=Some("Arguments must be a JSON list of strings.".into());cx.notify();return;}};
                neko_protocol::mcp_host::ServerConfig::Stdio{command:target,args,cwd:None}
            }else{neko_protocol::mcp_host::ServerConfig::Http{url:target}};
            let workspace_id=if r.setup.server_global{String::new()}else{r.setup.workspace.clone().unwrap_or_default()};
            r.setup_request(Command::Mcp(McpCommand::AddConnection{workspace_id,label:r.setup.server_name.read(cx).content().trim().to_owned(),config,trust_local_process:r.setup.server_trust,credentials:None}),cx);
        })).child(note("For credential JSON or a registered OAuth client ID, use Tools & skills after setup. No named service is required."))
    }

    fn setup_responsibility_view(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut body=div().flex().flex_col().gap(px(12.)).child(self.setup_workspace_picker(cx)).child(field("A standing responsibility",&self.setup.responsibility))
            .child(note("Neko checks the granted tools below. Local fixes still require approval; publishing and away messaging are not enabled here."));
        if let Some(workspace) = &self.setup.workspace {
            let connections: Vec<_> = self
                .setup
                .snapshot
                .mcp
                .connections
                .iter()
                .filter(|c| {
                    c.enabled
                        && c.available_in(workspace)
                        && self
                            .setup
                            .snapshot
                            .mcp
                            .grants
                            .iter()
                            .any(|g| g.workspace_id == *workspace && g.connection_id == c.id)
                })
                .map(|c| c.label.clone())
                .collect();
            body = body.child(note(format!(
                "Available: {}",
                if connections.is_empty() {
                    "none — grant a tool on the previous step".into()
                } else {
                    connections.join(", ")
                }
            )));
        }
        body.child(button(
            "setup-save-responsibility",
            "Start watching with these tools",
            !self.setup.busy && self.setup.workspace.is_some(),
            cx,
            |r, _, cx| {
                let Some(workspace_id) = r.setup.workspace.clone() else {
                    return;
                };
                let connection_ids =
                    r.setup
                        .snapshot
                        .mcp
                        .connections
                        .iter()
                        .filter(|c| {
                            c.enabled
                                && c.available_in(&workspace_id)
                                && r.setup.snapshot.mcp.grants.iter().any(|g| {
                                    g.workspace_id == workspace_id && g.connection_id == c.id
                                })
                        })
                        .map(|c| c.id.clone())
                        .collect();
                let instruction = r.setup.responsibility.read(cx).content().trim().to_owned();
                r.setup_request(
                    Command::Mcp(McpCommand::SaveResponsibility {
                        responsibility: Responsibility {
                            id: String::new(),
                            workspace_id,
                            instruction,
                            connection_ids,
                            enabled: true,
                            prepare_low_risk: false,
                            next_due_ms: 0,
                            last_attempt_ms: None,
                            last_result: String::new(),
                            failures: 0,
                        },
                    }),
                    cx,
                );
            },
        ))
        .children(
            self.setup
                .snapshot
                .mcp
                .responsibilities
                .iter()
                .filter(|r| Some(&r.workspace_id) == self.setup.workspace.as_ref())
                .map(|r| note(format!("{} · {}", r.instruction, r.last_result))),
        )
    }

    pub(super) fn render_setup(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let stage = self.setup.step;
        let labels = [
            "Welcome",
            "Your Mac",
            "Import",
            "Tools",
            "Responsibility",
            "First look",
        ];
        let titles = [
            "While you’re away,\nsomeone should be you.",
            "Let Neko work on your Mac",
            "Choose a workspace",
            "Your tools, your boundaries",
            "What should I look after?",
            "Your first look, grounded in real work",
        ];
        let subtitles = [
            "One Neko to talk to. A quick panel when you need it. Your work stays yours.",
            "Each permission is optional. You can always open the full app without a shortcut.",
            "Neko works with what is already in that folder. Nothing is copied or enabled.",
            "Global definitions are available everywhere; permissions are granted per workspace.",
            "Start with one clear responsibility. Tool access stays limited to what you grant.",
            "Neko reads the selected workspace and granted tools, then prepares a useful brief.",
        ];
        let mut progress = div().flex().gap(px(6.)).items_center();
        for (index, label) in labels.iter().enumerate() {
            let _ = label;
            progress = progress.child(div().w(px(20.)).h(px(3.)).rounded(px(2.)).bg(
                if index == stage.index() {
                    theme::active().text_primary
                } else {
                    theme::active().border_hairline_strong
                },
            ));
        }
        let mut body = div()
            .id("setup-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(if stage == SetupStep::Import { 76. } else { 90. }))
            .py(px(if stage == SetupStep::Import { 26. } else { 28. }))
            .flex()
            .flex_col()
            .gap(px(18.))
            .when(stage == SetupStep::Welcome, |b| {
                b.items_center().justify_center().text_center()
            });
        if stage == SetupStep::Welcome {
            body = body.child(
                gpui::svg()
                    .path(crate::assets::icon::MARK)
                    .size(px(88.))
                    .text_color(theme::active().text_primary),
            );
        }
        if stage == SetupStep::Import {
            body = body.child(
                div()
                    .text_size(px(12.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(gpui::rgb(0x92969d))
                    .child("WORK WHERE YOU ALREADY WORK"),
            );
        }
        body = body
            .child(
                div()
                    .text_size(px(if stage == SetupStep::Welcome {
                        48.
                    } else if stage == SetupStep::Import {
                        34.
                    } else {
                        30.
                    }))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .line_height(px(if stage == SetupStep::Welcome {
                        54.
                    } else {
                        38.
                    }))
                    .child(titles[stage.index()].to_owned()),
            )
            .child(note(subtitles[stage.index()].to_owned()));
        match stage {
            SetupStep::Welcome => {
                body=body.child(div().mt(px(18.)).child(button("setup-get-started","Get started",!self.setup.busy,cx,|r,w,cx|r.setup_next(w,cx))))
                    .child(note("Start with one workspace. Add tools and responsibilities at your own pace."));
            }
            SetupStep::Mac => {
                body = body.child(button("setup-accessibility", if self.flow.accessibility_granted { "Accessibility enabled" } else { "Allow Accessibility…" }, !self.setup.busy && !self.flow.accessibility_granted && !evidence(), cx, |r,_,cx| { r.flow.step=Step::AccessibilityAsk; r.request_accessibility(cx); }))
                    .child(note("Used for the global shortcut and paste actions. macOS controls this permission."))
                    .child(button("setup-clipboard", if !self.setup.clipboard_ready { "Reading clipboard setting…" } else if self.flow.clipboard_enabled { "Turn clipboard history off" } else { "Enable clipboard history" }, self.setup.clipboard_ready && !self.setup.busy && !evidence(), cx, |r,_,cx| {
                        if !begin_clipboard_change(&mut r.setup.busy,r.setup.clipboard_ready) { return; }
                        let enabled = !r.flow.clipboard_enabled;
                        let client=r.client.clone(); cx.notify();
                        cx.spawn(async move |this,cx| { let result=client.request(Request::SetClipboardHistoryEnabled{enabled}).await; let _=this.update(cx,|r,cx| { r.setup.busy=false; match result { Ok(Response::ClipboardHistoryEnabled{enabled:actual}) if actual==enabled=>r.flow.clipboard_enabled=actual,Ok(Response::Error{message})=>r.setup.error=Some(message),Err(e)=>r.setup.error=Some(e.to_string()),Ok(_)=>r.setup.error=Some("Clipboard setting was not acknowledged. Try again.".into()) }; cx.notify(); }); }).detach();
                    }))
                    .child(note("Saved locally. Off by default; enabling starts clipboard capture."))
                    .child(button("setup-shortcut", format!("Shortcut: {} · Change", self.flow.current_combo.display().replace('⌥',"Option + ").replace('⌘',"Command + ").replace('⌃',"Control + ").replace('⇧',"Shift + ")), self.flow.accessibility_granted && !self.setup.busy && !evidence(), cx, |r,_,cx| { r.flow.step=Step::LearnHotkey; r.toggle_recording(cx); }));
                if matches!(self.flow.hotkey_recording, RecordingState::Recording) {
                    body = body.child(note(
                        "Press the new key combination. Escape cancels recording.",
                    ));
                }
                if let RecordingState::Rejected(reason) = &self.flow.hotkey_recording {
                    body = body.child(note(reason.clone()));
                }
            }
            SetupStep::Import => {
                body = body.child(self.setup_workspace_view(cx));
            }
            SetupStep::Tools => {
                body = body.child(self.setup_tools_view(cx));
            }
            SetupStep::Responsibility => {
                body = body.child(self.setup_responsibility_view(cx));
            }
            SetupStep::Sweep => {
                body=body.child(self.setup_workspace_picker(cx)).child(button("setup-brief", "Read my workspace and prepare a brief", !self.setup.busy && self.setup.workspace.is_some() && !self.setup.snapshot.conversation.iter().any(|m|m.pending),cx,|r,_,cx| {
                    r.setup_request(Command::SendMessage { workspace_id:r.setup.workspace.clone(), text:"Prepare my first Neko brief. Read only the selected workspace and explicitly granted read-only tools. Do not edit, create tickets, publish, or call tools with side effects. Summarize what needs attention, cite the evidence you actually read, recommend one next step, and say what you could not inspect. Do not invent activity.".into() },cx);
                }));
                if let Some(turn) = self.setup.brief_turn.as_ref().and_then(|id| {
                    self.setup.snapshot.conversation.iter().find(|m| {
                        &m.id == id
                            && brief_in_scope(
                                m.workspace_id.as_deref(),
                                self.setup.workspace.as_deref(),
                            )
                    })
                }) {
                    body = body.child(note(if turn.pending {
                        "Reading your workspace…".to_string()
                    } else {
                        turn.text.clone()
                    }));
                    if turn.failed {
                        body=body.child(note("The brief could not finish. Retry or open Today to inspect the failure."));
                    }
                } else {
                    body=body.child(note("No sweep has run yet. You can prepare a brief now or open Today and start there."));
                }
            }
        }
        if let Some(error) = &self.setup.error {
            body = body.child(note(format!("Could not complete: {error}")));
        }
        if let Some(notice) = &self.setup.notice {
            body = body.child(note(notice.clone()));
        }
        let footer = div()
            .flex()
            .justify_between()
            .items_center()
            .px(px(40.))
            .py(px(18.))
            .border_t_1()
            .border_color(theme::active().border_hairline)
            .child(button(
                "setup-back",
                "Back",
                !self.setup.busy && stage != SetupStep::Welcome,
                cx,
                |r, _, cx| {
                    r.setup.step = r.setup.step.back();
                    r.flow.hotkey_recording =
                        recording_after_navigation(r.setup.step, r.flow.hotkey_recording.clone());
                    cx.notify();
                },
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .when(stage != SetupStep::Import, |actions| {
                        actions.child(button(
                            "setup-skip",
                            "Set up later",
                            !self.setup.busy,
                            cx,
                            |r, w, cx| r.close_and_persist_completion(w, cx),
                        ))
                    })
                    .child(button(
                        "setup-next",
                        if stage == SetupStep::Import {
                            if self.setup.workspace.is_some() {
                                "Open Neko →".to_owned()
                            } else {
                                "I'll do this later →".to_owned()
                            }
                        } else {
                            "Continue".to_owned()
                        },
                        !self.setup.busy,
                        cx,
                        |r, w, cx| r.setup_next(w, cx),
                    )),
            );
        let header = if stage == SetupStep::Import {
            div()
                .h(px(54.))
                .px(px(24.))
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(gpui::rgb(0x303136))
                .window_control_area(gpui::WindowControlArea::Drag)
                .child(div().w(px(80.)))
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child("Neko"),
                )
                .child(
                    div()
                        .w(px(80.))
                        .text_align(gpui::TextAlign::Right)
                        .text_size(px(12.))
                        .text_color(gpui::rgb(0x92949a))
                        .child("Setup · 3 of 3"),
                )
        } else {
            div()
                .h(px(52.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .window_control_area(gpui::WindowControlArea::Drag)
                .child(progress)
        };
        div()
            .id("neko-setup")
            .tab_group()
            .key_context("Onboarding")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .size_full()
            .bg(if stage == SetupStep::Import {
                gpui::rgb(0x1c1d20)
            } else {
                theme::active().surface_panel
            })
            .text_color(theme::active().text_primary)
            .child(header)
            .child(body)
            .when(stage != SetupStep::Welcome, |view| view.child(footer))
            .into_any_element()
    }
}

fn note(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_size(px(13.))
        .line_height(px(20.))
        .text_color(theme::active().text_secondary)
        .child(text.into())
}
fn field(label: &str, input: &Entity<TextField>) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(7.))
        .child(note(label.to_owned()))
        .child(
            div()
                .h(px(40.))
                .px(px(12.))
                .flex()
                .items_center()
                .rounded(px(8.))
                .border_1()
                .border_color(theme::active().border_hairline_strong)
                .bg(theme::active().surface_input)
                .child(input.clone()),
        )
}
fn button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    enabled: bool,
    cx: &mut Context<OnboardingRoot>,
    action: impl Fn(&mut OnboardingRoot, &mut Window, &mut Context<OnboardingRoot>) + 'static,
) -> impl IntoElement {
    let action = Rc::new(action);
    let keyboard = action.clone();
    div()
        .id(id.into())
        .tab_index(0)
        .tab_stop(enabled)
        .px(px(14.))
        .py(px(10.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme::active().border_hairline)
        .bg(theme::active().surface_input)
        .text_size(px(13.))
        .text_color(if enabled {
            theme::active().text_primary
        } else {
            theme::active().text_secondary
        })
        .focus_visible(|s| s.border_color(theme::active().text_primary))
        .when(enabled, |b| {
            b.cursor_pointer()
                .hover(|s| s.bg(theme::active().surface_selected))
                .on_click(cx.listener(move |r, _, w, cx| action(r, w, cx)))
                .on_key_down(cx.listener(move |r, e: &KeyDownEvent, w, cx| {
                    if matches!(r.flow.hotkey_recording, RecordingState::Recording) {
                        r.on_key_down(e, w, cx);
                        cx.stop_propagation();
                        return;
                    }
                    if e.keystroke.modifiers == gpui::Modifiers::default()
                        && matches!(e.keystroke.key.as_str(), "enter" | "space")
                    {
                        keyboard(r, w, cx);
                        cx.stop_propagation();
                    }
                }))
        })
        .child(label.into())
}

fn import_item_button(
    id: impl Into<SharedString>,
    name: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    status: impl Into<SharedString>,
    enabled: bool,
    cx: &mut Context<OnboardingRoot>,
    action: impl Fn(&mut OnboardingRoot, &mut Window, &mut Context<OnboardingRoot>) + 'static,
) -> impl IntoElement {
    let action = Rc::new(action);
    let keyboard = action.clone();
    let status: SharedString = status.into();
    let selected = status.as_ref() == "Selected";
    let already_present = status.as_ref() == "Already in Neko";
    div()
        .id(id.into())
        .tab_index(0)
        .tab_stop(enabled)
        .px(px(16.))
        .py(px(12.))
        .rounded(px(9.))
        .border_1()
        .border_color(gpui::rgb(0x44464d))
        .bg(gpui::rgb(0x27292e))
        .focus_visible(|style| style.border_color(gpui::rgb(0xcbd5e8)))
        .when(enabled, |row| {
            row.cursor_pointer()
                .hover(|style| style.bg(gpui::rgb(0x303238)))
                .on_click(cx.listener(move |root, _, window, cx| action(root, window, cx)))
                .on_key_down(cx.listener(move |root, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.modifiers == gpui::Modifiers::default()
                        && matches!(event.keystroke.key.as_str(), "enter" | "space")
                    {
                        keyboard(root, window, cx);
                        cx.stop_propagation();
                    }
                }))
        })
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .gap(px(12.))
                .child(
                    div()
                        .size(px(18.))
                        .flex_shrink_0()
                        .rounded(px(5.))
                        .border_1()
                        .border_color(if selected {
                            gpui::rgb(0xcbd5e8)
                        } else {
                            gpui::rgb(0x777b84)
                        })
                        .bg(if selected {
                            gpui::rgb(0xcbd5e8)
                        } else {
                            gpui::rgb(0x27292e)
                        })
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.))
                        .text_color(gpui::rgb(0x1c1d20))
                        .child(if selected { "✓" } else { "" }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme::active().text_primary)
                                .child(name.into()),
                        )
                        .child(note(detail)),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .rounded(px(6.))
                        .px(px(8.))
                        .py(px(4.))
                        .bg(if already_present {
                            gpui::rgb(0x383a40)
                        } else {
                            gpui::rgb(0x2f3239)
                        })
                        .text_size(px(11.))
                        .text_color(if already_present {
                            gpui::rgb(0xb3b6be)
                        } else {
                            gpui::rgb(0xcbd5e8)
                        })
                        .child(status),
                ),
        )
}

fn source_tile(
    id: String,
    name: String,
    width: f32,
    height: f32,
    enabled: bool,
    cx: &mut Context<OnboardingRoot>,
    action: impl Fn(&mut OnboardingRoot, &mut Window, &mut Context<OnboardingRoot>) + 'static,
) -> impl IntoElement {
    let (symbol, description, tint) = match id.as_str() {
        "codex" => ("C", "Local agents, skills and connections", 0xe8e9e6),
        "claude" => ("✳", "Local skills, tools and project setup", 0xc7b5e7),
        "paseo" => ("P", "Agent projects and saved workflows", 0xd2d8e6),
        _ => ("+", "Local skills and workspace setup", 0xd4d7dc),
    };
    let action = Rc::new(action);
    let keyboard = action.clone();
    div()
        .id(format!("setup-source-{id}"))
        .tab_index(0)
        .tab_stop(enabled)
        .w(px(width))
        .h(px(height))
        .p(px(if height < 160. { 16. } else { 22. }))
        .flex()
        .flex_col()
        .justify_between()
        .rounded(px(13.))
        .border_1()
        .border_color(gpui::rgb(0x44464d))
        .bg(gpui::rgb(0x27292e))
        .focus_visible(|style| style.border_color(gpui::rgb(0xb6c5e3)))
        .when(enabled, |tile| {
            tile.cursor_pointer()
                .hover(|style| style.bg(gpui::rgb(0x303238)))
                .on_click(cx.listener(move |root, _, window, cx| action(root, window, cx)))
                .on_key_down(cx.listener(move |root, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.modifiers == gpui::Modifiers::default()
                        && matches!(event.keystroke.key.as_str(), "enter" | "space")
                    {
                        keyboard(root, window, cx);
                        cx.stop_propagation();
                    }
                }))
        })
        .child(
            div()
                .flex()
                .justify_between()
                .items_start()
                .child(
                    div()
                        .size(px(if height < 160. { 32. } else { 42. }))
                        .rounded(px(10.))
                        .bg(gpui::rgb(tint))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(gpui::rgb(0x161719))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_size(px(if height < 160. { 17. } else { 22. }))
                        .child(symbol),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(gpui::rgb(0x9ca0a9))
                        .child("Installed"),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_size(px(if height < 160. { 17. } else { 20. }))
                        .child(name.clone()),
                )
                .child(
                    div()
                        .text_size(px(if height < 160. { 11. } else { 13. }))
                        .text_color(gpui::rgb(0xa5a9b2))
                        .child(description),
                ),
        )
        .child(
            div()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_size(px(if height < 160. { 11. } else { 13. }))
                .text_color(gpui::rgb(0xcedaf3))
                .child(format!("Look through {name} →")),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workspace_review_hides_generated_folders_until_expanded() {
        let workspace = |id: &str, group: &str| neko_protocol::setup_import::ImportCandidate {
            id: id.into(),
            kind: ImportCandidateKind::Workspace,
            source: "local".into(),
            scope: format!("workspace:/{id}"),
            workspace: Some(format!("/{id}")),
            name: id.into(),
            metadata: std::collections::BTreeMap::from([("review_group".into(), group.into())]),
            problem: None,
        };
        let candidates = vec![
            workspace("scratch", "other"),
            workspace("project", "primary"),
        ];
        assert_eq!(
            workspace_review_rows(&candidates, false)
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["project"]
        );
        assert_eq!(
            workspace_review_rows(&candidates, true)
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["project", "scratch"]
        );
    }
    #[test]
    fn delayed_clipboard_ack_rejects_duplicate_activation() {
        let mut busy = false;
        assert!(!begin_clipboard_change(&mut busy, false));
        assert!(!busy);
        assert!(begin_clipboard_change(&mut busy, true));
        assert!(!begin_clipboard_change(&mut busy, true));
        assert!(busy);
    }
    #[test]
    fn another_workspace_never_inherits_the_displayed_brief() {
        assert!(brief_in_scope(Some("a"), Some("a")));
        assert!(!brief_in_scope(Some("a"), Some("b")));
        assert!(!brief_in_scope(None, Some("b")));
    }
    #[test]
    fn leaving_mac_cancels_recording() {
        assert_eq!(
            recording_after_navigation(SetupStep::Welcome, RecordingState::Recording),
            RecordingState::Idle
        );
        assert_eq!(
            recording_after_navigation(SetupStep::Mac, RecordingState::Recording),
            RecordingState::Recording
        );
    }
    #[test]
    fn evidence_steps_are_one_based_and_invalid_values_start_at_welcome() {
        assert_eq!(evidence_step(Some("1")), SetupStep::Welcome);
        assert_eq!(evidence_step(Some("3")), SetupStep::Import);
        for value in [None, Some("0"), Some("4"), Some("6"), Some("oops")] {
            assert_eq!(evidence_step(value), SetupStep::Welcome);
        }
    }
    #[test]
    fn first_run_has_only_three_visible_steps() {
        assert_eq!(SetupStep::ALL.len(), 3);
        assert_eq!(SetupStep::Mac.next(), SetupStep::Import);
        assert_eq!(SetupStep::Import.next(), SetupStep::Import);
    }
    #[test]
    fn three_steps_have_bounded_back_and_next() {
        let mut step = SetupStep::Welcome;
        assert_eq!(step.back(), SetupStep::Welcome);
        for expected in [SetupStep::Mac, SetupStep::Import] {
            step = step.next();
            assert_eq!(step, expected);
        }
        assert_eq!(step.next(), SetupStep::Import);
        assert_eq!(step.back(), SetupStep::Mac);
    }
    #[test]
    fn choosing_an_existing_folder_reuses_its_workspace() {
        let folder = std::env::temp_dir().canonicalize().unwrap();
        let existing = Workspace {
            id: "existing".into(),
            name: "Existing".into(),
            repository: folder.to_string_lossy().into_owned(),
            instructions: String::new(),
            away_enabled: false,
        };
        assert!(matches!(
            folder_choice(&folder, &[existing]),
            Ok(FolderChoice::Existing(id)) if id == "existing"
        ));
    }
    #[test]
    fn choosing_a_new_folder_registers_only_a_workspace() {
        let folder = std::env::temp_dir().canonicalize().unwrap();
        let result = folder_choice(&folder, &[]).unwrap();
        let FolderChoice::New(workspace) = result else {
            panic!("expected a new workspace");
        };
        assert!(workspace.id.is_empty());
        assert_eq!(workspace.repository, folder.to_string_lossy());
        assert!(workspace.instructions.is_empty());
        assert!(!workspace.away_enabled);
    }
    #[test]
    fn choosing_a_file_does_not_register_a_workspace() {
        let file = std::env::current_exe().unwrap();
        assert!(folder_choice(&file, &[]).is_err());
    }
    #[test]
    fn import_discovery_starts_once_on_entry_but_can_retry_after_error() {
        assert!(should_start_import_discovery(
            SetupStep::Import,
            false,
            false
        ));
        assert!(!should_start_import_discovery(
            SetupStep::Import,
            true,
            false
        ));
        assert!(!should_start_import_discovery(
            SetupStep::Import,
            false,
            true
        ));
        assert!(should_start_import_discovery(
            SetupStep::Import,
            false,
            false
        ));
    }
    #[test]
    fn import_discovery_does_not_start_for_other_steps() {
        assert!(!should_start_import_discovery(
            SetupStep::Tools,
            false,
            false
        ));
    }
    #[test]
    fn busy_import_discovery_does_not_mark_started() {
        let mut started = false;
        assert!(!begin_import_discovery(true, &mut started, false));
        assert!(!started);
        assert!(begin_import_discovery(false, &mut started, false));
        assert!(!begin_import_discovery(false, &mut started, false));
        assert!(begin_import_discovery(false, &mut started, true));
    }
    #[test]
    fn legacy_import_rows_are_only_used_without_candidates() {
        assert!(uses_legacy_import_rows(0));
        assert!(!uses_legacy_import_rows(1));
    }
    #[test]
    fn every_discovery_failure_allows_refresh() {
        let mut started = true;
        reset_import_discovery_after_failure(true, &mut started);
        assert!(!started);
        let mut unrelated = true;
        reset_import_discovery_after_failure(false, &mut unrelated);
        assert!(unrelated);
    }
    #[test]
    fn credential_marker_requires_explicit_true_metadata() {
        let mut metadata = std::collections::BTreeMap::new();
        assert!(!candidate_has_credentials(&metadata));
        metadata.insert("has_credentials".into(), "false".into());
        assert!(!candidate_has_credentials(&metadata));
        metadata.insert("has_credentials".into(), "true".into());
        assert!(candidate_has_credentials(&metadata));
    }
    #[test]
    fn import_rows_name_mcp_servers_instead_of_internal_connections() {
        use neko_protocol::setup_import::ImportCandidateKind;
        assert_eq!(
            import_candidate_label(&ImportCandidateKind::Connection),
            "MCP server"
        );
        assert_eq!(import_candidate_label(&ImportCandidateKind::Skill), "Skill");
    }
    #[test]
    fn import_source_groups_are_lexical_and_keep_combined_sources_together() {
        use neko_protocol::setup_import::{ImportCandidate, ImportCandidateKind};
        let candidate = |id: &str, source: &str, scope: Option<&str>, name: &str| ImportCandidate {
            id: id.into(),
            kind: ImportCandidateKind::Connection,
            source: source.into(),
            scope: scope.map_or_else(|| "global".into(), |path| format!("workspace:{path}")),
            workspace: scope.map(str::to_owned),
            name: name.into(),
            metadata: Default::default(),
            problem: None,
        };
        let candidates = vec![
            candidate("z", "Codex", Some("/z"), "zeta"),
            candidate("shared", "Claude + Codex", None, "shared"),
            candidate("b", "Codex", None, "beta"),
            candidate("a", "Codex", None, "alpha"),
            candidate("claude", "Claude", None, "only Claude"),
        ];
        let groups = import_source_groups(&candidates);
        assert_eq!(
            groups.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["Claude", "Claude + Codex", "Codex"]
        );
        assert_eq!(
            groups["Codex"]
                .iter()
                .map(|candidate| candidate.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "z"]
        );
        assert_eq!(groups["Claude + Codex"].len(), 1);
    }
    #[test]
    fn scoped_connections_wait_for_workspace_but_schedules_with_warnings_remain_selectable() {
        use neko_protocol::setup_import::ImportCandidateKind;
        assert!(candidate_is_unavailable(
            &ImportCandidateKind::Connection,
            None,
            Some("/repo"),
            false,
            false
        ));
        assert!(!candidate_is_unavailable(
            &ImportCandidateKind::Connection,
            None,
            Some("/repo"),
            true,
            false
        ));
        assert!(candidate_is_unavailable(
            &ImportCandidateKind::Skill,
            None,
            Some("/repo"),
            false,
            false
        ));
        assert!(!candidate_is_unavailable(
            &ImportCandidateKind::Skill,
            None,
            Some("/repo"),
            true,
            false
        ));
        assert!(!candidate_is_unavailable(
            &ImportCandidateKind::Schedule,
            Some("warning"),
            None,
            false,
            false
        ));
        assert!(workspace_scope_selected(Some("/repo"), false, true));
    }

    #[test]
    fn explicitly_global_fallback_makes_only_a_scoped_connection_selectable() {
        use neko_protocol::setup_import::ImportCandidateKind;
        assert!(!candidate_is_unavailable(
            &ImportCandidateKind::Connection,
            None,
            Some("/missing"),
            false,
            true,
        ));
        assert!(candidate_is_unavailable(
            &ImportCandidateKind::Skill,
            None,
            Some("/missing"),
            false,
            true,
        ));
    }

    #[test]
    fn missing_workspace_is_not_reported_as_a_broken_connection() {
        assert_eq!(
            candidate_review_badge(false, false, true, false, false),
            "Add workspace first"
        );
        assert_eq!(
            candidate_review_badge(false, true, false, false, false),
            "Review setup"
        );
    }

    #[test]
    fn discovery_requires_individual_selection() {
        use neko_protocol::setup_import::{ImportCandidate, ImportCandidateKind, ImportPreview};
        let preview = ImportPreview {
            candidates: vec![
                ImportCandidate {
                    id: "workspace".into(),
                    kind: ImportCandidateKind::Workspace,
                    source: "Codex".into(),
                    scope: "workspace:/repo".into(),
                    workspace: Some("/repo".into()),
                    name: "repo".into(),
                    metadata: Default::default(),
                    problem: None,
                },
                ImportCandidate {
                    id: "connection".into(),
                    kind: ImportCandidateKind::Connection,
                    source: "Claude".into(),
                    scope: "workspace:/repo".into(),
                    workspace: Some("/repo".into()),
                    name: "Figma".into(),
                    metadata: Default::default(),
                    problem: None,
                },
                ImportCandidate {
                    id: "skill".into(),
                    kind: ImportCandidateKind::Skill,
                    source: "Codex".into(),
                    scope: "global".into(),
                    workspace: None,
                    name: "Review".into(),
                    metadata: Default::default(),
                    problem: None,
                },
                ImportCandidate {
                    id: "unavailable".into(),
                    kind: ImportCandidateKind::Connection,
                    source: "Claude".into(),
                    scope: "global".into(),
                    workspace: None,
                    name: "Broken".into(),
                    metadata: Default::default(),
                    problem: Some("Invalid config".into()),
                },
            ],
            ..Default::default()
        };

        let selection = default_import_selection(&preview);
        assert!(selection.workspaces.is_empty());
        assert!(selection.connections.is_empty());
        assert!(selection.skills.is_empty());
        assert!(!selection.connections.contains("unavailable"));
        assert!(selection.schedules.is_empty());
    }
}
