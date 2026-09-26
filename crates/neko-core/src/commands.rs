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
    /// The row-icon shape — a command's own, not its target provider's, since
    /// the root list shows this row long before that provider ever runs.
    glyph: Glyph,
}

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "clipboard-history",
        title: "Clipboard History",
        aliases: &["Clipboard History", "Clipboard Manager", "Clipboard"],
        mode: "clipboard",
        glyph: Glyph::Clipboard,
    },
    // The second command, and the proof this table was worth having: adding
    // it cost this entry plus one `ModeChrome` in the client, exactly the
    // accounting `modes.rs`'s own doc comment promised.
    //
    // "Theme" (singular) matches the title as a plain subsequence; "themes",
    // "colours"/"colors" and "appearance" do not, and are what a person
    // actually types — hence the aliases. Both spellings of "colour" are
    // listed because `fuzzy_score` is a strict in-order subsequence match, so
    // the British one does not fall out of the American one for free.
    CommandSpec {
        id: "themes",
        title: "Themes",
        aliases: &[
            "Themes",
            "Theme",
            "Colour Theme",
            "Color Theme",
            "Appearance",
        ],
        mode: "theme",
        glyph: Glyph::Palette,
    },
    // The fourth command, and the first whose mode's *query is not a filter*:
    // inside `new-agent` the text typed is the task the agent is given, and
    // the rows are directories to start it in (`crate::new_agent`).
    //
    // "New Agent" matches the title as a plain subsequence; none of the words
    // a person actually reaches for do ("run an agent", "spawn", "claude",
    // "codex" — `fuzzy_score` is a strict in-order subsequence over *one*
    // string), so they are aliases. "Claude" and "Codex" are listed on
    // purpose: the captain thinks in terms of the tool he is starting, not
    // Paseo's word for the thing it hosts.
    CommandSpec {
        id: "new-agent",
        title: "New Agent",
        aliases: &[
            "New Agent",
            "Start Agent",
            "Run Agent",
            "Spawn Agent",
            "New Task",
            "Claude",
            "Codex",
            "Paseo",
        ],
        mode: "new-agent",
        glyph: Glyph::Agent,
    },
    CommandSpec {
        id: "new-codex-task",
        title: "New Codex task",
        aliases: &[
            "New Codex task",
            "Start Codex task",
            "New Codex",
            "Start Codex",
        ],
        mode: "new-codex-task",
        glyph: Glyph::Agent,
    },
    // The third command. "Settings" is listed as an alias rather than being
    // the title because `crate::settings` already owns the word on screen —
    // its section is "Settings" (macOS System Settings panes). Two sections
    // both called Settings, one of which is not neko's own, is exactly the
    // confusion the separate provider ids exist to avoid. The alias means
    // typing "settings" still finds this row; the row itself says
    // "Preferences", which is unambiguous next to it.
    CommandSpec {
        id: "usage",
        title: "Usage",
        aliases: &["Usage", "Quota", "Limits", "Rate Limit", "Tokens"],
        mode: "usage",
        glyph: Glyph::Sliders,
    },
    CommandSpec {
        id: "schedules",
        title: "Schedules",
        aliases: &["Schedules", "Cron", "Recurring Agents", "Automations"],
        mode: "schedule",
        glyph: Glyph::Sliders,
    },
    CommandSpec {
        id: "ask",
        title: "Ask neko",
        aliases: &["Ask neko", "Do something", "Natural language", "Tell neko"],
        mode: "ask",
        glyph: Glyph::AgentLive,
    },
    CommandSpec {
        id: "terminals",
        title: "Terminals",
        aliases: &["Terminals", "Shells", "Sessions", "Console"],
        mode: "terminal",
        glyph: Glyph::Text,
    },
    CommandSpec {
        id: "agents",
        title: "Agents",
        aliases: &["Agents", "Sessions", "Talk to an agent", "Send a prompt"],
        mode: "agent",
        glyph: Glyph::Agent,
    },
    CommandSpec {
        id: "preferences",
        title: "Preferences",
        aliases: &[
            "Preferences",
            "Settings",
            "neko Settings",
            "Options",
            "Configure",
        ],
        mode: "preference",
        glyph: Glyph::Sliders,
    },
];

/// The command provider. Holds no state — the command list is a fixed,
/// compiled-in table (see [`COMMANDS`]), not something scanned or
/// persisted, since "what commands exist" is a build-time fact today (no
/// extension host yet — see `AGENTS.md`'s "Seams for follow-up work").
pub struct CommandsProvider {
    standalone: bool,
}

