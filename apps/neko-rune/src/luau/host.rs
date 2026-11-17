//! What a Luau plugin is given: the `host` table, the sandbox's limits and the Neko palette (docs/knowledge/luau-plugins.md).
//!
//! `options` builds the `ScriptOptions` of one plugin file. The sandbox itself is `rune-luau`'s: a 250 ms watchdog on every
//! entry, a 64 MB heap, no `io`, `os` (but `os.time`), network or files, `require` inside the pack's listed files only. This
//! adds: the `host` table (`read`, `ask`, `open`, `storage`, `now`), the stock nodes made safe (no raw colours), the
//! host nodes (`nodes.rs`), the start hook that reads the file's table and wraps its `render` (`ADAPT`), and a log hook
//! that stops the plugin on an error line.
//!
//! * **`host.read(name, args)`** runs on a worker thread (`Host::async_function`) and answers a table, or `nil, err`.
//!   `name` must be in `link::luau::READS`; anything else is refused in Lua's own words before anything is sent.
//!   At most four a second per plugin; the fifth in a second is `nil, "rate limited"`.
//! * **`host.storage.get/set`** is the pack's own JSON in the daemon's plugin storage, 256 KB per pack.
//! * **`host.ask(text)`** drafts into Neko's ask box; **`host.open(route)`** goes to a route Rune has. Both only queue
//!   what the window then does (`App::luau_apply`): a plugin never holds the window.
//!
//! These calls can only be made from a task (a handler, `task.spawn`), never while drawing: `rune-luau` raises an error
//! for an async call in a render, which is the point: a render cannot wait on the daemon.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};

use rune::Color;
use rune_luau::mlua::{self, Function, MultiValue, Table, Value};
use rune_luau::{Completer, Data, ScriptOptions};
use serde_json::json;

use super::{Action, Meta, Slot, nodes, note_failure, push_action, valid_id};
use crate::link::luau::{self as wire_api, Wire};
use crate::theme::Neko;

/// What one plugin file shares between its script and the window.
pub struct Context {
    pub pack: String,
    pub key: String,
    pub meta: Rc<RefCell<Option<Meta>>>,
    pub failure: Rc<RefCell<Option<String>>>,
    pub has_input: Rc<std::cell::Cell<bool>>,
}

/// `n` calls in any window of one second; the next is refused.
pub struct RateLimit {
    limit: usize,
    stamps: VecDeque<Instant>,
}

impl RateLimit {
    pub fn new(limit: usize) -> RateLimit {
        RateLimit { limit, stamps: VecDeque::new() }
    }

    pub fn allow(&mut self) -> bool {
        self.allow_at(Instant::now())
    }

    pub fn allow_at(&mut self, now: Instant) -> bool {
        while self.stamps.front().is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(1)) {
            self.stamps.pop_front();
        }
        if self.stamps.len() >= self.limit {
            return false;
        }
        self.stamps.push_back(now);
        true
    }
}

/// Reads a plugin may make in a second.
pub const READS_PER_SECOND: usize = 4;
/// Storage writes and reads a plugin may make in a second (a write is a file the daemon rewrites).
pub const STORAGE_WRITES_PER_SECOND: usize = 2;
pub const STORAGE_READS_PER_SECOND: usize = 8;
/// The longest text `host.ask` drafts.
pub const MAX_ASK: usize = 4000;
/// The least time between two drafts or two routes.
pub const ACTION_GAP: Duration = Duration::from_millis(400);

/// Whether a draft or a route may be taken now: from a press, and not right after the last one.
fn action_allowed(what: &str) -> Result<(), String> {
    if !UI_ENTRY.get() {
        return Err(format!("{what} only works when a person presses something (a button or a row), not in the background."));
    }
    let now = Instant::now();
    if LAST_ACTION.get().is_some_and(|t| now.duration_since(t) < ACTION_GAP) {
        return Err(format!("{what} was just used: wait a moment."));
    }
    LAST_ACTION.set(Some(now));
    Ok(())
}

