//! Setup, Workspaces, as the web draws it (`pages/setup/Workspaces.tsx`): every workspace is a 40 point head line (caret, name, how many
//! tasks are open, how many routines are on; at the right what needs you and its folder, with Sync now and Edit over the folder on
//! hover) and, open, its detail lines under it: Routines, Setup notes, Folders, Connections, Watching, Tools in Neko, Standing orders
//! and Instructions, then the empty facets offered as quiet links. A line with more to show opens it under itself. Every workspace
//! shows expanded until the person folds it.

use neko_protocol::routines::RoutineView;
use rune::{El, Family, TextStyle, col, el, icon, row, text};

use super::state::SetupSection;
use crate::app::App;
use crate::icons;
use crate::link::SetupData;
use crate::pages::is_goal_routine;
use crate::status::{StatusKind, status_glyph};
use crate::theme::Neko;
use crate::view::ui::{MenuEntry, note, with_menu};
use crate::view::webkit::{self, font, relative_time};
use crate::views::FullView;

/// How many items a list shows before "Show N more" (`SHORT`).
const SHORT: usize = 5;
/// The detail lines' left edge past the caret: 11 + 8 (gap) + 128 (label) + 12 (gap).
const INDENT: f32 = 159.0;

fn folder_name(path: &str) -> String {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path).to_owned()
}

fn mono(t: &Neko, size: f32, line_height: f32) -> TextStyle {
    TextStyle { family: Family::Mono, ..font(t, size, 400.0, line_height) }
}

/// What a workspace's routines add up to (`routineSummary`).
struct Summary {
    count: usize,
    enabled: usize,
    needs_you: usize,
    failing: Vec<String>,
    text: String,
}

fn routine_summary(views: &[&RoutineView], now_ms: i64) -> Summary {
    let enabled = views.iter().filter(|v| v.routine.enabled).count();
    let failing: Vec<String> = views.iter().filter(|v| v.runs.first().is_some_and(|r| !r.ok)).map(|v| v.routine.name.clone()).collect();
    let mut parts = vec![if views.is_empty() { "No routines here yet".to_owned() } else { format!("{enabled} of {} on", views.len()) }];
    if let Some(running) = views.iter().find(|v| v.state.running) {
        parts.push(format!("{} is running now", running.routine.name));
    } else if let Some((at, name, run)) = views.iter().flat_map(|v| v.runs.iter().map(move |r| (r.finished_ms.unwrap_or(r.started_ms), &v.routine.name, r))).max_by_key(|(at, _, _)| *at) {
        parts.push(format!("last: {name} {} {}", if !run.ok { "failed" } else if run.quiet { "was quiet" } else { "acted" }, relative_time(now_ms, at)));
    } else if !views.is_empty() {
        parts.push("none has run yet".to_owned());
    }
    Summary {
        count: views.len(),
        enabled,
        needs_you: views.iter().map(|v| v.runs.iter().map(|r| r.needs_you.iter().filter(|i| !i.done).count()).sum::<usize>()).sum(),
        failing,
        text: parts.join(" · "),
    }
}

/// A routine in the one status language (`setup/routine-status.ts`).
pub fn routine_status(v: &RoutineView, now_ms: i64) -> (StatusKind, String) {
    let last = v.runs.first();
    if v.state.running {
        return (StatusKind::InProgress, "Running".to_owned());
    }
    if v.state.run_requested {
        return (StatusKind::Todo, "Queued".to_owned());
    }
    if !v.routine.enabled {
        return (StatusKind::Backlog, "Paused".to_owned());
    }
    if let Some(hold) = v.state.hold.as_ref().filter(|h| h.until_ms > now_ms) {
        return (StatusKind::Backlog, format!("Held ({})", if hold.reason.is_empty() { "usage limit" } else { hold.reason.as_str() }));
    }
    match last {
        None => (StatusKind::Backlog, "Not run yet".to_owned()),
        Some(r) if !r.ok => (StatusKind::Failed, "Failed".to_owned()),
        Some(r) => (StatusKind::Done, if r.quiet { "Quiet" } else { "Acted" }.to_owned()),
    }
}

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>) -> super::Page {
    let Some(data) = s.setup_data() else { return super::Page::quiet(t, "ws-none", "Looking for your workspaces…") };
    let add = crate::view::page_button("Add workspace", "ws-add", t, |s: &mut App| s.open_workspace_dialog(None));
    let body = if data.workspaces.is_empty() {
        empty_block(t)
    } else {
        webkit::panel(data.workspaces.iter().map(|w| workspace(s, t, layer, data, &w.id)).collect()).gap(2.0)
    };
    super::Page { count: None, trailing: vec![add], body, title: None, back: None }
}

