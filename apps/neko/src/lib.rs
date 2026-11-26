//! Neko on the desktop: a thin client over the daemon, drawn with Rune and rune-ui (docs/knowledge/app-shell.md).
//!
//! It draws what the daemon says and sends what the person asks; it keeps no records of its own beyond where its window was.

pub mod about;
pub mod clock;
pub mod deeplink;
pub mod dock;
pub mod icons;
pub mod link;
pub mod menu;
pub mod needs_view;
pub mod notify;
pub mod review_view;
pub mod scenes;
pub mod settings_view;
pub mod state;
pub mod task_view;
/// rune-ui's components, copied in by the `rune-ui` CLI (`rune-ui.lock`); never edited here (docs/knowledge/rune-ui-in-neko.md).
pub mod ui;
pub mod view;
pub mod window_place;

use std::cell::Cell;
use std::path::PathBuf;

use rune::{Key, KeyEvent, Tx};
use state::{App, Daemon};

/// The prefs store for the window's frame (`~/Library/Application Support/dev.neko.app/prefs.json`), apart from the daemon's data.
pub const PREFS_ID: &str = "dev.neko.app";
/// The workspace chosen last for a new task (its id), in the same prefs.
pub const WORKSPACE_KEY: &str = "last-workspace";
/// The sidebar's folded workspace sections (their ids), in the same prefs.
pub const FOLDED_KEY: &str = "folded-workspaces";

thread_local! {
    /// The window's width as the last layout saw it (written by the view, read by `poll`).
    pub static WIDTH: Cell<Option<f32>> = const { Cell::new(None) };
    /// Whether the window is full screen (or going there), as the view last saw it.
    pub static FULL_SCREEN: Cell<bool> = const { Cell::new(false) };
    /// The last run's window frame, read once before the window is made.
    static SAVED: Cell<Option<window_place::Frame>> = const { Cell::new(None) };
}

/// Whether this process renders headless (`--snap`, `--tree`, `--script`, …): it then never touches the daemon, the prefs or the Dock.
pub fn headless() -> bool {
    rune::is_headless()
}

/// The data folder: `NEKO_DATA_DIR`, else `~/Library/Application Support/neko` (the daemon's default too).
pub fn data_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("NEKO_DATA_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/neko"))
}

/// A fixed moment for pictures and tests: 2026-11-26 at 14:05, in a zone at UTC.
pub fn fixed_clock() -> (i64, i64) {
    (1_795_701_900_000, 0)
}

/// The state of a picture or a test: no daemon, no prefs, the fixed clock.
pub fn headless_state() -> App {
    let (now_ms, utc_offset_s) = fixed_clock();
    App { now_ms, utc_offset_s, clock: fixed_clock, home: Some("/Users/example".into()), ..App::default() }
}

fn init() -> App {
    // The width is known before the first frame (the picture's size, the window's saved frame), so a narrow window opens with its
    // sidebar already folded instead of folding a frame later.
    if headless() {
        let width = rune::tooling::headless_run().map(|r| r.size().0);
        return App { width, ..headless_state() };
    }
    let (now_ms, utc_offset_s) = state::system_clock();
    let saved = SAVED.with(Cell::get);
    let width = Some(saved.map_or(window_place::DEFAULT.0, |f| f.w.max(window_place::MIN.0)) as f32);
    // The workspace chosen last time (checked against the daemon's list when it arrives) and the folded sections.
    let prefs = rune::prefs::Prefs::open(PREFS_ID);
    let workspace = prefs.get::<String>(WORKSPACE_KEY).filter(|w| !w.is_empty()).unwrap_or_else(|| neko_protocol::GENERAL.to_owned());
    let folded = prefs.get::<Vec<String>>(FOLDED_KEY).unwrap_or_default().into_iter().collect();
    // The real window posts real notifications; everything else keeps the recorder `App::default` gives.
    let mut notifier = notify::Notifier::new(Box::new(notify::System));
    notifier.switches = prefs.get::<notify::Switches>(notify::SWITCHES_KEY).unwrap_or_default();
    App { daemon: Daemon::Starting, now_ms, utc_offset_s, clock: state::system_clock, window: window_place::Keeper::new(saved), persist: true, width, workspace, folded, home: std::env::var("HOME").ok(), notifier, ..App::default() }
}

