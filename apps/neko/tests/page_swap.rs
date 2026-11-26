//! A page that replaces another is never drawn over it. Opening Review (and going back) used to cross-fade the conversation and
//! Review for a quarter second: Rune fades a removed element out by default, so both pages were on screen, one over the other. The
//! frames here are taken while the swap is moving (real time passes between them), as a person's eyes see it.

mod support;
use rune::Runtime;
use support::*;

use neko_app::state::App;

/// What of each page is drawn right now (opacity above nothing), exiting or not.
fn drawn(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && e.opacity > 0.01 && e.rect[2] > 0.0)
}

/// Frames a quarter second in, a little at a time, asserting at each that the two pages are not both drawn.
fn never_both(rt: &mut Runtime<App>, what: &str) {
    for i in 0..8 {
        rt.frame(0.03);
        let (conversation, review) = (drawn(rt, "task-words"), drawn(rt, "review-title"));
        if conversation && review {
            for e in rt.elements().iter().filter(|e| e.exiting && e.opacity > 0.01 && e.key.is_some()).take(12) {
                eprintln!("exiting {:?} opacity {} path {}", e.key, e.opacity, e.path);
            }
        }
        assert!(!(conversation && review), "{what}: after {} ms the conversation and Review are both drawn", (i + 1) * 30);
    }
}

#[test]
fn the_conversation_and_review_are_never_drawn_one_over_the_other() {
    let scene = scenes().iter().find(|s| s.0 == "task-undo").expect("the scene with a finished run and its Review button").1;
    let mut rt = runtime((1200.0, 800.0), false, scene);
    assert!(drawn(&rt, "task-words") && !drawn(&rt, "review-title"), "the conversation is open to begin with");
    let review = rt.elements().iter().find_map(|e| e.key.clone().filter(|k| k.starts_with("run-") && k.ends_with("-review"))).expect("the receipt's Review button");
    let (x, y) = rt.find(&review).expect("Review is on screen");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    never_both(&mut rt, "opening Review");
    settle(&mut rt);
    assert!(drawn(&rt, "review-title") && !drawn(&rt, "task-words"), "Review is open, the conversation gone");

    let (x, y) = rt.find("review-back").expect("Back to the conversation");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    never_both(&mut rt, "going back");
    settle(&mut rt);
    assert!(drawn(&rt, "task-words") && !drawn(&rt, "review-title"), "the conversation is back, Review gone");
}
