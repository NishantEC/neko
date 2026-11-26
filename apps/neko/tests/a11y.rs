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
                // A pressable thing without one is a hit area around a named control (rune-ui's 40 point targets).
                if has(n, "actions", "click") && !has(n, "actions", "focus") && !named {
                    assert!(children(i).any(|c| !str_of(&all[c].1, "name").is_empty()), "{at}: an unnamed {role} holds no named control: {n}");
                }
                if role.contains("text-input") || role == "text-field" {
                    assert!(named, "{at}: a field without a label: {n}");
                }
                if role == "list-box-option" {
                    let list = &all[parent.expect("an option has a list")].1;
                    let size = list.get("setSize").and_then(|v| v.as_u64()).unwrap_or(0);
                    let pos = n.get("position").and_then(|v| v.as_u64()).unwrap_or(0);
                    assert!(pos >= 1 && pos <= size, "{at}: a row without its place ({pos} of {size}): {n}");
                    assert!(has(n, "states", "selected") || has(n, "states", "unselected"), "{at}: a row that does not say whether it is chosen: {n}");
                }
                if role == "list-box" {
                    let chosen = children(i).filter(|&c| has(&all[c].1, "states", "selected")).count();
                    assert!(chosen <= 1, "{at}: {chosen} rows chosen in one list");
                    assert!(named, "{at}: a list without a name");
                }
            }
            let h1 = all.iter().filter(|(_, n)| str_of(n, "role") == "heading" && n.get("level").and_then(|v| v.as_u64()) == Some(1)).count();
            assert_eq!(h1, 1, "{at}: one main heading");
            assert!(all.iter().any(|(_, n)| str_of(n, "role") == "main"), "{at}: a main landmark");
            if size.0 >= 1000.0 {
                assert!(all.iter().any(|(_, n)| str_of(n, "role") == "navigation"), "{at}: the sidebar is a navigation landmark");
            }
            // The open task is the chosen row.
            if let Some(open) = &rt.state.open {
                let row = all.iter().find(|(_, n)| str_of(n, "key") == format!("task-{}", open.0)).map(|(_, n)| n.clone());
                if size.0 >= 1000.0 {
                    assert!(row.is_some_and(|r| has(&r, "states", "selected")), "{at}: the open task's row is selected");
                }
            }
        }
    }
}
