//! Which agent logins are bad right now, so a background run does not die on
//! one provider's sign-in. See docs/knowledge/neko-runs-the-day.md.
//!
//! Claude Code's logged-in account can change under Neko (another tool swaps
//! it), after which every run fails with the same account error. A failed run
//! is classified here; an auth/account failure is remembered in one small
//! settings row (never the workbench snapshot), runs go straight to the
//! fallback runtime while it lasts, and a cheap probe, at most every
//! `PROBE_EVERY_MS`, says when the login works again.

use serde::{Deserialize, Serialize};

const KEY: &str = "runtime_health_v1:claude";
/// The probe runs a real model call, so it is rare.
pub const PROBE_EVERY_MS: i64 = 30 * 60 * 1000;

/// True when the error says the login or account (not the task) is the
/// problem: an expired or missing sign-in, a disabled subscription, a 401, or
/// a credit/usage limit. Everything else (timeouts, a bad reply, a failing
/// tool) is the run's own failure.
pub fn is_auth_failure(error: &str) -> bool {
    let text = error.to_lowercase();
    // Claude Code's own wording only. Generic words ("unauthorized",
    // "session expired", "not signed in", "status 401") are left out: a
    // run's error carries Claude Code's stderr, where an MCP server or tool
    // reporting its own sign-in would otherwise send every run to Codex.
    const MARKERS: &[&str] = &[
        "oauth session expired",
        "oauth token has expired",
        "oauth token revoked",
        "organization has disabled",
        "disabled claude subscription",
        "subscription has been disabled",
        "not logged in",
        "please run /login",
        "claude /login",
        "invalid api key",
        "invalid x-api-key",
        "authentication_error",
        "failed to authenticate",
        "api error: 401",
        "credit balance is too low",
        "usage limit reached",
        "hit your limit",
        "out of extra usage",
        "reached your usage limit",
    ];
    MARKERS.iter().any(|m| text.contains(m))
}

/// The auth failure is a spent usage or credit limit rather than a sign-in
/// problem, so the notice says to wait or top up instead of `/login`.
pub fn is_usage_limit(error: &str) -> bool {
    let text = error.to_lowercase();
    ["credit balance is too low", "usage limit reached", "hit your limit", "out of extra usage", "reached your usage limit"]
        .iter()
        .any(|m| text.contains(m))
}

/// Codex's wording for the same thing: a spent usage limit, not a sign-in problem.
pub fn is_codex_usage_limit(error: &str) -> bool {
    let text = error.to_lowercase();
    ["usage_limit_reached", "hit your usage limit", "usage limit has been reached", "reached your usage limit", "insufficient_quota"]
        .iter()
        .any(|m| text.contains(m))
}

/// Either runtime says its usage limit is spent.
pub fn is_any_usage_limit(error: &str) -> bool {
    is_usage_limit(error) || is_codex_usage_limit(error)
}

/// Longest a hold parks work: a weekly limit may reset days away, never further.
const MAX_HOLD_MS: i64 = 7 * 24 * 3_600_000;
const DEFAULT_HOLD_MS: i64 = 3_600_000;

/// A clock time like `3pm`, `2:30 PM` or `14:00`, as (hour, minute).
fn clock(text: &str) -> Option<(u32, u32)> {
    let text = text.trim_start();
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    let mut hour: u32 = digits.parse().ok()?;
    let mut rest = &text[digits.len()..];
    let mut minute = 0;
    let mut has_minutes = false;
    if let Some(after) = rest.strip_prefix(':') {
        let minutes: String = after.chars().take_while(char::is_ascii_digit).collect();
        minute = minutes.parse().ok()?;
        has_minutes = true;
        rest = &after[minutes.len()..];
    }
    let rest = rest.trim_start();
    let pm = rest.starts_with("pm") || rest.starts_with("p.m");
    let am = rest.starts_with("am") || rest.starts_with("a.m");
    if !pm && !am && !has_minutes {
        return None;
    }
    if minute > 59 || ((pm || am) && !(1..=12).contains(&hour)) || hour > 23 {
        return None;
    }
    if pm && hour < 12 {
        hour += 12;
    }
    if am && hour == 12 {
        hour = 0;
    }
    Some((hour, minute))
}

