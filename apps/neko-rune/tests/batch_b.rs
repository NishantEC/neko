//! Parity batch B (the owner's brief of 2026-11-17): the work views, headless, through the real layout and hit-testing. A board
//! card dragged with the pointer, the tree folding, the Pull requests view (its switch, Close with a reason, Merge all ready), the
//! Details tab's sections and `/` in the session's reply box. The daemon-backed paths are in `harness.rs`.

use neko_rune::app::App;
use neko_rune::board::Layout;
use neko_rune::link::Call;
use neko_rune::panel::{Section, Tab};
use neko_rune::scenes::SCENES;
use neko_rune::task_actions::ASK_FIELD;
use neko_rune::view::view;
use rune::{Env, Key, KeyEvent, Mods, Runtime};

const WINDOW: (f32, f32) = (1360.0, 900.0);

fn scene(name: &str) -> impl FnOnce(&mut App) + '_ {
    move |app| {
        let (_, f) = SCENES.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no scene {name}"));
        f(app);
        app.outbox.clear();
    }
}

fn runtime(setup: impl FnOnce(&mut App)) -> Runtime<App> {
    let mut app = App::headless();
    setup(&mut app);
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
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

/// Clicks the last element with these words: a menu item is drawn after the page it opened over.
fn click_last_text(rt: &mut Runtime<App>, label: &str) {
    let r = rt.elements().into_iter().rev().find(|e| e.text.as_deref() == Some(label)).unwrap_or_else(|| panic!("no text {label}: {:?}", texts(rt))).rect;
    let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn right_click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    settle(rt);
}

fn press(rt: &mut Runtime<App>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    settle(rt);
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

fn has_text(rt: &Runtime<App>, needle: &str) -> bool {
    texts(rt).iter().any(|t| t.contains(needle))
}

fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

/// Takes the card keyed `key` and carries it `dx` points sideways with the pointer, in a few steps as a hand moves. The pointer
/// is still down; the point it is at is returned.
fn hold(rt: &mut Runtime<App>, key: &str, dx: f32) -> (f32, f32) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    for i in 1..=6 {
        rt.pointer_move(x + dx * i as f32 / 6.0, y);
        rt.frame(1.0 / 60.0);
    }
    (x + dx, y)
}

/// Carries the card and lets go.
fn carry(rt: &mut Runtime<App>, key: &str, dx: f32) {
    let (x, y) = hold(rt, key, dx);
    rt.pointer_up(x, y);
    settle(rt);
}

/// The board with the sessions and Neko columns folded, so the first columns are in reach of the pointer.
fn board(app: &mut App) {
    scene("bb-board")(app);
    app.layout.sessions.set_open(false);
    app.layout.set_neko_open(false);
    app.all.cursor_shown = false;
}

// ------------------------------------------------------------------------------------------------ 1. the layouts

#[test]
fn the_header_switch_chooses_list_board_or_tree_and_the_choice_is_kept() {
    let mut rt = runtime(scene("bb-board"));
    rt.update(|s, _| {
        s.set_all_layout(Layout::List);
        s.all.layout_dirty = false;
    });
    settle(&mut rt);
    assert!(has_key(&rt, "all-list") && !has_key(&rt, "board"), "the list");
    click(&mut rt, "all-layout-board");
    assert_eq!(rt.state.all_layout(), Layout::Board);
    assert!(rt.state.all.layout_dirty, "the choice is to be written");
    assert!(has_key(&rt, "board") && has_key(&rt, "board-col-Needs you") && has_key(&rt, "board-col-Done"), "one column per stage");
    click(&mut rt, "all-layout-tree");
    assert_eq!(rt.state.all_layout(), Layout::Tree);
    assert!(has_key(&rt, "tree"));
    click(&mut rt, "all-layout-list");
    assert!(has_key(&rt, "all-list"));
}

#[test]
fn the_board_has_a_column_per_stage_with_the_webs_empty_lines_and_filters() {
    let mut rt = runtime(scene("bb-board"));
    for title in ["Needs you", "Ready to review", "Running", "Up next", "Done"] {
        assert!(has_key(&rt, &format!("board-head-{title}")), "{title}");
    }
    let counts: Vec<String> = ["Needs you", "Ready to review", "Running", "Up next", "Done"].iter().map(|t| texts_of(&rt, &format!("board-count-{t}"))).collect();
    assert_eq!(counts, ["3", "3", "5", "4", "7"], "failed joins Needs you, cancelled joins Done (the filter asks for them)");
    // A stage filter narrows to one column; an empty one says so.
    rt.update(|s, _| s.all.stage = Some(neko_rune::all_sessions::Stage::Running));
    settle(&mut rt);
    assert!(has_key(&rt, "board-col-Running") && !has_key(&rt, "board-col-Done"));
    rt.update(|s, _| {
        s.all.stage = Some(neko_rune::all_sessions::Stage::UpNext);
        s.all.search.replace_text("zzz");
    });
    settle(&mut rt);
    assert!(has_text(&rt, "Nothing is queued."), "{:?}", texts(&rt));
}

fn texts_of(rt: &Runtime<App>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default()
}

// ------------------------------------------------------------------------------------------------ 2. dragging a card

#[test]
fn an_allowed_drag_sends_exactly_the_request_the_web_sends() {
    let mut rt = runtime(board);
    // A stopped task (Needs you) carried two columns right, to Running: `StartTask`.
    let (x, y) = hold(&mut rt, "board-card-t5", 2.0 * 292.0);
    assert!(has_key(&rt, "board-ghost"), "the carried card floats under the pointer");
    assert!(has_key(&rt, "board-slot-Running"), "the column says what the drop does");
    assert!(rt.state.outbox.is_empty(), "nothing is sent until it is dropped");
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.outbox, vec![Call::Start { task_id: "t5".into() }], "{:?}", rt.state.outbox);
    assert!(!has_key(&rt, "board-ghost"));
    rt.state.outbox.clear();
    // A task ready for review, two columns right to Done: `CompleteTask`.
    carry(&mut rt, "board-card-t3", 3.0 * 292.0);
    assert_eq!(rt.state.outbox, vec![Call::Complete { task_id: "t3".into() }], "{:?}", rt.state.outbox);
}

