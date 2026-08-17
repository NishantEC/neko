use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use neko_core::{AppEntry, Db};
use neko_protocol::{Event, Frame, Request, Response, read_frame, write_frame};

pub struct AppState {
    pub db: Mutex<Db>,
    pub apps: RwLock<Vec<AppEntry>>,
    broadcast: Mutex<Vec<UnixStream>>,
}

impl AppState {
    pub fn new(db: Db, apps: Vec<AppEntry>) -> Self {
        Self {
            db: Mutex::new(db),
            apps: RwLock::new(apps),
            broadcast: Mutex::new(Vec::new()),
        }
    }
}

pub fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Binds the daemon's socket, taking over a stale one if the process that
/// created it is gone. Returns `None` (rather than an error) when a live
/// daemon already answers on this socket — that's the normal "already
/// running" case, not a failure.
pub fn bind_singleton(socket_path: &Path) -> io::Result<Option<UnixListener>> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if socket_path.exists() {
        match UnixStream::connect(socket_path) {
            Ok(mut stream) => {
                if ping(&mut stream).is_ok() {
                    return Ok(None);
                }
                // Connected but didn't answer a ping like a neko-daemon
                // would — treat as stale rather than trusting it's alive.
                std::fs::remove_file(socket_path)?;
            }
            Err(_) => {
                // Nothing is listening; the file is left over from a
                // process that didn't clean up on exit.
                std::fs::remove_file(socket_path)?;
            }
        }
    }
    Ok(Some(UnixListener::bind(socket_path)?))
}

fn ping(stream: &mut UnixStream) -> io::Result<()> {
    write_frame(&mut *stream, &Frame::Request { id: 0, request: Request::Ping })?;
    match read_frame(&mut *stream)? {
        Some(Frame::Response { response: Response::Pong, .. }) => Ok(()),
        _ => Err(io::Error::other("unexpected reply to ping")),
    }
}

pub fn handle_connection(state: &AppState, stream: UnixStream) {
    let writer = match stream.try_clone() {
        Ok(w) => w,
        Err(_) => return,
    };
    state.broadcast.lock().unwrap().push(writer);

    loop {
        match read_frame(&stream) {
            Ok(Some(Frame::Request { id, request })) => {
                let response = handle_request(state, request);
                let mut writer = match stream.try_clone() {
                    Ok(w) => w,
                    Err(_) => return,
                };
                if write_frame(&mut writer, &Frame::Response { id, response }).is_err() {
                    return;
                }
            }
            Ok(Some(_)) => {} // Clients never send Response/Event frames.
            Ok(None) | Err(_) => return,
        }
    }
}

