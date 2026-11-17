//! The Board and Tree layouts of All sessions, as the web has them (`pages/work/Tasks.tsx`, `work/board/*`, `work/TreeView.tsx`):
//! the rules, with no drawing (`view/board.rs` and `view/tree.rs` draw).
//!
//! * **Board.** One column per stage in the web's order (`COLUMNS`), a card per task. A card is dragged to another column and
//!   dropped: the move is the daemon's own command (`moveCommand`): Start work, Mark done, or (behind a dialog) Cancel the
//!   task. A drop the web refuses sends nothing and says why. The same filters as the list apply.
//! * **Tree.** A task made by a split, or proposed by another task, sits under its parent; each level is indented 20 points
//!   and a chevron folds it.
//! * The chosen layout is kept in Rune's preferences (`app.rs`).

use std::collections::HashSet;

use neko_protocol::workbench::TaskStatus;
use rune::Phase;

use crate::all_sessions::Stage;
use crate::app::App;
use crate::link::{Call, TaskRow, Wait};
use crate::status::status_label;

/// The three ways All sessions lays its tasks out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Layout {
    #[default]
    List,
    Board,
    Tree,
}

impl Layout {
    pub const ALL: [Layout; 3] = [Layout::List, Layout::Board, Layout::Tree];

    pub fn title(self) -> &'static str {
        match self {
            Layout::List => "List",
            Layout::Board => "Board",
            Layout::Tree => "Tree",
        }
    }
}

/// A column's width, the gap between columns, and so how far one column is from the next.
pub const COLUMN_W: f32 = 280.0;
pub const COLUMN_GAP: f32 = 12.0;
pub const PITCH: f32 = COLUMN_W + COLUMN_GAP;
/// The pointer must travel this far from the press before a card is carried (the web's `distance: 6`).
pub const DRAG_SLOP: f32 = 6.0;
/// The tree's indent per level.
pub const INDENT: f32 = 20.0;

/// The web's `STAGE_EMPTY`: what an empty column says.
pub fn stage_empty(stage: Stage) -> &'static str {
    match stage {
        Stage::NeedsYou => "Nothing is waiting on you.",
        Stage::Review => "Nothing to review yet.",
        Stage::Running => "No agent is working right now.",
        Stage::UpNext => "Nothing is queued.",
        Stage::Done => "Nothing finished yet.",
    }
}

// ------------------------------------------------------------------------------------------------ what a drop means

/// What the daemon is asked to do for a move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    /// `StartTask`: a stopped or waiting task starts (again).
    Start,
    /// `CompleteTask`: a task ready for review is marked done.
    Complete,
    /// The daemon's cancel: work that has not been reviewed is stopped (after a dialog).
    Cancel,
}

impl Move {
    /// The words on the target column while a card is over it (the web's `VERB`).
    pub fn verb(self) -> &'static str {
        match self {
            Move::Start => "Start work",
            Move::Complete => "Mark done",
            Move::Cancel => "Cancel the task",
        }
    }
}

/// What dropping a card on a column means: nothing, a command, or a reason why not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Same,
    Move(Move),
    Refuse(String),
}

/// The web's `moveCommand` (`work/model.ts`): the daemon's own rule for a move, or the reason it is refused.
fn move_command(status: TaskStatus, to: Stage) -> (Option<Move>, Option<&'static str>) {
    use TaskStatus::*;
    let live = matches!(status, Queued | Planning | Building | Reviewing);
    if matches!(to, Stage::Running | Stage::UpNext) && matches!(status, AwaitingApproval | Failed | Cancelled) {
        return (Some(Move::Start), None);
    }
    if to == Stage::Done && status == ReadyForReview {
        return (Some(Move::Complete), None);
    }
    if to == Stage::Done && (status == AwaitingApproval || live) {
        return (Some(Move::Cancel), None);
    }
    if to == Stage::NeedsYou && live {
        return (None, Some("Work that has started can’t go back to waiting. Cancel it, or let it finish."));
    }
    if to == Stage::Review {
        return (None, Some("Neko moves a task to review once its work and checks are done."));
    }
    if to == Stage::Running && status == ReadyForReview {
        return (None, Some("Add a note on the task to send it back with changes."));
    }
    (None, None)
}

