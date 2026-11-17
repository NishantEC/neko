//! Everything this client asks of the daemon beyond the two snapshot reads in `link/mod.rs`
//! (docs/decisions/0002-rune-for-everything.md, P1). The daemon stays the only writer: each
//! [`Call`] is one request the web UI sends too, with the same fields, so a click here and a
//! click there do the same thing. `tests/allow_list.rs` names exactly the requests this file
//! may build and fails when one is added without a review.
//!
//! * **Reads** (`Call::reads`): the open task's checkpoints and one run's diff, the unseen
//!   marks, the Inbox's items and the morning digest. Fetching changes nothing. P3a adds the
//!   context panel's three: a task's folder, an artifact's stored file and an artifact's record.
//! * **Pages (P2)**: reads for the routines, the calendar, the phone settings, the quota meters and
//!   the plugin status; writes that are controls on the web's Inbox, Settings and Calendar pages
//!   (a routine's switch, Run now and editor, standing orders, the Inbox clean-up, the budgets, the
//!   runtime, the phone channels, the updater's switches and Update now). Nothing destructive.
//! * **Writes**: queue a message for a running agent and take it back, reply to a task, approve,
//!   complete, stop or start a task, a goal's pause, resume, extend, mark done, stop and answer,
//!   undo and redo a run, answer or clear an Inbox item, mark the digest read, mark a task seen
//!   or unread, start a session, pin its runtime, and talk to Neko (send, stop, approve).

use neko_protocol::checkpoints::CheckpointCommand;
use neko_protocol::crew::{CrewCommand, DeliveryMode, QueueKind};
use neko_protocol::goals::GoalCommand;
use neko_protocol::phone::PhoneSettings;
use neko_protocol::routines::Routine;
use neko_protocol::workbench::{AgentRuntime, Command, RuntimePreferences};
use neko_protocol::Request;

