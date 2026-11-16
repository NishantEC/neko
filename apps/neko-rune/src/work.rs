//! What the session view and the Neko column keep between frames, and what pressing their
//! controls does. No drawing and no I/O here: every request goes out through [`App::call`], so
//! each rule is tested by looking at what was asked of the daemon.
//!
//! The rules are the web UI's, ported: `chat/TaskConversation.tsx` (the composer and its modes),
//! `work/queue.tsx` (the durable queue), `work/goals.tsx` (a goal's actions), `work/Runs.tsx`
//! (runs, diffs, undo and its refusals), `pages/Inbox.tsx` (the default answer) and
//! `work/digest.tsx`.

use std::collections::{HashMap, HashSet};

use neko_protocol::checkpoints::CheckpointList;
use neko_protocol::crew::DeliveryMode;
use neko_protocol::digest::Digest;
use neko_protocol::goals::{Goal, GoalStatus};
use neko_protocol::workbench::{Task, TaskStatus};
use rune::TextEdit;
use rune_kit::diff::DiffState;

use crate::app::{App, Steer};
use crate::link::{Call, InboxItem, Reply};

/// Which run's diff is open under the runs list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RunKey {
    Run(u32),
    /// First start to last end.
    Whole,
}

impl RunKey {
    pub fn run(self) -> Option<u32> {
        match self {
            RunKey::Run(n) => Some(n),
            RunKey::Whole => None,
        }
    }
}

/// How an undo was asked, and the options a refusal can offer instead.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UndoOptions {
    pub run: u32,
    pub through_later: bool,
    pub snapshot_unsaved: bool,
    pub revert: bool,
}

/// Why the daemon would not undo, in its words, and what it offers (`checkpoint:<code>: <message>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub code: String,
    pub message: String,
    pub options: UndoOptions,
}

