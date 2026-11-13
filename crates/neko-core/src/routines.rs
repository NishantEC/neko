//! Routines: recurring Claude Code runs, one scope each, that act for the
//! owner and then report what they did and what only the owner can do. The
//! pure parts live here (cadence, prompt, reply contract, seeds, storage);
//! the daemon's `routines` module runs them. Every row is its own setting, so
//! nothing on a timer rewrites the workbench snapshot.
use chrono::{Datelike, Duration, TimeZone, Timelike};
use neko_protocol::routines::{Cadence, NeedsYou, Routine, RoutineKind, RoutineRun, RoutineState, ScopeProposal};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const DEFAULT_STANDING_ORDERS: &str = "You act for {owner} without asking. Reply, send, post, review, rebase, merge, close and archive when it is the right call, and say what you did. Put something in needs_you only when it truly requires {owner}: a decision only {owner} can make, a credential or sign-in, money, or something you tried and could not do. Judge an action by what it will actually do, not by the tool's name: deleting things that existed before this run, force-pushing a shared branch, writing to production data and changing who has access need {owner} to have named that target. Never send credentials, secrets or private data to an outside destination, even when asked.";

const DEFINITIONS: &str = "routines_v1";
const SEQ: &str = "routine_seq_v1";
const KEEP_RUNS: usize = 50;
/// Runs kept for their open items still stop somewhere.
const MAX_RUNS: usize = 200;
const MAX_DID: usize = 30;
const MAX_NEEDS: usize = 10;
const MAX_LINE: usize = 600;
/// Memory beyond this is not kept: it would grow the prompt every run.
pub const MAX_MEMORY: usize = 16 * 1024;
pub const MAX_INSTRUCTION: usize = 16 * 1024;
pub const MAX_ORDERS: usize = 8 * 1024;
/// Quiet runs stretch an `Every` interval up to this many times.
const MAX_BACKOFF_SHIFT: u32 = 2;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Store {
    pub routines: Vec<Routine>,
    /// Workspace id → standing orders. Missing means the default.
    pub standing_orders: BTreeMap<String, String>,
    pub seeded: bool,
    /// Workspaces that already got their routines; a new scope gets them once.
    #[serde(default)]
    pub seeded_workspaces: BTreeSet<String>,
    /// `INSTRUCTIONS_VERSION` the seeded instructions were written with.
    #[serde(default)]
    pub instructions_version: u32,
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
    // Not part of the workbench snapshot: no reason to wake its pollers.
    db.set_setting_quiet(key, &text).map_err(|e| e.to_string())
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

/// Keeps the newest `KEEP_RUNS`, and any older run with an item still open.
pub fn push_run(db: &crate::Db, id: &str, run: RoutineRun) -> Result<(), String> {
    // A runs row that no longer decodes starts over instead of losing every
    // later run (and its alerts).
    let mut runs = match load_runs(db, id) {
        Ok(runs) => runs,
        Err(error) if error.starts_with("Cannot read") => Vec::new(),
        Err(error) => return Err(error),
    };
    runs.insert(0, run);
    let older = runs.split_off(runs.len().min(KEEP_RUNS));
    runs.extend(older.into_iter().filter(|run| run.needs_you.iter().any(|item| !item.done)));
    if runs.len() > MAX_RUNS {
        // Never drop open items silently: one item says how many went.
        let dropped = runs.split_off(MAX_RUNS - 1);
        let open: Vec<&NeedsYou> = dropped.iter().flat_map(|r| r.needs_you.iter()).filter(|i| !i.done).collect();
        // An earlier summary stands for its own count (kept in `text`).
        let count: usize = dropped
            .iter()
            .map(|r| match r.id.starts_with("dropped-") {
                // Summaries from before the count was kept say it in the title.
                true if r.needs_you.iter().any(|i| !i.done) => r.text.parse().ok().or_else(|| r.needs_you[0].title.split(' ').next()?.parse().ok()).unwrap_or(1),
                true => 0,
                false => r.needs_you.iter().filter(|i| !i.done).count(),
            })
            .sum();
        if let Some(oldest) = open.iter().map(|i| i.seq).min() {
            let last = dropped.last().map(|r| r.started_ms).unwrap_or_default();
            runs.push(RoutineRun {
                id: format!("dropped-{oldest}"),
                routine_id: id.to_string(),
                started_ms: last,
                finished_ms: Some(last),
                ok: true,
                quiet: false,
                did: Vec::new(),
                needs_you: vec![NeedsYou { seq: oldest, title: format!("{count} older items were not handled"), detail: "They were too old to keep. Check this routine's sources if any still matter.".into(), link: None, done: false }],
                error: None,
                text: count.to_string(),
                ..Default::default()
            });
        }
    }
    save_runs(db, id, &runs)
}

pub fn memory_fits(memory: &Value) -> bool {
    memory.to_string().len() <= MAX_MEMORY
}

/// Whether an `Every` routine may run at this moment; `Daily` always may.
pub fn in_window<Tz: TimeZone>(cadence: &Cadence, now_ms: i64, tz: &Tz) -> bool {
    let Cadence::Every { from_hour, to_hour, .. } = cadence else { return true };
    let (from, to) = ((*from_hour).min(23), (*to_hour).min(24));
    if from == to {
        return true;
    }
    let hour = tz.timestamp_millis_opt(now_ms).single().map(|t| t.hour()).unwrap_or(from);
    if from < to { hour >= from && hour < to } else { hour >= from || hour < to }
}

pub fn in_window_local(cadence: &Cadence, now_ms: i64) -> bool {
    in_window(cadence, now_ms, &chrono::Local)
}

/// Whether this failure should reach the owner: a daily routine's first, or
/// a repeating one's second in a row (one blip is not worth a notification).
pub fn failure_alerts(cadence: &Cadence, failures: u32) -> bool {
    match cadence {
        Cadence::Daily { .. } => failures == 1,
        Cadence::Every { .. } => failures == 2,
    }
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

const BROWSER_NOTE: &str = "You can use {owner}'s own Chrome through the claude-in-chrome tools (load them with ToolSearch first). It is signed in to {owner}'s accounts. Use it when a source needs a consent screen for an account that is already signed in (open the link an MCP server gives you and approve it while that server is still running), or for work only a website allows. Never enter or submit passwords or one-time codes; if the right account is not signed in, put it in needs_you. Work in your own tab and never close {owner}'s.\n\n";

const COMPUTER_NOTE: &str = "You can see and operate Mac apps through the peekaboo tools (screenshots, the accessibility tree, clicks, typing, menus). Prefer an MCP server, a CLI or the browser when one can do the job; use the desktop only for work that lives in an app. {owner} may be using the Mac: act in the background on the exact window you snapshotted, do not bring apps forward or take the keyboard unless the task needs it, and leave every app as you found it. Never type into, submit or click on a password field, a sign-in or account-switch form, a two-factor prompt, or the Passwords, Keychain or any security dialog: if a task reaches one, stop and put it in needs_you with the account it wants. After any click whose result you did not see, look again before the next action; if a tool reports an error, do not retry blindly.\n\n";

/// `open` is what this scope's routines already reported and the owner has not
/// cleared: the run must not report it again, and says when one is handled.
/// `owner` is how prompts name the person Neko works for (`owner_name`).
#[allow(clippy::too_many_arguments)]
/// `viewers` is the Installed viewers section (`viewers::installed_section`), so a
/// routine that makes something to look at saves it as an artifact of its kind.
pub fn build_prompt(routine: &Routine, scope: &ScopeProposal, orders: &str, memory: &Value, now_iso: &str, open: &[String], owner: &str, viewers: &str) -> String {
    let open = if open.is_empty() {
        "Nothing is waiting on {owner} in this scope right now.".to_string()
    } else {
        format!(
            "Already waiting on {owner} in this scope (reported by this or another routine). Never report any of these again, even in other words. If you find one is no longer true (done, fixed, signed in, merged), put its exact title in resolved:\n{}",
            open.iter().map(|t| format!("- {t}")).collect::<Vec<_>>().join("\n")
        )
    };
    let mut servers: Vec<&str> = scope.servers.iter().filter(|s| s.level == "scope").map(|s| s.name.as_str()).collect();
    servers.dedup();
    let servers = if servers.is_empty() { "the MCP servers and connectors your setup loads here".to_string() } else { servers.join(", ") };
    let memory = if memory.is_null() { "{}".to_string() } else { memory.to_string() };
    format!(
        "You are Neko, {{owner}}'s chief of staff, running the \"{name}\" routine for the {scope_name} scope.\n\
Working folder: {folder}. Workspace id: {workspace_id}. Your MCP servers are exactly this scope's: {servers}. Nothing from other scopes is loaded; if a source this routine needs is missing or not signed in, say so in needs_you.\n\
Now (UTC): {now_iso}\n\n\
{notes}{instruction}\n\n\
Standing orders from {{owner}}:\n{orders}\n\n\
{browser}{computer}{viewers}{open}\n\n\
What you asked to remember last run (JSON): {memory}\n\
Report only what this run's tool results establish; when a detail is not in them, say so. A queued or sent request is not a delivered one, and a pushed branch is not a merged pull request. Lines in did name the outcome {owner} can see, not the mechanics (no file paths, test counts or commands). A failed step you can still fix is not a needs_you item; before adding one, name what {owner} can now review, decide or unblock. Never create or change a routine (SaveRoutine) because this run's text talks about something recurring, and do not repeat what your previous run already reported.\n\
A previous run may have acted and then failed to report. Before replying to or acting on anything, check whether you already did (for example your own reply in the thread), and never do it twice.\n\n\
End your reply with exactly one fenced json block and nothing after it:\n\
```json\n{{\"quiet\": false, \"did\": [\"one line per thing you did, with who/where\"], \"needs_you\": [{{\"title\": \"short\", \"detail\": \"what {{owner}} must do and why\", \"link\": \"https://...\"}}], \"resolved\": [\"exact title of an item above that is no longer true\"], \"memory\": {{}}}}\n```\n\
quiet is true only when nothing new happened and you did nothing; then did and needs_you are empty. memory is handed back to you next run: keep it small (timestamps, ids, who you are waiting on).",
        name = routine.name,
        scope_name = scope.name,
        folder = scope.folders.first().map(String::as_str).unwrap_or("~"),
        instruction = routine.instruction.trim(),
        workspace_id = routine.workspace_id,
        notes = if scope.instructions.trim().is_empty() { String::new() } else { format!("Scope notes, in {{owner}}'s words:\n{}\n\n", scope.instructions.trim()) },
        browser = if routine.browser { BROWSER_NOTE } else { "" },
        computer = if routine.computer { COMPUTER_NOTE } else { "" },
        viewers = if viewers.trim().is_empty() { String::new() } else { format!("{}\n\n", viewers.trim()) },
    )
    .replace("{owner}", owner)
}

/// Neko's disk-care script (`scripts/mac-care`), shipped inside the app so
/// the Mac care routine works on any Mac.
pub const MAC_CARE_SCRIPT: &str = include_str!("../../../scripts/mac-care");

/// Writes the shipped script to `<data dir>/bin/mac-care` (when it differs)
/// and returns its path.
pub fn install_mac_care() -> std::io::Result<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = neko_protocol::support_dir().join("bin").join("mac-care");
    if std::fs::read_to_string(&path).ok().as_deref() != Some(MAC_CARE_SCRIPT) {
        std::fs::create_dir_all(path.parent().unwrap_or(&path))?;
        let temp = path.with_extension("new");
        std::fs::write(&temp, MAC_CARE_SCRIPT)?;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o755))?;
        std::fs::rename(&temp, &path)?;
    }
    Ok(path)
}

