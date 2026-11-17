//! What a Luau plugin (a card or a view a pack ships, `src/luau/`) may ask of the daemon, and how its panels reach it.
//!
//! * **`host.read(name, args)` is a read, never a write.** [`READS`] is the whole list; `name` is matched against it
//!   and everything else is refused with a sentence, before anything is sent. Each name builds one fixed request
//!   ([`read_request`]), so a plugin cannot reach a request by spelling its JSON: it cannot name `DeleteTask`,
//!   `SendMessage` or any `Command` at all. Answers are cut to a small projection (`Workbench` is the tasks and two
//!   counts, not the multi-megabyte snapshot) and refused above [`MAX_ANSWER_BYTES`].
//! * **`host.storage`** is the daemon's plugin storage over HTTP (`/api/storage/<namespace>/<key>`, the web UI's
//!   own), in a namespace of the pack's own ([`namespace`]): `luau_<pack>`, which no web plugin id can equal, because a
//!   pack name is `[a-z0-9-]` and the pack's own web plugin is `<pack>-<leaf>`. A pack is held to [`MAX_STORAGE_BYTES`].
//! * **[`sync`]** is how Rune learns what to run: the pack list, then `PackUi` for each healthy pack that has
//!   Luau files. The daemon answers `PackUi` only while the pack is Healthy and its files are exactly what the person approved.
//!
//! A [`Wire`] is the one handle all of it goes through, so tests answer with a fake and count what was sent.

use std::sync::Arc;

use neko_protocol::packs::{PackCommand, PackStatus, PackUiFile};
use neko_protocol::workbench::Command;
use neko_protocol::{Request, Response};
use serde_json::{Value, json};

use super::http::{Http, HttpError};
use super::{LinkError, ask, calm};

/// The reads a plugin may make, and nothing else. Each is idempotent and changes nothing.
pub const READS: &[&str] = &["Workbench", "Routines", "MorningDigest", "MachineHealth", "UnseenTasks"];
/// The most a read's answer may hold once it is JSON.
pub const MAX_ANSWER_BYTES: usize = 256 * 1024;
/// The most a read's arguments may hold once they are JSON.
pub const MAX_ARGS_BYTES: usize = 8 * 1024;
/// A pack's storage, all keys together.
pub const MAX_STORAGE_BYTES: usize = 256 * 1024;
/// How long a `Workbench` or `Routines` answer is reused: the daemon builds the whole snapshot for each, so a plugin asking often
/// (four a second is its limit, and there may be several plugins) does not make the daemon do that again and again.
pub const CACHE_SECONDS: u64 = 2;
/// The most tasks `Workbench` lists.
const MAX_TASKS: usize = 100;

/// What the live wire keeps of the heavy reads: the answer and when it was made, by read name.
type Kept = Arc<std::sync::Mutex<std::collections::HashMap<&'static str, (std::time::Instant, Value)>>>;

/// How a plugin's wire reaches the daemon: a request and its answer, and the HTTP client when the daemon's address is known.
#[derive(Clone)]
pub struct Wire {
    ask: Arc<dyn Fn(Request) -> Result<Response, String> + Send + Sync>,
    pub http: Option<Http>,
    /// A fixed "now" in milliseconds (scenes and tests), else the system's.
    pub fixed_ms: Option<i64>,
    /// Recent answers to the heavy reads (the live wire only).
    cache: Option<Kept>,
}

impl Wire {
    /// Nowhere: every ask says so.
    pub fn none() -> Wire {
        Wire { ask: Arc::new(|_| Err("Neko isn't connected yet.".to_owned())), http: None, fixed_ms: None, cache: None }
    }

    /// The daemon's socket: a fresh connection for each ask, as every other call does.
    pub fn socket(path: std::path::PathBuf, http: Option<Http>) -> Wire {
        Wire { ask: Arc::new(move |request| ask(&path, request).map_err(|e| sentence(&e))), http, fixed_ms: None, cache: Some(Arc::default()) }
    }

