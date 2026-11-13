//! The morning digest, the daemon's half: gathers the facts under the lock,
//! asks `neko_core::digest::compose` for the text, keeps it in its own row
//! (`neko_digest_v1`), shows it to Home and pushes it once as one reminder
//! (and an ntfy message) when a channel is on. No model call. See
//! docs/knowledge/morning-digest.md.
use crate::server::AppState;
use neko_core::digest::{self, Facts, State};
use neko_core::phone as core;
use neko_core::Db;
use neko_protocol::digest::Digest;
use neko_protocol::{Request, Response};
use serde_json::Value;

/// A digest older than this is not pushed any more (the Mac slept or quiet hours ran long).
const PUSH_MAX_AGE_MS: i64 = 12 * 3_600_000;
const DAY_MS: i64 = 86_400_000;

fn lock(state: &AppState) -> std::sync::MutexGuard<'_, Db> {
    state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Whether the stored digest has been read (so its reminder can go).
pub fn is_read(db: &Db) -> bool {
    digest::load(db).digest.is_some_and(|d| d.read)
}

pub fn mark_read(db: &Db) {
    let mut state = digest::load(db);
    if let Some(d) = state.digest.as_mut().filter(|d| !d.read) {
        d.read = true;
        let _ = digest::save(db, &state);
    }
}

/// Composes today's digest from what Neko has, or `None` when there is
/// nothing to say.
pub fn gather(db: &Db, now_ms: i64, last_composed_ms: i64, day: &str) -> Option<Digest> {
    let snapshot = neko_core::workbench::load(db).ok()?;
    let goals = neko_core::goals::load_all(db).unwrap_or_default();
    let open: Vec<_> = crate::routines::all_items(db).unwrap_or_default().into_iter().filter(|i| !i.done).collect();
    let today = neko_core::spend::today(db, &snapshot, now_ms);
    let day_start = neko_core::spend::day_start_local(now_ms);
    let calendar = neko_core::calendar::status(db).ok().filter(|s| s.enabled && s.access == neko_protocol::calendar::CalendarAccess::Granted).map(|_| {
        let events = neko_core::calendar::events_in(db, day_start, day_start + DAY_MS).unwrap_or_default();
        let timed = events.iter().filter(|e| !e.all_day).min_by_key(|e| e.start_ms);
        let first = timed.map(|e| (local_time(e.start_ms), e.title.clone()));
        (events.len(), first)
    });
    let yesterday = neko_core::spend::yesterday_usd(db, &snapshot, now_ms);
    digest::compose(&Facts {
        now_ms,
        since_ms: digest::window_start(now_ms, last_composed_ms),
        day: day.to_owned(),
        tasks: &snapshot.tasks,
        goals: &goals,
        open_items: &open,
        spend_held: today.reached(),
        calendar,
        spend_yesterday_usd: (yesterday > 0.0).then_some(yesterday),
    })
}

fn local_time(ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_millis_opt(ms).single().map_or_else(String::new, |t| t.format("%H:%M").to_string())
}

/// The wall clock the tick uses: the local day, minutes into it, the hour.
pub fn clock() -> (String, u32, u32) {
    use chrono::{Local, Timelike};
    let now = Local::now();
    (now.format("%Y-%m-%d").to_string(), now.hour() * 60 + now.minute(), now.hour())
}

/// Composes today's digest when it is due and was not handled yet; returns the stored row. Both
/// the tick and a Home page that opens after the time ask, so the first of them does it once.
pub(crate) fn compose_if_due(db: &Db, now: i64, day: &str, minutes: u32) -> Result<State, String> {
    let settings = core::load_settings(db);
    let mut stored = digest::load(db);
    if digest::due(settings.digest, &stored, day, minutes, settings.digest_hour, settings.digest_minute) {
        stored = State { day: day.to_owned(), composed_ms: now, digest: gather(db, now, stored.composed_ms, day), pushed: false };
        digest::save(db, &stored)?;
    }
    Ok(stored)
}

