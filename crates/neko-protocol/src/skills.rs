//! User-controlled skill discovery and activation. Skills never grant tools.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Skill {
    /// Canonical SKILL.md path; also the stable local identity.
    pub path: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub workspace_id: Option<String>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnabledSkill {
    pub workspace_id: String,
    pub path: String,
    /// Changed instructions require renewed enablement.
    pub content_hash: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillState {
    pub enabled: Vec<EnabledSkill>,
    #[serde(default)]
    pub available: Vec<Skill>,
    #[serde(default)]
    pub proposals: Vec<SkillProposal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillProposal {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub body: String,
    pub content_hash: String,
    pub source: String,
    pub audit_url: Option<String>,
    pub audit_status: String,
    /// Explicit user review of the linked audit, pinned to these instructions.
    #[serde(default)]
    pub audit_reviewed_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillCommand {
    Refresh,
    SetEnabled {
        workspace_id: String,
        path: String,
        content_hash: String,
        enabled: bool,
    },
    PreviewRepository {
        workspace_id: String,
        url: String,
    },
    DecideProposal {
        id: String,
        content_hash: String,
        accept: bool,
    },
    ConfirmAuditReview {
        id: String,
        content_hash: String,
        audit_url: String,
    },
}