/// One request from the app to the daemon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    // Reads.
    /// The whole workbench again (the open task changed, or a write answered without one).
    Refresh,
    Checkpoints { task_id: String },
    CheckpointDiff { task_id: String, run: Option<u32> },
    /// The folder a session works in, for the context panel's Files tab (P3a).
    TaskFolder { task_id: String },
    /// Where one version of an artifact is stored (the newest for `None`), for the Canvas tab.
    ArtifactFile { id: String, version: Option<u32> },
    /// One artifact's record: its title, kind and versions.
    ArtifactInfo { id: String },
    /// The web UI's address, signed in, at a route (for the web panel's view).
    SignedWebUrl { route: String },
    /// The viewers the plugins' manifests declare, and the pages the plugins registered.
    PluginViewers,
    PluginPages,
    /// The artifacts a session's own runs saved (the daemon links them to the ticket), newest first.
    TaskArtifacts { task_id: String },
    Unseen,
    Digest,
    Inbox,
    /// The runtimes and models "+ New" offers.
    Models,
    /// The same list, read again from the sources (the popover's refresh button).
    ModelsRefresh,
    // Messages to a task.
    /// A message to a task that is in a turn: the durable queue, with its delivery mode. A retry
    /// of the same `command_id` is one message.
    Enqueue { task_id: String, command_id: String, text: String, mode: DeliveryMode },
    CancelQueued { task_id: String, id: String },
    /// A reply to a task between turns.
    Reply { task_id: String, text: String },
    // What the person decides about a task.
    Approve { task_id: String },
    Complete { task_id: String },
    Stop { task_id: String },
    Start { task_id: String },
    /// "Propose parallel work" (the web's `ProposeSplit`: a read-only proposal, approval is separate).
    ProposeSplit { task_id: String },
    // A goal.
    GoalPause { id: String },
    GoalResume { id: String },
    GoalExtend { id: String, turns: u32 },
    GoalDone { id: String },
    GoalStop { id: String },
    /// Answers a blocked goal's one question; empty text takes its default.
    GoalAnswer { id: String, text: String },
    /// A goal on an existing task, with the check that proves it done.
    GoalCreate { workspace_id: String, objective: String, done_check: String, ticket_id: String },
    // Runs.
    Undo { task_id: String, run: u32, through_later: bool, snapshot_unsaved: bool, revert: bool },
    Redo { task_id: String },
    // The Inbox and the digest.
    AnswerNeedsYou { seq: u64 },
    /// "Mark done" on an Inbox item (the web's `ClearNeedsYou`): the item closes as "Done by you".
    ClearNeedsYou { routine_id: String, run_id: String, seq: u64 },
    /// "Reopen" on an item under "Closed recently".
    ReopenNeedsYou { routine_id: String, run_id: String, seq: u64 },
    MarkDigestRead,
    // Seen and unread.
    MarkSeen { task_id: String, seen: bool },
    // A new session.
    CreateTask { workspace_id: String, title: String, goal: String },
    SetRuntime { task_id: String, provider: String, model: String },
    /// The model chip's "Check & apply": every pin of one conversation (`task:<id>`, `home:<profile>:<workspace>`), as the web sends them.
    SetRuntimePrefs { conversation_id: String, prefs: RuntimePreferences },
    // What the web's task menu does (`work/taskActions.ts`).
    /// A name the person gives it; empty lets Neko name it again.
    Rename { task_id: String, title: String },
    /// Commit, push and open a pull request for a reviewed result.
    Publish { task_id: String },
    /// Moves a queued task to the front of the queue.
    RunNext { task_id: String },
    /// How reviewed tasks in one repository are delivered: landed on local main, or as a pull request.
    SetLandOnMain { repository: String, enabled: bool },
    /// Retry from the beginning (a failed or cancelled task). Only built inside a confirm dialog.
    Retry { task_id: String },
    /// Remove one finished task and its subtasks from history. Only built inside a confirm dialog.
    Delete { task_id: String },
    /// Stop everything at once: every unfinished ticket and Neko's reply. Only built inside a confirm dialog.
    StopAll,
    /// Remove every completed or cancelled task, in one workspace or all. Only built inside a confirm dialog.
    ClearFinished { workspace_id: Option<String> },
    // Neko's own conversation.
    Ask { text: String, workspace_id: Option<String> },
    /// Stops the reply and sends this first (`InterruptAndSendMessage`, the web's ⌘↵).
    AskNow { text: String, workspace_id: Option<String> },
    /// Takes back a message that waits behind a reply.
    DiscardQueued { message_id: String },
    /// "Send now" on a queued message: it leaves the queue, then the reply stops and it goes first.
    SteerQueued { message_id: String, text: String, workspace_id: Option<String> },
    /// An edited queued message: the old one leaves the queue and the new words queue behind the reply.
    EditQueued { message_id: String, text: String, workspace_id: Option<String> },
    StopAsking { turn_id: String },
    DecideChatTool { turn_id: String, call_id: String, approve: bool },
    DecideChatAction { message_id: String, action_id: String, accept: bool },
    // The pages (P2). Reads first.
    /// The routines whole, for the Routines page, standing orders and the Inbox settings.
    RoutinesFull,
    CalendarEvents { from_ms: i64, to_ms: i64 },
    PhoneSettings,
    /// The web UI's appearance (System, Light or Dark), kept by the daemon so every client agrees.
    Appearance,
    /// The vendors' quota meters (the web's Usage page asks the `usage` provider).
    Quota,
    Plugins,
    // Routines, each a control on the web's Inbox page.
    SaveRoutine(Box<Routine>),
    RunRoutine { id: String },
    SetRoutineEnabled { id: String, enabled: bool },
    SetRoutinesOn { enabled: bool },
    SetStandingOrders { workspace_id: String, text: String },
    // Settings, each a control the web's Settings page has.
    SetInboxAgeOut { days: u32 },
    SetCalendarOn { enabled: bool },
    SavePhone(Box<PhoneSettings>),
    PhoneTest,
    SetAgentRuntime(AgentRuntime),
    SetTaskBudget { cents: Option<u32> },
    SetDailyBudget { cents: Option<u32> },
    SetMaxRunning { limit: Option<u32> },
    SetMaxWritable { limit: Option<u32> },
    SetPlanFirst { enabled: bool },
    SetStartWithoutApproval { enabled: bool },
    SetSelfUpdate { auto_ui: bool, auto_native: bool },
    UpdateNow,
    ApplyUpdate,
    SetDevCheckout { path: String },
    /// Saves the mode (`system`, `light`, `dark`): what the web's Appearance page sets.
    SetAppearance { mode: String },
    /// One request of the Setup pages (Workspaces, Watching, Tools, Skills, Memory, Profiles, Schedules): `link/setup.rs`.
    Setup(Box<super::SetupAction>),
}

