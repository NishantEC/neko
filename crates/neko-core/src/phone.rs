//! Phone push: when a needs-you item is worth a push, and what each channel
//! gets. Pure decisions and small rows; the daemon (`neko-daemon::phone`)
//! reads items, asks the Mac how long it has been idle and does the
//! sending. Nothing here touches the network except `send_ntfy`. See
//! docs/knowledge/phone-push.md.
//!
//! Rows (outside the workbench snapshot): `phone_settings_v1` (what the
//! person chose) and `phone_state_v1` (quiet writes: cursor, held items,
//! reminders in flight, the send log).
use crate::Db;
use neko_protocol::phone::{PhoneSettings, PhoneStatus, ReminderFinish, ReminderNew, ReminderPlan, RemindersAccess, DEFAULT_NTFY_SERVER};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::Duration;

pub const SETTINGS: &str = "phone_settings_v1";
pub const STATE: &str = "phone_state_v1";
/// The person is "at the Mac" when the keyboard or mouse was used this
/// recently; the macOS notification is enough then.
pub const AT_MAC_SECS: u64 = 180;
/// Pushes per rolling hour, then one summary push.
pub const MAX_PER_HOUR: usize = 6;
pub const HOUR_MS: i64 = 3_600_000;
/// A held item older than this is not worth a push any more.
const HELD_MAX_MS: i64 = 24 * HOUR_MS;
/// A reminder the app has not created within an hour is dropped (a stale
/// buzz is worse than none), and so is the summary reminder after 6 hours.
const PENDING_MAX_MS: i64 = HOUR_MS;
const SUMMARY_REMINDER_MS: i64 = 6 * HOUR_MS;
/// An answered reminder stays (completed, with its "Got it" echo) this long.
const ANSWERED_KEEP_MS: i64 = 24 * HOUR_MS;
/// The morning digest's reminder stands in for the digest this long.
const DIGEST_REMINDER_MS: i64 = 24 * HOUR_MS;
/// The `seq` the digest's reminder is mapped under (no item has one this big).
pub const DIGEST_SEQ: u64 = 4_000_000_000_000;
const MAX_TITLE: usize = 120;
const MAX_DETAIL: usize = 300;
const MAX_LOG: usize = 64;
const NTFY_TIMEOUT: Duration = Duration::from_secs(10);

/// A needs-you item that may deserve a push.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Candidate {
    pub seq: u64,
    pub title: String,
    pub detail: String,
    pub link: Option<String>,
    pub urgent: bool,
    pub since_ms: i64,
    /// What completing the reminder does ("extend NK-14 by 10 turns"); empty
    /// when the item has no default.
    pub action: String,
}

/// A reminder waiting for the app to create it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pending {
    pub reminder: ReminderNew,
    pub queued_ms: i64,
}

/// An item and the reminder that stands for it (`seq` 0: a summary).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mapped {
    pub seq: u64,
    pub id: String,
    pub created_ms: i64,
    /// The person answered in words and the app was told to echo and complete
    /// it; the reminder is kept (completed) until `ANSWERED_KEEP_MS` passes.
    pub answered_ms: i64,
    /// A hash of the last reply text looked at (used or rejected), so the
    /// same text is never judged twice.
    pub text_hash: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// Items with a larger `seq` are new. `None` until first looked at, so
    /// turning a channel on never pushes history.
    pub cursor: Option<u64>,
    pub held: Vec<Candidate>,
    pub pending: Vec<Pending>,
    pub mapped: Vec<Mapped>,
    /// Echoes the app has not yet written and completed.
    pub finish: Vec<ReminderFinish>,
    /// When each push of the last hour went out.
    pub sent_ms: Vec<i64>,
    pub summary_ms: i64,
    pub access: RemindersAccess,
    pub last_push_ms: Option<i64>,
    pub last_channel: String,
    pub last_error: Option<String>,
}

/// One push to send on every channel that is on. `seq` 0 is a summary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Push {
    pub seq: u64,
    pub title: String,
    pub detail: String,
    pub link: Option<String>,
    pub urgent: bool,
    /// What completing the reminder does; empty for a question with no
    /// default, a summary, a digest and a test.
    pub action: String,
    /// An escalation that has no default ("No default" is said in its notes).
    pub no_default: bool,
}

impl From<&Candidate> for Push {
    fn from(c: &Candidate) -> Self {
        Push { seq: c.seq, title: c.title.clone(), detail: c.detail.clone(), link: c.link.clone(), urgent: c.urgent, action: c.action.clone(), no_default: c.seq != 0 && c.action.is_empty() }
    }
}

pub fn any_channel(settings: &PhoneSettings) -> bool {
    settings.reminders || settings.ntfy
}