/// Parses the daemon's refusal text; None when it is some other error.
pub fn parse_refusal(text: &str, options: UndoOptions) -> Option<Refusal> {
    let rest = text.strip_prefix(neko_protocol::checkpoints::REFUSAL_PREFIX)?;
    let (code, message) = rest.split_once(": ")?;
    (!code.is_empty() && code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')).then(|| Refusal { code: code.to_owned(), message: message.to_owned(), options })
}

/// A question asked before a destructive press (the web's confirm dialogs), shown inline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirm {
    pub title: String,
    pub detail: String,
    pub ok: String,
    pub cancel: String,
    pub call: Call,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffLoad {
    Loading,
    Failed(String),
    /// Loaded: the files are in `Work::diffs`.
    Ready { truncated: bool },
}

/// The key a run's diff is kept under.
pub fn diff_key(task_id: &str, run: Option<u32>) -> String {
    match run {
        Some(n) => format!("{task_id}|{n}"),
        None => format!("{task_id}|whole"),
    }
}

/// Everything the open session keeps between frames.
pub struct Work {
    /// Expanded cards, by key (`tool:<id>`, `goal:<id>`, `digest`).
    pub expanded: HashSet<String>,
    pub checkpoints: HashMap<String, CheckpointList>,
    /// The revision of each task whose checkpoints were last asked for.
    pub requested: HashMap<String, String>,
    pub open_run: Option<RunKey>,
    pub diffs: HashMap<String, DiffState>,
    pub diff_load: HashMap<String, DiffLoad>,
    /// A diff for a tool card that edited a file, by tool call id.
    pub edit_diffs: HashMap<String, DiffState>,
    pub confirm: Option<Confirm>,
    pub refusal: Option<Refusal>,
    pub undoing: bool,
    /// One command id per (task, mode, text), kept until it succeeds, so a retry is one message.
    pub attempt: Option<(String, String)>,
    /// One calm line near the composer.
    pub notice: Option<String>,
    pub drafts: HashMap<String, String>,
    /// What a diff view reads while its state is not loaded.
    pub empty_diff: DiffState,
    pub goal_answer: TextEdit,
    pub counter: u64,
    /// Earlier turns than the last [`SHOWN_TURNS`] are shown.
    pub all_turns: bool,
    /// The task whose detail was last asked for by id.
    pub detail_asked: Option<String>,
    /// Writes in flight that must not be sent twice (`new`, `reply:<task>`, `ask`): each is
    /// released by the daemon's answer, as the web's `sending` is.
    pub busy: HashSet<String>,
}

/// Turns drawn before "Show earlier": the session view bounds what it lays out.
pub const SHOWN_TURNS: usize = 60;

impl Default for Work {
    fn default() -> Self {
        Work {
            expanded: HashSet::new(),
            checkpoints: HashMap::new(),
            requested: HashMap::new(),
            open_run: None,
            diffs: HashMap::new(),
            diff_load: HashMap::new(),
            edit_diffs: HashMap::new(),
            confirm: None,
            refusal: None,
            undoing: false,
            attempt: None,
            notice: None,
            drafts: HashMap::new(),
            empty_diff: DiffState::new(Vec::new()),
            goal_answer: TextEdit::new(""),
            counter: 0,
            all_turns: false,
            detail_asked: None,
            busy: HashSet::new(),
        }
    }
}

/// What the Neko column keeps.
pub struct NekoCol {
    pub inbox: Vec<InboxItem>,
    pub digest: Option<Digest>,
    /// When the Inbox and digest were last asked for, by the app's clock.
    pub asked_ms: i64,
    pub loaded: bool,
    /// What the last Inbox answer did, in the daemon's words.
    pub note: Option<String>,
    /// A refusal or failure shown next to what it belongs to: `inbox:<seq>` or `goal:<id>`.
    pub notes: HashMap<String, String>,
}

impl Default for NekoCol {
    fn default() -> Self {
        NekoCol { inbox: Vec::new(), digest: None, asked_ms: i64::MIN / 2, loaded: false, note: None, notes: HashMap::new() }
    }
}

/// How often the Inbox and the digest are read again (the web polls every 60 s).
pub const NEKO_POLL_MS: i64 = 60_000;

/// Where a refusal of this call is shown: next to the Inbox item or goal it belongs to.
pub fn note_key(call: &Call) -> Option<String> {
    match call {
        Call::AnswerNeedsYou { seq } => Some(format!("inbox:{seq}")),
        Call::GoalPause { id } | Call::GoalResume { id } | Call::GoalExtend { id, .. } | Call::GoalDone { id } | Call::GoalStop { id } | Call::GoalAnswer { id, .. } => Some(format!("goal:{id}")),
        _ => None,
    }
}

/// What an Inbox item can be answered with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemAct {
    Call(Call),
    Link(String),
}

/// One entry of an Inbox item's right-click menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemAction {
    pub label: String,
    pub act: ItemAct,
}

pub fn delivery(steer: Steer) -> DeliveryMode {
    match steer {
        Steer::Now => DeliveryMode::SteerNow,
        Steer::AfterTurn => DeliveryMode::AfterTurn,
        Steer::Interrupt => DeliveryMode::InterruptRestart,
    }
}

/// Whether the task is in a turn right now, so a message needs a delivery mode (`isTurnRunning`).
pub fn in_turn(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing)
}

/// A planner's question that holds an approval back (`waitingReason`): Some while the task is
/// awaiting approval and its own reason says it needs the person's words, not a click.
pub fn waiting_reason(task: &Task) -> Option<String> {
    if task.status != TaskStatus::AwaitingApproval {
        return None;
    }
    use neko_protocol::workbench::SupervisorAction;
    if let Some(d) = &task.supervision {
        match d.action {
            SupervisorAction::AskUser => return Some(format!("Needs your input: {}", d.reason)),
            SupervisorAction::Skip => return Some(format!("Nothing to build: {}", d.reason)),
            SupervisorAction::PrepareFix => {}
        }
    }
    let event = task.events.iter().rev().find(|e| e.role == "supervisor" || e.role == "coordinator")?;
    if let Some(rest) = event.message.strip_prefix("Needs your input before building: ") {
        return Some(format!("Needs your input: {rest}"));
    }
    event.message.strip_prefix("Nothing to build: ").map(|rest| format!("Nothing to build: {rest}"))
}

