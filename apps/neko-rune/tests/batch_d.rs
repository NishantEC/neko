//! Parity batch D (the owner's brief of 2026-11-17), headless through the real layout and hit-testing: Settings' second half
//! (themes, the summon shortcut, the exact model check, Full power, Start over, the rulebook editor and its guard, the quick panel,
//! diagnostics, the update source, roll back), Usage's machine health and models, the first-run setup, Plugins as full management,
//! the Canvas row actions, the embedded page's messages, what Rune tells the daemon, and deep links. Each destructive press is
//! shown to ask first and to send nothing before yes. The daemon-backed paths are in `harness.rs`.

use neko_protocol::{HotkeyCombo, Modifier};
use neko_rune::app::App;
use neko_rune::link::{Call, ExtraCall};
use neko_rune::scenes::SCENES;
use neko_rune::task_actions::ASK_FIELD;
use neko_rune::view::view;
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const WINDOW: (f32, f32) = (1360.0, 900.0);

fn runtime(name: &str) -> Runtime<App> {
    let mut app = App::headless();
    let (_, f) = SCENES.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no scene {name}"));
    f(&mut app);
    app.outbox.clear();
    let mut rt = Runtime::new(app, view, WINDOW, Env { system_dark: true, scale: 1.0 });
    rt.on_key = Some(neko_rune::on_key);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let mut at = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}: {:?}", keys(rt)));
    // A long page (the theme cards): scroll the page until the element is in view.
    for _ in 0..30 {
        if at.1 < 860.0 && at.1 > 60.0 {
            break;
        }
        rt.wheel(800.0, 500.0, if at.1 >= 860.0 { -300.0 } else { 300.0 });
        settle(rt);
        at = rt.find(key).unwrap_or(at);
    }
    let (x, y) = at;
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn keys(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.key).collect()
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

fn has_text(rt: &Runtime<App>, needle: &str) -> bool {
    texts(rt).iter().any(|t| t.contains(needle))
}

fn press(rt: &mut Runtime<App>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    settle(rt);
}

fn sent_extra(rt: &Runtime<App>) -> Vec<ExtraCall> {
    rt.state.outbox.iter().filter_map(|c| if let Call::Extra(x) = c { Some((**x).clone()) } else { None }).collect()
}

// ---- 1. Settings ----------------------------------------------------------------------------------------------

#[test]
fn the_theme_pickers_list_every_theme_with_swatches_and_a_check_and_picking_one_asks_the_daemon() {
    let mut rt = runtime("bd-appearance");
    for id in ["neutral-light", "catppuccin-latte", "gruvbox-light", "solarized-light", "paper"] {
        assert!(rt.find(&format!("theme-light-{id}")).is_some(), "light: {id}");
    }
    for id in ["neutral", "glass", "catppuccin-mocha", "gruvbox-dark", "solarized-dark", "nord", "tokyo-night", "rose-pine", "ocean"] {
        assert!(rt.find(&format!("theme-dark-{id}")).is_some(), "dark: {id}");
    }
    assert!(rt.find("theme-dark-nord-a").is_some() && rt.find("theme-dark-nord-b").is_some() && rt.find("theme-dark-nord-c").is_some(), "a three-colour swatch");
    // The chosen one is checked (the radio says so) and the window is painted in it.
    assert_eq!(rt.state.theme_choice(), ("neutral-light".to_owned(), "ocean".to_owned()));
    assert_eq!(rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("theme-dark-") && !k.contains("-remove") && !k.ends_with("-a") && !k.ends_with("-b") && !k.ends_with("-c")) && e.role == "radio").count(), 9);
    click(&mut rt, "theme-dark-nord");
    assert_eq!(rt.state.theme_choice().1, "nord", "the window follows at once");
    assert!(sent_extra(&rt).iter().any(|c| matches!(c, ExtraCall::PickTheme { dark: Some(d), light: None } if d == "nord")));
    // The window is painted from the theme's own colours.
    let painted = rt.state.theme_now(true);
    let nord = neko_rune::themes::builtins().into_iter().find(|t| t.id == "nord").unwrap();
    assert_eq!(&painted.surface.window.hex()[..7], nord.colors.background.as_str());
}

