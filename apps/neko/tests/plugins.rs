//! Plugins in the app (docs/knowledge/plugins.md): Settings ▸ Plugins and its Add sheet, the approval card in Needs you, a cat's page and
//! a plugin step, on scenes (structure, reading order, what is never shown, notifications); and the whole flow against a real daemon
//! with the fake CLI and the fake MCP server: paste, approve in Needs you, test, a secret, who may use it, switch off, remove.
//!
//! The daemon test needs `NEKO_DAEMON_BIN` (scripts/check.sh builds it and the fake servers beside it). It gives the daemon a file
//! for secrets (`NEKO_SECRETS_FILE`, debug builds only), so nothing here touches the login keychain. Every daemon started here is
//! stopped by the PID it wrote, pass or fail.

mod support;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use neko_app::notify::{ClickDo, Notifier, Recorder, Sent, note_id, read_click};
use neko_app::state::{App, Daemon, need_key};
use neko_protocol::plugins::{Approval, PluginState};
use neko_protocol::Need;
use rune::{Key, Mods, Runtime};
use support::*;

const ID: &str = "plugin_00000000000000000000000001";
const SECRET: &str = "sk-live-9f3a-not-for-display";

fn scene(name: &str) -> fn(&mut App) {
    scenes().iter().find(|s| s.0 == name).unwrap_or_else(|| panic!("no scene {name}")).1
}

fn focus(rt: &Runtime<App>) -> Option<String> {
    rt.presented_focus_key().map(str::to_owned)
}

fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn el(rt: &Runtime<App>, key: &str) -> rune::ElementInfo {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key} is not on screen"))
}

/// Everything a person could read: each node's name and description, and every text drawn.
fn names(rt: &Runtime<App>) -> String {
    let mut all: Vec<String> = nodes(&rt.accessibility_json()).iter().flat_map(|(_, n)| [str_of(n, "name").to_owned(), str_of(n, "description").to_owned()]).collect();
    all.extend(rt.elements().into_iter().filter_map(|e| e.text));
    all.join("\n")
}

fn disabled(rt: &Runtime<App>, key: &str) -> bool {
    nodes(&rt.accessibility_json()).into_iter().map(|(_, n)| n).find(|n| str_of(n, "key") == key).unwrap_or_else(|| panic!("{key} is not in the tree")).get("states").and_then(|s| s.as_array()).is_some_and(|s| s.iter().any(|x| x == "disabled"))
}

/// A button in a panel: the panel is let come in first (frames with time passing), then pressed.
fn click_in_panel(rt: &mut Runtime<App>, key: &str) {
    for _ in 0..4 {
        rt.frame(0.3);
    }
    click(rt, key);
}

fn type_text(rt: &mut Runtime<App>, words: &str) {
    press(rt, Key::Text(words.into()), Mods::default());
}

// ---- Settings ▸ Plugins ------------------------------------------------------------------------------------------------------------

#[test]
fn settings_has_a_plugins_page_and_its_empty_state_says_none_come_with_neko() {
    let rt = runtime((1200.0, 800.0), false, scene("plugins-empty"));
    for key in ["plugins-empty", "plugins-add-json", "plugins-add-folder"] {
        assert!(has_key(&rt, key), "{key}");
    }
    let all = names(&rt);
    assert!(all.contains("No plugins yet") && all.contains("None come with Neko"), "{all}");
    // The same two buttons, by the same names, once there are plugins.
    let rt = runtime((760.0, 560.0), false, scene("plugins"));
    assert!(has_key(&rt, "plugins-add-json") && has_key(&rt, "plugins-add-folder") && !has_key(&rt, "plugins-empty"));
    let nodes = nodes(&rt.accessibility_json());
    let btn = |name: &str| nodes.iter().any(|(_, n)| str_of(n, "role") == "button" && str_of(n, "name") == name);
    assert!(btn("Add MCP server…") && btn("Add plugin folder…"), "the agreed labels");
}

