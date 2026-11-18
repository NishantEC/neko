//! The Setup pages' dialogs: a workspace, a watch (and renaming one), adding a connection (browse the
//! registry, import from this Mac, by hand), a schedule, a profile, a memory, working guidance, correcting a
//! decision, and the confirmation every destructive action goes through (a coral button). The fields are
//! the web's, in the web's order.

use rune::{El, col, icon, row, text};
use rune_kit::SegmentedControl;

use super::kit::{self, coral_button, field, footer, form_buttons, form_note, text_area, text_field};
use super::state::{AddTab, Confirm, Dialog, F_NAME, F_PATH, F_TEXT};
use super::words::{self, Recurrence};
use crate::app::App;
use crate::icons;
use crate::link::SetupData;
use crate::theme::Neko;
use crate::view::ui::{note, pick, switch};
use crate::view::{primary_button, quiet_button};
use neko_protocol::workbench::MemoryKind;

/// Mounts the open dialog, if any. The dialog's own popups (a select's list) go above it.
pub fn mount(s: &App, t: &Neko, layer: &mut Vec<El>, win_h: f32) {
    let Some(dialog) = s.setup.dialog.clone() else { return };
    let Some(data) = s.setup_data() else { return };
    let mut popups = Vec::new();
    match dialog {
        Dialog::Workspace { id, folders } => workspace(s, t, layer, win_h, id.as_deref(), &folders),
        Dialog::Watch { id, workspace, connections, enabled, prepare } => watch(s, t, &mut popups, layer, win_h, data, id.as_deref(), &workspace, &connections, enabled, prepare),
        Dialog::RenameWatch { .. } => rename(s, t, layer, win_h),
        Dialog::Connection { tab, workspace, form, selected, expanded, added } => connection(s, t, &mut popups, layer, win_h, data, tab, &workspace, &form, selected.as_ref(), expanded.as_deref(), added.as_deref()),
        Dialog::Schedule { id, workspace: _, kind, weekday } => schedule(s, t, &mut popups, layer, win_h, id.as_deref(), kind, &weekday),
        Dialog::Profile { id } => profile(s, t, layer, win_h, id.as_deref()),
        Dialog::Memory { id, kind, workspace } => memory(s, t, &mut popups, layer, win_h, data, id.as_deref(), kind, &workspace),
        Dialog::Preference { id, .. } => preference(s, t, layer, win_h, id.as_deref()),
        Dialog::Correct { .. } => correct(s, t, layer, win_h),
        Dialog::Pack { tab, preview, intent } => super::pack_dialog::pack(s, t, layer, win_h, tab, preview.as_deref(), &intent),
        Dialog::Improve { id, evidence } => super::improvements::detail(s, t, layer, win_h, &id, evidence),
        Dialog::Confirm(c) => confirm(s, t, layer, win_h, &c),
    }
    layer.extend(popups);
}