/// New Task (⌘N): Home with the keyboard in the composer, in the workspace of the task in view.
pub fn new_task(s: &mut App, tx: &mut Tx) {
    s.new_task_here();
    tx.instant();
}

/// The Mac's folder panel: a folder to add as a workspace, or where a workspace's folder is now.
pub fn pick_folder(s: &App, pick: state::Pick, tx: &mut Tx) {
    let (title, start) = match &pick {
        state::Pick::Add { .. } => ("Add a folder as a workspace", None),
        state::Pick::Locate(id) => ("Where is this workspace's folder now?", s.workspace_of(id).and_then(|w| w.folder.as_deref()).and_then(|f| std::path::Path::new(f).parent()).filter(|p| p.is_dir()).map(PathBuf::from)),
    };
    let options = rune::FileDialogOptions { title: title.into(), directory: start, ..Default::default() };
    tx.open_folder(options, move |s: &mut App, answer: rune::FileDialogResult, _: &mut Tx| {
        if let Ok(Some(path)) = answer {
            let path = path.to_string_lossy().into_owned();
            match &pick {
                state::Pick::Add { select } => s.add_workspace(path, *select),
                state::Pick::Locate(id) => s.locate(id, path),
            }
        }
    });
}

/// Keys no control took.
pub fn on_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let m = ev.mods;
    if ev.is_command('n') && !(m.shift || m.alt || m.ctrl) {
        new_task(s, tx);
        return true;
    }
    if (ev.is_command('1') || ev.is_command('2')) && !(m.shift || m.alt || m.ctrl) {
        if ev.is_command('1') {
            s.go_home();
        } else {
            s.open_needs();
        }
        tx.instant();
        return true;
    }
    if ev.is_command(',') && !(m.shift || m.alt || m.ctrl) {
        s.open_settings();
        tx.instant();
        return true;
    }
    if rune_kit::shell::is_sidebar_toggle(ev) || (ev.is_command('s') && m.ctrl && !(m.shift || m.alt)) {
        s.toggle_sidebar();
        tx.instant();
        return true;
    }
    if ev.key == Key::Escape && !(m.cmd || m.shift || m.alt || m.ctrl) {
        // Escape closes the context panel first (focus goes back to its ring); then it stops the open task's agent while it
        // works; otherwise it gives focus back.
        if s.context_open {
            s.context_open = false;
            return true;
        }
        if !s.settings_open && !s.needs_open && s.review.is_some() {
            s.close_review();
            return true;
        }
        if !s.settings_open && s.open_is_working() {
            s.stop();
            return true;
        }
        if s.needs_open {
            s.leave_needs();
            return true;
        }
        s.draft.focused = false;
        tx.blur();
        return true;
    }
    false
}