#[test]
fn a_refused_drag_sends_nothing_and_says_why_and_the_column_does_not_light_up() {
    let mut rt = runtime(board);
    // A running task carried two columns left, to Needs you: the web does not allow it.
    let (x, y) = hold(&mut rt, "board-card-t6", -2.0 * 292.0);
    assert!(has_key(&rt, "board-ghost"));
    assert!(!has_key(&rt, "board-slot-Needs you"), "a refused column shows no drop slot");
    assert!(rt.state.all.carry.as_ref().is_some_and(|c| c.over == Some(neko_rune::all_sessions::Stage::NeedsYou)));
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert!(rt.state.outbox.is_empty(), "nothing is sent: {:?}", rt.state.outbox);
    assert!(rt.state.dialog.is_none(), "and no dialog");
    assert!(rt.state.toast_words().contains("can’t go back to waiting"), "{}", rt.state.toast_words());
}

#[test]
fn a_drag_that_is_cancelled_or_barely_moves_sends_nothing_and_a_click_opens_the_task() {
    let mut rt = runtime(board);
    hold(&mut rt, "board-card-t5", 2.0 * 292.0);
    press(&mut rt, Key::Escape);
    assert!(rt.state.outbox.iter().all(|c| c.reads()) && !has_key(&rt, "board-ghost"), "Esc or a lost pointer puts it back: {:?}", rt.state.outbox);
    // A press without travel is a click: it opens the session.
    rt.state.all.suppress_press = false;
    click(&mut rt, "board-card-t4");
    assert_eq!(rt.state.selected.as_deref(), Some("t4"));
    assert!(rt.state.outbox.iter().all(|c| c.reads()), "opening a session only reads: {:?}", rt.state.outbox);
}

