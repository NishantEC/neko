//! The session view: one line on top (title, then runtime and goal), the goal line, the agent's
//! turns streaming below, the runs with their diffs and undo, and at the bottom the note or
//! question for the person and the composer with its steer-mode menu. A full view (Calendar,
//! Settings, ...) takes this whole region when chosen; "+ New" takes it while a session is made.

use neko_protocol::crew::DeliveryMode;
use neko_protocol::workbench::{Task, TaskStatus};
use rune::{Color, El, Key, KeyEvent, ScrollSpec, col, el, icon, row, text};
use rune_kit::{Menu, MenuItem, Placement};
use crate::link::Call;

use super::turn::{self, Where};
use super::{Frame, TEXT_COLUMN, TEXT_PAD, band, icon_button, placeholder, primary_button, work};
use crate::app::{App, Steer};
use crate::icons;
use crate::lab::lab;
use crate::link::TaskRow;
use crate::status::{status_glyph, status_label, status_of};
use crate::theme::Neko;
use crate::turns::task_turns;
use crate::views::FullView;
use crate::work::{SHOWN_TURNS, working_line};

const LIST: Where = Where { ns: "", compact: false };

/// The fade under the header and the goal lines.
pub const FADE: f32 = 8.0;

/// The most an open goal's body takes: 380, or 35% of the window when that is less, so the
/// conversation and the composer under it are never covered.
pub fn goal_body_cap(win_h: f32) -> f32 {
    380.0f32.min(win_h * 0.35)
}

/// One centred text column: at most 760 wide with 32 at each side, however wide the region is.
fn text_column(inner: El) -> El {
    row().w_full().justify_center().child(col().w_full().max_w(TEXT_COLUMN).px(TEXT_PAD).child(inner))
}

/// The card in the middle: inset from the window's edges with the window's radius.
pub fn region(s: &App, t: &Neko, layer: &mut Vec<El>, frame: Frame) -> El {
    let inner = if s.lab {
        col().w_full().grow(1.0).min_h(0.0).child(band(t)).child(col().w_full().grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(row().w_full().justify_center().child(lab(t))))
    } else if s.home_shown() && s.onboarding_shown() {
        super::onboarding::view(s, t)
    } else if s.home_shown() {
        super::home::home(s, t, layer)
    } else if let Some(page) = s.plugin_view.as_ref() {
        if crate::luau::is_luau_page(page) { super::luau::page(s, t, page) } else { plugin_page(s, page, t) }
    } else if let Some(view) = s.view {
        full_view(s, view, t, layer, frame)
    } else {
        match s.session() {
            Some(task) => session(s, task, t, layer, frame),
            None => empty(s, t),
        }
    };
    // One calm line when a button could not do its job (opening the web UI while Neko is not running).
    let mut region = col()
        .key("session-region")
        .grow(1.0)
        .min_w(0.0)
        .my(8.0)
        .mx(2.0)
        .radius(t.radius.window)
        .fill(t.surface.window)
        // Files dropped anywhere on the region are attached to the open session's reply box (or Home's new-session box).
        .on_drop_files(|s: &mut App, files: &[std::path::PathBuf], _| s.drop_files(files))
        .on_drop_hover(|s: &mut App, over: bool, _| s.compose.drop_over = over)
        .child(col().w_full().grow(1.0).min_h(0.0).child(inner));
    if let Some(overlay) = super::compose::drop_overlay(s, t) {
        region = region.child(overlay);
    }
    if let Some(notice) = &s.notice {
        region = region.child(text(notice.clone()).key("notice").font(t.ty.meta).color(t.text.muted).px(20.0).py(6.0).live_region(rune::LiveRegion::Polite));
    }
    region
}

/// The colour of the state badge beside the title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Waiting on the person, or being worked on.
    Amber,
    /// A failure.
    Coral,
    /// A result to review.
    Sky,
}

