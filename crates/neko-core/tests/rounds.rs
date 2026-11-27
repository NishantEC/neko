//! Rounds in the core (docs/knowledge/rounds.md): the scheduler's logic with time and the time zone handed in. Every test says what
//! time it is; none waits.

use neko_core::rounds::{Env, RoundError};
use neko_core::store::SCHEMA_VERSION;
use neko_core::{Core, FixedClock, SequentialIds, Store};
use neko_protocol::rounds::{BRAKE_AFTER, CATCH_UP_MS, HISTORY_KEPT, RunEnd, RunKind};
use neko_protocol::schedule::{Fixed, Rules, Schedule, Weekday, Zone, days_from_civil};
use neko_protocol::{Change, PartBody, RoundStatus, TaskStatus};

struct Gone(std::path::PathBuf);
impl Drop for Gone {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir() -> Gone {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("neko-rounds-test-{}-{n}-{nanos}", std::process::id()));
    std::fs::create_dir_all(d.join("w")).unwrap();
    Gone(d)
}

fn open(d: &Gone) -> Core {
    // Each open starts the core's clock later, as a daemon that is started again does: the ids it makes then differ.
    static OPENS: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
    let n = OPENS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Core::open(&d.0.join("neko.sqlite3"), Box::new(FixedClock::new(1_764_000_000_000 + n * 10_000_000, 1000)), Box::new(SequentialIds::default())).unwrap()
}

const HOUR: i64 = 3_600_000;
const UTC: Fixed = Fixed(0);

fn at(y: i64, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
    (days_from_civil(y, mo, d) * 86_400 + i64::from(h) * 3600 + i64::from(mi) * 60) * 1000
}

fn env(now: i64) -> Env<'static> {
    Env { now, zone: &UTC }
}

fn daily(c: &mut Core, at_: &str, now: i64) -> String {
    c.create_round("Docs check", "Check the docs for broken links", "cat_mochi", None, &Schedule::Daily { at: at_.into() }, true, env(now)).unwrap().0.id
}

fn finish(c: &mut Core, task: &neko_protocol::TaskId, status: TaskStatus) {
    c.store.set_task_status(task, status, 1).unwrap();
}

#[test]
fn a_round_has_its_words_cat_workspace_schedule_and_a_next_time() {
    let d = dir();
    let mut c = open(&d);
    let now = at(2026, 11, 27, 8, 0);
    let (r, _) = c.create_round("  Docs   check ", "Check the docs", "cat_mochi", None, &Schedule::Weekdays { days: vec![Weekday::Fri, Weekday::Mon], at: "9:00".into() }, true, env(now)).unwrap();
    assert_eq!((r.name.as_str(), r.cat.as_str(), r.workspace.as_str(), r.on, r.status), ("Docs check", "cat_mochi", "default", true, RoundStatus::Scheduled));
    assert_eq!(r.schedule, Schedule::Weekdays { days: vec![Weekday::Mon, Weekday::Fri], at: "09:00".into() });
    assert_eq!(r.next_due_ms, Some(at(2026, 11, 27, 9, 0)), "Friday 09:00 is the first run");
    assert_eq!(c.rounds(env(now)).unwrap().0.len(), 1);
    // Manual rounds have no next time; off rounds neither.
    let (m, _) = c.create_round("By hand", "x", "cat_miso", None, &Schedule::Manual, true, env(now)).unwrap();
    assert_eq!((m.status, m.next_due_ms), (RoundStatus::Manual, None));
    let (off, _) = c.set_round_on(&r.id, false, env(now)).unwrap();
    assert_eq!((off.status, off.next_due_ms), (RoundStatus::Off, None));
}

