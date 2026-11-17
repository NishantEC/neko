//! Applying, rejecting, undoing and verifying a suggestion. Nothing here runs without the
//! person: `apply`, `reject` and `undo` are what a click (or the Inbox item's default, which the
//! person accepts) calls, and `verify` only watches. The one exception is a skill Neko wrote that
//! nobody changed, revised by lines already in it: the curator's own rule
//! (`skills::curator::submit`), unchanged, which a click on Apply reaches.
//!
//! Side effects outside the database (packs, tasks) are behind `Effects`, called with no database
//! lock held, so tests run without a model, a pack or a repository.
use super::{close_item, deliver_undo, load, note, save, save_undo, undo_record, Undo, DAY_MS, VERIFY_DAYS};
use crate::skills::{curator, revise};
use crate::{inbox, routines, Db};
use neko_protocol::improve::{ImproveKind, ImproveState, Improvement};
use std::path::Path;
use std::sync::Mutex;

/// What a suggestion needs from outside the database.
pub trait Effects {
    fn catalog_has(&self, pack: &str) -> bool;
    fn pack_installed(&self, pack: &str) -> bool;
    fn remove_pack(&self, pack: &str) -> Result<(), String>;
    /// The workspace for the Development checkout and the repository's check command ("" when unknown).
    /// An error says why there is none (no checkout set, or it is not a workspace).
    fn dev_target(&self) -> Result<(String, String), String>;
    /// Makes a task with a goal. Returns the task's id.
    fn create_task(&self, workspace_id: &str, objective: &str, done_check: &str) -> Result<String, String>;
    /// Reverses a code task's runs through their checkpoints, or makes a revert task once they were
    /// pushed. Returns what was done, in a line.
    fn undo_task(&self, task_id: &str, title: &str) -> Result<String, String>;
}

/// What an apply did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    pub note: String,
    /// A pack suggestion: the source whose preview the client opens. Nothing is installed.
    pub open_pack: Option<String>,
}

/// One apply, reject, undo or verification at a time.
static ONE: Mutex<()> = Mutex::new(());

fn lock(dbm: &Mutex<Db>) -> std::sync::MutexGuard<'_, Db> {
    dbm.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn date_of(now: i64) -> String {
    chrono::DateTime::from_timestamp_millis(now).map_or_else(String::new, |t| t.format("%Y-%m-%d").to_string())
}

fn find(db: &Db, id: &str) -> Result<Improvement, String> {
    load(db).into_iter().find(|p| p.id == id).ok_or_else(|| "That suggestion is gone".to_owned())
}

fn store(db: &Db, p: &Improvement) -> Result<(), String> {
    let mut all = load(db);
    match all.iter_mut().find(|q| q.id == p.id) {
        Some(slot) => *slot = p.clone(),
        None => all.push(p.clone()),
    }
    save(db, &all)
}

/// Applying starts the watch: the signal's count in the 7 days before.
fn start_watch(db: &Db, p: &mut Improvement, now: i64) {
    p.state = ImproveState::Applied;
    p.applied_ms = Some(now);
    p.before = super::signals::count_since(db, &p.signal_key, now - VERIFY_DAYS * DAY_MS, now);
    p.after = 0;
}

/// A standing order's line, added to the workspace's text. Returns what an undo needs.
fn add_order(db: &Db, workspace_id: &str, line: &str) -> Result<Undo, String> {
    if workspace_id.is_empty() {
        return Err("This suggestion has no workspace".into());
    }
    let mut definitions = routines::load(db)?;
    let had_entry = definitions.standing_orders.contains_key(workspace_id);
    let before = definitions.standing_orders.get(workspace_id).cloned().unwrap_or_else(|| routines::DEFAULT_STANDING_ORDERS.to_owned());
    if before.lines().any(|l| l.trim() == line) {
        return Err("That line is already in the standing orders".into());
    }
    let after = format!("{}\n{line}", before.trim_end());
    if after.len() > routines::MAX_ORDERS {
        return Err(format!("Standing orders are full (limit {} KB); shorten them first", routines::MAX_ORDERS / 1024));
    }
    let undo = Undo { order_workspace: workspace_id.to_owned(), order_line: line.to_owned(), order_before: before, order_had_entry: had_entry, ..Undo::default() };
    definitions.standing_orders.insert(workspace_id.to_owned(), after);
    routines::save_definitions(db, &definitions)?;
    Ok(undo)
}

