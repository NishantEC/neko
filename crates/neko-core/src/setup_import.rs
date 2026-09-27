//! Read-only import discovery. Credentials remain in daemon memory and are
//! omitted from the serializable preview. Discovery never launches a server.
use neko_protocol::{
    mcp_host::ServerConfig,
    setup_import::{
        ImportCandidate, ImportCandidateKind, ImportConnection, ImportPreview, ImportSourceInfo,
    },
    workbench::Secret,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_FILE: usize = 512 * 1024;
const MAX_CONNECTIONS: usize = 100;
const PREVIEW_SETTING: &str = "neko_import_preview_v1";

/// A bounded presence check for the source picker. No source file is read here.
pub fn available_sources(home: &Path) -> Vec<ImportSourceInfo> {
    [
        (
            "codex",
            "Codex",
            &[".codex/config.toml", ".codex/skills", ".codex/automations"] as &[_],
        ),
        ("claude", "Claude", &[".claude.json", ".claude/skills"]),
        (
            "paseo",
            "Paseo",
            &[".paseo/projects/projects.json", ".paseo/paseo.pid"],
        ),
        ("agents", "Local skills", &[".agents/skills"]),
    ]
    .into_iter()
    .filter(|(_, _, paths)| {
        paths
            .iter()
            .any(|path| home.join(path).symlink_metadata().is_ok())
    })
    .map(|(id, name, _)| ImportSourceInfo {
        id: id.into(),
        name: name.into(),
    })
    .collect()
}

pub fn read_skill_for_import(path: &str, expected_hash: &str) -> Result<String, String> {
    let body = file_text(Path::new(path))?.ok_or("Skill file is unavailable")?;
    if format!("{:x}", Sha256::digest(body.as_bytes())) != expected_hash {
        return Err("Skill changed; rediscover it before importing".into());
    }
    Ok(body)
}

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

/// Resolve one definition from its authoritative source for one workspace.
/// The returned secret stays in daemon memory; this function never launches it.
pub fn resolve_linked_source(
    home: &Path,
    workspace: &Path,
    paths: &[PathBuf],
    environment: &BTreeMap<String, String>,
    candidate_id: &str,
) -> Result<Candidate, String> {
    let workspace = workspace
        .canonicalize()
        .map_err(|_| "Workspace folder is unavailable")?;
    if !workspace.is_dir() {
        return Err("Workspace folder is unavailable".into());
    }
    // Tool dispatch needs only MCP definitions. Do not scan skills, schedules,
    // Paseo metadata, or other Claude project folders on every guard check.
    let repositories = [workspace.clone()];
    let context = DiscoveryContext::new(home, &repositories, paths, environment);
    let discovery = discover_codex_claude(&context, None, true);
    let candidate = discovery
        .candidates
        .into_iter()
        .find(|candidate| candidate.preview.id == candidate_id)
        .ok_or("Source MCP definition is unavailable")?;
    if let Some(repository) = &candidate.preview.repository {
        if Path::new(repository).canonicalize().ok().as_deref() != Some(workspace.as_path()) {
            return Err("Source MCP definition belongs to another workspace".into());
        }
    }
    if !candidate.preview.enabled_at_source {
        return Err("Source MCP definition is disabled".into());
    }
    if let Some(problem) = &candidate.preview.problem {
        return Err(format!("Source MCP definition needs attention: {problem}"));
    }
    if candidate.config.is_none() {
        return Err("Source MCP definition is unsupported".into());
    }
    Ok(candidate)
}

/// Stable identity of the supported transport fields, excluding source auth.
pub fn config_identity(config: &ServerConfig) -> String {
    let bytes = serde_json::to_vec(config).expect("ServerConfig is serializable");
    format!("{:x}", Sha256::digest(bytes))
}

/// Detect replacement or in-place modification of a trusted local launcher.
pub fn executable_identity(config: &ServerConfig) -> Result<Option<String>, String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let ServerConfig::Stdio { command, args, cwd } = config else {
        return Ok(None);
    };
    let path = Path::new(command)
        .canonicalize()
        .map_err(|_| "Linked MCP executable is unavailable")?;
    let metadata = fs::metadata(&path).map_err(|_| "Linked MCP executable is unavailable")?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err("Linked MCP executable is unavailable".into());
    }
    let mut identity = format!(
        "{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
        path.display(),
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
        metadata.mode()
    );
    // A launcher such as node or python is only half of the trusted program.
    // Track file arguments too, so changing its script invalidates prior trust.
    for argument in args.iter().take(64) {
        let value = argument
            .rsplit_once('=')
            .map_or(argument.as_str(), |(_, value)| value);
        let candidate = Path::new(value);
        let has_script_extension = candidate
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension,
                    "js" | "mjs" | "cjs" | "ts" | "py" | "json" | "toml" | "yaml" | "yml"
                )
            });
        if !candidate.is_absolute() && !has_script_extension {
            continue;
        }
        let candidate = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else if let Some(cwd) = cwd {
            Path::new(cwd).join(candidate)
        } else {
            continue;
        };
        let path = match candidate.canonicalize() {
            Ok(path) => path,
            Err(_) if has_script_extension => return Err("Linked MCP script is unavailable".into()),
            Err(_) => continue,
        };
        let metadata = fs::metadata(&path).map_err(|_| "Linked MCP script is unavailable")?;
        if !metadata.is_file() {
            continue;
        }
        use std::fmt::Write;
        write!(
            identity,
            "\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
            path.display(),
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.ctime(),
            metadata.ctime_nsec(),
            metadata.mode()
        )
        .expect("String formatting cannot fail");
    }
    Ok(Some(format!("{:x}", Sha256::digest(identity.as_bytes()))))
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
    pub skills: Vec<neko_protocol::skills::Skill>,
    pub warnings: Vec<String>,
    pub repositories: Vec<String>,
}
impl Discovery {
    pub fn preview(&self) -> ImportPreview {
        let mut connection_groups: Vec<Vec<&Candidate>> = Vec::new();
        for candidate in &self.candidates {
            if let Some(group) = connection_groups
                .iter_mut()
                .find(|group| same_connection_definition(group[0], candidate))
            {
                group.push(candidate);
            } else {
                connection_groups.push(vec![candidate]);
            }
        }
        let mut ledger = connection_groups
            .iter()
            .map(|group| {
                let candidate = group[0];
                ImportCandidate {
                    id: candidate.preview.id.clone(),
                    kind: ImportCandidateKind::Connection,
                    source: display_sources(
                        group
                            .iter()
                            .map(|candidate| candidate.preview.source.as_str()),
                    ),
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
                        (
                            "transport".into(),
                            match candidate.config {
                                Some(ServerConfig::Stdio { .. }) => "stdio",
                                Some(ServerConfig::Http { .. }) => "http",
                                None => "unsupported",
                            }
                            .into(),
                        ),
                        (
                            "config_summary".into(),
                            match candidate.config.as_ref() {
                                Some(ServerConfig::Http { url }) => reqwest::Url::parse(url)
                                    .map_or_else(
                                        |_| "Remote MCP server".into(),
                                        |mut parsed| {
                                            let _ = parsed.set_username("");
                                            let _ = parsed.set_password(None);
                                            parsed.set_query(None);
                                            parsed.set_fragment(None);
                                            parsed.to_string()
                                        },
                                    ),
                                Some(ServerConfig::Stdio { command, args, cwd }) => format!(
                                    "{} · {} arguments · working folder {}",
                                    command,
                                    args.len(),
                                    cwd.as_deref().unwrap_or("default")
                                ),
                                None => "Unsupported configuration".into(),
                            },
                        ),
                        (
                            "config_fingerprint".into(),
                            candidate
                                .config
                                .as_ref()
                                .map_or_else(String::new, |config| {
                                    config_identity(config).chars().take(12).collect()
                                }),
                        ),
                    ]),
                    problem: candidate.preview.problem.clone(),
                }
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
            let (review_group, review_reason, problem) = workspace_review(Path::new(path));
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
                metadata: BTreeMap::from([
                    ("review_group".into(), review_group.into()),
                    ("review_reason".into(), review_reason.into()),
                ]),
                problem,
            }
        }));
        let mut skill_groups: Vec<Vec<&neko_protocol::skills::Skill>> = Vec::new();
        for skill in &self.skills {
            if let Some(group) = skill_groups.iter_mut().find(|group| {
                let known = group[0];
                known.workspace_id == skill.workspace_id
                    && known.name == skill.name
                    && known.content_hash == skill.content_hash
            }) {
                group.push(skill);
            } else {
                skill_groups.push(vec![skill]);
            }
        }
        ledger.extend(skill_groups.iter().map(|group| {
            let skill = group[0];
            let id = format!(
                "skill:{:x}",
                Sha256::digest(format!("skill\0{}\0{}", skill.path, skill.content_hash).as_bytes())
            );
            let (scope, workspace) = skill
                .workspace_id
                .as_deref()
                .map(|path| (format!("workspace:{path}"), Some(path.to_owned())))
                .unwrap_or_else(|| ("global".into(), None));
            ImportCandidate {
                id,
                kind: ImportCandidateKind::Skill,
                source: display_sources(group.iter().map(|skill| skill.source.as_str())),
                scope,
                workspace,
                name: skill.name.clone(),
                metadata: BTreeMap::from([
                    ("path".into(), skill.path.clone()),
                    ("content_hash".into(), skill.content_hash.clone()),
                    ("description".into(), skill.description.clone()),
                ]),
                problem: None,
            }
        }));
        ImportPreview {
            sources: Vec::new(),
            active_source: None,
            candidates: ledger,
            schedules: self.schedules.clone(),
            preview_id: String::new(),
            connections: self.candidates.iter().map(|c| c.preview.clone()).collect(),
            repositories: self.repositories.clone(),
            warnings: self.warnings.clone(),
        }
    }
}

