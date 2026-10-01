//! User-configured tools; upstream credentials never leave this controller.
use neko_core::{
    Db,
    mcp_host::{
        capability::{Registry, Scope},
        credentials, store as policy, transport,
    },
    setup_import, workbench as store,
};
use neko_protocol::{
    mcp_host::*,
    workbench::{Command, Snapshot},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::{collections::BTreeMap, path::PathBuf};

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
        let mut attempts = self
            .host
            .auth
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if attempts
            .get(&self.id)
            .is_some_and(|current| Arc::ptr_eq(current, &self.cancel))
        {
            attempts.remove(&self.id);
        }
    }
}

/// A new credential belongs to a connection only after its snapshot is saved.
struct PendingCredential(Option<String>);
impl Drop for PendingCredential {
    fn drop(&mut self) {
        if let Some(id) = &self.0 {
            credentials::remove(id);
        }
    }
}
impl Host {
    fn source_candidate(
        &self,
        connection: &McpConnection,
    ) -> Result<Option<setup_import::Candidate>, String> {
        let Some(link) = &connection.source_link else {
            return Ok(None);
        };
        let result = self.resolve_source_candidate(connection, link);
        if let Err(error) = &result {
            self.invalidate_source_link(connection, error);
        }
        result.map(Some)
    }

    fn invalidate_source_link(&self, connection: &McpConnection, error: &str) {
        let Ok(db) = self.db.lock() else { return };
        let Ok(mut state) = store::load(&db) else {
            return;
        };
        let Some(current) = state
            .mcp
            .connections
            .iter_mut()
            .find(|c| c.id == connection.id && c.source_link == connection.source_link)
        else {
            return;
        };
        current.tools.clear();
        current.discovered_ms = None;
        current.error = Some(error.chars().take(512).collect());
        state
            .mcp
            .grants
            .retain(|grant| grant.connection_id != connection.id);
        let _ = store::save(&db, &state);
    }

    fn resolve_source_candidate(
        &self,
        connection: &McpConnection,
        link: &SourceLink,
    ) -> Result<setup_import::Candidate, String> {
        let db = self
            .db
            .lock()
            .map_err(|_| "Workspace storage unavailable")?;
        let state = store::load(&db)?;
        let workspace = state
            .workspaces
            .iter()
            .find(|w| w.id == connection.workspace_id)
            .ok_or("Linked workspace is unavailable")?;
        let repositories = state.folders_for(workspace).into_iter().map(PathBuf::from).collect::<Vec<_>>();
        drop(db);
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("Home directory unavailable")?;
        let paths = std::env::var_os("PATH")
            .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
            .unwrap_or_default();
        let environment = std::env::vars_os()
            .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
            .collect::<BTreeMap<_, _>>();
        let candidate = setup_import::resolve_linked_source_in_folders(
            &home,
            &repositories,
            &paths,
            &environment,
            &link.candidate_id,
        )?;
        let current_path = std::fs::canonicalize(&candidate.preview.source)
            .map_err(|_| "Source MCP file is unavailable")?;
        if current_path.to_string_lossy() != link.source_path
            || candidate.config.as_ref().is_none_or(|config| {
                setup_import::config_identity(config) != link.config_hash
                    || setup_import::executable_identity(config).ok()
                        != Some(link.executable_identity.clone())
            })
        {
            return Err("Source MCP definition changed; review it again before using tools".into());
        }
        Ok(candidate)
    }

    fn resolved_transport(
        &self,
        connection: &McpConnection,
        cancel: &AtomicBool,
    ) -> Result<(ServerConfig, transport::Credentials), String> {
        if let Some(candidate) = self.source_candidate(connection)? {
            let config = candidate
                .config
                .ok_or("Source MCP configuration unavailable")?;
            if connection.oauth {
                return Ok((config, self.credentials_cancellable(connection, cancel)?));
            }
            let secret = candidate
                .credentials
                .map(|value| credentials::parse(&value.0))
                .transpose()?
                .unwrap_or_default();
            return Ok((
                config,
                transport::Credentials {
                    bearer: secret.bearer,
                    environment: secret.environment,
                },
            ));
        }
        Ok((
            connection.config.clone(),
            self.credentials_cancellable(connection, cancel)?,
        ))
    }

    pub fn cancel_chat(&self, turn_id: &str) {
        self.registry.cancel_run(&format!("chat:{turn_id}"));
    }

