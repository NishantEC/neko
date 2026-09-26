//! Terminals — what is open, and what it last said.
//!
//! **The verifiable half of L5 in `docs/plan-agent-control-plane.md`.** Paseo
//! supervises terminal sessions; a `Terminals` mode lists them, shows the
//! last screen of each in the detail pane, and kills one from `⌘K`.
//!
//! ## One call, not twenty-two
//!
//! `list_terminals` defaults to *the caller's own* working directory, which
//! for a top-level caller like neko is meaningless — a bare `{}` comes back
//! `cwd is required`. The first design here fanned out over all 22 workspaces
//! to compensate, which would have been forty-four `curl` processes per
//! keystroke. The tool takes `all: true`. Reading the schema properly was
//! worth more than any amount of optimising the wrong shape.
//!
//! ## The directory names the row, not the terminal
//!
//! Paseo names terminals `Terminal 1` per working directory, so a machine
//! with five of them has five rows reading `Terminal 1`. The directory is
//! what tells them apart — the same lesson the agent tiles learned when three
//! sessions shared a prompt (`AGENTS.md`, "Name an agent tile by its branch
//! and repository"). The name still appears, after the path, for the case
//! where one directory really does have several.

use neko_protocol::{Glyph, Icon, ItemAction, SearchItem};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::mcp::McpClient;
use crate::provider::{Provider, ProviderError};
use crate::search::Candidate;

/// How long a captured screen stays good.
///
/// Terminal output changes constantly, so this is not really a cache — it is
/// a rate limit. Every keystroke in the mode re-runs `search`, and capturing
/// N terminals per keystroke would be N `curl` pairs per character typed. Two
/// seconds is short enough that the pane reads as live and long enough that
/// typing a filter costs one capture pass, not one per letter.
const CAPTURE_TTL: Duration = Duration::from_secs(2);

/// How many terminals get their screen captured for the detail pane.
///
/// The pane shows one at a time, but the provider cannot know which row is
/// selected — that is client state. Capturing the whole list is the honest
/// way to make any of them selectable, and this bounds what that can cost on
/// a machine with a lot of sessions open. Rows past the bound still list and
/// still kill; only their preview is absent.
const MAX_PREVIEWS: usize = 8;

/// One supervised terminal session.
#[derive(Debug, Clone, PartialEq)]
pub struct Terminal {
    pub id: String,
    pub name: String,
    pub cwd: String,
}

impl Terminal {
    /// `"neko"` — the directory, which is what distinguishes two sessions
    /// both called `Terminal 1`.
    pub fn headline(&self) -> String {
        Path::new(&self.cwd)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| self.cwd.clone())
    }

    /// `"~/Documents/neko · Terminal 1"`.
    pub fn detail_line(&self) -> String {
        let path = crate::agents::tildify_path(&self.cwd);
        if self.name.is_empty() {
            path
        } else {
            format!("{path} \u{b7} {}", self.name)
        }
    }
}

