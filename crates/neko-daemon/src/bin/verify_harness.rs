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

// **Dead code here is expected, not a smell.** This harness hosts the real
// `server` module but starts neither clipboard capture (the whole reason it
// exists) nor Paseo/quota/application pollers, since an evidence run must not
// reach the captain's pasteboard, Paseo, or three vendor APIs. It starts only
// the local Codex actor below, against a synthetic stdio executable. The poll
// functions are therefore genuinely uncalled *in this binary* and perfectly
// live in the real daemon.
#[allow(dead_code)]
#[path = "../codex.rs"]
mod codex;
#[path = "../mcp_host.rs"]
mod mcp_host;
#[path = "../server.rs"]
mod server;
#[path = "../workbench.rs"]
mod workbench;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use neko_core::Db;

fn main() {
    if let Err(error) = verify_environment() {
        eprintln!("verify-harness: refusing to start: {error}");
        std::process::exit(2);
    }

    let socket_path = neko_protocol::socket_path();
    let db_path = neko_protocol::database_path();

    let listener = match server::bind_singleton(&socket_path) {
        Ok(Some(listener)) => listener,
        Ok(None) => {
            eprintln!("verify-harness: another instance is already running, exiting");
            return;
        }
        Err(e) => {
            eprintln!(
                "verify-harness: failed to bind {}: {e}",
                socket_path.display()
            );
            std::process::exit(1);
        }
    };

    let db = Db::open(&db_path).unwrap_or_else(|e| {
        eprintln!(
            "verify-harness: failed to open database at {}: {e}",
            db_path.display()
        );
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
    if let Some(count) = std::env::var("NEKO_VERIFY_SEED_CLIPBOARD_COUNT")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
    {
        for i in 0..count {
            let _ = db.record_clipboard_entry(
                &format!(
                    "neko-frost edge-fade verification fixture #{i}: a distinct clipboard entry"
                ),
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

    // Exercise the same one-child Codex supervision seam as the shipped
    // daemon without starting its systemwide clipboard-capture loop. The
    // evidence recipe supplies only a synthetic local stdio executable via
    // `NEKO_CODEX_PATH`; this harness is the sole place that combination is
    // allowed for verification.
    codex::spawn(state.codex.clone(), state.clone());

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

/// Refuse before resolving neko's HOME-derived paths or starting any actor.
/// A verification run must explicitly contain both its isolated HOME and its
/// synthetic Codex executable under one disposable fixture root; otherwise a
/// cargo-discovered binary could silently touch the user's live daemon state.
fn verify_environment() -> Result<(), String> {
    let fixture_root = std::env::var_os("NEKO_VERIFY_FIXTURE_ROOT")
        .map(PathBuf::from)
        .ok_or_else(|| {
            "set NEKO_VERIFY_FIXTURE_ROOT to a disposable fixture directory".to_string()
        })?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME must point to an isolated fixture directory".to_string())?;
    let codex_path = std::env::var_os("NEKO_CODEX_PATH")
        .map(PathBuf::from)
        .ok_or_else(|| "set NEKO_CODEX_PATH to the synthetic Codex fixture".to_string())?;
    verify_environment_paths(&fixture_root, &home, &codex_path)
}

fn verify_environment_paths(
    fixture_root: &Path,
    home: &Path,
    codex_path: &Path,
) -> Result<(), String> {
    let fixture_root = fixture_root
        .canonicalize()
        .map_err(|_| "NEKO_VERIFY_FIXTURE_ROOT must name an existing directory".to_string())?;
    let home = home
        .canonicalize()
        .map_err(|_| "HOME must name an existing isolated fixture directory".to_string())?;
    let codex_path = codex_path
        .canonicalize()
        .map_err(|_| "NEKO_CODEX_PATH must name the synthetic Codex fixture".to_string())?;

    if home == fixture_root || !home.starts_with(&fixture_root) {
        return Err(
            "HOME must be a dedicated directory inside NEKO_VERIFY_FIXTURE_ROOT".to_string(),
        );
    }
    if !codex_path.starts_with(&fixture_root) || !codex_path.is_file() {
        return Err(
            "NEKO_CODEX_PATH must be a fixture executable inside NEKO_VERIFY_FIXTURE_ROOT"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_a_home_outside_the_fixture_before_any_daemon_state_can_be_opened() {
        let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let error = verify_environment_paths(
            &fixture_root,
            Path::new("/"),
            &fixture_root.join("src/bin/verify_harness.rs"),
        )
        .unwrap_err();

        assert!(error.contains("HOME must be a dedicated directory"));
    }
}
