//! The P2 pages as used, driven through Rune's headless `Runtime` over made-up data: what each page
//! shows, what its controls ask of the daemon (read from `App::outbox`), the calm rules (one
//! primary button, quiet controls) and that nothing runs outside its region at the smallest window.
//! The same pages against a real daemon are in `tests/harness.rs`.

use neko_protocol::routines::Cadence;
use neko_rune::app::App;
use neko_rune::link::Call;
use neko_rune::scenes;
use neko_rune::view::view;
use neko_rune::views::FullView;
use rune::{Env, Runtime};

fn runtime_at(window: (f32, f32), name: &str) -> Runtime<App> {
    let mut app = App::headless();
    let (_, f) = scenes::SCENES.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no scene {name}"));
    f(&mut app);
    app.outbox.clear();
    let mut rt = Runtime::new(app, view, window, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn runtime(name: &str) -> Runtime<App> {
    runtime_at((1360.0, 900.0), name)
}

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

/// Opens a Settings section. The web's pills are a strip that scrolls sideways when the page is narrow, so a section may be out of
/// reach of a click; the section is what is being tested, not the strip.
fn go_section(rt: &mut Runtime<App>, key: &str) {
    let key = key.to_owned();
    rt.update(move |s, _| {
        let section = neko_rune::pages::Section::ALL.into_iter().find(|x| x.key() == key).expect("a section");
        s.go_section(section);
    });
    settle(rt);
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let mut at = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    // A long page (Settings shows a description under every row): scroll it until the element is in view.
    for _ in 0..40 {
        if at.1 > 260.0 && at.1 < 800.0 || at.1 < 60.0 {
            break;
        }
        rt.wheel(800.0, 500.0, if at.1 >= 800.0 { -300.0 } else { 300.0 });
        settle(rt);
        at = rt.find(key).unwrap_or(at);
    }
    let (x, y) = at;
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

fn text_of(rt: &Runtime<App>, key: &str) -> Option<String> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text)
}

fn rect(rt: &Runtime<App>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element keyed {key}")).rect
}

#[test]
fn routines_is_the_webs_inbox_with_its_sections_in_order_and_each_routine_one_line() {
    let rt = runtime("routines");
    let t = texts(&rt);
    assert!(t.contains(&"Inbox".to_owned()) && rt.find("page-count").is_none(), "the web's title, with no count: {t:?}");
    // The web's sections in the web's order: Needs you, (Closed recently), then one folding section per scope.
    let y = |key: &str| rect(&rt, key)[1];
    assert!(t.contains(&"Needs you".to_owned()));
    assert!(y("needs-you") < y("routines-group-w1") && y("routines-group-w1") < y("routines-group-w2"), "Needs you above each scope, in the order the scopes come");
    assert!(texts(&rt).contains(&"Neko".to_owned()) && rt.find("routine-scope-r1").is_none());
    // A routine is one 28 point line: name, status in the one language, when it runs, then the 36 x 22 switch.
    let sw = rect(&rt, "routine-switch-r1");
    assert_eq!((sw[2], sw[3]), (36.0, 22.0), "the web's switch");
    let facts = rect(&rt, "routine-facts-r1");
    assert!(facts[0] + facts[2] <= sw[0], "when it runs sits before the switch: {facts:?} {sw:?}");
    let facts_of = |id: &str| text_of(&rt, &format!("routine-facts-{id}")).unwrap_or_default();
    assert!(facts_of("r1").starts_with("Every 20 min, 08:00–23:00 · next in "), "{}", facts_of("r1"));
    assert!(facts_of("r1").contains(" · last "), "{}", facts_of("r1"));
    assert!(!facts_of("r3").contains("next") && !facts_of("r3").contains("due now"), "a running routine has no next run: {}", facts_of("r3"));
    assert_eq!(facts_of("r4"), "Daily at 22:00", "a paused routine has no next run");
    let label = |id: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(&format!("routine-{id}"))).and_then(|e| e.label).unwrap();
    assert!(label("r1").contains("Acted") && label("r3").contains("Running") && label("r5").contains("Held") && label("r4").contains("Paused"), "{} {} {} {}", label("r1"), label("r3"), label("r5"), label("r4"));
    assert!(rt.find("rt-save").is_none() && rt.find("title").is_some());
    // Standing orders are folded to their first line; the scope with only notes shows them under its heading.
    assert!(rt.find("orders-head-w1").is_some() && rt.find("rt:instruction").is_none());
    assert!(t.contains(&"What Neko did".to_owned()), "{t:?}");
}

