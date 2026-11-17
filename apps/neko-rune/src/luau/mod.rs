//! Native plugins: Luau cards and views that installed, approved packs ship (`[ui] luau = [...]` in `pack.toml`),
//! run by `rune-luau` in a sandbox (docs/knowledge/luau-plugins.md; docs/decisions/0002-rune-for-everything.md, plugin
//! slots (b) and (d)).
//!
//! **Where Luau comes from.** Only from the daemon's `PackUi { name }` read, which answers only while the pack is
//! Healthy and its files are exactly what the person approved (the daemon's pack code, `ui_sources`). Rune never reads a pack
//! folder itself. [`sync`](App::luau_sync) asks for the pack list and then each healthy pack's files every
//! [`SYNC_EVERY`] seconds and after any pack answer; a changed file is reloaded in place with its state kept, a pack
//! that is no longer healthy or unchanged is unloaded at once.
//!
//! **What a file is.** One table: `slot` (`card` or `view`), `id`, `title`, an optional `icon`, `render(state, host)` and
//! an optional `init(host)` that returns the first state ([`ADAPT`] wraps it for `rune-luau`, whose own contract is
//! `render(view)`). **What it may do** is `host` (`host.rs`): `read` (the fixed list in `link/luau.rs`, four a second),
//! `ask` (drafts into Neko's ask box, never sends), `open` (a route Rune already has), `storage`, `now`; and the
//! nodes `ui.input/button/image/glyph` besides stock `@rune/ui`, all drawn with Neko's own colours (`nodes.rs`).
//! **How it is held**: 250 ms an entry, 64 MB, no io/os/net, `require` inside the listed files only (rune-luau's own
//! sandbox); a render that takes longer than [`SLOW_RENDER_MS`] stops the plugin like an error does, so a slow plugin
//! cannot make the window stutter. A plugin that errors or is killed shows one quiet line, "<title> stopped: <reason>",
//! with Reload, and the rest of the app carries on.

pub mod host;
pub mod nodes;

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use neko_protocol::PluginPage;
use rune::{TextEdit, Tx, Waker};
use rune_luau::{ErrorKind, LogLevel, Script, Sources};

use crate::app::App;
use crate::link::luau::{PackSync, Wire};

/// Seconds between asking the daemon what each pack's panels are.
pub const SYNC_EVERY: f64 = 30.0;
/// The cards the Neko column shows before "and N more".
pub const MAX_CARDS: usize = 3;
/// A card's header, and the tallest its body grows before it scrolls.
pub const CARD_HEAD: f32 = 32.0;
pub const CARD_BODY_MAX: f32 = 160.0;
/// A render longer than this stops the plugin (the watchdog's 250 ms is for runaway code; this is for a UI that must stay smooth).
pub const SLOW_RENDER_MS: u128 = 60;
/// The share of every second a plugin may spend running its own code (renders, handlers, tasks), over two seconds in a row.
/// The 250 ms watchdog bounds one entry; this bounds a plugin that keeps starting them.
pub const CPU_SHARE: f64 = 0.5;
/// The page id of a view is `luau_<pack>:<view id>`, a plugin-page route Rune and `--open` already understand.
pub const PAGE_PREFIX: &str = "luau_";

/// Where a plugin's panel goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Card,
    View,
}

/// What a plugin's file says about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    pub slot: Slot,
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
}

/// `[a-z0-9-]`, 1 to 40 characters.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// One plugin file, running (or stopped).
pub struct Plugin {
    pub pack: String,
    pub path: String,
    /// This file and the files it requires, entered at this file: a change in any of them reloads the script.
    pub sources: Sources,
    pub script: Script,
    /// Filled by the start hook once the file's table was read and checked.
    pub meta: Rc<RefCell<Option<Meta>>>,
    /// The last error line (any `LogLevel::Error`, a slow render): the plugin is stopped until Reload.
    pub failure: Rc<RefCell<Option<String>>>,
    /// An `ui.input` was built: this plugin is rendered every frame, never from a memo, so a stale snapshot cannot undo typing.
    pub has_input: Rc<Cell<bool>>,
    /// The clock second a one-second window of its CPU time began at, what its VM had spent then, and how many windows in a row went over.
    pub busy: Cell<Option<(f64, std::time::Duration, u8)>>,
}

