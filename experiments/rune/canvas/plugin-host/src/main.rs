//! A Rune app that hosts Luau plugins from a folder, without recompiling. Each folder in
//! `plugins/` (or `PLUGINS_DIR`) with an `init.luau` is a page in the sidebar; save the file and
//! the page reloads in place; add a folder and a page appears. The host gives plugins:
//!
//! * `require("@neko/host")`: `request(json)`, a pretend daemon call that completes off the UI
//!   thread (an async function: the calling task waits, the UI doesn't), and `inputValue(id)`.
//! * Three host-built nodes beyond stock `@rune/ui`: `ui.input` (text field), `ui.image` (a file
//!   inside the plugin's own folder) and `ui.canvas` (lines and filled rects from plain data).
//!
//! `plugin-host --snap out.png --script scripts/demo.json`

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rune::tok::{self, radius, space, type_};
use rune::{AppSpec, Color, Cx, El, ImageCache, Shape, TextEdit, Tx, Waker, canvas, col, el, row, text, text_input};
use rune_luau::mlua::{self, MultiValue, Value};
use rune_luau::{Completer, Data, HostNode, LogLevel, Script, ScriptOptions, Sources};

struct Plugin {
    name: String,
    dir: PathBuf,
    script: Script,
}

struct State {
    dir: PathBuf,
    plugins: Vec<Plugin>,
    selected: usize,
    inputs: HashMap<String, TextEdit>,
    /// Plugins whose input changed since the last poll: (plugin, id, text).
    dirty: Vec<(String, String, String)>,
    notified: HashMap<String, String>,
    next_scan: f64,
    waker: Option<Waker>,
}

thread_local! {
    /// The host nodes build from this: a copy of `State::inputs` made before each view.
    static MIRROR: RefCell<HashMap<String, TextEdit>> = RefCell::new(HashMap::new());
    static IMAGES: RefCell<(Option<ImageCache>, HashMap<PathBuf, rune::Image>)> = RefCell::new((None, HashMap::new()));
}

