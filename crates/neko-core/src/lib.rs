//! Daemon-owned logic: the SQLite schema, the application index, search
//! ranking, launching, hotkey-setting persistence, and the `AgentProvider`
//! seam. Nothing in this crate talks to a socket — that's `neko-daemon`'s
//! job — and nothing in it talks to AppKit's window/event-loop APIs beyond
//! the read-only icon/launch calls below, which work fine from a headless
//! process.

pub mod agent;
pub mod apps;
pub mod clipboard;
pub mod db;
pub mod hotkey;
pub mod icons;
pub mod launch;
pub mod onboarding;
pub mod search;

pub use agent::{AgentProvider, FakeProvider};
pub use apps::AppEntry;
pub use db::Db;
