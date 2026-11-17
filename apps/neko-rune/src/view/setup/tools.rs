//! Setup, Tools: the connections (MCP servers) the agents use. Problems rise to the top with a way to
//! reconnect; adding one is a dialog (browse the registry, import from this Mac, by hand). A connection
//! opens inside the section with a back arrow: sign in, refresh, pause, its details, and each tool with its
//! unattended-use grant for a workspace. Ported from `pages/setup/Tools.tsx`, `tools/ConnectionDetail.tsx`.

use neko_protocol::mcp_host::{McpConnection, McpTool, ServerConfig};
use rune::{El, col, icon, row, text};

use super::Page;
use super::kit::{self, details_block, empty, list_row};
use super::words::{self, Conn};
use crate::app::App;
use crate::icons;
use crate::link::SetupAction;
use crate::theme::Neko;
use crate::view::ui::{MenuEntry, note, pick, switch_when, with_menu};
use crate::view::{page_button, primary_button, quiet_button};

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>) -> Page {
    let Some(data) = s.setup_data() else { return Page::quiet(t, "tools-none", "Looking for your connections…") };
    if let Some(c) = s.setup.connection.as_deref().and_then(|id| data.connection(id)) {
        return detail(s, t, layer, c);
    }
    let add = page_button("Add connection", "tools-add", t, |s: &mut App| s.open_add_connection(super::state::AddTab::Browse));
    let mut content = col().w_full().gap(24.0);
    if let Some(n) = note(s, t, "setup:page") {
        content = content.child(n);
    }
    let sorted = words::sort_connections(&data.connections);
    let broken: Vec<&&McpConnection> = sorted.iter().filter(|c| words::conn_status(c).broken()).collect();
    if !broken.is_empty() {
        let line = if broken.len() == 1 { format!("{} needs attention.", broken[0].label) } else { format!("{} connections need attention.", broken.len()) };
        let banner = row().key("tools-attention").w_full().gap(10.0).child(icon(&icons::INFO, 16.0).color(t.meaning.amber).mt(3.0).a11y_hidden()).child(text(format!("{line} Agents and watches that use them can’t run until they’re reconnected.")).font(crate::view::webkit::font(t, 13.0, 400.0, 21.125)).color(t.meaning.amber).wrap().grow(1.0).min_w(0.0));
        content = content.child(banner);
    }
    if sorted.is_empty() {
        content = content.child(empty(t, "tools-empty", "No connections yet. Browse MCP servers, import from this Mac, or add a URL.", Some(("Add connection", Box::new(|s: &mut App| s.open_add_connection(super::state::AddTab::Browse))))));
    } else {
        let mut list = col().mx(-12.0);
        for c in sorted {
            list = list.child(connection_row(s, t, layer, c));
        }
        let count = data.connections.len().to_string();
        content = content.child(crate::view::webkit::section(t, "tools-connections", "Connections", Some(&count), None, list));
    }
    kit::tools_page(s, t, layer, false, Some(add), content)
}

fn status_ink(t: &Neko, c: Conn) -> rune::Color {
    match c {
        Conn::SignIn => t.meaning.amber,
        Conn::Attention => t.meaning.coral,
        _ => t.text.muted,
    }
}

