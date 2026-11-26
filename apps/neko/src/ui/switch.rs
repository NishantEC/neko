//! rune-ui registry: switch 0.5.0. Copied into your app by `rune-ui add switch`; yours to change.
//!
//! On or off, at once. The track is a **well** while off and fills with **ink** when on; the thumb is a small floating disc (the
//! **thumb** material off, the ink's words colour on). Radii are concentric. With words the whole line is the control; alone, the switch
//! tops up to the hit size. The flip lands in the frame it was pressed.
//!
//! ```ignore
//! Switch::new("sync", "Sync in the background", s.sync).on_toggle(|s: &mut App| s.sync = !s.sync).build(t)
//! ```

use rune::{El, el, row};

use super::base::{Delta, Material, Press, Recipe, Shape, State, Theme, hit_area, paint, paint_state, press, resolve, states, words};

const TRACK: (f32, f32) = (36.0, 22.0);
const PAD: f32 = 3.0;

pub struct Switch<S: 'static> {
    key: String,
    label: String,
    on: bool,
    show_label: bool,
    help: Option<String>,
    disabled: bool,
    pinned: Option<State>,
    press: Option<Press<S>>,
}

impl<S: 'static> Switch<S> {
    /// `label` shows at the line's start and is the switch's name.
    pub fn new(key: impl Into<String>, label: impl Into<String>, on: bool) -> Switch<S> {
        Switch { key: key.into(), label: label.into(), on, show_label: false, help: None, disabled: false, pinned: None, press: None }
    }

    /// Words at its left; the switch sits at the end of the line.
    pub fn show_label(mut self) -> Self {
        self.show_label = true;
        self
    }

    pub fn help(mut self, h: impl Into<String>) -> Self {
        self.help = Some(h.into());
        self.show_label = true;
        self
    }

    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    pub fn state(mut self, s: State) -> Self {
        self.pinned = Some(s);
        self
    }

    pub fn on_toggle(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.press = Some(press(f));
        self
    }

    fn track(&self, t: &Theme) -> El {
        let d = TRACK.1 - 2.0 * PAD;
        let disc = el().size(d, d).radius(d / 2.0);
        let thumb = if self.on { paint(disc, &Recipe::new().fill(t.c.primary.fg).shadows(&t.elevation.xs)) } else { paint(disc, &t.mat.thumb.base) }.a11y_hidden();
        let m = if self.on { &t.mat.ink } else { &t.mat.well };
        let track = paint(row().size(TRACK.0, TRACK.1).padding(PAD).center_y().radius(TRACK.1 / 2.0), &resolve(m, Default::default()).shadow_none()).child(thumb).a11y_hidden();
        if self.on { track.justify_end() } else { track.justify_start() }
    }

    pub fn build(self, t: &Theme) -> El {
        let live = !self.disabled;
        let st = states(self.pinned, self.disabled, false);
        let mut e = row().key(self.key.clone()).role("switch").label(self.label.clone()).checked(self.on).focusable(live).center_y().radius(t.radius.md).round_as(t);
        if let Some(h) = &self.help {
            e = e.description(h.clone());
        }
        let track = self.track(t);
        e = if self.show_label { e.w_full().min_h(t.control.md).px(t.space.at(1.0)).mx(-t.space.at(1.0)).gap(t.space.at(3.0)).child(words(t, &self.label, self.help.as_deref())).child(rune::spacer()).child(track) } else { e.child(track) };
        // A line washes like a row; a bare switch only rings. Neither sinks: the thumb moving is the answer.
        let line = if self.show_label { t.mat.row.clone() } else { Material::new(Recipe::new()).on(State::FocusVisible, t.mat.ghost.focus.clone()).on(State::Disabled, Delta::new().opacity(t.control.disabled)) };
        e = paint_state(e, &line, st);
        if !live {
            e = e.disabled(true);
        }
        let press = self.press.filter(|_| live);
        if let Some(f) = press.clone() {
            e = e.on_press(move |s: &mut S| f(s));
        }
        if self.show_label { e } else { hit_area(&self.key, e, t.control.slop(TRACK.0), t.control.slop(TRACK.1), press) }
    }
}
