//! The scenes a headless run can show (`neko --snap out.png --scene task-open`): placeholder data only, never the person's.

use std::collections::HashMap;

use neko_protocol::{
    Cat, Changes, FailureKind, GENERAL, Need, NeedsItem, Part, PartBody, PartId, Permission, PermissionId, PermissionMode, PermissionState, Pricing, Run, RunId, RunStatus, RuntimeInfo, Settings, Task, TaskId, TaskStatus, Tone,
    ToolState, Usage, Workspace,
};
use neko_protocol::rounds::{RunEnd, RunKind};
use neko_protocol::schedule::{Schedule, Weekday};
use neko_protocol::{Round, RoundRun, RoundStatus, TaskRound};
use neko_protocol::{DiffFile, DiffPatch, DiffSummary, FileStatus, RunUndo, UndoState, UndoStorage};
use rune::TextEdit;

use crate::state::{App, Book};

/// A scene: its name and what it does to the fresh state.
pub type Scene = (&'static str, fn(&mut App));

pub const SCENES: &[Scene] = &[
    ("new-thread", |_| {}),
    ("new-thread-typed", |s| { new_thread_typed(s); finish(s) }),
    ("new-thread-folder", |s| { new_thread_folder(s); finish(s) }),
    ("new-thread-in-folder", |s| { new_thread_in_folder(s); finish(s) }),
    ("new-thread-place-menu", |s| { new_thread_place_menu(s); finish(s) }),
    ("new-thread-branch-menu", |s| { new_thread_branch_menu(s); finish(s) }),
    ("new-thread-adding", |s| { new_thread_adding(s); finish(s) }),
    ("new-thread-started", |s| { new_thread_started(s); finish(s) }),
    ("new-thread-busy", |s| { new_thread_busy(s); finish(s) }),
    ("task-open", |s| { task_open(s); finish(s) }),
    ("tasks-home", |s| { tasks_home(s); finish(s) }),
    ("task-working", |s| { task_working(s); finish(s) }),
    ("task-working-live", |s| { task_working_live(s); finish(s) }),
    ("task-needs-you", |s| { task_needs_you(s); finish(s) }),
    ("task-needs-you-destructive", |s| { task_needs_you_destructive(s); finish(s) }),
    ("task-done", |s| { task_done(s); finish(s) }),
    ("task-done-open", |s| { task_done_open(s); finish(s) }),
    ("task-follow-up", |s| { task_follow_up(s); finish(s) }),
    ("composer-idle", |s| { composer::idle(s); finish(s) }),
    ("composer-working", |s| { composer::working_idle(s); finish(s) }),
    ("composer-typing", |s| { composer::typing(s); finish(s) }),
    ("composer-queued", |s| { composer::two_queued(s); finish(s) }),
    ("composer-queued-folded", |s| { composer::queued_folded(s); finish(s) }),
    ("composer-queued-folded-open", |s| { composer::queued_folded_open(s); finish(s) }),
    ("composer-editing", |s| { composer::editing(s); finish(s) }),
    ("composer-interrupting", |s| { composer::interrupting(s); finish(s) }),
    ("activity-collapsed", |s| { composer::activity_collapsed(s); finish(s) }),
    ("activity-open", |s| { composer::activity_open(s); finish(s) }),
    ("activity-open-queued", |s| { composer::activity_with_queue(s); finish(s) }),
    ("task-failed", |s| { task_failed(s); finish(s) }),
    ("task-thinking", |s| { task_thinking(s); finish(s) }),
    ("task-talked", |s| { task_talked(s); finish(s) }),
    ("task-asked", |s| { task_asked(s); finish(s) }),
    ("task-real-run", |s| { task_real_run(s); finish(s) }),
    ("task-real-run-open", |s| { task_real_run_open(s); finish(s) }),
    ("task-context-open", |s| { task_context_open(s); finish(s) }),
    ("task-stopped", |s| { task_stopped(s); finish(s) }),
    ("task-interrupted", |s| { task_interrupted(s); finish(s) }),
    ("settings", |s| { settings(s); finish(s) }),
    ("settings-ask", |s| { settings_ask(s); finish(s) }),
    ("settings-full-access", |s| { settings_full(s); finish(s) }),
    ("settings-signed-out", |s| { settings_signed_out(s); finish(s) }),
    ("workspaces", |s| { workspaces(s); finish(s) }),
    ("workspaces-folded", |s| { workspaces_folded(s); finish(s) }),
    ("workspace-picker", |s| { workspace_picker(s); finish(s) }),
    ("workspace-menu", |s| { workspace_menu(s); finish(s) }),
    ("workspace-rename", |s| { workspace_rename(s); finish(s) }),
    ("workspace-remove", |s| { workspace_remove(s); finish(s) }),
    ("workspace-task", |s| { workspace_task(s); finish(s) }),
    ("task-remove", |s| { task_remove(s); finish(s) }),
    ("task-remove-refused", |s| { task_remove_refused(s); finish(s) }),
    ("settings-workspaces", |s| { settings_workspaces(s); finish(s) }),
    ("needs-you", |s| { needs_you(s); finish(s) }),
    ("needs-you-answering", |s| { needs_you_answering(s); finish(s) }),
    ("needs-you-empty", |s| { needs_you_empty(s); finish(s) }),
    ("settings-notifications", |s| { settings_notifications(s); finish(s) }),
    ("settings-notifications-not-asked", |s| { settings_notifications_not_asked(s); finish(s) }),
    ("settings-notifications-denied", |s| { settings_notifications_denied(s); finish(s) }),
    ("settings-notifications-unavailable", |s| { settings_notifications_unavailable(s); finish(s) }),
    ("task-undo", |s| { task_undo(s); finish(s) }),
    ("task-undone", |s| { task_undone(s); finish(s) }),
    ("task-undo-older", |s| { task_undo_older(s); finish(s) }),
    ("task-undo-committed", |s| { task_undo_committed(s); finish(s) }),
    ("task-undo-unavailable", |s| { task_undo_unavailable(s); finish(s) }),
    ("task-undo-dropped", |s| { task_undo_dropped(s); finish(s) }),
    ("undo-anyway", |s| { undo_anyway(s); finish(s) }),
    ("pane-overview", |s| { pane_overview(s); finish(s) }),
    ("pane-overview-undone", |s| { pane_overview_undone(s); finish(s) }),
    ("pane-overview-empty", |s| { pane_overview_empty(s); finish(s) }),
    ("pane-overview-many", |s| { pane_overview_many(s); finish(s) }),
    ("pane-changes-uncommitted", |s| { pane_changes_uncommitted(s); finish(s) }),
    ("pane-changes-not-a-repository", |s| { pane_changes_not_a_repository(s); finish(s) }),
    ("pane-overview-kittens", |s| { subagents::pane_kittens(s); finish(s) }),
    ("pane-kitten-live", |s| { subagents::pane_kitten_live(s); finish(s) }),
    ("pane-kitten-done", |s| { subagents::pane_kitten_done(s); finish(s) }),
    ("review", |s| { review(s); finish(s) }),
    ("review-open", |s| { review_open(s); finish(s) }),
    ("review-all", |s| { review_all(s); finish(s) }),
    ("review-undone", |s| { review_undone(s); finish(s) }),
    ("settings-undo", |s| { settings_undo(s); finish(s) }),
    ("clear-undo", |s| { clear_undo(s); finish(s) }),
    ("cattain-thread", |s| { cattains::thread(s); finish(s) }),
    ("cattain-thread-new", |s| { cattains::thread_new(s); finish(s) }),
    ("cattain-digest", |s| { cattains::digest(s); finish(s) }),
    ("cattain-files", |s| { cattains::files(s); finish(s) }),
    ("cattain-files-editing", |s| { cattains::files_editing(s); finish(s) }),
    ("cattain-files-notes", |s| { cattains::files_notes(s); finish(s) }),
    ("cattain-profile", |s| { cattains::profile(s); finish(s) }),
    ("cattain-retire", |s| { cattains::retire(s); finish(s) }),
    ("crew-thread", |s| { cattains::crew_thread(s); finish(s) }),
    ("new-thread-crew", |s| { cattains::new_thread_crew(s); finish(s) }),
    ("sidebar-cattains-at-work", |s| { cattains::sidebar_at_work(s); finish(s) }),
    ("sidebar-cattains-quiet", |s| { cattains::sidebar_quiet(s); finish(s) }),
    ("cattain-made", |s| { make_cattain::note(s); finish(s) }),
    ("cattain-made-toast", |s| { make_cattain::toast(s); finish(s) }),
    ("cattain-made-thread", |s| { make_cattain::first_thread(s); finish(s) }),
    ("cattain-made-settings", |s| { make_cattain::settings(s); finish(s) }),
    ("cattain-made-limit", |s| { make_cattain::limit(s); finish(s) }),
    ("cat-new", |s| { cat_new(s); finish(s) }),
    ("task-rules-changed", |s| { task_rules_changed(s); finish(s) }),
    ("settings-cats", |s| { settings_cats(s); finish(s) }),
    ("rounds-empty", |s| { rounds_empty(s); finish(s) }),
    ("rounds", |s| { rounds(s); finish(s) }),
    ("rounds-history", |s| { rounds_history(s); finish(s) }),
    ("rounds-paused", |s| { rounds_paused(s); finish(s) }),
    ("rounds-braked", |s| { rounds_braked(s); finish(s) }),
    ("rounds-remove", |s| { rounds_remove(s); finish(s) }),
    ("round-new", |s| { round_new(s); finish(s) }),
    ("round-daily", |s| { round_daily(s); finish(s) }),
    ("round-hours", |s| { round_hours(s); finish(s) }),
    ("round-manual", |s| { round_manual(s); finish(s) }),
    ("round-edit", |s| { round_edit(s); finish(s) }),
    ("round-invalid", |s| { round_invalid(s); finish(s) }),
    ("round-cat-menu", |s| { round_cat_menu(s); finish(s) }),
    ("task-from-round", |s| { task_from_round(s); finish(s) }),
    ("sidebar-cattains", |s| { sidebar_cattains(s); finish(s) }),
    ("sparse-new-thread", |s| { sparse_new_thread(s); finish(s) }),
    ("sparse-empty", |s| { sparse_empty(s); finish(s) }),
    ("sparse-status", |s| { sparse_status(s); finish(s) }),
    ("threads-ungrouped", |s| { threads_ungrouped(s); finish(s) }),
    ("threads-view-menu", |s| { threads_view_menu(s); finish(s) }),
    ("threads-row-menu", |s| { threads_row_menu(s); finish(s) }),
    ("threads-pinned", |s| { threads_pinned(s); finish(s) }),
    ("threads-settled", |s| { threads_settled(s); finish(s) }),
    ("threads-settled-open", |s| { threads_settled_open(s); finish(s) }),
    ("threads-archived", |s| { threads_archived(s); finish(s) }),
    ("threads-pinned-menu", |s| { threads_pinned_menu(s); finish(s) }),
    ("threads-toast-settled", |s| { threads_toast_settled(s); finish(s) }),
    ("thread-band-menu", |s| { thread_band_menu(s); finish(s) }),
    ("sparse-working", |s| { sparse_working(s); finish(s) }),
    ("sparse-needs-you", |s| { sparse_needs_you(s); finish(s) }),
    ("sparse-two-tasks", |s| { sparse_two_tasks(s); finish(s) }),
    ("sparse-task", |s| { sparse_task(s); finish(s) }),
    ("briefing-sparse", |s| { briefing::sparse_briefing(s); finish(s) }),
    ("briefing", |s| { briefing::briefing(s); finish(s) }),
    ("briefing-finished", |s| { briefing::briefing_finished(s); finish(s) }),
    ("briefing-empty", |s| { briefing::briefing_empty(s); finish(s) }),
    ("briefing-caught-up", |s| { briefing::briefing_caught_up(s); finish(s) }),
    ("briefing-caught-up-waiting", |s| { briefing::briefing_caught_up_waiting(s); finish(s) }),
    ("briefing-many", |s| { briefing::briefing_many(s); finish(s) }),
    ("briefing-morning-off", |s| { briefing::briefing_morning_off(s); finish(s) }),
    ("home-briefing", |s| { briefing::home_briefing(s); finish(s) }),
    ("settings-briefing", |s| { briefing::settings_briefing(s); finish(s) }),
    ("settings-briefing-invalid", |s| { briefing::settings_briefing_invalid(s); finish(s) }),
    ("plugins-empty", |s| { plugins::plugins_empty(s); finish(s) }),
    ("plugins-waiting", |s| { plugins::plugins_waiting(s); finish(s) }),
    ("plugins", |s| { plugins::plugins(s); finish(s) }),
    ("plugins-changed", |s| { plugins::plugins_changed(s); finish(s) }),
    ("plugins-needs-setup", |s| { plugins::plugins_needs_setup(s); finish(s) }),
    ("plugins-check-ok", |s| { plugins::plugins_check_ok(s); finish(s) }),
    ("plugins-check-failed", |s| { plugins::plugins_check_failed(s); finish(s) }),
    ("plugins-testing", |s| { plugins::plugins_testing(s); finish(s) }),
    ("plugins-web", |s| { plugins::plugins_web(s); finish(s) }),
    ("plugins-fields-unset", |s| { plugins::plugins_fields_unset(s); finish(s) }),
    ("plugins-fields-set", |s| { plugins::plugins_fields_set(s); finish(s) }),
    ("plugins-secret-changing", |s| { plugins::plugins_secret_changing(s); finish(s) }),
    ("plugins-access-only", |s| { plugins::plugins_access_only(s); finish(s) }),
    ("plugins-access-none", |s| { plugins::plugins_access_none(s); finish(s) }),
    ("plugins-remove", |s| { plugins::plugins_remove(s); finish(s) }),
    ("plugin-add", |s| { plugins::plugin_add(s); finish(s) }),
    ("plugin-add-good", |s| { plugins::plugin_add_good(s); finish(s) }),
    ("plugin-add-bad", |s| { plugins::plugin_add_bad(s); finish(s) }),
    ("plugin-add-several", |s| { plugins::plugin_add_several(s); finish(s) }),
    ("plugin-add-unnamed", |s| { plugins::plugin_add_unnamed(s); finish(s) }),
    ("plugin-add-refused", |s| { plugins::plugin_add_refused(s); finish(s) }),
    ("needs-you-plugin", |s| { plugins::needs_you_plugin(s); finish(s) }),
    ("needs-you-plugin-changed", |s| { plugins::needs_you_plugin_changed(s); finish(s) }),
    ("needs-you-plugin-web", |s| { plugins::needs_you_plugin_web(s); finish(s) }),
    ("needs-you-plugin-only", |s| { plugins::needs_you_plugin_only(s); finish(s) }),
    ("kittens-placement", |s| { subagents::kittens_placement(s); finish(s) }),
    ("kittens-drafts", |s| { subagents::kittens_drafts(s); finish(s) }),
    ("kittens-limits", |s| { subagents::kittens_limits(s); finish(s) }),
    ("pane-kittens-deep", |s| { subagents::pane_kittens_deep(s); finish(s) }),
    ("subagents-cards", |s| { subagents::cards(s); finish(s) }),
    ("subagents-wake", |s| { subagents::wake(s); finish(s) }),
    ("subagents-wake-open", |s| { subagents::wake_open(s); finish(s) }),
    ("subagents-waiting", |s| { subagents::waiting(s); finish(s) }),
    ("subagent-view", |s| { subagents::view(s); finish(s) }),
    ("subagent-asks", |s| { subagents::asks(s); finish(s) }),
    ("needs-subagent", |s| { subagents::needs(s); finish(s) }),
    ("question-card", |s| { subagents::question_card(s); finish(s) }),
    ("question-answered", |s| { subagents::question_answered(s, false); finish(s) }),
    ("question-answered-open", |s| { subagents::question_answered(s, true); finish(s) }),
    ("question-to-person", |s| { subagents::question_to_person(s); finish(s) }),
    ("question-needs", |s| { subagents::question_needs(s); finish(s) }),
    ("question-kitten-tab", |s| { subagents::question_kitten_tab(s); finish(s) }),
    ("question-kitten-answered", |s| { subagents::question_kitten_answered(s); finish(s) }),
    ("task-plugin", |s| { plugins::task_plugin(s); finish(s) }),
    ("task-plugin-request", |s| { plugins::task_plugin_request(s); finish(s) }),
    ("cattain-profile-plugins", |s| { plugins::cat_page_plugins(s); finish(s) }),
    ("cattain-profile-no-plugins", |s| { plugins::cat_page_no_plugins(s); finish(s) }),
    ("cattain-profile-plugins-none-for-cat", |s| { plugins::cat_page_plugins_none_for_cat(s); finish(s) }),
    ("search-empty", |s| { search::empty(s); finish(s) }),
    ("search-typing", |s| { search::typing(s); finish(s) }),
    ("search-message", |s| { search::message(s); finish(s) }),
    ("search-actions", |s| { search::actions(s); finish(s) }),
    ("search-filter", |s| { search::filter(s); finish(s) }),
    ("search-problem", |s| { search::problem(s); finish(s) }),
    ("search-none", |s| { search::none(s); finish(s) }),
    ("search-put-away", |s| { search::put_away(s); finish(s) }),
    ("search-loading", |s| { search::loading(s); finish(s) }),
    ("search-first-moment", |s| { search::first_moment(s); finish(s) }),
];

mod briefing;
mod cattains;
mod composer;
mod make_cattain;
mod plugins;
mod search;
mod subagents;

/// What the daemon works out about each task, worked out here for placeholder data (the app never derives it for a real one):
/// the thread's status, the Needs you flag, why it failed and what it last said. A scene sets `status`, and anything it wants
/// different afterwards (an unread thread: `last_read_ms`).
pub fn finish(s: &mut App) {
    let failing: Vec<TaskId> = s.needs.iter().filter(|i| matches!(i.need, Need::Failure { .. })).map(|i| i.task.clone()).collect();
    // A subagent's request makes its thread need the person, as the daemon works it out.
    let subagent_asks: Vec<TaskId> = s.needs.iter().filter(|i| i.subagent.is_some()).filter_map(|i| s.tasks.iter().find(|t| t.id == i.task).and_then(|t| t.subagent.as_ref().map(|o| o.root.clone()))).collect();
    let mut i = 0;
    for t in &mut s.tasks {
        t.thread_status = match t.status {
            TaskStatus::Working | TaskStatus::NeedsYou => neko_protocol::ThreadStatus::Working,
            TaskStatus::Waiting => neko_protocol::ThreadStatus::Waiting,
            TaskStatus::Failed if t.last_read_ms < t.activity_ms => neko_protocol::ThreadStatus::Failed,
            _ => neko_protocol::ThreadStatus::Idle,
        };
        t.needs_you = matches!(t.status, TaskStatus::NeedsYou | TaskStatus::Asked) || (t.status == TaskStatus::Failed && failing.contains(&t.id)) || subagent_asks.contains(&t.id);
        if t.status == TaskStatus::Failed && t.failure_line.is_empty() {
            t.failure_line = "Claude Code is not signed in.".into();
        }
        if t.latest_line.is_empty() {
            i += 1;
            t.latest_line = match t.status {
                TaskStatus::Done => ["Renamed the label in 4 files and ran the tests.", "The nightly build is slow because of the retry loop.", "Moved the notes into one folder per project.", "Checked all 31 links; two redirects remain."][i % 4].to_owned(),
                TaskStatus::Working => ["Reading the release notes template.", "Running the tests for acme-api.", "Comparing the images before and after."][i % 3].to_owned(),
                _ => String::new(),
            };
        }
    }
}

/// The starter cats as a new install has them, each with one placeholder purpose.
pub fn sample_cats() -> Vec<Cat> {
    let cat = |id: &str, name: &str, face: &str, purpose: Option<&str>| Cat {
        id: id.into(),
        name: name.into(),
        face: face.into(),
        purpose: purpose.map(str::to_owned),
        workspace: None,
        thread: None,
        model: None,
        access: None,
        created_ms: 1,
        retired_ms: None,
        made_in: None,
    };
    vec![cat("cat_miso", "miso", "ginger", None), cat("cat_mochi", "mochi", "snow", Some("Writes and tidies the docs.")), cat("cat_tofu", "tofu", "calico", Some("Runs the tests and fixes what breaks."))]
}

/// Making a Cattain, from Settings ▸ Cattains (the only place): the dialog says which face it will get.
fn cat_new(s: &mut App) {
    with_workspaces(s);
    s.settings_open = true;
    s.settings_page = 3;
    s.start_new_cat();
    if let Some((_, e)) = &mut s.naming {
        *e = TextEdit::new("pip");
        e.focused = true;
    }
}

/// A follow-up after its cat's rules changed: one quiet line before the run that used the new ones.
fn task_rules_changed(s: &mut App) {
    task_follow_up(s);
    let id = s.open.clone().expect("open");
    // This thread is in miso's crew, so miso's CREW.md goes with its runs.
    if let Some(t) = s.tasks.iter_mut().find(|t| t.id == id) {
        t.cat = Some("cat_miso".into());
    }
    let cat = s.tasks.iter().find(|t| t.id == id).and_then(|t| t.cat.clone()).expect("the thread");
    let name = s.cat(&cat).map(|c| c.name.clone()).expect("its cat");
    if let Some(b) = s.books.get_mut(&id) {
        let at = b.parts.iter().position(|p| p.run.as_ref().is_some_and(|r| r.0 == "run_2")).expect("the second run");
        let seq = b.parts[at].seq;
        for p in b.parts.iter_mut().filter(|p| p.seq >= seq) {
            p.seq += 1;
        }
        let first = b.parts[at].clone();
        b.parts.insert(at, Part { id: PartId("part_rules".into()), seq, body: PartBody::RulesChanged { cat, name, file: Some("CREW.md".into()) }, ..first });
    }
}

fn settings_cats(s: &mut App) {
    with_workspaces(s);
    s.cats[2].retired_ms = Some(3);
    s.settings_open = true;
    s.settings_page = 3;
}

/// A task made `minutes_ago` before the fixed clock.
fn task(n: u32, words: &str, minutes_ago: i64) -> Task {
    let (now, _) = crate::fixed_clock();
    let title = words.lines().next().unwrap_or("").to_owned();
    // A sample thread has no cat, as a thread the person starts has none; a scene that shows a cat's work gives it one.
    Task::blank(TaskId(format!("task_{n:026}")), "default", words, &title, now - minutes_ago * 60_000, None)
}

/// Four tasks: three from today and one from earlier in the week, each in a different state.
pub fn sample_tasks() -> Vec<Task> {
    let mut t = vec![
        task(4, "Draft the release notes for 0.2 and send them to Lee for a read", 12),
        task(3, "Rename the Settings button to Preferences across the app\n\nKeep the keyboard shortcut (⌘,) and check ENG-123 is covered.", 48),
        task(2, "Find out why the nightly build of acme-api is slow", 190),
        task(1, "Tidy the Downloads folder into Archive and Screenshots", 3 * 24 * 60),
    ];
    t[0].status = TaskStatus::Working;
    t[1].status = TaskStatus::New;
    t[2].status = TaskStatus::Done;
    t[3].status = TaskStatus::Failed;
    t
}

fn new_thread_typed(s: &mut App) {
    s.draft = TextEdit::new("Rename the Settings button to Preferences across the app");
    s.draft.focused = true;
}

/// A project that is a repository: its branches are known, so the chips for where the thread works and where it starts show.
fn git_project(s: &mut App) {
    s.workspace = "ws_acme".into();
    s.new_thread.branches.insert(
        "ws_acme".into(),
        crate::new_thread::BranchList { git: true, current: Some("main".into()), names: vec!["main".into(), "release-2".into(), "sidebar-rework".into()] },
    );
}

fn new_thread_folder(s: &mut App) {
    with_workspaces(s);
    s.draft = TextEdit::new("Find out why the nightly build is slow");
    git_project(s);
}

fn new_thread_in_folder(s: &mut App) {
    new_thread_folder(s);
    s.new_thread.in_folder = true;
}

fn new_thread_place_menu(s: &mut App) {
    new_thread_folder(s);
    s.new_thread.place_open = true;
}

fn new_thread_branch_menu(s: &mut App) {
    new_thread_folder(s);
    s.new_thread.from_branch = Some("release-2".into());
    s.new_thread.branch_open = true;
}

/// A folder chosen from the menu a moment ago: the chip names it already, the project is not made yet.
fn new_thread_adding(s: &mut App) {
    with_workspaces(s);
    s.draft = TextEdit::new("Look at the footer on small screens");
    s.begin_adding("/Users/example/Projects/acme-web");
}

/// After ⌘Return: the thread is in the sidebar, the box is empty and ready, and one quiet line says it started.
fn new_thread_started(s: &mut App) {
    new_thread_folder(s);
    s.draft = TextEdit::default();
    s.draft.focused = true;
    s.notice = Some("Started Find out why the nightly build is slow".into());
}

/// A busy window: the sample threads in the sidebar, the briefing's line under the chips.
fn new_thread_busy(s: &mut App) {
    with_workspaces(s);
    git_project(s);
    briefing::home_briefing(s);
}

fn ws(id: &str, name: &str, folder: Option<&str>, position: i64) -> Workspace {
    Workspace { id: id.into(), name: name.into(), folder: folder.map(str::to_owned), created_ms: position, removed_ms: None, position, missing: false }
}

/// General, three workspaces (one with a long name, one whose folder is not found) and one removed, with tasks in each.
pub fn sample_workspaces() -> Vec<Workspace> {
    let mut gone = ws("ws_scripts", "old-scripts", Some("/Users/example/old-scripts"), 5);
    gone.removed_ms = Some(7);
    let mut notes = ws("ws_notes", "notes", Some("/Users/example/notes"), 4);
    notes.missing = true;
    vec![
        ws(GENERAL, "No project", None, 0),
        ws("ws_acme", "acme-api", Some("/Users/example/acme-api"), 1),
        ws("ws_web", "Acme marketing website redesign for the spring launch", Some("/Users/example/Projects/acme-marketing-website-redesign-2026"), 2),
        notes,
        gone,
    ]
}

fn with_workspaces(s: &mut App) {
    s.workspaces = sample_workspaces();
    s.tasks_loaded = true;
    let mut tasks = sample_tasks();
    tasks[0].workspace = GENERAL.into();
    tasks[1].workspace = "ws_acme".into();
    tasks[1].folder = Some("/Users/example/acme-api".into());
    tasks[2].workspace = "ws_acme".into();
    tasks[2].folder = Some("/Users/example/acme-api".into());
    tasks[3].workspace = GENERAL.into();
    let mut n = 10;
    let mut more = |ws: &str, folder: &str, words: &str, minutes: i64, status: TaskStatus| {
        n += 1;
        let mut t = task(n, words, minutes);
        t.workspace = ws.into();
        t.folder = Some(folder.into());
        t.status = status;
        t
    };
    tasks.push(more("ws_acme", "/Users/example/acme-api", "Answer the question about the retry limit before the release goes out to every customer", 30, TaskStatus::NeedsYou));
    let web = "/Users/example/Projects/acme-marketing-website-redesign-2026";
    for (i, words) in [
        "Make the hero image load faster on phones",
        "Check every page for broken links",
        "Write the pricing page copy",
        "Move the blog to the new layout",
        "Add the spring launch banner",
        "Tidy the footer links",
        "Fix the contact form on Safari",
        "Compress the case study images",
        "Draft the press page",
        "Translate the home page into French",
        "Set up the redirects from the old site",
    ]
    .iter()
    .enumerate()
    {
        tasks.push(more("ws_web", web, words, 60 + i as i64 * 90, if i == 0 { TaskStatus::Working } else { TaskStatus::Done }));
    }
    tasks.push(more("ws_notes", "/Users/example/notes", "Sort the meeting notes from last week into folders by project", 2 * 24 * 60, TaskStatus::Done));
    tasks.push(more("ws_scripts", "/Users/example/old-scripts", "Port the backup script to the new server", 9 * 24 * 60, TaskStatus::Done));
    tasks.sort_by_key(|t| std::cmp::Reverse(t.created_ms));
    s.tasks = tasks;
    // Two threads finished while the person was elsewhere: their titles are bold until opened.
    mark_unread(s, "Find out why the nightly build");
    mark_unread(s, "Check every page for broken links");
    let retry = s.tasks.iter().find(|t| t.title.starts_with("Answer the question about the retry limit")).cloned().expect("the sample thread");
    s.needs = vec![permission_item(s, &retry, "Run a command", "cargo test --package acme-api retry", false, 30)];
    s.needs_loaded = true;
}

/// The thread whose title starts so has activity the person has not seen.
fn mark_unread(s: &mut App, starts: &str) {
    if let Some(t) = s.tasks.iter_mut().find(|t| t.title.starts_with(starts)) {
        t.last_read_ms = t.activity_ms - 1;
    }
}

fn ago(s: &App, minutes: i64) -> i64 {
    s.now_ms - minutes * 60_000
}

/// A cat's name as the sample data has it.
fn cat_name(s: &App, id: &Option<String>) -> String {
    id.as_deref().map(|id| s.cat(id).map_or_else(|| "Neko".to_owned(), |c| c.name.clone())).unwrap_or_default()
}

fn permission_item(s: &App, task: &Task, title: &str, detail: &str, destructive: bool, minutes: i64) -> NeedsItem {
    let p = Permission {
        id: PermissionId(format!("perm_{}", task.id.0)),
        task: task.id.clone(),
        run: RunId("run_1".into()),
        tool: "Bash".into(),
        title: title.into(),
        why: "Commands can change anything on this Mac.".into(),
        detail: detail.into(),
        destructive,
        state: PermissionState::Pending,
        asked_ms: ago(s, minutes),
        answered_ms: None,
    };
    NeedsItem { id: format!("perm:{}", p.id.0), task: task.id.clone(), workspace: task.workspace.clone(), title: task.title.clone(), cat: task.cat.clone().unwrap_or_default(), cat_name: cat_name(s, &task.cat), since_ms: p.asked_ms, need: Need::Permission { permission: p }, subagent: None }
}

/// Every kind of item, in three workspaces, in the daemon's order (the workspace waiting longest first, oldest first inside it).
fn needs_you(s: &mut App) {
    with_workspaces(s);
    let find = |s: &App, start: &str| s.tasks.iter().find(|t| t.title.starts_with(start)).cloned().expect("the sample thread");
    let tidy = find(s, "Tidy the Downloads");
    let notes = find(s, "Draft the release notes");
    let nightly = find(s, "Find out why the nightly build");
    let retry = find(s, "Answer the question about the retry limit");
    let hero = find(s, "Make the hero image load faster");
    let failure = |s: &App, t: &Task, kind: FailureKind, text: &str, next: &str, command: Option<&str>, minutes: i64| NeedsItem {
        id: format!("failure:part_{}", t.id.0),
        task: t.id.clone(),
        workspace: t.workspace.clone(),
        title: t.title.clone(),
        cat: t.cat.clone().unwrap_or_default(),
        cat_name: cat_name(s, &t.cat),
        since_ms: ago(s, minutes),
        need: Need::Failure { failure: kind, text: text.into(), next: Some(next.into()), command: command.map(str::to_owned) }, subagent: None
    };
    s.needs = vec![
        failure(s, &tidy, FailureKind::NotSignedIn, "Claude Code is not signed in.", "Run `claude auth login` in Terminal, then send a follow-up.", Some("claude auth login"), 3 * 24 * 60),
        NeedsItem {
            id: format!("question:run_{}", notes.id.0),
            task: notes.id.clone(),
            workspace: notes.workspace.clone(),
            title: notes.title.clone(),
            cat: notes.cat.clone().unwrap_or_default(),
            cat_name: cat_name(s, &notes.cat),
            since_ms: ago(s, 12),
            need: Need::Question { run: RunId(format!("run_{}", notes.id.0)), question: "The notes cover the sidebar and the composer. Should I also mention the new keyboard shortcuts, or keep them for 0.3?".into(), kitten: None, via: Vec::new(), unanswered: false }, subagent: None
        },
        failure(s, &nightly, FailureKind::UsageLimit, "Claude Code has reached its usage limit.", "It resets at 17:00. Send a follow-up then to carry on.", None, 2 * 60),
        permission_item(s, &retry, "Run a command", "cargo test --package acme-api retry", false, 30),
        permission_item(s, &hero, "Run a command that deletes or overwrites", "rm -rf public/images/old-hero", true, 5),
    ];
    for t in &mut s.tasks {
        if [&notes.id].contains(&&t.id) {
            t.status = TaskStatus::Asked;
        } else if [&tidy.id, &nightly.id].contains(&&t.id) {
            t.status = TaskStatus::Failed;
        } else if [&retry.id, &hero.id].contains(&&t.id) {
            t.status = TaskStatus::NeedsYou;
        }
    }
    s.open_needs();
}

/// One answered (gone at once), and an answer being typed to the question.
fn needs_you_answering(s: &mut App) {
    needs_you(s);
    let first = s.needs[0].id.clone();
    s.needs_hidden.insert(first);
    let q = s.needs[1].id.clone();
    let mut edit = TextEdit::new("Keep them for 0.3");
    edit.focused = true;
    s.needs_replies.insert(q, edit);
}

fn needs_you_empty(s: &mut App) {
    with_workspaces(s);
    s.needs.clear();
    for t in &mut s.tasks {
        if t.status == TaskStatus::NeedsYou {
            t.status = TaskStatus::Working;
        }
    }
    s.open_needs();
}

fn settings_notifications_with(s: &mut App, a: crate::notify::Allowed) {
    settings(s);
    s.settings_page = 2;
    s.allowed.store(match a {
        crate::notify::Allowed::Unknown => 0,
        crate::notify::Allowed::NotAsked => 1,
        crate::notify::Allowed::Granted => 2,
        crate::notify::Allowed::Denied => 3,
        crate::notify::Allowed::Unavailable => 4,
    }, std::sync::atomic::Ordering::SeqCst);
}

fn settings_notifications(s: &mut App) {
    settings_notifications_with(s, crate::notify::Allowed::Granted);
}

fn settings_notifications_not_asked(s: &mut App) {
    settings_notifications_with(s, crate::notify::Allowed::NotAsked);
    s.notifier.switches.finished = false;
}

fn settings_notifications_denied(s: &mut App) {
    settings_notifications_with(s, crate::notify::Allowed::Denied);
}

fn settings_notifications_unavailable(s: &mut App) {
    settings_notifications_with(s, crate::notify::Allowed::Unavailable);
}

fn workspaces(s: &mut App) {
    with_workspaces(s);
    s.workspace = "ws_acme".into();
}

/// A person's first week, as sparse as real use is: General folded with no tasks, one workspace with one finished task, three cats
/// napping. The busy scenes hid how the sidebar looks when there is little in it.
pub(crate) fn sparse(s: &mut App) {
    s.workspaces = vec![ws(GENERAL, "No project", None, 0), ws("ws_acme", "acme-api", Some("/Users/example/acme-api"), 1)];
    s.tasks_loaded = true;
    s.folded = [GENERAL.to_owned()].into_iter().collect();
    let mut t = task(30, "What changed this week?", 26 * 60);
    t.workspace = "ws_acme".into();
    t.folder = Some("/Users/example/acme-api".into());
    t.cat = Some("cat_miso".into());
    t.status = TaskStatus::Done;
    t.latest_line = "Two changes were merged and one waits for a review.".into();
    s.tasks = vec![t];
}

/// Cats at work: three of six have a task (given them before threads needed none, or by a round), one of them waiting on the person.
/// The sidebar lists them under the nav as Cattains.
fn sidebar_cattains(s: &mut App) {
    with_workspaces(s);
    s.cats = six_cats();
    let mut busy = |title: &str, cat: &str| {
        if let Some(t) = s.tasks.iter_mut().find(|t| t.title.starts_with(title)) {
            t.cat = Some(cat.into());
        }
    };
    busy("Draft the release notes", "cat_miso");
    busy("Make the hero image load", "cat_sesame");
    busy("Answer the question about the retry limit", "cat_yuzu");
    let retry = s.tasks.iter().find(|t| t.title.starts_with("Answer the question about the retry limit")).cloned().expect("the sample thread");
    s.needs = vec![permission_item(s, &retry, "Run a command", "cargo test --package acme-api retry", false, 30)];
}

fn sparse_new_thread(s: &mut App) {
    sparse(s);
}

/// A first launch: a project added and nothing started yet.
fn sparse_empty(s: &mut App) {
    sparse(s);
    s.tasks.clear();
}

/// The three states a row can lead with, in one project: one working, one that failed, one with a request waiting.
fn sparse_status(s: &mut App) {
    sparse(s);
    s.tasks.clear();
    let working = sparse_task_of(s, 41, "Draft the release notes for 0.2", 4, "cat_mochi", TaskStatus::Working);
    let failed = sparse_task_of(s, 42, "Sync the changelog with the docs", 70, "cat_miso", TaskStatus::Failed);
    let asks = sparse_task_of(s, 43, "Find out why the nightly build is slow", 35, "cat_tofu", TaskStatus::NeedsYou);
    s.needs = vec![permission_item(s, &asks, "Run a command", "cargo test --package acme-api retry", false, 30)];
    let _ = (working, failed);
    sparse_task_of(s, 44, "What changed this week?", 26 * 60, "cat_miso", TaskStatus::Done);
    mark_unread(s, "What changed this week");
    // It failed while the person was elsewhere: it is Failed until opened.
    mark_unread(s, "Sync the changelog");
    s.tasks.sort_by_key(|t| std::cmp::Reverse(t.created_ms));
}

/// The busy house with the threads in one list by last activity, each naming its project.
fn threads_ungrouped(s: &mut App) {
    with_workspaces(s);
    s.grouping = crate::threads::Grouping::None;
}

/// View, open: Group by Project or None.
fn threads_view_menu(s: &mut App) {
    with_workspaces(s);
    s.menu_open = Some("threads-view".into());
}

/// A thread's menu, open under its row.
fn threads_row_menu(s: &mut App) {
    with_workspaces(s);
    let id = s.tasks.iter().find(|t| t.title.starts_with("Find out why the nightly build")).map(|t| t.id.clone()).expect("the sample thread");
    s.menu_open = Some(format!("thread-menu-{}", id.0));
}

/// The thread whose title starts so, in the sample data.
fn thread_titled(s: &mut App, starts: &str) -> TaskId {
    s.tasks.iter().find(|t| t.title.starts_with(starts)).map(|t| t.id.clone()).unwrap_or_else(|| panic!("no sample thread starting {starts:?}"))
}

fn shelve(s: &mut App, starts: &str, how: impl Fn(&mut Task)) {
    let id = thread_titled(s, starts);
    if let Some(t) = s.tasks.iter_mut().find(|t| t.id == id) {
        how(t);
    }
}

/// Two threads pinned, from two projects: Pinned is the first group and each row names its project.
fn threads_pinned(s: &mut App) {
    with_workspaces(s);
    // A shorter house, so the groups' ends fit in a picture: the web project keeps five threads.
    let drop: Vec<TaskId> = ["Translate the home page", "Set up the redirects", "Draft the press page", "Compress the case study", "Check every page for broken"].iter().map(|w| thread_titled(s, w)).collect();
    s.tasks.retain(|t| !drop.contains(&t.id));
    shelve(s, "Find out why the nightly build", |t| t.pinned = true);
    shelve(s, "Write the pricing page copy", |t| t.pinned = true);
    shelve(s, "Answer the question about the retry limit", |t| t.pinned = true);
}

/// Threads settled in two projects: each group ends in a closed Settled fold with its count.
fn threads_settled(s: &mut App) {
    threads_pinned(s);
    for starts in ["Move the blog to the new layout", "Add the spring launch banner", "Tidy the footer links", "Fix the contact form on Safari"] {
        shelve(s, starts, |t| t.settled = true);
    }
    shelve(s, "Rename the Settings button", |t| t.settled = true);
}

/// The same with the Settled fold of the web project open.
fn threads_settled_open(s: &mut App) {
    threads_settled(s);
    s.settled_open.insert("ws_web".into());
}

/// One thread archived, with View ▸ Show archived on and its fold open.
fn threads_archived(s: &mut App) {
    threads_settled(s);
    // A shorter list, so its end fits in a picture: the two archived threads were the other projects' (which are not drawn here).
    shelve(s, "Port the backup script", |t| {
        t.archived = true;
        t.workspace = "ws_acme".into();
    });
    shelve(s, "Sort the meeting notes", |t| {
        t.archived = true;
        t.workspace = "ws_acme".into();
    });
    s.workspaces.retain(|w| w.id != "ws_notes" && w.id != "ws_scripts");
    // The groups above are folded, so the end of the list fits in a picture.
    s.folded = ["default", "ws_web", "ws_notes", "ws_scripts"].iter().map(|x| (*x).to_owned()).collect::<std::collections::HashSet<_>>();
    s.show_archived = true;
    s.settled_open.insert(crate::threads::ARCHIVED.into());
}

/// A pinned thread's menu, open under its row.
fn threads_pinned_menu(s: &mut App) {
    threads_pinned(s);
    let id = thread_titled(s, "Find out why the nightly build");
    s.menu_open = Some(format!("thread-menu-{}", id.0));
}

/// A thread was just settled: the toast says so, with Undo (sticky here, so a picture can be taken).
fn threads_toast_settled(s: &mut App) {
    threads_settled(s);
    s.toasts.push(crate::ui::toast::ToastSpec::new("Settled").description("Fix the contact form on Safari").action("Undo", "unsettle:none:0").sticky());
}

/// The band's own menu on an open thread.
fn thread_band_menu(s: &mut App) {
    sparse_task(s);
    s.menu_open = Some("task-more".into());
}

/// A task of the sparse house's workspace, in this state, given to this cat.
fn sparse_task_of(s: &mut App, n: u32, words: &str, minutes: i64, cat: &str, status: TaskStatus) -> Task {
    let mut t = task(n, words, minutes);
    t.workspace = "ws_acme".into();
    t.folder = Some("/Users/example/acme-api".into());
    t.cat = Some(cat.into());
    t.status = status;
    s.tasks.push(t.clone());
    t
}

/// One cat at work: mochi on the release notes. The Cattains show it, the others nap and are not listed.
fn sparse_working(s: &mut App) {
    sparse(s);
    sparse_task_of(s, 31, "Draft the release notes for 0.2", 12, "cat_mochi", TaskStatus::Working);
}

/// One cat waiting on the person: tofu asks to run a command.
fn sparse_needs_you(s: &mut App) {
    sparse(s);
    let t = sparse_task_of(s, 32, "Find out why the nightly build is slow", 30, "cat_tofu", TaskStatus::NeedsYou);
    s.needs = vec![permission_item(s, &t, "Run a command", "cargo test --package acme-api retry", false, 4)];
}

/// One cat with two tasks at work: its tasks nest under it.
fn sparse_two_tasks(s: &mut App) {
    sparse(s);
    sparse_task_of(s, 33, "Tidy the Downloads folder", 25, "cat_miso", TaskStatus::Working);
    sparse_task_of(s, 34, "Check every page for broken links", 9, "cat_miso", TaskStatus::Working);
}

/// The sparse house with its one task open: a short answer, no files changed, so no receipt.
fn sparse_task(s: &mut App) {
    sparse(s);
    let task = s.tasks[0].clone();
    let at = s.now_ms - 26 * 60 * 60_000;
    let mut sc = Script::new(&task, "run_1", at);
    sc.tool("Bash", "git log --since=7.days --oneline", "{\n  \"command\": \"git log --since=7.days --oneline\"\n}", "9 commits", ToolState::Done, 2);
    sc.say("Nine commits landed this week, all in `acme-api`.\n\n- The retry limit is now configurable\n- The nightly build is faster after the cache fix\n- Two docs pages were tidied\n\nNothing is waiting for review.", true);
    let mut r = run("run_1", &task, RunStatus::Done, at, 41);
    r.changes = None;
    let mut book = Book { runs: vec![r], loaded: true, ..Book::default() };
    book.parts.extend(sc.parts);
    s.books.insert(task.id.clone(), book);
    s.open = Some(task.id);
}

fn workspaces_folded(s: &mut App) {
    workspaces(s);
    s.folded = ["ws_web".to_owned(), "ws_scripts".to_owned()].into_iter().collect();
}

fn workspace_picker(s: &mut App) {
    workspaces(s);
    s.picker_open = true;
}

fn workspace_menu(s: &mut App) {
    workspaces_folded(s);
    s.menu_open = Some("ws-menu-ws_acme".into());
}

fn workspace_rename(s: &mut App) {
    workspaces_folded(s);
    s.start_rename("ws_acme");
}

fn workspace_remove(s: &mut App) {
    workspaces_folded(s);
    s.confirm = Some(crate::state::Confirm::RemoveWorkspace("ws_acme".into()));
}

fn workspace_task(s: &mut App) {
    with_workspaces(s);
    let id = s.tasks.iter().find(|t| t.title.starts_with("Rename the Settings")).map(|t| t.id.clone()).expect("the sample thread");
    let task = s.tasks.iter().find(|t| t.id == id).cloned().expect("the sample thread");
    let at = start(s);
    let mut sc = Script::new(&task, "run_1", at);
    sc.person(&task.words);
    steps(&mut sc);
    sc.say(ANSWER, true);
    let mut book = Book { runs: vec![run("run_1", &task, RunStatus::Done, at, 29)], loaded: true, ..Book::default() };
    book.parts.extend(sc.parts);
    s.tasks.iter_mut().filter(|t| t.id == id).for_each(|t| t.status = TaskStatus::Done);
    s.books.insert(id.clone(), book);
    s.open = Some(id);
}

fn task_remove(s: &mut App) {
    workspace_task(s);
    let id = s.open.clone().expect("open");
    s.confirm = Some(crate::state::Confirm::RemoveTask(id));
}

fn task_remove_refused(s: &mut App) {
    workspace_task(s);
    s.refusal = Some(crate::state::Refusal {
        title: "Neko kept this thread".into(),
        words: "This thread's worktree has changes that are not committed. Neko deletes nothing with unsaved work.".into(),
        reveal: Some("/Users/example/Library/Application Support/neko/worktrees/x7k2m9qa".into()),
    });
}

fn settings_workspaces(s: &mut App) {
    with_workspaces(s);
    s.settings_open = true;
    s.settings_page = 1;
    s.runtimes = Some(agents(true, PermissionMode::Follow));
    s.undo_storage = Some(UndoStorage { bytes: 0, checkpoints: 0, tasks: 0 });
}

fn tasks_home(s: &mut App) {
    s.tasks = sample_tasks();
}

/// Six cats, the three starters and three more a person made, each with a face of its own.
pub fn six_cats() -> Vec<Cat> {
    let mut cats = sample_cats();
    for (id, name, face, purpose) in [("cat_nori", "nori", "void", "Keeps the servers tidy."), ("cat_yuzu", "yuzu", "siamese", "Answers the support inbox."), ("cat_sesame", "sesame", "smoke", "Looks into slow builds.")] {
        let mut c = cats[0].clone();
        (c.id, c.name, c.face, c.purpose) = (id.into(), name.into(), face.into(), Some(purpose.into()));
        cats.push(c);
    }
    cats
}

/// Home in a house of six with work under way: three busy (two working, one waiting on the person), three free.
/// The thread's overview as the daemon says it: its folder and branch, and the files all its runs changed, each with its own counts
/// (the totals: 42 added, 7 removed). The pane's Overview and its Changes tab badge read it.
fn receipt_files(s: &mut App, _run: &str, n: usize) {
    let all = [("src/toolbar.rs", 24, 4), ("src/menu.rs", 9, 2), ("docs/preferences.md", 3, 1), ("tests/labels.rs", 6, 0)];
    let take: Vec<_> = all.iter().take(n).collect();
    let mut files: Vec<DiffFile> = take.iter().map(|(p, a, r)| diff_file(p, FileStatus::Modified, *a, *r)).collect();
    // The rest of the totals goes to the first file, so the files add up to the run's figures.
    let (a, r): (u32, u32) = (files.iter().map(|f| f.added).sum(), files.iter().map(|f| f.removed).sum());
    files[0].added += 42 - a.min(42);
    files[0].removed += 7 - r.min(7);
    let task = s.open.clone().unwrap_or_else(|| s.tasks[1].id.clone());
    let changes = DiffSummary { task: task.clone(), run: None, source: neko_protocol::DiffSource::Thread, root: String::new(), files, added: 42, removed: 7 };
    s.pane.overview.insert(task.clone(), neko_protocol::Overview { task, folder: Some("/Users/example/Library/Application Support/neko/worktrees/x7k2m9qa".into()), branch: Some("neko/x7k2m9qa".into()), changes: Some(changes), uncommitted: None });
}

fn task_open(s: &mut App) {
    s.tasks = sample_tasks();
    s.open = Some(s.tasks[1].id.clone());
    s.books.insert(s.tasks[1].id.clone(), Book { loaded: true, ..Book::default() });
}

/// Builds a task's transcript in order, at a minute's pace from `start_ms`.
struct Script {
    task: TaskId,
    run: RunId,
    at: i64,
    seq: u64,
    parts: Vec<Part>,
    perms: HashMap<PermissionId, Permission>,
}

impl Script {
    fn new(task: &Task, run: &str, at: i64) -> Script {
        Script { task: task.id.clone(), run: RunId(run.into()), at, seq: 0, parts: Vec::new(), perms: HashMap::new() }
    }

    fn part(&mut self, body: PartBody, secs: i64, done: bool) -> &mut Self {
        self.seq += 1;
        let at = self.at;
        self.at += secs * 1000;
        self.parts.push(Part { id: PartId(format!("part_{}_{}", self.run.0, self.seq)), task: self.task.clone(), run: Some(self.run.clone()), seq: self.seq, at_ms: at, ended_ms: done.then_some(self.at), body });
        self
    }

    fn person(&mut self, words: &str) -> &mut Self {
        self.part(PartBody::Person { text: words.into() }, 1, true)
    }

    fn think(&mut self, words: &str, secs: i64) -> &mut Self {
        self.part(PartBody::Thinking { text: words.into(), done: true }, secs, true)
    }

    fn tool(&mut self, name: &str, summary: &str, input: &str, output: &str, state: ToolState, secs: i64) -> &mut Self {
        let done = state != ToolState::Running;
        self.part(PartBody::Tool { call: format!("call_{}", self.seq), name: name.into(), summary: summary.into(), input: input.into(), output: done.then(|| output.into()), state, exit: None }, secs, done)
    }

    /// A step that finished with this exit code, or was denied.
    fn tool_end(&mut self, name: &str, summary: &str, input: &str, output: &str, state: ToolState, exit: Option<i32>, secs: i64) -> &mut Self {
        self.part(PartBody::Tool { call: format!("call_{}", self.seq), name: name.into(), summary: summary.into(), input: input.into(), output: Some(output.into()), state, exit }, secs, true)
    }

    /// A thought with no words, as Claude Code sends them today.
    fn think_empty(&mut self) -> &mut Self {
        self.part(PartBody::Thinking { text: String::new(), done: true }, 1, true)
    }

    fn say(&mut self, words: &str, done: bool) -> &mut Self {
        self.part(PartBody::Agent { text: words.into(), done }, 4, done)
    }

    fn ask(&mut self, title: &str, why: &str, detail: &str, destructive: bool, state: PermissionState) -> &mut Self {
        let id = PermissionId(format!("perm_{}_{}", self.run.0, self.seq + 1));
        self.perms.insert(
            id.clone(),
            Permission { id: id.clone(), task: self.task.clone(), run: self.run.clone(), tool: "Bash".into(), title: title.into(), why: why.into(), detail: detail.into(), destructive, state, asked_ms: self.at, answered_ms: None },
        );
        self.part(PartBody::Permission { permission: id }, 6, true)
    }

    fn notice(&mut self, tone: Tone, words: &str, next: &str) -> &mut Self {
        self.part(PartBody::Notice { tone, text: words.into(), next: Some(next.into()), failure: None }, 0, true)
    }
}

const ANSWER: &str = "I renamed the button and its menu item, and kept **⌘,** as the shortcut.\n\n- `SettingsButton` is now `PreferencesButton` in `src/toolbar.rs`\n- The menu item reads **Preferences…**\n- ENG-123's check passes: the old label appears nowhere\n\n```rust\nButton::new(\"preferences\", \"Preferences\").keys(\"⌘,\")\n```\n\nNothing was merged or pushed. The work is on its own branch.";

fn run(id: &str, task: &Task, status: RunStatus, started: i64, secs: i64) -> Run {
    Run {
        id: RunId(id.into()),
        task: task.id.clone(),
        runtime: "claude".into(),
        model: Some("claude-sonnet-4-5".into()),
        status,
        started_ms: started,
        ended_ms: (status != RunStatus::Running).then_some(started + secs * 1000),
        session: Some("5f0c2a".into()),
        workdir: "/Users/example/Library/Application Support/neko/worktrees/x7k2m9qa".into(),
        branch: Some("neko/x7k2m9qa".into()),
        reason: None,
        usage: (status != RunStatus::Running).then(|| Usage {
            input_tokens: 38_400,
            output_tokens: 5_100,
            cached_tokens: 212_000,
            cost_usd: Some(0.42),
            model: Some("claude-sonnet-4-5".into()),
            pricing: Pricing::ApiEquivalent,
            reported_by: "Claude Code".into(),
            context_tokens: Some(48_200),
            context_window: Some(200_000),
        }),
        changes: (status == RunStatus::Done).then_some(Changes { files: 3, added: 42, removed: 7 }),
        undo: None,
        cat: task.cat.clone(),
        rules_hash: Some(neko_protocol::cats::rules_hash("")),
        ask: None,
    }
}

/// The open task (the second sample) with these runs and parts.
fn open_with(s: &mut App, status: TaskStatus, runs: Vec<Run>, scripts: Vec<Script>) {
    s.tasks = sample_tasks();
    s.tasks[1].status = status;
    s.tasks[1].folder = Some("/Users/example/acme-app".into());
    let id = s.tasks[1].id.clone();
    let mut book = Book { runs, loaded: true, ..Book::default() };
    for sc in scripts {
        book.parts.extend(sc.parts);
        book.permissions.extend(sc.perms);
    }
    s.books.insert(id.clone(), book);
    s.open = Some(id);
}

fn start(s: &App) -> i64 {
    s.now_ms - 47 * 60_000
}

fn steps(sc: &mut Script) {
    sc.think("The button is drawn in the toolbar and named again in the menu. I will find every place the old label appears before changing any of them, so nothing is left half renamed.", 4);
    sc.tool("Grep", "Settings", "{\n  \"pattern\": \"Settings\"\n}", "src/toolbar.rs:12\nsrc/menu.rs:48\ntests/labels.rs:7", ToolState::Done, 1);
    sc.tool("Read", "src/toolbar.rs", "{\n  \"file_path\": \"src/toolbar.rs\"\n}", "pub fn toolbar() -> El {\n    row().child(Button::new(\"settings\", \"Settings\"))\n}", ToolState::Done, 1);
    sc.tool("Edit", "src/toolbar.rs", "{\n  \"file_path\": \"src/toolbar.rs\"\n}", "The file was edited.", ToolState::Done, 2);
}

fn task_working(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    steps(&mut sc);
    sc.tool("Edit", "src/menu.rs", "{\n  \"file_path\": \"src/menu.rs\"\n}", "The file was edited.", ToolState::Done, 2);
    sc.say("I renamed the button in the toolbar and the menu item. Now I am checking that the old label appears", false);
    let r = run("run_1", task, RunStatus::Running, start(s), 0);
    open_with(s, TaskStatus::Working, vec![r], vec![sc]);
}

/// A run in progress, in the sidebar's company: a thought, three steps, the answer streaming.
fn task_working_live(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    sc.think("The button is drawn in the toolbar and named again in the menu. I will find every place the old label appears first.", 4);
    sc.tool("Grep", "Settings", "{\n  \"pattern\": \"Settings\"\n}", "src/toolbar.rs:12\nsrc/menu.rs:48", ToolState::Done, 1);
    sc.tool("Edit", "src/toolbar.rs", "{\n  \"file_path\": \"src/toolbar.rs\"\n}", "The file was edited.", ToolState::Done, 2);
    sc.tool("Edit", "src/menu.rs", "{\n  \"file_path\": \"src/menu.rs\"\n}", "The file was edited.", ToolState::Done, 2);
    sc.say("I renamed the button in the toolbar and the menu item. Now I am checking that the old label appears", false);
    let r = run("run_1", task, RunStatus::Running, start(s), 0);
    open_with(s, TaskStatus::Working, vec![r], vec![sc]);
}

fn task_needs_you(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    steps(&mut sc);
    sc.ask("Run a command", "Commands can change anything on this Mac.", "cargo test --package acme-app labels", false, PermissionState::Pending);
    let r = run("run_1", task, RunStatus::Running, start(s), 0);
    open_with(s, TaskStatus::NeedsYou, vec![r], vec![sc]);
}

fn task_needs_you_destructive(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    steps(&mut sc);
    sc.ask("Run a command that deletes or overwrites", "This command deletes or overwrites things.", "rm -rf target/old-screenshots", true, PermissionState::Pending);
    let r = run("run_1", task, RunStatus::Running, start(s), 0);
    open_with(s, TaskStatus::NeedsYou, vec![r], vec![sc]);
}

fn done_script(s: &App, task: &Task) -> Script {
    let mut sc = Script::new(task, "run_1", start(s));
    steps(&mut sc);
    sc.ask("Run a command", "Commands can change anything on this Mac.", "cargo test --package acme-app labels", false, PermissionState::Allowed);
    sc.tool("Bash", "cargo test --package acme-app labels", "cargo test --package acme-app labels", "test result: ok. 14 passed; 0 failed", ToolState::Done, 38);
    sc.say(ANSWER, true);
    sc
}

fn task_done(s: &mut App) {
    let task = &sample_tasks()[1];
    let sc = done_script(s, task);
    let r = run("run_1", task, RunStatus::Done, start(s), 134);
    open_with(s, TaskStatus::Done, vec![r], vec![sc]);
    receipt_files(s, "run_1", 3);
}

fn task_done_open(s: &mut App) {
    task_done(s);
    s.open_folds.insert("worked-run_1".into());
    s.open_folds.insert("part-1".into());
}

fn task_follow_up(s: &mut App) {
    let task = &sample_tasks()[1];
    let sc = done_script(s, task);
    let first = run("run_1", task, RunStatus::Done, start(s), 134);
    let mut sc2 = Script::new(task, "run_2", start(s) + 20 * 60_000);
    sc2.seq = sc.seq;
    sc2.person("Also rename it in the help pages.");
    sc2.tool("Grep", "Settings docs/", "{\n  \"pattern\": \"Settings\",\n  \"path\": \"docs/\"\n}", "docs/help.md:31", ToolState::Done, 1);
    sc2.tool("Edit", "docs/help.md", "{\n  \"file_path\": \"docs/help.md\"\n}", "The file was edited.", ToolState::Done, 1);
    sc2.say("The help page now says **Preferences** too. That was the last place.", true);
    let mut second = run("run_2", task, RunStatus::Done, start(s) + 20 * 60_000, 31);
    second.changes = Some(Changes { files: 4, added: 44, removed: 9 });
    if let Some(u) = &mut second.usage {
        u.cost_usd = Some(0.06);
        u.input_tokens = 9_800;
        u.output_tokens = 640;
    }
    open_with(s, TaskStatus::Done, vec![first, second], vec![sc, sc2]);
}

fn task_failed(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    sc.think("I will look for the button first.", 2);
    sc.tool("Grep", "Settings", "{\n  \"pattern\": \"Settings\"\n}", "src/toolbar.rs:12", ToolState::Done, 1);
    sc.notice(Tone::Error, "Claude Code has reached its usage limit.", "It resets at 17:00. Send a follow-up then to carry on.");
    let mut r = run("run_1", task, RunStatus::Failed, start(s), 9);
    r.reason = Some("Claude Code has reached its usage limit.".into());
    r.usage = Some(Usage { input_tokens: 2_100, output_tokens: 90, cached_tokens: 0, cost_usd: None, model: None, pricing: Pricing::ApiEquivalent, reported_by: "Claude Code".into(), context_tokens: None, context_window: None });
    open_with(s, TaskStatus::Failed, vec![r], vec![sc]);
}

fn task_stopped(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    steps(&mut sc);
    sc.say("I renamed the button in the toolbar. Next I will", true);
    let mut r = run("run_1", task, RunStatus::Stopped, start(s), 41);
    r.reason = Some("You stopped it.".into());
    r.changes = Some(Changes { files: 1, added: 1, removed: 1 });
    open_with(s, TaskStatus::Stopped, vec![r], vec![sc]);
}

fn task_interrupted(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    steps(&mut sc);
    sc.say("I renamed the button in the toolbar and", true);
    let mut r = run("run_1", task, RunStatus::Interrupted, start(s), 52);
    r.reason = Some("Neko stopped while this was running.".into());
    r.usage = None;
    let mut loose = Script::new(task, "none", start(s) + 60_000);
    loose.notice(Tone::Warning, "Neko was quit while this was running. Everything it said is kept.", "Send a follow-up to carry on.");
    for p in &mut loose.parts {
        p.run = None;
        p.seq += 100;
    }
    open_with(s, TaskStatus::Interrupted, vec![r], vec![sc, loose]);
}

fn agents(signed_in: bool, mode: PermissionMode) -> (Vec<RuntimeInfo>, Settings) {
    (
        vec![
            RuntimeInfo {
                id: "claude".into(),
                name: "Claude Code".into(),
                found: true,
                version: Some("2.1.0".into()),
                signed_in: Some(signed_in),
                login: signed_in.then(|| "Claude subscription (Max)".into()),
                sign_in: "claude auth login".into(),
                install: "curl -fsSL https://claude.ai/install.sh | bash".into(),
                supported: true,
                rules: Some("Your Claude Code settings allow Bash, Edit and Write without asking.".into()),
            },
            RuntimeInfo {
                id: "codex".into(),
                name: "Codex".into(),
                found: false,
                version: None,
                signed_in: None,
                login: None,
                sign_in: "codex login".into(),
                install: "npm install -g @openai/codex".into(),
                supported: false,
                rules: None,
            },
        ],
        Settings { runtime: "claude".into(), permission_mode: mode, own_mcp: false, queue_first: false, ask_first_subagent: false, cattains_by_hand: false },
    )
}

fn settings(s: &mut App) {
    s.tasks = sample_tasks();
    s.settings_open = true;
    s.runtimes = Some(agents(true, PermissionMode::Follow));
}

fn settings_ask(s: &mut App) {
    settings(s);
    s.runtimes = Some(agents(true, PermissionMode::Ask));
}

fn settings_full(s: &mut App) {
    settings(s);
    s.runtimes = Some(agents(true, PermissionMode::Full));
}

fn settings_signed_out(s: &mut App) {
    settings(s);
    s.runtimes = Some(agents(false, PermissionMode::Follow));
}

fn task_thinking(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    sc.tool("Grep", "Settings", "{\n  \"pattern\": \"Settings\"\n}", "src/toolbar.rs:12\nsrc/menu.rs:48", ToolState::Done, 1);
    sc.think_empty();
    let r = run("run_1", task, RunStatus::Running, start(s), 0);
    open_with(s, TaskStatus::Working, vec![r], vec![sc]);
    if let Some(b) = s.books.get_mut(&s.tasks[1].id) {
        b.thinking = true;
    }
}

/// A scratch folder's run: no branch, nothing changed.
fn scratch(mut r: Run) -> Run {
    r.workdir = "/Users/example/Library/Application Support/neko/scratch/x7k2m9qa".into();
    r.branch = None;
    r.changes = None;
    r
}

fn task_talked(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    sc.think_empty();
    sc.say("Preferences is the usual word on a Mac since macOS Ventura renamed it to Settings in the system, so **Settings** now matches the system and is the better choice. I would keep it.", true);
    let r = scratch(run("run_1", task, RunStatus::Done, start(s), 29));
    open_with(s, TaskStatus::Done, vec![r], vec![sc]);
}

/// A run that ends with a question for the person: the task is theirs to answer.
fn task_asked(s: &mut App) {
    let task = &sample_tasks()[1];
    let mut sc = Script::new(task, "run_1", start(s));
    sc.think_empty();
    sc.say("The button is called **Settings** in three places and **Preferences** in one. Should I rename all four to Preferences, or keep Settings to match macOS?", true);
    let r = scratch(run("run_1", task, RunStatus::Done, start(s), 14));
    open_with(s, TaskStatus::Asked, vec![r], vec![sc]);
}

/// Two runs with the shapes a real run showed: empty thoughts, words between tools, a long path under home, a check that exits 1,
/// a step denied.
fn real_run(s: &mut App) {
    let task = &sample_tasks()[1];
    let t0 = start(s);
    let mut sc = Script::new(task, "run_1", t0);
    sc.think_empty();
    sc.tool("Read", "/Users/example/Documents/acme-app/docs/guides/getting-started/installing-on-a-new-mac.md", "{\n  \"file_path\": \"/Users/example/Documents/acme-app/docs/guides/getting-started/installing-on-a-new-mac.md\"\n}", "# Installing\nSteps.", ToolState::Done, 0);
    sc.think_empty();
    sc.say("The guide is current. I will check the branches next.", true);
    sc.tool("Bash", "git status --short", "git status --short && git branch --list 'neko/*' --format='%(refname:short) %(upstream:track)' && git log --oneline -5 --decorate", "?? notes.txt\nneko/x7k2m9qa [ahead 1]", ToolState::Done, 2);
    sc.say("One branch is ahead. Checking whether anything is left to commit.", true);
    sc.tool_end("Bash", "git diff --quiet", "git diff --quiet && git diff --cached --quiet", "Exit code 1", ToolState::Done, Some(1), 0);
    sc.tool_end("Bash", "git stash drop", "git stash drop", "PreToolUse:Bash hook error: The person said no to this step in Neko.", ToolState::Denied, None, 0);
    sc.think_empty();
    sc.say("There are uncommitted changes, so I left them as they are.\n\n- `notes.txt` is new and untracked\n- `neko/x7k2m9qa` is one commit ahead", true);
    let first = scratch(run("run_1", task, RunStatus::Done, t0, 9));
    let t1 = t0 + 10 * 60_000;
    let mut sc2 = Script::new(task, "run_2", t1);
    sc2.seq = sc.seq;
    sc2.person("Commit notes.txt on that branch.");
    sc2.think_empty();
    sc2.tool("Bash", "git add notes.txt", "git add notes.txt && git commit -m 'Add notes'", "[neko/x7k2m9qa 4d5e6f7] Add notes\n 1 file changed, 2 insertions(+)", ToolState::Done, 1);
    sc2.say("Committed `notes.txt` on `neko/x7k2m9qa`. Nothing was pushed.", true);
    let mut second = scratch(run("run_2", task, RunStatus::Done, t1, 29));
    if let Some(u) = &mut second.usage {
        u.context_tokens = Some(61_400);
        u.cost_usd = Some(0.54);
    }
    open_with(s, TaskStatus::Done, vec![first, second], vec![sc, sc2]);
}

fn task_real_run(s: &mut App) {
    real_run(s);
}

fn task_real_run_open(s: &mut App) {
    real_run(s);
    s.open_folds.insert("worked-run_1".into());
}

fn task_context_open(s: &mut App) {
    real_run(s);
    s.context_open = true;
}

/// A finished run with its checkpoints: what it changed itself.
fn with_undo(mut r: Run, state: UndoState, files: u32) -> Run {
    r.undo = Some(RunUndo { state, why: None, changes: Some(Changes { files, added: 42, removed: 7 }), committed: false });
    r
}

/// The done task, its run undoable.
fn task_undo(s: &mut App) {
    let task = &sample_tasks()[1];
    let sc = done_script(s, task);
    let r = with_undo(run("run_1", task, RunStatus::Done, start(s), 134), UndoState::Applied, 4);
    open_with(s, TaskStatus::Done, vec![r], vec![sc]);
    receipt_files(s, "run_1", 4);
}

fn task_undone(s: &mut App) {
    let task = &sample_tasks()[1];
    let sc = done_script(s, task);
    let r = with_undo(run("run_1", task, RunStatus::Done, start(s), 134), UndoState::Undone, 4);
    open_with(s, TaskStatus::Done, vec![r], vec![sc]);
    receipt_files(s, "run_1", 4);
}

/// Two runs that changed files: the older one's Undo waits for the newer one.
fn two_runs(s: &App, task: &Task, second: UndoState) -> (Vec<Run>, Vec<Script>) {
    let sc = done_script(s, task);
    let first = with_undo(run("run_1", task, RunStatus::Done, start(s), 134), UndoState::Applied, 4);
    let mut sc2 = Script::new(task, "run_2", start(s) + 20 * 60_000);
    sc2.seq = sc.seq;
    sc2.person("Also rename it in the help pages.");
    sc2.tool("Edit", "docs/help.md", "{\n  \"file_path\": \"docs/help.md\"\n}", "The file was edited.", ToolState::Done, 1);
    sc2.say("The help page now says **Preferences** too.", true);
    let mut r2 = run("run_2", task, RunStatus::Done, start(s) + 20 * 60_000, 31);
    r2.undo = Some(RunUndo { state: second, why: None, changes: Some(Changes { files: 1, added: 2, removed: 2 }), committed: false });
    (vec![first, r2], vec![sc, sc2])
}

fn task_undo_older(s: &mut App) {
    let task = &sample_tasks()[1];
    let (runs, scripts) = two_runs(s, task, UndoState::Applied);
    open_with(s, TaskStatus::Done, runs, scripts);
    s.open_folds.insert("worked-run_2".into());
}

fn task_undo_committed(s: &mut App) {
    let task = &sample_tasks()[1];
    let sc = done_script(s, task);
    let mut r = with_undo(run("run_1", task, RunStatus::Done, start(s), 134), UndoState::Applied, 4);
    if let Some(u) = &mut r.undo {
        u.committed = true;
    }
    open_with(s, TaskStatus::Done, vec![r], vec![sc]);
}

fn task_undo_unavailable(s: &mut App) {
    let task = &sample_tasks()[1];
    let sc = done_script(s, task);
    let mut r = run("run_1", task, RunStatus::Done, start(s), 134);
    r.undo = Some(RunUndo { state: UndoState::Unavailable, why: Some("the folder has 12,480 new or changed files (310.2 MB), more than undo keeps (10,000 files or 100.0 MB)".into()), changes: None, committed: false });
    open_with(s, TaskStatus::Done, vec![r], vec![sc]);
}

fn task_undo_dropped(s: &mut App) {
    let task = &sample_tasks()[1];
    let (mut runs, mut scripts) = two_runs(s, task, UndoState::Applied);
    if let Some(u) = &mut runs[0].undo {
        u.state = UndoState::Dropped;
    }
    // The notice the follow-up left when it started from the undone files, right after the person's words.
    let person = scripts[1].parts[0].clone();
    for p in &mut scripts[1].parts[1..] {
        p.seq += 1;
    }
    let notice = Part {
        id: PartId("part_redo_dropped".into()),
        task: task.id.clone(),
        run: None,
        seq: person.seq + 1,
        at_ms: person.at_ms,
        ended_ms: Some(person.at_ms),
        body: PartBody::Notice { tone: Tone::Info, text: "This message starts from the undone files, so the undone run can no longer be redone.".into(), next: None, failure: None },
    };
    scripts[1].parts.insert(1, notice);
    open_with(s, TaskStatus::Done, runs, scripts);
}

fn undo_anyway(s: &mut App) {
    task_undo(s);
    let task = s.tasks[1].id.clone();
    s.confirm = Some(crate::state::Confirm::UndoAnyway { task, run: RunId("run_1".into()), redo: false, files: vec!["src/toolbar.rs".into(), "docs/help.md".into()] });
}

fn diff_file(path: &str, status: FileStatus, added: u32, removed: u32) -> DiffFile {
    DiffFile { path: path.into(), old_path: None, status, added, removed, binary: false, old_size: None, new_size: None, old_mode: None, new_mode: None }
}

/// What the review shows: several files, a renamed one, a binary one, a script made executable.
fn review_files() -> Vec<DiffFile> {
    let mut moved = diff_file("docs/preferences.md", FileStatus::Renamed, 3, 3);
    moved.old_path = Some("docs/settings.md".into());
    let mut icon = diff_file("assets/preferences.png", FileStatus::Modified, 0, 0);
    icon.binary = true;
    (icon.old_size, icon.new_size) = (Some(2_400), Some(3_150));
    let mut script = diff_file("scripts/check-labels.sh", FileStatus::Added, 6, 0);
    (script.old_mode, script.new_mode) = (None, None);
    vec![diff_file("src/toolbar.rs", FileStatus::Modified, 3, 2), diff_file("src/menu.rs", FileStatus::Modified, 1, 1), moved, icon, script, diff_file("tests/old_labels.rs", FileStatus::Deleted, 0, 18)]
}

fn review_with(s: &mut App, run: Option<&str>, open: &[&str]) {
    let task = s.tasks[1].id.clone();
    let mut r = crate::state::Review::new(task.clone(), if run.is_some() { neko_protocol::DiffSource::Run } else { neko_protocol::DiffSource::Thread }, run.map(|r| RunId(r.into())), Some(RunId("run_1".into())));
    let files = review_files();
    let (added, removed) = (files.iter().map(|f| f.added).sum(), files.iter().map(|f| f.removed).sum());
    r.summary = Some(DiffSummary { task, run: r.run.clone(), source: r.source, root: "/Users/example/Library/Application Support/neko/worktrees/x7k2m9qa".into(), files, added, removed });
    let toolbar = [
        "@@ -9,8 +9,9 @@ use crate::ui::button::Button;",
        " ",
        " /// The toolbar at the top of the window.",
        " pub fn toolbar() -> El {",
        "-    row().child(Button::new(\"settings\", \"Settings\"))",
        "-        .child(Button::new(\"help\", \"Help\"))",
        "+    row()",
        "+        .child(Button::new(\"preferences\", \"Preferences\").keys(\"⌘,\"))",
        "+        .child(Button::new(\"help\", \"Help\"))",
        " }",
        " ",
        " #[cfg(test)]",
    ];
    r.patches.insert("src/toolbar.rs".into(), DiffPatch { path: "src/toolbar.rs".into(), lines: toolbar.iter().map(|l| (*l).to_owned()).collect(), total: toolbar.len() as u32, cut: false });
    let moved = ["@@ -1,6 +1,6 @@", "-# Settings", "+# Preferences", " ", "-Open Settings with ⌘, to change how Acme looks.", "+Open Preferences with ⌘, to change how Acme looks.", " ", "-Settings are saved as you go.", "+Preferences are saved as you go."];
    r.patches.insert("docs/preferences.md".into(), DiffPatch { path: "docs/preferences.md".into(), lines: moved.iter().map(|l| (*l).to_owned()).collect(), total: 4_210, cut: true });
    for p in open {
        r.open.insert((*p).to_owned());
    }
    // The Changes tab of the pane, open.
    let root = r.task.clone();
    s.pane.choice = Some(true);
    s.pane.sheet = true;
    s.pane.tab.insert(root, crate::pane::Tab::Changes);
    s.review = Some(r);
}

/// The pane open on a tab of the open thread.
fn open_pane(s: &mut App, tab: crate::pane::Tab) {
    let Some(root) = s.pane_root() else { return };
    s.pane.choice = Some(true);
    s.pane.sheet = true;
    s.pane.tab.insert(root, tab);
}

/// The done thread with its changes, Overview open: the figures and files, Review and Undo, the folder and branch, and the usage.
fn pane_overview(s: &mut App) {
    task_undo(s);
    open_pane(s, crate::pane::Tab::Overview);
}

/// The same, undone: the figures are quiet, the one line says so, and Redo is offered.
fn pane_overview_undone(s: &mut App) {
    task_undone(s);
    open_pane(s, crate::pane::Tab::Overview);
}

/// A thread nothing has happened in: Overview says so in one quiet line.
fn pane_overview_empty(s: &mut App) {
    task_open(s);
    s.tasks[1].folder = None;
    open_pane(s, crate::pane::Tab::Overview);
}

/// A run that changed many files: five are listed, the rest behind Show all.
fn pane_overview_many(s: &mut App) {
    task_undo(s);
    let task = s.tasks[1].id.clone();
    let mut files = review_files();
    files.push(diff_file("src/theme.rs", FileStatus::Modified, 5, 1));
    let (added, removed) = (files.iter().map(|f| f.added).sum(), files.iter().map(|f| f.removed).sum());
    if let Some(o) = s.pane.overview.get_mut(&task) {
        o.changes = Some(DiffSummary { task: task.clone(), run: None, source: neko_protocol::DiffSource::Thread, root: String::new(), files, added, removed });
    }
    open_pane(s, crate::pane::Tab::Overview);
}

/// Changes on its third source: what is not committed in the folder now.
fn pane_changes_uncommitted(s: &mut App) {
    task_undo(s);
    review_with(s, None, &["src/toolbar.rs"]);
    if let Some(r) = &mut s.review {
        r.source = neko_protocol::DiffSource::Uncommitted;
        if let Some(d) = &mut r.summary {
            d.source = neko_protocol::DiffSource::Uncommitted;
        }
    }
}

/// Changes on Uncommitted for a folder that is not a repository: the daemon's words.
fn pane_changes_not_a_repository(s: &mut App) {
    task_undo(s);
    review_with(s, None, &[]);
    if let Some(r) = &mut s.review {
        r.source = neko_protocol::DiffSource::Uncommitted;
        r.summary = None;
        r.error = Some("This folder is not a git repository with commits, so there is nothing to compare with.".into());
    }
}

fn review(s: &mut App) {
    task_undo(s);
    review_with(s, Some("run_1"), &[]);
}

fn review_open(s: &mut App) {
    task_undo(s);
    review_with(s, Some("run_1"), &["src/toolbar.rs", "docs/preferences.md", "assets/preferences.png"]);
}

fn review_all(s: &mut App) {
    task_undo_older(s);
    review_with(s, None, &["src/toolbar.rs"]);
}

fn review_undone(s: &mut App) {
    task_undone(s);
    review_with(s, Some("run_1"), &["src/toolbar.rs"]);
}

fn settings_undo(s: &mut App) {
    settings_workspaces(s);
    s.undo_storage = Some(UndoStorage { bytes: 18_400_000, checkpoints: 38, tasks: 6 });
}

fn clear_undo(s: &mut App) {
    settings_undo(s);
    s.confirm = Some(crate::state::Confirm::ClearUndo);
}

// ---- Rounds (docs/knowledge/rounds.md) ----

const HOUR: i64 = 3_600_000;

/// The day's 09:00 (UTC, the scenes' zone) `days` after today.
fn nine(s: &App, days: i64) -> i64 {
    s.now_ms - s.now_ms.rem_euclid(86_400_000) + days * 86_400_000 + 9 * HOUR
}

fn round(s: &App, n: u32, name: &str, words: &str, cat: &str, workspace: &str, schedule: Schedule) -> Round {
    let mut r = Round {
        id: format!("round_{n:026}"),
        name: name.into(),
        words: words.into(),
        cat: cat.into(),
        workspace: workspace.into(),
        schedule,
        on: true,
        created_ms: s.now_ms - 20 * 86_400_000 + i64::from(n),
        changed_ms: s.now_ms - 20 * 86_400_000,
        status: RoundStatus::Scheduled,
        next_due_ms: None,
        note: None,
        last: None,
    };
    r.next_due_ms = r.schedule.next_due(r.created_ms, s.now_ms, &*s.zone());
    r
}

fn line(r: &Round, n: u32, due: Option<i64>, at: i64, kind: RunKind, task: Option<&Task>, end: Option<RunEnd>, reason: Option<&str>) -> RoundRun {
    RoundRun { id: format!("rrun_{n:026}"), round: r.id.clone(), due_ms: due, at_ms: at, kind, task: task.map(|t| t.id.clone()), reason: reason.map(str::to_owned), end }
}

/// Three rounds: weekdays at 09:00 that went well this morning, every six hours whose last time was skipped, and one by hand.
fn with_rounds(s: &mut App) {
    with_workspaces(s);
    let weekdays = Schedule::Weekdays { days: vec![Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri], at: "09:00".into() };
    let mut docs = round(s, 1, "Docs check", "Check the docs for broken links and fix the ones you can.", "cat_mochi", "ws_acme", weekdays);
    let mut notes = round(s, 2, "Release notes", "Draft the release notes from this week's merged changes.", "cat_miso", GENERAL, Schedule::Every { hours: 6 });
    // Made an hour and a half ago: its six hours count from then.
    notes.created_ms = s.now_ms - 90 * 60_000;
    notes.next_due_ms = notes.schedule.next_due(notes.created_ms, s.now_ms, &*s.zone());
    let mut tidy = round(s, 3, "Tidy downloads", "Move old files from Downloads into Archive.", "cat_tofu", GENERAL, Schedule::Manual);
    tidy.status = RoundStatus::Manual;
    // This morning's task for the docs check, done; the notes round's last time was skipped while its task still worked.
    let mut done = task(40, "Check the docs for broken links and fix the ones you can.", 5 * 60);
    done.title = "Docs check".into();
    done.workspace = "ws_acme".into();
    done.folder = Some("/Users/example/acme-api".into());
    done.cat = Some("cat_mochi".into());
    done.status = TaskStatus::Done;
    done.round = Some(TaskRound { id: docs.id.clone(), name: docs.name.clone(), due_ms: Some(nine(s, 0)) });
    let mut working = task(41, "Draft the release notes from this week's merged changes.", 90);
    working.title = "Release notes".into();
    working.cat = Some("cat_miso".into());
    working.status = TaskStatus::Working;
    working.round = Some(TaskRound { id: notes.id.clone(), name: notes.name.clone(), due_ms: Some(s.now_ms - 90 * 60_000) });
    docs.last = Some(line(&docs, 1, Some(nine(s, 0)), nine(s, 0), RunKind::Started, Some(&done), Some(RunEnd::Done), None));
    notes.last = Some(line(&notes, 2, Some(s.now_ms - 20 * 60_000), s.now_ms - 20 * 60_000, RunKind::Skipped, None, None, Some("The last run is still working")));
    s.tasks.push(done);
    s.tasks.push(working);
    s.tasks.sort_by_key(|t| std::cmp::Reverse(t.created_ms));
    s.rounds = vec![docs, notes, tidy];
    s.rounds_loaded = true;
}

fn rounds_empty(s: &mut App) {
    with_workspaces(s);
    s.rounds_loaded = true;
    s.open_rounds();
}

fn rounds(s: &mut App) {
    with_rounds(s);
    s.open_rounds();
}

fn rounds_history(s: &mut App) {
    with_rounds(s);
    s.open_rounds();
    let docs = s.rounds[0].clone();
    let done = s.tasks.iter().find(|t| t.title == "Docs check").cloned().expect("the sample thread");
    s.round_history_open.insert(docs.id.clone());
    s.round_history.insert(
        docs.id.clone(),
        vec![
            line(&docs, 1, Some(nine(s, 0)), nine(s, 0), RunKind::Started, Some(&done), Some(RunEnd::Done), None),
            line(&docs, 2, Some(nine(s, -1)), nine(s, -1), RunKind::Started, Some(&done), Some(RunEnd::Failed), None),
            line(&docs, 3, Some(nine(s, -2)), nine(s, -2), RunKind::Skipped, None, None, Some("Every round was paused")),
            line(&docs, 4, Some(nine(s, -5)), nine(s, -3) + HOUR, RunKind::Started, Some(&done), Some(RunEnd::Done), None),
            line(&docs, 5, Some(nine(s, -6)), nine(s, -3) + HOUR, RunKind::Missed, None, None, Some("Neko was not running, or this Mac was asleep")),
        ],
    );
}

fn rounds_paused(s: &mut App) {
    with_rounds(s);
    s.rounds_paused = true;
    for r in &mut s.rounds {
        if r.on && r.schedule.is_automatic() {
            r.status = RoundStatus::Paused;
            r.next_due_ms = None;
        }
    }
    s.open_rounds();
}

fn rounds_braked(s: &mut App) {
    with_rounds(s);
    let r = &mut s.rounds[0];
    r.on = false;
    r.status = RoundStatus::Braked;
    r.next_due_ms = None;
    r.note = Some("Turned itself off after 3 failed runs in a row. Turn it back on when it is fixed.".into());
    let n = &mut s.rounds[1];
    n.status = RoundStatus::CatRetired;
    n.next_due_ms = None;
    n.note = Some("miso is retired.".into());
    let mut folder = round(s, 4, "Check the notes", "Sort the meeting notes by project.", "cat_tofu", "ws_notes", Schedule::Daily { at: "18:30".into() });
    folder.status = RoundStatus::FolderMissing;
    folder.next_due_ms = None;
    folder.note = Some("The folder of notes was not found.".into());
    s.rounds.push(folder);
    s.open_rounds();
}

fn rounds_remove(s: &mut App) {
    with_rounds(s);
    s.open_rounds();
    let id = s.rounds[0].id.clone();
    s.ask_remove_round(&id);
}

fn round_new(s: &mut App) {
    with_rounds(s);
    s.open_rounds();
    s.start_new_round(Some("cat_mochi"));
    let f = s.form_mut();
    f.name = TextEdit::new("Link check");
    f.words = TextEdit::new("Check the docs for broken links in acme-api and list the ones that need a person.");
    f.workspace = "ws_acme".into();
}

fn round_daily(s: &mut App) {
    round_new(s);
    let f = s.form_mut();
    f.kind = 0;
    f.time = TextEdit::new("18:30");
}

fn round_hours(s: &mut App) {
    round_new(s);
    let f = s.form_mut();
    f.kind = 2;
    f.hours = TextEdit::new("6");
}

fn round_manual(s: &mut App) {
    round_new(s);
    s.form_mut().kind = 3;
}

fn round_edit(s: &mut App) {
    with_rounds(s);
    s.open_rounds();
    let id = s.rounds[0].id.clone();
    s.start_edit_round(&id);
}

fn round_invalid(s: &mut App) {
    round_new(s);
    let f = s.form_mut();
    f.time = TextEdit::new("9am");
    f.error = Some("Neko could not save the round.".into());
}

fn round_cat_menu(s: &mut App) {
    round_new(s);
    s.menu_open = Some("round-cat-menu".into());
}

fn task_from_round(s: &mut App) {
    with_rounds(s);
    let id = s.tasks.iter().find(|t| t.title == "Docs check").map(|t| t.id.clone()).expect("the sample thread");
    let task = s.tasks.iter().find(|t| t.id == id).cloned().expect("the sample thread");
    let mut sc = Script::new(&task, "run_1", start(s));
    steps(&mut sc);
    sc.say(ANSWER, true);
    let r = run("run_1", &task, RunStatus::Done, start(s), 134);
    s.books.insert(id.clone(), Book { runs: vec![r], loaded: true, parts: sc.parts, ..Book::default() });
    s.open = Some(id);
    // The run changed files: its receipt lists each with its figures, as an ordinary task's does.
    receipt_files(s, "run_1", 3);
}