fn connection_row(s: &App, t: &Neko, layer: &mut Vec<El>, c: &McpConnection) -> El {
    let id = c.id.clone();
    let status = words::conn_status(c);
    let summary = words::tool_summary(c);
    let scope = if c.workspace_id.is_empty() { "All workspaces".to_owned() } else { s.setup_data().map_or(String::new(), |d| d.workspace_name(&c.workspace_id)) };
    let open_id = id.clone();
    let leading = row().size(20.0, 20.0).center().child(icon(if matches!(c.config, neko_protocol::mcp_host::ServerConfig::Stdio { .. }) { &icons::TERMINAL } else { &icons::GLOBE }, 16.0).color(t.text.muted));
    let tools_words = if c.tools.is_empty() { "No tools".to_owned() } else { summary.clone() };
    let trailing = vec![kit::trail(t, &format!("conn-tools-{id}"), if summary == status.label() { "" } else { &tools_words }, t.text.muted.alpha(0.7)), kit::trail(t, &format!("conn-status-{id}"), status.label(), status_ink(t, status))];
    let mut line = kit::web_row(t, format!("conn-{id}"), format!("{}, {scope}, {}, {summary}", c.label, status.label()), Some(leading), &c.label, &words::conn_detail(c), trailing).on_press(move |s: &mut App| s.setup.connection = Some(open_id.clone()));
    let reconnect = words::reconnect_action(c);
    let enabled = c.enabled;
    let mut acts: Vec<kit::RowAct> = Vec::new();
    if let Some(r) = reconnect {
        let i = id.clone();
        acts.push((format!("conn-reconnect-{id}"), r.label().to_owned(), &icons::REFRESH, Box::new(move |s: &mut App| s.reconnect(&i))));
    }
    if enabled {
        let i = id.clone();
        acts.push((format!("conn-refresh-{id}"), format!("Refresh {} tools", c.label), &icons::REFRESH, Box::new(move |s: &mut App| s.setup_call(SetupAction::Refresh { id: i.clone() }))));
    }
    let i = id.clone();
    acts.push((format!("conn-switch-{id}"), format!("{} {}", if enabled { "Pause" } else { "Enable" }, c.label), if enabled { &icons::PAUSE } else { &icons::PLAY }, Box::new(move |s: &mut App| s.toggle_connection(&i))));
    line = line.child(kit::hover_actions(t, acts));
    let mut entries: Vec<MenuEntry> = Vec::new();
    let i = id.clone();
    entries.push(("Open".to_owned(), std::rc::Rc::new(move |s: &mut App| s.setup.connection = Some(i.clone()))));
    if let Some(r) = reconnect {
        let i = id.clone();
        entries.push((r.label().to_owned(), std::rc::Rc::new(move |s: &mut App| s.reconnect(&i))));
    }
    if enabled {
        let i = id.clone();
        entries.push(("Refresh tools".to_owned(), std::rc::Rc::new(move |s: &mut App| s.setup_call(SetupAction::Refresh { id: i.clone() }))));
    }
    let i = id.clone();
    entries.push((if enabled { "Pause connection" } else { "Enable connection" }.to_owned(), std::rc::Rc::new(move |s: &mut App| s.toggle_connection(&i))));
    let mut out = col().key(format!("conn-block-{id}")).w_full().child(with_menu(s, t, layer, &format!("conn:{id}"), line, entries));
    if let Some(n) = note(s, t, &format!("setup:{id}")) {
        out = out.child(n);
    }
    out
}

