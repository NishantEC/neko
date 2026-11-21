//! The Routines page, drawn as the web's Inbox page (`pages/Inbox.tsx`): under the title "Inbox", a "Needs you" section of cards
//! (the glyph that marks one done, its title and where it came from, the detail, "Default: ...", when), "Closed recently" (folded),
//! then one folding section per scope with its routine rows (name, status, parts, when it runs, hover actions, the switch), its
//! standing orders, setup notes and "What Neko did". The sessions column keeps calling the entry "Routines". Rune keeps what the web
//! does in a dialog inline: pressing a routine (or its pencil) opens its editor under it, one at a time.

use neko_protocol::routines::{RoutineRun, RoutineView};
use rune::{El, TextStyle, col, el, icon, row, text};

use super::setup::workspaces::routine_status;
use super::ui::{MenuEntry, area, group, note, page, pick, quiet_line, setting_row, switch, with_menu};
use super::webkit::{self, font, relative_time};
use super::{primary_button, quiet_button};
use crate::app::App;
use crate::icons;
use crate::link::InboxItem;
use crate::pages::{ago, cadence_text, is_goal_routine, model_options};
use crate::status::{StatusKind, status_glyph};
use crate::theme::Neko;

/// How many items a list shows before "Show N more" (`SHORT`).
const SHORT: usize = 5;

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let Some(data) = &s.pages.routines else {
        let line = match s.pages.notes.get("page:routines") {
            Some(why) => why.clone(),
            None => "Looking for your routines…".to_owned(),
        };
        return page(t, "routines", "Rounds", None, Vec::new(), quiet_line(t, "routines-loading", &line));
    };
    let views: Vec<&RoutineView> = data.views.iter().filter(|v| !is_goal_routine(v) && s.scope().is_none_or(|w| v.routine.workspace_id == w)).collect();
    let mut body = col().w_full().gap(28.0);
    if !data.routines_on {
        body = body.child(
            row()
                .key("routines-off")
                .w_full()
                .gap(8.0)
                .center_y()
                .child(row().gap(4.0).flex_wrap().child(text("Routines are off.").font(font(t, 13.0, 400.0, 20.0)).color(t.text.base)).child(text(" Neko will not check messages, PRs or your day until you turn them on.").font(font(t, 13.0, 400.0, 20.0)).color(t.text.muted)))
                .child(webkit::button(t, "routines-turn-on", "Turn on", None, false, false, |s: &mut App| s.call(crate::link::Call::SetRoutinesOn { enabled: true }))),
        );
    }
    let items = crate::link::inbox_items(&data.views.iter().filter(|v| s.scope().is_none_or(|w| v.routine.workspace_id == w)).cloned().collect::<Vec<_>>(), s.now_ms);
    let (closed, open): (Vec<InboxItem>, Vec<InboxItem>) = items.into_iter().partition(|i| i.closed_ms.is_some());
    let where_of = |i: &InboxItem| -> String {
        let scope = data.views.iter().find(|v| v.routine.id == i.routine_id).and_then(|v| data.scopes.iter().find(|sc| sc.workspace_id.as_deref() == Some(v.routine.workspace_id.as_str()))).map_or("Scope", |sc| sc.name.as_str());
        format!("{scope} · {}", i.from)
    };
    body = body.child(needs_you(s, t, layer, &open, &where_of));
    if !closed.is_empty() {
        body = body.child(closed_recently(s, t, layer, &closed, &where_of));
    }
    let mut any = false;
    for sc in &data.scopes {
        let Some(ws) = sc.workspace_id.clone() else { continue };
        let list: Vec<&RoutineView> = views.iter().copied().filter(|v| v.routine.workspace_id == ws).collect();
        if list.is_empty() && sc.problems.is_empty() {
            continue;
        }
        any = true;
        body = body.child(scope_section(s, t, layer, data, sc, &ws, &list));
    }
    if !any {
        body = body.child(empty_block(t));
    }
    page(t, "routines", "Rounds", None, Vec::new(), body)
}

/// `EmptyBlock`: no routines yet.
fn empty_block(t: &Neko) -> El {
    col()
        .key("routines-empty")
        .w_full()
        .px(24.0)
        .py(48.0)
        .gap(12.0)
        .items_center()
        .child(icon(&icons::INBOX, 20.0).color(t.text.muted).a11y_hidden())
        .child(text("No routines yet").font(font(t, 14.0, 500.0, 20.0)).color(t.text.base))
        .child(text("Neko creates them for your home folder and for each project folder with its own MCP setup in Documents, code, Projects or a similar folder.").font(font(t, 14.0, 400.0, 20.0)).color(t.text.muted).wrap().max_w(384.0).min_w(0.0))
}

/// A `Panel` of the inbox: no gap, bleeding 12 either side.
fn panel(t: &Neko, rows: Vec<El>) -> El {
    webkit::panel(t, rows)
}

