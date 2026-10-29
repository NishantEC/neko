//! Scopes found on disk: the owner keeps each part of their work (an employer,
//! a client, a side project) in a top folder under `~/Documents`, with that
//! scope's MCP servers in the folder's `.mcp.json` or in `~/.claude.json`'s
//! per-project entries. Reading only; nothing here writes a config file.
use neko_protocol::routines::{ScopeProposal, ScopeServer};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Display names for folders whose name says little.
const NAMES: &[(&str, &str)] = &[
    ("work", "Acme"),
    ("clientco", "Client A"),
    ("web-v2", "Client B"),
    ("ui-kit", "Sibling app"),
];

/// A repo-level server repeated this often under one scope belongs at the scope.
const REPEATED: usize = 3;

pub fn display_name(folder: &str) -> String {
    NAMES
        .iter()
        .find(|(dir, _)| *dir == folder)
        .map(|(_, name)| (*name).to_string())
        .unwrap_or_else(|| folder.to_string())
}

fn mcp_json_servers(path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let Ok(value) = serde_json::from_str::<Value>(&text) else { return Vec::new() };
    value["mcpServers"]
        .as_object()
        .map(|servers| servers.keys().cloned().collect())
        .unwrap_or_default()
}

fn project_servers(claude_json: Option<&Value>, path: &Path) -> Vec<String> {
    claude_json
        .and_then(|value| value["projects"][path.to_string_lossy().as_ref()]["mcpServers"].as_object())
        .map(|servers| servers.keys().cloned().collect())
        .unwrap_or_default()
}

fn visible_dirs(path: &Path) -> Vec<std::path::PathBuf> {
    let mut dirs: Vec<_> = std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
                .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .map(|entry| entry.path())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    dirs
}

/// Proposals sorted by name. `claude_json` is `~/.claude.json`, parsed.
pub fn discover(home: &Path, claude_json: Option<&Value>) -> Vec<ScopeProposal> {
    let documents = home.join("Documents");
    // Servers configured for exactly one folder say that folder is its own scope.
    let mut owners: BTreeMap<String, usize> = BTreeMap::new();
    if let Some(projects) = claude_json.and_then(|value| value["projects"].as_object()) {
        for project in projects.values() {
            for name in project["mcpServers"].as_object().into_iter().flat_map(|s| s.keys()) {
                *owners.entry(name.clone()).or_default() += 1;
            }
        }
    }
    let global: BTreeSet<String> = claude_json
        .and_then(|value| value["mcpServers"].as_object())
        .map(|servers| servers.keys().cloned().collect())
        .unwrap_or_default();

    let mut scopes = Vec::new();
    for top in visible_dirs(&documents) {
        let folder = top.file_name().unwrap_or_default().to_string_lossy().to_string();
        let mut servers = Vec::new();
        let top_file = top.join(".mcp.json");
        let top_names = mcp_json_servers(&top_file);
        for name in &top_names {
            servers.push(ScopeServer { name: name.clone(), origin: top_file.to_string_lossy().into(), level: "scope".into(), repo: None });
        }
        let own = project_servers(claude_json, &top);
        for name in &own {
            servers.push(ScopeServer { name: name.clone(), origin: "~/.claude.json".into(), level: "scope".into(), repo: None });
        }
        let mut repo_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut configured_repos = 0;
        let mut problems = Vec::new();
        for repo in visible_dirs(&top) {
            let repo_name = repo.file_name().unwrap_or_default().to_string_lossy().to_string();
            let file = repo.join(".mcp.json");
            let in_file = mcp_json_servers(&file);
            let in_claude = project_servers(claude_json, &repo);
            if !in_file.is_empty() || !in_claude.is_empty() {
                configured_repos += 1;
            }
            for (names, origin) in [(&in_file, file.to_string_lossy().to_string()), (&in_claude, "~/.claude.json".to_string())] {
                for name in names {
                    *repo_counts.entry(name.clone()).or_default() += 1;
                    if top_names.contains(name) {
                        problems.push(format!("{name} is set up both in {folder}/.mcp.json and in {repo_name}; the repo copy overrides the scope one"));
                    }
                    servers.push(ScopeServer { name: name.clone(), origin: origin.clone(), level: "repo".into(), repo: Some(repo_name.clone()) });
                }
            }
        }
        for (name, count) in &repo_counts {
            if *count >= REPEATED {
                problems.push(format!("{name} is set up separately in {count} repos under {folder}; one entry in {folder}/.mcp.json would cover them all"));
            }
        }
        let unique_own = own.iter().any(|name| owners.get(name) == Some(&1) && !global.contains(name));
        if top_names.is_empty() && configured_repos < 2 && !unique_own {
            continue;
        }
        scopes.push(ScopeProposal {
            name: display_name(&folder),
            folders: vec![top.to_string_lossy().into()],
            servers,
            problems,
            workspace_id: None,
            discovered: true,
        });
    }
    scopes.sort_by(|a, b| a.name.cmp(&b.name));
    scopes
}

