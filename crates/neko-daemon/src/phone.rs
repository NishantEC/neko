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

fn tick(state: &Arc<AppState>) -> Result<(), String> {
    let (pushes, settings) = {
        let db = lock(state);
        plan(&db, neko_core::workbench::now_ms(), &hid_idle_secs, core::local_hour())?
    };
    if pushes.is_empty() {
        return Ok(());
    }
    let results = deliver(&settings, &pushes, &core::send_ntfy);
    record(&lock(state), &results, neko_core::workbench::now_ms())
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
    let items = routines::all_items(db)?;
    let newest = items.iter().map(|i| i.seq).max().unwrap_or(0).max(latest);
    let cursor = state.cursor.unwrap_or(newest);
    // Turning a channel on never pushes what was already there.
    let new: Vec<Candidate> = items
        .iter()
        .filter(|i| i.seq > cursor && !i.done)
        .map(|i| Candidate { seq: i.seq, title: i.title.clone(), detail: i.detail.clone(), link: i.link.clone(), urgent: i.urgent, since_ms: now })
        .collect();
    state.cursor = Some(newest);
    let open: HashSet<u64> = items.iter().filter(|i| !i.done).map(|i| i.seq).collect();
    let waiting = !new.is_empty() || !state.held.is_empty();
    let env = Env { now_ms: now, idle_secs: if waiting && core::any_channel(&settings) { idle() } else { None }, local_hour: hour };
    let pushes = core::decide(&settings, &mut state, new, &open, &env);
    if settings.reminders {
        for push in pushes.iter() {
            state.pending.push(Pending { reminder: core::reminder_for(push), queued_ms: now });
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

/// The app's report. Items whose reminders were completed close as "Done on
/// your phone"; the answer says what to create and delete.
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
    let closed = core::absorb_sync(&mut state, access, present, completed);
    if !closed.is_empty() {
        routines::close_seqs(db, &closed, neko_core::inbox::ON_PHONE)?;
    }
    let open: HashSet<u64> = if state.mapped.is_empty() { HashSet::new() } else { routines::all_items(db)?.iter().filter(|i| !i.done).map(|i| i.seq).collect() };
    let plan = core::plan(&settings, &mut state, &open, completed, now);
    if state != before {
        core::save_state(db, &state)?;
    }
    Ok(plan)
}

/// A test push on every channel that is on, whatever the hour or idle time.
fn test_push(state: &AppState, send: &dyn Fn(&str, &Value) -> Result<(), String>) -> Vec<String> {
    let push = Push { seq: 0, title: "Test from Neko".into(), detail: "If you can read this, Neko can reach you here.".into(), link: None, urgent: false };
    let (settings, access) = {
        let db = lock(state);
        (core::settings_with_topic(&db), core::load_state(&db).access)
    };
    let mut lines = Vec::new();
    if settings.reminders {
        if access == RemindersAccess::Granted {
            let db = lock(state);
            let mut phone = core::load_state(&db);
            phone.pending.push(Pending { reminder: core::reminder_for(&push), queued_ms: neko_core::workbench::now_ms() });
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
        Request::PhoneRemindersSync { access, present, completed } => sync(&lock(state), now, access, &present, &completed).map(Response::PhonePlan),
        Request::PhoneRemindersDone { created, deleted } => {
            let db = lock(state);
            let mut phone = core::load_state(&db);
            core::absorb_done(&mut phone, &created, &deleted, now);
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
        core::absorb_done(&mut phone, &[neko_protocol::phone::ReminderCreated { seq, id: "rem1".into() }], &[], 23);
        core::save_state(&db, &phone).unwrap();
        assert!(sync(&db, 24, RemindersAccess::Granted, &["rem1".into()], &[]).unwrap().create.is_empty());
        // Completed on the phone: the item closes with its reason, the reminder is removed.
        let p = sync(&db, 25, RemindersAccess::Granted, &["rem1".into()], &["rem1".into()]).unwrap();
        assert_eq!(p.delete, vec!["rem1".to_string()]);
        let item = &rcore::load_runs(&db, "a").unwrap()[0].needs_you[0];
        assert!(item.done && item.reason.as_deref() == Some("Done on your phone"));
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
        core::absorb_done(&mut phone, &[neko_protocol::phone::ReminderCreated { seq, id: "rem1".into() }], &[], 22);
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
}