/// A source is provenance, not a distinct thing to install. We retain all
/// matching labels for review while one representative is imported.
fn display_sources<'a>(sources: impl IntoIterator<Item = &'a str>) -> String {
    let labels = sources
        .into_iter()
        .map(display_source)
        .collect::<BTreeSet<_>>();
    labels.into_iter().collect::<Vec<_>>().join(" + ")
}

fn display_source(source: &str) -> String {
    if source.ends_with("/.codex/config.toml") {
        "Codex".into()
    } else if source.ends_with("/.claude.json") {
        "Claude".into()
    } else if source.ends_with("/.mcp.json") {
        "MCP config".into()
    } else {
        source.into()
    }
}

fn same_connection_definition(left: &Candidate, right: &Candidate) -> bool {
    left.preview.name == right.preview.name
        && left.preview.repository == right.preview.repository
        && left.config == right.config
        && left.preview.problem == right.preview.problem
        && match (&left.credentials, &right.credentials) {
            (None, None) => true,
            (Some(left), Some(right)) => left.0 == right.0,
            _ => false,
        }
}

fn canonical_repositories(repositories: &[PathBuf]) -> Vec<PathBuf> {
    let mut normalized = Vec::new();
    for repository in repositories {
        let repository = repository
            .canonicalize()
            .unwrap_or_else(|_| repository.clone());
        if !normalized.iter().any(|known| known == &repository) {
            normalized.push(repository);
        }
    }
    normalized
}

