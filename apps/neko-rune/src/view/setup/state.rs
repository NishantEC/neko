//! What the Setup pages keep between frames and what pressing their controls does: the section, the
//! open dialog, which rows are open, the registry search. No drawing and no I/O here: every request goes
//! out through [`App::call`], so each rule is tested by looking at what was asked of the daemon.
//!
//! The rules are the web UI's, ported from `pages/setup/{Workspaces,Watching,Tools,Memory,Profiles,
//! Schedules}.tsx` and `tools/{ConnectionDetail,AddConnection,Skills}.tsx`.

use std::collections::HashSet;

use neko_protocol::agent_profiles::AgentProfile;
use neko_protocol::mcp_host::{Responsibility, ServerConfig};
use neko_protocol::scheduled_plans::Schedule;
use neko_protocol::workbench::{MemoryEntry, MemoryKind, MemoryOptions, Workspace};

use super::words::{self, ConnectionForm, Recurrence, RegistryServer, Reconnect};
use crate::app::{App, Dispatch};
use crate::link::{Call, Hidden, SetupAction, SetupData};
use crate::views::FullView;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SetupSection {
    Workspaces,
    Watching,
    Tools,
    Skills,
    Memory,
    Profiles,
    Schedules,
}

impl SetupSection {
    /// The rail's order.
    pub const ALL: [SetupSection; 7] = [SetupSection::Workspaces, SetupSection::Watching, SetupSection::Tools, SetupSection::Skills, SetupSection::Memory, SetupSection::Profiles, SetupSection::Schedules];

    pub fn title(self) -> &'static str {
        match self {
            SetupSection::Workspaces => "Workspaces",
            SetupSection::Watching => "Watching",
            SetupSection::Tools => "Tools",
            SetupSection::Skills => "Skills",
            SetupSection::Memory => "Memory",
            SetupSection::Profiles => "Profiles",
            SetupSection::Schedules => "Schedules",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            SetupSection::Workspaces => "workspaces",
            SetupSection::Watching => "watching",
            SetupSection::Tools => "tools",
            SetupSection::Skills => "skills",
            SetupSection::Memory => "memory",
            SetupSection::Profiles => "profiles",
            SetupSection::Schedules => "schedules",
        }
    }

    pub fn from_key(key: &str) -> Option<SetupSection> {
        SetupSection::ALL.into_iter().find(|s| s.key() == key)
    }

    /// The web UI's page for the same section ("Open in the web UI").
    pub fn web_id(self) -> &'static str {
        match self {
            // The web keeps skills with the tools.
            SetupSection::Skills => "tools",
            other => other.key(),
        }
    }
}

/// How a connection is added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddTab {
    Browse,
    Import,
    Manual,
}

/// A confirmation: what is about to happen, the button that does it (coral when it destroys something).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirm {
    pub title: String,
    pub description: String,
    pub button: String,
    pub coral: bool,
    pub calls: Vec<SetupAction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    /// Create or edit a workspace: its name, folders and instructions (fields `dlg:name`, `dlg:path`, `dlg:text`).
    Workspace { id: Option<String>, folders: Vec<String> },
    /// New or edited watch (field `dlg:text`).
    Watch { id: Option<String>, workspace: String, connections: Vec<String>, enabled: bool, prepare: bool },
    /// Rename a watch (field `dlg:name`; empty clears it).
    RenameWatch { id: String },
    /// Add a connection: browse the registry, import from this Mac, or by hand.
    Connection { tab: AddTab, workspace: String, form: ConnectionForm, selected: Option<RegistryServer>, expanded: Option<String>, added: Option<String> },
    Schedule { id: Option<String>, workspace: String, kind: Recurrence, weekday: String },
    Profile { id: Option<String> },
    Memory { id: Option<String>, kind: MemoryKind, workspace: String },
    Preference { id: Option<String>, workspace: String },
    Correct { record_id: String, workspace_id: String, version: u32 },
    Confirm(Confirm),
}

/// The public registry's search.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Registry {
    pub results: Vec<RegistryServer>,
    pub searching: bool,
    pub error: Option<String>,
    /// The query the answer in `results` belongs to (a slow answer for an older query is dropped).
    pub query: String,
}

/// Asks the public registry; a function so tests and pictures never touch the network.
pub type Fetch = fn(&str) -> Result<String, String>;

pub struct SetupState {
    pub section: SetupSection,
    pub dialog: Option<Dialog>,
    /// Rows opened to show their details, by key.
    pub open: HashSet<String>,
    /// The connection whose detail shows inside Tools.
    pub connection: Option<String>,
    /// The workspace whose grants Tools shows for a global connection, and whose skills Skills shows.
    pub grant_workspace: Option<String>,
    pub skills_workspace: Option<String>,
    /// Memory shows this workspace's memories ("" is everything).
    pub scope: String,
    pub registry: Registry,
    pub fetch: Fetch,
    /// "Suggest watches" was asked at this time for this workspace: Neko's reply is shown under the list.
    pub asked: Option<(i64, String)>,
    /// What a confirmation still has to send, each once the one before it was answered.
    pub queue: Vec<SetupAction>,
    /// Skill proposals whose published audit link was opened here (the web enables the confirm button only after).
    pub audit_opened: HashSet<String>,
    /// Which watch groups show their paused watches.
    pub dismissed_reply: bool,
}

impl Default for SetupState {
    fn default() -> Self {
        SetupState {
            section: SetupSection::Workspaces,
            dialog: None,
            open: HashSet::new(),
            connection: None,
            grant_workspace: None,
            skills_workspace: None,
            scope: String::new(),
            registry: Registry::default(),
            fetch: curl_registry,
            asked: None,
            queue: Vec::new(),
            audit_opened: HashSet::new(),
            dismissed_reply: false,
        }
    }
}