fn workspace(s: &App, t: &Neko, layer: &mut Vec<El>, win_h: f32, id: Option<&str>, folders: &[String]) {
    let pending = s.pages.text(F_PATH).trim().to_owned();
    let has_folder = !folders.is_empty() || !pending.is_empty();
    let name_ok = !s.pages.text(F_NAME).trim().is_empty();
    let mut body = col().w_full().gap(14.0);
    body = body.child(field(t, "ws-name", "Name", None, text_field(s, t, F_NAME, "Name", "acme-api", false, false, |s: &mut App| s.save_workspace())));
    let mut list = col().key("dlg-folders").w_full().gap(4.0);
    if !has_folder {
        list = list.child(text("Add at least one folder where this work lives.".to_owned()).font(t.ty.meta).color(t.text.muted));
    }
    for (i, f) in folders.iter().enumerate() {
        let path = f.clone();
        list = list.child(
            row()
                .key(format!("dlg-folder-{i}"))
                .w_full()
                .min_h(32.0)
                .px(10.0)
                .gap(8.0)
                .center_y()
                .radius(t.radius.row)
                .fill(t.surface.hover)
                .child(icon(&icons::FOLDER, 16.0).color(t.text.muted).a11y_hidden())
                .child(text(f.clone()).font(t.ty.mono_meta).color(t.text.base).truncate().grow(1.0).min_w(0.0).help(f.clone()))
                .child(kit::action(t, format!("dlg-folder-remove-{i}"), &format!("Remove {f}"), &icons::CLOSE_SMALL, move |s: &mut App| s.workspace_remove_folder(&path)).group_opacity(1.0, 1.0)),
        );
    }
    let add = row().w_full().gap(8.0).center_y().child(text_field(s, t, F_PATH, "Folder path", "/Users/you/Documents/project", true, false, |s: &mut App| s.workspace_add_folder()).grow(1.0)).child(if pending.is_empty() { quiet_button("Add folder", "dlg-folder-add", t, |_| {}).disabled(true) } else { quiet_button("Add folder", "dlg-folder-add", t, |s: &mut App| s.workspace_add_folder()) });
    body = body.child(field(t, "ws-folders", "Source folders", Some("Absolute paths on this Mac. Folders do not need to be Git repositories; the daemon checks each one when you save."), col().w_full().gap(6.0).child(list).child(add)));
    body = body.child(field(t, "ws-instructions", "Instructions · optional", None, text_area(s, t, F_TEXT, "Instructions", "How should Neko work here?", 4)));
    if let Some(n) = form_note(s, t) {
        body = body.child(n);
    }
    body = body.child(form_buttons(s, t, "Save workspace", "dlg-ws", name_ok && has_folder, |s: &mut App| s.save_workspace()));
    kit::dialog(t, layer, "dlg-workspace", if id.is_some() { "Workspace settings" } else { "Create workspace" }, Some("Give this work a name, a home, and its own instructions."), 520.0, win_h, body);
}

#[allow(clippy::too_many_arguments)]
fn watch(s: &App, t: &Neko, popups: &mut Vec<El>, layer: &mut Vec<El>, win_h: f32, data: &SetupData, id: Option<&str>, workspace: &str, connections: &[String], enabled: bool, prepare: bool) {
    let mut body = col().w_full().gap(14.0);
    if id.is_none() && data.workspaces.len() > 1 {
        let names: Vec<String> = data.workspaces.iter().map(|w| w.name.clone()).collect();
        let by: Vec<(String, String)> = data.workspaces.iter().map(|w| (w.name.clone(), w.id.clone())).collect();
        body = body.child(field(t, "watch-ws", "Workspace", None, pick(s, t, popups, "dlg-watch-workspace", "Workspace for the new watch", &names, &data.workspace_name(workspace), 240.0, move |s: &mut App, chosen: &str| {
            if let Some((_, ws)) = by.iter().find(|(n, _)| n == chosen) {
                s.watch_set_workspace(ws);
            }
        })));
    }
    body = body.child(field(t, "watch-what", "What to watch", None, text_area(s, t, F_TEXT, "What to watch", "New Linear issues assigned to me", 4)));
    let available: Vec<_> = data.connections.iter().filter(|c| c.workspace_id.is_empty() || c.workspace_id == workspace).collect();
    let mut tools = col().key("dlg-watch-tools").w_full().gap(2.0);
    for c in &available {
        let cid = c.id.clone();
        tools = tools.child(kit::check(t, &format!("dlg-watch-tool-{}", c.id), &c.label, connections.contains(&c.id), move |s: &mut App| s.watch_toggle_connection(&cid)));
    }
    let hint = available.is_empty().then_some("No tools are connected to this workspace yet. Add one in Tools.");
    body = body.child(field(t, "watch-tools", "Connected tools", hint, tools));
    let enable_help = if data.plan_first { "Remote tool permission and publication authority are separate." } else { "Tickets from this watch start working right away; the agent stops for your approval only on critical work." };
    body = body.child(
        row()
            .key("dlg-watch-enabled")
            .w_full()
            .gap(12.0)
            .px(12.0)
            .py(8.0)
            .center_y()
            .radius(t.radius.row)
            .fill(t.surface.hover)
            .child(col().grow(1.0).min_w(0.0).gap(2.0).child(text("Enabled".to_owned()).font(t.ty.row).color(t.text.base)).child(text(enable_help.to_owned()).font(t.ty.meta).color(t.text.muted).wrap()))
            .child(switch(t, "dlg-watch-enabled-switch", "Enabled", enabled, move |s: &mut App| s.watch_set_enabled(!enabled))),
    );
    let missing = if enabled { words::ungranted(data, workspace, connections) } else { Vec::new() };
    if !missing.is_empty() {
        body = body.child(kit::amber(t, "dlg-watch-missing", &format!("Choose access for {} in Tools before turning this on.", missing.join(", "))));
    }
    if data.plan_first {
        body = body.child(kit::check(t, "dlg-watch-prepare", "Allow preparation of low-risk local fixes", prepare, move |s: &mut App| s.watch_set_prepare(!prepare)));
    }
    if let Some(n) = form_note(s, t) {
        body = body.child(n);
    }
    let ok = !s.pages.text(F_TEXT).trim().is_empty() && !connections.is_empty() && missing.is_empty();
    body = body.child(form_buttons(s, t, "Save", "dlg-watch", ok, |s: &mut App| s.save_watch()));
    kit::dialog(t, layer, "dlg-watch", if id.is_some() { "Edit watch" } else { "New watch" }, Some("Choose what to watch, its sources, and the actions you allow."), 520.0, win_h, body);
}

