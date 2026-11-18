//! The app's state and what changes it. No drawing here (`view.rs` and its region modules) and no
//! I/O: the link thread (`connection.rs`) sends [`LinkEvent`]s in and the buttons that leave the
//! app go through function pointers, so everything in this file is tested without a window or a
//! daemon.
//!
//! The window is four regions (docs/decisions/0002-rune-for-everything.md): the sessions column,
//! the session view (or a full view in its place), the context panel and the Neko column. P0
//! fills the sessions and the counts from the live snapshot; the rest are placeholders.

use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rune::{AppHandle, PointerPress, TextEdit};

use crate::account::Account;
use crate::groups::Group;
use crate::layout::{Layout, Saved};
use crate::link::{Call, LinkError, LinkEvent, Summary, TaskRow, Wanted, calm, make};
use crate::new_session::NewForm;
use crate::pages::Pages;
use crate::work::{NekoCol, Work};
use crate::views::FullView;
use crate::web_link::{WebTarget, open_url, reveal_in_finder};

/// Where the connection to the daemon stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkState {
    Connecting,
    Live,
    /// Down, with the calm sentence to show. The last workbench stays in [`App::summary`].
    Down { message: String },
}

/// How a message to a running agent lands (`docs/knowledge/the-chat-directs-the-crew.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Steer {
    Now,
    AfterTurn,
    Interrupt,
}

impl Steer {
    pub const ALL: [Steer; 3] = [Steer::Now, Steer::AfterTurn, Steer::Interrupt];

    pub fn label(self) -> &'static str {
        match self {
            Steer::Now => "Steer now",
            Steer::AfterTurn => "After this turn",
            Steer::Interrupt => "Interrupt and restart",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Steer::Now => "The agent reads it at its next step",
            Steer::AfterTurn => "Waits for the turn to finish",
            Steer::Interrupt => "Stops the turn and starts again with it",
        }
    }
}

/// How a request reaches the daemon.
#[derive(Clone)]
pub enum Dispatch {
    /// Nowhere: requests wait in [`App::outbox`] (unit tests, scenes).
    None,
    /// On a short-lived thread; the answer comes back through the window's handle.
    Async { socket: PathBuf, handle: AppHandle<App> },
    /// On the calling thread, answered before the call returns: headless runs against a test
    /// daemon, where a scripted click must see its answer in the next frame.
    Blocking { socket: PathBuf },
}

/// A scripted stream for headless measurements: words arrive in the last pending reply at a rate,
/// by the run's own clock, so a 60-tokens-a-second reply can be timed without a model.
pub struct Stream {
    pub words: Vec<String>,
    pub per_sec: f64,
    pub started: Option<f64>,
    pub shown: usize,
}

/// The ⌘K palette: the row the arrow keys are on (the query is [`App::palette_query`]).
pub struct PaletteState {
    pub index: usize,
}

pub struct App {
    pub link: LinkState,
    /// The last workbench the daemon sent; kept while the link is down.
    pub summary: Option<Summary>,
    /// Where the web UI is, once the link thread has found the daemon's token.
    pub web: Option<WebTarget>,
    /// One calm line under a button that could not do its job.
    pub notice: Option<String>,
    /// How a URL is opened. The browser by default; a no-op in headless scenes and tests.
    pub opener: fn(&str) -> io::Result<()>,
    /// How a file is shown in Finder (`open -R`); a no-op in headless scenes and tests.
    pub revealer: fn(&str) -> io::Result<()>,
    /// Asks macOS what this process may do (Settings, Permissions); a headless run asks nothing.
    pub access_probe: fn() -> crate::permissions::Local,
    /// The time, in epoch milliseconds: the system's, or a fixed one in headless scenes.
    pub clock: fn() -> i64,
    /// When the last snapshot arrived, by `clock`: what "5m ago" counts from.
    pub now_ms: i64,
    /// Whether a daemon connection will be made at all (`nolink` turns it off for demos).
    pub linked: bool,
    /// Show the design lab (tokens and status glyphs) in place of the session.
    pub lab: bool,

    pub layout: Layout,
    /// The selected session's task id.
    pub selected: Option<String>,
    /// A full view in the session's place, or the session.
    pub view: Option<FullView>,
    /// Which of the groups are collapsed, in `Group::ALL` order ("Waiting a while" starts folded).
    pub collapsed: [bool; 4],
    /// "Show N more" in Needs you was pressed.
    pub needs_expanded: bool,
    /// The sessions column shows one group only (Home's "3 running" and "3 need you" set it).
    pub filter: Option<Group>,
    /// The account's name and picture (the footer, the greeting).
    pub account: Account,
    /// How many pages of 30 chat messages Neko's column shows ("Earlier messages" adds one).
    pub chat_pages: usize,
    /// The Activity line in Neko's column is open.
    pub activity_open: bool,
    /// When the sidebar's counts (routines, today's events) were last asked for.
    pub side_asked_ms: i64,

