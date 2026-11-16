//! A conversation as turns the screen draws: the open task's events, and Neko's own chat. Pure
//! functions of daemon data, so every rule is tested without a window.
//!
//! The task side is the Rust form of `the daemon's task_chat::task_messages` (the daemon's
//! projection, which the web UI reads over HTTP; this client has only the snapshot, so it
//! derives the same turns from the same events, the same way). Parts of a chat reply are the AI
//! SDK's `UIMessage` parts (`text`, `reasoning`, `dynamic-tool`, `data-neko-*`), as the web's
//! `chat/sdk/parts.ts` reads them.

use neko_protocol::workbench::{ChatMessage, ChatRole, Task, TaskEvent};
use serde_json::Value;

const RECEIPT_PREFIX: &str = "VERIFICATION_COMMAND ";
const RECEIPT_ROLES: [&str; 5] = ["scout", "supervisor", "builder", "reviewer", "coordinator"];
/// The tool part the daemon uses for an action Neko proposes on a task.
pub const ACTION_TOOL: &str = "neko_task_action";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Agent,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Command,
    Edit,
    Read,
    Search,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolState {
    Running,
    Done,
    Failed,
    /// Waiting for a click: approve or decline.
    Waiting,
    Declined,
}

/// What an edit changed, when the tool's input says (old and new text), so the card can show a diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub path: String,
    pub old: String,
    pub new: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tool {
    /// The part's call id: stable while the call goes from running to done.
    pub id: String,
    /// One line: "Ran `cargo test`", "Edited export.sh".
    pub title: String,
    pub kind: Kind,
    pub state: ToolState,
    /// What opens: the command and its output, or the arguments.
    pub detail: Option<String>,
    pub edit: Option<Edit>,
    /// Set on a Neko tool that needs approval: the id its decision carries.
    pub approval: Option<String>,
    /// The tool's name (`neko_task_action` for an action Neko proposes on a task).
    pub name: String,
    /// What a click would approve, in a line: the proposed action's text, or the tool's arguments.
    pub approval_note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Text(String),
    Tool(Tool),
    /// A status line between turns ("Plan ready.").
    Status(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnKind {
    Normal,
    /// A task's first message: its goal.
    Brief,
    /// A note the person added.
    Note,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub id: String,
    pub role: Role,
    /// Who spoke: "Planner", "Builder", "Reviewer", "Neko", "you".
    pub author: String,
    pub at_ms: i64,
    pub blocks: Vec<Block>,
    pub kind: TurnKind,
    /// A chat-opened task's brief: the person's own words.
    pub request: Option<String>,
    pub pending: bool,
    pub failed: bool,
    pub queued: bool,
    /// Tasks the turn opened (Neko's chat).
    pub tickets: Vec<String>,
}

impl Turn {
    fn new(id: String, role: Role, author: &str, at_ms: i64, blocks: Vec<Block>) -> Turn {
        Turn { id, role, author: author.to_owned(), at_ms, blocks, kind: TurnKind::Normal, request: None, pending: false, failed: false, queued: false, tickets: Vec::new() }
    }

    pub fn text(&self) -> String {
        self.blocks.iter().filter_map(|b| if let Block::Text(t) = b { Some(t.as_str()) } else { None }).collect::<Vec<_>>().join("\n\n")
    }

    /// The tool cards of the turn.
    pub fn tools(&self) -> impl Iterator<Item = &Tool> {
        self.blocks.iter().filter_map(|b| if let Block::Tool(t) = b { Some(t) } else { None })
    }
}

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
    message.starts_with("Command ")
        || message.starts_with("Advisory:")
        || message.starts_with("USAGE ")
        || message == "Updating workspace files"
        || message.starts_with('{')
        || message.starts_with("Decision: ")
        || message.starts_with("Needs your input before building:")
        || message.starts_with("Nothing to build:")
        || message == "Agent started in an isolated task worktree"
}

fn head(text: &str, chars: usize) -> String {
    text.trim().chars().take(chars).collect()
}

/// `"/bin/zsh -lc 'rg -n foo'"` reads as `rg -n foo` (the web's `shortCommand`).
pub fn short_command(command: &str) -> String {
    let mut text = command.to_owned();
    for shell in ["/bin/zsh -lc ", "/bin/bash -lc ", "bash -lc ", "zsh -lc "] {
        if let Some(rest) = text.strip_prefix(shell) {
            let rest = rest.to_owned();
            text = match rest.chars().next() {
                Some(q @ ('\'' | '"')) if rest.len() > 1 && rest.ends_with(q) => rest[1..rest.len() - 1].to_owned(),
                _ => rest,
            };
        }
    }
    text = text.replace("/Library/Developer/CommandLineTools/usr/bin/", "");
    while let Some(cut) = text.find(" && ").filter(|_| text.starts_with("cd ")) {
        let target = text[3..cut].trim_matches(|c| c == '"' || c == '\'' || c == ' ');
        if !(target.contains("/neko/task-worktrees/") || target == ".") {
            break;
        }
        text = text[cut + 4..].to_owned();
    }
    text.lines().next().unwrap_or("").chars().take(140).collect()
}

/// What a command event's saved receipt says, for the context panel's Terminal tab (the same events
/// [`task_turns`] shows as tool cards).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandReceipt {
    pub command: String,
    pub output: String,
    pub exit: Option<i64>,
}

