//! The main window's home, as designed in Paper (10 Today, 11 Ticket open,
//! 13 states): Neko's conversation with a morning brief, tickets as cards, a
//! rail for what's running, and a panel for one ticket.
//!
//! Tickets are the workbench's tasks. Their status places them in exactly one
//! group, and that grouping is the whole vocabulary of this screen.
use super::*;
use crate::assets::icon;
use gpui::{AnyElement, FontWeight, MouseButton, Rgba};
use neko_protocol::mcp_host::{McpCommand, Responsibility};
use neko_protocol::workbench::{ChatMessage, ChatRole, MemoryEntry, MemoryKind, Task};

/// The logo's pink: the only colour that means "this needs you".
const ATTENTION: u32 = 0xE88BA8;
const NOTE_ROLE: &str = "note";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Group {
    NeedsYou,
    Working,
    Done,
}

pub(super) fn group(status: TaskStatus) -> Group {
    match status {
        TaskStatus::AwaitingApproval | TaskStatus::ReadyForReview | TaskStatus::Failed => {
            Group::NeedsYou
        }
        TaskStatus::Queued
        | TaskStatus::Planning
        | TaskStatus::Building
        | TaskStatus::Reviewing => Group::Working,
        TaskStatus::Completed | TaskStatus::Cancelled => Group::Done,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TicketFilter {
    NeedsYou,
    Working,
    Done,
    All,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Brief {
    pub needs_you: usize,
    pub working: usize,
    pub done_today: usize,
}

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

pub(super) fn brief(tasks: &[&Task], now_ms: i64) -> Brief {
    let mut b = Brief::default();
    for task in tasks {
        match group(task.status) {
            Group::NeedsYou => b.needs_you += 1,
            Group::Working => b.working += 1,
            Group::Done
                if task.status == TaskStatus::Completed && now_ms - task.updated_at_ms < DAY_MS =>
            {
                b.done_today += 1
            }
            Group::Done => {}
        }
    }
    b
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Neko's first message each day, in its own voice.
pub(super) fn brief_text(b: &Brief) -> String {
    if b.needs_you == 0 && b.working == 0 && b.done_today == 0 {
        return "All quiet. Nothing needs you and nothing is running. Tell me what to look after, or ask me anything.".into();
    }
    let mut parts = Vec::new();
    if b.needs_you > 0 {
        parts.push(format!(
            "{} {} you",
            plural(b.needs_you, "ticket", "tickets"),
            if b.needs_you == 1 { "needs" } else { "need" }
        ));
    }
    if b.working > 0 {
        parts.push(format!(
            "{} {} in progress",
            b.working,
            if b.working == 1 { "is" } else { "are" }
        ));
    }
    if b.done_today > 0 {
        parts.push(format!("I finished {} today", b.done_today));
    }
    let mut text = match parts.len() {
        1 => parts[0].clone(),
        2 => format!("{} and {}", parts[0], parts[1]),
        _ => format!("{}, {} and {}", parts[0], parts[1], parts[2]),
    };
    if let Some(first) = text.get(0..1) {
        text = first.to_uppercase() + &text[1..];
    }
    text.push('.');
    if b.needs_you > 0 {
        text.push_str(" Here's what's waiting:");
    }
    text
}

pub(super) fn greeting(hour: u32) -> &'static str {
    match hour {
        5..=11 => "Good morning",
        12..=16 => "Good afternoon",
        _ => "Good evening",
    }
}

/// What a ticket is waiting on, in a few words.
pub(super) fn ticket_line(task: &Task) -> String {
    match task.status {
        TaskStatus::AwaitingApproval => "Plan ready. Approve to build it.".into(),
        TaskStatus::ReadyForReview => "Result ready for your review.".into(),
        TaskStatus::Failed => format!(
            "Stopped: {}",
            last_event(task).unwrap_or("see the ticket for details")
        ),
        TaskStatus::Queued => "Waiting to start".into(),
        TaskStatus::Planning => "Planning".into(),
        TaskStatus::Building => "Building in its own worktree".into(),
        TaskStatus::Reviewing => "Reviewing the result".into(),
        TaskStatus::Completed => "Done".into(),
        TaskStatus::Cancelled => "Cancelled".into(),
    }
}

fn last_event(task: &Task) -> Option<&str> {
    task.events
        .iter()
        .rev()
        .find(|e| e.role != NOTE_ROLE)
        .map(|e| e.message.as_str())
}

fn one_line(text: &str, limit: usize) -> String {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if line.chars().count() <= limit {
        return line.to_owned();
    }
    let cut: String = line.chars().take(limit).collect();
    format!("{}…", cut.trim_end())
}

pub(super) fn until(at_ms: i64, now_ms: i64) -> String {
    let minutes = (at_ms - now_ms).max(0) / 60_000;
    match minutes {
        0 => "any moment".into(),
        1..=59 => format!("in {minutes} min"),
        _ => format!("in {} h", minutes / 60),
    }
}

#[cfg(target_os = "macos")]
pub(super) fn local_hour() -> u32 {
    let now = (now_ms() / 1000) as libc::time_t;
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: both pointers are valid and aligned for the platform's ABI;
    // localtime_r initializes tm on success and does not retain either pointer.
    let ok = unsafe { !libc::localtime_r(&now, tm.as_mut_ptr()).is_null() };
    if ok {
        // SAFETY: the successful localtime_r call initialized every field.
        unsafe { tm.assume_init() }.tm_hour.clamp(0, 23) as u32
    } else {
        9
    }
}

#[cfg(not(target_os = "macos"))]
pub(super) fn local_hour() -> u32 {
    9
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn alpha(mut color: Rgba, a: f32) -> Rgba {
    color.a = a;
    color
}

fn attention() -> Rgba {
    gpui::rgb(ATTENTION)
}

fn dot(color: Rgba) -> impl IntoElement {
    div().size(px(7.)).rounded(px(4.)).flex_shrink_0().bg(color)
}

fn group_color(g: Group) -> Rgba {
    match g {
        Group::NeedsYou => attention(),
        Group::Working => theme::active().text_primary,
        Group::Done => alpha(theme::active().text_tertiary, 0.6),
    }
}

fn label(text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::active().text_tertiary)
        .child(text.into())
}

fn avatar() -> impl IntoElement {
    let t = theme::active();
    div()
        .size(px(28.))
        .flex_shrink_0()
        .rounded(px(8.))
        .bg(t.surface_raised)
        .border_1()
        .border_color(t.border_hairline)
        .flex()
        .items_center()
        .justify_center()
        .child(
            gpui::svg()
                .path(icon::MARK)
                .size(px(16.))
                .text_color(t.text_primary),
        )
}

/// A compact action. Buttons inside a clickable row stop the click there.
fn action(
    id: impl Into<SharedString>,
    text: impl Into<SharedString>,
    primary: bool,
    enabled: bool,
    cx: &mut Context<WorkspaceRoot>,
    run: impl Fn(&mut WorkspaceRoot, &mut Window, &mut Context<WorkspaceRoot>) + 'static,
) -> impl IntoElement {
    let t = theme::active();
    let run = Rc::new(run);
    let key_run = run.clone();
    div()
        .id(id.into())
        .tab_index(0)
        .tab_stop(enabled)
        .flex_shrink_0()
        .px(px(11.))
        .py(px(6.))
        .rounded(px(8.))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .when(primary, |b| {
            b.bg(t.text_primary).text_color(t.surface_panel)
        })
        .when(!primary, |b| {
            b.border_1()
                .border_color(t.border_hairline_strong)
                .text_color(t.text_primary)
        })
        .when(!enabled, |b| b.opacity(0.5))
        .focus_visible(|s| s.border_1().border_color(t.text_primary))
        .when(enabled, |b| {
            b.cursor_pointer()
                .hover(|s| s.opacity(0.85))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |root, _, window, cx| {
                    cx.stop_propagation();
                    run(root, window, cx)
                }))
                .on_key_down(cx.listener(move |root, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                        key_run(root, window, cx);
                        cx.stop_propagation();
                    }
                }))
        })
        .child(text.into())
}