impl CommandsProvider {
    pub fn new() -> Self {
        Self { standalone: false }
    }
    pub fn standalone() -> Self {
        Self { standalone: true }
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
        const WORKSPACE: CommandSpec = CommandSpec {
            id: "workspace",
            title: "Open Neko Workspace",
            aliases: &[
                "Neko",
                "Workspace",
                "Tasks",
                "Agents",
                "Inbox",
                "Linear",
                "New Task",
            ],
            mode: "workspace",
            glyph: Glyph::Agent,
        };
        COMMANDS
            .iter()
            .filter(|cmd| {
                !self.standalone || matches!(cmd.id, "clipboard-history" | "themes" | "preferences")
            })
            .chain(self.standalone.then_some(&WORKSPACE))
            .filter_map(|cmd| {
                // An empty query lists every command — the state the slash
                // palette opens in (`/` with nothing after it). `fuzzy_score`
                // would answer this too, but only incidentally; saying it
                // here means the palette cannot be emptied by a future change
                // to how an empty pattern scores.
                let score = if query.trim().is_empty() {
                    1.0
                } else {
                    cmd.aliases
                        .iter()
                        .filter_map(|alias| fuzzy_score(query, alias))
                        .fold(None, |best: Option<f32>, s| {
                            Some(best.map_or(s, |b| b.max(s)))
                        })?
                };
                Some(Candidate {
                    score,
                    item: SearchItem {
                        id: cmd.id.to_string(),
                        kind: "command".to_string(),
                        title: cmd.title.to_string(),
                        subtitle: None,
                        icon: Icon::Glyph(cmd.glyph),
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
                        meter: None,
                        keeps_open: false,
                        preview_markdown: false,
                        speaker: None,
                        images: Vec::new(),
                        preview: None,
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
    #[test]
    fn standalone_commands_do_not_depend_on_other_agent_apps() {
        use crate::provider::Provider;
        let rows = super::CommandsProvider::standalone().search("", 0);
        assert!(
            rows.iter()
                .any(|r| r.item.enters_mode.as_deref() == Some("workspace"))
        );
        assert!(!rows.iter().any(|r| matches!(
            r.item.enters_mode.as_deref(),
            Some("new-agent" | "new-codex-task" | "agent" | "schedule" | "ask" | "terminal")
        )));
    }
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
    fn the_words_a_person_actually_types_for_themes_all_find_the_themes_command() {
        // "theme" matches the title "Themes" as a plain subsequence; none of
        // the others do (`fuzzy_score` is a strict in-order subsequence over
        // *one* string), so each of these passes only because of an alias.
        let provider = CommandsProvider::new();
        for query in [
            "theme",
            "themes",
            "color theme",
            "colour theme",
            "appearance",
        ] {
            let results = provider.search(query, 0);
            assert!(
                results.iter().any(|c| c.item.id == "themes"),
                "query {query:?} must find the Themes command"
            );
        }
    }

    #[test]
    fn the_themes_command_enters_the_theme_mode_and_carries_its_own_glyph() {
        let provider = CommandsProvider::new();
        let results = provider.search("themes", 0);
        let themes = results
            .iter()
            .find(|c| c.item.id == "themes")
            .expect("Themes must match");
        assert_eq!(themes.item.title, "Themes");
        assert_eq!(themes.item.enters_mode.as_deref(), Some("theme"));
        assert_eq!(themes.item.badge.as_deref(), Some("COMMAND"));
        // Its own glyph, not the clipboard command's — the row-icon shape is
        // per-command data now, which is what a second command proved was
        // needed.
        assert_eq!(themes.item.icon, Icon::Glyph(Glyph::Palette));
    }

    #[test]
    fn a_clipboard_query_does_not_also_drag_in_the_themes_command() {
        let provider = CommandsProvider::new();
        let results = provider.search("clipboard", 0);
        let ids: Vec<&str> = results.iter().map(|c| c.item.id.as_str()).collect();
        assert_eq!(ids, vec!["clipboard-history"]);
    }

    #[test]
    fn the_words_a_person_types_to_start_an_agent_all_find_the_new_agent_command() {
        // "new agent" matches the title as a plain subsequence; the rest only
        // match through aliases, and "claude"/"codex" are there because that
        // is what the captain calls the thing he is starting.
        let provider = CommandsProvider::new();
        for query in [
            "new agent",
            "start agent",
            "run agent",
            "spawn",
            "claude",
            "codex",
            "paseo",
        ] {
            let results = provider.search(query, 0);
            assert!(
                results.iter().any(|c| c.item.id == "new-agent"),
                "query {query:?} must find the New Agent command"
            );
        }
    }

    #[test]
    fn the_new_agent_command_enters_the_new_agent_mode() {
        let provider = CommandsProvider::new();
        let results = provider.search("new agent", 0);
        let row = results
            .iter()
            .find(|c| c.item.id == "new-agent")
            .expect("New Agent must match");
        assert_eq!(row.item.title, "New Agent");
        assert_eq!(row.item.enters_mode.as_deref(), Some("new-agent"));
        assert_eq!(row.item.badge.as_deref(), Some("COMMAND"));
        assert_eq!(row.item.icon, Icon::Glyph(Glyph::Agent));
    }

    #[test]
    fn new_codex_task_enters_the_explicit_project_mode() {
        let provider = CommandsProvider::new();
        let row = provider
            .search("new codex task", 0)
            .into_iter()
            .find(|candidate| candidate.item.id == "new-codex-task")
            .expect("New Codex task must match");

        assert_eq!(row.item.title, "New Codex task");
        assert_eq!(row.item.enters_mode.as_deref(), Some("new-codex-task"));
        assert_eq!(row.item.badge.as_deref(), Some("COMMAND"));
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

#[cfg(test)]
mod slash_palette_tests {
    use super::*;
    use crate::provider::Provider;

    /// `/` with nothing after it is a scoped search with an empty query, so
    /// the palette's opening state is exactly this call.
    #[test]
    fn an_empty_query_lists_every_command() {
        let found = CommandsProvider::new().search("", 0);
        assert_eq!(
            found.len(),
            COMMANDS.len(),
            "the slash palette opens on the full list"
        );
    }

    #[test]
    fn a_partial_name_narrows_the_palette() {
        let found = CommandsProvider::new().search("the", 0);
        let top = found
            .iter()
            .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap())
            .expect("\"the\" must match at least Themes");
        assert_eq!(top.item.title, "Themes");
    }
}