/// One connection, inside the section: a back arrow, sign in, refresh, pause, its details and its tools.
fn detail(s: &App, t: &Neko, layer: &mut Vec<El>, c: &McpConnection) -> Page {
    let id = c.id.clone();
    let status = words::conn_status(c);
    let data = s.setup_data();
    let back = row()
        .key("tool-back")
        .role("button")
        .label("Back to connections")
        .help("Back to connections")
        .focusable(true)
        .size(28.0, 28.0)
        .center()
        .radius(t.radius.control)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.setup.connection = None)
        .child(icon(&icons::CHEVRON_LEFT, 16.0).color(t.text.muted).a11y_hidden());

    let mut body = col().w_full().gap(10.0);
    // What is wrong, in the connection's words.
    if let Some(e) = c.error.as_deref().filter(|e| !e.is_empty()) {
        let title = if status == Conn::SignIn { "Signed out" } else { "This connection needs attention" };
        body = body.child(col().key("tool-error").w_full().px(12.0).py(8.0).gap(2.0).radius(t.radius.row).fill(status_ink(t, status).alpha(0.1)).child(text(title.to_owned()).font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(t.text.base)).child(text(e.to_owned()).key("tool-error-text").font(t.ty.meta).color(t.text.muted).wrap().min_w(0.0)));
    }
    // Sign in or reconnect (the one primary), refresh, pause.
    let mut actions = row().w_full().gap(8.0).center_y().wrap();
    if let Some(r) = words::reconnect_action(c) {
        let i = id.clone();
        actions = actions.child(primary_button(r.label(), "tool-reconnect", t, move |s: &mut App| s.reconnect(&i)));
    }
    let (rf, en) = (id.clone(), id.clone());
    actions = actions.child(if c.enabled { quiet_button("Refresh tools", "tool-refresh", t, move |s: &mut App| s.setup_call(SetupAction::Refresh { id: rf.clone() })) } else { quiet_button("Refresh tools", "tool-refresh", t, |_| {}).disabled(true) });
    let enabled = c.enabled;
    actions = actions.child(quiet_button(if enabled { "Pause connection" } else { "Enable connection" }, "tool-pause", t, move |s: &mut App| s.toggle_connection(&en)));
    if matches!(c.config, ServerConfig::Http { .. }) && words::reconnect_action(c).is_none() && c.enabled {
        let i = id.clone();
        actions = actions.child(quiet_button("Sign in", "tool-signin", t, move |s: &mut App| {
            let client = s.pages.text("conn:client").trim().to_owned();
            s.setup_call(SetupAction::Authenticate { id: i.clone(), client_id: (!client.is_empty()).then_some(client) });
            s.show_toast("Finish signing in in your browser.");
        }));
    }
    body = body.child(actions);
    if let Some(n) = note(s, t, &format!("setup:{id}")) {
        body = body.child(n);
    }
    // Details.
    body = body.child(kit::heading(t, "tool-details-head", "Connection details", None, true));
    let mut d = details_block("tool-details".to_owned()).pl(8.0);
    d = d.child(kit::detail(t, "tool-d-workspace", "Workspace", &data.map_or(String::new(), |x| x.workspace_name(&c.workspace_id)), false));
    match &c.config {
        ServerConfig::Http { url } => {
            d = d.child(kit::detail(t, "tool-d-url", "URL", url, true));
            d = d.child(kit::field(t, "client", "OAuth client ID", Some("Only if the server asks for one. Sign-in never grants tool access."), kit::text_field(s, t, "conn:client", "OAuth client ID", "Optional", false, false, |_| {})));
        }
        ServerConfig::Stdio { command, args, cwd } => {
            d = d.child(kit::detail(t, "tool-d-command", "Command", command, true));
            d = d.child(kit::detail(t, "tool-d-args", "Arguments", &if args.is_empty() { "None".to_owned() } else { args.join(" ") }, true));
            if let Some(cwd) = cwd.as_deref().filter(|c| !c.is_empty()) {
                d = d.child(kit::detail(t, "tool-d-cwd", "Working directory", cwd, true));
            }
            d = d.child(kit::detail(t, "tool-d-trust", "Trust", if c.trusted { "Trusted local executable" } else { "Local executable trust required" }, false));
        }
    }
    d = d.child(kit::detail(t, "tool-d-credentials", "Credentials", if c.has_credentials { "Stored in Keychain" } else { "None" }, false));
    if let Some(link) = &c.source_link {
        d = d.child(kit::detail(t, "tool-d-source", "Linked source", &link.source_path, true));
    }
    body = body.child(d);
    body = body.child(tools_list(s, t, layer, c));
    Page { count: (!c.tools.is_empty()).then_some(c.tools.len()), trailing: Vec::new(), body, title: Some(c.label.clone()), back: Some(back) }
}

fn badge(t: &Neko, key: String, read_only: bool) -> El {
    text(if read_only { "Read-only" } else { "May change data" }.to_owned()).key(key).font(t.ty.id).color(if read_only { t.text.muted } else { t.meaning.amber }).a11y_hidden()
}