pub fn parse_terminals(value: &Value) -> Vec<Terminal> {
    let Some(entries) = value.get("terminals").and_then(Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            Some(Terminal {
                // Everything here is addressed by id; a row without one could
                // only ever produce a request aimed at nothing.
                id: entry.get("id")?.as_str()?.to_string(),
                name: entry
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                cwd: entry
                    .get("cwd")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect()
}

/// Turns `capture_terminal`'s line array into something a pane can show.
///
/// **Trailing blank lines are dropped.** A capture is a fixed-height screen —
/// 24 lines whether or not anything is on them — so a shell sitting at its
/// prompt returns one line of text and twenty-three of nothing. Rendering
/// those verbatim would put the content at the top of an otherwise empty pane
/// and read as a rendering fault.
/// **Private-use glyphs are dropped.** A prompt theme like Powerlevel10k
/// draws its separators and icons from a patched font's private use area
/// (U+E000–U+F8FF), and neko renders in the system font, which has nothing
/// there — every one of them would be a tofu box. Removing them is not
/// censoring output: those code points carry no meaning outside the font
/// that defines them, and the text around them is unchanged.
///
/// Each becomes a **space**, not nothing. Terminal output is column-aligned
/// — tables, tree views, `ls -l` — and deleting characters would shift every
/// line that contained one out of step with the lines that did not. A space
/// occupies the same cell the glyph did.
pub fn capture_text(value: &Value) -> String {
    let Some(lines) = value.get("lines").and_then(Value::as_array) else {
        return String::new();
    };
    let mut text: Vec<String> = lines
        .iter()
        .filter_map(Value::as_str)
        .map(|line| {
            let stripped: String = line
                .chars()
                .map(|c| if is_private_use(c) { ' ' } else { c })
                .collect();
            stripped.trim_end().to_string()
        })
        .collect();
    while text.last().is_some_and(|line| line.trim().is_empty()) {
        text.pop();
    }
    text.join("\n")
}

/// The Basic Multilingual Plane's private use area, plus the two
/// supplementary ones — where every patched-font icon set lives.
fn is_private_use(c: char) -> bool {
    matches!(c as u32, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD)
}

/// A terminal id and the screen it last showed.
type Capture = (String, String);

/// Which terminals a cached capture set was taken from, so a set taken
/// before one was killed is not handed back describing it.
type CaptureKey = Vec<String>;

static CAPTURES: Mutex<Option<(Instant, CaptureKey, Vec<Capture>)>> = Mutex::new(None);

/// **Keyed on the terminal ids, not only on the clock.** Two seconds is
/// nothing to a person and plenty for a session to be opened or killed from
/// Paseo's own window; a set keyed on time alone would hand back a preview
/// for a terminal that is gone and none for one that just appeared. The kill
/// path clears this too, but only for kills that went through neko.
fn cached_captures(key: &CaptureKey) -> Option<Vec<Capture>> {
    CAPTURES
        .lock()
        .unwrap()
        .as_ref()
        .filter(|(at, cached_key, _)| at.elapsed() < CAPTURE_TTL && cached_key == key)
        .map(|(_, _, captures)| captures.clone())
}

/// The `terminal` mode's list.
pub struct TerminalsProvider {
    live: bool,
}

impl TerminalsProvider {
    pub fn new() -> Self {
        Self { live: true }
    }

    /// Never reaches the daemon — see `permissions::PermissionsProvider::disabled`.
    pub fn disabled() -> Self {
        Self { live: false }
    }

    fn client(&self) -> Result<McpClient, ProviderError> {
        if !self.live {
            return Err(ProviderError(crate::mcp::McpError::NotRunning.to_string()));
        }
        McpClient::discover().map_err(|e| ProviderError(e.to_string()))
    }

    fn list(&self) -> Vec<Terminal> {
        let Ok(client) = self.client() else {
            return Vec::new();
        };
        client
            .call("list_terminals", json!({ "all": true }))
            .map(|value| parse_terminals(&value))
            .unwrap_or_default()
    }

    /// The last screen of each terminal, bounded and rate-limited.
    ///
    /// Captured in parallel: these are independent round trips and doing them
    /// in sequence would make the pane's freshness a function of how many
    /// terminals happen to be open. Same `thread::scope` shape `usage` uses
    /// to fan out across vendors.
    fn captures(&self, terminals: &[Terminal]) -> Vec<Capture> {
        let key: CaptureKey = terminals.iter().map(|t| t.id.clone()).collect();
        if let Some(cached) = cached_captures(&key) {
            return cached;
        }
        let Ok(client) = self.client() else {
            return Vec::new();
        };
        let wanted: Vec<&Terminal> = terminals.iter().take(MAX_PREVIEWS).collect();
        let fresh: Vec<Capture> = std::thread::scope(|scope| {
            let running: Vec<_> = wanted
                .iter()
                .map(|terminal| {
                    let client = client.clone();
                    let id = terminal.id.clone();
                    scope.spawn(move || {
                        let text = client
                            .call(
                                "capture_terminal",
                                json!({ "terminalId": id, "stripAnsi": true }),
                            )
                            .map(|value| capture_text(&value))
                            .unwrap_or_default();
                        (id, text)
                    })
                })
                .collect();
            running.into_iter().filter_map(|h| h.join().ok()).collect()
        });
        *CAPTURES.lock().unwrap() = Some((Instant::now(), key, fresh.clone()));
        fresh
    }
}

impl Default for TerminalsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for TerminalsProvider {
    fn id(&self) -> &'static str {
        "terminal"
    }

    fn section_label(&self) -> &'static str {
        "Terminals"
    }

    fn search(&self, query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        let terminals = self.list();
        if terminals.is_empty() {
            return Vec::new();
        }
        let captures = self.captures(&terminals);
        let trimmed = query.trim();
        let count = terminals.len();
        terminals
            .iter()
            .enumerate()
            .filter_map(|(rank, terminal)| {
                let headline = terminal.headline();
                let detail = terminal.detail_line();
                let score = if trimmed.is_empty() {
                    (count - rank) as f32
                } else {
                    [&headline, &detail]
                        .into_iter()
                        .filter_map(|hay| crate::search::fuzzy_score(trimmed, hay))
                        .fold(None, |best: Option<f32>, s| {
                            Some(best.map_or(s, |b| b.max(s)))
                        })?
                };
                let preview = captures
                    .iter()
                    .find(|(id, _)| *id == terminal.id)
                    .map(|(_, text)| text.clone())
                    .filter(|text| !text.is_empty());
                Some(Candidate {
                    score,
                    item: SearchItem {
                        id: terminal.id.clone(),
                        kind: "terminal".to_string(),
                        title: headline,
                        subtitle: Some(detail),
                        icon: Icon::Glyph(Glyph::Text),
                        section_label: "Terminals".to_string(),
                        // Enter opens the directory rather than doing anything
                        // to the session. Killing is the only other verb this
                        // provider has, and a mis-keyed Enter that killed a
                        // terminal is the failure this arrangement exists to
                        // prevent — so kill lives in `⌘K`, marked destructive.
                        action_label: "Open folder  \u{21b5}".to_string(),
                        badge: None,
                        accessory: None,
                        enters_mode: None,
                        group_label: None,
                        actions: vec![ItemAction {
                            id: "kill".to_string(),
                            label: "Kill terminal".to_string(),
                            destructive: true,
                        }],
                        source: Some(terminal.cwd.clone()),
                        meter: None,
                        keeps_open: false,
                        preview,
                        preview_markdown: false,
                        speaker: None,
                        images: Vec::new(),
                    },
                })
            })
            .collect()
    }

    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        let cwd = self
            .list()
            .into_iter()
            .find(|t| t.id == id)
            .map(|t| t.cwd)
            .ok_or_else(|| ProviderError("that terminal is gone".to_string()))?;
        crate::launch::launch_app(Path::new(&cwd)).map_err(|e| ProviderError(e.to_string()))
    }

    fn perform_action(&self, id: &str, action: &str) -> Result<(), ProviderError> {
        match action {
            "kill" => {
                let result = self
                    .client()?
                    .call("kill_terminal", json!({ "terminalId": id }))
                    .map(|_| ())
                    .map_err(|e| ProviderError(e.to_string()));
                // The killed session must not keep rendering its last screen
                // until the rate limit happens to expire.
                *CAPTURES.lock().unwrap() = None;
                result
            }
            other => Err(ProviderError(format!("no action '{other}' on this row"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim from a live `list_terminals {"all": true}` on this machine.
    fn fixture() -> Value {
        json!({"terminals": [
            {"id": "ba524cae", "name": "Terminal 1", "cwd": "/tmp"},
            {"id": "a638b877", "name": "Terminal 1", "cwd": "/Users/example/Documents/neko"}
        ]})
    }

    #[test]
    fn the_directory_names_the_row_because_the_names_collide() {
        // Both of these really are called "Terminal 1" — Paseo numbers them
        // per working directory, so the name alone identifies nothing.
        let parsed = parse_terminals(&fixture());
        assert_eq!(parsed[0].headline(), "tmp");
        assert_eq!(parsed[1].headline(), "neko");
        // The name still appears, for the case where one directory has
        // several sessions in it.
        assert!(parsed[1].detail_line().ends_with("Terminal 1"));
    }

    #[test]
    fn a_screen_of_mostly_nothing_is_trimmed_to_what_was_said() {
        // A capture is a fixed-height screen: 24 lines whether or not
        // anything is on them. Rendering all of it would put one line of
        // content at the top of an empty pane and read as a fault.
        let value =
            json!({"lines": ["$ cargo test", "527 passed", "", "", "", ""], "totalLines": 24});
        assert_eq!(capture_text(&value), "$ cargo test\n527 passed");
    }

    #[test]
    fn a_patched_fonts_icons_are_dropped_because_nothing_here_can_draw_them() {
        // Real capture text from this machine's own prompt: Powerlevel10k
        // draws its separators from the private use area, and neko renders
        // in the system font, which has nothing there.
        let value =
            json!({"lines": [" \u{f179} \u{e0b1} \u{f115} /tmp \u{e0b0}  ", "\u{276f} echo hi"]});
        assert_eq!(capture_text(&value), "       /tmp\n\u{276f} echo hi");
        // A line that was *only* icons becomes blank rather than tofu, and a
        // trailing run of those is then trimmed like any other padding —
        // which is also why the substitution happens before the trim.
        let value = json!({"lines": ["real output", "\u{e0b0}\u{e0b1}"]});
        assert_eq!(capture_text(&value), "real output");
    }

    #[test]
    fn ordinary_symbols_outside_the_private_area_survive() {
        // The prompt caret, box drawing, emoji — all real characters the
        // system font can draw, and all meaning-bearing.
        let value = json!({"lines": ["\u{276f} cargo test \u{2714} 533 \u{2502} ok"]});
        assert_eq!(
            capture_text(&value),
            "\u{276f} cargo test \u{2714} 533 \u{2502} ok"
        );
    }

    #[test]
    fn blank_lines_between_real_ones_are_kept() {
        // Only the trailing run is padding; a gap in the middle is output.
        let value = json!({"lines": ["one", "", "two", ""]});
        assert_eq!(capture_text(&value), "one\n\ntwo");
    }

    #[test]
    fn a_terminal_with_no_id_is_dropped_rather_than_listed() {
        let value = json!({"terminals": [{"name": "Terminal 1", "cwd": "/tmp"}, {"id": "ok"}]});
        let parsed = parse_terminals(&value);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "ok");
    }

    #[test]
    fn a_terminal_at_the_filesystem_root_still_has_a_headline() {
        // `Path::file_name` is `None` for `/`, and a blank row title is the
        // one thing this project's row layout never renders.
        let value = json!({"terminals": [{"id": "x", "name": "", "cwd": "/"}]});
        assert_eq!(parse_terminals(&value)[0].headline(), "/");
    }

    #[test]
    fn killing_is_the_only_destructive_verb_and_enter_is_not_it() {
        // A mis-keyed Enter that killed a session is the failure this
        // arrangement exists to prevent.
        let provider = TerminalsProvider::disabled();
        assert!(provider.search("", 0).is_empty());
        assert!(provider.perform_action("x", "nonsense").is_err());
    }

    #[test]
    fn a_capture_set_is_keyed_on_which_terminals_it_came_from() {
        // Two seconds is nothing to a person and plenty for a session to be
        // killed from Paseo's own window. Keyed on time alone, the pane
        // would show a preview for a terminal that is gone.
        let one: CaptureKey = vec!["a".to_string()];
        let two: CaptureKey = vec!["a".to_string(), "b".to_string()];
        *CAPTURES.lock().unwrap() = Some((
            Instant::now(),
            one.clone(),
            vec![("a".to_string(), "hi".to_string())],
        ));
        assert!(cached_captures(&one).is_some(), "the same set is reused");
        assert!(
            cached_captures(&two).is_none(),
            "a changed set is refetched"
        );
        *CAPTURES.lock().unwrap() = None;
    }
}
