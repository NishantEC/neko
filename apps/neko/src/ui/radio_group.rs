//! rune-ui registry: radio_group 0.9.0. Copied into your app by `rune-ui add radio_group`; yours to change.
//!
//! One of a few, all shown. Each option is a ring (a small **well**) that fills with an **ink** dot when chosen, its words beside it,
//! the whole line pressable. Lines sit far enough apart to keep a 40 point pitch. Stacked, each line is as wide as the group and an
//! option's description wraps to that width (in a row of options, at 320 points).
//!
//! ```ignore
//! RadioGroup::new("level", "Starting level", &[("Careful", None), ("Balanced", Some("The default"))], s.level).on_change(|s: &mut App, i| s.level = i).build(t)
//! ```

use rune::{El, col, el, row, text};

use super::base::{Choose, Shape, State, States, Theme, paint, paint_state, recipes, resolve, words};

/// An option's words when the options are stacked: the label on one line, the description wrapping to the line's width (never
/// narrower than [`MIN_HELP`], so a group in a content-sized box still reads in lines, not words).
fn stacked_words(t: &Theme, label: &str, help: Option<&str>) -> El {
    let mut c = col().grow(1.0).shrink().gap(1.0).child(text(label.to_owned()).font(t.ty.base).color(t.ink()).a11y_hidden());
    if let Some(h) = help {
        c = c.child(text(h.to_owned()).font(t.ty.sm).color(t.ink_muted()).wrap().min_w(MIN_HELP).a11y_hidden());
    }
    c
}

/// The narrowest a stacked option's description wraps to.
const MIN_HELP: f32 = 240.0;

const RING: f32 = 16.0;

pub struct RadioGroup<'a, S: 'static> {
    key: String,
    name: String,
    options: &'a [(&'a str, Option<&'a str>)],
    selected: Option<usize>,
    horizontal: bool,
    disabled: bool,
    pinned: Option<(usize, State)>,
    change: Option<Choose<S>>,
}

impl<'a, S: 'static> RadioGroup<'a, S> {
    pub fn new(key: impl Into<String>, name: impl Into<String>, options: &'a [(&'a str, Option<&'a str>)], selected: Option<usize>) -> RadioGroup<'a, S> {
        RadioGroup { key: key.into(), name: name.into(), options, selected, horizontal: false, disabled: false, pinned: None, change: None }
    }

    pub fn horizontal(mut self) -> Self {
        self.horizontal = true;
        self
    }

    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    pub fn state(mut self, i: usize, s: State) -> Self {
        self.pinned = Some((i, s));
        self
    }

    pub fn on_change(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.change = Some(std::rc::Rc::new(f));
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let mut g = if self.horizontal { row().gap(t.space.at(5.0)) } else { col().gap(t.metrics.stack / 2.0) }.key(self.key.clone()).role("radiogroup").label(self.name.clone());
        for (i, (label, help)) in self.options.iter().enumerate() {
            let on = self.selected == Some(i);
            let mut st = States { disabled: self.disabled, ..States::default() };
            if let Some((at, s)) = self.pinned
                && at == i
            {
                st.set(s, true);
            }
            let mut ring = row().size(RING, RING).center().radius(RING / 2.0).a11y_hidden();
            ring = paint(ring, &resolve(&recipes::check(t, on), States::default()).without(rune_ui_style::LayerKind::Scale));
            if on {
                ring = ring.child(el().size(6.0, 6.0).radius(3.0).fill(t.c.primary.fg));
            }
            let mut line = row().key(format!("{}-{i}", self.key)).role("radio").label((*label).to_owned()).checked(on).focusable(!self.disabled).min_h(t.control.md).px(t.space.at(1.0)).ml(-t.space.at(1.0)).gap(t.space.at(2.0)).center_y().radius(t.radius.md).round_as(t).child(ring).child(if self.horizontal { words(t, label, *help) } else { stacked_words(t, label, *help) });
            if let Some(h) = help {
                line = line.description((*h).to_owned());
            }
            line = paint_state(line, &t.mat.row, States { disabled: false, ..st });
            if self.disabled {
                line = line.disabled(true).opacity(t.control.disabled);
            } else if let Some(f) = self.change.clone() {
                line = line.on_press(move |s: &mut S| f(s, i));
            }
            g = g.child(line);
        }
        g
    }
}