/// `ShortList`'s "Show N more …" line: 12 at the sides, 4 above and below.
fn more_line(t: &Neko, flag: &str, all: bool, hidden: usize, noun: &str) -> El {
    let label = if all { "Show fewer".to_owned() } else { format!("Show {hidden} more {noun}") };
    let key = flag.to_owned();
    row().w_full().mt(2.0).px(12.0).py(4.0).child(row().key(format!("{flag}-more")).role("button").label(label.clone()).expanded(all).focusable(true).radius(6.0).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(move |s: &mut App| s.toggle_inbox_flag(&key)).child(text(label).font(font(t, 12.5, 400.0, 18.75)).color(t.text.muted).a11y_hidden()))
}

fn needs_you(s: &App, t: &Neko, layer: &mut Vec<El>, open: &[InboxItem], where_of: &dyn Fn(&InboxItem) -> String) -> El {
    let count = (!open.is_empty()).then(|| open.len().to_string());
    // The status line the web keeps for screen readers: one text, in its place and unseen.
    let said = if open.is_empty() { "Nothing needs you.".to_owned() } else { format!("{} thing{} only you can do.", open.len(), if open.len() == 1 { "" } else { "s" }) };
    let flag = "needs-all";
    let all = s.pages.inbox_flags.contains(flag);
    let mut rows: Vec<El> = Vec::new();
    for item in if all { open.iter().collect::<Vec<_>>() } else { open.iter().take(SHORT).collect::<Vec<_>>() } {
        rows.push(needs_card(s, t, layer, item, &where_of(item)));
    }
    if open.len() > SHORT {
        rows.push(more_line(t, flag, all, open.len() - SHORT, "items"));
    }
    let body = if open.is_empty() { text("Nothing needs you right now.").key("needs-none").font(font(t, 13.5, 400.0, 20.25)).color(t.text.muted) } else { panel(t, rows) };
    // The web's status line for screen readers: the count, said politely when it changes.
    let body = body.role("status").label(said).live_region(rune::LiveRegion::Polite);
    webkit::section(t, "needs-you", "Needs you", count.as_deref(), None, body)
}

/// One thing only you can do (`NeedsYouItem`): the glyph (press to mark it done), the title and where it came from, the detail,
/// "Default: ..." or "No default", and when. Use the default, Mark done and Open sit over the time on hover and in the right-click menu.
fn needs_card(s: &App, t: &Neko, layer: &mut Vec<El>, item: &InboxItem, whence: &str) -> El {
    let seq = item.seq;
    let expanded = s.pages.inbox_flags.contains(&format!("item:{seq}"));
    let flag = format!("item:{seq}");
    let actions = s.item_actions(item);
    let goal = item.goal.is_some();
    let done_item = item.clone();
    let glyph = row()
        .key(format!("needs-done-{seq}"))
        .role("button")
        .label(format!("Mark “{}” done", item.title))
        .focusable(true)
        .size(22.0, 22.0)
        .mx(-4.0)
        .center()
        .radius(11.0)
        .hover(t.text.base.alpha(0.06))
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.mark_item_done(&done_item))
        .child(status_glyph(StatusKind::InProgress, t, 14.0, Some("")).a11y_hidden());
    let mut texts = col().grow(1.0).shrink().min_w(0.0).gap(2.0).child(
        row().w_full().min_h(20.8).gap(12.0).center_y().child(text(item.title.clone()).key(format!("needs-title-{seq}")).font(font(t, 13.5, 400.0, 20.25)).color(t.text.base).truncate().min_w(0.0).shrink().a11y_hidden()).child(text(whence.to_owned()).key(format!("needs-where-{seq}")).font(font(t, 12.5, 400.0, 18.75)).color(t.text.muted).group_opacity(1.0, 0.0).a11y_hidden()),
    );
    if !item.detail.is_empty() {
        let d = text(item.detail.clone()).key(format!("needs-detail-{seq}")).font(font(t, 12.5, 400.0, 18.75)).color(t.text.muted).min_w(0.0).max_w(567.8).a11y_hidden();
        texts = texts.child(if expanded { d.wrap() } else { d.truncate() });
    }
    if !goal {
        let words = item.default.as_ref().map_or("No default".to_owned(), |d| format!("Default: {d}"));
        texts = texts.child(text(words).key(format!("needs-default-{seq}")).font(font(t, 12.5, 400.0, 18.75)).color(t.text.muted.alpha(0.8)).truncate().min_w(0.0).max_w(567.8).a11y_hidden());
    }
    let press = texts.role("button").label(item.title.clone()).expanded(expanded).focusable(true).radius(6.0).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(move |s: &mut App| s.toggle_inbox_flag(&flag));
    let mut hover: Vec<super::setup::kit::RowAct> = Vec::new();
    for a in &actions {
        let (key, glyph_icon): (String, &'static rune::Icon) = if a.label.starts_with("Use the default") || a.label.starts_with("Answer with the default") { (format!("needs-use-{seq}"), &icons::PLAY) } else if a.label == "Mark done" { (format!("needs-mark-{seq}"), &icons::CHECK_LINE) } else if a.label == "Open" { (format!("needs-open-{seq}"), &icons::EXTERNAL) } else { continue };
        let act = a.act.clone();
        hover.push((key, a.label.clone(), glyph_icon, Box::new(move |s: &mut App| s.run_item_action(act.clone()))));
    }
    let when = text(relative_time(s.now_ms, item.at_ms)).key(format!("needs-when-{seq}")).font(webkit::tnum(font(t, 12.5, 400.0, 18.75))).color(t.text.muted.alpha(0.7)).a11y_hidden();
    let end = row().h(20.0).center_y().child(when);
    let mut card = row().key(format!("needs-{seq}")).w_full().px(12.0).py(8.0).gap(12.0).radius(10.0).hover_group().hover(t.text.base.alpha(0.03)).child(row().size(14.0, 20.0).center().child(glyph)).child(press).child(end);
    if !hover.is_empty() {
        card = card.child(super::setup::kit::hover_actions(t, hover));
    }
    let mut entries: Vec<MenuEntry> = Vec::new();
    for a in actions {
        let act = a.act.clone();
        entries.push((a.label, std::rc::Rc::new(move |s: &mut App| s.run_item_action(act.clone()))));
    }
    let mut out = col().w_full().min_w(0.0).child(with_menu(s, t, layer, &format!("needs:{seq}"), card, entries));
    // A goal's own buttons answer its items.
    if goal {
        let mut line = row().key(format!("needs-goal-{seq}")).w_full().pl(44.0).pr(12.0).gap(8.0).flex_wrap();
        for (i, a) in s.item_actions(item).into_iter().enumerate() {
            let act = a.act.clone();
            line = line.child(webkit::ghost_button(t, &format!("needs-goal-{seq}-{i}"), &a.label, None, move |s: &mut App| s.run_item_action(act.clone())));
        }
        out = out.child(line);
    }
    out
}