/// Takes the line out again. When the text is exactly what the apply wrote, the earlier text is
/// restored exactly (including "no entry, so the default"); otherwise just the line goes.
fn remove_order(db: &Db, undo: &Undo) -> Result<String, String> {
    let mut definitions = routines::load(db)?;
    let Some(current) = definitions.standing_orders.get(&undo.order_workspace).cloned() else { return Ok("The standing orders were already back to the default".into()) };
    let written = format!("{}\n{}", undo.order_before.trim_end(), undo.order_line);
    if current == written {
        if undo.order_had_entry {
            definitions.standing_orders.insert(undo.order_workspace.clone(), undo.order_before.clone());
        } else {
            definitions.standing_orders.remove(&undo.order_workspace);
        }
        routines::save_definitions(db, &definitions)?;
        return Ok("Removed the line from the standing orders".into());
    }
    let mut lines: Vec<&str> = current.lines().collect();
    let Some(at) = lines.iter().rposition(|l| l.trim() == undo.order_line) else { return Ok("The line was already gone from the standing orders".into()) };
    lines.remove(at);
    definitions.standing_orders.insert(undo.order_workspace.clone(), lines.join("\n"));
    routines::save_definitions(db, &definitions)?;
    Ok("Removed the line from the standing orders".into())
}

/// Applies a suggestion the person chose. `closed_as` is the reason its Inbox item closes with.
pub fn apply(dbm: &Mutex<Db>, fx: &dyn Effects, data: &Path, id: &str, now: i64, closed_as: &str) -> Result<Applied, String> {
    let _one = ONE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut p = find(&lock(dbm), id)?;
    if p.state != ImproveState::Open {
        return Err(match p.state {
            ImproveState::Sent => "It was already sent to Skills; approve it there".to_owned(),
            _ => "That suggestion was already handled".to_owned(),
        });
    }
    let mut result = Applied::default();
    let undo: Undo;
    match p.kind {
        ImproveKind::StandingOrder => {
            let db = lock(dbm);
            undo = add_order(&db, &p.workspace_id, &p.change)?;
            start_watch(&db, &mut p, now);
            result.note = "Added the line to the standing orders.".into();
        }
        ImproveKind::SkillPatch => {
            let db = lock(dbm);
            let source = format!("{} improvement {}", curator::SOURCE_PREFIX, p.id);
            let sent = revise::submit_patch(&db, data, &p.target, &p.change, &p.workspace_id, &source, now, &date_of(now))?;
            undo = Undo { skill_path: sent.path, skill_base: sent.base, skill_revised: sent.revised, skill_source: source, ..Undo::default() };
            match sent.outcome {
                curator::Outcome::Applied(_) => {
                    start_watch(&db, &mut p, now);
                    result.note = "Applied: the skill is one Neko wrote, and the change only picks among lines already in it.".into();
                }
                curator::Outcome::Proposed(why) => {
                    let why = match why {
                        "locked" => "the skill is locked",
                        "edited" => "the skill was edited by hand",
                        "other" => "Neko did not write this skill",
                        "changed wording" => "the change adds wording",
                        other => other,
                    };
                    p.state = ImproveState::Sent;
                    result.note = format!("Sent to Skills for your approval ({why}).");
                }
                curator::Outcome::Unchanged => return Err("The revision changes nothing".into()),
            }
        }
        ImproveKind::NewSkill => {
            let db = lock(dbm);
            let source = format!("Improvement {}", p.id);
            let proposal = revise::submit_new(&db, &p.workspace_id, &p.change, &source)?;
            undo = Undo { skill_source: source, new_skill_proposal: proposal, ..Undo::default() };
            p.state = ImproveState::Sent;
            result.note = "Sent to Skills for your approval.".into();
        }
        ImproveKind::InstallPack => {
            if !fx.catalog_has(&p.target) {
                return Err(format!("The {} pack is not in the catalog any more", p.target));
            }
            // Only the preview opens. The person clicks Install there and the digest flow takes over.
            note(&mut p, now, "Opened the pack's preview");
            let db = lock(dbm);
            store(&db, &p)?;
            // The person took the suggestion up; the row stays open in Improvements until the pack is installed.
            if let Some(seq) = p.inbox_seq {
                close_item(&db, seq, closed_as, now)?;
            }
            drop(db);
            result.open_pack = Some(format!("catalog:{}", p.target));
            result.note = format!("Opened the {} preview. Install it there when you are ready; nothing was installed.", p.target);
            return Ok(result);
        }
        ImproveKind::CodeTask => {
            let (workspace, check) = fx.dev_target()?;
            let objective = format!("{}\n\nDone when the repository's checks pass.", p.change.trim());
            let task = fx.create_task(&workspace, &objective, &check)?;
            undo = Undo { task_id: task.clone(), ..Undo::default() };
            p.task_id = Some(task);
            start_watch(&lock(dbm), &mut p, now);
            result.note = "Started a task in the Development checkout.".into();
        }
    }
    let db = lock(dbm);
    save_undo(&db, &p.id, &undo)?;
    note(&mut p, now, result.note.clone());
    store(&db, &p)?;
    if let Some(seq) = p.inbox_seq {
        close_item(&db, seq, closed_as, now)?;
    }
    Ok(result)
}

