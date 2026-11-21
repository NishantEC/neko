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
    let mut app = App::headless_neko();
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
    let Some(Call::SetRuntimePrefs { conversation_id, prefs }) = rt.state.outbox.last().cloned() else { panic!("{:?}", rt.state.outbox) };
    assert_eq!(conversation_id, "task:t6");
    assert_eq!((prefs.provider.as_deref(), prefs.model.as_deref(), prefs.reasoning_effort.as_deref(), prefs.service_tier.clone(), prefs.allow_paid_speed), (Some("claude"), Some("opus"), Some("high"), None, false));
    // On the wire it is the web's request: SetConversationRuntime on `task:<id>`.
    let wire = serde_json::to_value(Call::SetRuntimePrefs { conversation_id: "task:t6".into(), prefs }.wire()).unwrap();
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
    // The sidebar lists a task only under its cat: miso's building task in the cats scene.
    let mut rt = runtime(scene("cat-page"));
    let id = "t6".to_owned();
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
    assert!(has_text(&rt, "Tasks"), "the web's title");
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
    // As the web, failed and cancelled tasks show by default; the overflow menu hides them.
    assert!(rt.state.all.include_stopped, "failed and cancelled are shown by default");
    click(&mut rt, "all-more");
    assert!(has_text(&rt, "Clear finished") && has_text(&rt, "Show failed and cancelled"));
    click_text(&mut rt, "Show failed and cancelled");
    assert!(!rt.state.all.include_stopped);
    let shown_failed = rt.state.all_rows().iter().any(|t| matches!(t.status, TaskStatus::Failed | TaskStatus::Cancelled));
    assert!(!shown_failed, "unchecked, they are hidden");
    click(&mut rt, "all-more");
    click_text(&mut rt, "Show failed and cancelled");
    assert!(rt.state.all.include_stopped);
    click(&mut rt, "all-more");
    rt.state.outbox.clear();
    click_text(&mut rt, "Clear finished");
    assert!(rt.state.outbox.is_empty(), "Clear finished asks first");
    assert!(has_key(&rt, "ask-yes") || has_text(&rt, "Nothing finished to clear."), "a dialog (or the calm line when there is nothing to clear)");
}

// ------------------------------------------------------------------------------------------------ 3. composer

fn type_ask(rt: &mut Runtime<App>, words: &str) {
    click(rt, "ask");
    rt.update(|s, _| s.ask.replace_text(words));
    settle(rt);
}

#[test]
fn a_slash_opens_the_popover_with_the_webs_commands_32_point_rows_and_at_most_eight_visible() {
    let mut rt = runtime(scene("live"));
    type_ask(&mut rt, "/");
    assert!(has_key(&rt, "ask-popover"), "the popover opens above the ask box");
    let rows = rt.state.ask_rows();
    assert_eq!(rows.len(), 12, "/plan and the web's eleven commands");
    assert_eq!(rows[0].title, "/plan");
    let first = rect(&rt, "ask-pop-0");
    assert_eq!(first[3], 32.0, "32 point rows");
    let list = rect(&rt, "ask-popover-scroll");
    assert!(list[3] <= 8.0 * 32.0 + 0.5, "at most 8 rows show: {list:?}");
    let card = rect(&rt, "ask-card");
    assert!(list[1] + list[3] <= card[1] + 1.0, "it sits above the composer: {list:?} {card:?}");
    // Typing narrows.
    rt.update(|s, _| s.ask.replace_text("/re"));
    settle(&mut rt);
    assert_eq!(rt.state.ask_rows().iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), ["/remember", "/recall"]);
}

#[test]
fn arrows_choose_enter_accepts_and_escape_closes_and_a_command_that_takes_an_argument_only_fills_it_in() {
    let mut rt = runtime(scene("live"));
    type_ask(&mut rt, "/re");
    press(&mut rt, Key::Down, Mods::default());
    assert_eq!(rt.state.compose.index, 1);
    press(&mut rt, Key::Up, Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.ask.text, "/remember ", "it takes an argument: it fills the box and waits");
    assert!(rt.state.outbox.is_empty());
    // /plan switches Plan on and empties the box.
    type_ask(&mut rt, "/pl");
    press(&mut rt, Key::Tab, Mods::default());
    assert!(rt.state.ask_plan && rt.state.ask.text.is_empty());
    // Esc closes the popover until the draft changes.
    type_ask(&mut rt, "/");
    press(&mut rt, Key::Escape, Mods::default());
    assert!(!has_key(&rt, "ask-popover"));
    // A command that runs: /tickets opens All sessions.
    type_ask(&mut rt, "/tickets");
    press(&mut rt, Key::Enter, Mods::default());
    assert!(rt.state.ask.text.is_empty());
    assert_eq!(rt.state.view, Some(neko_rune::views::FullView::AllSessions));
}

