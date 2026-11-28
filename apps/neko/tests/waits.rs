//! Timers, watches and standing rounds in the app (docs/knowledge/waits.md), and words steered into a running turn
//! (docs/knowledge/composer.md), headless on scenes: what a resting thread says in its row, its conversation, its activity bar and its
//! Overview; Stop watching; the note where a timer woke it; a standing round's message; the Rounds card of a round of nobody's; the
//! panel's switch; and the person's steered words drawn where the agent read them.

mod support;
use neko_app::state::App;
use rune::Runtime;
use support::*;

fn scene(name: &str) -> fn(&mut App) {
    scenes().iter().find(|s| s.0 == name).unwrap_or_else(|| panic!("no scene {name}")).1
}

fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn texts(rt: &Runtime<App>) -> String {
    nodes(&rt.accessibility_json()).iter().map(|(_, n)| str_of(n, "name").to_owned()).collect::<Vec<_>>().join("\n")
}

/// Where an element with this key is, top to bottom.
fn top(rt: &Runtime<App>, key: &str) -> f32 {
    rt.elements().iter().find(|e| e.key.as_deref() == Some(key)).map(|e| e.rect[1]).unwrap_or_else(|| panic!("no {key}"))
}

#[test]
fn a_thread_resting_on_its_timer_says_until_when_in_its_row_its_conversation_and_above_the_composer() {
    let rt = runtime((1200.0, 800.0), false, scene("waits-timer"));
    let all = texts(&rt);
    assert!(all.contains("Waiting until 18:00"), "the waiting line: {all}");
    let t = rt.state.open_task().cloned().expect("the thread");
    assert_eq!(rt.state.thread_line(&t).as_deref(), Some("Waiting until 18:00"), "its row's second line");
    assert!(has_key(&rt, "waiting-line"));
    assert!(has_key(&rt, "activity-wait-wait_1"), "one quiet line above the composer");
    assert!(all.contains("Until 18:00, Check the install page is live and the old links redirect."), "{all}");
    assert!(all.contains("Stop the timer"));
    assert_eq!(neko_app::composer::placeholder(&rt.state, &t, false), "The agent is waiting until 18:00. Say something to wake it now", "the composer says what a message does");
}

#[test]
fn stop_watching_asks_the_daemon_to_stop_that_one() {
    let mut rt = runtime((1200.0, 800.0), false, scene("waits-watch"));
    let all = texts(&rt);
    assert!(all.contains("Waiting on a file change or a check"), "{all}");
    assert!(has_key(&rt, "pane-wait-wait_2") && has_key(&rt, "pane-wait-wait_3"), "Overview's Waiting on lists both");
    assert!(all.contains("Watching build/deploy.log") && all.contains("Checking curl -fsI https://docs.example.com/install"), "{all}");
    // Each line has its own Stop watching (the press sends `stop_waiting`; the daemon's tests check what it does).
    assert!(has_key(&rt, "activity-wait-wait_2-stop") && has_key(&rt, "pane-wait-wait_3-stop"));
    assert!(all.matches("Stop watching").count() >= 4, "two lines above the composer and two in Overview: {all}");
    click(&mut rt, "activity-wait-wait_2-stop");
}

#[test]
fn the_note_says_what_woke_the_thread_and_folds_its_note() {
    let rt = runtime((1200.0, 800.0), false, scene("waits-woken"));
    let all = texts(&rt);
    assert!(all.contains("Woken by its timer"), "{all}");
    assert!(all.contains("Its note"), "{all}");
    assert!(!has_key(&rt, "waiting-line"), "it is done, not waiting");
    let rt = runtime((1200.0, 800.0), false, scene("waits-paused"));
    assert!(texts(&rt).contains("Not woken at 18:00: automatic work was paused."), "the thread says a skipped wake");
}

#[test]
fn a_standing_rounds_words_are_a_message_from_the_round() {
    let rt = runtime((1200.0, 800.0), false, scene("round-standing"));
    let all = texts(&rt);
    assert!(all.contains("The round Morning check said"), "a message from the round, not the person: {all}");
    assert!(all.contains("Look over the open pull requests on acme-api and tell me which are ready to merge."));
}

#[test]
fn a_round_of_nobodys_is_led_by_the_rounds_mark_and_a_standing_one_says_where_it_posts() {
    let rt = runtime((1200.0, 800.0), false, scene("rounds"));
    let all = texts(&rt);
    assert!(all.contains("Tidy downloads: Only when you press Run now"), "a round of nobody's names no cat: {all}");
    assert!(all.contains("Morning check: miso, in its own thread, Every day at 08:30"), "{all}");
    let lead = |id: &str| rt.elements().iter().find(|e| e.key.as_deref() == Some(&format!("round-{id}-face"))).map(|e| e.rect).unwrap();
    let (face, mark) = (lead("round_00000000000000000000000001"), lead("round_00000000000000000000000003"));
    assert_eq!((face[0], face[2]), (mark[0], mark[2]), "the mark takes a face's room, so the words line up");
    let rt = runtime((1200.0, 800.0), false, scene("round-new-standing"));
    assert!(texts(&rt).contains("Post in miso's own thread"));
    assert!(!has_key(&rt, "round-workspace"), "a standing round has no project to choose");
}

#[test]
fn words_steered_into_a_run_stand_where_the_agent_read_them() {
    // Finished: one fold, then the person's words, then the answer that followed.
    let rt = runtime((1200.0, 800.0), false, scene("steered-done"));
    let words = rt.elements().iter().find(|e| e.label.as_deref() == Some("Also make the warning red, not orange.") || e.key.as_deref().is_some_and(|k| k.ends_with("-words") && k.starts_with("part-"))).map(|e| e.rect[1]).expect("the person's words");
    let fold = top(&rt, "worked-run_1");
    let answer = rt.elements().iter().find(|e| e.key.as_deref() == Some("turn-run_1-2") || e.key.as_deref() == Some("turn-run_1-1")).map(|e| e.rect[1]).expect("the answer's piece of the turn");
    assert!(fold < words && words < answer, "fold {fold}, words {words}, answer {answer}");
    assert_eq!(rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("worked-run_1") && !k.contains('-') || k == "worked-run_1")).count(), 1, "one fold for the run");
    // Working: the turn is cut at the words; the live steps after them.
    let rt = runtime((1200.0, 800.0), false, scene("steered-working"));
    assert!(has_key(&rt, "turn-run_1") && rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("turn-run_1-"))), "two pieces of one run");
}
