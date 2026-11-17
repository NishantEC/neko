//! The Neko column: the super agent, always there, collapsible to an icon. A one-line crew status
//! ("3 agents working · 1 needs you"), what the sessions column does not list (the routines' and
//! goals' Inbox items as quiet rows, the default on hover and in the right-click menu), the morning digest,
//! Neko's own conversation streaming in, and an ask box. Tasks that need you are in the sessions
//! column's first group; this column does not repeat them (docs/decisions/0002).

use neko_protocol::workbench::TaskStatus;
use rune::{El, KeyEvent, ScrollSpec, col, el, icon, row, text};

use super::turn::{self, Where};
use super::{flat_band, icon_button, quiet_button};
use crate::activity;
use crate::app::{App, crew_line, status_line};
use crate::icons;
use crate::link::{Call, InboxItem};
use crate::theme::Neko;
use crate::turns::{Role, chat_turns};

const CHAT: Where = Where { ns: "neko-", compact: true };
/// How many Inbox items the column lists before saying how many more (two in a short window, so
/// the chat keeps room to read).
const INBOX_ITEMS: usize = 3;
const INBOX_ITEMS_SHORT: usize = 2;
/// A window this short or shorter is "short".
pub const SHORT_WINDOW: f32 = 600.0;
/// The chat keeps at least this much height at any window height.
const CHAT_MIN: f32 = 160.0;

fn inbox_limit(win_h: f32) -> usize {
    if win_h <= SHORT_WINDOW { INBOX_ITEMS_SHORT } else { INBOX_ITEMS }
}

/// What the composer card, its status line and their padding take at least (three lines, a bar).
const FOOT_MIN: f32 = 212.0;

pub fn column(s: &App, t: &Neko, win_h: f32, layer: &mut Vec<El>) -> El {
    col().h_full().min_w(0.0).child(header(t)).child(body(s, t, win_h, layer)).child(foot(s, t, win_h))
}

/// "Neko" and the toggle that folds the column to its icon: 36 points.
fn header(t: &Neko) -> El {
    flat_band(t)
        .child(icon(&icons::SPARK, 16.0).color(t.text.base).a11y_hidden())
        .child(text("Neko").key("neko-title").font(t.ty.brand).color(t.text.base).role("heading").grow(1.0))
        .child(icon_button(&icons::PANEL_RIGHT, "Collapse Neko", "neko-collapse", t, |s: &mut App| s.layout.set_neko_open(false)))
}

