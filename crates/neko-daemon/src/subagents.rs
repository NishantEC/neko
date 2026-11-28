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
use neko_core::subagents::{self as rules, DraftChange, NewSubagent, Standing, WakeRow};
use neko_core::workplace::{self, PlaceError};
use neko_protocol::{AskTo, Change, DiffFile, ErrorCode, FileStatus, Reply, RunAsk, SendMode, ServerMessage, SubPlace, SubagentOf, Task, TaskId, TaskStatus};
use serde_json::{Value, json};

use crate::heat::{self, Heat};
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

/// Where a stacked kitten stands against the sibling it follows.
enum Turn {
    /// It may start: it follows no one, or the one it follows is done.
    Go,
    /// The one it follows has not ended (or is a draft the person has not started).
    Wait,
    /// The one it follows ended without finishing: it will not start, and why.
    Cancel(String),
}

fn turn_of(shared: &Arc<Shared>, sub: &Task) -> Turn {
    let Some(of) = &sub.subagent else { return Turn::Go };
    let Some(after) = &of.after else { return Turn::Go };
    let pred = lock(&shared.core).store.children_of(&of.parent).unwrap_or_default().into_iter().find(|c| c.subagent.as_ref().is_some_and(|s| &s.number == after));
    match pred {
        None => Turn::Cancel(format!("Subagent {after}, which it follows, is gone, so Subagent {} was not started.", of.number)),
        // One that asked waits for its answer, and may still finish.
        Some(p) if p.subagent.as_ref().is_some_and(|s| s.draft) || p.is_live_subagent() || p.status == TaskStatus::Asked => Turn::Wait,
        Some(p) if p.status == TaskStatus::Done => Turn::Go,
        Some(p) => {
            let how = match p.status {
                TaskStatus::Failed => "failed",
                TaskStatus::Stopped if p.subagent.as_ref().is_some_and(|s| s.discarded) => "was discarded",
                TaskStatus::Stopped => "was stopped",
                _ => "was interrupted",
            };
            Turn::Cancel(format!("Subagent {after}, which it follows, {how}, so Subagent {} was not started. Start it again yourself once you have dealt with Subagent {after}.", of.number))
        }
    }
}

/// A stacked kitten that will not start: it ends stopped, and leaves a report that says why, so its thread is woken and decides.
fn cancel(shared: &Arc<Shared>, sub: &Task, why: &str) {
    set_status(shared, sub, TaskStatus::Stopped);
    let now_task = lock(&shared.core).store.get_task(&sub.id).ok().flatten().unwrap_or_else(|| sub.clone());
    report(shared, &now_task, Some(why));
    poke(shared);
}

