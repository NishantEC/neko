//! The composers' extras: the `/` and `@` popover above Neko's ask box, the queue tray of messages waiting behind a
//! reply, the drop overlay over the session region, and the right-click menu of a message (`compose.rs` holds the rules;
//! the web files are `ChatInputComposer.tsx`, `queue-tray.tsx`, `message-menu.ts`).

use rune::{Color, El, ScrollSpec, TextStyle, col, el, icon, row, text};
use rune_kit::{MenuItem, Placement, TextInput, context_menu, popover};

use crate::app::App;
use crate::compose::{ItemKind, MenuRow, VISIBLE_ROWS};
use crate::icons;
use crate::theme::Neko;

/// A popover row: 32 points tall.
pub const ROW_H: f32 = 32.0;
const WIDTH: f32 = 360.0;

fn glyph(kind: ItemKind, row: &MenuRow) -> &'static rune::Icon {
    match kind {
        ItemKind::Workspace => &icons::FOLDER,
        ItemKind::Tool => &icons::WRENCH,
        ItemKind::File => &icons::FILE,
        ItemKind::Command if row.id == "/plan" => &icons::REASON_LIST,
        ItemKind::Command => &icons::TERMINAL,
    }
}

/// The popover above the ask box, or above the session's reply box: at most 8 rows of 32 before it scrolls, the active one filled.
pub fn mount_popover(s: &App, t: &Neko, layer: &mut Vec<El>) {
    let ask = s.ask_rows();
    if !(ask.is_empty() || !s.ask.focused && s.ask.text.is_empty()) {
        return show_popover(s, t, layer, ask, false);
    }
    let reply = s.reply_rows();
    if !(reply.is_empty() || !s.composer.focused && s.composer.text.is_empty()) {
        show_popover(s, t, layer, reply, true);
    }
}

/// The popover's rows over `anchor`: `reply` is the session's, otherwise Neko's ask box's.
fn show_popover(s: &App, t: &Neko, layer: &mut Vec<El>, rows: Vec<MenuRow>, reply: bool) {
    let (prefix, anchor) = if reply { ("reply", "composer-card") } else { ("ask", "ask-card") };
    let active = s.ask_active(rows.len()).unwrap_or(0);
    let mut list = col().key(format!("{prefix}-popover-list")).role("listbox").label("Suggestions").w_full().gap(0.0);
    for (i, r) in rows.iter().enumerate() {
        let on = i == active;
        let pressed = r.clone();
        let mut line = row()
            .key(format!("{prefix}-pop-{i}"))
            .role("option")
            .label(format!("{}, {}", r.title, r.detail))
            .checked(on)
            .w_full()
            .h(ROW_H)
            .px(8.0)
            .gap(8.0)
            .center_y()
            .radius(t.radius.row)
            .fill(if on { t.surface.active } else { Color::TRANSPARENT })
            .hover(t.surface.hover)
            .on_press(move |s: &mut App| if reply { s.reply_apply(&pressed) } else { s.ask_apply(&pressed) })
            .child(icon(glyph(r.kind, r), 16.0).color(t.text.muted).a11y_hidden())
            .child(text(r.title.clone()).font(TextStyle { weight: 500.0, ..t.ty.row }).color(t.text.base).truncate().max_w(150.0).a11y_hidden())
            .child(text(r.detail.clone()).font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden());
        if let Some(key) = &r.shortcut {
            line = line.child(text(key.clone()).font(t.ty.hint).color(t.text.faint).a11y_hidden());
        }
        list = list.child(line);
    }
    let shown = rows.len().min(VISIBLE_ROWS) as f32;
    let body = col().key(format!("{prefix}-popover-scroll")).w_full().h(shown * ROW_H).max_h(VISIBLE_ROWS as f32 * ROW_H).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(list);
    let surface = popover::raised(&t.kit, col().key(format!("{prefix}-popover")).role("dialog").label(if reply { "Commands" } else { "Commands and mentions" }).w(WIDTH).padding(4.0).child(body), t.kit.radius.md);
    layer.push(popover::anchored(&t.kit, surface, anchor, Placement::TopStart, 8.0));
}

/// One queued message of Neko's chat: its words, and on hover Edit, Send now and Cancel. Edit turns the row into a field.
fn tray_row(s: &App, t: &Neko, id: &str, words: &str) -> El {
    if let Some((editing, edit)) = s.compose.editing.as_ref().filter(|(e, _)| e == id) {
        let _ = editing;
        let field = TextInput::new(edit)
            .label("Edit the queued message")
            .on_edit(|s: &mut App| &mut s.compose.editing.as_mut().expect("editing").1)
            .on_submit(|s: &mut App, _| s.finish_edit_queued())
            .on_key(|s: &mut App, ev: &rune::KeyEvent, _| {
                if ev.key == rune::Key::Escape {
                    s.cancel_edit_queued();
                    return true;
                }
                false
            })
            .build(&t.kit)
            .key("queue-edit")
            .grow(1.0)
            .min_w(0.0);
        return row().key(format!("queue-row-{id}")).w_full().h(ROW_H).px(4.0).gap(4.0).center_y().child(field);
    }
    let small = |key: String, glyph: &'static rune::Icon, label: &str, f: Box<dyn Fn(&mut App)>| {
        row()
            .key(key)
            .role("button")
            .label(label.to_owned())
            .help(label.to_owned())
            .focusable(true)
            .size(24.0, 24.0)
            .center()
            .radius(6.0)
            .hover(t.surface.hover)
            .focus_ring(2.0, 2.0, t.surface.ring)
            .on_press(move |s: &mut App| f(s))
            .child(icon(glyph, 14.0).color(t.text.muted).a11y_hidden())
    };
    let (edit_id, edit_words, steer_id, steer_words, cancel_id) = (id.to_owned(), words.to_owned(), id.to_owned(), words.to_owned(), id.to_owned());
    let actions = row()
        .key(format!("queue-actions-{id}"))
        .gap(2.0)
        .center_y()
        .group_opacity(0.0, 1.0)
        .child(small(format!("queue-edit-{id}"), &icons::EDIT, "Edit queued message (⌥↑ for the last)", Box::new(move |s| s.start_edit_queued(&edit_id, &edit_words))))
        .child(small(format!("queue-send-{id}"), &icons::SEND_NOW, "Send now: stop the reply and send this first (⌘↵)", Box::new(move |s| s.steer_queued(&steer_id, &steer_words))))
        .child(small(format!("queue-cancel-{id}"), &icons::CLOSE_SMALL, "Remove from queue", Box::new(move |s| s.discard_queued(&cancel_id))));
    row()
        .key(format!("queue-row-{id}"))
        .w_full()
        .h(ROW_H)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .hover_group()
        .child(icon(&icons::REASON_LIST, 14.0).color(t.text.faint).a11y_hidden())
        .child(text(words.to_owned()).font(t.ty.meta).color(t.text.base).truncate().grow(1.0).min_w(0.0).help(words.to_owned()).a11y_hidden())
        .child(actions)
}