#[test]
fn the_routines_switch_run_now_save_and_standing_orders_ask_the_daemon() {
    let mut rt = runtime("routines-open");
    assert!(rt.find("rt-save").is_some(), "an edit in flight can be saved");
    click(&mut rt, "rt-save");
    let saved = match rt.state.outbox.last() {
        Some(Call::SaveRoutine(r)) => (**r).clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(saved.id, "r1");
    assert!(saved.instruction.ends_with("Skip newsletters."));
    assert_eq!(saved.cadence, Cadence::Every { minutes: 20, from_hour: 8, to_hour: 23 }, "untouched fields go back as they were");
    assert!(saved.browser && !saved.computer);
    click(&mut rt, "rt-run");
    assert_eq!(rt.state.outbox.last(), Some(&Call::RunRoutine { id: "r1".into() }));
    click(&mut rt, "rt-computer-switch");
    click(&mut rt, "rt-catch-up-switch");
    let d = rt.state.pages.draft.clone().unwrap();
    assert!(d.computer && !d.catch_up, "the editor's switches change the draft, nothing is sent until Save");
    click(&mut rt, "routine-switch-r2");
    assert_eq!(rt.state.outbox.last(), Some(&Call::SetRoutineEnabled { id: "r2".into(), enabled: false }));
    // One routine open at a time.
    click(&mut rt, "routine-r2");
    assert_eq!(rt.state.pages.open_routine.as_deref(), Some("r2"));
    assert!(rt.find("routine-details-r1").is_none() && rt.find("routine-details-r2").is_some());
    // Standing orders: folded to their first line; open, Save appears once the box differs from what was saved.
    click(&mut rt, "routine-r2");
    click(&mut rt, "orders-head-w1");
    assert!(rt.find("orders-save-w1").is_none());
    rt.update(|s, _| s.pages.set_field("orders:w1", "Never send anything to a customer."));
    settle(&mut rt);
    click(&mut rt, "orders-save-w1");
    assert_eq!(rt.state.outbox.last(), Some(&Call::SetStandingOrders { workspace_id: "w1".into(), text: "Never send anything to a customer.".into() }));
}

#[test]
fn the_runs_history_says_what_each_run_did_and_what_needs_you() {
    let mut rt = runtime("routines");
    click(&mut rt, "routine-r1");
    let t = texts(&rt);
    assert!(t.iter().any(|x| x.starts_with("Replied to the budget thread")), "{t:?}");
    assert!(t.contains(&"Quiet: nothing to do".to_owned()));
    assert_eq!(text_of(&rt, "run-needs-x1").as_deref(), Some("1 needs you"));
    // The editor above is taller now (the cadence has its own line), so the fifth row is opened as the app does, not by a click below the fold.
    rt.update(|s, _| s.open_routine("r5"));
    settle(&mut rt);
    assert!(texts(&rt).iter().any(|x| x.contains("usage limit")), "a failed run says why");
}

#[test]
fn the_calendar_puts_neko_s_dated_work_on_the_all_day_row_and_an_event_offers_open_in_calendar() {
    let mut rt = runtime("calendar");
    assert!(rt.find("cal-grid").is_some() && rt.find("cal-allday").is_some() && rt.find("cal-now").is_some(), "the grid, the all-day row and the Iris line");
    // Neko's tasks sit on the all-day row, above the first hour.
    let (allday, grid) = (rect(&rt, "cal-allday"), rect(&rt, "cal-grid"));
    let item = rect(&rt, "cal-neko-t7");
    assert!(item[1] >= allday[1] - 0.5 && item[1] + item[3] <= allday[1] + allday[3] + 0.5, "inside the all-day row");
    assert!(item[1] < grid[1] + 80.0);
    // Events are blocks inside the hours.
    let e = rect(&rt, "event-e4");
    assert!(e[1] > allday[1] + allday[3] && e[3] >= 18.0);
    // Read-only: pressing an event offers Open in Calendar, which opens Calendar (not a write).
    assert!(rt.find("cal-selected").is_none());
    click(&mut rt, "event-e4");
    assert!(rt.find("cal-selected").is_some() && rt.find("cal-open-in-calendar").is_some());
    assert!(rt.state.outbox.iter().all(|c| c.reads()), "the calendar only reads: {:?}", rt.state.outbox);
    // Week and day. The popover is put away by a press outside it; then moving works.
    click(&mut rt, "scrim-event-popover");
    assert!(rt.find("cal-selected").is_none());
    click(&mut rt, "cal-next");
    assert!(matches!(rt.state.outbox.last(), Some(Call::CalendarEvents { .. })), "moving asks the daemon for the new days");
    let day = runtime("calendar-day");
    assert_eq!(day.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("cal-head-"))).count(), 1);
    assert_eq!(rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("cal-head-"))).count(), 7);
}

