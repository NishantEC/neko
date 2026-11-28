//! Making a Cattain by asking (docs/knowledge/cats.md, "Made by asking"; version 22 of the protocol, 18 of the store): a name from the
//! pool, the first free face, the asking thread's project, RULES.md seeded, a near-twin reused, the limit refused with the house listed,
//! Undo while it has no work, the list the agent reads, and the migration from version 17.

use neko_core::cats::{Made, MakeAsk, default_rules};
use neko_core::conductor::{RunBook, Who};
use neko_core::runtime::Outcome;
use neko_core::store::SCHEMA_VERSION;
use neko_core::{CatError, Core, FixedClock, Place, SequentialIds, Store};
use neko_protocol::cats::{MAX_ACTIVE, RULES};
use neko_protocol::{PermissionMode, Pricing, TaskId};

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
    let d = std::env::temp_dir().join(format!("neko-make-cattain-test-{}-{n}-{nanos}", std::process::id()));
    std::fs::create_dir_all(d.join("w")).unwrap();
    Gone(d)
}

fn open(d: &Gone) -> Core {
    Core::open(&d.0.join("neko.sqlite3"), Box::new(FixedClock::new(1_764_000_000_000, 1000)), Box::new(SequentialIds::default())).unwrap()
}

fn ask(purpose: &str) -> MakeAsk<'_> {
    MakeAsk { purpose, ..MakeAsk::default() }
}

fn made(m: Result<Made, CatError>) -> neko_protocol::Cat {
    match m {
        Ok(Made::New { cat, .. }) => cat,
        other => panic!("expected a new Cattain, got {other:?}"),
    }
}

fn who() -> Who {
    Who { id: "claude".into(), name: "Claude Code".into(), sign_in: "claude auth login".into(), install: String::new(), pricing: Pricing::ApiEquivalent }
}

#[test]
fn a_cattain_made_by_asking_gets_a_pool_name_the_first_free_face_the_threads_project_and_its_rules() {
    let d = dir();
    let mut c = open(&d);
    std::fs::create_dir_all(d.0.join("acme-ios")).unwrap();
    let ws = c.add_workspace(&d.0.join("acme-ios").to_string_lossy()).unwrap().workspace;
    let (asking, _) = c.create_task_in("Make a Cattain for the iOS work", Some(&ws.id)).unwrap();
    let yuzu = match c.make_cattain(&asking.id, &ask("iOS work")).unwrap() {
        Made::New { cat, rules: (file, _), .. } => {
            assert_eq!((file.name.as_str(), file.text.as_str(), file.previous), (RULES, default_rules("iOS work").as_str(), false), "RULES.md is seeded, with nothing earlier to restore");
            cat
        }
        Made::Reused(c) => panic!("reused {}", c.name),
    };
    assert_eq!((yuzu.name.as_str(), yuzu.face.as_str(), yuzu.purpose.as_deref()), ("yuzu", "tabby", Some("iOS work")), "the first pool name, and the first face no active cat has (the starters have ginger, snow and calico)");
    assert_eq!((yuzu.workspace.as_deref(), yuzu.made_in.as_ref()), (Some(ws.id.as_str()), Some(&asking.id)), "the asking thread's project, and the thread that made it");
    assert_eq!(std::fs::read_to_string(d.0.join("cattains").join(&yuzu.id).join(RULES)).unwrap(), default_rules("iOS work"), "on disk, where its agent reads it");
    assert!(yuzu.thread.is_none(), "no thread until it is opened, and no run");
    assert_eq!(c.store.get_cat(&yuzu.id).unwrap().unwrap(), yuzu, "stored as made");
    // A thread in No project gives it none; given rules are kept verbatim; the next name is the pool's next.
    let (plain, _) = c.create_task("Another").unwrap();
    let kuro = made(c.make_cattain(&plain.id, &MakeAsk { purpose: "Release notes", rules: Some("Write in British English.\n"), ..MakeAsk::default() }));
    assert_eq!((kuro.name.as_str(), kuro.face.as_str(), kuro.workspace.as_deref()), ("kuro", "tuxedo", None));
    assert_eq!(c.cattain_file(&kuro.id, RULES).unwrap().0.text, "Write in British English.\n");
}

#[test]
fn the_same_purpose_names_the_cattain_there_is_and_a_taken_name_or_unknown_project_is_refused_in_words() {
    let d = dir();
    let mut c = open(&d);
    let (asking, _) = c.create_task("Ask").unwrap();
    let yuzu = made(c.make_cattain(&asking.id, &ask("iOS work")));
    match c.make_cattain(&asking.id, &ask(" ios Work. ")).unwrap() {
        Made::Reused(cat) => assert_eq!(cat.id, yuzu.id),
        Made::New { cat, .. } => panic!("made a near-twin, {}", cat.name),
    }
    assert_eq!(c.cats().unwrap().len(), 4, "nothing new");
    // A name the person gave is kept; one that is taken is refused (a retired cat's too).
    let named = made(c.make_cattain(&asking.id, &MakeAsk { name: Some(" Mobile "), purpose: "iOS and Android work", ..MakeAsk::default() }));
    assert_eq!(named.name, "Mobile");
    assert!(matches!(c.make_cattain(&asking.id, &MakeAsk { name: Some("MISO"), purpose: "Docs", ..MakeAsk::default() }), Err(CatError::InvalidName(w)) if w.contains("already a cat called miso")));
    assert!(matches!(c.make_cattain(&asking.id, &ask("  ")), Err(CatError::Refused(w)) if w.contains("needs a purpose")));
    std::fs::create_dir_all(d.0.join("acme-web")).unwrap();
    c.add_workspace(&d.0.join("acme-web").to_string_lossy()).unwrap();
    let e = c.make_cattain(&asking.id, &MakeAsk { purpose: "Web work", project: Some("acme-ios"), ..MakeAsk::default() }).unwrap_err().to_string();
    assert!(e.contains("no project called acme-ios") && e.contains("acme-web"), "{e}");
    let web = made(c.make_cattain(&asking.id, &MakeAsk { purpose: "Web work", project: Some("ACME-WEB"), ..MakeAsk::default() }));
    assert!(web.workspace.is_some(), "a project by its name, without case");
}

