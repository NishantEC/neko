//! Search in the store (docs/knowledge/search.md): what the two FTS5 tables hold and how they are kept, what a query finds, and what
//! the filters and the quotes do.

mod support;
use neko_core::shelf::Shelf;
use neko_core::{Core, FixedClock, SequentialIds, Store};
use neko_protocol::{FoundBy, Part, PartBody, PartId, Task, TaskId, TaskStatus, ToolState};
use support::Temp;

const T0: i64 = 1_764_000_000_000;
const DAY: i64 = 24 * 3_600_000;

fn open(d: &Temp) -> Core {
    Core::open(&d.0.join("neko.sqlite3"), Box::new(FixedClock::new(T0, 1000)), Box::new(SequentialIds::default())).unwrap()
}

fn part(task: &TaskId, seq: u64, body: PartBody) -> Part {
    Part { id: PartId(format!("part_{}_{seq}", task.0)), task: task.clone(), run: None, seq, at_ms: T0 + seq as i64, ended_ms: None, body }
}

fn say(core: &mut Core, task: &TaskId, seq: u64, text: &str) {
    core.store.insert_part(&part(task, seq, PartBody::Agent { text: text.into(), done: true }), T0).unwrap();
}

fn ask(core: &mut Core, task: &TaskId, seq: u64, text: &str) {
    core.store.insert_part(&part(task, seq, PartBody::Person { text: text.into() }), T0).unwrap();
}

fn ids(f: &[neko_protocol::Found]) -> Vec<&str> {
    f.iter().map(|f| f.task.0.as_str()).collect()
}

#[test]
fn a_title_is_found_by_the_start_of_any_word_and_a_message_by_what_was_said() {
    let d = Temp::new("search-basic");
    let mut core = open(&d);
    let (login, _) = core.create_task("Fix the login redirect").unwrap();
    let (docs, _) = core.create_task("Write the docs").unwrap();
    say(&mut core, &docs.id, 1, "I changed the **login** page's copy and left the redirect alone.\nNext I will check the links.");
    ask(&mut core, &docs.id, 2, "Please also mention the changelog");
    let (found, problem) = core.store.search("log", 12, T0).unwrap();
    assert_eq!(problem, None);
    assert_eq!(ids(&found), [login.id.0.as_str(), docs.id.0.as_str()], "the title's thread first, then the one that said it");
    assert_eq!((found[0].by, found[1].by), (FoundBy::Title, FoundBy::Message));
    assert!(found[1].snippet.to_lowercase().contains("login") && !found[1].snippet.contains('\n'), "a one line snippet: {:?}", found[1].snippet);
    assert_eq!((found[1].part.as_ref().map(|p| p.0.as_str()), found[1].seq), (Some(format!("part_{}_1", docs.id.0).as_str()), 1));
    // The person's words count too, and every word must be in the same message.
    assert_eq!(ids(&core.store.search("changelog", 12, T0).unwrap().0), [docs.id.0.as_str()]);
    assert_eq!(ids(&core.store.search("login changelog", 12, T0).unwrap().0), Vec::<&str>::new(), "the two words are in different messages");
    assert_eq!(ids(&core.store.search("copy left", 12, T0).unwrap().0), [docs.id.0.as_str()]);
    // Quotes: the words in order, whole.
    assert_eq!(ids(&core.store.search("\"redirect alone\"", 12, T0).unwrap().0), [docs.id.0.as_str()]);
    assert_eq!(ids(&core.store.search("\"alone redirect\"", 12, T0).unwrap().0), Vec::<&str>::new());
    // A thread's first words are not a part of its transcript; they are found too, with no part to open at.
    let (zebra, _) = core.create_task("Plan the trip\nBring the zebrafish tank").unwrap();
    let (found, _) = core.store.search("zebrafish", 12, T0).unwrap();
    assert_eq!(ids(&found), [zebra.id.0.as_str()]);
    assert_eq!((found[0].by, found[0].part.clone(), found[0].seq), (FoundBy::Message, None, 0));
    assert!(found[0].snippet.contains("zebrafish"), "{:?}", found[0].snippet);
    assert_eq!(ids(&core.store.search("trip", 12, T0).unwrap().0), [zebra.id.0.as_str()]);
    assert_eq!(core.store.search("trip", 12, T0).unwrap().0[0].by, FoundBy::Title);
    // Case and accents do not matter; nothing typed is read as a query language.
    assert_eq!(ids(&core.store.search("LOGIN NEAR( \"", 12, T0).unwrap().0), Vec::<&str>::new());
    assert_eq!(ids(&core.store.search("LOGIN", 12, T0).unwrap().0).len(), 2);
}