    pub steer: Steer,
    pub steer_open: bool,
    /// The session header's menu is open.
    pub session_menu: bool,
    /// The account footer's menu is open.
    pub more_open: bool,
    /// The lines the window says for a few seconds ("Saved to Downloads"), newest last, at most three
    /// (`toasts.rs`): each goes four seconds after the first tick that shows it.
    pub toasts: Vec<crate::toasts::ToastItem>,
    pub toast_counter: u64,
    /// A question in a dialog before something that cannot be taken back (`task_actions.rs`).
    pub dialog: Option<crate::task_actions::Ask>,
    /// The title of the open session is being renamed.
    pub rename: Option<crate::task_actions::Rename>,
    /// The workspace the sessions list, Routines, Calendar's Neko rows and Home's composer are scoped to
    /// (None = all workspaces). Kept between runs (`scope.rs`).
    pub workspace_scope: Option<String>,
    /// The system's Reduce Motion, as the last build saw it (Home's art stops under it).
    pub reduce_motion: std::cell::Cell<bool>,
    /// The P4 overlays (the pill, the launcher, the stop-all key): empty unless `--overlays` (`overlays.rs`).
    pub overlays: crate::overlays::Overlays,
    /// The web view of the context panel and of a plugin's page (P3b).
    pub web_view: crate::web_panel::WebHost,
    /// What the plugins registered as pages (`PluginPages`), for "More"; None until asked.
    pub plugin_pages: Option<Vec<neko_protocol::PluginPage>>,
    /// A plugin's page open in the middle region (a web view of its whole content).
    pub plugin_view: Option<neko_protocol::PluginPage>,
    /// The plugins' viewers and pages were asked for once this run.
    pub plugins_asked: bool,
    pub composer: TextEdit,
    pub ask: TextEdit,
    /// Neko's ask box is on Plan (the web's Ask / Plan switch): the text goes with the plan-only instruction.
    pub ask_plan: bool,

    pub dispatch: Dispatch,
    /// Requests made while there is nowhere to send them (tests read this).
    pub outbox: Vec<Call>,
    /// The task open now, shared with the link thread so snapshots carry its detail.
    pub wanted: Wanted,
    /// Finished tasks not opened since they finished (the Iris dot).
    pub unseen: HashSet<String>,
    /// Tasks marked unread here: they stay unread until opened again.
    pub keep_unread: HashSet<String>,
    pub work: Work,
    pub neko: NekoCol,
    pub new_form: Option<NewForm>,
    pub new_request: TextEdit,
    /// The right-click menu on a session row: which row, and where.
    pub row_menu: Option<(String, PointerPress)>,
    pub palette: Option<PaletteState>,
    pub palette_query: TextEdit,
    pub stream: Option<Stream>,
    /// Whether this window keeps its widths between runs (the real window only; a headless run or
    /// a test never touches the preferences file).
    pub persist_layout: bool,
    /// What was last written, and when a newer layout was first seen (it is written once it stops moving).
    pub layout_written: Option<Saved>,
    pub layout_changed_at: Option<f64>,
    /// The layout the last tick saw, so a change restarts the wait.
    pub layout_seen: Option<Saved>,
    /// The right-click menu on a run in the runs list: which run, and where.
    pub run_menu: Option<(u32, PointerPress)>,
    /// The right-click menu on a page's row (a routine, a workspace, a plugin): its key, and where.
    pub page_menu: Option<(String, PointerPress)>,
    /// The window has the keyboard. A finished session is only seen while it does.
    pub focused: bool,
    /// The right-click menu on an Inbox item: which, and where.
    pub inbox_menu: Option<(u64, PointerPress)>,
    /// The right-click menu on a closed Inbox item: which, and where.
    pub closed_menu: Option<(u64, PointerPress)>,
    /// What the full views keep (Routines, Calendar, Settings, Usage, Plugins).
    pub pages: Pages,
    /// Setup: its section, the open dialog and rows (`view/setup/`).
    pub setup: crate::view::setup::SetupState,
    /// The context panel: its tabs, the loaded file or artifact, the viewers' state.
    pub panel: crate::panel::Panel,
    /// The right-click menu on a message: which, and where.
    pub msg_menu: Option<crate::view::MsgMenu>,
    /// What the composers keep: the `/` and `@` popover, files attached, the queue tray's edit (`compose.rs`).
    pub compose: crate::compose::ComposeUi,
    /// "All sessions": its search, cursor and filters (`all_sessions.rs`).
    pub all: crate::all_sessions::AllState,
    /// Pull requests: the groups folded, merged and closed shown, the header's menu (`pull_requests.rs`).
    pub prs: crate::pull_requests::PrState,
    /// The workspace switcher's list is open.
    pub scope_open: bool,
    /// The scope changed and is to be written (the real window only).
    pub scope_dirty: bool,
    /// The shortcut overlay (`?`).
    pub shortcuts_open: bool,
    /// The right-click menu on selected text: the words, and where.
    pub text_menu: Option<(String, PointerPress)>,
    /// The Dock badge (`dock.rs`): the real window sets it; a headless run has none.
    pub dock: fn(usize),
    /// The count the Dock shows now.
    pub dock_shown: Option<usize>,
    /// How many agents the daemon says are stopped waiting for a person (`AttentionChanged`).
    pub daemon_attention: Option<usize>,
    /// The model chip's popover (`runtime_pick.rs`, `view/runtime.rs`).
    pub runtime: crate::runtime_pick::RuntimeUi,
    /// The web server's port and token (from `SignedWebUrl`), for the three paths `link/http.rs` reaches.
    pub http: Option<crate::link::http::Http>,
    /// The address was asked for (once; asked again when the answer failed).
    pub http_asked: bool,
    /// Settings' second half, Usage's machine reading and the rulebook guard (`extra.rs`).
    pub extra: crate::extra::ExtraState,
    /// The first-run setup (`onboarding.rs`).
    pub onboarding: crate::onboarding::Onboarding,
    /// Plugins as full management (`plugin_manage.rs`).
    pub plugin_ui: crate::plugin_manage::PluginUi,
    /// Luau cards and views that installed packs ship (`src/luau/`).
    pub luau: crate::luau::Luau,
    /// What this window tells the daemon about itself (`report.rs`).
    pub reporter: crate::report::Reporter,
    /// A route from `--open` that waits for the daemon's first answer (tasks are only known then).
    pub pending_route: Option<String>,
    /// The window's size in points, as the last build saw it.
    pub window_size: std::cell::Cell<Option<[u32; 2]>>,
    /// The Browser tab's live view of a Chrome on this Mac (`src/browser/`).
    pub browser: crate::browser::BrowserTab,
}

