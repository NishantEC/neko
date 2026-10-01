//! Isolated, ephemeral Codex executions owned by Neko.
//!
//! Isolation here means independent execution state and constrained writes,
//! not confidentiality between workspaces: the legacy Codex sandbox modes
//! permit reads outside the selected directory. Do not describe this runner
//! as enforcing a filesystem read allowlist. Named permission profiles exist,
//! but must pass an OS-level denied-read probe before replacing this policy.
//! `--ignore-user-config` and `--ignore-rules` also do not disable skill
//! discovery. `skills.include_instructions=false` suppresses automatic skill
//! instructions; Neko injects only explicit workspace activations. This is not
//! a filesystem read privacy boundary.
//! Read-only runs cannot create temporary files, including tool caches. A
//! writable TMPDIR exception must not silently widen the review's policy.

use std::fs;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const MAX_PROMPT: usize = 256 * 1024;
const MAX_LINE: usize = 256 * 1024;
const MAX_STDOUT: usize = 4 * 1024 * 1024;
const MAX_STDERR: usize = 64 * 1024;
const MAX_ANSWER: usize = 64 * 1024;
const MAX_EVENT: usize = 2048;
// Neko owns the agent pool and tool grants. Ambient tools must not bypass its
// scoped bridge, even in ordinary writable task actors. All names are recognized
// by the supported CLI; ignore-user-config alone does not suppress built-ins.
const AMBIENT_DISABLED_FEATURES: &[&str] = &[
    "image_generation",
    "skill_search",
    "skill_mcp_dependency_install",
    "tool_suggest",
    "apps",
    "browser_use",
    "browser_use_external",
    "browser_use_full_cdp_access",
    "computer_use",
    "remote_plugin",
    "plugins",
    "goals",
    "hooks",
    "workspace_dependencies",
    "code_mode",
    "multi_agent",
    "multi_agent_v2",
    "memories",
    "chronicle",
    "in_app_browser",
    "in_app_chat",
    "in_app_local_automation",
    "in_app_updates",
    "artifact",
    "enable_mcp_apps",
    "request_permissions_tool",
    "default_mode_request_user_input",
    "standalone_web_search",
    "tool_call_mcp_elicitation",
    "auth_elicitation",
];
// code_mode_host transports legitimate shell/MCP calls on the supported CLI;
// disabling it for ordinary actors prevents their scoped tools from running.
const EXTRACTION_DISABLED_FEATURES: &[&str] = &[
    "shell_tool",
    "unified_exec",
    "view_image",
    "sleep_tool",
    "code_mode_host",
];

pub struct RunSpec {
    pub directory: PathBuf,
    pub prompt: String,
    pub writable: bool,
    pub timeout: Duration,
    pub runtime: neko_protocol::workbench::AgentRuntime,
}

/// Host capability only, never an upstream credential.
pub struct BridgeConfig {
    pub executable: PathBuf,
    pub socket: PathBuf,
    pub token: String,
}
fn configure_bridge(command: &mut Command, bridge: &BridgeConfig) -> Result<(), String> {
    if !bridge.executable.is_absolute() || !bridge.socket.is_absolute() || bridge.token.is_empty() {
        return Err("Invalid scoped bridge configuration".into());
    }
    let executable = serde_json::to_string(&bridge.executable.to_string_lossy())
        .map_err(|_| "Invalid bridge executable")?;
    command.args(["-c", &format!("mcp_servers.neko={{command={executable},args=[\"--mcp-bridge\"],env_vars=[\"NEKO_MCP_TOKEN\",\"NEKO_MCP_SOCKET\"],required=true,tool_timeout_sec=150,enabled_tools=[\"neko_list_tools\",\"neko_call_tool\"],default_tools_approval_mode=\"prompt\",tools={{neko_list_tools={{approval_mode=\"approve\"}},neko_call_tool={{approval_mode=\"approve\"}}}}}}")]);
    command.args(["-c", "shell_environment_policy.exclude=[\"NEKO_MCP_*\"]"]);
    command
        .env("NEKO_MCP_TOKEN", &bridge.token)
        .env("NEKO_MCP_SOCKET", &bridge.socket);
    Ok(())
}

pub fn run_with_bridge(
    spec: &RunSpec,
    bridge: &BridgeConfig,
    cancel: &AtomicBool,
    on_event: impl FnMut(String),
) -> Result<String, String> {
    run_configured(&resolve_codex()?, spec, Some(bridge), cancel, on_event)
}

pub fn run(
    spec: &RunSpec,
    cancel: &AtomicBool,
    on_event: impl FnMut(String),
) -> Result<String, String> {
    run_with_executable(&resolve_codex()?, spec, cancel, on_event)
}

fn run_with_executable(
    executable: &Path,
    spec: &RunSpec,
    cancel: &AtomicBool,
    on_event: impl FnMut(String),
) -> Result<String, String> {
    run_configured(executable, spec, None, cancel, on_event)
}

/// Extraction has no tool authority, no MCP bridge, and only supplied text.
pub fn extract(spec: &RunSpec, cancel: &AtomicBool) -> Result<String, String> {
    run_configured_mode(&resolve_codex()?, spec, None, cancel, |_| {}, true)
}

fn run_configured(
    executable: &Path,
    spec: &RunSpec,
    bridge: Option<&BridgeConfig>,
    cancel: &AtomicBool,
    on_event: impl FnMut(String),
) -> Result<String, String> {
    run_configured_mode(executable, spec, bridge, cancel, on_event, false)
}

