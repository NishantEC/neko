//! Model-provider quota, read from the provider's own API.
//!
//! **Not through Paseo.** neko can already see Paseo's agents, so proxying
//! quota through its daemon looked natural — and was investigated properly
//! before this was written. Its `provider.usage.list` RPC is WebSocket-only
//! (no HTTP route, no CLI command), needs a `hello` handshake at
//! `protocolVersion: 1`, and then routes through a session whose
//! establishment is not documented for third parties; their own
//! `docs/protocol-compatibility.md` is explicit that the compatibility
//! contract exists between *their* app and *their* daemon. Reading the
//! provider's own API instead needs no handshake, no dependency on Paseo
//! running, and no protocol that owes this app anything.
//!
//! The *method* is Paseo's — credentials from the macOS Keychain, then
//! `api.anthropic.com/api/oauth/usage` — because it is simply how the
//! provider works. Every line here is neko's own; see `refs/README.md` on
//! why that distinction matters more for that repository than for any other
//! in the tree.
//!
//! ## The token
//!
//! It is a real OAuth credential and is treated as one:
//!
//! * never logged, never persisted, never put in a struct that outlives the
//!   request;
//! * **never passed as a command-line argument.** `curl -H "Authorization:
//!   Bearer …"` would put it in `argv`, where any local process can read it
//!   out of `ps`. It goes to `curl` on stdin through `--config -` instead;
//! * only read when a refresh is actually due (see [`CACHE_TTL`]), so the
//!   Keychain is touched as rarely as the feature allows.
//!
//! ## Why `curl` rather than an HTTP crate
//!
//! This is the daemon's first outbound request. A Rust HTTP client would
//! pull in an async runtime and a TLS stack for one GET; `curl` ships with
//! macOS and is already how this crate reaches `mdfind`, `open`, `launchctl`
//! and `paseo`. If neko ever makes many requests, that trade flips.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Anthropic's OAuth usage endpoint, and the beta header it requires.
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";

/// The Keychain item Claude Code stores its credentials under.
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

/// How long a fetched answer stays good.
///
/// A quota window moves in hours, not seconds, and every refresh costs a
/// Keychain read plus a network round-trip — so re-fetching per keystroke
/// would be both slow and rude. The mode re-searches on every character;
/// this is what keeps that free.
const CACHE_TTL: Duration = Duration::from_secs(90);

/// How long to wait for the network before giving up. Short on purpose: this
/// sits behind a search, and a stalled quota lookup must never be what makes
/// the panel feel slow. (`security` takes no timeout flag; the Keychain read
/// is local and returns immediately or not at all.)
const REQUEST_TIMEOUT_SECS: u64 = 10;

/// One quota window, already reduced to what a row can show.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageWindow {
    /// The API's own key, e.g. `"five_hour"`.
    pub id: String,
    /// `"5-hour limit"` — the API returns snake_case keys, not labels.
    pub label: String,
    /// Percent used, `0.0..=100.0`.
    pub utilization: f32,
    /// RFC 3339, as the API gives it. `None` for a window with no reset.
    pub resets_at: Option<String>,
}

/// What the pane shows: either real numbers, or why there are none.
#[derive(Debug, Clone, PartialEq)]
pub enum Usage {
    Windows(Vec<UsageWindow>),
    /// Signed out, or the token expired — actionable, unlike a generic error.
    NeedsAuth,
    /// No Claude Code credential on this machine at all.
    NotConfigured,
    Failed(String),
}

static CACHE: Mutex<Option<(Instant, Usage)>> = Mutex::new(None);

/// Cached quota, refreshing only when [`CACHE_TTL`] has passed.
pub fn usage() -> Usage {
    if let Some((at, cached)) = CACHE.lock().unwrap().as_ref()
        && at.elapsed() < CACHE_TTL
    {
        return cached.clone();
    }
    let fresh = fetch();
    *CACHE.lock().unwrap() = Some((Instant::now(), fresh.clone()));
    fresh
}

/// Drops the cache so the next [`usage`] call really refetches.
pub fn invalidate() {
    *CACHE.lock().unwrap() = None;
}

fn fetch() -> Usage {
    let Some(token) = keychain_token() else {
        return Usage::NotConfigured;
    };
    match request_usage(&token) {
        Ok(body) => parse_usage(&body),
        Err(e) => e,
    }
}

/// The OAuth access token, or `None` when Claude Code has never signed in
/// here. The returned `String` is the secret; callers must not log it.
fn keychain_token() -> Option<String> {
    // `-w` prints only the password. The item is looked up by service alone:
    // Claude Code writes one entry, and matching on the account name as well
    // would fail for anybody whose local username differs from the one that
    // created it.
    let out = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-w", "-s", KEYCHAIN_SERVICE])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8(out.stdout).ok()?;
    token_from_credentials(&raw)
}

