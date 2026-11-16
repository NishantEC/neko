//! The context panel's state and rules (P3a): which tabs a session has, when the panel opens, what
//! the Canvas lists, what the Files tab reads, and the one file or artifact that is loaded. No
//! drawing here (`view/context*.rs`); every read of the daemon goes through [`App::call`] and every
//! read of a file through [`crate::readers`].
//!
//! **What belongs to a session.** The daemon links an artifact to a workspace and to the Neko-chat
//! reply that was running when it was saved (`Artifact::workspace_id`, `message_id`), and to no
//! task. So a session's Canvas lists the artifacts the daemon showed (`ShowArtifact`) while that
//! session was the one open, and an artifact shown again later goes back to the session that first
//! showed it. Nothing is guessed from a workspace.
//!
//! **Memory.** One file or artifact is loaded at a time (`Panel::open`); it, and what a viewer
//! decoded from it, are dropped as soon as it is not on screen ([`App::panel_settle`]).

use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use neko_protocol::Artifact;
use rune::{Key, KeyEvent, Tx};
use rune_kit::{FileNode, FileTree, TreeModel};

use crate::app::App;
use crate::link::Call;
use crate::readers::{Confined, Listing, Refused};
use crate::terminal::{self, Entry};
use crate::viewers::{self, ViewerKey, ViewerStates};
use crate::work::diff_key;

/// An artifact shown within this long counts as "just made": the panel opens on the Canvas.
pub const RECENT_MS: i64 = 10 * 60 * 1000;
/// Artifacts a session remembers.
const SHOWN_KEPT: usize = 40;

/// What the panel can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tab {
    Canvas,
    Diff,
    Files,
    Terminal,
}

impl Tab {
    /// In the order the segmented control shows them.
    pub const ALL: [Tab; 4] = [Tab::Canvas, Tab::Diff, Tab::Files, Tab::Terminal];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Canvas => "Canvas",
            Tab::Diff => "Diff",
            Tab::Files => "Files",
            Tab::Terminal => "Terminal",
        }
    }
}

/// An artifact the daemon showed for a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    pub id: String,
    /// The version asked for; None follows the newest.
    pub version: Option<u32>,
    pub at_ms: i64,
}

/// What the panel remembers about one session.
#[derive(Clone, Debug, Default)]
pub struct SessionPanel {
    /// The tab the person chose; None lets the rules choose.
    pub tab: Option<Tab>,
    /// The person closed the panel on this session: it stays closed until a `ShowArtifact`.
    pub person_closed: bool,
    /// Newest first.
    pub shown: Vec<Shown>,
    /// The artifact open on the Canvas (an id), and the version the person chose for each.
    pub canvas: Option<String>,
    pub versions: HashMap<String, u32>,
    /// The session's working folder: not asked yet (`None`), none (`Some(None)`) or its path.
    pub folder: Option<Option<String>>,
    /// The file open in the Files tab, by its path from the folder.
    pub file: Option<String>,
    /// The run whose diff the Diff tab shows; None is the whole task.
    pub run: Option<u32>,
    /// The task's `updated_at_ms` when its saved artifacts were last asked for (`ArtifactsForTask`): a
    /// change asks again, so what a run saves while the session is open shows up.
    pub artifacts_asked: Option<i64>,
}

/// What is loaded into a viewer: the file's bytes, once. Viewers share this allocation (`shared`); a
/// viewer that decodes on another thread takes a clone of the `Arc`, never a copy of the bytes.
pub struct Loaded {
    pub path: PathBuf,
    pub kind: String,
    pub size: u64,
    pub title: String,
    bytes: Arc<[u8]>,
    /// The bytes are UTF-8 (checked once, when they were loaded).
    utf8: bool,
}

impl Loaded {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The bytes, shared: a count, not a copy.
    pub fn shared(&self) -> Arc<[u8]> {
        self.bytes.clone()
    }

    /// The bytes as text when they are UTF-8.
    pub fn text(&self) -> Option<&str> {
        if self.utf8 { std::str::from_utf8(&self.bytes).ok() } else { None }
    }

    pub fn from_bytes(path: PathBuf, kind: String, title: String, bytes: Vec<u8>) -> Loaded {
        let size = bytes.len() as u64;
        let utf8 = std::str::from_utf8(&bytes).is_ok();
        Loaded { path, kind, size, title, bytes: Arc::from(bytes), utf8 }
    }
}

/// What the panel has been asked to show: a file in a session's folder, or one version of an artifact.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    File { task_id: String, path: String },
    Artifact { id: String, version: u32 },
}

