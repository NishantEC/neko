//! The requests the Mac-app duties make on their own threads (`src/shell/`, docs/knowledge/rune-as-the-app.md), each a one-shot
//! read on a fresh connection like every other `ask`: `Ping` (the daemon watch; the web's Diagnostics sends it too). None changes anything in the daemon.

use std::path::Path;

use neko_protocol::{Request, Response};

use super::ask;

/// What a `Ping` came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ping {
    /// The daemon answered `Pong`.
    Pong,
    /// Something answered, but not with `Pong`.
    Other,
    /// Nothing answered: not running, or not reachable.
    Down,
}

/// `Ping` on `socket` (the daemon's own answer, as `NativeLifecycle.swift` asks).
pub fn ping(socket: &Path) -> Ping {
    match ask(socket, Request::Ping) {
        Ok(Response::Pong) => Ping::Pong,
        Ok(_) => Ping::Other,
        Err(_) => Ping::Down,
    }
}
