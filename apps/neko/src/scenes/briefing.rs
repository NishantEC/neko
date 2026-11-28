//! The Briefing's scenes (docs/knowledge/briefing.md): a full morning, only finished work, an empty window, caught up, many
//! workspaces, the Home line, and Settings ▸ Briefing. Placeholder data only.

use neko_protocol::briefing::{Briefing, BriefingRound, BriefingTask, BriefingTotals, BriefingWorking, FinishedEnd, RoundOutcome};
use neko_protocol::undo::{Action, Offer};
use neko_protocol::{BriefingSettings, Changes, GENERAL, Need, NeedsItem, RunId, Task, TaskStatus};
use rune::TextEdit;

use super::{HOUR, permission_item, settings, task, with_workspaces};

/// The sample cat of a sample thread: none for "" (a thread the person started has no cat), else the cat's id.
fn opt(cat: &str) -> Option<String> {
    (!cat.is_empty()).then(|| cat.to_owned())
}

/// Its name, as the daemon says it: empty for no cat.
fn cat_name(s: &App, cat: &str) -> String {
    opt(cat).map(|c| super::cat_name(s, &Some(c))).unwrap_or_default()
}
use crate::state::App;

/// Eight in the morning, on the scenes' day: a briefing is read when the person sits down.
fn morning_clock() -> (i64, i64) {
    (crate::fixed_clock().0 - 6 * HOUR, 0)
}

/// The scenes' morning: Thursday 08:05, the window since yesterday 18:00.
fn in_the_morning(s: &mut App) {
    s.clock = morning_clock;
    (s.now_ms, s.utc_offset_s) = morning_clock();
}

/// `h`:`m` on the day `days` after today.
fn at_time(s: &App, days: i64, h: i64, m: i64) -> i64 {
    s.now_ms - s.now_ms.rem_euclid(86_400_000) + days * 86_400_000 + h * HOUR + m * 60_000
}

fn offer(review: bool, action: Option<(Action, bool)>, undone: bool) -> Offer {
    Offer { review, action, line: None, undone }
}

/// What a finished task says in the briefing, and the task behind it.
struct Done<'a> {
    n: u32,
    cat: &'a str,
    ws: &'a str,
    title: &'a str,
    end: FinishedEnd,
    ended: i64,
    answer: Option<&'a str>,
    changes: Option<Changes>,
    branch: Option<&'a str>,
    offer: Offer,
}

fn finished(s: &App, d: Done<'_>) -> (Task, BriefingTask) {
    let mut t = task(d.n, d.title, 60);
    t.title = d.title.into();
    t.workspace = d.ws.into();
    t.cat = opt(d.cat);
    t.folder = (d.ws != GENERAL).then(|| "/Users/example/acme-api".to_owned());
    t.status = match d.end {
        FinishedEnd::Done => TaskStatus::Done,
        FinishedEnd::Failed => TaskStatus::Failed,
        FinishedEnd::Stopped => TaskStatus::Stopped,
        FinishedEnd::Interrupted => TaskStatus::Interrupted,
    };
    let item = BriefingTask {
        task: t.id.clone(),
        workspace: d.ws.into(),
        title: d.title.into(),
        cat: d.cat.into(),
        cat_name: cat_name(s, d.cat),
        ended_ms: d.ended,
        end: d.end,
        answer: d.answer.map(str::to_owned),
        changes: d.changes,
        branch: d.branch.map(str::to_owned),
        run: RunId(format!("run_{}", d.n)),
        offer: d.offer,
    };
    (t, item)
}