fn closed_recently(s: &App, t: &Neko, layer: &mut Vec<El>, closed: &[InboxItem], where_of: &dyn Fn(&InboxItem) -> String) -> El {
    let flag = "closed-open";
    let open = s.pages.inbox_flags.contains(flag);
    let all_flag = "closed-all";
    let all = s.pages.inbox_flags.contains(all_flag);
    let body = if open {
        let mut rows: Vec<El> = Vec::new();
        for item in if all { closed.iter().collect::<Vec<_>>() } else { closed.iter().take(SHORT).collect::<Vec<_>>() } {
            let seq = item.seq;
            let again = item.clone();
            let whence = where_of(item);
            let scope_only = whence.split(" · ").next().unwrap_or("Scope").to_owned();
            let row_el = row()
                .key(format!("closed-{seq}"))
                .w_full()
                .min_h(32.0)
                .px(12.0)
                .py(6.0)
                .gap(12.0)
                .center_y()
                .radius(10.0)
                .hover_group()
                .hover(t.text.base.alpha(0.03))
                .child(text(item.title.clone()).font(font(t, 13.5, 400.0, 20.25)).color(t.text.muted).truncate().min_w(0.0).shrink().grow(1.0).help(if item.detail.is_empty() { item.title.clone() } else { item.detail.clone() }).a11y_hidden())
                .child(text(scope_only).font(font(t, 12.5, 400.0, 18.75)).color(t.text.muted.alpha(0.7)).truncate().a11y_hidden())
                .child(text(format!("{} · {}", if item.reason.is_empty() { "Closed" } else { item.reason.as_str() }, relative_time(s.now_ms, item.closed_ms.unwrap_or(0)))).font(webkit::tnum(font(t, 12.5, 400.0, 18.75))).color(t.text.muted.alpha(0.7)).a11y_hidden())
                .child(super::setup::kit::hover_actions(t, vec![(format!("closed-reopen-{seq}"), format!("Reopen “{}”", item.title), &icons::UNDO, Box::new(move |s: &mut App| s.reopen_item(&again)))]));
            let again = item.clone();
            let entries: Vec<MenuEntry> = vec![("Reopen".to_owned(), std::rc::Rc::new(move |s: &mut App| s.reopen_item(&again)))];
            rows.push(with_menu(s, t, layer, &format!("closed:{seq}"), row_el, entries));
        }
        if closed.len() > SHORT {
            rows.push(more_line(t, all_flag, all, closed.len() - SHORT, "closed items"));
        }
        panel(t, rows)
    } else {
        el()
    };
    webkit::section_collapsible(t, "closed", "Closed recently", Some(&closed.len().to_string()), open, flag, body)
}

