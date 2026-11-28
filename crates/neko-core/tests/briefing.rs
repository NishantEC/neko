//! The Briefing in the core (docs/knowledge/briefing.md): the window and its edges, each section built from seeded records, the first
//! sentence, undone since, round lines, totals, "mark read", and the morning notification's decisions with time and zone handed in.
//! Every test says what time it is; none waits.

use neko_core::briefing::BRAKE_LINE;
use neko_core::conductor::{RunBook, Who};
use neko_core::rounds::Env;
use neko_core::runtime::{Action, AgentEvent, Failure, Outcome, Reported};
use neko_core::store::SCHEMA_VERSION;
use neko_core::{Core, FixedClock, Place, SequentialIds, Store};
use neko_protocol::briefing::{BriefingSettings, FinishedEnd, MAX_FINISHED, RoundOutcome};
use neko_protocol::schedule::{Fixed, Rules, Schedule, days_from_civil};
use neko_protocol::undo::{Action as UndoAction, RunUndo, UndoState};
use neko_protocol::{Changes, Pricing, RunId, TaskId, TaskStatus};

struct Gone(std::path::PathBuf);
impl Drop for Gone {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir() -> Gone {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("neko-briefing-test-{}-{n}", std::process::id()));
    std::fs::create_dir_all(d.join("w")).unwrap();
    Gone(d)
}

fn open(d: &Gone) -> Core {
    Core::open(&d.0.join("neko.sqlite3"), Box::new(FixedClock::new(1_764_000_000_000, 1000)), Box::new(SequentialIds::default())).unwrap()
}

const HOUR: i64 = 3_600_000;
const UTC: Fixed = Fixed(0);

fn at(y: i64, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
    (days_from_civil(y, mo, d) * 86_400 + i64::from(h) * 3600 + i64::from(mi) * 60) * 1000
}

fn env(now: i64) -> Env<'static> {
    Env { now, zone: &UTC }
}

fn who() -> Who {
    Who { id: "claude".into(), name: "Claude Code".into(), sign_in: "claude auth login".into(), install: "install".into(), pricing: Pricing::ApiEquivalent }
}

fn begin(core: &mut Core, d: &Gone, task: &TaskId, branch: Option<&str>) -> RunBook {
    let place = Place { workdir: d.0.join("w").to_string_lossy().into_owned(), branch: branch.map(str::to_owned), base_commit: None, repo: None, plain: false };
    RunBook::begin(core, task, who(), place, neko_protocol::PermissionMode::Follow, None, None, None).unwrap().0
}

fn say(core: &mut Core, b: &mut RunBook, words: &str) {
    let now = core.now();
    b.on(core, AgentEvent::TextDelta(words.into()), now).unwrap();
    let now = core.now();
    b.on(core, AgentEvent::TextDone, now).unwrap();
}

fn finish(core: &mut Core, b: &mut RunBook, o: Outcome, changes: Option<Changes>) {
    let now = core.now();
    b.finish(core, o, changes, now).unwrap();
}

/// Puts a run's start and end where the test wants them.
fn place(core: &Core, run: &RunId, started: i64, ended: Option<i64>) {
    core.store.connection().execute("UPDATE runs SET started_ms = ?2, ended_ms = ?3 WHERE id = ?1", rusqlite::params![run.0, started, ended]).unwrap();
}

struct Seed<'a> {
    words: &'a str,
    answer: &'a str,
    outcome: Outcome,
    ended: i64,
    changes: Option<Changes>,
    cost: Option<f64>,
    branch: Option<&'a str>,
}

impl<'a> Seed<'a> {
    fn done(words: &'a str, answer: &'a str, ended: i64) -> Seed<'a> {
        Seed { words, answer, outcome: Outcome::Done, ended, changes: None, cost: None, branch: None }
    }
}

fn seed(core: &mut Core, d: &Gone, s: Seed<'_>) -> (TaskId, RunId) {
    let (task, _) = core.create_task(s.words).unwrap();
    let mut b = begin(core, d, &task.id, s.branch);
    if !s.answer.is_empty() {
        say(core, &mut b, s.answer);
    }
    if let Some(cost) = s.cost {
        let now = core.now();
        b.on(core, AgentEvent::Usage(Reported { input_tokens: 100, output_tokens: 50, cached_tokens: 0, cost_usd: Some(cost), model: None, context_tokens: None, context_window: None }), now).unwrap();
    }
    finish(core, &mut b, s.outcome, s.changes);
    place(core, &b.run.id, s.ended - 60_000, Some(s.ended));
    (task.id, b.run.id.clone())
}

