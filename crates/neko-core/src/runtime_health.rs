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
        set_notice(&db, "w", "Claude Code is signed out").unwrap();
        let before = mark_good(&db).unwrap().unwrap();
        assert_eq!(before.notice, Some(("w".into(), "Claude Code is signed out".into())));
        assert!(!load(&db).is_bad());
        assert!(mark_good(&db).unwrap().is_none());
        assert!(mark_bad(&db, "again", 99).unwrap());
    }
}
