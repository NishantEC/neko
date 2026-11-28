//! A Cattain runs its crew, in the daemon (docs/knowledge/cats.md, "The crew"): Neko's crew tools for a Cattain's own thread
//! (`start_crew_thread`, `crew`, `message_crew`), the crew threads that wait for room, and the news that wakes the Cattain: each crew
//! thread's turn end, and each request of one that waits on the person. The records and the words are in `neko_core::crew`; the
//! waking itself is the subagent hub's (`subagents.rs`), which delivers a Cattain's crew news like a kitten's report, batched over 30 s.
//!
//! Safety does not move: a crew thread asks the person before risky steps as any thread does, its access is never more than its
//! Cattain's (`runs.rs`), and no tool here answers a request or a question the thread put to the person.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use neko_core::crew::{self as rules, CrewAsk};
use neko_core::subagents::{self as kittens, WakeRow};
use neko_protocol::{Cat, Change, Reply, SendMode, ServerMessage, Task, TaskId, TaskStatus};
use serde_json::{Value, json};

use crate::heat::{self, Heat};
use crate::runs::{self, McpRun};
use crate::server::{Shared, lock};

fn answer(text: impl Into<String>, error: bool) -> Value {
    json!({"text": text.into(), "error": error})
}

/// The Cattain whose own thread this task is (a kitten of it is not).
fn cattain_of(core: &neko_core::Core, task: &TaskId) -> Option<Cat> {
    let t = core.store.get_task(task).ok().flatten()?;
    if !t.cattain || t.subagent.is_some() {
        return None;
    }
    core.store.get_cat(t.cat.as_deref()?).ok().flatten()
}

/// Whether this run's task is a Cattain's own thread: it is offered the crew tools.
pub fn may_run_crew(core: &neko_core::Core, task: &TaskId) -> bool {
    cattain_of(core, task).is_some()
}

fn own(shared: &Shared, run: &McpRun) -> Result<Cat, Value> {
    match cattain_of(&lock(&shared.core), &run.task) {
        Some(c) => Ok(c),
        None => Err(answer("Only a Cattain's own thread has a crew. Do the work yourself, or with subagents.", true)),
    }
}

fn stopped(shared: &Shared, run: &McpRun) -> bool {
    lock(&shared.runs.active).get(&run.task).is_some_and(|a| a.cancelled.load(Ordering::SeqCst))
}

/// Whether Neko may start one more run by itself now: under the runs across Neko, the account's limit not reached, the Mac not warm.
fn room(shared: &Shared) -> bool {
    lock(&shared.runs.active).len() < crate::subagents::max_runs() && !crate::subagents::paused(shared) && heat::level() == Heat::Ok
}

fn project_of(shared: &Shared, t: &Task) -> Option<String> {
    lock(&shared.core).store.get_workspace(&t.workspace).ok().flatten().filter(|w| !w.is_general()).map(|w| w.name)
}

/// `start_crew_thread {title, brief, project?, placement?}`: a thread in the Cattain's crew, started now when there is room (else it
/// waits, queued, and the hub starts it), its brief as its first message. Every screen hears it.
pub fn start_tool(shared: &Arc<Shared>, run: &McpRun, args: &Value) -> Value {
    let cat = match own(shared, run) {
        Ok(c) => c,
        Err(v) => return v,
    };
    let (Some(title), Some(brief)) = (args["title"].as_str(), args["brief"].as_str().or_else(|| args["prompt"].as_str())) else {
        return answer("start_crew_thread needs a title and a brief, both text.", true);
    };
    let in_folder = match args["placement"].as_str() {
        None | Some("worktree") => false,
        Some("folder") => true,
        Some(other) => return answer(format!("Placement {other} is not one of worktree and folder."), true),
    };
    if shared.stopping.load(Ordering::SeqCst) {
        return answer("Neko is stopping.", true);
    }
    if stopped(shared, run) {
        return answer("The person stopped this thread, so no crew thread was started.", true);
    }
    let made = {
        let mut core = lock(&shared.core);
        core.create_crew_thread(&cat.id, &CrewAsk { title, brief, project: args["project"].as_str(), in_folder })
    };
    let task = match made {
        Ok((task, rev)) => {
            shared.broadcast(&ServerMessage::Event { revision: rev, change: Change::TaskCreated { task: task.clone() } });
            task
        }
        Err(why) => return answer(why, true),
    };
    let started = room(shared) && runs::start(shared, task.clone(), None).is_ok();
    if !started {
        crate::subagents::poke(shared);
    }
    answer(rules::started_text(&task, project_of(shared, &task).as_deref(), started), false)
}