/// `EmptyBlock`: the title, what a workspace is, and the button.
fn empty_block(t: &Neko) -> El {
    col()
        .key("ws-none")
        .w_full()
        .px(24.0)
        .py(48.0)
        .gap(12.0)
        .items_center()
        .child(text("No workspaces yet").font(font(t, 14.0, 500.0, 20.0)).color(t.text.base))
        .child(text("A workspace is a name plus the folders where the work lives. Add one to start tasks, watches and schedules.").font(font(t, 14.0, 400.0, 20.0)).color(t.text.muted).wrap().max_w(384.0).min_w(0.0))
        .child(crate::view::page_button("Add workspace", "ws-none-action", t, |s: &mut App| s.open_workspace_dialog(None)))
}

/// One of a head line's two hover actions: 28 square, a 15 point icon, over the folder.
fn row_action(t: &Neko, key: String, label: &str, glyph: &'static rune::Icon, enabled: bool, on_press: impl Fn(&mut App) + 'static) -> El {
    let r = row().key(key).role("button").label(label.to_owned()).help(label.to_owned()).focusable(true).size(28.0, 28.0).center().radius(10.0).hover(t.text.base.alpha(0.06)).focus_ring(2.0, 2.0, t.kit.c.border_focus).group_opacity(0.0, 1.0).on_press(on_press);
    r.child(icon(glyph, 15.0).color(t.text.muted.alpha(if enabled { 1.0 } else { 0.5 })).a11y_hidden())
}

/// `Said`: a status in words with its 12 point glyph, 6 apart.
fn said(t: &Neko, key: &str, kind: StatusKind, parts: &[String]) -> El {
    row().key(key.to_owned()).gap(6.0).center_y().child(status_glyph(kind, t, 12.0, Some("")).a11y_hidden()).child(nodes(t, parts, t.text.muted, 1.0))
}

/// Words the web's JSX makes of several text nodes (`{n} need{s} you` is "2", " need", " you"): one line, each node its own text.
fn nodes(t: &Neko, parts: &[String], ink: rune::Color, alpha: f32) -> El {
    row().children(parts.iter().map(|p| text(p.clone()).font(font(t, 12.5, 400.0, 20.3125)).color(ink.alpha(alpha)).a11y_hidden()))
}

fn quiet_text(t: &Neko, words: &str) -> El {
    text(words.to_owned()).font(font(t, 12.5, 400.0, 20.3125)).color(t.text.muted).a11y_hidden()
}

/// `{n} need{s} you`: the web's text nodes.
fn need_parts(n: usize) -> Vec<String> {
    let mut parts = vec![n.to_string(), " need".to_owned()];
    if n == 1 {
        parts.push("s".to_owned());
    }
    parts.push(" you".to_owned());
    parts
}