/// The tools of a connection, with the grant of each for a workspace: allowed to run unattended there, or not.
fn tools_list(s: &App, t: &Neko, layer: &mut Vec<El>, c: &McpConnection) -> El {
    let id = c.id.as_str();
    let data = s.setup_data();
    let mut out = col().key("tool-list").w_full().gap(2.0);
    let query = s.pages.text("conn:tools-search").trim().to_lowercase();
    let shown: Vec<&McpTool> = c.tools.iter().filter(|x| query.is_empty() || format!("{} {}", x.name, x.description).to_lowercase().contains(&query)).collect();
    let mut head = row().w_full().gap(8.0).center_y().child(kit::heading(t, "tool-tools-head", "Tools", (!c.tools.is_empty()).then_some(c.tools.len()), false).grow(1.0));
    // A global connection is granted per workspace: which one is shown here.
    let workspace = s.grant_workspace();
    if c.workspace_id.is_empty()
        && let Some(d) = data
        && d.workspaces.len() > 1
    {
        let names: Vec<String> = d.workspaces.iter().map(|w| w.name.clone()).collect();
        let current = d.workspace_name(&workspace);
        let by: Vec<(String, String)> = d.workspaces.iter().map(|w| (w.name.clone(), w.id.clone())).collect();
        head = head.child(pick(s, t, layer, "tool-grant-workspace", "Grants for workspace", &names, &current, 180.0, move |s: &mut App, chosen: &str| {
            if let Some((_, ws)) = by.iter().find(|(n, _)| n == chosen) {
                s.setup.grant_workspace = Some(ws.clone());
            }
        }));
    } else if let Some(d) = data {
        let name = if c.workspace_id.is_empty() { d.workspace_name(&workspace) } else { d.workspace_name(&c.workspace_id) };
        head = head.child(text(format!("Unattended use in {name}")).key("tool-grant-for").font(t.ty.meta).color(t.text.faint));
    }
    out = out.child(head);
    if !c.tools.is_empty() {
        out = out.child(kit::text_field(s, t, "conn:tools-search", "Search tools", "Search tools", false, false, |_| {}));
    }
    if c.tools.is_empty() {
        let line = if !c.enabled {
            "Enable this connection to discover its tools."
        } else if words::conn_status(c) == Conn::SignIn {
            "Sign in to discover this server’s tools."
        } else if c.error.is_some() {
            "Fix the problem above, then refresh tools."
        } else {
            "Refresh tools to discover what this server offers."
        };
        return out.child(kit::words(t, "tool-no-tools", line));
    }
    if shown.is_empty() {
        return out.child(kit::words(t, "tool-no-match", &format!("No tools match “{}”.", s.pages.text("conn:tools-search").trim())));
    }
    let grant_ws = if c.workspace_id.is_empty() { workspace.clone() } else { c.workspace_id.clone() };
    let can_grant = c.enabled && c.trusted && c.error.is_none() && !grant_ws.is_empty();
    for tool in shown {
        let key = format!("tool-{}", tool.name);
        let open = s.row_open(&format!("tool:{id}:{}", tool.name));
        let granted = data.is_some_and(|d| d.granted(id, &grant_ws, &tool.name, &tool.schema_hash));
        let (toggle, g_conn, g_name, g_hash) = (format!("tool:{id}:{}", tool.name), id.to_owned(), tool.name.clone(), tool.schema_hash.clone());
        let line = list_row(t, key.clone(), format!("{}, {}", tool.name, if tool.read_only { "declared read-only" } else { "may change data" }), Some(open))
            .on_press(move |s: &mut App| s.toggle_row(&toggle))
            .child(icon(if open { &icons::CHEVRON_DOWN } else { &icons::CHEVRON_RIGHT }, 16.0).color(t.text.faint).a11y_hidden())
            .child(text(tool.name.clone()).font(t.ty.mono_meta).color(t.text.base).truncate().min_w(0.0).max_w(240.0).a11y_hidden())
            .child(badge(t, format!("{key}-badge"), tool.read_only))
            .child(text(first_line(&tool.description)).font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden())
            .child(switch_when(t, &format!("{key}-grant"), &format!("Allow {} to run unattended", tool.name), granted, can_grant, move |s: &mut App| s.set_tool_grant(&g_conn, &g_name, &g_hash, !granted)));
        out = out.child(line);
        if open {
            let mut d = details_block(format!("{key}-details"));
            if !tool.description.trim().is_empty() {
                d = d.child(text(tool.description.clone()).font(t.ty.meta).color(t.text.base).wrap().px(8.0).min_w(0.0));
            }
            d = d.child(kit::words(t, &format!("{key}-note"), if tool.read_only { "The server declares this tool read-only. Chat lookups can run without another prompt." } else { "Chat asks before each call. Turned-on watches may use their selected connections unattended." }));
            if !tool.input_schema.trim().is_empty() {
                let schema: String = tool.input_schema.chars().take(1200).collect();
                d = d.child(text(schema).key(format!("{key}-schema")).font(t.ty.mono_meta).color(t.text.muted).wrap().px(8.0).py(8.0).radius(t.radius.row).fill(t.surface.hover).min_w(0.0));
            }
            out = out.child(d);
        }
    }
    out
}

fn first_line(text: &str) -> String {
    text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_owned()
}
