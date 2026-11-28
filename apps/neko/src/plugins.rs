//! Plugins in the app (docs/knowledge/plugins.md): the words for a plugin's state, what it runs and how a check went; the Add sheet's
//! state, which reads a paste live exactly as the daemon will; and what each press sends. The daemon keeps the plugins, their
//! approvals and their secrets; this file keeps only what is on screen. A secret the person types is sent once and dropped: nothing
//! here holds one beyond the press that sends it, and nothing here ever shows one.

use std::collections::{HashMap, HashSet};

use neko_protocol::plugins::{self, Access, AccessMode, Approval, Check, FieldKind, Parsed, Plugin, PluginAsk, PluginState, SettingField};
use neko_protocol::schedule::Zone;
use neko_protocol::{NeedsItem, Need, Request};
use rune::TextEdit;
use rune_ui_style::Tone;

use crate::state::{App, Asked, Confirm};

/// What the person types in the Add sheet while they do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AddSheet {
    /// The pasted JSON. It may hold secret values: it is read here, sent once, and dropped when the sheet closes.
    pub json: TextEdit,
    /// A name for a single server that has none.
    pub name: TextEdit,
    /// Why the daemon refused it, in words.
    pub error: Option<String>,
    /// It was sent and the daemon has not answered.
    pub sending: bool,
}

/// What a paste says, read with the daemon's own code ([`plugins::parse_mcp_json`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reading {
    /// Nothing pasted yet.
    Empty,
    /// Not something Neko can add, and why, in words.
    Bad(String),
    /// The servers it holds.
    Found(Vec<Parsed>),
}

impl AddSheet {
    pub fn blank() -> AddSheet {
        let mut json = TextEdit::default();
        json.focused = true;
        AddSheet { json, ..AddSheet::default() }
    }

    /// The name the person gave, if any.
    pub fn given(&self) -> Option<String> {
        Some(self.name.text.trim().to_owned()).filter(|n| !n.is_empty())
    }

    /// Whether the paste is one server with no name of its own (the sheet then offers a Name field).
    pub fn wants_name(&self) -> bool {
        let Ok(serde_json::Value::Object(top)) = serde_json::from_str::<serde_json::Value>(&self.json.text) else { return false };
        let wrapped = top.get("mcpServers").or_else(|| top.get("servers")).is_some_and(serde_json::Value::is_object);
        !wrapped && (top.contains_key("command") || top.contains_key("url")) && !top.contains_key("name")
    }

    pub fn reading(&self) -> Reading {
        if self.json.text.trim().is_empty() {
            return Reading::Empty;
        }
        match plugins::parse_mcp_json(&self.json.text, self.given().as_deref()) {
            Ok(found) => Reading::Found(found),
            Err(why) => Reading::Bad(why),
        }
    }

    /// Add can be pressed: a paste that reads, and no request still out.
    pub fn ready(&self) -> bool {
        !self.sending && matches!(self.reading(), Reading::Found(_))
    }
}

/// What the plugin screens keep: the sheet, what is pending, a refusal each plugin shows, and the fields being typed in.
#[derive(Clone, Debug, Default)]
pub struct Ui {
    pub add: Option<AddSheet>,
    /// Plugins whose check is running (the reply comes up to ten seconds later).
    pub testing: HashSet<String>,
    /// Plugins with a change out that the daemon has not answered.
    pub busy: HashSet<String>,
    /// Why the daemon refused the last change of a plugin, shown on its card.
    pub notes: HashMap<String, String>,
    /// Text and folder settings being edited, by (plugin, key). A field with no entry shows what is saved.
    pub edits: HashMap<(String, String), TextEdit>,
    /// Secret fields being typed in, by (plugin, key): present while the masked field shows. Dropped the moment it is sent.
    pub secrets: HashMap<(String, String), TextEdit>,
    /// Secrets that are set and being changed, by (plugin, key): the masked field shows instead of "Set". Only the person's Change
    /// puts one here, so a stray edit event never turns a set secret back into a field.
    pub changing: HashSet<(String, String)>,
    /// Plugins the person narrowed to "Only some cats" and chose no cat for yet: nothing is sent until one is chosen.
    pub only: HashSet<String>,
}

