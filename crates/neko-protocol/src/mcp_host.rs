//! Service-neutral MCP configuration. Credentials are write-only commands.
use crate::workbench::Secret;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum ServerConfig {
    Stdio { command: String, args: Vec<String> },
    Http { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: String,
    pub schema_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpConnection {
    #[serde(default)]
    pub oauth: bool,
    pub id: String,
    pub workspace_id: String,
    pub label: String,
    pub config: ServerConfig,
    pub enabled: bool,
    pub trusted: bool,
    pub has_credentials: bool,
    pub tools: Vec<McpTool>,
    pub discovered_ms: Option<i64>,
    pub error: Option<String>,
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
    pub receipts: Vec<ToolReceipt>,
    pub sources: Vec<SourceEvidence>,
}
impl Default for McpState {
    fn default() -> Self {
        Self {
            version: 1,
            connections: vec![],
            grants: vec![],
            responsibilities: vec![],
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
    SaveResponsibility {
        responsibility: Responsibility,
    },
    Wake {
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
