use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::Duration;
use std::sync::{Arc, Mutex, RwLock};

use neko_core::cancel::Cancel;
use neko_core::provider::Provider;
use neko_core::search::Candidate;
use neko_core::{AppEntry, Db};
use neko_protocol::{Event, Frame, Request, Response, read_frame, write_frame};

pub struct AppState {
    pub db: Arc<Mutex<Db>>,
    pub apps: Arc<RwLock<Vec<AppEntry>>>,
    /// Every registered result-type provider, in section render order —
    /// see `neko_core::search::allocate`'s doc comment for what that order
    /// means for ranking. Registering a new provider (eight are registered
    /// today: app, file, clipboard, settings, command, theme, preference,
    /// agent) is exactly one more line here plus its own `impl Provider` —
    /// nothing else in this file, the wire protocol, or the client needs to
    /// change. See `AGENTS.md`'s "Provider abstraction" and "Commands and
    /// modes" sections for the full accounting.
    providers: Vec<Box<dyn Provider>>,
    /// Providers reachable only by an explicitly scoped search or an
    /// activation — never included in a root-list query. See
    /// `AppState::new` for why the folder-scope list and the new-agent
    /// working-directory list are each one.
    mode_providers: Vec<Box<dyn Provider>>,
    /// One shared writer lock per connected client, keyed by nothing (just
    /// a flat list) since a connection never needs to look itself up — see
    /// `handle_connection`'s doc comment for why every write to a given
    /// connection, whether a request's own response or a broadcast `Event`,
    /// has to go through the *same* lock.
    broadcast: Mutex<Vec<Arc<Mutex<UnixStream>>>>,
}

impl AppState {
    pub fn new(db: Db, apps: Vec<AppEntry>) -> Self {
        {
            // The scope follows the persisted Search Folders setting, so it
            // needs the same `Db` every other provider shares — which only
            // exists inside `with_test_providers`, hence the closure-free
            // two-step here rather than a direct call.
            Self::with_test_providers(
                db,
                apps,
                None,
                neko_core::settings::SettingsProvider::new(),
                neko_core::permissions::PermissionsProvider::new(),
            )
        }
    }