    /// A stand-in daemon for scenes: answers `Workbench` and `Routines` with this made-up data and refuses everything else, with its clock
    /// stopped at `now_ms`.
    pub fn demo(workbench: neko_protocol::workbench::Snapshot, routines: super::RoutinesData, now_ms: i64) -> Wire {
        Wire::fake(
            move |request| match request {
                Request::Workbench(_) => Ok(Response::Workbench(workbench.clone())),
                Request::Routines => Ok(Response::Routines {
                    routines: routines.views.clone(),
                    standing_orders: routines.standing_orders.clone(),
                    scopes: routines.scopes.clone(),
                    owner_name: routines.owner_name.clone(),
                    age_out_days: routines.age_out_days,
                    routines_on: routines.routines_on,
                    spend_today: routines.spend_today,
                }),
                other => Err(format!("the demo daemon does not answer {other:?}")),
            },
            None,
        )
        .at(now_ms)
    }

    /// A stand-in that answers every request with `answer` (tests and scenes).
    pub fn fake(answer: impl Fn(Request) -> Result<Response, String> + Send + Sync + 'static, http: Option<Http>) -> Wire {
        Wire { ask: Arc::new(answer), http, fixed_ms: None, cache: None }
    }

    /// The same wire with the clock stopped at `ms` (so a picture of it is the same every time).
    pub fn at(mut self, ms: i64) -> Wire {
        self.fixed_ms = Some(ms);
        self
    }

    pub fn ask(&self, request: Request) -> Result<Response, String> {
        (self.ask)(request)
    }

    /// Now, in milliseconds since 1970.
    pub fn now_ms(&self) -> i64 {
        self.fixed_ms.unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64))
    }
}

fn sentence(error: &LinkError) -> String {
    match error {
        LinkError::Daemon(message) => message.clone(),
        other => calm(other, std::time::Duration::from_secs(1)).split(" Trying again").next().unwrap_or("Neko didn't answer.").to_owned(),
    }
}

/// Whether `name` is a read a plugin may make.
pub fn is_read(name: &str) -> bool {
    READS.contains(&name)
}

/// The one fixed request a read name stands for. Anything not in [`READS`] is refused here, before anything is sent.
pub fn read_request(name: &str, _args: &Value) -> Result<Request, String> {
    if !is_read(name) {
        return Err(format!("{name} is not a read Neko offers plugins. Reads: {}.", READS.join(", ")));
    }
    Ok(match name {
        "Workbench" => Request::Workbench(Command::Snapshot),
        "Routines" => Request::Routines,
        "MorningDigest" => Request::MorningDigest,
        "MachineHealth" => Request::MachineHealth,
        "UnseenTasks" => Request::UnseenTasks,
        _ => return Err(format!("{name} is not a read Neko offers plugins.")),
    })
}

/// Makes a read and shapes its answer for the plugin.
pub fn read(wire: &Wire, name: &str, args: &Value, now_ms: i64) -> Result<Value, String> {
    let request = read_request(name, args)?;
    let heavy = matches!(name, "Workbench" | "Routines");
    let kept = |name: &str| wire.cache.as_ref().and_then(|c| c.lock().ok()).and_then(|c| c.get(name).filter(|(at, _)| at.elapsed().as_secs() < CACHE_SECONDS).map(|(_, v)| v.clone()));
    if heavy && let Some(hit) = kept(name) {
        return Ok(hit);
    }
    let response = wire.ask(request)?;
    let value = answer(name, response, now_ms)?;
    let size = serde_json::to_string(&value).map_or(usize::MAX, |t| t.len());
    if size > MAX_ANSWER_BYTES {
        return Err(format!("The answer to {name} is too large for a plugin ({} KB, at most {} KB).", size / 1024, MAX_ANSWER_BYTES / 1024));
    }
    if heavy && let Some(mut cache) = wire.cache.as_ref().and_then(|c| c.lock().ok()) {
        let key = if name == "Workbench" { "Workbench" } else { "Routines" };
        cache.insert(key, (std::time::Instant::now(), value.clone()));
    }
    Ok(value)
}