#[test]
fn only_what_was_said_is_indexed_and_the_index_follows_the_words_as_they_grow_and_go() {
    let d = Temp::new("search-triggers");
    let mut core = open(&d);
    let (t, _) = core.create_task("Notes").unwrap();
    let tool = PartBody::Tool { call: "c1".into(), name: "Bash".into(), summary: "ls".into(), input: "xylophone".into(), output: Some("xylophone".into()), state: ToolState::Done, exit: None };
    core.store.insert_part(&part(&t.id, 1, tool), T0).unwrap();
    core.store.insert_part(&part(&t.id, 2, PartBody::Thinking { text: "xylophone thoughts".into(), done: true }), T0).unwrap();
    assert!(core.store.search("xylophone", 12, T0).unwrap().0.is_empty(), "tool output and thinking are not what was said");
    // A part streams: each rewrite replaces what the index holds.
    let mut p = part(&t.id, 3, PartBody::Agent { text: "The plan is".into(), done: false });
    core.store.insert_part(&p, T0).unwrap();
    assert_eq!(core.store.search("plan", 12, T0).unwrap().0.len(), 1);
    p.body = PartBody::Agent { text: "The route is clear".into(), done: true };
    core.store.update_part(&p, T0).unwrap();
    assert!(core.store.search("plan", 12, T0).unwrap().0.is_empty(), "the old words are gone");
    assert_eq!(core.store.search("route clear", 12, T0).unwrap().0.len(), 1);
    // Removing the thread removes its words and its title from the index.
    core.store.delete_task(&t.id, T0).unwrap();
    assert!(core.store.search("route", 12, T0).unwrap().0.is_empty() && core.store.search("notes", 12, T0).unwrap().0.is_empty());
    let n: i64 = core.store.connection().query_row("SELECT (SELECT COUNT(*) FROM find_words) + (SELECT COUNT(*) FROM find_titles)", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn a_store_from_before_search_is_indexed_when_it_opens() {
    let d = Temp::new("search-migrate");
    let path = d.0.join("neko.sqlite3");
    let mut old = Store::open_at(&path, 13).unwrap();
    old.ensure_default_workspace(T0).unwrap();
    let task = old.create_task(&TaskId("task_old".into()), "default", "Plan the harvest", None, None, &Default::default(), T0).map(|t| t.0).unwrap();
    old.insert_part(&part(&task.id, 1, PartBody::Agent { text: "Carrots first, then the turnips.".into(), done: true }), T0).unwrap();
    old.insert_part(&part(&task.id, 2, PartBody::Notice { tone: neko_protocol::Tone::Info, text: "turnips are a notice".into(), next: None, failure: None }), T0).unwrap();
    assert_eq!(old.version().unwrap(), 13);
    drop(old);
    let s = Store::open(&path).unwrap();
    assert_eq!(s.version().unwrap(), 14);
    let (found, _) = s.search("turnip", 12, T0).unwrap();
    assert_eq!((ids(&found), found[0].by), (vec!["task_old"], FoundBy::Message), "the words of a thread made before search are found, a notice is not");
    assert_eq!(ids(&s.search("harvest", 12, T0).unwrap().0), ["task_old"]);
}

#[test]
fn the_filters_keep_only_what_they_say() {
    let d = Temp::new("search-filters");
    let mut core = open(&d);
    for (id, name) in [("ws_acme", "acme-api"), ("ws_mochi", "mochi-notes")] {
        std::fs::create_dir_all(d.0.join(name)).unwrap();
        core.store.add_workspace(id, d.0.join(name).to_str().unwrap(), name, T0).unwrap();
    }
    let (a, _) = core.create_task_in("Deploy the staging build", Some("ws_acme")).unwrap();
    let (m, _) = core.create_task_in("Deploy the notes site", Some("ws_mochi")).unwrap();
    let (n, _) = core.create_task_in("Deploy scratch", None).unwrap();
    core.store.set_task_status(&a.id, TaskStatus::Working, T0 + 10).unwrap();
    core.store.set_task_status(&m.id, TaskStatus::NeedsYou, T0 + 20).unwrap();
    let all = |q: &str| ids(&core.store.search(q, 12, T0 + 30).unwrap().0).into_iter().map(str::to_owned).collect::<Vec<_>>();
    assert_eq!(all("deploy").len(), 3);
    assert_eq!(all("deploy project:acme"), [a.id.0.clone()]);
    assert_eq!(all("project:MOCHI deploy"), [m.id.0.clone()]);
    assert_eq!(all("deploy project:none"), [n.id.0.clone()], "No project is `none` or `general`");
    assert_eq!(all("deploy project:nowhere"), Vec::<String>::new());
    // A thread that waits on a request is still working (its row shows the working ring); `needs` is the flag beside it.
    assert_eq!(all("deploy status:working").len(), 2);
    assert_eq!(all("deploy status:idle"), [n.id.0.clone()]);
    assert_eq!(all("status:needs"), [m.id.0.clone()], "a filter alone lists what it keeps, with no words");
    assert_eq!(all("deploy status:needs|idle").len(), 2);
    // `since:` counts from the thread's activity.
    core.store.connection().execute("UPDATE tasks SET activity_ms = ?2 WHERE id = ?1", rusqlite::params![n.id.0, T0 - 3 * DAY]).unwrap();
    assert_eq!(all("deploy since:2d").len(), 2);
    assert_eq!(all("deploy since:4d").len(), 3);
    // Threads made before cats went away still have theirs.
    let miso = core.store.list_cats().unwrap().into_iter().next().expect("a starter cat").id;
    core.store.connection().execute("UPDATE tasks SET cat_id = ?2 WHERE id = ?1", rusqlite::params![m.id.0, miso]).unwrap();
    let name = core.store.list_cats().unwrap().into_iter().next().unwrap().name.to_lowercase();
    assert_eq!(all(&format!("deploy cat:{}", &name[..2])), [m.id.0.clone()]);
    // A filter that cannot be read says why and finds nothing.
    let (none, problem) = core.store.search("deploy status:sleepy", 12, T0).unwrap();
    assert!(none.is_empty() && problem.unwrap().contains("not a status"));
}

#[test]
fn nothing_typed_lists_the_recent_threads_and_leaves_out_what_is_put_away() {
    let d = Temp::new("search-recent");
    let mut core = open(&d);
    let mut made: Vec<Task> = Vec::new();
    for (i, w) in ["First", "Second", "Third", "Fourth"].iter().enumerate() {
        let (t, _) = core.create_task(w).unwrap();
        core.store.connection().execute("UPDATE tasks SET activity_ms = ?2 WHERE id = ?1", rusqlite::params![t.id.0, T0 + i as i64 * 1000]).unwrap();
        made.push(t);
    }
    let (k, _) = core.create_subagent(&made[0].id, "Check links", "Look.").unwrap();
    core.store.connection().execute("UPDATE tasks SET activity_ms = ?2 WHERE id = ?1", rusqlite::params![k.id.0, T0 + 99_000]).unwrap();
    core.store.set_shelf(&made[2].id, Shelf::Settle, true, T0 + 5000).unwrap();
    core.store.set_shelf(&made[3].id, Shelf::Archive, true, T0 + 5000).unwrap();
    let (found, _) = core.store.search("", 12, T0 + 9000).unwrap();
    assert_eq!(ids(&found), [made[1].id.0.as_str(), made[0].id.0.as_str()], "newest first; a kitten, a settled and an archived thread are not recent");
    assert!(found.iter().all(|f| f.by == FoundBy::Recent && !f.kitten));
    // Settled and archived threads are found by name, by filter, and say so.
    assert_eq!(ids(&core.store.search("third", 12, T0).unwrap().0), [made[2].id.0.as_str()]);
    assert_eq!(ids(&core.store.search("status:archived", 12, T0).unwrap().0), [made[3].id.0.as_str()]);
    assert_eq!(ids(&core.store.search("status:settled", 12, T0).unwrap().0), [made[2].id.0.as_str()]);
    assert_eq!(core.store.search("", 1, T0).unwrap().0.len(), 1, "the limit holds");
}

#[test]
fn a_kitten_is_found_as_a_kitten_and_never_crowds_the_threads_out() {
    let d = Temp::new("search-kittens");
    let mut core = open(&d);
    let (parent, _) = core.create_task("Ship the release").unwrap();
    let mut kittens = Vec::new();
    for i in 0..6 {
        let (k, _) = core.create_subagent(&parent.id, &format!("Release check {i}"), "Look.").unwrap();
        kittens.push(k.id);
    }
    let (found, _) = core.store.search("release", 12, T0).unwrap();
    assert_eq!(found.iter().filter(|f| f.kitten).count(), 4, "at most four kittens");
    assert!(found.iter().any(|f| f.task == parent.id && !f.kitten));
    let (found, _) = core.store.search("release check 3", 12, T0).unwrap();
    assert_eq!((found.len(), found[0].task.clone(), found[0].kitten), (1, kittens[3].clone(), true));
}