/// Where the window's own settings are kept (Rune's `prefs` store).
pub const PREFS_ID: &str = "neko-rune";

/// Reads the layout the last run left, if any. Called once at startup, before the first frame.
pub fn load_layout() -> Option<Saved> {
    rune::prefs::Prefs::open(PREFS_ID).get::<Saved>("layout")
}

/// The workspace the window was scoped to the last run (None: all workspaces).
pub fn load_scope() -> Option<String> {
    rune::prefs::Prefs::open(PREFS_ID).get::<Option<String>>(crate::scope::KEY).flatten()
}

/// The Luau cards the person hid (`<pack>/<card id>`), as the last run left them.
pub fn load_luau_hidden() -> Vec<String> {
    rune::prefs::Prefs::open(PREFS_ID).get::<Vec<String>>("luau-hidden").unwrap_or_default()
}

/// Writes the hidden cards on their own thread.
pub fn write_luau_hidden(hidden: Vec<String>) {
    let _ = std::thread::Builder::new().name("neko-luau-hidden".into()).spawn(move || {
        let _ = rune::prefs::Prefs::open(PREFS_ID).set("luau-hidden", &hidden);
    });
}

/// The layout All sessions had the last run (List, Board or Tree).
pub fn load_all_layout() -> Option<crate::board::Layout> {
    rune::prefs::Prefs::open(PREFS_ID).get::<crate::board::Layout>("all-layout")
}

/// Writes the All sessions layout on its own thread.
fn write_all_layout(layout: crate::board::Layout) {
    let _ = std::thread::Builder::new().name("neko-all-layout".into()).spawn(move || {
        let _ = rune::prefs::Prefs::open(PREFS_ID).set("all-layout", &layout);
    });
}

/// The activity pill's on/off (Swift: UserDefaults `neko.presence.enabled`, default on), kept in this client's own saved state.
pub fn load_presence() -> bool {
    rune::prefs::Prefs::open(PREFS_ID).get::<bool>("presence").unwrap_or(true)
}

/// Writes the pill's preference on its own thread.
pub fn write_presence(on: bool) {
    let _ = std::thread::Builder::new().name("neko-presence".into()).spawn(move || {
        let _ = rune::prefs::Prefs::open(PREFS_ID).set("presence", &on);
    });
}

/// Writes the scope on its own thread.
fn write_scope(id: Option<String>) {
    let _ = std::thread::Builder::new().name("neko-scope".into()).spawn(move || {
        let _ = rune::prefs::Prefs::open(PREFS_ID).set(crate::scope::KEY, &id);
    });
}

/// Writes a layout on its own thread: nothing touches a file while input or frames run.
fn write_layout(saved: Saved) {
    let _ = std::thread::Builder::new().name("neko-layout".into()).spawn(move || {
        let _ = rune::prefs::Prefs::open(PREFS_ID).set("layout", &saved);
    });
}

