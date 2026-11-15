//! Spike: Rune system overlays on macOS. One process, three parts:
//!
//! 1. agent pointer: full-display click-through overlay, fed `x y label` lines on stdin;
//! 2. activity capsule: a glass pill at the top-centre, fed `capsule <agents> <ticket>`;
//! 3. launcher panel: `launcher_panel()` toggled by a global hotkey or the `toggle` stdin command.
//!
//! See REPORT.md. Headless: `--window pointer|capsule|launcher --snap out.png [--at T]`.

mod bench;
mod capsule;
mod diag;
mod launcher;
mod pointer;
mod stdin;

use std::sync::Arc;

use rune::{
    App, AppSpec, DisplayTarget, FrameOutcome, KeyPolicy, OverlayHandle, OverlayInput, OverlayOptions, OverlayPlacement, OverlaySpec,
    RunOptions, ScreenEdge, WindowOptions, WindowSpec,
};

use capsule::Capsule;
use launcher::Launcher;
use pointer::Pointer;

/// Everything the stdin thread may drive.
pub struct Handles {
    pub pointer: OverlayHandle<Pointer>,
    pub capsule: OverlayHandle<Capsule>,
    /// The launcher_panel host (an ordinary window with native panel options).
    pub panel: rune::AppHandle<Launcher>,
    /// The same view hosted as a plain overlay that can never become key; used for the no-key latency run.
    pub overlay_launcher: OverlayHandle<Launcher>,
    pub cursor: rune::Live<[f32; 2]>,
}

/// Hotkey and `toggle` both land here, on the app's own thread for the hotkey.
fn summon(s: &mut Launcher, _: &mut rune::Tx) {
    let Some(me) = s.me.clone() else { return };
    if me.is_visible() {
        me.close();
        return;
    }
    bench::start("panel");
    s.reset();
    let (display, placement) = launcher::place_top_centre();
    me.place(display, placement);
    me.show();
}

fn is_headless() -> bool {
    std::env::args().any(|a| {
        matches!(a.as_str(), "--snap" | "--trace" | "--tree" | "--frames" | "--strip" | "--first-pixels" | "--list-scenes" | "--list-windows" | "--script" | "--video" | "--desktop")
    })
}

fn main() {
    let headless = is_headless();
    let hotkey = std::env::var("RUNE_OVERLAY_HOTKEY").unwrap_or_else(|_| "ctrl+alt+shift+space".into());
    rune::run_windows_with(RunOptions { accessory: true }, move |displays| {
        let display = displays.iter().find(|d| d.primary).or(displays.first()).cloned();

        // Launcher: always a window with the native panel options, in windowed and headless runs.
        let launcher_app = App::new(AppSpec::new("Launcher", launcher::SIZE, || Launcher::new(None), launcher::view)).windowless();
        let me = launcher_app.handle();
        let me_for_state = me.clone();
        let mut launcher_app = launcher_app
            .init(move || Launcher::new(Some(me_for_state.clone())))
            .global_shortcut(&hotkey, summon)
            .on_frame_encoded(|f| {
                if bench::tracing() { eprintln!("frame window={:?} outcome={:?} seq={}", f.window(), f.outcome(), f.seq()); }
                if f.outcome() == FrameOutcome::Drawn && f.window() == Some("launcher") { bench::frame("panel", f.at()); }
            })
            .on_focus(|_, focused, tx| if !focused { tx.close_window() });
        let launcher_options = WindowOptions::launcher_panel();

        if headless {
            let d = display.expect("headless display");
            let scale = d.scale;
            let pointer_spec = WindowSpec::new("pointer", AppSpec { poll: Some(pointer::poll), ..AppSpec::new("Pointer", (1920.0, 1080.0), Pointer::new, pointer::view) })
                .options(WindowOptions::overlay(&d));
            let (cw, ch) = capsule::WINDOW;
            let capsule_options = WindowOptions {
                transparent: true, borderless: true, click_through: true, always_on_top: true,
                position: Some((d.position.0 + (((d.size.0 as f64 / scale) as f32 - cw) / 2.0 * scale as f32) as i32, (10.0 * scale) as i32)),
                min_size: Some((0.0, 0.0)), ..Default::default()
            };
            let capsule_spec = WindowSpec::new("capsule", AppSpec::new("Capsule", (cw, ch), Capsule::new, capsule::view)).options(capsule_options);
            let windows: Vec<rune::AnyWindowSpec> = vec![
                WindowSpec::from_app("launcher", launcher_app).configured_options(launcher_options).into(),
                pointer_spec.into(),
                capsule_spec.into(),
            ];
            return windows;
        }

        // Windowed: the pointer and the capsule are overlays of the launcher's app (separate state types).
        let pointer_state = Pointer::new();
        let cursor = pointer_state.cursor.clone();
        let pointer = launcher_app.overlay(
            OverlaySpec::new("Neko agent pointer", (1.0, 1.0), pointer::view).options(OverlayOptions {
                display: DisplayTarget::Primary, placement: Some(OverlayPlacement::FullScreen), input: OverlayInput::None,
                keys: KeyPolicy::None, prewarm: true, ..Default::default()
            }),
            pointer_state,
        );
        let capsule = launcher_app.overlay(
            OverlaySpec { edge: ScreenEdge::Top, margin: 10.0, ..OverlaySpec::new("Neko activity capsule", capsule::WINDOW, capsule::view) }
                .options(OverlayOptions { display: DisplayTarget::Primary, input: OverlayInput::None, keys: KeyPolicy::None, prewarm: true, ..Default::default() }),
            Capsule::new(),
        );
        let (display, placement) = launcher::place_top_centre();
        let overlay_launcher = launcher_app.overlay(
            OverlaySpec::new("Neko launcher (overlay host)", launcher::SIZE, launcher::view)
                .options(OverlayOptions {
                    display, placement: Some(placement), input: OverlayInput::All, keys: KeyPolicy::None, prewarm: true, ..Default::default()
                })
                .compositor(bench::Probe),
            Launcher::new(None),
        );
        pointer.show();
        capsule.show();
        let handles = Arc::new(Handles { pointer, capsule, panel: me.clone(), overlay_launcher, cursor });
        stdin::spawn(handles, me.clone());
        let windows: Vec<rune::AnyWindowSpec> = vec![WindowSpec::from_app("launcher", launcher_app).configured_options(launcher_options).into()];
        windows
    });
}
