//! The right pane (docs/knowledge/app-shell.md, "The right pane"), on scenes: open by width and by the person's choice, a sheet in a
//! narrow window, its tabs, Overview's sections, the change line that replaced the receipt card, a kitten's tab beside its thread
//! (no two elements of the window share a key), and the keys. The daemon's side is `crates/neko-daemon/tests/undo.rs`.

mod support;
use std::collections::HashSet;

use neko_app::pane::Tab;
use neko_app::state::App;
use neko_protocol::TaskId;
use rune::{Key, Mods, Runtime};
use support::*;

fn scene(name: &str) -> fn(&mut App) {
    scenes().iter().find(|s| s.0 == name).unwrap_or_else(|| panic!("no scene {name}")).1
}

fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key} is not on screen")).rect
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter(|e| e.opacity > 0.0).filter_map(|e| e.text).collect()
}

fn cmd_alt(rt: &mut Runtime<App>, c: char) {
    press(rt, Key::Command(c), Mods { cmd: true, alt: true, ..Mods::default() });
}

/// A frame later than the page swap's fade, so what came in can be pressed.
fn arrive(rt: &mut Runtime<App>) {
    rt.frame(0.3);
    settle(rt);
}

#[test]
fn the_pane_is_open_from_1240_points_and_shut_below_until_the_person_opens_it() {
    // A thread in view, at the three widths that decide it.
    let wide = runtime((1280.0, 800.0), false, scene("task-done"));
    assert!(has_key(&wide, "pane") && has_key(&wide, "pane-tabs") && has_key(&wide, "pane-hide"), "open by default at 1280");
    assert!(!has_key(&wide, "pane-show"), "the band's button is in the pane's header then");
    let [_, _, w, _] = rect(&wide, "window-right");
    assert!((w - 380.0).abs() < 1.0, "380 points by default: {w}");

    let mut mid = runtime((1200.0, 800.0), false, scene("task-done"));
    assert!(!has_key(&mid, "pane") && has_key(&mid, "pane-show"), "shut at 1200, with its button in the band");
    cmd_alt(&mut mid, 'b');
    assert!(has_key(&mid, "pane") && has_key(&mid, "pane-hide"), "⌘⌥B opens it");
    cmd_alt(&mut mid, 'b');
    assert!(!has_key(&mid, "pane"), "and shuts it again");
    // Nothing but a thread has a pane: Home, Settings and the lists have none and no button.
    let home = runtime((1280.0, 800.0), false, scene("empty-home"));
    assert!(!has_key(&home, "pane") && !has_key(&home, "pane-show") && !has_key(&home, "pane-hide"));
}

#[test]
fn under_1100_points_the_pane_is_a_sheet_that_escape_closes_and_the_title_keeps_its_room() {
    let mut rt = runtime((1000.0, 700.0), false, scene("task-done"));
    assert!(!has_key(&rt, "pane") && !has_key(&rt, "pane-sheet") && has_key(&rt, "pane-show"), "a sheet is shut until it is asked for");
    assert!(rect(&rt, "band-title")[2] >= 140.0, "the title keeps its 140 points");
    click(&mut rt, "pane-show");
    arrive(&mut rt);
    assert!(has_key(&rt, "pane-sheet") && has_key(&rt, "pane-tabs"), "the sheet holds the same tabs");
    let sheet = rect(&rt, "pane-sheet");
    assert!(sheet[0] + sheet[2] <= 1000.5 && sheet[2] <= 400.0, "a panel against the right edge: {sheet:?}");
    press(&mut rt, Key::Escape, Mods::default());
    assert!(!rt.state.pane.sheet, "Escape closes it");
    assert!(rt.state.open.is_some(), "and nothing else was stopped or left");
    // The band keeps the title's 140 points at the smallest window too, sheet open or shut.
    let mut small = runtime((760.0, 560.0), false, scene("task-done"));
    assert!(rect(&small, "band-title")[2] >= 140.0);
    small.update(|s, _| s.pane.sheet = true);
    settle(&mut small);
    assert!(has_key(&small, "pane-sheet") && rect(&small, "band-title")[2] >= 140.0);
}

