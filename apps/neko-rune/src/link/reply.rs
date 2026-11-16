//! One [`Call`] made and its answer read: a short-lived connection to the daemon's socket (the
//! held snapshot connection in `link/mod.rs` keeps going on its own), and the daemon's answer
//! as the small thing the app applies.

use std::path::Path;
use std::sync::{Arc, Mutex};

use neko_protocol::checkpoints::CheckpointList;
use neko_protocol::digest::Digest;
use neko_protocol::{Frame, Request, Response, read_frame, write_frame};

use super::write::Call;
use super::{LinkError, Summary};

/// The task the person has open, shared with the link thread so each snapshot it summarises
/// carries that task's detail.
#[derive(Clone, Debug, Default)]
pub struct Wanted(Arc<Mutex<Option<String>>>);

impl Wanted {
    pub fn set(&self, id: Option<&str>) {
        *self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = id.map(str::to_owned);
    }

    pub fn get(&self) -> Option<String> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }
}

/// An open Inbox item: what a routine or a goal needs from the person, with its default.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InboxItem {
    pub seq: u64,
    pub title: String,
    pub detail: String,
    pub default: Option<String>,
    pub link: Option<String>,
    /// The goal that raised it (its link is `goal:<id>`): it is answered with the goal's own actions.
    #[serde(default)]
    pub goal: Option<String>,
    pub urgent: bool,
    /// The routine that raised it.
    pub from: String,
    pub at_ms: i64,
}

/// A runtime a new session can be pinned to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeChoice {
    pub provider: String,
    pub model: String,
    pub label: String,
}

/// What came back, ready for the app. (One is made per call and moved once, so the big variant
/// is not boxed.)
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum Reply {
    Snapshot(Summary),
    Checkpoints { task_id: String, list: CheckpointList },
    Diff { task_id: String, run: Option<u32>, patch: String, truncated: bool },
    Unseen(Vec<String>),
    Digest(Option<Digest>),
    Inbox(Vec<InboxItem>),
    Models(Vec<RuntimeChoice>),
    Answered(String),
    /// Done, and nothing to apply.
    Done,
    /// The daemon said no, in its own words.
    Refused(String),
    Failed(LinkError),
}

const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Sends one request on a fresh connection and waits for its answer.
pub fn ask(socket: &Path, request: Request) -> Result<Response, LinkError> {
    let mut stream = std::os::unix::net::UnixStream::connect(socket).map_err(|e| LinkError::from_io(&e))?;
    stream.set_read_timeout(Some(READ_TIMEOUT)).map_err(|e| LinkError::from_io(&e))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(5))).map_err(|e| LinkError::from_io(&e))?;
    write_frame(&mut stream, &Frame::Request { id: 1, request }).map_err(|e| LinkError::from_io(&e))?;
    loop {
        match read_frame(&mut stream) {
            Ok(Some(Frame::Response { id: 1, response })) => return Ok(response),
            Ok(Some(_)) => continue,
            Ok(None) => return Err(LinkError::Dropped),
            Err(e) => return Err(LinkError::from_io(&e)),
        }
    }
}

/// The Inbox's open items from a `Routines` answer, newest first (the web's `Inbox` page).
pub fn inbox_items(routines: &[neko_protocol::routines::RoutineView]) -> Vec<InboxItem> {
    let mut items: Vec<InboxItem> = Vec::new();
    for view in routines {
        for run in &view.runs {
            for item in run.needs_you.iter().filter(|i| !i.done) {
                items.push(InboxItem {
                    seq: item.seq,
                    title: item.title.clone(),
                    detail: item.detail.clone(),
                    default: item.default.clone().filter(|d| !d.trim().is_empty()),
                    link: item.link.clone().filter(|l| l.starts_with("http://") || l.starts_with("https://")),
                    goal: item.link.as_deref().and_then(|l| l.strip_prefix("goal:")).filter(|g| !g.is_empty()).map(str::to_owned),
                    urgent: item.urgent,
                    from: view.routine.name.clone(),
                    at_ms: run.finished_ms.unwrap_or(run.started_ms),
                });
            }
        }
    }
    items.sort_by_key(|i| std::cmp::Reverse(i.seq));
    items
}

/// Makes one call and reads the answer. `open` is the task whose detail a snapshot answer carries.
pub fn make(socket: &Path, call: &Call, open: Option<&str>) -> Reply {
    let response = match ask(socket, call.wire()) {
        Ok(response) => response,
        Err(error) => return Reply::Failed(error),
    };
    match response {
        Response::Workbench(snapshot) => Reply::Snapshot(Summary::from_snapshot_for(&snapshot, open)),
        Response::TaskCheckpoints(list) => match call {
            Call::Checkpoints { task_id } => Reply::Checkpoints { task_id: task_id.clone(), list },
            _ => Reply::Done,
        },
        Response::TaskChanges { patch, truncated, .. } => match call {
            Call::CheckpointDiff { task_id, run } => Reply::Diff { task_id: task_id.clone(), run: *run, patch, truncated },
            _ => Reply::Done,
        },
        Response::UnseenTasks { ids } => Reply::Unseen(ids),
        Response::Digest { digest } => Reply::Digest(digest),
        Response::Routines { routines, .. } => Reply::Inbox(inbox_items(&routines)),
        Response::AgentModels(catalog) => Reply::Models(runtime_choices(&catalog)),
        Response::NeedsYouAnswered { summary } => Reply::Answered(summary),
        Response::Error { message } => Reply::Refused(message),
        _ => Reply::Done,
    }
}

/// The ready sources' listed models, as choices.
pub fn runtime_choices(catalog: &neko_protocol::agent_models::ModelCatalog) -> Vec<RuntimeChoice> {
    use neko_protocol::agent_models::{ModelAccess, SourceStatus};
    catalog
        .sources
        .iter()
        .filter(|s| s.status == SourceStatus::Ready)
        .flat_map(|s| s.models.iter().filter(|m| m.access != ModelAccess::Unavailable).map(|m| RuntimeChoice { provider: s.provider.clone(), model: m.id.clone(), label: format!("{} · {}", s.label, m.label) }))
        .take(40)
        .collect()
}