thread_local! {
    /// True while the window runs a plugin's press handler (`ScriptOptions::around` wraps the entries the UI makes): the only time
    /// `host.ask` and `host.open` do anything. A task, `init` or a render that calls them gets `nil, err`.
    static UI_ENTRY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// When the last draft or route was taken, so a handler that loops cannot hammer the window.
    static LAST_ACTION: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
    static WIRE: RefCell<Wire> = RefCell::new(Wire::none());
    static THEME: RefCell<Option<Rc<Neko>>> = const { RefCell::new(None) };
}

/// The wire plugins' reads and storage use now; the window sets it each frame.
pub fn publish_wire(wire: Wire) {
    WIRE.with(|w| *w.borrow_mut() = wire);
}

fn current_wire() -> Wire {
    WIRE.with(|w| w.borrow().clone())
}

/// The theme the next draw uses: every node and the palette read it, so a plugin draws in Neko's own colours, in whichever theme is on.
pub fn set_theme(theme: Rc<Neko>) {
    THEME.with(|t| *t.borrow_mut() = Some(theme));
}

pub fn theme() -> Rc<Neko> {
    THEME.with(|t| t.borrow().clone()).unwrap_or_else(|| crate::theme::theme(true))
}

/// The colour names a plugin may use. Nothing else resolves, and `#rrggbb` is refused before it gets that far.
pub const COLOURS: &[&str] = &[
    "text", "strong", "muted", "faint", "window", "card", "sidebar", "border", "hover", "active", "selected", "ring", "iris", "amber", "sky", "mint", "coral", "text-secondary", "bg", "card-border", "separator", "press", "control-off", "control-track",
    "indicator",
];

/// A colour name as Neko's current theme paints it (the names `rune-luau`'s default palette has are kept, so its docs hold).
pub fn colour(name: &str) -> Option<Color> {
    let t = theme();
    Some(match name {
        "text" | "base" => t.text.base,
        "strong" => t.text.strong,
        "muted" | "text-secondary" => t.text.muted,
        "faint" => t.text.faint,
        "window" | "bg" => t.surface.window,
        "card" => t.surface.card,
        "sidebar" => t.surface.sidebar,
        "border" | "card-border" | "separator" => t.surface.border,
        "hover" | "control-track" => t.surface.hover,
        "active" | "press" | "control-off" => t.surface.active,
        "selected" => t.surface.selected,
        "ring" => t.surface.ring,
        "iris" | "indicator" => t.meaning.iris,
        "amber" => t.meaning.amber,
        "sky" => t.meaning.sky,
        "mint" => t.meaning.mint,
        "coral" => t.meaning.coral,
        _ => return None,
    })
}

pub fn unix_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

// ---- values across the boundary -------------------------------------------------------------------------------------------

fn fail(message: &str) -> Data {
    Data::Map([("ok".to_owned(), Data::Bool(false)), ("err".to_owned(), Data::String(message.to_owned()))].into())
}

fn done_with(result: Result<serde_json::Value, String>) -> Data {
    match result {
        Ok(value) => Data::Map([("ok".to_owned(), Data::Bool(true)), ("value".to_owned(), json_to_data(&value, 0))].into()),
        Err(message) => fail(&message),
    }
}

/// JSON as the plain values a script receives: whole numbers stay whole, `null` is nil.
pub fn json_to_data(value: &serde_json::Value, depth: usize) -> Data {
    use serde_json::Value as J;
    if depth > 32 {
        return Data::Nil;
    }
    match value {
        J::Null => Data::Nil,
        J::Bool(b) => Data::Bool(*b),
        J::Number(n) => n.as_i64().map(Data::Integer).unwrap_or_else(|| n.as_f64().map_or(Data::Nil, Data::Number)),
        J::String(s) => Data::String(s.clone()),
        J::Array(items) => Data::List(items.iter().map(|v| json_to_data(v, depth + 1)).collect()),
        J::Object(map) => Data::Map(map.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), json_to_data(v, depth + 1))).collect()),
    }
}

