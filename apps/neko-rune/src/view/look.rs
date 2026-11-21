//! The prototype's widgets (`~/Documents/neko-design/prototype-2026-11-21/_src/app.css`), once, so the sidebar, Home and the cat page
//! compose them and draw no chrome of their own: the status dot and ring, the pill, the full-width segmented control, the small bordered
//! action buttons under a question, a section label, the composer's chips and its round send button.
//!
//! Sizes are the prototype's: a nav row is 32 high on 13.5 points of words, a list row 30, a button 28 (24 small), a pill 22, the
//! composer chip 26, the send button 30; all on its 4/8 grid. Colours come from the theme's tokens only.

use neko_protocol::workbench::TaskStatus;
use rune::{Color, El, LoopSpec, TextStyle, col, el, icon, row, text};

use super::webkit::{font, tnum};
use crate::app::App;
use crate::icons;
use crate::link::TaskRow;
use crate::theme::Neko;

/// A 7 point dot in a colour (`.dot`).
pub fn dot(color: Color) -> El {
    el().size(7.0, 7.0).radius(3.5).fill(color).a11y_hidden()
}

/// A hollow 7 point ring (`.dot.done`, `.dot.off`): finished or queued.
pub fn hollow(t: &Neko, alpha: f32) -> El {
    el().size(7.0, 7.0).radius(3.5).border(1.5, t.text.faint.alpha(alpha)).a11y_hidden()
}

/// The working ring (`.ring`): a 1.5 point track at 28% of the run colour and a quarter turn of it that spins once every 1.2 s.
pub fn ring(t: &Neko, size: f32) -> El {
    let track = icon(&icons::RING_TRACK, size).absolute(0.0, 0.0).color(t.meaning.iris.alpha(0.28));
    let arc = icon(&icons::RING_QUARTER, size).absolute(0.0, 0.0).color(t.meaning.iris).looping(LoopSpec::rotation(1.2));
    el().size(size, size).role("progressbar").label("Working").child(track).child(arc)
}

/// The merge glyph in the green of a pull request that is ready.
pub fn merge(t: &Neko, size: f32) -> El {
    icon(&icons::MERGE, size).color(t.meaning.mint).a11y_hidden()
}

/// What a task's glyph is in the prototype (`tglyph`): the ring while it works, an amber dot when it waits for you, the merge glyph when
/// it is ready, a hollow ring when it is queued or finished.
pub fn task_glyph(t: &Neko, task: &TaskRow) -> El {
    let slot = |e: El| row().size(14.0, 16.0).center().child(e);
    match task.status {
        TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing => slot(ring(t, 12.0)),
        TaskStatus::AwaitingApproval | TaskStatus::Failed => slot(dot(if task.status == TaskStatus::Failed { t.meaning.coral } else { t.meaning.amber })),
        TaskStatus::ReadyForReview => slot(merge(t, 14.0)),
        TaskStatus::Queued => slot(hollow(t, 0.6)),
        TaskStatus::Completed | TaskStatus::Cancelled => slot(hollow(t, 1.0)),
    }
}

/// A section label (`.sec`, `.hs`): 12 on a 28 point line in the third grey, medium, with a count or a quiet figure at the right.
pub fn label(t: &Neko, key: &str, title: &str, right: Option<String>) -> El {
    let mut r = row().key(key.to_owned()).w_full().h(28.0).px(8.0).gap(6.0).center_y().child(text(title.to_owned()).font(font(t, 12.0, 500.0, 16.0)).color(t.text.faint).role("heading").grow(1.0));
    if let Some(right) = right {
        r = r.child(text(right).key(format!("{key}-n")).font(tnum(font(t, 12.0, 400.0, 16.0))).color(t.text.faint));
    }
    r
}

/// A small outlined action under a question (`.acts button`): 24 high, 9 at the sides, radius 6, 12 point words; `primary` is the ink.
pub fn act(t: &Neko, key: &str, words: &str, primary: bool, on_press: impl Fn(&mut App) + 'static) -> El {
    let r = row().key(key.to_owned()).role("button").label(words.to_owned()).focusable(true).h(24.0).px(9.0).center().radius(6.0).focus_ring(2.0, 2.0, t.surface.ring).on_press(on_press);
    let r = if primary { r.fill(t.mat.ink_bottom) } else { r.border(1.0, t.surface.line_strong).hover(t.surface.tint) };
    r.child(text(words.to_owned()).font(font(t, 12.0, if primary { 500.0 } else { 400.0 }, 16.0)).color(if primary { t.mat.ink_fg } else { t.text.muted }).a11y_hidden())
}

