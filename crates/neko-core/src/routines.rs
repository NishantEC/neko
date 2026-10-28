//! Routines: recurring Claude Code runs, one scope each, that act for the
//! owner and then report what they did and what only the owner can do. The
//! pure parts live here (cadence, prompt, reply contract, seeds, storage);
//! the daemon's `routines` module runs them. Every row is its own setting, so
//! nothing on a timer rewrites the workbench snapshot.
use chrono::{Datelike, Duration, TimeZone, Timelike};
use neko_protocol::routines::{Cadence, NeedsYou, Routine, RoutineKind, RoutineRun, RoutineState, ScopeProposal};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const DEFAULT_STANDING_ORDERS: &str = "You act for Asha without asking. Reply, send, post, review, rebase, merge, close and archive when it is the right call, and say what you did. Put something in needs_you only when it truly requires Asha: a decision only he can make, a credential or sign-in, money, or something you tried and could not do.";

const DEFINITIONS: &str = "routines_v1";
const SEQ: &str = "routine_seq_v1";
const KEEP_RUNS: usize = 50;
const MAX_DID: usize = 30;
const MAX_NEEDS: usize = 10;
const MAX_LINE: usize = 600;
/// Quiet runs stretch an `Every` interval up to this many times.
const MAX_BACKOFF_SHIFT: u32 = 2;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Store {
    pub routines: Vec<Routine>,
    /// Workspace id → standing orders. Missing means the default.
    pub standing_orders: BTreeMap<String, String>,
    pub seeded: bool,
}

impl Store {
    pub fn orders_for(&self, workspace_id: &str) -> &str {
        self.standing_orders
            .get(workspace_id)
            .map(String::as_str)
            .filter(|text| !text.trim().is_empty())
            .unwrap_or(DEFAULT_STANDING_ORDERS)
    }
}

fn state_key(id: &str) -> String {
    format!("routine_state_v1:{id}")
}

fn runs_key(id: &str) -> String {
    format!("routine_runs_v1:{id}")
}

fn read<T: for<'de> Deserialize<'de> + Default>(db: &crate::Db, key: &str) -> Result<T, String> {
    match db.get_setting(key).map_err(|e| e.to_string())? {
        Some(text) => serde_json::from_str(&text).map_err(|e| format!("Cannot read {key}: {e}")),
        None => Ok(T::default()),
    }
}

fn write<T: Serialize>(db: &crate::Db, key: &str, value: &T) -> Result<(), String> {
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    db.set_setting(key, &text).map_err(|e| e.to_string())
}

pub fn load(db: &crate::Db) -> Result<Store, String> {
    read(db, DEFINITIONS)
}

pub fn save_definitions(db: &crate::Db, store: &Store) -> Result<(), String> {
    write(db, DEFINITIONS, store)
}

pub fn load_state(db: &crate::Db, id: &str) -> Result<RoutineState, String> {
    read(db, &state_key(id))
}

pub fn save_state(db: &crate::Db, id: &str, state: &RoutineState) -> Result<(), String> {
    write(db, &state_key(id), state)
}

/// Newest first.
pub fn load_runs(db: &crate::Db, id: &str) -> Result<Vec<RoutineRun>, String> {
    read(db, &runs_key(id))
}

pub fn save_runs(db: &crate::Db, id: &str, runs: &[RoutineRun]) -> Result<(), String> {
    write(db, &runs_key(id), &runs)
}

pub fn push_run(db: &crate::Db, id: &str, run: RoutineRun) -> Result<(), String> {
    let mut runs = load_runs(db, id)?;
    runs.insert(0, run);
    runs.truncate(KEEP_RUNS);
    save_runs(db, id, &runs)
}

/// The highest sequence number handed out so far.
pub fn latest_seq(db: &crate::Db) -> Result<u64, String> {
    read(db, SEQ)
}

/// Reserves `count` sequence numbers and returns the first.
pub fn reserve_seq(db: &crate::Db, count: u64) -> Result<u64, String> {
    let latest = latest_seq(db)?;
    write(db, SEQ, &(latest + count))?;
    Ok(latest + 1)
}