impl WorkspaceRoot {
    pub(super) fn scoped_tasks(&self) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self
            .snapshot
            .tasks
            .iter()
            .filter(|t| {
                self.selection
                    .workspace
                    .as_ref()
                    .is_none_or(|w| w == &t.workspace_id)
            })
            .collect();
        tasks.sort_by_key(|t| std::cmp::Reverse(t.updated_at_ms));
        tasks
    }

    fn workspace_name(&self, id: &str) -> String {
        self.snapshot
            .workspaces
            .iter()
            .find(|w| w.id == id)
            .map(|w| w.name.clone())
            .unwrap_or_else(|| "Unknown".into())
    }

    fn open_ticket(&mut self, id: String, cx: &mut Context<Self>) {
        self.selection.task = Some(id);
        self.note_input.update(cx, |f, cx| f.clear(cx));
        cx.notify();
    }

    pub(super) fn send_message(&mut self, cx: &mut Context<Self>) {
        let text = value(&self.composer, cx);
        if text.is_empty() {
            return;
        }
        self.request(
            Command::SendMessage {
                text,
                workspace_id: self.selection.workspace.clone(),
            },
            cx,
        );
    }

    fn add_note(&mut self, task_id: String, cx: &mut Context<Self>) {
        let text = value(&self.note_input, cx);
        if text.is_empty() {
            return;
        }
        self.request(Command::AddTicketNote { task_id, text }, cx);
    }

    fn ticket_actions(
        &self,
        task: &Task,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let enabled = !self.busy;
        let id = task.id.clone();
        let mut out = Vec::new();
        match task.status {
            TaskStatus::AwaitingApproval => out.push(
                action(
                    format!("approve-{id}"),
                    if compact {
                        "Approve"
                    } else {
                        "Approve and build"
                    },
                    true,
                    enabled,
                    cx,
                    move |r, _, cx| {
                        r.request(
                            Command::ApproveTask {
                                task_id: id.clone(),
                            },
                            cx,
                        )
                    },
                )
                .into_any_element(),
            ),
            TaskStatus::ReadyForReview => out.push(
                action(
                    format!("complete-{id}"),
                    "Mark reviewed",
                    true,
                    enabled,
                    cx,
                    move |r, _, cx| {
                        r.request(
                            Command::CompleteTask {
                                task_id: id.clone(),
                            },
                            cx,
                        )
                    },
                )
                .into_any_element(),
            ),
            TaskStatus::Failed | TaskStatus::Cancelled => out.push(
                action(
                    format!("retry-{id}"),
                    "Try again",
                    true,
                    enabled,
                    cx,
                    move |r, _, cx| {
                        r.request(
                            Command::RetryTask {
                                task_id: id.clone(),
                            },
                            cx,
                        )
                    },
                )
                .into_any_element(),
            ),
            _ => {}
        }
        // Only statuses the daemon can actually cancel.
        let cancellable = matches!(
            task.status,
            TaskStatus::Queued
                | TaskStatus::Planning
                | TaskStatus::AwaitingApproval
                | TaskStatus::Building
                | TaskStatus::Reviewing
        );
        if !compact && cancellable {
            let id = task.id.clone();
            out.push(
                action(
                    format!("cancel-{id}"),
                    "Cancel",
                    false,
                    enabled,
                    cx,
                    move |r, _, cx| {
                        r.request(
                            Command::CancelTask {
                                task_id: id.clone(),
                            },
                            cx,
                        )
                    },
                )
                .into_any_element(),
            );
        }
        out
    }

    fn ticket_row(
        &self,
        task: &Task,
        show_actions: bool,
        last: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = theme::active();
        let selected = self.selection.task.as_deref() == Some(&task.id);
        let id = task.id.clone();
        let click_id = id.clone();
        let g = group(task.status);
        div()
            .id(SharedString::from(format!("ticket-{}", task.id)))
            .tab_index(0)
            .tab_stop(true)
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(16.))
            .py(px(12.))
            .cursor_pointer()
            .when(!last, |r| r.border_b_1().border_color(t.border_hairline))
            .when(selected, |r| r.bg(t.surface_selected))
            .hover(|s| s.bg(alpha(t.surface_selected, 0.6)))
            .focus_visible(|s| s.border_1().border_color(t.text_primary))
            .on_click(cx.listener(move |root, _, _, cx| root.open_ticket(click_id.clone(), cx)))
            .on_key_down(cx.listener(move |root, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                    root.open_ticket(id.clone(), cx);
                    cx.stop_propagation();
                }
            }))
            .child(dot(group_color(g)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .items_baseline()
                            .min_w(px(0.))
                            .child(
                                div()
                                    .min_w(px(0.))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(task.title.clone()),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(12.))
                                    .text_color(t.text_tertiary)
                                    .child(format!(
                                        "{} · {}",
                                        self.workspace_name(&task.workspace_id),
                                        relative_time(task.updated_at_ms)
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(12.))
                            .text_color(t.text_secondary)
                            .child(one_line(&ticket_line(task), 120)),
                    ),
            )
            .when(show_actions, |r| {
                r.child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .children(self.ticket_actions(task, true, cx)),
                )
            })
            .into_any_element()
    }

    fn card_list(&self, rows: Vec<AnyElement>) -> impl IntoElement {
        let t = theme::active();
        div()
            .flex()
            .flex_col()
            .rounded(px(14.))
            .bg(alpha(t.surface_raised, 0.7))
            .border_1()
            .border_color(t.border_hairline)
            .overflow_hidden()
            .children(rows)
    }

    fn chip(
        &self,
        id: &'static str,
        text: String,
        cx: &mut Context<Self>,
        run: impl Fn(&mut WorkspaceRoot, &mut Context<WorkspaceRoot>) + 'static,
    ) -> impl IntoElement {
        let t = theme::active();
        let run = Rc::new(run);
        let key_run = run.clone();
        div()
            .id(id)
            .tab_index(0)
            .tab_stop(true)
            .px(px(11.))
            .py(px(6.))
            .rounded(px(999.))
            .bg(alpha(t.surface_raised, 0.8))
            .border_1()
            .border_color(t.border_hairline)
            .text_size(px(12.))
            .text_color(t.text_secondary)
            .cursor_pointer()
            .hover(|s| s.text_color(t.text_primary))
            .focus_visible(|s| s.border_1().border_color(t.text_primary))
            .on_click(cx.listener(move |root, _, _, cx| run(root, cx)))
            .on_key_down(cx.listener(move |root, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                    key_run(root, cx);
                    cx.stop_propagation();
                }
            }))
            .child(text)
    }

    // ── Sidebar ─────────────────────────────────────────────────────────

    pub(super) fn home_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::active();
        let needs_you = self
            .snapshot
            .tasks
            .iter()
            .filter(|x| group(x.status) == Group::NeedsYou)
            .count();
        let nav = |id: &'static str,
                   glyph: &'static str,
                   text: &'static str,
                   count: usize,
                   view: View,
                   cx: &mut Context<Self>| {
            let selected = self.view == view;
            div()
                .id(id)
                .tab_index(0)
                .tab_stop(true)
                .flex()
                .items_center()
                .gap(px(10.))
                .px(px(10.))
                .py(px(7.))
                .rounded(px(8.))
                .cursor_pointer()
                .when(selected, |r| r.bg(t.surface_selected))
                .hover(|s| s.bg(alpha(t.surface_selected, 0.6)))
                .focus_visible(|s| s.border_1().border_color(t.text_primary))
                .on_click(cx.listener(move |root, _, _, cx| {
                    if root.view == View::Integrations && view != View::Integrations {
                        root.api_key.update(cx, |field, cx| field.clear(cx));
                    }
                    root.view = view;
                    cx.notify();
                }))
                .on_key_down(cx.listener(move |root, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                        if root.view == View::Integrations && view != View::Integrations {
                            root.api_key.update(cx, |field, cx| field.clear(cx));
                        }
                        root.view = view;
                        cx.notify();
                        cx.stop_propagation();
                    }
                }))
                .child(
                    gpui::svg()
                        .path(glyph)
                        .size(px(15.))
                        .text_color(if selected {
                            t.text_primary
                        } else {
                            t.text_secondary
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .text_size(px(13.))
                        .text_color(if selected {
                            t.text_primary
                        } else {
                            t.text_secondary
                        })
                        .font_weight(if selected {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::NORMAL
                        })
                        .child(text),
                )
                .when(count > 0, |r| {
                    r.child(
                        div()
                            .text_size(px(12.))
                            .text_color(t.text_tertiary)
                            .child(count.to_string()),
                    )
                })
        };
        let mut workspaces = div()
            .id("sidebar-workspaces")
            .flex()
            .flex_col()
            .gap(px(2.))
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll();
        for w in &self.snapshot.workspaces {
            let tasks: Vec<_> = self
                .snapshot
                .tasks
                .iter()
                .filter(|x| x.workspace_id == w.id)
                .collect();
            let waiting = tasks
                .iter()
                .filter(|x| group(x.status) == Group::NeedsYou)
                .count();
            let running = tasks.iter().any(|x| group(x.status) == Group::Working);
            let selected = self.selection.workspace.as_deref() == Some(&w.id);
            let id = w.id.clone();
            let click_id = id.clone();
            let settings_id = w.id.clone();
            workspaces =
                workspaces.child(
                    div()
                        .id(SharedString::from(format!("ws-{}", w.id)))
                        .tab_index(0)
                        .tab_stop(true)
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .px(px(10.))
                        .py(px(7.))
                        .rounded(px(8.))
                        .cursor_pointer()
                        .when(selected, |r| r.bg(t.surface_selected))
                        .hover(|s| s.bg(alpha(t.surface_selected, 0.6)))
                        .focus_visible(|s| s.border_1().border_color(t.text_primary))
                        .on_click(cx.listener(move |root, _, _, cx| {
                            if root.selection.workspace.as_deref() == Some(&click_id) {
                                root.selection = Selection::default();
                                cx.notify();
                            } else {
                                root.select_workspace(&click_id, cx);
                            }
                        }))
                        .on_key_down(cx.listener(move |root, event: &KeyDownEvent, _, cx| {
                            if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                                if root.selection.workspace.as_deref() == Some(&id) {
                                    root.selection = Selection::default();
                                    cx.notify();
                                } else {
                                    root.select_workspace(&id, cx);
                                }
                                cx.stop_propagation();
                            }
                        }))
                        .child(div().w(px(15.)).flex().justify_center().child(dot(
                            if waiting > 0 {
                                attention()
                            } else if running {
                                t.state_success
                            } else {
                                alpha(t.text_tertiary, 0.5)
                            },
                        )))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(13.))
                                .text_color(if selected {
                                    t.text_primary
                                } else {
                                    t.text_secondary
                                })
                                .child(w.name.clone()),
                        )
                        .when(waiting > 0, |r| {
                            r.child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t.text_tertiary)
                                    .child(waiting.to_string()),
                            )
                        })
                        .when(selected, |r| {
                            r.child(
                                div()
                                    .id(SharedString::from(format!("ws-settings-{}", settings_id)))
                                    .text_size(px(12.))
                                    .text_color(t.text_tertiary)
                                    .hover(|s| s.text_color(t.text_primary))
                                    .on_click(cx.listener(|root, _, _, cx| {
                                        cx.stop_propagation();
                                        root.view = View::Workspaces;
                                        cx.notify();
                                    }))
                                    .child("Settings"),
                            )
                        }),
                );
        }
        let online = self.transport_error.is_none() && self.loaded;
        let heartbeat = if !self.loaded {
            "Connecting…".to_owned()
        } else if !online {
            "Not watching".to_owned()
        } else if self.snapshot.heartbeat_ms > 0 {
            format!("Last check {}", relative_time(self.snapshot.heartbeat_ms))
        } else {
            "Ready".to_owned()
        };
        div()
            .w(px(236.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .px(px(12.))
            .pb(px(12.))
            .bg(alpha(
                t.surface_raised,
                if self.translucent { 0.35 } else { 0.9 },
            ))
            .border_r_1()
            .border_color(t.border_hairline)
            .child(self.drag_strip("sidebar-drag", cx).h(px(40.)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(nav("nav-today", icon::MARK, "Today", 0, View::Today, cx))
                    .child(nav(
                        "nav-tickets",
                        icon::CLIPBOARD,
                        "Tickets",
                        needs_you,
                        View::Tickets,
                        cx,
                    ))
                    .child(nav(
                        "nav-agents",
                        icon::MARK,
                        "Agents",
                        self.snapshot.agent_profiles.profiles.len(),
                        View::Profiles,
                        cx,
                    ))
                    .child(nav(
                        "nav-responsibilities",
                        icon::SLIDERS,
                        "Responsibilities",
                        self.snapshot.mcp.responsibilities.len() + self.snapshot.schedules.len(),
                        View::Responsibilities,
                        cx,
                    ))
                    .child(nav(
                        "nav-tools",
                        icon::TERMINAL,
                        "Tools & skills",
                        0,
                        View::Integrations,
                        cx,
                    ))
                    .child(nav(
                        "nav-memory",
                        icon::TEXT_LINES,
                        "Memory",
                        self.snapshot.memory.len(),
                        View::Memory,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .flex_1()
                    .min_h(px(0.))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .px(px(10.))
                            .pb(px(4.))
                            .child(label("WORKSPACES"))
                            .child(
                                div()
                                    .id("sidebar-new-workspace")
                                    .text_size(px(15.))
                                    .text_color(t.text_tertiary)
                                    .cursor_pointer()
                                    .hover(|s| s.text_color(t.text_primary))
                                    .on_click(cx.listener(|root, _, _, cx| root.new_workspace(cx)))
                                    .child("+"),
                            ),
                    )
                    .when(self.snapshot.workspaces.is_empty() && self.loaded, |c| {
                        c.child(
                            div()
                                .px(px(10.))
                                .text_size(px(12.))
                                .text_color(t.text_tertiary)
                                .child("No workspaces yet"),
                        )
                    })
                    .child(workspaces),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .p(px(10.))
                    .rounded(px(10.))
                    .bg(alpha(t.surface_raised, 0.6))
                    .border_1()
                    .border_color(t.border_hairline)
                    .child(avatar())
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(if online { "Watching" } else { "Offline" }),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(t.text_tertiary)
                                    .child(heartbeat),
                            ),
                    )
                    .child(dot(if online { t.state_success } else { attention() })),
            )
    }

    /// A strip the window can be dragged by; double-click zooms, like a title bar.
    pub(super) fn drag_strip(
        &self,
        id: &'static str,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .flex_shrink_0()
            .window_control_area(gpui::WindowControlArea::Drag)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|root, _, _, _| root.drag_armed = true),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|root, _, _, _| root.drag_armed = false),
            )
            .on_mouse_move(
                cx.listener(|root, event: &gpui::MouseMoveEvent, window, _| {
                    if root.drag_armed && event.pressed_button == Some(MouseButton::Left) {
                        root.drag_armed = false;
                        window.start_window_move();
                    }
                }),
            )
            .on_click(|event, window, _| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            })
    }

    pub(super) fn page_header(
        &self,
        title: &str,
        subtitle: Option<String>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme::active();
        self.drag_strip("page-header", cx)
            .h(px(52.))
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(24.))
            .border_b_1()
            .border_color(t.border_hairline)
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(title.to_owned()),
            )
            .when_some(subtitle, |h, s| {
                h.child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.text_tertiary)
                        .child(s),
                )
            })
            .when_some(self.selection.workspace.clone(), |h, id| {
                h.child(div().flex_1()).child(
                    div()
                        .id("clear-workspace-filter")
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .px(px(10.))
                        .py(px(4.))
                        .rounded(px(999.))
                        .bg(alpha(t.surface_raised, 0.8))
                        .text_size(px(12.))
                        .text_color(t.text_secondary)
                        .cursor_pointer()
                        .hover(|s| s.text_color(t.text_primary))
                        .on_click(cx.listener(|root, _, _, cx| {
                            root.selection = Selection::default();
                            cx.notify();
                        }))
                        .child(format!("{} ×", self.workspace_name(&id))),
                )
            })
    }

    fn problem_banner(&self) -> Option<AnyElement> {
        let t = theme::active();
        let message = self
            .transport_error
            .clone()
            .or_else(|| self.error.clone())?;
        let offline = self.transport_error.is_some();
        Some(
            div()
                .mx(px(24.))
                .mt(px(16.))
                .flex()
                .items_center()
                .gap(px(12.))
                .px(px(16.))
                .py(px(12.))
                .rounded(px(12.))
                .bg(alpha(attention(), 0.08))
                .border_1()
                .border_color(alpha(attention(), 0.3))
                .child(dot(attention()))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(if offline {
                                    "Neko stopped watching"
                                } else {
                                    "That didn't work"
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(t.text_secondary)
                                .child(message),
                        ),
                )
                .into_any_element(),
        )
    }

    // ── Today ───────────────────────────────────────────────────────────

    pub(super) fn today_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = theme::active();
        let tasks = self.scoped_tasks();
        let b = brief(&tasks, now_ms());
        let mut column = div()
            .w_full()
            .max_w(px(640.))
            .flex()
            .flex_col()
            .gap(px(22.));

        // Neko's brief.
        let brief_body = if !self.loaded {
            "Catching up on your work…".to_owned()
        } else if self.snapshot.workspaces.is_empty() {
            "Hi, I'm Neko. Add a workspace so I know where your work lives, then tell me what to look after.".to_owned()
        } else {
            brief_text(&b)
        };
        let waiting: Vec<&Task> = tasks
            .iter()
            .copied()
            .filter(|x| group(x.status) == Group::NeedsYou)
            .collect();
        let shown = waiting.len().min(5);
        let mut brief_col = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .items_baseline()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Neko"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(t.text_tertiary)
                            .child(greeting(local_hour())),
                    ),
            )
            .child(
                div()
                    .text_size(px(15.))
                    .line_height(px(24.))
                    .child(brief_body),
            );
        if shown > 0 {
            let rows: Vec<AnyElement> = waiting[..shown]
                .iter()
                .enumerate()
                .map(|(i, task)| self.ticket_row(task, true, i + 1 == shown, cx))
                .collect();
            brief_col = brief_col.child(self.card_list(rows));
        }
        if self.loaded && self.snapshot.workspaces.is_empty() {
            brief_col = brief_col.child(div().flex().child(action(
                "brief-add-workspace",
                "Add a workspace",
                true,
                !self.busy,
                cx,
                |r, _, cx| r.new_workspace(cx),
            )));
        } else if self.loaded {
            let mut chips = div().flex().flex_wrap().gap(px(8.));
            if b.done_today > 0 {
                chips = chips.child(self.chip(
                    "chip-done",
                    format!("✓ {} done today", b.done_today),
                    cx,
                    |r, cx| {
                        r.view = View::Tickets;
                        r.ticket_filter = TicketFilter::Done;
                        cx.notify();
                    },
                ));
            }
            if b.working > 0 {
                chips = chips.child(self.chip(
                    "chip-working",
                    format!("● {} working", b.working),
                    cx,
                    |r, cx| {
                        r.view = View::Tickets;
                        r.ticket_filter = TicketFilter::Working;
                        cx.notify();
                    },
                ));
            }
            if waiting.len() > shown {
                chips = chips.child(self.chip(
                    "chip-more",
                    format!("{} more need you", waiting.len() - shown),
                    cx,
                    |r, cx| {
                        r.view = View::Tickets;
                        r.ticket_filter = TicketFilter::NeedsYou;
                        cx.notify();
                    },
                ));
            }
            if self.snapshot.mcp.responsibilities.is_empty() {
                chips = chips.child(self.chip(
                    "chip-add-responsibility",
                    "Add a responsibility".into(),
                    cx,
                    |r, cx| {
                        r.view = View::Integrations;
                        cx.notify();
                    },
                ));
            } else {
                chips = chips.child(self.chip("chip-all", "All tickets".into(), cx, |r, cx| {
                    r.view = View::Tickets;
                    r.ticket_filter = TicketFilter::All;
                    cx.notify();
                }));
            }
            brief_col = brief_col.child(chips);
        }
        column = column.child(div().flex().gap(px(14.)).child(avatar()).child(brief_col));

        // The conversation.
        let profile_id = self
            .snapshot
            .agent_profiles
            .for_scope(self.selection.workspace.as_deref());
        for message in
            self.snapshot.conversation.iter().filter(|m| {
                conversation_in_scope(m, profile_id, self.selection.workspace.as_deref())
            })
        {
            column = column.child(self.chat_message(message, cx));
        }

        let chat = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .child(
                div()
                    .id("today-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .px(px(40.))
                    .py(px(28.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(column),
            )
            .child(self.composer_bar(cx));

        let side = match self
            .selection
            .task
            .as_ref()
            .and_then(|id| self.snapshot.tasks.iter().find(|x| &x.id == id))
        {
            Some(task) => self.ticket_panel(task, cx).into_any_element(),
            None => self.rail(&tasks, cx).into_any_element(),
        };
        crate::motion::fade_in(
            "today-content-reveal",
            crate::motion::system_reduce_motion(),
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(self.page_header(
                    "Today",
                    Some(format!(
                            "Agent: {}",
                            self.snapshot
                                .agent_profiles
                                .profiles
                                .iter()
                                .find(|p| p.id == profile_id)
                                .map(|p| p.name.as_str())
                                .unwrap_or(profile_id)
                        )),
                    cx,
                ))
                .children(self.problem_banner())
                .child(div().flex_1().min_h(px(0.)).flex().child(chat).child(side)),
        )
    }

    fn chat_message(&self, message: &ChatMessage, cx: &mut Context<Self>) -> AnyElement {
        let t = theme::active();
        if message.role == ChatRole::User {
            return div()
                .flex()
                .justify_end()
                .child(
                    div()
                        .max_w(px(460.))
                        .px(px(14.))
                        .py(px(10.))
                        .rounded(px(14.))
                        .bg(alpha(t.surface_selected, 0.9))
                        .text_size(px(14.))
                        .line_height(px(21.))
                        .child(message.text.clone()),
                )
                .into_any_element();
        }
        let mut body = div().flex_1().min_w(px(0.)).flex().flex_col().gap(px(8.));
        if message.pending {
            body = body.child(
                div()
                    .text_size(px(14.))
                    .text_color(t.text_tertiary)
                    .child("Neko is thinking…"),
            );
        } else {
            body = body.child(
                div()
                    .flex()
                    .gap(px(8.))
                    .items_start()
                    .when(message.failed, |r| {
                        r.child(div().pt(px(8.)).child(dot(attention())))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(14.))
                            .line_height(px(22.))
                            .when(message.failed, |r| r.text_color(t.text_secondary))
                            .child(crate::markdown::render_cached(&message.text)),
                    ),
            );
            let linked: Vec<&Task> = message
                .ticket_ids
                .iter()
                .filter_map(|id| self.snapshot.tasks.iter().find(|x| &x.id == id))
                .collect();
            if !message.remembered.is_empty() {
                let mut saved = div().flex().flex_col().gap(px(4.));
                for text in &message.remembered {
                    saved = saved.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .text_size(px(12.))
                            .text_color(t.text_secondary)
                            .child(
                                gpui::svg()
                                    .path(icon::TEXT_LINES)
                                    .size(px(12.))
                                    .text_color(t.text_tertiary),
                            )
                            .child(format!("Remembered: {text}")),
                    );
                }
                body = body.child(saved);
            }
            if !linked.is_empty() {
                let mut chips = div().flex().flex_wrap().gap(px(8.));
                for task in linked {
                    let id = task.id.clone();
                    chips = chips.child(
                        div()
                            .id(SharedString::from(format!(
                                "chat-ticket-{}-{}",
                                message.id, task.id
                            )))
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .px(px(11.))
                            .py(px(6.))
                            .rounded(px(999.))
                            .bg(alpha(t.surface_raised, 0.8))
                            .border_1()
                            .border_color(t.border_hairline)
                            .text_size(px(12.))
                            .cursor_pointer()
                            .hover(|s| s.bg(t.surface_selected))
                            .on_click(
                                cx.listener(move |root, _, _, cx| root.open_ticket(id.clone(), cx)),
                            )
                            .child(dot(group_color(group(task.status))))
                            .child(one_line(&task.title, 48))
                            .child(
                                div()
                                    .text_color(t.text_tertiary)
                                    .child(self.workspace_name(&task.workspace_id)),
                            ),
                    );
                }
                body = body.child(chips);
            }
        }
        for call in &message.tool_calls {
            use neko_protocol::workbench::ChatToolStatus;
            let status = match call.status {
                ChatToolStatus::AwaitingApproval => "Needs your approval",
                ChatToolStatus::Approved => "Approved",
                ChatToolStatus::Running => "Running",
                ChatToolStatus::Succeeded => "Completed",
                ChatToolStatus::Failed => "Stopped or failed",
                ChatToolStatus::Denied => "Denied",
            };
            let connection = self
                .snapshot
                .mcp
                .connections
                .iter()
                .find(|c| c.id == call.connection_id)
                .map(|c| c.label.as_str())
                .unwrap_or("Removed connection");
            let mut card = div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .p(px(10.))
                .rounded(px(8.))
                .bg(t.surface_raised)
                .child(format!(
                    "{} · {connection} · {} · {status}",
                    self.workspace_name(&call.workspace_id),
                    call.tool_name
                ))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.text_secondary)
                        .child(call.arguments_json.clone()),
                );
            if message.pending && call.status == ChatToolStatus::AwaitingApproval {
                let mut actions = div().flex().gap(px(12.));
                for (approve, label) in [(true, "Approve once"), (false, "Deny")] {
                    let turn_id = message.id.clone();
                    let call_id = call.id.clone();
                    actions = actions.child(button(
                        format!("chat-call-{call_id}-{approve}"),
                        label,
                        !self.busy,
                        approve,
                        cx,
                        move |root, _, cx| {
                            root.request(
                                Command::DecideChatTool {
                                    turn_id: turn_id.clone(),
                                    call_id: call_id.clone(),
                                    approve,
                                },
                                cx,
                            )
                        },
                    ));
                }
                card = card.child(actions);
            }
            body = body.child(card);
        }
        for receipt in self
            .snapshot
            .mcp
            .receipts
            .iter()
            .filter(|r| r.run_id == format!("chat:{}", message.id))
        {
            body = body.child(div().text_size(px(12.)).text_color(t.text_secondary).child(
                format!(
                    "{} · {} · receipt {}",
                    receipt.tool_name,
                    if receipt.success {
                        "Completed"
                    } else {
                        "Failed"
                    },
                    receipt.id
                ),
            ));
        }
        if message.pending {
            let turn_id = message.id.clone();
            body = body.child(button(
                format!("stop-chat-{turn_id}"),
                "Stop",
                !self.busy,
                false,
                cx,
                move |root, _, cx| {
                    root.request(
                        Command::CancelChat {
                            turn_id: turn_id.clone(),
                        },
                        cx,
                    )
                },
            ));
        }
        div()
            .flex()
            .gap(px(14.))
            .child(avatar())
            .child(body)
            .into_any_element()
    }

    fn composer_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::active();
        let thinking = self.snapshot.conversation.iter().any(|m| m.pending);
        let scope = self
            .selection
            .workspace
            .as_ref()
            .map(|id| self.workspace_name(id))
            .or_else(|| {
                self.snapshot
                    .agent_profiles
                    .profiles
                    .iter()
                    .find(|p| p.id == self.snapshot.agent_profiles.active_profile_id)
                    .map(|p| format!("{} · no workspace", p.name))
            });
        div()
            .flex()
            .justify_center()
            .px(px(40.))
            .pb(px(20.))
            .pt(px(4.))
            .child(
                div()
                    .id("composer")
                    .w_full()
                    .max_w(px(640.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .pl(px(16.))
                    .pr(px(8.))
                    .h(px(52.))
                    .rounded(px(14.))
                    .bg(t.surface_input)
                    .border_1()
                    .border_color(t.border_hairline_strong)
                    .on_key_down(cx.listener(|root, event: &KeyDownEvent, _, cx| {
                        if event.keystroke.key == "enter"
                            && !event.keystroke.modifiers.shift
                            && !root.composer.read(cx).is_composing()
                        {
                            root.send_message(cx);
                            cx.stop_propagation();
                        }
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .child(self.composer.clone()),
                    )
                    .when_some(scope, |c, name| {
                        c.child(
                            div()
                                .flex_shrink_0()
                                .px(px(9.))
                                .py(px(4.))
                                .rounded(px(999.))
                                .bg(alpha(t.surface_raised, 0.9))
                                .text_size(px(12.))
                                .text_color(t.text_secondary)
                                .child(name),
                        )
                    })
                    .child(
                        div()
                            .id("composer-send")
                            .size(px(32.))
                            .flex_shrink_0()
                            .rounded(px(16.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(if thinking {
                                alpha(t.text_primary, 0.3)
                            } else {
                                t.text_primary
                            })
                            .text_color(t.surface_panel)
                            .font_weight(FontWeight::BOLD)
                            .cursor_pointer()
                            .on_click(cx.listener(|root, _, _, cx| root.send_message(cx)))
                            .child("↑"),
                    ),
            )
    }

    fn rail(&self, tasks: &[&Task], cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::active();
        let working: Vec<&Task> = tasks
            .iter()
            .copied()
            .filter(|x| group(x.status) == Group::Working)
            .collect();
        let mut work_rows: Vec<AnyElement> = Vec::new();
        let n = working.len();
        for (i, task) in working.iter().enumerate() {
            let id = task.id.clone();
            work_rows.push(
                div()
                    .id(SharedString::from(format!("rail-{}", task.id)))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .px(px(14.))
                    .py(px(12.))
                    .cursor_pointer()
                    .hover(|s| s.bg(alpha(t.surface_selected, 0.6)))
                    .when(i + 1 < n, |r| {
                        r.border_b_1().border_color(t.border_hairline)
                    })
                    .on_click(cx.listener(move |root, _, _, cx| root.open_ticket(id.clone(), cx)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .size(px(10.))
                                    .rounded(px(5.))
                                    .border_2()
                                    .border_color(t.text_primary)
                                    .flex_shrink_0(),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(task.title.clone()),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(11.))
                                    .text_color(t.text_tertiary)
                                    .child(self.workspace_name(&task.workspace_id)),
                            ),
                    )
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(12.))
                            .text_color(t.text_secondary)
                            .child(one_line(last_event(task).unwrap_or(&ticket_line(task)), 60)),
                    )
                    .into_any_element(),
            );
        }
        let responsibilities: Vec<&Responsibility> = self
            .snapshot
            .mcp
            .responsibilities
            .iter()
            .filter(|r| {
                self.selection
                    .workspace
                    .as_ref()
                    .is_none_or(|w| w == &r.workspace_id)
            })
            .collect();
        let now = now_ms();
        let mut resp_rows: Vec<AnyElement> = Vec::new();
        let m = responsibilities.len();
        for (i, r) in responsibilities.iter().enumerate() {
            let color = if !r.enabled {
                alpha(t.text_tertiary, 0.5)
            } else if r.failures > 0 {
                attention()
            } else {
                t.state_success
            };
            let status = if !r.enabled {
                "Paused".to_owned()
            } else if r.failures > 0 {
                "Needs attention".to_owned()
            } else {
                format!("Next check {}", until(r.next_due_ms, now))
            };
            resp_rows.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(14.))
                    .py(px(11.))
                    .when(i + 1 < m, |row| {
                        row.border_b_1().border_color(t.border_hairline)
                    })
                    .child(dot(color))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(13.))
                                    .child(one_line(&r.instruction, 60)),
                            )
                            .child(div().text_size(px(11.)).text_color(t.text_tertiary).child(
                                format!("{} · {}", self.workspace_name(&r.workspace_id), status),
                            )),
                    )
                    .into_any_element(),
            );
        }
        div()
            .id("rail")
            .w(px(320.))
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(22.))
            .px(px(20.))
            .py(px(24.))
            .border_l_1()
            .border_color(t.border_hairline)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .child(label("WORKING NOW"))
                            .child(label(plural(n, "agent", "agents"))),
                    )
                    .child(if work_rows.is_empty() {
                        div()
                            .text_size(px(12.))
                            .text_color(t.text_tertiary)
                            .child("Nothing running right now.")
                            .into_any_element()
                    } else {
                        self.card_list(work_rows).into_any_element()
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(label("RESPONSIBILITIES"))
                    .child(if resp_rows.is_empty() {
                        div()
                            .text_size(px(12.))
                            .text_color(t.text_tertiary)
                            .child("None yet. Add one in Tools & skills.")
                            .into_any_element()
                    } else {
                        self.card_list(resp_rows).into_any_element()
                    }),
            )
    }

    // ── One ticket ──────────────────────────────────────────────────────

    pub(super) fn ticket_panel(&self, task: &Task, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::active();
        let g = group(task.status);
        let heading = match g {
            Group::NeedsYou => "Needs you",
            Group::Working => "Working",
            Group::Done => "Done",
        };
        let mut body = div()
            .id(SharedString::from(format!("ticket-panel-{}", task.id)))
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(18.))
            .px(px(20.))
            .py(px(20.))
            .child(
                div()
                    .text_size(px(20.))
                    .line_height(px(26.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(task.title.clone()),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .line_height(px(20.))
                    .text_color(t.text_secondary)
                    .child(one_line(&ticket_line(task), 200)),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .children(self.ticket_actions(task, false, cx)),
            );
        if let Some(decision) = &task.supervision {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(label("NEKO'S ASSESSMENT"))
                    .child(
                        div()
                            .text_size(px(13.))
                            .line_height(px(20.))
                            .child(format!("Risk: {:?}. {}", decision.risk, decision.reason)),
                    ),
            );
        }
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(label("GOAL"))
                .child(
                    div()
                        .text_size(px(13.))
                        .line_height(px(20.))
                        .child(crate::markdown::render_cached(&task.goal)),
                ),
        );
        if !task.plan.is_empty() {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(label("PLAN"))
                    .child(
                        div()
                            .text_size(px(13.))
                            .line_height(px(20.))
                            .child(crate::markdown::render_cached(&task.plan)),
                    ),
            );
        }
        if task.status == TaskStatus::AwaitingApproval
            && !self.snapshot.splits.iter().any(|s| {
                s.parent_id == task.id
                    || s.subtasks
                        .iter()
                        .any(|p| p.task_id.as_deref() == Some(&task.id))
            })
        {
            let id = task.id.clone();
            body = body.child(action(
                "propose-split",
                "Propose parallel subtasks",
                false,
                true,
                cx,
                move |r, _, cx| {
                    r.request(
                        Command::ProposeSplit {
                            task_id: id.clone(),
                        },
                        cx,
                    )
                },
            ));
        }
        if let Some(split) = self.snapshot.splits.iter().find(|s| s.parent_id == task.id) {
            let mut children =
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(label(if split.approved {
                        "APPROVED SUBTASKS"
                    } else {
                        "SUBTASK PROPOSAL · APPROVAL REQUIRED"
                    }));
            for plan in &split.subtasks {
                let status = plan
                    .task_id
                    .as_ref()
                    .and_then(|id| self.snapshot.tasks.iter().find(|t| &t.id == id))
                    .map(|t| format!("{:?}", t.status))
                    .unwrap_or_else(|| "Awaiting approval".into());
                children = children.child(div().text_size(px(13.)).child(format!(
                    "{} · {}\nFiles: {}",
                    plan.title,
                    status,
                    plan.files.join(", ")
                )));
            }
            body = body.child(children);
        }
        if !task.result.is_empty() {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(label("RESULT"))
                    .child(
                        div()
                            .text_size(px(13.))
                            .line_height(px(20.))
                            .child(crate::markdown::render_cached(&task.result)),
                    ),
            );
        }
        if let Some(path) = &task.worktree {
            let reveal = std::path::PathBuf::from(path);
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis_middle()
                            .text_size(px(12.))
                            .text_color(t.text_tertiary)
                            .child(path.clone()),
                    )
                    .child(action(
                        "reveal-worktree",
                        "Show in Finder",
                        false,
                        true,
                        cx,
                        move |_, _, cx| cx.reveal_path(&reveal),
                    )),
            );
        }
        let steps: Vec<_> = task
            .events
            .iter()
            .filter(|e| e.role != NOTE_ROLE)
            .rev()
            .take(8)
            .collect();
        if !steps.is_empty() {
            let mut list = div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(label("ACTIVITY"));
            for event in steps.into_iter().rev() {
                list = list.child(
                    div()
                        .flex()
                        .gap(px(10.))
                        .child(div().pt(px(6.)).child(dot(alpha(t.text_tertiary, 0.6))))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .line_height(px(19.))
                                        .child(one_line(&event.message, 160)),
                                )
                                .child(div().text_size(px(11.)).text_color(t.text_tertiary).child(
                                    format!("{} · {}", event.role, relative_time(event.at_ms)),
                                )),
                        ),
                );
            }
            body = body.child(list);
        }

        let notes: Vec<_> = task.events.iter().filter(|e| e.role == NOTE_ROLE).collect();
        let task_id = task.id.clone();
        let key_task_id = task.id.clone();
        let mut thread = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .px(px(20.))
            .pt(px(14.))
            .pb(px(18.))
            .border_t_1()
            .border_color(t.border_hairline)
            .child(label("ON THIS TICKET"));
        // Where a note will actually be read, say so; where it can't, don't offer it.
        let hint = match task.status {
            TaskStatus::Queued | TaskStatus::Planning => {
                Some("Notes are read when this ticket is planned.")
            }
            TaskStatus::AwaitingApproval => Some("Notes are read when you approve and it builds."),
            TaskStatus::Failed | TaskStatus::Cancelled => {
                Some("Notes are read when you try again.")
            }
            TaskStatus::Building | TaskStatus::Reviewing | TaskStatus::ReadyForReview => {
                Some("This run already started. Notes apply if you try again later.")
            }
            TaskStatus::Completed => None,
        };
        if let Some(hint) = hint {
            thread = thread.child(
                div()
                    .text_size(px(12.))
                    .text_color(t.text_tertiary)
                    .child(hint),
            );
        }
        for n in notes.iter().rev().take(4).rev() {
            thread = thread.child(
                div()
                    .flex()
                    .gap(px(10.))
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .child(
                        div()
                            .w(px(32.))
                            .flex_shrink_0()
                            .text_color(t.text_tertiary)
                            .child("You"),
                    )
                    .child(div().flex_1().min_w(px(0.)).child(n.message.clone())),
            );
        }
        let accepts_notes = task.status != TaskStatus::Completed;
        thread = thread.when(accepts_notes, |thread| {
            thread.child(
                div()
                    .id("ticket-note")
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .pl(px(12.))
                    .pr(px(6.))
                    .h(px(40.))
                    .rounded(px(10.))
                    .bg(t.surface_input)
                    .border_1()
                    .border_color(t.border_hairline_strong)
                    .on_key_down(cx.listener(move |root, event: &KeyDownEvent, _, cx| {
                        if event.keystroke.key == "enter"
                            && !event.keystroke.modifiers.shift
                            && !root.note_input.read(cx).is_composing()
                        {
                            root.add_note(key_task_id.clone(), cx);
                            cx.stop_propagation();
                        }
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .child(self.note_input.clone()),
                    )
                    .child(action(
                        "ticket-note-send",
                        "Add",
                        false,
                        !self.busy,
                        cx,
                        move |r, _, cx| r.add_note(task_id.clone(), cx),
                    )),
            )
        });

        div()
            .w(px(440.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(alpha(t.surface_raised, 0.35))
            .border_l_1()
            .border_color(t.border_hairline)
            .child(
                div()
                    .h(px(46.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(20.))
                    .border_b_1()
                    .border_color(t.border_hairline)
                    .child(dot(group_color(g)))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .text_color(t.text_secondary)
                            .child(format!(
                                "{heading} · {} · {}",
                                self.workspace_name(&task.workspace_id),
                                status_label(task.status)
                            )),
                    )
                    .child(
                        div()
                            .id("ticket-close")
                            .px(px(6.))
                            .text_size(px(14.))
                            .text_color(t.text_tertiary)
                            .cursor_pointer()
                            .hover(|s| s.text_color(t.text_primary))
                            .on_click(cx.listener(|root, _, _, cx| {
                                root.selection.task = None;
                                cx.notify();
                            }))
                            .child("×"),
                    ),
            )
            .child(body)
            .child(thread)
    }

    // ── Tickets ─────────────────────────────────────────────────────────

    pub(super) fn tickets_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = theme::active();
        let tasks = self.scoped_tasks();
        let count = |f: TicketFilter| tasks.iter().filter(|x| matches_filter(x.status, f)).count();
        let mut tabs = div().flex().gap(px(4.));
        for (f, name) in [
            (TicketFilter::NeedsYou, "Needs you"),
            (TicketFilter::Working, "Working"),
            (TicketFilter::Done, "Done"),
            (TicketFilter::All, "All"),
        ] {
            let selected = self.ticket_filter == f;
            tabs = tabs.child(
                div()
                    .id(SharedString::from(format!("filter-{name}")))
                    .flex()
                    .gap(px(6.))
                    .px(px(10.))
                    .py(px(5.))
                    .rounded(px(7.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .when(selected, |r| {
                        r.bg(t.surface_selected).text_color(t.text_primary)
                    })
                    .when(!selected, |r| {
                        r.text_color(t.text_secondary)
                            .hover(|s| s.text_color(t.text_primary))
                    })
                    .on_click(cx.listener(move |root, _, _, cx| {
                        root.ticket_filter = f;
                        cx.notify();
                    }))
                    .child(name)
                    .child(
                        div()
                            .text_color(t.text_tertiary)
                            .child(count(f).to_string()),
                    ),
            );
        }
        let shown: Vec<&Task> = tasks
            .iter()
            .copied()
            .filter(|x| matches_filter(x.status, self.ticket_filter))
            .collect();
        let n = shown.len();
        let rows: Vec<AnyElement> = shown
            .iter()
            .enumerate()
            .map(|(i, task)| self.ticket_row(task, false, i + 1 == n, cx))
            .collect();
        let empty = match self.ticket_filter {
            TicketFilter::NeedsYou => "Nothing needs you. Neko will add tickets here when it does.",
            TicketFilter::Working => "Nothing is running right now.",
            TicketFilter::Done => "Nothing finished yet.",
            TicketFilter::All => "No tickets yet. Ask Neko for something on Today.",
        };
        let list = div()
            .id("tickets-list")
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(14.))
            .px(px(24.))
            .py(px(20.))
            .child(tabs)
            .child(if rows.is_empty() {
                div()
                    .pt(px(24.))
                    .text_size(px(13.))
                    .text_color(t.text_tertiary)
                    .child(empty)
                    .into_any_element()
            } else {
                self.card_list(rows).into_any_element()
            });
        let side = self
            .selection
            .task
            .as_ref()
            .and_then(|id| self.snapshot.tasks.iter().find(|x| &x.id == id))
            .map(|task| self.ticket_panel(task, cx).into_any_element());
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.page_header("Tickets", None, cx))
            .children(self.problem_banner())
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .child(list)
                    .children(side),
            )
            .into_any_element()
    }

    // ── Responsibilities ────────────────────────────────────────────────

    pub(super) fn responsibilities_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = theme::active();
        let now = now_ms();
        let list: Vec<&Responsibility> = self
            .snapshot
            .mcp
            .responsibilities
            .iter()
            .filter(|r| {
                self.selection
                    .workspace
                    .as_ref()
                    .is_none_or(|w| w == &r.workspace_id)
            })
            .collect();
        let n = list.len();
        let mut rows: Vec<AnyElement> = Vec::new();
        for (i, r) in list.iter().enumerate() {
            let color = if !r.enabled {
                alpha(t.text_tertiary, 0.5)
            } else if r.failures > 0 {
                attention()
            } else {
                t.state_success
            };
            let schedule = if r.enabled {
                format!("Next check {}", until(r.next_due_ms, now))
            } else {
                "Paused".into()
            };
            let last = if r.last_result.is_empty() {
                "Hasn't run yet".to_owned()
            } else {
                one_line(&r.last_result, 110)
            };
            let wake_id = r.id.clone();
            let mut toggled = (*r).clone();
            toggled.enabled = !r.enabled;
            rows.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(16.))
                    .py(px(14.))
                    .when(i + 1 < n, |row| {
                        row.border_b_1().border_color(t.border_hairline)
                    })
                    .child(dot(color))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(one_line(&r.instruction, 120)),
                            )
                            .child(div().text_size(px(12.)).text_color(t.text_tertiary).child(
                                format!(
                                    "{} · {} · {}",
                                    self.workspace_name(&r.workspace_id),
                                    schedule,
                                    if r.prepare_low_risk {
                                        "Prepares low-risk fixes"
                                    } else {
                                        "Tells you"
                                    }
                                ),
                            ))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t.text_secondary)
                                    .child(last),
                            ),
                    )
                    .child(action(
                        format!("wake-{}", r.id),
                        "Check now",
                        false,
                        !self.busy && r.enabled,
                        cx,
                        move |root, _, cx| {
                            root.request(
                                Command::Mcp(McpCommand::Wake {
                                    responsibility_id: wake_id.clone(),
                                }),
                                cx,
                            )
                        },
                    ))
                    .child(action(
                        format!("toggle-{}", r.id),
                        if r.enabled { "Pause" } else { "Resume" },
                        false,
                        !self.busy,
                        cx,
                        move |root, _, cx| {
                            root.request(
                                Command::Mcp(McpCommand::SaveResponsibility {
                                    responsibility: toggled.clone(),
                                }),
                                cx,
                            )
                        },
                    ))
                    .into_any_element(),
            );
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.page_header("Responsibilities", Some("Standing jobs Neko checks on a schedule".into()), cx))
            .children(self.problem_banner())
            .child(
                div()
                    .id("responsibilities-list")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .px(px(24.))
                    .py(px(20.))
                    .child(if rows.is_empty() {
                        div().text_size(px(13.)).text_color(t.text_tertiary).child("No responsibilities yet. Add one in Tools & skills: pick a workspace, connect a tool, then describe what Neko should watch.").into_any_element()
                    } else {
                        self.card_list(rows).into_any_element()
                    })
                    .child(div().flex().child(action("edit-responsibilities", "Add or edit in Tools & skills", false, true, cx, |root, _, cx| {
                        root.view = View::Integrations;
                        cx.notify();
                    })))
                    .child(super::schedules::view(self,cx)),
            )
            .into_any_element()
    }
}

