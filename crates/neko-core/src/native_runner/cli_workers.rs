//! Claude Code and OpenCode as Neko workers, each using its own login.
//!
//! Neither CLI has an OS sandbox like Codex's, so Neko supplies one: every
//! run executes under macOS `sandbox-exec` with a Neko-written profile that
//! denies file writes everywhere except the task worktree (writable runs
//! only), the CLI's own state and cache folders, and a private temp folder.
//! The CLI's tool allowlist adds a second layer: read-only roles get no edit
//! tools, extraction gets no tools, and web tools are never enabled.
//!
//! Known difference from Codex: shell commands these agents run may reach
//! the network, because the CLI itself must, and Seatbelt can't tell them
//! apart by host. File writes are still confined.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Agent {
    Claude,
    OpenCode,
}

impl Agent {
    pub(crate) fn for_provider(provider: &str) -> Option<Self> {
        match provider {
            "claude" => Some(Self::Claude),
            "opencode" => Some(Self::OpenCode),
            _ => None,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::OpenCode => "OpenCode",
        }
    }
    fn binary(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::OpenCode => "opencode",
        }
    }
    fn override_env(self) -> &'static str {
        match self {
            Self::Claude => "NEKO_CLAUDE_PATH",
            Self::OpenCode => "NEKO_OPENCODE_PATH",
        }
    }
}

/// Every directory a GUI-launched daemon should search, including npm/nvm.
pub(crate) fn agent_directories() -> Vec<PathBuf> {
    let mut directories = executable_directories();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        directories.push(home.join(".claude/local"));
        directories.push(home.join(".opencode/bin"));
        if let Ok(entries) = fs::read_dir(home.join(".nvm/versions/node")) {
            let mut nodes: Vec<PathBuf> = entries.flatten().map(|e| e.path().join("bin")).collect();
            nodes.sort();
            nodes.reverse();
            directories.extend(nodes);
        }
    }
    directories
}

pub(crate) fn resolve(agent: Agent) -> Result<PathBuf, String> {
    let executable = |path: &PathBuf| path.is_absolute() && fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    if let Some(path) = std::env::var_os(agent.override_env()).map(PathBuf::from) {
        return if executable(&path) { Ok(path) } else { Err(format!("{} at {} is not executable", agent.label(), path.display())) };
    }
    agent_directories()
        .into_iter()
        .map(|d| d.join(agent.binary()))
        .find(executable)
        .ok_or_else(|| format!("{} is not installed. Install it, or set {} to its absolute path.", agent.label(), agent.override_env()))
}

fn sbpl_path(path: &Path) -> Result<String, String> {
    let text = path.to_str().ok_or("Sandbox paths must be UTF-8")?;
    if text.contains('"') || text.contains('\\') || text.contains('\n') {
        return Err("Sandbox paths may not contain quotes, backslashes or newlines".into());
    }
    Ok(text.to_owned())
}

fn regex_escape(text: &str) -> String {
    text.chars().fold(String::new(), |mut out, c| {
        if "\\.^$|?*+()[]{}".contains(c) {
            out.push('\\');
        }
        out.push(c);
        out
    })
}

/// The Seatbelt profile for one run. Reads are unrestricted (as with Codex's
/// legacy modes); writes are limited to the listed places.
pub(crate) fn sandbox_profile(agent: Agent, writable: Option<&Path>, home: &Path, temp: &Path) -> Result<String, String> {
    let home_text = sbpl_path(home)?;
    let mut allow = vec![
        format!("(subpath \"{}\")", sbpl_path(temp)?),
        "(literal \"/dev/null\")".into(),
        "(literal \"/dev/tty\")".into(),
        "(regex #\"^/dev/fd/\")".into(),
        "(regex #\"^/dev/ttys[0-9]+$\")".into(),
        format!("(subpath \"{home_text}/Library/Caches\")"),
    ];
    if let Some(dir) = writable {
        allow.push(format!("(subpath \"{}\")", sbpl_path(dir)?));
    }
    match agent {
        Agent::Claude => {
            allow.push(format!("(subpath \"{home_text}/.claude\")"));
            allow.push(format!("(regex #\"^{}/\\.claude\\.json\")", regex_escape(&home_text)));
            allow.push("(regex #\"^/private/tmp/claude-\")".into());
        }
        Agent::OpenCode => {
            for dir in [".local/share/opencode", ".local/state/opencode", ".cache/opencode", ".config/opencode"] {
                allow.push(format!("(subpath \"{home_text}/{dir}\")"));
            }
            allow.push("(regex #\"^/private/tmp/opencode\")".into());
        }
    }
    Ok(format!("(version 1)\n(allow default)\n(deny file-write*)\n(allow file-write*\n  {})\n", allow.join("\n  ")))
}