fn window(core: &Core, from: i64, to: i64) -> neko_protocol::Briefing {
    core.briefing(Some(from), Some(to), env(to)).unwrap()
}

#[test]
fn the_window_includes_its_start_and_leaves_out_its_end() {
    let d = dir();
    let mut core = open(&d);
    let (from, to) = (at(2026, 11, 26, 18, 0), at(2026, 11, 27, 8, 0));
    for (words, ended) in [("before", from - 1), ("at the start", from), ("inside", from + HOUR), ("last millisecond", to - 1), ("at the end", to)] {
        seed(&mut core, &d, Seed::done(words, "Done.", ended));
    }
    let b = window(&core, from, to);
    let titles: Vec<&str> = b.finished.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, ["last millisecond", "inside", "at the start"], "start inclusive, end exclusive, newest first");
    assert_eq!(b.totals.finished, 3);
    // Two windows that meet share an edge: nothing is shown twice and nothing is lost.
    let next = window(&core, to, to + HOUR);
    assert_eq!(next.finished.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["at the end"]);
}

#[test]
fn the_first_window_is_the_last_24_hours_and_marking_read_moves_it() {
    let d = dir();
    let mut core = open(&d);
    let now = at(2026, 11, 27, 8, 0);
    seed(&mut core, &d, Seed::done("yesterday evening", "Done.", now - 13 * HOUR));
    seed(&mut core, &d, Seed::done("two days ago", "Done.", now - 40 * HOUR));
    let b = core.briefing(None, None, env(now)).unwrap();
    assert_eq!((b.from_ms, b.to_ms, b.first, b.read_ms), (now - 24 * HOUR, now, true, None));
    assert_eq!(b.finished.len(), 1);
    assert_eq!(b.finished[0].title, "yesterday evening");

    let (after, _) = core.mark_briefing_read(env(now)).unwrap();
    assert_eq!((after.from_ms, after.first, after.read_ms), (now, false, Some(now)));
    assert!(after.finished.is_empty() && after.is_empty(), "read: nothing new");
    seed(&mut core, &d, Seed::done("this morning", "Done.", now + HOUR));
    let later = core.briefing(None, None, env(now + 2 * HOUR)).unwrap();
    assert_eq!((later.from_ms, later.finished.len()), (now, 1), "the window starts where the person last read");
    assert_eq!(later.finished[0].title, "this morning");
    // The read mark survives a restart.
    drop(core);
    let core = open(&d);
    assert_eq!(core.store.briefing_read_ms().unwrap(), Some(now));
}

#[test]
fn finished_shows_each_way_a_task_ends_with_its_first_sentence() {
    let d = dir();
    let mut core = open(&d);
    let t0 = at(2026, 11, 27, 0, 0);
    let (done, run) = seed(&mut core, &d, Seed { changes: Some(Changes { files: 3, added: 40, removed: 5 }), branch: Some("neko/abc"), ..Seed::done("Rename the label", "**Renamed** the label in `Sidebar`. Then I ran the tests.", t0 + HOUR) });
    seed(&mut core, &d, Seed { outcome: Outcome::Failed(Failure::Crashed { code: Some(3), words: "fatal".into() }), ..Seed::done("Crash", "", t0 + 2 * HOUR) });
    seed(&mut core, &d, Seed { outcome: Outcome::Stopped, ..Seed::done("Stopped by hand", "I got as far as the tests.", t0 + 3 * HOUR) });
    seed(&mut core, &d, Seed::done("Silent", "", t0 + 4 * HOUR));
    seed(&mut core, &d, Seed::done("Fenced", "```sh\ncargo test\n```", t0 + 5 * HOUR));
    // Interrupted: the daemon went down while it ran.
    let (cut, _) = core.create_task("Cut off").unwrap();
    let b = begin(&mut core, &d, &cut.id, None);
    place(&core, &b.run.id, t0 + 5 * HOUR, None);
    drop(b);
    core.store.interrupt_leftovers(t0 + 6 * HOUR).unwrap();

    let b = window(&core, t0, t0 + 24 * HOUR);
    assert_eq!(b.finished.len(), 6);
    let by = |title: &str| b.finished.iter().find(|t| t.title == title).unwrap_or_else(|| panic!("no {title}: {:?}", b.finished.iter().map(|t| &t.title).collect::<Vec<_>>()));
    let t = by("Rename the label");
    assert_eq!((t.end, t.task.clone()), (FinishedEnd::Done, done));
    assert_eq!(t.answer.as_deref(), Some("Renamed the label in Sidebar."), "plain text, the first sentence");
    assert_eq!(t.changes, Some(Changes { files: 3, added: 40, removed: 5 }));
    assert_eq!(t.branch.as_deref(), Some("neko/abc"));
    assert_eq!(t.run, run);
    assert!(t.cat.is_empty() && t.cat_name.is_empty(), "a thread has no cat to lead it (protocol 14)");
    assert_eq!(by("Crash").end, FinishedEnd::Failed);
    assert_eq!(by("Stopped by hand").end, FinishedEnd::Stopped);
    assert_eq!(by("Stopped by hand").answer.as_deref(), Some("I got as far as the tests."));
    assert_eq!(by("Cut off").end, FinishedEnd::Interrupted);
    assert_eq!(by("Silent").answer, None, "a run that said nothing has no sentence");
    assert_eq!(by("Fenced").answer.as_deref(), Some("Code"));
    assert_eq!(b.totals.finished, 6);
    assert_eq!((b.totals.files, b.totals.added, b.totals.removed), (3, 40, 5));
}

