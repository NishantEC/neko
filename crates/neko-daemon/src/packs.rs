//! `Request::Packs`: the daemon end of `neko_core::packs`. Long steps (fetching a
//! repository, a health check) run without the database lock.
use crate::server::AppState;
use neko_core::packs::{self, Env, Paths};
use neko_protocol::{packs::PackCommand, Response};

pub fn handle(state: &AppState, command: PackCommand) -> Response {
    let (paths, env) = (Paths::system(), Env::system());
    let db = &*state.db;
    let changed = !matches!(command, PackCommand::List | PackCommand::Preview { .. });
    let done = match command {
        PackCommand::Preview { source } => return packs::preview(db, &paths, &env, &source).map_or_else(|message| Response::Error { message }, Response::PackPreview),
        PackCommand::List => Ok(()),
        PackCommand::Install { source, digest, env: values } => packs::install(db, &paths, &env, &source, &digest, &values.into_iter().map(|(k, v)| (k, v.0)).collect()),
        PackCommand::Remove { name } => packs::remove(db, &paths, &name),
        PackCommand::Update { name, digest } => packs::update(db, &paths, &env, &name, digest.as_deref()),
        PackCommand::Approve { name, digest } => packs::approve(db, &paths, &env, &name, &digest),
        PackCommand::CleanUp { name } => packs::clean_up(db, &paths, &name),
        PackCommand::Health { name } => packs::health(db, &paths, &env, &name),
    };
    if let Err(message) = done {
        return Response::Error { message };
    }
    if changed {
        // Skills a pack added or removed show up in Setup right away.
        let db = state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Ok(snapshot) = neko_core::workbench::load(&db) {
            let _ = neko_core::skills::refresh(&db, &snapshot.workspaces);
        }
    }
    packs::list(db, &paths, &env).map_or_else(|message| Response::Error { message }, Response::Packs)
}