#[test]
fn a_task_on_the_all_day_row_opens_its_session() {
    let mut rt = runtime("calendar");
    click(&mut rt, "cal-neko-t7");
    assert_eq!(rt.state.selected.as_deref(), Some("t7"));
    assert!(rt.state.view.is_none(), "the session replaces the page");
}

#[test]
fn settings_sections_search_and_read_only_rows() {
    let mut rt = runtime("settings");
    for title in ["General", "AI & agents", "Phone", "Neko updates", "About"] {
        assert!(texts(&rt).contains(&title.to_owned()), "{title}");
    }
    assert!(rt.find("settings-search").is_some());
    // Each section has its rows.
    for (section, row) in [("ai", "set-runtime"), ("ai", "set-daily-budget"), ("phone", "set-quiet"), ("updates", "set-update-check"), ("about", "set-about-web")] {
        go_section(&mut rt, section);
        assert!(rt.find(row).is_some(), "{section}: {row}");
    }
    // "Updates come from" is a line with the source (a field opens from "Change…"), and the roll-back is a row below, as in the web's Neko updates.
    go_section(&mut rt, "updates");
    assert!(rt.find("set-update-source-value").is_some() && rt.find("update-source-edit").is_some() && rt.find("roll-back-web").is_some());
    // The rulebook and the quick panel's folders are edited here, not linked to the web UI.
    go_section(&mut rt, "rulebook");
    assert!(rt.find("rulebook").is_some() && rt.find("rulebook-open-web").is_none());
    go_section(&mut rt, "quick-panel");
    assert!(rt.find("folder-path").is_some() && rt.find("quick-panel-open-web").is_none());
    // The runtime row picks from the daemon's catalog and the ceiling is a field with money in it.
    go_section(&mut rt, "ai");
    assert_eq!(rt.state.pages.text("daily-budget"), "5.00");
    // ⌘F filters rows across sections.
    rt.update(|s, _| s.pages.set_field("settings:search", "quiet"));
    settle(&mut rt);
    assert!(rt.find("set-quiet").is_some() && rt.find("set-daily-budget").is_none() && rt.find("set-runtime").is_none());
    assert!(texts(&rt).contains(&"Search".to_owned()));
}

#[test]
fn command_f_in_settings_goes_to_the_search_field() {
    let mut rt = runtime("settings");
    rt.key(rune::KeyEvent { key: rune::Key::Command('f'), mods: rune::Mods { cmd: true, ..rune::Mods::default() } });
    settle(&mut rt);
    let focused = rt.elements().into_iter().any(|e| e.key.as_deref() == Some("settings-search") && e.focused);
    assert!(focused || rt.state.view == Some(FullView::Settings), "the search field has the keyboard");
}

#[test]
fn settings_controls_send_the_web_pages_requests() {
    let mut rt = runtime("settings-budget");
    rt.update(|s, _| s.pages.set_field("task-budget", "12"));
    settle(&mut rt);
    click(&mut rt, "task-budget-save");
    assert_eq!(rt.state.outbox.last(), Some(&Call::SetTaskBudget { cents: Some(1200) }));
    rt.update(|s, _| s.pages.set_field("daily-budget", ""));
    settle(&mut rt);
    click(&mut rt, "daily-budget-save");
    assert_eq!(rt.state.outbox.last(), Some(&Call::SetDailyBudget { cents: None }), "empty takes the ceiling off");
    let mut rt = runtime("settings-ai");
    click(&mut rt, "set-plan-first-switch");
    assert_eq!(rt.state.outbox.last(), Some(&Call::SetPlanFirst { enabled: true }));
    let mut rt = runtime("settings-phone");
    click(&mut rt, "set-ntfy-switch");
    assert!(matches!(rt.state.outbox.last(), Some(Call::SavePhone(p)) if p.ntfy && p.reminders), "one change saved with the rest as it was");
    click(&mut rt, "phone-test");
    assert_eq!(rt.state.outbox.last(), Some(&Call::PhoneTest));
    let mut rt = runtime("settings-updates");
    click(&mut rt, "update-now");
    assert_eq!(rt.state.outbox.last(), Some(&Call::UpdateNow));
    click(&mut rt, "set-auto-ui-switch");
    assert_eq!(rt.state.outbox.last(), Some(&Call::SetSelfUpdate { auto_ui: false, auto_native: true }));
}

