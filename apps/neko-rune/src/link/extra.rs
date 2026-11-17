//! Parity batch D's requests: the rest of Settings (themes, the summon shortcut, the exact model check, Full power, the
//! clipboard switch, diagnostics, the update source, the quick panel's folders, launch at login), Usage's machine
//! reading, the first-run setup's state, and what Rune reports about itself to the daemon (`ReportApp` and the two
//! artifact reports). Each is a request the web UI sends too, with the same fields (`web/src/pages/setup/settings/*.tsx`,
//! `web/src/onboarding/*.tsx`, `web/src/shell/inspect.ts`), except the ones named in `tests/allow_list.rs`'s `RUNE_EXTRAS`.
//!
//! An [`ExtraCall`] runs on the calling thread and answers with a [`Reply::Extra`]; a call of two requests (diagnostics
//! pings first, then asks for the checks) is one call. A destructive request (start over, remove a theme, roll back
//! the web UI, remove a plugin) is a [`super::Call`] of its own, built only in `task_actions.rs` inside a confirm dialog.

use std::path::Path;

use neko_protocol::agent_models::{DiagnosticCheck, ModelCheck};
use neko_protocol::workbench::{AgentRuntime, MachineHealth};
use neko_protocol::{AppReport, HotkeyCombo, HotkeyConfig, Request, Response};

use super::{ask, LinkError, Reply};

/// One request from the second half of the pages and the reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtraCall {
    /// `ListThemes`: the person's theme files and the folder they live in.
    Themes,
    /// `SetAppearance` with the theme for light and/or for dark (the mode is `Call::SetAppearance`).
    PickTheme { light: Option<String>, dark: Option<String> },
    /// `GetHotkey`: the summon shortcut.
    Hotkey,
    /// `CheckHotkeyConflict`: a side-effect-free look for a known reserved combination.
    HotkeyCheck(HotkeyCombo),
    /// `CommitHotkey`: stores it (the client that owns the global key registers it when it hears `HotkeyChanged`).
    HotkeyCommit(HotkeyCombo),
    /// `CheckAgentModel` with `save: true`: one real prompt through the runner; the runtime is kept only when it works.
    ModelCheck(AgentRuntime),
    /// `SuperAgent`: read (`None`) or set Home chat's full-power mode.
    SuperAgent(Option<bool>),
    /// `GetClipboardHistoryEnabled` (`None`) or `SetClipboardHistoryEnabled`.
    Clipboard(Option<bool>),
    /// `Ping` (timed), then `Diagnostics`.
    Diagnostics,
    /// `MachineHealth`: the Mac's reading (load, memory, disk, and the hold or cap on new work).
    Machine,
    /// `SetUpdateSource`: `owner/name` or a GitHub address; empty is the project's own repository.
    UpdateSource(String),
    /// The quick panel's folders (`Search` of the `folder-scope` provider), and one change (`Activate`).
    Folders,
    FolderChange { id: String, action: Option<String> },
    /// The preference rows (`Search` of the `preference` provider: launch at login) and one toggle (`Activate`).
    Preferences,
    PreferenceToggle { id: String },
    /// `GetOnboardingState` and `SetOnboardingComplete`.
    Onboarding,
    OnboardingDone,
    /// `ReportApp`: what this window shows, for `InspectApp`.
    Report(Box<AppReport>),
    /// `ReportArtifactShown` and `ReportArtifactProblem`.
    ArtifactShown { id: String, version: u32, viewer: String },
    ArtifactProblem { id: String, version: u32, kind: String, message: String, viewer: Option<String> },
}

/// A row of a provider's list (a folder, a preference).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderRow {
    pub id: String,
    pub title: String,
    pub accessory: String,
}