/// A section's heading row: a caret, the name, a faint count. A button that folds the section.
pub(super) fn section_head(t: &Neko, key: &str, label: &str, count: Option<usize>, open: bool, toggle: &'static str) -> El {
    let mut h = row()
        .key(key.to_owned())
        .role("button")
        .label(format!("{label}{}", count.map_or(String::new(), |n| format!(", {n}"))))
        .expanded(open)
        .focusable(true)
        .w_full()
        .h(32.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(move |s: &mut App| s.toggle(toggle))
        .child(icon(if open { &icons::CHEVRON_DOWN } else { &icons::CHEVRON_RIGHT }, 16.0).color(t.text.muted).a11y_hidden())
        .child(text(label.to_owned()).font(t.ty.group).color(t.text.muted).grow(1.0).a11y_hidden());
    if let Some(n) = count {
        h = h.child(text(n.to_string()).font(t.ty.meta).color(t.text.muted).a11y_hidden());
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
    let title = text(item.title.clone()).font(t.ty.row).color(t.text.base).a11y_hidden();
    let mut text_col = col().grow(1.0).min_w(0.0).gap(2.0).child(if open { title.wrap() } else { title.truncate() });
    // One 32 point line: the detail is for the open row (and the tooltip), not a second line of every row.
    if !detail.is_empty() && open {
        text_col = text_col.child(text(item.detail.trim().to_owned()).key(format!("inbox-detail-{seq}")).font(t.ty.meta).color(t.text.muted).wrap().a11y_hidden());
    }
    let mut line = row()
        .key(format!("inbox-{seq}"))
        .role("button")
        .label(format!("{}{}", item.title, item.default.as_ref().map_or(String::new(), |d| format!(", default: {d}"))))
        .help(if detail.is_empty() { item.title.clone() } else { format!("{detail}{}", item.default.as_ref().map_or(String::new(), |d| format!(" · Default: {d}"))) })
        .expanded(open)
        .focusable(true)
        .hover_group()
        .w_full()
        .min_h(32.0)
        .px(8.0)
        .py(if open { 6.0 } else { 0.0 })
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(move |s: &mut App| s.toggle(&format!("inbox:{seq}")))
        .on_secondary_press(move |s: &mut App, p, _| s.inbox_menu = Some((seq, p)))
        .child(text_col);
    // The quick actions, hidden until the row is hovered or focused, in this order: Use the default (only when the item has one),
    // then Mark done.
    let hover = |key: String, glyph: &'static rune::Icon, help: String, f: Box<dyn Fn(&mut App)>| {
        row()
            .key(key)
            .role("button")
            .label(help.clone())
            .help(help)
            .focusable(true)
            .size(24.0, 24.0)
            .center()
            .radius(t.radius.control)
            .hover(t.surface.active)
            .focus_ring(2.0, 2.0, t.surface.ring)
            .group_opacity(0.0, 1.0)
            .on_press(move |s: &mut App| f(s))
            .child(icon(glyph, 14.0).color(t.text.muted).a11y_hidden())
    };
    if item.goal.is_none() && item.default.is_some() {
        let help = item.default.as_ref().map_or_else(String::new, |d| format!("Use the default: {d}"));
        line = line.child(hover(format!("inbox-use-{seq}"), &icons::CHECK_LINE, help, Box::new(move |s| s.answer_default(seq))));
    }
    if !item.routine_id.is_empty() {
        let done = item.clone();
        line = line.child(hover(format!("inbox-done-{seq}"), &icons::DONE, format!("Mark done: {}", item.title), Box::new(move |s| s.mark_item_done(&done))));
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
fn inbox(s: &App, t: &Neko, layer: &mut Vec<El>, win_h: f32) -> El {
    let items = &s.neko.inbox;
    let open = !s.open("inbox:closed");
    let mut c = col().key("inbox").w_full().gap(2.0).child(section_head(t, "inbox-toggle", "Inbox", (!items.is_empty()).then_some(items.len()), open, "inbox:closed"));
    if !open {
        return c;
    }
    if items.is_empty() && s.neko.failed && !s.neko.loaded {
        c = c.child(
            row()
                .key("inbox-failed")
                .w_full()
                .min_h(28.0)
                .px(6.0)
                .gap(8.0)
                .center_y()
                .child(text("Couldn’t read the Inbox").font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0))
                .child(quiet_button("Retry", "inbox-retry", t, |s: &mut App| s.retry_inbox())),
        );
    } else if items.is_empty() {
        c = c.child(text(if s.neko.loaded { "Nothing needs you right now" } else { "Looking for what needs you…" }).key("inbox-empty").font(t.ty.meta).color(t.text.muted).px(6.0).py(2.0));
    }
    let limit = inbox_limit(win_h);
    let all = s.open("inbox:all");
    let shown = if all { items.len() } else { items.len().min(limit) };
    for item in items.iter().take(shown) {
        c = c.child(inbox_item(s, t, item, layer));
    }
    if items.len() > limit {
        let label = if all { "Show fewer".to_owned() } else { format!("and {} more", items.len() - limit) };
        c = c.child(
            row()
                .key("inbox-more")
                .role("button")
                .label(label.clone())
                .focusable(true)
                .h(32.0)
                .px(8.0)
                .gap(8.0)
                .center_y()
                .radius(t.radius.row)
                .hover(t.surface.hover)
                .focus_ring(2.0, 2.0, t.surface.ring)
                .on_press(|s: &mut App| s.toggle("inbox:all"))
                .child(el().size(16.0, 16.0).a11y_hidden())
                .child(text(label).font(t.ty.meta).color(t.text.muted).a11y_hidden()),
        );
    }
    if let Some(note) = &s.neko.note {
        c = c.child(text(note.clone()).key("inbox-note").font(t.ty.meta).color(t.text.muted).wrap().px(6.0).live_region(rune::LiveRegion::Polite));
    }
    c
}

/// One item closed in the last three days: its title, why it closed in 12.5 muted, and on hover Reopen. The right-click menu
/// has Reopen, Open (its link) and Copy.
fn closed_item(s: &App, t: &Neko, item: &InboxItem, layer: &mut Vec<El>) -> El {
    let seq = item.seq;
    let key = format!("closed-{seq}");
    let at = item.closed_ms.unwrap_or(item.at_ms);
    let words = format!("{} · {}", if item.reason.is_empty() { "Closed" } else { item.reason.as_str() }, crate::groups::age(s.now_ms, at));
    let reopen = item.clone();
    let line = row()
        .key(key.clone())
        .role("listitem")
        .label(format!("{}, {words}", item.title))
        .help(if item.detail.is_empty() { item.title.clone() } else { item.detail.clone() })
        .focusable(true)
        .hover_group()
        .w_full()
        .min_h(32.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_secondary_press(move |s: &mut App, p, _| s.closed_menu = Some((seq, p)))
        .child(text(item.title.clone()).font(t.ty.row).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden())
        .child(text(words).key(format!("closed-reason-{seq}")).font(t.ty.meta).color(t.text.muted).a11y_hidden())
        .child(
            row()
                .key(format!("closed-reopen-{seq}"))
                .role("button")
                .label(format!("Reopen “{}”", item.title))
                .help("Reopen")
                .focusable(true)
                .size(24.0, 24.0)
                .center()
                .radius(t.radius.control)
                .hover(t.surface.active)
                .focus_ring(2.0, 2.0, t.surface.ring)
                .group_opacity(0.0, 1.0)
                .on_press(move |s: &mut App| s.reopen_item(&reopen))
                .child(icon(&icons::UNDO, 14.0).color(t.text.muted).a11y_hidden()),
        );
    match s.closed_menu.as_ref().filter(|(open, _)| *open == seq) {
        Some((_, at)) => {
            let mut menu = rune_kit::context_menu(format!("closed-menu-{seq}"), line, Some(*at)).on_close(|s: &mut App| s.closed_menu = None);
            let again = item.clone();
            menu = menu.item(rune_kit::MenuItem::new("Reopen").icon(&icons::UNDO).on_select(move |s: &mut App| {
                s.closed_menu = None;
                s.reopen_item(&again);
            }));
            if let Some(link) = item.link.clone() {
                menu = menu.item(rune_kit::MenuItem::new("Open link").icon(&icons::EXTERNAL).on_select(move |s: &mut App| {
                    s.closed_menu = None;
                    s.open_link(&link);
                }));
            }
            let copy = [item.title.as_str(), item.detail.as_str()].iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join("\n\n");
            menu = menu.separator().item(rune_kit::MenuItem::new("Copy").icon(&icons::COPY).on_select_tx(move |s: &mut App, tx: &mut rune::Tx| {
                s.closed_menu = None;
                tx.copy(copy.clone());
            }));
            menu.build(&t.kit, layer)
        }
        None => line,
    }
}

/// "Closed recently": after the Inbox, folded until opened, the last three days' closed items with why each closed.
fn closed_section(s: &App, t: &Neko, layer: &mut Vec<El>) -> Option<El> {
    let items = &s.neko.closed;
    if items.is_empty() {
        return None;
    }
    // Opened by the person: `closed:open` is in the expanded set (folded by default, as the web's `defaultOpen={false}`).
    let open = s.open("closed:open");
    let head = section_head(t, "closed-toggle", "Closed recently", Some(items.len()), open, "closed:open");
    let mut c = col().key("closed").w_full().gap(2.0).child(head);
    if open {
        for item in items {
            c = c.child(closed_item(s, t, item, layer));
        }
    }
    Some(c)
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
        return c.child(text("Nothing needs you").key("digest-empty").font(t.ty.meta).color(t.text.muted).px(8.0).py(2.0));
    };
    if !open {
        return c.child(text(d.headline.clone()).key("digest-headline").font(t.ty.meta).color(t.text.muted).truncate().px(8.0).py(2.0));
    }
    let mut details = col().key("digest-details").w_full().px(8.0).pt(2.0).gap(8.0);
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
            let role = match task.status {
                TaskStatus::Planning => "Scout",
                TaskStatus::Reviewing | TaskStatus::ReadyForReview => "Reviewer",
                _ => "Builder",
            };
            let shown = turn::HandoffTask { id: task.id.clone(), title: task.title.clone(), status: task.status };
            turn::handoff_row(t, &format!("neko-hand-{}", task.id), "Neko", role, &shown)
        })
        .collect()
}