impl Plugin {
    pub fn key(&self) -> String {
        plugin_key(&self.pack, &self.path)
    }

    pub fn meta(&self) -> Option<Meta> {
        self.meta.borrow().clone()
    }

    /// The title the person sees: the file's, else its file name.
    pub fn title(&self) -> String {
        self.meta().map_or_else(|| self.path.rsplit('/').next().unwrap_or(&self.path).trim_end_matches(".luau").to_owned(), |m| m.title)
    }

    /// Why it is not drawing, in a few words, or none when it runs.
    pub fn stopped(&self) -> Option<String> {
        if let Some(line) = self.failure.borrow().as_ref() {
            return Some(reason(line));
        }
        self.script.error().map(|e| match e.kind {
            ErrorKind::Timeout => reason("watchdog"),
            ErrorKind::Memory => reason("not enough memory"),
            _ => reason(e.summary()),
        })
    }

    /// Called each tick: a plugin that spends more than [`CPU_SHARE`] of two seconds running its own code is stopped. The watchdog bounds
    /// one entry; this bounds a task that starts a long one every frame.
    pub fn watch_cpu(&self, now: f64) {
        let spent = self.script.metrics().vm_time;
        match self.busy.get() {
            None => self.busy.set(Some((now, spent, 0))),
            Some((since, was, strikes)) if now - since >= 1.0 => {
                let share = spent.saturating_sub(was).as_secs_f64() / (now - since);
                let strikes = if share > CPU_SHARE { strikes + 1 } else { 0 };
                self.busy.set(Some((now, spent, strikes)));
                if strikes >= 2 && self.failure.borrow().is_none() {
                    *self.failure.borrow_mut() = Some(format!("it uses too much time (over {} ms of every second)", (CPU_SHARE * 1000.0) as u32));
                }
            }
            Some(_) => {}
        }
    }

    /// The pack's and the plugin's address: a card's menu and a view's route use it.
    pub fn page(&self) -> Option<PluginPage> {
        let meta = self.meta()?;
        (meta.slot == Slot::View).then(|| PluginPage { plugin: format!("{PAGE_PREFIX}{}", self.pack), id: format!("{PAGE_PREFIX}{}:{}", self.pack, meta.id), title: meta.title })
    }
}

/// The `Sources` of one plugin file: the file itself (the entry) and every listed file it names in a `require` (and what those
/// require, and so on), by its file name or its path without `.luau`. A file nothing asks for is not in the set, so it cannot
/// break the others.
pub fn sources_for(path: &str, files: &[neko_protocol::packs::PackUiFile]) -> Sources {
    let mut wanted: Vec<&neko_protocol::packs::PackUiFile> = Vec::new();
    let mut queue: Vec<&neko_protocol::packs::PackUiFile> = files.iter().filter(|f| f.path == path).collect();
    while let Some(file) = queue.pop() {
        if wanted.iter().any(|w| w.path == file.path) {
            continue;
        }
        wanted.push(file);
        for other in files {
            let stem = other.path.trim_end_matches(".luau");
            let name = stem.rsplit('/').next().unwrap_or(stem);
            if other.path != file.path && file.source.contains("require") && (file.source.contains(&format!("/{name}\"")) || file.source.contains(&format!("\"{name}\"")) || file.source.contains(stem)) {
                queue.push(other);
            }
        }
    }
    let mut sources = Sources::new();
    for f in wanted {
        sources.insert(&f.path, f.source.as_str());
    }
    sources.entry(path)
}

