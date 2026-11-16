//! The session view, the Neko column and the sessions column as used: real layout, real
//! hit-testing, scripted pointer and keys, and what each press asks of the daemon read from
//! `App::outbox` (there is nowhere to send it). The same flows against a real daemon are in
//! `tests/harness.rs`.

use neko_protocol::crew::DeliveryMode;
use neko_protocol::workbench::TaskStatus;
use neko_rune::app::{App, Steer};
use neko_rune::link::{Call, LinkEvent, Reply};
use neko_rune::scenes;
use neko_rune::view::view;
use neko_rune::work::UndoOptions;
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const WINDOW: (f32, f32) = (1360.0, 900.0);

fn scene(name: &str) -> impl FnOnce(&mut App) + '_ {
    move |app| {
        let (_, f) = scenes::SCENES.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no scene {name}"));
        f(app);
        app.outbox.clear();
    }
}

fn runtime(setup: impl FnOnce(&mut App)) -> Runtime<App> {
    let mut app = App::headless();
    setup(&mut app);
    let mut rt = Runtime::new(app, view, WINDOW, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    settle(&mut rt);
    rt
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

fn press(rt: &mut Runtime<App>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    settle(rt);
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

fn has_text(rt: &Runtime<App>, needle: &str) -> bool {
    texts(rt).iter().any(|t| t.contains(needle))
}

fn keyed(rt: &Runtime<App>, prefix: &str) -> usize {
    rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with(prefix))).count()
}

fn type_into(rt: &mut Runtime<App>, key: &str, text: &str) {
    click(rt, key);
    rt.paste(text);
    settle(rt);
}

#[test]
fn enter_sends_a_reply_and_shift_enter_is_a_new_line_and_a_refusal_keeps_the_words() {
    let mut rt = runtime(scene("live"));
    assert_eq!(rt.state.session().map(|t| t.status), Some(TaskStatus::AwaitingApproval));
    type_into(&mut rt, "composer-input", "use three tries");
    press(&mut rt, Key::Enter, Mods { shift: true, ..Mods::default() });
    assert!(rt.state.outbox.is_empty(), "Shift+Enter is a new line, not a send");
    assert!(rt.state.composer.text.contains('\n'));
    rt.update(|s, _| s.composer.replace_text("use three tries"));
    settle(&mut rt);
    press(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.outbox, vec![Call::Reply { task_id: "t0".into(), text: "use three tries".into() }], "a task between turns takes a reply");
    // The text stays until the daemon takes it; when it refuses, the words are still there.
    rt.update(|s, _| {
        let call = s.outbox[0].clone();
        s.apply_reply(&call, Reply::Refused("The agent still has 32 unread replies.".into()));
    });
    settle(&mut rt);
    assert_eq!(rt.state.composer.text, "use three tries");
    assert!(has_text(&rt, "The agent still has 32 unread replies. Your text is still here."));
    // And is cleared when it takes it.
    rt.update(|s, _| {
        let call = s.outbox[0].clone();
        s.apply_reply(&call, Reply::Snapshot(scenes::demo_full("t0")));
    });
    settle(&mut rt);
    assert!(rt.state.composer.text.is_empty());
}

#[test]
fn while_a_turn_runs_enter_queues_after_the_turn_and_command_enter_steers_now() {
    let mut rt = runtime(scene("running"));
    assert_eq!(rt.state.session().map(|t| t.status), Some(TaskStatus::Building));
    type_into(&mut rt, "composer-input", "check the dark theme too");
    press(&mut rt, Key::Enter, Mods::default());
    let Call::Enqueue { task_id, command_id, text, mode } = rt.state.outbox[0].clone() else { panic!("{:?}", rt.state.outbox) };
    assert_eq!((task_id.as_str(), text.as_str(), mode), ("t6", "check the dark theme too", DeliveryMode::AfterTurn));
    // A retry of the same text and mode is the same message: the daemon queues one.
    press(&mut rt, Key::Enter, Mods::default());
    let Call::Enqueue { command_id: again, .. } = rt.state.outbox[1].clone() else { panic!() };
    assert_eq!(command_id, again);
    // ⌘↩ steers now, whatever the menu says; the menu's choice is the default for Enter.
    press(&mut rt, Key::Enter, Mods { cmd: true, ..Mods::default() });
    let Call::Enqueue { mode, command_id: steer_id, .. } = rt.state.outbox[2].clone() else { panic!() };
    assert_eq!(mode, DeliveryMode::SteerNow);
    assert_ne!(steer_id, command_id, "another mode is another message");
    rt.update(|s, _| s.steer = Steer::Interrupt);
    press(&mut rt, Key::Enter, Mods::default());
    assert!(matches!(&rt.state.outbox[3], Call::Enqueue { mode: DeliveryMode::InterruptRestart, .. }));
}

#[test]
fn queued_messages_are_one_line_each_with_cancel() {
    let mut rt = runtime(scene("running"));
    assert_eq!(keyed(&rt, "queued-"), 2);
    assert!(has_text(&rt, "Queued · after the turn") && has_text(&rt, "Queued · steer now"));
    click(&mut rt, "cancel-queued-q2");
    assert_eq!(rt.state.outbox, vec![Call::CancelQueued { task_id: "t6".into(), id: "q2".into() }]);
}

#[test]
fn a_tool_card_is_one_line_and_opens_to_its_command_and_output() {
    let mut rt = runtime(scene("running"));
    assert!(rt.find("tool-receipt-5").is_some());
    assert!(!has_text(&rt, "test result: FAILED"), "closed, a card is one line");
    click(&mut rt, "tool-receipt-5");
    assert!(has_text(&rt, "$ cargo test -p board") && has_text(&rt, "test result: FAILED. 11 passed; 1 failed") && has_text(&rt, "exit 101"));
    click(&mut rt, "tool-receipt-5");
    assert!(!has_text(&rt, "test result: FAILED"));
}

#[test]
fn an_edit_card_opens_to_its_diff() {
    let mut rt = runtime(scene("running"));
    let tool = neko_rune::turns::blocks_of_parts(&[serde_json::json!({
        "type": "dynamic-tool", "toolName": "Edit", "toolCallId": "e9", "state": "output-available",
        "input": {"file_path": "/x/src/export.sh", "old_string": "run_export\n", "new_string": "test -e $lock && exit 0\nrun_export\n"}, "output": {}
    })]);
    let neko_rune::turns::Block::Tool(tool) = &tool[0] else { panic!() };
    rt.update(|s, _| s.toggle_tool(tool));
    assert!(rt.state.work.edit_diffs.get("e9").is_some_and(|d| d.files().len() == 1 && d.files()[0].path().ends_with("export.sh")));
}

#[test]
fn the_goal_line_opens_to_its_ledger_and_journal_and_its_actions_ask_the_daemon() {
    let mut rt = runtime(scene("running"));
    assert!(has_text(&rt, "turn 4/20 · check failing (exit 101)"), "{:?}", texts(&rt));
    assert!(!has_text(&rt, "Evidence"));
    click(&mut rt, "goal-g1");
    for word in ["Evidence", "Journal", "Wrote the failing test.", "cargo test -p board", "Judge"] {
        assert!(has_text(&rt, word), "{word}: {:?}", texts(&rt));
    }
    click(&mut rt, "goal-g1-Pause");
    click(&mut rt, "goal-g1-Extend");
    click(&mut rt, "goal-g1-Done");
    click(&mut rt, "goal-g1-Stop");
    assert_eq!(
        rt.state.outbox,
        vec![
            Call::GoalPause { id: "g1".into() },
            Call::GoalExtend { id: "g1".into(), turns: 10 },
            Call::GoalDone { id: "g1".into() },
            Call::GoalStop { id: "g1".into() },
        ],
        "Pause, Extend by half the turns, Mark done, Stop"
    );
}

#[test]
fn a_blocked_goal_shows_its_question_and_one_press_takes_the_default() {
    let mut rt = runtime(scene("answer"));
    assert!(has_text(&rt, "Should the retry also cover timeouts?"));
    click(&mut rt, "goal-default");
    assert_eq!(rt.state.outbox, vec![Call::GoalAnswer { id: "g1".into(), text: String::new() }], "empty words take the goal's default");
    type_into(&mut rt, "goal-answer", "No, only connection errors");
    press(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.outbox[1], Call::GoalAnswer { id: "g1".into(), text: "No, only connection errors".into() });
}

#[test]
fn a_run_opens_to_its_own_diff_and_undo_asks_first() {
    let mut rt = runtime(scene("running"));
    for n in 1..=3 {
        assert!(rt.find(&format!("run-{n}")).is_some());
    }
    assert!(has_text(&rt, "4 files") && has_text(&rt, "+120") && has_text(&rt, "−30"), "Run 2 · 4 files · +120 −30: {:?}", texts(&rt));
    click(&mut rt, "run-2");
    assert_eq!(rt.state.outbox, vec![Call::CheckpointDiff { task_id: "t6".into(), run: Some(2) }]);
    click(&mut rt, "undo-run-3");
    assert!(has_text(&rt, "Undo run 3?") && has_text(&rt, "A safety checkpoint is taken first"), "{:?}", texts(&rt));
    assert_eq!(rt.state.outbox.len(), 1, "nothing was sent before the answer");
    click(&mut rt, "confirm-no");
    assert_eq!(rt.state.outbox.len(), 1);
    click(&mut rt, "undo-run-3");
    click(&mut rt, "confirm-yes");
    assert_eq!(rt.state.outbox[1], Call::Undo { task_id: "t6".into(), run: 3, through_later: false, snapshot_unsaved: false, revert: false });
}

#[test]
fn a_refusal_says_why_and_offers_what_would_work() {
    let mut rt = runtime(scene("undo-refused"));
    assert!(has_text(&rt, "Run 3 came after run 2 and has not been undone."));
    assert!(rt.find("offer-through").is_some() && rt.find("offer-revert").is_none());
    click(&mut rt, "offer-through");
    assert!(has_text(&rt, "Undo back to run 2?"));
    click(&mut rt, "confirm-yes");
    assert_eq!(rt.state.outbox, vec![Call::Undo { task_id: "t6".into(), run: 2, through_later: true, snapshot_unsaved: false, revert: false }]);
    // Each code offers its own way out.
    for (code, offer) in [("unsaved_changes", "offer-snapshot"), ("pushed", "offer-revert"), ("foreign_commits", "offer-revert")] {
        rt.update(|s, _| s.work.refusal = neko_rune::work::parse_refusal(&format!("checkpoint:{code}: no"), UndoOptions { run: 2, ..Default::default() }));
        settle(&mut rt);
        assert!(rt.find(offer).is_some(), "{code} offers {offer}");
    }
}

#[test]
fn plan_approval_completion_and_stop_are_the_webs_actions() {
    let mut rt = runtime(scene("live"));
    click(&mut rt, "approve");
    assert_eq!(rt.state.outbox, vec![Call::Approve { task_id: "t0".into() }]);
    let mut rt = runtime(scene("running"));
    click(&mut rt, "stop");
    assert!(has_text(&rt, "Stop “Fix the empty state on the board”?") && rt.state.outbox.is_empty(), "Stop asks first");
    // The one dialog every destructive press goes through (`task_actions.rs`).
    click(&mut rt, "ask-yes");
    assert_eq!(rt.state.outbox, vec![Call::Stop { task_id: "t6".into() }]);
}

#[test]
fn a_new_session_asks_for_words_then_makes_the_ticket_the_chat_makes() {
    let mut rt = runtime(scene("live"));
    click(&mut rt, "new-session");
    assert!(rt.state.outbox.contains(&Call::Models), "the runtimes are read when Home opens");
    assert!(rt.find("home-composer").is_some() && rt.find("composer").is_none(), "Home takes the session's place");
    click(&mut rt, "new-start");
    assert!(!rt.state.outbox.iter().any(|c| matches!(c, Call::CreateTask { .. })), "nothing is made without words");
    type_into(&mut rt, "new-request", "Make the sync job retry when a request fails.\nThree tries.");
    click(&mut rt, "new-start");
    let made = rt.state.outbox.iter().find_map(|c| if let Call::CreateTask { workspace_id, title, goal } = c { Some((workspace_id.clone(), title.clone(), goal.clone())) } else { None }).expect("a ticket was asked for");
    assert_eq!(made.0, "w1");
    assert_eq!(made.1, "Make the sync job retry when a request fails");
    assert_eq!(made.2, "Make the sync job retry when a request fails.\nThree tries.", "the request is kept in the person's words");
    // The answer opens the new ticket, pins the runtime and model and, in Build, starts it.
    rt.update(|s, _| {
        let form = s.new_form.as_mut().unwrap();
        form.mode = neko_rune::new_session::Mode::Build;
        form.runtime = Some("claude".into());
        let mut m = scenes::demo_full("t0");
        let mut row = m.tasks[0].clone();
        row.id = "t99".into();
        row.title = "Make the sync job retry when a request fails".into();
        row.updated_at_ms += 1;
        m.tasks.push(row);
        let call = Call::CreateTask { workspace_id: "w1".into(), title: "x".into(), goal: "y".into() };
        s.apply_reply(&call, Reply::Snapshot(m));
    });
    settle(&mut rt);
    assert!(rt.state.new_form.is_none());
    assert_eq!(rt.state.selected.as_deref(), Some("t99"));
    assert!(rt.state.outbox.iter().any(|c| matches!(c, Call::SetRuntime { task_id, provider, model } if task_id == "t99" && provider == "claude" && model.is_empty())), "{:?}", rt.state.outbox);
    assert!(rt.state.outbox.iter().any(|c| matches!(c, Call::Start { task_id } if task_id == "t99")), "Build starts the ticket: {:?}", rt.state.outbox);
}

#[test]
fn the_palette_jumps_to_a_session_by_a_few_letters() {
    let mut rt = runtime(scene("live"));
    rt.update(|s, _| s.palette_toggle());
    settle(&mut rt);
    assert!(rt.find("palette-input").is_some());
    rt.paste("settings");
    settle(&mut rt);
    let rows: Vec<String> = rt.elements().into_iter().filter_map(|e| e.key).filter(|k| k.starts_with("palette-row-")).collect();
    assert!(!rows.is_empty() && rows.len() < 22, "narrowed to a few: {rows:?}");
    press(&mut rt, Key::Down, Mods::default());
    press(&mut rt, Key::Up, Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    assert!(rt.state.palette.is_none(), "choosing closes it");
    let chosen = rt.state.selected.clone().unwrap();
    assert!(rt.state.summary.as_ref().unwrap().task(&chosen).unwrap().title.to_lowercase().contains("settings"));
    assert!(rt.state.outbox.contains(&Call::Refresh), "opening it asks the daemon for its events");
}

#[test]
fn a_row_has_a_right_click_menu() {
    let mut rt = runtime(scene("live"));
    let (x, y) = rt.find("session-t6").unwrap();
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.row_menu.as_ref().map(|(id, _)| id.as_str()), Some("t6"));
    for item in ["Mark as unread", "Stop", "Open in web UI"] {
        assert!(has_text(&rt, item), "{item}: {:?}", texts(&rt));
    }
    let e = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Mark as unread")).unwrap();
    rt.pointer_down(e.rect[0] + 4.0, e.rect[1] + e.rect[3] / 2.0);
    rt.pointer_up(e.rect[0] + 4.0, e.rect[1] + e.rect[3] / 2.0);
    settle(&mut rt);
    assert!(rt.state.row_menu.is_none());
    assert!(rt.state.outbox.contains(&Call::MarkSeen { task_id: "t6".into(), seen: false }));
    assert!(rt.find("unseen-t6").is_some(), "the dot is back at once");
}

#[test]
fn finished_work_carries_a_dot_until_it_is_opened() {
    let mut rt = runtime(scene("live"));
    assert!(rt.find("unseen-t2").is_some() && rt.find("unseen-t3").is_some() && rt.find("unseen-t4").is_none());
    click(&mut rt, "session-t2");
    assert!(rt.find("unseen-t2").is_none() && rt.find("unseen-t3").is_some());
    assert!(rt.state.outbox.contains(&Call::MarkSeen { task_id: "t2".into(), seen: true }));
}

#[test]
fn up_and_down_move_between_rows_and_return_opens_one() {
    let mut rt = runtime(scene("live"));
    click(&mut rt, "session-t0");
    press(&mut rt, Key::Down, Mods::default());
    press(&mut rt, Key::Down, Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.selected.as_deref(), Some("t2"), "two rows down, opened with Return");
    press(&mut rt, Key::Up, Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.selected.as_deref(), Some("t1"));
}

#[test]
fn two_states_with_the_same_glyph_are_told_apart_in_words() {
    // As in the web UI, "needs you" and "working" are both the amber half circle (DESIGN.md §3);
    // the row says which in words, and a row's name for assistive technology does too.
    let rt = runtime(scene("live"));
    // The column's rows show an icon and an age or a fraction now; the words live in the row's name for assistive
    // technology and its tooltip, and the icons differ (a list, a ring).
    let name = |id: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(&format!("session-{id}"))).and_then(|e| e.label).unwrap_or_default();
    assert!(name("t0").contains("Needs you"), "{}", name("t0"));
    assert!(name("t6").contains("Building"), "{}", name("t6"));
    assert!(name("t9").contains("Planning"), "planning is an agent at work too: {}", name("t9"));
    assert!(name("t10").contains("Reviewing"), "{}", name("t10"));
    let meta = |id: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(&format!("meta-{id}"))).and_then(|e| e.text);
    assert_eq!(meta("t3").as_deref(), Some("23m"), "a needs-you row says its age");
    assert_eq!(meta("t6").as_deref(), Some("2/7"), "a running row says its fraction");
    let label = |id: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(&format!("session-{id}"))).and_then(|e| e.label).unwrap();
    assert!(label("t0").contains("Needs you") && label("t6").contains("Building"), "{} / {}", label("t0"), label("t6"));
}