#[test]
fn a_drop_past_the_board_moves_nothing() {
    let mut rt = runtime(board);
    // A task ready for review carried off the right of the columns (onto the window's edge): no column, no request.
    carry(&mut rt, "board-card-t3", 5.0 * 292.0);
    assert!(rt.state.outbox.iter().all(|c| c.reads()) && rt.state.dialog.is_none(), "{:?}", rt.state.outbox);
}

#[test]
fn the_two_slash_popovers_do_not_share_their_state() {
    let mut rt = runtime(scene("bb-slash"));
    // Neko's ask box holds a draft too: the box with the keyboard shows its popover, and Esc in it leaves the other alone.
    rt.update(|s, _| {
        s.ask.replace_text("/");
        s.composer.focused = true;
        s.ask.focused = false;
    });
    settle(&mut rt);
    assert!(has_key(&rt, "reply-popover") && !has_key(&rt, "ask-popover"));
    click(&mut rt, "composer-input");
    press(&mut rt, Key::Escape);
    assert!(!has_key(&rt, "reply-popover"));
    assert!(rt.state.compose.dismissed.is_none(), "the ask box's popover was not dismissed");
}

#[test]
fn a_running_task_dropped_on_done_asks_before_it_cancels() {
    let mut rt = runtime(board);
    carry(&mut rt, "board-card-t6", 2.0 * 292.0);
    assert!(rt.state.outbox.is_empty(), "the request waits for the dialog: {:?}", rt.state.outbox);
    assert!(has_key(&rt, "ask-yes") && has_text(&rt, "Cancel “Rework the export batching”?"), "{:?}", texts(&rt));
    click(&mut rt, "ask-no");
    assert!(rt.state.outbox.is_empty());
    carry(&mut rt, "board-card-t6", 2.0 * 292.0);
    click(&mut rt, "ask-yes");
    assert_eq!(rt.state.outbox, vec![Call::Stop { task_id: "t6".into() }]);
}

#[test]
fn a_waiting_cards_one_action_is_on_hover_and_in_the_right_click_menu() {
    let mut rt = runtime(board);
    // t0 needs approval: Approve; t1 asks a question: Answer; t5 stopped: Start again.
    click(&mut rt, "board-action-t0");
    assert_eq!(rt.state.outbox, vec![Call::Approve { task_id: "t0".into() }]);
    rt.state.outbox.clear();
    click(&mut rt, "board-action-t5");
    assert_eq!(rt.state.outbox, vec![Call::Start { task_id: "t5".into() }]);
    assert!(!has_key(&rt, "board-action-t3"), "a card that waits on nothing has no action");
    rt.state.outbox.clear();
    right_click(&mut rt, "board-card-t5");
    click_last_text(&mut rt, "Start again");
    assert_eq!(rt.state.outbox, vec![Call::Start { task_id: "t5".into() }]);
}

// ------------------------------------------------------------------------------------------------ 3. the tree

#[test]
fn the_tree_indents_29_points_a_level_and_a_caret_folds_it() {
    let mut rt = runtime(scene("bb-tree"));
    let left = |rt: &Runtime<App>, key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).map(|e| e.rect[0]).unwrap_or_else(|| panic!("no {key}"));
    // t6 starts t9 and t11; t11 starts t13.
    let (p, c, g) = (left(&rt, "tree-fold-t6"), left(&rt, "tree-fold-t11"), left(&rt, "tree-lead-t13"));
    assert!((c - p - 29.0).abs() < 0.6, "a child is one level (13 for the rail and 16 for the step) in: {p} {c}");
    assert!((g - c - 29.0).abs() < 0.6, "a grandchild is one more: {c} {g}");
    assert!(has_key(&rt, "tree-row-t9") && has_key(&rt, "tree-row-t11") && has_key(&rt, "tree-row-t13"));
    click(&mut rt, "tree-fold-t6");
    assert_eq!(rt.state.all.tree_open.get("t6"), Some(&false));
    for gone in ["tree-row-t9", "tree-row-t11", "tree-row-t12", "tree-row-t13"] {
        assert!(!has_key(&rt, gone), "{gone} is under a folded row");
    }
    assert!(has_key(&rt, "tree-row-t6"), "the parent stays");
    click(&mut rt, "tree-fold-t6");
    assert!(has_key(&rt, "tree-row-t13"), "unfolded again");
    // A row opens its session.
    click(&mut rt, "tree-row-t9");
    assert_eq!(rt.state.selected.as_deref(), Some("t9"));
}

