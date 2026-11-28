//! The plugin scenes (docs/knowledge/plugins.md): Settings ▸ Plugins empty and with plugins in every state, a check that worked and
//! one that failed, the plugin's own fields (a secret set and not), which cats may use one, the Add sheet reading a paste, the
//! approval card in Needs you and the same after a change, a task using a plugin's tool, and a cat's page. Placeholder data only.

use neko_protocol::plugins::{Access, AccessMode, Approval, Check, FieldKind, FieldValue, Manifest, Mcp, Plugin, PluginKind, SettingField};
use neko_protocol::{Need, NeedsItem, PermissionState, RunStatus, TaskStatus, ToolState};
use rune::TextEdit;

use super::{Script, ago, open_with, permission_item, run, sample_tasks, start, with_workspaces};
use crate::plugins::{AddSheet, SETTINGS_PAGE, ask_of};
use crate::state::App;

fn field(key: &str, kind: FieldKind, label: &str, help: Option<&str>) -> SettingField {
    SettingField { key: key.into(), kind, label: label.into(), help: help.map(str::to_owned), default: None, choices: Vec::new(), required: false }
}

fn mcp(command: &str, args: &[&str], env: &[(&str, &str)]) -> Mcp {
    Mcp { command: Some(command.into()), args: args.iter().map(|a| (*a).to_owned()).collect(), env: env.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(), ..Mcp::default() }
}

fn manifest(name: &str, version: &str, description: Option<&str>, mcp: Mcp, settings: Vec<SettingField>) -> Manifest {
    Manifest { neko_plugin: 1, name: name.into(), version: version.into(), description: description.map(str::to_owned), kind: PluginKind::Mcp, mcp, settings }
}

/// A plugin as the daemon would send it: approved when `approval` says so, set up as `set` says (`(key, Some(value))` for a field with a
/// value; a secret's value is only `set`).
fn plugin(n: u32, m: Manifest, folder: Option<&str>, approval: Approval, enabled: bool, set: &[(&str, Option<&str>)]) -> Plugin {
    let digest = m.digest(folder);
    let values: Vec<FieldValue> = m
        .settings
        .iter()
        .map(|f| match set.iter().find(|(k, _)| *k == f.key) {
            Some((_, v)) => FieldValue { key: f.key.clone(), value: if f.kind == FieldKind::Secret { None } else { v.map(str::to_owned) }, set: true },
            None => FieldValue { key: f.key.clone(), value: None, set: false },
        })
        .collect();
    let missing: Vec<String> = m.settings.iter().filter(|f| f.required && f.default.is_none() && !values.iter().any(|v| v.key == f.key && v.set)).map(|f| f.label.clone()).collect();
    Plugin {
        id: format!("plugin_{n:026}"),
        name: m.name.clone(),
        version: m.version.clone(),
        description: m.description.clone(),
        kind: PluginKind::Mcp,
        folder: folder.map(str::to_owned),
        approved_digest: (approval == Approval::Approved).then(|| digest.clone()),
        approved_ms: (approval == Approval::Approved).then_some(1),
        digest,
        manifest: m,
        approval,
        changes: Vec::new(),
        enabled,
        access: Access::default(),
        values,
        missing,
        last_check: None,
        problem: None,
        created_ms: i64::from(n),
        changed_ms: i64::from(n),
    }
}

/// `acme-docs`, pasted: `npx -y @acme/docs-server`, handed `ACME_TOKEN`.
fn acme_docs(n: u32, approval: Approval, enabled: bool, token_set: bool) -> Plugin {
    let token = field("acme_token", FieldKind::Secret, "ACME_TOKEN", Some("Passed to the server as ACME_TOKEN."));
    let m = manifest("acme-docs", "0", None, mcp("npx", &["-y", "@acme/docs-server"], &[("ACME_TOKEN", "${settings.acme_token}")]), vec![token]);
    plugin(n, m, None, approval, enabled, &if token_set { vec![("acme_token", None)] } else { vec![] })
}