fn read<T: for<'de> Deserialize<'de> + Default>(db: &Db, key: &str) -> T {
    // A row that does not decode starts over; a push is never worth a crash.
    db.get_setting(key).ok().flatten().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

pub fn load_settings(db: &Db) -> PhoneSettings {
    read(db, SETTINGS)
}

pub fn load_state(db: &Db) -> State {
    read(db, STATE)
}

pub fn save_state(db: &Db, state: &State) -> Result<(), String> {
    let text = serde_json::to_string(state).map_err(|e| e.to_string())?;
    db.set_setting_quiet(STATE, &text).map_err(|e| e.to_string())
}

/// `https://host[:port][/path]` without a trailing slash, or why not.
pub fn clean_server(server: &str) -> Result<String, String> {
    let server = server.trim().trim_end_matches('/');
    if server.is_empty() {
        return Ok(DEFAULT_NTFY_SERVER.into());
    }
    let lower = server.to_ascii_lowercase();
    let rest = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://")).ok_or("The server address must start with https:// (or http:// for your own server)")?;
    if rest.is_empty() || server.contains(char::is_whitespace) || server.len() > 200 {
        return Err("That is not a server address".into());
    }
    Ok(server.to_string())
}

/// The address the "Open in Neko" button opens: `http(s)://...` or empty. Never
/// guessed: without one the button is not offered.
pub fn clean_open_url(url: &str) -> Result<String, String> {
    let url = url.trim().trim_end_matches('/');
    if url.is_empty() {
        return Ok(String::new());
    }
    let lower = url.to_ascii_lowercase();
    let rest = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://")).ok_or("The address must start with https:// or http://")?;
    if rest.is_empty() || url.contains(char::is_whitespace) || url.len() > 200 {
        return Err("That is not an address".into());
    }
    Ok(url.to_string())
}

/// 29 characters, `neko-` and 24 random lowercase letters and digits.
pub fn new_topic() -> String {
    let mut bytes = [0u8; 24];
    let filled = std::fs::File::open("/dev/urandom").and_then(|mut f| std::io::Read::read_exact(&mut f, &mut bytes)).is_ok();
    if !filled {
        // No random device: mix the clock and the process, still unguessable
        // enough for a topic nobody has been told.
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0) ^ (u128::from(std::process::id()) << 64);
        for (n, b) in bytes.iter_mut().enumerate() {
            *b = (seed.rotate_left(n as u32 * 5) >> (n % 8)) as u8 ^ (n as u8).wrapping_mul(37);
        }
    }
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let tail: String = bytes.iter().map(|b| ALPHABET[usize::from(*b) % ALPHABET.len()] as char).collect();
    format!("neko-{tail}")
}