/// What a plugin sees of an answer: the tasks and counts for `Workbench`, a short row per routine for `Routines`,
/// and the daemon's own JSON (without its envelope) for the small ones.
pub fn answer(name: &str, response: Response, now_ms: i64) -> Result<Value, String> {
    match (name, response) {
        (_, Response::Error { message }) => Err(message),
        ("Workbench", Response::Workbench(snapshot)) => Ok(workbench_value(&super::Summary::from_snapshot_for(&snapshot, None))),
        ("Routines", Response::Routines { routines, .. }) => Ok(routines_value(&routines, now_ms)),
        ("MorningDigest", Response::Digest { digest }) => Ok(digest_value(digest.as_ref())),
        (_, other) => {
            let value = serde_json::to_value(&other).map_err(|e| e.to_string())?;
            Ok(match value {
                Value::Object(map) if map.len() == 1 => map.into_iter().next().map_or(Value::Null, |(_, inner)| inner),
                other => other,
            })
        }
    }
}

/// `{ day, at_ms, headline, finished = [text], stuck = [text], decisions = [{ text, default }] }`, or nil before the first digest:
/// what a card shows of it, without the calendar line and the spend.
pub fn digest_value(digest: Option<&neko_protocol::digest::Digest>) -> Value {
    let Some(d) = digest else { return Value::Null };
    let lines = |items: &[neko_protocol::digest::DigestLine]| Value::Array(items.iter().take(20).map(|l| Value::String(l.text.chars().take(300).collect())).collect());
    json!({
        "day": d.day,
        "at_ms": d.at_ms,
        "headline": d.headline.chars().take(300).collect::<String>(),
        "finished": lines(&d.finished),
        "stuck": lines(&d.stuck),
        "decisions": d.decisions.iter().take(20).map(|l| json!({ "text": l.text.chars().take(300).collect::<String>(), "default": l.default })).collect::<Vec<_>>(),
    })
}

fn kind_name(kind: crate::status::StatusKind) -> &'static str {
    use crate::status::StatusKind::*;
    match kind {
        Backlog => "backlog",
        Todo => "todo",
        InProgress => "in-progress",
        Review => "review",
        Done => "done",
        Failed => "failed",
        Cancelled => "cancelled",
    }
}

/// `{ working, needs_you, tasks = [{ id, key, title, status, glyph, wait, needs_you, updated_ms }] }`, newest first.
pub fn workbench_value(summary: &super::Summary) -> Value {
    use neko_protocol::workbench::TaskStatus;
    let mut rows: Vec<&super::TaskRow> = summary.tasks.iter().collect();
    rows.sort_by_key(|t| std::cmp::Reverse(t.updated_at_ms));
    let tasks: Vec<Value> = rows
        .into_iter()
        .take(MAX_TASKS)
        .map(|t| {
            json!({
                "id": t.id,
                "key": t.key,
                "title": t.title,
                "status": format!("{:?}", t.status),
                "glyph": kind_name(crate::status::status_of(t.status)),
                "wait": format!("{:?}", t.wait).to_lowercase(),
                "needs_you": matches!(t.status, TaskStatus::AwaitingApproval | TaskStatus::Failed | TaskStatus::ReadyForReview),
                "updated_ms": t.updated_at_ms,
            })
        })
        .collect();
    json!({ "working": summary.working(), "needs_you": crate::attention::needs_user(summary), "tasks": tasks })
}

/// `[{ id, name, enabled, cadence, state, glyph, last }]`: one short row per routine.
pub fn routines_value(views: &[neko_protocol::routines::RoutineView], now_ms: i64) -> Value {
    use crate::pages::RoutineGlyph;
    use crate::status::StatusKind;
    Value::Array(
        views
            .iter()
            .take(100)
            .map(|v| {
                let (state, glyph) = match crate::pages::routine_glyph(v, now_ms) {
                    RoutineGlyph::On => ("on", StatusKind::Todo),
                    RoutineGlyph::Running => ("running", StatusKind::InProgress),
                    RoutineGlyph::Held(_) => ("held", StatusKind::InProgress),
                    RoutineGlyph::Failed => ("failed", StatusKind::Failed),
                    RoutineGlyph::Paused => ("paused", StatusKind::Backlog),
                };
                let last = v.runs.first().map(|r| if !r.ok { "failed" } else if r.did.is_empty() { "quiet" } else { "acted" });
                json!({
                    "id": v.routine.id,
                    "name": v.routine.name,
                    "enabled": v.routine.enabled,
                    "cadence": crate::pages::cadence_text(&v.routine.cadence),
                    "state": state,
                    "glyph": kind_name(glyph),
                    "last": last,
                    "last_ms": v.state.last_run_ms,
                })
            })
            .collect(),
    )
}

