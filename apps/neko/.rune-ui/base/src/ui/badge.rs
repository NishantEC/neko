//! rune-ui registry: badge 0.5.0. Copied into your app by `rune-ui add badge`; yours to change.
//!
//! A small label. shadcn's variants (**Default** ink, **Secondary** quiet tint, **Outline**, **Destructive** solid danger) and a meaning
//! at low alpha ([`Badge::tone`]): its wash behind words in its own ink, never colour alone. A leading dot makes it a status pill (hollow
//! for off or a draft); figures are tabular, so it also serves as a count.
//!
//! ```ignore
//! Badge::new("state", "Ready").tone(Tone::Success).dot().build(t)
//! Badge::new("unread", "3").build(t)
//! ```

use rune::{Color, El, el, row, text};
use rune_ui_style::{Recipe, tabular, weight};

use super::base::{Theme, Tone, paint};

/// shadcn's badge variants.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeVariant {
    #[default]
    Default,
    Secondary,
    Outline,
    Destructive,
}

/// The dot before the words.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dot {
    #[default]
    None,
    Filled,
    Hollow,
}

pub struct Badge {
    key: String,
    words: String,
    variant: BadgeVariant,
    tone: Option<Tone>,
    dot: Dot,
}

impl Badge {
    pub fn new(key: impl Into<String>, words: impl Into<String>) -> Badge {
        Badge { key: key.into(), words: words.into(), variant: BadgeVariant::Secondary, tone: None, dot: Dot::None }
    }

    pub fn variant(mut self, v: BadgeVariant) -> Self {
        self.variant = v;
        self.tone = None;
        self
    }

    /// A meaning: its wash and its words.
    pub fn tone(mut self, t: Tone) -> Self {
        self.tone = Some(t);
        self
    }

    pub fn dot(mut self) -> Self {
        self.dot = Dot::Filled;
        self
    }

    pub fn hollow(mut self) -> Self {
        self.dot = Dot::Hollow;
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let (fill, ink, dot, rim) = match (self.tone, self.variant) {
            (Some(tone), _) => {
                let (wash, words, solid, _) = tone.colors(t);
                (wash, words, solid, None)
            }
            (None, BadgeVariant::Default) => (t.c.primary.bg, t.c.primary.fg, t.c.primary.fg, None),
            (None, BadgeVariant::Secondary) => (t.c.muted.bg, t.c.muted.fg, t.ink_faint(), None),
            (None, BadgeVariant::Outline) => (Color::TRANSPARENT, t.ink(), t.ink_faint(), Some(t.c.line.alpha((t.c.line.a * 2.0).min(1.0)))),
            (None, BadgeVariant::Destructive) => (t.c.danger.solid, t.c.danger.on_solid, t.c.danger.on_solid, None),
        };
        let mut recipe = Recipe::new().fill(fill);
        if let Some(r) = rim {
            recipe = recipe.inner_rim(r);
        }
        let mut b = paint(row().key(self.key).role("status").label(self.words.clone()).h(20.0).min_w(20.0).pl(t.space.at(if self.dot == Dot::None { 2.0 } else { 1.75 })).pr(t.space.at(2.0)).gap(t.space.at(1.25)).center().radius(10.0), &recipe);
        match self.dot {
            Dot::Filled => b = b.child(el().size(6.0, 6.0).radius(3.0).fill(dot).a11y_hidden()),
            Dot::Hollow => b = b.child(el().size(6.0, 6.0).radius(3.0).border(1.5, dot).a11y_hidden()),
            Dot::None => {}
        }
        b.child(text(self.words).font(tabular(weight(t.ty.sm, t.ty.medium))).color(ink).a11y_hidden())
    }
}
