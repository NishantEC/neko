//! The six-step first-run setup: a real,
//! ordinary chrome window — never the borderless summon popup — walking
//! the captain through both permission asks and hotkey capture exactly
//! once. See `state` for the pure step machine and `view` for the GPUI
//! window that drives it.

pub mod state;
pub mod view;

pub use view::{SharedAccessibility, SharedHotkeyController, SharedOnboardingSlot, open_window};
