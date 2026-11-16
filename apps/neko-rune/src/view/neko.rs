//! The Neko column: the super agent, always there, collapsible to an icon. A one-line crew status
//! ("3 agents working · 1 needs you"), the Inbox (what needs you, from the live snapshot), the
//! digest and an ask box. In P0 the crew line and the Inbox are live and the digest and the ask
//! box are placeholders (docs/decisions/0002, plugin slot (d): Neko cards).

use rune::{El, ScrollSpec, col, icon, row, text};
use rune_kit::TextInput;

use super::{band, heading, icon_button};
use crate::app::{App, crew_line, status_line};
use crate::groups::{Group, grouped};
use crate::icons;
use crate::link::TaskRow;
use crate::status::{status_glyph, status_label, status_of};
use crate::theme::Neko;

/// How many Inbox items the column lists before saying how many more.
const INBOX_ITEMS: usize = 4;

pub fn column(s: &App, t: &Neko) -> El {
    col().h_full().min_w(0.0).child(header(t)).child(body(s, t)).child(foot(s, t))
}

fn header(t: &Neko) -> El {
    band(t)
        .child(icon(&icons::SPARK, 16.0).color(t.text.base).a11y_hidden())
        .child(text("Neko").key("neko-title").font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(t.text.base).role("heading").grow(1.0))
        .child(icon_button(&icons::PANEL_RIGHT, "Collapse Neko", "neko-collapse", t, |s: &mut App| s.layout.neko.set_open(false)))
}

fn body(s: &App, t: &Neko) -> El {
    let working = s.summary.as_ref().map_or(0, |m| m.working());
    let waiting = s.waiting();
    let mut c = col().key("neko-body").w_full().px(14.0).pt(4.0).gap(18.0);
    c = c.child(text(crew_line(working, waiting)).key("crew").font(t.ty.row).color(t.text.base));

    // Inbox: what needs you, newest first; each opens its session.
    let items: Vec<&TaskRow> = s.summary.as_ref().map(|m| grouped(m, s.now_ms)).and_then(|g| g.into_iter().find(|(g, _)| *g == Group::NeedsYou)).map(|(_, rows)| rows).unwrap_or_default();
    let mut inbox = col().key("inbox").w_full().gap(2.0).child(heading("Inbox", (!items.is_empty()).then_some(items.len()), t));
    if items.is_empty() {
        inbox = inbox.child(text("Nothing needs you right now").key("inbox-empty").font(t.ty.meta).color(t.text.muted));
    }
    for task in items.iter().take(INBOX_ITEMS) {
        let id = task.id.clone();
        inbox = inbox.child(
            row()
                .key(format!("inbox-{}", task.id))
                .role("button")
                .label(format!("{}, {}", task.title, status_label(task.status)))
                .focusable(true)
                .w_full()
                .h(30.0)
                .px(6.0)
                .gap(8.0)
                .center_y()
                .radius(t.radius.row)
                .hover(t.surface.hover)
                .focus_ring(2.0, 0.0, t.kit.c.border_focus)
                .on_press(move |s: &mut App| s.select(&id))
                .child(status_glyph(status_of(task.status), t, 14.0, Some("")))
                .child(text(task.title.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).a11y_hidden()),
        );
    }
    if items.len() > INBOX_ITEMS {
        inbox = inbox.child(text(format!("and {} more", items.len() - INBOX_ITEMS)).font(t.ty.meta).color(t.text.faint).px(6.0));
    }
    c = c.child(inbox);

    c = c.child(
        col()
            .key("digest")
            .w_full()
            .gap(2.0)
            .child(heading("Digest", None, t))
            .child(text("Today's digest shows here (P1).").font(t.ty.meta).color(t.text.muted)),
    );
    col().key("neko-scroll").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(c)
}

/// The ask box and the link's state. Asking arrives in P1, so the box is off and says so.
fn foot(s: &App, t: &Neko) -> El {
    let ask = TextInput::new(&s.ask)
        .placeholder("Ask Neko…")
        .label("Ask Neko")
        .disabled(true)
        .on_edit(|s: &mut App| &mut s.ask)
        .build(&t.kit)
        .key("ask")
        .w_full()
        .min_w(0.0);
    col().key("neko-foot").w_full().px(14.0).pb(14.0).gap(8.0).child(ask).child(
        text(status_line(&s.link, s.summary.as_ref())).key("status-text").font(t.ty.meta).color(t.text.faint).truncate().live_region(rune::LiveRegion::Polite),
    )
}

/// The column collapsed to an icon: Neko's mark, with a count of what needs you.
pub fn strip(s: &App, t: &Neko) -> El {
    let waiting = s.waiting();
    let mut button = row()
        .key("neko-expand")
        .role("button")
        .label(if waiting > 0 { format!("Neko, {waiting} need you") } else { "Neko".to_owned() })
        .help("Show Neko")
        .focusable(true)
        .size(32.0, 32.0)
        .center()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .focus_ring(2.0, 0.0, t.kit.c.border_focus)
        .on_press(|s: &mut App| s.layout.neko.set_open(true))
        .child(icon(&icons::SPARK, 18.0).color(t.text.base).a11y_hidden());
    if waiting > 0 {
        button = button.child(
            row().key("badge").absolute(18.0, 0.0).h(16.0).min_w(16.0).px(4.0).radius(8.0).center().fill(t.meaning.amber).a11y_hidden().child(
                text(if waiting > 99 { "99+".to_owned() } else { waiting.to_string() }).font(rune::TextStyle { weight: 500.0, ..t.ty.id }).color(t.surface.window),
            ),
        );
    }
    col().h_full().items_center().child(band(t).px(0.0).justify_center()).child(button)
}