impl Target {
    /// What a viewer's state is kept by.
    pub fn key(&self) -> ViewerKey {
        match self {
            Target::File { task_id, path } => (format!("file:{task_id}:{path}"), 0),
            Target::Artifact { id, version } => (id.clone(), *version),
        }
    }

    /// The web app's embed route for this target, when it is an artifact (a file of a folder has none):
    /// `artifact/<id>/<n>?embed=1`. The id is the daemon's (12 hex characters, validated again here).
    pub fn web_route(&self) -> Option<String> {
        match self {
            Target::Artifact { id, version } if !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') => Some(format!("artifact/{id}/{version}?embed=1")),
            _ => None,
        }
    }

    pub fn route(&self) -> Option<String> {
        match self {
            Target::Artifact { id, version } => Some(format!("artifact?id={id}&v={version}")),
            Target::File { task_id, .. } => Some(format!("tasks?task={task_id}")),
        }
    }
}

/// How loading the open target went.
pub enum Content {
    Loading,
    Ready(Rc<Loaded>),
    TooLarge { size: u64, path: PathBuf },
    Failed(String),
}

pub struct Opened {
    pub target: Target,
    pub content: Content,
}

/// The Files tab's tree for one folder.
pub struct FilesMeta {
    pub task_id: String,
    pub root: PathBuf,
    pub truncated: bool,
    pub count: usize,
}

/// One row of the Terminal tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TermRow {
    /// "…N earlier commands hidden".
    Dropped,
    Header(usize),
    Line(usize, usize),
    /// "…N lines hidden" inside entry `.0`.
    Gap(usize),
}

/// The Terminal tab's entries and rows, shared with the rows' builders, and how many older commands were left out.
pub type TermRows = (Rc<Vec<Entry>>, Rc<Vec<TermRow>>, usize);

pub struct TermCache {
    pub task_id: String,
    /// What the events were when this was made: how many, and the last one's time.
    pub stamp: (usize, i64),
    pub entries: Rc<Vec<Entry>>,
    pub dropped: usize,
    pub rows: Rc<Vec<TermRow>>,
}

#[derive(Default)]
pub struct Panel {
    pub sessions: HashMap<String, SessionPanel>,
    pub states: ViewerStates,
    /// The daemon's data folder: artifacts live under `<data_dir>/artifacts`.
    pub data_dir: PathBuf,
    /// Records of the artifacts shown, by id.
    pub artifacts: HashMap<String, Artifact>,
    /// The one file or artifact loaded.
    pub open: Option<Opened>,
    /// The Files tab's tree, for `files_meta`'s folder.
    pub files_tree: TreeModel<FileNode<bool>>,
    pub files_meta: Option<FilesMeta>,
    pub terminal: std::cell::RefCell<Option<TermCache>>,
    /// The panel fills the session and panel regions.
    pub expanded: bool,
    /// An element key to scroll into view at the next frame (a page, a file's diff).
    pub pending_scroll: Option<String>,
    /// Which select is open ("runs", "versions").
    pub open_select: Option<String>,
    /// The panel is open but the window is too narrow to lay it out: set by the view, read by the loader,
    /// the release and the compositor, so a hidden panel holds nothing.
    pub hidden: Cell<bool>,
    pub dark: Cell<bool>,
    /// What each session's folder was when it was last asked for (its worktree and whether it is parked).
    pub folder_rev: HashMap<String, String>,
    /// The session whose folder is being listed now (there is one tree, for the session on screen).
    pub listing: Option<String>,
}

impl Panel {
    /// The panel of a window: artifacts are read from the daemon's own data folder.
    pub fn standard() -> Panel {
        Panel::new(neko_protocol::support_dir())
    }

    pub fn new(data_dir: PathBuf) -> Panel {
        viewers::ensure_builtins();
        Panel { data_dir, files_tree: FileTree::new().build(Vec::<(&str, bool)>::new()), ..Panel::default() }
    }

    pub fn session(&self, id: &str) -> Option<&SessionPanel> {
        self.sessions.get(id)
    }

    fn session_mut(&mut self, id: &str) -> &mut SessionPanel {
        self.sessions.entry(id.to_owned()).or_default()
    }

    /// The root artifacts are read under.
    pub fn artifact_root(&self) -> Option<Confined> {
        Confined::new(&self.data_dir.join("artifacts"))
    }

    /// The scene the compositor draws: the model on screen, its camera and the appearance.
    pub fn scene(&self, target: Option<&Target>) -> Option<crate::viewers::model3d_gpu::Scene> {
        let target = target?;
        let open = self.open.as_ref().filter(|o| &o.target == target)?;
        let Content::Ready(loaded) = &open.content else { return None };
        if viewers::resolve_for(&loaded.kind, Some(loaded.bytes())).map(|s| s.id) != Some("model3d") {
            return None;
        }
        self.states.with(&target.key(), |st| match st.peek::<viewers::model3d::Loaded>() {
            Some(viewers::model3d::Loaded::Mesh(m)) => Some(crate::viewers::model3d_gpu::Scene { mesh: m.clone(), camera: st.camera, dark: self.dark.get() }),
            _ => None,
        })
    }
}