/// The newest daemon message on a stopped task, in one readable line (`stopReason`).
pub fn stop_reason(task: &Task) -> String {
    let line = task.events.iter().rev().find(|e| !["user", "note", "host", "neko"].contains(&e.role.as_str())).map(|e| e.message.trim().to_owned()).unwrap_or_default();
    if line.contains("not a git repository") {
        "This task isn’t linked to a Git repository yet, so Neko couldn’t make its working copy.".to_owned()
    } else if line.contains("daemon restart") {
        "Stopped when Neko restarted. Start it again to continue.".to_owned()
    } else if line.starts_with("Run tool scope is paused, unavailable, or changed") {
        "A tool connection it relied on was paused or changed while it worked. Start it again to retry.".to_owned()
    } else if let Some(rest) = line.strip_prefix("Independent verification failed:") {
        format!("Review found problems: {}", rest.trim().chars().take(380).collect::<String>())
    } else if line.starts_with("Verifier returned a malformed verdict") {
        "The reviewer’s answer couldn’t be read. Start it again to re-review.".to_owned()
    } else if line.is_empty() {
        "Neko stopped without a reason. Start it again to retry.".to_owned()
    } else {
        line.chars().take(400).collect()
    }
}

/// The line under the conversation while an agent works (`workingLine`).
pub fn working_line(status: TaskStatus) -> Option<&'static str> {
    match status {
        TaskStatus::Queued => Some("Getting this task ready…"),
        TaskStatus::Planning => Some("Planner is reading the code…"),
        TaskStatus::Building => Some("Builder is making the change in its own copy…"),
        TaskStatus::Reviewing => Some("Reviewer is checking the change…"),
        _ => None,
    }
}

/// A goal's status as the one glyph set (`goalKind`).
pub fn goal_kind(status: GoalStatus) -> crate::status::StatusKind {
    use crate::status::StatusKind as K;
    match status {
        GoalStatus::Running => K::InProgress,
        GoalStatus::Blocked => K::Review,
        GoalStatus::Done => K::Done,
        GoalStatus::Stopped => K::Cancelled,
        GoalStatus::Waiting | GoalStatus::Held => K::Backlog,
        GoalStatus::Paused => K::Todo,
    }
}

pub fn goal_word(status: GoalStatus) -> &'static str {
    match status {
        GoalStatus::Running => "Working",
        GoalStatus::Waiting => "Waiting",
        GoalStatus::Blocked => "Needs you",
        GoalStatus::Held => "Held",
        GoalStatus::Paused => "Paused",
        GoalStatus::Done => "Done",
        GoalStatus::Stopped => "Stopped",
    }
}

/// "check passed", "check failing (exit 101)", or what a goal without a result yet says.
pub fn check_text(goal: &Goal) -> String {
    match &goal.last_check {
        None if goal.done_check.is_empty() => "no check".to_owned(),
        None => "check not run yet".to_owned(),
        Some(c) if c.exit_code == Some(0) => "check passed".to_owned(),
        Some(c) => format!("check failing ({})", c.exit_code.map_or("no exit".to_owned(), |c| format!("exit {c}"))),
    }
}

/// Where the goal stands, in the words a person would use (`stateText`).
pub fn goal_state(goal: &Goal) -> String {
    match goal.status {
        GoalStatus::Waiting => match &goal.wait {
            Some(w) if w.until_ms.is_some() => "waiting for a set time".to_owned(),
            Some(w) if !w.reason.is_empty() => format!("waiting: {}", w.reason),
            _ => "waiting: for something outside".to_owned(),
        },
        GoalStatus::Held => "usage limit".to_owned(),
        GoalStatus::Blocked => "needs your answer".to_owned(),
        GoalStatus::Paused => if goal.paused_reason.is_empty() { "paused".to_owned() } else { goal.paused_reason.clone() },
        GoalStatus::Stopped => if goal.paused_reason.is_empty() { "stopped".to_owned() } else { goal.paused_reason.clone() },
        _ => check_text(goal),
    }
}

pub fn turn_text(goal: &Goal) -> String {
    format!("turn {}/{}", goal.turns, goal.budget.max_turns)
}

