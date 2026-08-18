//! Commands and modes — the reusable concept a command's row
//! (`SearchItem::enters_mode`) transitions the panel into. Split the same
//! way `onboarding/state.rs` splits from `onboarding/view.rs`: this module
//! is pure, GPUI/daemon-IO-free state and geometry math, unit-testable with
//! plain `#[test]`s; `panel.rs`'s `Root` is the view layer that drives real
//! I/O (the `Request::Search` round-trip, the text field, the actual
//! window resize) from it.
//!
//! **What a mode is, concretely.** Confirming a command row doesn't call
//! `Request::Activate` at all — `panel::Root::confirm` checks
//! `SearchItem::enters_mode` first and, if set, transitions client-side
//! into that mode instead. From then on:
//! - every keystroke searches with `Request::Search`'s `provider` field set
//!   to [`ModeChrome::provider_id`] — the mode's own provider, scoped, not
//!   the merged root list;
//! - the search field's placeholder becomes [`ModeChrome::placeholder`];
//! - the footer's left side becomes [`ModeChrome::title`] instead of the
//!   selected row's own title;
//! - the panel widens to `theme::PANEL_WIDTH_WITH_DETAIL_PX` if
//!   [`ModeChrome::has_detail`], and a second column (the selected row's
//!   own preview + fields) appears alongside the list.
//!
//! Escape, or the panel's back affordance, exits: the panel narrows back
//! (if it had widened), the placeholder reverts, and the query the captain
//! had typed *before* entering the mode is restored verbatim — entering and
//! leaving a mode must never lose what was already being searched for.
//!
//! **What a second command has to implement — the same accounting
//! `settings.rs`'s own doc comment gives for a fourth provider**: one
//! `impl Provider` for its own list (exactly like any other provider — no
//! mode-specific trait exists), one `SearchItem` in some provider's
//! `search()` with `enters_mode: Some(new_mode_id)`, and one more
//! [`ModeChrome`] entry in [`MODES`] below (placeholder text, footer title,
//! whether it wants a detail pane). It does *not* need to touch: the wire
//! protocol (`Request::Search`'s `provider` field and `Request::Activate`'s
//! `action` field are already generic), `panel.rs`'s mode-transition logic
//! (`enter_mode`/`exit_mode`/`run_search` branch on "is a mode active,"
//! never on which one), or the window-resize plumbing (already keyed off
//! `ModeChrome::has_detail`, a plain bool). **What it does still have to
//! write itself**: the mode's own *detail pane content*, if it wants one —
//! `render_mode_detail` in `panel.rs` renders clipboard's specific three
//! fields (Application/Content Type/Copied) because a detail pane's
//! content is inherently mode-specific (Raycast's own command details all
//! differ from each other too) — there is no generic "detail pane
//! renderer" and building one speculatively, for a second command that
//! doesn't exist yet, is exactly the kind of general framework the launch
//! brief's "prefer the smaller abstraction" guidance rules out. That is the
//! one real cost a second command with its own detail view pays; a second
//! command with a plain list and no detail pane (`has_detail: false`) pays
//! nothing beyond the two bullet points above.

/// Static chrome for one mode — UI copy a person reads, not list data (list
/// data always arrives through `Provider::search`, scoped by
/// `Request::Search`'s `provider` field). This is the one place that
/// legitimately names a specific mode by id: unlike `SearchItem`'s fields
/// (rendering *data* every provider already carries symmetrically), a
/// mode's placeholder text and footer title are copy specific to that one
/// mode, the same way `Provider::section_label`/`action_label` are already
/// copy specific to one provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeChrome {
    /// Matches `SearchItem::enters_mode` and, today, also the scoped
    /// provider's own `Provider::id()` — a mode's list is always exactly
    /// one provider's own search, so the two happen to coincide for every
    /// mode that exists so far. A future mode that isn't "browse one
    /// provider's data" (if one is ever needed) would be the moment these
    /// two ids diverge; nothing here assumes they can't.
    pub id: &'static str,
    /// The provider `Request::Search`'s `provider` field scopes to while
    /// this mode is active.
    pub provider_id: &'static str,
    /// The footer's left-side label while this mode is active, e.g.
    /// `"Clipboard History"`.
    pub title: &'static str,
    /// The search field's placeholder while this mode is active.
    pub placeholder: &'static str,
    /// Whether this mode widens the panel and shows a second (preview)
    /// column — see this module's doc comment.
    pub has_detail: bool,
}

pub const MODES: &[ModeChrome] = &[ModeChrome {
    id: "clipboard",
    provider_id: "clipboard",
    title: "Clipboard History",
    placeholder: "Type to filter entries…",
    has_detail: true,
}];

/// Looks up a mode's chrome by id — `None` for an id that doesn't name a
/// real mode (a stale/corrupted `enters_mode` value; `panel::Root::confirm`
/// simply does nothing in that case rather than entering a broken mode).
pub fn chrome_for(mode_id: &str) -> Option<&'static ModeChrome> {
    MODES.iter().find(|m| m.id == mode_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clipboard_mode_is_registered_and_scopes_to_the_clipboard_provider() {
        let chrome = chrome_for("clipboard").expect("the clipboard mode must be registered");
        assert_eq!(chrome.provider_id, "clipboard");
        assert_eq!(chrome.title, "Clipboard History");
        assert!(chrome.has_detail);
    }

    #[test]
    fn an_unknown_mode_id_resolves_to_nothing() {
        assert!(chrome_for("does-not-exist").is_none());
    }
}
