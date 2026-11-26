//! What the first real run taught, held in every scene: no money on screen until the context ring is opened, a run card only when
//! it has something to show, no empty thoughts, a command that exits non-zero is not a failure, steps read as phrases, one reading
//! column, copy only where it helps, finished runs fold, and the ring is reached and left by the keyboard.

mod support;
use rune::{Key, Mods, Runtime};
use support::*;

use neko_app::state::App;

fn scene(name: &str) -> fn(&mut App) {
    scenes().iter().find(|s| s.0 == name).unwrap_or_else(|| panic!("no scene {name}")).1
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter(|e| e.opacity > 0.0).filter_map(|e| e.text).collect()
}

fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key} is not on screen")).rect
}

fn focus(rt: &Runtime<App>) -> Option<String> {
    rt.presented_focus_key().map(str::to_owned)
}

#[test]
fn no_money_is_on_screen_until_the_context_ring_is_opened() {
    for &(name, sc) in scenes() {
        for size in SIZES {
            let rt = runtime(size, false, sc);
            let money: Vec<String> = texts(&rt).into_iter().filter(|t| t.contains('$')).collect();
            if name == "task-context-open" {
                assert!(money.iter().any(|t| t == "$0.96"), "{name}: the cost is in the panel: {money:?}");
            } else {
                assert!(money.is_empty(), "{name} at {size:?} shows money: {money:?}");
            }
            assert!(!has_key(&rt, "day-usage"), "{name}: no day total at the sidebar's foot");
        }
    }
    let rt = runtime((1200.0, 800.0), false, scene("task-context-open"));
    let t = texts(&rt);
    assert!(t.iter().any(|x| x.contains("Not charged on a subscription")), "how to read the cost: {t:?}");
    assert!(t.iter().any(|x| x == "61.4k of 200k"), "how full the context is: {t:?}");
}

#[test]
fn a_run_card_shows_only_with_changes_a_branch_or_a_failure() {
    let rt = runtime((1200.0, 800.0), false, scene("task-talked"));
    assert!(!has_key(&rt, "run-run_1"), "a run that only talked has no card");
    assert!(texts(&rt).iter().any(|t| t == "Worked for 29s"), "its one quiet line carries the time");
    assert!(has_key(&runtime((1200.0, 800.0), false, scene("task-done")), "run-run_1-facts"), "files and a branch");
    assert!(has_key(&runtime((1200.0, 800.0), false, scene("task-failed")), "run-run_1"), "a failure");
    assert!(!has_key(&runtime((1200.0, 800.0), false, scene("task-real-run")), "run-run_1"), "commands only, in a scratch folder");
}

#[test]
fn empty_thoughts_show_nothing_and_thinking_shows_one_live_line() {
    let rt = runtime((1200.0, 800.0), false, scene("task-real-run-open"));
    assert!(!texts(&rt).iter().any(|t| t.starts_with("Thought")), "no fold that opens to nothing");
    let rt = runtime((1200.0, 800.0), false, scene("task-thinking"));
    let live = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("run-run_1-working")).expect("the live line");
    assert_eq!(live.label.as_deref(), Some("Neko is thinking"));
    assert!(texts(&rt).iter().any(|t| t == "Thinking…"));
    assert!(!texts(&rt).iter().any(|t| t == "Working…"), "one live line");
}

#[test]
fn a_command_that_exits_non_zero_finished_and_a_denied_step_says_so() {
    let rt = runtime((1200.0, 800.0), false, scene("task-real-run-open"));
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "exit 1"), "{t:?}");
    assert!(t.iter().any(|x| x == "Denied"), "{t:?}");
    assert!(!t.iter().any(|x| x == "Failed"), "nothing failed here: {t:?}");
    assert!(!rt.elements().iter().any(|e| e.label.as_deref() == Some("Failed")), "no failed mark");
}

#[test]
fn steps_read_as_phrases() {
    let rt = runtime((1200.0, 800.0), false, scene("task-real-run-open"));
    let t = texts(&rt);
    assert!(!t.iter().any(|x| x.starts_with("Bash")), "a shell step is its command: {t:?}");
    assert!(t.iter().any(|x| x.starts_with("git diff --quiet")), "{t:?}");
    assert!(t.iter().any(|x| x.starts_with("git status --short") && x.contains('…')), "a long command is cut in its middle: {t:?}");
    assert!(!t.iter().any(|x| x.ends_with("0.0s") || x == "0s"), "no duration under a second: {t:?}");
    let mut follow = runtime((1200.0, 800.0), false, scene("task-follow-up"));
    follow.update(|s, _| s.flip("worked-run_2"));
    settle(&mut follow);
    assert!(texts(&follow).iter().any(|x| x == "Edited docs/help.md"), "{:?}", texts(&follow));
}