/// What a change to one plugin was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Did {
    Approve,
    Decline,
    Other,
}

/// Settings' page for plugins.
pub const SETTINGS_PAGE: crate::settings_view::Page = crate::settings_view::Page::Plugins;

/// A plugin's state in words, as the list and its card say it.
pub fn state_words(s: PluginState) -> &'static str {
    match s {
        PluginState::Waiting => "Waiting for your approval",
        PluginState::On => "On",
        PluginState::Off => "Off",
        PluginState::Changed => "Changed since you approved it",
        PluginState::Declined => "Declined",
        PluginState::NeedsSetup => "Needs setting up",
        PluginState::Problem => "Can't be read",
    }
}

/// The badge's meaning: amber where the person is needed, red where it cannot go on, green only for "on".
pub fn state_tone(s: PluginState) -> Tone {
    match s {
        PluginState::Waiting | PluginState::Changed | PluginState::NeedsSetup => Tone::Warning,
        PluginState::Problem => Tone::Danger,
        PluginState::On => Tone::Success,
        PluginState::Off | PluginState::Declined => Tone::Neutral,
    }
}

/// Whether the plugin has been approved as it is (so it can be switched, tested and used).
pub fn approved(p: &Plugin) -> bool {
    p.approval == Approval::Approved
}

/// "ACME_TOKEN", "ACME_TOKEN and ACME_REGION", "A, B and C".
pub fn list_words(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// What a plugin is handed, by name only: "Passes ACME_TOKEN" for a program, "Sends the header Authorization" for a web address.
pub fn passes_words(remote: bool, passes: &[String]) -> Option<String> {
    if passes.is_empty() {
        return None;
    }
    Some(if remote { format!("Sends the {} {}", if passes.len() == 1 { "header" } else { "headers" }, list_words(passes)) } else { format!("Passes {}", list_words(passes)) })
}

/// The host of an address, and nothing after it (a path or a query may hold a token).
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    authority.rsplit('@').next().unwrap_or(authority).to_owned()
}

/// Where the plugin came from, under its name: "MCP server · pasted", "· from ~/plugins/acme", "· at example.com".
pub fn kind_line(p: &Plugin, home: Option<&str>) -> String {
    let from = if let Some(url) = &p.manifest.mcp.url {
        format!("at {}", host_of(url))
    } else if let Some(f) = &p.folder {
        format!("from {}", crate::task_view::show_path(f, "", home))
    } else {
        "pasted".to_owned()
    };
    format!("MCP server · {from}")
}

/// What the Needs you card, the approval block and the notification all show for one plugin, made from the plugin as it is.
pub fn ask_of(p: &Plugin) -> PluginAsk {
    PluginAsk {
        plugin: p.id.clone(),
        name: p.name.clone(),
        version: p.version.clone(),
        digest: p.digest.clone(),
        runs: p.manifest.command_line(p.folder.as_deref()),
        remote: p.manifest.mcp.is_remote(),
        passes: p.manifest.passes(),
        changed: p.approval == Approval::Changed,
        changes: p.changes.clone(),
        description: p.description.clone(),
        folder: p.folder.clone(),
    }
}

/// An item's whole ask in one sentence, for assistive technology and the briefing: "Approve plugin: acme-docs. It will run node
/// server.js with ACME_TOKEN".
pub fn ask_sentence(a: &PluginAsk) -> String {
    if a.changed {
        let what = if a.changes.is_empty() { String::new() } else { format!(": {}", a.changes.join("; ")) };
        return format!("{} changed since you approved it{what}", a.name);
    }
    let with = match (a.remote, a.passes.is_empty()) {
        (_, true) => String::new(),
        (false, false) => format!(" with {}", list_words(&a.passes)),
        (true, false) => format!(" with the {} {}", if a.passes.len() == 1 { "header" } else { "headers" }, list_words(&a.passes)),
    };
    format!("Approve plugin: {}. It will {} {}{with}", a.name, if a.remote { "connect to" } else { "run" }, a.runs)
}

