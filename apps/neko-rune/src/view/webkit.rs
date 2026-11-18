//! The web's own kit, in Rune (`web/src/setup/kit.tsx`, `ui/button.tsx`): the pieces a settings page is made of, with the web's
//! measures, so a page built from them lines up with the web's at the same width (docs/knowledge/rune-identity.md).
//!
//! Every number here was read off the web page in embed mode (`scripts/identity-dom.mjs`), not guessed.

use rune::{Color, El, Icon, TextStyle, col, el, icon, row, text};
use rune_kit::{Select, TextInput};

use crate::app::App;
use crate::theme::Neko;

thread_local! {
    /// The width of the page the settings sit in (the web's viewport), set when the page is built.
    static PAGE_W: std::cell::Cell<f32> = const { std::cell::Cell::new(834.0) };
}

pub fn set_page_width(w: f32) {
    PAGE_W.with(|c| c.set(w));
}

pub fn page_width() -> f32 {
    PAGE_W.with(|c| c.get())
}

thread_local! {
    /// The width of the column a Setup page's content sits in (the viewport less the page's own sides), for text that flows by hand.
    static CONTENT_W: std::cell::Cell<f32> = const { std::cell::Cell::new(578.0) };
}

pub fn set_content_width(w: f32) {
    CONTENT_W.with(|c| c.set(w));
}

pub fn content_width() -> f32 {
    CONTENT_W.with(|c| c.get())
}

/// The web's text at a size: Inter, the given weight, the line height the browser measured.
pub fn font(t: &Neko, size: f32, weight: f32, line_height: f32) -> TextStyle {
    TextStyle { size, weight, line_height, ..t.ty.row }
}

/// The web's `font-mono`: the same size and line, in the mono face.
///
/// Rune's mono face is 2.6% wider than the browser's `ui-monospace` (0.618 em an advance against 0.602, measured on the manifest
/// example), so the tracking takes the difference back and a path or a code line is as wide as the web's.
pub fn mono(style: TextStyle) -> TextStyle {
    TextStyle { family: rune::Family::Mono, tracking: style.tracking - 0.016 * style.size, ..style }
}

/// The web's `tabular-nums`: figures of one width.
pub fn tnum(style: TextStyle) -> TextStyle {
    TextStyle { features: "'tnum' 1", ..style }
}

/// `Section` (`kit.tsx`): a heading line 28 high (12.5 medium muted, a faint count after it, actions at the right), 6 below it, then
/// the body.
pub fn section(t: &Neko, key: &str, title: &str, count: Option<&str>, actions: Option<El>, body: El) -> El {
    let mut label = row().gap(6.0).center_y().child(text(title.to_owned()).font(font(t, 12.5, 500.0, 20.3125)).color(t.text.muted));
    if let Some(count) = count {
        label = label.child(text(count.to_owned()).font(font(t, 12.5, 500.0, 20.3125)).color(t.text.muted.alpha(0.7)));
    }
    let mut head = row().w_full().min_h(28.0).gap(16.0).center_y().child(label);
    if let Some(actions) = actions {
        head = head.child(el().grow(1.0).min_w(0.0)).child(actions);
    }
    col().key(key.to_owned()).w_full().gap(6.0).child(head).child(body)
}

/// `Panel`: no card, no border; it bleeds 12 either side so a row's text lines up with the heading above.
pub fn panel(rows: Vec<El>) -> El {
    col().mx(-12.0).children(rows)
}

/// A tone for [`status`] (`setup/lib.ts` `Tone`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Success,
    Warning,
    Error,
}

/// `Status` (`kit.tsx`, `ui/status-badge.tsx`): 23 high, 7 at the sides, radius 10, a 6 point dot and 11 point medium words.
pub fn status(t: &Neko, key: &str, tone: Tone, words: &str) -> El {
    status_keyed(t, key, &format!("{key}-words"), tone, words)
}

/// [`status`] with a key for its words too.
pub fn status_keyed(t: &Neko, key: &str, text_key: &str, tone: Tone, words: &str) -> El {
    let (dot, wash, ink) = match tone {
        Tone::Neutral => (t.text.muted.alpha(0.6), t.text.base.alpha(0.06), t.text.muted),
        Tone::Success => (t.meaning.mint, t.meaning.mint.alpha(0.1), t.meaning.mint),
        Tone::Warning => (t.meaning.amber, t.meaning.amber.alpha(0.1), t.meaning.amber),
        Tone::Error => (t.meaning.coral, t.meaning.coral.alpha(0.1), t.meaning.coral),
    };
    row().key(key.to_owned()).h(23.0).px(7.0).gap(6.0).center_y().radius(10.0).fill(wash).child(el().size(6.0, 6.0).radius(3.0).fill(dot).a11y_hidden()).child(text(words.to_owned()).key(text_key.to_owned()).font(font(t, 11.0, 500.0, 17.0)).color(ink))
}

/// [`status`] whose words are separate text nodes that run on ("App " then "9acc5352"): one line, the way the web's inline text is.
pub fn status_nodes(t: &Neko, key: &str, tone: Tone, nodes: &[&str]) -> El {
    let (dot, wash, ink) = match tone {
        Tone::Neutral => (t.text.muted.alpha(0.6), t.text.base.alpha(0.06), t.text.muted),
        Tone::Success => (t.meaning.mint, t.meaning.mint.alpha(0.1), t.meaning.mint),
        Tone::Warning => (t.meaning.amber, t.meaning.amber.alpha(0.1), t.meaning.amber),
        Tone::Error => (t.meaning.coral, t.meaning.coral.alpha(0.1), t.meaning.coral),
    };
    let last = nodes.len().saturating_sub(1);
    let words = row().children(nodes.iter().enumerate().map(|(i, n)| {
        let node = text((*n).to_owned()).font(font(t, 11.0, 500.0, 17.0)).color(ink);
        if i < last { node.mr(3.0) } else { node }
    }));
    row().key(key.to_owned()).h(23.0).px(7.0).gap(6.0).center_y().radius(10.0).fill(wash).child(el().size(6.0, 6.0).radius(3.0).fill(dot).a11y_hidden()).child(words)
}

