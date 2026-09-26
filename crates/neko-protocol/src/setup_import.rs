//! Secret-free previews of configurations discovered on this Mac.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportCandidateKind {
    Workspace,
    Connection,
    Skill,
    Schedule,
}

/// Secret-free, independently selectable ledger entry. The daemon keeps any
/// matching ServerConfig/Secret outside this wire type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportCandidate {
    pub id: String,
    pub kind: ImportCandidateKind,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub workspace: Option<String>,
    pub name: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    #[serde(default)]
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportConnection {
    pub id: String,
    pub name: String,
    pub source: String,
    /// None denotes a global definition; importing it does not grant access.
    pub repository: Option<String>,
    pub has_credentials: bool,
    pub enabled_at_source: bool,
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportPreview {
    #[serde(default)]
    pub sources: Vec<ImportSourceInfo>,
    #[serde(default)]
    pub active_source: Option<String>,
    #[serde(default)]
    pub candidates: Vec<ImportCandidate>,
    #[serde(default)]
    pub schedules: Vec<ImportSchedule>,
    #[serde(default)]
    pub preview_id: String,
    #[serde(default)]
    pub connections: Vec<ImportConnection>,
    #[serde(default)]
    pub repositories: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportSourceInfo {
    pub id: String,
    pub name: String,
}

/// Portable source content only; source authority is deliberately absent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportSchedule {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub source: String,
    pub repository: Option<String>,
    pub rule: String,
    pub timezone: String,
    pub anchor_ms: i64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImportCommand {
    ListSources,
    Discover {
        repositories: Vec<String>,
        #[serde(default)]
        source_id: Option<String>,
    },
    Apply {
        #[serde(default)]
        schedule_ids: Vec<String>,
        #[serde(default)]
        skill_ids: Vec<String>,
        #[serde(default)]
        workspace_ids: Vec<String>,
        preview_id: String,
        connection_ids: Vec<String>,
        /// Explicitly keep these selected workspace-scoped definitions as
        /// global, paused connections when their source workspace is absent.
        #[serde(default)]
        global_connection_ids: Vec<String>,
        repositories: Vec<String>,
        include_credentials: bool,
        trust_local_processes: bool,
    },
}
