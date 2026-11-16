//! The one confirmation every destructive press goes through (the web's `ConfirmDialog`): a modal dialog with the
//! question, what happens to the files, and two buttons. The yes button is coral when the press cannot be taken
//! back. Nothing is sent until it is pressed (`task_actions.rs`).

use rune::{Color, El, TextStyle, row, text};
use rune_kit::Dialog;

use crate::app::App;
use crate::theme::Neko;

pub fn mount(s: &App, t: &Neko, layer: &mut Vec<El>) {
    let Some(ask) = s.dialog.as_ref() else { return };
    let yes_fill = if ask.destructive { t.meaning.coral } else { t.meaning.iris };
    let no = row()
        .key("ask-no")
        .role("button")
        .label(ask.cancel.clone())
        .focusable(true)
        .h(32.0)
        .px(14.0)
        .center()
        .radius(t.radius.control)
        .border(1.0, t.surface.field_border)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.ask_no())
        .child(text(ask.cancel.clone()).font(t.ty.row).color(t.text.base).a11y_hidden());
    let yes = row()
        .key("ask-yes")
        .role("button")
        .label(ask.ok.clone())
        .focusable(true)
        .h(32.0)
        .px(14.0)
        .center()
        .radius(t.radius.control)
        .fill(yes_fill)
        .hover(Color::WHITE.alpha(0.14))
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.ask_yes())
        .child(text(ask.ok.clone()).font(TextStyle { weight: 500.0, ..t.ty.row }).color(Color::WHITE).a11y_hidden());
    let buttons = row().key("ask-buttons").w_full().gap(8.0).justify_end().pt(4.0).child(no).child(yes);
    Dialog::new("ask", ask.title.clone()).open(true).description(ask.detail.clone()).width(420.0).fit().child(buttons).on_close(|s: &mut App| s.ask_no()).build(&t.kit, layer);
}
