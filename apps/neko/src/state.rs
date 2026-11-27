//! What the window holds, and what each thing the person or the daemon does changes in it. No drawing here and no I/O: the link
//! is reached only through [`App::send`].

use std::collections::{HashMap, HashSet};

use neko_protocol::{
    Briefing, BriefingSettings, Cat, Change, Decision, DiffPatch, DiffSummary, ErrorCode, GENERAL, Need, NeedsItem, Part, PartBody, Permission, PermissionId, PermissionMode, Reply, Request, Run, RunId, RunStatus, RuntimeInfo,
    ServerMessage, Settings, Task, TaskId, Transcript, UndoStorage, Workspace,
};
use rune::TextEdit;

use crate::link::{Conn, Incoming};
use crate::notify::{self, ClickDo, Here, Notifier, Switches};
use crate::window_place;

/// Under this window width the sidebar folds by itself (points).
pub const FOLD_BELOW: f32 = 1000.0;

/// How the app stands with its daemon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Daemon {
    /// Not connected yet (just launched, or the daemon is starting).
    Starting,
    Connected,
    /// The connection ended; the link is trying again. The words say why.
    Lost(String),
    /// A picture or a test with no daemon at all.
    Absent,
}

/// How many of a workspace's tasks the sidebar shows before "Show more".
pub const SECTION_ROWS: usize = 8;

/// Something only the window's transaction can do (the folder panel, the Finder), asked from a handler that has none (a menu row,
/// the picker); `lib.rs`'s `poll` does it at the next frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// The Mac's folder panel, for a new workspace (`select`: the composer picks it) or for a workspace's folder that moved.
    PickFolder(Pick),
    Reveal(String),
    /// Bring the window forward (a notification or a link opened a task).
    Front,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    Add { select: bool },
    Locate(String),
    /// A folder holding a plugin (`neko-plugin.json`), to add.
    PluginFolder,
    /// The folder a plugin's folder setting names: (plugin, setting key).
    PluginSetting(String, String),
}

/// A question the window is asking before it acts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Confirm {
    RemoveWorkspace(String),
    RemoveTask(TaskId),
    /// Undo (or redo) over files that changed since: the files, named in the question.
    UndoAnyway { task: TaskId, run: RunId, redo: bool, files: Vec<String> },
    ClearUndo,
    /// Retire a cat (its id).
    RetireCat(String),
    /// Remove a round (its id): its past tasks stay.
    RemoveRound(String),
    /// Remove a plugin (its id): its settings and secrets go.
    RemovePlugin(String),
}

/// A cat page's fields while the person edits them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CatDraft {
    pub purpose: TextEdit,
    pub rules: TextEdit,
}

impl CatDraft {
    /// The fields as the cat has them saved.
    pub fn of(cat: &Cat) -> CatDraft {
        CatDraft { purpose: TextEdit::new(cat.purpose.as_deref().unwrap_or("")), rules: TextEdit::new(&cat.rules) }
    }

    /// Whether anything differs from what the cat has saved.
    pub fn unsaved(&self, cat: &Cat) -> bool {
        self.rules.text != cat.rules || self.purpose.text.split_whitespace().collect::<Vec<_>>().join(" ") != cat.purpose.clone().unwrap_or_default()
    }
}

/// What a cat is doing, as the sidebar and its page say it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatState {
    Napping,
    Working(usize),
    NeedsYou,
}

impl CatState {
    pub fn words(self) -> String {
        match self {
            CatState::Napping => "napping".to_owned(),
            CatState::Working(n) => format!("working on {n}"),
            CatState::NeedsYou => "needs you".to_owned(),
        }
    }
}

/// The prefs key of the cat picked last in each workspace.
pub const CAT_PICKS_KEY: &str = "cat-picks";

/// A run's changes (or the task's, with no run), open in place of the conversation (docs/knowledge/undo.md).
#[derive(Clone, Debug, PartialEq)]
pub struct Review {
    pub task: TaskId,
    /// None: all of the task's changes.
    pub run: Option<RunId>,
    /// The run the person opened it from: "This run" goes back to it.
    pub from: Option<RunId>,
    pub summary: Option<DiffSummary>,
    /// Why there is nothing to show.
    pub error: Option<String>,
    /// Each file's lines, as they arrive.
    pub patches: HashMap<String, DiffPatch>,
    /// Files open (each is folded until the person opens it).
    pub open: HashSet<String>,
}

/// Why something was not done, in a small panel: the words, and a folder to reveal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub title: String,
    pub words: String,
    pub reveal: Option<String>,
}

pub struct App {
    /// Every task of every workspace, newest first, as the daemon last said.
    pub tasks: Vec<Task>,
    /// Every cat: the active ones first, then the retired (as the daemon last said).
    pub cats: Vec<Cat>,
    /// The cat picked in Home's composer for each workspace (the last explicit choice there). Remembered for the next launch.
    pub cat_picks: HashMap<String, String>,
    /// The composer's cat picker is open.
    pub cat_picker_open: bool,
    /// The cat whose page shows instead of the content.
    pub open_cat: Option<String>,
    /// Each cat page's purpose and rules as the person is editing them (Save sends them; Revert puts the saved ones back).
    pub cat_drafts: HashMap<String, CatDraft>,
    /// The panel that names a cat: a new one (None) or a rename (its id), and the name being typed.
    pub naming: Option<(Option<String>, TextEdit)>,
    /// The workspaces: General first, then the person's order, then the removed ones (as the daemon last said).
    pub workspaces: Vec<Workspace>,
    /// The workspace Home's composer makes the next task in (General's id by default). Remembered for the next launch.
    pub workspace: String,
    /// The composer's workspace picker is open.
    pub picker_open: bool,
    /// Sections of the sidebar the person folded, by workspace id. Remembered for the next launch.
    pub folded: HashSet<String>,
    /// Sections showing every task, not only the newest [`SECTION_ROWS`].
    pub show_all: HashSet<String>,
    /// The menu that is open (a workspace's, by `ws-menu-<id>`, or the task's, `task-more`).
    pub menu_open: Option<String>,
    /// Renaming a workspace: its id and the name being typed.
    pub renaming: Option<(String, TextEdit)>,
    pub confirm: Option<Confirm>,
    pub refusal: Option<Refusal>,
    /// What `poll` does at the next frame.
    pub effects: Vec<Effect>,
    /// The open task; None is Home.
    pub open: Option<TaskId>,
    /// The words in Home's composer.
    pub draft: TextEdit,
    pub daemon: Daemon,
    pub conn: Option<Conn>,
    /// The request that is creating a task from these words (kept until the daemon answers, then cleared or put back).
    pub creating: Option<(u64, String)>,
    /// One quiet line under the composer: why the last task could not be added.
    pub notice: Option<String>,
    /// The person's own choice of sidebar (⌃⌘S), until the window crosses [`FOLD_BELOW`] again.
    pub sidebar_choice: Option<bool>,
    /// The window's width at the last frame (None before the first layout).
    pub width: Option<f32>,
    /// Now, for "Today" and the greeting: milliseconds since the epoch, and the local zone's offset from UTC in seconds.
    pub now_ms: i64,
    pub utc_offset_s: i64,
    pub clock: fn() -> (i64, i64),
    /// The window's frame keeper (`window_place.rs`); `persist` only in the real window.
    pub window: window_place::Keeper,
    pub persist: bool,
    /// The title the window was last given.
    pub title_set: String,
    /// What the menu bar last showed (sidebar shown, full screen), so it is replaced only when that changes.
    pub menu_shown: Option<(bool, bool)>,
    pub full_screen: bool,
    /// Until when (seconds of the window's clock) the raw menu fix-ups keep running after a menu change.
    pub menu_fix_until: f64,
    pub handle: Option<rune::AppHandle<App>>,
    /// Each task's runs, parts and requests, as the daemon last said (loaded when the task is opened, kept by events).
    pub books: HashMap<TaskId, Book>,
    /// The words in each task's own composer.
    pub replies: HashMap<TaskId, TextEdit>,
    /// Folds the person opened (by key).
    pub open_folds: HashSet<String>,
    /// Whether the task's conversation shows its newest content (it reports this).
    pub at_bottom: bool,
    /// Settings (⌘,) shows instead of the content.
    pub settings_open: bool,
    /// Which of Settings' pages shows (`settings_view::PAGES`).
    pub settings_page: usize,
    /// The agent CLIs and the settings, as the daemon last said.
    pub runtimes: Option<(Vec<RuntimeInfo>, Settings)>,
    /// The person's home folder, so paths under it read `~/…` (None: shown whole).
    pub home: Option<String>,
    /// Whether each message's actions show (pointer over it or focus in it), by the message's key (rune-ui's `reveal_actions`).
    pub reveals: HashMap<String, crate::ui::message::Reveal>,
    /// The context ring's panel is open (in the open task's composer).
    pub context_open: bool,
    /// The task whose words stream to this window.
    pub watching: Option<TaskId>,
    /// Requests sent and what their reply is for.
    pub asked: HashMap<u64, Asked>,
    /// Something to say to assistive technology at the next frame (the words, and whether it interrupts).
    pub announce: Option<(String, bool)>,
    /// Return was pressed in Home before the daemon answered: the words in the composer are sent as soon as it does.
    pub queued: bool,
    /// The daemon's list of tasks has arrived (until then the sidebar says it is loading).
    pub tasks_loaded: bool,
    /// Since when the app has waited for its daemon (None: connected, or no daemon at all).
    pub waiting_since: Option<std::time::Instant>,
    /// How long the app waits for its daemon before it says so and offers Retry.
    pub slow_after: std::time::Duration,
    /// The slow line was drawn (so the frame that crosses the wait redraws once).
    pub slow_shown: bool,
    /// What waits for the person, oldest first, grouped by workspace: the daemon's list, replaced whole each time it changes.
    pub needs: Vec<NeedsItem>,
    /// The daemon's list has arrived.
    pub needs_loaded: bool,
    /// The Needs you list shows instead of the content (⌘2).
    pub needs_open: bool,
    /// The task the person came from to the list (Escape goes back to it).
    pub needs_back: Option<TaskId>,
    /// Items answered here, hidden at once until the daemon's list drops them (or a refusal brings one back).
    pub needs_hidden: HashSet<String>,
    /// The words in each question's reply box, by item.
    pub needs_replies: HashMap<String, TextEdit>,
    /// One quiet line over the list ("That was already answered.").
    pub needs_notice: Option<String>,
    /// The element to focus at the next frame (the next item after one is answered).
    pub focus_next: Option<String>,
    /// The request in the open task the window last moved the keyboard for (a new one takes it out of the composer).
    pub request_seen: Option<PermissionId>,
    /// The notification model and its poster (a recorder everywhere but the real window).
    pub notifier: Notifier,
    /// Neko is the frontmost app with its window focused (the real window reads it every frame).
    pub front: bool,
    /// Whether the system lets Neko post ([`notify::Allowed`]'s code), written by the system's answer on another thread.
    pub allowed: std::sync::Arc<std::sync::atomic::AtomicU8>,
    /// A task a link or a notification asked for before the daemon's list arrived.
    pub pending_link: Option<TaskId>,
    /// Clicks on a notification that came before the daemon answered (one may have launched the app): done once it does.
    pub pending_clicks: Vec<ClickDo>,
    /// The main window, for the Dock badge (the real window only).
    pub window_handle: Option<rune::WindowHandle>,
    /// The count the Dock icon shows now.
    pub badge_set: Option<u32>,
    /// A run's changes, open in the task (Review).
    pub review: Option<Review>,
    /// How much room undo takes (Settings ▸ Workspaces), as the daemon last said.
    pub undo_storage: Option<UndoStorage>,
    /// Each finished run's files with their own counts, for its receipt: asked once per run that changed something.
    pub receipts: HashMap<RunId, DiffSummary>,
    /// The runs whose files were asked for (each is asked once).
    pub receipts_asked: HashSet<RunId>,
    /// Every round, oldest first, as the daemon last said (with each one's next time, status and last line).
    pub rounds: Vec<neko_protocol::Round>,
    /// Every round is paused.
    pub rounds_paused: bool,
    /// The daemon's list of rounds has arrived.
    pub rounds_loaded: bool,
    /// The Rounds page shows instead of the content (⌘3).
    pub rounds_open: bool,
    /// The New round / Edit panel, while it is open.
    pub round_form: Option<crate::rounds::RoundForm>,
    /// Every plugin, oldest first, as the daemon last said (docs/knowledge/plugins.md).
    pub plugins: Vec<neko_protocol::Plugin>,
    /// The Add sheet, what is pending on plugins, and the fields being typed in (`plugins.rs`).
    pub plugin_ui: crate::plugins::Ui,
    /// The rounds whose history shows on their card, and each one's lines (newest first) as the daemon last said.
    pub round_history_open: HashSet<String>,
    pub round_history: HashMap<String, Vec<neko_protocol::RoundRun>>,
    /// What happened while the person was away, as the daemon last said (replaced whole each time it changes).
    pub briefing: Option<Briefing>,
    /// The Briefing page shows instead of the content (⌘4).
    pub briefing_open: bool,
    /// The task the person came from to the Briefing (Escape goes back to it).
    pub briefing_back: Option<TaskId>,
    /// The morning time and whether its notification is posted, as the daemon last said.
    pub briefing_settings: Option<BriefingSettings>,
    /// The morning time being typed in Settings ▸ Briefing.
    pub briefing_time: TextEdit,
}

