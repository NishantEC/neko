//! Scenes for parity batch D (the owner's brief of 2026-11-17): the rest of Settings, Usage's machine health and models, the
//! first-run setup, and Plugins as full management. Synthetic data only, like every committed scene; the pictures are
//! `docs/design/rune/batch-d-*.png` (`scripts/snap-batch-d.sh`).

use neko_protocol::agent_models::{DiagnosticCheck, ModelCheck};
use neko_protocol::workbench::{AgentMemory, MachineHealth};
use neko_protocol::{HotkeyCombo, HotkeyConfig, Modifier};
use serde_json::json;

use crate::app::{App, DEMO_NOW};
use crate::link::ProviderRow;
use crate::onboarding::Step;
use crate::pages::Section;
use crate::scenes_pages::{base, plugins, with_settings};
use crate::views::FullView;

fn section(s: &mut App, section: Section) {
    with_settings(s);
    s.pages.section = section;
    s.extra.hotkey = Some(HotkeyConfig { combo: HotkeyCombo::new(vec![Modifier::Alt], "Space"), updated_at_unix_ms: DEMO_NOW });
    s.extra.login = Some(true);
    s.extra.super_agent = Some(true);
    s.extra.clipboard = Some(true);
    s.extra.themes_dir = Some("/data/themes".into());
    s.extra.user_themes = vec![
        ("ocean".into(), "ocean.json".into(), json!({"id":"ocean","name":"Ocean","appearance":"dark","family":"Mine","colors":{"background":"#0b1f2a","foreground":"#d7e9f0","primary":"#3ab0d0"}})),
        ("paper".into(), "paper.json".into(), json!({"id":"paper","name":"Paper","appearance":"light","colors":{"background":"#f7f3ea","foreground":"#3b3a36","primary":"#b5651d","mutedForeground":"#a09d95"}})),
    ];
    s.extra.theme_problems = vec![("broken.json".into(), "colors.background is not a hex colour".into())];
    s.pages.theme_ids = Some(("neutral-light".into(), "ocean".into()));
}

pub fn appearance(s: &mut App) {
    section(s, Section::Appearance);
}

pub fn general(s: &mut App) {
    section(s, Section::General);
    s.extra.hotkey_say = Some(crate::extra::Say { ok: true, words: "Saved. The summon shortcut is now ⌥ Space.".into() });
}

pub fn general_recording(s: &mut App) {
    section(s, Section::General);
    s.hotkey_record();
}

pub fn ai(s: &mut App) {
    section(s, Section::Ai);
    s.extra.model_check = Some(ModelCheck { runtime: s.runtime_shown(), ok: true, unavailable: false, message: "gpt-a answered. It is now the default model.".into() });
    s.pages.set_field("custom-model", "gpt-b-preview");
}

pub fn ai_failed(s: &mut App) {
    section(s, Section::Ai);
    s.extra.model_check = Some(ModelCheck { runtime: s.runtime_shown(), ok: false, unavailable: true, message: "This plan does not include that model. Your previous choice is kept.".into() });
}

pub fn start_over(s: &mut App) {
    section(s, Section::Ai);
    s.ask_start_over();
}

pub fn rulebook(s: &mut App) {
    section(s, Section::Rulebook);
    s.pages.set_field("rulebook", "Be careful with other people's files.\nPrefer small, reviewable changes.\nRun the tests before saying a task is done.\nNever force-push.");
}

pub fn rulebook_guard(s: &mut App) {
    rulebook(s);
    s.go_section(Section::General);
}

pub fn quick_panel(s: &mut App) {
    section(s, Section::QuickPanel);
    s.extra.folders = Some(vec![ProviderRow { id: "/Users/you/Documents".into(), title: "/Users/you/Documents".into(), accessory: String::new() }, ProviderRow { id: "/Users/you/Projects".into(), title: "/Users/you/Projects".into(), accessory: String::new() }]);
    s.pages.set_field("folder-path", "/Users/you/Desktop");
}

pub fn system(s: &mut App) {
    section(s, Section::Diagnostics);
    let check = |name: &str, ok: bool, detail: &str, millis: u64| DiagnosticCheck { name: name.into(), ok, detail: detail.into(), millis };
    s.extra.diag = Some((4, vec![check("Claude Code", true, "claude 2.1 at /usr/local/bin/claude", 212), check("Codex", true, "codex 0.9 at /usr/local/bin/codex", 188), check("OpenCode", false, "Not installed", 0)]));
}