fn valid_topic(topic: &str) -> bool {
    topic.len() >= 24 && topic.len() <= 64 && topic.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Checks and normalises what the person sent. A missing or unusable topic is
/// replaced with a fresh one (this is also how "new topic" works).
pub fn clean_settings(mut settings: PhoneSettings) -> Result<PhoneSettings, String> {
    settings.ntfy_server = clean_server(&settings.ntfy_server)?;
    settings.ntfy_topic = settings.ntfy_topic.trim().to_string();
    settings.open_url = clean_open_url(&settings.open_url)?;
    if !valid_topic(&settings.ntfy_topic) {
        settings.ntfy_topic = new_topic();
    }
    if settings.quiet_from_hour > 23 || settings.quiet_to_hour > 23 {
        return Err("Hours are 0 to 23".into());
    }
    if settings.digest_hour > 23 || settings.digest_minute > 59 {
        return Err("The digest time is 00:00 to 23:59".into());
    }
    Ok(settings)
}

/// Settings as stored, with a topic made on first read so Settings can show
/// the subscribe link before the switch is turned on.
pub fn settings_with_topic(db: &Db) -> PhoneSettings {
    let mut settings = load_settings(db);
    if !valid_topic(&settings.ntfy_topic) {
        settings.ntfy_topic = new_topic();
        if let Ok(text) = serde_json::to_string(&settings) {
            let _ = db.set_setting_quiet(SETTINGS, &text);
        }
    }
    settings
}

pub fn save_settings(db: &Db, settings: PhoneSettings) -> Result<PhoneSettings, String> {
    let settings = clean_settings(settings)?;
    let text = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
    db.set_setting_quiet(SETTINGS, &text).map_err(|e| e.to_string())?;
    Ok(settings)
}

/// `ntfy://host[/path]/topic`: opens the ntfy app subscribed to the topic.
pub fn subscribe_link(settings: &PhoneSettings) -> String {
    if settings.ntfy_topic.is_empty() {
        return String::new();
    }
    let server = settings.ntfy_server.trim_end_matches('/');
    let host = server.split_once("://").map_or(server, |(_, rest)| rest);
    format!("ntfy://{host}/{}", settings.ntfy_topic)
}

pub fn status(settings: &PhoneSettings, state: &State) -> PhoneStatus {
    PhoneStatus {
        reminders_access: state.access,
        last_push_ms: state.last_push_ms,
        last_channel: state.last_channel.clone(),
        last_error: state.last_error.clone(),
        held: state.held.len() as u32,
        subscribe_link: subscribe_link(settings),
    }
}

/// Whether `hour` (0-23, local) is inside quiet hours. Equal hours mean none.
pub fn quiet_at(settings: &PhoneSettings, hour: u32) -> bool {
    let (from, to) = (settings.quiet_from_hour, settings.quiet_to_hour);
    if !settings.quiet_enabled || from == to {
        return false;
    }
    if from < to { hour >= from && hour < to } else { hour >= from || hour < to }
}

/// `HIDIdleTime` (nanoseconds) from `ioreg -c IOHIDSystem` output, in seconds.
pub fn parse_hid_idle(output: &str) -> Option<u64> {
    let line = output.lines().find(|l| l.contains("\"HIDIdleTime\""))?;
    let digits: String = line.split('=').nth(1)?.trim().chars().take_while(char::is_ascii_digit).collect();
    digits.parse::<u64>().ok().map(|ns| ns / 1_000_000_000)
}

/// The hour (0-23) on the Mac's clock.
pub fn local_hour() -> u32 {
    use chrono::Timelike;
    chrono::Local::now().hour()
}

/// What the world looks like for one decision.
pub struct Env {
    pub now_ms: i64,
    /// Seconds since the last keyboard or mouse input; `None` if unknown
    /// (treated as away: reaching the person is the point).
    pub idle_secs: Option<u64>,
    pub local_hour: u32,
}

/// Takes new items, decides which go out now, and updates `state`: held
/// items stay for quiet hours, items for a person sitting at the Mac are
/// dropped, the rest are capped at `MAX_PER_HOUR` with one summary. `open`
/// holds the seqs still open: a held item that closed is forgotten.
pub fn decide(settings: &PhoneSettings, state: &mut State, new: Vec<Candidate>, open: &HashSet<u64>, env: &Env) -> Vec<Push> {
    state.held.extend(new);
    state.held.retain(|c| open.contains(&c.seq) && env.now_ms - c.since_ms < HELD_MAX_MS);
    state.sent_ms.retain(|t| env.now_ms - t < HOUR_MS);
    if !any_channel(settings) {
        state.held.clear();
        return Vec::new();
    }
    if state.held.is_empty() {
        return Vec::new();
    }
    if env.idle_secs.is_some_and(|idle| idle < AT_MAC_SECS) {
        // The macOS notification reached them; what waits for quiet hours
        // (non-urgent items during them) was not seen and stays held.
        let quiet = quiet_at(settings, env.local_hour);
        state.held.retain(|c| quiet && !c.urgent);
        return Vec::new();
    }
    let quiet = quiet_at(settings, env.local_hour);
    let mut ready: Vec<Candidate> = Vec::new();
    let mut waiting: Vec<Candidate> = Vec::new();
    for c in std::mem::take(&mut state.held) {
        if quiet && !c.urgent { waiting.push(c) } else { ready.push(c) }
    }
    state.held = waiting;
    // Urgent first, then oldest first.
    ready.sort_by_key(|c| (!c.urgent, c.since_ms, c.seq));
    let slots = MAX_PER_HOUR.saturating_sub(state.sent_ms.len());
    let excess = ready.split_off(slots.min(ready.len()));
    let mut pushes: Vec<Push> = ready.iter().map(Push::from).collect();
    if !excess.is_empty() && (state.summary_ms == 0 || env.now_ms - state.summary_ms >= HOUR_MS) {
        state.summary_ms = env.now_ms;
        let n = excess.len();
        pushes.push(Push { seq: 0, title: format!("{n} more thing{} need{} you", if n == 1 { "" } else { "s" }, if n == 1 { "s" } else { "" }), detail: "Open Neko's Inbox.".into(), link: None, urgent: false, action: String::new(), no_default: false });
    }
    // Excess beyond the summary stays in the Inbox; nothing is retried.
    state.sent_ms.extend(pushes.iter().map(|_| env.now_ms));
    pushes
}

fn clip(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max { text.to_string() } else { format!("{}…", text.chars().take(max - 1).collect::<String>()) }
}

/// The marker line that ties a reminder to its item, so a reminder made
/// before a crash is found again instead of doubled.
pub fn marker(seq: u64) -> String {
    format!("neko:{seq}")
}

/// A line that says what the two ways to answer do. Lines of the item's own
/// text never start with `>` (see `without_reply_marks`), so every `>` line
/// found later is the person's.
fn how_to_answer(push: &Push) -> Option<String> {
    if push.seq == 0 || push.seq == DIGEST_SEQ {
        return None;
    }
    Some(if push.action.is_empty() {
        "No default. Completing this only marks it seen. To answer, add a line that starts with > and your answer.".to_owned()
    } else {
        format!("Completing this = {}. To answer in words instead, add a line that starts with > and your answer.", push.action)
    })
}

pub fn reminder_for(push: &Push) -> ReminderNew {
    let mut notes = crate::inbox::without_reply_marks(&clip(&push.detail, 600));
    if let Some(link) = &push.link {
        if !notes.is_empty() {
            notes.push_str("\n\n");
        }
        notes.push_str(link);
    }
    if let Some(how) = how_to_answer(push) {
        if !notes.is_empty() {
            notes.push_str("\n\n");
        }
        notes.push_str(&how);
    }
    if push.seq != 0 {
        notes.push_str(&format!("\n\n{}", marker(push.seq)));
    }
    let title = if push.action.is_empty() { clip(&push.title, MAX_TITLE) } else { clip(&format!("{} ✓ = {}", push.title, push.action), MAX_TITLE) };
    // A link that is not on the web (a goal's `goal:<id>`, a task's) is not a reminder URL.
    let url = push.link.clone().filter(|l| l.starts_with("http"));
    ReminderNew { seq: push.seq, title, notes, url, key: crate::workbench::new_id() }
}

/// A stable hash of reply text.
pub fn hash_text(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish().max(1)
}

/// The JSON a publish to the ntfy server carries. With `titles_only` the
/// server never sees the detail.
pub fn ntfy_body(settings: &PhoneSettings, push: &Push) -> Value {
    let message = if settings.titles_only || push.detail.trim().is_empty() { "Open Neko on your Mac.".to_string() } else { clip(&push.detail, MAX_DETAIL) };
    // No click URL: tapping opens the ntfy app, not a private page.
    let mut body = json!({
        "topic": settings.ntfy_topic,
        "title": clip(&push.title, MAX_TITLE),
        "message": message,
        "priority": if push.urgent { 4 } else { 3 },
        "tags": ["cat"],
    });
    // The one button that works without exposing the Mac: open the address the
    // person gave. Accept and decline are not offered: they would need an
    // endpoint the phone can reach, and Neko does not open one. Replies go
    // through Reminders.
    if !settings.open_url.is_empty() {
        body["actions"] = json!([{ "action": "view", "label": "Open in Neko", "url": format!("{}/#/inbox", settings.open_url), "clear": true }]);
    }
    body
}

/// Sends one publish; a plain failure message, no retry here.
pub fn send_ntfy(server: &str, body: &Value) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder().timeout(NTFY_TIMEOUT).build().map_err(|e| e.to_string())?;
    let response = client.post(server).json(body).send().map_err(|e| if e.is_timeout() { "The ntfy server did not answer in 10 seconds".to_string() } else { format!("Could not reach the ntfy server ({})", e.without_url()) })?;
    if response.status().is_success() { Ok(()) } else { Err(format!("The ntfy server answered {}", response.status().as_u16())) }
}