fn rename(s: &App, t: &Neko, layer: &mut Vec<El>, win_h: f32) {
    let mut body = col().w_full().gap(14.0);
    body = body.child(field(t, "rename", "Name", Some("An empty name clears it, so Neko names it again."), text_field(s, t, F_NAME, "Name", "", false, false, |s: &mut App| s.rename_watch())));
    if let Some(id) = match &s.setup.dialog {
        Some(Dialog::RenameWatch { id }) => Some(id.clone()),
        _ => None,
    } && let Some(n) = note(s, t, &format!("setup:{id}"))
    {
        body = body.child(n);
    }
    body = body.child(form_buttons(s, t, "Rename", "dlg-rename", true, |s: &mut App| s.rename_watch()));
    kit::dialog(t, layer, "dlg-rename-watch", "Rename watch", None, 440.0, win_h, body);
}

#[allow(clippy::too_many_arguments)]
fn connection(s: &App, t: &Neko, popups: &mut Vec<El>, layer: &mut Vec<El>, win_h: f32, data: &SetupData, tab: AddTab, workspace: &str, form: &words::ConnectionForm, selected: Option<&words::RegistryServer>, expanded: Option<&str>, added: Option<&str>) {
    let tabs = ["Browse MCP servers", "Import from this Mac", "Add manually"];
    let now = [AddTab::Browse, AddTab::Import, AddTab::Manual].iter().position(|x| *x == tab);
    let switcher = SegmentedControl::new(&tabs, now).label("How to add a connection").width(None).on_pick(|s: &mut App, i: usize, _| s.connection_dialog_tab([AddTab::Browse, AddTab::Import, AddTab::Manual][i.min(2)])).build(&t.kit).key("dlg-conn-tabs");
    let mut body = col().w_full().gap(12.0).child(switcher);
    // The workspace a connection is for: "All workspaces" or one.
    let mut names = vec!["All workspaces".to_owned()];
    names.extend(data.workspaces.iter().map(|w| w.name.clone()));
    let by: Vec<(String, String)> = data.workspaces.iter().map(|w| (w.name.clone(), w.id.clone())).collect();
    let scope_pick = |popups: &mut Vec<El>, all: bool| {
        let (mut options, current) = (names.clone(), if workspace.is_empty() { "All workspaces".to_owned() } else { data.workspace_name(workspace) });
        if !all {
            options.remove(0);
        }
        let by = by.clone();
        pick(s, t, popups, "dlg-conn-workspace", "Available in", &options, &current, 240.0, move |s: &mut App, chosen: &str| s.connection_dialog_workspace(by.iter().find(|(n, _)| n == chosen).map_or("", |(_, id)| id.as_str())))
    };
    match tab {
        AddTab::Manual => {
            body = body.child(field(t, "conn-label", "Connection name", None, text_field(s, t, "conn:label", "Connection name", "Sentry", false, false, |_| {})));
            body = body.child(field(t, "conn-scope", "Available in", None, scope_pick(popups, true)));
            let kinds = ["Hosted server", "Local executable"];
            body = body.child(field(t, "conn-type", "Connection type", None, SegmentedControl::new(&kinds, Some(usize::from(form.local))).label("Connection type").width(None).on_pick(|s: &mut App, i: usize, _| s.connection_form_local(i == 1)).build(&t.kit).key("dlg-conn-type")));
            body = body.child(field(t, "conn-target", if form.local { "Absolute executable path" } else { "Server URL" }, None, text_field(s, t, "conn:target", "Address", if form.local { "/path/to/executable" } else { "https://example.com/mcp" }, form.local, false, |s: &mut App| s.connection_form_changed())));
            if form.local {
                body = body.child(field(t, "conn-args", "Arguments (JSON array)", None, text_field(s, t, "conn:args", "Arguments", "[]", true, false, |s: &mut App| s.connection_form_changed())));
                body = body.child(field(t, "conn-cwd", "Working directory · optional", None, text_field(s, t, "conn:cwd", "Working directory", "Default", true, false, |s: &mut App| s.connection_form_changed())));
                body = body.child(kit::check(t, "dlg-conn-trust", "I trust this executable and its arguments", form.trust, |s: &mut App| s.connection_form_trust()));
                body = body.child(kit::words(t, "conn-trust-note", "Local servers execute code outside worker sandboxes. Nothing is downloaded automatically."));
            }
            body = body.child(field(t, "conn-credentials", "Credentials · optional", Some("Use {\"bearer\":\"…\"} for HTTP or {\"environment\":{\"TOKEN\":\"…\"}} for local servers. Stored in Keychain. Keep secrets out of URLs and arguments."), text_field(s, t, "conn:credentials", "Credentials", "", true, true, |_| {})));
            if let Some(n) = form_note(s, t) {
                body = body.child(n);
            }
            let ok = !s.pages.text("conn:label").trim().is_empty() && !s.pages.text("conn:target").trim().is_empty() && (!form.local || form.trust);
            body = body.child(form_buttons(s, t, "Add connection", "dlg-conn", ok, |s: &mut App| s.add_connection_manually()));
        }
        AddTab::Browse => {
            let searching = s.setup.registry.searching;
            let go = if searching { quiet_button("Searching…", "dlg-registry-search", t, |_| {}).disabled(true) } else { quiet_button("Search", "dlg-registry-search", t, |s: &mut App| s.registry_search()) };
            body = body.child(row().w_full().gap(8.0).center_y().child(text_field(s, t, "conn:search", "Search the public registry", "Search the public registry", false, false, |s: &mut App| s.registry_search()).grow(1.0)).child(go));
            if let Some(e) = &s.setup.registry.error {
                body = body.child(kit::amber(t, "dlg-registry-error", e));
            }
            let server_row = |server: &words::RegistryServer, subtitle: &str| {
                let sv = server.clone();
                let on = selected.is_some_and(|x| x.url == server.url);
                kit::list_row(t, format!("dlg-registry-{}", server.short_name), format!("{}, {subtitle}", server.name), None)
                    .fill(if on { t.surface.selected } else { rune::Color::TRANSPARENT })
                    .on_press(move |s: &mut App| s.registry_select(Some(sv.clone())))
                    .child(icon(&icons::EXTERNAL, 16.0).color(t.text.muted).a11y_hidden())
                    .child(text(server.short_name.clone()).font(t.ty.row).color(t.text.base).truncate().min_w(0.0).max_w(200.0).a11y_hidden())
                    .child(text(format!("{subtitle}{}", if server.description.is_empty() { String::new() } else { format!(" · {}", server.description) })).font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden())
            };
            body = body.child(kit::heading(t, "dlg-registry-featured", "Featured", None, true)).child(server_row(&words::featured(), "Official Sentry server"));
            if !s.setup.registry.results.is_empty() {
                body = body.child(kit::heading(t, "dlg-registry-results", "Registry results", Some(s.setup.registry.results.len()), false));
                for r in &s.setup.registry.results {
                    body = body.child(server_row(r, "Community listing · verify publisher"));
                }
            } else if !s.setup.registry.query.is_empty() && !searching && s.setup.registry.error.is_none() {
                body = body.child(kit::words(t, "dlg-registry-none", "No hosted servers match that search."));
            }
            body = body.child(quiet_button("Open the registry in your browser", "dlg-registry-open", t, |s: &mut App| s.open_link("https://registry.modelcontextprotocol.io")));
            if let Some(name) = added {
                body = body.child(col().key("dlg-registry-added").w_full().gap(6.0).pt(8.0).child(text(format!("{name} added")).font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(t.text.base)).child(text("Open it from the list to sign in and discover its tools.".to_owned()).font(t.ty.meta).color(t.text.muted)).child(row().child(primary_button("Done", "dlg-registry-done", t, |s: &mut App| s.close_dialog()))));
            } else if let Some(server) = selected {
                let (sv, sv2) = (server.clone(), server.clone());
                let _ = sv2;
                let mut block = col().key("dlg-registry-selected").w_full().gap(8.0).pt(8.0).child(text(server.name.clone()).font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(t.text.base)).child(text(server.url.clone()).font(t.ty.mono_meta).color(t.text.muted).wrap());
                block = block.child(field(t, "registry-scope", "Available in", None, scope_pick(popups, true)));
                block = block.child(kit::words(t, "registry-note", "Its tools may read or change data in the selected workspace. Read-only work sources can start a watch; manage these in Watching."));
                if let Some(n) = form_note(s, t) {
                    block = block.child(n);
                }
                block = block.child(footer(vec![quiet_button("Cancel", "dlg-registry-cancel", t, |s: &mut App| s.registry_select(None)), primary_button("Add connection", "dlg-registry-add", t, move |s: &mut App| s.add_registry_server(&sv))]));
                body = body.child(block);
            } else {
                body = body.child(kit::words(t, "dlg-registry-hint", "Select a hosted server to review its publisher, address and workspace access."));
            }
        }
        AddTab::Import => {
            let target_folders = data.workspace(workspace).map(|w| data.folders_of(w)).unwrap_or_default();
            let mut head = row().w_full().gap(8.0).center_y();
            if !data.workspaces.is_empty() {
                head = head.child(field(t, "import-ws", "Workspace for the imported connection", None, scope_pick(popups, false)).grow(1.0));
            }
            head = head.child(if workspace.is_empty() { quiet_button("Scan again", "dlg-import-scan", t, |_| {}).disabled(true) } else { quiet_button("Scan again", "dlg-import-scan", t, |s: &mut App| s.import_scan()) });
            body = body.child(head);
            if let Some(n) = note(s, t, "setup:page") {
                body = body.child(n);
            }
            let candidates: Vec<_> = data.candidates.iter().filter(|c| c.workspace.as_deref().is_none_or(|p| p.is_empty() || target_folders.iter().any(|f| f == p)) && c.metadata.get("already_in_neko").map(String::as_str) != Some("true") && !data.connections.iter().any(|x| x.source_link.as_ref().is_some_and(|l| l.candidate_id == c.id))).collect();
            if candidates.is_empty() {
                body = body.child(kit::words(t, "dlg-import-none", "No new connections found. Scan again after adding a tool in Codex, Claude, or a project folder."));
            }
            for c in candidates {
                let local = c.metadata.get("transport").map(String::as_str) != Some("http");
                let open = expanded == Some(c.id.as_str());
                let problem = c.problem.clone().unwrap_or_default();
                let paused = c.metadata.get("enabled_at_source").map(String::as_str) == Some("false");
                let chip = if !problem.is_empty() { "Needs setup" } else if paused { "Paused at source" } else if local { "Local" } else { "Remote" };
                let cid = c.id.clone();
                body = body.child(kit::list_row(t, format!("dlg-import-{}", c.id), format!("{}, {}", c.name, if local { "local" } else { "remote" }), Some(open)).on_press(move |s: &mut App| s.import_expand(&cid)).child(icon(&icons::EXTERNAL, 16.0).color(t.text.muted).a11y_hidden()).child(text(c.name.clone()).font(t.ty.row).color(t.text.base).truncate().min_w(0.0).max_w(200.0).a11y_hidden()).child(text(if c.source.is_empty() { "Local configuration".to_owned() } else { c.source.clone() }).font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden()).child(text(chip.to_owned()).font(t.ty.meta).color(if problem.is_empty() { t.text.faint } else { t.meaning.amber }).a11y_hidden()));
                if let Some(n) = note(s, t, &format!("setup:{}", c.id)) {
                    body = body.child(n);
                }
                if open {
                    let mut d = kit::details_block(format!("dlg-import-details-{}", c.id));
                    if !problem.is_empty() {
                        d = d.child(kit::amber(t, &format!("dlg-import-problem-{}", c.id), &problem));
                    }
                    if let Some(summary) = c.metadata.get("config_summary") {
                        d = d.child(text(summary.clone()).font(t.ty.mono_meta).color(t.text.muted).wrap().px(8.0).min_w(0.0));
                    }
                    if local {
                        d = d.child(kit::words(t, &format!("dlg-import-trust-{}", c.id), "This executable runs on your Mac outside the agent sandbox. Review its command before trusting it."));
                    }
                    let (cid, ok) = (c.id.clone(), problem.is_empty() && !paused && !workspace.is_empty());
                    let label = if local { "Trust and connect" } else { "Connect" };
                    d = d.child(row().px(8.0).child(if ok { primary_button(label, &format!("dlg-import-link-{}", c.id), t, move |s: &mut App| s.import_link(&cid, local)) } else { primary_button(label, &format!("dlg-import-link-{}", c.id), t, |_| {}).disabled(true).fill(t.meaning.iris.alpha(0.4)) }));
                    body = body.child(d);
                }
            }
            body = body.child(footer(vec![quiet_button("Done", "dlg-import-done", t, |s: &mut App| s.close_dialog())]));
        }
    }
    kit::dialog(t, layer, "dlg-connection", "Add connection", Some("Connect a hosted server or a trusted executable on this Mac. Nothing is trusted or linked until you choose it."), 620.0, win_h, body);
}

