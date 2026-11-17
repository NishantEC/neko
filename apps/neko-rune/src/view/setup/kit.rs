//! The pieces the Setup sections are made of, on the existing page conventions (`view/ui.rs`): a 32 point
//! row with hover actions that mirror its right-click menu, a group heading with its count, the empty state
//! (one quiet line and one action), dialogs, the coral button and labelled fields.

use rune::{El, col, el, icon, row, text};
use rune_kit::{Checkbox, Dialog as KitDialog, TextArea, TextInput};

use super::super::ui::{edit_of, editing, note};
use super::super::{primary_button, quiet_button};
use crate::app::App;
use crate::status::{StatusKind, status_glyph};
use crate::theme::Neko;

pub const ROW_H: f32 = 32.0;
/// The web's `Row` (`setup/kit.tsx`): 8 above and below a 28 point line, 12 at the sides, and the hover wash bleeding 12 past the column.
pub const WEB_ROW_H: f32 = 44.0;

/// A list row: 32 high, 8 of padding, radius 8, hover fill; its hover actions appear on hover or focus.
pub fn list_row(t: &Neko, key: String, label: String, open: Option<bool>) -> El {
    let r = row().key(key).role("button").label(label).focusable(true).mx(-12.0).min_h(WEB_ROW_H).px(12.0).gap(12.0).center_y().radius(10.0).hover_group().hover(t.surface.hover).focus_ring(2.0, 2.0, t.surface.ring);
    match open {
        Some(open) => r.expanded(open),
        None => r,
    }
}

/// A group heading: 12.5 medium muted with the count right after it, 32 high.
pub fn heading(t: &Neko, key: &str, label: &str, count: Option<usize>, first: bool) -> El {
    let mut r = row().key(key.to_owned()).w_full().h(28.0).mt(if first { 0.0 } else { 32.0 }).mb(6.0).gap(6.0).center_y().child(text(label.to_owned()).font(t.ty.group).color(t.text.muted).truncate().min_w(0.0));
    if let Some(n) = count {
        r = r.child(text(n.to_string()).key(format!("{key}-count")).font(t.ty.meta).color(t.text.faint));
    }
    r
}

/// A row's hover action: a 28 point icon button that shows while the row is hovered or focused.
pub fn action(t: &Neko, key: String, label: &str, glyph: &'static rune::Icon, on_press: impl Fn(&mut App) + 'static) -> El {
    row().key(key).role("button").label(label.to_owned()).help(label.to_owned()).focusable(true).size(28.0, 28.0).center().radius(t.radius.control).hover(t.surface.active).focus_ring(2.0, 2.0, t.surface.ring).group_opacity(0.0, 1.0).on_press(on_press).child(icon(glyph, 16.0).color(t.text.muted).a11y_hidden())
}

/// The empty state: one quiet line and one action (DESIGN.md §8 rule 11).
/// What an empty state's one action does.
pub type Act = Box<dyn Fn(&mut App)>;

pub fn empty(t: &Neko, key: &str, line: &str, act: Option<(&str, Act)>) -> El {
    let mut r = row().key(key.to_owned()).w_full().min_h(28.0).gap(12.0).center_y().child(text(line.to_owned()).font(rune::TextStyle { size: 13.0, line_height: 20.0, ..t.ty.row }).color(t.text.muted).wrap().min_w(0.0));
    if let Some((label, f)) = act {
        r = r.child(quiet_button(label, &format!("{key}-action"), t, f));
    }
    r
}

/// A line of muted words with a state glyph, for what a row says about itself.
pub fn glyph(t: &Neko, kind: StatusKind, words: &str) -> El {
    status_glyph(kind, t, 14.0, Some(words))
}

/// The coral button: what destroys something (the confirm of a delete or a forget).
pub fn coral_button(label: &str, key: &str, t: &Neko, on_press: impl Fn(&mut App) + 'static) -> El {
    row()
        .key(key.to_owned())
        .role("button")
        .label(label.to_owned())
        .focusable(true)
        .h(28.0)
        .px(12.0)
        .radius(t.radius.control)
        .center()
        .fill(t.meaning.coral)
        .hover(rune::Color::WHITE.alpha(0.14))
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(on_press)
        .child(text(label.to_owned()).font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(rune::Color::WHITE).a11y_hidden())
}

/// A label above its control, for the dialogs' forms.
pub fn field(t: &Neko, key: &str, label: &str, hint: Option<&str>, control: El) -> El {
    let mut c = col().key(format!("field-{key}")).w_full().gap(6.0).child(text(label.to_owned()).font(t.ty.meta).color(t.text.muted));
    c = c.child(control);
    if let Some(h) = hint {
        c = c.child(text(h.to_owned()).font(t.ty.meta).color(t.text.faint).wrap());
    }
    c
}

/// A one-line text field across the dialog.
#[allow(clippy::too_many_arguments)]
pub fn text_field(s: &App, t: &Neko, key: &str, label: &str, placeholder: &str, mono: bool, secure: bool, submit: impl Fn(&mut App) + 'static) -> El {
    let mut input = TextInput::new(edit_of(s, key)).label(label).placeholder(placeholder).width(None).on_edit(editing(key.to_owned())).on_submit(move |s: &mut App, _| submit(s));
    if mono {
        input = input.text_style(t.ty.mono);
    }
    if secure {
        input = input.secure();
    }
    input.build(&t.kit).key(key.to_owned()).w_full()
}

