//! Packs: one installable unit that bundles tools (MCP connections), skills, a
//! web plugin and routine templates. A pack is a folder with a `pack.toml`
//! (see `docs/knowledge/packs.md`).
//!
//! Safety rules, kept in this file:
//! - Everything a pack adds is recorded in its own state row (`PackState`):
//!   the connection ids, skill folders, plugin folder and routine ids. Remove
//!   and Update touch only what the record names, and a folder is only ever
//!   deleted when it also carries the pack's marker (`.neko-pack`).
//! - A pack can only name programs by plain name (found in the usual folders,
//!   never inside the pack), and only paths inside its own folder. Links
//!   inside a pack are refused.
//! - Install and Update are all or nothing: every step is written to a
//!   `Journal` and any failure undoes it in reverse.
//! - Secrets (environment values) go to the Keychain under the connection's
//!   id; the state row and every answer carry only names.
use crate::{
    mcp_host::{credentials, store as policy},
    routines as routine_store, skills, workbench, Db,
};
use neko_protocol::{
    mcp_host::{McpConnection, ServerConfig},
    packs::*,
    routines::{Cadence, Routine, RoutineKind},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::Duration,
};

const INDEX: &str = "packs_index_v1";
const MARKER: &str = ".neko-pack";
const MAX_FILES: usize = 2000;
const MAX_BYTES: u64 = 20 * 1024 * 1024;
const HEALTH_TIMEOUT: Duration = Duration::from_secs(30);
const CLONE_TIMEOUT: Duration = Duration::from_secs(120);
const OUTPUT_TAIL: usize = 2000;

// ---------------------------------------------------------------------------
// Where things live, and the seams tests replace
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Paths {
    pub data: PathBuf,
    /// Folders holding the packs that ship with Neko (`<data>/source/packs`; the development checkout in tests).
    pub catalog: Vec<PathBuf>,
}

impl Paths {
    pub fn system() -> Self {
        let data = neko_protocol::support_dir();
        let mut catalog = vec![data.join("source").join("packs")];
        if let Some(extra) = std::env::var_os("NEKO_PACKS_DIR").map(PathBuf::from).filter(|p| p.is_absolute()) {
            catalog.push(extra);
        }
        Self { data, catalog }
    }
    fn packs(&self) -> PathBuf {
        self.data.join("packs")
    }
    fn skills(&self) -> PathBuf {
        self.data.join("skills")
    }
    fn plugins(&self) -> PathBuf {
        self.data.join("plugins")
    }
    fn archive(&self) -> PathBuf {
        self.data.join("cleanup-archive").join("packs")
    }
}

pub struct RunOutput {
    pub code: Option<i32>,
    pub output: String,
}

/// What the pack code asks of the machine. Tests replace all of it, so no test runs a real program.
pub struct Env {
    /// A program's absolute path by plain name.
    pub which: Box<dyn Fn(&str) -> Option<PathBuf>>,
    /// Runs `argv` (argv[0] absolute) in `cwd` with a time limit.
    pub run: Box<dyn Fn(&Path, &[String], Duration) -> RunOutput>,
    /// Shallow-clones a repository URL into an empty folder.
    pub fetch: Box<dyn Fn(&str, &Path) -> Result<(), String>>,
    /// Called after each install step; an error undoes the install (tests make one fail).
    pub hook: Box<dyn Fn(&str) -> Result<(), String>>,
}

impl Env {
    pub fn system() -> Self {
        Self { which: Box::new(which), run: Box::new(run_program), fetch: Box::new(clone_repository), hook: Box::new(|_| Ok(())) }
    }
}

fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".cargo/bin"));
    }
    for fixed in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
        dirs.push(PathBuf::from(fixed));
    }
    dirs.retain(|d| d.is_absolute());
    let mut seen = BTreeSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

/// Finds a program by plain name in the usual folders. Never looks inside a pack.
pub fn which(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    if !plain_program(name) {
        return None;
    }
    search_dirs().into_iter().map(|d| d.join(name)).find(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

fn run_program(cwd: &Path, argv: &[String], timeout: Duration) -> RunOutput {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let Some((program, args)) = argv.split_first() else { return RunOutput { code: None, output: "Nothing to run".into() } };
    let path = std::env::join_paths(search_dirs()).unwrap_or_default();
    let spawned = Command::new(program).args(args).current_dir(cwd).env("PATH", path).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => return RunOutput { code: None, output: format!("Could not start {program}: {error}") },
    };
    let reader = |stream: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut stream) = stream {
                let mut bytes = Vec::new();
                let _ = stream.by_ref().take(1 << 20).read_to_end(&mut bytes);
                text = String::from_utf8_lossy(&bytes).into_owned();
            }
            text
        })
    };
    let out = reader(child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let err = reader(child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let started = std::time::Instant::now();
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code().or(Some(-1)),
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                let text = format!("{}{}\nTimed out after {} s.", out.join().unwrap_or_default(), err.join().unwrap_or_default(), timeout.as_secs());
                return RunOutput { code: None, output: tail(&text) };
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => break None,
        }
    };
    let text = format!("{}{}", out.join().unwrap_or_default(), err.join().unwrap_or_default());
    RunOutput { code, output: tail(&text) }
}

fn clone_repository(url: &str, into: &Path) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let mut child = Command::new("git")
        .args(["-c", "protocol.file.allow=never", "-c", "core.hooksPath=/dev/null", "clone", "--depth", "1", "--no-tags", "--quiet", "--"])
        .arg(url)
        .arg(into)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ALLOW_PROTOCOL", "https:ssh")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Cannot run the repository fetch: {e}"))?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err("Could not fetch that repository (is the address right and the repository public?)".into()),
            Ok(None) if started.elapsed() >= CLONE_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Fetching that repository took too long".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn tail(text: &str) -> String {
    let text = text.trim();
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= OUTPUT_TAIL {
        return text.to_string();
    }
    let kept: String = chars[chars.len() - OUTPUT_TAIL..].iter().collect();
    format!("…{kept}")
}

// ---------------------------------------------------------------------------
// The manifest
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct Manifest {
    name: String,
    #[serde(default)]
    title: String,
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    homepage: String,
    #[serde(default)]
    licence: String,
    #[serde(default)]
    requires: Requires,
    #[serde(default)]
    mcp: Vec<McpDecl>,
    #[serde(default)]
    skills: Vec<SkillDecl>,
    #[serde(default)]
    ui: Ui,
    #[serde(default)]
    routines: Vec<RoutineDecl>,
    health: Option<HealthDecl>,
    #[serde(default)]
    permissions: Permissions,
}

#[derive(Debug, Default, Deserialize)]
struct Requires {
    #[serde(default)]
    binaries: Vec<BinaryDecl>,
}
#[derive(Debug, Deserialize)]
struct BinaryDecl {
    name: String,
    #[serde(default)]
    hint: String,
}
#[derive(Debug, Deserialize)]
struct McpDecl {
    name: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: Vec<String>,
}
#[derive(Debug, Deserialize)]
struct SkillDecl {
    path: String,
}
#[derive(Debug, Default, Deserialize)]
struct Ui {
    #[serde(default)]
    web_plugin: String,
    #[serde(default)]
    luau: Vec<String>,
}
#[derive(Debug, Deserialize)]
struct RoutineDecl {
    name: String,
    #[serde(default = "custom")]
    kind: String,
    instruction: String,
    cadence: Cadence,
}
fn custom() -> String {
    "Custom".into()
}
#[derive(Debug, Deserialize)]
struct HealthDecl {
    command: Vec<String>,
}
#[derive(Debug, Default, Deserialize)]
struct Permissions {
    #[serde(default)]
    summary: Vec<String>,
}

fn plain_name(name: &str, max: usize) -> bool {
    !name.is_empty() && name.len() <= max && !name.starts_with('-') && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn plain_program(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && !name.starts_with('-') && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

fn env_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && !name.as_bytes()[0].is_ascii_digit() && name.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

fn line(text: &str, max: usize) -> bool {
    text.len() <= max && !text.chars().any(|c| c.is_control())
}

/// A path inside the pack: relative, only plain components.
fn inside(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return Err(format!("“{}” must stay inside the pack's folder", path.display())),
        }
    }
    if out.as_os_str().is_empty() {
        return Err("A path in pack.toml is empty".into());
    }
    Ok(out)
}

fn leaf_slug(path: &Path) -> String {
    let leaf = path.file_name().and_then(|n| n.to_str()).unwrap_or("item").to_ascii_lowercase();
    let mut slug = String::new();
    for c in leaf.chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '-' };
        if !(c == '-' && slug.ends_with('-')) {
            slug.push(c);
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() { "item".into() } else { slug.chars().take(40).collect() }
}

fn command_line(command: &str, args: &[String]) -> String {
    std::iter::once(command).chain(args.iter().map(String::as_str)).collect::<Vec<_>>().join(" ")
}

/// Versions are one to three numbers with an optional `-suffix`: `0.1.0`, `2`, `1.4.2-beta`.
fn version_parts(version: &str) -> Option<Vec<u64>> {
    let core = version.split(['-', '+']).next()?;
    let parts: Option<Vec<u64>> = core.split('.').map(|p| p.parse().ok()).collect();
    parts.filter(|p| !p.is_empty() && p.len() <= 3)
}

/// Whether `candidate` is a higher version than `installed`.
pub fn is_newer(candidate: &str, installed: &str) -> bool {
    match (version_parts(candidate), version_parts(installed)) {
        (Some(mut a), Some(mut b)) => {
            a.resize(3, 0);
            b.resize(3, 0);
            a > b
        }
        _ => false,
    }
}

fn routine_id(pack: &str, name: &str) -> String {
    format!("pack-{pack}-{}", leaf_slug(Path::new(name)))
}

fn skill_dir_name(pack: &str, path: &Path) -> String {
    format!("pack-{pack}-{}", leaf_slug(path))
}