pub fn tick(state: &AppState, send: &dyn Fn(&str, &Value) -> Result<(), String>) -> Result<(), String> {
    let (day, minutes, hour) = clock();
    step(state, neko_core::workbench::now_ms(), &day, minutes, hour, send)
}

/// One look: composes when due, then pushes once when a channel is on and
/// it is not quiet hours. `send` is the ntfy publisher (a mock in tests).
pub(crate) fn step(state: &AppState, now: i64, day: &str, minutes: u32, hour: u32, send: &dyn Fn(&str, &Value) -> Result<(), String>) -> Result<(), String> {
    let (settings, mut stored) = {
        let db = lock(state);
        (core::load_settings(&db), digest::load(&db))
    };
    // The cheap path: nothing due and nothing to push is two small row reads.
    let is_due = digest::due(settings.digest, &stored, day, minutes, settings.digest_hour, settings.digest_minute);
    let wants_push = settings.digest && stored.digest.as_ref().is_some_and(|d| !d.read) && !stored.pushed;
    if !is_due && !wants_push {
        return Ok(());
    }
    if is_due {
        stored = compose_if_due(&lock(state), now, day, minutes)?;
    }
    let Some(d) = stored.digest.clone().filter(|d| !d.read && !stored.pushed) else { return Ok(()) };
    if !core::any_channel(&settings) || core::quiet_at(&settings, hour) {
        return Ok(());
    }
    if now - d.at_ms > PUSH_MAX_AGE_MS {
        stored.pushed = true;
        return digest::save(&lock(state), &stored);
    }
    let text = digest::to_text(&d);
    let push = core::Push { seq: core::DIGEST_SEQ, title: digest::title(&d), detail: text.clone(), ..Default::default() };
    if settings.reminders {
        let db = lock(state);
        let mut phone = core::load_state(&db);
        let mut reminder = core::reminder_for(&push);
        // The whole digest is in the notes (the generic clip is for items).
        reminder.notes = format!("{}\n\n{}", neko_core::inbox::without_reply_marks(&text.chars().take(1800).collect::<String>()), core::marker(core::DIGEST_SEQ));
        phone.pending.push(core::Pending { reminder, queued_ms: now });
        core::save_state(&db, &phone)?;
    }
    let results = crate::phone::deliver(&settings, std::slice::from_ref(&push), send);
    crate::phone::record(&lock(state), &results, now)?;
    stored.pushed = true;
    digest::save(&lock(state), &stored)
}