#[test]
fn the_tabs_are_overview_changes_then_a_tab_for_each_kitten_opened() {
    let mut rt = runtime((1280.0, 800.0), false, scene("subagents-waiting"));
    let root = rt.state.pane_root().expect("a thread is open");
    assert_eq!(rt.state.tabs_of(&root), [Tab::Overview, Tab::Changes], "no kitten is a tab until it is opened");
    assert_eq!(rt.state.tab_of(&root), Tab::Overview, "Overview first");
    click(&mut rt, "subagent-open-2");
    arrive(&mut rt);
    let kitten = |n: u32| TaskId(format!("task_{:026}", 40 + n));
    assert_eq!(rt.state.tabs_of(&root), [Tab::Overview, Tab::Changes, Tab::Kitten(kitten(2))]);
    assert_eq!(rt.state.tab_of(&root), Tab::Kitten(kitten(2)));
    assert!(has_key(&rt, "pane-kitten") && has_key(&rt, "pane-transcript") && has_key(&rt, "task-words"), "the kitten's conversation beside the thread's");
    // A kitten's tab shows the conversation read-only: no composer in it, and the thread's composer is still the thread's.
    assert!(!rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("pane-") && k.contains("composer"))));
    // Opening another puts it after; ⌘⌥1 goes to Overview; closing a tab goes back to Overview.
    click(&mut rt, "waiting-link-1");
    assert_eq!(rt.state.tabs_of(&root).len(), 4);
    assert_eq!(rt.state.tab_of(&root), Tab::Kitten(kitten(1)), "the waiting line's link opens that tab");
    cmd_alt(&mut rt, '1');
    assert_eq!(rt.state.tab_of(&root), Tab::Overview);
    cmd_alt(&mut rt, '4');
    assert_eq!(rt.state.tab_of(&root), Tab::Kitten(kitten(1)), "⌘⌥4 is the fourth tab, in the order they were opened");
    rt.update(|s, _| s.close_tab(3));
    assert_eq!(rt.state.tab_of(&root), Tab::Overview, "closing the tab shown goes to Overview");
    rt.update(|s, _| s.close_tab(0));
    rt.update(|s, _| s.close_tab(1));
    assert_eq!(rt.state.tabs_of(&root).len(), 3, "Overview and Changes cannot be closed");
}

#[test]
fn overview_holds_what_needs_you_the_kittens_as_a_tree_the_changes_the_folder_what_it_waits_on_and_the_usage() {
    let rt = runtime((1280.0, 800.0), false, scene("pane-overview-kittens"));
    for key in ["pane-needs", "pane-kittens", "pane-changes", "pane-folder-section", "pane-waiting", "pane-usage"] {
        assert!(has_key(&rt, key), "{key} is in Overview");
    }
    // The tree: Kitten 1.1 steps in under Kitten 1, 12 points.
    let (one, nested, two) = (rect(&rt, "pane-kitten-1"), rect(&rt, "pane-kitten-1.1"), rect(&rt, "pane-kitten-2"));
    assert!((nested[0] - one[0] - 12.0).abs() < 1.0 && (two[0] - one[0]).abs() < 1.0, "{one:?} {nested:?} {two:?}");
    assert!(one[1] < nested[1] && nested[1] < two[1], "in number order");
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "Kitten 1 asks: Run a command"), "the kitten's request first: {t:?}");
    assert!(t.iter().any(|x| x == "docs/setup.md") && t.iter().any(|x| x.starts_with("~/acme")), "files and folder: {t:?}");
    // A request is answered from Overview itself.
    assert!(has_key(&rt, "pane-need-perm-perm_task_00000000000000000000000041-allow") || rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("pane-need-perm") && k.ends_with("-allow"))));
    // The sections of an empty thread are not shown: a quiet line instead, and no empty headings.
    let empty = runtime((1280.0, 800.0), false, scene("pane-overview-empty"));
    for key in ["pane-needs", "pane-kittens", "pane-changes", "pane-folder-section", "pane-waiting", "pane-usage"] {
        assert!(!has_key(&empty, key), "{key} has nothing to show");
    }
    assert!(has_key(&empty, "pane-overview-empty"));
    // Many files: five and Show all.
    let many = runtime((1280.0, 800.0), false, scene("pane-overview-many"));
    assert!((0..5).all(|i| has_key(&many, &format!("pane-file-{i}"))) && !has_key(&many, "pane-file-5") && has_key(&many, "pane-changes-all"));
}

