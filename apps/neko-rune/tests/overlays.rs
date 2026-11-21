//! P4, the overlays (docs/knowledge/rune-overlays.md), headless: the pill's states drawn through the real layout, the launcher's content
//! and its keys, and the stop-all key through the window's own confirmation. Their shape in the structure crawl is in `structure.rs`.
//! Nothing here creates a window or registers a shortcut; the real check (`--overlays` on this Mac) is in the note.

use neko_protocol::workbench::TaskStatus;
use neko_rune::app::App;
use neko_rune::launcher::{self, Effect, Launcher, Presentation};
use neko_rune::link::{Call, TaskRow, Wait};
use neko_rune::palette::Do;
use neko_rune::presence::{self, Activity, Pill, PresenceState};
use neko_rune::scenes;
use neko_rune::view::view;
use rune::{Env, Key, KeyEvent, Mods, Runtime};
use std::sync::Arc;

fn settle<S: 'static>(rt: &mut Runtime<S>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn pill_runtime(activity: Activity, label: &str, expanded: bool, reduced_motion: bool) -> Runtime<Pill> {
    let mut pill = Pill::new(None);
    pill.state = Some(PresenceState { activity, label: label.to_owned(), lingers: !activity.working(), task: None });
    pill.force_expanded = expanded;
    let mut rt = Runtime::new(pill, presence::view, presence::WINDOW, Env { system_dark: false, scale: 1.0 });
    rt.set_system_reduced_motion(reduced_motion);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn rect_of<S: 'static>(rt: &Runtime<S>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element keyed {key}")).rect
}

fn has_key<S: 'static>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

const STATES: [(Activity, &str); 6] = [
    (Activity::Thinking, "Making a plan: Fix the login redirect"),
    (Activity::Creating, "Preparing a fix: Fix the login redirect +2 more"),
    (Activity::Debugging, "Checking the result: Fix the login redirect"),
    (Activity::NeedsYou, "Neko needs your OK for a tool"),
    (Activity::Ready, "Ready to review: Fix the login redirect"),
    (Activity::Failed, "Needs attention: Fix the login redirect"),
];

#[test]
fn every_pill_state_is_a_36_point_capsule_in_the_corner_that_widens_to_its_words_on_hover() {
    for (activity, label) in STATES {
        let compact = pill_runtime(activity, label, false, false);
        let c = rect_of(&compact, "pill");
        assert_eq!((c[2], c[3]), (36.0, 36.0), "{activity:?}: a glyph with 10 points around it");
        assert!(!has_key(&compact, "pill-label"), "{activity:?}: no words until the pointer is over it");
        assert!(has_key(&compact, "pill-glyph") || has_key(&compact, "status-InProgress") || has_key(&compact, "status-Review") || has_key(&compact, "status-Failed"), "{activity:?}: a glyph");
        // Its right edge is the window's, less the room for the glow.
        assert_eq!(c[0] + c[2], presence::WINDOW.0 - presence::GLOW, "{activity:?}");
        let open = pill_runtime(activity, label, true, false);
        let o = rect_of(&open, "pill");
        assert!(o[2] > 100.0 && o[3] == 36.0, "{activity:?}: {o:?}");
        assert!(o[0] >= presence::GLOW && o[0] + o[2] <= presence::WINDOW.0 - presence::GLOW + 0.5, "{activity:?}: inside the window with the glow's room: {o:?}");
        let words = open.elements().into_iter().find(|e| e.key.as_deref() == Some("pill-label")).expect("the label").text;
        assert_eq!(words.as_deref(), Some(label));
    }
}

#[test]
fn a_hover_over_the_capsule_shows_the_words_and_leaving_hides_them() {
    let mut rt = pill_runtime(Activity::Creating, "Preparing a fix: Ship it", false, false);
    let (x, y) = rt.find("pill").expect("the capsule");
    rt.pointer_move(x, y);
    settle(&mut rt);
    assert!(rt.state.expanded && has_key(&rt, "pill-label"));
    rt.pointer_move(5.0, 5.0);
    settle(&mut rt);
    assert!(!rt.state.expanded && !has_key(&rt, "pill-label"));
}

#[test]
fn reduce_motion_stops_the_pulse_and_the_spinning_glyph() {
    // Moving: the glow and (for working states) the ring's arc carry a loop; under Reduce Motion neither does.
    let moving = pill_runtime(Activity::Thinking, "Neko is replying", false, false);
    let still = pill_runtime(Activity::Thinking, "Neko is replying", false, true);
    let looping = |rt: &Runtime<Pill>| rt.elements().iter().filter(|e| e.loop_phase.is_some()).count();
    assert!(looping(&moving) >= 2, "the glow and the arc move: {}", looping(&moving));
    assert_eq!(looping(&still), 0, "nothing moves under Reduce Motion");
    // A settled outcome never pulses, motion or not.
    assert_eq!(looping(&pill_runtime(Activity::Ready, "Ready to review: x", false, false)), 0);
    // "Needs you" breathes (Swift's `animates`) unless Reduce Motion is on.
    assert!(looping(&pill_runtime(Activity::NeedsYou, "Plan ready: x", false, false)) >= 1);
    assert_eq!(looping(&pill_runtime(Activity::NeedsYou, "Plan ready: x", false, true)), 0);
}