/// The MCP servers a routine in this scope gets, as `{"mcpServers": …}`:
/// for each folder its `.mcp.json`, then its `~/.claude.json` entry (later
/// wins, as in Claude Code). Only the `personal` (home) scope also gets the
/// global servers in `~/.claude.json`; other scopes never see them.
pub fn mcp_config(folders: &[String], claude_json: Option<&Value>, personal: bool) -> Value {
    let mut servers = serde_json::Map::new();
    let mut take = |value: &Value| {
        if let Some(map) = value.as_object() {
            for (name, config) in map {
                servers.insert(name.clone(), config.clone());
            }
        }
    };
    if personal {
        if let Some(global) = claude_json {
            take(&global["mcpServers"]);
        }
    }
    for folder in folders {
        if let Ok(text) = std::fs::read_to_string(Path::new(folder).join(".mcp.json")) {
            if let Ok(value) = serde_json::from_str::<Value>(&text) {
                take(&value["mcpServers"]);
            }
        }
        if let Some(config) = claude_json {
            take(&config["projects"][folder.as_str()]["mcpServers"]);
        }
    }
    serde_json::json!({ "mcpServers": servers })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn home() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn tcc_scope_lists_scope_and_repo_servers() {
        let home = home();
        let docs = home.path().join("Documents");
        write(&docs.join("clientco/.mcp.json"), r#"{"mcpServers":{"slack-client-a":{},"google-client-a":{}}}"#);
        write(&docs.join("clientco/support-fe/README"), "");
        let triage = docs.join("clientco/support-fe");
        let claude = serde_json::json!({"projects": {triage.to_string_lossy(): {"mcpServers": {"expo": {}}}}});
        let scopes = discover(home.path(), Some(&claude));
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].name, "Client A");
        let names: Vec<_> = scopes[0].servers.iter().map(|s| (s.name.as_str(), s.level.as_str())).collect();
        assert!(names.contains(&("slack-client-a", "scope")));
        assert!(names.contains(&("expo", "repo")));
        assert!(scopes[0].problems.is_empty());
    }

    #[test]
    fn repeated_repo_server_is_a_problem() {
        let home = home();
        let docs = home.path().join("Documents");
        let mut projects = serde_json::Map::new();
        for repo in ["acme-api", "ria", "acme-data"] {
            write(&docs.join("work").join(repo).join("README"), "");
            projects.insert(docs.join("work").join(repo).to_string_lossy().into(), serde_json::json!({"mcpServers": {"ops-mcp-prod": {}}}));
        }
        let claude = serde_json::json!({ "projects": projects });
        let scopes = discover(home.path(), Some(&claude));
        assert_eq!(scopes[0].name, "Acme");
        assert!(scopes[0].problems.iter().any(|p| p.starts_with("ops-mcp-prod is set up separately in 3 repos under work")));
    }

    #[test]
    fn top_folder_without_config_is_not_a_scope() {
        let home = home();
        let docs = home.path().join("Documents");
        write(&docs.join("notes/a/README"), "");
        // A server shared with another folder does not make a scope either.
        let claude = serde_json::json!({"projects": {
            docs.join("notes").to_string_lossy(): {"mcpServers": {"admin": {}}},
            docs.join("other").to_string_lossy(): {"mcpServers": {"admin": {}}}
        }});
        assert!(discover(home.path(), Some(&claude)).is_empty());
    }

    #[test]
    fn a_server_only_one_folder_has_makes_it_a_scope() {
        let home = home();
        let docs = home.path().join("Documents");
        write(&docs.join("web-v2/README"), "");
        let claude = serde_json::json!({"mcpServers": {"sentry": {}}, "projects": {docs.join("web-v2").to_string_lossy(): {"mcpServers": {"linear-b": {}}}}});
        let scopes = discover(home.path(), Some(&claude));
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].name, "Client B");
    }

    #[test]
    fn scope_config_has_only_its_own_servers() {
        let home = home();
        let clientco = home.path().join("Documents/clientco");
        write(&clientco.join(".mcp.json"), r#"{"mcpServers":{"slack-client-a":{"type":"http"},"google-client-a":{"command":"uvx"}}}"#);
        let claude = serde_json::json!({
            "mcpServers": {"linear-personal": {}},
            "projects": {clientco.to_string_lossy(): {"mcpServers": {"google-client-a": {"command": "local"}}}}
        });
        let folders = vec![clientco.to_string_lossy().to_string()];
        let config = mcp_config(&folders, Some(&claude), false);
        let servers = config["mcpServers"].as_object().unwrap();
        assert_eq!(servers.len(), 2);
        assert!(!servers.contains_key("linear-personal"));
        assert_eq!(servers["google-client-a"]["command"], "local");
        let personal = mcp_config(&folders, Some(&claude), true);
        assert!(personal["mcpServers"].as_object().unwrap().contains_key("linear-personal"));
    }

    #[test]
    fn repo_copy_of_a_scope_server_is_a_problem() {
        let home = home();
        let docs = home.path().join("Documents");
        write(&docs.join("clientco/.mcp.json"), r#"{"mcpServers":{"slack-client-a":{}}}"#);
        write(&docs.join("clientco/support-fe/.mcp.json"), r#"{"mcpServers":{"slack-client-a":{}}}"#);
        let scopes = discover(home.path(), None);
        assert!(scopes[0].problems[0].starts_with("slack-client-a is set up both in clientco/.mcp.json and in support-fe"));
    }
}