/// When the routine should next run, strictly after `now_ms`.
pub fn next_due<Tz: TimeZone>(cadence: &Cadence, quiet_streak: u32, now_ms: i64, tz: &Tz) -> i64 {
    let now = tz.timestamp_millis_opt(now_ms).single().unwrap_or_else(|| tz.timestamp_millis_opt(0).unwrap());
    match cadence {
        Cadence::Every { minutes, from_hour, to_hour } => {
            let minutes = i64::from((*minutes).max(1)) << quiet_streak.min(MAX_BACKOFF_SHIFT);
            let candidate = now.clone() + Duration::minutes(minutes);
            let (from, to) = ((*from_hour).min(23), (*to_hour).min(24));
            let inside = |hour: u32| if from <= to { hour >= from && hour < to } else { hour >= from || hour < to };
            if from == to || inside(candidate.hour()) {
                return candidate.timestamp_millis();
            }
            // Outside the window: the next time the window opens.
            let mut day = candidate.date_naive();
            if candidate.hour() >= from {
                day = day.succ_opt().unwrap_or(day);
            }
            tz.with_ymd_and_hms(day.year(), day.month(), day.day(), from, 0, 0)
                .earliest()
                .map(|t| t.timestamp_millis())
                .unwrap_or(candidate.timestamp_millis())
        }
        Cadence::Daily { hour, minute } => {
            let (hour, minute) = ((*hour).min(23), (*minute).min(59));
            let mut day = now.date_naive();
            for _ in 0..3 {
                if let Some(slot) = tz.with_ymd_and_hms(day.year(), day.month(), day.day(), hour, minute, 0).earliest() {
                    if slot.timestamp_millis() > now_ms {
                        return slot.timestamp_millis();
                    }
                }
                day = day.succ_opt().unwrap_or(day);
            }
            now_ms + 24 * 3_600_000
        }
    }
}

