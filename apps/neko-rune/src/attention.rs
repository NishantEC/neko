//! What needs the person, counted the way the web counts it (`shell/status.ts` `deriveStatus`, shown in the title by
//! `shell/attention.ts` as "(2) Neko"): tasks waiting for approval or failed, results ready to review, and the tool calls
//! and proposed actions of Neko's chat that wait for a yes or no. The Dock badge shows the same number.
//!
//! The daemon's pushed events are acted on here too (`AttentionChanged`, `OpenTab`, `ThemeChanged`, `QuotaChanged`);
//! `WebUpdated` and the rest are ignored (they concern the web UI and the launcher, not this window).

use neko_protocol::workbench::{ChatActionStatus, ChatToolStatus, TaskStatus};

use crate::app::App;
use crate::link::{Call, Pushed, Summary};

/// A tool call or an action of Neko's that waits for a yes or no (the web's `Decision`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    /// The message that holds it.
    pub message_id: String,
    pub kind: DecisionKind,
    pub subject: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecisionKind {
    Tool { call_id: String },
    Action { action_id: String },
}

/// Newest first, as the web's `pendingDecisions` lists them.
pub fn pending_decisions(summary: &Summary) -> Vec<Decision> {
    let mut out = Vec::new();
    for message in summary.conversation.iter().rev() {
        for call in message.tool_calls.iter().filter(|c| c.status == ChatToolStatus::AwaitingApproval) {
            out.push(Decision { message_id: message.id.clone(), kind: DecisionKind::Tool { call_id: call.id.clone() }, subject: call.tool_name.clone() });
        }
        for action in message.actions.iter().filter(|a| a.status == ChatActionStatus::Proposed) {
            let subject = summary.task(&action.task_id).map_or_else(|| action.task_id.clone(), |t| t.title.clone());
            out.push(Decision { message_id: message.id.clone(), kind: DecisionKind::Action { action_id: action.id.clone() }, subject });
        }
    }
    out
}

/// The web's `needsUser`: tasks in the Needs you and Review stages, plus what waits for a yes or no.
pub fn needs_user(summary: &Summary) -> usize {
    let tasks = summary.tasks.iter().filter(|t| matches!(t.status, TaskStatus::AwaitingApproval | TaskStatus::Failed | TaskStatus::ReadyForReview)).count();
    tasks + pending_decisions(summary).len()
}

impl App {
    /// The number the window title would carry, and the Dock badge does.
    pub fn attention_count(&self) -> usize {
        self.summary.as_ref().map_or(0, needs_user)
    }

    /// Once a frame: puts the Dock badge right when the count changed. True never (nothing on screen depends on it).
    pub fn tick_dock(&mut self) -> bool {
        let now = self.attention_count();
        if self.dock_shown != Some(now) {
            self.dock_shown = Some(now);
            (self.dock)(now);
        }
        false
    }

    /// Approves the decision in front: the newest one, as ⌘↵ does outside the composer (the web's `approveFirstDecision`).
    pub fn approve_first_decision(&mut self) {
        let Some(first) = self.summary.as_ref().and_then(|m| pending_decisions(m).into_iter().next()) else {
            self.show_toast("Nothing is waiting for approval.");
            return;
        };
        self.decide(&first, true);
    }

    /// Esc with the ask box empty declines the one in front.
    pub fn decline_first_decision(&mut self) -> bool {
        let Some(first) = self.summary.as_ref().and_then(|m| pending_decisions(m).into_iter().next()) else { return false };
        self.decide(&first, false);
        true
    }

    fn decide(&mut self, d: &Decision, approve: bool) {
        self.call(match &d.kind {
            DecisionKind::Tool { call_id } => Call::DecideChatTool { turn_id: d.message_id.clone(), call_id: call_id.clone(), approve },
            DecisionKind::Action { action_id } => Call::DecideChatAction { message_id: d.message_id.clone(), action_id: action_id.clone(), accept: approve },
        });
        if approve {
            self.show_toast(&format!("Approved: {}", d.subject));
        }
    }

