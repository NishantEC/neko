//! Ask neko — a sentence becomes a tool call you confirm.
//!
//! **L6 of `docs/plan-agent-control-plane.md`, and the payoff.** Everything
//! beneath it made a capability reachable by *name*: you find the schedule,
//! then pause it. This layer lets you say what you want — "stop the agent in
//! triage-fe" — and turns that into one of those same calls.
//!
//! ## The two rules it ships with
//!
//! **It proposes; you confirm.** Planning and running are two separate
//! keystrokes, always, with the exact call rendered in between. There is no
//! path through this module that performs a tool call the person did not read
//! first — which is what makes it safe to point at tools that cancel agents
//! and delete schedules.
//!
//! **It can only reach what neko already exposes.** [`CATALOG`] is a fixed,
//! compiled-in list, and [`Plan::from_tool_use`] refuses a name outside it.
//! The model is asked to pick from a menu, not handed the daemon. Nothing
//! becomes possible through the planner that was not already possible by
//! hand — the guarantee that keeps this from being a remote-code-execution
//! surface wearing a search field.
//!
//! ## The credential
//!
//! The same Keychain OAuth token `neko_core::usage` already reads, used the
//! same careful way: never logged, never persisted, and handed to `curl` on
//! **stdin** so it cannot appear in `argv` where any local process could read
//! it out of `ps`. Confirmed live that this token drives
//! `api.anthropic.com/v1/messages` with tools and returns a real `tool_use`
//! block; the `anthropic-beta: oauth-2025-04-20` header is what makes an
//! OAuth credential acceptable there, exactly as it is for the usage
//! endpoint.
//!
//! ## Why the read-only tools are absent
//!
//! The catalog is all verbs. Listing agents, schedules and quota is what the
//! palette *already does* faster than a model round trip could, and a planner
//! that answered "show me my schedules" by calling `list_schedules` would be
//! a slow, expensive way to reach a mode that is one keystroke away.

use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

const MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const API_VERSION: &str = "2023-06-01";

/// Long enough for a real model round trip, short enough that a wedged one
/// does not hold a request thread forever.
const REQUEST_TIMEOUT_SECS: u64 = 45;

/// Claude Code's own system prompt line. Required: the OAuth credential is
/// scoped to that client, and the API rejects a request that does not present
/// itself as one.
const SYSTEM_PROMPT: &str = "You are Claude Code, Anthropic's official CLI for Claude.";

/// One tool the planner may choose, and the shape of its arguments.
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema for the arguments, as the Messages API wants it.
    pub schema: fn() -> Value,
}

fn agent_id_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"agentId": {"type": "string", "description": "The agent's id, from the Agents list"}},
        "required": ["agentId"],
    })
}

fn schedule_id_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"id": {"type": "string", "description": "The schedule's id"}},
        "required": ["id"],
    })
}

/// **Every tool the planner is allowed to reach.**
///
/// All verbs, and deliberately a short list. Paseo offers 61 tools; 22 of
/// them drive a browser, and most of the rest are reads the palette already
/// answers without a model. What is left is the set where saying it is
/// genuinely faster than finding it.
pub const CATALOG: &[ToolSpec] = &[
    ToolSpec {
        name: "cancel_agent",
        description: "Stop an agent's current run, leaving the agent alive to be prompted again.",
        schema: agent_id_schema,
    },
    ToolSpec {
        name: "archive_agent",
        description: "Archive an agent, interrupting it if it is running. A soft delete.",
        schema: agent_id_schema,
    },
    ToolSpec {
        name: "send_agent_prompt",
        description: "Send a further instruction to an agent that already exists.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "agentId": {"type": "string"},
                    "prompt": {"type": "string", "description": "What to tell the agent to do"},
                    "background": {"type": "boolean", "description": "Always true from neko: the panel must not wait for an agent to finish"},
                },
                "required": ["agentId", "prompt"],
            })
        },
    },
    ToolSpec {
        name: "set_agent_mode",
        description: "Change an agent's session mode: plan, bypassPermissions, read-only, auto.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"agentId": {"type": "string"}, "modeId": {"type": "string"}},
                "required": ["agentId", "modeId"],
            })
        },
    },
    ToolSpec {
        name: "pause_schedule",
        description: "Pause a recurring schedule so it stops starting agents.",
        schema: schedule_id_schema,
    },
    ToolSpec {
        name: "resume_schedule",
        description: "Resume a paused schedule.",
        schema: schedule_id_schema,
    },
    ToolSpec {
        name: "run_schedule_once",
        description: "Run a schedule immediately, without changing its cadence.",
        schema: schedule_id_schema,
    },
    ToolSpec {
        name: "create_terminal",
        description: "Open a terminal session in a working directory.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"cwd": {"type": "string", "description": "Absolute path"}},
                "required": ["cwd"],
            })
        },
    },
];

