//! The Calendar's storage and Neko's dated work. The installed app reads
//! macOS Calendar (EventKit, which includes the Google, iCloud and Exchange
//! accounts added in System Settings) every five minutes and on every change,
//! and sends the whole window (7 days back, 35 ahead) with
//! `SetCalendarEvents`. It is kept in one quiet row outside the workbench
//! snapshot, capped at `MAX_EVENTS` and `MAX_BYTES`; an identical resend
//! writes nothing. See docs/knowledge/calendar.md.
use crate::Db;
use chrono::TimeZone;
use neko_protocol::calendar::{CalendarAccess, CalendarEvent, CalendarStatus, NekoCalendarItem, NekoItemKind};
use neko_protocol::routines::{Cadence, Routine, RoutineState};
use neko_protocol::scheduled_plans::Schedule;
use neko_protocol::workbench::{Task, TaskStatus};
use std::sync::atomic::{AtomicI64, Ordering};

pub const EVENTS: &str = "calendar_events_v1";
/// Access, source and counts: rewritten only when one of them changes, or
/// when the stored sync time is an hour old (`SYNC_STALE_MS`). The sync time
/// itself changes every five minutes and lives in memory (`LAST_SYNC`).
pub const STATUS: &str = "calendar_status_v1";
pub const MAX_EVENTS: usize = 2000;
pub const MAX_BYTES: usize = 1024 * 1024;
/// Six weeks (a month view) plus slack; a wider ask is refused.
pub const MAX_RANGE_MS: i64 = 100 * DAY_MS;
const MAX_NEKO: usize = 500;
/// Occurrences listed per schedule or daily routine in one answer.
const MAX_REPEATS: usize = 60;
const SYNC_STALE_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;
const MAX_TITLE: usize = 300;
const MAX_CALENDAR: usize = 120;
const MAX_LOCATION: usize = 300;
const MAX_URL: usize = 2048;
const MAX_ID: usize = 512;
const MAX_SOURCE: usize = 80;

/// The newest sync this process has seen (one daemon, one database).
static LAST_SYNC: AtomicI64 = AtomicI64::new(0);

fn cap(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max { text.to_owned() } else { text.chars().take(max).collect() }
}

fn valid_color(color: &str) -> String {
    let color = color.trim();
    let hex = color.strip_prefix('#').unwrap_or("");
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) { color.to_ascii_uppercase() } else { String::new() }
}

fn web_link(url: Option<String>) -> Option<String> {
    let url = url?.trim().to_owned();
    let lower = url.to_ascii_lowercase();
    ((lower.starts_with("https://") || lower.starts_with("http://")) && url.len() <= MAX_URL && !url.contains(char::is_whitespace))
        .then_some(url)
}

/// Trims every field to its limit, drops events without an id (and repeats
/// of one), sorts by start, and keeps what fits in `MAX_EVENTS` and
/// `MAX_BYTES`. The flag says whether anything was left out.
pub fn clean(events: Vec<CalendarEvent>) -> (Vec<CalendarEvent>, bool) {
    let mut seen = std::collections::HashSet::new();
    let mut kept: Vec<CalendarEvent> = events
        .into_iter()
        .filter_map(|e| {
            let id = cap(&e.id, MAX_ID);
            if id.is_empty() || !seen.insert(id.clone()) {
                return None;
            }
            Some(CalendarEvent {
                id,
                calendar: cap(&e.calendar, MAX_CALENDAR),
                color: valid_color(&e.color),
                title: cap(&e.title, MAX_TITLE),
                start_ms: e.start_ms,
                end_ms: e.end_ms.max(e.start_ms),
                all_day: e.all_day,
                location: e.location.map(|l| cap(&l, MAX_LOCATION)).filter(|l| !l.is_empty()),
                url: web_link(e.url),
            })
        })
        .collect();
    let unique = kept.len();
    kept.sort_by(|a, b| (a.start_ms, &a.id).cmp(&(b.start_ms, &b.id)));
    kept.truncate(MAX_EVENTS);
    let mut bytes = 2;
    let fits = kept
        .iter()
        .position(|e| {
            bytes += serde_json::to_string(e).map(|s| s.len() + 1).unwrap_or(MAX_BYTES);
            bytes > MAX_BYTES
        })
        .unwrap_or(kept.len());
    kept.truncate(fits);
    let truncated = kept.len() < unique;
    (kept, truncated)
}