#[test]
fn past_sixteen_at_work_it_is_refused_with_the_house_listed() {
    let d = dir();
    let mut c = open(&d);
    let (asking, _) = c.create_task("Ask").unwrap();
    for i in 0..MAX_ACTIVE - 3 {
        made(c.make_cattain(&asking.id, &ask(&format!("Work {i}"))));
    }
    let e = c.make_cattain(&asking.id, &ask("iOS work")).unwrap_err();
    let CatError::Refused(words) = e else { panic!("{e:?}") };
    assert!(words.starts_with("Neko keeps at most 16 Cattains at work, and 16 are:\n- miso\n- mochi\n- tofu\n- yuzu (Work 0)\n"), "{words}");
    assert!(words.contains("No Cattain was made") && words.contains("Settings ▸ Cattains"), "{words}");
    // The same purpose as one at work is still that one, even at the limit.
    assert!(matches!(c.make_cattain(&asking.id, &ask("work 3")), Ok(Made::Reused(_))));
}

#[test]
fn undo_retires_one_that_has_no_work_and_keeps_one_that_has() {
    let d = dir();
    let mut c = open(&d);
    let (asking, _) = c.create_task("Ask").unwrap();
    let yuzu = made(c.make_cattain(&asking.id, &ask("iOS work")));
    let (cat, _) = c.undo_make_cattain(&yuzu.id).unwrap();
    assert!(!cat.is_active(), "retired");
    assert!(c.undo_make_cattain(&yuzu.id).is_ok(), "twice is the same");
    assert!(matches!(c.undo_make_cattain("cat_miso"), Err(CatError::Refused(w)) if w.contains("made by hand")));
    // Its own thread ran: kept.
    let kuro = made(c.make_cattain(&asking.id, &ask("Release notes")));
    let (own, _, _) = c.cattain_thread(&kuro.id).unwrap();
    assert!(!c.cattain_has_work(&kuro.id).unwrap(), "opened, never written in");
    let place = Place { workdir: d.0.join("w").to_string_lossy().into_owned(), ..Place::default() };
    let (mut b, _) = RunBook::begin(&mut c, &own.id, who(), place, PermissionMode::Follow, None, Some("Hello"), None).unwrap();
    let now = c.now();
    b.finish(&mut c, Outcome::Done, None, now).unwrap();
    assert!(matches!(c.undo_make_cattain(&kuro.id), Err(CatError::Refused(w)) if w.contains("has work already")));
    // A thread in its crew: kept.
    let sesame = made(c.make_cattain(&asking.id, &ask("Docs")));
    c.create_task_for("Fix the docs", None, Some(&sesame.id)).unwrap();
    assert!(c.undo_make_cattain(&sesame.id).is_err());
    assert!(c.store.get_cat(&sesame.id).unwrap().unwrap().is_active());
}

#[test]
fn the_list_the_agent_reads_names_each_at_work_with_its_purpose_and_crew() {
    let d = dir();
    let mut c = open(&d);
    let (asking, _) = c.create_task("Ask").unwrap();
    let yuzu = made(c.make_cattain(&asking.id, &ask("iOS work")));
    c.create_task_for("Fix the build", None, Some(&yuzu.id)).unwrap();
    c.retire_cat("cat_tofu").unwrap();
    let t = c.cattains_text().unwrap();
    assert!(t.starts_with("3 of at most 16 Cattains are at work:\n- miso: at rest\n- mochi: at rest\n- yuzu (iOS work): "), "{t}");
    assert!(t.contains("1 thread in its crew") && !t.contains("tofu") && t.contains("Reuse one"), "{t}");
    let _ = TaskId("unused".into());
}

#[test]
fn a_version_17_store_opens_with_every_cat_made_by_hand() {
    let d = dir();
    let path = d.0.join("neko.sqlite3");
    {
        let s = Store::open_at(&path, 17).unwrap();
        s.connection().execute("INSERT INTO cats (id, name, name_key, face, rules, created_ms) VALUES ('cat_pip', 'pip', 'pip', 'void', '', 1)", []).unwrap();
    }
    let c = open(&d);
    assert_eq!((c.store.version().unwrap(), SCHEMA_VERSION), (18, 18));
    assert!(c.cats().unwrap().iter().all(|c| c.made_in.is_none()));
}