/// What the notification says: never the command (the card does), only what is asked.
pub fn note_words(a: &PluginAsk) -> String {
    if a.changed {
        let what = if a.changes.is_empty() { String::new() } else { format!(": {}", a.changes.join("; ")) };
        format!("{} changed{what}", a.name)
    } else {
        format!("Approve the plugin {}", a.name)
    }
}

/// How the last check went: its headline and, when it connected, the tool names; then when, in the middle of a sentence.
pub fn check_words(c: &Check, now_ms: i64, zone: &dyn Zone) -> (String, Option<String>, String) {
    let when = crate::rounds::when_at(c.at_ms, now_ms, zone);
    if c.ok {
        let n = c.tools.len();
        let head = match n {
            0 => "Connected: no tools".to_owned(),
            1 => "Connected: 1 tool".to_owned(),
            n => format!("Connected: {n} tools"),
        };
        let tools = (n > 0).then(|| {
            let shown: Vec<String> = c.tools.iter().take(8).cloned().collect();
            if n > 8 { format!("{} and {} more", shown.join(", "), n - 8) } else { list_words(&shown) }
        });
        (head, tools, when)
    } else {
        (format!("The test failed: {}", c.error.as_deref().unwrap_or("it did not answer")), None, when)
    }
}

/// The words for which cats may use a plugin, for the card's quiet line: "Every cat", "miso and tofu", "Every cat but mochi".
pub fn access_words(a: &Access, cats: &[(String, String)]) -> String {
    let named = |ids: &[String]| -> Vec<String> { ids.iter().filter_map(|i| cats.iter().find(|(c, _)| c == i).map(|(_, n)| n.clone())).collect() };
    match a.mode {
        AccessMode::All => "Every cat".to_owned(),
        AccessMode::Only => list_words(&named(&a.cats)),
        AccessMode::Except => format!("Every cat but {}", list_words(&named(&a.cats))),
    }
}

