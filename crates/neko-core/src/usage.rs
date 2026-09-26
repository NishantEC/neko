//! Model-provider quota, read from each provider's own API.
//!
//! **Not through Paseo.** neko can already see Paseo's agents, so proxying
//! quota through its daemon looked natural — and was investigated properly
//! before this was written. Its `provider.usage.list` RPC is WebSocket-only
//! (no HTTP route, no CLI command), needs a `hello` handshake at
//! `protocolVersion: 1`, and then routes through a session whose
//! establishment is not documented for third parties; their own
//! `docs/protocol-compatibility.md` is explicit that the compatibility
//! contract exists between *their* app and *their* daemon. Reading each
//! provider's own API instead needs no handshake, no dependency on Paseo
//! running, and no protocol that owes this app anything.
//!
//! The *method* is Paseo's — where each credential lives, which endpoint
//! answers — because that is simply how these providers work. Every line
//! here is neko's own; see `refs/README.md` on why that distinction matters
//! more for that repository than for any other in the tree.
//!
//! ## Adding a vendor
//!
//! One [`Vendor`] variant and one arm in each of its four methods:
//! [`Vendor::name`], [`Vendor::credential`], [`Vendor::request`],
//! [`Vendor::parse`]. Nothing else — not the provider, not the cache, not
//! the fan-out, not the rendering. The same shape `agents::Backend` uses,
//! and for the same reason: each vendor's auth and payload are genuinely
//! unalike, so there is nothing to abstract beyond "read a credential, GET,
//! reduce to windows".
//!
//! **A vendor with no credential on this machine is omitted entirely**,
//! rather than contributing a "not configured" row. Eight providers' worth
//! of "you are not signed in" is a worse pane than the two real answers it
//! would bury. Only when *nothing* is signed in does the pane say so.
//!
//! ## The tokens
//!
//! Every one is a real credential and is treated as one:
//!
//! * never logged, never persisted, never put in a struct that outlives the
//!   request;
//! * **never passed as a command-line argument.** `curl -H "Authorization:
//!   Bearer …"` would put it in `argv`, where any local process can read it
//!   out of `ps`. Every header goes to `curl` on stdin through `--config -`;
//! * only read when a refresh is actually due (see [`CACHE_TTL`]), so the
//!   Keychain and the credential files are touched as rarely as the feature
//!   allows.
//!
//! ## Why `curl` rather than an HTTP crate
//!
//! These are the daemon's first outbound requests. A Rust HTTP client would
//! pull in an async runtime and a TLS stack; `curl` ships with macOS and is
//! already how this crate reaches `mdfind`, `open`, `launchctl` and `paseo`.
//! If neko ever makes many requests, that trade flips.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a fetched answer stays good.
///
/// A quota window moves in hours, not seconds, and every refresh costs a
/// credential read plus a network round-trip per vendor — so re-fetching
/// per keystroke would be both slow and rude. The mode re-searches on every
/// character; this is what keeps that free.
const CACHE_TTL: Duration = Duration::from_secs(90);

/// How long to wait for the network before giving up. Short on purpose:
/// this sits behind a search, and a stalled quota lookup must never be what
/// makes the panel feel slow. (`security` takes no timeout flag; the
/// Keychain read is local and returns immediately or not at all.)
const REQUEST_TIMEOUT_SECS: u64 = 10;

/// A provider neko can read quota from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vendor {
    Claude,
    Codex,
    Grok,
}

impl Vendor {
    /// Ordered by how much of this machine's work runs through each — the
    /// pane's sections come out in this order.
    pub const ALL: [Vendor; 3] = [Vendor::Claude, Vendor::Codex, Vendor::Grok];