#[allow(clippy::too_many_arguments)]
fn round_line(s: &App, id: &str, name: &str, ws: &str, cat: &str, outcome: RoundOutcome, due: Option<i64>, at: i64, task: Option<&Task>, reason: Option<&str>) -> BriefingRound {
    BriefingRound {
        round: format!("round_{id}"),
        name: name.into(),
        workspace: ws.into(),
        cat: cat.into(),
        cat_name: cat_name(s, cat),
        outcome,
        at_ms: at,
        due_ms: due,
        task: task.map(|t| t.id.clone()),
        task_title: task.map(|t| t.title.clone()),
        task_status: task.map(|t| t.status),
        reason: reason.map(str::to_owned),
    }
}

fn working_task(s: &App, n: u32, cat: &str, ws: &str, title: &str, minutes: i64) -> (Task, BriefingWorking) {
    let mut t = task(n, title, minutes);
    t.title = title.into();
    t.workspace = ws.into();
    t.cat = opt(cat);
    t.status = TaskStatus::Working;
    t.folder = (ws != GENERAL).then(|| "/Users/example/acme-api".to_owned());
    let w = BriefingWorking { task: t.id.clone(), workspace: ws.into(), title: title.into(), cat: cat.into(), cat_name: cat_name(s, cat), since_ms: s.now_ms - minutes * 60_000 };
    (t, w)
}

fn empty_briefing(s: &App) -> Briefing {
    Briefing {
        from_ms: at_time(s, -1, 18, 0),
        to_ms: s.now_ms,
        first: false,
        read_ms: Some(at_time(s, -1, 18, 0)),
        next_morning_ms: Some(at_time(s, 1, 8, 0)),
        needs: Vec::new(),
        finished: Vec::new(),
        rounds: Vec::new(),
        rounds_ran: 0,
        rounds_not_run: 0,
        working: Vec::new(),
        totals: BriefingTotals::default(),
    }
}

/// The settings the daemon has by default: 08:00, on.
fn default_morning(s: &mut App) {
    s.briefing_settings = Some(BriefingSettings::default());
    s.briefing_time = TextEdit::new("08:00");
}

/// A morning with nothing waiting or working: the base of the quiet scenes.
fn quiet_morning(s: &mut App) {
    with_workspaces(s);
    in_the_morning(s);
    default_morning(s);
    s.needs.clear();
    s.tasks.retain(|t| !matches!(t.status, TaskStatus::Working | TaskStatus::Asked | TaskStatus::NeedsYou));
}

