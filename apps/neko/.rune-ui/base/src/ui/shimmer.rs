//! rune-ui registry: shimmer 0.8.0 Copied into your app by `rune-ui add shimmer`; yours to change.
//!
//! Two quiet signs of work. [`Shimmer`] is words ("Working…") with a soft band of the page's colour sweeping across them; [`Loader`] is
//! three small dots that brighten in turn. Both are the only motion an agent screen needs while it waits, and both stop under Reduce
//! Motion: Rune holds a sweep at its midpoint, which here is just past the words' right edge (the sweep runs from one width before the
//! words to three after, so it also rests between passes), and the dots hold still. A theme with `reduced_motion` set draws them
//! still from the start: the words with no band, the dots at their middle brightness; both still say they are busy.
//!
//! ```ignore
//! Shimmer::new("working", "Working…").build(t)
//! Loader::new("Waiting for the model").build(t)
//! ```

use std::f32::consts::FRAC_PI_2;

use rune::{Color, Cycle, El, Easing, LoopSpec, TextStyle, el, row, text};

use super::base::Theme;

pub struct Shimmer {
    key: String,
    words: String,
    style: Option<TextStyle>,
    color: Option<Color>,
    on: bool,
}

impl Shimmer {
    pub fn new(key: impl Into<String>, words: impl Into<String>) -> Shimmer {
        Shimmer { key: key.into(), words: words.into(), style: None, color: None, on: true }
    }

    /// The words' type (the theme's base by default).
    pub fn font(mut self, s: TextStyle) -> Self {
        self.style = Some(s);
        self
    }

    /// The words' ink (the second ink by default).
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }

    /// Whether the band sweeps (off: plain words).
    pub fn on(mut self, on: bool) -> Self {
        self.on = on;
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let words = text(self.words.clone()).font(self.style.unwrap_or(t.ty.base)).color(self.color.unwrap_or(t.ink_muted())).a11y_hidden();
        let mut r = row().key(self.key).role("status").label(self.words).busy(self.on).clip_rounded(0.0).child(words);
        if self.on && !t.reduced_motion {
            let bg = t.c.page.bg;
            let band = el().absolute(0.0, 0.0).w_full().h_full().a11y_hidden()
                .linear_gradient(FRAC_PI_2, [(0.0, bg.alpha(0.0)), (0.5, bg.alpha(0.78)), (1.0, bg.alpha(0.0))])
                .looping(LoopSpec::slide(2.4, [-1.0, 0.0], [3.0, 0.0]).easing(Easing::LINEAR));
            r = r.child(band);
        }
        r
    }
}

/// Three dots that brighten in turn: a wait with no words.
pub struct Loader {
    label: String,
    size: f32,
}

impl Loader {
    /// `label` names the wait for assistive technology.
    pub fn new(label: impl Into<String>) -> Loader {
        Loader { label: label.into(), size: 4.0 }
    }

    /// Each dot's diameter (4).
    pub fn size(mut self, d: f32) -> Self {
        self.size = d;
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let d = self.size;
        let mut r = row().role("progressbar").label(self.label).busy(true).gap(d * 0.75).center_y().h(d * 3.0);
        for k in 0..3 {
            let dot = el().size(d, d).radius(d / 2.0).fill(t.ink_muted()).a11y_hidden();
            r = r.child(if t.reduced_motion { dot.opacity(0.6) } else { dot.cycle(Cycle { period: 1.2, phase: k as f32 / 3.0, stops: &[(0.0, 0.25), (0.4, 1.0), (0.8, 0.25), (1.0, 0.25)], still: 0.6, fps: 30.0 }) });
        }
        r
    }
}