/// A plugin from a folder, declaring a setting of every kind.
fn handbook(n: u32, approval: Approval, enabled: bool, secret_set: bool) -> Plugin {
    let mut token = field("api_key", FieldKind::Secret, "API key", Some("Neko keeps it in the Keychain and never shows it again."));
    token.required = true;
    let mut space = field("space", FieldKind::Text, "Space", Some("The handbook space to search."));
    space.default = Some("handbook".into());
    let mut region = field("region", FieldKind::Choice, "Region", None);
    region.choices = vec!["eu".into(), "us".into(), "asia".into()];
    region.default = Some("eu".into());
    let mut drafts = field("drafts", FieldKind::YesNo, "Include drafts", Some("Search pages that are not published yet."));
    drafts.default = Some("no".into());
    let root = field("root", FieldKind::Folder, "Local copy", Some("A folder with a copy of the handbook, for searching offline."));
    let m = manifest(
        "acme-handbook",
        "0.3.1",
        Some("Searches and reads the Acme handbook."),
        mcp("node", &["${plugin.dir}/server.js"], &[("ACME_API_KEY", "${settings.api_key}"), ("ACME_SPACE", "${settings.space}"), ("ACME_REGION", "${settings.region}")]),
        vec![token, space, region, drafts, root],
    );
    let mut set: Vec<(&str, Option<&str>)> = vec![("space", Some("engineering")), ("region", Some("us")), ("root", Some("/Users/example/Documents/handbook"))];
    if secret_set {
        set.push(("api_key", None));
    }
    plugin(n, m, Some("/Users/example/plugins/acme-handbook"), approval, enabled, &set)
}

/// A server reached by a web address, handed a header.
fn tracker(n: u32, approval: Approval, enabled: bool) -> Plugin {
    let token = field("authorization", FieldKind::Secret, "Authorization", Some("Sent to the server as the Authorization header."));
    let mcp = Mcp { url: Some("https://mcp.example.com/v1".into()), headers: [("Authorization".to_owned(), "${settings.authorization}".to_owned())].into(), ..Mcp::default() };
    let m = manifest("example-tracker", "0", None, mcp, vec![token]);
    plugin(n, m, None, approval, enabled, &[("authorization", None)])
}

fn notes_sync(n: u32, approval: Approval, enabled: bool) -> Plugin {
    let m = manifest("notes-sync", "1.2.0", Some("Reads and files notes."), mcp("node", &["${plugin.dir}/notes.js"], &[]), vec![]);
    plugin(n, m, Some("/Users/example/plugins/notes-sync"), approval, enabled, &[])
}

fn checked(mut p: Plugin, s: &App, ok: bool) -> Plugin {
    p.last_check = Some(if ok {
        Check { at_ms: ago(s, 3), ok: true, server: Some("acme-docs-server".into()), version: Some("0.3.1".into()), tools: ["search", "fetch_page", "list_pages", "get_page", "create_draft", "list_spaces"].map(str::to_owned).to_vec(), error: None }
    } else {
        Check { at_ms: ago(s, 3), ok: false, server: None, version: None, tools: Vec::new(), error: Some("The server did not answer in 10 seconds. Check that npx can run on this Mac.".into()) }
    });
    p
}

fn changed(mut p: Plugin) -> Plugin {
    p.approval = Approval::Changed;
    p.enabled = true;
    p.changes = vec!["Runs: node server.js to node server.js --verbose".into(), "Now also passes ACME_REGION".into()];
    p
}

fn page(s: &mut App) {
    s.settings_open = true;
    s.settings_page = SETTINGS_PAGE;
    s.tasks_loaded = true;
}

fn with_plugins(s: &mut App, list: Vec<Plugin>) {
    with_workspaces(s);
    s.plugins = list;
    page(s);
}

// ---- Settings ▸ Plugins -----------------------------------------------------------------------------------------------------------

pub fn plugins_empty(s: &mut App) {
    with_plugins(s, vec![]);
}

pub fn plugins_waiting(s: &mut App) {
    with_plugins(s, vec![acme_docs(1, Approval::Waiting, false, true)]);
}

/// On, off and waiting together.
pub fn plugins(s: &mut App) {
    let a = checked(acme_docs(1, Approval::Approved, true, true), s, true);
    with_plugins(s, vec![a, notes_sync(2, Approval::Approved, false), tracker(3, Approval::Waiting, false)]);
}

pub fn plugins_changed(s: &mut App) {
    with_plugins(s, vec![changed(handbook(1, Approval::Approved, true, true)), notes_sync(2, Approval::Declined, false)]);
}

pub fn plugins_needs_setup(s: &mut App) {
    let mut broken = notes_sync(2, Approval::Approved, true);
    broken.problem = Some("Its folder is not there any more: /Users/example/plugins/notes-sync.".into());
    with_plugins(s, vec![handbook(1, Approval::Approved, true, false), broken]);
}

pub fn plugins_check_ok(s: &mut App) {
    let a = checked(acme_docs(1, Approval::Approved, true, true), s, true);
    with_plugins(s, vec![a]);
}

pub fn plugins_check_failed(s: &mut App) {
    let a = checked(acme_docs(1, Approval::Approved, true, true), s, false);
    with_plugins(s, vec![a]);
}