/// What an [`ExtraCall`] answers with.
#[derive(Clone, Debug, PartialEq)]
pub enum ExtraAnswer {
    /// The person's theme files as `(id, file, theme)`, the folder they live in, and the files skipped with why.
    Themes { dir: String, themes: Vec<(String, String, serde_json::Value)>, problems: Vec<(String, String)> },
    Hotkey(HotkeyConfig),
    /// The reason a combination is refused, or none.
    Conflict(Option<String>),
    ModelCheck(ModelCheck),
    SuperAgent(bool),
    Clipboard(bool),
    /// The round trip to the daemon and every check it ran.
    Diagnostics { round_trip_ms: u64, checks: Vec<DiagnosticCheck> },
    Machine(Option<MachineHealth>),
    Rows { kind: &'static str, rows: Vec<ProviderRow> },
    Onboarding { completed: bool },
    /// The appearance after picking a theme.
    Appearance { mode: String, light: String, dark: String },
    /// The daemon took it (a report, a write that answers with the workbench).
    Noted,
}

impl ExtraCall {
    /// Where a refusal shows: beside the control that asked.
    pub fn key(&self) -> String {
        match self {
            ExtraCall::Themes | ExtraCall::PickTheme { .. } => "setting:themes".to_owned(),
            ExtraCall::Hotkey | ExtraCall::HotkeyCheck(_) | ExtraCall::HotkeyCommit(_) => "setting:hotkey".to_owned(),
            ExtraCall::ModelCheck(_) => "setting:model-check".to_owned(),
            ExtraCall::SuperAgent(_) => "setting:super-agent".to_owned(),
            ExtraCall::Clipboard(_) => "setting:clipboard".to_owned(),
            ExtraCall::Diagnostics => "setting:diagnostics".to_owned(),
            ExtraCall::Machine => "page:machine".to_owned(),
            ExtraCall::UpdateSource(_) => "setting:update-source".to_owned(),
            ExtraCall::Folders | ExtraCall::FolderChange { .. } => "setting:folders".to_owned(),
            ExtraCall::Preferences | ExtraCall::PreferenceToggle { .. } => "setting:launch-at-login".to_owned(),
            ExtraCall::Onboarding | ExtraCall::OnboardingDone => "onboarding".to_owned(),
            ExtraCall::Report(_) | ExtraCall::ArtifactShown { .. } | ExtraCall::ArtifactProblem { .. } => "report".to_owned(),
        }
    }

    /// Reports are fire and forget: nothing is said when one fails.
    pub fn quiet(&self) -> bool {
        matches!(self, ExtraCall::Report(_) | ExtraCall::ArtifactShown { .. } | ExtraCall::ArtifactProblem { .. } | ExtraCall::Machine)
    }

    /// The requests, in order. Only the last one's answer is read, except `Diagnostics` (see [`ExtraCall::run`]).
    pub fn wire(&self) -> Vec<Request> {
        use neko_protocol::workbench::Command;
        match self.clone() {
            ExtraCall::Themes => vec![Request::ListThemes],
            ExtraCall::PickTheme { light, dark } => vec![Request::SetAppearance { mode: None, light, dark }],
            ExtraCall::Hotkey => vec![Request::GetHotkey],
            ExtraCall::HotkeyCheck(candidate) => vec![Request::CheckHotkeyConflict { candidate }],
            ExtraCall::HotkeyCommit(candidate) => vec![Request::CommitHotkey { candidate }],
            ExtraCall::ModelCheck(runtime) => vec![Request::CheckAgentModel { runtime, save: true }],
            ExtraCall::SuperAgent(on) => vec![Request::SuperAgent { on }],
            ExtraCall::Clipboard(None) => vec![Request::GetClipboardHistoryEnabled],
            ExtraCall::Clipboard(Some(enabled)) => vec![Request::SetClipboardHistoryEnabled { enabled }],
            ExtraCall::Diagnostics => vec![Request::Ping, Request::Diagnostics],
            ExtraCall::Machine => vec![Request::MachineHealth],
            ExtraCall::UpdateSource(repository) => vec![Request::Workbench(Command::SetUpdateSource { repository })],
            ExtraCall::Folders => vec![provider("folder-scope")],
            ExtraCall::FolderChange { id, action } => vec![Request::Activate { kind: "folder-scope".to_owned(), id, action, query: String::new() }],
            ExtraCall::Preferences => vec![provider("preference")],
            ExtraCall::PreferenceToggle { id } => vec![Request::Activate { kind: "preference".to_owned(), id, action: None, query: String::new() }],
            ExtraCall::Onboarding => vec![Request::GetOnboardingState],
            ExtraCall::OnboardingDone => vec![Request::SetOnboardingComplete { completed: true }],
            ExtraCall::Report(report) => vec![Request::ReportApp { report: *report }],
            ExtraCall::ArtifactShown { id, version, viewer } => vec![Request::ReportArtifactShown { id, version, viewer }],
            ExtraCall::ArtifactProblem { id, version, kind, message, viewer } => vec![Request::ReportArtifactProblem { id, version, kind, message, viewer }],
        }
    }

