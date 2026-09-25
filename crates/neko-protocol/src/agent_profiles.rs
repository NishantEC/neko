//! User-owned agent identities. Read grants never grant tools or workspace access.
use serde::{Deserialize, Serialize};

pub const DEFAULT_PROFILE_ID: &str = "default";
pub fn default_profile_id() -> String {
    DEFAULT_PROFILE_ID.into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub instructions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceAssignment {
    pub workspace_id: String,
    pub profile_id: String,
}

/// Directional permission for global profile memories only. No workspace data,
/// conversation, credentials, tool grants, or source profile instructions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileReadGrant {
    pub reader_id: String,
    pub source_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AgentProfiles {
    pub profiles: Vec<AgentProfile>,
    pub assignments: Vec<WorkspaceAssignment>,
    pub read_grants: Vec<ProfileReadGrant>,
    pub active_profile_id: String,
    /// Changes invalidate existing worker/tool authority, including ABA edits.
    pub revision: u64,
}

impl Default for AgentProfiles {
    fn default() -> Self {
        Self {
            profiles: vec![AgentProfile {
                id: default_profile_id(),
                name: "Neko".into(),
                instructions: String::new(),
            }],
            assignments: vec![],
            read_grants: vec![],
            active_profile_id: default_profile_id(),
            revision: 0,
        }
    }
}

impl AgentProfiles {
    pub fn owner(&self, workspace_id: &str) -> &str {
        self.assignments
            .iter()
            .find(|a| a.workspace_id == workspace_id)
            .map(|a| a.profile_id.as_str())
            .unwrap_or(DEFAULT_PROFILE_ID)
    }
    pub fn for_scope(&self, workspace_id: Option<&str>) -> &str {
        workspace_id
            .map(|id| self.owner(id))
            .unwrap_or(&self.active_profile_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProfileCommand {
    Save {
        profile: AgentProfile,
    },
    AssignWorkspace {
        workspace_id: String,
        profile_id: String,
    },
    SetActive {
        profile_id: String,
    },
    SetReadGrant {
        reader_id: String,
        source_id: String,
        allowed: bool,
    },
}