/// The full-width segmented control of the cat panel (`.ovhead .seg`): a well of 2 inside (radius 8, `--seg`), equal buttons 24 high with
/// 12 point words, the chosen one raised (`--seg-on`, a hairline round it). `key` names the group, `<key>-<i>` each segment.
pub fn seg_full(t: &Neko, key: &str, labels: &[&str], selected: usize, pick: impl Fn(&mut App, usize) + Clone + 'static) -> El {
    let mut r = row().key(key.to_owned()).role("radiogroup").label(key.to_owned()).w_full().h(28.0).padding(2.0).gap(1.0).radius(8.0).fill(t.surface.seg);
    for (i, l) in labels.iter().enumerate() {
        let on = i == selected;
        let pick = pick.clone();
        let seg = row().key(format!("{key}-{i}")).role("radio").label((*l).to_owned()).checked(on).focusable(true).grow(1.0).min_w(0.0).h(24.0).px(4.0).center().radius(6.0).hover(if on { Color::TRANSPARENT } else { t.surface.hover }).focus_ring(2.0, 2.0, t.surface.ring).on_press(move |s: &mut App| pick(s, i));
        let seg = if on { seg.fill(t.surface.seg_on).border(1.0, t.surface.border) } else { seg };
        r = r.child(seg.child(text((*l).to_owned()).font(font(t, 12.0, 400.0, 16.0)).color(if on { t.text.base } else { t.text.faint }).a11y_hidden()));
    }
    r
}

/// A composer chip (`.chip`): 26 high, 9 at the sides, 12 point words in the second grey, a 14 point glyph before them and a small chevron
/// after them when it opens something.
pub fn chip(t: &Neko, key: &str, glyph: Option<&'static rune::Icon>, words: &str, chevron: bool) -> El {
    let mut r = row().key(key.to_owned()).role("button").label(words.to_owned()).focusable(true).h(26.0).px(9.0).gap(6.0).center_y().radius(7.0).hover(t.surface.tint).focus_ring(2.0, 2.0, t.surface.ring);
    if let Some(g) = glyph {
        r = r.child(icon(g, 14.0).color(t.text.muted).a11y_hidden());
    }
    r = r.child(text(words.to_owned()).font(font(t, 12.0, 400.0, 16.0)).color(t.text.muted).truncate().a11y_hidden());
    if chevron {
        r = r.child(icon(&icons::CHEVRON_DOWN, 12.0).color(t.text.faint).a11y_hidden());
    }
    r
}

/// A 26 point square chip with only a glyph (the attach clip).
pub fn chip_square(t: &Neko, key: &str, label: &str, glyph: &'static rune::Icon) -> El {
    row().key(key.to_owned()).role("button").label(label.to_owned()).help(label.to_owned()).focusable(true).size(26.0, 26.0).center().radius(7.0).hover(t.surface.tint).focus_ring(2.0, 2.0, t.surface.ring).child(icon(glyph, 14.0).color(t.text.muted).a11y_hidden())
}

/// The round send button (`.send`): 30 across, the ink with its arrow when there is something to send, the tint with a quiet arrow when
/// there is not.
pub fn send(t: &Neko, key: &str, label: &str, can: bool, on_press: impl Fn(&mut App) + 'static) -> El {
    let r = row().key(key.to_owned()).role("button").label(label.to_owned()).help(label.to_owned()).focusable(true).size(30.0, 30.0).center().radius(15.0).focus_ring(2.0, 2.0, t.surface.ring).press_scale(0.97).on_press(on_press);
    let r = if can { r.fill(t.mat.ink_bottom) } else { r.fill(t.surface.tint) };
    r.child(icon(&icons::ARROW_UP, 16.0).color(if can { t.mat.ink_fg } else { t.text.faint }).a11y_hidden())
}

/// Words with `**bold**` parts, as the briefing and its sentences are written (`<b>` in the prototype): bold ones are medium weight in the
/// first grey, the rest the second grey. One text, wrapping.
pub fn briefing_text(t: &Neko, key: &str, source: &str, style: TextStyle) -> El {
    let mut plain = String::new();
    let mut spans = Vec::new();
    for (i, part) in source.split("**").enumerate() {
        if i % 2 == 1 {
            let start = plain.len();
            plain.push_str(part);
            spans.push(rune::Span::new(start, plain.len()).weight(500.0).color(t.text.base));
        } else {
            plain.push_str(part);
        }
    }
    rune::rich(plain, spans).key(key.to_owned()).font(style).color(t.text.muted)
}

/// A column that scrolls inside the free space (`.scroll`): no scrollbar, as the prototype hides it.
pub fn scroller(key: &str, body: El) -> El {
    col().key(key.to_owned()).w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(body)
}