fn start_queued(shared: &Arc<Shared>) {
    let queued = lock(&shared.core).store.queued_subagents().unwrap_or_default();
    // Whether the Mac lets Neko start more by itself: read only when a kitten could start, then again after a while.
    let mut heat: Option<Heat> = None;
    let mut held = false;
    for sub in queued {
        if paused(shared) {
            return;
        }
        let Some(of) = &sub.subagent else { continue };
        match turn_of(shared, &sub) {
            Turn::Wait => continue,
            Turn::Cancel(why) => {
                cancel(shared, &sub, &why);
                continue;
            }
            Turn::Go => {}
        }
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
        if *heat.get_or_insert_with(heat::level) != Heat::Ok {
            held = true;
            continue;
        }
        // The thread's own run must not be undone or gone: `start` checks the rest.
        let _ = runs::start(shared, sub, None);
    }
    if held {
        // Something waits for the Mac to cool: look again after a while (the only timer this hub ever sets for it).
        let at = Instant::now() + heat::recheck();
        let mut st = lock(&shared.subagents.state);
        st.next = Some(st.next.map_or(at, |n| n.min(at)));
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
        // It asked with Neko's tools and waits for the answer: the reports wait for it too, kept, and go once it runs again (its answer
        // starts a run, which they are steered into). Nothing is lost, and the question stays the one thing in front of whoever answers.
        if waits_for_answer(shared, &task) {
            continue;
        }
        let in_subagent = task.subagent.is_some();
        let running = lock(&shared.runs.active).get(&task_id).map(|a| (a.control.clone(), a.book.clone()));
        match running {
            // Its run is going: the report is steered in, as one more message to it.
            Some((Some(control), book)) => {
                if control.steer(&rules::wake_message(&wakes, in_subagent)).is_ok() {
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
                if due > Instant::now() || paused(shared) || lock(&shared.runs.active).len() >= max_runs() || heat::level() == Heat::Wait {
                    if heat::level() == Heat::Wait {
                        let at = Instant::now() + heat::recheck();
                        let mut st = lock(&shared.subagents.state);
                        st.next = Some(st.next.map_or(at, |n| n.min(at)));
                    }
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
    // A kitten's question it read and neither answered nor passed on goes up by itself: no question is left to sit. (Last words that
    // only end with a question of the agent's own do not answer it either.)
    if matches!(task.status, TaskStatus::Done | TaskStatus::Asked) {
        pass_up_unanswered(shared, &task);
    }
    let task = lock(&shared.core).store.get_task(id).ok().flatten().unwrap_or(task);
    // Waiting: no run, and its subagents are not all reported. Not while it waits for the answer to a question it put: that is the
    // person's (or its parent's) turn, and the reports wait for the answer.
    let waits = waits_for_answer(shared, &task);
    if matches!(task.status, TaskStatus::Done | TaskStatus::Asked) && !waits && has_live_children(shared, &task) {
        set_status(shared, &task, TaskStatus::Waiting);
    }
    let task = lock(&shared.core).store.get_task(id).ok().flatten().unwrap_or(task);
    // The kittens whose question it passed up now say who has it.
    if waits {
        let chain = lock(&shared.core).store.newest_ask(&task.id).ok().flatten().map(|a| a.chain).unwrap_or_default();
        touch_chain(shared, &task, &chain);
    }
    if task.subagent.is_some() && matches!(task.status, TaskStatus::Done | TaskStatus::Failed | TaskStatus::Stopped | TaskStatus::Interrupted | TaskStatus::Asked) {
        report(shared, &task, None);
    }
    poke(shared);
}

/// Whether any subagent under this thread has not ended, or a report for it is unread: its folder is still in use.
pub fn has_live_subagents(shared: &Shared, task: &TaskId) -> bool {
    let core = lock(&shared.core);
    core.store.subagents_of(task).unwrap_or_default().iter().any(Task::is_live_subagent) || core.store.has_pending_wake(task).unwrap_or(false)
}

/// Whether a task asked a question with Neko's tools and waits for its answer (its newest run carries the question).
pub fn waits_for_answer(shared: &Shared, task: &Task) -> bool {
    task.status == TaskStatus::Asked && lock(&shared.core).store.newest_ask(&task.id).ok().flatten().is_some()
}

/// Screens hear again about the kittens a question came up from (who has it changed). `chain` holds their numbers under `task`'s
/// thread.
pub fn touch_chain(shared: &Shared, task: &Task, chain: &[String]) {
    if chain.is_empty() {
        return;
    }
    let root = task.subagent.as_ref().map_or_else(|| task.id.clone(), |s| s.root.clone());
    let core = lock(&shared.core);
    let Ok(rev) = core.store.revision() else { return };
    for t in core.store.subagents_of(&root).unwrap_or_default() {
        if t.subagent.as_ref().is_some_and(|s| chain.contains(&s.number))
            && let Ok(Some(t)) = core.store.get_task(&t.id)
        {
            shared.broadcast(&ServerMessage::Event { revision: rev, change: Change::TaskUpdated { task: t } });
        }
    }
}

/// The agent's turn ended and a question one of its kittens put with `ask_parent` was in front of it, unanswered and not passed on: Neko
/// passes it up itself, as this task's question (to the person for a thread, to its parent for a kitten), marked unanswered. One at a
/// time: the next goes up when this one is answered and the agent's turn ends again.
fn pass_up_unanswered(shared: &Shared, task: &Task) {
    let mut core = lock(&shared.core);
    let kids = core.store.children_of(&task.id).unwrap_or_default();
    for c in kids.iter().filter(|c| c.status == TaskStatus::Asked) {
        let Some(number) = c.subagent.as_ref().map(|s| s.number.clone()) else { continue };
        let Ok(Some(theirs)) = core.store.newest_ask(&c.id) else { continue };
        let Some(their_run) = core.store.runs_of(&c.id).ok().and_then(|mut r| r.pop()) else { continue };
        if !core.store.wake_delivered(&c.id, &their_run.id).unwrap_or(false) {
            continue;
        }
        let Some(mine) = core.store.runs_of(&task.id).ok().and_then(|mut r| r.pop()) else { return };
        if mine.ask.is_some() {
            return;
        }
        let mut chain = theirs.chain.clone();
        chain.push(number);
        let to = if task.subagent.is_some() { AskTo::Parent } else { AskTo::Person };
        let ask = RunAsk { to, text: theirs.text, chain, unanswered: true };
        let now = core.now();
        if core.store.set_run_ask(&mine.id, &ask, now).is_err() {
            return;
        }
        if let Ok(Some(run)) = core.store.get_run(&mine.id)
            && let Ok(rev) = core.store.revision()
        {
            shared.broadcast(&ServerMessage::Event { revision: rev, change: Change::RunChanged { run } });
        }
        if let Ok((t, rev)) = core.store.set_task_status(&task.id, TaskStatus::Asked, now) {
            shared.broadcast(&ServerMessage::Event { revision: rev, change: Change::TaskUpdated { task: t } });
        }
        return;
    }
}

fn has_live_children(shared: &Shared, task: &Task) -> bool {
    let core = lock(&shared.core);
    core.store.children_of(&task.id).unwrap_or_default().iter().any(Task::is_live_subagent) || core.store.has_pending_wake(&task.id).unwrap_or(false)
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

/// The branch an own copy left, when it is still there, and the note about work it did not commit (the agent merges a branch, so
/// only committed work is on it).
fn copy_facts(shared: &Arc<Shared>, task: &Task) -> (Option<String>, String) {
    let Some(of) = &task.subagent else { return (None, String::new()) };
    if of.placement != SubPlace::OwnCopy {
        return (None, String::new());
    }
    let place = lock(&shared.core).store.place(&task.id).ok().flatten();
    let (Some(place), Some(branch)) = (place, of.branch.clone()) else { return (None, String::new()) };
    let repo = place.repo.clone().map(std::path::PathBuf::from);
    let exists = repo.as_deref().is_some_and(|r| workplace::branch_exists(r, &branch));
    let mut note = String::new();
    let tree = workplace::worktree_root(&shared.runs.data_dir, &task.id);
    if exists
        && tree.exists()
        && let Some(base) = &place.base_commit
        && let Some(st) = workplace::tree_state(&tree, &branch, base)
        && st.uncommitted
    {
        note = format!("\n\nNote: it left changes in its copy that are not committed, so they are not on the branch {branch}. Its copy is {}. Commit them there before you merge.", place.workdir);
    }
    (exists.then_some(branch), note)
}

/// Stores the report of a subagent that ended, once. `said` stands in for its words (a kitten that never started).
fn report(shared: &Arc<Shared>, task: &Task, said: Option<&str>) {
    let Some(of) = &task.subagent else { return };
    if of.draft || of.discarded {
        return;
    }
    if lock(&shared.core).store.ended_without_wake().map_or(true, |t| !t.iter().any(|x| x.id == task.id)) {
        return;
    }
    let silenced = lock(&shared.subagents.state).silenced.remove(&task.id);
    let (run, question, chain) = {
        let core = lock(&shared.core);
        let run = core.store.runs_of(&task.id).ok().and_then(|mut r| r.pop()).map(|r| r.id);
        let asked = task.status == TaskStatus::Asked;
        let question = if asked { core.store.question_of(&task.id).ok().flatten() } else { None };
        let chain = if asked { core.store.newest_ask(&task.id).ok().flatten().map(|a| a.chain).unwrap_or_default() } else { Vec::new() };
        (run, question, chain)
    };
    let files = if silenced || said.is_some() { Vec::new() } else { files_of(shared, task) };
    let (branch, note) = if said.is_some() { (None, String::new()) } else { copy_facts(shared, task) };
    let mut core = lock(&shared.core);
    let now = core.now();
    let mut summary = said.map(str::to_owned).unwrap_or_else(|| core.store.final_words(&task.id).unwrap_or_default());
    if summary.is_empty() {
        let reason = core.store.runs_of(&task.id).ok().and_then(|mut r| r.pop()).and_then(|r| r.reason);
        summary = match (task.status, reason) {
            (_, Some(r)) => r,
            (TaskStatus::Stopped, None) => "It was stopped before it said anything.".to_owned(),
            _ => "It ended without saying anything.".to_owned(),
        };
    }
    summary.push_str(&note);
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
        run,
        question,
        chain,
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
                let now = lock(&shared.core).store.get_task(&t.id).ok().flatten();
                let now_status = now.as_ref().map(|x| x.status);
                if now.as_ref().and_then(|x| x.subagent.as_ref()).is_some_and(|s| s.draft) {
                    // Never started: it is thrown away, and no report wakes anyone.
                    discard(shared, &t.id);
                } else if matches!(now_status, Some(TaskStatus::New | TaskStatus::Waiting | TaskStatus::Asked))
                    && (t.subagent.is_some() || now_status == Some(TaskStatus::Waiting) || now.as_ref().is_some_and(|x| waits_for_answer(shared, x)))
                {
                    // A kitten waiting on an answer to its question is stopped too, and so is a thread waiting on the person's answer
                    // to one it put with Neko's tools: the question goes with it. (A thread whose last words only ended with a question
                    // has nothing to stop: it stays as it is, as before.)
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
        let branch = (of.placement == SubPlace::OwnCopy).then(|| of.branch.clone()).flatten();
        let run = core.store.runs_of(&task.id).ok().and_then(|mut r| r.pop()).map(|r| r.id);
        let asked = task.status == TaskStatus::Asked;
        let question = if asked { core.store.question_of(&task.id).ok().flatten() } else { None };
        let chain = if asked { core.store.newest_ask(&task.id).ok().flatten().map(|a| a.chain).unwrap_or_default() } else { Vec::new() };
        let w = WakeRow { id: core.new_id("wake"), task: of.parent.clone(), subagent: task.id.clone(), number: of.number.clone(), title: task.title.clone(), status, summary, files: Vec::new(), branch, created_ms: now, run, question, chain };
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
        "start_drafts" => start_drafts_tool(shared, run),
        "subagents" => list(shared, run, args),
        "stop_subagent" => stop_one(shared, run, args),
        "message_subagent" => message(shared, run, args),
        "ask_parent" => ask(shared, run, args, AskTo::Parent),
        "ask_person" => ask(shared, run, args, AskTo::Person),
        other => answer(format!("Neko has no tool called {other}."), true),
    }
}

fn start(shared: &Arc<Shared>, run: &McpRun, args: &Value) -> Value {
    let (Some(title), Some(prompt)) = (args["title"].as_str(), args["prompt"].as_str()) else { return answer("start_subagent needs a title and a prompt, both text.", true) };
    let placement = match args["placement"].as_str() {
        None | Some("shared") => SubPlace::Shared,
        Some("own_copy") => SubPlace::OwnCopy,
        Some(other) => return answer(format!("Placement {other} is not one of shared and own_copy."), true),
    };
    let after = args["after"].as_str().filter(|a| !a.trim().is_empty()).map(str::to_owned);
    let mut draft = args["draft"].as_bool().unwrap_or(false);
    if shared.stopping.load(Ordering::SeqCst) {
        return answer("Neko is stopping.", true);
    }
    if lock(&shared.runs.active).get(&run.task).is_some_and(|a| a.cancelled.load(Ordering::SeqCst)) {
        return answer("The person stopped this thread, so no subagent was started.", true);
    }
    if placement == SubPlace::OwnCopy {
        // A copy is a git worktree: the folder this agent works in must be in a repository that has a commit.
        let here = lock(&shared.core).store.place(&run.task).ok().flatten().map(|p| std::path::PathBuf::from(p.workdir));
        if here.as_deref().and_then(workplace::copy_source).is_none() {
            return answer("Placement own_copy needs the project to be a git repository with a commit, and this folder is not one. Use shared (it works in your folder), or do this part yourself.", true);
        }
    }
    let (settings, first) = {
        let core = lock(&shared.core);
        let first = core.store.get_task(&run.task).ok().flatten().is_some_and(|t| t.subagent.is_none()) && core.store.subagents_of(&run.task).map_or(false, |v| v.is_empty());
        (core.store.settings().ok(), first)
    };
    // "Ask before a thread's first kitten": the first one is written down as a draft, and the person starts it.
    let asked = !draft && first && settings.is_some_and(|s| s.ask_first_subagent);
    draft |= asked;
    let sub = {
        let mut core = lock(&shared.core);
        match core.create_subagent(&run.task, title, prompt, NewSubagent { placement, after, draft }) {
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
    let of = sub.subagent.clone().unwrap_or_else(|| SubagentOf::new(run.task.clone(), "", run.task.clone()));
    let standing = if draft {
        Standing::Draft { asked }
    } else {
        // Start it now when there is room, so the answer is true; otherwise it is queued and the next pass takes it.
        pass(shared);
        if lock(&shared.runs.active).contains_key(&sub.id) {
            Standing::Started
        } else {
            match of.after.clone() {
                Some(n) if matches!(turn_of(shared, &sub), Turn::Wait) => Standing::After(n),
                _ => Standing::Queued,
            }
        }
    };
    answer(rules::started_text(&of.number, &sub.title, &standing, of.branch.as_deref()), false)
}

/// `start_drafts`: starts the drafts this run's task wrote.
fn start_drafts_tool(shared: &Arc<Shared>, run: &McpRun) -> Value {
    let drafts = lock(&shared.core).store.drafts_of(&run.task).unwrap_or_default();
    if drafts.is_empty() {
        return answer("You have no drafts to start.", false);
    }
    let mut numbers = Vec::new();
    for d in &drafts {
        if let Some(t) = release_draft(shared, d) {
            numbers.push(t.subagent.as_ref().map(|s| s.number.clone()).unwrap_or_default());
        }
    }
    pass(shared);
    answer(
        format!("Started Subagent {}. Neko wakes you with each report. Do not poll or wait. If you have nothing else to do, end your turn now.", numbers.join(", Subagent ")),
        false,
    )
}

/// A draft becomes a kitten that waits its turn (or runs): the draft flag goes, with its event, and screens are told.
fn release_draft(shared: &Arc<Shared>, draft: &Task) -> Option<Task> {
    let mut core = lock(&shared.core);
    let now = core.now();
    let (t, rev) = core.store.change_draft(&draft.id, DraftChange::Start, now).ok()?;
    shared.broadcast(&ServerMessage::Event { revision: rev, change: Change::TaskUpdated { task: t.clone() } });
    Some(t)
}

/// A draft is thrown away: it never ran, and nothing is woken.
fn discard(shared: &Arc<Shared>, id: &TaskId) -> Option<Task> {
    let mut core = lock(&shared.core);
    let now = core.now();
    let (t, rev) = core.store.change_draft(id, DraftChange::Discard, now).ok()?;
    shared.broadcast(&ServerMessage::Event { revision: rev, change: Change::TaskUpdated { task: t.clone() } });
    Some(t)
}

fn draft_of(shared: &Arc<Shared>, id: &TaskId) -> Result<Task, Reply> {
    match lock(&shared.core).store.get_task(id) {
        Ok(Some(t)) if t.subagent.as_ref().is_some_and(|s| s.draft) => Ok(t),
        Ok(Some(_)) => Err(Reply::error(ErrorCode::Refused, "That kitten is not a draft.")),
        Ok(None) => Err(Reply::error(ErrorCode::NotFound, "There is no such kitten.")),
        Err(e) => Err(Reply::error(ErrorCode::Internal, e.to_string())),
    }
}

/// The person's Start on a draft card: it runs when there is room, and its thread (resting, with nothing else going) waits on it.
pub fn start_draft(shared: &Arc<Shared>, id: &TaskId) -> Reply {
    let draft = match draft_of(shared, id) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let Some(t) = release_draft(shared, &draft) else { return Reply::error(ErrorCode::Internal, "Neko could not start the draft.") };
    // The thread rests with nothing going: it is Waiting on this kitten now (a thread whose run is going gets there when the run ends).
    if let Some(of) = &t.subagent {
        let parent = lock(&shared.core).store.get_task(&of.parent).ok().flatten();
        if let Some(p) = parent
            && matches!(p.status, TaskStatus::Done | TaskStatus::Asked)
            && !waits_for_answer(shared, &p)
            && !lock(&shared.runs.active).contains_key(&p.id)
        {
            set_status(shared, &p, TaskStatus::Waiting);
        }
    }
    pass(shared);
    poke(shared);
    Reply::Task { task: lock(&shared.core).store.get_task(id).ok().flatten().unwrap_or(t) }
}

/// The person changes a draft's prompt, in the card.
pub fn edit_draft(shared: &Arc<Shared>, id: &TaskId, prompt: &str) -> Reply {
    if let Err(r) = draft_of(shared, id) {
        return r;
    }
    if prompt.trim().is_empty() {
        return Reply::error(ErrorCode::BadRequest, "A kitten needs a prompt: it sees nothing else.");
    }
    if prompt.len() > neko_protocol::MAX_WORDS_BYTES {
        return Reply::error(ErrorCode::BadRequest, "That prompt is too long. Put the long part in a file and name the file.");
    }
    let mut core = lock(&shared.core);
    let now = core.now();
    match core.store.change_draft(id, DraftChange::Prompt(prompt.trim_end()), now) {
        Ok((task, rev)) => {
            shared.broadcast(&ServerMessage::Event { revision: rev, change: Change::TaskUpdated { task: task.clone() } });
            Reply::Task { task }
        }
        Err(e) => Reply::error(ErrorCode::Internal, e.to_string()),
    }
}

/// The person's Discard on a draft card.
pub fn discard_draft(shared: &Arc<Shared>, id: &TaskId) -> Reply {
    if let Err(r) = draft_of(shared, id) {
        return r;
    }
    match discard(shared, id) {
        Some(task) => {
            poke(shared);
            Reply::Task { task }
        }
        None => Reply::error(ErrorCode::Internal, "Neko could not discard the draft."),
    }
}

// ---- Where a kitten works ---------------------------------------------------------------------------------------------------------

/// The task whose folder `task` works in: itself, or (a kitten in its parent's folder) the nearest one above it that has a folder of its
/// own: the thread, or a kitten in a copy.
pub fn folder_task(shared: &Shared, task: &TaskId) -> TaskId {
    let core = lock(&shared.core);
    let mut at = task.clone();
    for _ in 0..8 {
        let Some(t) = core.store.get_task(&at).ok().flatten() else { break };
        match &t.subagent {
            Some(of) if of.placement == SubPlace::Shared => at = of.parent.clone(),
            _ => break,
        }
    }
    at
}

/// Makes an own copy for a kitten about to run: a worktree on its own branch from the folder it follows, uncommitted changes included
/// (what a stacked kitten follows is the copy of the one before it, when that one has one still).
pub fn copy_place(shared: &Shared, task: &Task) -> Result<neko_core::records::Place, PlaceError> {
    let Some(of) = &task.subagent else { return Err(PlaceError::Other("Only a subagent has a copy.".into())) };
    let branch = of.branch.clone().unwrap_or_else(|| rules::branch_name(&of.root, &of.number));
    let data = &shared.runs.data_dir;
    // What it copies: the copy of the kitten it follows (when that is one and still there), else the folder its parent works in.
    let (source, repo) = {
        let core = lock(&shared.core);
        let before = of
            .after
            .as_ref()
            .and_then(|n| core.store.children_of(&of.parent).ok()?.into_iter().find(|c| c.subagent.as_ref().is_some_and(|s| &s.number == n && s.placement == SubPlace::OwnCopy)))
            .and_then(|c| core.store.place(&c.id).ok().flatten())
            .filter(|p| std::path::Path::new(&p.workdir).is_dir());
        match before {
            Some(p) => (Some(p), None),
            None => (None, Some(of.parent.clone())),
        }
    };
    let place = match (source, repo) {
        (Some(p), _) => p,
        (None, Some(parent)) => {
            // The parent's folder, made again first when it was tidied away while nothing worked in it.
            let owner = folder_task(shared, &parent);
            let p = lock(&shared.core).store.place(&parent).ok().flatten().ok_or_else(|| PlaceError::Other("The agent that started this subagent has no folder to copy.".into()))?;
            let stored = lock(&shared.core).store.place(&owner).ok().flatten().unwrap_or_else(|| p.clone());
            workplace::reopen(data, &owner, &stored)?;
            stored
        }
        (None, None) => return Err(PlaceError::Other("There is nothing to copy.".into())),
    };
    let from = std::path::PathBuf::from(&place.workdir);
    let Some(root) = workplace::copy_source(&from) else { return Err(PlaceError::Other("The folder this subagent copies is not a git repository with a commit.".into())) };
    let base = workplace::copy_base(data, &root, &shared.runs.undo_limits).map_err(|why| PlaceError::Other(format!("Neko could not copy the project for this subagent: {why}.")))?;
    workplace::prepare_copy(data, &task.id, &branch, &from, &base, place.repo.as_deref())
}

/// A subagent of this run's task by the number the agent names (`1`, `1.2`, or "Subagent 1").
fn child(shared: &Shared, run: &McpRun, id: &str) -> Option<Task> {
    let want = id.trim().trim_start_matches("Subagent").trim_start_matches("Kitten").trim();
    let core = lock(&shared.core);
    let found = core.store.children_of(&run.task).ok()?.into_iter().find(|c| c.subagent.as_ref().is_some_and(|s| s.number == want))?;
    // As screens see it: with its question, when it asked one.
    core.store.get_task(&found.id).ok().flatten()
}

fn state_words(t: &Task) -> &'static str {
    match t.status {
        TaskStatus::New if t.subagent.as_ref().is_some_and(|s| s.draft) => "a draft: waiting for the person to start it",
        TaskStatus::New => "queued",
        TaskStatus::Working | TaskStatus::NeedsYou => "working",
        TaskStatus::Waiting => "waiting on its own subagents",
        TaskStatus::Done => "done",
        TaskStatus::Failed => "failed",
        TaskStatus::Stopped if t.subagent.as_ref().is_some_and(|s| s.discarded) => "discarded by the person before it ran",
        TaskStatus::Stopped => "stopped",
        TaskStatus::Interrupted => "interrupted",
        TaskStatus::Asked => "asked you a question and waits for your answer (message_subagent)",
    }
}

/// Where it works, for the agent: "own copy neko/4f2a-1" or "shared folder".
fn place_words(t: &Task) -> String {
    match t.subagent.as_ref() {
        Some(s) if s.placement == SubPlace::OwnCopy => format!("own copy {}", s.branch.as_deref().unwrap_or("")).trim().to_owned(),
        _ => "shared folder".to_owned(),
    }
}

fn list(shared: &Arc<Shared>, run: &McpRun, args: &Value) -> Value {
    if let Some(id) = args["id"].as_str() {
        let Some(c) = child(shared, run, id) else { return answer(format!("You have no subagent {id}."), true) };
        let words = lock(&shared.core).store.final_words(&c.id).unwrap_or_default();
        let number = c.subagent.as_ref().map(|s| s.number.clone()).unwrap_or_default();
        let body = if words.is_empty() { "It has not said anything yet.".to_owned() } else { words };
        return answer(format!("Subagent {number} (\"{}\"), {}, {}:\n\n{body}", c.title, state_words(&c), place_words(&c)), false);
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
            let place = place_words(c);
            let after = c.subagent.as_ref().and_then(|s| s.after.as_ref()).map(|a| format!(" · after Subagent {a}")).unwrap_or_default();
            if last.is_empty() { format!("Subagent {number} · {} · {} · {place}{after}", c.title, state_words(c)) } else { format!("Subagent {number} · {} · {} · {place}{after} · {last}", c.title, state_words(c)) }
        })
        .collect();
    answer(format!("{}\n\nDo not call this to wait: Neko wakes you with each report.", lines.join("\n")), false)
}

fn stop_one(shared: &Arc<Shared>, run: &McpRun, args: &Value) -> Value {
    let Some(id) = args["id"].as_str() else { return answer("stop_subagent needs the subagent's number.", true) };
    let Some(c) = child(shared, run, id) else { return answer(format!("You have no subagent {id}."), true) };
    if c.status == TaskStatus::Asked {
        // It waits on your answer: stopping it takes its question back. Its report was the question, so nothing more wakes you.
        stop_tree(shared, &c.id);
        return answer(format!("Subagent {id} is stopped, and its question with it. Nothing more comes from it."), false);
    }
    if !rules::is_live(c.status) || c.subagent.as_ref().is_some_and(|s| s.discarded) {
        return answer(format!("Subagent {id} has already ended."), false);
    }
    stop_tree(shared, &c.id);
    answer(format!("Subagent {id} is stopped. You are woken with its report."), false)
}

// ---- Questions go up ----------------------------------------------------------------------------------------------------------------

/// `message_subagent {id, text, mode?}`: the agent writes to a kitten it started. At rest (it asked, or it ended) the kitten starts
/// again with the words (an answer starts at once: it was counted when it began, and it is the thing the kitten waits for); while it
/// works, the words are steered in, queued for its next turn or sent as an interrupt, as the person's are to a thread. A note in the
/// agent's own conversation says so. The words approve nothing: the kitten's risky steps still go to the person.
fn message(shared: &Arc<Shared>, run: &McpRun, args: &Value) -> Value {
    let (Some(id), Some(text)) = (args["id"].as_str(), args["text"].as_str()) else { return answer("message_subagent needs the subagent's number and the text.", true) };
    let text = text.trim();
    if text.is_empty() {
        return answer("message_subagent needs words to send.", true);
    }
    if text.len() > neko_protocol::MAX_WORDS_BYTES {
        return answer("That message is too long. Put the long part in a file and name the file.", true);
    }
    let mode = match args["mode"].as_str() {
        None | Some("steer") => SendMode::Steer,
        Some("queue") => SendMode::Queue,
        Some("interrupt") => SendMode::Interrupt,
        Some(other) => return answer(format!("Mode {other} is not one of steer, queue and interrupt."), true),
    };
    if lock(&shared.runs.active).get(&run.task).is_some_and(|a| a.cancelled.load(Ordering::SeqCst)) {
        return answer("The person stopped this thread, so nothing was sent.", true);
    }
    let Some(c) = child(shared, run, id) else { return answer(format!("You have no subagent {id}."), true) };
    let Some(of) = c.subagent.clone() else { return answer(format!("You have no subagent {id}."), true) };
    let n = of.number.clone();
    if of.discarded {
        return answer(format!("Subagent {n} was discarded by the person before it ran."), true);
    }
    if of.draft {
        return answer(format!("Subagent {n} is a draft: nothing runs it yet. Start it with start_drafts first."), true);
    }
    if c.status == TaskStatus::New {
        return answer(format!("Subagent {n} has not started yet: it reads its prompt first. Write to it once it works."), true);
    }
    let answering = c.status == TaskStatus::Asked;
    let running = lock(&shared.runs.active).contains_key(&c.id);
    if let Reply::Error { message, .. } = crate::queue::send(shared, c, text.to_owned(), mode) {
        return answer(format!("Neko could not write to Subagent {n}: {message}"), true);
    }
    {
        let mut b = lock(&run.book);
        let mut core = lock(&shared.core);
        let now = core.now();
        if let Ok(outs) = b.to_subagent_note(&mut core, &n, text, answering, now) {
            runs::push(shared, &run.task, outs, run.control.as_ref());
        }
    }
    poke(shared);
    let end = " Neko wakes you with its report when it ends. Do not poll or wait. If you have nothing else to do, end your turn now.";
    let said = match (answering, running, mode) {
        (true, _, _) => format!("Subagent {n} has your answer and is working again.{end}"),
        (false, false, _) => format!("Subagent {n} is working again with your message.{end}"),
        (false, true, SendMode::Steer) => format!("Subagent {n} reads your message at its next step.{end}"),
        (false, true, SendMode::Queue) => format!("Subagent {n} gets your message when its turn ends.{end}"),
        (false, true, SendMode::Interrupt) => format!("Subagent {n}'s turn is cut short, and it starts again with your message.{end}"),
    };
    answer(said, false)
}

/// `ask_parent {question, subagent?}` (a kitten) and `ask_person {question, subagent?}` (a thread): the run carries the question and
/// ends `asked`. Naming a kitten passes its question up one level: the chain says whose it was. A kitten's question reaches the person
/// only through each agent above it, one level at a time.
fn ask(shared: &Arc<Shared>, run: &McpRun, args: &Value, to: AskTo) -> Value {
    let tool = if to == AskTo::Parent { "ask_parent" } else { "ask_person" };
    let Some(question) = args["question"].as_str().map(str::trim).filter(|q| !q.is_empty()) else { return answer(format!("{tool} needs the question, in words."), true) };
    if question.chars().count() > rules::QUESTION_CHARS {
        return answer("That question is too long. Put the detail in a file, name the file, and ask in a few lines.", true);
    }
    let Some(me) = lock(&shared.core).store.get_task(&run.task).ok().flatten() else { return answer("This thread is gone.", true) };
    match (to, me.subagent.is_some()) {
        (AskTo::Person, true) => return answer("You are a subagent: put your question to the agent that started you with ask_parent.", true),
        (AskTo::Parent, false) => return answer("You are not a subagent, so there is no agent above you to ask: ask the person with ask_person.", true),
        _ => {}
    }
    if lock(&shared.runs.active).get(&run.task).is_some_and(|a| a.cancelled.load(Ordering::SeqCst)) {
        return answer("The person stopped this thread, so nothing was asked.", true);
    }
    let mut chain = Vec::new();
    let mut passing = None;
    if let Some(n) = args["subagent"].as_str().map(str::trim).filter(|n| !n.is_empty()) {
        let Some(c) = child(shared, run, n) else { return answer(format!("You have no subagent {n}."), true) };
        let Some(q) = c.subagent.as_ref().and_then(|s| s.question.clone()) else {
            return answer(format!("Subagent {n} has not asked anything, so there is nothing to pass up. Leave out subagent to ask a question of your own."), true);
        };
        let number = c.subagent.as_ref().map(|s| s.number.clone()).unwrap_or_default();
        chain = q.chain;
        chain.push(number.clone());
        passing = Some(number);
    }
    {
        let mut b = lock(&run.book);
        let mut core = lock(&shared.core);
        let now = core.now();
        match b.set_ask(&mut core, RunAsk { to, text: question.to_owned(), chain, unanswered: false }, now) {
            Ok(outs) => runs::push(shared, &run.task, outs, run.control.as_ref()),
            Err(e) => return answer(format!("Neko could not keep the question: {e}"), true),
        }
    }
    answer(rules::asked_text(to, passing.as_deref()), false)
}