fn handle_request(state: &AppState, request: Request) -> Response {
    match request {
        Request::Ping => Response::Pong,

        Request::Search { query, limit } => {
            let limit = limit.clamp(1, 50);
            let now = now_unix_ms();

            let apps = state.apps.read().unwrap();
            let (recency, clipboard_entries) = {
                let db = state.db.lock().unwrap();
                (
                    db.recency().unwrap_or_default(),
                    neko_core::clipboard::entries(&db).unwrap_or_default(),
                )
            };

            // Rank clipboard first so we know whether to reserve it a slot
            // — apps are still the primary result type, but must never
            // crowd clipboard out of the *response* entirely when there's
            // a matching entry (the panel's own `fit_within_budget` makes
            // the final call on how many of each actually render).
            let clipboard_matches = neko_core::search::rank_clipboard(&query, &clipboard_entries, now, limit);
            let app_limit = if clipboard_matches.is_empty() {
                limit
            } else {
                limit.saturating_sub(1)
            };
            let mut items = neko_core::search::rank_apps(&query, &apps, &recency, now, app_limit);
            let remaining = limit.saturating_sub(items.len());
            items.extend(clipboard_matches.into_iter().take(remaining));
            Response::SearchResults { items }
        }

        Request::Launch { id } => {
            let app_path = {
                let apps = state.apps.read().unwrap();
                apps.iter().find(|a| a.id == id).map(|a| a.path.clone())
            };
            let Some(app_path) = app_path else {
                return Response::Error {
                    message: format!("no such app: {id}"),
                };
            };
            match neko_core::launch::launch_app(&app_path) {
                Ok(()) => {
                    let _ = state.db.lock().unwrap().record_launch(&id, now_unix_ms());
                    Response::Launched
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::Paste { id } => {
            // No explicit DB touch here: writing `id` back onto the
            // pasteboard bumps the OS `changeCount`, which the capture loop
            // (already polling in the background — see `main.rs`) picks up
            // on its own next tick and re-records with a fresh timestamp,
            // the same "move to top" dedup path an ordinary re-copy takes.
            if neko_core::clipboard::write_to_pasteboard(&id) {
                Response::Pasted
            } else {
                Response::Error {
                    message: "failed to write to the pasteboard".to_string(),
                }
            }
        }

        Request::GetHotkey => {
            let db = state.db.lock().unwrap();
            match neko_core::hotkey::get_hotkey(&db) {
                Ok(config) => Response::Hotkey { config },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::CheckHotkeyConflict { candidate } => Response::HotkeyConflict {
            reason: neko_core::hotkey::check_known_conflict(&candidate),
        },

        Request::CommitHotkey { candidate } => {
            let result = {
                let db = state.db.lock().unwrap();
                neko_core::hotkey::set_hotkey(&db, candidate, now_unix_ms())
            };
            match result {
                Ok(config) => {
                    broadcast(state, &Event::HotkeyChanged { config: config.clone() });
                    Response::Hotkey { config }
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::GetOnboardingState => {
            let db = state.db.lock().unwrap();
            match neko_core::onboarding::get_onboarding_state(&db) {
                Ok(s) => onboarding_response(s),
                Err(e) => error_response(e),
            }
        }

        Request::SetOnboardingComplete { completed } => {
            let db = state.db.lock().unwrap();
            match neko_core::onboarding::set_onboarding_completed(&db, completed) {
                Ok(s) => onboarding_response(s),
                Err(e) => error_response(e),
            }
        }

        Request::DismissAccessibilityBanner => {
            let db = state.db.lock().unwrap();
            match neko_core::onboarding::dismiss_accessibility_banner(&db) {
                Ok(s) => onboarding_response(s),
                Err(e) => error_response(e),
            }
        }

        Request::GetClipboardHistoryEnabled => {
            let db = state.db.lock().unwrap();
            match neko_core::onboarding::get_clipboard_history_enabled(&db) {
                Ok(enabled) => Response::ClipboardHistoryEnabled { enabled },
                Err(e) => error_response(e),
            }
        }

        Request::SetClipboardHistoryEnabled { enabled } => {
            let db = state.db.lock().unwrap();
            match neko_core::onboarding::set_clipboard_history_enabled(&db, enabled) {
                Ok(enabled) => Response::ClipboardHistoryEnabled { enabled },
                Err(e) => error_response(e),
            }
        }
    }
}

fn onboarding_response(state: neko_core::onboarding::OnboardingState) -> Response {
    Response::OnboardingState {
        completed: state.completed,
        accessibility_banner_dismissed: state.accessibility_banner_dismissed,
    }
}

fn error_response(e: impl std::fmt::Display) -> Response {
    Response::Error {
        message: e.to_string(),
    }
}

fn broadcast(state: &AppState, event: &Event) {
    let mut writers = state.broadcast.lock().unwrap();
    writers.retain_mut(|w| write_frame(w, &Frame::Event(event.clone())).is_ok());
}

/// Called by `main.rs`'s background icon-extraction passes (startup and
/// each live `watch_applications` update) once a batch finishes, so a
/// client that already has a search response with blank `icon_path`s finds
/// out there's something new to ask for. See `Event::IconsUpdated`'s own
/// doc comment for why this push exists at all.
pub fn notify_icons_updated(state: &AppState) {
    broadcast(state, &Event::IconsUpdated);
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::{ClipboardContentKind, ResultKind};

    fn app(name: &str) -> AppEntry {
        AppEntry {
            id: name.to_string(),
            name: name.to_string(),
            path: std::path::PathBuf::from(format!("/Applications/{name}.app")),
        }
    }

    #[test]
    fn a_clipboard_match_is_never_crowded_out_of_the_response_by_many_app_matches() {
        let db = Db::open_in_memory().unwrap();
        neko_core::clipboard::record_entry(&db, "co-worker-notes", ClipboardContentKind::Text, None, 1000).unwrap();

        // 10 apps that all fuzzy-match "co" — comfortably more than the
        // server's own `limit`, the exact shape that used to leave 0 room
        // for clipboard in the response itself (not just on screen).
        let apps: Vec<AppEntry> = (0..10).map(|i| app(&format!("Console{i}"))).collect();
        let state = AppState::new(db, apps);

        let response = handle_request(
            &state,
            Request::Search { query: "co".into(), limit: 8 },
        );
        let Response::SearchResults { items } = response else {
            panic!("expected SearchResults")
        };

        assert!(
            items.iter().any(|i| i.kind == ResultKind::Clipboard),
            "a clipboard match must survive in the response even when apps alone would fill `limit`"
        );
        assert!(items.len() <= 8);
    }

    #[test]
    fn a_pure_app_query_still_returns_the_full_limit() {
        let db = Db::open_in_memory().unwrap();
        let apps: Vec<AppEntry> = (0..10).map(|i| app(&format!("Console{i}"))).collect();
        let state = AppState::new(db, apps);

        let response = handle_request(
            &state,
            Request::Search { query: "co".into(), limit: 8 },
        );
        let Response::SearchResults { items } = response else {
            panic!("expected SearchResults")
        };
        assert_eq!(items.len(), 8);
    }
}