/// The web's `friendlyStatus`.
pub fn friendly_status(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::AwaitingApproval => "Needs approval",
        TaskStatus::ReadyForReview => "Ready to review",
        TaskStatus::Queued => "Queued",
        TaskStatus::Planning => "Making a plan",
        TaskStatus::Building => "Preparing a fix",
        TaskStatus::Reviewing => "Checking the result",
        TaskStatus::Completed => "Completed",
        TaskStatus::Cancelled => "Cancelled",
        TaskStatus::Failed => "Stopped",
    }
}

fn fallback_reason(status: TaskStatus, to: Stage) -> String {
    if status == TaskStatus::Completed {
        return "Finished work stays finished. Describe a follow-up in Home instead.".to_owned();
    }
    if status == TaskStatus::Queued && to == Stage::Running {
        return "A queued task starts by itself when a slot is free. Use Run next to put it first.".to_owned();
    }
    if status == TaskStatus::Failed && to == Stage::Done {
        return "A stopped task needs a decision: start it again, or open it and delete it.".to_owned();
    }
    if to == Stage::UpNext {
        return "Work that has started can’t go back to the queue. Stop it first.".to_owned();
    }
    if to == Stage::Done {
        return "Only a task that is ready for review can be marked done.".to_owned();
    }
    format!("A task that is “{}” can’t move there.", friendly_status(status).to_lowercase())
}

/// The web's `decide` (`work/board/moves.ts`): a drop of a task in `status` on the column `to`.
pub fn decide(status: TaskStatus, to: Stage) -> Decision {
    if Stage::of(status) == to {
        return Decision::Same;
    }
    match move_command(status, to) {
        (Some(m), _) => Decision::Move(m),
        (None, Some(reason)) => Decision::Refuse(reason.to_owned()),
        (None, None) => Decision::Refuse(fallback_reason(status, to)),
    }
}

// ------------------------------------------------------------------------------------------------ the card

/// The one action a waiting card has (the same the task page runs), shown on hover or focus and in the right-click menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardAction {
    /// Approve the plan.
    Approve,
    /// A question to answer: opens the task.
    Answer,
    /// A stopped task starts again.
    StartAgain,
}

impl CardAction {
    pub fn label(self) -> &'static str {
        match self {
            CardAction::Approve => "Approve",
            CardAction::Answer => "Answer",
            CardAction::StartAgain => "Start again",
        }
    }
}

/// What a card says about where its task stands: the web's `cardFor` (the words and the action).
pub fn card_words(task: &TaskRow) -> (&'static str, Option<CardAction>) {
    match task.status {
        TaskStatus::Failed => ("Stopped", Some(CardAction::StartAgain)),
        TaskStatus::AwaitingApproval if task.wait == Wait::Question => ("Asks you", Some(CardAction::Answer)),
        TaskStatus::AwaitingApproval => ("Needs approval", Some(CardAction::Approve)),
        TaskStatus::ReadyForReview => ("Ready to review", None),
        TaskStatus::Queued => ("Up next", None),
        other => (friendly_status(other), None),
    }
}

// ------------------------------------------------------------------------------------------------ the tree

/// One visible row of the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRow {
    pub id: String,
    pub depth: usize,
    /// How many tasks it started (shown or not).
    pub children: usize,
    pub folded: bool,
}

/// The web's `taskOrder`: running first, then those waiting on you, review, queued, done; newest first inside each.
fn order_key(row: &TaskRow) -> (usize, i64) {
    let rank = match Stage::of(row.status) {
        Stage::Running => 0,
        Stage::NeedsYou => 1,
        Stage::Review => 2,
        Stage::UpNext => 3,
        Stage::Done => 4,
    };
    (rank, -row.updated_at_ms)
}