#[allow(clippy::too_many_arguments)]
fn scope_section(s: &App, t: &Neko, layer: &mut Vec<El>, data: &crate::link::RoutinesData, sc: &neko_protocol::routines::ScopeProposal, ws: &str, list: &[&RoutineView]) -> El {
    let flag = format!("scope-closed:{ws}");
    let open = !s.pages.inbox_flags.contains(&flag);
    let mut rows: Vec<El> = Vec::new();
    for v in list {
        rows.push(routine_row(s, t, layer, v));
        if s.pages.open_routine.as_deref() == Some(v.routine.id.as_str()) {
            rows.push(details(s, t, layer, v));
        }
    }
    if !list.is_empty() {
        rows.push(standing_orders(s, t, data, sc, ws));
    }
    if !sc.problems.is_empty() {
        let mut ul = col().key(format!("routines-notes-{ws}")).w_full().px(12.0).py(6.0).gap(4.0);
        for p in &sc.problems {
            ul = ul.child(row().gap(8.0).child(text("!").font(font(t, 12.5, 400.0, 18.75)).color(t.meaning.amber).a11y_hidden()).child(text(p.clone()).font(font(t, 12.5, 400.0, 18.75)).color(t.meaning.amber).wrap().min_w(0.0)));
        }
        rows.push(ul);
    }
    if !list.is_empty() {
        rows.push(what_neko_did(s, t, ws, list));
    }
    let body = if open { panel(t, rows) } else { el() };
    webkit::section_collapsible_keyed(t, &format!("routines-group-{ws}"), &sc.name, None, open, &flag, body)
}

/// "What Neko did": the lines the successful runs of this scope reported, newest first, twenty at most, five shown.
fn what_neko_did(s: &App, t: &Neko, ws: &str, list: &[&RoutineView]) -> El {
    let mut did: Vec<(String, i64, String)> = list.iter().flat_map(|v| v.runs.iter().filter(|r| r.ok).flat_map(move |r| r.did.iter().map(move |line| (line.clone(), r.finished_ms.unwrap_or(r.started_ms), v.routine.name.clone())))).collect();
    did.sort_by_key(|(_, at, _)| std::cmp::Reverse(*at));
    did.truncate(20);
    let flag = format!("did-all:{ws}");
    let all = s.pages.inbox_flags.contains(&flag);
    let mut c = col().key(format!("routines-did-{ws}")).w_full().mt(12.0).child(text("What Neko did").font(font(t, 12.5, 500.0, 18.75)).color(t.text.muted).px(12.0).pb(4.0));
    if did.is_empty() {
        return c.child(text("Nothing yet.").font(font(t, 13.0, 400.0, 20.0)).color(t.text.muted).px(12.0).py(4.0));
    }
    let total = did.len();
    for (i, (line, at, name)) in did.into_iter().enumerate() {
        if !all && i >= SHORT {
            break;
        }
        c = c.child(
            row()
                .w_full()
                .px(12.0)
                .py(4.0)
                .gap(12.0)
                .center_y()
                .radius(10.0)
                .hover(t.text.base.alpha(0.03))
                .child(text(line.clone()).font(font(t, 13.0, 400.0, 19.5)).color(t.text.base.alpha(0.9)).truncate().min_w(0.0).shrink().grow(1.0).help(line).a11y_hidden())
                .child(text(format!("{name} · {}", relative_time(s.now_ms, at))).font(font(t, 12.5, 400.0, 18.75)).color(t.text.muted.alpha(0.7)).a11y_hidden()),
        );
    }
    if total > SHORT {
        c = c.child(more_line(t, &flag, all, total - SHORT, "actions"));
    }
    c
}

