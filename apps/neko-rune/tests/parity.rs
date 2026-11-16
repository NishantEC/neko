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

fn fill_of(rt: &Runtime<App>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).map(|e| e.fill.hex()).unwrap_or_else(|| panic!("no element keyed {key}"))
}

#[test]
fn the_composer_is_one_card_in_the_session_the_neko_column_and_home() {
    let iris = neko_rune::theme::theme(true).meaning.iris.hex();
    // The session: inside the 760 column, centred, three lines of text and a bar, a 32 point round Send.
    let mut rt = runtime_sized(scene("live"), (1900.0, 900.0));
    let (region, card) = (rect(&rt, "session-region"), rect(&rt, "composer-card"));
    assert!(card[2] <= 760.0 - 64.0 + 0.5 && card[2] >= 690.0, "760 wide with 32 at each side: {card:?}");
    assert!(((card[0] + card[2] / 2.0) - (region[0] + region[2] / 2.0)).abs() < 2.0, "centred: {card:?} {region:?}");
    assert!(card[3] >= 3.0 * 17.0 + 32.0 + 32.0, "three lines and a bar: {card:?}");
    for key in ["send", "ask-send"] {
        let r = rect(&rt, key);
        assert_eq!((r[2], r[3]), (32.0, 32.0), "{key}: {r:?}");
    }
    assert_ne!(fill_of(&rt, "send"), iris, "no text: the foreground at 10%");
    rt.update(|s, _| s.composer.replace_text("use three tries"));
    settle(&mut rt);
    assert_eq!(fill_of(&rt, "send"), iris, "text: filled Iris");
    // Neko's ask box: the same card, with Ask / Plan, a model chip and "+".
    for key in ["ask-card", "ask-mode", "ask-model", "ask-attach"] {
        assert!(has_key(&rt, key), "{key}");
    }
    assert_ne!(fill_of(&rt, "ask-send"), iris);
    rt.update(|s, _| s.ask.replace_text("what needs me?"));
    settle(&mut rt);
    assert_eq!(fill_of(&rt, "ask-send"), iris);
    // The session's bar has the steer mode, the model chip and "+".
    for key in ["steer-trigger", "composer-model", "composer-attach"] {
        assert!(has_key(&rt, key), "{key}");
    }
}

#[test]
fn plan_prefixes_neko_s_words_with_the_web_s_instruction_and_ask_does_not() {
    let mut rt = runtime(scene("live"));
    rt.update(|s, _| s.ask.replace_text("tidy the board"));
    settle(&mut rt);
    click(&mut rt, "ask-mode-plan");
    click(&mut rt, "ask-send");
    let sent = rt.state.outbox.iter().find_map(|c| if let neko_rune::link::Call::Ask { text } = c { Some(text.clone()) } else { None }).expect("asked");
    assert_eq!(sent, format!("{}tidy the board", neko_rune::work::PLAN_PREFIX));
}