/// The system clock, in epoch milliseconds.
pub fn system_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl Default for App {
    fn default() -> Self {
        App {
            link: LinkState::Connecting,
            summary: None,
            web: None,
            notice: None,
            opener: open_url,
            revealer: reveal_in_finder,
            access_probe: crate::permissions::check,
            clock: system_ms,
            now_ms: system_ms(),
            linked: true,
            lab: false,
            layout: Layout::default(),
            selected: None,
            view: None,
            collapsed: [false, false, false, true],
            needs_expanded: false,
            filter: None,
            account: Account::default(),
            chat_pages: 1,
            activity_open: false,
            side_asked_ms: i64::MIN / 2,
            steer: Steer::AfterTurn,
            steer_open: false,
            session_menu: false,
            more_open: false,
            reduce_motion: std::cell::Cell::new(false),
            overlays: Default::default(),
            toasts: Vec::new(),
            toast_counter: 0,
            dialog: None,
            rename: None,
            workspace_scope: None,
            web_view: crate::web_panel::WebHost::default(),
            plugin_pages: None,
            plugin_view: None,
            plugins_asked: false,
            composer: TextEdit::new(""),
            ask: TextEdit::new(""),
            ask_plan: false,
            dispatch: Dispatch::None,
            outbox: Vec::new(),
            wanted: Wanted::default(),
            unseen: HashSet::new(),
            keep_unread: HashSet::new(),
            work: Work::default(),
            neko: NekoCol::default(),
            new_form: Some(NewForm::fresh()),
            new_request: TextEdit::new(""),
            row_menu: None,
            palette: None,
            palette_query: TextEdit::new(""),
            stream: None,
            persist_layout: false,
            layout_written: None,
            layout_changed_at: None,
            layout_seen: None,
            run_menu: None,
            page_menu: None,
            focused: true,
            inbox_menu: None,
            closed_menu: None,
            pages: Pages::default(),
            setup: crate::view::setup::SetupState::default(),
            panel: crate::panel::Panel::standard(),
            msg_menu: None,
            compose: crate::compose::ComposeUi::default(),
            all: crate::all_sessions::AllState::default(),
            prs: crate::pull_requests::PrState::default(),
            scope_open: false,
            scope_dirty: false,
            shortcuts_open: false,
            text_menu: None,
            dock: crate::dock::none,
            dock_shown: None,
            daemon_attention: None,
            runtime: crate::runtime_pick::RuntimeUi::default(),
            http: None,
            http_asked: false,
            extra: Default::default(),
            onboarding: Default::default(),
            plugin_ui: Default::default(),
            luau: Default::default(),
            reporter: Default::default(),
            pending_route: None,
            window_size: Default::default(),
            browser: Default::default(),
        }
    }
}


/// The time headless scenes pretend it is (a fixed instant, so a snapshot is a fact).
pub const DEMO_NOW: i64 = 1_800_000_000_000;

impl App {
    /// The state a headless run starts from: it never opens anything outside itself and its clock
    /// stands still.
    pub fn headless() -> App {
        App { opener: |_| Ok(()), revealer: |_| Ok(()), access_probe: crate::permissions::none, clock: || DEMO_NOW, now_ms: DEMO_NOW, linked: false, ..App::default() }
    }

    /// Whether the window is dark: the daemon's mode when it is Light or Dark (the web UI's own setting), else the system's.
    pub fn appearance_dark(&self, system_dark: bool) -> bool {
        match self.pages.appearance.as_deref() {
            Some("light") => false,
            Some("dark") => true,
            _ => system_dark,
        }
    }

    /// The person picks System, Light or Dark: the window follows at once, and the daemon keeps it for every client.
    pub fn set_appearance(&mut self, mode: &str) {
        self.pages.appearance = Some(mode.to_owned());
        self.call(Call::SetAppearance { mode: mode.to_owned() });
    }

    pub fn apply_link(&mut self, event: LinkEvent) -> bool {
        match event {
            LinkEvent::Connecting => {
                let changed = self.link != LinkState::Connecting && self.summary.is_none();
                if self.summary.is_none() {
                    self.link = LinkState::Connecting;
                }
                changed
            }
            LinkEvent::Live(summary) => {
                let changed = self.link != LinkState::Live || self.summary.as_ref() != Some(&summary);
                self.link = LinkState::Live;
                self.apply_summary(summary);
                changed
            }
            LinkEvent::Quiet => {
                // Nothing changed, but ages ("5m") and the day's edge move: redraw once a minute.
                let now = (self.clock)();
                let stale = now - self.now_ms >= 60_000;
                if stale {
                    self.now_ms = now;
                }
                // The Inbox and the digest are read again about once a minute.
                self.after_live();
                if let Some(view) = self.view {
                    self.refresh_page(view);
                }
                stale
            }
            LinkEvent::ShowArtifact { id, version } => {
                self.show_artifact(&id, version);
                true
            }
            LinkEvent::Pushed(pushed) => self.apply_pushed(pushed),
            LinkEvent::Down { error, retry_in, .. } => {
                let state = LinkState::Down { message: calm(&error, retry_in) };
                let changed = self.link != state;
                self.link = state;
                changed
            }
        }
    }