impl Review {
    pub fn new(task: TaskId, run: Option<RunId>, from: Option<RunId>) -> Review {
        Review { task, run, from, summary: None, error: None, patches: HashMap::new(), open: HashSet::new() }
    }
}

/// The picker's last line: a folder the person adds as a workspace, there and then.
pub const ADD_FOLDER: &str = "Add folder…";

/// How long the app waits for its daemon before a calm line says so, with Retry.
pub const SLOW_AFTER: std::time::Duration = std::time::Duration::from_secs(6);

/// What a sent request's reply is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asked {
    /// A follow-up: its words go back in the box if it is refused.
    FollowUp(TaskId, String),
    /// A folder added as a workspace; `select`: the composer makes the next task there.
    AddWorkspace { select: bool },
    /// A change to a workspace (rename, remove, locate, move): a refusal is said in a panel.
    Workspace,
    RemoveTask(TaskId),
    /// An answer from the Needs you list or a notification: a refusal says so plainly.
    Answer { item: String, task: TaskId, from_notification: bool },
    /// Undo or redo of a run: files changed since ask first; a refusal is said in a panel.
    Undo { task: TaskId, run: RunId, redo: bool },
    /// A review's file list, or one file's lines.
    Diff,
    /// A run's files for its receipt.
    Receipt,
    /// A change to a cat: a refusal is said in a panel. `open`: the cat made opens its page.
    Cat { open: bool },
    ClearUndo,
    /// A change to a round (switch, pause, remove): a refusal is said in a panel.
    Round,
    /// Save in the round panel: it closes when the daemon accepts it, and says why when it does not.
    RoundForm,
    /// Run now: a refusal is said in a panel.
    RoundRun,
    /// A round's history, for its card.
    RoundHistory(String),
    /// Add in the Add sheet: it closes when the daemon accepts the paste, and says why when it does not.
    PluginAdd,
    /// A folder added as a plugin.
    PluginFolder,
    /// A plugin's check: the answer comes up to ten seconds later.
    PluginTest(String),
    /// A change to one plugin (approve, decline, switch, a setting, who may use it); `item` is the Needs you card it came from.
    Plugin { plugin: String, item: Option<String>, did: crate::plugins::Did },
    PluginRemove(String),
    /// "Mark as read" in the Briefing.
    BriefingRead,
    /// A change to the briefing's settings (the morning time, its switch).
    BriefingSettings,
    Other,
}

/// A task's transcript as the window holds it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Book {
    pub runs: Vec<Run>,
    pub parts: Vec<Part>,
    pub permissions: HashMap<PermissionId, Permission>,
    /// The daemon's whole transcript arrived (until then, events only add to what is there).
    pub loaded: bool,
    /// The agent is thinking now (the daemon says so as it happens; never stored).
    pub thinking: bool,
}

impl Book {
    pub fn from(t: Transcript) -> Book {
        Book { runs: t.runs, parts: t.parts, permissions: t.permissions.into_iter().map(|p| (p.id.clone(), p)).collect(), loaded: true, thinking: false }
    }

    fn run(&mut self, r: Run) {
        match self.runs.iter_mut().find(|x| x.id == r.id) {
            Some(x) => *x = r,
            None => {
                self.runs.push(r);
                self.runs.sort_by(|a, b| (a.started_ms, &a.id.0).cmp(&(b.started_ms, &b.id.0)));
            }
        }
    }

    fn part(&mut self, p: Part) {
        match self.parts.iter_mut().find(|x| x.id == p.id) {
            Some(x) => *x = p,
            None => {
                let at = self.parts.iter().position(|x| x.seq > p.seq).unwrap_or(self.parts.len());
                self.parts.insert(at, p);
            }
        }
    }

    fn delta(&mut self, part: &neko_protocol::PartId, more: &str) {
        if let Some(p) = self.parts.iter_mut().find(|x| &x.id == part) {
            match &mut p.body {
                PartBody::Agent { text, .. } | PartBody::Thinking { text, .. } => text.push_str(more),
                _ => {}
            }
        }
    }

    /// The run in progress, if any.
    pub fn running(&self) -> Option<&Run> {
        self.runs.iter().rev().find(|r| r.status == RunStatus::Running)
    }

    /// What the task's runs cost so far, as reported (None when none reported a cost).
    pub fn cost(&self) -> Option<f64> {
        self.runs.iter().filter_map(|r| r.usage.as_ref().and_then(|u| u.cost_usd)).fold(None, |a, c| Some(a.unwrap_or(0.0) + c))
    }
}

/// The key of an item's row in the Needs you list.
pub fn need_key(item: &str) -> String {
    format!("need-{item}")
}

/// What is left, in words, after an item leaves the list.
fn left_words(n: usize) -> String {
    match n {
        0 => "Nothing needs you now.".to_owned(),
        1 => "1 thing needs you.".to_owned(),
        n => format!("{n} things need you."),
    }
}

/// The real clock: now and the local offset.
pub fn system_clock() -> (i64, i64) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    (now, crate::clock::local_offset_s(now / 1000))
}

impl Default for App {
    fn default() -> Self {
        App {
            tasks: Vec::new(),
            cats: Vec::new(),
            cat_picks: HashMap::new(),
            cat_picker_open: false,
            open_cat: None,
            cat_drafts: HashMap::new(),
            naming: None,
            workspaces: Vec::new(),
            workspace: GENERAL.to_owned(),
            picker_open: false,
            folded: HashSet::new(),
            show_all: HashSet::new(),
            menu_open: None,
            renaming: None,
            confirm: None,
            refusal: None,
            effects: Vec::new(),
            open: None,
            draft: TextEdit::default(),
            daemon: Daemon::Absent,
            conn: None,
            creating: None,
            notice: None,
            sidebar_choice: None,
            width: None,
            now_ms: 0,
            utc_offset_s: 0,
            clock: || (0, 0),
            window: window_place::Keeper::default(),
            persist: false,
            title_set: String::new(),
            menu_shown: None,
            full_screen: false,
            menu_fix_until: 0.0,
            handle: None,
            books: HashMap::new(),
            replies: HashMap::new(),
            open_folds: HashSet::new(),
            at_bottom: true,
            settings_open: false,
            settings_page: 0,
            runtimes: None,
            home: None,
            reveals: HashMap::new(),
            context_open: false,
            watching: None,
            asked: HashMap::new(),
            announce: None,
            queued: false,
            tasks_loaded: false,
            waiting_since: None,
            slow_after: SLOW_AFTER,
            slow_shown: false,
            needs: Vec::new(),
            needs_loaded: false,
            needs_open: false,
            needs_back: None,
            needs_hidden: HashSet::new(),
            needs_replies: HashMap::new(),
            needs_notice: None,
            focus_next: None,
            request_seen: None,
            notifier: Notifier::default(),
            front: true,
            allowed: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
            pending_link: None,
            pending_clicks: Vec::new(),
            window_handle: None,
            badge_set: None,
            review: None,
            undo_storage: None,
            receipts: HashMap::new(),
            receipts_asked: HashSet::new(),
            rounds: Vec::new(),
            rounds_paused: false,
            rounds_loaded: false,
            rounds_open: false,
            round_form: None,
            plugins: Vec::new(),
            plugin_ui: crate::plugins::Ui::default(),
            round_history_open: HashSet::new(),
            round_history: HashMap::new(),
            briefing: None,
            briefing_open: false,
            briefing_back: None,
            briefing_settings: None,
            briefing_time: TextEdit::default(),
        }
    }
}