impl WorkspaceRoot {
    fn save_memory(&mut self, kind: MemoryKind, cx: &mut Context<Self>) {
        let text = value(&self.memory_input, cx);
        if text.is_empty() {
            return;
        }
        if let Some(entry) = edited_memory(self.memory_editing.as_ref(), &text) {
            self.request(Command::SaveMemory { entry }, cx);
            return;
        }
        let workspace_id = if kind == MemoryKind::Workspace {
            self.selection.workspace.clone()
        } else {
            None
        };
        self.request(
            Command::SaveMemory {
                entry: MemoryEntry {
                    id: String::new(),
                    kind,
                    agent_profile_id: self
                        .snapshot
                        .agent_profiles
                        .for_scope(self.selection.workspace.as_deref())
                        .to_owned(),
                    workspace_id,
                    text,
                    source: "user".into(),
                    created_at_ms: 0,
                    updated_at_ms: 0,
                },
            },
            cx,
        );
    }

    fn memory_rows(&self, entries: Vec<&MemoryEntry>, cx: &mut Context<Self>) -> AnyElement {
        let t = theme::active();
        let n = entries.len();
        let rows: Vec<AnyElement> = entries
            .into_iter()
            .enumerate()
            .map(|(i, e)| {
                let id = e.id.clone();
                let edit = e.clone();
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(16.))
                    .py(px(11.))
                    .when(i + 1 < n, |r| {
                        r.border_b_1().border_color(t.border_hairline)
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(13.))
                            .line_height(px(19.))
                            .child(e.text.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.))
                            .text_color(t.text_tertiary)
                            .child(if e.source == "chat" {
                                "from chat"
                            } else {
                                "added by you"
                            }),
                    )
                    .child(action(
                        format!("edit-memory-{}", e.id),
                        "Edit",
                        false,
                        !self.busy,
                        cx,
                        move |root, _, cx| {
                            root.memory_editing = Some(edit.clone());
                            root.memory_input
                                .update(cx, |f, cx| f.set_content(&edit.text, cx));
                            cx.notify();
                        },
                    ))
                    .child(action(
                        format!("forget-{}", e.id),
                        "Forget",
                        false,
                        !self.busy,
                        cx,
                        move |root, _, cx| {
                            root.request(Command::DeleteMemory { id: id.clone() }, cx)
                        },
                    ))
                    .into_any_element()
            })
            .collect();
        self.card_list(rows).into_any_element()
    }

    pub(super) fn memory_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = theme::active();
        let memory = &self.snapshot.memory;
        let profile_id = self
            .snapshot
            .agent_profiles
            .for_scope(self.selection.workspace.as_deref());
        let mut sorted: Vec<&MemoryEntry> = memory
            .iter()
            .filter(|m| m.agent_profile_id == profile_id)
            .collect();
        sorted.sort_by_key(|e| std::cmp::Reverse(e.updated_at_ms));
        let profile: Vec<&MemoryEntry> = sorted
            .iter()
            .copied()
            .filter(|e| e.workspace_id.is_none() && e.kind != MemoryKind::Decision)
            .collect();
        let decisions: Vec<&MemoryEntry> = sorted
            .iter()
            .copied()
            .filter(|e| e.kind == MemoryKind::Decision)
            .collect();
        let selected_name = self
            .selection
            .workspace
            .as_ref()
            .map(|id| self.workspace_name(id));
        let mut list = div()
            .id("memory-list")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(22.))
            .px(px(24.))
            .py(px(20.))
            .child(div().text_size(px(13.)).line_height(px(20.)).text_color(t.text_secondary).child("These memories belong to the selected workspace’s agent, or the active agent when no workspace is selected. Memory shapes chat and plans; it never grants permissions. Manage one-way sharing in Agent profiles."))
            .child(
                div()
                    .id("memory-add")
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .pl(px(14.))
                    .pr(px(6.))
                    .h(px(44.))
                    .rounded(px(12.))
                    .bg(t.surface_input)
                    .border_1()
                    .border_color(t.border_hairline_strong)
                    .on_key_down(cx.listener(|root, event: &KeyDownEvent, _, cx| {
                        if event.keystroke.key == "enter" && !event.keystroke.modifiers.shift && !root.memory_input.read(cx).is_composing() {
                            root.save_memory(MemoryKind::Profile, cx);
                            cx.stop_propagation();
                        }
                    }))
                    .child(div().flex_1().min_w(px(0.)).overflow_hidden().child(self.memory_input.clone()))
                    .child(action("memory-add-me", if self.memory_editing.is_some(){"Save edit"}else{"About me"}, false, !self.busy, cx, |root, _, cx| root.save_memory(MemoryKind::Profile, cx)))
                    .when(self.memory_editing.is_some(),|row|row.child(action("memory-cancel-edit","Cancel edit",false,!self.busy,cx,|root,_,cx|{root.memory_editing=None;root.memory_input.update(cx,|f,cx|f.clear(cx));cx.notify();})))
                    .when_some(selected_name.filter(|_|self.memory_editing.is_none()), |row, name| row.child(action("memory-add-ws", format!("For {name}"), false, !self.busy, cx, |root, _, cx| root.save_memory(MemoryKind::Workspace, cx)))),
            );
        let proposals: Vec<_> = self
            .snapshot
            .memory_proposals
            .iter()
            .filter(|p| p.agent_profile_id == profile_id)
            .collect();
        if !proposals.is_empty() {
            let mut review = div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(label("PROPOSED MEMORIES · NOT ACTIVE YET"));
            for proposal in proposals {
                let accept = proposal.id.clone();
                let reject = proposal.id.clone();
                let scope = proposal
                    .workspace_id
                    .as_ref()
                    .map(|id| self.workspace_name(id))
                    .unwrap_or_else(|| "Profile-wide".into());
                review = review.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .p(px(14.))
                        .rounded(px(10.))
                        .border_1()
                        .border_color(t.border_hairline)
                        .child(div().text_size(px(13.)).child(proposal.text.clone()))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(t.text_secondary)
                                .child(format!("{scope} · {}", proposal.source)),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(8.))
                                .child(action(
                                    format!("accept-memory-{accept}"),
                                    "Remember this",
                                    true,
                                    !self.busy,
                                    cx,
                                    move |root, _, cx| {
                                        root.request(
                                            Command::DecideMemoryProposal {
                                                id: accept.clone(),
                                                accept: true,
                                            },
                                            cx,
                                        )
                                    },
                                ))
                                .child(action(
                                    format!("reject-memory-{reject}"),
                                    "Dismiss",
                                    false,
                                    !self.busy,
                                    cx,
                                    move |root, _, cx| {
                                        root.request(
                                            Command::DecideMemoryProposal {
                                                id: reject.clone(),
                                                accept: false,
                                            },
                                            cx,
                                        )
                                    },
                                )),
                        ),
                );
            }
            list = list.child(review);
        }
        if let Some(entry) = &self.memory_editing {
            let owner = self
                .snapshot
                .agent_profiles
                .profiles
                .iter()
                .find(|p| p.id == entry.agent_profile_id)
                .map(|p| p.name.as_str())
                .unwrap_or(&entry.agent_profile_id);
            list=list.child(div().text_size(px(12.)).text_color(t.text_secondary).child(format!("Editing a memory belonging to {owner}. Its original profile and workspace scope are preserved.")));
        }
        let empty = div()
            .text_size(px(12.))
            .text_color(t.text_tertiary)
            .child("Nothing yet. Tell Neko how you like to work, in chat or above.")
            .into_any_element();
        let about = if profile.is_empty() {
            empty
        } else {
            self.memory_rows(profile, cx)
        };
        list = list.child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(label("ABOUT YOU"))
                .child(about),
        );
        for workspace in &self.snapshot.workspaces {
            let notes: Vec<&MemoryEntry> = sorted
                .iter()
                .copied()
                .filter(|e| {
                    e.workspace_id.as_deref() == Some(workspace.id.as_str())
                        && e.kind != MemoryKind::Decision
                })
                .collect();
            if notes.is_empty() {
                continue;
            }
            let rows = self.memory_rows(notes, cx);
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(label(workspace.name.to_uppercase()))
                    .child(rows),
            );
        }
        if !decisions.is_empty() {
            let rows = self.memory_rows(decisions, cx);
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(label("DECISIONS"))
                    .child(rows),
            );
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.page_header(
                "Memory",
                Some(format!(
                        "Agent: {}",
                        self.snapshot
                            .agent_profiles
                            .profiles
                            .iter()
                            .find(|p| p.id == profile_id)
                            .map(|p| p.name.as_str())
                            .unwrap_or(profile_id)
                    )),
                cx,
            ))
            .children(self.problem_banner())
            .child(list)
            .into_any_element()
    }
}