/// When an artifact last changed (its newest version), as the daemon's `artifacts::updated_ms` says it.
fn updated_ms(artifact: &Artifact) -> i64 {
    artifact.versions.iter().map(|v| v.at_ms).max().unwrap_or(0)
}

/// The tab rules, as a pure function of what the session has: the tab to open on among `available`.
/// Canvas if an artifact was shown in the last ten minutes, else Diff, else Terminal, else Files;
/// the first available one when none of those apply (an old artifact alone).
pub fn first_tab(available: &[Tab], recent_artifact: bool) -> Option<Tab> {
    let has = |t: Tab| available.contains(&t);
    if recent_artifact && has(Tab::Canvas) {
        return Some(Tab::Canvas);
    }
    [Tab::Diff, Tab::Terminal, Tab::Files].into_iter().find(|t| has(*t)).or_else(|| available.first().copied())
}

impl App {
    fn session_panel(&self, id: &str) -> Option<&SessionPanel> {
        self.panel.session(id)
    }

    /// The tabs the selected session has content for, in the control's order.
    pub fn panel_tabs(&self) -> Vec<Tab> {
        let Some(id) = self.selected.as_deref() else { return Vec::new() };
        let sp = self.session_panel(id);
        let mut tabs = Vec::new();
        if sp.is_some_and(|p| !p.shown.is_empty()) {
            tabs.push(Tab::Canvas);
        }
        if self.work.checkpoints.get(id).is_some_and(|l| l.runs.iter().any(|r| r.files > 0)) {
            tabs.push(Tab::Diff);
        }
        if sp.is_some_and(|p| matches!(&p.folder, Some(Some(_)))) {
            tabs.push(Tab::Files);
        }
        if self.detail().is_some_and(|d| d.task.id == id && terminal::command_count(&d.task) > 0) {
            tabs.push(Tab::Terminal);
        }
        tabs
    }

    /// An artifact was shown in the last [`RECENT_MS`].
    fn recent_artifact(&self, id: &str) -> bool {
        self.session_panel(id).is_some_and(|p| p.shown.iter().any(|s| self.now_ms - s.at_ms <= RECENT_MS))
    }

    /// The tab on screen: the person's choice while it has content, else the rules' first.
    pub fn panel_tab(&self) -> Option<Tab> {
        let id = self.selected.as_deref()?;
        let tabs = self.panel_tabs();
        let chosen = self.session_panel(id).and_then(|p| p.tab).filter(|t| tabs.contains(t));
        chosen.or_else(|| first_tab(&tabs, self.recent_artifact(id)))
    }

    /// Whether the panel is on screen for the selected session.
    pub fn panel_visible(&self) -> bool {
        self.selected.is_some() && self.layout.context.is_open() && !self.panel.hidden.get()
    }

    /// What the panel is showing as a viewer, if it is showing one.
    pub fn panel_target(&self) -> Option<Target> {
        if !self.layout.context.is_open() || self.panel.hidden.get() {
            return None;
        }
        let id = self.selected.as_deref()?;
        let sp = self.session_panel(id)?;
        match self.panel_tab()? {
            Tab::Files => sp.file.clone().map(|path| Target::File { task_id: id.to_owned(), path }),
            Tab::Canvas => {
                let art = sp.canvas.clone().or_else(|| sp.shown.first().map(|s| s.id.clone()))?;
                let version = sp.versions.get(&art).copied().or_else(|| self.panel.artifacts.get(&art).and_then(|a| a.versions.iter().map(|v| v.n).max()))?;
                Some(Target::Artifact { id: art, version })
            }
            _ => None,
        }
    }

    /// The compositor's scene (the 3D viewer on screen, if there is one).
    pub fn panel_scene(&self) -> Option<crate::viewers::model3d_gpu::Scene> {
        self.panel.scene(self.panel_target().as_ref())
    }

    /// The person opens or closes the panel. Closing it sticks for this session until an artifact is shown.
    pub fn panel_set_open(&mut self, open: bool) {
        self.layout.context.set_open(open);
        if let Some(id) = self.selected.clone() {
            self.panel.session_mut(&id).person_closed = !open;
        }
        if !open {
            self.panel.expanded = false;
        }
        self.panel_sync();
    }

    pub fn panel_toggle(&mut self) {
        let open = !self.layout.context.is_open();
        self.panel_set_open(open);
    }

