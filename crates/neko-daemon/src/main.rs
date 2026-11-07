mod codex;
mod mcp_host;
mod routines;
mod self_update;
mod server;
mod web;
mod workbench;

use std::sync::Arc;

use neko_core::Db;
use server::AppState;

/// How to write a plugin (`docs/design/plugin-guide.md`), for agents on any Mac.
const PLUGIN_GUIDE: &str = include_str!("../../../docs/design/plugin-guide.md");

/// `neko-daemon --open-page <route>`: opens Neko's web app signed in at
/// `#/<route>` in the default browser, so an agent can load what it built.
fn open_page(route: &str) -> i32 {
    if !route.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/')) {
        eprintln!("a route is letters, digits, -, _ and /");
        return 2;
    }
    let token = std::fs::read_to_string(neko_protocol::support_dir().join("web-token")).unwrap_or_default();
    let token = token.trim();
    if token.len() != 64 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
        eprintln!("Neko's web interface has not published its address yet; is the daemon running?");
        return 1;
    }
    let port = std::env::var("NEKO_WEB_PORT").ok().and_then(|p| p.trim().parse::<u16>().ok()).unwrap_or(4747);
    let url = format!("http://127.0.0.1:{port}/?token={token}#/{route}");
    match std::process::Command::new("/usr/bin/open").arg(&url).status() {
        Ok(status) if status.success() => 0,
        _ => {
            eprintln!("could not open the browser");
            1
        }
    }
}

/// `neko-daemon --protocol` prints these: every `Request`, `Response` and
/// workbench `Command` with their fields.
const PROTOCOL: &[(&str, &str)] = &[
    ("neko-protocol/src/lib.rs (Request, Response, Event)", include_str!("../../neko-protocol/src/lib.rs")),
    ("neko-protocol/src/workbench.rs (Command, Snapshot)", include_str!("../../neko-protocol/src/workbench.rs")),
    ("neko-protocol/src/routines.rs (Routine, Cadence, RoutineRun)", include_str!("../../neko-protocol/src/routines.rs")),
];

/// `neko-daemon --request '<json>'`: sends one `Request` to the running
/// daemon and prints its `Response` as JSON (exit 1 for an error). This is
/// how agents, Neko's own chat included, manage Neko from a shell.
fn request_if_asked() -> Option<i32> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--request") => {}
        Some("--protocol") => {
            // The request shapes, from the sources this build was made from,
            // so an agent on any Mac can read them without a checkout.
            use std::io::Write;
            // A reader that stops early (`| head`) is fine; never panic on it.
            let mut out = std::io::stdout().lock();
            for (name, text) in PROTOCOL {
                if writeln!(out, "===== {name} =====\n{text}").is_err() {
                    break;
                }
            }
            return Some(0);
        }
        Some("--plugin-guide") => {
            use std::io::Write;
            // The guide, then what is installed on this Mac right now, so an
            // agent writing a viewer sees the kinds and fences already taken.
            let neko = std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| "neko-daemon".into());
            let installed = neko_core::viewers::installed_section(&neko, &neko_core::viewers::plugins_dir());
            let _ = write!(std::io::stdout().lock(), "{PLUGIN_GUIDE}\n## 8. On this Mac now\n\n{installed}\n");
            return Some(0);
        }
        Some("--open-page") => return Some(open_page(&args.next().unwrap_or_default())),
        _ => return None,
    }
    let Some(text) = args.next() else {
        eprintln!("usage: neko-daemon --request '<json Request>'");
        return Some(2);
    };
    let request: neko_protocol::Request = match serde_json::from_str(&text) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("not a Neko request: {error}");
            return Some(2);
        }
    };
    let run = || -> std::io::Result<neko_protocol::Response> {
        let stream = std::os::unix::net::UnixStream::connect(neko_protocol::socket_path())?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(120)))?;
        neko_protocol::write_frame(&stream, &neko_protocol::Frame::Request { id: 1, request })?;
        loop {
            match neko_protocol::read_frame(&stream)? {
                Some(neko_protocol::Frame::Response { id: 1, response }) => return Ok(response),
                Some(_) => continue,
                None => return Err(std::io::Error::other("the daemon closed the connection")),
            }
        }
    };
    match run() {
        Ok(response) => {
            println!("{}", serde_json::to_string(&response).unwrap_or_default());
            Some(if matches!(response, neko_protocol::Response::Error { .. }) { 1 } else { 0 })
        }
        Err(error) => {
            eprintln!("could not reach the Neko daemon: {error}");
            Some(1)
        }
    }
}