/// The app's report: which reminders were completed, and which of Neko's
/// are gone from the list. Returns the seqs whose reminders were completed
/// (to close), and drops mappings for completed or vanished reminders.
/// The completed ones are still returned in `delete` by `plan`.
pub fn absorb_sync(state: &mut State, access: RemindersAccess, present: &[String], completed: &[String]) -> Vec<u64> {
    state.access = access;
    if access != RemindersAccess::Granted {
        return Vec::new();
    }
    let present: HashSet<&String> = present.iter().collect();
    let done: HashSet<&String> = completed.iter().collect();
    let mut closed = Vec::new();
    state.mapped.retain(|m| {
        if done.contains(&m.id) {
            if m.seq != 0 && m.answered_ms == 0 {
                closed.push(m.seq);
            }
            // Kept one more round so `plan` deletes the finished reminder.
            return true;
        }
        // Deleted by the person: not done, so the item stays open.
        present.contains(&m.id)
    });
    closed
}

/// What the app should do now. `completed` reminders and those of closed
/// items (not in `open`), or of every item when the channel is off, are
/// deleted; pending reminders are created while access is granted.
pub fn plan(settings: &PhoneSettings, state: &mut State, open: &HashSet<u64>, completed: &[String], now_ms: i64) -> ReminderPlan {
    state.pending.retain(|p| now_ms - p.queued_ms < PENDING_MAX_MS);
    if !settings.reminders {
        state.pending.clear();
    }
    if state.access != RemindersAccess::Granted {
        return ReminderPlan { enabled: settings.reminders, ..Default::default() };
    }
    let done: HashSet<&String> = completed.iter().collect();
    let delete = state
        .mapped
        .iter()
        .filter(|m| {
            if !settings.reminders {
                return true;
            }
            // An answered reminder stays, completed, so the echo can be read.
            if m.answered_ms != 0 {
                return now_ms - m.answered_ms > ANSWERED_KEEP_MS;
            }
            done.contains(&m.id)
                || (m.seq == DIGEST_SEQ && now_ms - m.created_ms > DIGEST_REMINDER_MS)
                || (m.seq != 0 && m.seq != DIGEST_SEQ && !open.contains(&m.seq))
                || (m.seq == 0 && now_ms - m.created_ms > SUMMARY_REMINDER_MS)
        })
        .map(|m| m.id.clone())
        .collect();
    let create = if settings.reminders { state.pending.iter().map(|p| p.reminder.clone()).collect() } else { Vec::new() };
    let finish = if settings.reminders { state.finish.clone() } else { Vec::new() };
    ReminderPlan { enabled: settings.reminders, create, delete, finish }
}