pub fn plugin_key(pack: &str, path: &str) -> String {
    format!("{pack}/{path}")
}

/// An error line in words for the person: the watchdog and the heap cap by name, anything else its first line, cut short.
pub fn reason(message: &str) -> String {
    let first = message.lines().next().unwrap_or("").trim();
    let first = first.strip_prefix("runtime error: ").unwrap_or(first);
    let lower = message.to_lowercase();
    if lower.contains("watchdog") {
        return "it ran longer than 250 ms".to_owned();
    }
    if lower.contains("not enough memory") || lower.contains("memory limit") {
        return "it used more than 64 MB".to_owned();
    }
    let mut line: String = first.chars().take(140).collect();
    if first.chars().count() > 140 {
        line.push('…');
    }
    if line.is_empty() { "it stopped".to_owned() } else { line }
}

/// What the Setup and Plugins pages say about one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileStatus {
    Loaded,
    Stopped(String),
}

/// One file of a pack as the pages list it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRow {
    pub pack: String,
    pub path: String,
    pub title: String,
    pub slot: Option<Slot>,
    pub status: FileStatus,
}

/// What the last sync said about a pack that declares Luau files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackNote {
    pub title: String,
    pub declared: Vec<String>,
    /// Why none of its panels run (not healthy, changed on disk): the daemon's words.
    pub problem: Option<String>,
}

/// Everything the window keeps about Luau plugins.
#[derive(Default)]
pub struct Luau {
    pub plugins: Vec<Plugin>,
    pub packs: BTreeMap<String, PackNote>,
    /// The text of each `ui.input`, keyed by (plugin, the input's key).
    pub inputs: HashMap<(String, String), TextEdit>,
    /// What each plugin was last told about each input.
    pub told: HashMap<(String, String), String>,
    pub waker: Option<Waker>,
    /// Cards the person hid: `<pack>/<card id>`.
    pub hidden: BTreeSet<String>,
    /// The card whose menu is open, and where.
    pub menu: Option<(String, rune::PointerPress)>,
    /// A sync is on its way.
    pub in_flight: bool,
    /// The clock second the next sync is due at (0: at once).
    pub next_sync: f64,
    /// A fixed wire for tests and scenes; the live one is made from the dispatch otherwise.
    pub wire: Option<Wire>,
    /// The live wire, kept while the socket and the signed address stay the same (its cache of heavy reads lives in it).
    pub live: Option<(std::path::PathBuf, Option<crate::link::http::Http>, Wire)>,
    /// The daemon could not be asked: the last line, kept quietly.
    pub last_problem: Option<String>,
    /// Persist the hidden list (a real window only).
    pub persist: bool,
    /// The clock second of the last tick: tests and scenes carry on from it.
    pub clock: f64,
}

impl Luau {
    /// Plugins in the order cards and lists show them: pack, then the order the pack lists its files.
    pub fn plugin(&self, key: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.key() == key)
    }

    /// The cards that are not hidden, in order.
    pub fn cards(&self) -> Vec<&Plugin> {
        self.plugins.iter().filter(|p| p.meta().is_some_and(|m| m.slot == Slot::Card && !self.hidden.contains(&format!("{}/{}", p.pack, m.id)))).collect()
    }

    /// The views, in order.
    pub fn views(&self) -> Vec<&Plugin> {
        self.plugins.iter().filter(|p| p.meta().is_some_and(|m| m.slot == Slot::View)).collect()
    }

    /// The view a plugin page names (`luau_<pack>:<id>`).
    pub fn view_for(&self, page: &PluginPage) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.page().is_some_and(|q| q.id == page.id))
    }

    /// Every file of every pack with Luau, as the pages list them.
    pub fn rows(&self) -> Vec<FileRow> {
        let mut rows = Vec::new();
        for (pack, note) in &self.packs {
            for path in &note.declared {
                let row = match self.plugin(&plugin_key(pack, path)) {
                    Some(p) => FileRow { pack: pack.clone(), path: path.clone(), title: p.title(), slot: p.meta().map(|m| m.slot), status: p.stopped().map_or(FileStatus::Loaded, FileStatus::Stopped) },
                    None => FileRow { pack: pack.clone(), path: path.clone(), title: path.rsplit('/').next().unwrap_or(path).trim_end_matches(".luau").to_owned(), slot: None, status: FileStatus::Stopped(note.problem.clone().unwrap_or_else(|| "it is not loaded".to_owned())) },
                };
                rows.push(row);
            }
        }
        rows
    }

    /// What the next tick should ask for sooner: a pack answered (installed, updated, approved).
    pub fn resync(&mut self) {
        self.next_sync = 0.0;
    }

    pub fn is_hidden(&self, pack: &str, id: &str) -> bool {
        self.hidden.contains(&format!("{pack}/{id}"))
    }
}