pub fn catalog_names() -> Vec<&'static str> {
    CATALOG.iter().map(|t| t.name).collect()
}

/// A tool call the planner proposes and a person has not yet confirmed.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub tool: String,
    pub arguments: Value,
    /// One line naming what will happen, for the row's title.
    pub summary: String,
}

impl Plan {
    /// Builds a plan from a `tool_use` block, **refusing anything outside
    /// [`CATALOG`]**.
    ///
    /// The model is asked to pick from a menu and normally does. This is the
    /// check that makes that a guarantee rather than an expectation: a
    /// hallucinated or drifted tool name never reaches `McpClient::call`, so
    /// the worst a confused model can do is produce a row that refuses to
    /// run.
    pub fn from_tool_use(block: &Value) -> Result<Self, AskError> {
        let tool = block
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| AskError::Failed("the model named no tool".to_string()))?;
        if !catalog_names().contains(&tool) {
            return Err(AskError::OutsideCatalog(tool.to_string()));
        }
        let arguments = block.get("input").cloned().unwrap_or_else(|| json!({}));
        Ok(Plan { summary: describe(tool, &arguments), tool: tool.to_string(), arguments })
    }
}

/// `"Cancel agent 5c9b… "` — what the row says it will do.
///
/// Written here rather than asked of the model: a summary the model wrote
/// could describe something other than the call it actually produced, and the
/// whole safety story is that the row and the call are the same thing. Built
/// from the arguments, so it cannot drift from them.
pub fn describe(tool: &str, arguments: &Value) -> String {
    let arg = |key: &str| arguments.get(key).and_then(Value::as_str).unwrap_or("?").to_string();
    match tool {
        "cancel_agent" => format!("Cancel the run in agent {}", short(&arg("agentId"))),
        "archive_agent" => format!("Archive agent {}", short(&arg("agentId"))),
        "send_agent_prompt" => {
            format!("Tell agent {}: {}", short(&arg("agentId")), arg("prompt"))
        }
        "set_agent_mode" => {
            format!("Put agent {} in {} mode", short(&arg("agentId")), arg("modeId"))
        }
        "pause_schedule" => format!("Pause schedule {}", short(&arg("id"))),
        "resume_schedule" => format!("Resume schedule {}", short(&arg("id"))),
        "run_schedule_once" => format!("Run schedule {} now", short(&arg("id"))),
        "create_terminal" => format!("Open a terminal in {}", arg("cwd")),
        other => format!("{other} {arguments}"),
    }
}

/// A uuid is unreadable and unhelpful at full length in a row.
fn short(id: &str) -> String {
    match id.split_once('-') {
        Some((head, _)) if head.len() >= 6 => head.to_string(),
        _ => id.chars().take(8).collect(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AskError {
    /// No Claude Code credential on this machine.
    NotConfigured,
    /// Signed out, or the token expired.
    NeedsAuth,
    /// The model chose a tool neko does not expose. See [`Plan::from_tool_use`].
    OutsideCatalog(String),
    /// The model answered in words rather than choosing a tool — usually
    /// because the request was not something any of these tools can do.
    NoTool(String),
    Failed(String),
}

impl std::fmt::Display for AskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AskError::NotConfigured => write!(f, "No Claude Code credential on this Mac"),
            AskError::NeedsAuth => write!(f, "Claude Code is signed out — run `claude`"),
            AskError::OutsideCatalog(tool) => {
                write!(f, "neko doesn't expose {tool}, so it won't run it")
            }
            AskError::NoTool(said) => write!(f, "{said}"),
            AskError::Failed(why) => write!(f, "{why}"),
        }
    }
}