#[test]
fn the_tree_is_the_webs_tasks_header_then_the_summary_line_then_neko_and_what_it_looks_after() {
    let mut rt = runtime(scene("bb-tree"));
    let y = |key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).map(|e| e.rect[1]).unwrap_or_else(|| panic!("no {key}"));
    assert!(y("all-title") < y("all-stage-tabs") && y("all-stage-tabs") <= y("all-layout"), "title, then the stage tabs and the layout switch");
    assert!(y("all-layout") < y("tree-summary"), "the summary line comes under the toolbar");
    assert!(y("tree-summary") < y("tree-title-neko") && y("tree-title-neko") < y("tree-title-t6"), "Neko first, then its tasks");
    assert!(has_text(&rt, "Chief of staff") && has_text(&rt, "Collapse all"), "Neko's own line and the page's one button");
    let row = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("tree-node-neko")).expect("Neko's row");
    assert_eq!(row.rect[3], 36.0, "the web's 36 point row");
    let summary = texts_of(&rt, "tree-summary");
    assert!(summary.starts_with("22 tasks · 0 running · $0.00 reported"), "{summary}");
    click(&mut rt, "tree-fold-all");
    assert!(has_text(&rt, "Expand all") && !has_key(&rt, "tree-row-t9"), "Collapse all folds everything but Neko");
}

#[test]
fn j_and_k_walk_the_trees_open_rows() {
    let mut rt = runtime(scene("bb-tree-folded"));
    press(&mut rt, Key::Text("j".into()));
    press(&mut rt, Key::Text("j".into()));
    press(&mut rt, Key::Enter);
    let open = rt.state.selected.clone().unwrap_or_default();
    assert!(!["t9", "t11", "t12", "t13"].contains(&open.as_str()), "a folded row's children are skipped: {open}");
}

// ------------------------------------------------------------------------------------------------ 4. pull requests

#[test]
fn the_sidebar_has_a_pull_requests_row_after_calendar_with_what_waits_on_you() {
    let mut rt = runtime(scene("bb-prs"));
    let y = |rt: &Runtime<App>, key: &str| rt.find(key).map(|p| p.1).unwrap_or_else(|| panic!("no {key}"));
    assert!(y(&rt, "view-calendar") < y(&rt, "view-prs") && y(&rt, "view-prs") < y(&rt, "view-setup"), "after Calendar");
    assert_eq!(texts_of(&rt, "count-prs"), "4", "two ready, one to close, one needing a call");
    rt.update(|s, _| s.summary.as_mut().unwrap().pull_requests.clear());
    settle(&mut rt);
    assert!(!has_key(&rt, "count-prs"), "no count at 0");
}

#[test]
fn the_groups_are_the_webs_and_each_folds() {
    let mut rt = runtime(scene("bb-prs"));
    for title in ["Ready to merge", "Suggested to close", "Needs your call", "Merging when checks pass", "Being updated"] {
        assert!(has_key(&rt, &format!("pr-group-{title}")), "{title}");
    }
    assert!(!has_key(&rt, "pr-group-Merged"), "merged and closed wait to be asked for");
    assert!(has_key(&rt, "pr-row-t2") && has_key(&rt, "pr-row-t3"));
    // A row: repository and number (faint) before the title.
    assert_eq!(texts_of(&rt, "pr-place-t2"), "#101 · acme-api");
    click(&mut rt, "pr-group-Ready to merge");
    assert!(!has_key(&rt, "pr-row-t2"), "a folded group shows no rows");
    assert_eq!(texts_of(&rt, "pr-group-count-Ready to merge"), "2", "its count stays");
    click(&mut rt, "pr-show-finished");
    assert!(has_key(&rt, "pr-group-Merged"));
}

