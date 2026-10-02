//! Neko's own agent model catalog and model check.
//!
//! Discovery asks each runtime directly: the Codex CLI's `app-server`
//! (account, plan and model list), Ollama's `/api/tags` and LM Studio's
//! `/v1/models`. Listing is metadata only and never generates tokens. The
//! one explicit [`check`] sends a minimal prompt through the same no-tool,
//! read-only runner path real tasks use, so a passing check proves exactly
//! the configuration a task will run with.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use neko_protocol::agent_models::{
    CatalogModel, ModelAccess, ModelCatalog, ModelCheck, ModelSource, SourceStatus,
};
use neko_protocol::workbench::AgentRuntime;
use serde_json::{Value, json};

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(15);
const LOCAL_TIMEOUT: Duration = Duration::from_millis(1500);
const CHECK_TIMEOUT: Duration = Duration::from_secs(90);
const CACHE_TTL_MS: i64 = 5 * 60 * 1000;
const MAX_MODELS: usize = 200;
const CHECK_REPLY: &str = "NEKO_OK";
const CHECK_PROMPT: &str = "This is a connection check. Do not use any tools. Reply with exactly NEKO_OK and nothing else.";

struct State {
    catalog: Option<ModelCatalog>,
    /// "provider\u{0}model" -> latest explicit check outcome.
    checks: BTreeMap<String, (bool, String)>,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State { catalog: None, checks: BTreeMap::new() }))
}

fn key(provider: &str, model: &str) -> String {
    format!("{}\u{0}{model}", canonical_provider(provider))
}

fn canonical_provider(provider: &str) -> &str {
    if provider.is_empty() { "codex" } else { provider }
}

/// The current catalog, read fresh when `refresh` is set or the cache is old.
pub fn catalog(refresh: bool) -> ModelCatalog {
    let now = crate::now_unix_ms();
    if !refresh {
        let guard = state().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cached) = &guard.catalog {
            if now - cached.read_at_ms < CACHE_TTL_MS {
                return with_checks(cached.clone(), &guard.checks);
            }
        }
    }
    let (codex, claude, opencode, ollama, lmstudio) = std::thread::scope(|scope| {
        let codex = scope.spawn(codex_source);
        let claude = scope.spawn(claude_source);
        let opencode = scope.spawn(opencode_source);
        let ollama = scope.spawn(ollama_source);
        let lmstudio = scope.spawn(lmstudio_source);
        (
            codex.join().unwrap_or_else(|_| failed_source("codex", "Codex")),
            claude.join().unwrap_or_else(|_| failed_source("claude", "Claude Code")),
            opencode.join().unwrap_or_else(|_| failed_source("opencode", "OpenCode")),
            ollama.join().unwrap_or_else(|_| failed_source("ollama", "Ollama")),
            lmstudio.join().unwrap_or_else(|_| failed_source("lmstudio", "LM Studio")),
        )
    });
    let fresh = ModelCatalog { sources: vec![codex, claude, opencode, ollama, lmstudio], read_at_ms: now };
    let mut guard = state().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.catalog = Some(fresh.clone());
    with_checks(fresh, &guard.checks)
}

fn with_checks(mut catalog: ModelCatalog, checks: &BTreeMap<String, (bool, String)>) -> ModelCatalog {
    for source in &mut catalog.sources {
        for model in &mut source.models {
            if model.access == ModelAccess::Unavailable {
                continue;
            }
            if let Some((ok, message)) = checks.get(&key(&source.provider, &model.id)) {
                model.access = if *ok { ModelAccess::Checked } else { ModelAccess::Unavailable };
                if !*ok {
                    model.reason = Some(message.clone());
                }
            }
        }
    }
    catalog
}

/// A GUI-launched daemon inherits a bare PATH, but npm-installed CLIs are
/// scripts that need `node` beside them. Give discovery the same search
/// path the task runner uses.
fn agent_path(executable: &Path) -> std::ffi::OsString {
    let mut paths: Vec<PathBuf> = executable.parent().map(Path::to_owned).into_iter().collect();
    paths.extend(crate::native_runner::cli_workers::agent_directories());
    std::env::join_paths(paths).unwrap_or_default()
}

fn failed_source(provider: &str, label: &str) -> ModelSource {
    ModelSource {
        provider: provider.into(),
        label: label.into(),
        connection: format!("{label} · couldn’t read models"),
        status: SourceStatus::Error,
        note: Some("Refresh to try again.".into()),
        default_model: None,
        models: Vec::new(),
    }
}

// ---------------------------------------------------------------- Codex