fn main() {
    if neko_core::mcp_host::bridge::run_if_requested() {
        return;
    }
    if neko_core::native_runner::run_guard_if_requested() {
        return;
    }
    if let Some(code) = neko_core::machine::check_if_requested() {
        std::process::exit(code);
    }
    if let Some(code) = request_if_asked() {
        std::process::exit(code);
    }
    let socket_path = neko_protocol::socket_path();
    let db_path = neko_protocol::database_path();

    let listener = match server::bind_singleton(&socket_path) {
        Ok(Some(listener)) => listener,
        Ok(None) => {
            eprintln!("neko-daemon: another instance is already running, exiting");
            return;
        }
        Err(e) => {
            eprintln!("neko-daemon: failed to bind {}: {e}", socket_path.display());
            std::process::exit(1);
        }
    };

    let db = Db::open(&db_path).unwrap_or_else(|e| {
        eprintln!(
            "neko-daemon: failed to open database at {}: {e}",
            db_path.display()
        );
        std::process::exit(1);
    });

    eprintln!("neko-daemon: scanning installed applications…");
    let apps = neko_core::apps::scan_applications();
    eprintln!("neko-daemon: indexed {} applications", apps.len());

    let state = Arc::new(AppState::new(db, apps));

    // Icon extraction is real AppKit work per app (tens of ms each) — do it
    // after the index is already searchable, not before, so the daemon's
    // own startup never delays the first search a client can make.
    {
        let state = state.clone();
        std::thread::spawn(move || {
            // One-time migration off any previous cache generation — see
            // `purge_stale_icon_cache`'s own doc comment. Cheap (a single
            // `read_dir` over at most one app index's worth of files), but
            // still kept off the daemon's own startup path, same as the
            // extraction loop below.
            neko_core::icons::purge_stale_icon_cache();
            // The one icon every `settings::SettingsProvider` row shares
            // (see that module's doc comment, "Icon" section) — extracted
            // first, before the per-app loop below, so it's warm well
            // before a captain's first settings-pane search even on a cold
            // cache; a single `NSWorkspace.iconForFile` call is a few tens
            // of ms, not the "tens of ms *times 146 apps*" cost the rest of
            // this pass exists to keep off the daemon's own startup path.
            neko_core::icons::ensure_cached_icon(
                neko_core::settings::SETTINGS_APP_ICON_ID,
                std::path::Path::new(neko_core::settings::SETTINGS_APP_PATH),
            );
            // Verification-only, unset (0ms) in normal operation — the
            // real per-app extraction cost is small enough on real
            // hardware that a fresh index finishes in well under a second,
            // making the cold-cache window this exists to demonstrate hard
            // to land a screenshot inside without artificially stretching
            // it out. Same inert-by-default pattern as `evidence.rs`'s
            // `NEKO_BENCH`/`NEKO_FORCE_MATERIAL`.
            let extract_delay_ms: u64 = std::env::var("NEKO_ICON_EXTRACT_DELAY_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let apps = state.apps.read().unwrap().clone();
            for app in apps {
                if extract_delay_ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(extract_delay_ms));
                }
                neko_core::icons::ensure_cached_icon(&app.id, &app.path);
            }
            // Pushed once after the whole startup batch, not per-icon: a
            // client watching this event only needs to know "something is
            // worth re-asking for," and one notification per batch is
            // enough for `Root::refresh_icons` to pick up every icon that
            // finished, cheaply, on its own next opportunity.
            server::notify_icons_updated(&state);
        });
    }

    // Keeps the index live for the daemon's whole lifetime: installs,
    // moves, and removals are reflected without a restart — see
    // `neko_core::apps`'s module doc comment for the mechanism. Runs in
    // its own background thread; `watch_applications` returns immediately.
    {
        let state = state.clone();
        neko_core::apps::watch_applications(move |apps| {
            *state.apps.write().unwrap() = apps.clone();
            // Same reasoning as the startup pass above: a newly-appeared
            // app should get a real icon without waiting for a restart,
            // and this is a no-op for anything already cached.
            for app in apps {
                neko_core::icons::ensure_cached_icon(&app.id, &app.path);
            }
            server::notify_icons_updated(&state);
        });
    }

    // The clipboard capture loop is resident for the daemon's whole
    // lifetime, independent of any client connection — see
    // `neko_core::clipboard`'s module doc comment for why this is safe to
    // run headlessly.
    {
        let state = state.clone();
        std::thread::spawn(move || neko_core::clipboard::run_capture_loop(&state.db));
    }

    state.workbench.start();
    {
        let state = state.clone();
        std::thread::spawn(move || server::run_native_attention_poll(state));
    }

    // Historical task browsers are available only when explicitly opted in.
    // Normal Neko operation owns its own tasks and never opens those stores.
    if std::env::var_os("NEKO_LEGACY_AGENTS").is_some() {
        codex::spawn(state.codex.clone(), state.clone());

        {
            // Ambient awareness: keeps the permission inbox warm so the panel
            // already knows when it opens, and pushes the count to every client
            // so an agent that blocks while the panel is hidden still shows up —
            // on the Dock tile. See `server::run_attention_poll`.
            let state = state.clone();
            std::thread::spawn(move || server::run_attention_poll(state));
        }

        {
            // The other half of ambient awareness: what is left to spend. Its own
            // thread and its own cadence — see `server::run_quota_poll` for why
            // this cannot ride along with the attention poll above.
            let state = state.clone();
            std::thread::spawn(move || server::run_quota_poll(state));
        }
    }

    // Routines: recurring scoped runs that act and report (`routines.rs`).
    routines::spawn(state.clone());
    server::spawn_web_bridge(state.clone());
    // Neko applying landed changes to itself (see `self_update.rs`).
    self_update::spawn(state.clone());

    eprintln!("neko-daemon: listening on {}", socket_path.display());
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            // Out of descriptors or similar: back off instead of spinning.
            std::thread::sleep(std::time::Duration::from_millis(100));
            continue;
        };
        let state = state.clone();
        std::thread::spawn(move || server::handle_connection(state, stream));
    }
}
