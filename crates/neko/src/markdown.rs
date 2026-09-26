//! Rendering markdown in the detail pane.
//!
//! Agents write markdown. Until this module existed the conversation pane
//! rendered their prose as one flat monospace string, so every heading, fence
//! and bullet showed its raw markers — `**bold**` on screen, in an app whose
//! whole reason to show a conversation is that reading it here beats switching
//! apps.
//!
//! **Parsed with `pulldown-cmark` (MIT), not a hand-rolled parser** — the same
//! choice comet's own markdown stack makes, and for the same reason: markdown
//! parsing is edge cases all the way down, and every hand parser converges on
//! a worse pulldown-cmark. What is *not* taken from comet is its streaming
//! incremental reparse (~4000 lines): that machinery exists because comet
//! renders a transcript while an agent is still typing it, and this pane shows
//! a finished summary fetched whole. Parsing the whole string per render is
//! microseconds at this size.
//!
//! **Inline styling rides `StyledText::with_highlights`**, gpui's own run
//! mechanism, so bold and code sit inside naturally wrapping text rather than
//! in a flex row that would break mid-word. One honest limit, stated here
//! because it is invisible in code review: `HighlightStyle` has weight, style,
//! colour and background but **no font family**, so inline code gets its wash
//! and colour in the surrounding proportional face rather than in Menlo.
//! Fenced blocks are their own elements and do get the real monospace.
//!
//! The split is the usual one: [`parse`] is pure and unit-tested;
//! [`render`] is the thin gpui layer, reading every colour through
//! `theme::active()` at paint time like the rest of the app.

use gpui::{
    AnyElement, Div, FontStyle, FontWeight, HighlightStyle, IntoElement, ParentElement,
    SharedString, Styled, StyledText, UnderlineStyle, div, px,
};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::theme;

/// One run of identically-styled text within a block.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    /// Rendered underlined; the destination is dropped. A launcher pane has
    /// no cursor-driven link affordance, and painting the URL's text blue
    /// while nothing is clickable would be a promise — an underline says
    /// "this named something" without claiming it can be followed.
    pub link: bool,
}

/// One block-level element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: u8,
        spans: Vec<Span>,
    },
    Paragraph {
        spans: Vec<Span>,
    },
    /// The language tag is parsed and dropped — there is no highlighter here,
    /// and carrying a field nothing reads is how dead vocabulary starts.
    CodeFence {
        text: String,
    },
    /// One item per block, pre-numbered: `marker` is `"•"` or `"3."` so the
    /// renderer never re-derives ordinal state the parser already had.
    ListItem {
        depth: u8,
        marker: String,
        spans: Vec<Span>,
    },
    Quote {
        spans: Vec<Span>,
    },
    Rule,
}

/// Which inline styles are open at this point of the walk.
#[derive(Debug, Clone, Copy, Default)]
struct InlineState {
    bold: bool,
    italic: bool,
    link: bool,
}

/// Where accumulated spans belong when their block closes.
///
/// A list item's marker and depth are decided when it *opens* — that is when
/// the ordinal is current and the list stack has its true height. Deriving
/// either at close time reads the stack after nested lists have already
/// pushed and popped, which is how the first version numbered items wrongly.
enum Container {
    Heading(u8),
    Paragraph,
    Quote,
    ListItem {
        marker: String,
        depth: u8,
        emitted: bool,
    },
}