/// The command, output and exit code of a receipt event; None for any other event, and for a receipt
/// with no command (it names nothing to run).
pub fn command_receipt(event: &TaskEvent) -> Option<CommandReceipt> {
    if !RECEIPT_ROLES.contains(&event.role.as_str()) {
        return None;
    }
    let raw = event.message.strip_prefix(RECEIPT_PREFIX)?;
    let value = serde_json::from_str::<Value>(raw).ok().filter(Value::is_object)?;
    let command = value["command"].as_str().filter(|c| !c.trim().is_empty())?;
    Some(CommandReceipt { command: command.to_owned(), output: value["output"].as_str().unwrap_or("").to_owned(), exit: value["exit_code"].as_i64() })
}

/// The saved receipt of a command event, as a finished tool card.
fn receipt(event: &TaskEvent, index: usize) -> Option<Tool> {
    if !RECEIPT_ROLES.contains(&event.role.as_str()) {
        return None;
    }
    let raw = event.message.strip_prefix(RECEIPT_PREFIX)?;
    let value = serde_json::from_str::<Value>(raw).ok().filter(Value::is_object).unwrap_or_else(|| serde_json::json!({}));
    let command = value["command"].as_str().filter(|c| !c.trim().is_empty());
    let output = value["output"].as_str();
    let exit = value["exit_code"].as_i64();
    let failed = exit.is_some_and(|c| c != 0);
    let flag = |k: &str| value[k] == true;
    // The daemon's own test of how far a saved receipt can be trusted (`task_chat::receipt`).
    let odd_flag = ["command_truncated", "output_truncated", "receipt_incomplete", "receipt_malformed", "raw_truncated"].iter().any(|k| value.get(*k).is_some_and(|v| !v.is_boolean()));
    let incomplete = flag("receipt_incomplete") || flag("receipt_malformed") || !value["raw"].is_null() || odd_flag || command.is_none() || output.is_none() || exit.is_none() || flag("command_truncated");
    let mut lines = Vec::new();
    if let Some(c) = command {
        lines.push(format!("$ {c}"));
    }
    if let Some(o) = output {
        lines.push(if o.trim().is_empty() { "No output.".to_owned() } else { o.trim().to_owned() });
    }
    if let Some(code) = exit.filter(|c| *c != 0) {
        lines.push(format!("exit {code}"));
    }
    if incomplete {
        lines.push("This saved record is incomplete. It does not provide full command evidence.".to_owned());
    }
    if flag("command_truncated") {
        lines.push("The command was shortened before it was saved.".to_owned());
    }
    if flag("output_truncated") {
        lines.push("Output was shortened before it was saved. Only the retained output is available here.".to_owned());
    }
    Some(Tool {
        id: format!("receipt-{index}"),
        title: match command {
            Some(c) => format!("Ran `{}`", short_command(c)),
            None => "Command details unavailable".to_owned(),
        },
        kind: Kind::Command,
        state: if failed { ToolState::Failed } else { ToolState::Done },
        detail: Some(lines.join("\n\n")),
        edit: None,
        approval: None,
        name: "Bash".to_owned(),
        approval_note: None,
    })
}

