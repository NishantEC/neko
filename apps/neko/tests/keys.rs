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
    for _ in 0..12 {
        tab(&mut rt, false);
        let Some(k) = focus(&rt) else { continue };
        if seen.first() == Some(&k) {
            break;
        }
        seen.push(k);
    }
    assert_eq!(
        seen,
        ["sidebar-hide", "new-task", "home", "task-task_00000000000000000000000004", "task-task_00000000000000000000000001", "composer-input"],
        "each list is one stop; the empty composer's Send is off"
    );
    // Round once, the focus is back on the first control; Shift+Tab walks back the same way, wrapping.
    assert_eq!(focus(&rt).as_deref(), Some("sidebar-hide"));
    tab(&mut rt, true);
    assert_eq!(focus(&rt).as_deref(), Some("composer-input"));
    tab(&mut rt, true);
    assert_eq!(focus(&rt).as_deref(), Some("task-task_00000000000000000000000001"));
}

#[test]
fn the_arrows_move_in_the_list_and_return_opens() {
    let mut rt = runtime((1200.0, 800.0), false, scene("tasks-home"));
    for _ in 0..4 {
        tab(&mut rt, false);
    }
    assert_eq!(focus(&rt).as_deref(), Some("task-task_00000000000000000000000004"));
    press(&mut rt, Key::Down, Mods::default());
    assert_eq!(focus(&rt).as_deref(), Some("task-task_00000000000000000000000003"), "Down moves to the next row");
    assert_eq!(rt.state.open, None, "moving does not open");
    press(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.open.as_ref().map(|t| t.0.as_str()), Some("task_00000000000000000000000003"), "Return opens the row");
    press(&mut rt, Key::Escape, Mods::default());
    assert_eq!(focus(&rt), None, "Escape gives the focus back");
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
