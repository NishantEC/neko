//! Neko's native client on Rune, built beside the web UI (docs/decisions/0002-rune-for-everything.md).
//! P0 is the foundation: the pinned Rune, the read-only daemon link, the theme and the status
//! glyphs, and the four-region shell (sessions, the session, a context panel, Neko), with the
//! sessions and counts live from the daemon and the rest placeholders. No tabs.

pub mod account;
pub mod attention;
pub mod all_sessions;
pub mod activity;
pub mod app;
pub mod art;
pub mod artifact_actions;
pub mod art_gpu;
pub mod attachments;
pub mod board;
pub mod browser;
pub mod clock;
pub mod compose;
pub mod connection;
pub mod deeplink;
pub mod dock;
pub mod extra;
pub mod groups;
pub mod icons;
pub mod icons_phosphor;
pub mod lab;
pub mod launcher;
pub mod luau;
pub mod motion;
pub mod layout;
pub mod link;
pub mod new_session;
pub mod pages;
pub mod onboarding;
pub mod overlays;
pub mod palette;
pub mod permissions;
pub mod plan;
pub mod presence;
pub mod pull_requests;
pub mod panel;
pub mod plugin_manage;
pub mod samples;
pub mod scope;
pub mod scenes;
pub mod readers;
pub mod report;
pub mod runtime_pick;
pub mod scenes_batch_a;
pub mod scenes_batch_b;
pub mod scenes_batch_d;
pub mod scenes_luau;
pub mod scenes_pages;
pub mod scenes_setup;
pub mod scenes_panel;
pub mod status;
pub mod task_actions;
pub mod shortcuts;
pub mod terminal;
pub mod themes;
pub mod toasts;
pub mod theme;
pub mod turns;
pub mod view;
pub mod viewers;
pub mod views;
pub mod web_link;
pub mod web_panel;
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