/// `next_due` in the Mac's own time zone.
pub fn next_due_local(cadence: &Cadence, quiet_streak: u32, now_ms: i64) -> i64 {
    next_due(cadence, quiet_streak, now_ms, &chrono::Local)
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn build_prompt(routine: &Routine, scope: &ScopeProposal, orders: &str, memory: &Value, now_iso: &str) -> String {
    let mut servers: Vec<&str> = scope.servers.iter().filter(|s| s.level == "scope").map(|s| s.name.as_str()).collect();
    servers.dedup();
    let servers = if servers.is_empty() { "the MCP servers and connectors your setup loads here".to_string() } else { servers.join(", ") };
    let memory = if memory.is_null() { "{}".to_string() } else { memory.to_string() };
    format!(
        "You are Neko, Asha's chief of staff, running the \"{name}\" routine for the {scope_name} scope.\n\
Working folder: {folder}. Scope-level connections here: {servers}. Claude connectors (Gmail, Google Calendar, Drive, Slack) may also be available.\n\
Now (UTC): {now_iso}\n\n\
{instruction}\n\n\
Standing orders from Asha:\n{orders}\n\n\
What you asked to remember last run (JSON): {memory}\n\
A previous run may have acted and then failed to report. Before replying to or acting on anything, check whether you already did (for example your own reply in the thread), and never do it twice.\n\n\
End your reply with exactly one fenced json block and nothing after it:\n\
```json\n{{\"quiet\": false, \"did\": [\"one line per thing you did, with who/where\"], \"needs_you\": [{{\"title\": \"short\", \"detail\": \"what Asha must do and why\", \"link\": \"https://...\"}}], \"memory\": {{}}}}\n```\n\
quiet is true only when nothing new happened and you did nothing; then did and needs_you are empty. memory is handed back to you next run: keep it small (timestamps, ids, who you are waiting on).",
        name = routine.name,
        scope_name = scope.name,
        folder = scope.folders.first().map(String::as_str).unwrap_or("~"),
        instruction = routine.instruction.trim(),
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub quiet: bool,
    pub did: Vec<String>,
    /// `seq` is 0 until the daemon assigns one.
    pub needs_you: Vec<NeedsYou>,
    pub memory: Value,
}

fn clip(text: &str) -> String {
    let text = text.trim();
    if text.len() <= MAX_LINE {
        return text.to_string();
    }
    let mut end = MAX_LINE;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// Reads the last ```json fence of an agent's reply.
pub fn parse_reply(text: &str) -> Result<Parsed, String> {
    let start = text.rfind("```json").ok_or("The reply has no ```json block")?;
    let body = &text[start + "```json".len()..];
    let end = body.find("```").ok_or("The reply's ```json block is not closed")?;
    let value: Value = serde_json::from_str(body[..end].trim()).map_err(|e| format!("The reply's json block does not parse: {e}"))?;
    let object = value.as_object().ok_or("The reply's json block is not an object")?;
    let did: Vec<String> = object
        .get("did")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).filter(|s| !s.trim().is_empty()).map(clip).take(MAX_DID).collect())
        .unwrap_or_default();
    let needs_you: Vec<NeedsYou> = object
        .get("needs_you")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let title = item.get("title").and_then(Value::as_str).map(clip).filter(|t| !t.is_empty())?;
                    Some(NeedsYou {
                        seq: 0,
                        title,
                        detail: item.get("detail").and_then(Value::as_str).map(clip).unwrap_or_default(),
                        link: item.get("link").and_then(Value::as_str).map(clip).filter(|l| l.starts_with("http")),
                        done: false,
                    })
                })
                .take(MAX_NEEDS)
                .collect()
        })
        .unwrap_or_default();
    let quiet = object.get("quiet").and_then(Value::as_bool).unwrap_or(false) && did.is_empty() && needs_you.is_empty();
    let memory = object.get("memory").cloned().filter(Value::is_object).unwrap_or_else(|| serde_json::json!({}));
    Ok(Parsed { quiet, did, needs_you, memory })
}

/// Where each scope's work lives, so seeded instructions can be specific.
fn scope_hint(scope: &str) -> &'static str {
    match scope {
        "Client A" => "Slack is slack-client-a. Jira is project KAN (atlassian). GitHub org client-a-ai (repos triage, support-fe, triage-admin, BFF); Asha's GitHub login is NishantEC.",
        "Acme" => "Slack is slack-work. Issues are in Linear team HEA and Asana. GitHub org Acme; Asha's GitHub login is NishantEC.",
        "Client B" => "Issues are in Linear (linear-b). GitHub: the org that owns the repos in this folder (check `git remote -v`); Asha's GitHub login is NishantEC.",
        "Sibling app" => "This is Asha's own product, Sibling app (ui-kit). Issues are in Linear (linear). GitHub repo is the one in this folder.",
        _ => "Personal: Gmail and Google Calendar (Claude connectors), GitHub user NishantEC, Linear (linear-personal).",
    }
}