    pub fn panel_choose_tab(&mut self, tab: Tab) {
        if let Some(id) = self.selected.clone() {
            self.panel.session_mut(&id).tab = Some(tab);
        }
        self.panel.open_select = None;
        self.panel_sync();
    }

    pub fn panel_expand(&mut self, on: bool) {
        self.panel.expanded = on && self.layout.context.is_open();
    }

    /// A session was chosen: the panel opens when it has something and the person has not closed it
    /// there, and stays closed when it has nothing.
    pub fn panel_on_select(&mut self) {
        self.panel.expanded = false;
        self.panel.open_select = None;
        let Some(id) = self.selected.clone() else { return };
        let closed = self.session_panel(&id).is_some_and(|p| p.person_closed);
        let any = !self.panel_tabs().is_empty();
        self.layout.context.set_open(any && !closed);
        self.panel_sync();
    }

    /// Content may have arrived (a snapshot, a read): open the panel if the session now has some and
    /// the person has not closed it, and ask for what the open tab needs.
    pub fn panel_sync(&mut self) {
        let Some(id) = self.selected.clone() else { return };
        // The folder: asked while the task has one, and again when its worktree or parking changes.
        let rev = self.detail().filter(|d| d.task.id == id && d.has_folder).map(|d| format!("{:?}|{}", d.task.worktree, d.task.parked.is_some()));
        if let Some(rev) = rev
            && self.panel.folder_rev.get(&id) != Some(&rev)
        {
            self.panel.folder_rev.insert(id.clone(), rev);
            self.call(Call::TaskFolder { task_id: id.clone() });
        }
        // One tree serves the session on screen: coming back to a session whose folder is known but whose
        // tree was replaced lists it again.
        if let Some(Some(path)) = self.session_panel(&id).map(|p| p.folder.clone()).and_then(|f| f)
            && self.panel.files_meta.as_ref().is_none_or(|m| m.task_id != id)
            && self.panel.listing.as_deref() != Some(id.as_str())
        {
            self.folder_arrived(&id, Some(path));
        }
        // What this session's own runs saved, kept by the daemon (asked once, and again when the task moved).
        let updated = self.summary.as_ref().and_then(|m| m.tasks.iter().find(|t| t.id == id)).map_or(0, |t| t.updated_at_ms);
        if self.session_panel(&id).and_then(|p| p.artifacts_asked) != Some(updated) {
            self.panel.session_mut(&id).artifacts_asked = Some(updated);
            self.call(Call::TaskArtifacts { task_id: id.clone() });
        }
        let closed = self.session_panel(&id).is_some_and(|p| p.person_closed);
        if !closed && !self.layout.context.is_open() && !self.panel_tabs().is_empty() {
            self.layout.context.set_open(true);
        }
        if !self.layout.context.is_open() {
            return;
        }
        match self.panel_tab() {
            Some(Tab::Diff) => {
                let run = self.session_panel(&id).and_then(|p| p.run);
                self.want_diff(&id, run);
                // The Files tab marks changed files from the whole task's diff.
            }
            Some(Tab::Canvas) | Some(Tab::Files) | Some(Tab::Terminal) | None => {}
        }
        // The Files tab marks what changed from the whole task's diff.
        if self.panel_tab() == Some(Tab::Files) && self.session_panel(&id).is_some_and(|p| matches!(&p.folder, Some(Some(_)))) && self.work.checkpoints.get(&id).is_some_and(|l| l.runs.iter().any(|r| r.files > 0)) {
            self.want_diff(&id, None);
        }
        self.panel_want_target();
    }

    /// Asks for a diff the panel shows, once.
    pub fn want_diff(&mut self, task_id: &str, run: Option<u32>) {
        let k = diff_key(task_id, run);
        if !self.work.diffs.contains_key(&k) && !self.work.diff_load.contains_key(&k) {
            self.work.diff_load.insert(k, crate::work::DiffLoad::Loading);
            self.call(Call::CheckpointDiff { task_id: task_id.to_owned(), run });
        }
    }

