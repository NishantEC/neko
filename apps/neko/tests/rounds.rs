//! Rounds in the app (docs/knowledge/rounds.md), headless on scenes: the page and its words, the panel's keyboard order, what the
//! daemon's events change, the lines on Home, a task and a cat's page, and the two small fixes made with them.

mod support;
use neko_app::state::{App, Confirm};
use neko_protocol::{Change, Reply, RoundStatus, ServerMessage};
use rune::{Key, Mods, Runtime};
use support::*;

fn scene(name: &str) -> fn(&mut App) {
    scenes().iter().find(|s| s.0 == name).unwrap_or_else(|| panic!("no scene {name}")).1
}

fn focus(rt: &Runtime<App>) -> Option<String> {
    rt.presented_focus_key().map(str::to_owned)
}

fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn names(rt: &Runtime<App>) -> Vec<String> {
    nodes(&rt.accessibility_json()).iter().map(|(_, n)| str_of(n, "name").to_owned()).collect()
}

fn texts(rt: &Runtime<App>) -> String {
    names(rt).join("\n")
}

#[test]
fn command_3_opens_the_rounds_page_and_the_sidebar_row_is_chosen() {
    let mut rt = runtime((1200.0, 800.0), false, scene("tasks-home"));
    assert!(!has_key(&rt, "rounds-title"));
    press(&mut rt, Key::Command('3'), Mods { cmd: true, ..Mods::default() });
    assert!(rt.state.rounds_open, "⌘3 opens it");
    assert!(has_key(&rt, "rounds-title") && has_key(&rt, "rounds-empty"), "with no rounds it explains itself and offers New round…");
    assert!(has_key(&rt, "rounds-new-empty"));
    press(&mut rt, Key::Command('1'), Mods { cmd: true, ..Mods::default() });
    assert!(!rt.state.rounds_open, "Home leaves it");
}

#[test]
fn a_round_card_says_its_schedule_its_next_time_and_how_it_last_went() {
    let rt = runtime((1200.0, 800.0), false, scene("rounds"));
    let all = texts(&rt);
    assert!(all.contains("Docs check: mochi, Weekdays at 09:00"), "{all}");
    assert!(all.contains("Next: tomorrow 09:00"), "{all}");
    assert!(all.contains("Last: done, today at 09:00"), "{all}");
    assert!(all.contains("Every 6 hours") && all.contains("Only when you press Run now"), "{all}");
    for key in ["rounds-pause", "rounds-new"] {
        assert!(has_key(&rt, key), "{key}");
    }
    let id = rt.state.rounds[0].id.clone();
    for part in ["switch", "run", "menu", "open"] {
        assert!(has_key(&rt, &format!("round-{id}-{part}")), "{part} on the first card");
    }
}

#[test]
fn the_reasons_a_round_does_not_run_are_said_on_its_card() {
    let rt = runtime((1200.0, 800.0), false, scene("rounds-braked"));
    let all = texts(&rt);
    for words in ["Turned itself off after 3 failed runs in a row", "miso is retired", "Folder missing"] {
        assert!(all.contains(words), "{words}: {all}");
    }
    let rt = runtime((1200.0, 800.0), false, scene("rounds-paused"));
    assert!(texts(&rt).contains("Every round is paused"));
    assert!(has_key(&rt, "rounds-paused"));
    assert!(texts(&rt).contains("Resume rounds"), "the control says what it will do");
    assert!(rt.state.rounds.iter().filter(|r| r.on && r.schedule.is_automatic()).all(|r| r.status == RoundStatus::Paused));
}

#[test]
fn the_history_opens_in_the_card() {
    let rt = runtime((1200.0, 800.0), false, scene("rounds-history"));
    let all = texts(&rt);
    assert!(all.contains("History of Docs check"), "{all}");
    assert!(all.contains("Skipped, every round was paused"), "{all}");
    assert!(all.contains("Missed, Neko was not running"), "{all}");
}

