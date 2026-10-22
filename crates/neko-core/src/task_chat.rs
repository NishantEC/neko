//! A task's conversation as AI SDK messages (`docs/design/chat-on-ai-sdk.md`).
//!
//! Nothing is stored for this: a task's events already are the log. This is the
//! projection the chat stream endpoint serves for `task:<id>`, the Rust form of
//! what the web app's `threadRows` used to compute on the client:
//!
//! * the brief and your notes are `user` messages;
//! * what an agent said, and the commands it ran, are one `assistant` message
//!   per stretch of one agent (the commands as `dynamic-tool` parts holding the
//!   saved receipt);
//! * status lines are `system` messages with a `data-neko-status` part.
use neko_protocol::workbench::{ChatMessage, ChatRole, Task, TaskEvent};
use serde_json::{json, Value};

const RECEIPT_PREFIX: &str = "VERIFICATION_COMMAND ";
const RECEIPT_ROLES: [&str; 5] = ["scout", "supervisor", "builder", "reviewer", "coordinator"];

/// Who an agent role is to the person.
pub fn agent_name(role: &str) -> &'static str {
    match role {
        "scout" | "supervisor" | "splitter" => "Planner",
        "builder" => "Builder",
        "reviewer" => "Reviewer",
        "learning" => "Memory",
        _ => "Neko",
    }
}

fn is_plumbing(message: &str) -> bool {
    message.starts_with("Command ") || message.starts_with("Advisory:") || message.starts_with("USAGE ")
        || message == "Updating workspace files" || message.starts_with('{') || message.starts_with("Decision: ")
        || message.starts_with("Needs your input before building:") || message.starts_with("Nothing to build:")
        || message == "Agent started in an isolated task worktree"
}

fn head(text: &str, chars: usize) -> String { text.trim().chars().take(chars).collect() }

/// The saved receipt of a command, with the flags that say how far to trust it.
fn receipt(event: &TaskEvent) -> Option<Value> {
    if !RECEIPT_ROLES.contains(&event.role.as_str()) { return None; }
    let raw = event.message.strip_prefix(RECEIPT_PREFIX)?;
    let value = serde_json::from_str::<Value>(raw).ok().filter(Value::is_object).unwrap_or_else(|| json!({}));
    let command = value["command"].as_str().filter(|c| !c.trim().is_empty());
    let output = value["output"].as_str();
    let exit = value["exit_code"].as_i64();
    let flag = |k: &str| value[k] == true;
    let odd_flag = ["command_truncated", "output_truncated", "receipt_incomplete", "receipt_malformed", "raw_truncated"]
        .iter().any(|k| value.get(*k).is_some_and(|v| !v.is_boolean()));
    let incomplete = flag("receipt_incomplete") || flag("receipt_malformed") || !value["raw"].is_null() || odd_flag
        || command.is_none() || output.is_none() || exit.is_none() || flag("command_truncated");
    Some(json!({
        "command": command, "output": output, "exit_code": exit,
        "command_truncated": flag("command_truncated"), "output_truncated": flag("output_truncated"),
        "incomplete": incomplete, "failed": exit.is_some_and(|c| c != 0),
    }))
}

fn message(id: String, role: &str, at: i64, parts: Vec<Value>, meta: Value) -> Value {
    let mut metadata = json!({"atMs": at, "state": "done"});
    if let (Some(base), Some(extra)) = (metadata.as_object_mut(), meta.as_object()) {
        for (k, v) in extra { base.insert(k.clone(), v.clone()); }
    }
    json!({"id": id, "role": role, "parts": parts, "metadata": metadata})
}

pub fn task_messages(task: &Task) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let goal = task.goal.trim();
    if !goal.is_empty() {
        out.push(message("brief".into(), "user", task.created_at_ms, ChatMessage::text_parts(goal), json!({"kind": "brief"})));
    }
    let echoes: Vec<String> = [&task.plan, &task.result].iter().map(|s| head(s, 160)).filter(|s| !s.is_empty()).collect();
    // The stretch of one agent being built, by index in `out`.
    let mut stretch: Option<(usize, String)> = None;
    for (index, event) in task.events.iter().enumerate() {
        let text = event.message.trim();
        if text.is_empty() { continue; }
        let role = event.role.as_str();
        if let Some(receipt) = receipt(event) {
            let call = json!({
                "type": "dynamic-tool", "toolName": "Bash", "toolCallId": format!("receipt-{index}"), "state": "output-available",
                "input": {"command": receipt["command"]}, "output": receipt,
            });
            match &stretch {
                Some((at, who)) if who == role => out[*at]["parts"].as_array_mut().unwrap().push(call),
                _ => {
                    stretch = Some((out.len(), role.to_owned()));
                    out.push(message(format!("a{index}"), "assistant", event.at_ms, vec![call], json!({"author": agent_name(role), "authorRole": role})));
                }
            }
            continue;
        }
        if is_plumbing(text) { continue; }
        let status = |text: &str| message(format!("s{index}"), "system", event.at_ms, vec![json!({"type": "data-neko-status", "data": {"text": text}})], json!({}));
        match role {
            "note" => {
                stretch = None;
                out.push(message(format!("u{index}"), "user", event.at_ms, ChatMessage::text_parts(text), json!({"kind": "note"})));
            }
            "user" | "system" => {
                stretch = None;
                out.push(status(text));
            }
            _ => {
                if echoes.iter().any(|e| text.starts_with(e.as_str()) || e.starts_with(&head(text, 160))) { continue; }
                if text.starts_with("Working in ") || text == "Plan ready." || text.starts_with("Investigation complete.") {
                    stretch = None;
                    out.push(status(text));
                    continue;
                }
                let part = json!({"type": "text", "text": text, "state": "done"});
                match &stretch {
                    Some((at, who)) if who == role => out[*at]["parts"].as_array_mut().unwrap().push(part),
                    _ => {
                        stretch = Some((out.len(), role.to_owned()));
                        out.push(message(format!("a{index}"), "assistant", event.at_ms, vec![part], json!({"author": agent_name(role), "authorRole": role})));
                    }
                }
            }
        }
    }
    out
}

