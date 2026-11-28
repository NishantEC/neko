//! Cats in the store (docs/knowledge/cats.md): the starters, the name rules, the limits, that a thread has no cat (version 14), every
//! cat a Cattain with its own thread and folder, its files and their previous version, what a run is handed, retiring and bringing
//! back, a run's version of its words and the line when they changed, and the migrations from versions 5 and 16.

use neko_core::conductor::{RunBook, Who};
use neko_core::runtime::Outcome;
use neko_core::store::SCHEMA_VERSION;
use neko_core::{CatError, Core, FixedClock, NewTaskError, Place, SequentialIds, Store};
use neko_protocol::cats::{MAX_ACTIVE, MAX_RULES, STARTERS, rules_hash};
use neko_protocol::{PartBody, PermissionMode, Pricing, TaskId};

struct Gone(std::path::PathBuf);
impl Drop for Gone {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir() -> Gone {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("neko-cats-test-{}-{n}-{nanos}", std::process::id()));
    std::fs::create_dir_all(d.join("w")).unwrap();
    Gone(d)
}

fn open(d: &Gone) -> Core {
    Core::open(&d.0.join("neko.sqlite3"), Box::new(FixedClock::new(1_764_000_000_000, 1000)), Box::new(SequentialIds::default())).unwrap()
}

fn names(c: &Core) -> Vec<String> {
    c.cats().unwrap().into_iter().filter(|c| c.is_active()).map(|c| c.name).collect()
}

fn cat_of(c: &Core, t: &TaskId) -> String {
    c.store.get_task(t).unwrap().unwrap().cat.unwrap_or_default()
}

#[test]
fn a_new_install_has_three_starter_cats_made_once() {
    let d = dir();
    let c = open(&d);
    assert_eq!(names(&c), ["miso", "mochi", "tofu"]);
    let cats = c.cats().unwrap();
    assert!(cats.iter().all(|c| c.purpose.is_none() && c.workspace.is_none() && c.model.is_none() && c.access.is_none()), "starters have nothing set");
    assert_eq!(cats.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), STARTERS.iter().map(|s| s.0).collect::<Vec<_>>());
    drop(c);
    let c = open(&d);
    assert_eq!(c.cats().unwrap().len(), 3, "opening again makes none");
}

#[test]
fn names_are_one_to_forty_characters_and_unique_without_case() {
    let d = dir();
    let mut c = open(&d);
    assert!(matches!(c.create_cat("  ", None, None, None), Err(CatError::InvalidName(_))));
    assert!(matches!(c.create_cat(&"a".repeat(41), None, None, None), Err(CatError::InvalidName(_))));
    assert!(matches!(c.create_cat("MISO", None, None, None), Err(CatError::InvalidName(w)) if w.contains("already a cat called miso")));
    let (pip, _) = c.create_cat("  pip \n squeak ", None, Some("Reads the logs."), None).unwrap();
    assert_eq!((pip.name.as_str(), pip.purpose.as_deref()), ("pip squeak", Some("Reads the logs.")));
    assert_eq!(pip.face, "tabby", "the first face no active cat has");
    assert!(matches!(c.rename_cat(&pip.id, "Tofu"), Err(CatError::InvalidName(_))));
    assert_eq!(c.rename_cat(&pip.id, "Pip").unwrap().0.name, "Pip", "its own name in another case is fine");
    assert_eq!(c.rename_cat("cat_miso", "Miso").unwrap().0.name, "Miso");
    // A retired cat keeps its name.
    c.retire_cat(&pip.id).unwrap();
    assert!(matches!(c.create_cat("pip", None, None, None), Err(CatError::InvalidName(w)) if w.contains("retired")));
    assert!(matches!(c.set_cat_face("cat_miso", "dragon"), Err(CatError::Refused(_))));
    assert_eq!(c.set_cat_face("cat_miso", "void").unwrap().0.face, "void");
    assert!(c.set_cat_purpose("cat_miso", &"x".repeat(201)).is_err());
    assert_eq!(c.set_cat_purpose("cat_miso", "  ").unwrap().0.purpose, None, "an empty purpose clears it");
    assert_eq!(c.rename_cat("cat_nobody", "x").unwrap_err(), CatError::NotFound);
}

