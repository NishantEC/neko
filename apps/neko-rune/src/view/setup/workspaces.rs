//! Setup, Workspaces: the full editor. One line per workspace (name, how many of its routines are on, what
//! needs you, its folder) that opens to its details as indented rows: routines, notes on the MCP config,
//! folders, connections, watches, tools, standing orders, instructions. Add a workspace; edit its name,
//! folders and instructions; Sync now wakes its active watches. Ported from `pages/setup/Workspaces.tsx`.

use neko_protocol::routines::RoutineView;
use rune::{El, icon, row, text, col};

use super::kit::{self, action, details_block, empty, list_row};
use super::Page;
use super::state::SetupSection;
use super::words;
use crate::app::App;
use crate::icons;
use crate::link::SetupData;
use crate::pages::is_goal_routine;
use crate::theme::Neko;
use crate::view::ui::{MenuEntry, note, with_menu};
use crate::views::FullView;

fn folder_name(path: &str) -> String {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path).to_owned()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// What a workspace's routines add up to: how many are on, what waits on the owner, what failed.
struct Summary {
    count: usize,
    enabled: usize,
    needs_you: usize,
    failing: Vec<String>,
}

fn routine_summary(views: &[&RoutineView]) -> Summary {
    Summary {
        count: views.len(),
        enabled: views.iter().filter(|v| v.routine.enabled).count(),
        needs_you: views.iter().map(|v| v.runs.iter().map(|r| r.needs_you.iter().filter(|i| !i.done).count()).sum::<usize>()).sum(),
        failing: views.iter().filter(|v| v.runs.first().is_some_and(|r| !r.ok)).map(|v| v.routine.name.clone()).collect(),
    }
}

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>) -> Page {
    let Some(data) = s.setup_data() else { return Page::quiet(t, "ws-none", "Looking for your workspaces…") };
    let add = crate::view::page_button("Add workspace", "ws-add", t, |s: &mut App| s.open_workspace_dialog(None));
    let mut body = col().w_full().gap(2.0);
    if data.workspaces.is_empty() {
        body = body.child(empty(t, "ws-none", "A workspace is a name plus the folders where the work lives.", Some(("Add workspace", Box::new(|s: &mut App| s.open_workspace_dialog(None))))));
    }
    for w in &data.workspaces {
        body = body.child(workspace(s, t, layer, data, &w.id));
    }
    Page { count: (!data.workspaces.is_empty()).then_some(data.workspaces.len()), trailing: vec![add], body, title: None, back: None }
}