    /// A workbench arrived (from the link, or as the answer to a write).
    pub fn apply_summary(&mut self, summary: Summary) {
        self.now_ms = (self.clock)();
        // A window opens on Home; a selection that has gone puts it back there.
        let gone = self.selected.as_ref().is_some_and(|id| !summary.tasks.iter().any(|t| &t.id == id));
        self.summary = Some(summary);
        self.seed_settings();
        if gone {
            self.selected = None;
            self.wanted.set(None);
            self.ensure_home();
        }
        if self.new_form.as_ref().is_some_and(|f| f.branches.is_empty()) {
            self.read_branches();
        }
        // The open task's detail rides on the snapshot the link makes once it knows which task is
        // open; a task chosen by the window itself is asked for once.
        if let Some(id) = self.selected.clone() {
            let have = self.summary.as_ref().and_then(|s| s.detail.as_ref()).is_some_and(|d| d.task.id == id);
            if !have && self.work.detail_asked.as_deref() != Some(id.as_str()) {
                self.work.detail_asked = Some(id);
                self.call(Call::Refresh);
            }
        }
        // A session chosen for the person (the first, or the one that replaced a session that went) gets
        // the panel the rules give it, as one the person chose does.
        if gone {
            self.panel_on_select();
        }
        self.after_live();
    }

    /// Sends one request to the daemon (see [`Dispatch`]).
    pub fn call(&mut self, call: Call) {
        // A plugin's removal goes over HTTP, not the socket (only after its confirmation: `task_actions.rs`).
        if let Some(id) = call.plugin_to_remove() {
            self.plugin_ui.busy = Some(id.to_owned());
            self.http_work(crate::compose::HttpWork::PluginRemove { id: id.to_owned() });
            return;
        }
        match self.dispatch.clone() {
            Dispatch::None => self.outbox.push(call),
            Dispatch::Blocking { socket } => {
                let reply = make(&socket, &call, self.selected.as_deref());
                self.apply_reply(&call, reply);
            }
            Dispatch::Async { socket, handle } => {
                let open = self.selected.clone();
                let spawned = std::thread::Builder::new().name("neko-call".into()).spawn(move || {
                    let reply = make(&socket, &call, open.as_deref());
                    handle.update_if(move |s, tx| {
                        tx.instant();
                        s.apply_reply(&call, reply)
                    });
                });
                if spawned.is_err() {
                    self.work.notice = Some("Couldn’t reach Neko just now. Try again.".to_owned());
                }
            }
        }
    }

    /// Keeps the layout: once it has stopped changing for half a second, it is written (off the UI
    /// thread). True never: nothing on screen depends on it. Asks to be woken to write the last change.
    pub fn tick_layout(&mut self, t: f64, tx: &mut rune::Tx) -> bool {
        if !self.persist_layout {
            return false;
        }
        if std::mem::take(&mut self.scope_dirty) {
            write_scope(self.workspace_scope.clone());
        }
        if std::mem::take(&mut self.all.layout_dirty) {
            write_all_layout(self.all.layout);
        }
        let now = self.layout.saved();
        // The write waits until the layout has not changed for half a second: each change starts the wait again.
        if self.layout_seen != Some(now) {
            self.layout_seen = Some(now);
            self.layout_changed_at = Some(t);
        }
        match self.layout_changed_at {
            Some(at) if self.layout_written == Some(now) => {
                let _ = at;
                self.layout_changed_at = None;
            }
            Some(at) if t - at >= 0.5 => {
                write_layout(now);
                self.layout_written = Some(now);
                self.layout_changed_at = None;
            }
            Some(at) => tx.wake_at(at + 0.55),
            None => {}
        }
        false
    }

    /// Advances a scripted stream to time `t` (seconds on the run's clock). True when words arrived.
    pub fn tick_stream(&mut self, t: f64) -> bool {
        let Some(stream) = self.stream.as_mut() else { return false };
        let start = *stream.started.get_or_insert(t);
        let due = (((t - start) * stream.per_sec) as usize).min(stream.words.len());
        if due <= stream.shown {
            return false;
        }
        let add: String = stream.words[stream.shown..due].concat();
        stream.shown = due;
        let Some(summary) = self.summary.as_mut() else { return false };
        let Some(last) = summary.conversation.last_mut() else { return false };
        let at = last.parts.iter().rposition(|p| p["type"] == "text");
        match at {
            Some(i) => {
                let text = format!("{}{add}", last.parts[i]["text"].as_str().unwrap_or(""));
                last.parts[i]["text"] = serde_json::Value::String(text);
            }
            None => last.parts.push(serde_json::json!({"type": "text", "text": add, "state": "streaming"})),
        }
        true
    }

    /// Opens a folder in Finder. Only absolute paths, and never one that could read as an option.
    pub fn open_path(&mut self, path: &str) {
        if !path.starts_with('/') || (self.opener)(path).is_err() {
            self.notice = Some("Couldn’t open that folder.".to_owned());
        }
    }

    /// Shows a file in Finder. Only absolute paths; says why in one calm line when it cannot.
    pub fn reveal(&mut self, path: &str) {
        if !path.starts_with('/') || (self.revealer)(path).is_err() {
            self.notice = Some("Couldn’t show that file in Finder.".to_owned());
        }
    }