/// A task's conversation as turns: the brief and notes are the person's, what an agent said and
/// the commands it ran are one turn per stretch of one agent, status lines stand between.
pub fn task_turns(task: &Task) -> Vec<Turn> {
    let mut out: Vec<Turn> = Vec::new();
    let goal = task.goal.trim();
    if !goal.is_empty() {
        let mut turn = Turn::new("brief".into(), Role::User, "you", task.created_at_ms, vec![Block::Text(goal.to_owned())]);
        turn.kind = TurnKind::Brief;
        turn.request = task.request.as_deref().map(str::trim).filter(|r| !r.is_empty()).map(str::to_owned);
        out.push(turn);
    }
    let echoes: Vec<String> = [&task.plan, &task.result].iter().map(|s| head(s, 160)).filter(|s| !s.is_empty()).collect();
    let mut stretch: Option<(usize, String)> = None;
    for (index, event) in task.events.iter().enumerate() {
        let text = event.message.trim();
        if text.is_empty() {
            continue;
        }
        let role = event.role.as_str();
        let put = |out: &mut Vec<Turn>, stretch: &mut Option<(usize, String)>, block: Block| match stretch {
            Some((at, who)) if who == role => out[*at].blocks.push(block),
            _ => {
                *stretch = Some((out.len(), role.to_owned()));
                out.push(Turn::new(format!("a{index}"), Role::Agent, agent_name(role), event.at_ms, vec![block]));
            }
        };
        if let Some(tool) = receipt(event, index) {
            put(&mut out, &mut stretch, Block::Tool(tool));
            continue;
        }
        if is_plumbing(text) {
            continue;
        }
        let status = |out: &mut Vec<Turn>| out.push(Turn::new(format!("s{index}"), Role::System, "", event.at_ms, vec![Block::Status(text.to_owned())]));
        match role {
            "note" => {
                stretch = None;
                let mut turn = Turn::new(format!("u{index}"), Role::User, "you", event.at_ms, vec![Block::Text(text.to_owned())]);
                turn.kind = TurnKind::Note;
                out.push(turn);
            }
            "user" | "system" => {
                stretch = None;
                status(&mut out);
            }
            _ => {
                if echoes.iter().any(|e| text.starts_with(e.as_str()) || e.starts_with(&head(text, 160))) {
                    continue;
                }
                if text.starts_with("Working in ") || text == "Plan ready." || text.starts_with("Investigation complete.") {
                    stretch = None;
                    status(&mut out);
                    continue;
                }
                put(&mut out, &mut stretch, Block::Text(text.to_owned()));
            }
        }
    }
    out
}

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

fn base(path: &str) -> &str {
    path.rsplit('/').find(|s| !s.is_empty()).unwrap_or(path)
}

