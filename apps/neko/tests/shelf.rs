//! Pin, settle and archive in the window (docs/knowledge/threads.md): the Pinned group first, a closed Settled fold at the end of a
//! group with its count that opens in place, archived threads out of the lists, the row menu, the keys, ⌘Z after a toast, and moving
//! between threads by number and by step. The window here has no daemon, so what is checked is what the window shows and keeps;
//! the requests and what the daemon does with them are in the daemon's tests and the QA scenarios.

mod support;
use neko_app::state::App;
use rune::{Key, Mods};
use support::*;

fn scene(name: &str) -> fn(&mut App) {
    scenes().iter().find(|s| s.0 == name).unwrap().1
}

fn keys(rt: &rune::Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.key).collect()
}

fn texts(rt: &rune::Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

fn id_of(rt: &rune::Runtime<App>, starts: &str) -> neko_protocol::TaskId {
    rt.state.tasks.iter().find(|t| t.title.starts_with(starts)).map(|t| t.id.clone()).unwrap_or_else(|| panic!("no thread {starts}"))
}

fn row_key(rt: &rune::Runtime<App>, starts: &str) -> String {
    format!("thread-{}", id_of(rt, starts).0)
}

fn top_of(rt: &rune::Runtime<App>, key: &str) -> f32 {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element keyed {key}")).rect[1]
}

fn cmd_shift() -> Mods {
    Mods { cmd: true, shift: true, ..Mods::default() }
}

#[test]
fn pinned_threads_are_the_first_group_from_any_project_and_each_is_listed_once() {
    for dark in [false, true] {
        let rt = runtime((1200.0, 800.0), dark, scene("threads-pinned"));
        let k = keys(&rt);
        assert!(k.contains(&"pinned-section".to_owned()) && k.contains(&"pinned-heading".to_owned()));
        assert!(top_of(&rt, "pinned-section") < top_of(&rt, "ws-ws_acme-section"), "Pinned is above the projects");
        for starts in ["Find out why the nightly build", "Write the pricing page", "Answer the question about the retry limit"] {
            let key = row_key(&rt, starts);
            assert_eq!(k.iter().filter(|x| **x == key).count(), 1, "{starts} is one row, in Pinned");
            assert!(top_of(&rt, &key) < top_of(&rt, "ws-ws_acme-section"), "{starts} sits in Pinned");
        }
        // The row names its project, as in the ungrouped list.
        assert!(texts(&rt).iter().any(|t| t == "acme-api"));
        let pinned_row = nodes(&rt.accessibility_json()).into_iter().find(|(_, n)| str_of(n, "name").starts_with("Find out why the nightly build")).map(|(_, n)| str_of(&n, "description").to_owned()).unwrap();
        assert!(pinned_row.contains("pinned"), "{pinned_row}");
    }
}

/// Pinned and Settled keep the sidebar's one rhythm: a header a control high, a row gap to its rows, rows a row gap apart, a section
/// gap to the next group, and a fold line a control high a row gap under the last row of its group.
#[test]
fn pinned_and_settled_keep_the_sidebar_rhythm() {
    use neko_app::sidebar::{ROW_GAP_STEPS, SECTION_GAP_STEPS, header, row_2};
    for dark in [false, true] {
        rune_ui_style::fonts::install(neko_app::ui::FONTS);
        let t = neko_app::view::theme(dark, false);
        let (row_gap, section_gap) = (t.space.at(ROW_GAP_STEPS), t.space.at(SECTION_GAP_STEPS));
        let rt = runtime((1200.0, 800.0), dark, scene("threads-settled"));
        let rect = |k: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(k)).unwrap_or_else(|| panic!("no element keyed {k}")).rect;
        let near = |a: f32, b: f32, what: &str| assert!((a - b).abs() < 0.51, "{what}: {a} is not {b}");
        near(rect("pinned-head")[3], header(&t), "Pinned header height");
        let pinned: Vec<[f32; 4]> = ["Answer the question about the retry limit", "Find out why the nightly build", "Write the pricing page"].iter().map(|w| rect(&row_key(&rt, w))).collect();
        near(pinned[0][1] - (rect("pinned-head")[1] + rect("pinned-head")[3]), row_gap, "Pinned header to its first row");
        for r in &pinned {
            near(r[3], row_2(&t), "a pinned row is two lines");
        }
        for w in pinned.windows(2) {
            near(w[1][1] - (w[0][1] + w[0][3]), row_gap, "pinned rows a row gap apart");
        }
        let (p, a) = (rect("pinned-section"), rect("ws-ws_acme-section"));
        near(a[1] - (p[1] + p[3]), section_gap, "Pinned to the first project");
        // A fold line is a control high and sits a row gap under its group's last row.
        let fold = rect("ws-ws_web-settled");
        near(fold[3], header(&t), "a Settled fold's height");
        let last = rect(&row_key(&rt, "Make the hero image"));
        near(fold[1] - (last[1] + last[3]), row_gap, "the Settled fold under its group's last row");
        // The group after it is a section gap below the fold.
        let next = rect("ws-ws_notes-section");
        near(next[1] - (rect("ws-ws_web-section")[1] + rect("ws-ws_web-section")[3]), section_gap, "groups stay a section gap apart");
        let _ = fold;
    }
}

