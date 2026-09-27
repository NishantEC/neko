use neko_protocol::{mcp_host::*, workbench::Snapshot};

pub fn authorize<'a>(
    state: &'a Snapshot,
    workspace: &str,
    connection: &str,
    tool: &str,
) -> Result<&'a McpTool, String> {
    let connection = state
        .mcp
        .connections
        .iter()
        .find(|c| {
            c.id == connection
                && c.available_in(workspace)
                && c.enabled
                && c.trusted
                && c.error.is_none()
        })
        .ok_or("MCP connection is unavailable in this workspace")?;
    if !state.workspaces.iter().any(|w| w.id == workspace) {
        return Err("Workspace is unavailable".into());
    }
    let tool = connection
        .tools
        .iter()
        .find(|t| t.name == tool)
        .ok_or("MCP tool is no longer available")?;
    if !state.mcp.grants.iter().any(|g| {
        g.workspace_id == workspace
            && g.connection_id == connection.id
            && g.tool_name == tool.name
            && g.schema_hash == tool.schema_hash
    }) {
        return Err("MCP tool needs permission for its current schema".into());
    }
    Ok(tool)
}
pub fn migrate(state: &mut Snapshot) -> Result<bool, String> {
    match state.mcp.version {
        1 => Ok(false),
        0 => {
            for workspace in &mut state.workspaces {
                workspace.away_enabled = false;
            }
            for connection in &mut state.connections {
                connection.enabled = false;
                connection.intake_notice = Some("Historical connection. Add your own MCP server to resume access; no permissions were carried over.".into());
            }
            for issue in &mut state.issues {
                issue.assigned = false;
            }
            state.mcp = McpState::default();
            Ok(true)
        }
        _ => Err(
            "This workspace uses a newer MCP storage version; update Neko before continuing".into(),
        ),
    }
}

pub fn apply_command(state: &mut Snapshot, command: McpCommand, now: i64) -> Result<(), String> {
    match command {
        McpCommand::SetEnabled {
            connection_id,
            enabled,
        } => {
            let c = state
                .mcp
                .connections
                .iter_mut()
                .find(|c| c.id == connection_id)
                .ok_or("MCP connection missing")?;
            c.enabled = enabled;
            if !enabled {
                state
                    .mcp
                    .grants
                    .retain(|g| g.connection_id != connection_id);
            }
        }
        McpCommand::SetToolGrant {
            connection_id,
            tool_name,
            schema_hash,
            allowed,
        } => {
            let workspace_id = state
                .mcp
                .connections
                .iter()
                .find(|c| c.id == connection_id)
                .ok_or("MCP connection missing")?
                .workspace_id
                .clone();
            if workspace_id.is_empty() {
                return Err("Choose a workspace for this global connection's tool grant".into());
            }
            return apply_command(
                state,
                McpCommand::SetWorkspaceToolGrant {
                    workspace_id,
                    connection_id,
                    tool_name,
                    schema_hash,
                    allowed,
                },
                now,
            );
        }
        McpCommand::SetWorkspaceToolGrant {
            workspace_id,
            connection_id,
            tool_name,
            schema_hash,
            allowed,
        } => {
            let c = state
                .mcp
                .connections
                .iter()
                .find(|c| c.id == connection_id)
                .ok_or("MCP connection missing")?;
            if !c.available_in(&workspace_id)
                || !state.workspaces.iter().any(|w| w.id == workspace_id)
            {
                return Err("MCP connection is unavailable in this workspace".into());
            }
            if allowed
                && (!c.enabled
                    || !c.trusted
                    || c.error.is_some()
                    || !c
                        .tools
                        .iter()
                        .any(|t| t.name == tool_name && t.schema_hash == schema_hash))
            {
                return Err("Discover the current tool schema before granting access".into());
            }
            state.mcp.grants.retain(|g| {
                !(g.connection_id == connection_id
                    && g.tool_name == tool_name
                    && g.workspace_id == workspace_id)
            });
            if allowed {
                state.mcp.grants.push(ToolGrant {
                    connection_id,
                    workspace_id,
                    tool_name,
                    schema_hash,
                });
            }
        }
        McpCommand::SaveResponsibility { mut responsibility } => {
            validate_responsibility(state, &responsibility)?;
            if responsibility.id.is_empty() {
                responsibility.id = crate::workbench::new_id();
                responsibility.last_attempt_ms = None;
                responsibility.last_result.clear();
                responsibility.failures = 0;
                responsibility.next_due_ms = now;
                state.mcp.responsibilities.push(responsibility);
            } else {
                let old = state
                    .mcp
                    .responsibilities
                    .iter_mut()
                    .find(|r| {
                        r.id == responsibility.id && r.workspace_id == responsibility.workspace_id
                    })
                    .ok_or("Responsibility missing from this workspace")?;
                if old.instruction != responsibility.instruction
                    || old.connection_ids != responsibility.connection_ids
                {
                    for source in state
                        .mcp
                        .sources
                        .iter_mut()
                        .filter(|s| s.responsibility_id == old.id)
                    {
                        source.eligible = false;
                    }
                    old.next_due_ms = now;
                }
                // Client may edit instructions/grants, never synthesize progress.
                old.instruction = responsibility.instruction;
                old.connection_ids = responsibility.connection_ids;
                old.enabled = responsibility.enabled;
                old.prepare_low_risk = responsibility.prepare_low_risk;
            }
        }
        McpCommand::Wake { responsibility_id } => {
            let r = state
                .mcp
                .responsibilities
                .iter_mut()
                .find(|r| r.id == responsibility_id && r.enabled)
                .ok_or("Responsibility is missing or paused")?;
            r.next_due_ms = now;
        }
        _ => return Err("This MCP command needs the connection controller".into()),
    }
    validate(state)
}

