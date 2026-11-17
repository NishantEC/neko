//! Signals: trouble that repeated. Observations are kept in their own quiet row
//! (`improve_signals_v1`) with the day they were seen, so a signal can be counted again later
//! (to verify an apply) even after the data it came from was trimmed.
//!
//! | Key | Fires when |
//! | --- | --- |
//! | `missing_tool:<name>` | a run failed or a turn's text or error says a tool or command is missing |
//! | `correction:<topic>` | the same correction topic was learned 3+ times in 14 days |
//! | `ask:<title key>` | an Inbox item with the same normalised title was raised 3+ times in 7 days |
//!
//! Gathering happens when the Mac is idle, at most every 6 hours, and on the events the daemon
//! names (a run failing, a chat turn failing), at most every 10 minutes. With "Suggestions" off
//! nothing is read and nothing is written.
use super::{enabled, DAY_MS, PER_DAY, REJECT_MEMORY_DAYS};
use crate::Db;
use neko_protocol::improve::ImproveState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const ROW: &str = "improve_signals_v1";
pub const GATHER_EVERY_MS: i64 = 6 * 60 * 60 * 1000;
pub const EVENT_EVERY_MS: i64 = 10 * 60 * 1000;
const KEEP_DAYS: i64 = 30;
const MAX_OBS: usize = 1500;
const EXCERPT: usize = 300;
pub const CORRECTION_TIMES: usize = 3;
pub const CORRECTION_DAYS: i64 = 14;
pub const ASK_TIMES: usize = 3;
pub const ASK_DAYS: i64 = 7;
pub const MISSING_DAYS: i64 = 7;
/// A key whose proposal was dropped (the model had nothing, or it did not validate) waits this long.
const DROPPED_BACKOFF_MS: i64 = DAY_MS;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Obs {
    pub key: String,
    pub id: String,
    pub at_ms: i64,
    pub text: String,
    pub workspace_id: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Row {
    last_gather_ms: i64,
    last_event_ms: i64,
    obs: Vec<Obs>,
    dropped: BTreeMap<String, i64>,
}

fn read(db: &Db) -> Row {
    db.get_setting(ROW).ok().flatten().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn write(db: &Db, row: &Row) -> Result<(), String> {
    db.set_setting_quiet(ROW, &serde_json::to_string(row).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// One thing the daemon saw: where (`id`), when, what it said and which workspace it belongs to.
#[derive(Debug, Clone, Default)]
pub struct Seen {
    pub id: String,
    pub at_ms: i64,
    pub text: String,
    pub workspace_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct Inputs {
    /// Failed runs' errors and results, chat turns' text and errors (for `missing_tool`).
    pub failures: Vec<Seen>,
    /// What memory learning and ticket lessons recorded as a correction (for `correction`).
    pub corrections: Vec<Seen>,
    /// Inbox items, one per time raised (for `ask`); `text` is the title.
    pub asks: Vec<Seen>,
}

pub fn gather_due(db: &Db, now: i64) -> bool {
    enabled(db) && now - read(db).last_gather_ms >= GATHER_EVERY_MS
}

pub fn event_due(db: &Db, now: i64) -> bool {
    enabled(db) && now - read(db).last_event_ms >= EVENT_EVERY_MS
}

pub fn note_gather(db: &Db, now: i64) -> Result<(), String> {
    let mut row = read(db);
    row.last_gather_ms = now;
    row.last_event_ms = now;
    write(db, &row)
}

pub fn note_event(db: &Db, now: i64) -> Result<(), String> {
    let mut row = read(db);
    row.last_event_ms = now;
    write(db, &row)
}

// ---------------------------------------------------------------------------
// Missing tools
// ---------------------------------------------------------------------------

const FILLER: &[&str] = &["is", "are", "was", "were", "be", "been", "the", "a", "an", "tool", "not", "server", "mcp", "command", "found", "error", "and", "or", "of", "to", "your", "this", "that", "it", "no", "such", "unknown", "failed", "available", "connected", "installed", "ran", "run", "is:", "error:"];

/// A token as a tool name: trimmed of quotes and punctuation, lower case, plain characters only.
fn name_of(token: &str) -> Option<String> {
    let name = token.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '/' || c == '@')).to_ascii_lowercase();
    let ok = !name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | '@' | '+'));
    (ok && !FILLER.contains(&name.as_str())).then_some(name)
}

fn after(line: &str, at: usize) -> Option<String> {
    line[at..].trim_start_matches(|c: char| c == ':' || c == '-' || c.is_whitespace()).split_whitespace().next().and_then(name_of)
}

fn before(line: &str, at: usize) -> Option<String> {
    line[..at].split_whitespace().rev().find_map(|t| {
        let bare = t.trim_matches(|c: char| !c.is_ascii_alphanumeric()).to_ascii_lowercase();
        if bare.is_empty() || FILLER.contains(&bare.as_str()) {
            None
        } else {
            name_of(t)
        }
    })
}

/// The tools or commands a text says are missing, with the line that said it. Matches (case
/// insensitive): "tool not found", "no such tool", "unknown tool", "MCP server … (not available |
/// not connected | failed)", "command not found: <name>", "not installed".
pub fn missing_tools(text: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for line in text.lines() {
        // ASCII lower-casing keeps every byte offset the same as in `line`.
        let lower = line.to_ascii_lowercase();
        let mut names = Vec::new();
        for needle in ["command not found:"] {
            for (at, _) in lower.match_indices(needle) {
                names.extend(after(line, at + needle.len()));
            }
        }
        for needle in ["tool not found", "no such tool", "unknown tool"] {
            for (at, _) in lower.match_indices(needle) {
                // "no such tool: foo" and "foo: tool not found" both name it.
                let named = if needle == "tool not found" { before(line, at).or_else(|| after(line, at + needle.len())) } else { after(line, at + needle.len()) };
                names.extend(named);
            }
        }
        if let Some(at) = lower.find("mcp server") {
            let rest = &lower[at + "mcp server".len()..];
            if ["not available", "not connected", "failed"].iter().any(|p| rest.contains(p)) {
                names.extend(after(line, at + "mcp server".len()));
            }
        }
        for (at, _) in lower.match_indices("not installed") {
            names.extend(before(line, at));
        }
        let excerpt: String = line.trim().chars().take(EXCERPT).collect();
        for name in names {
            if !found.iter().any(|(n, _): &(String, String)| *n == name) {
                found.push((name, excerpt.clone()));
            }
        }
    }
    found
}

// ---------------------------------------------------------------------------
// Correction topics
// ---------------------------------------------------------------------------

const STOP: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "use", "using", "used", "always", "never", "please", "should", "would", "could", "instead", "dont", "not", "you", "your", "when", "from", "into", "have", "has", "had", "are", "was", "were", "its", "can", "will", "make", "sure", "each", "any", "all", "but", "than", "then", "them", "they", "what", "which", "who", "how", "why", "does", "did", "too", "also", "only", "just", "more", "most", "very", "about", "over", "out", "off", "his", "her", "our", "neko", "user", "prefers", "prefer", "like", "want", "wants", "ask", "asks", "before", "after", "first", "every",
];

