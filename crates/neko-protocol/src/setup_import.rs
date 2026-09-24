//! Secret-free previews of configurations discovered on this Mac.
use serde::{Deserialize, Serialize};

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
    pub schedules: Vec<ImportSchedule>,
    #[serde(default)]
    pub preview_id: String,
    pub connections: Vec<ImportConnection>,
    pub repositories: Vec<String>,
    pub warnings: Vec<String>,
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
    Discover { repositories: Vec<String> },
    Apply {
        #[serde(default)]
        schedule_ids: Vec<String>,
        preview_id: String,
        connection_ids: Vec<String>,
        repositories: Vec<String>,
        include_credentials: bool,
        trust_local_processes: bool,
    },
}
