//! Verification-only daemon harness for `fm/neko-double-panel` — never part
//! of the shipped app, never wired into `neko`'s own daemon-launch path.
//!
//! Standing rule (see the launch brief this task ran under, and `AGENTS.md`
//! going forward): the real `neko-daemon` binary must never be launched for
//! verification, even under an isolated `HOME`, because its clipboard
//! capture loop (`neko_core::clipboard::run_capture_loop`) polls the
//! *systemwide* pasteboard regardless of `HOME` and would record whatever
//! the captain has really copied. This harness reuses `neko-daemon`'s own
//! `server` module (real app index, real providers, real socket protocol)
//! but never starts that capture loop — a clipboard fixture is seeded
//! directly into the isolated SQLite database instead, via
//! `record_clipboard_entry`, before the socket ever accepts a connection.
//!
//! Delete this file once the investigation it was built for is closed.

#[path = "../server.rs"]
mod server;

use std::sync::Arc;

use neko_core::Db;

fn main() {
    let socket_path = neko_protocol::socket_path();
    let db_path = neko_protocol::database_path();

    let listener = match server::bind_singleton(&socket_path) {
        Ok(Some(listener)) => listener,
        Ok(None) => {
            eprintln!("verify-harness: another instance is already running, exiting");
            return;
        }
        Err(e) => {
            eprintln!("verify-harness: failed to bind {}: {e}", socket_path.display());
            std::process::exit(1);
        }
    };

    let db = Db::open(&db_path).unwrap_or_else(|e| {
        eprintln!("verify-harness: failed to open database at {}: {e}", db_path.display());
        std::process::exit(1);
    });

    // Obscure hotkey, committed before any client connects — per the
    // launch brief's standing rule, so nothing anyone could realistically
    // press lands on this isolated instance.
    let _ = neko_core::hotkey::set_hotkey(
        &db,
        neko_protocol::HotkeyCombo::new(
            vec![
                neko_protocol::Modifier::Cmd,
                neko_protocol::Modifier::Alt,
                neko_protocol::Modifier::Ctrl,
                neko_protocol::Modifier::Shift,
            ],
            "F13",
        ),
        server::now_unix_ms(),
    );

    // A seeded fixture, not a real capture — see this file's own doc
    // comment for why the real capture loop never runs here.
    let _ = db.record_clipboard_entry(
        "neko-double-panel verification fixture: a short clipboard entry",
        "text",
        Some("Terminal"),
        server::now_unix_ms(),
        200,
    );

    // `NEKO_VERIFY_SEED_CLIPBOARD_COUNT=<n>` — added for `fm/neko-frost`'s
    // edge-fade verification (`crates/neko/src/edge_fade.rs`): seeds `n`
    // more distinct fixtures (still `record_clipboard_entry`, still not a
    // real capture) so the clipboard-history mode list genuinely has more
    // entries than fit `panel::CONTENT_AREA_MIN_HEIGHT_PX`, which is what
    // makes the edge fade non-decorative to screenshot — a single fixture
    // (the one above) is the "short list, no fade" case on its own; this is
    // the "content scrolled out of view" case. Unset by default, same
    // pattern as every other verification-only env var in this codebase.
    if let Some(count) = std::env::var("NEKO_VERIFY_SEED_CLIPBOARD_COUNT").ok().and_then(|v| v.parse::<u32>().ok()) {
        for i in 0..count {
            let _ = db.record_clipboard_entry(
                &format!("neko-frost edge-fade verification fixture #{i}: a distinct clipboard entry"),
                "text",
                Some("Terminal"),
                server::now_unix_ms() - i as i64,
                200,
            );
        }
    }

    // `NEKO_VERIFY_SEED_MARKDOWN=1` — a fixture whose content is markdown,
    // for photographing the client's markdown renderer
    // (`crates/neko/src/markdown.rs`, driven by `NEKO_FORCE_PREVIEW_MARKDOWN`
    // on the client side). The real markdown rows (`conversation`) need a
    // live Paseo daemon an isolated `HOME` cannot have; a clipboard fixture
    // through the same render path is the photographable stand-in. Newest
    // fixture wins the mode's top slot, so this seeds with a fresher
    // timestamp than the default fixture above.
    if std::env::var_os("NEKO_VERIFY_SEED_MARKDOWN").is_some() {
        let _ = db.record_clipboard_entry(
            "## What changed\n\nAll **60 documents** now written, and the `node:` field is wired in. See [the plan](https://example.com/plan).\n\n- fixed the ingest script\n- re-ran `pnpm ingest-drive`\n  - twice, the first hit a stale cache\n\n```sh\npnpm ingest-drive --limit 0 | tail -3\n```\n\n> Not yet done: the granth UI.\n\n1. verify the drive KB\n2. ship it",
            "text",
            Some("Terminal"),
            server::now_unix_ms() + 5_000,
            200,
        );
    }

    eprintln!("verify-harness: scanning installed applications…");
    let apps = neko_core::apps::scan_applications();
    eprintln!("verify-harness: indexed {} applications", apps.len());

    let state = Arc::new(server::AppState::new(db, apps));

    {
        let state = state.clone();
        std::thread::spawn(move || {
            neko_core::icons::purge_stale_icon_cache();
            neko_core::icons::ensure_cached_icon(
                neko_core::settings::SETTINGS_APP_ICON_ID,
                std::path::Path::new(neko_core::settings::SETTINGS_APP_PATH),
            );
            let apps = state.apps.read().unwrap().clone();
            for app in apps {
                neko_core::icons::ensure_cached_icon(&app.id, &app.path);
            }
            server::notify_icons_updated(&state);
        });
    }

    // Deliberately no `neko_core::clipboard::run_capture_loop` thread, and
    // no `watch_applications` live-update thread (nothing in this
    // investigation depends on either), per this file's own doc comment.

    eprintln!("verify-harness: listening on {}", socket_path.display());
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let state = state.clone();
        std::thread::spawn(move || server::handle_connection(state, stream));
    }
}
