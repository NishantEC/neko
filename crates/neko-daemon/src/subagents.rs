//! Subagents in the daemon (docs/knowledge/subagents.md): starting them from the agent's tool, queueing them behind the limits,
//! reading their reports into wakes, waking a resting thread with `--resume` (or steering the report into a thread still working),
//! stopping a thread's whole tree, and picking up after a restart.
//!
//! One thread (`neko-subagents`) does the starting and the waking. It sleeps on a condition variable until something happened (a
//! subagent was made, a run ended, the limit news changed) or the 2 seconds a batch waits are over; with nothing to do it sets no
//! timer. Lock order, the same as everywhere: a run's book, then the core, then `runs.active`; this hub's own lock is a leaf (the
//! core is never taken while it is held).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use neko_core::Core;
use neko_core::conductor;
use neko_core::subagents::{self as rules, WakeRow};
use neko_protocol::{Change, DiffFile, FileStatus, Reply, ServerMessage, SubagentOf, Task, TaskId, TaskStatus};
use serde_json::{Value, json};

use crate::runs::{self, McpRun};
use crate::server::{Shared, lock};

/// What the hub remembers between passes.
#[derive(Default)]
struct State {
    dirty: bool,
    /// Tasks being stopped together with their thread: their reports wake no one.
    silenced: HashSet<TaskId>,
    /// While the account's limit is reached, nothing is started by itself until then.
    paused_until: Option<Instant>,
    /// Threads with a report waiting, and when the first was seen: it goes in a batch with those that come within [`rules::BATCH_MS`].
    seen: HashMap<TaskId, Instant>,
    /// The earliest time something is due (a batch ending), if any.
    next: Option<Instant>,
}

/// The daemon's subagent bookkeeping.
pub struct Hub {
    state: Mutex<State>,
    wake: Condvar,
    /// One pass at a time.
    pass: Mutex<()>,
}

impl Hub {
    pub fn new() -> Hub {
        Hub { state: Mutex::new(State { dirty: true, ..State::default() }), wake: Condvar::new(), pass: Mutex::new(()) }
    }

    fn silence(&self, ids: impl IntoIterator<Item = TaskId>) {
        lock(&self.state).silenced.extend(ids);
    }
}

