//! P4, the overlays (docs/knowledge/rune-overlays.md), headless: the pill's states drawn through the real layout and the gate. Their shape in the structure crawl is in `structure.rs`.
//! Nothing here creates a window or registers a shortcut; the real check (`--overlays` on this Mac) is in the note.

use neko_protocol::workbench::TaskStatus;
use neko_rune::app::App;
use neko_rune::link::{TaskRow, Wait};
use neko_rune::presence::{self, Activity, Pill, PresenceState};
use neko_rune::scenes;
use neko_rune::view::view;
use rune::{Env, Runtime};

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

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

// ---- the gate ----------------------------------------------------------------------------------------

#[test]
fn with_the_gate_off_a_poll_never_touches_the_pill_or_its_tracker() {
    let mut app = App::headless();
    assert!(!app.overlays.plan.any(), "a window starts with nothing planned");
    let (_, f) = scenes::SCENES.iter().find(|(n, _)| *n == "running").unwrap();
    f(&mut app);
    let mut rt = Runtime::new(app, view, (1360.0, 900.0), Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    rt.update(|s, tx| s.tick_presence(tx));
    assert!(rt.state.overlays.presence.shown().is_none(), "no pill is shown or even tracked");
    settle(&mut rt);
    assert!(!has_key(&rt, "set-pill-switch"));
}

#[test]
fn with_the_gate_on_the_pill_follows_the_summary_and_the_preference_hides_it() {
    let mut app = App::headless();
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
        let mut app = App::headless();
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
