//! Skill changes a self-improvement suggestion makes (`neko_core::improve`). Every one goes
//! through the curator's rules (`curator::submit`) or the proposal list the person reviews in
//! Skills, so a suggestion never writes a skill file by itself unless the curator would. Nothing is
//! deleted: lines a revision drops are archived in the file, an undone file is kept in the
//! cleanup archive, and a removed new skill's folder is moved there.
use super::{curator, hash, lessons, load, metadata, read, save, MAX_SKILL_BYTES};
use crate::Db;
use neko_protocol::skills::Skill;
use std::path::{Path, PathBuf};

fn normal(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Where a suggestion's skill copies are kept: `<data>/cleanup-archive/improve/<label>`.
fn archive_dir(data: &Path, label: &str) -> PathBuf {
    data.join("cleanup-archive").join("improve").join(label.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect::<String>())
}

/// The skill named `name` among those Neko knows, and the workspace that has it enabled.
pub fn find(db: &Db, name: &str) -> Result<(Skill, Option<String>), String> {
    let state = load(db)?;
    let skill = state.available.iter().find(|s| normal(&s.name) == normal(name)).ok_or("That skill no longer exists")?.clone();
    let workspace = state.enabled.iter().find(|e| e.path == skill.path).map(|e| e.workspace_id.clone());
    Ok((skill, workspace))
}

/// What a skill revision would look like: the file now, the revision the curator's rules make of
/// the model's file (dropped lines archived, name kept), and its diff. An error when it is not a
/// complete SKILL.md or changes nothing.
pub struct Preview {
    pub base: String,
    pub revised: String,
    pub diff: String,
    pub selection_only: bool,
}

pub fn preview(path: &str, answer: &str, date: &str) -> Result<Preview, String> {
    let base = read(Path::new(path))?;
    let consolidated = curator::consolidate(&base, answer, date)?.ok_or("The revision changes nothing")?;
    let diff = lessons::unified_diff(&base, &consolidated.text);
    Ok(Preview { base, revised: consolidated.text, diff, selection_only: consolidated.auto_safe })
}

/// What happened to a skill patch: its facts for the undo and what the curator's rules did.
pub struct Submitted {
    pub outcome: curator::Outcome,
    pub path: String,
    pub base: String,
    pub revised: String,
    pub workspace_id: String,
}

/// Sends a revised skill through the curator's rules: applied by itself only for a skill Neko wrote
/// that nobody changed, and only when the revision merely selects among lines already there;
/// otherwise a proposal the person approves in Skills. `source` is unique per suggestion.
pub fn submit_patch(db: &Db, data: &Path, target: &str, answer: &str, fallback_workspace: &str, source: &str, now_ms: i64, date: &str) -> Result<Submitted, String> {
    let (skill, enabled_in) = find(db, target)?;
    let preview = preview(&skill.path, answer, date)?;
    let workspace_id = enabled_in.unwrap_or_else(|| fallback_workspace.to_owned());
    let outcome = curator::submit(db, data, &skill, &workspace_id, &preview.base, preview.revised.clone(), preview.selection_only, source, now_ms)?;
    Ok(Submitted { outcome, path: skill.path, base: preview.base, revised: preview.revised, workspace_id })
}

/// Whether the skill proposal made with `source` is still waiting for the person.
pub fn proposal_pending(db: &Db, source: &str) -> bool {
    load(db).map_or(false, |s| s.proposals.iter().any(|p| p.source == source))
}

/// Withdraws a waiting proposal (the suggestion was rejected before the person reviewed it in Skills).
pub fn withdraw(db: &Db, data: &Path, source: &str) -> Result<(), String> {
    if let Some(p) = load(db)?.proposals.into_iter().find(|p| p.source == source) {
        super::decide(db, data, &p.id, &p.content_hash, false)?;
    }
    Ok(())
}

/// The file's content hash now (None when it is gone).
pub fn file_hash(path: &str) -> Option<String> {
    read(Path::new(path)).ok().map(|body| hash(&body))
}

/// The hash of a revised file, as `submit_patch` recorded it.
pub fn hash_of(body: &str) -> String {
    hash(body)
}

/// Puts the skill back to exactly `original`, but only while it still holds exactly `written` (the
/// revision the suggestion made): a skill someone edited since is left alone. Goes through the
/// same proposal-and-accept path as every skill rewrite (base-hash check, atomic rename, enabled
/// entries moved to the new hash, the curator's record kept), and the file being replaced is first
/// copied to the cleanup archive.
pub fn restore(db: &Db, data: &Path, path: &str, original: &str, written: &str, label: &str, now_ms: i64) -> Result<(), String> {
    let current = read(Path::new(path))?;
    if current == original {
        return Ok(());
    }
    if hash(&current) != hash(written) {
        return Err("The skill was edited after the suggestion was applied, so nothing was restored. Review it in Skills.".into());
    }
    let (skill, enabled_in) = {
        let state = load(db)?;
        let skill = state.available.iter().find(|s| s.path == path).ok_or("That skill is no longer listed")?.clone();
        let enabled_in = state.enabled.iter().find(|e| e.path == path).map(|e| e.workspace_id.clone());
        (skill, enabled_in)
    };
    let source = format!("{} undo {label}", curator::SOURCE_PREFIX);
    let lesson = lessons::TicketLesson::Patch { target: skill, base: current.clone(), revised: original.to_owned() };
    let workspace = enabled_in.unwrap_or_default();
    lessons::propose_lesson(db, &workspace, &lesson, &source)?;
    let proposal = load(db)?.proposals.into_iter().find(|p| p.source == source && p.workspace_id == workspace).ok_or("The restore was not saved")?;
    // Keep what is being replaced before anything is written.
    let kept = archive_dir(data, &format!("{label}-{now_ms}"));
    std::fs::create_dir_all(&kept).map_err(|e| format!("Cannot keep a copy of the skill: {e}"))?;
    std::fs::write(kept.join("SKILL.md"), &current).map_err(|e| format!("Cannot keep a copy of the skill: {e}"))?;
    let done = super::decide(db, data, &proposal.id, &proposal.content_hash, true);
    if done.is_err() {
        let _ = super::decide(db, data, &proposal.id, &proposal.content_hash, false);
    } else {
        let _ = curator::mark_looked_at(db, path, &hash(original));
    }
    done
}

/// Checks a new skill the model wrote: a complete SKILL.md with a name no skill has.
pub fn check_new(db: &Db, body: &str) -> Result<String, String> {
    let body = body.trim_end().to_owned() + "\n";
    if !body.starts_with("---\n") || body.len() > MAX_SKILL_BYTES || body.contains('\r') {
        return Err("A new skill was not a complete SKILL.md".into());
    }
    let (name, _) = metadata(&body, "");
    if normal(&name).is_empty() {
        return Err("A new skill needs a name in its frontmatter".into());
    }
    if load(db)?.available.iter().any(|s| normal(&s.name) == normal(&name)) {
        return Err("A new skill reused an existing skill's name; that is an improvement to it".into());
    }
    Ok(name)
}

/// Saves a new skill as a proposal for the person to review in Skills (it is never installed
/// here). Returns the proposal's id.
pub fn submit_new(db: &Db, workspace_id: &str, body: &str, source: &str) -> Result<String, String> {
    let name = check_new(db, body)?;
    let body = body.trim_end().to_owned() + "\n";
    super::propose(db, workspace_id, &name, &body, source, None)?;
    load(db)?.proposals.into_iter().find(|p| p.source == source && p.workspace_id == workspace_id).map(|p| p.id).ok_or_else(|| "The proposal was not saved".to_owned())
}

/// The folder a new skill was installed in once its proposal was accepted, if it was.
pub fn installed_new(data: &Path, proposal_id: &str) -> Option<PathBuf> {
    let file = data.join("skills").join(format!("reviewed-{proposal_id}")).join("SKILL.md");
    std::fs::symlink_metadata(&file).ok().filter(|m| m.is_file()).map(|_| file)
}

/// Undo of a new skill: its folder is moved to the cleanup archive (never deleted) and the skill
/// leaves the lists. A folder someone changed since it was installed is moved as it is.
pub fn archive_new(db: &Db, data: &Path, proposal_id: &str, label: &str, now_ms: i64) -> Result<(), String> {
    let Some(file) = installed_new(data, proposal_id) else { return Ok(()) };
    let folder = file.parent().ok_or("Skill has no folder")?.to_path_buf();
    let target = archive_dir(data, &format!("{label}-{now_ms}"));
    std::fs::create_dir_all(target.parent().ok_or("No archive folder")?).map_err(|e| format!("Cannot keep a copy of the skill: {e}"))?;
    std::fs::rename(&folder, &target).map_err(|e| format!("Cannot move the skill to the archive: {e}"))?;
    let path = file.to_string_lossy().into_owned();
    let canonical = std::fs::canonicalize(&path).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| path.clone());
    let mut state = load(db)?;
    state.enabled.retain(|e| e.path != path && e.path != canonical);
    state.available.retain(|s| s.path != path && s.path != canonical);
    save(db, &state)?;
    let _ = curator::forget(db, &path);
    Ok(())
}
