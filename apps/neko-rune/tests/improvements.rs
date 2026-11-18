//! Setup, Improvements, driven through Rune's headless `Runtime` over made-up data (`scenes_setup.rs`): the web's sections in order,
//! the row's pieces and its buttons by state, what Apply, Reject and Undo ask of the daemon (the `Improve` command, whole listing back),
//! the confirmation Undo goes through when the daemon says it is hard to take back, a pack suggestion that opens that pack's preview
//! and the detail dialog. Nothing real is applied: the answers are delivered by hand, as the daemon would.

use neko_protocol::improve::{ImproveReply, ImproveState};
use neko_rune::app::App;
use neko_rune::link::{Call, Reply, SetupAction};
use neko_rune::scenes;
use neko_rune::view::setup::{Dialog, SetupSection};
use neko_rune::view::view;
use rune::{Env, Runtime};

const WINDOW: (f32, f32) = (1360.0, 1000.0);

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

fn has_text(rt: &Runtime<App>, needle: &str) -> bool {
    texts(rt).iter().any(|t| t == needle)
}

fn text_of(rt: &Runtime<App>, key: &str) -> Option<String> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text)
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}")).rect
}

fn scene(name: &str) -> Runtime<App> {
    let mut app = App::headless();
    let (_, f) = scenes::SCENES.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no scene {name}"));
    f(&mut app);
    app.outbox.clear();
    let mut rt = Runtime::new(app, view, WINDOW, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn sent(rt: &Runtime<App>) -> Vec<SetupAction> {
    rt.state.outbox.iter().filter_map(|c| if let Call::Setup(s) = c { Some((**s).clone()) } else { None }).collect()
}

fn answer(rt: &mut Runtime<App>, call: &SetupAction, reply: ImproveReply) {
    let call = Call::Setup(Box::new(call.clone()));
    rt.update(move |s, _| {
        s.apply_reply(&call, Reply::Improve(Box::new(reply)));
    });
    settle(rt);
}

fn listing(rt: &Runtime<App>) -> ImproveReply {
    rt.state.improvements().cloned().expect("the listing")
}

#[test]
fn improvements_sits_after_packs_in_the_rail_as_in_the_webs_setup_nav_and_asks_for_the_listing() {
    assert_eq!(SetupSection::ALL[0], SetupSection::Packs);
    assert_eq!(SetupSection::ALL[1], SetupSection::Improvements);
    assert_eq!(SetupSection::Improvements.title(), "Improvements");
    let mut rt = scene("setup-workspaces");
    let (packs, improvements, workspaces) = (rt.find("setup-section-packs").unwrap().1, rt.find("setup-section-improvements").expect("the row").1, rt.find("setup-section-workspaces").unwrap().1);
    assert!(packs < improvements && improvements < workspaces);
    click(&mut rt, "setup-section-improvements");
    assert_eq!(rt.state.setup.section, SetupSection::Improvements);
    assert!(sent(&rt).iter().any(|a| matches!(a, SetupAction::ImproveList)), "{:?}", sent(&rt));
}

#[test]
fn the_list_is_the_webs_title_count_and_suggestions_section_with_a_row_per_proposal() {
    let rt = scene("setup-improvements");
    assert_eq!(text_of(&rt, "page-count").as_deref(), Some("6"));
    assert!(has_text(&rt, "Suggestions") && text_of(&rt, "improve-off").is_none() && rt.find("improve-off").is_none(), "suggestions are on: no off line");
    // Newest first as the daemon sends them, each row the web's 44 high (the 32 point buttons and 6 above and below).
    let ys: Vec<f32> = ["i1", "i2", "i3", "i4", "i5", "i6"].iter().map(|id| rect(&rt, &format!("improve-block-{id}"))[1]).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");
    for id in ["i1", "i2", "i3", "i4", "i5", "i6"] {
        assert!((rect(&rt, &format!("improve-block-{id}"))[3] - 44.0).abs() < 0.1, "{id}");
    }
    // The pieces of a row: kind, then what became of it (an open suggestion needs none), then how old.
    assert_eq!(text_of(&rt, "improve-kind-i1").as_deref(), Some("Improve a skill"));
    assert!(rt.find("improve-word-i1").is_none());
    assert_eq!(text_of(&rt, "improve-word-i2").as_deref(), Some("Applied"));
    assert_eq!(text_of(&rt, "improve-word-i3").as_deref(), Some("Rejected"));
    assert_eq!(text_of(&rt, "improve-word-i4").as_deref(), Some("Helped"));
    assert_eq!(text_of(&rt, "improve-word-i5").as_deref(), Some("Didn’t help"));
    assert_eq!(text_of(&rt, "improve-word-i6").as_deref(), Some("Waiting in Skills"));
    assert_eq!(text_of(&rt, "improve-age-i1").as_deref(), Some("yesterday"));
    // Buttons by state: Apply and Reject while open, Undo where the daemon says it can be undone, evidence on every row.
    assert!(rt.find("improve-apply-i1").is_some() && rt.find("improve-reject-i1").is_some() && rt.find("improve-undo-i1").is_none());
    for id in ["i2", "i4", "i5"] {
        assert!(rt.find(&format!("improve-undo-{id}")).is_some() && rt.find(&format!("improve-apply-{id}")).is_none(), "{id}");
    }
    for id in ["i3", "i6"] {
        assert!(rt.find(&format!("improve-undo-{id}")).is_none() && rt.find(&format!("improve-apply-{id}")).is_none(), "{id}");
    }
    for id in ["i1", "i2", "i3", "i4", "i5", "i6"] {
        assert!(rt.find(&format!("improve-evidence-{id}")).is_some(), "{id}");
    }
}

#[test]
fn apply_and_reject_send_the_improve_commands_and_never_ask() {
    let mut rt = scene("setup-improvements");
    click(&mut rt, "improve-reject-i1");
    assert_eq!(sent(&rt), vec![SetupAction::ImproveReject { id: "i1".into() }]);
    assert!(rt.find("confirm-yes").is_none(), "Reject never asks");
    // A second press waits for the answer.
    click(&mut rt, "improve-apply-i1");
    assert_eq!(sent(&rt).len(), 1, "busy: no second request");
    // The answer is the whole listing; the row now says Rejected and its buttons are gone.
    let mut next = listing(&rt);
    next.proposals[0].state = ImproveState::Rejected;
    next.note = Some("Rejected. It will not be suggested again for 30 days.".into());
    answer(&mut rt, &SetupAction::ImproveReject { id: "i1".into() }, next);
    assert_eq!(text_of(&rt, "improve-word-i1").as_deref(), Some("Rejected"));
    assert!(rt.find("improve-apply-i1").is_none());
    assert_eq!(text_of(&rt, "improve-note").as_deref(), Some("Rejected. It will not be suggested again for 30 days."));
    // And a fresh Apply on another open suggestion goes out.
    rt.update(|s, _| s.outbox.clear());
    let mut again = listing(&rt);
    again.proposals[2].state = ImproveState::Open;
    answer(&mut rt, &SetupAction::ImproveList, again);
    click(&mut rt, "improve-apply-i3");
    assert_eq!(sent(&rt), vec![SetupAction::ImproveApply { id: "i3".into() }]);
}

#[test]
fn undo_sends_at_once_when_it_is_easy_and_asks_first_when_the_daemon_says_it_is_not() {
    let mut rt = scene("setup-improvements");
    // i2 is a standing order: reversing it is trivial, so no question.
    click(&mut rt, "improve-undo-i2");
    assert_eq!(sent(&rt), vec![SetupAction::ImproveUndo { id: "i2".into() }]);
    assert!(rt.find("confirm-yes").is_none());
    let next = listing(&rt);
    answer(&mut rt, &SetupAction::ImproveUndo { id: "i2".into() }, next);
    rt.update(|s, _| s.outbox.clear());
    // i4 is a pack: `undo_confirms`, so the coral dialog says what is removed, and nothing is sent before yes.
    click(&mut rt, "improve-undo-i4");
    assert!(rt.find("confirm-yes").is_some() && has_text(&rt, "Undo “Install the issue tracker pack”?"), "{:?}", texts(&rt));
    assert!(has_text(&rt, "The pack is removed, with its connections, skills, plugin and routines. A copy is kept in the cleanup archive."));
    assert!(sent(&rt).is_empty(), "nothing before yes: {:?}", sent(&rt));
    click(&mut rt, "confirm-no");
    assert!(sent(&rt).is_empty() && rt.find("confirm-yes").is_none());
    click(&mut rt, "improve-undo-i4");
    click(&mut rt, "confirm-yes");
    assert_eq!(sent(&rt), vec![SetupAction::ImproveUndo { id: "i4".into() }]);
}

#[test]
fn applying_a_pack_suggestion_opens_that_packs_preview_on_the_packs_page_and_installs_nothing() {
    let mut rt = scene("setup-improvements");
    click(&mut rt, "improve-apply-i1");
    rt.update(|s, _| s.outbox.clear());
    let mut next = listing(&rt);
    next.open_pack = Some("catalog:issue-tracker".into());
    answer(&mut rt, &SetupAction::ImproveApply { id: "i1".into() }, next);
    assert_eq!(rt.state.setup.section, SetupSection::Packs, "the preview opens on the Packs page");
    assert!(matches!(rt.state.setup.dialog, Some(Dialog::Pack { .. })));
    let calls = sent(&rt);
    assert!(calls.iter().any(|c| matches!(c, SetupAction::PackPreview { source } if source == "catalog:issue-tracker")), "{calls:?}");
    assert!(!calls.iter().any(|c| matches!(c, SetupAction::PackInstall { .. })), "nothing installs by itself: {calls:?}");
}

#[test]
fn a_row_opens_its_detail_with_why_the_change_the_evidence_and_the_history() {
    let mut rt = scene("setup-improvements");
    click(&mut rt, "improve-i1");
    assert!(matches!(rt.state.setup.dialog, Some(Dialog::Improve { ref id, .. }) if id == "i1"), "the detail dialog");
    // The default button carries the daemon's own words; Reject is beside it; a skill patch shows its diff, line by line.
    assert!(has_text(&rt, "Improve the reviewer skill") && rt.find("improve-d-reject").is_some());
    for heading in ["Why", "The change", "Evidence"] {
        assert!(has_text(&rt, heading), "{heading}");
    }
    assert!(rt.find("improve-diff").is_some() && text_of(&rt, "improve-diff-4").as_deref() == Some("+Put the ticket link in the first line."));
    assert!(!has_text(&rt, "History"), "no history yet");
    click(&mut rt, "improve-d-reject");
    assert_eq!(sent(&rt), vec![SetupAction::ImproveReject { id: "i1".into() }]);
    // Once applied: the counts, Undo, and the history.
    let mut rt = scene("setup-improvement-detail-pack");
    assert_eq!(text_of(&rt, "improve-counts").as_deref(), Some("Fired 3 times in the 7 days before, 0 since."));
    assert!(has_text(&rt, "History") && rt.find("improve-history-1").is_some());
    assert!(has_text(&rt, " pack. Nothing installs by itself; you read what it would do and install it there."), "a pack's change is the preview's words: {:?}", texts(&rt));
    click(&mut rt, "improve-d-undo");
    assert!(rt.find("confirm-yes").is_some(), "the pack's undo asks first, here too");
}

#[test]
fn the_off_and_empty_states_say_so_and_the_switch_turns_suggestions_on() {
    let mut rt = scene("setup-improvements-off");
    assert!(has_text(&rt, "Suggestions are off. Neko is not looking for patterns."));
    click(&mut rt, "improve-enable");
    assert_eq!(sent(&rt), vec![SetupAction::ImproveEnable]);
    let rt = scene("setup-improvements-empty");
    assert!(has_text(&rt, "Neko hasn't suggested anything yet.") && rt.find("improve-suggestions").is_none());
    assert!(text_of(&rt, "page-count").is_none(), "no count at 0");
}