/// The app did what the plan said.
pub fn absorb_done(state: &mut State, created: &[neko_protocol::phone::ReminderCreated], deleted: &[String], finished: &[String], now_ms: i64) {
    state.finish.retain(|f| !finished.contains(&f.id));
    for c in created {
        // A failed save reports an empty id: dropped, not retried every cycle.
        state.pending.retain(|p| p.reminder.key != c.key);
        if c.id.is_empty() {
            continue;
        }
        state.mapped.retain(|m| m.id != c.id);
        state.mapped.push(Mapped { seq: c.seq, id: c.id.clone(), created_ms: now_ms, ..Default::default() });
    }
    let gone: HashSet<&String> = deleted.iter().collect();
    state.mapped.retain(|m| !gone.contains(&m.id));
}

/// Remembers a push for the Settings status line, and the error once.
pub fn note_outcome(state: &mut State, channel: &str, result: &Result<(), String>, now_ms: i64) -> bool {
    match result {
        Ok(()) => {
            state.last_push_ms = Some(now_ms);
            state.last_channel = channel.into();
            state.last_error = None;
            false
        }
        Err(error) => {
            let fresh = state.last_error.as_deref() != Some(error.as_str());
            state.last_error = Some(error.clone());
            fresh
        }
    }
}

/// Keeps the send log bounded.
pub fn trim(state: &mut State) {
    if state.sent_ms.len() > MAX_LOG {
        let cut = state.sent_ms.len() - MAX_LOG;
        state.sent_ms.drain(..cut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(seq: u64, urgent: bool, since: i64) -> Candidate {
        Candidate { seq, title: format!("Item {seq}"), detail: "private detail".into(), link: Some("https://example.com/x".into()), urgent, since_ms: since, action: String::new() }
    }
    fn on() -> PhoneSettings {
        PhoneSettings { reminders: true, ..Default::default() }
    }
    fn away(now: i64, hour: u32) -> Env {
        Env { now_ms: now, idle_secs: Some(3600), local_hour: hour }
    }
    fn open(seqs: &[u64]) -> HashSet<u64> {
        seqs.iter().copied().collect()
    }

    #[test]
    fn quiet_hours_wrap_midnight_and_equal_hours_mean_none() {
        let s = PhoneSettings::default();
        assert!(quiet_at(&s, 23) && quiet_at(&s, 0) && quiet_at(&s, 7));
        assert!(!quiet_at(&s, 8) && !quiet_at(&s, 12) && !quiet_at(&s, 22));
        let day = PhoneSettings { quiet_from_hour: 13, quiet_to_hour: 15, ..Default::default() };
        assert!(quiet_at(&day, 13) && quiet_at(&day, 14) && !quiet_at(&day, 15));
        assert!(!quiet_at(&PhoneSettings { quiet_to_hour: 23, ..Default::default() }, 3));
        assert!(!quiet_at(&PhoneSettings { quiet_enabled: false, ..Default::default() }, 2));
    }

    #[test]
    fn a_person_at_the_mac_is_not_pushed() {
        let mut state = State::default();
        let env = Env { now_ms: 1000, idle_secs: Some(60), local_hour: 12 };
        assert!(decide(&on(), &mut state, vec![cand(1, false, 0)], &open(&[1]), &env).is_empty());
        assert!(state.held.is_empty(), "dropped, not held for later");
        let env = Env { idle_secs: Some(AT_MAC_SECS), ..env };
        assert_eq!(decide(&on(), &mut state, vec![cand(2, false, 0)], &open(&[2]), &env).len(), 1);
    }

    #[test]
    fn quiet_hours_hold_until_they_end_unless_urgent() {
        let mut state = State::default();
        let night = away(1000, 2);
        let pushes = decide(&on(), &mut state, vec![cand(1, false, 0), cand(2, true, 0)], &open(&[1, 2]), &night);
        assert_eq!(pushes.iter().map(|p| p.seq).collect::<Vec<_>>(), vec![2]);
        assert_eq!(state.held.len(), 1);
        // Still closed-or-open at 08:00: sent only if still open.
        let morning = away(2000, 8);
        assert!(decide(&on(), &mut state.clone(), vec![], &open(&[]), &morning).is_empty());
        assert_eq!(decide(&on(), &mut state, vec![], &open(&[1]), &morning).len(), 1);
        assert!(state.held.is_empty());
    }

    #[test]
    fn no_channel_means_nothing_is_kept() {
        let mut state = State::default();
        assert!(decide(&PhoneSettings::default(), &mut state, vec![cand(1, true, 0)], &open(&[1]), &away(0, 12)).is_empty());
        assert!(state.held.is_empty());
    }

    #[test]
    fn six_an_hour_then_one_summary() {
        let mut state = State::default();
        let items: Vec<Candidate> = (1..=10).map(|n| cand(n, false, n as i64)).collect();
        let all: Vec<u64> = (1..=10).collect();
        let pushes = decide(&on(), &mut state, items, &open(&all), &away(10_000, 12));
        assert_eq!(pushes.len(), 7);
        assert_eq!(pushes[6].seq, 0);
        assert_eq!(pushes[6].title, "4 more things need you");
        assert!(state.held.is_empty());
        // More the same hour: no second summary, nothing sent.
        let more = vec![cand(11, false, 20_000)];
        assert!(decide(&on(), &mut state, more, &open(&[11]), &away(30_000, 12)).is_empty());
        // An hour later there is room again.
        let later = vec![cand(12, false, 4_000_000)];
        assert_eq!(decide(&on(), &mut state, later, &open(&[12]), &away(4_000_000 + 1, 12)).len(), 1);
    }

    #[test]
    fn a_held_item_does_not_outlive_a_day_or_its_closing() {
        let mut state = State::default();
        let night = away(1000, 2);
        decide(&on(), &mut state, vec![cand(1, false, 0), cand(2, false, 0)], &open(&[1, 2]), &night);
        assert_eq!(state.held.len(), 2);
        let much_later = Env { now_ms: 25 * HOUR_MS, idle_secs: Some(9999), local_hour: 3 };
        decide(&on(), &mut state, vec![], &open(&[1, 2]), &much_later);
        assert!(state.held.is_empty());
    }

    #[test]
    fn ntfy_titles_only_never_carries_the_detail() {
        let push = Push { seq: 1, title: "Reply to Sam".into(), detail: "secret plan".into(), link: Some("https://example.com/x".into()), urgent: false, action: String::new(), no_default: true };
        let mut s = PhoneSettings { ntfy_topic: "neko-abcdefghijklmnopqrstuvwx".into(), ..Default::default() };
        let body = ntfy_body(&s, &push);
        assert_eq!(body["title"], "Reply to Sam");
        assert!(!body.to_string().contains("secret plan") && !body.to_string().contains("example.com"));
        assert!(body.get("click").is_none());
        s.titles_only = false;
        assert!(ntfy_body(&s, &push)["message"].as_str().unwrap().contains("secret plan"));
        assert_eq!(ntfy_body(&s, &Push { urgent: true, ..push })["priority"], 4);
    }

    #[test]
    fn topics_are_long_random_and_validated() {
        let (a, b) = (new_topic(), new_topic());
        assert!(a.len() >= 24 && valid_topic(&a) && a != b);
        let cleaned = clean_settings(PhoneSettings { ntfy_topic: "short".into(), ..Default::default() }).unwrap();
        assert!(valid_topic(&cleaned.ntfy_topic), "a weak topic is replaced");
        let keep = clean_settings(PhoneSettings { ntfy_topic: a.clone(), ..Default::default() }).unwrap();
        assert_eq!(keep.ntfy_topic, a);
        assert!(clean_settings(PhoneSettings { ntfy_server: "ftp://x".into(), ..Default::default() }).is_err());
        assert!(clean_settings(PhoneSettings { quiet_from_hour: 24, ..Default::default() }).is_err());
        assert_eq!(clean_server("https://ntfy.example.org/").unwrap(), "https://ntfy.example.org");
        assert_eq!(clean_server("").unwrap(), DEFAULT_NTFY_SERVER);
    }

    #[test]
    fn the_subscribe_link_names_host_and_topic() {
        let s = PhoneSettings { ntfy_topic: "neko-abc".into(), ..Default::default() };
        assert_eq!(subscribe_link(&s), "ntfy://ntfy.sh/neko-abc");
        assert_eq!(subscribe_link(&PhoneSettings::default()), "");
    }

    #[test]
    fn hid_idle_is_read_from_ioreg() {
        let out = "    | |   \"HIDIdleTime\" = 4294967296\n    | |   \"Other\" = 5\n";
        assert_eq!(parse_hid_idle(out), Some(4));
        assert_eq!(parse_hid_idle("nothing"), None);
    }

    #[test]
    fn reminders_carry_notes_link_and_a_marker() {
        let r = reminder_for(&Push { seq: 7, title: "T".into(), detail: "why".into(), link: Some("https://example.com/p".into()), urgent: false, action: String::new(), no_default: true });
        assert!(r.notes.starts_with("why\n\nhttps://example.com/p\n\nNo default."), "{}", r.notes);
        assert!(r.notes.ends_with("\n\nneko:7"));
        let with = reminder_for(&Push { seq: 7, title: "Extend by 10 turns?".into(), detail: "> quoted by the agent".into(), action: "extend NK-14 by 10 turns".into(), ..Default::default() });
        assert_eq!(with.title, "Extend by 10 turns? ✓ = extend NK-14 by 10 turns");
        assert!(with.notes.contains("Completing this = extend NK-14 by 10 turns."));
        assert!(crate::inbox::reply_in_notes(&with.notes).is_none(), "nothing Neko wrote counts as a reply: {}", with.notes);
        assert_eq!(r.url.as_deref(), Some("https://example.com/p"));
        assert!(!reminder_for(&Push { title: "N more".into(), ..Default::default() }).notes.contains("neko:"));
    }

    fn mapped(state: &mut State, seq: u64, id: &str) {
        state.mapped.push(Mapped { seq, id: id.into(), ..Default::default() });
    }

    #[test]
    fn a_completed_reminder_closes_its_item_and_is_deleted() {
        let mut state = State::default();
        mapped(&mut state, 1, "r1");
        mapped(&mut state, 2, "r2");
        let closed = absorb_sync(&mut state, RemindersAccess::Granted, &["r1".into(), "r2".into()], &["r1".into()]);
        assert_eq!(closed, vec![1]);
        let p = plan(&on(), &mut state, &open(&[2]), &["r1".into()], 10);
        assert_eq!(p.delete, vec!["r1".to_string()]);
        absorb_done(&mut state, &[], &p.delete, &[], 10);
        assert_eq!(state.mapped.iter().map(|m| m.seq).collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn a_reminder_the_person_deleted_leaves_the_item_open() {
        let mut state = State::default();
        mapped(&mut state, 1, "r1");
        let closed = absorb_sync(&mut state, RemindersAccess::Granted, &[], &[]);
        assert!(closed.is_empty() && state.mapped.is_empty());
    }

    #[test]
    fn closing_in_neko_or_turning_the_channel_off_deletes_reminders() {
        let mut state = State { access: RemindersAccess::Granted, ..Default::default() };
        mapped(&mut state, 1, "r1");
        mapped(&mut state, 2, "r2");
        assert_eq!(plan(&on(), &mut state, &open(&[2]), &[], 10).delete, vec!["r1".to_string()]);
        let off = PhoneSettings::default();
        let p = plan(&off, &mut state, &open(&[1, 2]), &[], 10);
        assert_eq!(p.delete.len(), 2);
        assert!(!p.enabled && p.create.is_empty());
    }

    #[test]
    fn nothing_is_planned_without_access_and_stale_reminders_expire() {
        let mut state = State::default();
        state.pending.push(Pending { reminder: ReminderNew { seq: 1, ..Default::default() }, queued_ms: 0 });
        mapped(&mut state, 3, "r3");
        let p = plan(&on(), &mut state, &open(&[]), &[], 10);
        assert!(p.create.is_empty() && p.delete.is_empty() && p.enabled);
        state.access = RemindersAccess::Granted;
        assert_eq!(plan(&on(), &mut state, &open(&[3]), &[], 10).create.len(), 1);
        assert!(plan(&on(), &mut state, &open(&[3]), &[], PENDING_MAX_MS + 1).create.is_empty());
    }

    #[test]
    fn created_reminders_become_mappings_and_a_failure_is_noted_once() {
        let mut state = State::default();
        state.pending.push(Pending { reminder: ReminderNew { seq: 5, ..Default::default() }, queued_ms: 0 });
        absorb_done(&mut state, &[neko_protocol::phone::ReminderCreated { seq: 5, id: "x".into(), key: String::new() }], &[], &[], 7);
        assert!(state.pending.is_empty());
        assert_eq!(state.mapped, vec![Mapped { seq: 5, id: "x".into(), created_ms: 7, ..Default::default() }]);
        let err: Result<(), String> = Err("down".into());
        assert!(note_outcome(&mut state, "ntfy", &err, 1), "first failure is news");
        assert!(!note_outcome(&mut state, "ntfy", &err, 2), "the same failure is not");
        assert!(!note_outcome(&mut state, "ntfy", &Ok(()), 3));
        assert_eq!(state.last_error, None);
        assert_eq!(state.last_push_ms, Some(3));
    }

    #[test]
    fn a_summary_is_created_once_and_a_failed_save_is_not_retried() {
        let mut state = State { access: RemindersAccess::Granted, ..Default::default() };
        let summary = Push { title: "3 more things need you".into(), ..Default::default() };
        let (a, b) = (reminder_for(&summary), reminder_for(&summary));
        assert_ne!(a.key, b.key);
        state.pending.push(Pending { reminder: a.clone(), queued_ms: 0 });
        state.pending.push(Pending { reminder: b.clone(), queued_ms: 0 });
        let created = [neko_protocol::phone::ReminderCreated { seq: 0, id: "s1".into(), key: a.key.clone() }, neko_protocol::phone::ReminderCreated { seq: 0, id: String::new(), key: b.key.clone() }];
        absorb_done(&mut state, &created, &[], &[], 5);
        assert!(state.pending.is_empty(), "both leave the queue, the failed one too");
        assert_eq!(state.mapped.len(), 1);
        assert!(plan(&on(), &mut state, &open(&[]), &[], 6).create.is_empty());
    }

    #[test]
    fn turning_reminders_off_forgets_what_was_waiting() {
        let mut state = State { access: RemindersAccess::Granted, ..Default::default() };
        state.pending.push(Pending { reminder: ReminderNew::default(), queued_ms: 0 });
        plan(&PhoneSettings::default(), &mut state, &open(&[]), &[], 1);
        assert!(state.pending.is_empty());
    }

    #[test]
    fn touching_the_mac_keeps_what_waits_for_quiet_hours_to_end() {
        let mut state = State::default();
        decide(&on(), &mut state, vec![cand(1, false, 0)], &open(&[1]), &away(10, 2));
        let at_mac = Env { now_ms: 20, idle_secs: Some(5), local_hour: 2 };
        assert!(decide(&on(), &mut state, vec![], &open(&[1]), &at_mac).is_empty());
        assert_eq!(state.held.len(), 1);
    }

    #[test]
    fn settings_round_trip_and_old_rows_load() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(load_settings(&db), PhoneSettings::default());
        let shown = settings_with_topic(&db);
        assert!(valid_topic(&shown.ntfy_topic));
        assert_eq!(settings_with_topic(&db).ntfy_topic, shown.ntfy_topic, "the topic is made once");
        db.set_setting(SETTINGS, "{\"reminders\":true}").unwrap();
        let old = load_settings(&db);
        assert!(old.reminders && old.titles_only && old.quiet_from_hour == 23);
        let saved = save_settings(&db, PhoneSettings { ntfy: true, ..Default::default() }).unwrap();
        assert!(saved.ntfy && valid_topic(&saved.ntfy_topic));
    }

    #[test]
    fn the_real_sender_posts_json_and_reports_failures() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for status in ["200 OK", "500 Internal Server Error"] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = [0u8; 4096];
                let n = stream.read(&mut buffer).unwrap();
                seen.push(String::from_utf8_lossy(&buffer[..n]).to_string());
                write!(stream, "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            }
            seen
        });
        let url = format!("http://{address}");
        send_ntfy(&url, &json!({"topic": "t", "title": "Hello"})).unwrap();
        assert_eq!(send_ntfy(&url, &json!({})).unwrap_err(), "The ntfy server answered 500");
        let seen = server.join().unwrap();
        assert!(seen[0].starts_with("POST / ") && seen[0].contains("\"title\":\"Hello\""));
        assert!(send_ntfy("http://127.0.0.1:1", &json!({})).is_err());
    }

    #[test]
    fn ntfy_offers_only_an_open_button_and_only_for_an_address_the_person_gave() {
        let push = Push { seq: 1, title: "Reply to Sam?".into(), detail: "d".into(), ..Default::default() };
        let mut s = PhoneSettings { ntfy_topic: "neko-abcdefghijklmnopqrstuvwx".into(), ..Default::default() };
        assert!(ntfy_body(&s, &push).get("actions").is_none(), "no address, no button");
        s.open_url = "https://neko.example.net".into();
        let body = ntfy_body(&s, &push);
        let actions = body["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 1, "never an accept or decline button");
        assert_eq!((actions[0]["action"].as_str(), actions[0]["label"].as_str(), actions[0]["url"].as_str()), (Some("view"), Some("Open in Neko"), Some("https://neko.example.net/#/inbox")));
        assert_eq!(clean_open_url(" https://a.example/ ").unwrap(), "https://a.example");
        assert_eq!(clean_open_url("").unwrap(), "");
        assert!(clean_open_url("ftp://x").is_err() && clean_open_url("http://").is_err() && clean_open_url("https://a b").is_err());
        assert!(clean_settings(PhoneSettings { open_url: "javascript:alert(1)".into(), ..Default::default() }).is_err());
        assert!(clean_settings(PhoneSettings { digest_hour: 24, ..Default::default() }).is_err() && clean_settings(PhoneSettings { digest_minute: 60, ..Default::default() }).is_err());
        let ok = clean_settings(PhoneSettings::default()).unwrap();
        assert_eq!((ok.digest, ok.digest_hour, ok.digest_minute), (true, 8, 30), "08:30 by default");
    }
}
