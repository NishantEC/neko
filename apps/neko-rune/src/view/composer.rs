//! The one composer card: the session's reply box, Home's new-session box and Neko's ask box are this component
//! (the parity pass of 2026-11-16). Radius 16, a 1 px border at the foreground's 10%, padding 16, the text area at least
//! three lines, and a bottom bar: "+" attach and whatever else belongs at the left, then at the right the mode control
//! (steer mode in a session, Ask/Plan for Neko, Plan/Build on Home), the model chip and a 32 point round Send that is
//! filled Iris when there is text to send and the foreground at 10% when there is none.

use rune::{Color, El, Icon, KeyEvent, TextEdit, TextStyle, col, el, icon, row, text};

use crate::app::App;
use crate::compose::{ChipState, Dest, size_words};
use crate::icons;
use crate::theme::Neko;

/// The card's radius.
pub const RADIUS: f32 = 28.0;
/// The text area's fewest lines.
pub const MIN_LINES: usize = 3;

/// The card: `field` (a text area built without a fill), then the bar. `key` names it.
pub fn card(t: &Neko, key: &str, field: El, left: Vec<El>, right: Vec<El>, send: El) -> El {
    card_on(t, key, None, field, left, right, send)
}

/// [`card`], optionally on glass (Home): the card has no fill or border of its own then; its first child is the glass
/// backing (a blur of what is behind, the tint, the border and the top highlight) and the text and controls draw above it.
/// `glass` is how the backing is made, and `probe` leaves it out (a test hook that shows the raw art).
pub fn card_on(t: &Neko, key: &str, glass: Option<(crate::glass::Mode, bool)>, field: El, left: Vec<El>, right: Vec<El>, send: El) -> El {
    let mut bar = row().key(format!("{key}-bar")).w_full().min_w(0.0).gap(4.0).center_y().flex_wrap();
    for e in left {
        bar = bar.child(e);
    }
    bar = bar.child(el().grow(1.0).min_w(0.0));
    for e in right {
        bar = bar.child(e);
    }
    let card = col().key(key.to_owned()).w_full().padding(16.0).gap(12.0).radius(RADIUS);
    match glass {
        // Glass over the page, the floating drop shadow, no border (design system 4.9).
        None => card
            .fill(t.surface.window)
            .gradient(t.mat.glass_top, t.mat.glass_bottom)
            .box_shadow(rune::Shadow { y: 24.0, blur: 60.0, spread: -18.0, color: t.mat.drop_far, ..rune::Shadow::default() })
            .shadow(2.0, 6.0, t.mat.drop_near)
            .child(field)
            .child(bar.child(send)),
        Some((_, true)) => card.child(field).child(bar.child(send)),
        Some((mode, false)) => {
            let layer = if mode == crate::glass::Mode::Blur { crate::glass::CARD_GLASS } else { crate::glass::CARD_OPAQUE };
            card.child(crate::glass::backing(t, mode, RADIUS, layer, true)).child(field).child(bar.child(send))
        }
    }
}

/// A 32 point square icon button (the "+" attach).
pub fn square(t: &Neko, glyph: &'static Icon, label: &str, key: &str) -> El {
    row().key(key.to_owned()).role("button").label(label).help(label).focusable(true).size(32.0, 32.0).center().radius(t.radius.control).hover(t.surface.hover).focus_ring(2.0, 2.0, t.surface.ring).child(icon(glyph, 16.0).color(t.text.muted).a11y_hidden())
}

/// The "+" that opens a file dialog for any file: each file chosen becomes a chip above the text and is uploaded.
pub fn attach(t: &Neko, key: &str, dest: Dest) -> El {
    square(t, &icons::PLUS, "Attach a file", key).on_press_tx(move |_: &mut App, tx: &mut rune::Tx| {
        tx.open_file(rune::FileDialogOptions { title: "Attach a file".into(), ..Default::default() }, move |s: &mut App, answer, _| {
            if let Ok(Some(path)) = answer {
                s.attach_files(dest, &[path]);
            }
        });
    })
}