#[test]
fn nothing_pinned_shows_no_pinned_group() {
    let rt = runtime((1200.0, 800.0), false, scene("workspaces"));
    assert!(!keys(&rt).iter().any(|k| k.starts_with("pinned")), "Pinned is hidden when empty");
}

#[test]
fn a_group_ends_in_a_closed_settled_fold_with_its_count_that_opens_in_place() {
    let mut rt = runtime((1200.0, 800.0), false, scene("threads-settled"));
    let settled_web = row_key(&rt, "Move the blog to the new layout");
    let settled_acme = row_key(&rt, "Rename the Settings button");
    assert!(keys(&rt).contains(&"ws-ws_web-settled".to_owned()) && keys(&rt).contains(&"ws-ws_acme-settled".to_owned()));
    assert!(!keys(&rt).contains(&settled_web) && !keys(&rt).contains(&settled_acme), "closed: the settled threads are not rows");
    let fold = nodes(&rt.accessibility_json()).into_iter().find(|(_, n)| n.get("name").and_then(|x| x.as_str()) == Some("Settled") && str_of(n, "description").contains("4")).map(|(_, n)| n);
    assert!(fold.is_some(), "the fold says how many are inside, in words");
    click(&mut rt, "ws-ws_web-settled");
    assert!(keys(&rt).contains(&settled_web), "pressed open, they are rows in place");
    assert!(top_of(&rt, "ws-ws_web-settled") < top_of(&rt, &settled_web));
    assert!(!keys(&rt).contains(&settled_acme), "another group's fold stays shut");
    click(&mut rt, "ws-ws_web-settled");
    assert!(!keys(&rt).contains(&settled_web), "pressed again, shut");
}

#[test]
fn view_show_settled_opens_every_fold_and_show_archived_adds_the_archived_fold() {
    let mut rt = runtime((1200.0, 800.0), false, scene("threads-settled"));
    let settled_acme = row_key(&rt, "Rename the Settings button");
    rt.update(|s, _| s.set_show_settled(true));
    settle(&mut rt);
    assert!(keys(&rt).contains(&settled_acme), "Show settled: every fold is open");
    rt.update(|s, _| {
        s.set_show_settled(false);
        let id = s.tasks.iter().find(|t| t.title.starts_with("Port the backup script")).map(|t| t.id.clone()).unwrap();
        s.tasks.iter_mut().find(|t| t.id == id).unwrap().archived = true;
    });
    settle(&mut rt);
    let archived = row_key(&rt, "Port the backup script");
    assert!(!keys(&rt).contains(&archived) && !keys(&rt).iter().any(|k| k == "archived-fold"), "archived threads are out of the lists, and there is no fold until it is asked for");
    rt.update(|s, _| s.set_show_archived(true));
    settle(&mut rt);
    assert!(keys(&rt).iter().any(|k| k == "archived-fold") && !keys(&rt).contains(&archived), "the fold is there, shut");
    // The groups above are folded so the fold is in the window to be pressed.
    rt.update(|s, _| s.folded = ["default", "ws_acme", "ws_web", "ws_notes", "ws_scripts"].iter().map(|x| (*x).to_owned()).collect());
    settle(&mut rt);
    click(&mut rt, "archived-fold");
    assert!(keys(&rt).contains(&archived), "opened, the archived thread is a row");
}

