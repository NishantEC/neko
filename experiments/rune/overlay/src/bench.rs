//! Summon-latency measurement: the moment a caller asks for the launcher (hotkey handler entry,
//! `toggle`, or the bench loop) to the first frame Rune reports for it.
//!
//! "First frame" is `App::on_frame_encoded`'s `Drawn` for the panel host (taken after the
//! frame's commands were submitted, before the compositor shows it) and, for the overlay host,
//! the end of a `Compositor::render` that called `draw_ui` (before submit). Neither is the
//! scanout; add up to one display refresh (8.3 ms at 120 Hz).

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use rune::compose::{Compositor, Frame};

use crate::Handles;
use crate::launcher::{self, Launcher};

static T0: Mutex<Option<(&'static str, Instant)>> = Mutex::new(None);
static SEEN: (Mutex<Option<f64>>, Condvar) = (Mutex::new(None), Condvar::new());

pub fn tracing() -> bool {
    std::env::var_os("RUNE_OVERLAY_TRACE").is_some()
}

/// Mark the moment of the request.
pub fn start(kind: &'static str) {
    *T0.lock().unwrap() = Some((kind, Instant::now()));
}

/// A frame for `kind` was produced at `at`. Frames from before the request are ignored.
pub fn frame(kind: &'static str, at: Instant) {
    let mut t0 = T0.lock().unwrap();
    let Some((k, started)) = *t0 else { return };
    if k != kind { return; }
    let Some(elapsed) = at.checked_duration_since(started) else { return };
    *t0 = None;
    *SEEN.0.lock().unwrap() = Some(elapsed.as_secs_f64() * 1000.0);
    SEEN.1.notify_all();
}

/// The overlay host's compositor: the default frame, then a timestamp.
pub struct Probe;

impl Compositor<Launcher> for Probe {
    fn render(&mut self, f: &mut Frame<'_, Launcher>) {
        f.draw_ui();
        frame("overlay", Instant::now());
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let rank = ((p / 100.0) * sorted.len() as f64).ceil().max(1.0) as usize;
    sorted[rank.min(sorted.len()) - 1]
}

fn summary(label: &str, mut v: Vec<f64>, timeouts: usize) {
    if v.is_empty() {
        println!("bench {label}: no samples ({timeouts} timeouts)");
        return;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    println!(
        "bench {label}: n={} timeouts={timeouts} min={:.2} median={:.2} p95={:.2} max={:.2} (ms)",
        v.len(), v[0], percentile(&v, 50.0), percentile(&v, 95.0), v[v.len() - 1]
    );
}

/// `n` summons of `kind` ("panel" or "overlay"), each shown briefly, then hidden.
pub fn run(h: &Handles, n: usize, kind: &'static str) {
    let (mut frames, mut visible, mut timeouts) = (Vec::new(), Vec::new(), 0);
    for i in 0..n {
        *SEEN.0.lock().unwrap() = None;
        let (display, placement) = launcher::place_top_centre();
        let started = Instant::now();
        start(kind);
        match kind {
            "panel" => {
                h.panel.update(|s, _| s.reset());
                h.panel.place(display, placement);
                h.panel.show();
            }
            _ => {
                h.overlay_launcher.update(|s, _| s.reset());
                h.overlay_launcher.show();
            }
        }
        // Poll visibility while waiting for the frame: for the panel, `is_visible` flips once AppKit has ordered it in.
        let mut visible_ms = None;
        let deadline = started + Duration::from_secs(1);
        let mut seen = None;
        while Instant::now() < deadline {
            if kind == "panel" && visible_ms.is_none() && h.panel.is_visible() {
                visible_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
            }
            let guard = SEEN.0.lock().unwrap();
            let (guard, _) = SEEN.1.wait_timeout(guard, Duration::from_micros(200)).unwrap();
            if let Some(ms) = *guard {
                seen = Some(ms);
                if kind != "panel" || visible_ms.is_some() { break; }
            }
        }
        match seen {
            Some(ms) => frames.push(ms),
            None => timeouts += 1,
        }
        if let Some(ms) = visible_ms { visible.push(ms); }
        println!("bench {kind} i={i} first_frame_ms={} visible_ms={}",
            seen.map_or("none".into(), |m| format!("{m:.2}")), visible_ms.map_or("none".into(), |m| format!("{m:.2}")));
        std::thread::sleep(Duration::from_millis(350));
        match kind {
            "panel" => h.panel.close(),
            _ => h.overlay_launcher.hide(),
        }
        std::thread::sleep(Duration::from_millis(450));
    }
    summary(&format!("{kind} request->first frame"), frames, timeouts);
    if kind == "panel" {
        summary("panel request->is_visible", visible, 0);
    }
}