fn workspace(s: &App, t: &Neko, layer: &mut Vec<El>, data: &SetupData, id: &str) -> El {
    let Some(w) = data.workspace(id) else { return col() };
    let folders = data.folders_of(w);
    let open = s.workspace_open(id);
    let routines = s.pages.routines.as_ref();
    let views: Vec<&RoutineView> = routines.map(|r| r.views.iter().filter(|v| v.routine.workspace_id == id && !is_goal_routine(v)).collect()).unwrap_or_default();
    let sum = routine_summary(&views, s.now_ms);
    let watches: Vec<_> = data.watches.iter().filter(|x| x.workspace_id == id).collect();
    let active = watches.iter().filter(|x| x.enabled).count();
    let open_tasks = data.open_tasks.get(id).copied().unwrap_or(0);

    let toggle_id = id.to_owned();
    let mut left = row().grow(1.0).shrink().min_w(0.0).gap(8.0).center_y().child(icon(if open { &icons::CHEVRON_DOWN } else { &icons::CHEVRON_RIGHT }, 11.0).color(t.text.muted.alpha(0.7)).a11y_hidden());
    left = left.child(text(w.name.clone()).font(font(t, 13.5, 400.0, 21.9375)).color(t.text.base).truncate().a11y_hidden());
    if open_tasks > 0 {
        left = left.child(nodes(t, &[open_tasks.to_string(), " open".to_owned()], t.text.muted, 0.7).key(format!("ws-open-count-{id}")));
    }
    if sum.count > 0 {
        left = left.child(text(format!("{} of {} {} on", sum.enabled, sum.count, if sum.count == 1 { "routine" } else { "routines" })).key(format!("ws-routines-{id}")).font(font(t, 12.5, 400.0, 20.3125)).color(t.text.muted).truncate().min_w(0.0).shrink().a11y_hidden());
    }
    let mut right = row().gap(12.0).center_y();
    if sum.needs_you > 0 {
        let label = format!("{} {} you in {}: open Routines", sum.needs_you, if sum.needs_you == 1 { "needs" } else { "need" }, w.name);
        right = right.child(row().key(format!("ws-needs-link-{id}")).role("button").label(label).focusable(true).radius(10.0).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(|s: &mut App| s.open_view(FullView::Routines)).child(said(t, &format!("ws-needs-{id}"), StatusKind::InProgress, &need_parts(sum.needs_you))));
    } else if !sum.failing.is_empty() {
        right = right.child(said(t, &format!("ws-failed-{id}"), StatusKind::Failed, &[if sum.failing.len() == 1 { format!("{} failed", sum.failing[0]) } else { format!("{} failed", sum.failing.len()) }]));
    }
    // The folder ends at the row's right edge; on hover or focus the actions take its place.
    let (sync_id, edit_id) = (id.to_owned(), id.to_owned());
    let mut at_end = row().min_w(60.0).min_h(28.0).center_y().justify_end();
    if let Some(first) = folders.first() {
        let where_ = format!("{}{}", folder_name(first), if folders.len() > 1 { format!(" +{}", folders.len() - 1) } else { String::new() });
        at_end = at_end.child(text(where_).key(format!("ws-folder-{id}")).font(mono(t, 12.0, 19.5)).color(t.text.muted.alpha(0.8)).truncate().max_w(160.0).help(folders.join("\n")).group_opacity(1.0, 0.0).a11y_hidden());
    }
    let actions = row().pin(None, Some(0.0), Some(0.0), Some(0.0)).gap(2.0).center_y().child(row_action(t, format!("ws-sync-{id}"), "Sync now", &icons::REFRESH, active > 0, move |s: &mut App| s.sync_workspace(&sync_id))).child(row_action(t, format!("ws-edit-{id}"), &format!("Edit {}", w.name), &icons::PENCIL, true, move |s: &mut App| s.open_workspace_dialog(Some(&edit_id))));
    right = right.child(at_end.child(actions));

    let line = row()
        .key(format!("ws-{id}"))
        .role("button")
        .label(format!("{}, {} {}, {} {}", w.name, folders.len(), if folders.len() == 1 { "folder" } else { "folders" }, watches.len(), if watches.len() == 1 { "watch" } else { "watches" }))
        .expanded(open)
        .focusable(true)
        .min_h(40.0)
        .px(12.0)
        .gap(12.0)
        .center_y()
        .radius(10.0)
        .hover_group()
        .hover(t.text.base.alpha(0.03))
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.toggle_workspace(&toggle_id))
        .child(left)
        .child(right);

    // Everything the row does is also in its right-click menu.
    let mut entries: Vec<MenuEntry> = Vec::new();
    let rc = |f: fn(&mut App)| -> std::rc::Rc<dyn Fn(&mut App)> { std::rc::Rc::new(f) };
    let t_id = id.to_owned();
    entries.push((if open { "Collapse" } else { "Expand" }.to_owned(), std::rc::Rc::new(move |s: &mut App| s.toggle_workspace(&t_id))));
    let e_id = id.to_owned();
    entries.push(("Edit…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_workspace_dialog(Some(&e_id)))));
    let y_id = id.to_owned();
    entries.push(("Sync now".to_owned(), std::rc::Rc::new(move |s: &mut App| s.sync_workspace(&y_id))));
    entries.push(("Open Routines".to_owned(), rc(|s| s.open_view(FullView::Routines))));
    entries.push(("Open Watching".to_owned(), rc(|s| s.open_setup(SetupSection::Watching))));
    entries.push(("Open Tools".to_owned(), rc(|s| s.open_setup(SetupSection::Tools))));
    for f in folders.iter().take(4) {
        let path = f.clone();
        entries.push((format!("Open folder: {}", folder_name(f)), std::rc::Rc::new(move |s: &mut App| s.open_path(&path))));
    }
    let mut out = col().key(format!("ws-block-{id}")).w_full().child(with_menu(s, t, layer, &format!("ws:{id}"), line, entries));
    if let Some(n) = note(s, t, &format!("setup:{id}")) {
        out = out.child(n);
    }
    // A watch that refused to check shows why here, where Sync now was pressed.
    for x in &watches {
        if let Some(n) = note(s, t, &format!("setup:{}", x.id)) {
            out = out.child(n);
        }
    }
    if open {
        out = out.child(details(s, t, data, w, &folders, &views, &sum, active, &watches));
    }
    out
}

/// A quiet link in a line's meta (`MetaLink`): 12.5 muted words and an arrow, 4 apart.
fn meta_link(t: &Neko, key: &str, aria: &str, words: Option<&str>, arrow: bool, on_press: impl Fn(&mut App) + 'static) -> El {
    let mut r = row().key(key.to_owned()).role("button").label(aria.to_owned()).focusable(true).gap(4.0).center_y().radius(6.0).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(on_press);
    if let Some(w) = words {
        r = r.child(quiet_text(t, w));
    }
    if arrow {
        r = r.child(icon(&icons::ARROW_RIGHT, 12.0).color(t.text.muted).a11y_hidden());
    }
    r
}

/// One detail line (`Line`): a label 128 wide, its value in muted words that give way, meta at the right; with `more`, the head is a
/// button that opens it under the line, indented past the label.
struct Line<'a> {
    key: String,
    /// `<workspace id>/<label>`: what the open state is kept under.
    state: String,
    label: &'a str,
    value: El,
    meta: Vec<El>,
    more: Option<Box<dyn FnOnce() -> El + 'a>>,
}

