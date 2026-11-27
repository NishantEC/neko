//! The composer scenes (docs/knowledge/composer.md): the agent working with nothing waiting, with words typed (the mode and the key
//! hints), with steered and queued messages above the box, four queued ones folded, an edit in progress, an interrupt in progress, and
//! the activity bar closed and open over a thread whose kittens work. Placeholder data only.

use neko_protocol::{Queued, QueuedId, QueuedKind, TaskId};
use rune::TextEdit;

use super::subagents;
use crate::state::App;

fn queued(task: &TaskId, n: u32, kind: QueuedKind, words: &str) -> Queued {
    Queued { id: QueuedId(format!("queued_{n}")), task: task.clone(), words: words.into(), kind, at_ms: n as i64 }
}

/// The renamed-button thread at work (the same run as `task-working`).
fn working(s: &mut App) -> TaskId {
    super::task_working(s);
    s.open.clone().unwrap_or(TaskId(String::new()))
}

fn put(s: &mut App, id: &TaskId, q: Vec<Queued>) {
    if let Some(b) = s.books.get_mut(id) {
        b.queued = q;
    }
}

pub fn idle(s: &mut App) {
    super::task_done(s);
}

pub fn working_idle(s: &mut App) {
    working(s);
}

pub fn typing(s: &mut App) {
    let id = working(s);
    s.replies.insert(id, TextEdit::new("Also rename it in the help pages"));
}

pub fn two_queued(s: &mut App) {
    let id = working(s);
    let q = vec![
        queued(&id, 1, QueuedKind::Steered, "Use the word Preferences, not Settings, in the toolbar"),
        queued(&id, 2, QueuedKind::Queued, "Then run the tests for the labels crate"),
        queued(&id, 3, QueuedKind::Queued, "Add a line to the changelog about the new name"),
    ];
    put(s, &id, q);
}

pub fn queued_folded(s: &mut App) {
    let id = working(s);
    let q = vec![
        queued(&id, 1, QueuedKind::Queued, "Then run the tests for the labels crate"),
        queued(&id, 2, QueuedKind::Queued, "Add a line to the changelog about the new name"),
        queued(&id, 3, QueuedKind::Queued, "Check the help pages for the old name"),
        queued(&id, 4, QueuedKind::Queued, "Open a draft pull request when it all passes"),
    ];
    put(s, &id, q);
}

pub fn queued_folded_open(s: &mut App) {
    queued_folded(s);
    s.composer.queue_open = true;
}

pub fn editing(s: &mut App) {
    let id = working(s);
    let q = vec![queued(&id, 2, QueuedKind::Queued, "Add a line to the changelog about the new name")];
    put(s, &id, q);
    let mut e = TextEdit::new("Then run the tests for the labels crate, and the docs build too");
    e.focused = true;
    s.replies.insert(id, e);
}

pub fn interrupting(s: &mut App) {
    let id = working(s);
    let q = vec![queued(&id, 1, QueuedKind::Interrupting, "Stop that. Rename it to Preferences everywhere instead"), queued(&id, 2, QueuedKind::Queued, "Then run the tests for the labels crate")];
    put(s, &id, q);
}

pub fn activity_collapsed(s: &mut App) {
    subagents::cards(s);
}

pub fn activity_open(s: &mut App) {
    subagents::cards(s);
    s.composer.activity_open = true;
}

pub fn activity_with_queue(s: &mut App) {
    subagents::cards(s);
    s.composer.activity_open = true;
    let id = s.open.clone().unwrap_or(TaskId(String::new()));
    let q = vec![queued(&id, 1, QueuedKind::Queued, "And tell me which pages have the dead links")];
    put(s, &id, q);
}