#[test]
fn a_round_is_refused_with_words_it_can_act_on() {
    let d = dir();
    let mut c = open(&d);
    let e = env(0);
    let s = Schedule::Manual;
    assert!(matches!(c.create_round(" ", "x", "cat_miso", None, &s, true, e), Err(RoundError::InvalidName(_))));
    assert!(matches!(c.create_round("A", "  ", "cat_miso", None, &s, true, e), Err(RoundError::InvalidWords(_))));
    assert!(matches!(c.create_round("A", "x", "cat_nobody", None, &s, true, e), Err(RoundError::Refused(_))));
    assert!(matches!(c.create_round("A", "x", "cat_miso", Some("ws_nope"), &s, true, e), Err(RoundError::Refused(_))));
    assert!(matches!(c.create_round("A", "x", "cat_miso", None, &Schedule::Every { hours: 0 }, true, e), Err(RoundError::Refused(_))));
    assert!(matches!(c.create_round("A", "x", "cat_miso", None, &Schedule::Weekdays { days: vec![], at: "09:00".into() }, true, e), Err(RoundError::Refused(_))));
    c.retire_cat("cat_tofu").unwrap();
    assert!(matches!(c.create_round("A", "x", "cat_tofu", None, &s, true, e), Err(RoundError::Refused(w)) if w.contains("tofu is retired")));
    assert!(matches!(c.round("round_nope", e), Err(RoundError::NotFound)));
}

#[test]
fn a_due_time_makes_an_ordinary_task_marked_as_the_rounds() {
    let d = dir();
    let mut c = open(&d);
    let id = daily(&mut c, "09:00", at(2026, 11, 27, 8, 0));
    let t = c.tick_rounds(env(at(2026, 11, 27, 8, 59))).unwrap();
    assert!(t.started.is_empty(), "not due yet");
    assert_eq!(t.wake_at_ms, Some(at(2026, 11, 27, 9, 0)), "it sleeps until the next due time");
    let t = c.tick_rounds(env(at(2026, 11, 27, 9, 0) + 400)).unwrap();
    assert_eq!(t.started.len(), 1);
    let task = &t.started[0];
    assert_eq!((task.title.as_str(), task.cat.as_str(), task.workspace.as_str(), task.status), ("Docs check", "cat_mochi", "default", TaskStatus::New));
    assert_eq!(task.words, "Check the docs for broken links");
    let from = task.round.as_ref().unwrap();
    assert_eq!((from.id.as_str(), from.name.as_str(), from.due_ms), (id.as_str(), "Docs check", Some(at(2026, 11, 27, 9, 0))));
    assert_eq!(c.store.get_task(&task.id).unwrap().unwrap(), *task, "stored with the mark");
    assert_eq!(t.wake_at_ms, Some(at(2026, 11, 28, 9, 0)));
    assert!(t.events.iter().any(|(_, ch)| matches!(ch, Change::TaskCreated { .. })) && t.events.iter().any(|(_, ch)| matches!(ch, Change::RoundUpdated { .. })));
    let revs: Vec<u64> = t.events.iter().map(|e| e.0).collect();
    assert!(revs.windows(2).all(|w| w[0] <= w[1]), "events leave in revision order");
    let h = c.round_history(&id).unwrap();
    assert_eq!((h.len(), h[0].kind, h[0].due_ms, h[0].task.clone()), (1, RunKind::Started, Some(at(2026, 11, 27, 9, 0)), Some(task.id.clone())));
    // The same time is not run twice.
    assert!(c.tick_rounds(env(at(2026, 11, 27, 9, 0) + 900)).unwrap().started.is_empty());
}

#[test]
fn several_missed_times_run_once_and_say_so() {
    let d = dir();
    let mut c = open(&d);
    let id = daily(&mut c, "09:00", at(2026, 11, 20, 8, 0));
    // Made on the 20th at 08:00. The Mac slept through the 20th's, 21st's, 22nd's and 23rd's times; it is noticed on the 23rd at
    // 18:00 (the latest, 09:00 that day, is 9 hours old).
    let t = c.tick_rounds(env(at(2026, 11, 23, 18, 0))).unwrap();
    assert_eq!(t.started.len(), 1, "once, not once per missed time");
    assert_eq!(t.started[0].round.as_ref().unwrap().due_ms, Some(at(2026, 11, 23, 9, 0)), "for the latest time");
    let h = c.round_history(&id).unwrap();
    assert_eq!(h.iter().map(|l| l.kind).collect::<Vec<_>>().len(), 2);
    let missed: Vec<_> = h.iter().filter(|l| l.kind == RunKind::Missed).collect();
    assert_eq!(missed.len(), 1, "one line for the earlier ones");
    assert!(missed[0].reason.as_deref().unwrap().contains("3 earlier times"), "{:?}", missed[0].reason);
    assert_eq!(t.wake_at_ms, Some(at(2026, 11, 24, 9, 0)));
}