fn instruction(kind: RoutineKind, scope: &str) -> String {
    let hint = scope_hint(scope);
    match kind {
        RoutineKind::Messages => format!(
            "Check everything that came in for Asha in this scope since memory.last_check (if missing, the last 24 hours): direct messages and mentions, email, issue-tracker comments and mentions, and review comments on his pull requests. {hint}\n\
Handle each one yourself: reply, answer, acknowledge, look things up in code or data to answer properly, fix small things, or start the work. Keep memory.last_check (UTC ISO time) and memory.expecting_reply_from (people you are waiting on) current. If a source fails because a sign-in is missing, put that in needs_you once, then remember you reported it."
        ),
        RoutineKind::Digest => {
            let extra = match scope {
                "Sibling app" => " Also check docs/marketing/blog/PUBLISHED.md on origin/main for yesterday's and today's posts; if a day has none, find out why and fix it.",
                "Client A" | "Acme" | "Client B" => "",
                _ => " Also one line on the machine: free disk (df -h /System/Volumes/Data), load (uptime), and the size of ~/Library/Application Support/neko/task-worktrees.",
            };
            format!(
                "Morning brief for this scope. {hint} Look at: Asha's open pull requests (CI, review state, unresolved comments), pull requests waiting on his review, issues assigned to him that changed in the last 24 hours, and anything urgent from monitoring.{extra}\n\
Act on what you can first: answer review comments, fix or re-run failing CI, rebase, review the PRs waiting on him when the change is clear. Then report: did = what you handled; needs_you = only the few things Asha must do today, each with a link. Write the brief to ~/Documents/digests/neko-{slug}-<today's date>.md as well.",
                slug = scope.to_lowercase().replace(' ', "-")
            )
        }
        RoutineKind::PrSweep => format!(
            "Pull request and branch sweep. {hint} Go through Asha's open pull requests and his remote branches: rebase stale ones on their base, fix failing CI when the fix is clear, address review comments, nudge reviewers on PRs waiting more than two days, and close PRs and delete branches that are merged elsewhere, superseded or dead. Do it, then report each action in did."
        ),
        RoutineKind::Custom => String::new(),
    }
}

