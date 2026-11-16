//! One [`Call`] made and its answer read: a short-lived connection to the daemon's socket (the
//! held snapshot connection in `link/mod.rs` keeps going on its own), and the daemon's answer
//! as the small thing the app applies.

use std::path::Path;
use std::sync::{Arc, Mutex};

use neko_protocol::agent_models::ModelCatalog;
use neko_protocol::checkpoints::CheckpointList;
use neko_protocol::digest::Digest;
use neko_protocol::{Frame, Request, Response, read_frame, write_frame};

use super::pages::{CalendarData, PhoneData, QuotaRow, RoutinesData};
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
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Reply {
    Snapshot(Summary),
    Checkpoints { task_id: String, list: CheckpointList },
    Diff { task_id: String, run: Option<u32>, patch: String, truncated: bool },
    /// The folder a session works in, when it has one that exists now.
    Folder { task_id: String, path: Option<String> },
    /// Where one version of an artifact is stored.
    ArtifactFile { id: String, version: Option<u32>, path: String, kind: String, size: u64 },
    /// An artifact's record.
    ArtifactInfo(neko_protocol::Artifact),
    /// The web UI's address at a route, signed in. The token is in it: never printed.
    SignedUrl { route: String, url: String },
    PluginViewers(Vec<neko_protocol::PluginViewer>),
    PluginPages(Vec<neko_protocol::PluginPage>),
    /// What a session's own runs saved.
    TaskArtifacts { task_id: String, artifacts: Vec<neko_protocol::Artifact> },
    Unseen(Vec<String>),
    Digest(Option<Digest>),
    Inbox(Vec<InboxItem>),
    Models { choices: Vec<RuntimeChoice>, catalog: Box<ModelCatalog> },
    /// The `Routines` answer, whole (the routines page, standing orders, Inbox settings).
    Routines(Box<RoutinesData>),
    Calendar(Box<CalendarData>),
    CalendarStatus(neko_protocol::calendar::CalendarStatus),
    Phone(Box<PhoneData>),
    /// The appearance mode the daemon keeps: `system`, `light` or `dark`.
    Appearance(String),
    PhoneTest(Vec<String>),
    Quota(Vec<QuotaRow>),
    Plugins(Vec<neko_protocol::PluginHealth>),
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
        Response::TaskFolder { path } => match call {
            Call::TaskFolder { task_id } => Reply::Folder { task_id: task_id.clone(), path },
            _ => Reply::Done,
        },
        Response::ArtifactFile { path, kind, size } => match call {
            Call::ArtifactFile { id, version } => Reply::ArtifactFile { id: id.clone(), version: *version, path, kind, size },
            _ => Reply::Done,
        },
        Response::Artifact { artifact, .. } => Reply::ArtifactInfo(artifact),
        Response::SignedWebUrl { url } => match call {
            Call::SignedWebUrl { route } => Reply::SignedUrl { route: route.clone(), url },
            _ => Reply::Done,
        },
        Response::PluginViewers { viewers } => Reply::PluginViewers(viewers),
        Response::PluginPages { pages } => Reply::PluginPages(pages),
        Response::Artifacts { artifacts } => match call {
            Call::TaskArtifacts { task_id } => Reply::TaskArtifacts { task_id: task_id.clone(), artifacts },
            _ => Reply::Done,
        },
        Response::UnseenTasks { ids } => Reply::Unseen(ids),
        Response::Digest { digest } => Reply::Digest(digest),
        Response::Routines { routines, standing_orders, scopes, owner_name, age_out_days, routines_on, spend_today } => match call {
            // The Inbox asks the same request and keeps only the open items.
            Call::Inbox => Reply::Inbox(inbox_items(&routines)),
            _ => Reply::Routines(Box::new(RoutinesData { views: routines, standing_orders, scopes, owner_name, age_out_days, routines_on, spend_today })),
        },
        Response::AgentModels(catalog) => Reply::Models { choices: runtime_choices(&catalog), catalog: Box::new(catalog) },
        Response::CalendarEvents { events, neko, status } => match call {
            Call::CalendarEvents { from_ms, to_ms } => Reply::Calendar(Box::new(CalendarData { events, neko, status, from_ms: *from_ms, to_ms: *to_ms })),
            _ => Reply::Done,
        },
        Response::CalendarStatus(status) => Reply::CalendarStatus(status),
        Response::Phone { settings, status } => Reply::Phone(Box::new(PhoneData { settings, status })),
        Response::PhoneTest { lines } => Reply::PhoneTest(lines),
        Response::Appearance { mode, .. } => Reply::Appearance(mode),
        Response::PluginStatus { plugins } => Reply::Plugins(plugins),
        Response::SearchResults { items, .. } => Reply::Quota(quota_rows(&items)),
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

/// The usage provider's meters as rows: a label, how much is used, and the meter's own words.
pub fn quota_rows(items: &[neko_protocol::SearchItem]) -> Vec<QuotaRow> {
    items
        .iter()
        .filter_map(|i| {
            let meter = i.meter.as_ref()?;
            let label = match &i.group_label {
                Some(g) if !g.is_empty() => format!("{g} · {}", i.title),
                _ => i.title.clone(),
            };
            let mut words: Vec<String> = meter.stats.iter().map(|s| format!("{} {}", s.value, s.label).trim().to_owned()).collect();
            if let Some(sub) = i.subtitle.as_ref().filter(|s| !s.is_empty()) {
                words.push(sub.clone());
            }
            Some(QuotaRow { label, used: meter.fraction.clamp(0.0, 1.0), words })
        })
        .collect()
}
