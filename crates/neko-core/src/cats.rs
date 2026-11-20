//! Cat records: storage (the `cats` table, one JSON row each) and the
//! rules every change obeys. A cat's files are `cat_files`; the
//! requests are answered by `neko-daemon/src/cats.rs`. Nothing here reads
//! the clock, makes ids or finds the data folder: callers pass them in, so a
//! test never touches shared state. See `docs/knowledge/cats.md`.
use crate::db::Db;
use neko_protocol::cats::{
    Cat, CatSuggestion, FaceRef, ModelPolicy, FACE_PRESETS, MAX_ACTIVE_CATS, MAX_AREA_CHARS,
    MAX_DAILY_LIMIT_MICRO_USD, MAX_NAME_CHARS, MAX_PURPOSE_CHARS,
};
use neko_protocol::workbench::{Task, Workspace};
use std::path::Path;

/// Names `Suggest` draws from, in this order.
pub const CAT_NAMES: [&str; 12] = ["miso", "tofu", "nori", "pixel", "biscuit", "mochi", "kiki", "pepper", "olive", "juno", "waffles", "sesame"];

const DAY_MS: i64 = 86_400_000;

/// What `Create` is given.
pub struct NewCat {
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
        return Err("Give the cat a name".into());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("A cat's name can be at most {MAX_NAME_CHARS} characters"));
    }
    if has_control(name) {
        return Err("A cat's name cannot hold control characters".into());
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

/// The face a cat of this name wears when none is chosen: a stable hash of the lowercase name.
pub fn default_face(name: &str) -> FaceRef {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in name.trim().to_lowercase().bytes() {
        hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
    }
    FaceRef(FACE_PRESETS[hash as usize % FACE_PRESETS.len()].to_owned())
}

/// Every cat (archived too) in display order: active by `order`, then archived. Rows that do not parse are skipped.
fn load_all(db: &Db) -> Result<Vec<Cat>, String> {
    let mut all: Vec<Cat> = db.cat_rows().map_err(|e| e.to_string())?
        .iter().filter_map(|body| serde_json::from_str(body).ok()).collect();
    all.sort_by(|a, b| a.archived.cmp(&b.archived).then(a.order.cmp(&b.order)).then(a.created_at_ms.cmp(&b.created_at_ms)).then_with(|| a.id.cmp(&b.id)));
    Ok(all)
}

/// Active cats in order, then archived ones when asked.
pub fn list(db: &Db, include_archived: bool) -> Result<Vec<Cat>, String> {
    let mut all = load_all(db)?;
    if !include_archived {
        all.retain(|c| !c.archived);
    }
    Ok(all)
}

pub fn get(db: &Db, id: &str) -> Result<Cat, String> {
    load_all(db)?.into_iter().find(|c| c.id == id).ok_or_else(|| "That cat no longer exists".to_owned())
}

fn save(db: &Db, cat: &Cat) -> Result<(), String> {
    db.put_cat(&cat.id, &serde_json::to_string(cat).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn taken(all: &[Cat], name: &str, except: Option<&str>) -> bool {
    let wanted = name.to_lowercase();
    all.iter().any(|c| !c.archived && Some(c.id.as_str()) != except && c.name.to_lowercase() == wanted)
}

/// Makes a cat and its folder of files under `root` (the data folder), or changes nothing.
pub fn create(db: &Db, root: &Path, workspaces: &[Workspace], input: NewCat, id: String, now_ms: i64) -> Result<Cat, String> {
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
            return Err(format!("There is already a cat called {name}"));
        }
        let active: Vec<&Cat> = all.iter().filter(|c| !c.archived).collect();
        if active.len() >= MAX_ACTIVE_CATS {
            return Err(format!("You can have at most {MAX_ACTIVE_CATS} cats; archive one first"));
        }
        let cat = Cat {
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
        save(db, &cat)?;
        crate::cat_files::create_all(root, &cat.id, &cat.purpose)?;
        Ok(cat)
    })
}

/// Changes the given fields of an active cat. Its workspace and creation time never change.
pub fn update(db: &Db, id: &str, changes: Changes) -> Result<Cat, String> {
    db.atomic(|| {
        let all = load_all(db)?;
        let mut cat = all.iter().find(|c| c.id == id).cloned().ok_or("That cat no longer exists")?;
        if cat.archived {
            return Err("This cat is archived".into());
        }
        if let Some(name) = changes.name {
            cat.name = validate_name(&name)?;
            if taken(&all, &cat.name, Some(id)) {
                return Err(format!("There is already a cat called {}", cat.name));
            }
        }
        if let Some(face) = changes.face {
            validate_face(&face)?;
            cat.face = face;
        }
        if let Some(area) = changes.area {
            cat.area = validate_area(&area)?;
        }
        if let Some(policy) = changes.model_policy {
            validate_policy(&policy)?;
            cat.model_policy = policy;
        }
        if let Some(limit) = changes.daily_limit_micro_usd {
            validate_limit(limit)?;
            cat.daily_limit_micro_usd = limit;
        }
        if let Some(purpose) = changes.purpose {
            cat.purpose = validate_purpose(&purpose)?;
        }
        save(db, &cat)?;
        Ok(cat)
    })
}

/// Hides the cat and unpins it. Archiving twice is fine. Its row and files stay.
pub fn archive(db: &Db, id: &str) -> Result<(), String> {
    let mut cat = get(db, id)?;
    if !cat.archived {
        cat.archived = true;
        cat.pinned = false;
        save(db, &cat)?;
    }
    Ok(())
}

pub fn pin(db: &Db, id: &str, pinned: bool) -> Result<(), String> {
    let mut cat = get(db, id)?;
    if cat.archived {
        return Err("This cat is archived".into());
    }
    cat.pinned = pinned;
    save(db, &cat)
}

/// Renumbers the active cats 0..n in the given order, which must be exactly the active set.
pub fn reorder(db: &Db, ids: &[String]) -> Result<(), String> {
    db.atomic(|| {
        let mut active: Vec<Cat> = load_all(db)?.into_iter().filter(|c| !c.archived).collect();
        let mut wanted: Vec<&String> = ids.iter().collect();
        wanted.sort();
        let mut have: Vec<&String> = active.iter().map(|c| &c.id).collect();
        have.sort();
        if wanted != have {
            return Err("The list changed; reload and try again".into());
        }
        for (order, id) in ids.iter().enumerate() {
            let cat = active.iter_mut().find(|c| &c.id == id).ok_or("The list changed; reload and try again")?;
            cat.order = i32::try_from(order).unwrap_or(i32::MAX);
            save(db, cat)?;
        }
        Ok(())
    })
}

/// A proposed first cat: the workspace with the most tickets made in the last 30 days (ties: the one
/// touched most recently, then the first listed), the first cat name no active cat has, a face from
/// the name, the workspace's name as the area. Reads only; the same data always gives the same answer.
pub fn suggest(cats: &[Cat], workspaces: &[Workspace], tasks: &[Task], now_ms: i64) -> Result<CatSuggestion, String> {
    let busiest = workspaces.iter().enumerate().max_by_key(|(index, workspace)| {
        let mine: Vec<&Task> = tasks.iter().filter(|t| t.workspace_id == workspace.id).collect();
        let recent = mine.iter().filter(|t| t.created_at_ms >= now_ms - 30 * DAY_MS).count();
        (recent, mine.iter().map(|t| t.updated_at_ms).max().unwrap_or(0), std::cmp::Reverse(*index))
    }).map(|(_, workspace)| workspace).ok_or("Add a workspace first")?;
    let used = |name: &str| cats.iter().any(|c| !c.archived && c.name.to_lowercase() == name);
    let name = CAT_NAMES.iter().find(|name| !used(name)).map_or_else(
        || (2..).map(|n| format!("miso {n}")).find(|name| !used(name)).unwrap_or_else(|| "miso".into()),
        |name| (*name).to_owned(),
    );
    let folder = Path::new(&busiest.repository).file_name().and_then(|n| n.to_str()).filter(|n| !n.is_empty()).unwrap_or(&busiest.name);
    Ok(CatSuggestion {
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

    fn input(name: &str) -> NewCat {
        NewCat { name: name.into(), face: FaceRef("black-glasses".into()), workspace_id: "w1".into(), area: "Web".into(), model_policy: ModelPolicy::default(), daily_limit_micro_usd: None, purpose: "Look after the web app.".into() }
    }

    struct Fx { db: Db, root: tempfile::TempDir, workspaces: Vec<Workspace>, next: std::cell::Cell<u32> }

    impl Fx {
        fn new() -> Self {
            Self { db: Db::open_in_memory().unwrap(), root: tempfile::tempdir().unwrap(), workspaces: vec![workspace("w1", "/tmp/acme-web")], next: Default::default() }
        }
        fn make(&self, name: &str) -> Result<Cat, String> {
            self.make_with(input(name))
        }
        fn make_with(&self, input: NewCat) -> Result<Cat, String> {
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
    fn create_stores_the_cat_makes_its_files_and_lists_it() {
        let fx = Fx::new();
        let made = fx.make("miso").unwrap();
        assert_eq!((made.name.as_str(), made.order, made.created_at_ms, made.archived, made.pinned), ("miso", 0, 1_001, false, false));
        assert_eq!(list(&fx.db, false).unwrap(), vec![made.clone()]);
        assert_eq!(get(&fx.db, &made.id).unwrap(), made);
        let files = crate::cat_files::read_all(fx.root.path(), &made.id).unwrap();
        assert!(files.files[0].text.contains("Look after the web app."));
        assert_eq!(fx.make("tofu").unwrap().order, 1);
    }

    #[test]
    fn a_name_in_another_case_is_refused_but_an_archived_name_is_free() {
        let fx = Fx::new();
        let miso = fx.make("Miso").unwrap();
        assert_eq!(fx.make("MISO").unwrap_err(), "There is already a cat called MISO");
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
        assert!(!fx.root.path().join("cats").exists());
    }

    #[test]
    fn a_failed_file_write_rolls_the_row_back() {
        let fx = Fx::new();
        std::fs::write(fx.root.path().join("cats"), "a file where the folder should be").unwrap();
        assert!(fx.make("miso").is_err());
        assert!(list(&fx.db, true).unwrap().is_empty());
    }

    #[test]
    fn a_seventeenth_active_cat_is_refused() {
        let fx = Fx::new();
        for i in 0..16 {
            fx.make(&format!("cat{i}")).unwrap();
        }
        assert!(fx.make("one more").unwrap_err().contains("at most 16"));
        let first = list(&fx.db, false).unwrap()[0].id.clone();
        archive(&fx.db, &first).unwrap();
        assert!(fx.make("one more").is_ok(), "an archived cat does not count");
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
        fx.db.put_cat("junk", "not json").unwrap();
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
            cat_id: None,
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
        let taken = Cat { name: "MISO".into(), ..Default::default() };
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
    fn a_database_from_before_cats_gains_the_table_and_keeps_its_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("neko.db");
        {
            let db = Db::open(&path).unwrap();
            db.set_setting("some_setting", "kept").unwrap();
        }
        // Make it look like a database written before this change: no cats table.
        rusqlite::Connection::open(&path).unwrap().execute_batch("DROP TABLE cats;").unwrap();
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

    /// The tables as batches B1 and B2 made them, filled the way a daemon of that time would have.
    const LEGACY_B1: &str = "CREATE TABLE captains (id TEXT PRIMARY KEY, body TEXT NOT NULL);";
    const LEGACY_B2: &str = "
        CREATE TABLE captain_messages (captain_id TEXT NOT NULL, seq INTEGER NOT NULL, role TEXT NOT NULL CHECK (role IN ('person','captain','event')),
            text TEXT NOT NULL, at_ms INTEGER NOT NULL, PRIMARY KEY (captain_id, seq)) WITHOUT ROWID;
        CREATE TABLE captain_wakes (id INTEGER PRIMARY KEY AUTOINCREMENT, captain_id TEXT NOT NULL, reason TEXT NOT NULL, task_id TEXT NOT NULL DEFAULT '', at_ms INTEGER NOT NULL);
        CREATE INDEX idx_captain_wakes_captain ON captain_wakes (captain_id, id);";

    /// Turns a freshly opened database file into one as an older build left it: the new tables gone, the old ones in.
    fn age(path: &Path, with_b2: bool) {
        let raw = rusqlite::Connection::open(path).unwrap();
        raw.execute_batch("DROP TABLE cat_messages; DROP TABLE cat_wakes; DROP TABLE cats;").unwrap();
        raw.execute_batch(LEGACY_B1).unwrap();
        if with_b2 {
            raw.execute_batch(LEGACY_B2).unwrap();
        }
    }

    fn tables(path: &Path) -> Vec<String> {
        let raw = rusqlite::Connection::open(path).unwrap();
        let mut statement = raw.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'cat%' ORDER BY name").unwrap();
        statement.query_map([], |row| row.get(0)).unwrap().map(Result::unwrap).collect()
    }

    #[test]
    fn a_database_with_b1_and_b2_tables_and_data_moves_every_row_to_the_cat_tables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("neko.db");
        let body = serde_json::to_string(&Cat { id: "c1".into(), name: "miso".into(), workspace_id: "w1".into(), ..Default::default() }).unwrap();
        {
            drop(Db::open(&path).unwrap());
            age(&path, true);
            let raw = rusqlite::Connection::open(&path).unwrap();
            raw.execute("INSERT INTO captains (id, body) VALUES ('c1', ?1)", [&body]).unwrap();
            raw.execute_batch(
                "INSERT INTO captain_messages VALUES ('c1', 1, 'person', 'Split it.', 10), ('c1', 2, 'captain', 'Opened t1.', 20), ('c1', 3, 'event', 'A turn failed.', 30);
                 INSERT INTO captain_wakes (captain_id, reason, task_id, at_ms) VALUES ('c1', 'failed', 't1', 40), ('c1', 'needs_you', 't2', 50), ('c2', 'turn_finished', 't3', 60);
                 DELETE FROM captain_wakes WHERE reason = 'needs_you';
                 INSERT INTO settings (key, value) VALUES ('neko_captains_rev_v1', '7'), ('neko_captain_seen_v1:c1', '{\"tasks\":{}}'), ('neko_captain_seen_v1:c2', '{}'), ('neko_crew_origin_v1', '{\"t1\":\"captain:c1\",\"t9\":\"chat\",\"t8\":\"ticket:x\"}');
                 INSERT INTO usage_events (run_key, at_ms, provider, source, task_id, kind, cost_micro_usd) VALUES
                    ('captain:c1:5', 100, 'claude', 'reported', 'captain:c1', 'captain', 2500000),
                    ('ticket:t1:1', 90, 'claude', 'reported', 't1', 'ticket', 1000000);",
            ).unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(tables(&path), ["cat_messages", "cat_wakes", "cats"], "the old tables are gone");
        let cat = get(&db, "c1").unwrap();
        assert_eq!((cat.name.as_str(), cat.workspace_id.as_str()), ("miso", "w1"));
        let thread = db.cat_messages_last("c1", 10).unwrap();
        assert_eq!(thread.iter().map(|m| (m.0, m.1.as_str(), m.2.as_str(), m.3)).collect::<Vec<_>>(), [(1, "person", "Split it.", 10), (2, "cat", "Opened t1.", 20), (3, "event", "A turn failed.", 30)]);
        assert_eq!(crate::cat_thread::view(&db, "c1", 0, false).unwrap().messages[1].role, neko_protocol::cats::CatMessageRole::Cat, "the stored role reads as a cat's line");
        assert_eq!(db.cat_wakes_of("c1").unwrap().iter().map(|w| (w.0, w.1.as_str(), w.2.as_str())).collect::<Vec<_>>(), [(1, "failed", "t1")]);
        assert_eq!(db.cats_with_wakes().unwrap(), ["c1", "c2"]);
        assert_eq!(db.push_cat_wake("c1", "failed", "t4", 70).unwrap(), 4, "ids keep counting where the old table stopped");
        assert_eq!((db.get_setting("neko_cats_rev_v1").unwrap().as_deref(), db.get_setting("neko_captains_rev_v1").unwrap()), (Some("7"), None));
        assert_eq!((db.get_setting("neko_cat_seen_v1:c1").unwrap().as_deref(), db.get_setting("neko_cat_seen_v1:c2").unwrap().as_deref(), db.get_setting("neko_captain_seen_v1:c1").unwrap()), (Some("{\"tasks\":{}}"), Some("{}"), None));
        assert_eq!(crate::crew::origin_of(&db, "t1").unwrap().as_deref(), Some("cat:c1"));
        assert_eq!((crate::crew::origin_of(&db, "t9").unwrap().as_deref(), crate::crew::origin_of(&db, "t8").unwrap().as_deref()), (Some("chat"), Some("ticket:x")));
        let lines = db.usage_events_for_task("cat:c1").unwrap();
        assert_eq!((lines.len(), lines[0].run_key.as_str(), lines[0].kind.as_str(), lines[0].cost_micro_usd), (1, "cat:c1:5", "cat", Some(2_500_000)));
        assert!(db.usage_events_for_task("captain:c1").unwrap().is_empty());
        assert_eq!(db.usage_events_for_task("t1").unwrap()[0].kind, "ticket", "other ledger lines are untouched");
        assert_eq!(crate::spend::cats_usd_since(&db, 0), 2.5);
        drop(db);
        // Opening again changes nothing.
        let again = Db::open(&path).unwrap();
        assert_eq!((list(&again, true).unwrap().len(), again.cat_messages_last("c1", 10).unwrap().len(), again.cat_wakes_of("c1").unwrap().len()), (1, 3, 2));
        assert_eq!(tables(&path), ["cat_messages", "cat_wakes", "cats"]);
    }

    #[test]
    fn a_database_with_only_the_b1_table_keeps_its_cats_and_gains_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("neko.db");
        let body = serde_json::to_string(&Cat { id: "c1".into(), name: "miso".into(), ..Default::default() }).unwrap();
        drop(Db::open(&path).unwrap());
        age(&path, false);
        rusqlite::Connection::open(&path).unwrap().execute("INSERT INTO captains (id, body) VALUES ('c1', ?1)", [&body]).unwrap();
        let db = Db::open(&path).unwrap();
        assert_eq!(tables(&path), ["cat_messages", "cat_wakes", "cats"]);
        assert_eq!(list(&db, true).unwrap().iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["miso"]);
        assert!(db.cat_messages_last("c1", 10).unwrap().is_empty());
        assert_eq!(db.append_cat_message("c1", "cat", "hello", 1).unwrap(), 1);
    }

    #[test]
    fn a_database_from_before_b1_has_no_old_tables_to_move_and_gets_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("neko.db");
        drop(Db::open(&path).unwrap());
        rusqlite::Connection::open(&path).unwrap().execute_batch("DROP TABLE cat_messages; DROP TABLE cat_wakes; DROP TABLE cats;").unwrap();
        assert!(tables(&path).is_empty());
        let db = Db::open(&path).unwrap();
        assert_eq!(tables(&path), ["cat_messages", "cat_wakes", "cats"]);
        assert!(list(&db, true).unwrap().is_empty());
    }

    #[test]
    fn a_fresh_database_gets_only_the_cat_tables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("neko.db");
        drop(Db::open(&path).unwrap());
        assert_eq!(tables(&path), ["cat_messages", "cat_wakes", "cats"]);
        let memory = Db::open_in_memory().unwrap();
        assert!(list(&memory, true).unwrap().is_empty());
    }

    #[test]
    fn a_task_stored_with_the_old_captain_id_field_reads_as_a_cats_task() {
        let task = Task { cat_id: Some("c1".into()), ..task("w1", 1, 1) };
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("\"cat_id\":\"c1\""));
        let old: Task = serde_json::from_str(&json.replace("\"cat_id\"", "\"captain_id\"")).unwrap();
        assert_eq!(old.cat_id.as_deref(), Some("c1"));
        assert_eq!(old, task);
    }
}