    /// Loads what the panel is showing if it is not loaded: the artifact's file, or the file's bytes.
    fn panel_want_target(&mut self) {
        let Some(target) = self.panel_target() else {
            return;
        };
        if self.panel.open.as_ref().is_some_and(|o| o.target == target) {
            return;
        }
        match &target {
            Target::Artifact { id, version } => {
                self.panel.open = Some(Opened { target: target.clone(), content: Content::Loading });
                self.call(Call::ArtifactFile { id: id.clone(), version: Some(*version) });
            }
            Target::File { task_id, path } => {
                self.panel.open = Some(Opened { target: target.clone(), content: Content::Loading });
                let folder = self.session_panel(task_id).and_then(|p| p.folder.clone()).flatten();
                let (task_id, path) = (task_id.clone(), path.clone());
                let Some(root) = folder.as_deref().and_then(|f| Confined::new(Path::new(f))) else {
                    self.finish_open(&target, Err(Refused::Missing), None);
                    return;
                };
                self.background(move || (root.read(Path::new(&path)), path, task_id), move |app, (read, path, _)| {
                    let kind = Path::new(&path).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                    let title = Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(path.clone());
                    app.finish_open(&target, read, Some((kind, title)));
                    true
                });
            }
        }
    }

    /// The load finished: keep it if it is still what the panel wants.
    fn finish_open(&mut self, target: &Target, read: Result<(PathBuf, Vec<u8>), Refused>, names: Option<(String, String)>) {
        if self.panel.open.as_ref().is_none_or(|o| &o.target != target) {
            return;
        }
        let content = match read {
            Ok((path, bytes)) => {
                let (kind, title) = names.unwrap_or_else(|| (String::new(), String::new()));
                Content::Ready(Rc::new(Loaded::from_bytes(path, kind, title, bytes)))
            }
            Err(Refused::TooLarge(size)) => {
                // Where Finder is sent: the file's canonical path inside the folder it was read from.
                let path = match target {
                    Target::File { task_id, path } => self.panel.files_meta.as_ref().filter(|m| &m.task_id == task_id).and_then(|m| Confined::new(&m.root)?.resolve(Path::new(path)).ok()).unwrap_or_default(),
                    Target::Artifact { .. } => PathBuf::new(),
                };
                Content::TooLarge { size, path }
            }
            Err(refused) => Content::Failed(refused.line()),
        };
        if let Some(open) = self.panel.open.as_mut() {
            open.content = content;
        }
    }

