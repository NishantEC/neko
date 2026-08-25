//! Theme selection — the sixth provider, and the second one (after
//! `commands`) whose rows are about the app itself rather than about
//! something on the machine.
//!
//! **The split, and why it is where it is.** A palette is 20 `gpui::Rgba`
//! values; the daemon has no use for a single one of them and
//! `neko-protocol` must stay inert (see that crate's own doc comment). So the
//! colours live entirely in the `neko` app crate (`theme.rs`), and what
//! crosses the wire is only the identity: `neko_protocol::BUILTIN_THEMES`, a
//! compiled-in `(id, name, description)` table that both sides read. This
//! provider is the daemon's half — it makes those names *searchable* and
//! *persistable*, nothing more. `theme::tests::themes_match_the_protocol_registry`
//! (client side) pins the two halves together so they cannot drift.
//!
//! **`activate` persists, it does not paint.** By the time
//! `Request::Activate { kind: "theme", .. }` reaches here the client has
//! already been rendering that palette for however long the captain has been
//! arrowing over it — live preview is client-local and needs no round-trip.
//! Confirming is the moment the choice becomes durable, which is exactly the
//! one part that has to be daemon-side, because the daemon is the single
//! SQLite writer. This is why there is no `Request::SetTheme`: the generic
//! activate path already means "make this row's thing happen", and for a
//! theme row that is "remember it".

use neko_protocol::{BUILTIN_THEMES, DEFAULT_THEME_ID, Glyph, Icon, SearchItem};

use crate::provider::{Provider, ProviderError};
use crate::search::{Candidate, fuzzy_score};

/// The KV key the chosen theme id is stored under, same `settings` table and
/// same shape as `neko_core::hotkey` and `neko_core::onboarding`.
const THEME_KEY: &str = "theme";

/// The persisted theme id, or [`DEFAULT_THEME_ID`] when nothing is stored yet.
///
/// A stored id that no longer names a built-in (downgrade, or a theme removed
/// between releases) also reads as the default rather than as an error — the
/// client would ignore it anyway (`theme::set_active` returns `false` and
/// changes nothing), and reporting a hard error for a cosmetic setting would
/// be worse than quietly rendering the palette everybody already has.
pub fn get_theme(db: &crate::Db) -> rusqlite::Result<String> {
    Ok(db
        .get_setting(THEME_KEY)?
        .filter(|id| neko_protocol::builtin_theme(id).is_some())
        .unwrap_or_else(|| DEFAULT_THEME_ID.to_string()))
}

pub fn set_theme(db: &crate::Db, id: &str) -> Result<String, ProviderError> {
    if neko_protocol::builtin_theme(id).is_none() {
        return Err(ProviderError(format!("no such theme: {id}")));
    }
    db.set_setting(THEME_KEY, id)
        .map_err(|e| ProviderError(e.to_string()))?;
    Ok(id.to_string())
}

/// The provider behind the `Themes` mode's own list. Holds the database
/// handle only — the theme table itself is compiled in
/// (`neko_protocol::BUILTIN_THEMES`), so `search` does no I/O at all and this
/// provider is exactly as fast and hermetic in a test as in the real daemon.
pub struct ThemesProvider {
    db: std::sync::Arc<std::sync::Mutex<crate::Db>>,
}

impl ThemesProvider {
    pub fn new(db: std::sync::Arc<std::sync::Mutex<crate::Db>>) -> Self {
        Self { db }
    }
}

