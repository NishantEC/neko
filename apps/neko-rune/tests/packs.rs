//! Setup, Packs, driven through Rune's headless `Runtime` over made-up data (`scenes_setup.rs`): the list and its glyphs, the detail,
//! the Add pack dialog (catalog, folder or URL, preview, install), update and review with their digests, remove behind its
//! confirmation, and what each control asks of the daemon. Nothing real is installed or run: the answers are delivered by hand,
//! as the daemon would, through `App::apply_reply`.

use neko_protocol::packs::{PackListing, PackPreview};
use neko_rune::app::App;
use neko_rune::link::{Call, Reply, SetupAction};
use neko_rune::scenes;
use neko_rune::view::setup::{Dialog, PackIntent, PackTab, SetupSection};
use neko_rune::view::view;
use neko_rune::views::FullView;
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
    texts(rt).iter().any(|t| t.contains(needle))
}

fn text_of(rt: &Runtime<App>, key: &str) -> Option<String> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text)
}

fn label_of(rt: &Runtime<App>, key: &str) -> Option<String> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.label)
}

fn fill(rt: &mut Runtime<App>, key: &str, text: &str) {
    let (key, text) = (key.to_owned(), text.to_owned());
    rt.update(move |s, _| s.pages.set_field(&key, &text));
    settle(rt);
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

fn forget(rt: &mut Runtime<App>) {
    rt.state.outbox.clear();
}

/// The daemon's answer to a request, delivered as the link would.
fn answer(rt: &mut Runtime<App>, call: &SetupAction, reply: Reply) {
    let call = Call::Setup(Box::new(call.clone()));
    rt.update(move |s, _| {
        s.apply_reply(&call, reply);
    });
    settle(rt);
}

fn listing(rt: &Runtime<App>) -> PackListing {
    rt.state.packs().cloned().expect("the listing")
}

// ---- the list

#[test]
fn packs_is_the_first_section_of_the_rail_and_opening_it_asks_for_the_listing() {
    assert_eq!(SetupSection::ALL[0], SetupSection::Packs);
    assert_eq!(SetupSection::ALL[0].title(), "Packs");
    let mut rt = scene("setup-workspaces");
    let packs = rt.find("setup-section-packs").expect("the Packs row").1;
    let workspaces = rt.find("setup-section-workspaces").expect("the Workspaces row").1;
    assert!(packs < workspaces, "Packs is above Workspaces");
    click(&mut rt, "setup-section-packs");
    assert_eq!(rt.state.setup.section, SetupSection::Packs);
    assert!(sent(&rt).iter().any(|a| matches!(a, SetupAction::PackList)), "{:?}", sent(&rt));
}

#[test]
fn the_list_has_a_row_per_pack_with_its_glyph_version_and_source() {
    let rt = scene("setup-packs");
    assert_eq!(text_of(&rt, "page-count").as_deref(), Some("4"));
    // Health, needs setup, failing and changed: a mint check, two amber half circles and a coral cross.
    let keys: Vec<String> = rt.elements().into_iter().filter_map(|e| e.key).collect();
    for glyph in ["status-Done", "status-InProgress", "status-Failed"] {
        assert!(keys.iter().any(|k| k == glyph), "{glyph}");
    }
    assert!(label_of(&rt, "pack-demo-notes").unwrap().contains("Healthy"));
    assert!(label_of(&rt, "pack-demo-board").unwrap().contains("Needs setup"));
    assert!(label_of(&rt, "pack-demo-feed").unwrap().contains("Failing"));
    assert!(label_of(&rt, "pack-demo-sketch").unwrap().contains("Changed on disk"));
    assert_eq!(text_of(&rt, "pack-version-demo-notes").as_deref(), Some("1.2.0"));
    assert_eq!(text_of(&rt, "pack-source-demo-notes").as_deref(), Some("catalog"));
    assert_eq!(text_of(&rt, "pack-source-demo-board").as_deref(), Some("/work/packs/demo-board"));
    // Check health on every row; Update only where a newer version is known; Review only where the files changed.
    for name in ["demo-notes", "demo-board", "demo-feed", "demo-sketch"] {
        assert!(rt.find(&format!("pack-health-{name}")).is_some() && rt.find(&format!("pack-remove-{name}")).is_some(), "{name}");
    }
    assert!(rt.find("pack-update-demo-notes").is_some());
    assert_eq!(label_of(&rt, "pack-update-demo-notes").as_deref(), Some("Update to 1.3.0"));
    assert!(rt.find("pack-update-demo-board").is_none() && rt.find("pack-update-demo-feed").is_none());
    assert!(rt.find("pack-review-demo-sketch").is_some() && rt.find("pack-review-demo-notes").is_none());
}

#[test]
fn an_empty_list_is_one_quiet_line_and_the_add_button_and_suggests_nothing() {
    let rt = scene("setup-packs-empty");
    assert!(has_text(&rt, "No packs installed."));
    assert!(rt.find("packs-add").is_some());
    assert!(text_of(&rt, "page-count").is_none(), "no count at 0");
    assert!(!has_text(&rt, "Demo clock") && !has_text(&rt, "Recommended"), "nothing is suggested: {:?}", texts(&rt));
}

#[test]
fn a_row_action_runs_only_that_check_and_a_second_press_waits_for_the_first() {
    let mut rt = scene("setup-packs");
    click(&mut rt, "pack-health-demo-notes");
    assert_eq!(sent(&rt), vec![SetupAction::PackHealth { name: "demo-notes".into() }]);
    click(&mut rt, "pack-health-demo-notes");
    assert_eq!(sent(&rt).len(), 1, "busy: no second request");
    assert!(rt.find("pack-busy-demo-notes").is_some());
    // The answer carries the listing and frees the row.
    let mut l = listing(&rt);
    l.installed[0].health.as_mut().unwrap().output = "checked again".into();
    answer(&mut rt, &SetupAction::PackHealth { name: "demo-notes".into() }, Reply::Packs(l));
    assert!(rt.find("pack-busy-demo-notes").is_none());
}

#[test]
fn a_refused_check_says_so_beside_its_row() {
    let mut rt = scene("setup-packs");
    click(&mut rt, "pack-health-demo-board");
    let call = Call::Setup(Box::new(SetupAction::PackHealth { name: "demo-board".into() }));
    rt.update(move |s, _| {
        s.apply_reply(&call, Reply::Refused("No pack is installed as demo-board.".into()));
    });
    settle(&mut rt);
    assert!(has_text(&rt, "No pack is installed as demo-board."));
    assert!(rt.find("pack-busy-demo-board").is_none(), "its button is back");
}

// ---- the detail

#[test]
fn a_row_opens_a_detail_with_permissions_contents_requirements_and_the_last_check() {
    let mut rt = scene("setup-packs");
    click(&mut rt, "pack-demo-board");
    assert!(rt.find("pack-back").is_some());
    assert!(has_text(&rt, "Reads the notes folder you point it at") && has_text(&rt, "Starts the tool server on this Mac"));
    // Contents with a link to each section that shows them.
    assert!(rt.find("pack-c-connections-go").is_some() && rt.find("pack-c-plugin-go").is_some() && rt.find("pack-c-luau").is_some());
    assert!(has_text(&rt, "layout.luau"));
    // A missing requirement shows its hint in monospace with a Copy button; a found one its path.
    assert_eq!(text_of(&rt, "pack-req-state-0").as_deref(), Some("Missing"));
    assert_eq!(text_of(&rt, "pack-req-state-1").as_deref(), Some("Found"));
    assert_eq!(text_of(&rt, "pack-req-hint-text-0").as_deref(), Some("brew install demo-cli"));
    assert!(rt.find("pack-req-copy-0").is_some());
    // The last health output and when it ran.
    assert_eq!(text_of(&rt, "pack-health-output").as_deref(), Some("demo-cli: command not found"));
    assert!(text_of(&rt, "pack-health-when").unwrap().starts_with("Failed ·"));
    click(&mut rt, "pack-back");
    assert!(rt.find("pack-back").is_none() && rt.find("pack-demo-board").is_some());
}

#[test]
fn the_detail_links_go_to_the_sections_that_show_each_part() {
    let mut rt = scene("setup-packs");
    click(&mut rt, "pack-demo-notes");
    click(&mut rt, "pack-c-connections-go");
    assert_eq!(rt.state.setup.section, SetupSection::Tools);
    click(&mut rt, "setup-section-packs");
    click(&mut rt, "pack-demo-notes");
    click(&mut rt, "pack-c-skills-go");
    assert_eq!(rt.state.setup.section, SetupSection::Skills);
    click(&mut rt, "setup-section-packs");
    click(&mut rt, "pack-demo-notes");
    click(&mut rt, "pack-c-routines-go");
    assert_eq!(rt.state.view, Some(FullView::Routines));
    let mut rt = scene("setup-packs");
    click(&mut rt, "pack-demo-board");
    click(&mut rt, "pack-c-plugin-go");
    assert_eq!(rt.state.view, Some(FullView::Plugins));
}

#[test]
fn a_changed_pack_says_nothing_runs_and_offers_review_and_the_plugin_switch_and_the_clean_up() {
    let mut rt = scene("setup-pack-changed");
    assert!(has_text(&rt, "Nothing of it runs until you review them."));
    assert!(rt.find("pack-review").is_some());
    assert_eq!(label_of(&rt, "pack-plugin-switch").as_deref(), Some("Enable plugin"));
    assert!(has_text(&rt, "A folder from the last update could not be deleted."));
    click(&mut rt, "pack-cleanup");
    assert_eq!(sent(&rt), vec![SetupAction::PackCleanUp { name: "demo-sketch".into() }]);
    // The plugin switch uses the Plugins page's path (HTTP), so it asks nothing of the socket.
    forget(&mut rt);
    click(&mut rt, "pack-plugin-switch");
    assert!(sent(&rt).is_empty());
    assert_eq!(rt.state.plugin_ui.busy.as_deref(), Some("demo-sketch"), "the plugin id is the last part of its folder");
}

#[test]
fn an_enabled_plugin_offers_to_be_switched_off() {
    let mut rt = scene("setup-pack-changed");
    rt.update(|s, _| {
        let mut l = s.packs().cloned().unwrap();
        l.installed[3].plugin_disabled = false;
        s.setup.packs = Some(l);
    });
    settle(&mut rt);
    assert_eq!(label_of(&rt, "pack-plugin-switch").as_deref(), Some("Disable plugin"));
}

// ---- review

#[test]
fn review_reads_the_installed_copy_and_approve_sends_exactly_its_digest() {
    let mut rt = scene("setup-pack-changed");
    click(&mut rt, "pack-review");
    let look = SetupAction::PackPreview { source: "installed:demo-sketch".into() };
    assert_eq!(sent(&rt), vec![look.clone()]);
    assert!(has_text(&rt, "Reading the files on disk…"));
    let mut preview = neko_rune::scenes_setup::demo_update_preview();
    preview.info.title = "Demo sketch".into();
    preview.digest = "digest-sketch-7".into();
    preview.needs_approval = false;
    preview.changes = vec!["Its plugin changed.".into()];
    answer(&mut rt, &look, Reply::PackPreview(preview));
    assert!(has_text(&rt, "Review Demo sketch") && has_text(&rt, "Its plugin changed."));
    forget(&mut rt);
    click(&mut rt, "dlg-pack-install");
    assert_eq!(sent(&rt), vec![SetupAction::PackApprove { name: "demo-sketch".into(), digest: "digest-sketch-7".into() }]);
    // Only its own answer closes it.
    assert!(matches!(rt.state.setup.dialog, Some(Dialog::Pack { .. })));
    let now = listing(&rt);
    answer(&mut rt, &SetupAction::PackApprove { name: "demo-sketch".into(), digest: "digest-sketch-7".into() }, Reply::Packs(now));
    assert!(rt.state.setup.dialog.is_none());
}

// ---- update

#[test]
fn update_previews_the_source_first_and_a_power_change_needs_its_own_approval() {
    let mut rt = scene("setup-packs");
    click(&mut rt, "pack-update-demo-notes");
    let look = SetupAction::PackPreview { source: "catalog:demo-notes".into() };
    assert_eq!(sent(&rt), vec![look.clone()], "nothing is updated by looking");
    assert!(matches!(rt.state.setup.dialog, Some(Dialog::Pack { intent: PackIntent::Update { .. }, preview: None, .. })));
    answer(&mut rt, &look, Reply::PackPreview(neko_rune::scenes_setup::demo_update_preview()));
    assert!(has_text(&rt, "Update needs approval"), "{:?}", texts(&rt));
    assert!(has_text(&rt, "Its tool server now runs: bash -c demo-cli serve"));
    assert!(has_text(&rt, "Runs a script interpreter: bash"), "the warning is shown");
    assert!(has_text(&rt, "Approve and update"));
    forget(&mut rt);
    click(&mut rt, "dlg-pack-install");
    assert_eq!(sent(&rt), vec![SetupAction::PackUpdate { name: "demo-notes".into(), digest: Some("digest-notes-2".into()) }]);
}

#[test]
fn an_update_that_changes_nothing_of_power_is_a_plain_confirm() {
    let mut rt = scene("setup-packs");
    click(&mut rt, "pack-update-demo-notes");
    let mut p = neko_rune::scenes_setup::demo_update_preview();
    p.needs_approval = false;
    p.changes.clear();
    p.info.warnings.clear();
    answer(&mut rt, &SetupAction::PackPreview { source: "catalog:demo-notes".into() }, Reply::PackPreview(p));
    assert!(!has_text(&rt, "Update needs approval") && !has_text(&rt, "Approve and update"));
    assert_eq!(label_of(&rt, "dlg-pack-install").as_deref(), Some("Update"));
    click(&mut rt, "dlg-pack-cancel");
    assert!(rt.state.setup.dialog.is_none());
    assert!(sent(&rt).iter().all(|a| matches!(a, SetupAction::PackPreview { .. })), "Cancel sends nothing more: {:?}", sent(&rt));
}

// ---- remove

#[test]
fn remove_asks_first_cancel_sends_nothing_and_yes_sends_exactly_remove() {
    let mut rt = scene("setup-packs");
    click(&mut rt, "pack-remove-demo-feed");
    assert!(rt.find("confirm-yes").is_some());
    assert!(sent(&rt).is_empty(), "nothing is sent before the yes: {:?}", sent(&rt));
    assert!(has_text(&rt, "Remove pack") && has_text(&rt, "Remove “Demo feed”?"));
    click(&mut rt, "confirm-no");
    assert!(rt.find("confirm-yes").is_none() && sent(&rt).is_empty(), "Cancel sends nothing");
    click(&mut rt, "pack-remove-demo-feed");
    click(&mut rt, "confirm-yes");
    let remove = SetupAction::PackRemove { name: "demo-feed".into() };
    assert_eq!(sent(&rt), vec![remove.clone()]);
    // The dialog waits for the answer; a refusal shows in it and gives the button back.
    assert!(rt.find("confirm-yes").is_some());
    let call = Call::Setup(Box::new(remove.clone()));
    rt.update(move |s, _| {
        s.apply_reply(&call, Reply::Refused("Could not remove Demo feed: a folder is in use.".into()));
    });
    settle(&mut rt);
    assert!(has_text(&rt, "Could not remove Demo feed: a folder is in use."));
    assert!(rt.state.setup.awaiting.is_none());
    // Its own answer closes it and the list no longer has the pack.
    click(&mut rt, "confirm-yes");
    let mut l = listing(&rt);
    l.installed.retain(|p| p.info.name != "demo-feed");
    answer(&mut rt, &remove, Reply::Packs(l));
    assert!(rt.find("confirm-yes").is_none());
    assert!(rt.find("pack-demo-feed").is_none() && rt.find("pack-demo-notes").is_some());
    assert_eq!(text_of(&rt, "page-count").as_deref(), Some("3"));
}

// ---- Add pack

#[test]
fn the_add_dialog_lists_the_catalog_plainly_and_marks_what_is_installed() {
    let mut rt = scene("setup-packs");
    click(&mut rt, "packs-add");
    assert!(sent(&rt).iter().any(|a| matches!(a, SetupAction::PackList)), "the catalog is read again on open");
    assert!(has_text(&rt, "Add pack") && has_text(&rt, "Demo clock") && has_text(&rt, "Adds a world clock tool."));
    assert!(rt.find("dlg-pack-add-demo-clock").is_some());
    assert!(rt.find("dlg-pack-add-demo-notes").is_none() && has_text(&rt, "Installed"));
    assert!(!has_text(&rt, "Recommended") && !has_text(&rt, "Popular") && !has_text(&rt, "Suggested"));
}

#[test]
fn an_empty_catalog_is_one_quiet_line() {
    let mut rt = scene("setup-packs-empty");
    click(&mut rt, "packs-add");
    assert!(has_text(&rt, "No bundled packs."));
}

#[test]
fn from_folder_or_url_previews_what_was_typed_and_needs_something_typed() {
    let mut rt = scene("setup-pack-add");
    rt.update(|s, _| s.pack_dialog_tab(PackTab::Source));
    settle(&mut rt);
    // Nothing typed: Enter says what is missing and sends nothing.
    rt.update(|s, _| s.pack_preview_typed());
    settle(&mut rt);
    assert!(sent(&rt).is_empty() && has_text(&rt, "Enter a folder path or a repository address."));
    fill(&mut rt, "pack:source", "  /work/packs/demo-board  ");
    click(&mut rt, "dlg-pack-preview");
    assert_eq!(sent(&rt), vec![SetupAction::PackPreview { source: "/work/packs/demo-board".into() }]);
}

#[test]
fn previewing_changes_nothing_and_shows_what_the_pack_would_do() {
    let mut rt = scene("setup-pack-add");
    click(&mut rt, "dlg-pack-add-demo-clock");
    let look = SetupAction::PackPreview { source: "catalog:demo-clock".into() };
    assert_eq!(sent(&rt), vec![look.clone()]);
    // Waiting: a second press makes no second request.
    rt.update(|s, _| s.pack_preview("catalog:demo-clock"));
    assert_eq!(sent(&rt).len(), 1);
    answer(&mut rt, &look, Reply::PackPreview(neko_rune::scenes_setup::demo_preview()));
    assert_eq!(sent(&rt).len(), 1, "previewing installed nothing");
    // Permissions, what it runs, warnings, plugin power, routine text, what it adds, requirements and the masked value.
    assert!(has_text(&rt, "Reads the notes folder you point it at"));
    assert_eq!(text_of(&rt, "dlg-pack-run-0").as_deref(), Some("demo-cli serve --stdio"));
    assert!(has_text(&rt, "Runs a script interpreter: bash") && has_text(&rt, "Runs inside Neko with access to Neko's own API"));
    assert!(has_text(&rt, "When asked for the time somewhere, use the clock tool."), "the plugin's prompt is shown");
    assert_eq!(text_of(&rt, "dlg-pack-routine-text-0").as_deref(), Some("Tell me the time in three cities."));
    assert!(has_text(&rt, "Routines are installed switched off") && has_text(&rt, "granted to no workspace until you grant them in Tools"));
    assert_eq!(text_of(&rt, "dlg-pack-req-state-0").as_deref(), Some("Missing"));
    assert_eq!(text_of(&rt, "dlg-pack-req-hint-text-0").as_deref(), Some("brew install demo-cli"));
    assert!(rt.find("pack:env:DEMO_CLOCK_KEY").is_some());
    assert!(rt.find("dlg-pack-install").is_some());
}

#[test]
fn a_refused_preview_shows_the_daemons_words_and_gives_the_buttons_back() {
    let mut rt = scene("setup-pack-add");
    click(&mut rt, "dlg-pack-add-demo-clock");
    let call = Call::Setup(Box::new(SetupAction::PackPreview { source: "catalog:demo-clock".into() }));
    rt.update(move |s, _| {
        s.apply_reply(&call, Reply::Refused("There is no pack.toml in that folder.".into()));
    });
    settle(&mut rt);
    assert!(has_text(&rt, "There is no pack.toml in that folder."));
    assert!(rt.state.setup.awaiting.is_none());
    forget(&mut rt);
    click(&mut rt, "dlg-pack-add-demo-clock");
    assert_eq!(sent(&rt).len(), 1, "it can be asked again");
}

#[test]
fn an_installed_version_is_said_in_the_preview() {
    let mut rt = scene("setup-pack-add");
    click(&mut rt, "dlg-pack-add-demo-clock");
    let mut p = neko_rune::scenes_setup::demo_preview();
    p.installed_version = Some("0.0.9".into());
    answer(&mut rt, &SetupAction::PackPreview { source: "catalog:demo-clock".into() }, Reply::PackPreview(p));
    assert!(has_text(&rt, "Version 0.0.9 is already installed."));
}

#[test]
fn install_sends_the_previews_digest_and_the_typed_value_and_closes_only_on_its_own_answer() {
    let mut rt = scene("setup-pack-add");
    click(&mut rt, "dlg-pack-add-demo-clock");
    let look = SetupAction::PackPreview { source: "catalog:demo-clock".into() };
    answer(&mut rt, &look, Reply::PackPreview(neko_rune::scenes_setup::demo_preview()));
    forget(&mut rt);
    fill(&mut rt, "pack:env:DEMO_CLOCK_KEY", " k-123 ");
    click(&mut rt, "dlg-pack-install");
    let sent_now = sent(&rt);
    assert_eq!(sent_now.len(), 1, "{sent_now:?}");
    let install = sent_now[0].clone();
    let SetupAction::PackInstall { source, digest, env } = &install else { panic!("not an install: {install:?}") };
    assert_eq!((source.as_str(), digest.as_str()), ("catalog:demo-clock", "digest-clock-1"));
    assert_eq!(env.len(), 1);
    // The exact wire: the digest as shown and the value, no approve flag.
    let wire = serde_json::to_value(install.wire()).unwrap();
    assert_eq!(wire, serde_json::json!({"Packs": {"Install": {"source": "catalog:demo-clock", "digest": "digest-clock-1", "env": {"DEMO_CLOCK_KEY": "k-123"}}}}));
    // Buttons off while it is in flight: a second press sends nothing.
    click(&mut rt, "dlg-pack-install");
    assert_eq!(sent(&rt).len(), 1);
    // An answer to another request changes nothing.
    let now = listing(&rt);
    answer(&mut rt, &SetupAction::PackList, Reply::Packs(now));
    assert!(matches!(rt.state.setup.dialog, Some(Dialog::Pack { .. })), "the dialog is still open");
    // A refusal shows in the dialog and gives the buttons back.
    let call = Call::Setup(Box::new(install.clone()));
    rt.update(move |s, _| {
        s.apply_reply(&call, Reply::Refused("What you were shown changed. Preview it again.".into()));
    });
    settle(&mut rt);
    assert!(has_text(&rt, "What you were shown changed. Preview it again.") && rt.state.setup.awaiting.is_none());
    // Try again; its own answer closes the dialog and the list shows the pack.
    click(&mut rt, "dlg-pack-install");
    assert_eq!(sent(&rt).len(), 2);
    let mut l = listing(&rt);
    let mut added = l.installed[0].clone();
    added.info.name = "demo-clock".into();
    added.info.title = "Demo clock".into();
    added.source = "catalog:demo-clock".into();
    l.installed.push(added);
    l.catalog[1].installed = true;
    answer(&mut rt, &install, Reply::Packs(l));
    assert!(rt.state.setup.dialog.is_none());
    assert!(rt.find("pack-demo-clock").is_some());
    assert!(rt.state.pages.text("pack:env:DEMO_CLOCK_KEY").is_empty(), "a typed value does not outlive the dialog");
}

#[test]
fn back_returns_from_the_preview_and_an_empty_value_is_not_sent() {
    let mut rt = scene("setup-pack-add");
    click(&mut rt, "dlg-pack-add-demo-clock");
    answer(&mut rt, &SetupAction::PackPreview { source: "catalog:demo-clock".into() }, Reply::PackPreview(neko_rune::scenes_setup::demo_preview()));
    click(&mut rt, "dlg-pack-back");
    assert!(rt.find("dlg-pack-tabs").is_some() && rt.find("dlg-pack-install").is_none());
    click(&mut rt, "dlg-pack-add-demo-clock");
    answer(&mut rt, &SetupAction::PackPreview { source: "catalog:demo-clock".into() }, Reply::PackPreview(neko_rune::scenes_setup::demo_preview()));
    forget(&mut rt);
    click(&mut rt, "dlg-pack-install");
    assert!(matches!(sent(&rt).as_slice(), [SetupAction::PackInstall { env, .. }] if env.is_empty()), "{:?}", sent(&rt));
}

#[test]
fn the_preview_type_is_the_protocols() {
    // The scene's preview is a real `PackPreview`: it survives the wire.
    let p = neko_rune::scenes_setup::demo_preview();
    let back: PackPreview = serde_json::from_value(serde_json::to_value(&p).unwrap()).unwrap();
    assert_eq!(back, p);
}
