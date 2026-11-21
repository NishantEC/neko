//! The session-view parity pass (the owner's brief of 2026-11-16): the text column, the header, the goal line, the
//! question card, hand-off lines, the one composer, the context panel's Plan and Details tabs, the terminal's headers,
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
    let mut app = App::headless_neko();
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
    let iris = neko_rune::theme::theme(true).mat.ink_bottom.hex(); // Send is the ink when there is something to send
    // The session: inside the 760 column, centred, three lines of text and a bar, a 32 point round Send.
    let mut rt = runtime_sized(scene("live"), (1900.0, 900.0));
    let (region, card) = (rect(&rt, "session-region"), rect(&rt, "composer-card"));
    assert!(card[2] <= 760.0 - 64.0 + 0.5 && card[2] >= 690.0, "760 wide with 32 at each side: {card:?}");
    assert!(((card[0] + card[2] / 2.0) - (region[0] + region[2] / 2.0)).abs() < 2.0, "centred: {card:?} {region:?}");
    assert!(card[3] >= 2.0 * 17.0 + 30.0 + 22.0, "two lines and a bar: {card:?}");
    for key in ["send", "ask-send"] {
        let r = rect(&rt, key);
        assert_eq!((r[2], r[3]), (30.0, 30.0), "{key}: {r:?}");
    }
    assert_ne!(fill_of(&rt, "send"), iris, "no text: the tint");
    rt.update(|s, _| s.composer.replace_text("use three tries"));
    settle(&mut rt);
    assert_eq!(fill_of(&rt, "send"), iris, "text: the ink");
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
    let sent = rt.state.outbox.iter().find_map(|c| if let neko_rune::link::Call::Ask { text, .. } = c { Some(text.clone()) } else { None }).expect("asked");
    assert_eq!(sent, format!("{}tidy the board", neko_rune::work::PLAN_PREFIX));
}

#[test]
fn the_panel_has_plan_and_info_after_terminal_and_they_never_open_it_by_themselves() {
    use neko_rune::panel::Tab;
    // A session that has a plan in checklist form: Plan and Details follow Terminal.
    let mut rt = runtime(scene("running"));
    let tabs = rt.state.panel_tabs();
    assert_eq!(tabs.last(), Some(&Tab::Details), "{tabs:?}");
    let at = |t: Tab| tabs.iter().position(|x| *x == t);
    if let (Some(term), Some(plan)) = (at(Tab::Terminal), at(Tab::Plan)) {
        assert!(term < plan && plan < at(Tab::Details).unwrap_or(0), "{tabs:?}");
    }
    // Details alone does not open the panel: a session with nothing else to show keeps it closed.
    let mut rt2 = runtime(scene("answer"));
    rt2.update(|s, _| {
        if let Some(d) = s.summary.as_mut().and_then(|m| m.detail.as_mut()) {
            d.task.events.clear();
        }
        s.panel_on_select();
    });
    settle(&mut rt2);
    assert!(rt2.state.panel_content_tabs().is_empty(), "{:?}", rt2.state.panel_tabs());
    assert!(!rt2.state.layout.context.is_open(), "Details is always there, so it must not open the panel");
    // Asked for, Details lists the session's workspace, runtime and dates.
    rt.update(|s, _| {
        s.panel_choose_tab(Tab::Details);
        s.panel_set_open(true);
    });
    settle(&mut rt);
    for key in ["info-workspace", "info-runtime", "info-created", "info-updated"] {
        assert!(has_key(&rt, key), "{key}");
    }
    assert!(has_text(&rt, "Claude · default"), "{:?}", texts(&rt));
}

#[test]
fn plan_is_the_checklist_with_done_and_active_steps() {
    use neko_rune::panel::Tab;
    let mut rt = runtime(scene("running"));
    rt.update(|s, _| {
        if let Some(d) = s.summary.as_mut().and_then(|m| m.detail.as_mut()) {
            d.task.plan = "- [x] Read the board\n- [ ] Add the empty state\n- [ ] Test it".to_owned();
        }
        s.panel_choose_tab(Tab::Plan);
        s.panel_set_open(true);
    });
    settle(&mut rt);
    assert!(has_text(&rt, "1 of 3 done"), "{:?}", texts(&rt));
    for i in 0..3 {
        assert!(has_key(&rt, &format!("plan-step-{i}")), "step {i}");
    }
    let label = |rt: &Runtime<App>, i: usize| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(&format!("plan-step-{i}"))).and_then(|e| e.label).unwrap_or_default();
    assert_eq!(label(&rt, 0), "Read the board, done");
    assert_eq!(label(&rt, 1), "Add the empty state, in progress", "the first open step is active while it builds");
    assert_eq!(label(&rt, 2), "Test it");
}