#[test]
fn a_pasted_image_is_kept_and_referenced_in_the_message() {
    let dir = std::env::temp_dir().join(format!("neko-rune-paste-{}", std::process::id()));
    // SAFETY: this test is the only one in this binary that reads the variable.
    unsafe { std::env::set_var("NEKO_RUNE_ATTACHMENTS", &dir) };
    let mut rt = runtime(scene("live"));
    click(&mut rt, "composer-input");
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 2, 2);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[255u8; 16]).unwrap();
    }
    let image = rune::ClipboardImage::from_png(png.clone()).expect("a PNG");
    rt.paste_content(rune::ClipboardContent::new("").with_image(image));
    settle(&mut rt);
    let text = rt.state.composer.text.clone();
    assert!(text.starts_with("![Image](file://") && text.contains(&dir.to_string_lossy().into_owned()), "{text}");
    let path = text.trim_start_matches("![Image](file://").trim_end_matches(')');
    assert_eq!(std::fs::read(path).unwrap(), png);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_empty_digest_and_inbox_say_nothing_needs_you() {
    let rt = runtime(scene("empty"));
    assert!(has_text(&rt, "Nothing needs you") && rt.find("digest-empty").is_some());
}

#[test]
fn the_digest_opens_to_its_lists_and_marks_read() {
    let mut rt = runtime(|s| {
        scene("live")(s);
        s.neko.inbox.truncate(1);
    });
    assert!(has_text(&rt, "3 things finished, 1 thing is stuck"));
    assert!(!has_text(&rt, "Needs a decision"));
    // Fold the Inbox so the opened digest has room (the top of the column scrolls at its cap).
    click(&mut rt, "inbox-toggle");
    click(&mut rt, "digest-toggle");
    for word in ["Finished", "Stuck", "Needs a decision", "default: Send the numbers from last week", "3 events today"] {
        assert!(has_text(&rt, word), "{word}");
    }
    click(&mut rt, "digest-read");
    assert_eq!(rt.state.outbox, vec![Call::MarkDigestRead]);
    assert!(has_text(&rt, "Nothing needs you"), "once read, the block says so");
}