/// A small quiet action that fits inside a 28 point line (12.5 point words, 24 high).
pub fn link_button(t: &Neko, key: &str, label: &str, on_press: impl Fn(&mut App) + 'static) -> El {
    row().key(key.to_owned()).role("button").label(label).focusable(true).h(24.0).px(8.0).center().radius(6.0).hover(t.text.base.alpha(0.06)).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(on_press).child(text(label.to_owned()).font(font(t, 12.5, 500.0, 20.3125)).color(t.text.muted).a11y_hidden())
}

/// `Row` (`kit.tsx`): 44 high (8 above and below a 28 line), 12 at the sides, radius 10; the title, a dim line after it that gives way,
/// the trailing things at the right, and anything under it (`mt-1.5 ps-8`).
pub fn info_row(t: &Neko, key: &str, title: El, description: Option<El>, leading: Option<El>, trailing: Vec<El>, below: Option<El>) -> El {
    let mut line = row().w_full().min_h(28.0).gap(12.0).center_y();
    if let Some(l) = leading {
        line = line.child(l.a11y_hidden());
    }
    line = line.child(title.shrink());
    line = line.child(description.unwrap_or_else(|| el().grow(1.0)));
    if !trailing.is_empty() {
        line = line.child(row().gap(8.0).center_y().children(trailing));
    }
    let mut r = col().key(key.to_owned()).w_full().px(12.0).py(8.0).radius(10.0).hover(t.text.base.alpha(0.03)).child(line);
    if let Some(b) = below {
        r = r.child(col().w_full().mt(6.0).pl(32.0).child(b));
    }
    r
}

/// `PanelFooter`: a quiet line at the left and the actions at the right (under them when they do not fit); 12 at the sides, 4 above
/// and 20 below (the footer's own padding in the rendered page).
pub fn panel_footer(t: &Neko, key: &str, notes: &[String], buttons: Vec<El>) -> El {
    // Each `{...}` of the web's JSX is a text node of its own, 4 apart (a space): they flow and wrap as one line.
    let line = row().min_w(0.0).gaps(0.0, 4.0).flex_wrap().children(notes.iter().map(|n| text(n.clone()).font(font(t, 12.0, 400.0, 16.0)).color(t.text.muted).wrap().min_w(0.0)));
    panel_footer_with(key, line, buttons)
}

/// [`panel_footer`] with a left side of its own (the Rulebook's "Saved · ⌘S saves").
pub fn panel_footer_with(key: &str, left: El, buttons: Vec<El>) -> El {
    row().key(key.to_owned()).w_full().px(12.0).pt(4.0).pb(20.0).gaps(8.0, 16.0).flex_wrap().center_y().justify_between().child(left).child(row().gap(8.0).center_y().children(buttons))
}

/// The web's `Input` in a settings row: 40 high, a pill, the foreground at 5%, 14 at the sides; `mono` is `font-mono`.
#[allow(clippy::too_many_arguments)]
pub fn input(s: &App, t: &Neko, key: &str, label: &str, placeholder: &str, width: f32, mono: bool, submit: impl Fn(&mut App) + 'static) -> El {
    let mut kit = t.kit.clone();
    if mono {
        kit.ty.ui = TextStyle { family: rune::Family::Mono, size: t.ty.mono.size, ..kit.ty.ui };
    }
    TextInput::new(super::ui::edit_of(s, key))
        .label(label)
        .placeholder(placeholder)
        .width(Some(width))
        .on_edit(super::ui::editing(key.to_owned()))
        .on_submit(move |s: &mut App, _| submit(s))
        .build(&kit)
        .key(key.to_owned())
        .shrink()
        .h(40.0)
        .radius(24.0)
        .fill(t.text.base.alpha(0.05))
}

/// The web's `Textarea` for a document (`ui/textarea.tsx`, the Rulebook): 13 point words on a 21.45 line, 10 above and 12 at the
/// sides inside a 1 point border, radius 18, the foreground at 5%, 384 high at least and growing with the text (`field-sizing:
/// content`). Built from Rune's own text field because the kit's `TextArea` fixes the line at 17.5.
pub fn text_area(s: &App, t: &Neko, key: &str, label: &str, placeholder: &str, min_h: f32) -> El {
    let style = font(t, 13.0, 400.0, 21.45);
    let field = rune::text_input(super::ui::edit_of(s, key), style)
        .key("input")
        .w_full()
        .color(t.text.base)
        .placeholder(placeholder.to_owned(), t.text.faint)
        .caret(t.text.base, t.kit.selection())
        .label(label.to_owned())
        .on_edit(super::ui::editing(key.to_owned()));
    let lines = ((min_h - 22.0) / 21.45).ceil() as usize;
    col()
        .key(key.to_owned())
        .role("textbox")
        .w_full()
        .min_h(min_h)
        .px(13.0)
        .py(11.0)
        .radius(18.0)
        .fill(t.text.base.alpha(0.05))
        .focus_within_ring(2.0, 2.0, t.kit.c.border_focus)
        .child(rune::auto_grow(field, lines, 10_000))
}