/// The public registry through the system's `curl` (every Mac has it), ten seconds at most.
pub fn curl_registry(query: &str) -> Result<String, String> {
    let out = std::process::Command::new("/usr/bin/curl")
        .args(["-sS", "--max-time", "10", "-G", "--data-urlencode"])
        .arg(format!("search={query}"))
        .args(["--data-urlencode", "version=latest", "--data-urlencode", "limit=30", words::REGISTRY])
        .output()
        .map_err(|_| "This app can’t reach the public registry. Add a server URL manually.".to_owned())?;
    if !out.status.success() {
        return Err("The registry is unavailable. Try again or add a server URL manually.".to_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The time zone the Mac is set to (`America/New_York`), the Schedules editor's default.
pub fn local_zone() -> String {
    if let Ok(tz) = std::env::var("TZ")
        && !tz.is_empty()
    {
        return tz.trim_start_matches(':').to_owned();
    }
    std::fs::read_link("/etc/localtime").ok().and_then(|p| p.to_string_lossy().split("zoneinfo/").nth(1).map(str::to_owned)).unwrap_or_else(|| "UTC".to_owned())
}

/// Fields the dialogs share.
pub const F_NAME: &str = "dlg:name";
pub const F_TEXT: &str = "dlg:text";
pub const F_PATH: &str = "dlg:path";

impl App {
    pub fn setup_data(&self) -> Option<&SetupData> {
        self.summary.as_ref().map(|m| &m.setup)
    }

    pub fn setup_call(&mut self, call: SetupAction) {
        self.call(Call::Setup(Box::new(call)));
    }

    // ---- sections and rows

    /// Opens Setup on a section (a deep link from elsewhere).
    pub fn open_setup(&mut self, section: SetupSection) {
        self.setup.section = section;
        self.setup.dialog = None;
        self.setup.connection = None;
        self.pages.open_select = None;
        self.pages.notes.retain(|k, _| !k.starts_with("setup:"));
        if self.view == Some(FullView::Setup) {
            self.pages.asked_ms = i64::MIN / 2;
            self.refresh_page(FullView::Setup);
        } else {
            self.open_view(FullView::Setup);
        }
    }

    /// "Open connection": Tools, with this connection's detail.
    pub fn open_connection(&mut self, id: &str) {
        self.open_setup(SetupSection::Tools);
        self.setup.connection = Some(id.to_owned());
    }

    pub fn toggle_row(&mut self, key: &str) {
        if !self.setup.open.remove(key) {
            self.setup.open.insert(key.to_owned());
        }
    }

    pub fn row_open(&self, key: &str) -> bool {
        self.setup.open.contains(key)
    }

    pub fn close_dialog(&mut self) {
        self.setup.dialog = None;
        self.setup.queue.clear();
        self.pages.open_select = None;
        self.pages.notes.remove("setup:form");
    }

    fn seed_dialog(&mut self, name: &str, text: &str, path: &str) {
        self.pages.set_field(F_NAME, name);
        self.pages.set_field(F_TEXT, text);
        self.pages.set_field(F_PATH, path);
        self.pages.notes.remove("setup:form");
        self.pages.open_select = None;
    }

    /// A refusal or what is wrong with the form, shown in the dialog.
    fn form_note(&mut self, words: &str) {
        self.pages.notes.insert("setup:form".to_owned(), words.to_owned());
    }

    /// Runs when the daemon took a setup request: a form closes; a confirmation sends its next request, or closes.
    pub fn setup_applied(&mut self, call: &SetupAction) {
        if matches!(self.setup.dialog, Some(Dialog::Confirm(_))) {
            if self.setup.queue.is_empty() {
                self.close_dialog();
            } else {
                let next = self.setup.queue.remove(0);
                self.setup_call(next);
            }
            return;
        }
        if matches!((&self.setup.dialog, call), (Some(Dialog::RenameWatch { .. }), SetupAction::RenameWatch { .. }) | (Some(Dialog::Correct { .. }), SetupAction::CorrectDecision { .. })) {
            self.close_dialog();
            return;
        }
        if call.key() == "setup:form" {
            // A connection added from the registry stays open to say so, as the web's dialog does.
            let stay = matches!(call, SetupAction::AddConnection { .. }) && matches!(self.setup.dialog, Some(Dialog::Connection { tab: AddTab::Browse, .. }));
            if stay {
                if let (Some(Dialog::Connection { added, .. }), SetupAction::AddConnection { label, .. }) = (self.setup.dialog.as_mut(), call) {
                    *added = Some(label.clone());
                }
            } else {
                self.close_dialog();
            }
        }
    }

    // ---- workspaces

    pub fn open_workspace_dialog(&mut self, id: Option<&str>) {
        let Some(data) = self.setup_data() else { return };
        let (name, text, folders) = match id.and_then(|i| data.workspace(i)) {
            Some(w) => (w.name.clone(), w.instructions.clone(), data.folders_of(w)),
            None => (String::new(), String::new(), Vec::new()),
        };
        self.seed_dialog(&name, &text, "");
        self.setup.dialog = Some(Dialog::Workspace { id: id.map(str::to_owned), folders });
    }

    /// "Add folder": the typed path joins the list, and names an unnamed workspace after its folder.
    pub fn workspace_add_folder(&mut self) {
        let path = self.pages.text(F_PATH).trim().to_owned();
        if path.is_empty() {
            return;
        }
        if let Some(Dialog::Workspace { folders, .. }) = self.setup.dialog.as_mut()
            && !folders.contains(&path)
        {
            folders.push(path.clone());
        }
        if self.pages.text(F_NAME).trim().is_empty() {
            let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_owned();
            self.pages.set_field(F_NAME, &name);
        }
        self.pages.set_field(F_PATH, "");
    }

    pub fn workspace_remove_folder(&mut self, path: &str) {
        if let Some(Dialog::Workspace { folders, .. }) = self.setup.dialog.as_mut() {
            folders.retain(|f| f != path);
        }
    }

    pub fn save_workspace(&mut self) {
        let Some(Dialog::Workspace { id, folders }) = self.setup.dialog.clone() else { return };
        let pending = self.pages.text(F_PATH).trim().to_owned();
        let mut all = folders;
        if !pending.is_empty() && !all.contains(&pending) {
            all.push(pending);
        }
        let name = self.pages.text(F_NAME).trim().to_owned();
        if name.is_empty() || all.is_empty() {
            self.form_note(if name.is_empty() { "Give the workspace a name." } else { "Add at least one folder where this work lives." });
            return;
        }
        let away = id.as_deref().and_then(|i| self.setup_data().and_then(|d| d.workspace(i))).is_some_and(|w| w.away_enabled);
        let workspace = Workspace { id: id.unwrap_or_default(), name, repository: all[0].clone(), instructions: self.pages.text(F_TEXT).to_owned(), away_enabled: away };
        self.pages.notes.remove("setup:form");
        self.setup_call(SetupAction::SaveWorkspace { workspace, folders: all });
    }

    /// Wakes the active watches of a workspace, one at a time (a paused or suggested one is refused by the daemon).
    pub fn sync_workspace(&mut self, id: &str) {
        let ids: Vec<String> = self.setup_data().map(|d| d.watches.iter().filter(|w| w.workspace_id == id && w.enabled).map(|w| w.id.clone()).collect()).unwrap_or_default();
        if ids.is_empty() {
            self.pages.notes.insert(format!("setup:{id}"), "No active watches to sync here.".to_owned());
            return;
        }
        self.pages.notes.remove(&format!("setup:{id}"));
        for watch in ids {
            self.setup_call(SetupAction::Wake { id: watch });
        }
        self.show_toast("Asked this workspace’s watches to check now.");
    }

    // ---- watching

    pub fn open_watch_dialog(&mut self, id: Option<&str>, workspace: &str) {
        let Some(data) = self.setup_data() else { return };
        let (text, connections, enabled, prepare, ws) = match id.and_then(|i| data.watch(i)) {
            Some(w) => (w.instruction.clone(), w.connection_ids.clone(), w.enabled, w.prepare_low_risk, w.workspace_id.clone()),
            None => (String::new(), Vec::new(), false, false, workspace.to_owned()),
        };
        self.seed_dialog("", &text, "");
        self.setup.dialog = Some(Dialog::Watch { id: id.map(str::to_owned), workspace: ws, connections, enabled, prepare });
    }

    pub fn open_rename_watch(&mut self, id: &str) {
        let name = self.setup_data().and_then(|d| d.watch(id)).and_then(|w| w.name.clone()).unwrap_or_default();
        self.seed_dialog(&name, "", "");
        self.setup.dialog = Some(Dialog::RenameWatch { id: id.to_owned() });
    }

    pub fn rename_watch(&mut self) {
        let Some(Dialog::RenameWatch { id }) = self.setup.dialog.clone() else { return };
        let name = self.pages.text(F_NAME).trim().to_owned();
        self.setup_call(SetupAction::RenameWatch { id, name });
    }

    pub fn watch_toggle_connection(&mut self, connection: &str) {
        if let Some(Dialog::Watch { connections, .. }) = self.setup.dialog.as_mut() {
            if connections.iter().any(|c| c == connection) {
                connections.retain(|c| c != connection);
            } else {
                connections.push(connection.to_owned());
            }
        }
    }

    pub fn watch_set_workspace(&mut self, workspace: &str) {
        if let Some(Dialog::Watch { workspace: ws, connections, .. }) = self.setup.dialog.as_mut() {
            *ws = workspace.to_owned();
            connections.clear();
        }
    }

    /// What a watch form would save, or what is wrong with it.
    pub fn watch_from_form(&self, now_ms: i64) -> Result<Responsibility, &'static str> {
        let Some(Dialog::Watch { id, workspace, connections, enabled, prepare }) = self.setup.dialog.as_ref() else { return Err("") };
        let instruction = self.pages.text(F_TEXT).trim().to_owned();
        if instruction.is_empty() {
            return Err("Say what to watch.");
        }
        if connections.is_empty() {
            return Err("Choose at least one connected tool.");
        }
        let data = self.setup_data().ok_or("Neko isn't running.")?;
        if *enabled && !words::ungranted(data, workspace, connections).is_empty() {
            return Err("Choose access for the tools that cannot be used yet in Tools before turning this on.");
        }
        let mut sorted = connections.clone();
        sorted.sort();
        let base = id.as_deref().and_then(|i| data.watch(i)).cloned();
        Ok(Responsibility {
            id: id.clone().unwrap_or_default(),
            workspace_id: workspace.clone(),
            instruction,
            connection_ids: sorted,
            enabled: *enabled,
            prepare_low_risk: *prepare,
            next_due_ms: base.as_ref().map_or(now_ms, |b| b.next_due_ms),
            last_attempt_ms: base.as_ref().and_then(|b| b.last_attempt_ms),
            last_result: base.as_ref().map(|b| b.last_result.clone()).unwrap_or_default(),
            failures: base.as_ref().map_or(0, |b| b.failures),
            name: base.as_ref().and_then(|b| b.name.clone()),
            name_by_user: base.as_ref().is_some_and(|b| b.name_by_user),
        })
    }

    pub fn save_watch(&mut self) {
        match self.watch_from_form(self.now_ms) {
            Ok(w) => {
                self.pages.notes.remove("setup:form");
                self.setup_call(SetupAction::SaveWatch(Box::new(w)));
            }
            Err(why) => self.form_note(why),
        }
    }

    /// The activation switch: off pauses at once; on first needs the tools to be usable (else Tools opens), then a yes.
    pub fn watch_switch(&mut self, id: &str, on: bool) {
        let Some(w) = self.setup_data().and_then(|d| d.watch(id)).cloned() else { return };
        if !on {
            self.setup_call(SetupAction::SaveWatch(Box::new(Responsibility { enabled: false, ..w })));
            return;
        }
        let missing = self.setup_data().map(|d| words::ungranted(d, &w.workspace_id, &w.connection_ids)).unwrap_or_default();
        if missing.is_empty() {
            self.turn_on_watch_dialog(id);
        } else {
            self.open_setup(SetupSection::Tools);
            self.pages.notes.insert("setup:page".to_owned(), format!("Connect or discover tools for {} to turn on the watch.", missing.join(", ")));
        }
    }

    fn turn_on_watch_dialog(&mut self, id: &str) {
        let Some(data) = self.setup_data() else { return };
        let Some(w) = data.watch(id).cloned() else { return };
        let sources = w.connection_ids.iter().map(|c| words::connection_label(data, c)).collect::<Vec<_>>().join(", ");
        let description = format!("{}\n\nAllows unattended checks every 10 minutes using {} in {}. Connected tools may read or change data.", w.instruction, if sources.is_empty() { "the selected connections".to_owned() } else { sources }, data.workspace_name(&w.workspace_id));
        let wake = SetupAction::Wake { id: w.id.clone() };
        self.setup.dialog = Some(Dialog::Confirm(Confirm { title: "Turn on this watch?".to_owned(), description, button: "Turn on watch".to_owned(), coral: false, calls: vec![SetupAction::SaveWatch(Box::new(Responsibility { enabled: true, ..w })), wake] }));
    }

    /// "Turn on…" from a suggestion or a menu.
    pub fn watch_turn_on(&mut self, id: &str) {
        self.watch_switch(id, true);
    }

    pub fn confirm_delete_watch(&mut self, id: &str) {
        let Some(w) = self.setup_data().and_then(|d| d.watch(id)).cloned() else { return };
        self.setup.dialog = Some(Dialog::Confirm(Confirm { title: format!("Delete “{}”?", words::watch_title(&w)), description: "Neko stops checking it. Tasks it already filed stay.".to_owned(), button: "Delete watch".to_owned(), coral: true, calls: vec![SetupAction::RemoveWatch { id: w.id }] }));
    }

    /// Asks Neko, in a workspace, to suggest watches from its connected tools (the web's flow).
    pub fn suggest_watches(&mut self, workspace: &str) {
        if workspace.is_empty() {
            return;
        }
        self.setup.asked = Some((self.now_ms, workspace.to_owned()));
        self.setup.dismissed_reply = false;
        self.setup_call(SetupAction::AskIn { text: words::WATCH_SUGGEST_PROMPT.to_owned(), workspace_id: workspace.to_owned() });
    }

    /// Neko's newest reply to the suggestion, in the workspace asked.
    pub fn suggestion_reply(&self) -> Option<(bool, String)> {
        let (at, workspace) = self.setup.asked.as_ref()?;
        let m = self.summary.as_ref()?.conversation.iter().rev().find(|m| matches!(m.role, neko_protocol::workbench::ChatRole::Neko) && m.at_ms >= at - 2000 && m.workspace_id.as_deref() == Some(workspace.as_str()))?;
        let text: String = m.parts.iter().filter(|p| p["type"] == "text").filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n");
        Some((m.pending, text))
    }

    // ---- tools

    /// What the sign-in or reconnect button of a connection does.
    pub fn reconnect(&mut self, id: &str) {
        let Some(c) = self.setup_data().and_then(|d| d.connection(id)).cloned() else { return };
        match words::reconnect_action(&c) {
            Some(Reconnect::SignIn) => {
                let client = self.pages.text("conn:client").trim().to_owned();
                self.setup_call(SetupAction::Authenticate { id: c.id.clone(), client_id: (!client.is_empty()).then_some(client) });
                self.show_toast(&format!("Finish signing in to {} in your browser.", c.label));
            }
            Some(Reconnect::Relink) => {
                let local = !matches!(c.config, ServerConfig::Http { .. });
                let what = match &c.config {
                    ServerConfig::Http { url } => url.clone(),
                    ServerConfig::Stdio { command, args, .. } => std::iter::once(command.clone()).chain(args.iter().cloned()).collect::<Vec<_>>().join(" "),
                };
                let source = c.source_link.as_ref().map(|l| l.source_path.clone()).unwrap_or_else(|| "its source file".to_owned());
                let candidate = c.source_link.as_ref().map(|l| l.candidate_id.clone()).unwrap_or_default();
                let mut description = format!("Its definition in {source} no longer matches what you approved. Neko will run it as it is now:\n{what}");
                if local {
                    description.push_str("\nThis is a program on this Mac. Approve it only if you recognise it.");
                }
                self.setup.dialog = Some(Dialog::Confirm(Confirm {
                    title: format!("Approve {} again?", c.label),
                    description,
                    button: "Approve again".to_owned(),
                    coral: false,
                    calls: vec![SetupAction::LinkSource { workspace_id: c.workspace_id.clone(), candidate_id: candidate, trust_local_process: local }, SetupAction::Refresh { id: c.id.clone() }],
                }));
            }
            Some(Reconnect::Discover) => self.setup_call(SetupAction::Refresh { id: c.id }),
            None => {}
        }
    }

    /// Pauses or enables a connection. Pausing one whose tools have been allowed revokes those grants (the daemon does),
    /// so it asks first.
    pub fn toggle_connection(&mut self, id: &str) {
        let Some(c) = self.setup_data().and_then(|d| d.connection(id)).cloned() else { return };
        let granted = self.setup_data().is_some_and(|d| d.grants.iter().any(|g| g.connection_id == id));
        if c.enabled && granted {
            self.setup.dialog = Some(Dialog::Confirm(Confirm {
                title: format!("Pause {}?", c.label),
                description: "Pausing revokes the unattended-use access you gave its tools. Watches and agents that use it stop until you allow it again.".to_owned(),
                button: "Pause & revoke access".to_owned(),
                coral: true,
                calls: vec![SetupAction::SetEnabled { id: c.id, enabled: false }],
            }));
        } else {
            self.setup_call(SetupAction::SetEnabled { id: c.id, enabled: !c.enabled });
        }
    }

    /// The workspace a global connection's grants are shown for: the one chosen, else the first.
    pub fn grant_workspace(&self) -> String {
        let data = self.setup_data();
        self.setup.grant_workspace.clone().filter(|w| data.is_some_and(|d| d.workspace(w).is_some())).or_else(|| data.and_then(|d| d.workspaces.first().map(|w| w.id.clone()))).unwrap_or_default()
    }

    /// Allows or revokes one tool for unattended use: `SetToolGrant` for a connection that belongs to a workspace,
    /// `SetWorkspaceToolGrant` for a global one, in the workspace shown.
    pub fn set_tool_grant(&mut self, connection_id: &str, tool: &str, schema_hash: &str, allowed: bool) {
        let Some(c) = self.setup_data().and_then(|d| d.connection(connection_id)).cloned() else { return };
        let workspace_id = if c.workspace_id.is_empty() { self.grant_workspace() } else { String::new() };
        self.setup_call(SetupAction::Grant { connection_id: c.id, workspace_id, tool: tool.to_owned(), schema_hash: schema_hash.to_owned(), allowed });
    }

    pub fn open_add_connection(&mut self, tab: AddTab) {
        let workspace = self.setup_data().and_then(|d| d.workspaces.first().map(|w| w.id.clone())).unwrap_or_default();
        for f in ["conn:label", "conn:target", "conn:args", "conn:cwd", "conn:credentials", "conn:search"] {
            self.pages.set_field(f, if f == "conn:args" { "[]" } else { "" });
        }
        self.pages.notes.remove("setup:form");
        self.setup.registry = Registry::default();
        self.setup.dialog = Some(Dialog::Connection { tab, workspace, form: ConnectionForm::default(), selected: None, expanded: None, added: None });
    }

    pub fn connection_dialog_tab(&mut self, tab: AddTab) {
        if let Some(Dialog::Connection { tab: t, .. }) = self.setup.dialog.as_mut() {
            *t = tab;
        }
        self.pages.notes.remove("setup:form");
        self.pages.open_select = None;
    }

    pub fn connection_dialog_workspace(&mut self, workspace: &str) {
        if let Some(Dialog::Connection { workspace: w, .. }) = self.setup.dialog.as_mut() {
            *w = workspace.to_owned();
        }
    }

    /// Anything that changes what would run clears the trust you gave (`edit` in the web's form).
    pub fn connection_form_changed(&mut self) {
        if let Some(Dialog::Connection { form, .. }) = self.setup.dialog.as_mut() {
            form.trust = false;
        }
    }

    pub fn connection_form_local(&mut self, local: bool) {
        if let Some(Dialog::Connection { form, .. }) = self.setup.dialog.as_mut() {
            form.local = local;
            form.trust = false;
        }
        self.pages.set_field("conn:target", "");
    }

    pub fn connection_form_trust(&mut self) {
        if let Some(Dialog::Connection { form, .. }) = self.setup.dialog.as_mut() {
            form.trust = !form.trust;
        }
    }

    pub fn add_connection_manually(&mut self) {
        let Some(Dialog::Connection { workspace, form, .. }) = self.setup.dialog.clone() else { return };
        let label = self.pages.text("conn:label").trim().to_owned();
        let form = ConnectionForm { target: self.pages.text("conn:target").to_owned(), args: self.pages.text("conn:args").to_owned(), cwd: self.pages.text("conn:cwd").to_owned(), ..form };
        if label.is_empty() || form.target.trim().is_empty() {
            self.form_note("Give the connection a name and an address.");
            return;
        }
        if form.local && !form.trust {
            self.form_note("Trust the executable and its arguments first, or use a hosted server.");
            return;
        }
        match words::config_for(&form) {
            Ok(config) => {
                let credentials = self.pages.text("conn:credentials").trim().to_owned();
                self.pages.notes.remove("setup:form");
                self.setup_call(SetupAction::AddConnection { workspace_id: workspace, label, config, trust_local_process: form.local && form.trust, credentials: (!credentials.is_empty()).then_some(Hidden(credentials)) });
            }
            Err(why) => self.form_note(&why),
        }
    }

    pub fn registry_select(&mut self, server: Option<RegistryServer>) {
        if let Some(Dialog::Connection { selected, added, .. }) = self.setup.dialog.as_mut() {
            *selected = server;
            *added = None;
        }
        self.pages.notes.remove("setup:form");
    }

    pub fn add_registry_server(&mut self, server: &RegistryServer) {
        let Some(Dialog::Connection { workspace, .. }) = self.setup.dialog.clone() else { return };
        let exists = self.setup_data().is_some_and(|d| d.connections.iter().any(|c| c.workspace_id == workspace && matches!(&c.config, ServerConfig::Http { url } if *url == server.url)));
        if exists {
            if let Some(Dialog::Connection { added, .. }) = self.setup.dialog.as_mut() {
                *added = Some(server.short_name.clone());
            }
            return;
        }
        self.setup_call(SetupAction::AddConnection { workspace_id: workspace, label: server.short_name.clone(), config: ServerConfig::Http { url: server.url.clone() }, trust_local_process: false, credentials: None });
    }

    /// Searches the public registry for what the field holds. The answer comes back through the window.
    pub fn registry_search(&mut self) {
        let query = self.pages.text("conn:search").trim().to_owned();
        if query.is_empty() || self.setup.registry.searching {
            return;
        }
        self.setup.registry = Registry { searching: true, query: query.clone(), ..Registry::default() };
        let fetch = self.setup.fetch;
        match self.dispatch.clone() {
            Dispatch::Async { handle, .. } => {
                let q = query.clone();
                let _ = std::thread::Builder::new().name("neko-registry".into()).spawn(move || {
                    let answer = fetch(&q);
                    handle.update_if(move |s, tx| {
                        tx.instant();
                        s.registry_arrived(&q, answer);
                        true
                    });
                });
            }
            _ => {
                let answer = fetch(&query);
                self.registry_arrived(&query, answer);
            }
        }
    }

    pub fn registry_arrived(&mut self, query: &str, answer: Result<String, String>) {
        if self.setup.registry.query != query {
            return;
        }
        self.setup.registry.searching = false;
        match answer {
            Ok(body) => {
                self.setup.registry.results = words::parse_registry(&body);
                self.setup.registry.error = None;
            }
            Err(why) => {
                self.setup.registry.results = Vec::new();
                self.setup.registry.error = Some(why);
            }
        }
    }

    /// "Scan again" in the import tab: looks in the workspace's folders.
    pub fn import_scan(&mut self) {
        let Some(Dialog::Connection { workspace, .. }) = self.setup.dialog.clone() else { return };
        let folders = self.setup_data().and_then(|d| d.workspace(&workspace).map(|w| d.folders_of(w))).unwrap_or_default();
        self.setup_call(SetupAction::Discover { repositories: folders });
    }

    pub fn import_expand(&mut self, id: &str) {
        if let Some(Dialog::Connection { expanded, .. }) = self.setup.dialog.as_mut() {
            *expanded = if expanded.as_deref() == Some(id) { None } else { Some(id.to_owned()) };
        }
    }

    pub fn import_link(&mut self, candidate: &str, local: bool) {
        let Some(Dialog::Connection { workspace, .. }) = self.setup.dialog.clone() else { return };
        self.setup_call(SetupAction::LinkSource { workspace_id: workspace, candidate_id: candidate.to_owned(), trust_local_process: local });
    }

    // ---- skills

    /// The workspace Skills shows: the one chosen, else the first.
    pub fn skills_workspace(&self) -> String {
        let data = self.setup_data();
        self.setup.skills_workspace.clone().filter(|w| data.is_some_and(|d| d.workspace(w).is_some())).or_else(|| data.and_then(|d| d.workspaces.first().map(|w| w.id.clone()))).unwrap_or_default()
    }

    pub fn open_audit(&mut self, proposal_id: &str, url: &str) {
        self.open_link(url);
        self.setup.audit_opened.insert(proposal_id.to_owned());
    }

    pub fn preview_skill(&mut self) {
        let url = self.pages.text("skill:url").trim().to_owned();
        let workspace_id = self.skills_workspace();
        if !url.is_empty() && !workspace_id.is_empty() {
            self.setup_call(SetupAction::SkillPreview { workspace_id, url });
        }
    }

    // ---- memory

    /// The profile whose memories show: the workspace's own, else the active one (the web's `profileFor`).
    pub fn memory_profile(&self) -> String {
        let Some(d) = self.setup_data() else { return "default".to_owned() };
        if self.setup.scope.is_empty() {
            return if d.profiles.active_profile_id.is_empty() { "default".to_owned() } else { d.profiles.active_profile_id.clone() };
        }
        d.profiles.assignments.iter().find(|a| a.workspace_id == self.setup.scope).map_or("default".to_owned(), |a| a.profile_id.clone())
    }

    pub fn open_memory_dialog(&mut self, id: Option<&str>) {
        let Some(d) = self.setup_data() else { return };
        let (kind, text, workspace) = match id.and_then(|i| d.memory.iter().find(|m| m.id == i)) {
            Some(m) => (m.kind, m.text.clone(), m.workspace_id.clone().unwrap_or_default()),
            None => (MemoryKind::Profile, String::new(), if self.setup.scope.is_empty() { d.workspaces.first().map(|w| w.id.clone()).unwrap_or_default() } else { self.setup.scope.clone() }),
        };
        self.seed_dialog("", &text, "");
        self.setup.dialog = Some(Dialog::Memory { id: id.map(str::to_owned), kind, workspace });
    }

    pub fn save_memory(&mut self) {
        let Some(Dialog::Memory { id, kind, workspace }) = self.setup.dialog.clone() else { return };
        let text = self.pages.text(F_TEXT).trim().to_owned();
        if text.is_empty() {
            self.form_note("Write what to remember.");
            return;
        }
        if kind != MemoryKind::Profile && workspace.is_empty() {
            self.form_note("Choose a workspace.");
            return;
        }
        let now = self.now_ms;
        let base = id.as_deref().and_then(|i| self.setup_data().and_then(|d| d.memory.iter().find(|m| m.id == i)).cloned());
        let entry = MemoryEntry {
            agent_profile_id: base.as_ref().map_or_else(|| self.memory_profile(), |b| b.agent_profile_id.clone()),
            id: id.unwrap_or_default(),
            kind,
            workspace_id: (kind != MemoryKind::Profile).then_some(workspace),
            text,
            source: base.as_ref().map_or("user".to_owned(), |b| b.source.clone()),
            created_at_ms: base.as_ref().map_or(now, |b| b.created_at_ms),
            updated_at_ms: now,
        };
        self.pages.notes.remove("setup:form");
        self.setup_call(SetupAction::SaveMemory(Box::new(entry)));
    }

    pub fn confirm_forget_memory(&mut self, id: &str) {
        self.setup.dialog = Some(Dialog::Confirm(Confirm { title: "Forget this memory?".to_owned(), description: "Agents stop using it. This can’t be undone.".to_owned(), button: "Forget memory".to_owned(), coral: true, calls: vec![SetupAction::ForgetMemory { id: id.to_owned() }] }));
    }

    pub fn set_memory_options(&mut self, learning: Option<bool>, use_memory: Option<bool>) {
        let current = self.setup_data().and_then(|d| d.memory_options).unwrap_or_default();
        self.setup_call(SetupAction::SetMemoryOptions(MemoryOptions { learning: learning.unwrap_or(current.learning), use_memory: use_memory.unwrap_or(current.use_memory) }));
    }

    pub fn open_preference_dialog(&mut self, id: Option<&str>) {
        let Some(d) = self.setup_data() else { return };
        let target = if self.setup.scope.is_empty() { d.workspaces.first().map(|w| w.id.clone()).unwrap_or_default() } else { self.setup.scope.clone() };
        let (workspace, instruction, terms, exceptions) = match id.and_then(|i| d.preferences.iter().find(|p| p.id == i)) {
            Some(p) => (p.workspace_id.clone(), p.instruction.clone(), p.applicability.terms.join(", "), p.exceptions.join(", ")),
            None => (target, String::new(), String::new(), String::new()),
        };
        self.seed_dialog(&terms, &instruction, &exceptions);
        self.setup.dialog = Some(Dialog::Preference { id: id.map(str::to_owned), workspace });
    }

    /// Guidance: the instruction is `dlg:text`, its phrases `dlg:name`, its exceptions `dlg:path`.
    pub fn save_preference(&mut self) {
        let Some(Dialog::Preference { id, workspace }) = self.setup.dialog.clone() else { return };
        let instruction = self.pages.text(F_TEXT).trim().to_owned();
        let terms = words::phrases(self.pages.text(F_NAME));
        let base = id.as_deref().and_then(|i| self.setup_data().and_then(|d| d.preferences.iter().find(|p| p.id == i)).cloned());
        let task_ids = base.as_ref().map(|b| b.applicability.task_ids.clone()).unwrap_or_default();
        if instruction.is_empty() || (terms.is_empty() && task_ids.is_empty()) {
            self.form_note(if instruction.is_empty() { "Describe how Neko should approach it." } else { "Add at least one word or phrase it applies to." });
            return;
        }
        let exceptions = words::phrases(self.pages.text(F_PATH));
        self.pages.notes.remove("setup:form");
        self.setup_call(SetupAction::SavePreference { workspace_id: base.as_ref().map_or(workspace, |b| b.workspace_id.clone()), id: id.unwrap_or_default(), expected_version: base.as_ref().map(|b| b.version), terms, task_ids, instruction, supporting: base.map(|b| b.supporting_record_ids).unwrap_or_default(), exceptions });
    }

    pub fn confirm_forget_preference(&mut self, id: &str) {
        let Some(p) = self.setup_data().and_then(|d| d.preferences.iter().find(|p| p.id == id)).cloned() else { return };
        self.setup.dialog = Some(Dialog::Confirm(Confirm { title: "Forget this guidance?".to_owned(), description: "Neko stops applying it. This can’t be undone.".to_owned(), button: "Forget guidance".to_owned(), coral: true, calls: vec![SetupAction::ForgetPreference { workspace_id: p.workspace_id, id: p.id, version: p.version }] }));
    }

    pub fn open_correct_dialog(&mut self, record_id: &str) {
        let Some(r) = self.setup_data().and_then(|d| d.decisions.iter().find(|r| r.id == record_id)).cloned() else { return };
        self.seed_dialog("", "", "");
        self.setup.dialog = Some(Dialog::Correct { record_id: r.id, workspace_id: r.workspace_id, version: r.version });
    }

    pub fn save_correction(&mut self) {
        let Some(Dialog::Correct { record_id, workspace_id, version }) = self.setup.dialog.clone() else { return };
        let correction = self.pages.text(F_TEXT).trim().to_owned();
        if correction.is_empty() {
            self.form_note("Say what Neko should do differently.");
            return;
        }
        self.setup_call(SetupAction::CorrectDecision { workspace_id, record_id, version, correction });
    }

    // ---- profiles

    pub fn open_profile_dialog(&mut self, id: Option<&str>) {
        let (name, text) = match id.and_then(|i| self.setup_data().and_then(|d| d.profiles.profiles.iter().find(|p| p.id == i))) {
            Some(p) => (p.name.clone(), p.instructions.clone()),
            None => (String::new(), String::new()),
        };
        self.seed_dialog(&name, &text, "");
        self.setup.dialog = Some(Dialog::Profile { id: id.map(str::to_owned) });
    }

    pub fn save_profile(&mut self) {
        let Some(Dialog::Profile { id }) = self.setup.dialog.clone() else { return };
        let name = self.pages.text(F_NAME).trim().to_owned();
        if name.is_empty() {
            self.form_note("Give the profile a name.");
            return;
        }
        self.pages.notes.remove("setup:form");
        self.setup_call(SetupAction::SaveProfile(AgentProfile { id: id.unwrap_or_default(), name, instructions: self.pages.text(F_TEXT).to_owned() }));
    }

    // ---- schedules

    pub fn open_schedule_dialog(&mut self, id: Option<&str>, workspace: &str) {
        let Some(d) = self.setup_data() else { return };
        let base = id.and_then(|i| d.schedules.iter().find(|s| s.id == i)).cloned();
        let parts = base.as_ref().map(|s| words::parse_rule(&s.rule));
        let (name, prompt) = base.as_ref().map_or((String::new(), String::new()), |s| (s.name.clone(), s.prompt.clone()));
        let zone = base.as_ref().map(|s| s.timezone.clone()).filter(|z| !z.is_empty()).unwrap_or_else(local_zone);
        self.seed_dialog(&name, &prompt, "");
        let rule = base.as_ref().map_or("FREQ=DAILY;BYHOUR=9;BYMINUTE=0".to_owned(), |s| s.rule.clone());
        let p = parts.unwrap_or(words::RuleParts { kind: Recurrence::Daily, hour: 9, minute: 0, weekday: "MO".to_owned(), interval: 4 });
        self.pages.set_field("sch:time", &format!("{:02}:{:02}", p.hour, p.minute));
        self.pages.set_field("sch:interval", &p.interval.to_string());
        self.pages.set_field("sch:custom", &rule);
        self.pages.set_field("sch:zone", &zone);
        let ws = base.as_ref().and_then(|s| s.workspace_id.clone()).unwrap_or_else(|| workspace.to_owned());
        self.setup.dialog = Some(Dialog::Schedule { id: id.map(str::to_owned), workspace: ws, kind: p.kind, weekday: p.weekday });
    }

    /// Saving always pauses a schedule: turning it on is a separate, deliberate step.
    pub fn save_schedule(&mut self) {
        let Some(Dialog::Schedule { id, workspace, kind, weekday }) = self.setup.dialog.clone() else { return };
        let name = self.pages.text(F_NAME).trim().to_owned();
        let prompt = self.pages.text(F_TEXT).trim().to_owned();
        let zone = self.pages.text("sch:zone").trim().to_owned();
        let (hour, minute) = words::parse_time(self.pages.text("sch:time")).unwrap_or((9, 0));
        let interval = self.pages.text("sch:interval").trim().parse::<u32>().unwrap_or(4).clamp(1, 168);
        let rule = words::rule_for(kind, hour, minute, &weekday, interval, self.pages.text("sch:custom").trim());
        if name.is_empty() || prompt.is_empty() || rule.trim().is_empty() || zone.is_empty() {
            self.form_note(if name.is_empty() { "Give the schedule a name." } else if prompt.is_empty() { "Say what to prepare." } else if zone.is_empty() { "Enter a time zone." } else { "Enter a recurrence rule." });
            return;
        }
        if matches!(kind, Recurrence::Daily | Recurrence::Weekdays | Recurrence::Weekly) && words::parse_time(self.pages.text("sch:time")).is_none() {
            self.form_note("Enter the time as hours and minutes, like 09:00.");
            return;
        }
        let base = id.as_deref().and_then(|i| self.setup_data().and_then(|d| d.schedules.iter().find(|s| s.id == i)).cloned());
        let schedule = Schedule {
            id: id.unwrap_or_default(),
            name,
            prompt,
            workspace_id: match &base {
                Some(b) => b.workspace_id.clone(),
                None => (!workspace.is_empty()).then_some(workspace),
            },
            rule,
            timezone: zone,
            anchor_ms: base.as_ref().map_or(self.now_ms, |b| b.anchor_ms),
            enabled: false,
            ..base.unwrap_or_default()
        };
        self.pages.notes.remove("setup:form");
        self.setup_call(SetupAction::SaveSchedule(Box::new(schedule)));
    }

    pub fn confirm_delete_schedule(&mut self, id: &str) {
        let Some(s) = self.setup_data().and_then(|d| d.schedules.iter().find(|s| s.id == id)).cloned() else { return };
        self.setup.dialog = Some(Dialog::Confirm(Confirm { title: format!("Delete “{}”?", s.name), description: "Future runs stop. Tasks it already created stay.".to_owned(), button: "Delete schedule".to_owned(), coral: true, calls: vec![SetupAction::RemoveSchedule { id: s.id }] }));
    }

    /// The confirmation's button: its requests go out one after the other, and the dialog stays open until the last was
    /// answered (a refusal shows in it).
    pub fn confirm_setup(&mut self) {
        let Some(Dialog::Confirm(c)) = self.setup.dialog.clone() else { return };
        self.pages.notes.retain(|k, _| !k.starts_with("setup:"));
        self.setup.queue = c.calls.iter().skip(1).cloned().collect();
        if let Some(first) = c.calls.first().cloned() {
            self.setup_call(first);
        }
    }

    /// Choosing in the dialog's pick of Memory's kind.
    pub fn memory_set_kind(&mut self, kind: MemoryKind) {
        if let Some(Dialog::Memory { kind: k, .. }) = self.setup.dialog.as_mut() {
            *k = kind;
        }
    }

    pub fn memory_set_workspace(&mut self, workspace: &str) {
        if let Some(Dialog::Memory { workspace: w, .. }) = self.setup.dialog.as_mut() {
            *w = workspace.to_owned();
        }
    }

    pub fn schedule_set_kind(&mut self, kind: Recurrence) {
        if let Some(Dialog::Schedule { kind: k, .. }) = self.setup.dialog.as_mut() {
            *k = kind;
        }
    }

    pub fn schedule_set_weekday(&mut self, code: &str) {
        if let Some(Dialog::Schedule { weekday, .. }) = self.setup.dialog.as_mut() {
            *weekday = code.to_owned();
        }
    }

    pub fn schedule_set_workspace(&mut self, workspace: &str) {
        if let Some(Dialog::Schedule { workspace: w, .. }) = self.setup.dialog.as_mut() {
            *w = workspace.to_owned();
        }
    }

    pub fn watch_set_enabled(&mut self, on: bool) {
        if let Some(Dialog::Watch { enabled, .. }) = self.setup.dialog.as_mut() {
            *enabled = on;
        }
    }

    pub fn watch_set_prepare(&mut self, on: bool) {
        if let Some(Dialog::Watch { prepare, .. }) = self.setup.dialog.as_mut() {
            *prepare = on;
        }
    }
}