#[test]
fn a_task_that_waits_for_the_person_is_in_needs_you_and_not_in_finished() {
    let d = dir();
    let mut core = open(&d);
    let t0 = at(2026, 11, 27, 0, 0);
    let (asked, _) = seed(&mut core, &d, Seed::done("Rename it", "I found four places.\n\nShould I rename all four?", t0 + HOUR));
    // A request waiting in a run.
    let (waiting, _) = core.create_task("Run the tests").unwrap();
    let mut b = begin(&mut core, &d, &waiting.id, None);
    let now = core.now();
    b.on(&mut core, AgentEvent::PermissionAsked { id: "cli-1".into(), tool: "Bash".into(), action: Action::Command { command: "cargo test".into() }, tool_use: None }, now).unwrap();
    place(&core, &b.run.id, t0 + 2 * HOUR, None);
    core.store.connection().execute("UPDATE permissions SET asked_ms = ?1", [t0 + 2 * HOUR]).unwrap();
    seed(&mut core, &d, Seed::done("Plain done", "All good.", t0 + 3 * HOUR));
    let (working, _) = core.create_task("Still going").unwrap();
    let w = begin(&mut core, &d, &working.id, None);
    place(&core, &w.run.id, t0 + 4 * HOUR, None);

    let b = window(&core, t0, t0 + 24 * HOUR);
    let needs: Vec<(&str, &str)> = b.needs.iter().map(|n| (n.title.as_str(), n.cat_name.as_str())).collect();
    assert_eq!(needs.len(), 2, "{needs:?}");
    assert!(b.needs.iter().any(|n| n.task == asked) && b.needs.iter().any(|n| n.task == waiting.id));
    assert_eq!(b.finished.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["Plain done"], "what waits is not also finished");
    assert_eq!(b.working.iter().map(|w| (w.title.as_str(), w.since_ms)).collect::<Vec<_>>(), [("Still going", t0 + 4 * HOUR)]);
    let c = b.counts();
    assert_eq!((c.needs_you, c.finished, c.working, c.news), (2, 1, 1, true));
    assert_eq!(c.headline(), "1 finished, 2 needs you");
}

#[test]
fn a_request_that_has_long_waited_is_not_news_after_the_person_read() {
    let d = dir();
    let mut core = open(&d);
    let t0 = at(2026, 11, 27, 0, 0);
    let (waiting, _) = core.create_task("Run the tests").unwrap();
    let mut b = begin(&mut core, &d, &waiting.id, None);
    let now = core.now();
    b.on(&mut core, AgentEvent::PermissionAsked { id: "cli-1".into(), tool: "Bash".into(), action: Action::Command { command: "cargo test".into() }, tool_use: None }, now).unwrap();
    core.store.connection().execute("UPDATE permissions SET asked_ms = ?1", [t0 + HOUR]).unwrap();
    let before = window(&core, t0, t0 + 3 * HOUR).counts();
    assert!(before.news, "it began to wait inside the window");
    let after = window(&core, t0 + 2 * HOUR, t0 + 3 * HOUR);
    assert_eq!(after.needs.len(), 1, "still listed: the person must act on it");
    assert!(after.is_empty() && !after.counts().news, "but it is not news");
    assert_eq!(after.counts().needs_you, 1);
}

