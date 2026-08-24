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
            neko_core::icons::ensure_cached_icon(neko_core::settings::SETTINGS_APP_ICON_ID, std::path::Path::new(neko_core::settings::SETTINGS_APP_PATH));
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

    {
        // Ambient awareness: keeps the permission inbox warm so the panel
        // already knows when it opens, and pushes the count to every client
        // so an agent that blocks while the panel is hidden still shows up —
        // on the Dock tile. See `server::run_attention_poll`.
        let state = state.clone();
        std::thread::spawn(move || server::run_attention_poll(state));
    }

    eprintln!("neko-daemon: listening on {}", socket_path.display());
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let state = state.clone();
        std::thread::spawn(move || server::handle_connection(state, stream));
    }
}
