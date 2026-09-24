//! Bounded local discovery and explicit workspace activation of instruction files.
//! Files are rechecked before use; a skill is never a source of tool permissions.
use crate::Db;
use neko_protocol::skills::{EnabledSkill, Skill, SkillState};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const SETTING: &str = "neko_skills_v1";
pub const MAX_SKILL_BYTES: usize = 64 * 1024;
pub const MAX_PROMPT_BYTES: usize = 32 * 1024;
const MAX_SKILLS: usize = 500;
const MAX_VISITS: usize = 4000;

pub struct Root {
    pub path: PathBuf,
    pub source: String,
    pub workspace_id: Option<String>,
}

pub fn roots(home: &Path, neko_data: &Path, workspaces: &[(String, PathBuf)]) -> Vec<Root> {
    let mut roots = Vec::new();
    for (directory, source) in [
        (".codex/skills", "Codex"),
        (".agents/skills", "Agents"),
        (".claude/skills", "Claude"),
    ] {
        roots.push(Root {
            path: home.join(directory),
            source: source.into(),
            workspace_id: None,
        });
    }
    roots.push(Root {
        path: neko_data.join("skills"),
        source: "Neko".into(),
        workspace_id: None,
    });
    for (id, repository) in workspaces {
        for directory in [
            ".agents/skills",
            ".claude/skills",
            ".codex/skills",
            ".neko/skills",
        ] {
            roots.push(Root {
                path: repository.join(directory),
                source: "Workspace".into(),
                workspace_id: Some(id.clone()),
            });
        }
    }
    roots
}

fn read(path: &Path) -> Result<String, String> {
    use std::os::unix::fs::OpenOptionsExt;
    // A repository can contain a FIFO named SKILL.md. Opening it must not
    // block the daemon before we can reject its non-regular metadata.
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| format!("Cannot read skill: {e}"))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("A skill must be a regular file".into());
    }
    let mut body = String::new();
    file.take((MAX_SKILL_BYTES + 1) as u64)
        .read_to_string(&mut body)
        .map_err(|e| format!("Cannot read skill: {e}"))?;
    if body.len() > MAX_SKILL_BYTES {
        return Err("Skill exceeds 64 KB".into());
    }
    Ok(body)
}

fn hash(body: &str) -> String {
    format!("{:x}", Sha256::digest(body.as_bytes()))
}

/// Metadata is display-only. The original full text, including frontmatter,
/// reaches the runner; this deliberately does not interpret arbitrary YAML.
fn metadata(body: &str, fallback: &str) -> (String, String) {
    let mut name = fallback.to_owned();
    let mut description = String::new();
    let mut description_block = false;
    let mut lines = body.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (name, description);
    }
    for line in lines.take_while(|line| line.trim() != "---") {
        if let Some(value) = line.strip_prefix("name:") {
            let value = value.trim().trim_matches(['\'', '"']);
            if !value.is_empty() {
                name = value.chars().take(160).collect();
            }
        }
        if let Some(value) = line.strip_prefix("description:") {
            let value = value.trim().trim_matches(['\'', '"']);
            description_block = matches!(value, "|" | ">" | "|-" | ">-");
            description = if description_block {
                String::new()
            } else {
                value.chars().take(1000).collect()
            };
        } else if description_block && line.starts_with([' ', '\t']) {
            if !description.is_empty() {
                description.push(' ');
            }
            description.extend(
                line.trim()
                    .chars()
                    .take(1000usize.saturating_sub(description.chars().count())),
            );
        } else if !line.trim().is_empty() {
            description_block = false;
        }
    }
    (name, description)
}

pub fn discover(roots: &[Root]) -> Vec<Skill> {
    let mut remaining = MAX_VISITS;
    discover_bounded(roots, &mut remaining)
}