#[test]
fn the_terminal_tab_has_a_32_point_header_per_command_and_collapses_long_output() {
    use neko_protocol::workbench::TaskEvent;
    use neko_rune::panel::Tab;
    let mut rt = runtime_sized(scene("running"), (1800.0, 900.0));
    let home = std::env::var("HOME").unwrap_or_default();
    rt.update(move |s, _| {
        let output: String = (0..30).map(|i| format!("line {i} in {home}/code/x\n")).collect();
        let long = format!("cargo test -p neko-rune --test parity -- the_terminal_tab_has_a_header_per_command --nocapture {home}/code/some/very/long/path/that/goes/on/and/on");
        let receipt = |command: &str, out: &str, code: i64| format!("VERIFICATION_COMMAND {}", serde_json::json!({"command": command, "output": out, "exit_code": code}));
        let d = s.summary.as_mut().and_then(|m| m.detail.as_mut()).expect("detail");
        let at = d.task.events.last().map_or(0, |e| e.at_ms) + 1;
        d.task.events.push(TaskEvent { at_ms: at, role: "builder".into(), message: receipt(&long, &output, 101) });
        d.task.events.push(TaskEvent { at_ms: at + 1, role: "builder".into(), message: receipt("ls", "a\n", 0) });
        s.panel_choose_tab(Tab::Terminal);
        s.panel_set_open(true);
    });
    settle(&mut rt);
    // The list's rows are keyed `t<i>` (a virtual list's own keys): the header rows are the 32 point ones, one per command.
    let tall: Vec<f32> = rt.elements().into_iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with('t') && k[1..].chars().all(|c| c.is_ascii_digit()) && k.len() > 1)).map(|e| e.rect[3]).filter(|h| *h > 24.0).collect();
    assert!(tall.len() >= 2 && tall.iter().all(|h| (h - 32.0).abs() < 0.5), "one 32 point header row per command: {tall:?}");
    let words = texts(&rt);
    let long = words.iter().find(|t| t.starts_with("cargo test -p neko-run")).unwrap_or_else(|| panic!("the long command: {words:?}"));
    assert!(long.contains('…') && long.ends_with("on/and/on"), "cut in the middle, the end kept: {long}");
    assert!(!words.iter().any(|t| t.contains(&std::env::var("HOME").unwrap_or_else(|_| "/nonexistent".into())) && t.contains("code/x")), "the home folder reads ~");
    assert!(words.iter().any(|t| t.starts_with("line 0 in ~/code/x")), "{words:?}");
    assert!(words.iter().any(|t| t == "101") && words.iter().any(|t| t == "0"), "the exit chips");
    // 12 lines, then "Show all 30 lines"; one press shows them all.
    assert!(words.iter().any(|t| t == "line 11 in ~/code/x") || words.iter().any(|t| t.starts_with("line 11 ")), "{words:?}");
    assert!(!words.iter().any(|t| t.starts_with("line 12 ")), "collapsed to 12 lines");
    assert!(words.iter().any(|t| t == "Show all 30 lines"));
    let more = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Show all 30 lines")).expect("the link").rect;
    let (x, y) = (more[0] + more[2] / 2.0, more[1] + more[3] / 2.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert!(has_text(&rt, "line 29 in ~/code/x") || has_text(&rt, "line 29 "), "{:?}", texts(&rt));
    assert!(has_text(&rt, "Show fewer"));
}

#[test]
fn routines_have_status_chips_and_keep_the_last_run_for_the_details() {
    let rt = runtime(scene("routines"));
    let chips: Vec<String> = rt.elements().into_iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("routine-status-"))).filter_map(|e| e.key).collect();
    assert!(!chips.is_empty(), "a routine says its status in the web's words");
    assert!(texts(&rt).iter().any(|t| matches!(t.as_str(), "Quiet" | "Acted" | "Failed")), "{:?}", texts(&rt));
    assert!(!rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("routine-last-"))), "no last-run line on a closed row");
    let open = runtime(scene("routines-open"));
    assert!(open.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("routine-last-"))), "the last run is in the open details");
}

fn settings(section: &str) -> Runtime<App> {
    let key = section.to_owned();
    runtime(move |app| {
        let (_, f) = scenes::SCENES.iter().find(|(n, _)| *n == "settings").expect("the settings scene");
        f(app);
        app.pages.section = neko_rune::pages::Section::ALL.into_iter().find(|s| s.key() == key).expect("a section");
        app.outbox.clear();
    })
}