/// The state badge the web shows beside the title, where it shows one: its words and its tone (amber for what is
/// waiting on the person or being worked on, coral for a failure, sky for a result ready to review). Queued,
/// Completed and Cancelled have none.
pub fn state_badge(task: &Task) -> Option<(&'static str, Tone)> {
    match task.status {
        TaskStatus::AwaitingApproval if crate::work::waiting_reason(task).is_some() => Some(("Needs answer", Tone::Amber)),
        TaskStatus::AwaitingApproval => Some(("Needs approval", Tone::Amber)),
        TaskStatus::Planning => Some(("Planning", Tone::Amber)),
        TaskStatus::Building => Some(("Building", Tone::Amber)),
        TaskStatus::Reviewing => Some(("Reviewing", Tone::Amber)),
        TaskStatus::ReadyForReview => Some(("Ready to review", Tone::Sky)),
        TaskStatus::Failed => Some(("Failed", Tone::Coral)),
        _ => None,
    }
}

/// 12.5 medium, the tone's text on that colour at 12%, radius 6, padding 2 and 8.
fn badge(t: &Neko, words: &str, tone: Tone) -> El {
    let ink = match tone {
        Tone::Amber => t.meaning.amber,
        Tone::Coral => t.meaning.coral,
        Tone::Sky => t.meaning.sky,
    };
    row().key("state-badge").px(8.0).py(2.0).radius(6.0).fill(ink.alpha(0.12)).child(text(words.to_owned()).font(rune::TextStyle { weight: 500.0, ..t.ty.meta }).color(ink).a11y_hidden())
}

/// The top line, one 32 point row and nothing else: status glyph, title (14 semibold, ellipsis; a field while it is being
/// renamed), the state badge where the web shows one, then at the right the runtime chip, the "..." menu and the context
/// panel's toggle. Its fill is the region's own, opaque, so nothing under it ever shows through.
fn top_line(s: &App, task: &TaskRow, t: &Neko, layer: &mut Vec<El>) -> El {
    let detail = s.detail().filter(|d| d.task.id == task.id);
    let mut line = band(t).px(20.0).fill(t.surface.window).child(status_glyph(status_of(task.status), t, 14.0, Some(status_label(task.status))));
    let renaming = s.rename.as_ref().filter(|r| r.id == task.id);
    if let Some(r) = renaming {
        // Enter saves, Esc cancels; leaving the field saves nothing.
        let field = rune_kit::TextInput::new(&r.edit)
            .label("Session name")
            .placeholder("Name this session")
            .on_edit(|s: &mut App| &mut s.rename.as_mut().expect("renaming").edit)
            .on_submit(|s: &mut App, _| s.finish_rename())
            .on_key(|s: &mut App, ev: &KeyEvent, _| {
                if ev.key == Key::Escape {
                    s.cancel_rename();
                    return true;
                }
                false
            })
            .build(&t.kit)
            .key("rename-input")
            .grow(1.0)
            .min_w(0.0);
        line = line.child(field);
    } else {
        line = line.child(text(task.title.clone()).key("top-title").font(t.ty.brand).color(t.text.base).truncate().min_w(0.0).a11y_hidden().role("heading"));
    }
    if let Some((words, tone)) = detail.and_then(|d| state_badge(&d.task)) {
        line = line.child(badge(t, words, tone));
    }
    if renaming.is_none() {
        line = line.child(el().grow(1.0).min_w(0.0));
    }
    let runtime = super::runtime::chip_words(s);
    let runtime = Some(runtime).filter(|r| !r.is_empty()).or_else(|| s.summary.as_ref().map(|m| m.runtime.clone()));
    if let Some(r) = runtime {
        line = line.child(text(r).key("top-runtime").font(t.ty.meta).color(t.text.muted).truncate().min_w(0.0).on_press(|s: &mut App| s.runtime_toggle()));
    }
    line.child(overflow_menu(s, task, detail.map(|d| d.can_split).unwrap_or(false), t, layer))
        .child(icon_button(&icons::PANEL_RIGHT, if s.layout.context.is_open() { "Hide context" } else { "Show context" }, "context-toggle", t, |s: &mut App| s.panel_toggle()))
}

/// The "..." menu: Propose parallel work, then what the web's task menu offers (`task_actions.rs`).
fn overflow_menu(s: &App, task: &TaskRow, can_split: bool, t: &Neko, layer: &mut Vec<El>) -> El {
    let trigger = icon_button(&icons::MORE, "More actions", "session-more", t, |s: &mut App| s.session_menu = !s.session_menu);
    let split = task.id.clone();
    let mut menu = Menu::new("session-menu").trigger(trigger).open(s.session_menu).placement(Placement::BottomEnd).min_width(240.0).on_close(|s: &mut App| s.session_menu = false);
    if task.status == TaskStatus::AwaitingApproval {
        menu = menu.item(MenuItem::new("Propose parallel work").disabled(!can_split).on_select(move |s: &mut App| {
            s.session_menu = false;
            s.call(Call::ProposeSplit { task_id: split.clone() });
        }));
    }
    crate::task_actions::fill_menu(menu, s, &task.id).build(&t.kit, layer)
}