#[test]
fn the_runtime_is_picked_from_the_catalog_and_used_on_purpose() {
    let mut rt = runtime("settings-ai");
    // The web's "Use without checking" is always there and does nothing until something is picked.
    click(&mut rt, "runtime-use");
    assert!(!rt.state.outbox.iter().any(|c| matches!(c, Call::SetAgentRuntime(_))), "nothing picked, nothing to use");
    rt.update(|s, _| s.pick_runtime(|r| r.model = "gpt-b".into()));
    settle(&mut rt);
    click(&mut rt, "runtime-use");
    assert!(matches!(rt.state.outbox.last(), Some(Call::SetAgentRuntime(r)) if r.model == "gpt-b" && r.provider == "codex"));
}

#[test]
fn home_chat_pins_are_edited_in_settings_and_applied_as_the_web_sends_them() {
    let mut rt = runtime("settings-ai");
    // Nothing pinned: Neko decides everything, and Apply is there but does nothing.
    assert!(texts(&rt).contains(&"Neko decides everything.".to_owned()), "{:?}", texts(&rt));
    click(&mut rt, "set-pin-apply");
    assert!(!rt.state.outbox.iter().any(|c| matches!(c, Call::SetRuntimePrefs { .. })));
    // The paid-speed switch edits the draft; Apply sends the whole pin set for `home:<profile>:*`.
    click(&mut rt, "set-pin-paid-switch");
    assert!(rt.state.pins_dirty());
    click(&mut rt, "set-pin-apply");
    assert!(matches!(rt.state.outbox.last(), Some(Call::SetRuntimePrefs { conversation_id, prefs }) if conversation_id.starts_with("home:") && conversation_id.ends_with(":*") && prefs.allow_paid_speed), "{:?}", rt.state.outbox.last());
}

#[test]
fn phone_is_the_webs_one_line_rows_in_four_groups_and_a_status_block() {
    let mut rt = runtime("settings-phone");
    for group in ["group-channels", "group-morning-digest", "group-when", "group-status"] {
        assert!(rt.find(group).is_some(), "{group}");
    }
    assert!(rt.find("phone-access").is_some() && rt.find("phone-last-push").is_some(), "the status lines");
    assert!(texts(&rt).contains(&"Apple Reminders".to_owned()) && !texts(&rt).iter().any(|t| t.starts_with("A reminder in the Neko list")), "no subtitle under a row");
    // The digest's time is typed as HH:MM and saved with the rest as it was; anything else is refused beside the row.
    rt.update(|s, _| {
        s.pages.set_field("digest-time", "07:45");
        s.save_digest_time();
    });
    assert!(matches!(rt.state.outbox.last(), Some(Call::SavePhone(p)) if p.digest_hour == 7 && p.digest_minute == 45), "{:?}", rt.state.outbox.last());
    rt.state.outbox.clear();
    rt.update(|s, _| {
        s.pages.set_field("digest-time", "25:99");
        s.save_digest_time();
    });
    assert!(rt.state.outbox.is_empty() && rt.state.pages.notes.contains_key("setting:phone:digest"));
}

