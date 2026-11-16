//! The Neko column: the super agent, always there, collapsible to an icon. A one-line crew status
//! ("3 agents working · 1 needs you"), what the sessions column does not list (the routines' and
//! goals' Inbox items as quiet rows, the default on hover and in the right-click menu), the morning digest,
//! Neko's own conversation streaming in, and an ask box. Tasks that need you are in the sessions
//! column's first group; this column does not repeat them (docs/decisions/0002).

use neko_protocol::workbench::TaskStatus;
use rune::{El, Key, KeyEvent, ScrollSpec, col, icon, row, text};
use rune_kit::TextArea;

use super::turn::{self, Where};
use super::{band, icon_button, quiet_button};
use crate::app::{App, crew_line, status_line};
use crate::icons;
use crate::link::{Call, InboxItem};
use crate::status::{status_glyph, status_label, status_of};
use crate::theme::Neko;
use crate::turns::{Role, chat_turns};

const CHAT: Where = Where { ns: "neko-", compact: true };
/// How many Inbox items the column lists before saying how many more.
const INBOX_ITEMS: usize = 3;
/// How many of Neko's turns are drawn in the column.
const TURNS_SHOWN: usize = 8;

pub fn column(s: &App, t: &Neko, win_h: f32, layer: &mut Vec<El>) -> El {
    col().h_full().min_w(0.0).child(header(t)).child(body(s, t, win_h, layer)).child(foot(s, t))
}

/// "Neko" and the toggle that folds the column to its icon: 36 points.
fn header(t: &Neko) -> El {
    band(t)
        .h(36.0)
        .child(icon(&icons::SPARK, 16.0).color(t.text.base).a11y_hidden())
        .child(text("Neko").key("neko-title").font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(t.text.base).role("heading").grow(1.0))
        .child(icon_button(&icons::PANEL_RIGHT, "Collapse Neko", "neko-collapse", t, |s: &mut App| s.layout.set_neko_open(false)))
}

/// A section's heading row: a caret, the name, a faint count. A button that folds the section.
fn section_head(t: &Neko, key: &str, label: &str, count: Option<usize>, open: bool, toggle: &'static str) -> El {
    let mut h = row()
        .key(key.to_owned())
        .role("button")
        .label(format!("{label}{}", count.map_or(String::new(), |n| format!(", {n}"))))
        .expanded(open)
        .focusable(true)
        .w_full()
        .h(24.0)
        .px(6.0)
        .gap(6.0)
        .center_y()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .focus_ring(2.0, 0.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.toggle(toggle))
        .child(text(if open { "▾" } else { "▸" }).font(t.ty.meta).color(t.text.faint).a11y_hidden())
        .child(text(label.to_owned()).font(t.ty.group).color(t.text.muted).a11y_hidden());
    if let Some(n) = count {
        h = h.child(text(n.to_string()).font(t.ty.id).color(t.text.faint).a11y_hidden());
    }
    h
}

