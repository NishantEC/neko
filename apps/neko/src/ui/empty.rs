//! rune-ui registry: empty 0.5.0. Copied into your app by `rune-ui add empty`; yours to change.
//!
//! Nothing here yet: an icon in a small well, what is missing (the heading), why or what to expect, and the one thing to do.
//!
//! ```ignore
//! Empty::new("no-tasks", &icons::INBOX, "No tasks yet").description("Tasks you start show up here.").action(button).build(t)
//! ```

use rune::{El, Icon, col, row, text};
use rune_ui_style::weight;

use super::base::{Shape, Theme, glyph, paint};

pub struct Empty {
    key: String,
    icon: &'static Icon,
    title: String,
    description: Option<String>,
    action: Option<El>,
}

impl Empty {
    pub fn new(key: impl Into<String>, icon: &'static Icon, title: impl Into<String>) -> Empty {
        Empty { key: key.into(), icon, title: title.into(), description: None, action: None }
    }

    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }

    pub fn action(mut self, a: El) -> Self {
        self.action = Some(a);
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let well = paint(row().size(40.0, 40.0).center().radius(t.radius.lg).round_as(t), &t.mat.well.base).child(glyph(self.icon, 20.0, t.ink_muted()));
        let mut c = col().key(self.key).w_full().py(t.space.at(8.0)).px(t.space.at(4.0)).gap(t.space.at(2.0)).items_center().child(well.mb(t.space.at(1.0))).child(text(self.title).font(weight(t.ty.md, t.ty.medium)).color(t.ink()).role("heading").level(3));
        if let Some(d) = self.description {
            c = c.child(text(d).font(t.ty.base).color(t.ink_muted()).wrap().max_w(340.0).min_w(0.0).align(rune::Align::Center));
        }
        if let Some(a) = self.action {
            c = c.child(row().pt(t.space.at(2.0)).child(a));
        }
        c
    }
}