    pub fn id(self) -> &'static str {
        match self {
            Vendor::Claude => "claude",
            Vendor::Codex => "codex",
            Vendor::Grok => "grok",
        }
    }

    /// The section header this vendor's rows sit under.
    pub fn name(self) -> &'static str {
        match self {
            Vendor::Claude => "Claude Code",
            Vendor::Codex => "Codex",
            Vendor::Grok => "Grok",
        }
    }

    /// Where this vendor keeps its credential on macOS, or `None` when it
    /// has never signed in here. The returned token is the secret; callers
    /// must not log it.
    fn credential(self) -> Option<Credential> {
        match self {
            // The Keychain, **not** `~/.claude/.credentials.json` — that
            // path is the Linux case and does not exist on macOS.
            Vendor::Claude => {
                let raw = run_capture(
                    "/usr/bin/security",
                    &[
                        "find-generic-password",
                        "-w",
                        "-s",
                        "Claude Code-credentials",
                    ],
                )?;
                claude_token(&raw).map(Credential::bare)
            }
            Vendor::Codex => codex_credential(&read_home(".codex/auth.json")?),
            Vendor::Grok => grok_token(&read_home(".grok/auth.json")?).map(Credential::bare),
        }
    }

    /// One GET, with every header handed to `curl` on stdin.
    fn request(self, cred: &Credential) -> Result<String, Reading> {
        let bearer = format!("Bearer {}", cred.token);
        let (url, headers): (&str, Vec<(&str, &str)>) = match self {
            Vendor::Claude => (
                "https://api.anthropic.com/api/oauth/usage",
                vec![
                    ("Authorization", bearer.as_str()),
                    ("Accept", "application/json"),
                    ("anthropic-beta", "oauth-2025-04-20"),
                ],
            ),
            Vendor::Codex => {
                let mut h = vec![
                    ("Authorization", bearer.as_str()),
                    ("Accept", "application/json"),
                    // A browser User-Agent is not decoration: this is a
                    // `chatgpt.com` backend route, and it answers with an
                    // HTML challenge page rather than JSON without one.
                    (
                        "User-Agent",
                        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
                    ),
                ];
                if let Some(account) = &cred.account {
                    h.push(("ChatGPT-Account-Id", account.as_str()));
                }
                ("https://chatgpt.com/backend-api/wham/usage", h)
            }
            Vendor::Grok => (
                "https://cli-chat-proxy.grok.com/v1/billing",
                vec![
                    ("Authorization", bearer.as_str()),
                    ("X-XAI-Token-Auth", "xai-grok-cli"),
                    ("Accept", "application/json"),
                ],
            ),
        };
        curl_get(url, &headers)
    }

    fn parse(self, body: &str) -> Reading {
        match self {
            Vendor::Claude => parse_claude(body),
            Vendor::Codex => parse_codex(body),
            Vendor::Grok => parse_grok(body),
        }
    }
}

/// A token, plus whatever else that vendor needs alongside it.
struct Credential {
    token: String,
    /// Codex scopes its usage route to one ChatGPT account.
    account: Option<String>,
}

impl Credential {
    fn bare(token: String) -> Self {
        Credential {
            token,
            account: None,
        }
    }
}

/// One quota window, already reduced to what a row can show.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageWindow {
    /// Unique within its vendor, e.g. `"five_hour"`.
    pub id: String,
    /// `"5-hour limit"` — every one of these APIs returns keys, not labels.
    pub label: String,
    /// Percent used, `0.0..=100.0`.
    pub utilization: f32,
    /// Unix seconds. `None` for a window with no reset.
    pub resets_at: Option<i64>,
    /// The qualifier shown beside the reading, when a percentage alone
    /// hides the magnitude — 20% of 150 credits and 20% of 15,000 are very
    /// different amounts of headroom. `None` falls back to the remaining
    /// percentage, which is all a percentage-only window can say.
    pub detail: Option<UsageDetail>,
}

/// Rendered as `"{value} {label}"` — `"120 of 150", "credits left"`.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageDetail {
    pub value: String,
    pub label: String,
}

/// What one vendor has to say.
#[derive(Debug, Clone, PartialEq)]
pub enum Reading {
    Windows(Vec<UsageWindow>),
    /// A true statement with no number behind it — an account with no
    /// credit allocation, a plan with no metered window. Renders as an
    /// ordinary row, because a meter at 0% would be a claim about quota
    /// this answer cannot make. `title` names the same thing a window
    /// would, so the left column still lines up with the meters above it.
    Note {
        title: String,
        detail: String,
    },
    /// Signed out, or the token expired — actionable, unlike a generic error.
    NeedsAuth,
    Failed(String),
}

/// One vendor's answer, tagged with who gave it.
#[derive(Debug, Clone, PartialEq)]
pub struct VendorUsage {
    pub vendor: Vendor,
    pub reading: Reading,
}

static CACHE: Mutex<Option<(Instant, Vec<VendorUsage>)>> = Mutex::new(None);

/// Every signed-in vendor's quota, refreshing only when [`CACHE_TTL`] has
/// passed.
pub fn usage() -> Vec<VendorUsage> {
    if let Some((at, cached)) = CACHE.lock().unwrap().as_ref()
        && at.elapsed() < CACHE_TTL
    {
        return cached.clone();
    }
    let fresh = fetch_all();
    *CACHE.lock().unwrap() = Some((Instant::now(), fresh.clone()));
    fresh
}

/// Whatever quota is already known, without ever making a request.
///
/// **The blocking [`usage`] cannot be called from a search path.** A search
/// runs on every keystroke, and a cold call fans out to three HTTPS requests
/// bounded at `REQUEST_TIMEOUT` each — so a row that wanted to show headroom
/// would stall the first character typed. This returns only what is already
/// in hand; pair it with [`warm_in_background`] so the numbers arrive a
/// keystroke or two later rather than never.
pub fn cached() -> Option<Vec<VendorUsage>> {
    CACHE
        .lock()
        .unwrap()
        .as_ref()
        .filter(|(at, _)| at.elapsed() < CACHE_TTL)
        .map(|(_, cached)| cached.clone())
}

