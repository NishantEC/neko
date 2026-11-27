//! A body that replaces another is never drawn over it. Opening Review used to cross-fade the conversation and the Review page for a
//! quarter second: Rune fades a removed element out by default, so both were on screen, one over the other. Review is now a tab of the
//! right pane, and the same must hold for its tabs. The frames here are taken while the swap is moving (real time passes between
//! them), as a person's eyes see it.

mod support;
use rune::Runtime;
use support::*;

use neko_app::state::App;

/// What of each page is drawn right now (opacity above nothing), exiting or not.
fn drawn(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && e.opacity > 0.01 && e.rect[2] > 0.0)
}

/// Frames a quarter second in, a little at a time, asserting at each that the two bodies are not both drawn.
fn never_both(rt: &mut Runtime<App>, a: &str, b: &str, what: &str) {
    for i in 0..8 {
        rt.frame(0.03);
        let (first, second) = (drawn(rt, a), drawn(rt, b));
        if first && second {
            for e in rt.elements().iter().filter(|e| e.exiting && e.opacity > 0.01 && e.key.is_some()).take(12) {
                eprintln!("exiting {:?} opacity {} path {}", e.key, e.opacity, e.path);
            }
        }
        assert!(!(first && second), "{what}: after {} ms {a} and {b} are both drawn", (i + 1) * 30);
    }
}

fn press_key(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is on screen"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
}

#[test]
fn the_panes_tabs_are_never_drawn_one_over_the_other() {
    let scene = scenes().iter().find(|s| s.0 == "pane-overview").expect("the scene with a finished run and its Overview").1;
    let mut rt = runtime((1280.0, 800.0), false, scene);
    assert!(drawn(&rt, "pane-overview-scroll") && !drawn(&rt, "changes-tab"), "Overview is open to begin with");
    assert!(drawn(&rt, "task-words"), "the conversation is beside the pane, always");
    press_key(&mut rt, "overview-review");
    never_both(&mut rt, "pane-overview-scroll", "changes-tab", "opening Review");
    settle(&mut rt);
    assert!(drawn(&rt, "changes-tab") && !drawn(&rt, "pane-overview-scroll"), "Changes is open, Overview gone");
    assert!(drawn(&rt, "task-words"), "the conversation never moved");

    press_key(&mut rt, &neko_app::ui::tabs::tab_key("pane-tabs", 0));
    never_both(&mut rt, "pane-overview-scroll", "changes-tab", "going back to Overview");
    settle(&mut rt);
    assert!(drawn(&rt, "pane-overview-scroll") && !drawn(&rt, "changes-tab"), "Overview is back, Changes gone");
}