// ---- Pack sync ----------------------------------------------------------------------------------------------------------

/// What one pack that declares Luau files answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackSync {
    pub name: String,
    pub title: String,
    /// The `.luau` paths its manifest lists.
    pub declared: Vec<String>,
    /// The files, or why none are served (not healthy, changed on disk, not installed).
    pub outcome: Result<Vec<PackUiFile>, String>,
}

/// Why a pack that is not Healthy serves nothing, in a sentence for the person.
pub fn unhealthy(status: PackStatus) -> &'static str {
    match status {
        PackStatus::Healthy => "",
        PackStatus::NeedsSetup => "a program it needs is missing",
        PackStatus::Failing => "its health check failed",
        PackStatus::Changed => "its files changed on disk since you approved them",
    }
}

/// The pack list, then `PackUi` for each pack that has Luau files and is Healthy. A pack that is not Healthy is not asked for.
pub fn sync(wire: &Wire) -> Result<Vec<PackSync>, String> {
    let listing = match wire.ask(Request::Packs(PackCommand::List))? {
        Response::Packs(listing) => listing,
        Response::Error { message } => return Err(message),
        _ => return Err("Neko answered the pack list in a way this app doesn't understand.".to_owned()),
    };
    let mut out = Vec::new();
    for pack in listing.installed.iter().filter(|p| !p.info.contents.luau.is_empty()) {
        let outcome = if pack.status != PackStatus::Healthy {
            Err(format!("{} is not running: {}.", pack.info.title, unhealthy(pack.status)))
        } else {
            match wire.ask(Request::PackUi { name: pack.info.name.clone() }) {
                Ok(Response::PackUi(files)) => Ok(files),
                Ok(Response::Error { message }) | Err(message) => Err(message),
                Ok(_) => Err("Neko answered in a way this app doesn't understand.".to_owned()),
            }
        };
        out.push(PackSync { name: pack.info.name.clone(), title: pack.info.title.clone(), declared: pack.info.contents.luau.clone(), outcome });
    }
    Ok(out)
}

// ---- Storage ------------------------------------------------------------------------------------------------------------

/// The storage namespace of a pack's panels. Not the pack's name: a web plugin may be called that, and this must not
/// read its data. A pack name is `[a-z0-9-]` and its own web plugin `<pack>-<leaf>`, so no plugin id has an underscore here.
pub fn namespace(pack: &str) -> String {
    format!("luau_{pack}")
}

/// The daemon's key rule (`valid_storage_key`), checked here so a bad key is a sentence, not a 400.
pub fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 128 && !key.starts_with('.') && key.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn problem(error: HttpError) -> String {
    error.0
}

/// One stored value, or none when the key was never set.
pub fn storage_get(wire: &Wire, pack: &str, key: &str) -> Result<Value, String> {
    if !valid_key(key) {
        return Err(format!("“{key}” is not a storage key: letters, digits, \".\", \"_\" and \"-\", at most 128, no leading dot."));
    }
    let http = wire.http.as_ref().ok_or("Neko's address isn't known yet.")?;
    http.storage_get(&namespace(pack), key).map_err(problem)
}