const READ_TOOLS: &[&str] = &["Bash", "Read", "Grep", "Glob"];
const WRITE_TOOLS: &[&str] = &["Bash", "Read", "Grep", "Glob", "Edit", "MultiEdit", "Write"];
const BRIDGE_TOOLS: &[&str] = &["mcp__neko__neko_list_tools", "mcp__neko__neko_call_tool"];

pub(crate) fn claude_args(spec: &RunSpec, bridge: Option<&BridgeConfig>, extraction: bool) -> Result<Vec<String>, String> {
    let mut args: Vec<String> = [
        "-p", "--output-format", "stream-json", "--verbose", "--setting-sources", "", "--strict-mcp-config",
        "--disable-slash-commands", "--no-session-persistence", "--permission-mode", "dontAsk",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if !spec.runtime.model.is_empty() && spec.runtime.model != "default" {
        args.extend(["--model".into(), spec.runtime.model.clone()]);
    }
    if extraction {
        args.extend(["--tools".into(), String::new()]);
        return Ok(args);
    }
    let tools = if spec.writable { WRITE_TOOLS } else { READ_TOOLS };
    args.push("--tools".into());
    args.extend(tools.iter().map(|t| t.to_string()));
    args.push("--allowedTools".into());
    args.extend(tools.iter().map(|t| t.to_string()));
    if let Some(bridge) = bridge {
        args.extend(BRIDGE_TOOLS.iter().map(|t| t.to_string()));
        if !bridge.executable.is_absolute() {
            return Err("Invalid scoped bridge configuration".into());
        }
        // The bridge reads its token and socket from the inherited environment.
        let config = serde_json::json!({"mcpServers": {"neko": {"type": "stdio", "command": bridge.executable, "args": ["--mcp-bridge"]}}});
        args.extend(["--mcp-config".into(), config.to_string()]);
    }
    Ok(args)
}

pub(crate) fn opencode_config(spec: &RunSpec, bridge: Option<&BridgeConfig>, extraction: bool) -> serde_json::Value {
    let permission = if extraction {
        // "ask" auto-rejects in non-interactive runs. Denying every tool makes
        // OpenCode's free tier refuse the request, and any tool call in an
        // extraction run is discarded by the parser anyway.
        serde_json::json!({"edit": "deny", "bash": "ask", "webfetch": "deny"})
    } else {
        serde_json::json!({"edit": if spec.writable { "allow" } else { "deny" }, "bash": "allow", "webfetch": "deny"})
    };
    let mut config = serde_json::json!({"permission": permission, "autoupdate": false, "share": "disabled"});
    if let (Some(bridge), false) = (bridge, extraction) {
        config["mcp"] = serde_json::json!({"neko": {"type": "local", "command": [bridge.executable, "--mcp-bridge"], "enabled": true}});
    }
    config
}

/// Parse one event line into Neko's event vocabulary. Returns the final
/// answer when this line carries it.
pub(crate) struct Parser {
    agent: Agent,
    extraction: bool,
    commands: std::collections::HashMap<String, String>,
    pub answer: Option<String>,
    pub failure: Option<String>,
    pub completed: bool,
    last_text: Option<String>,
}

impl Parser {
    pub(crate) fn new(agent: Agent, extraction: bool) -> Self {
        Self { agent, extraction, commands: Default::default(), answer: None, failure: None, completed: false, last_text: None }
    }

    pub(crate) fn line(&mut self, line: &[u8], on_event: &mut dyn FnMut(String)) -> Result<(), String> {
        if line.iter().all(u8::is_ascii_whitespace) {
            return Ok(());
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
            return Ok(()); // Startup banners and log lines are not events.
        };
        match self.agent {
            Agent::Claude => self.claude(&value, on_event),
            Agent::OpenCode => self.opencode(&value, on_event),
        }
    }

    fn receipt(&self, command: &str, exit_code: i64, output: &str, on_event: &mut dyn FnMut(String)) {
        on_event("Command completed".into());
        on_event(format!(
            "VERIFICATION_COMMAND {}",
            serde_json::json!({"command": command, "exit_code": exit_code, "output": bounded_text(output, MAX_EVENT)})
        ));
    }

    fn claude(&mut self, value: &serde_json::Value, on_event: &mut dyn FnMut(String)) -> Result<(), String> {
        let content = value["message"]["content"].as_array().cloned().unwrap_or_default();
        match value["type"].as_str() {
            Some("assistant") => {
                for block in &content {
                    match block["type"].as_str() {
                        Some("tool_use") => {
                            if self.extraction {
                                return Err("Extraction attempted a tool call; output discarded".into());
                            }
                            let name = block["name"].as_str().unwrap_or("");
                            if name == "Bash" {
                                let command = block["input"]["command"].as_str().unwrap_or("").to_owned();
                                self.commands.insert(block["id"].as_str().unwrap_or("").to_owned(), command);
                                on_event("Command running".into());
                            } else if matches!(name, "Edit" | "MultiEdit" | "Write") {
                                on_event("Updating workspace files".into());
                            }
                        }
                        Some("text") => {
                            if let Some(text) = block["text"].as_str() {
                                self.last_text = Some(text.to_owned());
                                on_event(bounded_text(text, MAX_EVENT));
                            }
                        }
                        _ => {}
                    }
                }
            }
            Some("user") => {
                for block in content.iter().filter(|b| b["type"] == "tool_result") {
                    let id = block["tool_use_id"].as_str().unwrap_or("");
                    let Some(command) = self.commands.remove(id) else { continue };
                    let output = match &block["content"] {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n"),
                        _ => String::new(),
                    };
                    let (exit_code, body) = claude_exit(&output, block["is_error"].as_bool() == Some(true));
                    self.receipt(&command, exit_code, body, on_event);
                }
            }
            Some("result") => {
                self.completed = true;
                if let Some(cost) = value["total_cost_usd"].as_f64() {
                    on_event(format!("USAGE {}", serde_json::json!({"cost_usd": cost, "input_tokens": value["usage"]["input_tokens"], "output_tokens": value["usage"]["output_tokens"]})));
                }
                let text = value["result"].as_str().unwrap_or("");
                if value["is_error"].as_bool() == Some(true) || value["subtype"].as_str().is_some_and(|s| s != "success") {
                    self.failure = Some(bounded_text(if text.is_empty() { value["subtype"].as_str().unwrap_or("Claude Code reported an error") } else { text }, MAX_EVENT));
                } else {
                    if text.len() > if self.extraction { 4096 } else { MAX_ANSWER } {
                        return Err("Claude Code answer exceeded the output limit".into());
                    }
                    self.answer = Some(if text.is_empty() { self.last_text.clone().unwrap_or_default() } else { text.to_owned() });
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn opencode(&mut self, value: &serde_json::Value, on_event: &mut dyn FnMut(String)) -> Result<(), String> {
        let part = &value["part"];
        match value["type"].as_str() {
            Some("tool_use") => {
                if self.extraction {
                    return Err("Extraction attempted a tool call; output discarded".into());
                }
                let tool = part["tool"].as_str().unwrap_or("");
                let state = &part["state"];
                if tool == "bash" && matches!(state["status"].as_str(), Some("completed" | "error")) {
                    let exit = state["metadata"]["exit"].as_i64().unwrap_or(if state["status"] == "error" { 1 } else { 0 });
                    let output = state["output"].as_str().or_else(|| state["error"].as_str()).unwrap_or("");
                    self.receipt(state["input"]["command"].as_str().unwrap_or(""), exit, output, on_event);
                } else if matches!(tool, "edit" | "write" | "patch" | "multiedit") {
                    on_event("Updating workspace files".into());
                }
            }
            Some("text") => {
                if let Some(text) = part["text"].as_str() {
                    let next = self.answer.take().map(|a| a + text).unwrap_or_else(|| text.to_owned());
                    if next.len() > if self.extraction { 4096 } else { MAX_ANSWER } {
                        return Err("OpenCode answer exceeded the output limit".into());
                    }
                    on_event(bounded_text(text, MAX_EVENT));
                    self.answer = Some(next);
                }
            }
            Some("step_start") => {
                // A new step's text replaces the previous step's.
                if self.answer.is_some() {
                    self.last_text = self.answer.take();
                }
            }
            Some("step_finish") => {
                self.completed = true;
                if let Some(cost) = part["cost"].as_f64() {
                    on_event(format!("USAGE {}", serde_json::json!({"cost_usd": cost, "input_tokens": part["tokens"]["input"], "output_tokens": part["tokens"]["output"]})));
                }
            }
            Some("error") => {
                let detail = value["error"]["data"]["message"].as_str().or_else(|| value["error"]["message"].as_str()).or_else(|| value["error"]["name"].as_str()).unwrap_or("OpenCode reported an error");
                self.failure = Some(bounded_text(detail, MAX_EVENT));
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<String, String> {
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        if !self.completed {
            return Err(format!("{} exited without completing the turn", self.agent.label()));
        }
        self.answer
            .or(self.last_text)
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| format!("{} completed without a final answer", self.agent.label()))
    }
}

/// Claude Code prefixes failed shell output with "Exit code N".
fn claude_exit(output: &str, is_error: bool) -> (i64, &str) {
    if let Some(rest) = output.strip_prefix("Exit code ") {
        let (code, body) = rest.split_once('\n').unwrap_or((rest, ""));
        if let Ok(code) = code.trim().parse() {
            return (code, body);
        }
    }
    (if is_error { 1 } else { 0 }, output)
}

pub(crate) fn run(
    agent: Agent,
    spec: &RunSpec,
    directory: &Path,
    bridge: Option<&BridgeConfig>,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(String),
    extraction: bool,
) -> Result<String, String> {
    let executable = resolve(agent)?;
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or("HOME is not set")?;
    let home = home.canonicalize().unwrap_or(home);
    // A private temp folder for this run: the CLI's scratch files and Neko's config.
    let scratch = std::env::temp_dir().join(format!("neko-{}-{}-{}", agent.binary(), std::process::id(), crate::now_unix_ms()));
    fs::create_dir(&scratch).map_err(|e| format!("Cannot create a scratch folder: {e}"))?;
    let scratch = scratch.canonicalize().map_err(|e| e.to_string())?;
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(scratch.clone());
    let profile = sandbox_profile(agent, spec.writable.then_some(directory), &home, &scratch)?;
    let mut command = Command::new("/usr/bin/sandbox-exec");
    command.arg("-p").arg(&profile).arg(&executable);
    match agent {
        Agent::Claude => {
            command.args(claude_args(spec, bridge, extraction)?);
        }
        Agent::OpenCode => {
            let config = scratch.join("opencode.json");
            fs::write(&config, opencode_config(spec, bridge, extraction).to_string()).map_err(|e| e.to_string())?;
            fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
            // OpenCode takes its project folder from --dir/PWD, not the process cwd.
            command.args(["run", "--format", "json", "--dir"]).arg(directory);
            if !spec.runtime.model.is_empty() {
                command.args(["-m", spec.runtime.model.as_str()]);
            }
            command.env("OPENCODE_CONFIG", &config).env("OPENCODE_DISABLE_PROJECT_CONFIG", "1").env("OPENCODE_DISABLE_AUTOUPDATE", "1");
        }
    }
    command.current_dir(directory).env("PWD", directory).env("TMPDIR", &scratch);
    if let Some(bridge) = bridge {
        if !bridge.socket.is_absolute() || bridge.token.is_empty() {
            return Err("Invalid scoped bridge configuration".into());
        }
        command.env("NEKO_MCP_TOKEN", &bridge.token).env("NEKO_MCP_SOCKET", &bridge.socket);
    }
    let mut paths = executable.parent().map(Path::to_owned).into_iter().collect::<Vec<_>>();
    paths.extend(agent_directories());
    if let Ok(path) = std::env::join_paths(paths) {
        command.env("PATH", path);
    }
    for name in ["CODEX_THREAD_ID", "CODEX_INTERNAL_ORIGINATOR_OVERRIDE", "PASEO_AGENT_ID", "PASEO_WORKSPACE_ID", "CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT"] {
        command.env_remove(name);
    }
    let prompt = if extraction { spec.prompt.clone() } else { prompt_with_git(&spec.prompt) };
    let mut parser = Parser::new(agent, extraction);
    let mut pending: Vec<u8> = Vec::new();
    let output = execute(command, prompt.as_bytes(), cancel, spec.timeout, |bytes| {
        for &byte in bytes {
            if byte == b'\n' {
                parser.line(&pending, on_event)?;
                pending.clear();
            } else {
                if pending.len() >= MAX_LINE {
                    return Err(format!("{} JSON line exceeded the output limit", agent.label()));
                }
                pending.push(byte);
            }
        }
        Ok(())
    })?;
    if !pending.is_empty() {
        parser.line(&pending, on_event)?;
    }
    if !output.status.success() {
        return Err(format!("{} exited with {}: {}{}", agent.label(), output.status, bounded_text(output.stderr.trim(), MAX_EVENT), parser.failure.as_ref().map(|f| format!(" {f}")).unwrap_or_default()));
    }
    parser.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(writable: bool) -> RunSpec {
        RunSpec { directory: PathBuf::from("/tmp"), prompt: "x".into(), writable, timeout: Duration::from_secs(5), runtime: neko_protocol::workbench::AgentRuntime { provider: "claude".into(), model: "sonnet".into() } }
    }

    #[test]
    fn profiles_confine_writes_to_the_worktree_and_agent_state() {
        let p = sandbox_profile(Agent::Claude, Some(Path::new("/w/task")), Path::new("/Users/a"), Path::new("/t/run")).unwrap();
        assert!(p.contains("(deny file-write*)"));
        assert!(p.contains("(subpath \"/w/task\")") && p.contains("(subpath \"/Users/a/.claude\")"));
        assert!(p.contains("^/Users/a/\\.claude\\.json"));
        assert!(!p.contains("(subpath \"/Users/a\")"), "never the whole home folder");
        let read_only = sandbox_profile(Agent::OpenCode, None, Path::new("/Users/a"), Path::new("/t/run")).unwrap();
        assert!(!read_only.contains("/w/task") && read_only.contains(".local/share/opencode"));
        assert!(sandbox_profile(Agent::Claude, Some(Path::new("/w/\"x")), Path::new("/Users/a"), Path::new("/t")).is_err());
    }

    #[test]
    fn claude_roles_get_the_right_tools() {
        let read = claude_args(&spec(false), None, false).unwrap();
        assert!(read.contains(&"Bash".to_string()) && !read.contains(&"Edit".to_string()));
        assert!(read.windows(2).any(|w| w == ["--model", "sonnet"]));
        let write = claude_args(&spec(true), None, false).unwrap();
        assert!(write.contains(&"Edit".to_string()));
        assert!(!write.iter().any(|a| a.contains("WebFetch") || a.contains("WebSearch")));
        let extract = claude_args(&spec(false), None, true).unwrap();
        assert!(extract.windows(2).any(|w| w[0] == "--tools" && w[1].is_empty()));
        assert!(!extract.contains(&"Bash".to_string()));
        let bridge = BridgeConfig { executable: "/bin/neko-daemon".into(), socket: "/s".into(), token: "secret".into() };
        let bridged = claude_args(&spec(false), Some(&bridge), false).unwrap();
        assert!(bridged.contains(&"mcp__neko__neko_call_tool".to_string()));
        assert!(!bridged.iter().any(|a| a.contains("secret")), "the token never appears in argv");
    }

    #[test]
    fn opencode_permissions_follow_the_role() {
        assert_eq!(opencode_config(&spec(false), None, false)["permission"]["edit"], "deny");
        assert_eq!(opencode_config(&spec(true), None, false)["permission"]["edit"], "allow");
        assert_eq!(opencode_config(&spec(true), None, false)["permission"]["webfetch"], "deny");
        let extraction = opencode_config(&spec(true), None, true);
        assert_eq!(extraction["permission"]["edit"], "deny");
        assert_eq!(extraction["permission"]["bash"], "ask");
    }

    #[test]
    fn claude_events_become_receipts_and_an_answer() {
        let mut events = Vec::new();
        let mut p = Parser::new(Agent::Claude, false);
        let lines = [
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cargo test"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":"Exit code 101\nfailures: 1"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"ls"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"a.rs"}]}}"#,
            r#"{"type":"result","subtype":"success","is_error":false,"result":"All done","total_cost_usd":0.02,"usage":{"input_tokens":5,"output_tokens":7}}"#,
        ];
        for l in lines { p.line(l.as_bytes(), &mut |e| events.push(e)).unwrap(); }
        assert_eq!(p.finish().unwrap(), "All done");
        let receipts: Vec<&String> = events.iter().filter(|e| e.starts_with("VERIFICATION_COMMAND ")).collect();
        assert!(receipts[0].contains("\"exit_code\":101") && receipts[0].contains("cargo test"));
        assert!(receipts[1].contains("\"exit_code\":0"));
        assert!(events.iter().any(|e| e.starts_with("USAGE ") && e.contains("0.02")));
    }

    #[test]
    fn opencode_events_become_receipts_and_an_answer() {
        let mut events = Vec::new();
        let mut p = Parser::new(Agent::OpenCode, false);
        let lines = [
            r#"{"type":"step_start","part":{"type":"step-start"}}"#,
            r#"{"type":"tool_use","part":{"type":"tool","tool":"bash","state":{"status":"completed","input":{"command":"npm test"},"output":"ok\n","metadata":{"exit":0}}}}"#,
            r#"{"type":"step_finish","part":{"type":"step-finish","cost":0,"tokens":{"input":1,"output":2}}}"#,
            r#"{"type":"step_start","part":{"type":"step-start"}}"#,
            r#"{"type":"text","part":{"type":"text","text":"DONE"}}"#,
            r#"{"type":"step_finish","part":{"type":"step-finish","cost":0}}"#,
        ];
        for l in lines { p.line(l.as_bytes(), &mut |e| events.push(e)).unwrap(); }
        assert_eq!(p.finish().unwrap(), "DONE");
        assert!(events.iter().any(|e| e.contains("\"command\":\"npm test\"") && e.contains("\"exit_code\":0")));
    }

    /// Live: a writable run edits inside its worktree, reports receipts, and
    /// cannot write to the home folder. NEKO_LIVE_AGENT=claude:haiku or
    /// opencode:opencode/big-pickle. Uses a little quota.
    #[test]
    #[ignore = "Uses an authenticated agent CLI"]
    fn live_sandboxed_worker() {
        let target = std::env::var("NEKO_LIVE_AGENT").unwrap_or_else(|_| "claude:haiku".into());
        let (provider, model) = target.split_once(':').unwrap();
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().canonicalize().unwrap();
        assert!(Command::new("git").arg("init").arg("-q").arg(&repo).status().unwrap().success(), "task worktrees are Git checkouts");
        let escape = PathBuf::from(std::env::var("HOME").unwrap()).join(format!("neko-escape-{}", std::process::id()));
        let spec = RunSpec {
            directory: repo.clone(),
            prompt: format!("Do exactly these three steps with your tools, then reply DONE.\n1. Create the file hello.txt containing the word hi.\n2. Run this shell command: cat hello.txt\n3. Run this shell command: touch {}", escape.display()),
            writable: true,
            timeout: Duration::from_secs(240),
            runtime: neko_protocol::workbench::AgentRuntime { provider: provider.into(), model: model.into() },
        };
        let mut events = Vec::new();
        let agent = Agent::for_provider(provider).unwrap();
        let answer = run(agent, &spec, &repo, None, &AtomicBool::new(false), &mut |e| events.push(e), false);
        eprintln!("answer: {answer:?}");
        for e in events.iter().filter(|e| e.starts_with("VERIFICATION_COMMAND")) { eprintln!("{e}"); }
        assert_eq!(fs::read_to_string(repo.join("hello.txt")).unwrap().trim(), "hi");
        assert!(events.iter().any(|e| e.contains("cat ") && e.contains("hello.txt") && e.contains("\"exit_code\":0")), "receipt for cat");
        assert!(!escape.exists(), "the sandbox must block writes outside the worktree");
    }

    #[test]
    fn extraction_refuses_any_tool_call() {
        let mut p = Parser::new(Agent::Claude, true);
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t","name":"Read","input":{}}]}}"#;
        assert!(p.line(line.as_bytes(), &mut |_| {}).is_err());
    }
}
