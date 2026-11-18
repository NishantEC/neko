//! The activity pill (P4, docs/knowledge/rune-overlays.md): a small capsule in the corner of the screen while Neko works, a port of
//! the Swift app's `PresenceController` and `PresenceCapsule` (`native/NekoKit/Sources/NekoNative/Presence.swift`). It appears for work,
//! lingers to show how it ended, then hides. It never takes focus; a press brings this window forward and opens what it names.
//!
//! Two halves. Here, with no window in it so tests can walk it: [`Facts`] (what the capsule reads of one workbench summary),
//! [`state_from`] (Swift's `PresenceState.from`, word for word) and [`Tracker`] (Swift's `update`/`show`/`hide`: what to do now and when
//! a lingering outcome ends). Then the drawing: [`Pill`] is the overlay's state and [`view`] its view, built from Rune's theme (always the
//! dark one, as Swift forces a dark colour scheme on the capsule), the one icon set and the status glyphs.

use neko_protocol::workbench::TaskStatus;
use rune::{AppHandle, Cx, El, LoopSpec, el, icon, row, spacer, text};

use crate::app::App;
use crate::icons;
use crate::link::Summary;
use crate::status::{StatusKind, status_glyph};
use crate::theme::{self, Neko};

/// Outcome states disappear on their own after this long (Swift's `PresenceState.lingerSeconds`).
pub const LINGER_MS: i64 = 6_000;
/// How many characters of a title the label keeps (Swift's `prefix(40)`).
pub const TITLE_CHARS: usize = 40;

/// What the capsule says it is doing. The cases Swift's `PresenceState.from` can return.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Thinking,
    Creating,
    Debugging,
    NeedsYou,
    Ready,
    Failed,
}

impl Activity {
    /// Swift's `NekoActivity.animates` for these cases: working states and "needs you" move, settled ones hold still.
    pub fn animates(self) -> bool {
        !matches!(self, Activity::Ready | Activity::Failed)
    }

    /// The states that show the working ring (the others show their status glyph).
    pub fn working(self) -> bool {
        matches!(self, Activity::Thinking | Activity::Creating | Activity::Debugging)
    }
}

/// What the capsule should say, from one summary and the previous one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresenceState {
    pub activity: Activity,
    pub label: String,
    /// Outcome states disappear on their own after [`LINGER_MS`].
    pub lingers: bool,
    /// The task the capsule names, which a press selects (Swift only opens the workspace; here the window can select).
    pub task: Option<String>,
}

/// One task as the capsule reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskFact {
    pub id: String,
    pub status: TaskStatus,
    pub title: String,
}

/// What the capsule reads of a summary (Swift's `snapshot["tasks"]` and `snapshot["conversation"]`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub tasks: Vec<TaskFact>,
    /// Any conversation message still being written (Swift's `replying`: any pending message, not only the last).
    pub replying: bool,
    /// A pending message with a tool call waiting for a yes.
    pub tool_waiting: bool,
}

impl Facts {
    pub fn of(summary: &Summary) -> Facts {
        use neko_protocol::workbench::ChatToolStatus;
        Facts {
            tasks: summary.tasks.iter().map(|t| TaskFact { id: t.id.clone(), status: t.status, title: t.title.clone() }).collect(),
            replying: summary.conversation.iter().any(|m| m.pending),
            tool_waiting: summary.conversation.iter().any(|m| m.pending && m.tool_calls.iter().any(|c| c.status == ChatToolStatus::AwaitingApproval)),
        }
    }
}

fn is_running(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Queued | TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing)
}

/// Swift's `friendlyTaskStatus` for the running statuses (`DesignSystem/Controls.swift`).
fn friendly(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "Queued",
        TaskStatus::Planning => "Making a plan",
        TaskStatus::Building => "Preparing a fix",
        TaskStatus::Reviewing => "Checking the result",
        TaskStatus::AwaitingApproval => "Needs approval",
        TaskStatus::ReadyForReview => "Ready to review",
        TaskStatus::Completed => "Completed",
        TaskStatus::Cancelled => "Cancelled",
        TaskStatus::Failed => "Stopped",
    }
}

