//! The Setup pages' data and requests (Workspaces, Watching, Tools, Skills, Memory, Profiles,
//! Schedules). [`SetupData`] is what the client keeps of the workbench for them; [`SetupAction`] is one
//! request each of the web UI's setup pages sends (`pages/setup/*.tsx`), with the same fields, so a
//! click here and a click there do the same thing. The daemon stays the only writer: every request
//! answers with the new workbench, which carries the new [`SetupData`].

use std::collections::BTreeMap;

use neko_protocol::agent_profiles::{AgentProfile, AgentProfiles, ProfileCommand};
use neko_protocol::decision_context::{DecisionContextCommand, DecisionRecord, PreferenceApplicability, PreferenceState, WorkingPreference};
use neko_protocol::mcp_host::{McpCommand, McpConnection, Responsibility, ServerConfig, ToolGrant};
use neko_protocol::scheduled_plans::{Schedule, ScheduleCommand};
use neko_protocol::setup_import::{ImportCandidate, ImportCandidateKind, ImportCommand};
use neko_protocol::skills::{SkillCommand, SkillState};
use neko_protocol::workbench::{Command, MemoryEntry, MemoryOptions, MemoryProposal, Secret, Snapshot, Workspace};
use neko_protocol::Request;

/// How many decision records the client keeps (the web shows the newest 12 episodes).
pub const DECISIONS_KEPT: usize = 60;

/// What the setup pages read from the workbench.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SetupData {
    pub workspaces: Vec<Workspace>,
    /// Every folder of each workspace (a legacy workspace has its `repository`).
    pub folders: BTreeMap<String, Vec<String>>,
    pub connections: Vec<McpConnection>,
    pub grants: Vec<ToolGrant>,
    pub watches: Vec<Responsibility>,
    /// Watches Neko proposed in chat that nobody has acted on yet.
    pub suggested: Vec<String>,
    /// What `SetupImport` found on this Mac (connections only are shown).
    pub candidates: Vec<ImportCandidate>,
    pub skills: SkillState,
    pub memory: Vec<MemoryEntry>,
    pub memory_proposals: Vec<MemoryProposal>,
    pub memory_options: Option<MemoryOptions>,
    pub preferences: Vec<WorkingPreference>,
    pub decisions: Vec<DecisionRecord>,
    pub profiles: AgentProfiles,
    pub schedules: Vec<Schedule>,
    pub plan_first: bool,
}

impl SetupData {
    pub fn from_snapshot(snapshot: &Snapshot) -> SetupData {
        let offered: std::collections::HashSet<&str> = snapshot.conversation.iter().flat_map(|m| m.responsibility_ids.iter().map(String::as_str)).collect();
        let suggested = snapshot.mcp.responsibilities.iter().filter(|r| offered.contains(r.id.as_str()) && !r.enabled && r.last_attempt_ms.is_none()).map(|r| r.id.clone()).collect();
        let skip = snapshot.decision_records.len().saturating_sub(DECISIONS_KEPT);
        SetupData {
            workspaces: snapshot.workspaces.clone(),
            folders: snapshot.workspace_folders.clone(),
            connections: snapshot.mcp.connections.clone(),
            grants: snapshot.mcp.grants.clone(),
            watches: snapshot.mcp.responsibilities.clone(),
            suggested,
            candidates: snapshot.import_preview.candidates.iter().filter(|c| c.kind == ImportCandidateKind::Connection).cloned().collect(),
            skills: snapshot.skills.clone(),
            memory: snapshot.memory.clone(),
            memory_proposals: snapshot.memory_proposals.clone(),
            memory_options: Some(snapshot.memory_options),
            preferences: snapshot.working_preferences.clone(),
            decisions: snapshot.decision_records[skip..].to_vec(),
            profiles: snapshot.agent_profiles.clone(),
            schedules: snapshot.schedules.clone(),
            plan_first: snapshot.plan_first,
        }
    }