fn plugin_dir_name(pack: &str, path: &Path) -> String {
    format!("{pack}-{}", leaf_slug(path))
}

/// A pack folder read and checked, with what installing it would add.
struct Parsed {
    manifest: Manifest,
    info: PackInfo,
    skills: Vec<(PathBuf, String)>,
    plugin: Option<(PathBuf, String)>,
}

/// Lists the files under `dir` (skipping `.git`), refusing links and anything oversized.
fn scan_tree(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let mut bytes = 0u64;
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let entries = std::fs::read_dir(dir.join(&relative)).map_err(|e| format!("Cannot read the pack folder: {e}"))?;
        for entry in entries.flatten() {
            let name = entry.file_name();
            if relative.as_os_str().is_empty() && name == ".git" {
                continue;
            }
            let rel = relative.join(&name);
            let meta = std::fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() {
                return Err(format!("A pack cannot contain links ({})", rel.display()));
            }
            if meta.is_dir() {
                pending.push(rel.clone());
                files.push(rel);
            } else if meta.is_file() {
                bytes += meta.len();
                files.push(rel);
            } else {
                return Err(format!("A pack can only contain files and folders ({})", rel.display()));
            }
            if files.len() > MAX_FILES || bytes > MAX_BYTES {
                return Err("This pack is too large (over 2000 files or 20 MB)".into());
            }
        }
    }
    files.sort();
    Ok(files)
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    let files = scan_tree(from)?;
    std::fs::create_dir_all(to).map_err(|e| format!("Cannot create {}: {e}", to.display()))?;
    for rel in files {
        let (src, dst) = (from.join(&rel), to.join(&rel));
        if std::fs::symlink_metadata(&src).map_err(|e| e.to_string())?.is_dir() {
            std::fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| format!("Cannot copy {}: {e}", rel.display()))?;
        }
    }
    Ok(())
}

fn parse_dir(dir: &Path, env: &Env) -> Result<Parsed, String> {
    scan_tree(dir)?;
    let text = std::fs::read_to_string(dir.join("pack.toml")).map_err(|_| "There is no pack.toml in that folder".to_string())?;
    if text.len() > 64 * 1024 {
        return Err("pack.toml is too large".into());
    }
    let manifest: Manifest = toml::from_str(&text).map_err(|e| format!("pack.toml is not valid: {}", e.message()))?;
    let m = &manifest;
    if !plain_name(&m.name, 40) {
        return Err("A pack's name uses only a-z, 0-9 and -".into());
    }
    if version_parts(&m.version).is_none() {
        return Err("A pack's version looks like 0.1.0".into());
    }
    for (text, max) in [(&m.title, 80), (&m.description, 600), (&m.homepage, 300), (&m.licence, 60)] {
        if !line(text, max) {
            return Err("A title, description, homepage or licence is too long".into());
        }
    }
    if m.permissions.summary.is_empty() || m.permissions.summary.len() > 12 || m.permissions.summary.iter().any(|s| s.trim().is_empty() || !line(s, 200)) {
        return Err("A pack must say in plain words what it can do ([permissions] summary, up to 12 lines)".into());
    }
    let mut names = BTreeSet::new();
    for binary in &m.requires.binaries {
        if !plain_program(&binary.name) || !line(&binary.hint, 400) {
            return Err(format!("“{}” is not a program name", binary.name));
        }
    }
    if m.requires.binaries.len() > 8 {
        return Err("A pack can require at most 8 programs".into());
    }
    let mut env_names = BTreeSet::new();
    for server in &m.mcp {
        if !plain_name(&server.name, 40) || !names.insert(server.name.clone()) {
            return Err(format!("The tool server name “{}” is not valid or repeats", server.name));
        }
        if !plain_program(&server.command) || server.args.len() > 32 || server.args.iter().any(|a| !line(a, 1000)) {
            return Err(format!("The tool server “{}” must name a program by plain name (no path) with up to 32 arguments", server.name));
        }
        for name in &server.env {
            if !env_name(name) {
                return Err(format!("“{name}” is not an environment variable name"));
            }
            env_names.insert(name.clone());
        }
    }
    if m.mcp.len() > 8 || m.skills.len() > 20 || m.routines.len() > 12 || m.ui.luau.len() > 50 {
        return Err("This pack declares more than Neko accepts".into());
    }
    let mut skills_found = Vec::new();
    let mut leaves = BTreeSet::new();
    for skill in &m.skills {
        let rel = inside(&skill.path)?;
        let file = dir.join(&rel).join("SKILL.md");
        let meta = std::fs::metadata(&file).map_err(|_| format!("There is no SKILL.md in {}", skill.path))?;
        if !meta.is_file() || meta.len() as usize > skills::MAX_SKILL_BYTES || std::fs::read_to_string(&file).is_err() {
            return Err(format!("{}/SKILL.md must be a text file under 64 KB", skill.path));
        }
        let target = skill_dir_name(&m.name, &rel);
        if !leaves.insert(target.clone()) {
            return Err(format!("Two skills would be installed as {target}"));
        }
        skills_found.push((rel, target));
    }
    let plugin = if m.ui.web_plugin.trim().is_empty() {
        None
    } else {
        let rel = inside(&m.ui.web_plugin)?;
        let manifest_file = dir.join(&rel).join("manifest.json");
        let value: serde_json::Value = std::fs::read_to_string(&manifest_file).ok().and_then(|t| serde_json::from_str(&t).ok()).ok_or("The web plugin needs a valid manifest.json")?;
        let entry = value.get("entry").and_then(|e| e.as_str()).unwrap_or("index.js");
        if inside(entry).is_err() || !dir.join(&rel).join(entry).is_file() {
            return Err("The web plugin's entry file is missing".into());
        }
        Some((rel.clone(), plugin_dir_name(&m.name, &rel)))
    };
    for path in &m.ui.luau {
        let rel = inside(path)?;
        if rel.extension().and_then(|e| e.to_str()) != Some("luau") || !dir.join(&rel).is_file() {
            return Err(format!("{path} must be a .luau file in the pack"));
        }
    }
    let mut routine_ids = BTreeSet::new();
    for routine in &m.routines {
        if !matches!(routine.kind.as_str(), "Custom") {
            return Err("A pack's routines are kind = \"Custom\"".into());
        }
        if routine.name.trim().is_empty() || !line(&routine.name, 80) || routine.instruction.trim().is_empty() || routine.instruction.len() > routine_store::MAX_INSTRUCTION {
            return Err("A routine needs a short name and an instruction".into());
        }
        let valid = match &routine.cadence {
            Cadence::Daily { hour, minute } => *hour < 24 && *minute < 60,
            Cadence::Every { minutes, from_hour, to_hour } => *minutes >= 15 && *from_hour < 24 && *to_hour <= 24,
        };
        if !valid || !routine_ids.insert(routine_id(&m.name, &routine.name)) {
            return Err(format!("The routine “{}” has an invalid schedule or repeats a name", routine.name));
        }
    }
    if let Some(health) = &m.health {
        if health.command.is_empty() || health.command.len() > 16 || !plain_program(&health.command[0]) || health.command.iter().any(|a| !line(a, 1000)) {
            return Err("The health command must start with a program name".into());
        }
    }
    let requirements = m
        .requires
        .binaries
        .iter()
        .map(|b| {
            let path = (env.which)(&b.name);
            PackRequirement { name: b.name.clone(), hint: b.hint.clone(), found: path.is_some(), path: path.map(|p| p.to_string_lossy().into_owned()) }
        })
        .collect();
    let mut runs: Vec<String> = m.mcp.iter().map(|s| command_line(&s.command, &s.args)).collect();
    if let Some(health) = &m.health {
        runs.push(health.command.join(" "));
    }
    let info = PackInfo {
        name: m.name.clone(),
        title: if m.title.trim().is_empty() { m.name.clone() } else { m.title.trim().to_string() },
        version: m.version.clone(),
        description: m.description.clone(),
        homepage: m.homepage.clone(),
        licence: m.licence.clone(),
        permissions: m.permissions.summary.iter().map(|s| s.trim().to_string()).collect(),
        requirements,
        env: env_names.into_iter().collect(),
        contents: PackContents {
            connections: m.mcp.iter().map(|s| s.name.clone()).collect(),
            skills: skills_found.iter().map(|(_, t)| t.clone()).collect(),
            plugin: plugin.as_ref().map(|(_, t)| t.clone()),
            routines: m.routines.iter().map(|r| r.name.clone()).collect(),
            luau: m.ui.luau.clone(),
        },
        health_command: m.health.as_ref().map(|h| h.command.clone()).unwrap_or_default(),
        runs,
    };
    Ok(Parsed { manifest, info, skills: skills_found, plugin })
}

// ---------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------

enum Source {
    Catalog(String),
    Remote(String),
    Local(PathBuf),
}

fn classify(source: &str) -> Result<Source, String> {
    let source = source.trim();
    if source.is_empty() || source.len() > 500 || source.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("Give a folder path or a repository address".into());
    }
    if let Some(name) = source.strip_prefix("catalog:") {
        return if plain_name(name, 40) { Ok(Source::Catalog(name.into())) } else { Err("That catalog name is not valid".into()) };
    }
    if source.starts_with('-') {
        return Err("That is not a folder or repository address".into());
    }
    if source.starts_with("https://") || source.starts_with("ssh://") || (source.starts_with("git@") && source.contains(':')) {
        return Ok(Source::Remote(source.into()));
    }
    if source.contains("://") || source.contains("::") {
        return Err("Only https:// and ssh:// repository addresses are accepted".into());
    }
    let path = if let Some(rest) = source.strip_prefix("~/") {
        PathBuf::from(std::env::var_os("HOME").ok_or("No home folder")?).join(rest)
    } else {
        PathBuf::from(source)
    };
    if !path.is_absolute() {
        return Err("Use the full path of the folder (it starts with / or ~/)".into());
    }
    Ok(Source::Local(path))
}

