//! Read-only import discovery. Credentials remain in daemon memory and are
//! omitted from the serializable preview. Discovery never launches a server.
use neko_protocol::{
    mcp_host::ServerConfig,
    setup_import::{ImportCandidate, ImportCandidateKind, ImportConnection, ImportPreview},
    workbench::Secret,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_FILE: usize = 512 * 1024;
const MAX_CONNECTIONS: usize = 100;
const PREVIEW_SETTING: &str = "neko_import_preview_v1";

pub fn load_preview(db: &crate::Db) -> Result<ImportPreview, String> {
    db.get_setting(PREVIEW_SETTING)
        .map_err(|e| e.to_string())?
        .map(|value| serde_json::from_str(&value).map_err(|_| "Cannot read import preview".into()))
        .unwrap_or_else(|| Ok(ImportPreview::default()))
}

pub fn save_preview(db: &crate::Db, preview: &ImportPreview) -> Result<(), String> {
    let value = serde_json::to_string(preview).map_err(|e| e.to_string())?;
    if value.len() > 1024 * 1024 {
        return Err("Import preview exceeds its limit".into());
    }
    db.set_setting(PREVIEW_SETTING, &value)
        .map_err(|e| e.to_string())
}

pub struct Candidate {
    pub preview: ImportConnection,
    pub config: Option<ServerConfig>,
    pub credentials: Option<Secret>,
}

/// Read-only capability set handed to source adapters. It deliberately has no
/// process, network, or credential-store handle.
pub struct DiscoveryContext<'a> {
    home: &'a Path,
    repositories: &'a [PathBuf],
    paths: &'a [PathBuf],
    environment: &'a BTreeMap<String, String>,
}
impl<'a> DiscoveryContext<'a> {
    pub fn new(
        home: &'a Path,
        repositories: &'a [PathBuf],
        paths: &'a [PathBuf],
        environment: &'a BTreeMap<String, String>,
    ) -> Self {
        Self {
            home,
            repositories,
            paths,
            environment,
        }
    }
    pub fn home(&self) -> &Path {
        self.home
    }
    pub fn repositories(&self) -> &[PathBuf] {
        self.repositories
    }
    pub fn executable_paths(&self) -> &[PathBuf] {
        self.paths
    }
    pub fn environment(&self) -> &BTreeMap<String, String> {
        self.environment
    }
    pub fn read_text(&self, path: &Path) -> Result<String, String> {
        let candidate = path
            .canonicalize()
            .map_err(|_| "Import path is unavailable")?;
        let roots = std::iter::once(self.home)
            .chain(self.repositories.iter().map(PathBuf::as_path))
            .filter_map(|root| root.canonicalize().ok());
        if !roots.into_iter().any(|root| candidate.starts_with(root)) {
            return Err("Import path is outside the discovery roots".into());
        }
        file_text(&candidate)?.ok_or("Import file is unavailable".into())
    }
    fn canonical_directory(&self, path: &Path) -> Option<PathBuf> {
        let candidate = path.canonicalize().ok()?;
        if !candidate.is_dir() {
            return None;
        }
        let roots = std::iter::once(self.home)
            .chain(self.repositories.iter().map(PathBuf::as_path))
            .filter_map(|root| root.canonicalize().ok());
        roots
            .into_iter()
            .any(|root| candidate.starts_with(root))
            .then_some(candidate)
    }
}

/// Internal source seam. Implementations may only inspect the supplied
/// bounded context and append redacted records to a discovery session.
pub(crate) trait ImportSource {
    fn discover(&self, context: &DiscoveryContext<'_>, out: &mut Discovery);
}
impl std::fmt::Debug for Candidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Candidate")
            .field("preview", &self.preview)
            .field("configuration", &"[WITHHELD]")
            .finish()
    }
}

