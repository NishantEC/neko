//! The Neko column: the super agent, always there, collapsible to an icon. A one-line crew status
//! ("3 agents working · 1 needs you"), what the sessions column does not list (the routines' and
//! goals' Inbox items, each with its default and a "Use the default" button), the morning digest,
//! Neko's own conversation streaming in, and an ask box. Tasks that need you are in the sessions
//! column's first group; this column does not repeat them (docs/decisions/0002).

use neko_protocol::workbench::TaskStatus;
use rune::{El, Key, KeyEvent, ScrollSpec, col, icon, row, text};
use rune_kit::TextArea;

use super::turn::{self, Where};
use super::{band, heading, icon_button, primary_button, quiet_button};
use crate::app::{App, crew_line, status_line};
use crate::icons;
use crate::link::{Call, InboxItem};
use crate::status::{status_glyph, status_label, status_of};
use crate::theme::Neko;
use crate::turns::{Role, chat_turns};

const CHAT: Where = Where { ns: "neko-", compact: true };
/// How many Inbox items the column lists before saying how many more.
const INBOX_ITEMS: usize = 4;
/// How many of Neko's turns are drawn in the column.
const TURNS_SHOWN: usize = 8;

pub fn column(s: &App, t: &Neko) -> El {
    col().h_full().min_w(0.0).child(header(t)).child(body(s, t)).child(foot(s, t))
}

fn header(t: &Neko) -> El {
    band(t)
        .child(icon(&icons::SPARK, 16.0).color(t.text.base).a11y_hidden())
        .child(text("Neko").key("neko-title").font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(t.text.base).role("heading").grow(1.0))
        .child(icon_button(&icons::PANEL_RIGHT, "Collapse Neko", "neko-collapse", t, |s: &mut App| s.layout.neko.set_open(false)))
}

/// What a routine or goal needs from you: its title, a line of detail, its default, and the way
/// to take it.
fn inbox_item(t: &Neko, item: &InboxItem) -> El {
    let seq = item.seq;
    let mut c = col().key(format!("inbox-{seq}")).w_full().px(6.0).py(6.0).gap(3.0).radius(t.radius.row).fill(t.surface.hover);
    c = c.child(text(item.title.clone()).font(t.ty.row).color(t.text.base).wrap());
    let detail = item.detail.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if !detail.is_empty() {
        c = c.child(text(detail.chars().take(140).collect::<String>()).font(t.ty.meta).color(t.text.muted).wrap());
    }
    c = c.child(text(match &item.default {
        Some(d) => format!("default: {d}"),
        None => "No default".to_owned(),
    }).key(format!("inbox-default-{seq}")).font(t.ty.id).color(t.text.faint).wrap());
    let mut actions = row().gap(6.0).wrap().pt(2.0);
    if item.default.is_some() {
        actions = actions.child(primary_button("Use the default", &format!("inbox-use-{seq}"), t, move |s: &mut App| s.answer_default(seq)));
    }
    if let Some(link) = item.link.clone() {
        actions = actions.child(quiet_button("Open", &format!("inbox-open-{seq}"), t, move |s: &mut App| s.open_link(&link)));
    }
    c.child(actions)
}

fn inbox(s: &App, t: &Neko) -> El {
    let items = &s.neko.inbox;
    let mut c = col().key("inbox").w_full().gap(4.0).child(heading("Inbox", (!items.is_empty()).then_some(items.len()), t));
    if items.is_empty() {
        c = c.child(text(if s.neko.loaded { "Nothing needs you right now" } else { "Looking for what needs you…" }).key("inbox-empty").font(t.ty.meta).color(t.text.muted));
    }
    for item in items.iter().take(INBOX_ITEMS) {
        c = c.child(inbox_item(t, item));
    }
    if items.len() > INBOX_ITEMS {
        c = c.child(text(format!("and {} more in the web UI", items.len() - INBOX_ITEMS)).font(t.ty.meta).color(t.text.faint).px(6.0));
    }
    if let Some(note) = &s.neko.note {
        c = c.child(text(note.clone()).key("inbox-note").font(t.ty.meta).color(t.text.muted).wrap().px(6.0).live_region(rune::LiveRegion::Polite));
    }
    c
}