fn read_json<T: serde::de::DeserializeOwned + Default>(db: &Db, key: &str) -> Result<T, String> {
    match db.get_setting(key).map_err(|e| e.to_string())? {
        Some(text) => serde_json::from_str(&text).map_err(|e| format!("Cannot read {key}: {e}")),
        None => Ok(T::default()),
    }
}

/// Replaces the stored events with what the app just read. Without access,
/// nothing is kept: the person said no.
pub fn store(db: &Db, events: Vec<CalendarEvent>, source: &str, access: CalendarAccess, now_ms: i64) -> Result<CalendarStatus, String> {
    let (events, truncated) = if access == CalendarAccess::Granted { clean(events) } else { (Vec::new(), false) };
    let text = serde_json::to_string(&events).map_err(|e| e.to_string())?;
    db.set_setting_quiet(EVENTS, &text).map_err(|e| e.to_string())?;
    let source = match cap(source, MAX_SOURCE) {
        s if s.is_empty() => "macOS Calendar".to_owned(),
        s => s,
    };
    LAST_SYNC.fetch_max(now_ms, Ordering::AcqRel);
    let stored: CalendarStatus = read_json(db, STATUS)?;
    let mut status = CalendarStatus { access, source, last_sync_ms: Some(now_ms), events: events.len(), truncated, enabled: false };
    let same = CalendarStatus { last_sync_ms: stored.last_sync_ms, ..status.clone() } == stored;
    if !same || stored.last_sync_ms.is_none_or(|at| now_ms - at >= SYNC_STALE_MS) {
        db.set_setting_quiet(STATUS, &serde_json::to_string(&status).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    status.last_sync_ms = Some(now_ms);
    status.enabled = enabled(db);
    Ok(status)
}

const ENABLED: &str = "calendar_enabled_v1";

/// Whether the app may ask for, and read, the person's calendars. A choice is
/// kept; with none, a Mac whose calendar was already shared (an earlier build
/// asked at launch) stays on, and a fresh data folder stays off until the
/// first-run setup or Settings turns it on.
pub fn enabled(db: &Db) -> bool {
    match db.get_setting(ENABLED).ok().flatten().as_deref() {
        Some("on") => true,
        Some("off") => false,
        _ => read_json::<CalendarStatus>(db, STATUS).is_ok_and(|s| s.access == CalendarAccess::Granted),
    }
}

pub fn set_enabled(db: &Db, on: bool) -> Result<CalendarStatus, String> {
    db.set_setting(ENABLED, if on { "on" } else { "off" }).map_err(|e| e.to_string())?;
    if !on {
        // Switched off: what was copied stops being kept.
        db.set_setting_quiet(EVENTS, "[]").map_err(|e| e.to_string())?;
    }
    status(db)
}

/// What is stored, with the newest sync time this daemon has seen.
pub fn status(db: &Db) -> Result<CalendarStatus, String> {
    let mut status: CalendarStatus = read_json(db, STATUS)?;
    status.enabled = enabled(db);
    let recent = LAST_SYNC.load(Ordering::Acquire);
    if recent > 0 && status.last_sync_ms.is_none_or(|at| at < recent) {
        status.last_sync_ms = Some(recent);
    }
    Ok(status)
}

fn check_range(from_ms: i64, to_ms: i64) -> Result<(), String> {
    if to_ms <= from_ms {
        return Err("The calendar range ends before it starts".into());
    }
    if to_ms - from_ms > MAX_RANGE_MS {
        return Err("Ask for at most 100 days of the calendar at a time".into());
    }
    Ok(())
}

/// Stored events that overlap `[from_ms, to_ms)`, by start.
pub fn events_in(db: &Db, from_ms: i64, to_ms: i64) -> Result<Vec<CalendarEvent>, String> {
    check_range(from_ms, to_ms)?;
    let events: Vec<CalendarEvent> = read_json(db, EVENTS)?;
    Ok(events
        .into_iter()
        .filter(|e| e.start_ms < to_ms && (e.end_ms > from_ms || (e.end_ms == e.start_ms && e.start_ms >= from_ms)))
        .collect())
}

/// What Neko's dated work is read from, gathered under the database lock;
/// `neko_items` then works without it.
#[derive(Debug, Default, Clone)]
pub struct NekoSources {
    pub tasks: Vec<Task>,
    pub schedules: Vec<Schedule>,
    pub routines: Vec<(Routine, RoutineState)>,
}

pub fn read_sources(db: &Db) -> Result<NekoSources, String> {
    let mut snapshot = crate::workbench::load(db)?;
    // Only the fields `neko_items` reads; events and results can be large.
    let tasks = snapshot
        .tasks
        .drain(..)
        .filter(|t| finished(t.status).is_some())
        .map(|mut t| {
            t.events.clear();
            t.plan.clear();
            t.result.clear();
            t.goal.clear();
            t
        })
        .collect();
    let store = crate::routines::load(db)?;
    let mut routines = Vec::new();
    for routine in store.routines.into_iter().filter(|r| r.enabled) {
        let state = crate::routines::load_state(db, &routine.id)?;
        routines.push((routine, state));
    }
    Ok(NekoSources { tasks, schedules: std::mem::take(&mut snapshot.schedules), routines })
}

/// A task has a date on the Calendar once it is finished: the day it became
/// ready for review, done or failed. Open tasks carry no date.
fn finished(status: TaskStatus) -> Option<&'static str> {
    match status {
        TaskStatus::ReadyForReview => Some("Ready for review"),
        TaskStatus::Completed => Some("Done"),
        TaskStatus::Failed => Some("Failed"),
        _ => None,
    }
}

fn encode_component(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn pad(n: u32) -> String {
    format!("{n:02}")
}

/// Neko's work with a date inside `[from_ms, to_ms)`: finished tasks on the
/// day they finished, each enabled scheduled plan's coming runs, each daily
/// routine's coming runs and each repeating routine's next one. Past runs of
/// schedules and routines are not listed; what they produced is a task or an
/// Inbox item. Times are read in `tz` (the Mac's own zone in the daemon).
pub fn neko_items<Tz: TimeZone>(sources: &NekoSources, from_ms: i64, to_ms: i64, now_ms: i64, tz: &Tz) -> Vec<NekoCalendarItem> {
    let mut items = Vec::new();
    let inside = |at: i64| at >= from_ms && at < to_ms;
    for task in &sources.tasks {
        let Some(detail) = finished(task.status) else { continue };
        if !inside(task.updated_at_ms) {
            continue;
        }
        items.push(NekoCalendarItem {
            id: format!("task:{}", task.id),
            kind: NekoItemKind::Task,
            title: task.display_title.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| task.title.clone()),
            at_ms: task.updated_at_ms,
            all_day: true,
            status: format!("{:?}", task.status),
            detail: detail.to_owned(),
            route: format!("tasks?task={}", encode_component(&task.id)),
        });
    }
    for schedule in sources.schedules.iter().filter(|s| s.enabled) {
        let Some(mut at) = schedule.next_due_ms else { continue };
        if at < from_ms {
            match crate::scheduled_plans::following_due(schedule, from_ms - 1) {
                Ok(Some(next)) => at = next,
                _ => continue,
            }
        }
        for _ in 0..MAX_REPEATS {
            if at >= to_ms {
                break;
            }
            items.push(NekoCalendarItem {
                id: format!("schedule:{}:{at}", schedule.id),
                kind: NekoItemKind::Schedule,
                title: schedule.name.clone(),
                at_ms: at,
                all_day: false,
                status: String::new(),
                detail: "Scheduled plan".into(),
                route: "schedules".into(),
            });
            match crate::scheduled_plans::following_due(schedule, at) {
                Ok(Some(next)) if next > at => at = next,
                _ => break,
            }
        }
    }
    for (routine, state) in &sources.routines {
        let push = |items: &mut Vec<NekoCalendarItem>, at: i64, detail: String| {
            items.push(NekoCalendarItem {
                id: format!("routine:{}:{at}", routine.id),
                kind: NekoItemKind::Routine,
                title: routine.name.clone(),
                at_ms: at,
                all_day: false,
                status: String::new(),
                detail,
                route: "inbox".into(),
            });
        };
        match &routine.cadence {
            Cadence::Daily { hour, minute } => {
                let detail = format!("Daily at {}:{}", pad(*hour), pad(*minute));
                let mut cursor = from_ms.max(now_ms) - 1;
                for _ in 0..MAX_REPEATS {
                    let at = crate::routines::next_due(&routine.cadence, 0, cursor, tz);
                    if at >= to_ms || at <= cursor {
                        break;
                    }
                    push(&mut items, at, detail.clone());
                    cursor = at;
                }
            }
            Cadence::Every { minutes, .. } => {
                let at = if state.running { now_ms } else { state.next_due_ms.unwrap_or(now_ms).max(now_ms) };
                if inside(at) {
                    let detail = if state.running { "Running now".to_owned() } else { format!("Next run · every {minutes} min") };
                    push(&mut items, at, detail);
                }
            }
        }
    }
    items.sort_by(|a, b| (a.at_ms, &a.id).cmp(&(b.at_ms, &b.id)));
    items.truncate(MAX_NEKO);
    items
}

/// `neko_items` in the Mac's own time zone.
pub fn neko_items_local(sources: &NekoSources, from_ms: i64, to_ms: i64, now_ms: i64) -> Vec<NekoCalendarItem> {
    neko_items(sources, from_ms, to_ms, now_ms, &chrono::Local)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_is_off_on_a_fresh_folder_and_on_where_it_was_already_shared() {
        let db = Db::open_in_memory().unwrap();
        assert!(!enabled(&db));
        store(&db, vec![], "macOS Calendar", CalendarAccess::Granted, 1).unwrap();
        assert!(enabled(&db), "an earlier build asked at launch and was allowed");
        assert!(!set_enabled(&db, false).unwrap().enabled, "a choice wins over history");
        assert!(set_enabled(&db, true).unwrap().enabled);
    }
    use neko_protocol::routines::RoutineKind;

    fn event(id: &str, start: i64, end: i64) -> CalendarEvent {
        CalendarEvent { id: id.into(), calendar: "Work".into(), color: "#1e88e5".into(), title: format!("Event {id}"), start_ms: start, end_ms: end, ..Default::default() }
    }

    #[test]
    fn clean_trims_validates_dedupes_and_sorts() {
        let mut bad = event("b", 10, 5);
        bad.color = "red".into();
        bad.url = Some("javascript:alert(1)".into());
        bad.location = Some("   ".into());
        bad.title = "x".repeat(1000);
        let mut good = event("a", 20, 30);
        good.url = Some("https://meet.example.com/abc".into());
        let (kept, truncated) = clean(vec![good.clone(), bad, event("a", 1, 2), event("", 0, 1)]);
        assert!(!truncated);
        assert_eq!(kept.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["b", "a"]);
        let b = &kept[0];
        assert_eq!((b.start_ms, b.end_ms), (10, 10), "an end before the start becomes the start");
        assert_eq!(b.color, "");
        assert_eq!(b.url, None);
        assert_eq!(b.location, None);
        assert_eq!(b.title.chars().count(), MAX_TITLE);
        assert_eq!(kept[1].color, "#1E88E5");
        assert_eq!(kept[1].url.as_deref(), Some("https://meet.example.com/abc"));
    }

    #[test]
    fn clean_caps_count_and_bytes() {
        let many: Vec<_> = (0..2500).map(|i| event(&format!("e{i:04}"), i, i + 1)).collect();
        let (kept, truncated) = clean(many);
        assert!(truncated);
        assert_eq!(kept.len(), MAX_EVENTS);
        assert_eq!(kept[0].id, "e0000", "the earliest are kept");

        let big: Vec<_> = (0..2000).map(|i| {
            let mut e = event(&format!("{i:04}{}", "i".repeat(500)), i, i + 1);
            e.title = "t".repeat(MAX_TITLE);
            e
        }).collect();
        let (kept, truncated) = clean(big);
        assert!(truncated);
        assert!(serde_json::to_string(&kept).unwrap().len() <= MAX_BYTES);
        assert!(kept.len() > 100);
    }

    #[test]
    fn store_keeps_events_quietly_and_skips_identical_writes() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(status(&db).unwrap().access, CalendarAccess::NotAsked);
        let events = vec![event("a", 1_000, 2_000), event("b", 5_000, 6_000)];
        let first = store(&db, events.clone(), "macOS Calendar", CalendarAccess::Granted, 10_000).unwrap();
        assert_eq!((first.access, first.events, first.last_sync_ms), (CalendarAccess::Granted, 2, Some(10_000)));
        let writes = db.setting_writes();
        // The same events five minutes later: neither row is rewritten.
        let again = store(&db, events, "macOS Calendar", CalendarAccess::Granted, 310_000).unwrap();
        assert_eq!(db.setting_writes(), writes);
        assert_eq!(again.last_sync_ms, Some(310_000));
        assert!(status(&db).unwrap().last_sync_ms >= Some(310_000));

        assert_eq!(events_in(&db, 0, 3_000).unwrap().len(), 1);
        assert_eq!(events_in(&db, 2_000, 5_000).unwrap().len(), 0, "touching ends do not overlap");
        assert_eq!(events_in(&db, 0, 10_000).unwrap().len(), 2);
        assert!(events_in(&db, 10, 5).is_err());
        assert!(events_in(&db, 0, MAX_RANGE_MS + 1).is_err());
    }

    #[test]
    fn denied_access_drops_what_was_stored() {
        let db = Db::open_in_memory().unwrap();
        store(&db, vec![event("a", 1, 2)], "", CalendarAccess::Granted, 1).unwrap();
        let denied = store(&db, vec![event("a", 1, 2)], "", CalendarAccess::Denied, 2).unwrap();
        assert_eq!((denied.access, denied.events), (CalendarAccess::Denied, 0));
        assert_eq!(denied.source, "macOS Calendar");
        assert!(events_in(&db, 0, 10).unwrap().is_empty());
        assert_eq!(status(&db).unwrap().access, CalendarAccess::Denied);
    }

    fn task(id: &str, status: TaskStatus, at: i64) -> Task {
        Task {
            id: id.into(), workspace_id: "w".into(), issue_id: None, title: format!("Task {id}"), goal: String::new(),
            status, plan: String::new(), result: String::new(), worktree: None, events: vec![],
            created_at_ms: 0, updated_at_ms: at, source_revision: None, base_commit: None, pull_request: None,
            landed_commit: None, supervision: None, display_title: None, parked: None, request: None,
        }
    }

    fn routine(id: &str, cadence: Cadence) -> Routine {
        Routine { id: id.into(), workspace_id: "w".into(), kind: RoutineKind::Custom, name: format!("Routine {id}"), instruction: "x".into(), cadence, enabled: true, model: String::new(), paseo_schedule: None, browser: true, computer: true, customized: false }
    }

    #[test]
    fn neko_items_date_finished_tasks_and_coming_runs() {
        let utc = chrono::Utc;
        let day0 = utc.with_ymd_and_hms(2026, 11, 2, 0, 0, 0).unwrap().timestamp_millis();
        let now = day0 + 10 * 3_600_000; // Monday 10:00
        let week = day0 + 7 * DAY_MS;
        let mut named = task("t 1", TaskStatus::Completed, day0 + 3_600_000);
        named.display_title = Some("Fix the login".into());
        let sources = NekoSources {
            tasks: vec![named, task("t2", TaskStatus::Building, now), task("t3", TaskStatus::Failed, week + 1)],
            schedules: vec![
                Schedule { id: "s1".into(), name: "Weekly review".into(), rule: "FREQ=DAILY".into(), timezone: "UTC".into(), anchor_ms: day0 + 9 * 3_600_000, next_due_ms: Some(day0 + 33 * 3_600_000), enabled: true, ..Default::default() },
                Schedule { id: "s2".into(), name: "Paused".into(), next_due_ms: Some(now), enabled: false, ..Default::default() },
            ],
            routines: vec![
                (routine("brief", Cadence::Daily { hour: 9, minute: 0 }), RoutineState::default()),
                (routine("mail", Cadence::Every { minutes: 20, from_hour: 8, to_hour: 23 }), RoutineState { next_due_ms: Some(now + 600_000), ..Default::default() }),
            ],
        };
        let items = neko_items(&sources, day0, week, now, &utc);
        let tasks: Vec<_> = items.iter().filter(|i| i.kind == NekoItemKind::Task).collect();
        assert_eq!(tasks.len(), 1, "open tasks and tasks outside the range have no place");
        assert_eq!(tasks[0].title, "Fix the login");
        assert_eq!(tasks[0].route, "tasks?task=t%201");
        assert_eq!((tasks[0].status.as_str(), tasks[0].detail.as_str(), tasks[0].all_day), ("Completed", "Done", true));

        let schedule: Vec<_> = items.iter().filter(|i| i.kind == NekoItemKind::Schedule).collect();
        assert!(!schedule.is_empty() && schedule.len() <= 7, "{schedule:?}");
        assert_eq!(schedule[0].at_ms, day0 + 33 * 3_600_000);
        assert!(schedule.iter().all(|i| i.route == "schedules" && i.at_ms < week));

        let brief: Vec<_> = items.iter().filter(|i| i.id.starts_with("routine:brief")).collect();
        // Monday's 09:00 has passed; Tuesday to Sunday remain.
        assert_eq!(brief.len(), 6);
        assert_eq!(brief[0].at_ms, day0 + DAY_MS + 9 * 3_600_000);
        assert_eq!(brief[0].detail, "Daily at 09:00");
        let mail: Vec<_> = items.iter().filter(|i| i.id.starts_with("routine:mail")).collect();
        assert_eq!(mail.len(), 1);
        assert_eq!(mail[0].at_ms, now + 600_000);
        assert_eq!(mail[0].route, "inbox");
        assert!(items.windows(2).all(|w| w[0].at_ms <= w[1].at_ms));
    }
}
