//! Neko's native client on Rune, built beside the web UI (docs/decisions/0002-rune-for-everything.md).
//! P0 is the foundation: the pinned Rune, the read-only daemon link, the theme and the status
//! glyphs, and the four-region shell (sessions, the session, a context panel, Neko), with the
//! sessions and counts live from the daemon and the rest placeholders. No tabs.

pub mod account;
pub mod all_sessions;
pub mod activity;
pub mod app;
pub mod art;
pub mod art_gpu;
pub mod attachments;
pub mod clock;
pub mod connection;
pub mod groups;
pub mod icons;
pub mod lab;
pub mod motion;
pub mod layout;
pub mod link;
pub mod new_session;
pub mod pages;
pub mod permissions;
pub mod plan;
pub mod panel;
pub mod samples;
pub mod scenes;
pub mod readers;
pub mod runtime_pick;
pub mod scenes_pages;
pub mod scenes_panel;
pub mod status;
pub mod task_actions;
pub mod terminal;
pub mod toasts;
pub mod theme;
pub mod turns;
pub mod view;
pub mod viewers;
pub mod views;
pub mod web_link;
pub mod web_panel;
pub mod work;

use std::path::PathBuf;

use app::App;

/// Flags Rune itself takes for headless runs. A run with one of these never touches the
/// daemon, the browser or the window system.
const HEADLESS_FLAGS: [&str; 10] = ["--snap", "--tree", "--script", "--trace", "--strip", "--video", "--html", "--desktop", "--list-scenes", "--frames"];

/// Whether this process was started for a headless run (`--snap`, `--tree`, ...).
pub fn headless_run() -> bool {
    std::env::args().any(|a| HEADLESS_FLAGS.contains(&a.as_str()))
}

fn flag(name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    rune::extra_args().iter().find_map(|a| a.strip_prefix(&prefix).map(str::to_owned))
}

fn has(name: &str) -> bool {
    rune::extra_args().iter().any(|a| a == name)
}

fn init() -> App {
    if headless_run() {
        let mut app = App::headless();
        // A headless run against a test daemon (`NEKO_RUNE_SOCKET`): requests are answered on the
        // calling thread, so a scripted press sees its answer in the next frame.
        if let Some(socket) = std::env::var_os("NEKO_RUNE_SOCKET").map(PathBuf::from) {
            app.dispatch = app::Dispatch::Blocking { socket };
            app.call(link::Call::Refresh);
            app.call(link::Call::Unseen);
            app.call(link::Call::Inbox);
            app.call(link::Call::Digest);
            // Real time, so a task's age and the groups' 48 hour and 7 day edges are read as the person reads them.
            if app.summary.is_some() {
                app.clock = app::system_ms;
                app.now_ms = app::system_ms();
                // A window opens on Home: a session is chosen only when the snapshot asks (`NEKO_RUNE_OPEN_TASK`).
                app.call(link::Call::Refresh);
                app.call(link::Call::RoutinesFull);
                app.call(link::Call::Models);
                app.call(link::Call::Appearance);
                app.ask_calendar();
            }
            // For the panel's snapshots against a test daemon: open the task with this title, and show
            // this artifact as the daemon's `ShowArtifact` would (`scripts/snap-harness.sh`).
            if let Some(title) = std::env::var("NEKO_RUNE_OPEN_TASK").ok().filter(|t| !t.is_empty()) {
                let id = app.summary.as_ref().and_then(|m| m.tasks.iter().find(|t| t.title == title)).map(|t| t.id.clone());
                if let Some(id) = id {
                    app.select(&id);
                }
            }
            // The panel's tab for the pictures (`NEKO_RUNE_TAB=plan|info|terminal|diff|files|canvas`): opens the panel on it.
            if let Some(tab) = std::env::var("NEKO_RUNE_TAB").ok().and_then(|v| panel::Tab::ALL.into_iter().find(|t| t.title().eq_ignore_ascii_case(&v))) {
                app.panel_choose_tab(tab);
                app.panel_set_open(true);
            }
            // For the side-by-side pairs against the web UI (`/tmp/neko-polish`): the person's own name, and a full view.
            if std::env::var_os("NEKO_RUNE_ACCOUNT").is_some() {
                app.account = account::Account::load(|| {});
                // The pictures of the person's own Mac report what this Mac says (Settings, Permissions).
                app.access_probe = permissions::check;
            }
            if let Some(view) = std::env::var("NEKO_RUNE_VIEW").ok().and_then(|v| views::FullView::ALL.into_iter().find(|f| f.web_id() == v)) {
                app.open_view(view);
            }
            // The session header's menu open, for the pictures.
            if std::env::var_os("NEKO_RUNE_MENU").is_some() {
                app.session_menu = true;
            }
            // The Settings section for the pictures (`NEKO_RUNE_SECTION=permissions`).
            if let Some(section) = std::env::var("NEKO_RUNE_SECTION").ok().and_then(|v| pages::Section::ALL.into_iter().find(|x| x.key() == v)) {
                app.pages.section = section;
            }
            if let Some(id) = std::env::var("NEKO_RUNE_SHOW").ok().filter(|t| !t.is_empty()) {
                app.show_artifact(&id, None);
            }
        }
        app
    } else {
        // The last run's widths (read once, before the first frame); the window then keeps them.
        let saved = app::load_layout();
        let layout = saved.map(layout::Layout::restored).unwrap_or_default();
        App { linked: !has("nolink"), layout, layout_written: saved, layout_seen: saved, persist_layout: true, web_view: web_panel::WebHost::windowed(), ..App::default() }
    }
}