/// The scope's standing orders (`StandingOrders`): folded to the first line of them; open, the box and Save.
fn standing_orders(s: &App, t: &Neko, data: &crate::link::RoutinesData, sc: &neko_protocol::routines::ScopeProposal, ws: &str) -> El {
    let text_now = data.standing_orders.get(ws).cloned().unwrap_or_default();
    let owner = if data.owner_name.trim().is_empty() { "the owner" } else { data.owner_name.as_str() };
    let first = text_now.split('\n').next().unwrap_or("").replace("{owner}", owner);
    let flag = format!("orders-open:{ws}");
    let open = s.pages.inbox_flags.contains(&flag);
    let head = row()
        .key(format!("orders-head-{ws}"))
        .role("button")
        .label("Standing orders")
        .expanded(open)
        .focusable(true)
        .w_full()
        .py(2.0)
        .gap(12.0)
        .center_y()
        .radius(6.0)
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.toggle_inbox_flag(&flag))
        .child(icon(if open { &icons::CHEVRON_DOWN } else { &icons::CHEVRON_RIGHT }, 12.0).color(t.text.muted).a11y_hidden())
        .child(text("Standing orders").font(font(t, 13.5, 400.0, 20.25)).color(t.text.base).a11y_hidden())
        .child(text(first).font(font(t, 12.5, 400.0, 18.75)).color(t.text.muted).truncate().min_w(0.0).shrink().grow(1.0).a11y_hidden());
    let mut c = col().key(format!("orders-block-{ws}")).w_full().px(12.0).py(6.0).radius(10.0).hover(t.text.base.alpha(0.03)).child(head);
    if open {
        let key = format!("orders:{ws}");
        let mut inner = col().key("standing-orders").w_full().mt(8.0).pl(24.0).gap(8.0).child(area(s, t, &key, &format!("Standing orders for {}", sc.name), &format!("Every routine in this scope follows these. {{owner}} stands for {owner}."), 4));
        if s.orders_dirty(ws) {
            let save = ws.to_owned();
            inner = inner.child(row().child(webkit::button(t, &format!("orders-save-{ws}"), "Save standing orders", None, true, false, move |s: &mut App| s.save_orders(&save))));
        }
        if let Some(n) = note(s, t, &key) {
            inner = inner.child(n);
        }
        c = c.child(inner);
    }
    c
}