fn codex_source() -> ModelSource {
    let executable = match crate::native_runner::resolve_codex() {
        Ok(path) => path,
        Err(_) => {
            return ModelSource {
                provider: "codex".into(),
                label: "Codex".into(),
                connection: "Codex · not installed".into(),
                status: SourceStatus::NotInstalled,
                note: Some("Install the Codex app or CLI, then refresh.".into()),
                default_model: None,
                models: Vec::new(),
            };
        }
    };
    match discover_codex(&executable, DISCOVERY_TIMEOUT) {
        Ok(source) => source,
        Err(_) => ModelSource {
            note: Some("Couldn’t read Codex’s model list. Update Codex or check its login, then refresh.".into()),
            ..failed_source("codex", "Codex")
        },
    }
}

/// Bounded JSON-RPC conversation with a short-lived `codex app-server`.
/// It runs from an empty scratch folder with Codex's default provider, the
/// same provider Neko's workers use (they ignore user configuration).
pub(crate) fn discover_codex(executable: &Path, timeout: Duration) -> Result<ModelSource, String> {
    let scratch = Scratch::new("neko-models")?;
    let mut child = Command::new(executable)
        .args(["app-server", "-c", "model_provider=\"openai\""])
        .current_dir(&scratch.0)
        .env("PATH", agent_path(executable))
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_INTERNAL_ORIGINATOR_OVERRIDE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Couldn’t start Codex: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("Codex stdin unavailable")?;
    let stdout = child.stdout.take().ok_or("Codex stdout unavailable")?;
    let (lines_tx, lines) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.len() > 1024 * 1024 || lines_tx.send(line).is_err() {
                break;
            }
        }
    });
    let deadline = Instant::now() + timeout;
    let result = (|| {
        let mut send = |value: Value| -> Result<(), String> {
            writeln!(stdin, "{value}").map_err(|_| "Codex stopped accepting requests".to_string())
        };
        let reply = |id: u64| -> Result<Value, String> {
            loop {
                let left = deadline.checked_duration_since(Instant::now()).ok_or("Codex took too long to list models")?;
                let line = lines.recv_timeout(left).map_err(|_| "Codex stopped before listing models".to_string())?;
                let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
                if message.get("id").and_then(Value::as_u64) == Some(id) {
                    if message.get("error").is_some() {
                        return Err(format!("Codex request {id} failed"));
                    }
                    return Ok(message.get("result").cloned().unwrap_or(Value::Null));
                }
            }
        };
        send(json!({"method": "initialize", "id": 1, "params": {"clientInfo": {"name": "neko", "title": "Neko", "version": env!("CARGO_PKG_VERSION")}}}))?;
        reply(1)?;
        send(json!({"method": "initialized", "params": {}}))?;
        send(json!({"method": "account/read", "id": 2, "params": {"refreshToken": false}}))?;
        let account = reply(2)?;
        let mut pages = Vec::new();
        let mut cursor: Option<String> = None;
        for page in 0..20u64 {
            let mut params = json!({"limit": 100});
            if let Some(cursor) = &cursor {
                params["cursor"] = json!(cursor);
            }
            send(json!({"method": "model/list", "id": 3 + page, "params": params}))?;
            let result = reply(3 + page)?;
            cursor = result.get("nextCursor").and_then(Value::as_str).map(str::to_owned);
            pages.push(result);
            if cursor.is_none() {
                break;
            }
        }
        if cursor.is_some() {
            return Err("Codex returned an incomplete model list".into());
        }
        Ok(codex_catalog(&account, &pages))
    })();
    let _ = child.kill();
    let _ = child.wait();
    result
}

