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
use super::{ExtraAnswer, LinkError, Summary};

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

/// A string that is never printed: the signed web address carries the token, so a `{:?}` of a reply cannot leak it.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(text: String) -> Secret {
        Secret(text)
    }

    pub fn reveal(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<hidden>")
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
    /// Where it lives, which `ClearNeedsYou` and `ReopenNeedsYou` name (the routine and the run that reported it).
    #[serde(default)]
    pub routine_id: String,
    #[serde(default)]
    pub run_id: String,
    /// When it closed and why, for the items under "Closed recently"; None while it is open.
    #[serde(default)]
    pub closed_ms: Option<i64>,
    #[serde(default)]
    pub reason: String,
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

/// The web UI's appearance as the daemon keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppearanceChoice {
    /// `system`, `light` or `dark`.
    pub mode: String,
    /// The theme ids for light and for dark.
    pub light: String,
    pub dark: String,
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
    SignedUrl { route: String, url: Secret },
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
    /// The appearance the daemon keeps: the mode and the theme for light and for dark.
    Appearance(AppearanceChoice),
    /// The answer of an [`ExtraCall`] (and of removing a theme).
    Extra(Box<ExtraAnswer>),
    PhoneTest(Vec<String>),
    Quota(Vec<QuotaRow>),
    Plugins(Vec<neko_protocol::PluginHealth>),
    /// The installed packs and the bundled catalog (the answer to every pack request but a preview).
    Packs(neko_protocol::packs::PackListing),
    /// What installing a source would add.
    PackPreview(neko_protocol::packs::PackPreview),
    /// The suggestions and whether Neko looks for patterns (the answer to every Improvements request).
    Improve(Box<neko_protocol::improve::ImproveReply>),
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

/// How long a closed item stays under "Closed recently" (the web's `CLOSED_DAYS`, which the daemon keeps too).
pub const CLOSED_DAYS: i64 = 3;

/// The Inbox's items from a `Routines` answer: the open ones newest first, then the ones closed within the last
/// [`CLOSED_DAYS`] days, newest closed first (the web's `Inbox` page).
pub fn inbox_items(routines: &[neko_protocol::routines::RoutineView], now_ms: i64) -> Vec<InboxItem> {
    let mut open: Vec<InboxItem> = Vec::new();
    let mut closed: Vec<InboxItem> = Vec::new();
    for view in routines {
        for run in &view.runs {
            for item in &run.needs_you {
                let made = |closed_ms: Option<i64>, reason: String| InboxItem {
                    seq: item.seq,
                    title: item.title.clone(),
                    detail: item.detail.clone(),
                    default: item.default.clone().filter(|d| !d.trim().is_empty()),
                    link: item.link.clone().filter(|l| l.starts_with("http://") || l.starts_with("https://")),
                    goal: item.link.as_deref().and_then(|l| l.strip_prefix("goal:")).filter(|g| !g.is_empty()).map(str::to_owned),
                    routine_id: view.routine.id.clone(),
                    run_id: run.id.clone(),
                    closed_ms,
                    reason,
                    urgent: item.urgent,
                    from: view.routine.name.clone(),
                    at_ms: run.finished_ms.unwrap_or(run.started_ms),
                };
                if !item.done {
                    open.push(made(None, String::new()));
                } else if let Some(at) = item.closed_ms.filter(|at| now_ms - at <= CLOSED_DAYS * 86_400_000) {
                    closed.push(made(Some(at), item.reason.clone().filter(|r| !r.is_empty()).unwrap_or_else(|| "Closed".to_owned())));
                }
            }
        }
    }
    open.sort_by_key(|i| std::cmp::Reverse(i.seq));
    closed.sort_by_key(|i| std::cmp::Reverse(i.closed_ms));
    open.extend(closed);
    open
}

/// Makes one call and reads the answer. `open` is the task whose detail a snapshot answer carries.
pub fn make(socket: &Path, call: &Call, open: Option<&str>) -> Reply {
    if let Call::Extra(extra) = call {
        return extra.run(socket, open);
    }
    let mut response = match ask(socket, call.wire()) {
        Ok(response) => response,
        Err(error) => return Reply::Failed(error),
    };
    // A call of two steps (the old message leaves the queue, then the new words go): the second answer is the one that counts.
    if let (Some(next), Response::Workbench(_)) = (call.then(), &response) {
        response = match ask(socket, next) {
            Ok(response) => response,
            Err(error) => return Reply::Failed(error),
        };
    }
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
            Call::SignedWebUrl { route } => Reply::SignedUrl { route: route.clone(), url: Secret(url) },
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
            // The Inbox asks the same request and keeps the open items and the recently closed; closing or reopening one answers the same.
            Call::Inbox | Call::ClearNeedsYou { .. } | Call::ReopenNeedsYou { .. } => Reply::Inbox(inbox_items(&routines, crate::app::system_ms())),
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
        Response::Appearance { mode, light, dark, .. } => Reply::Appearance(AppearanceChoice { mode, light, dark }),
        Response::Themes { dir, themes, problems } => Reply::Extra(Box::new(ExtraAnswer::Themes { dir, themes: themes.into_iter().map(|t| (t.id, t.file, t.theme)).collect(), problems: problems.into_iter().map(|p| (p.file, p.message)).collect() })),
        Response::PluginStatus { plugins } => Reply::Plugins(plugins),
        Response::Packs(listing) => Reply::Packs(listing),
        Response::PackPreview(preview) => Reply::PackPreview(preview),
        Response::Improve(reply) => Reply::Improve(Box::new(reply)),
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
            // `Usage.tsx`: the first stat is the figure, the rest (and the subtitle) its muted line, with "N% used" first from 70%.
            let used = meter.fraction.clamp(0.0, 1.0);
            let first = meter.stats.first();
            let mut detail: Vec<String> = Vec::new();
            if used >= 0.7 {
                detail.push(format!("{}% used", (used * 100.0).round() as i64));
            }
            if let Some(sub) = i.subtitle.as_ref().filter(|s| !s.is_empty()) {
                detail.push(sub.clone());
            }
            detail.extend(meter.stats.iter().skip(1).map(|s| format!("{} {}", s.value, s.label)));
            let value = first.map(|s| s.value.clone()).filter(|v| !v.is_empty()).unwrap_or_else(|| format!("{}%", (used * 100.0).round() as i64));
            Some(QuotaRow { label, used, words, value, unit: first.map(|s| s.label.clone()).unwrap_or_default(), detail: detail.join(" · ") })
        })
        .collect()
}