    /// A workspace's folders as the web's `foldersOf` has them.
    pub fn folders_of(&self, workspace: &Workspace) -> Vec<String> {
        let own: Vec<String> = self.folders.get(&workspace.id).map(|f| f.iter().filter(|p| !p.is_empty()).cloned().collect()).unwrap_or_default();
        if !own.is_empty() {
            own
        } else if workspace.repository.is_empty() {
            Vec::new()
        } else {
            vec![workspace.repository.clone()]
        }
    }

    pub fn workspace(&self, id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    pub fn workspace_name(&self, id: &str) -> String {
        if id.is_empty() { "All workspaces".to_owned() } else { self.workspace(id).map_or("Unavailable workspace".to_owned(), |w| w.name.clone()) }
    }

    pub fn connection(&self, id: &str) -> Option<&McpConnection> {
        self.connections.iter().find(|c| c.id == id)
    }

    pub fn watch(&self, id: &str) -> Option<&Responsibility> {
        self.watches.iter().find(|w| w.id == id)
    }

    /// The tool grant is given for this workspace.
    pub fn granted(&self, connection_id: &str, workspace_id: &str, tool: &str, schema_hash: &str) -> bool {
        self.grants.iter().any(|g| g.connection_id == connection_id && g.workspace_id == workspace_id && g.tool_name == tool && g.schema_hash == schema_hash)
    }

    /// How many setup items need the person: connections that are on and broken, and everything proposed
    /// and not yet decided (suggested watches, skills, memories, guidance).
    pub fn attention(&self) -> usize {
        let broken = self.connections.iter().filter(|c| c.enabled && c.error.is_some()).count();
        let guidance = self.preferences.iter().filter(|p| p.state == PreferenceState::Proposed).count();
        broken + self.suggested.len() + self.skills.proposals.len() + self.memory_proposals.len() + guidance
    }
}

/// A secret kept out of every `Debug` print (a connection's credentials).
#[derive(Clone, PartialEq, Eq)]
pub struct Hidden(pub String);

impl std::fmt::Debug for Hidden {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// One request of a setup page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetupAction {
    // Workspaces.
    SaveWorkspace { workspace: Workspace, folders: Vec<String> },
    /// Looks for connections other apps on this Mac configured, in these folders (the web's "Scan again").
    Discover { repositories: Vec<String> },
    // Tools.
    Wake { id: String },
    Authenticate { id: String, client_id: Option<String> },
    Refresh { id: String },
    SetEnabled { id: String, enabled: bool },
    AddConnection { workspace_id: String, label: String, config: ServerConfig, trust_local_process: bool, credentials: Option<Hidden> },
    LinkSource { workspace_id: String, candidate_id: String, trust_local_process: bool },
    /// One tool's grant for one workspace. `workspace_id` empty: the connection's own workspace (`SetToolGrant`).
    Grant { connection_id: String, workspace_id: String, tool: String, schema_hash: String, allowed: bool },
    // Watching.
    SaveWatch(Box<Responsibility>),
    RemoveWatch { id: String },
    RenameWatch { id: String, name: String },
    /// A message to Neko in a workspace (the web's "Suggest from tools").
    AskIn { text: String, workspace_id: String },
    // Skills.
    SkillsRefresh,
    SkillEnable { workspace_id: String, path: String, content_hash: String, enabled: bool },
    SkillPreview { workspace_id: String, url: String },
    SkillDecide { id: String, content_hash: String, accept: bool },
    SkillLock { path: String, locked: bool },
    SkillCurator { on: bool },
    SkillAudit { id: String, content_hash: String, audit_url: String },
    // Memory.
    SaveMemory(Box<MemoryEntry>),
    ForgetMemory { id: String },
    DecideMemory { id: String, accept: bool },
    SetMemoryOptions(MemoryOptions),
    SavePreference { workspace_id: String, id: String, expected_version: Option<u32>, terms: Vec<String>, task_ids: Vec<String>, instruction: String, supporting: Vec<String>, exceptions: Vec<String> },
    KeepPreference { workspace_id: String, id: String, version: u32 },
    DismissPreference { workspace_id: String, id: String, version: u32 },
    ForgetPreference { workspace_id: String, id: String, version: u32 },
    CorrectDecision { workspace_id: String, record_id: String, version: u32, correction: String },
    // Profiles.
    SaveProfile(AgentProfile),
    AssignProfile { workspace_id: String, profile_id: String },
    ActivateProfile { id: String },
    ReadGrant { reader: String, source: String, allowed: bool },
    // Schedules.
    SaveSchedule(Box<Schedule>),
    ScheduleEnabled { id: String, enabled: bool },
    RemoveSchedule { id: String },
    RunSchedule { id: String },
}

impl SetupAction {
    /// The control a refusal shows next to: the page's `notes` key. Row actions name the row; a form's
    /// requests name the form (`setup:form`), the rest the page (`setup:page`).
    pub fn key(&self) -> String {
        use SetupAction::*;
        let row = |id: &str| format!("setup:{id}");
        match self {
            Wake { id } | Authenticate { id, .. } | Refresh { id } | SetEnabled { id, .. } => row(id),
            LinkSource { candidate_id, .. } => row(candidate_id),
            Grant { connection_id, .. } => row(connection_id),
            RemoveWatch { id } | RenameWatch { id, .. } => row(id),
            SkillEnable { path, .. } | SkillLock { path, .. } => row(path),
            SkillDecide { id, .. } | SkillAudit { id, .. } => row(id),
            ForgetMemory { id } | DecideMemory { id, .. } => row(id),
            KeepPreference { id, .. } | DismissPreference { id, .. } | ForgetPreference { id, .. } => row(id),
            CorrectDecision { record_id, .. } => row(record_id),
            AssignProfile { profile_id, .. } => row(profile_id),
            ActivateProfile { id } | ScheduleEnabled { id, .. } | RemoveSchedule { id } | RunSchedule { id } => row(id),
            ReadGrant { reader, .. } => row(reader),
            SaveWorkspace { .. } | AddConnection { .. } | SaveWatch(_) | SaveMemory(_) | SavePreference { .. } | SaveProfile(_) | SaveSchedule(_) => "setup:form".to_owned(),
            Discover { .. } | AskIn { .. } | SkillsRefresh | SkillPreview { .. } | SkillCurator { .. } | SetMemoryOptions(_) => "setup:page".to_owned(),
        }
    }

