//! Short-lived, process-local run authority. Never persisted or logged.
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone)]
pub struct Scope {
    pub run_id: String,
    pub workspace_id: String,
    pub connection_ids: Vec<String>,
    pub expires_ms: i64,
    pub cancelled: Arc<AtomicBool>,
}
#[derive(Default)]
pub struct Registry(Mutex<HashMap<String, Scope>>);
impl Registry {
    pub fn issue(&self, scope: Scope) -> Result<String, String> {
        use std::io::Read;
        let mut bytes = [0_u8; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut bytes))
            .map_err(|_| "Cannot generate a secure run capability")?;
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let mut scopes = self.0.lock().map_err(|_| "Run registry unavailable")?;
        scopes.retain(|_, s| !s.cancelled.load(Ordering::Acquire));
        if scopes.len() >= 32 {
            return Err("Too many active tool runs".into());
        }
        scopes.insert(token.clone(), scope);
        Ok(token)
    }
    pub fn get(&self, token: &str, now: i64) -> Result<Scope, String> {
        self.0
            .lock()
            .map_err(|_| "Run registry unavailable")?
            .get(token)
            .filter(|s| s.expires_ms > now && !s.cancelled.load(Ordering::Acquire))
            .cloned()
            .ok_or_else(|| "Invalid or expired run capability".into())
    }
    pub fn revoke(&self, token: &str) {
        if let Ok(mut scopes) = self.0.lock() {
            if let Some(s) = scopes.remove(token) {
                s.cancelled.store(true, Ordering::Release);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expires_and_revokes_without_restoring_authority() {
        let registry = Registry::default();
        let token = registry
            .issue(Scope {
                run_id: "run".into(),
                workspace_id: "w".into(),
                connection_ids: vec!["c".into()],
                expires_ms: 100,
                cancelled: Arc::new(AtomicBool::new(false)),
            })
            .unwrap();
        assert!(registry.get("unknown", 1).is_err());
        assert_eq!(registry.get(&token, 99).unwrap().workspace_id, "w");
        assert!(registry.get(&token, 100).is_err());
        registry.revoke(&token);
        assert!(registry.get(&token, 99).is_err());
    }
    #[test]
    fn cancellation_revokes_an_existing_run() {
        let registry = Registry::default();
        let cancel = Arc::new(AtomicBool::new(false));
        let token = registry
            .issue(Scope {
                run_id: "run".into(),
                workspace_id: "w".into(),
                connection_ids: vec!["c".into()],
                expires_ms: 100,
                cancelled: cancel.clone(),
            })
            .unwrap();
        cancel.store(true, Ordering::SeqCst);
        assert!(registry.get(&token, 1).is_err());
    }
}
