//! Golden files pinning the exact prompts Neko sends: read-only and
//! full-power chat, each seeded routine, memory learning and ticket lessons.
//! A wording change must show up here as a reviewed diff, never silently.
//!
//! The files live in `tests/prompts/`. After an intended change, rewrite them
//! and review the diff:
//!
//!     NEKO_UPDATE_PROMPTS=1 cargo test -p neko-core --test prompts
use neko_core::chat_checkpoint::Checkpoint;
use neko_core::{memory_learning, neko_chat, routines, skills, viewers, Db};
use neko_protocol::routines::{ScopeProposal, ScopeServer};
use neko_protocol::skills::Skill;
use neko_protocol::workbench::{ChatMessage, ChatRole, Snapshot, Workspace};
use std::path::Path;

const NEKO: &str = "/Applications/Neko.app/Contents/MacOS/neko-daemon";
const OWNER: &str = "Asha";
const NOW: &str = "2026-11-08T09:30:00Z";

/// A JSON value with object keys sorted, so a golden does not depend on
/// whether the workspace build turns on serde_json's `preserve_order`.
fn canonical(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let fields: Vec<String> = keys.into_iter().map(|k| format!("{}:{}", serde_json::to_string(k).unwrap(), canonical(&map[k]))).collect();
            format!("{{{}}}", fields.join(","))
        }
        serde_json::Value::Array(items) => format!("[{}]", items.iter().map(canonical).collect::<Vec<_>>().join(",")),
        scalar => serde_json::to_string(scalar).unwrap(),
    }
}