pub(crate) fn codex_catalog(account: &Value, pages: &[Value]) -> ModelSource {
    let signed_out = account.get("requiresOpenaiAuth").and_then(Value::as_bool) == Some(true)
        && account.get("account").is_none_or(Value::is_null);
    let connection = if signed_out {
        "Codex · sign-in required".to_string()
    } else {
        match account["account"]["type"].as_str() {
            Some("chatgpt") => format!("ChatGPT · {}", plan_label(account["account"]["planType"].as_str())),
            Some("apiKey") => "OpenAI API key".into(),
            _ => "Codex · configured provider".into(),
        }
    };
    let mut models: Vec<CatalogModel> = Vec::new();
    let mut default_model = None;
    for row in pages.iter().filter_map(|page| page.get("data")?.as_array()).flatten() {
        let Some(id) = row["model"].as_str().filter(|id| valid_model_id(id)) else { continue };
        if row["hidden"].as_bool() == Some(true) || models.iter().any(|m| m.id == id) || models.len() >= MAX_MODELS {
            continue;
        }
        let is_default = row["isDefault"].as_bool() == Some(true);
        if is_default {
            default_model = Some(id.to_string());
        }
        models.push(CatalogModel {
            id: id.into(),
            label: row["displayName"].as_str().filter(|s| !s.is_empty()).unwrap_or(id).chars().take(80).collect(),
            description: row["description"].as_str().map(|s| s.chars().take(240).collect()),
            recommended: is_default && !signed_out,
            access: if signed_out { ModelAccess::Unavailable } else { ModelAccess::Listed },
            reason: if signed_out {
                Some("Sign in to Codex, then refresh models.".into())
            } else if is_default {
                Some("Codex recommends this model for your account.".into())
            } else {
                None
            },
            reasoning_efforts: row["supportedReasoningEfforts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e["reasoningEffort"].as_str())
                .filter(|e| e.len() <= 16 && e.bytes().all(|b| b.is_ascii_alphanumeric()))
                .map(str::to_owned)
                .collect(),
        });
    }
    ModelSource {
        provider: "codex".into(),
        label: "Codex".into(),
        connection,
        status: if signed_out { SourceStatus::SignInRequired } else { SourceStatus::Ready },
        note: Some(if signed_out {
            "Sign in to Codex, then refresh.".into()
        } else {
            "Listed by your Codex account. Plan access and remaining quota are confirmed when a model is checked or used.".into()
        }),
        default_model,
        models,
    }
}

fn plan_label(plan: Option<&str>) -> String {
    let plan = plan.unwrap_or("").trim();
    if plan.is_empty() || plan.len() > 24 || !plan.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return "plan not reported".into();
    }
    plan.split(['_', '-'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------- Local servers

// ---------------------------------------------------------------- Claude Code and OpenCode

fn claude_source() -> ModelSource {
    let base = |connection: &str, status: SourceStatus, note: &str| ModelSource {
        provider: "claude".into(),
        label: "Claude Code".into(),
        connection: connection.into(),
        status,
        note: Some(note.into()),
        default_model: None,
        models: Vec::new(),
    };
    let Ok(executable) = crate::native_runner::cli_workers::resolve(crate::native_runner::cli_workers::Agent::Claude) else {
        return base("Claude Code · not installed", SourceStatus::NotInstalled, "Install Claude Code to run tasks with your Claude plan.");
    };
    match discover_claude(&executable, DISCOVERY_TIMEOUT) {
        Ok(source) => source,
        // Seen live: launched from a Neko app that hasn't been allowed into a
        // protected folder, Claude Code blocks on that file access.
        Err(error) if error.contains("took too long") => base(
            "Claude Code · not responding",
            SourceStatus::Error,
            "Claude Code didn’t answer. macOS may be holding a file it opens until Neko is allowed: check System Settings → Privacy & Security → Files and Folders, then refresh.",
        ),
        Err(_) => base("Claude Code · couldn’t read models", SourceStatus::Error, "Update Claude Code or check its login, then refresh."),
    }
}

/// Claude Code's own "initialize" control request lists its models and
/// account. No prompt is sent and no tokens are generated.
fn discover_claude(executable: &Path, timeout: Duration) -> Result<ModelSource, String> {
    let scratch = Scratch::new("neko-claude-models")?;
    let mut child = Command::new(executable)
        .args(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--tools", "", "--strict-mcp-config", "--setting-sources", "", "--disable-slash-commands", "--no-session-persistence"])
        .current_dir(&scratch.0)
        .env_remove("CLAUDECODE")
        .env("PATH", agent_path(executable))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Couldn’t start Claude Code: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("stdin unavailable")?;
    let stdout = child.stdout.take().ok_or("stdout unavailable")?;
    let (tx, lines) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.len() > 1024 * 1024 || tx.send(line).is_err() {
                break;
            }
        }
    });
    let result = (|| {
        writeln!(stdin, "{}", json!({"type": "control_request", "request_id": "neko-models", "request": {"subtype": "initialize", "hooks": {}}}))
            .map_err(|_| "Claude Code stopped accepting requests".to_string())?;
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.checked_duration_since(Instant::now()).ok_or("Claude Code took too long to list models")?;
            let line = lines.recv_timeout(left).map_err(|_| "Claude Code stopped before listing models".to_string())?;
            let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
            if message["type"] == "control_response" && message["response"]["request_id"] == "neko-models" {
                if message["response"]["subtype"] != "success" {
                    return Err("Claude Code initialize failed".into());
                }
                return Ok(claude_catalog(&message["response"]["response"]));
            }
        }
    })();
    let _ = child.kill();
    let _ = child.wait();
    result
}