/// Three routines per scope with a workspace: messages, digest, PR sweep.
pub fn seeds(scopes: &[ScopeProposal]) -> Vec<Routine> {
    let mut routines = Vec::new();
    for scope in scopes {
        let Some(workspace_id) = scope.workspace_id.clone() else { continue };
        let busy = matches!(scope.name.as_str(), "Client A" | "Acme");
        let personal = !matches!(scope.name.as_str(), "Client A" | "Acme" | "Client B" | "Sibling app");
        let specs = [
            (RoutineKind::Messages, "Messages", Cadence::Every { minutes: if busy { 20 } else { 60 }, from_hour: 8, to_hour: 23 }),
            (RoutineKind::Digest, "Morning brief", Cadence::Daily { hour: 10, minute: 30 }),
            (RoutineKind::PrSweep, "PR sweep", Cadence::Daily { hour: 11, minute: 0 }),
        ];
        for (kind, name, cadence) in specs {
            let paseo = match (kind, scope.name.as_str()) {
                (RoutineKind::Messages, "Client A") => Some("message-watch-adaptive"),
                (RoutineKind::Digest, _) if personal => Some("daily-work-digest"),
                (RoutineKind::PrSweep, _) if personal => Some("weekly-pr-sweep"),
                _ => None,
            };
            routines.push(Routine {
                id: format!("{workspace_id}-{}", format!("{kind:?}").to_lowercase()),
                workspace_id: workspace_id.clone(),
                kind,
                name: name.into(),
                instruction: instruction(kind, &scope.name),
                cadence,
                enabled: true,
                model: "sonnet".into(),
                paseo_schedule: paseo.map(str::to_string),
            });
        }
    }
    routines
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn ist() -> FixedOffset {
        FixedOffset::east_opt(5 * 3600 + 1800).unwrap()
    }

    fn at(h: u32, m: u32) -> i64 {
        ist().with_ymd_and_hms(2026, 10, 29, h, m, 0).unwrap().timestamp_millis()
    }

    #[test]
    fn every_backs_off_and_respects_hours() {
        let every = Cadence::Every { minutes: 20, from_hour: 8, to_hour: 23 };
        assert_eq!(next_due(&every, 0, at(10, 0), &ist()), at(10, 20));
        assert_eq!(next_due(&every, 1, at(10, 0), &ist()), at(10, 40));
        assert_eq!(next_due(&every, 9, at(10, 0), &ist()), at(11, 20));
        // 22:50 + 20 min is past 23:00: tomorrow at 08:00.
        let tomorrow = ist().with_ymd_and_hms(2026, 10, 30, 8, 0, 0).unwrap().timestamp_millis();
        assert_eq!(next_due(&every, 0, at(22, 50), &ist()), tomorrow);
        // 02:00 is before the window: today at 08:00.
        assert_eq!(next_due(&every, 0, at(2, 0), &ist()), at(8, 0));
    }

    #[test]
    fn daily_is_strictly_after_now() {
        let daily = Cadence::Daily { hour: 10, minute: 30 };
        assert_eq!(next_due(&daily, 0, at(9, 0), &ist()), at(10, 30));
        let tomorrow = ist().with_ymd_and_hms(2026, 10, 30, 10, 30, 0).unwrap().timestamp_millis();
        assert_eq!(next_due(&daily, 0, at(10, 30), &ist()), tomorrow);
        assert_eq!(next_due(&daily, 0, at(10, 31), &ist()), tomorrow);
    }

    #[test]
    fn parse_takes_last_json_fence() {
        let text = "Example:\n```json\n{\"quiet\": true}\n```\nDone.\n```json\n{\"quiet\": false, \"did\": [\"Replied to Asha in #triage\"], \"needs_you\": [{\"title\": \"Sign in to Jira\", \"detail\": \"atlassian needs auth\", \"link\": \"https://x\"}, {\"detail\": \"no title\"}], \"memory\": {\"last_check\": \"t\"}}\n```\n";
        let parsed = parse_reply(text).unwrap();
        assert!(!parsed.quiet);
        assert_eq!(parsed.did, vec!["Replied to Asha in #triage"]);
        assert_eq!(parsed.needs_you.len(), 1);
        assert_eq!(parsed.needs_you[0].link.as_deref(), Some("https://x"));
        assert_eq!(parsed.memory["last_check"], "t");
    }

    #[test]
    fn parse_rejects_missing_block() {
        assert!(parse_reply("I checked everything.").is_err());
        assert!(parse_reply("```json\n{not json}\n```").is_err());
        assert!(parse_reply("```json\n{\"quiet\": true}").is_err());
    }

    #[test]
    fn quiet_with_items_is_not_quiet() {
        let parsed = parse_reply("```json\n{\"quiet\": true, \"did\": [\"x\"]}\n```").unwrap();
        assert!(!parsed.quiet);
        assert!(parsed.memory.is_object());
    }

    #[test]
    fn seeds_three_per_scope() {
        let scope = |name: &str, id: Option<&str>| ScopeProposal { name: name.into(), folders: vec!["/x".into()], servers: vec![], problems: vec![], workspace_id: id.map(str::to_string) };
        let routines = seeds(&[scope("Client A", Some("w1")), scope("Asha", Some("w2")), scope("Loose", None)]);
        assert_eq!(routines.len(), 6);
        assert_eq!(routines[0].paseo_schedule.as_deref(), Some("message-watch-adaptive"));
        assert_eq!(routines[0].cadence, Cadence::Every { minutes: 20, from_hour: 8, to_hour: 23 });
        assert!(routines[0].instruction.contains("slack-client-a"));
        assert_eq!(routines[4].paseo_schedule.as_deref(), Some("daily-work-digest"));
        assert_eq!(routines[5].paseo_schedule.as_deref(), Some("weekly-pr-sweep"));
        assert!(routines.iter().all(|r| r.id.starts_with(&r.workspace_id)));
    }

    #[test]
    fn runs_keep_fifty() {
        let db = crate::Db::open_in_memory().unwrap();
        for i in 0..55 {
            let run = RoutineRun { id: i.to_string(), routine_id: "r".into(), started_ms: i, finished_ms: None, ok: true, quiet: false, did: vec![], needs_you: vec![], error: None, text: String::new() };
            push_run(&db, "r", run).unwrap();
        }
        let runs = load_runs(&db, "r").unwrap();
        assert_eq!(runs.len(), 50);
        assert_eq!(runs[0].id, "54");
        assert_eq!(reserve_seq(&db, 3).unwrap(), 1);
        assert_eq!(reserve_seq(&db, 1).unwrap(), 4);
        assert_eq!(latest_seq(&db).unwrap(), 4);
    }
}
