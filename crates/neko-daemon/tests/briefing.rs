//! The Briefing in the real daemon, with the fake CLI as `claude` (docs/knowledge/briefing.md). Time is handed in through the debug
//! clock (`NEKO_DEBUG_CLOCK=1`: a debug build only) and the zone is fixed with `TZ=UTC`, so a test makes the morning come without
//! waiting. Every wait has a deadline.

mod support;
use std::time::{Duration, Instant};

use neko_protocol::briefing::{BriefingSettings, FinishedEnd};
use neko_protocol::client::Client;
use neko_protocol::*;
use support::*;

const T: Duration = Duration::from_secs(30);
const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;

fn daemon(name: &str) -> Daemon {
    Daemon::start_with(temp_dir(name), &[("NEKO_DEBUG_CLOCK", "1"), ("TZ", "UTC")])
}

fn screen(d: &Daemon) -> Client {
    let mut c = d.client();
    assert!(matches!(c.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
    c
}

fn wait_for<R>(what: &str, mut f: impl FnMut() -> Option<R>) -> R {
    let end = Instant::now() + T;
    loop {
        if let Some(r) = f() {
            return r;
        }
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn briefing(c: &mut Client) -> Briefing {
    match c.request(Request::GetBriefing { from_ms: None, to_ms: None }).unwrap() {
        Reply::Briefing { briefing } => briefing,
        r => panic!("get_briefing: {r:?}"),
    }
}

fn task(c: &mut Client, words: &str) -> Task {
    match c.request(Request::CreateTask { crew: None, words: words.into(), workspace: None, in_folder: false, from_branch: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("create_task: {r:?}"),
    }
}

fn status_of(c: &mut Client, id: &TaskId) -> TaskStatus {
    match c.request(Request::GetTask { task: id.clone() }).unwrap() {
        Reply::Task { task } => task.status,
        r => panic!("get_task: {r:?}"),
    }
}

fn finished_task(c: &mut Client, words: &str) -> Task {
    let t = task(c, words);
    wait_for("the task to finish", || Some(()).filter(|_| status_of(c, &t.id) == TaskStatus::Done));
    t
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

fn clock(c: &mut Client, offset_ms: i64) {
    assert_eq!(c.request(Request::DebugClock { offset_ms: Some(offset_ms) }).unwrap(), Reply::Ok);
}

/// "HH:MM" (UTC) `ms` from now.
fn utc_in(ms: i64) -> String {
    let t = (now_ms() + ms).div_euclid(60_000).rem_euclid(24 * 60);
    format!("{:02}:{:02}", t / 60, t % 60)
}

/// Waits up to `limit` for a `BriefingDue` event, if one comes.
fn due_within(c: &mut Client, limit: Duration) -> Option<BriefingCounts> {
    let end = Instant::now() + limit;
    while Instant::now() < end {
        if let Some(ServerMessage::Event { change: Change::BriefingDue { counts }, .. }) = c.next_event(Duration::from_millis(100)).unwrap() {
            return Some(counts);
        }
    }
    None
}

#[test]
fn what_finished_is_in_the_briefing_with_its_first_sentence_and_marking_it_read_empties_it() {
    let d = daemon("briefing-basic");
    let mut c = screen(&d);
    let b = briefing(&mut c);
    assert!(b.is_empty() && b.first && b.read_ms.is_none(), "{b:?}");
    let t = finished_task(&mut c, "Check the docs [fake:plain]");
    let b = wait_for("the briefing to list it", || Some(briefing(&mut c)).filter(|b| !b.finished.is_empty()));
    let item = &b.finished[0];
    assert_eq!((item.task.clone(), item.end, item.cat_name.as_str()), (t.id.clone(), FinishedEnd::Done, ""), "a thread has no cat to name");
    assert_eq!(item.answer.as_deref(), Some("Hello from the fake Claude Code."), "plain text, the first sentence");
    assert!(b.totals.cost_usd.is_some() && b.totals.api_equivalent, "the fake reports a cost, as Claude Code does: {:?}", b.totals);
    assert_eq!(b.counts().headline(), "1 finished");

    // Marking it read moves the window to now; every screen hears the new briefing.
    let mut other = screen(&d);
    let Reply::Briefing { briefing: after } = c.request(Request::MarkBriefingRead).unwrap() else { panic!("mark") };
    assert!(after.is_empty() && after.read_ms.is_some() && !after.first && after.from_ms >= b.to_ms);
    let heard = wait_for("briefing_changed", || match other.next_event(Duration::from_millis(100)).unwrap() {
        Some(ServerMessage::Event { change: Change::BriefingChanged { briefing }, .. }) if briefing.is_empty() => Some(briefing),
        _ => None,
    });
    assert_eq!(heard.read_ms, after.read_ms);
    assert!(briefing(&mut c).finished.is_empty());
}

#[test]
fn the_briefing_is_pushed_when_its_content_changes_and_not_when_it_does_not() {
    let d = daemon("briefing-push");
    let mut c = screen(&d);
    let mut watcher = screen(&d);
    let t = task(&mut c, "Look at the docs [fake:plain]");
    // A finished task changes the briefing: it arrives whole.
    let b = wait_for("a briefing with the task", || match watcher.next_event(Duration::from_millis(100)).unwrap() {
        Some(ServerMessage::Event { change: Change::BriefingChanged { briefing }, .. }) if !briefing.finished.is_empty() => Some(briefing),
        _ => None,
    });
    assert_eq!(b.finished[0].task, t.id);
    // Nothing changed since: no more pushes. (Only the clock moved; the moment the briefing was made does not count.)
    let mut more = 0;
    let end = Instant::now() + Duration::from_millis(800);
    while Instant::now() < end {
        if let Some(ServerMessage::Event { change: Change::BriefingChanged { .. }, .. }) = watcher.next_event(Duration::from_millis(100)).unwrap() {
            more += 1;
        }
    }
    assert_eq!(more, 0, "an unchanged briefing is not pushed again");
}

#[test]
fn a_question_and_a_request_are_in_needs_you_first_and_never_also_finished() {
    let d = daemon("briefing-needs");
    let mut c = screen(&d);
    let q = task(&mut c, "Rename the button [fake:question]");
    wait_for("the question", || Some(()).filter(|_| status_of(&mut c, &q.id) == TaskStatus::Asked));
    let waiting = task(&mut c, "Run the tests [fake:permission]");
    let b = wait_for("two things waiting", || Some(briefing(&mut c)).filter(|b| b.needs.len() == 2));
    assert!(b.finished.is_empty(), "what waits for the person is not finished");
    assert!(b.needs.iter().any(|n| n.task == q.id) && b.needs.iter().any(|n| n.task == waiting.id));
    assert_eq!(b.counts().headline(), "2 needs you");
    // The same items Needs you lists, whole, so the app answers them with the cards it already has.
    let Reply::Needs { items } = c.request(Request::ListNeeds).unwrap() else { panic!("needs") };
    assert_eq!(b.needs, items);
    assert_eq!(b.working.len(), 0, "a run waiting on a request is in Needs you, not still working");
}

#[test]
fn the_morning_settings_are_kept_validated_and_heard() {
    let d = daemon("briefing-settings");
    let mut c = screen(&d);
    let mut other = screen(&d);
    let Reply::BriefingSettings { settings } = c.request(Request::GetBriefingSettings).unwrap() else { panic!("get") };
    assert_eq!(settings, BriefingSettings { morning_on: true, morning_at: "08:00".into() }, "the defaults");
    let Reply::BriefingSettings { settings } = c.request(Request::SetBriefingSettings { settings: BriefingSettings { morning_on: false, morning_at: "7:05".into() } }).unwrap() else { panic!("set") };
    assert_eq!(settings, BriefingSettings { morning_on: false, morning_at: "07:05".into() }, "kept as HH:MM");
    let heard = wait_for("settings changed", || match other.next_event(Duration::from_millis(100)).unwrap() {
        Some(ServerMessage::Event { change: Change::BriefingSettingsChanged { settings }, .. }) => Some(settings),
        _ => None,
    });
    assert_eq!(heard, settings);
    let bad = c.request(Request::SetBriefingSettings { settings: BriefingSettings { morning_on: true, morning_at: "25:99".into() } }).unwrap();
    assert!(matches!(bad, Reply::Error { code: ErrorCode::BadRequest, .. }), "{bad:?}");
    let Reply::BriefingSettings { settings } = c.request(Request::GetBriefingSettings).unwrap() else { panic!("get") };
    assert_eq!(settings.morning_at, "07:05", "a refused time changes nothing");
    // They survive a restart.
    let mut d = d;
    d.keep_dir = true;
    d.stop();
    let d2 = Daemon::start_with(d.dir.clone(), &[("TZ", "UTC")]);
    let mut c = d2.client();
    let Reply::BriefingSettings { settings } = c.request(Request::GetBriefingSettings).unwrap() else { panic!("get") };
    assert_eq!(settings, BriefingSettings { morning_on: false, morning_at: "07:05".into() });
}

#[test]
fn the_morning_posts_one_notification_with_the_counts_and_no_second_the_same_day() {
    let d = daemon("briefing-morning");
    let mut c = screen(&d);
    finished_task(&mut c, "Check the docs [fake:plain]");
    let q = task(&mut c, "Rename the button [fake:question]");
    wait_for("the question", || Some(()).filter(|_| status_of(&mut c, &q.id) == TaskStatus::Asked));
    // The morning is three hours from now by the scheduler's clock, which is moved to just after it.
    let at = utc_in(3 * HOUR);
    assert!(matches!(c.request(Request::SetBriefingSettings { settings: BriefingSettings { morning_on: true, morning_at: at } }).unwrap(), Reply::BriefingSettings { .. }));
    assert_eq!(due_within(&mut c, Duration::from_millis(800)), None, "not before its time");
    clock(&mut c, 3 * HOUR + 2 * MIN);
    let counts = due_within(&mut c, Duration::from_secs(10)).expect("the morning's notification");
    assert_eq!((counts.finished, counts.needs_you, counts.news), (1, 1, true));
    assert_eq!(counts.headline(), "1 finished, 1 needs you");
    // Later the same morning, and later the same day: nothing more.
    for extra in [4 * MIN, HOUR, 5 * HOUR] {
        clock(&mut c, 3 * HOUR + 2 * MIN + extra);
        assert_eq!(due_within(&mut c, Duration::from_millis(600)), None, "once a day");
    }
}

#[test]
fn an_empty_window_posts_nothing() {
    let d = daemon("briefing-empty");
    let mut c = screen(&d);
    let at = utc_in(3 * HOUR);
    assert!(matches!(c.request(Request::SetBriefingSettings { settings: BriefingSettings { morning_on: true, morning_at: at } }).unwrap(), Reply::BriefingSettings { .. }));
    clock(&mut c, 3 * HOUR + 2 * MIN);
    assert_eq!(due_within(&mut c, Duration::from_secs(2)), None, "nothing happened: nothing is posted");
}

#[test]
fn a_notification_that_comes_due_with_no_screen_waits_for_one_the_same_day() {
    let d = daemon("briefing-wait");
    // No screen is subscribed: this client only asks.
    let mut away = d.client();
    finished_task(&mut away, "Check the docs [fake:plain]");
    let at = utc_in(3 * HOUR);
    assert!(matches!(away.request(Request::SetBriefingSettings { settings: BriefingSettings { morning_on: true, morning_at: at } }).unwrap(), Reply::BriefingSettings { .. }));
    clock(&mut away, 3 * HOUR + 2 * MIN);
    std::thread::sleep(Duration::from_millis(800));
    // A screen connects the same day: the notification that waited is posted, once.
    let mut back = d.client();
    assert!(matches!(back.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
    let counts = due_within(&mut back, Duration::from_secs(10)).expect("the notification that waited for a screen");
    assert_eq!(counts.finished, 1);
    assert_eq!(due_within(&mut back, Duration::from_millis(800)), None);
}

#[test]
fn the_briefing_never_asks_a_model_for_anything() {
    // A daemon whose CLI cannot run at all still derives a briefing from its records: it spawns nothing to write it.
    let dir = temp_dir("briefing-nomodel");
    let d = Daemon::start_with(dir, &[("NEKO_CLAUDE_BIN", "/nonexistent/claude"), ("TZ", "UTC")]);
    let mut c = d.client();
    let b = briefing(&mut c);
    assert!(b.is_empty());
    assert!(matches!(c.request(Request::MarkBriefingRead).unwrap(), Reply::Briefing { .. }));
}

/// A Cattain's own thread, opened, given words: its run writes its digest in its folder (the fake's `write` step, `\n` a new line).
fn cattain_writes(c: &mut Client, cat: &str, digest: &str) -> TaskId {
    let Reply::Task { task: own } = c.request(Request::OpenCattain { cat: cat.into() }).unwrap() else { panic!("open_cattain") };
    let words = format!("Keep your digest [fake:mcp] write DIGEST.md {digest}");
    assert_eq!(c.request(Request::FollowUp { task: own.id.clone(), words, answering: None, mode: None }).unwrap(), Reply::Ok);
    let want = format!("{}\n", digest.replace(r"\n", "\n"));
    wait_for("the Cattain's turn to end with its digest written", || {
        let file = match c.request(Request::GetCattainFile { cat: cat.into(), name: "DIGEST.md".into() }).unwrap() {
            Reply::CattainFile { file } => file,
            r => panic!("{r:?}"),
        };
        Some(()).filter(|_| file.text == want && status_of(c, &own.id) == TaskStatus::Done)
    });
    own.id
}

#[test]
fn the_briefing_reads_the_digest_a_cattain_wrote_and_only_one_that_changed_since_it_was_read() {
    let d = daemon("briefing-digest");
    let mut c = screen(&d);
    let mut other = screen(&d);
    let miso = cattain_writes(&mut c, "cat_miso", r"## Needs you\n- Merge acme#12?\n\n## New\n- Fix login: a PR is open.\n\n## In progress\n- The docs.");
    let b = wait_for("miso's digest in the briefing", || Some(briefing(&mut c)).filter(|b| !b.cattains.is_empty()));
    assert_eq!(b.cattains.len(), 1);
    let block = &b.cattains[0];
    assert_eq!((block.name.as_str(), block.needs.as_slice(), block.new.as_slice()), ("miso", &["Merge acme#12?".to_owned()][..], &["Fix login: a PR is open.".to_owned()][..]));
    assert!(!b.finished.iter().any(|t| t.task == miso), "miso's turn is told by its digest, not as a finished row");
    // Every screen heard it, pushed after the file changed (no request of theirs, no model).
    wait_for("briefing_changed with the digest", || match other.next_event(Duration::from_millis(100)).unwrap() {
        Some(ServerMessage::Event { change: Change::BriefingChanged { briefing }, .. }) if !briefing.cattains.is_empty() => Some(()),
        _ => None,
    });
    cattain_writes(&mut c, "cat_mochi", r"## Needs you\nNothing.\n\n## New\n- The docs link is fixed.");
    let b = wait_for("both digests", || Some(briefing(&mut c)).filter(|b| b.cattains.len() == 2));
    assert_eq!(b.cattains[0].name, "mochi", "the most recent first");
    assert!(b.cattains[0].needs.is_empty(), "\"Nothing.\" is not a line");
    let Reply::Briefing { briefing: read } = c.request(Request::MarkBriefingRead).unwrap() else { panic!("mark") };
    assert!(read.cattains.is_empty() && !read.counts().news);
    // Only miso writes again: only miso's block is news.
    std::thread::sleep(Duration::from_millis(5));
    cattain_writes(&mut c, "cat_miso", r"## Needs you\n- Merge acme#12 and acme#13?\n\n## New\n- Fix docs: done.");
    let b = wait_for("miso's new digest", || Some(briefing(&mut c)).filter(|b| !b.cattains.is_empty()));
    assert_eq!(b.cattains.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["miso"], "mochi's digest did not change since");
    assert_eq!(b.cattains[0].needs, vec!["Merge acme#12 and acme#13?"]);
    assert!(b.counts().news && b.counts().digests == 1);
}