#[allow(clippy::too_many_arguments)]
fn schedule(s: &App, t: &Neko, popups: &mut Vec<El>, layer: &mut Vec<El>, win_h: f32, id: Option<&str>, kind: Recurrence, weekday: &str) {
    let mut body = col().w_full().gap(14.0);
    body = body.child(field(t, "sch-name", "Name", None, text_field(s, t, F_NAME, "Name", "Morning brief", false, false, |_| {})));
    body = body.child(field(t, "sch-prompt", "What to prepare", None, text_area(s, t, F_TEXT, "What to prepare", "Summarise what changed overnight and what needs me today.", 4)));
    let labels: Vec<String> = Recurrence::ALL.iter().map(|r| r.label().to_owned()).collect();
    let repeat = pick(s, t, popups, "dlg-sch-repeat", "Repeat", &labels, kind.label(), 200.0, |s: &mut App, chosen: &str| {
        if let Some(r) = Recurrence::ALL.iter().find(|r| r.label() == chosen) {
            s.schedule_set_kind(*r);
        }
    });
    let mut row_ = row().w_full().gap(12.0).wrap().child(field(t, "sch-repeat", "Repeat", None, repeat));
    match kind {
        Recurrence::Hourly => row_ = row_.child(field(t, "sch-interval", "Every N hours", None, text_field(s, t, "sch:interval", "Every N hours", "4", false, false, |_| {}).w(120.0))),
        Recurrence::Custom => {}
        _ => row_ = row_.child(field(t, "sch-time", "At", Some("Hours and minutes, like 09:00."), text_field(s, t, "sch:time", "At", "09:00", false, false, |_| {}).w(120.0))),
    }
    if kind == Recurrence::Weekly {
        let days: Vec<String> = words::WEEKDAYS.iter().map(|(_, n)| (*n).to_owned()).collect();
        let current = words::WEEKDAYS.iter().find(|(c, _)| *c == weekday).map_or("Monday", |(_, n)| *n);
        row_ = row_.child(field(t, "sch-day", "Day", None, pick(s, t, popups, "dlg-sch-day", "Day of the week", &days, current, 160.0, |s: &mut App, chosen: &str| {
            if let Some((code, _)) = words::WEEKDAYS.iter().find(|(_, n)| *n == chosen) {
                s.schedule_set_weekday(code);
            }
        })));
    }
    body = body.child(row_);
    if kind == Recurrence::Custom {
        body = body.child(field(t, "sch-custom", "Custom recurrence rule", Some("An iCalendar-style rule, e.g. FREQ=WEEKLY;BYDAY=MO;BYHOUR=9;BYMINUTE=0. The daemon validates it when you save."), text_field(s, t, "sch:custom", "Custom recurrence rule", "", true, false, |_| {})));
    }
    let (hour, minute) = words::parse_time(s.pages.text("sch:time")).unwrap_or((9, 0));
    let rule = words::rule_for(kind, hour, minute, weekday, s.pages.text("sch:interval").trim().parse().unwrap_or(4), s.pages.text("sch:custom").trim());
    body = body.child(field(t, "sch-zone", "Time zone", Some(&format!("Runs {} ({}).", words::rule_summary(&rule).to_lowercase(), s.pages.text("sch:zone").trim())), text_field(s, t, "sch:zone", "Time zone", "", true, false, |_| {})));
    if let Some(n) = form_note(s, t) {
        body = body.child(n);
    }
    let ok = !s.pages.text(F_NAME).trim().is_empty() && !s.pages.text(F_TEXT).trim().is_empty() && !s.pages.text("sch:zone").trim().is_empty();
    body = body.child(form_buttons(s, t, "Save", "dlg-sch", ok, |s: &mut App| s.save_schedule()));
    kit::dialog(t, layer, "dlg-schedule", if id.is_some() { "Edit schedule" } else { "New schedule" }, Some("Choose what to prepare and when it should run. Saving pauses this schedule; enable it after reviewing the saved settings."), 520.0, win_h, body);
}