/// The line the Add sheet says for each server it will add: "Will add acme-docs: runs `npx -y @acme/docs-server`, passes
/// ACME_TOKEN". Values are never in it, only names.
pub fn found_words(p: &Parsed) -> String {
    let m = &p.manifest;
    let how = if m.mcp.is_remote() { format!("connects to {}", m.command_line(None)) } else { format!("runs `{}`", m.command_line(None)) };
    let passes = passes_words(m.mcp.is_remote(), &m.passes()).map(|w| format!(", {}", lower_first(&w))).unwrap_or_default();
    format!("Will add {}: {how}{passes}", m.name)
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// A setting's saved value as a field shows it: what was set, else its default.
pub fn shown_value(p: &Plugin, f: &SettingField) -> Option<String> {
    p.value(&f.key).and_then(|v| v.value.clone()).filter(|v| !v.is_empty()).or_else(|| f.default.clone())
}

impl App {
    pub fn plugin(&self, id: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.id == id)
    }

    /// Settings ▸ Plugins, at a plugin when one is named: the keyboard goes to the plugin's first control.
    pub fn open_plugins(&mut self, at: Option<&str>) {
        self.open_settings();
        self.set_settings_page(SETTINGS_PAGE);
        if let Some(p) = at.and_then(|id| self.plugin(id)) {
            let key = match p.state() {
                PluginState::Waiting | PluginState::Changed | PluginState::Declined => format!("plugin-{}-approve", p.id),
                _ => format!("plugin-{}-switch", p.id),
            };
            self.focus_next = Some(key);
        }
    }

    // ---- The Add sheet ------------------------------------------------------------------------------------------------------

    pub fn start_add_json(&mut self) {
        self.plugin_ui.add = Some(AddSheet::blank());
    }

    pub fn close_add_json(&mut self) {
        self.plugin_ui.add = None;
    }

    pub fn add_sheet_mut(&mut self) -> &mut AddSheet {
        self.plugin_ui.add.get_or_insert_with(AddSheet::blank)
    }

    /// Add, in the sheet: the paste goes to the daemon as it is, with the name when the paste had none.
    pub fn submit_add_json(&mut self) {
        let Some(sheet) = &self.plugin_ui.add else { return };
        if !sheet.ready() {
            return;
        }
        let name = if sheet.wants_name() { sheet.given() } else { None };
        let json = sheet.json.text.clone();
        if let Some(id) = self.send(Request::AddMcpJson { json, name }) {
            self.asked.insert(id, Asked::PluginAdd);
            if let Some(s) = &mut self.plugin_ui.add {
                s.sending = true;
                s.error = None;
            }
        }
    }

    /// A folder chosen in the panel.
    pub fn add_plugin_folder(&mut self, folder: String) {
        if let Some(id) = self.send(Request::AddPluginFolder { folder }) {
            self.asked.insert(id, Asked::PluginFolder);
        }
    }

    // ---- One plugin ---------------------------------------------------------------------------------------------------------

    fn ask_plugin(&mut self, plugin: &str, item: Option<String>, did: Did, r: Request) -> bool {
        match self.send(r) {
            Some(id) => {
                self.asked.insert(id, Asked::Plugin { plugin: plugin.to_owned(), item, did });
                self.plugin_ui.busy.insert(plugin.to_owned());
                self.plugin_ui.notes.remove(plugin);
                true
            }
            None => false,
        }
    }

    /// Approve exactly the manifest with `digest` (the one the card showed).
    pub fn approve_plugin(&mut self, plugin: &str, digest: &str) {
        self.ask_plugin(plugin, None, Did::Approve, Request::ApprovePlugin { plugin: plugin.to_owned(), digest: digest.to_owned() });
    }

    pub fn decline_plugin(&mut self, plugin: &str) {
        self.ask_plugin(plugin, None, Did::Decline, Request::DeclinePlugin { plugin: plugin.to_owned() });
    }

    fn need_ask(&self, item: &str) -> Option<PluginAsk> {
        self.needs.iter().find(|i| i.id == item).and_then(|i| if let Need::Plugin { ask } = &i.need { Some(ask.clone()) } else { None })
    }

    /// Approve, in the Needs you list: the digest the card showed. The card goes at once, as an answered request does.
    pub fn approve_need(&mut self, item: &str) {
        let Some(ask) = self.need_ask(item) else { return };
        if self.ask_plugin(&ask.plugin, Some(item.to_owned()), Did::Approve, Request::ApprovePlugin { plugin: ask.plugin.clone(), digest: ask.digest }) {
            self.answered(item);
        }
    }

    pub fn decline_need(&mut self, item: &str) {
        let Some(ask) = self.need_ask(item) else { return };
        if self.ask_plugin(&ask.plugin, Some(item.to_owned()), Did::Decline, Request::DeclinePlugin { plugin: ask.plugin.clone() }) {
            self.answered(item);
        }
    }

    /// "Open in Settings" on a plugin's card in the list.
    pub fn open_need_in_settings(&mut self, item: &str) {
        if let Some(ask) = self.need_ask(item) {
            self.open_plugins(Some(&ask.plugin));
        }
    }

    pub fn set_plugin_on(&mut self, plugin: &str, on: bool) {
        self.ask_plugin(plugin, None, Did::Other, Request::SetPluginEnabled { plugin: plugin.to_owned(), on });
    }

    /// Test: the answer comes up to ten seconds later; the card says "Testing…" meanwhile.
    pub fn test_plugin(&mut self, plugin: &str) {
        if self.plugin_ui.testing.contains(plugin) {
            return;
        }
        if let Some(id) = self.send(Request::TestPlugin { plugin: plugin.to_owned() }) {
            self.asked.insert(id, Asked::PluginTest(plugin.to_owned()));
            self.plugin_ui.testing.insert(plugin.to_owned());
            self.plugin_ui.notes.remove(plugin);
        }
    }

    pub fn set_plugin_value(&mut self, plugin: &str, key: &str, value: Option<String>) {
        self.ask_plugin(plugin, None, Did::Other, Request::SetPluginSetting { plugin: plugin.to_owned(), key: key.to_owned(), value });
    }

    /// The edit of a text or folder setting, created from what is saved when the person first types.
    pub fn plugin_edit_mut(&mut self, plugin: &str, key: &str) -> &mut TextEdit {
        let saved = self.plugin(plugin).and_then(|p| p.manifest.settings.iter().find(|f| f.key == key).and_then(|f| shown_value(p, f))).unwrap_or_default();
        self.plugin_ui.edits.entry((plugin.to_owned(), key.to_owned())).or_insert_with(|| TextEdit::new(&saved))
    }

    /// Save, under a text setting: an empty field clears it.
    pub fn save_plugin_text(&mut self, plugin: &str, key: &str) {
        let Some(edit) = self.plugin_ui.edits.get(&(plugin.to_owned(), key.to_owned())) else { return };
        let words = edit.text.trim().to_owned();
        self.set_plugin_value(plugin, key, Some(words).filter(|w| !w.is_empty()));
    }

    /// The masked field of a secret, empty (Change, or a secret that is not set).
    pub fn plugin_secret_mut(&mut self, plugin: &str, key: &str) -> &mut TextEdit {
        self.plugin_ui.secrets.entry((plugin.to_owned(), key.to_owned())).or_default()
    }

    /// Change, on a secret that is set: an empty masked field, with the keyboard in it.
    pub fn change_plugin_secret(&mut self, plugin: &str, key: &str) {
        let mut edit = TextEdit::default();
        edit.focused = true;
        self.plugin_ui.secrets.insert((plugin.to_owned(), key.to_owned()), edit);
        self.plugin_ui.changing.insert((plugin.to_owned(), key.to_owned()));
        self.focus_next = Some(format!("plugin-{plugin}-{key}-input"));
    }

    /// Set: the typed secret goes to the daemon once and is dropped here at once.
    pub fn save_plugin_secret(&mut self, plugin: &str, key: &str) {
        self.plugin_ui.changing.remove(&(plugin.to_owned(), key.to_owned()));
        let Some(edit) = self.plugin_ui.secrets.remove(&(plugin.to_owned(), key.to_owned())) else { return };
        let value = edit.text.clone();
        if value.is_empty() {
            return;
        }
        self.set_plugin_value(plugin, key, Some(value));
    }

    /// Stops changing a secret without sending anything.
    pub fn cancel_plugin_secret(&mut self, plugin: &str, key: &str) {
        self.plugin_ui.secrets.remove(&(plugin.to_owned(), key.to_owned()));
        self.plugin_ui.changing.remove(&(plugin.to_owned(), key.to_owned()));
    }

    // ---- Which cats ---------------------------------------------------------------------------------------------------------

    pub fn set_access(&mut self, plugin: &str, access: Access) {
        self.ask_plugin(plugin, None, Did::Other, Request::SetPluginAccess { plugin: plugin.to_owned(), access });
    }

    /// The cats that may use the plugin now, by id, among those at work.
    pub fn plugin_cats(&self, p: &Plugin) -> Vec<String> {
        self.active_cats().into_iter().filter(|c| p.access.allows(&c.id)).map(|c| c.id.clone()).collect()
    }

    /// "Every cat" or "Only some cats", in the card's switch.
    pub fn choose_every_cat(&mut self, plugin: &str) {
        self.plugin_ui.only.remove(plugin);
        if self.plugin(plugin).is_some_and(|p| p.access.mode != AccessMode::All) {
            self.set_access(plugin, Access { mode: AccessMode::All, cats: Vec::new() });
        }
    }

    pub fn choose_some_cats(&mut self, plugin: &str) {
        // Nothing is sent: the daemon refuses an empty list. The first cat chosen sends it.
        if self.plugin(plugin).is_some_and(|p| p.access.mode == AccessMode::All) {
            self.plugin_ui.only.insert(plugin.to_owned());
        }
    }

    /// A cat's switch: the list of cats the plugin is for, with this one added or taken out. The last cat cannot be taken out.
    pub fn toggle_plugin_cat(&mut self, plugin: &str, cat: &str) {
        let Some(p) = self.plugin(plugin) else { return };
        let mut chosen = self.plugin_cats(p);
        if p.access.mode == AccessMode::All {
            chosen.clear();
        }
        match chosen.iter().position(|c| c == cat) {
            Some(i) => {
                chosen.remove(i);
            }
            None => chosen.push(cat.to_owned()),
        }
        if chosen.is_empty() {
            return;
        }
        self.plugin_ui.only.remove(plugin);
        self.set_access(plugin, Access { mode: AccessMode::Only, cats: chosen });
    }

    // ---- Removing -----------------------------------------------------------------------------------------------------------

    pub fn ask_remove_plugin(&mut self, plugin: &str) {
        self.confirm = Some(Confirm::RemovePlugin(plugin.to_owned()));
    }

    pub(crate) fn remove_plugin(&mut self, plugin: &str) {
        if let Some(id) = self.send(Request::RemovePlugin { plugin: plugin.to_owned() }) {
            self.asked.insert(id, Asked::PluginRemove(plugin.to_owned()));
            self.plugin_ui.busy.insert(plugin.to_owned());
        }
    }

    // ---- What the daemon says -----------------------------------------------------------------------------------------------

    /// A plugin as the daemon now has it: kept in place, its drafts dropped once they match what is saved.
    pub fn upsert_plugin(&mut self, p: Plugin) {
        let id = p.id.clone();
        self.plugin_ui.edits.retain(|(pid, key), e| {
            pid != &id || p.manifest.settings.iter().find(|f| &f.key == key).is_some_and(|f| shown_value(&p, f).unwrap_or_default() != e.text.trim())
        });
        // A secret now set (or gone) has nothing left to type; one being typed for a field that vanished goes.
        let secret = |key: &String| p.manifest.settings.iter().any(|f| &f.key == key && f.kind == FieldKind::Secret);
        self.plugin_ui.secrets.retain(|(pid, key), _| pid != &id || secret(key));
        self.plugin_ui.changing.retain(|(pid, key)| pid != &id || (secret(key) && p.value(key).is_some_and(|v| v.set)));
        if p.access.mode != AccessMode::All {
            self.plugin_ui.only.remove(&id);
        }
        match self.plugins.iter_mut().find(|x| x.id == id) {
            Some(x) => *x = p,
            None => self.plugins.push(p),
        }
    }

    pub fn drop_plugin(&mut self, id: &str) {
        self.plugins.retain(|p| p.id != id);
        self.plugin_ui.busy.remove(id);
        self.plugin_ui.testing.remove(id);
        self.plugin_ui.notes.remove(id);
        self.plugin_ui.only.remove(id);
        self.plugin_ui.edits.retain(|(p, _), _| p != id);
        self.plugin_ui.secrets.retain(|(p, _), _| p != id);
        self.plugin_ui.changing.retain(|(p, _)| p != id);
        if self.confirm == Some(Confirm::RemovePlugin(id.to_owned())) {
            self.confirm = None;
        }
    }

    /// The link is gone: an answer that will never come must not leave a card saying "Testing…".
    pub fn plugins_lost(&mut self) {
        self.plugin_ui.testing.clear();
        self.plugin_ui.busy.clear();
        if let Some(s) = &mut self.plugin_ui.add {
            s.sending = false;
        }
    }

    /// The reply to a plugin request. Returns the reply when it is not one of these.
    pub(crate) fn plugin_reply(&mut self, asked: &Option<Asked>, reply: neko_protocol::Reply) -> Option<neko_protocol::Reply> {
        use neko_protocol::Reply;
        match (asked, reply) {
            (Some(Asked::PluginAdd), Reply::Plugins { plugins }) => {
                let first = plugins.first().map(|p| p.id.clone());
                let names: Vec<String> = plugins.iter().map(|p| p.name.clone()).collect();
                for p in plugins {
                    self.upsert_plugin(p);
                }
                self.plugin_ui.add = None;
                self.announce = Some((format!("Added {}. It waits for your approval.", list_words(&names)), false));
                self.focus_next = first.map(|id| format!("plugin-{id}-approve"));
                None
            }
            (Some(Asked::PluginAdd), Reply::Error { message, .. }) => {
                if let Some(s) = &mut self.plugin_ui.add {
                    s.sending = false;
                    s.error = Some(message);
                }
                None
            }
            (Some(Asked::PluginFolder), Reply::Plugin { plugin }) => {
                self.announce = Some((format!("Added {}. It waits for your approval.", plugin.name), false));
                self.focus_next = Some(format!("plugin-{}-approve", plugin.id));
                self.upsert_plugin(plugin);
                None
            }
            (Some(Asked::PluginFolder), Reply::Error { message, .. }) => {
                self.refusal = Some(crate::state::Refusal { title: "Neko did not add that folder".into(), words: message, reveal: None });
                None
            }
            (Some(Asked::PluginTest(id)), Reply::Plugin { plugin }) => {
                self.plugin_ui.testing.remove(id);
                self.upsert_plugin(plugin);
                None
            }
            (Some(Asked::PluginTest(id)), Reply::Error { message, .. }) => {
                self.plugin_ui.testing.remove(id);
                self.plugin_ui.notes.insert(id.clone(), message);
                None
            }
            (Some(Asked::Plugin { plugin, did, .. }), Reply::Plugin { plugin: p }) => {
                self.plugin_ui.busy.remove(plugin);
                let name = p.name.clone();
                self.upsert_plugin(p);
                match did {
                    Did::Approve => self.announce = Some((format!("Approved {name}."), false)),
                    Did::Decline => self.announce = Some((format!("Declined {name}."), false)),
                    Did::Other => {}
                }
                None
            }
            (Some(Asked::Plugin { plugin, item, .. }), Reply::Error { message, .. }) => {
                self.plugin_ui.busy.remove(plugin);
                match item {
                    // From the list: the card comes back, with why.
                    Some(item) => {
                        self.needs_hidden.remove(item);
                        self.needs_notice = Some(message);
                    }
                    None => {
                        self.plugin_ui.notes.insert(plugin.clone(), message);
                    }
                }
                None
            }
            (Some(Asked::PluginRemove(id)), Reply::Ok) => {
                self.plugin_ui.busy.remove(id);
                None
            }
            (Some(Asked::PluginRemove(id)), Reply::Error { message, .. }) => {
                self.plugin_ui.busy.remove(id);
                self.refusal = Some(crate::state::Refusal { title: "Neko kept this plugin".into(), words: message, reveal: None });
                None
            }
            (None, Reply::Plugins { plugins }) => {
                self.plugins.clear();
                for p in plugins {
                    self.upsert_plugin(p);
                }
                None
            }
            (_, other) => Some(other),
        }
    }
}