    /// The request the daemon receives: the same JSON the web UI posts.
    pub fn wire(&self) -> Request {
        use SetupAction::*;
        let mcp = |c: McpCommand| Request::Workbench(Command::Mcp(c));
        let skills = |c: SkillCommand| Request::Workbench(Command::Skills(c));
        let profile = |c: ProfileCommand| Request::Workbench(Command::AgentProfiles(c));
        let schedules = |c: ScheduleCommand| Request::Workbench(Command::Schedules(c));
        let decision = |c: DecisionContextCommand| Request::Workbench(Command::DecisionContext(c));
        match self.clone() {
            SaveWorkspace { workspace, folders } => Request::Workbench(Command::SaveWorkspaceWithFolders { workspace, folders }),
            Discover { repositories } => Request::Workbench(Command::SetupImport(ImportCommand::Discover { repositories, source_id: None })),
            Wake { id } => mcp(McpCommand::Wake { responsibility_id: id }),
            Authenticate { id, client_id } => mcp(McpCommand::Authenticate { connection_id: id, client_id }),
            Refresh { id } => mcp(McpCommand::Discover { connection_id: id }),
            SetEnabled { id, enabled } => mcp(McpCommand::SetEnabled { connection_id: id, enabled }),
            AddConnection { workspace_id, label, config, trust_local_process, credentials } => mcp(McpCommand::AddConnection { workspace_id, label, config, trust_local_process, credentials: credentials.map(|h| Secret(h.0)) }),
            LinkSource { workspace_id, candidate_id, trust_local_process } => mcp(McpCommand::LinkSource { workspace_id, candidate_id, trust_local_process }),
            Grant { connection_id, workspace_id, tool, schema_hash, allowed } if workspace_id.is_empty() => mcp(McpCommand::SetToolGrant { connection_id, tool_name: tool, schema_hash, allowed }),
            Grant { connection_id, workspace_id, tool, schema_hash, allowed } => mcp(McpCommand::SetWorkspaceToolGrant { workspace_id, connection_id, tool_name: tool, schema_hash, allowed }),
            SaveWatch(responsibility) => mcp(McpCommand::SaveResponsibility { responsibility: *responsibility }),
            RemoveWatch { id } => mcp(McpCommand::RemoveResponsibility { responsibility_id: id }),
            RenameWatch { id, name } => mcp(McpCommand::RenameResponsibility { responsibility_id: id, name }),
            AskIn { text, workspace_id } => Request::Workbench(Command::SendMessage { text, workspace_id: Some(workspace_id) }),
            SkillsRefresh => skills(SkillCommand::Refresh),
            SkillEnable { workspace_id, path, content_hash, enabled } => skills(SkillCommand::SetEnabled { workspace_id, path, content_hash, enabled }),
            SkillPreview { workspace_id, url } => skills(SkillCommand::PreviewRepository { workspace_id, url }),
            SkillDecide { id, content_hash, accept } => skills(SkillCommand::DecideProposal { id, content_hash, accept }),
            SkillLock { path, locked } => skills(SkillCommand::SetLocked { path, locked }),
            SkillCurator { on } => skills(SkillCommand::SetCurator { on }),
            SkillAudit { id, content_hash, audit_url } => skills(SkillCommand::ConfirmAuditReview { id, content_hash, audit_url }),
            SaveMemory(entry) => Request::Workbench(Command::SaveMemory { entry: *entry }),
            ForgetMemory { id } => Request::Workbench(Command::DeleteMemory { id }),
            DecideMemory { id, accept } => Request::Workbench(Command::DecideMemoryProposal { id, accept }),
            SetMemoryOptions(options) => Request::Workbench(Command::SetMemoryOptions { options }),
            SavePreference { workspace_id, id, expected_version, terms, task_ids, instruction, supporting, exceptions } => decision(DecisionContextCommand::SavePreference {
                workspace_id,
                id,
                expected_version,
                applicability: PreferenceApplicability { terms, task_ids },
                instruction,
                supporting_record_ids: supporting,
                exceptions,
            }),
            KeepPreference { workspace_id, id, version } => decision(DecisionContextCommand::KeepPreference { workspace_id, id, expected_version: version }),
            DismissPreference { workspace_id, id, version } => decision(DecisionContextCommand::DismissPreference { workspace_id, id, expected_version: version }),
            ForgetPreference { workspace_id, id, version } => decision(DecisionContextCommand::ForgetPreference { workspace_id, id, expected_version: version }),
            CorrectDecision { workspace_id, record_id, version, correction } => decision(DecisionContextCommand::CorrectDecision { workspace_id, record_id, expected_version: version, correction }),
            SaveProfile(profile_row) => profile(ProfileCommand::Save { profile: profile_row }),
            AssignProfile { workspace_id, profile_id } => profile(ProfileCommand::AssignWorkspace { workspace_id, profile_id }),
            ActivateProfile { id } => profile(ProfileCommand::SetActive { profile_id: id }),
            ReadGrant { reader, source, allowed } => profile(ProfileCommand::SetReadGrant { reader_id: reader, source_id: source, allowed }),
            SaveSchedule(schedule) => schedules(ScheduleCommand::Save { schedule: *schedule }),
            ScheduleEnabled { id, enabled } => schedules(ScheduleCommand::SetEnabled { id, enabled }),
            RemoveSchedule { id } => schedules(ScheduleCommand::Remove { id }),
            RunSchedule { id } => schedules(ScheduleCommand::RunNow { id }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(call: &SetupAction) -> serde_json::Value {
        serde_json::to_value(call.wire()).unwrap()
    }

    #[test]
    fn a_workspace_is_saved_with_its_folders_as_the_web_does() {
        let ws = Workspace { id: String::new(), name: "acme".into(), repository: "/a".into(), instructions: "be brief".into(), away_enabled: false };
        let body = json(&SetupAction::SaveWorkspace { workspace: ws, folders: vec!["/a".into(), "/b".into()] });
        let save = &body["Workbench"]["SaveWorkspaceWithFolders"];
        assert_eq!(save["workspace"]["name"], "acme");
        assert_eq!(save["folders"], serde_json::json!(["/a", "/b"]));
    }

    #[test]
    fn tool_grants_use_the_connections_own_workspace_unless_one_is_named() {
        let grant = |ws: &str| SetupAction::Grant { connection_id: "c".into(), workspace_id: ws.into(), tool: "search".into(), schema_hash: "h".into(), allowed: true };
        assert!(json(&grant(""))["Workbench"]["Mcp"]["SetToolGrant"].is_object());
        let named = json(&grant("w1"));
        assert_eq!(named["Workbench"]["Mcp"]["SetWorkspaceToolGrant"]["workspace_id"], "w1");
    }

    #[test]
    fn a_watch_is_woken_renamed_and_removed_with_the_webs_fields() {
        assert_eq!(json(&SetupAction::Wake { id: "r".into() })["Workbench"]["Mcp"]["Wake"]["responsibility_id"], "r");
        assert_eq!(json(&SetupAction::RenameWatch { id: "r".into(), name: "N".into() })["Workbench"]["Mcp"]["RenameResponsibility"]["name"], "N");
        assert_eq!(json(&SetupAction::RemoveWatch { id: "r".into() })["Workbench"]["Mcp"]["RemoveResponsibility"]["responsibility_id"], "r");
    }

    #[test]
    fn credentials_are_never_printed() {
        let call = SetupAction::AddConnection { workspace_id: String::new(), label: "x".into(), config: ServerConfig::Http { url: "https://e.com/mcp".into() }, trust_local_process: false, credentials: Some(Hidden("s3cret".into())) };
        assert!(!format!("{call:?}").contains("s3cret"));
        assert_eq!(json(&call)["Workbench"]["Mcp"]["AddConnection"]["credentials"], "s3cret");
    }

    #[test]
    fn a_schedule_command_names_its_variant() {
        assert_eq!(json(&SetupAction::RunSchedule { id: "s".into() })["Workbench"]["Schedules"]["RunNow"]["id"], "s");
        assert_eq!(json(&SetupAction::ScheduleEnabled { id: "s".into(), enabled: true })["Workbench"]["Schedules"]["SetEnabled"]["enabled"], true);
    }

    #[test]
    fn a_refusal_lands_beside_the_control_that_asked() {
        assert_eq!(SetupAction::Wake { id: "w1".into() }.key(), "setup:w1");
        assert_eq!(SetupAction::SaveProfile(AgentProfile { id: String::new(), name: "n".into(), instructions: String::new() }).key(), "setup:form");
        assert_eq!(SetupAction::SkillsRefresh.key(), "setup:page");
    }

    #[test]
    fn what_needs_you_is_counted_from_broken_connections_and_proposals() {
        let mut data = SetupData::default();
        assert_eq!(data.attention(), 0);
        data.memory_proposals.push(MemoryProposal { id: "m".into(), agent_profile_id: "default".into(), workspace_id: None, kind: neko_protocol::workbench::MemoryKind::Profile, text: "t".into(), source: "chat".into(), created_at_ms: 0 });
        data.suggested.push("r".into());
        assert_eq!(data.attention(), 2);
    }
}