/// The most values a table handed to the host may hold, and how deep: the walk below stops at the budget, so a table of millions of
/// entries costs the UI thread a few thousand steps, not a second.
const MAX_NODES: usize = 20_000;
const MAX_DEPTH: usize = 12;

struct Budget {
    nodes: usize,
    bytes: usize,
}

impl Budget {
    /// One value, and `text` bytes of text with it.
    fn spend(&mut self, text: usize) -> Result<(), String> {
        self.nodes = self.nodes.checked_sub(1).ok_or("That is too large to send.")?;
        self.bytes = self.bytes.checked_sub(text).ok_or_else(|| "That is too large to send.".to_owned())?;
        Ok(())
    }
}

fn walk(value: &Value, depth: usize, budget: &mut Budget) -> Result<serde_json::Value, String> {
    use serde_json::Value as J;
    if depth > MAX_DEPTH {
        return Err("That is nested too deeply to send.".to_owned());
    }
    // Every value costs one step, whatever it is: a table of millions of numbers stops at the budget too.
    budget.spend(0)?;
    Ok(match value {
        Value::Nil => J::Null,
        Value::Boolean(b) => J::Bool(*b),
        Value::Integer(i) => J::from(*i),
        Value::Number(n) if n.fract() == 0.0 && n.abs() < 9.0e15 => J::from(*n as i64),
        Value::Number(n) => serde_json::Number::from_f64(*n).map_or(J::Null, J::Number),
        Value::String(s) => {
            let text = s.to_string_lossy();
            budget.spend(text.len())?;
            J::String(text.to_string())
        }
        Value::Table(t) => {
            let len = t.raw_len();
            if len > 0 {
                let mut items = Vec::new();
                for i in 1..=len {
                    items.push(walk(&t.raw_get::<Value>(i).map_err(|e| e.to_string())?, depth + 1, budget)?);
                }
                J::Array(items)
            } else {
                let mut map = serde_json::Map::new();
                for pair in t.pairs::<Value, Value>() {
                    let (k, v) = pair.map_err(|e| e.to_string())?;
                    let key = match k {
                        Value::String(s) => s.to_string_lossy().to_string(),
                        Value::Integer(i) => i.to_string(),
                        _ => continue,
                    };
                    budget.spend(key.len())?;
                    map.insert(key, walk(&v, depth + 1, budget)?);
                }
                J::Object(map)
            }
        }
        // A function, a thread, userdata: not data.
        _ => J::Null,
    })
}

/// A script value as JSON, within a budget of [`MAX_NODES`] values, [`MAX_DEPTH`] levels and `max` bytes of text.
fn value_to_json(_lua: &mlua::Lua, value: Value, max: usize) -> Result<serde_json::Value, String> {
    walk(&value, 0, &mut Budget { nodes: MAX_NODES, bytes: max })
}

fn string_arg(args: &MultiValue, at: usize) -> Option<String> {
    match args.iter().nth(at) {
        Some(Value::String(s)) => Some(s.to_string_lossy().to_string()),
        _ => None,
    }
}

// ---- the setup chunk (Luau) -------------------------------------------------------------------------------------------------

/// Runs once in each new VM before the sandbox seals it: makes the stock constructors refuse raw colours, adds `ui.input`'s
/// `on_change`, `ui.text{ muted = true }`, shapes the `host` table, and seals `ui`.
const SETUP: &str = r##"
local ui, readRaw, getRaw, setRaw, askRaw, openRaw, nowRaw = ...

-- A plugin's own words in the log, in an error or in a print are cut: the library keeps what is logged outside the 64 MB heap.
local rawPrint, rawWarn, rawError = print, warn, error
local function short(v)
    local s = tostring(v)
    if #s > 512 then s = string.sub(s, 1, 512) .. "…" end
    return s
end
local function line(...)
    local parts = {}
    for i = 1, math.min(select("#", ...), 16) do parts[i] = short((select(i, ...))) end
    local s = table.concat(parts, "\t")
    if #s > 1024 then s = string.sub(s, 1, 1024) end
    return s