fn session(s: &App, row_task: &TaskRow, t: &Neko, layer: &mut Vec<El>, frame: Frame) -> El {
    let mut c = col().w_full().h_full().min_w(0.0).child(top_line(s, row_task, t, layer));
    let Some(detail) = s.detail() else {
        // The row is here and its events are on the way (one read after choosing it).
        return c
            .child(col().key("turns").w_full().grow(1.0).min_h(0.0).child(placeholder(&["Opening the session…"], t)))
            .child(composer(s, t, layer, false));
    };
    let task = &detail.task;
    let goals = s.summary.as_ref().map(|m| m.goals_of(&task.id)).unwrap_or_default();
    // The goal line is sticky: it floats over the top of the conversation, and the conversation's
    // scroll area is inset by its height plus 8, so its first line is never under it.
    // The lines' own padding (4 above, 4 below) and 8 of air.
    let inset = if goals.is_empty() { 0.0 } else { goals.len() as f32 * work::GOAL_LINE_H + 16.0 };
    let content = col().w_full().max_w(TEXT_COLUMN).px(TEXT_PAD).pb(20.0).pt(inset).child(conversation(s, t, task, detail.has_folder, layer));
    let mut region = col().key("turns-region").w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).child(
        col()
            .key("turns")
            .w_full()
            .h_full()
            .min_h(0.0)
            .clip_rounded(0.0)
            .scroll(ScrollSpec { stick_bottom: true })
            .scroll_margin(inset)
            .child(row().w_full().justify_center().child(content)),
    );
    // What stays over the top of the conversation: the goal lines when there are any, opaque, and under them an 8 point
    // fade into the window's colour, so a line of the conversation never shows through or ends against an edge.
    let mut sticky = col().key("sticky").w_full();
    if !goals.is_empty() {
        let mut lines = col().key("goals").w_full().px(8.0).pt(4.0).pb(4.0).fill(t.surface.window);
        for goal in &goals {
            lines = lines.child(work::goal_line(s, t, goal));
            // What a goal adds (its question, a refusal, its evidence when opened) hangs under its line
            // and scrolls on its own, over the conversation, until it is closed.
            if let Some(body) = work::goal_body(s, t, goal) {
                lines = lines.child(col().key(format!("goal-body-{}", goal.id)).w_full().max_h(goal_body_cap(frame.win_h)).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body));
            }
        }
        sticky = sticky.child(lines);
    }
    sticky = sticky.child(el().key("header-fade").w_full().h(FADE).gradient(t.surface.window, t.surface.window.alpha(0.0)).a11y_hidden());
    region = region.child(sticky.absolute(0.0, 0.0));
    c = c.child(region);
    if let Some(tray) = work::tray(s, t, task) {
        c = c.child(text_column(tray.w_full().pb(8.0)));
    }
    c.child(composer(s, t, layer, true))
}