/// A routine's row (`RoutineRow`): its name, its status in the one language, the parts of its last run, when it runs, then Run now and
/// Edit on hover and the switch; under it, why the last run failed and what it said.
fn routine_row(s: &App, t: &Neko, layer: &mut Vec<El>, v: &RoutineView) -> El {
    let routine = &v.routine;
    let id = routine.id.clone();
    let toggle_id = id.clone();
    let open = s.pages.open_routine.as_deref() == Some(routine.id.as_str());
    let (kind, word) = routine_status(v, s.now_ms);
    let last = v.runs.first();
    let queued = v.state.run_requested && !v.state.running;
    let facts = {
        let mut parts = vec![cadence_text(&routine.cadence)];
        if !(v.state.running || queued || !routine.enabled) {
            parts.push(match v.state.next_due_ms {
                Some(due) if due > s.now_ms => format!("next {}", relative_time(s.now_ms, due)),
                _ => "due now".to_owned(),
            });
        }
        if let Some(r) = last {
            parts.push(format!("last {}", relative_time(s.now_ms, r.finished_ms.unwrap_or(r.started_ms))));
        }
        parts.retain(|p| !p.is_empty());
        parts.join(" · ")
    };
    let ink = if kind == StatusKind::Failed { t.meaning.coral } else { t.text.base.alpha(0.8) };
    let mut line = row()
        .key(format!("routine-{id}"))
        .role("button")
        .label(format!("{}, {word}, {facts}", routine.name))
        .expanded(open)
        .focusable(true)
        .w_full()
        .min_h(28.0)
        .gap(12.0)
        .center_y()
        .on_press({
            let id = id.clone();
            move |s: &mut App| s.open_routine(&id)
        })
        .child(text(routine.name.clone()).key(format!("routine-name-{id}")).font(font(t, 13.5, 400.0, 20.25)).color(t.text.base).a11y_hidden())
        .child(row().key(format!("routine-status-{id}")).gap(6.0).center_y().child(status_glyph(kind, t, 14.0, Some("")).a11y_hidden()).child(text(word.clone()).font(font(t, 13.0, 400.0, 19.5)).color(ink).a11y_hidden()));
    let parts: (usize, usize) = last.map_or((0, 0), |r| (r.needs_you.iter().filter(|i| i.done).count(), r.needs_you.len()));
    if parts.1 > 0 {
        line = line.child(row().key(format!("routine-parts-{id}")).role("img").label(format!("{} of {} done", parts.0, parts.1)).gap(4.0).center_y().child(super::sessions::progress_ring(Some((parts.0 as u32, parts.1 as u32)), t)).child(row().children([parts.0.to_string(), "/".to_owned(), parts.1.to_string()].into_iter().map(|p| text(p).font(webkit::tnum(font(t, 12.0, 400.0, 19.5))).color(t.text.muted).a11y_hidden()))));
    }
    let busy = v.state.running || v.state.run_requested;
    let (run_id, edit_id) = (id.clone(), id.clone());
    let act = |key: String, label: String, glyph: &'static rune::Icon, enabled: bool, f: Box<dyn Fn(&mut App)>| {
        row().key(key).role("button").label(label.clone()).help(label).focusable(enabled).size(28.0, 28.0).center().radius(10.0).hover(t.text.base.alpha(0.06)).focus_ring(2.0, 2.0, t.kit.c.border_focus).group_opacity(0.0, 1.0).on_press(webkit::gated(enabled, move |s: &mut App| f(s))).child(icon(glyph, 15.0).color(t.text.muted.alpha(if enabled { 1.0 } else { 0.4 })).a11y_hidden())
    };
    line = line
        .child(row().grow(1.0).shrink().min_w(0.0).justify_end().child(text(facts).key(format!("routine-facts-{id}")).font(font(t, 12.5, 400.0, 18.75)).color(t.text.muted).truncate().min_w(0.0).shrink().a11y_hidden()))
        .child(row().gap(2.0).center_y().child(act(format!("routine-run-{id}"), format!("Run {} now", routine.name), &icons::PLAY, !busy, Box::new(move |s: &mut App| s.run_routine(&run_id)))).child(act(format!("routine-edit-{id}"), format!("Edit {}", routine.name), &icons::PENCIL, true, Box::new(move |s: &mut App| {
            if s.pages.open_routine.as_deref() != Some(edit_id.as_str()) {
                s.open_routine(&edit_id);
            }
        }))))
        .child(webkit::toggle(t, &format!("routine-switch-{id}"), &format!("{} {}", if routine.enabled { "Pause" } else { "Resume" }, routine.name), routine.enabled, move |s: &mut App| {
            let on = s.pages.routine(&toggle_id).is_some_and(|v| v.routine.enabled);
            s.set_routine_enabled(&toggle_id, !on)
        }));
    // Everything the row does is also in its right-click menu.
    let mut entries: Vec<MenuEntry> = Vec::new();
    if !busy {
        let rid = id.clone();
        entries.push(("Run now".to_owned(), std::rc::Rc::new(move |s: &mut App| s.run_routine(&rid))));
    }
    let (eid, on) = (id.clone(), routine.enabled);
    entries.push((if on { "Pause" } else { "Resume" }.to_owned(), std::rc::Rc::new(move |s: &mut App| s.set_routine_enabled(&eid, !on))));
    let oid = id.clone();
    entries.push(("Edit".to_owned(), std::rc::Rc::new(move |s: &mut App| {
        if s.pages.open_routine.as_deref() != Some(oid.as_str()) {
            s.open_routine(&oid);
        }
    })));
    let mut out = col().key(format!("routine-block-{id}")).w_full().px(12.0).py(6.0).gap(4.0).radius(10.0).hover_group().hover(t.text.base.alpha(0.03)).child(with_menu(s, t, layer, &format!("routine:{id}"), line, entries));
    if let Some(r) = last.filter(|r| !r.ok) {
        out = out.child(text(r.error.clone().unwrap_or_default()).key(format!("routine-error-{id}")).font(font(t, 13.0, 400.0, 19.5)).color(t.meaning.coral).wrap().min_w(0.0));
        if !r.text.is_empty() {
            let flag = format!("reply:{id}");
            let shown = s.pages.inbox_flags.contains(&flag);
            out = out.child(row().child(webkit::link_button(t, &format!("routine-reply-{id}"), if shown { "Hide the agent’s reply" } else { "Show the agent’s reply" }, move |s: &mut App| s.toggle_inbox_flag(&flag))));
            if shown {
                out = out.child(text(r.text.clone()).font(TextStyle { ..font(t, 13.0, 400.0, 21.45) }).color(t.text.base.alpha(0.85)).wrap().min_w(0.0).padding(12.0).radius(12.0).fill(t.text.base.alpha(0.03)).border(1.0, t.text.base.alpha(0.08)));
            }
        }
    }
    if let Some(r) = last {
        let words = if !r.note.is_empty() { Some(r.note.clone()) } else if !r.runtime.is_empty() && r.runtime != "claude" { Some(format!("Last run used {}.", if r.runtime == "codex" { "Codex" } else { r.runtime.as_str() })) } else { None };
        if let Some(w) = words {
            out = out.child(text(w).font(font(t, 12.5, 400.0, 18.75)).color(t.text.muted).truncate().min_w(0.0));
        }
    }
    if let Some(n) = v.state.note.as_ref().filter(|n| !n.is_empty()) {
        out = out.child(text(n.clone()).font(font(t, 12.5, 400.0, 18.75)).color(t.text.muted).truncate().min_w(0.0));
    }
    out
}