fn batch() -> Duration {
    Duration::from_millis(std::env::var("NEKO_WAKE_BATCH_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(rules::BATCH_MS))
}

fn max_runs() -> usize {
    std::env::var("NEKO_MAX_RUNS").ok().and_then(|v| v.parse().ok()).unwrap_or(rules::MAX_RUNS)
}

/// Something happened that may let a subagent start or a thread wake.
pub fn poke(shared: &Shared) {
    lock(&shared.subagents.state).dirty = true;
    shared.subagents.wake.notify_one();
}

/// The thread that starts and wakes. It ends when the daemon stops.
pub fn spawn(shared: Arc<Shared>) -> std::io::Result<()> {
    std::thread::Builder::new().name("neko-subagents".into()).spawn(move || {
        loop {
            {
                let hub = &shared.subagents;
                let mut st = lock(&hub.state);
                loop {
                    if shared.stopping.load(Ordering::SeqCst) {
                        return;
                    }
                    if st.dirty {
                        break;
                    }
                    st = match st.next {
                        Some(at) => {
                            let now = Instant::now();
                            if at <= now {
                                break;
                            }
                            hub.wake.wait_timeout(st, at - now).unwrap_or_else(std::sync::PoisonError::into_inner).0
                        }
                        None => hub.wake.wait(st).unwrap_or_else(std::sync::PoisonError::into_inner),
                    };
                }
                st.dirty = false;
                st.next = None;
            }
            pass(&shared);
        }
    })?;
    Ok(())
}

/// The daemon is stopping: the thread wakes and ends.
pub fn stopping(shared: &Shared) {
    shared.subagents.wake.notify_all();
}

/// The account's limit news from a run: while it is reached, nothing is started by itself until it resets.
pub fn rate_limit(shared: &Shared, status: &str, resets_at: Option<i64>) {
    // `allowed_warning` only says the limit is near; anything but allowed or a warning means it is reached.
    let reached = !matches!(status, "allowed" | "allowed_warning");
    {
        let mut st = lock(&shared.subagents.state);
        if reached {
            let wait = resets_at.and_then(|at| {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
                (at > now).then(|| Duration::from_secs((at - now) as u64))
            });
            // Without a reset time, look again in five minutes.
            let until = Instant::now() + wait.unwrap_or(Duration::from_secs(300)).min(Duration::from_secs(6 * 3600));
            st.paused_until = Some(until);
            st.next = Some(st.next.map_or(until, |n| n.min(until)));
        } else {
            st.paused_until = None;
        }
    }
    poke(shared);
}

/// Whether the daemon holds back what it starts by itself.
fn paused(shared: &Shared) -> bool {
    let mut st = lock(&shared.subagents.state);
    match st.paused_until {
        Some(at) if at > Instant::now() => true,
        Some(_) => {
            st.paused_until = None;
            false
        }
        None => false,
    }
}

// ---- The pass ---------------------------------------------------------------------------------------------------------------------

/// Starts what may start and wakes what may wake. Safe to call from anywhere; passes run one at a time.
pub fn pass(shared: &Arc<Shared>) {
    let _one = lock(&shared.subagents.pass);
    if shared.stopping.load(Ordering::SeqCst) {
        return;
    }
    start_queued(shared);
    deliver_wakes(shared);
}

fn start_queued(shared: &Arc<Shared>) {
    let queued = lock(&shared.core).store.queued_subagents().unwrap_or_default();
    for sub in queued {
        if paused(shared) {
            return;
        }
        let Some(of) = &sub.subagent else { continue };
        let (total, mine) = {
            let active: Vec<TaskId> = lock(&shared.runs.active).keys().cloned().collect();
            if active.contains(&sub.id) {
                continue;
            }
            let core = lock(&shared.core);
            let mine = active.iter().filter(|id| core.store.get_task(id).ok().flatten().and_then(|t| t.subagent).is_some_and(|s| s.root == of.root)).count();
            (active.len(), mine)
        };
        if total >= max_runs() || mine >= rules::MAX_RUNNING_PER_THREAD {
            continue;
        }
        // The thread's own run must not be undone or gone: `start` checks the rest.
        let _ = runs::start(shared, sub, None);
    }
}

fn deliver_wakes(shared: &Arc<Shared>) {
    let pending = lock(&shared.core).store.pending_wakes().unwrap_or_default();
    let mut by_task: Vec<(TaskId, Vec<WakeRow>)> = Vec::new();
    for w in pending {
        match by_task.iter_mut().find(|(t, _)| *t == w.task) {
            Some((_, v)) => v.push(w),
            None => by_task.push((w.task.clone(), vec![w])),
        }
    }
    let waiting: HashSet<TaskId> = by_task.iter().map(|(t, _)| t.clone()).collect();
    lock(&shared.subagents.state).seen.retain(|t, _| waiting.contains(t));
    for (task_id, wakes) in by_task {
        let Some(task) = lock(&shared.core).store.get_task(&task_id).ok().flatten() else {
            let ids: Vec<String> = wakes.iter().map(|w| w.id.clone()).collect();
            let mut core = lock(&shared.core);
            let now = core.now();
            let _ = core.store.settle_wakes(&ids, "discarded", now);
            continue;
        };
        let running = lock(&shared.runs.active).get(&task_id).map(|a| (a.control.clone(), a.book.clone()));
        match running {
            // Its run is going: the report is steered in, as one more message to it.
            Some((Some(control), book)) => {
                if control.steer(&rules::wake_message(&wakes)).is_ok() {
                    let run = book.as_ref().map(|b| lock(b).run.id.clone());
                    delivered(shared, &task, run.as_ref(), &wakes);
                }
            }
            // Starting up: the next pass looks again (its run ending pokes).
            Some((None, _)) => {}
            None => {
                let due = {
                    let mut st = lock(&shared.subagents.state);
                    let first = *st.seen.entry(task_id.clone()).or_insert_with(Instant::now);
                    let due = first + batch();
                    if due > Instant::now() {
                        st.next = Some(st.next.map_or(due, |n| n.min(due)));
                    }
                    due
                };
                if due > Instant::now() || paused(shared) || lock(&shared.runs.active).len() >= max_runs() {
                    continue;
                }
                if runs::start_wake(shared, task, wakes).is_ok() {
                    lock(&shared.subagents.state).seen.remove(&task_id);
                }
            }
        }
    }
}

/// The reports were put in front of the agent: they are settled, and the notes are in the transcript.
pub fn delivered(shared: &Arc<Shared>, task: &Task, run: Option<&neko_protocol::RunId>, wakes: &[WakeRow]) {
    let mut core = lock(&shared.core);
    let now = core.now();
    let ids: Vec<String> = wakes.iter().map(|w| w.id.clone()).collect();
    let _ = core.store.settle_wakes(&ids, "delivered", now);
    if let Ok(outs) = conductor::wake_notes(&mut core, &task.id, run, wakes) {
        runs::push(shared, &task.id, outs, None);
    }
}

// ---- A subagent ended -------------------------------------------------------------------------------------------------------------

/// A run of `id` ended (however it did). A thread that rests while its subagents work is Waiting; a subagent that ended leaves its
/// report, which wakes its parent. Not at a stop: the next start picks up what is missing (`recover`).
pub fn ended(shared: &Arc<Shared>, id: &TaskId) {
    if shared.stopping.load(Ordering::SeqCst) {
        return;
    }
    let Some(task) = lock(&shared.core).store.get_task(id).ok().flatten() else { return };
    // Waiting: no run, and its subagents are not all reported.
    if matches!(task.status, TaskStatus::Done | TaskStatus::Asked) && has_live_children(shared, &task) {
        set_status(shared, &task, TaskStatus::Waiting);
    }
    let task = lock(&shared.core).store.get_task(id).ok().flatten().unwrap_or(task);
    if task.subagent.is_some() && matches!(task.status, TaskStatus::Done | TaskStatus::Failed | TaskStatus::Stopped | TaskStatus::Interrupted | TaskStatus::Asked) {
        report(shared, &task);
    }
    poke(shared);
}

/// Whether any subagent under this thread has not ended, or a report for it is unread: its folder is still in use.
pub fn has_live_subagents(shared: &Shared, task: &TaskId) -> bool {
    let core = lock(&shared.core);
    core.store.subagents_of(task).unwrap_or_default().iter().any(|c| rules::is_live(c.status)) || core.store.has_pending_wake(task).unwrap_or(false)
}

fn has_live_children(shared: &Shared, task: &Task) -> bool {
    let core = lock(&shared.core);
    core.store.children_of(&task.id).unwrap_or_default().iter().any(|c| rules::is_live(c.status)) || core.store.has_pending_wake(&task.id).unwrap_or(false)
}

fn set_status(shared: &Shared, task: &Task, status: TaskStatus) {
    let mut core = lock(&shared.core);
    let now = core.now();
    if let Ok((t, rev)) = core.store.set_task_status(&task.id, status, now) {
        shared.broadcast(&ServerMessage::Event { revision: rev, change: Change::TaskUpdated { task: t } });
    }
}

/// The files a subagent's newest run changed, with counts, for its report.
fn files_of(shared: &Arc<Shared>, task: &Task) -> Vec<String> {
    let run = lock(&shared.core).store.runs_of(&task.id).ok().and_then(|mut r| r.pop()).map(|r| r.id);
    let Some(run) = run else { return Vec::new() };
    match crate::undo::diff(shared, &task.id, Some(run), neko_protocol::DiffSource::Run) {
        Reply::Diff { diff } => {
            let mut v: Vec<String> = diff.files.iter().take(30).map(file_line).collect();
            if diff.files.len() > 30 {
                v.push(format!("and {} more", diff.files.len() - 30));
            }
            v
        }
        _ => Vec::new(),
    }
}

fn file_line(f: &DiffFile) -> String {
    match f.status {
        FileStatus::Deleted => format!("{} deleted", f.path),
        _ if f.binary => format!("{} (binary)", f.path),
        _ => format!("{} +{} -{}", f.path, f.added, f.removed),
    }
}

/// Stores the report of a subagent that ended, once.
fn report(shared: &Arc<Shared>, task: &Task) {
    let Some(of) = &task.subagent else { return };
    if lock(&shared.core).store.ended_without_wake().map_or(true, |t| !t.iter().any(|x| x.id == task.id)) {
        return;
    }
    let silenced = lock(&shared.subagents.state).silenced.remove(&task.id);
    let files = if silenced { Vec::new() } else { files_of(shared, task) };
    let mut core = lock(&shared.core);
    let now = core.now();
    let mut summary = core.store.final_words(&task.id).unwrap_or_default();
    if summary.is_empty() {
        let reason = core.store.runs_of(&task.id).ok().and_then(|mut r| r.pop()).and_then(|r| r.reason);
        summary = match (task.status, reason) {
            (_, Some(r)) => r,
            (TaskStatus::Stopped, None) => "It was stopped before it said anything.".to_owned(),
            _ => "It ended without saying anything.".to_owned(),
        };
    }
    let branch = core.store.place(&task.id).ok().flatten().and_then(|p| p.branch);
    let w = WakeRow {
        id: core.new_id("wake"),
        task: of.parent.clone(),
        subagent: task.id.clone(),
        number: of.number.clone(),
        title: task.title.clone(),
        status: rules::status_word(task.status).to_owned(),
        summary,
        files,
        branch,
        created_ms: now,
    };
    let _ = core.store.insert_wake(&w, if silenced { "discarded" } else { "pending" }, now);
}

// ---- Stop -------------------------------------------------------------------------------------------------------------------------

/// Every subagent under `task`, at any depth.
fn descendants(core: &Core, task: &Task) -> Vec<Task> {
    let root = task.subagent.as_ref().map_or_else(|| task.id.clone(), |s| s.root.clone());
    let all = core.store.subagents_of(&root).unwrap_or_default();
    match &task.subagent {
        None => all,
        Some(s) => {
            let prefix = format!("{}.", s.number);
            all.into_iter().filter(|t| t.subagent.as_ref().is_some_and(|x| x.number.starts_with(&prefix))).collect()
        }
    }
}

/// Stop on a thread or a subagent: everything it started stops with it (the run guard ends each one's processes), and their reports
/// wake no one. A thread that is only waiting becomes Stopped. A single subagent stopped from its own view or by its parent leaves a
/// report that says so, which wakes the parent.
pub fn stop_tree(shared: &Arc<Shared>, id: &TaskId) {
    let (task, below) = {
        let core = lock(&shared.core);
        let Some(task) = core.store.get_task(id).ok().flatten() else { return };
        let below = descendants(&core, &task);
        (task, below)
    };
    shared.subagents.silence(below.iter().map(|t| t.id.clone()));
    {
        let mut core = lock(&shared.core);
        let now = core.now();
        let _ = core.store.discard_wakes_of(&task.id, now);
        for t in &below {
            let _ = core.store.discard_wakes_of(&t.id, now);
        }
    }
    for t in below.iter().chain(std::iter::once(&task)) {
        let active = lock(&shared.runs.active).get(&t.id).map(|a| {
            a.cancelled.store(true, Ordering::SeqCst);
            a.control.clone()
        });
        match active {
            Some(Some(c)) => c.stop(),
            Some(None) => {}
            // Nothing runs for it: a subagent not started yet, or a thread that waits. It is stopped here.
            None => {
                let now_status = lock(&shared.core).store.get_task(&t.id).ok().flatten().map(|x| x.status);
                if matches!(now_status, Some(TaskStatus::New | TaskStatus::Waiting)) && (t.subagent.is_some() || now_status == Some(TaskStatus::Waiting)) {
                    set_status(shared, t, TaskStatus::Stopped);
                    ended(shared, &t.id);
                }
            }
        }
    }
}

// ---- Restart ----------------------------------------------------------------------------------------------------------------------

/// At start, after the runs the last daemon left running were interrupted: every subagent that ended with no report gets one (an
/// interrupted subagent reports that it was), so its thread is woken. Nothing is lost: queued subagents start, waiting threads stay
/// waiting for the reports.
pub fn recover(core: &mut Core) {
    let Ok(ended) = core.store.ended_without_wake() else { return };
    for task in ended {
        let Some(of) = &task.subagent else { continue };
        let now = core.now();
        let mut summary = core.store.final_words(&task.id).unwrap_or_default();
        if summary.is_empty() {
            summary = "Neko stopped while it worked, before it said anything.".to_owned();
        }
        let status = rules::status_word(task.status).to_owned();
        let w = WakeRow { id: core.new_id("wake"), task: of.parent.clone(), subagent: task.id.clone(), number: of.number.clone(), title: task.title.clone(), status, summary, files: Vec::new(), branch: None, created_ms: now };
        let _ = core.store.insert_wake(&w, "pending", now);
    }
}

// ---- The agent's tools --------------------------------------------------------------------------------------------------------------

fn answer(text: impl Into<String>, error: bool) -> Value {
    json!({"text": text.into(), "error": error})
}

/// `POST /v1/mcp` for a run: `info`, or one tool call. Every answer is quick; nothing here waits for a subagent.
pub fn handle(shared: &Arc<Shared>, run: &McpRun, body: &Value) -> Value {
    if body["op"] == "info" {
        let depth = lock(&shared.core).store.get_task(&run.task).ok().flatten().and_then(|t| t.subagent).map_or(0, |s| SubagentOf::depth(&s));
        return json!({"depth": depth});
    }
    let args = &body["arguments"];
    match body["tool"].as_str().unwrap_or("") {
        "start_subagent" => start(shared, run, args),
        "subagents" => list(shared, run, args),
        "stop_subagent" => stop_one(shared, run, args),
        other => answer(format!("Neko has no tool called {other}."), true),
    }
}

fn start(shared: &Arc<Shared>, run: &McpRun, args: &Value) -> Value {
    let (Some(title), Some(prompt)) = (args["title"].as_str(), args["prompt"].as_str()) else { return answer("start_subagent needs a title and a prompt, both text.", true) };
    match args["placement"].as_str() {
        None | Some("shared") => {}
        Some(other) => return answer(format!("Placement {other} is not available yet. Use shared: the subagent works in your folder."), true),
    }
    if shared.stopping.load(Ordering::SeqCst) {
        return answer("Neko is stopping.", true);
    }
    if lock(&shared.runs.active).get(&run.task).is_some_and(|a| a.cancelled.load(Ordering::SeqCst)) {
        return answer("The person stopped this thread, so no subagent was started.", true);
    }
    let sub = {
        let mut core = lock(&shared.core);
        match core.create_subagent(&run.task, title, prompt) {
            Ok((sub, rev)) => {
                shared.broadcast(&ServerMessage::Event { revision: rev, change: Change::TaskCreated { task: sub.clone() } });
                sub
            }
            Err(why) => return answer(why, true),
        }
    };
    {
        let mut b = lock(&run.book);
        let mut core = lock(&shared.core);
        let now = core.now();
        if let Ok(outs) = b.subagent_card(&mut core, &sub, now) {
            runs::push(shared, &run.task, outs, run.control.as_ref());
        }
    }
    // Start it now when there is room, so the answer is true; otherwise it is queued and the next pass takes it.
    pass(shared);
    let number = sub.subagent.as_ref().map(|s| s.number.clone()).unwrap_or_default();
    let started = lock(&shared.runs.active).contains_key(&sub.id);
    answer(rules::started_text(&number, &sub.title, !started), false)
}

/// A subagent of this run's task by the number the agent names (`1`, `1.2`, or "Subagent 1").
fn child(shared: &Shared, run: &McpRun, id: &str) -> Option<Task> {
    let want = id.trim().trim_start_matches("Subagent").trim();
    lock(&shared.core).store.children_of(&run.task).ok()?.into_iter().find(|c| c.subagent.as_ref().is_some_and(|s| s.number == want))
}

fn state_words(t: &Task) -> &'static str {
    match t.status {
        TaskStatus::New => "queued",
        TaskStatus::Working | TaskStatus::NeedsYou => "working",
        TaskStatus::Waiting => "waiting on its own subagents",
        TaskStatus::Done => "done",
        TaskStatus::Failed => "failed",
        TaskStatus::Stopped => "stopped",
        TaskStatus::Interrupted => "interrupted",
        TaskStatus::Asked => "ended with a question",
    }
}

fn list(shared: &Arc<Shared>, run: &McpRun, args: &Value) -> Value {
    if let Some(id) = args["id"].as_str() {
        let Some(c) = child(shared, run, id) else { return answer(format!("You have no subagent {id}."), true) };
        let words = lock(&shared.core).store.final_words(&c.id).unwrap_or_default();
        let number = c.subagent.as_ref().map(|s| s.number.clone()).unwrap_or_default();
        let body = if words.is_empty() { "It has not said anything yet.".to_owned() } else { words };
        return answer(format!("Subagent {number} (\"{}\"), {}:\n\n{body}", c.title, state_words(&c)), false);
    }
    let kids = lock(&shared.core).store.children_of(&run.task).unwrap_or_default();
    if kids.is_empty() {
        return answer("You have started no subagents.", false);
    }
    let lines: Vec<String> = kids
        .iter()
        .map(|c| {
            let number = c.subagent.as_ref().map(|s| s.number.clone()).unwrap_or_default();
            let last = lock(&shared.core).store.final_words(&c.id).unwrap_or_default();
            let last = last.lines().rev().find(|l| !l.trim().is_empty()).map(|l| l.chars().take(100).collect::<String>()).unwrap_or_default();
            if last.is_empty() { format!("Subagent {number} · {} · {}", c.title, state_words(c)) } else { format!("Subagent {number} · {} · {} · {last}", c.title, state_words(c)) }
        })
        .collect();
    answer(format!("{}\n\nDo not call this to wait: Neko wakes you with each report.", lines.join("\n")), false)
}

fn stop_one(shared: &Arc<Shared>, run: &McpRun, args: &Value) -> Value {
    let Some(id) = args["id"].as_str() else { return answer("stop_subagent needs the subagent's number.", true) };
    let Some(c) = child(shared, run, id) else { return answer(format!("You have no subagent {id}."), true) };
    if !rules::is_live(c.status) {
        return answer(format!("Subagent {id} has already ended."), false);
    }
    stop_tree(shared, &c.id);
    answer(format!("Subagent {id} is stopped. You are woken with its report."), false)
}