    /// The real constructor, parameterized on the file and settings
    /// providers so tests can pass `FileProvider::empty()` /
    /// `SettingsProvider::with_panes(Vec::new())` — otherwise every
    /// `Request::Search` test with a 2+ character query would shell out to
    /// a real `mdfind` against whatever the test machine's own `$HOME`
    /// happens to contain (slow, not hermetic), and every such test would
    /// also pick up whatever System Settings panes happen to exist on the
    /// machine running the test suite, which is both non-hermetic and
    /// varies by OS version.
    /// `file_provider: None` means "the real one" — built from the shared
    /// `Db` below so its scope follows the persisted Search Folders setting.
    /// Tests pass `Some(FileProvider::empty())` to stay hermetic.
    fn with_test_providers(
        db: Db,
        apps: Vec<AppEntry>,
        file_provider: Option<neko_core::files::FileProvider>,
        settings_provider: neko_core::settings::SettingsProvider,
        permissions_provider: neko_core::permissions::PermissionsProvider,
    ) -> Self {
        let db = Arc::new(Mutex::new(db));
        let file_provider =
            file_provider.unwrap_or_else(|| neko_core::files::FileProvider::with_db(db.clone()));
        let apps = Arc::new(RwLock::new(apps));
        let providers: Vec<Box<dyn Provider>> = vec![
            Box::new(neko_core::apps::AppsProvider::new(apps.clone(), db.clone())),
            Box::new(file_provider),
            Box::new(neko_core::clipboard::ClipboardProvider::new(db.clone())),
            Box::new(settings_provider),
            // No test-provided variant needed, unlike file/settings above:
            // `CommandsProvider` does zero I/O (a fixed, compiled-in table
            // — see `commands.rs`'s own doc comment), so it's exactly as
            // hermetic and fast in a test as in the real daemon.
            Box::new(neko_core::commands::CommandsProvider::new()),
            // Sixth provider. Registered last on purpose: `search::allocate`
            // orders sections by content strength, and registration order is
            // only the tie-break, so a theme row never leads the root list
            // unless a query genuinely names one. Its own mode is where it
            // does the work.
            Box::new(neko_core::themes::ThemesProvider::new(db.clone())),
            // Seventh. Like the theme provider above it, its rows are worth
            // finding directly from the root list — "hotkey" should reach
            // the setting, not just a container to open it from.
            Box::new(neko_core::preferences::PreferencesProvider::new(db.clone())),
            // Eighth. Reads Paseo's own on-disk agent documents — no index
            // to warm, no watcher, no subprocess; see `agents.rs`.
            Box::new(neko_core::agents::AgentsProvider::new(db.clone())),
            // Ninth, and appended rather than inserted at the front even
            // though its rows always lead: `search::allocate` orders sections
            // by content strength and `permissions::ATTENTION_BONUS` settles
            // that outright, so registration order is only the tie-break —
            // and the positions in this list are load-bearing for the daemon
            // tests, which address a provider by index.
            Box::new(permissions_provider),
        ];
        // Mode-only providers: reachable when a search explicitly scopes to
        // them (`Request::Search`'s `provider` field) and by
        // `Request::Activate`, but never searched for an ordinary root-list
        // query. A configured search-folder path is not a result anybody
        // wants back from the root list, and unlike every provider above,
        // these rows only mean anything inside their own screen.
        //
        // `NewAgentProvider` is the second: its rows are working directories
        // to start an agent in, which are only an answer once somebody has
        // entered the `New Agent` mode and said what the task is. It takes no
        // `Db` and does no I/O at construction — it reads Paseo's own
        // projects file per search — so a test that never scopes to it never
        // touches the machine's real Paseo state.
        let mode_providers: Vec<Box<dyn Provider>> = vec![
            Box::new(neko_core::preferences::FolderScopeProvider::new(db.clone())),
            Box::new(neko_core::new_agent::NewAgentProvider::new()),
            // The third: a quota pane, read from the model provider's own
            // API (`neko_core::usage`). Mode-only because a rate-limit
            // window is not an answer to a root-list query — "5-hour limit"
            // surfacing for a search containing "limit" would be a surprise.
            Box::new(neko_core::usage::UsageProvider::new()),
            // The fourth: Paseo's schedules. Mode-only for the same reason
            // the usage pane is — a cron entry is not an answer to a
            // root-list query, and "0 4 * * *" matching a search for "4"
            // would be a surprise.
            Box::new(neko_core::schedules::SchedulesProvider::new()),
            // The fifth: the planner. Mode-only and emphatically so — a
            // provider that calls a model must never be reachable from an
            // ordinary root-list keystroke.
            Box::new(neko_core::ask::AskProvider::new()),
            // The sixth: Paseo's supervised terminals.
            Box::new(neko_core::terminals::TerminalsProvider::new()),
            // The seventh: the same agents the root list shows, but with
            // Enter meaning "send this prompt" rather than "open it in
            // Paseo". One provider cannot have two primary actions.
            Box::new(neko_core::agents::AgentControlProvider::new()),
            // The eighth: one agent's conversation, read in neko rather than
            // by switching to Paseo. Its scoped query is an agent id, not
            // typing — see `conversation.rs`.
            Box::new(neko_core::conversation::ConversationProvider::new()),
        ];
        Self {
            db,
            apps,
            providers,
            mode_providers,
            broadcast: Mutex::new(Vec::new()),
        }
    }
}

impl AppState {
    /// Every provider that an explicitly-scoped search or an activation can
    /// reach: the root-list ones first, then the mode-only ones. Root-list
    /// searches deliberately do **not** go through this — they iterate
    /// `providers` directly.
    fn all_providers(&self) -> impl Iterator<Item = &dyn Provider> {
        self.providers.iter().chain(self.mode_providers.iter()).map(|p| p.as_ref())
    }
}