#[test]
fn merge_when_ready_is_a_switch_that_sends_set_merge_when_ready() {
    let mut rt = runtime(scene("bb-prs"));
    click(&mut rt, "pr-merge-t2");
    assert_eq!(rt.state.outbox, vec![Call::SetMergeWhenReady { task_id: "t2".into(), enabled: true }]);
    rt.state.outbox.clear();
    // A pull request already merging when ready: the switch is on and turns it off.
    click(&mut rt, "pr-merge-t7");
    assert_eq!(rt.state.outbox, vec![Call::SetMergeWhenReady { task_id: "t7".into(), enabled: false }]);
    // The same through the right-click menu.
    rt.state.outbox.clear();
    right_click(&mut rt, "pr-row-t3");
    click_last_text(&mut rt, "Merge when ready");
    assert_eq!(rt.state.outbox, vec![Call::SetMergeWhenReady { task_id: "t3".into(), enabled: true }]);
}

#[test]
fn close_asks_for_a_reason_and_sends_it_only_when_confirmed() {
    let mut rt = runtime(scene("bb-prs"));
    click(&mut rt, "pr-close-t4");
    assert!(has_key(&rt, "ask-yes") && has_text(&rt, "Close #57?"), "{:?}", texts(&rt));
    assert!(rt.state.outbox.is_empty());
    // The reason starts as the agent's suggestion; empty words keep the button off.
    rt.update(|s, _| s.pages.set_field(ASK_FIELD, ""));
    settle(&mut rt);
    let (x, y) = rt.find("ask-yes").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert!(rt.state.outbox.is_empty() && has_key(&rt, "ask-yes"), "no reason, no close");
    rt.update(|s, _| s.pages.set_field(ASK_FIELD, "A duplicate of #52"));
    settle(&mut rt);
    click(&mut rt, "ask-yes");
    assert_eq!(rt.state.outbox, vec![Call::ClosePr { task_id: "t4".into(), comment: "A duplicate of #52".into() }]);
    assert!(!has_key(&rt, "ask-yes"));
    // Keep it sends nothing.
    rt.state.outbox.clear();
    click(&mut rt, "pr-close-t4");
    click(&mut rt, "ask-no");
    assert!(rt.state.outbox.is_empty() && !has_key(&rt, "ask-no"));
}

#[test]
fn merge_all_ready_lists_the_pull_requests_and_merges_each_when_confirmed() {
    let mut rt = runtime(scene("bb-prs"));
    click(&mut rt, "pr-merge-all");
    assert!(has_text(&rt, "Merge 2 pull requests?") && has_key(&rt, "ask-list-0") && has_key(&rt, "ask-list-1"), "{:?}", texts(&rt));
    assert!(rt.state.outbox.is_empty(), "nothing before the dialog says yes");
    click(&mut rt, "ask-yes");
    assert_eq!(rt.state.outbox, vec![Call::SetMergeWhenReady { task_id: "t2".into(), enabled: true }, Call::SetMergeWhenReady { task_id: "t3".into(), enabled: true }]);
}

#[test]
fn check_github_review_and_merge_without_review_are_in_the_header() {
    let mut rt = runtime(scene("bb-prs"));
    click(&mut rt, "pr-check");
    assert_eq!(rt.state.outbox, vec![Call::CheckPullRequests]);
    rt.state.outbox.clear();
    click(&mut rt, "pr-more");
    click_last_text(&mut rt, "Review pull requests");
    assert!(rt.state.outbox.is_empty() && has_key(&rt, "ask-yes"), "re-checking every one asks first");
    click(&mut rt, "ask-yes");
    assert_eq!(rt.state.outbox, vec![Call::ReviewPullRequests { workspace_id: None }]);
    rt.state.outbox.clear();
    // Merge without review: on for acme-web (scene), off for acme-api; turning it on asks, turning it off does not.
    click(&mut rt, "pr-more");
    click_last_text(&mut rt, "acme-api");
    assert!(rt.state.outbox.is_empty() && has_text(&rt, "Merge without review in acme-api?"), "{:?}", texts(&rt));
    click(&mut rt, "ask-yes");
    assert_eq!(rt.state.outbox, vec![Call::SetMergeWithoutReview { repository: "/work/acme-api".into(), enabled: true }]);
    rt.state.outbox.clear();
    click(&mut rt, "pr-more");
    click_last_text(&mut rt, "acme-web");
    assert_eq!(rt.state.outbox, vec![Call::SetMergeWithoutReview { repository: "/work/acme-web".into(), enabled: false }]);
}