fn lines(t: &Neko, title: &str, items: &[neko_protocol::digest::DigestLine], defaults: bool) -> Option<El> {
    if items.is_empty() {
        return None;
    }
    let mut c = col().gap(2.0).child(text(title.to_owned()).font(t.ty.group).color(t.text.muted));
    for line in items {
        let mut words = line.text.clone();
        if defaults || line.default.is_some() {
            words.push_str(&format!(" · {}", line.default.as_ref().map_or("No default".to_owned(), |d| format!("default: {d}"))));
        }
        c = c.child(text(words).font(t.ty.meta).color(t.text.base).wrap());
    }
    Some(c)
}

/// The morning digest: its headline, a caret that opens the lists, "Mark read". When there is
/// none, it says that nothing needs you.
fn digest(s: &App, t: &Neko) -> El {
    let mut c = col().key("digest").w_full().gap(4.0).child(heading("Digest", None, t));
    let Some(d) = &s.neko.digest else {
        return c.child(text("Nothing needs you").key("digest-empty").font(t.ty.meta).color(t.text.muted));
    };
    let open = s.open("digest");
    c = c.child(
        row()
            .key("digest-toggle")
            .role("button")
            .label(format!("Morning digest: {}", d.headline))
            .expanded(open)
            .focusable(true)
            .w_full()
            .gap(6.0)
            .px(6.0)
            .py(4.0)
            .radius(t.radius.row)
            .hover(t.surface.hover)
            .focus_ring(2.0, 0.0, t.kit.c.border_focus)
            .on_press(|s: &mut App| s.toggle("digest"))
            .child(text(if open { "▾" } else { "▸" }).font(t.ty.meta).color(t.text.faint).a11y_hidden())
            .child(text(d.headline.clone()).key("digest-headline").font(t.ty.meta).color(t.text.base).wrap().grow(1.0).min_w(0.0).a11y_hidden()),
    );
    if open {
        let mut details = col().key("digest-details").w_full().pl(24.0).gap(8.0);
        for l in [lines(t, "Finished", &d.finished, false), lines(t, "Stuck", &d.stuck, false), lines(t, "Needs a decision", &d.decisions, true)].into_iter().flatten() {
            details = details.child(l);
        }
        if !d.calendar.is_empty() {
            details = details.child(text(d.calendar.clone()).font(t.ty.meta).color(t.text.muted).wrap());
        }
        if !d.spend.is_empty() {
            details = details.child(text(d.spend.clone()).font(t.ty.meta).color(t.text.muted).wrap());
        }
        c = c.child(details);
    }
    c.child(row().pl(6.0).child(quiet_button("Mark read", "digest-read", t, |s: &mut App| s.mark_digest_read())))
}

/// `Neko → Builder  NEK-1BB3: Fix the flaky login test`: the tasks a turn opened, each a link.
fn handoffs(s: &App, t: &Neko, ids: &[String]) -> Vec<El> {
    let Some(summary) = &s.summary else { return Vec::new() };
    ids.iter()
        .filter_map(|id| summary.task(id))
        .map(|task| {
            let id = task.id.clone();
            let role = match task.status {
                TaskStatus::Planning => "Scout",
                TaskStatus::Reviewing | TaskStatus::ReadyForReview => "Reviewer",
                _ => "Builder",
            };
            let short = task.key.clone().unwrap_or_else(|| format!("NEK-{}", task.id.chars().take(4).collect::<String>().to_uppercase()));
            row()
                .key(format!("handoff-{}", task.id))
                .role("button")
                .label(format!("Neko hands off to {role}: {short}, {}, {}", task.title, status_label(task.status)))
                .focusable(true)
                .w_full()
                .min_h(26.0)
                .px(4.0)
                .gap(6.0)
                .center_y()
                .radius(t.radius.row)
                .hover(t.surface.hover)
                .focus_ring(2.0, 0.0, t.kit.c.border_focus)
                .on_press(move |s: &mut App| s.open_linked(&id))
                .child(text(format!("Neko → {role}")).font(t.ty.id).color(t.text.muted).a11y_hidden())
                .child(status_glyph(status_of(task.status), t, 12.0, Some("")))
                .child(text(short).font(t.ty.id).color(t.meaning.iris).a11y_hidden())
                .child(text(task.title.clone()).font(t.ty.meta).color(t.text.base).truncate().grow(1.0).min_w(0.0).a11y_hidden())
        })
        .collect()
}

