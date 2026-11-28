//! A thread's status mark (docs/knowledge/threads.md, docs/knowledge/design.md): 12 points in a 16 point slot at the left of a row.
//! It replaces the cat's portrait that led every row: a face now means a Cattain.
//!
//! - **Needs you:** an amber dot, over everything else (it is the flag the person acts on).
//! - **Failed:** a red dot, until the thread is opened.
//! - **Working:** a small turning ring segment in the identity teal (a still arc under Reduce Motion).
//! - **Waiting** (on subagents): the same ring held still, in the second ink: the thread is not busy.
//! - **Idle:** nothing; the slot is kept so titles line up.
//!
//! Every colour is taken toward its strong ink until it reaches its contrast against the surface the row sits on. The mark is hidden
//! from assistive technology: the row's description says the state in words ([`crate::threads::state_words`]).

use neko_protocol::{Task, ThreadStatus};
use rune::{Color, El, Shadow, col, el};

use crate::ui::base::{Theme, spinner_in};
use crate::ui::portrait::{DOT_CONTRAST, RING_CONTRAST, seen_on};

/// The slot every row gives its mark, so nothing moves when the state changes.
pub const SLOT: f32 = 16.0;
/// The ring's size.
pub const MARK: f32 = 12.0;
/// How far down the slot the mark is: the title's line, in a 52 point row.
const TOP: f32 = 10.0;
/// The dots' size: lighter than the ring, because a filled disc weighs more than a thin arc.
const DOT: f32 = 8.0;

/// What the mark is, derived from the task by the daemon's fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    NeedsYou,
    Failed,
    Working,
    Waiting,
    Idle,
}

/// The mark of a thread: Needs you, then Failed, then Working, then none (the order the daemon's statuses follow).
pub fn mark_of(task: &Task) -> Mark {
    if task.needs_you {
        Mark::NeedsYou
    } else {
        match task.thread_status {
            ThreadStatus::Failed => Mark::Failed,
            ThreadStatus::Working => Mark::Working,
            ThreadStatus::Waiting => Mark::Waiting,
            ThreadStatus::Idle => Mark::Idle,
        }
    }
}

/// The colour a state is drawn in on `under`: the dots and ring taken toward their strong ink until they show.
pub fn ink(t: &Theme, mark: Mark, under: Color) -> Option<Color> {
    match mark {
        Mark::NeedsYou => Some(seen_on(t.c.warning.solid, t.c.warning.fg, under, DOT_CONTRAST)),
        Mark::Failed => Some(seen_on(t.c.danger.solid, t.c.danger.fg, under, DOT_CONTRAST)),
        Mark::Working => {
            let id = t.identity();
            Some(seen_on(id.solid, id.fg, under, RING_CONTRAST))
        }
        Mark::Waiting => Some(seen_on(t.ink_muted(), t.ink(), under, RING_CONTRAST)),
        Mark::Idle => None,
    }
}

/// The mark in its slot, `key` its own key (unique on the page). `under` is the surface the row sits on.
pub fn status_mark(t: &Theme, key: &str, task: &Task, under: Color) -> El {
    let mark = mark_of(task);
    // The mark sits on the title's line, not between the two lines: its slot is the row's height with the mark `TOP` down.
    let slot = el().key(key.to_owned()).size(SLOT, SLOT).center().a11y_hidden();
    let place = |mark: El| col().w(SLOT).h_full().child(slot.clone().child(mark).mt(TOP));
    match (mark, ink(t, mark, under)) {
        (Mark::Working, Some(c)) => place(spinner_in(t, MARK, c, "").a11y_hidden()),
        (Mark::Waiting, Some(c)) => place(crate::subagents_view::still_ring_in(MARK, c).a11y_hidden()),
        (Mark::NeedsYou | Mark::Failed, Some(c)) => place(el().size(DOT, DOT).radius(DOT / 2.0).fill(c).box_shadow(Shadow { spread: 1.0, color: under, ..Shadow::default() })),
        _ => place(el().size(1.0, 1.0)),
    }
}

/// The mark by itself, centred in a square of `size` (a palette row's lead, which is not 52 points tall): the same drawing as
/// [`status_mark`], nothing in an idle thread's place but the room.
pub fn bare_mark(t: &Theme, key: &str, task: &Task, under: Color, size: f32) -> El {
    let mark = mark_of(task);
    let slot = el().key(key.to_owned()).size(size, size).center().a11y_hidden();
    match (mark, ink(t, mark, under)) {
        (Mark::Working, Some(c)) => slot.child(spinner_in(t, MARK, c, "").a11y_hidden()),
        (Mark::Waiting, Some(c)) => slot.child(crate::subagents_view::still_ring_in(MARK, c).a11y_hidden()),
        (Mark::NeedsYou | Mark::Failed, Some(c)) => slot.child(el().size(DOT, DOT).radius(DOT / 2.0).fill(c).box_shadow(Shadow { spread: 1.0, color: under, ..Shadow::default() })),
        _ => slot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::{TaskId, TaskStatus};

    fn task() -> Task {
        Task::blank(TaskId("t".into()), "default", "w", "t", 1, None)
    }

    #[test]
    fn needs_you_leads_then_failed_then_working() {
        let mut t = task();
        assert_eq!(mark_of(&t), Mark::Idle);
        t.thread_status = ThreadStatus::Working;
        t.status = TaskStatus::Working;
        assert_eq!(mark_of(&t), Mark::Working);
        t.thread_status = ThreadStatus::Failed;
        assert_eq!(mark_of(&t), Mark::Failed);
        t.needs_you = true;
        assert_eq!(mark_of(&t), Mark::NeedsYou, "the amber dot replaces the other marks");
        t.needs_you = false;
        t.thread_status = ThreadStatus::Waiting;
        assert_eq!(mark_of(&t), Mark::Waiting, "no run, its subagents work: the ring, still");
    }

    #[test]
    fn every_mark_shows_against_the_panel_in_both_looks() {
        rune_ui_style::fonts::install(crate::ui::FONTS);
        for dark in [false, true] {
            let t = crate::view::theme(dark, false);
            let under = crate::sidebar::panel_bg(&t);
            for (mark, min) in [(Mark::NeedsYou, DOT_CONTRAST), (Mark::Failed, DOT_CONTRAST), (Mark::Working, RING_CONTRAST), (Mark::Waiting, RING_CONTRAST)] {
                let c = ink(&t, mark, under).expect("a colour");
                assert!(rune_ui_style::contrast(c, under) >= min, "{mark:?} dark={dark}");
            }
            assert_eq!(ink(&t, Mark::Idle, under), None);
        }
    }
}
