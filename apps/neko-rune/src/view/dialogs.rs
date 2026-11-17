//! The one confirmation every destructive press goes through (the web's `ConfirmDialog`): a modal dialog with the
//! question, what happens to the files, and two buttons. The yes button is coral when the press cannot be taken
//! back. Nothing is sent until it is pressed (`task_actions.rs`).

use rune::{Color, El, ScrollSpec, TextStyle, col, row, text};
use rune_kit::Dialog;

use crate::app::App;
use crate::task_actions::ASK_FIELD;
use crate::theme::Neko;

pub fn mount(s: &App, t: &Neko, layer: &mut Vec<El>) {
    let Some(ask) = s.dialog.as_ref() else { return };
    let mut yes_fill = if ask.destructive { t.meaning.coral } else { t.meaning.iris };
    // A dialog that asks for words (the reason of a closed pull request) needs them before it says yes.
    let blocked = ask.field.is_some() && s.pages.text(ASK_FIELD).trim().is_empty();
    if blocked {
        yes_fill = yes_fill.alpha(0.4);
    }
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
        .child(text(ask.ok.clone()).font(TextStyle { weight: 500.0, ..t.ty.row }).color(Color::WHITE).a11y_hidden());
    let yes = if blocked { yes.disabled(true) } else { yes.on_press(|s: &mut App| s.ask_yes()) };
    let buttons = row().key("ask-buttons").w_full().gap(8.0).justify_end().pt(4.0).child(no).child(yes);
    // The description wraps inside the body (the kit's header line does not wrap a long one).
    let mut body = col().key("ask-body").w_full().px(16.0).pb(16.0).gap(12.0).child(text(ask.detail.clone()).key("ask-detail").font(t.ty.meta).color(t.text.muted).wrap().min_w(0.0));
    if !ask.list.is_empty() {
        let mut list = col().key("ask-list").w_full().gap(2.0);
        for (i, line) in ask.list.iter().enumerate() {
            list = list.child(row().key(format!("ask-list-{i}")).w_full().min_h(24.0).px(8.0).center_y().radius(t.radius.control).fill(t.surface.hover).child(text(line.clone()).font(t.ty.meta).color(t.text.base).truncate().min_w(0.0).help(line.clone())));
        }
        body = body.child(col().key("ask-list-scroll").w_full().max_h(180.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(list));
    }
    if let Some(field) = &ask.field {
        body = body.child(col().w_full().gap(6.0).child(text(field.label.clone()).font(t.ty.meta).color(t.text.muted)).child(super::ui::area(s, t, ASK_FIELD, &field.label, &field.placeholder, 4)));
    }
    Dialog::new("ask", ask.title.clone()).open(true).width(420.0).fit().child(body.child(buttons)).on_close(|s: &mut App| s.ask_no()).build(&t.kit, layer);
}