/// The cadence, as the web's editor has it: a segmented control (Every, Daily); for Every a minutes field (5 to 720, in steps of
/// 5) with From and Until hour selects (0 to 23); for Daily an HH:MM time field. It is saved with `SaveRoutine`, the whole routine
/// with its cadence replaced, exactly as the web sends it.
fn cadence_editor(s: &App, t: &Neko, layer: &mut Vec<El>, cadence: &neko_protocol::routines::Cadence) -> El {
    use neko_protocol::routines::Cadence;
    use rune_kit::{SegmentedControl, TextInput};
    let every = matches!(cadence, Cadence::Every { .. });
    let mode = SegmentedControl::new(&["Every", "Daily"], Some(if every { 0 } else { 1 }))
        .label("How often")
        .width(Some(160.0))
        .on_pick(|s: &mut App, i, _| s.set_cadence_mode(i == 0))
        .build(&t.kit)
        .key("rt-cadence-mode");
    // One line for the segmented control and, beside it, what the mode needs.
    let mut fields = row().key("rt-cadence-fields").gap(6.0).center_y().wrap();
    let label = |words: &str| text(words.to_owned()).font(t.ty.meta).color(t.text.muted).a11y_hidden();
    match cadence {
        Cadence::Every { from_hour, to_hour, .. } => {
            let stepper = |key: &str, label: &str, by: i32| {
                row()
                    .key(key.to_owned())
                    .role("button")
                    .label(label.to_owned())
                    .focusable(true)
                    .size(28.0, 28.0)
                    .center()
                    .radius(t.radius.control)
                    .hover(t.surface.hover)
                    .focus_ring(2.0, 2.0, t.surface.ring)
                    .on_press(move |s: &mut App| s.cadence_minutes_step(by))
                    .child(text(if by < 0 { "−" } else { "+" }).font(t.ty.row).color(t.text.muted).a11y_hidden())
            };
            let minutes = TextInput::new(super::ui::edit_of(s, "rt:minutes"))
                .label("Every (minutes), 5 to 720 in steps of 5")
                .width(Some(64.0))
                .on_edit(super::ui::editing("rt:minutes".to_owned()))
                .on_change(|s: &mut App, _| s.cadence_minutes_typed())
                .build(&t.kit)
                .key("rt-minutes");
            fields = fields.child(stepper("rt-minutes-less", "5 minutes less", -5)).child(minutes).child(stepper("rt-minutes-more", "5 minutes more", 5)).child(label("min, from"));
            let hours = |from: bool| -> Vec<String> { (0..=23).map(|h| format!("{h:02}:00")).chain((!from && *to_hour == 24).then(|| "24:00".to_owned())).collect() };
            let (from_now, to_now) = (format!("{:02}:00", from_hour), format!("{:02}:00", to_hour));
            fields = fields.child(pick(s, t, layer, "rt-from-pick", "From hour", &hours(true), &from_now, 96.0, |s: &mut App, chosen: &str| s.cadence_hours(chosen.get(..2).and_then(|h| h.parse().ok()), None)));
            fields = fields.child(label("until")).child(pick(s, t, layer, "rt-to-pick", "Until hour", &hours(false), &to_now, 96.0, |s: &mut App, chosen: &str| s.cadence_hours(None, chosen.get(..2).and_then(|h| h.parse().ok()))));
        }
        Cadence::Daily { .. } => {
            let time = TextInput::new(super::ui::edit_of(s, "rt:time"))
                .label("At, as HH:MM")
                .placeholder("10:30")
                .width(Some(96.0))
                .on_edit(super::ui::editing("rt:time".to_owned()))
                .on_change(|s: &mut App, _| s.cadence_time_typed())
                .build(&t.kit)
                .key("rt-time");
            fields = fields.child(label("at")).child(time);
        }
    }
    let help = match cadence {
        Cadence::Every { from_hour, to_hour, .. } if from_hour == to_hour => "Same start and end hour: runs all day and night.".to_owned(),
        Cadence::Every { from_hour, to_hour, .. } => format!("Runs between {from_hour:02}:00 and {to_hour:02}:00, less often while nothing is happening."),
        Cadence::Daily { .. } => "Once a day, at this time.".to_owned(),
    };
    col().key("rt-cadence").w_full().gap(2.0).child(setting_row(t, "rt-cadence-row", "Cadence", Some(&help), mode)).child(row().w_full().pl(6.0).pb(4.0).child(fields))
}

