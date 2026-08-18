//! Verification-only tooling for the native-material task's acceptance
//! criteria (see `AGENTS.md`, "Window material" / "Summon latency"): none
//! of this runs in normal operation — every hook here is gated on its own
//! env var, unset by default. Kept separate from `main.rs` so the real
//! app's startup path stays readable.
//!
//! - `NEKO_SHOW_ON_LAUNCH=1` shows the summon panel immediately, skipping
//!   the hotkey/onboarding path — for a single screenshot.
//! - `NEKO_SHOW_QUERY=<text>` (only read alongside `NEKO_SHOW_ON_LAUNCH`)
//!   types `<text>` into the field before the screenshot — for capturing
//!   real search results (the provider-abstraction task's own evidence:
//!   apps, clipboard, and file results together in one list) without
//!   synthetic OS keystrokes, which turned out unreliable here beyond just
//!   the hotkey case `AGENTS.md`'s "Testing caveat" already documents (a
//!   `System Events` `keystroke` sent right after `activate_window` landed
//!   on the wrong frontmost process in practice). When set, `show_once`
//!   prints `neko: ready for evidence setup` and waits 2s before typing the
//!   query — room for an outside script to seed a real clipboard entry
//!   first, so the query can demonstrate a genuine clipboard match too,
//!   not just apps/files.
//! - `NEKO_SHOW_CONFIRM=1` (only read alongside `NEKO_SHOW_QUERY`) drives
//!   the field's top result through the exact same `Root::confirm` path a
//!   real Enter keystroke takes — `panel::Root::confirm_for_evidence` — for
//!   capturing the inline `"Couldn't open — …"` activation-failure footer
//!   (the neko-p0-fixes task's own evidence) without synthetic OS input,
//!   same reasoning as `NEKO_SHOW_QUERY` above. `show_once` waits for the
//!   query's own results to render, calls it, then waits once more for the
//!   real daemon round-trip `Request::Activate` makes before printing the
//!   window-number "now capture" line.
//! - `NEKO_BENCH=<n>` re-measures warm summon latency `n` times without
//!   synthetic OS keystrokes (unreliable — `AGENTS.md`, "Summon latency")
//!   or repeated `cx.activate(true)` (steals focus each time); prints one
//!   `neko: bench summon N latency …` line per summon, then exits.
//! - `NEKO_BENCH_REAL=<n>` — added for the client memory-leak investigation
//!   (`docs/evidence/neko-leak-audit-confirmation.md`) — drives the *real*
//!   summon/dismiss path `n` times: `window.activate_window()` +
//!   `cx.activate(true)` to show, `cx.hide()` to dismiss, exactly what
//!   `main.rs`'s hotkey handler calls, unlike `NEKO_BENCH` above (which
//!   deliberately avoids `activate_window`/`cx.hide()` so a long run
//!   doesn't steal focus — see its own doc comment). This one does steal
//!   focus and takes over the screen for real on every cycle, which is why
//!   it's meant for a small, deliberate iteration count, not a long bench.
//!   See `run_bench_real`'s own doc comment for the exact stderr markers an
//!   outside script samples `vmmap`/`footprint` against.
//! - `NEKO_BACKDROP_IMAGE=<path>` opens a second, full-display window
//!   showing the given image at `NSNormalWindowLevel` — strictly *below*
//!   the summon panel's own `NSPopUpWindowLevel` (`gpui-0.2.2`'s own
//!   `WindowKind` → level mapping), so it never obscures the panel and
//!   never needs its own ordering call. This is what stands in for a real
//!   desktop when checking material legibility or live compositing,
//!   instead of screenshotting whatever is genuinely behind the window on
//!   this machine (which may be another live session's real work — see
//!   `AGENTS.md`).

use std::path::PathBuf;
use std::time::Instant;

use gpui::{
    App, AsyncApp, Context, Focusable, IntoElement, ObjectFit, Render, Timer, Window,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, img, prelude::*,
};
use neko_client::NekoClient;
use neko_protocol::Request;