pub fn plugins_testing(s: &mut App) {
    plugins_check_ok(s);
    let id = s.plugins[0].id.clone();
    s.plugin_ui.testing.insert(id);
}

pub fn plugins_web(s: &mut App) {
    with_plugins(s, vec![tracker(1, Approval::Approved, true)]);
}

/// A plugin with a setting of every kind; its API key not set yet.
pub fn plugins_fields_unset(s: &mut App) {
    with_plugins(s, vec![handbook(1, Approval::Approved, true, false)]);
}

pub fn plugins_fields_set(s: &mut App) {
    let p = checked(handbook(1, Approval::Approved, true, true), s, true);
    with_plugins(s, vec![p]);
}

/// The person is changing a secret: a masked field with something typed in it, and Set.
pub fn plugins_secret_changing(s: &mut App) {
    plugins_fields_set(s);
    let id = s.plugins[0].id.clone();
    let mut e = TextEdit::new("a-new-secret-value");
    e.focused = true;
    s.plugin_ui.secrets.insert((id, "api_key".into()), e);
}

/// Only some cats: mochi chosen.
pub fn plugins_access_only(s: &mut App) {
    let mut a = checked(acme_docs(1, Approval::Approved, true, true), s, true);
    a.access = Access { mode: AccessMode::Only, cats: vec!["cat_mochi".into()] };
    with_plugins(s, vec![a]);
}

/// "Only some cats" chosen, no cat yet: nothing was sent.
pub fn plugins_access_none(s: &mut App) {
    plugins_check_ok(s);
    let id = s.plugins[0].id.clone();
    s.plugin_ui.only.insert(id);
}

pub fn plugins_remove(s: &mut App) {
    plugins(s);
    let id = s.plugins[0].id.clone();
    s.ask_remove_plugin(&id);
}

// ---- The Add sheet ------------------------------------------------------------------------------------------------------------------------

fn sheet(s: &mut App, json: &str, name: &str) {
    with_plugins(s, vec![]);
    let mut sh = AddSheet::blank();
    sh.json = TextEdit::new(json);
    sh.name = TextEdit::new(name);
    s.plugin_ui.add = Some(sh);
}

const ONE: &str = r#"{
  "mcpServers": {
    "acme-docs": {
      "command": "npx",
      "args": ["-y", "@acme/docs-server"],
      "env": { "ACME_TOKEN": "example-token-4f9a" }
    }
  }
}"#;

pub fn plugin_add(s: &mut App) {
    sheet(s, "", "");
}

pub fn plugin_add_good(s: &mut App) {
    sheet(s, ONE, "");
}

pub fn plugin_add_bad(s: &mut App) {
    sheet(s, "{\n  \"mcpServers\": {\n    \"acme-docs\": {\n      \"command\": \"npx\",\n      \"args\": [\"-y\", \"@acme/docs-server\"\n    }\n  }\n}", "");
}

pub fn plugin_add_several(s: &mut App) {
    sheet(
        s,
        r#"{"mcpServers": {
  "acme-docs": {"command": "npx", "args": ["-y", "@acme/docs-server"], "env": {"ACME_TOKEN": "example-token-4f9a"}},
  "example-tracker": {"url": "https://mcp.example.com/v1", "headers": {"Authorization": "Bearer example-token-77c1"}},
  "notes": {"command": "node", "args": ["/Users/example/tools/notes.js"], "timeout": 30000}
}}"#,
        "",
    );
}