/// True for the page of a Luau view.
pub fn is_luau_page(page: &PluginPage) -> bool {
    page.plugin.starts_with(PAGE_PREFIX)
}

thread_local! {
    /// What a plugin asked of the window while it ran: applied by `App::luau_apply` after the entry.
    static ACTIONS: RefCell<Vec<Action>> = const { RefCell::new(Vec::new()) };
}

/// What `host.ask` and `host.open` ask of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Draft these words into Neko's ask box.
    Ask(String),
    /// Go to this route.
    Open(String),
}

pub(crate) fn push_action(action: Action) {
    ACTIONS.with(|a| {
        let mut a = a.borrow_mut();
        if a.len() < 16 {
            a.push(action);
        }
    });
}

pub(crate) fn take_actions() -> Vec<Action> {
    ACTIONS.with(|a| std::mem::take(&mut *a.borrow_mut()))
}

/// The wrapper that gives `rune-luau`'s `render(view)` contract the plugin's `render(state, host)` and `init(host)`.
/// `state` is one table the plugin mutates: a hot reload copies the saved one *into* it, so tasks `init` started keep working.
pub const ADAPT: &str = r#"
local module, host = ...
local state = nil
if type(module.init) == "function" then state = module.init(host) end
if type(state) ~= "table" then state = {} end
local render = module.render
module.render = function(view) return render(state, host) end
-- Hot state is a copy with a budget and no cycles: the library walks the returned table in Rust, which no watchdog can interrupt.
local function clean(v, depth, path, budget)
    local kind = type(v)
    if kind == "table" then
        if depth > 8 or path[v] or budget.n <= 0 then return nil end
        path[v] = true
        local out = {}
        for k, x in next, v do
            budget.n -= 1
            if budget.n <= 0 then break end
            local key = type(k)
            if key == "string" or key == "number" then out[k] = clean(x, depth + 1, path, budget) end
        end
        path[v] = nil
        return out
    elseif kind == "string" or kind == "number" or kind == "boolean" then
        return v
    end
    return nil
end
module.saveHotState = function() return clean(state, 0, {}, { n = 20000 }) or {} end
module.loadHotState = function(saved)
    if type(saved) ~= "table" then return end
    for k in state do state[k] = nil end
    for k, v in saved do state[k] = v end
end
"#;

impl App {
    /// The wire plugins use: a fixed one (tests), else the daemon's socket and the signed address, as the window has them now.
    pub fn luau_wire(&mut self) -> Wire {
        if let Some(wire) = &self.luau.wire {
            return wire.clone();
        }
        let http = if self.luau.plugins.is_empty() { self.http.clone() } else { self.http() };
        let socket = match &self.dispatch {
            crate::app::Dispatch::Async { socket, .. } | crate::app::Dispatch::Blocking { socket } => socket.clone(),
            crate::app::Dispatch::None => return Wire::none(),
        };
        if let Some((s, h, wire)) = &self.luau.live
            && *s == socket
            && *h == http
        {
            return wire.clone();
        }
        let wire = Wire::socket(socket.clone(), http.clone());
        self.luau.live = Some((socket, http, wire.clone()));
        wire
    }