use crate::material;
use crate::panel::{PANEL_HEIGHT_PX, Root};
use crate::{display_placement, theme};

const BENCH_ENV_VAR: &str = "NEKO_BENCH";
const BENCH_REAL_ENV_VAR: &str = "NEKO_BENCH_REAL";
const SHOW_ON_LAUNCH_ENV_VAR: &str = "NEKO_SHOW_ON_LAUNCH";
const SHOW_QUERY_ENV_VAR: &str = "NEKO_SHOW_QUERY";
const SHOW_CONFIRM_ENV_VAR: &str = "NEKO_SHOW_CONFIRM";
const BACKDROP_IMAGE_ENV_VAR: &str = "NEKO_BACKDROP_IMAGE";

pub fn bench_iterations() -> Option<u32> {
    std::env::var(BENCH_ENV_VAR).ok()?.parse().ok()
}

pub fn bench_real_iterations() -> Option<u32> {
    std::env::var(BENCH_REAL_ENV_VAR).ok()?.parse().ok()
}

pub fn show_on_launch_requested() -> bool {
    std::env::var_os(SHOW_ON_LAUNCH_ENV_VAR).is_some()
}

pub fn show_query() -> Option<String> {
    std::env::var(SHOW_QUERY_ENV_VAR).ok()
}

pub fn show_confirm_requested() -> bool {
    std::env::var_os(SHOW_CONFIRM_ENV_VAR).is_some()
}

pub fn backdrop_image_path() -> Option<PathBuf> {
    std::env::var_os(BACKDROP_IMAGE_ENV_VAR).map(PathBuf::from)
}

/// Opens the full-display backdrop window described in this module's own
/// doc comment. Best-effort: silently does nothing if there is no primary
/// display, which never happens on the real hardware this is run on.
pub fn open_backdrop_window(cx: &mut App, image_path: PathBuf) {
    let Some(display) = cx.primary_display() else {
        return;
    };
    let bounds = display.bounds();
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            kind: WindowKind::Normal,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            focus: false,
            show: true,
            ..Default::default()
        },
        move |_window, cx| cx.new(|_cx| Backdrop { image_path: image_path.clone() }),
    );
}

struct Backdrop {
    image_path: PathBuf,
}

impl Render for Backdrop {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        img(self.image_path.clone())
            .w_full()
            .h_full()
            .object_fit(ObjectFit::Fill)
    }
}