/// A tool call part (`dynamic-tool`) as a card: the web's `toolText`, `toolDetail` and `toolFailed`.
fn tool_of(part: &Value) -> Tool {
    let name = str_of(part, "toolName");
    let lower = name.to_lowercase();
    let input = &part["input"];
    let output = &part["output"];
    let kind = match lower.as_str() {
        "bash" | "shell" => Kind::Command,
        "edit" | "multiedit" | "write" | "notebookedit" | "apply_patch" | "patch" => Kind::Edit,
        "read" | "ls" | "notebookread" => Kind::Read,
        "grep" | "glob" | "websearch" | "webfetch" | "search" => Kind::Search,
        _ => Kind::Other,
    };
    let path = ["file_path", "path", "notebook_path"].iter().map(|k| str_of(input, k)).find(|p| !p.is_empty()).unwrap_or("");
    let title = match kind {
        Kind::Command => format!("Ran `{}`", Some(short_command(str_of(input, "command"))).filter(|c| !c.is_empty()).unwrap_or_else(|| "a command".into())),
        Kind::Edit => format!("Edited {}", if path.is_empty() { input["changes"].as_array().map_or("files".to_owned(), |c| format!("{} files", c.len())) } else { base(path).to_owned() }),
        Kind::Read => format!("Read {}", if path.is_empty() { "a file" } else { base(path) }),
        Kind::Search => {
            let what = ["pattern", "query", "url"].iter().map(|k| str_of(input, k)).find(|p| !p.is_empty());
            format!("Searched {}", what.map_or("the code".to_owned(), |w| format!("“{}”", w.chars().take(60).collect::<String>())))
        }
        Kind::Other => {
            let title = str_of(part, "title");
            if !title.is_empty() && title != name { title.to_owned() } else { name.to_owned() }
        }
    };
    let state_word = str_of(part, "state");
    let exit = output["exit_code"].as_i64();
    let failed = state_word == "output-error" || (state_word == "output-available" && exit.is_some_and(|c| c != 0));
    let approved = part["approval"]["approved"].as_bool();
    let state = if failed {
        ToolState::Failed
    } else if state_word == "approval-requested" {
        ToolState::Waiting
    } else if state_word == "output-denied" || (state_word == "approval-responded" && approved == Some(false)) {
        ToolState::Declined
    } else if matches!(state_word, "input-streaming" | "input-available" | "approval-responded") {
        ToolState::Running
    } else {
        ToolState::Done
    };
    let mut lines = Vec::new();
    if kind == Kind::Command {
        if !str_of(input, "command").is_empty() {
            lines.push(format!("$ {}", str_of(input, "command")));
        }
        if let Some(o) = output["output"].as_str() {
            lines.push(if o.trim().is_empty() { "No output.".to_owned() } else { o.trim().to_owned() });
        }
        if let Some(code) = exit.filter(|c| *c != 0) {
            lines.push(format!("exit {code}"));
        }
    } else {
        if input.as_object().is_some_and(|o| !o.is_empty()) {
            lines.push(serde_json::to_string_pretty(input).unwrap_or_default());
        }
        if let Some(o) = output["output"].as_str().filter(|o| !o.trim().is_empty()) {
            lines.push(o.trim().to_owned());
        }
        if let Some(e) = part["errorText"].as_str().filter(|e| !e.is_empty()) {
            lines.push(e.to_owned());
        }
    }
    let detail = Some(lines.join("\n\n")).filter(|d| !d.trim().is_empty());
    let edit = (kind == Kind::Edit)
        .then(|| match (input["old_string"].as_str(), input["new_string"].as_str(), input["content"].as_str()) {
            (Some(old), Some(new), _) => Some(Edit { path: path.to_owned(), old: old.to_owned(), new: new.to_owned() }),
            (None, _, Some(content)) => Some(Edit { path: path.to_owned(), old: String::new(), new: content.to_owned() }),
            _ => None,
        })
        .flatten()
        .filter(|e| !e.path.is_empty() && e.old.len() + e.new.len() <= 64 * 1024);
    let approval = (state == ToolState::Waiting).then(|| part["approval"]["id"].as_str().map(str::to_owned).unwrap_or_else(|| str_of(part, "toolCallId").to_owned()));
    let approval_note = (state == ToolState::Waiting).then(|| {
        let note = if name == ACTION_TOOL {
            format!("{} · {}", str_of(input, "kind"), str_of(input, "text"))
        } else {
            serde_json::to_string(input).unwrap_or_default()
        };
        note.chars().take(300).collect::<String>()
    });
    Tool { id: str_of(part, "toolCallId").to_owned(), title, kind, state, detail, edit, approval, name: name.to_owned(), approval_note }
}