fn run_configured_mode(
    executable: &Path,
    spec: &RunSpec,
    bridge: Option<&BridgeConfig>,
    cancel: &AtomicBool,
    mut on_event: impl FnMut(String),
    extraction: bool,
) -> Result<String, String> {
    if spec.prompt.trim().is_empty() || spec.prompt.len() > MAX_PROMPT {
        return Err(format!(
            "Task prompt must contain between 1 and {MAX_PROMPT} bytes"
        ));
    }
    let directory = spec
        .directory
        .canonicalize()
        .map_err(|e| format!("Task directory unavailable: {e}"))?;
    if !directory.is_dir() {
        return Err("Task directory is not a directory".into());
    }
    let mut command = Command::new(executable);
    command.arg("exec");
    match spec.runtime.provider.as_str() {
        "" | "codex" => {}
        "ollama" | "lmstudio" => {
            command.args(["--oss", "--local-provider", spec.runtime.provider.as_str()]);
        }
        "opencodex" => configure_opencodex(&mut command, opencodex_port()?),
        _ => return Err("Unsupported agent provider".into()),
    }
    if !spec.runtime.model.is_empty() {
        command.args(["-m", spec.runtime.model.as_str()]);
    }
    command
        .current_dir(&directory)
        .args([
            "--json",
            "--ephemeral",
            "--ignore-user-config",
            "--ignore-rules",
            "--skip-git-repo-check",
            "-s",
            if spec.writable {
                "workspace-write"
            } else {
                "read-only"
            },
            "-c",
            "approval_policy=\"never\"",
            "-c",
            "skills.include_instructions=false",
            "-c",
            "sandbox_workspace_write.network_access=false",
            "-c",
            "sandbox_workspace_write.exclude_tmpdir_env_var=true",
            "-c",
            "sandbox_workspace_write.exclude_slash_tmp=true",
            "-c",
            "web_search=\"disabled\"",
            "-C",
        ])
        .arg(&directory)
        .arg("-");
    if let Some(bridge) = bridge {
        configure_bridge(&mut command, bridge)?;
    }
    for feature in AMBIENT_DISABLED_FEATURES {
        command.args(["-c", &format!("features.{feature}=false")]);
    }
    command.args(["-c", "features.skip_host_skill_discovery=true"]);
    if extraction {
        if spec.writable || bridge.is_some() {
            return Err("Extraction cannot receive write or tool authority".into());
        }
        for feature in EXTRACTION_DISABLED_FEATURES {
            command.args(["-c", &format!("features.{feature}=false")]);
        }
    }
    // A GUI launch commonly inherits only /usr/bin:/bin. Also make node-based
    // CLI installs usable without executing a shell's startup files.
    let mut paths = executable
        .parent()
        .map(Path::to_owned)
        .into_iter()
        .collect::<Vec<_>>();
    paths.extend(executable_directories());
    if let Ok(path) = std::env::join_paths(paths) {
        command.env("PATH", path);
    }
    for name in [
        "CODEX_THREAD_ID",
        "CODEX_INTERNAL_ORIGINATOR_OVERRIDE",
        "PASEO_AGENT_ID",
        "PASEO_WORKSPACE_ID",
    ] {
        command.env_remove(name);
    }

    let mut pending = Vec::new();
    let mut answer = None;
    let mut completed = false;
    let mut failure = None;
    let mut parse_line = |line: &[u8]| -> Result<(), String> {
        if line.iter().all(u8::is_ascii_whitespace) {
            return Ok(());
        }
        let value: serde_json::Value = serde_json::from_slice(line)
            .map_err(|e| format!("Codex returned invalid JSON: {e}"))?;
        if extraction
            && matches!(
                value["type"].as_str(),
                Some("item.started" | "item.updated" | "item.completed")
            )
            && !matches!(
                value["item"]["type"].as_str(),
                Some("agent_message" | "reasoning" | "error")
            )
        {
            return Err("Extraction attempted a tool call; output discarded".into());
        }
        match value["type"].as_str() {
            Some("turn.completed") => completed = true,
            Some("turn.failed" | "error") => {
                let detail = value["error"]["message"]
                    .as_str()
                    .or_else(|| value["message"].as_str())
                    .unwrap_or("Codex reported a failed turn");
                failure = Some(bounded_text(detail, MAX_EVENT));
            }
            Some("item.completed") if value["item"]["type"] == "agent_message" => {
                if let Some(text) = value["item"]["text"].as_str() {
                    if text.len() > if extraction { 4096 } else { MAX_ANSWER } {
                        return Err("Codex answer exceeded the output limit".into());
                    }
                    answer = Some(text.to_owned());
                    on_event(bounded_text(text, MAX_EVENT));
                }
            }
            Some("item.started" | "item.updated" | "item.completed") => {
                let item = &value["item"];
                match item["type"].as_str() {
                    Some("error") => {
                        // Item errors include nonfatal CLI diagnostics (for
                        // example shortened skill descriptions). Only turn
                        // failure/top-level errors or process failure decide
                        // execution outcome; still surface this diagnostic.
                        on_event(bounded_text(
                            &format!(
                                "Advisory: {}",
                                item["message"]
                                    .as_str()
                                    .unwrap_or("Codex reported a diagnostic")
                            ),
                            MAX_EVENT,
                        ));
                    }
                    Some("command_execution") => {
                        on_event(bounded_text(
                            &format!("Command {}", item["status"].as_str().unwrap_or("running")),
                            MAX_EVENT,
                        ));
                        if value["type"].as_str() == Some("item.completed") {
                            on_event(format!(
                                "VERIFICATION_COMMAND {}",
                                serde_json::json!({
                                    "command": item["command"].as_str().unwrap_or(""),
                                    "exit_code": item["exit_code"],
                                    "output": bounded_text(item["aggregated_output"].as_str().unwrap_or(""), MAX_EVENT),
                                })
                            ));
                        }
                    }
                    Some("file_change") => on_event("Updating workspace files".into()),
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(())
    };
    let prompt = if extraction {
        spec.prompt.clone()
    } else {
        prompt_with_git(&spec.prompt)
    };
    let output = execute(command, prompt.as_bytes(), cancel, spec.timeout, |bytes| {
        for &byte in bytes {
            if byte == b'\n' {
                parse_line(&pending)?;
                pending.clear();
            } else {
                if pending.len() >= MAX_LINE {
                    return Err("Codex JSON line exceeded the output limit".into());
                }
                pending.push(byte);
            }
        }
        Ok(())
    })?;
    if !pending.is_empty() {
        parse_line(&pending)?;
    }
    if !output.status.success() {
        return Err(format!(
            "Codex exited with {}: {}{}",
            output.status,
            bounded_text(output.stderr.trim(), MAX_EVENT),
            failure
                .map(|message| format!(" {message}"))
                .unwrap_or_default()
        ));
    }
    if let Some(error) = failure {
        return Err(error);
    }
    if !completed {
        return Err("Codex exited without completing the turn".into());
    }
    answer
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| "Codex completed without a final answer".into())
}

pub fn create_worktree(repository: &Path, task_id: &str) -> Result<PathBuf, String> {
    create_worktree_cancellable(repository, task_id, &AtomicBool::new(false))
}

/// Cancellation also covers Git validation and checkout. Interrupted partial
/// worktrees are retained for inspection; this never resets or removes work.
pub fn create_worktree_cancellable(
    repository: &Path,
    task_id: &str,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    create_worktree_in_cancellable(
        repository,
        task_id,
        &neko_protocol::support_dir().join("task-worktrees"),
        cancel,
    )
}

#[cfg(test)]
fn create_worktree_in(repository: &Path, task_id: &str, root: &Path) -> Result<PathBuf, String> {
    create_worktree_in_cancellable(repository, task_id, root, &AtomicBool::new(false))
}

fn create_worktree_in_cancellable(
    repository: &Path,
    task_id: &str,
    root: &Path,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    if cancel.load(Ordering::Acquire) {
        return Err("Task cancelled".into());
    }
    if task_id.is_empty()
        || task_id.len() > 80
        || !task_id.as_bytes()[0].is_ascii_alphanumeric()
        || !task_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Invalid task id: use 1–80 letters, digits, hyphens or underscores".into());
    }
    let repository = repository
        .canonicalize()
        .map_err(|e| format!("Repository unavailable: {e}"))?;
    if git_output(&repository, &["rev-parse", "--is-inside-work-tree"], cancel)?.trim() != "true" {
        return Err("Choose a non-bare Git repository".into());
    }
    git_output(
        &repository,
        &["rev-parse", "--verify", "HEAD^{commit}"],
        cancel,
    )?;
    // Smudge/process filters are arbitrary repository-configured executables.
    // Disable every configured filter as well as hooks and fsmonitor before
    // materializing any files. Never run repository code outside the sandbox.
    let mut config_command = git_command(&repository);
    config_command.args([
        "config",
        "--name-only",
        "--get-regexp",
        "^filter\\..*\\.(clean|smudge|process|required)$",
    ]);
    let mut config = Vec::new();
    let output = execute(
        config_command,
        &[],
        cancel,
        Duration::from_secs(15),
        |chunk| {
            config.extend_from_slice(chunk);
            Ok(())
        },
    )?;
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(format!(
            "Cannot inspect Git filters: {}",
            bounded_text(&output.stderr, MAX_EVENT)
        ));
    }
    let config = String::from_utf8(config).map_err(|_| "Git filter names are not UTF-8")?;
    let mut add = git_command(&repository);
    for key in config.lines() {
        add.arg("-c").arg(format!(
            "{key}={}",
            if key.ends_with(".required") {
                "false"
            } else {
                ""
            }
        ));
    }
    if cancel.load(Ordering::Acquire) {
        return Err("Task cancelled".into());
    }
    fs::create_dir_all(root).map_err(|e| format!("Cannot create task worktree directory: {e}"))?;
    let root = root
        .canonicalize()
        .map_err(|e| format!("Cannot resolve worktree directory: {e}"))?;
    let path = root.join(task_id);
    // Reserve this exact destination atomically; an existing directory, file,
    // symlink or another task's checkout must never be reused or overwritten.
    fs::create_dir(&path)
        .map_err(|e| format!("Task worktree destination already exists or is unavailable: {e}"))?;
    add.args(["worktree", "add", "-b"])
        .arg(format!("codex/neko-{task_id}"))
        .arg(&path)
        .arg("HEAD");
    let output = execute(add, &[], cancel, Duration::from_secs(120), |_| Ok(()))?;
    if !output.status.success() {
        return Err(format!(
            "Git worktree creation failed; any partial work is preserved at {}: {}",
            path.display(),
            bounded_text(&output.stderr, MAX_EVENT)
        ));
    }
    Ok(path)
}

