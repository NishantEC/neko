//! rune-ui registry: tooltip 0.5.0. Copied into your app by `rune-ui add tooltip`; yours to change.
//!
//! A short label in ink on hover or keyboard focus. Behaviour is Rune's own help tooltip (`El::help`: the delay, the skip-delay between
//! neighbours, placement, the fade, accessibility); the look is ours, installed once per frame with `cx.help_look(tooltip::look(t))`.
//! `Tooltip::panel` draws one in place (galleries).
//!
//! ```ignore
//! cx.help_look(tooltip::look(t));
//! Tooltip::new("Save").keys("⌘S").wrap(button)
//! ```

use rune::{El, HelpLook, Shadow, row, text};

use super::base::{Shape, Theme, paint};

/// The app's tooltip look from the theme: the ink, its words colour, a small radius, the floating shadows.
pub fn look(t: &Theme) -> HelpLook {
    let shadows: Vec<Shadow> = t.elevation.sm.clone();
    HelpLook::system(t.dark).fill(t.c.primary.bg).ink(t.c.primary.fg).radius(t.radius.sm + 1.0).padding(t.space.at(2.0), t.space.at(1.25)).font(t.ty.sm).max_width(Some(280.0)).shadows(shadows).delay(0.45).skip_delay(0.3)
}

pub struct Tooltip {
    words: String,
    keys: Option<String>,
}

impl Tooltip {
    pub fn new(words: impl Into<String>) -> Tooltip {
        Tooltip { words: words.into(), keys: None }
    }

    /// The shortcut that does the same thing, shown after the words.
    pub fn keys(mut self, k: impl Into<String>) -> Self {
        self.keys = Some(k.into());
        self
    }

    /// `trigger` with this tooltip.
    pub fn wrap(self, trigger: El) -> El {
        let words = match self.keys {
            Some(k) => format!("{}  {k}", self.words),
            None => self.words,
        };
        trigger.help(words)
    }

    /// One drawn in place.
    pub fn panel(self, t: &Theme, key: impl Into<String>) -> El {
        let mut r = paint(row().key(key.into()).role("tooltip").label(self.words.clone()).h(26.0).px(t.space.at(2.0)).gap(t.space.at(2.0)).center_y().radius(t.radius.sm + 1.0).round_as(t), &rune_ui_style::Recipe::new().fill(t.c.primary.bg).shadows(&t.elevation.sm));
        r = r.child(text(self.words).font(t.ty.sm).color(t.c.primary.fg).a11y_hidden());
        if let Some(k) = self.keys {
            r = r.child(text(k).font(rune_ui_style::tabular(t.ty.sm)).color(t.c.primary.fg.alpha(0.7)).a11y_hidden());
        }
        r
    }
}