#[test]
fn slash_stop_asks_before_it_stops() {
    let mut rt = runtime(scene("live"));
    type_ask(&mut rt, "/stop");
    press(&mut rt, Key::Enter, Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    assert!(rt.state.dialog.is_some() || has_text(&rt, "Nothing is running."), "a dialog, or the calm line");
    assert!(!rt.state.outbox.contains(&Call::StopAll), "nothing was stopped without a yes");
}

#[test]
fn a_mention_lists_workspaces_tools_and_files_and_replaces_the_word() {
    let mut rt = runtime(scene("live"));
    rt.update(|s, _| {
        let m = s.summary.as_mut().unwrap();
        m.connections = vec![("c1".into(), "Linear".into(), 12)];
        s.compose.files = vec![neko_rune::link::http::MentionFile { path: "/p/readme.md".into(), name: "readme.md".into(), location: "~/p".into() }];
    });
    type_ask(&mut rt, "look at @");
    let titles: Vec<String> = rt.state.ask_rows().into_iter().map(|r| format!("{}|{}", r.title, r.detail)).collect();
    assert!(titles.iter().any(|t| t.ends_with("|Workspace")), "{titles:?}");
    assert!(titles.contains(&"Linear|12 tools".to_owned()), "{titles:?}");
    assert!(titles.contains(&"readme.md|~/p".to_owned()), "{titles:?}");
    let at = rt.state.ask_rows().iter().position(|r| r.title == "Linear").unwrap();
    click(&mut rt, &format!("ask-pop-{at}"));
    assert_eq!(rt.state.ask.text, "look at @Linear ");
}

#[test]
fn files_are_attached_as_chips_uploaded_over_http_and_referenced_when_sent() {
    let dir = std::env::temp_dir().join(format!("neko-rune-batch-a-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("notes.txt");
    std::fs::write(&file, b"hello").unwrap();
    let mut rt = runtime(scene("running"));
    rt.update(|s, _| s.http = Some(neko_rune::link::http::Http::new(1, &"0".repeat(64))));
    // Dropping a file anywhere on the session region shows the overlay, then attaches.
    rt.drag_files(vec![file.clone()], Some((600.0, 300.0)));
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
    assert!(has_key(&rt, "drop-overlay"), "the overlay shows while files are dragged over");
    let frame = rect(&rt, "drop-frame");
    let region = rect(&rt, "session-region");
    assert!((frame[0] - region[0] - 12.0).abs() < 1.0 && (frame[1] - region[1] - 12.0).abs() < 1.0, "inset 12: {frame:?} {region:?}");
    assert!(has_text(&rt, "Drop to attach"));
    rt.drop_files(vec![file.clone()], Some((600.0, 300.0)));
    settle(&mut rt);
    assert!(!has_key(&rt, "drop-overlay"));
    let chips = rt.state.chips(neko_rune::compose::Dest::Reply).to_vec();
    assert_eq!(chips.len(), 1);
    assert_eq!((chips[0].name.as_str(), chips[0].size), ("notes.txt", 5));
    assert!(has_text(&rt, "notes.txt") && has_text(&rt, "Uploading…"));
    assert!(matches!(rt.state.compose.asked.first(), Some(neko_rune::compose::HttpWork::Upload { name, .. }) if name == "notes.txt"), "{:?}", rt.state.compose.asked);
    // Sending waits for the upload.
    rt.update(|s, _| s.composer.replace_text("see the notes"));
    rt.state.outbox.clear();
    rt.update(|s, _| s.send_composer(None));
    assert!(rt.state.outbox.is_empty(), "not while a file is on its way");
    // The daemon keeps it: the chip is ready, and the reference joins the message.
    let id = chips[0].id;
    rt.update(|s, _| {
        s.http_done(neko_rune::compose::HttpDone::Uploaded { dest: neko_rune::compose::Dest::Reply, id, result: Ok(neko_rune::link::http::Kept { name: "notes.txt".into(), reference: "[notes.txt](file:///a/notes.txt)".into(), is_image: false }) });
    });
    settle(&mut rt);
    assert!(has_text(&rt, "5 B"));
    rt.update(|s, _| s.send_composer(None));
    let Some(Call::Enqueue { text, .. }) = rt.state.outbox.last().cloned() else { panic!("{:?}", rt.state.outbox) };
    assert_eq!(text, "see the notes\n[notes.txt](file:///a/notes.txt)");
    // The x takes a chip away.
    rt.update(|s, _| s.attach_files(neko_rune::compose::Dest::Reply, std::slice::from_ref(&file)));
    settle(&mut rt);
    let second = rt.state.chips(neko_rune::compose::Dest::Reply).iter().map(|c| c.id).max().unwrap();
    click(&mut rt, &format!("chip-x-{second}"));
    assert!(rt.state.chips(neko_rune::compose::Dest::Reply).iter().all(|c| c.id != second));
    std::fs::remove_dir_all(&dir).ok();
}

fn with_queued(app: &mut App) {
    scene("live")(app);
    let m = app.summary.as_mut().unwrap();
    for (id, text) in [("q1", "first thing"), ("q2", "second thing")] {
        let message: neko_protocol::workbench::ChatMessage = serde_json::from_value(serde_json::json!({
            "id": id, "at_ms": 1, "role": "user", "parts": [{"type": "text", "text": text}], "queued": true,
        }))
        .unwrap();
        m.conversation.push(message);
    }
}

#[test]
fn queued_messages_show_hover_actions_edit_in_place_send_now_and_cancel() {
    let mut rt = runtime(with_queued);
    assert!(has_key(&rt, "queue-tray") && has_text(&rt, "first thing") && has_text(&rt, "second thing"));
    click(&mut rt, "queue-cancel-q1");
    assert_eq!(rt.state.outbox.last(), Some(&Call::DiscardQueued { message_id: "q1".into() }));
    click(&mut rt, "queue-send-q2");
    assert_eq!(rt.state.outbox.last(), Some(&Call::SteerQueued { message_id: "q2".into(), text: "second thing".into(), workspace_id: None }));
    // Edit turns the row into a field; Enter queues the new words, Esc keeps the old ones.
    click(&mut rt, "queue-edit-q1");
    assert!(has_key(&rt, "queue-edit"));
    rt.update(|s, _| s.compose.editing.as_mut().unwrap().1.replace_text("first thing, edited"));
    settle(&mut rt);
    press(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.outbox.last(), Some(&Call::EditQueued { message_id: "q1".into(), text: "first thing, edited".into(), workspace_id: None }));
    click(&mut rt, "queue-edit-q1");
    press(&mut rt, Key::Escape, Mods::default());
    assert!(!has_key(&rt, "queue-edit") && rt.state.compose.editing.is_none());
    // On the wire: Discard first, then the second request.
    let wire = serde_json::to_value(Call::SteerQueued { message_id: "q2".into(), text: "t".into(), workspace_id: None }.then().unwrap()).unwrap();
    assert_eq!(wire["Workbench"]["InterruptAndSendMessage"]["text"], "t");
}

#[test]
fn alt_up_edits_the_last_queued_message_and_command_return_stops_the_reply_and_sends() {
    let mut rt = runtime(with_queued);
    click(&mut rt, "ask");
    let ev = KeyEvent { key: Key::Up, mods: Mods { alt: true, ..Mods::default() } };
    rt.update(|s, _| assert!(s.ask_key(&ev)));
    assert_eq!(rt.state.compose.editing.as_ref().map(|(id, e)| (id.as_str(), e.text.as_str())), Some(("q2", "second thing")), "the last one");
    rt.update(|s, _| s.cancel_edit_queued());
    rt.update(|s, _| s.ask.replace_text("do this first"));
    let ev = KeyEvent { key: Key::Enter, mods: Mods { cmd: true, ..Mods::default() } };
    rt.state.outbox.clear();
    rt.update(|s, _| assert!(s.ask_key(&ev)));
    assert_eq!(rt.state.outbox, vec![Call::AskNow { text: "do this first".into(), workspace_id: None }]);
    let wire = serde_json::to_value(Call::AskNow { text: "x".into(), workspace_id: None }.wire()).unwrap();
    assert!(wire["Workbench"]["InterruptAndSendMessage"].is_object());
    // Plain Return queues (the daemon holds it behind the reply).
    rt.update(|s, _| s.work.busy.clear());
    rt.update(|s, _| s.ask.replace_text("and then this"));
    rt.state.outbox.clear();
    let ev = KeyEvent { key: Key::Enter, mods: Mods::default() };
    rt.update(|s, _| assert!(s.ask_key(&ev)));
    assert_eq!(rt.state.outbox, vec![Call::Ask { text: "and then this".into(), workspace_id: None }]);
}

#[test]
fn the_right_click_menu_of_a_message_copies_quotes_edits_and_opens_the_task() {
    let mut rt = runtime(scene("live"));
    let user_key = rt.elements().into_iter().filter_map(|e| e.key).find(|k| k.starts_with("neko-user-")).expect("a message of yours in Neko's chat");
    right_click(&mut rt, &user_key);
    assert!(has_text(&rt, "Copy message") && has_text(&rt, "Quote in reply") && has_text(&rt, "Edit and send again"), "{:?}", texts(&rt));
    click_text(&mut rt, "Quote in reply");
    assert!(rt.state.ask.text.starts_with("> "), "the quote goes into the composer: {:?}", rt.state.ask.text);
    right_click(&mut rt, &user_key);
    let words = rt.state.msg_menu.as_ref().unwrap().text.clone();
    click_text(&mut rt, "Edit and send again");
    assert_eq!(rt.state.ask.text, words);
    // A crew line: Open the task.
    let handoff = rt.elements().into_iter().filter_map(|e| e.key).find(|k| k.starts_with("handoff-")).expect("a crew line");
    right_click(&mut rt, &handoff);
    assert!(texts(&rt).iter().any(|t| t.starts_with("Open NEK-")), "{:?}", texts(&rt));
    assert!(!has_text(&rt, "Edit and send again"), "not on a crew line");
}

#[test]
fn a_finished_replys_csv_fence_is_a_table_and_a_3d_fence_names_the_artifact() {
    let mut rt = runtime_sized(scene("live"), (1360.0, 1800.0));
    rt.update(|s, _| {
        let m = s.summary.as_mut().unwrap();
        let message: neko_protocol::workbench::ChatMessage = serde_json::from_value(serde_json::json!({
            "id": "fence", "at_ms": 9, "started_ms": 5, "role": "neko",
            "parts": [{"type": "text", "text": "Numbers:\n\n```csv\nname,age\nada,36\ngrace,45\n```\n\nThe model:\n\n```3d\na1b2c3d4e5f6\n```\n\n```rust\nlet x = 1;\n```\n"}],
        }))
        .unwrap();
        m.conversation.push(message);
    });
    settle(&mut rt);
    assert!(has_key(&rt, "fence-table"), "csv is drawn through the table viewer");
    assert!(has_text(&rt, "ada") && has_text(&rt, "grace") && has_text(&rt, "name"));
    assert!(has_key(&rt, "fence-3d-a1b2c3d4e5f6"), "the 3d fence opens the artifact in the panel: {:?}", rt.elements().iter().filter_map(|e| e.key.clone()).filter(|k| k.contains("fence") || k.contains("code")).collect::<Vec<_>>());
    assert!(has_text(&rt, "let x = 1;"), "an ordinary fence stays code");
}

// ------------------------------------------------------------------------------------------------ 4. palette, shortcuts, scope

#[test]
fn the_palette_is_640_wide_at_20_percent_with_a_44_field_and_36_rows_in_the_webs_order() {
    let mut rt = runtime(scene("live"));
    rt.update(|s, _| s.palette_toggle());
    settle(&mut rt);
    let palette = rect(&rt, "palette");
    assert_eq!(palette[2], 640.0, "640 wide");
    assert!((palette[1] - 900.0 * 0.20).abs() < 1.5, "its top edge is at 20% of the window: {palette:?}");
    let anchor = rect(&rt, "palette-anchor");
    assert!((anchor[0] + anchor[2] / 2.0 - 680.0).abs() < 2.0, "centred: {anchor:?}");
    let field = rect(&rt, "palette-input");
    assert!(field[3] >= 43.5 && field[3] <= 44.5, "the search field is 44 high: {field:?}");
    assert_eq!(rect(&rt, "palette-row-0")[3], 36.0, "rows are 36");
    // The section headings come in the web's order.
    let headings: Vec<String> = rt.elements().into_iter().filter_map(|e| e.key).filter_map(|k| k.strip_prefix("palette-heading-").map(str::to_owned)).collect();
    assert_eq!(headings, ["Sessions", "Go to", "Actions", "Appearance"], "Plugins appears when a plugin registered something");
    // A row's shortcut is right-aligned (when the list shows it without scrolling).
    let items = rt.state.palette_items();
    assert!(items.iter().any(|i| i.right.as_deref() == Some("⌘⇧Esc")));
}

#[test]
fn arrows_move_return_runs_and_escape_closes_the_palette() {
    let mut rt = runtime(scene("live"));
    rt.update(|s, _| s.palette_toggle());
    settle(&mut rt);
    rt.paste("calendr");
    settle(&mut rt);
    press(&mut rt, Key::Down, Mods::default());
    press(&mut rt, Key::Up, Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    assert!(rt.state.palette.is_none());
    assert_eq!(rt.state.view, Some(neko_rune::views::FullView::Calendar), "a typo still finds Go to Calendar");
    rt.update(|s, _| s.palette_toggle());
    settle(&mut rt);
    press(&mut rt, Key::Escape, Mods::default());
    assert!(rt.state.palette.is_none(), "Esc closes");
}

#[test]
fn question_mark_opens_the_shortcut_overlay_in_two_columns_and_command_b_hides_the_sidebar() {
    let mut rt = runtime(scene("live"));
    let ev = KeyEvent { key: Key::Text("?".into()), mods: Mods::default() };
    rt.update(|s, tx| assert!(neko_rune::on_key(s, &ev, tx)));
    settle(&mut rt);
    assert!(has_text(&rt, "Keyboard shortcuts") && has_text(&rt, "Anywhere") && has_text(&rt, "Composer") && has_text(&rt, "A decision waiting for you"));
    let (left, right) = (rect(&rt, "shortcuts-left"), rect(&rt, "shortcuts-right"));
    assert!(right[0] > left[0] + left[2] - 1.0 && (left[1] - right[1]).abs() < 1.0, "two columns side by side: {left:?} {right:?}");
    assert!(has_text(&rt, "Stop all work") && has_text(&rt, "Show or hide the sidebar"));
    press(&mut rt, Key::Escape, Mods::default());
    assert!(!rt.state.shortcuts_open);
    assert!(rt.state.layout.sessions.is_open());
    let ev = KeyEvent { key: Key::Command('b'), mods: Mods { cmd: true, ..Mods::default() } };
    rt.update(|s, tx| assert!(neko_rune::on_key(s, &ev, tx)));
    assert!(!rt.state.layout.sessions.is_open(), "⌘B hides the sidebar");
    rt.update(|s, tx| assert!(neko_rune::on_key(s, &ev, tx)));
    assert!(rt.state.layout.sessions.is_open(), "and shows it again");
}

#[test]
fn the_right_click_menu_on_selected_text_asks_neko_or_searches() {
    let mut rt = runtime(scene("live"));
    let at = rune::PointerPress { x: 400.0, y: 300.0 };
    rt.update(|s, _| s.text_menu = Some(("the board draws nothing".to_owned(), at)));
    settle(&mut rt);
    assert!(has_text(&rt, "Ask Neko about this") && texts(&rt).iter().any(|t| t.starts_with("Search for “the board")), "{:?}", texts(&rt));
    click_text(&mut rt, "Ask Neko about this");
    assert_eq!(rt.state.ask.text, "> the board draws nothing\n\n", "Neko's composer has the quote");
    assert!(rt.state.layout.neko.is_open());
    rt.update(|s, _| s.text_menu = Some(("billing".to_owned(), at)));
    settle(&mut rt);
    let search = rt.elements().into_iter().find(|e| e.text.as_deref().is_some_and(|t| t.starts_with("Search for"))).unwrap().rect;
    rt.pointer_down(search[0] + 4.0, search[1] + search[3] / 2.0);
    rt.pointer_up(search[0] + 4.0, search[1] + search[3] / 2.0);
    settle(&mut rt);
    assert!(rt.state.palette.is_some());
    assert_eq!(rt.state.palette_query.text, "billing");
}

#[test]
fn the_workspace_switcher_is_in_the_brand_row_and_scopes_the_list_and_home() {
    let mut rt = runtime(scene("live"));
    let (brand, scope) = (rect(&rt, "sessions-header"), rect(&rt, "scope"));
    assert_eq!(scope[3], 20.0);
    assert!(scope[1] >= brand[1] && scope[1] + scope[3] <= brand[1] + brand[3] + 0.5 && scope[0] > brand[0] + 60.0, "at the right of the brand row: {brand:?} {scope:?}");
    assert!(has_text(&rt, "All workspaces"));
    let before = rt.state.group_count(neko_rune::groups::Group::NeedsYou);
    let workspaces: Vec<(String, String)> = rt.state.summary.as_ref().unwrap().workspaces.iter().map(|w| (w.id.clone(), w.name.clone())).collect();
    assert!(workspaces.len() >= 2, "the scene has workspaces: {workspaces:?}");
    // Give the tasks alternating workspaces so the scope has something to cut.
    rt.update(|s, _| {
        let m = s.summary.as_mut().unwrap();
        let ids: Vec<String> = m.tasks.iter().map(|t| t.id.clone()).collect();
        for (i, id) in ids.iter().enumerate() {
            m.meta.entry(id.clone()).or_default().workspace_id = workspaces[i % 2].0.clone();
        }
    });
    click(&mut rt, "scope");
    click_text(&mut rt, &workspaces[1].1);
    assert_eq!(rt.state.scope(), Some(workspaces[1].0.as_str()));
    assert!(has_text(&rt, &workspaces[1].1));
    assert!(rt.state.group_count(neko_rune::groups::Group::NeedsYou) < before, "fewer sessions are in the list now");
    assert!(rt.state.scope_dirty, "the choice is to be kept");
    rt.update(|s, _| s.open_new());
    settle(&mut rt);
    assert_eq!(rt.state.new_form.as_ref().unwrap().workspace, 1, "Home's composer starts in the scoped workspace");
}

// ------------------------------------------------------------------------------------------------ 7. toasts and the Dock

#[test]
fn toasts_stack_three_at_the_bottom_centre_forty_high_with_radius_twelve_and_go_after_four_seconds() {
    let mut rt = runtime(scene("live"));
    rt.update(|s, _| {
        for w in ["One.", "Two.", "Three.", "Four."] {
            s.show_toast(w);
        }
    });
    settle(&mut rt);
    let region = rect(&rt, "toast-region");
    let toasts: Vec<[f32; 4]> = rt.elements().into_iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("toast-") && !k.starts_with("toast-region") && !k.starts_with("toast-action"))).map(|e| e.rect).collect();
    assert_eq!(toasts.len(), 3, "at most three");
    assert!(toasts.iter().all(|r| (r[3] - 40.0).abs() < 0.5), "40 high: {toasts:?}");
    assert!((region[0] + region[2] / 2.0 - 680.0).abs() < 2.0, "bottom centre of the window: {region:?}");
    assert!(region[1] + region[3] > 800.0, "at the bottom: {region:?}");
    assert!(!has_text(&rt, "One."), "the oldest made room");
    rt.update(|s, tx| {
        s.tick_toast(10.0, tx);
    });
    rt.update(|s, tx| {
        assert!(!s.tick_toast(13.9, tx), "still up at 3.9 s");
        assert!(s.tick_toast(14.1, tx), "gone after 4 s");
    });
    assert!(rt.state.toasts.is_empty());
}

#[test]
fn a_toast_with_undo_sends_the_request_when_pressed() {
    let mut rt = runtime(scene("live"));
    rt.update(|s, _| s.toast_with("Moved to the front of the queue.", "Undo", Call::Unseen));
    settle(&mut rt);
    rt.state.outbox.clear();
    let id = rt.state.toasts[0].id;
    click(&mut rt, &format!("toast-action-{id}"));
    assert_eq!(rt.state.outbox, vec![Call::Unseen]);
    assert!(rt.state.toasts.is_empty());
}

// ------------------------------------------------------------------------------------------------ 5. the Inbox

fn inbox_item(seq: u64, default: Option<&str>) -> neko_rune::link::InboxItem {
    neko_rune::link::InboxItem {
        seq,
        title: format!("Item {seq}"),
        detail: "Something to decide.".into(),
        default: default.map(str::to_owned),
        routine_id: "r-messages".into(),
        run_id: format!("run-{seq}"),
        from: "Messages".into(),
        ..Default::default()
    }
}

#[test]
fn an_inbox_row_has_use_the_default_then_mark_done_and_mark_done_sends_clear_needs_you() {
    let mut rt = runtime(scene("live"));
    rt.update(|s, _| {
        s.neko.inbox = vec![inbox_item(9, Some("Send it")), inbox_item(8, None)];
        s.neko.loaded = true;
    });
    settle(&mut rt);
    // The check only where there is a default; Mark done on every row, to the right of it.
    assert!(has_key(&rt, "inbox-use-9") && !has_key(&rt, "inbox-use-8"));
    assert!(has_key(&rt, "inbox-done-9") && has_key(&rt, "inbox-done-8"));
    assert!(rect(&rt, "inbox-done-9")[0] > rect(&rt, "inbox-use-9")[0], "Use the default comes first, then Mark done");
    rt.state.outbox.clear();
    click(&mut rt, "inbox-done-9");
    assert_eq!(rt.state.outbox, vec![Call::ClearNeedsYou { routine_id: "r-messages".into(), run_id: "run-9".into(), seq: 9 }]);
    assert!(rt.state.toast_words().contains("Done. 1 left."), "{:?}", rt.state.toast_words());
    // On the wire: the web's own request.
    let wire = serde_json::to_value(Call::ClearNeedsYou { routine_id: "r".into(), run_id: "u".into(), seq: 9 }.wire()).unwrap();
    assert_eq!(wire["ClearNeedsYou"], serde_json::json!({"routine_id": "r", "run_id": "u", "seq": 9}));
}

#[test]
fn the_right_click_menu_of_an_inbox_row_has_the_same_actions_and_open() {
    let mut rt = runtime(scene("live"));
    rt.update(|s, _| {
        let mut item = inbox_item(9, Some("Send it"));
        item.link = Some("https://example.com/x".into());
        s.neko.inbox = vec![item];
        s.neko.loaded = true;
    });
    settle(&mut rt);
    right_click(&mut rt, "inbox-9");
    assert!(has_text(&rt, "Use the default: Send it") && has_text(&rt, "Mark done") && has_text(&rt, "Open"), "{:?}", texts(&rt));
    click_text(&mut rt, "Mark done");
    assert!(matches!(rt.state.outbox.last(), Some(Call::ClearNeedsYou { seq: 9, .. })));
}

#[test]
fn closed_recently_is_folded_shows_why_each_closed_and_reopen_sends_reopen_needs_you() {
    let mut rt = runtime(scene("live"));
    rt.update(|s, _| {
        let now = s.now_ms;
        let mut closed = inbox_item(5, None);
        closed.closed_ms = Some(now - 3_600_000);
        closed.reason = "PR merged".into();
        s.neko.closed = vec![closed];
        s.neko.loaded = true;
    });
    settle(&mut rt);
    assert!(has_text(&rt, "Closed recently"));
    assert!(!has_key(&rt, "closed-5"), "folded by default");
    click(&mut rt, "closed-toggle");
    assert!(has_key(&rt, "closed-5"));
    assert_eq!(rt.elements().into_iter().find(|e| e.key.as_deref() == Some("closed-reason-5")).and_then(|e| e.text).as_deref(), Some("PR merged · 1h"));
    // It is after the Inbox section.
    assert!(rect(&rt, "closed")[1] > rect(&rt, "inbox")[1]);
    rt.state.outbox.clear();
    click(&mut rt, "closed-reopen-5");
    assert_eq!(rt.state.outbox, vec![Call::ReopenNeedsYou { routine_id: "r-messages".into(), run_id: "run-5".into(), seq: 5 }]);
    let wire = serde_json::to_value(rt.state.outbox[0].wire()).unwrap();
    assert_eq!(wire["ReopenNeedsYou"]["seq"], 5);
}

#[test]
fn the_inbox_reads_open_items_and_the_last_three_days_of_closed_ones_from_the_routines_answer() {
    use neko_protocol::routines::{NeedsYou, RoutineRun};
    let item = |seq: u64, done: bool, closed_ms: Option<i64>, reason: Option<&str>| NeedsYou { seq, title: format!("t{seq}"), done, closed_ms, reason: reason.map(str::to_owned), ..Default::default() };
    let now = 10 * 86_400_000;
    let run = RoutineRun {
        id: "run".into(),
        routine_id: "r".into(),
        needs_you: vec![item(1, false, None, None), item(2, true, Some(now - 86_400_000), Some("Done by you")), item(3, true, Some(now - 4 * 86_400_000), Some("Expired")), item(4, true, Some(now - 3 * 86_400_000), Some("Aged out"))],
        ..Default::default()
    };
    let mut view: neko_protocol::routines::RoutineView = serde_json::from_value(serde_json::json!({
        "routine": {"id": "r", "workspace_id": "w", "kind": "Messages", "name": "Messages", "instruction": "x", "cadence": {"Daily": {"hour": 9, "minute": 0}}, "enabled": true},
        "state": {"last_run_ms": null, "next_due_ms": null, "quiet_streak": 0, "running": false},
        "runs": [],
    }))
    .expect("a routine view");
    view.runs = vec![run];
    let items = neko_rune::link::inbox_items(&[view], now);
    let shape: Vec<(u64, bool)> = items.iter().map(|i| (i.seq, i.closed_ms.is_some())).collect();
    assert_eq!(shape, [(1, false), (2, true), (4, true)], "open first; closed within 3 days (the 3-day edge stays in), newest closed first; older ones are gone");
    assert_eq!(items[1].reason, "Done by you");
    assert_eq!((items[1].routine_id.as_str(), items[1].run_id.as_str()), ("r", "run"));
}

// ------------------------------------------------------------------------------------------------ 6. routines and calendar

fn routines_open(app: &mut App) {
    let (_, f) = scenes::SCENES.iter().find(|(n, _)| *n == "routines-open").unwrap();
    f(app);
    app.outbox.clear();
}

#[test]
fn the_routine_editor_has_every_and_daily_with_the_webs_fields_and_saves_with_save_routine() {
    use neko_protocol::routines::Cadence;
    let mut rt = runtime(routines_open);
    assert!(has_key(&rt, "rt-cadence-mode") && has_key(&rt, "rt-minutes") && has_key(&rt, "pick-rt-from-pick") && has_key(&rt, "pick-rt-to-pick"), "Every: minutes, From, Until");
    assert!(!has_key(&rt, "rt-time"));
    // Minutes: the stepper moves by 5, typed numbers are kept between 5 and 720.
    click(&mut rt, "rt-minutes-more");
    assert_eq!(rt.state.pages.text("rt:minutes"), "25");
    click(&mut rt, "rt-minutes-less");
    click(&mut rt, "rt-minutes-less");
    assert_eq!(rt.state.pages.text("rt:minutes"), "15");
    rt.update(|s, _| {
        s.pages.set_field("rt:minutes", "9999");
        s.cadence_minutes_typed();
    });
    assert!(matches!(rt.state.pages.draft.as_ref().unwrap().cadence, Cadence::Every { minutes: 720, .. }), "at most 720");
    rt.update(|s, _| {
        s.pages.set_field("rt:minutes", "30");
        s.cadence_minutes_typed();
        s.cadence_hours(Some(9), Some(18));
    });
    settle(&mut rt);
    click(&mut rt, "rt-save");
    let Some(Call::SaveRoutine(saved)) = rt.state.outbox.last().cloned() else { panic!("{:?}", rt.state.outbox) };
    assert_eq!(saved.cadence, Cadence::Every { minutes: 30, from_hour: 9, to_hour: 18 });
    // The wire is the web's: SaveRoutine { routine } with the cadence as { Every: { ... } }.
    let wire = serde_json::to_value(Call::SaveRoutine(saved).wire()).unwrap();
    assert_eq!(wire["SaveRoutine"]["routine"]["cadence"], serde_json::json!({"Every": {"minutes": 30, "from_hour": 9, "to_hour": 18}}));
    // Daily: an HH:MM field.
    let mode = rect(&rt, "rt-cadence-mode");
    rt.pointer_down(mode[0] + mode[2] * 0.75, mode[1] + mode[3] / 2.0);
    rt.pointer_up(mode[0] + mode[2] * 0.75, mode[1] + mode[3] / 2.0);
    settle(&mut rt);
    assert!(has_key(&rt, "rt-time") && !has_key(&rt, "rt-minutes"));
    assert_eq!(rt.state.pages.text("rt:time"), "10:30", "the web's default time");
    rt.update(|s, _| {
        s.pages.set_field("rt:time", "07:45");
        s.cadence_time_typed();
    });
    settle(&mut rt);
    click(&mut rt, "rt-save");
    let Some(Call::SaveRoutine(saved)) = rt.state.outbox.last().cloned() else { panic!() };
    assert_eq!(saved.cadence, Cadence::Daily { hour: 7, minute: 45 });
    // A time that is not a time leaves the last good one.
    rt.update(|s, _| {
        s.pages.set_field("rt:time", "25:99");
        s.cadence_time_typed();
    });
    assert_eq!(rt.state.pages.draft.as_ref().unwrap().cadence, Cadence::Daily { hour: 7, minute: 45 });
}

#[test]
fn month_is_a_six_by_seven_grid_with_28_point_date_headers_three_chips_and_a_plus_n() {
    let mut rt = runtime(scene("live"));
    rt.update(|s, _| {
        s.open_view(neko_rune::views::FullView::Calendar);
        s.cal_mode(neko_rune::pages::CalMode::Month);
        // A month's worth of data: five events on the 15th.
        let (from, to) = s.calendar_range();
        let mid = neko_rune::clock::add_days(from, 17);
        let status = neko_protocol::calendar::CalendarStatus { access: neko_protocol::calendar::CalendarAccess::Granted, source: "macOS Calendar".into(), last_sync_ms: None, events: 5, truncated: false, enabled: true };
        let events: Vec<neko_protocol::calendar::CalendarEvent> = (0..5)
            .map(|i| neko_protocol::calendar::CalendarEvent { id: format!("m{i}"), title: format!("Meeting {i}"), start_ms: mid + (9 + i) * 3_600_000, end_ms: mid + (10 + i) * 3_600_000, ..Default::default() })
            .collect();
        s.pages.calendar = Some(neko_rune::link::CalendarData { events, neko: Vec::new(), status, from_ms: from, to_ms: to });
    });
    settle(&mut rt);
    assert!(has_key(&rt, "cal-month"));
    let (from, to) = rt.state.calendar_range();
    assert_eq!(((to - from) as f64 / 86_400_000.0).round() as i64, 42, "6 x 7 days");
    let cells: Vec<String> = rt.elements().into_iter().filter_map(|e| e.key).filter(|k| k.starts_with("cal-cell-")).collect();
    assert_eq!(cells.len(), 42);
    let day = neko_rune::clock::add_days(from, 17);
    let number = rect(&rt, &format!("cal-day-{day}"));
    assert_eq!(number[3], 28.0, "a 28 point date header");
    let chips: usize = rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("event-m"))).count();
    assert_eq!(chips, 3, "up to three chips");
    assert!(has_key(&rt, &format!("cal-more-{day}")) && has_text(&rt, "2") && has_text(&rt, " more"), "then the web's \"2 more\"");
    // The web's page: the month as the title with no count, then Today, the arrows and Day / Week / Month in that order; the weekday
    // line, and a "W<week>" label at the start of each of the six weeks.
    let y_of = |rt: &Runtime<App>, k: &str| rect(rt, k)[0];
    assert!(has_text(&rt, &neko_rune::clock::month_label(neko_rune::clock::day_start(rt.state.now_ms))), "the month is the title");
    assert!(rt.find("page-count").is_none(), "a month has no 'Week N' beside its title");
    assert!(y_of(&rt, "cal-today") < y_of(&rt, "cal-prev") && y_of(&rt, "cal-prev") < y_of(&rt, "cal-next") && y_of(&rt, "cal-next") < y_of(&rt, "cal-mode"), "Today, the arrows, the switch");
    for name in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] {
        assert!(has_text(&rt, name), "{name}");
    }
    assert_eq!(rt.elements().iter().filter(|e| e.text.as_deref() == Some("W")).count(), 6, "a week label for each of the six weeks");
    // A header opens that day.
    click(&mut rt, &format!("cal-day-{day}"));
    assert_eq!(rt.state.pages.cal_mode, neko_rune::pages::CalMode::Day);
    assert_eq!(rt.state.pages.cal_day, day);
}