/// Fixed installed developer-tool candidates, resolved on the host before the
/// worker sandbox starts. No shell, xcrun process, PATH lookup, or cache write.
/// At most two local filesystem candidates are checked, with no child to hang.
fn developer_git() -> Option<PathBuf> {
    developer_git_from(&[
        Path::new("/Library/Developer/CommandLineTools/usr/bin/git"),
        Path::new("/Applications/Xcode.app/Contents/Developer/usr/bin/git"),
    ])
}

fn developer_git_from(candidates: &[&Path]) -> Option<PathBuf> {
    candidates
        .iter()
        .take(2)
        .filter(|path| path.is_absolute())
        .find_map(|path| {
            let canonical = path.canonicalize().ok()?;
            let text = canonical.to_str()?;
            if canonical == Path::new("/usr/bin/git")
                || text.len() > 512
                || text.chars().any(char::is_control)
            {
                return None;
            }
            let metadata = fs::metadata(&canonical).ok()?;
            (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0).then_some(canonical)
        })
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

fn prompt_with_git(prompt: &str) -> String {
    let instruction = match developer_git() {
        Some(git) => format!("Direct Git executable: {}. Use this absolute executable for Git commands, including within login shells; do not rely on PATH or the /usr/bin/git developer-tools launcher.", shell_quote(&git)),
        None => "No direct developer Git executable is available. The /usr/bin/git launcher may fail when its cache is not writable; report that failure as missing evidence.".into(),
    };
    format!(
        "{prompt}\n\nHost tool instruction: {instruction} Sandbox permissions are unchanged. Report real Git failures and missing diff evidence; never treat an error as a clean result.\n"
    )
}

fn git_command(directory: &Path) -> Command {
    let mut command =
        Command::new(developer_git().unwrap_or_else(|| PathBuf::from("/usr/bin/git")));
    command.current_dir(directory).args([
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "submodule.recurse=false",
    ]);
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
    ] {
        command.env_remove(name);
    }
    command.env("GIT_TERMINAL_PROMPT", "0");
    command
}

pub(crate) fn git_output(
    directory: &Path,
    args: &[&str],
    cancel: &AtomicBool,
) -> Result<String, String> {
    let mut command = if args.first() == Some(&"diff") {
        git_without_filters(directory, cancel)?
    } else {
        git_command(directory)
    };
    command.args(args);
    let mut bytes = Vec::new();
    let output = execute(command, &[], cancel, Duration::from_secs(15), |chunk| {
        if bytes.len() + chunk.len() > MAX_STDOUT {
            return Err("Git evidence exceeded the output limit".into());
        }
        bytes.extend_from_slice(chunk);
        Ok(())
    })?;
    if !output.status.success() {
        return Err(format!(
            "Git repository check failed: {}",
            bounded_text(&output.stderr, MAX_EVENT)
        ));
    }
    String::from_utf8(bytes).map_err(|_| "Git returned non-UTF-8 output".into())
}

// --no-textconv does not disable clean/process filters used when Git compares
// working files. Host evidence collection must never execute repository code.
fn git_without_filters(directory: &Path, cancel: &AtomicBool) -> Result<Command, String> {
    let names = git_output(directory, &["config", "--name-only", "--list"], cancel)?;
    let mut command = git_command(directory);
    for name in names.lines().filter(|n| {
        n.starts_with("filter.")
            && [".clean", ".smudge", ".process", ".required"]
                .iter()
                .any(|suffix| n.ends_with(suffix))
    }) {
        command.arg("-c").arg(format!(
            "{name}={}",
            if name.ends_with(".required") {
                "false"
            } else {
                ""
            }
        ));
    }
    Ok(command)
}

/// Host-observed tracked and untracked scope, including committed worker edits.
pub fn changed_files(
    directory: &Path,
    base: &str,
    cancel: &AtomicBool,
) -> Result<Vec<String>, String> {
    let tracked = git_output(
        directory,
        &[
            "diff",
            "--name-only",
            "--no-ext-diff",
            "--no-textconv",
            "-z",
            base,
            "--",
        ],
        cancel,
    )?;
    let untracked = git_output(
        directory,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        cancel,
    )?;
    let mut files: Vec<String> = tracked
        .split('\0')
        .chain(untracked.split('\0'))
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

pub fn head(directory: &Path, cancel: &AtomicBool) -> Result<String, String> {
    Ok(git_output(directory, &["rev-parse", "HEAD"], cancel)?
        .trim()
        .into())
}

/// Binary patch with untracked additions. Commands never invoke repository
/// hooks, external diff drivers, or a shell. Patch evidence stays in memory.
pub fn task_patch(directory: &Path, base: &str, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    task_patch_scoped(directory, base, &[], cancel)
}

pub fn task_patch_scoped(
    directory: &Path,
    base: &str,
    scope: &[String],
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let mut args = vec![
        "diff",
        "--binary",
        "--no-ext-diff",
        "--no-textconv",
        base,
        "--",
    ];
    args.extend(scope.iter().map(String::as_str));
    let mut patch = git_output(directory, &args, cancel)?.into_bytes();
    let untracked = git_output(
        directory,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        cancel,
    )?;
    for file in untracked.split('\0').filter(|f| !f.is_empty()) {
        if !scope.is_empty() && !scope.iter().any(|s| s == file) {
            continue;
        }
        let mut command = git_without_filters(directory, cancel)?;
        command.args([
            "diff",
            "--no-index",
            "--binary",
            "--no-ext-diff",
            "--no-textconv",
            "--",
            "/dev/null",
            file,
        ]);
        let output = execute(command, &[], cancel, Duration::from_secs(15), |chunk| {
            if patch.len() + chunk.len() > 4 * 1024 * 1024 {
                return Err("Subtask patch exceeds 4 MiB".into());
            }
            patch.extend_from_slice(chunk);
            Ok(())
        })?;
        if output.status.code() != Some(1) && !output.status.success() {
            return Err("Cannot capture untracked subtask diff".into());
        }
    }
    if patch.len() > 4 * 1024 * 1024 {
        return Err("Subtask patch exceeds 4 MiB".into());
    }
    Ok(patch)
}

pub fn apply_task_patch(directory: &Path, patch: &[u8], cancel: &AtomicBool) -> Result<(), String> {
    for check in [true, false] {
        let mut command = git_command(directory);
        command.args(["apply", "--binary"]);
        if check {
            command.arg("--check");
        }
        command.arg("-");
        let output = execute(command, patch, cancel, Duration::from_secs(30), |_| Ok(()))?;
        if !output.status.success() {
            return Err(format!(
                "Subtask integration conflict; all child worktrees preserved: {}",
                bounded_text(&output.stderr, MAX_EVENT)
            ));
        }
    }
    Ok(())
}

fn executable_directories() -> Vec<PathBuf> {
    let mut directories = std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p)
                .filter(|p| p.is_absolute())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        directories.extend([
            home.join(".local/bin"),
            home.join(".cargo/bin"),
            home.join(".npm-global/bin"),
            home.join("Library/pnpm"),
        ]);
    }
    directories.extend(
        [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/usr/bin",
            "/bin",
            "/Applications/Codex.app/Contents/Resources",
        ]
        .into_iter()
        .map(PathBuf::from),
    );
    directories
}