/// One Inbox item as a row, not a card: its title, then one muted line of detail. At the right,
/// on hover or keyboard focus, a check mark takes the default; the right-click menu has the same,
/// the goal's own actions for an item a goal raised, and Open.
fn inbox_item(s: &App, t: &Neko, item: &InboxItem, layer: &mut Vec<El>) -> El {
    let seq = item.seq;
    let detail = item.detail.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_owned();
    let open = s.open(&format!("inbox:{seq}"));
    let actions = s.item_actions(item);
    let mut text_col = col().grow(1.0).min_w(0.0).gap(1.0).child(text(item.title.clone()).font(t.ty.row).color(t.text.base).truncate().a11y_hidden());
    if !detail.is_empty() {
        let full = if open { item.detail.trim().to_owned() } else { detail.clone() };
        let line = text(full).key(format!("inbox-detail-{seq}")).font(t.ty.meta).color(t.text.muted).a11y_hidden();
        text_col = text_col.child(if open { line.wrap() } else { line.truncate() });
    }
    let mut line = row()
        .key(format!("inbox-{seq}"))
        .role("button")
        .label(format!("{}{}", item.title, item.default.as_ref().map_or(String::new(), |d| format!(", default: {d}"))))
        .help(item.default.as_ref().map_or_else(|| item.title.clone(), |d| format!("Default: {d}")))
        .expanded(open)
        .focusable(true)
        .hover_group()
        .w_full()
        .min_h(40.0)
        .px(6.0)
        .py(5.0)
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .focus_ring(2.0, 0.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.toggle(&format!("inbox:{seq}")))
        .on_secondary_press(move |s: &mut App, p, _| s.inbox_menu = Some((seq, p)))
        .child(text_col);
    // The quick action, only for a routine's item with a default; hidden until the row is hovered or focused.
    if item.goal.is_none() && item.default.is_some() {
        let help = item.default.as_ref().map_or_else(String::new, |d| format!("Use the default: {d}"));
        line = line.child(
            row()
                .key(format!("inbox-use-{seq}"))
                .role("button")
                .label(help.clone())
                .help(help)
                .focusable(true)
                .size(24.0, 24.0)
                .center()
                .radius(t.radius.control)
                .hover(t.surface.active)
                .focus_ring(2.0, 0.0, t.kit.c.border_focus)
                .group_opacity(0.0, 1.0)
                .on_press(move |s: &mut App| s.answer_default(seq))
                .child(icon(&icons::CHECK_LINE, 14.0).color(t.text.muted).a11y_hidden()),
        );
    }
    let note_key = item.goal.as_ref().map_or_else(|| format!("inbox:{seq}"), |g| format!("goal:{g}"));
    let mut c = col().w_full().child(match s.inbox_menu.as_ref().filter(|(open, _)| *open == seq) {
        Some((_, at)) if !actions.is_empty() => {
            let mut menu = rune_kit::context_menu(format!("inbox-menu-{seq}"), line, Some(*at)).on_close(|s: &mut App| s.inbox_menu = None);
            for a in actions {
                let act = a.act.clone();
                menu = menu.item(rune_kit::MenuItem::new(a.label).on_select(move |s: &mut App| {
                    s.inbox_menu = None;
                    s.run_item_action(act.clone());
                }));
            }
            menu.build(&t.kit, layer)
        }
        _ => line,
    });
    if let Some(note) = s.neko.notes.get(&note_key) {
        c = c.child(text(note.clone()).key(format!("inbox-note-{seq}")).font(t.ty.meta).color(t.meaning.amber).wrap().px(6.0).pb(4.0).live_region(rune::LiveRegion::Polite));
    }
    c
}