/// The item's `workspace` is empty for a plugin: the Needs you list and the Briefing group such items under "Plugins".
pub fn is_plugin_item(item: &NeedsItem) -> bool {
    matches!(item.need, Need::Plugin { .. })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_read_as_words() {
        let v = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(list_words(&v(&[])), "");
        assert_eq!(list_words(&v(&["A"])), "A");
        assert_eq!(list_words(&v(&["A", "B"])), "A and B");
        assert_eq!(list_words(&v(&["A", "B", "C"])), "A, B and C");
        assert_eq!(passes_words(false, &v(&["ACME_TOKEN"])).as_deref(), Some("Passes ACME_TOKEN"));
        assert_eq!(passes_words(true, &v(&["Authorization", "X-Team"])).as_deref(), Some("Sends the headers Authorization and X-Team"));
        assert_eq!(passes_words(false, &[]), None);
    }

    #[test]
    fn an_address_shows_its_host_and_nothing_a_token_could_hide_in() {
        assert_eq!(host_of("https://mcp.example.com/v1/sse?key=abc123"), "mcp.example.com");
        assert_eq!(host_of("https://user:pw@example.com:8443/x"), "example.com:8443");
        assert_eq!(host_of("example.com/path"), "example.com");
    }

    #[test]
    fn a_paste_is_read_live_with_the_daemons_code_and_never_echoes_a_value() {
        let mut sheet = AddSheet::blank();
        assert_eq!(sheet.reading(), Reading::Empty);
        assert!(!sheet.ready());
        sheet.json = TextEdit::new(r#"{"command":"npx","args":["-y","@acme/docs-server"],"env":{"ACME_TOKEN":"sk-very-secret-value"}}"#);
        assert!(sheet.wants_name(), "one server with no name offers the Name field");
        let Reading::Found(found) = sheet.reading() else { panic!("{:?}", sheet.reading()) };
        assert_eq!(found.len(), 1);
        let said = found_words(&found[0]);
        assert_eq!(said, "Will add acme-docs-server: runs `npx -y @acme/docs-server`, passes ACME_TOKEN");
        assert!(!said.contains("sk-very-secret-value"), "{said}");
        assert!(sheet.ready());
        sheet.name = TextEdit::new("Acme docs");
        let Reading::Found(named) = sheet.reading() else { panic!() };
        assert_eq!(named[0].manifest.name, "Acme-docs");
        sheet.json = TextEdit::new(r#"{"mcpServers":{"a":{"command":"node","args":["a.js"]},"b":{"url":"https://example.com/mcp"}}}"#);
        assert!(!sheet.wants_name(), "a block of servers names its own");
        let Reading::Found(two) = sheet.reading() else { panic!() };
        assert_eq!(two.len(), 2);
        assert_eq!(found_words(&two[1]), "Will add b: connects to https://example.com/mcp");
        sheet.json = TextEdit::new(r#"{"command": "node", "args": ["a.js""#);
        match sheet.reading() {
            Reading::Bad(w) => assert!(w.contains("line") && w.contains("column"), "{w}"),
            other => panic!("{other:?}"),
        }
        assert!(!sheet.ready(), "Add stays off until it reads");
    }

    #[test]
    fn a_plugins_ask_is_one_sentence_with_the_command_and_names_only() {
        let ask = PluginAsk {
            plugin: "plugin_1".into(),
            name: "acme-docs".into(),
            version: "0.3.1".into(),
            digest: "sha256:x".into(),
            runs: "node server.js".into(),
            remote: false,
            passes: vec!["ACME_TOKEN".into()],
            changed: false,
            changes: vec![],
            description: None,
            folder: None,
        };
        assert_eq!(ask_sentence(&ask), "Approve plugin: acme-docs. It will run node server.js with ACME_TOKEN");
        assert_eq!(note_words(&ask), "Approve the plugin acme-docs");
        let changed = PluginAsk { changed: true, changes: vec!["Runs: node a.js to node b.js".into()], ..ask.clone() };
        assert_eq!(ask_sentence(&changed), "acme-docs changed since you approved it: Runs: node a.js to node b.js");
        assert_eq!(note_words(&changed), "acme-docs changed: Runs: node a.js to node b.js");
        let remote = PluginAsk { remote: true, runs: "https://example.com/mcp".into(), passes: vec!["Authorization".into()], ..ask };
        assert_eq!(ask_sentence(&remote), "Approve plugin: acme-docs. It will connect to https://example.com/mcp with the header Authorization");
    }

    #[test]
    fn a_check_says_how_many_tools_and_which() {
        let zone = neko_protocol::schedule::Fixed(0);
        let now = 1_795_701_900_000;
        let ok = Check { at_ms: now - 60_000, ok: true, server: Some("fake".into()), version: None, tools: vec!["search".into(), "fetch".into()], error: None };
        let (head, tools, when) = check_words(&ok, now, &zone);
        assert_eq!((head.as_str(), tools.as_deref(), when.as_str()), ("Connected: 2 tools", Some("search and fetch"), "today at 14:04"));
        let many = Check { tools: (1..=12).map(|i| format!("t{i}")).collect(), ..ok.clone() };
        assert_eq!(check_words(&many, now, &zone).1.as_deref(), Some("t1, t2, t3, t4, t5, t6, t7, t8 and 4 more"));
        let bad = Check { ok: false, tools: vec![], error: Some("It did not answer in 10 seconds.".into()), ..ok };
        assert_eq!(check_words(&bad, now, &zone).0, "The test failed: It did not answer in 10 seconds.");
    }
}