fn title_of(task: &TaskFact) -> String {
    task.title.chars().take(TITLE_CHARS).collect()
}

/// Swift's `PresenceState.from(snapshot:previous:)`: needs-your-OK first, then running work, then a reply being written, then (for one
/// summary only) how something that just stopped ended. None when nothing is happening.
pub fn state_from(now: &Facts, previous: Option<&Facts>) -> Option<PresenceState> {
    let running: Vec<&TaskFact> = now.tasks.iter().filter(|t| is_running(t.status)).collect();
    if now.tool_waiting {
        return Some(PresenceState { activity: Activity::NeedsYou, label: "Neko needs your OK for a tool".to_owned(), lingers: false, task: None });
    }
    if let Some(task) = running.first() {
        let activity = match task.status {
            TaskStatus::Building => Activity::Creating,
            TaskStatus::Reviewing => Activity::Debugging,
            _ => Activity::Thinking,
        };
        let more = if running.len() > 1 { format!(" +{} more", running.len() - 1) } else { String::new() };
        return Some(PresenceState { activity, label: format!("{}: {}{}", friendly(task.status), title_of(task), more), lingers: false, task: Some(task.id.clone()) });
    }
    if now.replying {
        return Some(PresenceState { activity: Activity::Thinking, label: "Neko is replying".to_owned(), lingers: false, task: None });
    }
    // Something just finished: show how it went, briefly.
    let previous = previous?;
    for task in &now.tasks {
        let before = previous.tasks.iter().find(|b| b.id == task.id).map(|b| b.status);
        if !before.is_some_and(is_running) {
            continue;
        }
        let (activity, words) = match task.status {
            TaskStatus::ReadyForReview => (Activity::Ready, "Ready to review"),
            TaskStatus::AwaitingApproval => (Activity::NeedsYou, "Plan ready"),
            TaskStatus::Failed => (Activity::Failed, "Needs attention"),
            _ => continue,
        };
        return Some(PresenceState { activity, label: format!("{words}: {}", title_of(task)), lingers: true, task: Some(task.id.clone()) });
    }
    None
}

/// What the overlay should do now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Nothing,
    /// Show the capsule saying this (a new state, or the same one again with its linger timer restarted).
    Show(PresenceState),
    Hide,
}

/// Swift's `PresenceController` without its panel: the on/off preference, the previous summary, what is on screen and when a lingering
/// outcome ends. Time is the caller's (milliseconds), so a test can step it.
#[derive(Clone, Debug)]
pub struct Tracker {
    /// `neko.presence.enabled` in Swift (UserDefaults, default on); here the layout's own saved value (`app::load_presence`).
    pub enabled: bool,
    previous: Option<Facts>,
    shown: Option<PresenceState>,
    hide_at: Option<i64>,
}

impl Default for Tracker {
    fn default() -> Self {
        Tracker { enabled: true, previous: None, shown: None, hide_at: None }
    }
}

impl Tracker {
    pub fn with_enabled(enabled: bool) -> Tracker {
        Tracker { enabled, ..Tracker::default() }
    }

    /// What is on screen.
    pub fn shown(&self) -> Option<&PresenceState> {
        self.shown.as_ref()
    }

    /// When a lingering outcome ends (milliseconds), if one is on screen.
    pub fn hide_at(&self) -> Option<i64> {
        self.hide_at
    }

    /// A new summary's facts (Swift's `update`). The first one is only remembered. Facts equal to the last are not a new summary.
    pub fn observe(&mut self, facts: Facts, now_ms: i64) -> Step {
        if self.previous.as_ref() == Some(&facts) {
            return Step::Nothing;
        }
        let previous = self.previous.replace(facts.clone());
        if !self.enabled || previous.is_none() {
            return Step::Nothing;
        }
        match state_from(&facts, previous.as_ref()) {
            // Keep a lingering outcome on screen until its timer ends.
            None if self.shown.as_ref().is_some_and(|s| s.lingers) => Step::Nothing,
            None => self.hide(),
            Some(state) => self.show(state, now_ms),
        }
    }