/// The Activity line (polish spec §0): what Neko posted on its own (it updated itself, reinstalled, rolled
/// back, an outage) folded into one quiet row at the top of the chat: an icon, the latest notice in 12.5
/// muted, and "+N" faint for the others. Pressing it opens the list in place; it is closed by default.
fn activity_line(s: &App, t: &Neko) -> Option<El> {
    let summary = s.summary.as_ref()?;
    let split = activity::split(&summary.conversation);
    let (latest, more) = activity::summary(&split.notices)?;
    let open = s.activity_open;
    let mut line = row()
        .key("activity")
        .role("button")
        .label(format!("Activity: {}{}", latest.text, if more > 0 { format!(", and {more} more") } else { String::new() }))
        .expanded(open)
        .focusable(true)
        .w_full()
        .h(32.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.activity_open = !s.activity_open)
        .child(icon(&icons::CLOCK, 16.0).color(t.text.muted).a11y_hidden())
        .child(text(latest.text.clone()).key("activity-latest").font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden());
    if more > 0 {
        line = line.child(text(format!("+{more}")).key("activity-more").font(t.ty.meta).color(t.text.faint).a11y_hidden());
    }
    let mut c = col().key("activity-block").w_full().px(12.0).pt(8.0).child(line);
    if open {
        let mut list = col().key("activity-list").w_full().px(8.0).pb(4.0).gap(4.0);
        for notice in split.notices.iter().rev() {
            list = list.child(text(notice.text.clone()).font(t.ty.meta).color(t.text.muted).wrap().py(2.0));
        }
        c = c.child(col().key("activity-scroll").w_full().max_h(180.0).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(list));
    }
    Some(c)
}

