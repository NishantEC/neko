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

pub fn current_roots(workspaces: &[neko_protocol::workbench::Workspace]) -> Vec<Root> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    roots(&home, &neko_protocol::support_dir(), &workspaces.iter().map(|w| (w.id.clone(), PathBuf::from(&w.repository))).collect::<Vec<_>>())
}

pub fn save(db: &Db, state: &SkillState) -> Result<(), String> {
    let json = serde_json::to_string(state).map_err(|e| e.to_string())?;
    if json.len() > 4 * 1024 * 1024 { return Err("Skill storage is full; review pending proposals first".into()); }
    db.set_setting(SETTING, &json).map_err(|e| e.to_string())
}

pub fn refresh(db: &Db, workspaces: &[neko_protocol::workbench::Workspace]) -> Result<(), String> {
    let mut state = load(db)?;
    state.available = discover(&current_roots(workspaces));
    save(db, &state)
}

pub fn instructions(db: &Db, workspaces: &[neko_protocol::workbench::Workspace], workspace: Option<&str>) -> Result<String, String> {
    let state = load(db)?;
    if !state.enabled.iter().any(|s| Some(s.workspace_id.as_str()) == workspace) { return Ok(String::new()); }
    for_prompt(&state, &discover(&current_roots(workspaces)), workspace)
}

/// A preview is durable review material, never executable installation authority.
pub fn propose(db: &Db, workspace: &str, name: &str, body: &str, source: &str, audit_url: Option<String>) -> Result<(), String> {
    if body.trim().is_empty() || body.len() > MAX_SKILL_BYTES { return Err("Skill content must contain 1–65536 bytes".into()); }
    let mut state = load(db)?;
    if state.proposals.iter().any(|p| p.source == source && p.workspace_id == workspace) { return Ok(()); }
    if state.proposals.len() >= 20 { return Err("Review pending skill proposals before adding more".into()); }
    state.proposals.push(neko_protocol::skills::SkillProposal {
        id: crate::workbench::new_id(), workspace_id: workspace.into(), name: name.chars().take(160).collect(), body: body.into(), content_hash: hash(body), source: source.into(), audit_url,
        audit_status: "Not audited by Neko. Review the exact instructions and source; linked third-party audit results are not verified here.".into(),
        audit_reviewed_hash: None,
    });
    save(db, &state)
}