/// Pulls `claudeAiOauth.accessToken` out of the Keychain blob.
///
/// Split out from the Keychain call so it can be tested against a fixture
/// rather than against whatever is really on the machine — the one part of
/// this module that can be tested at all without a credential.
pub fn token_from_credentials(raw: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(raw.trim()).ok()?;
    let token = parsed.get("claudeAiOauth")?.get("accessToken")?.as_str()?;
    (!token.is_empty()).then(|| token.to_string())
}

/// One GET, with the token handed to `curl` on **stdin**.
///
/// `--config -` reads ordinary curl options from stdin, so neither the token
/// nor the header containing it ever appears in `argv`. `-sS` keeps the
/// progress meter off while leaving real errors on stderr, and
/// `-w '\n%{http_code}'` puts the status on the last line so this can tell
/// 401 (signed out — actionable) from a transport failure without parsing
/// curl's own diagnostics.
fn request_usage(token: &str) -> Result<String, Usage> {
    let mut child = Command::new("/usr/bin/curl")
        .args(["--silent", "--show-error", "--config", "-", "--write-out", "\n%{http_code}"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Usage::Failed(format!("couldn't run curl: {e}")))?;

    let config = format!(
        "url = \"{USAGE_URL}\"\n\
         max-time = {REQUEST_TIMEOUT_SECS}\n\
         header = \"Authorization: Bearer {token}\"\n\
         header = \"Accept: application/json\"\n\
         header = \"anthropic-beta: {OAUTH_BETA}\"\n"
    );
    child
        .stdin
        .take()
        .ok_or_else(|| Usage::Failed("curl refused stdin".to_string()))?
        .write_all(config.as_bytes())
        .map_err(|e| Usage::Failed(format!("couldn't send the request: {e}")))?;

    let out = child.wait_with_output().map_err(|e| Usage::Failed(format!("curl failed: {e}")))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let (body, status) = split_status(&stdout);
    match status {
        Some(200) => Ok(body),
        Some(401) | Some(403) => Err(Usage::NeedsAuth),
        Some(code) => Err(Usage::Failed(format!("Claude usage API returned {code}"))),
        // No status line means curl never completed the exchange; its own
        // stderr says why far better than a guess would.
        None => Err(Usage::Failed(
            String::from_utf8_lossy(&out.stderr).trim().split('\n').next().unwrap_or("request failed").to_string(),
        )),
    }
}

/// Splits `--write-out`'s trailing status line off the body.
pub fn split_status(raw: &str) -> (String, Option<u16>) {
    match raw.rsplit_once('\n') {
        Some((body, status)) => (body.to_string(), status.trim().parse().ok()),
        None => (String::new(), raw.trim().parse().ok()),
    }
}

/// Turns the API's response into rows.
///
/// **Only the windows this app can label are shown.** The response also
/// carries a handful of opaque codenames (`nimbus_quill`, `amber_ladder`,
/// `cinder_cove` …) whose meaning is not public; rendering "amber_ladder 0%"
/// would be noise dressed as information. A window whose value is `null` is
/// one that does not apply to this account and is skipped for the same
/// reason.
pub fn parse_usage(body: &str) -> Usage {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(body) else {
        return Usage::Failed("the usage API returned something unreadable".to_string());
    };
    let mut windows = Vec::new();
    for (id, label) in KNOWN_WINDOWS {
        let Some(entry) = parsed.get(*id).filter(|v| !v.is_null()) else { continue };
        let Some(utilization) = entry.get("utilization").and_then(serde_json::Value::as_f64) else {
            continue;
        };
        windows.push(UsageWindow {
            id: (*id).to_string(),
            label: (*label).to_string(),
            utilization: utilization as f32,
            resets_at: entry.get("resets_at").and_then(serde_json::Value::as_str).map(str::to_string),
        });
    }
    if windows.is_empty() {
        return Usage::Failed("no quota windows apply to this account".to_string());
    }
    Usage::Windows(windows)
}

/// The windows worth showing, in the order a person cares about them: the
/// one that bites first leads.
const KNOWN_WINDOWS: &[(&str, &str)] = &[
    ("five_hour", "5-hour limit"),
    ("seven_day", "Weekly limit"),
    ("seven_day_opus", "Weekly · Opus"),
    ("seven_day_sonnet", "Weekly · Sonnet"),
    ("seven_day_cowork", "Weekly · Cowork"),
    ("seven_day_oauth_apps", "Weekly · Apps"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_token_is_read_from_the_oauth_block_and_nothing_else() {
        let raw = r#"{"claudeAiOauth":{"accessToken":"tok-123","refreshToken":"r","scopes":[]}}"#;
        assert_eq!(token_from_credentials(raw).as_deref(), Some("tok-123"));
    }

    #[test]
    fn a_credential_blob_with_no_token_is_none_rather_than_an_empty_string() {
        assert_eq!(token_from_credentials(r#"{"claudeAiOauth":{"accessToken":""}}"#), None);
        assert_eq!(token_from_credentials(r#"{"claudeAiOauth":{}}"#), None);
        assert_eq!(token_from_credentials("{}"), None);
        assert_eq!(token_from_credentials("not json"), None);
    }

    #[test]
    fn the_status_line_is_split_off_the_body() {
        assert_eq!(split_status("{\"a\":1}\n200"), ("{\"a\":1}".to_string(), Some(200)));
        assert_eq!(split_status("\n401").1, Some(401));
        assert_eq!(split_status("curl: (6) could not resolve host").1, None);
    }

    #[test]
    fn real_windows_are_parsed_in_the_order_that_matters() {
        // Trimmed from a real response, including the null and codename
        // entries that must not become rows.
        let body = r#"{
            "five_hour": {"utilization": 6.0, "resets_at": "2026-08-24T11:10:00Z"},
            "seven_day": {"utilization": 67.0, "resets_at": "2026-08-25T14:00:00Z"},
            "seven_day_opus": null,
            "nimbus_quill": {"utilization": 0.0, "resets_at": null},
            "amber_ladder": null
        }"#;
        let Usage::Windows(w) = parse_usage(body) else { panic!("expected windows") };
        assert_eq!(w.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), vec!["five_hour", "seven_day"]);
        assert_eq!(w[0].label, "5-hour limit");
        assert_eq!(w[0].utilization, 6.0);
        assert_eq!(w[1].resets_at.as_deref(), Some("2026-08-25T14:00:00Z"));
    }

    #[test]
    fn opaque_codenames_are_never_rendered_as_rows() {
        // `nimbus_quill` carries a real number, and it is still skipped: a
        // percentage against a name nobody can interpret is noise.
        let body = r#"{"nimbus_quill": {"utilization": 12.0}, "five_hour": {"utilization": 1.0}}"#;
        let Usage::Windows(w) = parse_usage(body) else { panic!("expected windows") };
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].id, "five_hour");
    }

    #[test]
    fn a_response_with_nothing_applicable_says_so_rather_than_showing_an_empty_pane() {
        assert!(matches!(parse_usage(r#"{"five_hour": null}"#), Usage::Failed(_)));
        assert!(matches!(parse_usage("garbage"), Usage::Failed(_)));
    }
}

// ---------------------------------------------------------------------
// The provider
// ---------------------------------------------------------------------

use neko_protocol::{Glyph, Icon, Meter, MeterStat, SearchItem};

use crate::provider::{Provider, ProviderError};
use crate::search::Candidate;

/// The `/usage` pane: one row per quota window.
///
/// **Mode-only** (`AppState::mode_providers`), like `folder-scope`: a quota
/// window is not an answer to a root-list query, and "5-hour limit" matching
/// a search for "limit" would be a surprise rather than a result.
pub struct UsageProvider;

impl UsageProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UsageProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for UsageProvider {
    fn id(&self) -> &'static str {
        "usage"
    }

    fn section_label(&self) -> &'static str {
        "Usage"
    }

    /// The query is ignored: this is a status pane, not a list to filter.
    /// Six rows at most, and filtering them would only ever hide one.
    fn search(&self, _query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        let rows = rows_for(usage());

        let count = rows.len();
        rows.into_iter()
            .enumerate()
            .map(|(rank, (id, title, subtitle, meter))| Candidate {
                // Descending, so `allocate` preserves the order above rather
                // than reordering by an incidental equal score.
                score: (count - rank) as f32,
                item: SearchItem {
                    id,
                    kind: "usage".to_string(),
                    title,
                    subtitle,
                    icon: Icon::Glyph(Glyph::Sliders),
                    section_label: "Usage".to_string(),
                    action_label: "Refresh  \u{21b5}".to_string(),
                    badge: None,
                    accessory: None,
                    enters_mode: None,
                    group_label: None,
                    actions: Vec::new(),
                    source: None,
                    meter,
                },
            })
            .collect()
    }

    /// Enter re-reads rather than opening anything: there is nowhere to go,
    /// and a stale number is the only thing a quota pane can get wrong.
    fn activate(&self, _id: &str) -> Result<(), ProviderError> {
        invalidate();
        Ok(())
    }
}

/// Turns a fetched [`Usage`] into the rows the provider hands back:
/// `(id, title, subtitle, meter)`.
///
/// A real quota window carries a [`Meter`] and renders as a card
/// (`panel::render_meter_card`); the three states with no number to show
/// carry `None` and render as ordinary rows, which is what they are — a
/// sentence saying what to do next. Split out from `search` so it is
/// testable: `search` itself calls [`usage`], which reads the Keychain and
/// the network.
fn rows_for(usage: Usage) -> Vec<(String, String, Option<String>, Option<Meter>)> {
    match usage {
            Usage::Windows(windows) => windows
                .into_iter()
                .map(|w| {
                    let used = w.utilization.clamp(0.0, 100.0);
                    let meter = Meter {
                        fraction: used / 100.0,
                        stats: vec![
                            MeterStat { label: "Used".into(), value: format!("{used:.0}%") },
                            MeterStat {
                                label: "Remaining".into(),
                                value: format!("{:.0}%", 100.0 - used),
                            },
                        ],
                    };
                    (w.id, w.label, w.resets_at.map(|at| resets_label(&at)), Some(meter))
                })
                .collect(),
            Usage::NeedsAuth => vec![(
                "needs-auth".to_string(),
                "Claude Code is signed out".to_string(),
                Some("Run `claude` and sign in, then reopen this".to_string()),
                None,
            )],
            Usage::NotConfigured => vec![(
                "not-configured".to_string(),
                "No Claude Code credential on this Mac".to_string(),
                Some("Sign in with `claude` to see quota here".to_string()),
                None,
            )],
            Usage::Failed(why) => {
                vec![("error".to_string(), "Couldn't read usage".to_string(), Some(why), None)]
            }
    }
}

/// `"2026-08-24T11:10:00Z"` → `"resets 11:10"`.
///
/// The same trade `agents.rs` made: no date/time dependency for one label,
/// so this reads the clock out of the RFC 3339 string rather than parsing
/// it. A value that is not shaped like a timestamp is shown verbatim, which
/// is more honest than inventing a time from it.
pub fn resets_label(at: &str) -> String {
    match at.split('T').nth(1).and_then(|t| t.get(0..5)) {
        Some(hhmm) => format!("resets {hhmm} UTC"),
        None => at.to_string(),
    }
}

#[cfg(test)]
mod provider_tests {
    use super::*;

    #[test]
    fn a_reset_timestamp_becomes_a_clock_and_anything_else_is_shown_verbatim() {
        assert_eq!(resets_label("2026-08-24T11:10:00.320985+00:00"), "resets 11:10 UTC");
        assert_eq!(resets_label("soon"), "soon");
    }

    #[test]
    fn every_state_produces_at_least_one_row_so_the_pane_is_never_blank() {
        // A pane that renders nothing looks broken; each of these says what
        // is wrong and what to do about it.
        for body in [r#"{"five_hour": null}"#, "garbage"] {
            assert!(matches!(parse_usage(body), Usage::Failed(_)));
        }
    }

    #[test]
    fn a_quota_window_carries_a_meter_whose_two_stats_sum_to_the_whole() {
        let rows = rows_for(Usage::Windows(vec![UsageWindow {
            id: "five_hour".into(),
            label: "5-hour limit".into(),
            utilization: 10.0,
            resets_at: Some("2026-08-24T11:10:00Z".into()),
        }]));
        let meter = rows[0].3.as_ref().expect("a real window renders as a card");
        assert!((meter.fraction - 0.10).abs() < f32::EPSILON);
        let shown: Vec<_> =
            meter.stats.iter().map(|s| (s.label.as_str(), s.value.as_str())).collect();
        assert_eq!(shown, vec![("Used", "10%"), ("Remaining", "90%")]);
        // The note qualifying the number belongs on the card, not squeezed
        // in beside the title as a row accessory.
        assert_eq!(rows[0].2.as_deref(), Some("resets 11:10 UTC"));
    }

    #[test]
    fn an_over_quota_window_fills_the_bar_rather_than_overflowing_its_track() {
        let rows = rows_for(Usage::Windows(vec![UsageWindow {
            id: "five_hour".into(),
            label: "5-hour limit".into(),
            utilization: 130.0,
            resets_at: None,
        }]));
        let meter = rows[0].3.as_ref().unwrap();
        assert_eq!(meter.fraction, 1.0);
        assert_eq!(meter.stats[1].value, "0%");
    }

    #[test]
    fn a_state_with_no_number_stays_an_ordinary_row() {
        // A card with an empty bar would read as "0% used", which is a
        // claim about quota this state cannot make.
        for state in [Usage::NeedsAuth, Usage::NotConfigured, Usage::Failed("nope".into())] {
            let rows = rows_for(state);
            assert_eq!(rows.len(), 1);
            assert!(rows[0].3.is_none());
            assert!(rows[0].2.is_some(), "it has to say what to do next");
        }
    }

}