pub fn parse(source: &str) -> Vec<Block> {
    // Strikethrough is on because agents produce it (`~~done~~`); tables and
    // footnotes are not, because a 460px pane cannot lay a table out and a
    // rendering that silently mangles one is worse than the raw pipes.
    let parser = Parser::new_ext(source, Options::ENABLE_STRIKETHROUGH);

    let mut blocks = Vec::new();
    // **Every open container carries its own span buffer.** A single flat
    // buffer was the first shape here and it was wrong in a way only a probe
    // caught: a nested list opens *inside* its parent item, so with one shared
    // buffer the inner item's close scooped up the outer item's text too —
    // `- outer / - inner` came out as `["outerinner", ""]`.
    let mut containers: Vec<(Container, Vec<Span>)> = Vec::new();
    let mut inline = InlineState::default();
    // Next ordinal per open list; `None` for bullets.
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut fence: Option<String> = None;

    #[allow(clippy::ptr_arg)] // `last_mut` on the Vec itself, not a slice view
    fn push_text(
        containers: &mut Vec<(Container, Vec<Span>)>,
        blocks: &mut Vec<Block>,
        inline: InlineState,
        text: &str,
        code: bool,
    ) {
        if text.is_empty() {
            return;
        }
        let span = Span {
            text: text.to_string(),
            bold: inline.bold,
            italic: inline.italic,
            code,
            link: inline.link,
        };
        let Some((_, spans)) = containers.last_mut() else {
            // Loose text outside any container (rare, but pulldown emits it
            // for some shapes) — its own paragraph rather than a dropped word.
            blocks.push(Block::Paragraph { spans: vec![span] });
            return;
        };
        // Merge into the previous span when nothing about the style changed,
        // so `a *b* c` is three spans rather than one per parser event.
        if let Some(last) = spans.last_mut()
            && last.bold == span.bold
            && last.italic == span.italic
            && last.link == span.link
            && last.code == span.code
        {
            last.text.push_str(text);
            return;
        }
        spans.push(span);
    }

    fn close(
        blocks: &mut Vec<Block>,
        containers: &mut Vec<(Container, Vec<Span>)>,
        lists: &[Option<u64>],
    ) {
        let Some((container, spans)) = containers.pop() else {
            return;
        };
        match container {
            Container::Heading(level) => blocks.push(Block::Heading { level, spans }),
            Container::Paragraph => {
                if !spans.is_empty() {
                    blocks.push(Block::Paragraph { spans });
                }
            }
            Container::Quote => {
                if !spans.is_empty() {
                    blocks.push(Block::Quote { spans });
                }
            }
            Container::ListItem {
                marker,
                depth,
                emitted,
            } => {
                // Already flushed when its nested list opened; anything left
                // is trailing text after the sublist, its own line.
                if !emitted || !spans.is_empty() {
                    blocks.push(Block::ListItem {
                        depth,
                        marker,
                        spans,
                    });
                }
            }
        }
        let _ = lists;
    }

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                containers.push((
                    Container::Heading(match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        _ => 3,
                    }),
                    Vec::new(),
                ));
            }
            Event::End(TagEnd::Heading(_)) => close(&mut blocks, &mut containers, &lists),
            Event::Start(Tag::Paragraph) => {
                // Inside a list item the paragraph is the item's own text
                // (a "loose" list); opening a second buffer here would file
                // the text under Paragraph and leave the item empty. The item
                // is already the open container, so the paragraph is
                // structural noise at this pane's fidelity.
                if !matches!(containers.last(), Some((Container::ListItem { .. }, _))) {
                    containers.push((Container::Paragraph, Vec::new()));
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if matches!(containers.last(), Some((Container::Paragraph, _))) {
                    close(&mut blocks, &mut containers, &lists);
                }
            }
            Event::Start(Tag::BlockQuote(_)) => containers.push((Container::Quote, Vec::new())),
            Event::End(TagEnd::BlockQuote(_)) => {
                if matches!(containers.last(), Some((Container::Quote, _))) {
                    close(&mut blocks, &mut containers, &lists);
                }
            }
            Event::Start(Tag::List(start)) => {
                // **A nested list opens *inside* its parent item, before that
                // item's End** — and blocks emit at close. Without this
                // flush the parent's own text landed *after* all of its
                // children, which on screen read as the sublist belonging to
                // the item above. Caught by a screenshot, not by the parse
                // tests, which only checked membership and depth.
                if let Some((
                    Container::ListItem {
                        marker,
                        depth,
                        emitted,
                    },
                    spans,
                )) = containers.last_mut()
                    && !spans.is_empty()
                {
                    blocks.push(Block::ListItem {
                        depth: *depth,
                        marker: std::mem::take(marker),
                        spans: std::mem::take(spans),
                    });
                    *emitted = true;
                }
                lists.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                lists.pop();
            }
            Event::Start(Tag::Item) => {
                let depth = lists.len().saturating_sub(1) as u8;
                let marker = match lists.last_mut().and_then(|slot| slot.as_mut()) {
                    Some(next) => {
                        let marker = format!("{next}.");
                        *next += 1;
                        marker
                    }
                    None => "\u{2022}".to_string(),
                };
                containers.push((
                    Container::ListItem {
                        marker,
                        depth,
                        emitted: false,
                    },
                    Vec::new(),
                ));
            }
            Event::End(TagEnd::Item) => close(&mut blocks, &mut containers, &lists),
            Event::Start(Tag::CodeBlock(_)) => fence = Some(String::new()),
            Event::End(TagEnd::CodeBlock) => {
                if let Some(text) = fence.take() {
                    blocks.push(Block::CodeFence {
                        text: text.trim_end().to_string(),
                    });
                }
            }
            Event::Start(Tag::Strong) => inline.bold = true,
            Event::End(TagEnd::Strong) => inline.bold = false,
            Event::Start(Tag::Emphasis) => inline.italic = true,
            Event::End(TagEnd::Emphasis) => inline.italic = false,
            Event::Start(Tag::Link { .. }) => inline.link = true,
            Event::End(TagEnd::Link) => inline.link = false,
            Event::Code(text) => {
                if let Some(fence) = fence.as_mut() {
                    fence.push_str(&text);
                } else {
                    push_text(&mut containers, &mut blocks, inline, &text, true);
                }
            }
            Event::Text(text) => {
                if let Some(fence) = fence.as_mut() {
                    fence.push_str(&text);
                } else {
                    push_text(&mut containers, &mut blocks, inline, &text, false);
                }
            }
            Event::SoftBreak => push_text(&mut containers, &mut blocks, inline, " ", false),
            Event::HardBreak => push_text(&mut containers, &mut blocks, inline, "\n", false),
            Event::Rule => blocks.push(Block::Rule),
            // Raw HTML, footnotes, task-list markers: keep any carried text,
            // drop the rest — a pane showing most of a message beats one that
            // panics on the odd shape.
            Event::Html(text) | Event::InlineHtml(text) => {
                push_text(&mut containers, &mut blocks, inline, &text, false)
            }
            _ => {}
        }
    }
    // Unterminated containers (a fence never closed, EOF mid-paragraph —
    // pulldown emits Ends reliably, but cheap insurance):
    while !containers.is_empty() {
        close(&mut blocks, &mut containers, &lists);
    }
    if let Some(text) = fence.take()
        && !text.trim().is_empty()
    {
        blocks.push(Block::CodeFence {
            text: text.trim_end().to_string(),
        });
    }
    blocks
}