pub fn plugin_add_unnamed(s: &mut App) {
    sheet(s, r#"{"command": "npx", "args": ["-y", "@acme/docs-server@latest"], "env": {"ACME_TOKEN": "example-token-4f9a"}}"#, "");
}

pub fn plugin_add_refused(s: &mut App) {
    plugin_add_good(s);
    if let Some(sh) = &mut s.plugin_ui.add {
        sh.error = Some("There are 64 plugins already. Remove one first.".into());
    }
}

// ---- Needs you ------------------------------------------------------------------------------------------------------------------------------

fn plugin_item(s: &App, p: &Plugin, minutes: i64) -> NeedsItem {
    NeedsItem { id: format!("plugin:{}", p.id), task: neko_protocol::TaskId(String::new()), workspace: String::new(), title: p.name.clone(), cat: String::new(), cat_name: String::new(), since_ms: ago(s, minutes), need: Need::Plugin { ask: ask_of(p) }, subagent: None }
}

/// The approval card among the other things that wait.
pub fn needs_you_plugin(s: &mut App) {
    with_workspaces(s);
    let retry = s.tasks.iter().find(|t| t.title.starts_with("Answer the question about the retry limit")).cloned().expect("the sample thread");
    let waiting = acme_docs(1, Approval::Waiting, false, true);
    let folder = handbook(2, Approval::Waiting, false, false);
    s.plugins = vec![waiting.clone(), folder.clone()];
    s.needs = vec![permission_item(s, &retry, "Run a command", "cargo test --package acme-api retry", false, 30), plugin_item(s, &waiting, 4), plugin_item(s, &folder, 1)];
    s.open_needs();
}

pub fn needs_you_plugin_changed(s: &mut App) {
    with_workspaces(s);
    let p = changed(handbook(1, Approval::Approved, true, true));
    s.plugins = vec![p.clone()];
    s.needs = vec![plugin_item(s, &p, 9)];
    s.open_needs();
}

/// A server reached by a web address: its address, the header it is sent.
pub fn needs_you_plugin_web(s: &mut App) {
    with_workspaces(s);
    let p = tracker(1, Approval::Waiting, false);
    s.plugins = vec![p.clone()];
    s.needs = vec![plugin_item(s, &p, 2)];
    s.open_needs();
}

/// The plugin card when nothing else waits, at the window's smallest.
pub fn needs_you_plugin_only(s: &mut App) {
    with_workspaces(s);
    let p = notes_sync(1, Approval::Waiting, false);
    s.plugins = vec![p.clone()];
    s.needs = vec![plugin_item(s, &p, 1)];
    s.open_needs();
}

// ---- A task and a cat ----------------------------------------------------------------------------------------------------------------------

/// A task whose steps used a plugin's tools, one still running, and its request to use another.
pub fn task_plugin(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    sc.think("The retry limit is documented in the Acme docs, so I will search them before I read any code.", 3);
    sc.tool("mcp__acme-docs__search", "retry limit", "{\n  \"query\": \"retry limit\"\n}", "3 results", ToolState::Done, 2);
    sc.tool("mcp__acme-docs__get_page", "Retries and timeouts", "{\n  \"page\": \"retries-and-timeouts\"\n}", "Retries and timeouts: the limit is 5.", ToolState::Done, 2);
    sc.tool("Read", "src/retry.rs", "{\n  \"file_path\": \"src/retry.rs\"\n}", "pub const LIMIT: u32 = 5;", ToolState::Done, 1);
    sc.tool("mcp__acme-docs__create_draft", "", "{\n  \"title\": \"Retry limit\"\n}", "", ToolState::Running, 0);
    let r = run("run_1", task, RunStatus::Running, start(s), 0);
    s.plugins = vec![checked(acme_docs(1, Approval::Approved, true, true), s, true)];
    open_with(s, TaskStatus::Working, vec![r], vec![sc]);
}

/// The request to use a plugin's tool, as the daemon words it.
pub fn task_plugin_request(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    sc.tool("mcp__acme-docs__search", "retry limit", "{\n  \"query\": \"retry limit\"\n}", "3 results", ToolState::Done, 2);
    sc.ask("Use create_draft from acme-docs", "acme-docs is a plugin you approved in Neko. It acts with its own rights, not Neko's, so Neko asks before each tool.", "{\n  \"title\": \"Retry limit\"\n}", false, PermissionState::Pending);
    for p in sc.perms.values_mut() {
        p.tool = "mcp__acme-docs__create_draft".into();
    }
    let r = run("run_1", task, RunStatus::Running, start(s), 0);
    s.plugins = vec![acme_docs(1, Approval::Approved, true, true)];
    open_with(s, TaskStatus::NeedsYou, vec![r], vec![sc]);
}

/// miso's page, with the plugins it may use: acme-docs for every cat, notes-sync only for mochi, one off.
pub fn cat_page_plugins(s: &mut App) {
    with_workspaces(s);
    let mut only_mochi = notes_sync(2, Approval::Approved, true);
    only_mochi.access = Access { mode: AccessMode::Only, cats: vec!["cat_mochi".into()] };
    s.plugins = vec![acme_docs(1, Approval::Approved, true, true), only_mochi, tracker(3, Approval::Approved, false)];
    super::cattains::show_profile(s, "cat_miso");
}

pub fn cat_page_no_plugins(s: &mut App) {
    with_workspaces(s);
    super::cattains::show_profile(s, "cat_miso");
}

pub fn cat_page_plugins_none_for_cat(s: &mut App) {
    with_workspaces(s);
    let mut only = notes_sync(1, Approval::Approved, true);
    only.access = Access { mode: AccessMode::Only, cats: vec!["cat_mochi".into()] };
    s.plugins = vec![only];
    super::cattains::show_profile(s, "cat_miso");
}
