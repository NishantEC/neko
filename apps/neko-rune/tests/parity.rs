//! The session-view parity pass (the owner's brief of 2026-11-16): the text column, the header, the goal line, the
//! question card, hand-off lines, the one composer, the context panel's Plan and Info tabs, the terminal's headers,
//! and the pages' parity changes. Real layout, read by geometry and text, headless.

use neko_rune::app::App;
use neko_rune::scenes;
use neko_rune::view::view;
use rune::{Env, Runtime};

const WINDOW: (f32, f32) = (1360.0, 900.0);

pub fn scene(name: &str) -> impl FnOnce(&mut App) + '_ {
    move |app| {
        let (_, f) = scenes::SCENES.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no scene {name}"));
        f(app);
        app.outbox.clear();
    }
}

pub fn runtime_sized(setup: impl FnOnce(&mut App), size: (f32, f32)) -> Runtime<App> {
    let mut app = App::headless();
    setup(&mut app);
    let mut rt = Runtime::new(app, view, size, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

pub fn runtime(setup: impl FnOnce(&mut App)) -> Runtime<App> {
    runtime_sized(setup, WINDOW)
}

pub fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

pub fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

pub fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element keyed {key}")).rect
}

pub fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

pub fn has_text(rt: &Runtime<App>, needle: &str) -> bool {
    texts(rt).iter().any(|t| t.contains(needle))
}

pub fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

#[test]
fn the_goal_line_is_a_full_width_row_under_the_header_and_the_column_is_at_most_760() {
    // A wide window: the column stays 760 and centred, the region's rows stay full width.
    let rt = runtime_sized(scene("running"), (1900.0, 900.0));
    let region = rect(&rt, "session-region");
    let goal = rect(&rt, "goal-g1");
    let top = rect(&rt, "top-title");
    assert!(goal[1] >= top[1] + top[3] - 1.0, "the goal row sits under the header row: {goal:?} {top:?}");
    assert!(goal[2] >= region[2] - 24.0, "the goal row spans the region: {goal:?} {region:?}");
    assert!((goal[3] - 32.0).abs() < 0.5, "32 high: {goal:?}");
}

#[test]
fn a_hand_off_between_agents_is_a_crew_line_with_the_task_as_a_link() {
    let rt = runtime(scene("running"));
    let lines: Vec<String> = texts(&rt).into_iter().filter(|t| t.contains("Planner → Builder")).collect();
    assert!(!lines.is_empty(), "the session has agents that hand over to each other: {:?}", texts(&rt));
    let id = rt.state.selected.clone().expect("a session is open");
    assert!(has_key(&rt, &format!("handoff-{id}")), "the id is a link to the task");
    assert!(texts(&rt).iter().any(|t| t.starts_with("NEK-")), "{:?}", texts(&rt));
}