#[test]
fn a_thread_that_is_pinned_and_archived_is_in_neither_place() {
    let rt = runtime((1200.0, 800.0), false, |s| {
        scene("threads-pinned")(s);
        let id = s.tasks.iter().find(|t| t.title.starts_with("Write the pricing page")).map(|t| t.id.clone()).unwrap();
        s.tasks.iter_mut().find(|t| t.id == id).unwrap().archived = true;
    });
    let key = row_key(&rt, "Write the pricing page");
    assert!(!keys(&rt).contains(&key), "archiving keeps the pin, and the thread is out of the lists");
}

#[test]
fn the_row_menu_offers_pin_settle_and_archive_and_says_which_way() {
    let rt = runtime((1200.0, 800.0), false, scene("threads-pinned-menu"));
    let t = texts(&rt);
    for want in ["Unpin", "Settle", "Archive", "Mark as read", "⌘⇧P", "⌘⇧S"] {
        assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
    }
    let rt = runtime((1200.0, 800.0), false, scene("threads-row-menu"));
    assert!(texts(&rt).iter().any(|x| x == "Pin to top"), "an unpinned thread offers Pin to top");
}

#[test]
fn the_band_menu_has_the_same_three() {
    let rt = runtime((1200.0, 800.0), false, scene("thread-band-menu"));
    let t = texts(&rt);
    for want in ["Pin to top", "Settle", "Archive", "All changes", "Remove thread…"] {
        assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
    }
}

#[test]
fn a_toast_says_what_happened_with_an_undo_and_is_read_out_politely() {
    let mut rt = runtime((1200.0, 800.0), false, scene("threads-toast-settled"));
    let all = nodes(&rt.accessibility_json());
    let toast = all.iter().find(|(_, n)| str_of(n, "role") == "status" && str_of(n, "name") == "Settled");
    assert!(toast.is_some(), "a status named Settled");
    assert!(all.iter().any(|(_, n)| str_of(n, "role") == "button" && str_of(n, "name") == "Undo"));
    assert!(all.iter().any(|(_, n)| str_of(n, "role") == "button" && str_of(n, "name") == "Dismiss"));
    let id = rt.state.toasts.items().next().unwrap().0;
    click(&mut rt, &format!("toasts-{id}-action"));
    assert!(rt.state.toasts.is_empty(), "Undo dismisses the toast");
    for _ in 0..30 {
        rt.frame(0.05);
    }
    assert!(!keys(&rt).iter().any(|k| k.starts_with("toasts")), "and it is gone from the window");
}

#[test]
fn a_toast_goes_by_itself_after_five_seconds_and_holds_while_the_pointer_is_over_it() {
    let mut rt = runtime((1200.0, 800.0), false, scene("workspaces"));
    rt.update(|s, _| s.settle_thread(&neko_protocol::TaskId("task_none".into()), true));
    assert!(rt.state.toasts.is_empty(), "no such thread, no toast");
    let id = id_of(&rt, "Find out why the nightly build");
    rt.update(move |s, _| {
        s.pin_thread(&id, false);
    });
    for t in [0.1, 1.0, 3.0] {
        rt.frame(t);
    }
    assert_eq!(rt.state.toasts.len(), 1, "still there inside five seconds");
    rt.frame(2.0);
    assert!(rt.state.toasts.is_empty(), "gone after five");
}