    /// The linger timer: hides an outcome that has been there long enough.
    pub fn tick(&mut self, now_ms: i64) -> Step {
        match self.hide_at {
            Some(at) if now_ms >= at => self.hide(),
            _ => Step::Nothing,
        }
    }

    /// The preference changed (Swift's `refresh`: turning it off hides the capsule).
    pub fn set_enabled(&mut self, on: bool) -> Step {
        self.enabled = on;
        if on { Step::Nothing } else { self.hide() }
    }

    fn show(&mut self, state: PresenceState, now_ms: i64) -> Step {
        // Swift cancels the timer first, and starts a new one only for an outcome.
        self.hide_at = state.lingers.then_some(now_ms + LINGER_MS);
        let same = self.shown.as_ref() == Some(&state);
        self.shown = Some(state.clone());
        if same { Step::Nothing } else { Step::Show(state) }
    }

    fn hide(&mut self) -> Step {
        self.hide_at = None;
        if self.shown.take().is_some() { Step::Hide } else { Step::Nothing }
    }
}

// ---- the overlay ----------------------------------------------------------------------------------

/// Space around the capsule for its glow (Swift's `glowRoom` is 28 for a 14 point blur; the glow here is a 12 point shadow).
pub const GLOW: f32 = 16.0;
/// The capsule's height: a 16 point glyph with 10 points above and below (Swift's padding).
pub const CAPSULE_H: f32 = 36.0;
/// The window: wide enough for the longest label (a status, 40 characters of title and `+N more`) and the glow on every side.
/// Everything outside the capsule is clear and lets clicks through (`OverlayInput::Regions`).
pub const WINDOW: (f32, f32) = (600.0, CAPSULE_H + 2.0 * GLOW);
/// Where the capsule sits: this far from the screen's right and bottom edges (the visible area, Swift's `visibleFrame`).
pub const MARGIN: f32 = 20.0;

/// The overlay's state.
pub struct Pill {
    pub state: Option<PresenceState>,
    /// The pointer is over the capsule: the label shows (Swift expands on hover).
    pub expanded: bool,
    /// Where a press goes: the window, to select what the capsule names.
    pub main: Option<AppHandle<App>>,
    /// For tests and scenes: show the label without a pointer (Swift's `forceExpanded`).
    pub force_expanded: bool,
}

impl Pill {
    pub fn new(main: Option<AppHandle<App>>) -> Pill {
        Pill { state: None, expanded: false, main, force_expanded: false }
    }

    /// A press: bring the window forward and select what the capsule names.
    pub fn open_neko(&self) {
        let Some(main) = &self.main else { return };
        let task = self.state.as_ref().and_then(|s| s.task.clone());
        main.update(move |app, tx| {
            if let Some(id) = &task {
                app.select(id);
            }
            tx.instant();
        });
        main.open();
    }
}

/// The glow's colour: the activity's meaning in the theme (the same hues the status glyphs use).
fn hue(activity: Activity, t: &Neko) -> rune::Color {
    match activity {
        Activity::Thinking | Activity::Creating | Activity::Debugging => t.meaning.iris,
        Activity::NeedsYou => t.meaning.amber,
        Activity::Ready => t.meaning.sky,
        Activity::Failed => t.meaning.coral,
    }
}

fn glyph(activity: Activity, t: &Neko, moving: bool) -> El {
    match activity {
        Activity::Thinking | Activity::Creating | Activity::Debugging => {
            let track = icon(&icons::RING_TRACK, 16.0).absolute(0.0, 0.0).color(t.text.base.alpha(0.12));
            let mut arc = icon(&icons::RING_QUARTER, 16.0).absolute(0.0, 0.0).color(t.meaning.iris);
            if moving {
                arc = arc.looping(LoopSpec::rotation(1.2));
            }
            el().key("pill-glyph").size(16.0, 16.0).role("img").label("Working").child(track).child(arc)
        }
        Activity::NeedsYou => status_glyph(StatusKind::InProgress, t, 16.0, Some("Needs you")),
        Activity::Ready => status_glyph(StatusKind::Review, t, 16.0, Some("Ready to review")),
        Activity::Failed => status_glyph(StatusKind::Failed, t, 16.0, Some("Needs attention")),
    }
}

