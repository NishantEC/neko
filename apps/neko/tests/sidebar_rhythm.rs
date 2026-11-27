//! The sidebar's one vertical rhythm (docs/knowledge/design.md, "Sidebar rhythm"), measured from the laid-out tree so it cannot drift:
//! one row height, one row gap, a header that sits closer to its own rows than to the section above, one value between groups (the
//! nav, the Cattains when a cat is at work, the workspaces).

mod support;
use neko_app::sidebar::{GROUP_GAP_STEPS, ROW_GAP_STEPS, SECTION_GAP_STEPS, header, row_1, row_2};
use support::*;

type Rects = std::collections::HashMap<String, [f32; 4]>;

/// The rect of every keyed element, by key (the leftmost, when a page reuses a sidebar row's key: Home's page is keyed "home" too).
fn rects(rt: &rune::Runtime<neko_app::state::App>) -> Rects {
    let mut out = Rects::new();
    for e in rt.elements() {
        if let Some(k) = e.key.clone() {
            let keep = out.get(&k).is_none_or(|old| e.rect[0] < old[0]);
            if keep {
                out.insert(k, e.rect);
            }
        }
    }
    out
}

/// A row's own key: the prefix and an id, not a part of the row (its portrait, its arc).
fn is_row(k: &str, prefix: &str) -> bool {
    k.strip_prefix(prefix).is_some_and(|rest| !rest.contains('-'))
}

fn top(r: &Rects, k: &str) -> f32 {
    r.get(k).unwrap_or_else(|| panic!("no element keyed {k}"))[1]
}

fn bottom(r: &Rects, k: &str) -> f32 {
    let e = r.get(k).unwrap_or_else(|| panic!("no element keyed {k}"));
    e[1] + e[3]
}

fn height(r: &Rects, k: &str) -> f32 {
    r.get(k).unwrap_or_else(|| panic!("no element keyed {k}"))[3]
}

fn near(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() < 0.51, "{what}: {a} is not {b}");
}

