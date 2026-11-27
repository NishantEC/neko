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
            } else if name.starts_with("briefing") && !name.contains("empty") && !name.contains("caught") {
                // The Briefing's totals line is the one other place that says what runs cost, as Claude Code reported it
                // (docs/knowledge/briefing.md): only that line, and only in the briefing's own scenes.
                assert!(money.iter().all(|t| t.contains("as Claude Code reported it")), "{name} at {size:?} shows money outside its totals line: {money:?}");
            } else {
                assert!(money.is_empty(), "{name} at {size:?} shows money: {money:?}");
            }
            assert!(!has_key(&rt, "day-usage"), "{name}: no day total at the sidebar's foot");
        }
    }
    let rt = runtime((1200.0, 800.0), false, scene("task-context-open"));
    let t = texts(&rt);
    assert!(t.iter().any(|x| x.contains("Not charged on a subscription")), "how to read the cost: {t:?}");
    assert!(t.iter().any(|x| x == "61.4k") && t.iter().any(|x| x == "200k"), "how full the context is, used and window: {t:?}");
    // The note is for a subscription only: a cost charged per call has none.
    let mut charged = runtime((1200.0, 800.0), false, scene("task-context-open"));
    charged.update(|s, _| {
        for b in s.books.values_mut() {
            for r in &mut b.runs {
                if let Some(u) = &mut r.usage {
                    u.pricing = neko_protocol::Pricing::Charged;
                }
            }
        }
    });
    settle(&mut charged);
    assert!(!texts(&charged).iter().any(|x| x.contains("Not charged")), "no subscription note for a charged cost");
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
    assert_eq!(live.label.as_deref(), Some("miso is thinking"), "the thread's cat, by name");
    // The head already shows the cat (portrait, name, ring): the line under it says only what is happening, never the cat again.
    assert!(texts(&rt).iter().any(|t| t.starts_with("Thinking…")), "the live line says what is happening: {:?}", texts(&rt));
    assert!(!texts(&rt).iter().any(|t| t.contains("is thinking") || t.contains("is working") || t.starts_with("Working…")), "one live line, and the cat's name once: {:?}", texts(&rt));
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
    assert!(person.visible[2] * person.visible[3] < 0.5, "hidden until hover or focus, and taking no room: {:?}", person.visible);
    // No loose gap under the person's words: the next block starts one turn's gap (20 points) after the bubble.
    let bubble = rect(&rt, "task-message-bubble");
    let next = rect(&rt, "turn-run_1");
    let gap = next[1] - (bubble[1] + bubble[3]);
    assert!((gap - 20.0).abs() <= 1.0, "the gap under the person's words is the turn gap, not {gap}");
}

#[test]
fn the_persons_copy_shows_on_hover_and_on_focus() {
    // A short conversation, so the person's words and the room under them are in sight (a long one opens at its end).
    let mut rt = runtime((1200.0, 800.0), false, scene("task-talked"));
    let [x, y, w, h] = rect(&rt, "task-message-bubble");
    rt.pointer_move(x + w / 2.0, y + h / 2.0);
    settle(&mut rt);
    let copy = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("copy-task-message")).unwrap();
    assert!(copy.visible[2] > 1.0 && copy.visible[3] > 1.0, "shown while the pointer is over the words: {:?}", copy.visible);
    rt.pointer_move(5.0, 790.0);
    settle(&mut rt);
    let copy = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("copy-task-message")).unwrap();
    assert!(copy.visible[2] * copy.visible[3] < 0.5, "hidden again: {:?}", copy.visible);
    while focus(&rt).as_deref() != Some("copy-task-message") {
        tab(&mut rt, false);
    }
    settle(&mut rt);
    let copy = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("copy-task-message")).unwrap();
    assert!(copy.visible[2] > 1.0, "shown while it has focus: {:?}", copy.visible);
}

#[test]
fn nothing_in_a_task_sits_further_apart_than_the_turn_gap() {
    for name in ["task-real-run-open", "task-done", "task-follow-up", "task-needs-you"] {
        let rt = runtime((1200.0, 800.0), false, scene(name));
        let content = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("transcript-content")).unwrap();
        // The direct children of the column: each turn. Between any two, at most the turn gap.
        let prefix = format!("{}/", content.path);
        let kids: Vec<_> = rt.elements().into_iter().filter(|e| e.path.strip_prefix(&prefix).is_some_and(|rest| !rest.contains('/'))).collect();
        assert!(kids.len() >= 2, "{name}: the turns are found");
        for pair in kids.windows(2) {
            let gap = pair[1].rect[1] - (pair[0].rect[1] + pair[0].rect[3]);
            assert!(gap <= 20.5, "{name}: {:?} and {:?} are {gap} apart", pair[0].key, pair[1].key);
        }
    }
}

#[test]
fn finished_runs_fold_and_the_choice_is_remembered() {
    let mut rt = runtime((1200.0, 800.0), false, scene("task-real-run"));
    assert!(has_key(&rt, "worked-run_1") && has_key(&rt, "worked-run_2"), "every run of the thread folds");
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
    assert_eq!(ring.label.as_deref(), Some("Context 31% used, 61.4k of 200k tokens"));
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
    // In sight without scrolling in the default window (1200 by 800).
    let r = rect(&rt, "settings-rules");
    assert!(r[1] + r[3] <= 800.0 - 16.0, "the rules line ends at {} in an 800 point window", r[1] + r[3]);
    assert!(!has_key(&runtime((1200.0, 800.0), false, scene("settings-ask")), "settings-rules"), "Neko's own rules apply there");
}

#[test]
fn a_run_that_ends_with_a_question_says_needs_you() {
    let rt = runtime((1200.0, 800.0), false, scene("task-asked"));
    let open = rt.state.open.clone().unwrap();
    let row = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(&format!("thread-{}", open.0))).expect("the thread's row");
    let tree = nodes(&rt.accessibility_json());
    let node = tree.iter().find(|(_, n)| str_of(n, "key") == format!("thread-{}", open.0)).map(|(_, n)| n.clone()).expect("the row in the tree");
    assert!(node.to_string().contains("needs you"), "the row says it needs you, not done: {node}");
    assert!(!rt.state.open_is_working(), "it is not working: the person can answer");
    assert!(row.rect[2] > 0.0);
    assert!(rt.accessibility_json().to_string().contains("Answer miso's question"), "the composer asks for the cat's answer");
}

#[test]
fn the_task_band_shows_its_folder_and_branch_once_with_reveal_in_finder() {
    let mut rt = runtime((1200.0, 800.0), false, scene("task-done"));
    let t = texts(&rt);
    assert_eq!(t.iter().filter(|x| x.as_str() == "~/Library/Application Support/neko/worktrees/x7k2m9qa").count(), 1, "the folder, once, with ~: {t:?}");
    assert!(t.iter().any(|x| x == "neko/x7k2m9qa"), "the branch beside it: {t:?}");
    click(&mut rt, "task-folder-reveal");
    let asked = rt.take_path_requests();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(asked[0].path, std::path::PathBuf::from("/Users/example/Library/Application Support/neko/worktrees/x7k2m9qa"));
    assert_eq!(asked[0].action, rune::PathAction::Reveal);
    // At the smallest window it still fits the band (cut in its middle, never over the title).
    let rt = runtime((760.0, 560.0), false, scene("task-done"));
    let band = rect(&rt, "content-band");
    let place = rect(&rt, "task-folder");
    assert!(place[0] + place[2] <= band[0] + band[2] + 0.5, "{place:?} in {band:?}");
}
