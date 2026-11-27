//! Cats in the store (docs/knowledge/cats.md): the starters, the name rules, the limits, that a thread has no cat (version 14), the
//! rules and their previous version, retiring and bringing back, a run's rules version and the line when it changed, and the
//! migration from version 5.

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
    assert!(cats.iter().all(|c| c.rules.is_empty() && c.purpose.is_none() && c.workspace.is_none()), "starters have empty rules");
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
fn rules_are_saved_verbatim_with_one_previous_version_to_restore() {
    let d = dir();
    let mut c = open(&d);
    assert!(matches!(c.restore_cat_rules("cat_miso"), Err(CatError::Refused(_))), "nothing to restore yet");
    let first = "# House rules\n\n- Use British spelling.  \n- **Never** push.\n\n";
    let (cat, _) = c.save_cat_rules("cat_miso", first).unwrap();
    assert_eq!((cat.rules.as_str(), cat.previous_rules.as_deref(), cat.rules_hash.clone()), (first, Some(""), rules_hash(first)));
    let (cat, _) = c.save_cat_rules("cat_miso", "Second").unwrap();
    assert_eq!((cat.rules.as_str(), cat.previous_rules.as_deref()), ("Second", Some(first)), "one previous version is kept");
    let (cat, _) = c.restore_cat_rules("cat_miso").unwrap();
    assert_eq!((cat.rules.as_str(), cat.previous_rules.as_deref()), (first, Some("Second")), "restore swaps them");
    assert!(matches!(c.save_cat_rules("cat_miso", &"x".repeat(MAX_RULES + 1)), Err(CatError::Refused(_))));
    assert!(c.save_cat_rules("cat_miso", &"x".repeat(MAX_RULES)).is_ok(), "16 KB fits");
    drop(c);
    let c = open(&d);
    assert_eq!(c.store.get_cat("cat_miso").unwrap().unwrap().previous_rules.as_deref(), Some(first), "kept across a restart");
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
    let cat = c.store.get_cat(&c.store.get_task(t).unwrap().unwrap().cat.unwrap_or_default()).unwrap();
    let (mut b, _) = RunBook::begin(c, t, who(), place, PermissionMode::Follow, None, Some(words), cat.as_ref()).unwrap();
    let now = c.now();
    b.finish(c, Outcome::Done, None, now).unwrap();
}

fn changed_lines(c: &Core, t: &TaskId) -> Vec<String> {
    c.store.parts_of(t).unwrap().into_iter().filter_map(|p| if let PartBody::RulesChanged { name, .. } = p.body { Some(name) } else { None }).collect()
}

#[test]
fn a_run_records_its_rules_version_and_one_line_says_when_they_changed() {
    let d = dir();
    let mut c = open(&d);
    let t = c.create_task_for("Tidy the docs", None, Some("cat_miso")).unwrap().0.id;
    run(&mut c, &d, &t, "Tidy the docs");
    run(&mut c, &d, &t, "Again");
    assert!(changed_lines(&c, &t).is_empty(), "the same rules: no line");
    c.save_cat_rules("cat_miso", "Use British spelling.").unwrap();
    run(&mut c, &d, &t, "Once more");
    assert_eq!(changed_lines(&c, &t), ["miso"], "one line, on the first run with the new rules");
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