fn workspace_review(path: &Path) -> (&'static str, &'static str, Option<String>) {
    if !path.is_dir() {
        return (
            "other",
            "This folder is no longer on this Mac",
            Some("Folder no longer exists".into()),
        );
    }
    let codex_date = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let bytes = name.as_bytes();
            bytes.len() == 10
                && bytes[4] == b'-'
                && bytes[7] == b'-'
                && bytes
                    .iter()
                    .enumerate()
                    .all(|(i, byte)| i == 4 || i == 7 || byte.is_ascii_digit())
        });
    let codex_parent = path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .is_some_and(|name| name == "Codex");
    let documents_parent = path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .is_some_and(|name| name == "Documents");
    if codex_date && codex_parent && documents_parent {
        return ("other", "One-off Codex output folder", None);
    }
    let uuid = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let groups: Vec<_> = name.split('-').collect();
            groups.iter().map(|part| part.len()).collect::<Vec<_>>() == [8, 4, 4, 4, 12]
                && groups
                    .iter()
                    .all(|part| part.bytes().all(|byte| byte.is_ascii_hexdigit()))
        });
    let design_project = path
        .parent()
        .is_some_and(|parent| parent.file_name().is_some_and(|name| name == "projects"))
        && path
            .ancestors()
            .nth(2)
            .is_some_and(|parent| parent.file_name().is_some_and(|name| name == "data"))
        && path
            .ancestors()
            .nth(4)
            .is_some_and(|parent| parent.file_name().is_some_and(|name| name == "namespaces"))
        && path
            .ancestors()
            .nth(5)
            .is_some_and(|parent| parent.file_name().is_some_and(|name| name == "Open Design"))
        && path.ancestors().nth(6).is_some_and(|parent| {
            parent
                .file_name()
                .is_some_and(|name| name == "Application Support")
        })
        && path
            .ancestors()
            .nth(7)
            .is_some_and(|parent| parent.file_name().is_some_and(|name| name == "Library"));
    if uuid && design_project {
        return ("other", "Generated design project folder", None);
    }
    let temp_root = std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir());
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if canonical.starts_with(temp_root)
        && path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("neko-workbench-smoke-"))
    {
        return ("other", "Neko test folder", None);
    }
    let git_marker = path.join(".git");
    if git_marker
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_file())
    {
        if let Ok(file) = fs::File::open(&git_marker) {
            let mut marker = String::new();
            if file.take(4096).read_to_string(&mut marker).is_ok()
                && marker.starts_with("gitdir:")
                && marker.contains("/worktrees/")
            {
                return ("other", "Git worktree for another project", None);
            }
        }
    }
    ("primary", "Project folder", None)
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
    home: &Path,
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
        let cwd = value
            .get("cwd")
            .map(|value| {
                let value = value
                    .as_str()
                    .ok_or("Working directory must be text".to_owned())?;
                let value = expand(value, environment)?;
                let path = Path::new(&value);
                let path = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    repository.map(Path::new).unwrap_or(home).join(path)
                };
                path.canonicalize()
                    .ok()
                    .filter(|path| path.is_dir())
                    .and_then(|path| path.to_str().map(str::to_owned))
                    .ok_or("Server working directory is unavailable".to_owned())
            })
            .transpose()?
            .or_else(|| {
                repository
                    .and_then(|repository| Path::new(repository).canonicalize().ok())
                    .filter(|path| path.is_dir())
                    .and_then(|path| path.to_str().map(str::to_owned))
            });
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
        if cwd.is_none()
            && args.iter().any(|arg| {
                let path = arg.split_once('=').map_or(arg.as_str(), |(_, value)| value);
                path == "."
                    || path.starts_with("./")
                    || path.starts_with("../")
                    || matches!(
                        Path::new(path)
                            .extension()
                            .and_then(|extension| extension.to_str()),
                        Some("js" | "mjs" | "cjs" | "ts" | "py" | "json" | "toml" | "yaml" | "yml")
                    ) && !Path::new(path).is_absolute()
            })
        {
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
            cwd,
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
    home: &Path,
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
        let result = configuration(value, paths, environment, repository, home);
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
fn discover_codex_claude(
    context: &DiscoveryContext<'_>,
    source_filter: Option<&str>,
    linked_only: bool,
) -> Discovery {
    let home = context.home;
    let repositories = context.repositories;
    let paths = context.paths;
    let environment = context.environment;
    let mut out = Discovery::default();
    let mut files = Vec::new();
    if source_filter.is_none_or(|source| source == "codex") {
        files.push((home.join(".codex/config.toml"), None, true));
    }
    if source_filter.is_none_or(|source| source == "claude") {
        files.push((home.join(".claude.json"), None, false));
    }
    for repository in repositories.iter().take(100) {
        let scope = Some(repository.to_string_lossy().into_owned());
        if source_filter.is_none_or(|source| source == "claude") {
            files.push((repository.join(".mcp.json"), scope.clone(), false));
        }
        if source_filter.is_none_or(|source| source == "codex") {
            files.push((repository.join(".codex/config.toml"), scope, true));
        }
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
            context.home(),
            value.get("disabledMcpServers"),
        );
        if let Some(projects) = value.get("projects").and_then(Value::as_object) {
            for (directory, project) in projects.iter().take(100) {
                if linked_only
                    && !repositories.iter().any(|repository| {
                        Path::new(directory).canonicalize().ok().as_deref()
                            == Some(repository.as_path())
                    })
                {
                    continue;
                }
                // Project inventory remains importable for its config files;
                // skill roots are separately bounded below to caller/home roots.
                if out.repositories.len() >= 100 && !out.repositories.contains(directory) {
                    continue;
                }
                if !out.repositories.contains(directory) {
                    out.repositories.push(directory.clone());
                }
                for (relative, is_toml) in [(".mcp.json", false), (".codex/config.toml", true)] {
                    if source_filter.is_some_and(|filter| (filter == "codex") != is_toml) {
                        continue;
                    }
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
                        context.home(),
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
    let (schedules, warnings) =
        if !linked_only && source_filter.is_none_or(|source| source == "codex") {
            crate::schedule_import::discover(home)
        } else {
            (Vec::new(), Vec::new())
        };
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
    discover_filtered(home, repositories, paths, environment, None).unwrap_or_default()
}

pub fn discover_source(
    home: &Path,
    repositories: &[PathBuf],
    paths: &[PathBuf],
    environment: &BTreeMap<String, String>,
    source: &str,
) -> Result<Discovery, String> {
    discover_filtered(home, repositories, paths, environment, Some(source))
}

fn discover_filtered(
    home: &Path,
    repositories: &[PathBuf],
    paths: &[PathBuf],
    environment: &BTreeMap<String, String>,
    source: Option<&str>,
) -> Result<Discovery, String> {
    if source.is_some_and(|source| !matches!(source, "codex" | "claude" | "paseo" | "agents")) {
        return Err("Unknown import source".into());
    }
    let repositories = canonical_repositories(repositories);
    let context = DiscoveryContext::new(home, &repositories, paths, environment);
    let mut out = Discovery::default();
    if source.is_none_or(|source| matches!(source, "codex" | "claude")) {
        out = discover_codex_claude(&context, source, false);
    }
    if source.is_none_or(|source| source == "paseo") {
        PaseoSource.discover(&context, &mut out);
    }
    if source.is_none() {
        for repository in repositories.iter().take(100) {
            let path = repository.to_string_lossy().into_owned();
            if !out.repositories.contains(&path) {
                out.repositories.push(path.clone());
            }
        }
    }
    // Sources may spell the same real directory differently (notably /var
    // versus /private/var on macOS). Normalize after all adapters contribute
    // before turning repositories into workspace and skill roots.
    out.repositories = canonical_repositories(
        &out.repositories
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>(),
    )
    .into_iter()
    .map(|path| path.to_string_lossy().into_owned())
    .collect();
    let workspace_roots = out
        .repositories
        .iter()
        .take(100)
        .filter_map(|repository| {
            let path = PathBuf::from(repository);
            let canonical = path.canonicalize().ok()?;
            let home_root = home.canonicalize().ok()?;
            // Home can be a workspace, but its skill directories are already
            // scanned as global roots above. Never scan those files twice.
            if canonical == home_root {
                return None;
            }
            let caller_root = repositories
                .iter()
                .filter_map(|root| root.canonicalize().ok())
                .any(|root| canonical.starts_with(root));
            (canonical.starts_with(home_root) || caller_root)
                .then_some((repository.clone(), canonical))
        })
        .collect::<Vec<_>>();
    let mut roots = Vec::new();
    for (directory, root_source) in [
        (".codex/skills", "Codex"),
        (".agents/skills", "Agents"),
        (".claude/skills", "Claude"),
    ] {
        if source.is_some_and(|source| !root_source.eq_ignore_ascii_case(source)) {
            continue;
        }
        roots.push(crate::skills::Root {
            path: home.join(directory),
            source: root_source.into(),
            workspace_id: None,
        });
    }
    for (workspace_id, repository) in workspace_roots {
        for directory in [
            ".agents/skills",
            ".claude/skills",
            ".codex/skills",
            ".neko/skills",
        ] {
            if source.is_some_and(|source| !directory.starts_with(&format!(".{source}/"))) {
                continue;
            }
            roots.push(crate::skills::Root {
                path: repository.join(directory),
                source: "Workspace".into(),
                workspace_id: Some(workspace_id.clone()),
            });
        }
    }
    let allowed_roots = roots
        .iter()
        .filter_map(|root| root.path.canonicalize().ok())
        .collect::<Vec<_>>();
    out.skills = crate::skills::discover(&roots)
        .into_iter()
        .filter(|skill| {
            let path = Path::new(&skill.path);
            allowed_roots.iter().any(|root| path.starts_with(root))
        })
        .collect();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::setup_import::ImportCandidateKind;

    #[test]
    fn linked_source_resolves_only_selected_workspace_and_detects_changes() {
        let home = tempfile::tempdir().unwrap();
        let one = home.path().join("one");
        let two = home.path().join("two");
        fs::create_dir_all(&one).unwrap();
        fs::create_dir_all(&two).unwrap();
        fs::write(
            one.join(".mcp.json"),
            r#"{"mcpServers":{"local":{"url":"https://one.example/mcp"}}}"#,
        )
        .unwrap();
        let first = discover(home.path(), &[one.clone()], &[], &BTreeMap::new());
        let id = first.candidates[0].preview.id.clone();
        let linked = resolve_linked_source(home.path(), &one, &[], &BTreeMap::new(), &id).unwrap();
        assert_eq!(linked.preview.name, "local");
        assert!(resolve_linked_source(home.path(), &two, &[], &BTreeMap::new(), &id).is_err());
        fs::write(
            one.join(".mcp.json"),
            r#"{"mcpServers":{"local":{"url":"https://changed.example/mcp"}}}"#,
        )
        .unwrap();
        let changed = resolve_linked_source(home.path(), &one, &[], &BTreeMap::new(), &id).unwrap();
        assert_ne!(linked.config, changed.config);
        fs::remove_file(one.join(".mcp.json")).unwrap();
        assert!(resolve_linked_source(home.path(), &one, &[], &BTreeMap::new(), &id).is_err());
    }

    #[test]
    fn trusted_launcher_identity_tracks_script_arguments() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("launcher");
        let script = directory.path().join("server.mjs");
        fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(&script, "version one").unwrap();
        let config = ServerConfig::Stdio {
            command: executable.to_string_lossy().into_owned(),
            args: vec![script.to_string_lossy().into_owned()],
            cwd: None,
        };
        let before = executable_identity(&config).unwrap();
        fs::write(&script, "version two has changed").unwrap();
        assert_ne!(before, executable_identity(&config).unwrap());
    }

    #[test]
    fn discovery_summary_never_displays_url_credentials() {
        let preview = Discovery {
            candidates: vec![Candidate {
                preview: ImportConnection {
                    id: "remote".into(),
                    name: "remote".into(),
                    source: "source".into(),
                    repository: None,
                    has_credentials: false,
                    enabled_at_source: true,
                    problem: None,
                },
                config: Some(ServerConfig::Http {
                    url: "https://user:password@example.com/mcp?token=secret#fragment".into(),
                }),
                credentials: None,
            }],
            ..Discovery::default()
        }
        .preview();
        let summary = preview.candidates[0]
            .metadata
            .get("config_summary")
            .unwrap();
        assert_eq!(summary, "https://example.com/mcp");
    }

    #[test]
    fn source_scan_reads_only_the_selected_source() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join(".codex/skills/codex-only")).unwrap();
        fs::create_dir_all(home.path().join(".claude/skills/claude-only")).unwrap();
        fs::write(
            home.path().join(".codex/config.toml"),
            "[mcp_servers.codex_tool]\nurl = 'https://codex.example/mcp'\n",
        )
        .unwrap();
        fs::write(
            home.path().join(".claude.json"),
            r#"{"mcpServers":{"claude_tool":{"url":"https://claude.example/mcp"}}}"#,
        )
        .unwrap();
        fs::write(
            home.path().join(".codex/skills/codex-only/SKILL.md"),
            "---\nname: Codex only\ndescription: Codex skill\n---\nUse Codex.\n",
        )
        .unwrap();
        fs::write(
            home.path().join(".claude/skills/claude-only/SKILL.md"),
            "---\nname: Claude only\ndescription: Claude skill\n---\nUse Claude.\n",
        )
        .unwrap();

        let codex = discover_source(home.path(), &[], &[], &BTreeMap::new(), "codex")
            .unwrap()
            .preview();
        let claude = discover_source(home.path(), &[], &[], &BTreeMap::new(), "claude")
            .unwrap()
            .preview();
        assert!(
            codex
                .candidates
                .iter()
                .any(|item| item.name == "codex_tool")
        );
        assert!(
            codex
                .candidates
                .iter()
                .any(|item| item.name == "Codex only")
        );
        assert!(
            !codex
                .candidates
                .iter()
                .any(|item| item.name == "claude_tool" || item.name == "Claude only")
        );
        assert!(
            claude
                .candidates
                .iter()
                .any(|item| item.name == "claude_tool")
        );
        assert!(
            claude
                .candidates
                .iter()
                .any(|item| item.name == "Claude only")
        );
        assert!(
            !claude
                .candidates
                .iter()
                .any(|item| item.name == "codex_tool" || item.name == "Codex only")
        );
    }

    #[test]
    fn discovery_preview_includes_global_and_workspace_skill_path_and_hash_without_body() {
        let home = tempfile::tempdir().unwrap();
        let repository = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join(".codex/skills/example")).unwrap();
        fs::create_dir_all(repository.path().join(".agents/skills/local")).unwrap();
        let global = "---\nname: Global\ndescription: safe\n---\nInstructions";
        let local = "---\nname: Local\ndescription: repo\n---\nWorkspace instructions";
        fs::write(home.path().join(".codex/skills/example/SKILL.md"), global).unwrap();
        fs::write(
            repository.path().join(".agents/skills/local/SKILL.md"),
            local,
        )
        .unwrap();
        let preview = discover(
            home.path(),
            &[repository.path().to_path_buf()],
            &[],
            &BTreeMap::new(),
        )
        .preview();
        let skills: Vec<_> = preview
            .candidates
            .iter()
            .filter(|candidate| candidate.kind == ImportCandidateKind::Skill)
            .collect();
        assert_eq!(skills.len(), 2);
        assert!(skills.iter().all(|candidate| {
            candidate.metadata.contains_key("path")
                && candidate.metadata.contains_key("content_hash")
                && !candidate
                    .metadata
                    .values()
                    .any(|value| value.contains("Instructions"))
        }));
        assert!(skills.iter().any(|candidate| candidate.scope == "global"));
        assert!(
            skills
                .iter()
                .any(|candidate| candidate.scope.starts_with("workspace:"))
        );
    }

    #[test]
    fn home_directory_is_selectable_without_duplicate_global_skills() {
        let home = tempfile::tempdir().unwrap();
        let skill_dir = home.path().join(".codex/skills/example");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: Example\ndescription: A global skill\n---\nInstructions\n",
        )
        .unwrap();

        let preview = discover(
            home.path(),
            &[home.path().to_path_buf()],
            &[],
            &BTreeMap::new(),
        )
        .preview();
        let skills: Vec<_> = preview
            .candidates
            .iter()
            .filter(|item| item.kind == ImportCandidateKind::Skill && item.name == "Example")
            .collect();
        assert_eq!(skills.len(), 1, "home skills must not be scanned twice");
        assert_eq!(skills[0].scope, "global");
        let canonical_home = home.path().canonicalize().unwrap();
        assert_eq!(
            preview
                .candidates
                .iter()
                .filter(|item| {
                    item.kind == ImportCandidateKind::Workspace
                        && item.scope == format!("workspace:{}", canonical_home.display())
                })
                .count(),
            1
        );
        assert!(
            preview
                .candidates
                .iter()
                .find(|item| {
                    item.kind == ImportCandidateKind::Workspace
                        && item.scope == format!("workspace:{}", canonical_home.display())
                })
                .unwrap()
                .problem
                .is_none()
        );
    }

    #[test]
    fn preview_collapses_identical_skills_and_mcp_definitions_across_sources() {
        let config = ServerConfig::Http {
            url: "https://example.com/mcp".into(),
        };
        let connection = |id: &str, source: &str| Candidate {
            preview: ImportConnection {
                id: id.into(),
                name: "paper".into(),
                source: source.into(),
                repository: None,
                has_credentials: false,
                enabled_at_source: true,
                problem: None,
            },
            config: Some(config.clone()),
            credentials: None,
        };
        let skill = |source: &str, path: &str| neko_protocol::skills::Skill {
            path: path.into(),
            name: "paseo-handoff".into(),
            description: "Hand work over".into(),
            source: source.into(),
            workspace_id: None,
            content_hash: "same-content".into(),
        };
        let preview = Discovery {
            candidates: vec![
                connection("codex-paper", "/Users/nish/.codex/config.toml"),
                connection("claude-paper", "/Users/nish/.claude.json"),
            ],
            skills: vec![
                skill("Codex", "/Users/nish/.codex/skills/paseo-handoff/SKILL.md"),
                skill(
                    "Claude",
                    "/Users/nish/.claude/skills/paseo-handoff/SKILL.md",
                ),
            ],
            ..Discovery::default()
        }
        .preview();
        let connections = preview
            .candidates
            .iter()
            .filter(|candidate| candidate.kind == ImportCandidateKind::Connection)
            .collect::<Vec<_>>();
        let skills = preview
            .candidates
            .iter()
            .filter(|candidate| candidate.kind == ImportCandidateKind::Skill)
            .collect::<Vec<_>>();
        assert_eq!(connections.len(), 1);
        assert_eq!(skills.len(), 1);
        assert_eq!(connections[0].name, "paper");
        assert_eq!(
            connections[0]
                .metadata
                .get("config_summary")
                .map(String::as_str),
            Some("https://example.com/mcp")
        );
        assert!(connections[0].source.contains("Codex"));
        assert!(connections[0].source.contains("Claude"));
        assert_eq!(skills[0].name, "paseo-handoff");
        assert!(skills[0].source.contains("Codex"));
        assert!(skills[0].source.contains("Claude"));
    }

    #[test]
    fn a_plain_directory_is_offered_as_an_importable_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let preview = Discovery {
            repositories: vec![directory.path().to_string_lossy().into_owned()],
            ..Default::default()
        }
        .preview();
        let workspace = preview
            .candidates
            .iter()
            .find(|candidate| candidate.kind == ImportCandidateKind::Workspace)
            .unwrap();
        assert!(workspace.problem.is_none());
    }

    #[test]
    fn generated_workspace_folders_are_secondary_but_still_selectable() {
        let home = tempfile::tempdir().unwrap();
        let project = home.path().join("Documents/hme");
        let scratch = home.path().join("Documents/Codex/2026-09-26/one-off-task");
        let worktree = home.path().join("Documents/worktree");
        let generated_design = home.path().join("Library/Application Support/Open Design/namespaces/release-stable/data/projects/e15eca3c-c066-462f-8aaa-3afb88728d92");
        let smoke_root = tempfile::Builder::new()
            .prefix("neko-workbench-smoke-")
            .tempdir()
            .unwrap();
        let smoke = smoke_root.path().join("repo");
        let missing = home.path().join("Documents/removed-project");
        for path in [&project, &scratch, &worktree, &smoke, &generated_design] {
            fs::create_dir_all(path).unwrap();
        }
        fs::write(
            worktree.join(".git"),
            "gitdir: /tmp/origin/.git/worktrees/task\n",
        )
        .unwrap();
        let preview = Discovery {
            repositories: [
                &project,
                &scratch,
                &worktree,
                &smoke,
                &generated_design,
                &missing,
            ]
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
            ..Default::default()
        }
        .preview();
        let workspace = |path: &Path| {
            preview
                .candidates
                .iter()
                .find(|item| {
                    item.kind == ImportCandidateKind::Workspace
                        && item.workspace.as_deref() == path.to_str()
                })
                .unwrap()
        };
        assert_eq!(
            workspace(&project)
                .metadata
                .get("review_group")
                .map(String::as_str),
            Some("primary")
        );
        for path in [&scratch, &worktree, &smoke, &generated_design] {
            let item = workspace(path);
            assert_eq!(
                item.metadata.get("review_group").map(String::as_str),
                Some("other")
            );
            assert!(item.metadata.get("review_reason").is_some());
            assert!(
                item.problem.is_none(),
                "secondary folders remain selectable"
            );
        }
        assert_eq!(
            workspace(&missing)
                .metadata
                .get("review_group")
                .map(String::as_str),
            Some("other")
        );
        assert_eq!(
            workspace(&missing).problem.as_deref(),
            Some("Folder no longer exists")
        );
    }

    #[test]
    fn discovery_adds_skills_for_workspaces_found_by_paseo_adapter() {
        let home = tempfile::tempdir().unwrap();
        let repository = home.path().join("repo");
        fs::create_dir_all(repository.join(".agents/skills/local")).unwrap();
        fs::write(
            repository.join(".agents/skills/local/SKILL.md"),
            "---\nname: Local\n---\nWorkspace instructions",
        )
        .unwrap();
        fs::create_dir_all(home.path().join(".paseo/projects")).unwrap();
        fs::write(
            home.path().join(".paseo/projects/projects.json"),
            serde_json::json!([{"rootPath": repository, "archivedAt": null}]).to_string(),
        )
        .unwrap();
        let preview = discover(home.path(), &[], &[], &BTreeMap::new()).preview();
        assert!(preview.candidates.iter().any(|candidate| {
            candidate.kind == ImportCandidateKind::Skill && candidate.name == "Local"
        }));
    }

    #[cfg(unix)]
    #[test]
    fn discovery_deduplicates_a_workspace_reached_through_a_symlink_alias() {
        use std::os::unix::fs::symlink;
        let home = tempfile::tempdir().unwrap();
        let repository = home.path().join("repo");
        fs::create_dir_all(repository.join(".agents/skills/local")).unwrap();
        fs::write(
            repository.join(".agents/skills/local/SKILL.md"),
            "---\nname: Local\n---\nWorkspace instructions",
        )
        .unwrap();
        let alias = home.path().join("repo-alias");
        symlink(&repository, &alias).unwrap();
        fs::create_dir_all(home.path().join(".paseo/projects")).unwrap();
        fs::write(
            home.path().join(".paseo/projects/projects.json"),
            serde_json::json!([{"rootPath": repository, "archivedAt": null}]).to_string(),
        )
        .unwrap();

        let preview = discover(home.path(), &[alias], &[], &BTreeMap::new()).preview();
        assert_eq!(
            preview
                .candidates
                .iter()
                .filter(|candidate| {
                    candidate.kind == ImportCandidateKind::Workspace && candidate.name == "repo"
                })
                .count(),
            1
        );
        assert_eq!(
            preview
                .candidates
                .iter()
                .filter(|candidate| {
                    candidate.kind == ImportCandidateKind::Skill && candidate.name == "Local"
                })
                .count(),
            1
        );
    }

    #[test]
    fn codex_external_project_inventory_cannot_expand_skill_roots() {
        let home = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        fs::create_dir_all(external.path().join(".agents/skills/escaped")).unwrap();
        fs::write(
            external.path().join(".agents/skills/escaped/SKILL.md"),
            "---\nname: Escaped\n---\nExternal content",
        )
        .unwrap();
        fs::create_dir_all(home.path().join(".codex")).unwrap();
        fs::write(
            home.path().join(".codex/config.toml"),
            format!(
                "[projects.\"{}\"]\ntrust_level=\"trusted\"\n",
                external.path().display()
            ),
        )
        .unwrap();
        let preview = discover(home.path(), &[], &[], &BTreeMap::new()).preview();
        assert!(
            !preview
                .candidates
                .iter()
                .any(|candidate| candidate.name == "Escaped")
        );
    }

    #[cfg(unix)]
    #[test]
    fn skill_discovery_rejects_symlink_escape_from_workspace_root() {
        use std::os::unix::fs::symlink;
        let home = tempfile::tempdir().unwrap();
        let repository = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir_all(outside.path().join("escape")).unwrap();
        fs::write(outside.path().join("escape/SKILL.md"), "outside").unwrap();
        fs::create_dir_all(repository.path().join(".agents/skills")).unwrap();
        symlink(
            outside.path().join("escape"),
            repository.path().join(".agents/skills/escape"),
        )
        .unwrap();
        let preview = discover(
            home.path(),
            &[repository.path().to_path_buf()],
            &[],
            &BTreeMap::new(),
        )
        .preview();
        assert!(!preview.candidates.iter().any(|candidate| {
            candidate.kind == ImportCandidateKind::Skill && candidate.name == "escape"
        }));
    }

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
                && candidate.source == "Codex"
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
    fn preserves_header_environment_project_disable_and_working_directory() {
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
        assert!(local.preview.problem.is_some()); // /repo is not an existing directory.
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
                Path::new("/tmp"),
            );
            assert!(result.is_err(), "accepted {argument}");
        }
    }

    #[test]
    fn workspace_stdio_arguments_without_relative_paths_import() {
        let result = configuration(
            &serde_json::json!({"command":"/bin/sh", "args":["-c", "exit 0"]}),
            &[],
            &BTreeMap::new(),
            Some("/repo"),
            Path::new("/tmp"),
        );
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn explicit_working_directory_is_kept_in_imported_configuration() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir(home.path().join(".codex")).unwrap();
        fs::write(
            home.path().join(".codex/config.toml"),
            "[mcp_servers.local]\ncommand='/bin/sh'\ncwd='.'\n",
        )
        .unwrap();
        let out = discover(home.path(), &[], &[], &BTreeMap::new());
        let local = out
            .candidates
            .iter()
            .find(|c| c.preview.name == "local")
            .unwrap();
        assert!(
            local.preview.problem.is_none(),
            "{:?}",
            local.preview.problem
        );
        assert_eq!(
            serde_json::to_value(local.config.as_ref().unwrap()).unwrap()["cwd"],
            home.path()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .as_ref()
        );
    }

    #[test]
    fn project_relative_server_argument_uses_project_working_directory() {
        let repository = tempfile::tempdir().unwrap();
        let result = configuration(
            &serde_json::json!({"command":"/bin/sh", "args":["./server.sh"]}),
            &[],
            &BTreeMap::new(),
            repository.path().to_str(),
            repository.path(),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(result.0).unwrap()["cwd"],
            repository
                .path()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .as_ref()
        );
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
            Path::new("/tmp"),
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
