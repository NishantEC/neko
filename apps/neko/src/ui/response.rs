//! rune-ui registry: response 0.9.0 Copied into your app by `rune-ui add response`; yours to change. Needs rune-kit's `markdown`
//! feature, which `rune-ui add response` turns on in your Cargo.toml.
//!
//! The assistant's words, as Markdown drawn by rune-kit (pulldown-cmark): paragraphs, headings, lists, task lists, quotes, inline code,
//! fenced code coloured by Rune's built-in highlighter, and tables, all in the theme's type and inks, in a reading column. Streaming is
//! fine: an unclosed fence keeps growing, the newest words fade in (rune-kit's reveal; it lands at once under Reduce Motion), and a
//! caret sits after the last word until the answer is whole. No surface: the answer is the page. A finished answer can be selected and
//! copied (`selectable`, on by default once it is whole); `muted` draws interim words (inside a fold) in the second ink.
//!
//! ```ignore
//! Response::new("answer", &message.text).streaming(message.streaming).build(t)
//! ```

use rune::{Color, El, TextStyle, col};
use rune_kit::{Markdown, MarkdownStyle};
use rune_ui_style::weight;

use super::agent::mono;
use super::base::Theme;

/// What trails the words while they arrive.
pub const CARET: &str = "\u{258D}";

pub struct Response {
    key: String,
    src: String,
    streaming: bool,
    selectable: Option<bool>,
    muted: bool,
    column: f32,
}

impl Response {
    pub fn new(key: impl Into<String>, markdown: impl Into<String>) -> Response {
        Response { key: key.into(), src: markdown.into(), streaming: false, selectable: None, muted: false, column: 720.0 }
    }

    /// The words are still arriving: show the caret and fade the newest words in.
    pub fn streaming(mut self, on: bool) -> Self {
        self.streaming = on;
        self
    }

    /// Whether people can select and copy the words (one selection across the whole answer). Unset, a finished answer is selectable
    /// and one still arriving is not.
    pub fn selectable(mut self, on: bool) -> Self {
        self.selectable = Some(on);
        self
    }

    /// Interim words (an agent's notes inside a fold, between steps): the second ink for text, headings and lists alike.
    pub fn muted(mut self, on: bool) -> Self {
        self.muted = on;
        self
    }

    /// The reading column's widest (720).
    pub fn column(mut self, w: f32) -> Self {
        self.column = w;
        self
    }

    /// The Markdown look in this theme's type and inks: reading text, three heading steps, code on the page's slab without an outline.
    pub fn style(t: &Theme, column: f32) -> MarkdownStyle {
        let body = t.ty.md;
        let heading = |s: TextStyle| weight(TextStyle { line_height: s.line_height.max(s.size * 1.35), ..s }, t.ty.semibold);
        MarkdownStyle::from_kit(t.to_kit())
            .text(t.ink())
            .muted(t.ink_muted())
            .link(t.c.accent.fg)
            .body_font(body)
            .heading_font(1, heading(t.ty.lg))
            .heading_font(2, heading(t.ty.md))
            .heading_font(3, heading(t.ty.base))
            .code_block_font(mono(t, t.ty.base))
            .code_block(t.c.surface.bg, Color::TRANSPARENT)
            .inline_code_bg(t.c.muted.bg)
            .quote_rule(t.c.line)
            .rule(t.c.line)
            .strong_weight(t.ty.semibold)
            .table_outline(None)
            .table_rules(t.c.line)
            .table_header_fill(Color::TRANSPARENT)
            .table_tabular_figures(true)
            .text_width(column)
    }

    pub fn build(self, t: &Theme) -> El {
        let mut src = self.src;
        if self.streaming {
            if !src.is_empty() && !src.ends_with(char::is_whitespace) {
                src.push(' ');
            }
            src.push_str(CARET);
        }
        let mut style = Response::style(t, self.column);
        if self.muted {
            style = style.text(t.ink_muted()).muted(t.ink_faint());
        }
        let selectable = self.selectable.unwrap_or(!self.streaming);
        let md = Markdown::new(src).streaming(self.streaming).reveal(if self.streaming && !t.reduced_motion { 0.18 } else { 0.0 }).selectable(selectable).style(style).build(t.to_kit());
        col().key(self.key).role("document").label("Response").busy(self.streaming).w_full().max_w(self.column).min_w(0.0).child(md)
    }
}
