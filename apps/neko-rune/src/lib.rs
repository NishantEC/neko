//! Neko's native client on Rune, built beside the web UI (docs/decisions/0002-rune-for-everything.md).
//! P0 is the foundation: the pinned Rune, the read-only daemon link, the theme and the status
//! glyphs, and one plain placeholder view. The layout (sessions, an agent workspace, Neko as a side
//! agent, contextual panels) is pending an owner decision; tabs and a rail are dropped.

pub mod app;
pub mod connection;
pub mod lab;
pub mod link;
pub mod scenes;
pub mod status;
pub mod theme;
pub mod view;
pub mod web_link;

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
        App::headless()
    } else {
        App { linked: !has("nolink"), ..App::default() }
    }
}

/// The daemon's socket: `socket=PATH`, else `NEKO_SOCKET`, else the one the daemon uses
/// (`$NEKO_DATA_DIR/neko.sock`, or the Application Support folder).
fn socket_path() -> PathBuf {
    flag("socket").or_else(|| std::env::var("NEKO_SOCKET").ok()).map(PathBuf::from).unwrap_or_else(neko_protocol::socket_path)
}

/// Opens the window and, unless `nolink`, starts the read-only link to the running daemon.
pub fn run() {
    // `neko-rune probe [socket=PATH]`: no window; connect, print, exit. For checking a daemon.
    if std::env::args().nth(1).as_deref() == Some("probe") {
        let socket = std::env::args().find_map(|a| a.strip_prefix("socket=").map(PathBuf::from)).unwrap_or_else(|| std::env::var("NEKO_SOCKET").map(PathBuf::from).unwrap_or_else(|_| neko_protocol::socket_path()));
        std::process::exit(connection::probe(socket, neko_protocol::support_dir(), std::time::Duration::from_secs(15)));
    }
    rune::prewarm_gpu();
    let spec = rune::AppSpec {
        scenes: scenes::SCENES,
        min_size: Some((720.0, 480.0)),
        ..rune::AppSpec::new("Neko (Rune)", (1180.0, 760.0), init, view::view)
    };
    let app = rune::App::new(spec).titlebar(rune::Titlebar::new(44.0));
    let handle = app.handle();
    let mut started = false;
    app.on_ready(move |_| {
        if std::mem::replace(&mut started, true) || has("nolink") {
            return;
        }
        connection::spawn(handle.clone(), socket_path(), neko_protocol::support_dir());
    })
    .run();
}