end
print = function(...) rawPrint(line(...)) end
warn = function(...) rawWarn(line(...)) end
error = function(message, level)
    if type(message) == "string" and #message > 1024 then message = string.sub(message, 1, 1024) .. "…" end
    if level == 0 then return rawError(message, 0) end
    return rawError(message, (level or 1) + 1)
end

local COLOR_KEYS = { "fill", "hover", "press", "border", "color", "hoverGroupColor" }
-- The host nodes take plain values only: a table handed to one is copied into Rust whole, and a table can be made to hold itself twice.
local function flat(t)
    if type(t) == "table" then
        for k, v in next, t do
            if type(v) == "table" then error(`the prop {k} must be a plain value (text, a number or true/false), not a table`, 3) end
        end
    end
end
local function check(t)
    if type(t) == "table" then
        -- Only a press is a person choosing something: a hover or a scroll must not be able to draft or go anywhere.
        if t.onHover ~= nil or t.onVisibleRows ~= nil then
            error("onHover and onVisibleRows are not available to plugins: use onPress", 3)
        end
        for _, key in COLOR_KEYS do
            local v = t[key]
            if type(v) == "string" and string.byte(v, 1) == 35 then
                error(`raw colours are not allowed ({key} = {v}): use a Neko colour name such as "text", "muted", "faint", "mint", "coral", "amber", "sky" or "iris"`, 3)
            end
        end
    end
end
local function guarded(raw, plain)
    return function(...)
        for i = 1, select("#", ...) do
            local a = (select(i, ...))
            check(a)
            if plain then flat(a) end
        end
        return raw(...)
    end
end
for _, name in { "row", "col", "box", "spacer", "virtualList" } do
    local raw = ui[name]
    if raw then ui[name] = guarded(raw, false) end
end
for _, name in { "button", "image", "glyph" } do
    local raw = ui[name]
    if raw then ui[name] = guarded(raw, true) end
end
ui.hex = function() error("raw colours are not allowed: use a Neko colour name such as \"text\", \"muted\" or \"mint\"", 2) end

-- ui.text("words", props) or ui.text{ "words", muted = true, size = 12 }
local rawText = ui.text
ui.text = function(first, ...)
    if type(first) == "table" then
        local props = table.clone(first)
        local content = props[1] or props.text
        props[1] = nil
        props.text = nil
        if props.muted then props.color = props.color or "muted" end
        props.muted = nil
        check(props)
        return rawText(content or "", props)
    end
    local args = table.pack(...)
    for i = 1, args.n do
        local p = args[i]
        check(p)
        if type(p) == "table" and p.muted ~= nil then
            local c = table.clone(p)
            if c.muted then c.color = c.color or "muted" end
            c.muted = nil
            args[i] = c
        end
    end
    return rawText(first, table.unpack(args, 1, args.n))
end

-- ui.input{ key = "q", value = state.q, on_change = function(text) ... end, placeholder = "..." }
local rawInput = ui.input
local handlers = {}
ui.input = function(props)
    local key = props.key or "input"
    local p = table.clone(props)
    handlers[key] = props.on_change
    p.on_change = nil
    p.key = key
    check(p)
    flat(p)
    return rawInput(p)
end
local function change(key, text)
    local handler = handlers[key]
    if handler then handler(text) end
end
table.freeze(ui)

local host = {}
function host.read(name, args)
    local r = readRaw(name, args)
    if type(r) ~= "table" or r.ok ~= true then return nil, (type(r) == "table" and r.err) or "no answer" end
    return r.value
end
local storage = {}
function storage.get(key)
    local r = getRaw(key)
    if type(r) ~= "table" or r.ok ~= true then return nil, (type(r) == "table" and r.err) or "no answer" end
    return r.value
end
function storage.set(key, value)
    local r = setRaw(key, value)
    if type(r) ~= "table" or r.ok ~= true then return nil, (type(r) == "table" and r.err) or "no answer" end
    return true