/// Local midnight of the day `now_ms` is in, in milliseconds since the epoch.
pub fn midnight(now_ms: i64, utc_offset_s: i64) -> i64 {
    let local = now_ms / 1000 + utc_offset_s;
    (local.div_euclid(86_400) * 86_400 - utc_offset_s) * 1000
}

impl App {
    /// The open task, if it is one the daemon has.
    pub fn open_task(&self) -> Option<&Task> {
        self.open.as_ref().and_then(|id| self.tasks.iter().find(|t| &t.id == id))
    }

    /// What the window's title says: the open task's title and its workspace's name, Settings, Needs you, else "Neko".
    pub fn window_title(&self) -> String {
        if self.settings_open {
            return "Settings".to_owned();
        }
        if self.needs_open {
            return "Needs you".to_owned();
        }
        if self.rounds_open {
            return "Rounds".to_owned();
        }
        if self.briefing_open {
            return "Briefing".to_owned();
        }
        match self.open_task() {
            Some(t) => match self.workspace_of(&t.workspace) {
                Some(w) => format!("{} — {}", t.title, w.name),
                None => t.title.clone(),
            },
            None => "Neko".to_owned(),
        }
    }

    /// A cat by its id, active or retired.
    pub fn cat(&self, id: &str) -> Option<&Cat> {
        self.cats.iter().find(|c| c.id == id)
    }

    /// The cat Home's composer gives the next task to (None: "Any cat", the daemon chooses).
    pub fn cat_pick(&self) -> Option<&str> {
        self.cat_picks.get(&self.workspace).map(String::as_str).filter(|id| self.cat(id).is_some_and(Cat::is_active))
    }

    /// Adds or replaces a cat, keeping the daemon's order (active first, oldest first; then the retired).
    fn upsert_cat(&mut self, cat: Cat) {
        // A page's fields follow a change saved elsewhere, unless the person has unsaved words in them.
        if let (Some(old), Some(d)) = (self.cat(&cat.id), self.cat_drafts.get(&cat.id))
            && !d.unsaved(old)
        {
            let mut fresh = CatDraft::of(&cat);
            fresh.rules.focused = d.rules.focused;
            fresh.purpose.focused = d.purpose.focused;
            self.cat_drafts.insert(cat.id.clone(), fresh);
        }
        self.cats.retain(|c| c.id != cat.id);
        self.cats.push(cat);
        self.cats.sort_by(|a, b| (!a.is_active(), a.created_ms, &a.id).cmp(&(!b.is_active(), b.created_ms, &b.id)));
    }

    /// The active cats, oldest first.
    pub fn active_cats(&self) -> Vec<&Cat> {
        self.cats.iter().filter(|c| c.is_active()).collect()
    }

    /// A task's cat's name ("Neko" until the cats arrive).
    /// A cat's face id (the first face for a cat not known yet, as the portrait draws an unknown one).
    pub fn cat_face(&self, id: &str) -> &str {
        self.cat(id).map_or("", |c| c.face.as_str())
    }

    pub fn cat_name(&self, task: &Task) -> String {
        self.cat(&task.cat).map_or_else(|| "Neko".to_owned(), |c| c.name.clone())
    }

    /// What a cat is doing now: something of it waits for the person, it works on some tasks, or it naps.
    pub fn cat_state(&self, id: &str) -> CatState {
        if self.visible_needs().iter().any(|i| i.cat == id) {
            return CatState::NeedsYou;
        }
        match self.tasks.iter().filter(|t| t.cat == id && t.status.is_active()).count() {
            0 => CatState::Napping,
            n => CatState::Working(n),
        }
    }

    /// "Any cat" (None) or a cat for the next task in the chosen workspace, remembered for that workspace.
    pub fn pick_cat(&mut self, cat: Option<String>) {
        self.cat_picker_open = false;
        match cat {
            Some(c) => self.cat_picks.insert(self.workspace.clone(), c),
            None => self.cat_picks.remove(&self.workspace),
        };
        if self.persist {
            let picks = self.cat_picks.clone();
            let _ = std::thread::Builder::new().name("neko-prefs".into()).spawn(move || {
                let _ = rune::prefs::Prefs::open(crate::PREFS_ID).set(CAT_PICKS_KEY, &picks);
            });
        }
    }

    /// A cat's page, in place of the content.
    pub fn open_cat_page(&mut self, id: &str) {
        let Some(cat) = self.cat(id).cloned() else { return };
        self.open = None;
        self.review = None;
        self.context_open = false;
        self.settings_open = false;
        self.needs_open = false;
        self.rounds_open = false;
        self.briefing_open = false;
        self.menu_open = None;
        let d = self.cat_drafts.entry(cat.id.clone()).or_insert_with(|| CatDraft::of(&cat));
        // Opening a page puts the keyboard in none of its fields.
        d.rules.focused = false;
        d.purpose.focused = false;
        self.open_cat = Some(cat.id);
        self.follow_open();
    }

    /// The open cat, when there is one the daemon has.
    pub fn page_cat(&self) -> Option<&Cat> {
        self.open_cat.as_deref().and_then(|id| self.cat(id))
    }

    pub fn cat_draft_mut(&mut self, id: &str) -> &mut CatDraft {
        let saved = self.cat(id).map(CatDraft::of).unwrap_or_default();
        self.cat_drafts.entry(id.to_owned()).or_insert(saved)
    }

    fn cat_request(&mut self, r: Request, open: bool) {
        if let Some(id) = self.send(r) {
            self.asked.insert(id, Asked::Cat { open });
        }
    }

    /// Save on a cat's page: the purpose and the rules, each only when it changed.
    pub fn save_cat(&mut self, id: &str) {
        let (Some(cat), Some(d)) = (self.cat(id).cloned(), self.cat_drafts.get(id).cloned()) else { return };
        let purpose = d.purpose.text.split_whitespace().collect::<Vec<_>>().join(" ");
        if purpose != cat.purpose.clone().unwrap_or_default() {
            self.cat_request(Request::SetCatPurpose { cat: cat.id.clone(), purpose }, false);
        }
        if d.rules.text != cat.rules {
            self.cat_request(Request::SaveCatRules { cat: cat.id.clone(), rules: d.rules.text.clone() }, false);
        }
    }

    /// Revert on a cat's page: the saved purpose and rules back in the fields.
    pub fn revert_cat(&mut self, id: &str) {
        if let Some(cat) = self.cat(id).cloned() {
            self.cat_drafts.insert(cat.id.clone(), CatDraft::of(&cat));
        }
    }

    /// Restore previous rules: the daemon swaps them; the fields follow.
    pub fn restore_cat_rules(&mut self, id: &str) {
        self.cat_drafts.remove(id);
        self.cat_request(Request::RestoreCatRules { cat: id.to_owned() }, false);
    }

    pub fn set_cat_face(&mut self, id: &str, face: &str) {
        self.menu_open = None;
        self.cat_request(Request::SetCatFace { cat: id.to_owned(), face: face.to_owned() }, false);
    }

    pub fn set_cat_workspace(&mut self, id: &str, workspace: Option<String>) {
        self.menu_open = None;
        self.cat_request(Request::SetCatWorkspace { cat: id.to_owned(), workspace }, false);
    }

    /// New cat…: the panel that names it.
    pub fn start_new_cat(&mut self) {
        let mut edit = TextEdit::default();
        edit.focused = true;
        self.naming = Some((None, edit));
    }

    pub fn start_rename_cat(&mut self, id: &str) {
        if let Some(c) = self.cat(id) {
            let mut edit = TextEdit::new(&c.name);
            edit.focused = true;
            self.naming = Some((Some(c.id.clone()), edit));
        }
    }

    /// The naming panel's button: make the cat (its page opens) or rename it.
    pub fn finish_naming(&mut self) {
        let Some((id, edit)) = self.naming.take() else { return };
        let name = edit.text.trim().to_owned();
        if name.is_empty() {
            return;
        }
        match id {
            None => self.cat_request(Request::CreateCat { name, face: None, purpose: None, workspace: None }, true),
            Some(id) if self.cat(&id).is_some_and(|c| c.name != name) => self.cat_request(Request::RenameCat { cat: id, name }, false),
            Some(_) => {}
        }
    }

    pub fn ask_retire_cat(&mut self, id: &str) {
        self.menu_open = None;
        self.confirm = Some(Confirm::RetireCat(id.to_owned()));
    }

    /// Bring back (Settings ▸ Cats).
    pub fn bring_back_cat(&mut self, id: &str) {
        self.cat_request(Request::BringBackCat { cat: id.to_owned() }, false);
    }