pub fn updates(s: &mut App) {
    section(s, Section::Updates);
    s.pages.set_field("update-source", "acme/neko-fork");
}

pub fn roll_back(s: &mut App) {
    section(s, Section::Updates);
    s.ask_roll_back_web();
}

pub fn remove_theme(s: &mut App) {
    section(s, Section::Appearance);
    s.ask_remove_theme("ocean");
}

pub fn make_theme(s: &mut App) {
    section(s, Section::Appearance);
    s.ask_make_theme();
    s.pages.set_field(crate::task_actions::ASK_FIELD, "A calm light theme in sage green");
}

pub fn usage(s: &mut App) {
    base(s, FullView::Usage);
    if let Some(m) = s.summary.as_mut() {
        m.settings = crate::scenes_pages::settings_view();
    }
    s.pages.routines = Some(crate::scenes_pages::routines_data());
    s.pages.quota = Some(crate::scenes_pages::quota_rows());
    s.extra.machine = Some((
        MachineHealth {
            at_ms: DEMO_NOW - 9_000,
            load: 9.2,
            cores: 14,
            free_disk_gb: 24.0,
            compressed_gb: 3.1,
            swap_used_gb: 2.4,
            swap_total_gb: 6.0,
            pressure: "normal".into(),
            kernel_task_cpu: 4.0,
            slowed: 0,
            agents: vec![AgentMemory { name: "Claude".into(), processes: 3, mb: 1840 }, AgentMemory { name: "Codex".into(), processes: 2, mb: 610 }],
            ..MachineHealth::default()
        },
        DEMO_NOW - 4_000,
    ));
}

pub fn usage_hold(s: &mut App) {
    usage(s);
    if let Some((h, _)) = s.extra.machine.as_mut() {
        h.holding = Some("Free disk is under 15 GB.".into());
        h.free_disk_gb = 11.0;
        h.load = 21.0;
    }
}

fn onboarding(s: &mut App, step: Step) {
    crate::scenes::live_for_pages(s);
    s.now_ms = DEMO_NOW;
    s.selected = None;
    s.view = None;
    s.pages.routines = Some(crate::scenes_pages::routines_data());
    s.pages.phone = Some(crate::scenes_pages::phone_data());
    s.pages.catalog = Some(crate::scenes_pages::catalog());
    s.pages.calendar = Some(crate::scenes_pages::calendar_data(DEMO_NOW, 0, 0));
    s.extra.clipboard = Some(false);
    s.onboarding_state(false);
    s.onboarding.step = Step::ALL.iter().position(|x| *x == step).unwrap_or(0);
}

pub fn onboarding_welcome(s: &mut App) {
    onboarding(s, Step::Welcome);
}
pub fn onboarding_agents(s: &mut App) {
    onboarding(s, Step::Agents);
}
pub fn onboarding_workspaces(s: &mut App) {
    onboarding(s, Step::Workspaces);
}
pub fn onboarding_routines(s: &mut App) {
    onboarding(s, Step::Routines);
}
pub fn onboarding_mac(s: &mut App) {
    onboarding(s, Step::Mac);
}
pub fn onboarding_done(s: &mut App) {
    onboarding(s, Step::Done);
}

pub fn plugins_list(s: &mut App) {
    plugins(s);
}

pub fn plugin_details(s: &mut App) {
    plugins(s);
    s.plugin_ui.detail = Some("board-viewer".into());
    s.plugin_ui.manifest = Some(("board-viewer".into(), "{\n  \"title\": \"board-viewer\",\n  \"description\": \"Draws a board artifact\",\n  \"version\": \"0.4.0\",\n  \"entry\": \"index.js\"\n}".into()));
}

pub fn plugin_remove(s: &mut App) {
    plugins(s);
    s.ask_remove_plugin("release-notes");
}

pub fn change_neko(s: &mut App) {
    plugins(s);
    s.ask_change_neko();
    s.pages.set_field(crate::task_actions::ASK_FIELD, "A page that lists my open pull requests by stage");
}

/// Home painted in a picked theme (Nord) instead of the default.
pub fn themed(s: &mut App) {
    crate::scenes::live_for_pages(s);
    s.account.name = "Ada Lovelace".into();
    s.open_new();
    s.pages.theme_ids = Some(("neutral-light".into(), "nord".into()));
    s.pages.appearance = Some("dark".into());
}
