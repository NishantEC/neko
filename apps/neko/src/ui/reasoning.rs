//! rune-ui registry: reasoning 0.6.0. Copied into your app by `rune-ui add reasoning`; yours to change.
//!
//! The model's thinking, kept out of the way. While it thinks, a quiet line says "Thinking…" with a shimmer; afterwards it says "Thought
//! for 12s" and folds open to the thoughts, in the second ink, indented under the line.
//!
//! ```ignore
//! Reasoning::new("r1", s.thinking, s.thought_secs, s.open("r1")).text(&thoughts).on_toggle(|s: &mut App| s.flip("r1")).build(t)
//! ```

use std::rc::Rc;

use rune::{El, text};

use super::agent::{FoldLine, elapsed};
use super::base::{Press, Theme, glyph};
use super::icons;
use super::shimmer::Shimmer;

pub struct Reasoning<S: 'static> {
    key: String,
    streaming: bool,
    secs: f64,
    open: bool,
    words: String,
    toggle: Option<Press<S>>,
}

impl<S: 'static> Reasoning<S> {
    /// `secs`: how long it thought (or has thought so far).
    pub fn new(key: impl Into<String>, streaming: bool, secs: f64, open: bool) -> Reasoning<S> {
        Reasoning { key: key.into(), streaming, secs, open, words: String::new(), toggle: None }
    }

    pub fn text(mut self, w: impl Into<String>) -> Self {
        self.words = w.into();
        self
    }

    pub fn on_toggle(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.toggle = Some(Rc::new(f));
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let label = if self.streaming { "Thinking…".to_owned() } else { format!("Thought for {}", elapsed(self.secs.round())) };
        let mut line = FoldLine::<S>::new(self.key.clone(), label.clone(), self.open).lead(glyph(&icons::SPARKLE, t.control.icon, t.ink_faint()));
        if self.streaming {
            line = line.words(Shimmer::new(format!("{}-shimmer", self.key), label).build(t));
        }
        if let Some(f) = self.toggle {
            line = line.on_toggle(move |s: &mut S| f(s));
        }
        let body = if self.words.is_empty() { Vec::new() } else { vec![text(self.words).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0)] };
        line.build(t, body)
    }
}