#[test]
fn a_time_more_than_a_day_old_is_recorded_as_missed_not_run() {
    let anchor = at(2026, 11, 20, 8, 0);
    let sunday = at(2026, 11, 22, 9, 0);
    let weekly = Schedule::Weekdays { days: vec![Weekday::Sun], at: "09:00".into() };
    // Exactly 24 hours old is "at most 24 hours": it runs.
    let d = dir();
    let mut c = open(&d);
    c.create_round("Weekly", "x", "cat_miso", None, &weekly, true, env(anchor)).unwrap();
    assert_eq!(c.tick_rounds(env(sunday + CATCH_UP_MS)).unwrap().started.len(), 1);
    // One millisecond older does not: it is recorded as missed, once.
    let d = dir();
    let mut c = open(&d);
    let (r, _) = c.create_round("Weekly", "x", "cat_miso", None, &weekly, true, env(anchor)).unwrap();
    assert!(c.tick_rounds(env(sunday + CATCH_UP_MS + 1)).unwrap().started.is_empty());
    let h = c.round_history(&r.id).unwrap();
    assert_eq!((h.len(), h[0].kind, h[0].due_ms), (1, RunKind::Missed, Some(sunday)));
    assert!(h[0].reason.as_deref().unwrap().contains("Neko was not running"));
    assert!(c.tick_rounds(env(sunday + CATCH_UP_MS + 2)).unwrap().started.is_empty());
    assert_eq!(c.round_history(&r.id).unwrap().len(), 1, "recorded once");
    // A long gap with many times due: the latest is what counts, so a week-long sleep ending just after a 09:00 runs once.
    let d = dir();
    let mut c = open(&d);
    let id = daily(&mut c, "09:00", anchor);
    let t = c.tick_rounds(env(at(2026, 11, 27, 9, 5))).unwrap();
    assert_eq!(t.started.len(), 1);
    assert_eq!(c.round_history(&id).unwrap().iter().filter(|l| l.kind == RunKind::Missed).count(), 1);
}

#[test]
fn a_time_while_the_last_task_still_works_or_waits_is_skipped_with_the_reason() {
    let d = dir();
    let mut c = open(&d);
    let id = daily(&mut c, "09:00", at(2026, 11, 26, 8, 0));
    let first = c.tick_rounds(env(at(2026, 11, 26, 9, 0))).unwrap().started.remove(0);
    finish(&mut c, &first.id, TaskStatus::Working);
    let t = c.tick_rounds(env(at(2026, 11, 27, 9, 0))).unwrap();
    assert!(t.started.is_empty());
    let h = c.round_history(&id).unwrap();
    assert_eq!((h[0].kind, h[0].due_ms), (RunKind::Skipped, Some(at(2026, 11, 27, 9, 0))));
    assert!(h[0].reason.as_deref().unwrap().starts_with("The last run is still working."), "{:?}", h[0].reason);
    // Waiting for the person (a request, or a question) skips too.
    finish(&mut c, &first.id, TaskStatus::NeedsYou);
    assert!(c.tick_rounds(env(at(2026, 11, 28, 9, 0))).unwrap().started.is_empty());
    assert!(c.round_history(&id).unwrap()[0].reason.as_deref().unwrap().starts_with("The last run is waiting for you."));
    finish(&mut c, &first.id, TaskStatus::Asked);
    assert!(c.tick_rounds(env(at(2026, 11, 29, 9, 0))).unwrap().started.is_empty());
    // Done: the next time runs.
    finish(&mut c, &first.id, TaskStatus::Done);
    assert_eq!(c.tick_rounds(env(at(2026, 11, 30, 9, 0))).unwrap().started.len(), 1);
}

