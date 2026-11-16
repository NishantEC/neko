//! The one composer card: the session's reply box, Home's new-session box and Neko's ask box are this component
//! (the parity pass of 2026-11-16). Radius 16, a 1 px border at the foreground's 10%, padding 16, the text area at least
//! three lines, and a bottom bar: "+" attach and whatever else belongs at the left, then at the right the mode control
//! (steer mode in a session, Ask/Plan for Neko, Plan/Build on Home), the model chip and a 32 point round Send that is
//! filled Iris when there is text to send and the foreground at 10% when there is none.

use rune::{Color, El, Icon, KeyEvent, TextEdit, TextStyle, col, el, icon, row, text};

use crate::app::App;
use crate::icons;
use crate::theme::Neko;

/// The card's radius.
pub const RADIUS: f32 = 16.0;
/// The text area's fewest lines.
pub const MIN_LINES: usize = 3;

/// The card: `field` (a text area built without a fill), then the bar. `key` names it.
pub fn card(t: &Neko, key: &str, field: El, left: Vec<El>, right: Vec<El>, send: El) -> El {
    let mut bar = row().key(format!("{key}-bar")).w_full().gap(4.0).center_y();
    for e in left {
        bar = bar.child(e);
    }
    bar = bar.child(el().grow(1.0).min_w(0.0));
    for e in right {
        bar = bar.child(e);
    }
    col().key(key.to_owned()).w_full().padding(16.0).gap(12.0).radius(RADIUS).fill(t.text.base.alpha(0.04)).border(1.0, t.surface.field_border).child(field).child(bar.child(send))
}

/// A 32 point square icon button (the "+" attach).
pub fn square(t: &Neko, glyph: &'static Icon, label: &str, key: &str) -> El {
    row().key(key.to_owned()).role("button").label(label).help(label).focusable(true).size(32.0, 32.0).center().radius(t.radius.control).hover(t.surface.hover).focus_ring(2.0, 2.0, t.surface.ring).child(icon(glyph, 16.0).color(t.text.muted).a11y_hidden())
}

/// The "+" that opens a file dialog for an image and hands the path to `attach`.
pub fn attach(t: &Neko, key: &str, attach: fn(&mut App, &std::path::Path)) -> El {
    square(t, &icons::PLUS, "Attach an image", key).on_press_tx(move |_: &mut App, tx: &mut rune::Tx| {
        tx.open_file(rune::FileDialogOptions { title: "Attach an image".into(), ..Default::default() }.filter("Images", &["png", "jpg", "jpeg", "gif", "webp"]), move |s: &mut App, answer, _| {
            if let Ok(Some(path)) = answer {
                attach(s, &path);
            }
        });
    })
}

/// What the composer's agent runs on, as a quiet chip: 12.5 muted.
pub fn chip(t: &Neko, key: &str, words: &str) -> El {
    row().key(key.to_owned()).h(32.0).px(8.0).center_y().child(text(words.to_owned()).font(t.ty.meta).color(t.text.muted).truncate().min_w(0.0).a11y_hidden())
}

/// One half of a two-way control (Ask/Plan, Plan/Build).
pub fn seg(t: &Neko, key: &str, label: &str, aria: &str, help: &str, on: bool, f: impl Fn(&mut App) + 'static) -> El {
    row()
        .key(key.to_owned())
        .role("button")
        .label(aria.to_owned())
        .help(help.to_owned())
        .checked(on)
        .focusable(true)
        .h(26.0)
        .px(10.0)
        .center()
        .radius(t.radius.control - 2.0)
        .fill(if on { t.surface.active } else { Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(f)
        .child(text(label.to_owned()).font(TextStyle { weight: if on { 500.0 } else { 400.0 }, ..t.ty.meta }).color(if on { t.text.base } else { t.text.muted }).a11y_hidden())
}

/// The frame of a two-way control.
pub fn segmented(t: &Neko, key: &str, aria: &str, a: El, b: El) -> El {
    row().key(key.to_owned()).role("group").label(aria.to_owned()).h(32.0).padding(3.0).gap(2.0).center_y().radius(t.radius.control + 2.0).border(1.0, t.surface.field_border).child(a).child(b)
}

/// The Send button: a 32 point circle, Iris with a white arrow when there is something to send, the foreground at 10%
/// with a muted arrow when there is not. A press with nothing to send does nothing, unless `press_anyway` (Home's Start,
/// which says what is missing).
pub fn send(t: &Neko, key: &str, label: &str, help: &str, can: bool, press_anyway: bool, f: impl Fn(&mut App) + 'static) -> El {
    row()
        .key(key.to_owned())
        .role("button")
        .label(label.to_owned())
        .help(help.to_owned())
        .focusable(true)
        .size(32.0, 32.0)
        .center()
        .radius(16.0)
        .fill(if can { t.meaning.iris } else { t.text.base.alpha(0.10) })
        .hover(Color::WHITE.alpha(if can { 0.14 } else { 0.0 }))
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(move |s: &mut App| {
            if can || press_anyway {
                f(s);
            }
        })
        .child(icon(&icons::ARROW_UP, 16.0).color(if can { Color::WHITE } else { t.text.muted }).a11y_hidden())
}

/// The text area every composer shares: Rune's own multi-line text input (the input method composes in place, an image
/// pasted is kept and referenced), no field chrome of its own (the card is the box), `MIN_LINES` to `max_lines` tall.
/// `inner` keys the input, `outer` the box around it.
#[allow(clippy::too_many_arguments)]
pub fn field(t: &Neko, edit: &TextEdit, outer: &str, inner: &str, label: &str, placeholder: &str, max_lines: usize, access: fn(&mut App) -> &mut TextEdit, on_key: impl Fn(&mut App, &KeyEvent) -> bool + 'static) -> El {
    let k = &t.kit;
    let style = TextStyle { line_height: (13.0f32 * 1.35 * 2.0).round() / 2.0, ..k.ty.ui };
    let input = rune::auto_grow(
        rune::text_input(edit, style)
            .key(inner.to_owned())
            .w_full()
            .color(t.text.base)
            .placeholder(placeholder.to_owned(), t.text.faint)
            .caret(t.text.base, k.selection())
            .label(label.to_owned())
            .on_edit(access)
            .on_input_key(move |s: &mut App, ev: &KeyEvent, _| on_key(s, ev))
            .on_input_paste(move |s: &mut App, content, _| {
                let Some(image) = content.image() else { return false };
                match crate::attachments::save_png(image.png()) {
                    Ok(path) => access(s).insert(&crate::attachments::reference(&path)),
                    Err(_) => s.work.notice = Some("Couldn’t keep that image. Your message is unchanged.".to_owned()),
                }
                true
            }),
        MIN_LINES,
        max_lines,
    );
    col().key(outer.to_owned()).role("textbox").w_full().px(2.0).py(4.0).child(input)
}