fn line(s: &App, t: &Neko, l: Line) -> El {
    let open = l.more.is_some() && s.pages.ws_lines.contains(&l.state);
    let caret = if l.more.is_some() { icon(if open { &icons::CHEVRON_DOWN } else { &icons::CHEVRON_RIGHT }, 11.0).color(t.text.muted.alpha(0.7)).a11y_hidden() } else { el().size(11.0, 11.0) };
    let mut head = row().key(format!("{}-head", l.key)).grow(1.0).shrink().min_w(0.0).gap(8.0).center_y().child(caret).child(text(l.label.to_owned()).font(font(t, 13.5, 400.0, 21.9375)).color(t.text.base).truncate().w(128.0).a11y_hidden()).child(l.value);
    if l.more.is_some() {
        let state = l.state.clone();
        head = head.role("button").label(format!("{}: {} details", l.label, if open { "hide" } else { "show" })).expanded(open).focusable(true).radius(6.0).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(move |s: &mut App| s.toggle_ws_line(&state));
    }
    let mut inner = row().w_full().min_h(28.0).gap(12.0).center_y().child(head);
    if !l.meta.is_empty() {
        inner = inner.child(row().gap(12.0).center_y().children(l.meta));
    }
    let mut r = col().key(l.key).w_full().px(12.0).py(6.0).radius(10.0).hover(t.text.base.alpha(0.03)).child(inner);
    if open && let Some(more) = l.more {
        r = r.child(col().w_full().mt(6.0).mb(4.0).gap(8.0).pl(INDENT).child(more()));
    }
    r
}

