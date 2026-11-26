//! Neko on the desktop: a thin client over the daemon, drawn with Rune and rune-ui (docs/knowledge/app-shell.md).
//!
//! It draws what the daemon says and sends what the person asks; it keeps no records of its own beyond where its window was.

pub mod about;
pub mod clock;
pub mod dock;
pub mod icons;
pub mod link;
pub mod menu;
pub mod scenes;
pub mod state;
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
    App { now_ms, utc_offset_s, clock: fixed_clock, ..App::default() }
}

fn init() -> App {
    if headless() {
        return headless_state();
    }
    let (now_ms, utc_offset_s) = state::system_clock();
    let saved = SAVED.with(Cell::get);
    App { daemon: Daemon::Starting, now_ms, utc_offset_s, clock: state::system_clock, window: window_place::Keeper::new(saved), persist: true, ..App::default() }
}

/// New Task: Home with the keyboard in the composer.
pub fn new_task(s: &mut App, tx: &mut Tx) {
    s.go_home_and_focus();
    tx.instant();
}

/// Keys no control took.
pub fn on_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let m = ev.mods;
    if ev.is_command('n') && !(m.shift || m.alt || m.ctrl) {
        new_task(s, tx);
        return true;
    }
    if rune_kit::shell::is_sidebar_toggle(ev) || (ev.is_command('s') && m.ctrl && !(m.shift || m.alt)) {
        s.toggle_sidebar();
        tx.instant();
        return true;
    }
    if ev.key == Key::Escape && !(m.cmd || m.shift || m.alt || m.ctrl) {
        // Escape gives focus back: nothing keeps the keyboard.
        s.draft.focused = false;
        tx.blur();
        return true;
    }
    false
}

/// Each frame: what the link brought, the clock, the width, the window's title and frame, the menu.
pub fn poll(s: &mut App, t: f64, tx: &mut Tx) -> bool {
    let mut changed = s.drain_link();
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
    let mut app = rune::App::new(spec()).standard_menus().tab_visits_controls(true).titlebar(rune::Titlebar::new(view::BAND));
    for item in menu::items(&menu::model(true, false)) {
        app = app.menu_item(item);
    }
    app
}

/// The app for a test's headless runtime: [`headless_state`], whatever the process's arguments.
pub fn test_app() -> rune::App<App> {
    app().init(headless_state)
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
    let mut started = false;
    let app = app.on_ready(move |_| {
        if std::mem::replace(&mut started, true) {
            return;
        }
        let Some(dir) = data_dir() else { return };
        let waker = handle.clone();
        let conn = link::Conn::start(dir, link::daemon_binary(), move || {
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
            s.conn = Some(conn);
            s.handle = Some(for_state);
        });
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