#[test]
fn at_most_sixteen_are_active_and_at_least_one_stays() {
    let d = dir();
    let mut c = open(&d);
    for i in 0..MAX_ACTIVE - 3 {
        c.create_cat(&format!("cat {i}"), None, None, None).unwrap();
    }
    assert!(matches!(c.create_cat("one too many", None, None, None), Err(CatError::Refused(w)) if w.contains("16")));
    c.retire_cat("cat_tofu").unwrap();
    let (extra, _) = c.create_cat("one more", None, None, None).unwrap();
    assert!(matches!(c.bring_back_cat("cat_tofu"), Err(CatError::Refused(_))), "bringing back counts too");
    c.retire_cat(&extra.id).unwrap();
    assert!(c.bring_back_cat("cat_tofu").unwrap().0.is_active());
    // Retire all but one: the last stays.
    let active: Vec<String> = c.cats().unwrap().into_iter().filter(|x| x.is_active()).map(|x| x.id).collect();
    for id in &active[1..] {
        c.retire_cat(id).unwrap();
    }
    assert!(matches!(c.retire_cat(&active[0]), Err(CatError::Refused(w)) if w.contains("only cat")));
    assert_eq!(names(&c).len(), 1);
}

#[test]
fn a_thread_has_no_cat_and_nothing_gives_it_one() {
    let d = dir();
    let mut c = open(&d);
    std::fs::create_dir_all(d.0.join("acme-api")).unwrap();
    let ws = c.add_workspace(&d.0.join("acme-api").to_string_lossy()).unwrap().workspace;
    // Several in a row, in either project: none has a cat (the starters stay idle, as they were not asked for).
    for i in 0..4 {
        assert_eq!(c.create_task(&format!("Task {i}")).unwrap().0.cat, None);
    }
    // A cat's default project no longer assigns anything.
    c.set_cat_workspace("cat_miso", Some(&ws.id)).unwrap();
    assert_eq!(c.create_task_in("In the project", Some(&ws.id)).unwrap().0.cat, None);
    // A cat can still be given on purpose by the code that owns it (a round): it must be there and active.
    assert_eq!(c.create_task_for("Given", None, Some("cat_tofu")).unwrap().0.cat.as_deref(), Some("cat_tofu"));
    c.retire_cat("cat_miso").unwrap();
    assert!(matches!(c.create_task_for("I", None, Some("cat_miso")), Err(NewTaskError::Cat(CatError::Refused(w))) if w.contains("retired")));
    assert!(matches!(c.create_task_for("J", None, Some("cat_nobody")), Err(NewTaskError::Cat(CatError::NotFound))));
}

#[test]
fn every_cat_is_a_cattain_with_its_folder_and_its_own_thread_once_opened() {
    let d = dir();
    let mut c = open(&d);
    assert!(c.store.all_tasks().unwrap().is_empty(), "no thread until a Cattain is opened");
    for cat in c.cats().unwrap() {
        let folder = d.0.join("cattains").join(&cat.id);
        for f in neko_protocol::cats::FILES {
            assert_eq!(std::fs::read_to_string(folder.join(f)).unwrap(), "", "{f} is there, empty");
        }
        assert!(folder.join("reports").is_dir());
        let (t, with, made) = c.cattain_thread(&cat.id).unwrap();
        assert!(made.is_some() && with.thread.as_ref() == Some(&t.id));
        assert!(t.cattain && !t.listed(), "its own thread, not listed under Threads");
        assert_eq!((t.title.as_str(), t.cat.as_deref(), t.status), (cat.name.as_str(), Some(cat.id.as_str()), neko_protocol::TaskStatus::New), "named for it, read, and not started");
        assert!(c.store.runs_of(&t.id).unwrap().is_empty(), "it starts no run by itself");
        assert_eq!(c.store.place(&t.id).unwrap().unwrap().workdir, folder.to_string_lossy(), "it runs in its folder");
        let (again, _, made) = c.cattain_thread(&cat.id).unwrap();
        assert_eq!((again.id, made), (t.id, None), "made once");
    }
    let (pip, _) = c.create_cat("pip", None, None, None).unwrap();
    assert!(d.0.join("cattains").join(&pip.id).join("RULES.md").exists(), "a new cat has its folder at once");
    let (t, _, _) = c.cattain_thread(&pip.id).unwrap();
    drop(c);
    let mut c = open(&d);
    assert_eq!(c.store.get_cat(&pip.id).unwrap().unwrap().thread, Some(t.id.clone()), "kept across a restart");
    assert_eq!(c.cattain_thread(&pip.id).unwrap().0.id, t.id);
    assert_eq!(c.store.all_tasks().unwrap().iter().filter(|t| t.cattain).count(), 4);
}