/// The person turned it down. A skill proposal waiting in Skills is withdrawn. The key is not
/// suggested again for 30 days.
pub fn reject(dbm: &Mutex<Db>, data: &Path, id: &str, now: i64, closed_as: &str) -> Result<(), String> {
    let _one = ONE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let db = lock(dbm);
    let mut p = find(&db, id)?;
    if !matches!(p.state, ImproveState::Open | ImproveState::Sent) {
        return Err("Only a suggestion that is still open can be rejected; undo one that was applied".into());
    }
    let undo = undo_record(&db, id);
    if p.state == ImproveState::Sent && !undo.skill_source.is_empty() {
        revise::withdraw(&db, data, &undo.skill_source)?;
    }
    p.state = ImproveState::Rejected;
    p.resolved_ms = Some(now);
    note(&mut p, now, "Rejected");
    store(&db, &p)?;
    if let Some(seq) = p.inbox_seq {
        close_item(&db, seq, closed_as, now)?;
    }
    Ok(())
}

/// Reverses an applied suggestion exactly: a skill from the copy kept when it was changed (only if
/// nobody edited it since), a standing order's line, a pack by Remove, a code task by its
/// checkpoints (or a revert task once pushed). The suggestion then counts as rejected, so the same
/// key is not suggested again for 30 days.
pub fn undo(dbm: &Mutex<Db>, fx: &dyn Effects, data: &Path, id: &str, now: i64, closed_as: &str) -> Result<String, String> {
    let _one = ONE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut p = find(&lock(dbm), id)?;
    if !matches!(p.state, ImproveState::Applied | ImproveState::Helped | ImproveState::DidntHelp) {
        return Err("Only an applied suggestion can be undone".into());
    }
    let undo = undo_record(&lock(dbm), id);
    let said = match p.kind {
        ImproveKind::StandingOrder => remove_order(&lock(dbm), &undo)?,
        ImproveKind::SkillPatch => {
            revise::restore(&lock(dbm), data, &undo.skill_path, &undo.skill_base, &undo.skill_revised, &p.id, now)?;
            "Restored the skill to what it was before".to_owned()
        }
        ImproveKind::NewSkill => {
            revise::archive_new(&lock(dbm), data, &undo.new_skill_proposal, &p.id, now)?;
            "Moved the new skill to the cleanup archive".to_owned()
        }
        ImproveKind::InstallPack => {
            fx.remove_pack(&undo.pack)?;
            format!("Removed the {} pack", undo.pack)
        }
        ImproveKind::CodeTask => fx.undo_task(&undo.task_id, &p.title)?,
    };
    let db = lock(dbm);
    p.state = ImproveState::Rejected;
    p.resolved_ms = Some(now);
    note(&mut p, now, format!("Undone. {said}"));
    store(&db, &p)?;
    for seq in [p.inbox_seq, p.undo_seq].into_iter().flatten() {
        close_item(&db, seq, closed_as, now)?;
    }
    Ok(said)
}

