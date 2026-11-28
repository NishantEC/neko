//! The Search palette's scenes (docs/knowledge/search.md): nothing typed (the recent threads and the actions), text typed with threads, a
//! kitten, a Cattain and a project found, a thread found by what was said in it, filters, no results, and the palette while the daemon is
//! still answering. Placeholder data only.

use neko_protocol::{Found, FoundBy, PartId, SubagentOf, TaskId, TaskStatus};
use rune::TextEdit;

use super::{task, with_workspaces};
use crate::state::App;

fn open(s: &mut App, text: &str, hits: Vec<Found>) {
    if s.tasks.is_empty() {
        with_workspaces(s);
    }
    s.palette.open = true;
    s.palette.query = TextEdit::new(text);
    s.palette.query.focused = true;
    s.palette.hits = hits;
    s.palette.answered = Some(text.to_owned());
    s.palette.sent = Some(text.to_owned());
}

fn id_of(s: &App, starts: &str) -> TaskId {
    s.tasks.iter().find(|t| t.title.starts_with(starts)).map(|t| t.id.clone()).unwrap_or_else(|| panic!("no sample thread starting {starts:?}"))
}

fn by_title(s: &App, starts: &str) -> Found {
    Found { task: id_of(s, starts), kitten: false, by: FoundBy::Title, snippet: String::new(), part: None, seq: 0 }
}

fn recent(s: &App, starts: &str) -> Found {
    Found { by: FoundBy::Recent, ..by_title(s, starts) }
}

fn said(s: &App, starts: &str, snippet: &str) -> Found {
    Found { task: id_of(s, starts), kitten: false, by: FoundBy::Message, snippet: snippet.into(), part: Some(PartId("part_1".into())), seq: 3 }
}

/// A kitten of the thread whose title starts so; it joins the sample threads.
fn kitten(s: &mut App, of: &str, number: &str, title: &str, minutes: i64, status: TaskStatus) -> Found {
    let parent = s.tasks.iter().find(|t| t.title.starts_with(of)).cloned().expect("the sample thread");
    let mut k = task(90 + number.len() as u32 + minutes as u32, title, minutes);
    k.title = title.into();
    k.workspace = parent.workspace.clone();
    k.folder = parent.folder.clone();
    k.status = status;
    k.subagent = Some(SubagentOf { parent: parent.id.clone(), number: number.into(), root: parent.id.clone() });
    let id = k.id.clone();
    s.tasks.push(k);
    s.tasks.sort_by_key(|t| std::cmp::Reverse(t.created_ms));
    Found { task: id, kitten: true, by: FoundBy::Title, snippet: String::new(), part: None, seq: 0 }
}

/// Nothing typed: the six newest threads under Recent (one needs you, one works), then the actions.
pub fn empty(s: &mut App) {
    with_workspaces(s);
    let hits = ["Answer the question about the retry limit", "Make the hero image load faster", "Draft the release notes", "Rename the Settings button", "Find out why the nightly build", "Write the pricing page copy"].iter().map(|w| recent(s, w)).collect();
    open(s, "", hits);
}

/// "mo": a thread by its title, one by what was said in it, a kitten, a Cattain and a project.
pub fn typing(s: &mut App) {
    with_workspaces(s);
    s.workspaces.iter_mut().find(|w| w.id == "ws_notes").map(|w| {
        w.name = "mochi-notes".into();
        w.missing = false;
    });
    let k = kitten(s, "Find out why the nightly build", "1", "Move the retry settings into one file", 40, TaskStatus::Done);
    let hits = vec![by_title(s, "Move the blog to the new layout"), said(s, "Fix the contact form on Safari", "…the handler moves focus to the first field with an error, not the form…"), k];
    open(s, "mo", hits);
}

/// "retry": a thread by its title and two by what was said, one of them working.
pub fn message(s: &mut App) {
    with_workspaces(s);
    let hits = vec![by_title(s, "Answer the question about the retry limit"), said(s, "Find out why the nightly build", "…the retry loop sends every call again with no pause between the tries…"), said(s, "Make the hero image load faster", "…the same retry logic is used when the image fails to load on a slow phone…")];
    open(s, "retry", hits);
}

/// "set": only actions (and one Cattain-free list), as a command palette.
pub fn actions(s: &mut App) {
    open(s, "set", Vec::new());
}

/// `status:needs`: the threads that wait for the person, found by a filter alone.
pub fn filter(s: &mut App) {
    with_workspaces(s);
    let hits = vec![recent(s, "Answer the question about the retry limit")];
    open(s, "status:needs", hits);
}

/// A filter that cannot be read says why.
pub fn problem(s: &mut App) {
    open(s, "status:sleepy", Vec::new());
    s.palette.problem = Some("“sleepy” is not a status. Try working, waiting, failed, needs, idle, settled, archived or pinned.".into());
}

/// Nothing matches.
pub fn none(s: &mut App) {
    open(s, "zzzz", Vec::new());
}

/// Settled and archived threads are found and say so at their right.
pub fn put_away(s: &mut App) {
    with_workspaces(s);
    super::shelve(s, "Port the backup script", |t| t.archived = true);
    super::shelve(s, "Sort the meeting notes", |t| t.settled = true);
    let hits = vec![by_title(s, "Port the backup script"), by_title(s, "Sort the meeting notes"), by_title(s, "Draft the release notes")];
    open(s, "no", hits);
}

/// The daemon is still answering for "mon": the rows of "mo" stay in place and the field spins.
pub fn loading(s: &mut App) {
    typing(s);
    s.palette.query = TextEdit::new("mon");
    s.palette.query.focused = true;
    s.palette.sent = Some("mon".into());
    s.palette.in_flight = Some(7);
}

/// The first moment of the palette with nothing yet known: a spinner, and "Searching…" where the rows will be.
pub fn first_moment(s: &mut App) {
    with_workspaces(s);
    s.palette.open = true;
    s.palette.query = TextEdit::new("login");
    s.palette.query.focused = true;
    s.palette.sent = Some("login".into());
    s.palette.in_flight = Some(3);
}