    /// Opens a link (a pull request) in the browser; says why in one calm line when it cannot.
    pub fn open_link(&mut self, url: &str) {
        let web = url.starts_with("https://") || url.starts_with("http://");
        if !web || (self.opener)(url).is_err() {
            self.work.notice = Some("Couldn’t open that link.".to_owned());
        }
    }

    pub fn set_web(&mut self, web: Option<WebTarget>) -> bool {
        let changed = self.web != web;
        self.web = web;
        changed
    }

    /// The selected session's row, if the snapshot still has it.
    pub fn session(&self) -> Option<&TaskRow> {
        let id = self.selected.as_ref()?;
        self.summary.as_ref()?.tasks.iter().find(|t| &t.id == id)
    }

    /// Chooses a session: it shows in the middle, replacing any full view.
    pub fn select(&mut self, id: &str) {
        self.new_form = None;
        self.open_task(id);
    }

    /// A plugin's page: the middle region shows its whole content, in the web view.
    pub fn open_plugin_page(&mut self, page: neko_protocol::PluginPage) {
        if !self.guard(crate::extra::Going::Plugin(page.clone())) {
            return;
        }
        self.plugin_view = Some(page);
        self.view = None;
        self.more_open = false;
        self.notice = None;
        self.new_form = None;
    }

    pub fn open_view(&mut self, view: FullView) {
        if view != FullView::Settings && !self.guard(crate::extra::Going::View(view)) {
            return;
        }
        self.plugin_view = None;
        self.view = Some(view);
        self.more_open = false;
        self.notice = None;
        self.new_form = None;
        self.enter_view(view);
    }

    /// Opens one of Neko's dated things from the Calendar: a task opens as a session, the rest in the web UI.
    pub fn open_neko_item(&mut self, route: &str) {
        match route.strip_prefix("tasks?task=").map(|r| r.split('&').next().unwrap_or(r)) {
            Some(id) if self.summary.as_ref().is_some_and(|m| m.task(id).is_some()) => self.select(id),
            _ => self.open_in_web(Some(route)),
        }
    }

    /// The HTTP client, when the daemon has signed its address; asks for it the first time it is wanted.
    pub fn http(&mut self) -> Option<crate::link::http::Http> {
        if self.http.is_none() && !self.http_asked {
            self.http_asked = true;
            self.call(Call::SignedWebUrl { route: crate::link::http::ROUTE.to_owned() });
        }
        self.http.clone()
    }

    /// Opens or folds a workspace's details; what was open is remembered.
    pub fn toggle_workspace(&mut self, id: &str) {
        let open = self.workspace_open(id);
        self.pages.ws_open.insert(id.to_owned(), !open);
    }

    /// Every workspace shows expanded until the person folds it.
    pub fn workspace_open(&self, id: &str) -> bool {
        self.pages.ws_open.get(id).copied().unwrap_or(true)
    }

    /// Folds or opens something on the Routines page away from its default (see `Pages::inbox_flags`).
    pub fn toggle_inbox_flag(&mut self, flag: &str) {
        if !self.pages.inbox_flags.remove(flag) {
            self.pages.inbox_flags.insert(flag.to_owned());
        }
    }

    /// Opens or folds one detail line of a workspace (or a list's "Show N more"), by `<workspace id>/<line>`.
    pub fn toggle_ws_line(&mut self, key: &str) {
        if !self.pages.ws_lines.remove(key) {
            self.pages.ws_lines.insert(key.to_owned());
        }
    }

    pub fn toggle_group(&mut self, group: Group) {
        let i = group.index();
        self.collapsed[i] = !self.collapsed[i];
    }

    /// Opens the first session of the first group that has one (what the scenes and tests that want a session open ask for;
    /// a real window opens on Home).
    pub fn open_first_session(&mut self) {
        if let Some(id) = self.summary.as_ref().and_then(|m| crate::groups::first_session(m, self.now_ms)) {
            self.open_task(&id);
        }
    }

    /// "View all" in the sessions column: every session, newest first, in the middle region.
    pub fn open_all_sessions(&mut self) {
        self.open_view(FullView::AllSessions);
        self.enter_all_sessions();
    }

    /// How many of the calendar's events start today (local time; an all-day event counts).
    pub fn events_today(&self, data: &crate::link::CalendarData) -> usize {
        let start = crate::clock::day_start(self.now_ms);
        let end = crate::clock::add_days(start, 1);
        data.events.iter().filter(|e| e.start_ms < end && e.end_ms.max(e.start_ms + 1) > start).count()
    }

    /// Whether Home is what the middle region shows: no session chosen and no full view or plugin page.
    pub fn home_shown(&self) -> bool {
        self.selected.is_none() && self.view.is_none() && self.plugin_view.is_none() && !self.lab
    }

    /// Home's "3 running" and "3 need you" filter the sessions column to that group; pressing it again clears it.
    pub fn set_filter(&mut self, group: Group) {
        self.filter = if self.filter == Some(group) { None } else { Some(group) };
        self.collapsed[group.index()] = false;
    }