/// A Neko turn as the SDK's `UIMessage`: its parts (fresher from the reply in
/// flight), and the lifecycle the daemon owns as `metadata`.
pub fn neko_message(m: &ChatMessage) -> Value {
    let live = m.pending.then(|| crate::ui_message::live_parts(&m.id)).flatten();
    let state = if m.queued { "queued" } else if m.pending { "pending" } else if m.failed { "failed" } else { "done" };
    let mut metadata = json!({"atMs": m.at_ms, "state": state});
    if m.started_ms > 0 { metadata["startedMs"] = json!(m.started_ms); }
    if let Some(w) = &m.workspace_id { metadata["workspaceId"] = json!(w); }
    if !m.ticket_ids.is_empty() { metadata["tickets"] = json!(m.ticket_ids); }
    json!({
        "id": m.id,
        "role": if m.role == ChatRole::User { "user" } else { "assistant" },
        "parts": live.unwrap_or_else(|| m.parts.clone()),
        "metadata": metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::workbench::{Task, TaskStatus};

    fn task(events: Vec<(&str, &str)>) -> Task {
        let mut t: Task = serde_json::from_value(json!({
            "id": "t1", "workspace_id": "w", "issue_id": null, "title": "T", "goal": "Fix the login", "status": "Building",
            "plan": "", "result": "", "worktree": null, "events": [], "created_at_ms": 10, "updated_at_ms": 10,
        })).unwrap();
        t.status = TaskStatus::Building;
        t.events = events.into_iter().enumerate().map(|(i, (role, message))| TaskEvent { at_ms: 100 + i as i64, role: role.into(), message: message.into() }).collect();
        t
    }

    #[test]
    fn a_task_reads_as_a_conversation_between_you_and_its_agents() {
        let receipt = format!("{RECEIPT_PREFIX}{}", json!({"command": "cargo test", "output": "ok", "exit_code": 0}));
        let t = task(vec![
            ("scout", "I will read the login code."),
            ("scout", "Command running"),
            ("builder", &receipt),
            ("builder", "Changed the check."),
            ("note", "Also add a test."),
            ("system", "Plan approved"),
            ("builder", "Added it."),
        ]);
        let messages = task_messages(&t);
        let shape: Vec<_> = messages.iter().map(|m| (m["role"].as_str().unwrap(), m["parts"].as_array().unwrap().len())).collect();
        assert_eq!(shape, [("user", 1), ("assistant", 1), ("assistant", 2), ("user", 1), ("system", 1), ("assistant", 1)]);
        assert_eq!(messages[1]["metadata"]["author"], "Planner");
        assert_eq!(messages[2]["parts"][0]["toolName"], "Bash");
        assert_eq!(messages[2]["parts"][0]["output"]["exit_code"], 0);
        assert_eq!(messages[2]["parts"][1]["text"], "Changed the check.");
        assert_eq!(messages[4]["parts"][0]["type"], "data-neko-status");
    }

    #[test]
    fn a_pasted_receipt_prefix_from_a_human_is_a_message_and_an_odd_receipt_is_incomplete() {
        let pasted = format!("{RECEIPT_PREFIX}{{\"command\":\"rm -rf\"}}");
        let t = task(vec![("note", &pasted), ("reviewer", &format!("{RECEIPT_PREFIX}{{\"command\":\"x\",\"output\":\"\",\"exit_code\":2}}"))]);
        let messages = task_messages(&t);
        assert_eq!(messages[1]["parts"][0]["type"], "text", "a note is never execution evidence");
        let output = &messages[2]["parts"][0]["output"];
        assert_eq!(output["failed"], true);
        assert_eq!(output["incomplete"], false);
        let t = task(vec![("builder", &format!("{RECEIPT_PREFIX}not json"))]);
        assert_eq!(task_messages(&t)[1]["parts"][0]["output"]["incomplete"], true);
    }

    #[test]
    fn the_plan_and_result_are_not_said_twice() {
        let mut t = task(vec![("scout", "Plan: change the guard in login.rs and add a test"), ("scout", "Plan ready.")]);
        t.plan = "Plan: change the guard in login.rs and add a test".into();
        let messages = task_messages(&t);
        assert_eq!(messages.len(), 2, "the brief and the 'Plan ready.' status only");
        assert_eq!(messages[1]["role"], "system");
    }
}