/// Each frame: what the link brought, the clock, the width, the window's title and frame, the menu.
pub fn poll(s: &mut App, t: f64, tx: &mut Tx) -> bool {
    let mut changed = s.drain_link();
    for e in std::mem::take(&mut s.effects) {
        match e {
            state::Effect::Reveal(path) => {
                let _ = tx.reveal_path(&path);
            }
            state::Effect::PickFolder(pick) => pick_folder(s, pick, tx),
            state::Effect::Front => {
                if s.persist {
                    rune_notify::bring_to_front();
                }
            }
        }
        changed = true;
    }
    if let Some(key) = s.focus_next.take() {
        tx.focus(key, true);
    }
    if let Some((words, urgent)) = s.announce.take() {
        tx.announce(words, if urgent { rune::LiveRegion::Assertive } else { rune::LiveRegion::Polite });
    }
    let (now, offset) = (s.clock)();
    if now / 60_000 != s.now_ms / 60_000 {
        changed = true;
    }
    (s.now_ms, s.utc_offset_s) = (now, offset);
    if let Some(w) = WIDTH.with(Cell::get)
        && s.width != Some(w)
    {
        let before = s.sidebar_shown();
        s.set_width(w);
        changed |= before != s.sidebar_shown();
    }
    s.full_screen = FULL_SCREEN.with(Cell::get);
    let title = s.window_title();
    if title != s.title_set {
        tx.set_title(title.clone());
        s.title_set = title;
    }
    if s.persist {
        // Whether Neko is in front decides what is worth a notification; coming back to it (from System Settings, say) reads
        // the permission again.
        let front = rune_notify::app_focused();
        if front && !s.front {
            s.read_allowed_soon();
        }
        s.front = front;
        let badge = s.badge_count();
        if s.badge_set != Some(badge)
            && let Some(w) = &s.window_handle
        {
            let words = if badge == 1 { "1 thing needs you".to_owned() } else { format!("{badge} things need you") };
            w.set_taskbar(rune::Taskbar::new().badge(badge, words));
            s.badge_set = Some(badge);
        }
        let (write, again) = s.window.look(t, window_place::current(s.full_screen));
        if let Some(f) = write {
            save_window(f);
        }
        if let Some(at) = again {
            tx.wake_after(std::time::Duration::from_secs_f64((at - t).max(0.05)));
        }
        let shown = (s.sidebar_shown(), s.full_screen);
        if s.menu_shown != Some(shown)
            && let Some(h) = &s.handle
        {
            let _ = h.set_menu(menu::items(&menu::model(shown.0, shown.1)));
            s.menu_shown = Some(shown);
            s.menu_fix_until = t + 2.0;
        }
        if t < s.menu_fix_until {
            menu::native::fix_up();
            tx.wake_after(std::time::Duration::from_millis(250));
        }
    }
    // Waiting for the daemon: look again when the wait becomes long enough to say so.
    if let Some(at) = s.waiting_since {
        let left = s.slow_after.saturating_sub(at.elapsed());
        if left.is_zero() {
            if !s.slow_shown {
                s.slow_shown = true;
                changed = true;
            }
        } else {
            s.slow_shown = false;
            tx.wake_after(left + std::time::Duration::from_millis(20));
        }
    }
    // The minute changes the greeting and "Today": look again when it turns.
    tx.wake_after(std::time::Duration::from_millis((60_000 - now.rem_euclid(60_000)) as u64 + 50));
    changed
}

/// Writes the frame off the UI thread (no file I/O in the frame loop).
fn save_window(f: window_place::Frame) {
    let _ = std::thread::Builder::new().name("neko-prefs".into()).spawn(move || {
        let _ = rune::prefs::Prefs::open(PREFS_ID).set(window_place::KEY, &f);
    });
}

fn flush_window(s: &mut App, _: &mut Tx) {
    if s.persist
        && let Some(f) = s.window.flush(window_place::current(s.full_screen))
    {
        // At quit there is no frame loop left to keep smooth: write now, so it is on disk before the process ends.
        let _ = rune::prefs::Prefs::open(PREFS_ID).set(window_place::KEY, &f);
    }
}

/// The app, as both a window and a headless run build it.
pub fn spec() -> rune::AppSpec<App> {
    rune::AppSpec {
        scenes: scenes::SCENES,
        min_size: Some((window_place::MIN.0 as f32, window_place::MIN.1 as f32)),
        fonts: rune_ui_style::fonts::install(ui::FONTS),
        on_key: Some(on_key),
        poll: Some(poll),
        ..rune::AppSpec::new("Neko", (window_place::DEFAULT.0 as f32, window_place::DEFAULT.1 as f32), init, view::view)
    }
}

/// The app with its titlebar and menus, before it runs.
pub fn app() -> rune::App<App> {
    // Tab reaches every control, not only fields and lists, whatever the Mac's own Keyboard Navigation setting says.
    let mut app = rune::App::new(spec()).standard_menus().tab_visits_controls(true).titlebar(rune::Titlebar::new(view::BAND)).on_open_url(open_url);
    for item in menu::items(&menu::model(true, false)) {
        app = app.menu_item(item);
    }
    app
}

/// A link the system opened with Neko (`neko://task/<id>`, from a notification, a browser or `open`): the task, in front.
fn open_url(s: &mut App, link: &str, tx: &mut Tx) {
    if s.open_link(link) {
        tx.instant();
    }
}

/// The app for a test's headless runtime: [`headless_state`], whatever the process's arguments.
pub fn test_app() -> rune::App<App> {
    app().init(headless_state)
}

/// Starts the link to the daemon of `dir`. Returns at once: finding, starting and greeting the daemon happen on the link's own
/// thread; `wake` is called there whenever something arrived, and the window drains it on its own thread.
pub fn begin_link(dir: PathBuf, daemon: Option<PathBuf>, wake: impl Fn() + Send + Sync + 'static) -> link::Conn {
    link::Conn::start(dir, daemon, wake)
}

