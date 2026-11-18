//! The structure crawl (docs/knowledge/rune-identity.md): every state of the window, read through the real layout, must keep
//! the window's shape. Rules checked on each state:
//!
//!  a. the sessions column is there at its width (232 to 360) unless the person folded it;
//!  b. the Neko column is there, at its width or as its 44 point strip with the spark and a count, never a lone icon;
//!  c. nothing drawn extends outside its region (the sessions column, the session region, the context panel, the Neko column);
//!  d. no two siblings partly overlap (one inside the other is an overlay and stands);
//!  e. no text is cut off by what holds it (an ellipsis shortens text inside its box and is fine);
//!  f. no panic, and no "stopped" state on screen.
//!
//! By default it crawls every scene (synthetic data, `scenes.rs`) at three window sizes. With `NEKO_STRUCTURE_SOCKET=<harness
//! socket>` (`scripts/identity.sh` sets it) it also crawls a COPY of the owner's data: every full view, Setup and Settings section,
//! the Calendar's modes, All sessions as a list, a board and a tree, one session of every state, the palette and the overlays.

use neko_rune::app::{App, Dispatch};
use neko_rune::layout::NEKO_STRIP;
use neko_rune::link::Call;
use neko_rune::pages::{CalMode, Section};
use neko_rune::scenes::SCENES;
use neko_rune::view::setup::SetupSection;
use neko_rune::view::view;
use neko_rune::views::FullView;
use rune::{Env, Runtime};

const SIZES: [(f32, f32); 3] = [(1440.0, 900.0), (1180.0, 760.0), (960.0, 600.0)];
const EPS: f32 = 0.75;

type Rect = [f32; 4];