fn plugins_dir() -> PathBuf {
    std::env::var("PLUGINS_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("plugins"))
}

fn init() -> State {
    State { dir: plugins_dir(), plugins: vec![], selected: 0, inputs: HashMap::new(), dirty: vec![], notified: HashMap::new(), next_scan: 0.0, waker: None }
}

/// The pretend daemon: a worker thread answers after 150 ms.
fn fake_daemon(request: &str) -> Data {
    let mut m = std::collections::BTreeMap::new();
    m.insert("ok".to_string(), Data::Bool(true));
    m.insert("echo".to_string(), Data::String(request.to_string()));
    let rows: Vec<Data> = ["Fix flaky test", "Review PR 412", "Draft release notes", "Triage inbox"].iter().enumerate().map(|(i, t)| {
        let mut r = std::collections::BTreeMap::new();
        r.insert("id".to_string(), Data::Integer(i as i64 + 1));
        r.insert("title".to_string(), Data::String(t.to_string()));
        Data::Map(r)
    }).collect();
    m.insert("tasks".to_string(), Data::List(rows));
    Data::Map(m)
}

fn parse_color(d: Option<&Data>) -> Color {
    match d {
        Some(Data::String(s)) => rune::color::hex(s).unwrap_or(Color::WHITE),
        _ => Color::WHITE,
    }
}

fn num(m: &std::collections::BTreeMap<String, Data>, k: &str) -> f32 {
    m.get(k).and_then(|d| d.as_f64()).unwrap_or(0.0) as f32
}

fn options(name: &str, dir: &Path) -> ScriptOptions {
    let (name_input, name_image, dir_image) = (name.to_string(), name.to_string(), dir.to_path_buf());
    let _ = &name_image;
    ScriptOptions::new()
        .time_limit(Duration::from_millis(250))
        .memory_limit(32 * 1024 * 1024)
        .on_log({
            let name = name.to_string();
            move |level: LogLevel, line: &str| eprintln!("[{name}] {level:?}: {line}")
        })
        .setup(|host| {
            let request = host.async_function("request", |_lua, args: MultiValue, done: Completer| {
                let json = match args.front() {
                    Some(Value::String(s)) => s.to_string_lossy().to_string(),
                    _ => String::new(),
                };
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(150));
                    done.complete(fake_daemon(&json));
                });
                Ok(())
            })?;
            let input_value = host.lua().create_function(|_, id: String| Ok(MIRROR.with(|m| m.borrow().get(&id).map(|e| e.text.clone()).unwrap_or_default())))?;
            let module = host.lua().create_table()?;
            module.set("request", request)?;
            module.set("inputValue", input_value)?;
            host.register_module("@neko/host", module)
        })
        .node(HostNode::new("input", move |a| {
            let id = a.string("id").unwrap_or("input").to_string();
            let edit = MIRROR.with(|m| m.borrow().get(&id).cloned()).unwrap_or_else(|| TextEdit::new(""));
            let placeholder = a.string("placeholder").unwrap_or("").to_string();
            let plugin = name_input.clone();
            let id2 = id.clone();
            let field = text_input(&edit, type_::BODY).key(format!("input-{id}")).single_line().placeholder(placeholder, Color::WHITE.alpha(0.4)).w_full()
                .on_edit(move |s: &mut State| {
                    // The accessor runs on every edit: note it so `poll` can tell the plugin.
                    let e = s.inputs.entry(id2.clone()).or_insert_with(|| TextEdit::new(""));
                    s.dirty.push((plugin.clone(), id2.clone(), e.text.clone()));
                    s.inputs.get_mut(&id2).expect("just inserted")
                });
            let t = tok::theme(true);
            Ok(row().w_full().px(space::S2).py(space::S1).radius(radius::MD).fill(t.control_track).child(field))
        }))
        .node(HostNode::new("image", move |a| {
            let rel = a.string("src").ok_or("ui.image needs src")?.to_string();
            // Plugins may read files inside their own folder only.
            if rel.contains("..") || rel.starts_with('/') {
                return Err(format!("ui.image: {rel} is outside the plugin folder"));
            }
            let path = dir_image.join(&rel);
            let (w, h) = (a.number("w").unwrap_or(96.0) as f32, a.number("h").unwrap_or(96.0) as f32);
            let px = IMAGES.with(|im| {
                let mut im = im.borrow_mut();
                let (cache, kept) = &mut *im;
                let cache = cache.as_ref()?;
                let image = kept.entry(path.clone()).or_insert_with(|| cache.file(&path, w.max(h), 2.0));
                Some(image.pixels().clone())
            });
            Ok(match px {
                Some(p) => el().size(w, h).radius(6.0).pixels_fit(&p, rune::SurfaceFit::Fit),
                None => el().size(w, h),
            })
        }))
        .node(HostNode::new("canvas", |a| {
            let (w, h) = (a.number("w").unwrap_or(200.0) as f32, a.number("h").unwrap_or(100.0) as f32);
            let mut shape = Shape::new();
            if let Some(Data::List(ops)) = a.data("ops") {
                for op in ops {
                    let Data::Map(m) = op else { continue };
                    let col = parse_color(m.get("color"));
                    match m.get("kind") {
                        Some(Data::String(k)) if k == "line" => {
                            shape = shape.line([num(&m, "x1"), num(&m, "y1")], [num(&m, "x2"), num(&m, "y2")], num(&m, "w").max(1.0), rune::LineCap::Round, col);
                        }
                        Some(Data::String(k)) if k == "rect" => {
                            let (x, y, rw, rh) = (num(&m, "x"), num(&m, "y"), num(&m, "w"), num(&m, "h"));
                            shape = shape.fill(vec![[x, y], [x + rw, y], [x + rw, y + rh], [x, y + rh]], col);
                        }
                        _ => {}
                    }
                }
            }
            Ok(canvas(shape).size(w, h))
        }))
}

fn scan(s: &mut State) -> bool {
    let mut changed = false;
    let Ok(read) = std::fs::read_dir(&s.dir) else { return false };
    let mut found: Vec<(String, PathBuf)> = read
        .flatten()
        .filter(|e| e.path().join("init.luau").is_file())
        .filter_map(|e| Some((e.file_name().to_str()?.to_string(), e.path())))
        .collect();
    found.sort();
    for (name, dir) in &found {
        if s.plugins.iter().any(|p| &p.name == name) {
            continue;
        }
        match Sources::from_dir(dir) {
            Ok(sources) => {
                let script = Script::with_options(name, sources, options(name, dir));
                if let Some(w) = &s.waker {
                    script.set_waker(w.clone());
                }
                if let Err(e) = script.watch(dir) {
                    eprintln!("plugin-host: not watching {}: {e}", dir.display());
                }
                eprintln!("plugin-host: loaded plugin {name}");
                s.plugins.push(Plugin { name: name.clone(), dir: dir.clone(), script });
                changed = true;
            }
            Err(e) => eprintln!("plugin-host: {name}: {e}"),
        }
    }
    let before = s.plugins.len();
    s.plugins.retain(|p| found.iter().any(|(n, _)| n == &p.name));
    changed |= s.plugins.len() != before;
    s.plugins.sort_by(|a, b| a.name.cmp(&b.name));
    s.selected = s.selected.min(s.plugins.len().saturating_sub(1));
    changed
}