/// Neko's own conversation: the last few turns, the reply in flight streaming.
fn conversation(s: &App, t: &Neko) -> Option<El> {
    let summary = s.summary.as_ref()?;
    if summary.conversation.is_empty() {
        return None;
    }
    let all = chat_turns(&summary.conversation);
    let skip = all.len().saturating_sub(TURNS_SHOWN);
    let mut c = col().key("neko-chat").w_full().gap(0.0).child(heading("Conversation", None, t));
    // The turns and their handoff lines, in order: a handoff follows the turn that opened it.
    let shown = &all[skip..];
    let mut start = 0;
    for (i, turn) in shown.iter().enumerate() {
        let followed = turn.role == Role::Agent && !turn.tickets.is_empty();
        if followed || i + 1 == shown.len() {
            for el in turn::turns(s, t, CHAT, &shown[start..=i]) {
                c = c.child(el);
            }
            for el in handoffs(s, t, &turn.tickets) {
                c = c.child(el);
            }
            start = i + 1;
        }
    }
    Some(c)
}

fn body(s: &App, t: &Neko) -> El {
    let working = s.summary.as_ref().map_or(0, |m| m.working());
    let waiting = s.waiting();
    // What needs you and the digest take at most a third of the column and scroll; the
    // conversation fills the rest and sticks to its end as Neko writes.
    let mut top = col().key("neko-top").w_full().px(14.0).pt(4.0).pb(10.0).gap(18.0);
    top = top.child(text(crew_line(working, waiting)).key("crew").font(t.ty.row).color(t.text.base));
    top = top.child(inbox(s, t));
    top = top.child(digest(s, t));
    let mut c = col().key("neko-body").w_full().grow(1.0).min_h(0.0).child(col().key("neko-scroll").w_full().max_h(360.0).scroll(ScrollSpec { stick_bottom: false }).child(top));
    if let Some(conv) = conversation(s, t) {
        c = c.child(col().key("neko-chat-scroll").w_full().grow(1.0).min_h(0.0).px(14.0).pb(8.0).scroll(ScrollSpec { stick_bottom: true }).child(conv));
    }
    c
}

/// The ask box and the link's state. Enter asks; Shift+Enter is a new line; the reply streams in
/// above while it is written, with a Stop.
fn foot(s: &App, t: &Neko) -> El {
    let replying = s.summary.as_ref().is_some_and(|m| m.replying());
    let ask = TextArea::new(&s.ask)
        .placeholder("Ask Neko…")
        .label("Ask Neko")
        .auto_grow(1, 5)
        .on_edit(|s: &mut App| &mut s.ask)
        .on_key(|s: &mut App, ev: &KeyEvent, _| {
            if ev.key != Key::Enter || ev.mods.shift || ev.mods.alt || s.ask.is_composing() {
                return false;
            }
            s.ask_neko();
            true
        })
        .build(&t.kit)
        .key("ask")
        .w_full()
        .min_w(0.0);
    let mut c = col().key("neko-foot").w_full().px(14.0).pb(14.0).gap(8.0).child(ask);
    if replying {
        let turn = s.summary.as_ref().and_then(|m| m.conversation.last()).map(|m| m.id.clone()).unwrap_or_default();
        c = c.child(row().gap(8.0).center_y().child(text("Neko is replying…").font(t.ty.meta).color(t.text.muted).grow(1.0)).child(quiet_button("Stop", "ask-stop", t, move |s: &mut App| s.call(Call::StopAsking { turn_id: turn.clone() }))));
    }
    c.child(text(status_line(&s.link, s.summary.as_ref())).key("status-text").font(t.ty.meta).color(t.text.faint).truncate().live_region(rune::LiveRegion::Polite))
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