/// The turns, then the plan and the result, the runs and the working line.
fn conversation(s: &App, t: &Neko, task: &Task, has_folder: bool, layer: &mut Vec<El>) -> El {
    let all = task_turns(task);
    let hidden = if s.work.all_turns { 0 } else { all.len().saturating_sub(SHOWN_TURNS) };
    let mut c = col().key("conversation").w_full().pt(4.0);
    if hidden > 0 {
        c = c.child(super::quiet_button(&format!("Show {hidden} earlier"), "show-earlier", t, |s: &mut App| s.work.all_turns = true));
    }
    let owner = turn::HandoffTask { id: task.id.clone(), title: task.display_title.clone().unwrap_or_else(|| task.title.clone()), status: task.status };
    for el in turn::turns_for(s, t, LIST, &all[hidden..], Some(&owner)) {
        c = c.child(el);
    }
    let plan = task.plan.trim();
    if !plan.is_empty() && matches!(task.status, TaskStatus::Planning | TaskStatus::AwaitingApproval) {
        c = c.child(block(t, "plan", if task.status == TaskStatus::Planning { "Saved plan · being revised" } else { "Plan" }, plan));
    }
    let result = task.result.trim();
    if !result.is_empty() {
        let (body, review) = match result.split_once("\n\nIndependent review:\n") {
            Some((body, review)) => (body, Some(review)),
            None => (result, None),
        };
        let mut r = block(t, "result", &format!("Result · {}", status_label(task.status)), body);
        if let Some(review) = review {
            let open = s.open("review");
            r = r.child(
                super::quiet_button(if open { "Hide the independent review" } else { "Show the independent review" }, "review-toggle", t, |s: &mut App| s.toggle("review")),
            );
            if open {
                r = r.child(text(review.chars().take(2000).collect::<String>()).font(t.ty.meta).color(t.text.muted).wrap().selectable(true).mt(4.0));
            }
        }
        c = c.child(r);
    }
    if let Some(pr) = task.pull_request.as_ref().filter(|p| !p.url.is_empty()) {
        let url = pr.url.clone();
        let number = url.rsplit('/').next().unwrap_or("").to_owned();
        c = c.child(
            row().w_full().gap(10.0).center_y().mt(18.0).child(icon(&icons::PULL_REQUEST, 16.0).color(t.meaning.iris).a11y_hidden()).child(text(format!("Pull request #{number}")).font(t.ty.row).color(t.text.base)).child(el().grow(1.0)).child(super::quiet_button("Open on GitHub", "open-pr-link", t, move |s: &mut App| s.open_link(&url))),
        );
    }
    if has_folder
        && let Some(list) = s.work.checkpoints.get(&task.id)
        && let Some(runs) = work::runs(s, t, &task.id, list, layer)
    {
        c = c.child(col().w_full().mt(22.0).child(runs));
    }
    if let Some(line) = working_line(task.status) {
        c = c.child(text(line).key("working").font(t.ty.meta).color(t.text.faint).mt(18.0).live_region(rune::LiveRegion::Polite));
    }
    if all.is_empty() && plan.is_empty() && result.is_empty() {
        c = c.child(text("Nothing has happened on this task yet.").font(t.ty.meta).color(t.text.muted).mt(40.0));
    }
    c
}

fn block(t: &Neko, key: &str, title: &str, body: &str) -> El {
    col()
        .key(format!("block-{key}"))
        .w_full()
        .gap(6.0)
        .mt(24.0)
        .child(text(title.to_owned()).font(t.ty.group).color(t.text.muted))
        .child(rune_kit::Markdown::new(body.to_owned()).selectable(true).build(&t.kit))
}

/// Nothing selected: before the first snapshot, or a workbench with no tasks. The real view of
/// an empty workbench says what to do first, in one line and one button.
fn empty(s: &App, t: &Neko) -> El {
    let (line, button) = if s.summary.is_some() { ("No sessions yet.", true) } else { ("Waiting for Neko.", false) };
    let mut c = col().key("empty").w_full().grow(1.0).min_h(0.0).items_center().justify_center().gap(12.0).child(text(line).font(t.ty.row).color(t.text.muted));
    if button {
        c = c.child(primary_button("New session", "empty-new", t, |s: &mut App| s.open_new()));
    }
    col().w_full().h_full().child(band(t)).child(c)
}

/// A plugin's page (slot b): its whole content is the web view, at `<route>?embed=1`. The band on top says
/// whose page it is; the daemon signs the address like any other.
fn plugin_page(s: &App, page: &neko_protocol::PluginPage, t: &Neko) -> El {
    // The page's route as the web app spells it: the plugin's own id for the page (`my-plugin:board`).
    s.web_view.set_want(Some(crate::web_panel::Want { route: format!("{}?embed=1", page.id), dark: t.dark }));
    let head = band(t).px(20.0).child(text(page.title.clone()).key("plugin-page-title").font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(t.text.base).truncate().a11y_hidden().role("heading")).child(text(format!("Plugin: {}", page.plugin)).font(t.ty.meta).color(t.text.faint).truncate().min_w(0.0));
    let body = crate::viewers::web::region(col().key("plugin-page-body").w_full().grow(1.0).min_h(0.0).child(crate::viewers::web::slot(t, &crate::viewers::web::line(s))));
    col().key("plugin-page").w_full().h_full().child(head).child(body)
}