#[test]
fn tab_goes_through_the_rounds_page_in_reading_order() {
    let mut rt = runtime((1200.0, 800.0), false, scene("rounds"));
    let id = rt.state.rounds[0].id.clone();
    let mut seen = Vec::new();
    for _ in 0..40 {
        tab(&mut rt, false);
        let Some(k) = focus(&rt) else { continue };
        if seen.first() == Some(&k) {
            break;
        }
        seen.push(k);
    }
    let at = |k: &str| seen.iter().position(|s| s.starts_with(k)).unwrap_or_else(|| panic!("{k} is not reached: {seen:?}"));
    let order = ["rounds-pause", "rounds-new", &format!("round-{id}-switch"), &format!("round-{id}-run"), &format!("round-{id}-menu"), &format!("round-{id}-open"), &format!("round-{id}-history")];
    let places: Vec<usize> = order.iter().map(|k| at(k)).collect();
    assert!(places.windows(2).all(|w| w[0] < w[1]), "reading order, left to right and down: {seen:?}");
}

#[test]
fn the_panel_names_everything_and_waits_for_a_name_and_words() {
    let mut rt = runtime((1200.0, 800.0), false, scene("round-new"));
    let all = texts(&rt);
    assert!(all.contains("The first run is tomorrow at 09:00."), "it says when, before saving: {all}");
    assert!(all.contains("Rounds run while Neko is running and this Mac is awake."));
    for key in ["round-name-input", "round-words-input", "round-cat", "round-workspace", "round-kind", "round-time-input", "round-cancel", "round-save"] {
        assert!(has_key(&rt, key), "{key}");
    }
    assert!(rt.state.round_form.as_ref().is_some_and(|f| f.ready()));
    rt.update(|s, _| s.form_mut().name = rune::TextEdit::new(""));
    settle(&mut rt);
    assert!(!rt.state.round_form.as_ref().unwrap().ready(), "no name, no Save");
    press(&mut rt, Key::Escape, Mods::default());
    assert!(rt.state.round_form.is_none(), "Escape closes the panel");
}

#[test]
fn each_schedule_kind_shows_its_own_fields_and_its_own_first_run() {
    for (scene_name, key, words) in [
        ("round-daily", "round-time-input", "The first run is today at 18:30."),
        ("round-hours", "round-hours-input", "The first run is today at 20:05, then every 6 hours."),
        ("round-manual", "round-kind", "It never starts by itself"),
        ("round-new", "round-day-0", "The first run is tomorrow at 09:00."),
    ] {
        let rt = runtime((1200.0, 800.0), false, scene(scene_name));
        assert!(has_key(&rt, key), "{scene_name}: {key}");
        assert!(texts(&rt).contains(words), "{scene_name}: {words}");
    }
    let rt = runtime((1200.0, 800.0), false, scene("round-invalid"));
    assert!(texts(&rt).contains("like 09:00"), "a time that is not a time says how to write one");
    assert!(!rt.state.round_form.as_ref().unwrap().ready());
}

#[test]
fn the_panel_is_reached_in_reading_order_by_keyboard() {
    let mut rt = runtime((1200.0, 800.0), false, scene("round-new"));
    let mut seen = Vec::new();
    for _ in 0..30 {
        tab(&mut rt, false);
        let Some(k) = focus(&rt) else { continue };
        if seen.first() == Some(&k) {
            break;
        }
        seen.push(k);
    }
    let at = |k: &str| seen.iter().position(|s| s == k).unwrap_or_else(|| panic!("{k} is not reached: {seen:?}"));
    let order = ["round-name-input", "round-words-input", "round-cat", "round-workspace", "round-day-0", "round-time-input", "round-cancel", "round-save"];
    let places: Vec<usize> = order.iter().map(|k| at(k)).collect();
    assert!(places.windows(2).all(|w| w[0] < w[1]), "top to bottom: {seen:?}");
    assert!(seen.iter().all(|k| k.starts_with("round") || k.starts_with("dialog")), "the keyboard stays in the panel: {seen:?}");
}