fn profile(s: &App, t: &Neko, layer: &mut Vec<El>, win_h: f32, id: Option<&str>) {
    let mut body = col().w_full().gap(14.0);
    body = body.child(field(t, "profile-name", "Name", None, text_field(s, t, F_NAME, "Name", "Reviewer", false, false, |s: &mut App| s.save_profile())));
    body = body.child(field(t, "profile-instructions", "Instructions", None, text_area(s, t, F_TEXT, "Instructions", "Be terse. Review like a staff engineer.", 6)));
    if let Some(n) = form_note(s, t) {
        body = body.child(n);
    }
    body = body.child(form_buttons(s, t, "Save", "dlg-profile", !s.pages.text(F_NAME).trim().is_empty(), |s: &mut App| s.save_profile()));
    kit::dialog(t, layer, "dlg-profile", if id.is_some() { "Edit profile" } else { "New profile" }, Some("Set the instructions Neko uses for this kind of work."), 520.0, win_h, body);
}

#[allow(clippy::too_many_arguments)]
fn memory(s: &App, t: &Neko, popups: &mut Vec<El>, layer: &mut Vec<El>, win_h: f32, data: &SetupData, id: Option<&str>, kind: MemoryKind, workspace: &str) {
    let mut body = col().w_full().gap(14.0);
    let kinds = ["Profile", "Workspace", "Decision"];
    let hints = ["About you", "About one workspace", "Guides similar work"];
    let _ = hints;
    let which = [MemoryKind::Profile, MemoryKind::Workspace, MemoryKind::Decision];
    let kind_pick = if id.is_some() {
        text(words::kind_label(kind).to_owned()).font(t.ty.row).color(t.text.muted).px(6.0).py(6.0)
    } else {
        pick(s, t, popups, "dlg-mem-kind", "Kind", &kinds.map(str::to_owned), words::kind_label(kind), 200.0, move |s: &mut App, chosen: &str| {
            if let Some(i) = kinds.iter().position(|k| *k == chosen) {
                s.memory_set_kind(which[i]);
            }
        })
    };
    let mut top = row().w_full().gap(12.0).wrap().child(field(t, "mem-kind", "Kind", None, kind_pick));
    if kind != MemoryKind::Profile {
        let names: Vec<String> = data.workspaces.iter().map(|w| w.name.clone()).collect();
        let by: Vec<(String, String)> = data.workspaces.iter().map(|w| (w.name.clone(), w.id.clone())).collect();
        top = top.child(field(t, "mem-ws", "Workspace", None, pick(s, t, popups, "dlg-mem-workspace", "Workspace", &names, &data.workspace_name(workspace), 220.0, move |s: &mut App, chosen: &str| {
            if let Some((_, w)) = by.iter().find(|(n, _)| n == chosen) {
                s.memory_set_workspace(w);
            }
        })));
    }
    body = body.child(top);
    body = body.child(field(t, "mem-text", "What to remember", None, text_area(s, t, F_TEXT, "What to remember", "I prefer small pull requests, with tests first.", 5)));
    if let Some(n) = form_note(s, t) {
        body = body.child(n);
    }
    body = body.child(form_buttons(s, t, "Save", "dlg-mem", !s.pages.text(F_TEXT).trim().is_empty(), |s: &mut App| s.save_memory()));
    kit::dialog(t, layer, "dlg-memory", if id.is_some() { "Edit memory" } else { "Add memory" }, Some("Save context in your own words."), 520.0, win_h, body);
}

