//! A folded sessions column can always be unfolded: the "Show sidebar" button after the traffic lights, ⌘\ both ways,
//! the tooltip on the fold button, the saved layout. Headless, through the real layout and hit-testing.

use neko_rune::app::App;
use neko_rune::scenes::SCENES;
use neko_rune::view::view;
use rune::{Env, Key, KeyEvent, Mods, Runtime};

fn runtime(scene: &str, size: (f32, f32)) -> Runtime<App> {
    let (_, f) = SCENES.iter().find(|(n, _)| *n == scene).unwrap_or_else(|| panic!("no scene {scene}"));
    let mut app = App::headless_neko();
    f(&mut app);
    app.outbox.clear();
    let mut rt = Runtime::new(app, view, size, Env { system_dark: true, scale: 1.0 });
    rt.on_key = Some(neko_rune::on_key);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn info(rt: &Runtime<App>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn press_backslash(rt: &mut Runtime<App>) {
    rt.key(KeyEvent { key: Key::Command('\\'), mods: Mods { cmd: true, ..Mods::default() } });
    settle(rt);
}

#[test]
fn a_folded_column_shows_the_button_at_x_80_on_the_traffic_lights_line_and_it_opens_the_column() {
    for scene in ["sessions-collapsed", "home-sessions-collapsed"] {
        let mut rt = runtime(scene, (1360.0, 900.0));
        assert!(!rt.state.layout.sessions.is_open(), "{scene}: the column is folded");
        let b = info(&rt, "sidebar-show").unwrap_or_else(|| panic!("{scene}: no Show sidebar button"));
        assert!((b.rect[0] - 80.0).abs() <= 1.0, "{scene}: its left edge is 80 from the window's: {:?}", b.rect);
        assert_eq!((b.rect[2], b.rect[3]), (28.0, 28.0), "{scene}: 28 by 28");
        assert!((b.rect[1] + b.rect[3] / 2.0 - 24.0).abs() <= 1.0, "{scene}: centred on the bar's line (24): {:?}", b.rect);
        assert_eq!(b.label.as_deref(), Some("Show sidebar"));
        assert_eq!(b.help.as_deref(), Some("Show sidebar ⌘\\"));
        assert_eq!(b.radius, 8.0);
        assert!(info(&rt, "sessions").is_none(), "{scene}: no sessions column while folded");
        let (x, y) = rt.find("sidebar-show").expect("the button");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(&mut rt);
        assert!(rt.state.layout.sessions.is_open(), "{scene}: pressing it opens the column");
        assert!(info(&rt, "sessions").is_some());
        assert!(info(&rt, "sidebar-show").is_none(), "{scene}: and the button goes away with the need for it");
    }
}

#[test]
fn the_session_header_moves_right_of_the_button_and_the_open_column_hides_with_its_own_tooltip() {
    let mut rt = runtime("sessions-collapsed", (1360.0, 900.0));
    let b = info(&rt, "sidebar-show").expect("the button").rect;
    let title = info(&rt, "top-title").expect("the session's title").rect;
    assert!(title[0] >= b[0] + b[2] + 4.0, "the title starts after the button, not under it or the traffic lights: {title:?} vs {b:?}");
    // Open: the key brings the column back (the prototype's sidebar has no fold button of its own; ⌘\ hides it again).
    press_backslash(&mut rt);
    assert!(rt.state.layout.sessions.is_open());
    press_backslash(&mut rt);
    assert!(!rt.state.layout.sessions.is_open(), "the same key folds it again");
    press_backslash(&mut rt);
    // Home has its own button over the art, at the same spot.
    let rt = runtime("home-sessions-collapsed", (1360.0, 900.0));
    let home = info(&rt, "sidebar-show").expect("Home's button").rect;
    assert!((home[0] - 80.0).abs() <= 1.0 && (home[1] + home[3] / 2.0 - 24.0).abs() <= 1.0, "{home:?}");
}

#[test]
fn command_backslash_toggles_the_column_both_ways_and_is_in_the_list() {
    let mut rt = runtime("live", (1360.0, 900.0));
    assert!(rt.state.layout.sessions.is_open());
    press_backslash(&mut rt);
    assert!(!rt.state.layout.sessions.is_open(), "⌘\\ folds it");
    assert!(info(&rt, "sidebar-show").is_some());
    press_backslash(&mut rt);
    assert!(rt.state.layout.sessions.is_open(), "and unfolds it");
    // ⌘⇧\ is not ours.
    rt.key(KeyEvent { key: Key::Command('\\'), mods: Mods { cmd: true, shift: true, ..Mods::default() } });
    assert!(rt.state.layout.sessions.is_open());
    let listed = neko_rune::shortcuts::GROUPS.iter().flat_map(|g| g.rows.iter()).any(|r| r.keys == ["⌘", "\\"]);
    assert!(listed, "the shortcuts overlay lists ⌘\\");
}

#[test]
fn the_fold_is_part_of_the_saved_layout() {
    let mut rt = runtime("live", (1360.0, 900.0));
    press_backslash(&mut rt);
    let saved = rt.state.layout.saved();
    assert!(!saved.sessions_open);
    assert!(!neko_rune::layout::Layout::restored(saved).sessions.is_open());
}