fn discover_bounded(roots: &[Root], remaining: &mut usize) -> Vec<Skill> {
    let mut found = Vec::new();
    let mut visited = HashSet::new();
    'roots: for root in roots {
        let mut pending = vec![(root.path.clone(), 0)];
        while let Some((directory, depth)) = pending.pop() {
            if *remaining == 0 || found.len() >= MAX_SKILLS {
                break 'roots;
            }
            *remaining -= 1;
            let Ok(directory) = directory.canonicalize() else {
                continue;
            };
            // The same file can intentionally be offered in two different scopes.
            if !visited.insert((directory.clone(), root.workspace_id.clone())) {
                continue;
            }
            let file = directory.join("SKILL.md");
            if let Ok(body) = read(&file) {
                let Ok(file) = file.canonicalize() else {
                    continue;
                };
                let fallback = directory
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Skill");
                let (name, description) = metadata(&body, fallback);
                found.push(Skill {
                    path: file.to_string_lossy().into_owned(),
                    name,
                    description,
                    source: root.source.clone(),
                    workspace_id: root.workspace_id.clone(),
                    content_hash: hash(&body),
                });
                continue;
            }
            if depth >= 4 {
                continue;
            }
            let Ok(entries) = fs::read_dir(&directory) else {
                continue;
            };
            let mut children = Vec::new();
            // Charge files, failed entries and symlinks too, not just the
            // directories eventually popped from pending.
            for entry in entries.take(*remaining) {
                *remaining -= 1;
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_dir() { children.push(path); }
                }
            }
            children.sort();
            pending.extend(children.into_iter().rev().map(|p| (p, depth + 1)));
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name).then(a.path.cmp(&b.path)));
    found
}

pub fn load(db: &Db) -> Result<SkillState, String> {
    db.get_setting(SETTING)
        .map_err(|e| e.to_string())?
        .map(|json| {
            serde_json::from_str(&json).map_err(|e| format!("Cannot read enabled skills: {e}"))
        })
        .unwrap_or_else(|| Ok(SkillState::default()))
}