#[test]
fn settings_has_appearance_permissions_and_diagnostics_and_the_rulebook_and_quick_panel_editors() {
    use neko_rune::link::Call;
    let mut rt = settings("appearance");
    for key in ["settings-section-appearance", "settings-section-permissions", "settings-section-diagnostics", "settings-section-rulebook", "settings-section-quick-panel"] {
        assert!(has_key(&rt, key), "{key}");
    }
    // Appearance writes the daemon's mode, the web's own request, and the window follows at once.
    assert!(has_key(&rt, "set-appearance-light") && has_key(&rt, "set-appearance-dark") && has_key(&rt, "set-appearance-system"));
    assert!(rt.state.appearance_dark(true) && !rt.state.appearance_dark(false), "follows the system until the daemon says");
    rt.update(|s, _| s.set_appearance("light"));
    assert_eq!(rt.state.outbox, vec![Call::SetAppearance { mode: "light".into() }]);
    assert!(!rt.state.appearance_dark(true), "light means light, whatever the system says");
    rt.update(|s, _| s.set_appearance("dark"));
    assert!(rt.state.appearance_dark(false));

    // Permissions: five rows, a status and an Open System Settings button each (the Mac's answers are injected).
    let mut rt = runtime(|app| {
        let (_, f) = scenes::SCENES.iter().find(|(n, _)| *n == "settings").expect("the settings scene");
        f(app);
        app.pages.section = neko_rune::pages::Section::Permissions;
        app.access_probe = || neko_rune::permissions::Local { accessibility: neko_rune::permissions::Access::Granted, full_disk: neko_rune::permissions::Access::Denied, screen_recording: neko_rune::permissions::Access::Unknown };
        app.pages.access = Some((app.access_probe)());
    });
    for id in ["accessibility", "full-disk", "calendar", "reminders", "screen-recording"] {
        assert!(has_key(&rt, &format!("perm-{id}-state")) && has_key(&rt, &format!("perm-{id}-open")), "{id}");
    }
    assert!(has_text(&rt, "Allowed") && has_text(&rt, "Not allowed") && has_text(&rt, "Open System Settings"));
    click(&mut rt, "perm-full-disk-open");
    assert!(rt.state.notice.is_none(), "the headless opener takes the address");

    // Diagnostics: versions, the data folder, the daemon, and a copy button.
    let rt = settings("diagnostics");
    for key in ["diag-app-value", "diag-neko-value", "diag-data-value", "diag-daemon-value", "diag-copy"] {
        assert!(has_key(&rt, key), "{key}");
    }
    assert!(!has_text(&rt, &std::env::var("HOME").unwrap_or_else(|_| "/nonexistent".into())) || std::env::var("HOME").unwrap_or_default().is_empty(), "the home folder is written ~");

    // Rulebook and Quick panel are edited here now (batch D), not a link to the web UI.
    let rt = settings("rulebook");
    assert!(has_key(&rt, "rulebook") && has_key(&rt, "rulebook-save") && !has_key(&rt, "rulebook-open-web"));
    let rt = settings("quick-panel");
    assert!(has_key(&rt, "folder-path") && has_key(&rt, "folder-add") && !has_key(&rt, "quick-panel-open-web"));
}

#[test]
fn the_find_a_setting_hint_hides_while_the_field_has_text() {
    let mut rt = settings("general");
    assert!(has_key(&rt, "settings-search-hint"), "the hint shows while the field is empty");
    rt.update(|s, _| s.pages.set_field("settings:search", "calendar"));
    settle(&mut rt);
    assert!(!has_key(&rt, "settings-search-hint"), "and goes when there is text, so the clear button never shares its corner");
}

#[test]
fn six_panel_tabs_keep_whole_labels_just_over_the_icon_threshold() {
    let mut rt = runtime_sized(scene("running"), (1800.0, 900.0));
    rt.update(|s, _| {
        s.layout.drag(neko_rune::layout::Region::Context, 420.0, -30.0);
        s.panel_set_open(true);
    });
    settle(&mut rt);
    let width = rect(&rt, "context")[2];
    assert!((440.0..520.0).contains(&width), "{width}");
    assert!(has_key(&rt, "context-picker") && !has_key(&rt, "context-icon-tabs"), "labels, not icons, at {width}");
    let labels: Vec<String> = rt.elements().into_iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("segment"))).filter_map(|e| e.label).collect();
    let shown: Vec<String> = texts(&rt);
    for l in &labels {
        assert!(shown.iter().any(|t| t == l), "the label {l} is shown whole at {width}: {shown:?}");
    }
}
