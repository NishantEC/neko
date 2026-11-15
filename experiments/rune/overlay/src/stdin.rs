//! The stdin control channel.
//!
//! ```text
//! 640 380 Clicking Save     move the pointer (points from the display's top-left) and label it
//! hide-label                clear the label
//! capsule 3 NK-15           update the capsule
//! toggle | show | hide      launcher panel
//! demo                      walk the pointer through five waypoints
//! bench 20 panel|overlay    time 20 summons (median and p95 to stdout)
//! status                    window numbers, visibility, availability
//! quit
//! ```

use std::io::BufRead;
use std::sync::Arc;
use std::time::Duration;

use crate::{Handles, bench, launcher, pointer};

pub fn spawn(h: Arc<Handles>, app: rune::AppHandle<launcher::Launcher>) {
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            let line = line.trim();
            if line.is_empty() { continue; }
            let mut words = line.split_whitespace();
            let first = words.next().unwrap_or_default();
            match first {
                "quit" => { app.quit(); return; }
                "toggle" => toggle(&h),
                "show" => show(&h),
                "hide" => h.panel.close(),
                "hide-label" => h.pointer.update(|s, _| s.label.clear()),
                "capsule" => {
                    let agents = words.next().and_then(|w| w.parse().ok()).unwrap_or(1);
                    let ticket = words.next().unwrap_or("NK-1").to_string();
                    h.capsule.update(move |s, _| { s.agents = agents; s.ticket = ticket; });
                }
                "demo" => {
                    let h = h.clone();
                    std::thread::spawn(move || {
                        for k in 0..pointer::DEMO.len() {
                            h.pointer.update(move |s, _| pointer::demo_step(s, k));
                            std::thread::sleep(Duration::from_secs_f64(pointer::DEMO_STEP_SECONDS));
                        }
                    });
                }
                "bench" => {
                    let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(20);
                    let kind = if words.next() == Some("overlay") { "overlay" } else { "panel" };
                    bench::run(&h, n, kind);
                }
                "status" => {
                    println!(
                        "status panel_visible={} pointer_window={:?} capsule_window={:?} pointer_availability={:?} capsule_availability={:?} pointer_capture={:?}",
                        h.panel.is_visible(), h.pointer.window_number(), h.capsule.window_number(),
                        h.pointer.availability(), h.capsule.availability(), h.pointer.capabilities(),
                    );
                    if let Some(e) = h.panel.take_shortcut_errors().into_iter().next() { println!("status hotkey_error={e:?}"); }
                }
                _ => match (first.parse::<f32>(), words.next().and_then(|w| w.parse::<f32>().ok())) {
                    (Ok(x), Some(y)) => {
                        let label = words.collect::<Vec<_>>().join(" ");
                        h.cursor.set([x, y]);
                        h.pointer.update(move |s, _| s.label = label);
                    }
                    _ => eprintln!("unknown command: {line}"),
                },
            }
        }
        // stdin closed: leave the overlays up until killed, like a daemon would.
    });
}

fn show(h: &Handles) {
    bench::start("panel");
    let (display, placement) = launcher::place_top_centre();
    h.panel.update(|s, _| s.reset());
    h.panel.place(display, placement);
    h.panel.show();
}

fn toggle(h: &Handles) {
    if h.panel.is_visible() { h.panel.close() } else { show(h) }
}
