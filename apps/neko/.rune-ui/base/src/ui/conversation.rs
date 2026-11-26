//! rune-ui registry: conversation 0.8.0 Copied into your app by `rune-ui add conversation`; yours to change.
//!
//! The scroll area a chat lives in. It stays pinned to the newest content while the reader is at the bottom (Rune's
//! `ScrollSpec::stick_bottom`), lets go as soon as they scroll up, and then shows a raised "Jump to latest" pill over its bottom edge that
//! scrolls back down. The messages sit in a reading column (720 points at most) centred in the area. With no messages it shows an empty
//! state. The app keeps whether the area is at its bottom (`on_at_bottom` reports it, only when it changes).
//!
//! ```ignore
//! Conversation::new("chat", s.at_bottom).children(messages).empty("Ask anything", "Answers, tool calls and files appear here.")
//!     .on_at_bottom(|s: &mut App, at| s.at_bottom = at).build(t)
//! ```

use std::rc::Rc;

use rune::{El, Tx, col, el, row, text};
use rune_ui_style::weight;

use super::base::{Shape, Theme, glyph, hit_area, paint_state};
use super::icons;

/// What a report of being at the bottom does.
pub type Report<S> = Rc<dyn Fn(&mut S, bool)>;

pub struct Conversation<S: 'static> {
    key: String,
    at_bottom: bool,
    body: Vec<El>,
    empty: Option<(String, String)>,
    height: Option<f32>,
    column: f32,
    report: Option<Report<S>>,
}

impl<S: 'static> Conversation<S> {
    /// `at_bottom`: what the area last reported (true until it says otherwise).
    pub fn new(key: impl Into<String>, at_bottom: bool) -> Conversation<S> {
        Conversation { key: key.into(), at_bottom, body: Vec::new(), empty: None, height: None, column: 720.0, report: None }
    }

    pub fn child(mut self, e: El) -> Self {
        self.body.push(e);
        self
    }

    pub fn children(mut self, es: impl IntoIterator<Item = El>) -> Self {
        self.body.extend(es);
        self
    }

    /// What shows while there are no messages: a title and a line under it.
    pub fn empty(mut self, title: impl Into<String>, description: impl Into<String>) -> Self {
        self.empty = Some((title.into(), description.into()));
        self
    }

    /// A fixed height (otherwise it fills what holds it).
    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(h);
        self
    }

    /// The reading column's widest (720).
    pub fn column(mut self, w: f32) -> Self {
        self.column = w;
        self
    }

    /// Whether the area shows its bottom: once after it is first laid out, then whenever that changes.
    pub fn on_at_bottom(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.report = Some(Rc::new(f));
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let scroll_key = format!("{}-scroll", self.key);
        let empty = self.body.is_empty();
        let content = if empty {
            let (title, words) = self.empty.clone().unwrap_or_else(|| ("No messages yet".to_owned(), String::new()));
            let mut e = col().key(format!("{}-empty", self.key)).role("status").label(title.clone()).w_full().min_h(240.0).gap(t.space.at(2.0)).center().px(t.metrics.inset)
                .child(glyph(&icons::SPARKLE, t.control.icon_lg + 4.0, t.ink_faint()))
                .child(text(title).font(weight(t.ty.lg, t.ty.medium)).color(t.ink()).a11y_hidden());
            if !words.is_empty() {
                e = e.child(text(words).font(t.ty.base).color(t.ink_muted()).wrap().max_w(360.0).a11y_hidden());
            }
            e
        } else {
            col().key(format!("{}-content", self.key)).w_full().max_w(self.column).px(t.metrics.inset).py(t.space.at(6.0)).gap(t.space.at(6.0)).children(self.body)
        };
        let mut scroll = col().key(scroll_key.clone()).role("log").label("Conversation").w_full().h_full().items_center().scroll(rune::ScrollSpec { stick_bottom: true }).scroll_edge_fade(24.0).child(content);
        if let Some(f) = self.report {
            scroll = scroll.on_at_bottom(move |s: &mut S, at: bool| f(s, at));
        }
        let mut frame = el().key(self.key.clone()).w_full().child(scroll);
        frame = match self.height {
            Some(h) => frame.h(h),
            None => frame.h_full(),
        };
        if !self.at_bottom && !empty {
            let h = t.control.sm;
            let (target, again) = (scroll_key.clone(), scroll_key.clone());
            let face = paint_state(row().key(format!("{}-jump", self.key)).role("button").label("Jump to latest").focusable(true).h(h).pl(t.space.at(2.5)).pr(t.space.at(3.0)).gap(t.space.at(1.5)).center().radius(h / 2.0), &t.mat.raised, Default::default())
                .child(glyph(&icons::ARROW_DOWN, t.control.icon_sm, t.ink_muted()))
                .child(text("Jump to latest").font(weight(t.ty.sm, t.ty.medium)).color(t.ink()).a11y_hidden())
                .on_press_tx(move |_: &mut S, tx: &mut Tx| tx.scroll_to_bottom(target.clone()));
            let hit = hit_area::<S>(&format!("{}-jump", self.key), face, 0.0, t.control.slop(h), None).on_press_tx(move |_: &mut S, tx: &mut Tx| tx.scroll_to_bottom(again.clone()));
            frame = frame.child(row().pin(Some(0.0), None, Some(0.0), Some(t.space.at(4.0))).justify_center().child(hit));
        }
        frame.round_as(t)
    }
}