/// Shows the summon panel once, immediately — `NEKO_SHOW_ON_LAUNCH`. Marks
/// onboarding complete first so it can't cover the panel for this run.
pub async fn show_once(client: &NekoClient, window: WindowHandle<Root>, cx: &mut AsyncApp) {
    let _ = client.request(Request::SetOnboardingComplete { completed: true }).await;
    let query = show_query();
    if query.is_some() {
        // A real daemon is now up and connected — printed so an outside
        // script driving `NEKO_SHOW_QUERY` (e.g. to seed a clipboard entry
        // before the query it wants to demonstrate actually fires) has a
        // deterministic "go" signal instead of guessing a sleep against
        // this process's own startup time.
        eprintln!("neko: ready for evidence setup");
        Timer::after(std::time::Duration::from_millis(2000)).await;
    } else {
        Timer::after(std::time::Duration::from_millis(300)).await;
    }
    let _ = cx.update(|cx| {
        let _ = window.update(cx, |root, window, cx| {
            root.reset_for_summon(window, cx);
            if let Some(query) = query.as_deref() {
                root.set_query_for_evidence(query, cx);
            }
        });
    });
    if query.is_some() {
        // `set_query_for_evidence` re-runs search the same way a real
        // keystroke does — a real daemon round-trip, not instant. File
        // results in particular can take up to `files::QUERY_TIMEOUT`
        // (1.5s) on a broad query, plus real observed scheduling variance
        // on a machine also running other `mdfind`-driven work; wait
        // comfortably past that so the window-number line below (an
        // outside script's "now capture" signal) isn't printed before the
        // row this evidence run exists to show has actually rendered.
        Timer::after(std::time::Duration::from_millis(3500)).await;
    }
    if show_confirm_requested() {
        let _ = cx.update(|cx| {
            let _ = window.update(cx, |root, window, cx| {
                root.confirm_for_evidence(window, cx);
            });
        });
        // `confirm()` makes its own real `Request::Activate` daemon
        // round-trip before it sets `activation_error` and notifies — wait
        // for it to land before the window-number "now capture" line below
        // prints, or the screenshot below would race the still-in-flight
        // request and show the pre-confirm footer instead.
        Timer::after(std::time::Duration::from_millis(800)).await;
    }
    let _ = cx.update(|cx| {
        let _ = window.update(cx, |root, window, cx| {
            window.activate_window();
            window.focus(&root.focus_handle(cx));
            if let Ok(number) = material::window_number(window) {
                eprintln!("neko: window number {number}");
            }
            // Points, top-left origin, `-R<x,y,w,h>`-ready — lets an
            // outside script capture *exactly* this window's own on-screen
            // rect (`screencapture -R`) rather than the whole display, the
            // window-scoped equivalent of the investigation's own
            // `crop_rect_px` (report §6/§10).
            let b = window.bounds();
            eprintln!(
                "neko: window rect {} {} {} {}",
                b.origin.x, b.origin.y, b.size.width, b.size.height
            );
        });
        cx.activate(true);
    });
}

/// Re-measures warm summon latency `iterations` times — `NEKO_BENCH`. Each
/// cycle orders the real native window front/out directly
/// (`material::order_front_regardless`/`order_out`) rather than going
/// through `Window::activate_window`/`cx.hide()`, so a long bench run
/// doesn't repeatedly steal focus from whatever else is on screen. Exits
/// the process when done.
///
/// **Also runs the real `display_placement::reposition_to_cursor_display`
/// call every cycle**, exactly as the two real summon entry points in
/// `main.rs` do — an earlier version of this bench measured
/// `order_front_regardless` alone, which silently excluded the
/// repositioning step from both the latency number and any RSS growth it
/// might cause. **Result, not hypothesis**: 60 cycles of this bench under
/// an isolated `HOME` measured flat RSS (~65MB, no growth) and 4-7ms
/// latency — this *rules out* `reposition_to_cursor_display` and the
/// `order_front_regardless`/`order_out` ordering path as leak sources,
/// it does not reproduce a leak. See `docs/evidence/` for the client
/// memory-leak investigation this bench mode was extended to support —
/// the real path (`window.activate_window()` + `cx.activate(true)` /
/// `cx.hide()`, which this mode still does not exercise) is the next
/// divergence point, covered by `run_bench_real` below.
pub async fn run_bench(client: &NekoClient, window: WindowHandle<Root>, cx: &mut AsyncApp, iterations: u32) {
    let _ = client.request(Request::SetOnboardingComplete { completed: true }).await;
    Timer::after(std::time::Duration::from_millis(300)).await;

    let panel_size = gpui::size(gpui::px(theme::PANEL_WIDTH_PX), gpui::px(PANEL_HEIGHT_PX));

    for i in 0..iterations {
        let started = Instant::now();
        let _ = cx.update(|cx| {
            let _ = window.update(cx, |root, window, cx| {
                root.reset_for_summon(window, cx);
                if let Err(e) = display_placement::reposition_to_cursor_display(window, panel_size) {
                    eprintln!("neko: bench reposition failed: {e}");
                }
                let _ = material::order_front_regardless(window);
                window.on_next_frame(move |_, _| {
                    eprintln!("neko: bench summon {i} latency {:?}", started.elapsed());
                });
            });
        });
        Timer::after(std::time::Duration::from_millis(150)).await;
        let _ = cx.update(|cx| {
            let _ = window.update(cx, |_root, window, _cx| {
                let _ = material::order_out(window);
            });
        });
        Timer::after(std::time::Duration::from_millis(80)).await;
    }

    eprintln!("neko: bench complete ({iterations} summons)");
    std::process::exit(0);
}