/// A full morning. With `workspaces` the activity spreads over acme-api, the marketing site and General (the page then groups by
/// workspace); otherwise it is all in acme-api.
fn full_morning(s: &mut App, workspaces: bool) {
    with_workspaces(s);
    in_the_morning(s);
    default_morning(s);
    let a = "ws_acme";
    let (w, g) = if workspaces { ("ws_web", GENERAL) } else { (a, a) };
    s.tasks.clear();
    let ch = |files, added, removed| Some(Changes { files, added, removed });
    let done = |n, cat, ws, title, ended, answer, changes, branch, o| Done { n, cat, ws, title, end: FinishedEnd::Done, ended, answer: Some(answer), changes, branch, offer: o };
    let (t1, f1) = finished(
        s,
        done(51, "", a, "Rename the Settings button to Preferences across the app", at_time(s, 0, 2, 14), "Renamed the button and its menu item, and kept ⌘, as the shortcut.", ch(3, 42, 7), Some("neko/4f9a1c"), offer(true, Some((Action::Undo, true)), false)),
    );
    let (t2, f2) = finished(s, done(52, "", w, "Docs check", at_time(s, 0, 6, 1), "Found two broken links in the install guide and fixed both.", ch(1, 2, 2), Some("neko/docs-links"), offer(true, Some((Action::Undo, true)), false)));
    let (t3, f3) = finished(
        s,
        Done { n: 53, cat: "", ws: a, title: "Find out why the nightly build of acme-api is slow", end: FinishedEnd::Failed, ended: at_time(s, -1, 23, 40), answer: Some("The test run timed out after 30 minutes."), changes: None, branch: None, offer: offer(false, None, false) },
    );
    let (t4, f4) = finished(
        s,
        Done { n: 54, cat: "", ws: g, title: "Tidy the Downloads folder into Archive and Screenshots", end: FinishedEnd::Stopped, ended: at_time(s, -1, 21, 5), answer: Some("I had moved the screenshots when you stopped me."), changes: None, branch: None, offer: offer(false, None, false) },
    );
    let (t5, f5) = finished(
        s,
        Done { n: 55, cat: "", ws: a, title: "Compress the case study images", end: FinishedEnd::Interrupted, ended: at_time(s, 0, 3, 0), answer: Some("Neko stopped while this ran."), changes: None, branch: None, offer: offer(false, None, false) },
    );
    let (t6, f6) = finished(
        s,
        done(56, "", w, "Write the pricing page copy", at_time(s, -1, 19, 30), "Drafted three versions of the copy and put the shortest one first.", ch(2, 30, 4), Some("neko/pricing-copy"), offer(true, Some((Action::Redo, true)), true)),
    );
    let (working, wk) = working_task(s, 57, "", a, "Port the backup script to the new server", 14);
    let docs = round_line(s, "docs", "Docs check", w, "cat_mochi", RoundOutcome::Ran, Some(at_time(s, 0, 6, 0)), at_time(s, 0, 6, 0), Some(&t2), None);
    let audit = round_line(s, "audit", "Dependency audit", a, "cat_tofu", RoundOutcome::Skipped, Some(at_time(s, 0, 4, 0)), at_time(s, 0, 4, 0), None, Some("The last run is still working."));
    let links = round_line(s, "links", "Link check", g, "cat_miso", RoundOutcome::Missed, Some(at_time(s, 0, 2, 0)), at_time(s, 0, 7, 55), None, Some("Neko was not running, or this Mac was asleep"));
    let backup = round_line(s, "backup", "Backup test", a, "cat_tofu", RoundOutcome::Braked, None, at_time(s, -1, 22, 30), None, Some("Turned itself off after 3 failed runs in a row."));
    let mut q_task = task(58, "Draft the release notes for 0.2 and send them to Lee for a read", 300);
    q_task.workspace = a.into();
    q_task.cat = None;
    q_task.status = TaskStatus::Asked;
    let question = NeedsItem {
        id: format!("question:run_{}", q_task.id.0),
        task: q_task.id.clone(),
        workspace: a.into(),
        title: q_task.title.clone(),
        cat: "".into(),
        cat_name: cat_name(s, ""),
        since_ms: at_time(s, 0, 3, 10),
        need: Need::Question { run: RunId(format!("run_{}", q_task.id.0)), question: "The notes cover the sidebar and the composer. Should I also mention the new keyboard shortcuts, or keep them for 0.3?".into(), kitten: None, via: Vec::new(), unanswered: false }, subagent: None
    };
    let mut p_task = task(59, "Answer the question about the retry limit before the release goes out", 300);
    p_task.workspace = w.into();
    p_task.cat = None;
    p_task.status = TaskStatus::NeedsYou;
    let request = permission_item(s, &p_task, "Run a command", "cargo test --package acme-api retry", false, 80);
    let needs = vec![question, request];
    s.tasks.extend([t1, t2, t3, t4, t5, t6, working, q_task, p_task]);
    s.tasks.sort_by_key(|t| std::cmp::Reverse(t.created_ms));
    let mut b = empty_briefing(s);
    b.finished = vec![f1, f2, f3, f4, f5, f6];
    b.finished.sort_by_key(|f| std::cmp::Reverse(f.ended_ms));
    b.rounds = vec![links, docs, audit, backup];
    b.rounds_ran = 1;
    b.rounds_not_run = 3;
    b.working = vec![wk];
    b.needs = needs.clone();
    b.totals = BriefingTotals { finished: 6, files: 4, added: 44, removed: 9, cost_usd: Some(1.84), api_equivalent: true };
    s.needs = needs;
    s.needs_loaded = true;
    s.briefing = Some(b);
    s.open_briefing();
}

