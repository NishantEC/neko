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
}
