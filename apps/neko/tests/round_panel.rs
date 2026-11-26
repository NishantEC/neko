//! The New round panel's pop-up pickers: they open above the panel and a press on a choice picks it.

mod support;
use support::*;

fn scene(name: &str) -> fn(&mut neko_app::state::App) {
    scenes().iter().find(|s| s.0 == name).unwrap_or_else(|| panic!("no scene {name}")).1
}

#[test]
fn the_cat_and_workspace_pickers_open_in_the_panel_and_choose() {
    let mut rt = runtime((1200.0, 800.0), false, scene("round-new"));
    click(&mut rt, "round-cat");
    assert_eq!(rt.state.menu_open.as_deref(), Some("round-cat-menu"), "the chip opens its menu");
    settle(&mut rt);
    let items: Vec<String> = rt.elements().iter().filter(|e| e.role == "menuitemcheckbox").filter_map(|e| e.label.clone()).collect();
    assert!(items.iter().any(|l| l == "miso") && items.iter().any(|l| l == "tofu"), "the menu lists the cats above the panel: {items:?}");
    let key = rt.elements().iter().find(|e| e.role == "menuitemcheckbox" && e.label.as_deref() == Some("tofu")).and_then(|e| e.key.clone()).unwrap();
    for _ in 0..20 {
        rt.frame(0.05);
    }
    click(&mut rt, &key);
    assert_eq!(rt.state.round_form.as_ref().map(|f| f.cat.as_str()), Some("cat_tofu"), "a press on a cat picks it");
    click(&mut rt, "round-workspace");
    assert_eq!(rt.state.menu_open.as_deref(), Some("round-workspace-menu"));
}

#[test]
fn a_press_on_history_opens_it_in_the_card() {
    let mut rt = runtime((1200.0, 800.0), false, scene("rounds"));
    let id = rt.state.rounds[0].id.clone();
    click(&mut rt, &format!("round-{id}-history-trigger"));
    assert!(rt.state.round_history_open.contains(&id), "the press opens the history");
    assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some(&format!("round-{id}-history-list"))), "and its list is on the card");
}
