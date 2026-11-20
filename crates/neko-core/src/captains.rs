//! Captain records: storage (the `captains` table, one JSON row each) and the
//! rules every change obeys. A captain's files are `captain_files`; the
//! requests are answered by `neko-daemon/src/captains.rs`. Nothing here reads
//! the clock, makes ids or finds the data folder: callers pass them in, so a
//! test never touches shared state. See `docs/knowledge/captains.md`.
use crate::db::Db;
use neko_protocol::captains::{
    Captain, CaptainSuggestion, FaceRef, ModelPolicy, FACE_PRESETS, MAX_ACTIVE_CAPTAINS, MAX_AREA_CHARS,
    MAX_DAILY_LIMIT_MICRO_USD, MAX_NAME_CHARS, MAX_PURPOSE_CHARS,
};
use neko_protocol::workbench::{Task, Workspace};
use std::path::Path;

/// Names `Suggest` draws from, in this order.
pub const CAT_NAMES: [&str; 12] = ["miso", "tofu", "nori", "pixel", "biscuit", "mochi", "kiki", "pepper", "olive", "juno", "waffles", "sesame"];

const DAY_MS: i64 = 86_400_000;

/// What `Create` is given.
pub struct NewCaptain {
    pub name: String,
    pub face: FaceRef,
    pub workspace_id: String,
    pub area: String,
    pub model_policy: ModelPolicy,
    pub daily_limit_micro_usd: Option<u64>,
    pub purpose: String,
}

/// What `Update` is given: only the fields that are `Some`.
#[derive(Default)]
pub struct Changes {
    pub name: Option<String>,
    pub face: Option<FaceRef>,
    pub area: Option<String>,
    pub model_policy: Option<ModelPolicy>,
    pub daily_limit_micro_usd: Option<Option<u64>>,
    pub purpose: Option<String>,
}

fn has_control(text: &str) -> bool {
    text.chars().any(char::is_control)
}

/// The trimmed name, or why it is refused.
pub fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the captain a name".into());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("A captain's name can be at most {MAX_NAME_CHARS} characters"));
    }
    if has_control(name) {
        return Err("A captain's name cannot hold control characters".into());
    }
    Ok(name.to_owned())
}

pub fn validate_area(area: &str) -> Result<String, String> {
    let area = area.trim();
    if area.chars().count() > MAX_AREA_CHARS {
        return Err(format!("The area can be at most {MAX_AREA_CHARS} characters"));
    }
    if has_control(area) {
        return Err("The area cannot hold control characters".into());
    }
    Ok(area.to_owned())
}

pub fn validate_purpose(purpose: &str) -> Result<String, String> {
    let purpose = purpose.trim();
    if purpose.chars().count() > MAX_PURPOSE_CHARS {
        return Err(format!("The purpose can be at most {MAX_PURPOSE_CHARS} characters"));
    }
    if purpose.contains('\0') {
        return Err("The purpose has a character Neko cannot keep".into());
    }
    Ok(purpose.to_owned())
}

pub fn validate_face(face: &FaceRef) -> Result<(), String> {
    if FACE_PRESETS.contains(&face.0.as_str()) { Ok(()) } else { Err(format!("Unknown face \"{}\"", face.0)) }
}

pub fn validate_limit(limit: Option<u64>) -> Result<(), String> {
    match limit {
        Some(n) if !(1..=MAX_DAILY_LIMIT_MICRO_USD).contains(&n) => Err("The daily limit must be between 1 micro-dollar and 1,000,000 dollars".into()),
        _ => Ok(()),
    }
}

pub fn validate_policy(policy: &ModelPolicy) -> Result<(), String> {
    crate::runtime_selection::validate_preferences(&policy.preferences)
}

/// The face a captain of this name wears when none is chosen: a stable hash of the lowercase name.
pub fn default_face(name: &str) -> FaceRef {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in name.trim().to_lowercase().bytes() {
        hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
    }
    FaceRef(FACE_PRESETS[hash as usize % FACE_PRESETS.len()].to_owned())
}

/// Every captain (archived too) in display order: active by `order`, then archived. Rows that do not parse are skipped.
fn load_all(db: &Db) -> Result<Vec<Captain>, String> {
    let mut all: Vec<Captain> = db.captain_rows().map_err(|e| e.to_string())?
        .iter().filter_map(|body| serde_json::from_str(body).ok()).collect();
    all.sort_by(|a, b| a.archived.cmp(&b.archived).then(a.order.cmp(&b.order)).then(a.created_at_ms.cmp(&b.created_at_ms)).then_with(|| a.id.cmp(&b.id)));
    Ok(all)
}