/// The files attached to a draft, as chips above its text: an icon, the name, the size and a x. A file still on its way says
/// so; one the daemon refused says why, in coral, and can be taken away.
pub fn chips(s: &App, t: &Neko, dest: Dest) -> Option<El> {
    let list = s.chips(dest);
    if list.is_empty() {
        return None;
    }
    let mut r = row().key(format!("chips-{dest:?}").to_lowercase()).w_full().min_w(0.0).gap(6.0).flex_wrap().px(2.0).pb(4.0);
    for chip in list {
        let id = chip.id;
        let failed = matches!(chip.state, ChipState::Failed(_));
        let words = match &chip.state {
            ChipState::Uploading => "Uploading…".to_owned(),
            ChipState::Ready { .. } => size_words(chip.size),
            ChipState::Failed(why) => why.clone(),
        };
        r = r.child(
            row()
                .key(format!("chip-{id}"))
                .shrink()
                .role("group")
                .label(format!("{}, {words}", chip.name))
                .h(28.0)
                .pl(8.0)
                .pr(2.0)
                .gap(6.0)
                .center_y()
                .radius(8.0)
                .fill(t.text.base.alpha(0.04))
                .border(1.0, if failed { t.meaning.coral.alpha(0.5) } else { t.surface.field_border })
                .child(icon(&icons::FILE, 14.0).color(if failed { t.meaning.coral } else { t.text.muted }).a11y_hidden())
                .child(text(chip.name.clone()).key(format!("chip-name-{id}")).font(t.ty.meta).color(t.text.base).truncate().max_w(180.0).min_w(0.0).a11y_hidden())
                .child(text(words).key(format!("chip-size-{id}")).font(t.ty.id).color(if failed { t.meaning.coral } else { t.text.faint }).truncate().max_w(220.0).min_w(0.0).a11y_hidden())
                .child(
                    row()
                        .key(format!("chip-x-{id}"))
                        .role("button")
                        .label(format!("Remove {}", chip.name))
                        .focusable(true)
                        .size(24.0, 24.0)
                        .center()
                        .radius(6.0)
                        .hover(t.surface.hover)
                        .focus_ring(2.0, 2.0, t.surface.ring)
                        .on_press(move |s: &mut App| s.remove_chip(dest, id))
                        .child(icon(&icons::CLOSE_SMALL, 14.0).color(t.text.muted).a11y_hidden()),
                ),
        );
    }
    Some(r)
}

/// The text area with the files above it.
pub fn with_chips(s: &App, t: &Neko, dest: Dest, field: El) -> El {
    match chips(s, t, dest) {
        Some(c) => col().w_full().child(c).child(field),
        None => field,
    }
}

/// One half of a two-way control (Ask/Plan, Plan/Build): the open one is a pebble.
pub fn seg(t: &Neko, key: &str, label: &str, aria: &str, help: &str, on: bool, f: impl Fn(&mut App) + 'static) -> El {
    let r = row()
        .key(key.to_owned())
        .role("button")
        .label(aria.to_owned())
        .help(help.to_owned())
        .checked(on)
        .focusable(true)
        .h(26.0)
        .px(12.0)
        .center()
        .radius(13.0)
        .hover(if on { Color::TRANSPARENT } else { t.surface.hover })
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(f);
    let r = if on { super::webkit::pebble(t, r) } else { r };
    r.child(text(label.to_owned()).font(TextStyle { weight: 500.0, ..t.ty.meta }).color(if on { t.mat.pebble_fg } else { t.text.muted }).a11y_hidden())
}

/// The frame of a two-way control: a well.
pub fn segmented(t: &Neko, key: &str, aria: &str, a: El, b: El) -> El {
    super::webkit::well(t, row().key(key.to_owned()).role("group").label(aria.to_owned()).h(32.0).padding(3.0).gap(2.0).center_y().radius(16.0)).child(a).child(b)
}

/// The Send button: a 36 point circle, the ink with its arrow when there is something to send, a pebble with a muted arrow when there is
/// not. A press with nothing to send does nothing, unless `press_anyway` (Home's Start, which says what is missing).
pub fn send(t: &Neko, key: &str, label: &str, help: &str, can: bool, press_anyway: bool, f: impl Fn(&mut App) + 'static) -> El {
    let r = row()
        .key(key.to_owned())
        .role("button")
        .label(label.to_owned())
        .help(help.to_owned())
        .focusable(true)
        .size(36.0, 36.0)
        .center()
        .radius(18.0)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .press_scale(0.97)
        .on_press(move |s: &mut App| {
            if can || press_anyway {
                f(s);
            }
        });
    let r = if can { super::webkit::ink(t, r) } else { super::webkit::pebble(t, r) };
    r.child(icon(&icons::ARROW_UP, 16.0).color(if can { t.mat.ink_fg } else { t.text.muted }).a11y_hidden())
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