#[test]
fn each_state_says_itself_in_words_and_offers_what_fits() {
    let rt = runtime((1200.0, 800.0), false, scene("plugins"));
    let all = names(&rt);
    for words in ["On", "Off", "Waiting for your approval"] {
        assert!(all.lines().any(|l| l == words), "{words}: {all}");
    }
    // The one that waits shows Approve and Decline and no switch; an approved one has a switch and no Approve.
    let waiting = "plugin_00000000000000000000000003";
    assert!(has_key(&rt, &format!("plugin-{waiting}-approve")) && has_key(&rt, &format!("plugin-{waiting}-decline")));
    assert!(!has_key(&rt, &format!("plugin-{waiting}-switch")), "nothing to switch before it is approved");
    assert!(has_key(&rt, &format!("plugin-{ID}-switch")) && !has_key(&rt, &format!("plugin-{ID}-approve")));
    // A web server cannot be tested yet, and says why; an unapproved one says to approve it first.
    let tall = runtime((1200.0, 2400.0), false, scene("plugins"));
    assert!(names(&tall).contains("Approve it to test it."), "an unapproved plugin says why it cannot be tested");
    let rt = runtime((1200.0, 800.0), false, scene("plugins-web"));
    assert!(names(&rt).contains("Neko cannot test web servers yet. Claude Code connects to it when a thread runs."));
    assert!(disabled(&rt, &format!("plugin-{ID}-test")), "Test is off for a web server");
    // Changed and declined cards say what differs, and a missing setting is named.
    let rt = runtime((1200.0, 800.0), false, scene("plugins-changed"));
    let all = names(&rt);
    assert!(all.contains("Changed since you approved it") && all.contains("Declined") && all.contains("Now also passes ACME_REGION"), "{all}");
    let rt = runtime((1200.0, 800.0), false, scene("plugins-needs-setup"));
    let all = names(&rt);
    assert!(all.contains("Needs setting up") && all.contains("Can't be read") && all.contains("Still to set: API key"), "{all}");
}

#[test]
fn a_check_shows_testing_then_the_tools_or_the_words_of_why_and_when() {
    let rt = runtime((1200.0, 800.0), false, scene("plugins-check-ok"));
    let all = names(&rt);
    assert!(all.contains("Connected: 6 tools") && all.contains("Tested today at 14:02"), "{all}");
    assert!(el(&rt, &format!("plugin-{ID}-tools")).text.unwrap_or_default().contains("search, fetch_page"));
    let rt = runtime((1200.0, 800.0), false, scene("plugins-check-failed"));
    assert!(names(&rt).contains("The test failed: The server did not answer in 10 seconds."));
    let rt = runtime((1200.0, 800.0), false, scene("plugins-testing"));
    assert!(has_key(&rt, &format!("plugin-{ID}-testing")));
    assert!(disabled(&rt, &format!("plugin-{ID}-test")), "no second test while one runs");
}

#[test]
fn tab_goes_through_a_plugins_card_in_reading_order() {
    let mut rt = runtime((1200.0, 800.0), false, scene("plugins-check-ok"));
    let mut seen = Vec::new();
    for _ in 0..40 {
        tab(&mut rt, false);
        let Some(k) = focus(&rt) else { continue };
        if seen.first() == Some(&k) {
            break;
        }
        seen.push(k);
    }
    let at = |k: &str| seen.iter().position(|s| s == k).unwrap_or_else(|| panic!("{k} is not a stop: {seen:?}"));
    let ours: Vec<&String> = seen.iter().filter(|k| k.starts_with("plugin")).collect();
    assert_eq!(
        ours,
        [
            "plugins-add-json",
            "plugins-add-folder",
            &format!("plugin-{ID}-switch"),
            &format!("plugin-{ID}-acme_token-change"),
            &format!("plugin-{ID}-acme_token-remove"),
            &format!("plugin-{ID}-access-0"),
            &format!("plugin-{ID}-test"),
            &format!("plugin-{ID}-remove"),
        ],
        "the page's buttons, then the card from its switch to its Remove: {seen:?}"
    );
    assert!(at("plugins-add-json") < at(&format!("plugin-{ID}-switch")));
}

#[test]
fn a_secret_is_masked_set_once_and_never_drawn_named_or_kept() {
    let mut rt = runtime((1200.0, 800.0), false, scene("plugins-fields-unset"));
    let key = format!("plugin-{ID}-api_key");
    click(&mut rt, &format!("{key}-input"));
    type_text(&mut rt, SECRET);
    assert!(rt.state.plugin_ui.secrets.values().any(|e| e.text == SECRET), "it is held only while typed");
    // Not in any name, description or text on screen; the masked field's own value is hidden from assistive technology too.
    let tree = rt.accessibility_json().to_string();
    assert!(!tree.contains(SECRET), "the secret is in the accessibility tree");
    assert!(rt.elements().iter().all(|e| !e.text.clone().unwrap_or_default().contains(SECRET)), "the secret is drawn as text");
    assert!(!disabled(&rt, &format!("{key}-set")), "Set is on once there is something to set");
    click(&mut rt, &format!("{key}-set"));
    assert!(rt.state.plugin_ui.secrets.values().all(|e| e.text.is_empty()), "sent once and dropped at once (here there is no daemon to send to)");
    assert!(!rt.accessibility_json().to_string().contains(SECRET));
    // A secret that is set shows only that: Set, with Change and Remove, and no field.
    let rt = runtime((1200.0, 800.0), false, scene("plugins-fields-set"));
    assert!(has_key(&rt, &format!("{key}-status")) && has_key(&rt, &format!("{key}-change")) && has_key(&rt, &format!("{key}-remove")));
    assert!(!has_key(&rt, &format!("{key}-input")), "no field to show a value in");
    assert!(names(&rt).contains("API key is set"));
    // Change opens an empty masked field.
    let mut rt = rt;
    click(&mut rt, &format!("{key}-change"));
    assert!(has_key(&rt, &format!("{key}-input")) && has_key(&rt, &format!("{key}-cancel")));
    assert_eq!(rt.state.plugin_ui.secrets.get(&(ID.to_owned(), "api_key".to_owned())).map(|e| e.text.as_str()), Some(""));
}

