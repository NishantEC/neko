//! Neko's native client on Rune, built beside the web UI (docs/decisions/0002-rune-for-everything.md).
//! P0 is the foundation: the pinned Rune, the read-only daemon link, the theme and the status
//! glyphs, and the four-region shell (sessions, the session, a context panel, Neko), with the
//! sessions and counts live from the daemon and the rest placeholders. No tabs.

pub mod app;
pub mod attachments;
pub mod clock;
pub mod connection;
pub mod groups;
pub mod icons;
pub mod lab;
pub mod layout;
pub mod link;
pub mod new_session;
pub mod pages;
pub mod scenes;
pub mod scenes_pages;
pub mod status;
pub mod theme;
pub mod turns;
pub mod view;
pub mod views;
pub mod web_link;
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
            // The daemon's own times, so "5m" counts from the newest change in it.
            if let Some(newest) = app.summary.as_ref().and_then(|m| m.tasks.iter().map(|t| t.updated_at_ms).max()) {
                app.now_ms = newest + 60_000;
                app.selected = app.summary.as_ref().and_then(|m| groups::first_session(m, app.now_ms));
                app.wanted.set(app.selected.as_deref());
                app.call(link::Call::Refresh);
            }
        }
        app
    } else {
        // The last run's widths (read once, before the first frame); the window then keeps them.
        let saved = app::load_layout();
        let layout = saved.map(layout::Layout::restored).unwrap_or_default();
        App { linked: !has("nolink"), layout, layout_written: saved, persist_layout: true, ..App::default() }
    }
}

/// The daemon's socket: `socket=PATH`, else `NEKO_SOCKET`, else the one the daemon uses
/// (`$NEKO_DATA_DIR/neko.sock`, or the Application Support folder).
fn socket_path() -> PathBuf {
    flag("socket").or_else(|| std::env::var("NEKO_SOCKET").ok()).map(PathBuf::from).unwrap_or_else(neko_protocol::socket_path)
}

/// Keys the window handles before any field: ⌘K opens the palette; ⌘N starts a session.
fn on_key(s: &mut App, ev: &rune::KeyEvent, tx: &mut rune::Tx) -> bool {
    // ⌘F in Settings goes to its search field, which filters the rows of every section.
    if ev.is_command('f') && !ev.mods.shift && s.view == Some(views::FullView::Settings) {
        tx.focus("settings-search", true);
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
    rune::prewarm_gpu();
    let spec = rune::AppSpec {
        scenes: scenes::SCENES,
        min_size: Some((960.0, 560.0)),
        on_key: Some(on_key),
        poll: Some(|s: &mut App, t: f64, tx: &mut rune::Tx| {
            let moved = s.tick_stream(t);
            s.tick_layout(t, tx);
            moved
        }),
        ..rune::AppSpec::new("Neko (Rune)", (1360.0, 840.0), init, view::view)
    };
    let app = rune::App::new(spec).titlebar(rune::Titlebar::new(44.0)).on_focus(|s: &mut App, focused: bool, _| s.set_focused(focused));
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
        handle.update_if(move |s, _| {
            s.wanted = for_app;
            s.dispatch = dispatch;
            false
        });
        connection::spawn(thread_handle, socket, neko_protocol::support_dir(), wanted);
    })
    .run();
}