/// Drives the *real* summon/dismiss path `iterations` times —
/// `NEKO_BENCH_REAL`. See this module's own doc comment for why this is a
/// separate mode from `run_bench` above: that one's `order_front_regardless`/
/// `order_out` never make the window key and so structurally cannot reach
/// gpui's `windowDidBecomeKey:` handler
/// (`gpui-0.2.2/src/platform/mac/window.rs:1976-2027`) — the code path
/// `docs/evidence/neko-leak-audit-confirmation.md` traces as the real
/// summon path's one structural divergence from the proven-flat bench. This
/// mode calls exactly what `main.rs`'s hotkey handler calls: `reset_for_summon`,
/// the real `display_placement::reposition_to_cursor_display`, `window.focus`,
/// `window.activate_window()`, `cx.activate(true)` to show; `cx.hide()` to
/// dismiss.
///
/// Prints `neko: real-bench pid {pid}` once at the very start, then one
/// `neko: real-bench cycle {i} {phase}` line per phase
/// (`activating`/`activated`/`hiding`/`hidden`) per cycle — an outside
/// script samples `vmmap`/`footprint` against that exact pid right after
/// each `activated`/`hidden` line, during the deliberate settle delay that
/// follows it, rather than on a guessed sleep. The delay after `activated`
/// is long enough to cover both the forced synchronous draw
/// `windowDidBecomeKey:` triggers on every activation after the first
/// (`activated_least_once`) and any async Metal command-buffer completion
/// handler it schedules, not just the frame callback itself.
///
/// Iteration counts here are expected to be small — the memory-leak
/// investigation that added this mode used 5, matching the captain's own
/// real repro — since each cycle takes over the screen for real and, if the
/// investigation's hypothesis is right, costs multiple gigabytes.
pub async fn run_bench_real(client: &NekoClient, window: WindowHandle<Root>, cx: &mut AsyncApp, iterations: u32) {
    let _ = client.request(Request::SetOnboardingComplete { completed: true }).await;
    eprintln!("neko: real-bench pid {}", std::process::id());
    Timer::after(std::time::Duration::from_millis(300)).await;

    let panel_size = gpui::size(gpui::px(theme::PANEL_WIDTH_PX), gpui::px(PANEL_HEIGHT_PX));

    for i in 0..iterations {
        eprintln!("neko: real-bench cycle {i} activating");
        let started = Instant::now();
        let _ = cx.update(|cx| {
            let _ = window.update(cx, |root, window, cx| {
                root.reset_for_summon(window, cx);
                if let Err(e) = display_placement::reposition_to_cursor_display(window, panel_size) {
                    eprintln!("neko: real-bench reposition failed: {e}");
                }
                window.activate_window();
                window.focus(&root.focus_handle(cx));
                window.on_next_frame(move |_, _| {
                    eprintln!("neko: real-bench cycle {i} frame latency {:?}", started.elapsed());
                });
            });
            cx.activate(true);
        });
        // Settle past the frame callback and gpui's own forced-draw /
        // completion-handler window (this function's own doc comment)
        // before printing `activated` — the moment an outside script should
        // sample.
        Timer::after(std::time::Duration::from_millis(400)).await;
        eprintln!("neko: real-bench cycle {i} activated");
        Timer::after(std::time::Duration::from_millis(900)).await;

        eprintln!("neko: real-bench cycle {i} hiding");
        let _ = cx.update(|cx| cx.hide());
        Timer::after(std::time::Duration::from_millis(400)).await;
        eprintln!("neko: real-bench cycle {i} hidden");
        Timer::after(std::time::Duration::from_millis(900)).await;
    }

    eprintln!("neko: real-bench complete ({iterations} cycles)");
    std::process::exit(0);
}