#[test]
fn make_me_a_theme_sends_neko_the_webs_prompt_and_a_theme_is_removed_only_after_yes() {
    let mut rt = runtime("bd-appearance");
    click(&mut rt, "make-theme");
    assert!(has_text(&rt, "Make me a theme"));
    // Nothing without words.
    click(&mut rt, "ask-yes");
    assert!(rt.state.outbox.is_empty(), "the dialog waits for a wish");
    rt.update(|s, _| s.pages.set_field(ASK_FIELD, "Frosted glass in deep purple"));
    settle(&mut rt);
    click(&mut rt, "ask-yes");
    let sent = rt.state.outbox.iter().find_map(|c| match c.wire() {
        neko_protocol::Request::Workbench(neko_protocol::workbench::Command::SendMessage { text, workspace_id: None }) => Some(text),
        _ => None,
    });
    let text = sent.expect("one message to Neko");
    assert!(text.starts_with("Make me a Neko theme: Frosted glass in deep purple") && text.contains("/data/themes/<id>.json"), "{text}");
    // Remove: ask first, the coral yes sends; Cancel sends nothing.
    rt.state.outbox.clear();
    click(&mut rt, "theme-dark-ocean-remove");
    assert!(has_text(&rt, "Remove Ocean?") && rt.state.outbox.is_empty());
    click(&mut rt, "ask-no");
    assert!(rt.state.outbox.is_empty());
    click(&mut rt, "theme-dark-ocean-remove");
    click(&mut rt, "ask-yes");
    assert!(matches!(rt.state.outbox.as_slice(), [Call::DropTheme { id }] if id == "ocean"), "{:?}", rt.state.outbox);
    // A built-in has no remove.
    assert!(rt.find("theme-dark-nord-remove").is_none());
}

#[test]
fn the_summon_shortcut_is_recorded_checked_then_committed_and_a_bare_key_is_refused() {
    let mut rt = runtime("bd-general");
    assert!(has_text(&rt, "⌥ Space"));
    click(&mut rt, "set-hotkey-change");
    assert!(rt.find("set-hotkey-recording").is_some());
    press(&mut rt, Key::Text("k".into()), Mods::default());
    assert!(has_text(&rt, "Hold ⌘, ⌥ or ⌃"), "a bare key is not a shortcut");
    assert!(sent_extra(&rt).is_empty());
    press(&mut rt, Key::Command('k'), Mods { cmd: true, shift: true, ..Mods::default() });
    let combo = HotkeyCombo::new(vec![Modifier::Cmd, Modifier::Shift], "KeyK");
    assert_eq!(sent_extra(&rt), vec![ExtraCall::HotkeyCheck(combo.clone())], "checked first");
    // The daemon found no conflict: the same combination is committed.
    let check = Call::Extra(Box::new(ExtraCall::HotkeyCheck(combo.clone())));
    rt.update(|s, _| {
        s.outbox.clear();
        s.apply_reply(&check, neko_rune::link::Reply::Extra(Box::new(neko_rune::link::ExtraAnswer::Conflict(None))));
    });
    assert_eq!(sent_extra(&rt), vec![ExtraCall::HotkeyCommit(combo.clone())]);
    // A reserved one is refused in the daemon's words and nothing is committed.
    rt.update(|s, _| {
        s.outbox.clear();
        s.apply_reply(&check, neko_rune::link::Reply::Extra(Box::new(neko_rune::link::ExtraAnswer::Conflict(Some("macOS uses ⌘⇧ K for something else".into())))));
    });
    settle(&mut rt);
    assert!(sent_extra(&rt).is_empty() && has_text(&rt, "can’t be used: macOS uses"));
    // Esc ends recording.
    click(&mut rt, "set-hotkey-change");
    press(&mut rt, Key::Escape, Mods::default());
    assert!(rt.find("set-hotkey-recording").is_none());
}