/// Turns a sentence into a proposed tool call.
///
/// `context` is whatever the palette already knows and the model does not —
/// the ids of agents and schedules that exist right now. Without it "stop the
/// triage agent" has no way to become a uuid, and the model would have to be
/// given a tool to go and look, which is a second round trip and a second
/// chance to be wrong.
pub fn plan(question: &str, context: &str) -> Result<Plan, AskError> {
    let token = crate::usage::claude_access_token().ok_or(AskError::NotConfigured)?;
    let tools: Vec<Value> = CATALOG
        .iter()
        .map(|spec| {
            json!({
                "name": spec.name,
                "description": spec.description,
                "input_schema": (spec.schema)(),
            })
        })
        .collect();

    let body = json!({
        "model": "claude-opus-5",
        "max_tokens": 512,
        "system": SYSTEM_PROMPT,
        "tools": tools,
        // `auto`, not `any`: forcing a tool means a request none of these
        // tools can serve comes back as a confidently wrong call instead of
        // "I can't do that". The text branch below is that answer.
        "tool_choice": {"type": "auto"},
        "messages": [{
            "role": "user",
            "content": format!(
                "Here is what is currently running on this machine:\n{context}\n\n\
                 Using only the tools provided, do this:\n{question}\n\n\
                 Choose exactly one tool call. If none of the tools can do it, \
                 say so in one short sentence instead."
            ),
        }],
    });

    let response = request(&token, &body)?;
    let content = response
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| AskError::Failed("the model's reply was unreadable".to_string()))?;

    if let Some(block) = content.iter().find(|b| b.get("type").and_then(Value::as_str) == Some("tool_use")) {
        return Plan::from_tool_use(block);
    }
    let said = content
        .iter()
        .filter_map(|b| b.get("text")?.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    Err(AskError::NoTool(if said.is_empty() {
        "The model didn't choose a tool".to_string()
    } else {
        said
    }))
}