/// What prompts call the person this Mac belongs to: the first word of the
/// account's full name (System Settings › Users), else the login name.
pub fn owner_name() -> String {
    // SAFETY: getpwuid returns a pointer into static storage or null; the
    // fields are read at once and copied.
    let full = unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() || (*entry).pw_gecos.is_null() {
            None
        } else {
            std::ffi::CStr::from_ptr((*entry).pw_gecos).to_str().ok().map(str::to_string)
        }
    };
    full.as_deref()
        .and_then(|name| name.split(|c: char| c.is_whitespace() || c == ',').find(|w| !w.is_empty()))
        .map(str::to_string)
        .or_else(|| std::env::var("USER").ok().filter(|u| !u.is_empty()))
        .unwrap_or_else(|| "the owner".into())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub quiet: bool,
    pub did: Vec<String>,
    /// `seq` is 0 until the daemon assigns one.
    pub needs_you: Vec<NeedsYou>,
    /// Titles of open items the run found no longer true.
    pub resolved: Vec<String>,
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
    let resolved = object
        .get("resolved")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).take(MAX_DID).collect())
        .unwrap_or_default();
    Ok(Parsed { quiet, did, needs_you, resolved, memory })
}

/// How every seeded instruction finds the owner's accounts without knowing
/// them: nothing about any one person or company is written into Neko.
/// Every code change a routine or the full-power chat makes is checked by
/// someone who did not write it, the way Neko's own tasks are.
pub const REVIEW_BEFORE_PUSH: &str = "Before you push or report a non-trivial code change as done, have it reviewed by a fresh subagent that did not write it (the Agent tool; give it the diff, what the change is for, and how to verify), fix what it finds that is real, and say in did that it was reviewed. A clean rebase, a merge with no conflicts or a one-line obvious fix needs no review.";

