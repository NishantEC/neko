//! Packs: one installable unit that bundles tools (MCP connections), skills,
//! a web plugin and routine templates (`neko_core::packs`). The wire types are
//! here; nothing in them carries a secret back to a client (an environment
//! value is write-only, like every other credential).
//!
//! **Approval is tied to what was shown.** `Preview` answers with a `digest`
//! (SHA-256 over everything the person is asked to approve: permissions, every
//! command and argument, environment names, the plugin's code and what it
//! declares, routine texts, skills). `Install` carries that digest and the
//! daemon recomputes it from the tree it actually fetched; a mismatch is
//! refused. `Update` needs the new version's digest when its power changed.
use crate::workbench::Secret;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PackCommand {
    /// The installed packs and the bundled catalog. Answered with `Packs`.
    List,
    /// Reads a pack's `pack.toml` from a local folder, `catalog:<name>`, a
    /// repository URL, or `installed:<name>` (the copy on disk, for "Review"),
    /// and says what installing it would add. Changes nothing. Answered with `PackPreview`.
    Preview { source: String },
    /// Installs the pack, all or nothing. `digest` is the one `Preview` returned
    /// for this source: installing is the person's approval of exactly that.
    /// `env` supplies the values of the environment variables the pack asks for;
    /// they go to the Keychain. Answered with `Packs`.
    Install {
        source: String,
        digest: String,
        #[serde(default)]
        env: BTreeMap<String, Secret>,
    },
    /// Removes everything the pack owns, after keeping a copy (best effort) in
    /// `<data>/cleanup-archive/packs/`. Answered with `Packs`.
    Remove { name: String },
    /// Installs the source's newer version over the installed one, keeping the
    /// person's grants and routine on/off states. When the new version's power
    /// changed (see `PackPreview::needs_approval`) `digest` is required and must be
    /// the new version's. The UIs always send the digest they showed. Answered with `Packs`.
    Update {
        name: String,
        #[serde(default)]
        digest: Option<String>,
    },
    /// Runs the pack's health check again. Answered with `Packs`.
    Health { name: String },
    /// After "Changed on disk": the person reviewed the installed copy
    /// (`Preview { source: "installed:<name>" }`) and approves exactly that digest. Answered with `Packs`.
    Approve { name: String, digest: String },
    /// Deletes what a failed update or removal had to leave behind (`InstalledPack::orphans`). Answered with `Packs`.
    CleanUp { name: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PackStatus {
    Healthy,
    /// A required program is missing.
    NeedsSetup,
    /// Everything is there but the health check failed.
    Failing,
    /// The installed files no longer match what the person approved; nothing runs until reviewed.
    Changed,
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
    /// The `.luau` files (native panels for Rune): cards and views, run in a sandbox by the Rune client.
    #[serde(default)]
    pub luau: Vec<String>,
}

/// What a pack's web plugin declares. It runs inside Neko with access to Neko's own API.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginPower {
    pub title: String,
    pub pages: Vec<String>,
    pub viewers: Vec<String>,
    pub fences: Vec<String>,
    /// Text the plugin adds to the model's instructions.
    pub prompt: String,
    /// SHA-256 over every file of the plugin.
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutineText {
    pub name: String,
    pub instruction: String,
    /// The schedule in words, e.g. "Daily 09:00".
    pub cadence: String,
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
    /// Things to read twice before approving: a command that is a script
    /// interpreter ("Runs a script interpreter: <cmd>"), a plugin that
    /// "Runs inside Neko with access to Neko's own API". Part of the digest.
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub plugin: Option<PluginPower>,
    #[serde(default)]
    pub routine_texts: Vec<RoutineText>,
    /// SHA-256 over the skill folders.
    #[serde(default)]
    pub skills_hash: String,
    /// SHA-256 over every `[ui] luau` file's path and **contents** (empty when the pack has none; an empty value is
    /// left out of the digest, so packs without Luau keep the digests they were approved with). Editing a `.luau`
    /// file changes the digest and sends an installed pack to "Changed on disk".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub luau_hash: String,
}

/// One `.luau` file of an installed, healthy pack whose files are exactly what the person approved: its path
/// inside the pack (`ui/card.luau`) and its text. Answered by `Request::PackUi`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackUiFile {
    pub path: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackHealth {
    pub ok: bool,
    pub at_ms: i64,
    /// The last lines of what the check printed (or why it could not run).
    pub output: String,
}

/// Something a failed update or removal had to leave behind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackOrphan {
    /// A folder path, or `keychain:<connection id>`.
    pub target: String,
    pub reason: String,
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
    /// The digest the person approved.
    #[serde(default)]
    pub approved_digest: String,
    pub health: Option<PackHealth>,
    /// Ids of the MCP connections the pack owns (Setup, Tools).
    pub connection_ids: Vec<String>,
    pub skill_paths: Vec<String>,
    /// The plugin's folder; its name is the plugin id (`/api/plugins/<id>/enable|disable`).
    pub plugin_dir: Option<String>,
    /// The plugin is installed switched off; the person enables it from the pack's detail.
    #[serde(default)]
    pub plugin_disabled: bool,
    pub routine_ids: Vec<String>,
    /// The source's version when it is higher than the installed one (known for
    /// folders and the catalog; a remote source is only looked at on Update).
    #[serde(default)]
    pub update_available: Option<String>,
    #[serde(default)]
    pub orphans: Vec<PackOrphan>,
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
    /// Send this back with `Install`, `Update` or `Approve`.
    pub digest: String,
    /// When a version is installed: what changed in what the pack can do
    /// (commands, arguments, health check, environment, permissions, plugin, and the
    /// text of a routine the person switched on). Empty when nothing did.
    #[serde(default)]
    pub changes: Vec<String>,
    /// An update with `changes` is refused until the person approves it ("Update needs approval").
    #[serde(default)]
    pub needs_approval: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;

    #[test]
    fn the_requests_have_the_shapes_the_web_and_rune_send() {
        let list: Request = serde_json::from_str(r#"{"Packs":"List"}"#).unwrap();
        assert!(matches!(list, Request::Packs(PackCommand::List)));
        let install: Request = serde_json::from_str(r#"{"Packs":{"Install":{"source":"catalog:x","digest":"d","env":{"TOKEN":"v"}}}}"#).unwrap();
        let Request::Packs(PackCommand::Install { source, digest, env }) = install else { panic!("not an install") };
        assert_eq!((source.as_str(), digest.as_str(), env["TOKEN"].0.as_str()), ("catalog:x", "d", "v"));
        let no_env: Request = serde_json::from_str(r#"{"Packs":{"Install":{"source":"/p","digest":"d"}}}"#).unwrap();
        assert!(matches!(no_env, Request::Packs(PackCommand::Install { .. })));
        assert!(serde_json::from_str::<Request>(r#"{"Packs":{"Install":{"source":"/p","approve":true}}}"#).is_err(), "no digest, no install");
        let update: Request = serde_json::from_str(r#"{"Packs":{"Update":{"name":"x","digest":"d"}}}"#).unwrap();
        assert!(matches!(update, Request::Packs(PackCommand::Update { digest: Some(_), .. })));
        for text in [
            r#"{"Packs":{"Preview":{"source":"installed:x"}}}"#,
            r#"{"Packs":{"Remove":{"name":"x"}}}"#,
            r#"{"Packs":{"Update":{"name":"x"}}}"#,
            r#"{"Packs":{"Health":{"name":"x"}}}"#,
            r#"{"Packs":{"Approve":{"name":"x","digest":"d"}}}"#,
            r#"{"Packs":{"CleanUp":{"name":"x"}}}"#,
        ] {
            serde_json::from_str::<Request>(text).unwrap();
        }
        assert_eq!(serde_json::to_string(&PackStatus::Changed).unwrap(), r#""Changed""#);
    }
}
