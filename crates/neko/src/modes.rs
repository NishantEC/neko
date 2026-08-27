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
    /// What to say when this mode has no rows.
    ///
    /// Every mode used to render `panel::NO_MATCHES` — "No matches, try
    /// fewer characters" — including the ones where nothing had been typed
    /// and there was nothing to shorten. Terminals and Schedules are empty
    /// on a machine with none, and a mode whose provider swallows a
    /// transport error is empty when Paseo is simply down; all of them told
    /// you to delete characters you never typed.
    pub empty_line: &'static str,
    /// Whether this mode widens the panel and shows a second (preview)
    /// column — see this module's doc comment.
    pub has_detail: bool,
    /// Renders as one scrolling chat transcript — user bubbles, agent prose,
    /// tool chips — instead of the list (or list+detail) every other mode
    /// uses. The panel branches on this, never on the provider's id: chrome
    /// is the client's own layout vocabulary, exactly like `has_detail`.
    pub transcript: bool,
}

pub const MODES: &[ModeChrome] = &[
    ModeChrome {
        id: "clipboard",
        provider_id: "clipboard",
        title: "Clipboard History",
        placeholder: "Type to filter entries…",
        empty_line: "Nothing copied yet \u{2014} anything you copy shows up here.",
        has_detail: true,
        transcript: false,
    },
    // The second mode, and the one that proved the accounting above: it cost
    // this entry, one `CommandSpec` in `neko_core::commands`, and one
    // `Provider` — no change at all to `enter_mode`/`exit_mode`,
    // `run_search`'s scoping branch, or the wire protocol.
    //
    // `has_detail: false` is a real design choice, not a shortcut. A detail
    // pane would show a theme's *description* next to its name; the panel
    // itself already shows the theme, live, in full, as you arrow — which is
    // strictly more informative than any pane could be. A second column here
    // would take 496px away from the very surface the captain is trying to
    // look at.
    ModeChrome {
        id: "theme",
        provider_id: "theme",
        title: "Themes",
        placeholder: "Type to filter themes…",
        empty_line: "No theme matches that.",
        has_detail: false,
        transcript: false,
    },
    // The third mode, and the first where **the query is not a filter**: what
    // is typed here is the task the agent is given, and the rows are the
    // directories it could work in (`neko_core::new_agent`). Nothing in this
    // module needed a new field to express that — a mode has always been "one
    // provider's own list, scoped by `Request::Search`'s `provider`", and that
    // provider is free to ignore the query when ranking. The placeholder is
    // the only place the difference is stated to a person, which is why it
    // reads as an instruction rather than as "type to filter…".
    //
    // `has_detail: false`: the second column would show a project's own
    // details, and the thing being composed is the *prompt*, which lives in
    // the input row. A detail pane here would take 496px to say nothing the
    // row does not.
    ModeChrome {
        id: "new-agent",
        provider_id: "new-agent",
        title: "New Agent",
        placeholder: "Describe the task, then pick where to run it…",
        empty_line: "No projects \u{2014} start an agent from Paseo once and this fills in.",
        has_detail: false,
        transcript: false,
    },
    // Reads the model provider's own quota API — see
    // `neko_core::usage`. A status pane rather than a list, so the typed
    // query is ignored and there is no detail column to fill.
    // Paseo's own cron: agents that start themselves. Enter toggles pause
    // — see `neko_core::schedules` for why not "Run now".
    // Say what you want; neko proposes one tool call and runs nothing until
    // you confirm it. See `neko_core::ask`.
    // Paseo's supervised shells. `has_detail` because the point is reading
    // what one last said, which is what the pane is for.
    // Every agent, with the keyboard pointed at the thing you came to do:
    // say something else to it. The query is the prompt, not a filter —
    // the same rule the New Agent mode follows.
    // What one agent has been doing, read here. `has_detail` because a long
    // reply is worth a pane rather than a truncated row.
    ModeChrome {
        id: "conversation",
        provider_id: "conversation",
        title: "Conversation",
        placeholder: "Reading the conversation\u{2026}",
        empty_line: "This agent has not said anything yet.",
        // The chat layout, not list+detail: a conversation is read as one
        // scrolling exchange, the way Paseo's own agent view and every
        // messaging surface draw it — not as rows about a transcript with
        // the transcript in a side pane.
        has_detail: false,
        transcript: true,
    },
    ModeChrome {
        id: "agent",
        provider_id: "agent-control",
        title: "Agents",
        placeholder: "Say something to the selected agent\u{2026}",
        empty_line: "No agents running.",
        has_detail: false,
        transcript: false,
    },
    ModeChrome {
        id: "terminal",
        provider_id: "terminal",
        title: "Terminals",
        placeholder: "Type to filter terminals\u{2026}",
        empty_line: "No terminals open \u{2014} start one in Paseo and it appears here.",
        has_detail: true,
        transcript: false,
    },
    ModeChrome {
        id: "ask",
        provider_id: "ask",
        title: "Ask neko",
        placeholder: "Say what you want done\u{2026}",
        empty_line: "Nothing to show yet.",
        has_detail: false,
        transcript: false,
    },
    ModeChrome {
        id: "schedule",
        provider_id: "schedule",
        title: "Schedules",
        placeholder: "Type to filter schedules\u{2026}",
        empty_line: "No schedules \u{2014} create one in Paseo and it appears here.",
        has_detail: false,
        transcript: false,
    },
    ModeChrome {
        id: "usage",
        provider_id: "usage",
        title: "Usage",
        placeholder: "Claude Code usage",
        empty_line: "No quota to show \u{2014} sign in with `claude`, `codex` or `grok`.",
        has_detail: false,
        transcript: false,
    },
];

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
    fn the_theme_mode_is_registered_scopes_to_the_theme_provider_and_has_no_detail_pane() {
        let chrome = chrome_for("theme").expect("the theme mode must be registered");
        assert_eq!(chrome.provider_id, "theme");
        assert_eq!(chrome.title, "Themes");
        assert!(
            !chrome.has_detail,
            "the theme mode's preview is the whole panel; a detail column would cover it"
        );
    }

    #[test]
    fn every_mode_id_is_unique_and_names_a_real_provider_scope() {
        let mut ids: Vec<&str> = MODES.iter().map(|m| m.id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "duplicate mode id");
        for m in MODES {
            assert!(!m.provider_id.is_empty());
            assert!(!m.placeholder.is_empty());
        }
    }

    #[test]
    fn preferences_is_a_window_and_therefore_not_a_mode_at_all() {
        // The `Preferences` command still carries `enters_mode: "preference"`
        // on the wire — from the provider's side "this row changes the UI" is
        // one statement — but the client resolves that to a real window, so
        // there must be no mode of that name for it to fall into instead.
        for id in ["preference", "preference.hotkey", "preference.folders"] {
            assert!(chrome_for(id).is_none(), "{id} must not be a mode");
        }
    }

    #[test]
    fn the_new_agent_mode_is_registered_and_says_the_query_is_a_task_not_a_filter() {
        let chrome = chrome_for("new-agent").expect("the new-agent mode must be registered");
        assert_eq!(chrome.provider_id, "new-agent");
        assert_eq!(chrome.title, "New Agent");
        assert!(!chrome.has_detail);
        // The placeholder is the only surface that tells a person the field is
        // not a filter here, so it must not read like every other mode's.
        assert!(
            !chrome.placeholder.contains("filter"),
            "the new-agent field is the prompt; calling it a filter would be a lie"
        );
    }

    #[test]
    fn an_unknown_mode_id_resolves_to_nothing() {
        assert!(chrome_for("does-not-exist").is_none());
    }
}
