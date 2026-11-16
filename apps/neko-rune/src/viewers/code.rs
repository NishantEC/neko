//! The code viewer: text with line numbers, read-only, coloured by `rune-highlights` through the
//! kit's `syntax` (the same lexers the diff and Markdown fences use). Lines are virtual rows, so a
//! two-megabyte log costs what the forty visible lines do; the highlighter colours the first
//! 128 KiB (the kit's own limit) and the rest draws plain.

use std::borrow::Cow;
use std::rc::Rc;

use rune::{El, GrowthSpec, ScrollSpec, Span, col, row, text};
use rune_kit::syntax::{SyntaxTheme, highlight, lang_for_path};

use super::{ViewerInput, ViewerState};
use crate::app::App;
use crate::theme::theme;

/// The height of a line: the mono face's own line height.
const LINE_H: f32 = 17.0;
/// The gutter's width: room for six digits.
const GUTTER: f32 = 52.0;

/// A text split into lines, with the colour spans of the whole text kept once.
pub struct CodeData {
    pub text: String,
    /// Where each line starts and ends in `text`, without its line break.
    pub lines: Vec<(u32, u32)>,
    /// Spans over the whole text, in order.
    pub spans: Vec<Span>,
}

impl CodeData {
    pub fn new(source: &str, kind: &str, syntax: &SyntaxTheme) -> CodeData {
        let mut lines = Vec::new();
        let mut start = 0usize;
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                let end = if i > start && source.as_bytes()[i - 1] == b'\r' { i - 1 } else { i };
                lines.push((start as u32, end as u32));
                start = i + 1;
            }
        }
        if start < source.len() {
            lines.push((start as u32, source.len() as u32));
        }
        let lang = lang_for_path(&format!("file.{kind}"));
        let spans = if lang.is_some() { syntax.spans(&highlight(lang, source), 0) } else { Vec::new() };
        CodeData { text: source.to_owned(), lines, spans }
    }

    pub fn count(&self) -> usize {
        self.lines.len()
    }

    pub fn line(&self, i: usize) -> &str {
        self.lines.get(i).map_or("", |(a, b)| &self.text[*a as usize..*b as usize])
    }

    /// The spans that touch line `i`, moved to start at the line's own first byte.
    pub fn line_spans(&self, i: usize) -> Vec<Span> {
        let Some(&(a, b)) = self.lines.get(i) else { return Vec::new() };
        let first = self.spans.partition_point(|s| s.end <= a);
        self.spans[first..]
            .iter()
            .take_while(|s| s.start < b)
            .map(|s| Span { start: s.start.max(a) - a, end: s.end.min(b) - a, ..s.clone() })
            .collect()
    }
}

pub fn build(input: &ViewerInput, state: &mut ViewerState) -> El {
    let t = theme(input.theme_dark);
    let Some(source) = input.text else {
        return col().key("viewer-code").w_full().grow(1.0).child(rune::text("This file isn’t text, so there is nothing to show.").font(t.ty.meta).color(t.text.muted).px(16.0).py(12.0));
    };
    let kind = input.kind.to_owned();
    let syntax = SyntaxTheme::from_kit(&t.kit);
    let data: Rc<CodeData> = state.data(|| Rc::new(CodeData::new(source, &kind, &syntax))).clone();
    let version = (state.key.clone(), data.count());
    let rows = {
        let data = data.clone();
        let (ink, faint, mono) = (t.text.base, t.text.faint, t.ty.mono);
        col()
            .key("code-rows")
            .w_full()
            .growth(GrowthSpec::default())
            .virtual_rows(
                data.count().max(1),
                LINE_H,
                |_: &App, i| Cow::Owned(format!("l{i}")),
                move |_: &App, i| {
                    row()
                        .w_full()
                        .h(LINE_H)
                        .child(text((i + 1).to_string()).font(rune::TextStyle { features: "'tnum' 1", ..mono }).color(faint).w(GUTTER).pr(12.0).align(rune::Align::End).a11y_hidden())
                        .child(text(data.line(i).to_owned()).font(mono).color(ink).spans(data.line_spans(i)).selectable(true))
                },
            )
            .virtual_rows_version(version)
    };
    col().key("viewer-code").w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(col().w_full().py(8.0).child(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn syntax() -> SyntaxTheme {
        SyntaxTheme::from_kit(&theme(true).kit)
    }

    #[test]
    fn text_is_split_into_lines_with_or_without_a_final_newline_and_crlf() {
        let d = CodeData::new("one\ntwo\r\nthree", "txt", &syntax());
        assert_eq!((0..d.count()).map(|i| d.line(i)).collect::<Vec<_>>(), ["one", "two", "three"]);
        let d = CodeData::new("a\nb\n", "txt", &syntax());
        assert_eq!(d.count(), 2, "a final newline is the end of the last line");
        assert_eq!(CodeData::new("", "txt", &syntax()).count(), 0);
        assert_eq!(CodeData::new("\n\n", "txt", &syntax()).count(), 2, "blank lines count");
    }

    #[test]
    fn colour_spans_land_on_the_line_they_belong_to() {
        let src = "fn main() {\n    let x = 1;\n}\n";
        let d = CodeData::new(src, "rs", &syntax());
        assert!(!d.spans.is_empty(), "Rust is highlighted");
        let first = d.line_spans(0);
        assert!(first.iter().any(|s| d.line(0)[s.start as usize..s.end as usize].trim() == "fn"), "the keyword is on line 1: {first:?}");
        for i in 0..d.count() {
            for s in d.line_spans(i) {
                assert!(s.end as usize <= d.line(i).len() && s.start < s.end, "line {i} span {s:?} fits its line");
            }
        }
        // A kind nobody highlights draws plain.
        assert!(CodeData::new(src, "log", &syntax()).spans.is_empty());
    }
}