/// The daemon's socket: `socket=PATH`, else `NEKO_SOCKET`, else the one the daemon uses
/// (`$NEKO_DATA_DIR/neko.sock`, or the Application Support folder).
fn socket_path() -> PathBuf {
    flag("socket").or_else(|| std::env::var("NEKO_SOCKET").ok()).map(PathBuf::from).unwrap_or_else(neko_protocol::socket_path)
}

/// Keys the window handles before any field: ⌘K opens the palette; ⌘N starts a session.
pub fn on_key(s: &mut App, ev: &rune::KeyEvent, tx: &mut rune::Tx) -> bool {
    // The context panel's keys: Esc restores an expanded panel; ⌘+, ⌘−, ⌘0 zoom a PDF; W turns a model's wireframe on.
    if s.panel_on_key(ev) {
        return true;
    }
    // ⌘F in Settings goes to its search field, which filters the rows of every section.
    if ev.is_command('f') && !ev.mods.shift && s.view == Some(views::FullView::Settings) {
        tx.focus("settings-search", true);
        return true;
    }
    // All sessions: `/` focuses its search, J and K move, Return opens (never while a field has the keyboard).
    if s.all_key(ev, tx) {
        return true;
    }
    // ⌘⇧Esc: stop all work (asks first).
    if ev.mods.cmd && ev.mods.shift && ev.key == rune::Key::Escape {
        s.ask_stop_all();
        return true;
    }
    if ev.is_command('k') && !ev.mods.shift {
        s.palette_toggle();
        return true;
    }
    if ev.is_command('n') && !ev.mods.shift {
        s.open_new();
        return true;
    }
    false
}

/// Opens the window and, unless `nolink`, starts the read-only link to the running daemon.
pub fn run() {
    // `neko-rune probe [json] [socket=PATH]`: no window; connect, print, exit. For checking a daemon
    // (`json` prints the workbench summary alone, for `scripts/snap-harness.sh`).
    if std::env::args().nth(1).as_deref() == Some("probe") {
        let socket = std::env::args().find_map(|a| a.strip_prefix("socket=").map(PathBuf::from)).unwrap_or_else(|| std::env::var("NEKO_SOCKET").map(PathBuf::from).unwrap_or_else(|_| neko_protocol::socket_path()));
        std::process::exit(connection::probe(socket, neko_protocol::support_dir(), std::time::Duration::from_secs(15), std::env::args().any(|a| a == "json")));
    }
    // The wgpu backend: the 3D viewer draws with depth, which Rune's own Metal backend cannot (docs/knowledge/rune-p3a-the-context-panel.md).
    rune::prewarm_gpu_on(rune::GpuBackend::Wgpu);
    let spec = rune::AppSpec {
        scenes: scenes::SCENES,
        min_size: Some((960.0, 560.0)),
        // The web UI's font stacks (`theme::FONT_SANS` starts with Inter, `FONT_MONO` with `ui-monospace`).
        families: (theme::FONT_SANS, theme::FONT_MONO),
        on_key: Some(on_key),
        poll: Some(|s: &mut App, t: f64, tx: &mut rune::Tx| {
            let moved = s.tick_stream(t);
            s.tick_layout(t, tx);
            let panel = s.panel_tick(t, tx);
            let toast = s.tick_toast(t, tx);
            moved || panel || toast
        }),
        ..rune::AppSpec::new("Neko (Rune)", (1360.0, 840.0), init, view::view)
    };
    let app = rune::App::new(spec)
        .gpu_backend(rune::GpuBackend::Wgpu)
        .compositor(viewers::model3d_gpu::Viewer3d::default())
        .titlebar(rune::Titlebar::new(view::BAND).traffic_lights(view::TRAFFIC_LIGHTS.0, view::TRAFFIC_LIGHTS.1))
        .on_focus(|s: &mut App, focused: bool, _| s.set_focused(focused))
        // The web view (P3b) is built on the window's own handle, when first needed.
        .on_window(web_panel::remember_window);
    let handle = app.handle();
    let mut started = false;
    app.on_ready(move |_| {
        if std::mem::replace(&mut started, true) || has("nolink") {
            return;
        }
        // The window and the link thread share which task is open.
        let wanted = link::Wanted::default();
        let socket = socket_path();
        let (for_app, thread_handle) = (wanted.clone(), handle.clone());
        let dispatch = app::Dispatch::Async { socket: socket.clone(), handle: handle.clone() };
        // A viewer's heavy work (a decode, a PDF page, a CSV index) runs on a thread of its own; when it is
        // done the window builds its view again and the viewer picks the answer up.
        let waker = std::sync::Mutex::new(handle.clone());
        let wake: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(move || {
            waker.lock().unwrap_or_else(std::sync::PoisonError::into_inner).update_if(|_, tx| {
                tx.instant();
                true
            });
        });
        handle.update_if(move |s, _| {
            s.wanted = for_app;
            s.dispatch = dispatch;
            s.panel.states.use_threads(wake);
            false
        });
        connection::spawn(thread_handle, socket, neko_protocol::support_dir(), wanted);
    })
    .run();
}