/// The content words of a correction, lower case, each once.
pub fn topic_words(text: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    text.split(|c: char| !c.is_alphanumeric()).map(str::to_lowercase).filter(|w| w.chars().count() >= 3 && !STOP.contains(&w.as_str())).filter(|w| seen.insert(w.clone())).collect()
}

/// A topic key from the words: the four longest, in alphabetical order.
fn topic_key(words: &[String]) -> String {
    let mut by_length: Vec<&String> = words.iter().collect();
    by_length.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()).then(a.cmp(b)));
    let mut four: Vec<&str> = by_length.into_iter().take(4).map(String::as_str).collect();
    four.sort_unstable();
    four.join("-")
}

/// The existing topic these words belong to (most of the topic's words are in them), or a new one.
fn assign_topic(existing: &BTreeSet<String>, words: &[String]) -> Option<String> {
    if words.is_empty() {
        return None;
    }
    let mine: BTreeSet<&str> = words.iter().map(String::as_str).collect();
    let best = existing
        .iter()
        .filter_map(|topic| {
            let theirs: Vec<&str> = topic.split('-').collect();
            let shared = theirs.iter().filter(|w| mine.contains(*w)).count();
            (shared >= 2.min(theirs.len()) && shared * 5 >= theirs.len() * 3).then_some((shared, topic))
        })
        .max_by_key(|(shared, topic)| (*shared, std::cmp::Reverse((*topic).clone())));
    Some(best.map_or_else(|| topic_key(words), |(_, topic)| topic.clone()))
}