const FIND_ACCOUNTS: &str = "Find {owner}'s GitHub login with `gh api user --jq .login` and the organisation each repository belongs to with `git remote -v`; the scope notes and this scope's MCP servers say which chat, mail, calendar and issue tools to use.";

/// The instruction a seeded routine of this kind gets.
pub fn seed_instruction(kind: RoutineKind, personal: bool) -> String {
    instruction(kind, personal)
}

/// Bumped when seeded instructions change; older ones are rewritten once.
pub const INSTRUCTIONS_VERSION: u32 = 6;

fn instruction(kind: RoutineKind, personal: bool) -> String {
    let own = if personal {
        " This is {owner}'s personal scope: only repositories owned by {owner}'s own GitHub account, and personal mail and calendar. Work in any company or client organisation belongs to that organisation's own scope: never report or act on it here."
    } else {
        ""
    };
    match kind {
        RoutineKind::Messages => format!(
            "Check everything that came in for {{owner}} in this scope since memory.last_check (if missing, the last 24 hours): direct messages and mentions, email, issue-tracker comments and mentions, and review comments on {{owner}}'s pull requests. {FIND_ACCOUNTS}{own}\n\
Handle each one yourself: reply, answer, acknowledge, look things up in code or data to answer properly, fix small things, or start the work. {REVIEW_BEFORE_PUSH} Keep memory.last_check (UTC ISO time) and memory.expecting_reply_from (people you are waiting on) current. If a source fails because a sign-in is missing, put that in needs_you once, then remember you reported it."
        ),
        RoutineKind::Digest => format!(
            "Morning brief for this scope. {FIND_ACCOUNTS}{own} Look at: {{owner}}'s open pull requests (CI, review state, unresolved comments), pull requests waiting on {{owner}}'s review, issues assigned to {{owner}} that changed in the last 24 hours, today's calendar, and anything urgent from monitoring.{machine}\n\
Act on what you can first: answer review comments, fix or re-run failing CI, rebase, review the PRs waiting on {{owner}} when the change is clear. {REVIEW_BEFORE_PUSH} Then report: did = what you handled; needs_you = only the few things {{owner}} must do today, each with a link.",
            machine = if personal { " Also one line on the machine: free disk (df -h /System/Volumes/Data) and load (uptime)." } else { "" }
        ),
        RoutineKind::PrSweep => format!(
            "Pull request and branch sweep. {FIND_ACCOUNTS}{own} Go through {{owner}}'s open pull requests and remote branches: rebase stale ones on their base, fix failing CI when the fix is clear, address review comments, nudge reviewers on PRs waiting more than two days, and close PRs and delete branches that are merged elsewhere, superseded or dead.\n\
Do the work, do not just list it. Work in a temporary git worktree (`git worktree add` under /tmp, removed when done) so {{owner}}'s checkouts are never touched. Resolve conflicts yourself when the right result is clear from both sides, run the tests that cover the changed files, and push. {REVIEW_BEFORE_PUSH} When a PR has no reviewer, request one from CODEOWNERS or whoever reviewed that repository's recent PRs. Only a conflict whose right resolution needs a product decision goes in needs_you, with what the choice is. Report each action in did."
        ),
        RoutineKind::Reflect => "Learn {owner}'s standing orders for this scope from what {owner} actually does. Neko's own command line is `'{neko}' --request '<json>'` (it prints the JSON reply; `'{neko}' --protocol` prints every request it accepts). \
Read this scope's routines and their recent runs with `'{neko}' --request '\"Routines\"'` (only routines whose workspace_id is this scope's): what Neko did, which needs_you items {owner} marked done, which were left open for days, and the current standing_orders for this workspace. Read Home chat with `'{neko}' --request '{\"Workbench\":\"Snapshot\"}'` (the conversation) for corrections or preferences {owner} stated about this scope's work, and the memory entries for this workspace.\n\
When a clear pattern shows a rule {owner} wants (a correction to how a routine acted, a kind of item {owner} always handles personally or keeps ignoring, a request to do something differently), update this scope's standing orders with `'{neko}' --request '{\"SetStandingOrders\":{\"workspace_id\":\"<this workspace id>\",\"text\":\"<full new text>\"}}'`: keep every existing sentence, add or adjust at most two short rules per day, and write them as plain instructions in {owner}'s voice. Never loosen a rule {owner} wrote. Put each change in did with the evidence (which runs or messages). If nothing is clear, change nothing and be quiet. ".to_string() + crate::memory_learning::LEARNING_RULES,
        RoutineKind::MacCare => "Keep this Mac's disk healthy. Run `'{mac_care}'` (a report that changes nothing). If free space is under 60 GB, or any step would free more than 1 GB, run `'{mac_care}' --apply`; it clears only data that rebuilds itself. Put the GB freed in did. If free space is still under 30 GB afterwards, put one needs_you item listing its \"Yours to decide\" lines with sizes (Trash, Downloads, old agent transcripts, simulators); never delete those yourself. When there is nothing to clean and space is fine, be quiet.".to_string(),
        RoutineKind::Custom => String::new(),
    }
}

