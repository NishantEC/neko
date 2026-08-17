mod server;

use std::sync::Arc;

use neko_core::Db;
use server::AppState;

fn main() {
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
        eprintln!("neko-daemon: failed to open database at {}: {e}", db_path.display());
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
            let apps = state.apps.read().unwrap().clone();
            for app in apps {
                neko_core::icons::ensure_cached_icon(&app.id, &app.path);
            }
        });
    }

    eprintln!("neko-daemon: listening on {}", socket_path.display());
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let state = state.clone();
        std::thread::spawn(move || server::handle_connection(&state, stream));
    }
}
