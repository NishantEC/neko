//! Packs: one installable unit that bundles tools (MCP connections), skills,
//! a web plugin and routine templates (`neko_core::packs`). The wire types are
//! here; nothing in them carries a secret back to a client (an environment
//! value is write-only, like every other credential).
use crate::workbench::Secret;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PackCommand {
    /// The installed packs and the bundled catalog. Answered with `Packs`.
    List,
    /// Reads a pack's `pack.toml` from a local folder, `catalog:<name>` or a
    /// repository URL and says what installing it would add. Changes nothing.
    /// Answered with `PackPreview`.
    Preview { source: String },
    /// Installs the pack, all or nothing. `approve` must be true: it is the
    /// person's yes to the permission summary. `env` supplies the values of the
    /// environment variables the pack asks for; they go to the Keychain. Answered with `Packs`.
    Install {
        source: String,
        approve: bool,
        #[serde(default)]
        env: BTreeMap<String, Secret>,
    },
    /// Removes everything the pack owns, after keeping a copy in
    /// `<data>/cleanup-archive/packs/`. Answered with `Packs`.
    Remove { name: String },
    /// Installs the source's newer version over the installed one, keeping the
    /// person's grants and routine on/off states. Answered with `Packs`.
    Update { name: String },
    /// Runs the pack's health check again. Answered with `Packs`.
    Health { name: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PackStatus {
    Healthy,
    /// A required program or environment value is missing.
    NeedsSetup,
    /// Everything is there but the health check failed.
    Failing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackRequirement {
    pub name: String,
    /// What to run to get it (shown with a Copy button).
    pub hint: String,
    pub found: bool,
    /// Where it was found.
    #[serde(default)]
    pub path: Option<String>,
}

/// What a pack adds (or, for an installed one, owns), by name.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackContents {
    pub connections: Vec<String>,
    pub skills: Vec<String>,
    pub plugin: Option<String>,
    pub routines: Vec<String>,
    /// `.luau` files stored for later; nothing runs them yet.
    #[serde(default)]
    pub luau: Vec<String>,
}

/// A pack as its `pack.toml` describes it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackInfo {
    pub name: String,
    pub title: String,
    pub version: String,
    pub description: String,
    pub homepage: String,
    pub licence: String,
    /// In plain words, shown before the person says yes.
    pub permissions: Vec<String>,
    pub requirements: Vec<PackRequirement>,
    /// Environment variable names the person must supply (never their values).
    pub env: Vec<String>,
    pub contents: PackContents,
    /// The health check command, as written.
    pub health_command: Vec<String>,
    /// Every command line the pack would run on this Mac (its tool servers and
    /// its health check), as written, so the person sees them before saying yes.
    #[serde(default)]
    pub runs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackHealth {
    pub ok: bool,
    pub at_ms: i64,
    /// The last lines of what the check printed (or why it could not run).
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledPack {
    pub info: PackInfo,
    /// Where it came from: a folder, a repository URL or `catalog:<name>`.
    pub source: String,
    pub status: PackStatus,
    pub installed_ms: i64,
    /// When the person approved the permission summary.
    pub approved_ms: i64,
    pub health: Option<PackHealth>,
    /// Ids of the MCP connections the pack owns (Setup, Tools).
    pub connection_ids: Vec<String>,
    pub skill_paths: Vec<String>,
    pub plugin_dir: Option<String>,
    pub routine_ids: Vec<String>,
    /// The source's version when it is higher than the installed one (known for
    /// folders and the catalog; a remote source is only looked at on Update).
    #[serde(default)]
    pub update_available: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogEntry {
    pub info: PackInfo,
    /// Pass this as `source` to preview or install it (`catalog:<name>`).
    pub source: String,
    pub installed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackListing {
    pub installed: Vec<InstalledPack>,
    pub catalog: Vec<CatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackPreview {
    pub info: PackInfo,
    pub source: String,
    /// The version already installed under this name, if any.
    pub installed_version: Option<String>,
}
