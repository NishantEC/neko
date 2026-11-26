//! rune-ui registry: segmented_control 0.8.0. Copied into your app by `rune-ui add segmented_control`; yours to change.
//!
//! One choice out of two to five, as a **well** track with a **raised** thumb under the chosen segment. The thumb slides to a new
//! choice on a spring, and a second choice while it is still moving turns it round where it is (springs retarget; nothing waits for
//! the first slide to finish). Under Reduce Motion (`Theme::reduced_motion`) it is a cut: the thumb is at once where it lands. The
//! segments share the track's width equally, so the thumb is always one segment wide.
//!
//! Keys: the control is one Tab stop, landing on the chosen segment. Left and Right choose the previous and next segment (wrapping),
//! Home and End the first and last; the choice follows the keys, as a radio group's does. Use ToggleGroup instead for several choices
//! or icon toggles; this is for switching a view.
//!
//! ```ignore
//! SegmentedControl::new("range", "Range", &["Day", "Week", "Month"], s.range).on_change(|s: &mut App, i| s.range = i).build(t)
//! ```

use rune::{Anim, El, Key, KeyEvent, Spring, Tx, el, row, text};
use rune_ui_style::{Delta, Radius};

use super::base::{Choose, Shape, Size, SizeExt, State, States, Theme, paint, paint_state};

/// The track's inner padding: the thumb sits this far in, at the track's radius minus it.
const PAD: f32 = 3.0;

/// The thumb's slide: short, no bounce.
pub const SLIDE: Spring = Spring::new(0.26, 0.0);

pub struct SegmentedControl<'a, S: 'static> {
    key: String,
    name: String,
    items: &'a [&'a str],
    selected: usize,
    size: Size,
    full: bool,
    disabled: bool,
    pinned: Option<(usize, State)>,
    change: Option<Choose<S>>,
}

/// The segment the keys move to from `at` among `n`: Left and Right wrap, Home and End go to the ends.
pub fn step(key: &Key, at: usize, n: usize) -> Option<usize> {
    if n == 0 {
        return None;
    }
    match key {
        Key::Left => Some((at + n - 1) % n),
        Key::Right => Some((at + 1) % n),
        Key::Home => Some(0),
        Key::End => Some(n - 1),
        _ => None,
    }
}

impl<'a, S: 'static> SegmentedControl<'a, S> {
    pub fn new(key: impl Into<String>, name: impl Into<String>, items: &'a [&'a str], selected: usize) -> SegmentedControl<'a, S> {
        SegmentedControl { key: key.into(), name: name.into(), items, selected, size: Size::Md, full: false, disabled: false, pinned: None, change: None }
    }

    pub fn size(mut self, s: Size) -> Self {
        self.size = s;
        self
    }

    /// As wide as what holds it (otherwise as wide as its longest segment needs, times the count).
    pub fn full(mut self) -> Self {
        self.full = true;
        self
    }

    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    /// Pins a live state on one segment (galleries).
    pub fn state(mut self, i: usize, s: State) -> Self {
        self.pinned = Some((i, s));
        self
    }

    /// The segment chosen, by a press or the keys.
    pub fn on_change(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.change = Some(std::rc::Rc::new(f));
        self
    }

    pub fn segment_key(key: &str, i: usize) -> String {
        format!("{key}-{i}")
    }

    pub fn build(self, t: &Theme) -> El {
        let n = self.items.len();
        let sel = self.selected.min(n.saturating_sub(1));
        let h = self.size.height(t);
        let outer = self.size.radius(t);
        let inner = Radius::inner(outer, PAD);
        let k = self.key.clone();
        let mut track = el().key(k.clone()).role("radiogroup").label(self.name.clone()).focus_group().equal_columns(n.max(1)).h(h).padding(PAD).radius(outer).round_as(t);
        track = paint(track, &t.mat.well.state(if self.disabled { State::Disabled } else { State::Rest }));
        if self.full {
            track = track.w_full();
        }
        // The thumb: the raised material, still (it does not sink; the segment over it takes the press), in the chosen cell.
        let still = t.mat.raised.clone().on(State::Pressed, Delta::new());
        let mut thumb = paint(el().key(format!("{k}-thumb")).grid_cell(sel).w_full().h(h - 2.0 * PAD).radius(inner).round_as(t), &still.base).a11y_hidden();
        thumb = if t.reduced_motion { thumb.frame_motion(Anim::INSTANT) } else { thumb.motion(SLIDE) };
        if n > 0 {
            track = track.child(thumb);
        }
        for (i, label) in self.items.iter().enumerate() {
            let on = i == sel;
            let mut st = States::default();
            if let Some((at, s)) = self.pinned
                && at == i
                && !self.disabled
            {
                st.set(s, true);
            }
            let mut seg = row().key(Self::segment_key(&k, i)).grid_cell(i).role("radio").label((*label).to_owned()).checked(on).focusable(!self.disabled).focus_selected(on).h(h - 2.0 * PAD).px(self.size.pad(t) - 2.0).center().radius(inner).round_as(t);
            // The chosen segment draws nothing of its own (the thumb is under it); the others wash on hover. Neither sinks.
            seg = if on { paint_state(seg, &t.mat.row.clone().on(State::Hover, Delta::new()).on(State::Pressed, Delta::new()), st) } else { paint_state(seg, &t.mat.row.clone().on(State::Pressed, Delta::new()), st) };
            let ink = if on { t.ink() } else { t.ink_muted() };
            seg = seg.child(text((*label).to_owned()).font(self.size.text(t, if on { t.ty.medium } else { t.ty.regular })).color(ink).truncate().min_w(0.0).a11y_hidden());
            if let Some(f) = self.change.clone().filter(|_| !self.disabled) {
                seg = seg.on_press(move |s: &mut S| f(s, i));
            }
            if self.disabled {
                seg = seg.disabled(true);
            }
            track = track.child(seg);
        }
        if self.disabled {
            return track.disabled(true);
        }
        let Some(f) = self.change else { return track };
        track.on_key(move |s: &mut S, ev: &KeyEvent, tx: &mut Tx| {
            if ev.mods.cmd || ev.mods.ctrl || ev.mods.alt {
                return false;
            }
            let at = tx.focused().and_then(|f| f.strip_prefix(&format!("{k}-"))).and_then(|i| i.parse::<usize>().ok()).unwrap_or(sel);
            let Some(to) = step(&ev.key, at, n) else { return false };
            f(s, to);
            tx.focus(Self::segment_key(&k, to), true);
            true
        })
    }
}