    /// Opens the web UI at `page` (its hash route id), home when none. Says why in one calm line
    /// when it cannot.
    pub fn open_in_web(&mut self, page: Option<&str>) {
        self.notice = None;
        let Some(web) = &self.web else {
            self.notice = Some(match self.link {
                LinkState::Live => "Neko is running, but its web address isn't known yet. Try again in a moment.".to_owned(),
                _ => "Neko isn't running, so there's no web UI to open yet.".to_owned(),
            });
            return;
        };
        if (self.opener)(&web.url(page)).is_err() {
            self.notice = Some("Couldn't open your browser.".to_owned());
        }
    }

    /// How many sessions wait on the person, for the Neko column and its collapsed icon.
    pub fn waiting(&self) -> usize {
        self.group_count(Group::NeedsYou)
    }

    /// How many sessions are in `group` right now (all of them, before the column's caps).
    pub fn group_count(&self, group: Group) -> usize {
        self.summary.as_ref().map_or(0, |m| m.tasks.iter().filter(|t| crate::groups::group_of(t, self.now_ms) == Some(group) && self.scope().is_none_or(|w| m.workspace_of(&t.id) == w)).count())
    }
}

/// The Neko column's one-line crew status: who is working, then what needs the person.
pub fn crew_line(working: usize, waiting: usize, inbox: usize) -> String {
    let work = match working {
        0 => "No agents working".to_owned(),
        1 => "1 agent working".to_owned(),
        n => format!("{n} agents working"),
    };
    let wait = match waiting {
        // No session needs you, but the Inbox may still hold items: say so rather than "nothing needs you".
        0 if inbox > 0 => format!("{inbox} in Inbox"),
        0 => "nothing needs you".to_owned(),
        1 => "1 needs you".to_owned(),
        n => format!("{n} need you"),
    };
    format!("{work} · {wait}")
}

/// The line at the Neko column's foot: the link's state, then what the workbench says. Leads with
/// the status, says what needs the person or that nothing does (DESIGN.md §6).
pub fn status_line(link: &LinkState, summary: Option<&Summary>) -> String {
    let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let known = summary.map(|s| plural(s.tasks.len(), "task", "tasks"));
    match (link, known) {
        (LinkState::Connecting, None) => "Connecting to Neko…".to_owned(),
        (LinkState::Connecting, Some(k)) => format!("Connecting to Neko… Last known: {k}"),
        (LinkState::Live, Some(k)) => format!("Connected · {k}"),
        (LinkState::Live, None) => "Connected".to_owned(),
        (LinkState::Down { message }, None) => message.clone(),
        (LinkState::Down { message }, Some(k)) => format!("{message} Last known: {k}"),
    }
}