    /// Whether there is anything to ask the daemon with: a fixed wire (tests, scenes) or the window's link.
    fn luau_can_ask(&self) -> bool {
        self.luau.wire.is_some() || !matches!(self.dispatch, crate::app::Dispatch::None)
    }

    /// Asks the daemon for each pack's panels: on a thread, answered through the window's handle.
    pub fn luau_sync(&mut self) {
        let wire = self.luau_wire();
        match self.dispatch.clone() {
            crate::app::Dispatch::Async { handle, .. } if self.luau.wire.is_none() => {
                self.luau.in_flight = true;
                let spawned = std::thread::Builder::new().name("neko-luau-sync".into()).spawn(move || {
                    let result = crate::link::luau::sync(&wire);
                    handle.update_if(move |s, tx| {
                        tx.instant();
                        s.luau_synced(result, Some(tx));
                        true
                    });
                });
                if spawned.is_err() {
                    self.luau.in_flight = false;
                }
            }
            _ => {
                let result = crate::link::luau::sync(&wire);
                self.luau_synced(result, None);
            }
        }
    }

    /// What the daemon said about each pack with Luau files: starts, reloads or stops the scripts to match.
    pub fn luau_synced(&mut self, result: Result<Vec<PackSync>, String>, tx: Option<&mut Tx>) -> bool {
        self.luau.in_flight = false;
        let packs = match result {
            Ok(packs) => {
                self.luau.last_problem = None;
                packs
            }
            Err(problem) => {
                // The daemon could not be asked: what runs keeps running (it was approved when it started). Try again soon.
                self.luau.last_problem = Some(problem);
                self.luau.next_sync = self.luau.clock + 5.0;
                return false;
            }
        };
        let mut changed = false;
        // A plugin's `init` may start a read at once: the wire it uses is the window's now.
        host::publish_wire(self.luau_wire());
        let names: BTreeSet<String> = packs.iter().map(|p| p.name.clone()).collect();
        // Packs that are gone, or no longer declare Luau.
        let before = self.luau.plugins.len();
        self.luau.plugins.retain(|p| names.contains(&p.pack));
        changed |= self.luau.plugins.len() != before;
        let had = self.luau.packs.len();
        self.luau.packs.retain(|name, _| names.contains(name));
        changed |= self.luau.packs.len() != had;
        for pack in packs {
            let note = PackNote { title: pack.title.clone(), declared: pack.declared.clone(), problem: pack.outcome.as_ref().err().cloned() };
            if self.luau.packs.get(&pack.name) != Some(&note) {
                self.luau.packs.insert(pack.name.clone(), note);
                changed = true;
            }
            match pack.outcome {
                // Not healthy, or the files differ from what was approved: nothing of it runs.
                Err(_) => {
                    let before = self.luau.plugins.len();
                    self.luau.plugins.retain(|p| p.pack != pack.name);
                    changed |= self.luau.plugins.len() != before;
                }
                Ok(files) => changed |= self.luau_load_pack(&pack.name, &pack.declared, files),
            }
        }
        self.luau_drop_orphans();
        if changed && let Some(tx) = tx {
            tx.instant();
        }
        changed
    }