thread_local! {
    /// The route `--open` named when no window was running to take it: the first window opens it once the daemon has answered.
    static FIRST_ROUTE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
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
            // Real time, so a task's age and the groups' 48 hour and 7 day edges are read as the person reads them.
            if app.summary.is_some() {
                app.clock = app::system_ms;
                app.now_ms = app::system_ms();
                // A window opens on Home: a session is chosen only when the snapshot asks (`NEKO_RUNE_OPEN_TASK`).
                app.call(link::Call::Refresh);
                app.call(link::Call::RoutinesFull);
                app.call(link::Call::Models);
                app.call(link::Call::Appearance);
                app.ask_calendar();
            }
            // For the panel's snapshots against a test daemon: open the task with this title, and show
            // this artifact as the daemon's `ShowArtifact` would (`scripts/snap-harness.sh`).
            if let Some(title) = std::env::var("NEKO_RUNE_OPEN_TASK").ok().filter(|t| !t.is_empty()) {
                let id = app.summary.as_ref().and_then(|m| m.tasks.iter().find(|t| t.title == title)).map(|t| t.id.clone());
                if let Some(id) = id {
                    app.select(&id);
                }
            }
            // The panel's tab for the pictures (`NEKO_RUNE_TAB=plan|info|terminal|diff|files|canvas`): opens the panel on it.
            if let Some(tab) = std::env::var("NEKO_RUNE_TAB").ok().and_then(|v| panel::Tab::ALL.into_iter().find(|t| t.title().eq_ignore_ascii_case(&v))) {
                app.panel_choose_tab(tab);
                app.panel_set_open(true);
            }
            // For the side-by-side pairs against the web UI (`/tmp/neko-polish`): the person's own name, and a full view.
            if std::env::var_os("NEKO_RUNE_ACCOUNT").is_some() {
                app.account = account::Account::load(|| {});
                // The pictures of the person's own Mac report what this Mac says (Settings, Permissions).
                app.access_probe = permissions::check;
            }
            if let Some(view) = std::env::var("NEKO_RUNE_VIEW").ok().and_then(|v| views::FullView::ALL.into_iter().find(|f| f.web_id() == v)) {
                app.open_view(view);
            }
            // Parity batch A's screens on the person's own data, for the pictures (`NEKO_RUNE_PREVIEW=palette|all|month|closed|model|shortcuts|slash|scope|cadence|board|tree|prs`).
            if let Some(what) = std::env::var("NEKO_RUNE_PREVIEW").ok().filter(|v| !v.is_empty()) {
                match what.as_str() {
                    "palette" => app.palette_toggle(),
                    "all" => app.open_all_sessions(),
                    "month" => {
                        app.open_view(views::FullView::Calendar);
                        app.cal_mode(pages::CalMode::Month);
                    }
                    "closed" => app.toggle("closed:open"),
                    "model" => app.runtime_toggle(),
                    "shortcuts" => app.shortcuts_open = true,
                    "slash" => {
                        app.ask.replace_text("/");
                        app.ask.focused = true;
                    }
                    "scope" => app.scope_open = true,
                    // Parity batch B: All sessions as a board or a tree, and Pull requests.
                    "board" | "tree" => {
                        app.open_all_sessions();
                        app.all.include_stopped = true;
                        app.set_all_layout(if what == "board" { board::Layout::Board } else { board::Layout::Tree });
                        app.all.layout_dirty = false;
                    }
                    "prs" => app.open_view(views::FullView::PullRequests),
                    // Parity batch D: the first-run setup at a step (`NEKO_RUNE_STEP=agents`).
                    "onboarding" => {
                        let step = std::env::var("NEKO_RUNE_STEP").ok().and_then(|k| onboarding::Step::from_key(&k)).unwrap_or(onboarding::Step::Welcome);
                        app.open_onboarding(step);
                    }
                    "cadence" => {
                        app.open_view(views::FullView::Routines);
                        let first = app.pages.routines.as_ref().and_then(|r| r.views.first().map(|v| v.routine.id.clone()));
                        if let Some(id) = first {
                            app.open_routine(&id);
                        }
                    }
                    _ => {}
                }
            }
            // The session header's menu open, for the pictures.
            if std::env::var_os("NEKO_RUNE_MENU").is_some() {
                app.session_menu = true;
            }
            // The Settings section for the pictures (`NEKO_RUNE_SECTION=permissions`).
            if let Some(section) = std::env::var("NEKO_RUNE_SECTION").ok().and_then(|v| pages::Section::ALL.into_iter().find(|x| x.key() == v)) {
                app.pages.section = section;
            }
            // The Setup section for the pictures (`NEKO_RUNE_VIEW=setup NEKO_RUNE_SETUP=watching`).
            if let Some(section) = std::env::var("NEKO_RUNE_SETUP").ok().and_then(|v| view::setup::SetupSection::from_key(&v)) {
                app.open_setup(section);
            }
            if let Some(id) = std::env::var("NEKO_RUNE_SHOW").ok().filter(|t| !t.is_empty()) {
                app.show_artifact(&id, None);
            }
        }
        app
    } else {
        // The last run's widths (read once, before the first frame); the window then keeps them.
        let saved = app::load_layout();
        let layout = saved.map(layout::Layout::restored).unwrap_or_default();
        App { pending_route: FIRST_ROUTE.with(|r| r.borrow_mut().take()), linked: !has("nolink"), dock: dock::set_badge, workspace_scope: app::load_scope(), all: all_sessions::AllState { layout: app::load_all_layout().unwrap_or_default(), ..Default::default() }, layout, layout_written: saved, layout_seen: saved, persist_layout: true, overlays: overlays::Overlays::new(app::load_presence()), web_view: web_panel::WebHost::windowed(), luau: luau::Luau { persist: true, hidden: App::luau_load_hidden(), ..Default::default() }, ..App::default() }
    }
}

/// The daemon's socket: `socket=PATH`, else `NEKO_SOCKET`, else the one the daemon uses
/// (`$NEKO_DATA_DIR/neko.sock`, or the Application Support folder).
fn socket_path() -> PathBuf {
    flag("socket").or_else(|| std::env::var("NEKO_SOCKET").ok()).map(PathBuf::from).unwrap_or_else(neko_protocol::socket_path)
}