pub fn handle(state: &AppState, request: Request) -> Response {
    let db = lock(state);
    match request {
        Request::MarkDigestRead => mark_read(&db),
        _ => {
            let (day, minutes, _) = clock();
            let _ = compose_if_due(&db, neko_core::workbench::now_ms(), &day, minutes);
        }
    }
    let digest = digest::load(&db).digest.filter(|d| !d.read);
    Response::Digest { digest }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::phone::PhoneSettings;
    use neko_protocol::routines::NeedsYou;
    use std::cell::RefCell;

    fn setup() -> AppState {
        let state = crate::server::tests::test_state_for_phone();
        {
            let db = lock(&state);
            // Something to say: one open item with a default.
            let routine = neko_protocol::routines::Routine {
                id: "a".into(), workspace_id: "w".into(), kind: neko_protocol::routines::RoutineKind::Custom, name: "R".into(), instruction: String::new(),
                cadence: neko_protocol::routines::Cadence::Daily { hour: 9, minute: 0 }, enabled: true, model: String::new(), paseo_schedule: None, browser: false, computer: false, customized: true, catch_up: None,
            };
            neko_core::routines::save_definitions(&db, &neko_core::routines::Store { routines: vec![routine], ..Default::default() }).unwrap();
            let seq = neko_core::routines::reserve_seq(&db, 1).unwrap();
            let item = NeedsYou { seq, title: "Reply to Sam?".into(), detail: "why".into(), default: Some("Say yes".into()), created_ms: 1, touched_ms: 1, ..Default::default() };
            neko_core::routines::save_runs(&db, "a", &[neko_protocol::routines::RoutineRun { id: "run".into(), routine_id: "a".into(), started_ms: 1, ok: true, needs_you: vec![item], ..Default::default() }]).unwrap();
        }
        state
    }

    #[test]
    fn it_composes_once_at_the_chosen_time_and_pushes_once() {
        let state = setup();
        core::save_settings(&lock(&state), PhoneSettings { reminders: true, ntfy: true, digest_hour: 8, digest_minute: 30, ..Default::default() }).unwrap();
        let sent = RefCell::new(Vec::new());
        let send = |_: &str, body: &Value| {
            sent.borrow_mut().push(body.clone());
            Ok(())
        };
        let now = 1_000_000;
        step(&state, now, "2026-11-14", 8 * 60 + 29, 8, &send).unwrap();
        assert!(digest::load(&lock(&state)).digest.is_none(), "not before 08:30");
        step(&state, now, "2026-11-14", 8 * 60 + 30, 8, &send).unwrap();
        let stored = digest::load(&lock(&state));
        let d = stored.digest.clone().unwrap();
        assert_eq!(d.decisions.len(), 1);
        assert_eq!(d.decisions[0].default.as_deref(), Some("Say yes"));
        assert!(stored.pushed);
        assert_eq!(sent.borrow().len(), 1, "one ntfy message");
        let phone = core::load_state(&lock(&state));
        assert_eq!(phone.pending.len(), 1, "one reminder");
        assert!(phone.pending[0].reminder.notes.contains("Reply to Sam? (default: Say yes)") && phone.pending[0].reminder.notes.ends_with(&core::marker(core::DIGEST_SEQ)));
        // Later ticks the same day do nothing more.
        step(&state, now + 60_000, "2026-11-14", 9 * 60, 9, &send).unwrap();
        assert_eq!((sent.borrow().len(), core::load_state(&lock(&state)).pending.len()), (1, 1));
        // The Home block shows it until it is read; reading hides it.
        let Response::Digest { digest } = handle(&state, Request::MorningDigest) else { panic!() };
        assert!(digest.is_some());
        let Response::Digest { digest } = handle(&state, Request::MarkDigestRead) else { panic!() };
        assert!(digest.is_none() && is_read(&lock(&state)));
    }

    #[test]
    fn nothing_to_say_is_skipped_and_the_day_is_not_retried() {
        let state = crate::server::tests::test_state_for_phone();
        core::save_settings(&lock(&state), PhoneSettings { reminders: true, ..Default::default() }).unwrap();
        let send = |_: &str, _: &Value| -> Result<(), String> { panic!("nothing to send") };
        step(&state, 5, "2026-11-14", 9 * 60, 9, &send).unwrap();
        let stored = digest::load(&lock(&state));
        assert!(stored.digest.is_none() && stored.day == "2026-11-14", "handled, with nothing to say");
        assert!(core::load_state(&lock(&state)).pending.is_empty());
    }

    #[test]
    fn it_waits_out_quiet_hours_and_off_means_off() {
        let state = setup();
        core::save_settings(&lock(&state), PhoneSettings { reminders: true, digest_hour: 7, digest_minute: 0, ..Default::default() }).unwrap();
        let send = |_: &str, _: &Value| Ok(());
        step(&state, 10, "d", 7 * 60, 7, &send).unwrap();
        let stored = digest::load(&lock(&state));
        assert!(stored.digest.is_some() && !stored.pushed, "composed, held for quiet hours (23:00 to 08:00)");
        assert!(core::load_state(&lock(&state)).pending.is_empty());
        step(&state, 20, "d", 8 * 60, 8, &send).unwrap();
        assert!(digest::load(&lock(&state)).pushed);
        assert_eq!(core::load_state(&lock(&state)).pending.len(), 1);
        // Off: nothing composed.
        let off = setup();
        core::save_settings(&lock(&off), PhoneSettings { reminders: true, digest: false, ..Default::default() }).unwrap();
        step(&off, 10, "d", 9 * 60, 9, &send).unwrap();
        assert!(digest::load(&lock(&off)).digest.is_none());
    }
}