end
host.storage = table.freeze(storage)
function host.ask(text) return askRaw(text) end
function host.open(route) return openRaw(route) end
function host.now() return nowRaw() end
table.freeze(host)
return host, change
"##;

fn setup(ctx: &Context, h: &rune_luau::Host<'_>) -> mlua::Result<()> {
    let lua = h.lua();
    let pack = ctx.pack.clone();
    let reads = Rc::new(RefCell::new(RateLimit::new(READS_PER_SECOND)));
    let gets = Rc::new(RefCell::new(RateLimit::new(STORAGE_READS_PER_SECOND)));
    let sets = Rc::new(RefCell::new(RateLimit::new(STORAGE_WRITES_PER_SECOND)));

    let read_raw = h.async_function("host.read", move |lua, args: MultiValue, done: Completer| {
        let Some(name) = string_arg(&args, 0) else {
            done.complete(fail("host.read needs the name of a read, such as \"Workbench\"."));
            return Ok(());
        };
        let json = match args.iter().nth(1) {
            None | Some(Value::Nil) => json!({}),
            Some(v) => match value_to_json(lua, v.clone(), wire_api::MAX_ARGS_BYTES) {
                Ok(j) => j,
                Err(e) => {
                    done.complete(fail(&e));
                    return Ok(());
                }
            },
        };
        // A name that is not a read is refused here, before it costs a call and before anything is sent.
        if let Err(e) = wire_api::read_request(&name, &json) {
            done.complete(fail(&e));
            return Ok(());
        }
        if !reads.borrow_mut().allow() {
            done.complete(fail("rate limited: a plugin may read four times a second"));
            return Ok(());
        }
        let wire = current_wire();
        rune_luau::spawn_blocking(move || done.complete(done_with(wire_api::read(&wire, &name, &json, wire.now_ms()))));
        Ok(())
    })?;

    let get_pack = pack.clone();
    let get_raw = h.async_function("host.storage.get", move |_lua, args: MultiValue, done: Completer| {
        let Some(key) = string_arg(&args, 0) else {
            done.complete(fail("host.storage.get needs a key."));
            return Ok(());
        };
        if !gets.borrow_mut().allow() {
            done.complete(fail("rate limited: storage reads are limited to eight a second"));
            return Ok(());
        }
        let (wire, pack) = (current_wire(), get_pack.clone());
        rune_luau::spawn_blocking(move || done.complete(done_with(wire_api::storage_get(&wire, &pack, &key))));
        Ok(())
    })?;

    let set_pack = pack.clone();
    let set_raw = h.async_function("host.storage.set", move |lua, args: MultiValue, done: Completer| {
        let Some(key) = string_arg(&args, 0) else {
            done.complete(fail("host.storage.set needs a key and a value."));
            return Ok(());
        };
        let value = match value_to_json(lua, args.iter().nth(1).cloned().unwrap_or(Value::Nil), 4 * 1024 * 1024) {
            Ok(v) => v,
            Err(e) => {
                done.complete(fail(&e));
                return Ok(());
            }
        };
        if !sets.borrow_mut().allow() {
            done.complete(fail("rate limited: storage writes are limited to two a second"));
            return Ok(());
        }
        let (wire, pack) = (current_wire(), set_pack.clone());
        rune_luau::spawn_blocking(move || done.complete(done_with(wire_api::storage_set(&wire, &pack, &key, &value).map(|()| json!(true)))));
        Ok(())
    })?;

    // Drafts into Neko's ask box. Never sends: the window only puts the words there (`App::luau_apply`).
    let ask = lua.create_function(|_, text: Value| -> mlua::Result<(Option<bool>, Option<String>)> {
        let Value::String(s) = text else { return Ok((None, Some("host.ask needs the words to draft.".to_owned()))) };
        let words = s.to_string_lossy().to_string();
        if words.trim().is_empty() {
            return Ok((None, Some("host.ask needs some words.".to_owned())));
        }
        if words.chars().count() > MAX_ASK {
            return Ok((None, Some(format!("host.ask drafts at most {MAX_ASK} characters."))));
        }
        if let Err(why) = action_allowed("host.ask") {
            return Ok((None, Some(why)));
        }
        push_action(Action::Ask(words));
        Ok((Some(true), None))
    })?;
    let open = lua.create_function(|_, route: Value| -> mlua::Result<(Option<bool>, Option<String>)> {
        let Value::String(s) = route else { return Ok((None, Some("host.open needs a route.".to_owned()))) };
        match crate::deeplink::clean_route(&s.to_string_lossy()) {
            Some(route) => match action_allowed("host.open") {
                Ok(()) => {
                    push_action(Action::Open(route));
                    Ok((Some(true), None))
                }
                Err(why) => Ok((None, Some(why))),
            },
            None => Ok((None, Some("That is not a route in Neko.".to_owned()))),
        }
    })?;
    let now = lua.create_function(|_, ()| Ok(current_wire().now_ms()))?;

    let ui: Table = lua.named_registry_value::<Table>("_REGISTEREDMODULES")?.get("@rune/ui")?;
    let make: Function = lua.load(SETUP).set_name("=neko-host").into_function()?;
    let (host, change): (Table, Function) = make.call((ui, read_raw, get_raw, set_raw, ask, open, now))?;
    h.register_module("@neko/host", host.clone())?;
    h.global("__neko_host", host)?;
    h.global("__neko_change", change)?;
    Ok(())
}