/// Keys the window handles before any field: ⌘K opens the palette; ⌘N starts a session.
pub fn on_key(s: &mut App, ev: &rune::KeyEvent, tx: &mut rune::Tx) -> bool {
    // The context panel's keys: Esc restores an expanded panel; ⌘+, ⌘−, ⌘0 zoom a PDF; W turns a model's wireframe on.
    if s.panel_on_key(ev) {
        return true;
    }
    // ⌘F in Settings goes to its search field, which filters the rows of every section.
    if ev.is_command('f') && !ev.mods.shift && s.view == Some(views::FullView::Settings) {
        tx.focus("settings-search", true);
        return true;
    }
    // ⌘S in the rulebook saves it (the web's textarea does the same).
    if ev.is_command('s') && !ev.mods.shift && s.view == Some(views::FullView::Settings) && s.pages.section == pages::Section::Rulebook {
        s.save_rulebook();
        return true;
    }
    // The summon shortcut's recorder takes the next key press.
    if s.hotkey_key(ev) {
        return true;
    }
    // All sessions: `/` focuses its search, J and K move, Return opens (never while a field has the keyboard).
    if s.all_key(ev, tx) {
        return true;
    }
    // ⌘⇧Esc: stop all work (asks first).
    if ev.mods.cmd && ev.mods.shift && ev.key == rune::Key::Escape {
        s.ask_stop_all();
        return true;
    }
    if ev.is_command('k') && !ev.mods.shift {
        s.palette_toggle();
        return true;
    }
    // ⌘B: the sessions column away and back.
    if ev.is_command('b') && !ev.mods.shift {
        s.toggle_sidebar();
        return true;
    }
    // ⌘↵ with no field holding the keyboard approves what waits for a yes; Esc with the ask box empty declines it.
    if ev.mods.cmd && !ev.mods.shift && ev.key == rune::Key::Enter && s.palette.is_none() && s.dialog.is_none() && s.work.confirm.is_none() {
        s.approve_first_decision();
        return true;
    }
    if ev.key == rune::Key::Escape && !(ev.mods.cmd || ev.mods.alt || ev.mods.ctrl || ev.mods.shift) {
        if s.shortcuts_open {
            s.shortcuts_open = false;
            return true;
        }
        if s.palette.is_none() && s.dialog.is_none() && s.work.confirm.is_none() && !s.composer.focused && !s.ask.focused && s.ask.text.trim().is_empty() && s.decline_first_decision() {
            return true;
        }
    }
    // `?` shows the shortcuts (a `?` typed in a field is the field's, which sees the key first).
    if matches!(&ev.key, rune::Key::Text(t) if t == "?") && !(ev.mods.cmd || ev.mods.ctrl || ev.mods.alt) {
        s.shortcuts_open = !s.shortcuts_open;
        return true;
    }
    if ev.is_command('/') && !ev.mods.shift {
        s.shortcuts_open = !s.shortcuts_open;
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
    // `neko-rune --open <route>`: hand the route to the window that is running and exit. With none running this launch opens it.
    let mut first_route = None;
    if !headless_run() {
        let raw: Vec<String> = std::env::args().skip(1).collect();
        if let Some(route) = deeplink::route_arg(&raw) {
            match deeplink::forward(&route, &deeplink::socket_path()) {
                Ok(()) => std::process::exit(0),
                // Only "nothing is listening" starts a window; a refusal or silence from a running one is not a second window's job.
                Err(why) if why == deeplink::NO_WINDOW => first_route = deeplink::clean_route(&route),
                Err(why) => {
                    eprintln!("neko-rune: {why}");
                    std::process::exit(1);
                }
            }
        }
    }
    FIRST_ROUTE.with(|r| *r.borrow_mut() = first_route);
    // `neko-rune probe [json] [socket=PATH]`: no window; connect, print, exit. For checking a daemon
    // (`json` prints the workbench summary alone, for `scripts/snap-harness.sh`).
    if std::env::args().nth(1).as_deref() == Some("probe") {
        let socket = std::env::args().find_map(|a| a.strip_prefix("socket=").map(PathBuf::from)).unwrap_or_else(|| std::env::var("NEKO_SOCKET").map(PathBuf::from).unwrap_or_else(|_| neko_protocol::socket_path()));
        std::process::exit(connection::probe(socket, neko_protocol::support_dir(), std::time::Duration::from_secs(15), std::env::args().any(|a| a == "json")));
    }
    // The wgpu backend: the 3D viewer draws with depth, which Rune's own Metal backend cannot (docs/knowledge/rune-p3a-the-context-panel.md).
    rune::prewarm_gpu_on(rune::GpuBackend::Wgpu);
    let spec = rune::AppSpec {
        scenes: scenes::SCENES,
        min_size: Some((960.0, 560.0)),
        // The web UI's font stacks (`theme::FONT_SANS` starts with Inter, `FONT_MONO` with `ui-monospace`).
        families: (theme::FONT_SANS, theme::FONT_MONO),
        on_key: Some(on_key),
        poll: Some(|s: &mut App, t: f64, tx: &mut rune::Tx| {
            let moved = s.tick_stream(t);
            s.tick_layout(t, tx);
            let panel = s.panel_tick(t, tx);
            let toast = s.tick_toast(t, tx);
            s.tick_compose(t, tx);
            s.tick_dock();
            // The activity pill (only with `--overlays`): what Neko is doing, and when an outcome's time is up.
            s.tick_presence(tx);
            // Luau plugins: asks for the packs' panels when due, runs what is due in each, applies what they asked of the window.
            let luau = s.luau_tick(t, tx);
            // The Browser tab: tells its thread whether to watch, shows the newest frame.
            let browser = s.browser_tick(t, tx);
            // Tell the daemon what is on screen (at most every two seconds); ask to be woken for a change that has to wait.
            if let Some(wait) = s.tick_report(s.window_size.get()) {
                tx.wake_after(std::time::Duration::from_millis(wait.max(50) as u64));
            }
            moved || panel || toast || luau || browser
        }),
        // The waker finished reads and storage calls of a Luau plugin ask a frame with.
        setup: Some(|s: &mut App, waker: rune::Waker| {
            luau::nodes::set_waker(waker.clone());
            s.luau.waker = Some(waker);
        }),
        ..rune::AppSpec::new("Neko (Rune)", (1360.0, 840.0), init, view::view)
    };
    let mut app = rune::App::new(spec)
        .gpu_backend(rune::GpuBackend::Wgpu)
        .compositor(viewers::model3d_gpu::Viewer3d::default())
        .titlebar(rune::Titlebar::new(view::BAND).traffic_lights(view::TRAFFIC_LIGHTS.0, view::TRAFFIC_LIGHTS.1))
        .on_focus(|s: &mut App, focused: bool, _| s.set_focused(focused))
        // The web view (P3b) is built on the window's own handle, when first needed.
        .on_window(web_panel::remember_window);
    let handle = app.handle();
    // The overlays (P4) are off unless asked for: with the gate off nothing is created and no shortcut is registered (`overlays.rs`).
    let plan = overlays::plan(overlays::gate(rune::extra_args(), std::env::var(overlays::ENV).ok().as_deref()));
    let mut installed = Some(overlays::install(&mut app, plan));
    let mut started = false;
    app.on_ready(move |_| {
        if std::mem::replace(&mut started, true) {
            return;
        }
        // Routes from `neko-rune --open <route>` run in another process land here and bring the window forward.
        let opener = handle.clone();
        let _ = deeplink::listen(&deeplink::socket_path(), move |route| {
            opener.update_if(move |s, tx| {
                s.open_route(&route);
                tx.instant();
                true
            });
            opener.activate("");
        });
        if has("nolink") {
            return;
        }
        // The overlays: the window keeps their handles and registers the stop-all key (the summon key follows the daemon's answer).
        if let Some(made) = installed.take().filter(|i| i.plan.any()) {
            let main = handle.clone();
            handle.update_if(move |s, _| {
                s.overlays.attach(made, main);
                false
            });
        }
        // The window and the link thread share which task is open.
        let wanted = link::Wanted::default();
        let socket = socket_path();
        let (for_app, thread_handle) = (wanted.clone(), handle.clone());
        let dispatch = app::Dispatch::Async { socket: socket.clone(), handle: handle.clone() };
        // A viewer's heavy work (a decode, a PDF page, a CSV index) runs on a thread of its own; when it is
        // done the window builds its view again and the viewer picks the answer up.
        let waker = std::sync::Mutex::new(handle.clone());
        let wake: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(move || {
            waker.lock().unwrap_or_else(std::sync::PoisonError::into_inner).update_if(|_, tx| {
                tx.instant();
                true
            });
        });
        // The Browser tab's thread calls this after every change; one build is asked for at a time (frames can come 30 a second).
        let browser_waker = std::sync::Mutex::new(handle.clone());
        let asked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let browser_wake: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(move || {
            if asked.swap(true, std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            let done = asked.clone();
            browser_waker.lock().unwrap_or_else(std::sync::PoisonError::into_inner).update_if(move |s, tx| {
                done.store(false, std::sync::atomic::Ordering::SeqCst);
                s.browser.pull();
                tx.instant();
                true
            });
        });
        handle.update_if(move |s, _| {
            s.wanted = for_app;
            s.dispatch = dispatch;
            s.panel.states.use_threads(wake);
            s.browser.use_wake(browser_wake);
            false
        });
        connection::spawn(thread_handle, socket, neko_protocol::support_dir(), wanted);
    })
    .run();
}