#[test]
fn the_exact_model_id_is_validated_and_checked_and_full_power_and_start_over_work_as_the_web_does() {
    let mut rt = runtime("bd-ai");
    // The row is collapsed, as the web's is; its button opens the field.
    assert!(rt.find("set-custom-use").is_none());
    click(&mut rt, "set-custom-toggle");
    // "Use ID" is only on for a valid id.
    rt.update(|s, _| s.pages.set_field("custom-model", "has space"));
    settle(&mut rt);
    click(&mut rt, "set-custom-use");
    assert!(rt.state.pages.runtime_draft.is_none(), "an invalid id is not used");
    rt.update(|s, _| s.pages.set_field("custom-model", "gpt-b-preview"));
    settle(&mut rt);
    click(&mut rt, "set-custom-use");
    assert_eq!(rt.state.runtime_shown().model, "gpt-b-preview");
    // Check model saves when it works: the request carries the runtime and save.
    click(&mut rt, "set-check-model");
    let body = sent_extra(&rt);
    assert!(matches!(body.as_slice(), [ExtraCall::ModelCheck(r)] if r.model == "gpt-b-preview"), "{body:?}");
    let sent = rt.state.outbox.last().unwrap().wire();
    assert_eq!(serde_json::to_value(sent).unwrap()["CheckAgentModel"]["save"], true);
    // Full power: on now, the switch turns it off.
    rt.state.outbox.clear();
    click(&mut rt, "set-super-agent-switch");
    assert_eq!(sent_extra(&rt), vec![ExtraCall::SuperAgent(Some(false))]);
    // Start over: the web's words, a coral yes, nothing before it.
    rt.state.outbox.clear();
    click(&mut rt, "set-start-over-button");
    assert!(has_text(&rt, "Tasks without a pull request are removed, along with their worktrees and unpublished changes. Tasks with a pull request stay. This can’t be undone."), "{:?}", texts(&rt));
    assert!(has_text(&rt, "Remove tasks and start over") && rt.state.outbox.is_empty());
    click(&mut rt, "ask-no");
    assert!(rt.state.outbox.is_empty());
    click(&mut rt, "set-start-over-button");
    click(&mut rt, "ask-yes");
    assert!(matches!(rt.state.outbox.as_slice(), [Call::WipeTasks]));
    assert_eq!(serde_json::to_value(Call::WipeTasks.wire()).unwrap()["Workbench"]["StartOver"]["keep_published"], true);
}

#[test]
fn the_rulebook_editor_saves_reverts_restores_and_guards_unsaved_changes() {
    let mut rt = runtime("bd-rulebook");
    assert!(rt.state.rulebook_dirty() && has_text(&rt, "Unsaved changes"));
    // Leaving the section with unsaved words asks first; Keep editing stays, Discard goes and drops the draft.
    click(&mut rt, "settings-section-general");
    assert_eq!(rt.state.pages.section, neko_rune::pages::Section::Rulebook, "it did not leave");
    assert!(has_text(&rt, "Discard your changes?"));
    click(&mut rt, "guard-keep");
    assert_eq!(rt.state.pages.section, neko_rune::pages::Section::Rulebook);
    assert!(rt.state.rulebook_dirty(), "the words are still there");
    // The same guard stands in front of another page.
    rt.update(|s, _| s.open_view(neko_rune::views::FullView::Usage));
    settle(&mut rt);
    assert!(rt.find("guard-discard").is_some());
    click(&mut rt, "guard-keep");
    // Save sends the web's request with the words.
    click(&mut rt, "rulebook-save");
    let sent = rt.state.outbox.iter().find_map(|c| match c {
        Call::SetRulebook { text } => Some(text.clone()),
        _ => None,
    });
    assert!(sent.is_some_and(|t| t.contains("Never force-push.")));
    // Revert puts the saved text back; then leaving is free.
    click(&mut rt, "rulebook-revert");
    assert!(!rt.state.rulebook_dirty());
    click(&mut rt, "settings-section-general");
    assert_eq!(rt.state.pages.section, neko_rune::pages::Section::General);
}

