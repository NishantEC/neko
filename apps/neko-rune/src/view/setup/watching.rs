//! Setup, Watching: what Neko keeps an eye on, grouped by workspace. A watch is an instruction plus the
//! connected tools allowed to answer it. Add one with the web's form, edit, rename, delete (with a
//! confirmation), check now, turn it on or off (turning on is a decision and says what it allows), and ask
//! Neko to suggest watches from the tools. Ported from `pages/setup/Watching.tsx`.

use neko_protocol::mcp_host::Responsibility;
use rune::{El, col, row, text};

use super::Page;
use super::kit::{self, action, details_block, empty, glyph, list_row};
use super::state::SetupSection;
use super::words::{self, Tone};
use crate::app::App;
use crate::icons;
use crate::link::SetupData;
use crate::theme::Neko;
use crate::view::ui::{MenuEntry, note, switch, with_menu};
use crate::view::{quiet_button, primary_button};

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>) -> Page {
    let Some(data) = s.setup_data() else { return Page::quiet(t, "watch-none", "Looking for your watches…") };
    if data.workspaces.is_empty() {
        let body = empty(t, "watch-no-workspace", "Add a workspace to create a watch: watches use a workspace’s connected tools.", Some(("Open Workspaces", Box::new(|s: &mut App| s.open_setup(SetupSection::Workspaces)))));
        return Page { count: None, trailing: Vec::new(), body, title: None, back: None };
    }
    let first = data.workspaces[0].id.clone();
    // "Suggest watches" asks in the first workspace that has a tool to look through.
    let target = data.workspaces.iter().find(|w| data.connections.iter().any(|c| c.enabled && (c.workspace_id.is_empty() || c.workspace_id == w.id))).map(|w| w.id.clone());
    let suggest = match target {
        Some(ws) => quiet_button("Suggest watches", "watch-suggest", t, move |s: &mut App| s.suggest_watches(&ws)),
        None => quiet_button("Suggest watches", "watch-suggest", t, |_| {}).disabled(true).help("Connect a tool in Tools first: Neko suggests what those tools can check."),
    };
    let new = primary_button("New watch", "watch-new", t, move |s: &mut App| s.open_watch_dialog(None, &first));
    let mut body = col().w_full().gap(2.0);
    if let Some((pending, text_)) = s.suggestion_reply().filter(|_| !s.setup.dismissed_reply) {
        body = body.child(reply(t, pending, &text_));
    }
    if let Some(n) = note(s, t, "setup:page") {
        body = body.child(n);
    }
    if data.watches.is_empty() {
        body = body.child(empty(t, "watch-empty", "No watches yet. Describe one, or ask Neko to suggest some from your tools.", Some(("New watch", Box::new(move |s: &mut App| {
            let ws = s.setup_data().and_then(|d| d.workspaces.first().map(|w| w.id.clone())).unwrap_or_default();
            s.open_watch_dialog(None, &ws)
        })))));
    }
    let mut shown = 0;
    for w in &data.workspaces {
        let list: Vec<&Responsibility> = data.watches.iter().filter(|x| x.workspace_id == w.id).collect();
        if data.watches.is_empty() {
            break;
        }
        // A workspace with no watch has no group (the web lists watches, not workspaces); "New watch" picks the workspace.
        if list.is_empty() {
            continue;
        }
        body = body.child(group_head(t, s, &w.id, &w.name, list.len(), shown == 0));
        shown += 1;
        // Suggestions first, then the active, then the paused.
        let mut ordered: Vec<&Responsibility> = list.iter().copied().filter(|x| data.suggested.contains(&x.id)).collect();
        ordered.extend(list.iter().copied().filter(|x| !data.suggested.contains(&x.id) && x.enabled));
        ordered.extend(list.iter().copied().filter(|x| !data.suggested.contains(&x.id) && !x.enabled));
        for item in ordered {
            body = body.child(if data.suggested.contains(&item.id) { suggestion(s, t, layer, data, item) } else { watch_row(s, t, layer, data, item) });
        }
    }
    Page { count: (!data.watches.is_empty()).then_some(data.watches.len()), trailing: vec![suggest, new], body, title: None, back: None }
}

/// Neko's reply to "Suggest watches", under the header, with a way to hide it.
fn reply(t: &Neko, pending: bool, said: &str) -> El {
    let head = row().w_full().gap(8.0).center_y().child(text(if pending { "Looking through your tools…" } else { "Neko’s reply" }.to_owned()).font(t.ty.group).color(t.text.muted).grow(1.0));
    let head = if pending { head } else { head.child(quiet_button("Hide reply", "watch-reply-hide", t, |s: &mut App| s.setup.dismissed_reply = true)) };
    let mut c = col().key("watch-reply").w_full().gap(4.0).px(8.0).py(8.0).radius(t.radius.row).fill(t.surface.hover).child(head);
    if !pending {
        c = c.child(text(said.to_owned()).key("watch-reply-text").font(t.ty.meta).color(t.text.base).wrap().min_w(0.0));
    }
    c
}