/// Watches what was applied (called by the daemon's idle tick and before a list is shown):
/// - a pack the person installed from its preview starts its watch;
/// - a skill sent to Skills becomes applied once approved, or rejected once turned down there;
/// - after 7 days the signal's count is compared with the week before: fired at least as often
///   means "didn't help" and an Inbox item whose default is Undo; fewer or none means "Helped".
///
/// Returns how many suggestions changed.
pub fn verify(dbm: &Mutex<Db>, fx: &dyn Effects, data: &Path, now: i64) -> Result<usize, String> {
    let _one = ONE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    // Asking about packs may take the database lock itself, so it is done before this one is held.
    let waiting: Vec<(String, String)> = load(&lock(dbm)).into_iter().filter(|p| p.kind == ImproveKind::InstallPack && p.state == ImproveState::Open).map(|p| (p.id, p.target)).collect();
    let installed: std::collections::HashSet<String> = waiting.into_iter().filter(|(_, pack)| fx.pack_installed(pack)).map(|(id, _)| id).collect();
    let db = lock(dbm);
    let mut changed = 0;
    for mut p in load(&db) {
        let before = p.state;
        match p.state {
            ImproveState::Open if installed.contains(&p.id) => {
                save_undo(&db, &p.id, &Undo { pack: p.target.clone(), ..Undo::default() })?;
                start_watch(&db, &mut p, now);
                let said = format!("The {} pack was installed", p.target);
                note(&mut p, now, said);
                if let Some(seq) = p.inbox_seq {
                    close_item(&db, seq, inbox::BY_YOU, now)?;
                }
            }
            ImproveState::Sent => settle_sent(&db, data, &mut p, now)?,
            ImproveState::Applied => {
                let Some(applied) = p.applied_ms else { continue };
                if now < applied + VERIFY_DAYS * DAY_MS {
                    continue;
                }
                p.after = super::signals::count_since(&db, &p.signal_key, applied, applied + VERIFY_DAYS * DAY_MS);
                p.resolved_ms = Some(now);
                if p.after >= p.before.max(1) {
                    p.state = ImproveState::DidntHelp;
                    let said = format!("Didn't help: it fired {} time(s) in the week before and {} in the week after", p.before, p.after);
                    note(&mut p, now, said);
                    deliver_undo(&db, &mut p, now)?;
                } else {
                    p.state = ImproveState::Helped;
                    let said = format!("Helped: it fired {} time(s) in the week before and {} in the week after", p.before, p.after);
                    note(&mut p, now, said);
                }
            }
            _ => continue,
        }
        if p.state != before {
            store(&db, &p)?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// A skill suggestion waiting in Skills: approved there (its file or folder is as proposed) means
/// applied; gone without that means the person turned it down.
fn settle_sent(db: &Db, data: &Path, p: &mut Improvement, now: i64) -> Result<(), String> {
    let undo = undo_record(db, &p.id);
    let accepted = match p.kind {
        ImproveKind::SkillPatch => {
            if revise::proposal_pending(db, &undo.skill_source) {
                return Ok(());
            }
            revise::file_hash(&undo.skill_path).is_some_and(|h| h == revise::hash_of(&undo.skill_revised))
        }
        ImproveKind::NewSkill => {
            if crate::skills::load(db)?.proposals.iter().any(|q| q.id == undo.new_skill_proposal) {
                return Ok(());
            }
            revise::installed_new(data, &undo.new_skill_proposal).is_some()
        }
        _ => return Ok(()),
    };
    if accepted {
        start_watch(db, p, now);
        note(p, now, "Approved in Skills");
    } else {
        p.state = ImproveState::Rejected;
        p.resolved_ms = Some(now);
        note(p, now, "Turned down in Skills");
    }
    if let Some(seq) = p.inbox_seq {
        close_item(db, seq, inbox::BY_YOU, now)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::improve::signals::{record, Inputs, Seen};
    use neko_protocol::skills::{EnabledSkill, Skill, SkillState};
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct Fake {
        installed: Cell<bool>,
        installs: Cell<u32>,
        removed: RefCell<Vec<String>>,
        dev: Option<(String, String)>,
        tasks: RefCell<Vec<(String, String, String)>>,
        undone: RefCell<Vec<String>>,
    }
    impl Effects for Fake {
        fn catalog_has(&self, pack: &str) -> bool {
            pack == "linear"
        }
        fn pack_installed(&self, pack: &str) -> bool {
            pack == "linear" && self.installed.get()
        }
        fn remove_pack(&self, pack: &str) -> Result<(), String> {
            self.removed.borrow_mut().push(pack.to_owned());
            self.installed.set(false);
            Ok(())
        }
        fn dev_target(&self) -> Result<(String, String), String> {
            self.dev.clone().ok_or_else(|| "Set a Development checkout in Settings, Neko updates, and add it as a workspace".to_owned())
        }
        fn create_task(&self, workspace_id: &str, objective: &str, done_check: &str) -> Result<String, String> {
            self.tasks.borrow_mut().push((workspace_id.into(), objective.into(), done_check.into()));
            Ok("task-1".into())
        }
        fn undo_task(&self, task_id: &str, _title: &str) -> Result<String, String> {
            self.undone.borrow_mut().push(task_id.to_owned());
            Ok("Undid the task's runs".into())
        }
    }

    const SKILL: &str = "---\nname: reviews\ndescription: Reviewing code\n---\n# Steps\n- Read the diff.\n- Run the tests.\n\n## Lessons\n- Check types first.\n- Check the types first.\n";

    struct Rig {
        _dir: tempfile::TempDir,
        data: std::path::PathBuf,
        path: String,
        db: Mutex<Db>,
    }

    fn rig(skill_by_neko: bool) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let data = std::fs::canonicalize(dir.path()).unwrap();
        let folder = data.join("skills/reviewed-abc");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("SKILL.md");
        std::fs::write(&path, SKILL).unwrap();
        let db = Db::open_in_memory().unwrap();
        let skill = Skill { path: path.to_string_lossy().into(), name: "reviews".into(), description: "Reviewing code".into(), source: "neko".into(), workspace_id: None, content_hash: crate::skills::revise::hash_of(SKILL) };
        let mut state = SkillState::default();
        state.available.push(skill.clone());
        state.enabled.push(EnabledSkill { workspace_id: "w1".into(), path: skill.path.clone(), content_hash: skill.content_hash.clone() });
        crate::skills::save(&db, &state).unwrap();
        if skill_by_neko {
            curator::note_installed(&db, &path, SKILL, "Completed ticket 1").unwrap();
        }
        Rig { path: path.to_string_lossy().into(), data, _dir: dir, db: Mutex::new(db) }
    }

    fn proposal(kind: ImproveKind, key: &str) -> Improvement {
        Improvement { id: format!("p-{key}"), signal_key: key.into(), kind, title: "Fix it".into(), why: "It keeps happening.".into(), default_label: "Do it".into(), workspace_id: "w1".into(), created_ms: 1000, ..Default::default() }
    }

    fn add(rig: &Rig, p: Improvement) -> Improvement {
        let db = rig.db.lock().unwrap();
        let mut all = load(&db);
        all.push(p.clone());
        save(&db, &all).unwrap();
        super::super::deliver(&db, &p.id, 1000).unwrap();
        find(&db, &p.id).unwrap()
    }

    fn open_items(rig: &Rig) -> Vec<(String, Option<String>, Option<String>)> {
        let db = rig.db.lock().unwrap();
        routines::load(&db)
            .unwrap()
            .routines
            .iter()
            .filter(|r| r.id.starts_with(super::super::ROUTINE_PREFIX))
            .flat_map(|r| routines::load_runs(&db, &r.id).unwrap())
            .flat_map(|r| r.needs_you)
            .filter(|i| !i.done)
            .map(|i| (i.title, i.default, i.link))
            .collect()
    }

    const DAY: i64 = DAY_MS;

    #[test]
    fn a_suggestion_is_one_inbox_item_with_the_apply_action_as_its_default() {
        let rig = rig(false);
        let mut p = proposal(ImproveKind::StandingOrder, "ask:which database");
        p.change = "Say which branch a pull request targets.".into();
        p.title = "Say the target branch".into();
        p.default_label = "Add this standing order".into();
        add(&rig, p);
        assert_eq!(open_items(&rig), vec![("Neko suggests: Say the target branch".to_owned(), Some("Add this standing order".to_owned()), Some("improve:p-ask:which database".to_owned()))]);
    }

    #[test]
    fn a_standing_order_needs_the_click_and_undo_restores_the_text_exactly() {
        let rig = rig(false);
        let fx = Fake::default();
        let mut p = proposal(ImproveKind::StandingOrder, "ask:x");
        p.change = "Say which branch a pull request targets.".into();
        let p = add(&rig, p);
        // Nothing changed before the click, however long it sits.
        assert!(routines::load(&rig.db.lock().unwrap()).unwrap().standing_orders.is_empty());
        verify(&rig.db, &fx, &rig.data, 100 * DAY).unwrap();
        assert!(routines::load(&rig.db.lock().unwrap()).unwrap().standing_orders.is_empty());
        apply(&rig.db, &fx, &rig.data, &p.id, 2000, inbox::ANSWERED).unwrap();
        let orders = routines::load(&rig.db.lock().unwrap()).unwrap().standing_orders;
        assert!(orders["w1"].ends_with("\nSay which branch a pull request targets."));
        assert_eq!(find(&rig.db.lock().unwrap(), &p.id).unwrap().state, ImproveState::Applied);
        assert!(open_items(&rig).is_empty(), "the item closed");
        assert!(apply(&rig.db, &fx, &rig.data, &p.id, 3000, inbox::ANSWERED).is_err(), "applied once");
        undo(&rig.db, &fx, &rig.data, &p.id, 4000, inbox::ANSWERED).unwrap();
        assert!(routines::load(&rig.db.lock().unwrap()).unwrap().standing_orders.is_empty(), "no entry again, so the default");
        assert_eq!(find(&rig.db.lock().unwrap(), &p.id).unwrap().state, ImproveState::Rejected);
    }

    #[test]
    fn undoing_a_standing_order_keeps_the_persons_later_edits() {
        let rig = rig(false);
        let fx = Fake::default();
        {
            let db = rig.db.lock().unwrap();
            let mut store = routines::load(&db).unwrap();
            store.standing_orders.insert("w1".into(), "Be brief.".into());
            routines::save_definitions(&db, &store).unwrap();
        }
        let mut p = proposal(ImproveKind::StandingOrder, "ask:x");
        p.change = "Say which branch a pull request targets.".into();
        let p = add(&rig, p);
        apply(&rig.db, &fx, &rig.data, &p.id, 2000, inbox::ANSWERED).unwrap();
        {
            let db = rig.db.lock().unwrap();
            let mut store = routines::load(&db).unwrap();
            let text = store.standing_orders["w1"].clone();
            store.standing_orders.insert("w1".into(), format!("{text}\nNever merge on Fridays."));
            routines::save_definitions(&db, &store).unwrap();
        }
        undo(&rig.db, &fx, &rig.data, &p.id, 3000, inbox::ANSWERED).unwrap();
        assert_eq!(routines::load(&rig.db.lock().unwrap()).unwrap().standing_orders["w1"], "Be brief.\nNever merge on Fridays.");
    }

    #[test]
    fn a_skill_neko_did_not_write_goes_to_skills_for_approval_and_undo_restores_the_bytes() {
        let rig = rig(false);
        let fx = Fake::default();
        let mut p = proposal(ImproveKind::SkillPatch, "correction:types-first");
        p.target = "reviews".into();
        p.change = SKILL.replace("## Lessons", "- Always run the type check before reading the diff.\n\n## Lessons");
        let p = add(&rig, p);
        let done = apply(&rig.db, &fx, &rig.data, &p.id, 2000, inbox::ANSWERED).unwrap();
        assert!(done.note.contains("Skills"), "{}", done.note);
        assert_eq!(std::fs::read_to_string(&rig.path).unwrap(), SKILL, "the file is untouched until the person approves");
        let waiting = find(&rig.db.lock().unwrap(), &p.id).unwrap();
        assert_eq!(waiting.state, ImproveState::Sent);
        assert!(undo(&rig.db, &fx, &rig.data, &p.id, 2500, inbox::ANSWERED).is_err(), "nothing was applied to undo");
        // The person approves it in Skills.
        let proposal = crate::skills::load(&rig.db.lock().unwrap()).unwrap().proposals.pop().unwrap();
        crate::skills::decide(&rig.db.lock().unwrap(), &rig.data, &proposal.id, &proposal.content_hash, true).unwrap();
        assert_eq!(verify(&rig.db, &fx, &rig.data, 3000).unwrap(), 1);
        let applied = find(&rig.db.lock().unwrap(), &p.id).unwrap();
        assert_eq!(applied.state, ImproveState::Applied);
        assert!(std::fs::read_to_string(&rig.path).unwrap().contains("type check before reading"));
        undo(&rig.db, &fx, &rig.data, &p.id, 4000, inbox::ANSWERED).unwrap();
        assert_eq!(std::fs::read_to_string(&rig.path).unwrap(), SKILL, "byte for byte");
        assert!(rig.data.join("cleanup-archive/improve").is_dir(), "the replaced file is kept");
    }

    #[test]
    fn undo_refuses_a_skill_edited_since() {
        let rig = rig(false);
        let fx = Fake::default();
        let mut p = proposal(ImproveKind::SkillPatch, "correction:types-first");
        p.target = "reviews".into();
        p.change = SKILL.replace("## Lessons", "- Always run the type check before reading the diff.\n\n## Lessons");
        let p = add(&rig, p);
        apply(&rig.db, &fx, &rig.data, &p.id, 2000, inbox::ANSWERED).unwrap();
        let proposal = crate::skills::load(&rig.db.lock().unwrap()).unwrap().proposals.pop().unwrap();
        crate::skills::decide(&rig.db.lock().unwrap(), &rig.data, &proposal.id, &proposal.content_hash, true).unwrap();
        verify(&rig.db, &fx, &rig.data, 3000).unwrap();
        let edited = std::fs::read_to_string(&rig.path).unwrap() + "- My own note.\n";
        std::fs::write(&rig.path, &edited).unwrap();
        let error = undo(&rig.db, &fx, &rig.data, &p.id, 4000, inbox::ANSWERED).unwrap_err();
        assert!(error.contains("edited"), "{error}");
        assert_eq!(std::fs::read_to_string(&rig.path).unwrap(), edited, "nothing was written");
        assert_eq!(find(&rig.db.lock().unwrap(), &p.id).unwrap().state, ImproveState::Applied, "still applied");
    }

    #[test]
    fn a_neko_written_skill_changed_only_by_selection_is_applied_by_the_curators_rule() {
        let rig = rig(true);
        let fx = Fake::default();
        let mut p = proposal(ImproveKind::SkillPatch, "correction:types-first");
        p.target = "reviews".into();
        // Drops the duplicate lesson and adds nothing: selection only.
        p.change = SKILL.replace("- Check the types first.\n", "");
        let p = add(&rig, p);
        let done = apply(&rig.db, &fx, &rig.data, &p.id, 2000, inbox::ANSWERED).unwrap();
        assert!(done.note.starts_with("Applied"), "{}", done.note);
        let now = std::fs::read_to_string(&rig.path).unwrap();
        assert!(now.contains("## Archive") && now.contains("Check the types first."), "the dropped line is archived, not deleted");
        assert_eq!(find(&rig.db.lock().unwrap(), &p.id).unwrap().state, ImproveState::Applied);
        undo(&rig.db, &fx, &rig.data, &p.id, 4000, inbox::ANSWERED).unwrap();
        assert_eq!(std::fs::read_to_string(&rig.path).unwrap(), SKILL);
    }

    #[test]
    fn added_wording_on_a_neko_written_skill_is_still_only_a_proposal() {
        let rig = rig(true);
        let fx = Fake::default();
        let mut p = proposal(ImproveKind::SkillPatch, "correction:types-first");
        p.target = "reviews".into();
        p.change = SKILL.replace("## Lessons", "- A line the skill never had.\n\n## Lessons");
        let p = add(&rig, p);
        apply(&rig.db, &fx, &rig.data, &p.id, 2000, inbox::ANSWERED).unwrap();
        assert_eq!(std::fs::read_to_string(&rig.path).unwrap(), SKILL);
        assert_eq!(find(&rig.db.lock().unwrap(), &p.id).unwrap().state, ImproveState::Sent);
    }

    #[test]
    fn a_new_skill_is_reviewed_in_skills_and_undo_archives_it_never_deletes() {
        let rig = rig(false);
        let fx = Fake::default();
        let mut p = proposal(ImproveKind::NewSkill, "ask:release notes");
        p.target = "release-notes".into();
        p.change = "---\nname: release-notes\ndescription: Writing release notes\n---\n# Steps\n- List user-visible changes.\n".into();
        let p = add(&rig, p);
        apply(&rig.db, &fx, &rig.data, &p.id, 2000, inbox::ANSWERED).unwrap();
        assert!(!rig.data.join("skills").read_dir().unwrap().any(|e| e.unwrap().file_name().to_string_lossy().contains("release")), "not installed by apply");
        let proposal = crate::skills::load(&rig.db.lock().unwrap()).unwrap().proposals.pop().unwrap();
        crate::skills::decide(&rig.db.lock().unwrap(), &rig.data, &proposal.id, &proposal.content_hash, true).unwrap();
        verify(&rig.db, &fx, &rig.data, 3000).unwrap();
        assert_eq!(find(&rig.db.lock().unwrap(), &p.id).unwrap().state, ImproveState::Applied);
        let installed = rig.data.join("skills").join(format!("reviewed-{}", proposal.id));
        assert!(installed.join("SKILL.md").is_file());
        undo(&rig.db, &fx, &rig.data, &p.id, 4000, inbox::ANSWERED).unwrap();
        assert!(!installed.exists());
        let kept: Vec<_> = std::fs::read_dir(rig.data.join("cleanup-archive/improve")).unwrap().map(|e| e.unwrap().path()).collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(std::fs::read_to_string(kept[0].join("SKILL.md")).unwrap(), p.change);
    }

    #[test]
    fn rejecting_a_skill_waiting_in_skills_withdraws_it() {
        let rig = rig(false);
        let fx = Fake::default();
        let mut p = proposal(ImproveKind::NewSkill, "ask:release notes");
        p.change = "---\nname: release-notes\ndescription: Writing release notes\n---\n# Steps\n- List user-visible changes.\n".into();
        let p = add(&rig, p);
        apply(&rig.db, &fx, &rig.data, &p.id, 2000, inbox::ANSWERED).unwrap();
        reject(&rig.db, &rig.data, &p.id, 3000, inbox::BY_YOU).unwrap();
        assert!(crate::skills::load(&rig.db.lock().unwrap()).unwrap().proposals.is_empty());
        assert_eq!(find(&rig.db.lock().unwrap(), &p.id).unwrap().state, ImproveState::Rejected);
    }

    #[test]
    fn a_pack_suggestion_only_opens_the_preview_and_never_installs() {
        let rig = rig(false);
        let fx = Fake::default();
        let mut p = proposal(ImproveKind::InstallPack, "missing_tool:linear");
        p.target = "linear".into();
        let p = add(&rig, p);
        let done = apply(&rig.db, &fx, &rig.data, &p.id, 2000, inbox::ANSWERED).unwrap();
        assert_eq!(done.open_pack.as_deref(), Some("catalog:linear"));
        assert_eq!(fx.installs.get(), 0);
        assert!(!fx.installed.get());
        verify(&rig.db, &fx, &rig.data, 100 * DAY).unwrap();
        assert_eq!(find(&rig.db.lock().unwrap(), &p.id).unwrap().state, ImproveState::Open, "still waiting for the person's Install");
        // The person installs it from the preview.
        fx.installed.set(true);
        assert_eq!(verify(&rig.db, &fx, &rig.data, 3000).unwrap(), 1);
        assert_eq!(find(&rig.db.lock().unwrap(), &p.id).unwrap().state, ImproveState::Applied);
        let said = undo(&rig.db, &fx, &rig.data, &p.id, 4000, inbox::ANSWERED).unwrap();
        assert_eq!(fx.removed.borrow().as_slice(), ["linear"]);
        assert!(said.contains("Removed"));
        // A pack that left the catalog cannot be opened.
        let mut gone = proposal(ImproveKind::InstallPack, "missing_tool:other");
        gone.target = "other".into();
        let gone = add(&rig, gone);
        assert!(apply(&rig.db, &fx, &rig.data, &gone.id, 5000, inbox::ANSWERED).is_err());
    }

    #[test]
    fn a_code_task_is_refused_without_a_development_checkout_and_undone_through_its_checkpoints() {
        let rig = rig(false);
        let mut p = proposal(ImproveKind::CodeTask, "missing_tool:rg");
        p.change = "Make the runner find ripgrep on the PATH it gives agents.".into();
        let p = add(&rig, p);
        let none = Fake::default();
        let error = apply(&rig.db, &none, &rig.data, &p.id, 2000, inbox::ANSWERED).unwrap_err();
        assert!(error.contains("Development checkout"), "{error}");
        assert!(none.tasks.borrow().is_empty());
        assert_eq!(find(&rig.db.lock().unwrap(), &p.id).unwrap().state, ImproveState::Open);
        let with = Fake { dev: Some(("dev-ws".into(), "bash scripts/check.sh".into())), ..Fake::default() };
        apply(&rig.db, &with, &rig.data, &p.id, 3000, inbox::ANSWERED).unwrap();
        let tasks = with.tasks.borrow();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].0, "dev-ws");
        assert!(tasks[0].1.contains("ripgrep") && tasks[0].1.contains("checks pass"));
        assert_eq!(tasks[0].2, "bash scripts/check.sh");
        drop(tasks);
        assert_eq!(find(&rig.db.lock().unwrap(), &p.id).unwrap().task_id.as_deref(), Some("task-1"));
        undo(&rig.db, &with, &rig.data, &p.id, 4000, inbox::ANSWERED).unwrap();
        assert_eq!(with.undone.borrow().as_slice(), ["task-1"]);
    }

    #[test]
    fn rejecting_closes_the_item_and_remembers_the_key() {
        let rig = rig(false);
        let mut p = proposal(ImproveKind::StandingOrder, "ask:x");
        p.change = "Say which branch a pull request targets.".into();
        let p = add(&rig, p);
        reject(&rig.db, &rig.data, &p.id, 5000, inbox::BY_YOU).unwrap();
        assert!(open_items(&rig).is_empty());
        let rejected = find(&rig.db.lock().unwrap(), &p.id).unwrap();
        assert_eq!((rejected.state, rejected.resolved_ms), (ImproveState::Rejected, Some(5000)));
        assert!(reject(&rig.db, &rig.data, &p.id, 6000, inbox::BY_YOU).is_err());
    }

    #[test]
    fn a_week_later_verify_says_helped_or_raises_the_undo_item() {
        let rig = rig(false);
        let fx = Fake::default();
        let now = 50 * DAY;
        // Two signals fired three times the week before; after the apply one stops, the other keeps going.
        let seen = |id: &str, at: i64, text: &str| Seen { id: id.into(), at_ms: at, text: text.into(), workspace_id: "w1".into() };
        let before: Vec<Seen> = (0..3).flat_map(|n| [seen(&format!("a{n}"), now - 2 * DAY - n, "command not found: rg"), seen(&format!("b{n}"), now - 2 * DAY - n, "command not found: jq")]).collect();
        record(&rig.db.lock().unwrap(), &Inputs { failures: before, ..Default::default() }, now).unwrap();
        let mut a = proposal(ImproveKind::StandingOrder, "missing_tool:rg");
        a.change = "Use the rg binary at the path in the tools note.".into();
        let mut b = proposal(ImproveKind::StandingOrder, "missing_tool:jq");
        b.change = "Install jq before parsing JSON in a shell.".into();
        b.id = "p-jq".into();
        let (a, b) = (add(&rig, a), add(&rig, b));
        apply(&rig.db, &fx, &rig.data, &a.id, now, inbox::ANSWERED).unwrap();
        apply(&rig.db, &fx, &rig.data, &b.id, now, inbox::ANSWERED).unwrap();
        assert_eq!(find(&rig.db.lock().unwrap(), &a.id).unwrap().before, 3);
        // Day 3: jq still fails, rg does not.
        record(&rig.db.lock().unwrap(), &Inputs { failures: vec![seen("b9", now + 3 * DAY, "command not found: jq"), seen("b10", now + 4 * DAY, "command not found: jq"), seen("b11", now + 5 * DAY, "command not found: jq")], ..Default::default() }, now + 5 * DAY).unwrap();
        assert_eq!(verify(&rig.db, &fx, &rig.data, now + 6 * DAY).unwrap(), 0, "still watching");
        assert_eq!(verify(&rig.db, &fx, &rig.data, now + 8 * DAY).unwrap(), 2);
        let a_now = find(&rig.db.lock().unwrap(), &a.id).unwrap();
        let b_now = find(&rig.db.lock().unwrap(), "p-jq").unwrap();
        let (a, b) = (a_now, b_now);
        assert_eq!((a.state, a.after), (ImproveState::Helped, 0));
        assert_eq!((b.state, b.after), (ImproveState::DidntHelp, 3));
        assert!(b.undo_seq.is_some() && a.undo_seq.is_none());
        let items = open_items(&rig);
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].0, "That didn't help: Fix it");
        assert_eq!(items[0].1.as_deref(), Some("Undo it"));
        assert_eq!(items[0].2.as_deref(), Some("improve-undo:p-jq"));
        // Accepting that item's default is an Undo: the line goes, the item closes.
        undo(&rig.db, &fx, &rig.data, "p-jq", now + 9 * DAY, inbox::ANSWERED).unwrap();
        assert!(open_items(&rig).is_empty());
        assert!(!routines::load(&rig.db.lock().unwrap()).unwrap().standing_orders["w1"].contains("Install jq"));
        // Run again: nothing more happens.
        assert_eq!(verify(&rig.db, &fx, &rig.data, now + 20 * DAY).unwrap(), 0);
    }
}