#[test]
fn rules_and_crew_are_saved_verbatim_with_one_previous_version_to_restore() {
    let d = dir();
    let mut c = open(&d);
    assert!(matches!(c.restore_cattain_file("cat_miso", "RULES.md"), Err(CatError::Refused(_))), "nothing to restore yet");
    let first = "# House rules\n\n- Use British spelling.  \n- **Never** push.\n\n";
    let (f, _) = c.save_cattain_file("cat_miso", "RULES.md", first).unwrap();
    assert_eq!((f.text.as_str(), f.previous, f.editable), (first, true, true));
    assert_eq!(std::fs::read_to_string(d.0.join("cattains/cat_miso/RULES.md")).unwrap(), first, "on disk, exactly");
    c.save_cattain_file("cat_miso", "RULES.md", "Second").unwrap();
    let (f, _) = c.restore_cattain_file("cat_miso", "RULES.md").unwrap();
    assert_eq!(f.text, first, "restore puts the one before back");
    let (f, _) = c.restore_cattain_file("cat_miso", "RULES.md").unwrap();
    assert_eq!(f.text, "Second", "and restoring again goes back");
    assert!(matches!(c.save_cattain_file("cat_miso", "CREW.md", &"x".repeat(MAX_RULES + 1)), Err(CatError::Refused(_))));
    assert!(c.save_cattain_file("cat_miso", "CREW.md", &"x".repeat(MAX_RULES)).is_ok(), "16 KB fits");
    // NOTES.md and DIGEST.md are the Cattain's to write; nothing else is a file of its.
    assert!(matches!(c.save_cattain_file("cat_miso", "DIGEST.md", "x"), Err(CatError::Refused(w)) if w.contains("miso's to write")));
    assert!(matches!(c.cattain_file("cat_miso", "../token"), Err(CatError::Refused(_))));
    drop(c);
    let mut c = open(&d);
    let (f, _) = c.cattain_file("cat_miso", "RULES.md").unwrap();
    assert_eq!((f.text.as_str(), f.previous), ("Second", true), "kept across a restart");
}

#[test]
fn what_the_cattain_writes_on_disk_is_taken_in_with_the_version_before_it() {
    let d = dir();
    let mut c = open(&d);
    let folder = d.0.join("cattains/cat_miso");
    std::fs::write(folder.join("DIGEST.md"), "## Needs you\n\n- Merge acme#12?\n").unwrap();
    let (f, changed) = c.cattain_file("cat_miso", "DIGEST.md").unwrap();
    assert_eq!((f.text.as_str(), f.previous, f.editable), ("## Needs you\n\n- Merge acme#12?\n", true, false));
    assert_eq!(changed.iter().map(|(f, _)| f.name.as_str()).collect::<Vec<_>>(), ["DIGEST.md"], "said once");
    assert!(c.sync_cattain("cat_miso").unwrap().is_empty(), "nothing new the second time");
    // The person's save after the Cattain's own edit keeps the Cattain's as the previous version.
    std::fs::write(folder.join("CREW.md"), "From miso").unwrap();
    c.save_cattain_file("cat_miso", "CREW.md", "From the person").unwrap();
    assert_eq!(c.restore_cattain_file("cat_miso", "CREW.md").unwrap().0.text, "From miso");
    // A file that went missing comes back from what Neko last saw.
    std::fs::remove_file(folder.join("RULES.md")).unwrap();
    c.save_cattain_file("cat_miso", "RULES.md", "Kept").unwrap();
    std::fs::remove_file(folder.join("RULES.md")).unwrap();
    assert!(c.sync_cattain("cat_miso").unwrap().is_empty());
    assert_eq!(std::fs::read_to_string(folder.join("RULES.md")).unwrap(), "Kept");
}