impl Call {
    /// The request that follows `wire()` when a call is two steps (the old message leaves the queue, then the new one goes).
    pub fn then(&self) -> Option<Request> {
        match self {
            Call::SteerQueued { text, workspace_id, .. } => Some(Request::Workbench(Command::InterruptAndSendMessage { text: text.clone(), workspace_id: workspace_id.clone() })),
            // An edited message goes first and the old one is taken back second: if the second step fails the person has the words twice, never none.
            Call::EditQueued { message_id, .. } => Some(Request::Workbench(Command::DiscardQueuedMessage { message_id: message_id.clone() })),
            _ => None,
        }
    }

    /// Whether this only fetches.
    pub fn reads(&self) -> bool {
        matches!(self, Call::Refresh | Call::Checkpoints { .. } | Call::CheckpointDiff { .. } | Call::TaskFolder { .. } | Call::ArtifactFile { .. } | Call::ArtifactInfo { .. } | Call::TaskArtifacts { .. } | Call::SignedWebUrl { .. } | Call::PluginViewers | Call::PluginPages | Call::Unseen | Call::Digest | Call::Inbox | Call::Models | Call::ModelsRefresh | Call::RoutinesFull | Call::CalendarEvents { .. } | Call::PhoneSettings | Call::Appearance | Call::Quota | Call::Plugins)
    }