/// `relativeTime` (`setup/lib.ts`): "just now" under 45 seconds, then `Intl.RelativeTimeFormat` with `numeric: "auto"`.
pub fn relative_time(now_ms: i64, at_ms: i64) -> String {
    if at_ms <= 0 {
        return "Never".to_owned();
    }
    let diff = at_ms - now_ms;
    let abs = diff.abs();
    if abs < 45_000 {
        return "just now".to_owned();
    }
    const STEPS: [(i64, &str); 5] = [(365 * 86_400_000, "year"), (30 * 86_400_000, "month"), (86_400_000, "day"), (3_600_000, "hour"), (60_000, "minute")];
    let (size, unit) = STEPS.iter().copied().find(|(s, _)| abs >= *s).unwrap_or(STEPS[4]);
    let n = (diff as f64 / size as f64).round() as i64;
    match (n, unit) {
        (0, "day") => "today".to_owned(),
        (-1, "day") => "yesterday".to_owned(),
        (1, "day") => "tomorrow".to_owned(),
        (-1, "year" | "month") => format!("last {unit}"),
        (1, "year" | "month") => format!("next {unit}"),
        (n, _) if n < 0 => format!("{} {unit}{} ago", -n, if n == -1 { "" } else { "s" }),
        (n, _) => format!("in {n} {unit}{}", if n == 1 { "" } else { "s" }),
    }
}

/// `SettingRow`: the title (13.5) and under it the description (12.5 muted, at most 65ch = 512.6 wide) at the left, the control at the
/// right; 48 high at least, 12 at the sides, 10 above and below, radius 10. Under 640 (`sm:`) the control drops under the words.
pub fn setting_row(t: &Neko, key: &str, title: &str, description: Option<&str>, control: El) -> El {
    build_row(t, key, title, description, control, false)
}

/// `SettingRow stacked`: the control always under the words and across the row (a text field, a collapsed form).
pub fn setting_row_stacked(t: &Neko, key: &str, title: &str, description: Option<&str>, control: El) -> El {
    build_row(t, key, title, description, control, true)
}

fn build_row(t: &Neko, key: &str, title: &str, description: Option<&str>, control: El, stacked: bool) -> El {
    let narrow = stacked || page_width() < 640.0;
    let mut left = col().min_w(0.0).gap(2.0).child(text(title.to_owned()).font(font(t, 13.5, 400.0, 21.9375)).color(t.text.base).wrap().min_w(0.0));
    if let Some(d) = description {
        left = left.child(text(d.to_owned()).font(font(t, 12.5, 400.0, 20.3125)).color(t.text.muted).wrap().min_w(0.0).max_w(512.6));
    }
    if narrow {
        let controls = row().w_full().gap(8.0).center_y().flex_wrap().child(control);
        return col().key(key.to_owned()).min_h(48.0).px(12.0).py(10.0).gap(8.0).radius(10.0).child(left.w_full()).child(controls);
    }
    row().key(key.to_owned()).min_h(48.0).px(12.0).py(10.0).gap(16.0).center_y().radius(10.0).child(left.shrink().grow(1.0)).child(row().gap(8.0).center_y().child(control))
}

/// `Kbd` (`ui/kbd.tsx`): 20 high, 20 wide at least, 4 at the sides, radius 6, 12 point medium muted words on a faint wash.
pub fn kbd(t: &Neko, key: &str, words: &str) -> El {
    row().key(key.to_owned()).h(20.0).min_w(20.0).px(4.0).center().radius(6.0).fill(t.text.base.alpha(0.04)).child(text(words.to_owned()).font(font(t, 12.0, 500.0, 16.0)).color(t.text.muted).a11y_hidden())
}

/// `Skeleton`: a quiet pulse-less block the size of what is loading.
pub fn skeleton(t: &Neko, key: &str, w: f32, h: f32) -> El {
    el().key(key.to_owned()).w(w).h(h).radius(6.0).fill(t.text.base.alpha(0.06))
}