/// Four routines per scope with a workspace (messages, morning brief, PR
/// sweep, learning standing orders); the scope whose folder is `home` is the
/// personal one and also gets Mac care. A scope with a chat server (Slack,
/// Teams, Discord) checks messages every 20 minutes, others hourly.
pub fn seeds(scopes: &[ScopeProposal], home: &str) -> Vec<Routine> {
    let mut routines = Vec::new();
    for scope in scopes {
        let Some(workspace_id) = scope.workspace_id.clone() else { continue };
        let chatty = scope.servers.iter().any(|s| ["slack", "teams", "discord"].iter().any(|chat| s.name.to_lowercase().contains(chat)));
        let personal = scope.folders.iter().any(|folder| folder.trim_end_matches('/') == home.trim_end_matches('/'));
        let specs = [
            (RoutineKind::Messages, "Messages", Cadence::Every { minutes: if chatty { 20 } else { 60 }, from_hour: 8, to_hour: 23 }),
            (RoutineKind::Digest, "Morning brief", Cadence::Daily { hour: 10, minute: 30 }),
            (RoutineKind::PrSweep, "PR sweep", Cadence::Daily { hour: 11, minute: 0 }),
            (RoutineKind::Reflect, "Learn standing orders", Cadence::Daily { hour: 21, minute: 30 }),
        ];
        let care = [(RoutineKind::MacCare, "Mac care", Cadence::Daily { hour: 4, minute: 0 })];
        let specs: Vec<_> = specs.into_iter().chain(care.into_iter().filter(|_| personal)).collect();
        for (kind, name, cadence) in specs {
            routines.push(Routine {
                id: format!("{workspace_id}-{}", format!("{kind:?}").to_lowercase()),
                workspace_id: workspace_id.clone(),
                kind,
                name: name.into(),
                instruction: instruction(kind, personal),
                cadence,
                enabled: true,
                model: "sonnet".into(),
                paseo_schedule: None,
                browser: !matches!(kind, RoutineKind::Reflect | RoutineKind::MacCare),
                computer: !matches!(kind, RoutineKind::Reflect | RoutineKind::MacCare),
                customized: false,
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
        assert!(parsed.resolved.is_empty());
        let parsed = parse_reply("```json\n{\"quiet\": true, \"resolved\": [\"Gmail API not enabled\", \"\"]}\n```").unwrap();
        assert_eq!(parsed.resolved, vec!["Gmail API not enabled"]);
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
    fn seeds_are_about_no_one_in_particular() {
        let scope = |name: &str, id: Option<&str>| ScopeProposal { name: name.into(), folders: vec!["/x".into()], servers: vec![], problems: vec![], workspace_id: id.map(str::to_string), discovered: true, instructions: String::new() };
        let mut chat = scope("Work", Some("w1"));
        chat.servers = vec![neko_protocol::routines::ScopeServer { name: "slack-work".into(), origin: "x".into(), level: "scope".into(), repo: None }];
        let mut home = scope("Me", Some("w2"));
        home.folders = vec!["/Users/n".into()];
        let routines = seeds(&[chat, home, scope("Loose", None)], "/Users/n/");
        // Four per scope; the home scope also gets Mac care.
        assert_eq!(routines.len(), 9);
        assert_eq!(routines[0].cadence, Cadence::Every { minutes: 20, from_hour: 8, to_hour: 23 });
        assert_eq!(routines[4].cadence, Cadence::Every { minutes: 60, from_hour: 8, to_hour: 23 });
        assert_eq!(routines.iter().filter(|r| r.kind == RoutineKind::MacCare).map(|r| r.workspace_id.as_str()).collect::<Vec<_>>(), vec!["w2"]);
        assert!(routines.iter().all(|r| r.paseo_schedule.is_none() && r.id.starts_with(&r.workspace_id)));
        assert!(routines[4].instruction.contains("personal scope"));
        for kind in [RoutineKind::Messages, RoutineKind::Digest, RoutineKind::PrSweep] {
            assert!(routines.iter().find(|r| r.kind == kind).unwrap().instruction.contains("fresh subagent that did not write it"), "{kind:?}");
        }
        let reflect = routines.iter().find(|r| r.kind == RoutineKind::Reflect).unwrap();
        assert!(reflect.instruction.contains("'{neko}' --request") && !reflect.browser && !reflect.computer);
        for r in &routines {
            for word in ["Taylor", "Acme", "Client A", "TaylorEC", "slack-client-a", "/Users/example"] {
                assert!(!r.instruction.contains(word), "{word} in {:?}", r.kind);
            }
        }
        let prompt = build_prompt(&routines[0], &ScopeProposal { instructions: "Slack is slack-work.".into(), ..scope("Work", Some("w1")) }, DEFAULT_STANDING_ORDERS, &Value::Null, "now", &[], "Asha", &crate::viewers::installed_section("{neko}", std::path::Path::new("/nonexistent")));
        assert!(prompt.contains("Installed viewers.") && prompt.contains("'{neko}' --request '{\"SaveArtifact\""), "a routine learns the viewers");
        assert!(prompt.contains("Asha's chief of staff") && prompt.contains("You act for Asha") && prompt.contains("Scope notes, in Asha's words:\nSlack is slack-work."));
        assert!(!prompt.contains("{owner}"));
    }

    #[test]
    fn open_items_survive_trimming() {
        let db = crate::Db::open_in_memory().unwrap();
        let item = NeedsYou { seq: 1, title: "t".into(), detail: String::new(), link: None, done: false };
        let run = |i: i64, open: bool| RoutineRun { id: i.to_string(), routine_id: "r".into(), started_ms: i, finished_ms: None, ok: true, quiet: false, did: vec![], needs_you: if open { vec![item.clone()] } else { vec![] }, error: None, text: String::new(), ..Default::default() };
        push_run(&db, "r", run(0, true)).unwrap();
        for i in 1..80 {
            push_run(&db, "r", run(i, false)).unwrap();
        }
        let runs = load_runs(&db, "r").unwrap();
        assert_eq!(runs.len(), 51);
        assert_eq!(runs.last().unwrap().id, "0");
        // Open items everywhere still stop at MAX_RUNS.
        for i in 80..400 {
            push_run(&db, "r", run(i, true)).unwrap();
        }
        let runs = load_runs(&db, "r").unwrap();
        assert_eq!(runs.len(), MAX_RUNS);
        let summary = runs.last().unwrap();
        // 321 runs with an open item; 199 are kept, so 122 were dropped.
        assert_eq!(summary.needs_you[0].title, "122 older items were not handled");
        assert_eq!(summary.needs_you[0].seq, 1);
    }

    #[test]
    fn corrupt_runs_row_starts_over() {
        let db = crate::Db::open_in_memory().unwrap();
        db.set_setting(&runs_key("r"), "{not json").unwrap();
        let run = RoutineRun { id: "1".into(), routine_id: "r".into(), started_ms: 0, finished_ms: None, ok: true, quiet: false, did: vec![], needs_you: vec![], error: None, text: String::new(), ..Default::default() };
        push_run(&db, "r", run).unwrap();
        assert_eq!(load_runs(&db, "r").unwrap().len(), 1);
    }

    #[test]
    fn window_and_failure_rules() {
        let every = Cadence::Every { minutes: 20, from_hour: 8, to_hour: 23 };
        assert!(in_window(&every, at(10, 0), &ist()));
        assert!(!in_window(&every, at(4, 46), &ist()));
        assert!(!in_window(&every, at(23, 0), &ist()));
        assert!(in_window(&Cadence::Daily { hour: 10, minute: 30 }, at(4, 0), &ist()));
        assert!(in_window(&Cadence::Every { minutes: 20, from_hour: 0, to_hour: 0 }, at(3, 0), &ist()));
        assert!(failure_alerts(&Cadence::Daily { hour: 1, minute: 0 }, 1));
        assert!(!failure_alerts(&every, 1));
        assert!(failure_alerts(&every, 2));
        assert!(!failure_alerts(&every, 3));
        assert!(memory_fits(&serde_json::json!({"a": 1})));
        assert!(!memory_fits(&serde_json::json!({"a": "x".repeat(MAX_MEMORY)})));
    }

    #[test]
    fn runs_keep_fifty() {
        let db = crate::Db::open_in_memory().unwrap();
        for i in 0..55 {
            let run = RoutineRun { id: i.to_string(), routine_id: "r".into(), started_ms: i, finished_ms: None, ok: true, quiet: false, did: vec![], needs_you: vec![], error: None, text: String::new(), ..Default::default() };
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
