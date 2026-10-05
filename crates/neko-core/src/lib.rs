//! Daemon-owned logic: the SQLite schema, the application index, search
//! ranking, launching, hotkey-setting persistence, and the `Provider` seam
//! every result type in the search list is built on. Nothing in this crate
//! talks to a socket — that's `neko-daemon`'s job — and nothing in it talks
//! to AppKit's window/event-loop APIs beyond the read-only icon/launch calls
//! below, which work fine from a headless process.

pub mod agent_profiles;
pub mod agent_catalog;
pub mod answers;
pub mod agents;
pub mod apps;
pub mod ask;
pub mod cancel;
pub mod clipboard;
pub mod codex;
pub mod commands;
pub mod conversation;
pub mod db;
pub mod decision_context;
pub mod decomposition;
pub mod files;
pub mod hotkey;
pub mod icons;
pub mod launch;
pub mod linear;
pub mod mcp;
pub mod mcp_host;
pub mod memory_learning;
pub mod native_runner;
pub mod native_tasks;
pub mod neko_chat;
pub mod neko_memory;
pub mod new_agent;
pub mod onboarding;
pub mod permissions;
pub mod preferences;
pub mod provider;
pub mod runtime_selection;
pub mod schedule_import;
pub mod schedule_time;
pub mod scheduled_plans;
pub mod schedules;
pub mod search;
pub mod settings;
pub mod setup_import;
pub mod skills;
pub mod supervision;
pub mod terminals;
pub mod themes;
pub mod usage;
pub mod verification;
pub mod workbench;

pub use apps::AppEntry;
pub use cancel::Cancel;
pub use db::Db;
pub use provider::{Provider, ProviderError};

/// Current wall-clock time in Unix milliseconds. Shared by every provider's
/// own recency scoring (`apps::AppsProvider`, `clipboard::ClipboardProvider`)
/// and by `neko-daemon`'s non-search request handlers, so there's exactly
/// one definition of "now" in this codebase.
pub fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