#[test]
fn undone_since_is_noticed_and_those_files_are_not_counted() {
    let d = dir();
    let mut core = open(&d);
    let t0 = at(2026, 11, 27, 0, 0);
    let ch = Changes { files: 2, added: 10, removed: 1 };
    let (_, kept) = seed(&mut core, &d, Seed { changes: Some(ch), ..Seed::done("Kept", "Done.", t0 + HOUR) });
    let (undone_task, undone) = seed(&mut core, &d, Seed { changes: Some(Changes { files: 4, added: 20, removed: 2 }), ..Seed::done("Undone", "Done.", t0 + 2 * HOUR) });
    let applied = |s: UndoState| RunUndo { state: s, why: None, changes: Some(Changes { files: 4, added: 20, removed: 2 }), committed: false };
    let applied_kept = RunUndo { state: UndoState::Applied, why: None, changes: Some(ch), committed: false };
    core.store.set_run_undo(&kept, &applied_kept, t0).unwrap();
    core.store.set_run_undo(&undone, &applied(UndoState::Applied), t0).unwrap();
    let b = window(&core, t0, t0 + 24 * HOUR);
    let undo_of = |b: &neko_protocol::Briefing, id: &TaskId| b.finished.iter().find(|t| &t.task == id).unwrap().offer.clone();
    assert_eq!(undo_of(&b, &undone_task).action, Some((UndoAction::Undo, true)), "Undo is offered where Neko offers it");
    assert!(undo_of(&b, &undone_task).review && !undo_of(&b, &undone_task).undone);
    assert_eq!((b.totals.files, b.totals.added), (6, 30));

    core.store.set_run_undo(&undone, &applied(UndoState::Undone), t0).unwrap();
    let b = window(&core, t0, t0 + 24 * HOUR);
    let o = undo_of(&b, &undone_task);
    assert!(o.undone, "undone since");
    assert_eq!(o.action, Some((UndoAction::Redo, true)));
    assert_eq!((b.totals.files, b.totals.added, b.totals.removed), (2, 10, 1), "an undone run's files are not in the folder");
    assert_eq!(b.totals.finished, 2);
}

#[test]
fn totals_carry_a_cost_only_when_one_was_reported() {
    let d = dir();
    let mut core = open(&d);
    let t0 = at(2026, 11, 27, 0, 0);
    seed(&mut core, &d, Seed::done("Free", "Done.", t0 + HOUR));
    let b = window(&core, t0, t0 + 24 * HOUR);
    assert_eq!((b.totals.cost_usd, b.totals.api_equivalent), (None, false), "no cost reported: none said");
    seed(&mut core, &d, Seed { cost: Some(0.25), ..Seed::done("Paid", "Done.", t0 + 2 * HOUR) });
    seed(&mut core, &d, Seed { cost: Some(0.5), ..Seed::done("Paid again", "Done.", t0 + 3 * HOUR) });
    seed(&mut core, &d, Seed { cost: Some(9.0), ..Seed::done("Outside", "Done.", t0 + 30 * HOUR) });
    let b = window(&core, t0, t0 + 24 * HOUR);
    assert_eq!(b.totals.cost_usd, Some(0.75), "the runs that ended in the window, as reported");
    assert!(b.totals.api_equivalent, "a subscription login: not money charged");
}

#[test]
fn every_round_outcome_has_a_line_and_a_run_now_has_none() {
    let d = dir();
    let mut core = open(&d);
    let t0 = at(2026, 11, 27, 0, 0);
    let e = env(t0);
    let (r, _) = core.create_round("Docs check", "Check", Some("cat_mochi"), false, None, &Schedule::Daily { at: "01:00".into() }, true, e).unwrap();
    let (task, _) = seed_task_for_round(&mut core, &d, "Docs check", t0 + 2 * HOUR);
    let line = |id: &str, at_ms: i64, due: Option<i64>, kind: &str, task: Option<&TaskId>, reason: Option<&str>| {
        core.store
            .connection()
            .execute("INSERT INTO round_runs (id, round_id, due_ms, at_ms, kind, task_id, reason, ended) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)", rusqlite::params![id, r.id, due, at_ms, kind, task.map(|t| &t.0), reason])
            .unwrap();
    };
    line("a", t0 + HOUR, Some(t0 + HOUR), "started", Some(&task), None);
    line("b", t0 + 2 * HOUR, Some(t0 + 2 * HOUR), "skipped", None, Some("The last run is still working."));
    line("c", t0 + 3 * HOUR, Some(t0 - 5 * HOUR), "missed", None, Some("Neko was not running, or this Mac was asleep"));
    line("d", t0 + 4 * HOUR, None, "started", Some(&task), None);
    line("e", t0 - HOUR, Some(t0 - HOUR), "skipped", None, Some("before the window"));
    // The brake.
    let mut row = core.store.get_round_row(&r.id).unwrap().unwrap();
    row.enabled = false;
    core.store.write_round(&row, "round_braked", t0 + 5 * HOUR).unwrap();

    let b = window(&core, t0, t0 + 24 * HOUR);
    let lines: Vec<(RoundOutcome, Option<&str>)> = b.rounds.iter().map(|l| (l.outcome, l.reason.as_deref())).collect();
    assert_eq!(
        lines,
        [
            (RoundOutcome::Braked, Some(BRAKE_LINE)),
            (RoundOutcome::Missed, Some("Neko was not running, or this Mac was asleep")),
            (RoundOutcome::Skipped, Some("The last run is still working.")),
            (RoundOutcome::Ran, None),
        ],
        "newest first; a Run now is the person's own doing, and a line from before the window is not shown"
    );
    let ran = b.rounds.last().unwrap();
    assert_eq!((ran.name.as_str(), ran.cat_name.as_str(), ran.task.as_ref(), ran.task_title.as_deref()), ("Docs check", "mochi", Some(&task), Some("Docs check")));
    assert_eq!(ran.task_status, Some(TaskStatus::Done));
    assert_eq!((b.rounds_ran, b.rounds_not_run), (1, 3));
    assert_eq!(b.counts().headline(), "1 finished", "a finished task leads the headline");
}