// ---------------------------------------------------------------------------
// Recording
// ---------------------------------------------------------------------------

fn push(row: &mut Row, key: String, id: String, at_ms: i64, text: &str, workspace_id: &str) -> bool {
    if row.obs.iter().any(|o| o.key == key && o.id == id) {
        return false;
    }
    row.obs.push(Obs { key, id, at_ms, text: text.chars().take(EXCERPT).collect(), workspace_id: workspace_id.to_owned() });
    true
}

/// Notes what the daemon saw. Returns how many observations are new. Nothing is read or written
/// while "Suggestions" is off.
pub fn record(db: &Db, inputs: &Inputs, now: i64) -> Result<usize, String> {
    if !enabled(db) {
        return Ok(0);
    }
    let mut row = read(db);
    let mut added = 0;
    for seen in &inputs.failures {
        for (name, excerpt) in missing_tools(&seen.text) {
            added += usize::from(push(&mut row, format!("missing_tool:{name}"), format!("{}:{name}", seen.id), seen.at_ms, &excerpt, &seen.workspace_id));
        }
    }
    let mut topics: BTreeSet<String> = row.obs.iter().filter_map(|o| o.key.strip_prefix("correction:")).map(str::to_owned).collect();
    for seen in &inputs.corrections {
        let Some(topic) = assign_topic(&topics, &topic_words(&seen.text)) else { continue };
        topics.insert(topic.clone());
        added += usize::from(push(&mut row, format!("correction:{topic}"), seen.id.clone(), seen.at_ms, &seen.text, &seen.workspace_id));
    }
    for seen in &inputs.asks {
        let key = crate::inbox::title_key(&seen.text);
        if key.is_empty() {
            continue;
        }
        added += usize::from(push(&mut row, format!("ask:{key}"), seen.id.clone(), seen.at_ms, &seen.text, &seen.workspace_id));
    }
    prune(&mut row, now);
    write(db, &row)?;
    Ok(added)
}

/// An Inbox item reported again (an open one refreshed): one more time it was raised.
pub fn note_ask(db: &Db, title: &str, seq: u64, workspace_id: &str, now: i64) {
    let _ = record(db, &Inputs { asks: vec![Seen { id: format!("{seq}:{now}"), at_ms: now, text: title.to_owned(), workspace_id: workspace_id.to_owned() }], ..Default::default() }, now);
}

fn prune(row: &mut Row, now: i64) {
    row.obs.retain(|o| now - o.at_ms < KEEP_DAYS * DAY_MS);
    let extra = row.obs.len().saturating_sub(MAX_OBS);
    if extra > 0 {
        row.obs.sort_by_key(|o| o.at_ms);
        row.obs.drain(..extra);
    }
    row.dropped.retain(|_, at| now - *at < 7 * DAY_MS);
}

/// How many times the key was seen in `[from, to]`.
pub fn count_since(db: &Db, key: &str, from: i64, to: i64) -> u32 {
    read(db).obs.iter().filter(|o| o.key == key && o.at_ms >= from && o.at_ms <= to).count() as u32
}

/// A key whose proposal was dropped waits a day before it is asked about again.
pub fn mark_dropped(db: &Db, key: &str, now: i64) -> Result<(), String> {
    let mut row = read(db);
    row.dropped.insert(key.to_owned(), now);
    write(db, &row)
}

// ---------------------------------------------------------------------------
// Which signals are due
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signal {
    pub key: String,
    /// One sentence for the model.
    pub label: String,
    pub count: usize,
    /// At most 4 KB (`MAX_EVIDENCE`), spilled to a file when longer.
    pub evidence: String,
    pub workspace_id: String,
}

fn date(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms).map_or_else(String::new, |t| t.format("%Y-%m-%d").to_string())
}