/// The value of a line: 12.5 muted, cut off with an ellipsis.
fn value(t: &Neko, key: &str, words: &str) -> El {
    text(words.replace('\n', " ")).key(key.to_owned()).font(font(t, 12.5, 400.0, 20.3125)).color(t.text.muted).truncate().min_w(0.0).shrink().grow(1.0).a11y_hidden()
}

/// `ShortList`: the first five, and a toggle for the rest.
fn short_list(s: &App, t: &Neko, state: &str, noun: &str, items: Vec<El>, container: El) -> El {
    let all = s.pages.ws_lines.contains(state);
    let total = items.len();
    let shown: Vec<El> = if all { items } else { items.into_iter().take(SHORT).collect() };
    let mut out = col().gap(4.0).child(container.children(shown));
    if total > SHORT {
        let label = if all { "Show fewer".to_owned() } else { format!("Show {} more {noun}", total - SHORT) };
        let key = state.to_owned();
        out = out.child(row().key(format!("{state}-more")).role("button").label(label.clone()).expanded(all).focusable(true).radius(6.0).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(move |s: &mut App| s.toggle_ws_line(&key)).child(quiet_text(t, &label)));
    }
    out
}

/// The words of a paragraph a line opens to: 13 point, 70ch wide at most, newlines kept.
fn paragraph(t: &Neko, words: &str) -> El {
    text(words.to_owned()).font(font(t, 13.0, 400.0, 21.45)).color(t.text.base.alpha(0.8)).wrap().min_w(0.0).max_w(565.0)
}

fn bullet(t: &Neko, words: &str) -> El {
    row().gap(8.0).child(el().size(4.0, 4.0).radius(2.0).mt(9.0).fill(t.text.muted).a11y_hidden()).child(text(words.to_owned()).font(font(t, 13.0, 400.0, 21.45)).color(t.text.muted).wrap().min_w(0.0))
}