/// Reads the file's table, checks it, and wraps `render`/`init` (`ADAPT`). A file that does not say what it is does not start.
fn start(ctx_meta: &RefCell<Option<Meta>>, lua: &mlua::Lua, module: &Table) -> mlua::Result<()> {
    let text = |key: &str| -> mlua::Result<Option<String>> {
        match module.raw_get::<Value>(key)? {
            Value::Nil => Ok(None),
            Value::String(s) => Ok(Some(s.to_string_lossy().to_string())),
            other => Err(mlua::Error::runtime(format!("{key} must be a string, not a {}", other.type_name()))),
        }
    };
    let slot = match text("slot")?.as_deref() {
        Some("card") => Slot::Card,
        Some("view") => Slot::View,
        _ => return Err(mlua::Error::runtime("the file's table needs slot = \"card\" or slot = \"view\"")),
    };
    let id = text("id")?.unwrap_or_default();
    if !valid_id(&id) {
        return Err(mlua::Error::runtime("the file's table needs id = \"...\" of letters a-z, digits and dashes (at most 40)"));
    }
    let title = text("title")?.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()).ok_or_else(|| mlua::Error::runtime("the file's table needs a title"))?;
    if title.chars().count() > 60 {
        return Err(mlua::Error::runtime("the title is at most 60 characters"));
    }
    let icon = text("icon")?;
    if !matches!(module.raw_get::<Value>("render")?, Value::Function(_)) {
        return Err(mlua::Error::runtime("the file's table needs render = function(state, host) ... end"));
    }
    if !matches!(module.raw_get::<Value>("init")?, Value::Nil | Value::Function(_)) {
        return Err(mlua::Error::runtime("init must be a function"));
    }
    *ctx_meta.borrow_mut() = Some(Meta { slot, id, title, icon });
    let adapt: Function = lua.load(super::ADAPT).set_name("=neko-adapt").into_function()?;
    let host: Table = lua.globals().get("__neko_host")?;
    adapt.call::<()>((module.clone(), host))?;
    let change: Function = lua.globals().get("__neko_change")?;
    module.raw_set("__neko_change", change)?;
    Ok(())
}