#[test]
fn a_run_that_changed_files_ends_with_one_quiet_line_not_a_card() {
    let rt = runtime((1200.0, 800.0), false, scene("task-undo"));
    assert!(has_key(&rt, "run-run_1") && has_key(&rt, "run-run_1-facts") && has_key(&rt, "run-run_1-review") && has_key(&rt, "run-run_1-undo"), "Changed 4 files · +42 −7 · Review · Undo");
    for gone in ["run-run_1-files", "run-run_1-actions", "run-run_1-reveal", "run-run_1-copy-branch"] {
        assert!(!has_key(&rt, gone), "{gone}: the files, the branch and Reveal moved to Overview");
    }
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "Changed 4 files") && t.iter().any(|x| x == "+42"), "{t:?}");
    // Redo on an undone run, and the figures are quiet.
    let undone = runtime((1200.0, 800.0), false, scene("task-undone"));
    assert!(has_key(&undone, "run-run_1-redo") && !has_key(&undone, "run-run_1-undo"));
}

#[test]
fn review_opens_the_changes_tab_on_this_run_and_the_keys_reach_it() {
    let mut rt = runtime((1280.0, 800.0), false, scene("task-undo"));
    click(&mut rt, "run-run_1-review");
    arrive(&mut rt);
    let root = rt.state.pane_root().unwrap();
    assert_eq!(rt.state.tab_of(&root), Tab::Changes);
    assert_eq!(rt.state.review.as_ref().map(|r| (r.source, r.run.clone())), Some((neko_protocol::DiffSource::Run, Some(neko_protocol::RunId("run_1".into())))), "This run is selected");
    assert!(has_key(&rt, "changes-source") && has_key(&rt, "changes-tab") && has_key(&rt, "task-words"), "the conversation stays");
    // ⌘⇧G and ⌘D reach Changes from Overview.
    rt.update(|s, _| s.open_overview());
    arrive(&mut rt);
    press(&mut rt, Key::Command('g'), Mods { cmd: true, shift: true, ..Mods::default() });
    assert_eq!(rt.state.tab_of(&root), Tab::Changes);
    rt.update(|s, _| s.open_overview());
    press(&mut rt, Key::Command('d'), Mods { cmd: true, ..Mods::default() });
    assert_eq!(rt.state.tab_of(&root), Tab::Changes);
}

#[test]
fn no_two_elements_of_the_window_share_a_key_with_a_kitten_beside_its_thread() {
    for name in ["pane-kitten-live", "pane-kitten-done", "pane-overview-kittens"] {
        let rt = runtime((1280.0, 800.0), false, scene(name));
        let mut seen: HashSet<String> = HashSet::new();
        for e in rt.elements() {
            // A key may repeat only for the pieces of one control (its hit area, its parts); element keys that name content must not.
            if let Some(k) = e.key.filter(|k| k.starts_with("part-") || k.starts_with("pane-part-") || k.starts_with("task-") || k.starts_with("pane-task-") || k.starts_with("turn-") || k.starts_with("wake-")) {
                assert!(seen.insert(format!("{k}@{}", e.path)) && !e_dup(&rt, &k), "{name}: {k} is drawn twice");
            }
        }
    }
}

fn e_dup(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().filter(|e| e.key.as_deref() == Some(key) && e.opacity > 0.0).count() > 1
}

#[test]
fn the_pane_opens_by_itself_once_when_the_first_kitten_or_the_first_changes_come_and_never_over_a_close() {
    let mut rt = runtime((1200.0, 800.0), false, scene("task-done"));
    let root = rt.state.pane_root().unwrap();
    assert!(!has_key(&rt, "pane"));
    rt.update(move |s, _| s.pane_proactive(&root));
    settle(&mut rt);
    assert!(has_key(&rt, "pane"), "the first changes of the thread in view open the pane");
    // The person shuts it: it is not opened again for the same thread.
    cmd_alt(&mut rt, 'b');
    let root = rt.state.pane_root().unwrap();
    rt.update(move |s, _| s.pane_proactive(&root));
    settle(&mut rt);
    assert!(!has_key(&rt, "pane"), "once per thread");
    // A narrow window's sheet never opens unasked.
    let mut narrow = runtime((1000.0, 700.0), false, scene("task-done"));
    let root = narrow.state.pane_root().unwrap();
    narrow.update(move |s, _| s.pane_proactive(&root));
    settle(&mut narrow);
    assert!(!has_key(&narrow, "pane-sheet") && !narrow.state.pane.sheet);
}