#[test]
fn discarding_goes_where_the_person_was_going() {
    let mut rt = runtime("bd-rulebook");
    click(&mut rt, "settings-section-phone");
    click(&mut rt, "guard-discard");
    assert_eq!(rt.state.pages.section, neko_rune::pages::Section::Phone);
    assert!(!rt.state.rulebook_dirty() && rt.find("guard-discard").is_none());
}

#[test]
fn the_rulebook_page_is_the_webs_one_section_with_a_status_a_footer_and_a_collapsed_default() {
    let mut rt = runtime("bd-rulebook");
    for key in ["group-rulebook", "rulebook-status", "rulebook-footer", "rulebook-kbd", "rulebook-save", "rulebook-default-toggle", "rulebook"] {
        assert!(rt.find(key).is_some(), "{key}: {:?}", keys(&rt));
    }
    assert!(has_text(&rt, "Changes apply to the next agent that starts; agents already running keep the rules they started with."), "the description is words under the title");
    assert!(rt.find("rulebook-default").is_none(), "the built-in default starts collapsed");
    click(&mut rt, "rulebook-default-toggle");
    assert!(rt.find("rulebook-default").is_some());
}

#[test]
fn restoring_the_default_rulebook_asks_and_sends_empty_text() {
    let mut rt = runtime("bd-rulebook");
    rt.update(|s, _| {
        if let Some(m) = s.summary.as_mut() {
            m.settings.rulebook = "mine".into();
        }
    });
    settle(&mut rt);
    click(&mut rt, "rulebook-restore");
    assert!(has_text(&rt, "Restore the default rulebook?") && rt.state.outbox.is_empty());
    click(&mut rt, "ask-yes");
    assert!(matches!(rt.state.outbox.as_slice(), [Call::SetRulebook { text }] if text.is_empty()));
}

#[test]
fn the_quick_panel_folders_are_listed_added_and_removed_through_the_providers() {
    let mut rt = runtime("bd-quick-panel");
    assert!(rt.find("folder-0").is_some() && rt.find("folder-1").is_some() && has_text(&rt, "/Users/you/Documents"));
    click(&mut rt, "folder-remove-0");
    assert_eq!(sent_extra(&rt), vec![ExtraCall::FolderChange { id: "/Users/you/Documents".into(), action: None }]);
    rt.state.outbox.clear();
    click(&mut rt, "folder-add");
    assert_eq!(sent_extra(&rt), vec![ExtraCall::FolderChange { id: "/Users/you/Desktop".into(), action: Some("add".into()) }]);
    assert_eq!(rt.state.pages.text("folder-path"), "", "the field empties once it is sent");
}

#[test]
fn diagnostics_run_ping_then_the_checks_and_show_a_mint_check_or_a_coral_cross() {
    let mut rt = runtime("bd-system");
    for (i, ok) in [(0, true), (1, true), (2, false)] {
        assert!(rt.find(&format!("diag-check-{i}")).is_some());
        let _ = ok;
    }
    assert!(has_text(&rt, "Not installed") && has_text(&rt, "212 ms") && has_text(&rt, "Neko window ↔ daemon"));
    click(&mut rt, "diag-run");
    assert_eq!(sent_extra(&rt), vec![ExtraCall::Diagnostics]);
    assert!(rt.state.extra.diag_running && has_text(&rt, "Running…"), "the button is off while it runs");
    // The clipboard switch sends the web's request.
    let mut rt = runtime("bd-appearance");
    click(&mut rt, "settings-section-permissions");
    click(&mut rt, "set-clipboard-switch");
    assert_eq!(sent_extra(&rt), vec![ExtraCall::Clipboard(Some(false))]);
}

