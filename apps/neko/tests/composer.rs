//! The composer while the agent works (docs/knowledge/composer.md), on scenes: what the placeholder and the hints say, the mode beside
//! Send, the messages that wait above the box and what can be done with each, four queued ones folded, and the activity bar closed and
//! open. The modes themselves, and what reaches the agent, are checked against a real daemon in `crates/neko-daemon/tests/composer.rs`.

mod support;
use neko_app::state::App;
use rune::{Key, Mods, Runtime};
use support::*;

fn scene(name: &str) -> fn(&mut App) {
    scenes().iter().find(|s| s.0 == name).unwrap_or_else(|| panic!("no scene {name}")).1
}

fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn node(rt: &Runtime<App>, key: &str) -> serde_json::Value {
    nodes(&rt.accessibility_json()).into_iter().map(|(_, n)| n).find(|n| str_of(n, "key") == key).unwrap_or_else(|| panic!("{key} is not in the tree"))
}

#[test]
fn the_placeholder_says_what_return_does_and_nothing_extra_shows_at_rest() {
    let rt = runtime((1200.0, 800.0), false, scene("composer-working"));
    assert_eq!(str_of(&node(&rt, "reply-input"), "placeholder"), "Steer the agent (⌥↩ to queue)");
    assert_eq!(str_of(&node(&rt, "reply-submit"), "name"), "Stop", "an empty box offers Stop");
    for k in ["waiting", "activity-bar", "send-mode", "reply-hints"] {
        assert!(!has_key(&rt, k), "{k} is not shown with nothing to say");
    }
    let rest = runtime((1200.0, 800.0), false, scene("composer-idle"));
    assert_eq!(str_of(&node(&rest, "reply-input"), "placeholder"), "Message the agent");
    assert!(!has_key(&rest, "send-mode") && !has_key(&rest, "waiting") && !has_key(&rest, "activity-bar"));
}

#[test]
fn typed_words_turn_stop_into_send_and_show_the_mode_and_the_hints() {
    let rt = runtime((1200.0, 800.0), false, scene("composer-typing"));
    assert_eq!(str_of(&node(&rt, "reply-submit"), "name"), "Send", "words to send, so the button sends");
    assert_eq!(str_of(&node(&rt, "send-mode"), "name"), "Send as: Steer");
    assert_eq!(str_of(&node(&rt, "reply-hints"), "name"), "Return steers · ⌥Return queues · ⌘Return interrupts");
}

#[test]
fn the_mode_menu_offers_the_three_ways_with_their_keys() {
    let mut rt = runtime((1200.0, 800.0), false, scene("composer-typing"));
    click(&mut rt, "send-mode");
    settle(&mut rt);
    let rows: Vec<String> = rt.elements().into_iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("send-mode-menu-item"))).filter_map(|e| e.label).collect();
    assert_eq!(rows.len(), 3, "{rows:?}");
    for (row, word) in rows.iter().zip(["Steer", "Queue", "Interrupt"]) {
        assert!(row.contains(word), "{word} is offered, in this order: {rows:?}");
    }
}

#[test]
fn a_steered_message_has_no_actions_and_a_queued_one_has_three() {
    let rt = runtime((1200.0, 800.0), false, scene("composer-queued"));
    assert_eq!(str_of(&node(&rt, "waiting-0"), "name"), "Steered, waiting to be read: Use the word Preferences, not Settings, in the toolbar");
    assert!(!has_key(&rt, "waiting-0-send") && !has_key(&rt, "waiting-0-edit") && !has_key(&rt, "waiting-0-discard"), "it is already in the agent");
    for n in [1, 2] {
        for what in ["send", "edit", "discard"] {
            assert!(has_key(&rt, &format!("waiting-{n}-{what}")), "waiting-{n}-{what}");
        }
    }
    assert_eq!(str_of(&node(&rt, "waiting-1-discard"), "name"), "Discard");
}

#[test]
fn an_interrupt_that_is_going_has_no_actions() {
    let rt = runtime((1200.0, 800.0), false, scene("composer-interrupting"));
    assert!(str_of(&node(&rt, "waiting-0"), "name").starts_with("Interrupting: Stop that."));
    assert!(!has_key(&rt, "waiting-0-discard"), "the turn is already being stopped");
    assert!(has_key(&rt, "waiting-1-send"), "the queued one behind it can still be sent");
}

#[test]
fn more_than_two_queued_fold_behind_a_count_and_open_on_demand() {
    let mut rt = runtime((1200.0, 800.0), false, scene("composer-queued-folded"));
    assert_eq!(str_of(&node(&rt, "waiting-fold"), "name"), "4 queued");
    assert!(!has_key(&rt, "waiting-0"), "folded: no row of its own");
    click(&mut rt, "waiting-fold");
    settle(&mut rt);
    assert!(rt.state.composer.queue_open);
    assert!(has_key(&rt, "waiting-1") && has_key(&rt, "waiting-4"), "open: all four, after the count");
}

#[test]
fn the_activity_bar_is_one_line_that_opens_to_each_kitten_with_its_own_stop() {
    let mut rt = runtime((1200.0, 800.0), false, scene("activity-collapsed"));
    assert_eq!(str_of(&node(&rt, "activity-bar"), "name"), "Running: 1 kitten working, 1 waiting its turn", "one works and one waits its turn: the bar does not call both working");
    assert!(!has_key(&rt, "activity-list"));
    click(&mut rt, "activity-bar");
    settle(&mut rt);
    assert!(has_key(&rt, "activity-list"));
    assert!(has_key(&rt, "activity-kitten-1-stop") && has_key(&rt, "activity-kitten-2-stop"));
    assert_eq!(str_of(&node(&rt, "activity-kitten-1-stop"), "name"), "Stop Kitten 1");
    assert!(has_key(&rt, "activity-run-stop"), "the agent itself is the first line, with the same stop");
}

#[test]
fn a_thread_with_no_kitten_and_no_long_command_has_no_activity_bar() {
    for name in ["composer-working", "composer-queued", "task-working"] {
        let rt = runtime((1200.0, 800.0), false, scene(name));
        assert!(!has_key(&rt, "activity-bar"), "{name}");
    }
}

#[test]
fn escape_stops_a_working_thread_even_with_words_typed_and_up_brings_a_message_back() {
    let mut rt = runtime((1200.0, 800.0), false, scene("composer-typing"));
    click(&mut rt, "reply-input");
    // No daemon here: Stop has nothing to reach, so what is checked is that the key is taken (the field does not blur on it).
    press(&mut rt, Key::Escape, Mods::default());
    assert!(rt.state.reply_box().is_some_and(|r| r.text == "Also rename it in the help pages"), "the words stay");
    // ↑ in an empty box brings the thread's last message back, ↓ gives the empty draft back.
    let mut rt = runtime((1200.0, 800.0), false, scene("task-follow-up"));
    click(&mut rt, "reply-input");
    press(&mut rt, Key::Up, Mods::default());
    assert_eq!(rt.state.reply_box().map(|r| r.text.clone()).as_deref(), Some("Also rename it in the help pages."));
    press(&mut rt, Key::Down, Mods::default());
    assert_eq!(rt.state.reply_box().map(|r| r.text.clone()).as_deref(), Some(""));
}