fn seed_task_for_round(core: &mut Core, d: &Gone, title: &str, ended: i64) -> (TaskId, RunId) {
    seed(core, d, Seed::done(title, "Checked the links.", ended))
}

#[test]
fn a_long_night_is_cut_to_the_biggest_lists_and_counts_what_was_left_out() {
    let d = dir();
    let mut core = open(&d);
    let t0 = at(2026, 11, 27, 0, 0);
    for i in 0..(MAX_FINISHED as i64 + 5) {
        seed(&mut core, &d, Seed::done(&format!("Task {i}"), "Done.", t0 + i * 60_000 + 1));
    }
    let b = window(&core, t0, t0 + 24 * HOUR);
    assert_eq!((b.finished.len(), b.totals.finished, b.finished_more()), (MAX_FINISHED, MAX_FINISHED as u32 + 5, 5));
    assert_eq!(b.finished[0].title, format!("Task {}", MAX_FINISHED + 4), "the newest are kept");
}

#[test]
fn an_empty_window_is_empty_and_has_the_next_morning() {
    let d = dir();
    let core = open(&d);
    let now = at(2026, 11, 27, 9, 0);
    let b = core.briefing(None, None, env(now)).unwrap();
    assert!(b.is_empty() && b.finished.is_empty() && b.rounds.is_empty() && b.needs.is_empty());
    assert_eq!(b.counts().headline(), "");
    assert_eq!(b.next_morning_ms, Some(at(2026, 11, 28, 8, 0)), "the default morning time is 08:00");
}

#[test]
fn a_store_from_before_the_briefing_opens_with_its_defaults_and_needs_no_migration() {
    let d = dir();
    let path = d.0.join("neko.sqlite3");
    {
        // Rounds' store: version 7, the newest before the Briefing.
        let mut core = Core::open(&path, Box::new(FixedClock::new(1_764_000_000_000, 1000)), Box::new(SequentialIds::default())).unwrap();
        assert_eq!(core.store.version().unwrap(), SCHEMA_VERSION);
        seed(&mut core, &d, Seed::done("Old", "Done.", 1_764_000_100_000));
        core.store.connection().execute("DELETE FROM settings WHERE key LIKE 'briefing_%'", []).unwrap();
    }
    assert_eq!(SCHEMA_VERSION, 20, "the briefing keeps its marks in `settings`: no table, no migration (the eighth migration is plugins', the ninth threads', the tenth subagents', the eleventh new thread's, the twelfth the composer's, the thirteenth pin, settle and archive's, the fourteenth search's, the fifteenth kittens' place, order and drafts, the sixteenth questions going up, the seventeenth Cattains, the eighteenth the thread that made a Cattain, the nineteenth the crew, the twentieth timers and watches)");
    let store = Store::open(&path).unwrap();
    assert_eq!((store.briefing_read_ms().unwrap(), store.briefing_settings().unwrap()), (None, BriefingSettings::default()));
}

// ---- the morning ----

fn core_with_news(d: &Gone, t0: i64) -> Core {
    let mut core = open(d);
    seed(&mut core, d, Seed::done("Overnight", "Done.", t0));
    core
}