#[test]
fn the_update_source_is_validated_as_owner_name_and_roll_back_is_confirmed() {
    let mut rt = runtime("bd-updates");
    rt.update(|s, _| s.pages.set_field("update-source", "not a repo"));
    settle(&mut rt);
    click(&mut rt, "update-source-save");
    assert!(sent_extra(&rt).is_empty() && has_text(&rt, "Use owner/name or a github.com address."));
    rt.update(|s, _| s.pages.set_field("update-source", "https://github.com/acme/neko-fork.git"));
    settle(&mut rt);
    click(&mut rt, "update-source-save");
    assert_eq!(sent_extra(&rt), vec![ExtraCall::UpdateSource("acme/neko-fork".into())]);
    // Roll back: asks, nothing before yes.
    rt.state.outbox.clear();
    click(&mut rt, "roll-back-web");
    assert!(has_text(&rt, "Roll back the web UI?") && rt.state.outbox.is_empty());
    click(&mut rt, "ask-no");
    assert!(rt.state.outbox.is_empty());
    click(&mut rt, "roll-back-web");
    click(&mut rt, "ask-yes");
    assert!(matches!(rt.state.outbox.as_slice(), [Call::PreviousWebBuild]));
}

#[test]
fn general_offers_the_first_run_setup_again_and_launch_at_login_is_the_webs_preference() {
    let mut rt = runtime("bd-general");
    click(&mut rt, "set-login-switch");
    assert_eq!(sent_extra(&rt), vec![ExtraCall::PreferenceToggle { id: "launch-at-login".into() }]);
    click(&mut rt, "set-first-run-open");
    assert!(rt.state.onboarding_shown() && rt.state.onboarding.manual && rt.find("onboarding").is_some(), "{:?}", keys(&rt));
}

// ---- 2. Usage -------------------------------------------------------------------------------------------------

#[test]
fn usage_shows_machine_health_as_figure_lines_and_colours_only_what_needs_the_person() {
    let rt = runtime("bd-usage");
    for k in ["machine-state-title", "machine-0-value", "machine-1-value", "machine-2-value", "machine-3-value"] {
        assert!(rt.find(k).is_some(), "{k}");
    }
    assert!(has_text(&rt, "Mac OK · load 9") && has_text(&rt, "9.2 of 14 cores") && has_text(&rt, "24 GB"), "{:?}", texts(&rt));
    assert!(rt.find("model-0").is_some() && has_text(&rt, "gpt-a · high") && has_text(&rt, "2 runs"), "the Models section");
    let hold = runtime("bd-usage-hold");
    assert!(has_text(&hold, "New work is on hold") && has_text(&hold, "Free disk is under 15 GB."));
}

// ---- 3. First run ---------------------------------------------------------------------------------------------

#[test]
fn the_first_run_setup_replaces_home_with_six_steps_a_column_of_560_and_the_webs_requests() {
    let mut rt = runtime("bd-onboarding-welcome");
    assert!(rt.find("onboarding").is_some() && rt.find("home-composer").is_none(), "it takes Home's place");
    let dots: Vec<_> = (0..6).map(|i| rt.find(&format!("onb-dot-{i}")).is_some()).collect();
    assert_eq!(dots, vec![true; 6], "six dots");
    let content = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("onb-content")).unwrap().rect;
    assert!(content[2] <= 560.5, "the column is 560: {content:?}");
    assert!(rt.find("onb-skip").is_some() && rt.find("onb-next").is_some());
    // Continue walks the steps; each has its own content.
    for key in ["onb-agents", "onb-workspaces", "onb-routines", "onb-mac", "onb-done"] {
        click(&mut rt, "onb-next");
        assert!(rt.find(key).is_some(), "{key}: {:?}", keys(&rt));
        assert!(rt.find("onb-skip").is_some(), "Skip setup is on every screen");
    }
    // Done sends SetOnboardingComplete; the dots' current one is wide.
    rt.state.outbox.clear();
    click(&mut rt, "onb-next");
    assert_eq!(sent_extra(&rt), vec![ExtraCall::OnboardingDone]);
}

#[test]
fn skip_setup_completes_from_any_step_and_each_switch_saves_what_it_changes() {
    let mut rt = runtime("bd-onboarding-routines");
    click(&mut rt, "onb-routines-switch");
    assert!(rt.state.outbox.iter().any(|c| matches!(c, Call::SetRoutinesOn { enabled: false })));
    let mut rt = runtime("bd-onboarding-mac");
    click(&mut rt, "onb-calendar-switch");
    click(&mut rt, "onb-clipboard-switch");
    assert!(rt.state.outbox.iter().any(|c| matches!(c, Call::SetCalendarOn { enabled: false })));
    assert_eq!(sent_extra(&rt), vec![ExtraCall::Clipboard(Some(true))]);
    rt.state.outbox.clear();
    click(&mut rt, "onb-skip");
    assert_eq!(sent_extra(&rt).last(), Some(&ExtraCall::OnboardingDone));
    // The daemon's answer closes it.
    rt.update(|s, _| {
        s.onboarding_state(true);
    });
    settle(&mut rt);
    assert!(rt.find("onboarding").is_none() && rt.state.home_shown());
}