#[test]
fn one_reading_column() {
    for size in SIZES {
        let mut rt = runtime(size, false, scene("task-real-run-open"));
        rt.update(|s, _| s.sidebar_choice = Some(false));
        settle(&mut rt);
        let edge = rect(&rt, "reply-area")[0];
        let width = rect(&rt, "reply-area")[2];
        for key in ["worked-run_1", "worked-run_2", "part-4-answer", "part-10-answer", "part-2", "part-7", "transcript-content"] {
            let r = rect(&rt, key);
            let x = if key == "transcript-content" { r[0] + (r[2] - width) / 2.0 } else { r[0] };
            assert!((x - edge).abs() <= 1.0, "{size:?}: {key} starts at {x}, the column at {edge}");
        }
    }
}

#[test]
fn copy_is_for_final_answers_and_the_persons_words_on_hover() {
    let rt = runtime((1200.0, 800.0), false, scene("task-real-run-open"));
    let copies: Vec<String> = rt.elements().into_iter().filter_map(|e| e.key).filter(|k| k.starts_with("copy-part-")).collect();
    assert_eq!(copies.len(), 3, "two final answers and the person's follow-up: {copies:?}");
    assert!(!copies.contains(&"copy-part-4".to_owned()), "interim words have none: {copies:?}");
    let person = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("copy-task-message")).expect("the person's copy button");
    assert!(person.opacity < 0.01, "hidden until hover or focus: {}", person.opacity);
}

#[test]
fn finished_runs_fold_and_the_choice_is_remembered() {
    let mut rt = runtime((1200.0, 800.0), false, scene("task-real-run"));
    assert!(has_key(&rt, "worked-run_1") && has_key(&rt, "worked-run_2"), "every run of the task folds");
    assert!(!texts(&rt).iter().any(|t| t.contains("The guide is current")), "interim words fold away");
    while focus(&rt).as_deref() != Some("worked-run_1-trigger") {
        tab(&mut rt, false);
    }
    press(&mut rt, Key::Enter, Mods::default());
    settle(&mut rt);
    // (It fades in: present, whatever its opacity this frame.)
    assert!(rt.elements().into_iter().filter_map(|e| e.text).any(|t| t.contains("The guide is current")));
    assert!(rt.state.is_open("worked-run_1") && !rt.state.is_open("worked-run_2"), "for that run only");
}

#[test]
fn the_context_ring_is_named_reached_and_left_by_the_keyboard() {
    let mut rt = runtime((1200.0, 800.0), false, scene("task-real-run"));
    let ring = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("ctx-trigger")).expect("the ring");
    assert_eq!(ring.label.as_deref(), Some("Context 31% used"));
    let mut seen = Vec::new();
    for _ in 0..40 {
        tab(&mut rt, false);
        let k = focus(&rt);
        if k.as_deref() == Some("ctx-trigger") {
            break;
        }
        seen.push(k);
    }
    assert_eq!(focus(&rt).as_deref(), Some("ctx-trigger"), "Tab reaches it: {seen:?}");
    press(&mut rt, Key::Enter, Mods::default());
    settle(&mut rt);
    assert!(rt.state.context_open && has_key(&rt, "ctx-panel"), "Return opens the panel");
    press(&mut rt, Key::Escape, Mods::default());
    // Let its closing fade finish.
    for _ in 0..6 {
        rt.frame(0.25);
    }
    assert!(!rt.state.context_open && !has_key(&rt, "ctx-panel"), "Escape closes it");
    assert_eq!(focus(&rt).as_deref(), Some("ctx-trigger"), "and gives focus back to the ring");
}

#[test]
fn settings_name_three_modes_and_say_what_claude_codes_own_rules_allow() {
    let rt = runtime((1200.0, 800.0), false, scene("settings"));
    let t: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
    for mode in ["Follow Claude Code's own rules", "Ask before risky steps", "Full access"] {
        assert!(t.iter().any(|x| x == mode), "{mode}: {t:?}");
    }
    assert!(has_key(&rt, "settings-rules"), "the one honest line, following Claude Code's rules");
    assert!(t.iter().any(|x| x == "Your Claude Code settings allow Bash, Edit and Write without asking."));
    assert!(!has_key(&runtime((1200.0, 800.0), false, scene("settings-ask")), "settings-rules"), "Neko's own rules apply there");
}