/// A full view: its own page (Calendar, Routines, Settings, Usage, Workspaces, Plugins).
fn full_view(s: &App, view: FullView, t: &Neko, layer: &mut Vec<El>, frame: Frame) -> El {
    match view {
        FullView::Calendar => super::calendar::view(s, t, layer),
        FullView::Routines => super::routines::view(s, t, layer),
        FullView::Settings => super::settings::view(s, t, layer, frame),
        FullView::Usage => super::more::usage(s, t),
        FullView::Setup => super::setup::view(s, t, layer, frame),
        FullView::Plugins => super::plugins::view(s, t, layer),
        FullView::PullRequests => super::prs::page(s, t, layer),
        FullView::AllSessions => super::all::page(s, t, layer),
    }
}

fn mode_of(steer: Steer) -> DeliveryMode {
    crate::work::delivery(steer)
}

/// The composer: a multi-line field (IME and image paste), the steer-mode menu and Send.
fn composer(s: &App, t: &Neko, layer: &mut Vec<El>, ready: bool) -> El {
    let k = &t.kit;
    let field = super::composer::field(t, &s.composer, "composer-input", "input", "Message the agent", placeholder_for(s), 8, |s: &mut App| &mut s.composer, |s: &mut App, ev: &KeyEvent| {
        // The `/` popover's keys first (Up, Down, Return and Tab accept, Esc), then: Enter sends, Shift+Enter is a new line, ⌘↩
        // steers now; never while an input method is composing (the Enter that confirms a word is not a send).
        if s.reply_key(ev) {
            return true;
        }
        if ev.key != Key::Enter || ev.mods.shift || ev.mods.alt || s.composer.is_composing() {
            return false;
        }
        s.send_composer(ev.mods.cmd.then_some(DeliveryMode::SteerNow));
        true
    });

    let trigger = row()
        .key("steer-trigger")
        .role("button")
        .label(format!("Steer mode: {}", s.steer.label()))
        .focusable(true)
        .h(28.0)
        .px(8.0)
        .gap(4.0)
        .center_y()
        .radius(t.radius.control)
        .fill(if s.steer_open { t.surface.active } else { Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, k.c.border_focus)
        .on_press(|s: &mut App| s.steer_open = !s.steer_open)
        .child(text(s.steer.label()).font(t.ty.meta).color(t.text.muted).a11y_hidden())
        .child(icon(k.glyph.chevron_down, 14.0).color(t.text.muted).a11y_hidden());
    let mut menu = Menu::new("steer-menu").trigger(trigger).open(s.steer_open).placement(Placement::TopStart).min_width(260.0).on_close(|s: &mut App| s.steer_open = false);
    for mode in Steer::ALL {
        menu = menu.item(MenuItem::new(mode.label()).description(mode.detail()).checked(s.steer == mode).on_select(move |s: &mut App| s.steer = mode));
    }
    let steer = menu.build(k, layer);

    let can_send = ready && !s.composer.text.trim().is_empty();
    let send = super::composer::send(t, "send", &format!("Send, {}", mode_label(s)), &format!("Send, {}", mode_label(s)), can_send, false, |s: &mut App| s.send_composer(None));
    let model = super::runtime::chip(s, t);
    let attach = super::composer::attach(t, "composer-attach", crate::compose::Dest::Reply);
    text_column(super::composer::card(t, "composer-card", super::composer::with_chips(s, t, crate::compose::Dest::Reply, field), vec![attach], vec![steer, model], send).mb(16.0)).key("composer")
}

/// What Send does right now, in words (for its tooltip and name).
fn mode_label(s: &App) -> String {
    match s.detail().map(|d| d.task.status) {
        Some(st) if crate::work::in_turn(st) => format!("{} while the agent works", mode_of(s.steer).word()),
        _ => "as a reply".to_owned(),
    }
}

fn placeholder_for(s: &App) -> &'static str {
    match s.detail().map(|d| d.task.status) {
        Some(TaskStatus::AwaitingApproval) => "Answer the agent, or tell it what to change…",
        Some(TaskStatus::ReadyForReview) => "Ask a question or request changes…",
        Some(TaskStatus::Failed | TaskStatus::Cancelled) => "Tell the agent how to try again…",
        Some(TaskStatus::Completed) => "Reopen with a follow-up…",
        _ => "Redirect the agent. It pauses and replans…",
    }
}