#[derive(Default)]
pub struct Discovery {
    pub schedules: Vec<neko_protocol::setup_import::ImportSchedule>,
    pub candidates: Vec<Candidate>,
    pub warnings: Vec<String>,
    pub repositories: Vec<String>,
}
impl Discovery {
    pub fn preview(&self) -> ImportPreview {
        let mut ledger = self
            .candidates
            .iter()
            .map(|candidate| ImportCandidate {
                id: candidate.preview.id.clone(),
                kind: ImportCandidateKind::Connection,
                source: candidate.preview.source.clone(),
                scope: candidate
                    .preview
                    .repository
                    .as_deref()
                    .map_or_else(|| "global".into(), |path| format!("workspace:{path}")),
                workspace: candidate.preview.repository.clone(),
                name: candidate.preview.name.clone(),
                metadata: BTreeMap::from([
                    (
                        "enabled_at_source".into(),
                        candidate.preview.enabled_at_source.to_string(),
                    ),
                    (
                        "has_credentials".into(),
                        candidate.preview.has_credentials.to_string(),
                    ),
                ]),
                problem: candidate.preview.problem.clone(),
            })
            .collect::<Vec<_>>();
        ledger.extend(self.schedules.iter().map(|schedule| {
            ImportCandidate {
                id: schedule.id.clone(),
                kind: ImportCandidateKind::Schedule,
                source: schedule.source.clone(),
                scope: schedule
                    .repository
                    .as_deref()
                    .map_or_else(|| "global".into(), |path| format!("workspace:{path}")),
                workspace: schedule.repository.clone(),
                name: schedule.name.clone(),
                metadata: BTreeMap::from([
                    ("timezone".into(), schedule.timezone.clone()),
                    ("rule".into(), schedule.rule.clone()),
                ]),
                problem: schedule.warnings.first().cloned(),
            }
        }));
        ledger.extend(self.repositories.iter().map(|path| {
            ImportCandidate {
                id: format!(
                    "workspace:{:x}",
                    Sha256::digest(format!("workspace\0{path}").as_bytes())
                ),
                kind: ImportCandidateKind::Workspace,
                source: "local".into(),
                scope: format!("workspace:{path}"),
                workspace: Some(path.clone()),
                name: Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(path)
                    .into(),
                metadata: BTreeMap::new(),
                problem: None,
            }
        }));
        ImportPreview {
            candidates: ledger,
            schedules: self.schedules.clone(),
            preview_id: String::new(),
            connections: self.candidates.iter().map(|c| c.preview.clone()).collect(),
            repositories: self.repositories.clone(),
            warnings: self.warnings.clone(),
        }
    }
}

fn file_text(path: &Path) -> Result<Option<String>, String> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Configuration cannot be read".into()),
    };
    if !file
        .metadata()
        .map_err(|_| "Configuration metadata unavailable")?
        .is_file()
    {
        return Err("Configuration is not a regular file".into());
    }
    let mut text = String::new();
    file.take((MAX_FILE + 1) as u64)
        .read_to_string(&mut text)
        .map_err(|_| "Configuration is not UTF-8")?;
    if text.len() > MAX_FILE {
        return Err("Configuration exceeds 512 KB".into());
    }
    Ok(Some(text))
}

fn expand(value: &str, environment: &BTreeMap<String, String>) -> Result<String, String> {
    fn append(out: &mut String, value: &str) -> Result<(), String> {
        if value.len() > 8192usize.saturating_sub(out.len()) {
            return Err("Expanded configuration value exceeds 8 KB".into());
        }
        out.push_str(value);
        Ok(())
    }
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        append(&mut out, &rest[..start])?;
        let tail = &rest[start + 2..];
        let end = tail.find('}').ok_or("Malformed environment reference")?;
        let expression = &tail[..end];
        let (key, default) = expression
            .split_once(":-")
            .map_or((expression, None), |(k, v)| (k, Some(v)));
        let replacement = environment.get(key).map(String::as_str).or(default).ok_or("A referenced environment variable is unavailable; reconnect or set it before importing")?;
        append(&mut out, replacement)?;
        rest = &tail[end + 1..];
    }
    append(&mut out, rest)?;
    Ok(out)
}