/// A crew thread of this Cattain by the id the agent names: the whole id, its last part, or (newest first) its exact title.
fn member(shared: &Shared, cat: &Cat, id: &str) -> Option<Task> {
    let want = id.trim().trim_matches('"').to_lowercase();
    if want.is_empty() {
        return None;
    }
    let all = lock(&shared.core).store.crew_of(&cat.id).unwrap_or_default();
    all.iter().find(|t| t.id.0.to_lowercase() == want || (want.len() >= 6 && t.id.0.to_lowercase().ends_with(&want))).or_else(|| all.iter().find(|t| t.title.to_lowercase() == want)).cloned()
}

/// `crew {id?}`: the Cattain's crew, a line each (newest first), or one thread's last words in full. Read-only.
pub fn list_tool(shared: &Arc<Shared>, run: &McpRun, args: &Value) -> Value {
    let cat = match own(shared, run) {
        Ok(c) => c,
        Err(v) => return v,
    };
    if let Some(id) = args["id"].as_str() {
        let Some(t) = member(shared, &cat, id) else { return answer(format!("Your crew has no thread {id}."), true) };
        let words = lock(&shared.core).store.final_words(&t.id).unwrap_or_default();
        let body = if words.is_empty() { "It has not said anything yet.".to_owned() } else { words };
        return answer(format!("\"{}\" ({}), {}:\n\n{body}", t.title, t.id.0, rules::state_words(&t)), false);
    }
    let all = lock(&shared.core).store.crew_of(&cat.id).unwrap_or_default();
    if all.is_empty() {
        return answer("Your crew has no threads yet. Start one with start_crew_thread.", false);
    }
    let shown = all.len().min(30);
    let mut lines: Vec<String> = all
        .iter()
        .take(shown)
        .map(|t| {
            let mut l = format!("{} · {} · {}", t.id.0, t.title, rules::state_words(t));
            if let Some(p) = project_of(shared, t) {
                l.push_str(&format!(" · project {p}"));
            }
            if t.started_by.is_none() {
                l.push_str(" · started by the person");
            }
            if t.round.is_some() {
                l.push_str(" · a round's");
            }
            if t.archived {
                l.push_str(" · archived");
            }
            let last = t.latest_line.chars().take(100).collect::<String>();
            if !last.is_empty() {
                l.push_str(&format!(" · {last}"));
            }
            l
        })
        .collect();
    if all.len() > shown {
        lines.push(format!("and {} older", all.len() - shown));
    }
    answer(format!("{}\n\nDo not call this to wait: Neko wakes you as each crew thread's turn ends.", lines.join("\n")), false)
}