    /// The request the daemon receives: the same JSON the web UI posts.
    pub fn wire(&self) -> Request {
        match self.clone() {
            Call::Refresh => Request::Workbench(Command::Snapshot),
            Call::Checkpoints { task_id } => Request::TaskCheckpoints { task_id },
            Call::CheckpointDiff { task_id, run } => Request::CheckpointDiff { task_id, run },
            Call::TaskFolder { task_id } => Request::TaskFolder { task_id },
            Call::ArtifactFile { id, version } => Request::ArtifactFile { id, version },
            Call::ArtifactInfo { id } => Request::Artifact { id },
            Call::TaskArtifacts { task_id } => Request::ArtifactsForTask { task_id },
            Call::SignedWebUrl { route } => Request::SignedWebUrl { route },
            Call::PluginViewers => Request::PluginViewers,
            Call::PluginPages => Request::PluginPages,
            Call::Unseen => Request::UnseenTasks,
            Call::Digest => Request::MorningDigest,
            Call::Inbox => Request::Routines,
            Call::Models => Request::AgentModels { refresh: false },
            Call::ModelsRefresh => Request::AgentModels { refresh: true },
            Call::Enqueue { task_id, command_id, text, mode } => {
                // The kind the web sends with each mode (`work/queue.tsx`).
                let kind = match mode {
                    DeliveryMode::InterruptRestart => QueueKind::Interrupt,
                    DeliveryMode::SteerNow => QueueKind::Steer,
                    DeliveryMode::AfterTurn => QueueKind::Send,
                };
                Request::Workbench(Command::Crew(CrewCommand::Enqueue { task_id, command_id: Some(command_id), text, mode, kind }))
            }
            Call::CancelQueued { task_id, id } => Request::Workbench(Command::Crew(CrewCommand::CancelQueued { task_id, id })),
            Call::Reply { task_id, text } => Request::Workbench(Command::ReplyToTask { task_id, text }),
            Call::Approve { task_id } => Request::Workbench(Command::ApproveTask { task_id }),
            Call::Complete { task_id } => Request::Workbench(Command::CompleteTask { task_id }),
            Call::Stop { task_id } => Request::Workbench(Command::CancelTask { task_id }),
            Call::Start { task_id } => Request::Workbench(Command::StartTask { task_id }),
            Call::ProposeSplit { task_id } => Request::Workbench(Command::ProposeSplit { task_id }),
            Call::GoalPause { id } => Request::Workbench(Command::Goals(GoalCommand::Pause { id })),
            Call::GoalResume { id } => Request::Workbench(Command::Goals(GoalCommand::Resume { id })),
            Call::GoalExtend { id, turns } => Request::Workbench(Command::Goals(GoalCommand::Extend { id, turns })),
            Call::GoalDone { id } => Request::Workbench(Command::Goals(GoalCommand::MarkDone { id })),
            Call::GoalStop { id } => Request::Workbench(Command::Goals(GoalCommand::Stop { id })),
            Call::GoalAnswer { id, text } => Request::Workbench(Command::Goals(GoalCommand::Answer { id, text })),
            Call::GoalCreate { workspace_id, objective, done_check, ticket_id } => Request::Workbench(Command::Goals(GoalCommand::Create {
                workspace_id,
                objective,
                done_check,
                constraints: String::new(),
                max_turns: None,
                ticket_id: Some(ticket_id),
                conversation_id: None,
            })),
            Call::Undo { task_id, run, through_later, snapshot_unsaved, revert } => {
                Request::Workbench(Command::Checkpoints(CheckpointCommand::Undo { task_id, run, through_later, snapshot_unsaved, revert }))
            }
            Call::Redo { task_id } => Request::Workbench(Command::Checkpoints(CheckpointCommand::Redo { task_id })),
            Call::AnswerNeedsYou { seq } => Request::AnswerNeedsYou { seq, text: None },
            Call::ClearNeedsYou { routine_id, run_id, seq } => Request::ClearNeedsYou { routine_id, run_id, seq },
            Call::ReopenNeedsYou { routine_id, run_id, seq } => Request::ReopenNeedsYou { routine_id, run_id, seq },
            Call::MarkDigestRead => Request::MarkDigestRead,
            Call::MarkSeen { task_id, seen } => Request::MarkTaskSeen { task_id, seen },
            Call::CreateTask { workspace_id, title, goal } => Request::Workbench(Command::CreateTask { workspace_id, title, goal }),
            Call::SetRuntime { task_id, provider, model } => Request::Workbench(Command::SetConversationRuntime {
                conversation_id: format!("task:{task_id}"),
                preferences: RuntimePreferences { provider: Some(provider).filter(|p| !p.is_empty()), model: Some(model).filter(|m| !m.is_empty()), ..RuntimePreferences::default() },
            }),
            Call::SetRuntimePrefs { conversation_id, prefs } => Request::Workbench(Command::SetConversationRuntime { conversation_id, preferences: prefs }),
            Call::Rename { task_id, title } => Request::Workbench(Command::RenameTask { task_id, title }),
            Call::Publish { task_id } => Request::Workbench(Command::PublishTask { task_id }),
            Call::RunNext { task_id } => Request::Workbench(Command::PrioritizeTask { task_id }),
            Call::SetLandOnMain { repository, enabled } => Request::Workbench(Command::SetLandOnMain { repository, enabled }),
            Call::Retry { task_id } => Request::Workbench(Command::RetryTask { task_id }),
            Call::Delete { task_id } => Request::Workbench(Command::DeleteTask { task_id }),
            Call::StopAll => Request::Workbench(Command::CancelAllWork),
            Call::ClearFinished { workspace_id } => Request::Workbench(Command::ClearFinishedTasks { workspace_id }),
            Call::Ask { text, workspace_id } => Request::Workbench(Command::SendMessage { text, workspace_id }),
            Call::AskNow { text, workspace_id } => Request::Workbench(Command::InterruptAndSendMessage { text, workspace_id }),
            Call::EditQueued { text, workspace_id, .. } => Request::Workbench(Command::SendMessage { text, workspace_id }),
            Call::DiscardQueued { message_id } | Call::SteerQueued { message_id, .. } => Request::Workbench(Command::DiscardQueuedMessage { message_id }),
            Call::StopAsking { turn_id } => Request::Workbench(Command::CancelChat { turn_id }),
            Call::DecideChatTool { turn_id, call_id, approve } => Request::Workbench(Command::DecideChatTool { turn_id, call_id, approve }),
            Call::DecideChatAction { message_id, action_id, accept } => Request::Workbench(Command::DecideChatAction { message_id, action_id, accept }),
            Call::RoutinesFull => Request::Routines,
            Call::CalendarEvents { from_ms, to_ms } => Request::CalendarEvents { from_ms, to_ms },
            Call::PhoneSettings => Request::PhoneSettings,
            Call::Appearance => Request::GetAppearance,
            Call::SetAppearance { mode } => Request::SetAppearance { mode: Some(mode), light: None, dark: None },
            Call::Quota => Request::Search { query: String::new(), limit: 50, provider: Some("usage".to_owned()) },
            Call::Plugins => Request::PluginStatus { id: None },
            Call::SaveRoutine(routine) => Request::SaveRoutine { routine: *routine },
            Call::RunRoutine { id } => Request::RunRoutineNow { id },
            Call::SetRoutineEnabled { id, enabled } => Request::SetRoutineEnabled { id, enabled },
            Call::SetRoutinesOn { enabled } => Request::SetRoutinesOn { enabled },
            Call::SetStandingOrders { workspace_id, text } => Request::SetStandingOrders { workspace_id, text },
            Call::SetInboxAgeOut { days } => Request::SetInboxAgeOutDays { days },
            Call::SetCalendarOn { enabled } => Request::SetCalendarEnabled { enabled },
            Call::SavePhone(settings) => Request::SetPhoneSettings { settings: *settings },
            Call::PhoneTest => Request::PhoneTest,
            Call::SetAgentRuntime(runtime) => Request::Workbench(Command::SetAgentRuntime { runtime }),
            Call::SetTaskBudget { cents } => Request::Workbench(Command::SetTaskBudget { cents }),
            Call::SetDailyBudget { cents } => Request::Workbench(Command::SetDailyBudget { cents }),
            Call::SetMaxRunning { limit } => Request::Workbench(Command::SetMaxRunningTickets { limit }),
            Call::SetMaxWritable { limit } => Request::Workbench(Command::SetMaxWritablePerRepository { limit }),
            Call::SetPlanFirst { enabled } => Request::Workbench(Command::SetPlanFirst { enabled }),
            Call::SetStartWithoutApproval { enabled } => Request::Workbench(Command::SetStartWithoutApproval { enabled }),
            Call::SetSelfUpdate { auto_ui, auto_native } => Request::Workbench(Command::SetSelfUpdate { auto_ui, auto_native }),
            Call::UpdateNow => Request::Workbench(Command::UpdateNekoNow),
            Call::ApplyUpdate => Request::Workbench(Command::ApplyPendingUpdate),
            Call::SetDevCheckout { path } => Request::Workbench(Command::SetDevCheckout { path }),
            Call::Setup(call) => call.wire(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(call: &Call) -> serde_json::Value {
        serde_json::to_value(call.wire()).unwrap()
    }

    #[test]
    fn a_queued_message_is_the_webs_request_for_each_mode() {
        let enqueue = |mode| Call::Enqueue { task_id: "t".into(), command_id: "c1".into(), text: "hi".into(), mode };
        let body = json(&enqueue(DeliveryMode::AfterTurn));
        assert_eq!(body["Workbench"]["Crew"]["Enqueue"]["mode"], "after_turn");
        assert_eq!(body["Workbench"]["Crew"]["Enqueue"]["kind"], "send");
        assert_eq!(body["Workbench"]["Crew"]["Enqueue"]["command_id"], "c1");
        assert_eq!(json(&enqueue(DeliveryMode::SteerNow))["Workbench"]["Crew"]["Enqueue"]["kind"], "steer");
        assert_eq!(json(&enqueue(DeliveryMode::InterruptRestart))["Workbench"]["Crew"]["Enqueue"]["kind"], "interrupt");
    }

    #[test]
    fn reads_are_reads_and_nothing_else_is() {
        assert!(Call::Refresh.reads() && Call::Unseen.reads() && Call::Inbox.reads() && Call::Digest.reads());
        assert!(Call::Checkpoints { task_id: "t".into() }.reads());
        assert!(!Call::Redo { task_id: "t".into() }.reads());
        assert!(!Call::MarkDigestRead.reads());
        assert!(!Call::Ask { text: "x".into(), workspace_id: None }.reads());
    }

    #[test]
    fn undo_carries_every_option_the_refusals_offer() {
        let body = json(&Call::Undo { task_id: "t".into(), run: 2, through_later: true, snapshot_unsaved: false, revert: true });
        let undo = &body["Workbench"]["Checkpoints"]["Undo"];
        assert_eq!((undo["run"].as_u64(), undo["through_later"].as_bool(), undo["snapshot_unsaved"].as_bool(), undo["revert"].as_bool()), (Some(2), Some(true), Some(false), Some(true)));
    }

    #[test]
    fn inbox_answer_sends_no_words_so_the_default_is_taken() {
        let body = json(&Call::AnswerNeedsYou { seq: 7 });
        assert_eq!(body["AnswerNeedsYou"]["seq"], 7);
        assert!(body["AnswerNeedsYou"]["text"].is_null());
    }

    #[test]
    fn a_runtime_is_pinned_on_the_tasks_conversation_like_the_composer_does() {
        let body = json(&Call::SetRuntime { task_id: "abc".into(), provider: "Codex".into(), model: String::new() });
        let set = &body["Workbench"]["SetConversationRuntime"];
        assert_eq!(set["conversation_id"], "task:abc");
        assert_eq!(set["preferences"]["provider"], "Codex");
        assert!(set["preferences"]["model"].is_null());
    }
}