fn text(value: &str, max: usize, required: bool) -> Result<(), String> {
    if value.len() > max || value.contains('\0') || (required && value.trim().is_empty()) {
        Err("MCP field is empty, invalid, or exceeds its size limit".into())
    } else {
        Ok(())
    }
}
pub fn validate_config(config: &ServerConfig) -> Result<(), String> {
    match config {
        ServerConfig::Stdio { command, args, cwd } => {
            text(command, 4096, true)?;
            if !std::path::Path::new(command).is_absolute() || args.len() > 64 {
                return Err("Use an absolute executable path and at most 64 arguments".into());
            }
            for arg in args {
                text(arg, 4096, false)?;
            }
            if let Some(cwd) = cwd {
                text(cwd, 4096, true)?;
                if !std::path::Path::new(cwd).is_absolute() {
                    return Err("MCP working directory must be absolute".into());
                }
            }
        }
        ServerConfig::Http { url } => {
            text(url, 4096, true)?;
            let u = reqwest::Url::parse(url).map_err(|_| "Invalid MCP URL")?;
            let loopback = u.host_str() == Some("localhost");
            let loopback = loopback
                || u.host_str().is_some_and(|h| {
                    h.trim_matches(['[', ']'])
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
                });
            if !(u.scheme() == "https" || (u.scheme() == "http" && loopback))
                || !u.username().is_empty()
                || u.password().is_some()
                || u.fragment().is_some()
                || u.host_str().is_none()
            {
                return Err("MCP requires HTTPS (or explicit loopback HTTP), no URL credentials or fragment".into());
            }
        }
    }
    Ok(())
}
fn validate_responsibility(state: &Snapshot, r: &Responsibility) -> Result<(), String> {
    text(&r.instruction, 16_384, true)?;
    if !state.workspaces.iter().any(|w| w.id == r.workspace_id)
        || r.connection_ids.is_empty()
        || r.connection_ids.len() > 32
    {
        return Err("Choose a workspace and 1–32 MCP connections".into());
    }
    let mut unique = std::collections::HashSet::new();
    for id in &r.connection_ids {
        if !unique.insert(id)
            || !state
                .mcp
                .connections
                .iter()
                .any(|c| &c.id == id && c.available_in(&r.workspace_id))
        {
            return Err(
                "Responsibility connection is duplicated or belongs to another workspace".into(),
            );
        }
    }
    Ok(())
}
pub fn validate(state: &Snapshot) -> Result<(), String> {
    if state.mcp.version != 1
        || state.mcp.connections.len() > 100
        || state.mcp.grants.len() > 2000
        || state.mcp.responsibilities.len() > 100
        || state.mcp.receipts.len() > 1000
        || state.mcp.sources.len() > 1000
    {
        return Err("Unsupported MCP state version or record limit exceeded".into());
    }
    let mut ids = std::collections::HashSet::new();
    for c in &state.mcp.connections {
        text(&c.id, 256, true)?;
        text(&c.label, 256, true)?;
        if !ids.insert(&c.id)
            || (!c.workspace_id.is_empty()
                && !state.workspaces.iter().any(|w| w.id == c.workspace_id))
        {
            return Err("MCP connection identity or workspace is invalid".into());
        }
        validate_config(&c.config)?;
        if c.tools.len() > 128 {
            return Err("MCP tool limit exceeded".into());
        }
        let mut tools = std::collections::HashSet::new();
        let mut bytes = 0;
        for t in &c.tools {
            text(&t.name, 256, true)?;
            text(&t.description, 8192, false)?;
            text(&t.schema_hash, 128, true)?;
            text(&t.input_schema, 32_768, true)?;
            if !tools.insert(&t.name)
                || !serde_json::from_str::<serde_json::Value>(&t.input_schema)
                    .is_ok_and(|v| v.is_object())
            {
                return Err("MCP tool schema is malformed or name is duplicated".into());
            }
            bytes += t.name.len() + t.description.len() + t.input_schema.len();
        }
        if bytes > 262_144 {
            return Err("MCP discovery exceeds its size limit".into());
        }
        if let Some(error) = &c.error {
            text(error, 2048, false)?;
        }
    }
    let mut grants = std::collections::HashSet::new();
    for g in &state.mcp.grants {
        if !grants.insert((&g.connection_id, &g.tool_name, &g.workspace_id))
            || !state.workspaces.iter().any(|w| w.id == g.workspace_id)
            || !state.mcp.connections.iter().any(|c| {
                c.id == g.connection_id
                    && c.available_in(&g.workspace_id)
                    && c.tools
                        .iter()
                        .any(|t| t.name == g.tool_name && t.schema_hash == g.schema_hash)
            })
        {
            return Err("MCP grant references an unavailable tool schema".into());
        }
    }
    let mut responsibilities = std::collections::HashSet::new();
    for r in &state.mcp.responsibilities {
        text(&r.id, 256, true)?;
        text(&r.last_result, 32_768, false)?;
        if !responsibilities.insert(&r.id) {
            return Err("Duplicate responsibility".into());
        }
        validate_responsibility(state, r)?;
    }
    for r in &state.mcp.receipts {
        for s in [
            &r.id,
            &r.run_id,
            &r.workspace_id,
            &r.connection_id,
            &r.tool_name,
            &r.schema_hash,
        ] {
            text(s, 256, true)?;
        }
    }
    for s in &state.mcp.sources {
        for value in [
            &s.id,
            &s.responsibility_id,
            &s.connection_id,
            &s.external_id,
            &s.revision,
        ] {
            text(value, 256, true)?;
        }
        text(&s.title, 1024, true)?;
        text(&s.description, 32_768, false)?;
        if s.receipt_ids.len() > 16 {
            return Err("Too many source receipts".into());
        }
        for id in &s.receipt_ids {
            text(id, 256, true)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> Snapshot {
        let mut state = Snapshot::default();
        state.workspaces.push(neko_protocol::workbench::Workspace {
            id: "a".into(),
            name: "A".into(),
            repository: "/tmp/a".into(),
            instructions: String::new(),
            away_enabled: false,
        });
        state.mcp.connections.push(McpConnection {
            oauth: false,
            id: "c".into(),
            workspace_id: "a".into(),
            label: "User tools".into(),
            config: ServerConfig::Http {
                url: "https://example.com/mcp".into(),
            },
            enabled: true,
            trusted: true,
            has_credentials: false,
            tools: vec![McpTool {
                read_only: false,
                name: "lookup".into(),
                description: "Read a record".into(),
                input_schema: "{}".into(),
                schema_hash: "v1".into(),
            }],
            discovered_ms: Some(1),
            error: None,
            source_link: None,
        });
        state
    }
    #[test]
    fn global_definitions_require_separate_workspace_grants() {
        let mut state = state();
        let mut second = state.workspaces[0].clone();
        second.id = "b".into();
        second.repository = "/tmp/b".into();
        state.workspaces.push(second);
        state.mcp.connections[0].workspace_id.clear();
        assert!(validate(&state).is_ok());
        assert!(authorize(&state, "a", "c", "lookup").is_err());
        let grant = |workspace: &str, allowed| McpCommand::SetWorkspaceToolGrant {
            workspace_id: workspace.into(),
            connection_id: "c".into(),
            tool_name: "lookup".into(),
            schema_hash: "v1".into(),
            allowed,
        };
        apply_command(&mut state, grant("a", true), 1).unwrap();
        assert!(authorize(&state, "a", "c", "lookup").is_ok());
        assert!(authorize(&state, "b", "c", "lookup").is_err());
        apply_command(&mut state, grant("b", true), 1).unwrap();
        apply_command(&mut state, grant("a", false), 1).unwrap();
        assert!(authorize(&state, "a", "c", "lookup").is_err());
        assert!(authorize(&state, "b", "c", "lookup").is_ok());
        assert!(apply_command(&mut state, grant("missing", true), 1).is_err());
    }

    #[test]
    fn grants_are_bound_to_discovered_schema_and_do_not_survive_pause() {
        let mut state = state();
        let grant = |hash: &str| McpCommand::SetToolGrant {
            connection_id: "c".into(),
            tool_name: "lookup".into(),
            schema_hash: hash.into(),
            allowed: true,
        };
        assert!(apply_command(&mut state, grant("not-discovered"), 1).is_err());
        apply_command(&mut state, grant("v1"), 1).unwrap();
        assert!(authorize(&state, "a", "c", "lookup").is_ok());
        apply_command(
            &mut state,
            McpCommand::SetEnabled {
                connection_id: "c".into(),
                enabled: false,
            },
            2,
        )
        .unwrap();
        assert!(state.mcp.grants.is_empty());
        apply_command(
            &mut state,
            McpCommand::SetEnabled {
                connection_id: "c".into(),
                enabled: true,
            },
            3,
        )
        .unwrap();
        assert!(authorize(&state, "a", "c", "lookup").is_err());
    }
    #[test]
    fn responsibilities_cannot_select_another_workspaces_connections() {
        let mut state = state();
        let responsibility = Responsibility {
            id: String::new(),
            workspace_id: "a".into(),
            instruction: "Watch my assigned bugs".into(),
            connection_ids: vec!["foreign".into()],
            enabled: true,
            prepare_low_risk: false,
            next_due_ms: 0,
            last_attempt_ms: None,
            last_result: String::new(),
            failures: 0,
        };
        assert!(
            apply_command(
                &mut state,
                McpCommand::SaveResponsibility {
                    responsibility: responsibility.clone()
                },
                123
            )
            .is_err()
        );
        let mut good = responsibility;
        good.connection_ids = vec!["c".into()];
        apply_command(
            &mut state,
            McpCommand::SaveResponsibility {
                responsibility: good,
            },
            123,
        )
        .unwrap();
        assert_eq!(state.mcp.responsibilities.len(), 1);
        assert_eq!(state.mcp.responsibilities[0].next_due_ms, 123);
        assert!(!state.mcp.responsibilities[0].id.is_empty());
    }
    #[test]
    fn invalid_configs_and_oversized_discovery_are_rejected() {
        let mut state = state();
        for url in [
            "http://example.com/mcp",
            "https://user:secret@example.com/mcp",
            "file:///tmp/x",
            "https://example.com/mcp#fragment",
        ] {
            state.mcp.connections[0].config = ServerConfig::Http { url: url.into() };
            assert!(validate(&state).is_err(), "{url}");
        }
        state.mcp.connections[0].config = ServerConfig::Stdio {
            command: "node".into(),
            args: vec![],
            cwd: None,
        };
        assert!(validate(&state).is_err());
        state.mcp.connections[0].config = ServerConfig::Stdio {
            command: "/usr/bin/node".into(),
            args: vec![],
            cwd: None,
        };
        assert!(validate(&state).is_ok());
        state.mcp.connections[0].tools[0].input_schema = "x".repeat(33_000);
        assert!(validate(&state).is_err());
    }
    #[test]
    fn explicit_grants_allow_only_the_current_schema_and_workspace() {
        let mut state = state();
        assert!(authorize(&state, "a", "c", "lookup").is_err());
        state.mcp.grants.push(ToolGrant {
            connection_id: "c".into(),
            workspace_id: "a".into(),
            tool_name: "lookup".into(),
            schema_hash: "v1".into(),
        });
        assert!(authorize(&state, "a", "c", "lookup").is_ok());
        assert!(authorize(&state, "b", "c", "lookup").is_err());
        state.mcp.connections[0].tools[0].schema_hash = "v2".into();
        assert!(authorize(&state, "a", "c", "lookup").is_err());
        state.mcp.connections[0].tools[0].schema_hash = "v1".into();
        state.mcp.connections[0].enabled = false;
        assert!(authorize(&state, "a", "c", "lookup").is_err());
    }
    #[test]
    fn future_versions_are_not_overwritten() {
        let mut state = state();
        state.mcp.version = 99;
        assert!(migrate(&mut state).is_err());
        assert_eq!(state.mcp.version, 99);
    }
    #[test]
    fn old_data_revokes_standing_authority_without_deleting_evidence() {
        let mut state = state();
        state.mcp = legacy_state();
        state.workspaces[0].away_enabled = true;
        state
            .connections
            .push(neko_protocol::workbench::LinearConnection {
                id: "old".into(),
                workspace_id: "a".into(),
                name: "Old".into(),
                organization_id: "org".into(),
                viewer_id: "me".into(),
                team_ids: vec![],
                project_ids: vec![],
                enabled: true,
                last_sync_ms: Some(1),
                error: None,
                intake_notice: None,
            });
        assert!(migrate(&mut state).unwrap());
        assert!(!state.workspaces[0].away_enabled);
        assert!(!state.connections[0].enabled);
        assert_eq!(state.connections[0].organization_id, "org");
        assert_eq!(state.mcp.version, 1);
        assert!(!migrate(&mut state).unwrap());
    }
}