    /// Starts, reloads or keeps each listed file of one pack. A file gets its own `Sources`: itself and the files it asks for with
    /// `require` (and theirs), so a file that does not compile stops only the plugins that use it.
    fn luau_load_pack(&mut self, pack: &str, declared: &[String], files: Vec<neko_protocol::packs::PackUiFile>) -> bool {
        let mut changed = false;
        let mut kept: Vec<Plugin> = Vec::new();
        let mut old: Vec<Plugin> = std::mem::take(&mut self.luau.plugins);
        for path in declared {
            if !files.iter().any(|f| &f.path == path) {
                continue;
            }
            let sources = sources_for(path, &files);
            let at = old.iter().position(|p| p.pack == pack && &p.path == path);
            match at.map(|i| old.remove(i)) {
                Some(mut plugin) => {
                    if plugin.sources != sources {
                        // The state comes across; a plugin that was stopped is given a clean start. A file that does not
                        // compile is logged by `reload`: the error line shows, and the running copy stays under it.
                        if plugin.script.reload(sources.clone()).is_ok() {
                            *plugin.failure.borrow_mut() = None;
                        }
                        plugin.sources = sources;
                        changed = true;
                    }
                    kept.push(plugin);
                }
                None => {
                    kept.push(self.luau_start(pack, path, sources));
                    changed = true;
                }
            }
        }
        // Plugins of this pack no longer listed are dropped; other packs' are put back as they were.
        changed |= old.iter().any(|p| p.pack == pack);
        old.retain(|p| p.pack != pack);
        old.extend(kept);
        self.luau.plugins = old;
        self.luau_order();
        changed
    }

    /// Keeps each pack's plugins in the order its manifest lists them.
    fn luau_order(&mut self) {
        let order: BTreeMap<String, Vec<String>> = self.luau.packs.iter().map(|(k, v)| (k.clone(), v.declared.clone())).collect();
        self.luau.plugins.sort_by_key(|p| (p.pack.clone(), order.get(&p.pack).and_then(|d| d.iter().position(|x| x == &p.path)).unwrap_or(usize::MAX)));
    }

    fn luau_drop_orphans(&mut self) {
        let keys: BTreeSet<String> = self.luau.plugins.iter().map(Plugin::key).collect();
        self.luau.inputs.retain(|(plugin, _), _| keys.contains(plugin));
        self.luau.told.retain(|(plugin, _), _| keys.contains(plugin));
    }

    /// Starts one plugin file from its sources.
    fn luau_start(&mut self, pack: &str, path: &str, sources: Sources) -> Plugin {
        let meta: Rc<RefCell<Option<Meta>>> = Rc::default();
        let failure: Rc<RefCell<Option<String>>> = Rc::default();
        let has_input: Rc<Cell<bool>> = Rc::default();
        let ctx = host::Context { pack: pack.to_owned(), key: plugin_key(pack, path), meta: meta.clone(), failure: failure.clone(), has_input: has_input.clone() };
        let script = Script::with_options(&plugin_key(pack, path), sources.clone(), host::options(ctx));
        if let Some(w) = &self.luau.waker {
            script.set_waker(w.clone());
        }
        Plugin { pack: pack.to_owned(), path: path.to_owned(), sources, script, meta, failure, has_input, busy: Cell::new(None) }
    }

    /// Reload: a clean start from the same files (its stored data stays; its state is made again by `init`).
    pub fn luau_reload(&mut self, key: &str) {
        let Some(at) = self.luau.plugins.iter().position(|p| p.key() == key) else { return };
        let (pack, path, sources) = {
            let p = &self.luau.plugins[at];
            (p.pack.clone(), p.path.clone(), p.sources.clone())
        };
        self.luau.inputs.retain(|(plugin, _), _| plugin != key);
        self.luau.told.retain(|(plugin, _), _| plugin != key);
        nodes::forget_plugin(key);
        let fresh = self.luau_start(&pack, &path, sources);
        self.luau.plugins[at] = fresh;
    }

    /// Reload every plugin of a pack (the pack's detail page offers it), and ask again for its files.
    pub fn luau_reload_pack(&mut self, pack: &str) {
        let keys: Vec<String> = self.luau.plugins.iter().filter(|p| p.pack == pack).map(Plugin::key).collect();
        for key in keys {
            self.luau_reload(&key);
        }
        self.luau.resync();
    }

    pub fn luau_hide(&mut self, pack: &str, id: &str) {
        self.luau.hidden.insert(format!("{pack}/{id}"));
        self.luau.menu = None;
        self.luau_save_hidden();
    }

