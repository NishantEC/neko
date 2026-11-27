//! rune-ui registry: kbd 0.11.0 Copied into your app by `rune-ui add kbd`; yours to change. Needs: icons.
//!
//! Keys as small raised caps: each key in a space-separated combination is its own cap ("⌘ K", "Ctrl Shift P"), in tabular figures
//! in the theme's keycap face (`Theme::keycap`, `base.keys-font`).
//!
//! **Modifier glyphs.** ⌘ ⌥ ⇧ ⌃ and ↩ are drawn as small icons (the icon set's `KEY_*` glyphs) instead of text, so they look the
//! same in every font, including the ones that do not carry them. They may be spaced ("⇧ ⌘ P") or run together ("⇧⌘P"); each
//! gets its own cap. Words ("Cmd", "Ctrl", "Enter") stay words in the keycap face. Assistive technology reads the whole chord with
//! the glyphs spoken ("Shift Command P").
//!
//! ```ignore
//! Kbd::new("search-keys", "⌘ K").build(t)
//! ```

use rune::{El, Icon, TextStyle, row, text};

use super::base::{Shape, Theme, glyph, paint};
use super::icons;

pub struct Kbd {
    key: String,
    keys: String,
}

/// The icon and spoken name of a modifier glyph, or none for any other character.
pub fn modifier(c: char) -> Option<(&'static Icon, &'static str)> {
    match c {
        '⌘' => Some((&icons::KEY_COMMAND, "Command")),
        '⌥' => Some((&icons::KEY_OPTION, "Option")),
        '⇧' => Some((&icons::KEY_SHIFT, "Shift")),
        '⌃' => Some((&icons::KEY_CONTROL, "Control")),
        '↩' | '⏎' => Some((&icons::KEY_RETURN, "Return")),
        _ => None,
    }
}

/// One cap's content.
enum Cap {
    Glyph(char),
    Word(String),
}

/// The caps of a chord: each modifier glyph alone, the other characters of a token together.
fn caps(keys: &str) -> Vec<Cap> {
    let mut out = Vec::new();
    for token in keys.split_whitespace() {
        let mut word = String::new();
        for c in token.chars() {
            if modifier(c).is_some() {
                if !word.is_empty() {
                    out.push(Cap::Word(std::mem::take(&mut word)));
                }
                out.push(Cap::Glyph(c));
            } else {
                word.push(c);
            }
        }
        if !word.is_empty() {
            out.push(Cap::Word(word));
        }
    }
    out
}

/// The chord as it is spoken: "⇧⌘P" is "Shift Command P".
pub fn spoken(keys: &str) -> String {
    caps(keys).into_iter().map(|c| match c {
        Cap::Glyph(g) => modifier(g).map_or(String::new(), |(_, n)| n.to_owned()),
        Cap::Word(w) => w,
    }).collect::<Vec<_>>().join(" ")
}

impl Kbd {
    pub fn new(key: impl Into<String>, keys: impl Into<String>) -> Kbd {
        Kbd { key: key.into(), keys: keys.into() }
    }

    pub fn build(self, t: &Theme) -> El {
        let mut r = row().key(self.key).role("note").label(spoken(&self.keys)).gap(t.space.at(0.75)).center_y();
        for k in caps(&self.keys) {
            let cap = row().h(20.0).min_w(20.0).px(t.space.at(1.25)).center().radius(t.radius.sm).round_as(t);
            let recipe = t.mat.raised.base.clone().shadow_none().shadow(rune::Shadow { y: 1.0, color: t.c.line, ..rune::Shadow::default() });
            let face = match k {
                Cap::Glyph(g) => glyph(modifier(g).map_or(&icons::KEY_COMMAND, |(i, _)| i), t.control.icon_sm, t.ink_muted()),
                Cap::Word(w) => text(w).font(TextStyle { size: t.ty.xs.size, line_height: t.ty.xs.line_height, ..t.keycap }).color(t.ink_muted()).a11y_hidden(),
            };
            r = r.child(paint(cap, &recipe).child(face));
        }
        r
    }
}