fn setup(s: &mut State, waker: Waker) {
    IMAGES.with(|im| im.borrow_mut().0 = Some(ImageCache::new(waker.clone())));
    s.waker = Some(waker);
    scan(s);
    for p in &s.plugins {
        if let Some(w) = &s.waker {
            p.script.set_waker(w.clone());
        }
    }
}

fn poll(s: &mut State, now: f64, tx: &mut Tx) -> bool {
    let mut changed = false;
    if now >= s.next_scan {
        s.next_scan = now + 1.0;
        changed |= scan(s);
        tx.wake_after(Duration::from_secs(1));
    }
    MIRROR.with(|m| *m.borrow_mut() = s.inputs.clone());
    for (plugin, id, _) in std::mem::take(&mut s.dirty) {
        // The accessor also runs for reads and before the edit lands: compare with what the
        // plugin was last told, using the text as it is now.
        let value = s.inputs.get(&id).map(|e| e.text.clone()).unwrap_or_default();
        if std::env::var("PLUGIN_HOST_DEBUG").is_ok() {
            eprintln!("plugin-host: dirty {id} = {value:?} focused={:?}", s.inputs.get(&id).map(|e| e.focused));
        }
        if s.notified.get(&id) == Some(&value) || (value.is_empty() && !s.notified.contains_key(&id)) {
            continue;
        }
        s.notified.insert(id.clone(), value.clone());
        if let Some(p) = s.plugins.iter().find(|p| p.name == plugin) {
            let r = p.script.call("onInput", [Data::String(id), Data::String(value)]);
            if std::env::var("PLUGIN_HOST_DEBUG").is_ok() {
                eprintln!("plugin-host: onInput -> {r:?}");
            }
            changed = true;
        }
    }
    for p in &s.plugins {
        changed |= p.script.poll(now, tx);
    }
    changed
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    MIRROR.with(|m| *m.borrow_mut() = s.inputs.clone());
    let t = tok::theme(true);
    let mut side = col().w(180.0).h_full().padding(space::S2).gap(space::S1).fill(t.card)
        .child(text("PLUGINS").font(type_::SECTION).color(t.text_secondary));
    for (i, p) in s.plugins.iter().enumerate() {
        let on = i == s.selected;
        side = side.child(row().key(format!("nav-{}", p.name)).role("button").label(p.name.clone()).focusable(true).w_full().px(space::S2).py(space::S1).radius(radius::SM)
            .fill(if on { t.control_off } else { Color::TRANSPARENT })
            .child(text(p.name.clone()).font(type_::BODY).color(t.text))
            .on_press(move |s: &mut State| s.selected = i));
    }
    let page = match s.plugins.get(s.selected) {
        Some(p) => {
            // A text field keeps its edit state in a snapshot the view rebuilds, so a memoized subtree
            // would hand the runtime a stale one every frame and undo typing. With inputs on screen
            // the script is rendered uncached (it re-enters the VM on every view).
            let body = if std::env::var("PLUGIN_HOST_MEMO").is_ok() { p.script.view(cx) } else { p.script.render_uncached::<State>("render", true) };
            let error = p.script.error().map(|e| format!("{e}"));
            let mut c = col().w_full().h_full().child(body);
            if let Some(e) = error {
                c = c.child(text(format!("plugin error: {e}")).font(type_::BODY).color(tok::accent(2, true)));
            }
            c
        }
        None => col().w_full().h_full().center().child(text("No plugins in the folder yet").font(type_::BODY).color(t.text_secondary)),
    };
    row().w_full().h_full().fill(t.bg).child(side).child(el().grow(1.0).h_full().child(page))
}

fn main() {
    let spec = AppSpec::<State> { setup: Some(setup), poll: Some(poll), ..AppSpec::new("Plugin host", (900.0, 620.0), init, view) };
    rune::run(spec);
}
