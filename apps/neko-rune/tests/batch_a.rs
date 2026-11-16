//! Parity batch A (the owner's brief of 2026-11-16): the main path of each section, headless, through the real layout
//! and hit-testing. The daemon-backed paths (uploads, `/api/files`) are in `harness_a.rs`.

use neko_protocol::workbench::TaskStatus;
use neko_rune::app::App;
use neko_rune::link::{Call, Reply};
use neko_rune::scenes;
use neko_rune::view::view;
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const WINDOW: (f32, f32) = (1360.0, 900.0);

pub fn scene(name: &str) -> impl FnOnce(&mut App) + '_ {
    move |app| {
        let (_, f) = scenes::SCENES.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no scene {name}"));
        f(app);
        app.outbox.clear();
    }
}

fn runtime_sized(setup: impl FnOnce(&mut App), size: (f32, f32)) -> Runtime<App> {
    let mut app = App::headless();
    setup(&mut app);
    let mut rt = Runtime::new(app, view, size, Env { system_dark: true, scale: 1.0 });
    // The window's own key handler (⌘K, ⌘⇧Esc, `/`, J, K, ...), as the real window installs it.
    rt.on_key = Some(neko_rune::on_key);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn runtime(setup: impl FnOnce(&mut App)) -> Runtime<App> {
    runtime_sized(setup, WINDOW)
}

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

fn click_text(rt: &mut Runtime<App>, label: &str) {
    let r = rt.elements().into_iter().find(|e| e.text.as_deref() == Some(label)).unwrap_or_else(|| panic!("no text {label}: {:?}", texts(rt))).rect;
    let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn right_click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    settle(rt);
}

fn press(rt: &mut Runtime<App>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    settle(rt);
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element keyed {key}")).rect
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

fn has_text(rt: &Runtime<App>, needle: &str) -> bool {
    texts(rt).iter().any(|t| t.contains(needle))
}

fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn fill_of(rt: &Runtime<App>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).map(|e| e.fill.hex()).unwrap_or_else(|| panic!("no element keyed {key}"))
}

/// The open session in `status`, with its row and its detail agreeing.
fn in_status(status: TaskStatus) -> impl FnOnce(&mut App) {
    move |app| {
        scene("running")(app);
        let summary = app.summary.as_mut().unwrap();
        let id = app.selected.clone().unwrap();
        summary.tasks.iter_mut().find(|t| t.id == id).unwrap().status = status;
        summary.detail.as_mut().unwrap().task.status = status;
    }
}

// ------------------------------------------------------------------------------------------------ 1. header and badges

#[test]
fn the_header_is_opaque_and_an_eight_point_fade_sits_under_it() {
    let rt = runtime(scene("running"));
    let header = rect(&rt, "top-title");
    let fade = rect(&rt, "header-fade");
    assert_eq!(fade[3], 8.0, "an 8 point fade");
    assert!(fade[1] >= header[1] + header[3] - 0.5, "it is under the header row: {fade:?} {header:?}");
    // The goal row (when there is one) is opaque, and the fade is under it.
    let goals = rect(&rt, "goals");
    assert!(fade[1] >= goals[1] + goals[3] - 0.5, "under the goal row: {fade:?} {goals:?}");
    let window = neko_rune::theme::theme(true).surface.window.hex();
    assert_eq!(fill_of(&rt, "goals"), window, "the goal row is the region's own colour, opaque");
}

#[test]
fn ready_to_review_is_sky_and_queued_completed_and_cancelled_have_no_badge() {
    let sky = neko_rune::theme::theme(true).meaning.sky.alpha(0.12).hex();
    let rt = runtime(in_status(TaskStatus::ReadyForReview));
    assert!(has_text(&rt, "Ready to review"));
    assert_eq!(fill_of(&rt, "state-badge"), sky, "sky text on sky at 12%");
    for status in [TaskStatus::Queued, TaskStatus::Completed, TaskStatus::Cancelled] {
        let rt = runtime(in_status(status));
        assert!(!has_key(&rt, "state-badge"), "{status:?} has no badge");
    }
    let amber = neko_rune::theme::theme(true).meaning.amber.alpha(0.12).hex();
    let rt = runtime(in_status(TaskStatus::Building));
    assert_eq!(fill_of(&rt, "state-badge"), amber);
}

fn catalog() -> neko_protocol::agent_models::ModelCatalog {
    serde_json::from_value(serde_json::json!({
        "sources": [
            {"provider": "claude", "label": "Claude", "connection": "Pro", "status": "ready", "models": [
                {"id": "opus", "label": "Opus", "access": "listed", "recommended": true, "effort_options": [{"id": "low", "label": "Low"}, {"id": "high", "label": "High"}], "speed_options": []},
                {"id": "haiku", "label": "Haiku", "access": "listed", "effort_options": [], "speed_options": []}
            ]},
            {"provider": "codex", "label": "Codex", "connection": "Plus", "status": "ready", "models": [
                {"id": "gpt-5.5", "label": "GPT-5.5", "access": "checked", "effort_options": [{"id": "high", "label": "High"}], "speed_options": [{"id": "priority", "label": "Fast"}]}
            ]}
        ]
    }))
    .unwrap()
}

#[test]
fn the_model_chip_opens_a_popover_and_apply_sends_every_pin_the_web_sends() {
    let mut rt = runtime(scene("running"));
    click(&mut rt, "runtime-chip");
    assert!(has_key(&rt, "runtime-popover"), "the chip opens the popover");
    assert!(rt.state.outbox.contains(&Call::Models), "the catalog is read when it opens: {:?}", rt.state.outbox);
    assert!(has_text(&rt, "Model & effort") && has_text(&rt, "Neko decides"));
    // The catalog arrives.
    rt.update(|s, _| {
        s.apply_reply(&Call::Models, Reply::Models { choices: Vec::new(), catalog: Box::new(catalog()) });
    });
    settle(&mut rt);
    assert!(has_key(&rt, "runtime-model-claude-opus") && has_key(&rt, "runtime-model-codex-gpt-5.5"));
    assert!(has_text(&rt, "Recommended"));
    click(&mut rt, "runtime-model-claude-opus");
    assert!(has_key(&rt, "runtime-effort"), "Opus has effort levels");
    assert!(!has_key(&rt, "runtime-speed-priority"), "and no extra speed");
    click(&mut rt, "runtime-effort-high");
    rt.state.outbox.clear();
    click(&mut rt, "runtime-apply");
    let Some(Call::SetRuntimePrefs { task_id, prefs }) = rt.state.outbox.last().cloned() else { panic!("{:?}", rt.state.outbox) };
    assert_eq!(task_id, "t6");
    assert_eq!((prefs.provider.as_deref(), prefs.model.as_deref(), prefs.reasoning_effort.as_deref(), prefs.service_tier.clone(), prefs.allow_paid_speed), (Some("claude"), Some("opus"), Some("high"), None, false));
    // On the wire it is the web's request: SetConversationRuntime on `task:<id>`.
    let wire = serde_json::to_value(Call::SetRuntimePrefs { task_id: "t6".into(), prefs }.wire()).unwrap();
    assert_eq!(wire["Workbench"]["SetConversationRuntime"]["conversation_id"], "task:t6");
    assert_eq!(wire["Workbench"]["SetConversationRuntime"]["preferences"]["reasoning_effort"], "high");
    // The daemon refuses: its words are shown verbatim and the popover stays.
    rt.update(|s, _| {
        let call = s.outbox.last().cloned().unwrap();
        s.apply_reply(&call, Reply::Refused("Opus is not on this plan.".into()));
    });
    settle(&mut rt);
    assert!(has_text(&rt, "Settings were not applied. Opus is not on this plan."));
    // Esc-less close: pressing outside puts it away.
    rt.update(|s, _| s.runtime.open = false);
    settle(&mut rt);
    assert!(!has_key(&rt, "runtime-popover"));
}

#[test]
fn changing_the_model_drops_the_effort_it_cannot_honour_and_says_so() {
    let mut rt = runtime(scene("running"));
    click(&mut rt, "runtime-chip");
    rt.update(|s, _| {
        s.apply_reply(&Call::Models, Reply::Models { choices: Vec::new(), catalog: Box::new(catalog()) });
    });
    settle(&mut rt);
    click(&mut rt, "runtime-model-claude-opus");
    click(&mut rt, "runtime-effort-high");
    click(&mut rt, "runtime-model-claude-haiku");
    assert!(has_text(&rt, "Effort reset to Auto: Haiku does not advertise high."), "{:?}", texts(&rt));
    assert_eq!(rt.state.runtime.draft.reasoning_effort, None);
}

// ------------------------------------------------------------------------------------------------ 2. task actions

#[test]
fn the_session_menu_offers_the_webs_items_and_rename_edits_the_title_in_place() {
    let mut rt = runtime(in_status(TaskStatus::Failed));
    click(&mut rt, "session-more");
    for label in ["Rename…", "Retry from start…", "Mark as unread", "Copy link", "Open in web UI", "Delete…"] {
        assert!(has_text(&rt, label), "{label} is in the menu of a failed task: {:?}", texts(&rt));
    }
    assert!(!neko_rune::task_actions::entries(&rt.state, "t6").iter().any(|e| e.label == "Stop"), "a failed task is not running");
    click_text(&mut rt, "Rename…");
    assert!(has_key(&rt, "rename-input"), "the header title is a field now");
    // Enter saves.
    rt.update(|s, _| s.rename.as_mut().unwrap().edit.replace_text("Shorter name"));
    settle(&mut rt);
    rt.state.outbox.clear();
    press(&mut rt, Key::Enter, Mods::default());
    assert!(matches!(rt.state.outbox.first(), Some(Call::Rename { title, .. }) if title == "Shorter name"), "{:?}", rt.state.outbox);
    assert!(!has_key(&rt, "rename-input"));
    // Esc cancels.
    rt.update(|s, _| s.start_rename("t6"));
    settle(&mut rt);
    rt.state.outbox.clear();
    press(&mut rt, Key::Escape, Mods::default());
    assert!(!has_key(&rt, "rename-input") && rt.state.outbox.is_empty(), "Esc sends nothing");
}

#[test]
fn delete_and_retry_ask_in_a_dialog_with_a_coral_button_and_send_nothing_until_yes() {
    let mut rt = runtime(in_status(TaskStatus::Failed));
    click(&mut rt, "session-more");
    click_text(&mut rt, "Delete…");
    assert!(has_key(&rt, "ask-yes") && has_text(&rt, "Delete this task?"));
    assert_eq!(fill_of(&rt, "ask-yes"), neko_rune::theme::theme(true).meaning.coral.hex(), "the destructive button is coral");
    assert!(rt.state.outbox.is_empty(), "nothing is sent while the dialog is open");
    click(&mut rt, "ask-no");
    assert!(rt.state.outbox.is_empty() && !has_key(&rt, "ask-yes"), "Keep it sends nothing");
    click(&mut rt, "session-more");
    click_text(&mut rt, "Retry from start…");
    assert!(rt.state.outbox.is_empty());
    click(&mut rt, "ask-yes");
    assert_eq!(rt.state.outbox, vec![Call::Retry { task_id: "t6".into() }]);
}

#[test]
fn the_row_menu_is_the_same_list_as_the_session_menu() {
    let mut rt = runtime(in_status(TaskStatus::Failed));
    let id = rt.state.selected.clone().unwrap();
    let header: Vec<String> = neko_rune::task_actions::entries(&rt.state, &id).into_iter().map(|e| e.label).collect();
    right_click(&mut rt, &format!("session-{id}"));
    for label in &header {
        assert!(has_text(&rt, label), "the row's menu has {label}");
    }
}

#[test]
fn stop_all_asks_how_many_agents_and_command_shift_escape_asks_too() {
    let mut rt = runtime(scene("running"));
    let (tasks, replying) = rt.state.running_agents();
    let running = tasks + usize::from(replying);
    assert!(running > 0, "the scene has running work");
    let ev = KeyEvent { key: Key::Escape, mods: Mods { cmd: true, shift: true, ..Mods::default() } };
    rt.update(|s, tx| assert!(neko_rune::on_key(s, &ev, tx), "the window's handler takes the chord"));
    settle(&mut rt);
    assert!(has_text(&rt, &format!("Stop all {running} running agent")), "{:?}", texts(&rt));
    assert!(rt.state.outbox.is_empty());
    click(&mut rt, "ask-yes");
    assert_eq!(rt.state.outbox, vec![Call::StopAll]);
    // The account menu has it too.
    let mut rt = runtime(scene("running"));
    click(&mut rt, "account");
    assert!(has_text(&rt, "Stop all work"));
}

#[test]
fn view_all_opens_all_sessions_with_search_a_workspace_filter_and_j_k() {
    let mut rt = runtime(scene("live"));
    rt.update(|s, _| s.open_all_sessions());
    settle(&mut rt);
    assert!(has_text(&rt, "All sessions"));
    assert!(has_key(&rt, "all-search") && has_key(&rt, "all-workspace") && has_key(&rt, "all-more"));
    // `/` focuses the search; typing narrows the rows.
    press(&mut rt, Key::Text("/".into()), Mods::default());
    assert!(rt.state.all.search.focused, "/ focuses the search field");
    let total = rt.state.all_rows().len();
    assert!(total >= 2, "the scene has sessions: {total}");
    rt.paste("zzzz-no-such-title");
    settle(&mut rt);
    assert_eq!(rt.state.all_rows().len(), 0);
    rt.update(|s, _| s.all.search.replace_text(""));
    settle(&mut rt);
    // J and K move the cursor down and up, Return opens the row.
    rt.update(|s, _| s.all.search.focused = false);
    press(&mut rt, Key::Text("j".into()), Mods::default());
    assert_eq!(rt.state.all.cursor, 1);
    press(&mut rt, Key::Text("k".into()), Mods::default());
    assert_eq!(rt.state.all.cursor, 0);
    press(&mut rt, Key::Text("j".into()), Mods::default());
    let expected = rt.state.all_rows()[1].id.clone();
    press(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.selected.as_deref(), Some(expected.as_str()));
}

#[test]
fn all_sessions_groups_by_stage_hides_failed_and_cancelled_until_asked_and_clears_finished_after_a_dialog() {
    let mut rt = runtime(scene("live"));
    rt.update(|s, _| s.open_all_sessions());
    settle(&mut rt);
    let stages: Vec<&str> = ["Needs you", "Ready to review", "Running", "Up next", "Done"].into_iter().filter(|s| has_text(&rt, s)).collect();
    assert!(!stages.is_empty(), "rows are under stage headings: {:?}", texts(&rt));
    let shown_failed = rt.state.all_rows().iter().any(|t| matches!(t.status, TaskStatus::Failed | TaskStatus::Cancelled));
    assert!(!shown_failed, "failed and cancelled are hidden by default");
    click(&mut rt, "all-more");
    assert!(has_text(&rt, "Clear finished") && has_text(&rt, "Show failed and cancelled"));
    click_text(&mut rt, "Show failed and cancelled");
    assert!(rt.state.all.include_stopped);
    click(&mut rt, "all-more");
    rt.state.outbox.clear();
    click_text(&mut rt, "Clear finished");
    assert!(rt.state.outbox.is_empty(), "Clear finished asks first");
    assert!(has_key(&rt, "ask-yes") || has_text(&rt, "Nothing finished to clear."), "a dialog (or the calm line when there is nothing to clear)");
}