pub(crate) fn claude_catalog(response: &Value) -> ModelSource {
    let account = &response["account"];
    let signed_out = account["tokenSource"].as_str() == Some("none")
        && account["apiKeySource"].as_str().is_none_or(|s| s == "none");
    let connection = if signed_out {
        "Claude Code · sign-in required".to_string()
    } else {
        match account["subscriptionType"].as_str().filter(|s| s.len() <= 40 && s.chars().all(|c| c.is_ascii_alphanumeric() || " _-".contains(c))) {
            Some(plan) if plan.to_ascii_lowercase().starts_with("claude") => plan.replace('_', " "),
            Some(plan) => format!("Claude · {}", plan.replace('_', " ")),
            None => "Claude Code · configured connection".into(),
        }
    };
    let mut models = Vec::new();
    let mut default_model = None;
    for row in response["models"].as_array().into_iter().flatten() {
        let Some(id) = row["value"].as_str().filter(|id| valid_model_id(id)) else { continue };
        if models.len() >= MAX_MODELS || models.iter().any(|m: &CatalogModel| m.id == id) {
            continue;
        }
        let resolved = row["resolvedModel"].as_str().filter(|r| valid_model_id(r));
        if id == "default" {
            default_model = resolved.map(str::to_owned).or(Some("default".into()));
            continue; // "Provider default" in the picker already means this.
        }
        models.push(CatalogModel {
            id: id.into(),
            label: row["displayName"].as_str().unwrap_or(id).chars().take(80).collect(),
            description: resolved.filter(|r| *r != id).map(str::to_owned),
            recommended: false,
            access: if signed_out { ModelAccess::Unavailable } else { ModelAccess::Listed },
            reason: signed_out.then(|| "Sign in to Claude Code, then refresh models.".into()),
            reasoning_efforts: Vec::new(),
        });
    }
    if let Some(default) = &default_model {
        for model in models.iter_mut().filter(|m| m.description.as_deref() == Some(default.as_str()) || &m.id == default) {
            model.recommended = !signed_out;
            model.reason.get_or_insert_with(|| "Claude Code’s default for your plan.".into());
        }
    }
    ModelSource {
        provider: "claude".into(),
        label: "Claude Code".into(),
        connection,
        status: if signed_out { SourceStatus::SignInRequired } else { SourceStatus::Ready },
        note: Some(if signed_out {
            "Sign in to Claude Code, then refresh.".into()
        } else {
            "Runs with your Claude Code login inside Neko’s sandbox. Plan limits apply; check a model before relying on it.".into()
        }),
        default_model,
        models,
    }
}

fn opencode_source() -> ModelSource {
    let base = ModelSource {
        provider: "opencode".into(),
        label: "OpenCode".into(),
        connection: "OpenCode · not installed".into(),
        status: SourceStatus::NotInstalled,
        note: Some("Install OpenCode to use its providers and free models.".into()),
        default_model: None,
        models: Vec::new(),
    };
    let Ok(executable) = crate::native_runner::cli_workers::resolve(crate::native_runner::cli_workers::Agent::OpenCode) else {
        return base;
    };
    let Ok(scratch) = Scratch::new("neko-opencode-models") else { return base };
    let output = run_bounded(Command::new(&executable).arg("models").current_dir(&scratch.0).env("PATH", agent_path(&executable)).env("OPENCODE_DISABLE_PROJECT_CONFIG", "1"), DISCOVERY_TIMEOUT);
    match output {
        Some(text) => opencode_catalog(&text),
        None => ModelSource { connection: "OpenCode · couldn’t read models".into(), status: SourceStatus::Error, note: Some("Update OpenCode, then refresh.".into()), ..base },
    }
}

pub(crate) fn opencode_catalog(text: &str) -> ModelSource {
    let mut ids: Vec<String> = text
        .lines()
        .map(|l| l.trim().trim_start_matches(|c: char| c == '\u{1b}' || c == '['))
        .filter(|l| l.contains('/') && !l.starts_with('/') && !l.ends_with('/') && valid_model_id(l))
        .map(str::to_owned)
        .collect();
    ids.dedup();
    ids.truncate(MAX_MODELS);
    let providers: std::collections::BTreeSet<&str> = ids.iter().filter_map(|i| i.split('/').next()).collect();
    ModelSource {
        provider: "opencode".into(),
        label: "OpenCode".into(),
        connection: format!("OpenCode · {} {}", providers.len(), if providers.len() == 1 { "provider" } else { "providers" }),
        status: SourceStatus::Ready,
        note: Some(if ids.is_empty() { "No models yet. Connect a provider in OpenCode, then refresh.".into() } else { "Runs with OpenCode’s own providers inside Neko’s sandbox. Free models may have provider limits.".into() }),
        default_model: None,
        models: ids
            .into_iter()
            .map(|id| CatalogModel {
                label: id.split_once('/').map(|(p, m)| format!("{m} · {p}")).unwrap_or_else(|| id.clone()),
                recommended: false,
                description: id.ends_with("-free").then(|| "Free model".into()),
                id,
                access: ModelAccess::Listed,
                reason: None,
                reasoning_efforts: Vec::new(),
            })
            .collect(),
    }
}