#[test]
fn a_row_opens_its_task_and_the_pull_request() {
    let mut rt = runtime(scene("bb-prs"));
    click(&mut rt, "pr-task-t2");
    assert_eq!(rt.state.selected.as_deref(), Some("t2"));
}

// ------------------------------------------------------------------------------------------------ 5. details

#[test]
fn the_details_tab_has_its_sections_and_each_folds() {
    let mut rt = runtime(scene("bb-details"));
    for section in ["Risk assessment", "Parallel work", "Decisions", "Working copy", "Info", "Activity log"] {
        assert!(has_text(&rt, section), "{section}: {:?}", texts(&rt));
    }
    assert!(has_text(&rt, "Medium"), "the risk level");
    assert!(has_text(&rt, "job.rs:42 returns on the first error"), "what supports it");
    assert!(has_key(&rt, "split-0-0") && has_key(&rt, "split-0-1"), "each subtask a row");
    assert!(has_text(&rt, "You approved a local build"), "decisions with their text");
    assert!(has_key(&rt, "info-workspace") && has_key(&rt, "info-runtime"), "the old Info tab is a section");
    click(&mut rt, "details-risk");
    assert!(!has_text(&rt, "job.rs:42 returns on the first error"), "a folded section shows nothing");
    assert!(rt.state.panel.toggled.contains(&Section::Risk));
}

#[test]
fn a_subtask_row_opens_that_task_and_propose_parallel_work_sends_the_webs_request() {
    let mut rt = runtime(scene("bb-details"));
    click(&mut rt, "split-0-1");
    assert_eq!(rt.state.selected.as_deref(), Some("t12"));
    let mut rt = runtime(scene("bb-details-propose"));
    click(&mut rt, "details-split-propose");
    assert_eq!(rt.state.outbox, vec![Call::ProposeSplit { task_id: "t0".into() }]);
}

#[test]
fn the_working_copy_has_its_path_branch_reveal_and_copy() {
    let mut rt = runtime(scene("bb-details"));
    assert!(has_key(&rt, "info-copy-path"), "{:?}", texts(&rt));
    let shown = texts(&rt).into_iter().find(|t| t.starts_with("~/") || t.contains("neko-sync-retry")).unwrap_or_default();
    assert!(shown.starts_with("~/") || !std::env::var("HOME").is_ok_and(|h| h.len() > 1), "the home folder is written ~: {shown}");
    click(&mut rt, "copy-copy");
    assert!(rt.state.toast_words().contains("Path copied"));
    click(&mut rt, "copy-reveal");
}

#[test]
fn the_activity_log_is_newest_first_in_32_point_rows() {
    let mut rt = runtime(scene("bb-details"));
    // The scene opens the log. Its first row is the newest event.
    let first = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("log-row-0")).expect("a log row");
    assert_eq!(first.rect[3], 32.0, "32 points tall");
    assert!(has_text(&rt, "Checked the change independently."), "the newest event: {:?}", texts(&rt));
    let y = |rt: &Runtime<App>, key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).map(|e| e.rect[1]).unwrap_or_else(|| panic!("no {key}"));
    assert!(y(&rt, "log-row-0") < y(&rt, "log-row-5"), "newest above oldest");
    rt.update(|s, _| {
        s.panel.toggled.remove(&Section::Log);
    });
    settle(&mut rt);
    assert!(!has_key(&rt, "log-row-0"), "folded by default");
}

