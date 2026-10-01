//! Service-neutral MCP configuration. Credentials are write-only commands.
use crate::workbench::Secret;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum ServerConfig {
    Stdio {
        command: String,
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    Http {
        url: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpTool {
    /// Server declaration, included in the schema identity the user grants.
    /// Missing declarations conservatively require per-call approval in chat.
    #[serde(default)]
    pub read_only: bool,
    pub name: String,
    pub description: String,
    pub input_schema: String,
    pub schema_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceLink {
    /// Canonical source configuration path, for provenance and revalidation.
    pub source_path: String,
    pub candidate_id: String,
    /// Hash of sanitized transport configuration, excluding credentials.
    pub config_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_identity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpConnection {
    #[serde(default)]
    pub oauth: bool,
    pub id: String,
    /// Empty means a global definition, never a global grant. Each workspace
    /// must separately authorize tools before any actor can use them.
    pub workspace_id: String,
    pub label: String,
    pub config: ServerConfig,
    pub enabled: bool,
    pub trusted: bool,
    pub has_credentials: bool,
    pub tools: Vec<McpTool>,
    pub discovered_ms: Option<i64>,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_link: Option<SourceLink>,
}

#[cfg(test)]
mod linked_tests {
    use super::*;

    #[test]
    fn existing_connection_json_remains_unlinked() {
        let json = r#"{"id":"c","workspace_id":"w","label":"Old","config":{"transport":"http","url":"https://example.com/mcp"},"enabled":true,"trusted":true,"has_credentials":false,"tools":[],"discovered_ms":null,"error":null}"#;
        let connection: McpConnection = serde_json::from_str(json).unwrap();
        assert!(connection.source_link.is_none());
    }
}
impl McpConnection {
    pub fn available_in(&self, workspace: &str) -> bool {
        !workspace.is_empty() && (self.workspace_id.is_empty() || self.workspace_id == workspace)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolGrant {
    pub connection_id: String,
    pub workspace_id: String,
    pub tool_name: String,
    pub schema_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Responsibility {
    pub id: String,
    pub workspace_id: String,
    pub instruction: String,
    pub connection_ids: Vec<String>,
    pub enabled: bool,
    pub prepare_low_risk: bool,
    pub next_due_ms: i64,
    pub last_attempt_ms: Option<i64>,
    pub last_result: String,
    pub failures: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolReceipt {
    pub id: String,
    pub run_id: String,
    pub workspace_id: String,
    pub connection_id: String,
    pub tool_name: String,
    pub schema_hash: String,
    pub at_ms: i64,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceEvidence {
    #[serde(default)]
    pub task_id: Option<String>,
    pub id: String,
    pub responsibility_id: String,
    pub connection_id: String,
    pub external_id: String,
    pub revision: String,
    pub title: String,
    pub description: String,
    pub retrieved_ms: i64,
    pub receipt_ids: Vec<String>,
    pub eligible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpState {
    pub version: u32,
    pub connections: Vec<McpConnection>,
    pub grants: Vec<ToolGrant>,
    pub responsibilities: Vec<Responsibility>,
    /// Source/workspace pairs already considered for automatic watching. A
    /// later manual pause must not be undone by the scheduler.
    #[serde(default)]
    pub auto_watch_scopes: Vec<String>,
    pub receipts: Vec<ToolReceipt>,
    pub sources: Vec<SourceEvidence>,
}
impl Default for McpState {
    fn default() -> Self {
        Self {
            version: 2,
            connections: vec![],
            grants: vec![],
            responsibilities: vec![],
            auto_watch_scopes: vec![],
            receipts: vec![],
            sources: vec![],
        }
    }
}
pub fn legacy_state() -> McpState {
    McpState {
        version: 0,
        ..McpState::default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpCommand {
    Authenticate {
        connection_id: String,
        client_id: Option<String>,
    },
    AddConnection {
        workspace_id: String,
        label: String,
        config: ServerConfig,
        trust_local_process: bool,
        credentials: Option<Secret>,
    },
    LinkSource {
        workspace_id: String,
        candidate_id: String,
        trust_local_process: bool,
    },
    Discover {
        connection_id: String,
    },
    SetEnabled {
        connection_id: String,
        enabled: bool,
    },
    SetToolGrant {
        connection_id: String,
        tool_name: String,
        schema_hash: String,
        allowed: bool,
    },
    SetWorkspaceToolGrant {
        workspace_id: String,
        connection_id: String,
        tool_name: String,
        schema_hash: String,
        allowed: bool,
    },
    SaveResponsibility {
        responsibility: Responsibility,
    },
    Wake {
        responsibility_id: String,
    },
    /// Deletes a responsibility (for example a dismissed suggestion). Its
    /// unlinked source observations go with it; ones behind tickets stay.
    RemoveResponsibility {
        responsibility_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BridgeAction {
    List,
    Call {
        connection_id: String,
        tool_name: String,
        arguments_json: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeRequest {
    pub token: Secret,
    pub action: BridgeAction,
}