fn executable(
    command: &str,
    paths: &[PathBuf],
    repository: Option<&str>,
) -> Result<String, String> {
    use std::os::unix::fs::PermissionsExt;
    let path = Path::new(command);
    let choices = if path.is_absolute() {
        vec![path.to_owned()]
    } else if command.contains('/') {
        vec![Path::new(repository.ok_or("Relative executable requires a workspace")?).join(path)]
    } else {
        paths.iter().map(|p| p.join(command)).collect()
    };
    choices
        .into_iter()
        .find(|p| fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
        .and_then(|p| p.canonicalize().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or("Server executable is unavailable; install it before importing".into())
}

fn configuration(
    value: &Value,
    paths: &[PathBuf],
    environment: &BTreeMap<String, String>,
    repository: Option<&str>,
) -> Result<(ServerConfig, Option<Secret>), String> {
    fn environment_value(
        environment: &BTreeMap<String, String>,
        key: &str,
    ) -> Result<String, String> {
        let value = environment
            .get(key)
            .ok_or("Credential environment variable is unavailable")?;
        if value.len() > 8192 {
            return Err("Credential environment value exceeds 8 KB".into());
        }
        Ok(value.clone())
    }
    let entries = value
        .get("env")
        .and_then(Value::as_object)
        .map_or(0, |v| v.len())
        + value
            .get("env_vars")
            .and_then(Value::as_array)
            .map_or(0, |v| v.len());
    if entries > 32 {
        return Err("Server environment exceeds 32 entries".into());
    }
    fn env_insert(
        env: &mut BTreeMap<String, String>,
        key: &str,
        value: String,
    ) -> Result<(), String> {
        if key.len() > 128
            || value.len() > 8192
            || env.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>() + key.len() + value.len()
                > 32768
        {
            return Err("Server environment exceeds credential limits".into());
        }
        env.insert(key.into(), value);
        Ok(())
    }
    let mut env = BTreeMap::<String, String>::new();
    if let Some(values) = value.get("env") {
        for (key, value) in values.as_object().ok_or("Environment must be an object")? {
            env_insert(
                &mut env,
                key,
                expand(
                    value.as_str().ok_or("Environment value must be text")?,
                    environment,
                )?,
            )?;
        }
    }
    if let Some(values) = value.get("env_vars") {
        for key in values
            .as_array()
            .ok_or("Environment references must be a list")?
        {
            let key = key.as_str().ok_or("Invalid environment reference")?;
            let replacement = environment
                .get(key)
                .ok_or("A server environment variable is unavailable")?;
            if replacement.len() > 8192 {
                return Err("Server environment value exceeds 8 KB".into());
            }
            env_insert(
                &mut env,
                key,
                environment
                    .get(key)
                    .cloned()
                    .ok_or("A server environment variable is unavailable")?,
            )?;
        }
    }
    let mut bearer = None;
    for field in ["http_headers", "headers", "env_http_headers"] {
        if let Some(headers) = value.get(field) {
            for (name, value) in headers.as_object().ok_or("Headers must be an object")? {
                if !name.eq_ignore_ascii_case("authorization") {
                    return Err("Custom HTTP headers require manual configuration; no headers were discarded".into());
                }
                let value = value.as_str().ok_or("Header must be text")?;
                let value = if field == "env_http_headers" {
                    environment_value(environment, value)?
                } else {
                    expand(value, environment)?
                };
                bearer = Some(
                    value
                        .strip_prefix("Bearer ")
                        .ok_or("This authorization method requires reconnecting")?
                        .to_owned(),
                );
            }
        }
    }
    if let Some(key) = value.get("bearer_token_env_var").and_then(Value::as_str) {
        bearer = Some(environment_value(environment, key)?);
    }
    let config = if let Some(url) = value.get("url").and_then(Value::as_str) {
        if value.get("type").and_then(Value::as_str) == Some("sse") {
            return Err("Legacy SSE transport requires a Streamable HTTP server URL".into());
        }
        let url = expand(url, environment)?;
        // URL secrets cannot be transferred into the supported bearer store.
        if reqwest::Url::parse(&url).is_ok_and(|u| u.query().is_some()) {
            return Err("URLs with query parameters need manual review before import".into());
        }
        ServerConfig::Http { url }
    } else {
        if value.get("cwd").is_some() {
            return Err(
                "Server working directory requires manual configuration; it was not discarded"
                    .into(),
            );
        }
        let command = value
            .get("command")
            .and_then(Value::as_str)
            .ok_or("Missing server command or URL")?;
        let args = value
            .get("args")
            .map(|v| v.as_array().ok_or("Arguments must be a list"))
            .transpose()?
            .map(|values| {
                values
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .ok_or("Argument must be text".to_owned())
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        if repository.is_some() && !args.is_empty() {
            return Err(
                "Workspace server arguments require manual working-directory configuration".into(),
            );
        }
        if args.iter().any(|arg| {
            let path = arg.split_once('=').map_or(arg.as_str(), |(_, value)| value);
            path.starts_with("./") || path.starts_with("../")
        }) {
            return Err(
                "Relative server arguments require manual working-directory configuration".into(),
            );
        }
        if args.iter().any(|arg| {
            arg.contains("${")
                || [
                    "--token",
                    "--api-key",
                    "--api_key",
                    "--password",
                    "--secret",
                    "--authorization",
                ]
                .iter()
                .any(|flag| arg.to_ascii_lowercase().starts_with(flag))
        }) {
            return Err("Move argument credentials or environment substitutions into server environment configuration before import".into());
        }
        ServerConfig::Stdio {
            command: executable(command, paths, repository)?,
            args,
        }
    };
    crate::mcp_host::store::validate_config(&config)?;
    let secret = if bearer.is_some() || !env.is_empty() {
        let json = serde_json::json!({"bearer":bearer,"environment":env}).to_string();
        crate::mcp_host::credentials::parse(&json)?;
        Some(Secret(json))
    } else {
        None
    };
    Ok((config, secret))
}

fn add_servers(
    out: &mut Discovery,
    servers: Option<&Value>,
    source: &str,
    repository: Option<&str>,
    paths: &[PathBuf],
    environment: &BTreeMap<String, String>,
    disabled: Option<&Value>,
) {
    let Some(servers) = servers.and_then(Value::as_object) else {
        return;
    };
    for (name, value) in servers {
        if out.candidates.len() >= MAX_CONNECTIONS {
            out.warnings
                .push("Only the first 100 servers are shown".into());
            break;
        }
        let id = format!(
            "{:x}",
            Sha256::digest(format!("{source}\0{}\0{name}", repository.unwrap_or("")).as_bytes())
        );
        let result = configuration(value, paths, environment, repository);
        let (config, credentials, problem) = match result {
            Ok((c, s)) => (Some(c), s, None),
            Err(e) => (None, None, Some(e)),
        };
        let preview = ImportConnection {
            id,
            name: name.clone(),
            source: source.into(),
            repository: repository.map(str::to_owned),
            has_credentials: credentials.is_some(),
            enabled_at_source: value
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true)
                && !value
                    .get("disabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                && !disabled
                    .and_then(Value::as_array)
                    .is_some_and(|names| names.iter().any(|item| item.as_str() == Some(name))),
            problem,
        };
        out.candidates.push(Candidate {
            preview,
            config,
            credentials,
        });
    }
}

/// Caller supplies environment and executable search roots; no shell is run.
fn discover_codex_claude(context: &DiscoveryContext<'_>) -> Discovery {
    let home = context.home;
    let repositories = context.repositories;
    let paths = context.paths;
    let environment = context.environment;
    let mut out = Discovery::default();
    let mut files = vec![
        (home.join(".codex/config.toml"), None, true),
        (home.join(".claude.json"), None, false),
    ];
    for repository in repositories.iter().take(100) {
        let scope = Some(repository.to_string_lossy().into_owned());
        files.push((repository.join(".mcp.json"), scope.clone(), false));
        files.push((repository.join(".codex/config.toml"), scope, true));
    }
    let mut cursor = 0;
    while cursor < files.len() && cursor < 202 {
        let (file, repository, toml) = files[cursor].clone();
        cursor += 1;
        let source = file.to_string_lossy().into_owned();
        if !file.exists() {
            continue;
        }
        let text = match context.read_text(&file) {
            Ok(text) => text,
            Err(e) => {
                out.warnings.push(format!("{source}: {e}"));
                continue;
            }
        };
        let parsed: Result<Value, ()> = if toml {
            toml::from_str::<toml::Value>(&text)
                .map_err(|_| ())
                .and_then(|v| serde_json::to_value(v).map_err(|_| ()))
        } else {
            serde_json::from_str(&text).map_err(|_| ())
        };
        let Ok(value) = parsed else {
            out.warnings.push(format!(
                "{source}: invalid configuration (contents withheld)"
            ));
            continue;
        };
        add_servers(
            &mut out,
            value.get(if toml { "mcp_servers" } else { "mcpServers" }),
            &source,
            repository.as_deref(),
            paths,
            environment,
            value.get("disabledMcpServers"),
        );
        if let Some(projects) = value.get("projects").and_then(Value::as_object) {
            for (directory, project) in projects.iter().take(100) {
                if !Path::new(directory).is_absolute() {
                    continue;
                }
                if out.repositories.len() >= 100 && !out.repositories.contains(directory) {
                    continue;
                }
                if !out.repositories.contains(directory) {
                    out.repositories.push(directory.clone());
                }
                for (relative, is_toml) in [(".mcp.json", false), (".codex/config.toml", true)] {
                    let path = Path::new(directory).join(relative);
                    if files.len() < 202 && !files.iter().any(|(p, _, _)| *p == path) {
                        files.push((path, Some(directory.clone()), is_toml));
                    }
                }
                if !toml {
                    add_servers(
                        &mut out,
                        project.get("mcpServers"),
                        &source,
                        Some(directory),
                        paths,
                        environment,
                        project.get("disabledMcpServers"),
                    );
                }
            }
        }
        if let Some(repository) = repository {
            if !out.repositories.contains(&repository) {
                out.repositories.push(repository);
            }
        }
    }
    let (schedules, warnings) = crate::schedule_import::discover(home);
    for schedule in &schedules {
        if let Some(repository) = &schedule.repository {
            if !out.repositories.contains(repository) && out.repositories.len() < 100 {
                out.repositories.push(repository.clone());
            }
        }
    }
    out.schedules = schedules;
    out.warnings.extend(warnings);
    out
}

struct CodexClaudeSource;
impl ImportSource for CodexClaudeSource {
    fn discover(&self, context: &DiscoveryContext<'_>, out: &mut Discovery) {
        let found = discover_codex_claude(context);
        *out = found;
    }
}

struct PaseoSource;
impl ImportSource for PaseoSource {
    fn discover(&self, context: &DiscoveryContext<'_>, out: &mut Discovery) {
        let metadata = context.home.join(".paseo/projects/projects.json");
        if let Ok(text) = context.read_text(&metadata) {
            let Ok(entries) = serde_json::from_str::<Vec<Value>>(&text) else {
                out.warnings.push(
                    "Paseo project metadata is unsupported; no live state was imported".into(),
                );
                return;
            };
            for entry in entries
                .into_iter()
                .filter(|entry| entry.get("archivedAt").is_none_or(Value::is_null))
                .take(100)
            {
                let Some(path) = entry.get("rootPath").and_then(Value::as_str).map(str::trim)
                else {
                    continue;
                };
                let Some(path) = context.canonical_directory(Path::new(path)) else {
                    continue;
                };
                let path = path.to_string_lossy().into_owned();
                if out.repositories.contains(&path) {
                    continue;
                }
                out.repositories.push(path);
                if out.repositories.len() >= 100 {
                    break;
                }
            }
        }
        if context
            .home
            .join(".paseo/paseo.pid")
            .symlink_metadata()
            .is_ok()
        {
            out.warnings.push("Paseo live state is unsupported; no processes, transcripts, credentials, grants, or enabled state were imported".into());
        }
    }
}

pub fn discover(
    home: &Path,
    repositories: &[PathBuf],
    paths: &[PathBuf],
    environment: &BTreeMap<String, String>,
) -> Discovery {
    let context = DiscoveryContext::new(home, repositories, paths, environment);
    let mut out = Discovery::default();
    CodexClaudeSource.discover(&context, &mut out);
    PaseoSource.discover(&context, &mut out);
    for repository in repositories.iter().take(100) {
        let path = repository.to_string_lossy().into_owned();
        if !out.repositories.contains(&path) {
            out.repositories.push(path.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::setup_import::ImportCandidateKind;

    #[test]
    fn preview_ledger_has_stable_redacted_ids_and_scope_metadata() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join(".codex")).unwrap();
        fs::write(
            home.path().join(".codex/config.toml"),
            "[mcp_servers.global]\nurl='https://example.com/mcp'\nbearer_token_env_var='TOKEN'\n[projects.\"/repo\"]\ntrust_level='trusted'\n",
        )
        .unwrap();
        let first = discover(
            home.path(),
            &[],
            &[],
            &BTreeMap::from([("TOKEN".into(), "secret-value".into())]),
        )
        .preview();
        let second = discover(
            home.path(),
            &[],
            &[],
            &BTreeMap::from([("TOKEN".into(), "secret-value".into())]),
        )
        .preview();
        assert_eq!(first.candidates, second.candidates);
        assert!(first.candidates.iter().any(|candidate| {
            candidate.kind == ImportCandidateKind::Connection
                && candidate.source.contains(".codex/config.toml")
                && candidate.scope == "global"
        }));
        assert!(first.candidates.iter().all(|candidate| {
            !candidate.id.is_empty()
                && !candidate.id.contains("secret")
                && !serde_json::to_string(candidate)
                    .unwrap()
                    .contains("secret-value")
        }));
    }

    #[test]
    fn discovery_context_reads_only_bounded_roots() {
        let home = tempfile::tempdir().unwrap();
        fs::write(home.path().join("inside.json"), "{}").unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        let environment = BTreeMap::new();
        let context = DiscoveryContext::new(home.path(), &[], &[], &environment);
        assert_eq!(
            context.read_text(&home.path().join("inside.json")).unwrap(),
            "{}"
        );
        assert!(context.read_text(outside.path()).is_err());
    }

    #[test]
    fn paseo_projects_use_root_path_and_ignore_archived_missing_or_outside_entries() {
        let home = tempfile::tempdir().unwrap();
        let good = home.path().join("good");
        fs::create_dir(&good).unwrap();
        fs::create_dir_all(home.path().join(".paseo/projects")).unwrap();
        let missing = home.path().join("missing");
        fs::write(
            home.path().join(".paseo/projects/projects.json"),
            serde_json::json!([
                {"rootPath": good, "archivedAt": null},
                {"rootPath": missing, "archivedAt": null},
                {"rootPath": good, "archivedAt": "2026-01-01T00:00:00Z"}
            ])
            .to_string(),
        )
        .unwrap();
        let preview = discover(home.path(), &[], &[], &BTreeMap::new()).preview();
        let workspaces: Vec<_> = preview
            .candidates
            .iter()
            .filter(|c| c.kind == ImportCandidateKind::Workspace)
            .collect();
        assert_eq!(workspaces.len(), 1);
        assert_eq!(
            workspaces[0].workspace.as_deref(),
            good.canonicalize().unwrap().to_str()
        );
    }

    #[cfg(unix)]
    #[test]
    fn discovery_context_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.json"), "secret").unwrap();
        symlink(
            outside.path().join("secret.json"),
            home.path().join("link.json"),
        )
        .unwrap();
        let environment = BTreeMap::new();
        let context = DiscoveryContext::new(home.path(), &[], &[], &environment);
        assert!(context.read_text(&home.path().join("link.json")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn codex_and_claude_config_symlink_escape_is_not_read() {
        use std::os::unix::fs::symlink;
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(
            outside.path().join("config.json"),
            r#"{"mcpServers":{"escaped":{"url":"https://outside.example"}}}"#,
        )
        .unwrap();
        symlink(
            outside.path().join("config.json"),
            home.path().join(".claude.json"),
        )
        .unwrap();
        let result = discover(home.path(), &[], &[], &BTreeMap::new());
        assert!(result.candidates.is_empty());
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("outside the discovery roots"))
        );
    }
    #[test]
    fn fresh_workbench_snapshot_includes_separately_stored_import_preview() {
        let db = crate::Db::open_in_memory().unwrap();
        let preview = ImportPreview {
            preview_id: "reviewed".into(),
            repositories: vec!["/fixture".into()],
            ..Default::default()
        };
        save_preview(&db, &preview).unwrap();
        assert_eq!(crate::workbench::load(&db).unwrap().import_preview, preview);
    }
    #[test]
    fn project_configs_are_discovered_from_global_project_inventory() {
        let home = tempfile::tempdir().unwrap();
        let repository = home.path().join("repo");
        fs::create_dir_all(repository.join(".codex")).unwrap();
        fs::create_dir_all(home.path().join(".codex")).unwrap();
        fs::write(
            home.path().join(".codex/config.toml"),
            format!(
                "[projects.{}]\ntrust_level='trusted'\n",
                serde_json::to_string(&repository.to_string_lossy()).unwrap()
            ),
        )
        .unwrap();
        fs::write(
            repository.join(".codex/config.toml"),
            "[mcp_servers.scoped]\nurl='https://example.org/mcp'",
        )
        .unwrap();
        let result = discover(home.path(), &[], &[], &BTreeMap::new());
        assert_eq!(result.candidates.len(), 1);
        assert_eq!(
            result.candidates[0].preview.repository.as_deref(),
            repository.to_str()
        );
    }
    #[test]
    fn preserves_header_environment_and_project_disable_and_rejects_unsupported_cwd() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir(home.path().join(".codex")).unwrap();
        fs::write(home.path().join(".codex/config.toml"), "[mcp_servers.remote]\nurl='https://example.com/mcp'\n[mcp_servers.remote.env_http_headers]\nAuthorization='AUTH'\n[mcp_servers.local]\ncommand='/bin/sh'\ncwd='/repo'\n").unwrap();
        fs::write(home.path().join(".claude.json"), r#"{"projects":{"/repo":{"disabledMcpServers":["paused"],"mcpServers":{"paused":{"url":"https://example.org/mcp"}}}}}"#).unwrap();
        let out = discover(
            home.path(),
            &[],
            &[],
            &BTreeMap::from([("AUTH".into(), "Bearer secret-header".into())]),
        );
        let remote = out
            .candidates
            .iter()
            .find(|c| c.preview.name == "remote")
            .unwrap();
        assert!(remote.preview.has_credentials && remote.preview.problem.is_none());
        assert!(
            remote
                .credentials
                .as_ref()
                .unwrap()
                .0
                .contains("secret-header")
        );
        let local = out
            .candidates
            .iter()
            .find(|c| c.preview.name == "local")
            .unwrap();
        assert!(
            local
                .preview
                .problem
                .as_ref()
                .unwrap()
                .contains("working directory")
        );
        assert!(local.config.is_none());
        assert!(
            !out.candidates
                .iter()
                .find(|c| c.preview.name == "paused")
                .unwrap()
                .preview
                .enabled_at_source
        );
        assert!(
            !serde_json::to_string(&out.preview())
                .unwrap()
                .contains("secret-header")
        );
    }
    #[test]
    fn imports_global_and_project_scopes_without_exposing_credentials_in_preview() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir(home.path().join(".codex")).unwrap();
        fs::write(home.path().join(".codex/config.toml"), "[mcp_servers.global]\nurl='https://example.com/mcp'\nbearer_token_env_var='TOKEN'\n[projects.\"/repo\"]\ntrust_level='trusted'\n").unwrap();
        fs::write(home.path().join(".claude.json"), r#"{"projects":{"/repo":{"mcpServers":{"scoped":{"url":"https://example.org/mcp","headers":{"Authorization":"Bearer secret-two"}}}}}}"#).unwrap();
        let out = discover(
            home.path(),
            &[],
            &[],
            &BTreeMap::from([("TOKEN".into(), "secret-one".into())]),
        );
        assert_eq!(out.candidates.len(), 2);
        assert_eq!(out.candidates[0].preview.repository, None);
        assert_eq!(
            out.candidates[1].preview.repository.as_deref(),
            Some("/repo")
        );
        assert_eq!(out.repositories, ["/repo"]);
        assert!(out.candidates.iter().all(|c| c.credentials.is_some()));
        let preview = serde_json::to_string(&out.preview()).unwrap();
        assert!(!preview.contains("secret-one") && !preview.contains("secret-two"));
        assert!(!format!("{:?}", out.candidates).contains("secret-one"));
    }
    #[test]
    fn expanded_configuration_is_bounded() {
        let env = BTreeMap::from([("TOKEN".into(), "x".repeat(8192))]);
        assert!(expand(&"${TOKEN}".repeat(100), &env).is_err());
        assert!(expand(&"x".repeat(512 * 1024 + 1), &env).is_err());
    }

    #[test]
    fn relative_script_arguments_require_manual_configuration() {
        for argument in [
            "./server.py",
            "../server.js",
            "--config=./config.json",
            "server.py",
            "scripts/server.js",
            "--config=config.json",
            ".",
        ] {
            let result = configuration(
                &serde_json::json!({"command":"/bin/sh", "args":[argument]}),
                &[],
                &BTreeMap::new(),
                Some("/repo"),
            );
            assert!(result.is_err(), "accepted {argument}");
        }
    }

    #[test]
    fn environment_is_bounded_before_expansion() {
        let environment = BTreeMap::from([("TOKEN".into(), "x".repeat(8193))]);
        assert!(expand("${TOKEN}", &environment).is_err());
        let values: serde_json::Map<String, Value> = (0..33)
            .map(|i| (format!("KEY{i}"), Value::String("${TOKEN}".into())))
            .collect();
        let result = configuration(
            &serde_json::json!({"url":"https://example.org", "env":values}),
            &[],
            &environment,
            None,
        );
        assert!(result.unwrap_err().contains("32"));
    }

    #[test]
    fn malformed_or_unsupported_configuration_is_reported_without_secret_values() {
        let home = tempfile::tempdir().unwrap();
        fs::write(home.path().join(".claude.json"), r#"{"mcpServers":{"custom":{"url":"https://example.com","headers":{"X-Token":"private-sentinel"}},"missing":{"url":"https://example.com","headers":{"Authorization":"${MISSING}"}}}}"#).unwrap();
        let out = discover(home.path(), &[], &[], &BTreeMap::new());
        assert!(
            out.candidates
                .iter()
                .all(|c| c.preview.problem.is_some() && c.config.is_none())
        );
        assert!(
            !serde_json::to_string(&out.preview())
                .unwrap()
                .contains("private-sentinel")
        );
    }
}