#[allow(clippy::too_many_arguments)]
fn details(s: &App, t: &Neko, data: &SetupData, w: &neko_protocol::workbench::Workspace, folders: &[String], views: &[&RoutineView], sum: &Summary, active: usize, watches: &[&neko_protocol::mcp_host::Responsibility]) -> El {
    let id = w.id.as_str();
    let mut c = col().key(format!("ws-details-{id}")).w_full().pl(19.0).pb(8.0);
    let scope = s.pages.routines.as_ref().and_then(|r| r.scopes.iter().find(|sc| sc.workspace_id.as_deref() == Some(id)));
    let state = |label: &str| format!("{id}/{label}");
    let orders = s.pages.routines.as_ref().and_then(|r| r.standing_orders.get(id)).map(|o| {
        let owner = s.pages.routines.as_ref().map_or("the owner", |r| if r.owner_name.trim().is_empty() { "the owner" } else { r.owner_name.as_str() });
        o.replace("{owner}", owner)
    });
    let orders = orders.filter(|o| !o.trim().is_empty());

    // Routines.
    if sum.count > 0 {
        let key = format!("ws-line-routines-{id}");
        c = c.child(line(
            s,
            t,
            Line {
                key: key.clone(),
                state: state("Routines"),
                label: "Routines",
                value: value(t, &format!("{key}-value"), &sum.text),
                meta: vec![meta_link(t, &format!("{key}-go"), &format!("Open Routines for {}", w.name), Some("Inbox"), true, |s: &mut App| s.open_view(FullView::Routines))],
                more: Some(Box::new(|| {
                    let mut list = row().key(format!("ws-routine-summary-{id}")).w_full().gaps(6.0, 16.0).flex_wrap();
                    for v in views {
                        let (kind, word) = routine_status(v, s.now_ms);
                        let last = v.runs.first();
                        let total = last.map_or(0, |r| r.needs_you.len());
                        let done = last.map_or(0, |r| r.needs_you.iter().filter(|i| i.done).count());
                        let mut li = row().key(format!("ws-routine-{}", v.routine.id)).gap(6.0).center_y().child(status_glyph(kind, t, 14.0, Some(&word))).child(text(v.routine.name.clone()).font(font(t, 13.0, 400.0, 20.0)).color(t.text.base.alpha(0.8)));
                        if total > 0 {
                            li = li.child(row().gap(4.0).center_y().child(crate::view::sessions::progress_ring(Some((done as u32, total as u32)), t)).child(text(format!("{done}/{total}")).font(font(t, 12.0, 400.0, 16.0)).color(t.text.muted)));
                        }
                        list = list.child(li);
                    }
                    list
                })),
            },
        ));
    } else if s.pages.routines.is_none() {
        c = c.child(line(s, t, Line { key: format!("ws-line-routines-{id}"), state: state("Routines"), label: "Routines", value: value(t, &format!("ws-line-routines-{id}-value"), "Loading routines…"), meta: Vec::new(), more: None }));
    }
    // Setup notes.
    if let Some(sc) = scope.filter(|sc| !sc.problems.is_empty()) {
        let n = sc.problems.len();
        let words = format!("{n} {} to tidy in the MCP config on disk; routines still run", if n == 1 { "thing" } else { "things" });
        let notes = sc.problems.clone();
        c = c.child(line(
            s,
            t,
            Line {
                key: format!("ws-problems-{id}"),
                state: state("Setup notes"),
                label: "Setup notes",
                value: text(words).key(format!("ws-problems-{id}-value")).font(font(t, 12.5, 400.0, 20.3125)).color(t.meaning.amber).truncate().min_w(0.0).shrink().grow(1.0).a11y_hidden(),
                meta: Vec::new(),
                more: Some(Box::new(move || short_list(s, t, &format!("{id}/Setup notes/all"), "notes", notes.iter().map(|p| bullet(t, p)).collect(), col().gap(4.0)))),
            },
        ));
    }
    // Folders.
    if !folders.is_empty() {
        let joined = folders.join(", ");
        let key = format!("ws-line-folders-{id}");
        let first = folders[0].clone();
        let more: Option<Box<dyn FnOnce() -> El + '_>> = (folders.len() > 1).then(|| -> Box<dyn FnOnce() -> El + '_> {
            Box::new(move || {
                let items: Vec<El> = folders
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        let path = f.clone();
                        text(f.clone()).key(format!("ws-folder-row-{id}-{i}")).font(mono(t, 12.0, 19.5)).color(t.text.base.alpha(0.8)).truncate().min_w(0.0).help(f.clone()).on_secondary_press(move |s: &mut App, _, _| s.open_path(&path))
                    })
                    .collect();
                short_list(s, t, &format!("{id}/Folders/all"), "folders", items, col().gap(4.0))
            })
        });
        let meta = if folders.len() > 1 { vec![quiet_text(t, &folders.len().to_string()).key(format!("{key}-count"))] } else { Vec::new() };
        let _ = first;
        c = c.child(line(
            s,
            t,
            Line {
                key: key.clone(),
                state: state("Folders"),
                label: "Folders",
                value: text(joined.clone()).key(format!("{key}-value")).font(mono(t, 12.0, 19.5)).color(t.text.muted).truncate().min_w(0.0).shrink().grow(1.0).help(folders.join("\n")).a11y_hidden(),
                meta,
                more,
            },
        ));
    }
    // Connections: the MCP servers the folders configure on disk, the whole scope's first, then each repository's own.
    if let Some(sc) = scope.filter(|sc| !sc.servers.is_empty()) {
        let mut names: Vec<&str> = sc.servers.iter().map(|x| x.name.as_str()).collect();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        let key = format!("ws-line-servers-{id}");
        let count = names.len();
        let value_words = names.join(", ");
        c = c.child(line(
            s,
            t,
            Line {
                key: key.clone(),
                state: state("Connections"),
                label: "Connections",
                value: value(t, &format!("{key}-value"), &value_words),
                meta: vec![quiet_text(t, &count.to_string())],
                more: Some(Box::new(move || {
                    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
                    let whole: Vec<String> = sc.servers.iter().filter(|x| x.level != "repo").map(|x| x.name.clone()).collect();
                    if !whole.is_empty() {
                        groups.push(("Whole scope".to_owned(), whole));
                    }
                    let mut repos: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
                    for x in sc.servers.iter().filter(|x| x.level == "repo") {
                        let repo = x.repo.as_deref().map(folder_name).filter(|r| !r.is_empty()).unwrap_or_else(|| "one repository".to_owned());
                        repos.entry(repo).or_default().push(x.name.clone());
                    }
                    groups.extend(repos.into_iter().map(|(r, n)| (format!("Only in {r}"), n)));
                    let items: Vec<El> = groups
                        .into_iter()
                        .map(|(label, mut names)| {
                            names.sort_by_key(|n| n.to_lowercase());
                            names.dedup();
                            let list = short_list(s, t, &format!("{id}/Connections/{label}"), "servers", names.iter().map(|n| text(n.clone()).font(mono(t, 12.0, 19.5)).color(t.text.base.alpha(0.8))).collect(), row().gaps(4.0, 12.0).flex_wrap());
                            row().key(format!("ws-servers-{id}-{label}")).gap(12.0).child(text(label.clone()).font(font(t, 12.5, 400.0, 20.3125)).color(t.text.muted.alpha(0.8)).truncate().w(112.0)).child(list)
                        })
                        .collect();
                    short_list(s, t, &format!("{id}/Connections/all"), "repositories", items, col().gap(6.0))
                })),
            },
        ));
    }
    // Watching.
    if !watches.is_empty() {
        let key = format!("ws-line-watching-{id}");
        let failing = watches.iter().any(|x| x.failures > 0);
        let last_checked = watches.iter().filter_map(|x| x.last_attempt_ms).max().unwrap_or(0);
        let state_el = if failing { text("Needs attention".to_owned()).font(font(t, 12.5, 400.0, 20.3125)).color(t.meaning.amber).a11y_hidden() } else { quiet_text(t, &if last_checked > 0 { format!("Synced {}", relative_time(s.now_ms, last_checked)) } else { "Not checked yet".to_owned() }) };
        let words = format!("{active} active{}", if watches.len() > active { format!(" · {} paused or suggested", watches.len() - active) } else { String::new() });
        c = c.child(line(
            s,
            t,
            Line {
                key: key.clone(),
                state: state("Watching"),
                label: "Watching",
                value: value(t, &format!("{key}-value"), &words),
                meta: vec![state_el, meta_link(t, &format!("{key}-go"), &format!("Open Watching for {}", w.name), None, true, |s: &mut App| s.open_setup(SetupSection::Watching))],
                more: None,
            },
        ));
    }
    // Tools.
    let tools: Vec<_> = data.connections.iter().filter(|x| x.workspace_id.is_empty() || x.workspace_id == id).collect();
    if !tools.is_empty() {
        let key = format!("ws-line-tools-{id}");
        let broken = tools.iter().filter(|x| x.enabled && x.error.as_deref().is_some_and(|e| !e.is_empty())).count();
        let schedules = data.schedules.iter().filter(|x| x.workspace_id.as_deref() == Some(id)).count();
        let names = tools.iter().map(|x| x.label.clone()).collect::<Vec<_>>().join(", ");
        let mut v = row().key(format!("{key}-value")).grow(1.0).shrink().min_w(0.0).center_y();
        if broken > 0 {
            let mut parts = vec![broken.to_string(), " need".to_owned()];
            if broken == 1 {
                parts.push("s".to_owned());
            }
            parts.push(" attention · ".to_owned());
            v = v.child(nodes(t, &parts, t.meaning.amber, 1.0));
        }
        v = v.child(text(names).font(font(t, 12.5, 400.0, 20.3125)).color(t.text.muted).truncate().min_w(0.0).shrink().a11y_hidden());
        let mut meta = Vec::new();
        if schedules > 0 {
            meta.push(quiet_text(t, &format!("{schedules} {}", if schedules == 1 { "schedule" } else { "schedules" })));
        }
        meta.push(meta_link(t, &format!("{key}-go"), &format!("Open Tools & skills for {}", w.name), None, true, |s: &mut App| s.open_setup(SetupSection::Tools)));
        let list = tools.clone();
        c = c.child(line(
            s,
            t,
            Line {
                key,
                state: state("Tools in Neko"),
                label: "Tools in Neko",
                value: v,
                meta,
                more: Some(Box::new(move || {
                    let items: Vec<El> = list.iter().map(|x| text(x.label.clone()).font(font(t, 12.5, 400.0, 20.3125)).color(if x.enabled && x.error.as_deref().is_some_and(|e| !e.is_empty()) { t.meaning.amber } else { t.text.base.alpha(0.8) })).collect();
                    short_list(s, t, &format!("{id}/Tools in Neko/all"), "tools", items, row().gaps(4.0, 12.0).flex_wrap())
                })),
            },
        ));
    }
    // Standing orders matter only where routines run.
    if sum.count > 0
        && let Some(orders) = orders.clone()
    {
        let key = format!("ws-line-orders-{id}");
        c = c.child(line(
            s,
            t,
            Line {
                key: key.clone(),
                state: state("Standing orders"),
                label: "Standing orders",
                value: value(t, &format!("{key}-value"), &orders),
                meta: vec![meta_link(t, &format!("{key}-go"), &format!("Edit standing orders for {} in Routines", w.name), Some("Edit in Inbox"), false, |s: &mut App| s.open_view(FullView::Routines))],
                more: Some(Box::new(move || paragraph(t, &orders))),
            },
        ));
    }
    if !w.instructions.is_empty() {
        let key = format!("ws-line-instructions-{id}");
        let first = w.instructions.split('\n').next().unwrap_or("").to_owned();
        let all = w.instructions.clone();
        c = c.child(line(s, t, Line { key: key.clone(), state: state("Instructions"), label: "Instructions", value: value(t, &format!("{key}-value"), &first), meta: Vec::new(), more: Some(Box::new(move || paragraph(t, &all))) }));
    }
    // Empty facets you can fill, offered as quiet links.
    type Offer<'a> = (&'a str, &'a str, Box<dyn Fn(&mut App)>);
    let mut offers: Vec<Offer> = Vec::new();
    if folders.is_empty() {
        let e = id.to_owned();
        offers.push(("folder", "Add a folder", Box::new(move |s: &mut App| s.open_workspace_dialog(Some(&e)))));
    }
    if s.pages.routines.is_some() && sum.count == 0 {
        offers.push(("routine", "Add a routine", Box::new(|s: &mut App| s.open_view(FullView::Routines))));
    }
    if watches.is_empty() {
        offers.push(("watch", "Add a watch", Box::new(|s: &mut App| s.open_setup(SetupSection::Watching))));
    }
    if tools.is_empty() {
        offers.push(("tool", "Connect a tool", Box::new(|s: &mut App| s.open_setup(SetupSection::Tools))));
    }
    if sum.count > 0 && orders.is_none() {
        offers.push(("orders", "Write standing orders", Box::new(|s: &mut App| s.open_view(FullView::Routines))));
    }
    if !offers.is_empty() {
        let mut r = row().key(format!("ws-offers-{id}")).w_full().min_h(32.0).pl(31.0).pr(12.0).gaps(4.0, 16.0).flex_wrap().center_y();
        for (key, label, press) in offers {
            r = r.child(row().key(format!("ws-offer-{key}-{id}")).role("button").label(label).focusable(true).gap(4.0).center_y().radius(6.0).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(press).child(icon(&icons::PLUS, 12.0).color(t.text.muted).a11y_hidden()).child(quiet_text(t, label)));
        }
        c = c.child(r);
    }
    c
}
