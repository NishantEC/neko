//! The requests the Mac-app duties make on their own threads (`src/shell/`, docs/knowledge/rune-as-the-app.md), each a one-shot
//! read on a fresh connection like every other `ask`: `Ping` (the daemon watch; the web's Diagnostics sends it too), `RoutineNotifications` (the notifier; `RoutineNotifier.swift` sends it) and
//! `CancelAllWork` (the uninstaller's first step, after the person confirmed). Only the last changes anything in the daemon.

use std::path::Path;

use neko_protocol::routines::RoutineNotice;
use neko_protocol::workbench::Command;
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
/// The needs-you items after `after`, and where the daemon's list now stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notices {
    pub items: Vec<RoutineNotice>,
    pub latest: u64,
}

/// `RoutineNotifications { after }`. None when the daemon did not answer with them (`RoutineNotifier.poll` returns at once then).
pub fn routine_notifications(socket: &Path, after: u64) -> Option<Notices> {
    match ask(socket, Request::RoutineNotifications { after }) {
        Ok(Response::RoutineNotifications { items, latest }) => Some(Notices { items, latest }),
        _ => None,
    }
}

/// `CancelAllWork`: the uninstaller stops all work first (`Uninstaller.confirmAndRun`). Only ever called after the confirmation said yes.
pub fn cancel_all_work(socket: &Path) -> bool {
    ask(socket, Request::Workbench(Command::CancelAllWork)).is_ok()
}