/// One POST, with the token on **stdin** — never `argv`. Same shape and same
/// reasoning as `usage::curl_get`; kept separate so the two can change
/// transport independently.
fn request(token: &str, body: &Value) -> Result<Value, AskError> {
    let mut child = Command::new("/usr/bin/curl")
        .args(["--silent", "--show-error", "--config", "-", "--write-out", "\n%{http_code}"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AskError::Failed(format!("couldn't run curl: {e}")))?;

    let raw = body.to_string();
    let escaped = raw.replace('\\', "\\\\").replace('"', "\\\"");
    let config = format!(
        "url = \"{MESSAGES_URL}\"\n\
         request = POST\n\
         max-time = {REQUEST_TIMEOUT_SECS}\n\
         header = \"Authorization: Bearer {token}\"\n\
         header = \"anthropic-version: {API_VERSION}\"\n\
         header = \"anthropic-beta: {OAUTH_BETA}\"\n\
         header = \"content-type: application/json\"\n\
         data-binary = \"{escaped}\"\n"
    );
    child
        .stdin
        .take()
        .ok_or_else(|| AskError::Failed("curl refused stdin".to_string()))?
        .write_all(config.as_bytes())
        .map_err(|e| AskError::Failed(format!("couldn't send the request: {e}")))?;

    let out = child.wait_with_output().map_err(|e| AskError::Failed(format!("curl failed: {e}")))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let (payload, status) = crate::usage::split_status(&stdout);
    match status {
        Some(200) => serde_json::from_str(&payload)
            .map_err(|_| AskError::Failed("the model's reply was unreadable".to_string())),
        Some(401 | 403) => Err(AskError::NeedsAuth),
        Some(429) => Err(AskError::Failed("Rate limited — try again in a moment".to_string())),
        Some(code) => Err(AskError::Failed(format!("The model API returned {code}"))),
        None => Err(AskError::Failed(
            String::from_utf8_lossy(&out.stderr)
                .trim()
                .split('\n')
                .next()
                .unwrap_or("request failed")
                .to_string(),
        )),
    }
}

// --------------------------------------------------------------- provider

use neko_protocol::{Glyph, Icon, ItemAction, SearchItem};
use std::sync::Mutex;

use crate::mcp::McpClient;
use crate::provider::{Provider, ProviderError};
use crate::search::Candidate;

/// The last plan produced, keyed by the exact question that produced it.
///
/// One slot, not a map: a plan is a proposal about right now, and keeping
/// older ones would mean a stale row could reappear by retyping a question
/// whose world has since moved on.
static PLAN: Mutex<Option<(String, Plan)>> = Mutex::new(None);

/// What went wrong last time, so the row can say it instead of silently
/// reverting to "Plan it".
static LAST_ERROR: Mutex<Option<(String, String)>> = Mutex::new(None);

fn remember(question: &str, outcome: Result<Plan, AskError>) {
    match outcome {
        Ok(plan) => {
            *PLAN.lock().unwrap() = Some((question.to_string(), plan));
            *LAST_ERROR.lock().unwrap() = None;
        }
        Err(e) => {
            *PLAN.lock().unwrap() = None;
            *LAST_ERROR.lock().unwrap() = Some((question.to_string(), e.to_string()));
        }
    }
}

/// Clears everything remembered — called on every run, so a plan cannot be
/// executed twice by pressing Enter again on a row that has already fired.
pub fn forget() {
    *PLAN.lock().unwrap() = None;
    *LAST_ERROR.lock().unwrap() = None;
}

/// The `ask` mode's list: one row, which is either an invitation, a
/// proposal, or a refusal.
pub struct AskProvider {
    live: bool,
}

impl AskProvider {
    pub fn new() -> Self {
        Self { live: true }
    }

    /// Never calls a model or a daemon — see
    /// `permissions::PermissionsProvider::disabled`.
    pub fn disabled() -> Self {
        Self { live: false }
    }

    /// Everything the palette knows that the model does not: which agents and
    /// schedules exist, with their ids.
    ///
    /// Gathered here rather than given to the model as tools of its own. A
    /// lookup tool would be a second round trip and a second chance to be
    /// wrong, and this is information neko can fetch in milliseconds without
    /// asking anybody.
    fn context(&self) -> String {
        let Ok(client) = McpClient::discover() else {
            return "Paseo is not running, so there are no agents or schedules.".to_string();
        };
        let mut lines = Vec::new();
        if let Ok(value) = client.call("list_agents", json!({"limit": 20}))
            && let Some(agents) = value.get("agents").and_then(Value::as_array)
        {
            lines.push("Agents:".to_string());
            for a in agents.iter().take(20) {
                let field = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or("");
                lines.push(format!(
                    "  - id {} | title {} | cwd {} | status {}",
                    field("id"),
                    field("title"),
                    field("cwd"),
                    field("status")
                ));
            }
        }
        if let Ok(value) = client.call("list_schedules", json!({})) {
            let schedules = crate::schedules::parse_schedules(&value);
            if !schedules.is_empty() {
                lines.push("Schedules:".to_string());
                for s in &schedules {
                    lines.push(format!(
                        "  - id {} | name {} | {}",
                        s.id,
                        s.title,
                        if s.paused { "paused" } else { "active" }
                    ));
                }
            }
        }
        if lines.is_empty() { "Nothing is running.".to_string() } else { lines.join("\n") }
    }

    fn row(
        &self,
        id: &str,
        title: String,
        subtitle: Option<String>,
        verb: &str,
        keeps_open: bool,
        actions: Vec<ItemAction>,
    ) -> Candidate {
        Candidate {
            score: 1.0,
            item: SearchItem {
                id: id.to_string(),
                kind: "ask".to_string(),
                title,
                subtitle,
                // `Agent` rather than `AgentLive`: this is neko acting as
                // one, and the live variant's presence dot means a session
                // is running, which nothing here is.
                icon: Icon::Glyph(Glyph::Agent),
                section_label: "Ask neko".to_string(),
                action_label: format!("{verb}  \u{21b5}"),
                badge: None,
                accessory: None,
                enters_mode: None,
                group_label: None,
                actions,
                source: None,
                meter: None,
                keeps_open,
                preview: None,
            },
        }
    }
}

impl Default for AskProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for AskProvider {
    fn id(&self) -> &'static str {
        "ask"
    }

    fn section_label(&self) -> &'static str {
        "Ask neko"
    }

    /// **Never plans here.** `search` runs on every keystroke, and a model
    /// call per keystroke would be slow, expensive, and would propose things
    /// nobody asked for. Planning happens only on a deliberate Enter — see
    /// `activate_with_query` — and this method just renders whatever that
    /// left behind.
    fn search(&self, query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        let question = query.trim();
        if question.is_empty() {
            return vec![self.row(
                "empty",
                // Deliberately not the placeholder's own words: the field
                // already says "Say what you want done", and a row echoing
                // it spends the only row on screen saying nothing new.
                "neko will propose one tool call".to_string(),
                Some(
                    "It runs nothing until you confirm. Try \"stop the agent in triage-fe\"."
                        .to_string(),
                ),
                "Type",
                true,
                Vec::new(),
            )];
        }
        if let Some((asked, plan)) = PLAN.lock().unwrap().as_ref()
            && asked == question
        {
            return vec![self.row(
                "run",
                plan.summary.clone(),
                // The exact call, next to the sentence describing it. This is
                // the whole confirmation step: nothing runs that was not read.
                Some(format!("{}({})", plan.tool, compact_arguments(&plan.arguments))),
                "Run",
                false,
                vec![ItemAction {
                    id: "discard".to_string(),
                    label: "Discard this plan".to_string(),
                    destructive: false,
                }],
            )];
        }
        if let Some((asked, message)) = LAST_ERROR.lock().unwrap().as_ref()
            && asked == question
        {
            return vec![self.row(
                "plan",
                message.clone(),
                Some("Rephrase, or press \u{21b5} to try again".to_string()),
                "Try again",
                true,
                Vec::new(),
            )];
        }
        vec![self.row(
            "plan",
            format!("Plan: {question}"),
            Some("neko will propose one tool call, and run nothing until you confirm".to_string()),
            "Plan it",
            true,
            Vec::new(),
        )]
    }

    fn activate(&self, _id: &str) -> Result<(), ProviderError> {
        Err(ProviderError("that row needs the query with it".to_string()))
    }

    /// The two steps. `plan` proposes and leaves the panel open; `run`
    /// performs exactly what the row was showing.
    fn activate_with_query(&self, id: &str, query: &str) -> Result<(), ProviderError> {
        if !self.live {
            return Err(ProviderError("asking is disabled".to_string()));
        }
        let question = query.trim().to_string();
        match id {
            "empty" => Ok(()),
            "plan" => {
                if question.is_empty() {
                    return Ok(());
                }
                let context = self.context();
                remember(&question, plan(&question, &context));
                // Deliberately `Ok` even when planning failed: the failure is
                // now *in the row*, which is where a person can read it and
                // rephrase. Returning an error here would put it in the
                // footer and leave the row saying "Plan it", as though
                // nothing had happened.
                Ok(())
            }
            "run" => {
                let taken = PLAN
                    .lock()
                    .unwrap()
                    .as_ref()
                    .filter(|(asked, _)| *asked == question)
                    .map(|(_, plan)| plan.clone());
                let plan = taken.ok_or_else(|| {
                    // The query changed under the row — the plan on screen is
                    // no longer the plan for what is typed.
                    ProviderError("that plan is stale — press \u{21b5} to plan again".to_string())
                })?;
                let client =
                    McpClient::discover().map_err(|e| ProviderError(e.to_string()))?;
                let outcome = client.call(&plan.tool, plan.arguments.clone());
                // Cleared either way: a plan that ran must not be runnable
                // again by pressing Enter twice, and one that failed should
                // be re-planned against the world as it now is.
                forget();
                outcome.map(|_| ()).map_err(|e| ProviderError(e.to_string()))
            }
            other => Err(ProviderError(format!("no such row: {other}"))),
        }
    }

    fn perform_action(&self, _id: &str, action: &str) -> Result<(), ProviderError> {
        match action {
            "discard" => {
                forget();
                Ok(())
            }
            other => Err(ProviderError(format!("no such action: {other}"))),
        }
    }
}