/// When the limit in `error` resets, if it says: Claude's `...limit reached|<epoch seconds>`,
/// Codex's `resets_in_seconds`, or a clock time after "resets" / "try again at" (read in `tz`,
/// the next time that clock shows).
pub fn limit_reset_ms<Tz: chrono::TimeZone>(error: &str, now_ms: i64, tz: &Tz) -> Option<i64> {
    use chrono::Duration;
    if let Some((_, tail)) = error.split_once('|') {
        let digits: String = tail.trim_start().chars().take_while(char::is_ascii_digit).collect();
        if digits.len() >= 9 {
            if let Ok(number) = digits.parse::<i64>() {
                let at = if digits.len() >= 13 { number } else { number * 1000 };
                if at > now_ms {
                    return Some(at);
                }
            }
        }
    }
    if let Some((_, tail)) = error.split_once("resets_in_seconds") {
        let digits: String = tail.chars().skip_while(|c| !c.is_ascii_digit()).take_while(char::is_ascii_digit).collect();
        if let Ok(seconds) = digits.parse::<i64>() {
            return Some(now_ms + seconds.min(MAX_HOLD_MS / 1000) * 1000);
        }
    }
    let lower = error.to_lowercase();
    let after = ["resets at ", "resets ", "try again at ", "available again at "].iter().find_map(|m| lower.find(m).map(|i| &lower[i + m.len()..]))?;
    let token: String = after.chars().take(12).collect();
    let (hour, minute) = clock(&token)?;
    let now = tz.timestamp_millis_opt(now_ms).single()?;
    let today = now.date_naive();
    let at = |day: chrono::NaiveDate| tz.from_local_datetime(&day.and_hms_opt(hour, minute, 0)?).earliest().map(|t| t.timestamp_millis());
    let first = at(today)?;
    if first > now_ms { Some(first) } else { at(today + Duration::days(1)) }
}