/// `message_crew {id, text, mode?}`: the Cattain writes to a thread of its crew. Its words reach the thread inside `<neko-from>` (so
/// neither the thread's agent nor the person reads them as the person's): at rest it starts again with them, working it reads them as a
/// steer, a queued message or an interrupt. Refused for a question the thread put to the person: only the person answers it.
pub fn message_tool(shared: &Arc<Shared>, run: &McpRun, args: &Value) -> Value {
    let cat = match own(shared, run) {
        Ok(c) => c,
        Err(v) => return v,
    };
    let (Some(id), Some(text)) = (args["id"].as_str(), args["text"].as_str()) else { return answer("message_crew needs the crew thread's id and the text.", true) };
    let text = text.trim();
    if text.is_empty() {
        return answer("message_crew needs words to send.", true);
    }
    if text.len() > neko_protocol::MAX_WORDS_BYTES - 200 {
        return answer("That message is too long. Put the long part in a file and name the file.", true);
    }
    let mode = match args["mode"].as_str() {
        None | Some("steer") => SendMode::Steer,
        Some("queue") => SendMode::Queue,
        Some("interrupt") => SendMode::Interrupt,
        Some(other) => return answer(format!("Mode {other} is not one of steer, queue and interrupt."), true),
    };
    if stopped(shared, run) {
        return answer("The person stopped this thread, so nothing was sent.", true);
    }
    let Some(t) = member(shared, &cat, id) else { return answer(format!("Your crew has no thread {id}. Call crew to see their ids."), true) };
    let title = t.title.clone();
    if t.archived {
        return answer(format!("The person archived \"{title}\", so it takes no messages until they bring it back."), true);
    }
    if crate::subagents::waits_for_answer(shared, &t) {
        return answer(format!("\"{title}\" asked the person a question and waits for their answer; only they answer it. Put it under Needs you in DIGEST.md."), true);
    }
    let ran = lock(&shared.core).store.has_run(&t.id).unwrap_or(false);
    if t.status == TaskStatus::New && !ran {
        return answer(format!("\"{title}\" has not started yet: it reads its brief first. Write to it once it works."), true);
    }
    let running = lock(&shared.runs.active).contains_key(&t.id);
    if let Reply::Error { message, .. } = crate::queue::send(shared, t, rules::from_cattain(&cat.name, text), mode) {
        return answer(format!("Neko could not write to \"{title}\": {message}"), true);
    }
    crate::subagents::poke(shared);
    let end = " Neko wakes you when its turn ends. Do not poll or wait. If you have nothing else to do, end your turn now.";
    let said = match (running, mode) {
        (false, _) => format!("\"{title}\" is working again with your message.{end}"),
        (true, SendMode::Steer) => format!("\"{title}\" reads your message at its next step.{end}"),
        (true, SendMode::Queue) => format!("\"{title}\" gets your message when its turn ends.{end}"),
        (true, SendMode::Interrupt) => format!("\"{title}\"'s turn is cut short, and it starts again with your message.{end}"),
    };
    answer(said, false)
}

// ---- What wakes the Cattain -------------------------------------------------------------------------------------------------------

/// The Cattain a crew thread tells, and its own thread, when it hears its crew: active, its own thread made and run at least once (a
/// Cattain nobody has talked to costs nothing).
fn listener(core: &neko_core::Core, t: &Task) -> Option<TaskId> {
    if !rules::tells_its_cattain(t) {
        return None;
    }
    let cat = core.store.get_cat(t.cat.as_deref()?).ok().flatten().filter(Cat::is_active)?;
    let thread = cat.thread?;
    core.store.has_run(&thread).ok()?.then_some(thread)
}

/// A crew thread's turn ended (done, failed, stopped, interrupted, or with a question for the person): its Cattain is told once, by a
/// crew wake the hub delivers. A thread that rests waiting on its own kittens has not ended its turn.
pub fn turn_ended(shared: &Arc<Shared>, task: &Task) {
    if !matches!(task.status, TaskStatus::Done | TaskStatus::Failed | TaskStatus::Stopped | TaskStatus::Interrupted | TaskStatus::Asked) {
        return;
    }
    let (listener, run) = {
        let core = lock(&shared.core);
        let Some(l) = listener(&core, task) else { return };
        let Some(run) = core.store.runs_of(&task.id).ok().and_then(|mut r| r.pop()) else { return };
        if core.store.crew_wake_for_run(&task.id, &run.id).unwrap_or(true) {
            return;
        }
        (l, run)
    };
    let files = crate::subagents::files_of(shared, task);
    let mut core = lock(&shared.core);
    let now = core.now();
    let mut summary = core.store.final_words(&task.id).unwrap_or_default();
    if summary.is_empty() {
        summary = run.reason.clone().unwrap_or_else(|| "It ended without saying anything.".to_owned());
    }
    let question = if task.status == TaskStatus::Asked { core.store.question_of(&task.id).ok().flatten() } else { None };
    let w = WakeRow {
        id: core.new_id("wake"),
        task: listener,
        subagent: task.id.clone(),
        number: String::new(),
        title: task.title.clone(),
        status: kittens::status_word(task.status).to_owned(),
        summary,
        files,
        branch: None,
        created_ms: now,
        run: Some(run.id),
        question,
        chain: Vec::new(),
        crew: true,
        about: None,
    };
    let _ = core.store.insert_wake(&w, "pending", now);
}

