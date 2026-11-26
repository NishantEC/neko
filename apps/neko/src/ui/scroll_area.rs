//! rune-ui registry: scroll_area 0.5.0. Copied into your app by `rune-ui add scroll_area`; yours to change.
//!
//! A region that scrolls inside a fixed height, clipped to its corners, its top and bottom edges fading so a cut-off row reads as one
//! that scrolls. Scrolling (wheel, trackpad momentum, keys, scroll bars as the system shows them) is Rune's.
//!
//! ```ignore
//! ScrollArea::new("log").height(240.0).child(rows).build(t)
//! ```

use rune::{El, col};

use super::base::{Shape, Theme};

pub struct ScrollArea {
    key: String,
    height: f32,
    width: Option<f32>,
    fade: bool,
    children: Vec<El>,
}

impl ScrollArea {
    pub fn new(key: impl Into<String>) -> ScrollArea {
        ScrollArea { key: key.into(), height: 240.0, width: None, fade: true, children: Vec::new() }
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    /// No fading edges.
    pub fn no_fade(mut self) -> Self {
        self.fade = false;
        self
    }

    pub fn child(mut self, c: El) -> Self {
        self.children.push(c);
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let inner = col().w_full().children(self.children);
        let s = col().key(self.key).w_full().h(self.height).radius(t.radius.lg).round_as(t).scroll(rune::ScrollSpec { stick_bottom: false }).child(inner);
        let f = t.space.at(4.0);
        let s = if self.fade { s.clip_fade(0.0, f, 0.0, f) } else { s.clip_rounded(t.radius.lg) };
        match self.width {
            Some(w) => s.w(w),
            None => s,
        }
    }
}