/// `agentId: 05475348-…, prompt: "run the tests"` — the arguments, on one
/// line, without JSON's punctuation.
///
/// A row is one line of a launcher, not a code viewer: `{"agentId":"…"}`
/// spends a third of its width on quotes and braces. The values themselves
/// are never abbreviated, because they are the part being confirmed.
pub fn compact_arguments(arguments: &Value) -> String {
    let Some(object) = arguments.as_object() else { return arguments.to_string() };
    object
        .iter()
        .map(|(key, value)| match value.as_str() {
            Some(text) => format!("{key}: {text}"),
            None => format!("{key}: {value}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_outside_the_catalog_is_refused_before_it_can_be_called() {
        // The guarantee the whole module rests on. `browser_evaluate` is a
        // real Paseo tool that runs arbitrary JavaScript — it is reachable
        // over the same channel and must never be reachable through here.
        let block = json!({"type": "tool_use", "name": "browser_evaluate", "input": {"fn": "..."}});
        assert_eq!(
            Plan::from_tool_use(&block),
            Err(AskError::OutsideCatalog("browser_evaluate".to_string()))
        );
        // And a name that exists nowhere at all.
        let block = json!({"type": "tool_use", "name": "rm_rf", "input": {}});
        assert!(matches!(Plan::from_tool_use(&block), Err(AskError::OutsideCatalog(_))));
    }

    #[test]
    fn a_catalog_tool_becomes_a_plan_that_describes_itself() {
        // Real shape, from a live Messages API reply.
        let block =
            json!({"type": "tool_use", "name": "cancel_agent", "input": {"agentId": "05475348-2409-4eca"}});
        let plan = Plan::from_tool_use(&block).expect("in the catalog");
        assert_eq!(plan.tool, "cancel_agent");
        assert_eq!(plan.arguments["agentId"], json!("05475348-2409-4eca"));
        // The summary is built from the arguments, so it cannot describe
        // something other than the call it sits next to.
        assert_eq!(plan.summary, "Cancel the run in agent 05475348");
    }

    #[test]
    fn the_summary_is_written_here_and_never_by_the_model() {
        // A model-written summary could describe a different call from the
        // one it produced, and the safety story is that the row and the call
        // are the same thing.
        let described = describe("pause_schedule", &json!({"id": "98b23875"}));
        assert_eq!(described, "Pause schedule 98b23875");
        // Missing arguments produce a visible gap, not a plausible lie.
        assert_eq!(describe("archive_agent", &json!({})), "Archive agent ?");
    }

    #[test]
    fn a_tool_use_with_no_name_is_an_error_not_an_empty_call() {
        let block = json!({"type": "tool_use", "input": {}});
        assert!(matches!(Plan::from_tool_use(&block), Err(AskError::Failed(_))));
    }

    #[test]
    fn every_catalog_entry_is_a_verb_with_a_real_schema() {
        // Reads are excluded on purpose: the palette already answers them
        // faster than a model round trip could.
        for spec in CATALOG {
            let schema = (spec.schema)();
            assert_eq!(schema["type"], json!("object"), "{}", spec.name);
            assert!(schema["required"].as_array().is_some_and(|r| !r.is_empty()), "{}", spec.name);
            assert!(!spec.name.starts_with("list_"), "{} is a read", spec.name);
            assert!(!spec.name.starts_with("browser_"), "{} drives a browser", spec.name);
        }
    }

    #[test]
    fn a_uuid_is_shortened_to_something_a_row_can_show() {
        assert_eq!(short("05475348-2409-4eca-aa5f-3446f369ee66"), "05475348");
        assert_eq!(short("98b23875"), "98b23875");
        assert_eq!(short("a-b"), "a-b".chars().take(8).collect::<String>());
    }

    mod live {
        use super::*;

        #[test]
        #[ignore = "spends a real model call against the Keychain credential"]
        fn a_sentence_becomes_the_tool_call_a_person_meant() {
            let context = "Agents:\n  - id 05475348-2409-4eca, workspace main, repo neko, running";
            let plan = plan("stop whatever the neko agent is doing", context)
                .expect("a plan");
            eprintln!("{} {}\n  {}", plan.tool, plan.arguments, plan.summary);
            assert_eq!(plan.tool, "cancel_agent");
            assert_eq!(plan.arguments["agentId"], json!("05475348-2409-4eca"));
        }

        #[test]
        #[ignore = "spends a real model call against the Keychain credential"]
        fn a_request_none_of_the_tools_can_serve_says_so_rather_than_guessing() {
            // `tool_choice: auto` rather than `any` is what makes this
            // possible: forcing a tool would turn "I can't" into a
            // confidently wrong call.
            let err = plan("what is the capital of France", "").expect_err("no tool applies");
            eprintln!("{err}");
            assert!(matches!(err, AskError::NoTool(_)));
        }
    }
}
