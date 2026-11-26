//! A scenario: what a person does and what they should see, as a data file (`apps/neko/qa/*.json`). See docs/knowledge/qa.md.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// One scenario, with every `$work`, `$files`, `$home` and `$data` already replaced by the run's folders.
pub struct Scenario {
    pub name: String,
    pub title: String,
    pub about: String,
    /// The CLIs it may run with: "fake", "real".
    pub cli: Vec<String>,
    /// Whether it also runs in a real window (`--window`).
    pub window: bool,
    /// The window's size in points.
    pub size: (f32, f32),
    /// Folders made before the app starts, by name (`$files/NAME`).
    pub files: BTreeMap<String, Folder>,
    /// The fake CLI's view of the person's Claude Code settings (`$home/.claude/settings.json`); fake runs only.
    pub claude_settings: Option<Value>,
    /// More environment for the daemon.
    pub daemon_env: BTreeMap<String, String>,
    pub steps: Vec<Step>,
}

pub struct Folder {
    pub git: bool,
    pub files: BTreeMap<String, String>,
}

/// One step: an action, and (for checks) what a person should see.
pub struct Step {
    pub n: usize,
    pub v: Value,
}

impl Step {
    pub fn kind(&self) -> &str {
        self.v["do"].as_str().unwrap_or("")
    }
    pub fn expect(&self) -> Option<&str> {
        self.v["expect"].as_str()
    }
    pub fn name(&self) -> String {
        self.v["name"].as_str().map(str::to_owned).unwrap_or_else(|| slug(self.expect().unwrap_or(self.kind())))
    }
    /// The file stem for this step's picture and tree: its number, then its name.
    pub fn stem(&self) -> String {
        format!("{:02}-{}", self.n, self.name())
    }
}

/// A short file-name-safe form of `s`.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 40 {
            break;
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// The run's folders, for the `$` names in a scenario.
pub struct Places {
    pub work: PathBuf,
    pub files: PathBuf,
    pub home: PathBuf,
    pub data: PathBuf,
}

fn substitute(v: &mut Value, p: &Places) {
    match v {
        Value::String(s) => {
            let w = p.work.to_string_lossy();
            *s = s.replace("$files", &p.files.to_string_lossy()).replace("$home", &p.home.to_string_lossy()).replace("$data", &p.data.to_string_lossy()).replace("$work", &w);
        }
        Value::Array(a) => a.iter_mut().for_each(|x| substitute(x, p)),
        Value::Object(o) => o.values_mut().for_each(|x| substitute(x, p)),
        _ => {}
    }
}

pub fn load(path: &Path, places: &Places) -> Result<Scenario, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    substitute(&mut v, places);
    let strs = |k: &str| v[k].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect::<Vec<_>>()).unwrap_or_default();
    let size = match v["size"].as_array() {
        Some(a) if a.len() == 2 => (a[0].as_f64().unwrap_or(1200.0) as f32, a[1].as_f64().unwrap_or(800.0) as f32),
        _ => (1200.0, 800.0),
    };
    let mut files = BTreeMap::new();
    if let Some(o) = v["setup"]["files"].as_object() {
        for (name, f) in o {
            let inner = f["files"].as_object().map(|m| m.iter().map(|(k, x)| (k.clone(), x.as_str().unwrap_or("").to_owned())).collect()).unwrap_or_default();
            files.insert(name.clone(), Folder { git: f["git"].as_bool().unwrap_or(false), files: inner });
        }
    }
    let daemon_env = v["setup"]["daemon_env"].as_object().map(|o| o.iter().map(|(k, x)| (k.clone(), x.as_str().unwrap_or("").to_owned())).collect()).unwrap_or_default();
    let steps = v["steps"].as_array().ok_or("a scenario needs \"steps\"")?.iter().enumerate().map(|(i, s)| Step { n: i + 1, v: s.clone() }).collect();
    let cli = strs("cli");
    Ok(Scenario {
        name: v["name"].as_str().ok_or("a scenario needs a \"name\"")?.to_owned(),
        title: v["title"].as_str().unwrap_or("").to_owned(),
        about: v["about"].as_str().unwrap_or("").to_owned(),
        cli: if cli.is_empty() { vec!["fake".to_owned()] } else { cli },
        window: v["window"].as_bool().unwrap_or(false),
        size,
        files,
        claude_settings: v["setup"]["claude_settings"].as_object().map(|o| Value::Object(o.clone())),
        daemon_env,
        steps,
    })
}

/// Makes the scenario's folders: files written, a git repository with one commit where asked (a placeholder author).
pub fn make_files(s: &Scenario, places: &Places) -> Result<(), String> {
    for (name, f) in &s.files {
        let dir = places.files.join(name);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for (rel, body) in &f.files {
            let p = dir.join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&p, body).map_err(|e| format!("{}: {e}", p.display()))?;
        }
        if f.git {
            for args in [vec!["init", "-q", "-b", "main"], vec!["add", "-A"], vec!["-c", "user.name=Asha", "-c", "user.email=asha@example.com", "commit", "-q", "-m", "First commit"]] {
                git(&dir, &places.home, &args)?;
            }
        }
    }
    Ok(())
}

/// Runs git in `dir` with the run's own HOME and no system config.
pub fn git(dir: &Path, home: &Path, args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