/// A fetched copy that is deleted when it goes out of scope.
struct Fetched {
    dir: PathBuf,
    cleanup: Option<PathBuf>,
}
impl Drop for Fetched {
    fn drop(&mut self) {
        if let Some(path) = &self.cleanup {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

fn catalog_dir(paths: &Paths, name: &str) -> Option<PathBuf> {
    paths.catalog.iter().map(|root| root.join(name)).find(|dir| dir.join("pack.toml").is_file())
}

fn fetch(paths: &Paths, env: &Env, source: &str) -> Result<Fetched, String> {
    match classify(source)? {
        Source::Catalog(name) => catalog_dir(paths, &name).map(|dir| Fetched { dir, cleanup: None }).ok_or_else(|| format!("There is no bundled pack called {name}")),
        Source::Local(dir) => {
            if dir.is_dir() { Ok(Fetched { dir, cleanup: None }) } else { Err("That folder does not exist".into()) }
        }
        Source::Remote(url) => {
            let temp = paths.data.join(".packs-tmp").join(workbench::new_id());
            std::fs::create_dir_all(temp.parent().unwrap_or(&paths.data)).map_err(|e| e.to_string())?;
            let fetched = Fetched { dir: temp.clone(), cleanup: Some(temp.clone()) };
            (env.fetch)(&url, &temp)?;
            Ok(fetched)
        }
    }
}

// ---------------------------------------------------------------------------
// Stored state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OwnedConnection {
    name: String,
    /// Reserved at install; the connection record exists once the program was found (`created`).
    id: String,
    created: bool,
    /// Environment values were stored in the Keychain under `id`.
    credentials: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackState {
    name: String,
    version: String,
    source: String,
    info: PackInfo,
    installed_ms: i64,
    approved_ms: i64,
    status: PackStatus,
    health: Option<PackHealth>,
    connections: Vec<OwnedConnection>,
    skill_dirs: Vec<String>,
    skill_paths: Vec<String>,
    plugin_dir: Option<String>,
    routine_ids: Vec<String>,
}

fn state_key(name: &str) -> String {
    format!("pack_state_v1:{name}")
}

fn index(db: &Db) -> Vec<String> {
    db.get_setting(INDEX).ok().flatten().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn load_state(db: &Db, name: &str) -> Option<PackState> {
    db.get_setting(&state_key(name)).ok().flatten().and_then(|t| serde_json::from_str(&t).ok())
}

fn save_state(db: &Db, state: &PackState) -> Result<(), String> {
    let mut names = index(db);
    if !names.contains(&state.name) {
        names.push(state.name.clone());
        names.sort();
    }
    db.set_setting_quiet(&state_key(&state.name), &serde_json::to_string(state).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    db.set_setting_quiet(INDEX, &serde_json::to_string(&names).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn delete_state(db: &Db, name: &str) -> Result<(), String> {
    let names: Vec<String> = index(db).into_iter().filter(|n| n != name).collect();
    db.set_setting_quiet(INDEX, &serde_json::to_string(&names).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    db.delete_setting(&state_key(name)).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// The undo journal
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Journal {
    name: String,
    /// (destination, the folder that was moved aside to make room, if any), in the order done.
    swaps: Vec<(PathBuf, Option<PathBuf>)>,
    locked: Vec<String>,
    created_connections: Vec<String>,
    updated_connections: Vec<McpConnection>,
    credentials: Vec<String>,
    created_routines: Vec<String>,
    updated_routines: Vec<Routine>,
    /// The state row before this change (`None` was not installed).
    previous_state: Option<PackState>,
    state_written: bool,
}

fn rollback(db: &Db, journal: &Journal) {
    let name = Some(journal.name.clone());
    if journal.state_written {
        match &journal.previous_state {
            Some(previous) => {
                let _ = save_state(db, previous);
            }
            None => {
                if let Some(name) = &name {
                    let _ = delete_state(db, name);
                }
            }
        }
    }
    if !journal.created_routines.is_empty() || !journal.updated_routines.is_empty() {
        if let Ok(mut store) = routine_store::load(db) {
            store.routines.retain(|r| !journal.created_routines.contains(&r.id));
            for old in &journal.updated_routines {
                if let Some(slot) = store.routines.iter_mut().find(|r| r.id == old.id) {
                    *slot = old.clone();
                }
            }
            let _ = routine_store::save_definitions(db, &store);
        }
        for id in &journal.created_routines {
            forget_routine_rows(db, id);
        }
    }
    if !journal.created_connections.is_empty() || !journal.updated_connections.is_empty() {
        let _ = edit_snapshot(db, |snapshot| {
            snapshot.mcp.connections.retain(|c| !journal.created_connections.contains(&c.id));
            snapshot.mcp.grants.retain(|g| !journal.created_connections.contains(&g.connection_id));
            for old in &journal.updated_connections {
                if let Some(slot) = snapshot.mcp.connections.iter_mut().find(|c| c.id == old.id) {
                    *slot = old.clone();
                }
            }
            Ok(())
        });
    }
    for id in &journal.credentials {
        credentials::remove(id);
    }
    for path in &journal.locked {
        let _ = skills::curator::forget(db, path);
    }
    for (dest, backup) in journal.swaps.iter().rev() {
        let _ = std::fs::remove_dir_all(dest);
        if let Some(backup) = backup {
            let _ = std::fs::rename(backup, dest);
        }
    }
}

fn forget_routine_rows(db: &Db, id: &str) {
    let _ = db.delete_setting(&routine_store::state_key(id));
    let _ = db.delete_setting(&routine_store::runs_key(id));
}

fn edit_snapshot(db: &Db, change: impl FnOnce(&mut neko_protocol::workbench::Snapshot) -> Result<(), String>) -> Result<(), String> {
    let mut snapshot = workbench::load(db)?;
    change(&mut snapshot)?;
    policy::validate(&snapshot)?;
    workbench::save(db, &snapshot)
}

/// Moves what is at `dest` aside (when something is), then puts a copy of `src` there.
fn swap_dir(journal: &mut Journal, src: &Path, dest: &Path, tag: &str) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let backup = if std::fs::symlink_metadata(dest).is_ok() {
        let leaf = dest.file_name().and_then(|n| n.to_str()).unwrap_or("x");
        let backup = dest.with_file_name(format!(".bak-{tag}-{leaf}"));
        std::fs::rename(dest, &backup).map_err(|e| format!("Cannot make room at {}: {e}", dest.display()))?;
        Some(backup)
    } else {
        None
    };
    journal.swaps.push((dest.to_path_buf(), backup));
    copy_tree(src, dest)
}

fn write_marker(dir: &Path, pack: &str) -> Result<(), String> {
    std::fs::write(dir.join(MARKER), pack).map_err(|e| e.to_string())
}

/// A folder is deletable only when it sits directly in `root`, is a real folder and carries the pack's marker.
fn owned_dir(dir: &Path, root: &Path, pack: &str) -> bool {
    dir.parent() == Some(root) && std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink()) && std::fs::read_to_string(dir.join(MARKER)).is_ok_and(|t| t.trim() == pack)
}

/// The database is locked only around reads and writes: fetching a repository and
/// running a health check can take half a minute and must not hold up the daemon.
fn lock(db: &Mutex<Db>) -> MutexGuard<'_, Db> {
    db.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ---------------------------------------------------------------------------
// Preview, list
// ---------------------------------------------------------------------------

pub fn preview(db: &Mutex<Db>, paths: &Paths, env: &Env, source: &str) -> Result<PackPreview, String> {
    let fetched = fetch(paths, env, source)?;
    let parsed = parse_dir(&fetched.dir, env)?;
    Ok(PackPreview { installed_version: load_state(&lock(db), &parsed.info.name).map(|s| s.version), source: source.trim().to_string(), info: parsed.info })
}

/// A pack folder's version without reading the rest (`None` for a remote source).
fn source_version(paths: &Paths, env: &Env, source: &str) -> Option<String> {
    match classify(source).ok()? {
        Source::Remote(_) => None,
        _ => {
            let fetched = fetch(paths, env, source).ok()?;
            let text = std::fs::read_to_string(fetched.dir.join("pack.toml")).ok()?;
            toml::from_str::<Manifest>(&text).ok().map(|m| m.version)
        }
    }
}

pub fn list(db: &Mutex<Db>, paths: &Paths, env: &Env) -> Result<PackListing, String> {
    let states: Vec<PackState> = {
        let db = lock(db);
        index(&db).into_iter().filter_map(|name| load_state(&db, &name)).collect()
    };
    let installed = states.iter().map(|state| installed_view(state, paths, env)).collect();
    let db = &*lock(db);
    let mut catalog = Vec::new();
    let mut seen = BTreeSet::new();
    for root in &paths.catalog {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(root).map(|e| e.flatten().map(|e| e.path()).filter(|p| p.join("pack.toml").is_file()).collect()).unwrap_or_default();
        dirs.sort();
        for dir in dirs {
            if let Ok(parsed) = parse_dir(&dir, env) {
                if seen.insert(parsed.info.name.clone()) {
                    catalog.push(CatalogEntry { installed: load_state(db, &parsed.info.name).is_some(), source: format!("catalog:{}", parsed.info.name), info: parsed.info });
                }
            }
        }
    }
    Ok(PackListing { installed, catalog })
}

fn installed_view(state: &PackState, paths: &Paths, env: &Env) -> InstalledPack {
    let mut info = state.info.clone();
    for requirement in &mut info.requirements {
        let found = (env.which)(&requirement.name);
        requirement.found = found.is_some();
        requirement.path = found.map(|p| p.to_string_lossy().into_owned());
    }
    InstalledPack {
        update_available: source_version(paths, env, &state.source).filter(|v| is_newer(v, &state.version)),
        info,
        source: state.source.clone(),
        status: state.status,
        installed_ms: state.installed_ms,
        approved_ms: state.approved_ms,
        health: state.health.clone(),
        connection_ids: state.connections.iter().filter(|c| c.created).map(|c| c.id.clone()).collect(),
        skill_paths: state.skill_paths.clone(),
        plugin_dir: state.plugin_dir.clone(),
        routine_ids: state.routine_ids.clone(),
    }
}

// ---------------------------------------------------------------------------
// Install and update
// ---------------------------------------------------------------------------

pub fn install(db: &Mutex<Db>, paths: &Paths, env: &Env, source: &str, approve: bool, values: &BTreeMap<String, String>) -> Result<(), String> {
    if !approve {
        return Err("Installing a pack needs your approval of what it can do".into());
    }
    let fetched = fetch(paths, env, source)?;
    let parsed = parse_dir(&fetched.dir, env)?;
    {
        let db = lock(db);
        if load_state(&db, &parsed.info.name).is_some() {
            return Err(format!("{} is already installed; use Update", parsed.info.title));
        }
        apply(&db, paths, env, &fetched.dir, &parsed, source.trim(), None, values)?;
    }
    // The first health check is not an install step: a failing check leaves the pack installed.
    let _ = check(db, paths, env, &parsed.info.name);
    Ok(())
}

pub fn update(db: &Mutex<Db>, paths: &Paths, env: &Env, name: &str) -> Result<(), String> {
    let state = load_state(&lock(db), name).ok_or("That pack is not installed")?;
    let fetched = fetch(paths, env, &state.source)?;
    let parsed = parse_dir(&fetched.dir, env)?;
    if parsed.info.name != state.name {
        return Err("The source now holds a different pack".into());
    }
    if !is_newer(&parsed.info.version, &state.version) {
        return Err(format!("{} {} is already the newest version", state.info.title, state.version));
    }
    let known: BTreeSet<&String> = state.info.env.iter().collect();
    if parsed.info.env.iter().any(|name| !known.contains(name)) {
        return Err("This version asks for new values; remove the pack and add it again".into());
    }
    let source = state.source.clone();
    {
        let db = lock(db);
        // Reload: it may have changed while the source was being fetched.
        let state = load_state(&db, name).ok_or("That pack is not installed")?;
        apply(&db, paths, env, &fetched.dir, &parsed, &source, Some(state), &BTreeMap::new())?;
    }
    let _ = check(db, paths, env, name);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply(db: &Db, paths: &Paths, env: &Env, dir: &Path, parsed: &Parsed, source: &str, previous: Option<PackState>, values: &BTreeMap<String, String>) -> Result<(), String> {
    let name = parsed.info.name.clone();
    // Preflight: nothing is changed until every name is known to be free or ours.
    let missing: Vec<&String> = parsed.info.env.iter().filter(|n| previous.is_none() && values.get(*n).is_none_or(|v| v.is_empty())).collect();
    if !missing.is_empty() {
        return Err(format!("This pack needs values for: {}", missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
    }
    let owns = |path: &Path| previous.as_ref().is_some_and(|p| p.skill_dirs.iter().chain(p.plugin_dir.iter()).any(|d| Path::new(d) == path));
    let pack_dir = paths.packs().join(&name);
    if std::fs::symlink_metadata(&pack_dir).is_ok() && previous.is_none() {
        return Err(format!("A folder named {name} is already in the packs folder"));
    }
    let skill_root = paths.skills();
    let plugin_root = paths.plugins();
    let skill_targets: Vec<PathBuf> = parsed.skills.iter().map(|(_, target)| skill_root.join(target)).collect();
    for target in skill_targets.iter().chain(parsed.plugin.iter().map(|(_, t)| plugin_root.join(t)).collect::<Vec<_>>().iter()) {
        if std::fs::symlink_metadata(target).is_ok() && !owns(target) {
            return Err(format!("{} already exists and is not part of this pack; nothing was changed", target.display()));
        }
    }
    let approved_ms = previous.as_ref().map_or_else(workbench::now_ms, |p| p.approved_ms);
    let installed_ms = previous.as_ref().map_or_else(workbench::now_ms, |p| p.installed_ms);
    let tag = workbench::new_id();
    let mut journal = Journal { name: name.clone(), previous_state: previous.clone(), ..Journal::default() };
    let result = apply_steps(db, paths, env, dir, parsed, source, previous.as_ref(), values, &tag, approved_ms, installed_ms, &mut journal);
    match result {
        Ok(()) => {
            // Committed: the moved-aside copies are no longer needed.
            for (_, backup) in &journal.swaps {
                if let Some(backup) = backup {
                    let _ = std::fs::remove_dir_all(backup);
                }
            }
            if let Some(previous) = &previous {
                drop_leftovers(db, paths, previous, db_state(db, &name).as_ref());
            }
            Ok(())
        }
        Err(error) => {
            rollback(db, &journal);
            Err(error)
        }
    }
}

fn db_state(db: &Db, name: &str) -> Option<PackState> {
    load_state(db, name)
}

#[allow(clippy::too_many_arguments)]
fn apply_steps(db: &Db, paths: &Paths, env: &Env, dir: &Path, parsed: &Parsed, source: &str, previous: Option<&PackState>, values: &BTreeMap<String, String>, tag: &str, approved_ms: i64, installed_ms: i64, journal: &mut Journal) -> Result<(), String> {
    let name = &parsed.info.name;
    let manifest = &parsed.manifest;
    swap_dir(journal, dir, &paths.packs().join(name), tag)?;
    (env.hook)("pack")?;

    let mut skill_dirs = Vec::new();
    let mut skill_paths = Vec::new();
    for (rel, target) in &parsed.skills {
        let dest = paths.skills().join(target);
        swap_dir(journal, &dir.join(rel), &dest, tag)?;
        write_marker(&dest, name)?;
        let file = std::fs::canonicalize(dest.join("SKILL.md")).map_err(|e| e.to_string())?;
        let file = file.to_string_lossy().into_owned();
        if !previous.is_some_and(|p| p.skill_paths.contains(&file)) {
            journal.locked.push(file.clone());
        }
        skills::curator::set_locked(db, &file, true)?;
        skill_dirs.push(dest.to_string_lossy().into_owned());
        skill_paths.push(file);
    }
    (env.hook)("skills")?;

    let mut plugin_dir = None;
    if let Some((rel, target)) = &parsed.plugin {
        let dest = paths.plugins().join(target);
        swap_dir(journal, &dir.join(rel), &dest, tag)?;
        write_marker(&dest, name)?;
        plugin_dir = Some(dest.to_string_lossy().into_owned());
    }
    (env.hook)("plugin")?;

    // Tool servers. The person grants their tools per workspace; discovery never does.
    let mut owned: Vec<OwnedConnection> = Vec::new();
    let mut snapshot_changes: Vec<(McpConnection, bool)> = Vec::new();
    for server in &manifest.mcp {
        let before = previous.and_then(|p| p.connections.iter().find(|c| c.name == server.name)).cloned();
        let id = before.as_ref().map_or_else(workbench::new_id, |c| c.id.clone());
        let mut has_credentials = before.as_ref().is_some_and(|c| c.credentials);
        let supplied: BTreeMap<String, String> = server.env.iter().filter_map(|n| values.get(n).filter(|v| !v.is_empty()).map(|v| (n.clone(), v.clone()))).collect();
        if !supplied.is_empty() {
            let json = serde_json::json!({ "environment": supplied }).to_string();
            credentials::store(&id, &json)?;
            journal.credentials.push(id.clone());
            has_credentials = true;
        }
        let found = (env.which)(&server.command);
        let created = found.is_some();
        if let Some(program) = found {
            let connection = McpConnection {
                oauth: false,
                id: id.clone(),
                workspace_id: String::new(),
                label: if manifest.mcp.len() == 1 { parsed.info.title.clone() } else { format!("{}: {}", parsed.info.title, server.name) },
                config: ServerConfig::Stdio { command: program.to_string_lossy().into_owned(), args: server.args.clone(), cwd: None },
                enabled: true,
                trusted: true,
                has_credentials,
                tools: vec![],
                discovered_ms: None,
                error: None,
                source_link: None,
                pack: Some(name.clone()),
            };
            snapshot_changes.push((connection, before.as_ref().is_some_and(|c| c.created)));
        }
        owned.push(OwnedConnection { name: server.name.clone(), id, created, credentials: has_credentials });
    }
    if !snapshot_changes.is_empty() {
        edit_snapshot(db, |snapshot| {
            for (connection, existed) in snapshot_changes {
                let existing = snapshot.mcp.connections.iter().position(|c| c.id == connection.id);
                match existing {
                    Some(i) => {
                        // Only a connection this pack owns may be replaced.
                        if snapshot.mcp.connections[i].pack.as_deref() != Some(name.as_str()) {
                            return Err("A connection with that id belongs to something else".into());
                        }
                        let old = snapshot.mcp.connections[i].clone();
                        let mut next = connection;
                        // Keep what was discovered and any sign-in state; only the way to start it is replaced.
                        next.tools = old.tools.clone();
                        next.discovered_ms = old.discovered_ms;
                        next.error = old.error.clone();
                        next.enabled = old.enabled;
                        journal.updated_connections.push(old);
                        snapshot.mcp.connections[i] = next;
                    }
                    None => {
                        let _ = existed;
                        journal.created_connections.push(connection.id.clone());
                        snapshot.mcp.connections.push(connection);
                    }
                }
            }
            Ok(())
        })?;
    }
    (env.hook)("connections")?;

    // Routine templates, installed disabled; an update keeps their on/off state.
    let mut store = routine_store::load(db)?;
    let snapshot = workbench::load(db)?;
    let home = std::env::var("HOME").unwrap_or_default();
    let workspace_id = snapshot
        .workspaces
        .iter()
        .find(|w| snapshot.workspace_folders.get(&w.id).is_some_and(|f| f.iter().any(|f| *f == home)) || w.repository == home)
        .or(snapshot.workspaces.first())
        .map(|w| w.id.clone())
        .unwrap_or_default();
    let mut routine_ids = Vec::new();
    for decl in &manifest.routines {
        let id = routine_id(name, &decl.name);
        match store.routines.iter().position(|r| r.id == id) {
            Some(i) => {
                let old = store.routines[i].clone();
                if !old.customized {
                    journal.updated_routines.push(old);
                    store.routines[i].name = decl.name.trim().to_string();
                    store.routines[i].instruction = decl.instruction.clone();
                    store.routines[i].cadence = decl.cadence.clone();
                }
            }
            None => {
                journal.created_routines.push(id.clone());
                store.routines.push(Routine {
                    id: id.clone(),
                    workspace_id: workspace_id.clone(),
                    kind: RoutineKind::Custom,
                    name: decl.name.trim().to_string(),
                    instruction: decl.instruction.clone(),
                    cadence: decl.cadence.clone(),
                    enabled: false,
                    model: String::new(),
                    paseo_schedule: None,
                    browser: false,
                    computer: false,
                    customized: false,
                    catch_up: None,
                });
            }
        }
        routine_ids.push(id);
    }
    if !manifest.routines.is_empty() {
        routine_store::save_definitions(db, &store)?;
    }
    (env.hook)("routines")?;

    let state = PackState {
        name: name.clone(),
        version: parsed.info.version.clone(),
        source: source.to_string(),
        info: parsed.info.clone(),
        installed_ms,
        approved_ms,
        status: PackStatus::NeedsSetup,
        health: None,
        connections: owned,
        skill_dirs,
        skill_paths,
        plugin_dir,
        routine_ids,
    };
    journal.state_written = true;
    save_state(db, &state)?;
    (env.hook)("state")?;
    Ok(())
}

/// After a committed update: what the old version owned and the new one does not.
fn drop_leftovers(db: &Db, paths: &Paths, old: &PackState, new: Option<&PackState>) {
    let Some(new) = new else { return };
    for dir in old.skill_dirs.iter().filter(|d| !new.skill_dirs.contains(d)) {
        if owned_dir(Path::new(dir), &paths.skills(), &old.name) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
    for path in old.skill_paths.iter().filter(|p| !new.skill_paths.contains(p)) {
        let _ = skills::curator::forget(db, path);
    }
    if let Some(dir) = old.plugin_dir.as_ref().filter(|d| new.plugin_dir.as_ref() != Some(d)) {
        if owned_dir(Path::new(dir), &paths.plugins(), &old.name) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
    let gone: Vec<&String> = old.routine_ids.iter().filter(|id| !new.routine_ids.contains(id)).collect();
    if !gone.is_empty() {
        if let Ok(mut store) = routine_store::load(db) {
            store.routines.retain(|r| !gone.contains(&&r.id));
            let _ = routine_store::save_definitions(db, &store);
        }
        for id in gone {
            forget_routine_rows(db, id);
        }
    }
    let dropped: Vec<&OwnedConnection> = old.connections.iter().filter(|c| !new.connections.iter().any(|n| n.name == c.name)).collect();
    if !dropped.is_empty() {
        let ids: Vec<String> = dropped.iter().map(|c| c.id.clone()).collect();
        let _ = edit_snapshot(db, |snapshot| {
            let in_use = snapshot.mcp.responsibilities.iter().any(|r| r.connection_ids.iter().any(|c| ids.contains(c)));
            if in_use {
                return Ok(());
            }
            snapshot.mcp.connections.retain(|c| !(ids.contains(&c.id) && c.pack.as_deref() == Some(old.name.as_str())));
            snapshot.mcp.grants.retain(|g| !ids.contains(&g.connection_id));
            Ok(())
        });
        for c in dropped.iter().filter(|c| c.credentials) {
            credentials::remove(&c.id);
        }
    }
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

pub fn health(db: &Mutex<Db>, paths: &Paths, env: &Env, name: &str) -> Result<(), String> {
    if load_state(&lock(db), name).is_none() {
        return Err("That pack is not installed".into());
    }
    check(db, paths, env, name)
}

/// Looks again for the programs, creates a tool connection that was waiting for one,
/// runs the health check (outside the lock) and stores the result.
fn check(dbm: &Mutex<Db>, paths: &Paths, env: &Env, name: &str) -> Result<(), String> {
    let pack_dir = paths.packs().join(name);
    let manifest_text = std::fs::read_to_string(pack_dir.join("pack.toml")).map_err(|_| "The installed copy of the pack is missing".to_string())?;
    let manifest: Manifest = toml::from_str(&manifest_text).map_err(|e| e.to_string())?;
    {
        let db = lock(dbm);
        let mut state = load_state(&db, name).ok_or("That pack is not installed")?;
        // A connection waiting for its program.
        let mut waiting = Vec::new();
        for owned in state.connections.iter().filter(|c| !c.created) {
            if let Some(server) = manifest.mcp.iter().find(|s| s.name == owned.name) {
                if let Some(program) = (env.which)(&server.command) {
                    waiting.push((owned.name.clone(), owned.id.clone(), owned.credentials, program, server.args.clone()));
                }
            }
        }
        if !waiting.is_empty() {
            let title = state.info.title.clone();
            let many = manifest.mcp.len() > 1;
            edit_snapshot(&db, |snapshot| {
                for (server, id, has_credentials, program, args) in &waiting {
                    if snapshot.mcp.connections.iter().any(|c| c.id == *id) {
                        continue;
                    }
                    snapshot.mcp.connections.push(McpConnection {
                        oauth: false,
                        id: id.clone(),
                        workspace_id: String::new(),
                        label: if many { format!("{title}: {server}") } else { title.clone() },
                        config: ServerConfig::Stdio { command: program.to_string_lossy().into_owned(), args: args.clone(), cwd: None },
                        enabled: true,
                        trusted: true,
                        has_credentials: *has_credentials,
                        tools: vec![],
                        discovered_ms: None,
                        error: None,
                        source_link: None,
                        pack: Some(state.name.clone()),
                    });
                }
                Ok(())
            })?;
            for owned in &mut state.connections {
                if waiting.iter().any(|(_, id, ..)| *id == owned.id) {
                    owned.created = true;
                }
            }
            save_state(&db, &state)?;
        }
    }
    let missing: Vec<String> = manifest
        .requires
        .binaries
        .iter()
        .filter(|b| (env.which)(&b.name).is_none())
        .map(|b| if b.hint.trim().is_empty() { b.name.clone() } else { format!("{} (install it with: {})", b.name, b.hint.trim()) })
        .collect();
    let now = workbench::now_ms();
    let (status, health) = if !missing.is_empty() {
        (PackStatus::NeedsSetup, PackHealth { ok: false, at_ms: now, output: format!("Not found on this Mac: {}", missing.join("; ")) })
    } else if let Some(command) = manifest.health.as_ref().map(|h| &h.command) {
        match (env.which)(&command[0]) {
            None => (PackStatus::NeedsSetup, PackHealth { ok: false, at_ms: now, output: format!("{} is not installed", command[0]) }),
            Some(program) => {
                let mut argv = vec![program.to_string_lossy().into_owned()];
                argv.extend(command[1..].iter().cloned());
                let ran = (env.run)(&pack_dir, &argv, HEALTH_TIMEOUT);
                let ok = ran.code == Some(0);
                (if ok { PackStatus::Healthy } else { PackStatus::Failing }, PackHealth { ok, at_ms: now, output: ran.output })
            }
        }
    } else {
        (PackStatus::Healthy, PackHealth { ok: true, at_ms: now, output: "No health check declared; everything it needs is here.".into() })
    };
    let db = lock(dbm);
    // Reload: the pack may have been removed or updated while the check ran.
    let mut state = load_state(&db, name).ok_or("That pack is not installed")?;
    state.status = status;
    state.health = Some(health);
    save_state(&db, &state)
}

// ---------------------------------------------------------------------------
// Remove
// ---------------------------------------------------------------------------

pub fn remove(db: &Mutex<Db>, paths: &Paths, name: &str) -> Result<(), String> {
    let db = &*lock(db);
    let state = load_state(db, name).ok_or("That pack is not installed")?;
    let snapshot = workbench::load(db)?;
    let owned_ids: Vec<&String> = state.connections.iter().map(|c| &c.id).collect();
    let watching: Vec<String> = snapshot.mcp.responsibilities.iter().filter(|r| r.connection_ids.iter().any(|c| owned_ids.contains(&c))).map(|r| r.name.clone().unwrap_or_else(|| r.instruction.chars().take(40).collect())).collect();
    if !watching.is_empty() {
        return Err(format!("These watches use this pack's tools; delete or change them first: {}", watching.join(", ")));
    }
    // Keep a copy before anything is deleted; if the copy fails nothing is removed.
    let archive = paths.archive().join(format!("{name}-{}", workbench::now_ms()));
    std::fs::create_dir_all(&archive).map_err(|e| format!("Cannot keep a copy before removing: {e}"))?;
    let result = (|| -> Result<(), String> {
        let pack_dir = paths.packs().join(name);
        if pack_dir.is_dir() {
            copy_tree(&pack_dir, &archive.join("pack"))?;
        }
        for dir in state.skill_dirs.iter().map(Path::new).filter(|d| owned_dir(d, &paths.skills(), name)) {
            copy_tree(dir, &archive.join("skills").join(dir.file_name().unwrap_or_default()))?;
        }
        if let Some(dir) = state.plugin_dir.as_deref().map(Path::new).filter(|d| owned_dir(d, &paths.plugins(), name)) {
            copy_tree(dir, &archive.join("plugin").join(dir.file_name().unwrap_or_default()))?;
        }
        let connections: Vec<&McpConnection> = snapshot.mcp.connections.iter().filter(|c| owned_ids.contains(&&c.id) && c.pack.as_deref() == Some(name)).collect();
        let routines: Vec<Routine> = routine_store::load(db)?.routines.into_iter().filter(|r| state.routine_ids.contains(&r.id)).collect();
        std::fs::write(archive.join("record.json"), serde_json::to_string_pretty(&serde_json::json!({ "state": state, "connections": connections, "routines": routines })).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(&archive);
        return Err(format!("Cannot keep a copy before removing: {error}"));
    }

    // Connections and their grants.
    let ids: Vec<String> = state.connections.iter().map(|c| c.id.clone()).collect();
    edit_snapshot(db, |snapshot| {
        snapshot.mcp.connections.retain(|c| !(ids.contains(&c.id) && c.pack.as_deref() == Some(name)));
        snapshot.mcp.grants.retain(|g| !ids.contains(&g.connection_id));
        Ok(())
    })?;
    for owned in state.connections.iter().filter(|c| c.credentials) {
        credentials::remove(&owned.id);
    }
    // Routines.
    if !state.routine_ids.is_empty() {
        let mut store = routine_store::load(db)?;
        store.routines.retain(|r| !state.routine_ids.contains(&r.id));
        routine_store::save_definitions(db, &store)?;
        for id in &state.routine_ids {
            forget_routine_rows(db, id);
        }
    }
    // Skills: enabled entries, the curator's lock, the folders.
    if !state.skill_paths.is_empty() {
        let mut skill_state = skills::load(db)?;
        skill_state.enabled.retain(|e| !state.skill_paths.contains(&e.path));
        skill_state.available.retain(|s| !state.skill_paths.contains(&s.path));
        skills::save(db, &skill_state)?;
        for path in &state.skill_paths {
            let _ = skills::curator::forget(db, path);
        }
    }
    for dir in state.skill_dirs.iter().map(Path::new).filter(|d| owned_dir(d, &paths.skills(), name)) {
        let _ = std::fs::remove_dir_all(dir);
    }
    if let Some(dir) = state.plugin_dir.as_deref().map(Path::new).filter(|d| owned_dir(d, &paths.plugins(), name)) {
        let _ = std::fs::remove_dir_all(dir);
    }
    let _ = std::fs::remove_dir_all(paths.packs().join(name));
    delete_state(db, name)
}

// ---------------------------------------------------------------------------
// Tool servers a run may be given
// ---------------------------------------------------------------------------

/// A pack's tool server, as Claude's MCP config spells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackServer {
    pub pack: String,
    pub name: String,
    pub command: PathBuf,
    pub args: Vec<String>,
}

fn servers_of(state: &PackState, paths: &Paths, env: &Env) -> Vec<PackServer> {
    let Ok(text) = std::fs::read_to_string(paths.packs().join(&state.name).join("pack.toml")) else { return Vec::new() };
    let Ok(manifest) = toml::from_str::<Manifest>(&text) else { return Vec::new() };
    manifest
        .mcp
        .iter()
        .filter(|s| state.connections.iter().any(|c| c.name == s.name))
        .filter_map(|s| (env.which)(&s.command).map(|command| PackServer { pack: state.name.clone(), name: s.name.clone(), command, args: s.args.clone() }))
        .collect()
}

/// The tool servers of the pack that owns routine `routine_id` (a pack's routines see its tools).
pub fn routine_servers(db: &Db, paths: &Paths, env: &Env, routine_id: &str) -> Vec<PackServer> {
    index(db).into_iter().filter_map(|n| load_state(db, &n)).filter(|s| s.routine_ids.iter().any(|r| r == routine_id) && s.status != PackStatus::NeedsSetup).flat_map(|s| servers_of(&s, paths, env)).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    const PACK_TOML: &str = r#"
name = "fakepack"
title = "Fake Pack"
version = "0.1.0"
description = "A pack for tests."
homepage = "https://example.com/fakepack"
licence = "MIT"
[requires]
binaries = [{ name = "fakebin", hint = "brew install fakebin" }]
[[mcp]]
name = "fake"
command = "fakebin"
args = ["mcp"]
env = []
[[skills]]
path = "skills/use-fake"
[ui]
web_plugin = "web"
luau = ["scripts/a.luau"]
[[routines]]
name = "Morning check"
kind = "Custom"
instruction = "Check the fake thing."
cadence = { Daily = { hour = 9, minute = 0 } }
[health]
command = ["fakebin", "doctor"]
[permissions]
summary = ["Does fake things", "Reads fake screens"]
"#;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn fake_pack(dir: &Path, version: &str) {
        write(&dir.join("pack.toml"), &PACK_TOML.replace("0.1.0", version));
        write(&dir.join("skills/use-fake/SKILL.md"), &format!("---\nname: use-fake\ndescription: Use the fake thing\n---\nVersion {version}.\n"));
        write(&dir.join("web/manifest.json"), r#"{"title":"Fake","version":"0.1.0","entry":"index.js"}"#);
        write(&dir.join("web/index.js"), "export default function register() {}\n");
        write(&dir.join("scripts/a.luau"), "print('hi')\n");
    }

    struct Fixture {
        _temp: tempfile::TempDir,
        db: Mutex<Db>,
        paths: Paths,
        source: PathBuf,
        found: Rc<RefCell<BTreeSet<String>>>,
        health_code: Rc<RefCell<Option<i32>>>,
        fail_at: Rc<RefCell<Option<&'static str>>>,
        ran: Rc<RefCell<Vec<Vec<String>>>>,
    }

    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("source/fakepack");
            fake_pack(&source, "0.1.0");
            let paths = Paths { data: temp.path().join("data"), catalog: vec![temp.path().join("source")] };
            std::fs::create_dir_all(&paths.data).unwrap();
            Self {
                db: Mutex::new(Db::open_in_memory().unwrap()),
                paths,
                source,
                found: Rc::new(RefCell::new(["fakebin".to_string()].into())),
                health_code: Rc::new(RefCell::new(Some(0))),
                fail_at: Rc::new(RefCell::new(None)),
                ran: Rc::new(RefCell::new(Vec::new())),
                _temp: temp,
            }
        }
        fn d(&self) -> MutexGuard<'_, Db> {
            lock(&self.db)
        }
        fn env(&self) -> Env {
            let (found, code, fail, ran) = (self.found.clone(), self.health_code.clone(), self.fail_at.clone(), self.ran.clone());
            let found2 = found.clone();
            Env {
                which: Box::new(move |name| found.borrow().contains(name).then(|| PathBuf::from(format!("/opt/fake/bin/{name}")))),
                run: Box::new(move |_, argv, _| {
                    ran.borrow_mut().push(argv.to_vec());
                    RunOutput { code: *code.borrow(), output: "doctor output".into() }
                }),
                fetch: Box::new(|_, _| Err("no network in tests".into())),
                hook: Box::new(move |step| if *fail.borrow() == Some(step) { Err(format!("injected failure at {step}")) } else { let _ = &found2; Ok(()) }),
            }
        }
        fn source(&self) -> String {
            self.source.to_string_lossy().into_owned()
        }
        fn install(&self) -> Result<(), String> {
            install(&self.db, &self.paths, &self.env(), &self.source(), true, &BTreeMap::new())
        }
        fn state(&self) -> Option<PackState> {
            load_state(&self.d(), "fakepack")
        }
    }

    fn tree_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir).map(|e| e.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
        names.sort();
        names
    }

    #[test]
    fn preview_reads_the_pack_and_changes_nothing() {
        let f = Fixture::new();
        let preview = preview(&f.db, &f.paths, &f.env(), &f.source()).unwrap();
        assert_eq!(preview.info.name, "fakepack");
        assert_eq!(preview.info.permissions, ["Does fake things", "Reads fake screens"]);
        assert_eq!(preview.info.contents.connections, ["fake"]);
        assert_eq!(preview.info.contents.skills, ["pack-fakepack-use-fake"]);
        assert_eq!(preview.info.contents.plugin.as_deref(), Some("fakepack-web"));
        assert_eq!(preview.info.contents.routines, ["Morning check"]);
        assert_eq!(preview.info.runs, ["fakebin mcp", "fakebin doctor"]);
        assert_eq!(preview.info.requirements[0].hint, "brew install fakebin");
        assert!(preview.info.requirements[0].found);
        assert!(preview.installed_version.is_none());
        assert!(!f.paths.data.join("packs").exists() && !f.paths.data.join("skills").exists() && !f.paths.data.join("plugins").exists());
        assert!(index(&f.d()).is_empty());
        assert!(workbench::load(&f.d()).unwrap().mcp.connections.is_empty());
        assert!(f.ran.borrow().is_empty(), "a preview never runs the health check");
    }

    #[test]
    fn install_adds_everything_the_pack_declares_and_nothing_is_granted() {
        let f = Fixture::new();
        assert!(install(&f.db, &f.paths, &f.env(), &f.source(), false, &BTreeMap::new()).is_err(), "no approval, no install");
        assert!(index(&f.d()).is_empty());
        f.install().unwrap();
        let state = f.state().unwrap();
        assert_eq!(state.status, PackStatus::Healthy);
        assert!(state.health.as_ref().unwrap().ok);
        assert_eq!(f.ran.borrow()[0], ["/opt/fake/bin/fakebin", "doctor"]);
        // The pack folder, the skill (locked, marked), the plugin (marked).
        assert!(f.paths.data.join("packs/fakepack/pack.toml").is_file());
        let skill_dir = f.paths.data.join("skills/pack-fakepack-use-fake");
        assert!(skill_dir.join("SKILL.md").is_file() && skill_dir.join(MARKER).is_file());
        let plugin_dir = f.paths.data.join("plugins/fakepack-web");
        assert!(plugin_dir.join("manifest.json").is_file() && plugin_dir.join(MARKER).is_file());
        let curator = skills::load(&f.d()).unwrap().curator;
        assert!(curator.skills.iter().any(|s| s.state == "locked" && s.path == state.skill_paths[0]));
        // The connection: global, owned, enabled, with no grants.
        let snapshot = workbench::load(&f.d()).unwrap();
        assert_eq!(snapshot.mcp.connections.len(), 1);
        let connection = &snapshot.mcp.connections[0];
        assert_eq!(connection.pack.as_deref(), Some("fakepack"));
        assert!(connection.workspace_id.is_empty() && connection.enabled && connection.trusted);
        assert_eq!(connection.config, ServerConfig::Stdio { command: "/opt/fake/bin/fakebin".into(), args: vec!["mcp".into()], cwd: None });
        assert!(snapshot.mcp.grants.is_empty());
        // The routine is off.
        let routines = routine_store::load(&f.d()).unwrap().routines;
        assert_eq!(routines.len(), 1);
        assert!(!routines[0].enabled && routines[0].id == "pack-fakepack-morning-check");
        // Listing.
        let listing = list(&f.db, &f.paths, &f.env()).unwrap();
        assert_eq!(listing.installed.len(), 1);
        assert_eq!(listing.catalog.len(), 1);
        assert!(listing.catalog[0].installed && listing.catalog[0].source == "catalog:fakepack");
        assert_eq!(listing.installed[0].connection_ids, [connection.id.clone()]);
        // Installing twice is refused.
        assert!(f.install().unwrap_err().contains("already installed"));
    }

    #[test]
    fn discovery_never_grants_a_pack_tool() {
        let f = Fixture::new();
        f.install().unwrap();
        let mut snapshot = workbench::load(&f.d()).unwrap();
        snapshot.workspaces.push(neko_protocol::workbench::Workspace { id: "w".into(), name: "W".into(), repository: "/tmp/w".into(), instructions: String::new(), away_enabled: false });
        let id = snapshot.mcp.connections[0].id.clone();
        snapshot.mcp.connections[0].tools.push(neko_protocol::mcp_host::McpTool { read_only: true, name: "t".into(), description: String::new(), input_schema: "{}".into(), schema_hash: "h".into() });
        policy::allow_discovered_tools(&mut snapshot, &id);
        assert!(snapshot.mcp.grants.is_empty());
        // The person can still grant it per workspace.
        policy::apply_command(&mut snapshot, neko_protocol::mcp_host::McpCommand::SetWorkspaceToolGrant { workspace_id: "w".into(), connection_id: id, tool_name: "t".into(), schema_hash: "h".into(), allowed: true }, 0).unwrap();
        assert_eq!(snapshot.mcp.grants.len(), 1);
    }

    #[test]
    fn a_missing_program_leaves_the_pack_needing_setup_and_health_completes_it() {
        let f = Fixture::new();
        f.found.borrow_mut().clear();
        f.install().unwrap();
        let state = f.state().unwrap();
        assert_eq!(state.status, PackStatus::NeedsSetup);
        assert!(state.health.as_ref().unwrap().output.contains("brew install fakebin"));
        assert!(workbench::load(&f.d()).unwrap().mcp.connections.is_empty(), "no program, no connection yet");
        assert!(f.ran.borrow().is_empty());
        f.found.borrow_mut().insert("fakebin".into());
        health(&f.db, &f.paths, &f.env(), "fakepack").unwrap();
        assert_eq!(f.state().unwrap().status, PackStatus::Healthy);
        assert_eq!(workbench::load(&f.d()).unwrap().mcp.connections.len(), 1);
        // A failing check is Failing, and says why.
        *f.health_code.borrow_mut() = Some(3);
        health(&f.db, &f.paths, &f.env(), "fakepack").unwrap();
        let state = f.state().unwrap();
        assert_eq!(state.status, PackStatus::Failing);
        assert_eq!(state.health.unwrap().output, "doctor output");
    }

    #[test]
    fn remove_deletes_what_the_pack_owns_after_keeping_a_copy_and_nothing_else() {
        let f = Fixture::new();
        // Things that are not the pack's.
        write(&f.paths.data.join("skills/pack-fakepack-lookalike/SKILL.md"), "mine");
        write(&f.paths.data.join("skills/other/SKILL.md"), "mine");
        write(&f.paths.data.join("plugins/mine/manifest.json"), "{}");
        let mut snapshot = workbench::load(&f.d()).unwrap();
        snapshot.mcp.connections.push(McpConnection { oauth: false, id: "keep".into(), workspace_id: String::new(), label: "Keep".into(), config: ServerConfig::Http { url: "https://example.com/mcp".into() }, enabled: true, trusted: true, has_credentials: false, tools: vec![], discovered_ms: None, error: None, source_link: None, pack: None });
        workbench::save(&f.d(), &snapshot).unwrap();
        let mut store = routine_store::load(&f.d()).unwrap();
        store.routines.push(Routine { id: "mine".into(), workspace_id: "w".into(), kind: RoutineKind::Custom, name: "Mine".into(), instruction: "x".into(), cadence: Cadence::Daily { hour: 8, minute: 0 }, enabled: true, model: String::new(), paseo_schedule: None, browser: false, computer: false, customized: false, catch_up: None });
        routine_store::save_definitions(&f.d(), &store).unwrap();

        f.install().unwrap();
        let skill_path = f.state().unwrap().skill_paths[0].clone();
        let mut skill_state = skills::load(&f.d()).unwrap();
        skill_state.enabled.push(neko_protocol::skills::EnabledSkill { workspace_id: "w".into(), path: skill_path.clone(), content_hash: "h".into() });
        skills::save(&f.d(), &skill_state).unwrap();

        remove(&f.db, &f.paths, "fakepack").unwrap();
        assert!(f.state().is_none() && index(&f.d()).is_empty());
        assert_eq!(tree_names(&f.paths.data.join("skills")), ["other", "pack-fakepack-lookalike"]);
        assert_eq!(tree_names(&f.paths.data.join("plugins")), ["mine"]);
        assert!(!f.paths.data.join("packs/fakepack").exists());
        let snapshot = workbench::load(&f.d()).unwrap();
        assert_eq!(snapshot.mcp.connections.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["keep"]);
        assert_eq!(routine_store::load(&f.d()).unwrap().routines.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["mine"]);
        assert!(skills::load(&f.d()).unwrap().enabled.is_empty(), "no enabled entry points at a deleted skill");
        // A copy was kept first.
        let archives = tree_names(&f.paths.data.join("cleanup-archive/packs"));
        assert_eq!(archives.len(), 1);
        let archive = f.paths.data.join("cleanup-archive/packs").join(&archives[0]);
        assert!(archive.join("pack/pack.toml").is_file() && archive.join("skills/pack-fakepack-use-fake/SKILL.md").is_file() && archive.join("plugin/fakepack-web/manifest.json").is_file() && archive.join("record.json").is_file());
        assert!(remove(&f.db, &f.paths, "fakepack").is_err());
    }

    #[test]
    fn remove_never_deletes_a_folder_without_the_packs_marker() {
        let f = Fixture::new();
        f.install().unwrap();
        // The record is tampered with to point at someone else's folder.
        let mut state = f.state().unwrap();
        write(&f.paths.data.join("skills/pack-fakepack-theirs/SKILL.md"), "theirs");
        state.skill_dirs.push(f.paths.data.join("skills/pack-fakepack-theirs").to_string_lossy().into_owned());
        state.plugin_dir = Some("/".into());
        save_state(&f.d(), &state).unwrap();
        remove(&f.db, &f.paths, "fakepack").unwrap();
        assert!(f.paths.data.join("skills/pack-fakepack-theirs/SKILL.md").is_file());
        assert!(!f.paths.data.join("skills/pack-fakepack-use-fake").exists());
    }

    #[test]
    fn remove_refuses_while_a_watch_uses_the_packs_tools() {
        let f = Fixture::new();
        f.install().unwrap();
        let id = workbench::load(&f.d()).unwrap().mcp.connections[0].id.clone();
        let mut snapshot = workbench::load(&f.d()).unwrap();
        snapshot.workspaces.push(neko_protocol::workbench::Workspace { id: "w".into(), name: "W".into(), repository: "/tmp/w".into(), instructions: String::new(), away_enabled: false });
        snapshot.mcp.responsibilities.push(neko_protocol::mcp_host::Responsibility { id: "r".into(), workspace_id: "w".into(), instruction: "watch it".into(), connection_ids: vec![id], enabled: true, prepare_low_risk: false, next_due_ms: 0, last_attempt_ms: None, last_result: String::new(), failures: 0, name: Some("My watch".into()), name_by_user: true });
        workbench::save(&f.d(), &snapshot).unwrap();
        let error = remove(&f.db, &f.paths, "fakepack").unwrap_err();
        assert!(error.contains("My watch"));
        assert!(f.state().is_some() && f.paths.data.join("skills/pack-fakepack-use-fake").is_dir());
    }

    #[test]
    fn update_installs_a_higher_version_and_keeps_grants_and_routine_states() {
        let f = Fixture::new();
        f.install().unwrap();
        let id = workbench::load(&f.d()).unwrap().mcp.connections[0].id.clone();
        // The person's choices: a grant, and the routine switched on.
        let mut snapshot = workbench::load(&f.d()).unwrap();
        snapshot.workspaces.push(neko_protocol::workbench::Workspace { id: "w".into(), name: "W".into(), repository: "/tmp/w".into(), instructions: String::new(), away_enabled: false });
        snapshot.mcp.connections[0].tools.push(neko_protocol::mcp_host::McpTool { read_only: true, name: "t".into(), description: String::new(), input_schema: "{}".into(), schema_hash: "h".into() });
        snapshot.mcp.grants.push(neko_protocol::mcp_host::ToolGrant { connection_id: id.clone(), workspace_id: "w".into(), tool_name: "t".into(), schema_hash: "h".into() });
        workbench::save(&f.d(), &snapshot).unwrap();
        let mut store = routine_store::load(&f.d()).unwrap();
        store.routines[0].enabled = true;
        routine_store::save_definitions(&f.d(), &store).unwrap();

        assert!(update(&f.db, &f.paths, &f.env(), "fakepack").unwrap_err().contains("newest"), "same version is not an update");
        fake_pack(&f.source, "0.2.0");
        write(&f.source.join("pack.toml"), &std::fs::read_to_string(f.source.join("pack.toml")).unwrap().replace("Check the fake thing.", "Check it better."));
        let listing = list(&f.db, &f.paths, &f.env()).unwrap();
        assert_eq!(listing.installed[0].update_available.as_deref(), Some("0.2.0"));
        update(&f.db, &f.paths, &f.env(), "fakepack").unwrap();

        let state = f.state().unwrap();
        assert_eq!(state.version, "0.2.0");
        assert!(std::fs::read_to_string(f.paths.data.join("skills/pack-fakepack-use-fake/SKILL.md")).unwrap().contains("Version 0.2.0"));
        let snapshot = workbench::load(&f.d()).unwrap();
        assert_eq!(snapshot.mcp.connections.len(), 1);
        assert_eq!(snapshot.mcp.connections[0].id, id, "the same connection, so its grants stay");
        assert_eq!(snapshot.mcp.connections[0].tools.len(), 1);
        assert_eq!(snapshot.mcp.grants.len(), 1);
        let routine = &routine_store::load(&f.d()).unwrap().routines[0];
        assert!(routine.enabled, "the person's switch is kept");
        assert_eq!(routine.instruction, "Check it better.");
        assert!(tree_names(&f.paths.data.join("skills")).iter().all(|n| !n.starts_with(".bak")), "no moved-aside copies are left");
        assert!(update(&f.db, &f.paths, &f.env(), "fakepack").unwrap_err().contains("newest"));
    }

    #[test]
    fn a_failure_part_way_through_install_removes_everything_it_added() {
        for step in ["pack", "skills", "plugin", "connections", "routines", "state"] {
            let f = Fixture::new();
            *f.fail_at.borrow_mut() = Some(step);
            let error = f.install().unwrap_err();
            assert!(error.contains(step), "{step}: {error}");
            assert!(f.state().is_none() && index(&f.d()).is_empty(), "{step}");
            assert!(!f.paths.data.join("packs/fakepack").exists(), "{step}");
            assert!(tree_names(&f.paths.data.join("skills")).is_empty(), "{step}");
            assert!(tree_names(&f.paths.data.join("plugins")).is_empty(), "{step}");
            assert!(workbench::load(&f.d()).unwrap().mcp.connections.is_empty(), "{step}");
            assert!(routine_store::load(&f.d()).unwrap().routines.is_empty(), "{step}");
            assert!(skills::load(&f.d()).unwrap().curator.skills.is_empty(), "{step}");
            // And a retry afterwards works.
            *f.fail_at.borrow_mut() = None;
            f.install().unwrap();
        }
    }

    #[test]
    fn a_failed_update_restores_the_previous_version() {
        let f = Fixture::new();
        f.install().unwrap();
        let before = workbench::load(&f.d()).unwrap().mcp.connections[0].clone();
        fake_pack(&f.source, "0.2.0");
        for step in ["pack", "skills", "plugin", "connections", "routines", "state"] {
            *f.fail_at.borrow_mut() = Some(step);
            assert!(update(&f.db, &f.paths, &f.env(), "fakepack").is_err());
            assert_eq!(f.state().unwrap().version, "0.1.0", "{step}");
            assert!(std::fs::read_to_string(f.paths.data.join("skills/pack-fakepack-use-fake/SKILL.md")).unwrap().contains("Version 0.1.0"), "{step}");
            assert!(std::fs::read_to_string(f.paths.data.join("packs/fakepack/pack.toml")).unwrap().contains("0.1.0"), "{step}");
            assert!(f.paths.data.join("plugins/fakepack-web/manifest.json").is_file(), "{step}");
            assert_eq!(workbench::load(&f.d()).unwrap().mcp.connections, [before.clone()], "{step}");
            assert!(tree_names(&f.paths.data.join("skills")).iter().all(|n| !n.starts_with(".bak")), "{step}");
        }
        *f.fail_at.borrow_mut() = None;
        update(&f.db, &f.paths, &f.env(), "fakepack").unwrap();
        assert_eq!(f.state().unwrap().version, "0.2.0");
    }

    #[test]
    fn a_pack_cannot_reach_outside_its_folder_or_overwrite_what_is_not_its() {
        let f = Fixture::new();
        let bad = |toml: &str| {
            write(&f.source.join("pack.toml"), toml);
            f.install()
        };
        let valid = PACK_TOML;
        assert!(bad(&valid.replace("skills/use-fake", "../outside")).is_err());
        assert!(bad(&valid.replace("skills/use-fake", "/etc")).is_err());
        assert!(bad(&valid.replace("command = \"fakebin\"", "command = \"/bin/sh\"")).is_err(), "a program is named, not pathed");
        assert!(bad(&valid.replace("web_plugin = \"web\"", "web_plugin = \"../web\"")).is_err());
        assert!(bad(&valid.replace("name = \"fakepack\"", "name = \"Fake/../Pack\"")).is_err());
        assert!(bad(&valid.replace("kind = \"Custom\"", "kind = \"MacCare\"")).is_err());
        assert!(bad(&valid.replace("summary = [\"Does fake things\", \"Reads fake screens\"]", "summary = []")).is_err());
        assert!(index(&f.d()).is_empty() && !f.paths.data.join("packs").exists());
        // A link inside the pack.
        write(&f.source.join("pack.toml"), valid);
        std::os::unix::fs::symlink("/etc", f.source.join("web/link")).unwrap();
        assert!(f.install().unwrap_err().contains("links"));
        std::fs::remove_file(f.source.join("web/link")).unwrap();
        // A skill folder that already exists and is not this pack's is never touched.
        write(&f.paths.data.join("skills/pack-fakepack-use-fake/SKILL.md"), "someone else's");
        assert!(f.install().unwrap_err().contains("not part of this pack"));
        assert_eq!(std::fs::read_to_string(f.paths.data.join("skills/pack-fakepack-use-fake/SKILL.md")).unwrap(), "someone else's");
        assert!(f.state().is_none() && !f.paths.data.join("packs/fakepack").exists());
        assert!(workbench::load(&f.d()).unwrap().mcp.connections.is_empty());
    }

    #[test]
    fn values_for_the_environment_are_required_before_anything_changes() {
        let f = Fixture::new();
        write(&f.source.join("pack.toml"), &PACK_TOML.replace("env = []", "env = [\"FAKE_TOKEN\"]"));
        let error = f.install().unwrap_err();
        assert!(error.contains("FAKE_TOKEN") && !error.contains("secret"));
        assert!(!f.paths.data.join("packs").exists() && index(&f.d()).is_empty());
        let preview = preview(&f.db, &f.paths, &f.env(), &f.source()).unwrap();
        assert_eq!(preview.info.env, ["FAKE_TOKEN"]);
    }

    #[test]
    fn sources_are_classified_and_unsafe_ones_refused() {
        assert!(matches!(classify("catalog:spaceo"), Ok(Source::Catalog(_))));
        assert!(matches!(classify("https://github.com/a/b"), Ok(Source::Remote(_))));
        assert!(matches!(classify("git@github.com:a/b.git"), Ok(Source::Remote(_))));
        assert!(matches!(classify("/tmp/x"), Ok(Source::Local(_))));
        for bad in ["", "relative/path", "http://example.com/x", "file:///etc", "ext::sh -c id", "--upload-pack=x", "https://a b", "catalog:../x"] {
            assert!(classify(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn versions_compare_by_number_not_text() {
        assert!(is_newer("0.2.0", "0.1.9") && is_newer("0.10.0", "0.9.0") && is_newer("1", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0") && !is_newer("0.1.0", "0.2.0") && !is_newer("x", "0.1.0"));
    }

    #[test]
    fn the_catalog_lists_bundled_packs_and_installs_by_catalog_name() {
        let f = Fixture::new();
        let listing = list(&f.db, &f.paths, &f.env()).unwrap();
        assert_eq!(listing.catalog[0].info.name, "fakepack");
        assert!(!listing.catalog[0].installed);
        install(&f.db, &f.paths, &f.env(), "catalog:fakepack", true, &BTreeMap::new()).unwrap();
        assert_eq!(f.state().unwrap().source, "catalog:fakepack");
        assert!(preview(&f.db, &f.paths, &f.env(), "catalog:nope").is_err());
    }

    #[test]
    fn a_packs_routines_see_the_packs_tools() {
        let f = Fixture::new();
        f.install().unwrap();
        let servers = routine_servers(&f.d(), &f.paths, &f.env(), "pack-fakepack-morning-check");
        assert_eq!(servers.len(), 1);
        assert!(routine_servers(&f.d(), &f.paths, &f.env(), "someone-elses-routine").is_empty());
    }

    #[test]
    fn the_bundled_example_pack_is_valid_and_only_listed() {
        let packs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs");
        let env = Env { which: Box::new(|_| None), run: Box::new(|_, _, _| unreachable!()), fetch: Box::new(|_, _| unreachable!()), hook: Box::new(|_| Ok(())) };
        let parsed = parse_dir(&packs.join("spaceo"), &env).unwrap();
        assert_eq!(parsed.info.name, "spaceo");
        assert_eq!(parsed.info.contents.connections, ["spaceo"]);
        assert_eq!(parsed.info.contents.skills, ["pack-spaceo-use-spaceo"]);
        assert!(!parsed.info.requirements[0].found && parsed.info.requirements[0].hint.contains("install.sh"));
        assert_eq!(parsed.info.runs, ["spaceo mcp", "spaceo doctor"]);
        // Nothing installs it: the catalog only lists it.
        let temp = tempfile::tempdir().unwrap();
        let listing = list(&Mutex::new(Db::open_in_memory().unwrap()), &Paths { data: temp.path().into(), catalog: vec![packs] }, &env).unwrap();
        assert!(listing.installed.is_empty() && listing.catalog.iter().any(|c| c.info.name == "spaceo" && !c.installed));
        assert!(!temp.path().join("packs").exists());
    }
}
