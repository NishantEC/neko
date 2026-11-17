//! The web's own kit, in Rune (`web/src/setup/kit.tsx`, `ui/button.tsx`): the pieces a settings page is made of, with the web's
//! measures, so a page built from them lines up with the web's at the same width (docs/knowledge/rune-identity.md).
//!
//! Every number here was read off the web page in embed mode (`scripts/identity-dom.mjs`), not guessed.

use rune::{Color, El, Icon, TextStyle, col, el, icon, row, text};

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

/// `SettingRow`: the title (13.5) and under it the description (12.5 muted) at the left, the control at the right; 48 high at least,
/// 12 at the sides, 10 above and below, radius 10.
pub fn setting_row(t: &Neko, key: &str, title: &str, description: Option<&str>, control: El) -> El {
    let mut left = col().min_w(0.0).grow(1.0).gap(2.0).child(text(title.to_owned()).font(font(t, 13.5, 400.0, 21.9375)).color(t.text.base).wrap().min_w(0.0));
    if let Some(d) = description {
        left = left.child(text(d.to_owned()).font(font(t, 12.5, 400.0, 20.3125)).color(t.text.muted).wrap().min_w(0.0));
    }
    row().key(key.to_owned()).min_h(48.0).px(12.0).py(10.0).gap(16.0).center_y().radius(10.0).child(left).child(row().gap(8.0).center_y().child(control))
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
