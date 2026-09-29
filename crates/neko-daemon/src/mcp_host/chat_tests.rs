use super::*;
use neko_core::neko_chat;
use neko_protocol::workbench::{ChatToolStatus, Secret, Workspace};
use std::time::{Duration, Instant};

fn fixture(read_only: bool) -> (Arc<Host>, String, Lease, String) {
    let db = Db::open_in_memory().unwrap();
    let mut state = Snapshot::default();
    for id in ["a", "b"] {
        state.workspaces.push(Workspace {
            id: id.into(),
            name: id.into(),
            repository: "/tmp".into(),
            instructions: String::new(),
            away_enabled: false,
        });
    }
    store::save(&db, &state).unwrap();
    let turn = neko_chat::begin_scoped_turn(&db, "Look this up", Some("a")).unwrap();
    let host = Arc::new(Host::new(Arc::new(Mutex::new(db))));
    let node = std::process::Command::new("which")
        .arg("node")
        .output()
        .unwrap();
    let command = String::from_utf8(node.stdout).unwrap().trim().to_owned();
    let state = host
        .command(McpCommand::AddConnection {
            workspace_id: "a".into(),
            label: "Fixture".into(),
            config: ServerConfig::Stdio {
                command,
                args: vec![
                    format!(
                        "{}/../../scripts/fixtures/mcp-host.mjs",
                        env!("CARGO_MANIFEST_DIR")
                    ),
                    if read_only { "readonly" } else { "normal" }.into(),
                ],
                cwd: None,
            },
            trust_local_process: true,
            credentials: None,
        })
        .unwrap();
    let connection = state.mcp.connections[0].id.clone();
    let state = host
        .command(McpCommand::Discover {
            connection_id: connection.clone(),
        })
        .unwrap();
    let tool = &state.mcp.connections[0].tools[0];
    assert_eq!(tool.read_only, read_only);
    host.command(McpCommand::SetToolGrant {
        connection_id: connection.clone(),
        tool_name: tool.name.clone(),
        schema_hash: tool.schema_hash.clone(),
        allowed: true,
    })
    .unwrap();
    let lease = host
        .lease(&format!("chat:{turn}"), "a", vec![connection.clone()], 0)
        .unwrap();
    (host, turn, lease, connection)
}

fn call(host: &Host, lease: &Lease, connection: &str) -> Result<String, String> {
    host.bridge(BridgeRequest {
        token: Secret(lease.token().into()),
        action: BridgeAction::Call {
            connection_id: connection.into(),
            tool_name: "echo".into(),
            arguments_json: "{\"text\":\"proof\"}".into(),
        },
    })
}

#[test]
fn all_workspaces_chat_uses_only_an_unambiguous_workspace_authority() {
    let db = Db::open_in_memory().unwrap();
    let mut state = Snapshot::default();
    state.workspaces.push(Workspace {
        id: "home".into(), name: "Home".into(), repository: "/tmp".into(),
        instructions: String::new(), away_enabled: false,
    });
    store::save(&db, &state).unwrap();
    let turn = neko_chat::begin_turn(&db, "Use my tools").unwrap();
    let host = Arc::new(Host::new(Arc::new(Mutex::new(db))));
    assert!(host.lease(&format!("chat:{turn}"), "home", vec![], 0).is_ok());
    {
        let db = host.db.lock().unwrap();
        let mut state = store::load(&db).unwrap();
        state.workspaces.push(Workspace {
            id: "other".into(), name: "Other".into(), repository: "/tmp".into(),
            instructions: String::new(), away_enabled: false,
        });
        store::save(&db, &state).unwrap();
    }
    assert!(host.lease(&format!("chat:{turn}"), "home", vec![], 0).is_err());
}

#[test]
fn all_workspaces_chat_can_call_a_granted_read_tool_in_its_only_workspace() {
    let (host, scoped_turn, scoped_lease, connection) = fixture(true);
    drop(scoped_lease);
    let global_turn = {
        let db = host.db.lock().unwrap();
        neko_chat::finish_turn(&db, &scoped_turn, "done", vec![], false).unwrap();
        let mut state = store::load(&db).unwrap();
        state.workspaces.retain(|w| w.id == "a");
        store::save(&db, &state).unwrap();
        neko_chat::begin_turn(&db, "Check my admin tool").unwrap()
    };
    let lease = host.lease(&format!("chat:{global_turn}"), "a", vec![connection.clone()], 0).unwrap();
    let catalog = host.bridge(BridgeRequest {
        token: Secret(lease.token().into()), action: BridgeAction::List,
    }).unwrap();
    assert!(catalog.contains("echo"));
    assert!(call(&host, &lease, &connection).unwrap().contains("proof"));
    let state = store::load(&host.db.lock().unwrap()).unwrap();
    let turn = state.conversation.iter().find(|m| m.id == global_turn).unwrap();
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.tool_calls[0].status, ChatToolStatus::Succeeded);
}