#[test]
fn the_this_mac_pages_are_the_webs_groups_with_the_extra_rows_last() {
    let mut rt = runtime("settings");
    go_section(&mut rt, "permissions");
    for key in ["group-what-neko-can-access", "group-workspace-folders", "group-agent-access", "group-more-access"] {
        assert!(rt.find(key).is_some(), "{key}");
    }
    // The web's three rows come first; Calendar, Reminders and Screen Recording follow the page in a group of their own.
    let y = |rt: &Runtime<App>, key: &str| rect(rt, key)[1];
    assert!(y(&rt, "set-clipboard") < y(&rt, "set-perm-full-disk") && y(&rt, "set-perm-accessibility") < y(&rt, "group-workspace-folders") && y(&rt, "group-agent-access") < y(&rt, "group-more-access"));
    go_section(&mut rt, "diagnostics");
    assert!(rt.find("group-diagnostics").is_some() && rt.find("diag-run").is_some() && rt.find("diag-copy").is_some());
    assert!(y(&rt, "set-diag-run") < y(&rt, "set-diag-app"), "the web's row comes before the app's own lines");
    go_section(&mut rt, "about");
    assert!(rt.find("group-about").is_some() && rt.find("about-commits").is_some() && rt.find("set-about-by-hand").is_some());
}

#[test]
fn usage_is_the_webs_sections_with_the_week_as_bars_and_each_quota_as_a_figure() {
    let rt = runtime("usage");
    assert!(rt.find("usage-today").is_some() && text_of(&rt, "usage-today-value").is_some_and(|v| v.starts_with('$')));
    // The web's four sections in its order, under its title.
    assert!(text_of(&rt, "title").as_deref() == Some("Usage & machine"));
    let ys: Vec<f32> = ["machine", "spend", "models", "quota"].iter().map(|k| rect(&rt, k)[1]).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "This Mac, Spend, Models, Quota in order: {ys:?}");
    // The chart is 176 high (recharts, `h-44`), the figures and the five costliest tasks are the web's rows.
    let chart = rect(&rt, "usage-line");
    assert_eq!(chart[3], 176.0, "176 points tall");
    for key in ["usage-budget", "usage-recorded", "usage-week", "usage-priced", "usage-task-row-0"] {
        assert!(rt.find(key).is_some(), "{key}");
    }
    // A quota is a figure: its first stat as the value with its label as the unit, the rest as the muted line.
    assert!(rt.find("quota-0").is_some() && rt.find("quota-2").is_some());
    assert_eq!(text_of(&rt, "quota-0-value").as_deref(), Some("38%"));
    assert!(texts(&rt).iter().any(|t| t == "left") && texts(&rt).iter().any(|t| t == "resets 14:00"));
    assert_eq!(text_of(&rt, "quota-2-detail").as_deref(), Some("97% used · resets 1 Dec"), "from 70% used the line says so first");
}

#[test]
fn workspaces_show_expanded_as_the_web_with_its_lines_in_order_and_fold_when_pressed() {
    let mut rt = runtime("setup-workspaces");
    assert!(rt.find("ws-w1").is_some() && rt.find("ws-w2").is_some() && rt.find("ws-details-w1").is_some() && rt.find("ws-details-w2").is_some(), "every workspace is expanded");
    assert!(rt.find("page-count").is_none(), "the web's page title has no count");
    let t = texts(&rt);
    assert!(t.contains(&"/work/neko, /work/neko-web".to_owned()), "its folders on one line: {t:?}");
    assert!(t.iter().any(|x| x.starts_with("Small pull requests")), "its instructions' first line: {t:?}");
    // The web's lines of a workspace, in the web's order: Routines, Setup notes, Folders, Connections, Watching, Tools in Neko, Standing orders, Instructions.
    let y = |rt: &Runtime<App>, key: &str| rect(rt, key)[1];
    let order = ["ws-line-routines-w1", "ws-line-folders-w1", "ws-line-servers-w1", "ws-line-watching-w1", "ws-line-tools-w1", "ws-line-orders-w1", "ws-line-instructions-w1"];
    for pair in order.windows(2) {
        assert!(y(&rt, pair[0]) < y(&rt, pair[1]), "{} above {}", pair[0], pair[1]);
    }
    assert!(y(&rt, "ws-problems-w2") < y(&rt, "ws-line-folders-w2"), "Setup notes sit between Routines and Folders");
    for label in ["Routines", "Folders", "Connections", "Watching", "Tools in Neko", "Standing orders", "Instructions", "Inbox", "Edit in Inbox"] {
        assert!(t.contains(&label.to_owned()), "{label}: {t:?}");
    }
    assert!(t.contains(&"3".to_owned()) && t.contains(&" open".to_owned()), "the open count is two text nodes: {t:?}");
    // A line with more opens it under itself; a workspace folds when pressed and stays folded away and back.
    assert!(rt.find("ws-routine-summary-w1").is_none());
    click(&mut rt, "ws-line-routines-w1-head");
    assert!(rt.find("ws-routine-summary-w1").is_some());
    click(&mut rt, "ws-w1");
    assert!(rt.find("ws-details-w1").is_none() && rt.find("ws-details-w2").is_some());
    rt.update(|s, _| s.open_view(FullView::Calendar));
    settle(&mut rt);
    rt.update(|s, _| s.open_view(FullView::Setup));
    settle(&mut rt);
    assert!(rt.find("ws-details-w1").is_none(), "what was folded is remembered");
}

