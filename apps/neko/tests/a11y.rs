//! The accessibility tree of every scene, held to rules: controls have names, list rows say where they are and whether they are
//! chosen, fields are labelled, each page has one main heading, and the landmarks are there.

mod support;
use support::*;

#[test]
fn every_scene_reads_well_to_assistive_technology() {
    for &(name, scene) in scenes() {
        for size in SIZES {
            let rt = runtime(size, false, scene);
            let at = format!("{name} at {}x{}", size.0, size.1);
            let all = nodes(&rt.accessibility_json());
            let children = |i: usize| all.iter().enumerate().filter(move |(_, (p, _))| *p == Some(i)).map(|(j, _)| j);
            for (i, (parent, n)) in all.iter().enumerate() {
                let role = str_of(n, "role");
                let named = !str_of(n, "name").trim().is_empty();
                // Anything the keyboard can reach has a name.
                if has(n, "actions", "focus") {
                    assert!(named, "{at}: a focusable {role} has no name: {n}");
                }
                // Anything pressable has a name too (rune-ui 0.9 hides its 40 point hit areas: no unnamed buttons round a control).
                if has(n, "actions", "click") {
                    assert!(named, "{at}: an unnamed pressable {role}: {n}");
                }
                if role.contains("text-input") || role == "text-field" {
                    assert!(named, "{at}: a field without a label: {n}");
                }
                if role == "list-box-option" {
                    let list = &all[parent.expect("an option has a list")].1;
                    let size = list.get("setSize").and_then(|v| v.as_u64()).unwrap_or(0);
                    let pos = n.get("position").and_then(|v| v.as_u64()).unwrap_or(0);
                    assert!(pos >= 1 && pos <= size, "{at}: a row without its place ({pos} of {size}): {n}");
                    // The palette's rows (rune-ui's Command) are not focusable: the field names the active one (`active_descendant`) and only
                    // that row says it is selected, so the others have no state of their own. An upstream ask (`rune-ui-in-neko.md`);
                    // below, the palette is held to exactly one selected row whenever it has rows.
                    let mut up = *parent;
                    let mut in_palette = false;
                    while let Some(p) = up {
                        in_palette |= str_of(&all[p].1, "key") == "palette";
                        up = all[p].0;
                    }
                    assert!(in_palette || has(n, "states", "selected") || has(n, "states", "unselected"), "{at}: a row that does not say whether it is chosen: {n}");
                }
                if role == "list-box" {
                    let chosen = children(i).filter(|&c| has(&all[c].1, "states", "selected")).count();
                    assert!(chosen <= 1, "{at}: {chosen} rows chosen in one list");
                    assert!(named, "{at}: a list without a name");
                }
            }
            if let Some(p) = all.iter().position(|(_, n)| str_of(n, "key") == "palette") {
                let in_it = |mut i: usize| {
                    while let Some(up) = all[i].0 {
                        if up == p {
                            return true;
                        }
                        i = up;
                    }
                    false
                };
                let rows: Vec<usize> = (0..all.len()).filter(|&i| str_of(&all[i].1, "role") == "list-box-option" && in_it(i)).collect();
                let chosen = rows.iter().filter(|&&i| has(&all[i].1, "states", "selected")).count();
                assert_eq!(chosen, usize::from(!rows.is_empty()), "{at}: the palette has one chosen row when it has rows");
            }
            // A modal panel (the context ring's) hides the page from assistive technology by design: it is named, and the page's
            // own rules hold again once it closes.
            if let Some((_, m)) = all.iter().find(|(_, n)| has(n, "states", "modal")) {
                assert!(!str_of(m, "name").trim().is_empty(), "{at}: a modal panel without a name: {m}");
                continue;
            }
            let h1 = all.iter().filter(|(_, n)| str_of(n, "role") == "heading" && n.get("level").and_then(|v| v.as_u64()) == Some(1)).count();
            assert_eq!(h1, 1, "{at}: one main heading");
            assert!(all.iter().any(|(_, n)| str_of(n, "role") == "main"), "{at}: a main landmark");
            if size.0 >= 1000.0 {
                assert!(all.iter().any(|(_, n)| str_of(n, "role") == "navigation"), "{at}: the sidebar is a navigation landmark");
            }
            // The open task is the chosen row.
            if let Some(open) = &rt.state.open {
                // A subagent has no row of its own: its thread's row is the chosen one.
                let id = rt.state.tasks.iter().find(|t| &t.id == open).and_then(|t| t.subagent.as_ref().map(|s| s.root.clone())).unwrap_or_else(|| open.clone());
                // A Cattain's own thread is not a row under Threads: its Cattain's row is the chosen one, while it is at work and listed.
                let own = rt.state.cattain_of(&id).map(|c| (c.id.clone(), rt.state.cat_state(&c.id) != neko_app::state::CatState::Napping));
                let want = own.as_ref().map_or_else(|| format!("thread-{}", id.0), |(c, _)| format!("cat-{c}"));
                let row = all.iter().find(|(_, n)| str_of(n, "key") == want).map(|(_, n)| n.clone());
                if size.0 >= 1000.0 && own.as_ref().is_none_or(|(_, listed)| *listed) {
                    assert!(row.is_some_and(|r| has(&r, "states", "selected")), "{at}: the open thread's row is selected");
                }
            }
        }
    }
}