/// Neko's own conversation: the last 30 messages, the reply in flight streaming.
fn conversation(s: &App, t: &Neko) -> Option<El> {
    let summary = s.summary.as_ref()?;
    if summary.conversation.is_empty() {
        return None;
    }
    // Conversation only: what the daemon posted on its own is the Activity line, never a message.
    let split = activity::split(&summary.conversation);
    let (messages, held_back) = activity::visible(&summary.conversation, &split, s.chat_pages);
    let all = chat_turns(&messages);
    let mut c = col().key("neko-chat").w_full().gap(0.0);
    if held_back > 0 {
        c = c.child(
            row().key("earlier-row").w_full().h(32.0).justify_center().child(quiet_button(&format!("Earlier messages ({held_back})"), "earlier", t, |s: &mut App| s.chat_pages += 1)),
        );
    }
    // The turns and their handoff lines, in order: a handoff follows the turn that opened it.
    let shown = &all[..];
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
    // The top block takes what it needs up to a share of the window, but never so much that the chat
    // (header 36, the ask box and status below it) is left under CHAT_MIN.
    let foot = FOOT_MIN;
    let cap = (win_h * 0.5).min(win_h - super::BAND - foot - CHAT_MIN).clamp(100.0, 440.0);
    let mut top = col().key("neko-top").w_full().px(12.0).pb(8.0).gap(8.0);
    top = top.child(row().key("crew-row").w_full().h(32.0).px(8.0).center_y().child(text(crew_line(working, waiting, s.neko.inbox.len())).key("crew").font(t.ty.row).color(t.text.base).truncate()));
    top = top.child(inbox(s, t, layer, win_h));
    if let Some(closed) = closed_section(s, t, layer) {
        top = top.child(closed);
    }
    top = top.child(digest(s, t));
    // Cards that installed packs ship (Luau), below the Digest.
    if let Some(cards) = super::luau::cards(s, t, layer) {
        top = top.child(cards);
    }
    let mut c = col().key("neko-body").w_full().grow(1.0).min_h(0.0).child(col().key("neko-scroll").w_full().max_h(cap).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(top));
    let activity = activity_line(s, t);
    if let Some(conv) = conversation(s, t) {
        c = c.child(row().key("neko-divider").w_full().h(1.0).fill(t.surface.border));
        if let Some(a) = activity {
            c = c.child(a);
        }
        c = c.child(col().key("neko-chat-scroll").w_full().grow(1.0).min_h(0.0).px(12.0).pb(8.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: true }).scroll_edge_fade(16.0).child(conv));
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
fn foot(s: &App, t: &Neko, win_h: f32) -> El {
    let replying = s.summary.as_ref().is_some_and(|m| m.replying());
    let ask = super::composer::field(t, &s.ask, "ask-box", "ask", "Ask Neko", "Ask Neko…", if win_h <= SHORT_WINDOW { 4 } else { 6 }, |s: &mut App| &mut s.ask, |s: &mut App, ev: &KeyEvent| s.ask_key(ev));
    let plan = s.ask_plan;
    let mode = super::composer::segmented(
        t,
        "ask-mode",
        "Ask or Plan",
        super::composer::seg(t, "ask-mode-ask", "Ask", "Ask: Neko answers now", "Ask: Neko answers now", !plan, |s: &mut App| s.ask_plan = false),
        super::composer::seg(t, "ask-mode-plan", "Plan", "Plan: Neko proposes a task with a plan for your approval, without changing anything", "Plan: Neko proposes a task with a plan for your approval, without changing anything", plan, |s: &mut App| s.ask_plan = true),
    );
    let model = super::runtime::ask_chip(s, t);
    let can = s.has_payload(crate::compose::Dest::Ask) && !s.work.busy.contains("ask");
    let send = super::composer::send(t, "ask-send", "Ask Neko", "Ask Neko (Enter)", can, false, |s: &mut App| s.ask_neko());
    let ask = super::composer::with_chips(s, t, crate::compose::Dest::Ask, ask);
    let card = super::composer::card(t, "ask-card", ask, vec![super::composer::attach(t, "ask-attach", crate::compose::Dest::Ask)], vec![mode, model], send);
    let mut c = col().key("neko-foot").w_full().px(12.0).pb(12.0).gap(8.0);
    if let Some(tray) = super::compose::tray(s, t) {
        c = c.child(tray);
    }
    c = c.child(card);
    if let Some(error) = s.compose.error.as_ref() {
        c = c.child(text(error.clone()).key("ask-error").font(t.ty.meta).color(t.meaning.amber).wrap().live_region(rune::LiveRegion::Polite));
    }
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
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(|s: &mut App| s.layout.set_neko_open(true))
        .child(icon(&icons::SPARK, 18.0).color(t.text.base).a11y_hidden());
    if waiting > 0 {
        button = button.child(
            row().key("badge").absolute(18.0, 0.0).h(16.0).min_w(16.0).px(4.0).radius(8.0).center().fill(t.meaning.amber).a11y_hidden().child(
                text(if waiting > 99 { "99+".to_owned() } else { waiting.to_string() }).font(rune::TextStyle { weight: 500.0, ..t.ty.id }).color(t.surface.window),
            ),
        );
    }
    col().h_full().items_center().child(flat_band(t).px(0.0).justify_center().child(button))
}
