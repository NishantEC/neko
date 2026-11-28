//! Words typed while the agent works (docs/knowledge/composer.md): steered into the running agent, queued for the next run, or sent
//! as an interrupt. The daemon keeps a list per thread (`neko_core::queue`, stored) that screens show above the composer, and decides
//! when each message goes: a queued one when the run ends well, an interrupting one when the turn it stopped is over.
//!
//! Locks: the core is locked and released, then `runs.active`, never the other way round with the core held inside `active`.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use neko_core::Core;
use neko_core::queue::MAX_WAITING;
use neko_core::runtime::Control;
use neko_protocol::{Change, ErrorCode, Queued, QueuedId, QueuedKind, Reply, SendMode, ServerMessage, Task, TaskId, TaskStatus};

use crate::runs;
use crate::server::{Shared, lock};

/// How long a CLI has to end the turn after an interrupt before it is stopped. `NEKO_INTERRUPT_GRACE_MS` shortens it in tests.
fn grace() -> Duration {
    Duration::from_millis(std::env::var("NEKO_INTERRUPT_GRACE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(5_000))
}

/// Pushes a thread's whole waiting list, as the store has it after the change that made `revision`.
fn changed(shared: &Shared, task: &TaskId, revision: u64) {
    let queued = lock(&shared.core).store.queued_of(task).unwrap_or_default();
    shared.broadcast(&ServerMessage::Event { revision, change: Change::QueueChanged { task: task.clone(), queued } });
}

/// Puts a message on a thread's list. None when the thread already holds [`MAX_WAITING`].
fn add(shared: &Shared, task: &TaskId, words: &str, kind: QueuedKind) -> Option<Queued> {
    let mut core = lock(&shared.core);
    if core.store.queued_of(task).map_or(0, |q| q.len()) >= MAX_WAITING {
        return None;
    }
    let now = core.now();
    let q = Queued { id: QueuedId(core.new_id("queued")), task: task.clone(), words: words.to_owned(), kind, at_ms: now };
    let revision = core.store.insert_queued(&q, now).ok()?;
    drop(core);
    changed(shared, task, revision);
    Some(q)
}

/// Takes a message off the list. Whether it was there.
fn remove(shared: &Shared, task: &TaskId, id: &QueuedId, why: &str) -> bool {
    let mut core = lock(&shared.core);
    let now = core.now();
    let Ok(Some(revision)) = core.store.delete_queued(id, why, now) else { return false };
    drop(core);
    changed(shared, task, revision);
    true
}

fn person(shared: &Shared, task: &TaskId, words: &str) -> Option<neko_protocol::PartId> {
    let mut core = lock(&shared.core);
    let outs = neko_core::conductor::person_part(&mut core, task, words).ok()?;
    let id = outs.iter().find_map(|o| match o {
        neko_core::conductor::Out::Change(_, Change::PartChanged { part }) => Some(part.id.clone()),
        _ => None,
    });
    runs::push(shared, task, outs, None);
    id
}

const FULL: &str = "Too many messages are waiting. Send or discard some first.";

/// Words in a thread's composer. At rest every mode starts a run that resumes the conversation; while it works: `Steer` writes them
/// into the running agent, `Queue` holds them for the next run, `Interrupt` stops the turn and starts a new one with them.
pub fn send(shared: &Arc<Shared>, task: Task, words: String, mode: SendMode) -> Reply {
    if words.trim().is_empty() {
        return Reply::error(ErrorCode::InvalidWords, "Type something first.");
    }
    let running = lock(&shared.runs.active).get(&task.id).map(|a| a.control.clone());
    match (running, mode) {
        (None, _) => match runs::start(shared, task, Some(words)) {
            Ok(()) => Reply::Ok,
            Err(r) => r,
        },
        (Some(_), SendMode::Queue) => for_next_run(shared, &task.id, &words),
        (Some(None), _) => Reply::error(ErrorCode::Busy, "Neko is still starting this thread's agent."),
        // The agent's turn is over and its input closed; the run is only ending. Nothing can reach it now, so the words wait for the
        // next run, which starts as soon as this one is released.
        (Some(Some(c)), _) if !c.takes_words() => for_next_run(shared, &task.id, &words),
        (Some(Some(c)), SendMode::Steer) => steer(shared, &task.id, &c, &words),
        (Some(Some(c)), SendMode::Interrupt) => interrupt(shared, &task.id, &c, &words),
    }
}

/// Writes the words into the running agent. The transcript has them at once; the row above the composer stays until the agent's next
/// step reads them (`step_began`).
fn steer(shared: &Arc<Shared>, task: &TaskId, c: &Arc<dyn Control>, words: &str) -> Reply {
    if let Err(e) = c.steer(words) {
        // Its input closed between the look and the write (the turn's last `result` came): the words wait for the next run.
        if !c.takes_words() {
            return for_next_run(shared, task, words);
        }
        return Reply::error(ErrorCode::Busy, e.0);
    }
    let part = person(shared, task, words);
    if let Some(q) = add(shared, task, words, QueuedKind::Steered) {
        // A message the agent was already reading when this was written cannot include it: it waits for the next one.
        if let Some(a) = lock(&shared.runs.active).get_mut(task) {
            let armed = !a.open_step;
            a.steers.push((q.id, armed, part));
        }
    }
    Reply::Ok
}

/// Holds the words as a queued message for the next run (docs/knowledge/composer.md). When the run is released before it looked at
/// the list (`after_run` already found it empty), nothing else would start them, so they start a run now.
fn for_next_run(shared: &Arc<Shared>, task: &TaskId, words: &str) -> Reply {
    let Some(q) = add(shared, task, words, QueuedKind::Queued) else { return Reply::error(ErrorCode::Refused, FULL) };
    if lock(&shared.runs.active).contains_key(task) {
        return Reply::Ok;
    }
    let Some(t) = lock(&shared.core).store.get_task(task).ok().flatten() else { return Reply::Ok };
    // Refused only because a run started meanwhile (`after_run`, or another screen): it, or the next end, takes the message.
    if runs::start(shared, t, Some(q.words.clone())).is_ok() {
        remove(shared, task, &q.id, "sent");
    }
    Reply::Ok
}

/// Stops the turn and starts a new one with the words. The row says so until it starts. A second interrupt while one is going waits
/// its turn like a queued message.
fn interrupt(shared: &Arc<Shared>, task: &TaskId, c: &Arc<dyn Control>, words: &str) -> Reply {
    let first = {
        let mut active = lock(&shared.runs.active);
        match active.get_mut(task) {
            Some(a) if !a.interrupting => {
                a.interrupting = true;
                Some(a.cancelled.clone())
            }
            _ => None,
        }
    };
    let Some(mine) = first else {
        return match add(shared, task, words, QueuedKind::Queued) {
            Some(_) => Reply::Ok,
            None => Reply::error(ErrorCode::Refused, FULL),
        };
    };
    if add(shared, task, words, QueuedKind::Interrupting).is_none() {
        if let Some(a) = lock(&shared.runs.active).get_mut(task) {
            a.interrupting = false;
        }
        return Reply::error(ErrorCode::Refused, FULL);
    }
    c.interrupt();
    // A CLI that does not end the turn in time is stopped; the words still start the next run.
    let (s, id, control) = (shared.clone(), task.clone(), c.clone());
    let _ = std::thread::Builder::new().name("neko-interrupt".into()).spawn(move || {
        let until = Instant::now() + grace();
        while Instant::now() < until {
            if !lock(&s.runs.active).get(&id).is_some_and(|a| Arc::ptr_eq(&a.cancelled, &mine)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if lock(&s.runs.active).get(&id).is_some_and(|a| Arc::ptr_eq(&a.cancelled, &mine)) {
            control.stop();
        }
    });
    Reply::Ok
}

/// Send now: the waiting message goes into the running agent at once (steered), or starts a run when none is going.
pub fn send_now(shared: &Arc<Shared>, task: Task, id: &QueuedId) -> Reply {
    let Some(q) = lock(&shared.core).store.get_queued(id).ok().flatten().filter(|q| q.task == task.id) else {
        return Reply::error(ErrorCode::NotFound, "That message was already sent.");
    };
    if q.kind != QueuedKind::Queued {
        return Reply::error(ErrorCode::Refused, "That message is already on its way.");
    }
    let running = lock(&shared.runs.active).get(&task.id).map(|a| a.control.clone());
    match running {
        Some(None) => Reply::error(ErrorCode::Busy, "Neko is still starting this thread's agent."),
        Some(Some(c)) => {
            if !remove(shared, &task.id, id, "sent now") {
                return Reply::error(ErrorCode::NotFound, "That message was already sent.");
            }
            match steer(shared, &task.id, &c, &q.words) {
                Reply::Ok => Reply::Ok,
                other => {
                    // The agent would not take it: it goes back on the list.
                    let _ = add(shared, &task.id, &q.words, QueuedKind::Queued);
                    other
                }
            }
        }
        None => match runs::start(shared, task.clone(), Some(q.words.clone())) {
            Ok(()) => {
                remove(shared, &task.id, id, "sent now");
                Reply::Ok
            }
            Err(r) => r,
        },
    }
}

/// Discard, and Edit (which puts the words back in the composer): the message leaves the list. A steered one is already in the agent.
pub fn discard(shared: &Shared, task: &TaskId, id: &QueuedId) -> Reply {
    let Some(q) = lock(&shared.core).store.get_queued(id).ok().flatten().filter(|q| &q.task == task) else {
        return Reply::error(ErrorCode::NotFound, "That message was already sent.");
    };
    if q.kind == QueuedKind::Steered {
        return Reply::error(ErrorCode::Refused, "The agent already has that message.");
    }
    if q.kind == QueuedKind::Interrupting {
        return Reply::error(ErrorCode::Refused, "That message is stopping the agent's turn and starts the next one.");
    }
    if remove(shared, task, id, "discarded") { Reply::Ok } else { Reply::error(ErrorCode::NotFound, "That message was already sent.") }
}

/// A model call began: the steered messages written before the previous one ended are in it. Each moves in the transcript to where
/// the agent read it, so what the agent says next stands after the person's words (it was sent while a step was still going).
pub fn step_began(shared: &Shared, task: &TaskId) {
    let read: Vec<(QueuedId, Option<neko_protocol::PartId>)> = {
        let mut active = lock(&shared.runs.active);
        let Some(a) = active.get_mut(task) else { return };
        a.open_step = true;
        let (armed, wait): (Vec<_>, Vec<_>) = std::mem::take(&mut a.steers).into_iter().partition(|(_, armed, _)| *armed);
        a.steers = wait;
        armed.into_iter().map(|(id, _, part)| (id, part)).collect()
    };
    for (id, part) in read {
        remove(shared, task, &id, "read");
        if let Some(p) = part {
            let moved = {
                let mut core = lock(&shared.core);
                let now = core.now();
                core.store.move_part_to_end(&p, now).ok().flatten()
            };
            if let Some((part, revision)) = moved {
                shared.broadcast(&neko_protocol::ServerMessage::Event { revision, change: Change::PartChanged { part } });
            }
        }
    }
}

/// That model call is over: a message written while it ran is read by the next.
pub fn step_ended(shared: &Shared, task: &TaskId) {
    if let Some(a) = lock(&shared.runs.active).get_mut(task) {
        a.open_step = false;
        for s in &mut a.steers {
            s.1 = true;
        }
    }
}

/// A run is over: nothing is waiting to be read any more.
fn settle_steered(shared: &Shared, task: &TaskId) {
    let mut core = lock(&shared.core);
    let now = core.now();
    if let Ok(Some(revision)) = core.store.delete_queued_kind(task, QueuedKind::Steered, "run ended", now) {
        drop(core);
        changed(shared, task, revision);
    }
}

/// A run ended: the steers it did not read are done with, and the next waiting message starts a run when this one ended well (done, a
/// question, or the interrupt that makes room for the words). After a stop or a failure the list is held for the person: nothing is
/// sent that they did not ask for, and an interrupt that was stopped becomes a queued message. Returns whether a run was started.
pub fn after_run(shared: &Arc<Shared>, task: &TaskId) -> bool {
    if shared.stopping.load(Ordering::SeqCst) {
        return false;
    }
    settle_steered(shared, task);
    let (status, waiting) = {
        let core = lock(&shared.core);
        let Some(t) = core.store.get_task(task).ok().flatten() else { return false };
        (t, core.store.queued_of(task).unwrap_or_default())
    };
    if waiting.is_empty() {
        return false;
    }
    let go = matches!(status.status, TaskStatus::Done | TaskStatus::Asked | TaskStatus::Interrupted);
    if !go {
        hold(shared, task, &waiting);
        return false;
    }
    // The interrupt's words first, then the oldest queued.
    let Some(next) = waiting.iter().find(|q| q.kind == QueuedKind::Interrupting).or_else(|| waiting.iter().find(|q| q.kind == QueuedKind::Queued)).cloned() else { return false };
    match runs::start(shared, status, Some(next.words.clone())) {
        Ok(()) => {
            remove(shared, task, &next.id, "sent");
            hold(shared, task, &waiting.into_iter().filter(|q| q.id != next.id).collect::<Vec<_>>());
            true
        }
        Err(_) => {
            // A run started meanwhile (the person sent words at rest): it takes the next turn.
            hold(shared, task, &waiting);
            false
        }
    }
}

/// Interrupts that will not go ahead are held as queued messages.
fn hold(shared: &Shared, task: &TaskId, waiting: &[Queued]) {
    for q in waiting.iter().filter(|q| q.kind == QueuedKind::Interrupting) {
        let mut core = lock(&shared.core);
        let now = core.now();
        if let Ok(Some(revision)) = core.store.set_queued_kind(&q.id, QueuedKind::Queued, now) {
            drop(core);
            changed(shared, task, revision);
        }
    }
}

/// At a start and at a stop: what was being read or interrupted when the daemon went is not any more. Steered messages were written
/// (the transcript has them); an interrupt that never went through is held for the person.
pub fn settle_at_stop(core: &mut Core) {
    let Ok(all) = core.store.queued_all() else { return };
    let now = core.now();
    for q in all {
        match q.kind {
            QueuedKind::Steered => {
                let _ = core.store.delete_queued(&q.id, "daemon stopped", now);
            }
            QueuedKind::Interrupting => {
                let _ = core.store.set_queued_kind(&q.id, QueuedKind::Queued, now);
            }
            QueuedKind::Queued => {}
        }
    }
}