/// Requests of crew threads that wait on the person and have not told their Cattain: each is told once ("needs_you"). One a Cattain
/// does not hear (retired, never talked to) is settled so it is not looked at again.
fn requests(shared: &Shared) {
    let mut core = lock(&shared.core);
    let Ok(asks) = core.store.crew_requests() else { return };
    for r in asks {
        let Some(t) = core.store.get_task(&r.thread).ok().flatten() else { continue };
        let to = listener(&core, &t);
        let now = core.now();
        let summary = if r.detail.trim().is_empty() { r.title.clone() } else { format!("{}: {}", r.title, r.detail.trim()) };
        let w = WakeRow {
            id: core.new_id("wake"),
            task: to.clone().unwrap_or_else(|| t.id.clone()),
            subagent: t.id.clone(),
            number: String::new(),
            title: t.title.clone(),
            status: "needs_you".to_owned(),
            summary,
            files: Vec::new(),
            branch: None,
            created_ms: now,
            run: None,
            question: None,
            chain: Vec::new(),
            crew: true,
            about: Some(r.permission),
        };
        let _ = core.store.insert_wake(&w, if to.is_some() { "pending" } else { "discarded" }, now);
    }
}

/// Crew threads that were made with no room start, in the order made, while there is room. Returns whether one waits for the Mac to
/// cool (the hub looks again later).
fn start_queued(shared: &Arc<Shared>) -> bool {
    let queued = lock(&shared.core).store.queued_crew().unwrap_or_default();
    for t in queued {
        if lock(&shared.runs.active).contains_key(&t.id) {
            continue;
        }
        if lock(&shared.runs.active).len() >= crate::subagents::max_runs() || crate::subagents::paused(shared) {
            return false;
        }
        if heat::level() != Heat::Ok {
            return true;
        }
        let _ = runs::start(shared, t, None);
    }
    false
}

/// The hub's pass, for crews: requests become wakes, queued crew threads start. Returns whether something waits for the Mac to cool.
pub fn pass(shared: &Arc<Shared>) -> bool {
    requests(shared);
    start_queued(shared)
}

/// Whether a Cattain's own thread waits on its crew: a crew thread that tells it has not finished. (A pending crew wake counts as for
/// any wake.)
pub fn has_live_crew(core: &neko_core::Core, task: &Task) -> bool {
    if !task.cattain || task.subagent.is_some() {
        return false;
    }
    let Some(cat) = task.cat.as_deref() else { return false };
    core.store.crew_of(cat).unwrap_or_default().iter().any(|t| rules::tells_its_cattain(t) && rules::is_live(t) && !t.archived)
}

/// How long a Cattain's crew wakes wait for each other: half a minute (`NEKO_CREW_BATCH_MS` shortens it in tests).
pub fn batch() -> Duration {
    Duration::from_millis(std::env::var("NEKO_CREW_BATCH_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(rules::BATCH_MS))
}

/// Whether this Cattain was woken by its crew as often as an hour allows (`NEKO_CREW_WAKES_PER_HOUR` changes it in tests). Its news
/// waits, kept, and the hub looks again in a minute.
pub fn over_budget(shared: &Shared, task: &TaskId) -> Option<Instant> {
    let most = std::env::var("NEKO_CREW_WAKES_PER_HOUR").ok().and_then(|v| v.parse().ok()).unwrap_or(rules::WAKES_PER_HOUR);
    let core = lock(&shared.core);
    let since = core.now() - 3_600_000;
    (core.store.crew_wakes_since(task, since).unwrap_or(0) >= most).then(|| Instant::now() + Duration::from_secs(60))
}