#[test]
fn a_dialog_never_leaves_the_pickers_under_it() {
    let rt = runtime((1200.0, 800.0), false, scene("round-cat-menu"));
    assert!(texts(&rt).contains("miso"), "the cat menu lists the cats");
}

#[test]
fn the_daemons_events_keep_the_list_and_the_pause_current() {
    let mut rt = runtime((1200.0, 800.0), false, scene("rounds"));
    let mut round = rt.state.rounds[0].clone();
    round.name = "Docs links".into();
    round.on = false;
    round.status = RoundStatus::Off;
    round.next_due_ms = None;
    let heard = round.clone();
    rt.update(move |s, _| s.change(Change::RoundUpdated { round: heard }));
    settle(&mut rt);
    assert_eq!(rt.state.rounds.len(), 3);
    assert_eq!(rt.state.rounds[0].name, "Docs links");
    assert!(texts(&rt).contains("Off"));
    let gone = round.id.clone();
    rt.update(move |s, _| s.change(Change::RoundRemoved { round: gone }));
    assert_eq!(rt.state.rounds.len(), 2);
    rt.update(|s, _| s.change(Change::RoundsPaused { paused: true }));
    assert!(rt.state.rounds_paused);
    // A reply to the panel's Save closes it; a refusal leaves it open with the words.
    rt.update(|s, _| {
        s.start_new_round(None);
        s.asked.insert(7, neko_app::state::Asked::RoundForm);
        s.take(neko_app::link::Incoming::Message(ServerMessage::Reply { id: 7, reply: Reply::error(neko_protocol::ErrorCode::Refused, "miso is retired.") }));
    });
    assert_eq!(rt.state.round_form.as_ref().and_then(|f| f.error.clone()).as_deref(), Some("miso is retired."));
    let kept = rt.state.rounds[0].clone();
    rt.update(move |s, _| {
        s.asked.insert(8, neko_app::state::Asked::RoundForm);
        s.take(neko_app::link::Incoming::Message(ServerMessage::Reply { id: 8, reply: Reply::Round { round: kept } }));
    });
    assert!(rt.state.round_form.is_none());
}

#[test]
fn removing_a_round_asks_first() {
    let rt = runtime((1200.0, 800.0), false, scene("rounds-remove"));
    assert!(matches!(rt.state.confirm, Some(Confirm::RemoveRound(_))));
    assert!(texts(&rt).contains("The tasks it made stay where they are."));
}

#[test]
fn home_says_which_round_is_next_and_nothing_when_there_is_none() {
    let rt = runtime((1200.0, 800.0), false, scene("home-next-round"));
    assert!(texts(&rt).contains("Next round: miso, Release notes, today 18:35"), "{}", texts(&rt));
    let mut rt = runtime((1200.0, 800.0), false, scene("home-next-round"));
    rt.update(|s, _| s.rounds.clear());
    settle(&mut rt);
    assert!(!has_key(&rt, "home-next-round"));
    let rt = runtime((1200.0, 800.0), false, scene("empty-home"));
    assert!(!has_key(&rt, "home-next-round"));
}

#[test]
fn a_task_a_round_made_says_so_and_links_back() {
    let rt = runtime((1200.0, 800.0), false, scene("task-from-round"));
    assert!(texts(&rt).contains("From the round Docs check, due today at 09:00"), "{}", texts(&rt));
    assert!(has_key(&rt, "task-round-open"));
    let rt = runtime((1200.0, 800.0), false, scene("task-done"));
    assert!(!has_key(&rt, "task-round"), "a task the person made has no such line");
}

#[test]
fn a_cats_page_lists_its_rounds_and_offers_a_new_one() {
    let mut rt = runtime((1200.0, 800.0), false, scene("cat-page-rounds"));
    assert!(texts(&rt).contains("Docs check: mochi, Weekdays at 09:00"));
    assert!(has_key(&rt, "cat-new-round"));
    rt.update(|s, _| s.start_new_round(Some("cat_mochi")));
    let f = rt.state.round_form.as_ref().expect("the panel opens");
    assert_eq!(f.cat, "cat_mochi", "for this cat");
    let rt = runtime((1200.0, 800.0), false, scene("cat-page"));
    assert!(texts(&rt).contains("miso has no rounds"));
}