/// Active captains in order, then archived ones when asked.
pub fn list(db: &Db, include_archived: bool) -> Result<Vec<Captain>, String> {
    let mut all = load_all(db)?;
    if !include_archived {
        all.retain(|c| !c.archived);
    }
    Ok(all)
}

pub fn get(db: &Db, id: &str) -> Result<Captain, String> {
    load_all(db)?.into_iter().find(|c| c.id == id).ok_or_else(|| "That captain no longer exists".to_owned())
}

fn save(db: &Db, captain: &Captain) -> Result<(), String> {
    db.put_captain(&captain.id, &serde_json::to_string(captain).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn taken(all: &[Captain], name: &str, except: Option<&str>) -> bool {
    let wanted = name.to_lowercase();
    all.iter().any(|c| !c.archived && Some(c.id.as_str()) != except && c.name.to_lowercase() == wanted)
}

/// Makes a captain and its folder of files under `root` (the data folder), or changes nothing.
pub fn create(db: &Db, root: &Path, workspaces: &[Workspace], input: NewCaptain, id: String, now_ms: i64) -> Result<Captain, String> {
    let name = validate_name(&input.name)?;
    let area = validate_area(&input.area)?;
    let purpose = validate_purpose(&input.purpose)?;
    let face = if input.face.0.is_empty() { default_face(&name) } else { input.face };
    validate_face(&face)?;
    validate_limit(input.daily_limit_micro_usd)?;
    validate_policy(&input.model_policy)?;
    if !workspaces.iter().any(|w| w.id == input.workspace_id) {
        return Err("That workspace no longer exists".into());
    }
    db.atomic(|| {
        let all = load_all(db)?;
        if taken(&all, &name, None) {
            return Err(format!("There is already a captain called {name}"));
        }
        let active: Vec<&Captain> = all.iter().filter(|c| !c.archived).collect();
        if active.len() >= MAX_ACTIVE_CAPTAINS {
            return Err(format!("You can have at most {MAX_ACTIVE_CAPTAINS} captains; archive one first"));
        }
        let captain = Captain {
            id,
            name,
            face,
            workspace_id: input.workspace_id,
            area,
            model_policy: input.model_policy,
            daily_limit_micro_usd: input.daily_limit_micro_usd,
            purpose,
            pinned: false,
            order: active.iter().map(|c| c.order).max().map_or(0, |n| n.saturating_add(1)),
            created_at_ms: now_ms,
            archived: false,
        };
        save(db, &captain)?;
        crate::captain_files::create_all(root, &captain.id, &captain.purpose)?;
        Ok(captain)
    })
}

/// Changes the given fields of an active captain. Its workspace and creation time never change.
pub fn update(db: &Db, id: &str, changes: Changes) -> Result<Captain, String> {
    db.atomic(|| {
        let all = load_all(db)?;
        let mut captain = all.iter().find(|c| c.id == id).cloned().ok_or("That captain no longer exists")?;
        if captain.archived {
            return Err("This captain is archived".into());
        }
        if let Some(name) = changes.name {
            captain.name = validate_name(&name)?;
            if taken(&all, &captain.name, Some(id)) {
                return Err(format!("There is already a captain called {}", captain.name));
            }
        }
        if let Some(face) = changes.face {
            validate_face(&face)?;
            captain.face = face;
        }
        if let Some(area) = changes.area {
            captain.area = validate_area(&area)?;
        }
        if let Some(policy) = changes.model_policy {
            validate_policy(&policy)?;
            captain.model_policy = policy;
        }
        if let Some(limit) = changes.daily_limit_micro_usd {
            validate_limit(limit)?;
            captain.daily_limit_micro_usd = limit;
        }
        if let Some(purpose) = changes.purpose {
            captain.purpose = validate_purpose(&purpose)?;
        }
        save(db, &captain)?;
        Ok(captain)
    })
}

/// Hides the captain and unpins it. Archiving twice is fine. Its row and files stay.
pub fn archive(db: &Db, id: &str) -> Result<(), String> {
    let mut captain = get(db, id)?;
    if !captain.archived {
        captain.archived = true;
        captain.pinned = false;
        save(db, &captain)?;
    }
    Ok(())
}

pub fn pin(db: &Db, id: &str, pinned: bool) -> Result<(), String> {
    let mut captain = get(db, id)?;
    if captain.archived {
        return Err("This captain is archived".into());
    }
    captain.pinned = pinned;
    save(db, &captain)
}

/// Renumbers the active captains 0..n in the given order, which must be exactly the active set.
pub fn reorder(db: &Db, ids: &[String]) -> Result<(), String> {
    db.atomic(|| {
        let mut active: Vec<Captain> = load_all(db)?.into_iter().filter(|c| !c.archived).collect();
        let mut wanted: Vec<&String> = ids.iter().collect();
        wanted.sort();
        let mut have: Vec<&String> = active.iter().map(|c| &c.id).collect();
        have.sort();
        if wanted != have {
            return Err("The list changed; reload and try again".into());
        }
        for (order, id) in ids.iter().enumerate() {
            let captain = active.iter_mut().find(|c| &c.id == id).ok_or("The list changed; reload and try again")?;
            captain.order = i32::try_from(order).unwrap_or(i32::MAX);
            save(db, captain)?;
        }
        Ok(())
    })
}

/// A proposed first captain: the workspace with the most tickets made in the last 30 days (ties: the one
/// touched most recently, then the first listed), the first cat name no active captain has, a face from
/// the name, the workspace's name as the area. Reads only; the same data always gives the same answer.
pub fn suggest(captains: &[Captain], workspaces: &[Workspace], tasks: &[Task], now_ms: i64) -> Result<CaptainSuggestion, String> {
    let busiest = workspaces.iter().enumerate().max_by_key(|(index, workspace)| {
        let mine: Vec<&Task> = tasks.iter().filter(|t| t.workspace_id == workspace.id).collect();
        let recent = mine.iter().filter(|t| t.created_at_ms >= now_ms - 30 * DAY_MS).count();
        (recent, mine.iter().map(|t| t.updated_at_ms).max().unwrap_or(0), std::cmp::Reverse(*index))
    }).map(|(_, workspace)| workspace).ok_or("Add a workspace first")?;
    let used = |name: &str| captains.iter().any(|c| !c.archived && c.name.to_lowercase() == name);
    let name = CAT_NAMES.iter().find(|name| !used(name)).map_or_else(
        || (2..).map(|n| format!("miso {n}")).find(|name| !used(name)).unwrap_or_else(|| "miso".into()),
        |name| (*name).to_owned(),
    );
    let folder = Path::new(&busiest.repository).file_name().and_then(|n| n.to_str()).filter(|n| !n.is_empty()).unwrap_or(&busiest.name);
    Ok(CaptainSuggestion {
        face: default_face(&name),
        name,
        workspace_id: busiest.id.clone(),
        area: busiest.name.chars().take(MAX_AREA_CHARS).collect(),
        purpose: format!("Looks after the work in {folder}: starts sessions, keeps an eye on them and tells you what needs you."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(id: &str, repository: &str) -> Workspace {
        Workspace { id: id.into(), name: format!("{id} app"), repository: repository.into(), instructions: String::new(), away_enabled: false }
    }

    fn input(name: &str) -> NewCaptain {
        NewCaptain { name: name.into(), face: FaceRef("black-glasses".into()), workspace_id: "w1".into(), area: "Web".into(), model_policy: ModelPolicy::default(), daily_limit_micro_usd: None, purpose: "Look after the web app.".into() }
    }

    struct Fx { db: Db, root: tempfile::TempDir, workspaces: Vec<Workspace>, next: std::cell::Cell<u32> }

    impl Fx {
        fn new() -> Self {
            Self { db: Db::open_in_memory().unwrap(), root: tempfile::tempdir().unwrap(), workspaces: vec![workspace("w1", "/tmp/acme-web")], next: Default::default() }
        }
        fn make(&self, name: &str) -> Result<Captain, String> {
            self.make_with(input(name))
        }
        fn make_with(&self, input: NewCaptain) -> Result<Captain, String> {
            self.next.set(self.next.get() + 1);
            create(&self.db, self.root.path(), &self.workspaces, input, format!("id{}", self.next.get()), 1_000 + i64::from(self.next.get()))
        }
        fn names(&self, archived: bool) -> Vec<String> {
            list(&self.db, archived).unwrap().into_iter().map(|c| c.name).collect()
        }
    }

    #[test]
    fn names_of_one_to_forty_characters_are_kept_trimmed_and_others_refused() {
        assert_eq!(validate_name("  miso ").unwrap(), "miso");
        assert!(validate_name("").is_err() && validate_name("   ").is_err());
        assert!(validate_name(&"a".repeat(40)).is_ok());
        assert!(validate_name(&"a".repeat(41)).is_err());
        assert!(validate_name(&"é".repeat(40)).is_ok(), "characters, not bytes");
        assert!(validate_name("a\nb").is_err());
    }

    #[test]
    fn area_and_purpose_have_their_own_limits() {
        assert!(validate_area(&"a".repeat(40)).is_ok() && validate_area(&"a".repeat(41)).is_err());
        assert!(validate_purpose(&"a".repeat(2_000)).is_ok() && validate_purpose(&"a".repeat(2_001)).is_err());
        assert!(validate_purpose("a\0b").is_err());
        assert!(validate_limit(None).is_ok() && validate_limit(Some(1)).is_ok() && validate_limit(Some(MAX_DAILY_LIMIT_MICRO_USD)).is_ok());
        assert!(validate_limit(Some(0)).is_err() && validate_limit(Some(MAX_DAILY_LIMIT_MICRO_USD + 1)).is_err());
    }

    #[test]
    fn create_stores_the_captain_makes_its_files_and_lists_it() {
        let fx = Fx::new();
        let made = fx.make("miso").unwrap();
        assert_eq!((made.name.as_str(), made.order, made.created_at_ms, made.archived, made.pinned), ("miso", 0, 1_001, false, false));
        assert_eq!(list(&fx.db, false).unwrap(), vec![made.clone()]);
        assert_eq!(get(&fx.db, &made.id).unwrap(), made);
        let files = crate::captain_files::read_all(fx.root.path(), &made.id).unwrap();
        assert!(files.files[0].text.contains("Look after the web app."));
        assert_eq!(fx.make("tofu").unwrap().order, 1);
    }

    #[test]
    fn a_name_in_another_case_is_refused_but_an_archived_name_is_free() {
        let fx = Fx::new();
        let miso = fx.make("Miso").unwrap();
        assert_eq!(fx.make("MISO").unwrap_err(), "There is already a captain called MISO");
        archive(&fx.db, &miso.id).unwrap();
        assert!(fx.make("miso").is_ok());
    }

    #[test]
    fn renaming_keeps_names_unique() {
        let fx = Fx::new();
        fx.make("miso").unwrap();
        let tofu = fx.make("tofu").unwrap();
        assert!(update(&fx.db, &tofu.id, Changes { name: Some("MISO".into()), ..Default::default() }).is_err());
        assert_eq!(update(&fx.db, &tofu.id, Changes { name: Some("tofu".into()), ..Default::default() }).unwrap().name, "tofu");
    }

    #[test]
    fn an_unknown_face_is_refused_and_an_empty_one_takes_the_default_for_the_name() {
        let fx = Fx::new();
        let mut bad = input("miso");
        bad.face = FaceRef("rainbow".into());
        assert_eq!(fx.make_with(bad).unwrap_err(), "Unknown face \"rainbow\"");
        assert!(list(&fx.db, true).unwrap().is_empty());
        let mut none = input("miso");
        none.face = FaceRef::default();
        let made = fx.make_with(none).unwrap();
        assert_eq!(made.face, default_face("miso"));
        assert!(FACE_PRESETS.contains(&made.face.0.as_str()));
        let id = made.id;
        assert!(update(&fx.db, &id, Changes { face: Some(FaceRef("nope".into())), ..Default::default() }).is_err());
        assert_eq!(update(&fx.db, &id, Changes { face: Some(FaceRef("grey-scarf".into())), ..Default::default() }).unwrap().face.0, "grey-scarf");
    }

    #[test]
    fn every_preset_face_is_accepted() {
        for (i, face) in FACE_PRESETS.iter().enumerate() {
            let fx = Fx::new();
            let mut one = input(&format!("cat{i}"));
            one.face = FaceRef((*face).into());
            assert!(fx.make_with(one).is_ok(), "{face}");
        }
    }

    #[test]
    fn a_missing_workspace_is_refused_and_writes_nothing() {
        let fx = Fx::new();
        let mut lost = input("miso");
        lost.workspace_id = "gone".into();
        assert_eq!(fx.make_with(lost).unwrap_err(), "That workspace no longer exists");
        assert!(list(&fx.db, true).unwrap().is_empty());
        assert!(!fx.root.path().join("captains").exists());
    }

    #[test]
    fn a_failed_file_write_rolls_the_row_back() {
        let fx = Fx::new();
        std::fs::write(fx.root.path().join("captains"), "a file where the folder should be").unwrap();
        assert!(fx.make("miso").is_err());
        assert!(list(&fx.db, true).unwrap().is_empty());
    }

    #[test]
    fn a_seventeenth_active_captain_is_refused() {
        let fx = Fx::new();
        for i in 0..16 {
            fx.make(&format!("cat{i}")).unwrap();
        }
        assert!(fx.make("one more").unwrap_err().contains("at most 16"));
        let first = list(&fx.db, false).unwrap()[0].id.clone();
        archive(&fx.db, &first).unwrap();
        assert!(fx.make("one more").is_ok(), "an archived captain does not count");
    }

    #[test]
    fn reorder_with_a_missing_or_extra_id_changes_nothing() {
        let fx = Fx::new();
        let (a, b, c) = (fx.make("a").unwrap(), fx.make("b").unwrap(), fx.make("c").unwrap());
        let before = fx.names(false);
        assert!(reorder(&fx.db, &[a.id.clone(), b.id.clone()]).is_err());
        assert!(reorder(&fx.db, &[a.id.clone(), b.id.clone(), c.id.clone(), "x".into()]).is_err());
        assert!(reorder(&fx.db, &[a.id.clone(), a.id.clone(), b.id.clone()]).is_err());
        assert_eq!(fx.names(false), before);
        reorder(&fx.db, &[c.id, a.id, b.id]).unwrap();
        assert_eq!(fx.names(false), ["c", "a", "b"]);
        assert_eq!(list(&fx.db, false).unwrap().iter().map(|c| c.order).collect::<Vec<_>>(), [0, 1, 2]);
    }

    #[test]
    fn pin_then_archive_leaves_it_unpinned_and_hidden() {
        let fx = Fx::new();
        let miso = fx.make("miso").unwrap();
        pin(&fx.db, &miso.id, true).unwrap();
        assert!(get(&fx.db, &miso.id).unwrap().pinned);
        archive(&fx.db, &miso.id).unwrap();
        archive(&fx.db, &miso.id).unwrap();
        let after = get(&fx.db, &miso.id).unwrap();
        assert!(after.archived && !after.pinned);
        assert!(pin(&fx.db, &miso.id, true).is_err());
        assert!(update(&fx.db, &miso.id, Changes::default()).is_err());
    }

    #[test]
    fn update_changes_only_what_is_given_and_never_the_workspace_or_creation_time() {
        let fx = Fx::new();
        let miso = fx.make("miso").unwrap();
        let mut limited = input("tofu");
        limited.daily_limit_micro_usd = Some(5_000_000);
        let tofu = fx.make_with(limited).unwrap();
        let changed = update(&fx.db, &miso.id, Changes { area: Some("Mobile".into()), purpose: Some("New purpose".into()), ..Default::default() }).unwrap();
        assert_eq!((changed.area.as_str(), changed.purpose.as_str(), changed.name.as_str()), ("Mobile", "New purpose", "miso"));
        assert_eq!((changed.workspace_id, changed.created_at_ms, changed.face), (miso.workspace_id, miso.created_at_ms, miso.face));
        assert_eq!(update(&fx.db, &tofu.id, Changes::default()).unwrap().daily_limit_micro_usd, Some(5_000_000));
        assert_eq!(update(&fx.db, &tofu.id, Changes { daily_limit_micro_usd: Some(None), ..Default::default() }).unwrap().daily_limit_micro_usd, None);
        assert!(update(&fx.db, &tofu.id, Changes { daily_limit_micro_usd: Some(Some(0)), ..Default::default() }).is_err());
        assert!(update(&fx.db, "nobody", Changes::default()).is_err());
    }

    #[test]
    fn a_model_policy_goes_through_the_runtime_validator() {
        let fx = Fx::new();
        let mut half = input("miso");
        half.model_policy.preferences.provider = Some("claude".into());
        assert_eq!(fx.make_with(half).unwrap_err(), "Choose a provider and model together");
    }

    #[test]
    fn list_is_in_order_hides_archived_and_shows_them_last_when_asked() {
        let fx = Fx::new();
        let (a, _b, _c) = (fx.make("a").unwrap(), fx.make("b").unwrap(), fx.make("c").unwrap());
        archive(&fx.db, &a.id).unwrap();
        assert_eq!(fx.names(false), ["b", "c"]);
        assert_eq!(fx.names(true), ["b", "c", "a"]);
    }

    #[test]
    fn a_row_that_does_not_parse_is_not_listed() {
        let fx = Fx::new();
        fx.make("miso").unwrap();
        fx.db.put_captain("junk", "not json").unwrap();
        assert_eq!(fx.names(true), ["miso"]);
        assert!(get(&fx.db, "junk").is_err());
    }

    #[test]
    fn the_default_face_is_stable_and_a_preset() {
        assert_eq!(default_face("Miso"), default_face(" miso "));
        for name in CAT_NAMES {
            assert!(FACE_PRESETS.contains(&default_face(name).0.as_str()));
        }
        assert_eq!(default_face("miso"), FaceRef("calico-welding".into()), "pins the hash so faces do not shuffle between releases");
    }

    fn task(workspace: &str, created: i64, updated: i64) -> Task {
        Task {
            id: format!("{workspace}-{created}"), workspace_id: workspace.into(), issue_id: None, title: "ENG-123".into(), goal: String::new(),
            status: neko_protocol::workbench::TaskStatus::Queued, plan: String::new(), result: String::new(), worktree: None, events: Vec::new(),
            created_at_ms: created, updated_at_ms: updated, source_revision: None, supervision: None, base_commit: None, pull_request: None,
            landed_commit: None, display_title: None, parked: None, request: None,
        }
    }

    #[test]
    fn suggest_picks_the_busiest_workspace_a_free_name_and_creates_nothing() {
        let workspaces = vec![workspace("w1", "/tmp/acme-web"), workspace("w2", "/tmp/acme-api")];
        let now = 100 * DAY_MS;
        let tasks = vec![task("w1", now - DAY_MS, now), task("w2", now - 2 * DAY_MS, now), task("w2", now - 3 * DAY_MS, now), task("w1", now - 40 * DAY_MS, now)];
        let one = suggest(&[], &workspaces, &tasks, now).unwrap();
        assert_eq!((one.workspace_id.as_str(), one.name.as_str(), one.area.as_str()), ("w2", "miso", "w2 app"));
        assert!(one.purpose.contains("acme-api") && FACE_PRESETS.contains(&one.face.0.as_str()));
        assert_eq!(one, suggest(&[], &workspaces, &tasks, now).unwrap());
        let taken = Captain { name: "MISO".into(), ..Default::default() };
        assert_eq!(suggest(&[taken], &workspaces, &tasks, now).unwrap().name, "tofu");
    }

    #[test]
    fn suggest_breaks_ties_by_recent_touch_then_order_and_needs_a_workspace() {
        let workspaces = vec![workspace("w1", "/tmp/a"), workspace("w2", "/tmp/b")];
        let now = 100 * DAY_MS;
        assert_eq!(suggest(&[], &workspaces, &[], now).unwrap().workspace_id, "w1");
        let tasks = vec![task("w1", now - DAY_MS, now - 5), task("w2", now - DAY_MS, now)];
        assert_eq!(suggest(&[], &workspaces, &tasks, now).unwrap().workspace_id, "w2");
        assert_eq!(suggest(&[], &[], &[], now).unwrap_err(), "Add a workspace first");
    }

    #[test]
    fn a_database_from_before_captains_gains_the_table_and_keeps_its_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("neko.db");
        {
            let db = Db::open(&path).unwrap();
            db.set_setting("some_setting", "kept").unwrap();
        }
        // Make it look like a database written before this change: no captains table.
        rusqlite::Connection::open(&path).unwrap().execute_batch("DROP TABLE captains;").unwrap();
        let db = Db::open(&path).unwrap();
        assert_eq!(db.get_setting("some_setting").unwrap().as_deref(), Some("kept"));
        assert!(list(&db, true).unwrap().is_empty());
        let root = tempfile::tempdir().unwrap();
        let made = create(&db, root.path(), &[workspace("w1", "/tmp/acme-web")], input("miso"), "c1".into(), 1).unwrap();
        drop(db);
        assert_eq!(list(&Db::open(&path).unwrap(), false).unwrap(), vec![made], "and it survives a reopen");
    }

    #[test]
    fn a_fresh_database_has_the_table_and_opening_twice_is_harmless() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fresh").join("neko.db");
        let first = Db::open(&path).unwrap();
        assert!(list(&first, true).unwrap().is_empty());
        let root = tempfile::tempdir().unwrap();
        create(&first, root.path(), &[workspace("w1", "/tmp/acme-web")], input("miso"), "c1".into(), 1).unwrap();
        let second = Db::open(&path).unwrap();
        assert_eq!(list(&second, false).unwrap().len(), 1);
    }
}
