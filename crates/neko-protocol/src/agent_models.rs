//! Wire types for the daemon-owned agent model catalog.
//!
//! Discovery is metadata only: listing models never generates tokens and
//! never edits a provider's own settings. Only [`ModelCheck`] (one explicit
//! request) sends a real, minimal prompt through the task runner.

use serde::{Deserialize, Serialize};

use crate::workbench::AgentRuntime;

/// Whether a source can serve tasks right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    Ready,
    SignInRequired,
    NotInstalled,
    NotRunning,
    Error,
}

/// What Neko knows about access to one model. `Listed` is not a guarantee:
/// plans and quotas are only proven by a successful [`ModelCheck`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelAccess {
    Listed,
    Checked,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeOption {
    /// Native runtime wire value, not a display alias.
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogModel {
    /// Exactly the value `AgentRuntime::model` takes for this source.
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub recommended: bool,
    pub access: ModelAccess,
    /// Why the model is unavailable, or what a check found.
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub reasoning_efforts: Vec<String>,
    /// None means unknown; an empty list means unsupported.
    #[serde(default)]
    pub effort_options: Option<Vec<RuntimeOption>>,
    #[serde(default)]
    pub default_effort: Option<String>,
    /// Additional tiers only. Normal (`default`) belongs to the resolver/UI.
    /// None means unknown; an empty list means unsupported by this adapter.
    #[serde(default)]
    pub speed_options: Option<Vec<RuntimeOption>>,
    #[serde(default)]
    pub default_speed: Option<String>,
}

/// One place models come from, keyed by the `AgentRuntime::provider` value
/// that runs them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSource {
    pub provider: String,
    pub label: String,
    /// Human connection summary, e.g. "ChatGPT · Pro" or "Not running".
    pub connection: String,
    pub status: SourceStatus,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub models: Vec<CatalogModel>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCatalog {
    pub sources: Vec<ModelSource>,
    /// Unix milliseconds when this catalog was read.
    #[serde(default)]
    pub read_at_ms: i64,
}

/// One line of the diagnostics report: what was checked, how long it took.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticCheck {
    pub name: String,
    pub ok: bool,
    pub detail: String,
    pub millis: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsReport {
    pub checks: Vec<DiagnosticCheck>,
}

/// The outcome of one explicit model check. A failed check never changes
/// the saved runtime; messages are credential-free by construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCheck {
    pub runtime: AgentRuntime,
    pub ok: bool,
    /// True when the failure is about access (plan, sign-in, quota), so the
    /// picker can gray the model out rather than suggest a retry.
    #[serde(default)]
    pub unavailable: bool,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_catalogs_keep_unknown_capabilities_distinct_from_unsupported() {
        let mut wire = serde_json::json!({"id":"gpt-a","label":"A","access":"listed","reasoning_efforts":["high"]});
        let old: CatalogModel = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(old.reasoning_efforts, ["high"]);
        assert!(old.effort_options.is_none() && old.speed_options.is_none());
        wire["effort_options"] = serde_json::json!([]);
        wire["speed_options"] = serde_json::json!([]);
        let unsupported: CatalogModel = serde_json::from_value(wire).unwrap();
        assert_eq!(unsupported.effort_options, Some(vec![]));
        assert_eq!(unsupported.speed_options, Some(vec![]));
        assert_eq!(serde_json::from_value::<CatalogModel>(serde_json::to_value(&unsupported).unwrap()).unwrap(), unsupported);
    }

    #[test]
    fn runtime_option_description_is_optional() {
        let option: RuntimeOption = serde_json::from_value(serde_json::json!({"id":"priority","label":"Fast"})).unwrap();
        assert_eq!(option.description, None);
    }
}
