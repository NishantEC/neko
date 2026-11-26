//! The keyboard, with real key events through the real app: Tab once round every control in reading order, the arrows in the
//! sidebar's list, Return to open, Return to send and Shift+Return for a new line, Escape to give focus back, ⌘N and ⌃⌘S.

mod support;
use rune::{Key, Mods};
use support::*;

fn scene(name: &str) -> fn(&mut neko_app::state::App) {
    scenes().iter().find(|s| s.0 == name).unwrap().1
}

fn focus(rt: &rune::Runtime<neko_app::state::App>) -> Option<String> {
    rt.presented_focus_key().map(str::to_owned)
}

#[test]
fn tab_reaches_every_control_once_in_reading_order() {
    let mut rt = runtime((1200.0, 800.0), false, scene("tasks-home"));
    let mut seen = Vec::new();
    for _ in 0..20 {
        tab(&mut rt, false);
        let Some(k) = focus(&rt) else { continue };
        if seen.first() == Some(&k) {
            break;
        }
        seen.push(k);
    }
    assert_eq!(
        seen,
        [
            "sidebar-hide",
            "new-task",
            "home",
            "briefing",
            "needs-you",
            "rounds",
            "ws-default",
            "ws-default-new",
            "add-workspace",
            "cat-cat_miso",
            "new-cat",
            "composer-input",
            "cat-picker",
            "workspace-picker",
            "cat-tile-cat_miso",
            "cat-tile-cat_mochi",
            "cat-tile-cat_tofu"
        ],
        "Home, then the Briefing, then Needs you, then Rounds; the sections and their rows are one stop; a section's New task is its own; the cats are one stop, then New cat; the words, then the cat picker and the workspace picker in the composer's footer (the empty composer's Send is off); then each cat's tile in the house"
    );
    // Round once, the focus is back on the first control; Shift+Tab walks back the same way, wrapping.
    assert_eq!(focus(&rt).as_deref(), Some("sidebar-hide"));
    tab(&mut rt, true);
    assert_eq!(focus(&rt).as_deref(), Some("cat-tile-cat_tofu"));
    tab(&mut rt, true);
    assert_eq!(focus(&rt).as_deref(), Some("cat-tile-cat_mochi"));
}

#[test]
fn the_arrows_move_in_the_list_and_return_opens() {
    let mut rt = runtime((1200.0, 800.0), false, scene("tasks-home"));
    for _ in 0..7 {
        tab(&mut rt, false);
    }
    assert_eq!(focus(&rt).as_deref(), Some("ws-default"), "with no task open, the list's stop is the first section");
    press(&mut rt, Key::Down, Mods::default());
    assert_eq!(focus(&rt).as_deref(), Some("task-task_00000000000000000000000004"), "Down goes from a section to its first row");
    press(&mut rt, Key::Down, Mods::default());
    assert_eq!(focus(&rt).as_deref(), Some("task-task_00000000000000000000000003"), "Down moves to the next row");
    assert_eq!(rt.state.open, None, "moving does not open");
    press(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.open.as_ref().map(|t| t.0.as_str()), Some("task_00000000000000000000000003"), "Return opens the row");
    press(&mut rt, Key::Escape, Mods::default());
    assert_eq!(focus(&rt), None, "Escape gives the focus back");
}

#[test]
fn the_keys_move_through_sections_fold_them_and_type_to_select() {
    let mut rt = runtime((1200.0, 800.0), false, scene("workspaces"));
    while focus(&rt).as_deref() != Some("ws-default") {
        tab(&mut rt, false);
    }
    // General has two tasks; then acme-api's header.
    for _ in 0..3 {
        press(&mut rt, Key::Down, Mods::default());
    }
    assert_eq!(focus(&rt).as_deref(), Some("ws-ws_acme"), "Down crosses from one section into the next");
    press(&mut rt, Key::Left, Mods::default());
    assert!(rt.state.folded.contains("ws_acme"), "Left folds a section");
    press(&mut rt, Key::Down, Mods::default());
    assert_eq!(focus(&rt).as_deref(), Some("ws-ws_web"), "a folded section's rows are passed over");
    press(&mut rt, Key::Up, Mods::default());
    press(&mut rt, Key::Right, Mods::default());
    assert!(!rt.state.folded.contains("ws_acme"), "Right opens it again");
    press(&mut rt, Key::Enter, Mods::default());
    assert!(rt.state.folded.contains("ws_acme"), "Return folds it too");
    press(&mut rt, Key::Text("n".into()), Mods::default());
    assert_eq!(focus(&rt).as_deref(), Some("ws-ws_notes"), "typing a name goes to it");
    assert!(rt.state.can_move("ws_web", -1) && !rt.state.can_move("ws_acme", -1), "Move up is there for all but the first");
}

#[test]
fn the_section_button_starts_a_task_there_and_command_n_follows_the_task_in_view() {
    let mut rt = runtime((1200.0, 800.0), false, scene("workspace-task"));
    assert_eq!(rt.state.window_title(), "Rename the Settings button to Preferences across the app — acme-api");
    rt.update(|s, _| s.choose_workspace(neko_protocol::GENERAL));
    press(&mut rt, Key::Command('n'), Mods { cmd: true, ..Mods::default() });
    assert_eq!((rt.state.open.as_ref(), rt.state.workspace.as_str()), (None, "ws_acme"), "⌘N: a new task in the workspace of the task in view");
    assert_eq!(focus(&rt).as_deref(), Some("composer-input"));
    click(&mut rt, "ws-ws_web-new");
    assert_eq!(rt.state.workspace, "ws_web", "a section's New task picks its workspace");
    click(&mut rt, "workspace-picker");
    assert!(rt.state.picker_open);
    settle(&mut rt);
    let rows: Vec<String> = rt.elements().into_iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("workspace-menu-item"))).filter_map(|e| e.label).collect();
    assert!(rows.iter().any(|r| r == "General") && rows.iter().any(|r| r == "Add folder…") && !rows.iter().any(|r| r.contains("old-scripts")), "{rows:?}");
}