    fn chat_approval(
        &self,
        scope: &Scope,
        connection_id: &str,
        tool: &McpTool,
        arguments_json: &str,
        timeout: std::time::Duration,
    ) -> Result<Option<String>, String> {
        use neko_protocol::workbench::{ChatToolCall, ChatToolStatus};
        let Some(turn_id) = scope.run_id.strip_prefix("chat:") else {
            return Ok(None);
        };
        if arguments_json.len() > 16 * 1024 {
            return Err("Chat tool arguments exceed 16 KB".into());
        }
        serde_json::from_str::<serde_json::Value>(arguments_json)
            .map_err(|_| "Invalid tool arguments")?;
        let id = store::new_id();
        neko_core::neko_chat::record_call(
            &*self.db.lock().map_err(|_| "Chat storage unavailable")?,
            turn_id,
            ChatToolCall {
                id: id.clone(),
                workspace_id: scope.workspace_id.clone(),
                connection_id: connection_id.into(),
                tool_name: tool.name.clone(),
                arguments_json: arguments_json.into(),
                status: if tool.read_only {
                    ChatToolStatus::Running
                } else {
                    ChatToolStatus::AwaitingApproval
                },
            },
        )?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if scope.cancelled.load(Ordering::Acquire)
                || std::time::Instant::now() >= deadline
                || !self.current_call_allowed(scope, connection_id, &tool.name, &tool.schema_hash)
            {
                let _ = self.finish_chat_call(scope, &id, false);
                return Err("Chat tool approval expired or permission was revoked".into());
            }
            let db = self.db.lock().map_err(|_| "Chat storage unavailable")?;
            match neko_core::neko_chat::call_status(&db, turn_id, &id)? {
                ChatToolStatus::Running if tool.read_only => return Ok(Some(id)),
                ChatToolStatus::Approved => {
                    neko_core::neko_chat::set_call_status(
                        &db,
                        turn_id,
                        &id,
                        ChatToolStatus::Running,
                    )?;
                    return Ok(Some(id));
                }
                ChatToolStatus::AwaitingApproval => {}
                _ => return Err("Tool call was denied or cancelled".into()),
            }
            drop(db);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    fn finish_chat_call(&self, scope: &Scope, call_id: &str, success: bool) -> Result<(), String> {
        use neko_protocol::workbench::ChatToolStatus;
        let turn_id = scope.run_id.strip_prefix("chat:").ok_or("Not a chat run")?;
        neko_core::neko_chat::set_call_status(
            &*self.db.lock().map_err(|_| "Chat storage unavailable")?,
            turn_id,
            call_id,
            if success {
                ChatToolStatus::Succeeded
            } else {
                ChatToolStatus::Failed
            },
        )
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
            McpCommand::LinkSource {
                workspace_id,
                candidate_id,
                trust_local_process,
            } => {
                let db = self
                    .db
                    .lock()
                    .map_err(|_| "Workspace storage unavailable")?;
                let mut state = store::load(&db)?;
                let workspace = state
                    .workspaces
                    .iter()
                    .find(|w| w.id == workspace_id)
                    .ok_or("Choose a workspace before linking tools")?;
                let repositories = state.folders_for(workspace).into_iter().map(PathBuf::from).collect::<Vec<_>>();
                let home = std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .ok_or("Home directory unavailable")?;
                let paths = std::env::var_os("PATH")
                    .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
                    .unwrap_or_default();
                let environment = std::env::vars_os()
                    .filter_map(|(key, value)| {
                        Some((key.into_string().ok()?, value.into_string().ok()?))
                    })
                    .collect::<BTreeMap<_, _>>();
                let candidate = setup_import::resolve_linked_source_in_folders(
                    &home,
                    &repositories,
                    &paths,
                    &environment,
                    &candidate_id,
                )?;
                let config = candidate
                    .config
                    .ok_or("Source MCP configuration unavailable")?;
                let local = matches!(config, ServerConfig::Stdio { .. });
                if local && !trust_local_process {
                    return Err("Approve this local MCP process before linking it".into());
                }
                let source_path = std::fs::canonicalize(&candidate.preview.source)
                    .map_err(|_| "Source MCP file is unavailable")?
                    .to_string_lossy()
                    .into_owned();
                let source_link = SourceLink {
                    source_path,
                    candidate_id,
                    config_hash: setup_import::config_identity(&config),
                    executable_identity: setup_import::executable_identity(&config)?,
                };
                if let Some(existing) = state.mcp.connections.iter_mut().find(|c| {
                    c.workspace_id == workspace_id
                        && c.source_link
                            .as_ref()
                            .is_some_and(|link| link.candidate_id == source_link.candidate_id)
                }) {
                    if existing.source_link.as_ref() == Some(&source_link)
                        && existing.error.is_none()
                    {
                        return Ok(state);
                    }
                    existing.config = config.clone();
                    existing.source_link = Some(source_link);
                    existing.oauth = false;
                    existing.enabled = true;
                    existing.trusted = !local || trust_local_process;
                    existing.has_credentials = candidate.preview.has_credentials;
                    existing.tools.clear();
                    existing.discovered_ms = None;
                    existing.error = None;
                    let existing_id = existing.id.clone();
                    state
                        .mcp
                        .grants
                        .retain(|grant| grant.connection_id != existing_id);
                    policy::validate(&state)?;
                    store::save(&db, &state)?;
                    return store::load(&db);
                }
                if let Some(existing) = state.mcp.connections.iter_mut().find(|c| {
                    c.workspace_id == workspace_id
                        && c.label == candidate.preview.name
                        && c.config == config
                        && c.source_link.is_none()
                }) {
                    existing.source_link = Some(source_link);
                    existing.oauth = false;
                    existing.enabled = true;
                    existing.trusted = !local || trust_local_process;
                    existing.has_credentials = candidate.preview.has_credentials;
                    existing.tools.clear();
                    existing.discovered_ms = None;
                    existing.error = None;
                    let existing_id = existing.id.clone();
                    state
                        .mcp
                        .grants
                        .retain(|grant| grant.connection_id != existing_id);
                    policy::validate(&state)?;
                    store::save(&db, &state)?;
                    return store::load(&db);
                }
                let connection = McpConnection {
                    oauth: false,
                    id: store::new_id(),
                    workspace_id,
                    label: candidate.preview.name,
                    config: config.clone(),
                    enabled: true,
                    trusted: !local || trust_local_process,
                    has_credentials: candidate.preview.has_credentials,
                    tools: Vec::new(),
                    discovered_ms: None,
                    error: None,
                    source_link: Some(source_link),
                };
                state.mcp.connections.push(connection);
                policy::validate(&state)?;
                store::save(&db, &state)?;
                store::load(&db)
            }
            McpCommand::Authenticate {
                connection_id,
                client_id,
            } => {
                let connection = self.connection(&connection_id)?;
                // Re-authentication must not depend on the old token still
                // being readable or refreshable. Resolve only the source URL.
                let config = match self.source_candidate(&connection)? {
                    Some(candidate) => candidate
                        .config
                        .ok_or("Source MCP configuration unavailable")?,
                    None => connection.config.clone(),
                };
                let ServerConfig::Http { url } = config else {
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
                if let Some(previous) = auth.get(&connection_id) {
                    previous.store(true, Ordering::Release);
                }
                let epoch = neko_core::mcp_host::oauth::begin_authorization(&connection_id)?;
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
                let attempt = AuthAttempt {
                    host: host.clone(),
                    id: connection_id.clone(),
                    cancel: cancel.clone(),
                };
                let worker_id = connection_id.clone();
                let worker_cancel = cancel.clone();
                std::thread::Builder::new()
                    .name("neko-oauth".into())
                    .spawn(move || {
                        let _attempt = attempt;
                        let result = neko_core::mcp_host::oauth::authorize(
                            &worker_id,
                            &url,
                            client_id.as_deref(),
                            epoch,
                            &worker_cancel,
                            |url| {
                                let _ = std::process::Command::new("/usr/bin/open")
                                    .arg(url)
                                    .status();
                            },
                        );
                        if let Ok(active) = host.auth.lock() {
                            if active.get(&worker_id).is_some_and(|current| Arc::ptr_eq(current, &worker_cancel))
                                && !worker_cancel.load(Ordering::Acquire)
                            {
                                if let Ok(db) = host.db.lock() {
                                    if let Ok(mut state) = store::load(&db) {
                                        if let Some(c) = state
                                            .mcp
                                            .connections
                                            .iter_mut()
                                            .find(|c| c.id == worker_id)
                                        {
                                            if c.enabled {
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
                            }
                        }
                    })
                    .map_err(|e| {
                        if let Ok(mut active) = self.auth.lock() {
                            if active.get(&connection_id).is_some_and(|current| Arc::ptr_eq(current, &cancel)) {
                                active.remove(&connection_id);
                            }
                        }
                        format!("Cannot start browser sign-in: {e}")
                    })?;
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
            } => self
                .add_connection(
                    workspace_id,
                    label,
                    config,
                    trust_local_process,
                    secret,
                    true,
                    false,
                )
                .map(|(_, snapshot)| snapshot),
            McpCommand::Discover { connection_id } => {
                let _slot = self
                    .call_slot
                    .try_lock()
                    .map_err(|_| "Another MCP operation is running; retry shortly")?;
                let connection = self.connection(&connection_id)?;
                let result = self
                    .resolved_transport(&connection, &AtomicBool::new(false))
                    .and_then(|(config, secrets)| {
                        transport::discover(&config, &secrets, &AtomicBool::new(false))
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
                if state
                    .mcp
                    .connections
                    .iter()
                    .any(|c| c.id == connection_id && c.error.is_none())
                {
                    policy::allow_discovered_tools(&mut state, &connection_id);
                }
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

    /// Import chooses the initial enabled state in the same durable write as
    /// creation. The returned ID is unambiguous even during concurrent adds.
    pub(crate) fn add_connection(
        &self,
        workspace_id: String,
        label: String,
        config: ServerConfig,
        trust_local_process: bool,
        secret: Option<neko_protocol::workbench::Secret>,
        enabled: bool,
        allow_untrusted_local_definition: bool,
    ) -> Result<(String, Snapshot), String> {
        policy::validate_config(&config)?;
        let is_local_process = matches!(config, ServerConfig::Stdio { .. });
        if is_local_process && !trust_local_process && !allow_untrusted_local_definition {
            return Err("Explicitly trust this local process before adding it".into());
        }
        let id = store::new_id();
        let connection = McpConnection {
            oauth: false,
            id: id.clone(),
            workspace_id,
            label: label.trim().into(),
            config,
            enabled,
            trusted: !is_local_process || trust_local_process,
            has_credentials: secret.is_some(),
            tools: vec![],
            discovered_ms: None,
            error: None,
            source_link: None,
        };
        {
            let db = self
                .db
                .lock()
                .map_err(|_| "Workspace storage unavailable")?;
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
        // Keychain can prompt; never hold the database while contacting it.
        if let Some(secret) = &secret {
            credentials::store(&id, &secret.0)?;
        }
        let db = self
            .db
            .lock()
            .map_err(|_| "Workspace storage unavailable")?;
        let mut state = store::load(&db)?;
        state.mcp.connections.push(connection);
        policy::validate(&state)?;
        store::save(&db, &state)?;
        pending.0 = None;
        Ok((id, store::load(&db)?))
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
                    s.agent_profiles.revision == scope.profile_revision
                        && policy::authorize(&s, &scope.workspace_id, connection, tool)
                            .is_ok_and(|t| t.schema_hash == hash)
                        && s.mcp
                            .connections
                            .iter()
                            .find(|c| c.id == connection)
                            .is_some_and(|c| self.source_candidate(c).is_ok())
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
        expected_profile_revision: u64,
    ) -> Result<Lease, String> {
        // Validate the actor's captured revision and issue its capability in
        // one DB critical section. Never upgrade a stale worker to whatever
        // profile authority happens to be current when it reaches the host.
        let db = self
            .db
            .lock()
            .map_err(|_| "Workspace storage unavailable")?;
        let state = store::load(&db)?;
        if state.agent_profiles.revision != expected_profile_revision {
            return Err("Agent profile authority changed before lease issuance".into());
        }
        if let Some(turn_id) = run_id.strip_prefix("chat:") {
            let turn = state
                .conversation
                .iter()
                .find(|m| m.id == turn_id && m.pending)
                .ok_or("Chat turn is no longer active")?;
            if turn.agent_profile_revision != state.agent_profiles.revision
                || (turn.workspace_id.as_deref() != Some(workspace_id)
                    && !(turn.workspace_id.is_none()
                        && state.workspaces.iter().filter(|w|
                            state.agent_profiles.owner(&w.id) == turn.agent_profile_id
                        ).count() == 1
                        && state.workspaces.iter().any(|w| w.id == workspace_id
                            && state.agent_profiles.owner(&w.id) == turn.agent_profile_id)))
                || turn.agent_profile_id != state.agent_profiles.owner(workspace_id)
            {
                return Err("Chat agent profile authority changed before launch".into());
            }
        }
        if connection_ids.iter().any(|id| {
            !state
                .mcp
                .connections
                .iter()
                .any(|c| &c.id == id && c.available_in(workspace_id))
        }) {
            return Err("Run connection belongs to another workspace".into());
        }
        let token = self.registry.issue(Scope {
            profile_revision: expected_profile_revision,
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
        if state.agent_profiles.revision != scope.profile_revision {
            return Err("Agent profile authority changed; start a new run".into());
        }
        match request.action {
            BridgeAction::List => {
                let mut tools = Vec::new();
                for id in &scope.connection_ids {
                    if let Some(c) = state.mcp.connections.iter().find(|c| &c.id == id) {
                        if self.source_candidate(c).is_err() {
                            continue;
                        }
                        for t in &c.tools {
                            if scope.run_id.starts_with("watch:") && !t.read_only {
                                continue;
                            }
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
                let tool =
                    policy::authorize(&state, &scope.workspace_id, &connection_id, &tool_name)?
                        .clone();
                if scope.run_id.starts_with("watch:") && !tool.read_only {
                    return Err("This background watch can call only tools declared read-only; ask Neko in chat for a write action".into());
                }
                let connection = state
                    .mcp
                    .connections
                    .iter()
                    .find(|c| c.id == connection_id)
                    .ok_or("Connection missing")?
                    .clone();
                let chat_call = self.chat_approval(
                    &scope,
                    &connection_id,
                    &tool,
                    &arguments_json,
                    std::time::Duration::from_secs(120),
                )?;
                let _slot = self.call_slot.try_lock().map_err(|_| {
                    if let Some(call_id) = &chat_call {
                        let _ = self.finish_chat_call(&scope, call_id, false);
                    }
                    "Another MCP operation is running; retry shortly".to_owned()
                })?;
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
                            || self.resolved_transport(&connection, &cancelled),
                        )
                        .and_then(|(config, secrets)| {
                            if std::time::Instant::now() >= deadline {
                                return Err("MCP operation timed out".into());
                            }
                            transport::call_guarded(
                                &config,
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
                if let Some(call_id) = &chat_call {
                    let _ = self.finish_chat_call(&scope, call_id, success);
                }
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

    #[test]
    fn linking_source_keeps_credentials_out_of_neko_and_grants_empty() {
        let home = tempfile::tempdir().unwrap();
        let repo = home.path().join("project");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join(".mcp.json"), r#"{"mcpServers":{"tool":{"url":"https://example.com/mcp","headers":{"Authorization":"Bearer private-sentinel"}}}}"#).unwrap();
        let h = host();
        {
            let db = h.db.lock().unwrap();
            let mut state = store::load(&db).unwrap();
            state.workspaces[0].repository = repo.to_string_lossy().into_owned();
            store::save(&db, &state).unwrap();
        }
        let home_directory = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
        let found = neko_core::setup_import::discover(
            &home_directory,
            &[repo.clone()],
            &[],
            &Default::default(),
        );
        let id = found
            .candidates
            .iter()
            .find(|c| c.preview.name == "tool")
            .unwrap()
            .preview
            .id
            .clone();
        let state = h
            .command(McpCommand::LinkSource {
                workspace_id: "w".into(),
                candidate_id: id,
                trust_local_process: false,
            })
            .unwrap();
        let c = &state.mcp.connections[0];
        assert!(c.source_link.is_some());
        assert!(state.mcp.grants.is_empty());
        assert!(!format!("{state:?}").contains("private-sentinel"));
    }

    #[test]
    fn changed_or_missing_linked_source_blocks_granted_tools() {
        let directory = tempfile::tempdir().unwrap();
        let repository = directory.path().join("project");
        std::fs::create_dir_all(&repository).unwrap();
        let source = repository.join(".mcp.json");
        std::fs::write(
            &source,
            r#"{"mcpServers":{"linked":{"url":"https://one.example/mcp"}}}"#,
        )
        .unwrap();
        let h = host();
        {
            let db = h.db.lock().unwrap();
            let mut state = store::load(&db).unwrap();
            state.workspaces[0].repository = repository.to_string_lossy().into_owned();
            store::save(&db, &state).unwrap();
        }
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let discovery =
            setup_import::discover(&home, &[repository.clone()], &[], &Default::default());
        let candidate_id = discovery
            .candidates
            .iter()
            .find(|c| c.preview.name == "linked")
            .unwrap()
            .preview
            .id
            .clone();
        let linked = h
            .command(McpCommand::LinkSource {
                workspace_id: "w".into(),
                candidate_id,
                trust_local_process: false,
            })
            .unwrap();
        let id = linked.mcp.connections[0].id.clone();
        {
            let db = h.db.lock().unwrap();
            let mut state = store::load(&db).unwrap();
            state.mcp.connections[0].tools.push(McpTool {
                read_only: true,
                name: "lookup".into(),
                description: "Read".into(),
                input_schema: "{}".into(),
                schema_hash: "schema".into(),
            });
            state.mcp.grants.push(ToolGrant {
                workspace_id: "w".into(),
                connection_id: id.clone(),
                tool_name: "lookup".into(),
                schema_hash: "schema".into(),
            });
            store::save(&db, &state).unwrap();
        }
        let revision = store::load(&h.db.lock().unwrap())
            .unwrap()
            .agent_profiles
            .revision;
        let lease = h
            .lease("test-run", "w", vec![id.clone()], revision)
            .unwrap();
        let list = || {
            h.bridge(BridgeRequest {
                token: neko_protocol::workbench::Secret(lease.token().into()),
                action: BridgeAction::List,
            })
            .unwrap()
        };
        assert!(list().contains("lookup"));
        std::fs::write(
            &source,
            r#"{"mcpServers":{"linked":{"url":"https://changed.example/mcp"}}}"#,
        )
        .unwrap();
        assert_eq!(list(), "[]");
        assert!(
            store::load(&h.db.lock().unwrap())
                .unwrap()
                .mcp
                .grants
                .is_empty()
        );
        assert!(
            h.bridge(BridgeRequest {
                token: neko_protocol::workbench::Secret(lease.token().into()),
                action: BridgeAction::Call {
                    connection_id: id.clone(),
                    tool_name: "lookup".into(),
                    arguments_json: "{}".into()
                }
            })
            .is_err()
        );
        std::fs::remove_file(&source).unwrap();
        assert_eq!(list(), "[]");
        std::fs::write(
            &source,
            r#"{"mcpServers":{"linked":{"url":"https://changed.example/mcp"}}}"#,
        )
        .unwrap();
        let relinked = h
            .command(McpCommand::LinkSource {
                workspace_id: "w".into(),
                candidate_id: linked.mcp.connections[0]
                    .source_link
                    .as_ref()
                    .unwrap()
                    .candidate_id
                    .clone(),
                trust_local_process: false,
            })
            .unwrap();
        assert_eq!(relinked.mcp.connections.len(), 1);
        assert!(
            matches!(&relinked.mcp.connections[0].config, ServerConfig::Http { url } if url == "https://changed.example/mcp")
        );
        assert!(relinked.mcp.connections[0].error.is_none());
        assert!(relinked.mcp.grants.is_empty());
    }

    #[test]
    fn linked_local_executable_change_revokes_the_link() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let repository = directory.path().join("project");
        std::fs::create_dir_all(&repository).unwrap();
        let executable = repository.join("server.sh");
        std::fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(
            repository.join(".mcp.json"),
            format!(
                r#"{{"mcpServers":{{"local":{{"command":"{}"}}}}}}"#,
                executable.display()
            ),
        )
        .unwrap();
        let h = host();
        {
            let db = h.db.lock().unwrap();
            let mut state = store::load(&db).unwrap();
            state.workspaces[0].repository = repository.to_string_lossy().into_owned();
            store::save(&db, &state).unwrap();
        }
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let discovery =
            setup_import::discover(&home, &[repository.clone()], &[], &Default::default());
        let id = discovery
            .candidates
            .iter()
            .find(|c| c.preview.name == "local")
            .unwrap()
            .preview
            .id
            .clone();
        let state = h
            .command(McpCommand::LinkSource {
                workspace_id: "w".into(),
                candidate_id: id,
                trust_local_process: true,
            })
            .unwrap();
        let connection = state.mcp.connections[0].clone();
        assert!(h.source_candidate(&connection).is_ok());
        std::fs::write(&executable, "#!/bin/sh\necho changed\n").unwrap();
        assert!(h.source_candidate(&connection).is_err());
    }

    #[test]
    fn linking_matching_neko_copy_reuses_it_without_duplicating_or_carrying_grants() {
        let directory = tempfile::tempdir().unwrap();
        let repository = directory.path().join("project");
        std::fs::create_dir_all(&repository).unwrap();
        std::fs::write(
            repository.join(".mcp.json"),
            r#"{"mcpServers":{"copy":{"url":"https://example.com/mcp"}}}"#,
        )
        .unwrap();
        let h = host();
        {
            let db = h.db.lock().unwrap();
            let mut state = store::load(&db).unwrap();
            state.workspaces[0].repository = repository.to_string_lossy().into_owned();
            store::save(&db, &state).unwrap();
        }
        let (existing_id, _) = h
            .add_connection(
                "w".into(),
                "copy".into(),
                ServerConfig::Http {
                    url: "https://example.com/mcp".into(),
                },
                false,
                None,
                true,
                false,
            )
            .unwrap();
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let discovery = setup_import::discover(&home, &[repository], &[], &Default::default());
        let candidate_id = discovery
            .candidates
            .iter()
            .find(|c| c.preview.name == "copy")
            .unwrap()
            .preview
            .id
            .clone();
        let state = h
            .command(McpCommand::LinkSource {
                workspace_id: "w".into(),
                candidate_id,
                trust_local_process: false,
            })
            .unwrap();
        assert_eq!(state.mcp.connections.len(), 1);
        assert_eq!(state.mcp.connections[0].id, existing_id);
        assert!(state.mcp.connections[0].source_link.is_some());
        assert!(state.mcp.grants.is_empty());
    }
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
    fn background_run_sees_only_read_declared_tools_and_cannot_call_write_tool() {
        let h = host();
        {
            let db = h.db.lock().unwrap();
            let mut state = store::load(&db).unwrap();
            state.mcp.connections.push(McpConnection {
                oauth: false, id: "source".into(), workspace_id: "w".into(),
                label: "Source".into(), config: ServerConfig::Http {
                    url: "https://example.com/mcp".into(),
                }, enabled: true, trusted: true, has_credentials: false,
                tools: vec![
                    McpTool { read_only: true, name: "lookup".into(), description: "Read".into(), input_schema: "{}".into(), schema_hash: "read".into() },
                    McpTool { read_only: false, name: "delete".into(), description: "Write".into(), input_schema: "{}".into(), schema_hash: "write".into() },
                ], discovered_ms: Some(1), error: None, source_link: None,
            });
            for (name, hash) in [("lookup", "read"), ("delete", "write")] {
                state.mcp.grants.push(ToolGrant {
                    workspace_id: "w".into(), connection_id: "source".into(),
                    tool_name: name.into(), schema_hash: hash.into(),
                });
            }
            store::save(&db, &state).unwrap();
        }
        let revision = store::load(&h.db.lock().unwrap()).unwrap().agent_profiles.revision;
        let lease = h.lease("watch:test", "w", vec!["source".into()], revision).unwrap();
        let token = || neko_protocol::workbench::Secret(lease.token().into());
        let tools = h.bridge(BridgeRequest { token: token(), action: BridgeAction::List }).unwrap();
        assert!(tools.contains("lookup"));
        assert!(!tools.contains("delete"));
        let error = h.bridge(BridgeRequest { token: token(), action: BridgeAction::Call {
            connection_id: "source".into(), tool_name: "delete".into(), arguments_json: "{}".into(),
        }}).unwrap_err();
        assert!(error.contains("read-only"));
    }
    #[test]
    fn imported_disabled_connection_is_created_disabled_without_touching_other_connections() {
        let h = host();
        let (first, _) = h
            .add_connection(
                "w".into(),
                "Existing".into(),
                ServerConfig::Http {
                    url: "https://example.org/mcp".into(),
                },
                false,
                None,
                true,
                false,
            )
            .unwrap();
        let (imported, state) = h
            .add_connection(
                String::new(),
                "Imported".into(),
                ServerConfig::Http {
                    url: "https://example.org/mcp".into(),
                },
                false,
                None,
                false,
                false,
            )
            .unwrap();
        assert!(
            state
                .mcp
                .connections
                .iter()
                .find(|c| c.id == first)
                .unwrap()
                .enabled
        );
        assert!(
            !state
                .mcp
                .connections
                .iter()
                .find(|c| c.id == imported)
                .unwrap()
                .enabled
        );
        assert!(state.mcp.grants.is_empty());
    }

    #[test]
    fn profile_changes_revoke_old_bridge_capabilities() {
        let h = host();
        let lease = h.lease("profile-test", "w", vec![], 0).unwrap();
        let request = || BridgeRequest {
            token: neko_protocol::workbench::Secret(lease.token().into()),
            action: BridgeAction::List,
        };
        assert_eq!(h.bridge(request()).unwrap(), "[]");
        let db = h.db.lock().unwrap();
        store::apply(
            &db,
            Command::AgentProfiles(neko_protocol::agent_profiles::ProfileCommand::Save {
                profile: neko_protocol::agent_profiles::AgentProfile {
                    id: "default".into(),
                    name: "Work".into(),
                    instructions: "Updated instructions".into(),
                },
            }),
        )
        .unwrap();
        drop(db);
        assert!(
            h.bridge(request())
                .unwrap_err()
                .contains("authority changed")
        );
    }

    #[test]
    fn stale_nonchat_claim_cannot_adopt_new_profile_authority_at_issuance() {
        let h = host();
        // Deterministically place the edit between the worker's snapshot read
        // and Host::lease's own reload. No watchdog timing or sleeps involved.
        let claimed_revision = store::load(&h.db.lock().unwrap())
            .unwrap()
            .agent_profiles
            .revision;
        let db = h.db.lock().unwrap();
        let newer = store::apply(
            &db,
            Command::AgentProfiles(neko_protocol::agent_profiles::ProfileCommand::Save {
                profile: neko_protocol::agent_profiles::AgentProfile {
                    id: "default".into(),
                    name: "Work".into(),
                    instructions: "Changed after claim".into(),
                },
            }),
        )
        .unwrap()
        .agent_profiles
        .revision;
        drop(db);
        assert_ne!(claimed_revision, newer);
        let stale = h.lease("task:old-claim", "w", vec![], claimed_revision);
        assert!(
            stale.is_err(),
            "An old actor received a fresh capability after its authority changed"
        );
        assert!(h.lease("task:new-claim", "w", vec![], newer).is_ok());
    }

    #[test]
    fn authentication_attempt_cleans_up_on_unwind_without_removing_a_replacement() {
        let host = host();
        let cancel = Arc::new(AtomicBool::new(false));
        host.auth
            .lock()
            .unwrap()
            .insert("connection".into(), cancel.clone());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _attempt = AuthAttempt {
                host: host.clone(),
                id: "connection".into(),
                cancel: cancel.clone(),
            };
            panic!("fixture failure");
        }));
        assert!(result.is_err());
        assert!(!host.auth.lock().unwrap().contains_key("connection"));
        let attempt = AuthAttempt {
            host: host.clone(),
            id: "connection".into(),
            cancel,
        };
        let replacement = Arc::new(AtomicBool::new(false));
        host.auth
            .lock()
            .unwrap()
            .insert("connection".into(), replacement.clone());
        drop(attempt);
        assert!(Arc::ptr_eq(
            host.auth.lock().unwrap().get("connection").unwrap(),
            &replacement
        ));
    }

    #[test]
    fn retrying_authentication_replaces_a_pending_attempt() {
        let host = host();
        let (connection_id, _) = host
            .add_connection(
                "w".into(),
                "Local OAuth fixture".into(),
                ServerConfig::Http {
                    url: "https://127.0.0.1/mcp".into(),
                },
                false,
                None,
                true,
                false,
            )
            .unwrap();
        let previous = Arc::new(AtomicBool::new(false));
        host.auth
            .lock()
            .unwrap()
            .insert(connection_id.clone(), previous.clone());

        let result = host.command(McpCommand::Authenticate {
            connection_id,
            client_id: None,
        });

        assert!(result.is_ok(), "retry should start a fresh browser attempt: {result:?}");
        assert!(previous.load(Ordering::Acquire), "the previous attempt must be cancelled");
    }

    #[test]
    fn reauthentication_does_not_require_a_working_old_token() {
        let host = host();
        let (connection_id, _) = host
            .add_connection(
                "w".into(),
                "Expired OAuth fixture".into(),
                ServerConfig::Http {
                    url: "https://127.0.0.1/mcp".into(),
                },
                false,
                None,
                true,
                false,
            )
            .unwrap();
        {
            let db = host.db.lock().unwrap();
            let mut state = store::load(&db).unwrap();
            let connection = state
                .mcp
                .connections
                .iter_mut()
                .find(|connection| connection.id == connection_id)
                .unwrap();
            connection.oauth = true;
            connection.has_credentials = true;
            store::save(&db, &state).unwrap();
        }

        let result = host.command(McpCommand::Authenticate {
            connection_id,
            client_id: None,
        });

        assert!(result.is_ok(), "a broken old token must not block a fresh sign-in: {result:?}");
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
                    cwd: None,
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
                    args: vec![],
                    cwd: None,
                },
                trust_local_process: false,
                credentials: None
            })
            .is_err()
        );
        assert!(
            store::load(
                &h.db
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
            )
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
            let db =
                h.db.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
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
                source_link: None,
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
            profile_revision: 0,
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
                profile_revision: 0,
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
            let db =
                h.db.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
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