fn resolve_codex() -> Result<PathBuf, String> {
    let candidates = if let Some(path) = std::env::var_os("NEKO_CODEX_PATH") {
        vec![PathBuf::from(path)]
    } else {
        executable_directories()
            .into_iter()
            .map(|directory| directory.join("codex"))
            .collect()
    };
    candidates.into_iter().find(|path| path.is_absolute() && fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
        .ok_or_else(|| "Codex CLI is not installed or executable. Install Codex, or set NEKO_CODEX_PATH to its absolute path.".into())
}

fn configure_opencodex(command: &mut Command, port: u16) {
    command.args([
        "-c",
        "model_provider=\"opencodex\"",
        "-c",
        &format!("model_providers.opencodex={{name=\"OpenCodex Proxy\",base_url=\"http://127.0.0.1:{port}/v1\",wire_api=\"responses\",requires_openai_auth=true}}"),
    ]);
}

fn opencodex_port() -> Result<u16, String> {
    let mut directories = executable_directories();
    if let Some(home) = std::env::var_os("HOME") {
        let versions = PathBuf::from(home).join(".nvm/versions/node");
        if let Ok(entries) = fs::read_dir(versions) {
            directories.extend(entries.flatten().map(|entry| entry.path().join("bin")));
        }
    }
    let executable = directories.into_iter().map(|directory| directory.join("ocx"))
        .find(|path| path.is_absolute() && fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
        .ok_or("OpenCodex CLI is not installed")?;
    let output = Command::new(executable).args(["resolve", "--json"]).output()
        .map_err(|e| format!("Could not inspect OpenCodex: {e}"))?;
    if !output.status.success() || output.stdout.len() > 64 * 1024 {
        return Err("OpenCodex is unavailable; start its proxy and try again".into());
    }
    let state: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "OpenCodex returned an invalid status")?;
    if state["liveness"]["status"] != "live" {
        return Err("OpenCodex proxy is not running".into());
    }
    state["port"]["effective"].as_u64().and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port > 0).ok_or_else(|| "OpenCodex did not report a valid local port".into())
}

fn bounded_text(text: &str, max: usize) -> String {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

struct ProcessOutput {
    status: ExitStatus,
    stderr: String,
}

/// Owns a process group, including any shell/tool descendants. Unwinding a
/// callback, malformed output, cancellation and every I/O error all clean up.
struct ProcessGroup {
    child: Child,
    guardian_control: Option<UnixStream>,
    terminated: bool,
}
impl ProcessGroup {
    fn kill(&mut self) {
        if self.terminated {
            return;
        }
        self.terminated = true;
        if self.guardian_control.take().is_some() {
            // EOF asks the helper to kill its still-owned child group. Never
            // SIGKILL the helper first: that would bypass its cleanup.
            return;
        }
        // SAFETY: process_group(0) made this child's pid the group id. A
        // negative pid targets only that group. waitid(WNOWAIT) below keeps
        // its leader unreaped until after this signal, preventing PID reuse.
        unsafe {
            libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
        }
    }

    fn poll(&mut self) -> Result<Option<ExitStatus>, String> {
        // SAFETY: siginfo is initialized, and waitid observes only our child.
        // WNOWAIT is essential: group cleanup must happen before reaping.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                self.child.id(),
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result == -1 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                return Ok(None);
            }
            return Err(format!("Cannot observe child exit: {error}"));
        }
        // SAFETY: waitid initialized the platform's SIGCHLD fields.
        if unsafe { info.si_pid() } == 0 {
            return Ok(None);
        }
        self.kill();
        self.child
            .wait()
            .map(Some)
            .map_err(|e| format!("Cannot reap child: {e}"))
    }
}
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        self.kill();
        let _ = self.child.wait();
    }
}

const GUARD_ARGUMENT: &str = "--neko-native-runner-guard";
const GUARD_FD: RawFd = 3;

/// Call first in neko-daemon's main, before database/socket/thread startup:
/// `if neko_core::native_runner::run_guard_if_requested() { return; }`
///
/// The helper never loads configuration, auth or task storage. It receives
/// only executable arguments and inherited stdio, and owns the actual child
/// group. EOF on fd 3 proves the supervising daemon has exited, even on
/// SIGKILL. The helper cleans up before relinquishing its child's PID.
pub fn run_guard_if_requested() -> bool {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(GUARD_ARGUMENT)) {
        return false;
    }
    let result = args
        .next()
        .ok_or_else(|| "Guardian executable argument missing".to_owned())
        .and_then(|executable| {
            let mut command = Command::new(executable);
            command.args(args);
            guard_run(command)
        });
    match result {
        Ok(status) => std::process::exit(
            status
                .code()
                .unwrap_or_else(|| 128 + status.signal().unwrap_or(1)),
        ),
        Err(error) => {
            eprintln!("Neko process guardian: {error}");
            std::process::exit(125);
        }
    }
}