fn preference(s: &App, t: &Neko, layer: &mut Vec<El>, win_h: f32, id: Option<&str>) {
    let mut body = col().w_full().gap(14.0);
    body = body.child(field(t, "pref-instruction", "Instruction", None, text_area(s, t, F_TEXT, "Instruction", "Prefer the smallest fix that makes the failing test pass.", 3)));
    body = body.child(field(t, "pref-terms", "Words or phrases", Some("Separate phrases with commas. Guidance matches complete words in a task."), text_field(s, t, F_NAME, "Words or phrases", "flaky test, login", false, false, |_| {})));
    body = body.child(field(t, "pref-exceptions", "Exceptions", Some("Exceptions narrow when guidance applies."), text_field(s, t, F_PATH, "Exceptions", "", false, false, |_| {})));
    if let Some(n) = form_note(s, t) {
        body = body.child(n);
    }
    body = body.child(form_buttons(s, t, "Save suggestion", "dlg-pref", !s.pages.text(F_TEXT).trim().is_empty(), |s: &mut App| s.save_preference()));
    kit::dialog(t, layer, "dlg-preference", if id.is_some() { "Edit working guidance" } else { "Add working guidance" }, Some("Describe a recurring situation and how you want Neko to approach it. Saved guidance returns to Suggested until you keep it; it does not grant new permissions."), 520.0, win_h, body);
}