pub fn now_unix_ms() -> i64 {
    neko_core::now_unix_ms()
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

/// Reads request frames off `stream` and spawns one thread per request to
/// compute and send its response. **Per-request, not per-connection,
/// concurrency is load-bearing, not an optimization**: `FileProvider`'s
/// `mdfind` round-trip can take several hundred milliseconds on a real,
/// repository-heavy home directory (see `neko_core::files`'s module doc
/// comment for the measured numbers), and the client holds exactly one
/// persistent connection to the daemon for every request it ever makes —
/// hotkey changes, onboarding state, every search-as-you-type keystroke,
/// all of it. A single-threaded read-handle-write loop (this function's
/// shape before this task) would queue every one of those behind whichever
/// `Search` a slow file query landed on, visibly degrading the whole app's
/// responsiveness rather than just that one search. Spawning a thread per
/// request means a slow `Search` blocks nothing else on the same
/// connection; the next keystroke's request is read and dispatched
/// immediately.
///
/// Responses can therefore complete out of order relative to requests —
/// safe by construction: `neko-client`'s `Shared::pending` map matches a
/// response back to its caller by the request's own `id`, never by arrival
/// order (see `neko-client/src/lib.rs`'s `read_until_disconnected`).
///
/// **The shared `Arc<Mutex<UnixStream>>` writer is the other half of this
/// change.** Two request threads on the same connection now genuinely can
/// write concurrently, and a `UnixStream::try_clone()` shares the
/// underlying socket fd — two unsynchronized `write_frame` calls (each two
/// separate `write_all`s: a length prefix, then the payload) could
/// interleave mid-frame and corrupt the stream. Every writer for a given
/// connection — a request's own response, and any `Event` broadcast to it
/// — goes through this one lock, so a full frame is always written
/// atomically relative to every other writer on the same connection.
pub fn handle_connection(state: Arc<AppState>, stream: UnixStream) {
    let writer = match stream.try_clone() {
        Ok(w) => Arc::new(Mutex::new(w)),
        Err(_) => return,
    };
    state.broadcast.lock().unwrap().push(writer.clone());

    // The one piece of genuinely per-connection state this daemon has: the
    // cancellation token of whichever `Search` is currently in flight for
    // *this* client. See `supersede_previous_search` for why it lives here
    // rather than on `AppState`.
    let in_flight_search: Arc<Mutex<Option<Cancel>>> = Arc::new(Mutex::new(None));

    loop {
        match read_frame(&stream) {
            Ok(Some(Frame::Request { id, request })) => {
                let cancel = match &request {
                    Request::Search { .. } => supersede_previous_search(&in_flight_search),
                    _ => Cancel::never(),
                };
                let state = state.clone();
                let writer = writer.clone();
                std::thread::spawn(move || {
                    // A search can answer in two frames (see
                    // `Response::SearchResults`'s own doc comment); every
                    // other request answers in exactly one. Both go out
                    // through this same per-connection writer lock, so a
                    // partial frame can never interleave with another
                    // thread's response.
                    let ctx = RequestContext::new(cancel, {
                        let writer = writer.clone();
                        move |response| {
                            let mut writer = writer.lock().unwrap();
                            let _ = write_frame(&mut *writer, &Frame::Response { id, response });
                        }
                    });
                    let response = handle_request(&state, request, &ctx);
                    ctx.send(response);
                });
            }
            Ok(Some(_)) => {} // Clients never send Response/Event frames.
            Ok(None) | Err(_) => return,
        }
    }
}

/// Cancels whatever `Search` this connection had in flight and installs a
/// fresh token for the one about to start, returning that token.
///
/// **Per connection, not global** — two clients (a real one and, say, an
/// evidence harness) must not cancel each other's searches; a single client
/// superseding its own previous keystroke is exactly the intended
/// behaviour and needs no new wire message to express it. The client
/// already tells the daemon it has moved on simply by sending the next
/// `Search` on the same socket: there is no such thing as a client that
/// wants two of its own searches answered at once, in the root list or in
/// a mode. See `neko_core::cancel` for what the cancelled side actually
/// does with the signal.
fn supersede_previous_search(in_flight: &Mutex<Option<Cancel>>) -> Cancel {
    let fresh = Cancel::new();
    let mut slot = in_flight.lock().unwrap();
    if let Some(previous) = slot.replace(fresh.clone()) {
        previous.cancel();
    }
    fresh
}

/// What one in-flight request can do beyond returning its final response:
/// observe cancellation, and emit an *earlier*, partial response for the
/// same request id.
pub struct RequestContext {
    cancel: Cancel,
    send: Box<dyn Fn(Response) + Send + Sync>,
}

impl RequestContext {
    fn new(cancel: Cancel, send: impl Fn(Response) + Send + Sync + 'static) -> Self {
        Self { cancel, send: Box::new(send) }
    }

    /// A context that discards partial responses and is never cancelled —
    /// for call sites with no client behind them (this module's own tests,
    /// which assert on `handle_request`'s final return value).
    #[cfg(test)]
    fn inert() -> Self {
        Self::new(Cancel::never(), |_| {})
    }

    fn send(&self, response: Response) {
        (self.send)(response)
    }
}

/// Runs `providers` concurrently, one thread each, and collects their
/// candidates. `std::thread::scope` guarantees every spawned thread joins
/// before this returns, so `query`/`now`/`cancel` are borrowed rather than
/// cloned per provider.
fn search_concurrently<'a>(
    providers: &[&'a dyn Provider],
    query: &str,
    now: i64,
    cancel: &Cancel,
) -> Vec<(&'a str, Vec<Candidate>)> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = providers
            .iter()
            .map(|&provider| scope.spawn(move || (provider.id(), provider.search_cancellable(query, now, cancel))))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

fn handle_request(state: &AppState, request: Request, ctx: &RequestContext) -> Response {
    match request {
        Request::Ping => Response::Pong,

        Request::Search { query, limit, provider: Some(provider_id) } => {
            // The mode seam: scoped to exactly one provider, no cross-
            // provider `allocate()` — see `Request::Search`'s own doc
            // comment. A mode's own list wants "this provider's best
            // matches, ranked," not a shared, budget-reserving merge with
            // every other result type.
            let limit = limit.clamp(1, 50);
            let now = now_unix_ms();
            let Some(provider) = state.all_providers().find(|p| p.id() == provider_id) else {
                return Response::Error { message: format!("no such provider: {provider_id}") };
            };
            let mut candidates = provider.search_cancellable(&query, now, &ctx.cancel);
            candidates.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.item.title.cmp(&b.item.title)));
            candidates.truncate(limit);
            Response::SearchResults { items: candidates.into_iter().map(|c| c.item).collect(), complete: true }
        }

        Request::Search { query, limit, provider: None } => {
            let limit = limit.clamp(1, 50);
            let now = now_unix_ms();
            let query = query.as_str();

            // **Two-phase, so a keystroke renders at the speed of the
            // fastest provider rather than the slowest.** Splitting the
            // registered providers by `defers_for` (see that method's doc
            // comment, and `AGENTS.md`'s "Two-phase search" section):
            // everything that answers from memory or SQLite runs first and
            // its allocation goes out immediately as a partial frame; the
            // slow one (file search's `mdfind`, up to
            // `files::QUERY_TIMEOUT`) then runs and the *full* allocation
            // — every provider's candidates, the same `allocate` call this
            // request always made — goes out as the final frame.
            //
            // The fast phase's candidates are computed once and reused by
            // the final `allocate`, so this costs one extra small frame per
            // keystroke, never a second round of provider work.
            // A root-list search with nothing typed yet must not reach a
            // provider whose whole list only means something once asked for
            // (themes, preferences) — see `Provider::answers_empty_root_query`.
            // Scoped searches return above and never come through here, so a
            // surface that *does* want that whole list is unaffected.
            let empty_query = query.trim().is_empty();
            let (deferred, immediate): (Vec<&dyn Provider>, Vec<&dyn Provider>) = state
                .providers
                .iter()
                .map(|provider| provider.as_ref())
                .filter(|provider| !empty_query || provider.answers_empty_root_query())
                .partition(|provider| provider.defers_for(query));

            let mut candidates = search_concurrently(&immediate, query, now, &ctx.cancel);

            if deferred.is_empty() {
                // Nothing slow to wait for: one frame, already final. The
                // ordinary shape for a query shorter than
                // `files::MIN_QUERY_LEN`, and for any build with no
                // deferring provider registered.
                let items = neko_core::search::allocate(candidates, limit, query);
                return Response::SearchResults { items, complete: true };
            }

            ctx.send(Response::SearchResults {
                items: neko_core::search::allocate(candidates.clone(), limit, query),
                complete: false,
            });

            candidates.extend(search_concurrently(&deferred, query, now, &ctx.cancel));
            let items = neko_core::search::allocate(candidates, limit, query);
            Response::SearchResults { items, complete: true }
        }

        Request::Activate { kind, id, action, query } => match state.all_providers().find(|p| p.id() == kind) {
            Some(provider) => {
                let result = match action {
                    // `activate_with_query`, never `activate` — it defaults to
                    // dropping the query and delegating, so this is the same
                    // call for every provider that does not care, and the only
                    // way `new_agent` ever learns what was typed. Same
                    // arrangement as `search_cancellable` above.
                    None => provider.activate_with_query(&id, &query),
                    Some(action_id) => provider.perform_action(&id, &action_id),
                };
                match result {
                    Ok(()) => {
                        // The committing client already applied this palette
                        // itself (live preview happens before the round-trip),
                        // so this exists for *other* connected clients — and,
                        // in this process, for the separate onboarding window
                        // if one is open. Harmlessly idempotent for the sender.
                        if kind == "theme" {
                            broadcast(state, &Event::ThemeChanged { id: id.clone() });
                        }
                        Response::Activated
                    }
                    Err(e) => Response::Error {
                        message: e.to_string(),
                    },
                }
            }
            None => Response::Error {
                message: format!("no such provider: {kind}"),
            },
        },

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

        Request::GetTheme => {
            let db = state.db.lock().unwrap();
            match neko_core::themes::get_theme(&db) {
                Ok(id) => Response::Theme { id },
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
    writers.retain(|w| {
        let mut w = w.lock().unwrap();
        write_frame(&mut *w, &Frame::Event(event.clone())).is_ok()
    });
}

/// Called by `main.rs`'s background icon-extraction passes (startup and
/// each live `watch_applications` update) once a batch finishes, so a
/// client that already has a search response with blank `icon_path`s finds
/// out there's something new to ask for. See `Event::IconsUpdated`'s own
/// doc comment for why this push exists at all.
pub fn notify_icons_updated(state: &AppState) {
    broadcast(state, &Event::IconsUpdated);
}

/// How often the daemon asks Paseo who is blocked.
///
/// **Deliberately shorter than `permissions::CACHE_TTL`, and that is the
/// whole point.** At 5s against a 1.5s TTL the cache was warm for 1.5
/// seconds out of every 5 — so `defers_for` found it cold most of the time
/// and the inbox landed in the panel's *second* frame, which is exactly what
/// L3 exists to prevent. Caught by timing a root query on a freshly started
/// daemon and getting two frames back.
///
/// The two numbers answer different questions and both are needed: the TTL
/// bounds how stale an answer may be before it is refetched on demand, and
/// this bounds how long the cache may sit cold. Polling faster than the TTL
/// is what keeps it continuously warm.
///
/// Two `curl` processes per tick, forever, so the cost is a real trade
/// rather than a shrug — measured below 1% of a core at this cadence.
// `verify_harness` hosts this same module and deliberately does not poll —
// evidence runs must not open a socket to the captain's real Paseo daemon,
// the same rule that keeps them off the real `neko-daemon` binary.
#[allow(dead_code)]
const ATTENTION_POLL_INTERVAL: Duration = Duration::from_millis(1200);

/// Keeps the permission inbox warm, and tells every client when the number
/// of waiting agents changes.
///
/// **Two jobs, and the second is the one that could not be done any other
/// way.** Warming the cache is what lets `PermissionsProvider::defers_for`
/// answer in the panel's first frame instead of its second. But the panel is
/// hidden almost all of the time, and a search cannot tell you about an
/// agent that blocked while you were not looking — that is what the
/// broadcast is for.
///
/// Broadcasts on **change only**. A client that repainted its Dock tile
/// every five seconds forever would be a wakeup per tick for a number that
/// is almost always the same one.
#[allow(dead_code)]
pub fn run_attention_poll(state: Arc<AppState>) {
    let mut last: Option<usize> = None;
    loop {
        // Rediscovered every tick rather than held: Paseo is restarted often,
        // and a poller that resolved the daemon once at startup would stay
        // silently dead for the rest of neko's process lifetime.
        if let Ok(client) = neko_core::mcp::McpClient::discover() {
            let count = neko_core::permissions::refresh(&client);
            if last != Some(count) {
                last = Some(count);
                broadcast(&state, &Event::AttentionChanged { count });
            }
        } else if last.is_some_and(|n| n > 0) {
            // Paseo went away while agents were waiting. Nothing is blocked
            // *that neko can see*, and leaving a stale count on the Dock
            // would be a standing claim that is no longer true.
            last = Some(0);
            broadcast(&state, &Event::AttentionChanged { count: 0 });
        }
        std::thread::sleep(ATTENTION_POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_core::clipboard::ClipboardContentKind;
    use neko_protocol::SearchItem;

    /// `handle_request` with no client behind it — partial responses are
    /// discarded and nothing is ever cancelled, so a test that only cares
    /// about the final answer reads exactly as it did before searches
    /// could answer in two frames. Tests that *do* care about the partial
    /// frame use [`handle_request_capturing`] instead.
    fn handle_request_for_test(state: &AppState, request: Request) -> Response {
        handle_request(state, request, &RequestContext::inert())
    }

    /// Drives `handle_request` the way a real connection does, collecting
    /// every frame it emits in order — the partial `complete: false`
    /// response first (when there is one), then the final one.
    fn handle_request_capturing(state: &AppState, request: Request) -> Vec<Response> {
        let collected: Arc<Mutex<Vec<Response>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = collected.clone();
        let ctx = RequestContext::new(Cancel::never(), move |response| sink.lock().unwrap().push(response));
        let final_response = handle_request(state, request, &ctx);
        let mut frames = collected.lock().unwrap().clone();
        frames.push(final_response);
        frames
    }

    fn app(name: &str) -> AppEntry {
        AppEntry {
            id: name.to_string(),
            name: name.to_string(),
            path: std::path::PathBuf::from(format!("/Applications/{name}.app")),
        }
    }

    /// `AppState::new` with an empty-scope file provider and an
    /// empty-pane-list settings provider — see
    /// `AppState::with_test_providers`'s doc comment for why every test in
    /// this module goes through this rather than the real constructor.
    fn test_state(db: Db, apps: Vec<AppEntry>) -> AppState {
        AppState::with_test_providers(
            db,
            apps,
            Some(neko_core::files::FileProvider::empty()),
            neko_core::settings::SettingsProvider::with_panes(Vec::new()),
            // Never reaches the daemon: whether Paseo happens to be running
            // must not decide whether this suite passes.
            neko_core::permissions::PermissionsProvider::disabled(),
        )
    }

    #[test]
    fn an_empty_root_query_never_returns_settings_or_theme_rows_but_a_scoped_one_still_does() {
        // Caught live, not by a unit test: "Launch at Login" was rendering in
        // the root list before anything had been typed, and seventeen themes
        // were taking the top slots ahead of it.
        let db = Db::open_in_memory().unwrap();
        let state = test_state(db, vec![]);
        let ctx = RequestContext::new(Cancel::never(), |_response| {});

        let root = handle_request(
            &state,
            Request::Search { query: String::new(), limit: 20, provider: None },
            &ctx,
        );
        let Response::SearchResults { items, .. } = root else { panic!("expected results") };
        for item in &items {
            assert!(
                item.kind != "preference" && item.kind != "theme" && item.kind != "clipboard",
                "{} must not answer an empty root query, got {:?}",
                item.kind,
                item.title
            );
        }

        // The same empty query, scoped, is how the Preferences window loads
        // its values — it must still return every setting.
        let scoped = handle_request(
            &state,
            Request::Search { query: String::new(), limit: 20, provider: Some("preference".into()) },
            &ctx,
        );
        let Response::SearchResults { items, .. } = scoped else { panic!("expected results") };
        // Asserted by identity, not by count: settings get added over time,
        // and a count here would fail for the wrong reason every time one is.
        let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        for expected in ["hotkey", "launch-at-login", "search-folders"] {
            assert!(
                ids.contains(&expected),
                "the window's own load must be unaffected by the root-list guard; missing {expected} from {ids:?}"
            );
        }
    }

    #[test]
    fn a_clipboard_match_is_never_crowded_out_of_the_response_by_many_app_matches() {
        let db = Db::open_in_memory().unwrap();
        neko_core::clipboard::record_entry(&db, "co-worker-notes", ClipboardContentKind::Text, None, 1000).unwrap();

        // 10 apps that all fuzzy-match "cons" — comfortably more than the
        // server's own `limit`, the exact shape that used to leave 0 room
        // for clipboard in the response itself (not just on screen). "cons"
        // rather than "co": the always-registered `CommandsProvider` (a
        // fixed table, unlike the empty-scope file/settings providers this
        // test fixture uses) would otherwise also match "co" against its
        // "Clipboard"/"Clipboard History"/"Clipboard Manager" aliases,
        // muddying what this test is isolating — "cons" matches none of
        // those (no 'n' in "Clipboard"/"History", no 's' in "Manager").
        let apps: Vec<AppEntry> = (0..10).map(|i| app(&format!("Console{i}"))).collect();
        let state = test_state(db, apps);

        let response = handle_request_for_test(
            &state,
            Request::Search { query: "cons".into(), limit: 8, provider: None },
        );
        let Response::SearchResults { items, .. } = response else {
            panic!("expected SearchResults")
        };

        assert!(
            items.iter().any(|i| i.kind == "clipboard"),
            "a clipboard match must survive in the response even when apps alone would fill `limit`"
        );
        assert!(items.len() <= 8);
    }

    /// A provider that stands in for `FileProvider` without touching
    /// `mdfind`: it defers (so the daemon splits the request in two), it
    /// blocks until released (so the partial frame is observably *earlier*
    /// than the final one), and it returns one candidate the fast providers
    /// could never produce.
    struct BlockingDeferredProvider {
        release: Arc<std::sync::Barrier>,
    }

    impl Provider for BlockingDeferredProvider {
        fn id(&self) -> &'static str {
            "file"
        }
        fn section_label(&self) -> &'static str {
            "Files"
        }
        fn defers_for(&self, query: &str) -> bool {
            !query.is_empty()
        }
        fn search(&self, _query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
            self.release.wait();
            vec![Candidate {
                score: 100.0,
                item: SearchItem {
                    id: "/tmp/console-notes.txt".into(),
                    kind: "file".into(),
                    title: "console-notes.txt".into(),
                    subtitle: None,
                    icon: neko_protocol::Icon::Glyph(neko_protocol::Glyph::File),
                    section_label: "Files".into(),
                    action_label: "Open  ↵".into(),
                    badge: None,
                    accessory: None,
                    enters_mode: None,
                    group_label: None,
                    actions: Vec::new(),
                    source: None,
                    meter: None,
                    keeps_open: false,
                    preview_markdown: false,
                    speaker: None,
                    preview: None,
                },
            }]
        }
        fn activate(&self, _id: &str) -> Result<(), neko_core::ProviderError> {
            Ok(())
        }
    }

    fn state_with_deferred_provider(
        db: Db,
        apps: Vec<AppEntry>,
        release: Arc<std::sync::Barrier>,
    ) -> AppState {
        let mut state = test_state(db, apps);
        state.providers[1] = Box::new(BlockingDeferredProvider { release });
        state
    }

    #[test]
    fn a_slow_provider_does_not_hold_up_the_fast_providers_own_results() {
        // The whole point of this task: the partial frame must be
        // observable while the deferred provider is still blocked, carrying
        // real results from every provider that was ready.
        let db = Db::open_in_memory().unwrap();
        let apps: Vec<AppEntry> = (0..3).map(|i| app(&format!("Console{i}"))).collect();
        let release = Arc::new(std::sync::Barrier::new(2));
        let state = Arc::new(state_with_deferred_provider(db, apps, release.clone()));

        let partials: Arc<Mutex<Vec<Response>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = partials.clone();
        let ctx = RequestContext::new(Cancel::never(), move |response| sink.lock().unwrap().push(response));

        let request_state = state.clone();
        let handle = std::thread::spawn(move || {
            handle_request(
                &request_state,
                Request::Search { query: "cons".into(), limit: 8, provider: None },
                &ctx,
            )
        });

        // While the deferred provider is still blocked on the barrier, poll
        // for the partial frame. It must arrive without the deferred
        // provider having returned anything at all.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if !partials.lock().unwrap().is_empty() {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "no partial frame arrived while the slow provider was blocked");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let partial = partials.lock().unwrap()[0].clone();
        let Response::SearchResults { items, complete } = partial else {
            panic!("expected SearchResults")
        };
        assert!(!complete, "the first frame must announce itself as partial");
        assert!(items.iter().any(|i| i.kind == "app"), "fast providers' real results are in the partial frame");
        assert!(!items.iter().any(|i| i.kind == "file"), "the deferred provider has not answered yet");

        // Now let the deferred provider finish; the final frame carries the
        // full, re-allocated set including its results.
        release.wait();
        let final_response = handle.join().unwrap();
        let Response::SearchResults { items, complete } = final_response else {
            panic!("expected SearchResults")
        };
        assert!(complete, "the second frame is the last one for this request id");
        assert!(items.iter().any(|i| i.kind == "file"), "the deferred provider's results land in the final frame");
        assert!(items.iter().any(|i| i.kind == "app"), "and the fast providers' results are still there");
    }

    #[test]
    fn a_query_no_provider_defers_for_is_answered_in_exactly_one_complete_frame() {
        // The short-query case (and any build with no slow provider): an
        // extra wire frame and an extra client render would be pure cost.
        let db = Db::open_in_memory().unwrap();
        let state = test_state(db, vec![app("Console")]);
        let frames = handle_request_capturing(
            &state,
            Request::Search { query: "cons".into(), limit: 8, provider: None },
        );
        assert_eq!(frames.len(), 1, "expected exactly one frame, got {}", frames.len());
        assert!(matches!(frames[0], Response::SearchResults { complete: true, .. }));
    }

    #[test]
    fn the_partial_and_final_frames_both_honour_allocates_own_reservation_rules() {
        // `allocate` runs over the fast providers alone for the partial
        // frame and over everything for the final one — the section
        // reservation that stops one provider crowding out another has to
        // hold in both, not just the merged case.
        let db = Db::open_in_memory().unwrap();
        neko_core::clipboard::record_entry(&db, "co-worker-notes", ClipboardContentKind::Text, None, 1000).unwrap();
        let apps: Vec<AppEntry> = (0..10).map(|i| app(&format!("Console{i}"))).collect();
        let release = Arc::new(std::sync::Barrier::new(1));
        let state = state_with_deferred_provider(db, apps, release);

        let frames = handle_request_capturing(
            &state,
            Request::Search { query: "cons".into(), limit: 8, provider: None },
        );
        assert_eq!(frames.len(), 2);
        for (i, frame) in frames.iter().enumerate() {
            let Response::SearchResults { items, .. } = frame else { panic!("expected SearchResults") };
            assert!(
                items.iter().any(|item| item.kind == "clipboard"),
                "frame {i} dropped the clipboard reservation: {:?}",
                items.iter().map(|item| item.kind.as_str()).collect::<Vec<_>>()
            );
            assert!(items.len() <= 8, "frame {i} exceeded the request's own limit");
        }
        let Response::SearchResults { items, .. } = &frames[1] else { panic!() };
        assert!(items.iter().any(|item| item.kind == "file"), "the final frame reserves the deferred section a slot too");
    }

    #[test]
    fn a_pure_app_query_still_returns_the_full_limit() {
        let db = Db::open_in_memory().unwrap();
        let apps: Vec<AppEntry> = (0..10).map(|i| app(&format!("Console{i}"))).collect();
        let state = test_state(db, apps);

        let response = handle_request_for_test(
            &state,
            Request::Search { query: "cons".into(), limit: 8, provider: None },
        );
        let Response::SearchResults { items, .. } = response else {
            panic!("expected SearchResults")
        };
        assert_eq!(items.len(), 8);
    }

    #[test]
    fn a_provider_scoped_search_returns_only_that_providers_own_matches() {
        // The mode seam: `provider: Some(id)` bypasses `allocate()`
        // entirely — a query that would otherwise also match apps (10 of
        // them, all containing "co") must come back as *only* clipboard
        // matches when scoped to "clipboard".
        let db = Db::open_in_memory().unwrap();
        neko_core::clipboard::record_entry(&db, "co-worker-notes", ClipboardContentKind::Text, None, 1000).unwrap();
        neko_core::clipboard::record_entry(&db, "unrelated", ClipboardContentKind::Text, None, 2000).unwrap();
        let apps: Vec<AppEntry> = (0..10).map(|i| app(&format!("Console{i}"))).collect();
        let state = test_state(db, apps);

        let response = handle_request_for_test(
            &state,
            Request::Search { query: "co".into(), limit: 50, provider: Some("clipboard".to_string()) },
        );
        let Response::SearchResults { items, .. } = response else {
            panic!("expected SearchResults")
        };
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, "clipboard");
        assert_eq!(items[0].id, "co-worker-notes");
    }

    #[test]
    fn a_provider_scoped_search_with_an_unknown_provider_id_errors() {
        let db = Db::open_in_memory().unwrap();
        let state = test_state(db, Vec::new());
        let response = handle_request_for_test(
            &state,
            Request::Search { query: "x".into(), limit: 8, provider: Some("nonexistent".to_string()) },
        );
        let Response::Error { message } = response else {
            panic!("expected an Error response")
        };
        assert!(message.contains("no such provider"), "unexpected message: {message}");
    }

    #[test]
    fn a_provider_scoped_search_with_an_empty_query_returns_the_providers_full_list() {
        // The mode's own "just entered, show everything" case: an empty
        // filter query still scores every clipboard entry (`fuzzy_score`
        // returns `Some(0.0)` for an empty query) and returns them all,
        // most-recent-first via the recency boost — not the merged root
        // list's "empty query = every provider returns nothing" behavior.
        let db = Db::open_in_memory().unwrap();
        neko_core::clipboard::record_entry(&db, "older", ClipboardContentKind::Text, None, 100).unwrap();
        neko_core::clipboard::record_entry(&db, "newer", ClipboardContentKind::Text, None, 900).unwrap();
        let state = test_state(db, Vec::new());
        let response = handle_request_for_test(
            &state,
            Request::Search { query: "".into(), limit: 50, provider: Some("clipboard".to_string()) },
        );
        let Response::SearchResults { items, .. } = response else {
            panic!("expected SearchResults")
        };
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "newer", "more recently copied entries sort first");
    }

    #[test]
    fn activate_routes_to_the_provider_named_by_kind() {
        let db = Db::open_in_memory().unwrap();
        let state = test_state(db, vec![app("Console")]);

        // "app" provider, id that doesn't exist — proves routing landed on
        // the right provider (a real app-not-found error), not a generic
        // "no such provider" failure.
        let response =
            handle_request_for_test(&state, Request::Activate { kind: "app".into(), id: "does-not-exist".into(), action: None, query: String::new() });
        let Response::Error { message } = response else {
            panic!("expected an Error response")
        };
        assert!(message.contains("no such app"), "unexpected message: {message}");
    }

    #[test]
    fn activate_with_an_unknown_provider_kind_errors() {
        let db = Db::open_in_memory().unwrap();
        let state = test_state(db, Vec::new());
        let response =
            handle_request_for_test(&state, Request::Activate { kind: "nonexistent".into(), id: "x".into(), action: None, query: String::new() });
        let Response::Error { message } = response else {
            panic!("expected an Error response")
        };
        assert!(message.contains("no such provider"), "unexpected message: {message}");
    }

    #[test]
    fn activate_with_a_named_action_routes_to_perform_action() {
        let db = Db::open_in_memory().unwrap();
        neko_core::clipboard::record_entry(&db, "delete me", ClipboardContentKind::Text, None, 100).unwrap();
        let state = test_state(db, Vec::new());
        let response = handle_request_for_test(
            &state,
            Request::Activate { kind: "clipboard".into(), id: "delete me".into(), action: Some("delete".into()), query: String::new() },
        );
        assert!(matches!(response, Response::Activated), "expected Activated, got {response:?}");
    }

    #[test]
    fn activate_with_an_unknown_action_on_a_known_provider_errors() {
        let db = Db::open_in_memory().unwrap();
        let state = test_state(db, vec![app("Console")]);
        let response = handle_request_for_test(
            &state,
            Request::Activate { kind: "app".into(), id: "Console".into(), action: Some("teleport".into()), query: String::new() },
        );
        let Response::Error { message } = response else {
            panic!("expected an Error response")
        };
        assert!(message.contains("no action"), "unexpected message: {message}");
    }
}