pub fn extend_by(goal: &Goal) -> u32 {
    (goal.budget.max_turns / 2).max(5)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoalAct {
    Pause,
    Resume,
    Extend,
    Done,
    Stop,
}

/// The actions a goal offers in its state (`goalActions`): Pause or Resume, Extend, Mark done, Stop.
pub fn goal_actions(goal: &Goal) -> Vec<GoalAct> {
    let mut out = Vec::new();
    if goal.status == GoalStatus::Paused {
        out.push(GoalAct::Resume);
    } else if !goal.status.is_finished() {
        out.push(GoalAct::Pause);
    }
    if !goal.status.is_finished() {
        out.extend([GoalAct::Extend, GoalAct::Done, GoalAct::Stop]);
    }
    out
}

impl GoalAct {
    pub fn label(self, goal: &Goal) -> String {
        match self {
            GoalAct::Pause => "Pause".into(),
            GoalAct::Resume => "Resume".into(),
            GoalAct::Extend => format!("Extend by {} turns", extend_by(goal)),
            GoalAct::Done => "Mark done".into(),
            GoalAct::Stop => "Stop".into(),
        }
    }

    pub fn call(self, goal: &Goal) -> Call {
        let id = goal.id.clone();
        match self {
            GoalAct::Pause => Call::GoalPause { id },
            GoalAct::Resume => Call::GoalResume { id },
            GoalAct::Extend => Call::GoalExtend { id, turns: extend_by(goal) },
            GoalAct::Done => Call::GoalDone { id },
            GoalAct::Stop => Call::GoalStop { id },
        }
    }
}

impl App {
    /// The open task, in full, when the snapshot has sent it.
    pub fn detail(&self) -> Option<&crate::link::Detail> {
        let id = self.selected.as_deref()?;
        self.summary.as_ref()?.detail.as_ref().filter(|d| d.task.id == id)
    }

    fn next_id(&mut self) -> String {
        self.work.counter += 1;
        format!("rune-{}-{}-{}", std::process::id(), crate::app::system_ms(), self.work.counter)
    }

    /// What was typed for the task open now is kept for when it is opened again.
    fn park_draft(&mut self) {
        if let Some(id) = self.selected.clone() {
            let text = self.composer.text.clone();
            if text.is_empty() { self.work.drafts.remove(&id) } else { self.work.drafts.insert(id, text) };
        }
    }

    /// Opens a task: its detail and runs are read, a finished task is marked seen.
    pub fn open_task(&mut self, id: &str) {
        let same = self.selected.as_deref() == Some(id);
        if !same {
            self.park_draft();
        }
        self.selected = Some(id.to_owned());
        self.view = None;
        self.notice = None;
        self.work.notice = None;
        self.work.open_run = None;
        self.work.refusal = None;
        self.work.confirm = None;
        self.work.expanded.retain(|k| !k.starts_with("goal:"));
        if !same {
            let draft = self.work.drafts.get(id).cloned().unwrap_or_default();
            self.composer.replace_text(draft);
            self.work.goal_answer.clear();
        }
        self.wanted.set(Some(id));
        self.work.detail_asked = Some(id.to_owned());
        self.call(Call::Refresh);
        self.mark_opened(id);
    }

    /// Opening a task marks it seen, unless the person marked it unread and has not left it.
    fn mark_opened(&mut self, id: &str) {
        self.keep_unread.remove(id);
        // Only while the window has the keyboard: a task opened behind another app is not looked at.
        if self.focused && self.unseen.remove(id) {
            self.call(Call::MarkSeen { task_id: id.to_owned(), seen: true });
        }
    }

    /// A session is seen when it is on screen (not under a full view or the new-session form) in
    /// a window that has the keyboard, and not marked unread by the person.
    pub fn session_in_view(&self) -> Option<&str> {
        self.selected.as_deref().filter(|_| self.focused && self.view.is_none() && self.new_form.is_none() && !self.lab)
    }

    /// The window gained or lost the keyboard: a finished session that was waiting on screen is
    /// seen the moment it comes back.
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.mark_visible_seen();
    }

    fn mark_visible_seen(&mut self) {
        if let Some(id) = self.session_in_view().map(str::to_owned).filter(|id| self.unseen.contains(id) && !self.keep_unread.contains(id)) {
            self.unseen.remove(&id);
            self.call(Call::MarkSeen { task_id: id, seen: true });
        }
    }

    /// "Mark as unread": the dot comes back at once and stays while the task stays open.
    pub fn mark_unread(&mut self, id: &str) {
        self.keep_unread.insert(id.to_owned());
        self.unseen.insert(id.to_owned());
        self.call(Call::MarkSeen { task_id: id.to_owned(), seen: false });
    }

    /// What a Live snapshot asks for next: the open task's runs when its revision moved, the
    /// unseen marks, and (once a minute) the Inbox and the digest.
    pub fn after_live(&mut self) {
        let Some(summary) = self.summary.as_ref() else { return };
        if let Some(detail) = summary.detail.as_ref().filter(|d| Some(d.task.id.as_str()) == self.selected.as_deref()) {
            let id = detail.task.id.clone();
            let revision = format!("{:?}:{}:{}", detail.task.status, detail.task.updated_at_ms, detail.task.events.len());
            if detail.has_folder && self.work.requested.get(&id) != Some(&revision) {
                let moved = self.work.requested.insert(id.clone(), revision).is_some();
                // The task moved on: a diff held for it may be out of date (the whole-task diff and
                // a running run's grow), so the open one is read again, as the web does on every open.
                if moved {
                    let prefix = format!("{id}|");
                    self.work.diffs.retain(|k, _| !k.starts_with(&prefix));
                    self.work.diff_load.retain(|k, _| !k.starts_with(&prefix));
                    if let Some(key) = self.work.open_run {
                        self.work.diff_load.insert(diff_key(&id, key.run()), DiffLoad::Loading);
                        self.call(Call::CheckpointDiff { task_id: id.clone(), run: key.run() });
                    }
                }
                self.call(Call::Checkpoints { task_id: id });
            }
        }
        let now = self.now_ms;
        if now - self.neko.asked_ms >= NEKO_POLL_MS {
            self.neko.asked_ms = now;
            self.call(Call::Unseen);
            self.call(Call::Inbox);
            self.call(Call::Digest);
        }
        // A finished task the person is looking at is seen (unless they marked it unread).
        self.mark_visible_seen();
    }

    /// Sends the composer's text: queued with its mode while the task is in a turn, else as a
    /// reply. The text stays until the daemon takes it (`TaskComposer`).
    pub fn send_composer(&mut self, as_mode: Option<DeliveryMode>) {
        let text = self.composer.text.trim().to_owned();
        if text.is_empty() {
            return;
        }
        let Some(task) = self.detail().map(|d| d.task.clone()) else { return };
        self.work.notice = None;
        if in_turn(task.status) {
            let mode = as_mode.unwrap_or_else(|| delivery(self.steer));
            let key = format!("{}|{mode:?}|{text}", task.id);
            let id = match &self.work.attempt {
                Some((k, id)) if *k == key => id.clone(),
                _ => {
                    let id = self.next_id();
                    self.work.attempt = Some((key, id.clone()));
                    id
                }
            };
            self.call(Call::Enqueue { task_id: task.id, command_id: id, text, mode });
        } else if self.work.busy.insert(format!("reply:{}", task.id)) {
            self.call(Call::Reply { task_id: task.id, text });
        }
    }

    pub fn cancel_queued(&mut self, task_id: &str, id: &str) {
        self.call(Call::CancelQueued { task_id: task_id.to_owned(), id: id.to_owned() });
    }

    pub fn toggle(&mut self, key: &str) {
        if !self.work.expanded.remove(key) {
            self.work.expanded.insert(key.to_owned());
        }
    }

    pub fn open(&self, key: &str) -> bool {
        self.work.expanded.contains(key)
    }

    /// Asks before a destructive press; `confirm_yes` runs it.
    pub fn ask_confirm(&mut self, confirm: Confirm) {
        self.work.confirm = Some(confirm);
    }

    pub fn confirm_yes(&mut self) {
        if let Some(c) = self.work.confirm.take() {
            if let Call::Undo { .. } | Call::Redo { .. } = &c.call {
                self.work.undoing = true;
                self.work.refusal = None;
            }
            self.call(c.call);
        }
    }

    pub fn confirm_no(&mut self) {
        self.work.confirm = None;
    }

    /// Stop the task: asks first (the web's "Stop it?").
    pub fn ask_stop(&mut self, id: &str, title: &str) {
        self.ask_confirm(Confirm {
            title: format!("Stop “{title}”?"),
            detail: "The agent stops where it is. Its files and worktree stay on disk, and you can start it again.".into(),
            ok: "Stop it".into(),
            cancel: "Keep working".into(),
            call: Call::Stop { task_id: id.to_owned() },
        });
    }

    /// "Undo this run": asks first, with what the undo will do.
    pub fn ask_undo(&mut self, task_id: &str, options: UndoOptions) {
        let through = options.through_later;
        let n = options.run;
        self.ask_confirm(Confirm {
            title: if through { format!("Undo back to run {n}?") } else { format!("Undo run {n}?") },
            detail: if options.revert {
                format!("Adds a commit that reverts run {n}{}. History is kept. A safety checkpoint is taken first, so you can redo it.", if through { " and the runs after it" } else { "" })
            } else {
                format!("The folder goes back to how it was before run {n} started{}. A safety checkpoint is taken first, so you can redo it.", if through { ", which also undoes the runs after it" } else { "" })
            },
            ok: if through { "Undo back to here".into() } else { "Undo this run".into() },
            cancel: "Keep it".into(),
            call: Call::Undo { task_id: task_id.to_owned(), run: n, through_later: through, snapshot_unsaved: options.snapshot_unsaved, revert: options.revert },
        });
    }

    pub fn redo(&mut self, task_id: &str) {
        self.work.undoing = true;
        self.work.refusal = None;
        self.call(Call::Redo { task_id: task_id.to_owned() });
    }

    /// Opens or closes a run's diff, asking for it the first time.
    pub fn toggle_run(&mut self, task_id: &str, key: RunKey) {
        if self.work.open_run == Some(key) {
            self.work.open_run = None;
            return;
        }
        self.work.open_run = Some(key);
        let k = diff_key(task_id, key.run());
        if !self.work.diffs.contains_key(&k) && !matches!(self.work.diff_load.get(&k), Some(DiffLoad::Loading)) {
            self.work.diff_load.insert(k, DiffLoad::Loading);
            self.call(Call::CheckpointDiff { task_id: task_id.to_owned(), run: key.run() });
        }
    }

    /// Opens a tool card; an edit's card builds its diff from what the tool changed.
    pub fn toggle_tool(&mut self, tool: &crate::turns::Tool) {
        let key = format!("tool:{}", tool.id);
        let opening = !self.open(&key);
        self.toggle(&key);
        if opening && let Some(edit) = tool.edit.as_ref().filter(|_| !self.work.edit_diffs.contains_key(&tool.id)) {
            let file = rune_kit::diff::FileDiff::from_texts(&edit.path, &edit.old, &edit.new);
            self.work.edit_diffs.insert(tool.id.clone(), DiffState::new(vec![file]));
        }
    }

    pub fn goal_do(&mut self, goal: &Goal, act: GoalAct) {
        self.call(act.call(goal));
    }

    /// Answers a blocked goal: the typed words, or none, which takes its default.
    pub fn goal_answer(&mut self, goal_id: &str, use_default: bool) {
        let text = if use_default { String::new() } else { self.work.goal_answer.text.trim().to_owned() };
        if !use_default && text.is_empty() {
            return;
        }
        self.work.goal_answer.clear();
        self.call(Call::GoalAnswer { id: goal_id.to_owned(), text });
    }

    /// What the right-click menu of an Inbox item offers. A routine's item: its default (the daemon's
    /// path the phone uses) and its link. A goal's item: the goal's own actions (its answer, or
    /// resume, extend and stop), never "Use the default" from the routines' path.
    pub fn item_actions(&self, item: &InboxItem) -> Vec<ItemAction> {
        let mut out = Vec::new();
        if let Some(gid) = &item.goal {
            if let Some(goal) = self.summary.as_ref().and_then(|m| m.goals.iter().find(|g| &g.id == gid)) {
                if goal.status == GoalStatus::Blocked
                    && let Some(b) = &goal.blocked
                {
                    out.push(ItemAction { label: format!("Answer with the default: {}", b.default_answer), act: ItemAct::Call(Call::GoalAnswer { id: gid.clone(), text: String::new() }) });
                }
                for act in goal_actions(goal) {
                    if matches!(act, GoalAct::Resume | GoalAct::Extend | GoalAct::Stop) && (goal.status == GoalStatus::Paused || act == GoalAct::Stop) {
                        out.push(ItemAction { label: act.label(goal), act: ItemAct::Call(act.call(goal)) });
                    }
                }
            }
        } else if let Some(d) = &item.default {
            out.push(ItemAction { label: format!("Use the default: {d}"), act: ItemAct::Call(Call::AnswerNeedsYou { seq: item.seq }) });
        }
        if let Some(link) = &item.link {
            out.push(ItemAction { label: "Open".to_owned(), act: ItemAct::Link(link.clone()) });
        }
        out
    }

    pub fn run_item_action(&mut self, act: ItemAct) {
        self.neko.notes.clear();
        match act {
            ItemAct::Call(c) => self.call(c),
            ItemAct::Link(url) => self.open_link(&url),
        }
    }

    /// "Use the default" on an Inbox item: the same daemon path the phone uses.
    pub fn answer_default(&mut self, seq: u64) {
        self.call(Call::AnswerNeedsYou { seq });
    }

    pub fn mark_digest_read(&mut self) {
        self.neko.digest = None;
        self.call(Call::MarkDigestRead);
    }

    /// Sends the ask box's text to Neko.
    pub fn ask_neko(&mut self) {
        let text = self.ask.text.trim().to_owned();
        if text.is_empty() {
            return;
        }
        if self.work.busy.insert("ask".to_owned()) {
            self.call(Call::Ask { text });
        }
    }

    /// Opens a task by id from a link in Neko's column (a handoff line).
    pub fn open_linked(&mut self, id: &str) {
        if self.summary.as_ref().is_some_and(|s| s.task(id).is_some()) {
            self.open_task(id);
        }
    }

    /// What a daemon answer does to the state. True when something on screen changed.
    pub fn apply_reply(&mut self, call: &Call, reply: Reply) -> bool {
        // The pages' own answers (routines, calendar, phone, quota, plugins, the model catalog).
        let mut page_changed = self.apply_page_reply(call, &reply);
        // Whatever the answer, the write is no longer in flight.
        match call {
            Call::Reply { task_id, .. } => self.work.busy.remove(&format!("reply:{task_id}")),
            Call::Ask { .. } => self.work.busy.remove("ask"),
            Call::CreateTask { .. } => self.work.busy.remove("new"),
            _ => false,
        };
        match reply {
            Reply::Snapshot(summary) => {
                let mut created = None;
                if let Some(key) = note_key(call) {
                    self.neko.notes.remove(&key);
                }
                // The answer to a write is the new workbench; a text the daemon took leaves the box.
                match call {
                    Call::Enqueue { task_id, text, .. } | Call::Reply { task_id, text } => {
                        if self.selected.as_deref() == Some(task_id) && self.composer.text.trim() == text {
                            self.composer.clear();
                        }
                        self.work.drafts.remove(task_id);
                        self.work.attempt = None;
                    }
                    Call::Ask { text } if self.ask.text.trim() == text => self.ask.clear(),
                    Call::Undo { .. } | Call::Redo { .. } => {
                        self.work.undoing = false;
                        self.work.open_run = None;
                        self.work.requested.clear();
                        self.work.diffs.clear();
                        self.work.diff_load.clear();
                    }
                    Call::CreateTask { .. } => created = self.created_task(&summary),
                    _ => {}
                }
                // An answer from the daemon proves the link is up.
                self.link = crate::app::LinkState::Live;
                self.apply_summary(summary);
                if let Some(id) = created {
                    self.after_created(&id);
                }
                true
            }
            Reply::Models { choices, .. } => {
                if let Some(form) = self.new_form.as_mut() {
                    form.choices = choices;
                }
                true
            }
            Reply::Routines(_) | Reply::Calendar(_) | Reply::CalendarStatus(_) | Reply::Phone(_) | Reply::PhoneTest(_) | Reply::Quota(_) | Reply::Plugins(_) => page_changed.take().unwrap_or(false),
            Reply::Checkpoints { task_id, list } => {
                let changed = self.work.checkpoints.get(&task_id) != Some(&list);
                self.work.checkpoints.insert(task_id, list);
                changed
            }
            Reply::Diff { task_id, run, patch, truncated } => {
                let key = diff_key(&task_id, run);
                self.work.diffs.insert(key.clone(), DiffState::new(rune_kit::diff::parse(&patch)));
                self.work.diff_load.insert(key, DiffLoad::Ready { truncated });
                true
            }
            Reply::Unseen(ids) => {
                let mut next: HashSet<String> = ids.into_iter().collect();
                // A task marked unread here stays unread until it is opened again.
                for id in &self.keep_unread {
                    next.insert(id.clone());
                }
                let changed = next != self.unseen;
                self.unseen = next;
                changed
            }
            Reply::Digest(digest) => {
                let changed = self.neko.digest != digest;
                self.neko.digest = digest;
                changed
            }
            Reply::Inbox(items) => {
                let changed = self.neko.inbox != items || !self.neko.loaded;
                self.neko.inbox = items;
                self.neko.loaded = true;
                changed
            }
            Reply::Answered(summary) => {
                self.neko.notes.clear();
                self.neko.note = Some(format!("Done: {summary}"));
                self.call(Call::Inbox);
                true
            }
            Reply::Done => {
                // A page's write the daemon took without sending the page back: read the page again.
                if !call.reads() && crate::pages::control_key(call).is_some() {
                    self.pages.asked_ms = i64::MIN / 2;
                    if let Some(view) = self.view {
                        self.refresh_page(view);
                    }
                }
                false
            }
            Reply::Refused(message) => {
                if self.page_refused(call, message.clone()) {
                    return true;
                }
                if let Some(key) = note_key(call) {
                    self.neko.notes.insert(key, message);
                    return true;
                }
                match call {
                    Call::Undo { task_id: _, run, through_later, snapshot_unsaved, revert } => {
                        self.work.undoing = false;
                        let options = UndoOptions { run: *run, through_later: *through_later, snapshot_unsaved: *snapshot_unsaved, revert: *revert };
                        match parse_refusal(&message, options) {
                            Some(r) => self.work.refusal = Some(r),
                            None => self.work.notice = Some(message),
                        }
                    }
                    Call::Redo { .. } => {
                        self.work.undoing = false;
                        self.work.notice = Some(message);
                    }
                    Call::Enqueue { .. } | Call::Reply { .. } => self.work.notice = Some(format!("{message} Your text is still here.")),
                    Call::CheckpointDiff { task_id, run } => {
                        self.work.diff_load.insert(diff_key(task_id, *run), DiffLoad::Failed(message));
                    }
                    Call::Checkpoints { .. } | Call::Unseen | Call::Digest | Call::Inbox | Call::Refresh | Call::Models => {}
                    Call::CreateTask { .. } => self.new_error(message),
                    _ => self.work.notice = Some(message),
                }
                true
            }
            Reply::Failed(error) => {
                let line = crate::link::calm(&error, std::time::Duration::from_secs(1));
                if self.page_refused(call, line.clone()) {
                    return true;
                }
                if let Some(key) = note_key(call) {
                    self.neko.notes.insert(key, line);
                    return true;
                }
                match call {
                    Call::Undo { .. } | Call::Redo { .. } => self.work.undoing = false,
                    Call::CheckpointDiff { task_id, run } => {
                        self.work.diff_load.insert(diff_key(task_id, *run), DiffLoad::Failed(line));
                    }
                    Call::Refresh | Call::Unseen | Call::Digest | Call::Inbox | Call::Models | Call::Checkpoints { .. } => return false,
                    Call::CreateTask { .. } => self.new_error(line),
                    _ => self.work.notice = Some(line),
                }
                true
            }
        }
    }
}