/// Starts a refresh if one is due, and returns immediately.
///
/// At most one in flight: a search path calls this per keystroke, and without
/// the guard a fast typist would launch a fan-out per character against three
/// vendors that are all rate-limited.
pub fn warm_in_background() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static IN_FLIGHT: AtomicBool = AtomicBool::new(false);

    if cached().is_some() {
        return;
    }
    if IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        let fresh = fetch_all();
        *CACHE.lock().unwrap() = Some((Instant::now(), fresh));
        IN_FLIGHT.store(false, Ordering::SeqCst);
    });
}

impl VendorUsage {
    /// How much of this vendor's quota is left, as a percentage.
    ///
    /// **The most-used window governs**, not an average: a vendor with a
    /// five-hour window at 95% and a weekly at 10% has 5% of headroom for the
    /// next few hours, and averaging to 47% would be a comfortable-looking
    /// lie at exactly the moment the number matters.
    ///
    /// `None` when there is no number to give — signed out, a plan with no
    /// metered window, or a failed read. A caller showing this must say
    /// nothing rather than guess, since "unknown" and "plenty" are the two
    /// answers a person would act on most differently.
    pub fn headroom_percent(&self) -> Option<f32> {
        match &self.reading {
            Reading::Windows(windows) if !windows.is_empty() => {
                let worst = windows
                    .iter()
                    .map(|w| w.utilization)
                    .fold(f32::MIN, f32::max);
                Some((100.0 - worst).clamp(0.0, 100.0))
            }
            _ => None,
        }
    }
}

/// Drops the cache so the next [`usage`] call really refetches.
pub fn invalidate() {
    *CACHE.lock().unwrap() = None;
}

/// **Concurrently**, one thread per vendor. Three sequential requests at
/// [`REQUEST_TIMEOUT_SECS`] each would put half a minute behind a keystroke
/// in the worst case; in parallel the pane waits for the slowest, not the
/// sum. The same `thread::scope` shape the daemon already uses to fan a
/// search out across providers.
fn fetch_all() -> Vec<VendorUsage> {
    std::thread::scope(|scope| {
        let running: Vec<_> = Vendor::ALL
            .iter()
            .map(|&vendor| scope.spawn(move || fetch_one(vendor)))
            .collect();
        running
            .into_iter()
            .filter_map(|h| h.join().ok().flatten())
            .collect()
    })
}

/// `None` when this vendor has no credential here — see the module comment
/// on why that is an omission rather than a row.
fn fetch_one(vendor: Vendor) -> Option<VendorUsage> {
    let cred = vendor.credential()?;
    let reading = match vendor.request(&cred) {
        Ok(body) => vendor.parse(&body),
        Err(reading) => reading,
    };
    Some(VendorUsage { vendor, reading })
}

// ---------------------------------------------------------------- plumbing

fn read_home(relative: &str) -> Option<String> {
    std::fs::read_to_string(std::env::home_dir()?.join(relative)).ok()
}

fn run_capture(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8(out.stdout).ok())?
}