    pub fn luau_show_all(&mut self) {
        self.luau.hidden.clear();
        self.luau_save_hidden();
    }

    fn luau_save_hidden(&self) {
        if !self.luau.persist {
            return;
        }
        crate::app::write_luau_hidden(self.luau.hidden.iter().cloned().collect());
    }

    /// The hidden list the last run saved.
    pub fn luau_load_hidden() -> BTreeSet<String> {
        crate::app::load_luau_hidden().into_iter().collect()
    }

    /// Setup, Packs, on one pack's page.
    pub fn open_pack_named(&mut self, name: &str) {
        self.open_setup(crate::view::setup::SetupSection::Packs);
        self.open_pack(name);
    }

    /// Opens a view's page in the middle region.
    pub fn open_luau_view(&mut self, page: PluginPage) {
        self.open_plugin_page(page);
    }

    /// What `host.ask` and `host.open` asked of the window, applied in the same turn as the plugin's entry. `host.ask` drafts: it
    /// opens Neko's column, puts the words after what the person has written and gives the box the keyboard; nothing is sent.
    pub fn luau_apply(&mut self, mut tx: Option<&mut Tx>) -> bool {
        let actions = take_actions();
        let mut did = false;
        for action in actions {
            did = true;
            match action {
                Action::Ask(text) => {
                    self.layout.set_neko_open(true);
                    self.into_composer(crate::compose::Dest::Ask, &text);
                    self.ask.focused = true;
                    if let Some(tx) = tx.as_deref_mut() {
                        tx.focus("ask", true);
                    }
                }
                Action::Open(route) => {
                    if self.route_known(&route) {
                        self.open_route(&route);
                    } else {
                        self.show_toast("A plugin asked for a place Neko doesn’t have.");
                    }
                }
            }
        }
        did
    }

    /// A route Rune opens itself (never a bounce to the browser).
    pub fn route_known(&self, route: &str) -> bool {
        let Some(route) = crate::deeplink::clean_route(route) else { return false };
        let path = route.split('?').next().unwrap_or(&route);
        matches!(path, "tasks" | "artifact" | "setup" | "settings" | "pull-requests" | "home" | "tools")
            || path.starts_with("setup/")
            || crate::views::FullView::ALL.into_iter().any(|v| v.web_id() == path)
            || self.plugin_pages.as_ref().is_some_and(|pages| pages.iter().any(|p| p.id == path))
            || self.luau.views().iter().any(|p| p.page().is_some_and(|q| q.id == path))
    }

    /// Once a frame (the window's poll): publishes the wire, asks for the packs' panels when due, lets each script run what is
    /// due (finished reads, `task.wait`), tells a plugin its `ui.input` changed, and applies what plugins asked of the window.
    pub fn luau_tick(&mut self, now: f64, tx: &mut Tx) -> bool {
        let mut changed = false;
        self.luau.clock = now;
        if self.luau.plugins.is_empty() && self.luau.packs.is_empty() && self.luau.next_sync > now {
            return self.luau_apply(Some(tx));
        }
        host::publish_wire(self.luau_wire());
        if now >= self.luau.next_sync && !self.luau.in_flight && self.luau_can_ask() {
            self.luau.next_sync = now + SYNC_EVERY;
            self.luau_sync();
            changed = true;
        }
        for (key, value) in nodes::take_set_values() {
            let told = (key.0.clone(), key.1.clone());
            let edit = self.luau.inputs.entry(key).or_insert_with(|| TextEdit::new(""));
            if edit.text != value {
                edit.replace_text(value.clone());
            }
            self.luau.told.insert(told, value);
            changed = true;
        }
        for p in &self.luau.plugins {
            // A stopped plugin is not run: its tasks wait until Reload.
            if p.stopped().is_some() {
                continue;
            }
            changed |= p.script.poll(now, tx);
            p.watch_cpu(now);
        }
        // `ui.input`: what the person typed since a plugin was last told is its `on_change`.
        let mut told: Vec<(usize, String, String)> = Vec::new();
        for ((plugin, key), edit) in &self.luau.inputs {
            let seen = self.luau.told.get(&(plugin.clone(), key.clone()));
            if seen.map(String::as_str) != Some(edit.text.as_str())
                && let Some(at) = self.luau.plugins.iter().position(|p| &p.key() == plugin)
            {
                told.push((at, key.clone(), edit.text.clone()));
            }
        }
        for (at, key, text) in told {
            let plugin = self.luau.plugins[at].key();
            self.luau.told.insert((plugin, key.clone()), text.clone());
            if self.luau.plugins[at].stopped().is_none() {
                let _ = self.luau.plugins[at].script.call("__neko_change", [rune_luau::Data::String(key), rune_luau::Data::String(text)]);
                changed = true;
            }
        }
        changed |= self.luau_apply(Some(tx));
        changed
    }

}