#[test]
fn three_failed_runs_in_a_row_turn_the_round_off_and_it_says_so() {
    let d = dir();
    let mut c = open(&d);
    let id = daily(&mut c, "09:00", at(2026, 11, 20, 8, 0));
    let mut last = None;
    for day in 21..=23 {
        let t = c.tick_rounds(env(at(2026, 11, day, 9, 0))).unwrap();
        let task = t.started[0].id.clone();
        finish(&mut c, &task, TaskStatus::Failed);
        last = Some(task);
        // Settling happens on the next look (the daemon looks when a task ends).
        c.settle_rounds(env(at(2026, 11, day, 9, 5))).unwrap();
        let r = c.round(&id, env(at(2026, 11, day, 9, 6))).unwrap();
        if day < 23 {
            assert!(r.on, "{day}: not yet");
        }
    }
    let r = c.round(&id, env(at(2026, 11, 23, 9, 6))).unwrap();
    assert!(!r.on);
    assert_eq!(r.status, RoundStatus::Braked);
    assert!(r.note.as_deref().unwrap().contains(&format!("{BRAKE_AFTER} failed runs in a row")), "{:?}", r.note);
    let parts = c.store.parts_of(&last.unwrap()).unwrap();
    assert!(parts.iter().any(|p| matches!(&p.body, PartBody::Notice { text, .. } if text.contains("turned itself off after 3 failed runs in a row"))), "a notice in its last task");
    assert_eq!(r.next_due_ms, None);
    assert!(c.tick_rounds(env(at(2026, 11, 24, 9, 0))).unwrap().started.is_empty(), "it runs no more");
    // Turned back on, it counts from now and the brake is clear.
    let (r, _) = c.set_round_on(&id, true, env(at(2026, 11, 24, 10, 0))).unwrap();
    assert!((r.on, r.status, r.note.clone()) == (true, RoundStatus::Scheduled, None));
    assert_eq!(r.next_due_ms, Some(at(2026, 11, 25, 9, 0)), "nothing old is caught up");
    let ends: Vec<_> = c.round_history(&id).unwrap().iter().filter_map(|l| l.end).collect();
    assert_eq!(ends, [RunEnd::Failed; 3]);
}

#[test]
fn a_success_or_an_interruption_does_not_count_toward_the_brake_the_way_a_failure_does() {
    let d = dir();
    let mut c = open(&d);
    let id = daily(&mut c, "09:00", at(2026, 11, 20, 8, 0));
    let outcomes = [TaskStatus::Failed, TaskStatus::Failed, TaskStatus::Done, TaskStatus::Failed, TaskStatus::Failed, TaskStatus::Interrupted, TaskStatus::Stopped];
    for (i, s) in outcomes.iter().enumerate() {
        let day = 21 + i as u32;
        let task = c.tick_rounds(env(at(2026, 11, day, 9, 0))).unwrap().started.remove(0).id;
        finish(&mut c, &task, *s);
        c.settle_rounds(env(at(2026, 11, day, 9, 5))).unwrap();
    }
    assert!(c.round(&id, env(at(2026, 11, 28, 9, 6))).unwrap().on, "two, a success, two, then runs that neither failed nor succeeded");
}

#[test]
fn pausing_every_round_holds_them_and_nothing_is_caught_up_after() {
    let d = dir();
    let mut c = open(&d);
    let id = daily(&mut c, "09:00", at(2026, 11, 20, 8, 0));
    let (events, _) = c.set_rounds_paused(true, env(at(2026, 11, 20, 12, 0))).unwrap();
    assert!(matches!(events[0].1, Change::RoundsPaused { paused: true }) && events.len() == 2);
    assert_eq!(c.round(&id, env(0)).unwrap().status, RoundStatus::Paused);
    assert!(c.store.rounds_paused().unwrap());
    let t = c.tick_rounds(env(at(2026, 11, 21, 9, 0))).unwrap();
    assert!(t.started.is_empty());
    let h = c.round_history(&id).unwrap();
    assert_eq!((h[0].kind, h[0].reason.as_deref()), (RunKind::Skipped, Some("Every round was paused.")));
    c.set_rounds_paused(false, env(at(2026, 11, 21, 12, 0))).unwrap();
    assert!(c.tick_rounds(env(at(2026, 11, 21, 12, 1))).unwrap().started.is_empty(), "the 09:00 that passed is not run now");
    assert_eq!(c.tick_rounds(env(at(2026, 11, 22, 9, 0))).unwrap().started.len(), 1);
    // The switch is in the store: a daemon that restarts reads it.
    c.set_rounds_paused(true, env(at(2026, 11, 22, 10, 0))).unwrap();
    drop(c);
    let c = open(&d);
    assert!(c.store.rounds_paused().unwrap());
}

