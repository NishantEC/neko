//! The link thread: connects to the daemon, keeps the app current, reconnects with backoff.
//! Everything it sends in is a [`LinkEvent`] or the web UI's address; it receives nothing from
//! the app and writes nothing to the daemon (`link/` only knows read requests).

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use rune::AppHandle;

use crate::app::App;
use crate::link::{LinkEvent, UnixConnector, run};
use crate::web_link::WebTarget;

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