/// Runs ticks until every plugin has been quiet for a while (finished reads come back on other threads): for tests and scenes,
/// where nothing else drives the clock. The clock it hands the scripts starts at one second and moves five milliseconds a tick.
#[doc(hidden)]
pub fn settle(app: &mut App) {
    let mut tx = Tx::default();
    let mut now = app.luau.clock.max(1.0);
    let mut quiet = 0;
    let calls = |app: &App| app.luau.plugins.iter().map(|p| p.script.metrics().vm_calls).sum::<u64>();
    for _ in 0..800 {
        std::thread::sleep(std::time::Duration::from_millis(5));
        now += 0.005;
        let before = calls(app);
        app.luau_tick(now, &mut tx);
        quiet = if calls(app) == before { quiet + 1 } else { 0 };
        if quiet >= 24 {
            break;
        }
    }
}

/// Starts the given files as `pack` over a fixed wire, as a sync would, and lets their first reads land (tests and scenes).
#[doc(hidden)]
pub fn install_for_tests(app: &mut App, pack: &str, files: &[(&str, &str)], wire: Wire) {
    app.luau.wire = Some(wire);
    let declared = files.iter().map(|(p, _)| (*p).to_owned()).collect();
    let files = files.iter().map(|(path, source)| neko_protocol::packs::PackUiFile { path: (*path).to_owned(), source: (*source).to_owned() }).collect();
    app.luau_synced(Ok(vec![PackSync { name: pack.to_owned(), title: pack.to_owned(), declared, outcome: Ok(files) }]), None);
    app.luau.next_sync = f64::MAX;
    settle(app);
}

/// Logs from a plugin that mean it stopped: any error line. The text is kept for the quiet line.
pub fn note_failure(failure: &Rc<RefCell<Option<String>>>, level: LogLevel, line: &str) {
    if level == LogLevel::Error {
        let mut f = failure.borrow_mut();
        if f.is_none() {
            *f = Some(line.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_lower_case_digits_and_dashes() {
        assert!(valid_id("needs-you-count") && valid_id("a1"));
        assert!(!valid_id("") && !valid_id("Needs") && !valid_id("a_b") && !valid_id("a b") && !valid_id(&"a".repeat(41)));
    }

    #[test]
    fn a_stopped_line_names_the_watchdog_and_the_heap_in_words() {
        assert_eq!(reason("script ran longer than its time limit (watchdog)"), "it ran longer than 250 ms");
        assert_eq!(reason("not enough memory"), "it used more than 64 MB");
        assert_eq!(reason("ui/card.luau:4: attempt to index nil\nstack traceback:"), "ui/card.luau:4: attempt to index nil");
        assert_eq!(reason("runtime error: ui/card.luau:4: attempt to index nil"), "ui/card.luau:4: attempt to index nil", "the library's own prefix is noise");
        assert!(reason(&"x".repeat(300)).chars().count() <= 141);
        assert_eq!(reason(""), "it stopped");
    }
}
