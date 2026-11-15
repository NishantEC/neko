//! Summon-latency measurement: the moment a caller asks for the launcher (hotkey handler entry,
//! `toggle`, or the bench loop) to the frames Rune reports for it.
//!
//! Every frame the host draws after the request is timestamped, not only the first: Rune
//! refreshes a prewarmed panel's retained frame while it is still hidden (alpha zero) and draws
//! again once it is shown, so the first frame is not the first one a person could see. Frames:
//! `App::on_frame_encoded` `Drawn` for the panel host (taken after the commands were submitted,
//! before presentation), and for the overlay host the end of a `Compositor::render` that called
//! `draw_ui` (before submit). The panel's `is_visible()` flip is polled on the side. None of these
//! is the scanout; add up to one display refresh (8.3 ms at 120 Hz).

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use rune::compose::{Compositor, Frame};

use crate::Handles;
use crate::launcher::{self, Launcher};

static T0: Mutex<Option<(&'static str, Instant)>> = Mutex::new(None);
/// Milliseconds since the request, one entry per frame drawn since it.
static FRAMES: (Mutex<Vec<f64>>, Condvar) = (Mutex::new(Vec::new()), Condvar::new());

pub fn tracing() -> bool {
    std::env::var_os("RUNE_OVERLAY_TRACE").is_some()
}

/// Mark the moment of the request.
pub fn start(kind: &'static str) {
    FRAMES.0.lock().unwrap().clear();
    *T0.lock().unwrap() = Some((kind, Instant::now()));
}

/// A frame for `kind` was produced at `at`. Frames from before the request are ignored.
pub fn frame(kind: &'static str, at: Instant) {
    let Some((k, started)) = *T0.lock().unwrap() else { return };
    if k != kind { return; }
    let Some(elapsed) = at.checked_duration_since(started) else { return };
    let mut frames = FRAMES.0.lock().unwrap();
    if frames.len() < 64 { frames.push(elapsed.as_secs_f64() * 1000.0); }
    FRAMES.1.notify_all();
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

fn summary(label: &str, mut v: Vec<f64>, of: usize) {
    if v.is_empty() {
        println!("bench {label}: no samples (of {of})");
        return;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    println!(
        "bench {label}: n={}/{of} min={:.2} median={:.2} p95={:.2} max={:.2} (ms)",
        v.len(), v[0], percentile(&v, 50.0), percentile(&v, 95.0), v[v.len() - 1]
    );
}

fn fmt(v: Option<f64>) -> String {
    v.map_or("none".into(), |m| format!("{m:.2}"))
}

/// `n` summons of `kind` ("panel" or "overlay"), each shown briefly, then hidden.
pub fn run(h: &Handles, n: usize, kind: &'static str) {
    let (mut first, mut visible, mut shown) = (Vec::new(), Vec::new(), Vec::new());
    for i in 0..n {
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
        // Poll visibility on the side: for the panel, `is_visible` flips once AppKit has ordered it in.
        let mut visible_ms = None;
        let deadline = started + Duration::from_millis(400);
        while Instant::now() < deadline {
            if kind == "panel" && visible_ms.is_none() && h.panel.is_visible() {
                visible_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
            }
            let guard = FRAMES.0.lock().unwrap();
            let _ = FRAMES.1.wait_timeout(guard, Duration::from_micros(200)).unwrap();
        }
        let frames = FRAMES.0.lock().unwrap().clone();
        // The panel: the first frame drawn once the window reports visible. The overlay has no
        // visibility flag, so its second frame stands in for it (the first is the hidden refresh).
        let after_visible = match visible_ms {
            Some(v) => frames.iter().copied().find(|f| *f >= v),
            None => frames.get(1).copied(),
        };
        if let Some(f) = frames.first() { first.push(*f); }
        if let Some(v) = visible_ms { visible.push(v); }
        if let Some(s) = after_visible { shown.push(s); }
        println!(
            "bench {kind} i={i} first_frame_ms={} visible_ms={} first_frame_after_visible_ms={} frames={:?}",
            fmt(frames.first().copied()), fmt(visible_ms), fmt(after_visible),
            frames.iter().map(|f| (f * 100.0).round() / 100.0).collect::<Vec<_>>()
        );
        match kind {
            "panel" => h.panel.close(),
            _ => h.overlay_launcher.hide(),
        }
        std::thread::sleep(Duration::from_millis(450));
    }
    summary(&format!("{kind} request->first frame (hidden refresh)"), first, n);
    if kind == "panel" {
        summary("panel request->is_visible", visible, n);
        summary("panel request->first frame after visible", shown, n);
    } else {
        summary("overlay request->second frame", shown, n);
    }
}