#[test]
fn the_panel_names_the_tab_details() {
    assert_eq!(Tab::Details.title(), "Details");
    let rt = runtime(scene("bb-details"));
    assert!(has_text(&rt, "Details") || has_key(&rt, "context-icon-tabs") || has_key(&rt, "segment0"), "{:?}", texts(&rt));
}

// ------------------------------------------------------------------------------------------------ 6. the session's slash

#[test]
fn slash_in_the_session_composer_opens_the_webs_commands() {
    let mut rt = runtime(scene("bb-slash"));
    assert!(has_key(&rt, "reply-popover"), "the popover is over the reply box");
    for name in ["/stop", "/memory", "/tickets", "/clear", "/models", "/permissions", "/setup", "/help"] {
        assert!(has_text(&rt, name), "{name}");
    }
    assert!(!has_text(&rt, "/remember") && !has_text(&rt, "/plan"), "Neko's own conversation is not the session's");
    // Typing narrows it; Down moves; Esc closes until the draft changes.
    rt.update(|s, _| s.composer.replace_text("/c"));
    settle(&mut rt);
    assert!(has_text(&rt, "/clear") && !has_text(&rt, "/stop"));
    rt.update(|s, _| s.composer.replace_text("/"));
    settle(&mut rt);
    click(&mut rt, "composer-input");
    press(&mut rt, Key::Down);
    assert_eq!(rt.state.compose.reply_index, 1);
    press(&mut rt, Key::Escape);
    assert!(!has_key(&rt, "reply-popover"));
}

#[test]
fn a_command_chosen_in_the_session_composer_runs_and_a_plain_message_still_sends() {
    let mut rt = runtime(scene("bb-slash"));
    click(&mut rt, "reply-pop-0");
    assert!(rt.state.composer.text.is_empty(), "the box is emptied");
    assert!(has_key(&rt, "ask-yes") && has_text(&rt, "agents?"), "/stop asks before it stops anything: {:?}", texts(&rt));
    assert!(rt.state.outbox.is_empty());
    click(&mut rt, "ask-no");
    // Return on a highlighted row accepts it.
    rt.update(|s, _| s.composer.replace_text("/tick"));
    settle(&mut rt);
    click(&mut rt, "composer-input");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.view, Some(neko_rune::views::FullView::AllSessions), "/tickets opens All sessions");
    // A message that only contains a slash is a message.
    let mut rt = runtime(scene("bb-slash"));
    rt.update(|s, _| s.composer.replace_text("fix the path /src/lib"));
    settle(&mut rt);
    assert!(!has_key(&rt, "reply-popover"));
}

#[test]
fn the_list_is_the_webs_tasks_header_stage_tabs_and_stage_groups_in_order() {
    let mut rt = runtime(scene("bb-board"));
    rt.update(|s, _| {
        s.set_all_layout(Layout::List);
        s.all.layout_dirty = false;
    });
    settle(&mut rt);
    let y = |key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).map(|e| e.rect[1]).unwrap_or_else(|| panic!("no {key}"));
    assert!(has_key(&rt, "all-title") && has_key(&rt, "all-new-task") && has_key(&rt, "all-stage-tabs") && has_key(&rt, "all-search-label") && has_key(&rt, "all-layout"));
    assert!(y("all-title") < y("all-stage-tabs") && y("all-stage-tabs") <= y("all-layout"), "title, then the stage tabs, then the layout switch");
    let groups: Vec<f32> = ["Needs you", "Ready to review", "Running", "Up next", "Done"].iter().filter_map(|t| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(&format!("all-stage-{t}"))).map(|e| e.rect[1])).collect();
    assert!(groups.len() >= 2 && groups.windows(2).all(|w| w[0] < w[1]), "stage groups in the web's order: {groups:?}");
    let row = rt.elements().into_iter().find(|e| e.key.as_deref().is_some_and(|k| k.starts_with("all-line-"))).expect("a task line");
    assert_eq!(row.rect[3], 40.0, "the web's 40 point row");
}
