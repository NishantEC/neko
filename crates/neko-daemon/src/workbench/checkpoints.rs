//! Refreshes a long Home conversation's checkpoint in the background
//! (`neko_core::chat_checkpoint`). A turn never waits for this: it uses the
//! checkpoint already stored, and the next turn sees the refreshed one.
use super::*;
use neko_core::chat_checkpoint::{self as checkpoint, Refresh};
use std::collections::BTreeSet;

/// Conversations whose checkpoint is being refreshed: one at a time each.
static REFRESHING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Releases a conversation's refresh slot however the refresh ends.
struct Slot(String);
impl Drop for Slot {
    fn drop(&mut self) {
        REFRESHING.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&self.0);
    }
}

/// One tool-free extraction on its own thread, in an empty scratch folder,
/// the way memory learning and naming run theirs. Skipped while the same
/// conversation's previous refresh is still running.
pub(super) fn refresh_in_background(db: Arc<Mutex<Db>>, profile: String, scope: Option<String>, refresh: Refresh, runtime: AgentRuntime) {
    let key = checkpoint::key(&profile, scope.as_deref());
    if !REFRESHING.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(key.clone()) {
        return;
    }
    let slot = Slot(key);
    std::thread::spawn(move || {
        let _slot = slot;
        if let Err(error) = refresh_with(&db, &profile, scope.as_deref(), &refresh, |prompt| extract(prompt, &runtime)) {
            eprintln!("neko chat: the conversation checkpoint was not refreshed: {error}");
        }
    });
}

/// Asks for the sections, then merges them into the checkpoint stored now.
fn refresh_with(db: &Mutex<Db>, profile: &str, scope: Option<&str>, refresh: &Refresh, ask: impl FnOnce(&str) -> Result<String, String>) -> Result<(), String> {
    let previous = {
        let guard = db.lock().map_err(|_| "Chat storage unavailable")?;
        checkpoint::load(&guard, profile, scope)
    };
    let fresh = checkpoint::parse(&ask(&checkpoint::prompt(previous.as_ref(), refresh))?)?;
    let db = db.lock().map_err(|_| "Chat storage unavailable")?;
    let current = checkpoint::load(&db, profile, scope).unwrap_or_default();
    checkpoint::save(&db, profile, scope, &checkpoint::merge(&current, &fresh, &refresh.through_id, store::now_ms()))
}

fn extract(prompt: &str, runtime: &AgentRuntime) -> Result<String, String> {
    let scratch = std::env::temp_dir().join(format!("neko-checkpoint-{}", store::new_id()));
    let result = (|| {
        std::fs::create_dir(&scratch).map_err(|e| format!("Cannot create checkpoint scratch: {e}"))?;
        native_runner::extract(
            &native_runner::RunSpec {
                directory: scratch.clone(),
                prompt: prompt.to_owned(),
                writable: false,
                timeout: Duration::from_secs(90),
                runtime: runtime.clone(),
            },
            &AtomicBool::new(false),
        )
    })();
    let _ = std::fs::remove_dir(&scratch);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refresh_merges_into_the_stored_checkpoint_and_a_bad_answer_changes_nothing() {
        let db = Mutex::new(Db::open_in_memory().unwrap());
        let stored = checkpoint::Checkpoint { corrections: vec!["Not the staging database".into()], through_id: "m1".into(), ..Default::default() };
        checkpoint::save(&db.lock().unwrap(), "p", Some("w"), &stored).unwrap();
        let refresh = Refresh { messages: vec![("User".into(), "Ship the export as CSV".into())], through_id: "m5".into() };
        let mut asked = String::new();
        refresh_with(&db, "p", Some("w"), &refresh, |prompt| {
            asked = prompt.to_owned();
            Ok("{\"goal\":\"Ship the export\",\"decisions\":[\"CSV\"]}".into())
        })
        .unwrap();
        assert!(asked.contains("Not the staging database") && asked.contains("User: Ship the export as CSV"));
        let after = checkpoint::load(&db.lock().unwrap(), "p", Some("w")).unwrap();
        assert_eq!(after.goal, "Ship the export");
        assert_eq!(after.corrections, ["Not the staging database"], "an earlier correction is kept");
        assert_eq!(after.through_id, "m5");
        assert!(refresh_with(&db, "p", Some("w"), &refresh, |_| Ok("no json here".into())).is_err());
        assert!(refresh_with(&db, "p", Some("w"), &refresh, |_| Err("model unavailable".into())).is_err());
        assert_eq!(checkpoint::load(&db.lock().unwrap(), "p", Some("w")).unwrap(), after);
    }
}