#[test]
fn its_own_thread_is_handed_rules_md_and_its_crew_crew_md() {
    let d = dir();
    let mut c = open(&d);
    std::fs::create_dir_all(d.0.join("acme-api")).unwrap();
    let ws = c.add_workspace(&d.0.join("acme-api").to_string_lossy()).unwrap().workspace;
    c.set_cat_workspace("cat_miso", Some(&ws.id)).unwrap();
    c.save_cattain_file("cat_miso", "RULES.md", "RULE-1: plan first.").unwrap();
    c.save_cattain_file("cat_miso", "CREW.md", "CREW-1: open a PR, never merge.").unwrap();
    let (own, _, _) = c.cattain_thread("cat_miso").unwrap();
    let (h, _) = c.handed(&own).unwrap();
    let h = h.unwrap();
    assert!(h.own && h.text == "RULE-1: plan first." && h.file() == "RULES.md");
    let block = c.block_for(&h);
    assert!(block.contains("RULE-1: plan first.") && !block.contains("CREW-1") && block.contains(&d.0.join("cattains/cat_miso").display().to_string()));
    assert!(block.contains("Your project is acme-api"));
    assert_eq!(c.read_too(&h), [std::path::PathBuf::from(ws.folder.clone().unwrap())], "it may read its project");
    let (crew, _) = c.create_task_for("Fix the login", Some(&ws.id), Some("cat_miso")).unwrap();
    assert_eq!(crew.crew_of(), Some("cat_miso"));
    let (h, _) = c.handed(&crew).unwrap();
    let h = h.unwrap();
    assert!(!h.own && h.text == "CREW-1: open a PR, never merge.");
    let block = c.block_for(&h);
    assert!(block.starts_with("<neko-crew>") && block.contains("CREW-1: open a PR, never merge.") && !block.contains("RULE-1"));
    assert!(c.read_too(&h).is_empty(), "a crew thread reads only its own folder");
    let (plain, _) = c.create_task("No cat").unwrap();
    assert_eq!(c.handed(&plain).unwrap().0, None);
    // Model and access are the Profile's, and only names, never flags.
    assert_eq!(c.set_cat_model("cat_miso", Some("sonnet")).unwrap().0.model.as_deref(), Some("sonnet"));
    assert!(c.set_cat_model("cat_miso", Some("--dangerously-skip-permissions")).is_err());
    assert_eq!(c.set_cat_access("cat_miso", Some(PermissionMode::Ask)).unwrap().0.access, Some(PermissionMode::Ask));
    // Renaming the cat renames its thread.
    c.rename_cat("cat_miso", "Miso").unwrap();
    assert_eq!(c.store.get_task(&own.id).unwrap().unwrap().title, "Miso");
}

#[test]
fn retired_cats_keep_their_tasks_and_come_back() {
    let d = dir();
    let mut c = open(&d);
    let (t, _) = c.create_task_for("Write the notes", None, Some("cat_mochi")).unwrap();
    let (cat, _) = c.retire_cat("cat_mochi").unwrap();
    assert!(cat.retired_ms.is_some());
    assert_eq!(names(&c), ["miso", "tofu"]);
    assert_eq!(c.cats().unwrap().last().unwrap().id, "cat_mochi", "retired ones are listed last");
    assert_eq!(cat_of(&c, &t.id), "cat_mochi", "its tasks stay its own");
    let (back, _) = c.bring_back_cat("cat_mochi").unwrap();
    assert!(back.is_active());
    assert_eq!(c.create_task_for("More", None, Some("cat_mochi")).unwrap().0.cat.as_deref(), Some("cat_mochi"));
}