#[test]
fn a_retired_cat_a_missing_folder_or_a_removed_workspace_skips_and_says_why() {
    let d = dir();
    let mut c = open(&d);
    let folder = d.0.join("w");
    let ws = c.add_workspace(folder.to_str().unwrap()).unwrap().workspace;
    let start = at(2026, 11, 20, 8, 0);
    let (r, _) = c.create_round("Docs check", "x", "cat_mochi", Some(&ws.id), &Schedule::Daily { at: "09:00".into() }, true, env(start)).unwrap();
    assert_eq!(r.workspace, ws.id);
    // The folder goes missing.
    std::fs::remove_dir_all(&folder).unwrap();
    let view = c.round(&r.id, env(start)).unwrap();
    assert_eq!((view.status, view.next_due_ms), (RoundStatus::FolderMissing, None));
    assert!(view.note.as_deref().unwrap().contains("was not found"));
    assert!(c.tick_rounds(env(at(2026, 11, 21, 9, 0))).unwrap().started.is_empty());
    let h = c.round_history(&r.id).unwrap();
    assert_eq!(h[0].kind, RunKind::Skipped);
    assert!(h[0].reason.as_deref().unwrap().contains("The folder of w was not found."), "{:?}", h[0].reason);
    // It comes back: the next time runs.
    std::fs::create_dir_all(&folder).unwrap();
    assert_eq!(c.tick_rounds(env(at(2026, 11, 22, 9, 0))).unwrap().started.len(), 1);
    // A retired cat's rounds do not run and say why.
    c.retire_cat("cat_mochi").unwrap();
    let view = c.round(&r.id, env(start)).unwrap();
    assert_eq!(view.status, RoundStatus::CatRetired);
    assert_eq!(view.note.as_deref(), Some("mochi is retired."));
    let task = c.store.all_tasks().unwrap()[0].id.clone();
    finish(&mut c, &task, TaskStatus::Done);
    assert!(c.tick_rounds(env(at(2026, 11, 23, 9, 0))).unwrap().started.is_empty());
    assert_eq!(c.round_history(&r.id).unwrap()[0].reason.as_deref(), Some("mochi is retired."));
    // A removed workspace likewise.
    c.bring_back_cat("cat_mochi").unwrap();
    c.remove_workspace(&ws.id).unwrap();
    assert_eq!(c.round(&r.id, env(start)).unwrap().status, RoundStatus::WorkspaceRemoved);
    assert!(c.tick_rounds(env(at(2026, 11, 24, 9, 0))).unwrap().started.is_empty());
    assert!(c.round_history(&r.id).unwrap()[0].reason.as_deref().unwrap().contains("was removed from Neko."));
}