#[test]
fn linked_workspace_stdio_server_dispatches_only_after_its_tool_grant() {
    let directory = tempfile::tempdir().unwrap();
    let repository = directory.path().join("project");
    std::fs::create_dir_all(&repository).unwrap();
    let node = std::process::Command::new("which")
        .arg("node")
        .output()
        .unwrap();
    let executable = String::from_utf8(node.stdout).unwrap().trim().to_owned();
    let script = format!(
        "{}/../../scripts/fixtures/mcp-host.mjs",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::write(repository.join(".mcp.json"), serde_json::json!({"mcpServers":{"fixture":{"command":executable,"args":[script,"readonly"]}}}).to_string()).unwrap();
    let db = Db::open_in_memory().unwrap();
    let mut state = Snapshot::default();
    state.workspaces.push(Workspace {
        id: "linked".into(),
        name: "Linked".into(),
        repository: repository.to_string_lossy().into_owned(),
        instructions: String::new(),
        away_enabled: false,
    });
    store::save(&db, &state).unwrap();
    let host = Arc::new(Host::new(Arc::new(Mutex::new(db))));
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let found = neko_core::setup_import::discover(
        &home,
        &[repository],
        &[std::path::PathBuf::from("/usr/bin")],
        &Default::default(),
    );
    let candidate_id = found
        .candidates
        .iter()
        .find(|c| c.preview.name == "fixture")
        .unwrap()
        .preview
        .id
        .clone();
    let linked = host
        .command(McpCommand::LinkSource {
            workspace_id: "linked".into(),
            candidate_id,
            trust_local_process: true,
        })
        .unwrap();
    let id = linked.mcp.connections[0].id.clone();
    let lease = host
        .lease("linked-test", "linked", vec![id.clone()], 0)
        .unwrap();
    assert!(call(&host, &lease, &id).is_err());
    let discovered = host
        .command(McpCommand::Discover {
            connection_id: id.clone(),
        })
        .unwrap();
    let tool = &discovered.mcp.connections[0].tools[0];
    host.command(McpCommand::SetToolGrant {
        connection_id: id.clone(),
        tool_name: tool.name.clone(),
        schema_hash: tool.schema_hash.clone(),
        allowed: true,
    })
    .unwrap();
    assert!(call(&host, &lease, &id).unwrap().contains("proof"));
}

#[test]
fn global_tool_dispatch_keeps_workspace_grants_separate() {
    let (host, _, lease_a, connection) = fixture(true);
    {
        let db = host.db.lock().unwrap();
        let mut state = store::load(&db).unwrap();
        state.mcp.connections[0].workspace_id.clear();
        store::save(&db, &state).unwrap();
    }
    assert!(
        call(&host, &lease_a, &connection)
            .unwrap()
            .contains("proof")
    );
    let lease_b = host
        .lease("ungranted-b", "b", vec![connection.clone()], 0)
        .unwrap();
    assert!(call(&host, &lease_b, &connection).is_err());
    let state = store::load(&host.db.lock().unwrap()).unwrap();
    assert_eq!(state.mcp.receipts.len(), 1);
    assert_eq!(state.mcp.receipts[0].workspace_id, "a");
}

fn wait_card(host: &Host, turn: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let db = host.db.lock().unwrap();
        let messages = neko_chat::load(&db).unwrap();
        if let Some(call) = messages
            .iter()
            .find(|m| m.id == turn)
            .unwrap()
            .tool_calls
            .first()
        {
            assert_eq!(call.status, ChatToolStatus::AwaitingApproval);
            return call.id.clone();
        }
        drop(db);
        assert!(Instant::now() < deadline, "Approval card never appeared");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn granted_read_runs_and_records_inline_call_and_receipt() {
    let (host, turn, lease, connection) = fixture(true);
    let result = call(&host, &lease, &connection).unwrap();
    assert!(result.contains("proof") && result.contains("receipt_id"));
    let db = host.db.lock().unwrap();
    assert_eq!(
        neko_chat::call_status(
            &db,
            &turn,
            &neko_chat::load(&db).unwrap()[1].tool_calls[0].id
        )
        .unwrap(),
        ChatToolStatus::Succeeded
    );
    let state = store::load(&db).unwrap();
    assert_eq!(state.mcp.receipts.len(), 1);
    assert_eq!(state.mcp.receipts[0].run_id, format!("chat:{turn}"));
    assert_eq!(state.mcp.receipts[0].workspace_id, "a");
}

#[test]
fn action_waits_without_dispatch_until_approved_once() {
    let (host, turn, lease, connection) = fixture(false);
    std::thread::scope(|threads| {
        let run = threads.spawn(|| call(&host, &lease, &connection));
        let id = wait_card(&host, &turn);
        {
            let db = host.db.lock().unwrap();
            assert!(store::load(&db).unwrap().mcp.receipts.is_empty());
            neko_chat::decide_call(&db, &turn, &id, true).unwrap();
            assert!(neko_chat::decide_call(&db, &turn, &id, true).is_err());
        }
        assert!(run.join().unwrap().unwrap().contains("receipt_id"));
    });
}

#[test]
fn pending_chat_approval_does_not_reserve_mcp_execution_slot() {
    let (host, turn, lease, connection) = fixture(false);
    std::thread::scope(|threads| {
        let run = threads.spawn(|| call(&host, &lease, &connection));
        let id = wait_card(&host, &turn);
        let slot_available = host.call_slot.try_lock().is_ok();
        let discovered = host
            .command(McpCommand::Discover {
                connection_id: connection.clone(),
            })
            .unwrap();
        assert_eq!(discovered.mcp.connections[0].tools[0].name, "echo");
        neko_chat::decide_call(&host.db.lock().unwrap(), &turn, &id, true).unwrap();
        assert!(run.join().unwrap().unwrap().contains("receipt_id"));
        assert!(
            slot_available,
            "Approval must not block unrelated MCP operations"
        );
    });
}

#[test]
fn denial_and_cancellation_never_dispatch() {
    for cancel in [false, true] {
        let (host, turn, lease, connection) = fixture(false);
        std::thread::scope(|threads| {
            let run = threads.spawn(|| call(&host, &lease, &connection));
            let id = wait_card(&host, &turn);
            if cancel {
                host.cancel_chat(&turn);
            } else {
                neko_chat::decide_call(&host.db.lock().unwrap(), &turn, &id, false).unwrap();
            }
            assert!(run.join().unwrap().is_err());
        });
        assert!(
            store::load(&host.db.lock().unwrap())
                .unwrap()
                .mcp
                .receipts
                .is_empty()
        );
    }
}

#[test]
fn timeout_restart_and_dropped_lease_fail_closed() {
    let (host, turn, lease, connection) = fixture(false);
    let scope = host.registry.get(lease.token(), store::now_ms()).unwrap();
    let tool = store::load(&host.db.lock().unwrap())
        .unwrap()
        .mcp
        .connections[0]
        .tools[0]
        .clone();
    assert!(
        host.chat_approval(&scope, &connection, &tool, "{}", Duration::ZERO)
            .is_err()
    );
    assert!(
        store::load(&host.db.lock().unwrap())
            .unwrap()
            .mcp
            .receipts
            .is_empty()
    );
    neko_chat::recover_interrupted(&host.db.lock().unwrap()).unwrap();
    assert!(call(&host, &lease, &connection).is_err());
    assert!(neko_chat::decide_call(&host.db.lock().unwrap(), &turn, "missing", true).is_err());
    let token = lease.token().to_owned();
    drop(lease);
    assert!(host.registry.get(&token, store::now_ms()).is_err());
}

#[test]
fn ungranted_and_cross_workspace_calls_are_rejected() {
    let (host, _, lease, connection) = fixture(true);
    assert!(
        host.lease("foreign", "b", vec![connection.clone()], 0)
            .is_err()
    );
    let foreign = host.lease("empty", "b", vec![], 0).unwrap();
    assert!(call(&host, &foreign, &connection).is_err());
    host.command(McpCommand::SetEnabled {
        connection_id: connection.clone(),
        enabled: false,
    })
    .unwrap();
    assert!(call(&host, &lease, &connection).is_err());
    assert!(
        store::load(&host.db.lock().unwrap())
            .unwrap()
            .mcp
            .receipts
            .is_empty()
    );
}
