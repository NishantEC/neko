//! The Markdown viewer: the kit's `Markdown`, the same renderer the chat uses, in a scrolling column.

use rune::{El, ScrollSpec, col};
use rune_kit::Markdown;

use super::{ViewerInput, ViewerState};
use crate::theme::theme;

pub fn build(input: &ViewerInput, _state: &mut ViewerState) -> El {
    let t = theme(input.theme_dark);
    let source = input.text.unwrap_or("");
    col().key("viewer-markdown").w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(col().w_full().px(16.0).py(12.0).child(Markdown::new(source).selectable(true).build(&t.kit)))
}