fn who() -> Who {
    Who { id: "claude".into(), name: "Claude Code".into(), sign_in: "claude auth login".into(), install: String::new(), pricing: Pricing::ApiEquivalent }
}

fn run(c: &mut Core, d: &Gone, t: &TaskId, words: &str) {
    let place = Place { workdir: d.0.join("w").to_string_lossy().into_owned(), ..Place::default() };
    let task = c.store.get_task(t).unwrap().unwrap();
    let (handed, _) = c.handed(&task).unwrap();
    let (mut b, _) = RunBook::begin(c, t, who(), place, PermissionMode::Follow, None, Some(words), handed.as_ref()).unwrap();
    let now = c.now();
    b.finish(c, Outcome::Done, None, now).unwrap();
}

fn changed_lines(c: &Core, t: &TaskId) -> Vec<String> {
    c.store.parts_of(t).unwrap().into_iter().filter_map(|p| if let PartBody::RulesChanged { name, .. } = p.body { Some(name) } else { None }).collect()
}

#[test]
fn a_crew_run_records_its_crew_md_version_and_one_line_says_when_it_changed() {
    let d = dir();
    let mut c = open(&d);
    let t = c.create_task_for("Tidy the docs", None, Some("cat_miso")).unwrap().0.id;
    run(&mut c, &d, &t, "Tidy the docs");
    run(&mut c, &d, &t, "Again");
    assert!(changed_lines(&c, &t).is_empty(), "the same rules: no line");
    c.save_cattain_file("cat_miso", "CREW.md", "Use British spelling.").unwrap();
    run(&mut c, &d, &t, "Once more");
    assert_eq!(changed_lines(&c, &t), ["miso"], "one line, on the first run with the new CREW.md");
    assert!(c.store.parts_of(&t).unwrap().iter().any(|p| matches!(&p.body, PartBody::RulesChanged { file: Some(f), .. } if f == "CREW.md")), "it names the file");
    run(&mut c, &d, &t, "And again");
    assert_eq!(changed_lines(&c, &t).len(), 1);
    let runs = c.store.runs_of(&t).unwrap();
    let hashes: Vec<String> = runs.iter().map(|r| r.rules_hash.clone().unwrap()).collect();
    assert_eq!(hashes, [rules_hash(""), rules_hash(""), rules_hash("Use British spelling."), rules_hash("Use British spelling.")]);
    assert!(runs.iter().all(|r| r.cat.as_deref() == Some("cat_miso")));
    // The line comes before the person's words of that run.
    let parts = c.store.parts_of(&t).unwrap();
    let at = parts.iter().position(|p| matches!(p.body, PartBody::RulesChanged { .. })).unwrap();
    assert_eq!(parts[at + 1].body, PartBody::Person { text: "Once more".into() });
    assert_eq!(parts[at].run, parts[at + 1].run);
}

#[test]
fn a_version_5_store_gives_its_tasks_to_the_starter_cats_by_the_rule() {
    let d = dir();
    let path = d.0.join("neko.sqlite3");
    {
        let s = Store::open_at(&path, 5).unwrap();
        assert_eq!(s.version().unwrap(), 5);
        let conn = s.connection();
        conn.execute("INSERT INTO workspaces (id, name, created_ms, position) VALUES ('default', 'General', 1, 0)", []).unwrap();
        // Out of order on purpose: the migration goes oldest first. The third is still marked working.
        for (id, at, status) in [("task_b", 200, "done"), ("task_a", 100, "done"), ("task_c", 300, "working"), ("task_e", 500, "done"), ("task_d", 400, "failed"), ("task_f", 600, "done")] {
            conn.execute("INSERT INTO tasks (id, workspace_id, words, title, status, created_ms) VALUES (?1, 'default', 'w', 'w', ?2, ?3)", rusqlite::params![id, status, at]).unwrap();
        }
        conn.execute("INSERT INTO runs (id, task_id, runtime, status, started_ms, workdir) VALUES ('run_1', 'task_a', 'claude', 'done', 100, '/tmp')", []).unwrap();
    }
    let c = open(&d);
    assert_eq!(c.store.version().unwrap(), SCHEMA_VERSION);
    let got: Vec<(String, String)> = ["task_a", "task_b", "task_c", "task_d", "task_e", "task_f"].iter().map(|t| (t.to_string(), cat_of(&c, &TaskId(t.to_string())))).collect();
    // a, b, c in turn; c stays working, so tofu is busy: d and e go to the free cats least recently used, then f to miso again.
    let want = [("task_a", "cat_miso"), ("task_b", "cat_mochi"), ("task_c", "cat_tofu"), ("task_d", "cat_miso"), ("task_e", "cat_mochi"), ("task_f", "cat_miso")];
    assert_eq!(got, want.map(|(a, b)| (a.to_owned(), b.to_owned())));
    let cats = c.cats().unwrap();
    assert_eq!(cats.len(), 3);
    assert!(cats.iter().all(|x| x.created_ms == 100), "as old as the oldest task");
    let run = c.store.get_run(&neko_protocol::RunId("run_1".into())).unwrap().unwrap();
    assert_eq!((run.cat, run.rules_hash), (None, None), "an old run used no rules");
}