/// The tree of `rows` (already filtered): a task whose parent is not among them is a root. Children follow their parent
/// unless it is folded.
pub fn tree_rows(rows: &[&TaskRow], parents: &std::collections::BTreeMap<String, String>, folded: &HashSet<String>) -> Vec<TreeRow> {
    use std::collections::HashMap;
    let shown: HashSet<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    // Each shown task's parent, when that is shown too.
    let parent_of: HashMap<&str, &str> = rows.iter().filter_map(|r| parents.get(&r.id).filter(|p| shown.contains(p.as_str()) && **p != r.id).map(|p| (r.id.as_str(), p.as_str()))).collect();
    let mut by_order: Vec<&TaskRow> = rows.to_vec();
    by_order.sort_by_key(|r| order_key(r));
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    fn walk(id: &str, depth: usize, by_order: &[&TaskRow], parent_of: &HashMap<&str, &str>, folded: &HashSet<String>, seen: &mut HashSet<String>, out: &mut Vec<TreeRow>) {
        if !seen.insert(id.to_owned()) {
            return;
        }
        let kids: Vec<&str> = by_order.iter().filter(|r| parent_of.get(r.id.as_str()).is_some_and(|p| *p == id)).map(|r| r.id.as_str()).collect();
        let is_folded = folded.contains(id) && !kids.is_empty();
        out.push(TreeRow { id: id.to_owned(), depth, children: kids.len(), folded: is_folded });
        if !is_folded {
            for kid in kids {
                walk(kid, depth + 1, by_order, parent_of, folded, seen, out);
            }
        } else {
            // What a folded row hides is not lost: it must not come back as a root below.
            let mut hidden = kids;
            while let Some(h) = hidden.pop() {
                if seen.insert(h.to_owned()) {
                    hidden.extend(by_order.iter().filter(|r| parent_of.get(r.id.as_str()).is_some_and(|p| *p == h)).map(|r| r.id.as_str()));
                }
            }
        }
    }
    for r in &by_order {
        if !parent_of.contains_key(r.id.as_str()) {
            walk(&r.id, 0, &by_order, &parent_of, folded, &mut seen, &mut out);
        }
    }
    // A loop of parents (a starts b starts a) has no root: whatever is left is shown at the top rather than lost.
    for r in &by_order {
        if !seen.contains(&r.id) {
            walk(&r.id, 0, &by_order, &parent_of, folded, &mut seen, &mut out);
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------ what the view keeps

/// A card being carried.
#[derive(Clone, Debug, PartialEq)]
pub struct Carry {
    pub id: String,
    /// Where it was picked up, and where the pointer is now (the column under it).
    pub from: Stage,
    pub over: Option<Stage>,
    /// Where on the card it was taken, and where the pointer is in the window.
    pub grab: (f32, f32),
    pub at: (f32, f32),
    /// The pointer went past the slop: it is a carry, not a click.
    pub moved: bool,
}

impl App {
    /// The layout All sessions shows.
    pub fn all_layout(&self) -> Layout {
        self.all.layout
    }

    /// Chooses the layout; it is kept for the next run.
    pub fn set_all_layout(&mut self, layout: Layout) {
        if self.all.layout != layout {
            self.all.layout = layout;
            self.all.carry = None;
            self.all.layout_dirty = true;
        }
    }

    /// The columns the board shows: all five, or the one stage the filter names.
    pub fn board_stages(&self) -> Vec<Stage> {
        match self.all.stage {
            Some(s) => vec![s],
            None => Stage::ALL.to_vec(),
        }
    }

    /// The tasks of one column, newest first (the rows of `all_rows` are already in stage order).
    pub fn board_column(&self, stage: Stage) -> Vec<&TaskRow> {
        self.all_rows().into_iter().filter(|r| Stage::of(r.status) == stage).collect()
    }

    /// The column the pointer is over, from where the card was taken and how far the pointer has travelled.
    /// None when the pointer is past the first or last column: a drop there is no drop (the web's "dropped outside a column").
    pub fn column_under(&self, from: Stage, grab_x: f32, dx: f32) -> Option<Stage> {
        let stages = self.board_stages();
        let home = stages.iter().position(|s| *s == from)?;
        let at = ((grab_x + dx) / PITCH).floor() as i32;
        let to = home as i32 + at;
        (0..stages.len() as i32).contains(&to).then(|| stages[to as usize])
    }

    /// A card's pointer gesture (Rune's `on_drag`): Down picks it up, Move carries it, Up drops it unless it was cancelled.
    pub fn board_drag(&mut self, id: &str, phase: Phase, grab: (f32, f32), travel: (f32, f32), window: [f32; 2], cancelled: bool) {
        match phase {
            Phase::Down => {
                self.all.suppress_press = false;
                let Some(from) = self.summary.as_ref().and_then(|m| m.task(id)).map(|t| Stage::of(t.status)) else { return };
                self.all.carry = Some(Carry { id: id.to_owned(), from, over: Some(from), grab, at: (window[0], window[1]), moved: false });
            }
            Phase::Move => {
                let Some(mut carry) = self.all.carry.take() else { return };
                if !carry.moved && travel.0.hypot(travel.1) > DRAG_SLOP {
                    carry.moved = true;
                }
                if carry.moved {
                    carry.over = self.column_under(carry.from, grab.0, travel.0);
                    carry.at = (window[0], window[1]);
                }
                self.all.carry = Some(carry);
            }
            Phase::Up => {
                let Some(carry) = self.all.carry.take() else { return };
                if !carry.moved {
                    return;
                }
                // The press that follows this release is the end of a carry, not a click.
                self.all.suppress_press = true;
                if !cancelled && let Some(over) = self.column_under(carry.from, grab.0, travel.0) {
                    self.board_drop(&carry.id, over);
                }
            }
        }
    }

    /// A card dropped on a column: the daemon's own command, a dialog first for a cancel, or the reason it cannot be, said once.
    pub fn board_drop(&mut self, id: &str, to: Stage) {
        let Some(task) = self.summary.as_ref().and_then(|m| m.task(id)).cloned() else { return };
        match decide(task.status, to) {
            Decision::Same => {}
            Decision::Refuse(reason) => self.show_toast(&reason),
            Decision::Move(Move::Start) => {
                self.call(Call::Start { task_id: id.to_owned() });
                self.show_toast(&format!("Started “{}”. Neko finds its repository, plans, then builds.", task.title));
            }
            Decision::Move(Move::Complete) => {
                self.call(Call::Complete { task_id: id.to_owned() });
                self.show_toast(&format!("Moved “{}” to {}.", task.title, to.title()));
            }
            Decision::Move(Move::Cancel) => self.ask_cancel_by_move(id),
        }
    }

    /// The card's one action (Approve, Answer, Start again).
    pub fn card_act(&mut self, id: &str, action: CardAction) {
        match action {
            CardAction::Approve => {
                let title = self.summary.as_ref().and_then(|m| m.task(id)).map(|t| t.title.clone()).unwrap_or_default();
                self.call(Call::Approve { task_id: id.to_owned() });
                self.show_toast(&format!("Approved “{title}”."));
            }
            CardAction::Answer => self.select(id),
            CardAction::StartAgain => {
                let title = self.summary.as_ref().and_then(|m| m.task(id)).map(|t| t.title.clone()).unwrap_or_default();
                self.call(Call::Start { task_id: id.to_owned() });
                self.show_toast(&format!("Started “{title}”."));
            }
        }
    }

    /// Folds or unfolds a row of the tree.
    pub fn tree_toggle(&mut self, id: &str) {
        if !self.all.tree_folded.remove(id) {
            self.all.tree_folded.insert(id.to_owned());
        }
    }

    /// The rows of the tree under the filters now.
    pub fn tree_visible(&self) -> Vec<TreeRow> {
        let Some(summary) = self.summary.as_ref() else { return Vec::new() };
        let rows = self.all_rows();
        tree_rows(&rows, &summary.parents, &self.all.tree_folded)
    }

    /// The words the card's state line gives a task: "Needs approval", "Ready to review"…
    pub fn state_words(task: &TaskRow) -> &'static str {
        let (words, _) = card_words(task);
        if words.is_empty() { status_label(task.status) } else { words }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::{Summary, TaskMeta};
    use std::collections::BTreeMap;
    use TaskStatus::*;

    fn row(id: &str, status: TaskStatus, at: i64) -> TaskRow {
        TaskRow { id: id.into(), key: None, title: format!("Task {id}"), status, updated_at_ms: at, goal: String::new(), wait: Wait::None, progress: None }
    }

    #[test]
    fn a_drop_is_what_the_web_allows_and_nothing_else() {
        // Same column: nothing.
        assert_eq!(decide(Building, Stage::Running), Decision::Same);
        assert_eq!(decide(Failed, Stage::NeedsYou), Decision::Same, "failed lives in Needs you");
        // The daemon's own commands.
        assert_eq!(decide(Failed, Stage::Running), Decision::Move(Move::Start));
        assert_eq!(decide(AwaitingApproval, Stage::UpNext), Decision::Move(Move::Start));
        assert_eq!(decide(Cancelled, Stage::Running), Decision::Move(Move::Start));
        assert_eq!(decide(ReadyForReview, Stage::Done), Decision::Move(Move::Complete));
        assert_eq!(decide(Building, Stage::Done), Decision::Move(Move::Cancel));
        assert_eq!(decide(AwaitingApproval, Stage::Done), Decision::Move(Move::Cancel));
        assert_eq!(decide(Queued, Stage::Done), Decision::Move(Move::Cancel));
        // Refused, with the web's words.
        assert_eq!(decide(Building, Stage::NeedsYou), Decision::Refuse("Work that has started can’t go back to waiting. Cancel it, or let it finish.".into()));
        assert_eq!(decide(Building, Stage::Review), Decision::Refuse("Neko moves a task to review once its work and checks are done.".into()));
        assert_eq!(decide(ReadyForReview, Stage::Running), Decision::Refuse("Add a note on the task to send it back with changes.".into()));
        assert_eq!(decide(Completed, Stage::Running), Decision::Refuse("Finished work stays finished. Describe a follow-up in Home instead.".into()));
        assert_eq!(decide(Queued, Stage::Running), Decision::Refuse("A queued task starts by itself when a slot is free. Use Run next to put it first.".into()));
        assert_eq!(decide(Failed, Stage::Done), Decision::Refuse("A stopped task needs a decision: start it again, or open it and delete it.".into()));
        assert_eq!(decide(Building, Stage::UpNext), Decision::Refuse("Work that has started can’t go back to the queue. Stop it first.".into()));
        assert_eq!(decide(Failed, Stage::Review), Decision::Refuse("Neko moves a task to review once its work and checks are done.".into()));
    }

    #[test]
    fn a_card_says_where_its_task_stands_and_offers_one_action() {
        let mut r = row("a", AwaitingApproval, 1);
        assert_eq!(card_words(&r), ("Needs approval", Some(CardAction::Approve)));
        r.wait = Wait::Question;
        assert_eq!(card_words(&r), ("Asks you", Some(CardAction::Answer)));
        assert_eq!(card_words(&row("b", Failed, 1)), ("Stopped", Some(CardAction::StartAgain)));
        assert_eq!(card_words(&row("c", ReadyForReview, 1)), ("Ready to review", None));
        assert_eq!(card_words(&row("d", Queued, 1)), ("Up next", None));
        assert_eq!(card_words(&row("e", Building, 1)), ("Preparing a fix", None));
        assert_eq!(card_words(&row("f", Completed, 1)), ("Completed", None));
    }

    fn app() -> App {
        let mut app = App::headless();
        let rows = vec![row("run", Building, 5), row("rev", ReadyForReview, 4), row("fail", Failed, 3), row("wait", AwaitingApproval, 2), row("done", Completed, 1)];
        let mut summary = Summary { tasks: rows, ..Summary::default() };
        for id in ["run", "rev", "fail", "wait", "done"] {
            summary.meta.insert(id.into(), TaskMeta::default());
        }
        app.summary = Some(summary);
        app.all.include_stopped = true;
        app
    }

    #[test]
    fn the_column_under_the_pointer_follows_how_far_it_travelled() {
        let app = app();
        // Taken 40 in on a card of the Running column (third): 292 right is the next column.
        assert_eq!(app.column_under(Stage::Running, 40.0, 0.0), Some(Stage::Running));
        assert_eq!(app.column_under(Stage::Running, 40.0, 292.0), Some(Stage::UpNext));
        assert_eq!(app.column_under(Stage::Running, 40.0, 292.0 * 2.0), Some(Stage::Done));
        assert_eq!(app.column_under(Stage::Running, 40.0, 5000.0), None, "past the last column is no column");
        assert_eq!(app.column_under(Stage::Running, 40.0, -292.0 * 2.0), Some(Stage::NeedsYou));
        assert_eq!(app.column_under(Stage::Running, 40.0, -5000.0), None);
        // With a stage filter there is one column and nothing to move to.
        let mut app = app;
        app.all.stage = Some(Stage::Running);
        assert_eq!(app.column_under(Stage::Running, 40.0, 292.0), None, "one column, nothing to move to");
    }

    #[test]
    fn a_drop_past_the_board_sends_nothing() {
        let mut app = app();
        carry(&mut app, "rev", 5000.0, false);
        carry(&mut app, "fail", -5000.0, false);
        assert!(app.outbox.is_empty() && app.dialog.is_none(), "{:?}", app.outbox);
    }

    fn carry(app: &mut App, id: &str, dx: f32, cancelled: bool) {
        app.board_drag(id, Phase::Down, (40.0, 20.0), (0.0, 0.0), [100.0, 100.0], false);
        app.board_drag(id, Phase::Move, (40.0, 20.0), (dx, 0.0), [100.0 + dx, 100.0], false);
        app.board_drag(id, Phase::Up, (40.0, 20.0), (dx, 0.0), [100.0 + dx, 100.0], cancelled);
    }

    #[test]
    fn an_allowed_drop_sends_the_webs_command_and_a_refused_one_sends_nothing() {
        let mut app = app();
        // A stopped task dropped on Running starts again: `StartTask`.
        carry(&mut app, "fail", 0.0, false);
        // (no travel at all: a click, nothing sent)
        assert!(app.outbox.is_empty());
        // From Needs you, two columns right is Running.
        carry(&mut app, "fail", 292.0 * 2.0, false);
        assert_eq!(app.outbox, vec![Call::Start { task_id: "fail".into() }]);
        app.outbox.clear();
        // A task ready for review dropped on Done is marked done.
        carry(&mut app, "rev", 292.0 * 3.0, false);
        assert_eq!(app.outbox, vec![Call::Complete { task_id: "rev".into() }]);
        app.outbox.clear();
        // A running task dropped on Needs you is refused: nothing is sent, and the reason is said.
        carry(&mut app, "run", -292.0 * 2.0, false);
        assert!(app.outbox.is_empty());
        assert!(app.toast_words().contains("can’t go back to waiting"), "{}", app.toast_words());
        // A cancelled carry sends nothing either.
        carry(&mut app, "fail", 292.0 * 2.0, true);
        assert!(app.outbox.is_empty());
        // Dropped where it started: nothing.
        carry(&mut app, "run", 20.0, false);
        assert!(app.outbox.is_empty());
    }

    #[test]
    fn a_running_task_dropped_on_done_asks_before_it_cancels() {
        let mut app = app();
        carry(&mut app, "run", 292.0 * 2.0, false);
        assert!(app.outbox.is_empty(), "the request waits for the dialog");
        let ask = app.dialog.clone().expect("a dialog is open");
        assert_eq!(ask.call, Call::Stop { task_id: "run".into() });
        assert!(ask.destructive);
        app.ask_yes();
        assert_eq!(app.outbox, vec![Call::Stop { task_id: "run".into() }]);
    }

    #[test]
    fn a_carry_that_ends_is_not_a_click() {
        let mut app = app();
        carry(&mut app, "run", 292.0 * 2.0, false);
        assert!(app.all.suppress_press, "the press that follows the release is swallowed");
        // The next press starts clean.
        app.board_drag("run", Phase::Down, (10.0, 10.0), (0.0, 0.0), [0.0, 0.0], false);
        assert!(!app.all.suppress_press);
        app.board_drag("run", Phase::Up, (10.0, 10.0), (1.0, 0.0), [1.0, 0.0], false);
        assert!(!app.all.suppress_press, "a click that did not travel is a click");
    }

    #[test]
    fn the_tree_puts_children_under_their_parent_and_folds() {
        let rows = [row("p", Building, 5), row("c1", Queued, 4), row("c2", Building, 3), row("g", Queued, 2), row("solo", Completed, 1)];
        let refs: Vec<&TaskRow> = rows.iter().collect();
        let parents: BTreeMap<String, String> = [("c1", "p"), ("c2", "p"), ("g", "c2")].into_iter().map(|(a, b)| (a.to_owned(), b.to_owned())).collect();
        let open = tree_rows(&refs, &parents, &HashSet::new());
        let flat: Vec<(String, usize)> = open.iter().map(|r| (r.id.clone(), r.depth)).collect();
        // Running first (p), its children running first (c2 with g under it, then c1), then the done one.
        assert_eq!(flat, [("p".into(), 0), ("c2".into(), 1), ("g".into(), 2), ("c1".into(), 1), ("solo".into(), 0)]);
        assert_eq!(open[0].children, 2);
        let folded = tree_rows(&refs, &parents, &["p".to_owned()].into_iter().collect());
        assert_eq!(folded.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["p", "solo"]);
        assert!(folded[0].folded);
        // A child whose parent is filtered out is a root; a cycle cannot loop.
        let some: Vec<&TaskRow> = rows.iter().filter(|r| r.id != "p").collect();
        assert_eq!(tree_rows(&some, &parents, &HashSet::new()).iter().filter(|r| r.depth == 0).count(), 3);
        let cycle: BTreeMap<String, String> = [("a", "b"), ("b", "a")].into_iter().map(|(a, b)| (a.to_owned(), b.to_owned())).collect();
        let pair = [row("a", Queued, 1), row("b", Queued, 2)];
        let pair_refs: Vec<&TaskRow> = pair.iter().collect();
        assert_eq!(tree_rows(&pair_refs, &cycle, &HashSet::new()).len(), 2, "both are shown, once");
    }
}
