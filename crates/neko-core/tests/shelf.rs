//! Pin, settle, archive in the store (docs/knowledge/threads.md): what each does to the others, what brings a thread back, what the
//! housekeeping settles and what it never touches, and that a store from before them opens with nothing put away.

mod support;
use neko_core::shelf::{AUTO_SETTLE_AFTER_MS, LOOK_EVERY_MS, Shelf, refusal};
use neko_core::{Core, FixedClock, SequentialIds};
use neko_protocol::{Task, TaskId, TaskStatus, ThreadStatus};
use support::Temp;

const T0: i64 = 1_764_000_000_000;
const DAY: i64 = 24 * 3_600_000;

fn open(d: &Temp) -> Core {
    Core::open(&d.0.join("neko.sqlite3"), Box::new(FixedClock::new(T0, 1000)), Box::new(SequentialIds::default())).unwrap()
}

fn get(core: &Core, id: &TaskId) -> Task {
    core.store.get_task(id).unwrap().unwrap()
}

fn flags(t: &Task) -> (bool, bool, bool) {
    (t.pinned, t.settled, t.archived)
}

#[test]
fn pinning_and_settling_exclude_each_other_and_archiving_keeps_both() {
    let d = Temp::new("shelf-flags");
    let mut core = open(&d);
    let (t, _) = core.create_task("Fix login").unwrap();
    assert_eq!(flags(&t), (false, false, false), "a new thread is none of them");
    let (t, _) = core.store.set_shelf(&t.id, Shelf::Settle, true, T0 + 10).unwrap();
    assert_eq!(flags(&t), (false, true, false));
    let (t, _) = core.store.set_shelf(&t.id, Shelf::Pin, true, T0 + 20).unwrap();
    assert_eq!(flags(&t), (true, false, false), "pinning brings a settled thread back");
    let (t, _) = core.store.set_shelf(&t.id, Shelf::Settle, true, T0 + 30).unwrap();
    assert_eq!(flags(&t), (false, true, false), "settling a pinned thread unpins it");
    let (t, _) = core.store.set_shelf(&t.id, Shelf::Pin, true, T0 + 40).unwrap();
    let (t, _) = core.store.set_shelf(&t.id, Shelf::Archive, true, T0 + 50).unwrap();
    assert_eq!(flags(&t), (true, false, true), "archiving leaves the pin, so restoring puts it back exactly");
    let (t, _) = core.store.set_shelf(&t.id, Shelf::Archive, false, T0 + 60).unwrap();
    assert_eq!(flags(&t), (true, false, false));
    let (t, _) = core.store.set_shelf(&t.id, Shelf::Pin, false, T0 + 70).unwrap();
    assert_eq!(flags(&t), (false, false, false));
    // Each change is one event in the log.
    let kinds: Vec<String> = core.store.events_since(0).unwrap().into_iter().map(|e| e.1).filter(|k| k != "task_created").collect();
    assert_eq!(kinds, ["task_settled", "task_pinned", "task_settled", "task_pinned", "task_archived", "task_unarchived", "task_unpinned"]);
    // They survive a reopen.
    let (t, _) = core.store.set_shelf(&t.id, Shelf::Settle, true, T0 + 80).unwrap();
    drop(core);
    assert_eq!(flags(&get(&open(&d), &t.id)), (false, true, false));
}

#[test]
fn what_cannot_be_done_now_is_refused_in_words_and_taking_back_never_is() {
    let d = Temp::new("shelf-refusals");
    let mut core = open(&d);
    let (thread, _) = core.create_task("Fix login").unwrap();
    let (kitten, _) = core.create_subagent(&thread.id, "Check links", "Look.").unwrap();
    assert!(refusal(&kitten, Shelf::Pin, true).unwrap().contains("kitten"), "a subagent is not a thread");
    assert!(refusal(&kitten, Shelf::Pin, false).unwrap().contains("kitten"));
    let mut t = thread;
    assert_eq!(refusal(&t, Shelf::Pin, true), None);
    t.thread_status = ThreadStatus::Working;
    assert_eq!(refusal(&t, Shelf::Pin, true), None, "a working thread may be pinned");
    assert!(refusal(&t, Shelf::Settle, true).unwrap().contains("working"));
    assert!(refusal(&t, Shelf::Archive, true).unwrap().contains("Stop it"));
    assert_eq!(refusal(&t, Shelf::Settle, false), None);
    assert_eq!(refusal(&t, Shelf::Archive, false), None, "restoring is always allowed");
    t.thread_status = ThreadStatus::Waiting;
    assert!(refusal(&t, Shelf::Archive, true).unwrap().contains("kittens"));
    assert_eq!(refusal(&t, Shelf::Settle, true), None, "a thread that waits may be settled; its report brings it back");
}