/// Stores `value` under `key`, holding the pack to [`MAX_STORAGE_BYTES`] across all its keys.
pub fn storage_set(wire: &Wire, pack: &str, key: &str, value: &Value) -> Result<(), String> {
    if !valid_key(key) {
        return Err(format!("“{key}” is not a storage key: letters, digits, \".\", \"_\" and \"-\", at most 128, no leading dot."));
    }
    let body = serde_json::to_string(value).map_err(|e| e.to_string())?;
    if body.len() > MAX_STORAGE_BYTES {
        return Err(format!("That value is too big to store ({} KB; a pack keeps at most {} KB).", body.len() / 1024, MAX_STORAGE_BYTES / 1024));
    }
    let http = wire.http.as_ref().ok_or("Neko's address isn't known yet.")?;
    let ns = namespace(pack);
    // The size check and the write are one step: two sets at once cannot both pass the check and together go over.
    static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one_at_a_time = WRITING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let others: usize = http.storage_list(&ns).map_err(problem)?.into_iter().filter(|(k, _)| k != key).map(|(_, size)| size as usize).sum();
    if others + body.len() > MAX_STORAGE_BYTES {
        return Err(format!("A pack keeps at most {} KB in storage; this would make {} KB.", MAX_STORAGE_BYTES / 1024, (others + body.len()) / 1024));
    }
    http.storage_set(&ns, key, &body).map_err(problem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_listed_reads_build_a_request_and_a_write_name_never_does() {
        for name in READS {
            assert!(read_request(name, &json!({})).is_ok(), "{name}");
        }
        assert!(!READS.contains(&"CalendarEvents"), "the calendar is the most personal thing Neko holds: not a plugin's to read");
        for write in ["DeleteTask", "SendMessage", "CancelTask", "ApproveTask", "SaveRoutine", "Packs", "McpBridge", "SaveArtifact", "RunRoutineNow", "Workbench::Snapshot", "workbench", "", "Request"] {
            let error = read_request(write, &json!({})).unwrap_err();
            assert!(error.contains("is not a read"), "{write}: {error}");
        }
    }

    #[test]
    fn a_read_is_one_fixed_request_whatever_the_arguments_say() {
        // Arguments never choose the request: they only fill the fields the read has.
        let evil = json!({ "Workbench": { "DeleteTask": { "id": "t" } }, "from_ms": 1, "to_ms": 2 });
        assert!(matches!(read_request("MorningDigest", &evil).unwrap(), Request::MorningDigest));
        assert!(matches!(read_request("Workbench", &evil).unwrap(), Request::Workbench(Command::Snapshot)));
    }

    #[test]
    fn a_big_answer_is_refused_not_handed_over() {
        let wire = Wire::fake(|_| Ok(Response::UnseenTasks { ids: (0..30_000).map(|i| format!("task-id-{i:08}")).collect() }), None);
        assert!(read(&wire, "UnseenTasks", &json!({}), 0).unwrap_err().contains("too large"));
    }

    #[test]
    fn the_digest_a_plugin_sees_has_no_calendar_or_spend() {
        let digest = neko_protocol::digest::Digest { headline: "3 things finished".into(), calendar: "3 events, first: a private meeting".into(), spend: "Yesterday: $9".into(), ..Default::default() };
        let value = digest_value(Some(&digest));
        assert_eq!(value["headline"], "3 things finished");
        let text = value.to_string();
        assert!(!text.contains("private meeting") && !text.contains("$9"), "{text}");
        assert_eq!(digest_value(None), Value::Null);
    }

    #[test]
    fn the_heavy_reads_are_kept_for_two_seconds_on_the_live_wire_only() {
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c = count.clone();
        let mut wire = Wire::fake(
            move |_| {
                c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(Response::Workbench(neko_protocol::workbench::Snapshot::default()))
            },
            None,
        );
        for _ in 0..3 {
            read(&wire, "Workbench", &json!({}), 0).unwrap();
        }
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 3, "a stand-in wire is never cached");
        wire.cache = Some(Arc::default());
        for _ in 0..3 {
            read(&wire, "Workbench", &json!({}), 0).unwrap();
        }
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 4, "one more, then the kept answer");
    }

    #[test]
    fn a_daemon_refusal_is_the_error() {
        let wire = Wire::fake(|_| Ok(Response::Error { message: "nope".into() }), None);
        assert_eq!(read(&wire, "Routines", &json!({}), 0).unwrap_err(), "nope");
    }

    #[test]
    fn keys_follow_the_daemons_rule_and_the_namespace_is_the_packs_own() {
        assert!(valid_key("a.b_c-1") && !valid_key("") && !valid_key(".hidden") && !valid_key("a/b") && !valid_key("a b") && !valid_key(&"k".repeat(129)));
        assert_eq!(namespace("hello-luau"), "luau_hello-luau");
        // No web plugin id can be this: pack-made ones are `<pack>-<leaf>`, and a pack name has no underscore.
        assert!(namespace("x").contains('_'));
    }
}
