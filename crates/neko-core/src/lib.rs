//! Daemon-owned logic: the SQLite schema, the application index, search
//! ranking, launching, hotkey-setting persistence, and the `Provider` seam
//! every result type in the search list is built on. Nothing in this crate
//! talks to a socket — that's `neko-daemon`'s job — and nothing in it talks
//! to AppKit's window/event-loop APIs beyond the read-only icon/launch calls
//! below, which work fine from a headless process.

pub mod agents;
pub mod ask;
pub mod apps;
pub mod cancel;
pub mod clipboard;
pub mod commands;
pub mod db;
pub mod files;
pub mod hotkey;
pub mod icons;
pub mod launch;
pub mod mcp;
pub mod new_agent;
pub mod onboarding;
pub mod permissions;
pub mod preferences;
pub mod provider;
pub mod schedules;
pub mod search;
pub mod settings;
pub mod themes;
pub mod usage;

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