#[test]
fn which_cats_never_sends_an_empty_choice() {
    let mut rt = runtime((1200.0, 800.0), false, scene("plugins-check-ok"));
    assert!(!has_key(&rt, &format!("plugin-{ID}-cat-cat_miso")), "every cat: no list");
    click(&mut rt, &format!("plugin-{ID}-access-1"));
    assert!(rt.state.plugin_ui.only.contains(ID), "Only some cats is chosen, and nothing is sent: the daemon would refuse an empty list");
    assert!(has_key(&rt, &format!("plugin-{ID}-cat-cat_miso")) && has_key(&rt, &format!("plugin-{ID}-cats-hint")));
    assert_eq!(rt.state.plugins[0].access.mode, neko_protocol::AccessMode::All, "still every cat until one is chosen");
    click_in_panel(&mut rt, &format!("plugin-{ID}-cat-cat_miso"));
    assert!(!rt.state.plugin_ui.only.contains(ID), "choosing a cat sends the list (here there is no daemon), and the draft is over");
    // Only mochi: its switch cannot be turned off (the last one), and says why.
    let rt = runtime((1200.0, 800.0), false, scene("plugins-access-only"));
    assert!(disabled(&rt, &format!("plugin-{ID}-cat-cat_mochi")), "the last cat cannot be turned off");
    assert!(names(&rt).contains("At least one cat has to be able to use it"));
}

#[test]
fn removing_asks_first_and_says_the_secrets_go_too() {
    let mut rt = runtime((1200.0, 800.0), false, scene("plugins"));
    click(&mut rt, &format!("plugin-{ID}-remove"));
    assert_eq!(rt.state.confirm, Some(neko_app::state::Confirm::RemovePlugin(ID.to_owned())));
    assert!(names(&rt).contains("Remove acme-docs?") && names(&rt).contains("secrets go, the secrets from the Keychain too"), "{}", names(&rt));
    click_in_panel(&mut rt, "remove-plugin-cancel");
    assert!(rt.state.confirm.is_none());
}

// ---- The Add sheet ------------------------------------------------------------------------------------------------------------------

