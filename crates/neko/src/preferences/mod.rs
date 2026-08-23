//! neko's Preferences window.
//!
//! **A real window, not a mode.** This was built as an in-panel mode first —
//! a sidebar with the settings on the left and each setting edited in the
//! detail pane — and replaced on the captain's own instruction after he
//! compared it against Raycast's settings window. What that comparison
//! exposes is not decoration: a launcher panel is a *transient* surface (it
//! hides on click-outside, its field is a query, its Enter is "do the thing
//! and get out of the way"), and settings are the opposite of transient.
//! Editing them wants a window you can leave open beside what you are
//! configuring, a real title bar, and controls that look like controls.
//!
//! The split mirrors `crate::onboarding`, for the same reason it does there:
//! [`state`] is pure, side-effect-free, unit-testable state (which tab is
//! showing, what a recorded key press means); [`view`] is the GPUI window and
//! every piece of real I/O — daemon requests, the live hotkey registration.
//!
//! **Nothing daemon-side changed to make this a window.** The settings still
//! live in `neko_core::preferences` behind the ordinary `Provider` seam, and
//! this window reads and writes them through exactly the requests the panel
//! already used: a scoped `Request::Search` per list, `Request::Activate` per
//! change. That is the point of the seam — the surface is the client's own
//! business.

pub mod state;
pub mod view;

use std::cell::RefCell;
use std::rc::Rc;

pub use view::PreferencesRoot;

/// The `SearchItem::enters_mode` value the `Preferences` command carries.
///
/// It is still spelled as a mode id on the wire even though it opens a
/// window, because from the provider's side the statement is the same one
/// every command makes: *confirming this row changes the UI rather than
/// performing a daemon action*. Which surface that turns out to be is the
/// client's own business, and `panel::Root::confirm` is where the two part
/// company.
pub const PREFERENCES_MODE_ID: &str = "preference";

/// The live Preferences window, if one is open.
///
/// Confirming the row again focuses that window rather than opening a
/// second — the same "never two of the same surface" rule
/// `onboarding::SharedOnboardingSlot` follows, and the reason `App::on_reopen`
/// can route a Dock click to whichever window is actually live.
pub type SharedPreferencesSlot = Rc<RefCell<Option<gpui::WindowHandle<PreferencesRoot>>>>;