/// The blocks of a chat reply's parts, in the order they happened.
pub fn blocks_of_parts(parts: &[Value]) -> Vec<Block> {
    let mut out = Vec::new();
    for part in parts {
        match str_of(part, "type") {
            "text" => {
                let text = str_of(part, "text");
                if !text.is_empty() {
                    out.push(Block::Text(text.to_owned()));
                }
            }
            "dynamic-tool" => out.push(Block::Tool(tool_of(part))),
            "data-neko-choices" => {
                let data = &part["data"];
                let mut text = String::new();
                if let Some(q) = data["question"].as_str() {
                    text.push_str(q);
                    text.push('\n');
                }
                for option in data["options"].as_array().into_iter().flatten() {
                    text.push_str(&format!("\n- {}{}", str_of(option, "label"), option["detail"].as_str().map(|d| format!(" — {d}")).unwrap_or_default()));
                }
                if !text.trim().is_empty() {
                    out.push(Block::Text(text));
                }
            }
            "data-neko-status" => {
                if let Some(t) = part["data"]["text"].as_str() {
                    out.push(Block::Status(t.to_owned()));
                }
            }
            _ => {}
        }
    }
    out
}

/// Neko's own conversation as turns.
pub fn chat_turns(messages: &[ChatMessage]) -> Vec<Turn> {
    messages
        .iter()
        .map(|m| {
            let (role, author) = if m.role == ChatRole::User { (Role::User, "you") } else { (Role::Agent, "Neko") };
            let mut turn = Turn::new(m.id.clone(), role, author, m.at_ms, blocks_of_parts(&m.parts));
            turn.pending = m.pending;
            turn.failed = m.failed;
            turn.queued = m.queued;
            turn.tickets = m.ticket_ids.clone();
            turn
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::workbench::TaskStatus;
    use serde_json::json;

    fn task(events: Vec<(&str, &str)>) -> Task {
        let mut t: Task = serde_json::from_value(json!({
            "id": "t1", "workspace_id": "w", "issue_id": null, "title": "T", "goal": "Fix the login", "status": "Building",
            "plan": "", "result": "", "worktree": null, "events": [], "created_at_ms": 10, "updated_at_ms": 10,
        }))
        .unwrap();
        t.status = TaskStatus::Building;
        t.events = events.into_iter().enumerate().map(|(i, (role, message))| TaskEvent { at_ms: 100 + i as i64, role: role.into(), message: message.into() }).collect();
        t
    }

    #[test]
    fn a_task_reads_as_a_conversation_between_you_and_its_agents() {
        // The same case as the daemon's `task_chat` test, so the two projections agree.
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
        let turns = task_turns(&t);
        let shape: Vec<_> = turns.iter().map(|m| (m.role, m.blocks.len())).collect();
        assert_eq!(shape, [(Role::User, 1), (Role::Agent, 1), (Role::Agent, 2), (Role::User, 1), (Role::System, 1), (Role::Agent, 1)]);
        assert_eq!(turns[1].author, "Planner");
        let Block::Tool(tool) = &turns[2].blocks[0] else { panic!("a command card") };
        assert_eq!((tool.title.as_str(), tool.state), ("Ran `cargo test`", ToolState::Done));
        assert!(tool.detail.as_deref().unwrap().starts_with("$ cargo test"));
        assert_eq!(turns[2].blocks[1], Block::Text("Changed the check.".into()));
        assert_eq!(turns[3].kind, TurnKind::Note);
    }

    #[test]
    fn a_chat_opened_task_keeps_the_persons_words_under_its_goal() {
        let mut t = task(vec![]);
        assert_eq!(task_turns(&t)[0].request, None);
        t.request = Some("the login button does nothing on Safari".into());
        let brief = &task_turns(&t)[0];
        assert_eq!((brief.kind, brief.request.as_deref()), (TurnKind::Brief, Some("the login button does nothing on Safari")));
    }

    #[test]
    fn a_pasted_receipt_prefix_is_a_message_and_a_failed_command_says_so() {
        let pasted = format!("{RECEIPT_PREFIX}{{\"command\":\"rm -rf\"}}");
        let t = task(vec![("note", &pasted), ("reviewer", &format!("{RECEIPT_PREFIX}{{\"command\":\"x\",\"output\":\"\",\"exit_code\":2}}"))]);
        let turns = task_turns(&t);
        assert!(matches!(&turns[1].blocks[0], Block::Text(_)), "a note is never execution evidence");
        let Block::Tool(tool) = &turns[2].blocks[0] else { panic!() };
        assert_eq!(tool.state, ToolState::Failed);
        assert!(tool.detail.as_deref().unwrap().contains("exit 2"));
    }

    #[test]
    fn the_plan_and_result_are_not_said_twice() {
        let mut t = task(vec![("scout", "Plan: change the guard in login.rs and add a test"), ("scout", "Plan ready.")]);
        t.plan = "Plan: change the guard in login.rs and add a test".into();
        let turns = task_turns(&t);
        assert_eq!(turns.len(), 2, "the brief and the 'Plan ready.' status only");
        assert_eq!(turns[1].role, Role::System);
    }

    #[test]
    fn commands_read_short() {
        assert_eq!(short_command("/bin/zsh -lc 'rg -n foo'"), "rg -n foo");
        assert_eq!(short_command("cd /x/neko/task-worktrees/abc && cargo test"), "cargo test");
        assert_eq!(short_command("cd /home/me && ls"), "cd /home/me && ls");
        assert_eq!(short_command("a\nb"), "a");
    }

    #[test]
    fn chat_parts_become_prose_and_tool_cards_with_their_state() {
        let parts = vec![
            json!({"type": "step-start"}),
            json!({"type": "text", "text": "Checking.", "state": "done"}),
            json!({"type": "dynamic-tool", "toolName": "Bash", "toolCallId": "h1", "state": "output-available", "input": {"command": "gh pr list"}, "output": {"exit_code": 0, "output": "5 open"}}),
            json!({"type": "dynamic-tool", "toolName": "Bash", "toolCallId": "h4", "state": "output-available", "input": {"command": "git log"}, "output": {"exit_code": 128, "output": "fatal"}}),
            json!({"type": "dynamic-tool", "toolName": "Edit", "toolCallId": "e1", "state": "output-available", "input": {"file_path": "/x/src/export.sh", "old_string": "a", "new_string": "b"}, "output": {}}),
            json!({"type": "dynamic-tool", "toolName": "save_issue", "toolCallId": "call-1", "state": "approval-requested", "input": {"title": "x"}, "approval": {"id": "call-1"}}),
            json!({"type": "dynamic-tool", "toolName": "Bash", "toolCallId": "h9", "state": "input-available", "input": {"command": "sleep 5"}}),
        ];
        let blocks = blocks_of_parts(&parts);
        assert_eq!(blocks.len(), 6);
        let tools: Vec<&Tool> = blocks.iter().filter_map(|b| if let Block::Tool(t) = b { Some(t) } else { None }).collect();
        assert_eq!(tools.iter().map(|t| t.state).collect::<Vec<_>>(), [ToolState::Done, ToolState::Failed, ToolState::Done, ToolState::Waiting, ToolState::Running]);
        assert_eq!(tools[2].title, "Edited export.sh");
        assert_eq!(tools[2].edit.as_ref().map(|e| (e.old.as_str(), e.new.as_str())), Some(("a", "b")));
        assert_eq!(tools[3].approval.as_deref(), Some("call-1"));
        assert_eq!(tools[3].name, "save_issue");
        assert!(tools[3].approval_note.as_deref().unwrap().contains("title"), "what a click would approve is shown");
        assert!(tools[1].detail.as_deref().unwrap().ends_with("exit 128"));
    }
}
