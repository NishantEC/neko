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
        .lease(&format!("chat:{turn}"), "a", vec![connection.clone()])
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
        host.lease("foreign", "b", vec![connection.clone()])
            .is_err()
    );
    let foreign = host.lease("empty", "b", vec![]).unwrap();
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