/// The sparse house's morning: one finished task, nothing waiting, nothing working.
pub fn sparse_briefing(s: &mut App) {
    super::sparse(s);
    in_the_morning(s);
    default_morning(s);
    let ended = at_time(s, -1, 19, 2);
    let (t, f) = finished(
        s,
        Done { n: 30, cat: "", ws: "ws_acme", title: "What changed this week?", end: FinishedEnd::Done, ended, answer: Some("Nine commits landed this week, all in acme-api. Nothing is waiting for review."), changes: None, branch: None, offer: offer(false, None, false) },
    );
    s.tasks = vec![t];
    let mut b = empty_briefing(s);
    b.finished = vec![f];
    b.totals = BriefingTotals { finished: 1, files: 0, added: 0, removed: 0, cost_usd: Some(0.08), api_equivalent: true };
    s.briefing = Some(b);
    s.open_briefing();
}

pub fn briefing(s: &mut App) {
    full_morning(s, false);
}

pub fn briefing_many(s: &mut App) {
    full_morning(s, true);
}

pub fn briefing_finished(s: &mut App) {
    full_morning(s, false);
    s.needs.clear();
    s.tasks.retain(|t| !matches!(t.status, TaskStatus::Working | TaskStatus::Asked | TaskStatus::NeedsYou));
    if let Some(b) = &mut s.briefing {
        b.needs.clear();
        b.rounds.clear();
        b.rounds_ran = 0;
        b.rounds_not_run = 0;
        b.working.clear();
        b.finished.truncate(3);
        b.totals = BriefingTotals { finished: 3, files: 4, added: 44, removed: 9, cost_usd: Some(0.62), api_equivalent: true };
    }
}

pub fn briefing_empty(s: &mut App) {
    quiet_morning(s);
    let mut b = empty_briefing(s);
    b.read_ms = None;
    b.first = true;
    b.from_ms = s.now_ms - 24 * HOUR;
    s.briefing = Some(b);
    s.open_briefing();
}

pub fn briefing_caught_up(s: &mut App) {
    quiet_morning(s);
    let mut b = empty_briefing(s);
    b.read_ms = Some(s.now_ms - 4 * 60_000);
    b.from_ms = s.now_ms - 4 * 60_000;
    s.briefing = Some(b);
    s.open_briefing();
}

/// Read, nothing new, but two requests that began to wait before still wait for the person.
pub fn briefing_caught_up_waiting(s: &mut App) {
    full_morning(s, false);
    s.tasks.retain(|t| t.status != TaskStatus::Working);
    let from = s.now_ms - 4 * 60_000;
    if let Some(b) = &mut s.briefing {
        b.finished.clear();
        b.rounds.clear();
        b.rounds_ran = 0;
        b.rounds_not_run = 0;
        b.working.clear();
        b.totals = BriefingTotals::default();
        b.read_ms = Some(from);
        b.from_ms = from;
        for n in &mut b.needs {
            n.since_ms = from - 2 * HOUR;
        }
    }
    for n in &mut s.needs {
        n.since_ms = from - 2 * HOUR;
    }
}

pub fn briefing_morning_off(s: &mut App) {
    briefing_caught_up(s);
    s.briefing_settings = Some(BriefingSettings { morning_on: false, morning_at: "08:00".into() });
    if let Some(b) = &mut s.briefing {
        b.next_morning_ms = None;
    }
}

pub fn home_briefing(s: &mut App) {
    full_morning(s, false);
    s.go_new_thread();
}

pub fn settings_briefing(s: &mut App) {
    settings(s);
    s.settings_page = 4;
    default_morning(s);
}

pub fn settings_briefing_invalid(s: &mut App) {
    settings_briefing(s);
    s.briefing_time = TextEdit::new("8 o'clock");
}