/// The Inbox: open by default, at most three items and "and N more" that opens the rest.
fn inbox(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let items = &s.neko.inbox;
    let open = !s.open("inbox:closed");
    let mut c = col().key("inbox").w_full().gap(2.0).child(section_head(t, "inbox-toggle", "Inbox", (!items.is_empty()).then_some(items.len()), open, "inbox:closed"));
    if !open {
        return c;
    }
    if items.is_empty() {
        c = c.child(text(if s.neko.loaded { "Nothing needs you right now" } else { "Looking for what needs you…" }).key("inbox-empty").font(t.ty.meta).color(t.text.muted).px(6.0).py(2.0));
    }
    let all = s.open("inbox:all");
    let shown = if all { items.len() } else { items.len().min(INBOX_ITEMS) };
    for item in items.iter().take(shown) {
        c = c.child(inbox_item(s, t, item, layer));
    }
    if items.len() > INBOX_ITEMS {
        let label = if all { "Show fewer".to_owned() } else { format!("and {} more", items.len() - INBOX_ITEMS) };
        c = c.child(
            row()
                .key("inbox-more")
                .role("button")
                .label(label.clone())
                .focusable(true)
                .h(24.0)
                .px(6.0)
                .center_y()
                .radius(t.radius.row)
                .hover(t.surface.hover)
                .focus_ring(2.0, 0.0, t.kit.c.border_focus)
                .on_press(|s: &mut App| s.toggle("inbox:all"))
                .child(text(label).font(t.ty.meta).color(t.text.muted).a11y_hidden()),
        );
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

/// The morning digest: closed by default, its heading and one muted summary line; open, the lists
/// and "Mark read". When there is none, it says that nothing needs you.
fn digest(s: &App, t: &Neko) -> El {
    let open = s.open("digest");
    let c = col().key("digest").w_full().gap(2.0).child(section_head(t, "digest-toggle", "Digest", None, open, "digest"));
    let Some(d) = &s.neko.digest else {
        return c.child(text("Nothing needs you").key("digest-empty").font(t.ty.meta).color(t.text.muted).px(6.0).py(2.0));
    };
    if !open {
        return c.child(text(d.headline.clone()).key("digest-headline").font(t.ty.meta).color(t.text.muted).truncate().px(6.0).py(2.0));
    }
    let mut details = col().key("digest-details").w_full().px(6.0).pt(2.0).gap(8.0);
    details = details.child(text(d.headline.clone()).key("digest-headline").font(t.ty.meta).color(t.text.base).wrap());
    for l in [lines(t, "Finished", &d.finished, false), lines(t, "Stuck", &d.stuck, false), lines(t, "Needs a decision", &d.decisions, true)].into_iter().flatten() {
        details = details.child(l);
    }
    if !d.calendar.is_empty() {
        details = details.child(text(d.calendar.clone()).font(t.ty.meta).color(t.text.muted).wrap());
    }
    if !d.spend.is_empty() {
        details = details.child(text(d.spend.clone()).font(t.ty.meta).color(t.text.muted).wrap());
    }
    c.child(details).child(row().pl(6.0).pt(2.0).child(quiet_button("Mark read", "digest-read", t, |s: &mut App| s.mark_digest_read())))
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
    let mut c = col().key("neko-chat").w_full().gap(0.0);
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

/// The crew line, the Inbox and the digest sit at the top, as tall as they are up to a share of the
/// window (they scroll inside it); Neko's conversation fills everything below, scrolls, and
/// follows its end while a reply streams. Nothing overlaps at any height: the chat's box starts
/// where the top block ends.
fn body(s: &App, t: &Neko, win_h: f32, layer: &mut Vec<El>) -> El {
    let working = s.summary.as_ref().map_or(0, |m| m.working());
    let waiting = s.waiting();
    let cap = (win_h * 0.46).clamp(120.0, 440.0);
    let mut top = col().key("neko-top").w_full().px(8.0).pb(8.0).gap(8.0);
    top = top.child(row().key("crew-row").w_full().h(24.0).px(6.0).center_y().child(text(crew_line(working, waiting)).key("crew").font(t.ty.row).color(t.text.base).truncate()));
    top = top.child(inbox(s, t, layer));
    top = top.child(digest(s, t));
    let mut c = col().key("neko-body").w_full().grow(1.0).min_h(0.0).child(col().key("neko-scroll").w_full().max_h(cap).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(top));
    if let Some(conv) = conversation(s, t) {
        c = c.child(row().key("neko-divider").w_full().h(1.0).fill(t.surface.border));
        c = c.child(col().key("neko-chat-scroll").w_full().grow(1.0).min_h(0.0).px(14.0).pb(8.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: true }).child(conv));
    } else {
        c = c.child(el_grow());
    }
    c
}

fn el_grow() -> El {
    rune::el().grow(1.0).min_h(0.0)
}

/// The ask box and the link's state. Enter asks; Shift+Enter is a new line; the reply streams in
/// above while it is written, with a Stop.
fn foot(s: &App, t: &Neko) -> El {
    let replying = s.summary.as_ref().is_some_and(|m| m.replying());
    let ask = TextArea::new(&s.ask)
        .placeholder("Ask Neko…")
        .label("Ask Neko")
        .auto_grow(2, 6)
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
        .min_w(0.0)
        .min_h(40.0)
        .max_h(120.0);
    let mut c = col().key("neko-foot").w_full().px(14.0).pb(8.0).gap(6.0).child(ask);
    if replying {
        let turn = s.summary.as_ref().and_then(|m| m.conversation.last()).map(|m| m.id.clone()).unwrap_or_default();
        c = c.child(row().gap(8.0).center_y().child(text("Neko is replying…").font(t.ty.meta).color(t.text.muted).grow(1.0)).child(quiet_button("Stop", "ask-stop", t, move |s: &mut App| s.call(Call::StopAsking { turn_id: turn.clone() }))));
    }
    c.child(row().h(20.0).center_y().child(text(status_line(&s.link, s.summary.as_ref())).key("status-text").font(t.ty.id).color(t.text.faint).truncate().live_region(rune::LiveRegion::Polite)))
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
        .on_press(|s: &mut App| s.layout.set_neko_open(true))
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