#[test]
fn the_morning_posts_once_a_day_and_not_before_its_time() {
    let d = dir();
    let t0 = at(2026, 11, 27, 3, 0);
    let mut core = core_with_news(&d, t0);
    // First look at 07:00: nothing is due yet, and the next time is 08:00.
    let m = core.morning(env(at(2026, 11, 27, 7, 0)), true).unwrap();
    assert_eq!((m.due, m.wake_at_ms), (None, Some(at(2026, 11, 27, 8, 0))));
    let m = core.morning(env(at(2026, 11, 27, 7, 59)), true).unwrap();
    assert_eq!(m.due, None, "not before its time");
    let m = core.morning(env(at(2026, 11, 27, 8, 0)), true).unwrap();
    let due = m.due.expect("due at 08:00");
    assert_eq!((due.finished, due.needs_you), (1, 0));
    assert_eq!(m.wake_at_ms, Some(at(2026, 11, 28, 8, 0)));
    // Looked at again the same day, any number of times: no second one.
    for h in [8, 9, 12, 20] {
        assert_eq!(core.morning(env(at(2026, 11, 27, h, 5)), true).unwrap().due, None, "once a day");
    }
    // The next day it is due again when there is news.
    seed(&mut core, &d, Seed::done("Next night", "Done.", at(2026, 11, 28, 2, 0)));
    assert!(core.morning(env(at(2026, 11, 28, 8, 1)), true).unwrap().due.is_some());
}

#[test]
fn a_mac_that_slept_posts_once_on_wake_when_it_is_still_the_same_day_and_never_a_stack() {
    let d = dir();
    let mut core = core_with_news(&d, at(2026, 11, 27, 3, 0));
    core.morning(env(at(2026, 11, 27, 7, 0)), true).unwrap();
    // Asleep through 08:00, awake at 10:30 the same day.
    let m = core.morning(env(at(2026, 11, 27, 10, 30)), true).unwrap();
    assert!(m.due.is_some(), "post once on wake");
    assert_eq!(core.morning(env(at(2026, 11, 27, 10, 31)), true).unwrap().due, None);
    // Asleep for three days: the mornings that passed are not posted, and none stacks up.
    seed(&mut core, &d, Seed::done("Third night", "Done.", at(2026, 11, 30, 2, 0)));
    let m = core.morning(env(at(2026, 11, 30, 10, 0)), true).unwrap();
    assert_eq!(m.due.map(|d| d.finished), Some(1), "today's, once: the two mornings that passed are not stacked");
    assert_eq!(core.morning(env(at(2026, 11, 30, 10, 1)), true).unwrap().due, None);
    let d2 = dir();
    let mut core = core_with_news(&d2, at(2026, 11, 27, 3, 0));
    core.morning(env(at(2026, 11, 27, 7, 0)), true).unwrap();
    // Wakes at 23:00 on the 29th: the 29th's 08:00 is today's, so once; the 28th's is gone.
    seed(&mut core, &d2, Seed::done("Second night", "Done.", at(2026, 11, 29, 2, 0)));
    let m = core.morning(env(at(2026, 11, 29, 23, 0)), true).unwrap();
    assert!(m.due.is_some());
    assert_eq!(core.morning(env(at(2026, 11, 29, 23, 5)), true).unwrap().due, None);
    // A morning that passed on an earlier day, noticed the next day before the time: passed over, nothing posted.
    let d3 = dir();
    let mut core = core_with_news(&d3, at(2026, 11, 27, 3, 0));
    core.morning(env(at(2026, 11, 27, 7, 0)), true).unwrap();
    let m = core.morning(env(at(2026, 11, 28, 7, 0)), true).unwrap();
    assert_eq!(m.due, None, "yesterday's 08:00 passed on another day: not posted on wake");
    assert_eq!(m.wake_at_ms, Some(at(2026, 11, 28, 8, 0)));
}

#[test]
fn with_no_screen_the_notification_waits_for_one_the_same_day() {
    let d = dir();
    let mut core = core_with_news(&d, at(2026, 11, 27, 3, 0));
    core.morning(env(at(2026, 11, 27, 7, 0)), true).unwrap();
    let m = core.morning(env(at(2026, 11, 27, 8, 0)), false).unwrap();
    assert_eq!((m.due, m.wake_at_ms), (None, None), "no screen: no timer, it waits");
    let m = core.morning(env(at(2026, 11, 27, 9, 15)), true).unwrap();
    assert!(m.due.is_some(), "a screen came: posted once");
    assert_eq!(core.morning(env(at(2026, 11, 27, 9, 20)), true).unwrap().due, None);
}