    /// What the daemon pushed. True when something on screen changed.
    pub fn apply_pushed(&mut self, pushed: Pushed) -> bool {
        match pushed {
            Pushed::Attention { count } => {
                // The counts (the Inbox, what is unseen, the digest) are read again now rather than at the next minute.
                self.daemon_attention = Some(count);
                self.neko.asked_ms = i64::MIN / 2;
                self.after_live();
                true
            }
            Pushed::OpenTab { route } => {
                self.open_route(&route);
                true
            }
            Pushed::Theme { .. } => {
                // The appearance is the daemon's own setting; read it again and the window follows.
                self.call(Call::Appearance);
                false
            }
            Pushed::Quota => {
                self.call(Call::Quota);
                self.view == Some(crate::views::FullView::Usage)
            }
        }
    }

    /// Opens what a route names (the daemon's `OpenTab`): `tasks?task=<id>&view=full`, `artifact?id=<id>[&v=<n>]`, a plugin
    /// page's `<plugin>:<page>`, or a page's own id (`calendar`, `settings`, ...). Anything else opens in the web UI.
    pub fn open_route(&mut self, route: &str) {
        let route = route.trim_start_matches("#/").trim_start_matches('/');
        let (path, query) = route.split_once('?').unwrap_or((route, ""));
        let param = |name: &str| query.split('&').find_map(|p| p.strip_prefix(name).and_then(|v| v.strip_prefix('='))).map(str::to_owned);
        match path {
            "tasks" => match param("task") {
                Some(id) if self.summary.as_ref().is_some_and(|m| m.task(&id).is_some()) => self.select(&id),
                Some(_) | None => self.open_all_sessions(),
            },
            "artifact" => {
                if let Some(id) = param("id") {
                    let version = param("v").and_then(|v| v.parse().ok());
                    self.show_artifact(&id, version);
                }
            }
            // The web's `#/setup` is the first-run setup; `setup/<section>` is Rune's Setup page at that section (`setup/tools`).
            "setup" => self.open_onboarding(crate::onboarding::Step::Welcome),
            p if p.starts_with("setup/") => match crate::view::setup::SetupSection::from_key(&p["setup/".len()..]) {
                Some(section) => self.open_setup(section),
                None => self.open_view(crate::views::FullView::Setup),
            },
            "settings" => {
                self.open_view(crate::views::FullView::Settings);
                if let Some(section) = param("section").and_then(|k| crate::pages::Section::ALL.into_iter().find(|s| s.key() == k)) {
                    self.go_section(section);
                }
            }
            "pull-requests" => self.open_view(crate::views::FullView::PullRequests),
            _ => {
                if let Some(page) = self.plugin_pages.as_ref().and_then(|pages| pages.iter().find(|p| p.id == path)).cloned() {
                    self.open_plugin_page(page);
                } else if let Some(view) = crate::views::FullView::ALL.into_iter().find(|v| v.web_id() == path || (path == "tools" && *v == crate::views::FullView::Plugins)) {
                    self.open_view(view);
                } else if path == "home" || path.is_empty() {
                    self.open_new();
                } else {
                    self.open_in_web(Some(route));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::{TaskRow, Wait};
    use neko_protocol::workbench::TaskStatus::*;

    fn row(id: &str, status: TaskStatus) -> TaskRow {
        TaskRow { id: id.into(), key: None, title: id.into(), status, updated_at_ms: 0, goal: String::new(), wait: Wait::None, progress: None }
    }

    fn message(json: serde_json::Value) -> neko_protocol::workbench::ChatMessage {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn the_count_is_the_webs_needs_user_tasks_that_wait_plus_decisions() {
        let mut summary = Summary { tasks: vec![row("a", AwaitingApproval), row("b", Failed), row("c", ReadyForReview), row("d", Building), row("e", Completed), row("f", Queued)], ..Summary::default() };
        assert_eq!(needs_user(&summary), 3, "approval, failed and review count; running, done and queued do not");
        summary.conversation.push(message(serde_json::json!({
            "id": "m1", "at_ms": 1, "role": "neko", "started_ms": 1,
            "tool_calls": [
                {"id": "c1", "workspace_id": "w", "connection_id": "x", "tool_name": "send_email", "arguments_json": "{}", "status": "awaiting_approval"},
                {"id": "c2", "workspace_id": "w", "connection_id": "x", "tool_name": "read", "arguments_json": "{}", "status": "succeeded"}
            ],
            "actions": [{"id": "a1", "task_id": "a", "kind": "message", "text": "hi", "status": "Proposed"}, {"id": "a2", "task_id": "a", "kind": "message", "text": "hi", "status": "Done"}],
        })));
        assert_eq!(pending_decisions(&summary).len(), 2, "one tool call and one proposed action wait");
        assert_eq!(needs_user(&summary), 5);
    }

    #[test]
    fn the_dock_is_told_once_per_change() {
        thread_local! { static SEEN: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) }; }
        let mut app = App::headless();
        app.dock = |n| SEEN.with(|s| s.borrow_mut().push(n));
        app.summary = Some(Summary { tasks: vec![row("a", AwaitingApproval)], ..Summary::default() });
        app.tick_dock();
        app.tick_dock();
        app.summary = Some(Summary { tasks: vec![row("a", AwaitingApproval), row("b", Failed)], ..Summary::default() });
        app.tick_dock();
        app.summary = Some(Summary::default());
        app.tick_dock();
        assert_eq!(SEEN.with(|s| s.borrow().clone()), vec![1, 2, 0], "set, changed, cleared; never repeated");
    }

    #[test]
    fn open_tab_routes_to_the_matching_view() {
        let mut app = App::headless();
        app.summary = Some(Summary { tasks: vec![row("t1", Building)], ..Summary::default() });
        app.apply_pushed(Pushed::OpenTab { route: "tasks?task=t1&view=full".into() });
        assert_eq!(app.selected.as_deref(), Some("t1"));
        app.apply_pushed(Pushed::OpenTab { route: "calendar".into() });
        assert_eq!(app.view, Some(crate::views::FullView::Calendar));
        app.apply_pushed(Pushed::OpenTab { route: "settings".into() });
        assert_eq!(app.view, Some(crate::views::FullView::Settings));
        app.apply_pushed(Pushed::OpenTab { route: "tasks".into() });
        assert_eq!(app.view, Some(crate::views::FullView::AllSessions));
        app.plugin_pages = Some(vec![neko_protocol::PluginPage { plugin: "p".into(), id: "p:board".into(), title: "Board".into() }]);
        app.apply_pushed(Pushed::OpenTab { route: "p:board".into() });
        assert_eq!(app.plugin_view.as_ref().map(|p| p.id.as_str()), Some("p:board"));
    }

    #[test]
    fn events_refresh_what_they_name_and_web_updates_are_ignored() {
        use neko_protocol::Event;
        let mut app = App::headless();
        app.summary = Some(Summary::default());
        assert!(app.apply_pushed(crate::link::pushed(&Event::AttentionChanged { count: 4 }).unwrap()));
        assert_eq!(app.daemon_attention, Some(4));
        assert!(app.neko.asked_ms > i64::MIN / 2 || app.outbox.contains(&Call::Inbox), "the Inbox and the unseen marks are asked for again now");
        app.outbox.clear();
        app.apply_pushed(crate::link::pushed(&Event::ThemeChanged { id: "ember".into() }).unwrap());
        assert_eq!(app.outbox, vec![Call::Appearance], "the theme is read again and the window follows");
        app.outbox.clear();
        app.apply_pushed(crate::link::pushed(&Event::QuotaChanged { quotas: Vec::new() }).unwrap());
        assert_eq!(app.outbox, vec![Call::Quota], "Usage is refreshed");
        assert!(crate::link::pushed(&Event::WebUpdated { commit: "abc".into() }).is_none(), "WebUpdated is ignored");
        assert!(crate::link::pushed(&Event::IconsUpdated).is_none());
    }
}
