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
    /// What the curator knows about skills, attached when read; never stored here.
    #[serde(default)]
    pub curator: Curation,
}

/// The skill curator as the app shows it (`neko_core::skills::curator`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Curation {
    /// The person switched the once-a-day pass off (it is on by default).
    pub off: bool,
    pub last_run_ms: i64,
    /// Skills the curator tracks: Neko-written ones, and any a lesson improved.
    pub skills: Vec<CuratedSkill>,
    /// Newest first, at most 10: consolidations applied by themselves to skills Neko wrote.
    pub recent: Vec<CuratorApplied>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CuratedSkill {
    pub path: String,
    /// "neko" (Neko wrote it, unchanged since: consolidations apply by themselves),
    /// "locked" (the person locked it), "edited" (the person changed it by hand: locked)
    /// or "other" (not Neko's: proposals only).
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CuratorApplied {
    pub path: String,
    pub name: String,
    pub at_ms: i64,
    /// The diff that was applied.
    pub diff: String,
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
    /// Set when the proposal improves a skill that already exists: `body` is
    /// then the diff shown for review, and accepting rewrites that file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<SkillPatch>,
}

/// A proposed improvement to one existing SKILL.md.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillPatch {
    /// The canonical SKILL.md path being improved.
    pub path: String,
    /// The content the diff was made against; accepting refuses if the file
    /// no longer has it.
    pub base_hash: String,
    /// The complete improved file.
    pub revised: String,
    /// The skill's name, as the review text names it.
    #[serde(default)]
    pub name: String,
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
    /// Lock a skill so the curator only ever proposes changes to it, or unlock it.
    SetLocked {
        path: String,
        locked: bool,
    },
    /// Switch the once-a-day curator pass on or off.
    SetCurator {
        on: bool,
    },
    ConfirmAuditReview {
        id: String,
        content_hash: String,
        audit_url: String,
    },
}
