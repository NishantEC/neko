//! The first-run setup (`web/src/onboarding/Onboarding.tsx` and `steps.tsx`): six steps, calm and skippable, shown in the
//! session region in place of Home while the daemon says setup is not complete (`GetOnboardingState`), and again when
//! Settings, General asks for it. Every choice is saved by its own switch (the same requests the web sends), so leaving halfway
//! keeps what was chosen. "Skip setup" marks it complete from any step; finishing never shows it again.
//!
//!   welcome -> agents -> workspaces -> routines -> mac -> done

use crate::app::App;
use crate::link::{Call, ExtraCall};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Welcome,
    Agents,
    Workspaces,
    Routines,
    Mac,
    Done,
}

impl Step {
    pub const ALL: [Step; 6] = [Step::Welcome, Step::Agents, Step::Workspaces, Step::Routines, Step::Mac, Step::Done];

    pub fn label(self) -> &'static str {
        match self {
            Step::Welcome => "Welcome",
            Step::Agents => "Agents",
            Step::Workspaces => "Workspaces",
            Step::Routines => "Routines",
            Step::Mac => "This Mac",
            Step::Done => "Done",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Step::Welcome => "welcome",
            Step::Agents => "agents",
            Step::Workspaces => "workspaces",
            Step::Routines => "routines",
            Step::Mac => "mac",
            Step::Done => "done",
        }
    }

    pub fn from_key(key: &str) -> Option<Step> {
        Step::ALL.into_iter().find(|s| s.key() == key)
    }

    fn index(self) -> usize {
        Step::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// What to type to install or sign in to each agent CLI (the web's `AGENT_CLIS`).
pub const AGENT_CLIS: [(&str, &str, &str, &str); 3] = [
    ("claude", "Claude Code", "npm install -g @anthropic-ai/claude-code", "claude /login"),
    ("codex", "Codex", "npm install -g @openai/codex", "codex login"),
    ("opencode", "OpenCode", "npm install -g opencode-ai", "opencode auth login"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentState {
    Ready,
    SignIn,
    Missing,
    Error,
}

/// One agent CLI's state from the model catalog's `status` (the web's `agentState`).
pub fn agent_state(source: Option<&neko_protocol::agent_models::ModelSource>) -> AgentState {
    use neko_protocol::agent_models::SourceStatus;
    match source.map(|s| s.status) {
        Some(SourceStatus::Ready) => AgentState::Ready,
        Some(SourceStatus::SignInRequired) => AgentState::SignIn,
        Some(SourceStatus::NotInstalled) => AgentState::Missing,
        Some(_) => AgentState::Error,
        None => AgentState::Missing,
    }
}

#[derive(Default)]
pub struct Onboarding {
    /// The daemon's answer: `Some(false)` is the first run.
    pub completed: Option<bool>,
    pub open: bool,
    /// Asked for again (Settings), not the first run: Esc from the first step closes it.
    pub manual: bool,
    pub step: usize,
    pub busy: bool,
    pub error: Option<String>,
    /// The agents were asked for again ("Check again").
    pub checking: bool,
    /// The path being typed for a workspace.
    pub adding: Option<String>,
}

impl Onboarding {
    pub fn current(&self) -> Step {
        Step::ALL[self.step.min(Step::ALL.len() - 1)]
    }

    pub fn last(&self) -> bool {
        self.step == Step::ALL.len() - 1
    }
}

impl App {
    /// The first-run setup is what the session region shows.
    pub fn onboarding_shown(&self) -> bool {
        self.onboarding.open
    }

    /// Opens it at a step (Settings, General: "Show again"; the web's `openOnboarding`).
    pub fn open_onboarding(&mut self, at: Step) {
        self.onboarding.step = at.index();
        self.onboarding.manual = true;
        self.onboarding.error = None;
        self.onboarding.open = true;
        // Home's place: no session, no page.
        self.selected = None;
        self.wanted.set(None);
        self.new_form = None;
        self.view = None;
        self.plugin_view = None;
        self.onboarding_load();
    }

    /// The data the steps show, asked for when the setup opens.
    fn onboarding_load(&mut self) {
        self.call(Call::Models);
        self.call(Call::RoutinesFull);
        self.call(Call::PhoneSettings);
        self.ask_calendar();
        self.extra_call(ExtraCall::Clipboard(None));
    }

    /// `GetOnboardingState` answered. False: the first run. A completed setup closes it.
    pub fn onboarding_state(&mut self, completed: bool) -> bool {
        self.onboarding.completed = Some(completed);
        self.onboarding.busy = false;
        if completed {
            let was = std::mem::replace(&mut self.onboarding.open, false);
            self.onboarding.manual = false;
            self.onboarding.step = 0;
            self.onboarding.error = None;
            return was;
        }
        if !self.onboarding.open {
            self.onboarding.open = true;
            self.onboarding.manual = false;
            self.onboarding.step = 0;
            self.onboarding_load();
        }
        true
    }

    pub fn onboarding_failed(&mut self, _key: String, message: &str) -> bool {
        self.onboarding.busy = false;
        // Reading the state at start-up fails quietly (the web ignores it); finishing says why.
        if self.onboarding.open {
            self.onboarding.error = Some(message.to_owned());
        }
        true
    }

    pub fn onboarding_next(&mut self) {
        if self.onboarding.busy {
            return;
        }
        if self.onboarding.last() {
            self.onboarding_complete();
            self.show_toast("Neko is ready.");
        } else {
            self.onboarding.step += 1;
            self.onboarding.error = None;
        }
    }

    pub fn onboarding_back(&mut self) {
        self.onboarding.error = None;
        self.onboarding.step = self.onboarding.step.saturating_sub(1);
    }

    /// Esc: a step back; from the first step only a re-opened setup closes (the first run is left with "Skip setup").
    pub fn onboarding_escape(&mut self) {
        if self.onboarding.step > 0 {
            self.onboarding_back();
        } else if self.onboarding.manual {
            self.onboarding.open = false;
            self.onboarding.manual = false;
        }
    }

    /// "Skip setup" and "Open Neko": the daemon marks it complete; its answer closes the setup.
    pub fn onboarding_complete(&mut self) {
        self.onboarding.busy = true;
        self.onboarding.error = None;
        self.extra_call(ExtraCall::OnboardingDone);
    }

    /// "Check again" on the agents step: the catalog is read again from the sources.
    pub fn onboarding_check_agents(&mut self) {
        self.onboarding.checking = true;
        self.call(Call::ModelsRefresh);
    }

    /// "Add" on a scope found on disk, or the typed folder: a workspace of that folder (the web's `add`).
    pub fn onboarding_add_workspace(&mut self, folder: &str, name: Option<&str>) {
        let clean = folder.trim().trim_end_matches('/').to_owned();
        if !clean.starts_with('/') {
            self.onboarding.error = Some("Type the full path of a folder, starting with /, for example /Users/you/Projects/app.".to_owned());
            return;
        }
        self.onboarding.error = None;
        let name = name.map(str::trim).filter(|n| !n.is_empty()).map(str::to_owned).or_else(|| clean.rsplit('/').next().filter(|n| !n.is_empty()).map(str::to_owned)).unwrap_or_else(|| "Workspace".to_owned());
        let workspace = neko_protocol::workbench::Workspace { id: String::new(), name, repository: clean.clone(), instructions: String::new(), away_enabled: false };
        self.pages.set_field("onb-path", "");
        self.call(Call::Setup(Box::new(crate::link::SetupAction::SaveWorkspace { workspace, folders: vec![clean] })));
        self.call(Call::RoutinesFull);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_six_steps_in_the_webs_order() {
        assert_eq!(Step::ALL.map(Step::label), ["Welcome", "Agents", "Workspaces", "Routines", "This Mac", "Done"]);
        assert_eq!(Step::from_key("mac"), Some(Step::Mac));
        assert_eq!(Step::from_key("nope"), None);
    }

    #[test]
    fn the_first_run_opens_it_and_a_completed_answer_closes_it() {
        let mut app = App::headless();
        assert!(!app.onboarding_shown());
        app.onboarding_state(true);
        assert!(!app.onboarding_shown(), "a completed setup is never shown on its own");
        app.onboarding_state(false);
        assert!(app.onboarding_shown() && !app.onboarding.manual && app.onboarding.step == 0);
        app.onboarding_next();
        app.onboarding_next();
        assert_eq!(app.onboarding.current(), Step::Workspaces);
        app.onboarding_state(true);
        assert!(!app.onboarding_shown() && app.onboarding.step == 0);
    }

    #[test]
    fn escape_goes_back_and_only_a_reopened_setup_closes_from_the_first_step() {
        let mut app = App::headless();
        app.onboarding_state(false);
        app.onboarding_next();
        app.onboarding_escape();
        assert_eq!(app.onboarding.current(), Step::Welcome);
        app.onboarding_escape();
        assert!(app.onboarding_shown(), "the first run is left with Skip setup, not Esc");
        let mut again = App::headless();
        again.open_onboarding(Step::Mac);
        assert_eq!(again.onboarding.current(), Step::Mac);
        again.onboarding.step = 0;
        again.onboarding_escape();
        assert!(!again.onboarding_shown());
    }

    #[test]
    fn done_and_skip_both_ask_the_daemon_to_mark_it_complete() {
        let mut app = App::headless();
        app.onboarding_state(false);
        app.onboarding_complete();
        assert!(app.onboarding.busy);
        assert!(app.outbox.iter().any(|c| matches!(c, Call::Extra(x) if **x == ExtraCall::OnboardingDone)));
        // A refusal shows its words and gives the buttons back.
        app.onboarding_failed("onboarding".into(), "The daemon said no.");
        assert!(!app.onboarding.busy && app.onboarding.error.as_deref() == Some("The daemon said no."));
    }

    #[test]
    fn a_workspace_needs_a_full_path_and_takes_the_folders_name() {
        let mut app = App::headless();
        app.onboarding_add_workspace("relative/path", None);
        assert!(app.onboarding.error.as_deref().is_some_and(|e| e.contains("full path")));
        app.outbox.clear();
        app.onboarding_add_workspace("/Users/me/Projects/app/", None);
        let saved = app.outbox.iter().find_map(|c| match c {
            Call::Setup(a) => match &**a {
                crate::link::SetupAction::SaveWorkspace { workspace, folders } => Some((workspace.name.clone(), workspace.repository.clone(), folders.clone())),
                _ => None,
            },
            _ => None,
        });
        assert_eq!(saved, Some(("app".to_owned(), "/Users/me/Projects/app".to_owned(), vec!["/Users/me/Projects/app".to_owned()])));
    }
}