/// The `ScriptOptions` of one plugin file.
pub fn options(ctx: Context) -> ScriptOptions {
    let failure = ctx.failure.clone();
    let meta = ctx.meta.clone();
    let ctx = Rc::new(ctx);
    let setup_ctx = ctx.clone();
    let node_ctx = ctx.clone();
    let base = {
        let t = theme();
        t.ty.row
    };
    let mut options = ScriptOptions::new()
        .palette(Rc::new(|name, _dark| colour(name)))
        .text(base, [400.0, 500.0, 600.0, 700.0])
        .time_limit(Duration::from_millis(250))
        .memory_limit(64 * 1024 * 1024)
        .on_log(move |level, line| note_failure(&failure, level, line))
        .setup(move |h| setup(&setup_ctx, h))
        .start(move |lua, module, _| start(&meta, lua, module))
        .around(|state, mut tx, run| {
            let outer = UI_ENTRY.replace(true);
            run(tx.as_deref_mut());
            UI_ENTRY.set(outer);
            if let Some(app) = state.downcast_mut::<crate::app::App>() {
                app.luau_apply(tx);
            }
        });
    for node in nodes::all(&node_ctx) {
        options = options.node(node);
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fifth_read_in_a_second_is_refused_and_a_second_later_it_is_allowed_again() {
        let mut limit = RateLimit::new(READS_PER_SECOND);
        let start = Instant::now();
        for i in 0..4 {
            assert!(limit.allow_at(start + Duration::from_millis(i * 10)), "read {i}");
        }
        assert!(!limit.allow_at(start + Duration::from_millis(500)));
        assert!(!limit.allow_at(start + Duration::from_millis(999)));
        assert!(limit.allow_at(start + Duration::from_millis(1010)), "the first slot is free again after a second");
    }

    #[test]
    fn a_table_handed_to_the_host_is_walked_within_a_budget() {
        let lua = mlua::Lua::new();
        let value = |code: &str| -> Value { lua.load(code).into_function().unwrap().call::<Value>(()).unwrap() };
        let ok = value_to_json(&lua, value("return { a = 1, b = { 'x', 'y' }, c = true }"), 1024).unwrap();
        assert_eq!(ok, json!({ "a": 1, "b": ["x", "y"], "c": true }));
        let huge = value("local t = {} for i = 1, 3000000 do t[i] = i end return t");
        let wide = value("local t = {} for i = 1, 300000 do t['k' .. i] = i end return t");
        let started = Instant::now();
        assert!(value_to_json(&lua, huge, 1 << 30).unwrap_err().contains("too large"));
        assert!(value_to_json(&lua, wide, 1 << 30).unwrap_err().contains("too large"));
        assert!(started.elapsed() < Duration::from_millis(250), "the walk stops at the budget: {:?}", started.elapsed());
        let deep = value("local t = {} local cur = t for i = 1, 100 do cur.x = {} cur = cur.x end return t");
        assert!(value_to_json(&lua, deep, 1024).unwrap_err().contains("deeply"));
        let cycle = value("local t = {} t.me = t return t");
        assert!(value_to_json(&lua, cycle, 1024).is_err(), "a cycle is cut by the depth and budget");
        let bytes = value("return string.rep('x', 5000)");
        assert!(value_to_json(&lua, bytes, 1024).unwrap_err().contains("too large"));
        assert_eq!(value_to_json(&lua, value("return function() end"), 10).unwrap(), serde_json::Value::Null);
    }

    #[test]
    fn json_comes_across_as_plain_values() {
        let data = json_to_data(&json!({ "n": 3, "f": 1.5, "s": "x", "list": [1, 2], "none": null, "deep": { "ok": true } }), 0);
        assert_eq!(data.get("n"), &Data::Integer(3));
        assert_eq!(data.get("f"), &Data::Number(1.5));
        assert_eq!(data.get("none"), &Data::Nil);
        assert_eq!(data.get("list"), &Data::List(vec![Data::Integer(1), Data::Integer(2)]));
        assert_eq!(data.get("deep").get("ok"), &Data::Bool(true));
    }

    #[test]
    fn every_colour_name_resolves_and_a_raw_one_does_not() {
        for name in COLOURS {
            assert!(colour(name).is_some(), "{name}");
        }
        assert!(colour("#ff0000").is_none() && colour("red").is_none());
    }
}