#[test]
fn a_live_snapshot_with_a_finished_reply_stops_asking() {
    let mut rt = runtime(scene("live"));
    assert!(has_text(&rt, "Neko is replying…"));
    rt.update(|s, _| {
        let mut m = scenes::demo_full("t0");
        if let Some(last) = m.conversation.last_mut() {
            last.pending = false;
        }
        s.apply_link(LinkEvent::Live(m));
    });
    settle(&mut rt);
    assert!(!has_text(&rt, "Neko is replying…") && rt.find("ask-stop").is_none());
}

#[test]
fn a_double_press_sends_a_write_once_until_the_daemon_answers() {
    // Reply.
    let mut rt = runtime(scene("live"));
    type_into(&mut rt, "composer-input", "use three tries");
    press(&mut rt, Key::Enter, Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.outbox.iter().filter(|c| matches!(c, Call::Reply { .. })).count(), 1, "{:?}", rt.state.outbox);
    // Ask Neko.
    type_into(&mut rt, "ask", "what needs me?");
    press(&mut rt, Key::Enter, Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.outbox.iter().filter(|c| matches!(c, Call::Ask { .. })).count(), 1);
    // A new session.
    let mut rt = runtime(scene("live"));
    click(&mut rt, "new-session");
    type_into(&mut rt, "new-request", "Make the sync job retry");
    click(&mut rt, "new-start");
    click(&mut rt, "new-start");
    assert_eq!(rt.state.outbox.iter().filter(|c| matches!(c, Call::CreateTask { .. })).count(), 1);
}