#[test]
fn return_sends_and_shift_return_makes_a_new_line() {
    let mut rt = runtime((1200.0, 800.0), false, |_| {});
    click(&mut rt, "composer-input");
    press(&mut rt, Key::Text("Fix the build".into()), Mods::default());
    press(&mut rt, Key::Enter, Mods { shift: true, ..Mods::default() });
    press(&mut rt, Key::Text("then tell Lee".into()), Mods::default());
    assert_eq!(rt.state.draft.text, "Fix the build\nthen tell Lee");
    press(&mut rt, Key::Enter, Mods::default());
    // No daemon in this test: the words stay, and a quiet line says why.
    assert_eq!(rt.state.draft.text, "Fix the build\nthen tell Lee");
    assert!(rt.state.notice.is_some(), "Return tried to send");
    assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("notice")));
}

#[test]
fn command_n_and_control_command_s() {
    let mut rt = runtime((1200.0, 800.0), false, scene("task-open"));
    assert!(rt.state.open.is_some());
    press(&mut rt, Key::Command('n'), Mods { cmd: true, ..Mods::default() });
    assert_eq!(rt.state.open, None, "⌘N goes Home");
    assert_eq!(focus(&rt).as_deref(), Some("composer-input"), "with the keyboard in the composer");
    press(&mut rt, Key::Escape, Mods::default());
    press(&mut rt, Key::Command('s'), Mods { cmd: true, ctrl: true, ..Mods::default() });
    assert!(!rt.state.sidebar_shown(), "⌃⌘S hides the sidebar");
    settle(&mut rt);
    assert!(rt.elements().iter().all(|e| e.key.as_deref() != Some("sidebar")));
    press(&mut rt, Key::Command('s'), Mods { cmd: true, ctrl: true, ..Mods::default() });
    assert!(rt.state.sidebar_shown(), "and shows it again");
}

/// Every Tab stop of a scene, once round.
fn stops(rt: &mut rune::Runtime<neko_app::state::App>) -> Vec<String> {
    let mut seen = Vec::new();
    for _ in 0..60 {
        tab(rt, false);
        let Some(k) = focus(rt) else { continue };
        if seen.first() == Some(&k) {
            break;
        }
        assert!(!seen.contains(&k), "{k} reached twice: {seen:?}");
        seen.push(k);
    }
    seen
}

#[test]
fn the_task_view_is_reached_in_reading_order() {
    let mut rt = runtime((1200.0, 800.0), false, scene("task-done"));
    let s = stops(&mut rt);
    let at = |k: &str| s.iter().position(|x| x == k).unwrap_or_else(|| panic!("{k} is not reached: {s:?}"));
    assert!(at("copy-task-message") < at("worked-run_1-trigger"), "the person's words, then the fold: {s:?}");
    assert!(at("worked-run_1-trigger") < at("reply-input"), "{s:?}");
    assert!(s.iter().any(|k| k.starts_with("copy-part-")), "the answer has its copy button: {s:?}");
    // Return opens the fold where it has focus; its steps are then reached too.
    while focus(&rt).as_deref() != Some("worked-run_1-trigger") {
        tab(&mut rt, false);
    }
    press(&mut rt, Key::Enter, Mods::default());
    assert!(rt.state.is_open("worked-run_1"));
    settle(&mut rt);
    let s = stops(&mut rt);
    assert!(s.iter().any(|k| k == "part-2-trigger"), "a tool line inside the fold: {s:?}");
}

#[test]
fn a_request_is_answered_from_the_keyboard_and_escape_denies() {
    let mut rt = runtime((1200.0, 800.0), false, scene("task-needs-you"));
    let s = stops(&mut rt);
    let allow = s.iter().position(|k| k.ends_with("-allow")).expect("Allow once is reached");
    assert!(s[allow + 1].ends_with("-always") && s[allow + 2].ends_with("-deny"), "the default answer first: {s:?}");
    let mut rt = runtime((1200.0, 800.0), false, scene("task-needs-you-destructive"));
    let s = stops(&mut rt);
    assert!(!s.iter().any(|k| k.ends_with("-always")), "no Always for a destructive step: {s:?}");
    assert!(s.iter().any(|k| k.ends_with("-allow")) && s.iter().any(|k| k.ends_with("-deny")));
}

#[test]
fn escape_stops_a_working_task_and_the_composer_shows_stop() {
    let mut rt = runtime((1200.0, 800.0), false, scene("task-working"));
    assert!(rt.state.open_is_working());
    let stop = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("reply-submit")).expect("the composer's button");
    assert_eq!(stop.label.as_deref(), Some("Stop"), "Send becomes Stop while it works");
    // No daemon in this test: pressing Escape asks to stop (the request has nowhere to go), and takes the key.
    press(&mut rt, Key::Escape, Mods::default());
    assert!(rt.state.open_is_working(), "the state changes only when the daemon says so");
}

#[test]
fn command_comma_opens_settings() {
    let mut rt = runtime((1200.0, 800.0), false, scene("tasks-home"));
    press(&mut rt, Key::Command(','), Mods { cmd: true, ..Mods::default() });
    assert!(rt.state.settings_open);
    assert_eq!(rt.state.window_title(), "Settings");
    settle(&mut rt);
    assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("settings-title")));
}