/// A workspace's heading, with its own "New watch" and "Suggest" actions at the right.
fn group_head(t: &Neko, s: &App, id: &str, name: &str, count: usize, first: bool) -> El {
    let (a, b) = (id.to_owned(), id.to_owned());
    let has_tools = s.setup_data().is_some_and(|d| d.connections.iter().any(|c| c.enabled && (c.workspace_id.is_empty() || c.workspace_id == id)));
    let mut r = kit::heading(t, &format!("watch-group-{id}"), name, Some(count), first).hover_group();
    r = r.child(kit::fill());
    if has_tools {
        r = r.child(action(t, format!("watch-group-suggest-{id}"), &format!("Suggest watches in {name}"), &icons::SPARK, move |s: &mut App| s.suggest_watches(&b)));
    }
    r.child(action(t, format!("watch-group-new-{id}"), &format!("New watch in {name}"), &icons::PLUS, move |s: &mut App| s.open_watch_dialog(None, &a)))
}

fn watch_row(s: &App, t: &Neko, layer: &mut Vec<El>, data: &SetupData, w: &Responsibility) -> El {
    let id = w.id.clone();
    let open = s.row_open(&format!("watch:{id}"));
    let title = words::watch_title(w);
    let (status, tone) = words::watch_status(w, s.now_ms);
    let (toggle_id, sw_id) = (id.clone(), id.clone());
    let line = list_row(t, format!("watch-{id}"), format!("{title}, {status}"), Some(open))
        .on_press(move |s: &mut App| s.toggle_row(&format!("watch:{toggle_id}")))
        .child(glyph(t, words::watch_glyph(tone), &status))
        .child(text(title.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).help(w.instruction.clone()).a11y_hidden())
        .child(text(status.clone()).key(format!("watch-status-{id}")).font(t.ty.meta).color(if tone == Tone::Warning { t.meaning.amber } else { t.text.muted }).truncate().min_w(0.0).a11y_hidden());
    let (c, e, d) = (id.clone(), id.clone(), id.clone());
    let line = if w.enabled { line.child(action(t, format!("watch-check-{id}"), &format!("Check {title} now"), &icons::REFRESH, move |s: &mut App| s.setup_call(crate::link::SetupAction::Wake { id: c.clone() }))) } else { line };
    let line = line
        .child(action(t, format!("watch-edit-{id}"), "Edit", &icons::PENCIL, move |s: &mut App| s.open_watch_dialog(Some(&e), "")))
        .child(action(t, format!("watch-delete-{id}"), "Delete watch…", &icons::TRASH, move |s: &mut App| s.confirm_delete_watch(&d)))
        .child(switch(t, &format!("watch-switch-{id}"), &format!("Watch {title}"), w.enabled, move |s: &mut App| {
            let on = s.setup_data().and_then(|d| d.watch(&sw_id)).is_some_and(|w| w.enabled);
            s.watch_switch(&sw_id, !on)
        }));
    // The right-click menu: every action of the row.
    let mut entries: Vec<MenuEntry> = Vec::new();
    let enabled = w.enabled;
    if enabled {
        let i = id.clone();
        entries.push(("Check now".to_owned(), std::rc::Rc::new(move |s: &mut App| s.setup_call(crate::link::SetupAction::Wake { id: i.clone() }))));
    }
    let i = id.clone();
    entries.push(("Edit…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_watch_dialog(Some(&i), ""))));
    let i = id.clone();
    entries.push(("Rename…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_rename_watch(&i))));
    let i = id.clone();
    entries.push((if enabled { "Pause" } else { "Turn on…" }.to_owned(), std::rc::Rc::new(move |s: &mut App| s.watch_switch(&i, !enabled))));
    let i = id.clone();
    entries.push(("Delete watch…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.confirm_delete_watch(&i))));
    let mut out = col().key(format!("watch-block-{id}")).w_full().child(with_menu(s, t, layer, &format!("watch:{id}"), line, entries));
    if let Some(n) = note(s, t, &format!("setup:{id}")) {
        out = out.child(n);
    }
    if open {
        out = out.child(details(s, t, data, w));
    }
    out
}

fn details(s: &App, t: &Neko, data: &SetupData, w: &Responsibility) -> El {
    let id = w.id.as_str();
    let sources: Vec<String> = w.connection_ids.iter().map(|c| words::connection_label(data, c)).collect();
    let missing = words::ungranted(data, &w.workspace_id, &w.connection_ids);
    let mut c = details_block(format!("watch-details-{id}"));
    c = c.child(text(w.instruction.clone()).key(format!("watch-instruction-{id}")).font(t.ty.row).color(t.text.base).wrap().px(8.0).min_w(0.0));
    c = c.child(kit::detail(t, &format!("watch-d-status-{id}"), "Status", if w.enabled { "Watching" } else { "Paused" }, false));
    c = c.child(kit::detail(t, &format!("watch-d-last-{id}"), "Last checked", &w.last_attempt_ms.map_or("Not checked yet".to_owned(), |at| words::relative_time(Some(at), s.now_ms)), false));
    c = c.child(kit::detail(t, &format!("watch-d-sources-{id}"), "Sources", &if sources.is_empty() { "No sources selected".to_owned() } else { sources.join(", ") }, false));
    let delivery = if !data.plan_first {
        "Tickets start working right away · reviewed results are delivered"
    } else if w.prepare_low_risk {
        "Low-risk fixes allowed · reviewed results are delivered"
    } else {
        "Plans require approval before local work"
    };
    c = c.child(kit::detail(t, &format!("watch-d-delivery-{id}"), "Delivery", delivery, false));
    if w.failures > 0 {
        c = c.child(kit::amber(t, &format!("watch-d-failures-{id}"), &format!("{} consecutive failed {}", w.failures, if w.failures == 1 { "check" } else { "checks" })));
    }
    if !missing.is_empty() {
        let ws = w.workspace_id.clone();
        c = c.child(row().w_full().gap(8.0).center_y().child(kit::amber(t, &format!("watch-d-missing-{id}"), &format!("Connect or discover tools for {} to turn this on.", missing.join(", ")))).child(quiet_button("Review connections", &format!("watch-review-{id}"), t, move |s: &mut App| {
            s.setup.grant_workspace = Some(ws.clone());
            s.open_setup(SetupSection::Tools)
        })));
    }
    if !w.last_result.trim().is_empty() {
        c = c.child(text("Last check result".to_owned()).font(t.ty.meta).color(t.text.muted).px(8.0).pt(4.0));
        c = c.child(text(w.last_result.clone()).key(format!("watch-d-result-{id}")).font(t.ty.meta).color(t.text.base).wrap().min_w(0.0).px(8.0).py(8.0).radius(t.radius.row).fill(t.surface.hover));
    }
    let (a, b) = (id.to_owned(), id.to_owned());
    let check = if w.enabled { quiet_button("Check now", &format!("watch-d-check-{id}"), t, move |s: &mut App| s.setup_call(crate::link::SetupAction::Wake { id: a.clone() })) } else { quiet_button("Check now", &format!("watch-d-check-{id}"), t, |_| {}).disabled(true) };
    c.child(row().w_full().gap(8.0).px(8.0).pt(4.0).child(check).child(quiet_button("Edit watch", &format!("watch-d-edit-{id}"), t, move |s: &mut App| s.open_watch_dialog(Some(&b), ""))))
}

/// A watch Neko proposed: nothing runs until you turn it on.
fn suggestion(s: &App, t: &Neko, layer: &mut Vec<El>, data: &SetupData, w: &Responsibility) -> El {
    let id = w.id.clone();
    let missing = words::ungranted(data, &w.workspace_id, &w.connection_ids);
    let sources: Vec<String> = w.connection_ids.iter().map(|c| words::connection_label(data, c)).collect();
    let (words_, ink) = if !missing.is_empty() {
        (format!("Needs access to {}", missing.join(", ")), t.meaning.amber)
    } else if sources.is_empty() {
        ("Ready to review".to_owned(), t.text.muted)
    } else {
        (sources.join(", "), t.text.muted)
    };
    let label = if missing.is_empty() { "Turn on…" } else { "Review connections" };
    let (on, e, d) = (id.clone(), id.clone(), id.clone());
    let line = list_row(t, format!("suggestion-{id}"), format!("Suggested watch: {}", w.instruction), None)
        .on_press({
            let i = id.clone();
            move |s: &mut App| s.watch_turn_on(&i)
        })
        .child(glyph(t, crate::status::StatusKind::Backlog, "Suggested"))
        .child(text(w.instruction.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).a11y_hidden())
        .child(text(words_).key(format!("suggestion-sources-{id}")).font(t.ty.meta).color(ink).truncate().min_w(0.0).max_w(260.0).a11y_hidden())
        .child(action(t, format!("suggestion-on-{id}"), label, &icons::EYE, move |s: &mut App| s.watch_turn_on(&on)))
        .child(action(t, format!("suggestion-edit-{id}"), "Edit", &icons::PENCIL, move |s: &mut App| s.open_watch_dialog(Some(&e), "")))
        .child(action(t, format!("suggestion-dismiss-{id}"), "Dismiss…", &icons::TRASH, move |s: &mut App| s.confirm_delete_watch(&d)));
    let (m1, m2, m3) = (id.clone(), id.clone(), id.clone());
    let entries: Vec<MenuEntry> = vec![
        (label.to_owned(), std::rc::Rc::new(move |s: &mut App| s.watch_turn_on(&m1))),
        ("Edit…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_watch_dialog(Some(&m2), ""))),
        ("Dismiss…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.confirm_delete_watch(&m3))),
    ];
    let mut out = col().key(format!("suggestion-block-{id}")).w_full().child(with_menu(s, t, layer, &format!("suggestion:{id}"), line, entries));
    if let Some(n) = note(s, t, &format!("setup:{id}")) {
        out = out.child(n);
    }
    out
}