#[test]
fn a_task_filed_in_the_same_moment_is_not_mistaken_for_the_new_session() {
    let mut rt = runtime(scene("live"));
    click(&mut rt, "new-session");
    type_into(&mut rt, "new-request", "Make the sync job retry");
    click(&mut rt, "new-start");
    rt.update(|s, _| {
        let mut m = scenes::demo_full("t0");
        let mut other = m.tasks[0].clone();
        other.id = "watch".into();
        other.title = "Something a watch filed".into();
        other.updated_at_ms += 10;
        let mut ours = m.tasks[0].clone();
        ours.id = "ours".into();
        ours.title = "Make the sync job retry".into();
        m.tasks.extend([ours, other]);
        let call = Call::CreateTask { workspace_id: "w1".into(), title: "x".into(), goal: "y".into() };
        s.apply_reply(&call, Reply::Snapshot(m));
    });
    assert_eq!(rt.state.selected.as_deref(), Some("ours"), "matched by its title, not by being the newest");
}

#[test]
fn an_action_neko_proposes_is_decided_with_the_web_s_request() {
    let mut rt = runtime(scene("live"));
    rt.update(|s, _| {
        let m = s.summary.as_mut().unwrap();
        m.conversation.last_mut().unwrap().parts = vec![
            serde_json::json!({"type": "dynamic-tool", "toolName": "neko_task_action", "toolCallId": "action:act-1", "state": "approval-requested", "input": {"taskId": "t6", "kind": "message", "text": "Rebase it"}, "approval": {"id": "act-1"}}),
            serde_json::json!({"type": "dynamic-tool", "toolName": "save_issue", "toolCallId": "call-1", "state": "approval-requested", "input": {"title": "x"}, "approval": {"id": "call-1"}}),
        ];
    });
    settle(&mut rt);
    assert!(has_text(&rt, "message · Rebase it"), "what a click approves is shown");
    click(&mut rt, "neko-approve-action:act-1");
    click(&mut rt, "neko-decline-call-1");
    assert_eq!(
        rt.state.outbox,
        vec![
            Call::DecideChatAction { message_id: "m4".into(), action_id: "act-1".into(), accept: true },
            Call::DecideChatTool { turn_id: "m4".into(), call_id: "call-1".into(), approve: false },
        ]
    );
}
