//! rune-ui registry: kbd 0.10.0 Copied into your app by `rune-ui add kbd`; yours to change.
//!
//! Keys as small raised caps: each key in a space-separated combination is its own cap ("⌘ K", "Ctrl Shift P"), in tabular figures.
//!
//! ```ignore
//! Kbd::new("search-keys", "⌘ K").build(t)
//! ```

use rune::{El, TextStyle, row, text};

use super::base::{Shape, Theme, paint};

pub struct Kbd {
    key: String,
    keys: String,
}

impl Kbd {
    pub fn new(key: impl Into<String>, keys: impl Into<String>) -> Kbd {
        Kbd { key: key.into(), keys: keys.into() }
    }

    pub fn build(self, t: &Theme) -> El {
        let mut r = row().key(self.key).role("note").label(self.keys.clone()).gap(t.space.at(0.75)).center_y();
        for k in self.keys.split_whitespace() {
            let cap = row().h(20.0).min_w(20.0).px(t.space.at(1.25)).center().radius(t.radius.sm).round_as(t);
            let recipe = t.mat.raised.base.clone().shadow_none().shadow(rune::Shadow { y: 1.0, color: t.c.line, ..rune::Shadow::default() });
            r = r.child(paint(cap, &recipe).child(text(k.to_owned()).font(TextStyle { size: t.ty.xs.size, line_height: t.ty.xs.line_height, ..t.keycap }).color(t.ink_muted()).a11y_hidden()));
        }
        r
    }
}