/// The evidence lines, newest first; text over 4 KB is cut to its head and tail and the whole is
/// spilled to a file (the same rule as a goal check's output).
fn evidence_of(observations: &[&Obs]) -> String {
    let mut sorted: Vec<&&Obs> = observations.iter().collect();
    sorted.sort_by_key(|o| std::cmp::Reverse(o.at_ms));
    let text = sorted.iter().map(|o| format!("- [{}] {}", date(o.at_ms), o.text.split_whitespace().collect::<Vec<_>>().join(" "))).collect::<Vec<_>>().join("\n");
    crate::spill::bounded_with(&crate::spill::dir(), "improve-evidence", &text, super::MAX_EVIDENCE, 2048, 1536)
}

/// The signals that have fired and may become a proposal now: past their threshold, with no open
/// proposal for the key, not rejected (or otherwise settled) within 30 days, not dropped lately,
/// at most as many as today's allowance of 3 leaves.
pub fn due(db: &Db, now: i64) -> Vec<Signal> {
    if !enabled(db) {
        return Vec::new();
    }
    let proposals = super::load(db);
    let today = proposals.iter().filter(|p| now - p.created_ms < DAY_MS).count();
    let room = PER_DAY.saturating_sub(today);
    if room == 0 {
        return Vec::new();
    }
    let row = read(db);
    let blocked = |key: &str| {
        proposals.iter().any(|p| {
            p.signal_key == key
                && match p.state {
                    ImproveState::Open | ImproveState::Sent | ImproveState::Applied => true,
                    ImproveState::Helped | ImproveState::DidntHelp | ImproveState::Rejected => p.resolved_ms.is_some_and(|t| now - t < REJECT_MEMORY_DAYS * DAY_MS),
                }
        }) || row.dropped.get(key).is_some_and(|t| now - t < DROPPED_BACKOFF_MS)
    };
    let mut by_key: BTreeMap<&str, Vec<&Obs>> = BTreeMap::new();
    for o in &row.obs {
        by_key.entry(&o.key).or_default().push(o);
    }
    let mut out = Vec::new();
    for (key, all) in by_key {
        let (times, days) = if key.starts_with("missing_tool:") {
            (1, MISSING_DAYS)
        } else if key.starts_with("correction:") {
            (CORRECTION_TIMES, CORRECTION_DAYS)
        } else if key.starts_with("ask:") {
            (ASK_TIMES, ASK_DAYS)
        } else {
            continue;
        };
        let recent: Vec<&Obs> = all.into_iter().filter(|o| now - o.at_ms < days * DAY_MS).collect();
        if recent.len() < times || blocked(key) {
            continue;
        }
        let workspace_id = recent.iter().max_by_key(|o| o.at_ms).map(|o| o.workspace_id.clone()).unwrap_or_default();
        let label = match key.split_once(':') {
            Some(("missing_tool", name)) => format!("A run or turn said the tool or command \"{name}\" is missing, not connected or not installed."),
            Some(("correction", topic)) => format!("The person corrected Neko about the same thing {} times in {CORRECTION_DAYS} days (topic words: {topic}).", recent.len()),
            Some((_, _)) => format!("The same question reached the Inbox {} times in {ASK_DAYS} days: \"{}\".", recent.len(), recent.first().map_or("", |o| o.text.as_str())),
            None => continue,
        };
        out.push(Signal { key: key.to_owned(), label, count: recent.len(), evidence: evidence_of(&recent), workspace_id });
    }
    out.sort_by(|a, b| b.count.cmp(&a.count).then(a.key.cmp(&b.key)));
    out.truncate(room);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(id: &str, at_ms: i64, text: &str) -> Seen {
        Seen { id: id.into(), at_ms, text: text.into(), workspace_id: "w1".into() }
    }

    #[test]
    fn missing_tools_are_named_from_every_phrase() {
        let names = |text: &str| missing_tools(text).into_iter().map(|(n, _)| n).collect::<Vec<_>>();
        assert_eq!(names("zsh: command not found: rg"), ["rg"]);
        assert_eq!(names("Error: Unknown tool: linear_search"), ["linear_search"]);
        assert_eq!(names("no such tool: browser_click"), ["browser_click"]);
        assert_eq!(names("browser_click: Tool not found"), ["browser_click"]);
        assert_eq!(names("The MCP server linear failed to start"), ["linear"]);
        assert_eq!(names("MCP server \"slack\" is not connected"), ["slack"]);
        assert_eq!(names("ripgrep is not installed"), ["ripgrep"]);
        assert_eq!(names("COMMAND NOT FOUND: Jq"), ["jq"]);
        assert!(names("everything worked, the tests passed").is_empty());
        assert!(names("MCP server started fine").is_empty());
        assert!(names("it is not installed").is_empty(), "no name to capture");
        assert_eq!(missing_tools("a\nzsh: command not found: rg\nb").first().unwrap().1, "zsh: command not found: rg");
    }

    #[test]
    fn corrections_group_by_topic_words() {
        let a = topic_words("Always use pnpm, never npm, for installs");
        let b = topic_words("Use pnpm instead of npm when installing");
        let mut topics = BTreeSet::new();
        let first = assign_topic(&topics, &a).unwrap();
        topics.insert(first.clone());
        assert_eq!(assign_topic(&topics, &b).as_deref(), Some(first.as_str()), "{a:?} / {b:?}");
        let other = topic_words("Reply to reviewers in a single comment");
        assert_ne!(assign_topic(&topics, &other).unwrap(), first);
        assert_eq!(assign_topic(&topics, &[]), None);
    }

    #[test]
    fn each_signal_type_is_detected_at_its_threshold() {
        let db = Db::open_in_memory().unwrap();
        let now = 100 * DAY_MS;
        let inputs = Inputs {
            failures: vec![seen("t1", now - DAY_MS, "run failed\nzsh: command not found: rg")],
            corrections: vec![
                seen("m1", now - 2 * DAY_MS, "Always use pnpm, never npm"),
                seen("m2", now - 5 * DAY_MS, "Use pnpm instead of npm"),
                seen("m3", now - 9 * DAY_MS, "Do not use npm, use pnpm"),
            ],
            asks: vec![seen("1", now - DAY_MS, "Which database should the report use?"), seen("2", now - 2 * DAY_MS, "which database should the report use"), seen("3", now - 3 * DAY_MS, "Which database should the report use?")],
        };
        assert_eq!(record(&db, &inputs, now).unwrap(), 7);
        assert_eq!(record(&db, &inputs, now).unwrap(), 0, "the same things are not counted twice");
        let keys: Vec<String> = due(&db, now).into_iter().map(|s| s.key).collect();
        assert_eq!(keys.len(), 3, "{keys:?}");
        assert!(keys.contains(&"missing_tool:rg".to_owned()));
        assert!(keys.iter().any(|k| k.starts_with("correction:") && k.contains("pnpm")));
        assert!(keys.iter().any(|k| k.starts_with("ask:") && k.contains("database")));
    }

    #[test]
    fn two_corrections_or_two_asks_or_old_ones_are_not_signals() {
        let db = Db::open_in_memory().unwrap();
        let now = 100 * DAY_MS;
        let inputs = Inputs {
            corrections: vec![seen("m1", now - DAY_MS, "Always use pnpm, never npm"), seen("m2", now - 2 * DAY_MS, "Use pnpm instead of npm"), seen("m3", now - 20 * DAY_MS, "Do not use npm, use pnpm")],
            asks: vec![seen("1", now - DAY_MS, "Which database?"), seen("2", now - 2 * DAY_MS, "Which database?"), seen("3", now - 9 * DAY_MS, "Which database?")],
            failures: vec![seen("t1", now - 9 * DAY_MS, "command not found: rg")],
        };
        record(&db, &inputs, now).unwrap();
        assert!(due(&db, now).is_empty());
    }

    #[test]
    fn limits_three_a_day_one_per_key_and_thirty_days_after_a_rejection() {
        use neko_protocol::improve::Improvement;
        let db = Db::open_in_memory().unwrap();
        let now = 100 * DAY_MS;
        let failures: Vec<Seen> = ["a", "b", "c", "d"].iter().map(|n| seen(n, now - 1000, &format!("command not found: tool-{n}"))).collect();
        record(&db, &Inputs { failures, ..Default::default() }, now).unwrap();
        assert_eq!(due(&db, now).len(), 3, "at most 3 proposals a day");
        // One open proposal per key: its key is not due again.
        let open = Improvement { id: "p1".into(), signal_key: "missing_tool:tool-a".into(), created_ms: now - 2 * DAY_MS, ..Default::default() };
        super::super::save(&db, &[open.clone()]).unwrap();
        let keys: Vec<String> = due(&db, now).into_iter().map(|s| s.key).collect();
        assert_eq!(keys.len(), 3);
        assert!(!keys.contains(&"missing_tool:tool-a".to_owned()), "{keys:?}");
        // A proposal made in the last day uses up the allowance.
        let fresh: Vec<Improvement> = (0..3).map(|n| Improvement { id: format!("n{n}"), signal_key: format!("other:{n}"), created_ms: now - 1000, ..Default::default() }).collect();
        super::super::save(&db, &fresh).unwrap();
        assert!(due(&db, now).is_empty());
        // Rejected: quiet for 30 days, then due again.
        let rejected = Improvement { id: "p2".into(), signal_key: "missing_tool:tool-a".into(), state: ImproveState::Rejected, created_ms: now - 40 * DAY_MS, resolved_ms: Some(now - 10 * DAY_MS), ..Default::default() };
        super::super::save(&db, &[rejected.clone()]).unwrap();
        assert!(!due(&db, now).iter().any(|s| s.key == "missing_tool:tool-a"));
        record(&db, &Inputs { failures: vec![seen("e", now + 25 * DAY_MS - 1000, "command not found: tool-a")], ..Default::default() }, now + 25 * DAY_MS).unwrap();
        assert!(due(&db, now + 25 * DAY_MS).iter().any(|s| s.key == "missing_tool:tool-a"), "30 days after the rejection it may be suggested again");
        // A dropped key waits a day.
        super::super::save(&db, &[]).unwrap();
        mark_dropped(&db, "missing_tool:tool-b", now).unwrap();
        assert!(!due(&db, now + 1000).iter().any(|s| s.key == "missing_tool:tool-b"));
        assert!(due(&db, now + DAY_MS + 1000).iter().any(|s| s.key == "missing_tool:tool-b") || count_since(&db, "missing_tool:tool-b", 0, now + 2 * DAY_MS) == 1);
    }

    #[test]
    fn nothing_is_gathered_or_written_while_suggestions_are_off() {
        let db = Db::open_in_memory().unwrap();
        super::super::set_enabled(&db, false).unwrap();
        let now = 100 * DAY_MS;
        assert!(!gather_due(&db, now) && !event_due(&db, now));
        assert_eq!(record(&db, &Inputs { failures: vec![seen("t", now, "command not found: rg")], ..Default::default() }, now).unwrap(), 0);
        assert!(read(&db).obs.is_empty());
        assert!(due(&db, now).is_empty());
        super::super::set_enabled(&db, true).unwrap();
        assert!(gather_due(&db, now));
        note_gather(&db, now).unwrap();
        assert!(!gather_due(&db, now + GATHER_EVERY_MS - 1) && gather_due(&db, now + GATHER_EVERY_MS));
        assert!(!event_due(&db, now + EVENT_EVERY_MS - 1) && event_due(&db, now + EVENT_EVERY_MS));
    }

    #[test]
    fn evidence_over_four_kilobytes_is_cut_and_spilled() {
        let db = Db::open_in_memory().unwrap();
        let now = 100 * DAY_MS;
        let long = "x".repeat(290);
        let failures: Vec<Seen> = (0..40).map(|n| seen(&format!("t{n}"), now - n, &format!("command not found: rg {long}"))).collect();
        record(&db, &Inputs { failures, ..Default::default() }, now).unwrap();
        let signal = due(&db, now).into_iter().find(|s| s.key == "missing_tool:rg").unwrap();
        assert!(signal.evidence.len() < 4 * 1024 + 300, "{}", signal.evidence.len());
        assert!(signal.evidence.contains("left out"));
    }
}
