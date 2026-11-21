//! Cats, slice 1 (docs/knowledge/cats.md, "Rune: slice 1"): the sidebar's Cats section, the New cat sheet, the cat page with its panel, the
//! four files and their editor, archive and reorder. Driven through Rune's headless `Runtime` over made-up data (`scenes_cats.rs`) with the
//! daemon's answers fed in by hand: nothing here needs a daemon, and every request the screens make is read from `App::outbox`.

use neko_protocol::cats::{CatFile, CatFileKind, CatFiles};
use neko_rune::app::App;
use neko_rune::cats::{F_FILE, F_NAME};
use neko_rune::link::{Call, CatCall, CatsData, Reply};
use neko_rune::scenes::SCENES;
use neko_rune::scenes_cats::{demo_cats, listing};
use neko_rune::view::view;
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const WINDOW: (f32, f32) = (1440.0, 900.0);

fn runtime(scene: &str) -> Runtime<App> {
    let (_, f) = SCENES.iter().find(|(n, _)| *n == scene).unwrap_or_else(|| panic!("no scene {scene}"));
    let mut app = App::headless();
    f(&mut app);
    app.outbox.clear();
    let mut rt = Runtime::new(app, view, WINDOW, Env { system_dark: true, scale: 1.0 });
    rt.on_key = Some(neko_rune::on_key);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn info(rt: &Runtime<App>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    info(rt, key).is_some()
}

fn has_text(rt: &Runtime<App>, needle: &str) -> bool {
    rt.elements().into_iter().any(|e| !e.exiting && e.text.as_deref().is_some_and(|t| t.contains(needle)))
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

/// Clicks the element whose words are `words` (a menu's item has no key of ours).
fn click_text(rt: &mut Runtime<App>, words: &str) {
    let at = rt.elements().into_iter().find(|e| !e.exiting && e.text.as_deref() == Some(words)).map(|e| (e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0));
    let (x, y) = at.unwrap_or_else(|| panic!("no text {words:?}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

/// What the screens asked of the daemon about cats, in order.
fn asked(rt: &Runtime<App>) -> Vec<CatCall> {
    rt.state.outbox.iter().filter_map(|c| if let Call::Cats(c) = c { Some((**c).clone()) } else { None }).collect()
}

fn answer(rt: &mut Runtime<App>, call: CatCall, data: CatsData) {
    rt.update(move |s, _| {
        s.apply_reply(&Call::Cats(Box::new(call)), Reply::Cats(Box::new(data)));
    });
    settle(rt);
}

fn refuse(rt: &mut Runtime<App>, call: CatCall, message: &str) {
    let message = message.to_owned();
    rt.update(move |s, _| {
        s.apply_reply(&Call::Cats(Box::new(call)), Reply::Refused(message));
    });
    settle(rt);
}

fn files_of(text: &str) -> CatFiles {
    CatFiles { id: "c1".into(), files: CatFileKind::ALL.iter().map(|k| CatFile { kind: *k, text: if *k == CatFileKind::Rules { text.into() } else { format!("# {k:?}\n") }, updated_ms: 1, prev: None, truncated: false }).collect() }
}

// ---- the sidebar --------------------------------------------------------------------------------

#[test]
fn the_sidebar_shows_three_pinned_faces_with_names_and_the_rest_as_rows_with_their_area() {
    let mut rt = runtime("cats-0");
    answer(&mut rt, CatCall::List, listing(5));
    for id in ["c1", "c2", "c3"] {
        assert!(has_key(&rt, &format!("cat-{id}")), "{id} is a face");
        assert!(!has_key(&rt, &format!("cat-area-{id}")), "{id}: a face has no area beside it");
    }
    let names: Vec<String> = ["c1", "c2", "c3"].iter().map(|id| info(&rt, &format!("cat-name-{id}")).and_then(|e| e.text).unwrap_or_default()).collect();
    assert_eq!(names, ["miso", "mochi", "tofu"]);
    // The rest are rows: face, name, area.
    for (id, area) in [("c4", "Mobile"), ("c5", "Docs")] {
        assert_eq!(info(&rt, &format!("cat-area-{id}")).and_then(|e| e.text).as_deref(), Some(area));
    }
    // Faces side by side, then the rows under them, in the Cats section.
    let (a, b, c) = (info(&rt, "cat-c1").unwrap().rect, info(&rt, "cat-c2").unwrap().rect, info(&rt, "cat-c3").unwrap().rect);
    assert!(a[0] < b[0] && b[0] < c[0] && (a[1] - c[1]).abs() < 0.5, "three faces in a row");
    let (row4, row5) = (info(&rt, "cat-c4").unwrap().rect, info(&rt, "cat-c5").unwrap().rect);
    assert!(row4[1] >= a[1] + a[3] && row5[1] > row4[1], "the rows come under the faces, in order");
    assert_eq!(row4[3], 36.0, "a row is 36 high like every row of the column");
    assert!(has_key(&rt, "cats-new") && has_text(&rt, "Cats"), "the header has the title and a plus");
}

#[test]
fn faces_carry_their_marks_a_dot_when_a_task_waits_and_a_block_when_tasks_run() {
    let rt = runtime("cats-3");
    // mochi has a plan to approve; miso and tofu have tasks building.
    assert_eq!(rt.state.cat_marks("c2"), neko_rune::cat_face::Marks { needs_you: true, working: false });
    assert_eq!(rt.state.cat_marks("c1"), neko_rune::cat_face::Marks { needs_you: false, working: true });
    assert_eq!(rt.state.cat_marks("c3"), neko_rune::cat_face::Marks { needs_you: false, working: true });
    assert!(has_key(&rt, "badge-needs") && has_key(&rt, "badge-working"));
    assert!(rt.state.cat_marks("nobody") == neko_rune::cat_face::Marks::default());
}

#[test]
fn no_cats_says_so_and_offers_a_new_cat_which_opens_the_sheet_and_asks_for_a_suggestion() {
    let mut rt = runtime("cats-0");
    assert!(has_text(&rt, "No cats yet"));
    assert!(has_key(&rt, "cats-empty-new"));
    click(&mut rt, "cats-empty-new");
    assert!(rt.state.cats.sheet.is_some() && has_key(&rt, "cat-sheet-body"), "the sheet is open");
    assert!(asked(&rt).contains(&CatCall::Suggest), "the first cat starts from what the daemon suggests: {:?}", asked(&rt));
    // The suggestion fills an untouched sheet, and never one the person has typed in.
    let suggestion = neko_protocol::cats::CatSuggestion { name: "miso".into(), face: neko_protocol::cats::FaceRef("tabby-headphones".into()), workspace_id: "w2".into(), area: "Web".into(), purpose: "Looks after the web work.".into() };
    answer(&mut rt, CatCall::Suggest, CatsData::Suggestion(suggestion));
    assert_eq!(rt.state.pages.text(F_NAME), "miso");
    assert_eq!(rt.state.cats.sheet.as_ref().map(|s| (s.face.as_str(), s.workspace.as_str())), Some(("tabby-headphones", "w2")));
}

#[test]
fn the_list_is_read_when_a_snapshot_first_arrives_and_again_when_cats_rev_moves() {
    let mut app = App::headless();
    app.cats_sync(0);
    assert_eq!(app.outbox.len(), 1, "the first snapshot asks for the list");
    app.cats_sync(0);
    assert_eq!(app.outbox.len(), 1, "a snapshot while the answer is on its way asks nothing more");
    app.cats.list = Some(Vec::new());
    app.cats.asked = false;
    app.cats_sync(0);
    assert_eq!(app.outbox.len(), 1, "the same revision asks nothing");
    app.cats_sync(3);
    assert_eq!(app.outbox.len(), 2, "a moved revision asks again");
    assert!(matches!(&app.outbox[1], Call::Cats(c) if **c == CatCall::List));
}

// ---- the New cat sheet --------------------------------------------------------------------------

#[test]
fn create_sends_every_field_and_the_new_cat_opens_when_the_daemon_answers() {
    let mut rt = runtime("cat-sheet");
    // The next free face is picked for a fourth cat, and the live preview shows what was typed.
    assert_eq!(rt.state.cats.sheet.as_ref().map(|s| s.face.clone()).as_deref(), Some("grey-scarf"));
    assert_eq!(info(&rt, "cat-preview-name").and_then(|e| e.text).as_deref(), Some("nori"));
    assert_eq!(info(&rt, "cat-preview-area").and_then(|e| e.text).as_deref(), Some("Mobile"));
    click(&mut rt, "cat-face-cream-bowtie");
    click(&mut rt, "cat-sheet-save");
    let sent = asked(&rt);
    let [CatCall::Create { name, face, workspace_id, area, daily_limit_micro_usd, purpose, model_policy }] = sent.as_slice() else { panic!("one Create: {sent:?}") };
    assert_eq!((name.as_str(), face.as_str(), workspace_id.as_str(), area.as_str()), ("nori", "cream-bowtie", "w1", "Mobile"));
    assert_eq!(*daily_limit_micro_usd, Some(4_000_000));
    assert!(purpose.starts_with("Looks after the mobile app"));
    assert_eq!(*model_policy, Default::default(), "Neko's default model");
    assert!(rt.state.cats.sheet.as_ref().is_some_and(|s| s.busy), "it waits for the answer");
    // The daemon answers with the active list; the sheet closes and the new cat's page opens.
    answer(&mut rt, sent[0].clone(), listing(4));
    assert!(rt.state.cats.sheet.is_none() && !has_key(&rt, "cat-sheet-body"));
    assert_eq!(rt.state.cats.open.as_deref(), Some("c4"));
    assert!(has_key(&rt, "cat-page") && has_text(&rt, "Talk to nori here"));
}

#[test]
fn a_model_chosen_in_the_sheet_goes_into_the_model_policy() {
    let mut rt = runtime("cat-sheet");
    rt.update(|s, _| s.sheet_model(Some(("claude".into(), "haiku".into()))));
    click(&mut rt, "cat-sheet-save");
    let sent = asked(&rt);
    let [CatCall::Create { model_policy, .. }] = sent.as_slice() else { panic!("{sent:?}") };
    assert_eq!(model_policy.preferences, neko_rune::scenes_cats::claude_haiku());
}

#[test]
fn the_daemons_refusals_show_under_the_field_they_are_about_and_the_sheet_stays_open() {
    let mut rt = runtime("cat-sheet");
    click(&mut rt, "cat-sheet-save");
    let call = asked(&rt).remove(0);
    refuse(&mut rt, call.clone(), "There is already a cat called nori");
    assert!(has_key(&rt, "cat-sheet-body"), "the sheet is still open");
    assert_eq!(info(&rt, "cat-error-name").and_then(|e| e.text).as_deref(), Some("There is already a cat called nori"));
    assert!(rt.state.cats.sheet.as_ref().is_some_and(|s| !s.busy), "it can be sent again");
    refuse(&mut rt, call.clone(), "The daily limit must be between 1 micro-dollar and 1,000,000 dollars");
    assert!(has_key(&rt, "cat-error-limit") && has_key(&rt, "cat-error-name"), "each refusal under its own field");
    refuse(&mut rt, call, "You can have at most 16 cats; archive one first");
    assert!(has_text(&rt, "You can have at most 16 cats"), "a refusal about no field in particular has its own line");
}

#[test]
fn this_client_refuses_what_it_can_before_asking_and_says_why_under_the_field() {
    let mut rt = runtime("cat-sheet");
    rt.update(|s, _| {
        s.pages.set_field(F_NAME, "   ");
        s.pages.set_field(neko_rune::cats::F_LIMIT, "five dollars");
        s.sheet_submit();
    });
    settle(&mut rt);
    assert!(asked(&rt).is_empty(), "nothing is sent: {:?}", asked(&rt));
    assert_eq!(info(&rt, "cat-error-name").and_then(|e| e.text).as_deref(), Some("Give the cat a name"));
    assert!(has_key(&rt, "cat-error-limit"));
    // With no name the Create button is off, and pressing it does nothing.
    click(&mut rt, "cat-sheet-save");
    assert!(asked(&rt).is_empty());
}

#[test]
fn edit_opens_the_same_sheet_filled_in_and_sends_only_what_changed() {
    let mut rt = runtime("cat-page");
    click(&mut rt, "cat-more");
    click_text(&mut rt, "Edit");
    assert!(has_key(&rt, "cat-sheet-body") && has_text(&rt, "Edit miso"));
    assert_eq!(rt.state.pages.text(F_NAME), "miso");
    assert_eq!(rt.state.pages.text(neko_rune::cats::F_LIMIT), "5.00");
    assert!(has_key(&rt, "cat-workspace-fixed"), "a cat's workspace never changes");
    rt.update(|s, _| s.pages.set_field(neko_rune::cats::F_AREA, "Storefront"));
    click(&mut rt, "cat-sheet-save");
    let sent = asked(&rt);
    let [CatCall::Update { id, name, face, area, model_policy, daily_limit_micro_usd, purpose }] = sent.as_slice() else { panic!("one Update: {sent:?}") };
    assert_eq!(id, "c1");
    assert_eq!(area.as_deref(), Some("Storefront"));
    assert!(name.is_none() && face.is_none() && model_policy.is_none() && daily_limit_micro_usd.is_none() && purpose.is_none(), "the rest is left alone: {sent:?}");
    // Emptying the limit removes it (`Some(None)`), which is not the same as leaving it.
    answer(&mut rt, sent[0].clone(), listing(3));
    assert!(rt.state.cats.sheet.is_none());
    click(&mut rt, "cat-more");
    click_text(&mut rt, "Edit");
    rt.state.outbox.clear();
    rt.update(|s, _| s.pages.set_field(neko_rune::cats::F_LIMIT, ""));
    click(&mut rt, "cat-sheet-save");
    assert!(matches!(asked(&rt).as_slice(), [CatCall::Update { daily_limit_micro_usd: Some(None), .. }]));
}

// ---- the cat page -------------------------------------------------------------------------------

#[test]
fn clicking_a_cat_opens_its_page_with_the_header_and_the_panel() {
    let mut rt = runtime("cats-3");
    click(&mut rt, "cat-c2");
    assert_eq!(rt.state.cats.open.as_deref(), Some("c2"));
    assert!(rt.state.selected.is_none(), "no session is open under the cat's page");
    assert_eq!(info(&rt, "cat-title").and_then(|e| e.text).as_deref(), Some("mochi"));
    assert_eq!(info(&rt, "cat-crumb").and_then(|e| e.text).as_deref(), Some("Billing · Acme api"));
    assert_eq!(info(&rt, "cat-cost").and_then(|e| e.text).as_deref(), Some("$0.00 today"));
    assert!(has_text(&rt, "Talk to mochi here. This comes in the next build."), "one quiet line, no fake messages");
    assert!(rt.state.layout.context.is_open() && has_key(&rt, "cat-panel-head"), "the cat's panel is in the context slot");
}

#[test]
fn the_overview_has_four_sections_each_with_its_own_honest_empty_line() {
    let mut rt = runtime("cats-3");
    click(&mut rt, "cat-c3");
    for (key, line) in [("needs", "Nothing is waiting for you."), ("ready", "Nothing is ready to merge yet."), ("rounds", "No rounds yet.")] {
        assert_eq!(info(&rt, &format!("cat-empty-{key}")).and_then(|e| e.text).as_deref(), Some(line), "{key}");
    }
    // tofu is building something: that section lists it and says nothing false.
    assert!(!has_key(&rt, "cat-empty-working") && has_key(&rt, "cat-task-t7"));
    for title in ["Needs you", "Working", "Ready to merge", "Rounds"] {
        assert!(has_text(&rt, title), "{title}");
    }
    // A cat with nothing running says so.
    let mut rt = runtime("cats-5");
    click(&mut rt, "cat-c5");
    assert_eq!(info(&rt, "cat-empty-working").and_then(|e| e.text).as_deref(), Some("No tasks are running."));
}

#[test]
fn opening_a_session_or_a_view_closes_the_cat_page_and_the_panel_it_opened() {
    let mut rt = runtime("cats-3");
    rt.update(|s, _| s.layout.context.set_open(false));
    click(&mut rt, "cat-c1");
    assert!(rt.state.layout.context.is_open());
    click(&mut rt, "view-inbox");
    assert!(rt.state.cats.open.is_none() && !rt.state.layout.context.is_open(), "Routines opened: the cat page and its panel are gone");
    click(&mut rt, "cat-c1");
    rt.update(|s, _| s.open_new());
    settle(&mut rt);
    assert!(rt.state.cats.open.is_none() && rt.state.home_shown());
}

// ---- the files ----------------------------------------------------------------------------------

#[test]
fn the_files_tab_lists_the_four_files_and_asks_for_them() {
    let mut rt = runtime("cat-page");
    click(&mut rt, "segment1");
    assert!(asked(&rt).contains(&CatCall::Files { id: "c1".into() }));
    for (file, title) in [("RULES.md", "Rules"), ("TASK_RULES.md", "Task rules"), ("PLAN.md", "Plan"), ("BRIEFING.md", "Briefing")] {
        assert!(has_key(&rt, &format!("cat-file-{file}")), "{file}");
        assert!(has_text(&rt, title) && has_text(&rt, file));
    }
}

#[test]
fn a_file_opens_in_the_editor_saves_with_the_button_and_with_command_s() {
    let mut rt = runtime("cat-files");
    click(&mut rt, "cat-file-RULES.md");
    assert!(has_key(&rt, "cat-editor") && has_text(&rt, "Saved"));
    assert!(rt.state.pages.text(F_FILE).starts_with("# Rules"));
    assert!(!rt.state.cat_file_dirty());
    // Typing makes it unsaved; Save sends the text, and the daemon's answer makes it the saved text.
    let edited = format!("{}- Review before merging.\n", rt.state.pages.text(F_FILE));
    let e = edited.clone();
    rt.update(move |s, _| s.pages.set_field(F_FILE, &e));
    settle(&mut rt);
    assert!(has_text(&rt, "Unsaved changes"));
    click(&mut rt, "cat-editor-save");
    assert_eq!(asked(&rt), [CatCall::WriteFile { id: "c1".into(), kind: CatFileKind::Rules, text: edited.clone() }]);
    let call = asked(&rt)[0].clone();
    answer(&mut rt, call, CatsData::Files(files_of(&edited)));
    assert!(!rt.state.cat_file_dirty() && has_text(&rt, "Saved"), "the daemon's answer is what is saved");
    // ⌘S saves too.
    let again = format!("{edited}- One more.\n");
    let a = again.clone();
    rt.update(move |s, _| s.pages.set_field(F_FILE, &a));
    rt.state.outbox.clear();
    rt.key(KeyEvent { key: Key::Command('s'), mods: Mods { cmd: true, ..Mods::default() } });
    settle(&mut rt);
    assert_eq!(asked(&rt), [CatCall::WriteFile { id: "c1".into(), kind: CatFileKind::Rules, text: again }]);
}

#[test]
fn revert_goes_back_to_the_saved_text() {
    let mut rt = runtime("cat-editor-dirty");
    assert!(rt.state.cat_file_dirty());
    click(&mut rt, "cat-editor-revert");
    assert!(!rt.state.cat_file_dirty() && !rt.state.pages.text(F_FILE).contains("Friday"));
}

#[test]
fn leaving_an_unsaved_file_asks_first_and_keep_editing_keeps_the_words() {
    let mut rt = runtime("cat-editor-dirty");
    let words = rt.state.pages.text(F_FILE).to_owned();
    for leave in ["cat-file-back", "segment0"] {
        click(&mut rt, leave);
        assert!(has_key(&rt, "guard-discard") && has_text(&rt, "This file has changes that are not saved"), "{leave}: asked first");
        click(&mut rt, "guard-keep");
        assert!(!has_key(&rt, "guard-discard"));
        assert_eq!(rt.state.pages.text(F_FILE), words, "{leave}: the words are still there");
        assert!(has_key(&rt, "cat-editor"));
    }
    // Another cat, and a session in the sidebar, are leaving too.
    click(&mut rt, "cat-c2");
    assert!(has_key(&rt, "guard-discard") && rt.state.cats.open.as_deref() == Some("c1"));
    click(&mut rt, "guard-keep");
    // Discard: the words go back to what was saved and the person goes where they were going.
    click(&mut rt, "cat-file-back");
    click(&mut rt, "guard-discard");
    assert!(!has_key(&rt, "cat-editor") && has_key(&rt, "cat-files"), "back at the list of files");
    assert!(!rt.state.cat_file_dirty());
    // A clean editor leaves without a question.
    click(&mut rt, "cat-file-RULES.md");
    click(&mut rt, "cat-c2");
    assert_eq!(rt.state.cats.open.as_deref(), Some("c2"));
}

#[test]
fn a_file_over_sixteen_kilobytes_is_not_sent_and_the_daemons_own_error_shows() {
    let mut rt = runtime("cat-editor");
    let big = "x".repeat(16 * 1024 + 1);
    rt.update(move |s, _| s.pages.set_field(F_FILE, &big));
    settle(&mut rt);
    assert!(has_key(&rt, "cat-editor-over") && has_text(&rt, "Keep it under 16 KB"));
    click(&mut rt, "cat-editor-save");
    assert!(asked(&rt).is_empty(), "too big to ask: {:?}", asked(&rt));
    // A refusal from the daemon is shown as the daemon said it, and the text stays.
    rt.update(|s, _| s.pages.set_field(F_FILE, "# Rules\n\nnew\n"));
    settle(&mut rt);
    click(&mut rt, "cat-editor-save");
    let call = asked(&rt).remove(0);
    refuse(&mut rt, call, "This cat is archived");
    assert_eq!(info(&rt, "cat-editor-error").and_then(|e| e.text).as_deref(), Some("This cat is archived"));
    assert_eq!(rt.state.pages.text(F_FILE), "# Rules\n\nnew\n");
    assert!(rt.state.cat_file_dirty());
}

#[test]
fn clearing_a_file_asks_before_it_sends_empty_text() {
    let mut rt = runtime("cat-editor");
    rt.update(|s, _| s.pages.set_field(F_FILE, "  \n"));
    settle(&mut rt);
    click(&mut rt, "cat-editor-save");
    assert!(asked(&rt).is_empty(), "nothing is sent until the question is answered");
    assert!(has_key(&rt, "ask-yes") && has_text(&rt, "Clear Rules?"));
    click(&mut rt, "ask-yes");
    assert_eq!(asked(&rt), [CatCall::WriteFile { id: "c1".into(), kind: CatFileKind::Rules, text: String::new() }]);
}

// ---- archive, pin, reorder ----------------------------------------------------------------------

#[test]
fn archive_asks_first_in_its_own_words_and_sends_only_on_the_coral_button() {
    let mut rt = runtime("cat-page");
    click(&mut rt, "cat-more");
    click_text(&mut rt, "Archive");
    assert!(asked(&rt).is_empty(), "nothing is sent by the menu item");
    assert!(has_text(&rt, "Archive miso?") && has_text(&rt, "Its tasks and files are kept."));
    assert!(rt.state.dialog.as_ref().is_some_and(|d| d.destructive), "the yes button is coral");
    click(&mut rt, "ask-no");
    assert!(asked(&rt).is_empty() && rt.state.dialog.is_none());
    click(&mut rt, "cat-more");
    click_text(&mut rt, "Archive");
    click(&mut rt, "ask-yes");
    assert_eq!(asked(&rt), [CatCall::Archive { id: "c1".into() }]);
    // The daemon answers with the list without miso: the page goes back to Home.
    let mut rest = demo_cats(3);
    rest.remove(0);
    let call = asked(&rt)[0].clone();
    answer(&mut rt, call, CatsData::Listing(rest));
    assert!(rt.state.cats.open.is_none() && !has_key(&rt, "cat-c1"), "miso is gone from the sidebar and its page is closed");
}

#[test]
fn pin_and_unpin_send_the_request_from_the_menu() {
    let mut rt = runtime("cat-page");
    click(&mut rt, "cat-more");
    click_text(&mut rt, "Unpin");
    assert_eq!(asked(&rt), [CatCall::Pin { id: "c1".into(), pinned: false }]);
    let mut rt = runtime("cats-5");
    // A right-press on a row opens the same menu as the page's: Pin for an unpinned cat.
    let (x, y) = rt.find("cat-c4").unwrap();
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    settle(&mut rt);
    click_text(&mut rt, "Pin");
    assert_eq!(asked(&rt), [CatCall::Pin { id: "c4".into(), pinned: true }]);
}

/// Takes the element keyed `key`, carries it by `(dx, dy)` in a few steps as a hand moves, and lets go.
fn drag(rt: &mut Runtime<App>, key: &str, dx: f32, dy: f32) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    for i in 1..=6 {
        rt.pointer_move(x + dx * i as f32 / 6.0, y + dy * i as f32 / 6.0);
        rt.frame(1.0 / 60.0);
    }
    rt.pointer_up(x + dx, y + dy);
    settle(rt);
}

#[test]
fn dragging_a_row_sends_the_new_order_of_every_cat() {
    let mut rt = runtime("cats-5");
    // kuro (the second row) up one place: above nori.
    drag(&mut rt, "cat-c5", 0.0, -36.0);
    assert_eq!(asked(&rt), [CatCall::Reorder { ids: vec!["c1".into(), "c2".into(), "c3".into(), "c5".into(), "c4".into()] }]);
    assert!(rt.state.cats.open.is_none(), "the press that ends a drag is not a click");
    // Dropped where it started: nothing is asked.
    rt.state.outbox.clear();
    drag(&mut rt, "cat-c5", 0.0, 4.0);
    assert!(asked(&rt).is_empty());
}

#[test]
fn dragging_a_face_along_the_row_reorders_the_pinned_cats() {
    let mut rt = runtime("cats-5");
    let cell = info(&rt, "cat-c2").unwrap().rect[0] - info(&rt, "cat-c1").unwrap().rect[0];
    drag(&mut rt, "cat-c1", cell, 0.0);
    assert_eq!(asked(&rt), [CatCall::Reorder { ids: vec!["c2".into(), "c1".into(), "c3".into(), "c4".into(), "c5".into()] }]);
}

#[test]
fn a_press_on_a_cat_without_a_drag_is_a_click_and_sends_no_order() {
    let mut rt = runtime("cats-5");
    click(&mut rt, "cat-c4");
    assert!(asked(&rt).is_empty() && rt.state.cats.open.as_deref() == Some("c4"));
}

#[test]
fn the_row_menu_has_move_up_and_down_for_those_who_do_not_drag() {
    let mut rt = runtime("cats-5");
    let (x, y) = rt.find("cat-c5").unwrap();
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    settle(&mut rt);
    click_text(&mut rt, "Move up");
    assert_eq!(asked(&rt), [CatCall::Reorder { ids: vec!["c1".into(), "c2".into(), "c3".into(), "c5".into(), "c4".into()] }]);
}

#[test]
fn a_cat_that_needs_you_says_so_to_a_screen_reader() {
    let rt = runtime("cats-3");
    let label = |key: &str| info(&rt, key).and_then(|e| e.label).unwrap_or_default();
    assert_eq!(label("cat-c2"), "mochi, Billing, needs you");
    assert_eq!(label("cat-c1"), "miso, Web, working");
}