/// A few lines of text across the dialog.
pub fn text_area(s: &App, t: &Neko, key: &str, label: &str, placeholder: &str, rows: usize) -> El {
    TextArea::new(edit_of(s, key)).label(label).placeholder(placeholder).rows(rows).on_edit(editing(key.to_owned())).build(&t.kit).key(key.to_owned()).w_full().min_w(0.0)
}

/// A checkbox with its words.
pub fn check(t: &Neko, key: &str, label: &str, on: bool, f: impl Fn(&mut App) + 'static) -> El {
    Checkbox::new(on).label(label).on_toggle(f).build(&t.kit).key(key.to_owned())
}

/// The buttons at the foot of a dialog or a form, at the right.
pub fn footer(buttons: Vec<El>) -> El {
    row().w_full().gap(8.0).justify_end().center_y().pt(4.0).children(buttons)
}

/// A modal dialog: its body scrolls when the window is short.
#[allow(clippy::too_many_arguments)]
pub fn dialog(t: &Neko, layer: &mut Vec<El>, key: &str, title: &str, description: Option<&str>, width: f32, win_h: f32, body: El) {
    dialog_with_footer(t, layer, key, title, description, width, win_h, body, None);
}

/// A modal dialog whose buttons stay in view: the body scrolls and the footer (the buttons) sits below it, so a long preview never
/// pushes the person's yes out of reach.
#[allow(clippy::too_many_arguments)]
pub fn dialog_with_footer(t: &Neko, layer: &mut Vec<El>, key: &str, title: &str, description: Option<&str>, width: f32, win_h: f32, body: El, foot: Option<El>) {
    // The description wraps inside the body (the kit's header line does not wrap a long one).
    let mut inner = col().w_full().gap(12.0);
    if let Some(desc) = description {
        inner = inner.child(text(desc.to_owned()).key(format!("{key}-description")).font(t.ty.meta).color(t.text.muted).wrap().min_w(0.0));
    }
    let scroll = col().key(format!("{key}-body")).w_full().max_h((win_h - 240.0).max(160.0)).scroll(rune::ScrollSpec { stick_bottom: false }).child(body);
    let mut inner = inner.child(scroll);
    if let Some(foot) = foot {
        inner = inner.child(foot);
    }
    let d = KitDialog::new(key.to_owned(), title.to_owned()).open(true).width(width).fit().child(col().w_full().px(16.0).pb(16.0).gap(12.0).child(inner)).on_close(|s: &mut App| s.close_dialog());
    d.build(&t.kit, layer);
}

/// The refusal shown at the foot of a form, when the daemon or the form said no.
pub fn form_note(s: &App, t: &Neko) -> Option<El> {
    note(s, t, "setup:form")
}

/// The Cancel and the form's one primary button.
pub fn form_buttons(s: &App, t: &Neko, save_label: &str, key: &str, enabled: bool, save: impl Fn(&mut App) + 'static) -> El {
    let enabled = enabled && s.setup.awaiting.is_none();
    let cancel = quiet_button("Cancel", &format!("{key}-cancel"), t, |s: &mut App| s.close_dialog());
    let main = if enabled { primary_button(save_label, &format!("{key}-save"), t, save) } else { primary_button(save_label, &format!("{key}-save"), t, |_| {}).disabled(true).fill(t.meaning.iris.alpha(0.4)) };
    footer(vec![cancel, main])
}

/// A `label value` line in a details block.
pub fn detail(t: &Neko, key: &str, label: &str, value: &str, mono: bool) -> El {
    let v = text(value.to_owned()).key(format!("{key}-value")).font(if mono { t.ty.mono_meta } else { t.ty.meta }).color(t.text.base).wrap().grow(1.0).min_w(0.0);
    row().key(key.to_owned()).w_full().min_h(24.0).px(8.0).gap(12.0).child(text(label.to_owned()).font(t.ty.meta).color(t.text.muted).w(112.0)).child(v)
}

/// The block under an open row: indented past its glyph.
pub fn details_block(key: String) -> El {
    col().key(key).w_full().pl(36.0).pr(8.0).pt(2.0).pb(10.0).gap(4.0)
}

/// A muted line of words (details, hints).
pub fn words(t: &Neko, key: &str, line: &str) -> El {
    text(line.to_owned()).key(key.to_owned()).font(t.ty.meta).color(t.text.muted).wrap().px(8.0).min_w(0.0)
}

/// An amber line (what needs attention).
pub fn amber(t: &Neko, key: &str, line: &str) -> El {
    text(line.to_owned()).key(key.to_owned()).font(t.ty.meta).color(t.meaning.amber).wrap().px(8.0).min_w(0.0)
}

/// An invisible spacer that takes what is left of a row.
pub fn fill() -> El {
    el().grow(1.0).min_w(0.0)
}
