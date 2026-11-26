//! rune-ui registry: reorder 0.10.0. Copied into your app by `rune-ui add reorder`; yours to change.
//!
//! Makes a column of rows something the person can put in order, by pointer and by keyboard, without owning the rows: you build them
//! (a `SidebarItem`, an `Item`, your own), hand them over in order, and `on_move(from, to)` tells you what was asked. The list is
//! yours: move the item in your state (`move_item` does it) and build again.
//!
//! * **Keyboard**: with focus on a row (or anything in it), Alt+Up and Alt+Down ask to move it one place, Alt+Home and Alt+End to the
//!   ends. Focus stays on the row; give the row the same key and it keeps focus through the move.
//! * **Pointer**: press and drag a row; on release it is asked to go where it was dropped (the rows' `pitch` says how far a place
//!   is). Escape during the drag cancels it. A press that does not move is still a press for the row.
//! * **Words**: the column is a `list` named `name`, each row a `listitem` with its place ("2 of 4") and a description saying how
//!   to move it, so nothing depends on the pointer.
//!
//! ```ignore
//! Reorder::new("workspaces", "Workspaces", 36.0).rows(s.names.iter().map(|n| row_for(n)).collect())
//!     .on_move(|s: &mut App, from, to| reorder::move_item(&mut s.names, from, to)).build(t)
//! ```

use std::rc::Rc;

use rune::{El, Key, KeyEvent, Phase, Tx, col};

use super::base::Theme;

/// Moves the item at `from` to the place `to` (both clamped into the list), the others shifting to make room.
pub fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) {
    if items.is_empty() {
        return;
    }
    let (from, to) = (from.min(items.len() - 1), to.min(items.len() - 1));
    if from != to {
        let it = items.remove(from);
        items.insert(to, it);
    }
}

/// Where a row of place `from` lands when dragged `dy` points down (negative: up) in a column whose rows are `pitch` points apart.
pub fn drop_index(from: usize, dy: f32, pitch: f32, len: usize) -> usize {
    if len == 0 || pitch <= 0.0 {
        return from;
    }
    let places = (dy / pitch).round() as i64;
    (from as i64 + places).clamp(0, len as i64 - 1) as usize
}

/// The place a key asks for, if it asks for one: Alt+Up and Alt+Down one place, Alt+Home and Alt+End the ends.
pub fn key_target(k: &KeyEvent, from: usize, len: usize) -> Option<usize> {
    if !k.mods.alt || k.mods.cmd || k.mods.ctrl || len == 0 {
        return None;
    }
    let to = match k.key {
        Key::Up => from.saturating_sub(1),
        Key::Down => (from + 1).min(len - 1),
        Key::Home => 0,
        Key::End => len - 1,
        _ => return None,
    };
    (to != from).then_some(to)
}

type Moved<S> = Rc<dyn Fn(&mut S, usize, usize)>;

pub struct Reorder<S: 'static> {
    key: String,
    name: String,
    pitch: f32,
    rows: Vec<El>,
    on_move: Option<Moved<S>>,
}

impl<S: 'static> Reorder<S> {
    /// `name` names the list for assistive technology; `pitch` is the distance from one row's top to the next, in points.
    pub fn new(key: impl Into<String>, name: impl Into<String>, pitch: f32) -> Reorder<S> {
        Reorder { key: key.into(), name: name.into(), pitch, rows: Vec::new(), on_move: None }
    }

    /// The rows, in order. Each is kept as built; the wrapper around it is keyed `<key>-row<N>`.
    pub fn rows(mut self, rows: Vec<El>) -> Self {
        self.rows = rows;
        self
    }

    /// What a request to move does: the place it was at and the place it was asked to go to (both 0-based).
    pub fn on_move(mut self, f: impl Fn(&mut S, usize, usize) + 'static) -> Self {
        self.on_move = Some(Rc::new(f));
        self
    }

    pub fn build(self, _t: &Theme) -> El {
        let n = self.rows.len();
        let mut list = col().key(self.key.clone()).role("list").label(self.name.clone()).w_full();
        for (i, row) in self.rows.into_iter().enumerate() {
            // The handlers go on the row itself: a press lands on the row (its own press, hover and focus), and a drag or a key
            // is looked for there first.
            let mut row = row;
            if let Some(f) = self.on_move.clone() {
                let g = f.clone();
                let pitch = self.pitch;
                row = row
                    .on_key(move |s: &mut S, k: &KeyEvent, _: &mut Tx| match key_target(k, i, n) {
                        Some(to) => {
                            f(s, i, to);
                            true
                        }
                        None => false,
                    })
                    .on_drag(move |s: &mut S, d: &mut rune::Drag, _: &mut Tx| {
                        if d.phase == Phase::Up && !d.cancelled() {
                            let to = drop_index(i, d.dy, pitch, n);
                            if to != i {
                                g(s, i, to);
                            }
                        }
                    });
            }
            list = list.child(col().key(format!("{}-row{i}", self.key)).role("listitem").label(format!("{} of {n}", i + 1)).description("Alt with Up or Down moves it").w_full().child(row));
        }
        list
    }
}
