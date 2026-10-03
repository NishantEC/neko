//! Durable observations and user-confirmed working style. These types confer
//! no tool, execution, filesystem, or publication authority.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionAction {
    PrepareFix,
    AskUser,
    Skip,
    ProposePlan,
    ReviewLocalResult,
    ReportFailure,
    ApproveLocalBuild,
    CancelTask,
    AddNote,
    AcceptLocalResult,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionProvenance {
    Supervisor,
    HostObservation,
    UserCommand,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStage {
    Unknown,
    LocalAccepted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreferenceVersion {
    pub id: String,
    pub version: u32,
}

/// Historical scope reference, never a capability or grant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DecisionScope {
    pub agent_profile_id: String,
    pub profile_revision: u64,
    pub repository: String,
    pub responsibility_ids: Vec<String>,
    pub connection_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSnapshot {
    pub source_id: String,
    pub connection_id: String,
    pub revision: String,
    pub retrieved_ms: i64,
    pub receipt_ids: Vec<String>,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DecisionSource {
    /// Host-computed SHA-256 of the decision inputs; retries are idempotent.
    pub fingerprint: String,
    pub revision: Option<String>,
    pub evidence: Vec<EvidenceSnapshot>,
    /// Supervisor's declared evidence, distinct from host source receipts.
    pub declared_evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DecisionCorrection {
    pub record_id: String,
    pub text: String,
    pub at_ms: i64,
}

/// Immutable episode version. A correction appends a new id/version linked to
/// the previous record; it never rewrites the host observation or its provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DecisionRecord {
    pub id: String,
    pub episode_id: String,
    pub version: u32,
    pub supersedes_record_id: Option<String>,
    pub workspace_id: String,
    pub task_id: Option<String>,
    pub source: DecisionSource,
    pub action: DecisionAction,
    pub alternatives: Vec<String>,
    pub rationale: String,
    pub provenance: DecisionProvenance,
    pub runtime: crate::workbench::AgentRuntime,
    pub preference_versions: Vec<PreferenceVersion>,
    pub scope: DecisionScope,
    pub expected_outcome: Option<String>,
    pub observed_outcome: Option<String>,
    pub delivery_stage: DeliveryStage,
    pub correction: Option<DecisionCorrection>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreferenceState {
    Proposed,
    Confirmed,
    Dismissed,
}

/// Terms match complete words in a task/request. With task ids, both selectors
/// must match. At least one selector is required; there is no global fallback.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreferenceApplicability {
    pub terms: Vec<String>,
    pub task_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkingPreference {
    pub id: String,
    pub workspace_id: String,
    pub agent_profile_id: String,
    pub state: PreferenceState,
    pub applicability: PreferenceApplicability,
    pub instruction: String,
    pub supporting_record_ids: Vec<String>,
    /// Complete-word phrases excluding this guidance from matching requests.
    pub exceptions: Vec<String>,
    pub version: u32,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// User controls only. No command accepts arbitrary DecisionRecord, outcomes,
/// host evidence, runtime identity, confirmation state, or authority.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DecisionContextCommand {
    /// Empty id + no expected version creates; edits require the current version.
    /// Every edit returns to proposed, so changed guidance needs confirmation.
    SavePreference {
        workspace_id: String,
        id: String,
        expected_version: Option<u32>,
        applicability: PreferenceApplicability,
        instruction: String,
        supporting_record_ids: Vec<String>,
        exceptions: Vec<String>,
    },
    KeepPreference {
        workspace_id: String,
        id: String,
        expected_version: u32,
    },
    DismissPreference {
        workspace_id: String,
        id: String,
        expected_version: u32,
    },
    ForgetPreference {
        workspace_id: String,
        id: String,
        expected_version: u32,
    },
    CorrectDecision {
        workspace_id: String,
        record_id: String,
        expected_version: u32,
        correction: String,
    },
}