fn conversation_in_scope(message: &ChatMessage, profile: &str, workspace: Option<&str>) -> bool {
    message.agent_profile_id == profile && message.workspace_id.as_deref() == workspace
}

fn edited_memory(existing: Option<&MemoryEntry>, text: &str) -> Option<MemoryEntry> {
    existing.map(|entry| MemoryEntry {
        text: text.to_owned(),
        ..entry.clone()
    })
}

pub(super) fn matches_filter(status: TaskStatus, filter: TicketFilter) -> bool {
    match filter {
        TicketFilter::All => true,
        TicketFilter::NeedsYou => group(status) == Group::NeedsYou,
        TicketFilter::Working => group(status) == Group::Working,
        TicketFilter::Done => group(status) == Group::Done,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    fn pending_memory_edit_cannot_intercept_chat_send(cx: &mut gpui::TestAppContext) {
        let (client, _events) = neko_client::NekoClient::connect(std::env::temp_dir().join(
            format!("neko-no-socket-{}-memory-routing", std::process::id()),
        ));
        let root = cx.update(|cx| cx.new(|cx| WorkspaceRoot::new(client, cx)));
        root.update(cx,|root,cx| {
            root.busy=true;root.refreshing=true;
            let original=MemoryEntry{agent_profile_id:"work".into(),id:"memory-1".into(),kind:MemoryKind::Workspace,workspace_id:Some("a".into()),text:"before".into(),source:"chat".into(),created_at_ms:10,updated_at_ms:20};
            root.memory_editing=Some(original.clone());
            root.selection.workspace=Some("different".into());
            root.composer.update(cx,|f,cx|f.set_content("hello",cx));
            root.send_message(cx);
            assert!(matches!(root.queued.pop_front(),Some(Command::SendMessage{text,..}) if text=="hello"));
            root.memory_input.update(cx,|f,cx|f.set_content("edited",cx));
            root.save_memory(MemoryKind::Profile,cx);
            let Some(Command::SaveMemory{entry})=root.queued.pop_front() else {panic!("Expected memory edit");};
            assert_eq!(entry,MemoryEntry{text:"edited".into(),..original});
        });
    }
    #[test]
    fn today_history_is_scoped_to_agent_and_workspace() {
        let message:ChatMessage=serde_json::from_value(serde_json::json!({"agent_profile_id":"personal","workspace_id":"a","id":"m","at_ms":0,"role":"neko","text":"hello"})).unwrap();
        assert!(conversation_in_scope(&message, "personal", Some("a")));
        assert!(!conversation_in_scope(&message, "work", Some("a")));
        assert!(!conversation_in_scope(&message, "personal", Some("b")));
        assert!(!conversation_in_scope(&message, "personal", None));
    }
    #[test]
    fn memory_edit_changes_only_text_and_never_creates_a_new_entry() {
        let old = MemoryEntry {
            agent_profile_id: "work".into(),
            id: "memory-1".into(),
            kind: MemoryKind::Workspace,
            workspace_id: Some("a".into()),
            text: "before".into(),
            source: "chat".into(),
            created_at_ms: 10,
            updated_at_ms: 20,
        };
        let updated = edited_memory(Some(&old), "after").unwrap();
        assert_eq!(
            updated,
            MemoryEntry {
                text: "after".into(),
                ..old
            }
        );
        assert!(edited_memory(None, "new").is_none());
    }

    fn task(status: TaskStatus, updated_at_ms: i64) -> Task {
        Task {
            id: "t".into(),
            workspace_id: "w".into(),
            issue_id: None,
            title: "T".into(),
            goal: "G".into(),
            status,
            plan: String::new(),
            result: String::new(),
            worktree: None,
            events: vec![],
            created_at_ms: 0,
            updated_at_ms,
            source_revision: None,
            supervision: None,
        }
    }

    #[test]
    fn every_status_lands_in_exactly_the_designed_group() {
        use TaskStatus::*;
        for (s, g) in [
            (AwaitingApproval, Group::NeedsYou),
            (ReadyForReview, Group::NeedsYou),
            (Failed, Group::NeedsYou),
            (Queued, Group::Working),
            (Planning, Group::Working),
            (Building, Group::Working),
            (Reviewing, Group::Working),
            (Completed, Group::Done),
            (Cancelled, Group::Done),
        ] {
            assert_eq!(group(s), g, "{s:?}");
        }
    }

    #[test]
    fn brief_counts_only_today_s_completions() {
        let now = 10 * DAY_MS;
        let tasks = [
            task(TaskStatus::Failed, now),
            task(TaskStatus::Building, now),
            task(TaskStatus::Completed, now - 1000),
            task(TaskStatus::Completed, now - 2 * DAY_MS),
            task(TaskStatus::Cancelled, now),
        ];
        let refs: Vec<&Task> = tasks.iter().collect();
        assert_eq!(
            brief(&refs, now),
            Brief {
                needs_you: 1,
                working: 1,
                done_today: 1
            }
        );
    }

    #[test]
    fn brief_text_reads_naturally() {
        assert!(brief_text(&Brief::default()).starts_with("All quiet."));
        assert_eq!(
            brief_text(&Brief {
                needs_you: 1,
                working: 0,
                done_today: 0
            }),
            "1 ticket needs you. Here's what's waiting:"
        );
        assert_eq!(
            brief_text(&Brief {
                needs_you: 2,
                working: 3,
                done_today: 4
            }),
            "2 tickets need you, 3 are in progress and I finished 4 today. Here's what's waiting:"
        );
        assert_eq!(
            brief_text(&Brief {
                needs_you: 0,
                working: 1,
                done_today: 0
            }),
            "1 is in progress."
        );
    }

    #[test]
    fn filters_follow_groups() {
        assert!(matches_filter(TaskStatus::Failed, TicketFilter::NeedsYou));
        assert!(!matches_filter(TaskStatus::Failed, TicketFilter::Working));
        assert!(matches_filter(TaskStatus::Cancelled, TicketFilter::All));
    }

    #[test]
    fn greeting_and_countdown() {
        assert_eq!(greeting(8), "Good morning");
        assert_eq!(greeting(14), "Good afternoon");
        assert_eq!(greeting(22), "Good evening");
        assert_eq!(until(0, 10), "any moment");
        assert_eq!(until(10 * 60_000, 0), "in 10 min");
        assert_eq!(until(3 * 60 * 60_000, 0), "in 3 h");
    }

    #[test]
    fn one_line_takes_the_first_line_and_shortens() {
        assert_eq!(one_line("\n  first\nsecond", 10), "first");
        assert_eq!(one_line("abcdefghijkl", 5), "abcde…");
    }
}