    /// Sends the requests and reads the answer.
    pub fn run(&self, socket: &Path, open: Option<&str>) -> Reply {
        let mut round_trip_ms = 0;
        let mut answer = None;
        for request in self.wire() {
            let started = std::time::Instant::now();
            match ask(socket, request) {
                Ok(response) => {
                    // The web times only the Ping (`System.tsx`): the first request of a call.
                    if answer.is_none() {
                        round_trip_ms = started.elapsed().as_millis() as u64;
                    }
                    answer = Some(response);
                }
                Err(error) => return Reply::Failed(error),
            }
        }
        let Some(response) = answer else { return Reply::Done };
        // A command answers with the whole workbench (the update source): the app takes it as any write's answer.
        if let Response::Workbench(snapshot) = &response {
            return Reply::Snapshot(super::Summary::from_snapshot_for(snapshot, open));
        }
        match read(self, response, round_trip_ms) {
            Ok(answer) => Reply::Extra(Box::new(answer)),
            Err(Reply::Refused(m)) => Reply::Refused(m),
            Err(other) => other,
        }
    }
}

fn provider(name: &str) -> Request {
    Request::Search { query: String::new(), limit: 200, provider: Some(name.to_owned()) }
}

/// The answer to `call` as data; the `Err` is a refusal or failure to hand on.
fn read(call: &ExtraCall, response: Response, round_trip_ms: u64) -> Result<ExtraAnswer, Reply> {
    Ok(match response {
        Response::Error { message } => return Err(Reply::Refused(message)),
        Response::Themes { dir, themes, problems } => ExtraAnswer::Themes { dir, themes: themes.into_iter().map(|t| (t.id, t.file, t.theme)).collect(), problems: problems.into_iter().map(|p| (p.file, p.message)).collect() },
        Response::Hotkey { config } => ExtraAnswer::Hotkey(config),
        Response::HotkeyConflict { reason } => ExtraAnswer::Conflict(reason),
        Response::AgentModelCheck(check) => ExtraAnswer::ModelCheck(check),
        Response::SuperAgent { on } => ExtraAnswer::SuperAgent(on),
        Response::ClipboardHistoryEnabled { enabled } => ExtraAnswer::Clipboard(enabled),
        Response::Diagnostics(report) => ExtraAnswer::Diagnostics { round_trip_ms, checks: report.checks },
        Response::MachineHealth(health) => ExtraAnswer::Machine(health),
        Response::SearchResults { items, .. } => ExtraAnswer::Rows {
            kind: if matches!(call, ExtraCall::Folders | ExtraCall::FolderChange { .. }) { "folders" } else { "preferences" },
            rows: items.into_iter().map(|i| ProviderRow { id: i.id, title: i.title, accessory: i.accessory.unwrap_or_default() }).collect(),
        },
        Response::OnboardingState { completed, .. } => ExtraAnswer::Onboarding { completed },
        Response::Noted => ExtraAnswer::Noted,
        // A write the daemon answers with the whole workbench or with nothing of its own (the update source, a folder, a toggle):
        // the caller reads the page again.
        Response::Appearance { mode, light, dark, .. } => ExtraAnswer::Appearance { mode, light, dark },
        Response::Workbench(_) | Response::Activated => ExtraAnswer::Noted,
        other => return Err(Reply::Failed(LinkError::Unreadable(format!("unexpected answer: {}", format!("{other:?}").chars().take(60).collect::<String>())))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::Modifier;

    fn json(call: &ExtraCall) -> Vec<serde_json::Value> {
        call.wire().iter().map(|r| serde_json::to_value(r).unwrap()).collect()
    }

    #[test]
    fn the_hotkey_requests_carry_the_candidate_as_the_native_app_sends_it() {
        let combo = HotkeyCombo::new(vec![Modifier::Alt], "Space");
        let check = json(&ExtraCall::HotkeyCheck(combo.clone()));
        assert_eq!(check[0]["CheckHotkeyConflict"]["candidate"]["key"], "Space");
        assert_eq!(check[0]["CheckHotkeyConflict"]["candidate"]["modifiers"], serde_json::json!(["Alt"]));
        assert!(json(&ExtraCall::HotkeyCommit(combo))[0]["CommitHotkey"].is_object());
        assert_eq!(json(&ExtraCall::Hotkey)[0], "GetHotkey");
    }

    #[test]
    fn the_exact_model_check_saves_when_it_works_as_the_web_asks() {
        let runtime = AgentRuntime { provider: "claude".into(), model: "claude-x".into(), reasoning_effort: None, ..AgentRuntime::default() };
        let body = json(&ExtraCall::ModelCheck(runtime));
        assert_eq!(body[0]["CheckAgentModel"]["save"], true);
        assert_eq!(body[0]["CheckAgentModel"]["runtime"]["model"], "claude-x");
    }

    #[test]
    fn full_power_reads_with_null_and_sets_with_a_value() {
        assert!(json(&ExtraCall::SuperAgent(None))[0]["SuperAgent"]["on"].is_null());
        assert_eq!(json(&ExtraCall::SuperAgent(Some(false)))[0]["SuperAgent"]["on"], false);
    }

    #[test]
    fn diagnostics_pings_first_and_the_clipboard_switch_is_the_webs() {
        let body = json(&ExtraCall::Diagnostics);
        assert_eq!((body[0].as_str(), body[1].as_str()), (Some("Ping"), Some("Diagnostics")));
        assert_eq!(json(&ExtraCall::Clipboard(Some(true)))[0]["SetClipboardHistoryEnabled"]["enabled"], true);
        assert_eq!(json(&ExtraCall::Clipboard(None))[0], "GetClipboardHistoryEnabled");
    }

    #[test]
    fn the_quick_panel_and_login_item_go_through_the_providers_as_the_web_does() {
        assert_eq!(json(&ExtraCall::Folders)[0]["Search"]["provider"], "folder-scope");
        let add = json(&ExtraCall::FolderChange { id: "/Users/a/b".into(), action: Some("add".into()) });
        assert_eq!((add[0]["Activate"]["kind"].as_str(), add[0]["Activate"]["action"].as_str()), (Some("folder-scope"), Some("add")));
        let remove = json(&ExtraCall::FolderChange { id: "/Users/a/b".into(), action: None });
        assert!(remove[0]["Activate"]["action"].is_null());
        assert_eq!(json(&ExtraCall::PreferenceToggle { id: "launch-at-login".into() })[0]["Activate"]["kind"], "preference");
    }

    #[test]
    fn onboarding_and_the_update_source_are_the_webs_requests() {
        assert_eq!(json(&ExtraCall::Onboarding)[0], "GetOnboardingState");
        assert_eq!(json(&ExtraCall::OnboardingDone)[0]["SetOnboardingComplete"]["completed"], true);
        assert_eq!(json(&ExtraCall::UpdateSource("a/b".into()))[0]["Workbench"]["SetUpdateSource"]["repository"], "a/b");
        let themes = json(&ExtraCall::PickTheme { light: Some("nord".into()), dark: None });
        assert_eq!(themes[0]["SetAppearance"]["light"], "nord");
        assert!(themes[0]["SetAppearance"]["mode"].is_null());
    }

    #[test]
    fn reports_say_they_are_rune_and_stay_quiet_when_they_fail() {
        let report = AppReport { page_id: "r1".into(), client: "rune".into(), ..AppReport::default() };
        let call = ExtraCall::Report(Box::new(report));
        assert_eq!(json(&call)[0]["ReportApp"]["report"]["client"], "rune");
        assert!(call.quiet() && ExtraCall::Machine.quiet() && !ExtraCall::Themes.quiet());
    }
}