#[test]
fn removing_a_round_removes_its_history_and_never_its_tasks() {
    let d = dir();
    let mut c = open(&d);
    let id = daily(&mut c, "09:00", at(2026, 11, 20, 8, 0));
    let task = c.tick_rounds(env(at(2026, 11, 21, 9, 0))).unwrap().started.remove(0);
    c.remove_round(&id, env(at(2026, 11, 21, 10, 0))).unwrap();
    assert!(c.rounds(env(0)).unwrap().0.is_empty());
    assert!(matches!(c.round_history(&id), Err(RoundError::NotFound)));
    let kept = c.store.get_task(&task.id).unwrap().unwrap();
    assert_eq!(kept.round.unwrap().name, "Docs check", "the task still says where it came from");
    assert_eq!(c.store.all_tasks().unwrap().len(), 1);
    assert!(c.tick_rounds(env(at(2026, 11, 22, 9, 0))).unwrap().started.is_empty());
    let n: i64 = c.store.connection().query_row("SELECT COUNT(*) FROM round_runs", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn run_now_works_whatever_the_switch_or_pause_say_and_refuses_while_unfinished() {
    let d = dir();
    let mut c = open(&d);
    let id = daily(&mut c, "09:00", at(2026, 11, 20, 8, 0));
    c.set_round_on(&id, false, env(at(2026, 11, 20, 8, 1))).unwrap();
    c.set_rounds_paused(true, env(at(2026, 11, 20, 8, 2))).unwrap();
    let (view, task, events) = c.run_round_now(&id, env(at(2026, 11, 20, 8, 3))).unwrap();
    assert_eq!(task.round.as_ref().unwrap().due_ms, None);
    assert_eq!(view.last.as_ref().map(|l| (l.kind, l.due_ms)), Some((RunKind::Started, None)));
    assert_eq!(events.len(), 2);
    assert!(matches!(c.run_round_now(&id, env(at(2026, 11, 20, 8, 4))), Err(RoundError::Busy(w)) if w.contains("still working")));
    finish(&mut c, &task.id, TaskStatus::Done);
    assert!(c.run_round_now(&id, env(at(2026, 11, 20, 8, 5))).is_ok());
    // It does not move the schedule.
    c.set_round_on(&id, true, env(at(2026, 11, 20, 8, 6))).unwrap();
    c.set_rounds_paused(false, env(at(2026, 11, 20, 8, 7))).unwrap();
    assert_eq!(c.round(&id, env(0)).unwrap().next_due_ms, Some(at(2026, 11, 20, 9, 0)), "turning it on counted from then");
    // A retired cat's round does not run now either.
    c.retire_cat("cat_mochi").unwrap();
    let t = c.store.all_tasks().unwrap()[0].id.clone();
    finish(&mut c, &t, TaskStatus::Done);
    assert!(matches!(c.run_round_now(&id, env(at(2026, 11, 20, 9, 0))), Err(RoundError::Refused(w)) if w == "mochi is retired."));
}

#[test]
fn every_n_hours_counts_from_when_it_was_made_turned_on_or_changed() {
    let d = dir();
    let mut c = open(&d);
    let t0 = at(2026, 11, 27, 10, 17);
    let (r, _) = c.create_round("Hourly", "x", "cat_miso", None, &Schedule::Every { hours: 3 }, true, env(t0)).unwrap();
    assert_eq!(r.next_due_ms, Some(t0 + 3 * HOUR));
    assert!(c.tick_rounds(env(t0 + 3 * HOUR - 1)).unwrap().started.is_empty());
    let t = c.tick_rounds(env(t0 + 3 * HOUR)).unwrap();
    assert_eq!(t.started.len(), 1);
    assert_eq!(t.wake_at_ms, Some(t0 + 6 * HOUR));
    finish(&mut c, &t.started[0].id, TaskStatus::Done);
    // Changing the schedule counts from then: nothing that was due under the old one is run.
    let (r, _) = c.update_round(&r.id, "Hourly", "x", "cat_miso", None, &Schedule::Every { hours: 5 }, env(t0 + 4 * HOUR)).unwrap();
    assert_eq!(r.next_due_ms, Some(t0 + 9 * HOUR));
    assert!(c.tick_rounds(env(t0 + 8 * HOUR)).unwrap().started.is_empty());
    assert_eq!(c.tick_rounds(env(t0 + 9 * HOUR)).unwrap().started.len(), 1);
}

#[test]
fn across_daylight_saving_a_daily_round_runs_exactly_once_on_each_date() {
    let d = dir();
    let mut c = open(&d);
    let ny = Rules::us(-5 * 3600);
    let envz = |now: i64| Env { now, zone: &ny };
    let (r, _) = c.create_round("Early", "x", "cat_miso", None, &Schedule::Daily { at: "02:30".into() }, true, envz(at(2026, 3, 6, 12, 0))).unwrap();
    let mut ran = Vec::new();
    // Look every half hour from Friday noon to Tuesday noon (UTC), as a daemon that was up all the time would.
    let mut now = at(2026, 3, 6, 12, 0);
    while now < at(2026, 3, 10, 12, 0) {
        now += 30 * 60_000;
        for t in c.tick_rounds(envz(now)).unwrap().started {
            ran.push(t.round.as_ref().unwrap().due_ms.unwrap());
            finish(&mut c, &t.id, TaskStatus::Done);
        }
    }
    assert_eq!(ran, vec![at(2026, 3, 7, 7, 30), at(2026, 3, 8, 7, 30), at(2026, 3, 9, 6, 30), at(2026, 3, 10, 6, 30)], "Sunday's 02:30 does not exist: it ran once, at 03:30 EDT");
    // Fall back: 01:30 happens twice on 2026-11-01; once.
    let (r2, _) = c.create_round("Late", "x", "cat_miso", None, &Schedule::Daily { at: "01:30".into() }, true, envz(at(2026, 10, 30, 12, 0))).unwrap();
    let mut ran = Vec::new();
    let mut now = at(2026, 10, 30, 12, 0);
    while now < at(2026, 11, 2, 12, 0) {
        now += 30 * 60_000;
        for t in c.tick_rounds(envz(now)).unwrap().started {
            if t.round.as_ref().unwrap().id == r2.id {
                ran.push(t.round.as_ref().unwrap().due_ms.unwrap());
            }
            finish(&mut c, &t.id, TaskStatus::Done);
        }
    }
    assert_eq!(ran, vec![at(2026, 10, 31, 5, 30), at(2026, 11, 1, 5, 30), at(2026, 11, 2, 6, 30)], "01:30 on the day the clocks go back runs once, the first time it happens");
    let _ = r;
    assert_eq!(ny.offset_secs(at(2026, 11, 1, 6, 0) / 1000), -5 * 3600);
}

#[test]
fn a_restart_in_the_middle_neither_runs_a_time_twice_nor_loses_one() {
    let d = dir();
    let mut c = open(&d);
    let id = daily(&mut c, "09:00", at(2026, 11, 20, 8, 0));
    assert_eq!(c.tick_rounds(env(at(2026, 11, 21, 9, 0))).unwrap().started.len(), 1);
    drop(c);
    // The daemon restarts a moment later: the 21st's time is not run again.
    let mut c = open(&d);
    assert!(c.tick_rounds(env(at(2026, 11, 21, 9, 1))).unwrap().started.is_empty());
    assert_eq!(c.store.all_tasks().unwrap().len(), 1);
    // The next time is still ahead: not lost.
    assert_eq!(c.round(&id, env(at(2026, 11, 21, 9, 1))).unwrap().next_due_ms, Some(at(2026, 11, 22, 9, 0)));
    // The daemon was down over the 22nd's time: it is run once when it is up again (within a day).
    let task = c.store.all_tasks().unwrap()[0].id.clone();
    finish(&mut c, &task, TaskStatus::Done);
    drop(c);
    let mut c = open(&d);
    let t = c.tick_rounds(env(at(2026, 11, 22, 9, 30))).unwrap();
    assert_eq!(t.started.len(), 1);
    assert_eq!(t.started[0].round.as_ref().unwrap().due_ms, Some(at(2026, 11, 22, 9, 0)));
    // A task made but whose agent never started is found again.
    assert_eq!(c.store.round_tasks_never_started().unwrap().len(), 1, "the daemon starts it at launch");
    // The task and the history line are written together: a time with a task always has its line.
    let h = c.round_history(&id).unwrap();
    assert!(h.iter().filter(|l| l.kind == RunKind::Started).all(|l| l.task.is_some()));
}

#[test]
fn a_manual_round_is_always_on_and_the_brake_leaves_it_alone() {
    let d = dir();
    let mut c = open(&d);
    let (r, _) = c.create_round("By hand", "x", "cat_miso", None, &Schedule::Manual, false, env(0)).unwrap();
    assert!(r.on, "it has nothing to switch");
    // Run now three times and fail each: still on.
    for i in 0..3 {
        let (_, task, _) = c.run_round_now(&r.id, env(1000 + i)).unwrap();
        finish(&mut c, &task.id, TaskStatus::Failed);
        c.settle_rounds(env(2000 + i)).unwrap();
    }
    let view = c.round(&r.id, env(5000)).unwrap();
    assert_eq!((view.on, view.status), (true, RoundStatus::Manual));
    // A round that was off and is then made manual is on.
    let id = daily(&mut c, "09:00", 0);
    c.set_round_on(&id, false, env(1)).unwrap();
    let (v, _) = c.update_round(&id, "Docs check", "x", "cat_mochi", None, &Schedule::Manual, env(2)).unwrap();
    assert!(v.on && v.status == RoundStatus::Manual);
}

#[test]
fn only_the_last_fifty_history_lines_are_kept() {
    let d = dir();
    let mut c = open(&d);
    let id = daily(&mut c, "09:00", at(2026, 1, 1, 0, 0));
    for day in 0..(HISTORY_KEPT as i64 + 20) {
        let t = c.tick_rounds(env(at(2026, 1, 2, 9, 0) + day * 24 * HOUR)).unwrap();
        for task in t.started {
            finish(&mut c, &task.id, TaskStatus::Done);
        }
    }
    assert_eq!(c.round_history(&id).unwrap().len(), HISTORY_KEPT);
    let h = c.round_history(&id).unwrap();
    assert!(h.windows(2).all(|w| w[0].at_ms >= w[1].at_ms), "newest first");
}

#[test]
fn the_migration_from_store_six_adds_rounds_and_changes_nothing_else() {
    let d = dir();
    let path = d.0.join("neko.sqlite3");
    {
        let mut core = Core::open(&path, Box::new(FixedClock::new(1_764_000_000_000, 1000)), Box::new(SequentialIds::default())).unwrap();
        core.create_task("Rename the sidebar label").unwrap();
    }
    // A store as the last Neko left it: version 6, no rounds.
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("DROP TABLE wakes; DROP INDEX tasks_by_parent; DROP INDEX tasks_by_root; ALTER TABLE tasks DROP COLUMN parent_id; ALTER TABLE tasks DROP COLUMN sub_number; ALTER TABLE tasks DROP COLUMN root_id; ALTER TABLE tasks DROP COLUMN last_read_ms; ALTER TABLE tasks DROP COLUMN activity_ms; ALTER TABLE tasks DROP COLUMN latest_line; DROP TABLE plugin_values; DROP TABLE plugins; DROP TABLE round_runs; DROP TABLE rounds; DROP INDEX tasks_by_round; ALTER TABLE tasks DROP COLUMN round_id; ALTER TABLE tasks DROP COLUMN round_due_ms; ALTER TABLE tasks DROP COLUMN round_name; PRAGMA user_version = 6;").unwrap();
    }
    assert_eq!(Store::open_at(&path, 6).unwrap().version().unwrap(), 6);
    let mut core = Core::open(&path, Box::new(FixedClock::new(1_764_000_100_000, 1000)), Box::new(SequentialIds::default())).unwrap();
    assert_eq!(core.store.version().unwrap(), 10, "every migration after 6 ran: rounds' own is the seventh");
    assert_eq!(SCHEMA_VERSION, 10);
    let tasks = core.store.all_tasks().unwrap();
    assert_eq!((tasks.len(), tasks[0].title.as_str(), tasks[0].round.clone()), (1, "Rename the sidebar label", None));
    assert!(core.rounds(env(0)).unwrap().0.is_empty());
    assert!(!core.store.rounds_paused().unwrap());
    assert_eq!(core.create_round("A", "x", "cat_miso", None, &Schedule::Manual, true, env(5)).unwrap().0.name, "A");
}