/// Stdout of a short metadata command, or None on failure or timeout.
fn run_bounded(command: &mut Command, timeout: Duration) -> Option<String> {
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::Read::take(stdout, 1024 * 1024), &mut text);
        let _ = tx.send(text);
    });
    let text = rx.recv_timeout(timeout).ok();
    let _ = child.kill();
    let status = child.wait().ok()?;
    text.filter(|_| status.success() || status.code().is_none())
}


// ---------------------------------------------------------------- Local servers

fn ollama_source() -> ModelSource {
    local_source(
        "ollama",
        "Ollama",
        "http://127.0.0.1:11434/api/tags",
        |value| value["models"].as_array().into_iter().flatten().filter_map(|m| m["name"].as_str()).map(str::to_owned).collect(),
        ollama_installed(),
    )
}

fn lmstudio_source() -> ModelSource {
    local_source(
        "lmstudio",
        "LM Studio",
        "http://127.0.0.1:1234/v1/models",
        |value| value["data"].as_array().into_iter().flatten().filter_map(|m| m["id"].as_str()).map(str::to_owned).collect(),
        lmstudio_installed(),
    )
}

fn local_source(provider: &str, label: &str, url: &str, parse: fn(&Value) -> Vec<String>, installed: bool) -> ModelSource {
    let base = ModelSource {
        provider: provider.into(),
        label: label.into(),
        connection: format!("{label} · local"),
        status: SourceStatus::Ready,
        note: None,
        default_model: None,
        models: Vec::new(),
    };
    let response = reqwest::blocking::Client::builder()
        .timeout(LOCAL_TIMEOUT)
        .no_proxy()
        .build()
        .ok()
        .and_then(|client| client.get(url).send().ok())
        .filter(|r| r.status().is_success())
        .and_then(|r| r.json::<Value>().ok());
    let Some(value) = response else {
        return if installed {
            ModelSource {
                connection: format!("{label} · not running"),
                status: SourceStatus::NotRunning,
                note: Some(format!("Start {label}’s local server, then refresh.")),
                ..base
            }
        } else {
            ModelSource {
                connection: format!("{label} · not installed"),
                status: SourceStatus::NotInstalled,
                note: Some(format!("Install {label} to run models on this Mac.")),
                ..base
            }
        };
    };
    let mut names = parse(&value);
    names.retain(|id| valid_model_id(id));
    names.sort();
    names.dedup();
    names.truncate(MAX_MODELS);
    let note = if names.is_empty() { Some(format!("{label} is running but has no models yet. Download one, then refresh.")) } else { None };
    ModelSource {
        note,
        models: names
            .into_iter()
            .map(|id| CatalogModel { label: id.clone(), id, description: None, recommended: false, access: ModelAccess::Listed, reason: None, reasoning_efforts: Vec::new() })
            .collect(),
        ..base
    }
}

fn ollama_installed() -> bool {
    Path::new("/Applications/Ollama.app").exists() || on_path("ollama")
}

fn lmstudio_installed() -> bool {
    Path::new("/Applications/LM Studio.app").exists()
        || std::env::var_os("HOME").is_some_and(|home| PathBuf::from(home).join(".lmstudio/bin/lms").exists())
}

fn on_path(name: &str) -> bool {
    crate::native_runner::executable_directories().iter().any(|d| d.join(name).is_file())
}

/// Model IDs become a single CLI argument; refuse anything that is not a
/// plain identifier so a catalog entry cannot smuggle flags or whitespace.
pub fn valid_model_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 120
        && !id.starts_with('-')
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b':' | b'/' | b'@' | b'[' | b']'))
}

// ---------------------------------------------------------------- Check

/// One explicit, minimal prompt through the real runner. May use quota.
pub fn check(runtime: &AgentRuntime) -> ModelCheck {
    let outcome = run_check(runtime);
    let (ok, unavailable, message) = match outcome {
        Ok(()) => (true, false, "Checked. This model is ready to use.".to_string()),
        Err(raw) => {
            let (unavailable, message) = friendly_failure(&raw);
            (false, unavailable, message)
        }
    };
    let mut guard = state().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if ok || unavailable {
        guard.checks.insert(key(&runtime.provider, &runtime.model), (ok, message.clone()));
    }
    ModelCheck { runtime: runtime.clone(), ok, unavailable, message }
}

