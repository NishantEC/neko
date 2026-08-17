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

            let mut items = neko_core::search::rank_apps(&query, &apps, &recency, now, limit);
            // Clipboard results fill whatever's left of `limit` after apps —
            // apps stay the primary result type (unchanged from the
            // app-only slice), clipboard is additive within the same
            // server-capped list per `AGENTS.md`'s "just another result
            // type in the same fast list."
            let remaining = limit.saturating_sub(items.len());
            if remaining > 0 {
                items.extend(neko_core::search::rank_clipboard(
                    &query,
                    &clipboard_entries,
                    now,
                    remaining,
                ));
            }
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
