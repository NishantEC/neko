//! Search (docs/knowledge/search.md): answered on a thread of its own, from a connection of its own, so a search never waits for a run
//! that is writing and the connection that asked can go on (a stop, a reply to a request) while it works.

use std::sync::Arc;
use std::time::Instant;

use neko_core::Store;
use neko_protocol::search::{DEFAULT_LIMIT, MAX_LIMIT};
use neko_protocol::{ErrorCode, Reply, ServerMessage};

use crate::server::{Shared, lock};

/// Finds `query` and says so to the connection that asked.
pub fn spawn(shared: &Arc<Shared>, conn: u64, reply_id: u64, query: String, limit: Option<u32>) {
    let s = shared.clone();
    let spawned = std::thread::Builder::new().name("neko-search".into()).spawn(move || {
        let reply = answer(&s, query, limit);
        s.send_to(conn, &ServerMessage::Reply { id: reply_id, reply });
    });
    if spawned.is_err() {
        shared.send_to(conn, &ServerMessage::Reply { id: reply_id, reply: Reply::error(ErrorCode::Internal, "Neko could not start a thread for this.") });
    }
}

fn answer(shared: &Shared, query: String, limit: Option<u32>) -> Reply {
    let started = Instant::now();
    let (path, now) = {
        let core = lock(&shared.core);
        (core.store.path(), core.now())
    };
    let Some(path) = path else { return Reply::error(ErrorCode::Internal, "The store has no file to search.") };
    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as usize;
    match Store::open_reader(&path).and_then(|reader| reader.search(&query, limit, now)) {
        Ok((hits, problem)) => Reply::Found { query, hits, problem, took_ms: started.elapsed().as_millis().min(u32::MAX as u128) as u32 },
        Err(e) => Reply::error(ErrorCode::Internal, e.to_string()),
    }
}