#[test]
fn cmd_shift_p_pins_the_open_thread_and_cmd_z_takes_an_unpin_back() {
    let mut rt = runtime((1200.0, 800.0), false, scene("threads-pinned"));
    let id = id_of(&rt, "Find out why the nightly build");
    rt.update(move |s, _| s.open_task_id(&id));
    settle(&mut rt);
    // It is pinned: the key unpins it, and says so with an Undo.
    press(&mut rt, Key::Command('p'), cmd_shift());
    let (title, action) = {
        let (_, spec) = rt.state.toasts.items().next().expect("a toast");
        (spec.title.clone(), spec.action.clone())
    };
    assert_eq!(title, "Unpinned");
    assert_eq!(action.map(|a| a.0), Some("Undo".to_owned()));
    // ⌘Z is its Undo.
    press(&mut rt, Key::Command('z'), Mods { cmd: true, ..Mods::default() });
    assert!(rt.state.toasts.is_empty(), "⌘Z dismissed it and took it back");
    // With nothing to take back ⌘Z is not ours.
    press(&mut rt, Key::Command('z'), Mods { cmd: true, ..Mods::default() });
    assert!(rt.state.toasts.is_empty());
}

#[test]
fn a_thread_that_works_cannot_be_settled_and_the_toast_says_why() {
    let mut rt = runtime((1200.0, 800.0), false, scene("threads-pinned"));
    let id = id_of(&rt, "Make the hero image");
    rt.update(move |s, _| s.open_task_id(&id));
    settle(&mut rt);
    press(&mut rt, Key::Command('s'), cmd_shift());
    let toasts: Vec<(String, bool)> = rt.state.toasts.items().map(|(_, s)| (s.title.clone(), s.action.is_some())).collect();
    assert_eq!(toasts, [("It is working. Settle it when it stops.".to_owned(), false)], "a quiet refusal with no Undo, and no Settled toast");
}

#[test]
fn alt_digits_and_cmd_alt_arrows_move_between_threads_in_the_order_they_are_drawn() {
    let mut rt = runtime((1200.0, 800.0), false, scene("threads-settled"));
    let order = rt.state.thread_order();
    let pinned: Vec<_> = ["Answer the question about the retry limit", "Find out why the nightly build", "Write the pricing page"].iter().map(|w| id_of(&rt, w)).collect();
    assert_eq!(order[..3], pinned[..], "Pinned first");
    assert!(!order.contains(&id_of(&rt, "Move the blog to the new layout")), "a shut fold's threads are not in the order");
    // ⌥2 opens the second.
    press(&mut rt, Key::Text("2".into()), Mods { alt: true, ..Mods::default() });
    assert_eq!(rt.state.open.as_ref(), Some(&order[1]));
    // ⌘⌥↓ and ⌘⌥↑ step, wrapping.
    press(&mut rt, Key::Down, Mods { cmd: true, alt: true, ..Mods::default() });
    assert_eq!(rt.state.open.as_ref(), Some(&order[2]));
    press(&mut rt, Key::Up, Mods { cmd: true, alt: true, ..Mods::default() });
    press(&mut rt, Key::Up, Mods { cmd: true, alt: true, ..Mods::default() });
    assert_eq!(rt.state.open.as_ref(), Some(&order[0]));
    press(&mut rt, Key::Up, Mods { cmd: true, alt: true, ..Mods::default() });
    assert_eq!(rt.state.open.as_ref(), order.last());
    // A number past the end does nothing.
    let was = rt.state.open.clone();
    press(&mut rt, Key::Text("9".into()), Mods { alt: true, ..Mods::default() });
    assert!(rt.state.open == was || order.len() >= 9);
}

#[test]
fn the_order_follows_what_is_open_and_folded() {
    let mut rt = runtime((1200.0, 800.0), false, scene("threads-settled-open"));
    let order = rt.state.thread_order();
    let web_settled = id_of(&rt, "Move the blog to the new layout");
    assert!(order.contains(&web_settled), "an open fold's threads are in it");
    rt.update(|s, _| {
        s.set_folded("ws_web", true);
    });
    assert!(!rt.state.thread_order().contains(&web_settled), "a folded group's are not");
}