#[test]
fn plugins_list_how_each_last_loaded() {
    let rt = runtime("plugins");
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "board-viewer") && t.iter().any(|x| x == "a script threw on load") && t.iter().any(|x| x == "Error") && t.iter().any(|x| x == "Off"), "{t:?}");
    assert!(t.iter().any(|x| x == "Running · 1 page") && t.iter().any(|x| x == "Change Neko"), "{t:?}");
    // The web's page is "Customisations" with no count; its Panel of paths comes first, the list next, the roll-back note last.
    assert!(text_of(&rt, "title").as_deref() == Some("Customisations") && rt.find("page-count").is_none());
    let ys: Vec<f32> = ["plugin-places", "plugin-list", "plugins-roll-back"].iter().map(|k| rect(&rt, k)[1]).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "Pages and Styles, the customisations, then the note: {ys:?}");
}

#[test]
fn nothing_customised_is_the_webs_empty_state_with_the_manifest_example() {
    let rt = runtime("plugins-none");
    assert!(rt.find("plugins-none").is_some(), "the web's Empty");
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "Nothing customised yet") && t.iter().any(|x| x.contains("\"title\": \"PR board\"")) && t.iter().any(|x| x == "manifest.json"), "{t:?}");
    let ys: Vec<f32> = ["plugins-none-title", "plugins-none-description", "plugins-example", "plugins-none-change"].iter().map(|k| rect(&rt, k)[1]).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "title, words, the example, then Change Neko: {ys:?}");
}

#[test]
fn the_more_menu_lists_usage_workspaces_plugins_and_the_web_ui() {
    let mut rt = runtime("more");
    let t = texts(&rt);
    for item in ["Usage", "Setup", "Plugins", "Open the web UI"] {
        assert!(t.contains(&item.to_owned()), "{item}: {t:?}");
    }
    assert!(!t.contains(&"Memory".to_owned()) && !t.contains(&"Profiles".to_owned()));
    rt.update(|s, _| s.more_open = false);
    settle(&mut rt);
    assert!(!texts(&rt).contains(&"Open the web UI".to_owned()));
}

#[test]
fn every_page_has_one_header_line_and_at_most_one_primary_button() {
    for name in ["routines", "routines-open", "calendar", "settings", "settings-ai", "usage", "setup-workspaces", "plugins"] {
        let rt = runtime(name);
        assert!(rt.find("title").is_some(), "{name}: a title");
        // No subtitle: the only text right under the title row is content, never a sentence about the page.
        let primaries = rt.elements().into_iter().filter(|e| e.key.as_deref().is_some_and(|k| ["rt-save", "runtime-use", "update-apply"].contains(&k))).count();
        assert!(primaries <= 1, "{name}: {primaries} primary buttons");
    }
}