fn right(r: Rect) -> f32 {
    r[0] + r[2]
}
fn bottom(r: Rect) -> f32 {
    r[1] + r[3]
}
fn has_area(r: Rect) -> bool {
    r[2] > 0.5 && r[3] > 0.5
}
fn inside(inner: Rect, outer: Rect) -> bool {
    inner[0] >= outer[0] - EPS && inner[1] >= outer[1] - EPS && right(inner) <= right(outer) + EPS && bottom(inner) <= bottom(outer) + EPS
}
fn overlap(a: Rect, b: Rect) -> bool {
    a[0] < right(b) - 1.5 && b[0] < right(a) - 1.5 && a[1] < bottom(b) - 1.5 && b[1] < bottom(a) - 1.5
}

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn runtime(app: App, size: (f32, f32), dark: bool) -> Runtime<App> {
    let mut rt = Runtime::new(app, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.on_key = Some(neko_rune::on_key);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

/// Keys that are overlays or deliberate layers over their siblings: they sit on top by design.
fn overlay_key(key: &str) -> bool {
    // Hover overlays (a board card's action slot, a pull request row's actions over its end) and the Calendar's "now" line lie over their siblings by design.
    ["badge", "grip-", "scrim", "art", "hairline", "ring", "caret", "toast", "tip", "board-action-slot", "cal-now", "hover-actions", "pr-actions-"].iter().any(|p| key.starts_with(p))
}

/// What is wrong with the window as drawn now (empty when nothing). `label` names the state in each line.
fn crawl(rt: &Runtime<App>, label: &str) -> Vec<String> {
    let mut bad = Vec::new();
    let els = rt.elements();
    let (vw, vh) = rt.viewport;
    let by_key = |k: &str| els.iter().find(|e| e.key.as_deref() == Some(k) && !e.exiting);
    let layout = rt.state.layout;

    // a. the sessions column
    match by_key("sessions") {
        Some(s) => {
            if !layout.sessions.is_open() {
                bad.push(format!("{label}: (a) the sessions column is drawn though the person folded it"));
            } else if s.rect[2] < 231.5 || s.rect[2] > 360.5 {
                bad.push(format!("{label}: (a) the sessions column is {} wide, not 232 to 360", s.rect[2]));
            }
        }
        None => {
            if layout.sessions.is_open() {
                bad.push(format!("{label}: (a) the sessions column is missing"));
            }
        }
    }

    // b. the Neko column: its width, or the 44 point strip with the spark and a count
    match by_key("neko") {
        None => bad.push(format!("{label}: (b) the Neko column is missing")),
        Some(n) => {
            let w = n.rect[2];
            if (w - NEKO_STRIP).abs() < 0.5 {
                let spark = by_key("neko-expand");
                let count = by_key("neko-strip-count");
                match (spark, count) {
                    (Some(s), Some(c)) => {
                        if !(inside(s.rect, n.rect) && inside(c.rect, n.rect)) {
                            bad.push(format!("{label}: (b) the strip's spark or count is outside the strip"));
                        }
                        if c.rect[1] < bottom(s.rect) - EPS {
                            bad.push(format!("{label}: (b) the strip's count is not under the spark"));
                        }
                        if s.rect[1] > 24.0 {
                            bad.push(format!("{label}: (b) the strip's spark is not at the top ({})", s.rect[1]));
                        }
                    }
                    _ => bad.push(format!("{label}: (b) the Neko strip is a lone icon (spark {}, count {})", spark.is_some(), count.is_some())),
                }
            } else if !(279.5..=480.5).contains(&w) {
                bad.push(format!("{label}: (b) the Neko column is {w} wide: neither 280 to 480 nor the {NEKO_STRIP} strip"));
            }
        }
    }

    // The regions: the window's children, and what each may hold.
    let regions: Vec<(&str, Rect)> = els.iter().filter(|e| !e.exiting && e.path.starts_with("kit-root/window/") && e.path.matches('/').count() == 2 && has_area(e.rect)).map(|e| (e.key.as_deref().unwrap_or("?"), e.rect)).collect();
    let window = [0.0, 0.0, vw, vh];
    for (i, e) in els.iter().enumerate() {
        if e.exiting || e.virtual_extent || !e.path.starts_with("kit-root/window/") || !has_area(e.visible) {
            continue;
        }
        let name = e.key.clone().unwrap_or_else(|| e.path.rsplit('/').next().unwrap_or("").to_owned());
        // A board scrolls sideways and the context panel's viewers (code, tables, diffs) scroll both ways: what they clip is
        // not cut off, it is scrolled.
        let scrolls_sideways = e.path.contains("scroll-x") || e.path.contains("settings-nav") || e.path.contains("/context-body/") || e.path.contains("diff");
        // c. inside the window and inside its region
        if !inside(e.visible, window) {
            bad.push(format!("{label}: (c) {name} {:?} extends outside the window", e.visible));
            continue;
        }
        let region = e.path.split('/').nth(2).and_then(|k| regions.iter().find(|(key, _)| e.path.split('/').nth(2) == Some(*key) || *key == k));
        if let Some((key, rect)) = region
            && !inside(e.visible, *rect)
            && !overlay_key(&name)
            && !scrolls_sideways
        {
            bad.push(format!("{label}: (c) {name} {:?} extends outside its region {key} {:?}", e.visible, rect));
        }
        // e. text cut off horizontally by what holds it (a vertical clip is a scroll)
        if e.role == "text" && !scrolls_sideways && e.text.as_deref().is_some_and(|t| !t.trim().is_empty()) && has_area(e.rect) {
            let cut = e.visible[0] > e.rect[0] + 1.0 || right(e.visible) < right(e.rect) - 1.0;
            if cut {
                bad.push(format!("{label}: (e) text {:?} is cut off ({:?} shows {:?})", e.text.as_deref().unwrap_or("").chars().take(40).collect::<String>(), e.rect, e.visible));
            }
        }
        // f. a stopped state on screen
        // A plugin that stopped says so ("Stopped: file:line: why"); the scenes that break one on purpose are exempt.
        if e.role == "text"
            && let Some(t) = e.text.as_deref()
            && t.to_lowercase().starts_with("stopped:")
            && !label.starts_with("luau-")
        {
            bad.push(format!("{label}: (f) a stopped state is on screen: {t:?}"));
        }
        let _ = i;
    }

    // d. siblings that overlap partly
    let mut kids: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for (i, e) in els.iter().enumerate() {
        if let Some(p) = e.parent
            && !e.exiting
            && !e.virtual_extent
            && e.path.starts_with("kit-root/window/")
            && has_area(e.visible)
        {
            kids.entry(p).or_default().push(i);
        }
    }
    for (p, list) in kids {
        // A parent whose children are stacked (all in one place) is a layer stack, not a row.
        let first = els[list[0]].rect;
        if list.iter().all(|i| els[*i].rect == first) {
            continue;
        }
        for (n, &a) in list.iter().enumerate() {
            for &b in &list[n + 1..] {
                let (ea, eb) = (&els[a], &els[b]);
                let (ka, kb) = (ea.key.as_deref().unwrap_or(""), eb.key.as_deref().unwrap_or(""));
                if overlay_key(ka) || overlay_key(kb) || !has_area(ea.rect) || !has_area(eb.rect) {
                    continue;
                }
                if inside(ea.rect, eb.rect) || inside(eb.rect, ea.rect) {
                    continue;
                }
                if overlap(ea.visible, eb.visible) {
                    bad.push(format!("{label}: (d) siblings overlap under {}: {} {:?} and {} {:?}", els[p].key.as_deref().unwrap_or(&els[p].path), if ka.is_empty() { &ea.path } else { ka }, ea.rect, if kb.is_empty() { &eb.path } else { kb }, eb.rect));
                }
            }
        }
    }
    bad
}

fn scene_app(name: &str) -> App {
    let mut app = App::headless();
    let (_, f) = SCENES.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no scene {name}"));
    f(&mut app);
    app.outbox.clear();
    app
}

/// Scenes that put the window in a shape on purpose: the sessions column folded, and so on.
fn folds_sessions(name: &str) -> bool {
    name == "settings-narrow"
}

#[test]
fn every_scene_keeps_the_windows_shape() {
    let mut bad = Vec::new();
    for (name, _) in SCENES {
        for (size, dark) in SIZES.iter().map(|s| (*s, true)).chain([((1440.0, 900.0), false)]) {
            let app = scene_app(name);
            let rt = runtime(app, size, dark);
            let _ = folds_sessions(name);
            bad.extend(crawl(&rt, &format!("{name} @{}x{} {}", size.0, size.1, if dark { "dark" } else { "light" })));
        }
    }
    bad.sort();
    bad.dedup();
    assert!(bad.is_empty(), "{} structure failures:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn the_neko_strip_is_a_spark_over_a_count() {
    // Folded by the person at a width where it would otherwise stay open: the strip, not a lone icon.
    let mut app = scene_app("default");
    app.layout.set_neko_open(false);
    let rt = runtime(app, (1440.0, 900.0), true);
    let n = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("neko")).expect("the Neko region");
    assert_eq!(n.rect[2], NEKO_STRIP);
    let bad = crawl(&rt, "folded");
    assert!(bad.is_empty(), "{}", bad.join("\n"));
    let mut app = scene_app("live");
    app.layout.set_neko_open(false);
    let rt = runtime(app, (1440.0, 900.0), true);
    assert!(rt.elements().into_iter().any(|e| e.key.as_deref() == Some("neko-strip-count")));
}

#[test]
fn a_full_view_never_replaces_the_sessions_or_neko_column() {
    for view_ in FullView::ALL {
        let mut app = scene_app("live");
        app.open_view(view_);
        let rt = runtime(app, (1440.0, 900.0), true);
        let bad = crawl(&rt, &format!("{view_:?}"));
        assert!(bad.is_empty(), "{}", bad.join("\n"));
        for key in ["sessions", "neko"] {
            assert!(rt.elements().into_iter().any(|e| e.key.as_deref() == Some(key)), "{view_:?} lost {key}");
        }
    }
}

/// The copy of the owner's data: every state the brief names, through the real layout.
#[test]
fn the_copied_data_keeps_the_windows_shape() {
    let Some(socket) = std::env::var_os("NEKO_STRUCTURE_SOCKET").map(std::path::PathBuf::from) else {
        eprintln!("skipped: NEKO_STRUCTURE_SOCKET is not set (scripts/identity.sh sets it to the harness on a copy of the data)");
        return;
    };
    let fresh = || {
        let mut app = App::headless();
        app.clock = neko_rune::app::system_ms;
        app.now_ms = neko_rune::app::system_ms();
        app.dispatch = Dispatch::Blocking { socket: socket.clone() };
        for call in [Call::Refresh, Call::Unseen, Call::Inbox, Call::Digest, Call::RoutinesFull, Call::Models, Call::Appearance] {
            app.call(call);
        }
        app.ask_calendar();
        app
    };
    type Step = Box<dyn Fn(&mut App)>;
    let mut states: Vec<(String, Step)> = vec![("home".into(), Box::new(|_| {}))];
    let probe = fresh();
    // One session per status the copy has.
    let mut seen = std::collections::BTreeSet::new();
    for t in probe.summary.iter().flat_map(|m| m.tasks.iter()) {
        if seen.insert(format!("{:?}", t.status)) {
            let id = t.id.clone();
            states.push((format!("session-{:?}", t.status), Box::new(move |a| a.select(&id))));
        }
    }
    for v in FullView::ALL {
        if v != FullView::Setup && v != FullView::Settings {
            states.push((format!("{v:?}"), Box::new(move |a| a.open_view(v))));
        }
    }
    states.push(("all-sessions".into(), Box::new(|a| a.open_all_sessions())));
    for (name, layout) in [("board", neko_rune::board::Layout::Board), ("tree", neko_rune::board::Layout::Tree)] {
        states.push((format!("all-{name}"), Box::new(move |a| {
            a.open_all_sessions();
            a.all.include_stopped = true;
            a.set_all_layout(layout);
            a.all.layout_dirty = false;
        })));
    }
    for (name, mode) in [("day", CalMode::Day), ("week", CalMode::Week), ("month", CalMode::Month)] {
        states.push((format!("calendar-{name}"), Box::new(move |a| {
            a.open_view(FullView::Calendar);
            a.cal_mode(mode);
        })));
    }
    for s in SetupSection::ALL {
        states.push((format!("setup-{}", s.title()), Box::new(move |a| a.open_setup(s))));
    }
    for s in Section::ALL {
        states.push((format!("settings-{}", s.key()), Box::new(move |a| {
            a.open_view(FullView::Settings);
            a.pages.section = s;
        })));
    }
    states.push(("palette".into(), Box::new(|a| a.palette_toggle())));
    states.push(("shortcuts".into(), Box::new(|a| a.shortcuts_open = true)));
    states.push(("scope".into(), Box::new(|a| a.scope_open = true)));
    states.push(("model".into(), Box::new(|a| a.runtime_toggle())));
    states.push(("session-menu".into(), Box::new(|a| {
        if let Some(id) = a.summary.as_ref().and_then(|m| m.tasks.first()).map(|t| t.id.clone()) {
            a.select(&id);
        }
        a.session_menu = true;
    })));
    let mut bad = Vec::new();
    for (name, step) in &states {
        for size in &SIZES[..2] {
            let mut app = fresh();
            step(&mut app);
            let rt = runtime(app, *size, true);
            bad.extend(crawl(&rt, &format!("copy:{name} @{}x{}", size.0, size.1)));
        }
    }
    bad.sort();
    bad.dedup();
    assert!(bad.is_empty(), "{} structure failures on the copied data:\n{}", bad.len(), bad.join("\n"));
}