#[test]
fn a_press_on_the_capsule_is_the_overlays_only_click_target() {
    let rt = pill_runtime(Activity::Ready, "Ready to review: Ship it", false, false);
    let regions = rt.hit_regions().to_vec();
    assert_eq!(regions.len(), 1, "everything else in the window lets clicks through: {regions:?}");
    let capsule = rect_of(&rt, "pill");
    let r = regions[0];
    assert!((r[0] - capsule[0]).abs() < 1.0 && (r[1] - capsule[1]).abs() < 1.0 && (r[2] - r[0] - capsule[2]).abs() < 1.0, "the region is the capsule: {r:?} against {capsule:?}");
}

// ---- the launcher ----------------------------------------------------------------------------------

fn live_rows() -> Vec<neko_rune::palette::Item> {
    let mut app = App::headless_neko();
    let (_, f) = scenes::SCENES.iter().find(|(n, _)| *n == "live").unwrap();
    f(&mut app);
    app.palette_all()
}

fn launcher_runtime() -> Runtime<Launcher> {
    let mut l = Launcher::new(Arc::new(Presentation::default()), None);
    l.record = true;
    let token = l.presentation.toggle().unwrap();
    l.present(live_rows(), None, token);
    let mut rt = Runtime::new(l, launcher::view, launcher::WINDOW, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

#[test]
fn the_launcher_shows_the_palettes_sections_in_a_760_by_500_window() {
    let rt = launcher_runtime();
    let card = rect_of(&rt, "palette");
    assert_eq!(card[2], launcher::CARD_WIDTH, "the card leaves 8 points of window around it for its shadow");
    assert!(card[0] >= 8.0 - 0.5 && card[1] >= 8.0 - 0.5 && card[0] + card[2] <= launcher::WINDOW.0 - 8.0 + 0.5 && card[1] + card[3] <= launcher::WINDOW.1 - 8.0 + 0.5, "{card:?}");
    for heading in ["Sessions", "Go to", "Actions", "Appearance"] {
        assert!(has_key(&rt, &format!("palette-heading-{heading}")), "{heading}");
    }
    assert!(has_key(&rt, "palette-input"));
}

#[test]
fn typing_narrows_the_launcher_and_a_press_on_a_row_runs_it_in_the_main_window_then_hides() {
    let mut rt = launcher_runtime();
    rt.update(|l, _| l.query.replace_text("calendr"));
    settle(&mut rt);
    let first = rt.state.visible().first().map(|i| i.title.clone()).unwrap();
    assert_eq!(first, "Go to Calendar");
    let (x, y) = rt.find("palette-row-0").expect("the first row");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.log.len(), 2, "{:?}", rt.state.log);
    assert!(matches!(&rt.state.log[0], Effect::Run { run: Do::GoTo(neko_rune::views::FullView::Calendar), front: true }), "{:?}", rt.state.log[0]);
    assert_eq!(rt.state.log[1], Effect::Hide);
    assert!(!rt.state.presentation.shown());
}

#[test]
fn escape_and_the_arrows_work_from_the_search_field() {
    let mut rt = launcher_runtime();
    rt.on_key = Some(|l: &mut Launcher, ev: &KeyEvent, _| l.key(ev));
    let down = KeyEvent { key: Key::Down, mods: Mods::default() };
    rt.key(down);
    settle(&mut rt);
    assert_eq!(rt.state.index, 1);
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    assert_eq!(rt.state.log, vec![Effect::Hide]);
}

// ---- the stop-all key ------------------------------------------------------------------------------

fn app_runtime(scene: &str) -> Runtime<App> {
    let mut app = App::headless_neko();
    let (_, f) = scenes::SCENES.iter().find(|(n, _)| *n == scene).unwrap_or_else(|| panic!("no scene {scene}"));
    f(&mut app);
    app.outbox.clear();
    let mut rt = Runtime::new(app, view, (1360.0, 900.0), Env { system_dark: true, scale: 1.0 });
    rt.on_key = Some(neko_rune::on_key);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

#[test]
fn the_stop_all_key_asks_first_and_only_a_yes_sends_the_request() {
    let mut rt = app_runtime("running");
    let (tasks, replying) = rt.state.running_agents();
    let n = tasks + usize::from(replying);
    assert!(n > 0, "the scene has running work");
    rt.update(|s, tx| {
        s.stop_all_key();
        tx.instant();
    });
    settle(&mut rt);
    assert!(texts(&rt).iter().any(|t| t.contains(&format!("Stop all {n} running agent"))), "{:?}", texts(&rt));
    assert!(rt.state.outbox.is_empty(), "nothing was stopped without a yes");
    let (x, y) = rt.find("ask-yes").expect("the dialog's yes");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.outbox, vec![Call::StopAll]);
}

#[test]
fn the_stop_all_key_with_nothing_running_opens_nothing() {
    let mut app = App::headless_neko();
    app.summary = Some(neko_rune::link::Summary::default());
    let mut rt = Runtime::new(app, view, (1360.0, 900.0), Env { system_dark: true, scale: 1.0 });
    rt.on_key = Some(neko_rune::on_key);
    rt.frame(0.0);
    settle(&mut rt);
    assert_eq!(rt.state.running_agents(), (0, false));
    rt.update(|s, tx| {
        s.stop_all_key();
        tx.instant();
    });
    settle(&mut rt);
    assert!(!has_key(&rt, "ask-yes") && rt.state.outbox.is_empty() && rt.state.dialog.is_none());
}

// ---- the gate ----------------------------------------------------------------------------------------

#[test]
fn with_the_gate_off_a_poll_never_touches_the_pill_or_its_tracker() {
    let mut app = App::headless_neko();
    assert!(!app.overlays.plan.any(), "a window starts with nothing planned");
    let (_, f) = scenes::SCENES.iter().find(|(n, _)| *n == "running").unwrap();
    f(&mut app);
    let mut rt = Runtime::new(app, view, (1360.0, 900.0), Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    rt.update(|s, tx| s.tick_presence(tx));
    assert!(rt.state.overlays.presence.shown().is_none(), "no pill is shown or even tracked");
    // The summon key does nothing without a launcher, and the launcher's settings row is not offered.
    rt.update(|s, _| s.summon_launcher());
    assert!(!rt.state.overlays.presentation.shown());
    settle(&mut rt);
    assert!(!has_key(&rt, "set-pill-switch"));
}

#[test]
fn with_the_gate_on_the_pill_follows_the_summary_and_the_preference_hides_it() {
    let mut app = App::headless_neko();
    app.overlays.plan = neko_rune::overlays::plan(true);
    app.clock = || 1_000;
    let task = |status: TaskStatus| TaskRow { id: "a".into(), key: None, title: "Ship it".into(), status, updated_at_ms: 1, goal: String::new(), wait: Wait::None, progress: None, line: Default::default() };
    app.summary = Some(neko_rune::link::Summary { tasks: vec![task(TaskStatus::Queued)], ..Default::default() });
    let mut rt = Runtime::new(app, view, (1360.0, 900.0), Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    // Without handles (a headless window) the steps are decided and not drawn: the tracker is the pill's state.
    rt.update(|s, tx| s.tick_presence(tx));
    rt.update(|s, _| s.summary.as_mut().unwrap().tasks[0].status = TaskStatus::Building);
    rt.update(|s, tx| {
        s.clock = || 2_000;
        s.tick_presence(tx);
    });
    let shown = rt.state.overlays.presence.shown().cloned().expect("work shows");
    assert_eq!((shown.activity, shown.label.as_str(), shown.task.as_deref()), (Activity::Creating, "Preparing a fix: Ship it", Some("a")));
    // Settings: the row is there, and turning it off hides what is shown.
    settle(&mut rt);
    rt.update(|s, _| s.overlays.presence.enabled = true);
    rt.update(|s, _| {
        let step = s.overlays.presence.set_enabled(false);
        assert_eq!(step, presence::Step::Hide);
    });
    assert!(rt.state.overlays.presence.shown().is_none());
}

#[test]
fn settings_offers_the_pill_switch_only_when_the_overlays_are_on_and_it_turns_the_pill_off() {
    let setup = |on: bool| {
        let mut app = App::headless_neko();
        let (_, f) = scenes::SCENES.iter().find(|(n, _)| *n == "live").unwrap();
        f(&mut app);
        app.overlays.plan = neko_rune::overlays::plan(on);
        app.open_view(neko_rune::views::FullView::Settings);
        let mut rt = Runtime::new(app, view, (1360.0, 900.0), Env { system_dark: true, scale: 1.0 });
        rt.frame(0.0);
        settle(&mut rt);
        rt
    };
    assert!(!has_key(&setup(false), "set-pill-switch"), "a client that runs no pill does not offer to switch it off");
    let mut rt = setup(true);
    assert!(rt.state.overlays.presence.enabled, "on by default, as in Swift");
    assert!(texts(&rt).iter().any(|t| t == "Show activity while Neko works"), "Swift's words");
    let (x, y) = rt.find("set-pill-switch").expect("the switch");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert!(!rt.state.overlays.presence.enabled);
}