fn guard_run(mut command: Command) -> Result<ExitStatus, String> {
    // SAFETY: the private helper protocol transfers one control socket at
    // fd 3. Validate it first; ordinary daemon launches never take this path.
    if unsafe { libc::fcntl(GUARD_FD, libc::F_GETFD) } == -1 {
        return Err("Parent control descriptor is missing".into());
    }
    let mut control = unsafe { UnixStream::from_raw_fd(GUARD_FD) };
    control
        .peer_addr()
        .map_err(|e| format!("Invalid parent control socket: {e}"))?;
    // The builder must not inherit any guardian/control fd. Authentication
    // remains solely in Codex's own process and is never read here.
    if unsafe { libc::fcntl(GUARD_FD, libc::F_SETFD, libc::FD_CLOEXEC) } == -1 {
        return Err(format!(
            "Cannot protect control descriptor: {}",
            io::Error::last_os_error()
        ));
    }
    nonblocking(GUARD_FD)?;
    let parent_closed = |control: &mut UnixStream| -> Result<bool, String> {
        match control.read(&mut [0; 1]) {
            Ok(0) => Ok(true),
            Ok(_) => Err("Invalid guardian control message".into()),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(false)
            }
            Err(e) => Err(format!("Cannot read parent control socket: {e}")),
        }
    };
    if parent_closed(&mut control)? {
        return Err("Parent exited before child launch".into());
    }
    command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .process_group(0);
    let mut process = ProcessGroup {
        child: command
            .spawn()
            .map_err(|e| format!("Cannot launch guarded child: {e}"))?,
        guardian_control: None,
        terminated: false,
    };
    loop {
        if parent_closed(&mut control)? {
            process.kill();
            return process
                .child
                .wait()
                .map_err(|e| format!("Cannot reap cancelled child: {e}"));
        }
        if let Some(status) = process.poll()? {
            return Ok(status);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn guardian_command(command: &Command, executable: &Path, control_fd: RawFd) -> Command {
    let mut helper = Command::new(executable);
    helper
        .arg(GUARD_ARGUMENT)
        .arg(command.get_program())
        .args(command.get_args());
    if let Some(directory) = command.get_current_dir() {
        helper.current_dir(directory);
    }
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            helper.env(key, value);
        } else {
            helper.env_remove(key);
        }
    }
    // SAFETY: after fork use only async-signal-safe fcntl/dup2. The source
    // socket is retained through spawn, and only fd 3 crosses exec.
    unsafe {
        helper.pre_exec(move || {
            if libc::dup2(control_fd, GUARD_FD) == -1
                || libc::fcntl(GUARD_FD, libc::F_SETFD, 0) == -1
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    helper
}

fn nonblocking(fd: RawFd) -> Result<(), String> {
    // SAFETY: the live pipe owner retains fd for both fcntl calls, which do
    // not take ownership. O_NONBLOCK preserves all existing status flags.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags == -1 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) == -1 {
            return Err(format!(
                "Cannot configure process pipe: {}",
                io::Error::last_os_error()
            ));
        }
    }
    Ok(())
}

/// Poll bounded nonblocking pipes on the caller's worker thread. No pipe
/// reader or stdin writer can block cancellation or outlive the execution.
fn execute(
    command: Command,
    input: &[u8],
    cancel: &AtomicBool,
    timeout: Duration,
    stdout_chunk: impl FnMut(&[u8]) -> Result<(), String>,
) -> Result<ProcessOutput, String> {
    // Only the real daemon dispatches this helper flag before normal startup.
    // Library fixtures and other embedding hosts retain direct execution.
    let helper = std::env::current_exe()
        .ok()
        .filter(|path| path.file_stem().is_some_and(|name| name == "neko-daemon"));
    execute_with_guard(
        command,
        input,
        cancel,
        timeout,
        stdout_chunk,
        helper.as_deref(),
    )
}

fn execute_with_guard(
    mut command: Command,
    input: &[u8],
    cancel: &AtomicBool,
    timeout: Duration,
    mut stdout_chunk: impl FnMut(&[u8]) -> Result<(), String>,
    guardian: Option<&Path>,
) -> Result<ProcessOutput, String> {
    if cancel.load(Ordering::Acquire) {
        return Err("Task cancelled".into());
    }
    if timeout.is_zero() {
        return Err("Task timed out".into());
    }
    let start = Instant::now();
    let sockets = guardian
        .map(|_| UnixStream::pair())
        .transpose()
        .map_err(|e| format!("Cannot create guardian control socket: {e}"))?;
    if let (Some(helper), Some((reader, _))) = (guardian, sockets.as_ref()) {
        command = guardian_command(&command, helper, reader.as_raw_fd());
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut process = ProcessGroup {
        child: command
            .spawn()
            .map_err(|e| format!("Cannot start process: {e}"))?,
        guardian_control: sockets.map(|(_reader, writer)| writer),
        terminated: false,
    };
    let mut stdin = process.child.stdin.take();
    let mut stdout = process.child.stdout.take().expect("piped stdout");
    let mut stderr = process.child.stderr.take().expect("piped stderr");
    nonblocking(stdin.as_ref().expect("piped stdin").as_raw_fd())?;
    nonblocking(stdout.as_raw_fd())?;
    nonblocking(stderr.as_raw_fd())?;
    let mut written = 0;
    let mut total = 0;
    let mut errors = Vec::new();
    let mut stdout_eof = false;
    let mut stderr_eof = false;
    let mut status = None;
    let mut buffer = [0; 8192];
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err("Task cancelled".into());
        }
        if start.elapsed() >= timeout {
            return Err("Task timed out".into());
        }
        let mut progressed = false;
        if written == input.len() {
            stdin.take();
        }
        if let Some(stdin) = &mut stdin {
            match stdin.write(&input[written..input.len().min(written + 8192)]) {
                Ok(n) => {
                    written += n;
                    progressed |= n != 0;
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::Interrupted
                            | io::ErrorKind::BrokenPipe
                    ) => {}
                Err(e) => return Err(format!("Cannot write task prompt: {e}")),
            }
        }
        if !stdout_eof {
            match stdout.read(&mut buffer) {
                Ok(0) => stdout_eof = true,
                Ok(n) => {
                    total += n;
                    if total > MAX_STDOUT {
                        return Err("Process stdout exceeded the output limit".into());
                    }
                    stdout_chunk(&buffer[..n])?;
                    progressed = true;
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(format!("Cannot read process output: {e}")),
            }
        }
        if !stderr_eof {
            match stderr.read(&mut buffer) {
                Ok(0) => stderr_eof = true,
                Ok(n) => {
                    if errors.len() + n > MAX_STDERR {
                        return Err("Process stderr exceeded the output limit".into());
                    }
                    errors.extend_from_slice(&buffer[..n]);
                    progressed = true;
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(format!("Cannot read process errors: {e}")),
            }
        }
        if status.is_none() {
            status = process.poll()?;
            if status.is_some() {
                process.kill();
                stdin.take();
            }
        }
        if let Some(status) = status.filter(|_| stdout_eof && stderr_eof) {
            return Ok(ProcessOutput {
                status,
                stderr: String::from_utf8_lossy(&errors).into_owned(),
            });
        }
        if !progressed {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn scoped_bridge_configuration_does_not_put_capabilities_in_arguments() {
        let mut command = std::process::Command::new("codex");
        let bridge = super::BridgeConfig {
            executable: "/app/neko-daemon".into(),
            socket: "/tmp/neko.sock".into(),
            token: "synthetic-secret-token".into(),
        };
        super::configure_bridge(&mut command, &bridge).unwrap();
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let args = args.join(" ");
        assert!(args.contains("mcp_servers.neko"));
        assert!(args.contains("approval_mode=\"approve\""));
        assert!(args.contains("shell_environment_policy.exclude"));
        assert!(!args.contains("synthetic-secret-token"));
        assert!(!args.contains("danger-full-access"));
        assert!(command.get_envs().any(
            |(k, v)| k == "NEKO_MCP_TOKEN" && v.is_some_and(|v| v == "synthetic-secret-token")
        ));
    }
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use std::sync::atomic::Ordering;
    use std::time::Instant;
    use tempfile::TempDir;

    // These cases fork a nested test binary, then a guardian, then a shell
    // child. Five seconds is ample in isolation but not under Cargo's full
    // parallel workspace load; this is a fixture startup bound, not the
    // product timeout contract exercised by the tests.
    const GUARDIAN_FIXTURE_DEADLINE: Duration = Duration::from_secs(15);

    fn fixture(body: &str) -> (TempDir, PathBuf, RunSpec) {
        let temp = TempDir::new().unwrap();
        let executable = temp.path().join("fixture-codex");
        fs::write(&executable, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let spec = RunSpec {
            directory: temp.path().to_owned(),
            prompt: "A prompt with $(literal) and 'quotes'".into(),
            writable: false,
            timeout: Duration::from_secs(10),
            runtime: Default::default(),
        };
        (temp, executable, spec)
    }

    #[test]
    fn selected_runtime_reaches_codex_exec() {
        let (temp, executable, mut spec) = fixture(
            "printf '%s\\n' \"$@\" > arguments\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"ok\"}}' '{\"type\":\"turn.completed\"}'",
        );
        spec.runtime.provider = "ollama".into();
        spec.runtime.model = "qwen3:8b".into();
        run_with_executable(&executable, &spec, &AtomicBool::new(false), |_| {}).unwrap();
        let args = fs::read_to_string(temp.path().join("arguments")).unwrap();
        assert!(args.starts_with("exec\n--oss\n--local-provider\nollama\n-m\nqwen3:8b\n"));
    }

    #[test]
    fn opencodex_runtime_uses_only_loopback_proxy_configuration() {
        let mut command = Command::new("codex");
        configure_opencodex(&mut command, 10100);
        let args: Vec<_> = command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();
        assert_eq!(args[0..2], ["-c", "model_provider=\"opencodex\""]);
        assert!(args[3].contains("base_url=\"http://127.0.0.1:10100/v1\""));
        assert!(!args.join(" ").contains("token"));
    }

    #[test]
    fn developer_git_rejects_malformed_non_executable_and_unbounded_candidates() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("git");
        fs::write(&file, "#!/bin/sh\nexit 0\n").unwrap();
        assert!(developer_git_from(&[Path::new("relative"), &file]).is_none());
        assert!(developer_git_from(&[temp.path()]).is_none());
        fs::set_permissions(&file, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            developer_git_from(&[&file]),
            Some(file.canonicalize().unwrap())
        );
        assert!(
            developer_git_from(&[Path::new("/missing-one"), Path::new("/missing-two"), &file])
                .is_none()
        );
        let bad = temp.path().join("git\nunsafe");
        fs::copy(&file, &bad).unwrap();
        assert!(developer_git_from(&[&bad]).is_none());
        let shim = temp.path().join("shim");
        std::os::unix::fs::symlink("/usr/bin/git", &shim).unwrap();
        assert!(developer_git_from(&[&shim]).is_none());
        let started = Instant::now();
        for _ in 0..100 {
            developer_git_from(&[Path::new("/missing-one"), Path::new("/missing-two")]);
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "bounded local lookup launches no child process"
        );
    }

    #[test]
    fn developer_git_survives_login_shell_path_reset_and_preserves_real_failures() {
        let Some(git) = developer_git() else { return };
        let temp = TempDir::new().unwrap();
        assert!(
            Command::new(&git)
                .args(["init", "--quiet"])
                .arg(temp.path())
                .status()
                .unwrap()
                .success()
        );
        let mut command = Command::new("/bin/zsh");
        command.args([
            "-lc",
            &format!(
                "exec {} -C {} status --porcelain",
                shell_quote(&git),
                shell_quote(temp.path())
            ),
        ]);
        let result = execute(
            command,
            &[],
            &AtomicBool::new(false),
            Duration::from_secs(10),
            |_| Ok(()),
        )
        .unwrap();
        assert!(result.status.success());
        assert!(result.stderr.is_empty(), "{}", result.stderr);
        assert!(
            git_output(
                temp.path(),
                &["not-a-real-git-subcommand"],
                &AtomicBool::new(false)
            )
            .is_err()
        );
    }

    #[test]
    fn enabled_skill_instructions_reach_child_prompt_and_automatic_skills_are_disabled() {
        let (temp, executable, mut spec) = fixture(
            r#"
printf '%s\n' "$@" > arguments
cat > received_prompt
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"done"}}' '{"type":"turn.completed"}'
"#,
        );
        let skill_dir = temp.path().join("skill");
        fs::create_dir(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "UNIQUE_SKILL_INSTRUCTION: check the actual diff",
        )
        .unwrap();
        let available = crate::skills::discover(&[crate::skills::Root {
            path: skill_dir,
            source: "Test".into(),
            workspace_id: Some("one".into()),
        }]);
        let db = crate::Db::open_in_memory().unwrap();
        let skill = &available[0];
        let enabled = crate::skills::set_enabled(
            &db,
            &available,
            &["one".into(), "two".into()],
            "one",
            &skill.path,
            &skill.content_hash,
            true,
        )
        .unwrap();
        spec.prompt
            .push_str(&crate::skills::for_prompt(&enabled, &available, Some("one")).unwrap());
        run_with_executable(&executable, &spec, &AtomicBool::new(false), |_| {}).unwrap();
        assert!(
            fs::read_to_string(temp.path().join("received_prompt"))
                .unwrap()
                .contains("UNIQUE_SKILL_INSTRUCTION")
        );
        assert!(
            fs::read_to_string(temp.path().join("arguments"))
                .unwrap()
                .contains("skills.include_instructions=false")
        );
        let arguments = fs::read_to_string(temp.path().join("arguments")).unwrap();
        for feature in [
            "apps",
            "browser_use",
            "computer_use",
            "multi_agent",
            "hooks",
            "plugins",
            "remote_plugin",
            "skill_mcp_dependency_install",
        ] {
            assert!(
                arguments.contains(&format!("features.{feature}=false")),
                "Ambient {feature} bypasses Neko authority"
            );
        }
        assert!(
            !arguments.contains("features.shell_tool=false"),
            "Ordinary actors retain their scoped shell"
        );
        assert!(
            !arguments.contains("features.code_mode_host=false"),
            "Scoped shell and bridge calls require the CLI's code-mode host transport"
        );
        assert!(
            crate::skills::for_prompt(&enabled, &available, Some("two"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn memory_extraction_disables_tools_and_rejects_tool_receipts() {
        let (temp, executable, spec) = fixture(
            r#"
printf '%s\n' "$@" > arguments
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"{\"memories\":[]}"}}' '{"type":"turn.completed"}'
"#,
        );
        run_configured_mode(
            &executable,
            &spec,
            None,
            &AtomicBool::new(false),
            |_| {},
            true,
        )
        .unwrap();
        let args = fs::read_to_string(temp.path().join("arguments")).unwrap();
        for feature in [
            "shell_tool",
            "unified_exec",
            "view_image",
            "image_generation",
            "skill_search",
            "tool_suggest",
            "sleep_tool",
        ] {
            assert!(args.contains(&format!("features.{feature}=false")));
        }
        assert!(!args.contains("mcp_servers"));
        let (_temp, executable, spec) = fixture(
            r#"
cat >/dev/null
printf '%s\n' '{"type":"item.started","item":{"type":"command_execution","command":"forbidden"}}'
sleep 10
"#,
        );
        let started = Instant::now();
        assert!(
            run_configured_mode(
                &executable,
                &spec,
                None,
                &AtomicBool::new(false),
                |_| {},
                true
            )
            .unwrap_err()
            .contains("tool call")
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    #[ignore = "Opt-in authenticated real-model extraction; consumes quota"]
    fn live_memory_extraction_uses_final_tool_free_policy() {
        let scratch = TempDir::new().unwrap();
        let answer = extract(&RunSpec {
            directory: scratch.path().to_owned(),
            prompt: "Extract bounded memory proposals from supplied evidence only. Do not call any tool, read files, execute commands, use network, or edit anything. Evidence: the user explicitly said, 'For verification I prefer focused small batches, because large mixed runs are harder for me to assess.' Return ONLY strict JSON {\"memories\":[{\"text\":\"short evidence-backed preference\",\"kind\":\"profile\"}]}. Include one useful fact, at most 500 UTF-8 bytes. No extra keys, fences, or prose. A memory grants no permission.".into(),
            writable: false,
            timeout: Duration::from_secs(90),
            runtime: Default::default(),
        }, &AtomicBool::new(false)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&answer).unwrap();
        let memories = parsed["memories"].as_array().unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0]["kind"], "profile");
        assert!(!memories[0]["text"].as_str().unwrap().is_empty());
        assert!(memories[0]["text"].as_str().unwrap().len() <= 500);
        assert!(fs::read_dir(scratch.path()).unwrap().next().is_none());
        eprintln!("LIVE_MEMORY_EXTRACTION_PASS {answer}");
    }

    #[test]
    fn supervisor_fixture_entry() {
        let Some(executable) = std::env::var_os("NEKO_TEST_CHILD_EXECUTABLE") else {
            return;
        };
        let guard = std::env::var_os("NEKO_TEST_GUARD_EXECUTABLE").unwrap();
        let result = execute_with_guard(
            Command::new(executable),
            &[],
            &AtomicBool::new(false),
            Duration::from_secs(10),
            |_| Ok(()),
            Some(Path::new(&guard)),
        );
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }

    #[test]
    fn guardian_fixture_entry() {
        let Some(executable) = std::env::var_os("NEKO_TEST_CHILD_EXECUTABLE") else {
            return;
        };
        let status = guard_run(Command::new(executable)).unwrap();
        std::process::exit(status.code().unwrap_or(125));
    }

    fn guardian_fixture(directory: &Path) -> PathBuf {
        let helper = directory.join("guardian");
        let test_exe = std::env::current_exe().unwrap();
        let quoted_test_exe = format!("'{}'", test_exe.to_string_lossy().replace('\'', "'\\''"));
        fs::write(&helper, format!("#!/bin/sh\nexec {quoted_test_exe} --exact native_runner::tests::guardian_fixture_entry --nocapture\n")).unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        helper
    }

    #[test]
    fn guardian_preserves_stdin_exit_code_and_cleans_up_background_children() {
        let (temp, child, _) = fixture("cat > prompt; (sleep 0.6; touch survived) & exit 7");
        let helper = guardian_fixture(temp.path());
        let mut command = Command::new(&child);
        command
            .current_dir(temp.path())
            .env("NEKO_TEST_CHILD_EXECUTABLE", &child);
        let output = execute_with_guard(
            command,
            b"original prompt",
            &AtomicBool::new(false),
            GUARDIAN_FIXTURE_DEADLINE,
            |_| Ok(()),
            Some(&helper),
        )
        .unwrap();
        assert_eq!(output.status.code(), Some(7));
        assert_eq!(
            fs::read(temp.path().join("prompt")).unwrap(),
            b"original prompt"
        );
        std::thread::sleep(Duration::from_millis(750));
        assert!(!temp.path().join("survived").exists());
    }

    #[test]
    fn cancellation_of_guarded_process_returns_after_descendant_cleanup() {
        let (temp, child, _) = fixture("(sleep 0.6; touch survived) & sleep 20");
        let helper = guardian_fixture(temp.path());
        let mut command = Command::new(&child);
        command
            .current_dir(temp.path())
            .env("NEKO_TEST_CHILD_EXECUTABLE", &child);
        let cancel = AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(100));
                cancel.store(true, Ordering::Release);
            });
            let err = execute_with_guard(
                command,
                &[],
                &cancel,
                GUARDIAN_FIXTURE_DEADLINE,
                |_| Ok(()),
                Some(&helper),
            )
            .err()
            .unwrap();
            assert!(err.contains("cancelled"), "{err}");
        });
        std::thread::sleep(Duration::from_millis(750));
        assert!(!temp.path().join("survived").exists());
    }

    #[test]
    fn cancelled_worktree_creation_does_not_run_git_or_create_a_destination() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("worktrees");
        let err = create_worktree_in_cancellable(
            &temp.path().join("missing-repository"),
            "task-1",
            &root,
            &AtomicBool::new(true),
        )
        .unwrap_err();
        assert!(err.contains("cancelled"), "{err}");
        assert!(!root.exists());
    }

    #[test]
    fn supervisor_death_kills_guarded_descendants() {
        let temp = TempDir::new().unwrap();
        let child = temp.path().join("child");
        fs::write(
            &child,
            "#!/bin/sh\ntouch started\n(sleep 0.7; touch survived) &\nsleep 20\n",
        )
        .unwrap();
        fs::set_permissions(&child, fs::Permissions::from_mode(0o700)).unwrap();
        let helper = guardian_fixture(temp.path());
        let test_exe = std::env::current_exe().unwrap();
        let mut supervisor = Command::new(test_exe)
            .args([
                "--exact",
                "native_runner::tests::supervisor_fixture_entry",
                "--nocapture",
            ])
            .current_dir(temp.path())
            .env("NEKO_TEST_CHILD_EXECUTABLE", &child)
            .env("NEKO_TEST_GUARD_EXECUTABLE", &helper)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        while !temp.path().join("started").exists() && started.elapsed() < GUARDIAN_FIXTURE_DEADLINE
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        supervisor.kill().unwrap();
        supervisor.wait().unwrap();
        assert!(
            temp.path().join("started").exists(),
            "fixture child did not start"
        );
        std::thread::sleep(Duration::from_millis(850));
        assert!(
            !temp.path().join("survived").exists(),
            "builder descendant survived its daemon"
        );
    }

    #[test]
    fn extracts_final_message_and_passes_prompt_as_stdin_with_safe_flags() {
        let (temp, executable, spec) = fixture(
            r#"
printf '%s\n' "$@" > args
cat > prompt
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"First"}}'
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"Finished"}}'
printf '%s\n' '{"type":"turn.completed"}'
"#,
        );
        let mut events = vec![];
        let result = run_with_executable(&executable, &spec, &AtomicBool::new(false), |e| {
            events.push(e)
        });
        assert_eq!(result.unwrap(), "Finished");
        assert!(
            fs::read_to_string(temp.path().join("prompt"))
                .unwrap()
                .contains("Direct Git executable:"),
            "each model run needs the validated developer Git path, not a PATH hint"
        );
        let received = fs::read_to_string(temp.path().join("prompt")).unwrap();
        assert!(received.starts_with(&spec.prompt));
        assert!(received.contains(&shell_quote(&developer_git().unwrap())));
        let args = fs::read_to_string(temp.path().join("args")).unwrap();
        for expected in [
            "exec",
            "--json",
            "--ephemeral",
            "--ignore-user-config",
            "--ignore-rules",
            "read-only",
            "approval_policy=\"never\"",
            "sandbox_workspace_write.network_access=false",
        ] {
            assert!(
                args.lines().any(|arg| arg == expected),
                "Missing {expected}: {args}"
            );
        }
        assert!(!events.is_empty());
    }

    #[test]
    fn refuses_nonzero_exit_even_after_a_final_message() {
        let (_temp, exe, spec) = fixture(
            r#"printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"Partial"}}'; echo 'authentication failed' >&2; exit 7"#,
        );
        let err = run_with_executable(&exe, &spec, &AtomicBool::new(false), |_| {}).unwrap_err();
        assert!(
            err.contains("7") && err.contains("authentication failed"),
            "{err}"
        );
    }

    #[test]
    fn failure_event_is_not_success_even_with_zero_exit() {
        let (_temp, exe, spec) = fixture(
            r#"printf '%s\n' '{"type":"turn.failed","error":{"message":"quota exhausted"}}'"#,
        );
        let err = run_with_executable(&exe, &spec, &AtomicBool::new(false), |_| {}).unwrap_err();
        assert!(err.contains("quota exhausted"), "{err}");
    }

    #[test]
    fn item_error_advisory_does_not_fail_a_completed_turn() {
        let (_temp, exe, spec) = fixture(
            r#"
printf '%s\n' '{"type":"item.completed","item":{"type":"error","message":"Skill descriptions were shortened to fit the skills context budget. Codex can still see every skill, but some descriptions are shorter. Disable unused skills or plugins to leave more room for the rest."}}'
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"Correct final answer"}}'
printf '%s\n' '{"type":"turn.completed"}'
"#,
        );
        let mut events = Vec::new();
        let answer = run_with_executable(&exe, &spec, &AtomicBool::new(false), |event| {
            events.push(event)
        })
        .unwrap();
        assert_eq!(answer, "Correct final answer");
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("Advisory: Skill descriptions were shortened"))
        );
    }

    #[test]
    fn terminal_errors_stay_fatal_even_after_a_final_answer_and_completion() {
        for error in [
            r#"{"type":"error","message":"transport failed"}"#,
            r#"{"type":"turn.failed","error":{"message":"transport failed"}}"#,
        ] {
            let (_temp, exe, spec) = fixture(&format!(
                r#"
printf '%s\n' '{{"type":"item.completed","item":{{"type":"agent_message","text":"Partial answer"}}}}'
printf '%s\n' '{{"type":"turn.completed"}}'
printf '%s\n' '{error}'
"#
            ));
            let error =
                run_with_executable(&exe, &spec, &AtomicBool::new(false), |_| {}).unwrap_err();
            assert!(error.contains("transport failed"), "{error}");
        }
    }

    #[test]
    fn an_item_advisory_without_a_completed_turn_still_fails() {
        let (_temp, exe, spec) = fixture(
            r#"printf '%s\n' '{"type":"item.completed","item":{"type":"error","message":"Warning only"}}'"#,
        );
        let error = run_with_executable(&exe, &spec, &AtomicBool::new(false), |_| {}).unwrap_err();
        assert!(error.contains("without completing"), "{error}");
    }

    #[test]
    fn nonzero_exit_preserves_the_structured_failure_reason() {
        let (_temp, exe, spec) = fixture(
            r#"printf '%s\n' '{"type":"turn.failed","error":{"message":"quota exhausted"}}'; exit 1"#,
        );
        let err = run_with_executable(&exe, &spec, &AtomicBool::new(false), |_| {}).unwrap_err();
        assert!(err.contains("quota exhausted"), "{err}");
    }

    #[test]
    fn missing_completion_and_missing_final_answer_are_errors() {
        for body in ["exit 0", r#"printf '%s\n' '{"type":"turn.completed"}'"#] {
            let (_temp, exe, spec) = fixture(body);
            assert!(run_with_executable(&exe, &spec, &AtomicBool::new(false), |_| {}).is_err());
        }
    }

    #[test]
    fn oversized_unterminated_stdout_is_bounded() {
        let (_temp, exe, spec) = fixture("head -c 400000 /dev/zero | tr '\\000' x");
        let err = run_with_executable(&exe, &spec, &AtomicBool::new(false), |_| {}).unwrap_err();
        assert!(err.contains("limit"), "{err}");
    }

    #[test]
    fn stderr_flood_is_bounded() {
        let (_temp, exe, spec) = fixture("head -c 200000 /dev/zero >&2; sleep 10");
        let err = run_with_executable(&exe, &spec, &AtomicBool::new(false), |_| {}).unwrap_err();
        assert!(err.contains("limit"), "{err}");
    }

    #[test]
    fn timeout_kills_descendants_and_returns_promptly() {
        let (temp, exe, mut spec) = fixture("(sleep 0.8; touch survived) & sleep 10");
        spec.timeout = Duration::from_millis(120);
        let start = Instant::now();
        let err = run_with_executable(&exe, &spec, &AtomicBool::new(false), |_| {}).unwrap_err();
        assert!(err.contains("timed out"), "{err}");
        assert!(start.elapsed() < Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(850));
        assert!(!temp.path().join("survived").exists());
    }

    #[test]
    fn cancellation_interrupts_a_child_that_never_reads_stdin() {
        let (_temp, exe, mut spec) = fixture("sleep 10");
        spec.prompt = "x".repeat(60000);
        let cancel = AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(100));
                cancel.store(true, Ordering::Release);
            });
            let start = Instant::now();
            let err = run_with_executable(&exe, &spec, &cancel, |_| {}).unwrap_err();
            assert!(err.contains("cancelled"), "{err}");
            assert!(start.elapsed() < Duration::from_secs(2));
        });
    }

    #[test]
    fn cancellation_before_launch_does_not_spawn_the_process() {
        let (temp, exe, spec) = fixture("touch spawned");
        let err = run_with_executable(&exe, &spec, &AtomicBool::new(true), |_| {}).unwrap_err();
        assert!(err.contains("cancelled"), "{err}");
        assert!(!temp.path().join("spawned").exists());
    }

    #[test]
    fn host_diff_never_runs_repository_clean_filters() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path();
        git(repo, &["init", "--quiet"]);
        fs::write(repo.join(".gitattributes"), "*.txt filter=evil\n").unwrap();
        fs::write(repo.join("a.txt"), "before\n").unwrap();
        git(repo, &["add", "."]);
        git(
            repo,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-qm",
                "initial",
            ],
        );
        git(
            repo,
            &["config", "filter.evil.clean", "touch filter-ran; cat"],
        );
        fs::write(repo.join("a.txt"), "after\n").unwrap();
        let cancel = AtomicBool::new(false);
        assert_eq!(changed_files(repo, "HEAD", &cancel).unwrap(), vec!["a.txt"]);
        assert!(
            !repo.join("filter-ran").exists(),
            "Host diff executed repository code outside the worker sandbox"
        );
        task_patch(repo, "HEAD", &cancel).unwrap();
        assert!(!repo.join("filter-ran").exists());
    }

    #[test]
    fn allows_256_kib_prompt_without_losing_bytes() {
        let (temp, executable, mut spec) = fixture(
            r#"
cat > prompt
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"Done"}}'
printf '%s\n' '{"type":"turn.completed"}'
"#,
        );
        spec.prompt = "x".repeat(256 * 1024);
        assert_eq!(
            run_with_executable(&executable, &spec, &AtomicBool::new(false), |_| {}).unwrap(),
            "Done"
        );
        let received = fs::read_to_string(temp.path().join("prompt")).unwrap();
        assert!(received.starts_with(&spec.prompt));
        assert!(received.len() <= MAX_PROMPT + 2048);
    }

    #[test]
    fn rejects_empty_or_over_limit_prompts_before_launch() {
        let (temp, exe, mut spec) = fixture("touch spawned");
        for prompt in [" ".into(), "x".repeat(256 * 1024 + 1)] {
            spec.prompt = prompt;
            let err =
                run_with_executable(&exe, &spec, &AtomicBool::new(false), |_| {}).unwrap_err();
            assert!(err.contains("prompt"), "{err}");
            assert!(!temp.path().join("spawned").exists());
        }
    }

    fn git(repo: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    #[test]
    fn worktree_is_clean_and_does_not_copy_source_edits_or_run_hooks() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repository");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        fs::write(repo.join("file.txt"), "committed\n").unwrap();
        git(&repo, &["add", "file.txt"]);
        git(
            &repo,
            &[
                "-c",
                "user.name=Neko Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-qm",
                "fixture",
            ],
        );
        fs::write(repo.join("file.txt"), "user changes\n").unwrap();
        let hook = repo.join(".git/hooks/post-checkout");
        fs::write(&hook, "#!/bin/sh\ntouch hook-ran\n").unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
        let root = temp.path().join("worktrees");
        let path = create_worktree_in(&repo, "task-123", &root).unwrap();
        assert_eq!(
            fs::read_to_string(path.join("file.txt")).unwrap(),
            "committed\n"
        );
        assert_eq!(
            fs::read_to_string(repo.join("file.txt")).unwrap(),
            "user changes\n"
        );
        assert_eq!(git(&path, &["status", "--porcelain"]), "");
        assert_eq!(
            git(&path, &["branch", "--show-current"]).trim(),
            "codex/neko-task-123"
        );
        assert!(!path.join("hook-ran").exists());
        assert!(create_worktree_in(&repo, "task-123", &root).is_err());
    }

    #[test]
    fn rejects_unsafe_ids_and_non_repositories_without_creating_directories() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("worktrees");
        for id in ["", "../outside", "-option", "a/b", "with space", "ok"] {
            assert!(create_worktree_in(temp.path(), id, &root).is_err());
        }
        assert!(!root.exists());
    }

    #[test]
    fn checkout_does_not_execute_repository_filters() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repository");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        fs::write(repo.join(".gitattributes"), "*.txt filter=probe\n").unwrap();
        fs::write(repo.join("file.txt"), "committed\n").unwrap();
        git(&repo, &["add", "."]);
        git(
            &repo,
            &[
                "-c",
                "user.name=Neko Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-qm",
                "fixture",
            ],
        );
        git(
            &repo,
            &["config", "filter.probe.smudge", "touch filter-ran; cat"],
        );
        git(&repo, &["config", "filter.probe.required", "true"]);
        let path =
            create_worktree_in(&repo, "filter-test", &temp.path().join("worktrees")).unwrap();
        assert!(!path.join("filter-ran").exists());
        assert_eq!(
            fs::read_to_string(path.join("file.txt")).unwrap(),
            "committed\n"
        );
    }

    #[test]
    fn writable_sandbox_is_explicit_and_completion_needs_no_final_newline() {
        let (temp, executable, mut spec) = fixture(
            r#"
printf '%s\n' "$@" > args
cat > /dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"Done"}}'
printf '%s' '{"type":"turn.completed"}'
"#,
        );
        spec.writable = true;
        assert_eq!(
            run_with_executable(&executable, &spec, &AtomicBool::new(false), |_| {}).unwrap(),
            "Done"
        );
        let args = fs::read_to_string(temp.path().join("args")).unwrap();
        assert!(args.lines().any(|arg| arg == "workspace-write"));
        assert!(args.contains("exclude_slash_tmp=true"));
        assert!(args.contains("exclude_tmpdir_env_var=true"));
    }

    #[test]
    fn repeated_valid_events_also_have_a_total_output_limit() {
        let (_temp, exe, spec) = fixture("yes '{\"type\":\"unknown\"}' | head -c 5000000");
        let err = run_with_executable(&exe, &spec, &AtomicBool::new(false), |_| {}).unwrap_err();
        assert!(err.contains("limit"), "{err}");
    }
}