/// Where a routine parked by a usage limit resumes: the earliest reset any of the
/// runtimes' errors names, else an hour from now; never in the past, never past a week.
pub fn hold_until<Tz: chrono::TimeZone>(errors: &[&str], now_ms: i64, tz: &Tz) -> i64 {
    let earliest = errors.iter().filter_map(|e| limit_reset_ms(e, now_ms, tz)).min();
    earliest.unwrap_or(now_ms + DEFAULT_HOLD_MS).clamp(now_ms + 60_000, now_ms + MAX_HOLD_MS)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Health {
    /// When the login was first seen bad in this outage; None when fine.
    #[serde(default)]
    pub bad_since_ms: Option<i64>,
    /// The error that started the outage (bounded).
    #[serde(default)]
    pub reason: String,
    /// The last time a probe (or a run) tried the login while it was bad.
    #[serde(default)]
    pub last_try_ms: i64,
    /// The Inbox item raised for this outage: (workspace id, title).
    #[serde(default)]
    pub notice: Option<(String, String)>,
}

impl Health {
    pub fn is_bad(&self) -> bool {
        self.bad_since_ms.is_some()
    }
    /// A bad login is only tried again once the probe interval has passed.
    pub fn probe_due(&self, now_ms: i64) -> bool {
        self.is_bad() && now_ms - self.last_try_ms >= PROBE_EVERY_MS
    }
}

pub fn load(db: &crate::Db) -> Health {
    db.get_setting(KEY).ok().flatten().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(db: &crate::Db, health: &Health) -> Result<(), String> {
    let text = serde_json::to_string(health).map_err(|e| e.to_string())?;
    db.set_setting_quiet(KEY, &text).map_err(|e| e.to_string())
}

/// Records a failure. Returns true when this starts a new outage (so the
/// caller raises the one notice); a failure inside an outage only moves the
/// retry clock.
pub fn mark_bad(db: &crate::Db, reason: &str, now_ms: i64) -> Result<bool, String> {
    let mut health = load(db);
    let new = !health.is_bad();
    if new {
        health.bad_since_ms = Some(now_ms);
        let mut end = reason.len().min(400);
        while !reason.is_char_boundary(end) {
            end -= 1;
        }
        health.reason = reason[..end].to_string();
        health.notice = None;
    }
    health.last_try_ms = now_ms;
    save(db, &health)?;
    Ok(new)
}

/// Records that the login works. Returns the health as it was, so the caller
/// can resolve the Inbox item; None when it was never bad.
pub fn mark_good(db: &crate::Db) -> Result<Option<Health>, String> {
    let before = load(db);
    if !before.is_bad() {
        return Ok(None);
    }
    save(db, &Health::default())?;
    Ok(Some(before))
}

pub fn set_notice(db: &crate::Db, workspace_id: &str, title: &str) -> Result<(), String> {
    let mut health = load(db);
    health.notice = Some((workspace_id.to_string(), title.to_string()));
    save(db, &health)
}

/// Records a probe attempt so the next one waits.
pub fn note_try(db: &crate::Db, now_ms: i64) -> Result<(), String> {
    let mut health = load(db);
    health.last_try_ms = now_ms;
    save(db, &health)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_failures_seen_in_the_field_are_auth() {
        for error in [
            "Your organization has disabled Claude subscription",
            "Your organization has disabled Claude subscription access. Contact your admin.",
            "OAuth session expired. Please run /login",
            "OAuth session expired",
            "Not logged in · Please run /login",
            "API Error: 401 {\"type\":\"error\",\"error\":{\"type\":\"authentication_error\"}}",
            "Invalid API key · Please run /login",
            "Credit balance is too low",
            "Claude AI usage limit reached|1760000000",
            "You've hit your limit · resets 3pm",
        ] {
            assert!(is_auth_failure(error), "{error}");
        }
    }

    #[test]
    fn other_failures_are_not_auth() {
        for error in [
            "timed out after 900s",
            "The reply did not end in a json block",
            "Scope folder missing: /tmp/x",
            "Claude Code is not installed. Install it, or set NEKO_CLAUDE_PATH to its absolute path.",
            "Cannot read routine_state_v1:a: expected value",
            "MCP server linear: HTTP status 401 Unauthorized",
            "Jira session expired; sign in again",
            "Gmail not signed in",
            "Authentication failed for the remote repository",
            "",
        ] {
            assert!(!is_auth_failure(error), "{error}");
        }
    }

    #[test]
    fn a_usage_limit_names_its_reset_or_waits_an_hour() {
        use chrono::TimeZone;
        let tz = chrono::FixedOffset::east_opt(0).unwrap();
        let now = tz.with_ymd_and_hms(2026, 11, 14, 10, 0, 0).unwrap().timestamp_millis();
        let at = |h, m| tz.with_ymd_and_hms(2026, 11, 14, h, m, 0).unwrap().timestamp_millis();
        let epoch = now / 1000 + 5_000;
        assert_eq!(limit_reset_ms(&format!("Claude AI usage limit reached|{epoch}"), now, &tz), Some(epoch * 1000));
        assert_eq!(limit_reset_ms("You've hit your limit · resets 3pm", now, &tz), Some(at(15, 0)));
        assert_eq!(limit_reset_ms("limit. try again at 2:30 PM.", now, &tz), Some(at(14, 30)));
        assert_eq!(limit_reset_ms("hit your limit, resets 9am", now, &tz), Some(at(9, 0) + 86_400_000), "the next 9am");
        assert_eq!(limit_reset_ms("{\"type\":\"usage_limit_reached\",\"resets_in_seconds\":600}", now, &tz), Some(now + 600_000));
        assert_eq!(limit_reset_ms("credit balance is too low", now, &tz), None);
        assert_eq!(limit_reset_ms("hit your limit, resets soon", now, &tz), None);
        assert!(is_codex_usage_limit("usage_limit_reached") && !is_codex_usage_limit("timed out"));
        assert_eq!(hold_until(&["credit balance is too low"], now, &tz), now + 3_600_000);
        // Every runtime out: the earliest reset wins.
        assert_eq!(hold_until(&["hit your limit, resets 5pm", "usage_limit_reached resets_in_seconds 1200"], now, &tz), now + 1_200_000);
    }

    #[test]
    fn probe_waits_for_its_interval() {
        let health = Health { bad_since_ms: Some(0), last_try_ms: 1000, ..Default::default() };
        assert!(!health.probe_due(1000 + PROBE_EVERY_MS - 1));
        assert!(health.probe_due(1000 + PROBE_EVERY_MS));
        assert!(!Health::default().probe_due(i64::MAX / 2));
    }

    #[test]
    fn an_outage_is_new_once() {
        let db = crate::Db::open_in_memory().unwrap();
        assert!(mark_bad(&db, "OAuth session expired", 10).unwrap());
        assert!(!mark_bad(&db, "OAuth session expired", 20).unwrap());
        let health = load(&db);
        assert_eq!(health.bad_since_ms, Some(10));
        assert_eq!(health.last_try_ms, 20);
        assert!(is_usage_limit("Claude AI usage limit reached|1731500000"));
        assert!(!is_usage_limit("OAuth session expired"));
        set_notice(&db, "w", "Claude Code is signed out").unwrap();
        let before = mark_good(&db).unwrap().unwrap();
        assert_eq!(before.notice, Some(("w".into(), "Claude Code is signed out".into())));
        assert!(!load(&db).is_bad());
        assert!(mark_good(&db).unwrap().is_none());
        assert!(mark_bad(&db, "again", 99).unwrap());
    }
}
