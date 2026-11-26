//! rune-ui registry: reasoning 0.9.0. Copied into your app by `rune-ui add reasoning`; yours to change.
//!
//! The model's thinking, kept out of the way. While it thinks, a quiet line says "Thinking…" with a shimmer (its words held still under
//! Reduce Motion); afterwards it says "Thought for 12s" ("Thought" under a second) and folds open to the thoughts, in the second ink,
//! indented under the line. Nothing to show means nothing shown: a finished thought with no words draws nothing and takes no room.
//!
//! ```ignore
//! Reasoning::new("r1", s.thinking, s.thought_secs, s.open("r1")).text(&thoughts).on_toggle(|s: &mut App| s.flip("r1")).build(t)
//! ```

use std::rc::Rc;

use rune::{El, el, text};

use super::agent::{FoldLine, elapsed, took};
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
        if !self.streaming && self.words.trim().is_empty() {
            // Out of the flow and no size: no height, and no gap in the column it sits in.
            return el().key(self.key).absolute(0.0, 0.0).size(0.0, 0.0).a11y_hidden();
        }
        let label = if self.streaming {
            "Thinking…".to_owned()
        } else {
            took(self.secs).map_or_else(|| "Thought".to_owned(), |_| format!("Thought for {}", elapsed(self.secs.round())))
        };
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
