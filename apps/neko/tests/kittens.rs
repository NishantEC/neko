//! Kittens that work in a copy of their own, wait in line, are written as drafts, nest three deep and queue (docs/knowledge/app-shell.md,
//! "Kittens in the app"), on scenes: what a card says about where a kitten works and whom it follows, the draft card with its
//! prompt, Start and Discard, "1 kitten working, 1 waiting its turn" in the row and the activity bar, the Overview tree, and Neko's own
//! tool steps in a person's words.

mod support;
use neko_protocol::{SubPlace, TaskStatus};
use rune::Runtime;
use support::*;

use neko_app::state::App;

fn scene(name: &str) -> fn(&mut App) {
    scenes().iter().find(|s| s.0 == name).unwrap_or_else(|| panic!("no scene {name}")).1
}

fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn el(rt: &Runtime<App>, key: &str) -> rune::ElementInfo {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key} is not on screen"))
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter(|e| e.opacity > 0.0).filter_map(|e| e.text).collect()
}

fn node(rt: &Runtime<App>, key: &str) -> serde_json::Value {
    nodes(&rt.accessibility_json()).into_iter().map(|(_, n)| n).find(|n| str_of(n, "key") == key).unwrap_or_else(|| panic!("{key} is not in the tree"))
}

#[test]
fn a_card_says_where_the_kitten_works_and_whom_it_follows() {
    let rt = runtime((1200.0, 800.0), false, scene("kittens-placement"));
    let t = texts(&rt);
    for want in ["Own copy · branch neko/4f2a-1", "Own copy · branch neko/4f2a-2 · After Kitten 1", "Shared folder"] {
        assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
    }
    // The state words are true: the stacked one is queued, not working.
    assert!(str_of(&node(&rt, "subagent-card-2"), "name").contains("Queued. Own copy · branch neko/4f2a-2 · After Kitten 1."), "{}", str_of(&node(&rt, "subagent-card-2"), "name"));
    // Neko's own tool steps read as a person would say them, never as a server and a tool name.
    for want in ["Started Kitten 1 · Write the migration", "Started Kitten 2 · Update the tests", "Started Kitten 3 · Read the docs"] {
        assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
    }
    assert!(!t.iter().any(|x| x.contains("neko ·") || x.contains("start subagent")), "{t:?}");
}

#[test]
fn the_row_and_the_activity_bar_do_not_say_more_kittens_work_than_do() {
    let rt = runtime((1200.0, 800.0), false, scene("kittens-limits"));
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "3 kittens working, 2 waiting their turn"), "the row's second line and the bar: {t:?}");
    let bar = node(&rt, "activity-bar");
    assert_eq!(str_of(&bar, "name"), "Running: 3 kittens working, 2 waiting their turn");
    // The open list says which wait.
    assert!(t.iter().filter(|x| *x == "waiting its turn").count() == 2, "{t:?}");
    // One working and one waiting.
    let mut rt = runtime((1200.0, 800.0), false, scene("kittens-limits"));
    rt.update(|s, _| {
        for t in s.tasks.iter_mut().filter(|t| t.subagent.is_some()) {
            let n = t.subagent.as_ref().unwrap().number.clone();
            t.status = match n.as_str() {
                "1" => TaskStatus::Working,
                "2" => TaskStatus::New,
                _ => TaskStatus::Done,
            };
        }
    });
    settle(&mut rt);
    assert!(texts(&rt).iter().any(|x| x == "1 kitten working, 1 waiting its turn"), "{:?}", texts(&rt));
}

#[test]
fn a_draft_has_its_prompt_to_change_and_start_and_discard_and_nothing_waits_on_it() {
    let mut rt = runtime((1200.0, 800.0), false, scene("kittens-drafts"));
    for n in ["1", "2"] {
        for key in [format!("draft-prompt-{n}"), format!("draft-start-{n}"), format!("draft-discard-{n}")] {
            assert!(has_key(&rt, &key), "{key}");
        }
        assert_eq!(str_of(&node(&rt, &format!("draft-start-{n}")), "name"), format!("Start Kitten {n}"));
        assert_eq!(str_of(&node(&rt, &format!("draft-discard-{n}")), "name"), format!("Discard Kitten {n}"));
        // A draft has nothing to open yet.
        assert!(!has_key(&rt, &format!("subagent-open-{n}")));
    }
    let t = texts(&rt);
    assert_eq!(t.iter().filter(|x| *x == "Draft").count(), 2, "{t:?}");
    assert!(t.iter().any(|x| x == "2 draft kittens"), "the thread's row says there are drafts: {t:?}");
    assert!(t.iter().any(|x| x.starts_with("Write the migration that adds the sessions table")), "its prompt is in the card: {t:?}");
    // The thread does not wait on a draft: no waiting line, and it is not Waiting.
    assert!(!has_key(&rt, "waiting-line"));
    assert!(rt.state.live_subagents(&rt.state.open.clone().unwrap()).is_empty());
    // Both are drafts of the state, with the prompt as the person last left it.
    assert_eq!(rt.state.draft_edits.len(), 2);
    // Once one is started its card is an ordinary row, and the draft's text box goes.
    rt.update(|s, _| {
        let id = s.tasks.iter().find(|t| t.subagent.as_ref().is_some_and(|o| o.number == "1")).unwrap().id.clone();
        let mut t = s.tasks.iter().find(|t| t.id == id).unwrap().clone();
        t.subagent.as_mut().unwrap().draft = false;
        s.change(neko_protocol::Change::TaskUpdated { task: t });
    });
    settle(&mut rt);
    assert!(!has_key(&rt, "draft-prompt-1") && has_key(&rt, "subagent-open-1") && has_key(&rt, "draft-prompt-2"));
    assert_eq!(rt.state.draft_edits.len(), 1);
}

#[test]
fn the_overview_nests_kittens_three_deep_with_where_each_works() {
    let rt = runtime((1200.0, 800.0), false, scene("pane-kittens-deep"));
    let xs: Vec<f32> = ["1", "1.1", "1.1.1"].iter().map(|n| el(&rt, &format!("pane-kitten-{n}")).rect[0]).collect();
    assert!(xs[0] < xs[1] && xs[1] < xs[2], "each level is further in: {xs:?}");
    let t = texts(&rt);
    for want in ["Own copy · neko/4f2a-1-2 · After Kitten 1.1", "Shared folder · claude-sonnet-4-5", "Draft"] {
        assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
    }
    // A draft is not waited on: the thread's "Waiting on" lists the live ones only.
    let live = rt.state.live_subagents(&rt.state.open.clone().unwrap());
    assert!(live.iter().all(|k| k.subagent.as_ref().is_some_and(|s| !s.draft)));
    assert_eq!(rt.state.tasks.iter().filter(|t| t.subagent.as_ref().is_some_and(|s| s.placement == SubPlace::OwnCopy)).count(), 4);
}

#[test]
fn settings_offers_asking_before_a_threads_first_kitten_and_says_the_limits() {
    let rt = runtime((1200.0, 1500.0), false, scene("settings"));
    let sw = node(&rt, "settings-ask-first-kitten");
    assert_eq!(str_of(&sw, "name"), "Ask before a thread's first kitten");
    assert!(!sw.get("states").and_then(|s| s.as_array()).is_some_and(|s| s.iter().any(|x| x == "checked")), "off by default: {sw}");
    assert!(texts(&rt).iter().any(|x| x.starts_with("Up to 3 kittens of a thread work at once and 8 across Neko")), "{:?}", texts(&rt));
}