    /// Runs `job` off the UI thread and applies its answer on it; inline when there is no window
    /// (tests and headless runs against a daemon), so a scripted run sees the result.
    pub fn background<T: Send + 'static>(&mut self, job: impl FnOnce() -> T + Send + 'static, apply: impl FnOnce(&mut App, T) -> bool + Send + 'static) {
        match &self.dispatch {
            crate::app::Dispatch::Async { handle, .. } => {
                let handle = handle.clone();
                let spawned = std::thread::Builder::new().name("neko-panel".into()).spawn(move || {
                    let value = job();
                    handle.update_if(move |s, tx| {
                        tx.instant();
                        apply(s, value)
                    });
                });
                let _ = spawned;
            }
            _ => {
                let value = job();
                apply(self, value);
            }
        }
    }

    /// The daemon asked every window to show an artifact: open the panel on the Canvas, at the session
    /// that showed it before (else the one open now). The person's close is forgotten.
    pub fn show_artifact(&mut self, id: &str, version: Option<u32>) {
        let owner = self.panel.sessions.iter().find(|(_, p)| p.shown.iter().any(|s| s.id == id)).map(|(k, _)| k.clone());
        let Some(session) = owner.or_else(|| self.selected.clone()) else { return };
        if self.selected.as_deref() != Some(session.as_str()) {
            self.select(&session);
        }
        let now = self.now_ms;
        let sp = self.panel.session_mut(&session);
        sp.shown.retain(|s| s.id != id);
        sp.shown.insert(0, Shown { id: id.to_owned(), version, at_ms: now });
        sp.shown.truncate(SHOWN_KEPT);
        sp.person_closed = false;
        sp.tab = Some(Tab::Canvas);
        sp.canvas = Some(id.to_owned());
        match version {
            Some(v) => {
                sp.versions.insert(id.to_owned(), v);
            }
            None => {
                sp.versions.remove(id);
            }
        }
        self.layout.context.set_open(true);
        // The record names its versions and title; the file is asked for once the version is known.
        self.call(Call::ArtifactInfo { id: id.to_owned() });
        self.panel_sync();
    }

    /// What the session's own runs saved arrived: their records join the Canvas's list beside what the
    /// daemon showed while the session was open (an artifact is listed once, by its id).
    pub fn task_artifacts(&mut self, task_id: &str, artifacts: Vec<Artifact>) -> bool {
        let mut changed = false;
        for artifact in artifacts {
            let at_ms = updated_ms(&artifact);
            let id = artifact.id.clone();
            if self.panel.artifacts.get(&id) != Some(&artifact) {
                self.panel.artifacts.insert(id.clone(), artifact);
                changed = true;
            }
            let sp = self.panel.session_mut(task_id);
            if !sp.shown.iter().any(|s| s.id == id) {
                sp.shown.push(Shown { id, version: None, at_ms });
                changed = true;
            }
        }
        if changed {
            let sp = self.panel.session_mut(task_id);
            sp.shown.sort_by_key(|s| std::cmp::Reverse(s.at_ms));
            sp.shown.truncate(SHOWN_KEPT);
            self.panel_sync();
        }
        changed
    }

    /// The artifact's record arrived.
    pub fn artifact_info(&mut self, artifact: Artifact) -> bool {
        let changed = self.panel.artifacts.get(&artifact.id) != Some(&artifact);
        self.panel.artifacts.insert(artifact.id.clone(), artifact);
        // A newer version than the one open: the target moves on, so ask for it.
        self.panel_sync();
        changed
    }

    /// Where an artifact version is stored: read it, confined to `<data dir>/artifacts`.
    pub fn artifact_file(&mut self, id: &str, version: Option<u32>, path: &str, kind: &str) {
        let Some(version) = version.or_else(|| self.panel.artifacts.get(id).and_then(|a| a.versions.iter().map(|v| v.n).max())) else { return };
        let target = Target::Artifact { id: id.to_owned(), version };
        if self.panel.open.as_ref().is_none_or(|o| o.target != target) {
            return;
        }
        let Some(root) = self.panel.artifact_root() else {
            self.finish_open(&target, Err(Refused::Missing), None);
            return;
        };
        let (path, kind) = (path.to_owned(), kind.to_ascii_lowercase());
        let title = self.panel.artifacts.get(id).map(|a| a.title.clone()).unwrap_or_else(|| id.to_owned());
        self.background(move || (root.read(Path::new(&path)), kind, title), move |app, (read, kind, title)| {
            app.finish_open(&target, read, Some((kind, title)));
            true
        });
    }

    /// The daemon could not name an artifact's file: the Canvas says so where the viewer would be.
    pub fn artifact_unreadable(&mut self, id: &str, version: Option<u32>, message: String) {
        let Some(version) = version.or_else(|| self.panel.artifacts.get(id).and_then(|a| a.versions.iter().map(|v| v.n).max())) else { return };
        let target = Target::Artifact { id: id.to_owned(), version };
        self.finish_open(&target, Err(Refused::Unreadable), None);
        if let Some(open) = self.panel.open.as_mut().filter(|o| o.target == target) {
            open.content = Content::Failed(message);
        }
    }

    /// The session's folder arrived: list it off the UI thread and build the Files tab's tree.
    pub fn folder_arrived(&mut self, task_id: &str, path: Option<String>) -> bool {
        self.panel.session_mut(task_id).folder = Some(path.clone());
        let Some(root) = path.as_deref().and_then(|p| Confined::new(Path::new(p))) else {
            // No folder, or one that is not there to read: the session has no Files tab.
            self.panel.session_mut(task_id).folder = Some(None);
            self.panel_sync();
            return true;
        };
        let task_id = task_id.to_owned();
        let root_path = root.root().to_path_buf();
        self.panel.listing = Some(task_id.clone());
        self.background(move || (root.list(), task_id), move |app, (listing, task_id): (Listing, String)| {
            app.panel.listing = None;
            // A listing that arrives after the person moved to another session is dropped: it is made again on return.
            if app.selected.as_deref() != Some(task_id.as_str()) {
                return false;
            }
            app.panel.files_tree = FileTree::new().build(listing.files.iter().map(|f| (f.as_str(), false)));
            app.panel.files_meta = Some(FilesMeta { task_id, root: root_path, truncated: listing.truncated, count: listing.files.len() });
            app.mark_changed_files();
            app.panel_sync();
            true
        });
        true
    }

    /// Marks the files the whole task changed, once its diff and the folder's tree are both here.
    pub fn mark_changed_files(&mut self) {
        let Some(meta) = self.panel.files_meta.as_ref() else { return };
        let Some(diff) = self.work.diffs.get(&diff_key(&meta.task_id, None)) else { return };
        let changed: Vec<String> = diff.files().iter().filter_map(|f| f.new_path.clone().or_else(|| f.old_path.clone())).collect();
        for path in changed {
            if let Some(node) = self.panel.files_tree.get_mut(&path) {
                node.data = Some(true);
                // What changed is what the person came to see: its folders are open.
                self.panel.files_tree.expand_to(&path);
            }
        }
    }

    /// Opens a file of the folder in the Files tab's viewer.
    pub fn panel_open_file(&mut self, path: &str) {
        let Some(id) = self.selected.clone() else { return };
        self.panel.session_mut(&id).file = Some(path.to_owned());
        self.panel_sync();
    }

    /// The back arrow in the Files tab: to the tree again.
    pub fn panel_close_file(&mut self) {
        if let Some(id) = self.selected.clone() {
            self.panel.session_mut(&id).file = None;
        }
        self.panel_settle();
    }

    pub fn panel_pick_artifact(&mut self, id: &str) {
        let Some(session) = self.selected.clone() else { return };
        self.panel.session_mut(&session).canvas = Some(id.to_owned());
        self.panel_sync();
    }

    pub fn panel_pick_version(&mut self, id: &str, version: u32) {
        let Some(session) = self.selected.clone() else { return };
        self.panel.session_mut(&session).versions.insert(id.to_owned(), version);
        self.panel.open_select = None;
        self.panel_sync();
    }

    pub fn panel_pick_run(&mut self, run: Option<u32>) {
        let Some(session) = self.selected.clone() else { return };
        self.panel.session_mut(&session).run = run;
        self.panel.open_select = None;
        self.panel_sync();
    }

    /// Drops what is not on screen: viewers' decoded content except the one shown, and the loaded
    /// file or artifact when nothing shows it. Runs every frame; cheap when there is nothing to drop.
    pub fn panel_settle(&mut self) {
        let target = self.panel_target();
        // The Terminal tab's rows (up to 200 commands) are kept only while the tab is the one showing.
        if !(self.layout.context.is_open() && !self.panel.hidden.get() && self.panel_tab() == Some(Tab::Terminal)) {
            self.panel.terminal.replace(None);
        }
        let resident = self.panel.open.as_ref().filter(|o| Some(&o.target) == target.as_ref() && matches!(o.content, Content::Ready(_))).map(|o| o.target.key());
        self.panel.states.release_except(resident.as_ref());
        if target.is_none() && self.panel.open.is_some() {
            self.panel.open = None;
        } else if let (Some(open), Some(target)) = (&self.panel.open, &target)
            && &open.target != target
        {
            self.panel.open = None;
        }
    }

    /// Once a frame: let go of what left the screen, load what came on it, scroll what was asked to.
    pub fn panel_tick(&mut self, t: f64, tx: &mut Tx) -> bool {
        self.panel_settle();
        self.panel_want_target();
        let web_changed = self.web_tick(t, tx);
        if let Some(key) = self.panel.pending_scroll.take() {
            tx.scroll_center(key);
        }
        if let Some(target) = self.panel_target() {
            self.panel.states.with(&target.key(), |st| viewers::image_view::tick(st, t, tx));
        }
        web_changed
    }

    /// The web view (P3b), once a frame: asks the daemon to sign what the panel wants, makes the view when
    /// its address is in, moves and follows it, and lets go of it when it is not needed. The plugins'
    /// viewers and pages are asked for once the daemon is answering.
    pub fn web_tick(&mut self, t: f64, tx: &mut Tx) -> bool {
        if !self.plugins_asked && self.summary.is_some() {
            self.plugins_asked = true;
            self.call(Call::PluginViewers);
            self.call(Call::PluginPages);
        }
        if let Some(route) = self.web_view.needs_signing() {
            self.call(Call::SignedWebUrl { route });
        }
        // A plugin's page keeps the view as long as it is open, even with the context panel closed.
        let panel_open = self.layout.context.is_open() || self.plugin_view.is_some();
        let existed = self.web_view.exists();
        self.web_view.tick(t, panel_open);
        // What the view says (a download finished) is a toast of the window's.
        let notes = self.web_view.drain_notes();
        let spoke = !notes.is_empty();
        if let Some(last) = notes.last() {
            self.show_toast(last);
        }
        // A view that is not on screen is looked at again, so its five minutes are kept.
        if self.web_view.exists() && !self.web_view.shown() {
            tx.wake_after(std::time::Duration::from_secs(30));
        }
        // The slot says "Opening…" until the view exists, and not after: the view is built again when that changes.
        existed != self.web_view.exists() || spoke
    }

    /// Keys the panel owns: ⌘+, ⌘−, ⌘0 zoom a PDF; W toggles a model's wireframe; Esc restores an
    /// expanded panel. True when it took the key.
    pub fn panel_on_key(&mut self, ev: &KeyEvent) -> bool {
        if ev.key == Key::Escape && self.panel.expanded {
            self.panel.expanded = false;
            return true;
        }
        let Some(target) = self.panel_target() else { return false };
        let kind = match self.panel.open.as_ref() {
            Some(Opened { content: Content::Ready(l), .. }) => viewers::resolve_for(&l.kind, Some(l.bytes())).map(|s| s.id),
            _ => None,
        };
        let key = target.key();
        match kind {
            Some("pdf") => {
                // ⌘+ is ⇧⌘= on most keyboards, so both name it.
                let dir = if ev.is_command('=') || ev.is_command('+') {
                    1
                } else if ev.is_command('-') || ev.is_command('_') {
                    -1
                } else if ev.is_command('0') {
                    0
                } else {
                    return false;
                };
                self.panel.states.with(&key, |st| viewers::pdf::zoom(st, dir));
                true
            }
            Some("model3d") if matches!(&ev.key, Key::Text(c) if c.eq_ignore_ascii_case("w")) && !ev.mods.cmd && !ev.mods.ctrl && !ev.mods.alt => {
                self.panel.states.with(&key, |st| st.camera.wire = !st.camera.wire);
                true
            }
            _ => false,
        }
    }

    /// The Terminal tab's rows for the open task, made when its events changed.
    pub fn terminal_rows(&self) -> Option<TermRows> {
        let detail = self.detail()?;
        let stamp = (detail.task.events.len(), detail.task.events.last().map_or(0, |e| e.at_ms));
        let mut cache = self.panel.terminal.borrow_mut();
        if cache.as_ref().is_none_or(|c| c.task_id != detail.task.id || c.stamp != stamp) {
            let (entries, dropped) = terminal::entries(&detail.task);
            let mut rows = Vec::new();
            if dropped > 0 {
                rows.push(TermRow::Dropped);
            }
            for (i, e) in entries.iter().enumerate() {
                rows.push(TermRow::Header(i));
                for l in 0..e.lines.len() {
                    if e.hidden > 0 && l == e.at {
                        rows.push(TermRow::Gap(i));
                    }
                    rows.push(TermRow::Line(i, l));
                }
            }
            *cache = Some(TermCache { task_id: detail.task.id.clone(), stamp, entries: Rc::new(entries), dropped, rows: Rc::new(rows) });
        }
        let c = cache.as_ref()?;
        Some((c.entries.clone(), c.rows.clone(), c.dropped))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_tab_follows_the_rules() {
        use Tab::*;
        assert_eq!(first_tab(&[Canvas, Diff, Files, Terminal], true), Some(Canvas), "a recent artifact");
        assert_eq!(first_tab(&[Canvas, Diff, Files, Terminal], false), Some(Diff), "an old artifact does not win");
        assert_eq!(first_tab(&[Files, Terminal], false), Some(Terminal));
        assert_eq!(first_tab(&[Files], false), Some(Files));
        assert_eq!(first_tab(&[Canvas], false), Some(Canvas), "an old artifact alone is still what there is");
        assert_eq!(first_tab(&[], true), None);
        assert_eq!(Tab::ALL.map(Tab::title), ["Canvas", "Diff", "Files", "Terminal"]);
    }

    #[test]
    fn a_target_names_its_viewer_state_and_its_web_route() {
        let a = Target::Artifact { id: "abc123".into(), version: 3 };
        assert_eq!((a.key(), a.route().as_deref()), (("abc123".to_owned(), 3), Some("artifact?id=abc123&v=3")));
        let f = Target::File { task_id: "t1".into(), path: "src/a.rs".into() };
        assert_eq!(f.key(), ("file:t1:src/a.rs".to_owned(), 0));
    }

    #[test]
    fn viewers_share_the_panels_bytes_and_never_copy_them() {
        let l = Loaded::from_bytes("/x/a.csv".into(), "csv".into(), "a.csv".into(), b"id\n1\n".to_vec());
        let (a, b) = (l.shared(), l.shared());
        assert!(std::ptr::eq(a.as_ptr(), l.bytes().as_ptr()) && std::ptr::eq(b.as_ptr(), l.bytes().as_ptr()), "one allocation");
        assert_eq!(Arc::strong_count(&a), 3, "the panel's copy and two borrowers: counts, not copies");
        let table = crate::viewers::table::CsvData::new(l.shared(), b',');
        assert_eq!(table.rows(), 1);
        assert_eq!(Arc::strong_count(&a), 4, "the index is built over the same bytes");
        assert_eq!(table.held(), 16, "so the viewer's own memory is the index alone (two records, eight bytes each)");
    }

    #[test]
    fn a_loaded_file_is_text_when_it_is_utf8_and_keeps_its_bytes_otherwise() {
        let t = Loaded::from_bytes("/x/a.txt".into(), "txt".into(), "a.txt".into(), b"hello".to_vec());
        assert_eq!((t.text(), t.bytes(), t.size), (Some("hello"), &b"hello"[..], 5));
        let b = Loaded::from_bytes("/x/a.bin".into(), "bin".into(), "a.bin".into(), vec![0xff, 0x00, 0xfe]);
        assert_eq!((b.text(), b.bytes()), (None, &[0xff, 0x00, 0xfe][..]));
    }
}