fn check(scene: &str, dark: bool) {
    rune_ui_style::fonts::install(neko_app::ui::FONTS);
    let t = neko_app::view::theme(dark, false);
    let (row_gap, section_gap, group_gap) = (t.space.at(ROW_GAP_STEPS), t.space.at(SECTION_GAP_STEPS), t.space.at(GROUP_GAP_STEPS));
    // The values themselves: a 36 point row, a 52 point two-line row, a 32 point header, gaps of 2, 12 and 20.
    assert_eq!((row_1(&t), row_2(&t), header(&t)), (36.0, 52.0, 32.0));
    assert_eq!((row_gap, section_gap, group_gap), (2.0, 12.0, 20.0));
    assert!(section_gap > row_gap && group_gap > section_gap, "a header sits closer to its rows than sections are to each other, and groups are further apart");
    let sc = scenes().iter().find(|s| s.0 == scene).unwrap_or_else(|| panic!("no scene {scene}")).1;
    let rt = runtime((1200.0, 800.0), dark, sc);
    let r = rects(&rt);
    let at = format!("{scene} {}", if dark { "dark" } else { "light" });

    // The nav: one row height, one gap.
    let nav = ["new-thread", "needs-you", "briefing", "rounds"];
    for k in nav {
        near(height(&r, k), row_1(&t), &format!("{at}: nav row {k} height"));
    }
    for w in nav.windows(2) {
        near(top(&r, w[1]) - bottom(&r, w[0]), row_gap, &format!("{at}: gap {} to {}", w[0], w[1]));
    }

    // The workspaces: each header a control high; its first row a row gap under it; rows a row gap apart; sections a section gap
    // apart; the first section a group gap under the nav; Add workspace… a row gap under the last row.
    let mut sections: Vec<(String, f32, f32)> = rt
        .elements()
        .into_iter()
        .filter_map(|e| e.key.as_deref().filter(|k| k.starts_with("ws-") && k.ends_with("-section")).map(|k| (k.trim_end_matches("-section").to_owned(), e.rect[1], e.rect[1] + e.rect[3])))
        .collect();
    sections.sort_by(|a, b| a.1.total_cmp(&b.1));
    assert!(!sections.is_empty(), "{at}: the sidebar shows projects");
    for (head, _, _) in &sections {
        near(height(&r, head), header(&t), &format!("{at}: header {head} height"));
    }
    // Threads: its header a control high, a group gap under the nav (or the Cattains), the first project a row gap under it.
    near(height(&r, "threads-head"), header(&t), &format!("{at}: Threads header height"));
    near(top(&r, "workspaces") - bottom(&r, "threads-head"), row_gap, &format!("{at}: the Threads header to its first group"));
    for w in sections.windows(2) {
        near(w[1].1 - w[0].2, section_gap, &format!("{at}: gap between sections {} and {}", w[0].0, w[1].0));
    }
    let mut task_rows: Vec<(String, f32, f32)> = r.iter().filter(|(k, _)| is_row(k, "thread-task_")).map(|(k, v)| (k.clone(), v[1], v[1] + v[3])).collect();
    task_rows.sort_by(|a, b| a.1.total_cmp(&b.1));
    assert!(!task_rows.is_empty(), "{at}: the sidebar shows threads");
    for (k, y, b) in &task_rows {
        near(b - y, row_2(&t), &format!("{at}: thread row {k} height"));
    }
    for (head, _, _) in &sections {
        // The first row under this header (if the section is open) is a row gap under it.
        if let Some((k, y, _)) = task_rows.iter().find(|(_, y, _)| *y > bottom(&r, head) - 0.5 && *y < bottom(&r, head) + 1.0 + row_gap) {
            near(y - bottom(&r, head), row_gap, &format!("{at}: {head} to its first row {k}"));
        }
    }
    for w in task_rows.windows(2) {
        // Rows in one section are a row gap apart; the next section's header lies between rows of different sections.
        let between = sections.iter().any(|(_, y, _)| *y >= w[0].2 - 0.5 && *y <= w[1].1);
        if !between {
            near(w[1].1 - w[0].2, row_gap, &format!("{at}: gap between thread rows {} and {}", w[0].0, w[1].0));
        }
    }
    let last = sections.last().expect("a section");
    near(top(&r, "add-project") - last.2, row_gap, &format!("{at}: Add project… under the last section"));
    near(height(&r, "add-project"), row_1(&t), &format!("{at}: Add project… height"));

    // The Cattains (only when a cat is at work): a group gap under the nav and over the workspaces, a header a control high, then the
    // cats and, under a cat with several tasks, its tasks: all rows of two lines, a row gap apart. Add workspace… is the last thing.
    let cattains = r.contains_key("cattains-section");
    if cattains {
        near(top(&r, "cattains-head") - bottom(&r, "rounds"), group_gap, &format!("{at}: nav to the Cattains"));
        near(top(&r, "threads-head") - bottom(&r, "cattains-section"), group_gap, &format!("{at}: Cattains to Threads"));
        near(height(&r, "cattains-head"), header(&t), &format!("{at}: Cattains header height"));
        let mut crew: Vec<(String, f32, f32)> = r.iter().filter(|(k, _)| is_row(k, "cat-cat_") || is_row(k, "cattain-thread-task_")).map(|(k, v)| (k.clone(), v[1], v[1] + v[3])).collect();
        crew.sort_by(|a, b| a.1.total_cmp(&b.1));
        assert!(!crew.is_empty(), "{at}: the Cattains list cats");
        near(crew[0].1 - bottom(&r, "cattains-head"), row_gap, &format!("{at}: the Cattains header to its first row"));
        for c in &crew {
            near(c.2 - c.1, row_2(&t), &format!("{at}: Cattains row {} height", c.0));
        }
        for w in crew.windows(2) {
            near(w[1].1 - w[0].2, row_gap, &format!("{at}: gap between Cattains rows {} and {}", w[0].0, w[1].0));
        }
        near(top(&r, "cattains-head") + header(&t) + row_gap, crew[0].1, &format!("{at}: the section is its header and its rows"));
    } else {
        near(top(&r, "threads-head") - bottom(&r, "rounds"), group_gap, &format!("{at}: nav to Threads"));
        assert!(!r.contains_key("cattains-head") && !r.contains_key("cattains"), "{at}: no cat at work, no section, no header");
    }
    assert!(!r.contains_key("new-cat") && !r.contains_key("cats-head"), "{at}: the sidebar has no New cat… and no Cats section");
}

#[test]
fn the_sidebar_keeps_one_rhythm() {
    for scene in ["sparse-home", "sparse-working", "sparse-needs-you", "sparse-two-tasks", "workspaces", "workspaces-folded", "home-house"] {
        for dark in [false, true] {
            check(scene, dark);
        }
    }
}

/// A section header is a label: no fill at rest, and not after a click either (the click gives it focus, not a ring or a wash). Its
/// hover and press washes are removed in `sidebar::section` (the headless element tree does not show a hover wash, so that part is
/// checked in a render with the pointer over it, not here).
#[test]
fn a_section_header_has_no_fill_at_rest() {
    let sc = scenes().iter().find(|s| s.0 == "sparse-home").expect("the scene").1;
    for dark in [false, true] {
        let mut rt = runtime((1200.0, 800.0), dark, sc);
        let fill = |rt: &rune::Runtime<neko_app::state::App>| rt.elements().into_iter().find(|e| e.key.as_deref() == Some("ws-ws_acme")).expect("the header").fill;
        assert_eq!(fill(&rt).a, 0.0, "at rest");
        click(&mut rt, "ws-ws_acme");
        assert_eq!(fill(&rt).a, 0.0, "after a click");
    }
}