/// The capsule: dark glass, a 1 point border, the glyph, and on hover the label; a glow in the activity's hue under it that breathes
/// while the activity moves, unless Reduce Motion is on.
pub fn view(p: &Pill, cx: &mut Cx<Pill>) -> El {
    let t = theme::theme(true);
    let Some(state) = &p.state else { return row().key("pill-root").w_full().h_full() };
    let moving = state.activity.animates() && !cx.system_reduced_motion();
    let expanded = p.expanded || p.force_expanded;
    let hue = hue(state.activity, &t);
    let mut capsule = row()
        .key("pill")
        .hit()
        .role("button")
        .label(state.label.clone())
        .h(CAPSULE_H)
        .max_w(WINDOW.0 - 2.0 * GLOW)
        .center_y()
        .gap(10.0)
        .pl(if expanded { 12.0 } else { 10.0 })
        .pr(if expanded { 16.0 } else { 10.0 })
        .radius(CAPSULE_H / 2.0)
        .fill(t.surface.window.alpha(0.88))
        .border(1.0, t.surface.border)
        .on_hover_change(|p: &mut Pill, inside, _| p.expanded = inside)
        .on_press(|p: &mut Pill| p.open_neko())
        .child(glyph(state.activity, &t, moving));
    if expanded {
        capsule = capsule.child(text(state.label.clone()).key("pill-label").font(t.ty.row).color(t.text.strong).truncate().min_w(0.0));
    }
    // The glow is the capsule's own shadow in the activity's hue; it breathes through a looping opacity on a sibling copy of the shape.
    let glow_alpha = if state.activity.animates() { 0.5 } else { 0.3 };
    let mut glow = el().key("pill-glow").absolute(0.0, 0.0).w_full().h(CAPSULE_H).radius(CAPSULE_H / 2.0).shadow(5.0, 14.0, hue.alpha(glow_alpha));
    if moving {
        glow = glow.looping(LoopSpec::opacity(1.6, 0.0, 0.45, 1.0));
    }
    let stack = el().key("pill-stack").h(CAPSULE_H).max_w(WINDOW.0 - 2.0 * GLOW).child(glow).child(capsule);
    row().key("pill-root").w_full().h_full().center_y().px(GLOW).child(spacer()).child(stack)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, status: TaskStatus, title: &str) -> TaskFact {
        TaskFact { id: id.into(), status, title: title.into() }
    }

    fn facts(tasks: Vec<TaskFact>) -> Facts {
        Facts { tasks, ..Facts::default() }
    }

    #[test]
    fn working_states_use_swifts_words_and_activities() {
        let now = facts(vec![task("a", TaskStatus::Building, "Fix the login redirect")]);
        let s = state_from(&now, None).unwrap();
        assert_eq!((s.activity, s.label.as_str(), s.lingers), (Activity::Creating, "Preparing a fix: Fix the login redirect", false));
        let s = state_from(&facts(vec![task("a", TaskStatus::Reviewing, "x")]), None).unwrap();
        assert_eq!((s.activity, s.label.as_str()), (Activity::Debugging, "Checking the result: x"));
        let s = state_from(&facts(vec![task("a", TaskStatus::Planning, "x"), task("b", TaskStatus::Queued, "y"), task("c", TaskStatus::Queued, "z")]), None).unwrap();
        assert_eq!((s.activity, s.label.as_str()), (Activity::Thinking, "Making a plan: x +2 more"), "the first running task, and how many more");
        let s = state_from(&facts(vec![task("q", TaskStatus::Queued, "later")]), None).unwrap();
        assert_eq!(s.label, "Queued: later");
    }

    #[test]
    fn a_long_title_is_cut_at_forty_characters() {
        let long = "é".repeat(60);
        let s = state_from(&facts(vec![task("a", TaskStatus::Building, &long)]), None).unwrap();
        assert_eq!(s.label.chars().filter(|c| *c == 'é').count(), 40);
    }

    #[test]
    fn a_tool_waiting_for_a_yes_comes_first_and_a_reply_comes_after_work() {
        let mut now = facts(vec![task("a", TaskStatus::Building, "x")]);
        now.tool_waiting = true;
        now.replying = true;
        let s = state_from(&now, None).unwrap();
        assert_eq!((s.activity, s.label.as_str()), (Activity::NeedsYou, "Neko needs your OK for a tool"));
        now.tool_waiting = false;
        assert_eq!(state_from(&now, None).unwrap().activity, Activity::Creating, "running work beats a reply");
        now.tasks.clear();
        let s = state_from(&now, None).unwrap();
        assert_eq!((s.activity, s.label.as_str()), (Activity::Thinking, "Neko is replying"));
    }

    #[test]
    fn idle_is_nothing() {
        assert_eq!(state_from(&facts(vec![task("a", TaskStatus::Completed, "x")]), None), None);
        assert_eq!(state_from(&facts(vec![]), Some(&facts(vec![]))), None);
    }

    #[test]
    fn an_outcome_shows_only_for_a_task_that_was_running_and_lingers() {
        let before = facts(vec![task("a", TaskStatus::Building, "Ship it"), task("b", TaskStatus::Reviewing, "Plan it"), task("c", TaskStatus::Building, "Break it")]);
        let ready = state_from(&facts(vec![task("a", TaskStatus::ReadyForReview, "Ship it")]), Some(&before)).unwrap();
        assert_eq!((ready.activity, ready.label.as_str(), ready.lingers), (Activity::Ready, "Ready to review: Ship it", true));
        assert_eq!(ready.task.as_deref(), Some("a"));
        let plan = state_from(&facts(vec![task("b", TaskStatus::AwaitingApproval, "Plan it")]), Some(&before)).unwrap();
        assert_eq!((plan.activity, plan.label.as_str(), plan.lingers), (Activity::NeedsYou, "Plan ready: Plan it", true));
        let failed = state_from(&facts(vec![task("c", TaskStatus::Failed, "Break it")]), Some(&before)).unwrap();
        assert_eq!((failed.activity, failed.label.as_str(), failed.lingers), (Activity::Failed, "Needs attention: Break it", true));
        // Cancelled or completed is not an outcome the capsule shows; a task that was not running before is not one either.
        assert_eq!(state_from(&facts(vec![task("a", TaskStatus::Cancelled, "Ship it")]), Some(&before)), None);
        assert_eq!(state_from(&facts(vec![task("z", TaskStatus::Failed, "New")]), Some(&before)), None);
        // And the next summary no longer shows it (the previous status is no longer a running one).
        assert_eq!(state_from(&facts(vec![task("a", TaskStatus::ReadyForReview, "Ship it")]), Some(&facts(vec![task("a", TaskStatus::ReadyForReview, "Ship it")]))), None);
    }

    #[test]
    fn the_first_summary_is_only_remembered_and_an_outcome_lingers_six_seconds() {
        let mut tr = Tracker::default();
        let running = facts(vec![task("a", TaskStatus::Building, "Ship it")]);
        assert_eq!(tr.observe(running.clone(), 0), Step::Nothing, "the first summary shows nothing");
        // Working shows at once.
        let Step::Show(state) = tr.observe(facts(vec![task("a", TaskStatus::Building, "Ship it"), task("b", TaskStatus::Queued, "Other")]), 1_000) else { panic!("working shows") };
        assert_eq!(state.label, "Preparing a fix: Ship it +1 more");
        assert_eq!(tr.hide_at(), None);
        // The task stops: the outcome shows and lingers; the same summary again changes nothing.
        let done = facts(vec![task("a", TaskStatus::ReadyForReview, "Ship it"), task("b", TaskStatus::Completed, "Other")]);
        let Step::Show(outcome) = tr.observe(done.clone(), 2_000) else { panic!("the outcome shows") };
        assert_eq!(outcome.activity, Activity::Ready);
        assert_eq!(tr.hide_at(), Some(2_000 + LINGER_MS));
        assert_eq!(tr.observe(done.clone(), 2_500), Step::Nothing);
        // Nothing else happens in the summary: the outcome stays until its timer ends.
        let quiet = Facts { replying: false, ..done.clone() };
        let mut later = quiet.clone();
        later.tasks.push(task("c", TaskStatus::Completed, "Old"));
        assert_eq!(tr.observe(later, 3_000), Step::Nothing, "a lingering outcome is kept");
        assert_eq!(tr.tick(7_999), Step::Nothing);
        assert_eq!(tr.tick(8_000), Step::Hide);
        assert!(tr.shown().is_none() && tr.hide_at().is_none());
        assert_eq!(tr.tick(9_000), Step::Nothing, "hiding is done once");
    }

    #[test]
    fn work_that_ends_without_an_outcome_hides_the_capsule() {
        let mut tr = Tracker::default();
        tr.observe(facts(vec![task("a", TaskStatus::Building, "x")]), 0);
        assert!(matches!(tr.observe(facts(vec![task("a", TaskStatus::Building, "x"), task("b", TaskStatus::Queued, "y")]), 1), Step::Show(_)));
        assert_eq!(tr.observe(facts(vec![task("a", TaskStatus::Cancelled, "x"), task("b", TaskStatus::Cancelled, "y")]), 2), Step::Hide);
        assert_eq!(tr.observe(facts(vec![]), 3), Step::Nothing, "nothing to hide");
    }

    #[test]
    fn a_new_working_state_replaces_a_lingering_outcome() {
        let mut tr = Tracker::default();
        tr.observe(facts(vec![task("a", TaskStatus::Building, "x")]), 0);
        tr.observe(facts(vec![task("a", TaskStatus::Failed, "x")]), 1);
        assert!(tr.shown().is_some_and(|s| s.lingers));
        let Step::Show(s) = tr.observe(facts(vec![task("a", TaskStatus::Failed, "x"), task("n", TaskStatus::Planning, "next")]), 2) else { panic!("new work shows") };
        assert!(!s.lingers && tr.hide_at().is_none(), "the timer is cancelled by a working state");
    }

    #[test]
    fn the_preference_off_shows_nothing_and_hides_what_is_there() {
        let mut tr = Tracker { enabled: false, ..Tracker::default() };
        tr.observe(facts(vec![]), 0);
        assert_eq!(tr.observe(facts(vec![task("a", TaskStatus::Building, "x")]), 1), Step::Nothing);
        assert_eq!(tr.set_enabled(true), Step::Nothing);
        assert!(matches!(tr.observe(facts(vec![task("a", TaskStatus::Building, "x"), task("b", TaskStatus::Queued, "y")]), 2), Step::Show(_)));
        assert_eq!(tr.set_enabled(false), Step::Hide);
        assert!(tr.shown().is_none());
    }

    #[test]
    fn facts_read_the_conversation_the_way_swift_does() {
        use neko_protocol::workbench::{ChatMessage, ChatRole, ChatToolCall, ChatToolStatus};
        let mut summary = Summary::default();
        let mut m = ChatMessage { agent_profile_revision: 0, agent_profile_id: neko_protocol::agent_profiles::default_profile_id(), id: "m".into(), at_ms: 1, role: ChatRole::Neko, parts: vec![], ticket_ids: vec![], pending: true, queued: false, failed: false, responsibility_ids: vec![], actions: vec![], started_ms: 1, tool_calls: vec![], workspace_id: None };
        summary.conversation.push(m.clone());
        summary.conversation.push(ChatMessage { pending: false, id: "n".into(), ..m.clone() });
        let f = Facts::of(&summary);
        assert!(f.replying && !f.tool_waiting, "any pending message is a reply being written, not only the last");
        m.tool_calls.push(ChatToolCall { id: "c".into(), workspace_id: "w".into(), connection_id: "k".into(), tool_name: "t".into(), arguments_json: "{}".into(), status: ChatToolStatus::AwaitingApproval });
        summary.conversation[0] = m;
        assert!(Facts::of(&summary).tool_waiting);
    }
}
