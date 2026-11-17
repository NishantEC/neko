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

/// The web's text at a size: Inter, the given weight, the line height the browser measured.
pub fn font(t: &Neko, size: f32, weight: f32, line_height: f32) -> TextStyle {
    TextStyle { size, weight, line_height, ..t.ty.row }
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
#[allow(dead_code)]
pub enum Tone {
    Neutral,
    Success,
    Warning,
    Error,
}

/// `Status` (`kit.tsx`, `ui/status-badge.tsx`): 23 high, 7 at the sides, radius 10, a 6 point dot and 11 point medium words.
pub fn status(t: &Neko, key: &str, tone: Tone, words: &str) -> El {
    let (dot, wash, ink) = match tone {
        Tone::Neutral => (t.text.muted.alpha(0.6), t.text.base.alpha(0.06), t.text.muted),
        Tone::Success => (t.meaning.mint, t.meaning.mint.alpha(0.1), t.meaning.mint),
        Tone::Warning => (t.meaning.amber, t.meaning.amber.alpha(0.1), t.meaning.amber),
        Tone::Error => (t.meaning.coral, t.meaning.coral.alpha(0.1), t.meaning.coral),
    };
    row().key(key.to_owned()).h(23.0).px(7.0).gap(6.0).center_y().radius(10.0).fill(wash).child(el().size(6.0, 6.0).radius(3.0).fill(dot).a11y_hidden()).child(text(words.to_owned()).font(font(t, 11.0, 500.0, 17.0)).color(ink))
}

/// `PanelFooter`: a quiet line at the left and the actions at the right (under them when they do not fit); 12 at the sides, 4 above
/// and 20 below (the footer's own padding in the rendered page).
pub fn panel_footer(t: &Neko, key: &str, notes: &[String], buttons: Vec<El>) -> El {
    // Each `{...}` of the web's JSX is a text node of its own, 4 apart (a space): they flow and wrap as one line.
    let line = row().min_w(0.0).gaps(0.0, 4.0).flex_wrap().children(notes.iter().map(|n| text(n.clone()).font(font(t, 12.0, 400.0, 16.0)).color(t.text.muted).wrap().min_w(0.0)));
    row().key(key.to_owned()).w_full().px(12.0).pt(4.0).pb(20.0).gaps(8.0, 16.0).flex_wrap().center_y().justify_between().child(line).child(row().gap(8.0).center_y().children(buttons))
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
    let mut r = row().key(key.to_owned()).role("button").label(label).focusable(true).h(36.0).px(14.0).gap(6.0).center().radius(18.0).fill(fill).hover(Color::WHITE.alpha(if primary { 0.14 } else { 0.05 })).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(on_press);
    if let Some(g) = glyph {
        r = r.child(icon(g, glyph_size).color(ink.alpha(0.8)).a11y_hidden());
    }
    r.child(text(label.to_owned()).font(font(t, size, 500.0, lh)).color(ink).a11y_hidden())
}

/// `Button variant="ghost" size="sm"`: no fill, the same words.
pub fn ghost_button(t: &Neko, key: &str, label: &str, glyph: Option<&'static Icon>, on_press: impl Fn(&mut App) + 'static) -> El {
    button(t, key, label, glyph, false, false, on_press).fill(Color::TRANSPARENT).radius(14.0)
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
    let (size, lh) = if page_width() < 640.0 { (16.0, 24.0) } else { (14.0, 20.0) };
    let mut well = row().key("tabs").role("tablist").h(42.0).padding(3.0).gap(2.0).center_y().radius(10.0).fill(t.text.base.alpha(0.06));
    for (key, label, open, press) in tabs {
        well = well.child(
            row()
                .key(key)
                .role("tab")
                .label(label.clone())
                .checked(open)
                .focusable(true)
                .h(36.0)
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