#[test]
fn nothing_is_posted_for_an_empty_window_or_when_the_morning_is_off_or_just_changed() {
    let d = dir();
    let mut core = open(&d);
    core.morning(env(at(2026, 11, 27, 7, 0)), true).unwrap();
    let m = core.morning(env(at(2026, 11, 27, 8, 0)), true).unwrap();
    assert_eq!(m.due, None, "an empty window posts nothing");
    assert_eq!(m.wake_at_ms, Some(at(2026, 11, 28, 8, 0)));
    // Off: no timer at all.
    let d2 = dir();
    let mut core = core_with_news(&d2, at(2026, 11, 27, 3, 0));
    core.set_briefing_settings(&BriefingSettings { morning_on: false, morning_at: "08:00".into() }, env(at(2026, 11, 27, 4, 0))).unwrap();
    let m = core.morning(env(at(2026, 11, 27, 9, 0)), true).unwrap();
    assert_eq!((m.due, m.wake_at_ms), (None, None), "off: nothing and no timer");
    // Changing the time to one that has passed today does not catch it up; one later today still posts.
    core.set_briefing_settings(&BriefingSettings { morning_on: true, morning_at: "08:30".into() }, env(at(2026, 11, 27, 9, 0))).unwrap();
    assert_eq!(core.morning(env(at(2026, 11, 27, 9, 1)), true).unwrap().due, None);
    core.set_briefing_settings(&BriefingSettings { morning_on: true, morning_at: "22:00".into() }, env(at(2026, 11, 27, 9, 2))).unwrap();
    assert_eq!(core.morning(env(at(2026, 11, 27, 21, 59)), true).unwrap().due, None);
    assert!(core.morning(env(at(2026, 11, 27, 22, 0)), true).unwrap().due.is_some());
    // A window the person already read posts nothing.
    let d3 = dir();
    let mut core = core_with_news(&d3, at(2026, 11, 27, 3, 0));
    core.morning(env(at(2026, 11, 27, 7, 0)), true).unwrap();
    core.mark_briefing_read(env(at(2026, 11, 27, 7, 30))).unwrap();
    assert_eq!(core.morning(env(at(2026, 11, 27, 8, 0)), true).unwrap().due, None);
}

#[test]
fn the_morning_on_daylight_saving_days_comes_once_by_the_wall_clock() {
    // New York: clocks jump forward on 2027-03-14 (02:00 -> 03:00) and back on 2026-11-01 (02:00 -> 01:00).
    let ny = Rules::us(-5 * 3600);
    let cases: [(&str, (i64, u32, u32), i64); 3] = [
        // 02:30 does not exist on the spring day: it comes once, at 03:30 by the new clock (07:30 UTC).
        ("02:30", (2027, 3, 14), at(2027, 3, 14, 7, 30)),
        // 01:30 happens twice on the autumn day: once, at its first occurrence (05:30 UTC).
        ("01:30", (2026, 11, 1), at(2026, 11, 1, 5, 30)),
        // An ordinary 08:00 on the spring day is 12:00 UTC (EDT).
        ("08:00", (2027, 3, 14), at(2027, 3, 14, 12, 0)),
    ];
    for (time, (y, mo, dd), due_utc) in cases {
        let d = dir();
        let mut core = core_with_news(&d, due_utc - 6 * HOUR);
        let e = |now: i64| Env { now, zone: &ny };
        core.set_briefing_settings(&BriefingSettings { morning_on: true, morning_at: time.into() }, e(due_utc - 3 * HOUR)).unwrap();
        assert_eq!(core.morning(e(due_utc - 2 * HOUR), true).unwrap().due, None, "{time} on {y}-{mo}-{dd}: not yet");
        let m = core.morning(e(due_utc), true).unwrap();
        assert!(m.due.is_some(), "{time} on {y}-{mo}-{dd}: due at {due_utc}");
        for extra in [HOUR, 2 * HOUR, 3 * HOUR] {
            assert_eq!(core.morning(e(due_utc + extra), true).unwrap().due, None, "{time} on {y}-{mo}-{dd}: once, not again an hour later");
        }
    }
}

/// Writes a Cattain's DIGEST.md as its agent does (on disk) and lets Neko see it; returns when Neko saw it change.
fn write_digest(core: &mut Core, cat: &str, words: &str) -> i64 {
    std::fs::write(core.cattain_folder(cat).join(neko_protocol::cats::DIGEST), words).unwrap();
    core.sync_cattain(cat).unwrap();
    core.cattain_file(cat, neko_protocol::cats::DIGEST).unwrap().0.changed_ms.expect("changed")
}