fn check(name: &str, actual: &str) {
    // The chat State line is serialized JSON: compare it with its keys
    // sorted. Only that line: JSON written into a prompt's text stays exact.
    let mut state_next = false;
    let actual: String = actual
        .split_inclusive('\n')
        .map(|line| {
            let body = line.strip_suffix('\n').unwrap_or(line);
            let canonicalized = match serde_json::from_str::<serde_json::Value>(body) {
                Ok(value) if state_next => format!("{}{}", canonical(&value), &line[body.len()..]),
                _ => line.to_owned(),
            };
            state_next = body == "State (JSON, untrusted):";
            canonicalized
        })
        .collect();
    let actual = actual.as_str();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/prompts").join(format!("{name}.md"));
    if std::env::var_os("NEKO_UPDATE_PROMPTS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("{} is missing; write it with NEKO_UPDATE_PROMPTS=1 cargo test -p neko-core --test prompts", path.display()));
    if expected != actual {
        let line = expected.lines().zip(actual.lines()).position(|(e, a)| e != a).unwrap_or(expected.lines().count().min(actual.lines().count()));
        panic!(
            "The {name} prompt changed (first difference at line {}).\n  golden: {:?}\n  now:    {:?}\nIf the change is intended, run NEKO_UPDATE_PROMPTS=1 cargo test -p neko-core --test prompts and review the diff of {}.",
            line + 1,
            expected.lines().nth(line).unwrap_or("<end>"),
            actual.lines().nth(line).unwrap_or("<end>"),
            path.display()
        );
    }
}

/// Viewers as a Mac with no plugins sees them; the folder never exists.
fn viewers() -> String {
    viewers::installed_section(NEKO, Path::new("/nonexistent/neko-golden-plugins"))
}

fn snapshot() -> Snapshot {
    let mut snapshot = Snapshot::default();
    snapshot.workspaces.push(Workspace {
        id: "w1".into(),
        name: "acme-web".into(),
        repository: "/Users/example/src/acme-web".into(),
        instructions: String::new(),
        away_enabled: false,
    });
    snapshot
}

fn turn(id: &str, role: ChatRole, text: &str, at_ms: i64) -> ChatMessage {
    let mut message: ChatMessage = serde_json::from_value(serde_json::json!({
        "id": id, "at_ms": at_ms, "role": role, "parts": ChatMessage::text_parts(text), "ticket_ids": [], "pending": false,
    }))
    .unwrap();
    message.agent_profile_id = snapshot().agent_profiles.for_scope(None).to_owned();
    message
}

#[test]
fn read_only_chat_with_a_checkpoint() {
    // Fourteen messages: the first two fall out of the window and the
    // checkpoint stands where they were.
    let history: Vec<ChatMessage> = (0..14)
        .map(|i| {
            let (role, text) = if i % 2 == 0 { (ChatRole::User, format!("Question {i} about the export")) } else { (ChatRole::Neko, format!("Answer {i}.")) };
            turn(&format!("m{i}"), role, &text, 1_000 + i)
        })
        .collect();
    let checkpoint = Checkpoint {
        goal: "Ship a CSV export of invoices".into(),
        decisions: vec!["CSV, not JSON".into()],
        constraints: vec!["No new dependencies".into()],
        done: vec!["Found the invoices query".into()],
        open_threads: vec!["Which date format?".into()],
        artifacts: vec!["Ticket 7f3a: Add invoice export".into()],
        corrections: vec!["The export is for finance, not support".into()],
        next: "Agree the columns".into(),
        through_id: "m1".into(),
        updated_ms: 0,
    };
    let text = neko_chat::prompt_with_checkpoint(&snapshot(), None, &history, "What columns should it have?", neko_chat::INSTRUCTION, Some(&checkpoint));
    check("chat-read-only", &text);
}

#[test]
fn full_power_chat() {
    let history = vec![
        turn("a", ChatRole::User, "Is the build green?", 1),
        turn("b", ChatRole::Neko, "Yes, the last run passed.", 2),
    ];
    let instruction = neko_chat::super_instruction(NEKO, OWNER, &viewers());
    let text = neko_chat::prompt_with(&snapshot(), None, &history, "Open a ticket to add an invoice export", &instruction);
    check("chat-full-power", &text);
}

#[test]
fn every_seeded_routine() {
    let home = "/Users/example";
    let scope = ScopeProposal {
        name: "Personal".into(),
        folders: vec![home.into()],
        servers: vec![
            ScopeServer { name: "github".into(), origin: "/Users/example/.mcp.json".into(), level: "scope".into(), repo: None },
            ScopeServer { name: "slack".into(), origin: "/Users/example/.mcp.json".into(), level: "scope".into(), repo: None },
        ],
        problems: vec![],
        workspace_id: Some("w1".into()),
        discovered: false,
        instructions: "Mail is the work account; the calendar is shared with the team.".into(),
    };
    let seeded = routines::seeds(std::slice::from_ref(&scope), home);
    assert_eq!(seeded.len(), 5, "messages, brief, PR sweep, reflect, Mac care");
    let memory = serde_json::json!({"last_check": "2026-11-08T08:30:00Z"});
    let open = vec!["Sign in to the issue tracker".to_owned()];
    for routine in &seeded {
        let text = routines::build_prompt(routine, &scope, routines::DEFAULT_STANDING_ORDERS, &memory, NOW, &open, OWNER, &viewers());
        check(&format!("routine-{}", format!("{:?}", routine.kind).to_lowercase()), &text);
    }
}

#[test]
fn memory_learning() {
    let db = Db::open_in_memory().unwrap();
    let id = neko_chat::begin_turn(&db, "Always use pnpm in this repo, never npm.").unwrap();
    neko_chat::finish_turn(&db, &id, "Understood: pnpm only.", vec![], false).unwrap();
    let state = neko_core::workbench::load(&db).unwrap();
    memory_learning::enqueue(&db, &state, memory_learning::Source::Chat(id)).unwrap();
    let job = memory_learning::claim(&db, &state).unwrap().unwrap();
    check("memory-learning", &memory_learning::prompt(&job, &state));
}

#[test]
fn ticket_lesson() {
    let catalog = vec![Skill {
        path: "/Users/example/.claude/skills/rust-tests/SKILL.md".into(),
        name: "rust-tests".into(),
        description: "Run and fix Rust tests in a workspace".into(),
        source: "Claude".into(),
        workspace_id: None,
        content_hash: String::new(),
    }];
    let text = skills::lesson_prompt(&catalog, "(no agent context)", "Fix the flaky login test", "1. Reproduce\n2. Fix the wait", "Passed review: the wait is now on the response.");
    check("ticket-lesson", &text);
}