pub fn decide(db: &Db, data: &Path, id: &str, expected_hash: &str, accept: bool) -> Result<(), String> {
    decide_with_hook(db, data, id, expected_hash, accept, |_| Ok(()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InstallStep { Open, Write, Sync, Publish, Save }

struct SkillStage(PathBuf);
impl Drop for SkillStage {
    fn drop(&mut self) {
        // This fresh generated directory is owned solely by this installation.
        // It is outside all discovery roots, including during a failed write.
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn install_exact(data: &Path, id: &str, body: &str, hook: &mut impl FnMut(InstallStep) -> Result<(), String>) -> Result<(), String> {
    use std::io::Write;
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') { return Err("Invalid skill proposal identity".into()); }
    let root = data.join("skills");
    let directory = root.join(format!("reviewed-{id}"));
    if fs::symlink_metadata(&directory).is_ok() {
        let file = directory.join("SKILL.md");
        let exact = fs::symlink_metadata(&directory).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
            && fs::symlink_metadata(&file).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
            && read(&file).is_ok_and(|installed| installed == body);
        return if exact { Ok(()) } else { Err("Existing installation differs from reviewed content; nothing overwritten".into()) };
    }
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let staging = data.join(format!(".skill-stage-{}", crate::workbench::new_id()));
    fs::create_dir(&staging).map_err(|e| format!("Cannot stage skill: {e}"))?;
    let staging = SkillStage(staging);
    hook(InstallStep::Open)?;
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(staging.0.join("SKILL.md")).map_err(|e| e.to_string())?;
    hook(InstallStep::Write)?;
    file.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
    hook(InstallStep::Sync)?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    hook(InstallStep::Publish)?;
    // The complete directory enters discovery in one rename. An existing
    // nonempty installation cannot be replaced by this operation.
    fs::rename(&staging.0, directory).map_err(|e| format!("Cannot publish reviewed skill: {e}"))?;
    Ok(())
}

fn decide_with_hook(db: &Db, data: &Path, id: &str, expected_hash: &str, accept: bool, mut hook: impl FnMut(InstallStep) -> Result<(), String>) -> Result<(), String> {
    let mut state = load(db)?;
    let proposal = state.proposals.iter().find(|p| p.id == id).ok_or("Proposal no longer exists")?;
    if proposal.content_hash != expected_hash || hash(&proposal.body) != expected_hash { return Err("Proposal changed; review it again before deciding".into()); }
    if accept {
        if proposal.audit_url.is_some() && proposal.audit_reviewed_hash.as_deref() != Some(expected_hash) {
            return Err("Open the linked audit, review its published results, then explicitly confirm that review before installing".into());
        }
        install_exact(data, &proposal.id, &proposal.body, &mut hook)?;
    }
    state.proposals.retain(|p| p.id != id);
    hook(InstallStep::Save)?;
    save(db, &state)
}

pub fn confirm_audit_review(db: &Db, id: &str, expected_hash: &str, audit_url: &str) -> Result<(), String> {
    let mut state = load(db)?;
    let proposal = state.proposals.iter_mut().find(|p| p.id == id).ok_or("Proposal no longer exists")?;
    if proposal.content_hash != expected_hash || hash(&proposal.body) != expected_hash || proposal.audit_url.as_deref() != Some(audit_url) {
        return Err("Skill content or audit link changed; review both again".into());
    }
    proposal.audit_reviewed_hash = Some(expected_hash.into());
    proposal.audit_status = "You confirmed review of the linked published audit results for these instructions. Neko has not verified the audit or declared this skill safe.".into();
    save(db, &state)
}

/// Fetch a single, reviewable instruction file. No archive extraction, scripts,
/// redirects, arbitrary hosts or relative asset downloads.
pub fn repository_preview(url: &str) -> Result<(String, String, String), String> {
    let url = reqwest::Url::parse(url).map_err(|_| "Use a GitHub SKILL.md file URL")?;
    if url.scheme() != "https" || url.host_str() != Some("github.com") || url.port().is_some() || !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
        return Err("Use an HTTPS github.com SKILL.md file URL".into());
    }
    let segments: Vec<_> = url.path_segments().ok_or("Missing skill path")?.collect();
    if segments.len() < 6 || segments[2] != "blob" || segments.last() != Some(&"SKILL.md") || segments.iter().any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)) || *s == "." || *s == "..") {
        return Err("Use github.com/owner/repo/blob/ref/path/SKILL.md (a commit SHA is preferred)".into());
    }
    let raw = format!("https://raw.githubusercontent.com/{}/{}/{}/{}", segments[0], segments[1], segments[3], segments[4..].join("/"));
    let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(15)).redirect(reqwest::redirect::Policy::none()).build().map_err(|e| e.to_string())?;
    let response = client.get(raw).send().map_err(|e| format!("Cannot fetch skill: {e}"))?.error_for_status().map_err(|e| format!("Cannot fetch skill: {e}"))?;
    let mut body = String::new();
    response.take((MAX_SKILL_BYTES + 1) as u64).read_to_string(&mut body).map_err(|e| e.to_string())?;
    if body.is_empty() || body.len() > MAX_SKILL_BYTES { return Err("Remote skill is empty or exceeds 64 KB".into()); }
    let (name, _) = metadata(&body, segments[segments.len()-2]);
    let audit = format!("https://skills.sh/{}/{}/{}", segments[0], segments[1], segments[segments.len()-2]);
    Ok((name, body, audit))
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
    fn proposals_require_exact_approval_and_rejection_never_installs() {
        let db = Db::open_in_memory().unwrap();
        let data = tempfile::tempdir().unwrap();
        let body = "---\nname: learned\ndescription: Test before commit\n---\nRun cargo test.";
        propose(&db, "a", "Learned", body, "ticket:1", None).unwrap();
        let proposal = load(&db).unwrap().proposals[0].clone();
        assert!(!data.path().join("skills").exists());
        assert!(decide(&db, data.path(), &proposal.id, "old hash", true).is_err());
        assert!(!data.path().join("skills").exists());
        decide(&db, data.path(), &proposal.id, &proposal.content_hash, false).unwrap();
        assert!(!data.path().join("skills").exists());
        assert!(load(&db).unwrap().proposals.is_empty());
        propose(&db, "a", "Learned", body, "ticket:2", None).unwrap();
        let proposal = load(&db).unwrap().proposals[0].clone();
        decide(&db, data.path(), &proposal.id, &proposal.content_hash, true).unwrap();
        assert_eq!(fs::read_to_string(data.path().join("skills").join(format!("reviewed-{}", proposal.id)).join("SKILL.md")).unwrap(), body);
        assert!(load(&db).unwrap().enabled.is_empty());
    }

    #[test]
    fn installation_failures_never_publish_partial_content_and_retry_succeeds() {
        for failure in [InstallStep::Open, InstallStep::Write, InstallStep::Sync, InstallStep::Publish, InstallStep::Save] {
            let db = Db::open_in_memory().unwrap();
            let data = tempfile::tempdir().unwrap();
            propose(&db, "w", "Safe", "Complete reviewed instructions", "ticket:test", None).unwrap();
            let proposal = load(&db).unwrap().proposals[0].clone();
            let installed = data.path().join("skills").join(format!("reviewed-{}", proposal.id)).join("SKILL.md");
            let result = decide_with_hook(&db, data.path(), &proposal.id, &proposal.content_hash, true, |step| {
                if step == failure { Err("Injected installation failure".into()) } else { Ok(()) }
            });
            assert!(result.is_err(), "{failure:?}");
            assert_eq!(load(&db).unwrap().proposals.len(), 1);
            assert!(!fs::read_dir(data.path()).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with(".skill-stage-")));
            if failure == InstallStep::Save {
                assert_eq!(fs::read_to_string(&installed).unwrap(), proposal.body);
            } else {
                assert!(!installed.exists());
                assert!(discover(&[Root { path: data.path().join("skills"), source: "Neko".into(), workspace_id: None }]).is_empty());
            }
            decide(&db, data.path(), &proposal.id, &proposal.content_hash, true).unwrap();
            assert_eq!(fs::read_to_string(installed).unwrap(), proposal.body);
            assert!(load(&db).unwrap().proposals.is_empty());
        }
    }

    #[test]
    fn retry_after_database_failure_refuses_changed_installed_content() {
        let db = Db::open_in_memory().unwrap();
        let data = tempfile::tempdir().unwrap();
        propose(&db, "w", "Safe", "Reviewed instructions", "ticket:test", None).unwrap();
        let proposal = load(&db).unwrap().proposals[0].clone();
        assert!(decide_with_hook(&db, data.path(), &proposal.id, &proposal.content_hash, true, |step| {
            if step == InstallStep::Save { Err("Injected database failure".into()) } else { Ok(()) }
        }).is_err());
        let installed = data.path().join("skills").join(format!("reviewed-{}", proposal.id)).join("SKILL.md");
        fs::write(&installed, "Changed after publication").unwrap();
        assert!(decide(&db, data.path(), &proposal.id, &proposal.content_hash, true).unwrap_err().contains("differs"));
        assert_eq!(fs::read_to_string(installed).unwrap(), "Changed after publication");
        assert_eq!(load(&db).unwrap().proposals.len(), 1);
    }

    #[test]
    fn repository_install_requires_content_and_link_pinned_audit_review() {
        let db = Db::open_in_memory().unwrap();
        let data = tempfile::tempdir().unwrap();
        let audit = "https://skills.sh/owner/repo/skill";
        propose(&db, "a", "Review", "Instructions", "https://github.com/owner/repo/blob/main/skill/SKILL.md", Some(audit.into())).unwrap();
        let proposal = load(&db).unwrap().proposals[0].clone();
        assert!(decide(&db, data.path(), &proposal.id, &proposal.content_hash, true).unwrap_err().contains("audit"));
        assert!(!data.path().join("skills").exists());
        assert!(confirm_audit_review(&db, &proposal.id, "wrong hash", audit).is_err());
        assert!(confirm_audit_review(&db, &proposal.id, &proposal.content_hash, "https://different.example").is_err());
        confirm_audit_review(&db, &proposal.id, &proposal.content_hash, audit).unwrap();
        let mut state = load(&db).unwrap();
        assert_eq!(state.proposals[0].audit_reviewed_hash.as_deref(), Some(proposal.content_hash.as_str()));
        assert!(state.proposals[0].audit_status.contains("not verified"));
        state.proposals[0].body = "Changed reviewed content".into();
        let changed_hash = hash(&state.proposals[0].body);
        state.proposals[0].content_hash = changed_hash.clone();
        save(&db, &state).unwrap();
        assert!(decide(&db, data.path(), &proposal.id, &changed_hash, true).is_err());
        assert!(!data.path().join("skills").exists());
        confirm_audit_review(&db, &proposal.id, &changed_hash, audit).unwrap();
        decide(&db, data.path(), &proposal.id, &changed_hash, true).unwrap();
        assert!(data.path().join("skills").exists());
    }

    #[test]
    fn skill_state_is_separate_from_ticket_snapshot_and_survives_ordinary_writes() {
        let db = Db::open_in_memory().unwrap();
        propose(&db, "a", "Learning", "Instructions", "ticket:1", None).unwrap();
        let snapshot = crate::workbench::load(&db).unwrap();
        assert_eq!(snapshot.skills.proposals.len(), 1);
        crate::workbench::save(&db, &snapshot).unwrap();
        assert_eq!(crate::workbench::load(&db).unwrap().skills.proposals.len(), 1);
        let raw = db.get_setting("workbench_snapshot_v1").unwrap().unwrap();
        assert!(!raw.contains("Instructions"));
    }

    #[test]
    fn remote_preview_rejects_non_github_and_ambiguous_urls_without_network() {
        for url in ["http://github.com/a/b/blob/main/x/SKILL.md", "https://127.0.0.1/a", "https://github.com/a/b/raw/main/x/SKILL.md", "https://token@github.com/a/b/blob/main/x/SKILL.md", "https://github.com/a/b/blob/main/x/SKILL.md?token=secret", "https://github.com/a/b/blob/main/x/script.sh"] {
            assert!(repository_preview(url).is_err(), "{url}");
        }
    }

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
