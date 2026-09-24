//! User-configured tools; upstream credentials never leave this controller.
use neko_core::{
    Db,
    mcp_host::{
        capability::{Registry, Scope},
        credentials, store as policy, transport,
    },
    workbench as store,
};
use neko_protocol::{
    mcp_host::*,
    workbench::{Command, Snapshot},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub struct Host {
    db: Arc<Mutex<Db>>,
    registry: Registry,
    call_slot: Mutex<()>,
    auth: Mutex<std::collections::HashMap<String, Arc<AtomicBool>>>,
}

/// Removes an in-flight authentication marker on success, failure or unwind.
struct AuthAttempt {
    host: Arc<Host>,
    id: String,
    cancel: Arc<AtomicBool>,
}
impl Drop for AuthAttempt {
    fn drop(&mut self) {
        let mut attempts = self.host.auth.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if attempts.get(&self.id).is_some_and(|current| Arc::ptr_eq(current, &self.cancel)) {
            attempts.remove(&self.id);
        }
    }
}

/// A new credential belongs to a connection only after its snapshot is saved.
struct PendingCredential(Option<String>);
impl Drop for PendingCredential {
    fn drop(&mut self) {
        if let Some(id) = &self.0 { credentials::remove(id); }
    }
}
impl Host {
    pub fn cancel_chat(&self, turn_id: &str) {
        self.registry.cancel_run(&format!("chat:{turn_id}"));
    }

    fn chat_approval(&self, scope: &Scope, connection_id: &str, tool: &McpTool, arguments_json: &str, timeout: std::time::Duration) -> Result<Option<String>, String> {
        use neko_protocol::workbench::{ChatToolCall, ChatToolStatus};
        let Some(turn_id) = scope.run_id.strip_prefix("chat:") else { return Ok(None); };
        if arguments_json.len() > 16 * 1024 { return Err("Chat tool arguments exceed 16 KB".into()); }
        serde_json::from_str::<serde_json::Value>(arguments_json).map_err(|_| "Invalid tool arguments")?;
        let id = store::new_id();
        neko_core::neko_chat::record_call(&*self.db.lock().map_err(|_| "Chat storage unavailable")?, turn_id, ChatToolCall {
            id: id.clone(), workspace_id: scope.workspace_id.clone(), connection_id: connection_id.into(), tool_name: tool.name.clone(), arguments_json: arguments_json.into(),
            status: if tool.read_only { ChatToolStatus::Running } else { ChatToolStatus::AwaitingApproval },
        })?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if scope.cancelled.load(Ordering::Acquire) || std::time::Instant::now() >= deadline || !self.current_call_allowed(scope, connection_id, &tool.name, &tool.schema_hash) {
                let _ = self.finish_chat_call(scope, &id, false);
                return Err("Chat tool approval expired or permission was revoked".into());
            }
            let db = self.db.lock().map_err(|_| "Chat storage unavailable")?;
            match neko_core::neko_chat::call_status(&db, turn_id, &id)? {
                ChatToolStatus::Running if tool.read_only => return Ok(Some(id)),
                ChatToolStatus::Approved => {
                    neko_core::neko_chat::set_call_status(&db, turn_id, &id, ChatToolStatus::Running)?;
                    return Ok(Some(id));
                }
                ChatToolStatus::AwaitingApproval => {},
                _ => return Err("Tool call was denied or cancelled".into()),
            }
            drop(db);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    fn finish_chat_call(&self, scope: &Scope, call_id: &str, success: bool) -> Result<(), String> {
        use neko_protocol::workbench::ChatToolStatus;
        let turn_id = scope.run_id.strip_prefix("chat:").ok_or("Not a chat run")?;
        neko_core::neko_chat::set_call_status(&*self.db.lock().map_err(|_| "Chat storage unavailable")?, turn_id, call_id, if success { ChatToolStatus::Succeeded } else { ChatToolStatus::Failed })
    }
    pub fn new(db: Arc<Mutex<Db>>) -> Self {
        Self {
            db,
            registry: Registry::default(),
            call_slot: Mutex::new(()),
            auth: Mutex::new(Default::default()),
        }
    }
    pub fn command(self: &Arc<Self>, command: McpCommand) -> Result<Snapshot, String> {
        match command {
            McpCommand::Authenticate {
                connection_id,
                client_id,
            } => {
                let connection = self.connection(&connection_id)?;
                let ServerConfig::Http { url } = connection.config else {
                    return Err("Browser authentication is for remote MCP servers".into());
                };
                if client_id
                    .as_ref()
                    .is_some_and(|id| id.len() > 2048 || id.chars().any(char::is_control))
                {
                    return Err("Invalid OAuth client ID".into());
                }
                let cancel = Arc::new(AtomicBool::new(false));
                let mut auth = self
                    .auth
                    .lock()
                    .map_err(|_| "Authentication state unavailable")?;
                if auth.contains_key(&connection_id) {
                    return Err("Authentication is already in progress".into());
                }
                {
                    let db = self
                        .db
                        .lock()
                        .map_err(|_| "Workspace storage unavailable")?;
                    let mut state = store::load(&db)?;
                    state
                        .mcp
                        .grants
                        .retain(|g| g.connection_id != connection_id);
                    state
                        .mcp
                        .connections
                        .iter_mut()
                        .find(|c| c.id == connection_id)
                        .ok_or("Connection missing")?
                        .error = Some("Waiting for browser sign-in (up to 120 seconds)…".into());
                    store::save(&db, &state)?;
                }
                auth.insert(connection_id.clone(), cancel.clone());
                drop(auth);
                let host = self.clone();
                let attempt = AuthAttempt { host: host.clone(), id: connection_id.clone(), cancel: cancel.clone() };
                std::thread::Builder::new().name("neko-oauth".into()).spawn(move || {
                    let _attempt = attempt;
                    let result = neko_core::mcp_host::oauth::authorize(
                        &connection_id,
                        &url,
                        client_id.as_deref(),
                        &cancel,
                        |url| {
                            let _ = std::process::Command::new("/usr/bin/open")
                                .arg(url)
                                .status();
                        },
                    );
                    if let Ok(db) = host.db.lock() {
                        if let Ok(mut state) = store::load(&db) {
                            if let Some(c) = state
                                .mcp
                                .connections
                                .iter_mut()
                                .find(|c| c.id == connection_id)
                            {
                                if c.enabled && !cancel.load(Ordering::Acquire) {
                                    match result {
                                        Ok(()) => {
                                            c.oauth = true;
                                            c.has_credentials = true;
                                            c.error = None;
                                        }
                                        Err(error) => c.error = Some(error),
                                    }
                                    let _ = store::save(&db, &state);
                                }
                            }
                        }
                    }
                }).map_err(|e| format!("Cannot start browser sign-in: {e}"))?;
                store::load(
                    &*self
                        .db
                        .lock()
                        .map_err(|_| "Workspace storage unavailable")?,
                )
            }
            McpCommand::AddConnection {
                workspace_id,
                label,
                config,
                trust_local_process,
                credentials: secret,
            } => {
                policy::validate_config(&config)?;
                if matches!(config, ServerConfig::Stdio { .. }) && !trust_local_process {
                    return Err("Explicitly trust this local process before adding it".into());
                }
                let id = store::new_id();
                let connection = McpConnection {
                    oauth: false,
                    id: id.clone(),
                    workspace_id,
                    label: label.trim().into(),
                    config,
                    enabled: true,
                    trusted: true,
                    has_credentials: secret.is_some(),
                    tools: vec![],
                    discovered_ms: None,
                    error: None,
                };
                {
                    let db = self.db.lock().map_err(|_| "Workspace storage unavailable")?;
                    let mut state = store::load(&db)?;
                    state.mcp.connections.push(connection.clone());
                    policy::validate(&state)?;
                    if let Some(secret) = &secret {
                        db.delete_clipboard_entry(&secret.0)
                            .map_err(|_| "Cannot remove a pasted credential from clipboard history")?;
                        db.delete_clipboard_entry(secret.0.trim())
                            .map_err(|_| "Cannot remove a pasted credential from clipboard history")?;
                    }
                }
                let mut pending = PendingCredential(secret.as_ref().map(|_| id.clone()));
                // Keychain may prompt or block. Never hold the database while
                // contacting it; other chats and workspace edits remain usable.
                if let Some(secret) = &secret { credentials::store(&id, &secret.0)?; }
                {
                    let db = self.db.lock().map_err(|_| "Workspace storage unavailable")?;
                    let mut state = store::load(&db)?;
                    state.mcp.connections.push(connection);
                    // Workspace/grant state may have changed while Keychain ran.
                    policy::validate(&state)?;
                    store::save(&db, &state)?;
                    pending.0 = None;
                    store::load(&db)
                }
            }
            McpCommand::Discover { connection_id } => {
                let _slot = self
                    .call_slot
                    .try_lock()
                    .map_err(|_| "Another MCP operation is running; retry shortly")?;
                let connection = self.connection(&connection_id)?;
                let result = self.credentials(&connection).and_then(|secrets| {
                    transport::discover(&connection.config, &secrets, &AtomicBool::new(false))
                });
                let db = self
                    .db
                    .lock()
                    .map_err(|_| "Workspace storage unavailable")?;
                let mut state = store::load(&db)?;
                let c = state
                    .mcp
                    .connections
                    .iter_mut()
                    .find(|c| c.id == connection_id)
                    .ok_or("MCP connection missing")?;
                if !c.enabled {
                    return Err("Connection was paused during discovery".into());
                }
                match result {
                    Ok(tools) => {
                        c.tools = tools;
                        c.discovered_ms = Some(store::now_ms());
                        c.error = None;
                    }
                    Err(error) => {
                        c.error = Some(error);
                    }
                }
                state.mcp.grants.retain(|g| {
                    g.connection_id != connection_id
                        || c.tools
                            .iter()
                            .any(|t| t.name == g.tool_name && t.schema_hash == g.schema_hash)
                });
                store::save(&db, &state)?;
                store::load(&db)
            }
            command => {
                if let McpCommand::SetEnabled {
                    connection_id,
                    enabled: false,
                } = &command
                {
                    if let Some(cancel) = self
                        .auth
                        .lock()
                        .map_err(|_| "Authentication state unavailable")?
                        .get(connection_id)
                    {
                        cancel.store(true, Ordering::Release);
                    }
                }
                store::apply(
                    &*self
                        .db
                        .lock()
                        .map_err(|_| "Workspace storage unavailable")?,
                    Command::Mcp(command),
                )
            }
        }
    }

    fn connection(&self, id: &str) -> Result<McpConnection, String> {
        store::load(
            &*self
                .db
                .lock()
                .map_err(|_| "Workspace storage unavailable")?,
        )?
        .mcp
        .connections
        .into_iter()
        .find(|c| c.id == id && c.enabled && c.trusted)
        .ok_or_else(|| "MCP connection is missing, paused or untrusted".into())
    }
    fn credentials(&self, c: &McpConnection) -> Result<transport::Credentials, String> {
        self.credentials_cancellable(c, &AtomicBool::new(false))
    }
    fn credentials_cancellable(
        &self,
        c: &McpConnection,
        cancel: &AtomicBool,
    ) -> Result<transport::Credentials, String> {
        if c.oauth {
            let ServerConfig::Http { url } = &c.config else {
                return Err("Invalid OAuth transport".into());
            };
            return Ok(transport::Credentials {
                bearer: Some(neko_core::mcp_host::oauth::bearer_cancellable(
                    &c.id, url, cancel,
                )?),
                environment: Default::default(),
            });
        }
        let secrets = credentials::load(&c.id, c.has_credentials)?;
        Ok(transport::Credentials {
            bearer: secrets.bearer,
            environment: secrets.environment,
        })
    }

    fn current_call_allowed(
        &self,
        scope: &Scope,
        connection: &str,
        tool: &str,
        hash: &str,
    ) -> bool {
        !scope.cancelled.load(Ordering::Acquire)
            && store::now_ms() < scope.expires_ms
            && scope.connection_ids.iter().any(|id| id == connection)
            && self
                .db
                .lock()
                .ok()
                .and_then(|db| store::load(&db).ok())
                .is_some_and(|s| {
                    policy::authorize(&s, &scope.workspace_id, connection, tool)
                        .is_ok_and(|t| t.schema_hash == hash)
                })
    }

    fn prepare_call<T>(
        &self,
        scope: &Scope,
        connection: &str,
        tool: &str,
        hash: &str,
        cancel: &AtomicBool,
        prepare: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let prepared = prepare()?;
        if cancel.load(Ordering::Acquire)
            || !self.current_call_allowed(scope, connection, tool, hash)
        {
            return Err("MCP permission changed or the run expired during preparation".into());
        }
        Ok(prepared)
    }

    pub fn lease(
        self: &Arc<Self>,
        run_id: &str,
        workspace_id: &str,
        connection_ids: Vec<String>,
    ) -> Result<Lease, String> {
        let state = store::load(
            &*self
                .db
                .lock()
                .map_err(|_| "Workspace storage unavailable")?,
        )?;
        if connection_ids.iter().any(|id| {
            !state
                .mcp
                .connections
                .iter()
                .any(|c| &c.id == id && c.workspace_id == workspace_id)
        }) {
            return Err("Run connection belongs to another workspace".into());
        }
        let token = self.registry.issue(Scope {
            run_id: run_id.into(),
            workspace_id: workspace_id.into(),
            connection_ids,
            expires_ms: store::now_ms() + 1_800_000,
            cancelled: Arc::new(AtomicBool::new(false)),
        })?;
        Ok(Lease {
            host: self.clone(),
            token,
        })
    }

    pub fn bridge(&self, request: BridgeRequest) -> Result<String, String> {
        let scope = self.registry.get(&request.token.0, store::now_ms())?;
        let state = store::load(
            &*self
                .db
                .lock()
                .map_err(|_| "Workspace storage unavailable")?,
        )?;
        match request.action {
            BridgeAction::List => {
                let mut tools = Vec::new();
                for id in &scope.connection_ids {
                    if let Some(c) = state.mcp.connections.iter().find(|c| &c.id == id) {
                        for t in &c.tools {
                            if policy::authorize(&state, &scope.workspace_id, id, &t.name).is_ok() {
                                tools.push(serde_json::json!({"connection_id": id, "connection_label": c.label, "tool_name": t.name, "description": t.description, "input_schema": serde_json::from_str::<serde_json::Value>(&t.input_schema).map_err(|_| "Invalid tool schema")?}));
                            }
                        }
                    }
                }
                let json =
                    serde_json::to_string(&tools).map_err(|_| "Cannot encode tool discovery")?;
                if json.len() > 262_144 {
                    return Err("Granted tool catalog is too large; narrow this responsibility's connections or grants".into());
                }
                Ok(json)
            }
            BridgeAction::Call {
                connection_id,
                tool_name,
                arguments_json,
            } => {
                if !scope.connection_ids.contains(&connection_id) {
                    return Err("Connection is not granted to this run".into());
                }
                let _slot = self
                    .call_slot
                    .try_lock()
                    .map_err(|_| "Another MCP operation is running; retry shortly")?;
                let tool =
                    policy::authorize(&state, &scope.workspace_id, &connection_id, &tool_name)?
                        .clone();
                let connection = state
                    .mcp
                    .connections
                    .iter()
                    .find(|c| c.id == connection_id)
                    .ok_or("Connection missing")?
                    .clone();
                let chat_call = self.chat_approval(&scope, &connection_id, &tool, &arguments_json, std::time::Duration::from_secs(120))?;
                let stop = AtomicBool::new(false);
                let cancelled = AtomicBool::new(false);
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
                let result = std::thread::scope(|threads| {
                    threads.spawn(|| {
                        let mut seen = 0_u64;
                        let mut allowed = true;
                        while !stop.load(Ordering::Acquire) {
                            // The full policy check parses the store; repeat
                            // it only after a save. Cancellation and expiry
                            // are checked every tick.
                            let revision = store::revision();
                            if revision != seen {
                                seen = revision;
                                allowed = self.current_call_allowed(
                                    &scope,
                                    &connection_id,
                                    &tool_name,
                                    &tool.schema_hash,
                                );
                            }
                            if std::time::Instant::now() >= deadline
                                || !allowed
                                || scope.cancelled.load(Ordering::Acquire)
                                || store::now_ms() >= scope.expires_ms
                            {
                                cancelled.store(true, Ordering::Release);
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                    });
                    let result = self
                        .prepare_call(
                            &scope,
                            &connection_id,
                            &tool_name,
                            &tool.schema_hash,
                            &cancelled,
                            || self.credentials_cancellable(&connection, &cancelled),
                        )
                        .and_then(|secrets| {
                            if std::time::Instant::now() >= deadline {
                                return Err("MCP operation timed out".into());
                            }
                            transport::call_guarded(
                                &connection.config,
                                &secrets,
                                &tool_name,
                                &tool.schema_hash,
                                &arguments_json,
                                &cancelled,
                                &|| {
                                    std::time::Instant::now() < deadline
                                        && self.current_call_allowed(
                                            &scope,
                                            &connection_id,
                                            &tool_name,
                                            &tool.schema_hash,
                                        )
                                },
                            )
                        });
                    stop.store(true, Ordering::Release);
                    result
                });
                let receipt_id = store::new_id();
                let success = result.as_ref().is_ok_and(|raw| {
                    serde_json::from_str::<serde_json::Value>(raw)
                        .is_ok_and(|v| v.get("isError").and_then(|v| v.as_bool()) != Some(true))
                });
                if let Some(call_id) = &chat_call { let _ = self.finish_chat_call(&scope, call_id, success); }
                {
                    let db = self
                        .db
                        .lock()
                        .map_err(|_| "Workspace storage unavailable")?;
                    let mut state = store::load(&db)?;
                    state.mcp.receipts.push(ToolReceipt {
                        id: receipt_id.clone(),
                        run_id: scope.run_id,
                        workspace_id: scope.workspace_id,
                        connection_id,
                        tool_name,
                        schema_hash: tool.schema_hash,
                        at_ms: store::now_ms(),
                        success,
                    });
                    if state.mcp.receipts.len() > 1000 {
                        state.mcp.receipts.remove(0);
                    }
                    store::save(&db, &state)?;
                }
                let raw = result?;
                let result: serde_json::Value =
                    serde_json::from_str(&raw).map_err(|_| "Invalid MCP result")?;
                Ok(serde_json::json!({"receipt_id": receipt_id, "result": result}).to_string())
            }
        }
    }
}

pub struct Lease {
    host: Arc<Host>,
    token: String,
}

#[cfg(test)]
#[path = "mcp_host/chat_tests.rs"]
mod chat_tests;
impl Lease {
    pub fn token(&self) -> &str {
        &self.token
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.host.registry.revoke(&self.token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn host() -> Arc<Host> {
        let db = Db::open_in_memory().unwrap();
        let mut snapshot = Snapshot::default();
        snapshot
            .workspaces
            .push(neko_protocol::workbench::Workspace {
                id: "w".into(),
                name: "W".into(),
                repository: "/tmp/w".into(),
                instructions: String::new(),
                away_enabled: false,
            });
        store::save(&db, &snapshot).unwrap();
        Arc::new(Host::new(Arc::new(Mutex::new(db))))
    }
    #[test]
    fn authentication_attempt_cleans_up_on_unwind_without_removing_a_replacement() {
        let host = host();
        let cancel = Arc::new(AtomicBool::new(false));
        host.auth.lock().unwrap().insert("connection".into(), cancel.clone());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _attempt = AuthAttempt { host: host.clone(), id: "connection".into(), cancel: cancel.clone() };
            panic!("fixture failure");
        }));
        assert!(result.is_err());
        assert!(!host.auth.lock().unwrap().contains_key("connection"));
        let attempt = AuthAttempt { host: host.clone(), id: "connection".into(), cancel };
        let replacement = Arc::new(AtomicBool::new(false));
        host.auth.lock().unwrap().insert("connection".into(), replacement.clone());
        drop(attempt);
        assert!(Arc::ptr_eq(host.auth.lock().unwrap().get("connection").unwrap(), &replacement));
    }

    #[test]
    fn adding_a_server_does_not_grant_or_launch_it() {
        let h = host();
        let snapshot = h
            .command(McpCommand::AddConnection {
                workspace_id: "w".into(),
                label: "Own server".into(),
                config: ServerConfig::Stdio {
                    command: "/does/not/exist".into(),
                    args: vec![],
                },
                trust_local_process: true,
                credentials: None,
            })
            .unwrap();
        assert_eq!(snapshot.mcp.connections.len(), 1);
        assert!(snapshot.mcp.connections[0].tools.is_empty());
        assert!(snapshot.mcp.grants.is_empty());
    }
    #[test]
    fn local_process_requires_explicit_trust() {
        let h = host();
        assert!(
            h.command(McpCommand::AddConnection {
                workspace_id: "w".into(),
                label: "Untrusted".into(),
                config: ServerConfig::Stdio {
                    command: "/bin/sh".into(),
                    args: vec![]
                },
                trust_local_process: false,
                credentials: None
            })
            .is_err()
        );
        assert!(
            store::load(&h.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
                .unwrap()
                .mcp
                .connections
                .is_empty()
        );
    }
    #[test]
    fn revocation_during_preparation_prevents_dispatch() {
        let h = host();
        {
            let db = h.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut state = store::load(&db).unwrap();
            state.mcp.connections.push(McpConnection {
                oauth: false,
                id: "c".into(),
                workspace_id: "w".into(),
                label: "Test".into(),
                config: ServerConfig::Http {
                    url: "https://example.com/mcp".into(),
                },
                enabled: true,
                trusted: true,
                has_credentials: false,
                tools: vec![McpTool {
                    read_only: false,
                    name: "lookup".into(),
                    description: "Read".into(),
                    input_schema: "{}".into(),
                    schema_hash: "hash".into(),
                }],
                discovered_ms: None,
                error: None,
            });
            state.mcp.grants.push(ToolGrant {
                connection_id: "c".into(),
                workspace_id: "w".into(),
                tool_name: "lookup".into(),
                schema_hash: "hash".into(),
            });
            store::save(&db, &state).unwrap();
        }
        let scope = Scope {
            run_id: "run".into(),
            workspace_id: "w".into(),
            connection_ids: vec!["c".into()],
            expires_ms: i64::MAX,
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        assert!(
            h.prepare_call(
                &scope,
                "c",
                "lookup",
                "hash",
                &AtomicBool::new(false),
                || Ok(())
            )
            .is_ok()
        );
        let result = h.prepare_call(
            &scope,
            "c",
            "lookup",
            "hash",
            &AtomicBool::new(false),
            || {
                scope.cancelled.store(true, Ordering::Release);
                Ok(())
            },
        );
        assert!(
            result.is_err(),
            "No dispatch after preparation loses its capability"
        );
        scope.cancelled.store(false, Ordering::Release);
        let result = h.prepare_call(
            &scope,
            "c",
            "lookup",
            "hash",
            &AtomicBool::new(false),
            || {
                h.command(McpCommand::SetEnabled {
                    connection_id: "c".into(),
                    enabled: false,
                })?;
                Ok(())
            },
        );
        assert!(
            result.is_err(),
            "A grant revoked during credentials must prevent dispatch"
        );
    }
    #[test]
    fn bridge_lists_only_granted_tools_in_its_scoped_workspace() {
        let h = host();
        let snapshot = h
            .command(McpCommand::AddConnection {
                workspace_id: "w".into(),
                label: "Own server".into(),
                config: ServerConfig::Http {
                    url: "https://example.com/mcp".into(),
                },
                trust_local_process: false,
                credentials: None,
            })
            .unwrap();
        let id = snapshot.mcp.connections[0].id.clone();
        let token = h
            .registry
            .issue(Scope {
                run_id: "run".into(),
                workspace_id: "w".into(),
                connection_ids: vec![id.clone()],
                expires_ms: i64::MAX,
                cancelled: Arc::new(AtomicBool::new(false)),
            })
            .unwrap();
        let request = || BridgeRequest {
            token: neko_protocol::workbench::Secret(token.clone()),
            action: BridgeAction::List,
        };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&h.bridge(request()).unwrap()).unwrap(),
            serde_json::json!([])
        );
        {
            let db = h.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut state = store::load(&db).unwrap();
            state.mcp.connections[0].tools.push(McpTool {
                read_only: false,
                name: "lookup".into(),
                description: "Read".into(),
                input_schema: "{}".into(),
                schema_hash: "v1".into(),
            });
            state.mcp.grants.push(ToolGrant {
                connection_id: id.clone(),
                workspace_id: "w".into(),
                tool_name: "lookup".into(),
                schema_hash: "v1".into(),
            });
            store::save(&db, &state).unwrap();
        }
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&h.bridge(request()).unwrap())
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
        h.command(McpCommand::SetEnabled {
            connection_id: id,
            enabled: false,
        })
        .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&h.bridge(request()).unwrap()).unwrap(),
            serde_json::json!([])
        );
        h.registry.revoke(&token);
        assert!(h.bridge(request()).is_err());
    }
}
