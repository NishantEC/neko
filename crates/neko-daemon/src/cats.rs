//! `Request::Cats`: the daemon end of `neko_core::cats` and `neko_core::cat_files`.
//! One lock on the database is held for the whole request, so two requests never interleave.
use crate::server::AppState;
use neko_core::cats::{self, Changes, NewCat};
use neko_protocol::cats::{CatCommand, CatsReply};
use neko_protocol::Response;
use std::path::Path;

pub fn handle(state: &AppState, command: CatCommand) -> Response {
    handle_in(state, &neko_protocol::support_dir(), command)
}

/// `handle` with the data folder given, so a test keeps its files in its own folder.
pub fn handle_in(state: &AppState, root: &Path, command: CatCommand) -> Response {
    match answer(state, root, command) {
        Ok(reply) => Response::Cats(Box::new(reply)),
        Err(message) => Response::Error { message },
    }
}

fn answer(state: &AppState, root: &Path, command: CatCommand) -> Result<CatsReply, String> {
    let db = state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let db = &*db;
    let listing = || cats::list(db, false).map(CatsReply::Listing);
    match command {
        CatCommand::List { include_archived } => cats::list(db, include_archived).map(CatsReply::Listing),
        CatCommand::Create { name, face, workspace_id, area, model_policy, daily_limit_micro_usd, purpose } => {
            let snapshot = neko_core::workbench::load(db).map_err(|e| e.to_string())?;
            let input = NewCat { name, face, workspace_id, area, model_policy, daily_limit_micro_usd, purpose };
            cats::create(db, root, &snapshot.workspaces, input, neko_core::workbench::new_id(), neko_core::workbench::now_ms())?;
            listing()
        }
        CatCommand::Update { id, name, face, area, model_policy, daily_limit_micro_usd, purpose } => {
            cats::update(db, &id, Changes { name, face, area, model_policy, daily_limit_micro_usd, purpose })?;
            listing()
        }
        CatCommand::Archive { id } => cats::archive(db, &id).and_then(|()| listing()),
        CatCommand::Pin { id, pinned } => cats::pin(db, &id, pinned).and_then(|()| listing()),
        CatCommand::Reorder { ids } => cats::reorder(db, &ids).and_then(|()| listing()),
        CatCommand::Suggest => {
            let snapshot = neko_core::workbench::load(db).map_err(|e| e.to_string())?;
            let all = cats::list(db, true)?;
            cats::suggest(&all, &snapshot.workspaces, &snapshot.tasks, neko_core::workbench::now_ms()).map(CatsReply::Suggestion)
        }
        CatCommand::Files { id } => {
            cats::get(db, &id)?;
            neko_core::cat_files::read_all(root, &id).map(CatsReply::Files)
        }
        CatCommand::WriteFile { id, kind, text } => {
            cats::get(db, &id)?;
            neko_core::cat_files::write(root, &id, kind, &text)?;
            // Clients that show the files re-read them when the change counter moves.
            neko_core::cat_thread::bump_rev(db)?;
            neko_core::cat_files::read_all(root, &id).map(CatsReply::Files)
        }
        CatCommand::CatThread { cat, since_seq } => {
            neko_core::cat_thread::view(db, &cat, since_seq, state.workbench.cat_running(&cat)).map(CatsReply::Thread)
        }
        // Stores the person's message in the cat's own thread and queues the wake; the cat's tick runs the
        // turn. The Home chat is not involved.
        CatCommand::CatSay { cat, text } => {
            let message = neko_core::cat_thread::say(db, &cat, &text, neko_core::workbench::now_ms())?;
            Ok(CatsReply::Thread(neko_protocol::cats::CatThreadView {
                cat_id: cat.clone(),
                last_seq: message.seq,
                messages: vec![message],
                running: state.workbench.cat_running(&cat),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::cats::{Cat, CatFileKind, FaceRef, ModelPolicy};

    fn setup() -> (AppState, tempfile::TempDir) {
        let state = crate::server::tests::test_state_for_phone();
        {
            let db = state.db.lock().unwrap();
            let mut snapshot = neko_core::workbench::load(&db).unwrap();
            snapshot.workspaces.push(neko_protocol::workbench::Workspace { id: "w1".into(), name: "Web".into(), repository: "/tmp/acme-web".into(), instructions: String::new(), away_enabled: false });
            neko_core::workbench::save(&db, &snapshot).unwrap();
        }
        (state, tempfile::tempdir().unwrap())
    }

    fn create(name: &str) -> CatCommand {
        CatCommand::Create { name: name.into(), face: FaceRef("tabby-headphones".into()), workspace_id: "w1".into(), area: "Web".into(), model_policy: ModelPolicy::default(), daily_limit_micro_usd: Some(2_000_000), purpose: "Look after the web app.".into() }
    }

    fn listing(response: Response) -> Vec<Cat> {
        match response {
            Response::Cats(reply) => match *reply { CatsReply::Listing(list) => list, other => panic!("not a listing: {other:?}") },
            other => panic!("not Cats: {other:?}"),
        }
    }

    fn error(response: Response) -> String {
        match response { Response::Error { message } => message, other => panic!("not an error: {other:?}") }
    }

    #[test]
    fn create_list_update_pin_reorder_and_archive_round_trip() {
        let (state, root) = setup();
        let run = |command| handle_in(&state, root.path(), command);
        let made = listing(run(create("miso")));
        assert_eq!(made.len(), 1);
        assert_eq!((made[0].name.as_str(), made[0].face.0.as_str(), made[0].daily_limit_micro_usd), ("miso", "tabby-headphones", Some(2_000_000)));
        let tofu = listing(run(create("tofu")));
        assert_eq!(tofu.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["miso", "tofu"]);
        let (miso, tofu) = (tofu[0].id.clone(), tofu[1].id.clone());

        let updated = listing(run(CatCommand::Update { id: miso.clone(), name: Some("Miso II".into()), face: None, area: None, model_policy: None, daily_limit_micro_usd: Some(None), purpose: None }));
        assert_eq!((updated[0].name.as_str(), updated[0].daily_limit_micro_usd, updated[0].area.as_str()), ("Miso II", None, "Web"));
        assert!(listing(run(CatCommand::Pin { id: tofu.clone(), pinned: true }))[1].pinned);
        let reordered = listing(run(CatCommand::Reorder { ids: vec![tofu.clone(), miso.clone()] }));
        assert_eq!(reordered.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), [tofu.as_str(), miso.as_str()]);
        let archived = listing(run(CatCommand::Archive { id: tofu.clone() }));
        assert_eq!(archived.len(), 1);
        let all = listing(run(CatCommand::List { include_archived: true }));
        assert_eq!(all.len(), 2);
        assert!(all[1].archived && !all[1].pinned && all[1].id == tofu);
        assert_eq!(listing(run(CatCommand::List { include_archived: false })).len(), 1);
    }

    #[test]
    fn requests_report_the_rules_they_break() {
        let (state, root) = setup();
        let run = |command| handle_in(&state, root.path(), command);
        listing(run(create("miso")));
        assert!(error(run(create("MISO"))).contains("already a cat"));
        assert!(error(run(create(&"x".repeat(41)))).contains("at most 40"));
        let CatCommand::Create { name, face: _, workspace_id: _, area, model_policy, daily_limit_micro_usd, purpose } = create("nori") else { unreachable!() };
        let bad_face = CatCommand::Create { name: name.clone(), face: FaceRef("dog".into()), workspace_id: "w1".into(), area: area.clone(), model_policy: model_policy.clone(), daily_limit_micro_usd, purpose: purpose.clone() };
        assert!(error(run(bad_face)).contains("Unknown face"));
        let no_workspace = CatCommand::Create { name, face: FaceRef::default(), workspace_id: "gone".into(), area, model_policy, daily_limit_micro_usd, purpose };
        assert_eq!(error(run(no_workspace)), "That workspace no longer exists");
        assert_eq!(listing(run(CatCommand::List { include_archived: true })).len(), 1);
    }

    #[test]
    fn create_makes_four_files_that_files_lists_and_write_file_replaces_keeping_prev() {
        let (state, root) = setup();
        let run = |command| handle_in(&state, root.path(), command);
        let id = listing(run(create("miso")))[0].id.clone();
        let files = |response| match response {
            Response::Cats(reply) => match *reply { CatsReply::Files(files) => files, other => panic!("{other:?}") },
            other => panic!("{other:?}"),
        };
        let first = files(run(CatCommand::Files { id: id.clone() }));
        assert_eq!(first.files.iter().map(|f| f.kind).collect::<Vec<_>>(), CatFileKind::ALL);
        assert_eq!(first.files[0].text, "# Rules\n\nLook after the web app.\n");
        let written = files(run(CatCommand::WriteFile { id: id.clone(), kind: CatFileKind::Plan, text: "Ship the export fix first.".into() }));
        assert_eq!((written.files[2].text.as_str(), written.files[2].prev.as_deref()), ("Ship the export fix first.", Some("# Plan\n")));
        assert!(error(run(CatCommand::WriteFile { id: id.clone(), kind: CatFileKind::Plan, text: "a".repeat(16_385) })).contains("under 16 KB"));
        assert!(root.path().join("cats").join(&id).join("PLAN.md.prev").exists());
    }

    #[test]
    fn files_of_a_cat_that_does_not_exist_or_has_a_bad_id_are_refused_and_nothing_is_made() {
        let (state, root) = setup();
        let run = |command| handle_in(&state, root.path(), command);
        for id in ["nobody", "../x", "a/b", ""] {
            assert!(!error(run(CatCommand::Files { id: id.into() })).is_empty(), "{id:?}");
            assert!(!error(run(CatCommand::WriteFile { id: id.into(), kind: CatFileKind::Rules, text: "x".into() })).is_empty(), "{id:?}");
        }
        assert!(!root.path().join("cats").exists());
    }

    #[test]
    fn archiving_keeps_the_files_readable() {
        let (state, root) = setup();
        let run = |command| handle_in(&state, root.path(), command);
        let id = listing(run(create("miso")))[0].id.clone();
        listing(run(CatCommand::Archive { id: id.clone() }));
        assert!(matches!(run(CatCommand::Files { id: id.clone() }), Response::Cats(_)));
        assert!(root.path().join("cats").join(&id).join("RULES.md").exists());
    }

    #[test]
    fn suggest_answers_from_the_workspaces_and_creates_nothing() {
        let (state, root) = setup();
        let run = |command| handle_in(&state, root.path(), command);
        let Response::Cats(reply) = run(CatCommand::Suggest) else { panic!("not Cats") };
        let CatsReply::Suggestion(one) = *reply else { panic!("not a suggestion") };
        assert_eq!((one.workspace_id.as_str(), one.name.as_str()), ("w1", "miso"));
        assert!(listing(run(CatCommand::List { include_archived: true })).is_empty());
        assert!(!root.path().join("cats").exists());
    }

    #[test]
    fn the_wire_form_of_a_create_request_parses_with_defaults() {
        let request: neko_protocol::Request = serde_json::from_str(r#"{"Cats":{"Create":{"name":"miso","workspace_id":"w1"}}}"#).unwrap();
        let neko_protocol::Request::Cats(CatCommand::Create { face, daily_limit_micro_usd, .. }) = request else { panic!() };
        assert_eq!((face, daily_limit_micro_usd), (FaceRef::default(), None));
    }
    fn thread(response: Response) -> neko_protocol::cats::CatThreadView {
        match response {
            Response::Cats(reply) => match *reply { CatsReply::Thread(view) => view, other => panic!("not a thread: {other:?}") },
            other => panic!("not Cats: {other:?}"),
        }
    }

    #[test]
    fn say_stores_the_person_message_in_the_cats_thread_and_queues_its_wake_and_thread_reads_it_back() {
        let (state, root) = setup();
        let run = |command| handle_in(&state, root.path(), command);
        let id = listing(run(create("miso")))[0].id.clone();
        let rev = |state: &AppState| neko_core::workbench::load(&state.db.lock().unwrap()).unwrap().cats_rev;
        let before = rev(&state);
        let said = thread(run(CatCommand::CatSay { cat: id.clone(), text: "  Split the export rework in two.  ".into() }));
        assert_eq!((said.messages.len(), said.messages[0].seq, said.messages[0].text.as_str(), said.last_seq, said.running), (1, 1, "Split the export rework in two.", 1, false));
        assert!(rev(&state) > before, "clients hear of it through the snapshot");
        assert_eq!(neko_core::cat_thread::pending(&state.db.lock().unwrap(), &id).unwrap().len(), 1, "and the cat is woken");
        let again = thread(run(CatCommand::CatSay { cat: id.clone(), text: "And add a progress bar.".into() }));
        assert_eq!(again.messages[0].seq, 2);
        let all = thread(run(CatCommand::CatThread { cat: id.clone(), since_seq: 0 }));
        assert_eq!(all.messages.iter().map(|m| m.seq).collect::<Vec<_>>(), [1, 2]);
        let newer = thread(run(CatCommand::CatThread { cat: id.clone(), since_seq: 1 }));
        assert_eq!((newer.messages.len(), newer.messages[0].text.as_str(), newer.last_seq), (1, "And add a progress bar.", 2));
        assert!(thread(run(CatCommand::CatThread { cat: id.clone(), since_seq: 2 })).messages.is_empty());
        // Not the Home chat.
        assert!(neko_core::workbench::load(&state.db.lock().unwrap()).unwrap().conversation.is_empty());
    }

    #[test]
    fn say_and_thread_refuse_what_cannot_be_stored() {
        let (state, root) = setup();
        let run = |command| handle_in(&state, root.path(), command);
        let id = listing(run(create("miso")))[0].id.clone();
        assert!(error(run(CatCommand::CatSay { cat: id.clone(), text: "   ".into() })).contains("Write something"));
        assert!(error(run(CatCommand::CatSay { cat: id.clone(), text: "x".repeat(8_001) })).contains("under 8000"));
        assert_eq!(error(run(CatCommand::CatSay { cat: "nobody".into(), text: "hi".into() })), "That cat no longer exists");
        assert_eq!(error(run(CatCommand::CatThread { cat: "nobody".into(), since_seq: 0 })), "That cat no longer exists");
        listing(run(CatCommand::Archive { id: id.clone() }));
        assert!(error(run(CatCommand::CatSay { cat: id.clone(), text: "hello".into() })).contains("archived"));
        assert!(thread(run(CatCommand::CatThread { cat: id, since_seq: 0 })).messages.is_empty(), "an archived cat's thread stays readable");
    }

    #[test]
    fn the_wire_form_of_the_thread_requests_parses() {
        let read: neko_protocol::Request = serde_json::from_str(r#"{"Cats":{"CatThread":{"cat":"c1"}}}"#).unwrap();
        assert!(matches!(read, neko_protocol::Request::Cats(CatCommand::CatThread { since_seq: 0, .. })));
        let say: neko_protocol::Request = serde_json::from_str(r#"{"Cats":{"CatSay":{"cat":"c1","text":"hi"}}}"#).unwrap();
        assert!(matches!(say, neko_protocol::Request::Cats(CatCommand::CatSay { .. })));
    }
}
