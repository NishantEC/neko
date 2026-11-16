//! The link thread: connects to the daemon, keeps the app current, reconnects with backoff.
//! Everything it sends in is a [`LinkEvent`] or the web UI's address; it receives nothing from
//! the app and writes nothing to the daemon (`link/` only knows read requests).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rune::AppHandle;

use crate::app::App;
use crate::link::{LinkEvent, UnixConnector, run};
use crate::web_link::WebTarget;

/// One line for a link event: what `NEKO_RUNE_LOG=1` and `probe` print.
pub fn describe(event: &LinkEvent) -> String {
    match event {
        LinkEvent::Connecting => "connecting".to_owned(),
        LinkEvent::Live(s) => format!("live: {} tasks, {} need you, {} in review, {} working", s.tasks.len(), s.needs_you(), s.in_review(), s.working()),
        LinkEvent::Quiet => "quiet: nothing changed".to_owned(),
        LinkEvent::Down { error, attempt, retry_in } => format!("down (attempt {attempt}, retry in {retry_in:?}): {}", crate::link::calm(error, *retry_in)),
    }
}

/// `neko-rune probe`: connect to a daemon without a window, print what the link sees, exit 0 once
/// the workbench arrives and 1 if it does not within `limit`. Read-only, like the window.
pub fn probe(socket: PathBuf, data_dir: PathBuf, limit: Duration, json: bool) -> i32 {
    let connector = UnixConnector::new(socket);
    let stop = AtomicBool::new(false);
    let live = AtomicBool::new(false);
    let started = std::time::Instant::now();
    run(
        &connector,
        &stop,
        |event| {
            // With `json`, stdout carries only the summary (for `scripts/snap-harness.sh`); the
            // narration moves to stderr.
            if json { eprintln!("{}", describe(&event)) } else { println!("{}", describe(&event)) }
            if let (true, LinkEvent::Live(summary)) = (json, &event) {
                println!("{}", serde_json::to_string(summary).unwrap_or_default());
            }
            if matches!(event, LinkEvent::Live(_)) {
                live.store(true, Ordering::Relaxed);
                let port = std::env::var("NEKO_WEB_PORT").ok();
                match WebTarget::discover(&data_dir, port.as_deref()) {
                    Some(web) if !json => println!("web UI: {}", web.display()),
                    None if !json => println!("web UI: address not known (no web-token yet)"),
                    _ => {}
                }
                stop.store(true, Ordering::Relaxed);
            } else if started.elapsed() > limit {
                stop.store(true, Ordering::Relaxed);
            }
        },
        |d| std::thread::sleep(d.min(Duration::from_millis(500))),
    );
    if live.load(Ordering::Relaxed) { 0 } else { 1 }
}

/// Starts the worker. `socket` is the daemon's Unix socket, `data_dir` where it keeps `web-token`.
pub fn spawn(handle: AppHandle<App>, socket: PathBuf, data_dir: PathBuf) {
    let spawned = std::thread::Builder::new().name("neko-link".into()).spawn(move || {
        let connector = UnixConnector::new(socket);
        let stop = AtomicBool::new(false);
        let port = std::env::var("NEKO_WEB_PORT").ok();
        let mut web: Option<WebTarget> = None;
        run(
            &connector,
            &stop,
            |event| {
                if std::env::var_os("NEKO_RUNE_LOG").is_some() {
                    eprintln!("neko-rune link: {}", describe(&event));
                }
                // The token is read here, off the UI thread, whenever the link starts, drops or
                // first goes live: it may not exist before the daemon has run once.
                let look = matches!(event, LinkEvent::Connecting | LinkEvent::Down { .. }) || (matches!(event, LinkEvent::Live(_)) && web.is_none());
                if look {
                    let found = WebTarget::discover(&data_dir, port.as_deref());
                    if found != web {
                        web = found.clone();
                        handle.update_if(move |s, tx| {
                            tx.instant();
                            s.set_web(found)
                        });
                    }
                }
                handle.update_if(move |s, tx| {
                    // What arrives just after launch lands without animating (Rune's "keep the
                    // first second still").
                    tx.instant();
                    s.apply_link(event)
                });
            },
            std::thread::sleep,
        );
    });
    if let Err(error) = spawned {
        eprintln!("neko-rune: could not start the link thread: {error}");
    }
}