/// `--config -` reads ordinary curl options from stdin, so neither a token
/// nor the header containing it ever appears in `argv`. `--write-out` puts
/// the status on the last line so this can tell 401 (signed out —
/// actionable) from a transport failure without parsing curl's diagnostics.
fn curl_get(url: &str, headers: &[(&str, &str)]) -> Result<String, Reading> {
    let mut child = Command::new("/usr/bin/curl")
        .args([
            "--silent",
            "--show-error",
            "--config",
            "-",
            "--write-out",
            "\n%{http_code}",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Reading::Failed(format!("couldn't run curl: {e}")))?;

    let mut config = format!("url = \"{url}\"\nmax-time = {REQUEST_TIMEOUT_SECS}\n");
    for (name, value) in headers {
        config.push_str(&format!("header = \"{name}: {value}\"\n"));
    }
    child
        .stdin
        .take()
        .ok_or_else(|| Reading::Failed("curl refused stdin".to_string()))?
        .write_all(config.as_bytes())
        .map_err(|e| Reading::Failed(format!("couldn't send the request: {e}")))?;

    let out = child
        .wait_with_output()
        .map_err(|e| Reading::Failed(format!("curl failed: {e}")))?;
    let (body, status) = split_status(&String::from_utf8_lossy(&out.stdout));
    match status {
        // Some of these routes answer an expired session with an HTML
        // challenge page and a 200, so the body decides too.
        Some(200) if body.trim_start().starts_with('<') => Err(Reading::NeedsAuth),
        Some(200) => Ok(body),
        Some(401 | 403) => Err(Reading::NeedsAuth),
        Some(code) => Err(Reading::Failed(format!("the usage API returned {code}"))),
        // No status line means curl never completed the exchange; its own
        // stderr says why far better than a guess would.
        None => Err(Reading::Failed(
            String::from_utf8_lossy(&out.stderr)
                .trim()
                .split('\n')
                .next()
                .unwrap_or("request failed")
                .to_string(),
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

// ------------------------------------------------------------- credentials

/// The live Claude Code OAuth token, or `None` when it has never signed in
/// here.
///
/// Public because `crate::ask` needs the same credential for the Messages
/// API — one reader rather than two, so there is exactly one place that
/// knows where this token lives and exactly one to audit. The returned
/// `String` is the secret; callers must not log it.
pub fn claude_access_token() -> Option<String> {
    let raw = run_capture(
        "/usr/bin/security",
        &[
            "find-generic-password",
            "-w",
            "-s",
            "Claude Code-credentials",
        ],
    )?;
    claude_token(&raw)
}

/// Pulls `claudeAiOauth.accessToken` out of the Keychain blob.
pub fn claude_token(raw: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(raw.trim()).ok()?;
    let token = parsed.get("claudeAiOauth")?.get("accessToken")?.as_str()?;
    (!token.is_empty()).then(|| token.to_string())
}

/// `~/.codex/auth.json` — the account id travels with the token, because
/// the usage route is scoped to one ChatGPT account and answers for the
/// wrong one otherwise.
fn codex_credential(raw: &str) -> Option<Credential> {
    let parsed: serde_json::Value = serde_json::from_str(raw.trim()).ok()?;
    let tokens = parsed.get("tokens")?;
    let token = tokens
        .get("access_token")?
        .as_str()
        .filter(|t| !t.is_empty())?;
    Some(Credential {
        token: token.to_string(),
        account: tokens
            .get("account_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    })
}

/// `~/.grok/auth.json`, which has had two shapes: a flat `access_token`,
/// and a map keyed by issuer whose values hold the token under `key`. The
/// `https://auth.x.ai::` entries are preferred when present — a stale entry
/// for another issuer would otherwise win on map order alone.
pub fn grok_token(raw: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(raw.trim()).ok()?;
    if let Some(flat) = parsed
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        && !flat.is_empty()
    {
        return Some(flat.to_string());
    }
    let entries = parsed.as_object()?;
    let nested = |key: &str| -> Option<String> {
        entries
            .get(key)?
            .get("key")?
            .as_str()
            .filter(|k| !k.is_empty())
            .map(str::to_string)
    };
    let preferred = entries
        .keys()
        .filter(|k| k.starts_with("https://auth.x.ai::"));
    let fallback = entries.keys();
    preferred.chain(fallback).find_map(|k| nested(k))
}

// ------------------------------------------------------------------ parse

/// **Only the windows this app can label are shown.** The response also
/// carries a handful of opaque codenames (`nimbus_quill`, `amber_ladder`,
/// `cinder_cove` …) whose meaning is not public; rendering "amber_ladder 0%"
/// would be noise dressed as information. A window whose value is `null` is
/// one that does not apply to this account and is skipped for the same
/// reason.
pub fn parse_claude(body: &str) -> Reading {
    const KNOWN: &[(&str, &str)] = &[
        ("five_hour", "5-hour limit"),
        ("seven_day", "Weekly limit"),
        ("seven_day_opus", "Weekly · Opus"),
        ("seven_day_sonnet", "Weekly · Sonnet"),
        ("seven_day_cowork", "Weekly · Cowork"),
        ("seven_day_oauth_apps", "Weekly · Apps"),
    ];
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(body) else {
        return Reading::Failed("the usage API returned something unreadable".to_string());
    };
    let mut windows = Vec::new();
    for (id, label) in KNOWN {
        let Some(entry) = parsed.get(*id).filter(|v| !v.is_null()) else {
            continue;
        };
        let Some(used) = entry.get("utilization").and_then(serde_json::Value::as_f64) else {
            continue;
        };
        windows.push(UsageWindow {
            id: (*id).to_string(),
            label: (*label).to_string(),
            utilization: used as f32,
            resets_at: entry
                .get("resets_at")
                .and_then(serde_json::Value::as_str)
                .and_then(epoch_from_rfc3339),
            detail: None,
        });
    }
    if windows.is_empty() {
        return Reading::Failed("no quota windows apply to this account".to_string());
    }
    Reading::Windows(windows)
}

/// **The label comes from the window's own length, not from its position.**
/// Paseo names these by slot — primary is "Session", secondary is "Weekly" —
/// but this account's *primary* window is 604800 seconds, so slot naming
/// would have called a seven-day window a session. The API says how long
/// each window is; using that cannot go stale.
pub fn parse_codex(body: &str) -> Reading {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(body) else {
        return Reading::Failed("the usage API returned something unreadable".to_string());
    };
    let slots = [
        (
            "primary",
            parsed.pointer("/rate_limit/primary_window"),
            None,
        ),
        (
            "secondary",
            parsed.pointer("/rate_limit/secondary_window"),
            None,
        ),
        (
            "code_review",
            parsed.pointer("/code_review_rate_limit/primary_window"),
            Some("Code review"),
        ),
    ];
    let mut windows = Vec::new();
    for (id, entry, prefix) in slots {
        let Some(entry) = entry.filter(|v| !v.is_null()) else {
            continue;
        };
        let Some(used) = entry
            .get("used_percent")
            .and_then(serde_json::Value::as_f64)
        else {
            continue;
        };
        let span = entry
            .get("limit_window_seconds")
            .and_then(serde_json::Value::as_i64);
        windows.push(UsageWindow {
            id: id.to_string(),
            label: match prefix {
                Some(p) => format!("{p} · {}", window_label(span)),
                None => window_label(span),
            },
            utilization: used as f32,
            resets_at: entry.get("reset_at").and_then(serde_json::Value::as_i64),
            detail: None,
        });
    }
    if windows.is_empty() {
        return Reading::Note {
            title: "Rate limits".to_string(),
            detail: "no metered limit on this plan".to_string(),
        };
    }
    Reading::Windows(windows)
}

/// `"5-hour limit"` from 18000 seconds. Falls back to the raw hour count
/// rather than inventing a name for a length nobody has seen yet.
fn window_label(seconds: Option<i64>) -> String {
    match seconds {
        Some(3600) => "Hourly limit".to_string(),
        Some(86400) => "Daily limit".to_string(),
        Some(604800) => "Weekly limit".to_string(),
        Some(2592000) => "Monthly limit".to_string(),
        Some(s) if s > 0 && s % 3600 == 0 => format!("{}-hour limit", s / 3600),
        _ => "Rate limit".to_string(),
    }
}

/// Grok bills credits rather than percentage windows, so the fraction is
/// derived. An account with **no** allocation (`monthlyLimit` 0, which is
/// what this machine's own account reports) gets a note instead of a bar:
/// zero of zero is not 0% used, it is a plan with nothing metered, and a
/// full-looking or empty-looking bar would both be lies.
pub fn parse_grok(body: &str) -> Reading {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(body) else {
        return Reading::Failed("the billing API returned something unreadable".to_string());
    };
    let num = |p: &str| parsed.pointer(p).and_then(serde_json::Value::as_f64);
    let limit = num("/config/monthlyLimit/val");
    // Live CLI billing reports `config.used`; older responses used
    // `usage.creditUsage`.
    let used = num("/config/used/val").or_else(|| num("/usage/creditUsage"));
    let (Some(limit), Some(used)) = (limit, used) else {
        return Reading::Failed("no credit balance in the billing response".to_string());
    };
    if limit <= 0.0 {
        return Reading::Note {
            title: "Monthly credits".to_string(),
            detail: "no allocation on this account".to_string(),
        };
    }
    Reading::Windows(vec![UsageWindow {
        id: "monthly_credits".to_string(),
        label: "Monthly credits".to_string(),
        utilization: (used / limit * 100.0) as f32,
        resets_at: parsed
            .pointer("/config/billingPeriodEnd")
            .and_then(serde_json::Value::as_str)
            .and_then(epoch_from_rfc3339),
        detail: Some(UsageDetail {
            value: format!("{:.0} of {limit:.0}", limit - used),
            label: "credits left".to_string(),
        }),
    }])
}

// ------------------------------------------------------------------- time

/// `"2026-08-25T14:00:00Z"` → unix seconds.
///
/// Hand-rolled because the alternative is a date dependency for one field,
/// the same trade `clipboard` and `agents` already made. Only the UTC form
/// these APIs actually emit is accepted; anything else is `None` rather
/// than a wrong answer.
pub fn epoch_from_rfc3339(s: &str) -> Option<i64> {
    let (date, rest) = s.split_once('T')?;
    let time = rest.split(['Z', '+', '.']).next()?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.split(':').map(|p| p.parse::<i64>().ok());
    let (hh, mm) = (t.next()??, t.next()??);
    let ss = t.next().flatten().unwrap_or(0);
    // Days from civil, Howard Hinnant's algorithm: shift the year to start
    // in March so the leap day lands at the end and needs no special case.
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hh * 3600 + mm * 60 + ss)
}

/// `"resets in 4h"`, one unit, never an absolute clock.
///
/// An absolute "resets 13:59 UTC" reads as *today* at 13:59 — which for a
/// weekly window is six days wrong — and makes the reader do timezone
/// arithmetic to act on it either way. A duration is unambiguous at every
/// window length these providers use.
pub fn resets_label(at: i64, now_unix_ms: i64) -> String {
    let left = at - now_unix_ms / 1000;
    match left {
        ..=0 => "resetting now".to_string(),
        s if s < 3600 => format!("resets in {}m", (s + 59) / 60),
        s if s < 86400 => format!("resets in {}h", s / 3600),
        s => format!("resets in {}d", s / 86400),
    }
}

// --------------------------------------------------------------- provider

use neko_protocol::{Glyph, Icon, Meter, MeterStat, SearchItem};

use crate::provider::{Provider, ProviderError};
use crate::search::Candidate;

/// The `usage` mode's own list. Registered in `AppState::mode_providers`,
/// never the root list: a rate-limit window is not an answer to a root
/// query — "5-hour limit" surfacing for a search containing "limit" would
/// be a surprise.
pub struct UsageProvider;

impl UsageProvider {
    pub fn new() -> Self {
        UsageProvider
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

    fn search(&self, _query: &str, now_unix_ms: i64) -> Vec<Candidate> {
        let rows = rows_for(&usage(), now_unix_ms);
        let count = rows.len();
        rows.into_iter()
            .enumerate()
            .map(|(rank, row)| Candidate {
                // Descending, so `allocate` preserves the order above rather
                // than reordering by an incidental equal score.
                score: (count - rank) as f32,
                item: SearchItem {
                    id: row.id,
                    kind: "usage".to_string(),
                    title: row.title,
                    subtitle: row.subtitle,
                    icon: Icon::Glyph(Glyph::Sliders),
                    section_label: "Usage".to_string(),
                    action_label: "Refresh  \u{21b5}".to_string(),
                    badge: None,
                    accessory: None,
                    enters_mode: None,
                    // The mode list groups on this, so each vendor's rows
                    // land under its own header.
                    group_label: row.group,
                    actions: Vec::new(),
                    source: None,
                    meter: row.meter,
                    // **"Refresh" that closes the panel refreshes nothing a
                    // person can see.** `activate` only invalidates the
                    // cache; the refetch is the re-search, and
                    // `perform_activation` only re-searches when the panel
                    // stays. It hid instead.
                    keeps_open: true,
                    preview_markdown: false,
                    speaker: None,
                    images: Vec::new(),
                    preview: None,
                },
            })
            .collect()
    }

    /// Enter drops the cache, so the pane can be refreshed without waiting
    /// out [`CACHE_TTL`]. The re-search that follows is what refetches.
    fn activate(&self, _id: &str) -> Result<(), ProviderError> {
        invalidate();
        Ok(())
    }

    fn answers_empty_root_query(&self) -> bool {
        false
    }
}

/// One rendered row, before it becomes a `SearchItem`.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub group: Option<String>,
    pub meter: Option<Meter>,
}

/// Turns what the vendors said into rows.
///
/// A window carries a [`Meter`] and renders as a headline number over a
/// bar; everything else — signed out, no allocation, a failed fetch — is an
/// ordinary row saying what is true and what to do next. Split out from
/// `search` so it is testable: `search` itself calls [`usage`], which reads
/// credentials and the network.
pub fn rows_for(readings: &[VendorUsage], now_unix_ms: i64) -> Vec<Row> {
    if readings.is_empty() {
        return vec![Row {
            id: "none".to_string(),
            title: "No provider signed in on this Mac".to_string(),
            subtitle: Some(
                "Sign in with `claude`, `codex` or `grok` to see quota here".to_string(),
            ),
            group: None,
            meter: None,
        }];
    }
    let mut rows = Vec::new();
    for VendorUsage { vendor, reading } in readings {
        let group = Some(vendor.name().to_string());
        let mut push = |id: &str, title: String, subtitle: Option<String>, meter| {
            rows.push(Row {
                // Ids have to be unique across vendors: two providers can
                // both call a window "weekly", and `panel::resolve_selection`
                // follows the highlight by `(kind, id)`.
                id: format!("{}:{id}", vendor.id()),
                title,
                subtitle,
                group: group.clone(),
                meter,
            });
        };
        match reading {
            Reading::Windows(windows) => {
                for w in windows {
                    let used = w.utilization.clamp(0.0, 100.0);
                    // The reading leads, the qualifier follows —
                    // `panel::render_meter` colours the first and mutes the
                    // rest. Headroom is what the pane is opened to ask, so
                    // it is always the qualifier rather than an omission.
                    let stats = vec![
                        MeterStat {
                            value: format!("{used:.0}%"),
                            label: "used".to_string(),
                        },
                        match &w.detail {
                            Some(d) => MeterStat {
                                value: d.value.clone(),
                                label: d.label.clone(),
                            },
                            None => MeterStat {
                                value: format!("{:.0}%", 100.0 - used),
                                label: "left".to_string(),
                            },
                        },
                    ];
                    push(
                        &w.id,
                        w.label.clone(),
                        w.resets_at.map(|at| resets_label(at, now_unix_ms)),
                        Some(Meter {
                            fraction: used / 100.0,
                            stats,
                        }),
                    );
                }
            }
            Reading::Note { title, detail } => {
                push("note", title.clone(), Some(detail.clone()), None)
            }
            Reading::NeedsAuth => push(
                "needs-auth",
                format!("{} is signed out", vendor.name()),
                Some("Sign in again, then reopen this".to_string()),
                None,
            ),
            Reading::Failed(why) => push(
                "error",
                "Couldn't read usage".to_string(),
                Some(why.clone()),
                None,
            ),
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW_MS: i64 = 1_787_000_000_000;

    #[test]
    fn each_vendor_reads_its_own_credential_shape() {
        assert_eq!(
            claude_token(r#"{"claudeAiOauth":{"accessToken":"tok","refreshToken":"r"}}"#)
                .as_deref(),
            Some("tok")
        );
        let codex = codex_credential(
            r#"{"tokens":{"access_token":"tok","account_id":"acct","refresh_token":"r"}}"#,
        )
        .expect("a real auth.json");
        assert_eq!(codex.token, "tok");
        assert_eq!(codex.account.as_deref(), Some("acct"));
        // Both shapes grok has shipped.
        assert_eq!(
            grok_token(r#"{"access_token":"flat"}"#).as_deref(),
            Some("flat")
        );
        assert_eq!(
            grok_token(r#"{"https://auth.x.ai::default":{"key":"nested"}}"#).as_deref(),
            Some("nested")
        );
    }

    #[test]
    fn a_stale_issuer_entry_never_beats_the_real_one() {
        // Map order alone would pick whichever came first; the x.ai issuer
        // has to win, or a leftover entry signs the request with a token
        // that is not the CLI's.
        let raw = r#"{"https://other.example::a":{"key":"stale"},
                      "https://auth.x.ai::default":{"key":"live"}}"#;
        assert_eq!(grok_token(raw).as_deref(), Some("live"));
    }

    #[test]
    fn a_credential_with_no_usable_token_is_none_rather_than_an_empty_string() {
        assert_eq!(
            claude_token(r#"{"claudeAiOauth":{"accessToken":""}}"#),
            None
        );
        assert_eq!(claude_token("{}"), None);
        assert_eq!(claude_token("not json"), None);
        assert!(codex_credential(r#"{"tokens":{"access_token":""}}"#).is_none());
        assert_eq!(grok_token(r#"{"https://auth.x.ai::d":{"key":""}}"#), None);
    }

    #[test]
    fn the_status_line_is_split_off_the_body() {
        assert_eq!(
            split_status("{\"a\":1}\n200"),
            ("{\"a\":1}".to_string(), Some(200))
        );
        assert_eq!(split_status("\n401").1, Some(401));
        assert_eq!(split_status("curl: (6) could not resolve host").1, None);
    }

    #[test]
    fn claude_windows_are_parsed_in_the_order_that_matters() {
        // Trimmed from a real response, including the null and codename
        // entries that must not become rows.
        let body = r#"{
            "five_hour": {"utilization": 6.0, "resets_at": "2026-08-24T11:10:00Z"},
            "seven_day": {"utilization": 67.0, "resets_at": "2026-08-25T14:00:00Z"},
            "seven_day_opus": null,
            "nimbus_quill": {"utilization": 0.0, "resets_at": null},
            "amber_ladder": null
        }"#;
        let Reading::Windows(w) = parse_claude(body) else {
            panic!("expected windows")
        };
        assert_eq!(
            w.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(),
            ["five_hour", "seven_day"]
        );
        assert_eq!(w[0].label, "5-hour limit");
        assert_eq!(w[0].utilization, 6.0);
    }

    #[test]
    fn opaque_codenames_are_never_rendered_as_rows() {
        // `nimbus_quill` carries a real number, and it is still skipped: a
        // percentage against a name nobody can interpret is noise.
        let body = r#"{"nimbus_quill": {"utilization": 12.0}, "five_hour": {"utilization": 1.0}}"#;
        let Reading::Windows(w) = parse_claude(body) else {
            panic!("expected windows")
        };
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].id, "five_hour");
    }

    #[test]
    fn a_codex_window_is_named_by_its_length_not_by_its_slot() {
        // Verbatim from this machine's own response: the *primary* window
        // is seven days long. Paseo names primary "Session", which would
        // have labelled this one wrong.
        let body = r#"{"plan_type":"team","rate_limit":{
            "primary_window":{"used_percent":0,"limit_window_seconds":604800,"reset_at":1788163446},
            "secondary_window":null},"code_review_rate_limit":null}"#;
        let Reading::Windows(w) = parse_codex(body) else {
            panic!("expected windows")
        };
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].label, "Weekly limit");
        assert_eq!(w[0].resets_at, Some(1788163446));
        assert_eq!(window_label(Some(18000)), "5-hour limit");
        assert_eq!(window_label(Some(7200)), "2-hour limit");
        assert_eq!(window_label(None), "Rate limit");
    }

    #[test]
    fn grok_turns_credits_into_a_fraction_and_keeps_the_raw_count() {
        let body = r#"{"config":{"monthlyLimit":{"val":150},"used":{"val":30},
                      "billingPeriodEnd":"2026-09-01T00:00:00Z"}}"#;
        let Reading::Windows(w) = parse_grok(body) else {
            panic!("expected windows")
        };
        assert_eq!(w[0].utilization, 20.0);
        // The percentage alone hides the magnitude — 20% of 150 and 20% of
        // 15000 are very different amounts of headroom.
        // Headroom, not consumption: "how much is left" is the question
        // the pane is opened to ask.
        assert_eq!(w[0].detail.as_ref().unwrap().value, "120 of 150");
    }

    #[test]
    fn an_account_with_no_credit_allocation_gets_a_note_rather_than_a_bar() {
        // Exactly what this machine's own account reports. Zero of zero is
        // not "0% used"; an empty bar and a full one would both be lies.
        let body = r#"{"config":{"monthlyLimit":{"val":0},"used":{"val":0}}}"#;
        assert!(matches!(parse_grok(body), Reading::Note { .. }));
    }

    #[test]
    fn a_response_with_nothing_applicable_says_so_rather_than_showing_an_empty_pane() {
        assert!(matches!(
            parse_claude(r#"{"five_hour": null}"#),
            Reading::Failed(_)
        ));
        assert!(matches!(parse_claude("garbage"), Reading::Failed(_)));
        assert!(matches!(
            parse_codex(r#"{"rate_limit":null}"#),
            Reading::Note { .. }
        ));
        assert!(matches!(parse_grok("{}"), Reading::Failed(_)));
    }

    #[test]
    fn rfc3339_becomes_epoch_seconds_across_leap_years_and_centuries() {
        assert_eq!(epoch_from_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(epoch_from_rfc3339("2026-08-25T14:00:00Z"), Some(1787666400));
        // 2000 is a leap year, 1900 was not — the case a naive %4 gets wrong.
        assert_eq!(epoch_from_rfc3339("2000-03-01T00:00:00Z"), Some(951868800));
        assert_eq!(
            epoch_from_rfc3339("2026-08-25T14:00:00.512Z"),
            Some(1787666400)
        );
        assert_eq!(epoch_from_rfc3339("not a date"), None);
    }

    #[test]
    fn a_reset_is_a_duration_because_an_absolute_clock_reads_as_today() {
        let now = NOW_MS;
        let secs = now / 1000;
        assert_eq!(resets_label(secs + 1800, now), "resets in 30m");
        assert_eq!(resets_label(secs + 4 * 3600, now), "resets in 4h");
        // The case that made this worth doing: a weekly window rendered as
        // "resets 13:59 UTC" reads as *today*, six days early.
        assert_eq!(resets_label(secs + 6 * 86400, now), "resets in 6d");
        assert_eq!(resets_label(secs - 5, now), "resetting now");
    }

    #[test]
    fn every_vendor_row_is_grouped_and_uniquely_identified() {
        // Two vendors can both call a window "weekly", and
        // `panel::resolve_selection` follows the highlight by `(kind, id)` —
        // colliding ids would move the selection to the wrong provider's row.
        let readings = vec![
            VendorUsage {
                vendor: Vendor::Claude,
                reading: Reading::Windows(vec![UsageWindow {
                    id: "seven_day".into(),
                    label: "Weekly limit".into(),
                    utilization: 68.0,
                    resets_at: None,
                    detail: None,
                }]),
            },
            VendorUsage {
                vendor: Vendor::Codex,
                reading: Reading::Windows(vec![UsageWindow {
                    id: "seven_day".into(),
                    label: "Weekly limit".into(),
                    utilization: 0.0,
                    resets_at: None,
                    detail: None,
                }]),
            },
        ];
        let rows = rows_for(&readings, NOW_MS);
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["claude:seven_day", "codex:seven_day"]
        );
        assert_eq!(rows[0].group.as_deref(), Some("Claude Code"));
        assert_eq!(rows[1].group.as_deref(), Some("Codex"));
        let stats = &rows[0].meter.as_ref().unwrap().stats;
        assert_eq!(
            (stats[0].value.as_str(), stats[0].label.as_str()),
            ("68%", "used")
        );
        assert_eq!(
            (stats[1].value.as_str(), stats[1].label.as_str()),
            ("32%", "left")
        );
    }

    #[test]
    fn a_vendor_with_nothing_to_measure_stays_an_ordinary_row() {
        // A card with an empty bar would read as "0% used", which is a
        // claim about quota none of these states can make.
        for reading in [
            Reading::Note {
                title: "Monthly credits".into(),
                detail: "none".into(),
            },
            Reading::NeedsAuth,
            Reading::Failed("x".into()),
        ] {
            let rows = rows_for(
                &[VendorUsage {
                    vendor: Vendor::Grok,
                    reading,
                }],
                NOW_MS,
            );
            assert_eq!(rows.len(), 1);
            assert!(rows[0].meter.is_none());
        }
    }

    #[test]
    fn nothing_signed_in_says_so_once_rather_than_per_vendor() {
        // The pane must never be blank, and eight "you are not signed in"
        // rows would be worse than one.
        let rows = rows_for(&[], NOW_MS);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].subtitle.is_some(), "it has to say what to do next");
    }
}