impl Provider for ThemesProvider {
    fn id(&self) -> &'static str {
        "theme"
    }

    fn section_label(&self) -> &'static str {
        "Themes"
    }

    /// Seventeen palettes are not a useful answer to "I have not typed
    /// anything yet" — this file's own comment named that distinction long
    /// before anything acted on it, and until `answers_empty_root_query`
    /// existed there was no way for a provider to express it.
    fn answers_empty_root_query(&self) -> bool {
        false
    }

    /// An empty query lists **every** theme, in registry order, rather than
    /// nothing. That is the whole point of the mode: entering it clears the
    /// query (`panel::Root::enter_mode`), and a browsable list of palettes to
    /// arrow through is the feature.
    fn search(&self, query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        let active = self
            .db
            .lock()
            .ok()
            .and_then(|db| get_theme(&db).ok())
            .unwrap_or_else(|| DEFAULT_THEME_ID.to_string());

        BUILTIN_THEMES
            .iter()
            .enumerate()
            .filter_map(|(index, theme)| {
                // Registry order is meaningful (neko's own palettes first,
                // the default leading), so an unfiltered list must not be
                // resorted by an arbitrary equal score. Scoring by descending
                // index keeps the table's own order through `handle_request`'s
                // score sort; a real query overrides it with real relevance.
                let score = if query.trim().is_empty() {
                    (BUILTIN_THEMES.len() - index) as f32
                } else {
                    // Best of "Rosé Pine Dawn" and "rose-pine-dawn": the
                    // display name is what a person reads, but an accented or
                    // spaced name can be unreachable from a plain-ASCII query
                    // that the id matches perfectly.
                    match (fuzzy_score(query, theme.name), fuzzy_score(query, theme.id)) {
                        (Some(a), Some(b)) => a.max(b),
                        (Some(a), None) => a,
                        (None, Some(b)) => b,
                        (None, None) => return None,
                    }
                };
                Some(Candidate {
                    score,
                    item: SearchItem {
                        id: theme.id.to_string(),
                        kind: "theme".to_string(),
                        title: theme.name.to_string(),
                        subtitle: Some(theme.description.to_string()),
                        icon: Icon::Glyph(Glyph::Palette),
                        section_label: "Themes".to_string(),
                        action_label: "Use Theme  ↵".to_string(),
                        badge: (theme.id == active).then(|| "CURRENT".to_string()),
                        accessory: None,
                        enters_mode: None,
                        group_label: None,
                        actions: Vec::new(),
                        source: None,
                        meter: None,
                        keeps_open: false,
                        preview_markdown: false,
                        preview: None,
                    },
                })
            })
            .collect()
    }

    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        let db = self
            .db
            .lock()
            .map_err(|_| ProviderError("theme settings unavailable".to_string()))?;
        set_theme(&db, id).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;
    use std::sync::{Arc, Mutex};

    fn provider() -> (ThemesProvider, Arc<Mutex<Db>>) {
        let db = Arc::new(Mutex::new(Db::open_in_memory().unwrap()));
        (ThemesProvider::new(db.clone()), db)
    }

    #[test]
    fn an_empty_query_lists_every_built_in_theme_in_registry_order() {
        let (p, _db) = provider();
        let results = p.search("", 0);
        assert_eq!(results.len(), BUILTIN_THEMES.len());
        // Descending score == registry order once the daemon sorts by score.
        let mut sorted = results.clone();
        sorted.sort_by(|a, b| b.score.total_cmp(&a.score));
        let ids: Vec<&str> = sorted.iter().map(|c| c.item.id.as_str()).collect();
        let expected: Vec<&str> = BUILTIN_THEMES.iter().map(|t| t.id).collect();
        assert_eq!(ids, expected);
    }

    #[test]
    fn the_default_theme_is_marked_current_before_anything_is_ever_chosen() {
        let (p, _db) = provider();
        let results = p.search("", 0);
        let current: Vec<&str> = results
            .iter()
            .filter(|c| c.item.badge.as_deref() == Some("CURRENT"))
            .map(|c| c.item.id.as_str())
            .collect();
        assert_eq!(current, vec![DEFAULT_THEME_ID]);
    }

    #[test]
    fn activating_a_theme_persists_it_and_moves_the_current_badge() {
        let (p, db) = provider();
        p.activate("gruvbox-dark").unwrap();
        assert_eq!(get_theme(&db.lock().unwrap()).unwrap(), "gruvbox-dark");
        let results = p.search("", 0);
        let current: Vec<&str> = results
            .iter()
            .filter(|c| c.item.badge.as_deref() == Some("CURRENT"))
            .map(|c| c.item.id.as_str())
            .collect();
        assert_eq!(current, vec!["gruvbox-dark"]);
    }

    #[test]
    fn activating_a_theme_that_does_not_exist_is_an_error_not_a_silent_write() {
        let (p, db) = provider();
        assert!(p.activate("hot-pink").is_err());
        assert_eq!(get_theme(&db.lock().unwrap()).unwrap(), DEFAULT_THEME_ID);
    }

    #[test]
    fn a_persisted_id_that_no_longer_names_a_theme_reads_as_the_default() {
        let (_p, db) = provider();
        // Written directly, bypassing `set_theme`'s validation — this models a
        // downgrade, or a theme removed between releases, not a bad call.
        db.lock().unwrap().set_setting(THEME_KEY, "removed-in-a-later-build").unwrap();
        assert_eq!(get_theme(&db.lock().unwrap()).unwrap(), DEFAULT_THEME_ID);
    }

    #[test]
    fn a_query_finds_a_theme_by_display_name() {
        let (p, _db) = provider();
        let results = p.search("mocha", 0);
        assert!(results.iter().any(|c| c.item.id == "catppuccin-mocha"));
    }

    #[test]
    fn a_query_finds_a_theme_by_its_id_too_not_just_its_display_name() {
        // "rose-pine-dawn" is the id; the display name is "Rosé Pine Dawn",
        // whose accented 'é' is not in the id — matching the id as well is
        // what makes a plain-ASCII query reach it.
        let (p, _db) = provider();
        let results = p.search("rose-pine-dawn", 0);
        assert!(results.iter().any(|c| c.item.id == "rose-pine-dawn"));
    }

    #[test]
    fn a_query_that_matches_nothing_returns_nothing_unlike_the_empty_query() {
        let (p, _db) = provider();
        assert!(p.search("zzzzqqq", 0).is_empty());
    }

    #[test]
    fn every_row_carries_the_rendering_data_the_client_needs_and_enters_no_mode() {
        let (p, _db) = provider();
        for c in p.search("", 0) {
            assert_eq!(c.item.kind, "theme");
            assert_eq!(c.item.section_label, "Themes");
            assert_eq!(c.item.action_label, "Use Theme  ↵");
            assert!(c.item.subtitle.is_some());
            assert_eq!(c.item.enters_mode, None, "a theme row applies a theme; it does not open another mode");
        }
    }
}