fn run_check(runtime: &AgentRuntime) -> Result<(), String> {
    if !runtime.model.is_empty() && !valid_model_id(&runtime.model) {
        return Err("invalid model id".into());
    }
    if !matches!(runtime.provider.as_str(), "" | "codex" | "ollama" | "lmstudio" | "opencodex" | "claude" | "opencode") {
        return Err("unsupported provider".into());
    }
    let scratch = Scratch::new("neko-model-check")?;
    let spec = crate::native_runner::RunSpec {
        directory: scratch.0.clone(),
        prompt: CHECK_PROMPT.into(),
        writable: false,
        timeout: CHECK_TIMEOUT,
        runtime: runtime.clone(),
    };
    let reply = crate::native_runner::extract(&spec, &AtomicBool::new(false))?;
    if reply.trim().trim_matches('.').trim() == CHECK_REPLY {
        Ok(())
    } else {
        Err("unexpected reply".into())
    }
}

/// Map raw CLI/provider errors to short, actionable, credential-free text.
/// Raw errors can contain keys, tokens or paths, so they are never shown.
pub fn friendly_failure(raw: &str) -> (bool, String) {
    // CLI log lines (timestamps, WARN/INFO diagnostics) are noise and contain
    // digit runs that would otherwise look like HTTP status codes.
    let text = raw
        .to_ascii_lowercase()
        .split(['\n', '|'])
        .flat_map(|line| line.split("\\n"))
        // A log line can still end with the provider's JSON error payload.
        .filter_map(|line| {
            if [" warn ", " info ", " debug ", " trace "].iter().any(|level| line.contains(level)) {
                line.find('{').map(|at| &line[at..])
            } else {
                Some(line)
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let any = |needles: &[&str]| needles.iter().any(|n| text.contains(n));
    let status = |code: &str| has_status(&text, code);
    if any(&["newer version of codex", "upgrade to the latest app or cli", "requires a newer version"]) {
        return (true, "This model needs a newer Codex. Update Codex, then refresh models.".into());
    }
    if status("429") || any(&["quota", "rate limit", "rate_limit", "ratelimit", "usage limit", "usage_limit", "insufficient credit", "insufficient_quota", "limit reached", "credit balance"]) {
        return (true, "Usage limits or credits are used up for this connection. Wait for the reset or update your provider account, then refresh.".into());
    }
    if status("401") || any(&["unauthorized", "authentication", "not logged", "not signed", "sign in", "log in", "login", "no credentials", "invalid api key", "api key is missing", "missing api key"]) {
        return (true, "Sign in or reconnect this provider, then refresh models.".into());
    }
    if status("403") || status("404") || any(&["model not found", "not supported", "unsupported model", "does not exist", "not available", "no access", "access denied", "upgrade your plan", "requires a paid", "requires pro", "requires plus", "subscription", "modelnotfound", "model_not_found"]) {
        return (true, "This model isn’t available for your connection or plan. Choose another model.".into());
    }
    if status("400") && any(&["model"]) {
        return (true, "The provider rejected this model. Choose another model or update the runtime.".into());
    }
    if any(&["not installed", "no such file", "enoent", "not executable"]) {
        return (false, "The runtime couldn’t start. Check that it is installed, then retry.".into());
    }
    if any(&["connection refused", "couldn't connect", "could not connect", "proxy is not running", "unavailable; start"]) {
        return (false, "Couldn’t reach the model server. Start it, then retry.".into());
    }
    if any(&["timed out", "timeout", "took too long"]) {
        return (false, "The check timed out. Check your connection and retry.".into());
    }
    if text == "unexpected reply" {
        return (false, "The model answered, but didn’t complete the check. Retry, or choose another model.".into());
    }
    if text == "invalid model id" {
        return (false, "Enter a valid model ID.".into());
    }
    (false, "Couldn’t check this model. Check your connection and provider setup, then retry. Your previous model is still selected.".into())
}

/// True when `code` appears as a standalone number, not inside a longer
/// digit run, or decimal such as "49.071429".
fn has_status(text: &str, code: &str) -> bool {
    let bytes = text.as_bytes();
    let joined = |b: u8| b.is_ascii_alphanumeric() || b == b'.';
    text.match_indices(code).any(|(at, _)| {
        let before = at.checked_sub(1).map(|i| bytes[i]);
        let after = bytes.get(at + code.len()).copied();
        !before.is_some_and(joined) && !after.is_some_and(joined)
    })
}

// ---------------------------------------------------------------- Scratch dir

/// What Neko runs with on this Mac, and how long each part takes to answer.
/// Generates no tokens; also refreshes the cached catalog.
pub fn diagnostics(data_dir: &Path) -> neko_protocol::agent_models::DiagnosticsReport {
    use neko_protocol::agent_models::{DiagnosticCheck, DiagnosticsReport};
    let mut checks = Vec::new();
    let started = Instant::now();
    let installs: Vec<(PathBuf, Option<String>)> = crate::native_runner::codex_candidates()
        .into_iter()
        .map(|path| { let version = crate::native_runner::codex_version_label(&path); (path, version) })
        .collect();
    let chosen = crate::native_runner::resolve_codex().ok();
    let describe = |(path, version): &(PathBuf, Option<String>)| format!("{} ({})", path.display(), version.as_deref().unwrap_or("version unknown"));
    checks.push(DiagnosticCheck {
        name: "Codex CLI".into(),
        ok: chosen.is_some(),
        detail: match &chosen {
            Some(path) => {
                let used = installs.iter().find(|(p, _)| p == path).map(describe).unwrap_or_else(|| path.display().to_string());
                let others: Vec<String> = installs.iter().filter(|(p, _)| p != path).map(describe).collect();
                if others.is_empty() { format!("Using {used}") } else { format!("Using {used}. Also installed: {}", others.join(", ")) }
            }
            None => "Not found. Install Codex, or set NEKO_CODEX_PATH.".into(),
        },
        millis: started.elapsed().as_millis() as u64,
    });
    for (name, read) in [("Codex account and models", codex_source as fn() -> ModelSource), ("Claude Code", claude_source), ("OpenCode", opencode_source), ("Ollama", ollama_source), ("LM Studio", lmstudio_source)] {
        let started = Instant::now();
        let source = read();
        let optional = source.provider != "codex" && source.status == SourceStatus::NotInstalled;
        checks.push(DiagnosticCheck {
            name: name.into(),
            ok: source.status == SourceStatus::Ready || optional,
            detail: format!("{} · {} models{}", source.connection, source.models.len(), source.default_model.as_ref().map(|d| format!(" · default {d}")).unwrap_or_default()),
            millis: started.elapsed().as_millis() as u64,
        });
    }
    checks.push(DiagnosticCheck {
        name: "Neko daemon".into(),
        ok: true,
        detail: format!("Version {} · data in {}", env!("CARGO_PKG_VERSION"), data_dir.display()),
        millis: 0,
    });
    // A fresh read: keep the picker in step with what was just measured.
    let _ = catalog(true);
    DiagnosticsReport { checks }
}

// ---------------------------------------------------------------- Scratch dir

/// A private, empty working folder removed on drop, so no project's
/// instructions or settings join a discovery or check.
struct Scratch(PathBuf);

impl Scratch {
    fn new(prefix: &str) -> Result<Self, String> {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let path = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
        std::fs::create_dir(&path).map_err(|e| format!("Couldn’t create a scratch folder: {e}"))?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_catalog_reports_plan_default_and_efforts() {
        let account = json!({"account": {"type": "chatgpt", "email": "x@example.com", "planType": "pro"}, "requiresOpenaiAuth": true});
        let pages = [json!({"data": [
            {"model": "gpt-a", "displayName": "GPT A", "isDefault": true, "supportedReasoningEfforts": [{"reasoningEffort": "low"}, {"reasoningEffort": "high"}]},
            {"model": "gpt-hidden", "hidden": true},
            {"model": "--flag"},
            {"model": "gpt-b"}
        ], "nextCursor": null})];
        let source = codex_catalog(&account, &pages);
        assert_eq!(source.connection, "ChatGPT · Pro");
        assert_eq!(source.status, SourceStatus::Ready);
        assert_eq!(source.default_model.as_deref(), Some("gpt-a"));
        assert_eq!(source.models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["gpt-a", "gpt-b"]);
        assert!(source.models[0].recommended);
        assert_eq!(source.models[0].reasoning_efforts, ["low", "high"]);
        assert!(!format!("{source:?}").contains("example.com"), "account email must never reach the catalog");
    }

    #[test]
    fn signed_out_codex_grays_out_every_model() {
        let account = json!({"account": null, "requiresOpenaiAuth": true});
        let source = codex_catalog(&account, &[json!({"data": [{"model": "gpt-a", "isDefault": true}]})]);
        assert_eq!(source.status, SourceStatus::SignInRequired);
        assert_eq!(source.models[0].access, ModelAccess::Unavailable);
        assert!(!source.models[0].recommended);
    }

    #[test]
    fn failures_map_to_actionable_text_without_leaking_detail() {
        let (unavailable, message) = friendly_failure("Codex exited with 1: 429 rate limit for sk-secret-123 at /Users/me/x");
        assert!(unavailable);
        assert!(message.starts_with("Usage limits"));
        assert!(!message.contains("sk-") && !message.contains("/Users"));
        assert!(friendly_failure("401 Unauthorized").0);
        assert!(friendly_failure("model_not_found").1.contains("isn’t available"));
        assert!(!friendly_failure("request timed out").0);
        assert!(friendly_failure("weird").1.contains("previous model is still selected"));
        // A timestamp containing "429" is not a rate limit.
        let real = "Codex exited with 1: 2026-10-02T00:10:49.071429Z  WARN codex_features: unknown feature\n{\"type\":\"error\",\"status\":400,\"error\":{\"message\":\"The 'gpt-x' model requires a newer version of Codex. Please upgrade to the latest app or CLI and try again.\"}}";
        assert!(friendly_failure(real).1.contains("newer Codex"), "{:?}", friendly_failure(real));
        assert!(!friendly_failure("at 12:04:29.1429 nothing happened").1.starts_with("Usage"));
        assert!(friendly_failure("{\"status\":429}").1.starts_with("Usage"));
        let trailing = "Codex exited with 1: 2026-10-02T00:10:49.071Z  WARN codex_rollout::list: falling_back {\"type\":\"error\",\"status\":400,\"error\":{\"message\":\"The 'x' model is not supported when using Codex with a ChatGPT account.\"}}";
        assert!(friendly_failure(trailing).1.contains("isn’t available"), "{:?}", friendly_failure(trailing));
    }

    #[test]
    fn claude_and_opencode_catalogs_parse() {
        let response = json!({"models": [
            {"value": "default", "displayName": "Default (recommended)", "resolvedModel": "claude-opus-5-5"},
            {"value": "opus", "displayName": "Opus 5.5", "resolvedModel": "claude-opus-5-5"},
            {"value": "haiku", "displayName": "Haiku 4.5", "resolvedModel": "claude-haiku-4-5"}
        ], "account": {"subscriptionType": "Claude Team", "email": "x@example.com"}});
        let source = claude_catalog(&response);
        assert_eq!(source.connection, "Claude Team");
        assert_eq!(source.default_model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(source.models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["opus", "haiku"]);
        assert!(source.models[0].recommended && !source.models[1].recommended);
        assert!(!format!("{source:?}").contains("example.com"));
        let signed_out = claude_catalog(&json!({"models": [{"value": "opus", "displayName": "Opus"}], "account": {"tokenSource": "none"}}));
        assert_eq!(signed_out.status, SourceStatus::SignInRequired);
        let oc = opencode_catalog("opencode/big-pickle\nopencode/mimo-v2.6-flash-free\nnot a model\nanthropic/claude-x\n");
        assert_eq!(oc.models.len(), 3);
        assert_eq!(oc.connection, "OpenCode · 2 providers");
        assert_eq!(oc.models[1].label, "mimo-v2.6-flash-free · opencode");
        assert_eq!(oc.models[1].description.as_deref(), Some("Free model"));
    }

    #[test]
    fn model_ids_cannot_carry_flags_or_whitespace() {
        assert!(valid_model_id("gpt-6-astra"));
        assert!(valid_model_id("qwen3:8b"));
        assert!(valid_model_id("anthropic/claude-sonnet-5"));
        assert!(!valid_model_id("-c"));
        assert!(!valid_model_id("a b"));
        assert!(!valid_model_id(""));
    }

    #[test]
    fn checks_mark_catalog_entries() {
        let mut checks = BTreeMap::new();
        checks.insert(key("", "gpt-a"), (true, "ok".to_string()));
        checks.insert(key("codex", "gpt-b"), (false, "No plan".to_string()));
        let catalog = ModelCatalog {
            read_at_ms: 0,
            sources: vec![codex_catalog(&json!({"account": {"type": "apiKey"}}), &[json!({"data": [{"model": "gpt-a"}, {"model": "gpt-b"}]})])],
        };
        let marked = with_checks(catalog, &checks);
        assert_eq!(marked.sources[0].models[0].access, ModelAccess::Checked);
        assert_eq!(marked.sources[0].models[1].access, ModelAccess::Unavailable);
        assert_eq!(marked.sources[0].models[1].reason.as_deref(), Some("No plan"));
    }

    /// Talks to the Codex CLI on this Mac; metadata only, no tokens.
    #[test]
    #[ignore = "Requires a local Codex CLI"]
    fn live_codex_discovery() {
        let source = discover_codex(&crate::native_runner::resolve_codex().unwrap(), DISCOVERY_TIMEOUT).unwrap();
        assert!(!source.models.is_empty(), "{source:?}");
        eprintln!("{} · {} models · default {:?}", source.connection, source.models.len(), source.default_model);
    }

    #[test]
    #[ignore = "Reads local runtimes"]
    fn live_catalog() {
        for (name, f) in [("ollama", ollama_source as fn() -> ModelSource), ("lmstudio", lmstudio_source), ("codex", codex_source)] {
            let started = Instant::now();
            let source = f();
            eprintln!("{name}: {:?} {} in {:?}", source.status, source.connection, started.elapsed());
        }
    }
}