/// Flattens spans to one string plus highlight ranges — the shape
/// `StyledText::with_highlights` wants. Pure, so the range arithmetic (byte
/// offsets, not chars) is testable without a window.
pub fn flatten(spans: &[Span]) -> (String, Vec<(std::ops::Range<usize>, HighlightStyle)>) {
    let mut text = String::new();
    let mut highlights = Vec::new();
    for span in spans {
        let start = text.len();
        text.push_str(&span.text);
        let style = span_style(span);
        if style != HighlightStyle::default() {
            highlights.push((start..text.len(), style));
        }
    }
    (text, highlights)
}

fn span_style(span: &Span) -> HighlightStyle {
    let mut style = HighlightStyle::default();
    if span.bold {
        style.font_weight = Some(FontWeight::SEMIBOLD);
        style.color = Some(theme::active().text_primary.into());
    }
    if span.italic {
        style.font_style = Some(FontStyle::Italic);
    }
    if span.code {
        style.background_color = Some(theme::active().row_icon_socket_bg.into());
        style.color = Some(theme::active().text_primary.into());
    }
    if span.link {
        style.underline = Some(UnderlineStyle {
            thickness: px(1.),
            ..Default::default()
        });
    }
    style
}

fn styled_line(spans: &[Span]) -> StyledText {
    let (text, highlights) = flatten(spans);
    StyledText::new(SharedString::from(text)).with_highlights(highlights)
}