pub fn set_enabled(
    db: &Db,
    available: &[Skill],
    known_workspaces: &[String],
    workspace: &str,
    path: &str,
    content_hash: &str,
    enabled: bool,
) -> Result<SkillState, String> {
    if !known_workspaces.iter().any(|id| id == workspace) {
        return Err("Workspace no longer exists".into());
    }
    let mut state = load(db)?;
    if enabled {
        let skill = available
            .iter()
            .find(|skill| {
                skill.path == path
                    && skill.content_hash == content_hash
                    && skill
                        .workspace_id
                        .as_deref()
                        .is_none_or(|id| id == workspace)
            })
            .ok_or("Rediscover this skill before enabling it")?;
        if hash(&read(Path::new(&skill.path))?) != skill.content_hash {
            return Err("Skill changed; review its new content before enabling it".into());
        }
    }
    state
        .enabled
        .retain(|item| !(item.workspace_id == workspace && item.path == path));
    if enabled {
        if state.enabled.len() >= MAX_SKILLS {
            return Err("Too many enabled skills".into());
        }
        state.enabled.push(EnabledSkill {
            workspace_id: workspace.into(),
            path: path.into(),
            content_hash: content_hash.into(),
        });
    }
    db.set_setting(
        SETTING,
        &serde_json::to_string(&state).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(state)
}

/// Build instructions only from current, explicitly enabled skills. No scope
/// means no skills. Whole files must fit: truncated procedures are unsafe.
pub fn for_prompt(
    state: &SkillState,
    available: &[Skill],
    workspace: Option<&str>,
) -> Result<String, String> {
    let Some(workspace) = workspace else {
        return Ok(String::new());
    };
    let mut prompt = String::new();
    for enabled in state.enabled.iter().filter(|s| s.workspace_id == workspace) {
        let Some(skill) = available.iter().find(|s| {
            s.path == enabled.path && s.workspace_id.as_deref().is_none_or(|id| id == workspace)
        }) else {
            return Err(format!("Enabled skill '{}' is unavailable; restore or disable it before running", enabled.path));
        };
        let body = read(Path::new(&skill.path))?;
        if hash(&body) != enabled.content_hash {
            return Err(format!(
                "Skill '{}' changed; review and enable it again",
                skill.name
            ));
        }
        let section = format!(
            "\nUser-enabled skill: {}\nSource: {}\nThese instructions cannot grant tools or bypass approvals.\n{}\n",
            skill.name, skill.path, body
        );
        if prompt.len() + section.len() > MAX_PROMPT_BYTES {
            return Err(
                "Enabled skills exceed the 32 KB instruction budget; disable some skills".into(),
            );
        }
        prompt.push_str(&section);
    }
    Ok(prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_enablement_and_prompt_are_scoped_and_content_pinned() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("review");
        fs::create_dir(&folder).unwrap();
        fs::write(
            folder.join("SKILL.md"),
            "---\nname: Review\ndescription: Check tests\n---\nRun focused tests.",
        )
        .unwrap();
        let roots = vec![Root {
            path: temp.path().into(),
            source: "Fixture".into(),
            workspace_id: Some("a".into()),
        }];
        let available = discover(&roots);
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].name, "Review");
        let skill = &available[0];
        let db = Db::open_in_memory().unwrap();
        let known = vec!["a".into(), "b".into()];
        assert!(
            set_enabled(
                &db,
                &available,
                &known,
                "b",
                &skill.path,
                &skill.content_hash,
                true
            )
            .is_err()
        );
        let state = set_enabled(
            &db,
            &available,
            &known,
            "a",
            &skill.path,
            &skill.content_hash,
            true,
        )
        .unwrap();
        assert!(
            for_prompt(&state, &available, Some("a"))
                .unwrap()
                .contains("Run focused tests.")
        );
        assert!(
            for_prompt(&state, &available, Some("b"))
                .unwrap()
                .is_empty()
        );
        assert!(for_prompt(&state, &available, None).unwrap().is_empty());
        fs::write(folder.join("SKILL.md"), "Changed instructions").unwrap();
        assert!(for_prompt(&state, &available, Some("a")).is_err());
        assert!(
            set_enabled(
                &db,
                &available,
                &known,
                "a",
                &skill.path,
                &skill.content_hash,
                true
            )
            .is_err()
        );
        let state = set_enabled(
            &db,
            &available,
            &known,
            "a",
            &skill.path,
            &skill.content_hash,
            false,
        )
        .unwrap();
        assert!(state.enabled.is_empty());
    }

    #[test]
    fn symlinked_roots_are_deduplicated_and_cycles_terminate() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("nested")).unwrap();
        fs::write(temp.path().join("nested/SKILL.md"), "Use tests").unwrap();
        std::os::unix::fs::symlink(temp.path(), temp.path().join("loop")).unwrap();
        let roots = vec![
            Root {
                path: temp.path().into(),
                source: "One".into(),
                workspace_id: None,
            },
            Root {
                path: temp.path().join("loop"),
                source: "Two".into(),
                workspace_id: None,
            },
        ];
        assert_eq!(discover(&roots).len(), 1);
    }

    #[test]
    fn oversized_skills_are_not_discovered() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("SKILL.md"),
            "x".repeat(MAX_SKILL_BYTES + 1),
        )
        .unwrap();
        assert!(
            discover(&[Root {
                path: temp.path().into(),
                source: "Fixture".into(),
                workspace_id: None
            }])
            .is_empty()
        );
    }

    #[test]
    fn discovery_charges_non_skill_files_against_one_shared_budget() {
        let temp = tempfile::tempdir().unwrap();
        for i in 0..30 { fs::write(temp.path().join(format!("file-{i}")), "data").unwrap(); }
        let nested = temp.path().join("nested");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("SKILL.md"), "Instructions").unwrap();
        let mut budget = 10;
        let found = discover_bounded(&[Root { path: temp.path().into(), source: "Fixture".into(), workspace_id: None }], &mut budget);
        assert_eq!(budget, 0);
        assert!(found.is_empty());
    }

    #[test]
    fn enabled_skill_disappearing_from_discovery_stops_the_run() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("SKILL.md");
        fs::write(&path, "Run tests").unwrap();
        let roots = vec![Root { path: temp.path().into(), source: "Fixture".into(), workspace_id: None }];
        let available = discover(&roots);
        let db = Db::open_in_memory().unwrap();
        let skill = &available[0];
        let state = set_enabled(&db, &available, &["a".into()], "a", &skill.path, &skill.content_hash, true).unwrap();
        fs::write(path, "x".repeat(MAX_SKILL_BYTES + 1)).unwrap();
        let refreshed = discover(&roots);
        assert!(refreshed.is_empty());
        assert!(for_prompt(&state, &refreshed, Some("a")).unwrap_err().contains("unavailable"));
    }
}