#[test]
fn a_version_16_store_makes_each_cat_a_cattain_with_its_rules_in_rules_md_and_leaves_its_threads_alone() {
    let d = dir();
    let path = d.0.join("neko.sqlite3");
    {
        let s = Store::open_at(&path, 16).unwrap();
        let conn = s.connection();
        conn.execute("INSERT INTO workspaces (id, name, created_ms, position) VALUES ('default', 'General', 1, 0)", []).unwrap();
        for (id, name, face, rules, previous) in [("cat_miso", "miso", "ginger", "MISO-RULE: plan first.", Some("older")), ("cat_mochi", "mochi", "snow", "MOCHI-RULE", None), ("cat_tofu", "tofu", "calico", "", None)] {
            conn.execute("INSERT INTO cats (id, name, name_key, face, rules, previous_rules, rules_saved_ms, created_ms) VALUES (?1, ?2, ?2, ?3, ?4, ?5, 7, 1)", rusqlite::params![id, name, face, rules, previous]).unwrap();
        }
        for (id, cat) in [("task_a", Some("cat_miso")), ("task_b", Some("cat_mochi")), ("task_c", None)] {
            conn.execute("INSERT INTO tasks (id, workspace_id, words, title, status, created_ms, cat_id) VALUES (?1, 'default', 'w', 'Old thread', 'done', 5, ?2)", rusqlite::params![id, cat]).unwrap();
        }
    }
    let mut c = open(&d);
    assert_eq!(c.store.version().unwrap(), SCHEMA_VERSION);
    let (rules, _) = c.cattain_file("cat_miso", "RULES.md").unwrap();
    assert_eq!((rules.text.as_str(), rules.previous), ("MISO-RULE: plan first.", true), "the rules are RULES.md, with their previous version");
    assert_eq!(c.restore_cattain_file("cat_miso", "RULES.md").unwrap().0.text, "older");
    assert_eq!(c.cattain_file("cat_mochi", "CREW.md").unwrap().0.text, "MOCHI-RULE", "and CREW.md, so its old threads read what they read before");
    assert_eq!(std::fs::read_to_string(d.0.join("cattains/cat_mochi/RULES.md")).unwrap(), "MOCHI-RULE", "on disk");
    assert_eq!(c.cattain_file("cat_tofu", "NOTES.md").unwrap().0.text, "");
    for cat in c.cats().unwrap() {
        assert!(d.0.join("cattains").join(&cat.id).join("DIGEST.md").exists(), "{} has its folder", cat.name);
        assert!(c.cattain_thread(&cat.id).unwrap().0.cattain, "{} has its own thread once opened", cat.name);
    }
    for (id, cat) in [("task_a", "cat_miso"), ("task_b", "cat_mochi"), ("task_c", "")] {
        let t = c.store.get_task(&TaskId(id.into())).unwrap().unwrap();
        assert_eq!((t.title.as_str(), t.cat.unwrap_or_default().as_str(), t.cattain, t.status), ("Old thread", cat, false, neko_protocol::TaskStatus::Done), "{id} is untouched");
    }
}