/// Markdown → one column of gpui elements, in this app's own tokens. Reuses
/// parsed blocks across frames. Views re-render
/// on every poll; chat history and ticket results can be long, so parsing is
/// paid once per distinct text instead of once per frame.
pub fn render_cached(source: &str) -> Div {
    use std::hash::{Hash, Hasher};
    thread_local! {
        static CACHE: std::cell::RefCell<std::collections::HashMap<u64, std::rc::Rc<Vec<Block>>>> = Default::default();
    }
    const MAX_ENTRIES: usize = 512;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    let key = hasher.finish();
    let blocks = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(blocks) = cache.get(&key) {
            return blocks.clone();
        }
        if cache.len() >= MAX_ENTRIES {
            cache.clear();
        }
        let blocks = std::rc::Rc::new(parse(source));
        cache.insert(key, blocks.clone());
        blocks
    });
    render_blocks(&blocks)
}

fn render_blocks(blocks: &[Block]) -> Div {
    let mut column = div().flex().flex_col().gap(px(7.));
    for block in blocks {
        column = column.child(render_block(block));
    }
    column
}

fn render_block(block: &Block) -> AnyElement {
    let palette = theme::active();
    match block {
        Block::Heading { level, spans } => {
            let size = match level {
                1 => 14.5,
                2 => 13.0,
                _ => 12.0,
            };
            div()
                .mt(px(4.))
                .text_size(px(size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(palette.text_primary)
                .child(styled_line(spans))
                .into_any_element()
        }
        Block::Paragraph { spans } => div()
            .text_size(px(12.))
            .text_color(palette.text_primary)
            .child(styled_line(spans))
            .into_any_element(),
        Block::CodeFence { text } => div()
            .px(px(9.))
            .py(px(7.))
            .rounded(px(theme::CHIP_RADIUS_PX))
            .bg(palette.surface_input)
            .font_family(theme::MONOSPACE_FAMILY)
            .text_size(px(theme::PREVIEW_MONOSPACE_SIZE_PX))
            .text_color(palette.text_primary)
            .child(SharedString::from(text.clone()))
            .into_any_element(),
        Block::ListItem {
            depth,
            marker,
            spans,
        } => div()
            .flex()
            .gap(px(6.))
            .pl(px(4. + 14. * f32::from(*depth)))
            .text_size(px(12.))
            .text_color(palette.text_primary)
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(palette.text_tertiary)
                    .child(SharedString::from(marker.clone())),
            )
            .child(div().min_w(px(0.)).child(styled_line(spans)))
            .into_any_element(),
        Block::Quote { spans } => div()
            .pl(px(9.))
            .border_l_2()
            .border_color(palette.border_hairline_strong)
            .text_size(px(12.))
            .text_color(palette.text_secondary)
            .child(styled_line(spans))
            .into_any_element(),
        Block::Rule => div()
            .my(px(2.))
            .h(px(1.))
            .bg(palette.border_hairline)
            .into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(blocks: &[Block]) -> Vec<String> {
        blocks
            .iter()
            .map(|b| match b {
                Block::Heading { spans, .. }
                | Block::Paragraph { spans }
                | Block::Quote { spans }
                | Block::ListItem { spans, .. } => {
                    spans.iter().map(|s| s.text.as_str()).collect::<String>()
                }
                Block::CodeFence { text } => text.clone(),
                Block::Rule => "---".into(),
            })
            .collect()
    }

    /// The shape an agent actually produces, assembled from real transcript
    /// fragments rather than a markdown showcase.
    const AGENT: &str = "## What changed\n\n\
        All **60 documents** now written. The `node:` field is wired in.\n\n\
        - fixed the ingest script\n\
        - re-ran `pnpm ingest-drive`\n\n\
        ```sh\npnpm ingest-drive --limit 0 | tail -3\n```\n\n\
        Not yet done: the granth UI.";

    #[test]
    fn an_agent_reply_parses_into_the_blocks_a_reader_expects() {
        let blocks = parse(AGENT);
        assert!(matches!(&blocks[0], Block::Heading { level: 2, .. }));
        assert!(matches!(&blocks[1], Block::Paragraph { .. }));
        assert!(matches!(&blocks[2], Block::ListItem { .. }));
        assert!(matches!(&blocks[3], Block::ListItem { .. }));
        assert!(matches!(&blocks[4], Block::CodeFence { .. }));
        assert!(matches!(&blocks[5], Block::Paragraph { .. }));
    }

    #[test]
    fn bold_and_inline_code_become_styled_spans_not_markers() {
        let blocks = parse(AGENT);
        let Block::Paragraph { spans } = &blocks[1] else {
            panic!("not a paragraph")
        };
        assert!(spans.iter().any(|s| s.bold && s.text == "60 documents"));
        assert!(spans.iter().any(|s| s.code && s.text == "node:"));
        assert!(
            !spans.iter().any(|s| s.text.contains("**")),
            "markers must not survive"
        );
    }

    #[test]
    fn a_fence_keeps_its_text_verbatim_and_drops_its_language() {
        let blocks = parse(AGENT);
        let Block::CodeFence { text } = &blocks[4] else {
            panic!("not a fence")
        };
        assert_eq!(text, "pnpm ingest-drive --limit 0 | tail -3");
    }

    #[test]
    fn a_tool_headline_survives_a_markdown_pass_unchanged() {
        // `[Write] /path` is the one non-markdown shape the conversation
        // provider sends through this renderer; a bracketed span with no `(`
        // after it is not link syntax and must come through verbatim.
        let blocks = parse("[Write] /Users/example/a.md");
        assert_eq!(plain(&blocks), vec!["[Write] /Users/example/a.md"]);
    }

    #[test]
    fn a_sublist_renders_under_its_own_parent_not_the_item_above() {
        // The exact shape the screenshot caught: "twice" is nested under
        // "re-ran", and it rendered between "fixed" and "re-ran" because
        // blocks emitted at close and a parent closes after its children.
        let blocks = parse("- fixed the script\n- re-ran ingest\n  - twice\n");
        assert_eq!(
            plain(&blocks),
            vec!["fixed the script", "re-ran ingest", "twice"]
        );
    }

    #[test]
    fn ordered_items_carry_their_own_numbers() {
        let blocks = parse("1. first\n2. second\n");
        let markers: Vec<&str> = blocks
            .iter()
            .filter_map(|b| match b {
                Block::ListItem { marker, .. } => Some(marker.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(markers, vec!["1.", "2."]);
    }

    #[test]
    fn each_nested_item_keeps_its_own_text() {
        // The first parser here used one flat span buffer, and this exact
        // input came out as ["outerinner", ""] — the inner item's close
        // scooped up the outer item's text. Every container buffers its own
        // spans now, which is what this pins.
        let blocks = parse("- outer\n  - inner\n");
        assert_eq!(
            plain(&blocks),
            vec!["outer", "inner"],
            "parent before child, as written"
        );
    }
    #[test]
    fn nested_bullets_know_their_depth() {
        let blocks = parse("- outer\n  - inner\n");
        let depths: Vec<u8> = blocks
            .iter()
            .filter_map(|b| match b {
                Block::ListItem { depth, .. } => Some(*depth),
                _ => None,
            })
            .collect();
        assert_eq!(depths, vec![0, 1], "parent first, child one level deeper");
    }

    #[test]
    fn a_link_keeps_its_text_and_drops_its_destination() {
        let blocks = parse("see [the docs](https://example.com/x) here");
        let Block::Paragraph { spans } = &blocks[0] else {
            panic!()
        };
        assert!(spans.iter().any(|s| s.link && s.text == "the docs"));
        assert!(
            !plain(&blocks)[0].contains("https"),
            "the URL is not rendered text"
        );
    }

    #[test]
    fn flatten_ranges_are_byte_offsets_into_the_joined_string() {
        // Non-ASCII before a styled span is the case that breaks a char-count
        // implementation silently.
        let spans = vec![
            Span {
                text: "café ".into(),
                ..Default::default()
            },
            Span {
                text: "东".into(),
                bold: true,
                ..Default::default()
            },
        ];
        let (text, highlights) = flatten(&spans);
        assert_eq!(&text[highlights[0].0.clone()], "东");
    }

    #[test]
    fn an_unclosed_fence_still_shows_its_text() {
        let blocks = parse("```\nhalf a fence, no closer");
        assert_eq!(plain(&blocks), vec!["half a fence, no closer"]);
    }

    #[test]
    fn empty_input_renders_nothing_rather_than_an_empty_block() {
        assert!(parse("").is_empty());
        assert!(parse("\n\n  \n").is_empty());
    }
}