    pub fn workspace_of(&self, id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    /// The workspace Home's composer makes the next task in (General until the list arrives).
    pub fn chosen(&self) -> Option<&Workspace> {
        self.workspace_of(&self.workspace)
    }

    /// The workspace's tasks, newest first.
    pub fn tasks_in(&self, id: &str) -> Vec<&Task> {
        self.tasks.iter().filter(|t| t.workspace == id).collect()
    }

    /// How many things in the workspace need the person (the sidebar's quiet count): the daemon's items there.
    pub fn needs_in(&self, id: &str) -> usize {
        self.visible_needs().iter().filter(|i| i.workspace == id).count()
    }

    /// The items the list shows: the daemon's, less those answered here and not yet gone from its list.
    pub fn visible_needs(&self) -> Vec<&NeedsItem> {
        self.needs.iter().filter(|i| !self.needs_hidden.contains(&i.id)).collect()
    }

    /// The count on the Dock icon: everything waiting, while its switch is on.
    pub fn badge_count(&self) -> u32 {
        if self.notifier.switches.badge { self.visible_needs().len() as u32 } else { 0 }
    }

    /// Whether the system lets Neko post, as it last said.
    pub fn allowed(&self) -> notify::Allowed {
        notify::Allowed::from_code(self.allowed.load(std::sync::atomic::Ordering::SeqCst))
    }

    /// Where the person is, for the notifications.
    pub fn here(&self) -> Here {
        let looking = !self.settings_open && !self.needs_open && !self.rounds_open && !self.briefing_open;
        Here { front: self.front, task: self.open.clone().filter(|_| looking), needs_view: self.needs_open || self.briefing_open, briefing_view: self.briefing_open }
    }

    /// The Needs you list (⌘2, the sidebar's row), with the keyboard on its first item.
    pub fn open_needs(&mut self) {
        if !self.needs_open {
            self.needs_back = self.open.clone().filter(|_| !self.settings_open);
        }
        self.open = None;
        self.open_cat = None;
        self.review = None;
        self.context_open = false;
        self.settings_open = false;
        self.rounds_open = false;
        self.briefing_open = false;
        self.needs_open = true;
        self.needs_notice = None;
        self.follow_open();
        self.focus_next = self.visible_needs().first().map(|i| need_key(&i.id));
    }

    /// Escape in the list: back to the task the person came from, else Home.
    pub fn leave_needs(&mut self) {
        if self.briefing_open {
            return self.leave_briefing();
        }
        match self.needs_back.take().filter(|t| self.tasks.iter().any(|x| &x.id == t)) {
            Some(t) => self.open_task_id(&t),
            None => self.go_home(),
        }
    }

    /// The daemon's list, as it is now.
    fn set_needs(&mut self, items: Vec<NeedsItem>) {
        let before = self.visible_needs().len();
        self.needs = items;
        self.needs_loaded = true;
        let live: HashSet<String> = self.needs.iter().map(|i| i.id.clone()).collect();
        self.needs_hidden.retain(|h| live.contains(h));
        self.needs_replies.retain(|k, _| live.contains(k));
        let here = self.here();
        self.notifier.needs(&self.needs, &here);
        let after = self.visible_needs().len();
        if self.needs_open && after < before {
            self.announce = Some((left_words(after), false));
        }
    }

    /// Hides an answered item at once and gives the keyboard to the next one (or the one before it, at the end).
    pub(crate) fn answered(&mut self, item: &str) {
        let shown: Vec<String> = self.visible_needs().iter().map(|i| i.id.clone()).collect();
        let at = shown.iter().position(|i| i == item);
        self.needs_hidden.insert(item.to_owned());
        self.needs_notice = None;
        if let Some(at) = at {
            let next = shown.get(at + 1).or_else(|| at.checked_sub(1).and_then(|b| shown.get(b)));
            // After the last one, the keyboard goes to the sidebar's Needs you row (its empty page has no control of its own).
            self.focus_next = Some(next.map_or_else(|| "needs-you".to_owned(), |n| need_key(n)));
            self.announce = Some((left_words(shown.len() - 1), false));
        }
    }

    /// An answer to a request in the list: the same request the task's own control sends.
    pub fn answer_need(&mut self, item: &str, decision: Decision) {
        let Some((task, perm)) = self.needs.iter().find(|i| i.id == item).and_then(|i| if let Need::Permission { permission } = &i.need { Some((i.task.clone(), permission.id.clone())) } else { None }) else { return };
        if let Some(id) = self.send(Request::AnswerPermission { permission: perm, decision }) {
            self.asked.insert(id, Asked::Answer { item: item.to_owned(), task, from_notification: false });
            self.answered(item);
        }
    }

    /// The words in a question's reply box: a follow-up that answers that run's question (refused if it was answered already).
    pub fn reply_need(&mut self, item: &str) {
        let Some((task, run)) = self.needs.iter().find(|i| i.id == item).and_then(|i| if let Need::Question { run, .. } = &i.need { Some((i.task.clone(), run.clone())) } else { None }) else { return };
        let words = self.needs_replies.get(item).map(|r| r.text.clone()).unwrap_or_default();
        if words.trim().is_empty() {
            return;
        }
        if let Some(id) = self.send(Request::FollowUp { task: task.clone(), words, answering: Some(run) }) {
            self.asked.insert(id, Asked::Answer { item: item.to_owned(), task, from_notification: false });
            self.needs_replies.remove(item);
            self.answered(item);
        }
    }

    /// Opening an item: its task, where it waits (the newest part).
    pub fn open_need(&mut self, item: &str) {
        if let Some(task) = self.needs.iter().find(|i| i.id == item).map(|i| i.task.clone()) {
            self.open_task_id(&task);
        }
    }

    /// The reply box of a question in the list.
    pub fn need_reply_mut(&mut self, item: &str) -> &mut TextEdit {
        self.needs_replies.entry(item.to_owned()).or_default()
    }

    /// A click on one of Neko's notifications (from `rune-notify`, any thread, handed here through the app's handle).
    pub fn clicked(&mut self, action: Option<String>, typed: Option<String>, info: Vec<(String, String)>) {
        let what = notify::read_click(action.as_deref(), typed.as_deref(), &info);
        self.act_on(what);
    }

    fn act_on(&mut self, what: ClickDo) {
        if !matches!(what, ClickDo::Open(_) | ClickDo::OpenBriefing | ClickDo::OpenNeeds(_) | ClickDo::Nothing) && self.daemon != Daemon::Connected {
            self.pending_clicks.push(what);
            return;
        }
        match what {
            ClickDo::Open(task) => {
                self.effects.push(Effect::Front);
                self.go_to_task(task);
            }
            ClickDo::OpenNeeds(item) => {
                self.effects.push(Effect::Front);
                self.open_needs();
                if self.visible_needs().iter().any(|i| i.id == item) {
                    self.focus_next = Some(need_key(&item));
                }
            }
            ClickDo::Answer { task, permission, decision } => {
                let item = format!("perm:{}", permission.0);
                if let Some(id) = self.send(Request::AnswerPermission { permission, decision }) {
                    self.asked.insert(id, Asked::Answer { item: item.clone(), task, from_notification: true });
                    self.needs_hidden.insert(item);
                }
            }
            ClickDo::Reply { task, run, words } => {
                let item = format!("question:{}", run.0);
                if let Some(id) = self.send(Request::FollowUp { task: task.clone(), words, answering: Some(run) }) {
                    self.asked.insert(id, Asked::Answer { item: item.clone(), task, from_notification: true });
                    self.needs_hidden.insert(item);
                }
            }
            ClickDo::OpenBriefing => {
                self.effects.push(Effect::Front);
                self.open_briefing();
            }
            ClickDo::Nothing => {}
        }
    }

    /// A link the system opened with Neko (`neko://task/<id>`). Anything else is ignored. True when it was one of Neko's.
    pub fn open_link(&mut self, link: &str) -> bool {
        if crate::deeplink::is_briefing(link) {
            self.effects.push(Effect::Front);
            self.open_briefing();
            return true;
        }
        let Some(task) = crate::deeplink::parse(link) else { return false };
        self.effects.push(Effect::Front);
        self.go_to_task(task);
        true
    }

    /// Opens a task a link or a notification named: now if the window knows it, once the list arrives if it does not yet, and
    /// Home with a quiet line if it is gone.
    fn go_to_task(&mut self, task: TaskId) {
        if self.tasks.iter().any(|t| t.id == task) {
            self.open_task_id(&task);
        } else if !self.tasks_loaded {
            self.pending_link = Some(task);
        } else {
            self.go_home();
            self.notice = Some("That task is not in Neko any more.".to_owned());
        }
    }

    /// The switches on Settings ▸ Notifications, remembered for the next launch in the real window.
    pub fn set_switches(&mut self, sw: Switches) {
        self.notifier.switches = sw;
        if self.persist {
            let _ = std::thread::Builder::new().name("neko-prefs".into()).spawn(move || {
                let _ = rune::prefs::Prefs::open(crate::PREFS_ID).set(notify::SWITCHES_KEY, &sw);
            });
        }
    }

    /// "Send a test notification" (the system asks the person first, the first time).
    pub fn send_test(&mut self) {
        self.notifier.test();
        self.read_allowed_soon();
    }

    /// Asks the system again whether Neko may post (the real window only; it never asks the person).
    pub fn read_allowed_soon(&self) {
        if self.persist {
            notify::read_allowed(self.allowed.clone());
        }
    }

    /// The newest run's last words in a task, when the window has them.
    fn last_words(&self, task: &TaskId) -> Option<String> {
        let b = self.books.get(task)?;
        let run = b.runs.last()?;
        b.parts.iter().rev().find_map(|p| match &p.body {
            PartBody::Agent { text, .. } if p.run.as_ref() == Some(&run.id) && !text.trim().is_empty() => Some(text.clone()),
            _ => None,
        })
    }

    /// The sections of the sidebar: General, the person's workspaces, then the removed ones that still have tasks. Before the
    /// daemon's list arrives, General alone.
    pub fn sections(&self) -> Vec<Workspace> {
        if self.workspaces.is_empty() {
            return vec![Workspace { id: GENERAL.into(), name: "General".into(), folder: None, created_ms: 0, removed_ms: None, position: 0, missing: false }];
        }
        self.workspaces.iter().filter(|w| w.is_active() || self.tasks.iter().any(|t| t.workspace == w.id)).cloned().collect()
    }

    /// The composer's picker: each workspace a new task can be made in (one line each, the same name told apart by its folder),
    /// then "Add folder…".
    pub fn picker_options(&self) -> Vec<(String, Option<String>)> {
        let live: Vec<&Workspace> = if self.workspaces.is_empty() { Vec::new() } else { self.workspaces.iter().filter(|w| w.is_active()).collect() };
        let mut out: Vec<(String, Option<String>)> = Vec::new();
        if live.is_empty() {
            out.push(("General".into(), Some(GENERAL.into())));
        }
        for w in &live {
            let twins = live.iter().filter(|x| x.name == w.name).count() > 1;
            let mut label = w.name.clone();
            if twins && let Some(f) = &w.folder {
                label = format!("{} ({})", w.name, crate::task_view::show_path(f, "", self.home.as_deref()));
            }
            if w.missing {
                label = format!("{label} (not found)");
            }
            out.push((label, Some(w.id.clone())));
        }
        out.push((ADD_FOLDER.into(), None));
        out
    }

    /// A choice in the picker, by its words.
    pub fn pick(&mut self, label: &str) {
        self.picker_open = false;
        match self.picker_options().into_iter().find(|(l, _)| l == label).and_then(|(_, id)| id) {
            Some(id) => self.choose_workspace(&id),
            None if label == ADD_FOLDER => self.effects.push(Effect::PickFolder(Pick::Add { select: true })),
            None => {}
        }
    }

    /// The workspace new tasks go to, remembered for the next launch in the real window.
    pub fn choose_workspace(&mut self, id: &str) {
        self.workspace = id.to_owned();
        if self.persist {
            let id = id.to_owned();
            let _ = std::thread::Builder::new().name("neko-prefs".into()).spawn(move || {
                let _ = rune::prefs::Prefs::open(crate::PREFS_ID).set(crate::WORKSPACE_KEY, &id);
            });
        }
    }

    /// Folds or opens a workspace's section, remembered for the next launch.
    pub fn toggle_fold(&mut self, id: &str) {
        if !self.folded.remove(id) {
            self.folded.insert(id.to_owned());
        }
        self.save_folded();
    }

    pub fn set_folded(&mut self, id: &str, folded: bool) {
        if folded != self.folded.contains(id) {
            self.toggle_fold(id);
        }
    }

    fn save_folded(&self) {
        if self.persist {
            let mut list: Vec<String> = self.folded.iter().cloned().collect();
            list.sort();
            let _ = std::thread::Builder::new().name("neko-prefs".into()).spawn(move || {
                let _ = rune::prefs::Prefs::open(crate::PREFS_ID).set(crate::FOLDED_KEY, &list);
            });
        }
    }

    /// New task in this workspace (a section's button): Home, the picker on it, the keyboard in the composer.
    pub fn new_task_in(&mut self, id: &str) {
        self.choose_workspace(id);
        self.go_home_and_focus();
    }

    /// ⌘N: a new task in the workspace of the task in view (when it can take one), else in the workspace chosen last.
    pub fn new_task_here(&mut self) {
        if let Some(w) = self.open_task().and_then(|t| self.workspace_of(&t.workspace)).filter(|w| w.is_active()).map(|w| w.id.clone()) {
            self.choose_workspace(&w);
        }
        self.go_home_and_focus();
    }

    /// Adds a folder as a workspace (from the folder panel).
    pub fn add_workspace(&mut self, folder: String, select: bool) {
        if let Some(id) = self.send(Request::AddWorkspace { folder }) {
            self.asked.insert(id, Asked::AddWorkspace { select });
        }
    }

    fn workspace_request(&mut self, r: Request) {
        if let Some(id) = self.send(r) {
            self.asked.insert(id, Asked::Workspace);
        }
    }

    pub fn start_rename(&mut self, id: &str) {
        self.menu_open = None;
        if let Some(w) = self.workspace_of(id) {
            let mut edit = TextEdit::new(&w.name);
            edit.focused = true;
            self.renaming = Some((id.to_owned(), edit));
        }
    }

    /// Save in the rename panel.
    pub fn finish_rename(&mut self) {
        if let Some((id, edit)) = self.renaming.take() {
            let name = edit.text.trim().to_owned();
            if !name.is_empty() && self.workspace_of(&id).is_some_and(|w| w.name != name) {
                self.workspace_request(Request::RenameWorkspace { workspace: id, name });
            }
        }
    }

    pub fn ask_remove_workspace(&mut self, id: &str) {
        self.menu_open = None;
        self.confirm = Some(Confirm::RemoveWorkspace(id.to_owned()));
    }

    pub fn ask_remove_task(&mut self, id: &TaskId) {
        self.menu_open = None;
        self.confirm = Some(Confirm::RemoveTask(id.clone()));
    }

    /// The answer to the open question: go ahead.
    pub fn confirmed(&mut self) {
        match self.confirm.take() {
            Some(Confirm::RemoveWorkspace(id)) => {
                if self.workspace == id {
                    self.choose_workspace(GENERAL);
                }
                self.workspace_request(Request::RemoveWorkspace { workspace: id });
            }
            Some(Confirm::RemoveTask(task)) => {
                if let Some(id) = self.send(Request::RemoveTask { task: task.clone() }) {
                    self.asked.insert(id, Asked::RemoveTask(task));
                }
            }
            Some(Confirm::UndoAnyway { task, run, redo, .. }) => self.send_undo(task, run, redo, true),
            Some(Confirm::RetireCat(id)) => {
                if self.open_cat.as_deref() == Some(id.as_str()) {
                    self.go_home();
                }
                self.cat_picks.retain(|_, c| *c != id);
                self.cat_request(Request::RetireCat { cat: id }, false);
            }
            Some(Confirm::ClearUndo) => {
                if let Some(id) = self.send(Request::ClearUndo) {
                    self.asked.insert(id, Asked::ClearUndo);
                }
            }
            Some(Confirm::RemoveRound(id)) => self.remove_round(&id),
            Some(Confirm::RemovePlugin(id)) => self.remove_plugin(&id),
            None => {}
        }
    }

    /// Review on a run's card (None: the task's "All changes"): its files, folded, in place of the conversation.
    pub fn open_review(&mut self, task: &TaskId, run: Option<RunId>) {
        let from = run.clone().or_else(|| self.review.as_ref().filter(|r| &r.task == task).and_then(|r| r.from.clone()));
        self.menu_open = None;
        self.review = Some(Review::new(task.clone(), run.clone(), from));
        if let Some(id) = self.send(Request::GetDiff { task: task.clone(), run }) {
            self.asked.insert(id, Asked::Diff);
        }
    }

    /// Asks once for the files of each finished run of the open task that changed something, so its receipt can list them with
    /// their counts. Without the daemon nothing is asked (and nothing is marked asked).
    pub fn ask_receipts(&mut self) {
        let Some(book) = self.book() else { return };
        let want: Vec<(TaskId, RunId)> = book
            .runs
            .iter()
            .filter(|r| r.status != RunStatus::Running && r.undo.as_ref().is_some_and(neko_protocol::RunUndo::changed))
            .filter(|r| !self.receipts_asked.contains(&r.id) && !self.receipts.contains_key(&r.id))
            .map(|r| (r.task.clone(), r.id.clone()))
            .collect();
        for (task, run) in want {
            if let Some(id) = self.send(Request::GetDiff { task, run: Some(run.clone()) }) {
                self.asked.insert(id, Asked::Receipt);
                self.receipts_asked.insert(run);
            }
        }
    }

    /// Back to the conversation.
    pub fn close_review(&mut self) {
        self.review = None;
    }

    /// Opens or folds a file of the review; its lines are asked for the first time it opens.
    pub fn review_flip(&mut self, path: &str) {
        let Some(r) = &mut self.review else { return };
        if r.open.remove(path) {
            return;
        }
        r.open.insert(path.to_owned());
        if !r.patches.contains_key(path) {
            let (task, run) = (r.task.clone(), r.run.clone());
            self.ask_lines(task, run, path, false);
        }
    }

    /// "Show all": every line of a file that was cut.
    pub fn review_all(&mut self, path: &str) {
        if let Some(r) = &self.review {
            let (task, run) = (r.task.clone(), r.run.clone());
            self.ask_lines(task, run, path, true);
        }
    }

    fn ask_lines(&mut self, task: TaskId, run: Option<RunId>, path: &str, full: bool) {
        if let Some(id) = self.send(Request::GetDiffFile { task, run, path: path.to_owned(), full }) {
            self.asked.insert(id, Asked::Diff);
        }
    }

    /// Undo (or Redo) on a run's card.
    pub fn undo(&mut self, run: &RunId, redo: bool) {
        let Some(task) = self.open.clone() else { return };
        self.send_undo(task, run.clone(), redo, false);
    }

    /// Undo (or Redo) of a run of any task: the Briefing's rows offer it without opening the task. Files changed since ask first, a
    /// refusal is said in a panel, exactly as on the run's own card.
    pub fn undo_run(&mut self, task: &TaskId, run: &RunId, redo: bool) {
        self.send_undo(task.clone(), run.clone(), redo, false);
    }

    fn send_undo(&mut self, task: TaskId, run: RunId, redo: bool, anyway: bool) {
        let r = if redo { Request::RedoRun { task: task.clone(), run: run.clone(), anyway } } else { Request::UndoRun { task: task.clone(), run: run.clone(), anyway } };
        if let Some(id) = self.send(r) {
            self.asked.insert(id, Asked::Undo { task, run, redo });
        }
    }

    /// Settings ▸ Workspaces: "Clear undo history…" asks first.
    pub fn ask_clear_undo(&mut self) {
        self.confirm = Some(Confirm::ClearUndo);
    }

    /// The room undo takes, asked again (Settings ▸ Workspaces shows it).
    pub fn ask_undo_storage(&mut self) {
        self.send(Request::UndoStorage);
    }

    /// Up (-1) or down (1) among the workspaces the person added: the keyboard's move and the menu's Move up / Move down.
    pub fn move_workspace(&mut self, id: &str, by: i32) {
        self.menu_open = None;
        let order: Vec<&str> = self.workspaces.iter().filter(|w| !w.is_general() && w.is_active()).map(|w| w.id.as_str()).collect();
        let Some(at) = order.iter().position(|w| *w == id) else { return };
        let to = at as i32 + by;
        if to < 0 || to >= order.len() as i32 {
            return;
        }
        self.workspace_request(Request::MoveWorkspace { workspace: id.to_owned(), to: to as u32 });
    }

    /// Whether the workspace can move up (-1) or down (1).
    pub fn can_move(&self, id: &str, by: i32) -> bool {
        let order: Vec<&str> = self.workspaces.iter().filter(|w| !w.is_general() && w.is_active()).map(|w| w.id.as_str()).collect();
        order.iter().position(|w| *w == id).is_some_and(|at| {
            let to = at as i32 + by;
            to >= 0 && to < order.len() as i32
        })
    }

    pub fn locate(&mut self, id: &str, folder: String) {
        self.workspace_request(Request::LocateWorkspace { workspace: id.to_owned(), folder });
    }

    /// Adds or replaces a workspace, keeping General first, then the person's order, then the removed ones.
    fn upsert_workspace(&mut self, w: Workspace) {
        self.workspaces.retain(|x| x.id != w.id);
        self.workspaces.push(w);
        self.workspaces.sort_by_key(|w| (!w.is_general(), w.removed_ms.is_some(), w.position, w.removed_ms.unwrap_or(0), w.created_ms));
    }

    /// The open task's transcript.
    pub fn book(&self) -> Option<&Book> {
        self.open.as_ref().and_then(|id| self.books.get(id))
    }

    /// The newest request still waiting for the person in the open task (Escape answers it no).
    pub fn pending_request(&self) -> Option<PermissionId> {
        let b = self.book()?;
        b.permissions.values().filter(|p| p.state == neko_protocol::PermissionState::Pending).max_by_key(|p| (p.asked_ms, p.id.0.clone())).map(|p| p.id.clone())
    }

    /// Whether the open task's agent is at work.
    pub fn open_is_working(&self) -> bool {
        self.open_task().is_some_and(|t| t.status.is_active()) || self.book().is_some_and(|b| b.running().is_some())
    }

    /// The open task's composer.
    pub fn reply_mut(&mut self) -> &mut TextEdit {
        let id = self.open.clone().unwrap_or(TaskId(String::new()));
        self.replies.entry(id).or_default()
    }

    pub fn reply_box(&self) -> Option<&TextEdit> {
        self.open.as_ref().and_then(|id| self.replies.get(id))
    }

    /// Opens or shuts a fold.
    pub fn flip(&mut self, key: &str) {
        if !self.open_folds.remove(key) {
            self.open_folds.insert(key.to_owned());
        }
    }

    /// Whether a message's actions show now.
    pub fn reveal(&self, key: &str) -> crate::ui::message::Reveal {
        self.reveals.get(key).copied().unwrap_or_default()
    }

    pub fn is_open(&self, key: &str) -> bool {
        self.open_folds.contains(key)
    }

    /// Asks for the open task's transcript and its streaming words.
    fn follow_open(&mut self) {
        let want = self.open.clone();
        if self.watching != want {
            self.send(Request::WatchTask { task: want.clone() });
            self.watching = want.clone();
        }
        if let Some(id) = want {
            self.send(Request::GetTranscript { task: id });
        }
    }

    /// Return in a task's composer: the words continue the conversation.
    pub fn follow_up(&mut self) {
        let Some(task) = self.open.clone() else { return };
        let words = self.reply_box().map(|r| r.text.clone()).unwrap_or_default();
        if words.trim().is_empty() || self.open_is_working() {
            return;
        }
        if let Some(id) = self.send(Request::FollowUp { task: task.clone(), words: words.clone(), answering: None }) {
            self.asked.insert(id, Asked::FollowUp(task.clone(), words));
            self.replies.insert(task, TextEdit::default());
            self.at_bottom = true;
        }
    }

    /// Stop (the composer's button, Escape): the open task's agent stops.
    pub fn stop(&mut self) {
        if let Some(task) = self.open.clone() {
            self.send(Request::StopTask { task });
        }
    }

    /// An answer to a permission request.
    pub fn decide(&mut self, permission: &PermissionId, decision: Decision) {
        self.send(Request::AnswerPermission { permission: permission.clone(), decision });
    }

    /// Settings (⌘,).
    pub fn open_settings(&mut self) {
        self.settings_open = true;
        self.needs_open = false;
        self.rounds_open = false;
        self.briefing_open = false;
        self.open_cat = None;
        self.send(Request::ListRuntimes);
        self.ask_undo_storage();
    }

    pub fn set_permission_mode(&mut self, mode: PermissionMode) {
        if let Some((_, s)) = &self.runtimes {
            let settings = Settings { permission_mode: mode, ..s.clone() };
            self.send(Request::SetSettings { settings });
        }
    }

    pub fn set_runtime(&mut self, id: &str) {
        if let Some((_, s)) = &self.runtimes {
            let settings = Settings { runtime: id.to_owned(), ..s.clone() };
            self.send(Request::SetSettings { settings });
        }
    }

    /// Whether the sidebar shows: the person's choice, else wide windows only.
    pub fn sidebar_shown(&self) -> bool {
        self.sidebar_choice.unwrap_or(self.width.is_none_or(|w| w >= FOLD_BELOW))
    }

    pub fn toggle_sidebar(&mut self) {
        self.sidebar_choice = Some(!self.sidebar_shown());
    }

    /// A new width: crossing [`FOLD_BELOW`] gives the sidebar back to the window's own rule.
    pub fn set_width(&mut self, w: f32) {
        let crossed = self.width.is_some_and(|old| (old >= FOLD_BELOW) != (w >= FOLD_BELOW));
        if crossed {
            self.sidebar_choice = None;
        }
        self.width = Some(w);
    }

    pub fn go_home(&mut self) {
        self.open = None;
        self.open_cat = None;
        self.review = None;
        self.context_open = false;
        self.settings_open = false;
        self.needs_open = false;
        self.rounds_open = false;
        self.briefing_open = false;
        self.follow_open();
    }

    /// New Task (⌘N, the sidebar's button): Home, with the keyboard in the composer.
    pub fn go_home_and_focus(&mut self) {
        self.go_home();
        self.draft.focused = true;
    }

    pub fn open_task_id(&mut self, id: &TaskId) {
        if self.open.as_ref() != Some(id) {
            self.review = None;
        }
        self.open = Some(id.clone());
        self.open_cat = None;
        self.context_open = false;
        self.settings_open = false;
        self.needs_open = false;
        self.rounds_open = false;
        self.briefing_open = false;
        self.notifier.opened(id);
        self.at_bottom = true;
        self.follow_open();
    }

    /// The link is set up (after the first frame, `lib.rs`): the app waits for the daemon from now.
    pub fn link_started(&mut self, conn: Conn) {
        self.conn = Some(conn);
        if self.daemon != Daemon::Connected {
            self.daemon = Daemon::Starting;
            self.waiting_since = Some(std::time::Instant::now());
        }
    }

    /// The daemon has not answered for longer than [`App::slow_after`]: a calm line says so, with Retry.
    pub fn daemon_slow(&self) -> bool {
        matches!(self.daemon, Daemon::Starting | Daemon::Lost(_)) && self.waiting_since.is_some_and(|at| at.elapsed() >= self.slow_after)
    }

    /// Retry: the link starts the daemon again now, and the wait starts over.
    pub fn retry_link(&mut self) {
        if let Some(c) = &self.conn {
            c.retry();
            self.waiting_since = Some(std::time::Instant::now());
        }
    }

    /// Sends a request; None when there is no daemon to send it to.
    pub fn send(&self, r: Request) -> Option<u64> {
        self.conn.as_ref().and_then(|c| c.send(r))
    }

    /// Return in the composer: the words go to the daemon. They leave the box at once and come back if the daemon refuses them.
    pub fn submit(&mut self) {
        let words = self.draft.text.clone();
        if words.trim().is_empty() || self.creating.is_some() {
            return;
        }
        let workspace = Some(self.workspace.clone()).filter(|w| w != GENERAL);
        let cat = self.cat_pick().map(str::to_owned);
        match self.send(Request::CreateTask { words: words.clone(), workspace, cat }) {
            Some(id) => {
                self.creating = Some((id, words));
                self.draft = TextEdit::default();
                self.notice = None;
                self.queued = false;
            }
            // The link is coming up: the words stay in the box and go as soon as the daemon answers.
            None if self.conn.is_some() => {
                self.queued = true;
                self.notice = Some("Neko is starting. Your task is sent as soon as it is ready.".to_owned());
            }
            None => self.notice = Some("Neko is not running. Your words are kept.".to_owned()),
        }
    }

    /// Adds or replaces a task, keeping newest first.
    fn upsert(&mut self, mut task: Task) {
        // One short line, whatever the store holds (a title kept from before titles were cut): the sidebar, the band and the
        // window's title all read this.
        task.title = neko_protocol::title::title(&task.title);
        self.tasks.retain(|t| t.id != task.id);
        let at = self.tasks.iter().position(|t| (t.created_ms, &t.id) < (task.created_ms, &task.id)).unwrap_or(self.tasks.len());
        self.tasks.insert(at, task);
    }

    /// One thing from the link.
    pub fn take(&mut self, i: Incoming) {
        match i {
            Incoming::Connected(_) => {
                self.daemon = Daemon::Connected;
                self.waiting_since = None;
                self.send(Request::Subscribe);
                self.send(Request::ListTasks);
                self.send(Request::ListWorkspaces);
                self.send(Request::ListRuntimes);
                self.send(Request::ListNeeds);
                self.send(Request::ListCats);
                self.send(Request::ListRounds);
                self.send(Request::ListPlugins);
                self.send(Request::GetBriefing { from_ms: None, to_ms: None });
                self.send(Request::GetBriefingSettings);
                // A new connection watches nothing yet.
                self.watching = None;
                self.follow_open();
                if std::mem::take(&mut self.queued) {
                    self.submit();
                }
                for c in std::mem::take(&mut self.pending_clicks) {
                    self.act_on(c);
                }
            }
            Incoming::Lost(why) => {
                self.plugins_lost();
                self.daemon = Daemon::Lost(why);
                self.waiting_since.get_or_insert_with(std::time::Instant::now);
                // An answer that will never come: the words go back in the box.
                if let Some((_, words)) = self.creating.take()
                    && self.draft.text.is_empty()
                {
                    self.draft = TextEdit::new(&words);
                }
            }
            Incoming::Message(ServerMessage::Event { change, .. }) => self.change(change),
            Incoming::Message(ServerMessage::Reply { id, reply }) => self.reply(id, reply),
        }
    }

    /// A change the daemon pushed.
    pub fn change(&mut self, change: Change) {
        match change {
            Change::TaskCreated { task } => self.upsert(task),
            Change::TaskUpdated { task } => {
                let before = self.tasks.iter().find(|t| t.id == task.id).map(|t| t.status);
                if before != Some(task.status) {
                    self.say_status(&task);
                }
                // A run finished with nothing asked: "Done" (a question or a failure to act on is an item of the list instead).
                if task.status == neko_protocol::TaskStatus::Done && before.is_some_and(neko_protocol::TaskStatus::is_active) {
                    let words = self.last_words(&task.id);
                    let here = self.here();
                    let title = neko_protocol::title::title(&task.title);
                    let cat = self.cat_name(&task);
                    self.notifier.finished(&task.id, &title, &cat, words.as_deref(), &here);
                }
                self.upsert(task);
            }
            Change::TaskRemoved { task } => {
                self.tasks.retain(|t| t.id != task);
                self.books.remove(&task);
                self.replies.remove(&task);
                if self.open.as_ref() == Some(&task) {
                    self.go_home();
                }
                if self.confirm == Some(Confirm::RemoveTask(task)) {
                    self.confirm = None;
                }
            }
            Change::WorkspaceAdded { workspace } | Change::WorkspaceUpdated { workspace } => {
                if !workspace.is_active() && self.workspace == workspace.id {
                    self.workspace = GENERAL.to_owned();
                }
                self.upsert_workspace(workspace);
            }
            Change::RunChanged { run } => {
                // "All changes" ends where the newest run left the folder: an undo or a new run moves it.
                let again = self.review.as_ref().is_some_and(|r| r.task == run.task && r.run.is_none() && run.status != RunStatus::Running);
                self.books.entry(run.task.clone()).or_default().run(run);
                if again && let Some(r) = self.review.clone() {
                    self.open_review(&r.task, None);
                }
            }
            Change::PartChanged { part } => self.books.entry(part.task.clone()).or_default().part(part),
            Change::PartDelta { task, part, text } => {
                if let Some(b) = self.books.get_mut(&task) {
                    b.delta(&part, &text);
                }
            }
            Change::PermissionChanged { permission } => {
                self.books.entry(permission.task.clone()).or_default().permissions.insert(permission.id.clone(), permission);
            }
            Change::Activity { task, thinking } => {
                if let Some(b) = self.books.get_mut(&task) {
                    b.thinking = thinking;
                }
            }
            Change::SettingsChanged { settings } => {
                if let Some((_, s)) = &mut self.runtimes {
                    *s = settings;
                }
            }
            Change::NeedsChanged { items } => self.set_needs(items),
            Change::CatAdded { cat } | Change::CatUpdated { cat } => self.upsert_cat(cat),
            Change::RoundAdded { round } | Change::RoundUpdated { round } => {
                let id = round.id.clone();
                self.upsert_round(round);
                self.refresh_round_history(&id);
            }
            Change::RoundRemoved { round } => {
                self.rounds.retain(|r| r.id != round);
                self.round_history.remove(&round);
                self.round_history_open.remove(&round);
                if self.confirm == Some(Confirm::RemoveRound(round)) {
                    self.confirm = None;
                }
            }
            Change::RoundsPaused { paused } => self.rounds_paused = paused,
            Change::PluginAdded { plugin } | Change::PluginUpdated { plugin } => self.upsert_plugin(plugin),
            Change::PluginRemoved { plugin } => self.drop_plugin(&plugin),
            Change::BriefingChanged { briefing } => self.set_briefing(briefing),
            Change::BriefingSettingsChanged { settings } => self.set_briefing_settings(settings),
            Change::BriefingDue { counts } => self.briefing_due(&counts),
        }
    }

    /// Tells assistive technology what a task's new state means, when it matters.
    fn say_status(&mut self, task: &Task) {
        use neko_protocol::TaskStatus as S;
        let cat = self.cat_name(task);
        let words = match task.status {
            S::NeedsYou => Some((format!("{cat} needs you: {}", task.title), true)),
            S::Asked => Some((format!("{cat} has a question for you: {}", task.title), true)),
            S::Done => Some((format!("{cat} finished: {}", task.title), false)),
            S::Failed => Some((format!("{cat} could not finish: {}", task.title), false)),
            S::Stopped => Some((format!("Stopped: {}", task.title), false)),
            _ => None,
        };
        if let Some(w) = words {
            self.announce = Some(w);
        }
    }

    fn reply(&mut self, id: u64, reply: Reply) {
        let mine = self.creating.as_ref().is_some_and(|(c, _)| *c == id);
        let asked = self.asked.remove(&id);
        let Some(reply) = self.plugin_reply(&asked, reply) else { return };
        match reply {
            Reply::Transcript { transcript } => {
                let id = transcript.task.id.clone();
                self.upsert(transcript.task.clone());
                // Words that streamed in between are in the stored part or arrive as deltas after it: the whole replaces.
                self.books.insert(id, Book::from(transcript));
                return;
            }
            Reply::Runtimes { runtimes, settings } => {
                self.runtimes = Some((runtimes, settings));
                return;
            }
            Reply::Settings { settings } => {
                if let Some((_, s)) = &mut self.runtimes {
                    *s = settings;
                }
                return;
            }
            Reply::DayUsage { .. } => return,
            Reply::Rounds { rounds, paused } => {
                self.rounds_loaded = true;
                self.rounds_paused = paused;
                self.rounds.clear();
                for r in rounds {
                    self.upsert_round(r);
                }
                return;
            }
            Reply::Round { round } => {
                self.upsert_round(round);
                if matches!(asked, Some(Asked::RoundForm)) {
                    self.round_form = None;
                }
                return;
            }
            Reply::RoundRuns { runs } => {
                if let Some(Asked::RoundHistory(id)) = asked {
                    self.round_history.insert(id, runs);
                }
                return;
            }
            Reply::Error { message, .. } if matches!(asked, Some(Asked::RoundForm)) => {
                if let Some(f) = &mut self.round_form {
                    f.error = Some(message);
                }
                return;
            }
            Reply::Error { message, .. } if matches!(asked, Some(Asked::Round | Asked::RoundRun)) => {
                let title = if matches!(asked, Some(Asked::RoundRun)) { "Neko did not run this round" } else { "Neko could not do that" };
                self.refusal = Some(Refusal { title: title.into(), words: message, reveal: None });
                return;
            }
            Reply::Needs { items } => {
                self.set_needs(items);
                return;
            }
            Reply::Briefing { briefing } => {
                let read = matches!(asked, Some(Asked::BriefingRead));
                self.set_briefing(briefing);
                if read {
                    self.caught_up();
                }
                return;
            }
            Reply::BriefingSettings { settings } => {
                self.set_briefing_settings(settings);
                return;
            }
            Reply::Error { message, .. } if matches!(asked, Some(Asked::BriefingRead | Asked::BriefingSettings)) => {
                self.refusal = Some(Refusal { title: "Neko could not do that".into(), words: message, reveal: None });
                return;
            }
            Reply::Error { code, message } if matches!(asked, Some(Asked::Answer { .. })) => {
                let Some(Asked::Answer { item, task, from_notification }) = asked else { return };
                if code == ErrorCode::AlreadyAnswered {
                    if from_notification && !self.front {
                        let title = self.tasks.iter().find(|t| t.id == task).map_or_else(|| "Neko".to_owned(), |t| t.title.clone());
                        self.notifier.stale(&task, &title);
                    } else {
                        self.needs_notice = Some("That was already answered.".to_owned());
                        self.announce = Some(("That was already answered.".to_owned(), false));
                    }
                } else {
                    // Not answered: the item comes back, with why.
                    self.needs_hidden.remove(&item);
                    self.needs_notice = Some(message);
                }
                return;
            }
            Reply::Diff { diff } if matches!(asked, Some(Asked::Receipt)) => {
                if let Some(run) = diff.run.clone() {
                    self.receipts.insert(run, diff);
                }
                return;
            }
            Reply::Diff { diff } => {
                if let Some(r) = &mut self.review
                    && r.task == diff.task
                    && r.run == diff.run
                {
                    r.error = None;
                    // One file opens by itself; more stay folded until the person opens them.
                    let only = (diff.files.len() == 1).then(|| diff.files[0].path.clone());
                    r.summary = Some(diff);
                    if let Some(p) = only {
                        self.review_flip(&p);
                    }
                }
                return;
            }
            Reply::DiffFile { file } => {
                if let Some(r) = &mut self.review {
                    r.patches.insert(file.path.clone(), file);
                }
                return;
            }
            Reply::Error { message, .. } if matches!(asked, Some(Asked::Diff)) => {
                if let Some(r) = &mut self.review {
                    r.error = Some(message);
                }
                return;
            }
            Reply::ChangedSince { files } => {
                if let Some(Asked::Undo { task, run, redo }) = asked {
                    self.confirm = Some(Confirm::UndoAnyway { task, run, redo, files });
                }
                return;
            }
            Reply::Ok if matches!(asked, Some(Asked::Undo { .. })) => {
                if let Some(Asked::Undo { redo, .. }) = asked {
                    self.announce = Some((if redo { "Redone: the run's changes are back." } else { "Undone: the files are as they were before the run." }.to_owned(), false));
                }
                return;
            }
            Reply::Error { message, .. } if matches!(asked, Some(Asked::Undo { .. })) => {
                let redo = matches!(asked, Some(Asked::Undo { redo: true, .. }));
                self.refusal = Some(Refusal { title: if redo { "Neko did not redo this run" } else { "Neko did not undo this run" }.into(), words: message, reveal: None });
                return;
            }
            Reply::UndoStorage { storage } => {
                self.undo_storage = Some(storage);
                return;
            }
            Reply::Cats { cats } => {
                self.cats.clear();
                for c in cats {
                    self.upsert_cat(c);
                }
                return;
            }
            Reply::Cat { cat } => {
                let id = cat.id.clone();
                self.upsert_cat(cat);
                if matches!(asked, Some(Asked::Cat { open: true })) {
                    self.open_cat_page(&id);
                }
                return;
            }
            Reply::Error { message, .. } if matches!(asked, Some(Asked::Cat { .. })) => {
                self.refusal = Some(Refusal { title: "Neko could not do that".into(), words: message, reveal: None });
                return;
            }
            Reply::Ok if matches!(asked, Some(Asked::ClearUndo)) => {
                self.ask_undo_storage();
                self.announce = Some(("The undo history was cleared.".to_owned(), false));
                return;
            }
            Reply::Error { message, .. } if matches!(asked, Some(Asked::ClearUndo)) => {
                self.refusal = Some(Refusal { title: "Neko kept the undo history".into(), words: message, reveal: None });
                return;
            }
            Reply::Workspaces { workspaces } => {
                self.workspaces.clear();
                for w in workspaces {
                    self.upsert_workspace(w);
                }
                // The choice remembered from last time holds while its workspace takes new tasks.
                if !self.chosen().is_some_and(Workspace::is_active) {
                    self.workspace = GENERAL.to_owned();
                }
                return;
            }
            Reply::Workspace { workspace, .. } => {
                let id = workspace.id.clone();
                self.upsert_workspace(workspace);
                if matches!(asked, Some(Asked::AddWorkspace { select: true })) {
                    self.choose_workspace(&id);
                }
                self.folded.remove(&id);
                return;
            }
            Reply::Error { code, message } if matches!(asked, Some(Asked::RemoveTask(_))) => {
                let Some(Asked::RemoveTask(task)) = asked else { return };
                let reveal = (code == ErrorCode::UnsavedWork).then(|| self.place_of(&task)).flatten();
                self.refusal = Some(Refusal { title: "Neko kept this task".into(), words: message, reveal });
                return;
            }
            Reply::Error { message, .. } if matches!(asked, Some(Asked::Workspace | Asked::AddWorkspace { .. })) => {
                self.refusal = Some(Refusal { title: "Neko could not do that".into(), words: message, reveal: None });
                return;
            }
            Reply::Error { message, .. } if matches!(asked, Some(Asked::FollowUp(..))) => {
                if let Some(Asked::FollowUp(task, words)) = asked {
                    let r = self.replies.entry(task).or_default();
                    if r.text.is_empty() {
                        *r = TextEdit::new(&words);
                    }
                }
                self.notice = Some(message);
                return;
            }
            _ => {}
        }
        match reply {
            Reply::Tasks { tasks } => {
                self.tasks_loaded = true;
                // A list answers from the store, so it replaces what the window had (a task made meanwhile comes as an event).
                for t in tasks {
                    self.upsert(t);
                }
                if let Some(t) = self.pending_link.take() {
                    self.go_to_task(t);
                }
            }
            Reply::Task { task } => {
                if mine {
                    self.creating = None;
                    let id = task.id.clone();
                    self.upsert(task);
                    self.open_task_id(&id);
                    // The keyboard follows the words: into the new task's composer (Home's has gone).
                    self.replies.entry(id).or_default().focused = true;
                    return;
                }
                self.upsert(task);
            }
            Reply::Error { message, .. } if mine => {
                if let Some((_, words)) = self.creating.take()
                    && self.draft.text.is_empty()
                {
                    self.draft = TextEdit::new(&words);
                }
                self.notice = Some(message);
            }
            _ => {}
        }
    }

    /// Where a task works: its newest run's folder, else the folder it was made in.
    pub fn place_of(&self, task: &TaskId) -> Option<String> {
        let run = self.books.get(task).and_then(|b| b.runs.iter().rev().find(|r| !r.workdir.is_empty()).map(|r| r.workdir.clone()));
        run.or_else(|| self.tasks.iter().find(|t| &t.id == task).and_then(|t| t.folder.clone()))
    }

    /// Everything waiting in the link's inbox. True when something changed.
    pub fn drain_link(&mut self) -> bool {
        let got = self.conn.as_ref().map(Conn::drain).unwrap_or_default();
        let any = !got.is_empty();
        for i in got {
            self.take(i);
        }
        any
    }

    /// "Good morning", "Good afternoon" or "Good evening", by the local hour.
    pub fn greeting(&self) -> &'static str {
        match crate::clock::local_hour(self.now_ms, self.utc_offset_s) {
            5..=11 => "Good morning",
            12..=17 => "Good afternoon",
            _ => "Good evening",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::TaskStatus;

    fn task(id: &str, ms: i64) -> Task {
        Task { id: TaskId(id.into()), workspace: "default".into(), words: format!("words of {id}"), title: id.into(), created_ms: ms, status: TaskStatus::New, folder: None, cat: "cat_miso".into(), round: None }
    }

    #[test]
    fn tasks_stay_newest_first_and_are_never_doubled() {
        let mut a = App::default();
        a.take(Incoming::Message(ServerMessage::Reply { id: 1, reply: Reply::Tasks { tasks: vec![task("b", 2), task("a", 1)] } }));
        a.take(Incoming::Message(ServerMessage::Event { revision: 3, change: Change::TaskCreated { task: task("c", 3) } }));
        a.take(Incoming::Message(ServerMessage::Event { revision: 3, change: Change::TaskCreated { task: task("c", 3) } }));
        assert_eq!(a.tasks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["c", "b", "a"]);
    }

    #[test]
    fn the_answer_to_my_words_opens_the_task_and_a_refusal_puts_them_back() {
        let mut a = App { creating: Some((7, "Fix the build".into())), ..App::default() };
        a.take(Incoming::Message(ServerMessage::Reply { id: 7, reply: Reply::Task { task: task("t", 5) } }));
        assert_eq!(a.open, Some(TaskId("t".into())));
        assert_eq!(a.window_title(), "t");
        let mut b = App { creating: Some((8, "Too long".into())), ..App::default() };
        b.take(Incoming::Message(ServerMessage::Reply { id: 8, reply: Reply::error(neko_protocol::ErrorCode::InvalidWords, "No.") }));
        assert_eq!(b.draft.text, "Too long");
        assert_eq!(b.notice.as_deref(), Some("No."));
        let mut c = App { creating: Some((9, "Kept".into())), ..App::default() };
        c.take(Incoming::Lost("gone".into()));
        assert_eq!(c.draft.text, "Kept", "a lost connection gives the words back");
    }

    #[test]
    fn a_task_title_is_one_short_line_everywhere() {
        let mut long = task("t", 5);
        long.title = "Look at the failing nightly build, find the cause, fix it, and then write up what happened\nLogs:\nerror".into();
        let mut a = App::default();
        a.take(Incoming::Message(ServerMessage::Reply { id: 1, reply: Reply::Tasks { tasks: vec![long] } }));
        a.open_task_id(&TaskId("t".into()));
        let title = a.window_title();
        assert!(!title.contains('\n') && title.chars().count() <= neko_protocol::title::MAX + 1, "{title}");
        assert_eq!(a.tasks[0].title, title, "the sidebar and the window agree");
    }

    #[test]
    fn without_a_daemon_the_words_stay_in_the_box() {
        let mut a = App { draft: TextEdit::new("Hello"), ..App::default() };
        a.submit();
        assert_eq!(a.draft.text, "Hello");
        assert!(a.notice.is_some());
        assert!(!a.queued, "no link at all: nothing to send it to later");
    }

    #[test]
    fn the_sidebar_folds_under_1000_and_the_choice_lasts_until_the_edge_is_crossed() {
        let mut a = App::default();
        assert!(a.sidebar_shown(), "before the first layout: shown");
        a.set_width(1200.0);
        assert!(a.sidebar_shown());
        a.set_width(900.0);
        assert!(!a.sidebar_shown(), "narrow: folded");
        a.toggle_sidebar();
        assert!(a.sidebar_shown(), "asked for");
        a.set_width(880.0);
        assert!(a.sidebar_shown(), "still narrow: the choice holds");
        a.set_width(1100.0);
        a.set_width(900.0);
        assert!(!a.sidebar_shown(), "crossing the edge gives it back to the rule");
    }

    #[test]
    fn the_greeting_follows_the_local_hour() {
        // 2026-11-26 10:00 UTC, in a zone 5 hours behind: 05:00 local.
        let a = App { now_ms: 1_795_687_200_000, utc_offset_s: -5 * 3600, ..App::default() };
        assert_eq!(a.greeting(), "Good morning");
    }

    fn ws(id: &str, name: &str, position: i64) -> Workspace {
        Workspace { id: id.into(), name: name.into(), folder: (id != GENERAL).then(|| format!("/Users/example/{name}")), created_ms: position, removed_ms: None, position, missing: false }
    }

    fn with_workspaces() -> App {
        let mut a = App::default();
        a.take(Incoming::Message(ServerMessage::Reply { id: 1, reply: Reply::Workspaces { workspaces: vec![ws("ws_b", "acme-web", 2), ws(GENERAL, "General", 0), ws("ws_a", "acme-api", 1)] } }));
        a
    }

    #[test]
    fn workspaces_keep_general_first_and_the_persons_order() {
        let mut a = with_workspaces();
        assert_eq!(a.workspaces.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(), ["General", "acme-api", "acme-web"]);
        let mut gone = ws("ws_a", "acme-api", 1);
        gone.removed_ms = Some(9);
        a.change(Change::WorkspaceUpdated { workspace: gone });
        assert_eq!(a.workspaces.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(), ["General", "acme-web", "acme-api"], "removed ones go last");
        assert!(a.sections().iter().all(|w| w.id != "ws_a"), "a removed workspace with no tasks has no section");
        let mut t = task("t", 1);
        t.workspace = "ws_a".into();
        a.change(Change::TaskCreated { task: t });
        assert!(a.sections().iter().any(|w| w.id == "ws_a"), "its tasks stay readable under it");
        assert_eq!(a.picker_options().iter().map(|o| o.0.as_str()).collect::<Vec<_>>(), ["General", "acme-web", ADD_FOLDER], "no new task in a removed workspace");
    }

    #[test]
    fn the_picker_chooses_where_the_next_task_goes_and_command_n_follows_the_task_in_view() {
        let mut a = with_workspaces();
        assert_eq!(a.workspace, GENERAL);
        a.pick("acme-web");
        assert_eq!(a.workspace, "ws_b");
        a.pick(ADD_FOLDER);
        assert_eq!(a.effects, [Effect::PickFolder(Pick::Add { select: true })], "Add folder… opens the folder panel");
        let mut t = task("t", 1);
        t.workspace = "ws_a".into();
        t.title = "Fix the build".into();
        a.change(Change::TaskCreated { task: t });
        a.open_task_id(&TaskId("t".into()));
        assert_eq!(a.window_title(), "Fix the build — acme-api");
        a.new_task_here();
        assert_eq!((a.workspace.as_str(), a.open.as_ref()), ("ws_a", None), "⌘N: a new task in the workspace of the task in view");
        // Two workspaces with one name are told apart by their folders.
        a.change(Change::WorkspaceAdded { workspace: Workspace { folder: Some("/Users/example/old/acme-web".into()), ..ws("ws_c", "acme-web", 3) } });
        let labels: Vec<String> = a.picker_options().into_iter().map(|o| o.0).collect();
        assert!(labels.contains(&"acme-web (/Users/example/acme-web)".to_owned()) && labels.contains(&"acme-web (/Users/example/old/acme-web)".to_owned()), "{labels:?}");
    }

    #[test]
    fn a_removed_task_leaves_the_window_and_a_refusal_says_why() {
        let mut a = with_workspaces();
        a.change(Change::TaskCreated { task: task("t", 1) });
        a.open_task_id(&TaskId("t".into()));
        a.change(Change::TaskRemoved { task: TaskId("t".into()) });
        assert!(a.tasks.is_empty() && a.open.is_none());
        a.asked.insert(4, Asked::RemoveTask(TaskId("u".into())));
        a.take(Incoming::Message(ServerMessage::Reply { id: 4, reply: Reply::error(ErrorCode::UnsavedWork, "This task's worktree has changes that are not committed.") }));
        assert!(a.refusal.as_ref().is_some_and(|r| r.words.contains("not committed")));
    }

}