/// The open routine: its editor, Run now, and its recent runs.
fn details(s: &App, t: &Neko, layer: &mut Vec<El>, v: &RoutineView) -> El {
    let id = v.routine.id.clone();
    let Some(draft) = s.pages.draft.clone() else { return el() };
    let dirty = s.routine_dirty();
    let mut c = col().key(format!("routine-details-{id}")).w_full().pl(36.0).pr(6.0).pt(4.0).pb(14.0).gap(6.0);
    // The last run and what was caught up after downtime live here, not on the row.
    let mut said = Vec::new();
    if let Some(ms) = v.state.last_run_ms {
        said.push(format!("Last run {}", ago(s.now_ms, ms)));
    }
    if let Some(n) = v.state.note.as_ref().filter(|n| !n.is_empty()) {
        said.push(n.clone());
    }
    if !said.is_empty() {
        c = c.child(text(said.join(" · ")).key(format!("routine-last-{id}")).font(t.ty.meta).color(t.text.muted).wrap().px(6.0).pb(2.0));
    }
    c = c.child(group(t, "Instruction")).child(area(s, t, "rt:instruction", "What this routine does each run", "", 6));

    c = c.child(cadence_editor(s, t, layer, &draft.cadence));

    let models = model_options(&draft.model);
    let model_labels: Vec<String> = models.iter().map(|(l, _)| l.clone()).collect();
    let model_now = models.iter().find(|(_, val)| *val == draft.model).map(|(l, _)| l.clone()).unwrap_or_default();
    let by = models.clone();
    c = c.child(setting_row(
        t,
        "rt-model",
        "Model",
        Some("A Claude Code model alias. Sonnet unless you choose another."),
        pick(s, t, layer, "rt-model-pick", "Model", &model_labels, &model_now, 200.0, move |s: &mut App, chosen: &str| {
            if let Some((_, value)) = by.iter().find(|(l, _)| l == chosen)
                && let Some(d) = s.pages.draft.as_mut()
            {
                d.model = value.clone();
            }
        }),
    ));
    c = c.child(setting_row(
        t,
        "rt-browser",
        "Use my Chrome",
        Some("Lets this routine work in your own Chrome, where you are signed in. Chrome has to be open."),
        switch(t, "rt-browser-switch", "Use my Chrome", draft.browser, |s: &mut App| {
            if let Some(d) = s.pages.draft.as_mut() {
                d.browser = !d.browser;
            }
        }),
    ));
    c = c.child(setting_row(
        t,
        "rt-computer",
        "Use my Mac’s apps",
        Some("Lets this routine see and operate Mac apps, in the background where it can."),
        switch(t, "rt-computer-switch", "Use my Mac’s apps", draft.computer, |s: &mut App| {
            if let Some(d) = s.pages.draft.as_mut() {
                d.computer = !d.computer;
            }
        }),
    ));
    c = c.child(setting_row(
        t,
        "rt-catch-up",
        "Catch up after downtime",
        Some("If Neko or the Mac was off across this routine’s times, run it once when it is back and say how many it missed."),
        switch(t, "rt-catch-up-switch", "Catch up after downtime", draft.catch_up, |s: &mut App| {
            if let Some(d) = s.pages.draft.as_mut() {
                d.catch_up = !d.catch_up;
            }
        }),
    ));

    // Run now is quiet; Save is the page's one primary button, and only when something changed.
    let run_id = id.clone();
    let busy = v.state.running || v.state.run_requested;
    let mut actions = row().gap(8.0).center_y().pt(4.0);
    if dirty {
        actions = actions.child(primary_button("Save", "rt-save", t, |s: &mut App| s.save_routine()));
    }
    actions = actions.child(if busy {
        quiet_button(if v.state.running { "Running now" } else { "Starts within 30 seconds" }, "rt-run", t, |_| {}).disabled(true)
    } else {
        quiet_button("Run now", "rt-run", t, move |s: &mut App| s.run_routine(&run_id))
    });
    c = c.child(actions);
    if let Some(n) = note(s, t, &format!("routine:{id}")) {
        c = c.child(n);
    }

    c = c.child(el().h(8.0)).child(group(t, "Recent runs"));
    if v.runs.is_empty() {
        c = c.child(quiet_line(t, "rt-no-runs", "Not run yet"));
    }
    for run in v.runs.iter().take(8) {
        c = c.child(run_row(s, t, run));
    }
    c
}

/// One run: when, what it did (or why it failed), and how many things need you.
fn run_row(s: &App, t: &Neko, run: &RoutineRun) -> El {
    let (kind, what) = if !run.ok {
        (StatusKind::Failed, run.error.clone().unwrap_or_else(|| "Failed".to_owned()))
    } else if run.did.is_empty() {
        (StatusKind::Done, "Quiet: nothing to do".to_owned())
    } else {
        let more = if run.did.len() > 1 { format!(" (+{} more)", run.did.len() - 1) } else { String::new() };
        (StatusKind::Done, format!("{}{more}", run.did[0]))
    };
    let open = run.needs_you.iter().filter(|i| !i.done).count();
    let mut r = row()
        .key(format!("run-{}", run.id))
        .w_full()
        .min_h(28.0)
        .px(6.0)
        .gap(10.0)
        .center_y()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .help(what.clone())
        .child(status_glyph(kind, t, 12.0, Some(if run.ok { "Done" } else { "Failed" })))
        .child(text(ago(s.now_ms, run.finished_ms.unwrap_or(run.started_ms))).font(t.ty.id).color(t.text.faint).w(64.0))
        .child(text(what).font(t.ty.meta).color(if run.ok { t.text.base } else { t.meaning.coral }).truncate().grow(1.0).min_w(0.0));
    if open > 0 {
        r = r.child(text(format!("{open} {}", if open == 1 { "needs you" } else { "need you" })).key(format!("run-needs-{}", run.id)).font(t.ty.meta).color(t.meaning.amber));
    } else if !run.needs_you.is_empty() {
        r = r.child(text("all handled").font(t.ty.meta).color(t.text.faint));
    }
    r
}