#[test]
fn only_the_cattains_whose_digest_changed_in_the_window_are_there_with_their_needs_you_and_new_lines() {
    let d = dir();
    let mut core = open(&d);
    let miso1 = write_digest(&mut core, "cat_miso", "## Needs you\n- Nothing.\n\n## New\n- Fix test one: done.\n");
    let mochi = write_digest(&mut core, "cat_mochi", "## Needs you\n- Decide the retry limit.\n\n## New\n- Docs link fixed.\n\n## In progress\n- The backup port.\n");
    let from = mochi + 1;
    // After the person read it, only miso writes again: four new lines, of which three are quoted.
    let miso2 = write_digest(&mut core, "cat_miso", "# miso\n\n## Needs you\n- Merge the three branches?\n\n## New\n- Fix test one: done.\n- Fix test two: done.\n- Fix test three: done.\n- Fix docs: done.\n\n## In progress\n- Nothing.\n");
    assert!(miso1 < mochi && mochi < miso2);
    let b = window(&core, from, miso2 + 1);
    assert_eq!(b.cattains.len(), 1, "only the digest that changed in the window: {:?}", b.cattains);
    let c = &b.cattains[0];
    assert_eq!((c.cat.as_str(), c.name.as_str(), c.changed_ms), ("cat_miso", "miso", miso2));
    assert_eq!(c.needs, vec!["Merge the three branches?"]);
    assert_eq!((c.new.as_slice(), c.new_more), (&["Fix test one: done.".to_owned(), "Fix test two: done.".to_owned(), "Fix test three: done.".to_owned()][..], 1));
    let counts = b.counts();
    assert!(counts.news && counts.digests == 1, "a digest that changed is news");
    assert_eq!(counts.headline(), "1 digest updated");
    // The whole day has both, the most recent first; a window before either has none and no news from them.
    let all = window(&core, miso1, miso2 + 1);
    assert_eq!(all.cattains.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["miso", "mochi"]);
    assert_eq!(all.cattains[1].needs, vec!["Decide the retry limit."]);
    let before = window(&core, miso1 - 10 * HOUR, miso1);
    assert!(before.cattains.is_empty() && !before.counts().news);
    // A digest emptied has nothing to say.
    let emptied = write_digest(&mut core, "cat_miso", "   \n");
    assert!(window(&core, emptied, emptied + 1).cattains.is_empty());
}

#[test]
fn a_cattains_own_turn_is_told_by_its_digest_not_as_a_finished_row() {
    let d = dir();
    let mut core = open(&d);
    let (own, _, _) = core.cattain_thread("cat_miso").unwrap();
    let mut b = begin(&mut core, &d, &own.id, None);
    say(&mut core, &mut b, "I started three crew threads.");
    finish(&mut core, &mut b, Outcome::Done, None);
    let ended = core.store.runs_of(&own.id).unwrap().last().unwrap().ended_ms.unwrap();
    let (plain, _) = seed(&mut core, &d, Seed::done("Fix the login test", "Fixed it.", ended + 1000));
    // No digest yet: miso's turn is a finished row as any thread's is.
    let w = window(&core, ended - HOUR, ended + HOUR);
    assert!(w.finished.iter().any(|t| t.task == own.id) && w.cattains.is_empty());
    let changed = write_digest(&mut core, "cat_miso", "## New\n- Started three crew threads.\n");
    let w = window(&core, ended - HOUR, changed.max(ended) + HOUR);
    assert_eq!(w.cattains.len(), 1);
    assert_eq!(w.finished.iter().map(|t| &t.task).collect::<Vec<_>>(), [&plain], "miso's turn is in its block; the other thread stays");
    assert_eq!(w.totals.finished, 1);
}

#[test]
fn a_cattain_made_by_asking_is_a_new_cattain_line_once_in_the_window_it_was_made() {
    let d = dir();
    let mut core = open(&d);
    let (asking, _) = core.create_task("Make a Cattain for the iOS work").unwrap();
    let by_hand = core.create_cat("kinako", None, Some("Docs"), None).unwrap().0;
    let yuzu = match core.make_cattain(&asking.id, &neko_core::cats::MakeAsk { purpose: "iOS work", ..Default::default() }).unwrap() {
        neko_core::cats::Made::New { cat, .. } => cat,
        other => panic!("{other:?}"),
    };
    let w = window(&core, by_hand.created_ms, yuzu.created_ms + 1);
    assert_eq!(w.new_cattains.len(), 1, "made by hand is the person's own doing: {:?}", w.new_cattains);
    let n = &w.new_cattains[0];
    assert_eq!((n.name.as_str(), n.purpose.as_deref(), n.made_ms, &n.made_in), ("yuzu", Some("iOS work"), yuzu.created_ms, &asking.id));
    assert!(w.counts().news && w.counts().new_cattains == 1);
    assert!(window(&core, yuzu.created_ms + 1, yuzu.created_ms + HOUR).new_cattains.is_empty(), "once: the next window has it no more");
    // Undone at once, it is not news.
    core.undo_make_cattain(&yuzu.id).unwrap();
    assert!(window(&core, by_hand.created_ms, yuzu.created_ms + 1).new_cattains.is_empty());
}