/// The tray above the ask box while messages wait behind a reply: "Queued 2", then one row each.
pub fn tray(s: &App, t: &Neko) -> Option<El> {
    let queued = s.queued_chat();
    if queued.is_empty() {
        return None;
    }
    let mut c = col().key("queue-tray").role("region").label(format!("{} queued message{}", queued.len(), if queued.len() == 1 { "" } else { "s" })).w_full().radius(12.0).border(1.0, t.surface.field_border).fill(t.text.base.alpha(0.03)).padding(4.0).live_region(rune::LiveRegion::Polite);
    c = c.child(
        row()
            .key("queue-head")
            .w_full()
            .h(ROW_H)
            .px(8.0)
            .gap(8.0)
            .center_y()
            .child(text("Queued").font(TextStyle { weight: 500.0, ..t.ty.meta }).color(t.text.muted).grow(1.0))
            .child(text(queued.len().to_string()).key("queue-count").font(t.ty.meta).color(t.text.muted)),
    );
    let mut list = col().key("queue-list").w_full();
    for (id, words) in &queued {
        list = list.child(tray_row(s, t, id, words));
    }
    Some(c.child(col().w_full().max_h(128.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(list)))
}

/// The overlay while files are dragged over the session region: a dashed Iris border inset 12, radius 16, "Drop to attach".
pub fn drop_overlay(s: &App, t: &Neko) -> Option<El> {
    if !s.compose.drop_over {
        return None;
    }
    Some(
        col().key("drop-overlay")
            .role("status")
            .label("Drop to attach")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .padding(12.0)
            .child(
                row()
                    .key("drop-frame")
                    .w_full()
                    .grow(1.0)
                    .center()
                    .radius(16.0)
                    .fill(t.meaning.iris.alpha(0.06))
                    .border(2.0, t.meaning.iris)
                    .border_dash(8.0, 6.0)
                    .child(text("Drop to attach").font(TextStyle { weight: 500.0, size: 14.0, ..t.ty.row }).color(t.text.base).a11y_hidden()),
            ),
    )
}

/// What a message's right-click menu was opened on.
#[derive(Clone, Debug)]
pub struct MsgMenu {
    pub key: String,
    /// The message's words ("" for a crew line).
    pub text: String,
    /// The person's own message: "Edit and send again" is offered.
    pub own: bool,
    /// Tasks the message opened or names: (id, "NEK-1234: title").
    pub tasks: Vec<(String, String)>,
    /// Which composer a quote or an edit goes to.
    pub in_neko: bool,
    pub at: rune::PointerPress,
}

/// The menu (the web's `useMessageMenu`): Copy, Quote in reply, Edit and send again (yours), then Open for each task.
pub fn mount_message_menu(s: &App, t: &Neko, layer: &mut Vec<El>) {
    let Some(m) = s.msg_menu.as_ref() else { return };
    let mut menu = context_menu("msg-menu", el(), Some(m.at)).on_close(|s: &mut App| s.msg_menu = None);
    let words = m.text.clone();
    let has = !m.text.trim().is_empty();
    let copy = words.clone();
    menu = menu.item(MenuItem::new(if m.own { "Copy message" } else { "Copy reply" }).icon(&icons::COPY).disabled(!has).on_select_tx(move |s: &mut App, tx: &mut rune::Tx| {
        s.msg_menu = None;
        tx.copy(copy.clone());
        s.show_toast("Copied");
    }));
    let quoted = words.clone();
    let quote_in_neko = m.in_neko;
    menu = menu.item(MenuItem::new("Quote in reply").icon(&icons::QUOTE).disabled(!has).on_select(move |s: &mut App| {
        s.msg_menu = None;
        s.quote_into_composer(&quoted, quote_in_neko);
    }));
    if m.own {
        let again = words.clone();
        let in_neko = m.in_neko;
        menu = menu.item(MenuItem::new("Edit and send again").icon(&icons::EDIT).disabled(!has).on_select(move |s: &mut App| {
            s.msg_menu = None;
            s.edit_and_send_again(&again, in_neko);
        }));
    }
    if !m.tasks.is_empty() {
        menu = menu.separator();
    }
    for (id, label) in m.tasks.iter().take(5) {
        let id = id.clone();
        menu = menu.item(MenuItem::new(format!("Open {label}")).icon(&icons::EXTERNAL).on_select(move |s: &mut App| {
            s.msg_menu = None;
            s.open_linked(&id);
        }));
    }
    let _ = menu.build(&t.kit, layer);
}