#[test]
fn an_event_opens_a_popover_with_join_meeting_for_a_meeting_link_and_open_link_for_another() {
    assert!(neko_rune::view::is_meeting("https://meet.google.com/abc-defg-hij"));
    assert!(neko_rune::view::is_meeting("https://us02web.zoom.us/j/123"), "a subdomain of a meeting host");
    assert!(neko_rune::view::is_meeting("https://teams.microsoft.com/l/meetup-join/x"));
    assert!(!neko_rune::view::is_meeting("https://example.com/zoom.us"), "the host decides, not the path");
    assert!(!neko_rune::view::is_meeting("https://notzoom.us/j/1"));
    let mut rt = runtime(scene("live"));
    rt.update(|s, _| {
        s.open_view(neko_rune::views::FullView::Calendar);
        s.cal_mode(neko_rune::pages::CalMode::Day);
        let (from, to) = s.calendar_range();
        let status = neko_protocol::calendar::CalendarStatus { access: neko_protocol::calendar::CalendarAccess::Granted, source: "macOS Calendar".into(), last_sync_ms: None, events: 2, truncated: false, enabled: true };
        let ev = |id: &str, title: &str, h: i64, url: Option<&str>| neko_protocol::calendar::CalendarEvent {
            id: id.into(),
            title: title.into(),
            start_ms: from + h * 3_600_000,
            end_ms: from + (h + 1) * 3_600_000,
            location: Some("Room 4".into()),
            url: url.map(str::to_owned),
            ..Default::default()
        };
        s.pages.calendar = Some(neko_rune::link::CalendarData { events: vec![ev("a", "Standup", 9, Some("https://meet.google.com/abc-defg-hij")), ev("b", "Docs review", 11, Some("https://example.com/doc")), ev("c", "Lunch", 12, None)], neko: Vec::new(), status, from_ms: from, to_ms: to });
    });
    settle(&mut rt);
    click(&mut rt, "event-a");
    assert!(has_key(&rt, "cal-selected"));
    assert!(has_text(&rt, "Standup") && has_text(&rt, "Room 4") && has_text(&rt, "Join meeting"), "{:?}", texts(&rt));
    assert_eq!(fill_of(&rt, "cal-event-link"), neko_rune::theme::theme(true).mat.ink_bottom.hex(), "Join meeting is the primary button (the ink)");
    click(&mut rt, "cal-event-link");
    assert!(!has_key(&rt, "cal-selected"), "opening the link puts the popover away");
    click(&mut rt, "event-b");
    assert!(has_text(&rt, "Open link") && !has_text(&rt, "Join meeting"));
    click(&mut rt, "scrim-event-popover");
    click(&mut rt, "event-c");
    assert!(!has_key(&rt, "cal-event-link"), "no address, no button");
}