#[test]
fn the_add_sheet_reads_a_paste_as_it_is_typed_and_never_echoes_a_value() {
    let mut rt = runtime((1200.0, 800.0), false, scene("plugin-add"));
    assert!(has_key(&rt, "plugin-json") && has_key(&rt, "plugin-add-submit") && has_key(&rt, "plugin-add-cancel"));
    assert!(!rt.state.plugin_ui.add.as_ref().unwrap().ready(), "Add is off with nothing pasted");
    click(&mut rt, "plugin-json-input");
    type_text(&mut rt, &format!(r#"{{"command":"npx","args":["-y","@acme/docs-server"],"env":{{"ACME_TOKEN":"{SECRET}"}}}}"#));
    assert!(rt.state.plugin_ui.add.as_ref().unwrap().ready(), "it reads, so Add is on");
    assert!(has_key(&rt, "plugin-name"), "one server with no name offers a Name field");
    let all = names(&rt);
    assert!(all.contains("Will add acme-docs-server: runs `npx -y @acme/docs-server`, passes ACME_TOKEN"), "{all}");
    // The paste is in its own text area, and nowhere else.
    let holders: Vec<String> = nodes(&rt.accessibility_json()).into_iter().map(|(_, n)| n).filter(|n| n.to_string().contains(SECRET)).map(|n| str_of(&n, "key").to_owned()).collect();
    assert!(holders.iter().all(|k| k.starts_with("plugin-json")), "the value shows only where it was pasted: {holders:?}");
    assert!(rt.elements().iter().filter(|e| !e.key.clone().unwrap_or_default().starts_with("plugin-json")).all(|e| !e.text.clone().unwrap_or_default().contains(SECRET)));
    // Escape closes the sheet and drops the paste.
    press(&mut rt, Key::Escape, Mods::default());
    assert!(rt.state.plugin_ui.add.is_none());
}

#[test]
fn bad_json_says_where_in_words_and_keeps_add_off() {
    let rt = runtime((1200.0, 800.0), false, scene("plugin-add-bad"));
    assert!(names(&rt).contains("not valid JSON") && names(&rt).contains("line 6, column 5"), "{}", names(&rt));
    assert!(!rt.state.plugin_ui.add.as_ref().unwrap().ready());
    let rt = runtime((1200.0, 800.0), false, scene("plugin-add-several"));
    let all = names(&rt);
    for line in ["Will add acme-docs:", "Will add example-tracker: connects to https://mcp.example.com/v1, sends the header Authorization", "Will add notes:"] {
        assert!(all.contains(line), "{line}: {all}");
    }
    assert!(all.contains("“timeout” of notes is not used by Neko and was left out."), "a note for what was left out");
    assert!(!all.contains("example-token-77c1") || rt.elements().iter().all(|e| !e.text.clone().unwrap_or_default().contains("77c1") || e.key.as_deref().is_some_and(|k| k.starts_with("plugin-json"))));
    let rt = runtime((1200.0, 800.0), false, scene("plugin-add-refused"));
    assert!(names(&rt).contains("There are 64 plugins already."));
}

#[test]
fn the_add_sheet_fits_the_smallest_window() {
    for name in ["plugin-add", "plugin-add-good", "plugin-add-bad", "plugin-add-several", "plugin-add-unnamed", "plugin-add-refused"] {
        let rt = runtime((760.0, 560.0), false, scene(name));
        for key in ["plugin-add-submit", "plugin-add-cancel", "plugin-json"] {
            let r = el(&rt, key).rect;
            assert!(r[1] >= 0.0 && r[1] + r[3] <= 560.0 && r[0] >= 0.0 && r[0] + r[2] <= 760.0, "{name}: {key} is clipped at 760x560: {r:?}");
        }
    }
}

// ---- Needs you ----------------------------------------------------------------------------------------------------------------------

#[test]
fn the_approval_card_shows_exactly_what_will_run_and_has_no_default_answer() {
    let mut rt = runtime((1200.0, 800.0), false, scene("needs-you-plugin"));
    let all = nodes(&rt.accessibility_json());
    let lists: Vec<String> = all.iter().map(|(_, n)| n).filter(|n| str_of(n, "role") == "list" && str_of(n, "name").starts_with("Waiting in")).map(|n| str_of(n, "name").to_owned()).collect();
    assert_eq!(lists, ["Waiting in acme-api", "Waiting in Plugins"], "plugins have a group of their own, headed Plugins");
    let item = rt.state.needs.iter().find(|i| matches!(i.need, Need::Plugin { .. })).unwrap().clone();
    let key = need_key(&item.id);
    let row = all.iter().map(|(_, n)| n).find(|n| str_of(n, "key") == key).expect("the card");
    assert_eq!(str_of(row, "name"), "A plugin asks: acme-docs: Approve plugin: acme-docs. It will run npx -y @acme/docs-server with ACME_TOKEN");
    assert!(str_of(row, "description").starts_with("Plugins, waiting 4 min"), "{row}");
    // The command line is whole, in its own block; no cat, no task.
    assert_eq!(el(&rt, &format!("{key}-runs")).text.as_deref().unwrap_or(""), "npx -y @acme/docs-server");
    assert!(has_key(&rt, &format!("{key}-approve")) && has_key(&rt, &format!("{key}-decline")) && has_key(&rt, &format!("{key}-open")));
    assert_eq!(el(&rt, &format!("{key}-open")).label.as_deref().unwrap_or(""), "Open in Settings");
    assert!(!has_key(&rt, &format!("{key}-allow")) && !has_key(&rt, &format!("{key}-always")));
    // Return on the card answers nothing; Return on Approve does not approve either (Space does, as on a destructive request).
    tx_focus(&mut rt, &key);
    press(&mut rt, Key::Enter, Mods::default());
    assert!(rt.state.needs_open && rt.state.visible_needs().len() == 3, "Return on the card answers nothing");
    click_focus(&mut rt, &format!("{key}-approve"));
    press(&mut rt, Key::Enter, Mods::default());
    assert_eq!(rt.state.visible_needs().len(), 3, "Return does not approve");
    // Open in Settings goes to Settings ▸ Plugins with the keyboard on that plugin's Approve.
    click(&mut rt, &format!("{key}-open"));
    assert!(rt.state.settings_open && rt.state.settings_page == neko_app::plugins::SETTINGS_PAGE && !rt.state.needs_open);
    assert_eq!(focus(&rt), Some(format!("plugin-{}-approve", item.id.trim_start_matches("plugin:"))));
}

fn tx_focus(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x + 2.0, y + 2.0);
    rt.pointer_up(x + 2.0, y + 2.0);
    rt.frame(0.0);
}

/// Focuses a button without pressing it: Tab from the card until the focus is on it.
fn click_focus(rt: &mut Runtime<App>, key: &str) {
    for _ in 0..12 {
        if focus(rt).as_deref() == Some(key) {
            return;
        }
        tab(rt, false);
    }
    panic!("could not reach {key}");
}

#[test]
fn a_changed_manifest_shows_what_differs_and_a_web_server_shows_its_address() {
    let rt = runtime((1200.0, 800.0), false, scene("needs-you-plugin-changed"));
    let item = rt.state.needs[0].clone();
    let key = need_key(&item.id);
    let all = names(&rt);
    assert!(all.contains("A plugin changed: acme-handbook: acme-handbook changed since you approved it: Runs: node server.js to node server.js --verbose; Now also passes ACME_REGION"), "{all}");
    assert!(all.contains("Runs: node server.js to node server.js --verbose") && all.contains("Now also passes ACME_REGION"), "each change is a line");
    assert!(el(&rt, &format!("{key}-runs")).text.unwrap_or_default().starts_with("node /Users/example/plugins/acme-handbook/server.js"));
    let rt = runtime((1200.0, 800.0), false, scene("needs-you-plugin-web"));
    let key = need_key(&rt.state.needs[0].id);
    assert_eq!(el(&rt, &format!("{key}-runs")).text.as_deref(), Some("https://mcp.example.com/v1"));
    assert!(names(&rt).contains("Sends the header Authorization"));
}

#[test]
fn the_card_keeps_its_buttons_whole_at_the_smallest_window() {
    for name in ["needs-you-plugin", "needs-you-plugin-changed", "needs-you-plugin-web", "needs-you-plugin-only"] {
        let rt = runtime((760.0, 560.0), false, scene(name));
        let item = rt.state.needs.iter().find(|i| matches!(i.need, Need::Plugin { .. })).unwrap();
        let key = need_key(&item.id);
        let card = el(&rt, &key).rect;
        for k in ["approve", "decline", "open"] {
            let r = el(&rt, &format!("{key}-{k}")).rect;
            assert!(r[0] >= card[0] && r[0] + r[2] <= card[0] + card[2] + 0.5, "{name}: {k} leaves the card: {r:?} in {card:?}");
        }
    }
}

#[test]
fn the_briefing_shows_a_plugin_item_without_a_cat_or_a_workspace() {
    let mut rt = runtime((1200.0, 800.0), false, scene("briefing"));
    rt.update(|s, _| {
        let mut b = s.briefing.clone().expect("the briefing scene has one");
        scene("needs-you-plugin-only")(s);
        b.needs = s.needs.clone();
        s.briefing = Some(b);
        s.open_briefing();
    });
    settle(&mut rt);
    let key = need_key(&rt.state.needs[0].id);
    assert!(has_key(&rt, &key) && has_key(&rt, &format!("{key}-approve")), "the very card, answered where it is");
    assert_eq!(rt.state.briefing.as_ref().map(neko_app::briefing::note_body).as_deref(), Some("Approve the plugin notes-sync"), "the morning's line has no cat in front of it");
}

#[test]
fn a_plugins_notification_has_no_buttons_opens_the_list_and_is_taken_back() {
    let mut rt = runtime((1200.0, 800.0), false, scene("tasks-home"));
    let rec = Recorder::default();
    let r = rec.clone();
    rt.update(move |s, _| {
        s.notifier = Notifier::new(Box::new(r));
        s.front = false;
        s.take(neko_app::link::Incoming::Message(neko_protocol::ServerMessage::Reply { id: 0, reply: neko_protocol::Reply::Needs { items: vec![] } }));
    });
    assert!(rec.take().is_empty());
    rt.update(|s, _| neko_app::scenes::SCENES.iter().find(|x| x.0 == "needs-you-plugin-only").map(|x| (x.1)(s)).unwrap());
    let item = rt.state.needs[0].clone();
    let items = vec![item.clone()];
    rt.update(move |s, _| {
        s.take(neko_app::link::Incoming::Message(neko_protocol::ServerMessage::Reply { id: 0, reply: neko_protocol::Reply::Needs { items } }));
    });
    let sent = rec.take();
    let Some(Sent::Post(note)) = sent.iter().find(|s| matches!(s, Sent::Post(_))) else { panic!("{sent:?}") };
    assert!(note.buttons.is_empty(), "never approve from a banner");
    assert_eq!(note.body, "Approve the plugin notes-sync");
    assert_eq!(note.title, "notes-sync");
    assert!(note.id.starts_with("neko.plugin."), "{}", note.id);
    assert_eq!(read_click(None, None, &note.info), ClickDo::OpenNeeds(item.id.clone()), "a click opens its card");
    assert_eq!(read_click(Some("allow_once"), None, &note.info), ClickDo::OpenNeeds(item.id.clone()), "no action ever approves");
    // The same list again posts nothing; the list without it takes the notification back.
    let again = vec![item.clone()];
    rt.update(move |s, _| s.take(neko_app::link::Incoming::Message(neko_protocol::ServerMessage::Reply { id: 0, reply: neko_protocol::Reply::Needs { items: again } })));
    assert!(rec.take().is_empty(), "one per item");
    rt.update(|s, _| s.take(neko_app::link::Incoming::Message(neko_protocol::ServerMessage::Reply { id: 0, reply: neko_protocol::Reply::Needs { items: vec![] } })));
    assert!(rec.take().contains(&Sent::Withdraw(vec![note.id.clone()])), "taken back when it leaves the list");
    // With Neko in front the person is where it can be approved: no banner.
    rt.update(|s, _| s.front = true);
    let again = vec![item];
    rt.update(move |s, _| s.take(neko_app::link::Incoming::Message(neko_protocol::ServerMessage::Reply { id: 0, reply: neko_protocol::Reply::Needs { items: again } })));
    assert!(rec.take().iter().all(|s| !matches!(s, Sent::Post(_))), "no banner for what the person is looking at");
}

// ---- A cat, a task ------------------------------------------------------------------------------------------------------------------

#[test]
fn a_cattains_profile_lists_the_plugins_it_can_use_and_a_name_opens_settings_at_it() {
    let mut rt = runtime((1200.0, 2400.0), false, scene("cattain-profile-plugins"));
    let all = names(&rt);
    assert!(all.contains("miso can use these plugins."), "{all}");
    assert!(has_key(&rt, &format!("cat-plugin-{ID}")), "acme-docs: for every cat, on");
    assert!(!has_key(&rt, "cat-plugin-plugin_00000000000000000000000002"), "notes-sync is only for mochi");
    assert!(!has_key(&rt, "cat-plugin-plugin_00000000000000000000000003"), "example-tracker is off");
    click(&mut rt, &format!("cat-plugin-{ID}"));
    assert!(rt.state.settings_open && rt.state.settings_page == neko_app::plugins::SETTINGS_PAGE);
    assert_eq!(focus(&rt), Some(format!("plugin-{ID}-switch")));
    let rt = runtime((1200.0, 2400.0), false, scene("cattain-profile-no-plugins"));
    assert!(names(&rt).contains("No plugins. A plugin gives every cat more tools; add one in Settings."));
    let rt = runtime((1200.0, 2400.0), false, scene("cattain-profile-plugins-none-for-cat"));
    assert!(names(&rt).contains("No plugin that is on is open to miso."));
}

#[test]
fn a_plugins_tool_step_reads_by_its_plugins_name() {
    let rt = runtime((1200.0, 800.0), false, scene("task-plugin"));
    let all = names(&rt);
    for line in ["acme-docs · search retry limit", "acme-docs · get page Retries and timeouts", "acme-docs · create draft"] {
        assert!(all.contains(line), "{line}: {all}");
    }
    assert!(!all.contains("mcp__"), "the raw tool name is never shown");
    let rt = runtime((1200.0, 800.0), false, scene("task-plugin-request"));
    assert!(names(&rt).contains("Use create_draft from acme-docs"), "the daemon words the request, the app shows it");
}

// ---- Looks --------------------------------------------------------------------------------------------------------------------------------

#[test]
fn the_plug_reads_on_its_ground_in_light_and_dark() {
    for dark in [false, true] {
        let t = neko_app::view::theme(dark, false);
        let c = rune_ui_style::contrast(neko_app::plugins_view::tile_ink(&t), neko_app::plugins_view::tile_ground(&t));
        assert!(c >= 4.5, "{} plug on its tile: {c:.2}:1", if dark { "dark" } else { "light" });
    }
}

#[test]
fn every_plugin_scene_fits_both_window_shapes_without_a_horizontal_overflow() {
    for (name, _) in scenes().iter().filter(|s| s.0.starts_with("plugin") || s.0.contains("-plugin")) {
        for size in SIZES {
            let rt = runtime(size, false, scene(name));
            for e in rt.elements() {
                if let Some(k) = e.key.as_deref().filter(|k| k.starts_with("plugin-") || k.starts_with("plugins-")) {
                    assert!(e.rect[0] + e.rect[2] <= size.0 + 0.5 && e.rect[0] >= -0.5, "{name} at {size:?}: {k} leaves the window: {:?}", e.rect);
                }
            }
        }
    }
}

// ---- The whole flow against a real daemon ----------------------------------------------------------------------------------------------

struct Folder {
    dir: PathBuf,
    pids: Vec<i32>,
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

impl Drop for Folder {
    fn drop(&mut self) {
        for &pid in &self.pids {
            if alive(pid) {
                // SAFETY: a daemon this test started, by the PID it wrote.
                unsafe { libc::kill(pid, libc::SIGKILL) };
            }
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn until(rt: &mut Runtime<App>, what: &str, ok: impl Fn(&Runtime<App>) -> bool) {
    let end = Instant::now() + Duration::from_secs(40);
    while !ok(rt) {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
        rt.frame(0.0);
    }
    rt.frame(0.0);
}

#[test]
fn paste_approve_test_set_a_secret_choose_cats_switch_off_and_remove_against_a_real_daemon() {
    let bin = PathBuf::from(std::env::var("NEKO_DAEMON_BIN").expect("set NEKO_DAEMON_BIN to a built neko-daemon (scripts/check.sh does)"));
    let fake_claude = bin.with_file_name("neko-fake-claude");
    let fake_mcp = bin.with_file_name("neko-fake-mcp");
    assert!(fake_claude.is_file() && fake_mcp.is_file(), "build the fake CLI and the fake MCP server first");
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let mut folder = Folder { dir: std::env::temp_dir().join(format!("neko-plugins-e2e-{}-{nanos}", std::process::id())), pids: Vec::new() };
    let dir: &Path = &folder.dir.clone();
    let home = dir.join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    // SAFETY: set before any thread of this test reads the environment; this is the one test in this binary that starts a daemon.
    unsafe {
        std::env::set_var("NEKO_CLAUDE_BIN", &fake_claude);
        std::env::set_var("HOME", &home);
        std::env::set_var("NEKO_SECRETS_FILE", dir.join("secrets-test.json"));
        std::env::set_var("NEKO_PLUGIN_CHECK_MS", "20000");
    }

    // Tall, so the whole card is on screen and every press lands.
    let mut rt = runtime((1200.0, 1600.0), false, |_| {});
    let rec = Recorder::default();
    let conn = neko_app::link::Conn::start(dir.to_path_buf(), Some(bin), || {});
    let r = rec.clone();
    rt.update(move |s, _| {
        s.notifier = Notifier::new(Box::new(r));
        s.front = false;
        s.daemon = Daemon::Starting;
        s.conn = Some(conn);
    });
    until(&mut rt, "the app to connect", |rt| rt.state.daemon == Daemon::Connected && rt.state.needs_loaded && rt.state.cats.len() >= 3);
    folder.pids.extend(neko_protocol::client::find(dir).ok().map(|(i, _)| i.pid as i32));
    assert!(rt.state.plugins.is_empty(), "nothing is added yet");

    // Settings ▸ Plugins, the Add sheet, a paste with a secret in it.
    rt.update(|s, _| s.open_plugins(None));
    settle(&mut rt);
    assert!(has_key(&rt, "plugins-empty"));
    click(&mut rt, "plugins-add-json");
    assert!(rt.state.plugin_ui.add.is_some(), "the sheet is open");
    click(&mut rt, "plugin-json-input");
    let paste = serde_json::json!({"mcpServers": {"fake-docs": {"command": fake_mcp.to_string_lossy(), "args": ["--name", "fake-docs"], "env": {"ACME_TOKEN": SECRET}}}}).to_string();
    type_text(&mut rt, &paste);
    assert!(rt.state.plugin_ui.add.as_ref().unwrap().ready());
    assert!(names(&rt).contains("Will add fake-docs: runs `"), "{}", names(&rt));
    click_in_panel(&mut rt, "plugin-add-submit");
    until(&mut rt, "the plugin to be added", |rt| rt.state.plugins.len() == 1 && rt.state.plugin_ui.add.is_none());
    let p = rt.state.plugins[0].clone();
    let id = p.id.clone();
    assert_eq!((p.state(), p.approval), (PluginState::Waiting, Approval::Waiting), "adding runs nothing: it waits");
    assert!(p.value("acme_token").is_some_and(|v| v.set && v.value.is_none()), "the secret is set, and its value is not in the plugin");
    assert!(!format!("{:?}", rt.state.plugins).contains(SECRET), "the app never holds the secret");
    assert!(!rt.accessibility_json().to_string().contains(SECRET), "nor shows it");
    assert!(has_key(&rt, &format!("plugin-{id}-approve")), "its card, with Approve");
    assert_eq!(focus(&rt), Some(format!("plugin-{id}-approve")), "the keyboard goes to it");

    // Needs you holds the card (the person is in another app: one banner with no buttons, never the secret).
    until(&mut rt, "the card in Needs you", |rt| rt.state.needs.iter().any(|i| matches!(i.need, Need::Plugin { .. })));
    let sent = rec.take();
    let banners: Vec<&neko_app::notify::Note> = sent.iter().filter_map(|s| if let Sent::Post(n) = s { Some(n) } else { None }).collect();
    assert_eq!(banners.len(), 1, "{sent:?}");
    assert!(banners[0].buttons.is_empty() && banners[0].body == "Approve the plugin fake-docs" && !format!("{banners:?}").contains(SECRET), "{banners:?}");
    press(&mut rt, Key::Command('2'), Mods { cmd: true, ..Mods::default() });
    let item = rt.state.needs.iter().find(|i| matches!(i.need, Need::Plugin { .. })).unwrap().clone();
    let key = need_key(&item.id);
    assert_eq!(item.id, format!("plugin:{id}"));
    assert_eq!(el(&rt, &format!("{key}-runs")).text.as_deref().map(|t| t.starts_with(&*fake_mcp.to_string_lossy())), Some(true), "the exact command");
    click(&mut rt, &format!("{key}-approve"));
    assert_eq!(rt.state.visible_needs().len(), 0, "answered: gone at once");
    until(&mut rt, "the plugin to be approved", |rt| rt.state.plugins[0].approval == Approval::Approved);
    until(&mut rt, "the list to be empty", |rt| rt.state.needs.is_empty());
    assert!(rec.take().contains(&Sent::Withdraw(vec![note_id(&neko_protocol::TaskId(item.id.clone()))])), "its banner is taken back");
    assert_eq!(rt.state.plugins[0].state(), PluginState::On);

    // Test: "Testing…" at once, then the tools.
    rt.update(|s, _| s.open_plugins(Some(&s.plugins[0].id.clone())));
    settle(&mut rt);
    click(&mut rt, &format!("plugin-{id}-test"));
    assert!(rt.state.plugin_ui.testing.contains(&id) && has_key(&rt, &format!("plugin-{id}-testing")), "Testing… while it runs");
    until(&mut rt, "the check to end", |rt| rt.state.plugins[0].last_check.is_some() && !rt.state.plugin_ui.testing.contains(&id));
    let check = rt.state.plugins[0].last_check.clone().unwrap();
    assert!(check.ok && !check.tools.is_empty(), "{check:?}");
    assert!(names(&rt).contains(&format!("Connected: {} tool", check.tools.len())), "{}", names(&rt));
    assert!(el(&rt, &format!("plugin-{id}-tools")).text.unwrap_or_default().contains(&check.tools[0]));

    // The secret: Remove clears it (the daemon deletes it from its store), and the card asks for a new one.
    click(&mut rt, &format!("plugin-{id}-acme_token-remove"));
    until(&mut rt, "the secret to be gone", |rt| rt.state.plugins[0].value("acme_token").is_some_and(|v| !v.set));
    assert!(has_key(&rt, &format!("plugin-{id}-acme_token-input")), "a masked field again");
    click(&mut rt, &format!("plugin-{id}-acme_token-input"));
    type_text(&mut rt, "a-second-secret-value");
    click(&mut rt, &format!("plugin-{id}-acme_token-set"));
    until(&mut rt, "the secret to be set", |rt| rt.state.plugins[0].value("acme_token").is_some_and(|v| v.set));
    assert!(rt.state.plugin_ui.secrets.values().all(|e| e.text.is_empty()) && has_key(&rt, &format!("plugin-{id}-acme_token-status")));
    assert!(!rt.accessibility_json().to_string().contains("a-second-secret-value"));

    // Which cats: Only some cats sends nothing until a cat is chosen; then exactly that cat.
    click(&mut rt, &format!("plugin-{id}-access-1"));
    assert_eq!(rt.state.plugins[0].access.mode, neko_protocol::AccessMode::All);
    let cat = rt.state.active_cats()[1].id.clone();
    assert!(rt.state.plugin_ui.only.contains(&id), "Only some cats was chosen");
    click_in_panel(&mut rt, &format!("plugin-{id}-cat-{cat}"));
    until(&mut rt, "the access to change", |rt| rt.state.plugins[0].access.mode == neko_protocol::AccessMode::Only);
    assert_eq!(rt.state.plugins[0].access.cats, vec![cat.clone()]);
    assert!(rt.state.plugins[0].runs_for(&cat) && !rt.state.plugins[0].runs_for(&rt.state.active_cats()[0].id.clone()));

    // A Cattain's Profile names the plugins it may use.
    rt.update({
        let cat = cat.clone();
        move |s, _| s.open_cattain_on(&cat, neko_app::pane::Tab::Profile)
    });
    let key = format!("cat-plugin-{id}");
    until(&mut rt, "its Profile", move |rt| has_key(rt, &key));

    // Off, and a plugin that is off is on no cat's page.
    rt.update(|s, _| s.open_plugins(None));
    settle(&mut rt);
    click(&mut rt, &format!("plugin-{id}-switch"));
    until(&mut rt, "it to be off", |rt| !rt.state.plugins[0].enabled);
    assert_eq!(rt.state.plugins[0].state(), PluginState::Off);

    // Remove asks first, then goes, and its secrets with it.
    click(&mut rt, &format!("plugin-{id}-remove"));
    assert!(rt.state.confirm.is_some() && rt.state.plugins.len() == 1, "asked first");
    click_in_panel(&mut rt, "remove-plugin-action");
    until(&mut rt, "the plugin to be removed", |rt| rt.state.plugins.is_empty());
    assert!(has_key(&rt, "plugins-empty"));
    let secrets = std::fs::read_to_string(dir.join("secrets-test.json")).unwrap_or_default();
    assert!(!secrets.contains(SECRET), "and the daemon kept nothing under a removed plugin's name: {secrets}");

    rt.update(|s, _| s.conn = None);
    for &pid in &folder.pids {
        // SAFETY: the daemon this test started, by its PID.
        unsafe { libc::kill(pid, libc::SIGTERM) };
        let end = Instant::now() + Duration::from_secs(10);
        while alive(pid) && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive(pid), "the daemon stopped cleanly");
    }
}