#[test]
fn nothing_is_clipped_at_the_smallest_window() {
    for name in ["rounds", "rounds-braked", "round-new", "round-hours", "task-from-round", "cat-page-rounds"] {
        let rt = runtime((760.0, 560.0), false, scene(name));
        for e in rt.elements() {
            if let Some(k) = e.key.as_deref().filter(|k| k.starts_with("round-") && (k.ends_with("-switch") || k.ends_with("-run") || k.ends_with("-menu"))) {
                let r = e.rect;
                assert!(r[0] >= 0.0 && r[0] + r[2] <= 760.0, "{name}: {k} is cut by the window's edge ({r:?})");
            }
        }
    }
}

#[test]
fn a_run_with_no_steps_shows_only_its_time() {
    // "0 steps · 9s" says nothing: with no steps the counter is the time alone.
    let mut rt = runtime((1200.0, 800.0), false, scene("task-done"));
    rt.update(|s, _| {
        let id = s.open.clone().unwrap();
        s.books.get_mut(&id).unwrap().parts.retain(|p| !matches!(p.body, neko_protocol::PartBody::Tool { .. }));
    });
    settle(&mut rt);
    let all = texts(&rt);
    assert!(!all.contains("0 steps"), "{all}");
}

/// The step count the band's chip says and the one the run's fold says, from the elements of a scene.
fn step_counts(rt: &Runtime<App>) -> (Option<String>, Option<String>) {
    let all = rt.elements();
    let chip = all.iter().find(|e| e.key.as_deref() == Some("run-counter")).and_then(|e| e.label.clone());
    let fold = all.iter().filter_map(|e| e.text.clone()).find(|t| t.ends_with(" steps") || t.ends_with(" step") || t.contains(" steps ·") || t.contains(" step ·"));
    (chip, fold)
}

fn first_number(s: &str) -> Option<u32> {
    s.split_whitespace().next().and_then(|w| w.parse().ok())
}

#[test]
fn the_bands_chip_and_the_runs_fold_count_the_same_steps() {
    for name in ["task-from-round", "task-done", "task-real-run"] {
        let rt = runtime((1200.0, 800.0), false, scene(name));
        let (chip, fold) = step_counts(&rt);
        let (chip, fold) = (chip.unwrap_or_else(|| panic!("{name}: no chip")), fold.unwrap_or_else(|| panic!("{name}: no fold count")));
        assert_eq!(first_number(&chip), first_number(&fold), "{name}: the chip says {chip:?}, the fold says {fold:?}");
    }
}

#[test]
fn a_manual_round_has_no_switch_and_no_line_repeating_its_schedule() {
    let rt = runtime((1200.0, 800.0), false, scene("rounds"));
    let manual = rt.state.rounds.iter().find(|r| !r.schedule.is_automatic()).expect("a manual round").id.clone();
    let auto = rt.state.rounds.iter().find(|r| r.schedule.is_automatic()).expect("a scheduled round").id.clone();
    assert!(!has_key(&rt, &format!("round-{manual}-switch")), "nothing to switch");
    assert!(has_key(&rt, &format!("round-{manual}-run")), "Run now stays");
    assert!(!has_key(&rt, &format!("round-{manual}-next")), "no second line saying it runs only when pressed");
    assert!(has_key(&rt, &format!("round-{auto}-switch")), "a scheduled round has its switch");
    assert!(texts(&rt).contains("Only when you press Run now"));
}

#[test]
fn a_task_from_a_round_lists_its_files_with_their_figures_and_its_line_has_its_own_ground() {
    let rt = runtime((1200.0, 800.0), false, scene("task-from-round"));
    let all = texts(&rt);
    assert!(all.contains("src/toolbar.rs") && all.contains("src/menu.rs") && all.contains("docs/preferences.md"), "every file, as an ordinary task's receipt lists them: {all}");
    assert!(has_key(&rt, "task-round-strip"));
}