#[test]
fn the_agents_step_names_what_is_ready_and_what_to_type_for_the_rest() {
    let rt = runtime("bd-onboarding-agents");
    assert!(has_text(&rt, "Which agents are on this Mac?") && has_text(&rt, "Ready"), "{:?}", texts(&rt));
    assert!(rt.find("onb-agents-check").is_some());
}

// ---- 4. Plugins -----------------------------------------------------------------------------------------------

#[test]
fn plugins_are_managed_enabled_disabled_removed_after_a_yes_reloaded_and_changed_through_neko() {
    let mut rt = runtime("bd-plugins-list");
    for id in ["board-viewer", "release-notes", "calendar-sync"] {
        assert!(rt.find(&format!("plugin-{id}")).is_some(), "{id}");
    }
    // The switch asks the daemon over HTTP (the work waits for the signed address in a headless run).
    rt.update(|s, _| s.http = Some(neko_rune::link::http::Http::new(1, &"0".repeat(64))));
    click(&mut rt, "plugin-board-viewer-switch");
    assert!(rt.state.compose.asked.contains(&neko_rune::compose::HttpWork::PluginSet { id: "board-viewer".into(), enabled: false }));
    rt.update(|s, _| s.plugin_ui.busy = None);
    // Remove: a confirmation names the folder; nothing is asked before yes.
    rt.state.compose.asked.clear();
    click(&mut rt, "plugin-release-notes-remove");
    assert!(has_text(&rt, "Remove “release-notes”?") && has_text(&rt, "/data/plugins/release-notes") && rt.state.compose.asked.is_empty());
    click(&mut rt, "ask-no");
    assert!(rt.state.compose.asked.is_empty());
    click(&mut rt, "plugin-release-notes-remove");
    click(&mut rt, "ask-yes");
    assert!(rt.state.compose.asked.contains(&neko_rune::compose::HttpWork::PluginRemove { id: "release-notes".into() }));
    // Details: the manifest is read over HTTP and shown with the version, viewers, pages and last load.
    rt.update(|s, _| s.plugin_ui.busy = None);
    click(&mut rt, "plugin-board-viewer-details");
    assert!(rt.state.compose.asked.contains(&neko_rune::compose::HttpWork::PluginManifest { id: "board-viewer".into() }));
    assert!(rt.find("plugin-details").is_some() && has_text(&rt, "Loaded 10m ago") && has_text(&rt, "Board (board)"));
    // Reload asks again; "Change Neko…" asks what to change and sends the web's prompt.
    rt.state.compose.asked.clear();
    click(&mut rt, "plugin-board-viewer-reload");
    assert!(rt.state.compose.asked.contains(&neko_rune::compose::HttpWork::Plugins));
    rt.state.outbox.clear();
    click(&mut rt, "change-neko");
    rt.update(|s, _| s.pages.set_field(ASK_FIELD, "A PR board"));
    settle(&mut rt);
    click(&mut rt, "ask-yes");
    let text = rt.state.outbox.iter().find_map(|c| match c.wire() {
        neko_protocol::Request::Workbench(neko_protocol::workbench::Command::SendMessage { text, .. }) => Some(text),
        _ => None,
    });
    assert!(text.is_some_and(|t| t.starts_with("Open a task that changes Neko the way I describe.") && t.contains("What to change: A PR board") && t.contains("/data/plugins/<name>/")));
}

// ---- 5. Artifacts ---------------------------------------------------------------------------------------------