/// After the window's first frame: starts the link ([`begin_link`]) and hands it to the window.
pub fn start_link(handle: &rune::AppHandle<App>, dir: PathBuf, daemon: Option<PathBuf>) {
    let waker = handle.clone();
    let conn = begin_link(dir, daemon, move || {
        waker.update_if(|s: &mut App, tx| {
            let changed = s.drain_link();
            if changed {
                tx.instant();
            }
            changed
        });
    });
    let for_state = handle.clone();
    handle.update(move |s: &mut App, _| {
        s.link_started(conn);
        s.handle = Some(for_state);
    });
}

/// Opens the window, starts (or finds) the daemon and connects to it.
pub fn run() {
    if headless() {
        app().run();
        return;
    }
    // Before the first window: no Enter Full Screen of AppKit's own (View has Neko's).
    menu::native::prepare();
    let saved = rune::prefs::Prefs::open(PREFS_ID).get::<window_place::Frame>(window_place::KEY);
    SAVED.with(|c| c.set(saved));
    let app = app().keep_running().on_window(window_place::remember).on_close(flush_window).on_quit(flush_window);
    let handle = app.handle();
    // A click on one of Neko's notifications, from any thread: installed now, so the click that launched the app arrives too.
    let clicks = app.handle();
    rune_notify::on_click(move |c: rune_notify::Click| {
        let action = c.action().map(str::to_owned);
        let typed = action.as_deref().and_then(|a| c.input(a)).map(str::to_owned);
        let info: Vec<(String, String)> = c.info.iter().filter(|(k, _)| !k.starts_with("rune.")).cloned().collect();
        clicks.update(move |s: &mut App, tx| {
            s.clicked(action, typed, info);
            tx.instant();
        });
    });
    let mut started = false;
    // Nothing before the first frame waits for the daemon: the window opens with what it can show by itself, and the link (find or
    // start the daemon, connect, hello, the first lists) starts after the first frame, off the UI thread (`start_link`).
    let app = app.on_ready(move |window: &rune::WindowHandle| {
        if std::mem::replace(&mut started, true) {
            return;
        }
        // The Dock badge is the window's; whether Neko may post is asked of the system now (never of the person).
        let w = window.clone();
        handle.update(move |s: &mut App, _| {
            s.window_handle = Some(w);
            s.read_allowed_soon();
        });
        let Some(dir) = data_dir() else { return };
        start_link(&handle, dir, link::daemon_binary());
    });
    // Before Rune starts: the Dock registration must happen on this thread (`dock.rs`).
    dock::register_on_main_thread();
    rune::run_windows(move |displays| {
        let screens: Vec<window_place::Screen> = displays.iter().map(window_place::Screen::of).collect();
        let app = match window_place::restore(saved, &screens) {
            Some(open) => app.window_options(rune::WindowOptions { position: Some(open.position), physical_size: Some(open.size), ..Default::default() }),
            None => app,
        };
        [rune::WindowSpec::from_app("main", app)]
    });
}

#[cfg(test)]
mod tests {
    /// Nothing before the first frame waits for the daemon: `run` starts the link only from `on_ready` (after the first frame), and
    /// neither `init` nor `main`'s setup reaches the link. (The behaviour itself is tested in `tests/startup.rs`.)
    #[test]
    fn the_link_starts_only_after_the_first_frame() {
        let all = include_str!("lib.rs");
        let code = &all[..all.rfind("#[cfg(test)]").expect("this module")];
        let run = &code[code.find("pub fn run() {").expect("run")..];
        let run = &run[..run.find("\n}\n").expect("the end of run")];
        let ready = run.find(".on_ready(").expect("run starts the link from on_ready");
        let start = run.find("start_link(").expect("run starts the link");
        assert!(ready < start, "the link starts inside on_ready");
        assert!(!run.contains("Conn::start") && !run.contains("begin_link"), "run reaches the link only through start_link");
        let init = &code[code.find("fn init() -> App {").expect("init")..];
        let init = &init[..init.find("\n}\n").unwrap()];
        assert!(!init.contains("link") && !init.contains("Conn"), "init does not touch the link");
        assert_eq!(code.matches("link::Conn::start(").count(), 1, "one place starts the link: begin_link");
    }
}
