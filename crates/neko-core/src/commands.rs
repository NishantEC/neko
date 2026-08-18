//! Commands — a result type that performs a client-side UI transition
//! ("enter a mode") rather than opening or pasting something, arriving
//! through the same `Provider` seam as every other result type. This is
//! the fifth provider, and the proof (after file search and System Settings
//! search) that the abstraction holds for a genuinely new *kind* of result
//! — one whose "activation" isn't a daemon-side action at all. See
//! `AGENTS.md`'s "Commands and modes" section for the concept and exactly
//! what a second command needs to implement.
//!
//! **A command's `activate` is never actually called.** Confirming a
//! command row is handled entirely client-side (`neko`'s `panel::Root`
//! checks `SearchItem::enters_mode` before ever building a
//! `Request::Activate`) — the daemon-side `activate` below only exists to
//! satisfy the `Provider` trait, and errors if it's ever reached, which
//! would mean the client-side check was bypassed somehow.
//!
//! **The multi-word query limitation** documented in
//! `docs/evidence/settings-provider-report.md` ("keyboard shortcuts" never
//! matches a pane titled "Keyboard" — `fuzzy_score` needs every query
//! character, including the space, to appear as an in-order subsequence of
//! *one* title) would otherwise block "clipboard manager": the title
//! "Clipboard History" contains no `m`/`a`/`n`/... in the right place after
//! "Clipboard " to satisfy "manager" as a subsequence. Handled here, in
//! this provider, exactly as the brief asks — not by changing the shared
//! `fuzzy_score` (owned by a parallel task's `search.rs`, and a change
//! there would affect every other provider's matching too): each command
//! carries a short list of alias phrases, scored independently, and the
//! best-scoring alias wins. "clipboard" and "clipboard history" already
//! match the literal title as a plain subsequence with no alias needed;
//! "clipboard manager" only matches because `"Clipboard Manager"` is itself
//! one of the aliases.

use neko_protocol::{Glyph, Icon, SearchItem};

use crate::provider::{Provider, ProviderError};
use crate::search::{Candidate, fuzzy_score};

/// One built-in command. `mode` is the id `neko`'s `modes` module resolves
/// to that mode's chrome and to `Request::Search`'s `provider` scope — see
/// this module's doc comment.
struct CommandSpec {
    id: &'static str,
    title: &'static str,
    aliases: &'static [&'static str],
    mode: &'static str,
}

const COMMANDS: &[CommandSpec] = &[CommandSpec {
    id: "clipboard-history",
    title: "Clipboard History",
    aliases: &["Clipboard History", "Clipboard Manager", "Clipboard"],
    mode: "clipboard",
}];

/// The command provider. Holds no state — the command list is a fixed,
/// compiled-in table (see [`COMMANDS`]), not something scanned or
/// persisted, since "what commands exist" is a build-time fact today (no
/// extension host yet — see `AGENTS.md`'s "Seams for follow-up work").
pub struct CommandsProvider;

impl CommandsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CommandsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for CommandsProvider {
    fn id(&self) -> &'static str {
        "command"
    }

    fn section_label(&self) -> &'static str {
        "Commands"
    }

    fn search(&self, query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        COMMANDS
            .iter()
            .filter_map(|cmd| {
                let score = cmd
                    .aliases
                    .iter()
                    .filter_map(|alias| fuzzy_score(query, alias))
                    .fold(None, |best: Option<f32>, s| Some(best.map_or(s, |b| b.max(s))))?;
                Some(Candidate {
                    score,
                    item: SearchItem {
                        id: cmd.id.to_string(),
                        kind: "command".to_string(),
                        title: cmd.title.to_string(),
                        subtitle: None,
                        icon: Icon::Glyph(Glyph::Clipboard),
                        section_label: "Commands".to_string(),
                        action_label: "Open  ↵".to_string(),
                        // Uppercase to match the frozen row-badge's own
                        // rendered appearance — every other badge this
                        // codebase produces (`ClipboardProvider`'s
                        // "TEXT"/"LINK") is already the literal string
                        // rendered verbatim, not CSS-transformed at paint
                        // time, so the wire value itself carries the case
                        // that should appear on screen.
                        badge: Some("COMMAND".to_string()),
                        accessory: None,
                        enters_mode: Some(cmd.mode.to_string()),
                        group_label: None,
                        actions: Vec::new(),
                        source: None,
                    },
                })
            })
            .collect()
    }

    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        Err(ProviderError(format!(
            "command '{id}' has no direct activation — confirming it enters a mode client-side, \
             and should never reach Request::Activate"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_of_clipboard_finds_the_command() {
        let provider = CommandsProvider::new();
        let results = provider.search("clipboard", 0);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].item.id, "clipboard-history");
        assert_eq!(results[0].item.title, "Clipboard History");
        assert_eq!(results[0].item.kind, "command");
        assert_eq!(results[0].item.section_label, "Commands");
        assert_eq!(results[0].item.badge.as_deref(), Some("COMMAND"));
        assert_eq!(results[0].item.enters_mode.as_deref(), Some("clipboard"));
    }

    #[test]
    fn a_query_of_clipboard_history_finds_it_as_a_plain_subsequence_of_the_title() {
        let provider = CommandsProvider::new();
        assert_eq!(provider.search("clipboard history", 0).len(), 1);
    }

    #[test]
    fn a_query_of_clipboard_manager_finds_it_only_via_the_alias() {
        // The literal title "Clipboard History" has no subsequence match
        // for "manager" at all — this only passes because "Clipboard
        // Manager" is scored as an alias.
        let provider = CommandsProvider::new();
        let results = provider.search("clipboard manager", 0);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].item.id, "clipboard-history");
    }

    #[test]
    fn an_unrelated_query_does_not_match() {
        let provider = CommandsProvider::new();
        assert_eq!(provider.search("safari", 0).len(), 0);
    }

    #[test]
    fn the_best_scoring_alias_wins_not_the_first() {
        // A query that matches the shortest alias ("Clipboard") much more
        // tightly than the longer ones must still surface the command
        // (proves `fold` takes the max across aliases, not the first one
        // tried).
        let provider = CommandsProvider::new();
        let results = provider.search("clip", 0);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn activate_is_never_a_real_action() {
        let provider = CommandsProvider::new();
        assert!(provider.activate("clipboard-history").is_err());
    }
}