/// A press that does nothing while `enabled` is false: the web's `disabled` button (the look is `.disabled(true).opacity(0.5)`).
pub fn gated(enabled: bool, f: impl Fn(&mut App) + 'static) -> impl Fn(&mut App) + 'static {
    move |s: &mut App| {
        if enabled {
            f(s);
        }
    }
}

/// The web's `Button` at its usual size: 36 high, 14 at the sides, a pill, 14 medium words, the icon 16 before them. `primary` is the
/// blue one; the others are the outline (and, selected, the secondary) button.
pub fn button(t: &Neko, key: &str, label: &str, glyph: Option<&'static Icon>, primary: bool, selected: bool, on_press: impl Fn(&mut App) + 'static) -> El {
    // Under 640 the web's `sm:text-sm` is not on: 16 point words and an 18 point icon.
    let big = page_width() < 640.0;
    let (size, lh, glyph_size) = if big { (16.0, 24.0, 18.0) } else { (14.0, 20.0, 16.0) };
    let ink = if primary { Color::WHITE } else { t.text.base };
    let fill = if primary { t.meaning.iris } else if selected { t.text.base.alpha(0.1) } else { t.text.base.alpha(0.065) };
    // With an icon the web's box is the 1 point border and 14 of padding each side, the icon pulled 2 points in at both ends (`[&_svg]:-mx-0.5`).
    let mut r = row().key(key.to_owned()).role("button").label(label).focusable(true).h(36.0).px(if glyph.is_some() { 15.0 } else { 14.0 }).gap(6.0).center().radius(18.0).fill(fill).hover(Color::WHITE.alpha(if primary { 0.14 } else { 0.05 })).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(on_press);
    if let Some(g) = glyph {
        r = r.child(icon(g, glyph_size).color(ink.alpha(0.8)).mx(-2.0).a11y_hidden());
    }
    r.child(text(label.to_owned()).font(font(t, size, 500.0, lh)).color(ink).a11y_hidden())
}

/// `Button variant="ghost" size="sm"`: no fill, the same words.
pub fn ghost_button(t: &Neko, key: &str, label: &str, glyph: Option<&'static Icon>, on_press: impl Fn(&mut App) + 'static) -> El {
    button(t, key, label, glyph, false, false, on_press).fill(Color::TRANSPARENT).radius(14.0)
}

/// A ghost button whose icon follows its words (the Rulebook's "View Neko’s built-in default" and its caret).
pub fn ghost_button_after(t: &Neko, key: &str, label: &str, glyph: &'static Icon, on_press: impl Fn(&mut App) + 'static) -> El {
    let big = page_width() < 640.0;
    let (size, lh, glyph_size) = if big { (16.0, 24.0, 18.0) } else { (14.0, 20.0, 16.0) };
    row()
        .key(key.to_owned())
        .role("button")
        .label(label)
        .focusable(true)
        .h(36.0)
        .px(14.0)
        .gap(6.0)
        .center()
        .radius(14.0)
        .hover(Color::WHITE.alpha(0.05))
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(on_press)
        .child(text(label.to_owned()).font(font(t, size, 500.0, lh)).color(t.text.base).a11y_hidden())
        .child(icon(glyph, glyph_size).color(t.text.base.alpha(0.8)).a11y_hidden())
}

/// The outline button with its icon after the words ("Open commits on GitHub" and its arrow).
pub fn button_after(t: &Neko, key: &str, label: &str, glyph: &'static Icon, on_press: impl Fn(&mut App) + 'static) -> El {
    let big = page_width() < 640.0;
    let (size, lh, glyph_size) = if big { (16.0, 24.0, 18.0) } else { (14.0, 20.0, 16.0) };
    row()
        .key(key.to_owned())
        .role("button")
        .label(label)
        .focusable(true)
        .h(36.0)
        .px(14.0)
        .gap(6.0)
        .center()
        .radius(18.0)
        .fill(t.text.base.alpha(0.065))
        .hover(Color::WHITE.alpha(0.05))
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(on_press)
        .child(text(label.to_owned()).font(font(t, size, 500.0, lh)).color(t.text.base).a11y_hidden())
        .child(icon(glyph, glyph_size).color(t.text.base.alpha(0.8)).a11y_hidden())
}

/// `Button variant="destructive-outline" size="sm"`: coral words on a faint wash with a hairline.
pub fn danger_button(t: &Neko, key: &str, label: &str, on_press: impl Fn(&mut App) + 'static) -> El {
    let big = page_width() < 640.0;
    let (size, lh) = if big { (16.0, 24.0) } else { (14.0, 20.0) };
    row()
        .key(key.to_owned())
        .role("button")
        .label(label)
        .focusable(true)
        .h(36.0)
        .px(14.0)
        .center()
        .radius(18.0)
        .fill(Color::WHITE.alpha(0.03))
        .border(1.0, Color::WHITE.alpha(0.09))
        .hover(t.meaning.coral.alpha(0.1))
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(on_press)
        .child(text(label.to_owned()).font(font(t, size, 500.0, lh)).color(t.meaning.coral).a11y_hidden())
}

/// The web's `Select` trigger in a settings row (`ui/select.tsx`): `width` wide, 36 high, a pill on the foreground at 5%, 15 at the
/// sides, the value in 13 point words at the left and the chevron at the right. `choose` hears the words picked.
#[allow(clippy::too_many_arguments)]
pub fn select(s: &App, t: &Neko, layer: &mut Vec<El>, key: &str, label: &str, options: &[String], current: &str, width: f32, choose: impl Fn(&mut App, &str) + Clone + 'static) -> El {
    let mut kit = t.kit.clone();
    kit.ty.ui_medium = font(t, 13.0, 400.0, 19.5);
    let (open_key, close_key) = (key.to_owned(), key.to_owned());
    Select::new(key, options)
        .current(Some(current))
        .label(label)
        .width(width)
        .open(s.pages.open_select.as_deref() == Some(key))
        .on_toggle(move |s: &mut App| s.toggle_select(&open_key))
        .on_close(move |s: &mut App| {
            if s.pages.open_select.as_deref() == Some(close_key.as_str()) {
                s.pages.open_select = None;
            }
        })
        .on_choose(move |s: &mut App, chosen: &str, _| {
            s.pages.open_select = None;
            choose(s, chosen);
        })
        .build(&kit, layer)
        .key(format!("pick-{key}"))
        .w(width)
        .h(36.0)
        .pl(15.0)
        .pr(15.0)
        .py(0.0)
        .gap(6.0)
        .radius(24.0)
        .fill(t.text.base.alpha(0.05))
        .justify_between()
}

/// `Tabs` (`ui/tabs.tsx`): a 42 high well (radius 10, 3 inside, tabs 2 apart) holding 36 high tabs with 9 at the sides, 16 point
/// (14 from 640) medium words, the open tab on a soft fill. `tabs` are (key, label, open, press).
pub type Tab = (String, String, bool, Box<dyn Fn(&mut App)>);

pub fn tabs(t: &Neko, tabs: Vec<Tab>) -> El {
    // Under 640 the web's `sm:` sizes are off: a 42 high well of 36 high tabs in 16 point words; from 640, 38 and 32 in 13 point words.
    let big = page_width() < 640.0;
    let (size, lh, well_h, tab_h) = if big { (16.0, 24.0, 42.0, 36.0) } else { (13.0, 19.5, 38.0, 32.0) };
    let mut well = row().key("tabs").role("tablist").h(well_h).padding(3.0).gap(2.0).center_y().radius(10.0).fill(t.text.base.alpha(0.06));
    for (key, label, open, press) in tabs {
        well = well.child(
            row()
                .key(key)
                .role("tab")
                .label(label.clone())
                .checked(open)
                .focusable(true)
                .h(tab_h)
                .px(10.0)
                .center()
                .radius(7.0)
                .fill(if open { Color::WHITE.alpha(0.09) } else { Color::TRANSPARENT })
                .hover(t.surface.hover)
                .focus_ring(2.0, 2.0, t.kit.c.border_focus)
                .on_press(move |s: &mut App| press(s))
                .child(text(label).font(font(t, size, 500.0, lh)).color(if open { t.text.base } else { t.text.muted.alpha(0.72) }).a11y_hidden()),
        );
    }
    well
}

/// `Section collapsible` (`kit.tsx`): the heading is a button (caret 11, title, faint count; 4 points left of the column, 4 at its
/// sides, 2 above and below) that folds the body; only an open section has a body. `flag` is the page's fold flag for it.
pub fn section_collapsible(t: &Neko, key: &str, title: &str, count: Option<&str>, open: bool, flag: &str, body: El) -> El {
    section_collapsible_keyed(t, key, title, count, open, flag, body)
}

/// [`section_collapsible`] whose section is named `key` (the heading button is `<key>-toggle`).
pub fn section_collapsible_keyed(t: &Neko, key: &str, title: &str, count: Option<&str>, open: bool, flag: &str, body: El) -> El {
    let flag = flag.to_owned();
    let mut label = row().gap(6.0).center_y().child(icon(if open { &crate::icons::CHEVRON_DOWN } else { &crate::icons::CHEVRON_RIGHT }, 11.0).color(t.text.muted).a11y_hidden()).child(text(title.to_owned()).font(font(t, 12.5, 500.0, 20.3125)).color(t.text.muted).a11y_hidden());
    if let Some(count) = count {
        label = label.child(text(count.to_owned()).font(font(t, 12.5, 500.0, 20.3125)).color(t.text.muted.alpha(0.7)).a11y_hidden());
    }
    let button = row().key(format!("{key}-toggle")).role("button").label(title.to_owned()).expanded(open).focusable(true).ml(-4.0).px(4.0).py(2.0).radius(10.0).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(move |s: &mut App| s.toggle_inbox_flag(&flag)).child(label);
    let head = row().w_full().min_h(28.0).gap(16.0).center_y().child(button);
    let mut c = col().key(key.to_owned()).w_full().gap(6.0).child(head);
    if open {
        c = c.child(body);
    }
    c
}

/// The web's `Switch` (`ui/switch.tsx`, `Toggle`): 36 x 22, the thumb 18 with 2 around it, Iris when on.
pub fn toggle(t: &Neko, key: &str, label: &str, on: bool, f: impl Fn(&mut App) + 'static) -> El {
    let thumb = if on { Color::WHITE } else { t.text.base };
    let track = if on { t.meaning.iris } else { t.text.base.alpha(0.18) };
    let r = row().key(key.to_owned()).role("switch").label(label.to_owned()).checked(on).focusable(true).size(36.0, 22.0).px(2.0).center_y().radius(11.0).fill(track).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(f).child(el().size(18.0, 18.0).radius(9.0).fill(thumb).a11y_hidden());
    if on { r.justify_end() } else { r.justify_start() }
}

/// An empty page's one button: its key, its words and what it does.
pub type EmptyButton<'a> = (&'a str, &'a str, Box<dyn Fn(&mut App)>);

/// `Empty` (`ui/empty.tsx`): centred in what is left of the page, 48 round it and 16 between its parts; a 40 point icon well (radius
/// 14, the foreground at 7%) with a 24 point icon, 8 below it, the title (18 medium), the description (14 on a 22.75 line, muted, at
/// most 384 wide) and, when there is one, a button.
pub fn empty(t: &Neko, key: &str, glyph: &'static Icon, title: &str, description: &str, button: Option<EmptyButton>) -> El {
    let well = row().size(40.0, 40.0).center().radius(14.0).fill(t.text.base.alpha(0.07)).mb(8.0).child(icon(glyph, 24.0).color(t.text.base).a11y_hidden());
    // `text-balance`: the description's lines come out about as long as each other, centred.
    let style = font(t, 14.0, 400.0, 22.75);
    let width = balanced_width(description, style, 384.0);
    let header = col().max_w(384.0).gap(8.0).items_center().child(well).child({
        // `tracking-tight`: -0.025em.
        let title_style = TextStyle { tracking: -0.45, ..font(t, 18.0, 500.0, 28.0) };
        let line = text(title.to_owned()).key(format!("{key}-title")).font(title_style).color(t.text.base);
        // One line sits in its own width (the web centres the text, the box hugs it); a longer one wraps.
        if rune::text::measure(title, &[], title_style, None).0 <= 384.0 { line } else { line.wrap().align(rune::Align::Center) }
    }).child(
        text(description.to_owned()).key(format!("{key}-description")).font(style).color(t.text.muted).wrap().align(rune::Align::Center).w(width),
    );
    let mut c = col().key(key.to_owned()).w_full().grow(1.0).min_h(0.0).padding(48.0).gap(16.0).center().items_center().child(header);
    if let Some((k, label, press)) = button {
        c = c.child(button_after_none(t, k, label, press));
    }
    c
}

/// The outline button without an icon (`Button variant="outline"`).
fn button_after_none(t: &Neko, key: &str, label: &str, on_press: Box<dyn Fn(&mut App)>) -> El {
    button(t, key, label, None, false, false, move |s: &mut App| on_press(s))
}

/// `Pick` (`setup/kit.tsx`) on a Setup page: the select trigger 176 wide at least and 36 high, a pill on the foreground at 5%, 14
/// at the sides; 16 point words under 640 and 13 from there.
#[allow(clippy::too_many_arguments)]
pub fn pick(s: &App, t: &Neko, layer: &mut Vec<El>, key: &str, label: &str, options: &[String], current: &str, choose: impl Fn(&mut App, &str) + Clone + 'static) -> El {
    let big = page_width() < 640.0;
    let mut kit = t.kit.clone();
    kit.ty.ui_medium = if big { font(t, 16.0, 400.0, 24.0) } else { font(t, 13.0, 400.0, 19.5) };
    let (open_key, close_key) = (key.to_owned(), key.to_owned());
    Select::new(key, options)
        .current(Some(current))
        .label(label)
        .width(176.0)
        .open(s.pages.open_select.as_deref() == Some(key))
        .on_toggle(move |s: &mut App| s.toggle_select(&open_key))
        .on_close(move |s: &mut App| {
            if s.pages.open_select.as_deref() == Some(close_key.as_str()) {
                s.pages.open_select = None;
            }
        })
        .on_choose(move |s: &mut App, chosen: &str, _| {
            s.pages.open_select = None;
            choose(s, chosen);
        })
        .build(&kit, layer)
        .key(format!("pick-{key}"))
        .w(176.0)
        .h(36.0)
        .pl(14.0)
        .pr(14.0)
        .py(0.0)
        .gap(6.0)
        .radius(24.0)
        .fill(t.text.base.alpha(0.05))
        .justify_between()
}

/// The words of a list row's secondary text (`text-[12.5px] text-muted-foreground/70`).
pub fn faint(t: &Neko, words: &str) -> El {
    text(words.to_owned()).font(font(t, 12.5, 400.0, 20.3125)).color(t.text.muted.alpha(0.7)).a11y_hidden()
}

/// [`faint`] words that are several text nodes of the web's JSX (`{kind}{" · source"}`): one line, each node a text of its own, so the
/// measure finds each where the browser puts it.
pub fn faint_nodes(t: &Neko, nodes: &[String]) -> El {
    row().children(nodes.iter().map(|n| text(n.clone()).font(font(t, 12.5, 400.0, 20.3125)).color(t.text.muted.alpha(0.7)).a11y_hidden()))
}

/// A setup list row of the web's pages (`ROW` in `Memory.tsx`, `Watching.tsx`): 6 above and below a line 32 high (taller when
/// something in it is), 12 at the sides, radius 10, a faint wash on hover; the row's own children go in `line`, 12 apart.
pub fn item_row(t: &Neko, key: &str, line: Vec<El>, below: Vec<El>) -> El {
    col().key(key.to_owned()).w_full().px(12.0).py(6.0).gap(4.0).radius(10.0).hover_group().hover(t.text.base.alpha(0.03)).child(row().w_full().min_h(32.0).gap(12.0).center_y().children(line)).children(below)
}

/// `OneLine` (`Memory.tsx`): the row's words as a button, one line cut off with an ellipsis, or all of them when `open`.
pub fn one_line(t: &Neko, key: &str, words: &str, open: bool, on_press: impl Fn(&mut App) + 'static) -> El {
    let style = font(t, 13.5, 400.0, 21.9375);
    let label = if open { text(words.to_owned()).font(style).color(t.text.base).wrap().min_w(0.0).shrink() } else { text(words.to_owned()).font(style).color(t.text.base).truncate().min_w(0.0).shrink() };
    row().key(key.to_owned()).role("button").label(words.to_owned()).expanded(open).focusable(true).grow(1.0).shrink().min_w(0.0).radius(10.0).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(on_press).child(label.a11y_hidden())
}

/// `RowActions` (`kit.tsx`): the icon buttons at a row's end, there while the row is hovered or holds focus; they keep their room.
pub fn row_actions(children: Vec<El>) -> El {
    row().key("row-actions").gap(2.0).center_y().children(children)
}

/// `RowAction`: a 28 point icon button of a row's actions (shows on the row's hover).
pub fn row_action(t: &Neko, key: &str, label: &str, glyph: &'static Icon, on_press: impl Fn(&mut App) + 'static) -> El {
    row().key(key.to_owned()).role("button").label(label.to_owned()).help(label.to_owned()).focusable(true).size(28.0, 28.0).center().radius(10.0).hover(t.text.base.alpha(0.07)).focus_ring(2.0, 2.0, t.kit.c.border_focus).group_opacity(0.0, 1.0).on_press(on_press).child(icon(glyph, 15.0).color(t.text.muted).a11y_hidden())
}

/// One entry of [`more_menu`]: its words, its icon, whether it is the destructive one, whether it can be used, and what it does.
/// `None` is a separator.
pub type MoreEntry = Option<(String, &'static Icon, bool, bool, std::rc::Rc<dyn Fn(&mut App)>)>;

/// The web's row menu (`Button variant="ghost" size="icon-sm"` with the three dots and a `DropdownMenu`): a 36 point round trigger.
/// In a row's actions it shows with the row's hover (`always` false); on a page it is always there. The fold flag `more:<key>` says
/// it is open.
pub fn more_menu(s: &App, t: &Neko, layer: &mut Vec<El>, key: &str, label: &str, always: bool, entries: Vec<MoreEntry>) -> El {
    let flag = format!("more:{key}");
    let open = s.row_open(&flag);
    let toggle = flag.clone();
    let mut trigger = row().key(key.to_owned()).role("button").label(label.to_owned()).help(label.to_owned()).focusable(true).size(36.0, 36.0).center().radius(18.0).hover(t.text.base.alpha(0.07)).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(move |s: &mut App| s.toggle_row(&toggle)).child(icon(&crate::icons_phosphor::DOTS_THREE_BOLD, 16.0).color(t.text.base).a11y_hidden());
    if !open && !always {
        trigger = trigger.group_opacity(0.0, 1.0);
    }
    let close = flag.clone();
    let mut menu = rune_kit::Menu::new(format!("{key}-menu")).trigger(trigger).open(open).placement(rune_kit::Placement::BottomEnd).min_width(180.0).on_close(move |s: &mut App| {
        if s.row_open(&close) {
            s.toggle_row(&close);
        }
    });
    for e in entries {
        match e {
            None => menu = menu.separator(),
            Some((words, glyph, danger, enabled, act)) => {
                let close = flag.clone();
                menu = menu.item(rune_kit::MenuItem::new(words).icon(glyph).danger(danger).disabled(!enabled).on_select(move |s: &mut App| {
                    if s.row_open(&close) {
                        s.toggle_row(&close);
                    }
                    act(s);
                }));
            }
        }
    }
    menu.build(&t.kit, layer)
}

/// `Button variant="ghost" size="xs"`: 32 high, 11 at the sides (the border and 10), a pill; 14 point words under 640 and 12 from there,
/// the icon 16 or 14 pulled 2 points in at both ends.
pub fn ghost_xs_button(t: &Neko, key: &str, label: &str, glyph: Option<&'static Icon>, help: &str, on_press: impl Fn(&mut App) + 'static) -> El {
    let big = page_width() < 640.0;
    let (size, lh, glyph_size) = if big { (14.0, 20.0, 16.0) } else { (12.0, 16.0, 14.0) };
    let mut r = row().key(key.to_owned()).role("button").label(label).help(help.to_owned()).focusable(true).h(32.0).px(11.0).gap(4.0).center().radius(16.0).hover(t.text.base.alpha(0.06)).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(on_press);
    if let Some(g) = glyph {
        r = r.child(icon(g, glyph_size).color(t.text.base.alpha(0.8)).mx(-2.0).a11y_hidden());
    }
    r.child(text(label.to_owned()).font(font(t, size, 500.0, lh)).color(t.text.base).a11y_hidden())
}

/// `Button variant="outline" size="xs"`: the ghost xs button on the popover fill (`dark:bg-input/32`) with the input border, 32 high.
pub fn outline_xs_button(t: &Neko, key: &str, label: &str, help: &str, on_press: impl Fn(&mut App) + 'static) -> El {
    ghost_xs_button(t, key, label, None, help, on_press).fill(t.text.base.alpha(0.06)).border(1.0, t.surface.field_border).px(10.0)
}

/// The web's `Textarea` with no fill or border of its own (`!border-0 !bg-transparent px-1`): 4 at the sides, 10 above and below,
/// 48 high at least, one line growing with the text; 16 point words under 640 and 13 from there.
pub fn bare_text_area(s: &App, t: &Neko, key: &str, label: &str, placeholder: &str) -> El {
    let big = page_width() < 640.0;
    let style = if big { font(t, 16.0, 400.0, 24.0) } else { font(t, 13.0, 400.0, 19.5) };
    let field = rune::text_input(super::ui::edit_of(s, key), style).key("input").w_full().color(t.text.base).placeholder(placeholder.to_owned(), t.text.muted).caret(t.text.base, t.kit.selection()).label(label.to_owned()).on_edit(super::ui::editing(key.to_owned()));
    col().key(key.to_owned()).role("textbox").w_full().min_h(48.0).px(4.0).py(10.0).radius(18.0).child(rune::auto_grow(field, 1, 10_000))
}

/// `Figure` (`setup/kit.tsx`): one calm line, 36 high at least (8 above and below), 12 at the sides; the label 160 wide and cut off,
/// the value in 13.5 point figures (coloured only when it needs the person) with an optional unit 4 after it, then a muted detail.
#[allow(clippy::too_many_arguments)]
pub fn figure(t: &Neko, key: &str, label: &str, value: &str, unit: Option<&str>, detail: Option<&str>, ink: Option<Color>, help: Option<&str>) -> El {
    let big = font(t, 13.5, 400.0, 21.9375);
    let small = font(t, 12.5, 400.0, 20.3125);
    let mut value_box = row().gap(4.0).child(text(value.to_owned()).key(format!("{key}-value")).font(tnum(big)).color(ink.unwrap_or(t.text.base)).a11y_hidden());
    if let Some(u) = unit.filter(|u| !u.is_empty()) {
        value_box = value_box.child(text(u.to_owned()).font(small).color(t.text.muted).a11y_hidden());
    }
    let mut r = row().key(key.to_owned()).w_full().min_h(36.0).px(12.0).py(8.0).gap(12.0).center_y().radius(10.0).hover(t.text.base.alpha(0.03)).child(text(label.to_owned()).font(big).color(t.text.base).truncate().w(160.0).a11y_hidden()).child(value_box);
    if let Some(d) = detail.filter(|d| !d.is_empty()) {
        r = r.child(text(d.to_owned()).key(format!("{key}-detail")).font(small).color(t.text.muted).truncate().grow(1.0).shrink().min_w(0.0).a11y_hidden());
    }
    if let Some(h) = help {
        r = r.help(h.to_owned());
    }
    r
}

/// `h3` of `GROUP_HEADING` after a panel (`mt-4`): 12.5 medium muted words, then an optional faint note after them.
pub fn group_heading(t: &Neko, key: &str, title: &str, note: Option<&str>) -> El {
    let style = font(t, 12.5, 500.0, 20.3125);
    let mut r = row().key(key.to_owned()).w_full().mt(16.0).child(text(title.to_owned()).font(style).color(t.text.muted));
    if let Some(n) = note {
        r = r.child(text(n.to_owned()).font(tnum(style)).color(t.text.muted.alpha(0.7)).ml(3.4));
    }
    r
}

/// `BarList` (`setup/kit.tsx`): ranked bars, each a name, a mono value and a 6 point track, in 12 of padding and 16 between.
pub fn bar_list(t: &Neko, key: &str, items: &[(String, usize, String)]) -> El {
    let max = items.iter().map(|i| i.1).max().unwrap_or(1).max(1) as f32;
    let body = font(t, 13.0, 400.0, 21.125);
    let mono = TextStyle { family: rune::Family::Mono, ..body };
    let mut c = col().key(key.to_owned()).w_full().px(12.0).py(12.0).gap(16.0);
    for (i, (name, n, words)) in items.iter().enumerate() {
        let line = row().w_full().gap(16.0).justify_between().child(text(name.clone()).key(format!("{key}-name-{i}")).font(body).color(t.text.base.alpha(0.9)).truncate().min_w(0.0).shrink().a11y_hidden()).child(text(words.clone()).key(format!("{key}-words-{i}")).font(tnum(mono)).color(t.text.muted).a11y_hidden());
        let track = row().w_full().h(6.0).radius(3.0).fill(t.text.base.alpha(0.1)).child(el().key(format!("{key}-fill-{i}")).h(6.0).w_frac(*n as f32 / max).radius(3.0).fill(t.text.base.alpha(0.6)));
        c = c.child(col().key(format!("{key}-{i}")).w_full().gap(6.0).child(line).child(track));
    }
    c
}

/// `text-wrap: balance`: the narrowest width, at most `max`, at which the words still take the lines they take at `max`.
pub fn balanced_width(words: &str, style: TextStyle, max: f32) -> f32 {
    let lines_at = |w: f32| rune::text::measure(words, &[], style, Some(w)).1;
    let full = lines_at(max);
    if full <= style.line_height * 1.5 {
        return rune::text::measure(words, &[], style, Some(max)).0.min(max);
    }
    let (mut lo, mut hi) = (max * 0.4, max);
    for _ in 0..12 {
        let mid = (lo + hi) / 2.0;
        if lines_at(mid) > full + 0.5 { lo = mid } else { hi = mid }
    }
    hi.ceil()
}

/// [`empty`] whose description is any element (words with an inline code span) and that holds more under it: `extras` between the
/// description and the button, and the button itself as built by the caller (the web's `EmptyContent`).
#[allow(clippy::too_many_arguments)]
pub fn empty_rich(t: &Neko, key: &str, glyph: &'static Icon, title: &str, description: El, extras: Vec<El>, button: Option<El>) -> El {
    let well = row().size(40.0, 40.0).center().radius(14.0).fill(t.text.base.alpha(0.07)).mb(8.0).child(icon(glyph, 24.0).color(t.text.base).a11y_hidden());
    let title_style = TextStyle { tracking: -0.45, ..font(t, 18.0, 500.0, 28.0) };
    let header = col().max_w(384.0).gap(8.0).items_center().child(well).child(text(title.to_owned()).key(format!("{key}-title")).font(title_style).color(t.text.base)).child(description);
    let mut c = col().key(key.to_owned()).w_full().padding(48.0).gap(16.0).center().items_center().child(header);
    for e in extras {
        c = c.child(e);
    }
    if let Some(b) = button {
        c = c.child(b);
    }
    c
}

/// Words with an inline code span, laid out the way a browser flows the JSX's text nodes: balanced (`text-balance`) lines of at most
/// `max_w`, each centred, every node a text of its own on each line it touches (the code in the mono face, a point lower as a
/// baseline puts it). `nodes` are (words, is_code); spaces stay where the JSX has them.
pub fn inline_flow(t: &Neko, key: &str, nodes: &[(&str, bool)], style: TextStyle, code_style: TextStyle, max_w: f32) -> El {
    let plain: String = nodes.iter().map(|n| n.0).collect();
    let width = balanced_width(&plain, style, max_w);
    let style_of = |code: bool| if code { code_style } else { style };
    // Each word keeps the space after it; a line breaks before the word that no longer fits (its own trailing space hangs).
    let mut lines: Vec<Vec<(usize, String)>> = vec![Vec::new()];
    let mut used = 0.0f32;
    for (n, (words, code)) in nodes.iter().enumerate() {
        for word in words.split_inclusive(' ') {
            let w = rune::text::measure(word.trim_end(), &[], style_of(*code), None).0;
            let full = rune::text::measure(word, &[], style_of(*code), None).0;
            if used + w > width + 0.5 && !lines.last().is_some_and(Vec::is_empty) {
                if let Some(last) = lines.last_mut().and_then(|l| l.last_mut()) {
                    last.1 = last.1.trim_end().to_owned();
                }
                lines.push(Vec::new());
                used = 0.0;
            }
            if used == 0.0 && word.trim().is_empty() {
                continue;
            }
            if let Some(line) = lines.last_mut() {
                match line.last_mut() {
                    Some((m, run)) if *m == n => run.push_str(word),
                    _ => line.push((n, word.to_owned())),
                }
            }
            used += full;
        }
    }
    let mut c = col().key(key.to_owned()).items_center();
    for (i, line) in lines.into_iter().enumerate() {
        let last = line.len().saturating_sub(1);
        let runs = line.into_iter().enumerate().map(|(j, (n, run))| {
            let code = nodes[n].1;
            let run = if j == last { run.trim_end().to_owned() } else { run };
            let e = text(run).font(style_of(code)).color(t.text.muted).a11y_hidden();
            if code { e.mt(3.0) } else { e }
        });
        c = c.child(row().key(format!("{key}-line-{i}")).center_y().children(runs));
    }
    c
}