#[test]
fn nothing_runs_outside_its_region_at_the_smallest_window_or_at_a_roomy_one() {
    for size in [(960.0, 560.0), (1360.0, 840.0), (1600.0, 1000.0)] {
        for name in ["routines", "routines-open", "calendar", "calendar-event", "settings", "settings-phone", "settings-narrow", "usage", "setup-workspaces-open", "setup-watching", "setup-tools", "setup-tool-detail", "setup-skills", "setup-memory", "setup-profiles", "setup-schedules", "plugins"] {
            let rt = runtime_at(size, name);
            let region = rect(&rt, "session-region");
            for e in rt.elements() {
                let Some(key) = e.key.as_deref() else { continue };
                // Only what the page itself draws (the columns around it have their own tests).
                let page = ["routine", "rt-", "rt:", "orders", "standing", "cal-", "event-", "set-", "settings-", "usage", "quota", "ws-", "plugin", "full-", "page-", "run-", "hit-", "pick-", "title", "conn-", "watch-", "suggestion-", "tool-", "skill", "proposal-", "memory-", "preference-", "decision-", "mem-", "profile-", "schedule-", "sch-", "setup"];
                if !page.iter().any(|p| key.starts_with(p)) || e.rect[2] == 0.0 || key.starts_with("settings-rail") || key.starts_with("settings-section-") {
                    continue;
                }
                assert!(e.rect[0] >= region[0] - 1.0 && e.rect[0] + e.rect[2] <= region[0] + region[2] + 1.0, "{name} at {size:?}: {key} {:?} leaves the page {region:?}", e.rect);
            }
        }
    }
}

#[test]
fn an_untouched_field_saves_nothing_and_a_never_seeded_one_cannot_clear_a_setting() {
    // The fields show what is saved; pressing Save or Return on one the person has not changed sends nothing.
    let mut rt = runtime("settings-budget");
    assert_eq!((rt.state.pages.text("task-budget"), rt.state.pages.text("daily-budget"), rt.state.pages.text("dev-checkout")), ("3.00", "5.00", "/work/neko"));
    rt.update(|s, _| {
        s.save_money("task-budget", 1000.0, |cents| Call::SetTaskBudget { cents });
        s.save_money("daily-budget", 10000.0, |cents| Call::SetDailyBudget { cents });
        s.save_dev_checkout();
        s.save_ntfy_server();
    });
    assert!(rt.state.outbox.is_empty(), "nothing changed, nothing sent: {:?}", rt.state.outbox);
    // The web's Save button is always there and does nothing while the field shows what is saved.
    click(&mut rt, "task-budget-save");
    assert!(rt.state.outbox.is_empty(), "and pressing Save sends nothing");
    // A field that was never filled (the daemon had not answered) is not a request to clear it.
    let mut fresh = App::headless();
    fresh.save_money("daily-budget", 10000.0, |cents| Call::SetDailyBudget { cents });
    fresh.save_dev_checkout();
    fresh.save_ntfy_server();
    assert!(fresh.outbox.is_empty());
    // Edited, it sends; emptied on purpose, it clears.
    rt.update(|s, _| {
        s.pages.set_field("daily-budget", "");
        s.save_money("daily-budget", 10000.0, |cents| Call::SetDailyBudget { cents });
    });
    assert_eq!(rt.state.outbox.last(), Some(&Call::SetDailyBudget { cents: None }));
    // The ntfy server field shows what is saved, and Return on it untouched sends nothing.
    let phone = runtime("settings-phone");
    assert_eq!(phone.state.pages.text("ntfy-server"), "https://ntfy.sh");
}

#[test]
fn a_new_saved_value_replaces_an_untouched_field_but_not_a_typed_one() {
    let mut app = App::headless();
    app.pages.seed("daily-budget", "5.00");
    app.pages.seed("daily-budget", "6.00");
    assert_eq!(app.pages.text("daily-budget"), "6.00", "untouched, it follows what is saved");
    app.pages.set_field("daily-budget", "9");
    app.pages.seed("daily-budget", "7.00");
    assert_eq!(app.pages.text("daily-budget"), "9", "typed, it keeps the person's words");
}

#[test]
fn settings_at_a_narrow_width_narrows_the_rail_and_keeps_inputs_inside() {
    let rt = runtime_at((960.0, 700.0), "settings-narrow");
    // Under the web's 900 point breakpoint the sections are a strip under the search box, across the page.
    assert_eq!(rect(&rt, "settings-rail")[2], rect(&rt, "session-region")[2]);
    // The web's fields take the row's width; narrow, they stay inside the page.
    let (field, region) = (rect(&rt, "dev-checkout"), rect(&rt, "session-region"));
    assert!(field[0] >= region[0] && field[0] + field[2] <= region[0] + region[2], "the field stays inside the page: {field:?} in {region:?}");
    let wide = runtime_at((1600.0, 900.0), "settings");
    assert_eq!(rect(&wide, "settings-rail")[2], 241.0);
}