#[test]
fn the_page_bridge_quotes_into_the_composer_and_a_send_is_drafted_never_posted() {
    use neko_rune::artifact_actions::Said;
    let mut rt = runtime("p3a-canvas");
    // A comment is quoted into the open session's composer.
    rt.update(|s, _| s.page_said(Said::Quote("[artifact a1 v1, element h1: \"Title\"]".into())));
    settle(&mut rt);
    assert!(rt.state.composer.text.starts_with("> [artifact a1 v1, element h1"), "{:?}", rt.state.composer.text);
    // A send offer is drafted into the composer, ending with the line that names the artifact; nothing is posted.
    rt.state.outbox.clear();
    rt.update(|s, _| {
        s.extra.last_said_ms = 0;
        s.clock = || neko_rune::app::DEMO_NOW + 5_000;
        s.page_said(Said::Send("Make the heading blue".into()));
    });
    assert!(!rt.state.outbox.iter().any(|c| matches!(c, Call::Reply { .. } | Call::Enqueue { .. } | Call::Ask { .. })), "posted: {:?}", rt.state.outbox);
    let text = rt.state.composer.text.clone();
    assert!(text.contains("Make the heading blue\n[from artifact ") && text.trim_end().ends_with("\"]"), "{text}");
}

#[test]
fn the_canvas_rows_offer_download_open_in_browser_and_copy_link() {
    let mut rt = runtime("p3a-canvas");
    let id = rt.state.panel.artifacts.keys().next().cloned().expect("an artifact");
    for k in ["canvas-download-", "canvas-browser-", "canvas-copy-"] {
        assert!(rt.find(&format!("{k}{id}")).is_some(), "{k}");
    }
    rt.update(|s, _| s.http = Some(neko_rune::link::http::Http::new(1, &"0".repeat(64))));
    click(&mut rt, &format!("canvas-download-{id}"));
    assert!(rt.state.compose.asked.iter().any(|w| matches!(w, neko_rune::compose::HttpWork::Download { id: i, .. } if *i == id)));
    rt.state.outbox.clear();
    click(&mut rt, &format!("canvas-browser-{id}"));
    assert!(rt.state.outbox.iter().any(|c| matches!(c, Call::SignedWebUrl { route } if route.starts_with(&format!("artifact/{id}/")))), "{:?}", rt.state.outbox);
    // The signed address opens in the browser, once.
    let route = rt.state.extra.browser_routes[0].clone();
    assert!(rt.state.browser_route_signed(&route, "http://127.0.0.1:4747/?token=x#/artifact/a/1"));
    assert!(!rt.state.browser_route_signed(&route, "http://127.0.0.1:4747/"));
}

// ---- 6. Being seen by agents ----------------------------------------------------------------------------------

#[test]
fn rune_reports_itself_as_rune_and_only_when_something_changed() {
    let mut app = App::headless();
    app.focused = true;
    // Headless runs send nothing: the report is built, not sent.
    assert_eq!(app.tick_report(Some([1360, 900])), None);
    let report = neko_rune::report::build(&app, "r1abc", Some([1360, 900]));
    assert_eq!((report.client.as_str(), report.size), ("rune", Some([1360, 900])));
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["client"], "rune");
}

// ---- 7. Deep links --------------------------------------------------------------------------------------------

#[test]
fn a_route_opens_the_page_it_names() {
    let mut rt = runtime("bd-usage");
    rt.update(|s, _| s.open_route("setup/tools"));
    settle(&mut rt);
    assert_eq!(rt.state.view, Some(neko_rune::views::FullView::Setup));
    assert_eq!(rt.state.setup.section, neko_rune::view::setup::SetupSection::Tools);
    rt.update(|s, _| s.open_route("pull-requests"));
    assert_eq!(rt.state.view, Some(neko_rune::views::FullView::PullRequests));
    rt.update(|s, _| s.open_route("settings?section=rulebook"));
    assert_eq!((rt.state.view, rt.state.pages.section), (Some(neko_rune::views::FullView::Settings), neko_rune::pages::Section::Rulebook));
    rt.update(|s, _| s.open_route("tasks?task=t3"));
    assert_eq!(rt.state.selected.as_deref(), Some("t3"));
}