fn correct(s: &App, t: &Neko, layer: &mut Vec<El>, win_h: f32) {
    let mut body = col().w_full().gap(14.0);
    body = body.child(field(t, "correction", "What should Neko do differently?", None, text_area(s, t, F_TEXT, "Correction", "", 4)));
    if let Some(n) = form_note(s, t) {
        body = body.child(n);
    }
    if let Some(Dialog::Correct { record_id, .. }) = &s.setup.dialog
        && let Some(n) = note(s, t, &format!("setup:{record_id}"))
    {
        body = body.child(n);
    }
    body = body.child(form_buttons(s, t, "Save correction", "dlg-correct", !s.pages.text(F_TEXT).trim().is_empty(), |s: &mut App| s.save_correction()));
    kit::dialog(t, layer, "dlg-correct-decision", "Correct this decision", Some("The original observation stays in history. This correction applies to this decision; broader guidance needs your confirmation in Working style."), 480.0, win_h, body);
}

/// The confirmation: what is about to happen, Cancel, and the button that does it (coral when it destroys).
fn confirm(s: &App, t: &Neko, layer: &mut Vec<El>, win_h: f32, c: &Confirm) {
    let mut body = col().w_full().gap(12.0);
    for (i, line) in c.description.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        body = body.child(text(line.to_owned()).key(format!("confirm-line-{i}")).font(t.ty.meta).color(t.text.muted).wrap().min_w(0.0));
    }
    // A refusal of any of its requests, or of the form it came from, shows here.
    let mut keys: Vec<String> = c.calls.iter().map(crate::link::SetupAction::key).collect();
    keys.dedup();
    for k in keys {
        if let Some(n) = note(s, t, &k) {
            body = body.child(n);
        }
    }
    let go = if s.setup.awaiting.is_some() {
        primary_button(&c.button, "confirm-yes", t, |_| {}).disabled(true).fill(if c.coral { t.meaning.coral.alpha(0.4) } else { t.meaning.iris.alpha(0.4) })
    } else if c.coral {
        coral_button(&c.button, "confirm-yes", t, |s: &mut App| s.confirm_setup())
    } else {
        primary_button(&c.button, "confirm-yes", t, |s: &mut App| s.confirm_setup())
    };
    body = body.child(footer(vec![quiet_button("Cancel", "confirm-no", t, |s: &mut App| s.close_dialog()), go]));
    kit::dialog(t, layer, "dlg-confirm", &c.title, None, 440.0, win_h, body);
}