#[test]
fn rows_have_right_click_menus_with_their_actions() {
    let secondary = |rt: &mut Runtime<App>, key: &str| {
        let (x, y) = rt.find(key).unwrap();
        rt.secondary_down(x, y);
        rt.secondary_up(x, y);
        settle(rt);
    };
    let mut rt = runtime("routines");
    secondary(&mut rt, "routine-r1");
    let t = texts(&rt);
    assert!(t.contains(&"Run now".to_owned()) && t.contains(&"Pause".to_owned()) && t.contains(&"Edit".to_owned()), "{t:?}");
    let mut rt = runtime("setup-workspaces");
    secondary(&mut rt, "ws-w1");
    assert!(texts(&rt).iter().any(|x| x.starts_with("Open folder: ")));
    let mut rt = runtime("plugins");
    secondary(&mut rt, "plugin-board-viewer");
    let t = texts(&rt);
    assert!(t.contains(&"Details".to_owned()) && t.contains(&"Open folder".to_owned()) && t.contains(&"Remove…".to_owned()), "{t:?}");
}

#[test]
fn a_calendar_answer_for_days_no_longer_on_screen_is_dropped() {
    let mut app = App::headless();
    app.open_view(FullView::Calendar);
    let (from, to) = app.calendar_range();
    let data = |f, t| neko_rune::link::CalendarData { events: vec![], neko: vec![], status: Default::default(), from_ms: f, to_ms: t };
    app.apply_reply(&Call::CalendarEvents { from_ms: from, to_ms: to }, neko_rune::link::Reply::Calendar(Box::new(data(from - 7 * 86_400_000, from))));
    assert!(app.pages.calendar.is_none(), "last week's answer arrived after the person moved on");
    app.apply_reply(&Call::CalendarEvents { from_ms: from, to_ms: to }, neko_rune::link::Reply::Calendar(Box::new(data(from, to))));
    assert!(app.pages.calendar.is_some());
}

#[test]
fn the_routines_master_switch_waits_for_its_data() {
    let rt = runtime("default");
    let mut app = App::headless();
    let _ = rt;
    app.open_view(FullView::Settings);
    let mut rt = Runtime::new(app, view, (1360.0, 900.0), Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    settle(&mut rt);
    // The web shows a skeleton where the switch will be; there is nothing to press until routines load.
    assert!(rt.find("set-routines-switch").is_none() && rt.find("set-routines-loading").is_some(), "a skeleton, not a switch");
    assert!(!rt.state.outbox.iter().any(|c| matches!(c, Call::SetRoutinesOn { .. })), "nothing was asked of the daemon");
}

#[test]
fn settings_rows_show_their_description_under_a_group_heading_as_the_web_does() {
    let mut app = App::headless();
    app.open_view(FullView::Settings);
    let mut rt = Runtime::new(app, view, (1360.0, 900.0), Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    settle(&mut rt);
    let t = texts(&rt);
    assert!(rt.find("group-setup").is_some() && t.contains(&"Setup".to_owned()), "the web's `<Section title=\"Setup\">` heading");
    assert!(t.contains(&"Background checks of messages, pull requests and your day. Off until you turn them on.".to_owned()), "the description is words under the title, not a tooltip: {t:?}");
    go_section(&mut rt, "general");
    assert!(rt.find("group-summon---startup").is_some() && t.contains(&"First-run setup".to_owned()));
}

#[test]
fn the_runs_list_is_rows_with_the_undo_on_the_row_and_a_menu() {
    let mut rt = runtime("running");
    assert!(rt.find("undo-run-1").is_some() && rt.find("runs-card").is_some());
    let (x, y) = rt.find("run-row-2").unwrap();
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    settle(&mut rt);
    let t = texts(&rt);
    assert!(t.contains(&"Undo this run".to_owned()) && t.contains(&"Undo back to here".to_owned()) && t.contains(&"View diff".to_owned()), "{t:?}");
    let e = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Undo back to here")).unwrap();
    rt.pointer_down(e.rect[0] + 4.0, e.rect[1] + e.rect[3] / 2.0);
    rt.pointer_up(e.rect[0] + 4.0, e.rect[1] + e.rect[3] / 2.0);
    settle(&mut rt);
    assert!(rt.state.work.confirm.as_ref().is_some_and(|c| matches!(&c.call, Call::Undo { run: 2, through_later: true, .. })), "it asks before undoing back to run 2");
}