fn workspace(s: &App, t: &Neko, layer: &mut Vec<El>, data: &SetupData, id: &str) -> El {
    let Some(w) = data.workspace(id) else { return col() };
    let folders = data.folders_of(w);
    let open = s.pages.ws_open.contains(id);
    let routines = s.pages.routines.as_ref();
    let views: Vec<&RoutineView> = routines.map(|r| r.views.iter().filter(|v| v.routine.workspace_id == id && !is_goal_routine(v)).collect()).unwrap_or_default();
    let sum = routine_summary(&views);
    let watches: Vec<_> = data.watches.iter().filter(|x| x.workspace_id == id).collect();
    let active = watches.iter().filter(|x| x.enabled).count();

    let toggle_id = id.to_owned();
    let mut line = list_row(t, format!("ws-{id}"), format!("{}, {}, {}", w.name, plural(folders.len(), "folder", "folders"), plural(watches.len(), "watch", "watches")), Some(open))
        .on_press(move |s: &mut App| s.toggle_workspace(&toggle_id))
        .child(icon(if open { &icons::CHEVRON_DOWN } else { &icons::CHEVRON_RIGHT }, 16.0).color(t.text.faint).a11y_hidden())
        .child(text(w.name.clone()).font(t.ty.row).color(t.text.base).truncate().min_w(0.0).a11y_hidden());
    if sum.count > 0 {
        line = line.child(text(format!("{} of {} {} on", sum.enabled, sum.count, if sum.count == 1 { "routine" } else { "routines" })).key(format!("ws-routines-{id}")).font(t.ty.meta).color(t.text.muted).truncate().min_w(0.0).a11y_hidden());
    }
    line = line.child(kit::fill());
    if sum.needs_you > 0 {
        line = line.child(text(format!("{} {} you", sum.needs_you, if sum.needs_you == 1 { "needs" } else { "need" })).key(format!("ws-needs-{id}")).font(t.ty.meta).color(t.meaning.amber).a11y_hidden());
    } else if !sum.failing.is_empty() {
        line = line.child(text(if sum.failing.len() == 1 { format!("{} failed", sum.failing[0]) } else { format!("{} failed", sum.failing.len()) }).font(t.ty.meta).color(t.meaning.coral).a11y_hidden());
    }
    if let Some(first) = folders.first() {
        let where_ = format!("{}{}", folder_name(first), if folders.len() > 1 { format!(" +{}", folders.len() - 1) } else { String::new() });
        line = line.child(text(where_).key(format!("ws-folder-{id}")).font(t.ty.mono_meta).color(t.text.faint).truncate().min_w(0.0).max_w(160.0).help(folders.join("\n")).a11y_hidden());
    }
    let (sync_id, edit_id) = (id.to_owned(), id.to_owned());
    line = line
        .child(action(t, format!("ws-sync-{id}"), "Sync now", &icons::REFRESH, move |s: &mut App| s.sync_workspace(&sync_id)))
        .child(action(t, format!("ws-edit-{id}"), &format!("Edit {}", w.name), &icons::PENCIL, move |s: &mut App| s.open_workspace_dialog(Some(&edit_id))));

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

/// A details line with a quiet link at its end.
fn link_row(t: &Neko, key: &str, label: &str, value: &str, go: &str, on_press: impl Fn(&mut App) + 'static) -> El {
    kit::detail(t, key, label, value, false).child(crate::view::quiet_button(go, &format!("{key}-go"), t, on_press))
}

#[allow(clippy::too_many_arguments)]
fn details(s: &App, t: &Neko, data: &SetupData, w: &neko_protocol::workbench::Workspace, folders: &[String], views: &[&RoutineView], sum: &Summary, active: usize, watches: &[&neko_protocol::mcp_host::Responsibility]) -> El {
    let id = w.id.as_str();
    let mut c = details_block(format!("ws-details-{id}"));
    let scope = s.pages.routines.as_ref().and_then(|r| r.scopes.iter().find(|sc| sc.workspace_id.as_deref() == Some(id)));
    // Routines.
    if sum.count > 0 {
        let mut parts = vec![format!("{} of {} on", sum.enabled, sum.count)];
        if let Some(running) = views.iter().find(|v| v.state.running) {
            parts.push(format!("{} is running now", running.routine.name));
        } else if let Some(last) = views.iter().filter_map(|v| v.runs.first().map(|r| (r.finished_ms.unwrap_or(r.started_ms), &v.routine.name, r))).max_by_key(|(at, _, _)| *at) {
            parts.push(format!("last: {} {} {}", last.1, if !last.2.ok { "failed" } else if last.2.did.is_empty() { "was quiet" } else { "acted" }, words::relative_time(Some(last.0), s.now_ms)));
        }
        c = c.child(link_row(t, &format!("ws-line-routines-{id}"), "Routines", &parts.join(" · "), "Open Routines", |s: &mut App| s.open_view(FullView::Routines)));
    } else if s.pages.routines.is_none() {
        c = c.child(kit::detail(t, &format!("ws-line-routines-{id}"), "Routines", "Loading routines…", false));
    }
    if let Some(sc) = scope
        && !sc.problems.is_empty()
    {
        c = c.child(kit::amber(t, &format!("ws-problems-{id}"), &format!("{} to tidy in the MCP config on disk; routines still run", plural(sc.problems.len(), "thing", "things"))));
    }
    // Folders, one row each.
    for (i, f) in folders.iter().enumerate() {
        let path = f.clone();
        c = c.child(kit::detail(t, &format!("ws-folder-row-{id}-{i}"), if i == 0 { "Folders" } else { "" }, f, true).on_secondary_press(move |s: &mut App, _, _| s.open_path(&path)));
    }
    // The MCP servers the folders configure on disk.
    if let Some(sc) = scope
        && !sc.servers.is_empty()
    {
        let mut names: Vec<&str> = sc.servers.iter().map(|x| x.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        c = c.child(kit::detail(t, &format!("ws-line-servers-{id}"), "Connections", &names.join(", "), true));
    }
    // Watches.
    if !watches.is_empty() {
        let value = format!("{active} active{}", if watches.len() > active { format!(" · {} paused or suggested", watches.len() - active) } else { String::new() });
        c = c.child(link_row(t, &format!("ws-line-watching-{id}"), "Watching", &value, "Open Watching", |s: &mut App| s.open_setup(SetupSection::Watching)));
    }
    // Tools.
    let tools: Vec<_> = data.connections.iter().filter(|x| x.workspace_id.is_empty() || x.workspace_id == id).collect();
    if !tools.is_empty() {
        let broken = tools.iter().filter(|x| x.enabled && x.error.is_some()).count();
        let schedules = data.schedules.iter().filter(|x| x.workspace_id.as_deref() == Some(id)).count();
        let mut value = tools.iter().map(|x| x.label.clone()).collect::<Vec<_>>().join(", ");
        if broken > 0 {
            value = format!("{broken} {} attention · {value}", if broken == 1 { "needs" } else { "need" });
        }
        if schedules > 0 {
            value = format!("{value} · {}", plural(schedules, "schedule", "schedules"));
        }
        c = c.child(link_row(t, &format!("ws-line-tools-{id}"), "Tools in Neko", &value, "Open Tools", |s: &mut App| s.open_setup(SetupSection::Tools)));
    }
    // Standing orders are edited on the Routines page.
    if let Some(orders) = s.pages.routines.as_ref().and_then(|r| r.standing_orders.get(id)).filter(|o| !o.trim().is_empty()).filter(|_| sum.count > 0) {
        let owner = s.pages.routines.as_ref().map_or("the owner", |r| if r.owner_name.trim().is_empty() { "the owner" } else { r.owner_name.as_str() });
        c = c.child(link_row(t, &format!("ws-line-orders-{id}"), "Standing orders", &orders.replace("{owner}", owner), "Edit in Routines", |s: &mut App| s.open_view(FullView::Routines)));
    }
    if !w.instructions.trim().is_empty() {
        c = c.child(kit::detail(t, &format!("ws-line-instructions-{id}"), "Instructions", &w.instructions, false));
    }
    // Empty facets you can fill, offered as actions.
    let mut offers = row().key(format!("ws-offers-{id}")).w_full().gap(8.0).pt(4.0).px(8.0);
    let mut any = false;
    let mut offer = |label: &str, key: &str, f: Box<dyn Fn(&mut App)>| {
        offers = std::mem::replace(&mut offers, row()).child(crate::view::quiet_button(label, &format!("ws-offer-{key}-{id}"), t, f));
        any = true;
    };
    if folders.is_empty() {
        let e = id.to_owned();
        offer("Add a folder", "folder", Box::new(move |s: &mut App| s.open_workspace_dialog(Some(&e))));
    }
    if watches.is_empty() {
        offer("Add a watch", "watch", Box::new(|s: &mut App| s.open_setup(SetupSection::Watching)));
    }
    if tools.is_empty() {
        offer("Connect a tool", "tool", Box::new(|s: &mut App| s.open_setup(SetupSection::Tools)));
    }
    if sum.count == 0 && s.pages.routines.is_some() {
        offer("Add a routine", "routine", Box::new(|s: &mut App| s.open_view(FullView::Routines)));
    }
    if any {
        c = c.child(offers);
    }
    c
}
