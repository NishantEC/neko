//! `Request::Captains`: the daemon end of `neko_core::captains` and `neko_core::captain_files`.
//! One lock on the database is held for the whole request, so two requests never interleave.
use crate::server::AppState;
use neko_core::captains::{self, Changes, NewCaptain};
use neko_protocol::captains::{CaptainCommand, CaptainsReply};
use neko_protocol::Response;
use std::path::Path;

pub fn handle(state: &AppState, command: CaptainCommand) -> Response {
    handle_in(state, &neko_protocol::support_dir(), command)
}

/// `handle` with the data folder given, so a test keeps its files in its own folder.
pub fn handle_in(state: &AppState, root: &Path, command: CaptainCommand) -> Response {
    match answer(state, root, command) {
        Ok(reply) => Response::Captains(Box::new(reply)),
        Err(message) => Response::Error { message },
    }
}

fn answer(state: &AppState, root: &Path, command: CaptainCommand) -> Result<CaptainsReply, String> {
    let db = state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let db = &*db;
    let listing = || captains::list(db, false).map(CaptainsReply::Listing);
    match command {
        CaptainCommand::List { include_archived } => captains::list(db, include_archived).map(CaptainsReply::Listing),
        CaptainCommand::Create { name, face, workspace_id, area, model_policy, daily_limit_micro_usd, purpose } => {
            let snapshot = neko_core::workbench::load(db).map_err(|e| e.to_string())?;
            let input = NewCaptain { name, face, workspace_id, area, model_policy, daily_limit_micro_usd, purpose };
            captains::create(db, root, &snapshot.workspaces, input, neko_core::workbench::new_id(), neko_core::workbench::now_ms())?;
            listing()
        }
        CaptainCommand::Update { id, name, face, area, model_policy, daily_limit_micro_usd, purpose } => {
            captains::update(db, &id, Changes { name, face, area, model_policy, daily_limit_micro_usd, purpose })?;
            listing()
        }
        CaptainCommand::Archive { id } => captains::archive(db, &id).and_then(|()| listing()),
        CaptainCommand::Pin { id, pinned } => captains::pin(db, &id, pinned).and_then(|()| listing()),
        CaptainCommand::Reorder { ids } => captains::reorder(db, &ids).and_then(|()| listing()),
        CaptainCommand::Suggest => {
            let snapshot = neko_core::workbench::load(db).map_err(|e| e.to_string())?;
            let all = captains::list(db, true)?;
            captains::suggest(&all, &snapshot.workspaces, &snapshot.tasks, neko_core::workbench::now_ms()).map(CaptainsReply::Suggestion)
        }
        CaptainCommand::Files { id } => {
            captains::get(db, &id)?;
            neko_core::captain_files::read_all(root, &id).map(CaptainsReply::Files)
        }
        CaptainCommand::WriteFile { id, kind, text } => {
            captains::get(db, &id)?;
            neko_core::captain_files::write(root, &id, kind, &text)?;
            // Clients that show the files re-read them when the change counter moves.
            neko_core::captain_thread::bump_rev(db)?;
            neko_core::captain_files::read_all(root, &id).map(CaptainsReply::Files)
        }
        CaptainCommand::CaptainThread { captain, since_seq } => {
            neko_core::captain_thread::view(db, &captain, since_seq, state.workbench.captain_running(&captain)).map(CaptainsReply::Thread)
        }
        // Stores the person's message in the captain's own thread and queues the wake; the captain's tick runs the
        // turn. The Home chat is not involved.
        CaptainCommand::CaptainSay { captain, text } => {
            let message = neko_core::captain_thread::say(db, &captain, &text, neko_core::workbench::now_ms())?;
            Ok(CaptainsReply::Thread(neko_protocol::captains::CaptainThreadView {
                captain_id: captain.clone(),
                last_seq: message.seq,
                messages: vec![message],
                running: state.workbench.captain_running(&captain),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::captains::{Captain, CaptainFileKind, FaceRef, ModelPolicy};

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

    fn create(name: &str) -> CaptainCommand {
        CaptainCommand::Create { name: name.into(), face: FaceRef("tabby-headphones".into()), workspace_id: "w1".into(), area: "Web".into(), model_policy: ModelPolicy::default(), daily_limit_micro_usd: Some(2_000_000), purpose: "Look after the web app.".into() }
    }

    fn listing(response: Response) -> Vec<Captain> {
        match response {
            Response::Captains(reply) => match *reply { CaptainsReply::Listing(list) => list, other => panic!("not a listing: {other:?}") },
            other => panic!("not Captains: {other:?}"),
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

        let updated = listing(run(CaptainCommand::Update { id: miso.clone(), name: Some("Miso II".into()), face: None, area: None, model_policy: None, daily_limit_micro_usd: Some(None), purpose: None }));
        assert_eq!((updated[0].name.as_str(), updated[0].daily_limit_micro_usd, updated[0].area.as_str()), ("Miso II", None, "Web"));
        assert!(listing(run(CaptainCommand::Pin { id: tofu.clone(), pinned: true }))[1].pinned);
        let reordered = listing(run(CaptainCommand::Reorder { ids: vec![tofu.clone(), miso.clone()] }));
        assert_eq!(reordered.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), [tofu.as_str(), miso.as_str()]);
        let archived = listing(run(CaptainCommand::Archive { id: tofu.clone() }));
        assert_eq!(archived.len(), 1);
        let all = listing(run(CaptainCommand::List { include_archived: true }));
        assert_eq!(all.len(), 2);
        assert!(all[1].archived && !all[1].pinned && all[1].id == tofu);
        assert_eq!(listing(run(CaptainCommand::List { include_archived: false })).len(), 1);
    }

    #[test]
    fn requests_report_the_rules_they_break() {
        let (state, root) = setup();
        let run = |command| handle_in(&state, root.path(), command);
        listing(run(create("miso")));
        assert!(error(run(create("MISO"))).contains("already a captain"));
        assert!(error(run(create(&"x".repeat(41)))).contains("at most 40"));
        let CaptainCommand::Create { name, face: _, workspace_id: _, area, model_policy, daily_limit_micro_usd, purpose } = create("nori") else { unreachable!() };
        let bad_face = CaptainCommand::Create { name: name.clone(), face: FaceRef("dog".into()), workspace_id: "w1".into(), area: area.clone(), model_policy: model_policy.clone(), daily_limit_micro_usd, purpose: purpose.clone() };
        assert!(error(run(bad_face)).contains("Unknown face"));
        let no_workspace = CaptainCommand::Create { name, face: FaceRef::default(), workspace_id: "gone".into(), area, model_policy, daily_limit_micro_usd, purpose };
        assert_eq!(error(run(no_workspace)), "That workspace no longer exists");
        assert_eq!(listing(run(CaptainCommand::List { include_archived: true })).len(), 1);
    }

    #[test]
    fn create_makes_four_files_that_files_lists_and_write_file_replaces_keeping_prev() {
        let (state, root) = setup();
        let run = |command| handle_in(&state, root.path(), command);
        let id = listing(run(create("miso")))[0].id.clone();
        let files = |response| match response {
            Response::Captains(reply) => match *reply { CaptainsReply::Files(files) => files, other => panic!("{other:?}") },
            other => panic!("{other:?}"),
        };
        let first = files(run(CaptainCommand::Files { id: id.clone() }));
        assert_eq!(first.files.iter().map(|f| f.kind).collect::<Vec<_>>(), CaptainFileKind::ALL);
        assert_eq!(first.files[0].text, "# Rules\n\nLook after the web app.\n");
        let written = files(run(CaptainCommand::WriteFile { id: id.clone(), kind: CaptainFileKind::Plan, text: "Ship the export fix first.".into() }));
        assert_eq!((written.files[2].text.as_str(), written.files[2].prev.as_deref()), ("Ship the export fix first.", Some("# Plan\n")));
        assert!(error(run(CaptainCommand::WriteFile { id: id.clone(), kind: CaptainFileKind::Plan, text: "a".repeat(16_385) })).contains("under 16 KB"));
        assert!(root.path().join("captains").join(&id).join("PLAN.md.prev").exists());
    }

    #[test]
    fn files_of_a_captain_that_does_not_exist_or_has_a_bad_id_are_refused_and_nothing_is_made() {
        let (state, root) = setup();
        let run = |command| handle_in(&state, root.path(), command);
        for id in ["nobody", "../x", "a/b", ""] {
            assert!(!error(run(CaptainCommand::Files { id: id.into() })).is_empty(), "{id:?}");
            assert!(!error(run(CaptainCommand::WriteFile { id: id.into(), kind: CaptainFileKind::Rules, text: "x".into() })).is_empty(), "{id:?}");
        }
        assert!(!root.path().join("captains").exists());
    }

    #[test]
    fn archiving_keeps_the_files_readable() {
        let (state, root) = setup();
        let run = |command| handle_in(&state, root.path(), command);
        let id = listing(run(create("miso")))[0].id.clone();
        listing(run(CaptainCommand::Archive { id: id.clone() }));
        assert!(matches!(run(CaptainCommand::Files { id: id.clone() }), Response::Captains(_)));
        assert!(root.path().join("captains").join(&id).join("RULES.md").exists());
    }

    #[test]
    fn suggest_answers_from_the_workspaces_and_creates_nothing() {
        let (state, root) = setup();
        let run = |command| handle_in(&state, root.path(), command);
        let Response::Captains(reply) = run(CaptainCommand::Suggest) else { panic!("not Captains") };
        let CaptainsReply::Suggestion(one) = *reply else { panic!("not a suggestion") };
        assert_eq!((one.workspace_id.as_str(), one.name.as_str()), ("w1", "miso"));
        assert!(listing(run(CaptainCommand::List { include_archived: true })).is_empty());
        assert!(!root.path().join("captains").exists());
    }

    #[test]
    fn the_wire_form_of_a_create_request_parses_with_defaults() {
        let request: neko_protocol::Request = serde_json::from_str(r#"{"Captains":{"Create":{"name":"miso","workspace_id":"w1"}}}"#).unwrap();
        let neko_protocol::Request::Captains(CaptainCommand::Create { face, daily_limit_micro_usd, .. }) = request else { panic!() };
        assert_eq!((face, daily_limit_micro_usd), (FaceRef::default(), None));
    }
    fn thread(response: Response) -> neko_protocol::captains::CaptainThreadView {
        match response {
            Response::Captains(reply) => match *reply { CaptainsReply::Thread(view) => view, other => panic!("not a thread: {other:?}") },
            other => panic!("not Captains: {other:?}"),
        }
    }

    #[test]
    fn say_stores_the_person_message_in_the_captains_thread_and_queues_its_wake_and_thread_reads_it_back() {
        let (state, root) = setup();
        let run = |command| handle_in(&state, root.path(), command);
        let id = listing(run(create("miso")))[0].id.clone();
        let rev = |state: &AppState| neko_core::workbench::load(&state.db.lock().unwrap()).unwrap().captains_rev;
        let before = rev(&state);
        let said = thread(run(CaptainCommand::CaptainSay { captain: id.clone(), text: "  Split the export rework in two.  ".into() }));
        assert_eq!((said.messages.len(), said.messages[0].seq, said.messages[0].text.as_str(), said.last_seq, said.running), (1, 1, "Split the export rework in two.", 1, false));
        assert!(rev(&state) > before, "clients hear of it through the snapshot");
        assert_eq!(neko_core::captain_thread::pending(&state.db.lock().unwrap(), &id).unwrap().len(), 1, "and the captain is woken");
        let again = thread(run(CaptainCommand::CaptainSay { captain: id.clone(), text: "And add a progress bar.".into() }));
        assert_eq!(again.messages[0].seq, 2);
        let all = thread(run(CaptainCommand::CaptainThread { captain: id.clone(), since_seq: 0 }));
        assert_eq!(all.messages.iter().map(|m| m.seq).collect::<Vec<_>>(), [1, 2]);
        let newer = thread(run(CaptainCommand::CaptainThread { captain: id.clone(), since_seq: 1 }));
        assert_eq!((newer.messages.len(), newer.messages[0].text.as_str(), newer.last_seq), (1, "And add a progress bar.", 2));
        assert!(thread(run(CaptainCommand::CaptainThread { captain: id.clone(), since_seq: 2 })).messages.is_empty());
        // Not the Home chat.
        assert!(neko_core::workbench::load(&state.db.lock().unwrap()).unwrap().conversation.is_empty());
    }

    #[test]
    fn say_and_thread_refuse_what_cannot_be_stored() {
        let (state, root) = setup();
        let run = |command| handle_in(&state, root.path(), command);
        let id = listing(run(create("miso")))[0].id.clone();
        assert!(error(run(CaptainCommand::CaptainSay { captain: id.clone(), text: "   ".into() })).contains("Write something"));
        assert!(error(run(CaptainCommand::CaptainSay { captain: id.clone(), text: "x".repeat(8_001) })).contains("under 8000"));
        assert_eq!(error(run(CaptainCommand::CaptainSay { captain: "nobody".into(), text: "hi".into() })), "That captain no longer exists");
        assert_eq!(error(run(CaptainCommand::CaptainThread { captain: "nobody".into(), since_seq: 0 })), "That captain no longer exists");
        listing(run(CaptainCommand::Archive { id: id.clone() }));
        assert!(error(run(CaptainCommand::CaptainSay { captain: id.clone(), text: "hello".into() })).contains("archived"));
        assert!(thread(run(CaptainCommand::CaptainThread { captain: id, since_seq: 0 })).messages.is_empty(), "an archived captain's thread stays readable");
    }

    #[test]
    fn the_wire_form_of_the_thread_requests_parses() {
        let read: neko_protocol::Request = serde_json::from_str(r#"{"Captains":{"CaptainThread":{"captain":"c1"}}}"#).unwrap();
        assert!(matches!(read, neko_protocol::Request::Captains(CaptainCommand::CaptainThread { since_seq: 0, .. })));
        let say: neko_protocol::Request = serde_json::from_str(r#"{"Captains":{"CaptainSay":{"captain":"c1","text":"hi"}}}"#).unwrap();
        assert!(matches!(say, neko_protocol::Request::Captains(CaptainCommand::CaptainSay { .. })));
    }
}