/// A link that is down, for scenes.
pub fn down(error: LinkError, secs: u64) -> LinkEvent {
    LinkEvent::Down { error, attempt: 1, retry_in: Duration::from_secs(secs) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::Wait;
    use neko_protocol::workbench::TaskStatus;

    fn row(id: &str, status: TaskStatus, ago_ms: i64) -> TaskRow {
        TaskRow { id: id.into(), key: None, title: id.into(), status, updated_at_ms: DEMO_NOW - ago_ms, goal: String::new(), wait: Wait::None, progress: None, line: Default::default() }
    }

    fn summary(rows: Vec<TaskRow>) -> Summary {
        Summary { digest: format!("d{}", rows.len()), tasks: rows, ..Summary::default() }
    }

    thread_local! { static OPENED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) }; }

    fn record(url: &str) -> io::Result<()> {
        OPENED.with(|o| o.borrow_mut().push(url.to_owned()));
        Ok(())
    }

    #[test]
    fn the_link_goes_connecting_live_down_live() {
        let mut app = App::headless();
        let s = || summary(vec![row("a", TaskStatus::Building, 1000)]);
        assert_eq!(app.link, LinkState::Connecting);
        assert!(app.apply_link(LinkEvent::Live(s())));
        assert_eq!(app.link, LinkState::Live);
        // The same workbench again changes nothing, so the window does not redraw.
        assert!(!app.apply_link(LinkEvent::Live(s())));
        assert!(!app.apply_link(LinkEvent::Quiet));
        assert!(app.apply_link(down(LinkError::Dropped, 1)));
        assert!(matches!(&app.link, LinkState::Down { message } if message.contains("dropped")));
        assert_eq!(app.summary.as_ref().map(|s| s.tasks.len()), Some(1), "what was known stays");
        assert!(app.apply_link(LinkEvent::Live(s())));
        assert_eq!(app.link, LinkState::Live);
    }

    #[test]
    fn a_reconnect_after_having_data_does_not_flash_connecting() {
        let mut app = App::headless();
        app.apply_link(LinkEvent::Live(summary(vec![])));
        app.apply_link(down(LinkError::NotRunning, 1));
        app.apply_link(LinkEvent::Connecting);
        assert!(matches!(app.link, LinkState::Down { .. }));
    }

    #[test]
    fn a_window_opens_on_home_and_keeps_your_choice() {
        let mut app = App::headless();
        app.apply_link(LinkEvent::Live(summary(vec![row("work", TaskStatus::Building, 1000), row("ask", TaskStatus::AwaitingApproval, 9000)])));
        assert_eq!(app.selected, None, "a window opens on Home, not on a session");
        assert!(app.home_shown() && app.new_form.is_some());
        app.select("work");
        assert!(!app.home_shown());
        app.apply_link(LinkEvent::Live(summary(vec![row("work", TaskStatus::Building, 500), row("ask", TaskStatus::AwaitingApproval, 9000)])));
        assert_eq!(app.selected.as_deref(), Some("work"), "a new snapshot does not move the selection");
        app.apply_link(LinkEvent::Live(summary(vec![row("ask", TaskStatus::AwaitingApproval, 9000)])));
        assert_eq!(app.selected, None, "a session that is gone puts Home back");
        assert!(app.home_shown() && app.new_form.is_some());
        app.open_new();
        app.select("ask");
        app.open_new();
        assert!(app.home_shown(), "New session lands on Home from a session");
    }

    #[test]
    fn choosing_a_session_replaces_a_full_view_and_choosing_a_view_replaces_the_session() {
        let mut app = App::headless();
        app.open_view(FullView::Settings);
        assert_eq!(app.view, Some(FullView::Settings));
        app.select("t1");
        assert_eq!(app.view, None);
        app.more_open = true;
        app.open_view(FullView::Usage);
        assert!(!app.more_open, "choosing closes the menu");
    }

    #[test]
    fn groups_collapse_one_at_a_time() {
        let mut app = App::headless();
        app.toggle_group(Group::Running);
        assert_eq!(app.collapsed, [false, true, false, true], "Waiting a while starts folded");
        app.toggle_group(Group::Running);
        assert_eq!(app.collapsed, [false, false, false, true]);
        app.toggle_group(Group::WaitingAWhile);
        assert_eq!(app.collapsed, [false; 4]);
    }

    #[test]
    fn the_crew_line_says_who_works_and_what_needs_you() {
        assert_eq!(crew_line(3, 1, 0), "3 agents working · 1 needs you");
        assert_eq!(crew_line(1, 2, 27), "1 agent working · 2 need you");
        assert_eq!(crew_line(0, 0, 0), "No agents working · nothing needs you");
        assert_eq!(crew_line(0, 0, 27), "No agents working · 27 in Inbox", "the Inbox counts when no session needs you");
    }

    #[test]
    fn waiting_counts_the_needs_you_group() {
        use TaskStatus::*;
        let mut app = App::headless();
        app.apply_link(LinkEvent::Live(summary(vec![row("a", AwaitingApproval, 1), row("b", ReadyForReview, 1), row("c", Failed, 1), row("d", Building, 1), row("e", Completed, 1)])));
        assert_eq!(app.waiting(), 3);
    }

    #[test]
    fn status_lines_lead_with_the_status() {
        let s = summary(vec![row("a", TaskStatus::Building, 1), row("b", TaskStatus::Building, 1)]);
        assert_eq!(status_line(&LinkState::Live, Some(&s)), "Connected · 2 tasks");
        assert_eq!(status_line(&LinkState::Connecting, None), "Connecting to Neko…");
        let down = LinkState::Down { message: "Neko isn't running. Trying again in 4 s.".into() };
        assert_eq!(status_line(&down, None), "Neko isn't running. Trying again in 4 s.");
        assert!(status_line(&down, Some(&s)).ends_with("Last known: 2 tasks"));
    }

    #[test]
    fn open_in_web_opens_the_same_place_and_never_shows_the_token() {
        OPENED.with(|o| o.borrow_mut().clear());
        let token = "ab".repeat(32);
        let mut app = App { opener: record, web: Some(WebTarget::new(4747, &token)), ..App::headless() };
        app.open_in_web(Some("schedules"));
        app.open_in_web(None);
        assert_eq!(OPENED.with(|o| o.borrow().clone()), vec![format!("http://127.0.0.1:4747/?token={token}#/schedules"), format!("http://127.0.0.1:4747/?token={token}")]);
        assert_eq!(app.notice, None);
    }

    #[test]
    fn open_in_web_says_why_when_it_cannot() {
        let mut app = App::headless();
        app.open_in_web(None);
        assert_eq!(app.notice.as_deref(), Some("Neko isn't running, so there's no web UI to open yet."));
        app.link = LinkState::Live;
        app.open_in_web(None);
        assert!(app.notice.as_deref().is_some_and(|n| n.contains("web address isn't known")));
        let mut failing = App { opener: |_| Err(io::Error::other("no browser")), web: Some(WebTarget::new(1, &"0".repeat(64))), ..App::headless() };
        failing.open_in_web(None);
        assert_eq!(failing.notice.as_deref(), Some("Couldn't open your browser."));
    }

    #[test]
    fn the_steer_modes_are_named() {
        assert_eq!(Steer::ALL.map(Steer::label), ["Steer now", "After this turn", "Interrupt and restart"]);
    }
}