#[test]
fn new_activity_brings_a_settled_thread_back_and_a_run_wakes_an_archived_one() {
    let d = Temp::new("shelf-wake");
    let mut core = open(&d);
    let (t, _) = core.create_task("Fix login").unwrap();
    core.store.set_task_status(&t.id, TaskStatus::Done, T0 + 5).unwrap();
    core.store.set_shelf(&t.id, Shelf::Settle, true, T0 + 10).unwrap();
    core.store.set_shelf(&t.id, Shelf::Archive, true, T0 + 11).unwrap();
    // A thread that stops is the person's own doing: it stays where it was put.
    let t = core.store.set_task_status(&t.id, TaskStatus::Stopped, T0 + 20).unwrap().0;
    assert_eq!(flags(&t), (false, true, true));
    // A run starting (a message, a wake, a round) brings it back and wakes it.
    let t = core.store.set_task_status(&t.id, TaskStatus::Working, T0 + 30).unwrap().0;
    assert_eq!(flags(&t), (false, false, false));
    // A run's end brings a settled thread back; so do the agent's words.
    core.store.set_shelf(&t.id, Shelf::Settle, true, T0 + 40).unwrap();
    let t = core.store.set_task_status(&t.id, TaskStatus::Done, T0 + 50).unwrap().0;
    assert!(!t.settled, "the end of a run is news");
}

#[test]
fn the_housekeeping_settles_what_was_left_alone_for_a_week_and_nothing_else() {
    let d = Temp::new("shelf-auto");
    let mut core = open(&d);
    let make = |core: &mut Core, words: &str, status: TaskStatus| {
        let (t, _) = core.create_task(words).unwrap();
        core.store.set_task_status(&t.id, status, T0).unwrap();
        // Every task begins at T0, whatever the clock said when it was made.
        core.store.connection().execute("UPDATE tasks SET created_ms = ?2, activity_ms = ?2 WHERE id = ?1", rusqlite::params![t.id.0, T0]).unwrap();
        t.id
    };
    let idle = make(&mut core, "Idle for a week", TaskStatus::Done);
    let pinned = make(&mut core, "Pinned", TaskStatus::Done);
    let waiting = make(&mut core, "Waiting on kittens", TaskStatus::Waiting);
    let asked = make(&mut core, "Asked a question", TaskStatus::Asked);
    let working = make(&mut core, "Still working", TaskStatus::Working);
    let archived = make(&mut core, "Archived", TaskStatus::Done);
    let recent = make(&mut core, "Recent", TaskStatus::Done);
    core.store.connection().execute("UPDATE tasks SET created_ms = ?2, activity_ms = ?2 WHERE id = ?1", rusqlite::params![recent.0, T0 + 6 * DAY]).unwrap();
    core.store.set_shelf(&pinned, Shelf::Pin, true, T0).unwrap();
    core.store.set_shelf(&archived, Shelf::Archive, true, T0).unwrap();

    // Not a week yet: nothing is settled, and it says when to look again (the first week ends in 7 days, but not later than a look).
    let (settled, wake) = core.store.auto_settle(T0 + 7 * DAY - 1, AUTO_SETTLE_AFTER_MS).unwrap();
    assert!(settled.is_empty());
    assert_eq!(wake, Some(T0 + 7 * DAY), "the nearest week is up in a moment, sooner than a look");

    let (settled, wake) = core.store.auto_settle(T0 + 8 * DAY, AUTO_SETTLE_AFTER_MS).unwrap();
    let names: Vec<&str> = settled.iter().map(|(t, _)| t.words.as_str()).collect();
    assert_eq!(names, ["Idle for a week"], "only the idle one; a recent one waits for its own week");
    assert!(settled[0].0.settled);
    for (id, why) in [(&pinned, "a pinned thread"), (&waiting, "one that waits on its kittens"), (&asked, "one that needs the person"), (&working, "one that works"), (&archived, "an archived one")] {
        assert!(!get(&core, id).settled, "never settled by itself: {why}");
    }
    assert!(!get(&core, &recent).settled);
    assert_eq!(wake, Some(T0 + 8 * DAY + LOOK_EVERY_MS), "the others may become idle: it looks again in a few hours");
    // Asking again settles nothing more, and the idle one is not settled twice.
    let (again, _) = core.store.auto_settle(T0 + 8 * DAY, AUTO_SETTLE_AFTER_MS).unwrap();
    assert!(again.is_empty());
    // Two weeks on, the recent one goes too.
    let (later, _) = core.store.auto_settle(T0 + 14 * DAY, AUTO_SETTLE_AFTER_MS).unwrap();
    assert_eq!(later.iter().map(|(t, _)| t.id.clone()).collect::<Vec<_>>(), [recent]);
}

#[test]
fn with_nothing_that_could_be_settled_the_housekeeping_asks_for_no_timer() {
    let d = Temp::new("shelf-no-timer");
    let mut core = open(&d);
    assert_eq!(core.store.auto_settle(T0, AUTO_SETTLE_AFTER_MS).unwrap().1, None, "no threads at all");
    let (t, _) = core.create_task("Pinned").unwrap();
    core.store.set_shelf(&t.id, Shelf::Pin, true, T0).unwrap();
    assert_eq!(core.store.auto_settle(T0 + 30 * DAY, AUTO_SETTLE_AFTER_MS).unwrap().1, None, "only a pinned thread: nothing to look for");
}

#[test]
fn a_store_from_before_them_opens_with_nothing_put_away() {
    let d = Temp::new("shelf-migrate");
    {
        let _old = neko_core::Store::open_at(&d.0.join("neko.sqlite3"), 12).unwrap();
    }
    let core = open(&d);
    assert_eq!(core.store.version().unwrap(), neko_core::store::SCHEMA_VERSION);
    assert_eq!(core.store.all_tasks().unwrap().len(), 0);
}
